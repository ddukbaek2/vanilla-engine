//==============================================================================
// 2D 뼈대 애니메이션 초안 예제. (원본 examples/skeleton2d 와 같은 장면)
// - 저장소 자산 hero(도형으로 그린 아틀라스, 엔진 JSON, DragonBones JSON, examples/assets/skeleton2d)를 넷 세운다.
//   1. DragonBones JSON 으로 읽은 걷기. (걸음 사건을 센다)
//   2. 걷기 위에 손 흔들기를 0.7 만 얹은 층.
//   3. 대기와 걷기를 2 초마다 0.6 초 동안 섞어 넘기는 크로스페이드.
//   4. 대기 위에 앞팔 두 뼈 IK. 목표 뼈를 원을 따라 옮기고 목표에 점을 그린다.
// - 화면을 같게 찍으려고 시간은 프레임마다 1/60 초씩 흘린다. (애니메이터의 빠르기는 0, 씬이 update 를 부른다)
//   SKELETON2D_FRAME=135 면 135 프레임만 흘리고 멈춘다. (원본 ?frame=135)
// - 실행: cargo run --example skeleton2d
//   화면 저장: SKELETON2D_FRAME=135 VANILLA_CAPTURE_PATH=shots/skeleton2d_{frame}.png VANILLA_CAPTURE_FRAMES=170 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::path::PathBuf;
use std::rc::Rc;
use vanilla::core::graphic::TextAlign;
use vanilla::experimental::skeleton2d::{Skeleton2DAnimator, Skeleton2DData};
use vanilla::{Color, Engine, EngineConfiguration, Graphic, JsonAsset, NodeId, Pivot, Rect, Scene, SceneBase, SpriteAtlas, Text, Vector2};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const EXAMPLE_ASSET_DIRECTORY: &str = "examples/assets";
const VIEW_WIDTH: f32 = 960.0;
const VIEW_HEIGHT: f32 = 540.0;
const GROUND_Y: f32 = 430.0;
const CHARACTER_SCALE: f32 = 2.0;
const CHARACTER_X_LIST: [f32; 4] = [120.0, 360.0, 600.0, 840.0];
const TIME_STEP: f32 = 1.0 / 60.0;
const CROSSFADE_PERIOD_FRAMES: u64 = 120;
const CROSSFADE_MIX_SECONDS: f32 = 0.6;
const BACKGROUND_COLOR: Color = Color::create(0.11, 0.133, 0.188, 1.0);
const GROUND_COLOR: Color = Color::create(0.165, 0.192, 0.259, 1.0);
const LABEL_COLOR: Color = Color::create(0.82, 0.85, 0.9, 1.0);
const TARGET_COLOR: Color = Color::create(1.0, 0.84, 0.3, 1.0);
const LABEL_TEXT_LIST: [&str; 4] = ["DragonBones walk", "walk + wave 0.7", "idle <-> walk mix", "arm IK"];

//==============================================================================
// 2D 뼈대 예제 씬.
//==============================================================================
struct Skeleton2DScene {
	scene_base: SceneBase,
	frozen_frame: Option<u64>,
	frame_index: u64,
	step_count: u32,
	character_id_list: Vec<NodeId>,
	step_label_id: Option<NodeId>,
	sprite_atlas: SpriteAtlas,
	skeleton_json_asset: JsonAsset,
	dragon_bones_json_asset: JsonAsset,
	is_atlas_applied: bool,
}

impl Skeleton2DScene {
	//==============================================================================
	// 생성. (SKELETON2D_FRAME 이 있으면 그 프레임 수만큼만 흘린다)
	//==============================================================================
	fn new() -> Skeleton2DScene {
		let frozen_frame = std::env::var("SKELETON2D_FRAME").ok().and_then(|frame_text| frame_text.parse::<u64>().ok());
		Skeleton2DScene {
			scene_base: SceneBase::new(),
			frozen_frame,
			frame_index: 0,
			step_count: 0,
			character_id_list: Vec::new(),
			step_label_id: None,
			sprite_atlas: SpriteAtlas::new(),
			skeleton_json_asset: JsonAsset::new(),
			dragon_bones_json_asset: JsonAsset::new(),
			is_atlas_applied: false,
		}
	}

	//==============================================================================
	// 글 노드 만들기. (가운데 맞춤)
	//==============================================================================
	fn create_label(&mut self, name: &str, position: Vector2, text: &str) -> NodeId {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let label_id = node_tree.create_child(root_id, name);
		let label = &mut node_tree[label_id];
		label.set_pivot(Pivot::MIDDLE_CENTER);
		label.set_local_position(position);
		label.set_content_size(Vector2::create(220.0, 24.0));
		let label_text = label.add_component(Text::create_with_text(text, 16.0));
		label_text.set_text_color(LABEL_COLOR);
		label_text.set_text_align(TextAlign::Center);
		label_id
	}

	//==============================================================================
	// 아틀라스 조각 걸기. (아틀라스가 다 읽힌 뒤 한 번)
	//==============================================================================
	fn apply_atlas(&mut self, engine: &Engine) {
		if self.is_atlas_applied || self.character_id_list.is_empty() || !self.sprite_atlas.update(engine) {
			return;
		}
		self.is_atlas_applied = true;
		let node_tree = self.scene_base.get_node_tree_mut();
		for character_id in &self.character_id_list {
			if let Some(skeleton_animator) = node_tree[*character_id].get_component_mut::<Skeleton2DAnimator>() {
				skeleton_animator.set_atlas(&self.sprite_atlas);
			}
		}
	}

	//==============================================================================
	// IK 목표 자리. (뼈대 기준, 어깨 앞의 원)
	//==============================================================================
	fn read_target_position(frame_index: u64) -> Vector2 {
		let angle = frame_index as f32 * TIME_STEP * 2.0;
		Vector2::create(16.0 + angle.cos() * 16.0, -70.0 + angle.sin() * 16.0)
	}

	//==============================================================================
	// 한 걸음. (크로스페이드 넘기기, 시간 흘리기, 사건 세기, IK 목표 옮기기)
	//==============================================================================
	fn step(&mut self) {
		let frame_index = self.frame_index;
		let node_tree = self.scene_base.get_node_tree_mut();
		for (character_index, character_id) in self.character_id_list.iter().enumerate() {
			let Some(skeleton_animator) = node_tree[*character_id].get_component_mut::<Skeleton2DAnimator>() else {
				continue;
			};
			if character_index == 2 {
				if frame_index % (CROSSFADE_PERIOD_FRAMES * 2) == 0 {
					skeleton_animator.set_animation(0, "idle", true, if frame_index == 0 { 0.0 } else { CROSSFADE_MIX_SECONDS });
				}
				else if frame_index % (CROSSFADE_PERIOD_FRAMES * 2) == CROSSFADE_PERIOD_FRAMES {
					skeleton_animator.set_animation(0, "walk", true, CROSSFADE_MIX_SECONDS);
				}
			}
			skeleton_animator.update(TIME_STEP);
			let fired_events = skeleton_animator.take_events();
			if character_index == 0 {
				self.step_count += fired_events.iter().filter(|fired_event| fired_event.get_event().get_name() == "step").count() as u32;
			}
			if character_index == 3 {
				let target_position = Self::read_target_position(frame_index + 1);
				if let Some(target_bone) = skeleton_animator.find_bone_mut("handTarget") {
					target_bone.set_position(target_position.x, target_position.y);
				}
			}
		}
		if let Some(step_label_id) = self.step_label_id {
			if let Some(step_text) = node_tree[step_label_id].get_component_mut::<Text>() {
				step_text.set_text(&format!("steps {}", self.step_count));
			}
		}
		self.frame_index += 1;
	}
}

impl Scene for Skeleton2DScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (아틀라스와 JSON 둘을 씬이 기다리게 맡긴다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		self.sprite_atlas.load(engine, "skeleton2d/hero.atlas.json");
		self.sprite_atlas.track_json_asset(&mut self.scene_base);
		self.skeleton_json_asset.load(engine, "skeleton2d/hero.skeleton.json");
		self.scene_base.track_asset(&self.skeleton_json_asset);
		self.dragon_bones_json_asset.load(engine, "skeleton2d/hero_ske.json");
		self.scene_base.track_asset(&self.dragon_bones_json_asset);
	}

	//==============================================================================
	// 초기화. (캐릭터 넷과 이름 글)
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		let (Some(skeleton_json), Some(dragon_bones_json)) = (self.skeleton_json_asset.get_data(), self.dragon_bones_json_asset.get_data()) else {
			eprintln!("skeleton2d: 뼈대 JSON 을 읽지 못했다.");
			return;
		};
		let skeleton_data = match Skeleton2DData::create_from_json(skeleton_json) {
			Ok(skeleton_data) => {
				Rc::new(skeleton_data)
			}
			Err(error) => {
				eprintln!("{}", error);
				return;
			}
		};
		let dragon_bones_data = match Skeleton2DData::create_from_dragon_bones(dragon_bones_json, Some("hero")) {
			Ok(dragon_bones_data) => {
				Rc::new(dragon_bones_data)
			}
			Err(error) => {
				eprintln!("{}", error);
				return;
			}
		};
		let root_id = self.scene_base.get_root_id();
		for (character_index, character_x) in CHARACTER_X_LIST.iter().enumerate() {
			let node_tree = self.scene_base.get_node_tree_mut();
			let character_id = node_tree.create_child(root_id, "hero");
			let character = &mut node_tree[character_id];
			character.set_local_position(Vector2::create(*character_x, GROUND_Y));
			character.set_local_scale_xy(CHARACTER_SCALE, CHARACTER_SCALE);
			let skeleton_animator = character.add_component(Skeleton2DAnimator::new());
			skeleton_animator.set_skeleton_data(if character_index == 0 { dragon_bones_data.clone() } else { skeleton_data.clone() });
			skeleton_animator.set_time_scale(0.0);
			match character_index {
				0 => {
					skeleton_animator.set_animation(0, "walk", true, 0.0);
				}
				1 => {
					skeleton_animator.set_animation(0, "walk", true, 0.0);
					skeleton_animator.set_animation(1, "wave", true, 0.0);
					skeleton_animator.set_track_alpha(1, 0.7);
				}
				3 => {
					skeleton_animator.set_animation(0, "idle", true, 0.0);
					if let Some(ik_constraint) = skeleton_animator.get_skeleton_mut().and_then(|skeleton| skeleton.find_ik_constraint_mut("armIk")) {
						ik_constraint.set_mix(1.0);
					}
				}
				_ => {}
			}
			self.character_id_list.push(character_id);
			self.create_label("label", Vector2::create(*character_x, GROUND_Y + 40.0), LABEL_TEXT_LIST[character_index]);
			if character_index == 0 {
				self.step_label_id = Some(self.create_label("stepLabel", Vector2::create(*character_x, GROUND_Y + 66.0), "steps 0"));
			}
		}
		self.apply_atlas(engine);
	}

	//==============================================================================
	// 주기적 갱신. (컴포넌트를 돌린 뒤 한 걸음)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.apply_atlas(engine);
		if self.character_id_list.is_empty() {
			return;
		}
		let is_stepping = match self.frozen_frame {
			Some(frozen_frame) => {
				self.frame_index < frozen_frame
			}
			None => {
				true
			}
		};
		if is_stepping {
			self.step();
		}
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건 뒤 바닥)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		graphic.set_fill_color(BACKGROUND_COLOR);
		graphic.draw_rect(Rect::create(0.0, 0.0, VIEW_WIDTH, VIEW_HEIGHT));
		graphic.set_fill_color(GROUND_COLOR);
		graphic.draw_rect(Rect::create(0.0, GROUND_Y, VIEW_WIDTH, VIEW_HEIGHT - GROUND_Y));
	}

	//==============================================================================
	// 이후 출력. (IK 목표 점)
	//==============================================================================
	fn post_draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		if self.character_id_list.is_empty() {
			return;
		}
		let target_position = Self::read_target_position(self.frame_index);
		let center = Vector2::create(CHARACTER_X_LIST[3] + target_position.x * CHARACTER_SCALE, GROUND_Y + target_position.y * CHARACTER_SCALE);
		graphic.set_fill_color(TARGET_COLOR);
		graphic.draw_circle(center, 4.0);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("2D 뼈대 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(Skeleton2DScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
