//==============================================================================
//! 엔진.
//! - 원본은 캔버스를 잡고 request_animation_frame 으로 프레임을 돈다.
//!   러스트 판은 창, GL 컨텍스트, 입력 이벤트, 화면 넘기기를 플랫폼 층(crate::platform)이 맡고,
//!   여기 EngineRunner 는 플랫폼을 모른 채 씬과 프레임 흐름만 돈다. (데스크탑: platform/desktop.rs, 웹: platform/web.rs)
//! - 씬에는 &mut Engine 을 넘긴다. 씬은 여기서 매니저(시간, 뷰, 입력)를 꺼내 쓴다. (원본 Engine.getEngine() 자리)
//! - Graphic 은 플랫폼이 GL 컨텍스트를 만든 뒤 attach_graphic 으로 넘겨받고, 씬의 draw 계열에 따로 넘긴다.
//! - 오디오 매니저를 원본처럼 생성할 때 만들어 든다. AudioContext(소리 장치)는 처음 쓸 때 만든다. 프레임마다 하는 오디오 일은 없다.
//!   웹은 플랫폼이 click / touchstart / keydown 에서 resume_on_user_gesture, focus / visibilitychange 에서 resume 을 부른다.
//! - 원본 SceneManager.getAllLoadedScenes 는 get_all_loaded_scenes 로, 씬 객체 대신 핸들(SceneId)을 돌려준다. (씬마다 &mut Engine 을 받아 도는 동안 다른 씬을 빌릴 수 없다)
//! - 원본처럼 씬을 불러오면 루트를 뷰 크기로 맞추고(create 바로 뒤), 맡긴 애셋이 다 끝난 뒤에도 로딩 화면을 0.3초 더 보이고 initialize 를 부른다.
//! - 글자 입력칸 포커스(is_text_input_focused)는 원본처럼 문서의 초점을 보고, 문서가 없는 데스크탑은 엔진 입력 필드의 포커스를 본다.
//! - 씬 하나만 내리기(unload_scene)는 원본 SceneManager.unloadScene 자리다. 원본은 씬 객체로, 러스트 판은 load_scene 이 돌려준 SceneId 로 가리키고,
//!   unload_all_scenes 처럼 이번 프레임이 끝난 뒤 내린다. 아직 create 전인(같은 프레임에 불러온) 씬이면 create 도 destroy 도 부르지 않고 뺀다.
//! - 페이지 이벤트(PageEvent): 원본은 모듈마다 document / window 에 리스너를 단다. 러스트 판은 플랫폼이 엔진에 알리고
//!   엔진이 add_page_event_listener 로 받은 리스너에 그 자리에서 넘긴다. (SoundSystem 이 가려짐과 포커스를 이것으로 받는다)
//!   웹: visibilitychange(Hidden, Visible), blur(Blurred), focus(Focused), pointerdown(PointerDown).
//!   데스크탑: 창 가림(Occluded, 맥과 X11 이 알린다), 최소화(윈도우가 이것으로 알린다), 앱 멈춤(모바일), 웨이랜드 프레임 콜백이 3 초 넘게 오지 않음은 Hidden, 포커스, 마우스 단추와 손가락 누름.
//! - 페이지 복원 훅: 원본 setupAllDocumentEvents 처럼 소리를 다시 깨우고 입력을 비운 뒤 씬마다 부른다.
//!   웹: visibilitychange 의 visible → on_page_visibility_restored, 문서 resume 과 pageshow(persisted) → on_page_process_restored.
//!   pageshow 는 resume 과 같은 복구를 한다. (원본이 정의되지 않은 dispatchPageProcessRestored 를 불러 예외가 나던 것은 원본에서도 같게 고쳤다)
//!   데스크탑: 창 가림이 풀리면 on_page_visibility_restored, 앱이 suspended 뒤 resumed 되면 on_page_process_restored.
//!   (윈도우, 맥, 리눅스의 winit 은 suspended 를 보내지 않아 데스크탑에서는 on_page_process_restored 가 불리지 않는다)
//! - async/await: 엔진이 스레드 하나짜리 실행기(core/task.rs)를 품는다. spawn_task, spawn_scene_task 는 원본 async 함수처럼 맡기는 자리에서
//!   첫 await 까지 돌리고, 남은 작업은 프레임마다 씬의 tick 앞에서 깨어난 것만 한 번씩 폴링한다. 원본에서 Promise 를 돌려주는 엔진 함수는
//!   await 할 수 있는 꼴을 같이 둔다. (load_scene_async, unload_scene_async, unload_all_scenes_async, read_clipboard_text_async, request_frame_capture_async,
//!   원본 window 의 alert_async, confirm_async, prompt_async. 질문 창이 떠 있는 동안은 원본 자바스크립트처럼 작업도 폴링하지 않는다)
//! - 씬 내리기는 원본 SceneManager.unloadScene 처럼 finalize, unload 를 부르고, unload 가 track_task 로 맡긴 작업이 끝나기를 기다린다.
//!   (그동안은 원본처럼 목록에 남아 tick 과 draw 를 받는다) 끝나면 목록에서 빼고, 그 씬의 작업이 남았으면 원본의 Promise 사슬처럼
//!   다 끝날 때까지 씬 값을 살려 둔다. (tick, draw 는 받지 않고 SceneContext::with 는 그 씬을 준다) 불러오는 도중에 내린 씬은 원본 loadScene 처럼
//!   불러오기가 끝나면 initialize 까지 받는다. 다 끝나면 destroy 를 부르고 버린다.
//!
//! 매 프레임 (update_engine):
//! 1. graphic.apply_settings()      # 뷰포트/프로젝션 갱신 + 상태 리셋 + 화면 비우기
//! 2. time_manager.update()         # time_delta 산출
//! 3. input_manager.tick()
//! 4. 깨어난 작업 폴링 (begin_poll_frame, 깨운 차례 줄이 빌 때까지 poll_woken_tasks 와 내리기 마무리)
//! 5. 씬마다 tick → pre_draw → draw → post_draw (→ draw_gizmos)
//! 6. 통계 출력(옵션) → 1회성 입력 플래그 클리어(글 입력 줄 마무리 포함) → 묶음 내보내기 → (검증용 화면 저장)
//! 화면 넘기기는 플랫폼이 update_engine 다음에 한다.
//==============================================================================
use crate::base::color::Color;
use crate::core::textshaping::FontFaceDescriptors;
use crate::base::pivot::Pivot;
use crate::base::platform::Platform;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::base::version::Version;
use crate::core::asset::AssetLoadHandle;
use crate::core::audiomanager::AudioManager;
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::image::Image;
use crate::core::inputmanager::InputManager;
use crate::core::modaldialog::{ModalDialogHandle, ModalDialogKind};
use crate::core::pointercursor::PointerCursor;
use crate::core::scene::{Scene, SceneId};
use crate::core::task::{self, EngineContext, Promise, PromiseResolver, SceneContext, TaskExecutor, TaskHandle};
use crate::core::timemanager::TimeManager;
use crate::core::viewmanager::{ViewManager, ViewScaleMode};
use crate::platform::{self, PlatformWindow};
use crate::resource::audioasset::AudioAsset;
use crate::resource::fontasset::FontLoadHandle;
use std::cell::RefCell;
use std::future::Future;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use web_time::Instant;

/// 엔진 판. (원본 vanilla.js 0.3.2 를 옮긴 첫 판)
const ENGINE_VERSION: Version = Version::create(0, 3, 2);

/// 설정의 기본 창 제목. (데스크탑 창 제목, 웹은 이 기본값이면 원본처럼 문서 제목을 바꾸지 않는다. 원본 기본값은 빈 글)
pub(crate) const DEFAULT_ENGINE_TITLE: &str = "바닐라엔진";

/// 씬이 맡긴 애셋이 다 끝난 뒤에도 로딩 화면을 더 보이는 시간. (초, 원본 SceneManager.loadScene 의 setTimeout 300)
const SCENE_LOAD_EXTRA_WAIT_SECONDS: f64 = 0.3;

//==============================================================================
/// 엔진 설정.
/// - EngineConfiguration::new() 의 기본값에서 필요한 칸만 바꿔 Engine::new 에 넘긴다.
///
/// # 예제
/// 기준 해상도, 뷰 스케일 모드, 애셋 폴더 정하기
/// ```rust
/// use std::path::PathBuf;
/// use vanilla::{Engine, EngineConfiguration, Vector2, ViewScaleMode};
///
/// let mut engine_configuration = EngineConfiguration::new();
/// engine_configuration.title = String::from("My Game");
/// engine_configuration.reference_resolution_size = Vector2::create(1920.0, 1080.0);
/// engine_configuration.view_scale_mode = ViewScaleMode::StretchShortExpandLong;
/// engine_configuration.asset_directory_paths = vec![PathBuf::from("examples/assets")];
/// engine_configuration.maximum_frame_per_second = 60.0;
/// assert!(engine_configuration.use_vertical_sync);
/// let engine = Engine::new(engine_configuration);
/// assert_eq!(engine.get_engine_configuration().title, "My Game");
/// assert_eq!(engine.resolve_asset_path("atlas/icons.png"), PathBuf::from("examples/assets/atlas/icons.png"));
/// ```
//==============================================================================
pub struct EngineConfiguration {
	/// 기준 해상도. (뷰 스케일 모드가 none 이 아니면 씬은 이 좌표계로 그린다)
	pub reference_resolution_size: Vector2,
	/// 뷰 스케일 모드.
	pub view_scale_mode: ViewScaleMode,
	/// 창 제목. (데스크탑 창 제목, 웹은 비어 있지 않고 기본값이 아니면 원본처럼 시작할 때 문서 제목(document.title)에 쓴다)
	pub title: String,
	/// 처음 창 크기. (논리 크기, 0 이면 기준 해상도)
	pub window_size: Vector2,
	/// 창 크기를 바꿀 수 있는지.
	pub is_resizable: bool,
	/// 화면 왼쪽 위에 FPS / 드로우 콜 출력.
	pub use_statistics: bool,
	/// 개발자 도구(F2 하이어라키/인스펙터)를 GameSceneBase 가 만드는지. (원본 useDevTools, 기본 true)
	pub use_dev_tools: bool,
	/// 최대 FPS. 0 이면 제한하지 않는다. (수직 동기를 켜면 화면 주사율을 따른다)
	pub maximum_frame_per_second: f32,
	/// 수직 동기.
	pub use_vertical_sync: bool,
	/// 기본 글꼴 사슬의 파일. (데스크탑, 비우면(기본) 원본 SYSTEM_FONT_STRING 의 이름을 차례로 운영체제 글꼴에서 찾아 잇는다)
	/// - 주면 읽히는 파일을 모두 차례로 잇는다. 앞 글꼴에 없는 글자는 뒤 글꼴로 그린다. (원본 글꼴 목록과 같다) TTC 는 굵기로 얼굴을 고르고, 가변 글꼴은 굵기를 축 값으로 만든다.
	pub default_font_paths: Vec<PathBuf>,
	/// 기본 글꼴 사슬의 굵은 파일. (default_font_paths 와 같은 자리의 글꼴의 굵기 700 얼굴, 데스크탑)
	pub default_bold_font_paths: Vec<PathBuf>,
	/// 대체 글꼴 후보 경로. (데스크탑, 비우면(기본) 원본 브라우저처럼 어느 글꼴에도 없는 글자를 운영체제가 고른 대체 글꼴로 그린다. 맥 CoreText, 윈도우 DirectWrite, 리눅스 fontconfig)
	/// - 주면 운영체제에 묻지 않고 등록된 어느 글꼴에도 없는 글자가 나올 때 앞에서부터 읽어 대체 폰트로 쓴다.
	///   색 이모지 글꼴(색 글리프 표가 있는 파일)은 이모지 표시 글자가 나올 때 먼저 읽고, 다른 글자 때문에는 마지막에 읽는다.
	pub fallback_font_paths: Vec<PathBuf>,
	/// 애셋 폴더 후보. (resolve_asset_path 가 앞에서부터 찾는다)
	pub asset_directory_paths: Vec<PathBuf>,
	/// 창 아이콘 PNG. (애셋 폴더 기준 상대 경로, 비우면 기본 아이콘)
	pub window_icon_path: String,
	/// 기본 폰트 파일 내용. (있으면 경로 후보보다 먼저 쓴다. 파일을 못 읽는 웹에서 미리 받아 넘긴다)
	pub default_font_bytes: Option<Vec<u8>>,
	/// 굵은 기본 폰트 파일 내용.
	pub default_bold_font_bytes: Option<Vec<u8>>,
	/// 웹에서 그릴 캔버스 id. (없으면 만들어 body 에 붙인다)
	pub canvas_id: String,
	/// 웹 기본 폰트 주소. (원본 defaultFontUrl. 웹에서 default_font_bytes 가 없으면 여기서 받아 기본 폰트로 쓴다. 빈 글이면 받지 않는다)
	/// - 기본값은 빈 글이다. (원본 기본값 DOSGothic 은 넣지 않는다) 아무 글꼴도 주지 않으면 원본처럼 브라우저 시스템 글꼴로 그린다.
	/// - 게임 글꼴을 쓰려면 이 주소나 default_font_bytes, FontAsset 으로 준다.
	/// - 데스크탑은 쓰지 않고 default_font_paths(비면 운영체제가 찾은 시스템 글꼴 사슬)를 본다.
	pub default_font_url: String,
	/// 웹에서 touchend 와 pointerup 도 user gesture 로 보고 오디오를 재개한다. (원본과 같이 기본은 끔)
	pub resume_audio_on_touch_end: bool,
	/// 웹에서 애셋 주소 앞에 붙일 것. (비우면 페이지 기준 상대 주소. 데스크탑은 asset_directory_paths 를 쓴다)
	pub asset_base_url: String,
	/// 창 크기가 바뀌면 캔버스를 창에 맞춘다. (원본처럼 기본은 끔. 켜면 visualViewport 크기에 맞추고 캔버스 비 상한을 건다. 끄면 웹은 페이지가 정한 캔버스 크기를 읽는다)
	/// 데스크탑 창은 캔버스가 창을 가득 채운 페이지와 같아, 끄면 창 크기 그대로이고 화면 키보드에 줄지 않으며 캔버스 비 상한도 걸지 않는다.
	pub auto_resize_on_window_resize: bool,
	/// 프레임이 끝난 뒤에도 그린 것을 캔버스에 남긴다. (원본 WebGL 컨텍스트 옵션 preserveDrawingBuffer, 켜면 프레임 첫머리에 색을 비우지 않아 지난 프레임 그림 위에 그린다.
	/// 웹은 컨텍스트 옵션으로 넘기고, 데스크탑은 다 그린 프레임을 떠 두었다가 다음 프레임 첫머리에 되그린다. platform/desktop/preserveddrawingbuffer.rs)
	pub preserve_drawing_buffer: bool,
	/// 화면 키보드: 글자 입력칸이 포커스된 동안 가로가 같고 세로만 줄었으면 크기를 그대로 둔다. (auto_resize_on_window_resize 일 때,
	/// 데스크탑은 크롬 윈도우처럼 터치 키보드가 visualViewport 를 줄인다. 맥과 리눅스는 크롬처럼 줄지 않는다)
	pub ignore_virtual_keyboard_resize: bool,
	/// 캔버스의 가로세로 비 상한. (창이 그보다 길쭉하면 캔버스를 줄여 창의 가운데에 둔다, 0 이면 제한 없음)
	pub max_canvas_aspect_ratio: f32,
	/// 글꼴이 늦게 받아지면 그 글꼴로 구운 글자 가운데 그 글꼴의 글자 범위에 든 것만 버려 다시 굽게 할지. (원본 rebakeTextOnFontLoad, 원본처럼 기본 끔)
	/// 끄면 받기 전에 다른 글꼴로 구운 글자는 캐시에서 밀려날 때까지 그대로 남는다. (글자 범위로 나눈 글꼴 조각을 그때그때 받는 프로젝트가 켠다)
	pub rebake_text_on_font_load: bool,
	/// 문서 언어. (웹 판 index.html 의 html lang 자리, Document::get_language. 비우면 데스크탑은 빈 글이고 웹은 페이지가 적어 둔 그대로)
	pub document_language: String,
	/// 문서의 meta. ((이름, content) 목록, 웹 판 index.html 의 head 의 meta 자리. Document::get_meta_content, BuildInfo 가 build-version 을 읽는다)
	/// - 데스크탑 배포 빌드는 웹 판이 index.html 에 적는 판을 여기에 넣는다. (build.rs 의 rustc-env 를 env! 로 읽는 것 같은 것) 웹에서도 비어 있지 않으면 문서에 쓴다.
	pub document_meta_list: Vec<(String, String)>,
}

impl EngineConfiguration {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 기본 설정. (기준 해상도 1280x720, StretchShort, 제목 바닐라엔진, 창 크기 0(기준 해상도), 크기 바꾸기 켬, 수직 동기 켬, 최대 FPS 0(제한 없음), 개발자 도구 켬, 캔버스 id mainCanvas, 나머지는 끔이거나 빈 값)
	//==============================================================================
	pub fn new() -> EngineConfiguration {
		EngineConfiguration {
			reference_resolution_size: Vector2::create(1280.0, 720.0),
			view_scale_mode: ViewScaleMode::StretchShort,
			title: String::from(DEFAULT_ENGINE_TITLE),
			window_size: Vector2::zero(),
			is_resizable: true,
			use_statistics: false,
			use_dev_tools: true,
			maximum_frame_per_second: 0.0,
			use_vertical_sync: true,
			default_font_paths: Vec::new(),
			default_bold_font_paths: Vec::new(),
			fallback_font_paths: Vec::new(),
			asset_directory_paths: Vec::new(),
			window_icon_path: String::new(),
			default_font_bytes: None,
			default_bold_font_bytes: None,
			canvas_id: String::from("mainCanvas"),
			default_font_url: String::new(),
			resume_audio_on_touch_end: false,
			asset_base_url: String::new(),
			auto_resize_on_window_resize: false,
			preserve_drawing_buffer: false,
			ignore_virtual_keyboard_resize: false,
			max_canvas_aspect_ratio: 0.0,
			rebake_text_on_font_load: false,
			document_language: String::new(),
			document_meta_list: Vec::new(),
		}
	}
}

impl Default for EngineConfiguration {
	//==============================================================================
	/// 기본값. (EngineConfiguration::new 와 같다)
	//==============================================================================
	fn default() -> EngineConfiguration {
		EngineConfiguration::new()
	}
}

//==============================================================================
/// 페이지 이벤트. (플랫폼이 엔진에 알리고, 엔진이 리스너에 그 자리에서 넘긴다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageEvent {
	/// 페이지가 가려짐. (다른 탭, 앱 전환, 창이 가려짐)
	Hidden,
	/// 페이지가 다시 보임.
	Visible,
	/// 창이 포커스를 잃음. (blur)
	Blurred,
	/// 창이 포커스를 얻음. (focus)
	Focused,
	/// 창을 누름. (pointerdown, 포커스를 알리지 않아도 누르면 창은 포커스를 가진 것이다)
	PointerDown,
}

/// 페이지 이벤트 리스너. (엔진은 약한 참조로 들어, 리스너를 버리면 더 부르지 않는다)
pub type PageEventListener = Rc<RefCell<dyn FnMut(PageEvent)>>;

/// 검증용 화면 저장을 받는 함수. (그 프레임을 다 그린 화면의 PNG 바이트, 원본은 PNG data URL)
pub type FrameCaptureHandler = Box<dyn FnOnce(&[u8])>;

//==============================================================================
/// 프레임 시작에 할 글꼴 일. (Graphic 이 있어야 한다)
//==============================================================================
enum PendingFontOperation {
	/// 이름, 얼굴 열쇠.
	Unregister(String, String),
	/// 이름, 얼굴 열쇠의 차례.
	Order(String, Vec<String>),
	ClearTextCache,
}

//==============================================================================
/// 엔진.
/// - 설정으로 만들고 run 에 첫 씬을 넘겨 돌린다. 씬의 훅은 &mut Engine 을 받아 매니저(시간, 뷰, 입력, 오디오, 커서)를 꺼내 쓴다.
///
/// # 예제
/// 첫 씬으로 엔진 돌리기 (examples/quickstart.rs)
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, Vector2};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
/// }
///
/// let mut engine_configuration = EngineConfiguration::new();
/// engine_configuration.auto_resize_on_window_resize = true;
/// engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
/// let engine = Engine::new(engine_configuration);
/// let main_scene = MainScene {
/// 	scene_base: SceneBase::new(),
/// };
/// if let Err(error) = engine.run(Box::new(main_scene)) {
/// 	eprintln!("{}", error);
/// 	std::process::exit(1);
/// }
/// ```
///
/// 창 없이 엔진을 만들어 매니저와 설정 다루기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration};
///
/// let mut engine = Engine::new(EngineConfiguration::new());
/// assert_eq!(engine.get_frame_number(), 1);
/// assert!(!engine.get_version_string().is_empty());
/// engine.get_time_manager_mut().set_time_scale(0.5);
/// assert_eq!(engine.get_time_manager().get_time_scale(), 0.5);
/// engine.set_maximum_frame_per_second(30.0);
/// assert_eq!(engine.get_maximum_frame_per_second(), 30.0);
/// assert!(engine.get_canvas().is_none());
/// ```
//==============================================================================
pub struct Engine {
	engine_configuration: EngineConfiguration,
	time_manager: TimeManager,
	view_manager: ViewManager,
	input_manager: InputManager,
	pointer_cursor: PointerCursor,
	audio_manager: AudioManager,
	pub(crate) platform_window: Option<PlatformWindow>,
	frame_number: u64,
	version: Version,
	is_exit_requested: bool,
	/// 프레임 번호마다 화면 저장을 받을 함수. (request_frame_capture)
	frame_capture_handler_map: std::collections::HashMap<u64, Vec<FrameCaptureHandler>>,
	/// 다음 프레임 첫머리에 캔버스와 뷰 영역을 다시 맞춰 달라고 했는지. (resize)
	pub(crate) is_resize_requested: bool,
	pending_scene_list: Vec<Box<dyn Scene>>,
	is_unload_all_scenes_requested: bool,
	pending_font_registration_list: Vec<(String, FontFaceDescriptors, FontLoadHandle)>,
	/// 글꼴 얼굴 지우기, 차례 정하기, 구운 글자 비우기 부탁. (Graphic 이 있는 프레임 시작에 한다)
	pending_font_operation_list: Vec<PendingFontOperation>,
	pending_default_font_handle: Option<AssetLoadHandle>,
	platform: Platform,
	next_scene_number: u64,
	pending_unload_scene_id_list: Vec<SceneId>,
	/// 불러온(create 를 부른) 씬의 핸들. (불러온 차례, get_all_loaded_scenes)
	loaded_scene_id_list: Vec<SceneId>,
	is_page_hidden: bool,
	page_event_listener_list: RefCell<Vec<Weak<RefCell<dyn FnMut(PageEvent)>>>>,
	/// 떠 있거나 기다리는 질문 창. (데스크탑, 맨 앞 것이 떠 있다. 웹은 브라우저가 띄운다)
	#[cfg(not(target_arch = "wasm32"))]
	modal_dialog_list: std::collections::VecDeque<crate::core::modaldialog::ModalDialog>,
	/// 개발자 도구에서 고를 노드. (크롬 문맥 메뉴의 검사 자리, 그 노드를 가진 씬의 DevTools 가 꺼낸다)
	dev_tools_inspect_request: std::cell::Cell<Option<crate::core::node::nodetree::NodeId>>,
	/// async 작업의 실행기. (EngineRunner 가 프레임마다 폴링한다)
	pub(crate) task_executor: TaskExecutor,
	/// 씬 내리기가 끝나면 풀 Promise. (unload_scene_async 는 그 씬, unload_all_scenes_async 는 그때의 모든 씬)
	pending_unload_resolver_list: Vec<(Option<SceneId>, PromiseResolver<()>)>,
}

impl Engine {
	//==============================================================================
	/// 생성.
	/// - 매니저(시간, 뷰, 입력, 커서, 오디오)를 만들고 문서 언어와 meta 를 설정대로 둔다. 창과 GL 은 run 에서 플랫폼이 만든다.
	///
	/// # 인자
	/// - `engine_configuration`: 엔진 설정. (엔진이 소유한다, get_engine_configuration 으로 읽는다)
	///
	/// # 돌려줌
	/// 새 엔진. (프레임 번호 1, 씬 없음)
	//==============================================================================
	pub fn new(engine_configuration: EngineConfiguration) -> Engine {
		let reference_resolution_size = engine_configuration.reference_resolution_size;
		let view_scale_mode = engine_configuration.view_scale_mode;
		crate::platform::document::apply_engine_configuration(&engine_configuration.document_language, &engine_configuration.document_meta_list);
		Engine {
			engine_configuration,
			time_manager: TimeManager::new(),
			view_manager: ViewManager::new(reference_resolution_size, view_scale_mode),
			input_manager: InputManager::new(),
			pointer_cursor: PointerCursor::new(),
			audio_manager: AudioManager::new(),
			platform_window: None,
			frame_number: 1,
			version: ENGINE_VERSION,
			is_exit_requested: false,
			frame_capture_handler_map: std::collections::HashMap::new(),
			is_resize_requested: false,
			pending_scene_list: Vec::new(),
			is_unload_all_scenes_requested: false,
			pending_font_registration_list: Vec::new(),
			pending_font_operation_list: Vec::new(),
			pending_default_font_handle: None,
			platform: Platform::new(),
			next_scene_number: 0,
			pending_unload_scene_id_list: Vec::new(),
			loaded_scene_id_list: Vec::new(),
			is_page_hidden: false,
			page_event_listener_list: RefCell::new(Vec::new()),
			#[cfg(not(target_arch = "wasm32"))]
			modal_dialog_list: std::collections::VecDeque::new(),
			dev_tools_inspect_request: std::cell::Cell::new(None),
			task_executor: TaskExecutor::new(),
			pending_unload_resolver_list: Vec::new(),
		}
	}

	//==============================================================================
	/// 개발자 도구로 노드 검사하기 부탁. (크롬 문맥 메뉴의 검사 자리. 그 노드를 가진 씬의 DevTools 가 다음 tick 에 열고 고른다)
	///
	/// # 인자
	/// - `node_id`: 검사할 노드. (앞서 부탁한 노드는 바꾼다)
	//==============================================================================
	pub fn request_dev_tools_inspect(&self, node_id: crate::core::node::nodetree::NodeId) {
		self.dev_tools_inspect_request.set(Some(node_id));
	}

	//==============================================================================
	/// 검사 부탁 꺼내기. (그 노드가 node_tree 에 있을 때만 꺼낸다)
	//==============================================================================
	pub(crate) fn take_dev_tools_inspect_request(&self, node_tree: &crate::core::node::nodetree::NodeTree) -> Option<crate::core::node::nodetree::NodeId> {
		let node_id = self.dev_tools_inspect_request.get()?;
		if !node_tree.contains(node_id) {
			return None;
		}
		self.dev_tools_inspect_request.set(None);
		Some(node_id)
	}

	//==============================================================================
	/// 알림 창. (원본 window.alert, 확인만 있다. 손잡이는 확인하면 끝난다)
	/// - 웹은 브라우저 창이 닫힐 때까지 막고 끝난 손잡이를 돌려준다. 데스크탑은 엔진이 그린 창을 띄우고, 닫힐 때까지 씬을 갱신하지 않는다. (core/modaldialog.rs)
	///
	/// # 인자
	/// - `message`: 창에 보일 글.
	///
	/// # 돌려줌
	/// 질문 창 손잡이. (확인하면 끝난다)
	//==============================================================================
	pub fn alert(&mut self, message: &str) -> ModalDialogHandle {
		self.show_modal_dialog(ModalDialogKind::Alert, message, "")
	}

	//==============================================================================
	/// 확인 창. (원본 window.confirm, 손잡이의 is_confirmed 가 원본이 돌려주는 값)
	///
	/// # 인자
	/// - `message`: 창에 보일 글.
	///
	/// # 돌려줌
	/// 질문 창 손잡이. (확인이나 취소로 답하면 끝난다)
	//==============================================================================
	pub fn confirm(&mut self, message: &str) -> ModalDialogHandle {
		self.show_modal_dialog(ModalDialogKind::Confirm, message, "")
	}

	//==============================================================================
	/// 입력 창. (원본 window.prompt(message, default), 손잡이의 get_prompt_text 가 원본이 돌려주는 값, 취소는 None)
	///
	/// # 인자
	/// - `message`: 창에 보일 글.
	/// - `default_value`: 입력 칸에 처음 넣어 둘 글. (데스크탑은 모두 고른 채로 초점을 받는다)
	///
	/// # 돌려줌
	/// 질문 창 손잡이. (확인이나 취소로 답하면 끝난다)
	//==============================================================================
	pub fn prompt(&mut self, message: &str, default_value: &str) -> ModalDialogHandle {
		self.show_modal_dialog(ModalDialogKind::Prompt, message, default_value)
	}

	//==============================================================================
	/// 알림 창을 띄우고 닫힐 때 풀리는 Promise. (원본 window.alert 를 await 꼴로)
	///
	/// # 인자
	/// - `message`: 창에 보일 글.
	///
	/// # 돌려줌
	/// 창이 닫히면 풀리는 Promise.
	//==============================================================================
	pub fn alert_async(&mut self, message: &str) -> Promise<()> {
		let (alert_promise, alert_resolver) = Promise::create();
		self.alert(message).set_callback(move |_result| {
			alert_resolver.resolve(());
		});
		alert_promise
	}

	//==============================================================================
	/// 확인 창을 띄우고 답하면 풀리는 Promise. (원본 window.confirm 이 돌려주는 값)
	///
	/// # 인자
	/// - `message`: 창에 보일 글.
	///
	/// # 돌려줌
	/// 답하면 풀리는 Promise. (확인이면 true, 취소면 false)
	///
	/// # 예제
	/// 엔진 작업에서 물어보고 답에 따라 끝내기
	/// ```rust,no_run
	/// use vanilla::Engine;
	///
	/// fn ask_exit(engine: &mut Engine) {
	/// 	engine.spawn_task(|mut engine_context| async move {
	/// 		let is_confirmed = engine_context.with(|engine| engine.confirm_async("Exit?")).await;
	/// 		if is_confirmed {
	/// 			engine_context.with(|engine| engine.exit_application());
	/// 		}
	/// 	});
	/// }
	/// ```
	//==============================================================================
	pub fn confirm_async(&mut self, message: &str) -> Promise<bool> {
		let (confirm_promise, confirm_resolver) = Promise::create();
		self.confirm(message).set_callback(move |result| {
			confirm_resolver.resolve(matches!(result, crate::core::modaldialog::ModalDialogResult::Confirmed(_)));
		});
		confirm_promise
	}

	//==============================================================================
	/// 입력 창을 띄우고 답하면 풀리는 Promise. (원본 window.prompt 가 돌려주는 값, 취소는 원본 null 자리의 None)
	///
	/// # 인자
	/// - `message`: 창에 보일 글.
	/// - `default_value`: 입력 칸에 처음 넣어 둘 글.
	///
	/// # 돌려줌
	/// 답하면 풀리는 Promise. (확인이면 입력한 글, 취소면 None)
	//==============================================================================
	pub fn prompt_async(&mut self, message: &str, default_value: &str) -> Promise<Option<String>> {
		let (prompt_promise, prompt_resolver) = Promise::create();
		self.prompt(message, default_value).set_callback(move |result| {
			let prompt_text = match result {
				crate::core::modaldialog::ModalDialogResult::Confirmed(text) => {
					Some(text.clone())
				}
				crate::core::modaldialog::ModalDialogResult::Cancelled => {
					None
				}
			};
			prompt_resolver.resolve(prompt_text);
		});
		prompt_promise
	}

	//==============================================================================
	/// 질문 창 띄우기. (웹은 브라우저 함수, 데스크탑은 목록 끝에 더한다)
	//==============================================================================
	fn show_modal_dialog(&mut self, kind: ModalDialogKind, message: &str, default_value: &str) -> ModalDialogHandle {
		#[cfg(target_arch = "wasm32")]
		{
			crate::core::modaldialog::show_browser_dialog(kind, message, default_value)
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			let handle = ModalDialogHandle::create_pending();
			self.modal_dialog_list.push_back(crate::core::modaldialog::ModalDialog::new(kind, message, default_value, handle.clone()));
			handle
		}
	}

	//==============================================================================
	/// 질문 창이 떠 있는지. (데스크탑만 참이 될 수 있다. 떠 있는 동안 엔진은 씬을 갱신하지 않는다)
	///
	/// # 돌려줌
	/// 떠 있거나 기다리는 질문 창이 있으면 true. (웹은 늘 false)
	//==============================================================================
	pub fn is_modal_dialog_open(&self) -> bool {
		#[cfg(target_arch = "wasm32")]
		{
			false
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			!self.modal_dialog_list.is_empty()
		}
	}

	//==============================================================================
	/// 맨 앞 질문 창 꺼내기. (갱신하는 동안 엔진을 함께 빌리려고 꺼냈다가 put_back_front_modal_dialog 로 돌려놓는다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn take_front_modal_dialog(&mut self) -> Option<crate::core::modaldialog::ModalDialog> {
		self.modal_dialog_list.pop_front()
	}

	//==============================================================================
	/// 맨 앞 질문 창 돌려놓기.
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn put_back_front_modal_dialog(&mut self, modal_dialog: crate::core::modaldialog::ModalDialog) {
		self.modal_dialog_list.push_front(modal_dialog);
	}

	//==============================================================================
	/// 맨 앞 질문 창.
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn get_front_modal_dialog_mut(&mut self) -> Option<&mut crate::core::modaldialog::ModalDialog> {
		self.modal_dialog_list.front_mut()
	}

	//==============================================================================
	/// 시작.
	/// - 데스크탑: 창을 닫거나 exit_application 을 부를 때까지 돌아오지 않는다.
	/// - 웹: 캔버스와 WebGL2 를 잡고 request_animation_frame 을 걸어 둔 뒤 바로 돌아온다.
	///
	/// # 인자
	/// - `scene`: 첫 씬. (load_scene 으로 불러온다)
	///
	/// # 돌려줌
	/// 정상으로 끝나면 Ok. (데스크탑은 창이 닫혀 이벤트 루프가 끝날 때, 웹은 프레임을 걸어 둔 뒤 바로) 이벤트 루프나 창, GL 컨텍스트를 만들지 못하는 등 플랫폼이 실패하면 Err(까닭).
	//==============================================================================
	pub fn run(self, scene: Box<dyn Scene>) -> Result<(), String> {
		platform::run(EngineRunner::new(self, scene))
	}

	//==============================================================================
	/// 씬 불러오기. (이번 프레임이 끝난 뒤 목록에 더한다. unload_scene 에 넘길 핸들을 돌려준다)
	/// - 목록에 더할 때 create 를 부르고 루트를 뷰 크기로 맞춘다. 맡긴 애셋과 작업이 다 끝나고 0.3 초 뒤 initialize 를 부른다.
	///
	/// # 인자
	/// - `scene`: 불러올 씬. (엔진이 소유한다)
	///
	/// # 돌려줌
	/// 그 씬의 핸들. (1 부터 매긴다)
	//==============================================================================
	pub fn load_scene(&mut self, mut scene: Box<dyn Scene>) -> SceneId {
		self.next_scene_number += 1;
		let scene_id = SceneId(self.next_scene_number);
		scene.get_scene_base_mut().set_scene_id(scene_id);
		self.pending_scene_list.push(scene);
		scene_id
	}

	//==============================================================================
	/// 씬 하나 내리기. (이번 프레임이 끝난 뒤 finalize, unload 를 부르고, unload 가 맡긴 작업이 끝나면 목록에서 뺀다. 없는 핸들이면 아무것도 하지 않는다)
	/// - 핸들(SceneId)이나 원본처럼 씬(씬 공통 상태 &SceneBase)을 받는다.
	/// - 그 씬의 작업이 남았으면 다 끝날 때까지 씬 값을 살려 두고, 다 끝나면 destroy 를 부르고 버린다.
	///
	/// # 인자
	/// - `scene`: 내릴 씬의 핸들(SceneId)이나 씬 공통 상태(&SceneBase).
	///
	/// # 예제
	/// 씬 스스로 내리기
	/// ```rust,no_run
	/// use vanilla::{Engine, Scene, SceneBase, Vector2};
	///
	/// struct PopupScene {
	/// 	scene_base: SceneBase,
	/// }
	///
	/// impl Scene for PopupScene {
	/// 	fn get_scene_base(&self) -> &SceneBase {
	/// 		&self.scene_base
	/// 	}
	///
	/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
	/// 		&mut self.scene_base
	/// 	}
	///
	/// 	fn touch_release(&mut self, engine: &mut Engine, _view_input_position: Vector2) {
	/// 		engine.unload_scene(&self.scene_base);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn unload_scene(&mut self, scene: impl Into<SceneId>) {
		self.pending_unload_scene_id_list.push(scene.into());
	}

	//==============================================================================
	/// 모든 씬 내리기. (이번 프레임이 끝난 뒤, load_scene 과 같이 부르면 내린 다음 불러온다)
	//==============================================================================
	pub fn unload_all_scenes(&mut self) {
		self.is_unload_all_scenes_requested = true;
	}

	//==============================================================================
	/// 씬 불러오기. (원본 await SceneManager.loadScene(scene), initialize 까지 끝나면 그 씬의 핸들로 풀리는 Promise 를 돌려준다)
	/// - 불러오기는 load_scene 과 같다. 같은 프레임에 create 전에 그 씬을 내리면 풀리지 않는다. (create 뒤에 내리면 initialize 까지 마치고 풀린다)
	///
	/// # 인자
	/// - `scene`: 불러올 씬. (엔진이 소유한다)
	///
	/// # 돌려줌
	/// initialize 가 끝나면 그 씬의 핸들로 풀리는 Promise.
	///
	/// # 예제
	/// 다음 씬을 불러 다 되면 지금 씬 내리기
	/// ```rust,no_run
	/// use vanilla::{Engine, Scene, SceneId};
	///
	/// fn change_scene(engine: &mut Engine, current_scene_id: SceneId, next_scene: Box<dyn Scene>) {
	/// 	engine.spawn_task(move |mut engine_context| async move {
	/// 		let next_scene_id = engine_context.with(|engine| engine.load_scene_async(next_scene)).await;
	/// 		engine_context.with(|engine| engine.unload_scene_async(current_scene_id)).await;
	/// 		println!("{:?}", next_scene_id);
	/// 	});
	/// }
	/// ```
	//==============================================================================
	pub fn load_scene_async(&mut self, mut scene: Box<dyn Scene>) -> Promise<SceneId> {
		let (load_promise, load_resolver) = Promise::create();
		scene.get_scene_base_mut().add_load_resolver(load_resolver);
		self.load_scene(scene);
		load_promise
	}

	//==============================================================================
	/// 씬 하나 내리기. (원본 await SceneManager.unloadScene(scene), 내리기가 끝나면 풀리는 Promise 를 돌려준다. 없는 핸들이어도 풀린다)
	/// - 이번 프레임 끝에 finalize, unload 를 부르고, unload 가 track_task 로 맡긴 작업까지 끝나면 목록에서 뺀 뒤 푼다.
	///
	/// # 인자
	/// - `scene`: 내릴 씬의 핸들(SceneId)이나 씬 공통 상태(&SceneBase).
	///
	/// # 돌려줌
	/// 내리기가 끝나면 풀리는 Promise.
	//==============================================================================
	pub fn unload_scene_async(&mut self, scene: impl Into<SceneId>) -> Promise<()> {
		let scene_id = scene.into();
		let (unload_promise, unload_resolver) = Promise::create();
		self.unload_scene(scene_id);
		self.pending_unload_resolver_list.push((Some(scene_id), unload_resolver));
		unload_promise
	}

	//==============================================================================
	/// 모든 씬 내리기. (원본 await SceneManager.unloadAllScenes(), 그때의 모든 씬의 내리기가 끝나면 풀리는 Promise 를 돌려준다)
	///
	/// # 돌려줌
	/// 그때 불러와 있던 모든 씬의 내리기가 끝나면 풀리는 Promise.
	//==============================================================================
	pub fn unload_all_scenes_async(&mut self) -> Promise<()> {
		let (unload_promise, unload_resolver) = Promise::create();
		self.unload_all_scenes();
		self.pending_unload_resolver_list.push((None, unload_resolver));
		unload_promise
	}

	//==============================================================================
	/// 엔진 작업 맡기기. (씬과 따로 사는 async 작업, 작업 함수가 받은 EngineContext 의 with 안에서 엔진을 빌린다)
	/// - 원본 async 함수를 부를 때처럼 이 자리에서 첫 await 까지 돌고, 남은 것은 깨어날 때마다 프레임의 폴링 차례에 돈다.
	/// - 돌려준 손잡이로 끝났는지 묻거나 await 한다.
	///
	/// # 인자
	/// - `task_function`: EngineContext 를 받아 퓨처를 돌려주는 함수. (보통 `|mut engine_context| async move { ... }`, 퓨처는 Send 가 아니어도 된다)
	///
	/// # 돌려줌
	/// 작업 손잡이. (첫 구간에서 끝났으면 이미 끝난 손잡이)
	///
	/// # 예제
	/// 첫 await 까지는 맡기는 자리에서 돈다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration};
	///
	/// let mut engine = Engine::new(EngineConfiguration::new());
	/// let task_handle = engine.spawn_task(|mut engine_context| async move {
	/// 	engine_context.with(|engine| {
	/// 		engine.set_maximum_frame_per_second(30.0);
	/// 	});
	/// });
	/// assert!(task_handle.is_finished());
	/// assert_eq!(engine.get_maximum_frame_per_second(), 30.0);
	/// ```
	//==============================================================================
	pub fn spawn_task<F, TaskFuture>(&mut self, task_function: F) -> TaskHandle
	where
		F: FnOnce(EngineContext) -> TaskFuture,
		TaskFuture: Future<Output = ()> + 'static,
	{
		task::spawn_engine_task(self, task_function)
	}

	//==============================================================================
	/// 씬 작업 맡기기. (그 씬의 async 작업, 작업 함수가 받은 SceneContext 의 with 안에서 그 씬(S 타입)과 엔진을 빌린다)
	/// - 원본 async 함수 안에서 this 를 쓰는 자리다. 원본처럼 이 자리에서 첫 await 까지 돌고, 남은 것은 깨어날 때마다 프레임의 폴링 차례에 돈다.
	///   (이 자리의 첫 구간에서는 그래픽이 없어 with_graphic 이 None 이다)
	/// - 씬을 내려도 원본처럼 이어 돈다. (씬 값은 작업이 다 끝날 때까지 산다) load_scene 전의 씬이면 첫 구간만 돌고 버린다.
	/// - 불러오기 작업이면 create 에서 씬의 track_task 로 맡겨 끝날 때까지 initialize 를 미룬다. (unload 에서 맡기면 내리기 끝을 미룬다)
	///
	/// # 인자
	/// - `scene`: 작업을 맡기는 씬. (보통 훅 안의 self)
	/// - `task_function`: `SceneContext<S>` 를 받아 퓨처를 돌려주는 함수. (보통 `|mut scene_context| async move { ... }`)
	///
	/// # 돌려줌
	/// 작업 손잡이. (track_task 에 넘기거나 await 한다)
	///
	/// # 예제
	/// 씬 작업에서 애셋을 기다려 씬에 넣기
	/// ```rust,no_run
	/// use vanilla::{Engine, Scene, SceneBase, SceneContext};
	///
	/// struct StageScene {
	/// 	scene_base: SceneBase,
	/// 	stage_bytes: Vec<u8>,
	/// }
	///
	/// impl Scene for StageScene {
	/// 	fn get_scene_base(&self) -> &SceneBase {
	/// 		&self.scene_base
	/// 	}
	///
	/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
	/// 		&mut self.scene_base
	/// 	}
	///
	/// 	fn create(&mut self, engine: &mut Engine) {
	/// 		let task_handle = engine.spawn_scene_task(self, |mut scene_context: SceneContext<StageScene>| async move {
	/// 			let load_handle = scene_context.with(|_scene, engine| engine.request_asset_bytes("examples/assets/scenedocument.scene.json"));
	/// 			let stage_bytes = load_handle.await.map(|bytes| bytes.to_vec()).unwrap_or_default();
	/// 			scene_context.with(|scene, _engine| {
	/// 				scene.stage_bytes = stage_bytes;
	/// 			});
	/// 		});
	/// 		self.scene_base.track_task(&task_handle);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn spawn_scene_task<S, F, TaskFuture>(&mut self, scene: &mut S, task_function: F) -> TaskHandle
	where
		S: Scene + 'static,
		F: FnOnce(SceneContext<S>) -> TaskFuture,
		TaskFuture: Future<Output = ()> + 'static,
	{
		task::spawn_scene_task(self, scene, task_function)
	}

	//==============================================================================
	/// 엔진 작업을 다음 폴링 차례로 맡기기. (&Engine 만 있는 자리, 컴포넌트 문맥처럼. 다음 프레임의 폴링 차례에 처음 돈다)
	///
	/// # 인자
	/// - `task_function`: EngineContext 를 받아 퓨처를 돌려주는 함수.
	///
	/// # 돌려줌
	/// 작업 손잡이. (다음 폴링 차례 전에는 끝나지 않은 손잡이)
	///
	/// # 예제
	/// 맡긴 자리에서는 돌지 않는다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let task_handle = engine.spawn_deferred_task(|_engine_context| async move {});
	/// assert!(!task_handle.is_finished());
	/// ```
	//==============================================================================
	pub fn spawn_deferred_task<F, TaskFuture>(&self, task_function: F) -> TaskHandle
	where
		F: FnOnce(EngineContext) -> TaskFuture,
		TaskFuture: Future<Output = ()> + 'static,
	{
		task::spawn_deferred_engine_task(&self.task_executor, task_function)
	}

	//==============================================================================
	/// 씬 작업을 다음 폴링 차례로 맡기기. (&Engine 과 &씬만 있는 자리, 다음 프레임의 폴링 차례에 처음 돈다)
	/// - 그 씬이 다음 폴링 차례에 불러온 씬 목록에도 살려 둔 씬 목록에도 없으면 작업을 버린다.
	///
	/// # 인자
	/// - `scene`: 작업을 맡기는 씬.
	/// - `task_function`: `SceneContext<S>` 를 받아 퓨처를 돌려주는 함수.
	///
	/// # 돌려줌
	/// 작업 손잡이. (다음 폴링 차례 전에는 끝나지 않은 손잡이)
	//==============================================================================
	pub fn spawn_deferred_scene_task<S, F, TaskFuture>(&self, scene: &S, task_function: F) -> TaskHandle
	where
		S: Scene + 'static,
		F: FnOnce(SceneContext<S>) -> TaskFuture,
		TaskFuture: Future<Output = ()> + 'static,
	{
		let scene_id = SceneId::from(scene.get_scene_base());
		task::spawn_deferred_scene_task(&self.task_executor, scene_id, task_function)
	}

	//==============================================================================
	/// 불러온 모든 씬의 핸들. (원본 SceneManager.getAllLoadedScenes, 불러온 차례, 로딩 중인 씬도 든다)
	/// - 원본은 씬 객체를 돌려준다. 러스트 판은 씬마다 &mut Engine 을 받아 도는 동안 다른 씬을 빌릴 수 없어서 핸들을 돌려준다.
	///   load_scene 한 프레임이 끝나 create 가 불린 씬부터 들고, 내린 씬은 빠진다.
	///
	/// # 돌려줌
	/// 불러온 씬의 핸들 목록. (불러온 차례)
	//==============================================================================
	pub fn get_all_loaded_scenes(&self) -> &[SceneId] {
		&self.loaded_scene_id_list
	}

	//==============================================================================
	/// 애셋 경로 찾기. (설정의 애셋 폴더 후보에서 처음 있는 것, 없으면 첫 후보 기준)
	///
	/// # 인자
	/// - `relative_path`: 애셋 폴더 기준 상대 경로.
	///
	/// # 돌려줌
	/// 파일이 있는 첫 후보 폴더의 경로. (어디에도 없으면 첫 후보 기준 경로, 후보가 없으면 상대 경로 그대로)
	//==============================================================================
	pub fn resolve_asset_path(&self, relative_path: &str) -> PathBuf {
		for asset_directory_path in &self.engine_configuration.asset_directory_paths {
			let candidate_path = asset_directory_path.join(relative_path);
			if candidate_path.exists() {
				return candidate_path;
			}
		}
		match self.engine_configuration.asset_directory_paths.first() {
			Some(first_asset_directory_path) => {
				first_asset_directory_path.join(relative_path)
			}
			None => {
				PathBuf::from(relative_path)
			}
		}
	}

	//==============================================================================
	/// 이미지 애셋 읽기. (원본 new ImageAsset() + load(path))
	/// - "data:" 주소도 받는다. (주소의 형으로 푼다)
	///
	/// # 인자
	/// - `relative_path`: 애셋 폴더 기준 상대 경로나 "data:" 주소.
	///
	/// # 돌려줌
	/// 읽은 이미지. 파일을 못 읽거나 풀지 못하면 Err(까닭).
	///
	/// # 예제
	/// 저장소의 아틀라스 그림 읽기
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let image = engine.load_image("examples/assets/atlas/icons.png").unwrap();
	/// assert!(image.get_width() > 0);
	/// assert!(engine.load_image("examples/assets/missing.png").is_err());
	/// ```
	//==============================================================================
	pub fn load_image(&self, relative_path: &str) -> Result<Rc<Image>, String> {
		if crate::base::dataurl::is_data_url(relative_path) {
			let data_url = crate::base::dataurl::parse_data_url(relative_path).ok_or_else(|| String::from("이미지 읽기 실패 (data: 주소를 풀 수 없습니다)"))?;
			return Image::load_from_memory_with_mime_type(&data_url.bytes, &data_url.mime_type);
		}
		let image_path = self.resolve_asset_path(relative_path);
		Image::load(&image_path)
	}

	//==============================================================================
	/// 애셋 바이트 부탁. (애셋들의 load 가 부른다)
	/// - 데스크탑: 애셋 폴더 후보에서 파일을 찾아 그 자리에서 읽고 끝낸다.
	/// - 웹: asset_base_url + 상대 경로를 fetch 하고, 받으면 끝낸다. (그 사이 프레임은 계속 돈다)
	/// - "data:" 주소는 원본 fetch 처럼 주소 그대로 푼다. (데스크탑은 그 자리에서 base/dataurl.rs 로 풀고 형을 응답 형으로 둔다, 웹은 기준 주소를 붙이지 않고 fetch)
	///
	/// # 인자
	/// - `relative_path`: 애셋 폴더 기준 상대 경로나 "data:" 주소.
	///
	/// # 돌려줌
	/// 읽기 손잡이. (읽히면 바이트, 실패하면 까닭을 든다. await 할 수 있다)
	///
	/// # 예제
	/// 파일과 data: 주소 읽기 (데스크탑은 그 자리에서 끝난다)
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let load_handle = engine.request_asset_bytes("examples/assets/atlas/icons.json");
	/// assert!(load_handle.is_loaded());
	/// let data_load_handle = engine.request_asset_bytes("data:text/plain,hello");
	/// assert_eq!(data_load_handle.get_bytes().unwrap().as_slice(), b"hello");
	/// assert!(engine.request_asset_bytes("examples/assets/missing.json").is_failed());
	/// ```
	//==============================================================================
	pub fn request_asset_bytes(&self, relative_path: &str) -> AssetLoadHandle {
		self.request_asset_bytes_with_cache(relative_path, false)
	}

	//==============================================================================
	/// 애셋 바이트 부탁. (is_no_cache 면 웹은 브라우저 캐시를 쓰지 않고 다시 확인한다, 원본 TextAsset.setNoCache)
	///
	/// # 인자
	/// - `relative_path`: 애셋 폴더 기준 상대 경로나 "data:" 주소.
	/// - `is_no_cache`: true 면 웹은 브라우저 캐시를 쓰지 않는다. (데스크탑은 늘 파일을 읽어 상관없다)
	///
	/// # 돌려줌
	/// 읽기 손잡이.
	//==============================================================================
	pub fn request_asset_bytes_with_cache(&self, relative_path: &str, is_no_cache: bool) -> AssetLoadHandle {
		let load_handle = AssetLoadHandle::create_pending(relative_path);
		self.request_asset_bytes_into_with_cache(relative_path, &load_handle, is_no_cache);
		load_handle
	}

	//==============================================================================
	/// 애셋 바이트 부탁. (이미 만든 읽기 손잡이를 끝낸다. AssetLoader 가 미리 나눠 준 손잡이를 차례가 오면 채울 때 쓴다)
	///
	/// # 인자
	/// - `relative_path`: 애셋 폴더 기준 상대 경로나 "data:" 주소.
	/// - `load_handle`: 끝낼 읽기 손잡이. (AssetLoadHandle::create_pending 으로 만든 것)
	///
	/// # 예제
	/// 미리 나눠 준 손잡이를 나중에 채우기
	/// ```rust
	/// use vanilla::{AssetLoadHandle, Engine, EngineConfiguration};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let load_handle = AssetLoadHandle::create_pending("examples/assets/atlas/icons.json");
	/// assert!(!load_handle.is_finished());
	/// engine.request_asset_bytes_into("examples/assets/atlas/icons.json", &load_handle);
	/// assert!(load_handle.is_loaded());
	/// ```
	//==============================================================================
	pub fn request_asset_bytes_into(&self, relative_path: &str, load_handle: &AssetLoadHandle) {
		self.request_asset_bytes_into_with_cache(relative_path, load_handle, false);
	}

	//==============================================================================
	/// 애셋 바이트 부탁의 몸통. (data: 주소, 데스크탑 애셋 폴더, 웹 기준 주소)
	//==============================================================================
	fn request_asset_bytes_into_with_cache(&self, relative_path: &str, load_handle: &AssetLoadHandle, is_no_cache: bool) {
		#[cfg(not(target_arch = "wasm32"))]
		{
			if crate::base::dataurl::is_data_url(relative_path) {
				match crate::base::dataurl::parse_data_url(relative_path) {
					Some(data_url) => {
						load_handle.set_content_type(Some(data_url.mime_type));
						load_handle.finish(Ok(data_url.bytes));
					}
					None => {
						load_handle.finish(Err(String::from("애셋 읽기 실패 (data: 주소를 풀 수 없습니다)")));
					}
				}
				return;
			}
			let asset_location = self.resolve_asset_path(relative_path).to_string_lossy().into_owned();
			platform::request_asset_bytes(asset_location, load_handle.clone(), is_no_cache);
		}
		#[cfg(target_arch = "wasm32")]
		{
			platform::request_asset_bytes(self.create_asset_url(relative_path), load_handle.clone(), is_no_cache);
		}
	}

	//==============================================================================
	/// 웹 애셋 주소. (asset_base_url + 상대 경로, data: 주소는 그대로)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	fn create_asset_url(&self, relative_path: &str) -> String {
		if crate::base::dataurl::is_data_url(relative_path) {
			return relative_path.to_string();
		}
		format!("{}{}", self.engine_configuration.asset_base_url, relative_path)
	}

	//==============================================================================
	/// 글꼴 바이트가 오면 이름으로 등록해 달라고 맡기기. (FontAsset 이 부른다. 등록은 프레임 시작에 한다, 올리면 끝나는 손잡이를 돌려준다)
	///
	/// # 인자
	/// - `family`: 글꼴 이름. (글자를 그릴 때 쓰는 이름)
	/// - `is_bold`: true 면 굵기 700 얼굴, false 면 400 얼굴로 올린다.
	/// - `load_handle`: 글꼴 파일 바이트의 읽기 손잡이.
	///
	/// # 돌려줌
	/// 글꼴 목록에 올리면(또는 받기나 올리기에 실패하면) 끝나는 손잡이.
	//==============================================================================
	pub fn register_font_when_loaded(&mut self, family: &str, is_bold: bool, load_handle: AssetLoadHandle) -> FontLoadHandle {
		self.register_font_face_when_loaded(family, FontFaceDescriptors::create_with_bold(is_bold), load_handle)
	}

	//==============================================================================
	/// 글꼴 얼굴 바이트가 오면 서술자(굵기, 기울기, 글자 범위)대로 이름 아래에 올려 달라고 맡기기. (원본 new FontFace(family, url, descriptors) + document.fonts.add)
	/// - 서술자의 source_key 가 비었으면 읽기 손잡이의 경로를 쓴다.
	/// - 돌려준 손잡이는 글꼴 목록에 올리면(또는 받기나 올리기에 실패하면) 끝난다. (원본 await fontFace.load() 와 document.fonts.add 자리)
	///
	/// # 인자
	/// - `family`: 글꼴 이름.
	/// - `font_face_descriptors`: 굵기, 기울기, 글자 범위, 얼굴 열쇠. (열쇠가 비었으면 읽기 손잡이의 경로)
	/// - `load_handle`: 글꼴 파일 바이트의 읽기 손잡이.
	///
	/// # 돌려줌
	/// 글꼴 목록에 올리면(또는 받기나 올리기에 실패하면) 끝나는 손잡이.
	//==============================================================================
	pub fn register_font_face_when_loaded(&mut self, family: &str, mut font_face_descriptors: FontFaceDescriptors, load_handle: AssetLoadHandle) -> FontLoadHandle {
		if font_face_descriptors.source_key.is_empty() {
			font_face_descriptors.source_key = load_handle.get_asset_path().to_string();
		}
		let font_load_handle = FontLoadHandle::new(load_handle);
		self.pending_font_registration_list.push((family.to_string(), font_face_descriptors, font_load_handle.clone()));
		font_load_handle
	}

	//==============================================================================
	/// 글꼴 얼굴 지우기 맡기기. (원본 document.fonts.delete, 열쇠는 등록할 때의 source_key 또는 경로)
	/// - 아직 바이트를 기다리는 같은 얼굴의 등록은 바로 뺀다. (이름은 대소문자를 가리지 않는다) 지우기는 다음 프레임 시작에 한다.
	///
	/// # 인자
	/// - `family`: 글꼴 이름.
	/// - `source_key`: 지울 얼굴의 열쇠.
	//==============================================================================
	pub fn unregister_font_face(&mut self, family: &str, source_key: &str) {
		self.pending_font_registration_list.retain(|(pending_family, font_face_descriptors, _)| !(pending_family.eq_ignore_ascii_case(family) && font_face_descriptors.source_key == source_key));
		self.pending_font_operation_list.push(PendingFontOperation::Unregister(family.to_string(), source_key.to_string()));
	}

	//==============================================================================
	/// 한 이름의 얼굴 차례 정하기 맡기기. (얼굴 열쇠 목록, 뒤가 먼저 그린다, TextStringTextureCache.set_font_face_order)
	///
	/// # 인자
	/// - `family`: 글꼴 이름.
	/// - `source_key_list`: 얼굴 열쇠 목록. (뒤의 얼굴이 먼저 그린다, 다음 프레임 시작에 건다)
	//==============================================================================
	pub fn set_font_face_order(&mut self, family: &str, source_key_list: Vec<String>) {
		self.pending_font_operation_list.push(PendingFontOperation::Order(family.to_string(), source_key_list));
	}

	//==============================================================================
	/// 구운 글자 모두 버리기 맡기기. (원본 textStringTextureCache.clear, 다음 프레임 시작에)
	//==============================================================================
	pub fn clear_text_cache(&mut self) {
		self.pending_font_operation_list.push(PendingFontOperation::ClearTextCache);
	}

	//==============================================================================
	/// 아직 바이트가 오지 않은 글꼴 얼굴 수. (UI 글꼴이 기다릴 때 본다)
	///
	/// # 돌려줌
	/// 등록을 기다리는 글꼴 얼굴 수. (바이트를 기다리는 것과 다음 프레임 시작에 올릴 것)
	//==============================================================================
	pub fn get_pending_font_face_count(&self) -> usize {
		self.pending_font_registration_list.len()
	}

	//==============================================================================
	/// 오디오 애셋 읽기. (원본 new AudioAsset() + load(path), 애셋 폴더 기준 경로나 "data:" 주소)
	/// - 데스크탑: 그 자리에서 읽고 디코딩한다. 실패하면 Err. data: 주소는 형을 보지 않고 바이트를 내용으로 가려 푼다. (원본 decodeAudioData 와 같다)
	/// - 웹: fetch 와 디코딩을 걸어 두고 바로 애셋을 돌려준다. 끝나면 is_loaded() 가 참이 된다. (실패는 get_load_error())
	///
	/// # 인자
	/// - `relative_path`: 애셋 폴더 기준 상대 경로나 "data:" 주소.
	///
	/// # 돌려줌
	/// 오디오 애셋. 데스크탑은 읽기나 디코딩에 실패하면 Err(까닭). (웹은 그 자리에서 실패한 것만 Err)
	///
	/// # 예제
	/// 소리 파일을 읽어 길이 보기 (데스크탑은 그 자리에서 디코딩한다)
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let audio_asset = engine.load_audio_asset("tests/assets/audio/wav_s16_stereo.wav").unwrap();
	/// assert!(audio_asset.is_loaded());
	/// assert!(audio_asset.get_duration() > 0.0);
	/// assert!(engine.load_audio_asset("tests/assets/audio/missing.wav").is_err());
	/// ```
	//==============================================================================
	pub fn load_audio_asset(&self, relative_path: &str) -> Result<Rc<AudioAsset>, String> {
		let audio_asset = Rc::new(AudioAsset::new());
		if let Some(load_error) = audio_asset.load_from_asset(self, relative_path).get_load_error() {
			return Err(load_error);
		}
		Ok(audio_asset)
	}

	//==============================================================================
	/// 오디오 애셋 경로 풀기. (데스크탑은 애셋 폴더 후보에서 찾은 경로, 웹은 asset_base_url + 상대 경로, 웹의 data: 주소는 그대로)
	///
	/// # 인자
	/// - `relative_path`: 애셋 폴더 기준 상대 경로.
	///
	/// # 돌려줌
	/// 읽을 경로. (웹은 주소를 경로 꼴에 담는다)
	//==============================================================================
	pub fn resolve_audio_asset_path(&self, relative_path: &str) -> PathBuf {
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.resolve_asset_path(relative_path)
		}
		#[cfg(target_arch = "wasm32")]
		{
			PathBuf::from(self.create_asset_url(relative_path))
		}
	}

	//==============================================================================
	/// 오디오 컨텍스트 재개. (focus, visibilitychange 같은 user gesture 가 아닌 복귀. gesture 전이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn resume(&self) {
		let audio_manager = self.get_audio_manager();
		audio_manager.resume_context();
	}

	//==============================================================================
	/// user gesture (click / touch / keydown) 핸들러.
	/// AudioManager 에 user gesture 발생을 알리고 컨텍스트 재개를 시도한다.
	//==============================================================================
	pub fn resume_on_user_gesture(&self) {
		let audio_manager = self.get_audio_manager();
		audio_manager.mark_user_gesture();
		audio_manager.resume_context();
	}

	//==============================================================================
	/// 종료 요청. (원본 exitApplication, 데스크탑은 이번 프레임 뒤 창을 닫고, 웹은 원본처럼 Capacitor App.exitApp 이나 window.close 를 시도하고 프레임을 멈춘다)
	//==============================================================================
	pub fn exit_application(&mut self) {
		self.is_exit_requested = true;
		platform::exit_application();
	}

	//==============================================================================
	/// 종료 요청 여부.
	///
	/// # 돌려줌
	/// exit_application 을 불렀으면 true.
	//==============================================================================
	pub fn is_exit_requested(&self) -> bool {
		self.is_exit_requested
	}

	//==============================================================================
	/// 커서 보이기 설정.
	///
	/// # 인자
	/// - `is_visible`: false 면 창이나 캔버스 위에서 마우스 커서를 숨긴다. (플랫폼이 붙이기 전에는 아무것도 하지 않는다)
	//==============================================================================
	pub fn set_visible_cursor(&self, is_visible: bool) {
		platform::set_cursor_visible(self.platform_window.as_ref(), is_visible);
	}

	//==============================================================================
	/// 해상도 다시 맞추기. (원본 resize, 설정을 바꾼 뒤 부르면 플랫폼이 다음 프레임 첫머리에 캔버스와 뷰 영역을 다시 셈하고 씬마다 resize 를 부른다)
	//==============================================================================
	pub fn resize(&mut self) {
		self.is_resize_requested = true;
	}

	//==============================================================================
	/// 캔버스. (원본 getCanvas, 웹은 그리는 캔버스 요소, 데스크탑은 창, 플랫폼이 붙이기 전에는 None)
	///
	/// # 돌려줌
	/// 플랫폼 창 손잡이. (run 으로 창을 만들기 전에는 None)
	//==============================================================================
	pub fn get_canvas(&self) -> Option<&PlatformWindow> {
		self.platform_window.as_ref()
	}

	//==============================================================================
	/// 글자 입력칸이 포커스되어 있는지. (원본 isTextInputFocused, 웹은 문서의 초점이 글자 input, textarea, contenteditable 이면 참)
	/// - 원본의 엔진 입력 필드는 숨긴 DOM input 에 초점을 두어 여기서 참이 된다. 데스크탑에는 문서가 없어 엔진 입력 필드의 포커스를 본다.
	///   (입력 매니저의 is_typing_in_text_field 와 같다)
	///
	/// # 돌려줌
	/// 글자 입력칸에 초점이 있으면 true.
	//==============================================================================
	pub fn is_text_input_focused(&self) -> bool {
		self.input_manager.is_typing_in_text_field()
	}

	//==============================================================================
	/// 창 제목 설정.
	///
	/// # 인자
	/// - `title`: 새 창 제목. (웹은 문서 제목, 데스크탑은 창이 생기기 전에는 아무것도 하지 않는다)
	//==============================================================================
	pub fn set_title(&self, title: &str) {
		platform::set_title(self.platform_window.as_ref(), title);
	}

	//==============================================================================
	/// 시스템 클립보드에 글 쓰기. (데스크탑 arboard, 웹 navigator.clipboard.writeText. 원본 UIInputField.writeClipboard 자리)
	///
	/// # 인자
	/// - `text`: 쓸 글.
	//==============================================================================
	pub fn write_clipboard_text(&self, text: &str) {
		platform::write_clipboard_text(text);
	}

	//==============================================================================
	/// 시스템 클립보드 글 읽기. (데스크탑만 그 자리에서 읽는다. 웹은 비동기라 None 이고 read_clipboard_text_with_callback 으로 읽는다)
	///
	/// # 돌려줌
	/// 클립보드 글. (웹, 또는 클립보드를 못 읽으면 None)
	//==============================================================================
	pub fn read_clipboard_text(&self) -> Option<String> {
		platform::read_clipboard_text()
	}

	//==============================================================================
	/// 시스템 클립보드 글을 읽어 콜백에 넘기기. (원본 UIInputField.readClipboard 가 돌려주는 Promise 의 then 자리, 못 읽거나 거부되면 빈 글)
	/// - 웹은 navigator.clipboard.readText 가 끝난 뒤 부르고, 데스크탑은 그 자리에서 읽어 바로 부른다.
	///
	/// # 인자
	/// - `callback`: 읽은 글을 받는 함수. (못 읽거나 거부되면 빈 글)
	///
	/// # 예제
	/// 붙여넣을 글 읽기
	/// ```rust,no_run
	/// use vanilla::Engine;
	///
	/// fn paste_text(engine: &Engine) {
	/// 	engine.read_clipboard_text_with_callback(|clipboard_text| {
	/// 		println!("{}", clipboard_text);
	/// 	});
	/// }
	/// ```
	//==============================================================================
	pub fn read_clipboard_text_with_callback(&self, callback: impl FnOnce(String) + 'static) {
		platform::read_clipboard_text_with_callback(Box::new(callback));
	}

	//==============================================================================
	/// 시스템 클립보드 글 읽기. (원본 await engine.readClipboardText(), 클립보드가 없거나 거부되면 원본 null 자리의 None)
	/// - 웹은 navigator.clipboard.readText 가 끝난 뒤, 데스크탑은 그 자리에서 읽어 풀린다.
	///
	/// # 돌려줌
	/// 클립보드 글로 풀리는 Promise. (클립보드가 없거나 거부되면 None)
	///
	/// # 예제
	/// 엔진 작업에서 클립보드 글 기다리기
	/// ```rust,no_run
	/// use vanilla::Engine;
	///
	/// fn paste_text(engine: &mut Engine) {
	/// 	engine.spawn_task(|mut engine_context| async move {
	/// 		let clipboard_text = engine_context.with(|engine| engine.read_clipboard_text_async()).await;
	/// 		println!("{}", clipboard_text.unwrap_or_default());
	/// 	});
	/// }
	/// ```
	//==============================================================================
	pub fn read_clipboard_text_async(&self) -> Promise<Option<String>> {
		let (clipboard_promise, clipboard_resolver) = Promise::create();
		platform::read_optional_clipboard_text_with_callback(Box::new(move |clipboard_text: Option<String>| {
			clipboard_resolver.resolve(clipboard_text);
		}));
		clipboard_promise
	}

	//==============================================================================
	/// 엔진 설정 반환.
	///
	/// # 돌려줌
	/// Engine::new 에 넘긴 설정. (set_maximum_frame_per_second 가 바꾼 최대 FPS 도 여기에 있다)
	//==============================================================================
	pub fn get_engine_configuration(&self) -> &EngineConfiguration {
		&self.engine_configuration
	}

	//==============================================================================
	/// 최대 FPS 설정. (실행 중에 바꿀 수 있다, 0 이면 제한하지 않는다. 0 보다 작거나 유한하지 않으면 0)
	/// - EngineConfiguration.maximum_frame_per_second 에 적는다. 플랫폼이 프레임마다 그 값을 읽어 다음 프레임부터 따른다.
	///
	/// # 인자
	/// - `maximum_frame_per_second`: 초당 최대 프레임 수. (0 이면 제한 없음)
	///
	/// # 예제
	/// 실행 중에 30 FPS 로 묶고, 잘못된 값은 제한 없음으로
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration};
	///
	/// let mut engine = Engine::new(EngineConfiguration::new());
	/// engine.set_maximum_frame_per_second(30.0);
	/// assert_eq!(engine.get_maximum_frame_per_second(), 30.0);
	/// engine.set_maximum_frame_per_second(-5.0);
	/// assert_eq!(engine.get_maximum_frame_per_second(), 0.0);
	/// ```
	//==============================================================================
	pub fn set_maximum_frame_per_second(&mut self, maximum_frame_per_second: f32) {
		if !maximum_frame_per_second.is_finite() || maximum_frame_per_second < 0.0 {
			self.engine_configuration.maximum_frame_per_second = 0.0;
			return;
		}
		self.engine_configuration.maximum_frame_per_second = maximum_frame_per_second;
	}

	//==============================================================================
	/// 최대 FPS 반환. (0 이면 제한하지 않음)
	///
	/// # 돌려줌
	/// 초당 최대 프레임 수. (기본 0)
	//==============================================================================
	pub fn get_maximum_frame_per_second(&self) -> f32 {
		self.engine_configuration.maximum_frame_per_second
	}

	//==============================================================================
	/// 시간 관리자.
	///
	/// # 돌려줌
	/// 엔진의 시간 관리자. (프레임 간격, 누적 시간, 초당 프레임 수)
	//==============================================================================
	pub fn get_time_manager(&self) -> &TimeManager {
		&self.time_manager
	}

	//==============================================================================
	/// 시간 관리자. (변경용)
	///
	/// # 돌려줌
	/// 엔진의 시간 관리자. (시간 배율, 프레임 간격 상한을 바꾼다)
	//==============================================================================
	pub fn get_time_manager_mut(&mut self) -> &mut TimeManager {
		&mut self.time_manager
	}

	//==============================================================================
	/// 뷰 매니저.
	///
	/// # 돌려줌
	/// 엔진의 뷰 매니저. (뷰 크기, 뷰 영역, 세이프 에어리어)
	//==============================================================================
	pub fn get_view_manager(&self) -> &ViewManager {
		&self.view_manager
	}

	//==============================================================================
	/// 뷰 매니저. (변경용)
	///
	/// # 돌려줌
	/// 엔진의 뷰 매니저. (뷰 스케일 모드, 렌더 배율 상한 같은 설정을 바꾼다)
	//==============================================================================
	pub fn get_view_manager_mut(&mut self) -> &mut ViewManager {
		&mut self.view_manager
	}

	//==============================================================================
	/// 입력 매니저.
	///
	/// # 돌려줌
	/// 엔진의 입력 매니저. (키, 마우스, 터치, 휠)
	//==============================================================================
	pub fn get_input_manager(&self) -> &InputManager {
		&self.input_manager
	}

	//==============================================================================
	/// 입력 매니저. (변경용)
	///
	/// # 돌려줌
	/// 엔진의 입력 매니저.
	//==============================================================================
	pub fn get_input_manager_mut(&mut self) -> &mut InputManager {
		&mut self.input_manager
	}

	//==============================================================================
	/// 마우스 커서. (켜면 프레임 끝마다 창이나 캔버스의 커서를 정한다)
	///
	/// # 돌려줌
	/// 엔진의 마우스 커서. (단추는 &Engine 으로 mark_over_button 을 부른다)
	//==============================================================================
	pub fn get_pointer_cursor(&self) -> &PointerCursor {
		&self.pointer_cursor
	}

	//==============================================================================
	/// 마우스 커서. (변경용)
	///
	/// # 돌려줌
	/// 엔진의 마우스 커서. (켜기, 화면 커서, 그림 커서를 정한다)
	//==============================================================================
	pub fn get_pointer_cursor_mut(&mut self) -> &mut PointerCursor {
		&mut self.pointer_cursor
	}

	//==============================================================================
	/// 주된 손이 손가락인지. (CSS 의 pointer: coarse, 데스크탑은 크로미엄처럼 윈도우 태블릿과 리눅스 터치스크린 기기에서 참)
	///
	/// # 돌려줌
	/// 주된 손이 손가락이면 true. (맥은 늘 false)
	//==============================================================================
	pub fn is_coarse_pointer(&self) -> bool {
		platform::is_coarse_pointer()
	}

	//==============================================================================
	/// 오디오 매니저. (메서드가 &self 라 컴포넌트가 받는 &Engine 으로도 플레이어를 만들 수 있다)
	///
	/// # 돌려줌
	/// 엔진의 오디오 매니저.
	//==============================================================================
	pub fn get_audio_manager(&self) -> &AudioManager {
		&self.audio_manager
	}

	//==============================================================================
	/// 오디오 매니저. (가변)
	///
	/// # 돌려줌
	/// 엔진의 오디오 매니저.
	//==============================================================================
	pub fn get_audio_manager_mut(&mut self) -> &mut AudioManager {
		&mut self.audio_manager
	}

	//==============================================================================
	/// 플랫폼 정보. (원본 Platform, 운영체제와 브라우저 종류)
	///
	/// # 돌려줌
	/// 엔진을 만들 때 읽은 플랫폼 정보.
	//==============================================================================
	pub fn get_platform(&self) -> &Platform {
		&self.platform
	}

	//==============================================================================
	/// 페이지가 가려져 있는지. (다른 탭, 앱 전환, 창이 가려짐. 데스크탑은 창 가림(맥, X11), 최소화(윈도우), 앱 멈춤, 웨이랜드 프레임 콜백이 3 초 넘게 오지 않음)
	///
	/// # 돌려줌
	/// 마지막 페이지 이벤트가 Hidden 이면 true. (Visible 이 오면 false, 처음 false)
	//==============================================================================
	pub fn is_page_hidden(&self) -> bool {
		self.is_page_hidden
	}

	//==============================================================================
	/// 페이지 이벤트 리스너 더하기. (리스너를 버리면 더 부르지 않는다, &Engine 으로도 더할 수 있다)
	///
	/// # 인자
	/// - `page_event_listener`: 더할 리스너. (엔진은 약한 참조로 든다, 부른 쪽이 살려 둔다)
	///
	/// # 예제
	/// 가려짐과 포커스 이벤트 받기
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{Engine, EngineConfiguration, PageEvent, PageEventListener};
	///
	/// let mut engine = Engine::new(EngineConfiguration::new());
	/// let received_list: Rc<RefCell<Vec<PageEvent>>> = Rc::new(RefCell::new(Vec::new()));
	/// let listener_received_list = received_list.clone();
	/// let page_event_listener: PageEventListener = Rc::new(RefCell::new(move |page_event: PageEvent| {
	/// 	listener_received_list.borrow_mut().push(page_event);
	/// }));
	/// engine.add_page_event_listener(&page_event_listener);
	/// engine.dispatch_page_event(PageEvent::Hidden);
	/// assert!(engine.is_page_hidden());
	/// engine.dispatch_page_event(PageEvent::Visible);
	/// assert_eq!(*received_list.borrow(), vec![PageEvent::Hidden, PageEvent::Visible]);
	/// engine.remove_page_event_listener(&page_event_listener);
	/// engine.dispatch_page_event(PageEvent::Focused);
	/// assert_eq!(received_list.borrow().len(), 2);
	/// ```
	//==============================================================================
	pub fn add_page_event_listener(&self, page_event_listener: &PageEventListener) {
		self.page_event_listener_list.borrow_mut().push(Rc::downgrade(page_event_listener));
	}

	//==============================================================================
	/// 페이지 이벤트 리스너 빼기. (같은 리스너를 더 부르지 않는다, 버린 리스너도 빠진다)
	///
	/// # 인자
	/// - `page_event_listener`: 뺄 리스너. (add_page_event_listener 에 넘긴 것)
	//==============================================================================
	pub fn remove_page_event_listener(&self, page_event_listener: &PageEventListener) {
		let target_listener = Rc::downgrade(page_event_listener);
		self.page_event_listener_list.borrow_mut().retain(|weak_listener| weak_listener.strong_count() > 0 && !weak_listener.ptr_eq(&target_listener));
	}

	//==============================================================================
	/// 페이지 이벤트 알리기. (플랫폼이 부르고, 원본처럼 게임이 불러도 된다. 가려짐을 적고 살아 있는 리스너에 그 자리에서 넘긴다)
	/// - 리스너가 다른 일로 빌려져 있으면 그 리스너는 이번 이벤트를 건너뛴다.
	///
	/// # 인자
	/// - `page_event`: 알릴 이벤트. (Hidden 이면 가려짐을 적고 Visible 이면 지운다)
	//==============================================================================
	pub fn dispatch_page_event(&mut self, page_event: PageEvent) {
		match page_event {
			PageEvent::Hidden => {
				self.is_page_hidden = true;
			}
			PageEvent::Visible => {
				self.is_page_hidden = false;
			}
			_ => {
			}
		}
		let listener_list: Vec<PageEventListener> = {
			let mut page_event_listener_list = self.page_event_listener_list.borrow_mut();
			page_event_listener_list.retain(|weak_listener| weak_listener.strong_count() > 0);
			page_event_listener_list.iter().filter_map(|weak_listener| weak_listener.upgrade()).collect()
		};
		for page_event_listener in listener_list {
			if let Ok(mut listener) = page_event_listener.try_borrow_mut() {
				(*listener)(page_event);
			}
		}
	}

	//==============================================================================
	/// 지금 프레임의 번호. (원본처럼 첫 프레임이 1, 검증용 화면 저장의 프레임 번호도 이것)
	///
	/// # 돌려줌
	/// 지금 프레임 번호. (프레임이 끝날 때마다 1 씩 는다)
	//==============================================================================
	pub fn get_frame_number(&self) -> u64 {
		self.frame_number
	}

	//==============================================================================
	/// 엔진 판 글.
	///
	/// # 돌려줌
	/// 엔진 판. ("0.3.2" 꼴)
	//==============================================================================
	pub fn get_version_string(&self) -> String {
		self.version.to_string()
	}

	//==============================================================================
	/// 검증용 화면 저장 요청. (그 프레임을 다 그린 뒤 화면의 PNG 바이트를 넘긴다, 원본 requestFrameCapture 는 PNG data URL 을 프로미스로 넘긴다)
	/// - 이미 지난 번호면 이번(프레임 사이에 부르면 다음) 프레임을 저장한다.
	/// - 주소 쿼리(웹 ?capture=120,240&captureName=shot_{frame}.png&captureExit=1)와 환경 변수(데스크탑 VANILLA_CAPTURE_*)로도 건다.
	///
	/// # 인자
	/// - `frame_number`: 저장할 프레임 번호. (지난 번호면 지금 프레임)
	/// - `capture_handler`: 그 프레임 화면의 PNG 바이트를 받는 함수.
	///
	/// # 예제
	/// 120 번째 프레임을 파일로 저장하기
	/// ```rust,no_run
	/// use vanilla::Engine;
	///
	/// fn capture_frame(engine: &mut Engine) {
	/// 	engine.request_frame_capture(120, Box::new(|png_bytes: &[u8]| {
	/// 		std::fs::write("capture_120.png", png_bytes).unwrap();
	/// 	}));
	/// }
	/// ```
	//==============================================================================
	pub fn request_frame_capture(&mut self, frame_number: u64, capture_handler: FrameCaptureHandler) {
		let target_frame_number = frame_number.max(self.frame_number);
		self.frame_capture_handler_map.entry(target_frame_number).or_default().push(capture_handler);
	}

	//==============================================================================
	/// 검증용 화면 저장 요청. (원본 await engine.requestFrameCapture(번호), 그 프레임을 다 그린 화면의 PNG 바이트로 풀리는 Promise. 원본은 PNG data URL)
	///
	/// # 인자
	/// - `frame_number`: 저장할 프레임 번호. (지난 번호면 지금 프레임)
	///
	/// # 돌려줌
	/// 그 프레임 화면의 PNG 바이트로 풀리는 Promise.
	//==============================================================================
	pub fn request_frame_capture_async(&mut self, frame_number: u64) -> Promise<Vec<u8>> {
		let (capture_promise, capture_resolver) = Promise::create();
		self.request_frame_capture(frame_number, Box::new(move |png_bytes: &[u8]| {
			capture_resolver.resolve(png_bytes.to_vec());
		}));
		capture_promise
	}
}

//==============================================================================
/// 화면 저장 요청. (검증용, 데스크탑은 환경 변수, 웹은 원본처럼 주소 쿼리로 켠다)
/// - VANILLA_CAPTURE_PATH:   저장 경로. "{frame}" 은 프레임 번호로 바뀐다. 예: shots/shot_{frame}.png
/// - VANILLA_CAPTURE_FRAMES: 저장할 프레임 번호, 쉼표로 여럿. (기본 120)
/// - VANILLA_CAPTURE_EXIT:   1 이면 마지막 저장 뒤 끝낸다.
/// - VANILLA_CAPTURE_SHOW_WINDOW: 1 이면 창을 보인다. (찍기 모드의 데스크탑 창은 기본으로 숨긴다)
/// - 웹: ?capture=120,240 (번호, 빈 값이면 120), captureName=파일 이름("{frame}" 은 번호, 기본 capture_{frame}.png), captureExit=1. 내려받기로 저장한다.
//==============================================================================
struct CaptureRequest {
	output_path: String,
	frame_numbers: Vec<u64>,
	is_exit_after_last_capture: bool,
}

impl CaptureRequest {
	//==============================================================================
	/// 환경 변수에서 읽기. (경로가 없으면 None)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn create_from_environment() -> Option<CaptureRequest> {
		let output_path = std::env::var("VANILLA_CAPTURE_PATH").ok()?;
		let frame_numbers_text = std::env::var("VANILLA_CAPTURE_FRAMES").unwrap_or_else(|_| String::from("120"));
		let frame_numbers = parse_capture_frame_numbers(&frame_numbers_text);
		let is_exit_after_last_capture = std::env::var("VANILLA_CAPTURE_EXIT").map(|value| value == "1").unwrap_or(false);
		Some(CaptureRequest {
			output_path,
			frame_numbers,
			is_exit_after_last_capture,
		})
	}

	//==============================================================================
	/// 주소 쿼리에서 읽기. (웹, capture 가 없으면 None)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	fn create_from_environment() -> Option<CaptureRequest> {
		let frame_numbers_text = platform::read_page_query_value("capture")?;
		let frame_numbers = parse_capture_frame_numbers(&frame_numbers_text);
		let output_path = platform::read_page_query_value("captureName").filter(|capture_name| !capture_name.is_empty()).unwrap_or_else(|| String::from("capture_{frame}.png"));
		let is_exit_after_last_capture = platform::read_page_query_value("captureExit").map(|value| value == "1").unwrap_or(false);
		Some(CaptureRequest {
			output_path,
			frame_numbers,
			is_exit_after_last_capture,
		})
	}
}

//==============================================================================
/// 엔진 실행자. (엔진, 그래픽, 씬 목록을 들고 프레임을 돈다. 플랫폼이 들고 부른다)
/// - 게임 코드는 직접 만들지 않는다. Engine::run 이 엔진과 첫 씬으로 만들어 플랫폼에 넘긴다.
///
/// # 예제
/// Engine::run 이 실행자를 만들어 돈다
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, Scene, SceneBase};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// engine.run(Box::new(MainScene { scene_base: SceneBase::new() })).unwrap();
/// ```
//==============================================================================
pub struct EngineRunner {
	pub(crate) engine: Engine,
	graphic: Option<Graphic>,
	scene_list: Vec<Box<dyn Scene>>,
	/// 내리는 중인 씬. (finalize, unload 를 불렀고 unload 가 맡긴 작업을 기다린다. 원본처럼 목록에 남아 tick 과 draw 를 받는다)
	unloading_scene_id_list: Vec<SceneId>,
	/// 목록에서 뺐지만 작업이 남아 살려 둔 씬. (tick, draw 는 받지 않고, 작업의 with 가 이 씬을 받는다)
	detached_scene_list: Vec<Box<dyn Scene>>,
	/// 내리기가 끝나기를 기다리는 Promise. (기다리는 씬 핸들 목록, 다 내리면 푼다)
	unload_waiting_list: Vec<(Vec<SceneId>, PromiseResolver<()>)>,
	start_instant: Instant,
	capture_request: Option<CaptureRequest>,
}

impl EngineRunner {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub(crate) fn new(mut engine: Engine, scene: Box<dyn Scene>) -> EngineRunner {
		engine.load_scene(scene);
		EngineRunner {
			engine,
			graphic: None,
			scene_list: Vec::new(),
			unloading_scene_id_list: Vec::new(),
			detached_scene_list: Vec::new(),
			unload_waiting_list: Vec::new(),
			start_instant: Instant::now(),
			capture_request: CaptureRequest::create_from_environment(),
		}
	}

	//==============================================================================
	/// 그래픽 넘겨받기. (플랫폼이 GL 컨텍스트를 만든 뒤 한 번. 기본 폰트도 이때 읽는다)
	//==============================================================================
	pub(crate) fn attach_graphic(&mut self, mut graphic: Graphic) {
		let is_rebake_text_on_font_load = self.engine.engine_configuration.rebake_text_on_font_load;
		graphic.get_text_string_texture_cache().set_rebake_text_on_font_load(is_rebake_text_on_font_load);
		self.load_default_fonts(&mut graphic);

		// 웹: 기본 폰트가 아직 없고 default_font_url 이 있으면 거기서 받는다. (받는 동안은 시스템 글꼴로 그린다)
		#[cfg(target_arch = "wasm32")]
		{
			let default_font_url = self.engine.engine_configuration.default_font_url.clone();
			let has_default_font = graphic.get_text_string_texture_cache().has_default_font();
			if !has_default_font && !default_font_url.is_empty() {
				let load_handle = AssetLoadHandle::create_pending(&default_font_url);
				platform::request_asset_bytes(default_font_url, load_handle.clone(), false);
				self.engine.pending_default_font_handle = Some(load_handle);
			}
		}
		self.graphic = Some(graphic);
	}

	//==============================================================================
	/// 그래픽의 묶음 기억 잊기. (웹 플랫폼이 GL 컨텍스트를 잃거나 되살릴 때, 원본 Graphic 의 webglcontextlost / webglcontextrestored)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	pub(crate) fn invalidate_render_state_cache(&self) {
		if let Some(graphic) = &self.graphic {
			graphic.invalidate_render_state_cache();
		}
	}

	//==============================================================================
	/// 그래픽. (데스크탑 플랫폼이 프레임 끝에 렌더 영역을 늘일 때, 붙이기 전에는 None)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn get_graphic(&self) -> Option<&Graphic> {
		self.graphic.as_ref()
	}

	//==============================================================================
	/// 요청된 프레임이면 화면을 PNG 로 저장. (화면을 넘기기 전에 부른다)
	//==============================================================================
	fn capture_frame_if_requested(&mut self) {
		let frame_number = self.engine.frame_number;
		// request_frame_capture 로 건 함수들. (PNG 바이트)
		if let Some(capture_handler_list) = self.engine.frame_capture_handler_map.remove(&frame_number) {
			if let Some(graphic) = &self.graphic {
				let (width, height, pixels) = graphic.read_framebuffer_pixels();
				match encode_png(&pixels, width, height) {
					Ok(png_bytes) => {
						for capture_handler in capture_handler_list {
							capture_handler(&png_bytes);
						}
					}
					Err(error) => {
						eprintln!("Engine: 화면 저장 실패: {}", error);
					}
				}
			}
		}
		let capture_request = match &mut self.capture_request {
			Some(capture_request) => {
				capture_request
			}
			None => {
				return;
			}
		};
		if !capture_request.frame_numbers.contains(&frame_number) {
			return;
		}
		let graphic = match &self.graphic {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let (width, height, pixels) = graphic.read_framebuffer_pixels();
		let output_path = capture_request.output_path.replace("{frame}", &frame_number.to_string());
		let save_result = encode_png(&pixels, width, height).and_then(|png_bytes| platform::save_capture_file(&output_path, &png_bytes));
		match save_result {
			Ok(()) => {
				eprintln!("Engine: 화면 저장 {} ({}x{})", output_path, width, height);
			}
			Err(error) => {
				eprintln!("Engine: 화면 저장 실패 {}: {}", output_path, error);
			}
		}
		capture_request.frame_numbers.retain(|requested_frame_number| *requested_frame_number != frame_number);
		if capture_request.frame_numbers.is_empty() && capture_request.is_exit_after_last_capture {
			self.engine.exit_application();
		}
	}

	//==============================================================================
	/// 기본 폰트 읽기. (미리 받은 바이트, 그다음 데스크탑 시스템 글꼴 사슬과 대체 글꼴)
	/// - 데스크탑 사슬: default_font_paths 가 비었으면 원본 SYSTEM_FONT_STRING 의 이름 차례로 운영체제 글꼴에서 찾는다. (맥은 크롬처럼 -apple-system 을 모르는 이름으로 넘겨 Apple SD Gothic Neo, 윈도우 Segoe UI 와 맑은 고딕)
	///   얼굴은 그 굵기와 기울기의 글자를 처음 쓸 때 읽는다.
	/// - 대체 글꼴: fallback_font_paths 가 비었으면 운영체제가 고른다.
	//==============================================================================
	fn load_default_fonts(&self, graphic: &mut Graphic) {
		let engine_configuration = self.engine.get_engine_configuration();
		let text_string_texture_cache = graphic.get_text_string_texture_cache();
		let font_source_list = [
			(false, &engine_configuration.default_font_bytes),
			(true, &engine_configuration.default_bold_font_bytes),
		];
		for (is_bold, font_bytes) in font_source_list {
			if let Some(font_bytes) = font_bytes {
				if let Err(error) = text_string_texture_cache.set_default_font(is_bold, font_bytes.clone()) {
					eprintln!("Engine: {} (미리 받은 폰트)", error);
				}
			}
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			use crate::core::textstringtexturecache::{parse_font_string, FontFileSource};
			if engine_configuration.default_font_paths.is_empty() && engine_configuration.default_bold_font_paths.is_empty() {
				let system_font_description = parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
				text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
			}
			else {
				let path_font_list = [
					(400, &engine_configuration.default_font_paths),
					(700, &engine_configuration.default_bold_font_paths),
				];
				for (weight, font_paths) in path_font_list {
					for (path_index, font_path) in font_paths.iter().enumerate() {
						if !font_path.exists() {
							continue;
						}
						let source = FontFileSource {
							path: font_path.clone(),
							face_index: None,
							post_script_name: None,
						};
						text_string_texture_cache.add_system_font_candidate(&format!("default font path {}", path_index), weight, false, source);
					}
				}
			}
			if engine_configuration.fallback_font_paths.is_empty() {
				text_string_texture_cache.set_system_font_fallback_enabled(true);
			}
			else {
				for font_path in engine_configuration.fallback_font_paths.iter() {
					if font_path.exists() {
						text_string_texture_cache.add_fallback_font_candidate(font_path.clone());
					}
				}
			}
			if !text_string_texture_cache.has_default_font() && !text_string_texture_cache.has_system_font() {
				eprintln!("Engine: 기본 폰트를 찾지 못했습니다. 글자가 보이지 않습니다.");
			}
		}
	}

	//==============================================================================
	/// 씬마다 해상도 변경 알림.
	//==============================================================================
	pub(crate) fn notify_scene_resize(&mut self) {
		let canvas_native_size = self.engine.view_manager.get_canvas_native_size();
		let mut scene_list = std::mem::take(&mut self.scene_list);
		for scene in scene_list.iter_mut() {
			scene.resize(&mut self.engine, canvas_native_size);
		}
		self.scene_list = scene_list;
	}

	//==============================================================================
	/// 바이트가 온 글꼴을 글자 캐시에 등록. (프레임 시작에)
	//==============================================================================
	fn register_pending_fonts(&mut self) {
		self.register_pending_default_font();
		if self.engine.pending_font_registration_list.is_empty() && self.engine.pending_font_operation_list.is_empty() {
			return;
		}
		let graphic = match self.graphic.as_mut() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let pending_font_operation_list = std::mem::take(&mut self.engine.pending_font_operation_list);
		for pending_font_operation in pending_font_operation_list {
			let text_string_texture_cache = graphic.get_text_string_texture_cache();
			match pending_font_operation {
				PendingFontOperation::Unregister(family, source_key) => {
					text_string_texture_cache.unregister_font_face(&family, &source_key);
				}
				PendingFontOperation::Order(family, source_key_list) => {
					text_string_texture_cache.set_font_face_order(&family, &source_key_list);
				}
				PendingFontOperation::ClearTextCache => {
					text_string_texture_cache.clear();
				}
			}
		}
		let pending_font_registration_list = std::mem::take(&mut self.engine.pending_font_registration_list);
		for (family, font_face_descriptors, font_load_handle) in pending_font_registration_list {
			let load_handle = font_load_handle.get_load_handle().clone();
			if !load_handle.is_finished() {
				self.engine.pending_font_registration_list.push((family, font_face_descriptors, font_load_handle));
				continue;
			}
			let font_bytes = match load_handle.get_bytes() {
				Some(font_bytes) => {
					font_bytes
				}
				None => {
					font_load_handle.finish(Err(load_handle.get_error_message().unwrap_or_default()));
					continue;
				}
			};
			let text_string_texture_cache = graphic.get_text_string_texture_cache();
			match text_string_texture_cache.register_font_face(&family, font_face_descriptors, font_bytes.to_vec()) {
				Ok(_) => {
					font_load_handle.finish(Ok(()));
				}
				Err(error) => {
					eprintln!("Engine: {} ({})", error, load_handle.get_asset_path());
					font_load_handle.finish(Err(error.to_string()));
				}
			}
		}
	}

	//==============================================================================
	/// default_font_url 에서 받은 바이트를 기본 폰트(보통, 굵게 둘 다)로 등록. (웹)
	//==============================================================================
	fn register_pending_default_font(&mut self) {
		let is_finished = match &self.engine.pending_default_font_handle {
			Some(load_handle) => {
				load_handle.is_finished()
			}
			None => {
				return;
			}
		};
		if !is_finished {
			return;
		}
		let load_handle = match self.engine.pending_default_font_handle.take() {
			Some(load_handle) => {
				load_handle
			}
			None => {
				return;
			}
		};
		let font_bytes = match load_handle.get_bytes() {
			Some(font_bytes) => {
				font_bytes
			}
			None => {
				return;
			}
		};
		let graphic = match self.graphic.as_mut() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		// 보통 굵기 얼굴로만 올린다. (원본 FontFace 를 서술자 없이 만든 것과 같아, 굵은 글은 가짜 굵게로 그린다)
		let text_string_texture_cache = graphic.get_text_string_texture_cache();
		if let Err(error) = text_string_texture_cache.set_default_font(false, font_bytes.to_vec()) {
			eprintln!("Engine: {} ({})", error, load_handle.get_asset_path());
		}
	}

	//==============================================================================
	/// 페이지 가시성 복원 알림. (소리를 다시 깨우고 입력을 비운 뒤 씬마다 on_page_visibility_restored)
	//==============================================================================
	pub(crate) fn notify_page_visibility_restored(&mut self) {
		self.recover_engine_state();
		let mut scene_list = std::mem::take(&mut self.scene_list);
		for scene in scene_list.iter_mut() {
			scene.on_page_visibility_restored(&mut self.engine);
		}
		scene_list.append(&mut self.scene_list);
		self.scene_list = scene_list;
	}

	//==============================================================================
	/// 페이지 프로세스 복원 알림. (소리를 다시 깨우고 입력을 비운 뒤 씬마다 on_page_process_restored)
	//==============================================================================
	pub(crate) fn notify_page_process_restored(&mut self) {
		self.recover_engine_state();
		let mut scene_list = std::mem::take(&mut self.scene_list);
		for scene in scene_list.iter_mut() {
			scene.on_page_process_restored(&mut self.engine);
		}
		scene_list.append(&mut self.scene_list);
		self.scene_list = scene_list;
	}

	//==============================================================================
	/// 엔진 복구. (백그라운드나 파기에서 돌아올 때. 멈춘 오디오 컨텍스트를 깨우고, 눌린 채 남은 키와 터치를 비운다)
	//==============================================================================
	fn recover_engine_state(&mut self) {
		self.engine.get_audio_manager().resume_context();
		self.engine.get_input_manager_mut().clear();
	}

	//==============================================================================
	/// 깨어난 작업 폴링. (그래픽 없이, 시험에서 프레임 하나를 흉내 낼 때)
	//==============================================================================
	#[cfg(test)]
	pub(crate) fn poll_tasks(&mut self) {
		task::begin_poll_frame(&mut self.engine);
		loop {
			task::poll_woken_tasks(&mut self.engine, &mut self.scene_list, &mut self.detached_scene_list, None);
			self.finish_scene_unloads();
			if !self.engine.task_executor.has_woken_tasks() {
				break;
			}
		}
	}

	//==============================================================================
	/// 씬 하나의 내리기 시작. (내리는 중이 아니면 finalize, unload 를 부르고 내리는 중으로 적는다)
	//==============================================================================
	fn begin_scene_unload(&mut self, scene_index: usize) {
		let scene_id = match self.scene_list[scene_index].get_scene_base().get_scene_id() {
			Some(scene_id) => {
				scene_id
			}
			None => {
				return;
			}
		};
		if self.unloading_scene_id_list.contains(&scene_id) {
			return;
		}
		self.unloading_scene_id_list.push(scene_id);
		begin_scene_instance_unload(&mut self.engine, self.scene_list[scene_index].as_mut());
	}

	//==============================================================================
	/// 내리기 마무리. (finish_scene_unloads 를 이 실행자의 목록으로)
	//==============================================================================
	fn finish_scene_unloads(&mut self) {
		let current_time = self.start_instant.elapsed().as_secs_f64();
		finish_scene_unloads(&mut self.engine, &mut self.scene_list, &mut self.unloading_scene_id_list, &mut self.detached_scene_list, &mut self.unload_waiting_list, current_time);
	}

	//==============================================================================
	/// 미뤄 둔 씬 내리기 / 불러오기.
	//==============================================================================
	pub(crate) fn apply_pending_scenes(&mut self) {
		if self.engine.is_unload_all_scenes_requested {
			self.engine.is_unload_all_scenes_requested = false;
			for scene_index in 0..self.scene_list.len() {
				self.begin_scene_unload(scene_index);
			}
		}
		while !self.engine.pending_unload_scene_id_list.is_empty() {
			let pending_unload_scene_id_list = std::mem::take(&mut self.engine.pending_unload_scene_id_list);
			for scene_id in pending_unload_scene_id_list {
				self.engine.pending_scene_list.retain(|pending_scene| pending_scene.get_scene_base().get_scene_id() != Some(scene_id));
				let scene_index = self.scene_list.iter().position(|scene| scene.get_scene_base().get_scene_id() == Some(scene_id));
				if let Some(scene_index) = scene_index {
					self.begin_scene_unload(scene_index);
				}
			}
		}
		for (scene_id, unload_resolver) in std::mem::take(&mut self.engine.pending_unload_resolver_list) {
			let waiting_scene_id_list = match scene_id {
				Some(scene_id) => {
					vec![scene_id]
				}
				None => {
					self.scene_list.iter().filter_map(|scene| scene.get_scene_base().get_scene_id()).collect()
				}
			};
			self.unload_waiting_list.push((waiting_scene_id_list, unload_resolver));
		}
		self.finish_scene_unloads();
		while !self.engine.pending_scene_list.is_empty() {
			let pending_scene_list = std::mem::take(&mut self.engine.pending_scene_list);
			for mut scene in pending_scene_list {
				if let Some(scene_id) = scene.get_scene_base().get_scene_id() {
					self.engine.loaded_scene_id_list.push(scene_id);
				}
				scene.create(&mut self.engine);
				apply_scene_root_to_view_size(&self.engine, scene.as_mut());
				let current_time = self.start_instant.elapsed().as_secs_f64();
				initialize_scene_if_ready(&mut self.engine, scene.as_mut(), current_time);
				self.scene_list.push(scene);
			}
		}
	}

	//==============================================================================
	/// 엔진 갱신. (한 프레임)
	//==============================================================================
	pub(crate) fn update_engine(&mut self) {
		// 바이트가 온 글꼴 등록.
		self.register_pending_fonts();
		let graphic = match self.graphic.as_mut() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};

		// 렌더러 처리.
		let canvas_pixel_size = self.engine.view_manager.get_canvas_pixel_size();
		graphic.apply_settings(canvas_pixel_size.x as i32, canvas_pixel_size.y as i32);

		// 시간 처리.
		let seconds_since_startup = self.start_instant.elapsed().as_secs_f64();
		self.engine.time_manager.update(seconds_since_startup);
		let time_delta = self.engine.time_manager.get_time_delta();

		// 입력 처리.
		self.engine.input_manager.tick(time_delta);

		// 질문 창. (데스크탑, 떠 있으면 원본에서 자바스크립트가 멈추듯 씬과 async 작업을 갱신하지 않고 창이 입력을 받는다)
		let is_modal_dialog_open = self.engine.is_modal_dialog_open();
		#[cfg(not(target_arch = "wasm32"))]
		{
			if is_modal_dialog_open {
				let measure_graphic: &Graphic = graphic;
				crate::core::modaldialog::tick_modal_dialog(&mut self.engine, &|text: &str| crate::core::modaldialog::measure_field_text_width(measure_graphic, text), time_delta);
			}
		}

		// 깨어난 작업 폴링. (await 다음 줄이 이번 프레임의 시간과 입력을 보고, 바꾼 것이 이번 프레임에 그려진다)
		if !is_modal_dialog_open {
			// unload 가 맡긴 작업이 끝난 씬은 원본처럼 이번 프레임부터 목록에서 빠지고, 내리기를 기다리던 작업도 원본 마이크로태스크처럼 같은 차례에 이어 간다.
			task::begin_poll_frame(&mut self.engine);
			loop {
				task::poll_woken_tasks(&mut self.engine, &mut self.scene_list, &mut self.detached_scene_list, Some(&mut *graphic));
				finish_scene_unloads(&mut self.engine, &mut self.scene_list, &mut self.unloading_scene_id_list, &mut self.detached_scene_list, &mut self.unload_waiting_list, seconds_since_startup);
				if !self.engine.task_executor.has_woken_tasks() {
					break;
				}
			}
		}

		// 그리기 통계. (지난 프레임 값을 넘겨 두고 이번 프레임을 새로 센다)
		graphic.begin_frame_statistics();

		// 씬 처리.
		// - 씬마다 그리기 앞에 뷰 좌표를 걸어 둔다. (씬이 받는 터치 좌표와 같은 좌표, 기기 픽셀 배율이 1 이 아닌 화면에서도 같은 크기와 자리로 그린다, 원본과 같다)
		//   캔버스의 실제 픽셀로 그리려면 씬에서 graphic.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0) 을 부른다.
		let mut scene_list = std::mem::take(&mut self.scene_list);
		for scene in scene_list.iter_mut() {
			// 맡긴 애셋을 기다리는 씬은 로딩 화면만. (다 읽혔으면 이번 프레임부터 돈다, 원본처럼 내리는 중이어도 불러오기가 끝나면 initialize 한다)
			if !scene.get_scene_base().is_loaded() {
				initialize_scene_if_ready(&mut self.engine, scene.as_mut(), seconds_since_startup);
			}
			if !scene.get_scene_base().is_loaded() {
				self.engine.view_manager.apply_view_rect(graphic);
				scene.draw_on_load(&mut self.engine, graphic);
				continue;
			}

			// 갱신. (씬의 터치 훅은 원본 Scene.tickTouch 처럼 tick 안의 터치 다음, 질문 창이 떠 있던 프레임은 건너뛴다)
			if !is_modal_dialog_open {
				scene.tick(&mut self.engine, time_delta);
				dispatch_scene_touch(&mut self.engine, scene.as_mut());
			}

			// 출력.
			self.engine.view_manager.apply_view_rect(graphic);
			scene.pre_draw(&mut self.engine, graphic);
			scene.draw(&mut self.engine, graphic);
			scene.post_draw(&mut self.engine, graphic);

			let is_gizmo_visible = scene.get_scene_base().is_gizmo_visible();
			if is_gizmo_visible {
				scene.draw_gizmos(&mut self.engine, graphic);
			}
		}
		scene_list.append(&mut self.scene_list);
		self.scene_list = scene_list;

		// 개발 정보 출력.
		if self.engine.engine_configuration.use_statistics {
			draw_statistics(&self.engine, graphic);
		}

		// 질문 창. (모든 것 위에, 창 안쪽 논리 좌표)
		#[cfg(not(target_arch = "wasm32"))]
		{
			if self.engine.is_modal_dialog_open() {
				self.engine.view_manager.apply_canvas_native_rect(graphic);
				crate::core::modaldialog::draw_modal_dialog(&mut self.engine, graphic);
			}
		}

		// 마우스 커서. (켜져 있고 바뀌었을 때만 플랫폼에 넘긴다)
		let is_coarse_pointer = platform::is_coarse_pointer();
		if let Some(cursor_value) = self.engine.pointer_cursor.take_cursor_change(is_coarse_pointer) {
			platform::set_cursor(self.engine.platform_window.as_ref(), &cursor_value);
		}

		// 입력 관련해서 상태 유지가 아닌, 현재 프레임이 끝난 후에는 다음 프레임에서는 상태를 유지하지 않음. (1회성)
		let input_manager = &mut self.engine.input_manager;
		input_manager.set_touch_pressed(false);
		input_manager.set_touch_released(false);
		input_manager.set_touch_cancelled(false);
		input_manager.clear_wheel_delta();
		input_manager.update_frame_snapshot();
		input_manager.get_text_input_mut().finish_frame();

		// 프레임 마무리. (묶음을 내보내고, 검증용 화면 저장은 플랫폼이 화면을 넘기기 전에 한다)
		graphic.end_frame();
		self.capture_frame_if_requested();
		self.engine.frame_number += 1;

		// 씬 목록 바꾸기.
		self.apply_pending_scenes();
	}
}

//==============================================================================
/// 개발 정보 출력. (왼쪽 위에 FPS, 드로우 콜, 정점 수, 화면 밖이라 거른 노드 수)
//==============================================================================
fn draw_statistics(engine: &Engine, graphic: &mut Graphic) {
	let view_manager = engine.get_view_manager();
	let time_manager = engine.get_time_manager();
	view_manager.apply_canvas_native_rect(graphic);

	let frame_per_second = time_manager.get_frame_per_second();
	let draw_call_count = graphic.get_last_frame_draw_call_count();
	let vertex_count = graphic.get_last_frame_vertex_count();
	let culled_node_count = graphic.get_last_frame_culled_node_count();
	let statistics_text = format!("FPS {}  DrawCall {}  Vertex {}  Culled {}", frame_per_second, draw_call_count, vertex_count, culled_node_count);

	graphic.set_font_string("14px default");
	let text_metrics = graphic.measure_text(&statistics_text);
	graphic.set_fill_color(Color::create(0.0, 0.0, 0.0, 0.6));
	graphic.draw_rect(Rect::create(4.0, 4.0, text_metrics.width + 12.0, 22.0));
	graphic.set_text_align(TextAlign::Left);
	graphic.set_text_baseline(TextBaseline::Middle);
	graphic.set_fill_color(Color::white());
	graphic.draw_fill_text(&statistics_text, 10.0, 15.0);
}

//==============================================================================
/// 화면 저장 프레임 번호 읽기. (쉼표로 여럿, 같은 번호는 한 번, 하나도 없으면 원본처럼 120)
//==============================================================================
fn parse_capture_frame_numbers(frame_numbers_text: &str) -> Vec<u64> {
	let mut frame_numbers: Vec<u64> = Vec::new();
	for part in frame_numbers_text.split(',') {
		if let Ok(frame_number) = part.trim().parse::<u64>() {
			if !frame_numbers.contains(&frame_number) {
				frame_numbers.push(frame_number);
			}
		}
	}
	if frame_numbers.is_empty() {
		frame_numbers.push(120);
	}
	frame_numbers.sort();
	frame_numbers
}

//==============================================================================
/// RGBA 픽셀을 PNG 바이트로.
//==============================================================================
fn encode_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
	let mut png_bytes: Vec<u8> = Vec::new();
	image::write_buffer_with_format(&mut std::io::Cursor::new(&mut png_bytes), pixels, width, height, image::ExtendedColorType::Rgba8, image::ImageFormat::Png).map_err(|error| error.to_string())?;
	Ok(png_bytes)
}

//==============================================================================
/// 내리기 마무리. (unload 가 맡긴 작업이 끝난 씬을 목록에서 빼 작업이 남았으면 살려 두고, 작업이 다 끝난 씬은 destroy 뒤 버리고, 기다리는 Promise 를 푼다)
/// - 불러오는 도중에 내린 씬은 원본 SceneManager.loadScene 처럼 불러오기가 끝나면 initialize 하고, 그때까지 살려 둔다. (current_time 은 엔진 시작 뒤 초)
//==============================================================================
fn finish_scene_unloads(engine: &mut Engine, scene_list: &mut Vec<Box<dyn Scene>>, unloading_scene_id_list: &mut Vec<SceneId>, detached_scene_list: &mut Vec<Box<dyn Scene>>, unload_waiting_list: &mut Vec<(Vec<SceneId>, PromiseResolver<()>)>, current_time: f64) {
	let mut unloading_index = 0;
	while unloading_index < unloading_scene_id_list.len() {
		let scene_id = unloading_scene_id_list[unloading_index];
		if let Some(scene_index) = scene_list.iter().position(|scene| scene.get_scene_base().get_scene_id() == Some(scene_id)) {
			if !scene_list[scene_index].get_scene_base().are_tracked_unloads_finished() {
				unloading_index += 1;
				continue;
			}
			let scene = scene_list.remove(scene_index);
			engine.loaded_scene_id_list.retain(|loaded_scene_id| *loaded_scene_id != scene_id);
			detached_scene_list.push(scene);
		}
		unloading_scene_id_list.remove(unloading_index);
	}
	for mut scene in std::mem::take(detached_scene_list) {
		if !scene.get_scene_base().is_loaded() {
			initialize_scene_if_ready(engine, scene.as_mut(), current_time);
		}
		let is_task_left = scene.get_scene_base().get_scene_id().map(|scene_id| engine.task_executor.has_scene_tasks(scene_id)).unwrap_or(false);
		if is_task_left || !scene.get_scene_base().is_loaded() {
			detached_scene_list.push(scene);
		}
		else {
			scene.destroy(engine);
		}
	}
	for (waiting_scene_id_list, unload_resolver) in std::mem::take(unload_waiting_list) {
		if waiting_scene_id_list.iter().any(|scene_id| unloading_scene_id_list.contains(scene_id)) {
			unload_waiting_list.push((waiting_scene_id_list, unload_resolver));
		}
		else {
			unload_resolver.resolve(());
		}
	}
}

//==============================================================================
/// 씬 하나의 내리기 시작. (원본 SceneManager.unloadScene 의 finalize, unload. 내리기 맡기기로 바꾼 뒤 unload 를 불러, unload 가 track_task 로 맡긴 것만 기다린다)
//==============================================================================
fn begin_scene_instance_unload(engine: &mut Engine, scene: &mut dyn Scene) {
	scene.finalize(engine);
	scene.get_scene_base_mut().begin_unload_tracking();
	scene.unload(engine);
}

//==============================================================================
/// 씬의 터치 훅 부르기. (원본 Scene.tickTouch: 누름, 그다음 뗌 또는 누르고 있음(누른 프레임 빼고) 또는 취소, 그리고 휠)
//==============================================================================
fn dispatch_scene_touch(engine: &mut Engine, scene: &mut dyn Scene) {
	let input_manager = engine.get_input_manager();
	let view_input_position = input_manager.get_view_input_position();
	let is_touch_pressed = input_manager.is_touch_pressed();
	let is_touch_released = input_manager.is_touch_released();
	let is_touch_moved = input_manager.is_touch_moved();
	let is_touch_cancelled = input_manager.is_touch_cancelled();
	let wheel_delta = Vector2::create(input_manager.get_wheel_delta_x(), input_manager.get_wheel_delta_y());
	let has_wheel_delta = input_manager.has_wheel_delta();
	if is_touch_pressed {
		scene.touch_press(engine, view_input_position);
	}
	if is_touch_released {
		scene.touch_release(engine, view_input_position);
	}
	else if is_touch_moved && !is_touch_pressed {
		scene.touch_move(engine, view_input_position);
	}
	else if is_touch_cancelled {
		scene.touch_cancel(engine, view_input_position);
	}
	if has_wheel_delta {
		scene.touch_wheel(engine, view_input_position, wheel_delta);
	}
}

//==============================================================================
/// 씬의 루트를 뷰 크기로 맞추기. (원본 Scene.load: 자리 0, 왼쪽 위 피벗, 콘텐츠 크기는 뷰 크기. create 바로 뒤에 부른다)
//==============================================================================
fn apply_scene_root_to_view_size(engine: &Engine, scene: &mut dyn Scene) {
	let view_size = engine.view_manager.get_view_size();
	let root = scene.get_scene_base_mut().get_root_mut();
	root.set_local_position(Vector2::zero());
	root.set_pivot(Pivot::TOP_LEFT);
	root.set_content_size(view_size);
}

//==============================================================================
/// 씬이 맡긴 애셋과 작업이 다 끝났으면 initialize 하고 불러온 것으로 친다. (원본 load 다음 0.3초 더 기다리고 set_loaded, initialize, 그리고 loadScene 의 Promise 를 푼다)
/// - current_time 은 엔진 시작 뒤 지금까지의 실제 초다. (첫 프레임 전에도 맞는 시각이어야 해서 시간 관리자의 지난 값을 쓰지 않는다)
//==============================================================================
fn initialize_scene_if_ready(engine: &mut Engine, scene: &mut dyn Scene, current_time: f64) {
	if scene.get_scene_base().is_loaded() || !scene.get_scene_base().are_tracked_assets_finished() {
		return;
	}
	let tracked_assets_finished_time = scene.get_scene_base_mut().mark_tracked_assets_finished(current_time);
	if current_time - tracked_assets_finished_time < SCENE_LOAD_EXTRA_WAIT_SECONDS {
		return;
	}
	scene.get_scene_base_mut().clear_tracked_assets();
	scene.initialize(engine);
	scene.get_scene_base_mut().set_loaded(true);
	let canvas_native_size = engine.view_manager.get_canvas_native_size();
	scene.resize(engine, canvas_native_size);
	scene.get_scene_base_mut().resolve_load_resolvers();
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::scene::SceneBase;
	use std::cell::Cell;

	//==============================================================================
	/// 시험용 씬. (create, destroy, 페이지 복원 훅이 불린 수를 센다)
	//==============================================================================
	struct CountingScene {
		scene_base: SceneBase,
		create_count: Rc<Cell<u32>>,
		destroy_count: Rc<Cell<u32>>,
		restore_count: Rc<Cell<u32>>,
	}

	impl CountingScene {
		//==============================================================================
		/// 세는 칸을 나눠 들고 생성.
		//==============================================================================
		fn create_boxed(create_count: &Rc<Cell<u32>>, destroy_count: &Rc<Cell<u32>>, restore_count: &Rc<Cell<u32>>) -> Box<dyn Scene> {
			Box::new(CountingScene {
				scene_base: SceneBase::new(),
				create_count: create_count.clone(),
				destroy_count: destroy_count.clone(),
				restore_count: restore_count.clone(),
			})
		}
	}

	impl Scene for CountingScene {
		//==============================================================================
		/// 씬 공통 상태 반환.
		//==============================================================================
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		//==============================================================================
		/// 씬 공통 상태 반환. (변경용)
		//==============================================================================
		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}

		//==============================================================================
		/// 생성. (create 수를 센다)
		//==============================================================================
		fn create(&mut self, _engine: &mut Engine) {
			self.create_count.set(self.create_count.get() + 1);
		}

		//==============================================================================
		/// 해제. (destroy 수를 센다)
		//==============================================================================
		fn destroy(&mut self, _engine: &mut Engine) {
			self.destroy_count.set(self.destroy_count.get() + 1);
		}

		//==============================================================================
		/// 페이지 프로세스 복원. (복원 수에 100 을 더한다)
		//==============================================================================
		fn on_page_process_restored(&mut self, _engine: &mut Engine) {
			self.restore_count.set(self.restore_count.get() + 100);
		}

		//==============================================================================
		/// 페이지 가시성 복원. (복원 수에 1 을 더한다)
		//==============================================================================
		fn on_page_visibility_restored(&mut self, _engine: &mut Engine) {
			self.restore_count.set(self.restore_count.get() + 1);
		}
	}

	//==============================================================================
	/// 씬 하나 내리기: 프레임이 끝날 때 그 씬만 destroy, 다른 씬은 남고, create 전인 씬은 create 없이 빠진다.
	/// (불러오기를 마친 씬으로 본다. 불러오는 도중에 내린 씬은 scene_unloaded_while_loading_still_initializes)
	//==============================================================================
	#[test]
	fn unload_scene_removes_only_that_scene() {
		let first_counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
		let second_counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
		let third_counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), CountingScene::create_boxed(&first_counts[0], &first_counts[1], &first_counts[2]));
		engine_runner.apply_pending_scenes();
		let second_scene_id = engine_runner.engine.load_scene(CountingScene::create_boxed(&second_counts[0], &second_counts[1], &second_counts[2]));
		engine_runner.apply_pending_scenes();
		assert_eq!(engine_runner.scene_list.len(), 2);
		let first_scene_id = engine_runner.scene_list[0].get_scene_base().get_scene_id().expect("핸들");
		assert_ne!(first_scene_id, second_scene_id);

		let first_scene_base_id = SceneId::from(engine_runner.scene_list[0].get_scene_base());
		assert_eq!(first_scene_base_id, first_scene_id);
		for scene in engine_runner.scene_list.iter_mut() {
			scene.get_scene_base_mut().set_loaded(true);
		}
		engine_runner.engine.unload_scene(engine_runner.scene_list[0].get_scene_base());
		assert_eq!(first_counts[1].get(), 0);
		engine_runner.apply_pending_scenes();
		assert_eq!(first_counts[1].get(), 1);
		assert_eq!(second_counts[1].get(), 0);
		assert_eq!(engine_runner.scene_list.len(), 1);
		assert_eq!(engine_runner.scene_list[0].get_scene_base().get_scene_id(), Some(second_scene_id));

		// 같은 프레임에 불러오고 내리면 create 도 destroy 도 없다. 이미 내린 핸들은 아무것도 하지 않는다.
		let third_scene_id = engine_runner.engine.load_scene(CountingScene::create_boxed(&third_counts[0], &third_counts[1], &third_counts[2]));
		engine_runner.engine.unload_scene(third_scene_id);
		engine_runner.engine.unload_scene(first_scene_id);
		engine_runner.apply_pending_scenes();
		assert_eq!(third_counts[0].get(), 0);
		assert_eq!(third_counts[1].get(), 0);
		assert_eq!(first_counts[1].get(), 1);
		assert_eq!(engine_runner.scene_list.len(), 1);
	}

	//==============================================================================
	/// 불러온 씬은 create 뒤 루트가 뷰 크기(자리 0, 왼쪽 위 피벗)가 되고, 맡긴 애셋이 끝난 뒤 0.3초 더 지나야 initialize 된다. 핸들 목록에 든다.
	//==============================================================================
	#[test]
	fn scene_root_fits_view_and_initialize_waits_extra_load_time() {
		let counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), CountingScene::create_boxed(&counts[0], &counts[1], &counts[2]));
		engine_runner.engine.get_view_manager_mut().calculate_view_rect(640, 360, 1.0);
		engine_runner.apply_pending_scenes();
		let view_size = engine_runner.engine.get_view_manager().get_view_size();
		let root = engine_runner.scene_list[0].get_scene_base().get_root();
		assert_eq!(root.get_content_size(), view_size);
		assert_eq!(root.get_pivot(), Pivot::TOP_LEFT);
		assert_eq!(root.get_local_position(), Vector2::zero());
		assert_eq!(engine_runner.engine.get_all_loaded_scenes().len(), 1);
		assert!(!engine_runner.scene_list[0].get_scene_base().is_loaded());

		let created_time = engine_runner.scene_list[0].get_scene_base_mut().mark_tracked_assets_finished(0.0);
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[0].as_mut(), created_time + 0.25);
		assert!(!engine_runner.scene_list[0].get_scene_base().is_loaded());
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[0].as_mut(), created_time + 0.31);
		assert!(engine_runner.scene_list[0].get_scene_base().is_loaded());

		engine_runner.engine.unload_all_scenes();
		engine_runner.apply_pending_scenes();
		assert!(engine_runner.engine.get_all_loaded_scenes().is_empty());
	}

	//==============================================================================
	/// 페이지 복원: 입력을 비우고 씬마다 훅을 부른다.
	//==============================================================================
	#[test]
	fn page_restore_hooks_reach_scenes_and_clear_input() {
		let counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), CountingScene::create_boxed(&counts[0], &counts[1], &counts[2]));
		engine_runner.apply_pending_scenes();
		engine_runner.engine.get_input_manager_mut().push_key("KeyA");
		assert!(engine_runner.engine.get_input_manager().is_key_pressed("KeyA"));
		engine_runner.notify_page_visibility_restored();
		assert_eq!(counts[2].get(), 1);
		assert!(!engine_runner.engine.get_input_manager().is_key_pressed("KeyA"));
		engine_runner.notify_page_process_restored();
		assert_eq!(counts[2].get(), 101);
		assert_eq!(engine_runner.scene_list.len(), 1);
	}

	//==============================================================================
	/// 페이지 이벤트: 가려짐을 적고 리스너에 넘긴다. 버린 리스너는 더 부르지 않는다.
	//==============================================================================
	#[test]
	fn page_events_reach_listeners() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let received_list: Rc<RefCell<Vec<PageEvent>>> = Rc::new(RefCell::new(Vec::new()));
		let received_list_for_listener = received_list.clone();
		let page_event_listener: PageEventListener = Rc::new(RefCell::new(move |page_event: PageEvent| {
			received_list_for_listener.borrow_mut().push(page_event);
		}));
		engine.add_page_event_listener(&page_event_listener);
		assert!(!engine.is_page_hidden());
		engine.dispatch_page_event(PageEvent::Hidden);
		assert!(engine.is_page_hidden());
		engine.dispatch_page_event(PageEvent::Blurred);
		engine.dispatch_page_event(PageEvent::Visible);
		assert!(!engine.is_page_hidden());
		assert_eq!(*received_list.borrow(), vec![PageEvent::Hidden, PageEvent::Blurred, PageEvent::Visible]);
		engine.remove_page_event_listener(&page_event_listener);
		engine.dispatch_page_event(PageEvent::Focused);
		assert_eq!(received_list.borrow().len(), 3);
	}

	//==============================================================================
	/// 최대 FPS 는 실행 중에 바꾸고, 잘못된 값은 0(제한 없음)이다.
	//==============================================================================
	#[test]
	fn maximum_frame_per_second_changes_at_runtime() {
		let mut engine = Engine::new(EngineConfiguration::new());
		assert_eq!(engine.get_maximum_frame_per_second(), 0.0);
		engine.set_maximum_frame_per_second(30.0);
		assert_eq!(engine.get_engine_configuration().maximum_frame_per_second, 30.0);
		engine.set_maximum_frame_per_second(-5.0);
		assert_eq!(engine.get_maximum_frame_per_second(), 0.0);
		engine.set_maximum_frame_per_second(f32::INFINITY);
		assert_eq!(engine.get_maximum_frame_per_second(), 0.0);
	}

	//==============================================================================
	/// 차례를 적는 시험 씬. (내리기와 터치 훅)
	//==============================================================================
	struct RecordingScene {
		scene_base: SceneBase,
		record_list: Rc<RefCell<Vec<&'static str>>>,
	}

	impl Scene for RecordingScene {
		//==============================================================================
		/// 씬 공통 상태 반환.
		//==============================================================================
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		//==============================================================================
		/// 씬 공통 상태 반환. (변경용)
		//==============================================================================
		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}

		//==============================================================================
		/// 종료처리. (finalize 를 적는다)
		//==============================================================================
		fn finalize(&mut self, _engine: &mut Engine) {
			self.record_list.borrow_mut().push("finalize");
		}

		//==============================================================================
		/// 로딩 해제. (unload 를 적는다)
		//==============================================================================
		fn unload(&mut self, _engine: &mut Engine) {
			self.record_list.borrow_mut().push("unload");
		}

		//==============================================================================
		/// 해제. (destroy 를 적는다)
		//==============================================================================
		fn destroy(&mut self, _engine: &mut Engine) {
			self.record_list.borrow_mut().push("destroy");
		}

		//==============================================================================
		/// 터치 누름. (press 를 적는다)
		//==============================================================================
		fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
			self.record_list.borrow_mut().push("press");
		}

		//==============================================================================
		/// 터치 이동. (move 를 적는다)
		//==============================================================================
		fn touch_move(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
			self.record_list.borrow_mut().push("move");
		}

		//==============================================================================
		/// 터치 뗌. (release 를 적는다)
		//==============================================================================
		fn touch_release(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
			self.record_list.borrow_mut().push("release");
		}

		//==============================================================================
		/// 마우스 휠. (wheel 을 적는다)
		//==============================================================================
		fn touch_wheel(&mut self, _engine: &mut Engine, _view_input_position: Vector2, _wheel_delta: Vector2) {
			self.record_list.borrow_mut().push("wheel");
		}
	}

	//==============================================================================
	/// 씬 터치 훅은 원본 차례(누름, 같은 프레임 뗌, 누른 프레임 빼고 누르고 있음, 휠), 내리기는 finalize, unload, destroy.
	//==============================================================================
	#[test]
	fn scene_touch_hooks_and_unload_order() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let record_list: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
		let mut scene = RecordingScene { scene_base: SceneBase::new(), record_list: record_list.clone() };
		let input_manager = engine.get_input_manager_mut();
		input_manager.set_touch_pressed(true);
		input_manager.set_touch_moved(true);
		dispatch_scene_touch(&mut engine, &mut scene);
		engine.get_input_manager_mut().set_touch_pressed(false);
		dispatch_scene_touch(&mut engine, &mut scene);
		let input_manager = engine.get_input_manager_mut();
		input_manager.set_touch_moved(false);
		input_manager.set_touch_released(true);
		input_manager.add_wheel_delta(0.0, 10.0);
		dispatch_scene_touch(&mut engine, &mut scene);
		begin_scene_instance_unload(&mut engine, &mut scene);
		scene.destroy(&mut engine);
		assert_eq!(*record_list.borrow(), vec!["press", "move", "release", "wheel", "finalize", "unload", "destroy"]);
	}

	//==============================================================================
	/// 화면 저장 요청은 지난 번호면 지금 프레임으로, 번호 글은 쉼표로 여럿(같은 번호 한 번, 없으면 120), PNG 로 굽는다.
	//==============================================================================
	#[test]
	fn frame_capture_request_and_frame_numbers() {
		let mut engine = Engine::new(EngineConfiguration::new());
		assert_eq!(engine.get_frame_number(), 1);
		engine.frame_number = 5;
		engine.request_frame_capture(2, Box::new(|_png_bytes: &[u8]| {}));
		engine.request_frame_capture(9, Box::new(|_png_bytes: &[u8]| {}));
		assert!(engine.frame_capture_handler_map.contains_key(&5));
		assert!(engine.frame_capture_handler_map.contains_key(&9));
		assert_eq!(parse_capture_frame_numbers("240, 120,240,x"), vec![120, 240]);
		assert_eq!(parse_capture_frame_numbers(""), vec![120]);
		let png_bytes = encode_png(&[255, 0, 0, 255], 1, 1).expect("png");
		assert_eq!(&png_bytes[1..4], b"PNG");
	}

	//==============================================================================
	/// 시험용 씬. (원본 async load 처럼 create 에서 불러오기 작업을 맡기고, initialize 에서 오래 도는 작업을 맡기고, unload 에서 내리기 작업을 맡긴다)
	//==============================================================================
	struct AsyncLoadScene {
		scene_base: SceneBase,
		load_step_count: u32,
		initialize_count: u32,
		long_task_handle: Rc<RefCell<Option<TaskHandle>>>,
		long_promise: Option<Promise<()>>,
		unload_promise: Option<Promise<()>>,
		destroy_count: Rc<Cell<u32>>,
		seen_load_step_count: Rc<Cell<u32>>,
	}

	impl Scene for AsyncLoadScene {
		//==============================================================================
		/// 씬 공통 상태 반환.
		//==============================================================================
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		//==============================================================================
		/// 씬 공통 상태 반환. (변경용)
		//==============================================================================
		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}

		//==============================================================================
		/// 생성. (불러오기 작업을 맡겨 첫 구간에서 한 단계, 다음 프레임에 한 단계 센다)
		//==============================================================================
		fn create(&mut self, engine: &mut Engine) {
			let load_task_handle = engine.spawn_scene_task(self, |mut scene_context| async move {
				let load_handle = AssetLoadHandle::create_pending("stage.json");
				let task_load_handle = load_handle.clone();
				scene_context.with(move |scene: &mut AsyncLoadScene, _engine| {
					scene.load_step_count += 1;
					task_load_handle.finish(Ok(b"{}".to_vec()));
				});
				load_handle.await.expect("읽힘");
				crate::base::wait::next_frame().await;
				scene_context.with(|scene, _engine| {
					scene.load_step_count += 1;
				});
			});
			self.scene_base.track_task(&load_task_handle);
		}

		//==============================================================================
		/// 초기화. (초기화 수를 세고 오래 도는 작업을 맡긴다)
		//==============================================================================
		fn initialize(&mut self, engine: &mut Engine) {
			self.initialize_count += 1;
			let long_promise = self.long_promise.take().expect("기다릴 Promise");
			let seen_load_step_count = self.seen_load_step_count.clone();
			let long_task_handle = engine.spawn_scene_task(self, move |mut scene_context| async move {
				long_promise.await;
				let load_step_count = scene_context.with(|scene, _engine| scene.load_step_count);
				seen_load_step_count.set(load_step_count);
			});
			*self.long_task_handle.borrow_mut() = Some(long_task_handle);
		}

		//==============================================================================
		/// 로딩 해제. (기다릴 Promise 가 있으면 내리기 작업을 맡긴다)
		//==============================================================================
		fn unload(&mut self, engine: &mut Engine) {
			if let Some(unload_promise) = self.unload_promise.take() {
				let unload_task_handle = engine.spawn_scene_task(self, move |_scene_context| async move {
					unload_promise.await;
				});
				self.scene_base.track_task(&unload_task_handle);
			}
		}

		//==============================================================================
		/// 해제. (destroy 수를 센다)
		//==============================================================================
		fn destroy(&mut self, _engine: &mut Engine) {
			self.destroy_count.set(self.destroy_count.get() + 1);
		}
	}

	//==============================================================================
	/// 시험용 async 씬 만들기. (오래 도는 작업과 내리기 작업이 기다릴 Promise 를 넣는다)
	//==============================================================================
	fn create_async_load_scene(long_promise: Promise<()>, unload_promise: Option<Promise<()>>, destroy_count: &Rc<Cell<u32>>, seen_load_step_count: &Rc<Cell<u32>>, long_task_handle: &Rc<RefCell<Option<TaskHandle>>>) -> Box<dyn Scene> {
		Box::new(AsyncLoadScene {
			scene_base: SceneBase::new(),
			load_step_count: 0,
			initialize_count: 0,
			long_task_handle: long_task_handle.clone(),
			long_promise: Some(long_promise),
			unload_promise,
			destroy_count: destroy_count.clone(),
			seen_load_step_count: seen_load_step_count.clone(),
		})
	}

	//==============================================================================
	/// 씬의 async 불러오기: create 의 작업은 그 자리에서 첫 await 까지 돌고, 끝난 뒤 0.3초에 initialize 한다.
	/// 씬을 내려도 원본처럼 작업은 이어 돌고 씬 값은 작업이 끝날 때까지 산다. (목록에서는 빠지고 with 는 그 씬을 준다) 끝나면 destroy 하고 버린다.
	//==============================================================================
	#[test]
	fn scene_load_task_delays_initialize_and_unloaded_scene_lives_until_tasks_end() {
		let long_task_handle: Rc<RefCell<Option<TaskHandle>>> = Rc::new(RefCell::new(None));
		let destroy_count = Rc::new(Cell::new(0));
		let seen_load_step_count = Rc::new(Cell::new(0));
		let (long_promise, long_resolver) = Promise::create();
		let scene = create_async_load_scene(long_promise, None, &destroy_count, &seen_load_step_count, &long_task_handle);
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), scene);
		engine_runner.apply_pending_scenes();
		let read_scene = |engine_runner: &mut EngineRunner| -> (u32, u32) {
			let scene = engine_runner.scene_list[0].as_scene_any_mut().downcast_mut::<AsyncLoadScene>().expect("시험용 씬");
			(scene.load_step_count, scene.initialize_count)
		};
		assert_eq!(read_scene(&mut engine_runner), (1, 0));
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[0].as_mut(), 10.0);
		assert!(!engine_runner.scene_list[0].get_scene_base().is_loaded());
		engine_runner.poll_tasks();
		assert_eq!(read_scene(&mut engine_runner), (2, 0));
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[0].as_mut(), 10.0);
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[0].as_mut(), 10.2);
		assert!(!engine_runner.scene_list[0].get_scene_base().is_loaded());
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[0].as_mut(), 10.31);
		assert!(engine_runner.scene_list[0].get_scene_base().is_loaded());
		assert_eq!(read_scene(&mut engine_runner), (2, 1));

		let long_task_handle = long_task_handle.borrow().clone().expect("오래 도는 작업");
		assert!(!long_task_handle.is_finished());
		engine_runner.engine.unload_all_scenes();
		engine_runner.apply_pending_scenes();
		assert!(engine_runner.scene_list.is_empty());
		assert!(engine_runner.engine.get_all_loaded_scenes().is_empty());
		assert_eq!(engine_runner.detached_scene_list.len(), 1);
		assert_eq!(destroy_count.get(), 0);
		assert!(!long_task_handle.is_finished());
		long_resolver.resolve(());
		engine_runner.poll_tasks();
		assert!(long_task_handle.is_finished());
		assert_eq!(seen_load_step_count.get(), 2);
		assert!(engine_runner.detached_scene_list.is_empty());
		assert_eq!(destroy_count.get(), 1);
		assert_eq!(engine_runner.engine.task_executor.get_task_count(), 0);
	}

	//==============================================================================
	/// 불러오는 도중에 내린 씬: 원본 SceneManager.loadScene 처럼 불러오기 작업이 끝나고 0.3초 뒤에 initialize 까지 받고,
	/// 그동안과 그 뒤의 작업이 끝날 때까지 살아 있다가 destroy 된다.
	//==============================================================================
	#[test]
	fn scene_unloaded_while_loading_still_initializes() {
		let long_task_handle: Rc<RefCell<Option<TaskHandle>>> = Rc::new(RefCell::new(None));
		let destroy_count = Rc::new(Cell::new(0));
		let seen_load_step_count = Rc::new(Cell::new(0));
		let (long_promise, long_resolver) = Promise::create();
		let scene = create_async_load_scene(long_promise, None, &destroy_count, &seen_load_step_count, &long_task_handle);
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), scene);
		engine_runner.apply_pending_scenes();
		engine_runner.engine.unload_all_scenes();
		engine_runner.apply_pending_scenes();
		assert!(engine_runner.scene_list.is_empty());
		assert_eq!(engine_runner.detached_scene_list.len(), 1);
		engine_runner.poll_tasks();
		assert_eq!(engine_runner.detached_scene_list.len(), 1);
		std::thread::sleep(std::time::Duration::from_millis(320));
		engine_runner.poll_tasks();
		let read_initialize_count = |engine_runner: &mut EngineRunner| -> u32 {
			engine_runner.detached_scene_list[0].as_scene_any_mut().downcast_mut::<AsyncLoadScene>().expect("시험용 씬").initialize_count
		};
		assert_eq!(read_initialize_count(&mut engine_runner), 1);
		assert_eq!(destroy_count.get(), 0);
		long_resolver.resolve(());
		engine_runner.poll_tasks();
		assert_eq!(seen_load_step_count.get(), 2);
		assert!(engine_runner.detached_scene_list.is_empty());
		assert_eq!(destroy_count.get(), 1);
	}

	//==============================================================================
	/// 원본 await unloadScene: unload 가 맡긴 작업이 끝날 때까지 씬은 목록에 남아(원본처럼 tick 과 draw 를 받는다) Promise 가 풀리지 않고,
	/// 끝나면 목록에서 빠지고 풀린다.
	//==============================================================================
	#[test]
	fn unload_scene_async_waits_for_unload_task() {
		let long_task_handle: Rc<RefCell<Option<TaskHandle>>> = Rc::new(RefCell::new(None));
		let destroy_count = Rc::new(Cell::new(0));
		let seen_load_step_count = Rc::new(Cell::new(0));
		let (long_promise, _long_resolver) = Promise::create();
		let (unload_promise, unload_resolver) = Promise::create();
		let scene = create_async_load_scene(long_promise, Some(unload_promise), &destroy_count, &seen_load_step_count, &long_task_handle);
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), scene);
		engine_runner.apply_pending_scenes();
		let scene_id = engine_runner.scene_list[0].get_scene_base().get_scene_id().expect("핸들");
		let record_list: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
		let task_record_list = record_list.clone();
		engine_runner.engine.spawn_task(move |mut engine_context| async move {
			engine_context.with(|engine| engine.unload_scene_async(scene_id)).await;
			task_record_list.borrow_mut().push("unloaded");
		});
		engine_runner.apply_pending_scenes();
		assert_eq!(engine_runner.scene_list.len(), 1);
		assert_eq!(engine_runner.engine.get_all_loaded_scenes().len(), 1);
		engine_runner.poll_tasks();
		engine_runner.poll_tasks();
		assert!(record_list.borrow().is_empty());
		assert_eq!(engine_runner.scene_list.len(), 1);
		unload_resolver.resolve(());
		engine_runner.poll_tasks();
		assert!(engine_runner.scene_list.is_empty());
		assert!(engine_runner.engine.get_all_loaded_scenes().is_empty());
		engine_runner.poll_tasks();
		assert_eq!(*record_list.borrow(), vec!["unloaded"]);
		// 불러오기를 마치기 전에 내린 씬이라 원본 loadScene 처럼 initialize 를 받을 때까지 살아 있다.
		assert_eq!(engine_runner.detached_scene_list.len(), 1);
		assert_eq!(destroy_count.get(), 0);
	}

	//==============================================================================
	/// 원본 await loadScene, await unloadScene: 엔진 작업에서 initialize 까지 끝나면 핸들로, 내린 뒤에 풀린다.
	//==============================================================================
	#[test]
	fn load_scene_async_and_unload_scene_async_resolve() {
		let counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), CountingScene::create_boxed(&counts[0], &counts[1], &counts[2]));
		engine_runner.apply_pending_scenes();
		let second_counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
		let second_scene = CountingScene::create_boxed(&second_counts[0], &second_counts[1], &second_counts[2]);
		let record_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let task_record_list = record_list.clone();
		let task_handle = engine_runner.engine.spawn_task(move |mut engine_context| async move {
			let scene_id = engine_context.with(|engine| engine.load_scene_async(second_scene)).await;
			task_record_list.borrow_mut().push(String::from("loaded"));
			engine_context.with(|engine| engine.unload_scene_async(scene_id)).await;
			let loaded_count = engine_context.with(|engine| engine.get_all_loaded_scenes().len());
			task_record_list.borrow_mut().push(format!("unloaded {}", loaded_count));
		});
		engine_runner.apply_pending_scenes();
		assert_eq!(second_counts[0].get(), 1);
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[1].as_mut(), 0.0);
		engine_runner.poll_tasks();
		assert!(record_list.borrow().is_empty());
		initialize_scene_if_ready(&mut engine_runner.engine, engine_runner.scene_list[1].as_mut(), 0.31);
		engine_runner.poll_tasks();
		assert_eq!(*record_list.borrow(), vec![String::from("loaded")]);
		engine_runner.apply_pending_scenes();
		assert_eq!(second_counts[1].get(), 1);
		engine_runner.poll_tasks();
		assert_eq!(*record_list.borrow(), vec![String::from("loaded"), String::from("unloaded 1")]);
		assert!(task_handle.is_finished());
	}

	//==============================================================================
	/// 원본 await requestFrameCapture(번호): 그 프레임의 PNG 바이트로 풀린다.
	//==============================================================================
	#[test]
	fn request_frame_capture_async_resolves_with_png_bytes() {
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), Box::new(RecordingScene { scene_base: SceneBase::new(), record_list: Rc::new(RefCell::new(Vec::new())) }));
		let mut capture_promise = engine_runner.engine.request_frame_capture_async(1);
		let handler_list = engine_runner.engine.frame_capture_handler_map.remove(&1).expect("요청");
		for capture_handler in handler_list {
			capture_handler(&[1, 2, 3]);
		}
		let waker = std::task::Waker::noop();
		let mut context = std::task::Context::from_waker(waker);
		assert_eq!(std::pin::Pin::new(&mut capture_promise).poll(&mut context), std::task::Poll::Ready(vec![1, 2, 3]));
	}

	//==============================================================================
	/// "data:" 주소 애셋. (데스크탑은 주소를 풀어 바이트로, 소리는 형을 보지 않고 내용으로 가린다. 원본 fetch, decodeAudioData 와 같다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	#[test]
	fn data_url_assets_load_on_desktop() {
		//==============================================================================
		/// 바이트를 base64 글로. (시험용 data: 주소를 만든다, 끝은 = 로 채운다)
		//==============================================================================
		fn encode_base64(bytes: &[u8]) -> String {
			const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
			let mut text = String::new();
			for chunk in bytes.chunks(3) {
				let value = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
				for index in 0..4 {
					if index <= chunk.len() {
						text.push(ALPHABET[(value >> (18 - 6 * index) & 63) as usize] as char);
					}
					else {
						text.push('=');
					}
				}
			}
			text
		}
		let engine = Engine::new(EngineConfiguration::new());
		// 타임라인 편집기가 넣는 꼴. (FileReader.readAsDataURL, 형은 파일 확장자에서 오고 틀려도 된다)
		let webm_bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/audio/opus_stereo.webm")).expect("webm");
		let audio_asset = engine.load_audio_asset(&format!("data:application/octet-stream;base64,{}", encode_base64(&webm_bytes))).expect("data audio");
		assert!(audio_asset.is_loaded());
		assert!((audio_asset.get_duration() - 11002.0 / 44100.0).abs() < 1e-9);
		assert!(engine.load_audio_asset("data:audio/wav;base64,@@").is_err());
		// 애셋 바이트 부탁은 그 자리에서 끝나고 형을 응답 형으로 둔다.
		let load_handle = engine.request_asset_bytes("data:text/plain;charset=utf-8,hello%20world");
		assert!(load_handle.is_loaded());
		assert_eq!(load_handle.get_bytes().expect("bytes").as_slice(), b"hello world");
		assert_eq!(load_handle.get_content_type().as_deref(), Some("text/plain;charset=utf-8"));
		assert!(engine.request_asset_bytes("data:nocomma").is_failed());
		// 그림.
		let png_bytes = encode_png(&[0, 255, 0, 255], 1, 1).expect("png");
		let image = engine.load_image(&format!("data:image/png;base64,{}", encode_base64(&png_bytes))).expect("data image");
		assert_eq!((image.get_width(), image.get_height()), (1, 1));
	}
}
