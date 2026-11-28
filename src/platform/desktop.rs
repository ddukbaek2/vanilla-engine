//==============================================================================
//! 데스크탑 플랫폼. (윈도우, 맥, 리눅스)
//! - 창(winit)과 GL 3.3 코어 컨텍스트(glutin)를 만들고, 창의 다시 그리기 요청마다 엔진 프레임을 하나 돈다.
//! - 창 이벤트를 입력 관리자 꼴로 바꿔 넣는다. (키는 W3C KeyboardEvent.code 와 같은 이름, 마우스 단추는 원본처럼 어느 것이든 터치, 왼쪽만으로 바꿀 수 있다)
//! - 엔진 본체(EngineRunner)는 창을 모른다. 여기서 프레임을 부르고, 그린 뒤 화면을 넘긴다.
//! - 글 입력(키 글자, 편집 키, IME 조합)과 클립보드는 desktop/textinput.rs 가 맡는다. 오른쪽 단추는 입력 필드 문맥 메뉴 요청이다.
//! - 맥의 게임패드 진동은 desktop/gamepadhaptics.rs 가 맡는다. (GameController 와 CoreHaptics, 다른 OS 는 gilrs 가 떤다)
//! - 마지막으로 쓴 장치를 키, 마우스, 손가락 이벤트마다 적는다. 막을 키(set_prevented_keys)와 오른쪽 클릭 메뉴 막기는 창에 막을 브라우저 기본 동작이 없어 쓰지 않는다.
//!   (크롬에서 막은 것과 같은 결과다. winit 은 윈도우에서 메뉴가 없는 창의 WM_SYSKEYUP 을 DefWindowProc 에 넘기지 않아 Alt, F10 도 시스템 메뉴로 들어가지 않는다)
//! - 페이지 이벤트: 창이 가려짐(Occluded, 맥과 X11 이 알린다), 최소화(크기 0 이나 is_minimized, 윈도우가 이것으로 알린다), 앱 멈춤(suspended, 모바일),
//!   웨이랜드 프레임 콜백이 3 초 넘게 오지 않음(크롬 리눅스 웨이랜드의 가려짐, desktop/waylandframecallback.rs)
//!   가운데 하나라도 맞으면 Hidden, 모두 풀리면 Visible 과 on_page_visibility_restored. 포커스는 Focused / Blurred, 마우스 단추와 손가락 누름은 PointerDown.
//!   웨이랜드의 xdg_toplevel suspended 상태는 winit 이 쥐고 알리지 않아 받지 못한다.
//! - 웨이랜드 수직 동기: EGL 스왑 간격을 0 으로 두고 넘기기 전에 지난 프레임 콜백을 1/20 초까지 기다린다. (메사는 간격 1 이면 가려진 창에서 스왑 안에 멈춘다)
//! - 캔버스 크기(auto_resize_on_window_resize): 창은 캔버스가 창을 가득 채운 페이지와 같다. 켜면 원본처럼 visualViewport(창에서 화면 키보드가 가린 아래를 뺀 것)에
//!   맞추고 ignore_virtual_keyboard_resize 와 캔버스 비 상한을 건다. 끄면 창 크기 그대로다.
//! - 화면 키보드: 윈도우는 크롬처럼 손가락으로 플랫폼 입력 필드에 포커스하면 터치 키보드(InputPane)를 띄우고 가린 영역을 visualViewport 에서 뺀다. (desktop/windowsinputpane.rs)
//!   맥과 리눅스는 크롬처럼 뷰포트를 줄이지 않는다.
//! - 드로잉 버퍼 남기기(preserve_drawing_buffer): 다 그린 프레임을 떠 두고 다음 프레임 첫머리에 되그린다. (desktop/preserveddrawingbuffer.rs)
//! - 페이지 프로세스 복원(on_page_process_restored, 원본 Chromium freeze 뒤 resume 과 bfcache): suspended 뒤 resumed, 그리고 시스템 잠자기에서 깬 것.
//!   윈도우는 창을 서브클래싱해 WM_POWERBROADCAST 의 PBT_APMRESUMEAUTOMATIC 을 받는다. (desktop/windowsplatform.rs, 윈도우의 단조 시계는 잠든 동안에도 흘러 시계로는 못 가린다)
//!   맥과 리눅스는 두 프레임 사이에 벽시계(SystemTime)가 단조 시계(Instant)보다 SYSTEM_SLEEP_GAP_SECONDS 넘게 더 흘렀으면 깬 것으로 본다.
//!   (러스트 Instant 는 맥에서 CLOCK_UPTIME_RAW, 리눅스에서 CLOCK_MONOTONIC 이라 잠든 동안 멈춘다. 러스트는 이것을 보장하지 않는다고 적어 두었다)
//! - 주된 손이 손가락인지(is_coarse_pointer): 윈도우는 크로미엄의 (pointer: coarse) 판정을 옮겨 태블릿 모드나 마우스 없는 터치 기기면 참이다.
//!   맥은 크로미엄도 늘 마우스로 보아 거짓이다. 리눅스와 그 밖의 유닉스는 크로미엄처럼 화면 서버(웨이랜드 wl_seat 능력, X11 XInput 장치 목록)에서 터치스크린과
//!   마우스, 터치패드를 읽어 터치스크린만 있으면 참이다. (desktop/linuxplatform.rs, 읽은 값은 2 초 동안 쓴다)
//! - 키 이름: winit 의 SuperLeft, SuperRight 는 W3C 의 MetaLeft, MetaRight, winit 의 Meta 는 W3C 의 Super 로 바꿔 넣는다.
//! - 그림 커서(PointerCursor.register_image_cursor): 그림 파일을 읽어 winit 의 CustomCursor 로 만든다. 맥은 그림 크기가 그대로 포인트 크기라 1 배 그림을,
//!   윈도우와 리눅스는 물리 픽셀이라 창 배율에 맞춰(2 배 그림이 있으면 그것을) 늘인 그림과 핫스팟을 쓴다. 만드는 동안과 못 만들면 함께 준 CSS 커서다.
//! - 렌더 배율 상한(ViewManager.set_max_render_pixel_ratio): 그 배율로 줄인 크기로 그리고 프레임 끝에 창 크기로 늘인다. (desktop/renderscaler.rs)
//!   입력 좌표는 창의 배율로 논리 좌표로 바꾸므로 렌더 크기와 상관없다.
//! - 캔버스 비 상한(EngineConfiguration.max_canvas_aspect_ratio): 창이 그보다 길쭉하면 캔버스(그리는 영역)를 원본처럼 줄여 창 가운데에 두고 나머지는 검다.
//!   입력 좌표에서 캔버스의 왼쪽 위를 빼고, read_canvas_client_position 이 그 자리를 돌려준다.
//! - 페이지 주소의 쿼리 값(read_page_query_value)은 환경 변수 VANILLA_ 뒤에 이름을 대문자 뱀꼴로 붙인 것으로 읽는다. (safeArea → VANILLA_SAFE_AREA)
//! - 찍기 모드(VANILLA_CAPTURE_PATH)는 창을 숨기고 돈다. 숨긴 창은 다시 그리기 요청과 수직 동기가 없어 about_to_wait 에서 프레임을 바로 돌고 주사율로 빠르기를 맞춘다.
//!   가려짐 알림은 받지 않는다. 맥은 독 아이콘과 포커스도 가져가지 않는다. 창을 보려면 VANILLA_CAPTURE_SHOW_WINDOW=1.
//!
//! # 예제
//! 페이지 쿼리 값, 세이프 에어리어, 주된 손 읽기 (데스크탑은 환경 변수, 0, 페이지 없음)
//! ```rust
//! use vanilla::platform;
//!
//! let quality = platform::read_page_query_value("quality").unwrap_or_else(|| String::from("high"));
//! let safe_area_insets = platform::read_safe_area_insets();
//! let is_touch_first = platform::is_coarse_pointer();
//! println!("{} {:?} {}", quality, safe_area_insets, is_touch_first);
//! assert_eq!(safe_area_insets, [0.0, 0.0, 0.0, 0.0]);
//! assert_eq!(platform::read_client_native_size(), None);
//! assert!(!platform::is_text_field_focused());
//! ```
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::{Engine, EngineRunner, PageEvent};
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
use winit::window::{Icon, Window, WindowAttributes, WindowId};

mod preserveddrawingbuffer;
mod renderscaler;
mod textinput;
#[cfg(target_os = "windows")]
mod windowsinputpane;
#[cfg(target_os = "windows")]
mod windowsplatform;
#[cfg(any(test, all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
mod linuxplatform;
#[cfg(any(test, all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
mod waylandframecallback;
pub use textinput::{read_clipboard_text, read_clipboard_text_with_callback, read_optional_clipboard_text_with_callback, write_clipboard_text};
#[cfg(target_os = "macos")]
pub mod gamepadhaptics;
pub mod systemfont;
#[cfg(target_os = "macos")]
pub mod coretextglyph;
#[cfg(target_os = "windows")]
pub mod directwriteglyph;
pub mod emojipanel;

/// 엔진이 들고 있는 창 손잡이. (커서, 제목을 바꿀 때)
pub type PlatformWindow = Rc<Window>;

/// 휠 한 줄을 픽셀로 볼 때의 값. (브라우저 deltaMode 줄 단위 환산과 같은 자리)
const WHEEL_LINE_PIXEL_SIZE: f64 = 40.0;
/// 두 프레임 사이에 벽시계가 단조 시계보다 이만큼 넘게 더 흐르면 시스템이 잠들었다 깬 것으로 본다. (초, 맥과 리눅스)
#[cfg(not(target_os = "windows"))]
const SYSTEM_SLEEP_GAP_SECONDS: f64 = 2.0;
/// 기본 프레임버퍼의 멀티샘플 표본 수. (크롬 WebGL antialias 의 표본 수 기본값, 블링크 WebglPreferences.msaa_sample_count)
const MULTISAMPLE_COUNT: u8 = 4;

thread_local! {
	/// 창 안에서 캔버스의 왼쪽 위. (논리 좌표, 캔버스 비 상한으로 가운데에 둘 때만 0 이 아니다)
	static CANVAS_CLIENT_POSITION: std::cell::Cell<Vector2> = const { std::cell::Cell::new(Vector2::create(0.0, 0.0)) };
	/// 만든 그림 커서. (열쇠는 그림 주소, 2 배 그림 주소, 핫스팟, 창 배율. 못 만든 것은 None 으로 두어 다시 읽지 않는다)
	static CUSTOM_CURSOR_TABLE: RefCell<HashMap<String, Option<CustomCursor>>> = RefCell::new(HashMap::new());
	/// 아직 만들지 않은 그림 커서. (set_cursor 가 넣고, 이벤트 루프가 프레임 뒤에 만들어 건다)
	static PENDING_CUSTOM_CURSOR: RefCell<Option<CursorValue>> = const { RefCell::new(None) };
}

//==============================================================================
/// 시작. (창을 닫거나 exit_application 을 부를 때까지 돌아오지 않는다)
///
/// # 인자
/// - `engine_runner`: 엔진 실행자. (Engine::run 이 엔진과 첫 씬으로 만들어 넘긴다)
///
/// # 돌려줌
/// 창이 닫히고 이벤트 루프가 끝나면 Ok. 이벤트 루프를 만들거나 돌리지 못했거나 창, GL 컨텍스트를 만들다 실패했으면 Err(까닭 글).
//==============================================================================
pub fn run(engine_runner: EngineRunner) -> Result<(), String> {
	let is_window_hidden = is_capture_window_hidden();
	// Engine::new 가 지어 둔 이벤트 루프와 숨긴 창이 있으면 그것을 쓴다. (이벤트 루프는 프로세스에 하나만 지을 수 있다)
	let (event_loop, prepared_window_state) = match PREPARED_DESKTOP.with(|prepared_desktop| prepared_desktop.borrow_mut().take()) {
		Some(prepared_desktop) => {
			let prepared_desktop = std::mem::ManuallyDrop::into_inner(prepared_desktop);
			(prepared_desktop.event_loop, prepared_desktop.window_state)
		}
		None => {
			(build_event_loop(is_window_hidden)?, None)
		}
	};
	event_loop.set_control_flow(ControlFlow::Poll);
	let mut desktop_application = DesktopApplication {
		engine_runner,
		window_state: None,
		prepared_window_state,
		next_frame_instant: Instant::now(),
		fatal_error: None,
		text_input_state: textinput::DesktopTextInputState::new(),
		is_suspended: false,
		is_occluded: false,
		is_minimized: false,
		is_frame_callback_occluded: false,
		is_page_hidden: false,
		is_window_hidden,
		window_scale_factor: 1.0,
		render_scaler: None,
		preserved_drawing_buffer: None,
		virtual_keyboard_inset: 0.0,
		applied_client_size: None,
		#[cfg(target_os = "windows")]
		virtual_keyboard_controller: None,
		#[cfg(target_os = "windows")]
		touch_started_position: None,
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
/// 이벤트 루프 짓기. (찍기 모드에서 숨긴 창이면 맥은 독 아이콘과 포커스를 가져가지 않는다)
//==============================================================================
fn build_event_loop(is_window_hidden: bool) -> Result<EventLoop<()>, String> {
	let mut event_loop_builder = EventLoop::builder();
	#[cfg(target_os = "macos")]
	if is_window_hidden {
		use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
		event_loop_builder.with_activation_policy(ActivationPolicy::Accessory).with_activate_ignoring_other_apps(false);
	}
	#[cfg(not(target_os = "macos"))]
	let _ = is_window_hidden;
	event_loop_builder.build().map_err(|error| format!("이벤트 루프 생성 실패: {}", error))
}

//==============================================================================
/// Engine::new 가 지어 run 에 넘기는 것. (주 스레드의 이벤트 루프와 숨긴 창, GL 컨텍스트)
//==============================================================================
struct PreparedDesktop {
	event_loop: EventLoop<()>,
	window_state: Option<WindowState>,
}

thread_local! {
	/// Engine::new 가 지은 이벤트 루프와 창. (run 이 꺼낸다. winit 이벤트 루프는 프로세스에 하나라 엔진을 버려도 남겨 다음 엔진이 쓴다)
	/// - run 없이 끝나는 프로세스(문서 예제 같은)에서 스레드 지역 값을 버릴 때 winit 과 AppKit 정리가 이미 버린 값을 만져 멈추므로 버리지 않는다. (ManuallyDrop, 프로세스가 끝나며 운영체제가 거둔다)
	static PREPARED_DESKTOP: RefCell<Option<std::mem::ManuallyDrop<PreparedDesktop>>> = const { RefCell::new(None) };
}

//==============================================================================
/// 엔진을 만들 때 그래픽 짓기. (원본 Engine 생성자의 Graphic, Engine::new 가 부른다)
/// - 주 스레드에서만 짓는다. 이벤트 루프를 짓고, 그 루프로 창을 숨긴 채 만들어 GL 컨텍스트와 Graphic 을 짓는다. run 은 같은 창을 보이고 그대로 그린다.
///   (창 없는 컨텍스트를 따로 짓고 넘기는 길보다 네 갈래가 같다: 윈도우 WGL 은 확장을 읽으려면 창이 있어야 하고, 리눅스 EGL 은 winit 연결과 같은 디스플레이여야 창에 붙는다)
/// - 주 스레드가 아니거나(단위 시험은 이름 붙은 스레드에서 돈다) 이벤트 루프나 GL 을 지을 수 없으면(화면 없는 CI) None 이고, run 이 지금처럼 창을 열 때 짓는다.
///
/// # 인자
/// - `engine`: 갓 만든 엔진. (설정과 애셋 자리를 읽고, 창 손잡이를 넣는다)
///
/// # 돌려줌
/// 지은 그래픽. (못 지으면 None)
//==============================================================================
pub(crate) fn prepare_graphic(engine: &Engine) -> Option<Graphic> {
	if std::thread::current().name() != Some("main") {
		return None;
	}
	let is_window_hidden = is_capture_window_hidden();
	PREPARED_DESKTOP.with(|prepared_desktop| {
		let mut prepared_desktop = prepared_desktop.borrow_mut();
		if prepared_desktop.is_none() {
			let event_loop = build_event_loop(is_window_hidden).ok()?;
			*prepared_desktop = Some(std::mem::ManuallyDrop::new(PreparedDesktop {
				event_loop,
				window_state: None,
			}));
		}
		let prepared_desktop = prepared_desktop.as_mut()?;
		match &prepared_desktop.window_state {
			// 앞서 버린 엔진이 지은 창이 남았으면 그 컨텍스트에 그래픽만 새로 짓는다.
			Some(window_state) => {
				let graphic = create_graphic(&window_state.context.display(), engine.get_engine_configuration().preserve_drawing_buffer, engine.get_color_space()).ok()?;
				*engine.platform_window.borrow_mut() = Some(window_state.window.clone());
				Some(graphic)
			}
			None => {
				let (window_state, graphic) = create_window_state(WindowEventLoop::EventLoop(&prepared_desktop.event_loop), engine, false).ok()?;
				*engine.platform_window.borrow_mut() = Some(window_state.window.clone());
				prepared_desktop.window_state = Some(window_state);
				Some(graphic)
			}
		}
	})
}

//==============================================================================
/// 창을 지을 이벤트 루프. (Engine::new 의 돌기 전 루프, run 안의 ActiveEventLoop)
//==============================================================================
enum WindowEventLoop<'a> {
	EventLoop(&'a EventLoop<()>),
	ActiveEventLoop(&'a ActiveEventLoop),
}

//==============================================================================
/// 창과 GL 컨텍스트, 그래픽 짓기. (Engine::new 는 숨긴 채, run 은 찍기 모드가 아니면 보이게)
//==============================================================================
fn create_window_state(window_event_loop: WindowEventLoop<'_>, engine: &Engine, is_visible: bool) -> Result<(WindowState, Graphic), String> {
	let engine_configuration = engine.get_engine_configuration();
	let window_size = if engine_configuration.window_size.x > 0.0 && engine_configuration.window_size.y > 0.0 {
		engine_configuration.window_size
	}
	else {
		engine_configuration.reference_resolution_size
	};
	let mut window_attributes = Window::default_attributes()
		.with_title(engine_configuration.title.clone())
		.with_inner_size(LogicalSize::new(window_size.x, window_size.y))
		.with_resizable(engine_configuration.is_resizable)
		.with_visible(is_visible)
		.with_active(is_visible);
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

	// 깊이 24 / 스텐실 8 비트에 멀티샘플 4 인 설정. (원본 WebGL 컨텍스트의 기본 depth: true 와 antialias: true, 깊이는 3D 가 쓴다)
	// - 크롬은 antialias 캔버스를 min(4, GL_MAX_SAMPLES) 표본의 MSAA 로 칠한다. (블링크 DrawingBuffer::Initialize 의 sample_count_, WebglPreferences.msaa_sample_count 기본 4)
	// - 멀티샘플을 묻지 않으면 맥 CGL 은 멀티샘플 없는 설정 하나만 주어 도형 가장자리가 계단이 된다. 그래서 먼저 4 를 묻고,
	//   4 이상인 설정이 없으면 묻지 않고 다시 골라 4 에 가까운 것을 쓴다. (크롬도 기기 최대가 4 보다 작으면 그 수로 칠한다)
	// - 엔진 설정 antialias 를 끄면(원본 컨텍스트 속성 antialias: false) 멀티샘플을 묻지 않고 표본이 가장 적은 설정을 고른다.
	let is_antialias = engine_configuration.antialias;
	let config_template_builder = ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(24).with_stencil_size(8);
	let build_display = |window_attributes: WindowAttributes, config_template_builder: ConfigTemplateBuilder| {
		let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));
		let picker = |configs: Box<dyn Iterator<Item = Config> + '_>| pick_config(configs, is_antialias);
		match window_event_loop {
			WindowEventLoop::EventLoop(event_loop) => {
				display_builder.build(event_loop, config_template_builder, picker)
			}
			WindowEventLoop::ActiveEventLoop(event_loop) => {
				display_builder.build(event_loop, config_template_builder, picker)
			}
		}
	};
	let multisampled_built = if is_antialias {
		build_display(window_attributes.clone(), config_template_builder.clone().with_multisampling(MULTISAMPLE_COUNT)).ok()
	}
	else {
		None
	};
	let (window, gl_config) = match multisampled_built {
		Some(built) => {
			built
		}
		None => {
			build_display(window_attributes, config_template_builder).map_err(|error| format!("GL 설정 고르기 실패: {}", error))?
		}
	};
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

	// 웨이랜드는 EGL 스왑 간격을 0 으로 두고 수직 동기를 프레임 콜백 맞추개가 맡는다. (메사가 가려진 창에서 스왑 안에 멈추지 않게)
	#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
	let wayland_frame_pacer = waylandframecallback::WaylandFramePacer::new(&window);
	#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
	let is_vertical_sync_paced = wayland_frame_pacer.is_some();
	#[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
	let is_vertical_sync_paced = false;
	let use_vertical_sync = engine_configuration.use_vertical_sync && !is_vertical_sync_paced;
	let swap_interval = if use_vertical_sync {
		SwapInterval::Wait(NonZeroU32::new(1).expect("one"))
	}
	else {
		SwapInterval::DontWait
	};
	if let Err(error) = surface.set_swap_interval(&context, swap_interval) {
		eprintln!("Engine: 수직 동기 설정 실패: {}", error);
	}
	let graphic = create_graphic(&gl_display, engine_configuration.preserve_drawing_buffer, engine_configuration.color_space)?;
	let window_state = WindowState {
		#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
		wayland_frame_pacer,
		window: Rc::new(window),
		surface,
		context,
	};
	Ok((window_state, graphic))
}

//==============================================================================
/// 지금 컨텍스트로 그래픽 짓기. (GL 3.3 코어가 아니면 실패)
//==============================================================================
fn create_graphic(gl_display: &glutin::display::Display, preserve_drawing_buffer: bool, color_space: crate::core::colorspace::ColorSpace) -> Result<Graphic, String> {
	let gl = unsafe {
		glow::Context::from_loader_function_cstr(|symbol_name| gl_display.get_proc_address(symbol_name) as *const _)
	};
	// 3.3 을 청해도 드라이버가 없는 윈도우는 GDI 의 GL 1.1 을 돌려준다. (WGL_ARB_create_context 가 없으면 glutin 이 판을 묻지 않은 컨텍스트를 만든다) 셰이더 함수가 없어 그리기 전에 알린다.
	let gl_version = glow::HasContext::version(&gl);
	if gl_version.is_embedded || (gl_version.major, gl_version.minor) < (3, 3) {
		return Err(format!("OpenGL 3.3 코어가 없다: {}.{} {} (그래픽 드라이버를 깔거나, GPU 가 없는 기계는 메사 llvmpipe 같은 소프트웨어 GL 을 둔다)", gl_version.major, gl_version.minor, gl_version.vendor_info));
	}
	// 찍기 모드는 GL 렌더러를 남긴다. (CI 기계가 하드웨어 GL 인지 소프트웨어 GL 인지 기록에서 가른다)
	if std::env::var("VANILLA_CAPTURE_PATH").is_ok() {
		let (renderer, version_text) = unsafe { (glow::HasContext::get_parameter_string(&gl, glow::RENDERER), glow::HasContext::get_parameter_string(&gl, glow::VERSION)) };
		eprintln!("Engine: 찍기 GL 렌더러 {}, 판 {}", renderer, version_text);
	}
	Graphic::new(gl, preserve_drawing_buffer, color_space)
}

//==============================================================================
/// 커서 보이기 설정.
///
/// # 인자
/// - `platform_window`: 창 손잡이. (None 이면 아무것도 하지 않는다)
/// - `is_visible`: 보이려면 true.
//==============================================================================
pub fn set_cursor_visible(platform_window: Option<&PlatformWindow>, is_visible: bool) {
	if let Some(window) = platform_window {
		window.set_cursor_visible(is_visible);
	}
}

//==============================================================================
/// 커서 모양 설정. (CSS 커서 이름을 winit 커서 아이콘으로, none 이면 감춤, 모르는 이름은 기본 화살표)
/// - 그림 커서는 만들어 둔 것이 있으면 그것을 건다. 아직 없으면 이벤트 루프가 만들도록 부탁해 두고 그동안은 함께 준 CSS 커서를 쓴다.
///
/// # 인자
/// - `platform_window`: 창 손잡이. (None 이면 아무것도 하지 않는다)
/// - `cursor_value`: 커서 값. (image_url 이 있으면 그림 커서, 아니면 fallback_name 의 CSS 커서 이름)
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
/// 주된 손이 손가락인지. (윈도우는 크로미엄처럼 태블릿 모드나 마우스 없는 터치 기기면 참, 리눅스는 크로미엄처럼 마우스, 터치패드 없이 터치스크린만 있으면 참, 맥은 마우스가 주된 손으로 본다)
///
/// # 돌려줌
/// 주된 손이 손가락이면 true. (맥은 늘 false)
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
/// 문서의 초점이 글자 입력칸에 있는지. (데스크탑에는 문서가 없어 늘 false, 엔진 입력 필드는 입력 매니저가 따로 본다)
///
/// # 돌려줌
/// 늘 false.
//==============================================================================
pub fn is_text_field_focused() -> bool {
	false
}

//==============================================================================
/// 페이지 주소의 쿼리 값. (데스크탑에는 주소가 없어 환경 변수 VANILLA_ 뒤에 이름을 대문자 뱀꼴로 붙인 것을 읽는다, 없으면 None)
/// - safeArea → VANILLA_SAFE_AREA, quality → VANILLA_QUALITY
///
/// # 인자
/// - `name`: 쿼리 이름. (낙타꼴, safeArea 같은)
///
/// # 돌려줌
/// 그 환경 변수의 값. 없거나 유니코드가 아니면 None.
//==============================================================================
pub fn read_page_query_value(name: &str) -> Option<String> {
	std::env::var(convert_query_name_to_variable_name(name)).ok()
}

//==============================================================================
/// 쿼리 이름을 환경 변수 이름으로. (첫 글자가 아닌 대문자 앞에 밑줄, 모두 대문자, 앞에 VANILLA_)
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
/// 화면(client) 좌표에서 캔버스의 왼쪽 위. (데스크탑은 창의 논리 좌표가 client 좌표다, 캔버스 비 상한으로 가운데에 둘 때만 0 이 아니다)
///
/// # 돌려줌
/// 창 안에서 캔버스의 왼쪽 위 자리. (논리 픽셀, 처음은 0, 0)
//==============================================================================
pub fn read_canvas_client_position() -> Vector2 {
	CANVAS_CLIENT_POSITION.with(|canvas_client_position| canvas_client_position.get())
}

//==============================================================================
/// 검증용 화면 저장. (파일로 쓴다)
///
/// # 인자
/// - `output_path`: 쓸 파일 경로. (VANILLA_CAPTURE_PATH, 있으면 덮어쓴다)
/// - `png_bytes`: PNG 파일 바이트.
///
/// # 돌려줌
/// 썼으면 Ok. 파일을 쓰지 못했으면 Err(입출력 오류 글).
//==============================================================================
pub fn save_capture_file(output_path: &str, png_bytes: &[u8]) -> Result<(), String> {
	std::fs::write(output_path, png_bytes).map_err(|error| error.to_string())
}

//==============================================================================
/// 페이지 전체 크기. (원본 window.innerWidth, innerHeight, 데스크탑에는 페이지가 없어 None 이고 ViewManager 가 창 안쪽 크기를 쓴다)
///
/// # 돌려줌
/// 늘 None.
//==============================================================================
pub fn read_client_native_size() -> Option<Vector2> {
	None
}

//==============================================================================
/// 화면 키보드가 가린 창 아래. (논리 좌표, visualViewport 아래 인셋)
/// - 윈도우는 크롬처럼 터치 키보드(InputPane)가 가린 영역의 세로다. (desktop/windowsinputpane.rs)
/// - 맥과 리눅스는 0 이다. 크롬 맥은 화면 키보드를 띄우지 않고 보이는 뷰포트가 늘 창 크기다. (render_widget_host_view_base.cc GetVisibleViewportSize)
///   크롬 리눅스 웨이랜드는 text-input 으로 입력판을 띄우기만 하고 그 영역을 받지 않아 뷰포트를 줄이지 않는다. (wayland_input_method_context.cc OnInputPanelState)
///   엔진도 winit 의 IME 켜기(웨이랜드 text-input-v3 enable)로 같은 신호를 낸다.
//==============================================================================
fn read_virtual_keyboard_inset() -> f64 {
	#[cfg(target_os = "windows")]
	{
		windowsinputpane::read_occluded_height()
	}
	#[cfg(not(target_os = "windows"))]
	{
		0.0
	}
}

//==============================================================================
/// CSS env(safe-area-inset-*) 세이프 에어리어 인셋. (데스크탑은 0)
/// - 크롬 데스크탑도 0 이다. 0 이 아닌 인셋은 WebContentsImpl::SetDisplayCutoutSafeArea 로만 들어오는데, 이 함수는 #if BUILDFLAG(IS_ANDROID) 안에 있고
///   안드로이드(web_contents_android.cc)만 부른다. 그 밖에는 블링크 기본값 "0px" 이다. (style_environment_variables.cc, 노치가 있는 맥북의 전체 화면도 같다)
///
/// # 돌려줌
/// 늘 [0, 0, 0, 0]. (웹과 같은 차례 위, 오른쪽, 아래, 왼쪽)
//==============================================================================
pub fn read_safe_area_insets() -> [f64; 4] {
	[0.0, 0.0, 0.0, 0.0]
}

//==============================================================================
/// 힙 사용량. (바이트, (쓰는 중, 잡아 둔 것), 못 읽으면 None. 개발자 도구의 메모리 칸, 원본 performance.memory 의 usedJSHeapSize, totalJSHeapSize 자리)
/// - 데스크탑에는 자바스크립트 힙이 없어 러스트가 쓰는 시스템 할당자의 힙을 읽는다. (러스트 System 할당자는 맥과 리눅스는 malloc, 윈도우는 프로세스 힙의 HeapAlloc)
/// - 맥: malloc_zone_statistics(모든 영역)의 size_in_use, size_allocated.
/// - 윈도우: 프로세스 힙의 HeapSummary 의 cbAllocated, cbCommitted.
/// - 리눅스(glibc): mallinfo2 의 uordblks + hblkhd(쓰는 중, mmap 으로 잡은 큰 덩어리 포함), arena + hblkhd(잡아 둔 것). glibc 2.33 부터라 실행할 때 찾고, 없으면 None.
/// - 그 밖의 OS 는 None 이다.
///
/// # 돌려줌
/// (쓰는 중인 바이트, 잡아 둔 바이트). 윈도우에서 HeapSummary 가 실패했거나, 리눅스에 mallinfo2 가 없거나, 그 밖의 OS 면 None.
///
/// # 예제
/// 힙 사용량을 메가바이트로 찍기
/// ```rust
/// use vanilla::platform::desktop::read_heap_usage;
///
/// if let Some((used_bytes, reserved_bytes)) = read_heap_usage() {
/// 	assert!(used_bytes > 0.0);
/// 	println!("{:.1} / {:.1} MB", used_bytes / 1048576.0, reserved_bytes / 1048576.0);
/// }
/// ```
//==============================================================================
pub fn read_heap_usage() -> Option<(f64, f64)> {
	#[cfg(target_os = "macos")]
	{
		#[repr(C)]
		struct MallocStatistics {
			blocks_in_use: std::os::raw::c_uint,
			size_in_use: usize,
			max_size_in_use: usize,
			size_allocated: usize,
		}
		extern "C" {
			fn malloc_zone_statistics(zone: *mut std::ffi::c_void, statistics: *mut MallocStatistics);
		}
		let mut statistics = MallocStatistics {
			blocks_in_use: 0,
			size_in_use: 0,
			max_size_in_use: 0,
			size_allocated: 0,
		};
		unsafe {
			malloc_zone_statistics(std::ptr::null_mut(), &mut statistics);
		}
		Some((statistics.size_in_use as f64, statistics.size_allocated as f64))
	}
	#[cfg(target_os = "windows")]
	{
		use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapSummary, HEAP_SUMMARY};
		let mut heap_summary = HEAP_SUMMARY {
			cb: std::mem::size_of::<HEAP_SUMMARY>() as u32,
			cbAllocated: 0,
			cbCommitted: 0,
			cbReserved: 0,
			cbMaxReserve: 0,
		};
		let is_read = unsafe {
			HeapSummary(GetProcessHeap(), 0, &mut heap_summary)
		};
		if is_read == 0 {
			return None;
		}
		Some((heap_summary.cbAllocated as f64, heap_summary.cbCommitted as f64))
	}
	#[cfg(all(target_os = "linux", target_env = "gnu"))]
	{
		#[repr(C)]
		struct MallocInformation {
			arena: usize,
			ordblks: usize,
			smblks: usize,
			hblks: usize,
			hblkhd: usize,
			usmblks: usize,
			fsmblks: usize,
			uordblks: usize,
			fordblks: usize,
			keepcost: usize,
		}
		extern "C" {
			fn dlsym(handle: *mut std::ffi::c_void, symbol: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
		}
		// RTLD_DEFAULT 는 glibc 에서 널이다.
		let mallinfo2_pointer = unsafe {
			dlsym(std::ptr::null_mut(), c"mallinfo2".as_ptr())
		};
		if mallinfo2_pointer.is_null() {
			return None;
		}
		let mallinfo2: extern "C" fn() -> MallocInformation = unsafe {
			std::mem::transmute(mallinfo2_pointer)
		};
		let malloc_information = mallinfo2();
		Some(((malloc_information.uordblks + malloc_information.hblkhd) as f64, (malloc_information.arena + malloc_information.hblkhd) as f64))
	}
	#[cfg(not(any(target_os = "macos", target_os = "windows", all(target_os = "linux", target_env = "gnu"))))]
	{
		None
	}
}

//==============================================================================
/// 힙 한도. (바이트, 힙이 자랄 수 있는 상한, 못 읽으면 None. 개발자 도구의 메모리 칸, 원본 performance.memory 의 jsHeapSizeLimit 자리)
/// - 데스크탑에는 자바스크립트 힙의 상한이 없어 운영체제가 이 프로세스의 힙에 허락하는 상한을 읽는다.
/// - 맥, 리눅스: 프로세스 자료 영역 한도(getrlimit RLIMIT_DATA 의 rlim_cur)가 정해져 있으면 그것, 없으면(무한) 물리 메모리 크기.
///   (맥은 sysctl hw.memsize, 리눅스는 sysconf 의 _SC_PHYS_PAGES 곱하기 _SC_PAGESIZE)
/// - 윈도우: GlobalMemoryStatusEx 의 물리 메모리 크기(ullTotalPhys)와 이 프로세스가 쓸 수 있는 가상 주소 크기(ullTotalVirtual) 가운데 작은 것.
/// - 그 밖의 OS 는 None 이다.
///
/// # 돌려줌
/// 힙 한도 바이트. 운영체제 함수가 실패했거나 그 밖의 OS 면 None.
///
/// # 예제
/// 힙 한도를 메가바이트로 찍기
/// ```rust
/// use vanilla::platform::desktop::read_heap_size_limit;
///
/// if let Some(limit_bytes) = read_heap_size_limit() {
/// 	assert!(limit_bytes > 0.0);
/// 	println!("{:.1} MB", limit_bytes / 1048576.0);
/// }
/// ```
//==============================================================================
pub fn read_heap_size_limit() -> Option<f64> {
	#[cfg(any(target_os = "macos", target_os = "linux"))]
	{
		#[repr(C)]
		struct ResourceLimit {
			current_limit: u64,
			maximum_limit: u64,
		}
		extern "C" {
			fn getrlimit(resource: std::os::raw::c_int, resource_limit: *mut ResourceLimit) -> std::os::raw::c_int;
		}
		// RLIMIT_DATA 는 맥과 리눅스 모두 2 다. 무한(RLIM_INFINITY)은 맥 2^63 - 1, 리눅스 2^64 - 1 이라 2^62 이상을 무한으로 본다.
		const RESOURCE_LIMIT_DATA: std::os::raw::c_int = 2;
		const INFINITE_LIMIT_FLOOR: u64 = 1 << 62;
		let mut resource_limit = ResourceLimit {
			current_limit: 0,
			maximum_limit: 0,
		};
		let result = unsafe {
			getrlimit(RESOURCE_LIMIT_DATA, &mut resource_limit)
		};
		if result == 0 && resource_limit.current_limit > 0 && resource_limit.current_limit < INFINITE_LIMIT_FLOOR {
			return Some(resource_limit.current_limit as f64);
		}
		read_physical_memory_size()
	}
	#[cfg(target_os = "windows")]
	{
		use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
		let mut memory_status: MEMORYSTATUSEX = unsafe {
			std::mem::zeroed()
		};
		memory_status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
		let is_read = unsafe {
			GlobalMemoryStatusEx(&mut memory_status)
		};
		if is_read == 0 {
			return None;
		}
		Some(memory_status.ullTotalPhys.min(memory_status.ullTotalVirtual) as f64)
	}
	#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
	{
		None
	}
}

//==============================================================================
/// 물리 메모리 크기. (바이트, 못 읽으면 None)
//==============================================================================
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn read_physical_memory_size() -> Option<f64> {
	#[cfg(target_os = "macos")]
	{
		extern "C" {
			fn sysctlbyname(name: *const std::os::raw::c_char, old_value: *mut std::ffi::c_void, old_length: *mut usize, new_value: *mut std::ffi::c_void, new_length: usize) -> std::os::raw::c_int;
		}
		let mut memory_size: u64 = 0;
		let mut memory_size_length = std::mem::size_of::<u64>();
		let result = unsafe {
			sysctlbyname(c"hw.memsize".as_ptr(), (&mut memory_size as *mut u64).cast(), &mut memory_size_length, std::ptr::null_mut(), 0)
		};
		if result != 0 || memory_size == 0 {
			return None;
		}
		Some(memory_size as f64)
	}
	#[cfg(target_os = "linux")]
	{
		extern "C" {
			fn sysconf(name: std::os::raw::c_int) -> std::os::raw::c_long;
		}
		// 리눅스의 _SC_PAGESIZE 는 30, _SC_PHYS_PAGES 는 85 다.
		const SYSTEM_CONFIGURATION_PAGE_SIZE: std::os::raw::c_int = 30;
		const SYSTEM_CONFIGURATION_PHYSICAL_PAGES: std::os::raw::c_int = 85;
		let (page_size, page_count) = unsafe {
			(sysconf(SYSTEM_CONFIGURATION_PAGE_SIZE), sysconf(SYSTEM_CONFIGURATION_PHYSICAL_PAGES))
		};
		if page_size <= 0 || page_count <= 0 {
			return None;
		}
		Some(page_size as f64 * page_count as f64)
	}
}

//==============================================================================
/// 응용 프로그램 종료. (데스크탑은 엔진의 종료 요청을 보고 이벤트 루프가 끝나므로 따로 할 일이 없다)
//==============================================================================
pub fn exit_application() {
}

//==============================================================================
/// 바깥 주소 열기. (원본 Engine.openUrl, 운영체제의 기본 프로그램으로 연다)
/// - 맥은 LaunchServices 를 쓰는 `open`, 윈도우는 ShellExecuteW, 그 밖의 유닉스는 xdg-open 이다.
/// - 열 수 있는 것은 http, https, mailto, tel 뿐이다. (is_openable_url) 그 밖의 꼴은 열지 않고 false.
///
/// # 인자
/// - `url`: 열 주소. (http, https, mailto, tel 꼴)
///
/// # 돌려줌
/// 운영체제에 열기를 넘겼으면 true.
//==============================================================================
pub fn open_url(url: &str) -> bool {
	if !crate::platform::is_openable_url(url) {
		return false;
	}
	#[cfg(target_os = "windows")]
	{
		use windows_sys::Win32::UI::Shell::ShellExecuteW;
		use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
		let operation_text: Vec<u16> = "open\0".encode_utf16().collect();
		let url_text: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
		// ShellExecuteW 는 성공하면 32 보다 큰 값을 돌려준다.
		let result = unsafe { ShellExecuteW(0, operation_text.as_ptr(), url_text.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
		return result as isize > 32;
	}
	#[cfg(not(target_os = "windows"))]
	{
		#[cfg(target_os = "macos")]
		let program = "open";
		#[cfg(not(target_os = "macos"))]
		let program = "xdg-open";
		return std::process::Command::new(program).arg(url).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_ok();
	}
}

//==============================================================================
/// 현지 시각 읽기. (원본 Engine.readLocalTime, 지금에서 분 오프셋만큼 더한 때를 이 기계의 시간대로 본 달력 시각)
/// - 맥과 리눅스는 localtime_r, 윈도우는 SystemTimeToTzSpecificLocalTime 이다. 더한 때의 일광 절약 시간까지 반영한다. (원본 new Date(Date.now() + 오프셋) 과 같다)
///
/// # 인자
/// - `minute_offset`: 지금에 더할 분. (음수는 과거, 0 이면 지금)
///
/// # 돌려줌
/// 더한 때를 이 기계의 시간대로 본 달력 시각.
//==============================================================================
pub fn read_local_time(minute_offset: f64) -> crate::platform::LocalTime {
	let now_milliseconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_millis() as i64).unwrap_or(0);
	let epoch_seconds = (now_milliseconds + (minute_offset * 60000.0).round() as i64).div_euclid(1000);
	#[cfg(target_os = "windows")]
	{
		#[repr(C)]
		struct SystemTime {
			year: u16,
			month: u16,
			day_of_week: u16,
			day: u16,
			hour: u16,
			minute: u16,
			second: u16,
			milliseconds: u16,
		}
		#[link(name = "kernel32")]
		extern "system" {
			fn SystemTimeToTzSpecificLocalTime(time_zone: *const u8, universal_time: *const SystemTime, local_time: *mut SystemTime) -> i32;
		}
		let universal = crate::platform::civil_from_epoch_seconds(epoch_seconds);
		let universal_time = SystemTime { year: universal.year as u16, month: universal.month as u16, day_of_week: universal.weekday as u16, day: universal.day as u16, hour: universal.hour as u16, minute: universal.minute as u16, second: universal.second as u16, milliseconds: 0 };
		let mut local_time = SystemTime { year: 0, month: 0, day_of_week: 0, day: 0, hour: 0, minute: 0, second: 0, milliseconds: 0 };
		if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &universal_time, &mut local_time) } != 0 {
			return crate::platform::LocalTime { year: local_time.year as i32, month: local_time.month as u32, day: local_time.day as u32, hour: local_time.hour as u32, minute: local_time.minute as u32, second: local_time.second as u32, weekday: local_time.day_of_week as u32 };
		}
		return universal;
	}
	#[cfg(not(target_os = "windows"))]
	{
		#[repr(C)]
		struct Tm {
			tm_sec: i32,
			tm_min: i32,
			tm_hour: i32,
			tm_mday: i32,
			tm_mon: i32,
			tm_year: i32,
			tm_wday: i32,
			tm_yday: i32,
			tm_isdst: i32,
			tm_gmtoff: i64,
			tm_zone: *const u8,
		}
		extern "C" {
			fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
		}
		let mut local_time = Tm { tm_sec: 0, tm_min: 0, tm_hour: 0, tm_mday: 1, tm_mon: 0, tm_year: 70, tm_wday: 4, tm_yday: 0, tm_isdst: 0, tm_gmtoff: 0, tm_zone: std::ptr::null() };
		unsafe {
			localtime_r(&epoch_seconds, &mut local_time);
		}
		return crate::platform::LocalTime { year: local_time.tm_year + 1900, month: (local_time.tm_mon + 1) as u32, day: local_time.tm_mday as u32, hour: local_time.tm_hour as u32, minute: local_time.tm_min as u32, second: local_time.tm_sec as u32, weekday: local_time.tm_wday as u32 };
	}
}

//==============================================================================
/// 창 제목 설정.
///
/// # 인자
/// - `platform_window`: 창 손잡이. (None 이면 아무것도 하지 않는다)
/// - `title`: 제목 글.
//==============================================================================
pub fn set_title(platform_window: Option<&PlatformWindow>, title: &str) {
	if let Some(window) = platform_window {
		window.set_title(title);
	}
}

//==============================================================================
/// 애셋 바이트 읽기. (파일을 그 자리에서 읽고 끝낸다. 엔진 캐시 없이 늘 새로 읽어 is_no_cache 를 켠 웹 fetch 처럼 지금 내용이다. 읽은 파일은 리소스 사용량에 적는다)
///
/// # 인자
/// - `asset_location`: 읽을 파일 경로. (엔진이 애셋 경로로 만든 것)
/// - `load_handle`: 다 읽으면 바이트나 오류 글로 끝낼 손잡이.
/// - `_is_no_cache`: 캐시를 거르지 않고 읽을지. (데스크탑은 늘 거르지 않아 보지 않아도 같다)
//==============================================================================
pub fn request_asset_bytes(asset_location: String, load_handle: AssetLoadHandle, _is_no_cache: bool) {
	let result = std::fs::read(&asset_location).map_err(|error| format!("애셋 읽기 실패 ({}): {}", asset_location, error));
	if let Ok(bytes) = &result {
		crate::base::platform::record_loaded_file(&asset_location, bytes.len() as u64);
	}
	load_handle.finish(result);
}

//==============================================================================
/// 창과 GL 상태.
/// - 필드 차례대로 버린다. 웨이랜드 프레임 콜백 맞추개는 winit 창의 표면과 wl_display 를 빌려 쓰므로 창보다 먼저 버린다.
//==============================================================================
struct WindowState {
	/// 웨이랜드 창이면 프레임 콜백 맞추개. (가려짐과 수직 동기, desktop/waylandframecallback.rs)
	#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
	wayland_frame_pacer: Option<waylandframecallback::WaylandFramePacer>,
	window: Rc<Window>,
	surface: Surface<WindowSurface>,
	context: PossiblyCurrentContext,
}

//==============================================================================
/// 이벤트 루프에 물리는 데스크탑 실행자.
//==============================================================================
struct DesktopApplication {
	engine_runner: EngineRunner,
	window_state: Option<WindowState>,
	/// Engine::new 가 숨긴 채 지은 창. (run 의 처음 resumed 에서 보이고 window_state 로 옮긴다)
	prepared_window_state: Option<WindowState>,
	next_frame_instant: Instant,
	fatal_error: Option<String>,
	text_input_state: textinput::DesktopTextInputState,
	/// 앱이 suspended 되었는지. (다시 resumed 되면 페이지 프로세스 복원으로 알린다)
	is_suspended: bool,
	/// 창이 가려졌는지. (Occluded)
	is_occluded: bool,
	/// 창이 최소화되었는지.
	is_minimized: bool,
	/// 웨이랜드 프레임 콜백이 3 초 넘게 오지 않아 가려진 것으로 보는지. (크롬 리눅스 웨이랜드의 가려짐)
	is_frame_callback_occluded: bool,
	/// 엔진에 알린 페이지 가려짐. (가려짐, 최소화, 앱 멈춤, 웨이랜드 프레임 콜백 가려짐 가운데 하나)
	is_page_hidden: bool,
	/// 창의 배율. (입력 좌표를 논리 좌표로 바꿀 때, 렌더 배율 상한과 상관없다)
	window_scale_factor: f64,
	/// 줄인 렌더 영역을 창 크기로 늘이개. (처음 쓸 때 만든다)
	render_scaler: Option<renderscaler::RenderScaler>,
	/// 드로잉 버퍼 남기개. (preserve_drawing_buffer 일 때 처음 쓸 때 만든다)
	preserved_drawing_buffer: Option<preserveddrawingbuffer::PreservedDrawingBuffer>,
	/// 화면 키보드가 가린 창 아래. (논리 좌표, visualViewport 아래 인셋, 윈도우 터치 키보드만 0 이 아니다)
	virtual_keyboard_inset: f64,
	/// 마지막으로 맞춘 visualViewport 크기. (ignore_virtual_keyboard_resize 가 견준다, 원본 appliedClientNativeSize)
	applied_client_size: Option<(f64, f64)>,
	/// 윈도우 터치 키보드 제어. (창을 만들 때 만든다)
	#[cfg(target_os = "windows")]
	virtual_keyboard_controller: Option<windowsinputpane::VirtualKeyboardController>,
	/// 지난 프레임 뒤에 손가락을 댄 자리. (창 안쪽 논리 좌표, 윈도우 터치 키보드 띄우기)
	#[cfg(target_os = "windows")]
	touch_started_position: Option<Vector2>,
	/// 창을 숨기고 도는지. (찍기 모드, is_capture_window_hidden)
	is_window_hidden: bool,
	/// 지난 프레임의 단조 시계와 벽시계. (맥과 리눅스의 잠자기 판정)
	#[cfg(not(target_os = "windows"))]
	last_frame_clock: Option<(Instant, SystemTime)>,
	/// 창 안의 캔버스 자리. (물리 픽셀 [왼쪽, 위, 가로, 세로], 위 왼쪽 원점)
	canvas_physical_rect: [i32; 4],
}

impl DesktopApplication {
	//==============================================================================
	/// 창과 GL 컨텍스트, 그래픽 만들기.
	//==============================================================================
	fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
		let window_state = match self.prepared_window_state.take() {
			// Engine::new 가 지은 숨긴 창: 보이고(찍기 모드는 숨긴 채), 엔진에 그래픽이 없으면(앞 엔진이 지은 창) 그 컨텍스트로 짓는다.
			Some(window_state) => {
				if !self.is_window_hidden {
					window_state.window.set_visible(true);
					window_state.window.focus_window();
				}
				if self.engine_runner.get_graphic().is_none() {
					let preserve_drawing_buffer = self.engine_runner.engine.get_engine_configuration().preserve_drawing_buffer;
					let graphic = create_graphic(&window_state.context.display(), preserve_drawing_buffer, self.engine_runner.engine.get_color_space())?;
					self.engine_runner.attach_graphic(graphic);
				}
				window_state
			}
			None => {
				let (window_state, graphic) = create_window_state(WindowEventLoop::ActiveEventLoop(event_loop), &self.engine_runner.engine, !self.is_window_hidden)?;
				self.engine_runner.attach_graphic(graphic);
				window_state
			}
		};
		let window = window_state.window.clone();
		#[cfg(target_os = "windows")]
		{
			windowsplatform::install_power_broadcast_hook(&window);
			self.virtual_keyboard_controller = windowsinputpane::VirtualKeyboardController::new(&window);
		}
		*self.engine_runner.engine.platform_window.borrow_mut() = Some(window);
		self.window_state = Some(window_state);
		self.resize();
		Ok(())
	}

	//==============================================================================
	/// 해상도 변경됨. (서피스 크기를 맞추고 뷰 영역을 다시 계산한 뒤 씬에 알린다)
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
		let scale_factor = window_state.window.scale_factor();
		if let (Some(width), Some(height)) = (NonZeroU32::new(physical_size.width), NonZeroU32::new(physical_size.height)) {
			window_state.surface.resize(&window_state.context, width, height);
		}
		let client_width = physical_size.width as f64 / scale_factor;
		let client_height = physical_size.height as f64 / scale_factor;
		// 폰트 문자열의 뷰포트 단위(vw, vh)는 원본 window.innerWidth / innerHeight 처럼 창의 논리 크기에 댄다. (화면 키보드는 visualViewport 만 줄인다)
		crate::core::textstringtexturecache::set_font_viewport_size(client_width as f32, client_height as f32);
		let engine_configuration = self.engine_runner.engine.get_engine_configuration();
		let is_auto_resize = engine_configuration.auto_resize_on_window_resize;
		let is_virtual_keyboard_ignored = engine_configuration.ignore_virtual_keyboard_resize;
		let max_canvas_aspect_ratio = engine_configuration.max_canvas_aspect_ratio;
		drop(engine_configuration);
		let is_text_field_focused = self.engine_runner.engine.get_input_manager().get_text_input().get_active_field_state().map(|field_state| field_state.use_dominput).unwrap_or(false);
		let (canvas_left, canvas_top, canvas_width, canvas_height) = if is_auto_resize {
			// 원본 engine.resize: visualViewport 크기에 맞춘다. (창 안쪽에서 화면 키보드가 가린 아래를 뺀 것)
			let viewport_width = client_width;
			let mut viewport_height = (client_height - self.virtual_keyboard_inset).max(0.0);
			// 화면 키보드: 글자 입력칸이 포커스된 동안 가로가 같고 세로만 줄었으면 크기를 그대로 둔다. (설정으로 켠다)
			if is_virtual_keyboard_ignored && is_text_field_focused {
				if let Some((applied_client_width, applied_client_height)) = self.applied_client_size {
					if (viewport_width - applied_client_width).abs() < 1.0 && viewport_height < applied_client_height {
						viewport_height = applied_client_height;
					}
				}
			}
			self.applied_client_size = Some((viewport_width, viewport_height));
			// 캔버스 비 상한이 있으면 원본처럼 캔버스를 줄여 가운데에 둔다. (논리 좌표, 내림)
			if max_canvas_aspect_ratio > 0.0 {
				let canvas_width = viewport_width.min(viewport_height * max_canvas_aspect_ratio).floor();
				let canvas_height = viewport_height.min(viewport_width * max_canvas_aspect_ratio).floor();
				(((viewport_width - canvas_width) * 0.5).floor(), ((viewport_height - canvas_height) * 0.5).floor(), canvas_width, canvas_height)
			}
			else {
				(0.0, 0.0, viewport_width, viewport_height)
			}
		}
		else {
			// 끄면 원본은 페이지가 정한 캔버스 크기를 읽는다. 데스크탑 창은 캔버스가 창을 가득 채운 페이지(CSS 100%)와 같아 창 크기다.
			// (그런 페이지의 캔버스는 화면 키보드에 줄지 않고, 캔버스 비 상한도 원본처럼 걸지 않는다)
			(0.0, 0.0, client_width, client_height)
		};
		CANVAS_CLIENT_POSITION.with(|canvas_client_position| canvas_client_position.set(Vector2::create(canvas_left, canvas_top)));
		self.canvas_physical_rect = [(canvas_left * scale_factor).round() as i32, (canvas_top * scale_factor).round() as i32, (canvas_width * scale_factor).round() as i32, (canvas_height * scale_factor).round() as i32];
		// 렌더 배율 상한을 넘으면 그 배율로 줄인 크기로 그린다. (웹 캔버스 드로잉 버퍼와 같은 셈)
		let mut view_manager = self.engine_runner.engine.get_view_manager_mut();
		let render_pixel_ratio = view_manager.limit_render_pixel_ratio(scale_factor);
		let render_width = ((canvas_width * render_pixel_ratio).round() as u32).max(1);
		let render_height = ((canvas_height * render_pixel_ratio).round() as u32).max(1);
		view_manager.calculate_view_rect(render_width, render_height, render_pixel_ratio);
		drop(view_manager);
		self.window_scale_factor = scale_factor;
		self.engine_runner.notify_scene_resize();
	}

	//==============================================================================
	/// 페이지 가려짐 알리기. (가려짐, 최소화, 앱 멈춤, 웨이랜드 프레임 콜백 가려짐 가운데 하나라도 맞으면 Hidden, 모두 풀리면 Visible 과 on_page_visibility_restored, 바뀔 때만)
	//==============================================================================
	fn update_page_visibility(&mut self) {
		let is_page_hidden = self.is_occluded || self.is_minimized || self.is_suspended || self.is_frame_callback_occluded;
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
	/// 웨이랜드 프레임 콜백으로 본 가려짐 알리기. (크롬 리눅스 웨이랜드처럼 보낸 콜백이 3 초 넘게 오지 않으면 가려짐, 찍기 모드는 받지 않는다)
	//==============================================================================
	#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
	fn update_frame_callback_occlusion(&mut self) {
		if self.is_window_hidden {
			return;
		}
		let is_frame_callback_occluded = match self.window_state.as_mut().and_then(|window_state| window_state.wayland_frame_pacer.as_mut()) {
			Some(wayland_frame_pacer) => {
				wayland_frame_pacer.is_occluded()
			}
			None => {
				return;
			}
		};
		if is_frame_callback_occluded != self.is_frame_callback_occluded {
			self.is_frame_callback_occluded = is_frame_callback_occluded;
			self.update_page_visibility();
		}
	}

	//==============================================================================
	/// 윈도우 터치 키보드 띄우고 내리기. (크롬 윈도우처럼 손가락으로 글자 입력칸에 포커스하거나 그 입력칸을 누르면 띄우고, 포커스가 떠나면 내린다. desktop/windowsinputpane.rs)
	//==============================================================================
	#[cfg(target_os = "windows")]
	fn update_virtual_keyboard(&mut self) {
		let touch_started_position = self.touch_started_position.take();
		let input_manager = self.engine_runner.engine.get_input_manager();
		let (focused_field_id, is_field_touched) = match input_manager.get_text_input().get_active_field_state() {
			Some(field_state) if field_state.use_dominput => {
				(field_state.field_id, touch_started_position.map(|position| field_state.field_rect.contains(position)).unwrap_or(false))
			}
			_ => {
				(0, false)
			}
		};
		let is_last_pointer_touch = input_manager.get_last_input_device() == InputDevice::Touch;
		if let Some(virtual_keyboard_controller) = &mut self.virtual_keyboard_controller {
			virtual_keyboard_controller.update(focused_field_id, is_field_touched, is_last_pointer_touch);
		}
	}

	//==============================================================================
	/// 시스템 잠자기에서 깼는지 보고 알리기. (윈도우는 WM_POWERBROADCAST 의 깸 알림을 받았으면)
	//==============================================================================
	#[cfg(target_os = "windows")]
	fn check_system_sleep(&mut self) {
		if windowsplatform::take_system_resumed() {
			self.engine_runner.notify_page_process_restored();
		}
	}

	//==============================================================================
	/// 시스템 잠자기에서 깼는지 보고 알리기. (맥과 리눅스는 두 프레임 사이에 벽시계가 단조 시계보다 SYSTEM_SLEEP_GAP_SECONDS 넘게 더 흘렀으면)
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
	/// 창 안쪽 물리 좌표 → 입력 위치 갱신.
	//==============================================================================
	fn update_input_position(&mut self, physical_x: f64, physical_y: f64) {
		let window_scale_factor = self.window_scale_factor;
		let canvas_client_position = read_canvas_client_position();
		let engine = &mut self.engine_runner.engine;
		let canvas_native_input_position = Vector2::create(physical_x / window_scale_factor - canvas_client_position.x, physical_y / window_scale_factor - canvas_client_position.y);
		let view_input_position = engine.get_view_manager().canvas_position_to_view_position(canvas_native_input_position);
		let mut input_manager = engine.get_input_manager_mut();
		input_manager.set_canvas_native_input_position(canvas_native_input_position);
		input_manager.set_view_input_position(view_input_position);
	}

	//==============================================================================
	/// 손가락. (손가락마다 닿음, 움직임, 뗌을 적고, 첫 손가락은 마우스 왼쪽 단추처럼 터치로 넘긴다)
	//==============================================================================
	fn handle_touch(&mut self, touch: winit::event::Touch) {
		let window_scale_factor = self.window_scale_factor;
		let canvas_client_position = read_canvas_client_position();
		let canvas_native_position = Vector2::create(touch.location.x / window_scale_factor - canvas_client_position.x, touch.location.y / window_scale_factor - canvas_client_position.y);
		let view_position = self.engine_runner.engine.get_view_manager().canvas_position_to_view_position(canvas_native_position);
		let identifier = touch.id as i64;
		let input_manager = self.engine_runner.engine.get_input_manager();
		let touch_points = input_manager.get_touch_points();
		let is_first_finger = touch_points.is_empty() || touch_points[0].identifier == identifier;
		drop(input_manager);
		#[cfg(target_os = "windows")]
		if touch.phase == TouchPhase::Started {
			self.touch_started_position = Some(Vector2::create(touch.location.x as f64 / window_scale_factor, touch.location.y as f64 / window_scale_factor));
		}
		if is_first_finger {
			self.update_input_position(touch.location.x, touch.location.y);
		}
		let mut input_manager = self.engine_runner.engine.get_input_manager_mut();
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
	/// 한 프레임 돌고 화면 넘기기.
	//==============================================================================
	fn update_frame(&mut self) {
		if self.window_state.is_none() {
			return;
		}
		self.check_system_sleep();
		#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
		self.update_frame_callback_occlusion();
		// 화면 키보드가 가린 아래가 바뀌면 다시 맞춘다. (원본은 visualViewport 의 resize 이벤트로 engine.resize 를 부른다)
		let virtual_keyboard_inset = read_virtual_keyboard_inset();
		if virtual_keyboard_inset != self.virtual_keyboard_inset {
			self.virtual_keyboard_inset = virtual_keyboard_inset;
			self.engine_runner.engine.is_resize_requested.set(true);
		}
		// 렌더 배율 상한을 바꿨으면 프레임 머리에서 창 크기 바뀜과 같은 길로 맞춘다. (원본 Engine 의 isViewRectPending, 그리는 중에 바꾸면 그 프레임이 까맣다)
		if self.engine_runner.engine.get_view_manager().is_view_rect_pending() || self.engine_runner.engine.is_resize_requested.get() {
			self.engine_runner.engine.is_resize_requested.set(false);
			self.resize();
		}
		self.restore_preserved_drawing_buffer();
		self.engine_runner.update_engine();
		self.save_preserved_drawing_buffer();
		self.present_render_scaled();
		#[cfg(target_os = "windows")]
		self.update_virtual_keyboard();
		#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
		let use_vertical_sync = self.engine_runner.engine.get_engine_configuration().use_vertical_sync;
		if let Some(window_state) = &mut self.window_state {
			self.text_input_state.apply_active_field(&window_state.window, self.engine_runner.engine.get_input_manager().get_text_input());
			#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
			if let Some(wayland_frame_pacer) = &mut window_state.wayland_frame_pacer {
				wayland_frame_pacer.prepare_swap(use_vertical_sync);
			}
			if let Err(error) = window_state.surface.swap_buffers(&window_state.context) {
				eprintln!("Engine: 화면 넘기기 실패: {}", error);
			}
			#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
			if let Some(wayland_frame_pacer) = &mut window_state.wayland_frame_pacer {
				wayland_frame_pacer.finish_swap();
			}
		}
	}
}

impl DesktopApplication {
	//==============================================================================
	/// 부탁받은 그림 커서를 만들어 걸기. (그림 파일을 애셋 폴더에서 읽어 winit 커서로 만든다, 못 만들면 CSS 커서를 그대로 둔다)
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
	/// 지난 프레임 그림 되살리기. (preserve_drawing_buffer 일 때 엔진 프레임 첫머리, desktop/preserveddrawingbuffer.rs)
	//==============================================================================
	fn restore_preserved_drawing_buffer(&mut self) {
		let graphic = match self.engine_runner.get_graphic() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		if !graphic.is_drawing_buffer_preserved() {
			return;
		}
		let canvas_pixel_size = self.engine_runner.engine.get_view_manager().get_canvas_pixel_size();
		let gl = graphic.get_tracked_gl();
		if self.preserved_drawing_buffer.is_none() {
			match preserveddrawingbuffer::PreservedDrawingBuffer::new(gl) {
				Ok(preserved_drawing_buffer) => {
					self.preserved_drawing_buffer = Some(preserved_drawing_buffer);
				}
				Err(error) => {
					eprintln!("Engine: 드로잉 버퍼 남기개 생성 실패: {}", error);
					return;
				}
			}
		}
		if let Some(preserved_drawing_buffer) = &self.preserved_drawing_buffer {
			preserved_drawing_buffer.restore(gl, canvas_pixel_size.x as i32, canvas_pixel_size.y as i32);
		}
	}

	//==============================================================================
	/// 다 그린 프레임 떠 두기. (preserve_drawing_buffer 일 때 엔진 프레임 뒤, 늘이개가 늘이기 전)
	//==============================================================================
	fn save_preserved_drawing_buffer(&mut self) {
		let canvas_pixel_size = self.engine_runner.engine.get_view_manager().get_canvas_pixel_size();
		let graphic = match self.engine_runner.get_graphic() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		if let Some(preserved_drawing_buffer) = &mut self.preserved_drawing_buffer {
			preserved_drawing_buffer.save(graphic.get_tracked_gl(), canvas_pixel_size.x as i32, canvas_pixel_size.y as i32);
		}
	}

	//==============================================================================
	/// 줄인 렌더 크기로 그렸거나 캔버스를 가운데에 두었으면 창의 캔버스 자리로 늘여 그리기. (렌더 크기가 창 크기와 같으면 하지 않는다)
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
		let gl = graphic.get_tracked_gl();
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
	/// 창 만들 차례. (데스크탑에서는 시작할 때 한 번)
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
	/// 앱이 멈춤. (모바일의 앱 전환, 윈도우, 맥, 리눅스에서는 오지 않는다)
	//==============================================================================
	fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
		self.is_suspended = true;
		self.update_page_visibility();
	}

	//==============================================================================
	/// 창 이벤트.
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
					let mut input_manager = self.engine_runner.engine.get_input_manager_mut();
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
					let mut input_manager = self.engine_runner.engine.get_input_manager_mut();
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
				let mut input_manager = self.engine_runner.engine.get_input_manager_mut();
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
				let mut input_manager = self.engine_runner.engine.get_input_manager_mut();
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
				let mut input_manager = self.engine_runner.engine.get_input_manager_mut();
				let is_ctrl_pressed = input_manager.is_key_pressed("ControlLeft") || input_manager.is_key_pressed("ControlRight");
				input_manager.set_wheel_with_ctrl(is_ctrl_pressed);
				input_manager.set_last_input_device(InputDevice::Mouse);
				match delta {
					MouseScrollDelta::LineDelta(line_x, line_y) => {
						input_manager.add_wheel_delta(-line_x as f64 * WHEEL_LINE_PIXEL_SIZE, -line_y as f64 * WHEEL_LINE_PIXEL_SIZE);
					}
					MouseScrollDelta::PixelDelta(pixel_delta) => {
						input_manager.add_wheel_delta(-pixel_delta.x, -pixel_delta.y);
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
	/// 이벤트를 다 처리한 뒤. (다음 프레임 요청, 최대 FPS 제한)
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
			maximum_frame_per_second = refresh_rate_millihertz as f64 / 1000.0;
		}

		// 차례가 된 프레임만 그린다. (한 프레임 넘게 밀렸으면 지금부터 다시 센다)
		let frame_interval = Duration::from_secs_f64(1.0 / maximum_frame_per_second);
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
/// 그림 커서 열쇠. (그림 주소, 2 배 그림 주소, 핫스팟, 창 배율이 같으면 같은 커서)
//==============================================================================
fn create_custom_cursor_key(cursor_value: &CursorValue, scale_factor: f64) -> String {
	format!("{}|{}|{}|{}|{}", cursor_value.image_url, cursor_value.image_url_2x, cursor_value.hotspot_x, cursor_value.hotspot_y, scale_factor)
}

//==============================================================================
/// 그림 파일로 winit 커서 원본 만들기. (1 배 그림 기준 핫스팟)
/// - 맥: winit 이 그림의 픽셀 수를 그대로 포인트 크기로 쓰므로 1 배 그림 그대로.
/// - 윈도우, 리눅스: 그림의 픽셀 수가 화면 픽셀 수라 창 배율만큼 늘인다. 배율이 1 보다 크고 2 배 그림이 있으면 그것을 맞춰 쓰고 핫스팟도 배율만큼 옮긴다.
///   (브라우저가 그림 커서를 기기 배율로 늘이는 것과 같은 크기)
//==============================================================================
fn create_custom_cursor_source(image_path: &std::path::Path, image_path_2x: Option<&std::path::Path>, hotspot_x: f64, hotspot_y: f64, scale_factor: f64) -> Result<winit::window::CustomCursorSource, String> {
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
	let (source_width, source_height, source_pixels) = crate::core::image::Image::decode_straight_pixels(&image_bytes).map_err(|error| format!("{} ({})", error, source_path.display()))?;
	let source_image = image::RgbaImage::from_raw(source_width, source_height, source_pixels).ok_or_else(|| format!("그림 크기가 틀렸다 ({})", source_path.display()))?;
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
	let cursor_hotspot_x = ((hotspot_x * cursor_scale).round().max(0.0) as u32).min(cursor_width - 1);
	let cursor_hotspot_y = ((hotspot_y * cursor_scale).round().max(0.0) as u32).min(cursor_height - 1);
	CustomCursor::from_rgba(cursor_image.into_raw(), cursor_width as u16, cursor_height as u16, cursor_hotspot_x as u16, cursor_hotspot_y as u16).map_err(|error| error.to_string())
}

//==============================================================================
/// winit 키를 원본 KeyboardEvent.code 이름으로. (winit 이름이 W3C 와 같아 그대로 쓰고, 다른 셋만 바꾼다)
/// - 윈도우 키(커맨드 키): winit SuperLeft, SuperRight → W3C MetaLeft, MetaRight
/// - W3C 의 옛 Super 키: winit Meta → Super
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
/// 창을 숨기고 돌지. (찍기 모드 VANILLA_CAPTURE_PATH 면 숨긴다. VANILLA_CAPTURE_SHOW_WINDOW=1 이면 보인다)
/// - 검증 도구가 예제를 찍을 때 사용자 화면에 창이 뜨지 않게 한다. 맥은 독 아이콘과 포커스도 가져가지 않는다.
//==============================================================================
fn is_capture_window_hidden() -> bool {
	if std::env::var("VANILLA_CAPTURE_PATH").is_err() {
		return false;
	}
	std::env::var("VANILLA_CAPTURE_SHOW_WINDOW").map(|value| value != "1").unwrap_or(true)
}

//==============================================================================
/// GL 설정 고르기. (스텐실 8 비트 이상, 멀티샘플은 4 에 가까운 것, 4 를 넘으면 작은 쪽. is_antialias 가 거짓이면 표본이 가장 적은 것)
//==============================================================================
fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>, is_antialias: bool) -> Config {
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
				if !is_antialias {
					sample_count < current_sample_count
				}
				else if sample_count <= 4 {
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
/// 창 아이콘 읽기. (PNG, 스트레이트 알파)
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
	/// 그림 커서 원본은 그림 파일로 만들고(창 배율과 2 배 그림, 핫스팟은 그림 안으로), 없는 파일은 오류다.
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
	/// winit 키는 W3C code 이름으로 넣는다. (윈도우 키는 MetaLeft, MetaRight)
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
	/// 힙 사용량은 맥, 윈도우, 리눅스(glibc)에서 읽히고, 쓰는 중인 것이 잡아 둔 것보다 크지 않다.
	//==============================================================================
	#[test]
	#[cfg(any(target_os = "macos", target_os = "windows", all(target_os = "linux", target_env = "gnu")))]
	fn heap_usage_is_read() {
		let held_bytes = vec![7u8; 1 << 20];
		let (used_heap_size, total_heap_size) = read_heap_usage().expect("heap usage");
		assert!(used_heap_size >= held_bytes.len() as f64);
		assert!(used_heap_size <= total_heap_size);
	}

	//==============================================================================
	/// 쿼리 이름은 VANILLA_ 뒤 대문자 뱀꼴 환경 변수로 읽는다.
	//==============================================================================
	#[test]
	fn query_name_becomes_variable_name() {
		assert_eq!(convert_query_name_to_variable_name("safeArea"), "VANILLA_SAFE_AREA");
		assert_eq!(convert_query_name_to_variable_name("quality"), "VANILLA_QUALITY");
	}
}
