//==============================================================================
// 스프라이트 애니메이터 컴포넌트.
// - 원본 misc/spriteanimator.js 를 옮긴 것이다. 이름을 붙인 클립(프레임 시퀀스)을 등록해 두고 play("run") 으로 바꾼다.
// - 같은 클립을 다시 play 해도 처음부터 하지 않는다. (restart_if_same 으로 바꿀 수 있다)
// - 클립이 끝나면 on_complete 콜백으로 다음 상태를 이어 간다. (FSM 과 묶어 쓴다)
// - 같은 노드의 Sprite 컴포넌트에 지금 프레임(이미지 + 영역)을 넣는다. tick 때 context.get_node_mut().get_component_mut::<Sprite>() 로 찾는다.
// - 사용:
//     let animator = node_tree[hero_id].add_component(SpriteAnimator::new());
//     animator.add_clip("idle", image.clone(), &idleRects, 10.0, true);
//     animator.add_clip("attack", image.clone(), &attackRects, 12.0, false);
//     SpriteAnimator::play_on_node(&mut node_tree[hero_id], "attack", SpriteAnimatorPlayOptions::create_with_on_complete(|animator| {
//         animator.play("idle", SpriteAnimatorPlayOptions::default());
//     }));
// - 원본과 다른 점:
//   play 는 원본처럼 그 자리에서 Sprite 에 프레임을 넣지 못한다. 러스트 컴포넌트는 자기 노드를 가리키지 않고(노드는 NodeTree 가 들고,
//     컴포넌트에게 노드를 넘겨주는 것은 tick 의 ComponentContext 뿐이다), 노드에서 애니메이터를 &mut 로 빌린 동안 같은 노드의 Sprite 를 함께 빌릴 수 없다.
//     그래서 다음 tick 에 넣는다. 그 자리에서 넣으려면 노드를 받는 play_on_node 를 쓴다.
//   on_complete 는 애니메이터를 받는 한 번짜리 콜백(FnOnce(&mut SpriteAnimator))이다. 원본은 클로저가 애니메이터를 붙잡아 썼다.
//   fps 는 이름을 줄이지 않아 frame_per_second 다. 기본값(10, 반복)은 DEFAULT_FRAME_PER_SECOND 와 true 를 넘긴다.
//   아틀라스 JSON 은 serde_json::Value 로 받는다. 원본 findSprite(찾아서 들고 있기)는 형제 컴포넌트의 참조를 들고 있을 수 없어(빌림이 tick 을 넘어 살 수 없다)
//     옮기지 않았다. 대신 tick 마다 context.get_node_mut().get_component_mut::<Sprite>() 로 찾고, 밖에서는 apply_current_frame(sprite) 에 Sprite 를 넘긴다.
//==============================================================================
use crate::base::rect::Rect;
use crate::core::animation::Animation;
use crate::core::component::sprite::Sprite;
use crate::core::component::{Component, ComponentBase};
use crate::core::image::Image;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::WorldNode;
use std::collections::HashMap;
use std::rc::Rc;

// 클립이 끝났을 때 한 번 불린다. (애니메이터를 받아 다음 클립을 틀 수 있다)
type SpriteAnimatorCompleteEvent = Box<dyn FnOnce(&mut SpriteAnimator)>;

//==============================================================================
// 클립. (시트 한 장, 칸 영역 목록, 초당 프레임 수, 반복)
//==============================================================================
#[derive(Clone)]
struct SpriteAnimatorClip {
	image: Rc<Image>,
	frame_rects: Vec<Rect>,
	frame_per_second: f32,
	is_loop: bool,
}

//==============================================================================
// 재생 설정. (원본 options = { on_complete?, restart_if_same = false })
//==============================================================================
#[derive(Default)]
pub struct SpriteAnimatorPlayOptions {
	// 클립이 끝났을 때 한 번 불린다. (반복하지 않는 클립만 끝난다)
	pub on_complete: Option<SpriteAnimatorCompleteEvent>,
	// 같은 클립이 도는 중이어도 처음부터 다시 틀지.
	pub restart_if_same: bool,
}

impl SpriteAnimatorPlayOptions {
	//==============================================================================
	// 완료 콜백만 정한 설정.
	//==============================================================================
	pub fn create_with_on_complete(on_complete: impl FnOnce(&mut SpriteAnimator) + 'static) -> SpriteAnimatorPlayOptions {
		SpriteAnimatorPlayOptions {
			on_complete: Some(Box::new(on_complete)),
			restart_if_same: false,
		}
	}
}

//==============================================================================
// 스프라이트 애니메이터 컴포넌트.
//==============================================================================
pub struct SpriteAnimator {
	component_base: ComponentBase,
	clip_table: HashMap<String, SpriteAnimatorClip>,
	clip_name: Option<String>,
	animation: Animation,
	on_complete: Option<SpriteAnimatorCompleteEvent>,
}

#[crate::reflect]
impl SpriteAnimator {
	// 클립의 기본 초당 프레임 수. (원본 fps = 10)
	pub const DEFAULT_FRAME_PER_SECOND: f32 = 10.0;

	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> SpriteAnimator {
		SpriteAnimator {
			component_base: ComponentBase::new(),
			clip_table: HashMap::new(),
			clip_name: None,
			animation: Animation::new(),
			on_complete: None,
		}
	}

	//==============================================================================
	// 클립 등록. (같은 이름이 있으면 바꾼다)
	//==============================================================================
	pub fn add_clip(&mut self, clip_name: &str, image: Rc<Image>, frame_rects: &[Rect], frame_per_second: f32, is_loop: bool) {
		let clip = SpriteAnimatorClip {
			image,
			frame_rects: frame_rects.to_vec(),
			frame_per_second,
			is_loop,
		};
		self.clip_table.insert(clip_name.to_string(), clip);
	}

	//==============================================================================
	// 아틀라스 JSON 으로 클립 등록. (TexturePacker 꼴 { frames: { 이름: { frame: { x, y, w, h } } } }, 없는 칸 이름은 건너뛴다)
	//==============================================================================
	pub fn add_clip_from_atlas(&mut self, clip_name: &str, image: Rc<Image>, atlas_json: &serde_json::Value, frame_ids: &[&str], frame_per_second: f32, is_loop: bool) {
		let mut frame_rects: Vec<Rect> = Vec::new();
		for frame_id in frame_ids {
			let frame_data = match atlas_json.get("frames").and_then(|frames| frames.get(*frame_id)) {
				Some(frame_data) => {
					frame_data
				}
				None => {
					continue;
				}
			};
			let rect_data = frame_data.get("frame").unwrap_or(frame_data);
			let read_number = |names: &[&str]| -> f32 {
				for name in names {
					if let Some(number_value) = rect_data.get(*name).and_then(|value| value.as_f64()) {
						return number_value as f32;
					}
				}
				0.0
			};
			let x = read_number(&["x"]);
			let y = read_number(&["y"]);
			let width = read_number(&["w", "width"]);
			let height = read_number(&["h", "height"]);
			frame_rects.push(Rect::create(x, y, width, height));
		}
		self.add_clip(clip_name, image, &frame_rects, frame_per_second, is_loop);
	}

	//==============================================================================
	// 재생. (처음부터 틀었으면 true)
	// - 같은 클립이 이미 도는 중이면 다시 틀지 않고 완료 콜백만 바꾼다. (restart_if_same 이면 처음부터)
	// - Sprite 에는 다음 tick 에 들어간다. 그 자리에서 넣으려면 play_on_node.
	//==============================================================================
	pub fn play(&mut self, clip_name: &str, options: SpriteAnimatorPlayOptions) -> bool {
		let clip = match self.clip_table.get(clip_name) {
			Some(clip) => {
				clip
			}
			None => {
				return false;
			}
		};
		let is_same_clip = self.clip_name.as_deref() == Some(clip_name);
		if is_same_clip && self.animation.is_playing() && !options.restart_if_same {
			// 완주 콜백만 갈아 끼운다.
			self.on_complete = options.on_complete;
			return false;
		}
		self.animation.stop();
		self.animation.set_frames_from_rects(clip.image.clone(), &clip.frame_rects);
		self.animation.set_animation_speed(clip.frame_per_second);
		self.animation.set_loop(clip.is_loop);
		self.clip_name = Some(clip_name.to_string());
		self.on_complete = options.on_complete;
		self.animation.goto_and_play(0);
		true
	}

	//==============================================================================
	// 노드의 애니메이터로 재생하고 같은 노드의 Sprite 에 그 자리에서 첫 프레임을 넣는다. (원본 play 와 같은 효과)
	// - 애니메이터가 없으면 false.
	//==============================================================================
	pub fn play_on_node(node: &mut WorldNode, clip_name: &str, options: SpriteAnimatorPlayOptions) -> bool {
		let (is_started, current_frame) = match node.get_component_mut::<SpriteAnimator>() {
			Some(sprite_animator) => {
				let is_started = sprite_animator.play(clip_name, options);
				let current_frame = sprite_animator.animation.get_current_frame().cloned();
				(is_started, current_frame)
			}
			None => {
				return false;
			}
		};
		if let (Some(current_frame), Some(sprite)) = (current_frame, node.get_component_mut::<Sprite>()) {
			let frame_image = current_frame.get_image().clone();
			let frame_rect = current_frame.get_image_rect();
			sprite.set_image(Some(frame_image));
			sprite.set_image_rect(frame_rect);
		}
		is_started
	}

	//==============================================================================
	// 정지. (첫 프레임으로 되돌리고, 완료 콜백은 부르지 않는다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.animation.stop();
	}

	//==============================================================================
	// 지금 프레임을 스프라이트에 넣기. (프레임이 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn apply_current_frame(&self, sprite: &mut Sprite) {
		let current_frame = match self.animation.get_current_frame() {
			Some(current_frame) => {
				current_frame
			}
			None => {
				return;
			}
		};
		let frame_image = current_frame.get_image().clone();
		sprite.set_image(Some(frame_image));
		let frame_rect = current_frame.get_image_rect();
		sprite.set_image_rect(frame_rect);
	}

	//==============================================================================
	// 지금 클립 이름 반환. (아직 틀지 않았으면 None)
	//==============================================================================
	pub fn get_clip_name(&self) -> Option<&str> {
		self.clip_name.as_deref()
	}

	//==============================================================================
	// 재생 중인지.
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.animation.is_playing()
	}

	//==============================================================================
	// 클립이 있는지.
	//==============================================================================
	pub fn has_clip(&self, clip_name: &str) -> bool {
		self.clip_table.contains_key(clip_name)
	}

	//==============================================================================
	// 지금 프레임 번호.
	//==============================================================================
	pub fn get_current_frame_index(&self) -> usize {
		self.animation.get_current_frame_index()
	}
}

impl Default for SpriteAnimator {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> SpriteAnimator {
		SpriteAnimator::new()
	}
}

#[crate::reflect]
impl Component for SpriteAnimator {
	//==============================================================================
	// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"SpriteAnimator"
	}

	//==============================================================================
	// 갱신. (프레임을 넘기고, 반복하지 않는 클립이 끝났으면 완료 콜백, 그다음 같은 노드의 Sprite 에 지금 프레임을 넣는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		let was_playing = self.animation.is_playing();
		self.animation.tick(time_delta);
		let is_completed = was_playing && !self.animation.is_playing();
		if is_completed {
			if let Some(on_complete) = self.on_complete.take() {
				on_complete(self);
			}
		}
		if let Some(sprite) = context.get_node_mut().get_component_mut::<Sprite>() {
			self.apply_current_frame(sprite);
		}
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::NodeTree;
	use std::cell::Cell;

	//==============================================================================
	// 가로로 16x16 칸이 frame_count 개 있는 시트 영역.
	//==============================================================================
	fn create_sheet_rects(first_column: usize, frame_count: usize) -> Vec<Rect> {
		(first_column..first_column + frame_count).map(|column| Rect::create(column as f32 * 16.0, 0.0, 16.0, 16.0)).collect()
	}

	//==============================================================================
	// 그 자리 반영, 프레임 넘김, 같은 클립 다시 틀기, 끝나면 콜백으로 다음 클립.
	//==============================================================================
	#[test]
	fn play_advance_and_chain_on_complete() {
		let engine = Engine::new(EngineConfiguration::new());
		let image = Image::create_from_pixels(128, 16, vec![0u8; 128 * 16 * 4]);
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let hero_id = node_tree.create_child(root_id, "hero");
		node_tree[hero_id].add_component(Sprite::new());
		let animator = node_tree[hero_id].add_component(SpriteAnimator::new());
		animator.add_clip("idle", image.clone(), &create_sheet_rects(0, 2), 10.0, true);
		animator.add_clip("attack", image.clone(), &create_sheet_rects(4, 3), 10.0, false);
		assert!(animator.has_clip("idle"));
		assert!(!animator.play("missing", SpriteAnimatorPlayOptions::default()));

		let complete_count = std::rc::Rc::new(Cell::new(0));
		let complete_count_for_event = complete_count.clone();
		let is_started = SpriteAnimator::play_on_node(&mut node_tree[hero_id], "attack", SpriteAnimatorPlayOptions::create_with_on_complete(move |animator| {
			complete_count_for_event.set(complete_count_for_event.get() + 1);
			animator.play("idle", SpriteAnimatorPlayOptions::default());
		}));
		assert!(is_started);
		assert_eq!(node_tree[hero_id].get_component::<Sprite>().expect("sprite").get_image_rect(), Rect::create(64.0, 0.0, 16.0, 16.0));

		// 같은 클립은 다시 틀지 않는다.
		let animator = node_tree[hero_id].get_component_mut::<SpriteAnimator>().expect("animator");
		assert!(!animator.play("attack", SpriteAnimatorPlayOptions::create_with_on_complete(|animator| {
			animator.play("idle", SpriteAnimatorPlayOptions::default());
		})));

		node_tree.tick(&engine, 0.1);
		assert_eq!(node_tree[hero_id].get_component::<Sprite>().expect("sprite").get_image_rect(), Rect::create(80.0, 0.0, 16.0, 16.0));
		node_tree.tick(&engine, 0.1);
		node_tree.tick(&engine, 0.1);

		// 갈아 끼운 콜백이 불려 idle 로 넘어갔다. (처음 콜백은 불리지 않는다)
		assert_eq!(complete_count.get(), 0);
		let animator = node_tree[hero_id].get_component::<SpriteAnimator>().expect("animator");
		assert_eq!(animator.get_clip_name(), Some("idle"));
		assert!(animator.is_playing());
		assert_eq!(node_tree[hero_id].get_component::<Sprite>().expect("sprite").get_image_rect(), Rect::create(0.0, 0.0, 16.0, 16.0));

		// idle 은 반복한다.
		node_tree.tick(&engine, 0.1);
		node_tree.tick(&engine, 0.1);
		assert_eq!(node_tree[hero_id].get_component::<SpriteAnimator>().expect("animator").get_current_frame_index(), 0);
	}

	//==============================================================================
	// 아틀라스 JSON 으로 클립 만들기. (frame 아래, 또는 바로, w/h 또는 width/height)
	//==============================================================================
	#[test]
	fn add_clip_from_atlas_reads_rects() {
		let image = Image::create_from_pixels(64, 64, vec![0u8; 64 * 64 * 4]);
		let atlas_json = serde_json::json!({
			"frames": {
				"run_0": { "frame": { "x": 0, "y": 0, "w": 16, "h": 20 } },
				"run_1": { "x": 16, "y": 0, "width": 16, "height": 20 }
			}
		});
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let hero_id = node_tree.create_child(root_id, "hero");
		node_tree[hero_id].add_component(Sprite::new());
		let animator = node_tree[hero_id].add_component(SpriteAnimator::new());
		animator.add_clip_from_atlas("run", image, &atlas_json, &["run_0", "missing", "run_1"], SpriteAnimator::DEFAULT_FRAME_PER_SECOND, true);
		assert!(animator.play("run", SpriteAnimatorPlayOptions::default()));

		// play 만 하면 다음 tick 에 들어간다.
		assert!(node_tree[hero_id].get_component::<Sprite>().expect("sprite").get_image().is_none());
		node_tree.tick(&engine, 0.0);
		assert_eq!(node_tree[hero_id].get_component::<Sprite>().expect("sprite").get_image_rect(), Rect::create(0.0, 0.0, 16.0, 20.0));
		node_tree.tick(&engine, 0.1);
		assert_eq!(node_tree[hero_id].get_component::<Sprite>().expect("sprite").get_image_rect(), Rect::create(16.0, 0.0, 16.0, 20.0));
	}
}
