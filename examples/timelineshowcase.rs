//==============================================================================
// 타임라인 쇼케이스 예제. (Timeline + GameSceneBase 눈으로 확인)
// - 원본 examples/timelineshowcase 의 문서 8 개(*.timeline.json)와 그림을 그대로 읽어 무대를 만들고 차례로 튼다.
//   애셋은 원본 저장소 폴더에서 읽기만 한다. (modelviewer 예제와 같은 방식)
// - 씬은 GameSceneBase 를 품는다: 로딩 화면, 세이프 에어리어 노드, 뷰 크기가 바뀌면 다시 놓기, 배경 칠하기.
// - "screen" 트랙은 external 대상이라 tick 뒤에 apply_external_properties 로 ScreenEffect 에 넘긴다.
// - 무대 오른쪽 아래에 HeatHazeEffect 노드(아지랑이), 머리 오른쪽에 Indicator(돌아가는 톱니)를 같이 띄운다.
// - 화면 저장이 같은 모양으로 나오게 타임라인은 프레임마다 1/60 초씩 흘린다. 150 프레임마다 다음 문서로, 누르면 바로 다음으로 넘어간다.
// - 실행: cargo run --release --example timelineshowcase
//   화면 저장: VANILLA_CAPTURE_PATH=shots/timeline_{frame}.png VANILLA_CAPTURE_FRAMES=90,240,390 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::core::graphic::TextAlign;
use vanilla::core::component::mask::Mask;
use vanilla::{Color, HeatHazeEffect, Indicator, Engine, EngineConfiguration, GameSceneBase, Graphic, Image, ImageAsset, JsonAsset, NodeId, NodeTree, Paint, Pivot, Scene, SceneBase, ScreenEffect, Text, Timeline, TimelineKey, TimelineMarker, TimelineTarget, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/timelineshowcase/assets";
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const DOCUMENT_WIDTH: f32 = 960.0;
const DOCUMENT_HEIGHT: f32 = 540.0;
const HEADER_HEIGHT: f32 = 64.0;
const MODE_FRAME_COUNT: u64 = 150;
const TIMELINE_FRAME_STEP: f32 = 1.0 / 60.0;
const IMAGE_NAME_LIST: [&str; 7] = ["emblem.png", "slime.png", "star.png", "coin.png", "bat.png", "gem.png", "cover.png"];
// (문서 파일, 제목) 목록. (원본 MODE_DEFINITIONS 차례)
const MODE_DEFINITIONS: [(&str, &str); 8] = [
	("title.timeline.json", "TITLE REVEAL"),
	("logo.timeline.json", "LOGO STING"),
	("hud.timeline.json", "HUD INTRO"),
	("cutscene.timeline.json", "CUTSCENE"),
	("combo.timeline.json", "COMBO POPUP"),
	("transition.timeline.json", "SCENE TRANSITION"),
	("idle.timeline.json", "LOOPING IDLE"),
	("parallax.timeline.json", "PARALLAX FLYBY"),
];

//==============================================================================
// 모드 하나. (무대 뿌리 노드, 타임라인, 제목)
//==============================================================================
struct TimelineMode {
	root_id: NodeId,
	timeline: Timeline,
	title: String,
}

//==============================================================================
// 타임라인 쇼케이스 씬.
//==============================================================================
struct TimelineShowcaseScene {
	game_scene_base: GameSceneBase,
	timeline_asset_list: Vec<JsonAsset>,
	image_asset_list: Vec<(String, ImageAsset)>,
	mode_list: Vec<TimelineMode>,
	current_mode_index: usize,
	mode_frame_count: u64,
	screen_effect: ScreenEffect,
	stage_id: Option<NodeId>,
	title_label_id: Option<NodeId>,
	status_label_id: Option<NodeId>,
	// 마지막 마커 / 이벤트. (핸들러가 적고 상태 줄이 읽는다)
	last_signal: Rc<RefCell<String>>,
	indicator: Indicator,
}

impl TimelineShowcaseScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> TimelineShowcaseScene {
		TimelineShowcaseScene {
			game_scene_base: GameSceneBase::new(),
			timeline_asset_list: Vec::new(),
			image_asset_list: Vec::new(),
			mode_list: Vec::new(),
			current_mode_index: 0,
			mode_frame_count: 0,
			screen_effect: ScreenEffect::new(),
			stage_id: None,
			title_label_id: None,
			status_label_id: None,
			last_signal: Rc::new(RefCell::new(String::new())),
			indicator: Indicator::new(),
		}
	}

	//==============================================================================
	// 라벨 노드 만들기. (왼쪽 정렬 글)
	//==============================================================================
	fn create_label(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, font_size: f32, color: Color) -> NodeId {
		let label_id = node_tree.create_child(parent_id, name);
		let label = &mut node_tree[label_id];
		label.set_pivot(Pivot::TOP_LEFT);
		label.set_anchor(Pivot::TOP_LEFT);
		label.set_local_position(position);
		label.set_content_size(Vector2::create(900.0, font_size * 1.4));
		let text = label.add_component(Text::create_with_text("", font_size));
		text.set_text_align(TextAlign::Left);
		text.set_text_color(color);
		label_id
	}

	//==============================================================================
	// 모드 고르기. (모두 멈추고 화면 효과를 되돌린 뒤 고른 문서를 처음부터 튼다. 원본 select_mode)
	//==============================================================================
	fn select_mode(&mut self, mode_index: usize) {
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		for mode in self.mode_list.iter_mut() {
			mode.timeline.stop(node_tree);
			node_tree[mode.root_id].set_active(false);
		}
		self.screen_effect.reset_all();
		self.current_mode_index = mode_index;
		self.mode_frame_count = 0;
		self.last_signal.borrow_mut().clear();
		if let Some(mode) = self.mode_list.get_mut(mode_index) {
			node_tree[mode.root_id].set_active(true);
			mode.timeline.seek(node_tree, 0.0);
			mode.timeline.play();
			mode.timeline.apply_external_properties("screen", &mut self.screen_effect);
			if let Some(title_label_id) = self.title_label_id {
				if let Some(text) = node_tree[title_label_id].get_component_mut::<Text>() {
					text.set_text(&format!("TIMELINE SHOWCASE  {} / {}  {}", mode_index + 1, MODE_DEFINITIONS.len(), mode.title));
				}
			}
		}
	}

	//==============================================================================
	// 무대를 세이프 에어리어 안에 맞춰 놓는다. (씬의 layout, 원본 layout 재정의 자리)
	//==============================================================================
	fn layout(&mut self) {
		let safe_area_rect = self.game_scene_base.get_safe_area_rect();
		let stage_id = match self.stage_id {
			Some(stage_id) => {
				stage_id
			}
			None => {
				return;
			}
		};
		let available_width = (safe_area_rect.size.x - 32.0).max(1.0);
		let available_height = (safe_area_rect.size.y - HEADER_HEIGHT - 16.0).max(1.0);
		let stage_scale = (available_width / DOCUMENT_WIDTH).min(available_height / DOCUMENT_HEIGHT);
		let stage_x = (safe_area_rect.size.x - DOCUMENT_WIDTH * stage_scale) * 0.5;
		let stage = &mut self.game_scene_base.get_scene_base_mut().get_node_tree_mut()[stage_id];
		stage.set_local_scale(Vector2::create(stage_scale, stage_scale));
		stage.set_local_position(Vector2::create(stage_x, HEADER_HEIGHT));
	}
}

impl Scene for TimelineShowcaseScene {
	fn get_scene_base(&self) -> &SceneBase {
		self.game_scene_base.get_scene_base()
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		self.game_scene_base.get_scene_base_mut()
	}

	//==============================================================================
	// 생성. (로딩 화면을 시작하고 문서와 그림을 요청해 맡긴다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.game_scene_base.set_view_scale_mode(ViewScaleMode::StretchShort, None);
		self.game_scene_base.set_loading_min_duration_ms(0.0);
		self.game_scene_base.set_scene_background_color(Color::create_from_hex("#14171e"));
		self.game_scene_base.create(engine);
		for (file_name, _) in MODE_DEFINITIONS.iter() {
			let mut timeline_asset = JsonAsset::new();
			timeline_asset.load(engine, file_name);
			self.game_scene_base.get_scene_base_mut().track_asset(&timeline_asset);
			self.timeline_asset_list.push(timeline_asset);
		}
		for image_name in IMAGE_NAME_LIST.iter() {
			let mut image_asset = ImageAsset::new();
			image_asset.load(engine, image_name);
			self.game_scene_base.get_scene_base_mut().track_asset(&image_asset);
			self.image_asset_list.push((image_name.to_string(), image_asset));
		}
	}

	//==============================================================================
	// 로딩 화면.
	//==============================================================================
	fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.game_scene_base.draw_on_load(engine, graphic);
	}

	//==============================================================================
	// 초기화. (머리 글, 무대, 모드마다 무대 뿌리와 타임라인)
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		self.game_scene_base.initialize(engine);
		let safe_area_id = self.game_scene_base.get_safe_area_node_id().expect("safe area node");
		let mut image_table: HashMap<String, Rc<Image>> = HashMap::new();
		for (image_name, image_asset) in self.image_asset_list.iter_mut() {
			if let Some(image) = image_asset.get_image() {
				image_table.insert(image_name.clone(), image);
			}
		}
		let description_list: Vec<Option<Value>> = self.timeline_asset_list.iter_mut().map(|timeline_asset| timeline_asset.get_data().cloned()).collect();

		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		self.title_label_id = Some(TimelineShowcaseScene::create_label(node_tree, safe_area_id, "Title", Vector2::create(24.0, 10.0), 22.0, Color::create_from_hex("#e8ebf2")));
		self.status_label_id = Some(TimelineShowcaseScene::create_label(node_tree, safe_area_id, "Status", Vector2::create(24.0, 38.0), 14.0, Color::create_from_hex("#9aa2b2")));

		// 무대. (문서 크기, 둥근 모서리로 자른다)
		let stage_id = node_tree.create_child(safe_area_id, "Stage");
		{
			let stage = &mut node_tree[stage_id];
			stage.set_pivot(Pivot::TOP_LEFT);
			stage.set_anchor(Pivot::TOP_LEFT);
			stage.set_content_size(Vector2::create(DOCUMENT_WIDTH, DOCUMENT_HEIGHT));
			stage.add_component(Paint::create_with_color(Color::create_from_hex("#0d1017")));
			let mut mask = Mask::new();
			mask.set_round_size(12.0);
			stage.add_component(mask);
		}
		self.stage_id = Some(stage_id);

		let image_resolver = |image_name: &str| -> Option<Rc<Image>> {
			image_table.get(image_name).cloned()
		};
		for ((_, title), description) in MODE_DEFINITIONS.iter().zip(description_list) {
			let description = match description {
				Some(description) => {
					description
				}
				None => {
					eprintln!("timelineshowcase: {} 문서를 읽지 못했습니다.", title);
					continue;
				}
			};
			let root_id = node_tree.create_child(stage_id, title);
			{
				let root = &mut node_tree[root_id];
				root.set_pivot(Pivot::TOP_LEFT);
				root.set_anchor(Pivot::TOP_LEFT);
				root.set_content_size(Vector2::create(DOCUMENT_WIDTH, DOCUMENT_HEIGHT));
				root.set_active(false);
			}
			let mut timeline = Timeline::new(Some(description));
			timeline.build_stage(node_tree, root_id, Some(&image_resolver));
			timeline.set_target_resolver(Some(Box::new(|target_name: &str| {
				if target_name == "screen" {
					Some(TimelineTarget::External)
				}
				else {
					None
				}
			})));
			let marker_signal = self.last_signal.clone();
			timeline.set_marker_handler(Some(Box::new(move |marker_name: &str, _marker: &TimelineMarker, _node_tree: &mut NodeTree| {
				*marker_signal.borrow_mut() = format!("marker {}", marker_name);
			})));
			let event_signal = self.last_signal.clone();
			timeline.set_event_handler(Some(Box::new(move |target_name: &str, event_name: &str, _key: &TimelineKey, _node_tree: &mut NodeTree| {
				*event_signal.borrow_mut() = format!("event {} {}", target_name, event_name);
			})));
			self.mode_list.push(TimelineMode { root_id, timeline, title: title.to_string() });
		}
		// 아지랑이. (무대의 마지막 자식이라 문서 위에 그려진다)
		let haze_id = node_tree.create_child(stage_id, "HeatHaze");
		{
			let haze = &mut node_tree[haze_id];
			haze.set_pivot(Pivot::TOP_LEFT);
			haze.set_anchor(Pivot::TOP_LEFT);
			haze.set_local_position(Vector2::create(DOCUMENT_WIDTH - 340.0, DOCUMENT_HEIGHT - 220.0));
			haze.set_content_size(Vector2::create(320.0, 200.0));
			let heat_haze_effect = haze.add_component(HeatHazeEffect::new());
			heat_haze_effect.set_amplitude(6.0);
			heat_haze_effect.set_frequency(0.08);
		}
		self.indicator.set_radius(14.0);
		self.indicator.set_color(Color::create_from_hex("#d4b06a"));
		self.select_mode(0);
	}

	//==============================================================================
	// 화면 크기 변경.
	//==============================================================================
	fn resize(&mut self, engine: &mut Engine, canvas_native_size: Vector2) {
		self.game_scene_base.resize(engine, canvas_native_size);
		self.layout();
	}

	//==============================================================================
	// 갱신. (노드 / 터치 / 레이아웃 → 타임라인 → 화면 효과, 150 프레임마다 또는 누르면 다음 문서)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, _time_delta: f32) {
		if self.game_scene_base.tick(engine, TIMELINE_FRAME_STEP) {
			self.layout();
		}
		self.mode_frame_count += 1;
		let is_touch_pressed = engine.get_input_manager().is_touch_pressed();
		if (self.mode_frame_count >= MODE_FRAME_COUNT || is_touch_pressed) && !self.mode_list.is_empty() {
			let next_mode_index = (self.current_mode_index + 1) % self.mode_list.len();
			self.select_mode(next_mode_index);
		}
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		if let Some(mode) = self.mode_list.get_mut(self.current_mode_index) {
			mode.timeline.tick(node_tree, TIMELINE_FRAME_STEP);
			mode.timeline.apply_external_properties("screen", &mut self.screen_effect);
			let status_text = format!("{:.2} / {:.2} s  {}  {}", mode.timeline.get_time(), mode.timeline.get_duration(), if mode.timeline.is_loop() { "loop" } else { "once" }, self.last_signal.borrow());
			if let Some(status_label_id) = self.status_label_id {
				if let Some(text) = node_tree[status_label_id].get_component_mut::<Text>() {
					text.set_text(&status_text);
				}
			}
		}
		self.screen_effect.tick(TIMELINE_FRAME_STEP);
		self.indicator.tick(TIMELINE_FRAME_STEP);
	}

	//==============================================================================
	// 이전 출력. (화면 효과 시작 → 배경)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.screen_effect.begin(graphic);
		self.game_scene_base.pre_draw(engine, graphic);
	}

	//==============================================================================
	// 이후 출력. (화면 효과 끝)
	//==============================================================================
	fn post_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		let view_size = engine.get_view_manager().get_view_size();
		self.indicator.set_position(view_size.x - 40.0, 36.0);
		self.indicator.draw(graphic);
		self.screen_effect.end(graphic);
	}

	//==============================================================================
	// 해제.
	//==============================================================================
	fn destroy(&mut self, _engine: &mut Engine) {
	}
}

//==============================================================================
// 진입점.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("타임라인 쇼케이스 (바닐라엔진)");
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(TimelineShowcaseScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
