//==============================================================================
// 게임 공통 베이스 씬. (원본 game/gamescene.js 의 GameScene)
// - 로딩 화면 (가운데 이미지 + 아래 게이지 + 터치로 건너뛰기, 최소 노출 시간).
// - 터치 입력은 TouchRecognizer 로 돌린다. (스크롤 뷰 안의 버튼을 끌어도 스크롤되는 손짓)
// - 세이프 에어리어 노드 "safeArea" + 세이프 에어리어 영역 계산.
// - 뷰 크기가 바뀌면 다음 tick 에서 layout 을 다시 부른다.
// - 캔버스 전체를 배경색으로 칠한다. (pre_draw)
// - 러스트 판은 상속 대신 씬이 GameSceneBase 를 품고 Scene 함수마다 이쪽을 부른다:
//     struct TitleScene { game_scene_base: GameSceneBase, logoAsset: ImageAsset }
//     impl Scene for TitleScene {
//         fn get_scene_base(&self) -> &SceneBase { self.game_scene_base.get_scene_base() }
//         fn get_scene_base_mut(&mut self) -> &mut SceneBase { self.game_scene_base.get_scene_base_mut() }
//         fn create(&mut self, engine: &mut Engine) {
//             self.game_scene_base.set_loading_image_url("loading.png");
//             self.game_scene_base.create(engine);                       // 원본 load + super.load_assets
//             self.logoAsset.load(engine, "logo.png");                 // 원본 load_assets 재정의 자리
//             self.game_scene_base.get_scene_base_mut().track_asset(&self.logoAsset);
//         }
//         fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) { self.game_scene_base.draw_on_load(engine, graphic); }
//         fn initialize(&mut self, engine: &mut Engine) {
//             self.game_scene_base.initialize(engine);
//             let safe_area_id = self.game_scene_base.get_safe_area_node_id().unwrap();   // 콘텐츠는 여기 아래에 붙인다
//         }
//         fn resize(&mut self, engine: &mut Engine, canvas_native_size: Vector2) { self.game_scene_base.resize(engine, canvas_native_size); self.layout(engine); }
//         fn tick(&mut self, engine: &mut Engine, time_delta: f32) { if self.game_scene_base.tick(engine, time_delta) { self.layout(engine); } }
//         fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) { self.game_scene_base.pre_draw(engine, graphic); }
//     }
//   씬의 layout 은 원본 layout 재정의 자리다. (game_scene_base.get_safe_area_rect() 를 읽어 콘텐츠를 놓는다)
// - 원본과 다른 점:
//   - 원본 load(비동기)의 "load_assets 를 기다린 뒤 최소 시간 / 터치까지 기다림"은 러스트 애셋 흐름으로 옮겼다.
//     create 에서 "로딩 문" 읽기 손잡이 하나를 맡겨 두고, draw_on_load 가 최소 시간이 지나거나 화면을 누르면 문을 연다.
//     엔진은 맡긴 애셋이 모두(문 포함) 끝나야 initialize 를 부르므로 결과는 원본과 같다. (애셋 다 읽힘 + (최소 시간 또는 터치))
//   - 시간은 원본 Date.now() 대신 엔진 시작 뒤 실제 경과 시간(get_realtime_since_startup)으로 잰다.
//   - 화면 누름은 원본 window pointerdown 대신 입력 매니저의 is_touch_pressed 로 본다.
//   - 원본은 TouchRecognizer 를 initialize 에서 만든다. 러스트 판은 생성할 때 만든다. 터치는 원본 touch_press 같은 함수 대신 tick 이 넘긴다.
//     (SceneBase 의 터치 레이캐스터와 첫 터치 콜백은 쓰지 않는다)
//   - tick 과 resize 는 원본처럼 layout 을 부르되, 씬의 layout 은 부를 수 없어서 bool(tick) 로 알리거나 씬이 resize 뒤에 직접 부른다.
//   - 세이프 에어리어: 웹은 원본처럼 CSS env(safe-area-inset-*) 를 읽는다. 데스크탑에는 그런 값이 없어서 0 이고,
//     set_safe_area_insets 로 정해 줄 수 있다. (정하면 웹에서도 그 값을 쓴다)
//   - set_view_scale_mode 는 엔진을 같이 받는다. (전역 엔진이 없다) 엔진을 주면 원본처럼 바로 뷰 영역을 다시 계산한다.
//   - 개발자 도구(DEVTools)는 옮기지 않았다. is_dev_tools_capturing_input 은 늘 false 이고, post_draw 의 도구 그리기는 없다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::NodeId;
use crate::core::scene::SceneBase;
use crate::core::viewmanager::ViewScaleMode;
use crate::resource::imageasset::ImageAsset;
use crate::ui::touchrecognizer::TouchRecognizer;

// 로딩 화면 최소 노출 시간 기본값. (밀리초)
const DEFAULT_LOAD_MIN_DURATION_MS: f32 = 3000.0;

// 로딩 게이지 높이. (뷰 좌표)
const LOADING_BAR_HEIGHT: f32 = 24.0;

// 로딩 문 손잡이 경로. (애셋이 아닌 대기 표시)
const LOADING_GATE_PATH: &str = "GameSceneBase.loadingGate";

//==============================================================================
// 게임 공통 베이스 씬.
//==============================================================================
pub struct GameSceneBase {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	safe_area_node_id: Option<NodeId>,
	last_safe_area_rect: Rect,
	last_view_size_x: f32,
	last_view_size_y: f32,
	loading_image_asset: Option<ImageAsset>,
	loading_image_url: String,
	// 로딩 시작 시각. (엔진 시작 뒤 초)
	load_start_time: f64,
	load_min_duration_ms: f32,
	load_touched_to_skip: bool,
	load_gate_handle: Option<AssetLoadHandle>,
	view_scale_mode: ViewScaleMode,
	// 세이프 에어리어 인셋. (CSS 픽셀, 위 / 오른쪽 / 아래 / 왼쪽. 없으면 플랫폼에서 읽는다)
	safe_area_insets: Option<[f32; 4]>,
}

impl GameSceneBase {
	//==============================================================================
	// 생성. (최소 노출 3 초, 검은 배경, ViewScaleMode::None)
	//==============================================================================
	pub fn new() -> GameSceneBase {
		let mut scene_base = SceneBase::new();
		scene_base.set_scene_background_color(Color::black());
		GameSceneBase {
			scene_base,
			touch_recognizer: TouchRecognizer::new(),
			safe_area_node_id: None,
			last_safe_area_rect: Rect::zero(),
			last_view_size_x: 0.0,
			last_view_size_y: 0.0,
			loading_image_asset: None,
			loading_image_url: String::new(),
			load_start_time: 0.0,
			load_min_duration_ms: DEFAULT_LOAD_MIN_DURATION_MS,
			load_touched_to_skip: false,
			load_gate_handle: None,
			view_scale_mode: ViewScaleMode::None,
			safe_area_insets: None,
		}
	}

	//==============================================================================
	// 씬 공통 상태 반환. (Scene::get_scene_base 에서 돌려준다)
	//==============================================================================
	pub fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	//==============================================================================
	// 씬 공통 상태 반환. (변경용)
	//==============================================================================
	pub fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 터치 인식기 반환.
	//==============================================================================
	pub fn get_touch_recognizer(&self) -> &TouchRecognizer {
		&self.touch_recognizer
	}

	//==============================================================================
	// 터치 인식기 반환. (변경용)
	//==============================================================================
	pub fn get_touch_recognizer_mut(&mut self) -> &mut TouchRecognizer {
		&mut self.touch_recognizer
	}

	//==============================================================================
	// ViewScaleMode 설정. (기본 none. 엔진을 주면 바로 뷰 영역에 반영하고, 아니면 initialize 에서 적용한다)
	//==============================================================================
	pub fn set_view_scale_mode(&mut self, view_scale_mode: ViewScaleMode, engine: Option<&mut Engine>) {
		self.view_scale_mode = view_scale_mode;
		if let Some(engine) = engine {
			self.apply_view_scale_mode(engine);
		}
	}

	//==============================================================================
	// 지금 ViewScaleMode 반환.
	//==============================================================================
	pub fn get_view_scale_mode(&self) -> ViewScaleMode {
		self.view_scale_mode
	}

	//==============================================================================
	// 뷰 매니저에 ViewScaleMode 반영. (바뀌었으면 지금 캔버스 크기로 뷰 영역을 다시 계산한다. 원본 view_manager.set_view_scale_mode)
	//==============================================================================
	fn apply_view_scale_mode(&self, engine: &mut Engine) {
		let view_manager = engine.get_view_manager_mut();
		if view_manager.get_view_scale_mode() == self.view_scale_mode {
			return;
		}
		view_manager.set_view_scale_mode(self.view_scale_mode);
		let canvas_pixel_size = view_manager.get_canvas_pixel_size();
		if canvas_pixel_size.x > 0.0 && canvas_pixel_size.y > 0.0 {
			let device_pixel_ratio = view_manager.get_device_pixel_ratio();
			view_manager.calculate_view_rect(canvas_pixel_size.x as u32, canvas_pixel_size.y as u32, device_pixel_ratio);
		}
	}

	//==============================================================================
	// 로딩 화면 가운데 이미지 경로 설정. (빈 글이면 이미지 없이 게이지만. create 전에 정한다)
	//==============================================================================
	pub fn set_loading_image_url(&mut self, url: &str) {
		self.loading_image_url = url.to_string();
	}

	//==============================================================================
	// 로딩 화면 최소 노출 시간 설정. (밀리초. create 전에 정한다)
	//==============================================================================
	pub fn set_loading_min_duration_ms(&mut self, milliseconds: f32) {
		self.load_min_duration_ms = milliseconds;
	}

	//==============================================================================
	// 캔버스 배경색 설정. (pre_draw 에서 캔버스 전체에 칠한다)
	//==============================================================================
	pub fn set_scene_background_color(&mut self, color: impl Into<Color>) {
		self.scene_base.set_scene_background_color(color);
	}

	//==============================================================================
	// 세이프 에어리어 노드 반환. (씬이 콘텐츠를 여기에 붙인다. initialize 전이면 None)
	//==============================================================================
	pub fn get_safe_area_node_id(&self) -> Option<NodeId> {
		self.safe_area_node_id
	}

	//==============================================================================
	// 마지막으로 계산한 세이프 에어리어 영역 반환. (뷰 좌표)
	//==============================================================================
	pub fn get_safe_area_rect(&self) -> Rect {
		self.last_safe_area_rect
	}

	//==============================================================================
	// 세이프 에어리어 인셋 직접 설정. (CSS 픽셀: 위, 오른쪽, 아래, 왼쪽. None 이면 플랫폼에서 읽는다)
	//==============================================================================
	pub fn set_safe_area_insets(&mut self, insets: Option<[f32; 4]>) {
		self.safe_area_insets = insets;
	}

	//==============================================================================
	// 생성. (씬 create 에서 맨 먼저 부른다. 원본 load 의 앞부분 + load_assets)
	//==============================================================================
	pub fn create(&mut self, engine: &mut Engine) {
		let current_time = engine.get_time_manager().get_realtime_since_startup();
		self.begin_loading(current_time);
		self.load_assets(engine);
	}

	//==============================================================================
	// 로딩 시작. (시작 시각을 적고, 최소 시간 / 터치를 기다리는 로딩 문을 씬에 맡긴다)
	//==============================================================================
	pub fn begin_loading(&mut self, current_time: f64) {
		self.load_start_time = current_time;
		self.load_touched_to_skip = false;
		let load_gate_handle = AssetLoadHandle::create_pending(LOADING_GATE_PATH);
		self.scene_base.track_load_handle(&load_gate_handle);
		if self.load_min_duration_ms <= 0.0 {
			load_gate_handle.finish(Ok(Vec::new()));
		}
		self.load_gate_handle = Some(load_gate_handle);
	}

	//==============================================================================
	// 자산 읽기. (원본 load_assets 의 기본 부분: 로딩 이미지. 씬의 자체 애셋은 씬 create 에서 요청하고 track_asset 으로 맡긴다)
	//==============================================================================
	pub fn load_assets(&mut self, engine: &Engine) {
		if self.loading_image_url.is_empty() {
			return;
		}
		let mut loading_image_asset = ImageAsset::new();
		loading_image_asset.load(engine, &self.loading_image_url);
		self.scene_base.track_asset(&loading_image_asset);
		self.loading_image_asset = Some(loading_image_asset);
	}

	//==============================================================================
	// 로딩 갱신. (누르면 건너뛰기로 적고, 최소 시간이 지났거나 건너뛰었으면 로딩 문을 연다)
	//==============================================================================
	pub fn update_loading(&mut self, current_time: f64, is_touch_pressed: bool) {
		if is_touch_pressed {
			self.load_touched_to_skip = true;
		}
		let load_gate_handle = match self.load_gate_handle.as_ref() {
			Some(load_gate_handle) => {
				load_gate_handle
			}
			None => {
				return;
			}
		};
		if load_gate_handle.is_finished() {
			return;
		}
		let elapsed_milliseconds = (current_time - self.load_start_time) * 1000.0;
		if elapsed_milliseconds >= self.load_min_duration_ms as f64 || self.load_touched_to_skip {
			load_gate_handle.finish(Ok(Vec::new()));
		}
	}

	//==============================================================================
	// 로딩 게이지 진행률. (경과 시간 / 최소 노출 시간, 0 ~ 1)
	//==============================================================================
	pub fn get_loading_progress(&self, current_time: f64) -> f32 {
		if self.load_min_duration_ms <= 0.0 {
			return 1.0;
		}
		let elapsed_milliseconds = ((current_time - self.load_start_time) * 1000.0) as f32;
		(elapsed_milliseconds / self.load_min_duration_ms).clamp(0.0, 1.0)
	}

	//==============================================================================
	// 건너뛰기 눌렸는지.
	//==============================================================================
	pub fn is_load_touched_to_skip(&self) -> bool {
		self.load_touched_to_skip
	}

	//==============================================================================
	// 로딩 화면 출력. (엔진이 initialize 전까지 매 프레임 부른다. 검은 배경 + 가운데 로딩 이미지 + 아래 게이지)
	//==============================================================================
	pub fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		let current_time = engine.get_time_manager().get_realtime_since_startup();
		let is_touch_pressed = engine.get_input_manager().is_touch_pressed();
		self.update_loading(current_time, is_touch_pressed);

		let view_manager = engine.get_view_manager();
		let canvas_native_size = view_manager.get_canvas_native_size();
		let view_size = view_manager.get_view_size();

		// 캔버스 전체 검은색.
		view_manager.apply_canvas_native_rect(graphic);
		graphic.set_fill_color(Color::black());
		graphic.draw_rect(Rect::create(0.0, 0.0, canvas_native_size.x, canvas_native_size.y));

		// 뷰 좌표계 적용.
		view_manager.apply_view_rect(graphic);
		graphic.set_fill_color(Color::black());
		graphic.draw_rect(Rect::create(0.0, 0.0, view_size.x, view_size.y));

		// 가운데 로딩 이미지. (가로 70%, 비율 유지)
		let loading_image = match self.loading_image_asset.as_mut() {
			Some(loading_image_asset) => {
				loading_image_asset.get_image()
			}
			None => {
				None
			}
		};
		if let Some(image) = loading_image {
			let target_width = view_size.x * 0.7;
			let ratio = image.get_height() as f32 / (image.get_width() as f32).max(1.0);
			let target_height = target_width * ratio;
			let position_x = (view_size.x - target_width) * 0.5;
			let position_y = (view_size.y - target_height) * 0.5;
			graphic.draw_image(&image, Vector2::create(position_x, position_y), Vector2::create(target_width, target_height));
		}

		// 아래 로딩 게이지.
		let progress = self.get_loading_progress(current_time);
		let bar_width = view_size.x * 0.6;
		let bar_x = (view_size.x - bar_width) * 0.5;
		let bar_y = view_size.y * 0.78;
		graphic.set_fill_color(Color::create_from_hex("#333333"));
		graphic.draw_rect(Rect::create(bar_x, bar_y, bar_width, LOADING_BAR_HEIGHT));
		graphic.set_fill_color(Color::create_from_hex("#ffffff"));
		graphic.draw_rect(Rect::create(bar_x, bar_y, bar_width * progress, LOADING_BAR_HEIGHT));
	}

	//==============================================================================
	// 초기화. (씬 initialize 에서 맨 먼저 부른다. ViewScaleMode 적용, 세이프 에어리어 노드 생성)
	//==============================================================================
	pub fn initialize(&mut self, engine: &mut Engine) {
		self.apply_view_scale_mode(engine);
		self.create_safe_area_node();
	}

	//==============================================================================
	// 세이프 에어리어 노드 생성. (루트 아래 "safeArea", 피봇 / 앵커 왼쪽 위)
	//==============================================================================
	fn create_safe_area_node(&mut self) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let root_id = node_tree.get_root_id();
		let safe_area_node_id = node_tree.create_child(root_id, "safeArea");
		node_tree[safe_area_node_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[safe_area_node_id].set_anchor(Pivot::TOP_LEFT);
		self.safe_area_node_id = Some(safe_area_node_id);
		self.last_safe_area_rect = Rect::zero();
		self.last_view_size_x = 0.0;
		self.last_view_size_y = 0.0;
	}

	//==============================================================================
	// 화면 크기 변경됨. (기본 레이아웃을 다시 한다. 씬은 이 뒤에 자기 layout 을 부른다)
	//==============================================================================
	pub fn resize(&mut self, engine: &Engine, _canvas_native_size: Vector2) {
		self.layout(engine);
	}

	//==============================================================================
	// 레이아웃. (세이프 에어리어 영역을 다시 계산해 노드에 건다)
	//==============================================================================
	pub fn layout(&mut self, engine: &Engine) {
		let safe_area_rect = self.compute_safe_area_rect(engine);
		self.apply_safe_area_rect(safe_area_rect);
	}

	//==============================================================================
	// 세이프 에어리어 영역을 노드에 건다. (노드가 아직 없으면 영역만 적는다)
	//==============================================================================
	fn apply_safe_area_rect(&mut self, safe_area_rect: Rect) {
		self.last_safe_area_rect = safe_area_rect;
		if let Some(safe_area_node_id) = self.safe_area_node_id {
			if let Some(safe_area_node) = self.scene_base.get_node_tree_mut().get_node_mut(safe_area_node_id) {
				safe_area_node.set_local_position(safe_area_rect.position);
				safe_area_node.set_content_size(safe_area_rect.size);
			}
		}
	}

	//==============================================================================
	// 세이프 에어리어 계산. (뷰 좌표)
	//==============================================================================
	pub fn compute_safe_area_rect(&self, engine: &Engine) -> Rect {
		let view_manager = engine.get_view_manager();
		let insets = match self.safe_area_insets {
			Some(insets) => {
				insets
			}
			None => {
				read_platform_safe_area_insets()
			}
		};
		GameSceneBase::calculate_safe_area_rect(view_manager.get_view_size(), view_manager.get_target_resolution_scale(), insets)
	}

	//==============================================================================
	// 세이프 에어리어 계산. (정적. CSS 픽셀 인셋 [위, 오른쪽, 아래, 왼쪽] 을 뷰 배율로 나눠 뷰 영역에서 뺀다)
	//==============================================================================
	pub fn calculate_safe_area_rect(view_size: Vector2, target_resolution_scale: f32, insets: [f32; 4]) -> Rect {
		let scale = if target_resolution_scale > 0.0 {
			target_resolution_scale
		}
		else {
			1.0
		};
		let inset_top = insets[0] / scale;
		let inset_right = insets[1] / scale;
		let inset_bottom = insets[2] / scale;
		let inset_left = insets[3] / scale;
		let width = (view_size.x - inset_left - inset_right).max(0.0);
		let height = (view_size.y - inset_top - inset_bottom).max(0.0);
		Rect::create(inset_left, inset_top, width, height)
	}

	//==============================================================================
	// 뷰 크기가 바뀌었으면 기본 레이아웃을 다시 한다. (바뀌었으면 true)
	//==============================================================================
	pub fn update_layout_if_view_size_changed(&mut self, engine: &Engine) -> bool {
		let view_size = engine.get_view_manager().get_view_size();
		if view_size.x == self.last_view_size_x && view_size.y == self.last_view_size_y {
			return false;
		}
		self.last_view_size_x = view_size.x;
		self.last_view_size_y = view_size.y;
		self.layout(engine);
		true
	}

	//==============================================================================
	// 갱신. (노드 → TouchRecognizer 터치 → 트윈, 그다음 뷰 크기 변화를 보고 레이아웃. 레이아웃을 다시 했으면 true)
	// - 모바일 주소창이 나타나고 사라질 때처럼 resize 만으로는 놓칠 수 있어 매 tick 확인한다. (원본과 같음)
	//==============================================================================
	pub fn tick(&mut self, engine: &Engine, time_delta: f32) -> bool {
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
		self.scene_base.tick_tweens(time_delta);
		self.update_layout_if_view_size_changed(engine)
	}

	//==============================================================================
	// 개발자 도구가 입력을 가져가는 중인지. (DEVTools 를 옮기지 않아 늘 false)
	//==============================================================================
	pub fn is_dev_tools_capturing_input(&self) -> bool {
		false
	}

	//==============================================================================
	// 이전 출력. (캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 건다. 세이프 에어리어 바깥 포함)
	//==============================================================================
	pub fn pre_draw(&self, engine: &Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

impl Default for GameSceneBase {
	fn default() -> GameSceneBase {
		GameSceneBase::new()
	}
}

//==============================================================================
// 글 앞의 정수 읽기. (원본 parseInt(글) || 0. "34px" → 34)
//==============================================================================
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parse_leading_integer(text: &str) -> f32 {
	let trimmed_text = text.trim_start();
	let (sign, unsigned_text) = match trimmed_text.strip_prefix('-') {
		Some(rest) => {
			(-1.0, rest)
		}
		None => {
			(1.0, trimmed_text.strip_prefix('+').unwrap_or(trimmed_text))
		}
	};
	let digit_text: String = unsigned_text.chars().take_while(|character| character.is_ascii_digit()).collect();
	match digit_text.parse::<f32>() {
		Ok(value) => {
			sign * value
		}
		Err(_) => {
			0.0
		}
	}
}

//==============================================================================
// 플랫폼의 세이프 에어리어 인셋 읽기. (웹: CSS env(safe-area-inset-*), 위 / 오른쪽 / 아래 / 왼쪽 CSS 픽셀)
// - 원본처럼 보이지 않는 div 에 패딩으로 걸고 계산된 스타일을 읽은 뒤 뗀다.
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_platform_safe_area_insets() -> [f32; 4] {
	use wasm_bindgen::JsCast;
	let read_insets = || -> Option<[f32; 4]> {
		let window = web_sys::window()?;
		let document = window.document()?;
		let body = document.body()?;
		let element: web_sys::HtmlElement = document.create_element("div").ok()?.dyn_into().ok()?;
		let style = element.style();
		style.set_property("position", "absolute").ok()?;
		style.set_property("visibility", "hidden").ok()?;
		style.set_property("padding-top", "env(safe-area-inset-top)").ok()?;
		style.set_property("padding-right", "env(safe-area-inset-right)").ok()?;
		style.set_property("padding-bottom", "env(safe-area-inset-bottom)").ok()?;
		style.set_property("padding-left", "env(safe-area-inset-left)").ok()?;
		body.insert_adjacent_element("beforeend", &element).ok()?;
		let insets = match window.get_computed_style(&element) {
			Ok(Some(computed_style)) => {
				let read_padding = |property_name: &str| parse_leading_integer(&computed_style.get_property_value(property_name).unwrap_or_default());
				Some([read_padding("padding-top"), read_padding("padding-right"), read_padding("padding-bottom"), read_padding("padding-left")])
			}
			_ => {
				None
			}
		};
		element.remove();
		insets
	};
	read_insets().unwrap_or([0.0, 0.0, 0.0, 0.0])
}

//==============================================================================
// 플랫폼의 세이프 에어리어 인셋 읽기. (데스크탑에는 그런 값이 없어 0)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_platform_safe_area_insets() -> [f32; 4] {
	[0.0, 0.0, 0.0, 0.0]
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;

	//==============================================================================
	// 로딩 문은 최소 노출 시간이 지나야 열리고, 게이지는 경과 시간 비율이다. 다른 애셋이 남아 있으면 씬은 계속 기다린다.
	//==============================================================================
	#[test]
	fn loading_gate_waits_for_minimum_duration() {
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_loading_min_duration_ms(3000.0);
		let other_asset_handle = AssetLoadHandle::create_pending("other.png");
		game_scene_base.begin_loading(10.0);
		game_scene_base.get_scene_base_mut().track_load_handle(&other_asset_handle);
		assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());
		assert_eq!(game_scene_base.get_loading_progress(10.0), 0.0);
		assert_eq!(game_scene_base.get_loading_progress(11.5), 0.5);
		assert_eq!(game_scene_base.get_loading_progress(20.0), 1.0);

		game_scene_base.update_loading(12.9, false);
		assert_eq!(game_scene_base.get_scene_base().get_tracked_asset_progress(), 0.0);
		game_scene_base.update_loading(13.0, false);
		assert_eq!(game_scene_base.get_scene_base().get_tracked_asset_progress(), 0.5);
		assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());
		other_asset_handle.finish(Ok(vec![1, 2, 3]));
		assert!(game_scene_base.get_scene_base().are_tracked_assets_finished());
	}

	//==============================================================================
	// 화면을 누르면 최소 시간 전이라도 로딩 문이 열린다. 최소 시간이 0 이면 처음부터 열려 있다.
	//==============================================================================
	#[test]
	fn touch_skips_loading_and_zero_duration_opens_immediately() {
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.begin_loading(0.0);
		game_scene_base.update_loading(0.1, false);
		assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());
		game_scene_base.update_loading(0.2, true);
		assert!(game_scene_base.is_load_touched_to_skip());
		assert!(game_scene_base.get_scene_base().are_tracked_assets_finished());

		let mut immediate_scene_base = GameSceneBase::new();
		immediate_scene_base.set_loading_min_duration_ms(0.0);
		immediate_scene_base.begin_loading(5.0);
		assert!(immediate_scene_base.get_scene_base().are_tracked_assets_finished());
		assert_eq!(immediate_scene_base.get_loading_progress(5.0), 1.0);
	}

	//==============================================================================
	// 뷰 크기가 바뀌면 tick 이 세이프 에어리어를 다시 계산해 노드에 걸고 true 를 돌려준다. (인셋은 뷰 배율로 나눈다)
	//==============================================================================
	#[test]
	fn layout_follows_view_size_changes() {
		let mut engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_safe_area_insets(Some([10.0, 0.0, 20.0, 5.0]));
		game_scene_base.initialize(&mut engine);
		assert_eq!(engine.get_view_manager().get_view_scale_mode(), ViewScaleMode::None);
		assert_eq!(engine.get_view_manager().get_view_size(), Vector2::create(800.0, 600.0));
		let safe_area_node_id = game_scene_base.get_safe_area_node_id().expect("safe area node");
		assert_eq!(game_scene_base.get_scene_base().get_node_tree()[safe_area_node_id].get_name(), "safeArea");

		assert!(game_scene_base.tick(&engine, 0.016));
		assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 795.0, 570.0));
		let safe_area_node = &game_scene_base.get_scene_base().get_node_tree()[safe_area_node_id];
		assert_eq!(safe_area_node.get_local_position(), Vector2::create(5.0, 10.0));
		assert_eq!(safe_area_node.get_content_size(), Vector2::create(795.0, 570.0));
		assert!(!game_scene_base.tick(&engine, 0.016));

		engine.get_view_manager_mut().calculate_view_rect(1024, 768, 1.0);
		assert!(game_scene_base.tick(&engine, 0.016));
		assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 1019.0, 738.0));
		assert!(!game_scene_base.tick(&engine, 0.016));

		game_scene_base.set_view_scale_mode(ViewScaleMode::ReferenceResolution, Some(&mut engine));
		assert_eq!(engine.get_view_manager().get_view_size(), Vector2::create(1280.0, 720.0));
		assert!(game_scene_base.tick(&engine, 0.016));
		assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 1275.0, 690.0));
	}

	//==============================================================================
	// 세이프 에어리어 계산은 뷰 배율로 인셋을 나누고 음수 크기를 0 으로 막는다. CSS 패딩 글은 앞 정수만 읽는다.
	//==============================================================================
	#[test]
	fn safe_area_calculation() {
		let safe_area_rect = GameSceneBase::calculate_safe_area_rect(Vector2::create(400.0, 300.0), 2.0, [40.0, 10.0, 20.0, 30.0]);
		assert_eq!(safe_area_rect, Rect::create(15.0, 20.0, 380.0, 270.0));
		let collapsed_rect = GameSceneBase::calculate_safe_area_rect(Vector2::create(10.0, 10.0), 0.0, [20.0, 20.0, 20.0, 20.0]);
		assert_eq!(collapsed_rect, Rect::create(20.0, 20.0, 0.0, 0.0));
		assert_eq!(parse_leading_integer("34px"), 34.0);
		assert_eq!(parse_leading_integer("12.7px"), 12.0);
		assert_eq!(parse_leading_integer(""), 0.0);
		assert_eq!(parse_leading_integer("-3px"), -3.0);
	}
}
