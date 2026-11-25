//==============================================================================
// 브라우저 화면 글. (러스트 판에만 있다, 원본은 브라우저가 제 화면 언어로 그린다)
// - 원본 페이지 위에 브라우저가 그리는 것(자바스크립트 질문 창의 제목과 단추, 편집 칸 문맥 메뉴의 항목)의 글이다.
//   데스크탑은 엔진이 그것을 그리므로 같은 글이 필요하다. (core/modaldialog.rs, ui/uiinputfield.rs)
// - 글은 크로미엄의 번역(generated_resources, ui_strings, components_strings 의 xtb)을 화면 꼴로 옮긴 표다. (browseruitext.json)
//   크롬 Views 메뉴는 단축 글자 표시 "(&U)" 와 "&" 를 지워 그리고, 맥은 FixUpWindowsStyleLabel 로 같은 것을 지운다. 표는 지운 글이다.
// - 크롬은 제 화면 언어로 그린다. 데스크탑은 그 자리인 시스템 선호 언어(sys-locale)에서 표에 있는 로케일을 고른다.
//   웹은 브라우저가 그리므로 이 표를 쓰지 않는다.
//==============================================================================
use serde_json::{Map, Value};
use std::cell::RefCell;

// 번역 표. (로케일 → 키 → 글)
const BROWSER_UI_TEXT_JSON: &str = include_str!("browseruitext.json");
// 표에 없을 때의 로케일.
const DEFAULT_LOCALE: &str = "en-US";

//==============================================================================
// 글 종류. (표의 키)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserUiText {
	// 실행 취소.
	Undo,
	// 다시 실행.
	Redo,
	// 잘라내기.
	Cut,
	// 복사.
	Copy,
	// 붙여넣기.
	Paste,
	// 일반 텍스트로 붙여넣기. (윈도우, 리눅스)
	PastePlainText,
	// 붙여넣고 스타일 일치시킴. (맥)
	PasteMatchStyle,
	// 전체 선택. (윈도우, 리눅스)
	SelectAll,
	// 모두 선택. (맥)
	SelectAllMac,
	// 그림 이모티콘. (윈도우)
	Emoji,
	// 그림 이모티콘 및 기호. (맥)
	EmojiMac,
	// 쓰기 방향 하위 메뉴.
	WritingDirection,
	// 쓰기 방향 기본값.
	WritingDirectionDefault,
	// 왼쪽에서 오른쪽으로.
	WritingDirectionLeftToRight,
	// 오른쪽에서 왼쪽으로.
	WritingDirectionRightToLeft,
	// 검사.
	Inspect,
	// 질문 창 제목. (출처가 없는 페이지의 "이 페이지 내용:")
	PageSays,
	// 확인.
	Ok,
	// 취소.
	Cancel,
	// 단축키 글의 마침표 키 이름.
	PeriodKey,
	// 단축키 글의 윈도우 키 이름.
	WindowsKey,
}

impl BrowserUiText {
	//==============================================================================
	// 표의 키.
	//==============================================================================
	fn get_key(self) -> &'static str {
		match self {
			BrowserUiText::Undo => {
				"undo"
			}
			BrowserUiText::Redo => {
				"redo"
			}
			BrowserUiText::Cut => {
				"cut"
			}
			BrowserUiText::Copy => {
				"copy"
			}
			BrowserUiText::Paste => {
				"paste"
			}
			BrowserUiText::PastePlainText => {
				"paste_plain_text"
			}
			BrowserUiText::PasteMatchStyle => {
				"paste_match_style"
			}
			BrowserUiText::SelectAll => {
				"select_all"
			}
			BrowserUiText::SelectAllMac => {
				"select_all_mac"
			}
			BrowserUiText::Emoji => {
				"emoji"
			}
			BrowserUiText::EmojiMac => {
				"emoji_mac"
			}
			BrowserUiText::WritingDirection => {
				"writing_direction"
			}
			BrowserUiText::WritingDirectionDefault => {
				"writing_direction_default"
			}
			BrowserUiText::WritingDirectionLeftToRight => {
				"writing_direction_ltr"
			}
			BrowserUiText::WritingDirectionRightToLeft => {
				"writing_direction_rtl"
			}
			BrowserUiText::Inspect => {
				"inspect"
			}
			BrowserUiText::PageSays => {
				"page_says"
			}
			BrowserUiText::Ok => {
				"ok"
			}
			BrowserUiText::Cancel => {
				"cancel"
			}
			BrowserUiText::PeriodKey => {
				"period_key"
			}
			BrowserUiText::WindowsKey => {
				"windows_key"
			}
		}
	}
}

//==============================================================================
// 읽은 표와 고른 로케일. (스레드마다 처음 쓸 때)
//==============================================================================
struct BrowserUiTextState {
	locale_map: Map<String, Value>,
	locale: String,
}

thread_local! {
	static BROWSER_UI_TEXT_STATE: RefCell<Option<BrowserUiTextState>> = const { RefCell::new(None) };
}

//==============================================================================
// 표 다루기. (처음이면 읽고 시스템 언어로 로케일을 고른다)
//==============================================================================
fn with_state<R>(worker: impl FnOnce(&BrowserUiTextState) -> R) -> R {
	BROWSER_UI_TEXT_STATE.with(|state_cell| {
		let mut state = state_cell.borrow_mut();
		let state = state.get_or_insert_with(|| {
			let locale_map = match serde_json::from_str::<Value>(BROWSER_UI_TEXT_JSON) {
				Ok(Value::Object(locale_map)) => {
					locale_map
				}
				_ => {
					Map::new()
				}
			};
			let locale = select_locale(&locale_map, &crate::misc::localization::read_preferred_language_tags());
			BrowserUiTextState {
				locale_map,
				locale,
			}
		});
		worker(state)
	})
}

//==============================================================================
// 언어 태그 목록에서 표의 로케일 고르기. (태그 그대로, 그다음 언어 부분, 없으면 en-US)
//==============================================================================
fn select_locale(locale_map: &Map<String, Value>, language_tag_list: &[String]) -> String {
	for language_tag in language_tag_list {
		let normalized_tag = language_tag.trim().replace('_', "-");
		for (locale, _) in locale_map.iter() {
			if locale.eq_ignore_ascii_case(&normalized_tag) {
				return locale.clone();
			}
		}
		let language = normalized_tag.split('-').next().unwrap_or("").to_ascii_lowercase();
		for (locale, _) in locale_map.iter() {
			if locale.eq_ignore_ascii_case(&language) {
				return locale.clone();
			}
		}
	}
	String::from(DEFAULT_LOCALE)
}

//==============================================================================
// 브라우저 화면 글. (고른 로케일, 그 로케일에 없으면 en-US)
//==============================================================================
pub fn read_browser_ui_text(text: BrowserUiText) -> String {
	with_state(|state| {
		let key = text.get_key();
		for locale in [state.locale.as_str(), DEFAULT_LOCALE] {
			if let Some(found_text) = state.locale_map.get(locale).and_then(|text_map| text_map.get(key)).and_then(|found_text| found_text.as_str()) {
				return found_text.to_string();
			}
		}
		String::new()
	})
}

//==============================================================================
// 고른 로케일.
//==============================================================================
pub fn read_browser_ui_locale() -> String {
	with_state(|state| state.locale.clone())
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 로케일 고르기와 모든 키가 en-US 에 있는지.
	//==============================================================================
	#[test]
	fn selects_locale_and_has_all_texts() {
		let locale_map = match serde_json::from_str::<Value>(BROWSER_UI_TEXT_JSON) {
			Ok(Value::Object(locale_map)) => {
				locale_map
			}
			_ => {
				panic!("표를 읽지 못했다");
			}
		};
		assert_eq!(select_locale(&locale_map, &[String::from("ko-KR")]), "ko");
		assert_eq!(select_locale(&locale_map, &[String::from("xx"), String::from("ko")]), "ko");
		assert_eq!(select_locale(&locale_map, &[String::from("xx")]), "en-US");
		let english_map = locale_map.get("en-US").and_then(|text_map| text_map.as_object()).expect("en-US");
		for text in [BrowserUiText::Undo, BrowserUiText::Redo, BrowserUiText::Cut, BrowserUiText::Copy, BrowserUiText::Paste, BrowserUiText::PastePlainText, BrowserUiText::PasteMatchStyle, BrowserUiText::SelectAll, BrowserUiText::SelectAllMac, BrowserUiText::Emoji, BrowserUiText::EmojiMac, BrowserUiText::WritingDirection, BrowserUiText::WritingDirectionDefault, BrowserUiText::WritingDirectionLeftToRight, BrowserUiText::WritingDirectionRightToLeft, BrowserUiText::Inspect, BrowserUiText::PageSays, BrowserUiText::Ok, BrowserUiText::Cancel, BrowserUiText::PeriodKey, BrowserUiText::WindowsKey] {
			assert!(english_map.contains_key(text.get_key()), "{:?}", text);
		}
		assert!(!read_browser_ui_text(BrowserUiText::Ok).is_empty());
		assert!(!read_browser_ui_locale().is_empty());
	}
}
