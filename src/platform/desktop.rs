//==============================================================================
// 데스크탑 플랫폼. (윈도우, 맥, 리눅스)
// - 창(winit)과 GL 3.3 코어 컨텍스트(glutin)를 만들고, 창의 다시 그리기 요청마다 엔진 프레임을 하나 돈다.
// - 창 이벤트를 입력 관리자 꼴로 바꿔 넣는다. (키는 W3C KeyboardEvent.code 와 같은 이름, 마우스 단추는 원본처럼 어느 것이든 터치, 왼쪽만으로 바꿀 수 있다)
// - 엔진 본체(EngineRunner)는 창을 모른다. 여기서 프레임을 부르고, 그린 뒤 화면을 넘긴다.
// - 글 입력(키 글자, 편집 키, IME 조합)과 클립보드는 desktop/textinput.rs 가 맡는다. 오른쪽 단추는 입력 필드 문맥 메뉴 요청이다.
// - 맥의 게임패드 진동은 desktop/gamepadhaptics.rs 가 맡는다. (GameController 와 CoreHaptics, 다른 OS 는 gilrs 가 떤다)
// - 마지막으로 쓴 장치를 키, 마우스, 손가락 이벤트마다 적는다. 막을 키(set_prevented_keys)는 창에 막을 브라우저 기본 동작이 없어 쓰지 않는다.
// - 페이지 이벤트: 창이 가려짐(Occluded, 맥과 X11 이 알린다), 최소화(크기 0 이나 is_minimized, 윈도우가 이것으로 알린다), 앱 멈춤(suspended, 모바일)
//   가운데 하나라도 맞으면 Hidden, 모두 풀리면 Visible 과 on_page_visibility_restored. 포커스는 Focused / Blurred, 마우스 단추와 손가락 누름은 PointerDown.
//   웨이랜드는 가림도 최소화도 알리지 않아 Hidden 이 오지 않는다.
// - 페이지 프로세스 복원(on_page_process_restored, 원본 Chromium freeze 뒤 resume 과 bfcache): suspended 뒤 resumed, 그리고 시스템 잠자기에서 깬 것.
//   윈도우는 창을 서브클래싱해 WM_POWERBROADCAST 의 PBT_APMRESUMEAUTOMATIC 을 받는다. (desktop/windowsplatform.rs, 윈도우의 단조 시계는 잠든 동안에도 흘러 시계로는 못 가린다)
//   맥과 리눅스는 두 프레임 사이에 벽시계(SystemTime)가 단조 시계(Instant)보다 SYSTEM_SLEEP_GAP_SECONDS 넘게 더 흘렀으면 깬 것으로 본다.
//   (러스트 Instant 는 맥에서 CLOCK_UPTIME_RAW, 리눅스에서 CLOCK_MONOTONIC 이라 잠든 동안 멈춘다. 러스트는 이것을 보장하지 않는다고 적어 두었다)
// - 주된 손이 손가락인지(is_coarse_pointer): 윈도우는 크로미엄의 (pointer: coarse) 판정을 옮겨 태블릿 모드나 마우스 없는 터치 기기면 참이다.
//   맥은 크로미엄도 늘 마우스로 보아 거짓이다. 리눅스와 그 밖의 유닉스는 크로미엄처럼 화면 서버(웨이랜드 wl_seat 능력, X11 XInput 장치 목록)에서 터치스크린과
//   마우스, 터치패드를 읽어 터치스크린만 있으면 참이다. (desktop/linuxplatform.rs, 읽은 값은 2 초 동안 쓴다)
// - 키 이름: winit 의 SuperLeft, SuperRight 는 W3C 의 MetaLeft, MetaRight, winit 의 Meta 는 W3C 의 Super 로 바꿔 넣는다.
// - 그림 커서(PointerCursor.register_image_cursor): 그림 파일을 읽어 winit 의 CustomCursor 로 만든다. 맥은 그림 크기가 그대로 포인트 크기라 1 배 그림을,
//   윈도우와 리눅스는 물리 픽셀이라 창 배율에 맞춰(2 배 그림이 있으면 그것을) 늘인 그림과 핫스팟을 쓴다. 만드는 동안과 못 만들면 함께 준 CSS 커서다.
// - 렌더 배율 상한(ViewManager.set_max_render_pixel_ratio): 그 배율로 줄인 크기로 그리고 프레임 끝에 창 크기로 늘인다. (desktop/renderscaler.rs)
//   입력 좌표는 창의 배율로 논리 좌표로 바꾸므로 렌더 크기와 상관없다.
// - 캔버스 비 상한(EngineConfiguration.max_canvas_aspect_ratio): 창이 그보다 길쭉하면 캔버스(그리는 영역)를 원본처럼 줄여 창 가운데에 두고 나머지는 검다.
//   입력 좌표에서 캔버스의 왼쪽 위를 빼고, read_canvas_client_position 이 그 자리를 돌려준다.
// - 페이지 주소의 쿼리 값(read_page_query_value)은 환경 변수 VANILLA_ 뒤에 이름을 대문자 뱀꼴로 붙인 것으로 읽는다. (safeArea → VANILLA_SAFE_AREA)
// - 찍기 모드(VANILLA_CAPTURE_PATH)는 창을 숨기고 돈다. 숨긴 창은 다시 그리기 요청과 수직 동기가 없어 about_to_wait 에서 프레임을 바로 돌고 주사율로 빠르기를 맞춘다.
//   가려짐 알림은 받지 않는다. 맥은 독 아이콘과 포커스도 가져가지 않는다. 창을 보려면 VANILLA_CAPTURE_SHOW_WINDOW=1.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::{EngineRunner, PageEvent};
use crate::core::graphic::Graphic;
use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version as GlVersion};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Duration;
use web_time::Instant;
#[cfg(not(target_os = "windows"))]
use web_time::SystemTime;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use crate::core::inputmanager::{InputDevice, MouseButton as EngineMouseButton};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, CustomCursor};
use crate::core::pointercursor::CursorValue;
use std::cell::RefCell;
use std::collections::HashMap;
use winit::keyboard::PhysicalKey;
use winit::window::{Icon, Window, WindowId};

mod renderscaler;
mod textinput;
#[cfg(target_os = "windows")]
mod windowsplatform;
#[cfg(any(test, all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
mod linuxplatform;
pub use textinput::{read_clipboard_text, read_clipboard_text_with_callback, write_clipboard_text};
#[cfg(target_os = "macos")]
pub mod gamepadhaptics;
pub mod systemfont;
#[cfg(target_os = "macos")]
pub mod coretextglyph;
pub mod emojipanel;

// 엔진이 들고 있는 창 손잡이. (커서, 제목을 바꿀 때)
pub type PlatformWindow = Rc<Window>;

// 휠 한 줄을 픽셀로 볼 때의 값. (브라우저 deltaMode 줄 단위 환산과 같은 자리)
const WHEEL_LINE_PIXEL_SIZE: f32 = 40.0;
// 두 프레임 사이에 벽시계가 단조 시계보다 이만큼 넘게 더 흐르면 시스템이 잠들었다 깬 것으로 본다. (초, 맥과 리눅스)
#[cfg(not(target_os = "windows"))]
const SYSTEM_SLEEP_GAP_SECONDS: f64 = 2.0;

thread_local! {
	// 창 안에서 캔버스의 왼쪽 위. (논리 좌표, 캔버스 비 상한으로 가운데에 둘 때만 0 이 아니다)
	static CANVAS_CLIENT_POSITION: std::cell::Cell<Vector2> = const { std::cell::Cell::new(Vector2::create(0.0, 0.0)) };
	// 만든 그림 커서. (열쇠는 그림 주소, 2 배 그림 주소, 핫스팟, 창 배율. 못 만든 것은 None 으로 두어 다시 읽지 않는다)
	static CUSTOM_CURSOR_TABLE: RefCell<HashMap<String, Option<CustomCursor>>> = RefCell::new(HashMap::new());
	// 아직 만들지 않은 그림 커서. (set_cursor 가 넣고, 이벤트 루프가 프레임 뒤에 만들어 건다)
	static PENDING_CUSTOM_CURSOR: RefCell<Option<CursorValue>> = const { RefCell::new(None) };
}

//==============================================================================
// 시작. (창을 닫거나 exit_application 을 부를 때까지 돌아오지 않는다)
//==============================================================================
pub fn run(engine_runner: EngineRunner) -> Result<(), String> {
	let is_window_hidden = is_capture_window_hidden();
	let mut event_loop_builder = EventLoop::builder();
	#[cfg(target_os = "macos")]
	if is_window_hidden {
		use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
		event_loop_builder.with_activation_policy(ActivationPolicy::Accessory).with_activate_ignoring_other_apps(false);
	}
	let event_loop = event_loop_builder.build().map_err(|error| format!("이벤트 루프 생성 실패: {}", error))?;
	event_loop.set_control_flow(ControlFlow::Poll);
	let mut desktop_application = DesktopApplication {
		engine_runner,
		window_state: None,
		next_frame_instant: Instant::now(),
		fatal_error: None,
		text_input_state: textinput::DesktopTextInputState::new(),
		is_suspended: false,
		is_occluded: false,
		is_minimized: false,
		is_page_hidden: false,
		is_window_hidden,
		window_scale_factor: 1.0,
		applied_max_render_pixel_ratio: 0.0,
		render_scaler: None,
		#[cfg(not(target_os = "windows"))]
		last_frame_clock: None,
		canvas_physical_rect: [0, 0, 0, 0],
	};
	event_loop.run_app(&mut desktop_application).map_err(|error| format!("이벤트 루프 실행 실패: {}", error))?;
	match desktop_application.fatal_error.take() {
		Some(fatal_error) => {
			Err(fatal_error)
		}
		None => {
			Ok(())
		}
	}
}

//==============================================================================
// 커서 보이기 설정.
//==============================================================================
pub fn set_cursor_visible(platform_window: Option<&PlatformWindow>, is_visible: bool) {
	if let Some(window) = platform_window {
		window.set_cursor_visible(is_visible);
	}
}

//==============================================================================
// 커서 모양 설정. (CSS 커서 이름을 winit 커서 아이콘으로, none 이면 감춤, 모르는 이름은 기본 화살표)
// - 그림 커서는 만들어 둔 것이 있으면 그것을 건다. 아직 없으면 이벤트 루프가 만들도록 부탁해 두고 그동안은 함께 준 CSS 커서를 쓴다.
//==============================================================================
pub fn set_cursor(platform_window: Option<&PlatformWindow>, cursor_value: &CursorValue) {
	let window = match platform_window {
		Some(window) => {
			window
		}
		None => {
			return;
		}
	};
	if !cursor_value.image_url.is_empty() {
		let custom_cursor_key = create_custom_cursor_key(cursor_value, window.scale_factor());
		let custom_cursor = CUSTOM_CURSOR_TABLE.with(|custom_cursor_table| custom_cursor_table.borrow().get(&custom_cursor_key).cloned());
		match custom_cursor {
			Some(Some(custom_cursor)) => {
				window.set_cursor_visible(true);
				window.set_cursor(custom_cursor);
				return;
			}
			Some(None) => {
			}
			None => {
				PENDING_CUSTOM_CURSOR.with(|pending_custom_cursor| {
					*pending_custom_cursor.borrow_mut() = Some(cursor_value.clone());
				});
			}
		}
	}
	else {
		PENDING_CUSTOM_CURSOR.with(|pending_custom_cursor| {
			*pending_custom_cursor.borrow_mut() = None;
		});
	}
	let cursor_name = cursor_value.fallback_name.as_str();
	if cursor_name == "none" {
		window.set_cursor_visible(false);
		return;
	}
	window.set_cursor_visible(true);
	let cursor_icon = match cursor_name {
		"pointer" => {
			CursorIcon::Pointer
		}
		"crosshair" => {
			CursorIcon::Crosshair
		}
		"not-allowed" => {
			CursorIcon::NotAllowed
		}
		"grab" => {
			CursorIcon::Grab
		}
		"grabbing" => {
			CursorIcon::Grabbing
		}
		"text" => {
			CursorIcon::Text
		}
		"move" => {
			CursorIcon::Move
		}
		"help" => {
			CursorIcon::Help
		}
		"wait" => {
			CursorIcon::Wait
		}
		"progress" => {
			CursorIcon::Progress
		}
		"col-resize" => {
			CursorIcon::ColResize
		}
		"row-resize" => {
			CursorIcon::RowResize
		}
		_ => {
			CursorIcon::Default
		}
	};
	window.set_cursor(cursor_icon);
}

//==============================================================================
// 주된 손이 손가락인지. (윈도우는 크로미엄처럼 태블릿 모드나 마우스 없는 터치 기기면 참, 리눅스는 크로미엄처럼 마우스, 터치패드 없이 터치스크린만 있으면 참, 맥은 마우스가 주된 손으로 본다)
//==============================================================================
pub fn is_coarse_pointer() -> bool {
	#[cfg(target_os = "windows")]
	{
		windowsplatform::is_coarse_pointer()
	}
	#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
	{
		linuxplatform::is_coarse_pointer()
	}
	#[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))))]
	{
		false
	}
}

//==============================================================================
// 문서의 초점이 글자 입력칸에 있는지. (데스크탑에는 문서가 없어 늘 false, 엔진 입력 필드는 입력 매니저가 따로 본다)
//==============================================================================
pub fn is_text_field_focused() -> bool {
	false
}

//==============================================================================
// 페이지 주소의 쿼리 값. (데스크탑에는 주소가 없어 환경 변수 VANILLA_ 뒤에 이름을 대문자 뱀꼴로 붙인 것을 읽는다, 없으면 None)
// - safeArea → VANILLA_SAFE_AREA, quality → VANILLA_QUALITY
//==============================================================================
pub fn read_page_query_value(name: &str) -> Option<String> {
	std::env::var(convert_query_name_to_variable_name(name)).ok()
}

//==============================================================================
// 쿼리 이름을 환경 변수 이름으로. (첫 글자가 아닌 대문자 앞에 밑줄, 모두 대문자, 앞에 VANILLA_)
//==============================================================================
fn convert_query_name_to_variable_name(name: &str) -> String {
	let mut variable_name = String::from("VANILLA_");
	for (character_index, character) in name.chars().enumerate() {
		if character.is_ascii_uppercase() && character_index > 0 {
			variable_name.push('_');
		}
		variable_name.push(character.to_ascii_uppercase());
	}
	variable_name
}

//==============================================================================
// 화면(client) 좌표에서 캔버스의 왼쪽 위. (데스크탑은 창의 논리 좌표가 client 좌표다, 캔버스 비 상한으로 가운데에 둘 때만 0 이 아니다)
//==============================================================================
pub fn read_canvas_client_position() -> Vector2 {
	CANVAS_CLIENT_POSITION.with(|canvas_client_position| canvas_client_position.get())
}

//==============================================================================
// 검증용 화면 저장. (파일로 쓴다)
//==============================================================================
pub fn save_capture_file(output_path: &str, png_bytes: &[u8]) -> Result<(), String> {
	std::fs::write(output_path, png_bytes).map_err(|error| error.to_string())
}

//==============================================================================
// 페이지 전체 크기. (원본 window.innerWidth, innerHeight, 데스크탑에는 페이지가 없어 None 이고 ViewManager 가 창 안쪽 크기를 쓴다)
//==============================================================================
pub fn read_client_native_size() -> Option<Vector2> {
	None
}

//==============================================================================
// CSS env(safe-area-inset-*) 세이프 에어리어 인셋. (데스크탑에는 없어 0)
//==============================================================================
pub fn read_safe_area_insets() -> [f32; 4] {
	[0.0, 0.0, 0.0, 0.0]
}

//==============================================================================
// 응용 프로그램 종료. (데스크탑은 엔진의 종료 요청을 보고 이벤트 루프가 끝나므로 따로 할 일이 없다)
//==============================================================================
pub fn exit_application() {
}

//==============================================================================
// 창 제목 설정.
//==============================================================================
pub fn set_title(platform_window: Option<&PlatformWindow>, title: &str) {
	if let Some(window) = platform_window {
		window.set_title(title);
	}
}

//==============================================================================
// 애셋 바이트 읽기. (파일을 그 자리에서 읽고 끝낸다, 파일에는 캐시가 없어 is_no_cache 는 쓰지 않는다. 읽은 파일은 리소스 사용량에 적는다)
//==============================================================================
pub fn request_asset_bytes(asset_location: String, load_handle: AssetLoadHandle, _is_no_cache: bool) {
	let result = std::fs::read(&asset_location).map_err(|error| format!("애셋 읽기 실패 ({}): {}", asset_location, error));
	if let Ok(bytes) = &result {
		crate::base::platform::record_loaded_file(&asset_location, bytes.len() as u64);
	}
	load_handle.finish(result);
}

//==============================================================================
// 창과 GL 상태.
//==============================================================================
struct WindowState {
	window: Rc<Window>,
	surface: Surface<WindowSurface>,
	context: PossiblyCurrentContext,
}

//==============================================================================
// 이벤트 루프에 물리는 데스크탑 실행자.
//==============================================================================
struct DesktopApplication {
	engine_runner: EngineRunner,
	window_state: Option<WindowState>,
	next_frame_instant: Instant,
	fatal_error: Option<String>,
	text_input_state: textinput::DesktopTextInputState,
	// 앱이 suspended 되었는지. (다시 resumed 되면 페이지 프로세스 복원으로 알린다)
	is_suspended: bool,
	// 창이 가려졌는지. (Occluded)
	is_occluded: bool,
	// 창이 최소화되었는지.
	is_minimized: bool,
	// 엔진에 알린 페이지 가려짐. (가려짐, 최소화, 앱 멈춤 가운데 하나)
	is_page_hidden: bool,
	// 창의 배율. (입력 좌표를 논리 좌표로 바꿀 때, 렌더 배율 상한과 상관없다)
	window_scale_factor: f32,
	// 지금 렌더 크기에 건 배율 상한. (바뀌면 다시 맞춘다)
	applied_max_render_pixel_ratio: f32,
	// 줄인 렌더 영역을 창 크기로 늘이개. (처음 쓸 때 만든다)
	render_scaler: Option<renderscaler::RenderScaler>,
	// 창을 숨기고 도는지. (찍기 모드, is_capture_window_hidden)
	is_window_hidden: bool,
	// 지난 프레임의 단조 시계와 벽시계. (맥과 리눅스의 잠자기 판정)
	#[cfg(not(target_os = "windows"))]
	last_frame_clock: Option<(Instant, SystemTime)>,
	// 창 안의 캔버스 자리. (물리 픽셀 [왼쪽, 위, 가로, 세로], 위 왼쪽 원점)
	canvas_physical_rect: [i32; 4],
}

impl DesktopApplication {
	//==============================================================================
	// 창과 GL 컨텍스트, 그래픽 만들기.
	//==============================================================================
	fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
		let engine = &self.engine_runner.engine;
		let engine_configuration = engine.get_engine_configuration();
		let window_size = if engine_configuration.window_size.x > 0.0 && engine_configuration.window_size.y > 0.0 {
			engine_configuration.window_size
		}
		else {
			engine_configuration.reference_resolution_size
		};
		let mut window_attributes = Window::default_attributes()
			.with_title(engine_configuration.title.clone())
			.with_inner_size(LogicalSize::new(window_size.x as f64, window_size.y as f64))
			.with_resizable(engine_configuration.is_resizable)
			.with_visible(!self.is_window_hidden)
			.with_active(!self.is_window_hidden);
		// 검증용: VANILLA_WINDOW_POSITION=x,y 면 창을 그 자리(물리 픽셀)에 띄운다. (화면 밖에 두고 잴 때)
		if let Ok(window_position_text) = std::env::var("VANILLA_WINDOW_POSITION") {
			let position_list: Vec<i32> = window_position_text.split(',').filter_map(|part| part.trim().parse::<i32>().ok()).collect();
			if position_list.len() == 2 {
				window_attributes = window_attributes.with_position(winit::dpi::PhysicalPosition::new(position_list[0], position_list[1]));
			}
		}
		if !engine_configuration.window_icon_path.is_empty() {
			let window_icon_path = engine.resolve_asset_path(&engine_configuration.window_icon_path);
			match load_window_icon(&window_icon_path) {
				Ok(window_icon) => {
					window_attributes = window_attributes.with_window_icon(Some(window_icon));
				}
				Err(error) => {
					eprintln!("Engine: {}", error);
				}
			}
		}

		// 깊이 24 / 스텐실 8 비트가 있는 설정 가운데 멀티샘플 4 에 가까운 것. (원본 WebGL 컨텍스트의 기본 depth: true 와 안티에일리어싱 자리, 깊이는 3D 가 쓴다)
		let config_template_builder = ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(24).with_stencil_size(8);
		let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));
		let (window, gl_config) = display_builder.build(event_loop, config_template_builder, pick_config).map_err(|error| format!("GL 설정 고르기 실패: {}", error))?;
		let window = window.ok_or_else(|| String::from("창 생성 실패"))?;

		let raw_window_handle = window.window_handle().ok().map(|window_handle| window_handle.as_raw());
		let gl_display = gl_config.display();
		let context_attributes = ContextAttributesBuilder::new()
			.with_context_api(ContextApi::OpenGl(Some(GlVersion::new(3, 3))))
			.with_profile(GlProfile::Core)
			.build(raw_window_handle);
		let not_current_context = unsafe {
			gl_display.create_context(&gl_config, &context_attributes)
		}.map_err(|error| format!("GL 3.3 코어 컨텍스트 생성 실패: {}", error))?;
		let surface_attributes = window.build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new()).map_err(|error| format!("서피스 속성 실패: {}", error))?;
		let surface = unsafe {
			gl_display.create_window_surface(&gl_config, &surface_attributes)
		}.map_err(|error| format!("서피스 생성 실패: {}", error))?;
		let context = not_current_context.make_current(&surface).map_err(|error| format!("컨텍스트 활성화 실패: {}", error))?;

		let use_vertical_sync = engine_configuration.use_vertical_sync;
		let swap_interval = if use_vertical_sync {
			SwapInterval::Wait(NonZeroU32::new(1).expect("one"))
		}
		else {
			SwapInterval::DontWait
		};
		if let Err(error) = surface.set_swap_interval(&context, swap_interval) {
			eprintln!("Engine: 수직 동기 설정 실패: {}", error);
		}

		let gl = unsafe {
			glow::Context::from_loader_function_cstr(|symbol_name| gl_display.get_proc_address(symbol_name) as *const _)
		};
		let graphic = Graphic::new(gl)?;

		#[cfg(target_os = "windows")]
		{
			windowsplatform::install_power_broadcast_hook(&window);
		}
		let window = Rc::new(window);
		self.engine_runner.attach_graphic(graphic);
		self.engine_runner.engine.platform_window = Some(window.clone());
		self.window_state = Some(WindowState {
			window,
			surface,
			context,
		});
		self.resize();
		Ok(())
	}

	//==============================================================================
	// 해상도 변경됨. (서피스 크기를 맞추고 뷰 영역을 다시 계산한 뒤 씬에 알린다)
	//==============================================================================
	fn resize(&mut self) {
		let window_state = match &self.window_state {
			Some(window_state) => {
				window_state
			}
			None => {
				return;
			}
		};
		let physical_size = window_state.window.inner_size();
		let scale_factor = window_state.window.scale_factor() as f32;
		if let (Some(width), Some(height)) = (NonZeroU32::new(physical_size.width), NonZeroU32::new(physical_size.height)) {
			window_state.surface.resize(&window_state.context, width, height);
		}
		// 캔버스 비 상한이 있으면 원본처럼 캔버스를 줄여 창 가운데에 둔다. (논리 좌표, 내림)
		let client_width = physical_size.width as f32 / scale_factor;
		let client_height = physical_size.height as f32 / scale_factor;
		// 폰트 문자열의 뷰포트 단위(vw, vh)는 원본 window.innerWidth / innerHeight 처럼 창의 논리 크기에 댄다.
		crate::core::textstringtexturecache::set_font_viewport_size(client_width, client_height);
		let max_canvas_aspect_ratio = self.engine_runner.engine.get_engine_configuration().max_canvas_aspect_ratio;
		let (canvas_left, canvas_top, canvas_width, canvas_height) = if max_canvas_aspect_ratio > 0.0 {
			let canvas_width = client_width.min(client_height * max_canvas_aspect_ratio).floor();
			let canvas_height = client_height.min(client_width * max_canvas_aspect_ratio).floor();
			(((client_width - canvas_width) * 0.5).floor(), ((client_height - canvas_height) * 0.5).floor(), canvas_width, canvas_height)
		}
		else {
			(0.0, 0.0, client_width, client_height)
		};
		CANVAS_CLIENT_POSITION.with(|canvas_client_position| canvas_client_position.set(Vector2::create(canvas_left, canvas_top)));
		self.canvas_physical_rect = [(canvas_left * scale_factor).round() as i32, (canvas_top * scale_factor).round() as i32, (canvas_width * scale_factor).round() as i32, (canvas_height * scale_factor).round() as i32];
		// 렌더 배율 상한을 넘으면 그 배율로 줄인 크기로 그린다. (웹 캔버스 드로잉 버퍼와 같은 셈)
		let view_manager = self.engine_runner.engine.get_view_manager_mut();
		let render_pixel_ratio = view_manager.limit_render_pixel_ratio(scale_factor);
		let render_width = ((canvas_width * render_pixel_ratio).round() as u32).max(1);
		let render_height = ((canvas_height * render_pixel_ratio).round() as u32).max(1);
		view_manager.calculate_view_rect(render_width, render_height, render_pixel_ratio);
		self.applied_max_render_pixel_ratio = view_manager.get_max_render_pixel_ratio();
		self.window_scale_factor = scale_factor;
		self.engine_runner.notify_scene_resize();
	}

	//==============================================================================
	// 페이지 가려짐 알리기. (가려짐, 최소화, 앱 멈춤 가운데 하나라도 맞으면 Hidden, 모두 풀리면 Visible 과 on_page_visibility_restored, 바뀔 때만)
	//==============================================================================
	fn update_page_visibility(&mut self) {
		let is_page_hidden = self.is_occluded || self.is_minimized || self.is_suspended;
		if is_page_hidden == self.is_page_hidden {
			return;
		}
		self.is_page_hidden = is_page_hidden;
		if is_page_hidden {
			self.engine_runner.engine.dispatch_page_event(PageEvent::Hidden);
		}
		else {
			self.engine_runner.engine.dispatch_page_event(PageEvent::Visible);
			self.engine_runner.notify_page_visibility_restored();
		}
	}

	//==============================================================================
	// 시스템 잠자기에서 깼는지 보고 알리기. (윈도우는 WM_POWERBROADCAST 의 깸 알림을 받았으면)
	//==============================================================================
	#[cfg(target_os = "windows")]
	fn check_system_sleep(&mut self) {
		if windowsplatform::take_system_resumed() {
			self.engine_runner.notify_page_process_restored();
		}
	}

	//==============================================================================
	// 시스템 잠자기에서 깼는지 보고 알리기. (맥과 리눅스는 두 프레임 사이에 벽시계가 단조 시계보다 SYSTEM_SLEEP_GAP_SECONDS 넘게 더 흘렀으면)
	//==============================================================================
	#[cfg(not(target_os = "windows"))]
	fn check_system_sleep(&mut self) {
		let now_instant = Instant::now();
		let now_system_time = SystemTime::now();
		if let Some((last_instant, last_system_time)) = self.last_frame_clock {
			let monotonic_seconds = now_instant.duration_since(last_instant).as_secs_f64();
			let wall_seconds = now_system_time.duration_since(last_system_time).map(|duration| duration.as_secs_f64()).unwrap_or(0.0);
			if wall_seconds - monotonic_seconds > SYSTEM_SLEEP_GAP_SECONDS {
				self.engine_runner.notify_page_process_restored();
			}
		}
		self.last_frame_clock = Some((now_instant, now_system_time));
	}

	//==============================================================================
	// 창 안쪽 물리 좌표 → 입력 위치 갱신.
	//==============================================================================
	fn update_input_position(&mut self, physical_x: f64, physical_y: f64) {
		let window_scale_factor = self.window_scale_factor;
		let canvas_client_position = read_canvas_client_position();
		let engine = &mut self.engine_runner.engine;
		let canvas_native_input_position = Vector2::create(physical_x as f32 / window_scale_factor - canvas_client_position.x, physical_y as f32 / window_scale_factor - canvas_client_position.y);
		let view_input_position = engine.get_view_manager().canvas_position_to_view_position(canvas_native_input_position);
		let input_manager = engine.get_input_manager_mut();
		input_manager.set_canvas_native_input_position(canvas_native_input_position);
		input_manager.set_view_input_position(view_input_position);
	}

	//==============================================================================
	// 손가락. (손가락마다 닿음, 움직임, 뗌을 적고, 첫 손가락은 마우스 왼쪽 단추처럼 터치로 넘긴다)
	//==============================================================================
	fn handle_touch(&mut self, touch: winit::event::Touch) {
		let window_scale_factor = self.window_scale_factor;
		let canvas_client_position = read_canvas_client_position();
		let canvas_native_position = Vector2::create(touch.location.x as f32 / window_scale_factor - canvas_client_position.x, touch.location.y as f32 / window_scale_factor - canvas_client_position.y);
		let view_position = self.engine_runner.engine.get_view_manager().canvas_position_to_view_position(canvas_native_position);
		let identifier = touch.id as i64;
		let touch_points = self.engine_runner.engine.get_input_manager().get_touch_points();
		let is_first_finger = touch_points.is_empty() || touch_points[0].identifier == identifier;
		if is_first_finger {
			self.update_input_position(touch.location.x, touch.location.y);
		}
		let input_manager = self.engine_runner.engine.get_input_manager_mut();
		match touch.phase {
			TouchPhase::Started => {
				input_manager.begin_touch_point(identifier, canvas_native_position, view_position);
			}
			TouchPhase::Moved => {
				input_manager.move_touch_point(identifier, canvas_native_position, view_position);
			}
			TouchPhase::Ended => {
				input_manager.end_touch_point(identifier, canvas_native_position, view_position, false);
			}
			TouchPhase::Cancelled => {
				input_manager.end_touch_point(identifier, canvas_native_position, view_position, true);
			}
		}
		input_manager.set_touch_input(true);
		input_manager.set_last_input_device(InputDevice::Touch);
		if !is_first_finger {
			return;
		}
		match touch.phase {
			TouchPhase::Started => {
				input_manager.set_touch_pressed(true);
				input_manager.set_touch_moved(true);
				input_manager.set_touch_down(true);
			}
			TouchPhase::Moved => {
			}
			TouchPhase::Ended => {
				input_manager.set_touch_moved(false);
				input_manager.set_touch_released(true);
				input_manager.set_touch_down(false);
			}
			TouchPhase::Cancelled => {
				input_manager.set_touch_moved(false);
				input_manager.set_touch_cancelled(true);
				input_manager.set_touch_down(false);
			}
		}
	}

	//==============================================================================
	// 한 프레임 돌고 화면 넘기기.
	//==============================================================================
	fn update_frame(&mut self) {
		if self.window_state.is_none() {
			return;
		}
		self.check_system_sleep();
		let max_render_pixel_ratio = self.engine_runner.engine.get_view_manager().get_max_render_pixel_ratio();
		if max_render_pixel_ratio != self.applied_max_render_pixel_ratio || self.engine_runner.engine.is_resize_requested {
			self.engine_runner.engine.is_resize_requested = false;
			self.resize();
		}
		self.engine_runner.update_engine();
		self.present_render_scaled();
		if let Some(window_state) = &self.window_state {
			self.text_input_state.apply_active_field(&window_state.window, self.engine_runner.engine.get_input_manager().get_text_input());
			if let Err(error) = window_state.surface.swap_buffers(&window_state.context) {
				eprintln!("Engine: 화면 넘기기 실패: {}", error);
			}
		}
	}
}

impl DesktopApplication {
	//==============================================================================
	// 부탁받은 그림 커서를 만들어 걸기. (그림 파일을 애셋 폴더에서 읽어 winit 커서로 만든다, 못 만들면 CSS 커서를 그대로 둔다)
	//==============================================================================
	fn apply_pending_custom_cursor(&mut self, event_loop: &ActiveEventLoop) {
		let cursor_value = match PENDING_CUSTOM_CURSOR.with(|pending_custom_cursor| pending_custom_cursor.borrow_mut().take()) {
			Some(cursor_value) => {
				cursor_value
			}
			None => {
				return;
			}
		};
		let window = match &self.window_state {
			Some(window_state) => {
				window_state.window.clone()
			}
			None => {
				return;
			}
		};
		let scale_factor = window.scale_factor();
		let custom_cursor_key = create_custom_cursor_key(&cursor_value, scale_factor);
		let engine = &self.engine_runner.engine;
		let image_path = engine.resolve_asset_path(&cursor_value.image_url);
		let image_path_2x = if cursor_value.image_url_2x.is_empty() {
			None
		}
		else {
			Some(engine.resolve_asset_path(&cursor_value.image_url_2x))
		};
		let custom_cursor = match create_custom_cursor_source(&image_path, image_path_2x.as_deref(), cursor_value.hotspot_x, cursor_value.hotspot_y, scale_factor) {
			Ok(custom_cursor_source) => {
				Some(event_loop.create_custom_cursor(custom_cursor_source))
			}
			Err(error) => {
				eprintln!("Engine: 그림 커서를 만들지 못했습니다: {}", error);
				None
			}
		};
		CUSTOM_CURSOR_TABLE.with(|custom_cursor_table| {
			custom_cursor_table.borrow_mut().insert(custom_cursor_key, custom_cursor.clone());
		});
		if let Some(custom_cursor) = custom_cursor {
			window.set_cursor_visible(true);
			window.set_cursor(custom_cursor);
		}
	}

	//==============================================================================
	// 줄인 렌더 크기로 그렸거나 캔버스를 가운데에 두었으면 창의 캔버스 자리로 늘여 그리기. (렌더 크기가 창 크기와 같으면 하지 않는다)
	//==============================================================================
	fn present_render_scaled(&mut self) {
		let window_size = match &self.window_state {
			Some(window_state) => {
				window_state.window.inner_size()
			}
			None => {
				return;
			}
		};
		let canvas_pixel_size = self.engine_runner.engine.get_view_manager().get_canvas_pixel_size();
		let render_width = canvas_pixel_size.x as i32;
		let render_height = canvas_pixel_size.y as i32;
		let window_width = window_size.width as i32;
		let window_height = window_size.height as i32;
		if render_width <= 0 || render_height <= 0 || (render_width == window_width && render_height == window_height) {
			return;
		}
		// GL 은 왼쪽 아래 원점이라 위에서 잰 캔버스 자리를 뒤집는다.
		let canvas_physical_rect = self.canvas_physical_rect;
		let destination_rect = [canvas_physical_rect[0], window_height - canvas_physical_rect[1] - canvas_physical_rect[3], canvas_physical_rect[2], canvas_physical_rect[3]];
		let graphic = match self.engine_runner.get_graphic() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let gl = graphic.get_gl();
		if self.render_scaler.is_none() {
			match renderscaler::RenderScaler::new(gl) {
				Ok(render_scaler) => {
					self.render_scaler = Some(render_scaler);
				}
				Err(error) => {
					eprintln!("Engine: 렌더 배율 늘이개 생성 실패: {}", error);
					return;
				}
			}
		}
		if let Some(render_scaler) = &self.render_scaler {
			render_scaler.present(gl, render_width, render_height, destination_rect, window_width, window_height);
		}
	}
}

impl ApplicationHandler for DesktopApplication {
	//==============================================================================
	// 창 만들 차례. (데스크탑에서는 시작할 때 한 번)
	//==============================================================================
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if self.window_state.is_some() {
			if self.is_suspended {
				self.is_suspended = false;
				self.update_page_visibility();
				self.engine_runner.notify_page_process_restored();
			}
			return;
		}
		if let Err(error) = self.create_window(event_loop) {
			eprintln!("Engine: {}", error);
			self.fatal_error = Some(error);
			event_loop.exit();
			return;
		}
		self.engine_runner.apply_pending_scenes();
	}

	//==============================================================================
	// 앱이 멈춤. (모바일의 앱 전환, 윈도우, 맥, 리눅스에서는 오지 않는다)
	//==============================================================================
	fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
		self.is_suspended = true;
		self.update_page_visibility();
	}

	//==============================================================================
	// 창 이벤트.
	//==============================================================================
	fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, window_event: WindowEvent) {
		// 마우스 단추와 손가락 누름은 페이지 이벤트로도 알린다. (원본 window 의 pointerdown)
		let is_pointer_down = matches!(window_event, WindowEvent::MouseInput { state: ElementState::Pressed, .. } | WindowEvent::Touch(winit::event::Touch { phase: TouchPhase::Started, .. }));
		if is_pointer_down {
			self.engine_runner.engine.dispatch_page_event(PageEvent::PointerDown);
		}
		match window_event {
			WindowEvent::CloseRequested => {
				event_loop.exit();
			}
			WindowEvent::Resized(physical_size) => {
				let is_window_minimized = self.window_state.as_ref().and_then(|window_state| window_state.window.is_minimized()).unwrap_or(false);
				self.is_minimized = physical_size.width == 0 || physical_size.height == 0 || is_window_minimized;
				self.update_page_visibility();
				self.resize();
			}
			WindowEvent::ScaleFactorChanged { .. } => {
				self.resize();
			}
			WindowEvent::Focused(is_focused) => {
				// 원본처럼 창이 포커스를 잃으면 누르던 것을 뗀 것으로 넘긴다.
				if !is_focused {
					let input_manager = self.engine_runner.engine.get_input_manager_mut();
					input_manager.clear();
					input_manager.set_touch_released(true);
				}
				let page_event = if is_focused {
					PageEvent::Focused
				}
				else {
					PageEvent::Blurred
				};
				self.engine_runner.engine.dispatch_page_event(page_event);
			}
			WindowEvent::Occluded(is_occluded) => {
				if self.is_window_hidden {
					return;
				}
				self.is_occluded = is_occluded;
				self.update_page_visibility();
			}

			WindowEvent::KeyboardInput { event: key_event, .. } => {
				if let PhysicalKey::Code(key_code) = key_event.physical_key {
					let key = convert_key_code_to_code_name(key_code);
					let input_manager = self.engine_runner.engine.get_input_manager_mut();
					match key_event.state {
						ElementState::Pressed => {
							input_manager.push_key(&key);
							input_manager.set_last_input_device(InputDevice::Keyboard);
						}
						ElementState::Released => {
							input_manager.pop_key(&key);
						}
					}
				}
				self.text_input_state.handle_key_event(&key_event, self.engine_runner.engine.get_input_manager_mut().get_text_input_mut());
			}
			WindowEvent::ModifiersChanged(modifiers) => {
				self.text_input_state.set_modifiers(modifiers.state());
			}
			WindowEvent::Ime(ime) => {
				self.text_input_state.handle_ime_event(ime, self.engine_runner.engine.get_input_manager_mut().get_text_input_mut());
			}
			WindowEvent::CursorMoved { position, .. } => {
				self.update_input_position(position.x, position.y);
				let input_manager = self.engine_runner.engine.get_input_manager_mut();
				input_manager.set_touch_input(false);
				input_manager.set_last_input_device(InputDevice::Mouse);
			}
			WindowEvent::MouseInput { state, button, .. } => {
				let engine_mouse_button = match button {
					MouseButton::Left => {
						EngineMouseButton::Left
					}
					MouseButton::Right => {
						EngineMouseButton::Right
					}
					MouseButton::Middle => {
						EngineMouseButton::Middle
					}
					_ => {
						return;
					}
				};
				let input_manager = self.engine_runner.engine.get_input_manager_mut();
				input_manager.set_mouse_button_down(engine_mouse_button, state == ElementState::Pressed);
				input_manager.set_touch_input(false);
				input_manager.set_last_input_device(InputDevice::Mouse);
				// 원본처럼 어느 단추든 터치다. (set_touch_from_left_button_only 면 왼쪽만)
				if input_manager.is_touch_button(engine_mouse_button) {
					match state {
						ElementState::Pressed => {
							input_manager.set_touch_pressed(true);
							input_manager.set_touch_moved(true);
							input_manager.set_touch_down(true);
						}
						ElementState::Released => {
							if input_manager.is_touch_down() {
								input_manager.set_touch_moved(false);
								input_manager.set_touch_released(true);
								input_manager.set_touch_down(false);
							}
						}
					}
				}
				// 오른쪽 단추는 입력 필드 문맥 메뉴 요청이다.
				if engine_mouse_button == EngineMouseButton::Right && state == ElementState::Pressed {
					let view_input_position = input_manager.get_view_input_position();
					input_manager.get_text_input_mut().set_context_menu_request_position(Some(view_input_position));
				}
			}
			WindowEvent::Touch(touch) => {
				self.handle_touch(touch);
			}
			WindowEvent::MouseWheel { delta, .. } => {
				let input_manager = self.engine_runner.engine.get_input_manager_mut();
				let is_ctrl_pressed = input_manager.is_key_pressed("ControlLeft") || input_manager.is_key_pressed("ControlRight");
				input_manager.set_wheel_with_ctrl(is_ctrl_pressed);
				input_manager.set_last_input_device(InputDevice::Mouse);
				match delta {
					MouseScrollDelta::LineDelta(line_x, line_y) => {
						input_manager.add_wheel_delta(-line_x * WHEEL_LINE_PIXEL_SIZE, -line_y * WHEEL_LINE_PIXEL_SIZE);
					}
					MouseScrollDelta::PixelDelta(pixel_delta) => {
						input_manager.add_wheel_delta(-pixel_delta.x as f32, -pixel_delta.y as f32);
					}
				}
			}
			WindowEvent::RedrawRequested => {
				self.update_frame();
				self.apply_pending_custom_cursor(event_loop);
				if self.engine_runner.engine.is_exit_requested() {
					event_loop.exit();
				}
			}
			_ => {
			}
		}
	}

	//==============================================================================
	// 이벤트를 다 처리한 뒤. (다음 프레임 요청, 최대 FPS 제한)
	//==============================================================================
	fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
		if self.engine_runner.engine.is_exit_requested() {
			event_loop.exit();
			return;
		}
		let window_state = match &self.window_state {
			Some(window_state) => {
				window_state
			}
			None => {
				return;
			}
		};
		// 숨긴 창은 다시 그리기 요청이 오지 않고 수직 동기로 기다리지도 않는다.
		// 그래서 여기서 바로 프레임을 돌고, 최대 FPS 가 없으면 모니터 주사율(모르면 60)로 보일 때와 같은 빠르기를 맞춘다.
		let mut maximum_frame_per_second = self.engine_runner.engine.get_engine_configuration().maximum_frame_per_second;
		if maximum_frame_per_second <= 0.0 {
			if !self.is_window_hidden {
				event_loop.set_control_flow(ControlFlow::Poll);
				window_state.window.request_redraw();
				return;
			}
			let refresh_rate_millihertz = window_state.window.current_monitor().and_then(|monitor| monitor.refresh_rate_millihertz()).unwrap_or(60000);
			maximum_frame_per_second = refresh_rate_millihertz as f32 / 1000.0;
		}

		// 차례가 된 프레임만 그린다. (한 프레임 넘게 밀렸으면 지금부터 다시 센다)
		let frame_interval = Duration::from_secs_f64(1.0 / maximum_frame_per_second as f64);
		let now = Instant::now();
		if now >= self.next_frame_instant {
			self.next_frame_instant += frame_interval;
			if self.next_frame_instant + frame_interval < now {
				self.next_frame_instant = now + frame_interval;
			}
			if self.is_window_hidden {
				self.update_frame();
				if self.engine_runner.engine.is_exit_requested() {
					event_loop.exit();
					return;
				}
			}
			else {
				window_state.window.request_redraw();
			}
		}
		event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_instant));
	}
}

//==============================================================================
// 그림 커서 열쇠. (그림 주소, 2 배 그림 주소, 핫스팟, 창 배율이 같으면 같은 커서)
//==============================================================================
fn create_custom_cursor_key(cursor_value: &CursorValue, scale_factor: f64) -> String {
	format!("{}|{}|{}|{}|{}", cursor_value.image_url, cursor_value.image_url_2x, cursor_value.hotspot_x, cursor_value.hotspot_y, scale_factor)
}

//==============================================================================
// 그림 파일로 winit 커서 원본 만들기. (1 배 그림 기준 핫스팟)
// - 맥: winit 이 그림의 픽셀 수를 그대로 포인트 크기로 쓰므로 1 배 그림 그대로.
// - 윈도우, 리눅스: 그림의 픽셀 수가 화면 픽셀 수라 창 배율만큼 늘인다. 배율이 1 보다 크고 2 배 그림이 있으면 그것을 맞춰 쓰고 핫스팟도 배율만큼 옮긴다.
//   (브라우저가 그림 커서를 기기 배율로 늘이는 것과 같은 크기)
//==============================================================================
fn create_custom_cursor_source(image_path: &std::path::Path, image_path_2x: Option<&std::path::Path>, hotspot_x: f32, hotspot_y: f32, scale_factor: f64) -> Result<winit::window::CustomCursorSource, String> {
	let cursor_scale = if cfg!(target_os = "macos") {
		1.0
	}
	else {
		scale_factor.max(1.0)
	};
	let (source_path, source_scale) = match image_path_2x {
		Some(image_path_2x) if cursor_scale > 1.0 => {
			(image_path_2x, 2.0)
		}
		_ => {
			(image_path, 1.0)
		}
	};
	let image_bytes = std::fs::read(source_path).map_err(|error| format!("{} ({})", error, source_path.display()))?;
	crate::base::platform::record_loaded_file(&source_path.to_string_lossy(), image_bytes.len() as u64);
	let source_image = image::load_from_memory(&image_bytes).map_err(|error| format!("{} ({})", error, source_path.display()))?.to_rgba8();
	let base_width = source_image.width() as f64 / source_scale;
	let base_height = source_image.height() as f64 / source_scale;
	let cursor_width = ((base_width * cursor_scale).round() as u32).max(1);
	let cursor_height = ((base_height * cursor_scale).round() as u32).max(1);
	let cursor_image = if cursor_width == source_image.width() && cursor_height == source_image.height() {
		source_image
	}
	else {
		image::imageops::resize(&source_image, cursor_width, cursor_height, image::imageops::FilterType::Triangle)
	};
	let cursor_hotspot_x = ((hotspot_x as f64 * cursor_scale).round().max(0.0) as u32).min(cursor_width - 1);
	let cursor_hotspot_y = ((hotspot_y as f64 * cursor_scale).round().max(0.0) as u32).min(cursor_height - 1);
	CustomCursor::from_rgba(cursor_image.into_raw(), cursor_width as u16, cursor_height as u16, cursor_hotspot_x as u16, cursor_hotspot_y as u16).map_err(|error| error.to_string())
}

//==============================================================================
// winit 키를 원본 KeyboardEvent.code 이름으로. (winit 이름이 W3C 와 같아 그대로 쓰고, 다른 셋만 바꾼다)
// - 윈도우 키(커맨드 키): winit SuperLeft, SuperRight → W3C MetaLeft, MetaRight
// - W3C 의 옛 Super 키: winit Meta → Super
//==============================================================================
fn convert_key_code_to_code_name(key_code: winit::keyboard::KeyCode) -> String {
	match key_code {
		winit::keyboard::KeyCode::SuperLeft => {
			String::from("MetaLeft")
		}
		winit::keyboard::KeyCode::SuperRight => {
			String::from("MetaRight")
		}
		winit::keyboard::KeyCode::Meta => {
			String::from("Super")
		}
		_ => {
			format!("{:?}", key_code)
		}
	}
}

//==============================================================================
// 창을 숨기고 돌지. (찍기 모드 VANILLA_CAPTURE_PATH 면 숨긴다. VANILLA_CAPTURE_SHOW_WINDOW=1 이면 보인다)
// - 검증 도구가 예제를 찍을 때 사용자 화면에 창이 뜨지 않게 한다. 맥은 독 아이콘과 포커스도 가져가지 않는다.
//==============================================================================
fn is_capture_window_hidden() -> bool {
	if std::env::var("VANILLA_CAPTURE_PATH").is_err() {
		return false;
	}
	std::env::var("VANILLA_CAPTURE_SHOW_WINDOW").map(|value| value != "1").unwrap_or(true)
}

//==============================================================================
// GL 설정 고르기. (스텐실 8 비트 이상, 멀티샘플은 4 에 가까운 것)
//==============================================================================
fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
	let config_list: Vec<Config> = configs.collect();
	let mut best_config: Option<Config> = None;
	for config in config_list.iter() {
		if config.stencil_size() < 8 {
			continue;
		}
		let sample_count = config.num_samples();
		let is_better = match &best_config {
			None => {
				true
			}
			Some(current_config) => {
				let current_sample_count = current_config.num_samples();
				if sample_count <= 4 {
					current_sample_count > 4 || sample_count > current_sample_count
				}
				else {
					current_sample_count > 4 && sample_count < current_sample_count
				}
			}
		};
		if is_better {
			best_config = Some(config.clone());
		}
	}
	match best_config {
		Some(best_config) => {
			best_config
		}
		None => {
			config_list.into_iter().next().expect("GL 설정이 하나도 없습니다")
		}
	}
}

//==============================================================================
// 창 아이콘 읽기. (PNG, 스트레이트 알파)
//==============================================================================
fn load_window_icon(icon_path: &std::path::Path) -> Result<Icon, String> {
	let icon_bytes = std::fs::read(icon_path).map_err(|error| format!("창 아이콘 읽기 실패 ({}): {}", icon_path.display(), error))?;
	let icon_image = image::load_from_memory(&icon_bytes).map_err(|error| format!("창 아이콘 해석 실패: {}", error))?.to_rgba8();
	let icon_width = icon_image.width();
	let icon_height = icon_image.height();
	Icon::from_rgba(icon_image.into_raw(), icon_width, icon_height).map_err(|error| format!("창 아이콘 생성 실패: {}", error))
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 그림 커서 원본은 그림 파일로 만들고(창 배율과 2 배 그림, 핫스팟은 그림 안으로), 없는 파일은 오류다.
	//==============================================================================
	#[test]
	fn custom_cursor_source_from_image_file() {
		let image_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/atlas/icons.png");
		assert!(create_custom_cursor_source(&image_path, None, 4.0, 4.0, 1.0).is_ok());
		assert!(create_custom_cursor_source(&image_path, Some(&image_path), 4000.0, 4.0, 1.5).is_ok());
		let missing_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/missing_cursor.png");
		assert!(create_custom_cursor_source(&missing_path, None, 0.0, 0.0, 1.0).is_err());
		let cursor_value = CursorValue {
			css_value: String::new(),
			fallback_name: String::from("pointer"),
			image_url: String::from("cursor.png"),
			image_url_2x: String::new(),
			hotspot_x: 1.0,
			hotspot_y: 2.0,
		};
		assert_ne!(create_custom_cursor_key(&cursor_value, 1.0), create_custom_cursor_key(&cursor_value, 2.0));
	}

	//==============================================================================
	// winit 키는 W3C code 이름으로 넣는다. (윈도우 키는 MetaLeft, MetaRight)
	//==============================================================================
	#[test]
	fn key_code_becomes_code_name() {
		assert_eq!(convert_key_code_to_code_name(winit::keyboard::KeyCode::SuperLeft), crate::core::inputmanager::KeyCode::META);
		assert_eq!(convert_key_code_to_code_name(winit::keyboard::KeyCode::SuperRight), "MetaRight");
		assert_eq!(convert_key_code_to_code_name(winit::keyboard::KeyCode::NumpadAdd), crate::core::inputmanager::KeyCode::NUMPAD_ADD);
		assert_eq!(convert_key_code_to_code_name(winit::keyboard::KeyCode::Backquote), crate::core::inputmanager::KeyCode::BACKQUOTE);
		assert_eq!(convert_key_code_to_code_name(winit::keyboard::KeyCode::AudioVolumeUp), crate::core::inputmanager::KeyCode::AUDIOVOLUMEUP);
		assert_eq!(convert_key_code_to_code_name(winit::keyboard::KeyCode::AudioVolumeMute), crate::core::inputmanager::KeyCode::AUDIOMUTE);
		assert_eq!(convert_key_code_to_code_name(winit::keyboard::KeyCode::MediaPlayPause), crate::core::inputmanager::KeyCode::AUDIOPLAY);
	}

	//==============================================================================
	// 쿼리 이름은 VANILLA_ 뒤 대문자 뱀꼴 환경 변수로 읽는다.
	//==============================================================================
	#[test]
	fn query_name_becomes_variable_name() {
		assert_eq!(convert_query_name_to_variable_name("safeArea"), "VANILLA_SAFE_AREA");
		assert_eq!(convert_query_name_to_variable_name("quality"), "VANILLA_QUALITY");
	}
}
