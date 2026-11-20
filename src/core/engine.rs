//==============================================================================
// 엔진.
// - 원본은 캔버스를 잡고 request_animation_frame 으로 프레임을 돈다.
//   러스트 판은 창, GL 컨텍스트, 입력 이벤트, 화면 넘기기를 플랫폼 층(crate::platform)이 맡고,
//   여기 EngineRunner 는 플랫폼을 모른 채 씬과 프레임 흐름만 돈다. (데스크탑: platform/desktop.rs, 웹: platform/web.rs)
// - 씬에는 &mut Engine 을 넘긴다. 씬은 여기서 매니저(시간, 뷰, 입력)를 꺼내 쓴다. (원본 Engine.get_engine() 자리)
// - Graphic 은 플랫폼이 GL 컨텍스트를 만든 뒤 attach_graphic 으로 넘겨받고, 씬의 draw 계열에 따로 넘긴다.
// - 오디오 매니저를 원본처럼 생성할 때 만들어 든다. AudioContext(소리 장치)는 처음 쓸 때 만든다. 프레임마다 하는 오디오 일은 없다.
//   웹은 플랫폼이 click / touchstart / keydown 에서 resume_on_user_gesture, focus / visibilitychange 에서 resume 을 부른다.
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
use crate::core::scene::Scene;
use crate::core::timemanager::TimeManager;
use crate::core::viewmanager::{ViewManager, ViewScaleMode};
use crate::platform::{self, PlatformWindow};
use crate::resource::audioasset::AudioAsset;
use std::path::PathBuf;
use std::rc::Rc;
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
	// 최대 FPS. 0 이면 제한하지 않는다. (수직 동기를 켜면 화면 주사율을 따른다)
	pub maximum_frame_per_second: f32,
	// 수직 동기.
	pub use_vertical_sync: bool,
	// 기본 폰트 후보 경로. (앞에서부터 읽히는 첫 파일. 원본 default_font_url 자리)
	pub default_font_paths: Vec<PathBuf>,
	// 굵은 기본 폰트 후보 경로.
	pub default_bold_font_paths: Vec<PathBuf>,
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
			asset_directory_paths: Vec::new(),
			window_icon_path: String::new(),
			default_font_bytes: None,
			default_bold_font_bytes: None,
			canvas_id: String::from("mainCanvas"),
			default_font_url: String::new(),
			resume_audio_on_touch_end: false,
			asset_base_url: String::new(),
		}
	}
}

impl Default for EngineConfiguration {
	fn default() -> EngineConfiguration {
		EngineConfiguration::new()
	}
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
	pending_scene_list: Vec<Box<dyn Scene>>,
	is_unload_all_scenes_requested: bool,
	pending_font_registration_list: Vec<(String, bool, AssetLoadHandle)>,
	pending_default_font_handle: Option<AssetLoadHandle>,
	platform: Platform,
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
			frame_number: 0,
			version: ENGINE_VERSION,
			is_exit_requested: false,
			pending_scene_list: Vec::new(),
			is_unload_all_scenes_requested: false,
			pending_font_registration_list: Vec::new(),
			pending_default_font_handle: None,
			platform: Platform::new(),
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
	// 씬 불러오기. (이번 프레임이 끝난 뒤 목록에 더한다)
	//==============================================================================
	pub fn load_scene(&mut self, scene: Box<dyn Scene>) {
		self.pending_scene_list.push(scene);
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
		let load_handle = AssetLoadHandle::create_pending(relative_path);
		#[cfg(not(target_arch = "wasm32"))]
		let asset_location = self.resolve_asset_path(relative_path).to_string_lossy().into_owned();
		#[cfg(target_arch = "wasm32")]
		let asset_location = format!("{}{}", self.engine_configuration.asset_base_url, relative_path);
		platform::request_asset_bytes(asset_location, load_handle.clone());
		load_handle
	}

	//==============================================================================
	// 글꼴 바이트가 오면 이름으로 등록해 달라고 맡기기. (FontAsset 이 부른다. 등록은 프레임 시작에 한다)
	//==============================================================================
	pub fn register_font_when_loaded(&mut self, family: &str, is_bold: bool, load_handle: AssetLoadHandle) {
		self.pending_font_registration_list.push((family.to_string(), is_bold, load_handle));
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

	pub fn get_frame_number(&self) -> u64 {
		self.frame_number
	}

	pub fn get_version_string(&self) -> String {
		self.version.to_string()
	}
}

//==============================================================================
// 화면 저장 요청. (검증용, 환경 변수로 켠다)
// - VANILLA_CAPTURE_PATH:   저장 경로. "{frame}" 은 프레임 번호로 바뀐다. 예: shots/shot_{frame}.png
// - VANILLA_CAPTURE_FRAMES: 저장할 프레임 번호, 쉼표로 여럿. (기본 120)
// - VANILLA_CAPTURE_EXIT:   1 이면 마지막 저장 뒤 끝낸다.
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
	fn create_from_environment() -> Option<CaptureRequest> {
		let output_path = std::env::var("VANILLA_CAPTURE_PATH").ok()?;
		let frame_numbers_text = std::env::var("VANILLA_CAPTURE_FRAMES").unwrap_or_else(|_| String::from("120"));
		let mut frame_numbers: Vec<u64> = frame_numbers_text.split(',').filter_map(|part| part.trim().parse::<u64>().ok()).collect();
		frame_numbers.sort();
		let is_exit_after_last_capture = std::env::var("VANILLA_CAPTURE_EXIT").map(|value| value == "1").unwrap_or(false);
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
				platform::request_asset_bytes(default_font_url, load_handle.clone());
				self.engine.pending_default_font_handle = Some(load_handle);
			}
		}
		self.graphic = Some(graphic);
	}

	//==============================================================================
	// 요청된 프레임이면 화면을 PNG 로 저장. (화면을 넘기기 전에 부른다)
	//==============================================================================
	fn capture_frame_if_requested(&mut self) {
		let frame_number = self.engine.frame_number;
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
		match image::save_buffer(&output_path, &pixels, width, height, image::ExtendedColorType::Rgba8) {
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
		if self.engine.pending_font_registration_list.is_empty() {
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
		let pending_font_registration_list = std::mem::take(&mut self.engine.pending_font_registration_list);
		for (family, is_bold, load_handle) in pending_font_registration_list {
			if !load_handle.is_finished() {
				self.engine.pending_font_registration_list.push((family, is_bold, load_handle));
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
			if let Err(error) = text_string_texture_cache.register_font(&family, is_bold, font_bytes.to_vec()) {
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
		let text_string_texture_cache = graphic.get_text_string_texture_cache();
		for is_bold in [false, true] {
			if let Err(error) = text_string_texture_cache.set_default_font(is_bold, font_bytes.to_vec()) {
				eprintln!("Engine: {} ({})", error, load_handle.get_asset_path());
			}
		}
	}

	//==============================================================================
	// 미뤄 둔 씬 내리기 / 불러오기.
	//==============================================================================
	pub(crate) fn apply_pending_scenes(&mut self) {
		if self.engine.is_unload_all_scenes_requested {
			self.engine.is_unload_all_scenes_requested = false;
			let mut scene_list = std::mem::take(&mut self.scene_list);
			for scene in scene_list.iter_mut() {
				scene.destroy(&mut self.engine);
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

			// 갱신.
			scene.tick(&mut self.engine, time_delta);

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
		input_manager.set_touch_moved(false);
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
