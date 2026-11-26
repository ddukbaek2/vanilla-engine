# 트윈과 타임라인

시간에 따라 값을 바꾸는 방법을 다룹니다. 값이나 노드 속성을 시작값에서 목표값으로 옮기는 `Tween`, 프레임 그림을 넘기는 `Animation` 과 `SpriteAnimator`, 타임라인 편집기가 저장한 문서를 재생하는 `Timeline`, 상태 묶음인 `Animator`, 스텝을 차례로 도는 `Action` 차례입니다. 노드 자체는 "노드와 컴포넌트" 장, 3D 모델의 본 애니메이션은 "3D" 장, 편집기는 "저작 도구와 두 엔진이 같이 쓰는 파일" 장에서 다룹니다.

## Tween

`Tween` 은 이름 붙은 값들을 시작값에서 목표값으로 옮깁니다. 값 묶음은 `TweenValues` 이고, 값 하나는 `TweenValue::Number(f32)` 또는 `TweenValue::Vector2(Vector2)` 입니다. 시작값과 목표값에 같은 이름이 있고 꼴이 같은 값만 바뀝니다.

- `TweenValues::new().with(이름, 값)` 으로 묶음을 만듭니다. 값에는 `f32` 나 `Vector2` 를 바로 줍니다. `set`, `get`, `get_number`, `get_vector2`, `get_count` 도 있습니다.
- `Tween::new(시작값)` 으로 만들고 `to(목표값, duration)`(초, 처음 1), `delay(seconds)`(처음 0), `easing(함수)`(처음 `easing_function::linear`)를 잇습니다. 이 메서드들은 트윈을 받아 고친 트윈을 돌려주므로 한 줄로 이어 씁니다.
- `set_update(|values, node_tree| {})` 는 대기가 끝난 뒤 갱신마다 이번 값 묶음과 노드 트리를 받고, `set_complete(|node_tree| {})` 는 진행률이 1 에 닿은 갱신에서 한 번 불립니다.
- `start()`, `stop()`, `tick(&mut node_tree, time_delta)`, `is_finished()` 로 직접 돌릴 수 있습니다. `duration` 이 0 이하면 첫 갱신에 끝으로 칩니다. `stop` 은 완료 콜백을 부르지 않습니다.

```rust
use std::cell::Cell;
use std::rc::Rc;
use vanilla::{easing_function, NodeTree, Tween, TweenValues};

let mut node_tree = NodeTree::new();
let last_x = Rc::new(Cell::new(0.0f32));
let last_x_in_update = last_x.clone();
let complete_count = Rc::new(Cell::new(0));
let complete_count_in_callback = complete_count.clone();
let mut tween = Tween::new(TweenValues::new().with("x", 0.0).with("alpha", 1.0))
	.to(TweenValues::new().with("x", 100.0).with("alpha", 0.0), 0.5)
	.easing(easing_function::quadratic::out)
	.set_update(move |values, _node_tree| {
		last_x_in_update.set(values.get_number("x").unwrap());
	})
	.set_complete(move |_node_tree| {
		complete_count_in_callback.set(complete_count_in_callback.get() + 1);
	});
tween.start();
for _ in 0..4 {
	tween.tick(&mut node_tree, 0.125);
}
assert!(tween.is_finished());
assert_eq!(last_x.get(), 100.0);
assert_eq!(complete_count.get(), 1);
```

### 이징

`easing_function` 모듈에 `linear` 와 `quadratic`, `cubic`, `quartic`, `quintic`, `sinusoidal`, `exponential`, `circular`, `elastic`, `back`, `bounce` 모듈이 있고, `linear` 를 뺀 것은 저마다 `r#in`, `out`, `in_out` 함수를 가집니다(`easing_function::back::out` 처럼). `in` 은 러스트 예약어라 `r#in` 으로 부릅니다. 진행률(0 ~ 1)을 받아 바뀐 진행률을 돌려주는 함수나 클로저면 무엇이든 `easing` 에 줄 수 있습니다. `back` 과 `elastic` 은 0 ~ 1 밖으로 넘칩니다.

```rust
use vanilla::easing_function;

assert_eq!(easing_function::linear(0.25), 0.25);
assert_eq!(easing_function::quadratic::r#in(0.5), 0.25);
assert_eq!(easing_function::quadratic::out(0.5), 0.75);
assert_eq!(easing_function::cubic::in_out(0.5), 0.5);
let four_steps = |progress: f32| (progress * 4.0).floor() / 4.0;
assert_eq!(four_steps(0.6), 0.5);
```

### 노드 속성 트윈

`set_target(node_id)` 로 대상 노드를 정하면 값 이름이 `TweenProperty` 의 이름인 값을 갱신마다 그 노드에 바로 씁니다. 갱신 콜백은 노드에 쓴 다음에 불립니다.

| `TweenProperty` | 값 이름 | 값 | 쓰는 곳 |
| --- | --- | --- | --- |
| `LocalPosition` | `"localPosition"` | `Vector2` | `set_local_position` |
| `LocalScale` | `"localScale"` | `Vector2` | `set_local_scale` |
| `LocalRotation` | `"localRotation"` | 숫자 (도) | `set_local_rotation` |
| `LocalOpacity` | `"localOpacity"` | 숫자 (0 ~ 1) | `set_local_opacity` |
| `ContentSize` | `"contentSize"` | `Vector2` (픽셀) | `set_content_size` |

`Vector2` 속성에 숫자를 주면 두 축에 같이 씁니다(`"localScale"` 에 `0.0` 은 `(0, 0)`). 숫자 속성에 `Vector2` 를 주면 쓰지 않습니다. 값 이름은 원본처럼 camelCase 글이고, `TweenProperty::get_name()` 과 `TweenProperty::find_by_name(이름)` 으로 오갑니다.

`Tween::create_node_tween(node_id, tween_property, start_value, end_value, duration)` 은 속성 하나를 움직이는 트윈을 한 줄로 만듭니다. 노드에 값 하나만 쓰고 싶으면 `tween_property.apply_to_node(&mut node, value)` 를 씁니다(원본 `applyTweenProperty`).

```rust
use vanilla::{NodeTree, Tween, TweenProperty, TweenValue, TweenValues, Vector2};

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let box_id = node_tree.create_child(root_id, "Box");

let mut move_tween = Tween::new(TweenValues::new().with("localPosition", Vector2::create(160.0, 360.0)).with("localOpacity", 0.0))
	.to(TweenValues::new().with("localPosition", Vector2::create(1120.0, 360.0)).with("localOpacity", 1.0), 1.5)
	.delay(0.5)
	.set_target(box_id);
move_tween.start();
move_tween.tick(&mut node_tree, 0.5);
move_tween.tick(&mut node_tree, 0.75);
assert_eq!(node_tree[box_id].get_local_position(), Vector2::create(640.0, 360.0));
assert_eq!(node_tree[box_id].get_local_opacity(), 0.5);

let mut pulse_tween = Tween::create_node_tween(box_id, TweenProperty::LocalScale, 1.0, 1.4, 0.4);
pulse_tween.start();
pulse_tween.tick(&mut node_tree, 0.4);
assert_eq!(node_tree[box_id].get_local_scale(), Vector2::create(1.4, 1.4));

TweenProperty::LocalRotation.apply_to_node(&mut node_tree[box_id], TweenValue::Number(45.0));
assert_eq!(node_tree[box_id].get_local_rotation(), 45.0);
assert_eq!(TweenProperty::find_by_name("localScale"), Some(TweenProperty::LocalScale));
```

대상 노드가 지워지거나(`destroy_node`) 트윈이 처음 돈 때의 노드 트리에서 떨어지면(맨 위 노드가 달라지면) 트윈은 완료 콜백 없이 조용히 끝납니다.

### 씬에 맡기기

씬에서는 `scene_base.start_tween(tween)` 으로 맡깁니다. 씬이 트윈을 가져가 시작하고, `SceneBase::tick` 이 노드 갱신과 터치 다음에 트윈을 갱신하며, 끝난 트윈은 목록에서 뺍니다. 원본은 트윈 객체를 들고 가리키지만 러스트 판은 씬이 트윈을 가져가므로 `start_tween` 이 돌려준 `TweenId` 로 가리킵니다.

- `stop_tween(tween_id)`(완료 콜백 없이 멈추고 뺌), `stop_all_tweens()`, `get_tween(tween_id)` 와 `get_tween_mut(tween_id)`(끝나 빠졌으면 `None`), `get_tween_count()`(대기 중인 것 포함).
- 같은 속성을 두 트윈이 움직이면 먼저 시작한 트윈이 나중에 돌아 이깁니다(원본과 같음).
- 완료 콜백은 노드 트리만 받고 씬은 받지 않습니다. 다음 트윈을 이어 걸려면 아래처럼 씬의 `tick` 에서 `get_tween(tween_id)` 가 `None` 이 되었는지 보고 새로 맡깁니다.

```rust,no_run
use vanilla::{easing_function, Color, Engine, EngineConfiguration, NodeId, Paint, Scene, SceneBase, Tween, TweenId, TweenProperty, TweenValues, Vector2};

struct TweenScene {
	scene_base: SceneBase,
	box_id: Option<NodeId>,
	move_tween_id: Option<TweenId>,
	pulse_tween_id: Option<TweenId>,
	is_pulse_growing: bool,
}

impl TweenScene {
	fn is_tween_running(&self, tween_id: Option<TweenId>) -> bool {
		match tween_id {
			Some(tween_id) => {
				self.scene_base.get_tween(tween_id).is_some()
			}
			None => {
				false
			}
		}
	}

	fn start_next_pulse(&mut self) {
		if self.is_tween_running(self.move_tween_id) || self.is_tween_running(self.pulse_tween_id) {
			return;
		}
		if let Some(box_id) = self.box_id {
			let (start_scale, end_scale) = if self.is_pulse_growing {
				(1.0, 1.4)
			}
			else {
				(1.4, 1.0)
			};
			let pulse_tween = Tween::create_node_tween(box_id, TweenProperty::LocalScale, start_scale, end_scale, 0.4).easing(easing_function::sinusoidal::in_out);
			self.pulse_tween_id = Some(self.scene_base.start_tween(pulse_tween));
			self.is_pulse_growing = !self.is_pulse_growing;
		}
	}
}

impl Scene for TweenScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, _engine: &mut Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let box_id = node_tree.create_child(root_id, "Box");
		let box_node = &mut node_tree[box_id];
		box_node.set_content_size(Vector2::create(80.0, 80.0));
		box_node.set_local_position(Vector2::create(160.0, 360.0));
		let paint = box_node.add_component(Paint::create_with_color(Color::create(0.35, 0.8, 1.0, 1.0)));
		paint.set_round_size(12.0);
		self.box_id = Some(box_id);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		if let Some(box_id) = self.box_id {
			let move_tween = Tween::new(TweenValues::new().with("localPosition", Vector2::create(160.0, 360.0)).with("localOpacity", 0.0))
				.to(TweenValues::new().with("localPosition", Vector2::create(1120.0, 360.0)).with("localOpacity", 1.0), 1.5)
				.delay(0.2)
				.easing(easing_function::cubic::in_out)
				.set_target(box_id);
			self.move_tween_id = Some(self.scene_base.start_tween(move_tween));
		}
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.start_next_pulse();
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let tween_scene = TweenScene {
		scene_base: SceneBase::new(),
		box_id: None,
		move_tween_id: None,
		pulse_tween_id: None,
		is_pulse_growing: true,
	};
	if let Err(error) = engine.run(Box::new(tween_scene)) {
		eprintln!("{}", error);
	}
}
```

`Scene::tick` 을 덮어쓰면 위처럼 `self.scene_base.tick(engine, time_delta)` 를 직접 불러야 노드와 트윈이 갱신됩니다.

## Animation 과 Frame

`Frame` 은 그림 하나(`Rc<Image>`)와 그 안의 영역(`Rect`, 픽셀)입니다. `Frame::new(image, Some(rect))` 로 만들고 영역에 `None` 을 주면 그림 전체입니다. 영역을 그림 크기 안으로 자르지 않습니다. `Sprite::set_frame(Some(&frame))` 은 그림과 영역을 함께 겁니다(`None` 이면 그림을 뗍니다).

`Animation` 은 프레임 목록을 정해진 빠르기로 넘기는 처리기입니다. 노드에 붙는 컴포넌트가 아니므로 `tick` 을 직접 부르고, 지금 프레임을 읽어 그립니다.

- 프레임은 `set_frames(Vec<Frame>)`, `set_frames_from_rects(image, &rects)`(스프라이트 시트 한 장과 칸 영역들), `set_frames_from_images(&images)`(낱장 그림들)로 넣습니다. 넣으면 첫 프레임으로 돌아가 멈춥니다. 뒤의 두 함수는 빈 목록을 받으면 아무것도 하지 않습니다.
- `set_animation_speed(frames_per_second)` 는 초당 넘기는 프레임 수입니다(처음 10, 0 이하면 넘기지 않음). `set_loop(is_loop)`(처음 `true`), `set_on_complete(|| {})`(반복하지 않을 때 마지막 프레임에 닿으면 불림).
- `play()`, `pause()`(지금 프레임에 멈춤), `resume()`, `stop()`(첫 프레임으로), `goto_and_play(index)`, `goto_and_stop(index)`. 번호는 `usize` 이고 프레임 수 안으로 맞춥니다.
- `get_current_frame()`(`Option<&Frame>`), `get_current_image()`, `get_current_rect()`, `get_current_frame_index()`, `get_total_frame_count()`, `is_playing()`.
- 한 번의 `tick` 에 쌓인 시간만큼 여러 프레임을 넘길 수 있고, 한 프레임에 못 미친 시간은 다음 `tick` 으로 넘깁니다.

```rust
use vanilla::{Animation, Frame, Image, Rect, Sprite};

let sheet = Image::create_from_pixels(64, 16, vec![255u8; 64 * 16 * 4]);
let whole_frame = Frame::new(sheet.clone(), None);
assert_eq!(whole_frame.get_image_rect(), Rect::create(0.0, 0.0, 64.0, 16.0));

let rects: Vec<Rect> = (0..4).map(|frame_index| Rect::create(16.0 * frame_index as f32, 0.0, 16.0, 16.0)).collect();
let mut animation = Animation::new();
animation.set_frames_from_rects(sheet, &rects);
animation.set_animation_speed(8.0);
animation.set_loop(false);
animation.play();
animation.tick(0.25);
assert_eq!(animation.get_current_frame_index(), 2);

let mut sprite = Sprite::new();
sprite.set_frame(animation.get_current_frame());
assert_eq!(sprite.get_image_rect(), Rect::create(32.0, 0.0, 16.0, 16.0));

animation.tick(1.0);
assert_eq!(animation.get_current_frame_index(), 3);
assert!(!animation.is_playing());
```

아래는 저장소의 `examples/assets/atlas/icons.png` 에서 아이콘 네 칸을 떼어 차례로 넘기는 씬입니다. 그림은 `create` 에서 요청해 씬에 맡기고, 다 읽힌 뒤 불리는 `initialize` 에서 프레임을 넣습니다.

```rust,no_run
use vanilla::{Animation, Engine, EngineConfiguration, ImageAsset, NodeId, Rect, Scene, SceneBase, Sprite, Vector2};

struct AnimationScene {
	scene_base: SceneBase,
	image_asset: ImageAsset,
	animation: Animation,
	icon_id: Option<NodeId>,
}

impl Scene for AnimationScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.image_asset.load(engine, "examples/assets/atlas/icons.png");
		self.scene_base.track_asset(&self.image_asset);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let image = match self.image_asset.get_image() {
			Some(image) => {
				image
			}
			None => {
				return;
			}
		};
		let rects = [Rect::create(2.0, 2.0, 56.0, 40.0), Rect::create(63.0, 2.0, 48.0, 48.0), Rect::create(39.0, 55.0, 40.0, 36.0), Rect::create(84.0, 55.0, 30.0, 30.0)];
		self.animation.set_frames_from_rects(image, &rects);
		self.animation.set_animation_speed(4.0);
		self.animation.set_loop(true);
		self.animation.play();

		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let icon_id = node_tree.create_child(root_id, "Icon");
		let icon_node = &mut node_tree[icon_id];
		icon_node.set_content_size(Vector2::create(96.0, 96.0));
		icon_node.set_local_position(Vector2::create(640.0, 360.0));
		icon_node.add_component(Sprite::new());
		self.icon_id = Some(icon_id);
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.animation.tick(time_delta);
		if let Some(icon_id) = self.icon_id {
			let node_tree = self.scene_base.get_node_tree_mut();
			if let Some(sprite) = node_tree[icon_id].get_component_mut::<Sprite>() {
				sprite.set_frame(self.animation.get_current_frame());
			}
		}
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let animation_scene = AnimationScene {
		scene_base: SceneBase::new(),
		image_asset: ImageAsset::new(),
		animation: Animation::new(),
		icon_id: None,
	};
	if let Err(error) = engine.run(Box::new(animation_scene)) {
		eprintln!("{}", error);
	}
}
```

## SpriteAnimator

`SpriteAnimator` 는 이름 붙은 클립(프레임 차례)을 등록해 두고 `play("run", ...)` 처럼 바꾸는 컴포넌트입니다. 노드 갱신마다 같은 노드의 `Sprite` 에 지금 프레임을 걸어 줍니다. 그래서 노드에 `Sprite` 를 먼저 붙여 둡니다.

- `add_clip(clip_name, image, &frame_rects, frame_per_second, is_loop)` 는 클립을 등록합니다. 러스트에는 기본 인자가 없어서 늘 다 넘깁니다. 원본 기본값은 `SpriteAnimator::DEFAULT_FRAME_PER_SECOND`(10)과 `true` 입니다. 같은 이름이 있으면 바꿉니다.
- `add_clip_from_atlas(clip_name, image, &atlas_json, &frame_ids, frame_per_second, is_loop)` 는 TexturePacker 꼴(`{ "frames": { 이름: { "frame": { "x", "y", "w", "h" } } } }`) 표에서 프레임을 뽑습니다. 표는 `serde_json::Value` 이고 없는 칸 이름은 건너뜁니다.
- `play(clip_name, options)` 는 클립을 바꿉니다. `options` 는 `SpriteAnimatorPlayOptions { on_complete, restart_if_same }` 이고 `SpriteAnimatorPlayOptions::default()` 는 콜백 없음, 다시 틀지 않음입니다. 같은 클립이 이미 돌고 있으면 다시 시작하지 않고(`restart_if_same: true` 면 다시 시작) 완료 콜백만 바꿉니다. 처음부터 틀었으면 `true` 를 돌려줍니다.
- 완료 콜백은 애니메이터를 받는 한 번짜리 함수(`FnOnce(&mut SpriteAnimator)`)입니다. `SpriteAnimatorPlayOptions::create_with_on_complete(|animator| {})` 로 콜백만 정한 옵션을 만들 수 있습니다.
- `play` 는 프레임을 다음 노드 갱신에 `Sprite` 에 넣습니다. 그 자리에서 첫 프레임까지 넣으려면 노드를 받는 `SpriteAnimator::play_on_node(&mut node, clip_name, options)` 를 씁니다(원본 `play` 와 같은 효과, 노드에 애니메이터가 없으면 `false`).
- `stop()`(첫 프레임으로, 완료 콜백 없음), `get_clip_name()`(`Option<&str>`), `is_playing()`, `get_current_frame_index()`, `has_clip(clip_name)`, `apply_current_frame(&mut sprite)` 가 있습니다.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, ImageAsset, NodeId, Rect, Scene, SceneBase, Sprite, SpriteAnimator, SpriteAnimatorPlayOptions, Vector2};

struct AnimatorScene {
	scene_base: SceneBase,
	image_asset: ImageAsset,
	hero_id: Option<NodeId>,
}

impl Scene for AnimatorScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.image_asset.load(engine, "examples/assets/atlas/icons.png");
		self.scene_base.track_asset(&self.image_asset);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let image = match self.image_asset.get_image() {
			Some(image) => {
				image
			}
			None => {
				return;
			}
		};
		let idle_rects = [Rect::create(2.0, 2.0, 56.0, 40.0), Rect::create(63.0, 2.0, 48.0, 48.0)];
		let attack_rects = [Rect::create(39.0, 55.0, 40.0, 36.0), Rect::create(84.0, 55.0, 30.0, 30.0), Rect::create(2.0, 47.0, 32.0, 56.0)];

		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let hero_id = node_tree.create_child(root_id, "Hero");
		let hero_node = &mut node_tree[hero_id];
		hero_node.set_content_size(Vector2::create(96.0, 96.0));
		hero_node.set_local_position(Vector2::create(640.0, 360.0));
		hero_node.add_component(Sprite::new());
		let sprite_animator = hero_node.add_component(SpriteAnimator::new());
		sprite_animator.add_clip("idle", image.clone(), &idle_rects, 4.0, true);
		sprite_animator.add_clip("attack", image, &attack_rects, 12.0, false);
		SpriteAnimator::play_on_node(hero_node, "idle", SpriteAnimatorPlayOptions::default());
		self.hero_id = Some(hero_id);
	}

	fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
		if let Some(hero_id) = self.hero_id {
			let options = SpriteAnimatorPlayOptions {
				on_complete: Some(Box::new(|sprite_animator: &mut SpriteAnimator| {
					sprite_animator.play("idle", SpriteAnimatorPlayOptions::default());
				})),
				restart_if_same: true,
			};
			let hero_node = &mut self.scene_base.get_node_tree_mut()[hero_id];
			SpriteAnimator::play_on_node(hero_node, "attack", options);
		}
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let animator_scene = AnimatorScene {
		scene_base: SceneBase::new(),
		image_asset: ImageAsset::new(),
		hero_id: None,
	};
	if let Err(error) = engine.run(Box::new(animator_scene)) {
		eprintln!("{}", error);
	}
}
```

## Timeline

`Timeline` 은 타임라인 편집기가 저장한 문서(`*.timeline.json`)를 재생합니다. 문서는 무대 노드 목록, 트랙(대상 이름, 속성, 키), 마커로 이루어진 JSON 이고, 키 이름은 원본과 같은 camelCase 입니다.

```json
{
	"name": "Title Reveal",
	"duration": 5,
	"frameRate": 30,
	"loop": true,
	"stage": {
		"width": 960,
		"height": 540,
		"backgroundColor": "#101114ff",
		"nodes": [
			{ "name": "Logo", "type": "text", "parent": null, "x": 480, "y": 270, "text": "LOGO", "fontSize": 64 }
		]
	},
	"tracks": [
		{ "target": "Logo", "property": "opacity", "keys": [
			{ "time": 0, "value": 0, "easing": "cubic.out" },
			{ "time": 1, "value": 1, "easing": "linear" }
		] },
		{ "target": "Logo", "property": "event", "keys": [{ "time": 1, "value": "shine", "easing": "step" }] }
	],
	"markers": [{ "time": 0.5, "name": "hit" }]
}
```

### 만들기와 무대 짓기

- `Timeline::load_from_text(text)` 는 JSON 글에서 만들고, 읽지 못하면 까닭을 담은 `Err` 를 돌려줍니다. `Timeline::create_from_json_asset(&mut json_asset)` 은 읽은 `JsonAsset` 에서 만듭니다(아직 안 읽혔으면 `None`). `Timeline::new(Some(value))` 는 `serde_json::Value` 문서를 그대로 씁니다. 이것을 쓰려면 게임의 `Cargo.toml` 에 `serde_json` 을 더합니다.
- `Timeline::new(None)` 과 `Timeline::default()` 는 이름 `"Timeline"`, 3 초, 30 프레임, 반복, 960 x 540 무대(배경 `"#101114ff"`)의 빈 문서입니다. 문서에 빠진 `stage`, `stage.nodes`, `tracks`, `markers` 는 채웁니다.
- `Timeline::load_from_url(engine, url)` 은 애셋 경로나 `"data:"` 주소의 바이트를 엔진에 부탁하고, await 하면 `Result<Timeline, String>` 이 나오는 Future 를 돌려줍니다. 엔진 작업 안에서 await 합니다("async/await" 장). 받기나 JSON 읽기에 실패하면 `Err` 입니다.
- `build_stage(&mut node_tree, parent_id, image_resolver)` 는 문서의 무대 노드를 만들어 `parent_id` 아래에 붙이고, 이름에서 노드로 가는 `HashMap<String, NodeId>` 를 돌려줍니다. 만든 노드는 트랙 대상으로도 등록합니다. 노드 종류는 `group`(빈 노드), `paint`(`Paint`), `sprite`(`ShaderSprite`, 격자로 자른 프레임과 효과), `text`(`Text`), `particle`(`ParticleSystem`), `sound`(`TimelineSound`) 입니다. `image_resolver` 는 문서의 그림 이름을 받아 `Option<Rc<Image>>` 를 돌려주는 함수의 참조이고, `None` 이면 `"data:"` 주소 그림만 풉니다. 같은 이름의 노드가 둘이면 둘 다 붙고 표에는 뒤의 것이 남습니다.
- 무대를 직접 지었으면 `bind(&node_tree, root_id)` 로 트랙의 대상 이름과 같은 이름의 노드를 `root_id` 자신과 그 자손에서 찾아 붙입니다. 이미 등록된 이름은 건너뛰고 못 찾은 이름은 등록하지 않습니다.
- 대상은 `TimelineTarget` 입니다. 노드는 `TimelineTarget::Node(node_id)`, 노드가 아닌 것은 `TimelineTarget::External(Rc<RefCell<dyn TimelinePropertyTarget>>)` 입니다. `set_target(name, target)` 으로 하나씩 붙이거나, `set_target_resolver(Some(Box::new(|name| ...)))` 로 이름을 대상으로 바꾸는 함수를 줍니다.
- 사운드 노드를 쓰려면 원본 `Timeline.setAudioManager` 대신 무대를 지은 뒤 `timeline.load_sounds(&mut node_tree, engine)` 을 한 번 부릅니다. 사운드 노드마다 오디오 애셋을 읽고 플레이어를 만들며, 읽은 애셋 목록을 돌려줍니다(씬이 `track_audio_asset` 으로 맡겨도 됩니다).

### 재생

- `play()`, `pause()`, `stop(&mut node_tree)`(처음으로, 소리와 파티클도 멈춤), `seek(&mut node_tree, time)`(0 ~ 길이로 자른 시각의 값을 바로 적용, 이벤트는 내지 않음).
- `tick(&mut node_tree, time_delta)` 를 매 프레임 직접 부릅니다. 시간을 흘리고 값을 적용하며, 지나친 이벤트 키와 마커를 알립니다. 반복이면 끝을 넘을 때 감아 돌고, 아니면 끝에서 멈춥니다.
- `set_speed(speed)`(1 이 실시간, 음수면 거꾸로 가며 0 에서 멈추고 이벤트는 나지 않음), `get_speed()`, `set_loop(is_loop)`, `is_loop()`, `get_time()`, `get_duration()`, `get_frame_rate()`(문서의 `frameRate`, 재생에는 쓰지 않음), `is_playing()`.
- 처리기는 `Option<Box<dyn FnMut(...)>>` 로 겁니다. `set_event_handler`(대상 이름, 이벤트 이름, 키, 타임라인, 노드 트리), `set_marker_handler`(마커 이름, 마커, 타임라인, 노드 트리), `set_complete_handler`(타임라인, 노드 트리, 반복이 아닐 때 끝에 닿으면). `None` 을 주면 뗍니다. 부르는 동안 처리기를 꺼내 두므로 처리기 안에서 받은 타임라인의 `pause`, `seek`, `set_*_handler` 를 불러도 됩니다.

아래는 문서를 코드로 만들어 이미 있는 노드에 붙이는 예입니다. 이름이 `Box` 인 노드를 좌우로 옮기며 색을 바꾸고, 0.5 초에 마커를, 1 초에 이벤트를 냅니다.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use vanilla::{Color, NodeTree, Paint, Timeline, TimelineKey, TimelineMarker, Vector2};

const BOX_TIMELINE: &str = r##"{
	"name": "Box",
	"duration": 2,
	"frameRate": 30,
	"loop": true,
	"tracks": [
		{ "target": "Box", "property": "x", "keys": [
			{ "time": 0, "value": 120, "easing": "cubic.inOut" },
			{ "time": 1, "value": 520, "easing": "cubic.inOut" },
			{ "time": 2, "value": 120, "easing": "linear" }
		] },
		{ "target": "Box", "property": "color", "keys": [
			{ "time": 0, "value": "#3b82f6ff", "easing": "linear" },
			{ "time": 1, "value": "#f97316ff", "easing": "linear" },
			{ "time": 2, "value": "#3b82f6ff", "easing": "linear" }
		] },
		{ "target": "Box", "property": "event", "keys": [{ "time": 1, "value": "bounce", "easing": "step" }] }
	],
	"markers": [{ "time": 0.5, "name": "half" }]
}"##;

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let box_id = node_tree.create_child(root_id, "Box");
node_tree[box_id].set_content_size(Vector2::create(64.0, 64.0));
node_tree[box_id].set_local_position(Vector2::create(120.0, 180.0));
node_tree[box_id].add_component(Paint::create_with_color(Color::white()));

let signal_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
let mut timeline = Timeline::load_from_text(BOX_TIMELINE).expect("timeline");
timeline.bind(&node_tree, root_id);
let event_list = signal_list.clone();
timeline.set_event_handler(Some(Box::new(move |target_name: &str, event_name: &str, _key: &TimelineKey, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
	event_list.borrow_mut().push(format!("event {} {}", target_name, event_name));
})));
let marker_list = signal_list.clone();
timeline.set_marker_handler(Some(Box::new(move |marker_name: &str, _marker: &TimelineMarker, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
	marker_list.borrow_mut().push(format!("marker {}", marker_name));
})));
timeline.play();
timeline.tick(&mut node_tree, 0.5);
assert_eq!(node_tree[box_id].get_local_position(), Vector2::create(320.0, 180.0));
timeline.tick(&mut node_tree, 0.5);
assert_eq!(node_tree[box_id].get_local_position().x, 520.0);
assert_eq!(node_tree[box_id].get_component::<Paint>().unwrap().get_color(), Color::create_from_hex("#f97316ff"));
assert_eq!(*signal_list.borrow(), vec![String::from("marker half"), String::from("event Box bounce")]);
```

씬에서는 `initialize` 에서 무대를 짓고 `tick` 에서 재생합니다. 아래는 문서를 글로 넣고, `sprite` 노드의 그림 이름 `"icons.png"` 를 저장소의 `examples/assets/atlas/icons.png` 로 풀어 줍니다.

```rust,no_run
use std::rc::Rc;
use vanilla::{Engine, EngineConfiguration, Image, ImageAsset, NodeTree, Scene, SceneBase, Timeline, TimelineKey, TimelineMarker, Vector2};

const INTRO_TIMELINE: &str = r##"{
	"name": "Intro",
	"duration": 3,
	"loop": true,
	"stage": {
		"width": 960,
		"height": 540,
		"backgroundColor": "#101114ff",
		"nodes": [
			{ "name": "Logo", "type": "text", "x": 480, "y": 200, "text": "VANILLA", "fontSize": 64 },
			{ "name": "Icons", "type": "sprite", "x": 480, "y": 360, "width": 128, "height": 128, "image": "icons.png" }
		]
	},
	"tracks": [
		{ "target": "Logo", "property": "opacity", "keys": [
			{ "time": 0, "value": 0, "easing": "cubic.out" },
			{ "time": 1, "value": 1, "easing": "linear" }
		] },
		{ "target": "Icons", "property": "rotation", "keys": [
			{ "time": 0, "value": 0, "easing": "back.out" },
			{ "time": 2, "value": 360, "easing": "linear" }
		] },
		{ "target": "Logo", "property": "event", "keys": [{ "time": 1, "value": "shine", "easing": "step" }] }
	],
	"markers": [{ "time": 0.5, "name": "hit" }]
}"##;

struct TimelineScene {
	scene_base: SceneBase,
	image_asset: ImageAsset,
	timeline: Timeline,
}

impl Scene for TimelineScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.image_asset.load(engine, "examples/assets/atlas/icons.png");
		self.scene_base.track_asset(&self.image_asset);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let icons_image = self.image_asset.get_image();
		let image_resolver = |image_name: &str| -> Option<Rc<Image>> {
			if image_name == "icons.png" {
				icons_image.clone()
			}
			else {
				None
			}
		};
		self.timeline = Timeline::load_from_text(INTRO_TIMELINE).unwrap_or_default();
		let root_id = self.scene_base.get_root_id();
		self.timeline.build_stage(self.scene_base.get_node_tree_mut(), root_id, Some(&image_resolver));
		self.timeline.set_marker_handler(Some(Box::new(|marker_name: &str, _marker: &TimelineMarker, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			println!("marker {}", marker_name);
		})));
		self.timeline.set_event_handler(Some(Box::new(|target_name: &str, event_name: &str, _key: &TimelineKey, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			println!("event {} {}", target_name, event_name);
		})));
		self.timeline.play();
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.timeline.tick(self.scene_base.get_node_tree_mut(), time_delta);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(960.0, 540.0);
	let engine = Engine::new(engine_configuration);
	let timeline_scene = TimelineScene {
		scene_base: SceneBase::new(),
		image_asset: ImageAsset::new(),
		timeline: Timeline::default(),
	};
	if let Err(error) = engine.run(Box::new(timeline_scene)) {
		eprintln!("{}", error);
	}
}
```

### 트랙과 키

- 키의 `easing` 은 그 키부터 다음 키까지 구간에 걸립니다. 이름은 `TIMELINE_EASING_NAMES`(33 개)에 있고, `"linear"`, `"step"`, `"bezier"`(키의 `"curve": [x1, y1, x2, y2]`, CSS `cubic-bezier` 와 같은 제어점, 없으면 `[0.42, 0, 0.58, 1]`), 그리고 `"cubic.out"` 처럼 `<종류>.<in|out|inOut>` 입니다. 문서의 이름은 원본처럼 `inOut` 이고 러스트 함수 이름만 `in_out` 입니다. 빈 이름과 모르는 이름은 `linear` 로 칩니다.
- `resolve_timeline_easing(name, curve)` 는 함수 대신 `TimelineEasing` 값을 돌려주고 `evaluate(progress)` 로 부릅니다. `create_bezier_easing(x1, y1, x2, y2)` 는 같은 베지어 곡선을 클로저로 돌려줍니다.
- 노드 속성은 `TIMELINE_PROPERTY_DEFINITIONS` 배열에 있고 `find_timeline_property_definition(name)` 으로 찾습니다: `x`, `y`, `width`, `height`, `scaleX`, `scaleY`, `rotation`, `opacity`, `visible`, `color`, `text`, `number`, `fontSize`, `visibleCharacters`, `frame`, `effect`, `event`. 숫자는 이징으로 보간하고, 색(`"#rrggbbaa"`)은 채널마다 보간하며, 글과 참거짓은 계단으로 바뀝니다. 글 속성은 `Text` 노드와 `RichText` 노드에 모두 겁니다.
- 노드 대상에는 이 목록에 있는 속성만 겁니다. `External` 대상에는 트랙의 속성 이름과 값을 그대로 `set_timeline_property(name, value)` 로 넘깁니다(원본은 목록에 없는 속성을 대상의 `setTimelineProperty` 로 넘깁니다). `ScreenEffect` 는 이 트레이트를 구현해 두었습니다("파티클" 장). 마지막 적용에서 `External` 대상에 건 값은 `get_external_properties()` 로 읽습니다.
- 속성이 `event` 인 트랙은 키의 값을 이벤트 이름으로 알립니다. 대상이 파티클 노드면 `"play"`, `"stop"`, `"clear"`, `"emit:N"`(N 개 뿜기, 없으면 30)을, 사운드 노드면 `"play"`, `"loop"`, `"stop"` 을 스스로 처리하고, 그 뒤 이벤트 처리기도 부릅니다.
- 대상 노드에 붙은 `ParticleSystem` 은 타임라인 시간으로만 진행합니다. 이벤트 트랙이 없는 파티클은 0 초에 튼 것으로 보고, `seek` 나 감아 돌 때는 비운 뒤 그 시각까지 다시 진행해서 되감을 때마다 같은 모양이 나옵니다.

아래는 노드가 아닌 대상에 값을 넘기는 예입니다. `"screen"` 이라는 이름을 해석기가 `External` 대상으로 바꿔 줍니다.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use vanilla::{resolve_timeline_easing, NodeTree, Timeline, TimelinePropertyTarget, TimelineTarget, TimelineValue};

struct Vignette {
	strength: f32,
}

impl TimelinePropertyTarget for Vignette {
	fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue) {
		if property_name == "strength" {
			self.strength = value.as_number();
		}
	}
}

let vignette = Rc::new(RefCell::new(Vignette { strength: 0.0 }));
let vignette_target: Rc<RefCell<dyn TimelinePropertyTarget>> = vignette.clone();
let mut timeline = Timeline::load_from_text(r#"{
	"duration": 2,
	"tracks": [{ "target": "screen", "property": "strength", "keys": [
		{ "time": 0, "value": 0, "easing": "bezier", "curve": [0.42, 0, 0.58, 1] },
		{ "time": 2, "value": 1 }
	] }]
}"#).expect("timeline");
timeline.set_target_resolver(Some(Box::new(move |target_name: &str| {
	if target_name == "screen" {
		Some(TimelineTarget::External(vignette_target.clone()))
	}
	else {
		None
	}
})));
let mut node_tree = NodeTree::new();
timeline.seek(&mut node_tree, 1.0);
assert!((vignette.borrow().strength - 0.5).abs() < 0.001);
assert_eq!(timeline.get_external_properties().len(), 1);

assert_eq!(resolve_timeline_easing("cubic.inOut", None).evaluate(0.5), 0.5);
assert_eq!(resolve_timeline_easing("step", None).evaluate(0.9), 0.0);
assert_eq!(resolve_timeline_easing("unknown", None).evaluate(0.3), 0.3);
```

## Animator

`Animator`, `AnimationState`, `AnimationTransition`, `AnimationClip` 은 원본에서도 실험 단계의 뼈대이고, 러스트 판도 그 범위만 옮겼습니다. 지금은 상태와 전환, 프로퍼티를 담아 두는 그릇입니다.

- `animator.add_state(state)` 와 `add_transition(transition)` 은 목록에 넣고 번호(`usize`)를 돌려줍니다. `get_states()`, `get_transitions()` 는 조각(`&[..]`)을 돌려줍니다.
- `AnimationState::set_clip(Some(clip))` 은 상태에 클립을 답니다. `AnimationTransition` 은 `set_from(Some(state_index))`, `set_to(Some(state_index))` 로 두 상태 번호를 잇습니다.
- `set_property(name, value)`, `get_property(name)`, `get_properties()` 는 이름 붙은 값을 담습니다. 값은 `serde_json::Value` 로 바뀌는 것(숫자, 참거짓, 글)이고 없으면 `None` 입니다.
- 이 `AnimationClip` 은 3D 의 `AnimationClip` 과 이름이 같아서 `vanilla::resource::animationclip::AnimationClip` 으로 씁니다. 프레임 영역(`Rect`) 목록, 길이(초), 반복 여부를 담습니다. `AnimationClip::from_atlas(&atlas_json, &frame_ids, fps, is_loop)` 는 아틀라스 표(TexturePacker 꼴이나 배열 꼴)에서 영역을 뽑고 길이를 프레임 수 나누기 초당 프레임 수로 정합니다.
- `animator.tick(time_delta)` 는 안의 `Animation`(`get_animation()`, `get_animation_mut()`)만 갱신합니다. 전환에는 조건을 거는 자리가 없고, 상태를 스스로 바꾸지도 않습니다.

```rust
use serde_json::json;
use vanilla::resource::animationclip::AnimationClip;
use vanilla::{AnimationState, AnimationTransition, Animator};

let mut animator = Animator::new();
let idle_clip = AnimationClip::from_atlas(&json!({ "frames": { "idle0": { "frame": { "x": 0, "y": 0, "w": 200, "h": 200 } } } }), &["idle0"], 10.0, true);
let mut run_clip = AnimationClip::new();
run_clip.set_loop(true);
let mut idle_state = AnimationState::new();
idle_state.set_clip(Some(idle_clip));
let mut run_state = AnimationState::new();
run_state.set_clip(Some(run_clip));
let idle_index = animator.add_state(idle_state);
let run_index = animator.add_state(run_state);
let mut transition = AnimationTransition::new();
transition.set_from(Some(idle_index));
transition.set_to(Some(run_index));
animator.add_transition(transition);
animator.set_property("speed", 2);
animator.tick(1.0 / 60.0);

let states = animator.get_states();
assert_eq!(states.len(), 2);
assert_eq!(states[idle_index].get_clip().unwrap().get_duration(), 0.1);
assert_eq!(animator.get_transitions()[0].get_to(), Some(run_index));
assert_eq!(animator.get_property("speed").and_then(|value| value.as_i64()), Some(2));
```

캐릭터의 상태에 따라 클립을 바꾸는 일은 지금은 `FiniteStateMachine` 과 `SpriteAnimator` 를 함께 써서 합니다. 러스트 판의 상태 처리 함수는 노드 트리를 빌릴 수 없으므로, 씬의 `tick` 에서 기계를 갱신한 뒤 지금 상태 이름으로 `play_on_node` 를 부릅니다. 같은 클립이 이미 돌고 있으면 `play` 는 다시 시작하지 않으므로 매 프레임 불러도 됩니다. 처리 함수 안에서 상태를 바꿀 때는 `get_state_change_requester()` 로 받은 손잡이의 `change_state` 로 부탁합니다.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, FiniteStateMachine, ImageAsset, NodeId, Rect, Scene, SceneBase, Sprite, SpriteAnimator, SpriteAnimatorPlayOptions, StateHandlers, Vector2};

struct StateScene {
	scene_base: SceneBase,
	image_asset: ImageAsset,
	hero_id: Option<NodeId>,
	state_machine: FiniteStateMachine<&'static str>,
}

impl Scene for StateScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.image_asset.load(engine, "examples/assets/atlas/icons.png");
		self.scene_base.track_asset(&self.image_asset);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let image = match self.image_asset.get_image() {
			Some(image) => {
				image
			}
			None => {
				return;
			}
		};
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let hero_id = node_tree.create_child(root_id, "Hero");
		let hero_node = &mut node_tree[hero_id];
		hero_node.set_content_size(Vector2::create(96.0, 96.0));
		hero_node.set_local_position(Vector2::create(640.0, 360.0));
		hero_node.add_component(Sprite::new());
		let sprite_animator = hero_node.add_component(SpriteAnimator::new());
		sprite_animator.add_clip("idle", image.clone(), &[Rect::create(2.0, 2.0, 56.0, 40.0), Rect::create(63.0, 2.0, 48.0, 48.0)], 4.0, true);
		sprite_animator.add_clip("run", image, &[Rect::create(39.0, 55.0, 40.0, 36.0), Rect::create(84.0, 55.0, 30.0, 30.0), Rect::create(2.0, 47.0, 32.0, 56.0)], 10.0, true);
		self.hero_id = Some(hero_id);

		let idle_requester = self.state_machine.get_state_change_requester();
		self.state_machine.add_state("idle", StateHandlers {
			tick: Some(Box::new(move |_time_delta: f32, state_seconds: f32| {
				if state_seconds >= 2.0 {
					idle_requester.change_state("run", false);
				}
			})),
			..StateHandlers::default()
		});
		let run_requester = self.state_machine.get_state_change_requester();
		self.state_machine.add_state("run", StateHandlers {
			tick: Some(Box::new(move |_time_delta: f32, state_seconds: f32| {
				if state_seconds >= 1.0 {
					run_requester.change_state("idle", false);
				}
			})),
			..StateHandlers::default()
		});
		self.state_machine.change_state("idle", false);
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.state_machine.tick(time_delta);
		if let (Some(hero_id), Some(state_name)) = (self.hero_id, self.state_machine.get_state_name()) {
			let hero_node = &mut self.scene_base.get_node_tree_mut()[hero_id];
			SpriteAnimator::play_on_node(hero_node, state_name, SpriteAnimatorPlayOptions::default());
		}
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let state_scene = StateScene {
		scene_base: SceneBase::new(),
		image_asset: ImageAsset::new(),
		hero_id: None,
		state_machine: FiniteStateMachine::new(),
	};
	if let Err(error) = engine.run(Box::new(state_scene)) {
		eprintln!("{}", error);
	}
}
```

## Action

`Action` 은 스텝을 쌓아 두고 차례로 도는 연출 스크립트입니다(원본 `experimental/action.js`). 대상 꼴 `T` 는 기본 `WorldNode` 이고, 게임 상태나 노드 여럿을 다루려면 그것을 `T` 로 씁니다. 러스트는 대상을 붙잡아 둘 수 없어서 `start(&mut target)` 과 `step(&mut target, time_delta)` 가 대상을 받아 콜백에 넘깁니다.

- `wait(duration, ActionCallbacks::new().started(..).updated(|progress, target| ..).completed(..))`: 정한 시간(초) 동안 기다리며 진행률(0 ~ 1)을 넘깁니다. 0 이하면 첫 `step` 에서 진행률 1 로 끝납니다.
- `condition(|target| bool, ActionConditionCallbacks::new())`: 조건이 참이 될 때까지 기다립니다.
- `call(|target| ..)`: 닿자마자 부르고 같은 프레임에 다음 스텝으로 넘어갑니다.
- `repeat(times, inner_action)`, `forever(inner_action)`, `r#loop(|target| bool, inner_action)`(`loop` 는 러스트 예약어): 안쪽 액션을 반복합니다. `forever` 뒤의 스텝에는 닿지 않으므로 멈추려면 `stop()` 을 부릅니다.
- `is_running()`, `is_done()`. `step` 은 한 번에 스텝 하나만 진행하고, 끝난 스텝 뒤의 `call` 스텝은 같은 프레임에 돕니다.

```rust
use vanilla::{Action, ActionCallbacks, Vector2, WorldNode};

let mut node = WorldNode::new("Banner");
let mut action: Action<WorldNode> = Action::new()
	.wait(1.0, ActionCallbacks::new().updated(|progress, node: &mut WorldNode| node.set_local_opacity(progress)))
	.call(|node: &mut WorldNode| node.set_local_scale(Vector2::create(1.2, 1.2)))
	.repeat(2, Action::new().wait(0.5, ActionCallbacks::new().completed(|node: &mut WorldNode| node.set_local_rotation(node.get_local_rotation() + 90.0))));
action.start(&mut node);
action.step(&mut node, 0.5);
assert_eq!(node.get_local_opacity(), 0.5);
action.step(&mut node, 0.5);
assert_eq!(node.get_local_opacity(), 1.0);
assert_eq!(node.get_local_scale(), Vector2::create(1.2, 1.2));
for _ in 0..4 {
	action.step(&mut node, 0.5);
}
assert_eq!(node.get_local_rotation(), 180.0);
assert!(action.is_done());
```

## 주의할 점

- `Animation`, `Timeline`, `FiniteStateMachine`, `Action` 은 스스로 시간이 가지 않습니다. 씬의 `tick` 에서 `tick(time_delta)` 나 `step` 을 불러 줍니다. 트윈은 `scene_base.start_tween` 으로 맡기면 `SceneBase::tick` 이 갱신하므로, `Scene::tick` 을 덮어쓸 때 `self.scene_base.tick(engine, time_delta)` 를 빼면 트윈이 멈춥니다.
- 트윈과 타임라인의 콜백은 원본처럼 노드 객체를 받지 않고 `&mut NodeTree` 를 받습니다. 붙잡아 둔 `NodeId` 로 `node_tree[node_id]` 를 꺼내 씁니다. 콜백 밖의 값을 바꾸려면 `Rc<Cell>` 이나 `Rc<RefCell>` 을 잡아 넣습니다.
- 타임라인 처리기는 `Box::new` 로 감싸 `Some` 에 넣고, 위 예제처럼 클로저 인자의 꼴을 적어 줍니다.
- `SpriteAnimator` 는 같은 노드의 `Sprite` 를 찾아 씁니다. `Sprite` 가 없으면 그림이 바뀌지 않습니다. `play` 는 다음 노드 갱신에 프레임을 넣으므로, 그 자리에서 보이게 하려면 `play_on_node` 를 씁니다.
- `Timeline::seek` 는 이벤트와 마커를 내지 않습니다. 이벤트는 `tick` 이 시간을 지나갈 때만 납니다.
- `Timeline::load_from_url` 은 받기에 실패하면 JSON 해석 전에 `Err` 를 돌려줍니다. 원본은 응답 상태를 보지 않고 JSON 으로 읽어 해석 오류로 실패합니다.

## 관련 타입

Tween, TweenValues, TweenValue, TweenProperty, TweenId, easing_function, SceneBase, Frame, Animation, Sprite, SpriteAnimator, SpriteAnimatorPlayOptions, Timeline, TimelineTarget, TimelinePropertyTarget, TimelineValue, TimelineKey, TimelineMarker, TimelineEasing, TimelinePropertyDefinition, TimelineExternalProperty, TimelineNodeDescription, TimelineSound, TimelineSpriteFrameGrid, ShaderSprite, ParticleSystem, ScreenEffect, Animator, AnimationState, AnimationTransition, AnimationClip, FiniteStateMachine, StateHandlers, StateChangeRequester, Action, ActionCallbacks, ActionConditionCallbacks, Scene
