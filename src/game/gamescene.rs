//==============================================================================
//! 게임 공통 베이스 씬. (원본 game/gamescene.js 의 GameScene)
//! - 로딩 화면 (가운데 이미지 + 아래 게이지 + 터치로 건너뛰기, 최소 노출 시간).
//! - 터치 입력은 TouchRecognizer 로 돌린다. (스크롤 뷰 안의 버튼을 끌어도 스크롤되는 손짓)
//! - 세이프 에어리어 노드 "safeArea" + 세이프 에어리어 영역 계산.
//! - 뷰 크기가 바뀌면 다음 tick 에서 layout 을 다시 부른다.
//! - 캔버스 전체를 배경색으로 칠한다. (pre_draw)
//! - 러스트 판은 상속 대신 씬이 GameSceneBase 를 품고 GameScene 트레이트(원본에서 덮어쓰는 자리)를 구현한 뒤 game_scene! 한 줄로 Scene 을 얻는다:
//!   ```text
//!     struct TitleScene { game_scene_base: GameSceneBase, logo_asset: ImageAsset }
//!     impl GameScene for TitleScene {
//!         fn get_game_scene_base(&self) -> &GameSceneBase { &self.game_scene_base }
//!         fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase { &mut self.game_scene_base }
//!         fn load_assets(&mut self) {                       // 원본 loadAssets 재정의 자리 (바탕의 로딩 이미지 요청 뒤)
//!             self.logo_asset.load(engine, "logo.png");
//!             self.game_scene_base.get_scene_base_mut().track_asset(&self.logo_asset);
//!         }
//!         fn initialize(&mut self, engine: &Engine) {                          // 바탕 initialize 뒤
//!             let safe_area_id = self.game_scene_base.get_safe_area_node_id().unwrap();   // 콘텐츠는 여기 아래에 붙인다
//!         }
//!         fn layout(&mut self) { /* game_scene_base.get_safe_area_rect() 를 읽어 콘텐츠를 놓는다 */ }
//!     }
//!     vanilla::game_scene!(TitleScene);
//!   ```
//!   Scene 함수마다 바탕을 먼저 부르고 GameScene 의 같은 이름 함수를 부른다. (원본에서 super 를 먼저 부르는 덮어쓰기)
//!   post_draw 만 씬의 것을 먼저 부르고 바탕(페이드 막, 개발자 도구)을 위에 그린다. layout 은 바탕이 세이프 에어리어를 다시 계산한 뒤 부른다.
//!   (크기 바뀜, 그리고 tick 이 뷰 크기 변화를 볼 때. 첫 tick 에도 한 번)
//!   GameSceneBase 함수를 Scene 함수에서 직접 부르는 길도 그대로 된다. 그때는 tick 이 돌려준 bool 을 보고 씬이 자기 layout 을 부르고, post_draw 도 직접 부른다.
//! - 원본과 다른 점:
//!   - 원본 load(비동기)의 "load_assets 를 기다린 뒤 최소 시간 / 터치까지 기다림"은 러스트 애셋 흐름으로 옮겼다.
//!     load 에서 "로딩 문" 읽기 손잡이 하나를 맡겨 두고(SceneBase 가 든다), 엔진이 로딩 중 프레임마다 최소 시간이 지나거나 화면을 누르면 문을 연다.
//!     (원본 load 가 거는 엔진 프레임 함수와 같은 자리, 덮어쓸 수 있는 draw_on_load 에 기대지 않아 씬이 바탕 draw_on_load 를 부르지 않아도 멈추지 않는다)
//!     엔진은 맡긴 애셋이 모두(문 포함) 끝나야 initialize 를 부르므로 결과는 원본과 같다. (애셋 다 읽힘 + (최소 시간 또는 터치))
//! - 두 엔진이 같게 하는 것:
//!   - 로딩 시간은 엔진 시계(get_realtime_since_startup)로 잰다. 엔진 시계가 아직 돌지 않았으면(첫 프레임 전, 0) 첫 로딩 화면 프레임의 시각부터 잰다.
//!   - 로딩 화면 누름은 엔진 입력(is_touch_pressed)으로 엔진이 로딩 중 프레임마다 본다.
//!   - TouchRecognizer 는 원본처럼 initialize 에서 만든다. (그 전에는 get_touch_recognizer 가 None, 원본 undefined)
//!     터치는 원본 touchPress 같은 함수 대신 tick 이 넘긴다. (SceneBase 의 터치 레이캐스터와 첫 터치 콜백은 쓰지 않는다)
//! - 그 밖에 원본과 다른 점:
//!   - 세이프 에어리어: 웹은 원본처럼 CSS env(safe-area-inset-*) 를 읽는다. 데스크탑은 크롬 데스크탑처럼 0 이고,
//!     set_safe_area_insets 로 정해 줄 수 있다. (정하면 웹에서도 그 값을 쓴다)
//!   - set_view_scale_mode 는 엔진을 같이 받는다. (뷰 매니저를 &mut 로 고쳐서 전역 엔진(&Engine)으로는 안 된다) 엔진을 주면 원본처럼 바로 뷰 영역을 다시 계산한다.
//!   - 개발자 도구(DevTools)는 EngineConfiguration::use_dev_tools 가 켜져 있으면 initialize 에서 만든다. (원본과 같음) tick 이 F2 와 터치를 넘기고 post_draw 가 그린다.
//! - 로딩 화면 바꾸기: set_loading_colors 로 색만, set_loading_drawer 로 바탕 위 그림 전체를 바꾼다. (원본 setLoadingColors, setLoadingDrawer)
//!   건 함수는 `Rc<RefCell>` 에 담는다. get_loading_drawer 는 그 Rc 를 돌려주고, 부르는 쪽은 borrow_mut 로 빌려 부른다.
//!   게이지는 기본으로 최소 노출 시간의 경과(get_loading_progress)를 보이고, set_loading_progress 나 track_asset_loader 로 진행률을 알리면
//!   그 진행률을 부드럽게 따라간다. (LoadingProgress, get_loading_progress_state)
//!   - 원본 getLoadingProgress(LoadingProgress 를 돌려줌)는 러스트 판에 이미 같은 이름(경과 시간 비율)이 있어 get_loading_progress_state 다.
//!   - 원본은 load_assets 가 끝나면(알린 적이 있을 때) 진행률을 1 로 채운다. 러스트 판은 씬에 맡긴 애셋 가운데 로딩 문을 뺀 것이 다 끝나면
//!     draw_on_load 가 1 로 채운다. (complete_loading_progress_if_assets_finished, 프레임에 걸쳐 일하는 씬은 읽기 손잡이를 맡겨 두고 끝나면 연다)
//!   - track_asset_loader 로 맡긴 AssetLoader 는 draw_on_load 가 프레임마다 update 를 불러 준다.
//!   - 원본 setLoadingColors 의 넷째 값(바깥 색)은 러스트에 기본값 인자가 없어 set_loading_outside_color 로 따로 정한다.
//! - 화면 틀 (vanilla.js GameScene 의 fadeScreen, isUserInterfaceAt, setPlayInputHandler, setPlayAreaReference 와 같은 기능):
//!   - 화면 페이드: 원본은 onDark 가 Promise 를 돌려주면 끝날 때까지 어둡게 둔다. 러스트 판의 같은 꼴은 fade_screen_async(on_dark) 다.
//!     다 어두워진 그 tick 에서 on_dark 의 퓨처를 씬 작업으로 맡겨 원본처럼 그 자리에서 첫 await 까지 돌리고(SceneContext 로 씬과 엔진에 닿는다),
//!     퓨처가 끝나면 밝아진다. GameScene 을 구현한 씬은 GameScene::fade_screen_async 로 부르고 game_scene! 이 시작을 맡는다.
//!     Scene 을 직접 구현한 씬은 `GameSceneBase::fade_screen_async::<씬 타입>` 으로 부르고, 바탕 tick 뒤에 take_screen_fade_dark_starter 로 꺼내 start 한다.
//!     (꺼내지 않으면 바탕이 post_draw 나 다음 tick 에서 다음 폴링 차례로 맡긴다)
//!     동기 꼴 fade_screen(on_dark) 은 다 어두워지면 on_dark(트리, 엔진)를 부르고, on_dark 가 ScreenFadeHold 를 돌려주면 그 손잡이의 release 가 불릴 때까지 어둡게 둔다.
//!     (None 이면 바로 밝힌다) 콜백 꼴 fade_screen_with_hold(on_dark) 는 on_dark 가 손잡이를 받아 두었다가 일이 끝나면 release 한다. (Promise 의 resolve 자리)
//!     막은 post_draw 가 그린다. 막이 걸린 동안은 누름과 휠을 받지 않는다.
//!   - UI 가 받지 않은 입력: 원본처럼 처리기(set_play_input_handler)를 부른다. 원본 처리기 객체의 press, move, release, wheel 은 PlayInput 의 갈래다.
//!     처리기는 씬을 빌릴 수 없어서 씬 상태는 `Rc<RefCell>` 로 나눠 쥔다. 씬이 tick 에서 직접 꺼내 쓰려면 set_play_input_enabled(true) 로 모아 take_play_inputs 로 꺼낸다.
//!     (좌표는 세이프 에어리어 왼쪽 위 기준)
//!   - 휠: 원본처럼 창 층(UIWindowLayer) 안을 가리키면 맨 위의 창이 먼저 받고(UIWindowLayer::scroll_window_at), 아니면 그 자리의 스크롤 칸이 받는다.
//!   - 세이프 에어리어는 set_safe_area_insets 로 정하지 않았으면 ViewManager::get_safe_area_insets 를 읽는다. (강제값, 주소, env, 무시할 쪽을 따른다)
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::scene::{Scene, SceneBase, SceneId};
use crate::core::task::{self, SceneContext};
use crate::core::viewmanager::{SafeAreaInsets, ViewScaleMode};
use crate::misc::devtools::DevTools;
use crate::misc::loadingprogress::LoadingProgress;
use crate::resource::assetloader::AssetLoader;
use crate::resource::imageasset::ImageAsset;
use crate::ui::touchrecognizer::TouchRecognizer;
use crate::ui::uiwindowlayer::UIWindowLayer;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;

/// 로딩 화면 최소 노출 시간 기본값. (밀리초)
const DEFAULT_LOAD_MIN_DURATION_MS: f64 = 3000.0;

/// 로딩 게이지 높이. (뷰 좌표)
const LOADING_BAR_HEIGHT: f64 = 24.0;


/// 로딩 화면 그리기 함수. (graphic, 뷰 크기, 보이는 진행률 0 ~ 1, 로딩을 시작한 뒤의 초. 뷰 좌표계가 걸려 있다)
pub type LoadingDrawer = Box<dyn FnMut(&mut Graphic, Vector2, f64, f64)>;
/// 화면 페이드의 어두워지는 시간과 밝아지는 시간 기본값. (초)
const DEFAULT_SCREEN_FADE_SECONDS: f64 = 0.35;

/// 화면 페이드가 한 프레임에 쓰는 가장 긴 시간. (초)
const SCREEN_FADE_MAXIMUM_STEP_SECONDS: f64 = 0.1;

//==============================================================================
/// 화면 페이드를 어둡게 붙잡는 손잡이. (원본 onDark 가 돌려주는 Promise 자리, 복제한 손잡이 어느 것이든 release 하면 밝아진다)
///
/// # 예제
/// 다 어두워지면 손잡이를 돌려주고, 놓을 때까지 어둡게 둔다 (페이드 시간 0 이면 한 tick 에 다 어두워진다)
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, NodeTree, ScreenFadeHold};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
/// let mut game_scene_base = GameSceneBase::new();
/// game_scene_base.set_screen_fade_seconds(0.0);
/// let screen_fade_hold = ScreenFadeHold::new();
/// let returned_hold = screen_fade_hold.clone();
/// assert!(game_scene_base.fade_screen(Some(Box::new(move |_node_tree: &mut NodeTree, _engine: &Engine| {
/// 	Some(returned_hold)
/// }))));
/// game_scene_base.tick(0.016);
/// game_scene_base.tick(0.016);
/// assert_eq!(game_scene_base.get_screen_fade_alpha(), 1.0);
/// screen_fade_hold.release();
/// assert!(screen_fade_hold.is_released());
/// game_scene_base.tick(0.016);
/// game_scene_base.tick(0.016);
/// assert_eq!(game_scene_base.get_screen_fade_alpha(), 0.0);
/// assert!(!game_scene_base.is_screen_fading());
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ScreenFadeHold {
	is_released: Rc<Cell<bool>>,
}

impl ScreenFadeHold {
	//==============================================================================
	/// 생성. (아직 놓지 않음)
	///
	/// # 돌려줌
	/// 놓지 않은 손잡이.
	//==============================================================================
	pub fn new() -> ScreenFadeHold {
		ScreenFadeHold {
			is_released: Rc::new(Cell::new(false)),
		}
	}

	//==============================================================================
	/// 놓기. (막이 밝아진다)
	//==============================================================================
	pub fn release(&self) {
		self.is_released.set(true);
	}

	//==============================================================================
	/// 놓았는지.
	///
	/// # 돌려줌
	/// 이 손잡이나 복제한 손잡이 가운데 하나라도 release 했으면 true.
	//==============================================================================
	pub fn is_released(&self) -> bool {
		self.is_released.get()
	}
}

//==============================================================================
/// 버릴 때 손잡이를 놓는 값. (fade_screen_async 의 작업이 끝나거나 버려지면 막이 밝아지게)
//==============================================================================
struct ScreenFadeHoldRelease {
	screen_fade_hold: ScreenFadeHold,
}

impl Drop for ScreenFadeHoldRelease {
	//==============================================================================
	/// 놓기.
	//==============================================================================
	fn drop(&mut self) {
		self.screen_fade_hold.release();
	}
}

//==============================================================================
/// 화면 페이드 작업을 시작하는 자리. (다 어두워진 tick 의 씬과 엔진에서 바로, 또는 다음 폴링 차례로)
//==============================================================================
enum ScreenFadeDarkStartPlace<'a> {
	Now(&'a Engine, &'a mut dyn Scene),
	Deferred(&'a Engine, SceneId),
}

//==============================================================================
/// 화면 페이드의 async on_dark 를 시작하는 것. (fade_screen_async 가 만들고, 다 어두워지면 씬이 take_screen_fade_dark_starter 로 꺼내 start 한다)
/// - Scene 을 직접 구현한 씬이 바탕 tick 뒤에 꺼내 start 한다. (game_scene!, ui_scene! 은 저절로 한다)
///
/// # 예제
/// 바탕 tick 뒤에 꺼내 시작하고, 퓨처가 끝나면 밝아진다
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, Scene, SceneBase, SceneContext};
///
/// struct PuzzleScene {
/// 	game_scene_base: GameSceneBase,
/// 	stage_number: u32,
/// }
///
/// impl Scene for PuzzleScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		self.game_scene_base.get_scene_base()
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		self.game_scene_base.get_scene_base_mut()
/// 	}
///
/// 	fn tick(&mut self, time_delta: f64) {
/// 		self.game_scene_base.tick(time_delta);
/// 		if let Some(screen_fade_dark_starter) = self.game_scene_base.take_screen_fade_dark_starter() {
/// 			screen_fade_dark_starter.start(self);
/// 		}
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
/// let mut puzzle_scene = PuzzleScene {
/// 	game_scene_base: GameSceneBase::new(),
/// 	stage_number: 1,
/// };
/// puzzle_scene.game_scene_base.set_screen_fade_seconds(0.0);
/// assert!(puzzle_scene.game_scene_base.fade_screen_async::<PuzzleScene, _, _>(|mut scene_context: SceneContext<PuzzleScene>| async move {
/// 	scene_context.with(|scene, _engine| {
/// 		scene.stage_number += 1;
/// 	});
/// }));
/// puzzle_scene.tick(0.016);
/// assert_eq!(puzzle_scene.stage_number, 2);
/// puzzle_scene.tick(0.016);
/// puzzle_scene.tick(0.016);
/// assert!(!puzzle_scene.game_scene_base.is_screen_fading());
/// ```
//==============================================================================
pub struct ScreenFadeDarkStarter {
	start_function: Box<dyn for<'a> FnOnce(ScreenFadeDarkStartPlace<'a>, ScreenFadeHold)>,
	screen_fade_hold: ScreenFadeHold,
}

impl ScreenFadeDarkStarter {
	//==============================================================================
	/// 시작. (on_dark 의 퓨처를 이 씬의 작업으로 맡겨 그 자리에서 첫 await 까지 돌린다. 씬이 fade_screen_async 의 타입이 아니면 바로 밝아진다)
	///
	/// # 인자
	/// - `scene`: 이 바탕을 든 씬. (보통 tick 안의 self, fade_screen_async 의 S 타입이어야 한다. 작업은 전역 엔진에 맡긴다)
	//==============================================================================
	pub fn start(self, scene: &mut dyn Scene) {
		let engine = crate::core::globalengine::expect_global_engine("ScreenFadeDarkStarter::start");
		(self.start_function)(ScreenFadeDarkStartPlace::Now(engine, scene), self.screen_fade_hold);
	}

	//==============================================================================
	/// 다음 폴링 차례로 시작. (씬이 꺼내 가지 않았을 때 바탕이 부른다)
	//==============================================================================
	fn start_deferred(self, engine: &Engine, scene_id: SceneId) {
		(self.start_function)(ScreenFadeDarkStartPlace::Deferred(engine, scene_id), self.screen_fade_hold);
	}
}

//==============================================================================
/// 다 어두워졌을 때 부르는 함수. (트리와 엔진을 받는다. 손잡이를 돌려주면 놓을 때까지 어둡게 둔다)
//==============================================================================
pub type ScreenFadeEvent = Box<dyn FnOnce(&mut NodeTree, &Engine) -> Option<ScreenFadeHold>>;

//==============================================================================
/// 다 어두워졌을 때 부르는 함수의 콜백 꼴. (트리, 엔진, 손잡이를 받는다. 손잡이를 놓을 때까지 어둡게 둔다, fade_screen_with_hold)
//==============================================================================
pub type ScreenFadeHoldEvent = Box<dyn FnOnce(&mut NodeTree, &Engine, ScreenFadeHold)>;

//==============================================================================
/// UI 가 받지 않은 입력의 처리기. (원본 setPlayInputHandler 의 처리기 객체, 좌표는 게임 영역 좌표)
//==============================================================================
pub type PlayInputHandler = Box<dyn FnMut(PlayInput)>;

//==============================================================================
/// UI 가 받지 않은 입력. (게임 영역 좌표: 세이프 에어리어 왼쪽 위 기준, 원본 처리기의 press, move, release, wheel)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayInput {
	/// UI 가 받지 않은 누름.
	Press(Vector2),
	/// 놀이로 간 누름의 이동.
	Move(Vector2),
	/// 놀이로 간 누름의 뗌. (취소 포함)
	Release(Vector2),
	/// UI 밖의 휠. (자리, 휠 값)
	Wheel(Vector2, Vector2),
}

//==============================================================================
/// 게임 공통 베이스 씬.
/// - 로딩 화면, 터치 인식, 세이프 에어리어, 화면 페이드, 놀이 입력을 맡는다. 씬이 품고 GameScene 트레이트와 game_scene! 으로 쓰거나, Scene 함수에서 직접 부른다.
///
/// # 예제
/// Scene 을 직접 구현하고 바탕 함수를 부르는 길 (tick 이 돌려준 bool 로 씬의 layout 을 부른다)
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, Graphic, Scene, SceneBase, Vector2};
///
/// struct PuzzleScene {
/// 	game_scene_base: GameSceneBase,
/// 	layout_count: u32,
/// }
///
/// impl PuzzleScene {
/// 	fn layout(&mut self) {
/// 		self.layout_count += 1;
/// 	}
/// }
///
/// impl Scene for PuzzleScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		self.game_scene_base.get_scene_base()
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		self.game_scene_base.get_scene_base_mut()
/// 	}
///
/// 	fn load(&mut self, engine: &Engine) {
/// 		self.game_scene_base.load(engine);
/// 	}
///
/// 	fn draw_on_load(&mut self, graphic: &mut Graphic) {
/// 		self.game_scene_base.draw_on_load(graphic);
/// 	}
///
/// 	fn initialize(&mut self, engine: &Engine) {
/// 		self.game_scene_base.initialize(engine);
/// 	}
///
/// 	fn resize(&mut self, canvas_native_size: Vector2) {
/// 		self.game_scene_base.resize(canvas_native_size);
/// 		self.layout();
/// 	}
///
/// 	fn tick(&mut self, time_delta: f64) {
/// 		if self.game_scene_base.tick(time_delta) {
/// 			self.layout();
/// 		}
/// 		if let Some(screen_fade_dark_starter) = self.game_scene_base.take_screen_fade_dark_starter() {
/// 			screen_fade_dark_starter.start(self);
/// 		}
/// 	}
///
/// 	fn pre_draw(&mut self, graphic: &mut Graphic) {
/// 		self.game_scene_base.pre_draw(graphic);
/// 	}
///
/// 	fn draw(&mut self, graphic: &mut Graphic) {
/// 		self.game_scene_base.get_scene_base_mut().draw(graphic);
/// 	}
///
/// 	fn post_draw(&mut self, graphic: &mut Graphic) {
/// 		self.game_scene_base.post_draw(graphic);
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
/// let mut puzzle_scene = PuzzleScene {
/// 	game_scene_base: GameSceneBase::new(),
/// 	layout_count: 0,
/// };
/// puzzle_scene.initialize(engine);
/// puzzle_scene.tick(0.016);
/// assert_eq!(puzzle_scene.layout_count, 1);
/// assert!(puzzle_scene.game_scene_base.get_safe_area_node_id().is_some());
/// assert_eq!(puzzle_scene.game_scene_base.get_safe_area_rect().size, Vector2::create(800.0, 600.0));
/// ```
//==============================================================================
pub struct GameSceneBase {
	scene_base: SceneBase,
	touch_recognizer: Option<TouchRecognizer>,
	safe_area_node_id: Option<NodeId>,
	last_safe_area_rect: Rect,
	last_view_size_x: f64,
	last_view_size_y: f64,
	loading_image_asset: Option<ImageAsset>,
	loading_image_url: String,
	/// 로딩 최소 노출 시간. (밀리초, 로딩 문의 시작 시각과 건너뛰기는 SceneBase 의 로딩 문이 든다)
	load_min_duration_ms: f64,
	view_scale_mode: ViewScaleMode,
	/// 세이프 에어리어 인셋. (CSS 픽셀, 위 / 오른쪽 / 아래 / 왼쪽. 없으면 뷰 매니저에서 읽는다)
	safe_area_insets: Option<SafeAreaInsets>,
	loading_drawer: Option<Rc<RefCell<LoadingDrawer>>>,
	loading_background_color: Color,
	loading_bar_back_color: Color,
	loading_bar_fill_color: Color,
	/// 뷰 바깥(캔버스 나머지)의 색.
	loading_outside_color: Color,
	loading_progress: LoadingProgress,
	/// 진행률을 알린 적이 있는지. (없으면 게이지는 경과 시간)
	is_loading_progress_reported: bool,
	tracked_asset_loader_list: Vec<AssetLoader>,
	/// 어두워지는 막의 진하기. (0 ~ 1)
	screen_fade_alpha: f64,
	/// 1 이면 어두워지는 중, -1 이면 밝아지는 중, 0 이면 멈춤.
	screen_fade_direction: i32,
	screen_fade_event: Option<ScreenFadeEvent>,
	/// 다 어두워진 뒤 붙잡고 있는 손잡이.
	screen_fade_hold: Option<ScreenFadeHold>,
	/// fade_screen_async 의 시작. (다 어두워지면 꺼낼 수 있다)
	screen_fade_dark_starter: Option<ScreenFadeDarkStarter>,
	is_screen_fade_dark_starter_ready: bool,
	screen_fade_color: Color,
	screen_fade_seconds: f64,
	is_play_input_enabled: bool,
	play_inputs: Vec<PlayInput>,
	play_input_handler: Option<PlayInputHandler>,
	/// 지금 누름이 놀이로 갔는지.
	is_play_press_active: bool,
	/// 게임 영역의 고정 쪽 길이. (0 이면 쓰지 않음)
	play_area_fixed_length: f64,
	is_play_area_landscape: bool,
	/// initialize 에서 세이프 에어리어 노드를 만드는지. (UISceneBase 는 노드 대신 가이드 UINode 를 쓴다)
	is_safe_area_node_enabled: bool,
	/// 개발자 도구. (EngineConfiguration::use_dev_tools 가 꺼져 있으면 None)
	dev_tools: Option<DevTools>,
}

impl GameSceneBase {
	//==============================================================================
	/// 생성. (최소 노출 3 초, 검은 배경, ViewScaleMode::None)
	/// - 로딩 색은 검정, #333333, #ffffff, 페이드는 검정 0.35 초다. 세이프 에어리어 노드는 initialize 에서 만든다.
	///
	/// # 돌려줌
	/// initialize 전의 바탕.
	//==============================================================================
	pub fn new() -> GameSceneBase {
		let mut scene_base = SceneBase::new();
		scene_base.set_scene_background_color(Color::black());
		GameSceneBase {
			scene_base,
			touch_recognizer: None,
			safe_area_node_id: None,
			last_safe_area_rect: Rect::zero(),
			last_view_size_x: 0.0,
			last_view_size_y: 0.0,
			loading_image_asset: None,
			loading_image_url: String::new(),
			load_min_duration_ms: DEFAULT_LOAD_MIN_DURATION_MS,
			view_scale_mode: ViewScaleMode::None,
			safe_area_insets: None,
			loading_drawer: None,
			loading_background_color: Color::black(),
			loading_bar_back_color: Color::create_from_hex("#333333"),
			loading_bar_fill_color: Color::create_from_hex("#ffffff"),
			loading_outside_color: Color::black(),
			loading_progress: LoadingProgress::new(),
			is_loading_progress_reported: false,
			tracked_asset_loader_list: Vec::new(),
			screen_fade_alpha: 0.0,
			screen_fade_direction: 0,
			screen_fade_event: None,
			screen_fade_hold: None,
			screen_fade_dark_starter: None,
			is_screen_fade_dark_starter_ready: false,
			screen_fade_color: Color::black(),
			screen_fade_seconds: DEFAULT_SCREEN_FADE_SECONDS,
			is_play_input_enabled: false,
			play_inputs: Vec::new(),
			play_input_handler: None,
			is_play_press_active: false,
			play_area_fixed_length: 0.0,
			is_play_area_landscape: false,
			is_safe_area_node_enabled: true,
			dev_tools: None,
		}
	}

	//==============================================================================
	/// 개발자 도구 반환. (initialize 전이거나 EngineConfiguration::use_dev_tools 가 꺼져 있으면 None)
	///
	/// # 돌려줌
	/// 개발자 도구. initialize 전이거나 꺼져 있으면 None.
	//==============================================================================
	pub fn get_dev_tools(&self) -> Option<&DevTools> {
		self.dev_tools.as_ref()
	}

	//==============================================================================
	/// 개발자 도구 반환. (변경용)
	///
	/// # 돌려줌
	/// 개발자 도구. (변경용) initialize 전이거나 꺼져 있으면 None.
	//==============================================================================
	pub fn get_dev_tools_mut(&mut self) -> Option<&mut DevTools> {
		self.dev_tools.as_mut()
	}

	//==============================================================================
	/// initialize 에서 세이프 에어리어 노드를 만들지 설정. (UISceneBase 가 끈다)
	//==============================================================================
	pub(crate) fn set_safe_area_node_enabled(&mut self, is_safe_area_node_enabled: bool) {
		self.is_safe_area_node_enabled = is_safe_area_node_enabled;
	}

	//==============================================================================
	/// 씬 공통 상태 반환. (Scene::get_scene_base 에서 돌려준다)
	///
	/// # 돌려줌
	/// 바탕이 품은 씬 공통 상태.
	//==============================================================================
	pub fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	//==============================================================================
	/// 씬 공통 상태 반환. (변경용)
	///
	/// # 돌려줌
	/// 바탕이 품은 씬 공통 상태. (변경용)
	//==============================================================================
	pub fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	/// 터치 인식기 반환. (원본 getTouchRaycaster, initialize 에서 만든다)
	///
	/// # 돌려줌
	/// tick 이 터치를 넘기는 인식기. initialize 전에는 None. (원본 undefined)
	//==============================================================================
	pub fn get_touch_recognizer(&self) -> Option<&TouchRecognizer> {
		self.touch_recognizer.as_ref()
	}

	//==============================================================================
	/// 터치 인식기 반환. (변경용)
	///
	/// # 돌려줌
	/// tick 이 터치를 넘기는 인식기. (변경용) initialize 전에는 None.
	//==============================================================================
	pub fn get_touch_recognizer_mut(&mut self) -> Option<&mut TouchRecognizer> {
		self.touch_recognizer.as_mut()
	}

	//==============================================================================
	/// ViewScaleMode 설정. (기본 none. 엔진을 주면 바로 뷰 영역에 반영하고, 아니면 initialize 에서 적용한다)
	///
	/// # 인자
	/// - `view_scale_mode`: 뷰 배율 방식.
	/// - `engine`: 바로 반영할 엔진. None 이면 initialize 에서 적용한다.
	///
	/// # 예제
	/// 엔진을 주면 그 자리에서 뷰 영역을 다시 셈한다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, Vector2, ViewScaleMode};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_view_scale_mode(ViewScaleMode::ReferenceResolution, Some(engine));
	/// assert_eq!(game_scene_base.get_view_scale_mode(), ViewScaleMode::ReferenceResolution);
	/// assert_eq!(engine.get_view_manager().get_view_size(), Vector2::create(1280.0, 720.0));
	/// ```
	//==============================================================================
	pub fn set_view_scale_mode(&mut self, view_scale_mode: ViewScaleMode, engine: Option<&Engine>) {
		self.view_scale_mode = view_scale_mode;
		if let Some(engine) = engine {
			self.apply_view_scale_mode(engine);
		}
	}

	//==============================================================================
	/// 지금 ViewScaleMode 반환.
	///
	/// # 돌려줌
	/// 이 바탕에 정한 뷰 배율 방식. (기본 ViewScaleMode::None)
	//==============================================================================
	pub fn get_view_scale_mode(&self) -> ViewScaleMode {
		self.view_scale_mode
	}

	//==============================================================================
	/// 뷰 매니저에 ViewScaleMode 반영. (바뀌었으면 지금 캔버스 크기로 뷰 영역을 다시 계산한다. 원본 viewManager.set_view_scale_mode)
	//==============================================================================
	fn apply_view_scale_mode(&self, engine: &Engine) {
		let mut view_manager = engine.get_view_manager_mut();
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
	/// 로딩 화면 가운데 이미지 경로 설정. (빈 글이면 이미지 없이 게이지만. create 전에 정한다)
	///
	/// # 인자
	/// - `url`: 로딩 이미지의 애셋 경로. (create 의 load_assets 가 읽어 씬에 맡긴다)
	//==============================================================================
	pub fn set_loading_image_url(&mut self, url: &str) {
		self.loading_image_url = url.to_string();
	}

	//==============================================================================
	/// 로딩 화면 최소 노출 시간 설정. (밀리초. create 전에 정한다)
	///
	/// # 인자
	/// - `milliseconds`: 최소 노출 시간. (밀리초, 기본 3000, 0 이하이면 기다리지 않는다)
	//==============================================================================
	pub fn set_loading_min_duration_ms(&mut self, milliseconds: f64) {
		self.load_min_duration_ms = milliseconds;
		self.scene_base.set_loading_gate_min_duration_ms(milliseconds);
	}

	//==============================================================================
	/// 로딩 화면 그리기 함수 설정. (None 이면 기본 그림: 가운데 로딩 이미지 + 아래 게이지)
	/// - 캔버스를 바깥 색으로, 뷰를 바탕색으로 칠하고 뷰 좌표계를 건 뒤 drawer(graphic, 뷰 크기, 보이는 진행률, 경과 초) 를 부른다.
	/// - 그림과 글꼴을 받기 전에도 그려지므로 도형과 기본 글꼴로 그린다. 그릴 것은 함수가 스스로 들고 있는다.
	///
	/// # 인자
	/// - `loading_drawer`: 로딩 화면 그리기 함수. (graphic, 뷰 크기, 보이는 진행률 0 ~ 1, 경과 초) None 이면 기본 그림.
	///
	/// # 예제
	/// 아래쪽 가는 막대 하나로 바꾸기
	/// ```rust
	/// use vanilla::{Colors, GameSceneBase, Graphic, Rect, Vector2};
	///
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_loading_drawer(Some(Box::new(|graphic: &mut Graphic, view_size: Vector2, progress: f64, _elapsed_seconds: f64| {
	/// 	graphic.set_fill_color(Colors::WHITE);
	/// 	graphic.draw_rect(Rect::create(0.0, view_size.y - 8.0, view_size.x * progress, 8.0));
	/// })));
	/// assert!(game_scene_base.has_loading_drawer());
	/// ```
	//==============================================================================
	pub fn set_loading_drawer(&mut self, loading_drawer: Option<LoadingDrawer>) {
		self.loading_drawer = loading_drawer.map(|loading_drawer| Rc::new(RefCell::new(loading_drawer)));
	}

	//==============================================================================
	/// 로딩 화면 그리기 함수가 있는지.
	///
	/// # 돌려줌
	/// 그리기 함수를 걸었으면 true.
	//==============================================================================
	pub fn has_loading_drawer(&self) -> bool {
		self.loading_drawer.is_some()
	}

	//==============================================================================
	/// 로딩 화면 그리기 함수 반환. (없으면 None, 부를 때는 borrow_mut 로 빌린다)
	///
	/// # 돌려줌
	/// 그리기 함수를 담은 `Rc<RefCell>`. 없으면 None.
	//==============================================================================
	pub fn get_loading_drawer(&self) -> Option<Rc<RefCell<LoadingDrawer>>> {
		self.loading_drawer.clone()
	}

	//==============================================================================
	/// 로딩 화면 색 설정. (바탕, 게이지 바탕, 게이지. 뷰 바깥도 바탕색이 된다. 기본은 검정, #333333, #ffffff)
	///
	/// # 인자
	/// - `background_color`: 뷰 바탕색이자 뷰 바깥 색. (Color 나 "#rrggbb" 같은 글)
	/// - `bar_back_color`: 게이지 바탕색.
	/// - `bar_fill_color`: 게이지 채움색.
	//==============================================================================
	pub fn set_loading_colors(&mut self, background_color: impl Into<Color>, bar_back_color: impl Into<Color>, bar_fill_color: impl Into<Color>) {
		let background_color = background_color.into();
		self.loading_background_color = background_color;
		self.loading_bar_back_color = bar_back_color.into();
		self.loading_bar_fill_color = bar_fill_color.into();
		self.loading_outside_color = background_color;
	}

	//==============================================================================
	/// 로딩 화면 뷰 바깥(캔버스 나머지)의 색 설정. (원본 setLoadingColors 의 넷째 값, set_loading_colors 뒤에 부른다)
	///
	/// # 인자
	/// - `outside_color`: 뷰 바깥 색. (Color 나 "#rrggbb" 같은 글)
	//==============================================================================
	pub fn set_loading_outside_color(&mut self, outside_color: impl Into<Color>) {
		self.loading_outside_color = outside_color.into();
	}

	//==============================================================================
	/// 로딩 진행률 알리기. (0 ~ 1, 줄어들지 않는다. 알린 뒤로 게이지가 경과 시간 대신 이 값을 따라간다)
	///
	/// # 인자
	/// - `progress`: 알릴 진행률. (0 ~ 1)
	//==============================================================================
	pub fn set_loading_progress(&mut self, progress: f64) {
		self.is_loading_progress_reported = true;
		self.loading_progress.set_progress(progress);
	}

	//==============================================================================
	/// 로딩 진행률 상태. (알린 정도와 부드럽게 따라가는 보이는 정도, 원본 getLoadingProgress)
	///
	/// # 돌려줌
	/// 로딩 진행률 상태.
	//==============================================================================
	pub fn get_loading_progress_state(&self) -> &LoadingProgress {
		&self.loading_progress
	}

	//==============================================================================
	/// 로딩 진행률 상태. (변경용, 따라가는 빠르기를 바꿀 때)
	///
	/// # 돌려줌
	/// 로딩 진행률 상태. (변경용)
	//==============================================================================
	pub fn get_loading_progress_state_mut(&mut self) -> &mut LoadingProgress {
		&mut self.loading_progress
	}

	//==============================================================================
	/// 애셋 함께 받기 맡기기. (다 받을 때까지 initialize 를 미루고, draw_on_load 가 update 를 부르고 진행률을 알린다)
	/// - 맡긴 뒤로 게이지는 경과 시간 대신 받은 진행률을 따라간다.
	///
	/// # 인자
	/// - `asset_loader`: 맡길 받기. (나눠 든다, load 는 부르는 쪽이 한다)
	///
	/// # 예제
	/// 맡긴 받기의 끝난 수로 진행률이 오른다
	/// ```rust
	/// use vanilla::{AssetLoadHandle, AssetLoader, Engine, EngineConfiguration, GameSceneBase};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_loading_min_duration_ms(0.0);
	/// game_scene_base.begin_loading(0.0);
	/// let asset_loader = AssetLoader::new();
	/// asset_loader.add_image("examples/assets/atlas/icons.png");
	/// let generate_handle = AssetLoadHandle::create_pending("level.generate");
	/// asset_loader.add_load_handle(&generate_handle);
	/// game_scene_base.track_asset_loader(&asset_loader);
	/// asset_loader.load(&engine);
	/// game_scene_base.update_tracked_asset_loaders(&engine);
	/// assert_eq!(game_scene_base.get_loading_progress_state().get_progress(), 0.5);
	/// assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());
	/// generate_handle.finish(Ok(Vec::new()));
	/// game_scene_base.update_tracked_asset_loaders(&engine);
	/// assert_eq!(game_scene_base.get_loading_progress_state().get_progress(), 1.0);
	/// assert!(game_scene_base.get_scene_base().are_tracked_assets_finished());
	/// ```
	//==============================================================================
	pub fn track_asset_loader(&mut self, asset_loader: &AssetLoader) {
		let completion_handle = asset_loader.get_completion_handle();
		self.scene_base.track_load_handle(&completion_handle);
		self.tracked_asset_loader_list.push(asset_loader.clone());
		self.is_loading_progress_reported = true;
	}

	//==============================================================================
	/// 맡긴 애셋 함께 받기를 갱신하고 진행률을 알린다. (맡긴 것들의 끝난 수 / 전체 수)
	/// - 맡긴 것이 없으면 아무것도 하지 않는다. draw_on_load 가 프레임마다 부른다.
	///
	/// # 인자
	/// - `engine`: 받기의 update 에 넘길 엔진.
	//==============================================================================
	pub fn update_tracked_asset_loaders(&mut self, engine: &Engine) {
		if self.tracked_asset_loader_list.is_empty() {
			return;
		}
		let mut finished_count = 0;
		let mut total_count = 0;
		for asset_loader in self.tracked_asset_loader_list.iter() {
			asset_loader.update(engine);
			finished_count += asset_loader.get_finished_count();
			total_count += asset_loader.get_total_count();
		}
		let progress = if total_count == 0 {
			1.0
		}
		else {
			finished_count as f64 / total_count as f64
		};
		self.loading_progress.set_progress(progress);
	}

	//==============================================================================
	/// 로딩 문을 뺀 맡긴 애셋이 다 끝났으면 진행률을 1 로 채운다. (원본 load 가 load_assets 를 기다린 뒤 1 을 알리는 자리, 알린 적이 없으면 하지 않는다)
	//==============================================================================
	pub fn complete_loading_progress_if_assets_finished(&mut self) {
		if !self.is_loading_progress_reported {
			return;
		}
		let are_assets_finished = match self.scene_base.get_loading_gate_handle() {
			Some(load_gate_handle) => {
				self.scene_base.are_tracked_assets_finished_except(load_gate_handle)
			}
			None => {
				self.scene_base.are_tracked_assets_finished()
			}
		};
		if are_assets_finished {
			self.loading_progress.set_progress(1.0);
		}
	}

	//==============================================================================
	/// 캔버스 배경색 설정. (pre_draw 에서 캔버스 전체에 칠한다)
	///
	/// # 인자
	/// - `color`: 배경색. (Color 나 "#rrggbb" 같은 글, 기본 검정)
	//==============================================================================
	pub fn set_scene_background_color(&mut self, color: impl Into<Color>) {
		self.scene_base.set_scene_background_color(color);
	}

	//==============================================================================
	/// 세이프 에어리어 노드 반환. (씬이 콘텐츠를 여기에 붙인다. initialize 전이면 None)
	///
	/// # 돌려줌
	/// 루트 아래 "safeArea" 노드. initialize 전이거나 노드를 만들지 않는 바탕(UISceneBase)이면 None.
	//==============================================================================
	pub fn get_safe_area_node_id(&self) -> Option<NodeId> {
		self.safe_area_node_id
	}

	//==============================================================================
	/// 마지막으로 계산한 세이프 에어리어 영역 반환. (뷰 좌표)
	///
	/// # 돌려줌
	/// 마지막 layout 이 계산한 영역. (뷰 좌표, 계산 전이면 0)
	//==============================================================================
	pub fn get_safe_area_rect(&self) -> Rect {
		self.last_safe_area_rect
	}

	//==============================================================================
	/// 세이프 에어리어 인셋 직접 설정. (CSS 픽셀, None 이면 뷰 매니저의 get_safe_area_insets 를 읽는다)
	///
	/// # 인자
	/// - `insets`: 위, 오른쪽, 아래, 왼쪽 인셋. (CSS 픽셀, 다음 layout 부터 쓴다) None 이면 뷰 매니저에서 읽는다.
	//==============================================================================
	pub fn set_safe_area_insets(&mut self, insets: Option<SafeAreaInsets>) {
		self.safe_area_insets = insets;
	}

	//==============================================================================
	/// 생성. (씬 load 에서 맨 먼저 부른다. 원본 load 의 앞부분 + load_assets)
	/// - 엔진 시작 뒤 실제 경과 시간으로 begin_loading 을 부르고 load_assets 를 부른다.
	///
	/// # 인자
	/// - `engine`: 시각을 읽고 로딩 이미지를 부탁할 엔진.
	//==============================================================================
	pub fn load(&mut self, engine: &Engine) {
		let current_time = engine.get_time_manager().get_realtime_since_startup();
		// 엔진 시계가 아직 돌지 않았으면(첫 프레임 전) 첫 로딩 프레임의 시각부터 잰다. (원본 load 와 같은 규칙)
		self.scene_base.begin_loading_gate(current_time, current_time <= 0.0, self.load_min_duration_ms);
		self.load_assets();
	}

	//==============================================================================
	/// 로딩 시작. (시작 시각을 적고, 최소 시간 / 터치를 기다리는 로딩 문을 씬에 맡긴다)
	/// - 최소 노출 시간이 0 이하이면 로딩 문을 처음부터 열어 둔다.
	/// - 문은 엔진이 로딩 중 프레임마다(엔진 시계와 엔진 입력으로) 갱신해 연다. 덮어쓸 수 있는 draw_on_load 에 기대지 않아, 씬이 바탕 draw_on_load 를 부르지 않아도 initialize 가 불린다. (원본 load 가 거는 엔진 프레임 함수)
	///
	/// # 인자
	/// - `current_time`: 지금 시각. (초, 엔진 시작 뒤 실제 경과 시간)
	//==============================================================================
	pub fn begin_loading(&mut self, current_time: f64) {
		self.scene_base.begin_loading_gate(current_time, false, self.load_min_duration_ms);
	}

	//==============================================================================
	/// 자산 읽기. (원본 loadAssets 의 기본 부분: 로딩 이미지. 씬의 자체 애셋은 씬 load 에서 요청하고 track_asset 으로 맡긴다)
	/// - 로딩 이미지 경로가 비었으면 아무것도 하지 않는다.
	//==============================================================================
	pub fn load_assets(&mut self) {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::load_assets");
		if self.loading_image_url.is_empty() {
			return;
		}
		let mut loading_image_asset = ImageAsset::new();
		loading_image_asset.load(engine, &self.loading_image_url);
		self.scene_base.track_asset(&loading_image_asset);
		self.loading_image_asset = Some(loading_image_asset);
	}

	//==============================================================================
	/// 로딩 갱신. (누르면 건너뛰기로 적고, 최소 시간이 지났거나 건너뛰었으면 로딩 문을 연다)
	/// - begin_loading 전이거나 문이 이미 열렸으면 문은 그대로다.
	/// - 엔진이 로딩 중 프레임마다 같은 갱신을 부른다. 게임이 따로 부르지 않아도 된다. (시각을 손으로 주는 시험과 도구 자리)
	///
	/// # 인자
	/// - `current_time`: 지금 시각. (초, begin_loading 에 준 것과 같은 시계)
	/// - `is_touch_pressed`: 이번 프레임에 화면을 눌렀는지.
	///
	/// # 예제
	/// 최소 노출 시간이 지나야 로딩 문이 열린다
	/// ```rust
	/// use vanilla::GameSceneBase;
	///
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_loading_min_duration_ms(3000.0);
	/// game_scene_base.begin_loading(10.0);
	/// assert_eq!(game_scene_base.get_loading_progress(11.5), 0.5);
	/// game_scene_base.update_loading(12.9, false);
	/// assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());
	/// game_scene_base.update_loading(13.0, false);
	/// assert!(game_scene_base.get_scene_base().are_tracked_assets_finished());
	/// ```
	///
	/// 누르면 최소 시간 전이라도 열린다
	/// ```rust
	/// use vanilla::GameSceneBase;
	///
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.begin_loading(0.0);
	/// game_scene_base.update_loading(0.2, true);
	/// assert!(game_scene_base.is_load_touched_to_skip());
	/// assert!(game_scene_base.get_scene_base().are_tracked_assets_finished());
	/// ```
	//==============================================================================
	pub fn update_loading(&mut self, current_time: f64, is_touch_pressed: bool) {
		self.scene_base.update_loading_gate(current_time, is_touch_pressed);
	}

	//==============================================================================
	/// 로딩 게이지 진행률. (경과 시간 / 최소 노출 시간, 0 ~ 1)
	///
	/// # 인자
	/// - `current_time`: 지금 시각. (초, begin_loading 에 준 것과 같은 시계)
	///
	/// # 돌려줌
	/// 경과 시간 비율. (0 ~ 1, 최소 노출 시간이 0 이하이면 1)
	//==============================================================================
	pub fn get_loading_progress(&self, current_time: f64) -> f64 {
		if self.load_min_duration_ms <= 0.0 {
			return 1.0;
		}
		let elapsed_milliseconds = (current_time - self.scene_base.get_loading_gate_start_time()) * 1000.0;
		(elapsed_milliseconds / self.load_min_duration_ms).clamp(0.0, 1.0)
	}

	//==============================================================================
	/// 건너뛰기 눌렸는지.
	///
	/// # 돌려줌
	/// begin_loading 뒤 update_loading 에 누름이 왔으면 true.
	//==============================================================================
	pub fn is_load_touched_to_skip(&self) -> bool {
		self.scene_base.is_loading_gate_touched_to_skip()
	}

	//==============================================================================
	/// 로딩 화면 출력. (엔진이 initialize 전까지 매 프레임 부른다. 바깥 색 캔버스 + 바탕색 뷰 + (그리기 함수, 없으면 가운데 로딩 이미지 + 아래 게이지))
	/// - 그리기 전에 맡긴 받기를 갱신하고, 보이는 진행률을 따라가게 한다. (로딩 시각과 건너뛰기는 엔진이 로딩 중 프레임마다 잡는다)
	///
	/// # 인자
	/// - `graphic`: 로딩 화면을 그릴 그래픽.
	//==============================================================================
	pub fn draw_on_load(&mut self, graphic: &mut Graphic) {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::draw_on_load");
		// 지금 시각. (로딩 시각과 건너뛰기는 엔진이 프레임마다 SceneBase 의 로딩 문에서 잡는다)
		let current_time = engine.get_time_manager().get_realtime_since_startup();

		// 진행률. (맡긴 애셋 함께 받기를 갱신해 읽고, 로딩 문을 뺀 애셋이 다 끝났으면 1 로 채우고, 보이는 정도를 따라가게 한다)
		self.update_tracked_asset_loaders(engine);
		self.complete_loading_progress_if_assets_finished();
		let unscaled_time_delta = engine.get_time_manager().get_unscale_delta_time();
		self.loading_progress.update(unscaled_time_delta);
		let progress = if self.is_loading_progress_reported {
			self.loading_progress.get_shown_progress()
		}
		else {
			self.get_loading_progress(current_time)
		};
		let elapsed_seconds = (current_time - self.scene_base.get_loading_gate_start_time()).max(0.0);

		let view_manager = engine.get_view_manager();
		let canvas_native_size = view_manager.get_canvas_native_size();
		let view_size = view_manager.get_view_size();

		// 캔버스 전체 바깥 색. (기본 검은색)
		view_manager.apply_canvas_native_rect(graphic);
		graphic.set_fill_color(self.loading_outside_color);
		graphic.draw_rect(Rect::create(0.0, 0.0, canvas_native_size.x, canvas_native_size.y));

		// 뷰 좌표계 적용.
		view_manager.apply_view_rect(graphic);
		graphic.set_fill_color(self.loading_background_color);
		graphic.draw_rect(Rect::create(0.0, 0.0, view_size.x, view_size.y));

		// 그리기 함수.
		if let Some(loading_drawer) = self.get_loading_drawer() {
			if let Ok(mut drawer_function) = loading_drawer.try_borrow_mut() {
				(*drawer_function)(graphic, view_size, progress, elapsed_seconds);
			}
			return;
		}

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
			let ratio = image.get_height() as f64 / (image.get_width() as f64).max(1.0);
			let target_height = target_width * ratio;
			let position_x = (view_size.x - target_width) * 0.5;
			let position_y = (view_size.y - target_height) * 0.5;
			graphic.draw_image(&image, Vector2::create(position_x, position_y), Vector2::create(target_width, target_height));
		}

		// 아래 로딩 게이지.
		let bar_width = view_size.x * 0.6;
		let bar_x = (view_size.x - bar_width) * 0.5;
		let bar_y = view_size.y * 0.78;
		graphic.set_fill_color(self.loading_bar_back_color);
		graphic.draw_rect(Rect::create(bar_x, bar_y, bar_width, LOADING_BAR_HEIGHT));
		graphic.set_fill_color(self.loading_bar_fill_color);
		graphic.draw_rect(Rect::create(bar_x, bar_y, bar_width * progress, LOADING_BAR_HEIGHT));
	}

	//==============================================================================
	/// 초기화. (씬 initialize 에서 맨 먼저 부른다. ViewScaleMode 적용, 세이프 에어리어 노드 생성)
	/// - 게임 영역 기준을 걸고, EngineConfiguration::use_dev_tools 가 켜져 있으면 개발자 도구를 만든다.
	///
	/// # 인자
	/// - `engine`: 뷰 매니저와 설정을 가진 엔진.
	//==============================================================================
	pub fn initialize(&mut self, engine: &Engine) {
		self.apply_view_scale_mode(engine);
		self.apply_play_area_reference();
		// 터치 인식기. (원본처럼 initialize 에서 만든다)
		self.touch_recognizer = Some(TouchRecognizer::new());
		// 개발자 도구. (EngineConfiguration::use_dev_tools 가 꺼져 있으면 만들지 않는다)
		self.dev_tools = None;
		if engine.get_engine_configuration().use_dev_tools {
			let mut dev_tools = DevTools::new();
			dev_tools.set_root_nodes(vec![self.scene_base.get_root_id()]);
			self.dev_tools = Some(dev_tools);
		}
		if self.is_safe_area_node_enabled {
			self.create_safe_area_node();
		}
		else {
			self.last_safe_area_rect = Rect::zero();
			self.last_view_size_x = 0.0;
			self.last_view_size_y = 0.0;
		}
	}

	//==============================================================================
	/// 세이프 에어리어 노드 생성. (루트 아래 "safeArea", 피봇 / 앵커 왼쪽 위)
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
	/// 화면 크기 변경됨. (게임 영역 기준을 다시 걸고 기본 레이아웃을 다시 한다. 씬의 layout 은 game_scene! 이 이 뒤에 부른다)
	///
	/// # 인자
	/// - `_canvas_native_size`: 창 안쪽 논리 크기. (쓰지 않는다, 뷰 매니저에서 읽는다)
	//==============================================================================
	pub fn resize(&mut self, _canvas_native_size: Vector2) {
		self.apply_play_area_reference();
		self.layout();
	}

	//==============================================================================
	/// 레이아웃. (세이프 에어리어 영역을 다시 계산해 노드에 건다)
	//==============================================================================
	pub fn layout(&mut self) {
		let safe_area_rect = self.compute_safe_area_rect();
		self.apply_safe_area_rect(safe_area_rect);
	}

	//==============================================================================
	/// 세이프 에어리어 영역을 노드에 건다. (노드가 아직 없으면 영역만 적는다)
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
	/// 세이프 에어리어 계산. (뷰 좌표)
	/// - 직접 정한 인셋은 뷰 배율로 나눠 빼고, 아니면 뷰 매니저의 인셋(이미 뷰 좌표)을 뺀다. 게임 영역 기준을 쓰면 캔버스를 배율로 나눈 크기로 셈한다.
	///
	/// # 돌려줌
	/// 세이프 에어리어 영역. (뷰 좌표, 크기는 0 아래로 내려가지 않는다)
	//==============================================================================
	pub fn compute_safe_area_rect(&self) -> Rect {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::compute_safe_area_rect");
		let view_manager = engine.get_view_manager();
		let target_resolution_scale = view_manager.get_target_resolution_scale();
		// 게임 영역 기준을 쓰면 올림한 뷰 크기 대신 캔버스를 배율로 나눈 실제 크기로 셈해 고정 쪽이 딱 맞게 한다.
		let mut view_size = view_manager.get_view_size();
		if self.play_area_fixed_length > 0.0 {
			let canvas_native_size = view_manager.get_canvas_native_size();
			let view_scale = if target_resolution_scale > 0.0 {
				target_resolution_scale
			}
			else {
				1.0
			};
			view_size = Vector2::create(canvas_native_size.x / view_scale, canvas_native_size.y / view_scale);
		}
		match self.safe_area_insets {
			Some(insets) => {
				GameSceneBase::calculate_safe_area_rect(view_size, target_resolution_scale, insets)
			}
			None => {
				let insets = view_manager.get_safe_area_insets();
				let width = (view_size.x - insets.left - insets.right).max(0.0);
				let height = (view_size.y - insets.top - insets.bottom).max(0.0);
				Rect::create(insets.left, insets.top, width, height)
			}
		}
	}

	//==============================================================================
	/// 세이프 에어리어 계산. (정적. CSS 픽셀 인셋을 뷰 배율로 나눠 뷰 영역에서 뺀다)
	///
	/// # 인자
	/// - `view_size`: 뷰 크기. (뷰 좌표)
	/// - `target_resolution_scale`: 뷰 배율. (뷰 좌표 1 이 CSS 픽셀 몇인지, 0 이하이면 1)
	/// - `insets`: 위, 오른쪽, 아래, 왼쪽 인셋. (CSS 픽셀)
	///
	/// # 돌려줌
	/// 세이프 에어리어 영역. (뷰 좌표, 크기는 0 아래로 내려가지 않는다)
	///
	/// # 예제
	/// 배율 2 에서 인셋은 반으로 줄어 빠진다
	/// ```rust
	/// use vanilla::{GameSceneBase, Rect, SafeAreaInsets, Vector2};
	///
	/// let safe_area_rect = GameSceneBase::calculate_safe_area_rect(Vector2::create(400.0, 300.0), 2.0, SafeAreaInsets::create(40.0, 10.0, 20.0, 30.0));
	/// assert_eq!(safe_area_rect, Rect::create(15.0, 20.0, 380.0, 270.0));
	/// let collapsed_rect = GameSceneBase::calculate_safe_area_rect(Vector2::create(10.0, 10.0), 0.0, SafeAreaInsets::create(20.0, 20.0, 20.0, 20.0));
	/// assert_eq!(collapsed_rect, Rect::create(20.0, 20.0, 0.0, 0.0));
	/// ```
	//==============================================================================
	pub fn calculate_safe_area_rect(view_size: Vector2, target_resolution_scale: f64, insets: SafeAreaInsets) -> Rect {
		let scale = if target_resolution_scale > 0.0 {
			target_resolution_scale
		}
		else {
			1.0
		};
		let inset_top = insets.top / scale;
		let inset_right = insets.right / scale;
		let inset_bottom = insets.bottom / scale;
		let inset_left = insets.left / scale;
		let width = (view_size.x - inset_left - inset_right).max(0.0);
		let height = (view_size.y - inset_top - inset_bottom).max(0.0);
		Rect::create(inset_left, inset_top, width, height)
	}

	//==============================================================================
	/// 뷰 크기가 바뀌었으면 기본 레이아웃을 다시 한다. (바뀌었으면 true)
	///
	/// # 인자
	/// - `engine`: 뷰 크기를 읽을 엔진.
	///
	/// # 돌려줌
	/// 뷰 크기가 마지막으로 본 것과 달라 레이아웃을 다시 했으면 true. (initialize 가 본 크기를 0 으로 되돌려, 뷰 크기가 0 이 아니면 그 뒤 처음 부를 때도 true)
	//==============================================================================
	pub fn update_layout_if_view_size_changed(&mut self, engine: &Engine) -> bool {
		let view_size = engine.get_view_manager().get_view_size();
		if view_size.x == self.last_view_size_x && view_size.y == self.last_view_size_y {
			return false;
		}
		self.last_view_size_x = view_size.x;
		self.last_view_size_y = view_size.y;
		self.layout();
		true
	}

	//==============================================================================
	/// 갱신. (노드 → TouchRecognizer 터치 → 트윈, 그다음 뷰 크기 변화를 보고 레이아웃. 레이아웃을 다시 했으면 true)
	/// - 모바일 주소창이 나타나고 사라질 때처럼 resize 만으로는 놓칠 수 있어 매 tick 확인한다. (원본과 같음)
	/// - 뒤이어 화면 페이드와 개발자 도구를 갱신한다. 씬이 꺼내 가지 않은 페이드 시작은 다음 폴링 차례로 맡긴다.
	///
	/// # 인자
	/// - `time_delta`: 배율을 곱한 프레임 간격. (초, 노드와 트윈에 쓴다. 페이드는 배율을 곱하지 않은 시간으로 돈다)
	///
	/// # 돌려줌
	/// 뷰 크기가 바뀌어 레이아웃을 다시 했으면 true. (씬은 이때 자기 layout 을 부른다)
	///
	/// # 예제
	/// 뷰 크기가 바뀐 tick 만 true 이고 세이프 에어리어가 다시 셈해진다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, Rect, SafeAreaInsets};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(10.0, 0.0, 20.0, 5.0)));
	/// game_scene_base.initialize(engine);
	/// assert!(game_scene_base.tick(0.016));
	/// assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 795.0, 570.0));
	/// assert!(!game_scene_base.tick(0.016));
	/// engine.get_view_manager_mut().calculate_view_rect(1024, 768, 1.0);
	/// assert!(game_scene_base.tick(0.016));
	/// assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 1019.0, 738.0));
	/// ```
	//==============================================================================
	pub fn tick(&mut self, time_delta: f64) -> bool {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::tick");
		self.start_left_screen_fade_dark_starter(engine);
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.tick_touch(engine);
		self.scene_base.tick_tweens(time_delta);
		let is_layout_changed = self.update_layout_if_view_size_changed(engine);
		self.update_screen_fade(engine);
		if let Some(dev_tools) = self.dev_tools.as_mut() {
			let unscaled_time_delta = engine.get_time_manager().get_unscale_delta_time();
			dev_tools.tick(engine, self.scene_base.get_node_tree_mut(), unscaled_time_delta);
		}
		is_layout_changed
	}

	//==============================================================================
	/// 터치 갱신. (TouchRecognizer 로 UI 에 넘기고, 처리기나 모으기가 켜져 있으면 UI 가 받지 않은 입력을 놀이 입력으로 넘긴다)
	/// - 페이드 막이 걸린 동안은 누름과 휠을 막고, 이미 누르던 손가락의 이동, 뗌, 취소만 넘긴다.
	/// - UI 판정은 넘기기 전에 한다. (누른 단추가 창을 띄워도 이 누름은 놀이로 가지 않게)
	/// - 휠은 창 층 안을 가리키면 맨 위의 창이 먼저 받는다. (원본 touchWheel)
	//==============================================================================
	fn tick_touch(&mut self, engine: &Engine) {
		let input_manager = engine.get_input_manager();
		let view_input_position = input_manager.get_view_input_position();
		let is_touch_pressed = input_manager.is_touch_pressed();
		let is_touch_moved = input_manager.is_touch_moved();
		let is_touch_ended = input_manager.is_touch_released() || input_manager.is_touch_cancelled();
		let wheel_delta = Vector2::create(input_manager.get_wheel_delta_x(), input_manager.get_wheel_delta_y());
		let is_touch_released = input_manager.is_touch_released();
		let is_touch_cancelled = input_manager.is_touch_cancelled();
		// 입력 값은 먼저 읽어 두고 빌림을 놓는다. (아래 터치 인식기가 부르는 컴포넌트가 입력 매니저를 고칠 수 있다)
		drop(input_manager);
		let is_wheel = wheel_delta.x != 0.0 || wheel_delta.y != 0.0;
		// 개발자 도구 판 위의 입력은 UI 와 놀이로 넘기지 않는다. (원본 isDevToolsCapturingInput)
		if self.is_dev_tools_capturing_input() {
			return;
		}
		let is_screen_fading = self.is_screen_fading();
		let is_play_input_wanted = self.is_play_input_enabled || self.play_input_handler.is_some();
		if is_screen_fading {
			if is_touch_pressed {
				self.is_play_press_active = false;
			}
			if let Some(touch_recognizer) = self.touch_recognizer.as_mut().filter(|touch_recognizer| touch_recognizer.is_touching()) {
				let node_tree = self.scene_base.get_node_tree_mut();
				if is_touch_moved {
					touch_recognizer.touch_move(node_tree, engine, view_input_position);
				}
				if is_touch_released {
					touch_recognizer.touch_release(node_tree, engine, view_input_position);
				}
				else if is_touch_cancelled {
					touch_recognizer.touch_cancel(node_tree, engine, view_input_position);
				}
			}
		}
		else {
			let is_user_interface_pressed = is_touch_pressed && self.is_user_interface_at(view_input_position);
			let mut is_play_wheel = false;
			if is_wheel {
				let hit_node_id = self.scene_base.get_node_tree().find_topmost_interactable_node(view_input_position);
				let window_layer_id = hit_node_id.and_then(|hit_node_id| UIWindowLayer::find_window_layer_of(self.scene_base.get_node_tree(), hit_node_id));
				match window_layer_id {
					Some(window_layer_id) => {
						UIWindowLayer::scroll_window_at(self.scene_base.get_node_tree_mut(), engine, window_layer_id, view_input_position, wheel_delta);
						if let Some(touch_recognizer) = self.touch_recognizer.as_mut() {
							touch_recognizer.mark_wheel_handled();
						}
					}
					None => {
						let is_wheel_handled = self.touch_recognizer.as_mut().is_some_and(|touch_recognizer| touch_recognizer.tick_wheel(self.scene_base.get_node_tree_mut(), engine));
						is_play_wheel = !is_wheel_handled && hit_node_id.is_none();
					}
				}
			}
			else if let Some(touch_recognizer) = self.touch_recognizer.as_mut() {
				touch_recognizer.tick_wheel(self.scene_base.get_node_tree_mut(), engine);
			}
			if let Some(touch_recognizer) = self.touch_recognizer.as_mut() {
				touch_recognizer.tick_pointer(self.scene_base.get_node_tree_mut(), engine);
			}
			if is_play_wheel && is_play_input_wanted {
				let play_area_position = self.convert_view_to_play_area(view_input_position);
				self.dispatch_play_input(PlayInput::Wheel(play_area_position, wheel_delta));
			}
			if is_touch_pressed {
				self.is_play_press_active = is_play_input_wanted && !is_user_interface_pressed;
				if self.is_play_press_active {
					let play_area_position = self.convert_view_to_play_area(view_input_position);
					self.dispatch_play_input(PlayInput::Press(play_area_position));
				}
			}
		}
		if !self.is_play_press_active {
			return;
		}
		let play_area_position = self.convert_view_to_play_area(view_input_position);
		if is_touch_moved && !is_touch_pressed {
			self.dispatch_play_input(PlayInput::Move(play_area_position));
		}
		if is_touch_ended {
			self.is_play_press_active = false;
			self.dispatch_play_input(PlayInput::Release(play_area_position));
		}
	}

	//==============================================================================
	/// 놀이 입력 넘기기. (처리기가 있으면 부르고, 모으기가 켜져 있으면 모은다, 원본 dispatchPlayInput)
	//==============================================================================
	fn dispatch_play_input(&mut self, play_input: PlayInput) {
		if let Some(play_input_handler) = self.play_input_handler.as_mut() {
			play_input_handler(play_input);
		}
		if self.is_play_input_enabled {
			self.play_inputs.push(play_input);
		}
	}

	//==============================================================================
	/// 이번 프레임의 휠을 스크롤 칸 등이 받아 썼는지. (tick 뒤에 읽는다, false 일 때만 게임의 지도 확대 등에 쓰면 창의 스크롤과 겹치지 않는다)
	///
	/// # 돌려줌
	/// 이번 프레임의 휠을 스크롤 칸이나 창이 받아 썼으면 true.
	//==============================================================================
	pub fn is_wheel_handled(&self) -> bool {
		self.touch_recognizer.as_ref().is_some_and(|touch_recognizer| touch_recognizer.is_wheel_handled())
	}

	//==============================================================================
	/// 개발자 도구가 입력을 가져가는 중인지. (보이고, 누른 자리가 판 안)
	///
	/// # 돌려줌
	/// 개발자 도구가 보이고 입력 자리가 판 안이면 true. 개발자 도구가 없으면 false.
	//==============================================================================
	pub fn is_dev_tools_capturing_input(&self) -> bool {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::is_dev_tools_capturing_input");
		match self.dev_tools.as_ref() {
			Some(dev_tools) => {
				dev_tools.is_visible() && dev_tools.is_pointer_inside_panel(engine)
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 이전 출력. (캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 건다. 세이프 에어리어 바깥 포함)
	///
	/// # 인자
	/// - `graphic`: 칠할 그래픽.
	//==============================================================================
	pub fn pre_draw(&self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
	}

	//==============================================================================
	/// 이후 출력. (어두워지는 막을 세이프 에어리어까지 캔버스 전체에 그리고, 개발자 도구를 맨 위에 그린다)
	/// - 씬이 꺼내 가지 않은 페이드 시작은 여기서 다음 폴링 차례로 맡긴다.
	///
	/// # 인자
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn post_draw(&mut self, graphic: &mut Graphic) {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::post_draw");
		self.start_left_screen_fade_dark_starter(engine);
		self.draw_screen_fade(graphic);
		if let Some(dev_tools) = self.dev_tools.as_mut() {
			dev_tools.draw(engine, self.scene_base.get_node_tree(), graphic);
		}
	}

	//==============================================================================
	/// 어두워지는 막 그리기. (세이프 에어리어까지 캔버스 전체, 원본 drawScreenFade)
	/// - 막의 진하기가 0 이면 그리지 않는다. 그린 뒤 뷰 좌표계를 다시 건다.
	///
	/// # 인자
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn draw_screen_fade(&self, graphic: &mut Graphic) {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::draw_screen_fade");
		if self.screen_fade_alpha <= 0.0 {
			return;
		}
		let view_manager = engine.get_view_manager();
		let canvas_native_size = view_manager.get_canvas_native_size();
		let global_alpha = graphic.get_global_alpha();
		view_manager.apply_canvas_native_rect(graphic);
		graphic.set_global_alpha(global_alpha * self.screen_fade_alpha);
		graphic.set_fill_color(self.screen_fade_color);
		graphic.draw_rect(Rect::create(0.0, 0.0, canvas_native_size.x, canvas_native_size.y));
		graphic.set_global_alpha(global_alpha);
		view_manager.apply_view_rect(graphic);
	}

	//==============================================================================
	/// 어두워졌다 밝아지기. (다 어두워지면 on_dark 를 부르고, 그것이 손잡이를 돌려주면 놓을 때까지 어둡게 둔다, 이미 페이드 중이면 false)
	/// - 막은 tick 이 움직이고 post_draw 가 그린다. 막이 걸린 동안은 누름과 휠을 받지 않는다.
	///
	/// # 인자
	/// - `on_dark`: 다 어두워졌을 때 부를 함수. (트리와 엔진을 받고, 손잡이를 돌려주면 놓을 때까지 어둡다) None 이면 어두워졌다 바로 밝아진다.
	///
	/// # 돌려줌
	/// 페이드를 시작했으면 true. 이미 페이드 중이면 false.
	///
	/// # 예제
	/// 다 어두워진 자리에서 화면을 바꾸고 바로 밝아지기
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, NodeTree};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_screen_fade_seconds(0.0);
	/// assert!(game_scene_base.fade_screen(Some(Box::new(|node_tree: &mut NodeTree, _engine: &Engine| {
	/// 	let root_id = node_tree.get_root_id();
	/// 	node_tree.create_child(root_id, "nextStage");
	/// 	None
	/// }))));
	/// assert!(!game_scene_base.fade_screen(None));
	/// game_scene_base.tick(0.016);
	/// assert_eq!(game_scene_base.get_screen_fade_alpha(), 1.0);
	/// game_scene_base.tick(0.016);
	/// assert!(!game_scene_base.is_screen_fading());
	/// ```
	//==============================================================================
	pub fn fade_screen(&mut self, on_dark: Option<ScreenFadeEvent>) -> bool {
		if self.is_screen_fading() {
			return false;
		}
		self.screen_fade_event = on_dark;
		self.screen_fade_direction = 1;
		true
	}

	//==============================================================================
	/// 어두워졌다 밝아지기의 콜백 꼴. (다 어두워지면 on_dark 에 손잡이를 넘기고, 그 손잡이를 놓을 때까지 어둡게 둔다, 이미 페이드 중이면 false)
	/// - 원본 onDark 가 돌려주는 Promise 의 resolve 자리가 손잡이의 release 다. 다 어두워진 자리에서 바로 놓으면 바로 밝아진다.
	///
	/// # 인자
	/// - `on_dark`: 다 어두워졌을 때 트리, 엔진, 손잡이를 받을 함수. (손잡이를 들고 있다가 일이 끝나면 release 한다)
	///
	/// # 돌려줌
	/// 페이드를 시작했으면 true. 이미 페이드 중이면 false.
	///
	/// # 예제
	/// 손잡이를 받아 두었다가 일이 끝나면 놓기
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, NodeTree, ScreenFadeHold};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_screen_fade_seconds(0.0);
	/// let held_hold: Rc<RefCell<Option<ScreenFadeHold>>> = Rc::new(RefCell::new(None));
	/// let held_hold_for_event = held_hold.clone();
	/// assert!(game_scene_base.fade_screen_with_hold(Box::new(move |_node_tree: &mut NodeTree, _engine: &Engine, screen_fade_hold: ScreenFadeHold| {
	/// 	*held_hold_for_event.borrow_mut() = Some(screen_fade_hold);
	/// })));
	/// game_scene_base.tick(0.016);
	/// game_scene_base.tick(0.016);
	/// assert_eq!(game_scene_base.get_screen_fade_alpha(), 1.0);
	/// if let Some(screen_fade_hold) = held_hold.borrow().as_ref() {
	/// 	screen_fade_hold.release();
	/// }
	/// game_scene_base.tick(0.016);
	/// game_scene_base.tick(0.016);
	/// assert!(!game_scene_base.is_screen_fading());
	/// ```
	//==============================================================================
	pub fn fade_screen_with_hold(&mut self, on_dark: ScreenFadeHoldEvent) -> bool {
		self.fade_screen(Some(Box::new(move |node_tree: &mut NodeTree, engine: &Engine| {
			let screen_fade_hold = ScreenFadeHold::new();
			on_dark(node_tree, engine, screen_fade_hold.clone());
			Some(screen_fade_hold)
		})))
	}

	//==============================================================================
	/// 어두워졌다 밝아지기의 async 꼴. (원본 fadeScreen(async () => { ... }), 이미 페이드 중이면 false)
	/// - 다 어두워진 tick 에서 on_dark 가 돌려준 퓨처를 이 씬의 작업으로 맡겨 그 자리에서 첫 await 까지 돌리고, 퓨처가 끝나면 밝아진다.
	///   퓨처 안에서는 SceneContext::with 로 씬(S 타입)과 엔진을 빌린다.
	/// - S 는 이 바탕을 든 씬의 타입이다. (GameScene 을 구현한 씬은 GameScene::fade_screen_async 가 S 를 채운다) 다른 타입이면 바로 밝아진다.
	/// - 시작은 바탕 tick 뒤에 씬이 take_screen_fade_dark_starter 로 꺼내 start 한다. (game_scene!, ui_scene! 은 저절로 한다)
	///   꺼내지 않으면 바탕이 post_draw 나 다음 tick 에서 다음 폴링 차례로 맡긴다.
	///
	/// # 인자
	/// - `on_dark`: 다 어두워졌을 때 `SceneContext<S>` 를 받아 퓨처를 돌려주는 함수. (퓨처가 끝나거나 버려지면 밝아진다)
	///
	/// # 돌려줌
	/// 페이드를 시작했으면 true. 이미 페이드 중이면 false.
	///
	/// # 예제
	/// Scene 을 직접 구현한 씬에서 씬 타입을 주어 부르기 (시작은 tick 에서 꺼낸다, ScreenFadeDarkStarter 예제)
	/// ```rust,no_run
	/// use vanilla::{Engine, GameSceneBase, Scene, SceneBase, SceneContext, Vector2};
	///
	/// struct PuzzleScene {
	/// 	game_scene_base: GameSceneBase,
	/// 	stage_number: u32,
	/// }
	///
	/// impl Scene for PuzzleScene {
	/// 	fn get_scene_base(&self) -> &SceneBase {
	/// 		self.game_scene_base.get_scene_base()
	/// 	}
	///
	/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
	/// 		self.game_scene_base.get_scene_base_mut()
	/// 	}
	///
	/// 	fn touch_press(&mut self, _view_input_position: Vector2) {
	/// 		self.game_scene_base.fade_screen_async::<PuzzleScene, _, _>(|mut scene_context: SceneContext<PuzzleScene>| async move {
	/// 			scene_context.with(|scene, _engine| {
	/// 				scene.stage_number += 1;
	/// 			});
	/// 		});
	/// 	}
	///
	/// 	fn tick(&mut self, time_delta: f64) {
	/// 		self.game_scene_base.tick(time_delta);
	/// 		if let Some(screen_fade_dark_starter) = self.game_scene_base.take_screen_fade_dark_starter() {
	/// 			screen_fade_dark_starter.start(self);
	/// 		}
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn fade_screen_async<S, F, OnDarkFuture>(&mut self, on_dark: F) -> bool
	where
		S: Scene + 'static,
		F: FnOnce(SceneContext<S>) -> OnDarkFuture + 'static,
		OnDarkFuture: Future<Output = ()> + 'static,
	{
		if self.is_screen_fading() {
			return false;
		}
		let start_function = Box::new(move |start_place: ScreenFadeDarkStartPlace<'_>, screen_fade_hold: ScreenFadeHold| {
			let screen_fade_hold_release = ScreenFadeHoldRelease {
				screen_fade_hold,
			};
			let task_function = move |scene_context: SceneContext<S>| {
				async move {
					let _screen_fade_hold_release = screen_fade_hold_release;
					on_dark(scene_context).await;
				}
			};
			match start_place {
				ScreenFadeDarkStartPlace::Now(engine, scene) => {
					if let Some(scene) = scene.as_scene_any_mut().downcast_mut::<S>() {
						engine.spawn_scene_task(scene, task_function);
					}
				}
				ScreenFadeDarkStartPlace::Deferred(engine, scene_id) => {
					task::spawn_deferred_scene_task(&engine.task_executor, scene_id, task_function);
				}
			}
		});
		self.screen_fade_dark_starter = Some(ScreenFadeDarkStarter {
			start_function,
			screen_fade_hold: ScreenFadeHold::new(),
		});
		self.is_screen_fade_dark_starter_ready = false;
		self.screen_fade_direction = 1;
		true
	}

	//==============================================================================
	/// 다 어두워진 fade_screen_async 의 시작 꺼내기. (바탕 tick 뒤에 씬이 꺼내 start(씬, 엔진) 한다, 아직이면 None)
	///
	/// # 돌려줌
	/// 다 어두워진 뒤 처음 부를 때 시작. 아직 어두워지지 않았거나 이미 꺼냈으면 None.
	//==============================================================================
	pub fn take_screen_fade_dark_starter(&mut self) -> Option<ScreenFadeDarkStarter> {
		if !self.is_screen_fade_dark_starter_ready {
			return None;
		}
		self.is_screen_fade_dark_starter_ready = false;
		self.screen_fade_dark_starter.take()
	}

	//==============================================================================
	/// 씬이 꺼내 가지 않은 시작을 다음 폴링 차례로 맡기기.
	//==============================================================================
	fn start_left_screen_fade_dark_starter(&mut self, engine: &Engine) {
		if let Some(screen_fade_dark_starter) = self.take_screen_fade_dark_starter() {
			let scene_id = SceneId::from(&self.scene_base);
			screen_fade_dark_starter.start_deferred(engine, scene_id);
		}
	}

	//==============================================================================
	/// 페이드 중인지. (어두워지거나, 손잡이를 기다리거나, 밝아지는 중)
	///
	/// # 돌려줌
	/// 어두워지거나 손잡이를 기다리거나 밝아지는 중이면 true.
	//==============================================================================
	pub fn is_screen_fading(&self) -> bool {
		self.screen_fade_direction != 0 || self.screen_fade_hold.is_some() || self.screen_fade_alpha > 0.0
	}

	//==============================================================================
	/// 어두워지는 막의 색 설정. (기본 검정)
	///
	/// # 인자
	/// - `color`: 막의 색. (Color 나 "#rrggbb" 같은 글)
	//==============================================================================
	pub fn set_screen_fade_color(&mut self, color: impl Into<Color>) {
		self.screen_fade_color = color.into();
	}

	//==============================================================================
	/// 어두워지는 시간과 밝아지는 시간 설정. (초, 기본 0.35, 0 이면 바로)
	///
	/// # 인자
	/// - `seconds`: 어두워지는 시간이자 밝아지는 시간. (초, 음수는 0)
	//==============================================================================
	pub fn set_screen_fade_seconds(&mut self, seconds: f64) {
		self.screen_fade_seconds = seconds.max(0.0);
	}

	//==============================================================================
	/// 어두워지는 막의 진하기. (0 ~ 1)
	///
	/// # 돌려줌
	/// 막의 진하기. (0 이면 막 없음, 1 이면 다 어두움)
	//==============================================================================
	pub fn get_screen_fade_alpha(&self) -> f64 {
		self.screen_fade_alpha
	}

	//==============================================================================
	/// 막 갱신. (배율을 곱하지 않은 시간으로 움직여 놀이를 멈춰도 돈다, 한 프레임은 0.1 초까지)
	//==============================================================================
	fn update_screen_fade(&mut self, engine: &Engine) {
		if let Some(screen_fade_hold) = self.screen_fade_hold.as_ref() {
			if screen_fade_hold.is_released() {
				self.screen_fade_hold = None;
				self.screen_fade_direction = -1;
			}
			return;
		}
		if self.screen_fade_direction == 0 {
			return;
		}
		let unscaled_time_delta = engine.get_time_manager().get_unscale_delta_time().min(SCREEN_FADE_MAXIMUM_STEP_SECONDS);
		let fade_step = if self.screen_fade_seconds > 0.0 {
			unscaled_time_delta / self.screen_fade_seconds
		}
		else {
			1.0
		};
		if self.screen_fade_direction < 0 {
			self.screen_fade_alpha = (self.screen_fade_alpha - fade_step).max(0.0);
			if self.screen_fade_alpha <= 0.0 {
				self.screen_fade_direction = 0;
			}
			return;
		}
		self.screen_fade_alpha = (self.screen_fade_alpha + fade_step).min(1.0);
		if self.screen_fade_alpha < 1.0 {
			return;
		}
		// 다 어두워졌다. 맡긴 일이 손잡이를 돌려주면 놓을 때까지 막을 그대로 두고, 아니면 바로 밝아진다.
		self.screen_fade_direction = -1;
		if let Some(screen_fade_dark_starter) = self.screen_fade_dark_starter.as_ref() {
			// async 꼴: 손잡이를 쥐고 시작을 꺼낼 수 있게 한다. (작업이 끝나거나 버려지면 놓는다)
			self.screen_fade_direction = 0;
			self.screen_fade_hold = Some(screen_fade_dark_starter.screen_fade_hold.clone());
			self.is_screen_fade_dark_starter_ready = true;
			return;
		}
		if let Some(screen_fade_event) = self.screen_fade_event.take() {
			let screen_fade_hold = screen_fade_event(self.scene_base.get_node_tree_mut(), engine);
			if let Some(screen_fade_hold) = screen_fade_hold {
				if !screen_fade_hold.is_released() {
					self.screen_fade_direction = 0;
					self.screen_fade_hold = Some(screen_fade_hold);
				}
			}
		}
	}

	//==============================================================================
	/// 그 자리에 UI(누를 수 있는 노드: 단추, 창의 어둠과 판, 스크롤 칸)가 있는지. (뷰 좌표)
	///
	/// # 인자
	/// - `view_position`: 볼 자리. (뷰 좌표)
	///
	/// # 돌려줌
	/// 그 자리에 누를 수 있는 노드가 있으면 true.
	//==============================================================================
	pub fn is_user_interface_at(&self, view_position: Vector2) -> bool {
		self.scene_base.get_node_tree().find_topmost_interactable_node(view_position).is_some()
	}

	//==============================================================================
	/// 놀이 입력 모으기 켜기. (끄면 모은 것을 버린다)
	/// - 켜면 tick 이 UI 가 받지 않은 입력을 모으고, 씬이 take_play_inputs 로 꺼낸다. 놀이로 간 누름은 잊는다.
	///
	/// # 인자
	/// - `is_play_input_enabled`: true 면 모으고, false 면 끄고 모은 것을 버린다.
	///
	/// # 예제
	/// UI 가 받지 않은 누름과 뗌을 모아 꺼내기 (좌표는 세이프 에어리어 왼쪽 위 기준)
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, PlayInput, SafeAreaInsets, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(10.0, 0.0, 0.0, 20.0)));
	/// game_scene_base.initialize(engine);
	/// game_scene_base.tick(0.016);
	/// game_scene_base.set_play_input_enabled(true);
	/// engine.get_input_manager_mut().set_view_input_position(Vector2::create(120.0, 110.0));
	/// engine.get_input_manager_mut().set_touch_pressed(true);
	/// game_scene_base.tick(0.016);
	/// engine.get_input_manager_mut().set_touch_pressed(false);
	/// engine.get_input_manager_mut().set_touch_released(true);
	/// game_scene_base.tick(0.016);
	/// let play_inputs = game_scene_base.take_play_inputs();
	/// assert_eq!(play_inputs.first(), Some(&PlayInput::Press(Vector2::create(100.0, 100.0))));
	/// assert_eq!(play_inputs.last(), Some(&PlayInput::Release(Vector2::create(100.0, 100.0))));
	/// assert!(game_scene_base.take_play_inputs().is_empty());
	/// ```
	//==============================================================================
	pub fn set_play_input_enabled(&mut self, is_play_input_enabled: bool) {
		self.is_play_input_enabled = is_play_input_enabled;
		self.is_play_press_active = false;
		if !is_play_input_enabled {
			self.play_inputs.clear();
		}
	}

	//==============================================================================
	/// UI 가 받지 않은 입력의 처리기 설정. (원본 setPlayInputHandler, None 이면 끈다, 놀이로 간 누름은 잊는다)
	/// - 처리기는 씬을 빌릴 수 없어서 씬 상태는 `Rc<RefCell>` 로 나눠 쥔다.
	///
	/// # 인자
	/// - `play_input_handler`: UI 가 받지 않은 입력을 받을 함수. (게임 영역 좌표) None 이면 끈다.
	///
	/// # 예제
	/// 처리기가 누름과 뗌을 받는다
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, PlayInput, SafeAreaInsets, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(0.0, 0.0, 0.0, 0.0)));
	/// game_scene_base.initialize(engine);
	/// let play_input_list: Rc<RefCell<Vec<PlayInput>>> = Rc::new(RefCell::new(Vec::new()));
	/// let play_input_list_for_handler = play_input_list.clone();
	/// game_scene_base.set_play_input_handler(Some(Box::new(move |play_input: PlayInput| {
	/// 	play_input_list_for_handler.borrow_mut().push(play_input);
	/// })));
	/// engine.get_input_manager_mut().set_view_input_position(Vector2::create(50.0, 60.0));
	/// engine.get_input_manager_mut().set_touch_pressed(true);
	/// game_scene_base.tick(0.016);
	/// engine.get_input_manager_mut().set_touch_pressed(false);
	/// engine.get_input_manager_mut().set_touch_released(true);
	/// game_scene_base.tick(0.016);
	/// assert_eq!(*play_input_list.borrow(), vec![PlayInput::Press(Vector2::create(50.0, 60.0)), PlayInput::Release(Vector2::create(50.0, 60.0))]);
	/// ```
	//==============================================================================
	pub fn set_play_input_handler(&mut self, play_input_handler: Option<PlayInputHandler>) {
		self.play_input_handler = play_input_handler;
		self.is_play_press_active = false;
	}

	//==============================================================================
	/// 모은 놀이 입력 꺼내기. (온 차례대로, 꺼내면 비운다)
	///
	/// # 돌려줌
	/// 모은 놀이 입력 목록. (모으기가 꺼져 있으면 빈 목록)
	//==============================================================================
	pub fn take_play_inputs(&mut self) -> Vec<PlayInput> {
		std::mem::take(&mut self.play_inputs)
	}

	//==============================================================================
	/// 뷰 좌표 → 게임 영역 좌표. (세이프 에어리어 왼쪽 위가 0, 0)
	///
	/// # 인자
	/// - `view_position`: 바꿀 자리. (뷰 좌표)
	///
	/// # 돌려줌
	/// 마지막으로 계산한 세이프 에어리어 왼쪽 위를 뺀 자리.
	//==============================================================================
	pub fn convert_view_to_play_area(&self, view_position: Vector2) -> Vector2 {
		let safe_area_position = self.last_safe_area_rect.position;
		Vector2::create(view_position.x - safe_area_position.x, view_position.y - safe_area_position.y)
	}

	//==============================================================================
	/// 게임 영역 기준 설정. (세이프 에어리어를 뺀 게임 영역의 고정 쪽 길이가 늘 fixed_length 가 되게 기준 해상도를 늘린다)
	/// - 캔버스 길이 : 세이프 에어리어를 뺀 길이 = 기준 해상도 길이 : fixed_length.
	/// - is_landscape 면 가로, 아니면 세로가 고정 쪽이다. 뷰 배율 방식도 그에 맞춰 둔다. (가로 고정 StretchWidthExpandHeight, 세로 고정 StretchHeightExpandWidth)
	/// - 무시할 세이프 에어리어 쪽은 ViewManager::set_ignored_safe_area_sides 로 넘긴다. fixed_length 가 0 이면 끈다.
	/// - 엔진을 주면 바로 건다. 씬은 resize 첫머리에 apply_play_area_reference 를 부른다.
	///
	/// # 인자
	/// - `fixed_length`: 세이프 에어리어를 뺀 게임 영역의 고정 쪽 길이. (뷰 좌표, 0 이하이면 끈다)
	/// - `is_landscape`: true 면 가로, false 면 세로가 고정 쪽이다.
	/// - `engine`: 바로 걸 엔진. None 이면 뷰 배율 방식만 정해 두고 initialize 에서 건다.
	///
	/// # 예제
	/// 세로 고정 720 이면 위아래 인셋을 뺀 세로가 늘 720 이다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, GameSceneBase, SafeAreaInsets, ViewScaleMode};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
	/// engine.get_view_manager_mut().set_safe_area_override(Some(SafeAreaInsets::create(40.0, 0.0, 30.0, 0.0)));
	/// let mut game_scene_base = GameSceneBase::new();
	/// game_scene_base.set_play_area_reference(720.0, false, Some(engine));
	/// assert_eq!(engine.get_view_manager().get_view_scale_mode(), ViewScaleMode::StretchHeightExpandWidth);
	/// assert_eq!(game_scene_base.get_play_area_fixed_length(), 720.0);
	/// game_scene_base.initialize(engine);
	/// game_scene_base.layout();
	/// assert!((game_scene_base.get_safe_area_rect().size.y - 720.0).abs() < 0.01);
	/// ```
	//==============================================================================
	pub fn set_play_area_reference(&mut self, fixed_length: f64, is_landscape: bool, engine: Option<&Engine>) {
		self.play_area_fixed_length = fixed_length.max(0.0);
		self.is_play_area_landscape = is_landscape;
		if self.play_area_fixed_length <= 0.0 {
			return;
		}
		let view_scale_mode = if is_landscape {
			ViewScaleMode::StretchWidthExpandHeight
		}
		else {
			ViewScaleMode::StretchHeightExpandWidth
		};
		match engine {
			Some(engine) => {
				self.set_view_scale_mode(view_scale_mode, Some(engine));
				self.apply_play_area_reference();
			}
			None => {
				self.view_scale_mode = view_scale_mode;
			}
		}
	}

	//==============================================================================
	/// 게임 영역의 고정 쪽 길이. (0 이면 쓰지 않음)
	///
	/// # 돌려줌
	/// 고정 쪽 길이. (뷰 좌표, 0 이면 게임 영역 기준을 쓰지 않는다)
	//==============================================================================
	pub fn get_play_area_fixed_length(&self) -> f64 {
		self.play_area_fixed_length
	}

	//==============================================================================
	/// 게임 영역 기준을 기준 해상도에 걸기. (캔버스와 인셋 크기에서만 셈해 몇 번을 불러도 같다, 바꿨으면 뷰 영역을 다시 셈하고 true)
	///
	/// # 돌려줌
	/// 기준 해상도를 바꿨으면 true. 게임 영역 기준을 쓰지 않거나, 캔버스 크기가 0 이거나, 셈할 수 없거나, 이미 같으면 false.
	//==============================================================================
	pub fn apply_play_area_reference(&mut self) -> bool {
		let engine = crate::core::globalengine::expect_global_engine("GameSceneBase::apply_play_area_reference");
		if self.play_area_fixed_length <= 0.0 {
			return false;
		}
		let mut view_manager = engine.get_view_manager_mut();
		let canvas_native_size = view_manager.get_canvas_native_size();
		if canvas_native_size.x <= 0.0 || canvas_native_size.y <= 0.0 {
			return false;
		}
		// 인셋은 지금 배율로 나눈 뷰 좌표로 나온다. 배율을 곱해 캔버스 단위로 되돌린다. (직접 정한 인셋은 CSS 픽셀 그대로라 배율 1)
		let (safe_area_insets, inset_scale) = match self.safe_area_insets {
			Some(insets) => {
				(insets, 1.0)
			}
			None => {
				let target_resolution_scale = view_manager.get_target_resolution_scale();
				let inset_scale = if target_resolution_scale > 0.0 {
					target_resolution_scale
				}
				else {
					1.0
				};
				(view_manager.get_safe_area_insets(), inset_scale)
			}
		};
		let reference_length = GameSceneBase::calculate_play_area_reference_length(self.play_area_fixed_length, self.is_play_area_landscape, canvas_native_size, safe_area_insets, inset_scale);
		if reference_length <= 0.0 {
			return false;
		}
		let reference_resolution_size = view_manager.get_reference_resolution_size();
		let current_reference_length = if self.is_play_area_landscape {
			reference_resolution_size.x
		}
		else {
			reference_resolution_size.y
		};
		if (current_reference_length - reference_length).abs() < 0.0001 {
			return false;
		}
		// 고정하는 쪽만 뷰 계산에 쓰인다. 다른 쪽에도 같은 값을 넣어 둔다.
		view_manager.apply_reference_resolution_size(Vector2::create(reference_length, reference_length));
		let canvas_pixel_size = view_manager.get_canvas_pixel_size();
		if canvas_pixel_size.x > 0.0 && canvas_pixel_size.y > 0.0 {
			let device_pixel_ratio = view_manager.get_device_pixel_ratio();
			view_manager.calculate_view_rect(canvas_pixel_size.x as u32, canvas_pixel_size.y as u32, device_pixel_ratio);
		}
		true
	}

	//==============================================================================
	/// 게임 영역 기준의 기준 해상도 길이 셈. (정적, 인셋은 뷰 좌표, inset_scale 은 뷰 좌표 → 캔버스 단위 배율, 못 셈하면 0)
	/// - fixed_length × 캔버스 길이 / (캔버스 길이 - 인셋 합 × inset_scale) 이다.
	///
	/// # 인자
	/// - `fixed_length`: 게임 영역의 고정 쪽 길이.
	/// - `is_landscape`: true 면 가로(왼쪽, 오른쪽 인셋), false 면 세로(위, 아래 인셋)로 셈한다.
	/// - `canvas_native_size`: 캔버스 논리 크기.
	/// - `safe_area_insets`: 세이프 에어리어 인셋. (뷰 좌표)
	/// - `inset_scale`: 인셋을 캔버스 단위로 바꾸는 배율.
	///
	/// # 돌려줌
	/// 고정 쪽의 기준 해상도 길이. 캔버스 길이나 인셋을 뺀 길이가 0 이하이면 0.
	///
	/// # 예제
	/// 세로 600 에서 인셋 70 을 빼면 기준 해상도를 그만큼 늘린다
	/// ```rust
	/// use vanilla::{GameSceneBase, SafeAreaInsets, Vector2};
	///
	/// let reference_length = GameSceneBase::calculate_play_area_reference_length(720.0, false, Vector2::create(800.0, 600.0), SafeAreaInsets::create(40.0, 0.0, 30.0, 0.0), 1.0);
	/// assert!((reference_length - 720.0 * 600.0 / 530.0).abs() < 0.01);
	/// let landscape_length = GameSceneBase::calculate_play_area_reference_length(1920.0, true, Vector2::create(1000.0, 500.0), SafeAreaInsets::create(0.0, 25.0, 0.0, 25.0), 2.0);
	/// assert!((landscape_length - 1920.0 * 1000.0 / 900.0).abs() < 0.01);
	/// assert_eq!(GameSceneBase::calculate_play_area_reference_length(720.0, false, Vector2::create(800.0, 0.0), SafeAreaInsets::create(0.0, 0.0, 0.0, 0.0), 1.0), 0.0);
	/// ```
	//==============================================================================
	pub fn calculate_play_area_reference_length(fixed_length: f64, is_landscape: bool, canvas_native_size: Vector2, safe_area_insets: SafeAreaInsets, inset_scale: f64) -> f64 {
		let (canvas_length, inset_sum) = if is_landscape {
			(canvas_native_size.x, safe_area_insets.left + safe_area_insets.right)
		}
		else {
			(canvas_native_size.y, safe_area_insets.top + safe_area_insets.bottom)
		};
		let safe_area_length = canvas_length - inset_sum * inset_scale;
		if safe_area_length <= 0.0 || canvas_length <= 0.0 {
			return 0.0;
		}
		fixed_length * canvas_length / safe_area_length
	}
}

impl Default for GameSceneBase {
	//==============================================================================
	/// initialize 전의 바탕. (new 와 같다)
	//==============================================================================
	fn default() -> GameSceneBase {
		GameSceneBase::new()
	}
}

//==============================================================================
/// 게임 씬. (원본 GameScene 을 상속한 씬이 덮어쓰는 자리)
/// - 씬은 GameSceneBase 를 품고 이 트레이트를 구현한 뒤 game_scene!(씬 타입) 으로 Scene 을 얻는다.
/// - game_scene! 이 만든 Scene 함수는 바탕(GameSceneBase)을 먼저 부르고 이 트레이트의 같은 이름 함수를 부른다. (원본에서 super 를 먼저 부르는 덮어쓰기)
///   post_draw 만 씬의 것을 먼저 부르고 바탕(페이드 막, 개발자 도구)을 위에 그린다.
/// - layout 은 바탕이 세이프 에어리어를 다시 계산한 뒤 부른다. (resize, 그리고 tick 이 뷰 크기 변화를 본 때. 첫 tick 에도 한 번)
/// - load_assets 는 load 에서 바탕의 로딩 이미지 요청 뒤에 부른다. 씬의 애셋을 요청하고 get_scene_base_mut().track_asset 으로 맡긴다.
///
/// # 예제
/// 바탕을 품고 덮어쓸 자리만 채운 뒤 game_scene! 한 줄로 Scene 을 얻기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, GameScene, GameSceneBase, ImageAsset, Scene};
///
/// struct TitleScene {
/// 	game_scene_base: GameSceneBase,
/// 	logo_asset: ImageAsset,
/// 	layout_count: u32,
/// }
///
/// impl GameScene for TitleScene {
/// 	fn get_game_scene_base(&self) -> &GameSceneBase {
/// 		&self.game_scene_base
/// 	}
///
/// 	fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
/// 		&mut self.game_scene_base
/// 	}
///
/// 	fn load_assets(&mut self) {
/// 		let engine = Engine::get_engine().unwrap();
/// 		self.logo_asset.load(engine, "examples/assets/atlas/icons.png");
/// 		self.game_scene_base.get_scene_base_mut().track_asset(&self.logo_asset);
/// 	}
///
/// 	fn initialize(&mut self, _engine: &Engine) {
/// 		let safe_area_id = self.game_scene_base.get_safe_area_node_id().expect("safe area node");
/// 		self.game_scene_base.get_scene_base_mut().get_node_tree_mut().create_child(safe_area_id, "logo");
/// 	}
///
/// 	fn layout(&mut self) {
/// 		self.layout_count += 1;
/// 	}
/// }
///
/// vanilla::game_scene!(TitleScene);
///
/// let engine = Engine::new(EngineConfiguration::new());
/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
/// let mut title_scene = TitleScene {
/// 	game_scene_base: GameSceneBase::new(),
/// 	logo_asset: ImageAsset::new(),
/// 	layout_count: 0,
/// };
/// title_scene.game_scene_base.set_loading_min_duration_ms(0.0);
/// Scene::create(&mut title_scene);
/// Scene::load(&mut title_scene, engine);
/// assert!(title_scene.get_scene_base().are_tracked_assets_finished());
/// Scene::initialize(&mut title_scene, engine);
/// Scene::tick(&mut title_scene, 0.016);
/// assert_eq!(title_scene.layout_count, 1);
/// ```
///
/// 엔진에 첫 씬으로 넘겨 돌리기 (창이 떠서 컴파일만)
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, GameScene, GameSceneBase, Vector2};
///
/// struct MainScene {
/// 	game_scene_base: GameSceneBase,
/// }
///
/// impl GameScene for MainScene {
/// 	fn get_game_scene_base(&self) -> &GameSceneBase {
/// 		&self.game_scene_base
/// 	}
///
/// 	fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
/// 		&mut self.game_scene_base
/// 	}
/// }
///
/// vanilla::game_scene!(MainScene);
///
/// let mut engine_configuration = EngineConfiguration::new();
/// engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
/// let engine = Engine::new(engine_configuration);
/// let mut game_scene_base = GameSceneBase::new();
/// game_scene_base.set_loading_image_url("examples/assets/atlas/icons.png");
/// let main_scene = MainScene {
/// 	game_scene_base,
/// };
/// if let Err(error) = engine.run(Box::new(main_scene)) {
/// 	eprintln!("{}", error);
/// }
/// ```
//==============================================================================
pub trait GameScene: 'static {
	//==============================================================================
	/// 바탕 반환.
	///
	/// # 돌려줌
	/// 씬이 품은 바탕.
	//==============================================================================
	fn get_game_scene_base(&self) -> &GameSceneBase;

	//==============================================================================
	/// 바탕 반환. (변경용)
	///
	/// # 돌려줌
	/// 씬이 품은 바탕. (변경용)
	//==============================================================================
	fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase;

	//==============================================================================
	/// 씬의 애셋 읽기. (원본 async loadAssets() 덮어쓰기, 바탕의 로딩 이미지 요청 뒤)
	/// - 엔진은 Engine::get_engine() 으로 꺼낸다. await 로 읽으려면 engine.spawn_scene_task(self, ...) 로 불러오기 작업을 맡기고 get_game_scene_base_mut().get_scene_base_mut().track_task 로 맡긴다.
	//==============================================================================
	fn load_assets(&mut self) {
	}

	//==============================================================================
	/// 어두워졌다 밝아지기의 async 꼴. (원본 fadeScreen(async () => { ... }), 이미 페이드 중이면 false)
	/// - 다 어두워진 tick 에서 on_dark 가 돌려준 퓨처를 이 씬의 작업으로 맡겨 그 자리에서 첫 await 까지 돌리고, 끝나면 밝아진다.
	///   퓨처 안에서는 SceneContext::with 로 이 씬과 엔진을 빌린다.
	///
	/// # 인자
	/// - `on_dark`: 다 어두워졌을 때 `SceneContext<Self>` 를 받아 퓨처를 돌려주는 함수. (퓨처가 끝나거나 버려지면 밝아진다)
	///
	/// # 돌려줌
	/// 페이드를 시작했으면 true. 이미 페이드 중이면 false.
	///
	/// # 예제
	/// 누르면 어두워진 뒤 다음 판으로 넘기기 (game_scene! 이 시작을 맡는다)
	/// ```rust,no_run
	/// use vanilla::{Engine, GameScene, GameSceneBase, SceneContext, Vector2};
	///
	/// struct StageScene {
	/// 	game_scene_base: GameSceneBase,
	/// 	stage_number: u32,
	/// }
	///
	/// impl GameScene for StageScene {
	/// 	fn get_game_scene_base(&self) -> &GameSceneBase {
	/// 		&self.game_scene_base
	/// 	}
	///
	/// 	fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
	/// 		&mut self.game_scene_base
	/// 	}
	///
	/// 	fn touch_press(&mut self, _view_input_position: Vector2) {
	/// 		self.fade_screen_async(|mut scene_context: SceneContext<StageScene>| async move {
	/// 			scene_context.with(|scene, _engine| {
	/// 				scene.stage_number += 1;
	/// 			});
	/// 		});
	/// 	}
	/// }
	///
	/// vanilla::game_scene!(StageScene);
	/// ```
	//==============================================================================
	fn fade_screen_async<F, OnDarkFuture>(&mut self, on_dark: F) -> bool
	where
		Self: Scene + Sized,
		F: FnOnce(SceneContext<Self>) -> OnDarkFuture + 'static,
		OnDarkFuture: Future<Output = ()> + 'static,
	{
		self.get_game_scene_base_mut().fade_screen_async::<Self, F, OnDarkFuture>(on_dark)
	}

	//==============================================================================
	/// 로딩 화면 위에 더 그리기. (바탕의 로딩 화면 뒤)
	///
	/// # 인자
	/// - `_graphic`: 그릴 그래픽. (뷰 좌표계가 걸려 있다)
	//==============================================================================
	fn draw_on_load(&mut self, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 초기화. (바탕 initialize 뒤, 세이프 에어리어 노드가 생긴 다음)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn initialize(&mut self, _engine: &Engine) {
	}

	//==============================================================================
	/// 소멸.
	//==============================================================================
	fn destroy(&mut self) {
	}

	//==============================================================================
	/// 레이아웃. (원본 layout 덮어쓰기, 바탕이 세이프 에어리어를 다시 계산한 뒤)
	//==============================================================================
	fn layout(&mut self) {
	}

	//==============================================================================
	/// 갱신. (바탕 tick 뒤: 노드, 터치, 트윈, 레이아웃, 페이드, 개발자 도구)
	///
	/// # 인자
	/// - `_time_delta`: 배율을 곱한 프레임 간격. (초)
	//==============================================================================
	fn tick(&mut self, _time_delta: f64) {
	}

	//==============================================================================
	/// 이전 출력. (바탕의 배경 칠하기 뒤)
	///
	/// # 인자
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn pre_draw(&mut self, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 출력. (노드 트리를 그린 뒤)
	///
	/// # 인자
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn draw(&mut self, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 이후 출력. (바탕의 페이드 막과 개발자 도구보다 먼저)
	///
	/// # 인자
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn post_draw(&mut self, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 종료처리. (내릴 때 unload 앞, 원본 Scene.finalize 덮어쓰기)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn finalize(&mut self, _engine: &Engine) {
	}

	//==============================================================================
	/// 로딩 해제. (내릴 때 finalize 뒤, 원본 Scene.unload 덮어쓰기)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn unload(&mut self, _engine: &Engine) {
	}

	//==============================================================================
	/// 페이지 프로세스 복원과 가시성 복원. (원본 onPageProcessRestored, onPageVisibilityRestored 덮어쓰기)
	/// - 이 함수는 페이지 프로세스 복원 자리다. 가시성 복원은 on_page_visibility_restored.
	//==============================================================================
	fn on_page_process_restored(&mut self) {
	}

	//==============================================================================
	/// 페이지 가시성 복원. (원본 onPageVisibilityRestored 덮어쓰기)
	//==============================================================================
	fn on_page_visibility_restored(&mut self) {
	}

	//==============================================================================
	/// 씬의 터치 훅. (원본 touchPress, touchMove, touchRelease, touchCancel, touchWheel 덮어쓰기, 바탕이 UI 와 놀이 입력에 넘긴 뒤 엔진이 부른다)
	/// - 이 함수는 누름 자리다. 이동, 뗌, 취소, 휠은 touch_move, touch_release, touch_cancel, touch_wheel.
	///
	/// # 인자
	/// - `_view_input_position`: 누른 자리. (뷰 좌표)
	//==============================================================================
	fn touch_press(&mut self, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 이동. (원본 touchMove 덮어쓰기)
	///
	/// # 인자
	/// - `_view_input_position`: 지금 자리. (뷰 좌표)
	//==============================================================================
	fn touch_move(&mut self, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 뗌. (원본 touchRelease 덮어쓰기)
	///
	/// # 인자
	/// - `_view_input_position`: 뗀 자리. (뷰 좌표)
	//==============================================================================
	fn touch_release(&mut self, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 취소. (원본 touchCancel 덮어쓰기)
	///
	/// # 인자
	/// - `_view_input_position`: 취소된 자리. (뷰 좌표)
	//==============================================================================
	fn touch_cancel(&mut self, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 휠. (원본 touchWheel 덮어쓰기)
	///
	/// # 인자
	/// - `_view_input_position`: 휠을 굴린 자리. (뷰 좌표)
	/// - `_wheel_delta`: 휠 값. (가로, 세로)
	//==============================================================================
	fn touch_wheel(&mut self, _view_input_position: Vector2, _wheel_delta: Vector2) {
	}
}

//==============================================================================
/// GameScene 을 구현한 씬에 Scene 을 만들어 준다. (vanilla::game_scene!(TitleScene);)
//==============================================================================
#[macro_export]
macro_rules! game_scene {
	($scene_type:ty) => {
		impl $crate::core::scene::Scene for $scene_type {
			fn get_scene_base(&self) -> &$crate::core::scene::SceneBase {
				$crate::game::gamescene::GameScene::get_game_scene_base(self).get_scene_base()
			}

			fn get_scene_base_mut(&mut self) -> &mut $crate::core::scene::SceneBase {
				$crate::game::gamescene::GameScene::get_game_scene_base_mut(self).get_scene_base_mut()
			}

			fn load(&mut self, engine: &$crate::core::engine::Engine) {
				$crate::game::gamescene::GameScene::get_game_scene_base_mut(self).load(engine);
				$crate::game::gamescene::GameScene::load_assets(self);
			}

			fn draw_on_load(&mut self, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::game::gamescene::GameScene::get_game_scene_base_mut(self).draw_on_load(graphic);
				$crate::game::gamescene::GameScene::draw_on_load(self, graphic);
			}

			fn initialize(&mut self, engine: &$crate::core::engine::Engine) {
				$crate::game::gamescene::GameScene::get_game_scene_base_mut(self).initialize(engine);
				$crate::game::gamescene::GameScene::initialize(self, engine);
			}

			fn destroy(&mut self) {
				$crate::game::gamescene::GameScene::destroy(self);
			}

			fn finalize(&mut self, engine: &$crate::core::engine::Engine) {
				$crate::game::gamescene::GameScene::finalize(self, engine);
			}

			fn unload(&mut self, engine: &$crate::core::engine::Engine) {
				$crate::game::gamescene::GameScene::unload(self, engine);
			}

			fn on_page_process_restored(&mut self) {
				$crate::game::gamescene::GameScene::on_page_process_restored(self);
			}

			fn on_page_visibility_restored(&mut self) {
				$crate::game::gamescene::GameScene::on_page_visibility_restored(self);
			}

			fn touch_press(&mut self, view_input_position: $crate::base::vector2::Vector2) {
				$crate::game::gamescene::GameScene::touch_press(self, view_input_position);
			}

			fn touch_move(&mut self, view_input_position: $crate::base::vector2::Vector2) {
				$crate::game::gamescene::GameScene::touch_move(self, view_input_position);
			}

			fn touch_release(&mut self, view_input_position: $crate::base::vector2::Vector2) {
				$crate::game::gamescene::GameScene::touch_release(self, view_input_position);
			}

			fn touch_cancel(&mut self, view_input_position: $crate::base::vector2::Vector2) {
				$crate::game::gamescene::GameScene::touch_cancel(self, view_input_position);
			}

			fn touch_wheel(&mut self, view_input_position: $crate::base::vector2::Vector2, wheel_delta: $crate::base::vector2::Vector2) {
				$crate::game::gamescene::GameScene::touch_wheel(self, view_input_position, wheel_delta);
			}

			fn resize(&mut self, canvas_native_size: $crate::base::vector2::Vector2) {
				$crate::game::gamescene::GameScene::get_game_scene_base_mut(self).resize(canvas_native_size);
				$crate::game::gamescene::GameScene::layout(self);
			}

			fn tick(&mut self, time_delta: f64) {
				if $crate::game::gamescene::GameScene::get_game_scene_base_mut(self).tick(time_delta) {
					$crate::game::gamescene::GameScene::layout(self);
				}
				if let Some(screen_fade_dark_starter) = $crate::game::gamescene::GameScene::get_game_scene_base_mut(self).take_screen_fade_dark_starter() {
					screen_fade_dark_starter.start(self);
				}
				$crate::game::gamescene::GameScene::tick(self, time_delta);
			}

			fn pre_draw(&mut self, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::game::gamescene::GameScene::get_game_scene_base(self).pre_draw(graphic);
				$crate::game::gamescene::GameScene::pre_draw(self, graphic);
			}

			fn draw(&mut self, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::game::gamescene::GameScene::get_game_scene_base_mut(self).get_scene_base_mut().draw(graphic);
				$crate::game::gamescene::GameScene::draw(self, graphic);
			}

			fn post_draw(&mut self, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::game::gamescene::GameScene::post_draw(self, graphic);
				$crate::game::gamescene::GameScene::get_game_scene_base_mut(self).post_draw(graphic);
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::asset::AssetLoadHandle;
	use crate::core::engine::EngineConfiguration;
	use crate::core::viewmanager::SafeAreaSide;

	//==============================================================================
	/// 찍기 모드의 프레임 세우기: 실제 시간이 드는 맡긴 애셋이 남은 동안만 세우고, 엔진 시계로 열리는 로딩 문은 기다리지 않는다. (원본 Scene.isCaptureFrameHeld 와 같다)
	//==============================================================================
	#[test]
	fn capture_frame_is_held_only_while_real_time_loads_pending() {
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_loading_min_duration_ms(3000.0);
		let other_asset_handle = AssetLoadHandle::create_pending("other.png");
		game_scene_base.begin_loading(0.0);
		game_scene_base.get_scene_base_mut().track_load_handle(&other_asset_handle);
		assert!(game_scene_base.get_scene_base().is_capture_frame_held());
		other_asset_handle.finish(Ok(vec![1, 2, 3]));
		// 로딩 문은 아직 닫혀 있지만(최소 노출 시간 전) 엔진 시계로 열리므로 세우지 않는다.
		assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());
		assert!(!game_scene_base.get_scene_base().is_capture_frame_held());
		game_scene_base.get_scene_base_mut().set_loaded(true);
		assert!(!game_scene_base.get_scene_base().is_capture_frame_held());
	}

	//==============================================================================
	/// 로딩 문은 최소 노출 시간이 지나야 열리고, 게이지는 경과 시간 비율이다. 다른 애셋이 남아 있으면 씬은 계속 기다린다.
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
	/// 화면을 누르면 최소 시간 전이라도 로딩 문이 열린다. 최소 시간이 0 이면 처음부터 열려 있다.
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
	/// 뷰 크기가 바뀌면 tick 이 세이프 에어리어를 다시 계산해 노드에 걸고 true 를 돌려준다. (인셋은 뷰 배율로 나눈다)
	//==============================================================================
	#[test]
	fn layout_follows_view_size_changes() {
		let engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(10.0, 0.0, 20.0, 5.0)));
		game_scene_base.initialize(engine);
		assert_eq!(engine.get_view_manager().get_view_scale_mode(), ViewScaleMode::None);
		assert_eq!(engine.get_view_manager().get_view_size(), Vector2::create(800.0, 600.0));
		let safe_area_node_id = game_scene_base.get_safe_area_node_id().expect("safe area node");
		assert_eq!(game_scene_base.get_scene_base().get_node_tree()[safe_area_node_id].get_name(), "safeArea");

		assert!(game_scene_base.tick(0.016));
		assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 795.0, 570.0));
		let safe_area_node = &game_scene_base.get_scene_base().get_node_tree()[safe_area_node_id];
		assert_eq!(safe_area_node.get_local_position(), Vector2::create(5.0, 10.0));
		assert_eq!(safe_area_node.get_content_size(), Vector2::create(795.0, 570.0));
		assert!(!game_scene_base.tick(0.016));

		engine.get_view_manager_mut().calculate_view_rect(1024, 768, 1.0);
		assert!(game_scene_base.tick(0.016));
		assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 1019.0, 738.0));
		assert!(!game_scene_base.tick(0.016));

		game_scene_base.set_view_scale_mode(ViewScaleMode::ReferenceResolution, Some(engine));
		assert_eq!(engine.get_view_manager().get_view_size(), Vector2::create(1280.0, 720.0));
		assert!(game_scene_base.tick(0.016));
		assert_eq!(game_scene_base.get_safe_area_rect(), Rect::create(5.0, 10.0, 1275.0, 690.0));
	}

	//==============================================================================
	/// 세이프 에어리어 계산은 뷰 배율로 인셋을 나누고 음수 크기를 0 으로 막는다.
	//==============================================================================
	#[test]
	fn safe_area_calculation() {
		let safe_area_rect = GameSceneBase::calculate_safe_area_rect(Vector2::create(400.0, 300.0), 2.0, SafeAreaInsets::create(40.0, 10.0, 20.0, 30.0));
		assert_eq!(safe_area_rect, Rect::create(15.0, 20.0, 380.0, 270.0));
		let collapsed_rect = GameSceneBase::calculate_safe_area_rect(Vector2::create(10.0, 10.0), 0.0, SafeAreaInsets::create(20.0, 20.0, 20.0, 20.0));
		assert_eq!(collapsed_rect, Rect::create(20.0, 20.0, 0.0, 0.0));
	}

	//==============================================================================
	/// 한 프레임 진행. (시간을 seconds 만큼 흘리고 tick)
	//==============================================================================
	fn step_frame(engine: &Engine, game_scene_base: &mut GameSceneBase, current_seconds: &mut f64, seconds: f64) {
		*current_seconds += seconds;
		engine.get_time_manager_mut().update(*current_seconds);
		game_scene_base.tick(seconds);
	}

	//==============================================================================
	/// 누름을 한 프레임 넣고 다음 프레임에 뗀다.
	//==============================================================================
	fn press_and_release(engine: &Engine, game_scene_base: &mut GameSceneBase, current_seconds: &mut f64, position: Vector2) {
		engine.get_input_manager_mut().set_view_input_position(position);
		engine.get_input_manager_mut().set_touch_pressed(true);
		step_frame(engine, game_scene_base, current_seconds, 0.016);
		engine.get_input_manager_mut().set_touch_pressed(false);
		engine.get_input_manager_mut().set_touch_released(true);
		step_frame(engine, game_scene_base, current_seconds, 0.016);
		engine.get_input_manager_mut().set_touch_released(false);
	}

	//==============================================================================
	/// 페이드: 다 어두워지면 on_dark, 손잡이를 놓을 때까지 어둡고, 그동안 누름은 막히고, 놓으면 밝아진다. 페이드 중 다시 부르면 false.
	//==============================================================================
	#[test]
	fn screen_fade_holds_until_release_and_blocks_press() {
		let engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(0.0, 0.0, 0.0, 0.0)));
		game_scene_base.initialize(engine);
		game_scene_base.set_play_input_enabled(true);
		game_scene_base.set_screen_fade_seconds(0.2);
		let mut current_seconds = 1.0;
		engine.get_time_manager_mut().update(current_seconds);
		let screen_fade_hold = ScreenFadeHold::new();
		let returned_hold = screen_fade_hold.clone();
		let dark_count = Rc::new(Cell::new(0));
		let dark_count_event = dark_count.clone();
		assert!(game_scene_base.fade_screen(Some(Box::new(move |_node_tree: &mut NodeTree, _engine: &Engine| {
			dark_count_event.set(dark_count_event.get() + 1);
			Some(returned_hold)
		}))));
		assert!(!game_scene_base.fade_screen(None));
		step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.1);
		assert!((game_scene_base.get_screen_fade_alpha() - 0.5).abs() < 0.001);
		assert_eq!(dark_count.get(), 0);
		step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.1);
		assert_eq!(game_scene_base.get_screen_fade_alpha(), 1.0);
		assert_eq!(dark_count.get(), 1);
		step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.1);
		assert_eq!(game_scene_base.get_screen_fade_alpha(), 1.0);
		press_and_release(engine, &mut game_scene_base, &mut current_seconds, Vector2::create(100.0, 100.0));
		assert!(game_scene_base.take_play_inputs().is_empty());
		screen_fade_hold.release();
		for _ in 0..4 {
			step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.1);
		}
		assert_eq!(game_scene_base.get_screen_fade_alpha(), 0.0);
		assert!(!game_scene_base.is_screen_fading());
		press_and_release(engine, &mut game_scene_base, &mut current_seconds, Vector2::create(100.0, 100.0));
		assert_eq!(game_scene_base.take_play_inputs().first(), Some(&PlayInput::Press(Vector2::create(100.0, 100.0))));
	}

	//==============================================================================
	/// 놀이 입력: UI 가 받지 않은 누름만 게임 영역 좌표로, 그 누름의 뗌까지. UI 위 누름은 넘기지 않는다.
	//==============================================================================
	#[test]
	fn play_input_skips_user_interface() {
		let engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(10.0, 0.0, 0.0, 20.0)));
		game_scene_base.initialize(engine);
		let mut current_seconds = 1.0;
		step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.016);
		game_scene_base.set_play_input_enabled(true);
		let root_id = game_scene_base.get_scene_base().get_node_tree().get_root_id();
		let button_id = game_scene_base.get_scene_base_mut().get_node_tree_mut().create_child(root_id, "button");
		{
			let button_node = &mut game_scene_base.get_scene_base_mut().get_node_tree_mut()[button_id];
			button_node.set_pivot(Pivot::TOP_LEFT);
			button_node.set_content_size(Vector2::create(100.0, 100.0));
			button_node.set_local_position(Vector2::create(600.0, 400.0));
			button_node.set_interactable(true);
		}
		assert!(game_scene_base.is_user_interface_at(Vector2::create(650.0, 450.0)));
		assert!(!game_scene_base.is_user_interface_at(Vector2::create(120.0, 110.0)));
		press_and_release(engine, &mut game_scene_base, &mut current_seconds, Vector2::create(120.0, 110.0));
		let play_inputs = game_scene_base.take_play_inputs();
		assert_eq!(play_inputs.first(), Some(&PlayInput::Press(Vector2::create(100.0, 100.0))));
		assert_eq!(play_inputs.last(), Some(&PlayInput::Release(Vector2::create(100.0, 100.0))));
		press_and_release(engine, &mut game_scene_base, &mut current_seconds, Vector2::create(650.0, 450.0));
		assert!(game_scene_base.take_play_inputs().is_empty());
		engine.get_input_manager_mut().add_wheel_delta(0.0, 120.0);
		step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.016);
		assert!(game_scene_base.take_play_inputs().is_empty());
		engine.get_input_manager_mut().clear_wheel_delta();
		engine.get_input_manager_mut().set_view_input_position(Vector2::create(120.0, 110.0));
		engine.get_input_manager_mut().add_wheel_delta(0.0, 120.0);
		step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.016);
		assert_eq!(game_scene_base.take_play_inputs(), vec![PlayInput::Wheel(Vector2::create(100.0, 100.0), Vector2::create(0.0, 120.0))]);
	}

	//==============================================================================
	/// 게임 영역 기준: 세이프 에어리어를 뺀 세로가 늘 고정 길이가 되게 기준 해상도를 늘린다. 무시한 쪽은 빼지 않는다.
	//==============================================================================
	#[test]
	fn play_area_reference_keeps_fixed_length() {
		let engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		let mut game_scene_base = GameSceneBase::new();
		engine.get_view_manager_mut().set_safe_area_override(Some(SafeAreaInsets::create(40.0, 0.0, 30.0, 0.0)));
		game_scene_base.set_play_area_reference(720.0, false, Some(engine));
		assert_eq!(engine.get_view_manager().get_view_scale_mode(), ViewScaleMode::StretchHeightExpandWidth);
		game_scene_base.initialize(engine);
		game_scene_base.layout();
		assert!((game_scene_base.get_safe_area_rect().size.y - 720.0).abs() < 0.01);
		assert!((engine.get_view_manager().get_reference_resolution_size().y - 720.0 * 600.0 / 530.0).abs() < 0.01);
		engine.get_view_manager_mut().set_ignored_safe_area_sides(&[SafeAreaSide::Top]);
		assert!(game_scene_base.apply_play_area_reference());
		game_scene_base.layout();
		assert!((game_scene_base.get_safe_area_rect().size.y - 720.0).abs() < 0.01);
		assert_eq!(game_scene_base.get_safe_area_rect().position.y, 0.0);
		assert!(!game_scene_base.apply_play_area_reference());
		assert!((GameSceneBase::calculate_play_area_reference_length(1920.0, true, Vector2::create(1000.0, 500.0), SafeAreaInsets::create(0.0, 25.0, 0.0, 25.0), 2.0) - 1920.0 * 1000.0 / 900.0).abs() < 0.01);
	}

	//==============================================================================
	/// 맡긴 애셋 함께 받기: 다 받을 때까지 씬이 기다리고, 진행률을 알린다. 알리기 전 게이지는 경과 시간이다.
	//==============================================================================
	#[test]
	fn tracked_asset_loader_reports_progress() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_loading_min_duration_ms(0.0);
		game_scene_base.begin_loading(0.0);
		assert!(!game_scene_base.is_loading_progress_reported);
		let asset_loader = AssetLoader::new();
		let first_handle = AssetLoadHandle::create_pending("first");
		let second_handle = AssetLoadHandle::create_pending("second");
		asset_loader.add_load_handle(&first_handle);
		asset_loader.add_load_handle(&second_handle);
		game_scene_base.track_asset_loader(&asset_loader);
		asset_loader.load(&engine);
		assert!(game_scene_base.is_loading_progress_reported);
		assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());

		first_handle.finish(Ok(Vec::new()));
		game_scene_base.update_tracked_asset_loaders(&engine);
		assert_eq!(game_scene_base.get_loading_progress_state().get_progress(), 0.5);
		assert!(!game_scene_base.get_scene_base().are_tracked_assets_finished());
		second_handle.finish(Err(String::from("fail")));
		game_scene_base.update_tracked_asset_loaders(&engine);
		assert_eq!(game_scene_base.get_loading_progress_state().get_progress(), 1.0);
		assert!(game_scene_base.get_scene_base().are_tracked_assets_finished());

		let mut colored_scene_base = GameSceneBase::new();
		colored_scene_base.set_loading_colors("#16324a", "#0b1a27", "#ffcc33");
		assert_eq!(colored_scene_base.loading_outside_color, Color::create_from_hex("#16324a"));
		colored_scene_base.set_loading_outside_color("#05090d");
		assert_eq!(colored_scene_base.loading_outside_color, Color::create_from_hex("#05090d"));
		colored_scene_base.set_loading_drawer(Some(Box::new(|_graphic, _view_size, _progress, _elapsed_seconds| {})));
		assert!(colored_scene_base.has_loading_drawer());
		assert!(colored_scene_base.get_loading_drawer().is_some());
		colored_scene_base.set_loading_progress(0.25);
		assert_eq!(colored_scene_base.get_loading_progress_state().get_progress(), 0.25);

		// 원본처럼 로딩 문을 뺀 애셋이 다 끝나면 1. (로딩 문이 아직 닫혀 있어도)
		let mut reported_scene_base = GameSceneBase::new();
		reported_scene_base.begin_loading(0.0);
		let scene_asset_handle = AssetLoadHandle::create_pending("scene");
		reported_scene_base.get_scene_base_mut().track_load_handle(&scene_asset_handle);
		reported_scene_base.set_loading_progress(0.3);
		reported_scene_base.complete_loading_progress_if_assets_finished();
		assert_eq!(reported_scene_base.get_loading_progress_state().get_progress(), 0.3);
		scene_asset_handle.finish(Ok(Vec::new()));
		reported_scene_base.complete_loading_progress_if_assets_finished();
		assert_eq!(reported_scene_base.get_loading_progress_state().get_progress(), 1.0);
		assert!(!reported_scene_base.get_scene_base().are_tracked_assets_finished());
	}

	struct LayoutCountScene {
		game_scene_base: GameSceneBase,
		layout_count: u32,
		tick_count: u32,
	}

	impl GameScene for LayoutCountScene {
		//==============================================================================
		/// 바탕.
		//==============================================================================
		fn get_game_scene_base(&self) -> &GameSceneBase {
			&self.game_scene_base
		}

		//==============================================================================
		/// 바탕. (변경용)
		//==============================================================================
		fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
			&mut self.game_scene_base
		}

		//==============================================================================
		/// 레이아웃 수 세기.
		//==============================================================================
		fn layout(&mut self) {
			self.layout_count += 1;
		}

		//==============================================================================
		/// 갱신 수 세기.
		//==============================================================================
		fn tick(&mut self, _time_delta: f64) {
			self.tick_count += 1;
		}
	}

	crate::game_scene!(LayoutCountScene);

	//==============================================================================
	/// game_scene! 으로 만든 씬의 initialize, tick, resize 가 바탕과 씬의 훅을 부르는지.
	//==============================================================================
	#[test]
	fn game_scene_macro_calls_base_and_hooks() {
		use crate::core::scene::Scene;
		let engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		engine.get_view_manager_mut().set_safe_area_override(Some(SafeAreaInsets::create(40.0, 0.0, 30.0, 0.0)));
		let mut scene = LayoutCountScene {
			game_scene_base: GameSceneBase::new(),
			layout_count: 0,
			tick_count: 0,
		};
		scene.game_scene_base.set_play_area_reference(720.0, false, None);
		Scene::initialize(&mut scene, engine);
		assert!(scene.game_scene_base.get_dev_tools().is_some());
		Scene::tick(&mut scene, 0.016);
		assert_eq!(scene.layout_count, 1);
		assert_eq!(scene.tick_count, 1);
		Scene::tick(&mut scene, 0.016);
		assert_eq!(scene.layout_count, 1);
		Scene::resize(&mut scene, Vector2::create(800.0, 600.0));
		assert_eq!(scene.layout_count, 2);
		assert!((scene.game_scene_base.get_safe_area_rect().size.y - 720.0).abs() < 0.01);
	}

	//==============================================================================
	/// 놀이 입력 처리기: UI 가 받지 않은 누름과 그 뗌을 처리기로 받는다. 페이드 콜백 꼴: 받은 손잡이를 놓을 때까지 어둡다.
	//==============================================================================
	#[test]
	fn play_input_handler_and_fade_with_hold() {
		let engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(0.0, 0.0, 0.0, 0.0)));
		game_scene_base.initialize(engine);
		let received_inputs: Rc<std::cell::RefCell<Vec<PlayInput>>> = Rc::new(std::cell::RefCell::new(Vec::new()));
		let received_inputs_handler = received_inputs.clone();
		game_scene_base.set_play_input_handler(Some(Box::new(move |play_input: PlayInput| {
			received_inputs_handler.borrow_mut().push(play_input);
		})));
		let mut current_seconds = 1.0;
		press_and_release(engine, &mut game_scene_base, &mut current_seconds, Vector2::create(50.0, 60.0));
		assert_eq!(*received_inputs.borrow(), vec![PlayInput::Press(Vector2::create(50.0, 60.0)), PlayInput::Release(Vector2::create(50.0, 60.0))]);
		assert!(game_scene_base.take_play_inputs().is_empty());

		game_scene_base.set_screen_fade_seconds(0.1);
		let held_hold: Rc<std::cell::RefCell<Option<ScreenFadeHold>>> = Rc::new(std::cell::RefCell::new(None));
		let held_hold_event = held_hold.clone();
		assert!(game_scene_base.fade_screen_with_hold(Box::new(move |_node_tree: &mut NodeTree, _engine: &Engine, screen_fade_hold: ScreenFadeHold| {
			*held_hold_event.borrow_mut() = Some(screen_fade_hold);
		})));
		for _ in 0..3 {
			step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.1);
		}
		assert!(held_hold.borrow().is_some());
		assert_eq!(game_scene_base.get_screen_fade_alpha(), 1.0);
		if let Some(screen_fade_hold) = held_hold.borrow().as_ref() {
			screen_fade_hold.release();
		}
		for _ in 0..3 {
			step_frame(engine, &mut game_scene_base, &mut current_seconds, 0.1);
		}
		assert!(!game_scene_base.is_screen_fading());
	}

	//==============================================================================
	/// async 페이드: 다 어두워진 tick 에서 on_dark 의 퓨처가 씬 작업으로 맡겨져 그 자리에서 첫 await 까지 돌고(game_scene! 이 시작한다),
	/// 퓨처가 await 하는 동안은 어둡고, 끝나면 밝아진다. 페이드 중 다시 부르면 false. 씬이 꺼내지 않으면 바탕이 다음 tick 에 다음 폴링 차례로 맡긴다.
	//==============================================================================
	#[test]
	fn fade_screen_async_starts_on_dark_tick_and_holds_until_future_finishes() {
		use crate::core::scene::Scene;
		let engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		let mut scene = LayoutCountScene {
			game_scene_base: GameSceneBase::new(),
			layout_count: 0,
			tick_count: 0,
		};
		scene.get_scene_base_mut().set_scene_id(SceneId(5));
		scene.game_scene_base.set_safe_area_insets(Some(SafeAreaInsets::create(0.0, 0.0, 0.0, 0.0)));
		Scene::initialize(&mut scene, engine);
		scene.game_scene_base.set_screen_fade_seconds(0.1);
		let (finish_promise, finish_resolver) = crate::core::task::Promise::create();
		assert!(scene.fade_screen_async(move |mut scene_context: SceneContext<LayoutCountScene>| async move {
			scene_context.with(|scene, _engine| {
				scene.layout_count += 1000;
			});
			finish_promise.await;
			scene_context.with(|scene, _engine| {
				scene.layout_count += 100000;
			});
		}));
		assert!(!scene.fade_screen_async(|_scene_context| async {}));
		let mut scene_list: Vec<Box<dyn Scene>> = vec![Box::new(scene)];
		let mut current_seconds = 1.0;
		engine.get_time_manager_mut().update(current_seconds);
		let mut step_scene_frame = |engine: &Engine, scene_list: &mut Vec<Box<dyn Scene>>| -> (f64, u32) {
			current_seconds += 0.06;
			engine.get_time_manager_mut().update(current_seconds);
			task::poll_tasks(engine, scene_list, &mut Vec::new());
			scene_list[0].tick(0.06);
			let scene = scene_list[0].as_scene_any_mut().downcast_mut::<LayoutCountScene>().expect("시험용 씬");
			(scene.game_scene_base.get_screen_fade_alpha(), scene.layout_count)
		};
		let (alpha, layout_count) = step_scene_frame(engine, &mut scene_list);
		assert!(alpha < 1.0);
		assert!(layout_count < 1000);
		let (alpha, layout_count) = step_scene_frame(engine, &mut scene_list);
		assert_eq!(alpha, 1.0);
		assert!(layout_count >= 1000 && layout_count < 100000);
		for _ in 0..3 {
			let (alpha, layout_count) = step_scene_frame(engine, &mut scene_list);
			assert_eq!(alpha, 1.0);
			assert!(layout_count < 100000);
		}
		finish_resolver.resolve(());
		let (alpha, layout_count) = step_scene_frame(engine, &mut scene_list);
		assert!(layout_count >= 100000);
		assert_eq!(alpha, 1.0);
		let (alpha, _) = step_scene_frame(engine, &mut scene_list);
		assert!(alpha < 1.0);
		for _ in 0..3 {
			step_scene_frame(engine, &mut scene_list);
		}
		let scene = scene_list[0].as_scene_any_mut().downcast_mut::<LayoutCountScene>().expect("시험용 씬");
		assert!(!scene.game_scene_base.is_screen_fading());
		assert_eq!(engine.task_executor.get_task_count(), 0);

		// 바탕만 직접 돌려 시작을 꺼내지 않으면 다음 tick 에 다음 폴링 차례로 맡긴다.
		let started = Rc::new(Cell::new(false));
		let task_started = started.clone();
		assert!(scene.game_scene_base.fade_screen_async::<LayoutCountScene, _, _>(move |_scene_context| async move {
			task_started.set(true);
		}));
		for _ in 0..2 {
			current_seconds += 0.06;
			engine.get_time_manager_mut().update(current_seconds);
			scene.game_scene_base.tick(0.06);
		}
		assert!(!started.get());
		scene.game_scene_base.tick(0.06);
		let mut moved_scene_list: Vec<Box<dyn Scene>> = vec![scene_list.remove(0)];
		task::poll_tasks(engine, &mut moved_scene_list, &mut Vec::new());
		assert!(started.get());
	}
}
