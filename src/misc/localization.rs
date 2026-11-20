//==============================================================================
// 로컬라이제이션.
// - 원본 misc/localization.js 를 옮긴 것이다. 언어 자동 감지, 번역 테이블, {0}{1} 자리 표시자 바꾸기, 폴백을 한 곳에 모았다.
// - 키는 원문 그대로 써도 된다. 표에 없으면 원문(키)을 그대로 돌려주므로 조금씩 번역해 나갈 수 있다.
// - 사용:
//     Localization::set_table("en", [("확인", "OK"), ("{0}점 획득", "Got {0} points")]);
//     Localization::set_language(&Localization::detect_language(&["ko", "en"], "ko"));
//     Localization::text("{0}점 획득", &[&120]);
// - 원본과 다른 점:
//   원본은 클래스의 정적 필드에 표를 든다. 러스트 판은 스레드 하나의 전역 상태로 든다. (엔진은 한 스레드에서 돈다)
//   표의 값은 글 하나 또는 글 목록(LocalizationValue)이다. JSON 표(JsonAsset 으로 읽은 것)는 set_table_from_json 으로 넣는다.
//   text 의 가변 인자는 Display 참조 목록(&[&dyn Display])이다.
//   detect_language 는 웹에서 원본처럼 navigator.languages 를 읽고, 데스크탑에서는 LANGUAGE / LC_ALL / LC_MESSAGES / LANG 환경 변수를 읽는다.
//     (윈도우는 보통 이 변수가 없어서 기본 언어가 된다)
//==============================================================================
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;

//==============================================================================
// 번역 값. (원본 표의 값: 문자열 또는 문자열 배열)
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalizationValue {
	Text(String),
	TextList(Vec<String>),
}

impl From<&str> for LocalizationValue {
	//==============================================================================
	// 글 하나로.
	//==============================================================================
	fn from(text: &str) -> LocalizationValue {
		LocalizationValue::Text(text.to_string())
	}
}

impl From<String> for LocalizationValue {
	//==============================================================================
	// 글 하나로.
	//==============================================================================
	fn from(text: String) -> LocalizationValue {
		LocalizationValue::Text(text)
	}
}

impl From<Vec<String>> for LocalizationValue {
	//==============================================================================
	// 글 목록으로.
	//==============================================================================
	fn from(text_list: Vec<String>) -> LocalizationValue {
		LocalizationValue::TextList(text_list)
	}
}

impl From<Vec<&str>> for LocalizationValue {
	//==============================================================================
	// 글 목록으로.
	//==============================================================================
	fn from(text_list: Vec<&str>) -> LocalizationValue {
		LocalizationValue::TextList(text_list.into_iter().map(|text| text.to_string()).collect())
	}
}

// 번역 표. (키(원문) → 번역 값)
pub type LocalizationTable = HashMap<String, LocalizationValue>;

//==============================================================================
// 로컬라이제이션 전역 상태. (원본 정적 필드 #table_by_language, #language_code, #fallback_language_code)
//==============================================================================
struct LocalizationState {
	table_by_language: HashMap<String, LocalizationTable>,
	language_code: String,
	fallback_language_code: String,
}

thread_local! {
	static LOCALIZATION_STATE: RefCell<Option<LocalizationState>> = const { RefCell::new(None) };
}

//==============================================================================
// 전역 상태 읽고 쓰기. (처음 쓸 때 언어 "ko", 폴백 "ko" 로 만든다)
//==============================================================================
fn with_state<R>(visitor: impl FnOnce(&mut LocalizationState) -> R) -> R {
	LOCALIZATION_STATE.with(|state_cell| {
		let mut state_option = state_cell.borrow_mut();
		let state = state_option.get_or_insert_with(|| LocalizationState {
			table_by_language: HashMap::new(),
			language_code: String::from("ko"),
			fallback_language_code: String::from("ko"),
		});
		visitor(state)
	})
}

//==============================================================================
// 로컬라이제이션. (정적 함수 묶음)
//==============================================================================
pub struct Localization;

impl Localization {
	//==============================================================================
	// 사용자 언어 감지. (선호 언어 순서대로 지원 목록에 있는 첫 언어, 없으면 기본 언어)
	// - "zh-TW" 같은 지역 표기는 "zh-tw" → "zh" 순으로 대조한다.
	//==============================================================================
	pub fn detect_language(supported_language_codes: &[&str], default_language_code: &str) -> String {
		let candidate_list = read_preferred_language_tags();
		Localization::select_language(&candidate_list, supported_language_codes, default_language_code)
	}

	//==============================================================================
	// 선호 언어 목록에서 지원하는 첫 언어 고르기. (detect_language 의 대조 부분)
	//==============================================================================
	pub fn select_language(candidate_list: &[String], supported_language_codes: &[&str], default_language_code: &str) -> String {
		let supported_set: HashSet<String> = supported_language_codes.iter().map(|code| code.to_lowercase()).collect();
		for language_tag in candidate_list {
			let lowered_tag = language_tag.to_lowercase();
			if supported_set.contains(&lowered_tag) {
				return lowered_tag;
			}
			let primary_code = lowered_tag.split('-').next().unwrap_or("").to_string();
			if supported_set.contains(&primary_code) {
				return primary_code;
			}
		}
		default_language_code.to_string()
	}

	//==============================================================================
	// 번역 표 설정. (언어 코드는 소문자로 바꿔 든다. 같은 언어 표가 있으면 바꾼다)
	//==============================================================================
	pub fn set_table<K, V>(language_code: &str, table: impl IntoIterator<Item = (K, V)>)
	where
		K: Into<String>,
		V: Into<LocalizationValue>,
	{
		let localization_table: LocalizationTable = table.into_iter().map(|(key, value)| (key.into(), value.into())).collect();
		let lowered_language_code = language_code.to_lowercase();
		with_state(|state| {
			state.table_by_language.insert(lowered_language_code, localization_table);
		});
	}

	//==============================================================================
	// JSON 객체로 번역 표 설정. (값이 글이면 글, 글 배열이면 목록. 그 밖의 값은 건너뛴다)
	//==============================================================================
	pub fn set_table_from_json(language_code: &str, table_json: &serde_json::Value) {
		let mut localization_table = LocalizationTable::new();
		if let Some(object_map) = table_json.as_object() {
			for (key, value) in object_map {
				match value {
					serde_json::Value::String(text) => {
						localization_table.insert(key.clone(), LocalizationValue::Text(text.clone()));
					}
					serde_json::Value::Array(items) => {
						let text_list: Vec<String> = items.iter().filter_map(|item| item.as_str().map(|text| text.to_string())).collect();
						localization_table.insert(key.clone(), LocalizationValue::TextList(text_list));
					}
					_ => {
					}
				}
			}
		}
		Localization::set_table(language_code, localization_table);
	}

	//==============================================================================
	// 지금 언어 설정. (소문자로)
	//==============================================================================
	pub fn set_language(language_code: &str) {
		let lowered_language_code = language_code.to_lowercase();
		with_state(|state| {
			state.language_code = lowered_language_code;
		});
	}

	//==============================================================================
	// 지금 언어 반환.
	//==============================================================================
	pub fn get_language() -> String {
		with_state(|state| state.language_code.clone())
	}

	//==============================================================================
	// 폴백 언어 설정. (지금 언어 표에 없으면 이 언어 표를 본다)
	//==============================================================================
	pub fn set_fallback_language(language_code: &str) {
		let lowered_language_code = language_code.to_lowercase();
		with_state(|state| {
			state.fallback_language_code = lowered_language_code;
		});
	}

	//==============================================================================
	// 번역문 찾기. (지금 언어 → 폴백 언어 → 키 원문 순, {0} {1} ... 을 인자로 바꾼다)
	// - 값이 목록이면 원본처럼 키를 쓴다.
	//==============================================================================
	pub fn text(key: &str, substitutions: &[&dyn Display]) -> String {
		let mut resolved_text = match Localization::lookup(key) {
			Some(LocalizationValue::Text(text)) => {
				text
			}
			_ => {
				key.to_string()
			}
		};
		for (substitution_index, substitution) in substitutions.iter().enumerate() {
			let placeholder = format!("{{{}}}", substitution_index);
			let substitution_text = substitution.to_string();
			resolved_text = resolved_text.replace(&placeholder, &substitution_text);
		}
		resolved_text
	}

	//==============================================================================
	// 목록형 번역 찾기. (대사 목록처럼 여러 줄인 항목. 글 하나면 한 줄짜리 목록, 없으면 [키])
	//==============================================================================
	pub fn text_list(key: &str) -> Vec<String> {
		match Localization::lookup(key) {
			Some(LocalizationValue::TextList(text_list)) => {
				text_list
			}
			Some(LocalizationValue::Text(text)) => {
				vec![text]
			}
			None => {
				vec![key.to_string()]
			}
		}
	}

	//==============================================================================
	// 값 그대로 찾기. (지금 언어 → 폴백 언어, 없으면 None)
	//==============================================================================
	pub fn lookup(key: &str) -> Option<LocalizationValue> {
		with_state(|state| {
			if let Some(current_value) = state.table_by_language.get(&state.language_code).and_then(|current_table| current_table.get(key)) {
				return Some(current_value.clone());
			}
			state.table_by_language.get(&state.fallback_language_code).and_then(|fallback_table| fallback_table.get(key)).cloned()
		})
	}

	//==============================================================================
	// 모든 표 비우기. (시험용, 언어 설정은 그대로)
	//==============================================================================
	pub fn clear() {
		with_state(|state| {
			state.table_by_language.clear();
		});
	}
}

//==============================================================================
// 사용자 선호 언어 목록. (웹: navigator.languages, 없으면 navigator.language)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_preferred_language_tags() -> Vec<String> {
	let mut candidate_list: Vec<String> = Vec::new();
	let navigator = match web_sys::window() {
		Some(window) => {
			window.navigator()
		}
		None => {
			return candidate_list;
		}
	};
	let language_array = navigator.languages();
	for language_index in 0..language_array.length() {
		if let Some(language_tag) = language_array.get(language_index).as_string() {
			candidate_list.push(language_tag);
		}
	}
	if candidate_list.is_empty() {
		if let Some(language_tag) = navigator.language() {
			candidate_list.push(language_tag);
		}
	}
	candidate_list
}

//==============================================================================
// 사용자 선호 언어 목록. (데스크탑: LANGUAGE(콜론으로 여럿) → LC_ALL → LC_MESSAGES → LANG, "ko_KR.UTF-8" 은 "ko-KR" 로)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_preferred_language_tags() -> Vec<String> {
	let mut candidate_list: Vec<String> = Vec::new();
	for variable_name in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
		let variable_value = match std::env::var(variable_name) {
			Ok(variable_value) => {
				variable_value
			}
			Err(_) => {
				continue;
			}
		};
		for locale_name in variable_value.split(':') {
			let language_tag = convert_locale_to_language_tag(locale_name);
			if !language_tag.is_empty() && !candidate_list.contains(&language_tag) {
				candidate_list.push(language_tag);
			}
		}
	}
	candidate_list
}

//==============================================================================
// POSIX 로캘 이름 → 언어 태그. ("ko_KR.UTF-8@euro" → "ko-KR", "C" / "POSIX" 는 빈 글)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn convert_locale_to_language_tag(locale_name: &str) -> String {
	let without_modifier = locale_name.split('@').next().unwrap_or("");
	let without_encoding = without_modifier.split('.').next().unwrap_or("").trim();
	if without_encoding.is_empty() || without_encoding == "C" || without_encoding == "POSIX" {
		return String::new();
	}
	without_encoding.replace('_', "-")
}

//==============================================================================
// 시험. (전역 상태가 스레드마다 따로라 시험끼리 섞이지 않는다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 지금 언어 → 폴백 → 키, 자리 표시자, 목록.
	//==============================================================================
	#[test]
	fn lookup_fallback_and_substitution() {
		Localization::clear();
		Localization::set_table("EN", [("확인", "OK"), ("{0}점 획득", "Got {0} points"), ("{0}/{1}", "{1} of {0}")]);
		Localization::set_table("ko", [("only.korean", LocalizationValue::from("한국어만")), ("intro", LocalizationValue::from(vec!["첫 줄", "둘째 줄"]))]);
		Localization::set_language("En");
		assert_eq!(Localization::get_language(), "en");
		assert_eq!(Localization::text("확인", &[]), "OK");
		assert_eq!(Localization::text("{0}점 획득", &[&120]), "Got 120 points");
		assert_eq!(Localization::text("{0}/{1}", &[&3, &"열"]), "열 of 3");
		assert_eq!(Localization::text("only.korean", &[]), "한국어만");
		assert_eq!(Localization::text("없는 키 {0}", &[&1.5]), "없는 키 1.5");
		assert_eq!(Localization::text("intro", &[]), "intro");
		assert_eq!(Localization::text_list("intro"), vec!["첫 줄", "둘째 줄"]);
		assert_eq!(Localization::text_list("확인"), vec!["OK"]);
		assert_eq!(Localization::text_list("none"), vec!["none"]);

		Localization::set_fallback_language("en");
		Localization::set_language("ja");
		assert_eq!(Localization::text("확인", &[]), "OK");
		assert_eq!(Localization::lookup("only.korean"), None);
	}

	//==============================================================================
	// JSON 표와 언어 고르기.
	//==============================================================================
	#[test]
	fn json_table_and_language_selection() {
		Localization::set_table_from_json("fr", &serde_json::json!({ "확인": "D'accord", "lines": ["a", "b"], "number": 3 }));
		Localization::set_language("fr");
		assert_eq!(Localization::text("확인", &[]), "D'accord");
		assert_eq!(Localization::text_list("lines"), vec!["a", "b"]);
		assert_eq!(Localization::text("number", &[]), "number");

		let candidate_list = vec![String::from("zh-TW"), String::from("en-US")];
		assert_eq!(Localization::select_language(&candidate_list, &["ko", "zh-tw", "en"], "ko"), "zh-tw");
		assert_eq!(Localization::select_language(&candidate_list, &["ko", "en"], "ko"), "en");
		assert_eq!(Localization::select_language(&candidate_list, &["ko"], "ko"), "ko");
		assert_eq!(convert_locale_to_language_tag("ko_KR.UTF-8"), "ko-KR");
		assert_eq!(convert_locale_to_language_tag("C.UTF-8"), "");
		let detected_language = Localization::detect_language(&["ko", "en"], "ko");
		assert!(detected_language == "ko" || detected_language == "en");
	}
}
