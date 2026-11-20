//==============================================================================
// 웹 플랫폼. (wasm32)
// - 원본처럼 캔버스를 창 안쪽에 꽉 채우고, 원본과 같은 옵션으로 WebGL2 컨텍스트를 얻는다.
// - request_animation_frame 콜백마다 엔진 프레임 하나. (브라우저가 수직 동기에 맞춰 부르고, 콜백이 끝나면 화면을 넘긴다)
// - 입력은 아직 키보드만 받는다. 입력 필드의 글 입력(숨긴 DOM <input>, IME 조합, 문맥 메뉴)과 클립보드는 web/textinput.rs 가 맡는다.
// - 오디오: 원본 engine.js 처럼 click / touchstart / keydown(설정하면 touchend / pointerup 도)은 user gesture 로 보고
//   engine.resume_on_user_gesture(), focus 와 페이지가 다시 보일 때는 engine.resume() 을 부른다.
//==============================================================================
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::EngineRunner;
use crate::core::graphic::Graphic;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

// 글꼴 바이트가 없을 때 브라우저 시스템 글꼴로 글자 굽기. (원본처럼 Canvas2D)
pub(crate) mod systemfont;
mod textinput;
pub use textinput::{read_clipboard_text, write_clipboard_text};

// 엔진이 들고 있는 창 손잡이. (커서를 바꿀 때)
pub type PlatformWindow = web_sys::HtmlCanvasElement;

// 프레임 콜백을 담는 칸. (콜백이 자기 자신을 다시 걸어야 해서 Rc 로 나눠 든다)
type FrameCallbackCell = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

//==============================================================================
// 커서 보이기 설정.
//==============================================================================
pub fn set_cursor_visible(platform_window: Option<&PlatformWindow>, is_visible: bool) {
	if let Some(canvas) = platform_window {
		let cursor_value = if is_visible {
			""
		}
		else {
			"none"
		};
		let _ = canvas.style().set_property("cursor", cursor_value);
	}
}

//==============================================================================
// 창 제목 설정. (문서 제목)
//==============================================================================
pub fn set_title(_platform_window: Option<&PlatformWindow>, title: &str) {
	if let Some(document) = web_sys::window().and_then(|window| window.document()) {
		document.set_title(title);
	}
}

//==============================================================================
// 애셋 바이트 읽기. (fetch 로 받고, 받으면 끝낸다. 그 사이 프레임은 계속 돈다)
//==============================================================================
pub fn request_asset_bytes(asset_location: String, load_handle: AssetLoadHandle) {
	wasm_bindgen_futures::spawn_local(async move {
		let result = fetch_bytes(&asset_location).await;
		load_handle.finish(result);
	});
}

//==============================================================================
// 주소에서 바이트 받기.
//==============================================================================
async fn fetch_bytes(asset_location: &str) -> Result<Vec<u8>, String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let response_promise = window.fetch_with_str(asset_location);
	let response_value = wasm_bindgen_futures::JsFuture::from(response_promise).await.map_err(|error| format!("애셋 받기 실패 ({}): {}", asset_location, describe_js_error(error)))?;
	let response: web_sys::Response = response_value.dyn_into().map_err(|_| format!("애셋 응답이 아님 ({})", asset_location))?;
	if !response.ok() {
		return Err(format!("애셋 받기 실패 ({}): HTTP {}", asset_location, response.status()));
	}
	let array_buffer_promise = response.array_buffer().map_err(|error| format!("애셋 본문 읽기 실패 ({}): {}", asset_location, describe_js_error(error)))?;
	let array_buffer = wasm_bindgen_futures::JsFuture::from(array_buffer_promise).await.map_err(|error| format!("애셋 본문 읽기 실패 ({}): {}", asset_location, describe_js_error(error)))?;
	let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
	Ok(bytes)
}

//==============================================================================
// 캔버스 상태. (마지막으로 맞춘 창 크기와 배율)
//==============================================================================
struct WindowState {
	canvas: web_sys::HtmlCanvasElement,
	client_width: f64,
	client_height: f64,
	device_pixel_ratio: f64,
}

//==============================================================================
// 웹 실행자.
//==============================================================================
struct WebApplication {
	engine_runner: EngineRunner,
	window_state: Option<WindowState>,
}

//==============================================================================
// 시작. (캔버스와 WebGL2 를 잡고 request_animation_frame 을 걸어 둔 뒤 바로 돌아온다)
//==============================================================================
pub fn run(engine_runner: EngineRunner) -> Result<(), String> {
	let web_application_cell = Rc::new(RefCell::new(WebApplication {
		engine_runner,
		window_state: None,
	}));
	{
		let mut web_application = web_application_cell.borrow_mut();
		web_application.create_canvas()?;
		web_application.engine_runner.apply_pending_scenes();
	}
	add_keyboard_listeners(&web_application_cell)?;
	add_audio_listeners(&web_application_cell)?;
	let canvas = web_application_cell.borrow().engine_runner.engine.platform_window.clone();
	if let Some(canvas) = canvas {
		textinput::add_text_input_listeners(&canvas)?;
	}

	// 프레임 콜백. (자기 자신을 다시 걸어야 하므로 Rc 안에 넣어 둔다. 페이지가 살아 있는 동안 계속 산다)
	let frame_callback_cell: FrameCallbackCell = Rc::new(RefCell::new(None));
	let frame_callback_cell_for_loop = frame_callback_cell.clone();
	let web_application_cell_for_loop = web_application_cell.clone();
	let mut last_frame_timestamp = 0.0f64;
	*frame_callback_cell.borrow_mut() = Some(Closure::new(move |timestamp: f64| {
		let mut web_application = web_application_cell_for_loop.borrow_mut();
		if web_application.engine_runner.engine.is_exit_requested() {
			return;
		}
		// 최대 FPS 제한. (0 이면 화면 주사율 그대로)
		let maximum_frame_per_second = web_application.engine_runner.engine.get_engine_configuration().maximum_frame_per_second as f64;
		let is_frame_turn = maximum_frame_per_second <= 0.0 || timestamp - last_frame_timestamp >= 1000.0 / maximum_frame_per_second - 1.0;
		if is_frame_turn {
			last_frame_timestamp = timestamp;
			web_application.resize_if_changed();
			textinput::begin_frame(&mut web_application.engine_runner.engine);
			web_application.engine_runner.update_engine();
			textinput::apply_frame(&web_application.engine_runner.engine);
		}
		if web_application.engine_runner.engine.is_exit_requested() {
			return;
		}
		if let Some(frame_callback) = frame_callback_cell_for_loop.borrow().as_ref() {
			request_animation_frame(frame_callback);
		}
	}));
	if let Some(frame_callback) = frame_callback_cell.borrow().as_ref() {
		request_animation_frame(frame_callback);
	}
	Ok(())
}

//==============================================================================
// 다음 프레임 요청.
//==============================================================================
fn request_animation_frame(frame_callback: &Closure<dyn FnMut(f64)>) {
	if let Some(window) = web_sys::window() {
		let _ = window.request_animation_frame(frame_callback.as_ref().unchecked_ref());
	}
}

//==============================================================================
// 키보드 입력. (KeyboardEvent.code 는 winit KeyCode 이름과 같은 W3C 규약)
//==============================================================================
fn add_keyboard_listeners(web_application_cell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let web_application_cell_for_key_down = web_application_cell.clone();
	let key_down_callback: Closure<dyn FnMut(web_sys::KeyboardEvent)> = Closure::new(move |keyboard_event: web_sys::KeyboardEvent| {
		if keyboard_event.repeat() {
			return;
		}
		let key_code = keyboard_event.code();
		web_application_cell_for_key_down.borrow_mut().engine_runner.engine.get_input_manager_mut().push_key(&key_code);
	});
	let web_application_cell_for_key_up = web_application_cell.clone();
	let key_up_callback: Closure<dyn FnMut(web_sys::KeyboardEvent)> = Closure::new(move |keyboard_event: web_sys::KeyboardEvent| {
		let key_code = keyboard_event.code();
		web_application_cell_for_key_up.borrow_mut().engine_runner.engine.get_input_manager_mut().pop_key(&key_code);
	});
	let web_application_cell_for_blur = web_application_cell.clone();
	let blur_callback: Closure<dyn FnMut()> = Closure::new(move || {
		web_application_cell_for_blur.borrow_mut().engine_runner.engine.get_input_manager_mut().clear();
	});
	window.add_event_listener_with_callback("keydown", key_down_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	window.add_event_listener_with_callback("keyup", key_up_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	window.add_event_listener_with_callback("blur", blur_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	key_down_callback.forget();
	key_up_callback.forget();
	blur_callback.forget();
	Ok(())
}

//==============================================================================
// 오디오 재개 이벤트. (원본 setupAllDocumentEvents 의 오디오 부분)
// - 프레임 안에서 이벤트가 바로 불려도 겹쳐 빌리지 않게 try_borrow 로 본다. (못 빌리면 다음 이벤트에 맡긴다)
//==============================================================================
fn add_audio_listeners(web_application_cell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let document = window.document().ok_or_else(|| String::from("document 없음"))?;

	// click / touchstart / keydown 은 진짜 user gesture.
	let web_application_cell_for_gesture = web_application_cell.clone();
	let user_gesture_callback: Closure<dyn FnMut()> = Closure::new(move || {
		if let Ok(web_application) = web_application_cell_for_gesture.try_borrow() {
			web_application.engine_runner.engine.resume_on_user_gesture();
		}
	});
	let mut user_gesture_event_names = vec!["click", "touchstart", "keydown"];
	let is_resume_audio_on_touch_end = web_application_cell.borrow().engine_runner.engine.get_engine_configuration().resume_audio_on_touch_end;
	if is_resume_audio_on_touch_end {
		user_gesture_event_names.push("touchend");
		user_gesture_event_names.push("pointerup");
	}
	for event_name in user_gesture_event_names {
		window.add_event_listener_with_callback(event_name, user_gesture_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	}

	// focus 와 다시 보일 때는 일반 재개. (gesture 전이면 아무것도 하지 않는다)
	let web_application_cell_for_focus = web_application_cell.clone();
	let focus_callback: Closure<dyn FnMut()> = Closure::new(move || {
		if let Ok(web_application) = web_application_cell_for_focus.try_borrow() {
			web_application.engine_runner.engine.resume();
		}
	});
	let web_application_cell_for_visibility = web_application_cell.clone();
	let visibility_callback: Closure<dyn FnMut()> = Closure::new(move || {
		let is_visible = web_sys::window()
			.and_then(|window| window.document())
			.map(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
			.unwrap_or(false);
		if !is_visible {
			return;
		}
		if let Ok(web_application) = web_application_cell_for_visibility.try_borrow() {
			web_application.engine_runner.engine.resume();
		}
	});
	window.add_event_listener_with_callback("focus", focus_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	document.add_event_listener_with_callback("visibilitychange", visibility_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	user_gesture_callback.forget();
	focus_callback.forget();
	visibility_callback.forget();
	Ok(())
}

//==============================================================================
// 자바스크립트 오류를 글로.
//==============================================================================
fn describe_js_error(error: JsValue) -> String {
	format!("{:?}", error)
}

impl WebApplication {
	//==============================================================================
	// 캔버스와 WebGL2 컨텍스트, 그래픽 만들기.
	//==============================================================================
	fn create_canvas(&mut self) -> Result<(), String> {
		let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
		let document = window.document().ok_or_else(|| String::from("document 없음"))?;
		let canvas_id = self.engine_runner.engine.get_engine_configuration().canvas_id.clone();
		let canvas: web_sys::HtmlCanvasElement = match document.get_element_by_id(&canvas_id) {
			Some(element) => {
				element.dyn_into::<web_sys::HtmlCanvasElement>().map_err(|_| format!("#{} 가 캔버스가 아님", canvas_id))?
			}
			None => {
				let element = document.create_element("canvas").map_err(describe_js_error)?;
				element.set_id(&canvas_id);
				let body = document.body().ok_or_else(|| String::from("body 없음"))?;
				body.append_child(&element).map_err(describe_js_error)?;
				element.dyn_into::<web_sys::HtmlCanvasElement>().map_err(|_| String::from("캔버스 생성 실패"))?
			}
		};

		// 원본 graphic.js 와 같은 컨텍스트 옵션.
		let context_options = js_sys::Object::new();
		for (option_name, option_value) in [("alpha", false), ("stencil", true), ("premultipliedAlpha", true), ("preserveDrawingBuffer", false)] {
			js_sys::Reflect::set(&context_options, &JsValue::from_str(option_name), &JsValue::from_bool(option_value)).map_err(describe_js_error)?;
		}
		let context = canvas
			.get_context_with_context_options("webgl2", &context_options)
			.map_err(describe_js_error)?
			.ok_or_else(|| String::from("WebGL2 Not Supported."))?
			.dyn_into::<web_sys::WebGl2RenderingContext>()
			.map_err(|_| String::from("WebGL2 컨텍스트 변환 실패"))?;
		let gl = glow::Context::from_webgl2_context(context);
		let graphic = Graphic::new(gl)?;

		self.engine_runner.attach_graphic(graphic);
		self.engine_runner.engine.platform_window = Some(canvas.clone());
		self.window_state = Some(WindowState {
			canvas,
			client_width: 0.0,
			client_height: 0.0,
			device_pixel_ratio: 0.0,
		});
		self.resize_if_changed();
		Ok(())
	}

	//==============================================================================
	// 창 크기나 배율이 바뀌었으면 캔버스를 맞추고 뷰 영역을 다시 계산.
	// (원본 view_manager.calculate_view_rect 처럼 캔버스를 창 안쪽에 꽉 채운다)
	//==============================================================================
	fn resize_if_changed(&mut self) {
		let window = match web_sys::window() {
			Some(window) => {
				window
			}
			None => {
				return;
			}
		};
		let client_width = window.inner_width().ok().and_then(|value| value.as_f64()).unwrap_or(0.0);
		let client_height = window.inner_height().ok().and_then(|value| value.as_f64()).unwrap_or(0.0);
		let device_pixel_ratio = if window.device_pixel_ratio() > 0.0 {
			window.device_pixel_ratio()
		}
		else {
			1.0
		};
		let window_state = match self.window_state.as_mut() {
			Some(window_state) => {
				window_state
			}
			None => {
				return;
			}
		};
		if window_state.client_width == client_width && window_state.client_height == client_height && window_state.device_pixel_ratio == device_pixel_ratio {
			return;
		}
		window_state.client_width = client_width;
		window_state.client_height = client_height;
		window_state.device_pixel_ratio = device_pixel_ratio;
		let canvas_style = window_state.canvas.style();
		let _ = canvas_style.set_property("width", &format!("{}px", client_width));
		let _ = canvas_style.set_property("height", &format!("{}px", client_height));
		let canvas_pixel_width = (client_width * device_pixel_ratio).round() as u32;
		let canvas_pixel_height = (client_height * device_pixel_ratio).round() as u32;
		window_state.canvas.set_width(canvas_pixel_width);
		window_state.canvas.set_height(canvas_pixel_height);
		self.engine_runner.engine.get_view_manager_mut().calculate_view_rect(canvas_pixel_width, canvas_pixel_height, device_pixel_ratio as f32);
		self.engine_runner.notify_scene_resize();
	}
}
