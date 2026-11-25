//==============================================================================
// 엔진.
// - 원본은 캔버스를 잡고 request_animation_frame 으로 프레임을 돈다.
//   러스트 판은 창, GL 컨텍스트, 입력 이벤트, 화면 넘기기를 플랫폼 층(crate::platform)이 맡고,
//   여기 EngineRunner 는 플랫폼을 모른 채 씬과 프레임 흐름만 돈다. (데스크탑: platform/desktop.rs, 웹: platform/web.rs)
// - 씬에는 &mut Engine 을 넘긴다. 씬은 여기서 매니저(시간, 뷰, 입력)를 꺼내 쓴다. (원본 Engine.get_engine() 자리)
// - Graphic 은 플랫폼이 GL 컨텍스트를 만든 뒤 attach_graphic 으로 넘겨받고, 씬의 draw 계열에 따로 넘긴다.
// - 오디오 매니저를 원본처럼 생성할 때 만들어 든다. AudioContext(소리 장치)는 처음 쓸 때 만든다. 프레임마다 하는 오디오 일은 없다.
//   웹은 플랫폼이 click / touchstart / keydown 에서 resume_on_user_gesture, focus / visibilitychange 에서 resume 을 부른다.
// - 씬 하나만 내리기(unload_scene)는 원본 SceneManager.unloadScene 자리다. 원본은 씬 객체로, 러스트 판은 load_scene 이 돌려준 SceneId 로 가리키고,
//   unload_all_scenes 처럼 이번 프레임이 끝난 뒤 내린다. 아직 create 전인(같은 프레임에 불러온) 씬이면 create 도 destroy 도 부르지 않고 뺀다.
// - 페이지 이벤트(PageEvent): 원본은 모듈마다 document / window 에 리스너를 단다. 러스트 판은 플랫폼이 엔진에 알리고
//   엔진이 add_page_event_listener 로 받은 리스너에 그 자리에서 넘긴다. (SoundSystem 이 가려짐과 포커스를 이것으로 받는다)
//   웹: visibilitychange(Hidden, Visible), blur(Blurred), focus(Focused), pointerdown(PointerDown).
//   데스크탑: 창 가림(Occluded, 맥과 X11 만 winit 이 알린다, 윈도우와 웨이랜드는 오지 않는다), 포커스, 마우스 단추와 손가락 누름.
// - 페이지 복원 훅: 원본 setupAllDocumentEvents 처럼 소리를 다시 깨우고 입력을 비운 뒤 씬마다 부른다.
//   웹: visibilitychange 의 visible → on_page_visibility_restored, 문서 resume 과 pageshow(persisted) → on_page_process_restored.
//   원본은 pageshow 에서 정의되지 않은 dispatchPageProcessRestored 를 불러 예외가 났다. 러스트 판은 resume 과 같은 복구를 한다.
//   데스크탑: 창 가림이 풀리면 on_page_visibility_restored, 앱이 suspended 뒤 resumed 되면 on_page_process_restored.
//   (윈도우, 맥, 리눅스의 winit 은 suspended 를 보내지 않아 데스크탑에서는 on_page_process_restored 가 불리지 않는다)
//
// 매 프레임 (update_engine):
// 1. graphic.apply_settings()      # 뷰포트/프로젝션 갱신 + 상태 리셋 + 화면 비우기
// 2. time_manager.update()         # time_delta 산출
// 3. input_manager.tick()
// 4. 씬마다 tick → pre_draw → draw → post_draw (→ draw_gizmos)
// 5. 통계 출력(옵션) → 1회성 입력 플래그 클리어(글 입력 줄 마무리 포함) → 묶음 내보내기 → (검증용 화면 저장)
// 화면 넘기기는 플랫폼이 update_engine 다음에 한다.
//==============================================================================
use crate::base::color::Color;
use crate::core::textshaping::FontFaceDescriptors;
use crate::base::platform::Platform;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::base::version::Version;
use crate::core::asset::AssetLoadHandle;
use crate::core::audiomanager::AudioManager;
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::image::Image;
use crate::core::inputmanager::InputManager;
use crate::core::pointercursor::PointerCursor;
use crate::core::scene::{Scene, SceneId};
use crate::core::timemanager::TimeManager;
use crate::core::viewmanager::{ViewManager, ViewScaleMode};
use crate::platform::{self, PlatformWindow};
use crate::resource::audioasset::AudioAsset;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use web_time::Instant;

// 엔진 판. (원본 vanilla.js 0.3.2 를 옮긴 첫 판)
const ENGINE_VERSION: Version = Version::create(0, 3, 2);

//==============================================================================
// 엔진 설정.
//==============================================================================
pub struct EngineConfiguration {
	// 기준 해상도. (뷰 스케일 모드가 none 이 아니면 씬은 이 좌표계로 그린다)
	pub reference_resolution_size: Vector2,
	// 뷰 스케일 모드.
	pub view_scale_mode: ViewScaleMode,
	// 창 제목.
	pub title: String,
	// 처음 창 크기. (논리 크기, 0 이면 기준 해상도)
	pub window_size: Vector2,
	// 창 크기를 바꿀 수 있는지.
	pub is_resizable: bool,
	// 화면 왼쪽 위에 FPS / 드로우 콜 출력.
	pub use_statistics: bool,
	// 개발자 도구(F2 하이어라키/인스펙터)를 GameSceneBase 가 만드는지. (원본 useDevTools, 기본 true)
	pub use_dev_tools: bool,
	// 최대 FPS. 0 이면 제한하지 않는다. (수직 동기를 켜면 화면 주사율을 따른다)
	pub maximum_frame_per_second: f32,
	// 수직 동기.
	pub use_vertical_sync: bool,
	// 기본 폰트 후보 경로. (앞에서부터 읽히는 첫 파일. 원본 default_font_url 자리)
	pub default_font_paths: Vec<PathBuf>,
	// 굵은 기본 폰트 후보 경로.
	pub default_bold_font_paths: Vec<PathBuf>,
	// 대체 글꼴 후보 경로. (원본은 브라우저가 글꼴에 없는 글자를 시스템 글꼴로 그린다. 데스크탑은 등록된 어느 글꼴에도 없는 글자가 나오면 앞에서부터 읽어 대체 폰트로 쓴다)
	// - 색 이모지 글꼴(윈도우 Segoe UI Emoji, 맥 Apple Color Emoji, 리눅스 Noto Color Emoji)은 이모지 표시 글자가 나올 때 먼저 읽고, 다른 글자 때문에는 마지막에 읽는다.
	pub fallback_font_paths: Vec<PathBuf>,
	// 애셋 폴더 후보. (resolve_asset_path 가 앞에서부터 찾는다)
	pub asset_directory_paths: Vec<PathBuf>,
	// 창 아이콘 PNG. (애셋 폴더 기준 상대 경로, 비우면 기본 아이콘)
	pub window_icon_path: String,
	// 기본 폰트 파일 내용. (있으면 경로 후보보다 먼저 쓴다. 파일을 못 읽는 웹에서 미리 받아 넘긴다)
	pub default_font_bytes: Option<Vec<u8>>,
	// 굵은 기본 폰트 파일 내용.
	pub default_bold_font_bytes: Option<Vec<u8>>,
	// 웹에서 그릴 캔버스 id. (없으면 만들어 body 에 붙인다)
	pub canvas_id: String,
	// 웹 기본 폰트 주소. (원본 default_font_url. 웹에서 default_font_bytes 가 없으면 여기서 받아 기본 폰트로 쓴다. 빈 글이면 받지 않는다)
	// - 기본값은 빈 글이다. (원본 기본값 DOSGothic 은 넣지 않는다) 아무 글꼴도 주지 않으면 원본처럼 브라우저 시스템 글꼴로 그린다.
	// - 게임 글꼴을 쓰려면 이 주소나 default_font_bytes, FontAsset 으로 준다.
	// - 데스크탑은 쓰지 않고 default_font_paths 를 본다.
	pub default_font_url: String,
	// 웹에서 touchend 와 pointerup 도 user gesture 로 보고 오디오를 재개한다. (원본과 같이 기본은 끔)
	pub resume_audio_on_touch_end: bool,
	// 웹에서 애셋 주소 앞에 붙일 것. (비우면 페이지 기준 상대 주소. 데스크탑은 asset_directory_paths 를 쓴다)
	pub asset_base_url: String,
	// 창 크기가 바뀌면 캔버스를 창에 맞춘다. (웹, 끄면 페이지가 정한 캔버스 크기를 읽는다. 원본처럼 기본은 끔,
	// 데스크탑은 창이 곧 캔버스라 늘 맞춘다)
	pub auto_resize_on_window_resize: bool,
	// 프레임이 끝난 뒤에도 그린 것을 캔버스에 남긴다. (웹 WebGL 컨텍스트 옵션, 캔버스 위에 후처리를 얹는 페이지가 읽는다, 데스크탑은 쓰지 않는다)
	pub preserve_drawing_buffer: bool,
	// 화면 키보드: 글자 입력칸이 포커스된 동안 가로가 같고 세로만 줄었으면 크기를 그대로 둔다. (웹, 데스크탑은 화면 키보드가 없다)
	pub ignore_virtual_keyboard_resize: bool,
	// 캔버스의 가로세로 비 상한. (창이 그보다 길쭉하면 캔버스를 줄여 창의 가운데에 둔다, 0 이면 제한 없음)
	pub max_canvas_aspect_ratio: f32,
}

impl EngineConfiguration {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> EngineConfiguration {
		EngineConfiguration {
			reference_resolution_size: Vector2::create(1280.0, 720.0),
			view_scale_mode: ViewScaleMode::StretchShort,
			title: String::from("바닐라엔진"),
			window_size: Vector2::zero(),
			is_resizable: true,
			use_statistics: false,
			use_dev_tools: true,
			maximum_frame_per_second: 0.0,
			use_vertical_sync: true,
			default_font_paths: vec![
				PathBuf::from("C:/Windows/Fonts/malgun.ttf"),
				PathBuf::from("C:/Windows/Fonts/segoeui.ttf"),
				PathBuf::from("C:/Windows/Fonts/arial.ttf"),
				PathBuf::from("/System/Library/Fonts/AppleSDGothicNeo.ttc"),
				PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
			],
			default_bold_font_paths: vec![
				PathBuf::from("C:/Windows/Fonts/malgunbd.ttf"),
				PathBuf::from("C:/Windows/Fonts/segoeuib.ttf"),
				PathBuf::from("C:/Windows/Fonts/arialbd.ttf"),
			],
			fallback_font_paths: vec![
				PathBuf::from("C:/Windows/Fonts/msyh.ttc"),
				PathBuf::from("C:/Windows/Fonts/meiryo.ttc"),
				PathBuf::from("C:/Windows/Fonts/arial.ttf"),
				PathBuf::from("C:/Windows/Fonts/LeelawUI.ttf"),
				PathBuf::from("C:/Windows/Fonts/seguisym.ttf"),
				PathBuf::from("C:/Windows/Fonts/seguiemj.ttf"),
				PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
				PathBuf::from("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"),
				PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
				PathBuf::from("/System/Library/Fonts/Apple Color Emoji.ttc"),
				PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
				PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
				PathBuf::from("/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf"),
			],
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
		}
	}
}

impl Default for EngineConfiguration {
	fn default() -> EngineConfiguration {
		EngineConfiguration::new()
	}
}

//==============================================================================
// 페이지 이벤트. (플랫폼이 엔진에 알리고, 엔진이 리스너에 그 자리에서 넘긴다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageEvent {
	// 페이지가 가려짐. (다른 탭, 앱 전환, 창이 가려짐)
	Hidden,
	// 페이지가 다시 보임.
	Visible,
	// 창이 포커스를 잃음. (blur)
	Blurred,
	// 창이 포커스를 얻음. (focus)
	Focused,
	// 창을 누름. (pointerdown, 포커스를 알리지 않아도 누르면 창은 포커스를 가진 것이다)
	PointerDown,
}

// 페이지 이벤트 리스너. (엔진은 약한 참조로 들어, 리스너를 버리면 더 부르지 않는다)
pub type PageEventListener = Rc<RefCell<dyn FnMut(PageEvent)>>;

// 검증용 화면 저장을 받는 함수. (그 프레임을 다 그린 화면의 PNG 바이트, 원본은 PNG data URL)
pub type FrameCaptureHandler = Box<dyn FnOnce(&[u8])>;

//==============================================================================
// 프레임 시작에 할 글꼴 일. (Graphic 이 있어야 한다)
//==============================================================================
enum PendingFontOperation {
	// 이름, 얼굴 열쇠.
	Unregister(String, String),
	// 이름, 얼굴 열쇠의 차례.
	Order(String, Vec<String>),
	ClearTextCache,
}

//==============================================================================
// 엔진.
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
	// 프레임 번호마다 화면 저장을 받을 함수. (request_frame_capture)
	frame_capture_handler_map: std::collections::HashMap<u64, Vec<FrameCaptureHandler>>,
	// 다음 프레임 첫머리에 캔버스와 뷰 영역을 다시 맞춰 달라고 했는지. (resize)
	pub(crate) is_resize_requested: bool,
	pending_scene_list: Vec<Box<dyn Scene>>,
	is_unload_all_scenes_requested: bool,
	pending_font_registration_list: Vec<(String, FontFaceDescriptors, AssetLoadHandle)>,
	// 글꼴 얼굴 지우기, 차례 정하기, 구운 글자 비우기 부탁. (Graphic 이 있는 프레임 시작에 한다)
	pending_font_operation_list: Vec<PendingFontOperation>,
	pending_default_font_handle: Option<AssetLoadHandle>,
	platform: Platform,
	next_scene_number: u64,
	pending_unload_scene_id_list: Vec<SceneId>,
	is_page_hidden: bool,
	page_event_listener_list: RefCell<Vec<Weak<RefCell<dyn FnMut(PageEvent)>>>>,
}

impl Engine {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(engine_configuration: EngineConfiguration) -> Engine {
		let reference_resolution_size = engine_configuration.reference_resolution_size;
		let view_scale_mode = engine_configuration.view_scale_mode;
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
			is_page_hidden: false,
			page_event_listener_list: RefCell::new(Vec::new()),
		}
	}

	//==============================================================================
	// 시작.
	// - 데스크탑: 창을 닫거나 exit_application 을 부를 때까지 돌아오지 않는다.
	// - 웹: 캔버스와 WebGL2 를 잡고 request_animation_frame 을 걸어 둔 뒤 바로 돌아온다.
	//==============================================================================
	pub fn run(self, scene: Box<dyn Scene>) -> Result<(), String> {
		platform::run(EngineRunner::new(self, scene))
	}

	//==============================================================================
	// 씬 불러오기. (이번 프레임이 끝난 뒤 목록에 더한다. unload_scene 에 넘길 핸들을 돌려준다)
	//==============================================================================
	pub fn load_scene(&mut self, mut scene: Box<dyn Scene>) -> SceneId {
		self.next_scene_number += 1;
		let scene_id = SceneId(self.next_scene_number);
		scene.get_scene_base_mut().set_scene_id(scene_id);
		self.pending_scene_list.push(scene);
		scene_id
	}

	//==============================================================================
	// 씬 하나 내리기. (이번 프레임이 끝난 뒤 destroy 를 부르고 목록에서 뺀다. 없는 핸들이면 아무것도 하지 않는다)
	// - 핸들(SceneId)이나 원본처럼 씬(씬 공통 상태 &SceneBase)을 받는다.
	//==============================================================================
	pub fn unload_scene(&mut self, scene: impl Into<SceneId>) {
		self.pending_unload_scene_id_list.push(scene.into());
	}

	//==============================================================================
	// 모든 씬 내리기. (이번 프레임이 끝난 뒤, load_scene 과 같이 부르면 내린 다음 불러온다)
	//==============================================================================
	pub fn unload_all_scenes(&mut self) {
		self.is_unload_all_scenes_requested = true;
	}

	//==============================================================================
	// 애셋 경로 찾기. (설정의 애셋 폴더 후보에서 처음 있는 것, 없으면 첫 후보 기준)
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
	// 이미지 애셋 읽기. (원본 new ImageAsset() + load(path))
	//==============================================================================
	pub fn load_image(&self, relative_path: &str) -> Result<Rc<Image>, String> {
		let image_path = self.resolve_asset_path(relative_path);
		Image::load(&image_path)
	}

	//==============================================================================
	// 애셋 바이트 부탁. (애셋들의 load 가 부른다)
	// - 데스크탑: 애셋 폴더 후보에서 파일을 찾아 그 자리에서 읽고 끝낸다.
	// - 웹: asset_base_url + 상대 경로를 fetch 하고, 받으면 끝낸다. (그 사이 프레임은 계속 돈다)
	//==============================================================================
	pub fn request_asset_bytes(&self, relative_path: &str) -> AssetLoadHandle {
		self.request_asset_bytes_with_cache(relative_path, false)
	}

	//==============================================================================
	// 애셋 바이트 부탁. (is_no_cache 면 웹은 브라우저 캐시를 쓰지 않고 다시 확인한다, 원본 TextAsset.setNoCache)
	//==============================================================================
	pub fn request_asset_bytes_with_cache(&self, relative_path: &str, is_no_cache: bool) -> AssetLoadHandle {
		let load_handle = AssetLoadHandle::create_pending(relative_path);
		#[cfg(not(target_arch = "wasm32"))]
		let asset_location = self.resolve_asset_path(relative_path).to_string_lossy().into_owned();
		#[cfg(target_arch = "wasm32")]
		let asset_location = format!("{}{}", self.engine_configuration.asset_base_url, relative_path);
		platform::request_asset_bytes(asset_location, load_handle.clone(), is_no_cache);
		load_handle
	}

	//==============================================================================
	// 애셋 바이트 부탁. (이미 만든 읽기 손잡이를 끝낸다. AssetLoader 가 미리 나눠 준 손잡이를 차례가 오면 채울 때 쓴다)
	//==============================================================================
	pub fn request_asset_bytes_into(&self, relative_path: &str, load_handle: &AssetLoadHandle) {
		#[cfg(not(target_arch = "wasm32"))]
		let asset_location = self.resolve_asset_path(relative_path).to_string_lossy().into_owned();
		#[cfg(target_arch = "wasm32")]
		let asset_location = format!("{}{}", self.engine_configuration.asset_base_url, relative_path);
		platform::request_asset_bytes(asset_location, load_handle.clone(), false);
	}

	//==============================================================================
	// 글꼴 바이트가 오면 이름으로 등록해 달라고 맡기기. (FontAsset 이 부른다. 등록은 프레임 시작에 한다)
	//==============================================================================
	pub fn register_font_when_loaded(&mut self, family: &str, is_bold: bool, load_handle: AssetLoadHandle) {
		self.register_font_face_when_loaded(family, FontFaceDescriptors::create_with_bold(is_bold), load_handle);
	}

	//==============================================================================
	// 글꼴 얼굴 바이트가 오면 서술자(굵기, 기울기, 글자 범위)대로 이름 아래에 올려 달라고 맡기기. (원본 new FontFace(family, url, descriptors) + document.fonts.add)
	// - 서술자의 source_key 가 비었으면 읽기 손잡이의 경로를 쓴다.
	//==============================================================================
	pub fn register_font_face_when_loaded(&mut self, family: &str, mut font_face_descriptors: FontFaceDescriptors, load_handle: AssetLoadHandle) {
		if font_face_descriptors.source_key.is_empty() {
			font_face_descriptors.source_key = load_handle.get_asset_path().to_string();
		}
		self.pending_font_registration_list.push((family.to_string(), font_face_descriptors, load_handle));
	}

	//==============================================================================
	// 글꼴 얼굴 지우기 맡기기. (원본 document.fonts.delete, 열쇠는 등록할 때의 source_key 또는 경로)
	//==============================================================================
	pub fn unregister_font_face(&mut self, family: &str, source_key: &str) {
		self.pending_font_registration_list.retain(|(pending_family, font_face_descriptors, _)| !(pending_family.eq_ignore_ascii_case(family) && font_face_descriptors.source_key == source_key));
		self.pending_font_operation_list.push(PendingFontOperation::Unregister(family.to_string(), source_key.to_string()));
	}

	//==============================================================================
	// 한 이름의 얼굴 차례 정하기 맡기기. (얼굴 열쇠 목록, 뒤가 먼저 그린다, TextStringTextureCache.set_font_face_order)
	//==============================================================================
	pub fn set_font_face_order(&mut self, family: &str, source_key_list: Vec<String>) {
		self.pending_font_operation_list.push(PendingFontOperation::Order(family.to_string(), source_key_list));
	}

	//==============================================================================
	// 구운 글자 모두 버리기 맡기기. (원본 textStringTextureCache.clear, 다음 프레임 시작에)
	//==============================================================================
	pub fn clear_text_cache(&mut self) {
		self.pending_font_operation_list.push(PendingFontOperation::ClearTextCache);
	}

	//==============================================================================
	// 아직 바이트가 오지 않은 글꼴 얼굴 수. (UI 글꼴이 기다릴 때 본다)
	//==============================================================================
	pub fn get_pending_font_face_count(&self) -> usize {
		self.pending_font_registration_list.len()
	}

	//==============================================================================
	// 오디오 애셋 읽기. (원본 new AudioAsset() + load(path), 애셋 폴더 기준 경로)
	// - 데스크탑: 그 자리에서 읽고 디코딩한다. 실패하면 Err.
	// - 웹: fetch 와 디코딩을 걸어 두고 바로 애셋을 돌려준다. 끝나면 is_loaded() 가 참이 된다. (실패는 get_load_error())
	//==============================================================================
	pub fn load_audio_asset(&self, relative_path: &str) -> Result<Rc<AudioAsset>, String> {
		#[cfg(not(target_arch = "wasm32"))]
		let audio_asset_path = self.resolve_asset_path(relative_path);
		#[cfg(target_arch = "wasm32")]
		let audio_asset_path = PathBuf::from(format!("{}{}", self.engine_configuration.asset_base_url, relative_path));
		let audio_asset = Rc::new(AudioAsset::new());
		audio_asset.load(&audio_asset_path)?;
		Ok(audio_asset)
	}

	//==============================================================================
	// 오디오 컨텍스트 재개. (focus, visibilitychange 같은 user gesture 가 아닌 복귀. gesture 전이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn resume(&self) {
		let audio_manager = self.get_audio_manager();
		audio_manager.resume_context();
	}

	//==============================================================================
	// user gesture (click / touch / keydown) 핸들러.
	// AudioManager 에 user gesture 발생을 알리고 컨텍스트 재개를 시도한다.
	//==============================================================================
	pub fn resume_on_user_gesture(&self) {
		let audio_manager = self.get_audio_manager();
		audio_manager.mark_user_gesture();
		audio_manager.resume_context();
	}

	//==============================================================================
	// 종료 요청.
	//==============================================================================
	pub fn exit_application(&mut self) {
		self.is_exit_requested = true;
	}

	//==============================================================================
	// 종료 요청 여부.
	//==============================================================================
	pub fn is_exit_requested(&self) -> bool {
		self.is_exit_requested
	}

	//==============================================================================
	// 커서 보이기 설정.
	//==============================================================================
	pub fn set_visible_cursor(&self, is_visible: bool) {
		platform::set_cursor_visible(self.platform_window.as_ref(), is_visible);
	}

	//==============================================================================
	// 해상도 다시 맞추기. (원본 resize, 설정을 바꾼 뒤 부르면 플랫폼이 다음 프레임 첫머리에 캔버스와 뷰 영역을 다시 셈하고 씬마다 resize 를 부른다)
	//==============================================================================
	pub fn resize(&mut self) {
		self.is_resize_requested = true;
	}

	//==============================================================================
	// 캔버스. (원본 getCanvas, 웹은 그리는 캔버스 요소, 데스크탑은 창, 플랫폼이 붙이기 전에는 None)
	//==============================================================================
	pub fn get_canvas(&self) -> Option<&PlatformWindow> {
		self.platform_window.as_ref()
	}

	//==============================================================================
	// 글자 입력칸이 포커스되어 있는지. (원본 isTextInputFocused, 웹은 문서의 초점이 글자 input, textarea, contenteditable 이면 참, 데스크탑은 문서가 없어 거짓)
	//==============================================================================
	pub fn is_text_input_focused(&self) -> bool {
		platform::is_text_field_focused()
	}

	//==============================================================================
	// 창 제목 설정.
	//==============================================================================
	pub fn set_title(&self, title: &str) {
		platform::set_title(self.platform_window.as_ref(), title);
	}

	//==============================================================================
	// 시스템 클립보드에 글 쓰기. (데스크탑 arboard, 웹 navigator.clipboard.writeText. 원본 UIInputField.writeClipboard 자리)
	//==============================================================================
	pub fn write_clipboard_text(&self, text: &str) {
		platform::write_clipboard_text(text);
	}

	//==============================================================================
	// 시스템 클립보드 글 읽기. (데스크탑만 그 자리에서 읽는다. 웹은 비동기라 None, 웹 붙여넣기는 플랫폼이 읽어 글 입력 줄에 넣는다)
	//==============================================================================
	pub fn read_clipboard_text(&self) -> Option<String> {
		platform::read_clipboard_text()
	}

	pub fn get_engine_configuration(&self) -> &EngineConfiguration {
		&self.engine_configuration
	}

	//==============================================================================
	// 최대 FPS 설정. (실행 중에 바꿀 수 있다, 0 이면 제한하지 않는다. 0 보다 작거나 유한하지 않으면 0)
	// - EngineConfiguration.maximum_frame_per_second 에 적는다. 플랫폼이 프레임마다 그 값을 읽어 다음 프레임부터 따른다.
	//==============================================================================
	pub fn set_maximum_frame_per_second(&mut self, maximum_frame_per_second: f32) {
		if !maximum_frame_per_second.is_finite() || maximum_frame_per_second < 0.0 {
			self.engine_configuration.maximum_frame_per_second = 0.0;
			return;
		}
		self.engine_configuration.maximum_frame_per_second = maximum_frame_per_second;
	}

	//==============================================================================
	// 최대 FPS 반환. (0 이면 제한하지 않음)
	//==============================================================================
	pub fn get_maximum_frame_per_second(&self) -> f32 {
		self.engine_configuration.maximum_frame_per_second
	}

	pub fn get_time_manager(&self) -> &TimeManager {
		&self.time_manager
	}

	pub fn get_time_manager_mut(&mut self) -> &mut TimeManager {
		&mut self.time_manager
	}

	pub fn get_view_manager(&self) -> &ViewManager {
		&self.view_manager
	}

	pub fn get_view_manager_mut(&mut self) -> &mut ViewManager {
		&mut self.view_manager
	}

	pub fn get_input_manager(&self) -> &InputManager {
		&self.input_manager
	}

	pub fn get_input_manager_mut(&mut self) -> &mut InputManager {
		&mut self.input_manager
	}

	//==============================================================================
	// 마우스 커서. (켜면 프레임 끝마다 창이나 캔버스의 커서를 정한다)
	//==============================================================================
	pub fn get_pointer_cursor(&self) -> &PointerCursor {
		&self.pointer_cursor
	}

	pub fn get_pointer_cursor_mut(&mut self) -> &mut PointerCursor {
		&mut self.pointer_cursor
	}

	//==============================================================================
	// 주된 손이 손가락인지. (CSS 의 pointer: coarse, 데스크탑은 false)
	//==============================================================================
	pub fn is_coarse_pointer(&self) -> bool {
		platform::is_coarse_pointer()
	}

	//==============================================================================
	// 오디오 매니저. (메서드가 &self 라 컴포넌트가 받는 &Engine 으로도 플레이어를 만들 수 있다)
	//==============================================================================
	pub fn get_audio_manager(&self) -> &AudioManager {
		&self.audio_manager
	}

	//==============================================================================
	// 오디오 매니저. (가변)
	//==============================================================================
	pub fn get_audio_manager_mut(&mut self) -> &mut AudioManager {
		&mut self.audio_manager
	}

	pub fn get_platform(&self) -> &Platform {
		&self.platform
	}

	//==============================================================================
	// 페이지가 가려져 있는지. (다른 탭, 앱 전환, 창이 가려짐. 데스크탑은 winit 이 가림을 알리는 맥과 X11 만)
	//==============================================================================
	pub fn is_page_hidden(&self) -> bool {
		self.is_page_hidden
	}

	//==============================================================================
	// 페이지 이벤트 리스너 더하기. (리스너를 버리면 더 부르지 않는다, &Engine 으로도 더할 수 있다)
	//==============================================================================
	pub fn add_page_event_listener(&self, page_event_listener: &PageEventListener) {
		self.page_event_listener_list.borrow_mut().push(Rc::downgrade(page_event_listener));
	}

	//==============================================================================
	// 페이지 이벤트 리스너 빼기. (같은 리스너를 더 부르지 않는다, 버린 리스너도 빠진다)
	//==============================================================================
	pub fn remove_page_event_listener(&self, page_event_listener: &PageEventListener) {
		let target_listener = Rc::downgrade(page_event_listener);
		self.page_event_listener_list.borrow_mut().retain(|weak_listener| weak_listener.strong_count() > 0 && !weak_listener.ptr_eq(&target_listener));
	}

	//==============================================================================
	// 페이지 이벤트 알리기. (플랫폼이 부른다. 가려짐을 적고 살아 있는 리스너에 그 자리에서 넘긴다)
	// - 리스너가 다른 일로 빌려져 있으면 그 리스너는 이번 이벤트를 건너뛴다.
	//==============================================================================
	pub(crate) fn dispatch_page_event(&mut self, page_event: PageEvent) {
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
	// 지금 프레임의 번호. (원본처럼 첫 프레임이 1, 검증용 화면 저장의 프레임 번호도 이것)
	//==============================================================================
	pub fn get_frame_number(&self) -> u64 {
		self.frame_number
	}

	pub fn get_version_string(&self) -> String {
		self.version.to_string()
	}

	//==============================================================================
	// 검증용 화면 저장 요청. (그 프레임을 다 그린 뒤 화면의 PNG 바이트를 넘긴다, 원본 requestFrameCapture 는 PNG data URL 을 프로미스로 넘긴다)
	// - 이미 지난 번호면 이번(프레임 사이에 부르면 다음) 프레임을 저장한다.
	// - 주소 쿼리(웹 ?capture=120,240&captureName=shot_{frame}.png&captureExit=1)와 환경 변수(데스크탑 VANILLA_CAPTURE_*)로도 건다.
	//==============================================================================
	pub fn request_frame_capture(&mut self, frame_number: u64, capture_handler: FrameCaptureHandler) {
		let target_frame_number = frame_number.max(self.frame_number);
		self.frame_capture_handler_map.entry(target_frame_number).or_default().push(capture_handler);
	}
}

//==============================================================================
// 화면 저장 요청. (검증용, 데스크탑은 환경 변수, 웹은 원본처럼 주소 쿼리로 켠다)
// - VANILLA_CAPTURE_PATH:   저장 경로. "{frame}" 은 프레임 번호로 바뀐다. 예: shots/shot_{frame}.png
// - VANILLA_CAPTURE_FRAMES: 저장할 프레임 번호, 쉼표로 여럿. (기본 120)
// - VANILLA_CAPTURE_EXIT:   1 이면 마지막 저장 뒤 끝낸다.
// - 웹: ?capture=120,240 (번호, 빈 값이면 120), captureName=파일 이름("{frame}" 은 번호, 기본 capture_{frame}.png), captureExit=1. 내려받기로 저장한다.
//==============================================================================
struct CaptureRequest {
	output_path: String,
	frame_numbers: Vec<u64>,
	is_exit_after_last_capture: bool,
}

impl CaptureRequest {
	//==============================================================================
	// 환경 변수에서 읽기. (경로가 없으면 None)
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
	// 주소 쿼리에서 읽기. (웹, capture 가 없으면 None)
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
// 엔진 실행자. (엔진, 그래픽, 씬 목록을 들고 프레임을 돈다. 플랫폼이 들고 부른다)
//==============================================================================
pub struct EngineRunner {
	pub(crate) engine: Engine,
	graphic: Option<Graphic>,
	scene_list: Vec<Box<dyn Scene>>,
	start_instant: Instant,
	capture_request: Option<CaptureRequest>,
}

impl EngineRunner {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub(crate) fn new(mut engine: Engine, scene: Box<dyn Scene>) -> EngineRunner {
		engine.load_scene(scene);
		EngineRunner {
			engine,
			graphic: None,
			scene_list: Vec::new(),
			start_instant: Instant::now(),
			capture_request: CaptureRequest::create_from_environment(),
		}
	}

	//==============================================================================
	// 그래픽 넘겨받기. (플랫폼이 GL 컨텍스트를 만든 뒤 한 번. 기본 폰트도 이때 읽는다)
	//==============================================================================
	pub(crate) fn attach_graphic(&mut self, mut graphic: Graphic) {
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
	// 그래픽. (데스크탑 플랫폼이 프레임 끝에 렌더 영역을 늘일 때, 붙이기 전에는 None)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn get_graphic(&self) -> Option<&Graphic> {
		self.graphic.as_ref()
	}

	//==============================================================================
	// 요청된 프레임이면 화면을 PNG 로 저장. (화면을 넘기기 전에 부른다)
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
	// 기본 폰트 읽기. (후보 경로 가운데 처음 읽히는 것)
	//==============================================================================
	fn load_default_fonts(&self, graphic: &mut Graphic) {
		let engine_configuration = self.engine.get_engine_configuration();
		let text_string_texture_cache = graphic.get_text_string_texture_cache();
		let font_source_list = [
			(false, &engine_configuration.default_font_bytes, &engine_configuration.default_font_paths),
			(true, &engine_configuration.default_bold_font_bytes, &engine_configuration.default_bold_font_paths),
		];
		for (is_bold, font_bytes, font_paths) in font_source_list {
			// 미리 받은 내용이 있으면 그것부터.
			if let Some(font_bytes) = font_bytes {
				match text_string_texture_cache.set_default_font(is_bold, font_bytes.clone()) {
					Ok(()) => {
						continue;
					}
					Err(error) => {
						eprintln!("Engine: {} (미리 받은 폰트)", error);
					}
				}
			}
			for font_path in font_paths.iter() {
				let font_bytes = match std::fs::read(font_path) {
					Ok(font_bytes) => {
						font_bytes
					}
					Err(_) => {
						continue;
					}
				};
				match text_string_texture_cache.set_default_font(is_bold, font_bytes) {
					Ok(()) => {
						break;
					}
					Err(error) => {
						eprintln!("Engine: {} ({})", error, font_path.display());
					}
				}
			}
		}
		for font_path in engine_configuration.fallback_font_paths.iter() {
			if font_path.exists() {
				text_string_texture_cache.add_fallback_font_candidate(font_path.clone());
			}
		}
		if !text_string_texture_cache.has_default_font() {
			eprintln!("Engine: 기본 폰트를 찾지 못했습니다. 글자가 보이지 않습니다.");
		}
	}

	//==============================================================================
	// 씬마다 해상도 변경 알림.
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
	// 바이트가 온 글꼴을 글자 캐시에 등록. (프레임 시작에)
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
		for (family, font_face_descriptors, load_handle) in pending_font_registration_list {
			if !load_handle.is_finished() {
				self.engine.pending_font_registration_list.push((family, font_face_descriptors, load_handle));
				continue;
			}
			let font_bytes = match load_handle.get_bytes() {
				Some(font_bytes) => {
					font_bytes
				}
				None => {
					continue;
				}
			};
			let text_string_texture_cache = graphic.get_text_string_texture_cache();
			if let Err(error) = text_string_texture_cache.register_font_face(&family, font_face_descriptors, font_bytes.to_vec()) {
				eprintln!("Engine: {} ({})", error, load_handle.get_asset_path());
			}
		}
	}

	//==============================================================================
	// default_font_url 에서 받은 바이트를 기본 폰트(보통, 굵게 둘 다)로 등록. (웹)
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
	// 페이지 가시성 복원 알림. (소리를 다시 깨우고 입력을 비운 뒤 씬마다 on_page_visibility_restored)
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
	// 페이지 프로세스 복원 알림. (소리를 다시 깨우고 입력을 비운 뒤 씬마다 on_page_process_restored)
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
	// 엔진 복구. (백그라운드나 파기에서 돌아올 때. 멈춘 오디오 컨텍스트를 깨우고, 눌린 채 남은 키와 터치를 비운다)
	//==============================================================================
	fn recover_engine_state(&mut self) {
		self.engine.get_audio_manager().resume_context();
		self.engine.get_input_manager_mut().clear();
	}

	//==============================================================================
	// 미뤄 둔 씬 내리기 / 불러오기.
	//==============================================================================
	pub(crate) fn apply_pending_scenes(&mut self) {
		if self.engine.is_unload_all_scenes_requested {
			self.engine.is_unload_all_scenes_requested = false;
			let mut scene_list = std::mem::take(&mut self.scene_list);
			for scene in scene_list.iter_mut() {
				unload_scene_instance(&mut self.engine, scene.as_mut());
			}
		}
		while !self.engine.pending_unload_scene_id_list.is_empty() {
			let pending_unload_scene_id_list = std::mem::take(&mut self.engine.pending_unload_scene_id_list);
			for scene_id in pending_unload_scene_id_list {
				self.engine.pending_scene_list.retain(|pending_scene| pending_scene.get_scene_base().get_scene_id() != Some(scene_id));
				let scene_index = self.scene_list.iter().position(|scene| scene.get_scene_base().get_scene_id() == Some(scene_id));
				if let Some(scene_index) = scene_index {
					let mut scene = self.scene_list.remove(scene_index);
					unload_scene_instance(&mut self.engine, scene.as_mut());
				}
			}
		}
		while !self.engine.pending_scene_list.is_empty() {
			let pending_scene_list = std::mem::take(&mut self.engine.pending_scene_list);
			for mut scene in pending_scene_list {
				scene.create(&mut self.engine);
				initialize_scene_if_ready(&mut self.engine, scene.as_mut());
				self.scene_list.push(scene);
			}
		}
	}

	//==============================================================================
	// 엔진 갱신. (한 프레임)
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

		// 그리기 통계. (지난 프레임 값을 넘겨 두고 이번 프레임을 새로 센다)
		graphic.begin_frame_statistics();

		// 씬 처리.
		let mut scene_list = std::mem::take(&mut self.scene_list);
		for scene in scene_list.iter_mut() {
			// 맡긴 애셋을 기다리는 씬은 로딩 화면만. (다 읽혔으면 이번 프레임부터 돈다)
			if !scene.get_scene_base().is_loaded() {
				initialize_scene_if_ready(&mut self.engine, scene.as_mut());
			}
			if !scene.get_scene_base().is_loaded() {
				scene.draw_on_load(&mut self.engine, graphic);
				continue;
			}

			// 갱신. (씬의 터치 훅은 원본 Scene.tickTouch 처럼 tick 안의 터치 다음)
			scene.tick(&mut self.engine, time_delta);
			dispatch_scene_touch(&mut self.engine, scene.as_mut());

			// 출력.
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

		// 마우스 커서. (켜져 있고 바뀌었을 때만 플랫폼에 넘긴다)
		let is_coarse_pointer = platform::is_coarse_pointer();
		if let Some(cursor_value) = self.engine.pointer_cursor.take_cursor_change(is_coarse_pointer) {
			platform::set_cursor(self.engine.platform_window.as_ref(), &cursor_value.css_value, &cursor_value.fallback_name);
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
// 개발 정보 출력. (왼쪽 위에 FPS, 드로우 콜, 정점 수, 화면 밖이라 거른 노드 수)
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
// 화면 저장 프레임 번호 읽기. (쉼표로 여럿, 같은 번호는 한 번, 하나도 없으면 원본처럼 120)
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
// RGBA 픽셀을 PNG 바이트로.
//==============================================================================
fn encode_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
	let mut png_bytes: Vec<u8> = Vec::new();
	image::write_buffer_with_format(&mut std::io::Cursor::new(&mut png_bytes), pixels, width, height, image::ExtendedColorType::Rgba8, image::ImageFormat::Png).map_err(|error| error.to_string())?;
	Ok(png_bytes)
}

//==============================================================================
// 씬 하나 내리기. (원본 SceneManager.unloadScene 의 finalize, unload 다음 러스트 판의 destroy)
//==============================================================================
fn unload_scene_instance(engine: &mut Engine, scene: &mut dyn Scene) {
	scene.finalize(engine);
	scene.unload(engine);
	scene.destroy(engine);
}

//==============================================================================
// 씬의 터치 훅 부르기. (원본 Scene.tickTouch: 누름, 그다음 뗌 또는 누르고 있음(누른 프레임 빼고) 또는 취소, 그리고 휠)
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
// 씬이 맡긴 애셋이 다 읽혔으면 initialize 하고 불러온 것으로 친다. (원본 load 다음 set_loaded, initialize)
//==============================================================================
fn initialize_scene_if_ready(engine: &mut Engine, scene: &mut dyn Scene) {
	if scene.get_scene_base().is_loaded() || !scene.get_scene_base().are_tracked_assets_finished() {
		return;
	}
	scene.get_scene_base_mut().clear_tracked_assets();
	scene.initialize(engine);
	scene.get_scene_base_mut().set_loaded(true);
	let canvas_native_size = engine.view_manager.get_canvas_native_size();
	scene.resize(engine, canvas_native_size);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::scene::SceneBase;
	use std::cell::Cell;

	//==============================================================================
	// 시험용 씬. (create, destroy, 페이지 복원 훅이 불린 수를 센다)
	//==============================================================================
	struct CountingScene {
		scene_base: SceneBase,
		create_count: Rc<Cell<u32>>,
		destroy_count: Rc<Cell<u32>>,
		restore_count: Rc<Cell<u32>>,
	}

	impl CountingScene {
		//==============================================================================
		// 세는 칸을 나눠 들고 생성.
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
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}

		fn create(&mut self, _engine: &mut Engine) {
			self.create_count.set(self.create_count.get() + 1);
		}

		fn destroy(&mut self, _engine: &mut Engine) {
			self.destroy_count.set(self.destroy_count.get() + 1);
		}

		fn on_page_process_restored(&mut self, _engine: &mut Engine) {
			self.restore_count.set(self.restore_count.get() + 100);
		}

		fn on_page_visibility_restored(&mut self, _engine: &mut Engine) {
			self.restore_count.set(self.restore_count.get() + 1);
		}
	}

	//==============================================================================
	// 씬 하나 내리기: 프레임이 끝날 때 그 씬만 destroy, 다른 씬은 남고, create 전인 씬은 create 없이 빠진다.
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
	// 페이지 복원: 입력을 비우고 씬마다 훅을 부른다.
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
	// 페이지 이벤트: 가려짐을 적고 리스너에 넘긴다. 버린 리스너는 더 부르지 않는다.
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
	// 최대 FPS 는 실행 중에 바꾸고, 잘못된 값은 0(제한 없음)이다.
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
	// 차례를 적는 시험 씬. (내리기와 터치 훅)
	//==============================================================================
	struct RecordingScene {
		scene_base: SceneBase,
		record_list: Rc<RefCell<Vec<&'static str>>>,
	}

	impl Scene for RecordingScene {
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}

		fn finalize(&mut self, _engine: &mut Engine) {
			self.record_list.borrow_mut().push("finalize");
		}

		fn unload(&mut self, _engine: &mut Engine) {
			self.record_list.borrow_mut().push("unload");
		}

		fn destroy(&mut self, _engine: &mut Engine) {
			self.record_list.borrow_mut().push("destroy");
		}

		fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
			self.record_list.borrow_mut().push("press");
		}

		fn touch_move(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
			self.record_list.borrow_mut().push("move");
		}

		fn touch_release(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
			self.record_list.borrow_mut().push("release");
		}

		fn touch_wheel(&mut self, _engine: &mut Engine, _view_input_position: Vector2, _wheel_delta: Vector2) {
			self.record_list.borrow_mut().push("wheel");
		}
	}

	//==============================================================================
	// 씬 터치 훅은 원본 차례(누름, 같은 프레임 뗌, 누른 프레임 빼고 누르고 있음, 휠), 내리기는 finalize, unload, destroy.
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
		unload_scene_instance(&mut engine, &mut scene);
		assert_eq!(*record_list.borrow(), vec!["press", "move", "release", "wheel", "finalize", "unload", "destroy"]);
	}

	//==============================================================================
	// 화면 저장 요청은 지난 번호면 지금 프레임으로, 번호 글은 쉼표로 여럿(같은 번호 한 번, 없으면 120), PNG 로 굽는다.
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
}
