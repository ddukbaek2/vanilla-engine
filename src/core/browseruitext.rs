//==============================================================================
// 브라우저 화면 글. (러스트 판에만 있다, 원본은 브라우저가 제 화면 언어로 그린다)
// - 원본 페이지 위에 브라우저가 그리는 것(자바스크립트 질문 창의 제목과 단추, 편집 칸 문맥 메뉴의 항목)의 글이다.
//   데스크탑은 엔진이 그것을 그리므로 같은 글이 필요하다. (core/modaldialog.rs, ui/uiinputfield.rs)
// - 글은 크로미엄의 번역(chrome/app/resources/generated_resources_*.xtb, ui/strings/translations/ui_strings_*.xtb,
//   components/strings/components_strings_*.xtb)을 grit 의 메시지 번호로 찾아 화면 꼴로 옮긴 표다. (browseruitext.json, 데스크탑 로케일 82 개)
//   크롬 Views 메뉴는 단축 글자 표시 "(&U)" 와 "&" 를 지워 그리고(gfx RemoveAcceleratorChar), 맥은 FixUpWindowsStyleLabel 로 지운다. 표는 지운 글이다.
// - 크롬은 제 화면 언어로 그린다. 데스크탑은 그 자리인 시스템 선호 언어(sys-locale) 차례에서 크롬 규칙으로 로케일을 고른다.
//   (l10n_util 의 GetApplicationLocale 과 language_tag_matcher: 같은 이름이 있으면 그것, es-* 는 es-419, pt 는 pt-BR, zh 는 zh-CN, zh-Hant 는 zh-TW,
//   en 과 en-PH, en-LR 은 en-US, en-CA 와 그 밖의 en-* 는 en-GB, 옛 이름 iw, no, tl, in 은 he, nb, fil, id. 맞는 것이 없으면 en-US)
//   zh-MO 는 CLDR 의 부모 로케일 zh-Hant-HK 를 따라 zh-HK, zh-SG 는 zh-CN 으로 본다. (크롬의 ICU4X 데이터로 돌려 보지는 않았다)
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
	// 붙여넣기. (윈도우, 리눅스)
	Paste,
	// 붙여넣기. (맥, 번역이 다를 수 있다)
	PasteMac,
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
			BrowserUiText::PasteMac => {
				"paste_mac"
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
// 언어 태그 목록에서 표의 로케일 고르기. (차례대로 처음 맞는 것, 없으면 en-US)
//==============================================================================
fn select_locale(locale_map: &Map<String, Value>, language_tag_list: &[String]) -> String {
	for language_tag in language_tag_list {
		if let Some(locale) = match_locale(locale_map, language_tag) {
			return locale;
		}
	}
	String::from(DEFAULT_LOCALE)
}

//==============================================================================
// 언어 태그 하나를 표의 로케일로. (크롬 규칙, 맞는 것이 없으면 None)
//==============================================================================
fn match_locale(locale_map: &Map<String, Value>, language_tag: &str) -> Option<String> {
	let normalized_tag = language_tag.trim().replace('_', "-");
	let find_locale = |candidate: &str| -> Option<String> {
		locale_map.keys().find(|locale| locale.eq_ignore_ascii_case(candidate)).cloned()
	};
	if let Some(locale) = find_locale(&normalized_tag) {
		return Some(locale);
	}
	let subtag_list: Vec<String> = normalized_tag.split('-').map(|subtag| subtag.to_ascii_lowercase()).collect();
	let language = subtag_list.first().cloned().unwrap_or_default();
	let has_subtag = |wanted_subtag: &str| subtag_list.iter().skip(1).any(|subtag| subtag == wanted_subtag);
	let candidate = match language.as_str() {
		"es" => {
			if subtag_list.len() > 1 && !has_subtag("es") {
				"es-419"
			}
			else {
				"es"
			}
		}
		"pt" => {
			if subtag_list.len() == 1 || has_subtag("br") {
				"pt-BR"
			}
			else {
				"pt-PT"
			}
		}
		"zh" => {
			if has_subtag("hk") {
				"zh-HK"
			}
			else if has_subtag("mo") {
				"zh-HK"
			}
			else if has_subtag("hant") || has_subtag("tw") {
				"zh-TW"
			}
			else {
				"zh-CN"
			}
		}
		"en" => {
			let us_region_list = ["us", "ph", "lr", "pr", "vi", "as", "gu", "um", "mp"];
			if subtag_list.len() == 1 || subtag_list.iter().skip(1).any(|subtag| us_region_list.contains(&subtag.as_str())) {
				"en-US"
			}
			else {
				"en-GB"
			}
		}
		"iw" | "he" => {
			"he"
		}
		"no" | "nb" | "nn" => {
			"nb"
		}
		"tl" | "fil" => {
			"fil"
		}
		"in" | "id" => {
			"id"
		}
		_ => {
			language.as_str()
		}
	};
	find_locale(candidate)
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
		assert_eq!(select_locale(&locale_map, &[String::from("es-MX")]), "es-419");
		assert_eq!(select_locale(&locale_map, &[String::from("es_ES")]), "es");
		assert_eq!(select_locale(&locale_map, &[String::from("pt")]), "pt-BR");
		assert_eq!(select_locale(&locale_map, &[String::from("pt-PT")]), "pt-PT");
		assert_eq!(select_locale(&locale_map, &[String::from("zh-Hant-TW")]), "zh-TW");
		assert_eq!(select_locale(&locale_map, &[String::from("zh-Hans")]), "zh-CN");
		assert_eq!(select_locale(&locale_map, &[String::from("zh-HK")]), "zh-HK");
		assert_eq!(select_locale(&locale_map, &[String::from("en-CA")]), "en-GB");
		assert_eq!(select_locale(&locale_map, &[String::from("en-AU")]), "en-GB");
		assert_eq!(select_locale(&locale_map, &[String::from("en")]), "en-US");
		assert_eq!(select_locale(&locale_map, &[String::from("iw-IL")]), "he");
		assert_eq!(select_locale(&locale_map, &[String::from("no")]), "nb");
		assert_eq!(select_locale(&locale_map, &[String::from("ja-JP")]), "ja");
		assert_eq!(locale_map.len(), 82);
		let english_map = locale_map.get("en-US").and_then(|text_map| text_map.as_object()).expect("en-US");
		for text in [BrowserUiText::Undo, BrowserUiText::Redo, BrowserUiText::Cut, BrowserUiText::Copy, BrowserUiText::Paste, BrowserUiText::PasteMac, BrowserUiText::PastePlainText, BrowserUiText::PasteMatchStyle, BrowserUiText::SelectAll, BrowserUiText::SelectAllMac, BrowserUiText::Emoji, BrowserUiText::EmojiMac, BrowserUiText::WritingDirection, BrowserUiText::WritingDirectionDefault, BrowserUiText::WritingDirectionLeftToRight, BrowserUiText::WritingDirectionRightToLeft, BrowserUiText::Inspect, BrowserUiText::PageSays, BrowserUiText::Ok, BrowserUiText::Cancel, BrowserUiText::PeriodKey, BrowserUiText::WindowsKey] {
			assert!(english_map.contains_key(text.get_key()), "{:?}", text);
		}
		assert!(!read_browser_ui_text(BrowserUiText::Ok).is_empty());
		assert!(!read_browser_ui_locale().is_empty());
	}
}
