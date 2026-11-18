//==============================================================================
// 웹 플랫폼. (wasm32)
// - 원본처럼 캔버스를 창 안쪽에 꽉 채우고, 원본과 같은 옵션으로 WebGL2 컨텍스트를 얻는다.
// - requestAnimationFrame 콜백마다 엔진 프레임 하나. (브라우저가 수직 동기에 맞춰 부르고, 콜백이 끝나면 화면을 넘긴다)
// - 입력은 아직 키보드만 받는다. 입력 필드의 글 입력(숨긴 DOM <input>, IME 조합, 문맥 메뉴)과 클립보드는 web/textinput.rs 가 맡는다.
// - 오디오: 원본 engine.js 처럼 click / touchstart / keydown(설정하면 touchend / pointerup 도)은 user gesture 로 보고
//   engine.resumeOnUserGesture(), focus 와 페이지가 다시 보일 때는 engine.resume() 을 부른다.
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
pub use textinput::{readClipboardText, writeClipboardText};

// 엔진이 들고 있는 창 손잡이. (커서를 바꿀 때)
pub type PlatformWindow = web_sys::HtmlCanvasElement;

// 프레임 콜백을 담는 칸. (콜백이 자기 자신을 다시 걸어야 해서 Rc 로 나눠 든다)
type FrameCallbackCell = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

//==============================================================================
// 커서 보이기 설정.
//==============================================================================
pub fn setCursorVisible(platformWindow: Option<&PlatformWindow>, isVisible: bool) {
	if let Some(canvas) = platformWindow {
		let cursorValue = if isVisible {
			""
		}
		else {
			"none"
		};
		let _ = canvas.style().set_property("cursor", cursorValue);
	}
}

//==============================================================================
// 창 제목 설정. (문서 제목)
//==============================================================================
pub fn setTitle(_platformWindow: Option<&PlatformWindow>, title: &str) {
	if let Some(document) = web_sys::window().and_then(|window| window.document()) {
		document.set_title(title);
	}
}

//==============================================================================
// 애셋 바이트 읽기. (fetch 로 받고, 받으면 끝낸다. 그 사이 프레임은 계속 돈다)
//==============================================================================
pub fn requestAssetBytes(assetLocation: String, loadHandle: AssetLoadHandle) {
	wasm_bindgen_futures::spawn_local(async move {
		let result = fetchBytes(&assetLocation).await;
		loadHandle.finish(result);
	});
}

//==============================================================================
// 주소에서 바이트 받기.
//==============================================================================
async fn fetchBytes(assetLocation: &str) -> Result<Vec<u8>, String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let responsePromise = window.fetch_with_str(assetLocation);
	let responseValue = wasm_bindgen_futures::JsFuture::from(responsePromise).await.map_err(|error| format!("애셋 받기 실패 ({}): {}", assetLocation, describeJsError(error)))?;
	let response: web_sys::Response = responseValue.dyn_into().map_err(|_| format!("애셋 응답이 아님 ({})", assetLocation))?;
	if !response.ok() {
		return Err(format!("애셋 받기 실패 ({}): HTTP {}", assetLocation, response.status()));
	}
	let arrayBufferPromise = response.array_buffer().map_err(|error| format!("애셋 본문 읽기 실패 ({}): {}", assetLocation, describeJsError(error)))?;
	let arrayBuffer = wasm_bindgen_futures::JsFuture::from(arrayBufferPromise).await.map_err(|error| format!("애셋 본문 읽기 실패 ({}): {}", assetLocation, describeJsError(error)))?;
	let bytes = js_sys::Uint8Array::new(&arrayBuffer).to_vec();
	Ok(bytes)
}

//==============================================================================
// 캔버스 상태. (마지막으로 맞춘 창 크기와 배율)
//==============================================================================
struct WindowState {
	canvas: web_sys::HtmlCanvasElement,
	clientWidth: f64,
	clientHeight: f64,
	devicePixelRatio: f64,
}

//==============================================================================
// 웹 실행자.
//==============================================================================
struct WebApplication {
	engineRunner: EngineRunner,
	windowState: Option<WindowState>,
}

//==============================================================================
// 시작. (캔버스와 WebGL2 를 잡고 requestAnimationFrame 을 걸어 둔 뒤 바로 돌아온다)
//==============================================================================
pub fn run(engineRunner: EngineRunner) -> Result<(), String> {
	let webApplicationCell = Rc::new(RefCell::new(WebApplication {
		engineRunner,
		windowState: None,
	}));
	{
		let mut webApplication = webApplicationCell.borrow_mut();
		webApplication.createCanvas()?;
		webApplication.engineRunner.applyPendingScenes();
	}
	addKeyboardListeners(&webApplicationCell)?;
	addAudioListeners(&webApplicationCell)?;
	let canvas = webApplicationCell.borrow().engineRunner.engine.platformWindow.clone();
	if let Some(canvas) = canvas {
		textinput::addTextInputListeners(&canvas)?;
	}

	// 프레임 콜백. (자기 자신을 다시 걸어야 하므로 Rc 안에 넣어 둔다. 페이지가 살아 있는 동안 계속 산다)
	let frameCallbackCell: FrameCallbackCell = Rc::new(RefCell::new(None));
	let frameCallbackCellForLoop = frameCallbackCell.clone();
	let webApplicationCellForLoop = webApplicationCell.clone();
	let mut lastFrameTimestamp = 0.0f64;
	*frameCallbackCell.borrow_mut() = Some(Closure::new(move |timestamp: f64| {
		let mut webApplication = webApplicationCellForLoop.borrow_mut();
		if webApplication.engineRunner.engine.isExitRequested() {
			return;
		}
		// 최대 FPS 제한. (0 이면 화면 주사율 그대로)
		let maximumFramePerSecond = webApplication.engineRunner.engine.getEngineConfiguration().maximumFramePerSecond as f64;
		let isFrameTurn = maximumFramePerSecond <= 0.0 || timestamp - lastFrameTimestamp >= 1000.0 / maximumFramePerSecond - 1.0;
		if isFrameTurn {
			lastFrameTimestamp = timestamp;
			webApplication.resizeIfChanged();
			textinput::beginFrame(&mut webApplication.engineRunner.engine);
			webApplication.engineRunner.updateEngine();
			textinput::applyFrame(&webApplication.engineRunner.engine);
		}
		if webApplication.engineRunner.engine.isExitRequested() {
			return;
		}
		if let Some(frameCallback) = frameCallbackCellForLoop.borrow().as_ref() {
			requestAnimationFrame(frameCallback);
		}
	}));
	if let Some(frameCallback) = frameCallbackCell.borrow().as_ref() {
		requestAnimationFrame(frameCallback);
	}
	Ok(())
}

//==============================================================================
// 다음 프레임 요청.
//==============================================================================
fn requestAnimationFrame(frameCallback: &Closure<dyn FnMut(f64)>) {
	if let Some(window) = web_sys::window() {
		let _ = window.request_animation_frame(frameCallback.as_ref().unchecked_ref());
	}
}

//==============================================================================
// 키보드 입력. (KeyboardEvent.code 는 winit KeyCode 이름과 같은 W3C 규약)
//==============================================================================
fn addKeyboardListeners(webApplicationCell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let webApplicationCellForKeyDown = webApplicationCell.clone();
	let keyDownCallback: Closure<dyn FnMut(web_sys::KeyboardEvent)> = Closure::new(move |keyboardEvent: web_sys::KeyboardEvent| {
		if keyboardEvent.repeat() {
			return;
		}
		let keyCode = keyboardEvent.code();
		webApplicationCellForKeyDown.borrow_mut().engineRunner.engine.getInputManagerMut().pushKey(&keyCode);
	});
	let webApplicationCellForKeyUp = webApplicationCell.clone();
	let keyUpCallback: Closure<dyn FnMut(web_sys::KeyboardEvent)> = Closure::new(move |keyboardEvent: web_sys::KeyboardEvent| {
		let keyCode = keyboardEvent.code();
		webApplicationCellForKeyUp.borrow_mut().engineRunner.engine.getInputManagerMut().popKey(&keyCode);
	});
	let webApplicationCellForBlur = webApplicationCell.clone();
	let blurCallback: Closure<dyn FnMut()> = Closure::new(move || {
		webApplicationCellForBlur.borrow_mut().engineRunner.engine.getInputManagerMut().clear();
	});
	window.add_event_listener_with_callback("keydown", keyDownCallback.as_ref().unchecked_ref()).map_err(describeJsError)?;
	window.add_event_listener_with_callback("keyup", keyUpCallback.as_ref().unchecked_ref()).map_err(describeJsError)?;
	window.add_event_listener_with_callback("blur", blurCallback.as_ref().unchecked_ref()).map_err(describeJsError)?;
	keyDownCallback.forget();
	keyUpCallback.forget();
	blurCallback.forget();
	Ok(())
}

//==============================================================================
// 오디오 재개 이벤트. (원본 setupAllDocumentEvents 의 오디오 부분)
// - 프레임 안에서 이벤트가 바로 불려도 겹쳐 빌리지 않게 try_borrow 로 본다. (못 빌리면 다음 이벤트에 맡긴다)
//==============================================================================
fn addAudioListeners(webApplicationCell: &Rc<RefCell<WebApplication>>) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let document = window.document().ok_or_else(|| String::from("document 없음"))?;

	// click / touchstart / keydown 은 진짜 user gesture.
	let webApplicationCellForGesture = webApplicationCell.clone();
	let userGestureCallback: Closure<dyn FnMut()> = Closure::new(move || {
		if let Ok(webApplication) = webApplicationCellForGesture.try_borrow() {
			webApplication.engineRunner.engine.resumeOnUserGesture();
		}
	});
	let mut userGestureEventNames = vec!["click", "touchstart", "keydown"];
	let isResumeAudioOnTouchEnd = webApplicationCell.borrow().engineRunner.engine.getEngineConfiguration().resumeAudioOnTouchEnd;
	if isResumeAudioOnTouchEnd {
		userGestureEventNames.push("touchend");
		userGestureEventNames.push("pointerup");
	}
	for eventName in userGestureEventNames {
		window.add_event_listener_with_callback(eventName, userGestureCallback.as_ref().unchecked_ref()).map_err(describeJsError)?;
	}

	// focus 와 다시 보일 때는 일반 재개. (gesture 전이면 아무것도 하지 않는다)
	let webApplicationCellForFocus = webApplicationCell.clone();
	let focusCallback: Closure<dyn FnMut()> = Closure::new(move || {
		if let Ok(webApplication) = webApplicationCellForFocus.try_borrow() {
			webApplication.engineRunner.engine.resume();
		}
	});
	let webApplicationCellForVisibility = webApplicationCell.clone();
	let visibilityCallback: Closure<dyn FnMut()> = Closure::new(move || {
		let isVisible = web_sys::window()
			.and_then(|window| window.document())
			.map(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
			.unwrap_or(false);
		if !isVisible {
			return;
		}
		if let Ok(webApplication) = webApplicationCellForVisibility.try_borrow() {
			webApplication.engineRunner.engine.resume();
		}
	});
	window.add_event_listener_with_callback("focus", focusCallback.as_ref().unchecked_ref()).map_err(describeJsError)?;
	document.add_event_listener_with_callback("visibilitychange", visibilityCallback.as_ref().unchecked_ref()).map_err(describeJsError)?;
	userGestureCallback.forget();
	focusCallback.forget();
	visibilityCallback.forget();
	Ok(())
}

//==============================================================================
// 자바스크립트 오류를 글로.
//==============================================================================
fn describeJsError(error: JsValue) -> String {
	format!("{:?}", error)
}

impl WebApplication {
	//==============================================================================
	// 캔버스와 WebGL2 컨텍스트, 그래픽 만들기.
	//==============================================================================
	fn createCanvas(&mut self) -> Result<(), String> {
		let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
		let document = window.document().ok_or_else(|| String::from("document 없음"))?;
		let canvasId = self.engineRunner.engine.getEngineConfiguration().canvasId.clone();
		let canvas: web_sys::HtmlCanvasElement = match document.get_element_by_id(&canvasId) {
			Some(element) => {
				element.dyn_into::<web_sys::HtmlCanvasElement>().map_err(|_| format!("#{} 가 캔버스가 아님", canvasId))?
			}
			None => {
				let element = document.create_element("canvas").map_err(describeJsError)?;
				element.set_id(&canvasId);
				let body = document.body().ok_or_else(|| String::from("body 없음"))?;
				body.append_child(&element).map_err(describeJsError)?;
				element.dyn_into::<web_sys::HtmlCanvasElement>().map_err(|_| String::from("캔버스 생성 실패"))?
			}
		};

		// 원본 graphic.js 와 같은 컨텍스트 옵션.
		let contextOptions = js_sys::Object::new();
		for (optionName, optionValue) in [("alpha", false), ("stencil", true), ("premultipliedAlpha", true), ("preserveDrawingBuffer", false)] {
			js_sys::Reflect::set(&contextOptions, &JsValue::from_str(optionName), &JsValue::from_bool(optionValue)).map_err(describeJsError)?;
		}
		let context = canvas
			.get_context_with_context_options("webgl2", &contextOptions)
			.map_err(describeJsError)?
			.ok_or_else(|| String::from("WebGL2 Not Supported."))?
			.dyn_into::<web_sys::WebGl2RenderingContext>()
			.map_err(|_| String::from("WebGL2 컨텍스트 변환 실패"))?;
		let gl = glow::Context::from_webgl2_context(context);
		let graphic = Graphic::new(gl)?;

		self.engineRunner.attachGraphic(graphic);
		self.engineRunner.engine.platformWindow = Some(canvas.clone());
		self.windowState = Some(WindowState {
			canvas,
			clientWidth: 0.0,
			clientHeight: 0.0,
			devicePixelRatio: 0.0,
		});
		self.resizeIfChanged();
		Ok(())
	}

	//==============================================================================
	// 창 크기나 배율이 바뀌었으면 캔버스를 맞추고 뷰 영역을 다시 계산.
	// (원본 viewManager.calculateViewRect 처럼 캔버스를 창 안쪽에 꽉 채운다)
	//==============================================================================
	fn resizeIfChanged(&mut self) {
		let window = match web_sys::window() {
			Some(window) => {
				window
			}
			None => {
				return;
			}
		};
		let clientWidth = window.inner_width().ok().and_then(|value| value.as_f64()).unwrap_or(0.0);
		let clientHeight = window.inner_height().ok().and_then(|value| value.as_f64()).unwrap_or(0.0);
		let devicePixelRatio = if window.device_pixel_ratio() > 0.0 {
			window.device_pixel_ratio()
		}
		else {
			1.0
		};
		let windowState = match self.windowState.as_mut() {
			Some(windowState) => {
				windowState
			}
			None => {
				return;
			}
		};
		if windowState.clientWidth == clientWidth && windowState.clientHeight == clientHeight && windowState.devicePixelRatio == devicePixelRatio {
			return;
		}
		windowState.clientWidth = clientWidth;
		windowState.clientHeight = clientHeight;
		windowState.devicePixelRatio = devicePixelRatio;
		let canvasStyle = windowState.canvas.style();
		let _ = canvasStyle.set_property("width", &format!("{}px", clientWidth));
		let _ = canvasStyle.set_property("height", &format!("{}px", clientHeight));
		let canvasPixelWidth = (clientWidth * devicePixelRatio).round() as u32;
		let canvasPixelHeight = (clientHeight * devicePixelRatio).round() as u32;
		windowState.canvas.set_width(canvasPixelWidth);
		windowState.canvas.set_height(canvasPixelHeight);
		self.engineRunner.engine.getViewManagerMut().calculateViewRect(canvasPixelWidth, canvasPixelHeight, devicePixelRatio as f32);
		self.engineRunner.notifySceneResize();
	}
}
