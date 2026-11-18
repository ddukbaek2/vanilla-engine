//==============================================================================
// 데스크탑 글 입력. (winit 0.30 창 이벤트 → 엔진 글 입력 줄, 시스템 클립보드)
// - 원본 UIInputField 의 DOM <input>(setUseDOMInput(true)) 자리와 document keydown(캔버스 모드) 자리를 함께 맡는다.
//   - KeyboardInput: 편집 키(Backspace, Delete, 화살표, Home, End, Enter, Tab), Ctrl/Cmd + A/C/V/X, KeyEvent.text 의 글자 → 글 입력 줄.
//   - Ime: Preedit 는 조합 시작 / 조합 글 / 조합 끝으로, Commit 은 확정 글로 바꾼다. (Preedit 커서는 바이트 자리라 문자 자리로 바꾼다)
//   - 프레임마다 지난 프레임 필드 상태를 읽어, 포커스를 가진 필드가 플랫폼 입력(useDOMInput)을 쓸 때만 IME 를 켜고 후보 창 자리를 맞춘다.
//     포커스가 다른 필드로 옮겨 가거나 조합 중에 필드 안을 눌러 필드가 조합 글을 확정하면 IME 를 껐다 켜서 조합을 끊는다.
//     (끊긴 조합 글은 필드가 이미 확정했으므로 OS 가 다시 보낸 같은 Commit 은 버린다)
// - 클립보드는 arboard 로 읽고 쓴다. (winit 에 클립보드가 없다. 리눅스는 Clipboard 객체가 살아 있어야 내용이 남아서 스레드에 하나 들고 있는다)
//==============================================================================
use crate::base::rect::Rect;
use crate::core::textinput::{convertByteIndexToCharacterIndex, TextInput, TextInputEventKind, TextInputKey};
use std::cell::RefCell;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::Window;

thread_local! {
	// 시스템 클립보드. (처음 쓸 때 연다)
	static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

//==============================================================================
// 데스크탑 글 입력 상태.
//==============================================================================
pub(super) struct DesktopTextInputState {
	modifiersState: ModifiersState,
	isImeComposing: bool,
	isImeAllowed: bool,
	imeOwnerFieldId: u64,
	imeCursorArea: Option<Rect>,
}

impl DesktopTextInputState {
	//==============================================================================
	// 생성. (IME 는 winit 기본처럼 꺼진 채로 시작한다)
	//==============================================================================
	pub(super) fn new() -> DesktopTextInputState {
		DesktopTextInputState {
			modifiersState: ModifiersState::empty(),
			isImeComposing: false,
			isImeAllowed: false,
			imeOwnerFieldId: 0,
			imeCursorArea: None,
		}
	}

	//==============================================================================
	// 수정 키 상태 갱신. (WindowEvent::ModifiersChanged)
	//==============================================================================
	pub(super) fn setModifiers(&mut self, modifiersState: ModifiersState) {
		self.modifiersState = modifiersState;
	}

	//==============================================================================
	// 키 이벤트 → 글 입력 줄. (눌림과 반복만)
	//==============================================================================
	pub(super) fn handleKeyEvent(&mut self, keyEvent: &KeyEvent, textInput: &mut TextInput) {
		if keyEvent.state != ElementState::Pressed {
			return;
		}
		let text = keyEvent.text.as_ref().map(|text| text.as_str());
		if let Some(eventKind) = translateKey(&keyEvent.logical_key, keyEvent.physical_key, text, self.modifiersState) {
			textInput.pushFocusedFieldEvent(eventKind);
		}
	}

	//==============================================================================
	// IME 이벤트 → 글 입력 줄.
	//==============================================================================
	pub(super) fn handleImeEvent(&mut self, ime: Ime, textInput: &mut TextInput) {
		match ime {
			Ime::Enabled => {
			}
			Ime::Preedit(preeditText, cursorRange) => {
				if preeditText.is_empty() {
					self.finishComposition(textInput);
					return;
				}
				if !self.isImeComposing {
					self.isImeComposing = true;
					textInput.pushFocusedFieldEvent(TextInputEventKind::compositionStart);
				}
				let cursorIndex = cursorRange.map(|(cursorByteIndex, _)| convertByteIndexToCharacterIndex(&preeditText, cursorByteIndex));
				textInput.pushFocusedFieldEvent(TextInputEventKind::preedit {
					text: preeditText,
					cursorIndex,
				});
			}
			Ime::Commit(committedText) => {
				self.finishComposition(textInput);
				if committedText.is_empty() || textInput.consumeImeCommitGuard(&committedText) {
					return;
				}
				textInput.pushFocusedFieldEvent(TextInputEventKind::commit(committedText));
			}
			Ime::Disabled => {
				self.finishComposition(textInput);
			}
		}
	}

	//==============================================================================
	// 조합 끝내기. (조합 중이었으면 빈 조합 글과 조합 끝을 넣는다)
	//==============================================================================
	fn finishComposition(&mut self, textInput: &mut TextInput) {
		if !self.isImeComposing {
			return;
		}
		self.isImeComposing = false;
		textInput.pushFocusedFieldEvent(TextInputEventKind::preedit {
			text: String::new(),
			cursorIndex: None,
		});
		textInput.pushFocusedFieldEvent(TextInputEventKind::compositionEnd);
	}

	//==============================================================================
	// 지난 프레임 필드 상태를 창에 적용. (IME 켜고 끄기, 후보 창 자리. 프레임마다 화면을 넘기기 전에)
	//==============================================================================
	pub(super) fn applyActiveField(&mut self, window: &Window, textInput: &TextInput) {
		let activeFieldState = textInput.getActiveFieldState();
		let ownerFieldId = activeFieldState.map(|fieldState| fieldState.fieldId).unwrap_or(0);
		let isImeWanted = activeFieldState.map(|fieldState| fieldState.useDOMInput).unwrap_or(false);

		// 다른 필드로 옮겨 가거나 필드가 조합 글을 스스로 확정했으면 껐다 켜서 조합을 끊는다.
		let isImeResetRequested = textInput.takeImeResetRequest();
		if (ownerFieldId != self.imeOwnerFieldId || isImeResetRequested) && self.isImeAllowed {
			window.set_ime_allowed(false);
			self.isImeAllowed = false;
			self.imeCursorArea = None;
		}
		self.imeOwnerFieldId = ownerFieldId;
		if isImeWanted != self.isImeAllowed {
			window.set_ime_allowed(isImeWanted);
			self.isImeAllowed = isImeWanted;
			self.imeCursorArea = None;
		}
		if !isImeWanted {
			return;
		}

		// 후보 창 자리. (커서 자리, 창 안쪽 논리 좌표)
		let caretRect = match activeFieldState {
			Some(fieldState) => {
				fieldState.caretRect
			}
			None => {
				return;
			}
		};
		if self.imeCursorArea == Some(caretRect) {
			return;
		}
		self.imeCursorArea = Some(caretRect);
		let areaPosition = LogicalPosition::new(caretRect.position.x as f64, caretRect.position.y as f64);
		let areaSize = LogicalSize::new(caretRect.size.x.max(1.0) as f64, caretRect.size.y.max(1.0) as f64);
		window.set_ime_cursor_area(areaPosition, areaSize);
	}
}

//==============================================================================
// 키 하나를 글 입력 이벤트로. (없으면 None)
// - Ctrl(맥은 Cmd)만 눌렸으면 A/C/V/X 단축키만 본다. (원본 handleCanvasKeydown)
// - Ctrl 과 Alt 가 같이 눌린 것(AltGr)은 글자로 본다. Alt 만 눌린 글자는 버린다. (원본 !event.altKey)
// - 글자는 제어 문자가 없을 때만. (Enter 의 "\r", Backspace 의 "\u{8}" 같은 것은 이름 있는 키로 먼저 걸러진다)
//==============================================================================
fn translateKey(logicalKey: &Key, physicalKey: PhysicalKey, text: Option<&str>, modifiersState: ModifiersState) -> Option<TextInputEventKind> {
	let isShift = modifiersState.shift_key();
	let isControl = modifiersState.control_key() || modifiersState.super_key();
	let isAlt = modifiersState.alt_key();
	if isControl && !isAlt {
		let shortcutKey = translateShortcutKey(logicalKey, physicalKey)?;
		return Some(TextInputEventKind::key {
			key: shortcutKey,
			isShift,
		});
	}
	if let Key::Named(namedKey) = logicalKey {
		let editKey = match namedKey {
			NamedKey::Backspace => {
				Some(TextInputKey::backspace)
			}
			NamedKey::Delete => {
				Some(TextInputKey::delete)
			}
			NamedKey::ArrowLeft => {
				Some(TextInputKey::left)
			}
			NamedKey::ArrowRight => {
				Some(TextInputKey::right)
			}
			NamedKey::Home => {
				Some(TextInputKey::home)
			}
			NamedKey::End => {
				Some(TextInputKey::end)
			}
			NamedKey::Enter => {
				Some(TextInputKey::enter)
			}
			NamedKey::Tab => {
				Some(TextInputKey::tab)
			}
			_ => {
				None
			}
		};
		if let Some(editKey) = editKey {
			return Some(TextInputEventKind::key {
				key: editKey,
				isShift,
			});
		}
	}
	if isAlt && !isControl {
		return None;
	}
	let text = text?;
	if text.is_empty() || text.chars().any(|character| character.is_control()) {
		return None;
	}
	Some(TextInputEventKind::commit(text.to_string()))
}

//==============================================================================
// Ctrl/Cmd 단축키. (논리 키 글자를 먼저 보고, 글자가 아니면 물리 키 자리로)
//==============================================================================
fn translateShortcutKey(logicalKey: &Key, physicalKey: PhysicalKey) -> Option<TextInputKey> {
	if let Key::Character(characterText) = logicalKey {
		let lowercaseText = characterText.to_lowercase();
		let shortcutKey = match lowercaseText.as_str() {
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
		if shortcutKey.is_some() {
			return shortcutKey;
		}
	}
	match physicalKey {
		PhysicalKey::Code(KeyCode::KeyA) => {
			Some(TextInputKey::selectAll)
		}
		PhysicalKey::Code(KeyCode::KeyC) => {
			Some(TextInputKey::copy)
		}
		PhysicalKey::Code(KeyCode::KeyX) => {
			Some(TextInputKey::cut)
		}
		PhysicalKey::Code(KeyCode::KeyV) => {
			Some(TextInputKey::paste)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
// 클립보드를 열어 일 하기. (열 수 없으면 None)
//==============================================================================
fn withClipboard<R>(action: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
	CLIPBOARD.with(|clipboardCell| {
		let mut clipboardSlot = clipboardCell.borrow_mut();
		if clipboardSlot.is_none() {
			match arboard::Clipboard::new() {
				Ok(clipboard) => {
					*clipboardSlot = Some(clipboard);
				}
				Err(error) => {
					eprintln!("Engine: 클립보드 열기 실패: {}", error);
					return None;
				}
			}
		}
		clipboardSlot.as_mut().map(action)
	})
}

//==============================================================================
// 시스템 클립보드에 글 쓰기.
//==============================================================================
pub fn writeClipboardText(text: &str) {
	let result = withClipboard(|clipboard| clipboard.set_text(text.to_string()));
	if let Some(Err(error)) = result {
		eprintln!("Engine: 클립보드 쓰기 실패: {}", error);
	}
}

//==============================================================================
// 시스템 클립보드 글 읽기. (글이 없거나 읽을 수 없으면 None)
//==============================================================================
pub fn readClipboardText() -> Option<String> {
	withClipboard(|clipboard| clipboard.get_text().ok()).flatten()
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 이름 있는 키, 단축키, 글자, 걸러지는 것.
	//==============================================================================
	#[test]
	fn translateKeys() {
		let noModifiers = ModifiersState::empty();
		let backspace = translateKey(&Key::Named(NamedKey::Backspace), PhysicalKey::Code(KeyCode::Backspace), Some("\u{8}"), noModifiers);
		assert_eq!(backspace, Some(TextInputEventKind::key {
			key: TextInputKey::backspace,
			isShift: false,
		}));
		let shiftLeft = translateKey(&Key::Named(NamedKey::ArrowLeft), PhysicalKey::Code(KeyCode::ArrowLeft), None, ModifiersState::SHIFT);
		assert_eq!(shiftLeft, Some(TextInputEventKind::key {
			key: TextInputKey::left,
			isShift: true,
		}));
		let selectAll = translateKey(&Key::Character("a".into()), PhysicalKey::Code(KeyCode::KeyA), Some("\u{1}"), ModifiersState::CONTROL);
		assert_eq!(selectAll, Some(TextInputEventKind::key {
			key: TextInputKey::selectAll,
			isShift: false,
		}));
		let pasteByPosition = translateKey(&Key::Character("ㅍ".into()), PhysicalKey::Code(KeyCode::KeyV), None, ModifiersState::SUPER);
		assert_eq!(pasteByPosition, Some(TextInputEventKind::key {
			key: TextInputKey::paste,
			isShift: false,
		}));
		let character = translateKey(&Key::Character("A".into()), PhysicalKey::Code(KeyCode::KeyA), Some("A"), ModifiersState::SHIFT);
		assert_eq!(character, Some(TextInputEventKind::commit(String::from("A"))));
		let altCharacter = translateKey(&Key::Character("a".into()), PhysicalKey::Code(KeyCode::KeyA), Some("a"), ModifiersState::ALT);
		assert_eq!(altCharacter, None);
		let escape = translateKey(&Key::Named(NamedKey::Escape), PhysicalKey::Code(KeyCode::Escape), Some("\u{1b}"), noModifiers);
		assert_eq!(escape, None);
		let otherShortcut = translateKey(&Key::Character("z".into()), PhysicalKey::Code(KeyCode::KeyZ), None, ModifiersState::CONTROL);
		assert_eq!(otherShortcut, None);
	}

	//==============================================================================
	// IME 조합 → 조합 시작, 조합 글(문자 자리 커서), 조합 끝, 확정. 가드에 걸린 확정은 버린다.
	//==============================================================================
	#[test]
	fn imeEventsBecomeComposition() {
		let mut desktopTextInputState = DesktopTextInputState::new();
		let mut textInput = TextInput::new();
		desktopTextInputState.handleImeEvent(Ime::Preedit(String::from("한"), Some((3, 3))), &mut textInput);
		desktopTextInputState.handleImeEvent(Ime::Preedit(String::new(), None), &mut textInput);
		desktopTextInputState.handleImeEvent(Ime::Commit(String::from("한")), &mut textInput);
		textInput.setImeCommitGuard("글");
		desktopTextInputState.handleImeEvent(Ime::Commit(String::from("글")), &mut textInput);
		let eventKindList = textInput.takeEventsForField(1, true);
		assert_eq!(eventKindList, vec![
			TextInputEventKind::compositionStart,
			TextInputEventKind::preedit {
				text: String::from("한"),
				cursorIndex: Some(1),
			},
			TextInputEventKind::preedit {
				text: String::new(),
				cursorIndex: None,
			},
			TextInputEventKind::compositionEnd,
			TextInputEventKind::commit(String::from("한")),
		]);
	}
}
