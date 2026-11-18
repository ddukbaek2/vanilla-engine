//==============================================================================
// 웹 글 입력. (숨긴 DOM <input> → 엔진 글 입력 줄, DOM 문맥 메뉴, 캔버스 모드 키, 클립보드)
// - 원본 UIInputField 의 DOM 쪽을 옮긴 것이다. 원본은 필드가 자기 <input> 을 만들고 리스너를 단다.
//   러스트 판은 필드가 프레임마다 상태(자리, 값, 선택, 포커스)를 엔진에 올리고, 여기서 attach 한 DOM 입력 필드마다 숨긴 <input> 을 하나씩 맞춘다.
//   - 값과 선택은 DOM 이 주인이다. input / compositionstart / compositionupdate / compositionend / Enter keydown / focus / blur 를
//     그 필드 id 로 글 입력 줄에 넣고(replaceValue, 조합, 포커스), 화살표 같은 네이티브 이동은 프레임마다 선택을 폴링해 넣는다. (원본 tick)
//   - 필드 쪽에서 값이나 커서를 바꾸면(setText, 누른 자리, 잘라내기) 상태의 valueRevision 이 늘고, 여기서 <input> 에 다시 쓴다. (조합 중엔 미룬다)
//   - iOS 는 사용자 제스처 안에서 focus() 해야 키보드가 뜬다. 원본처럼 캔버스 touchstart / mousedown(설정하면 touchend)에 동기 핸들러를 달아
//     지난 프레임 필드 자리로 맞춰 보고 그 자리에서 focus / blur 한다.
//   - 오른쪽 단추 문맥 메뉴는 원본과 같은 모양의 DOM 메뉴다. 항목은 그 필드에 잘라내기 / 복사 키나 붙여 넣을 글로 넣는다.
//   - useDOMInput 이 꺼진 필드는 원본 캔버스 모드처럼 window keydown 을 편집 키와 글자로 바꿔 넣는다.
// - DOM 이벤트 리스너는 엔진을 빌리지 않는다. 대기 줄(PENDING_EVENT_LIST)에 넣고 다음 프레임 시작에 엔진으로 옮긴다.
//   (applyFrame 이 부르는 focus() 가 그 자리에서 focus / blur 이벤트를 부르기 때문)
// - 원본과 다른 것: 숨긴 <input> 은 필드가 tick 을 도는 동안만 있다. (노드를 끄거나 지우면 함께 빠진다) setTimeout 으로 미루는 커서 옮기기와
//   메뉴 닫기 리스너는 원본과 같다.
//==============================================================================
use crate::core::engine::Engine;
use crate::core::textinput::{convertCharacterIndexToUtf16Index, convertUtf16IndexToCharacterIndex, TextInputEvent, TextInputEventKind, TextInputFieldState, TextInputKey};
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
// 숨긴 <input> 의 선택, 안내 글, 자동 완성 색을 지우는 스타일. (원본 ensureGlobalHiddenInputStyle)
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
// 문맥 메뉴 스타일. (원본 showContextMenu)
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
	fieldId: u64,
	inputElement: web_sys::HtmlInputElement,
	listenerList: Vec<(&'static str, EventListener)>,
	appliedRevision: Option<u64>,
	appliedMaxLength: usize,
	appliedPointerEvents: String,
	isFocusApplied: bool,
	knownSelection: (usize, usize),
	isComposing: bool,
	useDirectTouch: Rc<Cell<bool>>,
	isFocusedWithoutActivation: Rc<Cell<bool>>,
}

//==============================================================================
// 열린 DOM 문맥 메뉴.
//==============================================================================
struct WebContextMenu {
	menuSerial: u64,
	fieldId: u64,
	menuElement: web_sys::HtmlElement,
	listenerList: Vec<EventListener>,
	dismissListener: Option<EventListener>,
}

//==============================================================================
// 웹 글 입력 상태.
//==============================================================================
struct WebTextInputState {
	canvas: Option<web_sys::HtmlCanvasElement>,
	entryList: Vec<HiddenInputEntry>,
	fieldStateList: Vec<TextInputFieldState>,
	isCanvasKeyboardFocused: bool,
	permanentListenerList: Vec<EventListener>,
}

thread_local! {
	// 웹 글 입력 상태. (applyFrame 과 캔버스 핸들러가 쓴다. DOM 의 focus / blur 를 부르는 동안에는 빌리지 않는다)
	static WEB_TEXT_INPUT: RefCell<WebTextInputState> = const { RefCell::new(WebTextInputState {
		canvas: None,
		entryList: Vec::new(),
		fieldStateList: Vec::new(),
		isCanvasKeyboardFocused: false,
		permanentListenerList: Vec::new(),
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
fn pushPendingEvent(targetFieldId: u64, kind: TextInputEventKind) {
	PENDING_EVENT_LIST.with(|pendingEventCell| {
		if let Ok(mut pendingEventList) = pendingEventCell.try_borrow_mut() {
			pendingEventList.push(TextInputEvent {
				targetFieldId,
				kind,
			});
		}
	});
}

//==============================================================================
// 자바스크립트 오류를 글로.
//==============================================================================
fn describeJsError(error: JsValue) -> String {
	format!("{:?}", error)
}

//==============================================================================
// 문서.
//==============================================================================
fn getDocument() -> Option<web_sys::Document> {
	web_sys::window().and_then(|window| window.document())
}

//==============================================================================
// 요소가 지금 포커스를 가졌는지.
//==============================================================================
fn isActiveElement(element: &web_sys::Element) -> bool {
	match getDocument().and_then(|document| document.active_element()) {
		Some(activeElement) => {
			&activeElement == element
		}
		None => {
			false
		}
	}
}

//==============================================================================
// <input> 의 선택. (문자 자리, 앞 뒤)
//==============================================================================
fn readInputSelection(inputElement: &web_sys::HtmlInputElement) -> Option<(usize, usize)> {
	let value = inputElement.value();
	let selectionStart = inputElement.selection_start().ok().flatten()?;
	let selectionEnd = inputElement.selection_end().ok().flatten()?;
	Some((convertUtf16IndexToCharacterIndex(&value, selectionStart as usize), convertUtf16IndexToCharacterIndex(&value, selectionEnd as usize)))
}

//==============================================================================
// <input> 의 값 바뀜 이벤트 만들기. (원본 handleDOMInputEvent 에 넘기는 값과 커서)
//==============================================================================
fn buildReplaceValueEvent(inputElement: &web_sys::HtmlInputElement) -> TextInputEventKind {
	let value = inputElement.value();
	let characterCount = value.chars().count();
	let (selectionStart, selectionEnd) = readInputSelection(inputElement).unwrap_or((characterCount, characterCount));
	TextInputEventKind::replaceValue {
		value,
		selectionStart,
		selectionEnd,
	}
}

//==============================================================================
// 이 페이지에 사용자 활성화가 한 번이라도 있었는지. (navigator.userActivation 이 없으면 있었던 것으로 본다)
//==============================================================================
fn hasStickyUserActivation() -> bool {
	let navigator = match web_sys::window() {
		Some(window) => {
			window.navigator()
		}
		None => {
			return true;
		}
	};
	let userActivation = js_sys::Reflect::get(&navigator, &JsValue::from_str("userActivation")).unwrap_or(JsValue::UNDEFINED);
	if userActivation.is_undefined() || userActivation.is_null() {
		return true;
	}
	js_sys::Reflect::get(&userActivation, &JsValue::from_str("hasBeenActive")).ok().and_then(|value| value.as_bool()).unwrap_or(true)
}

//==============================================================================
// 0 ms 뒤에 한 번 부르기. (원본 setTimeout(..., 0))
//==============================================================================
fn runAfterTimeout(action: impl FnOnce() + 'static) {
	let callback = Closure::once_into_js(action);
	if let Some(window) = web_sys::window() {
		let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0);
	}
}

//==============================================================================
// navigator.clipboard. (없으면 None)
//==============================================================================
fn getNavigatorClipboard() -> Option<JsValue> {
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
fn callClipboardFunction(functionName: &str, argumentList: &js_sys::Array) -> Option<js_sys::Promise> {
	let clipboard = getNavigatorClipboard()?;
	let clipboardFunction = js_sys::Reflect::get(&clipboard, &JsValue::from_str(functionName)).ok()?.dyn_into::<js_sys::Function>().ok()?;
	let result = js_sys::Reflect::apply(&clipboardFunction, &clipboard, argumentList).ok()?;
	result.dyn_into::<js_sys::Promise>().ok()
}

//==============================================================================
// 시스템 클립보드에 글 쓰기. (navigator.clipboard.writeText, 실패는 무시. 원본 writeClipboard)
//==============================================================================
pub fn writeClipboardText(text: &str) {
	let argumentList = js_sys::Array::of1(&JsValue::from_str(text));
	if let Some(promise) = callClipboardFunction("writeText", &argumentList) {
		wasm_bindgen_futures::spawn_local(async move {
			let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
		});
	}
}

//==============================================================================
// 시스템 클립보드 글 읽기. (웹은 비동기라 그 자리에서 읽지 못한다. 붙여넣기는 requestClipboardPaste 가 한다)
//==============================================================================
pub fn readClipboardText() -> Option<String> {
	None
}

//==============================================================================
// 클립보드를 비동기로 읽어 그 필드에 붙여 넣을 글로 넣기. (원본 paste 의 readClipboard().then)
//==============================================================================
fn requestClipboardPaste(targetFieldId: u64) {
	let promise = match callClipboardFunction("readText", &js_sys::Array::new()) {
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
					pushPendingEvent(targetFieldId, TextInputEventKind::paste(text));
				}
			}
		}
	});
}

//==============================================================================
// 리스너 달기. (시작할 때. 캔버스 누름 / 문맥 메뉴, window keydown, 전역 스타일)
//==============================================================================
pub(super) fn addTextInputListeners(canvas: &web_sys::HtmlCanvasElement) -> Result<(), String> {
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	ensureGlobalHiddenInputStyle()?;

	// 캔버스 누름: 사용자 제스처 안에서 숨긴 <input> 에 focus / blur. (원본 attachCanvasFocusHandler)
	let canvasFocusListener: EventListener = Closure::new(handleCanvasPointer);
	let passiveOptions = web_sys::AddEventListenerOptions::new();
	passiveOptions.set_passive(true);
	for eventName in ["touchstart", "touchend", "mousedown"] {
		canvas.add_event_listener_with_callback_and_add_event_listener_options(eventName, canvasFocusListener.as_ref().unchecked_ref(), &passiveOptions).map_err(describeJsError)?;
	}

	// 오른쪽 단추 문맥 메뉴. (원본 attachContextMenuHandler)
	let contextMenuListener: EventListener = Closure::new(handleCanvasContextMenu);
	canvas.add_event_listener_with_callback("contextmenu", contextMenuListener.as_ref().unchecked_ref()).map_err(describeJsError)?;

	// 캔버스 모드 키. (원본 attachCanvasKeyboard)
	let canvasKeyboardListener: EventListener = Closure::new(handleCanvasKeyDown);
	window.add_event_listener_with_callback("keydown", canvasKeyboardListener.as_ref().unchecked_ref()).map_err(describeJsError)?;

	WEB_TEXT_INPUT.with(|stateCell| {
		let mut webTextInputState = stateCell.borrow_mut();
		webTextInputState.canvas = Some(canvas.clone());
		webTextInputState.permanentListenerList.push(canvasFocusListener);
		webTextInputState.permanentListenerList.push(contextMenuListener);
		webTextInputState.permanentListenerList.push(canvasKeyboardListener);
	});
	Ok(())
}

//==============================================================================
// 전역 스타일 한 번 넣기. (원본 ensureGlobalHiddenInputStyle)
//==============================================================================
fn ensureGlobalHiddenInputStyle() -> Result<(), String> {
	let document = getDocument().ok_or_else(|| String::from("document 없음"))?;
	if document.get_element_by_id(GLOBAL_STYLE_ID).is_some() {
		return Ok(());
	}
	let styleElement = document.create_element("style").map_err(describeJsError)?;
	styleElement.set_id(GLOBAL_STYLE_ID);
	styleElement.set_text_content(Some(GLOBAL_STYLE_TEXT));
	let head = document.head().ok_or_else(|| String::from("head 없음"))?;
	head.append_child(&styleElement).map_err(describeJsError)?;
	Ok(())
}

//==============================================================================
// 프레임 시작. (대기 줄을 엔진으로 옮기고, 포커스를 가진 <input> 의 선택을 폴링한다. 원본 tick 의 selection 폴링)
//==============================================================================
pub(super) fn beginFrame(engine: &mut Engine) {
	DISPOSED_LISTENER_LIST.with(|disposedCell| {
		if let Ok(mut disposedListenerList) = disposedCell.try_borrow_mut() {
			disposedListenerList.clear();
		}
	});
	let pendingEventList = PENDING_EVENT_LIST.with(|pendingEventCell| std::mem::take(&mut *pendingEventCell.borrow_mut()));
	let textInput = engine.getInputManagerMut().getTextInputMut();
	WEB_TEXT_INPUT.with(|stateCell| {
		let mut webTextInputState = stateCell.borrow_mut();
		for pendingEvent in pendingEventList {
			let isCompositionStart = pendingEvent.kind == TextInputEventKind::compositionStart;
			let isCompositionEnd = pendingEvent.kind == TextInputEventKind::compositionEnd;
			if isCompositionStart || isCompositionEnd {
				if let Some(entry) = webTextInputState.entryList.iter_mut().find(|entry| entry.fieldId == pendingEvent.targetFieldId) {
					entry.isComposing = isCompositionStart;
				}
			}
			textInput.pushEvent(pendingEvent);
		}

		// 키로 옮긴 커서 / 선택. (조합 중이 아닐 때, 지난번과 다르면)
		for entry in webTextInputState.entryList.iter_mut() {
			if entry.isComposing || !isActiveElement(&entry.inputElement) {
				continue;
			}
			let selection = match readInputSelection(&entry.inputElement) {
				Some(selection) => {
					selection
				}
				None => {
					continue;
				}
			};
			if selection == entry.knownSelection {
				continue;
			}
			entry.knownSelection = selection;
			textInput.pushEvent(TextInputEvent {
				targetFieldId: entry.fieldId,
				kind: TextInputEventKind::selection {
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
pub(super) fn applyFrame(engine: &Engine) {
	let fieldStateList = engine.getInputManager().getTextInput().getPublishedFieldStates().to_vec();
	let mut focusActionList: Vec<(web_sys::HtmlInputElement, bool)> = Vec::new();
	let mut removedEntryList: Vec<HiddenInputEntry> = Vec::new();
	WEB_TEXT_INPUT.with(|stateCell| {
		let mut webTextInputState = stateCell.borrow_mut();
		let canvasRect = match &webTextInputState.canvas {
			Some(canvas) => {
				canvas.get_bounding_client_rect()
			}
			None => {
				return;
			}
		};

		// 더는 올라오지 않는 필드의 <input> 은 뺀다.
		let entryList = std::mem::take(&mut webTextInputState.entryList);
		for entry in entryList {
			let isStillRegistered = fieldStateList.iter().any(|fieldState| fieldState.fieldId == entry.fieldId && fieldState.isAttached && fieldState.useDOMInput);
			if isStillRegistered {
				webTextInputState.entryList.push(entry);
			}
			else {
				removedEntryList.push(entry);
			}
		}

		for fieldState in fieldStateList.iter() {
			if !fieldState.isAttached || !fieldState.useDOMInput {
				continue;
			}
			let entryIndex = match webTextInputState.entryList.iter().position(|entry| entry.fieldId == fieldState.fieldId) {
				Some(entryIndex) => {
					entryIndex
				}
				None => {
					match createHiddenInputEntry(fieldState) {
						Ok(entry) => {
							webTextInputState.entryList.push(entry);
							webTextInputState.entryList.len() - 1
						}
						Err(error) => {
							web_sys::console::error_1(&JsValue::from_str(&format!("Engine: 숨긴 입력 만들기 실패: {}", error)));
							continue;
						}
					}
				}
			};
			let entry = &mut webTextInputState.entryList[entryIndex];
			applyFieldStateToEntry(entry, fieldState, &canvasRect, &mut focusActionList);
		}
		webTextInputState.fieldStateList = fieldStateList.clone();
		webTextInputState.isCanvasKeyboardFocused = fieldStateList.iter().any(|fieldState| fieldState.isFocused && fieldState.isAttached && !fieldState.useDOMInput);
	});

	// 빌림을 놓은 뒤에 DOM 을 바꾼다. (focus / blur 이벤트가 그 자리에서 불린다)
	for entry in removedEntryList {
		removeHiddenInputEntry(entry);
	}
	for (inputElement, isFocusRequested) in focusActionList {
		if isFocusRequested {
			let _ = inputElement.focus();
		}
		else if isActiveElement(&inputElement) {
			let _ = inputElement.blur();
		}
	}

	// 문맥 메뉴의 필드가 포커스를 잃었으면 닫는다. (원본 handleBlur 의 hideContextMenu)
	let menuFieldId = CONTEXT_MENU.with(|menuCell| menuCell.borrow().as_ref().map(|contextMenu| contextMenu.fieldId));
	if let Some(menuFieldId) = menuFieldId {
		let isMenuFieldFocused = fieldStateList.iter().any(|fieldState| fieldState.fieldId == menuFieldId && fieldState.isFocused);
		if !isMenuFieldFocused {
			hideContextMenu();
		}
	}
}

//==============================================================================
// 필드 상태를 숨긴 <input> 에 적용. (자리, 누름 받기, 최대 길이, 필드 쪽에서 바꾼 값과 선택, 포커스 바뀜)
//==============================================================================
fn applyFieldStateToEntry(entry: &mut HiddenInputEntry, fieldState: &TextInputFieldState, canvasRect: &web_sys::DomRect, focusActionList: &mut Vec<(web_sys::HtmlInputElement, bool)>) {
	let inputStyle = entry.inputElement.style();
	let fieldRect = fieldState.fieldRect;
	let _ = inputStyle.set_property("left", &format!("{}px", canvasRect.left() + fieldRect.position.x as f64));
	let _ = inputStyle.set_property("top", &format!("{}px", canvasRect.top() + fieldRect.position.y as f64));
	let _ = inputStyle.set_property("width", &format!("{}px", fieldRect.size.x));
	let _ = inputStyle.set_property("height", &format!("{}px", fieldRect.size.y));

	// 직접 누름이면 보이는 동안 누름을 받는다. (원본 layoutDOMInput)
	entry.useDirectTouch.set(fieldState.useDirectTouch);
	let pointerEvents = if fieldState.useDirectTouch && fieldState.isVisible {
		"auto"
	}
	else {
		"none"
	};
	if entry.appliedPointerEvents != pointerEvents {
		entry.appliedPointerEvents = pointerEvents.to_string();
		let _ = inputStyle.set_property("pointer-events", pointerEvents);
		if fieldState.useDirectTouch {
			let _ = inputStyle.set_property("user-select", "text");
			let _ = inputStyle.set_property("-webkit-user-select", "text");
		}
	}

	if entry.appliedMaxLength != fieldState.maxLength {
		entry.appliedMaxLength = fieldState.maxLength;
		if fieldState.maxLength > 0 {
			entry.inputElement.set_max_length(fieldState.maxLength as i32);
		}
		else {
			let _ = entry.inputElement.remove_attribute("maxlength");
		}
	}

	// 필드 쪽에서 바꾼 값과 선택. (조합 중에는 IME 를 깨지 않게 미룬다)
	if !entry.isComposing && entry.appliedRevision != Some(fieldState.valueRevision) {
		entry.appliedRevision = Some(fieldState.valueRevision);
		if entry.inputElement.value() != fieldState.value {
			entry.inputElement.set_value(&fieldState.value);
		}
		let selectionStart = fieldState.selectionStart.min(fieldState.cursorIndex);
		let selectionEnd = fieldState.selectionStart.max(fieldState.cursorIndex);
		let utf16Start = convertCharacterIndexToUtf16Index(&fieldState.value, selectionStart) as u32;
		let utf16End = convertCharacterIndexToUtf16Index(&fieldState.value, selectionEnd) as u32;
		let _ = entry.inputElement.set_selection_range(utf16Start, utf16End);
		entry.knownSelection = (selectionStart, selectionEnd);
	}

	// 포커스 바뀜. (바뀔 때만 한다. DOM 에서 온 포커스는 리스너가 이미 맞춰 둔다)
	if fieldState.isFocused != entry.isFocusApplied {
		entry.isFocusApplied = fieldState.isFocused;
		focusActionList.push((entry.inputElement.clone(), fieldState.isFocused));
	}
}

//==============================================================================
// 숨긴 <input> 만들기. (원본 attachDOMInput)
//==============================================================================
fn createHiddenInputEntry(fieldState: &TextInputFieldState) -> Result<HiddenInputEntry, String> {
	let document = getDocument().ok_or_else(|| String::from("document 없음"))?;
	let inputElement = document.create_element("input").map_err(describeJsError)?.dyn_into::<web_sys::HtmlInputElement>().map_err(|_| String::from("input 변환 실패"))?;
	inputElement.set_type("text");
	inputElement.set_value(&fieldState.value);
	if fieldState.maxLength > 0 {
		inputElement.set_max_length(fieldState.maxLength as i32);
	}
	inputElement.set_autocomplete("off");
	inputElement.set_spellcheck(false);
	inputElement.set_class_name(HIDDEN_INPUT_CLASS_NAME);
	let inputStyle = inputElement.style();
	for (propertyName, propertyValue) in HIDDEN_INPUT_STYLE_LIST {
		let _ = inputStyle.set_property(propertyName, propertyValue);
	}

	let fieldId = fieldState.fieldId;
	let useDirectTouch = Rc::new(Cell::new(fieldState.useDirectTouch));
	let isFocusedWithoutActivation = Rc::new(Cell::new(false));
	let mut listenerList: Vec<(&'static str, EventListener)> = Vec::new();

	// 포커스. (직접 누름이면 브라우저가 누른 자리에 둔 커서를 글 끝으로 옮긴다)
	let inputForFocus = inputElement.clone();
	let useDirectTouchForFocus = useDirectTouch.clone();
	let isFocusedWithoutActivationForFocus = isFocusedWithoutActivation.clone();
	listenerList.push(("focus", Closure::new(move |_event: web_sys::Event| {
		isFocusedWithoutActivationForFocus.set(!hasStickyUserActivation());
		pushPendingEvent(fieldId, TextInputEventKind::focusGained);
		if useDirectTouchForFocus.get() {
			let inputForCaret = inputForFocus.clone();
			runAfterTimeout(move || {
				let length = inputForCaret.value().encode_utf16().count() as u32;
				let _ = inputForCaret.set_selection_range(length, length);
			});
		}
	})));
	listenerList.push(("blur", Closure::new(move |_event: web_sys::Event| {
		pushPendingEvent(fieldId, TextInputEventKind::focusLost);
	})));
	let inputForInput = inputElement.clone();
	listenerList.push(("input", Closure::new(move |_event: web_sys::Event| {
		pushPendingEvent(fieldId, buildReplaceValueEvent(&inputForInput));
	})));
	listenerList.push(("compositionstart", Closure::new(move |_event: web_sys::Event| {
		pushPendingEvent(fieldId, TextInputEventKind::compositionStart);
	})));
	listenerList.push(("compositionupdate", Closure::new(move |event: web_sys::Event| {
		let compositionText = event.dyn_ref::<web_sys::CompositionEvent>().and_then(|compositionEvent| compositionEvent.data()).unwrap_or_default();
		pushPendingEvent(fieldId, TextInputEventKind::preedit {
			text: compositionText,
			cursorIndex: None,
		});
	})));
	let inputForCompositionEnd = inputElement.clone();
	listenerList.push(("compositionend", Closure::new(move |_event: web_sys::Event| {
		pushPendingEvent(fieldId, TextInputEventKind::compositionEnd);
		pushPendingEvent(fieldId, buildReplaceValueEvent(&inputForCompositionEnd));
	})));
	listenerList.push(("keydown", Closure::new(move |event: web_sys::Event| {
		let isEnter = event.dyn_ref::<web_sys::KeyboardEvent>().map(|keyboardEvent| keyboardEvent.key() == "Enter").unwrap_or(false);
		if isEnter {
			event.prevent_default();
			pushPendingEvent(fieldId, TextInputEventKind::key {
				key: TextInputKey::enter,
				isShift: false,
			});
		}
	})));
	for (eventName, listener) in listenerList.iter() {
		inputElement.add_event_listener_with_callback(eventName, listener.as_ref().unchecked_ref()).map_err(describeJsError)?;
	}
	let body = document.body().ok_or_else(|| String::from("body 없음"))?;
	body.append_child(&inputElement).map_err(describeJsError)?;
	let characterCount = fieldState.value.chars().count();
	Ok(HiddenInputEntry {
		fieldId,
		inputElement,
		listenerList,
		appliedRevision: None,
		appliedMaxLength: fieldState.maxLength,
		appliedPointerEvents: String::from("none"),
		isFocusApplied: false,
		knownSelection: (characterCount, characterCount),
		isComposing: false,
		useDirectTouch,
		isFocusedWithoutActivation,
	})
}

//==============================================================================
// 숨긴 <input> 빼기. (리스너를 먼저 떼서 빼는 동안의 blur 가 오지 않게 한다. 원본 detachDOMInput)
//==============================================================================
fn removeHiddenInputEntry(entry: HiddenInputEntry) {
	for (eventName, listener) in entry.listenerList.iter() {
		let _ = entry.inputElement.remove_event_listener_with_callback(eventName, listener.as_ref().unchecked_ref());
	}
	entry.inputElement.remove();
	let listenerList: Vec<EventListener> = entry.listenerList.into_iter().map(|(_, listener)| listener).collect();
	disposeListeners(listenerList);
}

//==============================================================================
// 다 쓴 리스너 버리기. (다음 프레임 시작에 버린다)
//==============================================================================
fn disposeListeners(listenerList: Vec<EventListener>) {
	DISPOSED_LISTENER_LIST.with(|disposedCell| {
		if let Ok(mut disposedListenerList) = disposedCell.try_borrow_mut() {
			disposedListenerList.extend(listenerList);
		}
	});
}

//==============================================================================
// 포인터 이벤트의 화면 좌표. (터치면 바뀐 첫 손가락)
//==============================================================================
fn readClientPosition(event: &web_sys::Event) -> Option<(f64, f64)> {
	if let Some(touchEvent) = event.dyn_ref::<web_sys::TouchEvent>() {
		let touch = touchEvent.changed_touches().get(0)?;
		return Some((touch.client_x() as f64, touch.client_y() as f64));
	}
	let mouseEvent = event.dyn_ref::<web_sys::MouseEvent>()?;
	Some((mouseEvent.client_x() as f64, mouseEvent.client_y() as f64))
}

//==============================================================================
// 화면 좌표 아래 맨 위의 보이는 필드 상태. (DOM 입력 필드만 보려면 isDomInputOnly)
//==============================================================================
fn findFieldStateAtClientPosition(webTextInputState: &WebTextInputState, clientX: f64, clientY: f64, isDomInputOnly: bool) -> Option<TextInputFieldState> {
	let canvasRect = webTextInputState.canvas.as_ref()?.get_bounding_client_rect();
	let canvasX = (clientX - canvasRect.left()) as f32;
	let canvasY = (clientY - canvasRect.top()) as f32;
	webTextInputState.fieldStateList.iter().rev().find(|fieldState| {
		let fieldRect = fieldState.fieldRect;
		let isInside = canvasX >= fieldRect.position.x && canvasX <= fieldRect.position.x + fieldRect.size.x && canvasY >= fieldRect.position.y && canvasY <= fieldRect.position.y + fieldRect.size.y;
		isInside && fieldState.isVisible && fieldState.isAttached && (!isDomInputOnly || fieldState.useDOMInput)
	}).cloned()
}

//==============================================================================
// 캔버스 누름 처리. (원본 canvasFocusHandler)
// - 필드 안: 그 자리에서 focus. touchend 는 setUseTouchEndFocus 를 켠 필드만, 활성화 없이 얻은 포커스면 blur 뒤 다시 focus.
// - 필드 밖(누를 때만): 포커스를 가진 <input> 을 blur.
//==============================================================================
fn handleCanvasPointer(event: web_sys::Event) {
	let isRelease = event.type_() == "touchend";
	let (clientX, clientY) = match readClientPosition(&event) {
		Some(clientPosition) => {
			clientPosition
		}
		None => {
			return;
		}
	};
	// 빌린 동안 할 일을 정하고, 놓은 뒤에 DOM 을 바꾼다.
	let mut focusTarget: Option<(web_sys::HtmlInputElement, bool)> = None;
	let mut blurTargetList: Vec<web_sys::HtmlInputElement> = Vec::new();
	WEB_TEXT_INPUT.with(|stateCell| {
		let mut webTextInputState = stateCell.borrow_mut();
		let hitFieldState = findFieldStateAtClientPosition(&webTextInputState, clientX, clientY, true);
		match hitFieldState {
			Some(hitFieldState) => {
				if isRelease && !hitFieldState.useTouchEndFocus {
					return;
				}
				if let Some(entry) = webTextInputState.entryList.iter_mut().find(|entry| entry.fieldId == hitFieldState.fieldId) {
					let isRefocusNeeded = isRelease && hitFieldState.isFocused && entry.isFocusedWithoutActivation.get();
					entry.isFocusApplied = true;
					focusTarget = Some((entry.inputElement.clone(), isRefocusNeeded));
				}
			}
			None => {
				if isRelease {
					return;
				}
				for entry in webTextInputState.entryList.iter_mut() {
					if entry.isFocusApplied || isActiveElement(&entry.inputElement) {
						entry.isFocusApplied = false;
						blurTargetList.push(entry.inputElement.clone());
					}
				}
			}
		}
	});
	if let Some((inputElement, isRefocusNeeded)) = focusTarget {
		if isRefocusNeeded {
			let _ = inputElement.blur();
		}
		let _ = inputElement.focus();
	}
	for inputElement in blurTargetList {
		let _ = inputElement.blur();
	}
}

//==============================================================================
// 캔버스 문맥 메뉴 처리. (필드 위면 브라우저 메뉴를 막고 포커스를 준 뒤 DOM 메뉴를 연다. 원본 handleContextMenuEvent)
//==============================================================================
fn handleCanvasContextMenu(event: web_sys::Event) {
	let (clientX, clientY) = match readClientPosition(&event) {
		Some(clientPosition) => {
			clientPosition
		}
		None => {
			return;
		}
	};
	let mut focusElement: Option<web_sys::HtmlInputElement> = None;
	let hitFieldState = WEB_TEXT_INPUT.with(|stateCell| {
		let mut webTextInputState = stateCell.borrow_mut();
		let hitFieldState = findFieldStateAtClientPosition(&webTextInputState, clientX, clientY, false)?;
		if let Some(entry) = webTextInputState.entryList.iter_mut().find(|entry| entry.fieldId == hitFieldState.fieldId) {
			entry.isFocusApplied = true;
			focusElement = Some(entry.inputElement.clone());
		}
		Some(hitFieldState)
	});
	let hitFieldState = match hitFieldState {
		Some(hitFieldState) => {
			hitFieldState
		}
		None => {
			return;
		}
	};
	event.prevent_default();
	event.stop_propagation();
	match focusElement {
		Some(inputElement) => {
			let _ = inputElement.focus();
		}
		None => {
			pushPendingEvent(hitFieldState.fieldId, TextInputEventKind::focusGained);
		}
	}
	let hasSelection = hitFieldState.selectionStart != hitFieldState.cursorIndex;
	if let Err(error) = showContextMenu(hitFieldState.fieldId, hasSelection, clientX, clientY) {
		web_sys::console::error_1(&JsValue::from_str(&format!("Engine: 문맥 메뉴 열기 실패: {}", error)));
	}
}

//==============================================================================
// DOM 문맥 메뉴 열기. (잘라내기 / 복사 / 붙여넣기. 원본 showContextMenu)
//==============================================================================
fn showContextMenu(fieldId: u64, hasSelection: bool, clientX: f64, clientY: f64) -> Result<(), String> {
	hideContextMenu();
	let document = getDocument().ok_or_else(|| String::from("document 없음"))?;
	let menuElement = document.create_element("div").map_err(describeJsError)?.dyn_into::<web_sys::HtmlElement>().map_err(|_| String::from("div 변환 실패"))?;
	let menuStyle = menuElement.style();
	for (propertyName, propertyValue) in CONTEXT_MENU_STYLE_LIST {
		let _ = menuStyle.set_property(propertyName, propertyValue);
	}
	let _ = menuStyle.set_property("left", &format!("{}px", clientX));
	let _ = menuStyle.set_property("top", &format!("{}px", clientY));

	let itemList = [("잘라내기", hasSelection, TextInputKey::cut), ("복사", hasSelection, TextInputKey::copy), ("붙여넣기", true, TextInputKey::paste)];
	let mut listenerList: Vec<EventListener> = Vec::new();
	for (itemText, isEnabled, itemKey) in itemList {
		let itemElement = document.create_element("div").map_err(describeJsError)?.dyn_into::<web_sys::HtmlElement>().map_err(|_| String::from("div 변환 실패"))?;
		itemElement.set_text_content(Some(itemText));
		let itemStyle = itemElement.style();
		let _ = itemStyle.set_property("padding", "8px 16px");
		let _ = itemStyle.set_property("cursor", if isEnabled {
			"pointer"
		}
		else {
			"default"
		});
		let _ = itemStyle.set_property("color", if isEnabled {
			"#222222"
		}
		else {
			"#aaaaaa"
		});
		if isEnabled {
			let itemForEnter = itemElement.clone();
			let mouseEnterListener: EventListener = Closure::new(move |_event: web_sys::Event| {
				let _ = itemForEnter.style().set_property("background", "#eef2ff");
			});
			let itemForLeave = itemElement.clone();
			let mouseLeaveListener: EventListener = Closure::new(move |_event: web_sys::Event| {
				let _ = itemForLeave.style().set_property("background", "transparent");
			});
			let mouseDownListener: EventListener = Closure::new(move |event: web_sys::Event| {
				event.prevent_default();
				event.stop_propagation();
			});
			let clickListener: EventListener = Closure::new(move |event: web_sys::Event| {
				event.prevent_default();
				event.stop_propagation();
				match itemKey {
					TextInputKey::paste => {
						requestClipboardPaste(fieldId);
					}
					_ => {
						pushPendingEvent(fieldId, TextInputEventKind::key {
							key: itemKey,
							isShift: false,
						});
					}
				}
				hideContextMenu();
			});
			for (eventName, listener) in [("mouseenter", &mouseEnterListener), ("mouseleave", &mouseLeaveListener), ("mousedown", &mouseDownListener), ("click", &clickListener)] {
				itemElement.add_event_listener_with_callback(eventName, listener.as_ref().unchecked_ref()).map_err(describeJsError)?;
			}
			listenerList.push(mouseEnterListener);
			listenerList.push(mouseLeaveListener);
			listenerList.push(mouseDownListener);
			listenerList.push(clickListener);
		}
		menuElement.append_child(&itemElement).map_err(describeJsError)?;
	}
	let body = document.body().ok_or_else(|| String::from("body 없음"))?;
	body.append_child(&menuElement).map_err(describeJsError)?;

	let menuSerial = NEXT_MENU_SERIAL.with(|serialCell| {
		let menuSerial = serialCell.get();
		serialCell.set(menuSerial + 1);
		menuSerial
	});
	CONTEXT_MENU.with(|menuCell| {
		*menuCell.borrow_mut() = Some(WebContextMenu {
			menuSerial,
			fieldId,
			menuElement,
			listenerList,
			dismissListener: None,
		});
	});

	// 메뉴 밖 누름으로 닫기. 다음 차례부터. (지금 누름이 닫기로 처리되지 않게)
	runAfterTimeout(move || {
		addContextMenuDismissListener(menuSerial);
	});
	Ok(())
}

//==============================================================================
// 메뉴 밖 누름 리스너 달기. (그 사이 메뉴가 바뀌었으면 달지 않는다)
//==============================================================================
fn addContextMenuDismissListener(menuSerial: u64) {
	let document = match getDocument() {
		Some(document) => {
			document
		}
		None => {
			return;
		}
	};
	let dismissListener: EventListener = Closure::new(move |event: web_sys::Event| {
		let targetNode = event.target().and_then(|target| target.dyn_into::<web_sys::Node>().ok());
		let isInsideMenu = CONTEXT_MENU.with(|menuCell| {
			match menuCell.borrow().as_ref() {
				Some(contextMenu) => {
					contextMenu.menuElement.contains(targetNode.as_ref())
				}
				None => {
					true
				}
			}
		});
		if !isInsideMenu {
			hideContextMenu();
		}
	});
	CONTEXT_MENU.with(|menuCell| {
		let mut contextMenuSlot = menuCell.borrow_mut();
		if let Some(contextMenu) = contextMenuSlot.as_mut() {
			if contextMenu.menuSerial != menuSerial {
				return;
			}
			if document.add_event_listener_with_callback_and_bool("pointerdown", dismissListener.as_ref().unchecked_ref(), true).is_ok() {
				contextMenu.dismissListener = Some(dismissListener);
			}
		}
	});
}

//==============================================================================
// DOM 문맥 메뉴 닫기. (원본 hideContextMenu. 리스너는 다음 프레임에 버린다)
//==============================================================================
fn hideContextMenu() {
	let contextMenu = CONTEXT_MENU.with(|menuCell| menuCell.borrow_mut().take());
	let contextMenu = match contextMenu {
		Some(contextMenu) => {
			contextMenu
		}
		None => {
			return;
		}
	};
	contextMenu.menuElement.remove();
	let mut listenerList = contextMenu.listenerList;
	if let Some(dismissListener) = contextMenu.dismissListener {
		if let Some(document) = getDocument() {
			let _ = document.remove_event_listener_with_callback_and_bool("pointerdown", dismissListener.as_ref().unchecked_ref(), true);
		}
		listenerList.push(dismissListener);
	}
	disposeListeners(listenerList);
}

//==============================================================================
// 캔버스 모드 키 처리. (useDOMInput 이 꺼진 필드가 포커스를 가졌을 때만. 원본 handleCanvasKeydown)
//==============================================================================
fn handleCanvasKeyDown(event: web_sys::Event) {
	let keyboardEvent = match event.dyn_ref::<web_sys::KeyboardEvent>() {
		Some(keyboardEvent) => {
			keyboardEvent
		}
		None => {
			return;
		}
	};
	let isCanvasKeyboardFocused = WEB_TEXT_INPUT.with(|stateCell| stateCell.try_borrow().map(|webTextInputState| webTextInputState.isCanvasKeyboardFocused).unwrap_or(false));
	if !isCanvasKeyboardFocused {
		return;
	}
	let isHiddenInputActive = getDocument().and_then(|document| document.active_element()).map(|activeElement| activeElement.class_name() == HIDDEN_INPUT_CLASS_NAME).unwrap_or(false);
	if isHiddenInputActive {
		return;
	}
	let key = keyboardEvent.key();
	let isShift = keyboardEvent.shift_key();
	if keyboardEvent.ctrl_key() || keyboardEvent.meta_key() {
		let shortcutKey = match key.to_lowercase().as_str() {
			"a" => {
				Some(TextInputKey::selectAll)
			}
			"c" => {
				Some(TextInputKey::copy)
			}
			"x" => {
				Some(TextInputKey::cut)
			}
			"v" => {
				Some(TextInputKey::paste)
			}
			_ => {
				None
			}
		};
		if let Some(shortcutKey) = shortcutKey {
			event.prevent_default();
			if shortcutKey == TextInputKey::paste {
				requestClipboardPaste(0);
			}
			else {
				pushPendingEvent(0, TextInputEventKind::key {
					key: shortcutKey,
					isShift,
				});
			}
		}
		return;
	}
	let editKey = match key.as_str() {
		"Backspace" => {
			Some(TextInputKey::backspace)
		}
		"Delete" => {
			Some(TextInputKey::delete)
		}
		"ArrowLeft" => {
			Some(TextInputKey::left)
		}
		"ArrowRight" => {
			Some(TextInputKey::right)
		}
		"Home" => {
			Some(TextInputKey::home)
		}
		"End" => {
			Some(TextInputKey::end)
		}
		"Enter" => {
			Some(TextInputKey::enter)
		}
		_ => {
			None
		}
	};
	if let Some(editKey) = editKey {
		event.prevent_default();
		pushPendingEvent(0, TextInputEventKind::key {
			key: editKey,
			isShift,
		});
		return;
	}
	if key.chars().count() == 1 && !keyboardEvent.alt_key() {
		event.prevent_default();
		pushPendingEvent(0, TextInputEventKind::commit(key));
	}
}
