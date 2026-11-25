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
// - 외계행성 광부 키우기에서 엔진으로 옮긴 것: 언어 태그를 뒤 조각부터 하나씩 떼며 맞추기와 별칭(set_language_aliases),
//   오른쪽에서 왼쪽으로 쓰는 언어(is_right_to_left, set_right_to_left)와 넣는 값을 방향 격리 문자로 묶기(isolate_text),
//   이름 자리 채우기(text_named, "{seconds}" 꼴, 빗금으로만 이은 값들은 통째로 한 번 묶는다).
//   배치 뒤집기(mirror_layout_x, read_layout_text_align, 원본 LanguagePack 의 mirrorLayoutX, readLayoutTextAlign).
//   detect_language 는 웹에서 원본처럼 navigator.languages 를 읽고, 데스크탑에서는 LANGUAGE / LC_ALL / LC_MESSAGES / LANG 환경 변수를 읽는다.
//     (윈도우는 보통 이 변수가 없어서 기본 언어가 된다)
//==============================================================================
use crate::core::graphic::TextAlign;
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
	alias_to_language: HashMap<String, String>,
	right_to_left_languages: HashSet<String>,
}

// 처음부터 오른쪽에서 왼쪽으로 쓰는 언어. (아랍어, 페르시아어, 히브리어, 우르두어, 파슈토어, 이디시어, 디베히어)
const DEFAULT_RIGHT_TO_LEFT_LANGUAGES: [&str; 7] = ["ar", "fa", "he", "ur", "ps", "yi", "dv"];
// 방향 격리 문자. (처음 나오는 방향이 강한 글자로 방향을 정하는 격리의 시작과 끝)
const FIRST_STRONG_ISOLATE: char = '\u{2068}';
const POP_DIRECTIONAL_ISOLATE: char = '\u{2069}';

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
			alias_to_language: HashMap::new(),
			right_to_left_languages: DEFAULT_RIGHT_TO_LEFT_LANGUAGES.iter().map(|code| String::from(*code)).collect(),
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
	// - 태그의 뒤 조각부터 하나씩 떼며 지원 목록의 코드와 별칭을 본다. ("zh-Hant-TW" → "zh-hant-tw", "zh-hant", "zh", 밑줄은 하이픈으로)
	pub fn select_language(candidate_list: &[String], supported_language_codes: &[&str], default_language_code: &str) -> String {
		let supported_set: HashSet<String> = supported_language_codes.iter().map(|code| code.to_lowercase()).collect();
		let alias_to_language = with_state(|state| state.alias_to_language.clone());
		for language_tag in candidate_list {
			let lowered_tag = language_tag.to_lowercase().replace('_', "-");
			let subtag_list: Vec<&str> = lowered_tag.split('-').collect();
			for subtag_count in (1..=subtag_list.len()).rev() {
				let candidate_tag = subtag_list[..subtag_count].join("-");
				if supported_set.contains(&candidate_tag) {
					return candidate_tag;
				}
				if let Some(alias_language) = alias_to_language.get(&candidate_tag) {
					if supported_set.contains(alias_language) {
						return alias_language.clone();
					}
				}
			}
		}
		default_language_code.to_string()
	}

	//==============================================================================
	// 언어의 별칭 설정. (예: "zh-hant" 에 "zh-tw", "zh-hk", 태그를 맞출 때 별칭도 그 언어로 본다)
	//==============================================================================
	pub fn set_language_aliases(language_code: &str, alias_list: &[&str]) {
		let lowered_language_code = language_code.to_lowercase();
		with_state(|state| {
			for alias in alias_list {
				state.alias_to_language.insert(alias.to_lowercase().replace('_', "-"), lowered_language_code.clone());
			}
		});
	}

	//==============================================================================
	// 오른쪽에서 왼쪽으로 쓰는 언어로 정하거나 풀기. (처음은 ar, fa, he, ur, ps, yi, dv)
	//==============================================================================
	pub fn set_right_to_left(language_code: &str, is_right_to_left: bool) {
		let primary_code = language_code.to_lowercase().replace('_', "-").split('-').next().unwrap_or("").to_string();
		with_state(|state| {
			if is_right_to_left {
				state.right_to_left_languages.insert(primary_code);
			}
			else {
				state.right_to_left_languages.remove(&primary_code);
			}
		});
	}

	//==============================================================================
	// 지금 언어가 오른쪽에서 왼쪽으로 쓰는 언어인지. (글자를 그 방향으로 굽고 배치를 좌우로 뒤집을 때)
	//==============================================================================
	pub fn is_right_to_left() -> bool {
		with_state(|state| {
			let primary_code = state.language_code.split('-').next().unwrap_or("");
			state.right_to_left_languages.contains(primary_code)
		})
	}

	//==============================================================================
	// 배치의 가로 자리. (오른쪽에서 왼쪽 언어면 부모 너비 안에서 좌우를 뒤집은 왼쪽, 아니면 그대로)
	// - 창과 표의 왼쪽, 오른쪽 배치를 언어 방향에 맞춰 뒤집을 때 쓴다. 화면 구석의 자리나 슬라이더의 방향처럼 뒤집지 않을 것은 부르지 않는다.
	//==============================================================================
	pub fn mirror_layout_x(x: f32, width: f32, parent_width: f32) -> f32 {
		if !Localization::is_right_to_left() {
			return x;
		}
		parent_width - x - width
	}

	//==============================================================================
	// 배치의 글자 정렬. (오른쪽에서 왼쪽 언어면 Left 와 Right 를 바꾸고, 나머지는 그대로)
	//==============================================================================
	pub fn read_layout_text_align(text_align: TextAlign) -> TextAlign {
		if !Localization::is_right_to_left() {
			return text_align;
		}
		match text_align {
			TextAlign::Left => {
				TextAlign::Right
			}
			TextAlign::Right => {
				TextAlign::Left
			}
			_ => {
				text_align
			}
		}
	}

	//==============================================================================
	// 글 사이에 끼우는 값 묶기. (오른쪽에서 왼쪽 언어면 방향 격리 문자로 묶고, 아니면 그대로)
	// - 숫자와 단위(123°/s, +60%)나 사람 이름이 앞뒤 글과 섞여 흩어지지 않고 제 방향으로 한 덩어리로 놓이게 한다.
	//==============================================================================
	pub fn isolate_text(text: &str) -> String {
		if text.is_empty() || !Localization::is_right_to_left() {
			return text.to_string();
		}
		format!("{}{}{}", FIRST_STRONG_ISOLATE, text, POP_DIRECTIONAL_ISOLATE)
	}

	//==============================================================================
	// 이름 자리 채우기. ("Restart {seconds}" + [("seconds", &3)] → "Restart 3")
	// - 오른쪽에서 왼쪽 언어면 넣은 값을 방향 격리 문자로 묶는다.
	// - 빗금으로만 이은 값들("{count}/{limit}")은 값마다가 아니라 통째로 한 번 묶는다. (값마다 묶으면 1/40 이 40/1 로 보인다)
	// - 이름이 없는 자리는 그대로 둔다.
	//==============================================================================
	pub fn text_named(key: &str, values: &[(&str, &dyn Display)]) -> String {
		let text = Localization::text(key, &[]);
		let characters: Vec<char> = text.chars().collect();
		let mut result = String::new();
		let mut index = 0;
		while index < characters.len() {
			let (group_end, group_text, is_filled) = Localization::read_placeholder_group(&characters, index, values);
			if group_end > index {
				if is_filled {
					result.push_str(&Localization::isolate_text(&group_text));
				}
				else {
					result.push_str(&group_text);
				}
				index = group_end;
				continue;
			}
			result.push(characters[index]);
			index += 1;
		}
		result
	}

	//==============================================================================
	// index 에서 시작하는 자리 무리 읽기. ({이름} 하나, 또는 빗금(앞뒤 빈칸 허용)으로만 이은 여럿)
	// - (무리 끝, 채운 글, 하나라도 채웠는지), 자리가 아니면 끝이 index 그대로
	//==============================================================================
	fn read_placeholder_group(characters: &[char], index: usize, values: &[(&str, &dyn Display)]) -> (usize, String, bool) {
		let mut cursor = index;
		let mut group_text = String::new();
		let mut is_filled = false;
		let mut group_end = index;
		loop {
			let (placeholder_end, name) = match Localization::read_placeholder(characters, cursor) {
				Some(placeholder) => {
					placeholder
				}
				None => {
					break;
				}
			};
			if group_end > index {
				let separator: String = characters[group_end..cursor].iter().collect();
				group_text.push_str(&separator);
			}
			match values.iter().find(|(value_name, _)| *value_name == name) {
				Some((_, value)) => {
					group_text.push_str(&value.to_string());
					is_filled = true;
				}
				None => {
					group_text.push('{');
					group_text.push_str(&name);
					group_text.push('}');
				}
			}
			group_end = placeholder_end;

			// 다음이 빈칸, 빗금, 빈칸 뒤 자리면 같은 무리로 잇는다.
			let mut next = group_end;
			while next < characters.len() && characters[next] == ' ' {
				next += 1;
			}
			if next >= characters.len() || characters[next] != '/' {
				break;
			}
			next += 1;
			while next < characters.len() && characters[next] == ' ' {
				next += 1;
			}
			if Localization::read_placeholder(characters, next).is_none() {
				break;
			}
			cursor = next;
		}
		(group_end, group_text, is_filled)
	}

	//==============================================================================
	// index 에서 시작하는 {이름} 하나 읽기. (이름은 영문과 숫자, (끝, 이름))
	//==============================================================================
	fn read_placeholder(characters: &[char], index: usize) -> Option<(usize, String)> {
		if characters.get(index) != Some(&'{') {
			return None;
		}
		let mut cursor = index + 1;
		let mut name = String::new();
		while cursor < characters.len() && characters[cursor].is_ascii_alphanumeric() {
			name.push(characters[cursor]);
			cursor += 1;
		}
		if name.is_empty() || characters.get(cursor) != Some(&'}') {
			return None;
		}
		Some((cursor + 1, name))
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
	// 오른쪽에서 왼쪽 언어면 배치의 가로 자리와 정렬을 뒤집고, 아니면 그대로 둔다.
	//==============================================================================
	#[test]
	fn mirror_layout_for_right_to_left() {
		Localization::clear();
		Localization::set_language("en");
		assert!(!Localization::is_right_to_left());
		assert_eq!(Localization::mirror_layout_x(10.0, 20.0, 100.0), 10.0);
		assert_eq!(Localization::read_layout_text_align(TextAlign::Left), TextAlign::Left);
		Localization::set_language("ar");
		assert!(Localization::is_right_to_left());
		assert_eq!(Localization::mirror_layout_x(10.0, 20.0, 100.0), 70.0);
		assert_eq!(Localization::read_layout_text_align(TextAlign::Left), TextAlign::Right);
		assert_eq!(Localization::read_layout_text_align(TextAlign::Right), TextAlign::Left);
		assert_eq!(Localization::read_layout_text_align(TextAlign::Center), TextAlign::Center);
	}

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

	//==============================================================================
	// 태그를 조각마다 떼며 맞추기, 별칭, 쓰기 방향, 이름 자리와 빗금 무리 묶기.
	//==============================================================================
	#[test]
	fn tag_matching_direction_and_named_placeholders() {
		Localization::set_language_aliases("zh-hant", &["zh-tw", "zh_HK"]);
		assert_eq!(Localization::select_language(&[String::from("zh-Hant-TW")], &["en", "zh-hant"], "en"), "zh-hant");
		assert_eq!(Localization::select_language(&[String::from("zh_HK")], &["en", "zh-hant"], "en"), "zh-hant");
		assert_eq!(Localization::select_language(&[String::from("pt-BR")], &["en", "pt"], "en"), "pt");
		assert_eq!(Localization::select_language(&[String::from("xx")], &["en"], "en"), "en");

		Localization::set_table("ar", [("restart", "اعادة {seconds}"), ("count", "{count}/{limit} عمال"), ("name", "{who} و {unknown}")]);
		Localization::set_language("ar");
		assert!(Localization::is_right_to_left());
		assert_eq!(Localization::text_named("restart", &[("seconds", &3)]), "اعادة \u{2068}3\u{2069}");
		assert_eq!(Localization::text_named("count", &[("count", &1), ("limit", &40)]), "\u{2068}1/40\u{2069} عمال");
		assert_eq!(Localization::text_named("name", &[("who", &"Ann")]), "\u{2068}Ann\u{2069} و {unknown}");
		Localization::set_language("en");
		assert!(!Localization::is_right_to_left());
		assert_eq!(Localization::text_named("{a} / {b}", &[("a", &2), ("b", &5)]), "2 / 5");
		Localization::set_right_to_left("en", true);
		assert!(Localization::is_right_to_left());
		Localization::set_right_to_left("en", false);
	}
}
