//==============================================================================
// 웹 글 입력. (숨긴 DOM <input> → 엔진 글 입력 줄, DOM 문맥 메뉴, 캔버스 모드 키, 클립보드)
// - 원본 UIInputField 의 DOM 쪽을 옮긴 것이다. 원본은 필드가 자기 <input> 을 만들고 리스너를 단다.
//   러스트 판은 필드가 프레임마다 상태(자리, 값, 선택, 포커스)를 엔진에 올리고, 여기서 attach 한 DOM 입력 필드마다 숨긴 <input> 을 하나씩 맞춘다.
//   - 값과 선택은 DOM 이 주인이다. input / compositionstart / compositionupdate / compositionend / Enter keydown / focus / blur 를
//     그 필드 id 로 글 입력 줄에 넣고(replaceValue, 조합, 포커스), 화살표 같은 네이티브 이동은 프레임마다 선택을 폴링해 넣는다. (원본 tick)
//   - 필드 쪽에서 값이나 커서를 바꾸면(set_text, 누른 자리, 잘라내기) 상태의 value_revision 이 늘고, 여기서 <input> 에 다시 쓴다. (조합 중엔 미룬다)
//   - iOS 는 사용자 제스처 안에서 focus() 해야 키보드가 뜬다. 원본처럼 캔버스 touchstart / mousedown(설정하면 touchend)에 동기 핸들러를 달아
//     지난 프레임 필드 자리로 맞춰 보고 그 자리에서 focus / blur 한다.
//   - 오른쪽 단추 문맥 메뉴는 원본과 같은 모양의 DOM 메뉴다. 항목은 그 필드에 잘라내기 / 복사 키나 붙여 넣을 글로 넣는다.
//   - use_dominput 이 꺼진 필드는 원본 캔버스 모드처럼 window keydown 을 편집 키와 글자로 바꿔 넣는다.
// - DOM 이벤트 리스너는 엔진을 빌리지 않는다. 대기 줄(PENDING_EVENT_LIST)에 넣고 다음 프레임 시작에 엔진으로 옮긴다.
//   (apply_frame 이 부르는 focus() 가 그 자리에서 focus / blur 이벤트를 부르기 때문)
// - 원본과 다른 것: 숨긴 <input> 은 필드가 tick 을 도는 동안만 있다. (노드를 끄거나 지우면 함께 빠진다) setTimeout 으로 미루는 커서 옮기기와
//   메뉴 닫기 리스너는 원본과 같다.
//==============================================================================
use crate::core::engine::Engine;
use crate::core::textinput::{convert_character_index_to_utf16_index, convert_utf16_index_to_character_index, TextInputEvent, TextInputEventKind, TextInputFieldState, TextInputKey};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 숨긴 <input> 의 클래스 이름. (원본과 같음)
const HIDDEN_INPUT_CLASS_NAME: &str = "uiinputfield-hidden";
// 한 번만 넣는 전역 스타일 id. (원본 GLOBAL_STYLE_ID)
const GLOBAL_STYLE_ID: &str = "uiinputfield-hidden-style";
// 숨긴 <input> 의 선택, 안내 글, 자동 완성 색을 지우는 스타일. (원본 ensure_global_hidden_input_style)
const GLOBAL_STYLE_TEXT: &str = "
input.uiinputfield-hidden::selection { background: transparent; color: transparent; }
input.uiinputfield-hidden::-moz-selection { background: transparent; color: transparent; }
input.uiinputfield-hidden::placeholder { color: transparent; }
input.uiinputfield-hidden::-webkit-input-placeholder { color: transparent; }
input.uiinputfield-hidden:-ms-input-placeholder { color: transparent; }
input.uiinputfield-hidden::-ms-clear { display: none; }
input.uiinputfield-hidden::-ms-reveal { display: none; }
input.uiinputfield-hidden:-webkit-autofill,
input.uiinputfield-hidden:-webkit-autofill:hover,
input.uiinputfield-hidden:-webkit-autofill:focus { -webkit-text-fill-color: transparent !important; transition: background-color 9999s ease-in-out 0s; }
";
// 숨긴 <input> 의 처음 스타일. (원본 attachDOMInput)
const HIDDEN_INPUT_STYLE_LIST: [(&str, &str); 20] = [
	("position", "absolute"),
	("left", "-9999px"),
	("top", "-9999px"),
	("width", "1px"),
	("height", "1px"),
	("z-index", "1000"),
	("pointer-events", "none"),
	("box-sizing", "border-box"),
	("outline", "none"),
	("border", "none"),
	("background", "transparent"),
	("color", "transparent"),
	("caret-color", "transparent"),
	("text-shadow", "none"),
	("box-shadow", "none"),
	("padding", "0"),
	("margin", "0"),
	("font-family", "inherit"),
	("-webkit-tap-highlight-color", "transparent"),
	// iOS 줌인 방지.
	("font-size", "16px"),
];
// 문맥 메뉴 스타일. (원본 show_context_menu)
const CONTEXT_MENU_STYLE_LIST: [(&str, &str); 12] = [
	("position", "absolute"),
	("z-index", "10000"),
	("background", "#ffffff"),
	("color", "#222222"),
	("border", "1px solid #cccccc"),
	("border-radius", "8px"),
	("box-shadow", "0 4px 16px rgba(0, 0, 0, 0.18)"),
	("padding", "4px 0"),
	("font-family", "system-ui, -apple-system, sans-serif"),
	("font-size", "14px"),
	("min-width", "140px"),
	("user-select", "none"),
];

// DOM 이벤트 리스너.
type EventListener = Closure<dyn FnMut(web_sys::Event)>;

//==============================================================================
// 필드 하나의 숨긴 <input>.
//==============================================================================
struct HiddenInputEntry {
	field_id: u64,
	input_element: web_sys::HtmlInputElement,
	listener_list: Vec<(&'static str, EventListener)>,
	applied_revision: Option<u64>,
	applied_max_length: usize,
	applied_pointer_events: String,
	is_focus_applied: bool,
	known_selection: (usize, usize),
	is_composing: bool,
	use_direct_touch: Rc<Cell<bool>>,
	is_focused_without_activation: Rc<Cell<bool>>,
}

//==============================================================================
// 열린 DOM 문맥 메뉴.
//==============================================================================
struct WebContextMenu {
	menu_serial: u64,
	field_id: u64,
	menu_element: web_sys::HtmlElement,
	listener_list: Vec<EventListener>,
	dismiss_listener: Option<EventListener>,
}

//==============================================================================
// 웹 글 입력 상태.
//==============================================================================
struct WebTextInputState {
	canvas: Option<web_sys::HtmlCanvasElement>,
	entry_list: Vec<HiddenInputEntry>,
	field_state_list: Vec<TextInputFieldState>,
	is_canvas_keyboard_focused: bool,
	permanent_listener_list: Vec<EventListener>,
}

thread_local! {
	// 웹 글 입력 상태. (apply_frame 과 캔버스 핸들러가 쓴다. DOM 의 focus / blur 를 부르는 동안에는 빌리지 않는다)
	static WEB_TEXT_INPUT: RefCell<WebTextInputState> = const { RefCell::new(WebTextInputState {
		canvas: None,
		entry_list: Vec::new(),
		field_state_list: Vec::new(),
		is_canvas_keyboard_focused: false,
		permanent_listener_list: Vec::new(),
	}) };
	// 리스너가 넣는 대기 줄. (다음 프레임 시작에 엔진 글 입력 줄로 옮긴다)
	static PENDING_EVENT_LIST: RefCell<Vec<TextInputEvent>> = const { RefCell::new(Vec::new()) };
	// 열린 문맥 메뉴.
	static CONTEXT_MENU: RefCell<Option<WebContextMenu>> = const { RefCell::new(None) };
	// 다 쓴 리스너. (불리는 중에 버리지 않게 다음 프레임에 버린다)
	static DISPOSED_LISTENER_LIST: RefCell<Vec<EventListener>> = const { RefCell::new(Vec::new()) };
	// 문맥 메뉴 번호.
	static NEXT_MENU_SERIAL: Cell<u64> = const { Cell::new(1) };
}

//==============================================================================
// 대기 줄에 이벤트 넣기.
//==============================================================================
fn push_pending_event(target_field_id: u64, kind: TextInputEventKind) {
	PENDING_EVENT_LIST.with(|pending_event_cell| {
		if let Ok(mut pending_event_list) = pending_event_cell.try_borrow_mut() {
			pending_event_list.push(TextInputEvent {
				target_field_id,
				kind,
			});
		}
	});
}

//==============================================================================
// 자바스크립트 오류를 글로.
//==============================================================================
fn describe_js_error(error: JsValue) -> String {
	format!("{:?}", error)
}

//==============================================================================
// 문서.
//==============================================================================
fn get_document() -> Option<web_sys::Document> {
	web_sys::window().and_then(|window| window.document())
}

//==============================================================================
// 요소가 지금 포커스를 가졌는지.
//==============================================================================
fn is_active_element(element: &web_sys::Element) -> bool {
	match get_document().and_then(|document| document.active_element()) {
		Some(active_element) => {
			&active_element == element
		}
		None => {
			false
		}
	}
}

//==============================================================================
// <input> 의 선택. (문자 자리, 앞 뒤)
//==============================================================================
fn read_input_selection(input_element: &web_sys::HtmlInputElement) -> Option<(usize, usize)> {
	let value = input_element.value();
	let selection_start = input_element.selection_start().ok().flatten()?;
	let selection_end = input_element.selection_end().ok().flatten()?;
	Some((convert_utf16_index_to_character_index(&value, selection_start as usize), convert_utf16_index_to_character_index(&value, selection_end as usize)))
}

//==============================================================================
// <input> 의 값 바뀜 이벤트 만들기. (원본 handleDOMInputEvent 에 넘기는 값과 커서)
//==============================================================================
fn build_replace_value_event(input_element: &web_sys::HtmlInputElement) -> TextInputEventKind {
	let value = input_element.value();
	let character_count = value.chars().count();
	let (selection_start, selection_end) = read_input_selection(input_element).unwrap_or((character_count, character_count));
	TextInputEventKind::ReplaceValue {
		value,
		selection_start,
		selection_end,
	}
}

//==============================================================================
// 이 페이지에 사용자 활성화가 한 번이라도 있었는지. (navigator.user_activation 이 없으면 있었던 것으로 본다)
//==============================================================================
fn has_sticky_user_activation() -> bool {
	let navigator = match web_sys::window() {
		Some(window) => {
			window.navigator()
		}
		None => {
			return true;
		}
	};
	let user_activation = js_sys::Reflect::get(&navigator, &JsValue::from_str("userActivation")).unwrap_or(JsValue::UNDEFINED);
	if user_activation.is_undefined() || user_activation.is_null() {
		return true;
	}
	js_sys::Reflect::get(&user_activation, &JsValue::from_str("hasBeenActive")).ok().and_then(|value| value.as_bool()).unwrap_or(true)
}

//==============================================================================
// 0 ms 뒤에 한 번 부르기. (원본 setTimeout(..., 0))
//==============================================================================
fn run_after_timeout(action: impl FnOnce() + 'static) {
	let callback = Closure::once_into_js(action);
	if let Some(window) = web_sys::window() {
		let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0);
	}
}

//==============================================================================
// navigator.clipboard. (없으면 None)
//==============================================================================
fn get_navigator_clipboard() -> Option<JsValue> {
	let navigator = web_sys::window()?.navigator();
	let clipboard = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")).ok()?;
	if clipboard.is_undefined() || clipboard.is_null() {
		return None;
	}
	Some(clipboard)
}

//==============================================================================
// navigator.clipboard 의 함수 부르기. (Promise 를 돌려준다)
//==============================================================================
fn call_clipboard_function(function_name: &str, argument_list: &js_sys::Array) -> Option<js_sys::Promise> {
	let clipboard = get_navigator_clipboard()?;
	let clipboard_function = js_sys::Reflect::get(&clipboard, &JsValue::from_str(function_name)).ok()?.dyn_into::<js_sys::Function>().ok()?;
	let result = js_sys::Reflect::apply(&clipboard_function, &clipboard, argument_list).ok()?;
	result.dyn_into::<js_sys::Promise>().ok()
}

//==============================================================================
// 시스템 클립보드에 글 쓰기. (navigator.clipboard.writeText, 실패는 무시. 원본 writeClipboard)
//==============================================================================
pub fn write_clipboard_text(text: &str) {
	let argument_list = js_sys::Array::of1(&JsValue::from_str(text));
	if let Some(promise) = call_clipboard_function("writeText", &argument_list) {
		wasm_bindgen_futures::spawn_local(async move {
			let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
		});
	}
}

//==============================================================================
// 시스템 클립보드 글 읽기. (웹은 비동기라 그 자리에서 읽지 못한다. read_clipboard_text_with_callback 이나 request_clipboard_paste 로 읽는다)
//==============================================================================
pub fn read_clipboard_text() -> Option<String> {
	None
}

//==============================================================================
// 시스템 클립보드 글을 비동기로 읽어 콜백에 넘기기. (원본 readClipboard 의 navigator.clipboard.readText().then, 못 읽거나 거부되면 빈 글)
//==============================================================================
pub fn read_clipboard_text_with_callback(callback: Box<dyn FnOnce(String)>) {
	read_optional_clipboard_text_with_callback(Box::new(move |text: Option<String>| {
		callback(text.unwrap_or_default());
	}));
}

//==============================================================================
// 시스템 클립보드 글을 비동기로 읽어 콜백에 넘기기. (원본 engine.readClipboardText, 클립보드가 없거나 거부되면 원본 null 자리의 None)
//==============================================================================
pub fn read_optional_clipboard_text_with_callback(callback: Box<dyn FnOnce(Option<String>)>) {
	let promise = match call_clipboard_function("readText", &js_sys::Array::new()) {
		Some(promise) => {
			promise
		}
		None => {
			callback(None);
			return;
		}
	};
	wasm_bindgen_futures::spawn_local(async move {
		let text = match wasm_bindgen_futures::JsFuture::from(promise).await {
			Ok(value) => {
				value.as_string()
			}
			Err(_) => {
				None
			}
		};
		callback(text);
	});
}

//==============================================================================
// 클립보드를 비동기로 읽어 그 필드에 붙여 넣을 글로 넣기. (원본 paste 의 readClipboard().then, UIInputField.paste 와 문맥 메뉴, 단축키가 부른다)
//==============================================================================
pub(crate) fn request_clipboard_paste(target_field_id: u64) {
	let promise = match call_clipboard_function("readText", &js_sys::Array::new()) {
		Some(promise) => {
			promise
		}
		None => {
			return;
		}
	};
	wasm_bindgen_futures::spawn_local(async move {
		if let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await {
			if let Some(text) = value.as_string() {
				if !text.is_empty() {
					push_pending_event(target_field_id, TextInputEventKind::Paste(text));
				}
			}
		}
	});
}

//==============================================================================
// 리스너 달기. (시작할 때. 캔버스 누름 / 문맥 메뉴, window keydown, 전역 스타일)
//==============================================================================
pub(super) fn add_text_input_listeners(canvas: &web_sys::HtmlCanvasElement) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	ensure_global_hidden_input_style()?;

	// 캔버스 누름: 사용자 제스처 안에서 숨긴 <input> 에 focus / blur. (원본 attachCanvasFocusHandler)
	let canvas_focus_listener: EventListener = Closure::new(handle_canvas_pointer);
	let passive_options = web_sys::AddEventListenerOptions::new();
	passive_options.set_passive(true);
	for event_name in ["touchstart", "touchend", "mousedown"] {
		canvas.add_event_listener_with_callback_and_add_event_listener_options(event_name, canvas_focus_listener.as_ref().unchecked_ref(), &passive_options).map_err(describe_js_error)?;
	}

	// 오른쪽 단추 문맥 메뉴. (원본 attachContextMenuHandler)
	let context_menu_listener: EventListener = Closure::new(handle_canvas_context_menu);
	canvas.add_event_listener_with_callback("contextmenu", context_menu_listener.as_ref().unchecked_ref()).map_err(describe_js_error)?;

	// 캔버스 모드 키. (원본 attachCanvasKeyboard)
	let canvas_keyboard_listener: EventListener = Closure::new(handle_canvas_key_down);
	window.add_event_listener_with_callback("keydown", canvas_keyboard_listener.as_ref().unchecked_ref()).map_err(describe_js_error)?;

	WEB_TEXT_INPUT.with(|state_cell| {
		let mut web_text_input_state = state_cell.borrow_mut();
		web_text_input_state.canvas = Some(canvas.clone());
		web_text_input_state.permanent_listener_list.push(canvas_focus_listener);
		web_text_input_state.permanent_listener_list.push(context_menu_listener);
		web_text_input_state.permanent_listener_list.push(canvas_keyboard_listener);
	});
	Ok(())
}

//==============================================================================
// 전역 스타일 한 번 넣기. (원본 ensure_global_hidden_input_style)
//==============================================================================
fn ensure_global_hidden_input_style() -> Result<(), String> {
	let document = get_document().ok_or_else(|| String::from("document 없음"))?;
	if document.get_element_by_id(GLOBAL_STYLE_ID).is_some() {
		return Ok(());
	}
	let style_element = document.create_element("style").map_err(describe_js_error)?;
	style_element.set_id(GLOBAL_STYLE_ID);
	style_element.set_text_content(Some(GLOBAL_STYLE_TEXT));
	let head = document.head().ok_or_else(|| String::from("head 없음"))?;
	head.append_child(&style_element).map_err(describe_js_error)?;
	Ok(())
}

//==============================================================================
// 프레임 시작. (대기 줄을 엔진으로 옮기고, 포커스를 가진 <input> 의 선택을 폴링한다. 원본 tick 의 selection 폴링)
//==============================================================================
pub(super) fn begin_frame(engine: &mut Engine) {
	DISPOSED_LISTENER_LIST.with(|disposed_cell| {
		if let Ok(mut disposed_listener_list) = disposed_cell.try_borrow_mut() {
			disposed_listener_list.clear();
		}
	});
	let pending_event_list = PENDING_EVENT_LIST.with(|pending_event_cell| std::mem::take(&mut *pending_event_cell.borrow_mut()));
	let text_input = engine.get_input_manager_mut().get_text_input_mut();
	WEB_TEXT_INPUT.with(|state_cell| {
		let mut web_text_input_state = state_cell.borrow_mut();
		for pending_event in pending_event_list {
			let is_composition_start = pending_event.kind == TextInputEventKind::CompositionStart;
			let is_composition_end = pending_event.kind == TextInputEventKind::CompositionEnd;
			if is_composition_start || is_composition_end {
				if let Some(entry) = web_text_input_state.entry_list.iter_mut().find(|entry| entry.field_id == pending_event.target_field_id) {
					entry.is_composing = is_composition_start;
				}
			}
			text_input.push_event(pending_event);
		}

		// 키로 옮긴 커서 / 선택. (조합 중이 아닐 때, 지난번과 다르면)
		for entry in web_text_input_state.entry_list.iter_mut() {
			if entry.is_composing || !is_active_element(&entry.input_element) {
				continue;
			}
			let selection = match read_input_selection(&entry.input_element) {
				Some(selection) => {
					selection
				}
				None => {
					continue;
				}
			};
			if selection == entry.known_selection {
				continue;
			}
			entry.known_selection = selection;
			text_input.push_event(TextInputEvent {
				target_field_id: entry.field_id,
				kind: TextInputEventKind::Selection {
					start: selection.0,
					end: selection.1,
				},
			});
		}
	});
}

//==============================================================================
// 프레임 끝. (지난 프레임 필드 상태로 숨긴 <input> 을 만들고, 옮기고, 값과 포커스를 맞춘다. 원본 layoutDOMInput)
//==============================================================================
pub(super) fn apply_frame(engine: &Engine) {
	let field_state_list = engine.get_input_manager().get_text_input().get_published_field_states().to_vec();
	let mut focus_action_list: Vec<(web_sys::HtmlInputElement, bool)> = Vec::new();
	let mut removed_entry_list: Vec<HiddenInputEntry> = Vec::new();
	WEB_TEXT_INPUT.with(|state_cell| {
		let mut web_text_input_state = state_cell.borrow_mut();
		let canvas_rect = match &web_text_input_state.canvas {
			Some(canvas) => {
				canvas.get_bounding_client_rect()
			}
			None => {
				return;
			}
		};

		// 더는 올라오지 않는 필드의 <input> 은 뺀다.
		let entry_list = std::mem::take(&mut web_text_input_state.entry_list);
		for entry in entry_list {
			let is_still_registered = field_state_list.iter().any(|field_state| field_state.field_id == entry.field_id && field_state.is_attached && field_state.use_dominput);
			if is_still_registered {
				web_text_input_state.entry_list.push(entry);
			}
			else {
				removed_entry_list.push(entry);
			}
		}

		for field_state in field_state_list.iter() {
			if !field_state.is_attached || !field_state.use_dominput {
				continue;
			}
			let entry_index = match web_text_input_state.entry_list.iter().position(|entry| entry.field_id == field_state.field_id) {
				Some(entry_index) => {
					entry_index
				}
				None => {
					match create_hidden_input_entry(field_state) {
						Ok(entry) => {
							web_text_input_state.entry_list.push(entry);
							web_text_input_state.entry_list.len() - 1
						}
						Err(error) => {
							web_sys::console::error_1(&JsValue::from_str(&format!("Engine: 숨긴 입력 만들기 실패: {}", error)));
							continue;
						}
					}
				}
			};
			let entry = &mut web_text_input_state.entry_list[entry_index];
			apply_field_state_to_entry(entry, field_state, &canvas_rect, &mut focus_action_list);
		}
		web_text_input_state.field_state_list = field_state_list.clone();
		web_text_input_state.is_canvas_keyboard_focused = field_state_list.iter().any(|field_state| field_state.is_focused && field_state.is_attached && !field_state.use_dominput);
	});

	// 빌림을 놓은 뒤에 DOM 을 바꾼다. (focus / blur 이벤트가 그 자리에서 불린다)
	for entry in removed_entry_list {
		remove_hidden_input_entry(entry);
	}
	for (input_element, is_focus_requested) in focus_action_list {
		if is_focus_requested {
			let _ = input_element.focus();
		}
		else if is_active_element(&input_element) {
			let _ = input_element.blur();
		}
	}

	// 문맥 메뉴의 필드가 포커스를 잃었으면 닫는다. (원본 handle_blur 의 hide_context_menu)
	let menu_field_id = CONTEXT_MENU.with(|menu_cell| menu_cell.borrow().as_ref().map(|context_menu| context_menu.field_id));
	if let Some(menu_field_id) = menu_field_id {
		let is_menu_field_focused = field_state_list.iter().any(|field_state| field_state.field_id == menu_field_id && field_state.is_focused);
		if !is_menu_field_focused {
			hide_context_menu();
		}
	}
}

//==============================================================================
// 필드 상태를 숨긴 <input> 에 적용. (자리, 누름 받기, 최대 길이, 필드 쪽에서 바꾼 값과 선택, 포커스 바뀜)
//==============================================================================
fn apply_field_state_to_entry(entry: &mut HiddenInputEntry, field_state: &TextInputFieldState, canvas_rect: &web_sys::DomRect, focus_action_list: &mut Vec<(web_sys::HtmlInputElement, bool)>) {
	let input_style = entry.input_element.style();
	let field_rect = field_state.field_rect;
	let _ = input_style.set_property("left", &format!("{}px", canvas_rect.left() + field_rect.position.x as f64));
	let _ = input_style.set_property("top", &format!("{}px", canvas_rect.top() + field_rect.position.y as f64));
	let _ = input_style.set_property("width", &format!("{}px", field_rect.size.x));
	let _ = input_style.set_property("height", &format!("{}px", field_rect.size.y));

	// 직접 누름이면 보이는 동안 누름을 받는다. (원본 layoutDOMInput)
	entry.use_direct_touch.set(field_state.use_direct_touch);
	let pointer_events = if field_state.use_direct_touch && field_state.is_visible {
		"auto"
	}
	else {
		"none"
	};
	if entry.applied_pointer_events != pointer_events {
		entry.applied_pointer_events = pointer_events.to_string();
		let _ = input_style.set_property("pointer-events", pointer_events);
		if field_state.use_direct_touch {
			let _ = input_style.set_property("user-select", "text");
			let _ = input_style.set_property("-webkit-user-select", "text");
		}
	}

	if entry.applied_max_length != field_state.max_length {
		entry.applied_max_length = field_state.max_length;
		if field_state.max_length > 0 {
			entry.input_element.set_max_length(field_state.max_length as i32);
		}
		else {
			let _ = entry.input_element.remove_attribute("maxlength");
		}
	}

	// 필드 쪽에서 바꾼 값과 선택. (조합 중에는 IME 를 깨지 않게 미룬다)
	if !entry.is_composing && entry.applied_revision != Some(field_state.value_revision) {
		entry.applied_revision = Some(field_state.value_revision);
		if entry.input_element.value() != field_state.value {
			entry.input_element.set_value(&field_state.value);
		}
		let selection_start = field_state.selection_start.min(field_state.cursor_index);
		let selection_end = field_state.selection_start.max(field_state.cursor_index);
		let utf16_start = convert_character_index_to_utf16_index(&field_state.value, selection_start) as u32;
		let utf16_end = convert_character_index_to_utf16_index(&field_state.value, selection_end) as u32;
		let _ = entry.input_element.set_selection_range(utf16_start, utf16_end);
		entry.known_selection = (selection_start, selection_end);
	}

	// 포커스 바뀜. (바뀔 때만 한다. DOM 에서 온 포커스는 리스너가 이미 맞춰 둔다)
	if field_state.is_focused != entry.is_focus_applied {
		entry.is_focus_applied = field_state.is_focused;
		focus_action_list.push((entry.input_element.clone(), field_state.is_focused));
	}
}

//==============================================================================
// 숨긴 <input> 만들기. (원본 attachDOMInput)
//==============================================================================
fn create_hidden_input_entry(field_state: &TextInputFieldState) -> Result<HiddenInputEntry, String> {
	let document = get_document().ok_or_else(|| String::from("document 없음"))?;
	let input_element = document.create_element("input").map_err(describe_js_error)?.dyn_into::<web_sys::HtmlInputElement>().map_err(|_| String::from("input 변환 실패"))?;
	input_element.set_type("text");
	input_element.set_value(&field_state.value);
	if field_state.max_length > 0 {
		input_element.set_max_length(field_state.max_length as i32);
	}
	input_element.set_autocomplete("off");
	input_element.set_spellcheck(false);
	input_element.set_class_name(HIDDEN_INPUT_CLASS_NAME);
	let input_style = input_element.style();
	for (property_name, property_value) in HIDDEN_INPUT_STYLE_LIST {
		let _ = input_style.set_property(property_name, property_value);
	}

	let field_id = field_state.field_id;
	let use_direct_touch = Rc::new(Cell::new(field_state.use_direct_touch));
	let is_focused_without_activation = Rc::new(Cell::new(false));
	let mut listener_list: Vec<(&'static str, EventListener)> = Vec::new();

	// 포커스. (직접 누름이면 브라우저가 누른 자리에 둔 커서를 글 끝으로 옮긴다)
	let input_for_focus = input_element.clone();
	let use_direct_touch_for_focus = use_direct_touch.clone();
	let is_focused_without_activation_for_focus = is_focused_without_activation.clone();
	listener_list.push(("focus", Closure::new(move |_event: web_sys::Event| {
		is_focused_without_activation_for_focus.set(!has_sticky_user_activation());
		push_pending_event(field_id, TextInputEventKind::FocusGained);
		if use_direct_touch_for_focus.get() {
			let input_for_caret = input_for_focus.clone();
			run_after_timeout(move || {
				let length = input_for_caret.value().encode_utf16().count() as u32;
				let _ = input_for_caret.set_selection_range(length, length);
			});
		}
	})));
	listener_list.push(("blur", Closure::new(move |_event: web_sys::Event| {
		push_pending_event(field_id, TextInputEventKind::FocusLost);
	})));
	let input_for_input = input_element.clone();
	listener_list.push(("input", Closure::new(move |_event: web_sys::Event| {
		push_pending_event(field_id, build_replace_value_event(&input_for_input));
	})));
	listener_list.push(("compositionstart", Closure::new(move |_event: web_sys::Event| {
		push_pending_event(field_id, TextInputEventKind::CompositionStart);
	})));
	listener_list.push(("compositionupdate", Closure::new(move |event: web_sys::Event| {
		let composition_text = event.dyn_ref::<web_sys::CompositionEvent>().and_then(|composition_event| composition_event.data()).unwrap_or_default();
		push_pending_event(field_id, TextInputEventKind::Preedit {
			text: composition_text,
			cursor_index: None,
		});
	})));
	let input_for_composition_end = input_element.clone();
	listener_list.push(("compositionend", Closure::new(move |_event: web_sys::Event| {
		push_pending_event(field_id, TextInputEventKind::CompositionEnd);
		push_pending_event(field_id, build_replace_value_event(&input_for_composition_end));
	})));
	listener_list.push(("keydown", Closure::new(move |event: web_sys::Event| {
		let is_enter = event.dyn_ref::<web_sys::KeyboardEvent>().map(|keyboard_event| keyboard_event.key() == "Enter").unwrap_or(false);
		if is_enter {
			event.prevent_default();
			push_pending_event(field_id, TextInputEventKind::Key {
				key: TextInputKey::Enter,
				is_shift: false,
			});
		}
	})));
	for (event_name, listener) in listener_list.iter() {
		input_element.add_event_listener_with_callback(event_name, listener.as_ref().unchecked_ref()).map_err(describe_js_error)?;
	}
	let body = document.body().ok_or_else(|| String::from("body 없음"))?;
	body.append_child(&input_element).map_err(describe_js_error)?;
	let character_count = field_state.value.chars().count();
	Ok(HiddenInputEntry {
		field_id,
		input_element,
		listener_list,
		applied_revision: None,
		applied_max_length: field_state.max_length,
		applied_pointer_events: String::from("none"),
		is_focus_applied: false,
		known_selection: (character_count, character_count),
		is_composing: false,
		use_direct_touch,
		is_focused_without_activation,
	})
}

//==============================================================================
// 숨긴 <input> 빼기. (리스너를 먼저 떼서 빼는 동안의 blur 가 오지 않게 한다. 원본 detachDOMInput)
//==============================================================================
fn remove_hidden_input_entry(entry: HiddenInputEntry) {
	for (event_name, listener) in entry.listener_list.iter() {
		let _ = entry.input_element.remove_event_listener_with_callback(event_name, listener.as_ref().unchecked_ref());
	}
	entry.input_element.remove();
	let listener_list: Vec<EventListener> = entry.listener_list.into_iter().map(|(_, listener)| listener).collect();
	dispose_listeners(listener_list);
}

//==============================================================================
// 다 쓴 리스너 버리기. (다음 프레임 시작에 버린다)
//==============================================================================
fn dispose_listeners(listener_list: Vec<EventListener>) {
	DISPOSED_LISTENER_LIST.with(|disposed_cell| {
		if let Ok(mut disposed_listener_list) = disposed_cell.try_borrow_mut() {
			disposed_listener_list.extend(listener_list);
		}
	});
}

//==============================================================================
// 포인터 이벤트의 화면 좌표. (터치면 바뀐 첫 손가락)
//==============================================================================
fn read_client_position(event: &web_sys::Event) -> Option<(f64, f64)> {
	if let Some(touch_event) = event.dyn_ref::<web_sys::TouchEvent>() {
		let touch = touch_event.changed_touches().get(0)?;
		return Some((touch.client_x() as f64, touch.client_y() as f64));
	}
	let mouse_event = event.dyn_ref::<web_sys::MouseEvent>()?;
	Some((mouse_event.client_x() as f64, mouse_event.client_y() as f64))
}

//==============================================================================
// 화면 좌표 아래 맨 위의 보이는 필드 상태. (DOM 입력 필드만 보려면 is_dom_input_only)
//==============================================================================
fn find_field_state_at_client_position(web_text_input_state: &WebTextInputState, client_x: f64, client_y: f64, is_dom_input_only: bool) -> Option<TextInputFieldState> {
	let canvas_rect = web_text_input_state.canvas.as_ref()?.get_bounding_client_rect();
	let canvas_x = (client_x - canvas_rect.left()) as f32;
	let canvas_y = (client_y - canvas_rect.top()) as f32;
	web_text_input_state.field_state_list.iter().rev().find(|field_state| {
		let field_rect = field_state.field_rect;
		let is_inside = canvas_x >= field_rect.position.x && canvas_x <= field_rect.position.x + field_rect.size.x && canvas_y >= field_rect.position.y && canvas_y <= field_rect.position.y + field_rect.size.y;
		is_inside && field_state.is_visible && field_state.is_attached && (!is_dom_input_only || field_state.use_dominput)
	}).cloned()
}

//==============================================================================
// 캔버스 누름 처리. (원본 canvasFocusHandler)
// - 필드 안: 그 자리에서 focus. touchend 는 set_use_touch_end_focus 를 켠 필드만, 활성화 없이 얻은 포커스면 blur 뒤 다시 focus.
// - 필드 밖(누를 때만): 포커스를 가진 <input> 을 blur.
//==============================================================================
fn handle_canvas_pointer(event: web_sys::Event) {
	let is_release = event.type_() == "touchend";
	let (client_x, client_y) = match read_client_position(&event) {
		Some(client_position) => {
			client_position
		}
		None => {
			return;
		}
	};
	// 빌린 동안 할 일을 정하고, 놓은 뒤에 DOM 을 바꾼다.
	let mut focus_target: Option<(web_sys::HtmlInputElement, bool)> = None;
	let mut blur_target_list: Vec<web_sys::HtmlInputElement> = Vec::new();
	WEB_TEXT_INPUT.with(|state_cell| {
		let mut web_text_input_state = state_cell.borrow_mut();
		let hit_field_state = find_field_state_at_client_position(&web_text_input_state, client_x, client_y, true);
		match hit_field_state {
			Some(hit_field_state) => {
				if is_release && !hit_field_state.use_touch_end_focus {
					return;
				}
				if let Some(entry) = web_text_input_state.entry_list.iter_mut().find(|entry| entry.field_id == hit_field_state.field_id) {
					let is_refocus_needed = is_release && hit_field_state.is_focused && entry.is_focused_without_activation.get();
					entry.is_focus_applied = true;
					focus_target = Some((entry.input_element.clone(), is_refocus_needed));
				}
			}
			None => {
				if is_release {
					return;
				}
				for entry in web_text_input_state.entry_list.iter_mut() {
					if entry.is_focus_applied || is_active_element(&entry.input_element) {
						entry.is_focus_applied = false;
						blur_target_list.push(entry.input_element.clone());
					}
				}
			}
		}
	});
	if let Some((input_element, is_refocus_needed)) = focus_target {
		if is_refocus_needed {
			let _ = input_element.blur();
		}
		let _ = input_element.focus();
	}
	for input_element in blur_target_list {
		let _ = input_element.blur();
	}
}

//==============================================================================
// 캔버스 문맥 메뉴 처리. (필드 위면 브라우저 메뉴를 막고 포커스를 준 뒤 DOM 메뉴를 연다. 원본 handleContextMenuEvent)
//==============================================================================
fn handle_canvas_context_menu(event: web_sys::Event) {
	let (client_x, client_y) = match read_client_position(&event) {
		Some(client_position) => {
			client_position
		}
		None => {
			return;
		}
	};
	let mut focus_element: Option<web_sys::HtmlInputElement> = None;
	let hit_field_state = WEB_TEXT_INPUT.with(|state_cell| {
		let mut web_text_input_state = state_cell.borrow_mut();
		let hit_field_state = find_field_state_at_client_position(&web_text_input_state, client_x, client_y, false)?;
		if let Some(entry) = web_text_input_state.entry_list.iter_mut().find(|entry| entry.field_id == hit_field_state.field_id) {
			entry.is_focus_applied = true;
			focus_element = Some(entry.input_element.clone());
		}
		Some(hit_field_state)
	});
	let hit_field_state = match hit_field_state {
		Some(hit_field_state) => {
			hit_field_state
		}
		None => {
			return;
		}
	};
	event.prevent_default();
	event.stop_propagation();
	match focus_element {
		Some(input_element) => {
			let _ = input_element.focus();
		}
		None => {
			push_pending_event(hit_field_state.field_id, TextInputEventKind::FocusGained);
		}
	}
	let has_selection = hit_field_state.selection_start != hit_field_state.cursor_index;
	if let Err(error) = show_context_menu(hit_field_state.field_id, has_selection, client_x, client_y) {
		web_sys::console::error_1(&JsValue::from_str(&format!("Engine: 문맥 메뉴 열기 실패: {}", error)));
	}
}

//==============================================================================
// DOM 문맥 메뉴 열기. (잘라내기 / 복사 / 붙여넣기. 원본 show_context_menu)
//==============================================================================
fn show_context_menu(field_id: u64, has_selection: bool, client_x: f64, client_y: f64) -> Result<(), String> {
	hide_context_menu();
	let document = get_document().ok_or_else(|| String::from("document 없음"))?;
	let menu_element = document.create_element("div").map_err(describe_js_error)?.dyn_into::<web_sys::HtmlElement>().map_err(|_| String::from("div 변환 실패"))?;
	let menu_style = menu_element.style();
	for (property_name, property_value) in CONTEXT_MENU_STYLE_LIST {
		let _ = menu_style.set_property(property_name, property_value);
	}
	let _ = menu_style.set_property("left", &format!("{}px", client_x));
	let _ = menu_style.set_property("top", &format!("{}px", client_y));

	let item_list = [("잘라내기", has_selection, TextInputKey::Cut), ("복사", has_selection, TextInputKey::Copy), ("붙여넣기", true, TextInputKey::Paste)];
	let mut listener_list: Vec<EventListener> = Vec::new();
	for (item_text, is_enabled, item_key) in item_list {
		let item_element = document.create_element("div").map_err(describe_js_error)?.dyn_into::<web_sys::HtmlElement>().map_err(|_| String::from("div 변환 실패"))?;
		item_element.set_text_content(Some(item_text));
		let item_style = item_element.style();
		let _ = item_style.set_property("padding", "8px 16px");
		let _ = item_style.set_property("cursor", if is_enabled {
			"pointer"
		}
		else {
			"default"
		});
		let _ = item_style.set_property("color", if is_enabled {
			"#222222"
		}
		else {
			"#aaaaaa"
		});
		if is_enabled {
			let item_for_enter = item_element.clone();
			let mouse_enter_listener: EventListener = Closure::new(move |_event: web_sys::Event| {
				let _ = item_for_enter.style().set_property("background", "#eef2ff");
			});
			let item_for_leave = item_element.clone();
			let mouse_leave_listener: EventListener = Closure::new(move |_event: web_sys::Event| {
				let _ = item_for_leave.style().set_property("background", "transparent");
			});
			let mouse_down_listener: EventListener = Closure::new(move |event: web_sys::Event| {
				event.prevent_default();
				event.stop_propagation();
			});
			let click_listener: EventListener = Closure::new(move |event: web_sys::Event| {
				event.prevent_default();
				event.stop_propagation();
				match item_key {
					TextInputKey::Paste => {
						request_clipboard_paste(field_id);
					}
					_ => {
						push_pending_event(field_id, TextInputEventKind::Key {
							key: item_key,
							is_shift: false,
						});
					}
				}
				hide_context_menu();
			});
			for (event_name, listener) in [("mouseenter", &mouse_enter_listener), ("mouseleave", &mouse_leave_listener), ("mousedown", &mouse_down_listener), ("click", &click_listener)] {
				item_element.add_event_listener_with_callback(event_name, listener.as_ref().unchecked_ref()).map_err(describe_js_error)?;
			}
			listener_list.push(mouse_enter_listener);
			listener_list.push(mouse_leave_listener);
			listener_list.push(mouse_down_listener);
			listener_list.push(click_listener);
		}
		menu_element.append_child(&item_element).map_err(describe_js_error)?;
	}
	let body = document.body().ok_or_else(|| String::from("body 없음"))?;
	body.append_child(&menu_element).map_err(describe_js_error)?;

	let menu_serial = NEXT_MENU_SERIAL.with(|serial_cell| {
		let menu_serial = serial_cell.get();
		serial_cell.set(menu_serial + 1);
		menu_serial
	});
	CONTEXT_MENU.with(|menu_cell| {
		*menu_cell.borrow_mut() = Some(WebContextMenu {
			menu_serial,
			field_id,
			menu_element,
			listener_list,
			dismiss_listener: None,
		});
	});

	// 메뉴 밖 누름으로 닫기. 다음 차례부터. (지금 누름이 닫기로 처리되지 않게)
	run_after_timeout(move || {
		add_context_menu_dismiss_listener(menu_serial);
	});
	Ok(())
}

//==============================================================================
// 메뉴 밖 누름 리스너 달기. (그 사이 메뉴가 바뀌었으면 달지 않는다)
//==============================================================================
fn add_context_menu_dismiss_listener(menu_serial: u64) {
	let document = match get_document() {
		Some(document) => {
			document
		}
		None => {
			return;
		}
	};
	let dismiss_listener: EventListener = Closure::new(move |event: web_sys::Event| {
		let target_node = event.target().and_then(|target| target.dyn_into::<web_sys::Node>().ok());
		let is_inside_menu = CONTEXT_MENU.with(|menu_cell| {
			match menu_cell.borrow().as_ref() {
				Some(context_menu) => {
					context_menu.menu_element.contains(target_node.as_ref())
				}
				None => {
					true
				}
			}
		});
		if !is_inside_menu {
			hide_context_menu();
		}
	});
	CONTEXT_MENU.with(|menu_cell| {
		let mut context_menu_slot = menu_cell.borrow_mut();
		if let Some(context_menu) = context_menu_slot.as_mut() {
			if context_menu.menu_serial != menu_serial {
				return;
			}
			if document.add_event_listener_with_callback_and_bool("pointerdown", dismiss_listener.as_ref().unchecked_ref(), true).is_ok() {
				context_menu.dismiss_listener = Some(dismiss_listener);
			}
		}
	});
}

//==============================================================================
// DOM 문맥 메뉴 닫기. (원본 hide_context_menu. 리스너는 다음 프레임에 버린다)
//==============================================================================
fn hide_context_menu() {
	let context_menu = CONTEXT_MENU.with(|menu_cell| menu_cell.borrow_mut().take());
	let context_menu = match context_menu {
		Some(context_menu) => {
			context_menu
		}
		None => {
			return;
		}
	};
	context_menu.menu_element.remove();
	let mut listener_list = context_menu.listener_list;
	if let Some(dismiss_listener) = context_menu.dismiss_listener {
		if let Some(document) = get_document() {
			let _ = document.remove_event_listener_with_callback_and_bool("pointerdown", dismiss_listener.as_ref().unchecked_ref(), true);
		}
		listener_list.push(dismiss_listener);
	}
	dispose_listeners(listener_list);
}

//==============================================================================
// 캔버스 모드 키 처리. (use_dominput 이 꺼진 필드가 포커스를 가졌을 때만. 원본 handleCanvasKeydown)
//==============================================================================
fn handle_canvas_key_down(event: web_sys::Event) {
	let keyboard_event = match event.dyn_ref::<web_sys::KeyboardEvent>() {
		Some(keyboard_event) => {
			keyboard_event
		}
		None => {
			return;
		}
	};
	let is_canvas_keyboard_focused = WEB_TEXT_INPUT.with(|state_cell| state_cell.try_borrow().map(|web_text_input_state| web_text_input_state.is_canvas_keyboard_focused).unwrap_or(false));
	if !is_canvas_keyboard_focused {
		return;
	}
	let is_hidden_input_active = get_document().and_then(|document| document.active_element()).map(|active_element| active_element.class_name() == HIDDEN_INPUT_CLASS_NAME).unwrap_or(false);
	if is_hidden_input_active {
		return;
	}
	let key = keyboard_event.key();
	let is_shift = keyboard_event.shift_key();
	if keyboard_event.ctrl_key() || keyboard_event.meta_key() {
		let shortcut_key = match key.to_lowercase().as_str() {
			"a" => {
				Some(TextInputKey::SelectAll)
			}
			"c" => {
				Some(TextInputKey::Copy)
			}
			"x" => {
				Some(TextInputKey::Cut)
			}
			"v" => {
				Some(TextInputKey::Paste)
			}
			_ => {
				None
			}
		};
		if let Some(shortcut_key) = shortcut_key {
			event.prevent_default();
			if shortcut_key == TextInputKey::Paste {
				request_clipboard_paste(0);
			}
			else {
				push_pending_event(0, TextInputEventKind::Key {
					key: shortcut_key,
					is_shift,
				});
			}
		}
		return;
	}
	let edit_key = match key.as_str() {
		"Backspace" => {
			Some(TextInputKey::Backspace)
		}
		"Delete" => {
			Some(TextInputKey::Delete)
		}
		"ArrowLeft" => {
			Some(TextInputKey::Left)
		}
		"ArrowRight" => {
			Some(TextInputKey::Right)
		}
		"Home" => {
			Some(TextInputKey::Home)
		}
		"End" => {
			Some(TextInputKey::End)
		}
		"Enter" => {
			Some(TextInputKey::Enter)
		}
		_ => {
			None
		}
	};
	if let Some(edit_key) = edit_key {
		event.prevent_default();
		push_pending_event(0, TextInputEventKind::Key {
			key: edit_key,
			is_shift,
		});
		return;
	}
	if key.chars().count() == 1 && !keyboard_event.alt_key() {
		event.prevent_default();
		push_pending_event(0, TextInputEventKind::Commit(key));
	}
}
