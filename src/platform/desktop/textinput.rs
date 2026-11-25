//==============================================================================
//! 데스크탑 글 입력. (winit 0.30 창 이벤트 → 엔진 글 입력 줄, 시스템 클립보드)
//! - 원본 UIInputField 의 DOM <input>(set_use_dominput(true)) 자리와 document keydown(캔버스 모드) 자리를 함께 맡는다.
//!   - KeyboardInput: 편집 키(Backspace, Delete, 화살표, Home, End, Enter, Tab), Ctrl/Cmd + A/C/V/X, KeyEvent.text 의 글자 → 글 입력 줄.
//!   - Ime: Preedit 는 조합 시작 / 조합 글 / 조합 끝으로, Commit 은 확정 글로 바꾼다. (Preedit 커서는 바이트 자리라 문자 자리로 바꾼다)
//!   - 프레임마다 지난 프레임 필드 상태를 읽어, 포커스를 가진 필드가 플랫폼 입력(use_dominput)을 쓸 때만 IME 를 켜고 후보 창 자리를 맞춘다.
//!   ```text
//!     포커스가 다른 필드로 옮겨 가거나 조합 중에 필드 안을 눌러 필드가 조합 글을 확정하면 IME 를 껐다 켜서 조합을 끊는다.
//!     (끊긴 조합 글은 필드가 이미 확정했으므로 OS 가 다시 보낸 같은 Commit 은 버린다)
//!   ```
//! - 클립보드는 arboard 로 읽고 쓴다. (winit 에 클립보드가 없다. 리눅스는 Clipboard 객체가 살아 있어야 내용이 남아서 스레드에 하나 들고 있는다)
//==============================================================================
use crate::base::rect::Rect;
use crate::core::textinput::{convert_byte_index_to_character_index, TextInput, TextInputEventKind, TextInputKey};
use std::cell::RefCell;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::Window;

thread_local! {
	/// 시스템 클립보드. (처음 쓸 때 연다)
	static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

//==============================================================================
/// 데스크탑 글 입력 상태.
//==============================================================================
pub(super) struct DesktopTextInputState {
	modifiers_state: ModifiersState,
	is_ime_composing: bool,
	is_ime_allowed: bool,
	ime_owner_field_id: u64,
	ime_cursor_area: Option<Rect>,
}

impl DesktopTextInputState {
	//==============================================================================
	/// 생성. (IME 는 winit 기본처럼 꺼진 채로 시작한다)
	//==============================================================================
	pub(super) fn new() -> DesktopTextInputState {
		DesktopTextInputState {
			modifiers_state: ModifiersState::empty(),
			is_ime_composing: false,
			is_ime_allowed: false,
			ime_owner_field_id: 0,
			ime_cursor_area: None,
		}
	}

	//==============================================================================
	/// 수정 키 상태 갱신. (WindowEvent::ModifiersChanged)
	//==============================================================================
	pub(super) fn set_modifiers(&mut self, modifiers_state: ModifiersState) {
		self.modifiers_state = modifiers_state;
	}

	//==============================================================================
	/// 키 이벤트 → 글 입력 줄. (눌림과 반복만)
	//==============================================================================
	pub(super) fn handle_key_event(&mut self, key_event: &KeyEvent, text_input: &mut TextInput) {
		if key_event.state != ElementState::Pressed {
			return;
		}
		let text = key_event.text.as_ref().map(|text| text.as_str());
		if let Some(event_kind) = translate_key(&key_event.logical_key, key_event.physical_key, text, self.modifiers_state) {
			text_input.push_focused_field_event(event_kind);
		}
	}

	//==============================================================================
	/// IME 이벤트 → 글 입력 줄.
	//==============================================================================
	pub(super) fn handle_ime_event(&mut self, ime: Ime, text_input: &mut TextInput) {
		match ime {
			Ime::Enabled => {
			}
			Ime::Preedit(preedit_text, cursor_range) => {
				if preedit_text.is_empty() {
					self.finish_composition(text_input);
					return;
				}
				if !self.is_ime_composing {
					self.is_ime_composing = true;
					text_input.push_focused_field_event(TextInputEventKind::CompositionStart);
				}
				let cursor_index = cursor_range.map(|(cursor_byte_index, _)| convert_byte_index_to_character_index(&preedit_text, cursor_byte_index));
				text_input.push_focused_field_event(TextInputEventKind::Preedit {
					text: preedit_text,
					cursor_index,
				});
			}
			Ime::Commit(committed_text) => {
				self.finish_composition(text_input);
				if committed_text.is_empty() || text_input.consume_ime_commit_guard(&committed_text) {
					return;
				}
				text_input.push_focused_field_event(TextInputEventKind::Commit(committed_text));
			}
			Ime::Disabled => {
				self.finish_composition(text_input);
			}
		}
	}

	//==============================================================================
	/// 조합 끝내기. (조합 중이었으면 빈 조합 글과 조합 끝을 넣는다)
	//==============================================================================
	fn finish_composition(&mut self, text_input: &mut TextInput) {
		if !self.is_ime_composing {
			return;
		}
		self.is_ime_composing = false;
		text_input.push_focused_field_event(TextInputEventKind::Preedit {
			text: String::new(),
			cursor_index: None,
		});
		text_input.push_focused_field_event(TextInputEventKind::CompositionEnd);
	}

	//==============================================================================
	/// 지난 프레임 필드 상태를 창에 적용. (IME 켜고 끄기, 후보 창 자리. 프레임마다 화면을 넘기기 전에)
	//==============================================================================
	pub(super) fn apply_active_field(&mut self, window: &Window, text_input: &TextInput) {
		let active_field_state = text_input.get_active_field_state();
		let owner_field_id = active_field_state.map(|field_state| field_state.field_id).unwrap_or(0);
		let is_ime_wanted = active_field_state.map(|field_state| field_state.use_dominput).unwrap_or(false);

		// 다른 필드로 옮겨 가거나 필드가 조합 글을 스스로 확정했으면 껐다 켜서 조합을 끊는다.
		let is_ime_reset_requested = text_input.take_ime_reset_request();
		if (owner_field_id != self.ime_owner_field_id || is_ime_reset_requested) && self.is_ime_allowed {
			window.set_ime_allowed(false);
			self.is_ime_allowed = false;
			self.ime_cursor_area = None;
		}
		self.ime_owner_field_id = owner_field_id;
		if is_ime_wanted != self.is_ime_allowed {
			window.set_ime_allowed(is_ime_wanted);
			self.is_ime_allowed = is_ime_wanted;
			self.ime_cursor_area = None;
		}
		if !is_ime_wanted {
			return;
		}

		// 후보 창 자리. (커서 자리, 창 안쪽 논리 좌표)
		let caret_rect = match active_field_state {
			Some(field_state) => {
				field_state.caret_rect
			}
			None => {
				return;
			}
		};
		if self.ime_cursor_area == Some(caret_rect) {
			return;
		}
		self.ime_cursor_area = Some(caret_rect);
		let area_position = LogicalPosition::new(caret_rect.position.x as f64, caret_rect.position.y as f64);
		let area_size = LogicalSize::new(caret_rect.size.x.max(1.0) as f64, caret_rect.size.y.max(1.0) as f64);
		window.set_ime_cursor_area(area_position, area_size);
	}
}

//==============================================================================
/// 키 하나를 글 입력 이벤트로. (없으면 None)
/// - Ctrl(맥은 Cmd)만 눌렸으면 A/C/V/X 단축키(원본 handleCanvasKeydown)와 되돌리기, 다시 하기를 본다.
///   되돌리기와 다시 하기는 블링크 editing_behavior.cc 의 키 표다: Z 와 Shift + Z, 맥이 아니면 Y, 윈도우는 Alt + Backspace 와 Alt + Shift + Backspace.
/// - Ctrl 과 Alt 가 같이 눌린 것(AltGr)은 글자로 본다. Alt 만 눌린 글자는 버린다. (원본 !event.altKey)
/// - 글자는 제어 문자가 없을 때만. (Enter 의 "\r", Backspace 의 "\u{8}" 같은 것은 이름 있는 키로 먼저 걸러진다)
//==============================================================================
fn translate_key(logical_key: &Key, physical_key: PhysicalKey, text: Option<&str>, modifiers_state: ModifiersState) -> Option<TextInputEventKind> {
	let is_shift = modifiers_state.shift_key();
	let is_control = modifiers_state.control_key() || modifiers_state.super_key();
	let is_alt = modifiers_state.alt_key();
	if is_control && !is_alt {
		let shortcut_key = translate_shortcut_key(logical_key, physical_key, is_shift)?;
		return Some(TextInputEventKind::Key {
			key: shortcut_key,
			is_shift,
		});
	}
	if cfg!(target_os = "windows") && is_alt && !is_control && *logical_key == Key::Named(NamedKey::Backspace) {
		let history_key = if is_shift {
			TextInputKey::Redo
		}
		else {
			TextInputKey::Undo
		};
		return Some(TextInputEventKind::Key {
			key: history_key,
			is_shift,
		});
	}
	if let Key::Named(named_key) = logical_key {
		let edit_key = match named_key {
			NamedKey::Backspace => {
				Some(TextInputKey::Backspace)
			}
			NamedKey::Delete => {
				Some(TextInputKey::Delete)
			}
			NamedKey::ArrowLeft => {
				Some(TextInputKey::Left)
			}
			NamedKey::ArrowRight => {
				Some(TextInputKey::Right)
			}
			NamedKey::Home => {
				Some(TextInputKey::Home)
			}
			NamedKey::End => {
				Some(TextInputKey::End)
			}
			NamedKey::Enter => {
				Some(TextInputKey::Enter)
			}
			NamedKey::Tab => {
				Some(TextInputKey::Tab)
			}
			_ => {
				None
			}
		};
		if let Some(edit_key) = edit_key {
			return Some(TextInputEventKind::Key {
				key: edit_key,
				is_shift,
			});
		}
	}
	if is_alt && !is_control {
		return None;
	}
	let text = text?;
	if text.is_empty() || text.chars().any(|character| character.is_control()) {
		return None;
	}
	Some(TextInputEventKind::Commit(text.to_string()))
}

//==============================================================================
/// Ctrl/Cmd 단축키. (논리 키 글자를 먼저 보고, 글자가 아니면 물리 키 자리로)
//==============================================================================
fn translate_shortcut_key(logical_key: &Key, physical_key: PhysicalKey, is_shift: bool) -> Option<TextInputKey> {
	let undo_or_redo = if is_shift {
		TextInputKey::Redo
	}
	else {
		TextInputKey::Undo
	};
	let is_redo_y_enabled = !cfg!(target_os = "macos") && !is_shift;
	if let Key::Character(character_text) = logical_key {
		let lowercase_text = character_text.to_lowercase();
		let shortcut_key = match lowercase_text.as_str() {
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
			"z" => {
				Some(undo_or_redo)
			}
			"y" if is_redo_y_enabled => {
				Some(TextInputKey::Redo)
			}
			_ => {
				None
			}
		};
		if shortcut_key.is_some() {
			return shortcut_key;
		}
	}
	match physical_key {
		PhysicalKey::Code(KeyCode::KeyA) => {
			Some(TextInputKey::SelectAll)
		}
		PhysicalKey::Code(KeyCode::KeyC) => {
			Some(TextInputKey::Copy)
		}
		PhysicalKey::Code(KeyCode::KeyX) => {
			Some(TextInputKey::Cut)
		}
		PhysicalKey::Code(KeyCode::KeyV) => {
			Some(TextInputKey::Paste)
		}
		PhysicalKey::Code(KeyCode::KeyZ) => {
			Some(undo_or_redo)
		}
		PhysicalKey::Code(KeyCode::KeyY) if is_redo_y_enabled => {
			Some(TextInputKey::Redo)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 클립보드를 열어 일 하기. (열 수 없으면 None)
//==============================================================================
fn with_clipboard<R>(action: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
	CLIPBOARD.with(|clipboard_cell| {
		let mut clipboard_slot = clipboard_cell.borrow_mut();
		if clipboard_slot.is_none() {
			match arboard::Clipboard::new() {
				Ok(clipboard) => {
					*clipboard_slot = Some(clipboard);
				}
				Err(error) => {
					eprintln!("Engine: 클립보드 열기 실패: {}", error);
					return None;
				}
			}
		}
		clipboard_slot.as_mut().map(action)
	})
}

//==============================================================================
/// 시스템 클립보드에 글 쓰기.
//==============================================================================
pub fn write_clipboard_text(text: &str) {
	let result = with_clipboard(|clipboard| clipboard.set_text(text.to_string()));
	if let Some(Err(error)) = result {
		eprintln!("Engine: 클립보드 쓰기 실패: {}", error);
	}
}

//==============================================================================
/// 시스템 클립보드 글 읽기. (글이 없거나 읽을 수 없으면 None)
//==============================================================================
pub fn read_clipboard_text() -> Option<String> {
	with_clipboard(|clipboard| clipboard.get_text().ok()).flatten()
}

//==============================================================================
/// 시스템 클립보드 글을 읽어 콜백에 넘기기. (원본 readClipboard 자리, 데스크탑은 그 자리에서 읽어 바로 부른다. 못 읽으면 빈 글)
//==============================================================================
pub fn read_clipboard_text_with_callback(callback: Box<dyn FnOnce(String)>) {
	let text = read_clipboard_text().unwrap_or_default();
	callback(text);
}

//==============================================================================
/// 시스템 클립보드 글을 읽어 콜백에 넘기기. (원본 engine.readClipboardText 자리, 데스크탑은 그 자리에서 읽어 바로 부른다. 못 읽으면 None)
//==============================================================================
pub fn read_optional_clipboard_text_with_callback(callback: Box<dyn FnOnce(Option<String>)>) {
	callback(read_clipboard_text());
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 이름 있는 키, 단축키, 글자, 걸러지는 것.
	//==============================================================================
	#[test]
	fn translate_undo_keys() {
		let control = if cfg!(target_os = "macos") {
			ModifiersState::SUPER
		}
		else {
			ModifiersState::CONTROL
		};
		let undo = translate_key(&Key::Character("z".into()), PhysicalKey::Code(KeyCode::KeyZ), None, control);
		assert_eq!(undo, Some(TextInputEventKind::Key { key: TextInputKey::Undo, is_shift: false }));
		let redo = translate_key(&Key::Character("Z".into()), PhysicalKey::Code(KeyCode::KeyZ), None, control | ModifiersState::SHIFT);
		assert_eq!(redo, Some(TextInputEventKind::Key { key: TextInputKey::Redo, is_shift: true }));
		let redo_y = translate_key(&Key::Character("y".into()), PhysicalKey::Code(KeyCode::KeyY), None, control);
		if cfg!(target_os = "macos") {
			assert_eq!(redo_y, None);
		}
		else {
			assert_eq!(redo_y, Some(TextInputEventKind::Key { key: TextInputKey::Redo, is_shift: false }));
		}
		// 한글 자판에서도 물리 자리로 본다.
		let undo_by_position = translate_key(&Key::Character("ㅋ".into()), PhysicalKey::Code(KeyCode::KeyZ), None, control);
		assert_eq!(undo_by_position, Some(TextInputEventKind::Key { key: TextInputKey::Undo, is_shift: false }));
	}

	#[test]
	fn translate_keys() {
		let no_modifiers = ModifiersState::empty();
		let backspace = translate_key(&Key::Named(NamedKey::Backspace), PhysicalKey::Code(KeyCode::Backspace), Some("\u{8}"), no_modifiers);
		assert_eq!(backspace, Some(TextInputEventKind::Key {
			key: TextInputKey::Backspace,
			is_shift: false,
		}));
		let shift_left = translate_key(&Key::Named(NamedKey::ArrowLeft), PhysicalKey::Code(KeyCode::ArrowLeft), None, ModifiersState::SHIFT);
		assert_eq!(shift_left, Some(TextInputEventKind::Key {
			key: TextInputKey::Left,
			is_shift: true,
		}));
		let select_all = translate_key(&Key::Character("a".into()), PhysicalKey::Code(KeyCode::KeyA), Some("\u{1}"), ModifiersState::CONTROL);
		assert_eq!(select_all, Some(TextInputEventKind::Key {
			key: TextInputKey::SelectAll,
			is_shift: false,
		}));
		let paste_by_position = translate_key(&Key::Character("ㅍ".into()), PhysicalKey::Code(KeyCode::KeyV), None, ModifiersState::SUPER);
		assert_eq!(paste_by_position, Some(TextInputEventKind::Key {
			key: TextInputKey::Paste,
			is_shift: false,
		}));
		let character = translate_key(&Key::Character("A".into()), PhysicalKey::Code(KeyCode::KeyA), Some("A"), ModifiersState::SHIFT);
		assert_eq!(character, Some(TextInputEventKind::Commit(String::from("A"))));
		let alt_character = translate_key(&Key::Character("a".into()), PhysicalKey::Code(KeyCode::KeyA), Some("a"), ModifiersState::ALT);
		assert_eq!(alt_character, None);
		let escape = translate_key(&Key::Named(NamedKey::Escape), PhysicalKey::Code(KeyCode::Escape), Some("\u{1b}"), no_modifiers);
		assert_eq!(escape, None);
		let other_shortcut = translate_key(&Key::Character("q".into()), PhysicalKey::Code(KeyCode::KeyQ), None, ModifiersState::CONTROL);
		assert_eq!(other_shortcut, None);
	}

	//==============================================================================
	/// IME 조합 → 조합 시작, 조합 글(문자 자리 커서), 조합 끝, 확정. 가드에 걸린 확정은 버린다.
	//==============================================================================
	#[test]
	fn ime_events_become_composition() {
		let mut desktop_text_input_state = DesktopTextInputState::new();
		let mut text_input = TextInput::new();
		desktop_text_input_state.handle_ime_event(Ime::Preedit(String::from("한"), Some((3, 3))), &mut text_input);
		desktop_text_input_state.handle_ime_event(Ime::Preedit(String::new(), None), &mut text_input);
		desktop_text_input_state.handle_ime_event(Ime::Commit(String::from("한")), &mut text_input);
		text_input.set_ime_commit_guard("글");
		desktop_text_input_state.handle_ime_event(Ime::Commit(String::from("글")), &mut text_input);
		let event_kind_list = text_input.take_events_for_field(1, true);
		assert_eq!(event_kind_list, vec![
			TextInputEventKind::CompositionStart,
			TextInputEventKind::Preedit {
				text: String::from("한"),
				cursor_index: Some(1),
			},
			TextInputEventKind::Preedit {
				text: String::new(),
				cursor_index: None,
			},
			TextInputEventKind::CompositionEnd,
			TextInputEventKind::Commit(String::from("한")),
		]);
	}
}
