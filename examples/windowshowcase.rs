//==============================================================================
// 창 쌓기와 화면 틀 예제.
// - 게임 영역 기준(set_play_area_reference): 세이프 에어리어(위 40, 아래 30 을 흉내 냄)를 뺀 게임 영역의 세로가 늘 720 이다.
// - 창 층(UIWindowLayer)에 기본 창과 확인 창(UIConfirmWindow, 긴 글은 스크롤)을 쌓는다. 맨 위 창만 보이고, 어둠이 세이프 에어리어까지 덮는다.
// - 화면 페이드(fade_screen_with_hold): 다 어두워지면 글을 바꾸고 받은 손잡이(ScreenFadeHold)를 0.6 초 붙잡았다가 놓아 밝힌다.
// - UI 가 받지 않은 누름만 놀이 입력 처리기(set_play_input_handler)로 받아 표시를 옮긴다. 단추와 창 위 누름은 놀이로 가지 않는다.
// - 씬은 GameScene 트레이트를 구현하고 game_scene! 으로 Scene 을 얻는다. (바탕의 resize, tick, post_draw 를 직접 부르지 않는다)
// - 처음 몇 초는 저절로 둘러본다: 창 둘 쌓기 → 확인 창 닫기 → 모두 닫기 → 페이드. 그 뒤로는 단추를 눌러 본다.
// - 실행: cargo run --example windowshowcase
//==============================================================================

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use vanilla::{Color, ComponentContext, Engine, EngineConfiguration, GameScene, GameSceneBase, NodeId, NodeTree, Paint, Pivot, PlayInput, SafeAreaInsets, ScreenFadeHold, Text, TextAlign, UIButton, UIConfirmWindow, UIConfirmWindowOptions, UILabel, UIWindow, UIWindowDefaultBehavior, UIWindowLayer, Vector2};

// 저절로 둘러보는 때. (초)
const TOUR_CLOSE_CONFIRM_SECONDS: f32 = 2.5;
const TOUR_CLOSE_ALL_SECONDS: f32 = 4.0;
const TOUR_FADE_SECONDS: f32 = 4.6;
// 페이드를 어둡게 붙잡는 시간. (초)
const FADE_HOLD_SECONDS: f32 = 0.6;

//==============================================================================
// 첫 씬.
//==============================================================================
struct MainScene {
	game_scene_base: GameSceneBase,
	play_area_id: Option<NodeId>,
	layer_id: Option<NodeId>,
	status_id: Option<NodeId>,
	marker_id: Option<NodeId>,
	confirm_window_id: Option<NodeId>,
	elapsed_seconds: f32,
	tour_step: u32,
	// 단추가 페이드를 부탁했는지.
	fade_requested: Rc<Cell<bool>>,
	// 다 어두워진 횟수. (on_dark 가 늘린다)
	dark_count: Rc<Cell<u32>>,
	// 다 어두워졌을 때 받은 손잡이. (on_dark 가 넣는다)
	screen_fade_hold: Rc<RefCell<Option<ScreenFadeHold>>>,
	hold_remain_seconds: f32,
	press_count: Rc<Cell<u32>>,
	// 놀이 입력 처리기가 정한 표시 자리.
	marker_position: Rc<Cell<Option<Vector2>>>,
	confirm_count: Rc<Cell<u32>>,
}

impl MainScene {
	//==============================================================================
	// 왼쪽 위 피봇의 노드 만들기.
	//==============================================================================
	fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2) -> NodeId {
		let node_id = node_tree.create_child(parent_id, name);
		let node = &mut node_tree[node_id];
		node.set_pivot(Pivot::TOP_LEFT);
		node.set_anchor(Pivot::TOP_LEFT);
		node.set_local_position(position);
		node.set_content_size(size);
		node_id
	}

	//==============================================================================
	// 단추 만들기. (둥근 Paint + UIButton + UILabel)
	//==============================================================================
	fn create_button(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, label_text: &str, position: Vector2) -> NodeId {
		let button_id = MainScene::create_box_node(node_tree, parent_id, name, position, Vector2::create(220.0, 64.0));
		node_tree[button_id].set_interactable(true);
		let paint = node_tree[button_id].add_component(Paint::create_with_color("#3a82f6"));
		paint.set_round_size(14.0);
		node_tree.add_component(button_id, UIButton::new());
		if let Some(label) = node_tree.add_component(button_id, UILabel::new()) {
			label.set_text(label_text);
			label.set_font_size(24.0);
		}
		button_id
	}

	//==============================================================================
	// 확인 창 띄우기. (긴 글, 확인하면 횟수를 센다)
	//==============================================================================
	fn open_confirm_window(node_tree: &mut NodeTree, layer_id: NodeId, confirm_count: Rc<Cell<u32>>) -> NodeId {
		let message = "아주 긴 글이 창 높이를 넘으면 글 칸만 스크롤되고 두 단추는 늘 보여요. ".repeat(30);
		let window_id = UIConfirmWindow::create(node_tree, layer_id, "confirmWindow", UIConfirmWindowOptions { title: String::from("확인 창"), message, ..UIConfirmWindowOptions::default() });
		if let Some(confirm_window) = node_tree[window_id].get_component_mut::<UIWindow>().and_then(|window| window.get_behavior_mut::<UIConfirmWindow>()) {
			confirm_window.set_confirm_event(Some(Box::new(move || {
				confirm_count.set(confirm_count.get() + 1);
			})));
		}
		UIWindowLayer::open_window_in_tree(node_tree, layer_id, window_id);
		window_id
	}

	//==============================================================================
	// 기본 창 띄우기. (판 세로만 정한 창, 안에 글 하나)
	//==============================================================================
	fn open_plain_window(node_tree: &mut NodeTree, layer_id: NodeId) -> NodeId {
		let window_id = UIWindow::create(node_tree, layer_id, "plainWindow", Box::new(UIWindowDefaultBehavior));
		let panel_id = match node_tree[window_id].get_component_mut::<UIWindow>() {
			Some(window) => {
				window.set_panel_height(200.0);
				window.get_panel_node()
			}
			None => {
				None
			}
		};
		if let Some(panel_id) = panel_id {
			let text_id = MainScene::create_box_node(node_tree, panel_id, "plainText", Vector2::create(24.0, 24.0), Vector2::create(432.0, 152.0));
			let text = node_tree[text_id].add_component(Text::create_with_text("기본 창 (바깥을 누르면 닫혀요)", 22.0));
			text.set_text_color("#f5f7ff");
		}
		UIWindowLayer::open_window_in_tree(node_tree, layer_id, window_id);
		window_id
	}

	//==============================================================================
	// 레이아웃. (게임 영역 = 세이프 에어리어, 창 층에 크기와 세이프 에어리어 너비를 알린다)
	//==============================================================================
	fn layout_content(&mut self, engine: &Engine) {
		let (play_area_id, layer_id) = match (self.play_area_id, self.layer_id) {
			(Some(play_area_id), Some(layer_id)) => {
				(play_area_id, layer_id)
			}
			_ => {
				return;
			}
		};
		let safe_area_rect = self.game_scene_base.get_safe_area_rect();
		let view_manager = engine.get_view_manager();
		let css_pixel_scale = view_manager.get_css_pixel_scale();
		let safe_area_insets = view_manager.get_safe_area_insets();
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		node_tree[play_area_id].set_content_size(safe_area_rect.size);
		node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
			window_layer.set_view_size(tree, safe_area_rect.size.x, safe_area_rect.size.y, css_pixel_scale);
			window_layer.set_view_bleed(tree, safe_area_insets.top, safe_area_insets.right, safe_area_insets.bottom, safe_area_insets.left);
		});
	}

	//==============================================================================
	// 페이드 시작. (다 어두워지면 글을 바꾸고 손잡이를 붙잡는다)
	//==============================================================================
	fn start_fade(&mut self) {
		let dark_count = self.dark_count.clone();
		let status_id = self.status_id;
		let screen_fade_hold_slot = self.screen_fade_hold.clone();
		let is_started = self.game_scene_base.fade_screen_with_hold(Box::new(move |node_tree: &mut NodeTree, _engine: &Engine, screen_fade_hold: ScreenFadeHold| {
			dark_count.set(dark_count.get() + 1);
			if let Some(status_text) = status_id.and_then(|status_id| node_tree.get_node_mut(status_id)).and_then(|status_node| status_node.get_component_mut::<Text>()) {
				status_text.set_text_color("#ffd166");
			}
			*screen_fade_hold_slot.borrow_mut() = Some(screen_fade_hold);
		}));
		if is_started {
			self.hold_remain_seconds = FADE_HOLD_SECONDS;
		}
	}

	//==============================================================================
	// 저절로 둘러보기. (창 둘 쌓기 → 확인 창 닫기 → 모두 닫기 → 페이드)
	//==============================================================================
	fn update_tour(&mut self) {
		let layer_id = match self.layer_id {
			Some(layer_id) => {
				layer_id
			}
			None => {
				return;
			}
		};
		if self.tour_step == 0 {
			self.tour_step = 1;
			let confirm_count = self.confirm_count.clone();
			let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
			MainScene::open_plain_window(node_tree, layer_id);
			self.confirm_window_id = Some(MainScene::open_confirm_window(node_tree, layer_id, confirm_count));
		}
		else if self.tour_step == 1 && self.elapsed_seconds >= TOUR_CLOSE_CONFIRM_SECONDS {
			self.tour_step = 2;
			if let Some(confirm_window_id) = self.confirm_window_id.take() {
				UIWindow::close_in_tree(self.game_scene_base.get_scene_base_mut().get_node_tree_mut(), confirm_window_id);
			}
		}
		else if self.tour_step == 2 && self.elapsed_seconds >= TOUR_CLOSE_ALL_SECONDS {
			self.tour_step = 3;
			self.game_scene_base.get_scene_base_mut().get_node_tree_mut().with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
				window_layer.close_all_windows(tree);
			});
		}
		else if self.tour_step == 3 && self.elapsed_seconds >= TOUR_FADE_SECONDS {
			self.tour_step = 4;
			self.start_fade();
		}
	}

	//==============================================================================
	// 상태 글 고치기.
	//==============================================================================
	fn update_status(&mut self) {
		let (status_id, layer_id) = match (self.status_id, self.layer_id) {
			(Some(status_id), Some(layer_id)) => {
				(status_id, layer_id)
			}
			_ => {
				return;
			}
		};
		let fade_alpha = self.game_scene_base.get_screen_fade_alpha();
		let status_line = {
			let node_tree = self.game_scene_base.get_scene_base().get_node_tree();
			let window_layer = node_tree[layer_id].get_component::<UIWindowLayer>();
			let window_count = window_layer.map(|window_layer| window_layer.get_window_count()).unwrap_or(0);
			let ui_scale = window_layer.map(|window_layer| window_layer.get_ui_scale()).unwrap_or(1.0);
			let safe_area_rect = self.game_scene_base.get_safe_area_rect();
			format!("게임 영역 {:.0} x {:.0}   창 {}   UI 배율 {:.2}   놀이 누름 {}   확인 {}   어두워짐 {}   막 {:.2}", safe_area_rect.size.x, safe_area_rect.size.y, window_count, ui_scale, self.press_count.get(), self.confirm_count.get(), self.dark_count.get(), fade_alpha)
		};
		if let Some(status_text) = self.game_scene_base.get_scene_base_mut().get_node_tree_mut()[status_id].get_component_mut::<Text>() {
			status_text.set_text(&status_line);
		}
	}
}

impl GameScene for MainScene {
	fn get_game_scene_base(&self) -> &GameSceneBase {
		&self.game_scene_base
	}

	fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
		&mut self.game_scene_base
	}

	//==============================================================================
	// 애셋 읽기 자리. (세이프 에어리어를 흉내 내고 게임 영역 기준과 놀이 입력 처리기를 정한다)
	//==============================================================================
	fn load_assets(&mut self, engine: &mut Engine) {
		engine.get_view_manager_mut().set_safe_area_override(Some(SafeAreaInsets::create(40.0, 0.0, 30.0, 0.0)));
		self.game_scene_base.set_scene_background_color(Color::create_from_hex("#4a1f1f"));
		self.game_scene_base.set_play_area_reference(720.0, false, Some(engine));
		let press_count = self.press_count.clone();
		let marker_position = self.marker_position.clone();
		self.game_scene_base.set_play_input_handler(Some(Box::new(move |play_input: PlayInput| {
			if let PlayInput::Press(play_area_position) = play_input {
				press_count.set(press_count.get() + 1);
				marker_position.set(Some(play_area_position));
			}
		})));
	}

	//==============================================================================
	// 초기화. (게임 영역, 상태 글, 표시, 단추 둘, 창 층)
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		let safe_area_id = self.game_scene_base.get_safe_area_node_id().expect("safe area node");
		let confirm_count = self.confirm_count.clone();
		let fade_requested = self.fade_requested.clone();
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		let play_area_id = MainScene::create_box_node(node_tree, safe_area_id, "playArea", Vector2::zero(), Vector2::create(1.0, 1.0));
		node_tree[play_area_id].add_component(Paint::create_with_color("#1f3a2e"));
		let status_id = MainScene::create_box_node(node_tree, play_area_id, "status", Vector2::create(24.0, 20.0), Vector2::create(1200.0, 40.0));
		let status_text = node_tree[status_id].add_component(Text::create_with_text("", 22.0));
		status_text.set_text_align(TextAlign::Left);
		status_text.set_text_color("#f5f7ff");
		let marker_id = node_tree.create_child(play_area_id, "marker");
		node_tree[marker_id].set_content_size(Vector2::create(36.0, 36.0));
		node_tree[marker_id].set_local_position(Vector2::create(640.0, 360.0));
		let marker_paint = node_tree[marker_id].add_component(Paint::create_with_color("#ffd166"));
		marker_paint.set_round_size(18.0);

		// 창 층은 게임 영역의 맨 위 자식. 단추는 그 앞에 둔다.
		let confirm_button_id = MainScene::create_button(node_tree, play_area_id, "confirmButton", "확인 창", Vector2::create(24.0, 80.0));
		let fade_button_id = MainScene::create_button(node_tree, play_area_id, "fadeButton", "페이드", Vector2::create(264.0, 80.0));
		let layer_id = UIWindowLayer::create(node_tree, play_area_id);
		if let Some(button) = node_tree[confirm_button_id].get_component_mut::<UIButton>() {
			button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				MainScene::open_confirm_window(context.get_node_tree_mut(), layer_id, confirm_count.clone());
			}));
		}
		if let Some(button) = node_tree[fade_button_id].get_component_mut::<UIButton>() {
			button.set_clicked_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
				fade_requested.set(true);
			}));
		}
		self.play_area_id = Some(play_area_id);
		self.layer_id = Some(layer_id);
		self.status_id = Some(status_id);
		self.marker_id = Some(marker_id);
		self.layout_content(engine);
	}

	//==============================================================================
	// 레이아웃. (바탕이 세이프 에어리어를 다시 계산한 뒤)
	//==============================================================================
	fn layout(&mut self, engine: &mut Engine) {
		self.layout_content(engine);
	}

	//==============================================================================
	// 갱신. (둘러보기, 놀이 입력으로 표시 옮기기, 페이드 손잡이 놓기)
	//==============================================================================
	fn tick(&mut self, _engine: &mut Engine, time_delta: f32) {
		self.elapsed_seconds += time_delta;
		self.update_tour();
		if self.fade_requested.replace(false) {
			self.start_fade();
		}
		let is_hold_finished = match self.screen_fade_hold.borrow().as_ref() {
			Some(_) => {
				self.hold_remain_seconds -= time_delta;
				self.hold_remain_seconds <= 0.0
			}
			None => {
				false
			}
		};
		if is_hold_finished {
			if let Some(screen_fade_hold) = self.screen_fade_hold.borrow_mut().take() {
				screen_fade_hold.release();
			}
		}
		if let (Some(play_area_position), Some(marker_id)) = (self.marker_position.take(), self.marker_id) {
			self.game_scene_base.get_scene_base_mut().get_node_tree_mut()[marker_id].set_local_position(play_area_position);
		}
		self.update_status();
	}
}

vanilla::game_scene!(MainScene);

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("창 쌓기와 화면 틀 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	// 로딩 화면 설정은 create 전에. (원본 생성자 자리)
	let mut game_scene_base = GameSceneBase::new();
	game_scene_base.set_loading_min_duration_ms(0.0);
	let main_scene = MainScene {
		game_scene_base,
		play_area_id: None,
		layer_id: None,
		status_id: None,
		marker_id: None,
		confirm_window_id: None,
		elapsed_seconds: 0.0,
		tour_step: 0,
		fade_requested: Rc::new(Cell::new(false)),
		dark_count: Rc::new(Cell::new(0)),
		screen_fade_hold: Rc::new(RefCell::new(None)),
		hold_remain_seconds: 0.0,
		press_count: Rc::new(Cell::new(0)),
		marker_position: Rc::new(Cell::new(None)),
		confirm_count: Rc::new(Cell::new(0)),
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
