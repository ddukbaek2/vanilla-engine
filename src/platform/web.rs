//==============================================================================
//! 웹 플랫폼. (wasm32)
//! - 원본처럼 캔버스를 창 안쪽에 꽉 채우고, 원본과 같은 옵션으로 WebGL2 컨텍스트를 얻는다.
//! - request_animation_frame 콜백마다 엔진 프레임 하나. (브라우저가 수직 동기에 맞춰 부르고, 콜백이 끝나면 화면을 넘긴다)
//! - 키보드, 마우스, 손가락을 받는다. 입력 필드의 글 입력(숨긴 DOM <input>, IME 조합, 문맥 메뉴)과 클립보드는 web/textinput.rs 가 맡는다.
//! - 키를 누르면 입력 매니저의 막을 키(set_prevented_keys)면 글자 입력칸에 쓰는 중이 아닐 때 기본 동작을 막는다. 마지막으로 쓴 장치를 키, 마우스, 손가락 사건마다 적는다.
//! - 오디오: 원본 engine.js 처럼 click / touchstart / keydown(설정하면 touchend / pointerup 도)은 user gesture 로 보고
//!   engine.resume_on_user_gesture(), focus 와 페이지가 다시 보일 때는 engine.resume() 을 부른다.
//! - 페이지: visibilitychange, blur, focus, pointerdown 을 엔진의 페이지 이벤트로 넘기고, 다시 보이면 on_page_visibility_restored,
//!   문서 resume 과 pageshow(persisted) 면 on_page_process_restored 를 부른다. (원본 setupAllDocumentEvents)
//! - 최대 FPS 는 원본처럼 세기 시작한 뒤 처리한 프레임 수로 차례를 정한다. (platform/framelimiter.rs, 90 Hz 에서 60 FPS 도 평균 60)
//! - 캔버스의 webglcontextlost, webglcontextrestored 에서 원본 WebGLStateCache 처럼 GL 상태 기억을 잊는다.
//! - 키, 마우스, 손가락, 페이지 사건은 프레임 콜백 안에서 다시 들어와도(게임 코드의 dispatchEvent, focus() 같은 것) 겹쳐 빌리지 않는다.
//!   바로 빌릴 수 있으면 그 자리에서 처리하고, 못 빌리면 줄에 쌓았다가 프레임 콜백 끝에 온 차례대로 처리한다. (platform/eventqueue.rs)
//! - exit_application: 원본처럼 Capacitor 의 App.exitApp, 없으면 window.close 를 시도한다.
//!
//! # 예제
//! 페이지 쿼리 값, 세이프 에어리어, 주된 손 읽기 (데스크탑과 같은 이름으로 부른다)
//! ```rust,no_run
//! use vanilla::platform;
//!
//! let quality = platform::read_page_query_value("quality").unwrap_or_else(|| String::from("high"));
//! let safe_area_insets = platform::read_safe_area_insets();
//! let is_touch_first = platform::is_coarse_pointer();
//! println!("{} {:?} {}", quality, safe_area_insets, is_touch_first);
//! ```
//==============================================================================
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::{EngineRunner, PageEvent};
use crate::platform::eventqueue::EventQueue;
use crate::platform::framelimiter::FrameLimiter;
use crate::core::graphic::Graphic;
use crate::core::inputmanager::{InputDevice, MouseButton};
use crate::core::pointercursor::CursorValue;
use crate::base::vector2::Vector2;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

// 글자 굽기와 재기. (원본처럼 늘 브라우저 Canvas2D 로, 올린 글꼴 바이트는 document.fonts 의 FontFace 로 올린다)
pub(crate) mod systemfont;
mod textinput;
pub use textinput::{read_clipboard_text, read_clipboard_text_with_callback, read_optional_clipboard_text_with_callback, write_clipboard_text};
pub(crate) use textinput::request_clipboard_paste;

/// 엔진이 들고 있는 창 손잡이. (커서를 바꿀 때)
pub type PlatformWindow = web_sys::HtmlCanvasElement;

/// 프레임 콜백을 담는 칸. (콜백이 자기 자신을 다시 걸어야 해서 Rc 로 나눠 든다)
type FrameCallbackCell = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

//==============================================================================
/// 커서 보이기 설정.
/// - 캔버스의 cursor 스타일을 감출 때 none, 보일 때 빈 값(브라우저 기본)으로 쓴다.
///
/// # 인자
/// - `platform_window`: 엔진 캔버스. (None 이면 아무것도 하지 않는다)
/// - `is_visible`: 보이려면 true.
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
/// 커서 모양 설정. (캔버스의 cursor 스타일에 CSS 값을 쓴다, 빈 값이면 브라우저의 기본, 그림 커서는 CSS 값에 그림이 든다)
///
/// # 인자
/// - `platform_window`: 엔진 캔버스. (None 이면 아무것도 하지 않는다)
/// - `cursor_value`: 커서 값. (css_value 를 그대로 쓴다)
//==============================================================================
pub fn set_cursor(platform_window: Option<&PlatformWindow>, cursor_value: &CursorValue) {
	if let Some(canvas) = platform_window {
		let _ = canvas.style().set_property("cursor", &cursor_value.css_value);
	}
}

//==============================================================================
/// 주된 손이 손가락인지. (CSS 의 pointer: coarse)
///
/// # 돌려줌
/// matchMedia("(pointer: coarse)") 가 맞으면 true. window 가 없거나 읽지 못하면 false.
//==============================================================================
pub fn is_coarse_pointer() -> bool {
	web_sys::window()
		.and_then(|window| window.match_media("(pointer: coarse)").ok().flatten())
		.map(|media_query_list| media_query_list.matches())
		.unwrap_or(false)
}

//==============================================================================
/// 문서의 초점이 글자 입력칸에 있는지. (글자 종류의 input, textarea, contenteditable, select)
///
/// # 돌려줌
/// document.activeElement 가 글자 입력칸이면 true. (button, checkbox 같은 input 은 false, 초점이 없으면 false)
//==============================================================================
pub fn is_text_field_focused() -> bool {
	let active_element = match web_sys::window().and_then(|window| window.document()).and_then(|document| document.active_element()) {
		Some(active_element) => {
			active_element
		}
		None => {
			return false;
		}
	};
	if let Some(html_element) = active_element.dyn_ref::<web_sys::HtmlElement>() {
		if html_element.is_content_editable() {
			return true;
		}
	}
	let tag_name = active_element.tag_name();
	if tag_name == "TEXTAREA" || tag_name == "SELECT" {
		return true;
	}
	if tag_name != "INPUT" {
		return false;
	}
	let input_type = match active_element.dyn_ref::<web_sys::HtmlInputElement>() {
		Some(input_element) => {
			input_element.type_().to_lowercase()
		}
		None => {
			String::from("text")
		}
	};
	let non_text_input_types = ["button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit"];
	!non_text_input_types.contains(&input_type.as_str())
}

//==============================================================================
/// 페이지 주소의 쿼리 값. (원본처럼 location.search 를 URLSearchParams 로 읽어 처음 값, 없으면 None)
///
/// # 인자
/// - `name`: 쿼리 이름. (safeArea 같은)
///
/// # 돌려줌
/// 그 이름의 첫 값. 쿼리에 없거나 window, location, URLSearchParams 를 읽지 못하면 None.
//==============================================================================
pub fn read_page_query_value(name: &str) -> Option<String> {
	let window = web_sys::window()?;
	let location = js_sys::Reflect::get(&window, &JsValue::from_str("location")).ok()?;
	let search_text = js_sys::Reflect::get(&location, &JsValue::from_str("search")).ok()?.as_string()?;
	let constructor: js_sys::Function = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("URLSearchParams")).ok()?.dyn_into().ok()?;
	let search_parameters = js_sys::Reflect::construct(&constructor, &js_sys::Array::of1(&JsValue::from_str(&search_text))).ok()?;
	let get_function: js_sys::Function = js_sys::Reflect::get(&search_parameters, &JsValue::from_str("get")).ok()?.dyn_into().ok()?;
	get_function.call1(&search_parameters, &JsValue::from_str(name)).ok()?.as_string()
}

//==============================================================================
/// 화면(client) 좌표에서 캔버스의 왼쪽 위. (원본 canvas.getBoundingClientRect() 의 left, top, 캔버스가 없으면 0)
///
/// # 돌려줌
/// 캔버스의 왼쪽 위 자리. (CSS 픽셀, 엔진 캔버스가 아직 없으면 0, 0)
//==============================================================================
pub fn read_canvas_client_position() -> Vector2 {
	let canvas = ENGINE_CANVAS.with(|engine_canvas| engine_canvas.borrow().clone());
	match canvas {
		Some(canvas) => {
			let canvas_rect = canvas.get_bounding_client_rect();
			Vector2::create(canvas_rect.left() as f64, canvas_rect.top() as f64)
		}
		None => {
			Vector2::zero()
		}
	}
}

//==============================================================================
/// 검증용 화면 저장. (원본처럼 내려받기, PNG 를 Blob 주소로 만들어 a 요소의 download 로 누른다)
///
/// # 인자
/// - `file_name`: 내려받을 파일 이름. (a 요소의 download 값)
/// - `png_bytes`: PNG 파일 바이트.
///
/// # 돌려줌
/// 내려받기를 눌렀으면 Ok. window, document, body, Blob, URL.createObjectURL 이 없거나 DOM 호출이 실패하면 Err(까닭 글).
//==============================================================================
pub fn save_capture_file(file_name: &str, png_bytes: &[u8]) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let document = window.document().ok_or_else(|| String::from("document 없음"))?;
	let byte_array = js_sys::Uint8Array::from(png_bytes);
	let blob_options = js_sys::Object::new();
	js_sys::Reflect::set(&blob_options, &JsValue::from_str("type"), &JsValue::from_str("image/png")).map_err(describe_js_error)?;
	let blob_constructor: js_sys::Function = js_sys::Reflect::get(&window, &JsValue::from_str("Blob")).map_err(describe_js_error)?.dyn_into().map_err(|_| String::from("Blob 없음"))?;
	let blob = js_sys::Reflect::construct(&blob_constructor, &js_sys::Array::of2(&js_sys::Array::of1(&byte_array), &blob_options)).map_err(describe_js_error)?;
	let url_object = js_sys::Reflect::get(&window, &JsValue::from_str("URL")).map_err(describe_js_error)?;
	let create_object_url: js_sys::Function = js_sys::Reflect::get(&url_object, &JsValue::from_str("createObjectURL")).map_err(describe_js_error)?.dyn_into().map_err(|_| String::from("URL.createObjectURL 없음"))?;
	let object_url = create_object_url.call1(&url_object, &blob).map_err(describe_js_error)?;
	let anchor_element: web_sys::HtmlElement = document.create_element("a").map_err(describe_js_error)?.dyn_into().map_err(|_| String::from("a 요소 아님"))?;
	anchor_element.set_attribute("href", &object_url.as_string().unwrap_or_default()).map_err(describe_js_error)?;
	anchor_element.set_attribute("download", file_name).map_err(describe_js_error)?;
	let body = document.body().ok_or_else(|| String::from("body 없음"))?;
	body.append_child(&anchor_element).map_err(describe_js_error)?;
	anchor_element.click();
	anchor_element.remove();
	if let Ok(revoke_object_url) = js_sys::Reflect::get(&url_object, &JsValue::from_str("revokeObjectURL")).and_then(|value| value.dyn_into::<js_sys::Function>().map_err(JsValue::from)) {
		let _ = revoke_object_url.call1(&url_object, &object_url);
	}
	Ok(())
}

//==============================================================================
/// 페이지 전체 크기. (원본 window.innerWidth, innerHeight)
///
/// # 돌려줌
/// 창 안쪽 크기. (CSS 픽셀) window 가 없거나 값을 읽지 못하면 None.
//==============================================================================
pub fn read_client_native_size() -> Option<Vector2> {
	let window = web_sys::window()?;
	let client_width = window.inner_width().ok()?.as_f64()?;
	let client_height = window.inner_height().ok()?.as_f64()?;
	Some(Vector2::create(client_width as f64, client_height as f64))
}

thread_local! {
	/// 엔진이 그리는 캔버스. (read_canvas_client_position 이 자리를 읽는다)
	static ENGINE_CANVAS: RefCell<Option<web_sys::HtmlCanvasElement>> = const { RefCell::new(None) };
	/// env(safe-area-inset-*) 를 재는 숨은 요소. (처음 잴 때 만들어 body 에 붙여 둔다)
	static SAFE_AREA_PROBE_ELEMENT: RefCell<Option<web_sys::HtmlElement>> = const { RefCell::new(None) };
	/// 엔진의 WebGL2 컨텍스트. (glow 가 감싼 것과 같은 컨텍스트, is_gl_binding_present 가 묶인 것을 읽는다)
	static ENGINE_GL_CONTEXT: RefCell<Option<web_sys::WebGl2RenderingContext>> = const { RefCell::new(None) };
	/// 웹 실행자를 빌리지 못해(프레임 콜백 안) 쌓아 둔 사건. (프레임 콜백 끝에 처리한다)
	static WEB_EVENT_QUEUE: EventQueue<WebApplication> = EventQueue::new();
}

//==============================================================================
/// 사건을 웹 실행자에 보내기. (바로 빌릴 수 있으면 쌓인 것부터 처리하고 그 자리에서, 프레임 콜백 안이라 못 빌리면 줄에 쌓는다)
//==============================================================================
fn dispatch_web_event(web_application_cell: &RefCell<WebApplication>, handler: impl FnOnce(&mut WebApplication) + 'static) {
	WEB_EVENT_QUEUE.with(|web_event_queue| {
		web_event_queue.dispatch(web_application_cell, handler);
	});
}

//==============================================================================
/// 기본 프레임버퍼가 안티에일리어싱(멀티샘플)인지. (원본 getContextAttributes().antialias 가 false 가 아니면 true, 컨텍스트가 없으면 true)
//==============================================================================
pub(crate) fn is_default_framebuffer_antialiased() -> bool {
	ENGINE_GL_CONTEXT.with(|engine_gl_context| {
		let context = match engine_gl_context.borrow().as_ref() {
			Some(context) => {
				context.clone()
			}
			None => {
				return true;
			}
		};
		let context_value: &JsValue = context.as_ref();
		let attribute_reader = match js_sys::Reflect::get(context_value, &JsValue::from_str("getContextAttributes")).ok().and_then(|value| value.dyn_into::<js_sys::Function>().ok()) {
			Some(attribute_reader) => {
				attribute_reader
			}
			None => {
				return true;
			}
		};
		let context_attributes = match attribute_reader.call0(context_value) {
			Ok(context_attributes) => {
				context_attributes
			}
			Err(_) => {
				return true;
			}
		};
		if context_attributes.is_null() || context_attributes.is_undefined() {
			return true;
		}
		js_sys::Reflect::get(&context_attributes, &JsValue::from_str("antialias")).ok().and_then(|value| value.as_bool()) != Some(false)
	})
}

//==============================================================================
/// GL 의 묶인 것(..._BINDING, CURRENT_PROGRAM)이 있는지. (원본 getParameter 가 null 이 아닌지)
/// - glow 웹 컨텍스트는 묶인 것이 없을 때(null) 읽으면 "A resource was created externally" 로 패닉하고 감싼 컨텍스트를 내주지 않는다.
///   그래서 컨텍스트를 만들 때 같은 것을 따로 들고 있다가 여기서 읽는다. 컨텍스트가 아직 없으면 false.
//==============================================================================
pub(crate) fn is_gl_binding_present(parameter: u32) -> bool {
	ENGINE_GL_CONTEXT.with(|engine_gl_context| {
		match engine_gl_context.borrow().as_ref() {
			Some(context) => {
				match context.get_parameter(parameter) {
					Ok(value) => {
						!value.is_null() && !value.is_undefined()
					}
					Err(_) => {
						false
					}
				}
			}
			None => {
				false
			}
		}
	})
}

//==============================================================================
/// 렌더버퍼 포맷이 받는 멀티샘플 표본 수. (원본 getInternalformatParameter(RENDERBUFFER, 포맷, SAMPLES), 큰 것부터, 못 읽으면 빈 목록)
/// - WebGL2 는 NUM_SAMPLE_COUNTS 를 받지 않고 SAMPLES 로 Int32Array 를 돌려준다. glow 는 길이를 먼저 알아야 해서 여기서 바로 읽는다.
//==============================================================================
pub(crate) fn read_renderbuffer_sample_counts(internal_format: u32) -> Vec<i32> {
	ENGINE_GL_CONTEXT.with(|engine_gl_context| {
		let engine_gl_context = engine_gl_context.borrow();
		let Some(context) = engine_gl_context.as_ref() else {
			return Vec::new();
		};
		match context.get_internalformat_parameter(web_sys::WebGl2RenderingContext::RENDERBUFFER, internal_format, web_sys::WebGl2RenderingContext::SAMPLES) {
			Ok(value) => {
				match value.dyn_ref::<js_sys::Int32Array>() {
					Some(sample_count_array) => {
						sample_count_array.to_vec()
					}
					None => {
						Vec::new()
					}
				}
			}
			Err(_) => {
				Vec::new()
			}
		}
	})
}

//==============================================================================
/// CSS env(safe-area-inset-*) 세이프 에어리어 인셋. (원본 viewManager.readSafeAreaFromEnvironment, CSS 픽셀 [위, 오른쪽, 아래, 왼쪽])
/// - 숨은 요소의 padding 에 env() 를 걸고 계산된 값을 parseFloat 로 읽는다. 문서나 body 가 없으면 0.
///
/// # 돌려줌
/// [위, 오른쪽, 아래, 왼쪽] 인셋. (CSS 픽셀, 읽지 못한 칸은 0)
//==============================================================================
pub fn read_safe_area_insets() -> [f64; 4] {
	let read_insets = || -> Option<[f64; 4]> {
		let window = web_sys::window()?;
		let document = window.document()?;
		let body = document.body()?;
		let probe_element = SAFE_AREA_PROBE_ELEMENT.with(|probe_element_cell| -> Option<web_sys::HtmlElement> {
			if let Some(probe_element) = probe_element_cell.borrow().as_ref() {
				return Some(probe_element.clone());
			}
			let probe_element: web_sys::HtmlElement = document.create_element("div").ok()?.dyn_into().ok()?;
			probe_element.style().set_css_text("position:fixed;left:0;top:0;width:0;height:0;visibility:hidden;pointer-events:none;padding-top:env(safe-area-inset-top);padding-right:env(safe-area-inset-right);padding-bottom:env(safe-area-inset-bottom);padding-left:env(safe-area-inset-left);");
			body.append_child(&probe_element).ok()?;
			*probe_element_cell.borrow_mut() = Some(probe_element.clone());
			Some(probe_element)
		})?;
		let computed_style = window.get_computed_style(&probe_element).ok()??;
		let read_padding = |property_name: &str| -> f64 {
			let value = crate::core::viewmanager::parse_leading_float(&computed_style.get_property_value(property_name).unwrap_or_default());
			if value.is_nan() {
				0.0
			}
			else {
				value as f64
			}
		};
		Some([read_padding("padding-top"), read_padding("padding-right"), read_padding("padding-bottom"), read_padding("padding-left")])
	};
	read_insets().unwrap_or([0.0, 0.0, 0.0, 0.0])
}

//==============================================================================
/// 창 제목 설정. (문서 제목)
///
/// # 인자
/// - `_platform_window`: 엔진 캔버스. (쓰지 않는다, 데스크탑과 같은 꼴을 맞춘 것)
/// - `title`: 제목 글. (document.title)
//==============================================================================
pub fn set_title(_platform_window: Option<&PlatformWindow>, title: &str) {
	if let Some(document) = web_sys::window().and_then(|window| window.document()) {
		document.set_title(title);
	}
}

//==============================================================================
/// 애셋 바이트 읽기. (fetch 로 받고, 받으면 끝낸다. 그 사이 프레임은 계속 돈다)
/// - 응답의 content-type 을 손잡이에 적는다. (원본 Material.loadImageDescription 의 response.headers.get("content-type"))
/// - 응답이 그림 형(image/...)이거나 주소가 그림 확장자(png, jpg, gif, webp, bmp, ico, svg, avif 등)면 끝내기 전에 원본처럼 브라우저가 풀게 해 손잡이에 둔다.
///   (ImageAsset, AssetLoader 가 그대로 쓴다) 원본 HTMLImageElement 처럼 응답의 content-type 을 그대로 형으로 넘겨, 브라우저가 SVG 는 그 형(image/svg+xml)으로,
///   그 밖의 그림은 내용으로 가린다. 머리가 없는 응답만 확장자의 형을 쓴다. data: 주소는 fetch 가 주소의 형을 content-type 으로 준다.
/// - 그림으로 풀어야 하는 손잡이(ImageAsset, AssetLoader 의 그림)는 형과 확장자가 그림이 아니어도 형 없이 브라우저가 내용으로 가려 푼다. (원본 img.src 와 같다, 웹에는 엔진 해독기가 없다)
///
/// # 인자
/// - `asset_location`: 받을 주소. (엔진이 애셋 경로로 만든 것, data: 주소도 된다)
/// - `load_handle`: 다 받으면 바이트나 오류 글로 끝낼 손잡이.
/// - `is_no_cache`: true 면 fetch 에 cache: "no-cache" 를 붙인다. (원본 TextAsset)
//==============================================================================
pub fn request_asset_bytes(asset_location: String, load_handle: AssetLoadHandle, is_no_cache: bool) {
	wasm_bindgen_futures::spawn_local(async move {
		let result = match fetch_bytes(&asset_location, is_no_cache).await {
			Ok((bytes, content_type)) => {
				let browser_mime_type = read_browser_image_mime_type(&asset_location, content_type.as_deref()).or_else(|| load_handle.is_image_decode_requested().then(String::new));
				if let Some(mime_type) = browser_mime_type {
					if let Some(image) = decode_browser_image(&bytes, &mime_type).await {
						load_handle.set_decoded_image(image);
					}
				}
				load_handle.set_content_type(content_type);
				Ok(bytes)
			}
			Err(error_message) => {
				Err(error_message)
			}
		};
		load_handle.finish(result);
	});
}

//==============================================================================
/// 브라우저가 풀 그림의 형. (응답이 그림 형이거나 주소가 그림 확장자일 때, 형은 응답의 content-type 이고 머리가 없으면 확장자의 형. 그림이 아니면 None)
//==============================================================================
fn read_browser_image_mime_type(asset_location: &str, content_type: Option<&str>) -> Option<String> {
	let extension_mime_type = read_image_mime_type(asset_location);
	let is_image_content_type = content_type.map(|content_type| content_type.trim_start().to_ascii_lowercase().starts_with("image/")).unwrap_or(false);
	if !is_image_content_type && extension_mime_type.is_none() {
		return None;
	}
	content_type.map(String::from).or(extension_mime_type.map(String::from))
}

//==============================================================================
/// 주소의 확장자로 그림 형식 이름. (그림이 아니면 None)
//==============================================================================
fn read_image_mime_type(asset_location: &str) -> Option<&'static str> {
	let path = asset_location.split(['?', '#']).next()?;
	let file_name = path.rsplit('/').next()?;
	let (_, extension) = file_name.rsplit_once('.')?;
	match extension.to_lowercase().as_str() {
		"png" | "apng" => {
			Some("image/png")
		}
		"jpg" | "jpeg" | "jfif" | "pjpeg" | "pjp" => {
			Some("image/jpeg")
		}
		"gif" => {
			Some("image/gif")
		}
		"webp" => {
			Some("image/webp")
		}
		"bmp" => {
			Some("image/bmp")
		}
		"ico" | "cur" => {
			Some("image/x-icon")
		}
		"svg" => {
			Some("image/svg+xml")
		}
		"avif" => {
			Some("image/avif")
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 가진 바이트를 브라우저로 풀기. (원본 ImageAsset.loadFromBytes 처럼 형 없는 Blob 을 img 로 풀고, 풀면 그림을 담아 바이트로 끝내고 못 풀면 실패로 끝낸다)
///
/// # 인자
/// - `bytes`: 그림 파일 바이트.
/// - `load_handle`: 읽는 중인 손잡이. (ImageAsset::load_from_bytes 가 만든 것)
//==============================================================================
pub(crate) fn request_image_bytes_decode(bytes: Vec<u8>, load_handle: AssetLoadHandle) {
	wasm_bindgen_futures::spawn_local(async move {
		match decode_browser_image(&bytes, "").await {
			Some(image) => {
				load_handle.set_decoded_image(image);
				load_handle.finish(Ok(bytes));
			}
			None => {
				let error_message = format!("Load fail: {}", load_handle.get_asset_path());
				load_handle.finish(Err(error_message));
			}
		}
	});
}

//==============================================================================
/// 브라우저로 그림 풀기. (원본 HTMLImageElement 처럼 Blob 주소를 img 에 넣고 decode 를 기다려 캔버스에 그려 픽셀을 읽는다, 못 풀면 None)
/// - 크기는 그림의 natural 크기다. 크기가 없는 SVG 는 CSS 의 기본 대상 크기 300 x 150 으로 그린다.
//==============================================================================
async fn decode_browser_image(bytes: &[u8], mime_type: &str) -> Option<std::rc::Rc<crate::core::image::Image>> {
	let byte_array = js_sys::Uint8Array::from(bytes);
	let blob_part_list = js_sys::Array::of1(&byte_array);
	let blob_options = web_sys::BlobPropertyBag::new();
	blob_options.set_type(mime_type);
	let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_part_list, &blob_options).ok()?;
	let object_url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
	let image_element = web_sys::HtmlImageElement::new().ok()?;
	image_element.set_src(&object_url);
	let decode_result = wasm_bindgen_futures::JsFuture::from(image_element.decode()).await;
	let _ = web_sys::Url::revoke_object_url(&object_url);
	decode_result.ok()?;
	let mut image_width = image_element.natural_width();
	let mut image_height = image_element.natural_height();
	if image_width == 0 || image_height == 0 {
		image_width = 300;
		image_height = 150;
	}
	let document = web_sys::window()?.document()?;
	let canvas: web_sys::HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
	canvas.set_width(image_width);
	canvas.set_height(image_height);
	let context_options = js_sys::Object::new();
	js_sys::Reflect::set(&context_options, &JsValue::from_str("willReadFrequently"), &JsValue::TRUE).ok()?;
	let context: web_sys::CanvasRenderingContext2d = canvas.get_context_with_context_options("2d", &context_options).ok()??.dyn_into().ok()?;
	context.draw_image_with_html_image_element_and_dw_and_dh(&image_element, 0.0, 0.0, image_width as f64, image_height as f64).ok()?;
	let image_data = context.get_image_data(0.0, 0.0, image_width as f64, image_height as f64).ok()?;
	Some(crate::core::image::Image::create_from_pixels(image_width, image_height, image_data.data().0))
}

//==============================================================================
/// 주소에서 바이트와 응답의 content-type 받기. (is_no_cache 면 원본 TextAsset 처럼 fetch 에 cache: "no-cache" 를 붙인다, 머리가 없으면 content-type 은 None)
//==============================================================================
async fn fetch_bytes(asset_location: &str, is_no_cache: bool) -> Result<(Vec<u8>, Option<String>), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let response_promise = if is_no_cache {
		let request_init = js_sys::Object::new();
		js_sys::Reflect::set(&request_init, &JsValue::from_str("cache"), &JsValue::from_str("no-cache")).map_err(describe_js_error)?;
		let fetch_function: js_sys::Function = js_sys::Reflect::get(&window, &JsValue::from_str("fetch")).map_err(describe_js_error)?.dyn_into().map_err(|_| String::from("fetch 없음"))?;
		let promise_value = fetch_function.call2(&window, &JsValue::from_str(asset_location), &request_init).map_err(describe_js_error)?;
		promise_value.dyn_into::<js_sys::Promise>().map_err(|_| String::from("fetch 가 프로미스를 돌려주지 않음"))?
	}
	else {
		window.fetch_with_str(asset_location)
	};
	let response_value = wasm_bindgen_futures::JsFuture::from(response_promise).await.map_err(|error| format!("애셋 받기 실패 ({}): {}", asset_location, describe_js_error(error)))?;
	let response: web_sys::Response = response_value.dyn_into().map_err(|_| format!("애셋 응답이 아님 ({})", asset_location))?;
	if !response.ok() {
		return Err(format!("애셋 받기 실패 ({}): HTTP {}", asset_location, response.status()));
	}
	let content_type = read_response_content_type(&response);
	let array_buffer_promise = response.array_buffer().map_err(|error| format!("애셋 본문 읽기 실패 ({}): {}", asset_location, describe_js_error(error)))?;
	let array_buffer = wasm_bindgen_futures::JsFuture::from(array_buffer_promise).await.map_err(|error| format!("애셋 본문 읽기 실패 ({}): {}", asset_location, describe_js_error(error)))?;
	let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
	Ok((bytes, content_type))
}

//==============================================================================
/// 응답의 content-type 머리. (response.headers.get("content-type"), 없으면 None)
//==============================================================================
fn read_response_content_type(response: &web_sys::Response) -> Option<String> {
	let headers = js_sys::Reflect::get(response, &JsValue::from_str("headers")).ok()?;
	let get_function: js_sys::Function = js_sys::Reflect::get(&headers, &JsValue::from_str("get")).ok()?.dyn_into().ok()?;
	get_function.call1(&headers, &JsValue::from_str("content-type")).ok()?.as_string()
}

//==============================================================================
/// 캔버스 상태. (마지막으로 맞춘 창 크기와 배율, 배율은 렌더 배율 상한을 건 값)
//==============================================================================
struct WindowState {
	canvas: web_sys::HtmlCanvasElement,
	canvas_width: f64,
	canvas_height: f64,
	device_pixel_ratio: f64,
	/// 마지막으로 맞춘 창 안쪽 크기. (화면 키보드로 세로만 줄었는지 볼 때, 원본 appliedClientNativeSize)
	applied_client_size: Option<(f64, f64)>,
}

//==============================================================================
/// 웹 실행자.
//==============================================================================
struct WebApplication {
	engine_runner: EngineRunner,
	window_state: Option<WindowState>,
}

//==============================================================================
/// 시작. (캔버스와 WebGL2 를 잡고 request_animation_frame 을 걸어 둔 뒤 바로 돌아온다)
///
/// # 인자
/// - `engine_runner`: 엔진 실행자. (Engine::run 이 엔진과 첫 씬으로 만들어 넘긴다)
///
/// # 돌려줌
/// 프레임 콜백을 걸었으면 Ok. 캔버스나 WebGL2 컨텍스트를 얻지 못했거나 사건 리스너를 달지 못했으면 Err(까닭 글).
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
	add_pointer_listeners(&web_application_cell)?;
	add_audio_listeners(&web_application_cell)?;
	add_page_listeners(&web_application_cell)?;
	add_context_listeners(&web_application_cell)?;
	let canvas = web_application_cell.borrow().engine_runner.engine.platform_window.borrow().clone();
	if let Some(canvas) = canvas {
		textinput::add_text_input_listeners(&canvas)?;
	}

	// 프레임 콜백. (자기 자신을 다시 걸어야 하므로 Rc 안에 넣어 둔다. 페이지가 살아 있는 동안 계속 산다)
	let frame_callback_cell: FrameCallbackCell = Rc::new(RefCell::new(None));
	let frame_callback_cell_for_loop = frame_callback_cell.clone();
	let web_application_cell_for_loop = web_application_cell.clone();
	let mut frame_limiter = FrameLimiter::new();
	*frame_callback_cell.borrow_mut() = Some(Closure::new(move |timestamp: f64| {
		let mut web_application = web_application_cell_for_loop.borrow_mut();
		if web_application.engine_runner.engine.is_exit_requested() {
			return;
		}
		// 최대 FPS 제한. (0 이면 화면 주사율 그대로, 원본처럼 세기 시작한 뒤의 프레임 수로 차례를 정한다)
		let maximum_frame_per_second = web_application.engine_runner.engine.get_engine_configuration().maximum_frame_per_second as f64;
		let is_frame_turn = frame_limiter.is_frame_turn(timestamp, maximum_frame_per_second);
		if is_frame_turn {
			web_application.resize_if_changed();
			textinput::begin_frame(&mut web_application.engine_runner.engine);
			web_application.engine_runner.update_engine();
			textinput::apply_frame(&web_application.engine_runner.engine);
		}
		// 프레임 안에서 들어와 쌓인 사건을 온 차례대로 처리한다. (프레임 끝의 입력 정리 뒤라 다음 프레임에 방금 누름으로 보인다)
		WEB_EVENT_QUEUE.with(|web_event_queue| {
			web_event_queue.drain(&mut web_application);
		});
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
/// 다음 프레임 요청.
//==============================================================================
fn request_animation_frame(frame_callback: &Closure<dyn FnMut(f64)>) {
	if let Some(window) = web_sys::window() {
		let _ = window.request_animation_frame(frame_callback.as_ref().unchecked_ref());
	}
}

//==============================================================================
/// 키보드 입력. (KeyboardEvent.code 는 winit KeyCode 이름과 같은 W3C 규약)
//==============================================================================
fn add_keyboard_listeners(web_application_cell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let web_application_cell_for_key_down = web_application_cell.clone();
	let key_down_callback: Closure<dyn FnMut(web_sys::KeyboardEvent)> = Closure::new(move |keyboard_event: web_sys::KeyboardEvent| {
		dispatch_web_event(&web_application_cell_for_key_down, move |web_application| {
			let key_code = keyboard_event.code();
			let mut input_manager = web_application.engine_runner.engine.get_input_manager_mut();
			// 막을 키의 브라우저 기본 동작. (되풀이도 막는다, 글자 입력칸에 쓰는 중이면 그대로)
			if input_manager.is_key_prevented(&key_code) && !input_manager.is_typing_in_text_field() {
				keyboard_event.prevent_default();
			}
			if keyboard_event.repeat() {
				return;
			}
			input_manager.push_key(&key_code);
			input_manager.set_last_input_device(InputDevice::Keyboard);
		});
	});
	let web_application_cell_for_key_up = web_application_cell.clone();
	let key_up_callback: Closure<dyn FnMut(web_sys::KeyboardEvent)> = Closure::new(move |keyboard_event: web_sys::KeyboardEvent| {
		dispatch_web_event(&web_application_cell_for_key_up, move |web_application| {
			let key_code = keyboard_event.code();
			web_application.engine_runner.engine.get_input_manager_mut().pop_key(&key_code);
		});
	});
	let web_application_cell_for_blur = web_application_cell.clone();
	let blur_callback: Closure<dyn FnMut()> = Closure::new(move || {
		// 원본처럼 창이 포커스를 잃으면 키, 마우스 단추, 손가락 눌림을 모두 놓고 터치는 뗀 것으로 넘긴다. (keyup, mouseup 이 오지 않는다)
		dispatch_web_event(&web_application_cell_for_blur, |web_application| {
			let mut input_manager = web_application.engine_runner.engine.get_input_manager_mut();
			input_manager.clear();
			input_manager.set_touch_released(true);
		});
	});
	window.add_event_listener_with_callback("keydown", key_down_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	window.add_event_listener_with_callback("keyup", key_up_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	window.add_event_listener_with_callback("blur", blur_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	key_down_callback.forget();
	key_up_callback.forget();
	blur_callback.forget();
	Ok(())
}

/// 손가락을 뗀 바로 뒤 브라우저가 흉내 내 보내는 마우스 사건으로 보는 시간. (밀리초, 원본 engine.js 의 800)
const COMPATIBILITY_MOUSE_MILLISECONDS: f64 = 800.0;
/// 마우스가 이만큼(CSS 픽셀) 넘게 움직여야 마지막 장치를 마우스로 본다.
const MOUSE_DEVICE_MOVE_THRESHOLD: i32 = 6;

//==============================================================================
/// 마우스와 손가락 입력. (원본 engine.js 의 mousedown, mousemove, mouseup, wheel, touchstart, touchmove, touchend, touchcancel)
/// - 마우스 단추(원본처럼 어느 것이든, set_touch_from_left_button_only 면 왼쪽만)와 첫 손가락을 엔진의 터치로 넘기고, 단추 셋과 손가락 목록, 휠의 Ctrl 을 입력 매니저에 적는다.
/// - 손가락을 뗀 바로 뒤(800 밀리초)에 브라우저가 흉내 내 보내는 마우스 사건은 원본 engine.js 처럼 마지막 장치와 마지막 손에만 적지 않는다.
///   단추와 자리, 터치는 원본처럼 그대로 넘긴다. (손가락 사건의 기본 동작을 막아 브라우저는 흉내 낸 마우스 사건을 보내지 않는다)
///   CameraGestures 처럼 자기 문턱(set_thresholds 의 넷째 인자)으로 거르는 쪽이 원본처럼 사건을 모두 보고 거르게 하려는 것이다.
/// - 캔버스 위의 손가락은 기본 동작(스크롤, 확대)을 막는다. 오른쪽 클릭 메뉴는 set_context_menu_prevented 가 참(처음)이면 막는다.
/// - 프레임 안에서 사건이 바로 불려도 겹쳐 빌리지 않게 dispatch_web_event 로 보낸다. (못 빌리면 줄에 쌓았다가 프레임 끝에 처리한다)
//==============================================================================
fn add_pointer_listeners(web_application_cell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let canvas = web_application_cell.borrow().engine_runner.engine.platform_window.borrow().clone().ok_or_else(|| String::from("캔버스 없음"))?;
	let last_touch_time = Rc::new(std::cell::Cell::new(-COMPATIBILITY_MOUSE_MILLISECONDS));

	// 마우스 누름. (캔버스 위에서만)
	let web_application_cell_for_mouse_down = web_application_cell.clone();
	let last_touch_time_for_mouse_down = last_touch_time.clone();
	let mouse_down_callback: Closure<dyn FnMut(web_sys::MouseEvent)> = Closure::new(move |mouse_event: web_sys::MouseEvent| {
		let is_compatibility_mouse_event = is_compatibility_mouse(last_touch_time_for_mouse_down.get());
		dispatch_web_event(&web_application_cell_for_mouse_down, move |web_application| {
			web_application.update_pointer_position(mouse_event.client_x() as f64, mouse_event.client_y() as f64);
			let mut input_manager = web_application.engine_runner.engine.get_input_manager_mut();
			if !is_compatibility_mouse_event {
				input_manager.set_touch_input(false);
				input_manager.set_last_input_device(InputDevice::Mouse);
			}
			let mouse_button = read_mouse_button(mouse_event.button());
			if let Some(mouse_button) = mouse_button {
				input_manager.set_mouse_button_down(mouse_button, true);
			}
			if mouse_button.map(|mouse_button| input_manager.is_touch_button(mouse_button)).unwrap_or(false) {
				input_manager.set_touch_pressed(true);
				input_manager.set_touch_moved(true);
				input_manager.set_touch_down(true);
			}
			if mouse_button == Some(MouseButton::Middle) {
				mouse_event.prevent_default();
			}
		});
	});

	// 마우스 움직임과 뗌. (캔버스 밖으로 끌고 나가도 받게 창에 단다)
	let web_application_cell_for_mouse_move = web_application_cell.clone();
	let last_touch_time_for_mouse_move = last_touch_time.clone();
	let mouse_move_callback: Closure<dyn FnMut(web_sys::MouseEvent)> = Closure::new(move |mouse_event: web_sys::MouseEvent| {
		let is_compatibility_mouse_event = is_compatibility_mouse(last_touch_time_for_mouse_move.get());
		dispatch_web_event(&web_application_cell_for_mouse_move, move |web_application| {
			web_application.update_pointer_position(mouse_event.client_x() as f64, mouse_event.client_y() as f64);
			if is_compatibility_mouse_event {
				return;
			}
			let mut input_manager = web_application.engine_runner.engine.get_input_manager_mut();
			input_manager.set_touch_input(false);
			// 창이 흔들릴 때 나는 작은 움직임은 마우스를 쓴 것으로 보지 않는다.
			let movement = mouse_event.movement_x().abs() + mouse_event.movement_y().abs();
			if movement >= MOUSE_DEVICE_MOVE_THRESHOLD {
				input_manager.set_last_input_device(InputDevice::Mouse);
			}
		});
	});
	let web_application_cell_for_mouse_up = web_application_cell.clone();
	let mouse_up_callback: Closure<dyn FnMut(web_sys::MouseEvent)> = Closure::new(move |mouse_event: web_sys::MouseEvent| {
		dispatch_web_event(&web_application_cell_for_mouse_up, move |web_application| {
			web_application.update_pointer_position(mouse_event.client_x() as f64, mouse_event.client_y() as f64);
			let mut input_manager = web_application.engine_runner.engine.get_input_manager_mut();
			let mouse_button = read_mouse_button(mouse_event.button());
			if let Some(mouse_button) = mouse_button {
				input_manager.set_mouse_button_down(mouse_button, false);
			}
			let is_touch_button = mouse_button.map(|mouse_button| input_manager.is_touch_button(mouse_button)).unwrap_or(false);
			if is_touch_button && input_manager.is_touch_down() {
				input_manager.set_touch_moved(false);
				input_manager.set_touch_released(true);
				input_manager.set_touch_down(false);
			}
		});
	});

	// 휠. (Ctrl 을 함께 눌렀는지 적고, 페이지가 스크롤되지 않게 막는다)
	let web_application_cell_for_wheel = web_application_cell.clone();
	let wheel_callback: Closure<dyn FnMut(web_sys::WheelEvent)> = Closure::new(move |wheel_event: web_sys::WheelEvent| {
		wheel_event.prevent_default();
		dispatch_web_event(&web_application_cell_for_wheel, move |web_application| {
			web_application.update_pointer_position(wheel_event.client_x() as f64, wheel_event.client_y() as f64);
			let mut input_manager = web_application.engine_runner.engine.get_input_manager_mut();
			input_manager.add_wheel_delta(wheel_event.delta_x() as f64, wheel_event.delta_y() as f64);
			input_manager.set_wheel_with_ctrl(wheel_event.ctrl_key());
			input_manager.set_last_input_device(InputDevice::Mouse);
		});
	});

	// 손가락. (닿은 손가락 목록을 통째로 새로 쓰고, 첫 손가락의 누름과 뗌을 터치로 넘긴다)
	let web_application_cell_for_touch = web_application_cell.clone();
	let last_touch_time_for_touch = last_touch_time.clone();
	let touch_callback: Closure<dyn FnMut(web_sys::TouchEvent)> = Closure::new(move |touch_event: web_sys::TouchEvent| {
		touch_event.prevent_default();
		let now = read_now_milliseconds();
		last_touch_time_for_touch.set(now);
		dispatch_web_event(&web_application_cell_for_touch, move |web_application| {
			web_application.handle_touch_event(&touch_event);
		});
	});

	// 오른쪽 클릭 메뉴 막기. (막게 해 두었을 때)
	let web_application_cell_for_context_menu = web_application_cell.clone();
	let context_menu_callback: Closure<dyn FnMut(web_sys::Event)> = Closure::new(move |event: web_sys::Event| {
		let is_context_menu_prevented = match web_application_cell_for_context_menu.try_borrow() {
			Ok(web_application) => {
				web_application.engine_runner.engine.get_input_manager().is_context_menu_prevented()
			}
			Err(_) => {
				true
			}
		};
		if is_context_menu_prevented {
			event.prevent_default();
		}
	});

	let passive_false_options = web_sys::AddEventListenerOptions::new();
	passive_false_options.set_passive(false);
	canvas.add_event_listener_with_callback("mousedown", mouse_down_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	window.add_event_listener_with_callback("mousemove", mouse_move_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	window.add_event_listener_with_callback("mouseup", mouse_up_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	canvas.add_event_listener_with_callback_and_add_event_listener_options("wheel", wheel_callback.as_ref().unchecked_ref(), &passive_false_options).map_err(describe_js_error)?;
	for event_name in ["touchstart", "touchmove", "touchend", "touchcancel"] {
		canvas.add_event_listener_with_callback_and_add_event_listener_options(event_name, touch_callback.as_ref().unchecked_ref(), &passive_false_options).map_err(describe_js_error)?;
	}
	canvas.add_event_listener_with_callback("contextmenu", context_menu_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	mouse_down_callback.forget();
	mouse_move_callback.forget();
	mouse_up_callback.forget();
	wheel_callback.forget();
	touch_callback.forget();
	context_menu_callback.forget();
	Ok(())
}

//==============================================================================
/// 지금 시각. (밀리초, performance.now)
//==============================================================================
fn read_now_milliseconds() -> f64 {
	web_sys::window().and_then(|window| window.performance()).map(|performance| performance.now()).unwrap_or(0.0)
}

//==============================================================================
/// 손가락을 뗀 바로 뒤의 마우스 사건인지. (브라우저가 흉내 내 보내는 것)
//==============================================================================
fn is_compatibility_mouse(last_touch_time: f64) -> bool {
	read_now_milliseconds() - last_touch_time < COMPATIBILITY_MOUSE_MILLISECONDS
}

//==============================================================================
/// MouseEvent.button 을 마우스 단추로. (0 왼쪽, 1 가운데, 2 오른쪽, 그 밖은 없음)
//==============================================================================
fn read_mouse_button(button: i16) -> Option<MouseButton> {
	match button {
		0 => {
			Some(MouseButton::Left)
		}
		1 => {
			Some(MouseButton::Middle)
		}
		2 => {
			Some(MouseButton::Right)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 오디오 재개 이벤트. (원본 setupAllDocumentEvents 의 오디오 부분)
/// - 프레임 안에서 이벤트가 바로 불려도 겹쳐 빌리지 않게 try_borrow 로 본다. (못 빌리면 다음 이벤트에 맡긴다)
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
/// 페이지 이벤트와 복원. (원본 setupAllDocumentEvents 의 resume, pageshow, visibilitychange 와 SoundSystem 의 blur, focus, pointerdown)
/// - 엔진을 고쳐 써야 해서 dispatch_web_event 로 보낸다. (프레임 콜백 안에서 겹쳐 불려도 줄에 쌓았다가 프레임 끝에 처리한다)
//==============================================================================
fn add_page_listeners(web_application_cell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let document = window.document().ok_or_else(|| String::from("document 없음"))?;

	// 처음부터 가려져 있으면 엔진에 적어 둔다.
	if document.visibility_state() == web_sys::VisibilityState::Hidden {
		web_application_cell.borrow_mut().engine_runner.engine.dispatch_page_event(PageEvent::Hidden);
	}

	// 가려짐과 다시 보임. (다시 보이면 복구하고 씬마다 on_page_visibility_restored)
	let web_application_cell_for_visibility = web_application_cell.clone();
	let visibility_callback: Closure<dyn FnMut()> = Closure::new(move || {
		let is_visible = web_sys::window()
			.and_then(|window| window.document())
			.map(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
			.unwrap_or(false);
		dispatch_web_event(&web_application_cell_for_visibility, move |web_application| {
			if is_visible {
				web_application.engine_runner.engine.dispatch_page_event(PageEvent::Visible);
				web_application.engine_runner.notify_page_visibility_restored();
			}
			else {
				web_application.engine_runner.engine.dispatch_page_event(PageEvent::Hidden);
			}
		});
	});

	// 페이지 프로세스 복원. (Chromium freeze 뒤 문서 resume, bfcache 에서 돌아온 pageshow 의 persisted)
	let web_application_cell_for_resume = web_application_cell.clone();
	let resume_callback: Closure<dyn FnMut()> = Closure::new(move || {
		dispatch_web_event(&web_application_cell_for_resume, |web_application| {
			web_application.engine_runner.notify_page_process_restored();
		});
	});
	let web_application_cell_for_page_show = web_application_cell.clone();
	let page_show_callback: Closure<dyn FnMut(web_sys::Event)> = Closure::new(move |event: web_sys::Event| {
		let is_persisted = js_sys::Reflect::get(&event, &JsValue::from_str("persisted")).ok().and_then(|value| value.as_bool()).unwrap_or(false);
		if !is_persisted {
			return;
		}
		dispatch_web_event(&web_application_cell_for_page_show, |web_application| {
			web_application.engine_runner.notify_page_process_restored();
		});
	});

	// 창 포커스와 누름.
	let page_event_callback_list: Vec<(&str, PageEvent)> = vec![("blur", PageEvent::Blurred), ("focus", PageEvent::Focused), ("pointerdown", PageEvent::PointerDown)];
	for (event_name, page_event) in page_event_callback_list {
		let web_application_cell_for_page_event = web_application_cell.clone();
		let page_event_callback: Closure<dyn FnMut()> = Closure::new(move || {
			dispatch_web_event(&web_application_cell_for_page_event, move |web_application| {
				web_application.engine_runner.engine.dispatch_page_event(page_event);
			});
		});
		window.add_event_listener_with_callback(event_name, page_event_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
		page_event_callback.forget();
	}

	document.add_event_listener_with_callback("visibilitychange", visibility_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	document.add_event_listener_with_callback("resume", resume_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	window.add_event_listener_with_callback("pageshow", page_show_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	visibility_callback.forget();
	resume_callback.forget();
	page_show_callback.forget();
	Ok(())
}

//==============================================================================
/// GL 컨텍스트를 잃거나 되살림. (원본 WebGLStateCache 처럼 캔버스의 webglcontextlost, webglcontextrestored 에서 GL 상태 기억을 잊는다)
/// - 컨텍스트를 잃거나 되살리면 GL 의 묶음이 처음으로 돌아가므로 기억을 잊는다. 프레임 콜백 안에서 겹쳐 불리지 않게 try_borrow 로 본다.
//==============================================================================
fn add_context_listeners(web_application_cell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let canvas = web_application_cell.borrow().engine_runner.engine.platform_window.borrow().clone().ok_or_else(|| String::from("캔버스 없음"))?;
	let web_application_cell_for_context = web_application_cell.clone();
	// 잃음: 되살릴 수 있게 기본 동작을 막고(preventDefault), 엔진이 그리기를 멈추고 리스너에 알린다. (원본 Graphic 의 webglcontextlost)
	let context_lost_callback: Closure<dyn FnMut(web_sys::Event)> = Closure::new(move |event: web_sys::Event| {
		event.prevent_default();
		if let Ok(web_application) = web_application_cell_for_context.try_borrow() {
			web_application.engine_runner.handle_context_lost();
		}
	});
	canvas.add_event_listener_with_callback("webglcontextlost", context_lost_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	context_lost_callback.forget();
	// 되살림: 엔진이 기본 자원을 다시 짓고 리스너에 알린다. (원본 Graphic 의 webglcontextrestored)
	let web_application_cell_for_restore = web_application_cell.clone();
	let context_restored_callback: Closure<dyn FnMut()> = Closure::new(move || {
		if let Ok(web_application) = web_application_cell_for_restore.try_borrow() {
			web_application.engine_runner.handle_context_restored();
		}
	});
	canvas.add_event_listener_with_callback("webglcontextrestored", context_restored_callback.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	context_restored_callback.forget();
	Ok(())
}

//==============================================================================
/// 응용 프로그램 종료. (원본 exitApplication: Capacitor 네이티브면 App 플러그인으로 끝내고, 아니면 창 닫기를 시도한다)
/// - 둘 다 환경에 따라 거부될 수 있으며 실패해도 오류를 내지 않는다. 엔진은 종료 요청을 적어 다음 프레임부터 돌지 않는다.
//==============================================================================
pub fn exit_application() {
	let window = match web_sys::window() {
		Some(window) => {
			window
		}
		None => {
			return;
		}
	};
	let exit_app_function = js_sys::Reflect::get(&window, &JsValue::from_str("Capacitor"))
		.ok()
		.filter(|capacitor| capacitor.is_object())
		.and_then(|capacitor| js_sys::Reflect::get(&capacitor, &JsValue::from_str("Plugins")).ok())
		.filter(|plugins| plugins.is_object())
		.and_then(|plugins| js_sys::Reflect::get(&plugins, &JsValue::from_str("App")).ok())
		.filter(|app_plugin| app_plugin.is_object())
		.and_then(|app_plugin| {
			let exit_app_function = js_sys::Reflect::get(&app_plugin, &JsValue::from_str("exitApp")).ok()?.dyn_into::<js_sys::Function>().ok()?;
			Some((app_plugin, exit_app_function))
		});
	if let Some((app_plugin, exit_app_function)) = exit_app_function {
		if exit_app_function.call0(&app_plugin).is_ok() {
			return;
		}
	}
	let _ = window.close();
}

//==============================================================================
/// 바깥 주소 열기. (원본 Engine.openUrl, window.open(주소, "_blank", "noopener") 로 새 탭에 연다)
/// - 사용자 입력(누름, 탭, 키) 처리 안에서 부를 때만 열린다. 브라우저의 팝업 차단 규칙이다.
/// - 열 수 있는 것은 http, https, mailto, tel 뿐이다. (is_openable_url) 그 밖의 꼴은 열지 않고 false.
//==============================================================================
pub fn open_url(url: &str) -> bool {
	if !crate::platform::is_openable_url(url) {
		return false;
	}
	let window = match web_sys::window() {
		Some(window) => {
			window
		}
		None => {
			return false;
		}
	};
	window.open_with_url_and_target_and_features(url, "_blank", "noopener").is_ok()
}

//==============================================================================
/// 자바스크립트 오류를 글로.
//==============================================================================
fn describe_js_error(error: JsValue) -> String {
	format!("{:?}", error)
}

impl WebApplication {
	//==============================================================================
	/// 화면(client) 좌표를 캔버스 논리 좌표와 뷰 좌표로 바꿔 입력 매니저에 적기.
	//==============================================================================
	fn update_pointer_position(&mut self, client_x: f64, client_y: f64) {
		let canvas_native_position = match self.read_canvas_native_position(client_x, client_y) {
			Some(canvas_native_position) => {
				canvas_native_position
			}
			None => {
				return;
			}
		};
		let engine = &mut self.engine_runner.engine;
		let view_input_position = engine.get_view_manager().canvas_position_to_view_position(canvas_native_position);
		let mut input_manager = engine.get_input_manager_mut();
		input_manager.set_canvas_native_input_position(canvas_native_position);
		input_manager.set_view_input_position(view_input_position);
	}

	//==============================================================================
	/// 화면(client) 좌표를 캔버스 논리 좌표로. (캔버스의 실제 자리를 읽는다)
	//==============================================================================
	fn read_canvas_native_position(&self, client_x: f64, client_y: f64) -> Option<Vector2> {
		let canvas = self.engine_runner.engine.get_canvas()?;
		let canvas_rect = canvas.get_bounding_client_rect();
		Some(Vector2::create((client_x - canvas_rect.left()) as f64, (client_y - canvas_rect.top()) as f64))
	}

	//==============================================================================
	/// 손가락 사건. (바뀐 손가락마다 닿음, 움직임, 뗌을 적고, 첫 손가락의 누름, 움직임, 뗌을 터치로 넘긴다)
	/// - 사건의 touches 에 없는 손가락은 놓친 뗌으로 보아 취소로 지운다.
	//==============================================================================
	fn handle_touch_event(&mut self, touch_event: &web_sys::TouchEvent) {
		let event_type = touch_event.type_();
		let touch_list = touch_event.touches();
		let changed_touch_list = touch_event.changed_touches();
		let mut changed_touch_point_list: Vec<(i64, Vector2, Vector2)> = Vec::new();
		for touch_index in 0..changed_touch_list.length() {
			if let Some(touch) = changed_touch_list.get(touch_index) {
				if let Some(canvas_native_position) = self.read_canvas_native_position(touch.client_x() as f64, touch.client_y() as f64) {
					let view_position = self.engine_runner.engine.get_view_manager().canvas_position_to_view_position(canvas_native_position);
					changed_touch_point_list.push((touch.identifier() as i64, canvas_native_position, view_position));
				}
			}
		}
		let mut touching_identifier_list: Vec<i64> = Vec::new();
		for touch_index in 0..touch_list.length() {
			if let Some(touch) = touch_list.get(touch_index) {
				touching_identifier_list.push(touch.identifier() as i64);
			}
		}
		let changed_touch = changed_touch_list.get(0);
		let was_down = self.engine_runner.engine.get_input_manager().is_touch_down();
		let previous_first_identifier = self.engine_runner.engine.get_input_manager().get_touch_points().first().map(|touch_point| touch_point.identifier);
		if let Some(changed_touch) = &changed_touch {
			let is_first_finger = previous_first_identifier.is_none() || previous_first_identifier == Some(changed_touch.identifier() as i64);
			if is_first_finger {
				self.update_pointer_position(changed_touch.client_x() as f64, changed_touch.client_y() as f64);
			}
		}
		let mut input_manager = self.engine_runner.engine.get_input_manager_mut();
		for (identifier, canvas_native_position, view_position) in changed_touch_point_list {
			match event_type.as_str() {
				"touchstart" => {
					input_manager.begin_touch_point(identifier, canvas_native_position, view_position);
				}
				"touchmove" => {
					input_manager.move_touch_point(identifier, canvas_native_position, view_position);
				}
				"touchend" => {
					input_manager.end_touch_point(identifier, canvas_native_position, view_position, false);
				}
				_ => {
					input_manager.end_touch_point(identifier, canvas_native_position, view_position, true);
				}
			}
		}
		let missed_touch_point_list: Vec<_> = input_manager.get_touch_points().iter().filter(|touch_point| !touching_identifier_list.contains(&touch_point.identifier)).copied().collect();
		for missed_touch_point in missed_touch_point_list {
			input_manager.end_touch_point(missed_touch_point.identifier, missed_touch_point.canvas_native_position, missed_touch_point.view_position, true);
		}
		input_manager.set_touch_input(true);
		input_manager.set_last_input_device(InputDevice::Touch);
		match event_type.as_str() {
			"touchstart" => {
				if !was_down {
					input_manager.set_touch_pressed(true);
					input_manager.set_touch_moved(true);
					input_manager.set_touch_down(true);
				}
			}
			"touchmove" => {
			}
			"touchend" => {
				if was_down && touch_list.length() == 0 {
					input_manager.set_touch_moved(false);
					input_manager.set_touch_released(true);
					input_manager.set_touch_down(false);
				}
			}
			"touchcancel" => {
				if was_down && touch_list.length() == 0 {
					input_manager.set_touch_cancelled(true);
					input_manager.set_touch_down(false);
				}
			}
			_ => {
			}
		}
	}

	//==============================================================================
	/// 캔버스와 WebGL2 컨텍스트, 그래픽 만들기.
	//==============================================================================
	fn create_canvas(&mut self) -> Result<(), String> {
		let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
		let document = window.document().ok_or_else(|| String::from("document 없음"))?;

		// 이름 설정. (원본처럼 비어 있지 않으면 문서 제목에 쓴다, 데스크탑 창 제목의 기본값이면 페이지의 제목을 그대로 둔다)
		// 설정은 이 줄에서만 빌린다. (&...title 로 두면 빌림이 함수 끝까지 늘어나 아래 attach_graphic 이 설정을 고칠 때 이미 빌렸다고 패닉한다)
		let title = self.engine_runner.engine.get_engine_configuration().title.clone();
		if !title.is_empty() && title != crate::core::engine::DEFAULT_ENGINE_TITLE {
			document.set_title(&title);
		}
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
		let preserve_drawing_buffer = self.engine_runner.engine.get_engine_configuration().preserve_drawing_buffer;
		let is_antialias = self.engine_runner.engine.get_engine_configuration().antialias;
		for (option_name, option_value) in [("alpha", false), ("stencil", true), ("premultipliedAlpha", true), ("preserveDrawingBuffer", preserve_drawing_buffer), ("antialias", is_antialias)] {
			js_sys::Reflect::set(&context_options, &JsValue::from_str(option_name), &JsValue::from_bool(option_value)).map_err(describe_js_error)?;
		}
		let context = canvas
			.get_context_with_context_options("webgl2", &context_options)
			.map_err(describe_js_error)?
			.ok_or_else(|| String::from("WebGL2 Not Supported."))?
			.dyn_into::<web_sys::WebGl2RenderingContext>()
			.map_err(|_| String::from("WebGL2 컨텍스트 변환 실패"))?;
		ENGINE_GL_CONTEXT.with(|engine_gl_context| {
			*engine_gl_context.borrow_mut() = Some(context.clone());
		});
		let gl = glow::Context::from_webgl2_context(context);
		let graphic = Graphic::new(gl, preserve_drawing_buffer, self.engine_runner.engine.get_color_space())?;

		self.engine_runner.attach_graphic(graphic);
		*self.engine_runner.engine.platform_window.borrow_mut() = Some(canvas.clone());
		ENGINE_CANVAS.with(|engine_canvas| {
			*engine_canvas.borrow_mut() = Some(canvas.clone());
		});
		self.window_state = Some(WindowState {
			canvas,
			canvas_width: 0.0,
			canvas_height: 0.0,
			device_pixel_ratio: 0.0,
			applied_client_size: None,
		});
		self.resize_if_changed();
		Ok(())
	}

	//==============================================================================
	/// 창 크기나 배율이 바뀌었으면 캔버스를 맞추고 뷰 영역을 다시 계산. (원본 engine.resize 와 view_manager.calculate_view_rect)
	/// - auto_resize_on_window_resize 면 캔버스를 창 안쪽(visualViewport 가 있으면 그것)에 맞춘다. 끄면 페이지가 정한 캔버스 크기를 읽는다.
	/// - ignore_virtual_keyboard_resize 면 글자 입력칸이 포커스된 동안 가로가 같고 세로만 줄었을 때 세로를 그대로 둔다.
	/// - max_canvas_aspect_ratio 가 있으면 창이 그보다 길쭉할 때 캔버스를 줄여 가운데에 둔다.
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
		let engine_configuration = self.engine_runner.engine.get_engine_configuration();
		let is_auto_resize = engine_configuration.auto_resize_on_window_resize;
		let is_virtual_keyboard_ignored = engine_configuration.ignore_virtual_keyboard_resize;
		let max_canvas_aspect_ratio = engine_configuration.max_canvas_aspect_ratio as f64;
		drop(engine_configuration);
		let device_pixel_ratio = if window.device_pixel_ratio() > 0.0 {
			window.device_pixel_ratio()
		}
		else {
			1.0
		};
		// 렌더 배율 상한을 건다. (상한이 바뀌어도 이 값이 달라져 다시 맞춘다)
		let device_pixel_ratio = self.engine_runner.engine.get_view_manager().limit_render_pixel_ratio(device_pixel_ratio as f64) as f64;
		let is_text_field_focused = is_text_field_focused();
		let window_state = match self.window_state.as_mut() {
			Some(window_state) => {
				window_state
			}
			None => {
				return;
			}
		};
		let canvas_style = window_state.canvas.style();
		let (canvas_width, canvas_height, canvas_left, canvas_top) = if is_auto_resize {
			// 창 안쪽 크기. (visualViewport 가 있으면 그것, 모바일 주소 줄과 화면 키보드로 줄어든 실제 보이는 영역)
			let visual_viewport = js_sys::Reflect::get(&window, &JsValue::from_str("visualViewport")).ok().filter(|value| !value.is_undefined() && !value.is_null());
			let read_viewport_number = |name: &str| -> Option<f64> {
				visual_viewport.as_ref().and_then(|visual_viewport| js_sys::Reflect::get(visual_viewport, &JsValue::from_str(name)).ok()).and_then(|value| value.as_f64())
			};
			let client_width = read_viewport_number("width").unwrap_or_else(|| window.inner_width().ok().and_then(|value| value.as_f64()).unwrap_or(0.0));
			let mut client_height = read_viewport_number("height").unwrap_or_else(|| window.inner_height().ok().and_then(|value| value.as_f64()).unwrap_or(0.0));
			if is_virtual_keyboard_ignored && is_text_field_focused {
				if let Some((applied_client_width, applied_client_height)) = window_state.applied_client_size {
					if (client_width - applied_client_width).abs() < 1.0 && client_height < applied_client_height {
						client_height = applied_client_height;
					}
				}
			}
			window_state.applied_client_size = Some((client_width, client_height));
			if max_canvas_aspect_ratio > 0.0 {
				let canvas_width = client_width.min(client_height * max_canvas_aspect_ratio).floor();
				let canvas_height = client_height.min(client_width * max_canvas_aspect_ratio).floor();
				(canvas_width, canvas_height, Some(((client_width - canvas_width) * 0.5).floor()), Some(((client_height - canvas_height) * 0.5).floor()))
			}
			else {
				(client_width, client_height, None, None)
			}
		}
		else {
			// 페이지가 정한 캔버스 크기. (원본 canvas.getBoundingClientRect)
			let canvas_rect = window_state.canvas.get_bounding_client_rect();
			(canvas_rect.width().round(), canvas_rect.height().round(), None, None)
		};
		// 렌더 배율 상한을 바꿨으면 프레임 머리에서 창 크기 바뀜과 같은 길로 맞춘다. (원본 Engine 의 isViewRectPending, 그리는 중에 바꾸면 그 프레임이 까맣다)
		let is_resize_requested = self.engine_runner.engine.is_resize_requested.take() || self.engine_runner.engine.get_view_manager().is_view_rect_pending();
		if !is_resize_requested && window_state.canvas_width == canvas_width && window_state.canvas_height == canvas_height && window_state.device_pixel_ratio == device_pixel_ratio {
			return;
		}
		window_state.canvas_width = canvas_width;
		window_state.canvas_height = canvas_height;
		window_state.device_pixel_ratio = device_pixel_ratio;
		if is_auto_resize {
			let _ = canvas_style.set_property("width", &format!("{}px", canvas_width));
			let _ = canvas_style.set_property("height", &format!("{}px", canvas_height));
			// 비 상한이 없으면 가운데에 두던 여백을 지운다. (정한 적이 없으면 아무것도 바뀌지 않는다)
			let margin_left = canvas_left.map(|canvas_left| format!("{}px", canvas_left)).unwrap_or_default();
			let margin_top = canvas_top.map(|canvas_top| format!("{}px", canvas_top)).unwrap_or_default();
			let _ = canvas_style.set_property("margin-left", &margin_left);
			let _ = canvas_style.set_property("margin-top", &margin_top);
		}
		let canvas_pixel_width = (canvas_width * device_pixel_ratio).round() as u32;
		let canvas_pixel_height = (canvas_height * device_pixel_ratio).round() as u32;
		window_state.canvas.set_width(canvas_pixel_width);
		window_state.canvas.set_height(canvas_pixel_height);
		self.engine_runner.engine.get_view_manager_mut().calculate_view_rect(canvas_pixel_width, canvas_pixel_height, device_pixel_ratio as f64);
		self.engine_runner.notify_scene_resize();
	}
}
