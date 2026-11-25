//==============================================================================
// 모달 질문 창. (원본과 게임이 부르는 브라우저 window.alert, window.confirm, window.prompt 자리)
// - 원본 DevTools 의 Local Storage 탭이 window.prompt, window.confirm 으로 키와 값을 묻고 지울지 묻는다. 게임도 같은 브라우저 함수를 부른다.
//   러스트 판은 엔진 함수 Engine::alert, Engine::confirm, Engine::prompt 로 같은 일을 한다.
// - 웹은 원본처럼 브라우저 함수를 부른다. 브라우저가 창을 띄우고 답할 때까지 페이지를 멈춘 뒤 돌려준 값을 손잡이에 넣는다. (그 자리에서 끝난다)
// - 데스크탑은 엔진이 창 위에 크롬 탭 모달 대화 상자(JavaScriptTabModalDialogViewViews)와 같은 질문 창을 그린다.
//   - 자리: 창 안쪽 가로 가운데, 위 끝. (크롬은 탭 내용 영역 가로 가운데에서 툴바 아래 끝과 3px 겹친다. 데스크탑 창에는 툴바가 없어 위 끝이다)
//   - 제목: "이 페이지 내용:" (크롬이 출처가 없는 페이지(file:, data:)에 쓰는 제목, 데스크탑 앱에는 출처가 없다) 닫기 X 는 없다.
//   - alert 는 확인만, confirm 과 prompt 는 확인과 취소. 기본 단추는 확인이다. 단추 차례는 크롬처럼 윈도우 [확인][취소], 맥과 리눅스 [취소][확인].
//   - prompt 입력 칸은 기본값을 넣고 모두 고른 채로 초점을 받는다. Enter 는 확인, Escape 는 취소.
//   - 글(제목, 확인, 취소)은 브라우저 화면 언어 자리인 시스템 언어로 고른다. (core/browseruitext.rs)
//   - 떠 있는 동안 원본에서 자바스크립트가 멈추듯 씬과 async 작업을 갱신하지 않고(그리기는 멈춘 화면 그대로), 입력은 창만 받는다. 닫히면 눌린 키와 단추를 비운다.
//     (원본은 창이 포커스를 가져가 페이지에 키가 오지 않는다)
//   - 입력 칸은 글 입력 줄(core/textinput.rs)에서 IME 와 편집 키를 받는다. 누르고 끌어 커서와 선택을 정한다. 되돌리기와 다시 하기도 된다. (크롬 Textfield 처럼)
//   - 여러 개를 한 프레임에 부르면 차례로 띄운다. (원본은 창이 떠 있는 동안 자바스크립트가 멈춰 겹쳐 부를 수 없다)
// - 원본 prompt 는 답할 때까지 막고 값을 돌려준다. 러스트 판은 엔진 루프를 막을 수 없어 ModalDialogHandle 을 돌려주고, 답하면 끝난다. (is_finished)
//   끝난 뒤 부를 함수는 set_callback 으로 건다. 엔진 작업 안에서는 Engine::alert_async, confirm_async, prompt_async 의 Promise 를 await 한다.
//   (떠 있는 동안은 async 작업도 원본 자바스크립트처럼 멈춘다)
//==============================================================================
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
// 질문 창 종류. (원본 window.alert, window.confirm, window.prompt)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalDialogKind {
	Alert,
	Confirm,
	Prompt,
}

//==============================================================================
// 질문 창의 답.
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModalDialogResult {
	// 확인. (prompt 는 입력한 글, alert 와 confirm 은 빈 글)
	Confirmed(String),
	// 취소. (원본 prompt 의 null, confirm 의 false)
	Cancelled,
}

// 끝난 뒤 부를 함수.
type ModalDialogCallback = Box<dyn FnOnce(&ModalDialogResult)>;

//==============================================================================
// 손잡이 상태. (답과 끝난 뒤 부를 함수)
//==============================================================================
struct ModalDialogHandleState {
	result: Option<ModalDialogResult>,
	callback: Option<ModalDialogCallback>,
}

//==============================================================================
// 질문 창 손잡이. (clone 은 같은 창을 가리킨다)
//==============================================================================
#[derive(Clone)]
pub struct ModalDialogHandle {
	state: Rc<RefCell<ModalDialogHandleState>>,
}

impl ModalDialogHandle {
	//==============================================================================
	// 답을 기다리는 손잡이.
	//==============================================================================
	pub(crate) fn create_pending() -> ModalDialogHandle {
		ModalDialogHandle {
			state: Rc::new(RefCell::new(ModalDialogHandleState {
				result: None,
				callback: None,
			})),
		}
	}

	//==============================================================================
	// 답 넣기. (걸어 둔 함수를 부른다, 두 번째부터는 무시)
	//==============================================================================
	pub(crate) fn finish(&self, result: ModalDialogResult) {
		let callback = {
			let mut state = self.state.borrow_mut();
			if state.result.is_some() {
				return;
			}
			state.result = Some(result.clone());
			state.callback.take()
		};
		if let Some(callback) = callback {
			callback(&result);
		}
	}

	//==============================================================================
	// 답했는지.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.state.borrow().result.is_some()
	}

	//==============================================================================
	// 답. (아직이면 None)
	//==============================================================================
	pub fn get_result(&self) -> Option<ModalDialogResult> {
		self.state.borrow().result.clone()
	}

	//==============================================================================
	// 확인했는지. (원본 confirm 이 돌려주는 값, 아직이거나 취소면 거짓)
	//==============================================================================
	pub fn is_confirmed(&self) -> bool {
		matches!(self.state.borrow().result, Some(ModalDialogResult::Confirmed(_)))
	}

	//==============================================================================
	// 입력한 글. (원본 prompt 가 돌려주는 값, 취소면 원본 null 처럼 None. 아직이어도 None)
	//==============================================================================
	pub fn get_prompt_text(&self) -> Option<String> {
		match &self.state.borrow().result {
			Some(ModalDialogResult::Confirmed(text)) => {
				Some(text.clone())
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 끝난 뒤 부를 함수 걸기. (이미 끝났으면 바로 부른다)
	//==============================================================================
	pub fn set_callback(&self, callback: impl FnOnce(&ModalDialogResult) + 'static) {
		let result = self.state.borrow().result.clone();
		match result {
			Some(result) => {
				callback(&result);
			}
			None => {
				self.state.borrow_mut().callback = Some(Box::new(callback));
			}
		}
	}
}

//==============================================================================
// 웹. (브라우저 함수를 불러 그 자리에서 끝낸다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
pub(crate) fn show_browser_dialog(kind: ModalDialogKind, message: &str, default_value: &str) -> ModalDialogHandle {
	let handle = ModalDialogHandle::create_pending();
	let result = match web_sys::window() {
		Some(window) => {
			match kind {
				ModalDialogKind::Alert => {
					let _ = window.alert_with_message(message);
					ModalDialogResult::Confirmed(String::new())
				}
				ModalDialogKind::Confirm => {
					if window.confirm_with_message(message).unwrap_or(false) {
						ModalDialogResult::Confirmed(String::new())
					}
					else {
						ModalDialogResult::Cancelled
					}
				}
				ModalDialogKind::Prompt => {
					match window.prompt_with_message_and_default(message, default_value).ok().flatten() {
						Some(text) => {
							ModalDialogResult::Confirmed(text)
						}
						None => {
							ModalDialogResult::Cancelled
						}
					}
				}
			}
		}
		None => {
			ModalDialogResult::Cancelled
		}
	};
	handle.finish(result);
	handle
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use desktop_dialog::{draw_modal_dialog, measure_field_text_width, tick_modal_dialog, ModalDialog};

//==============================================================================
// 데스크탑. (엔진이 그리는 질문 창)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod desktop_dialog {
	use super::{ModalDialogHandle, ModalDialogKind, ModalDialogResult};
	use crate::base::color::Color;
	use crate::base::rect::Rect;
	use crate::base::vector2::Vector2;
	use crate::core::browseruitext::{read_browser_ui_text, BrowserUiText};
	use crate::core::engine::Engine;
	use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
	use crate::core::inputmanager::KeyCode;
	use crate::core::textinput::{convert_character_index_to_byte_index, count_characters, TextInputEventKind, TextInputFieldState, TextInputKey};
	use crate::ui::uiinputfield::{TextEditBuffer, TextEditKind};

	// 글 입력 줄에서 질문 창 입력 칸을 가리키는 id. (입력 필드 id 는 1 부터 늘고, DevTools 는 u64::MAX 를 쓴다)
	const DIALOG_FIELD_ID: u64 = u64::MAX - 1;
	// 입력 칸 포커스 차례. (떠 있는 동안 다른 필드보다 늘 나중)
	const DIALOG_FOCUS_SERIAL: u64 = u64::MAX - 1;
	// 크롬 prompt 기본값의 최대 길이. (javascript_dialog 의 kDefaultPromptMaxSize)
	const DEFAULT_PROMPT_MAXIMUM_LENGTH: usize = 2000;
	// 묻는 글의 최대 줄 수와 줄 글자 수. (kMessageTextMaxRows, kMessageTextMaxCols)
	const MESSAGE_MAXIMUM_ROW_COUNT: usize = 24;
	const MESSAGE_MAXIMUM_COLUMN_COUNT: usize = 80;
	// 커서 깜빡임 주기. (초)
	const CURSOR_BLINK_SECONDS: f32 = 0.5;

	// 모양. (크롬 main 의 값: MessageBoxView kDefaultMessageWidth 400, ChromeLayoutProvider INSETS_DIALOG 20, 제목 STYLE_HEADLINE_4 16px 500 줄 24,
	// 본문 CONTEXT_DIALOG_BODY_TEXT 13px 줄 20, 제목과 본문 사이 DIALOG_CONTENT_MARGIN_TOP_TEXT 8, 본문과 입력 칸 사이 RELATED_CONTROL_VERTICAL 8,
	// 아래 여백 입력 칸이 있으면 CONTROL 24 없으면 TEXT 16, 단추 줄 여백 (0, 20, 20, 20), 단추 사이 8, 단추 최소 폭 64 안쪽 16 글 13px 500 알약 꼴,
	// 입력 칸 안쪽 10 모서리 8, 대화 상자 모서리 12, 기본 밝은 팔레트 색. 단추와 입력 칸 높이는 글꼴 높이 + 위아래 10 을 기본 글꼴로 셈한 값이다)
	const DIALOG_WIDTH: f32 = 400.0;
	const DIALOG_MARGIN: f32 = 8.0;
	const DIALOG_PADDING_TOP: f32 = 20.0;
	const DIALOG_PADDING_SIDE: f32 = 20.0;
	const DIALOG_PADDING_BOTTOM: f32 = 20.0;
	const DIALOG_CORNER_RADIUS: f32 = 12.0;
	const TITLE_FONT_STRING: &str = "500 16px system-ui";
	const TITLE_LINE_HEIGHT: f32 = 24.0;
	const MESSAGE_FONT_STRING: &str = "13px system-ui";
	const FIELD_FONT_STRING: &str = "13px system-ui";
	const MESSAGE_LINE_HEIGHT: f32 = 20.0;
	const TITLE_MESSAGE_GAP: f32 = 8.0;
	const MESSAGE_FIELD_GAP: f32 = 8.0;
	const CONTENT_BOTTOM_WITH_FIELD: f32 = 24.0;
	const CONTENT_BOTTOM_WITHOUT_FIELD: f32 = 16.0;
	const FIELD_HEIGHT: f32 = 36.0;
	const FIELD_PADDING_X: f32 = 10.0;
	const FIELD_CORNER_RADIUS: f32 = 8.0;
	const BUTTON_HEIGHT: f32 = 36.0;
	const BUTTON_MINIMUM_WIDTH: f32 = 64.0;
	const BUTTON_PADDING_X: f32 = 16.0;
	const BUTTON_GAP: f32 = 8.0;
	const BUTTON_FONT_STRING: &str = "500 13px system-ui";
	const COLOR_DIALOG_BACKGROUND: &str = "#ffffff";
	const COLOR_DIALOG_BORDER: &str = "#e1e3e1";
	const COLOR_TITLE: &str = "#1f1f1f";
	const COLOR_MESSAGE: &str = "#1f1f1f";
	const COLOR_FOCUS_RING: &str = "#0b57d0";
	const COLOR_SELECTION: &str = "#d3e3fd";
	const COLOR_PRIMARY_BUTTON: &str = "#0b57d0";
	const COLOR_PRIMARY_BUTTON_TEXT: &str = "#ffffff";
	const COLOR_SECONDARY_BUTTON_BORDER: &str = "#a8c7fa";
	const COLOR_SECONDARY_BUTTON_TEXT: &str = "#0b57d0";
	// 그림자. (bubble_border.cc 의 두 겹: 아래로 2, 흐림 8 과 12, kGoogleGrey800 #3C4043 의 알파 0x33 과 0x1A. 맥은 운영체제가 그린다)
	const SHADOW_COLOR_RED: f32 = 60.0 / 255.0;
	const SHADOW_COLOR_GREEN: f32 = 64.0 / 255.0;
	const SHADOW_COLOR_BLUE: f32 = 67.0 / 255.0;

	//==============================================================================
	// 단추.
	//==============================================================================
	#[derive(Clone, Copy, Debug, PartialEq, Eq)]
	enum DialogButton {
		Ok,
		Cancel,
	}

	//==============================================================================
	// 질문 창 자리. (창 안쪽 논리 좌표)
	//==============================================================================
	#[derive(Clone, Debug, Default)]
	struct DialogLayout {
		box_rect: Rect,
		title_line_list: Vec<String>,
		title_top: f32,
		message_line_list: Vec<String>,
		message_top: f32,
		field_rect: Option<Rect>,
		button_rect_list: Vec<(DialogButton, Rect)>,
	}

	//==============================================================================
	// 떠 있는 질문 창 하나.
	//==============================================================================
	pub struct ModalDialog {
		kind: ModalDialogKind,
		message: String,
		edit_buffer: TextEditBuffer,
		handle: ModalDialogHandle,
		value_revision: u64,
		layout: Option<DialogLayout>,
		pressed_button: Option<DialogButton>,
		hovered_button: Option<DialogButton>,
		drag_start_index: Option<usize>,
		// 입력 칸 글의 가로 밀림. (커서가 보이게)
		text_scroll_x: f32,
		// 마지막으로 그린 커서 자리. (IME 후보 창)
		caret_rect: Rect,
		blink_timer: f32,
		is_cursor_visible: bool,
	}

	impl ModalDialog {
		//==============================================================================
		// 생성. (prompt 는 기본값을 넣고 모두 고른다. 크롬처럼 커서는 글 맨 앞 쪽에 둔다)
		//==============================================================================
		pub fn new(kind: ModalDialogKind, message: &str, default_value: &str, handle: ModalDialogHandle) -> ModalDialog {
			let mut edit_buffer = TextEditBuffer::new();
			edit_buffer.set_max_length(DEFAULT_PROMPT_MAXIMUM_LENGTH);
			if kind == ModalDialogKind::Prompt {
				let mut default_text = default_value.to_string();
				if count_characters(&default_text) > DEFAULT_PROMPT_MAXIMUM_LENGTH {
					default_text = default_text.chars().take(DEFAULT_PROMPT_MAXIMUM_LENGTH).collect();
				}
				edit_buffer.set_text(&default_text);
				edit_buffer.set_selection(count_characters(&default_text), 0);
			}
			ModalDialog {
				kind,
				message: limit_message(message),
				edit_buffer,
				handle,
				value_revision: 1,
				layout: None,
				pressed_button: None,
				hovered_button: None,
				drag_start_index: None,
				text_scroll_x: 0.0,
				caret_rect: Rect::create(0.0, 0.0, 1.0, 1.0),
				blink_timer: 0.0,
				is_cursor_visible: true,
			}
		}

		//==============================================================================
		// 입력 칸의 글. (시험이 본다)
		//==============================================================================
		#[cfg(test)]
		pub(crate) fn get_text(&self) -> &str {
			self.edit_buffer.get_text()
		}

		//==============================================================================
		// 단추 목록. (크롬 kIsOkButtonLeading: 윈도우는 확인이 앞, 맥과 리눅스는 취소가 앞)
		//==============================================================================
		fn read_button_list(&self) -> Vec<DialogButton> {
			if self.kind == ModalDialogKind::Alert {
				return vec![DialogButton::Ok];
			}
			if cfg!(target_os = "windows") {
				vec![DialogButton::Ok, DialogButton::Cancel]
			}
			else {
				vec![DialogButton::Cancel, DialogButton::Ok]
			}
		}

		//==============================================================================
		// 답. (확인이면 prompt 는 입력한 글)
		//==============================================================================
		fn build_result(&self, is_confirmed: bool) -> ModalDialogResult {
			if !is_confirmed {
				return ModalDialogResult::Cancelled;
			}
			if self.kind == ModalDialogKind::Prompt {
				return ModalDialogResult::Confirmed(self.edit_buffer.get_text().to_string());
			}
			ModalDialogResult::Confirmed(String::new())
		}

		//==============================================================================
		// 커서 깜빡임을 처음부터.
		//==============================================================================
		fn reset_cursor_blink(&mut self) {
			self.blink_timer = 0.0;
			self.is_cursor_visible = true;
		}

		//==============================================================================
		// 글이나 커서가 바뀜. (플랫폼 IME 상태를 다시 맞춘다)
		//==============================================================================
		fn bump_value_revision(&mut self) {
			self.value_revision += 1;
			self.reset_cursor_blink();
		}

		//==============================================================================
		// 글 입력 이벤트 하나. (답이 정해지면 Some(확인인지))
		//==============================================================================
		fn handle_text_input_event(&mut self, engine: &Engine, event_kind: TextInputEventKind) -> Option<bool> {
			match event_kind {
				TextInputEventKind::Commit(text) | TextInputEventKind::Paste(text) => {
					let printable_text: String = text.chars().filter(|character| !character.is_control()).collect();
					if self.kind == ModalDialogKind::Prompt && !printable_text.is_empty() {
						self.edit_buffer.begin_edit(TextEditKind::InsertText);
						self.edit_buffer.replace_selection(&printable_text);
						self.bump_value_revision();
					}
				}
				TextInputEventKind::Preedit { text, .. } => {
					self.edit_buffer.set_composition(&text);
					self.reset_cursor_blink();
				}
				TextInputEventKind::CompositionEnd => {
					self.edit_buffer.set_composition("");
				}
				TextInputEventKind::Key { key, is_shift } => {
					return self.handle_edit_key(engine, key, is_shift);
				}
				_ => {
				}
			}
			None
		}

		//==============================================================================
		// 편집 키. (Enter 는 확인, 나머지는 prompt 입력 칸 편집)
		//==============================================================================
		fn handle_edit_key(&mut self, engine: &Engine, key: TextInputKey, is_shift: bool) -> Option<bool> {
			if key == TextInputKey::Enter {
				return Some(true);
			}
			if self.kind != ModalDialogKind::Prompt {
				return None;
			}
			let cursor_index = self.edit_buffer.get_cursor_index();
			let character_count = count_characters(self.edit_buffer.get_text());
			match key {
				TextInputKey::SelectAll => {
					self.edit_buffer.select_all();
				}
				TextInputKey::Copy => {
					if self.edit_buffer.has_selection() {
						engine.write_clipboard_text(&self.edit_buffer.get_selection_text());
					}
				}
				TextInputKey::Cut => {
					if self.edit_buffer.has_selection() {
						engine.write_clipboard_text(&self.edit_buffer.get_selection_text());
						self.edit_buffer.begin_edit(TextEditKind::Paste);
						self.edit_buffer.replace_selection("");
					}
				}
				TextInputKey::Paste => {
					if let Some(clipboard_text) = engine.read_clipboard_text() {
						let printable_text: String = clipboard_text.chars().filter(|character| !character.is_control()).collect();
						self.edit_buffer.begin_edit(TextEditKind::Paste);
						self.edit_buffer.replace_selection(&printable_text);
					}
				}
				TextInputKey::Backspace => {
					if self.edit_buffer.has_selection() || cursor_index > 0 {
						self.edit_buffer.begin_edit(TextEditKind::DeleteBackward);
					}
					self.edit_buffer.delete_backward();
				}
				TextInputKey::Delete => {
					if self.edit_buffer.has_selection() || cursor_index < character_count {
						self.edit_buffer.begin_edit(TextEditKind::DeleteForward);
					}
					self.edit_buffer.delete_forward();
				}
				TextInputKey::Left => {
					let new_index = match (self.edit_buffer.get_selection_range(), is_shift) {
						(Some((start_index, _)), false) => {
							start_index
						}
						_ => {
							cursor_index.saturating_sub(1)
						}
					};
					self.edit_buffer.move_cursor(new_index, is_shift);
				}
				TextInputKey::Right => {
					let new_index = match (self.edit_buffer.get_selection_range(), is_shift) {
						(Some((_, end_index)), false) => {
							end_index
						}
						_ => {
							(cursor_index + 1).min(character_count)
						}
					};
					self.edit_buffer.move_cursor(new_index, is_shift);
				}
				TextInputKey::Home => {
					self.edit_buffer.move_cursor(0, is_shift);
				}
				TextInputKey::End => {
					self.edit_buffer.move_cursor(character_count, is_shift);
				}
				TextInputKey::Undo => {
					self.edit_buffer.undo();
				}
				TextInputKey::Redo => {
					self.edit_buffer.redo();
				}
				TextInputKey::Enter | TextInputKey::Tab => {
				}
			}
			self.bump_value_revision();
			None
		}

		//==============================================================================
		// 입력 칸 안 x 자리의 글자 자리. (가장 가까운 글자 경계)
		//==============================================================================
		fn find_character_index_at(&self, measure_width: &dyn Fn(&str) -> f32, local_x: f32) -> usize {
			let text = self.edit_buffer.get_text();
			let character_count = count_characters(text);
			let mut previous_width = 0.0;
			for character_index in 1..=character_count {
				let byte_index = convert_character_index_to_byte_index(text, character_index);
				let width = measure_width(&text[..byte_index]);
				if local_x < (previous_width + width) * 0.5 {
					return character_index - 1;
				}
				previous_width = width;
			}
			character_count
		}
	}

	//==============================================================================
	// 입력 칸 글꼴의 글 폭. (tick_modal_dialog 에 넘긴다)
	//==============================================================================
	pub fn measure_field_text_width(graphic: &Graphic, text: &str) -> f32 {
		graphic.measure_text_with_font_string(FIELD_FONT_STRING, text).width
	}

	//==============================================================================
	// 묻는 글 자르기. (크롬처럼 24 줄, 줄마다 80 글자까지)
	//==============================================================================
	fn limit_message(message: &str) -> String {
		let mut line_list: Vec<String> = Vec::new();
		for line in message.split('\n').take(MESSAGE_MAXIMUM_ROW_COUNT) {
			line_list.push(line.chars().take(MESSAGE_MAXIMUM_COLUMN_COUNT).collect());
		}
		line_list.join("\n")
	}

	//==============================================================================
	// 글을 폭에 맞춰 줄로 나누기. (공백에서 나누고, 공백 없이 넘치면 글자에서 나눈다. 줄바꿈 문자는 지킨다)
	//==============================================================================
	fn wrap_lines(graphic: &Graphic, font_string: &str, text: &str, maximum_width: f32) -> Vec<String> {
		let mut line_list: Vec<String> = Vec::new();
		for paragraph in text.split('\n') {
			let mut current_line = String::new();
			for word in paragraph.split_inclusive(' ') {
				let candidate = format!("{}{}", current_line, word);
				if current_line.is_empty() || graphic.measure_text_with_font_string(font_string, candidate.trim_end()).width <= maximum_width {
					current_line = candidate;
					if graphic.measure_text_with_font_string(font_string, current_line.trim_end()).width <= maximum_width {
						continue;
					}
				}
				else {
					line_list.push(std::mem::take(&mut current_line).trim_end().to_string());
					current_line = word.to_string();
				}
				// 한 낱말이 폭을 넘으면 글자에서 나눈다.
				let mut character_line = String::new();
				for character in current_line.chars() {
					let character_candidate = format!("{}{}", character_line, character);
					if !character_line.is_empty() && graphic.measure_text_with_font_string(font_string, character_candidate.trim_end()).width > maximum_width {
						line_list.push(std::mem::replace(&mut character_line, character.to_string()));
					}
					else {
						character_line = character_candidate;
					}
				}
				current_line = character_line;
			}
			line_list.push(current_line.trim_end().to_string());
		}
		line_list
	}

	//==============================================================================
	// 질문 창 자리 셈. (창 안쪽 가로 가운데 위 끝, 폭은 창보다 넓지 않게)
	//==============================================================================
	fn calculate_layout(dialog: &ModalDialog, graphic: &Graphic, canvas_native_size: Vector2) -> DialogLayout {
		let box_width = DIALOG_WIDTH.min((canvas_native_size.x - DIALOG_MARGIN * 2.0).max(160.0));
		let content_width = box_width - DIALOG_PADDING_SIDE * 2.0;
		let title_line_list = wrap_lines(graphic, TITLE_FONT_STRING, &read_browser_ui_text(BrowserUiText::PageSays), content_width);
		let message_line_list = if dialog.message.is_empty() {
			Vec::new()
		}
		else {
			wrap_lines(graphic, MESSAGE_FONT_STRING, &dialog.message, content_width)
		};
		let box_x = ((canvas_native_size.x - box_width) * 0.5).max(0.0);
		let box_y = 0.0;
		let title_top = box_y + DIALOG_PADDING_TOP;
		let mut content_bottom = title_top + TITLE_LINE_HEIGHT * title_line_list.len() as f32 + TITLE_MESSAGE_GAP;
		let message_top = content_bottom;
		if !message_line_list.is_empty() {
			content_bottom = message_top + MESSAGE_LINE_HEIGHT * message_line_list.len() as f32;
		}
		let field_rect = if dialog.kind == ModalDialogKind::Prompt {
			let field_rect = Rect::create(box_x + DIALOG_PADDING_SIDE, content_bottom + MESSAGE_FIELD_GAP, content_width, FIELD_HEIGHT);
			content_bottom = field_rect.position.y + FIELD_HEIGHT + CONTENT_BOTTOM_WITH_FIELD;
			Some(field_rect)
		}
		else {
			content_bottom += CONTENT_BOTTOM_WITHOUT_FIELD;
			None
		};
		let button_top = content_bottom;
		let mut button_rect_list: Vec<(DialogButton, Rect)> = Vec::new();
		let mut button_right = box_x + box_width - DIALOG_PADDING_SIDE;
		for button in dialog.read_button_list().into_iter().rev() {
			let button_text = read_button_text(button);
			let text_width = graphic.measure_text_with_font_string(BUTTON_FONT_STRING, &button_text).width;
			let button_width = (text_width + BUTTON_PADDING_X * 2.0).ceil().max(BUTTON_MINIMUM_WIDTH);
			let button_rect = Rect::create(button_right - button_width, button_top, button_width, BUTTON_HEIGHT);
			button_rect_list.insert(0, (button, button_rect));
			button_right -= button_width + BUTTON_GAP;
		}
		let box_height = button_top + BUTTON_HEIGHT + DIALOG_PADDING_BOTTOM - box_y;
		DialogLayout {
			box_rect: Rect::create(box_x, box_y, box_width, box_height),
			title_line_list,
			title_top,
			message_line_list,
			message_top,
			field_rect,
			button_rect_list,
		}
	}

	//==============================================================================
	// 단추 글. (브라우저 화면 언어의 확인, 취소)
	//==============================================================================
	fn read_button_text(button: DialogButton) -> String {
		match button {
			DialogButton::Ok => {
				read_browser_ui_text(BrowserUiText::Ok)
			}
			DialogButton::Cancel => {
				read_browser_ui_text(BrowserUiText::Cancel)
			}
		}
	}

	//==============================================================================
	// 점이 사각형 안에 있는지.
	//==============================================================================
	fn is_inside(rect: Rect, position: Vector2) -> bool {
		position.x >= rect.position.x && position.x < rect.position.x + rect.size.x && position.y >= rect.position.y && position.y < rect.position.y + rect.size.y
	}

	//==============================================================================
	// 맨 앞 질문 창 갱신. (엔진이 프레임마다 씬 대신 부른다. 답이 정해지면 창을 내리고 손잡이를 끝낸다)
	//==============================================================================
	// - measure_width 는 입력 칸 글꼴로 글 폭을 잰다. (엔진은 Graphic 으로 잰다, measure_field_text_width)
	pub fn tick_modal_dialog(engine: &mut Engine, measure_width: &dyn Fn(&str) -> f32, time_delta: f32) {
		let mut dialog = match engine.take_front_modal_dialog() {
			Some(dialog) => {
				dialog
			}
			None => {
				return;
			}
		};
		let mut answer: Option<bool> = None;

		// 글과 키.
		let event_kind_list = engine.get_input_manager().get_text_input().take_events_for_field(DIALOG_FIELD_ID, true);
		for event_kind in event_kind_list {
			if let Some(is_confirmed) = dialog.handle_text_input_event(engine, event_kind) {
				answer = Some(is_confirmed);
			}
		}
		if engine.get_input_manager().is_key_just_pressed(KeyCode::ESCAPE) {
			answer = Some(false);
		}

		// 누르기. (단추는 눌렀다 그 위에서 떼면, 입력 칸은 눌러 커서를 두고 끌어 고른다)
		let input_manager = engine.get_input_manager();
		let pointer_position = input_manager.get_canvas_native_input_position();
		let is_touch_pressed = input_manager.is_touch_pressed();
		let is_touch_released = input_manager.is_touch_released();
		let is_touch_down = input_manager.is_touch_down();
		if let Some(layout) = dialog.layout.clone() {
			dialog.hovered_button = layout.button_rect_list.iter().find(|(_, button_rect)| is_inside(*button_rect, pointer_position)).map(|(button, _)| *button);
			if is_touch_pressed {
				dialog.pressed_button = dialog.hovered_button;
				if let Some(field_rect) = layout.field_rect {
					if is_inside(field_rect, pointer_position) {
						let local_x = pointer_position.x - field_rect.position.x - FIELD_PADDING_X + dialog.text_scroll_x;
						let character_index = dialog.find_character_index_at(measure_width, local_x);
						dialog.edit_buffer.set_selection(character_index, character_index);
						dialog.drag_start_index = Some(character_index);
						dialog.bump_value_revision();
					}
				}
			}
			else if is_touch_down {
				if let (Some(drag_start_index), Some(field_rect)) = (dialog.drag_start_index, layout.field_rect) {
					let local_x = pointer_position.x - field_rect.position.x - FIELD_PADDING_X + dialog.text_scroll_x;
					let character_index = dialog.find_character_index_at(measure_width, local_x);
					if character_index != dialog.edit_buffer.get_cursor_index() {
						dialog.edit_buffer.set_selection(drag_start_index, character_index);
						dialog.bump_value_revision();
					}
				}
			}
			if is_touch_released {
				if dialog.pressed_button.is_some() && dialog.pressed_button == dialog.hovered_button {
					answer = Some(dialog.pressed_button == Some(DialogButton::Ok));
				}
				dialog.pressed_button = None;
				dialog.drag_start_index = None;
			}
			if dialog.hovered_button.is_some() {
				engine.get_pointer_cursor().mark_over_button();
			}
		}

		// 커서 깜빡임.
		dialog.blink_timer += time_delta;
		if dialog.blink_timer >= CURSOR_BLINK_SECONDS {
			dialog.blink_timer = 0.0;
			dialog.is_cursor_visible = !dialog.is_cursor_visible;
		}

		match answer {
			Some(is_confirmed) => {
				// 창을 내리면 페이지가 포커스를 돌려받는다. 창이 받던 키와 단추는 비운다.
				let result = dialog.build_result(is_confirmed);
				let handle = dialog.handle.clone();
				engine.get_input_manager_mut().clear();
				handle.finish(result);
			}
			None => {
				// prompt 는 입력 칸 상태를 올려 데스크탑 IME 를 켠다. (답한 프레임에는 올리지 않아 페이지의 필드가 포커스를 그대로 돌려받는다)
				if let (ModalDialogKind::Prompt, Some(field_rect)) = (dialog.kind, dialog.layout.as_ref().and_then(|layout| layout.field_rect)) {
					engine.get_input_manager().get_text_input().register_field_state(TextInputFieldState {
						field_id: DIALOG_FIELD_ID,
						focus_serial: DIALOG_FOCUS_SERIAL,
						is_focused: true,
						is_attached: true,
						use_dominput: true,
						use_touch_end_focus: false,
						use_direct_touch: false,
						is_visible: true,
						field_rect,
						caret_rect: dialog.caret_rect,
						value: dialog.edit_buffer.get_text().to_string(),
						selection_start: dialog.edit_buffer.get_selection_start(),
						cursor_index: dialog.edit_buffer.get_cursor_index(),
						max_length: DEFAULT_PROMPT_MAXIMUM_LENGTH,
						value_revision: dialog.value_revision,
					});
				}
				engine.put_back_front_modal_dialog(dialog);
			}
		}
	}

	//==============================================================================
	// 맨 앞 질문 창 그리기. (씬과 개발 정보 위에, 창 안쪽 논리 좌표로)
	//==============================================================================
	pub fn draw_modal_dialog(engine: &mut Engine, graphic: &mut Graphic) {
		let canvas_native_size = engine.get_view_manager().get_canvas_native_size();
		let dialog = match engine.get_front_modal_dialog_mut() {
			Some(dialog) => {
				dialog
			}
			None => {
				return;
			}
		};
		let layout = calculate_layout(dialog, graphic, canvas_native_size);
		graphic.push_state();

		// 그림자와 바탕. (흐림은 겹친 둥근 사각형으로 갈음한다, 윈도우와 리눅스는 1px 테두리)
		let box_rect = layout.box_rect;
		for (blur_size, layer_alpha) in [(8.0f32, 0x33 as f32 / 255.0), (12.0f32, 0x1A as f32 / 255.0)] {
			let step_count = 4;
			for step_index in 0..step_count {
				let spread = blur_size * 0.5 * (step_index as f32 + 1.0) / step_count as f32;
				graphic.set_fill_color(Color::create(SHADOW_COLOR_RED, SHADOW_COLOR_GREEN, SHADOW_COLOR_BLUE, layer_alpha / step_count as f32));
				graphic.draw_round_rect(Rect::create(box_rect.position.x - spread, box_rect.position.y + 2.0 - spread, box_rect.size.x + spread * 2.0, box_rect.size.y + spread * 2.0), DIALOG_CORNER_RADIUS + spread);
			}
		}
		graphic.set_fill_color(COLOR_DIALOG_BACKGROUND);
		graphic.draw_round_rect(box_rect, DIALOG_CORNER_RADIUS);
		if !cfg!(target_os = "macos") {
			graphic.set_stroke_color(COLOR_DIALOG_BORDER);
			graphic.draw_stroke_round_rect(Rect::create(box_rect.position.x + 0.5, box_rect.position.y + 0.5, box_rect.size.x - 1.0, box_rect.size.y - 1.0), DIALOG_CORNER_RADIUS - 0.5, 1.0);
		}

		// 제목과 묻는 글.
		let content_x = box_rect.position.x + DIALOG_PADDING_SIDE;
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_font_string(TITLE_FONT_STRING);
		graphic.set_fill_color(COLOR_TITLE);
		for (line_index, line) in layout.title_line_list.iter().enumerate() {
			graphic.draw_fill_text(line, content_x, layout.title_top + TITLE_LINE_HEIGHT * (line_index as f32 + 0.5));
		}
		graphic.set_font_string(MESSAGE_FONT_STRING);
		graphic.set_fill_color(COLOR_MESSAGE);
		for (line_index, line) in layout.message_line_list.iter().enumerate() {
			graphic.draw_fill_text(line, content_x, layout.message_top + MESSAGE_LINE_HEIGHT * (line_index as f32 + 0.5));
		}

		// 입력 칸. (초점 테두리, 선택, 글, 조합 밑줄, 커서)
		if let Some(field_rect) = layout.field_rect {
			graphic.set_fill_color(COLOR_DIALOG_BACKGROUND);
			graphic.draw_round_rect(field_rect, FIELD_CORNER_RADIUS);
			graphic.set_stroke_color(COLOR_FOCUS_RING);
			graphic.draw_stroke_round_rect(Rect::create(field_rect.position.x + 1.0, field_rect.position.y + 1.0, field_rect.size.x - 2.0, field_rect.size.y - 2.0), FIELD_CORNER_RADIUS - 1.0, 2.0);
			graphic.set_font_string(FIELD_FONT_STRING);
			let text = dialog.edit_buffer.get_text().to_string();
			let composition = dialog.edit_buffer.get_composition().to_string();
			let display_text = dialog.edit_buffer.build_display_text();
			let text_before_cursor = dialog.edit_buffer.get_text_before_cursor().to_string();
			let caret_width = graphic.measure_text(&format!("{}{}", text_before_cursor, composition)).width;
			let inner_width = field_rect.size.x - FIELD_PADDING_X * 2.0;
			if caret_width - dialog.text_scroll_x > inner_width {
				dialog.text_scroll_x = caret_width - inner_width;
			}
			if caret_width < dialog.text_scroll_x {
				dialog.text_scroll_x = caret_width;
			}
			let text_x = field_rect.position.x + FIELD_PADDING_X - dialog.text_scroll_x;
			let middle_y = field_rect.position.y + field_rect.size.y * 0.5;
			graphic.begin_clip_rect(Rect::create(field_rect.position.x + 2.0, field_rect.position.y + 2.0, field_rect.size.x - 4.0, field_rect.size.y - 4.0), 0.0);
			if let Some((start_index, end_index)) = dialog.edit_buffer.get_selection_range() {
				let start_width = graphic.measure_text(&text[..convert_character_index_to_byte_index(&text, start_index)]).width;
				let end_width = graphic.measure_text(&text[..convert_character_index_to_byte_index(&text, end_index)]).width;
				graphic.set_fill_color(COLOR_SELECTION);
				graphic.draw_rect(Rect::create(text_x + start_width, middle_y - 9.0, end_width - start_width, 18.0));
			}
			graphic.set_fill_color(COLOR_TITLE);
			graphic.draw_fill_text(&display_text, text_x, middle_y);
			if !composition.is_empty() {
				let composition_start = graphic.measure_text(&text_before_cursor).width;
				graphic.draw_rect(Rect::create(text_x + composition_start, middle_y + 8.0, caret_width - composition_start, 1.0));
			}
			let caret_x = text_x + caret_width;
			dialog.caret_rect = Rect::create(caret_x, middle_y - 9.0, 1.0, 18.0);
			if dialog.is_cursor_visible && !dialog.edit_buffer.has_selection() {
				graphic.draw_rect(dialog.caret_rect);
			}
			graphic.end_clip_rect();
		}

		// 단추. (기본 단추는 확인)
		graphic.set_font_string(BUTTON_FONT_STRING);
		graphic.set_text_align(TextAlign::Center);
		for (button, button_rect) in layout.button_rect_list.iter() {
			let is_primary = *button == DialogButton::Ok;
			let is_hovered = dialog.hovered_button == Some(*button);
			let corner_radius = BUTTON_HEIGHT * 0.5;
			if is_primary {
				graphic.set_fill_color(COLOR_PRIMARY_BUTTON);
				graphic.draw_round_rect(*button_rect, corner_radius);
			}
			else {
				graphic.set_fill_color(COLOR_DIALOG_BACKGROUND);
				graphic.draw_round_rect(*button_rect, corner_radius);
				graphic.set_stroke_color(COLOR_SECONDARY_BUTTON_BORDER);
				graphic.draw_stroke_round_rect(Rect::create(button_rect.position.x + 0.5, button_rect.position.y + 0.5, button_rect.size.x - 1.0, button_rect.size.y - 1.0), corner_radius - 0.5, 1.0);
			}
			if is_hovered {
				graphic.set_fill_color(Color::create(0.0, 0.0, 0.0, 0.08));
				graphic.draw_round_rect(*button_rect, corner_radius);
			}
			let text_color = if is_primary {
				COLOR_PRIMARY_BUTTON_TEXT
			}
			else {
				COLOR_SECONDARY_BUTTON_TEXT
			};
			graphic.set_fill_color(text_color);
			graphic.draw_fill_text(&read_button_text(*button), button_rect.position.x + button_rect.size.x * 0.5, button_rect.position.y + button_rect.size.y * 0.5);
		}
		graphic.pop_state();
		dialog.layout = Some(layout);
	}
}

//==============================================================================
// 시험. (데스크탑 질문 창)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::inputmanager::KeyCode;
	use crate::core::textinput::{TextInputEventKind, TextInputKey};
	use std::cell::Cell;

	//==============================================================================
	// 글자마다 8 의 폭.
	//==============================================================================
	fn measure_fixed_width(text: &str) -> f32 {
		text.chars().count() as f32 * 8.0
	}

	//==============================================================================
	// prompt: 기본값을 모두 고른 채로 뜨고, 친 글이 고른 것을 바꾸고, Enter 로 확인한다. 끝난 뒤 함수가 불린다.
	//==============================================================================
	#[test]
	fn prompt_replaces_selected_default_and_confirms() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let handle = engine.prompt("이름:", "기본");
		let callback_count = Rc::new(Cell::new(0));
		let callback_count_for_callback = callback_count.clone();
		handle.set_callback(move |result| {
			assert_eq!(result, &ModalDialogResult::Confirmed(String::from("바닐라")));
			callback_count_for_callback.set(callback_count_for_callback.get() + 1);
		});
		assert!(engine.is_modal_dialog_open());
		assert!(!handle.is_finished());
		assert_eq!(engine.get_front_modal_dialog_mut().expect("dialog").get_text(), "기본");
		let text_input = engine.get_input_manager_mut().get_text_input_mut();
		text_input.push_focused_field_event(TextInputEventKind::Commit(String::from("바닐라")));
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		assert_eq!(engine.get_front_modal_dialog_mut().expect("dialog").get_text(), "바닐라");
		engine.get_input_manager_mut().get_text_input_mut().finish_frame();
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Key { key: TextInputKey::Enter, is_shift: false });
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		assert!(!engine.is_modal_dialog_open());
		assert_eq!(handle.get_prompt_text().as_deref(), Some("바닐라"));
		assert!(handle.is_confirmed());
		assert_eq!(callback_count.get(), 1);
	}

	//==============================================================================
	// confirm 과 prompt 는 Escape 로 취소한다. (원본 false 와 null) 여럿을 부르면 차례로 뜬다.
	//==============================================================================
	#[test]
	fn escape_cancels_and_dialogs_queue() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let confirm_handle = engine.confirm("지울까요?");
		let prompt_handle = engine.prompt("키:", "");
		engine.get_input_manager_mut().push_key(KeyCode::ESCAPE);
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		assert_eq!(confirm_handle.get_result(), Some(ModalDialogResult::Cancelled));
		assert!(!confirm_handle.is_confirmed());
		assert!(engine.is_modal_dialog_open());
		assert!(!prompt_handle.is_finished());
		engine.get_input_manager_mut().update_frame_snapshot();
		engine.get_input_manager_mut().push_key(KeyCode::ESCAPE);
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		assert_eq!(prompt_handle.get_prompt_text(), None);
		assert!(!engine.is_modal_dialog_open());

		// alert 는 Enter 로 닫힌다.
		let alert_handle = engine.alert("끝");
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Key { key: TextInputKey::Enter, is_shift: false });
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		assert!(alert_handle.is_confirmed());
	}

	//==============================================================================
	// await 꼴: 엔진 작업이 prompt_async 를 기다리고, 답하면 다음 폴링에 그 값으로 이어 간다.
	//==============================================================================
	#[test]
	fn prompt_async_resolves_in_task() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut scene_list: Vec<Box<dyn crate::core::scene::Scene>> = Vec::new();
		let received_text = Rc::new(std::cell::RefCell::new(None));
		let task_received_text = received_text.clone();
		engine.spawn_task(move |mut engine_context| async move {
			let prompt_promise = engine_context.with(|engine| engine.prompt_async("이름:", "기본"));
			*task_received_text.borrow_mut() = Some(prompt_promise.await);
		});
		crate::core::task::poll_tasks(&mut engine, &mut scene_list, None);
		assert!(engine.is_modal_dialog_open());
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Key { key: TextInputKey::Enter, is_shift: false });
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		crate::core::task::poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(*received_text.borrow(), Some(Some(String::from("기본"))));
	}

	//==============================================================================
	// prompt 편집 키. (왼쪽은 선택을 접고, Shift 와 함께면 늘린다. 지우기, 모두 고르기)
	//==============================================================================
	#[test]
	fn prompt_edit_keys() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let handle = engine.prompt("", "abc");
		let key_list = [(TextInputKey::Left, false), (TextInputKey::Right, false), (TextInputKey::Left, true), (TextInputKey::Backspace, false), (TextInputKey::Home, false), (TextInputKey::Delete, false)];
		for (key, is_shift) in key_list {
			engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Key { key, is_shift });
		}
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		// "abc" 모두 고름 → 왼쪽(맨 앞으로 접음) → 오른쪽 → Shift 왼쪽("a" 고름) → 뒤로 지우기("bc") → 맨 앞 → 앞으로 지우기("c")
		assert_eq!(engine.get_front_modal_dialog_mut().expect("dialog").get_text(), "c");
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Key { key: TextInputKey::SelectAll, is_shift: false });
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Commit(String::from("z")));
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Key { key: TextInputKey::Enter, is_shift: false });
		tick_modal_dialog(&mut engine, &measure_fixed_width, 0.016);
		assert_eq!(handle.get_prompt_text().as_deref(), Some("z"));
	}
}
