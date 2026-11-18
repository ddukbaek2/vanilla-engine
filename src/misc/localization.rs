//==============================================================================
// 로컬라이제이션.
// - 원본 misc/localization.js 를 옮긴 것이다. 언어 자동 감지, 번역 테이블, {0}{1} 자리 표시자 바꾸기, 폴백을 한 곳에 모았다.
// - 키는 원문 그대로 써도 된다. 표에 없으면 원문(키)을 그대로 돌려주므로 조금씩 번역해 나갈 수 있다.
// - 사용:
//     Localization::setTable("en", [("확인", "OK"), ("{0}점 획득", "Got {0} points")]);
//     Localization::setLanguage(&Localization::detectLanguage(&["ko", "en"], "ko"));
//     Localization::text("{0}점 획득", &[&120]);
// - 원본과 다른 점:
//   원본은 클래스의 정적 필드에 표를 든다. 러스트 판은 스레드 하나의 전역 상태로 든다. (엔진은 한 스레드에서 돈다)
//   표의 값은 글 하나 또는 글 목록(LocalizationValue)이다. JSON 표(JsonAsset 으로 읽은 것)는 setTableFromJson 으로 넣는다.
//   text 의 가변 인자는 Display 참조 목록(&[&dyn Display])이다.
//   detectLanguage 는 웹에서 원본처럼 navigator.languages 를 읽고, 데스크탑에서는 LANGUAGE / LC_ALL / LC_MESSAGES / LANG 환경 변수를 읽는다.
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
	text(String),
	textList(Vec<String>),
}

impl From<&str> for LocalizationValue {
	//==============================================================================
	// 글 하나로.
	//==============================================================================
	fn from(text: &str) -> LocalizationValue {
		LocalizationValue::text(text.to_string())
	}
}

impl From<String> for LocalizationValue {
	//==============================================================================
	// 글 하나로.
	//==============================================================================
	fn from(text: String) -> LocalizationValue {
		LocalizationValue::text(text)
	}
}

impl From<Vec<String>> for LocalizationValue {
	//==============================================================================
	// 글 목록으로.
	//==============================================================================
	fn from(textList: Vec<String>) -> LocalizationValue {
		LocalizationValue::textList(textList)
	}
}

impl From<Vec<&str>> for LocalizationValue {
	//==============================================================================
	// 글 목록으로.
	//==============================================================================
	fn from(textList: Vec<&str>) -> LocalizationValue {
		LocalizationValue::textList(textList.into_iter().map(|text| text.to_string()).collect())
	}
}

// 번역 표. (키(원문) → 번역 값)
pub type LocalizationTable = HashMap<String, LocalizationValue>;

//==============================================================================
// 로컬라이제이션 전역 상태. (원본 정적 필드 #tableByLanguage, #languageCode, #fallbackLanguageCode)
//==============================================================================
struct LocalizationState {
	tableByLanguage: HashMap<String, LocalizationTable>,
	languageCode: String,
	fallbackLanguageCode: String,
}

thread_local! {
	static LOCALIZATION_STATE: RefCell<Option<LocalizationState>> = const { RefCell::new(None) };
}

//==============================================================================
// 전역 상태 읽고 쓰기. (처음 쓸 때 언어 "ko", 폴백 "ko" 로 만든다)
//==============================================================================
fn withState<R>(visitor: impl FnOnce(&mut LocalizationState) -> R) -> R {
	LOCALIZATION_STATE.with(|stateCell| {
		let mut stateOption = stateCell.borrow_mut();
		let state = stateOption.get_or_insert_with(|| LocalizationState {
			tableByLanguage: HashMap::new(),
			languageCode: String::from("ko"),
			fallbackLanguageCode: String::from("ko"),
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
	pub fn detectLanguage(supportedLanguageCodes: &[&str], defaultLanguageCode: &str) -> String {
		let candidateList = readPreferredLanguageTags();
		Localization::selectLanguage(&candidateList, supportedLanguageCodes, defaultLanguageCode)
	}

	//==============================================================================
	// 선호 언어 목록에서 지원하는 첫 언어 고르기. (detectLanguage 의 대조 부분)
	//==============================================================================
	pub fn selectLanguage(candidateList: &[String], supportedLanguageCodes: &[&str], defaultLanguageCode: &str) -> String {
		let supportedSet: HashSet<String> = supportedLanguageCodes.iter().map(|code| code.to_lowercase()).collect();
		for languageTag in candidateList {
			let loweredTag = languageTag.to_lowercase();
			if supportedSet.contains(&loweredTag) {
				return loweredTag;
			}
			let primaryCode = loweredTag.split('-').next().unwrap_or("").to_string();
			if supportedSet.contains(&primaryCode) {
				return primaryCode;
			}
		}
		defaultLanguageCode.to_string()
	}

	//==============================================================================
	// 번역 표 설정. (언어 코드는 소문자로 바꿔 든다. 같은 언어 표가 있으면 바꾼다)
	//==============================================================================
	pub fn setTable<K, V>(languageCode: &str, table: impl IntoIterator<Item = (K, V)>)
	where
		K: Into<String>,
		V: Into<LocalizationValue>,
	{
		let localizationTable: LocalizationTable = table.into_iter().map(|(key, value)| (key.into(), value.into())).collect();
		let loweredLanguageCode = languageCode.to_lowercase();
		withState(|state| {
			state.tableByLanguage.insert(loweredLanguageCode, localizationTable);
		});
	}

	//==============================================================================
	// JSON 객체로 번역 표 설정. (값이 글이면 글, 글 배열이면 목록. 그 밖의 값은 건너뛴다)
	//==============================================================================
	pub fn setTableFromJson(languageCode: &str, tableJson: &serde_json::Value) {
		let mut localizationTable = LocalizationTable::new();
		if let Some(objectMap) = tableJson.as_object() {
			for (key, value) in objectMap {
				match value {
					serde_json::Value::String(text) => {
						localizationTable.insert(key.clone(), LocalizationValue::text(text.clone()));
					}
					serde_json::Value::Array(items) => {
						let textList: Vec<String> = items.iter().filter_map(|item| item.as_str().map(|text| text.to_string())).collect();
						localizationTable.insert(key.clone(), LocalizationValue::textList(textList));
					}
					_ => {
					}
				}
			}
		}
		Localization::setTable(languageCode, localizationTable);
	}

	//==============================================================================
	// 지금 언어 설정. (소문자로)
	//==============================================================================
	pub fn setLanguage(languageCode: &str) {
		let loweredLanguageCode = languageCode.to_lowercase();
		withState(|state| {
			state.languageCode = loweredLanguageCode;
		});
	}

	//==============================================================================
	// 지금 언어 반환.
	//==============================================================================
	pub fn getLanguage() -> String {
		withState(|state| state.languageCode.clone())
	}

	//==============================================================================
	// 폴백 언어 설정. (지금 언어 표에 없으면 이 언어 표를 본다)
	//==============================================================================
	pub fn setFallbackLanguage(languageCode: &str) {
		let loweredLanguageCode = languageCode.to_lowercase();
		withState(|state| {
			state.fallbackLanguageCode = loweredLanguageCode;
		});
	}

	//==============================================================================
	// 번역문 찾기. (지금 언어 → 폴백 언어 → 키 원문 순, {0} {1} ... 을 인자로 바꾼다)
	// - 값이 목록이면 원본처럼 키를 쓴다.
	//==============================================================================
	pub fn text(key: &str, substitutions: &[&dyn Display]) -> String {
		let mut resolvedText = match Localization::lookup(key) {
			Some(LocalizationValue::text(text)) => {
				text
			}
			_ => {
				key.to_string()
			}
		};
		for (substitutionIndex, substitution) in substitutions.iter().enumerate() {
			let placeholder = format!("{{{}}}", substitutionIndex);
			let substitutionText = substitution.to_string();
			resolvedText = resolvedText.replace(&placeholder, &substitutionText);
		}
		resolvedText
	}

	//==============================================================================
	// 목록형 번역 찾기. (대사 목록처럼 여러 줄인 항목. 글 하나면 한 줄짜리 목록, 없으면 [키])
	//==============================================================================
	pub fn textList(key: &str) -> Vec<String> {
		match Localization::lookup(key) {
			Some(LocalizationValue::textList(textList)) => {
				textList
			}
			Some(LocalizationValue::text(text)) => {
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
		withState(|state| {
			if let Some(currentValue) = state.tableByLanguage.get(&state.languageCode).and_then(|currentTable| currentTable.get(key)) {
				return Some(currentValue.clone());
			}
			state.tableByLanguage.get(&state.fallbackLanguageCode).and_then(|fallbackTable| fallbackTable.get(key)).cloned()
		})
	}

	//==============================================================================
	// 모든 표 비우기. (시험용, 언어 설정은 그대로)
	//==============================================================================
	pub fn clear() {
		withState(|state| {
			state.tableByLanguage.clear();
		});
	}
}

//==============================================================================
// 사용자 선호 언어 목록. (웹: navigator.languages, 없으면 navigator.language)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn readPreferredLanguageTags() -> Vec<String> {
	let mut candidateList: Vec<String> = Vec::new();
	let navigator = match web_sys::window() {
		Some(window) => {
			window.navigator()
		}
		None => {
			return candidateList;
		}
	};
	let languageArray = navigator.languages();
	for languageIndex in 0..languageArray.length() {
		if let Some(languageTag) = languageArray.get(languageIndex).as_string() {
			candidateList.push(languageTag);
		}
	}
	if candidateList.is_empty() {
		if let Some(languageTag) = navigator.language() {
			candidateList.push(languageTag);
		}
	}
	candidateList
}

//==============================================================================
// 사용자 선호 언어 목록. (데스크탑: LANGUAGE(콜론으로 여럿) → LC_ALL → LC_MESSAGES → LANG, "ko_KR.UTF-8" 은 "ko-KR" 로)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn readPreferredLanguageTags() -> Vec<String> {
	let mut candidateList: Vec<String> = Vec::new();
	for variableName in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
		let variableValue = match std::env::var(variableName) {
			Ok(variableValue) => {
				variableValue
			}
			Err(_) => {
				continue;
			}
		};
		for localeName in variableValue.split(':') {
			let languageTag = convertLocaleToLanguageTag(localeName);
			if !languageTag.is_empty() && !candidateList.contains(&languageTag) {
				candidateList.push(languageTag);
			}
		}
	}
	candidateList
}

//==============================================================================
// POSIX 로캘 이름 → 언어 태그. ("ko_KR.UTF-8@euro" → "ko-KR", "C" / "POSIX" 는 빈 글)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn convertLocaleToLanguageTag(localeName: &str) -> String {
	let withoutModifier = localeName.split('@').next().unwrap_or("");
	let withoutEncoding = withoutModifier.split('.').next().unwrap_or("").trim();
	if withoutEncoding.is_empty() || withoutEncoding == "C" || withoutEncoding == "POSIX" {
		return String::new();
	}
	withoutEncoding.replace('_', "-")
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
	fn lookupFallbackAndSubstitution() {
		Localization::clear();
		Localization::setTable("EN", [("확인", "OK"), ("{0}점 획득", "Got {0} points"), ("{0}/{1}", "{1} of {0}")]);
		Localization::setTable("ko", [("only.korean", LocalizationValue::from("한국어만")), ("intro", LocalizationValue::from(vec!["첫 줄", "둘째 줄"]))]);
		Localization::setLanguage("En");
		assert_eq!(Localization::getLanguage(), "en");
		assert_eq!(Localization::text("확인", &[]), "OK");
		assert_eq!(Localization::text("{0}점 획득", &[&120]), "Got 120 points");
		assert_eq!(Localization::text("{0}/{1}", &[&3, &"열"]), "열 of 3");
		assert_eq!(Localization::text("only.korean", &[]), "한국어만");
		assert_eq!(Localization::text("없는 키 {0}", &[&1.5]), "없는 키 1.5");
		assert_eq!(Localization::text("intro", &[]), "intro");
		assert_eq!(Localization::textList("intro"), vec!["첫 줄", "둘째 줄"]);
		assert_eq!(Localization::textList("확인"), vec!["OK"]);
		assert_eq!(Localization::textList("none"), vec!["none"]);

		Localization::setFallbackLanguage("en");
		Localization::setLanguage("ja");
		assert_eq!(Localization::text("확인", &[]), "OK");
		assert_eq!(Localization::lookup("only.korean"), None);
	}

	//==============================================================================
	// JSON 표와 언어 고르기.
	//==============================================================================
	#[test]
	fn jsonTableAndLanguageSelection() {
		Localization::setTableFromJson("fr", &serde_json::json!({ "확인": "D'accord", "lines": ["a", "b"], "number": 3 }));
		Localization::setLanguage("fr");
		assert_eq!(Localization::text("확인", &[]), "D'accord");
		assert_eq!(Localization::textList("lines"), vec!["a", "b"]);
		assert_eq!(Localization::text("number", &[]), "number");

		let candidateList = vec![String::from("zh-TW"), String::from("en-US")];
		assert_eq!(Localization::selectLanguage(&candidateList, &["ko", "zh-tw", "en"], "ko"), "zh-tw");
		assert_eq!(Localization::selectLanguage(&candidateList, &["ko", "en"], "ko"), "en");
		assert_eq!(Localization::selectLanguage(&candidateList, &["ko"], "ko"), "ko");
		assert_eq!(convertLocaleToLanguageTag("ko_KR.UTF-8"), "ko-KR");
		assert_eq!(convertLocaleToLanguageTag("C.UTF-8"), "");
		let detectedLanguage = Localization::detectLanguage(&["ko", "en"], "ko");
		assert!(detectedLanguage == "ko" || detectedLanguage == "en");
	}
}
