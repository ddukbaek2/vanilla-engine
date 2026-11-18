//==============================================================================
// 글 입력 줄. (InputManager 가 든다)
// - 원본에는 이런 층이 없다. 원본 UIInputField 는 DOM <input> 과 document keydown 에 직접 리스너를 단다.
//   러스트 판은 플랫폼이 창 이벤트(데스크탑 winit 의 KeyboardInput / Ime, 웹 숨긴 <input> 의 input / composition / keydown)를
//   여기 줄에 넣고, 포커스를 가진 입력 필드가 tick 에서 꺼내 쓴다. (ui/uiinputfield.rs)
// - 컴포넌트는 &Engine 만 받는다. 그래서 필드가 쓰는 쪽(꺼내기, 상태 올리기)은 RefCell 로 &self 에서 되게 했다.
//   플랫폼은 &mut Engine 을 쥐므로 넣는 쪽은 &mut self 다.
// - 프레임 흐름:
//   1. 플랫폼이 프레임 사이에 이벤트를 넣는다. (pushEvent)
//   2. 씬 tick 에서 필드가 자기 몫을 꺼낸다. (takeEventsForField) 붙어 있는(attach) 필드와 포커스를 가진 필드는 상태를 올린다. (registerFieldState)
//   3. 프레임 끝에 엔진이 finishFrame 을 부른다. 남은 이벤트를 버리고, 올라온 상태를 "지난 프레임 상태"로 넘긴다.
//   4. 플랫폼이 지난 프레임 상태를 읽어 적용한다. (데스크탑: IME 켜고 끄기와 후보 창 자리, 웹: 숨긴 <input> 자리, 값, 포커스)
// - 글자 자리는 모두 문자(char) 단위다. 원본(자바스크립트 문자열)은 UTF-16 단위다. 한글은 둘이 같고, 이모지 같은 글자에서만 다르다.
//   웹 플랫폼은 DOM 의 UTF-16 자리를 여기 함수로 바꿔 넣는다.
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use std::cell::{Cell, RefCell};

//==============================================================================
// 편집 키. (원본 handleCanvasKeydown 이 보는 키)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputKey {
	backspace,
	delete,
	left,
	right,
	home,
	end,
	enter,
	// 원본 캔버스 모드처럼 필드는 쓰지 않는다. (게임이 다음 칸으로 옮길 때 보라고 넘긴다)
	tab,
	// Ctrl(맥은 Cmd) + A
	selectAll,
	// Ctrl(맥은 Cmd) + C
	copy,
	// Ctrl(맥은 Cmd) + X
	cut,
	// Ctrl(맥은 Cmd) + V
	paste,
}

//==============================================================================
// 글 입력 이벤트 내용.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum TextInputEventKind {
	// 확정된 글. (IME 확정, 키로 친 글자)
	commit(String),
	// 조합 시작. (원본 compositionstart)
	compositionStart,
	// 조합 중인 글. 커서는 조합 글 안의 문자 자리. (원본 compositionupdate, winit Ime::Preedit)
	preedit {
		text: String,
		cursorIndex: Option<usize>,
	},
	// 조합 끝. (원본 compositionend)
	compositionEnd,
	// 편집 키.
	key {
		key: TextInputKey,
		isShift: bool,
	},
	// 붙여 넣을 글. (웹에서 클립보드를 비동기로 읽은 뒤)
	paste(String),
	// 숨긴 <input> 의 값이 바뀜. (웹, 원본 handleDOMInputEvent)
	replaceValue {
		value: String,
		selectionStart: usize,
		selectionEnd: usize,
	},
	// 숨긴 <input> 의 선택이 바뀜. (웹, 원본 tick 의 selection 폴링)
	selection {
		start: usize,
		end: usize,
	},
	// 숨긴 <input> 이 포커스를 얻음. (웹, 원본 input focus 이벤트)
	focusGained,
	// 숨긴 <input> 이 포커스를 잃음. (웹, 원본 input blur 이벤트)
	focusLost,
}

//==============================================================================
// 글 입력 이벤트. (받을 필드 + 내용)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputEvent {
	// 받을 필드 id. 0 이면 지금 포커스를 가진 필드. (데스크탑 키와 IME, 웹 캔버스 모드 키)
	pub targetFieldId: u64,
	pub kind: TextInputEventKind,
}

//==============================================================================
// 필드가 프레임마다 올리는 상태. (플랫폼이 IME 와 숨긴 <input> 을 맞출 때 읽는다)
// - 자리는 창 안쪽 논리 좌표(웹은 캔버스 기준 CSS 픽셀)다.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputFieldState {
	pub fieldId: u64,
	// 포커스를 얻은 차례. (클수록 나중. 포커스가 없으면 0)
	pub focusSerial: u64,
	pub isFocused: bool,
	// attach() 로 입력을 받기로 했는지.
	pub isAttached: bool,
	// 플랫폼 글 입력(IME, 시스템 키보드, 웹 숨긴 <input>)을 쓰는지. (원본 setUseDOMInput)
	pub useDOMInput: bool,
	pub useTouchEndFocus: bool,
	pub useDirectTouch: bool,
	// 노드와 조상이 모두 보이는지.
	pub isVisible: bool,
	pub fieldRect: Rect,
	pub caretRect: Rect,
	pub value: String,
	pub selectionStart: usize,
	pub cursorIndex: usize,
	pub maxLength: usize,
	// 필드 쪽에서 값이나 커서를 바꿀 때마다 는다. (웹이 숨긴 <input> 에 다시 써야 하는지 볼 때)
	pub valueRevision: u64,
}

//==============================================================================
// 글 입력 줄.
//==============================================================================
pub struct TextInput {
	eventList: RefCell<Vec<TextInputEvent>>,
	currentFieldStateList: RefCell<Vec<TextInputFieldState>>,
	publishedFieldStateList: Vec<TextInputFieldState>,
	contextMenuRequestPosition: Option<Vector2>,
	imeCommitGuard: RefCell<Option<(String, u32)>>,
	isImeResetRequested: Cell<bool>,
}

impl TextInput {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TextInput {
		TextInput {
			eventList: RefCell::new(Vec::new()),
			currentFieldStateList: RefCell::new(Vec::new()),
			publishedFieldStateList: Vec::new(),
			contextMenuRequestPosition: None,
			imeCommitGuard: RefCell::new(None),
			isImeResetRequested: Cell::new(false),
		}
	}

	//==============================================================================
	// 이벤트 넣기. (플랫폼)
	//==============================================================================
	pub fn pushEvent(&mut self, event: TextInputEvent) {
		self.eventList.get_mut().push(event);
	}

	//==============================================================================
	// 포커스를 가진 필드에 갈 이벤트 넣기. (플랫폼)
	//==============================================================================
	pub fn pushFocusedFieldEvent(&mut self, kind: TextInputEventKind) {
		self.pushEvent(TextInputEvent {
			targetFieldId: 0,
			kind,
		});
	}

	//==============================================================================
	// 쌓인 이벤트 수.
	//==============================================================================
	pub fn getEventCount(&self) -> usize {
		self.eventList.borrow().len()
	}

	//==============================================================================
	// 필드 몫의 이벤트 꺼내기. (자기 id 로 온 것, 그리고 포커스 이벤트를 받을 수 있으면 0 으로 온 것)
	// - 꺼낸 이벤트는 줄에서 빠진다. 포커스를 가진 필드가 둘이 되는 한 프레임 동안에도 같은 글이 두 번 들어가지 않는다.
	//==============================================================================
	pub fn takeEventsForField(&self, fieldId: u64, acceptsFocusedFieldEvents: bool) -> Vec<TextInputEventKind> {
		let mut eventList = self.eventList.borrow_mut();
		if eventList.is_empty() {
			return Vec::new();
		}
		let mut takenKindList: Vec<TextInputEventKind> = Vec::new();
		let mut remainEventList: Vec<TextInputEvent> = Vec::new();
		for event in eventList.drain(..) {
			let isMine = event.targetFieldId == fieldId || (event.targetFieldId == 0 && acceptsFocusedFieldEvents);
			if isMine {
				takenKindList.push(event.kind);
			}
			else {
				remainEventList.push(event);
			}
		}
		*eventList = remainEventList;
		takenKindList
	}

	//==============================================================================
	// 필드 상태 올리기. (필드 tick, 같은 필드가 두 번 올리면 뒤의 것)
	//==============================================================================
	pub fn registerFieldState(&self, fieldState: TextInputFieldState) {
		let mut currentFieldStateList = self.currentFieldStateList.borrow_mut();
		currentFieldStateList.retain(|candidate| candidate.fieldId != fieldState.fieldId);
		currentFieldStateList.push(fieldState);
	}

	//==============================================================================
	// 지난 프레임에 올라온 필드 상태 목록. (올라온 순서 = 트리 순서)
	//==============================================================================
	pub fn getPublishedFieldStates(&self) -> &[TextInputFieldState] {
		&self.publishedFieldStateList
	}

	//==============================================================================
	// 지난 프레임에 가장 나중에 포커스를 얻은 필드의 포커스 차례. (없으면 0)
	//==============================================================================
	pub fn getLatestFocusSerial(&self) -> u64 {
		let mut latestFocusSerial = 0;
		for fieldState in self.publishedFieldStateList.iter() {
			if fieldState.isFocused && fieldState.focusSerial > latestFocusSerial {
				latestFocusSerial = fieldState.focusSerial;
			}
		}
		latestFocusSerial
	}

	//==============================================================================
	// 글 입력을 받는 필드 상태. (지난 프레임에 포커스를 가진, 붙어 있는 필드 가운데 가장 나중 것)
	//==============================================================================
	pub fn getActiveFieldState(&self) -> Option<&TextInputFieldState> {
		let mut activeFieldState: Option<&TextInputFieldState> = None;
		for fieldState in self.publishedFieldStateList.iter() {
			if !fieldState.isFocused || !fieldState.isAttached {
				continue;
			}
			let isLater = match activeFieldState {
				Some(currentFieldState) => {
					fieldState.focusSerial > currentFieldState.focusSerial
				}
				None => {
					true
				}
			};
			if isLater {
				activeFieldState = Some(fieldState);
			}
		}
		activeFieldState
	}

	//==============================================================================
	// 문맥 메뉴 요청 자리 설정. (데스크탑 오른쪽 단추, 뷰 좌표. 이번 프레임에만 남는다)
	//==============================================================================
	pub fn setContextMenuRequestPosition(&mut self, viewPosition: Option<Vector2>) {
		self.contextMenuRequestPosition = viewPosition;
	}

	//==============================================================================
	// 문맥 메뉴 요청 자리.
	//==============================================================================
	pub fn getContextMenuRequestPosition(&self) -> Option<Vector2> {
		self.contextMenuRequestPosition
	}

	//==============================================================================
	// 포커스를 잃으며 필드가 스스로 확정한 조합 글 기록. (필드)
	// - 데스크탑에서 IME 를 끄면 OS 가 같은 조합 글을 Commit 으로 한 번 더 보낼 수 있다. 플랫폼이 그것을 버리게 한다.
	//==============================================================================
	pub fn setImeCommitGuard(&self, committedText: &str) {
		*self.imeCommitGuard.borrow_mut() = Some((committedText.to_string(), 2));
	}

	//==============================================================================
	// IME 확정 글이 필드가 이미 확정한 글이면 true 를 돌려주고 기록을 지운다. (플랫폼)
	//==============================================================================
	pub fn consumeImeCommitGuard(&mut self, committedText: &str) -> bool {
		let imeCommitGuard = self.imeCommitGuard.get_mut();
		let isGuarded = match imeCommitGuard {
			Some((guardText, _)) => {
				guardText == committedText
			}
			None => {
				false
			}
		};
		if isGuarded {
			*imeCommitGuard = None;
		}
		isGuarded
	}

	//==============================================================================
	// IME 조합 끊기 부탁. (필드가 조합 글을 스스로 확정했을 때. 데스크탑 플랫폼이 IME 를 껐다 켠다)
	//==============================================================================
	pub fn requestImeReset(&self) {
		self.isImeResetRequested.set(true);
	}

	//==============================================================================
	// IME 조합 끊기 부탁 꺼내기. (플랫폼, 꺼내면 지워진다)
	//==============================================================================
	pub fn takeImeResetRequest(&self) -> bool {
		self.isImeResetRequested.replace(false)
	}

	//==============================================================================
	// 프레임 마무리. (엔진이 프레임 끝에 부른다)
	// - 아무도 꺼내지 않은 이벤트를 버리고, 이번 프레임에 올라온 상태를 지난 프레임 상태로 넘긴다.
	//==============================================================================
	pub fn finishFrame(&mut self) {
		self.eventList.get_mut().clear();
		self.publishedFieldStateList = std::mem::take(self.currentFieldStateList.get_mut());
		self.contextMenuRequestPosition = None;
		let imeCommitGuard = self.imeCommitGuard.get_mut();
		let isExpired = match imeCommitGuard {
			Some((_, remainFrameCount)) => {
				*remainFrameCount = remainFrameCount.saturating_sub(1);
				*remainFrameCount == 0
			}
			None => {
				false
			}
		};
		if isExpired {
			*imeCommitGuard = None;
		}
	}
}

impl Default for TextInput {
	fn default() -> TextInput {
		TextInput::new()
	}
}

//==============================================================================
// 문자 수.
//==============================================================================
pub fn countCharacters(text: &str) -> usize {
	text.chars().count()
}

//==============================================================================
// 문자 자리 → 바이트 자리. (넘으면 끝)
//==============================================================================
pub fn convertCharacterIndexToByteIndex(text: &str, characterIndex: usize) -> usize {
	match text.char_indices().nth(characterIndex) {
		Some((byteIndex, _)) => {
			byteIndex
		}
		None => {
			text.len()
		}
	}
}

//==============================================================================
// 바이트 자리 → 문자 자리. (winit Ime::Preedit 의 커서가 바이트 단위다)
//==============================================================================
pub fn convertByteIndexToCharacterIndex(text: &str, byteIndex: usize) -> usize {
	text.char_indices().take_while(|(characterByteIndex, _)| *characterByteIndex < byteIndex).count()
}

//==============================================================================
// UTF-16 자리 → 문자 자리. (DOM selectionStart / selectionEnd)
//==============================================================================
pub fn convertUtf16IndexToCharacterIndex(text: &str, utf16Index: usize) -> usize {
	let mut utf16Count = 0;
	let mut characterIndex = 0;
	for character in text.chars() {
		if utf16Count >= utf16Index {
			break;
		}
		utf16Count += character.len_utf16();
		characterIndex += 1;
	}
	characterIndex
}

//==============================================================================
// 문자 자리 → UTF-16 자리. (DOM setSelectionRange)
//==============================================================================
pub fn convertCharacterIndexToUtf16Index(text: &str, characterIndex: usize) -> usize {
	text.chars().take(characterIndex).map(|character| character.len_utf16()).sum()
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 자리 바꾸기. (한글과 이모지가 섞인 글)
	//==============================================================================
	#[test]
	fn indexConversion() {
		let text = "a한😀b";
		assert_eq!(countCharacters(text), 4);
		assert_eq!(convertCharacterIndexToByteIndex(text, 2), 4);
		assert_eq!(convertCharacterIndexToByteIndex(text, 9), text.len());
		assert_eq!(convertByteIndexToCharacterIndex(text, 4), 2);
		assert_eq!(convertByteIndexToCharacterIndex(text, text.len()), 4);
		assert_eq!(convertCharacterIndexToUtf16Index(text, 3), 4);
		assert_eq!(convertUtf16IndexToCharacterIndex(text, 4), 3);
		assert_eq!(convertUtf16IndexToCharacterIndex(text, 99), 4);
	}

	//==============================================================================
	// 필드 몫만 꺼내고, 남은 것은 프레임 끝에 버린다. 상태는 프레임 끝에 넘어간다.
	//==============================================================================
	#[test]
	fn takeEventsAndPublish() {
		let mut textInput = TextInput::new();
		textInput.pushFocusedFieldEvent(TextInputEventKind::commit(String::from("a")));
		textInput.pushEvent(TextInputEvent {
			targetFieldId: 7,
			kind: TextInputEventKind::focusGained,
		});
		assert!(textInput.takeEventsForField(3, false).is_empty());
		assert_eq!(textInput.takeEventsForField(7, false), vec![TextInputEventKind::focusGained]);
		assert_eq!(textInput.takeEventsForField(3, true), vec![TextInputEventKind::commit(String::from("a"))]);
		assert_eq!(textInput.getEventCount(), 0);

		textInput.pushFocusedFieldEvent(TextInputEventKind::compositionStart);
		textInput.registerFieldState(TextInputFieldState {
			fieldId: 3,
			focusSerial: 5,
			isFocused: true,
			isAttached: true,
			useDOMInput: true,
			useTouchEndFocus: false,
			useDirectTouch: false,
			isVisible: true,
			fieldRect: Rect::zero(),
			caretRect: Rect::zero(),
			value: String::new(),
			selectionStart: 0,
			cursorIndex: 0,
			maxLength: 0,
			valueRevision: 0,
		});
		assert!(textInput.getActiveFieldState().is_none());
		textInput.finishFrame();
		assert_eq!(textInput.getEventCount(), 0);
		assert_eq!(textInput.getActiveFieldState().map(|fieldState| fieldState.fieldId), Some(3));
		assert_eq!(textInput.getLatestFocusSerial(), 5);
	}

	//==============================================================================
	// 확정 글 가드는 같은 글 한 번만 막고, 두 프레임 뒤에는 사라진다.
	//==============================================================================
	#[test]
	fn imeCommitGuard() {
		let mut textInput = TextInput::new();
		textInput.setImeCommitGuard("한");
		assert!(!textInput.consumeImeCommitGuard("가"));
		assert!(textInput.consumeImeCommitGuard("한"));
		assert!(!textInput.consumeImeCommitGuard("한"));
		textInput.setImeCommitGuard("글");
		textInput.finishFrame();
		textInput.finishFrame();
		assert!(!textInput.consumeImeCommitGuard("글"));
	}
}
