//==============================================================================
//! 글 입력 줄. (InputManager 가 든다)
//! - 원본에는 이런 층이 없다. 원본 UIInputField 는 DOM <input> 과 document keydown 에 직접 리스너를 단다.
//!   러스트 판은 플랫폼이 창 이벤트(데스크탑 winit 의 KeyboardInput / Ime, 웹 숨긴 <input> 의 input / composition / keydown)를
//!   여기 줄에 넣고, 포커스를 가진 입력 필드가 tick 에서 꺼내 쓴다. (ui/uiinputfield.rs)
//! - 컴포넌트는 &Engine 만 받는다. 그래서 필드가 쓰는 쪽(꺼내기, 상태 올리기)은 RefCell 로 &self 에서 되게 했다.
//!   플랫폼은 &mut Engine 을 쥐므로 넣는 쪽은 &mut self 다.
//! - 프레임 흐름:
//!   1. 플랫폼이 프레임 사이에 이벤트를 넣는다. (push_event)
//!   2. 씬 tick 에서 필드가 자기 몫을 꺼낸다. (take_events_for_field) 붙어 있는(attach) 필드와 포커스를 가진 필드는 상태를 올린다. (register_field_state)
//!   3. 프레임 끝에 엔진이 finish_frame 을 부른다. 남은 이벤트를 버리고, 올라온 상태를 "지난 프레임 상태"로 넘긴다.
//!   4. 플랫폼이 지난 프레임 상태를 읽어 적용한다. (데스크탑: IME 켜고 끄기와 후보 창 자리, 웹: 숨긴 <input> 자리, 값, 포커스)
//! - 글자 자리는 모두 문자(char) 단위다. 원본(자바스크립트 문자열)은 UTF-16 단위다. 한글은 둘이 같고, 이모지 같은 글자에서만 다르다.
//!   웹 플랫폼은 DOM 의 UTF-16 자리를 여기 함수로 바꿔 넣는다.
//!
//! # 예제
//! 문자 자리와 바이트 자리, UTF-16 자리 바꾸기
//! ```rust
//! use vanilla::core::textinput::{convert_byte_index_to_character_index, convert_character_index_to_byte_index, convert_character_index_to_utf16_index, convert_utf16_index_to_character_index, count_characters};
//!
//! let text = "a\u{D55C}\u{1F600}b";
//! assert_eq!(count_characters(text), 4);
//! assert_eq!(convert_character_index_to_byte_index(text, 2), 4);
//! assert_eq!(convert_byte_index_to_character_index(text, 4), 2);
//! assert_eq!(convert_character_index_to_utf16_index(text, 3), 4);
//! assert_eq!(convert_utf16_index_to_character_index(text, 4), 3);
//! ```
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use std::cell::{Cell, RefCell};

//==============================================================================
/// 편집 키. (원본 handleCanvasKeydown 이 보는 키)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputKey {
	/// 커서 앞 글자 지우기. (선택이 있으면 선택을 지운다)
	Backspace,
	/// 커서 뒤 글자 지우기. (선택이 있으면 선택을 지운다)
	Delete,
	/// 커서를 한 글자 왼쪽으로. (is_shift 면 선택을 늘린다)
	Left,
	/// 커서를 한 글자 오른쪽으로. (is_shift 면 선택을 늘린다)
	Right,
	/// 줄 앞. (여러 줄 필드는 보이는 줄의 앞, 한 줄 필드는 글 앞. 맥은 Cmd + 왼쪽도, is_shift 면 선택을 늘린다)
	Home,
	/// 줄 끝. (여러 줄 필드는 보이는 줄의 끝, 한 줄 필드는 글 끝. 맥은 Cmd + 오른쪽도, is_shift 면 선택을 늘린다)
	End,
	/// 위 줄. (여러 줄 필드, 한 줄 DOM 입력 칸은 글 앞)
	Up,
	/// 아래 줄. (여러 줄 필드, 한 줄 DOM 입력 칸은 글 끝)
	Down,
	/// 한 화면만큼 위. (여러 줄 필드, 맥은 Option 과 함께)
	PageUp,
	/// 한 화면만큼 아래. (여러 줄 필드, 맥은 Option 과 함께)
	PageDown,
	/// 글 앞. (Ctrl + Home, 맥은 Cmd + 위)
	DocumentStart,
	/// 글 끝. (Ctrl + End, 맥은 Cmd + 아래)
	DocumentEnd,
	/// 엔터. (한 줄 필드는 제출 이벤트를 내고, 여러 줄 필드는 줄을 넣는다)
	Enter,
	/// 원본 캔버스 모드처럼 필드는 쓰지 않는다. (게임이 다음 칸으로 옮길 때 보라고 넘긴다)
	Tab,
	/// Ctrl(맥은 Cmd) + A
	SelectAll,
	/// Ctrl(맥은 Cmd) + C
	Copy,
	/// Ctrl(맥은 Cmd) + X
	Cut,
	/// Ctrl(맥은 Cmd) + V
	Paste,
	/// 되돌리기. (Ctrl(맥은 Cmd) + Z, 윈도우는 Alt + Backspace 도. 원본 DOM 입력 칸의 브라우저 실행 취소)
	Undo,
	/// 다시 하기. (Ctrl(맥은 Cmd) + Shift + Z, 윈도우와 리눅스는 Ctrl + Y, 윈도우는 Alt + Shift + Backspace 도)
	Redo,
}

//==============================================================================
/// 글 입력 이벤트 내용.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum TextInputEventKind {
	/// 확정된 글. (IME 확정, 키로 친 글자)
	Commit(String),
	/// 조합 시작. (원본 compositionstart)
	CompositionStart,
	/// 조합 중인 글. 커서는 조합 글 안의 문자 자리. (원본 compositionupdate, winit Ime::Preedit)
	Preedit {
		text: String,
		cursor_index: Option<usize>,
	},
	/// 조합 끝. (원본 compositionend)
	CompositionEnd,
	/// 편집 키.
	Key {
		key: TextInputKey,
		is_shift: bool,
	},
	/// 붙여 넣을 글. (웹에서 클립보드를 비동기로 읽은 뒤)
	Paste(String),
	/// 숨긴 <input> 의 값이 바뀜. (웹, 원본 handleDOMInputEvent)
	ReplaceValue {
		value: String,
		selection_start: usize,
		selection_end: usize,
	},
	/// 숨긴 <input> 의 선택이 바뀜. (웹, 원본 tick 의 selection 폴링)
	Selection {
		start: usize,
		end: usize,
	},
	/// 숨긴 <input> 이 포커스를 얻음. (웹, 원본 input focus 이벤트)
	FocusGained,
	/// 숨긴 <input> 이 포커스를 잃음. (웹, 원본 input blur 이벤트)
	FocusLost,
}

//==============================================================================
/// 글 입력 이벤트. (받을 필드 + 내용)
///
/// # 예제
/// 포커스를 가진 필드에 확정 글 보내기
/// ```rust
/// use vanilla::{TextInput, TextInputEvent, TextInputEventKind};
///
/// let mut text_input = TextInput::new();
/// text_input.push_event(TextInputEvent {
/// 	target_field_id: 0,
/// 	kind: TextInputEventKind::Commit(String::from("hello")),
/// });
/// assert_eq!(text_input.get_event_count(), 1);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputEvent {
	/// 받을 필드 id. 0 이면 지금 포커스를 가진 필드. (데스크탑 키와 IME, 웹 캔버스 모드 키)
	pub target_field_id: u64,
	/// 이벤트 내용.
	pub kind: TextInputEventKind,
}

//==============================================================================
/// 필드가 프레임마다 올리는 상태. (플랫폼이 IME 와 숨긴 <input> 을 맞출 때 읽는다)
/// - 자리는 창 안쪽 논리 좌표(웹은 캔버스 기준 CSS 픽셀)다.
///
/// # 예제
/// 필드 상태를 올리고 프레임 끝에 넘기기
/// ```rust
/// use vanilla::{Rect, TextInput, TextInputFieldState};
///
/// let mut text_input = TextInput::new();
/// text_input.register_field_state(TextInputFieldState {
/// 	field_id: 3,
/// 	focus_serial: 1,
/// 	is_focused: true,
/// 	is_attached: true,
/// 	use_dominput: true,
/// 	use_touch_end_focus: false,
/// 	use_direct_touch: false,
/// 	is_visible: true,
/// 	field_rect: Rect::create(10.0, 20.0, 200.0, 40.0),
/// 	caret_rect: Rect::create(10.0, 20.0, 1.0, 40.0),
/// 	value: String::from("abc"),
/// 	selection_start: 0,
/// 	cursor_index: 3,
/// 	max_length: 0,
/// 	value_revision: 0,
/// 	is_multiline: false,
/// });
/// text_input.finish_frame();
/// assert_eq!(text_input.get_published_field_states().len(), 1);
/// assert_eq!(text_input.get_published_field_states()[0].value, "abc");
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputFieldState {
	/// 필드 id. (UIInputField 가 만들 때 1 부터 매긴다)
	pub field_id: u64,
	/// 포커스를 얻은 차례. (클수록 나중. 포커스가 없으면 0)
	pub focus_serial: u64,
	/// 포커스를 가졌는지.
	pub is_focused: bool,
	/// attach() 로 입력을 받기로 했는지.
	pub is_attached: bool,
	/// 플랫폼 글 입력(IME, 시스템 키보드, 웹 숨긴 <input>)을 쓰는지. (원본 set_use_dominput)
	pub use_dominput: bool,
	/// 손을 뗄 때도 포커스를 줄지. (웹 DOM 입력 전용)
	pub use_touch_end_focus: bool,
	/// DOM 입력을 필드 위에서 직접 누르게 할지. (웹 DOM 입력 전용)
	pub use_direct_touch: bool,
	/// 노드와 조상이 모두 보이는지.
	pub is_visible: bool,
	/// 필드 사각형.
	pub field_rect: Rect,
	/// 커서 사각형. (필드 높이만큼의 세로 막대)
	pub caret_rect: Rect,
	/// 지금 값.
	pub value: String,
	/// 선택 시작 자리. (문자 단위, 선택을 늘릴 때 고정되는 쪽. cursor_index 와 같으면 선택 없음)
	pub selection_start: usize,
	/// 커서 자리. (문자 단위)
	pub cursor_index: usize,
	/// 최대 길이. (UTF-16 단위, 0 이면 제한 없음)
	pub max_length: usize,
	/// 필드 쪽에서 값이나 커서를 바꿀 때마다 는다. (웹이 숨긴 <input> 에 다시 써야 하는지 볼 때)
	pub value_revision: u64,
	/// 여러 줄 필드인지. (웹은 숨긴 `<textarea>` 를 쓴다)
	pub is_multiline: bool,
}

//==============================================================================
/// 글 입력 줄.
/// - InputManager 가 하나 든다. 플랫폼이 이벤트를 넣고, 입력 필드가 꺼내 쓰고, 엔진이 프레임 끝에 finish_frame 을 부른다.
///
/// # 예제
/// 넣고, 필드가 꺼내고, 프레임 끝에 남은 것을 버리기
/// ```rust
/// use vanilla::{TextInput, TextInputEvent, TextInputEventKind};
///
/// let mut text_input = TextInput::new();
/// text_input.push_focused_field_event(TextInputEventKind::Commit(String::from("a")));
/// text_input.push_event(TextInputEvent {
/// 	target_field_id: 7,
/// 	kind: TextInputEventKind::FocusGained,
/// });
/// text_input.push_event(TextInputEvent {
/// 	target_field_id: 9,
/// 	kind: TextInputEventKind::FocusLost,
/// });
/// let taken_list = text_input.take_events_for_field(7, true);
/// assert_eq!(taken_list, vec![TextInputEventKind::Commit(String::from("a")), TextInputEventKind::FocusGained]);
/// assert_eq!(text_input.get_event_count(), 1);
/// text_input.finish_frame();
/// assert_eq!(text_input.get_event_count(), 0);
/// ```
//==============================================================================
pub struct TextInput {
	event_list: RefCell<Vec<TextInputEvent>>,
	current_field_state_list: RefCell<Vec<TextInputFieldState>>,
	published_field_state_list: Vec<TextInputFieldState>,
	context_menu_request_position: Option<Vector2>,
	ime_commit_guard: RefCell<Option<(String, u32)>>,
	is_ime_reset_requested: Cell<bool>,
}

impl TextInput {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 이벤트도 필드 상태도 없는 빈 줄.
	//==============================================================================
	pub fn new() -> TextInput {
		TextInput {
			event_list: RefCell::new(Vec::new()),
			current_field_state_list: RefCell::new(Vec::new()),
			published_field_state_list: Vec::new(),
			context_menu_request_position: None,
			ime_commit_guard: RefCell::new(None),
			is_ime_reset_requested: Cell::new(false),
		}
	}

	//==============================================================================
	/// 이벤트 넣기. (플랫폼)
	///
	/// # 인자
	/// - `event`: 넣을 이벤트. (target_field_id 가 0 이면 포커스를 가진 필드 몫)
	//==============================================================================
	pub fn push_event(&mut self, event: TextInputEvent) {
		self.event_list.get_mut().push(event);
	}

	//==============================================================================
	/// 포커스를 가진 필드에 갈 이벤트 넣기. (플랫폼)
	///
	/// # 인자
	/// - `kind`: 넣을 이벤트 내용. (target_field_id 0 으로 넣는다)
	//==============================================================================
	pub fn push_focused_field_event(&mut self, kind: TextInputEventKind) {
		self.push_event(TextInputEvent {
			target_field_id: 0,
			kind,
		});
	}

	//==============================================================================
	/// 쌓인 이벤트 수.
	///
	/// # 돌려줌
	/// 아직 아무도 꺼내지 않은 이벤트 수.
	//==============================================================================
	pub fn get_event_count(&self) -> usize {
		self.event_list.borrow().len()
	}

	//==============================================================================
	/// 필드 몫의 이벤트 꺼내기. (자기 id 로 온 것, 그리고 포커스 이벤트를 받을 수 있으면 0 으로 온 것)
	/// - 꺼낸 이벤트는 줄에서 빠진다. 포커스를 가진 필드가 둘이 되는 한 프레임 동안에도 같은 글이 두 번 들어가지 않는다.
	///
	/// # 인자
	/// - `field_id`: 꺼낼 필드 id.
	/// - `accepts_focused_field_events`: true 면 target_field_id 가 0 인 이벤트(포커스를 가진 필드 몫)도 꺼낸다.
	///
	/// # 돌려줌
	/// 꺼낸 이벤트 내용 목록. (넣은 차례대로, 없으면 빈 목록)
	///
	/// # 예제
	/// id 로 온 것과 포커스 몫 꺼내기
	/// ```rust
	/// use vanilla::{TextInput, TextInputEvent, TextInputEventKind};
	///
	/// let mut text_input = TextInput::new();
	/// text_input.push_focused_field_event(TextInputEventKind::CompositionStart);
	/// text_input.push_event(TextInputEvent {
	/// 	target_field_id: 5,
	/// 	kind: TextInputEventKind::FocusGained,
	/// });
	/// assert!(text_input.take_events_for_field(3, false).is_empty());
	/// assert_eq!(text_input.take_events_for_field(5, false), vec![TextInputEventKind::FocusGained]);
	/// assert_eq!(text_input.take_events_for_field(3, true), vec![TextInputEventKind::CompositionStart]);
	/// ```
	//==============================================================================
	pub fn take_events_for_field(&self, field_id: u64, accepts_focused_field_events: bool) -> Vec<TextInputEventKind> {
		let mut event_list = self.event_list.borrow_mut();
		if event_list.is_empty() {
			return Vec::new();
		}
		let mut taken_kind_list: Vec<TextInputEventKind> = Vec::new();
		let mut remain_event_list: Vec<TextInputEvent> = Vec::new();
		for event in event_list.drain(..) {
			let is_mine = event.target_field_id == field_id || (event.target_field_id == 0 && accepts_focused_field_events);
			if is_mine {
				taken_kind_list.push(event.kind);
			}
			else {
				remain_event_list.push(event);
			}
		}
		*event_list = remain_event_list;
		taken_kind_list
	}

	//==============================================================================
	/// 필드 상태 올리기. (필드 tick, 같은 필드가 두 번 올리면 뒤의 것)
	///
	/// # 인자
	/// - `field_state`: 올릴 상태. (finish_frame 뒤 get_published_field_states 로 보인다)
	//==============================================================================
	pub fn register_field_state(&self, field_state: TextInputFieldState) {
		let mut current_field_state_list = self.current_field_state_list.borrow_mut();
		current_field_state_list.retain(|candidate| candidate.field_id != field_state.field_id);
		current_field_state_list.push(field_state);
	}

	//==============================================================================
	/// 지난 프레임에 올라온 필드 상태 목록. (올라온 순서 = 트리 순서)
	///
	/// # 돌려줌
	/// 지난 프레임 상태 목록. (첫 finish_frame 전에는 비었다)
	//==============================================================================
	pub fn get_published_field_states(&self) -> &[TextInputFieldState] {
		&self.published_field_state_list
	}

	//==============================================================================
	/// 지난 프레임에 가장 나중에 포커스를 얻은 필드의 포커스 차례. (없으면 0)
	///
	/// # 돌려줌
	/// 포커스를 가진 필드의 focus_serial 가운데 가장 큰 값. (없으면 0)
	//==============================================================================
	pub fn get_latest_focus_serial(&self) -> u64 {
		let mut latest_focus_serial = 0;
		for field_state in self.published_field_state_list.iter() {
			if field_state.is_focused && field_state.focus_serial > latest_focus_serial {
				latest_focus_serial = field_state.focus_serial;
			}
		}
		latest_focus_serial
	}

	//==============================================================================
	/// 글 입력을 받는 필드 상태. (지난 프레임에 포커스를 가진, 붙어 있는 필드 가운데 가장 나중 것)
	///
	/// # 돌려줌
	/// 그 필드 상태. 포커스를 가지고 붙어 있는(is_attached) 필드가 없으면 None.
	///
	/// # 예제
	/// 포커스를 가진 붙은 필드 가운데 나중 것
	/// ```rust
	/// use vanilla::{Rect, TextInput, TextInputFieldState};
	///
	/// fn create_field_state(field_id: u64, focus_serial: u64) -> TextInputFieldState {
	/// 	TextInputFieldState {
	/// 		field_id,
	/// 		focus_serial,
	/// 		is_focused: true,
	/// 		is_attached: true,
	/// 		use_dominput: true,
	/// 		use_touch_end_focus: false,
	/// 		use_direct_touch: false,
	/// 		is_visible: true,
	/// 		field_rect: Rect::zero(),
	/// 		caret_rect: Rect::zero(),
	/// 		value: String::new(),
	/// 		selection_start: 0,
	/// 		cursor_index: 0,
	/// 		max_length: 0,
	/// 		value_revision: 0,
	/// 		is_multiline: false,
	/// 	}
	/// }
	///
	/// let mut text_input = TextInput::new();
	/// text_input.register_field_state(create_field_state(1, 4));
	/// text_input.register_field_state(create_field_state(2, 6));
	/// assert!(text_input.get_active_field_state().is_none());
	/// text_input.finish_frame();
	/// assert_eq!(text_input.get_active_field_state().map(|field_state| field_state.field_id), Some(2));
	/// assert_eq!(text_input.get_latest_focus_serial(), 6);
	/// ```
	//==============================================================================
	pub fn get_active_field_state(&self) -> Option<&TextInputFieldState> {
		let mut active_field_state: Option<&TextInputFieldState> = None;
		for field_state in self.published_field_state_list.iter() {
			if !field_state.is_focused || !field_state.is_attached {
				continue;
			}
			let is_later = match active_field_state {
				Some(current_field_state) => {
					field_state.focus_serial > current_field_state.focus_serial
				}
				None => {
					true
				}
			};
			if is_later {
				active_field_state = Some(field_state);
			}
		}
		active_field_state
	}

	//==============================================================================
	/// 문맥 메뉴 요청 자리 설정. (데스크탑 오른쪽 단추, 뷰 좌표. 이번 프레임에만 남는다)
	///
	/// # 인자
	/// - `view_position`: 요청 자리. (뷰 좌표, None 이면 요청 없음. finish_frame 이 None 으로 돌린다)
	//==============================================================================
	pub fn set_context_menu_request_position(&mut self, view_position: Option<Vector2>) {
		self.context_menu_request_position = view_position;
	}

	//==============================================================================
	/// 문맥 메뉴 요청 자리.
	///
	/// # 돌려줌
	/// 이번 프레임의 요청 자리. (뷰 좌표, 요청이 없으면 None)
	//==============================================================================
	pub fn get_context_menu_request_position(&self) -> Option<Vector2> {
		self.context_menu_request_position
	}

	//==============================================================================
	/// 포커스를 잃으며 필드가 스스로 확정한 조합 글 기록. (필드)
	/// - 데스크탑에서 IME 를 끄면 OS 가 같은 조합 글을 Commit 으로 한 번 더 보낼 수 있다. 플랫폼이 그것을 버리게 한다.
	///
	/// # 인자
	/// - `committed_text`: 필드가 확정한 글. (finish_frame 을 두 번 지나면 기록이 사라진다)
	//==============================================================================
	pub fn set_ime_commit_guard(&self, committed_text: &str) {
		*self.ime_commit_guard.borrow_mut() = Some((committed_text.to_string(), 2));
	}

	//==============================================================================
	/// IME 확정 글이 필드가 이미 확정한 글이면 true 를 돌려주고 기록을 지운다. (플랫폼)
	///
	/// # 인자
	/// - `committed_text`: IME 가 보낸 확정 글.
	///
	/// # 돌려줌
	/// 기록된 글과 같으면 true. (기록이 없거나 다르면 false, 기록은 그대로 둔다)
	///
	/// # 예제
	/// 같은 확정 글을 한 번만 막기
	/// ```rust
	/// use vanilla::TextInput;
	///
	/// let mut text_input = TextInput::new();
	/// text_input.set_ime_commit_guard("abc");
	/// assert!(!text_input.consume_ime_commit_guard("xyz"));
	/// assert!(text_input.consume_ime_commit_guard("abc"));
	/// assert!(!text_input.consume_ime_commit_guard("abc"));
	/// ```
	//==============================================================================
	pub fn consume_ime_commit_guard(&mut self, committed_text: &str) -> bool {
		let ime_commit_guard = self.ime_commit_guard.get_mut();
		let is_guarded = match ime_commit_guard {
			Some((guard_text, _)) => {
				guard_text == committed_text
			}
			None => {
				false
			}
		};
		if is_guarded {
			*ime_commit_guard = None;
		}
		is_guarded
	}

	//==============================================================================
	/// IME 조합 끊기 부탁. (필드가 조합 글을 스스로 확정했을 때. 데스크탑 플랫폼이 IME 를 껐다 켠다)
	//==============================================================================
	pub fn request_ime_reset(&self) {
		self.is_ime_reset_requested.set(true);
	}

	//==============================================================================
	/// IME 조합 끊기 부탁 꺼내기. (플랫폼, 꺼내면 지워진다)
	///
	/// # 돌려줌
	/// request_ime_reset 이 불렸으면 true.
	//==============================================================================
	pub fn take_ime_reset_request(&self) -> bool {
		self.is_ime_reset_requested.replace(false)
	}

	//==============================================================================
	/// 프레임 마무리. (엔진이 프레임 끝에 부른다)
	/// - 아무도 꺼내지 않은 이벤트를 버리고, 이번 프레임에 올라온 상태를 지난 프레임 상태로 넘긴다.
	//==============================================================================
	pub fn finish_frame(&mut self) {
		self.event_list.get_mut().clear();
		self.published_field_state_list = std::mem::take(self.current_field_state_list.get_mut());
		self.context_menu_request_position = None;
		let ime_commit_guard = self.ime_commit_guard.get_mut();
		let is_expired = match ime_commit_guard {
			Some((_, remain_frame_count)) => {
				*remain_frame_count = remain_frame_count.saturating_sub(1);
				*remain_frame_count == 0
			}
			None => {
				false
			}
		};
		if is_expired {
			*ime_commit_guard = None;
		}
	}
}

impl Default for TextInput {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TextInput {
		TextInput::new()
	}
}

//==============================================================================
/// 문자 수.
///
/// # 인자
/// - `text`: 셀 글.
///
/// # 돌려줌
/// char 수. (바이트 수가 아니다)
//==============================================================================
pub fn count_characters(text: &str) -> usize {
	text.chars().count()
}

//==============================================================================
/// 문자 자리 → 바이트 자리. (넘으면 끝)
///
/// # 인자
/// - `text`: 글.
/// - `character_index`: 문자 자리.
///
/// # 돌려줌
/// 그 문자가 시작하는 바이트 자리. (글 문자 수 이상이면 text.len())
//==============================================================================
pub fn convert_character_index_to_byte_index(text: &str, character_index: usize) -> usize {
	match text.char_indices().nth(character_index) {
		Some((byte_index, _)) => {
			byte_index
		}
		None => {
			text.len()
		}
	}
}

//==============================================================================
/// 바이트 자리 → 문자 자리. (winit Ime::Preedit 의 커서가 바이트 단위다)
///
/// # 인자
/// - `text`: 글.
/// - `byte_index`: 바이트 자리.
///
/// # 돌려줌
/// byte_index 앞에서 시작하는 문자 수. (글자 가운데 바이트면 그 글자 뒤 자리)
//==============================================================================
pub fn convert_byte_index_to_character_index(text: &str, byte_index: usize) -> usize {
	text.char_indices().take_while(|(character_byte_index, _)| *character_byte_index < byte_index).count()
}

//==============================================================================
/// UTF-16 자리 → 문자 자리. (DOM selection_start / selection_end)
///
/// # 인자
/// - `text`: 글.
/// - `utf16_index`: UTF-16 자리.
///
/// # 돌려줌
/// 문자 자리. (서로게이트 쌍 가운데면 그 글자 뒤 자리, 넘으면 문자 수)
///
/// # 예제
/// 이모지는 UTF-16 두 칸
/// ```rust
/// use vanilla::core::textinput::convert_utf16_index_to_character_index;
///
/// let text = "a\u{1F600}b";
/// assert_eq!(convert_utf16_index_to_character_index(text, 1), 1);
/// assert_eq!(convert_utf16_index_to_character_index(text, 2), 2);
/// assert_eq!(convert_utf16_index_to_character_index(text, 3), 2);
/// assert_eq!(convert_utf16_index_to_character_index(text, 99), 3);
/// ```
//==============================================================================
pub fn convert_utf16_index_to_character_index(text: &str, utf16_index: usize) -> usize {
	let mut utf16_count = 0;
	let mut character_index = 0;
	for character in text.chars() {
		if utf16_count >= utf16_index {
			break;
		}
		utf16_count += character.len_utf16();
		character_index += 1;
	}
	character_index
}

//==============================================================================
/// 문자 자리 → UTF-16 자리. (DOM setSelectionRange)
///
/// # 인자
/// - `text`: 글.
/// - `character_index`: 문자 자리. (문자 수를 넘으면 끝으로 본다)
///
/// # 돌려줌
/// UTF-16 자리.
//==============================================================================
pub fn convert_character_index_to_utf16_index(text: &str, character_index: usize) -> usize {
	text.chars().take(character_index).map(|character| character.len_utf16()).sum()
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 자리 바꾸기. (한글과 이모지가 섞인 글)
	//==============================================================================
	#[test]
	fn index_conversion() {
		let text = "a한😀b";
		assert_eq!(count_characters(text), 4);
		assert_eq!(convert_character_index_to_byte_index(text, 2), 4);
		assert_eq!(convert_character_index_to_byte_index(text, 9), text.len());
		assert_eq!(convert_byte_index_to_character_index(text, 4), 2);
		assert_eq!(convert_byte_index_to_character_index(text, text.len()), 4);
		assert_eq!(convert_character_index_to_utf16_index(text, 3), 4);
		assert_eq!(convert_utf16_index_to_character_index(text, 4), 3);
		assert_eq!(convert_utf16_index_to_character_index(text, 99), 4);
	}

	//==============================================================================
	/// 필드 몫만 꺼내고, 남은 것은 프레임 끝에 버린다. 상태는 프레임 끝에 넘어간다.
	//==============================================================================
	#[test]
	fn take_events_and_publish() {
		let mut text_input = TextInput::new();
		text_input.push_focused_field_event(TextInputEventKind::Commit(String::from("a")));
		text_input.push_event(TextInputEvent {
			target_field_id: 7,
			kind: TextInputEventKind::FocusGained,
		});
		assert!(text_input.take_events_for_field(3, false).is_empty());
		assert_eq!(text_input.take_events_for_field(7, false), vec![TextInputEventKind::FocusGained]);
		assert_eq!(text_input.take_events_for_field(3, true), vec![TextInputEventKind::Commit(String::from("a"))]);
		assert_eq!(text_input.get_event_count(), 0);

		text_input.push_focused_field_event(TextInputEventKind::CompositionStart);
		text_input.register_field_state(TextInputFieldState {
			field_id: 3,
			focus_serial: 5,
			is_focused: true,
			is_attached: true,
			use_dominput: true,
			use_touch_end_focus: false,
			use_direct_touch: false,
			is_visible: true,
			field_rect: Rect::zero(),
			caret_rect: Rect::zero(),
			value: String::new(),
			selection_start: 0,
			cursor_index: 0,
			max_length: 0,
			value_revision: 0,
			is_multiline: false,
		});
		assert!(text_input.get_active_field_state().is_none());
		text_input.finish_frame();
		assert_eq!(text_input.get_event_count(), 0);
		assert_eq!(text_input.get_active_field_state().map(|field_state| field_state.field_id), Some(3));
		assert_eq!(text_input.get_latest_focus_serial(), 5);
	}

	//==============================================================================
	/// 확정 글 가드는 같은 글 한 번만 막고, 두 프레임 뒤에는 사라진다.
	//==============================================================================
	#[test]
	fn ime_commit_guard() {
		let mut text_input = TextInput::new();
		text_input.set_ime_commit_guard("한");
		assert!(!text_input.consume_ime_commit_guard("가"));
		assert!(text_input.consume_ime_commit_guard("한"));
		assert!(!text_input.consume_ime_commit_guard("한"));
		text_input.set_ime_commit_guard("글");
		text_input.finish_frame();
		text_input.finish_frame();
		assert!(!text_input.consume_ime_commit_guard("글"));
	}
}
