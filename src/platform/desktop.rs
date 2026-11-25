//==============================================================================
// 데스크탑 플랫폼. (윈도우, 맥, 리눅스)
// - 창(winit)과 GL 3.3 코어 컨텍스트(glutin)를 만들고, 창의 다시 그리기 요청마다 엔진 프레임을 하나 돈다.
// - 창 이벤트를 입력 관리자 꼴로 바꿔 넣는다. (키는 W3C KeyboardEvent.code 와 같은 이름, 마우스 단추는 원본처럼 어느 것이든 터치, 왼쪽만으로 바꿀 수 있다)
// - 엔진 본체(EngineRunner)는 창을 모른다. 여기서 프레임을 부르고, 그린 뒤 화면을 넘긴다.
// - 글 입력(키 글자, 편집 키, IME 조합)과 클립보드는 desktop/textinput.rs 가 맡는다. 오른쪽 단추는 입력 필드 문맥 메뉴 요청이다.
// - 마지막으로 쓴 장치를 키, 마우스, 손가락 이벤트마다 적는다. 막을 키(set_prevented_keys)는 창에 막을 브라우저 기본 동작이 없어 쓰지 않는다.
// - 페이지 이벤트: 창이 가려짐(Occluded, 맥과 X11 이 알린다), 최소화(크기 0 이나 is_minimized, 윈도우가 이것으로 알린다), 앱 멈춤(suspended, 모바일)
//   가운데 하나라도 맞으면 Hidden, 모두 풀리면 Visible 과 on_page_visibility_restored. 포커스는 Focused / Blurred, 마우스 단추와 손가락 누름은 PointerDown.
//   웨이랜드는 가림도 최소화도 알리지 않아 Hidden 이 오지 않는다.
// - 페이지 프로세스 복원(on_page_process_restored, 원본 Chromium freeze 뒤 resume 과 bfcache): suspended 뒤 resumed, 그리고 시스템 잠자기에서 깬 것.
//   잠자기는 두 프레임 사이에 벽시계(SystemTime)가 단조 시계(Instant)보다 SYSTEM_SLEEP_GAP_SECONDS 넘게 더 흘렀으면 깬 것으로 본다.
//   (맥과 리눅스의 단조 시계는 잠든 동안 멈춘다. 윈도우의 단조 시계는 잠든 동안에도 흐르는 것으로 알려져 있어 이 판정이 맞지 않을 수 있다, 짐작)
// - 렌더 배율 상한(ViewManager.set_max_render_pixel_ratio): 그 배율로 줄인 크기로 그리고 프레임 끝에 창 크기로 늘인다. (desktop/renderscaler.rs)
//   입력 좌표는 창의 배율로 논리 좌표로 바꾸므로 렌더 크기와 상관없다.
// - 페이지 주소의 쿼리 값(read_page_query_value)은 환경 변수 VANILLA_ 뒤에 이름을 대문자 뱀꼴로 붙인 것으로 읽는다. (safeArea → VANILLA_SAFE_AREA)
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
use web_time::{Instant, SystemTime};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use crate::core::inputmanager::{InputDevice, MouseButton as EngineMouseButton};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::CursorIcon;
use winit::keyboard::PhysicalKey;
use winit::window::{Icon, Window, WindowId};

mod renderscaler;
mod textinput;
pub use textinput::{read_clipboard_text, write_clipboard_text};

// 엔진이 들고 있는 창 손잡이. (커서, 제목을 바꿀 때)
pub type PlatformWindow = Rc<Window>;

// 휠 한 줄을 픽셀로 볼 때의 값. (브라우저 deltaMode 줄 단위 환산과 같은 자리)
const WHEEL_LINE_PIXEL_SIZE: f32 = 40.0;
// 두 프레임 사이에 벽시계가 단조 시계보다 이만큼 넘게 더 흐르면 시스템이 잠들었다 깬 것으로 본다. (초)
const SYSTEM_SLEEP_GAP_SECONDS: f64 = 2.0;

//==============================================================================
// 시작. (창을 닫거나 exit_application 을 부를 때까지 돌아오지 않는다)
//==============================================================================
pub fn run(engine_runner: EngineRunner) -> Result<(), String> {
	let event_loop = EventLoop::new().map_err(|error| format!("이벤트 루프 생성 실패: {}", error))?;
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
		window_scale_factor: 1.0,
		applied_max_render_pixel_ratio: 0.0,
		render_scaler: None,
		last_frame_clock: None,
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
//==============================================================================
pub fn set_cursor(platform_window: Option<&PlatformWindow>, _css_value: &str, cursor_name: &str) {
	let window = match platform_window {
		Some(window) => {
			window
		}
		None => {
			return;
		}
	};
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
		_ => {
			CursorIcon::Default
		}
	};
	window.set_cursor(cursor_icon);
}

//==============================================================================
// 주된 손이 손가락인지. (데스크탑은 마우스가 주된 손이다)
//==============================================================================
pub fn is_coarse_pointer() -> bool {
	false
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
// 화면(client) 좌표에서 캔버스의 왼쪽 위. (데스크탑은 창 안쪽 전체가 캔버스라 창의 논리 좌표가 곧 client 좌표다, 늘 0)
//==============================================================================
pub fn read_canvas_client_position() -> Vector2 {
	Vector2::zero()
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
// 창 제목 설정.
//==============================================================================
pub fn set_title(platform_window: Option<&PlatformWindow>, title: &str) {
	if let Some(window) = platform_window {
		window.set_title(title);
	}
}

//==============================================================================
// 애셋 바이트 읽기. (파일을 그 자리에서 읽고 끝낸다)
//==============================================================================
pub fn request_asset_bytes(asset_location: String, load_handle: AssetLoadHandle) {
	let result = std::fs::read(&asset_location).map_err(|error| format!("애셋 읽기 실패 ({}): {}", asset_location, error));
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
	// 지난 프레임의 단조 시계와 벽시계. (잠자기 판정)
	last_frame_clock: Option<(Instant, SystemTime)>,
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
			.with_resizable(engine_configuration.is_resizable);
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
		// 렌더 배율 상한을 넘으면 그 배율로 줄인 크기로 그린다. (웹 캔버스 드로잉 버퍼와 같은 셈)
		let view_manager = self.engine_runner.engine.get_view_manager_mut();
		let render_pixel_ratio = view_manager.limit_render_pixel_ratio(scale_factor);
		let (render_width, render_height) = if render_pixel_ratio < scale_factor {
			let render_scale = render_pixel_ratio / scale_factor;
			(((physical_size.width as f32 * render_scale).round() as u32).max(1), ((physical_size.height as f32 * render_scale).round() as u32).max(1))
		}
		else {
			(physical_size.width, physical_size.height)
		};
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
	// 시스템 잠자기에서 깼는지 보고 알리기. (두 프레임 사이에 벽시계가 단조 시계보다 SYSTEM_SLEEP_GAP_SECONDS 넘게 더 흘렀으면)
	//==============================================================================
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
		let engine = &mut self.engine_runner.engine;
		let canvas_native_input_position = Vector2::create(physical_x as f32 / window_scale_factor, physical_y as f32 / window_scale_factor);
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
		let canvas_native_position = Vector2::create(touch.location.x as f32 / window_scale_factor, touch.location.y as f32 / window_scale_factor);
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
				input_manager.set_touch_down(true);
			}
			TouchPhase::Moved => {
				input_manager.set_touch_moved(true);
			}
			TouchPhase::Ended => {
				input_manager.set_touch_released(true);
				input_manager.set_touch_down(false);
			}
			TouchPhase::Cancelled => {
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
		if max_render_pixel_ratio != self.applied_max_render_pixel_ratio {
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
	// 줄인 렌더 크기로 그렸으면 창 크기로 늘여 그리기. (렌더 크기가 창 크기와 같으면 하지 않는다)
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
			render_scaler.present(gl, render_width, render_height, window_width, window_height);
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
				if !is_focused {
					self.engine_runner.engine.get_input_manager_mut().clear();
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
				self.is_occluded = is_occluded;
				self.update_page_visibility();
			}

			WindowEvent::KeyboardInput { event: key_event, .. } => {
				if let PhysicalKey::Code(key_code) = key_event.physical_key {
					let key = format!("{:?}", key_code);
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
				input_manager.set_touch_moved(true);
				input_manager.set_touch_input(false);
				input_manager.set_last_input_device(InputDevice::Mouse);
			}
			WindowEvent::CursorLeft { .. } => {
				let input_manager = self.engine_runner.engine.get_input_manager_mut();
				if input_manager.is_touch_down() {
					input_manager.set_touch_down(false);
					input_manager.set_touch_cancelled(true);
				}
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
		let maximum_frame_per_second = self.engine_runner.engine.get_engine_configuration().maximum_frame_per_second;
		if maximum_frame_per_second <= 0.0 {
			event_loop.set_control_flow(ControlFlow::Poll);
			window_state.window.request_redraw();
			return;
		}

		// 차례가 된 프레임만 그린다. (한 프레임 넘게 밀렸으면 지금부터 다시 센다)
		let frame_interval = Duration::from_secs_f64(1.0 / maximum_frame_per_second as f64);
		let now = Instant::now();
		if now >= self.next_frame_instant {
			window_state.window.request_redraw();
			self.next_frame_instant += frame_interval;
			if self.next_frame_instant + frame_interval < now {
				self.next_frame_instant = now + frame_interval;
			}
		}
		event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_instant));
	}
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
	// 쿼리 이름은 VANILLA_ 뒤 대문자 뱀꼴 환경 변수로 읽는다.
	//==============================================================================
	#[test]
	fn query_name_becomes_variable_name() {
		assert_eq!(convert_query_name_to_variable_name("safeArea"), "VANILLA_SAFE_AREA");
		assert_eq!(convert_query_name_to_variable_name("quality"), "VANILLA_QUALITY");
	}
}
