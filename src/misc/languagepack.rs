//==============================================================================
//! 언어 묶음. (원본 misc/languagepack.js 의 LanguagePack, 자산의 표로 두는 화면 글자의 언어)
//! - Localization(코드 안의 표)과 달리 글자를 자산의 JSON 파일로 둔다. 언어가 많고 번역가가 파일을 따로 고치는 게임에 쓴다.
//! - 경로는 set_language_paths 로 정한다. (처음은 ./assets/localization/languages.json 과 그 폴더)
//! - 언어의 목록은 languages.json({ fallbackLanguage, languages: [{ code, name, englishName, fontGroup, direction, aliases, countries, file }] })이고,
//!   언어마다 한 파일(열쇠 → 글자)이다. 설정의 언어가 비어 있으면 플랫폼이 알려 준 언어(set_preferred_language_tag), 그다음 기기의 언어를 따르고,
//!   목록에 없으면 기본 언어(영어)이다. 고른 언어의 표에 없는 열쇠는 기본 언어의 글자로, 거기에도 없으면 열쇠 그대로 쓴다.
//! - 언어를 불러오면 페이지의 언어(html lang)와 제목(set_document_title_key 의 열쇠, 처음은 game.title)을 그 언어로 바꾼다.
//!   언어는 Document::set_language 로 쓴다. (웹은 html 의 lang, 데스크탑은 엔진이 든 문서 언어. Document::get_language 로 읽는다) 제목은 창 제목이다.
//! - 원본과 다른 것:
//!   원본은 async 함수로 기다린다. 러스트 판은 엔진의 "요청 → 나중에 다 읽힘" 꼴이라 load_language_list, load_language, apply_language_setting 이
//!   읽기를 요청하고, 프레임마다 update(engine) 를 불러야 이어 간다. (데스크탑은 그 자리에서 읽혀 요청한 함수 안에서 끝난다) is_loading 으로 기다린다.
//!   load_language_list, load_language, apply_language_setting 은 원본처럼 await 할 수 있는 LanguageLoadHandle 을 돌려준다.
//!   (엔진 작업 안에서 scene_context.with(|_, 엔진| languagepack::load_language(엔진, 코드)).await, 기다리는 동안 프레임마다 update 를 불러 준다)
//!   원본 applyLanguageSetting 은 불러온 뒤 함수가 돌려준 Promise 까지 기다린다. 퓨처를 돌려주는 함수는 set_language_loaded_async_handler 로 걸고,
//!   엔진이 그 퓨처를 엔진 작업으로 돌려 끝날 때까지 불러오는 중으로 친다. (손잡이의 await 도 그때까지 기다린다)
//!   전역 상태는 스레드 하나의 thread_local 이다. (원본 모듈 변수) 언어 한 줄은 LanguageEntry 구조체다.
//!   불러온 뒤 부를 함수(set_language_loaded_handler)는 (엔진, 언어 한 줄, 모든 글자) 를 받는다. (원본 (언어, 모든 글자), 엔진은 글꼴 받기에 쓴다)
//!
//! # 예제
//! 파일 없이 목록과 표를 바로 넣고 번역하기
//! ```rust
//! use vanilla::languagepack;
//!
//! let language_list_json = r#"{ "fallbackLanguage": "en", "languages": [
//! 	{ "code": "en", "name": "English", "direction": "ltr", "file": "en.json" },
//! 	{ "code": "ar", "name": "Arabic", "direction": "rtl", "file": "ar.json" }
//! ] }"#;
//! assert!(languagepack::set_language_list_from_json(language_list_json));
//! assert!(languagepack::set_language_texts_from_json("en", r#"{ "game.title": "Mine" }"#, r#"{ "game.title": "Mine", "menu.start": "Start" }"#));
//! assert_eq!(languagepack::read_language_code(), "en");
//! assert_eq!(languagepack::translate("game.title"), "Mine");
//! assert_eq!(languagepack::translate("menu.start"), "Start");
//! assert_eq!(languagepack::translate("menu.unknown"), "menu.unknown");
//! ```
//==============================================================================
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use crate::core::graphic::TextAlign;
use crate::core::task::{self, TaskHandle};
use crate::platform::document::Document;
use std::cell::RefCell;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

/// 방향 격리 문자. (처음 나오는 방향이 강한 글자로 방향을 정하는 격리의 시작과 끝, isolate_text)
const FIRST_STRONG_ISOLATE: char = '\u{2068}';
const POP_DIRECTIONAL_ISOLATE: char = '\u{2069}';

//==============================================================================
/// 언어 목록의 한 줄. (원본 { code, name, englishName, fontGroup, direction, aliases, countries, file })
///
/// # 예제
/// JSON 한 줄에서 읽기
/// ```rust
/// use vanilla::LanguageEntry;
///
/// let language_json = serde_json::json!({ "code": "zh-Hant", "name": "Chinese", "englishName": "Chinese (Traditional)", "aliases": ["zh-TW", "zh-HK"], "file": "zh-Hant.json" });
/// let language = LanguageEntry::create_from_json(&language_json).unwrap();
/// assert_eq!(language.code, "zh-Hant");
/// assert_eq!(language.english_name, "Chinese (Traditional)");
/// assert_eq!(language.aliases, vec![String::from("zh-tw"), String::from("zh-hk")]);
/// assert_eq!(language.direction, "");
/// assert!(LanguageEntry::create_from_json(&serde_json::json!({ "name": "No code" })).is_none());
/// ```
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LanguageEntry {
	/// 언어 코드. (원본 code, 언어 파일과 html lang 에 쓰는 이름, 찾을 때는 대소문자를 가리지 않는다)
	pub code: String,
	/// 그 언어로 쓴 언어 이름. (원본 name)
	pub name: String,
	/// 영어로 쓴 언어 이름. (원본 englishName)
	pub english_name: String,
	/// 글꼴 무리 이름. (원본 fontGroup, 게임이 언어에 맞는 글꼴을 고를 때 쓴다)
	pub font_group: String,
	/// 글자 방향. (원본 direction, "rtl" 이면 오른쪽에서 왼쪽, 그 밖은 왼쪽에서 오른쪽)
	pub direction: String,
	/// 이 언어로 볼 다른 태그. (원본 aliases, 소문자로 든다)
	pub aliases: Vec<String>,
	/// 이 언어를 쓰는 나라 코드. (원본 countries, 소문자로 든다)
	pub countries: Vec<String>,
	/// 언어 파일 이름. (원본 file, 언어 폴더 경로 뒤에 붙여 받는다)
	pub file: String,
}

impl LanguageEntry {
	//==============================================================================
	/// JSON 한 줄 읽기. (code 가 없으면 None, 나머지 칸은 없으면 빈 값)
	/// - aliases 와 countries 는 글만 골라 소문자로 바꾼다.
	///
	/// # 인자
	/// - `value`: languages 배열의 한 칸. (JSON 객체)
	///
	/// # 돌려줌
	/// 읽은 한 줄. code 가 없거나 글이 아니면 None.
	//==============================================================================
	pub fn create_from_json(value: &serde_json::Value) -> Option<LanguageEntry> {
		let read_text = |name: &str| -> String {
			value.get(name).and_then(|field| field.as_str()).unwrap_or("").to_string()
		};
		let read_text_list = |name: &str| -> Vec<String> {
			match value.get(name).and_then(|field| field.as_array()) {
				Some(array) => {
					array.iter().filter_map(|item| item.as_str()).map(|item| item.to_lowercase()).collect()
				}
				None => {
					Vec::new()
				}
			}
		};
		let code = value.get("code")?.as_str()?.to_string();
		Some(LanguageEntry {
			code,
			name: read_text("name"),
			english_name: read_text("englishName"),
			font_group: read_text("fontGroup"),
			direction: read_text("direction"),
			aliases: read_text_list("aliases"),
			countries: read_text_list("countries"),
			file: read_text("file"),
		})
	}
}

/// 언어를 불러온 뒤 부르는 함수. (엔진, 언어 한 줄, 그 언어의 모든 글자)
pub type LanguageLoadedHandler = Box<dyn FnMut(&Engine, &LanguageEntry, &str)>;

/// 언어를 불러온 뒤 부르는 async 함수. (원본처럼 돌려준 퓨처가 끝날 때까지 apply_language_setting 이 끝나지 않는다)
pub type LanguageLoadedAsyncHandler = Box<dyn FnMut(&Engine, &LanguageEntry, &str) -> Pin<Box<dyn Future<Output = ()>>>>;

//==============================================================================
/// 언어 불러오기 손잡이. (load_language_list, load_language, apply_language_setting 이 돌려준다. 원본 async 함수의 Promise 자리)
/// - await 하면 불러오기(와 async 로 건 불러온 뒤 함수)가 끝날 때까지 기다린다. 기다리는 동안 프레임마다 update 를 불러 준다.
///
/// # 예제
/// 엔진 작업에서 언어를 불러오고 기다리기
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{languagepack, Engine, EngineConfiguration};
///
/// let asset_directory_path = std::env::temp_dir().join(format!("vanillaLanguageLoadHandleDoc-{}", std::process::id()));
/// std::fs::create_dir_all(asset_directory_path.join("localization")).unwrap();
/// std::fs::write(asset_directory_path.join("localization/languages.json"), r#"{ "fallbackLanguage": "en", "languages": [{ "code": "en", "name": "English", "file": "en.json" }] }"#).unwrap();
/// std::fs::write(asset_directory_path.join("localization/en.json"), r#"{ "greeting": "Hello" }"#).unwrap();
/// let mut engine_configuration = EngineConfiguration::new();
/// engine_configuration.asset_directory_paths = vec![asset_directory_path.clone()];
/// let engine = Engine::new(engine_configuration);
/// languagepack::set_language_paths("localization/languages.json", "localization/");
///
/// let is_loaded = Rc::new(Cell::new(false));
/// let is_loaded_for_task = is_loaded.clone();
/// engine.spawn_task(move |mut engine_context| async move {
/// 	engine_context.with(|engine| languagepack::load_language(engine, "en")).await;
/// 	is_loaded_for_task.set(true);
/// });
/// assert!(is_loaded.get());
/// assert_eq!(languagepack::translate("greeting"), "Hello");
/// std::fs::remove_dir_all(&asset_directory_path).unwrap();
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct LanguageLoadHandle;

//==============================================================================
/// 언어 불러오기를 기다리는 퓨처. (LanguageLoadHandle 을 await 하거나 wait_for_load 가 돌려준다)
/// - 폴링할 때마다 update 를 부르고, is_loading 이 거짓이면 끝난다.
///
/// # 예제
/// 불러오는 것이 없으면 바로 끝난다
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{languagepack, Engine, EngineConfiguration};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let is_finished = Rc::new(Cell::new(false));
/// let is_finished_for_task = is_finished.clone();
/// engine.spawn_task(move |_engine_context| async move {
/// 	languagepack::wait_for_load().await;
/// 	is_finished_for_task.set(true);
/// });
/// assert!(!languagepack::is_loading());
/// assert!(is_finished.get());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct LanguageLoadFuture;

impl Future for LanguageLoadFuture {
	type Output = ();

	//==============================================================================
	/// update 를 부르고 불러오기가 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut LanguageLoadFuture>, context: &mut Context<'_>) -> Poll<()> {
		task::with_polling_engine(update);
		if !is_loading() {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

impl IntoFuture for LanguageLoadHandle {
	type Output = ();
	type IntoFuture = LanguageLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> LanguageLoadFuture {
		LanguageLoadFuture
	}
}

//==============================================================================
/// 불러오는 중인 언어.
//==============================================================================
struct PendingLanguageLoad {
	/// 설정의 언어. (apply_language_setting 이면 목록을 읽은 뒤 쓸 언어를 정한다)
	setting_language_code: String,
	is_setting: bool,
	language: Option<LanguageEntry>,
	fallback_handle: Option<AssetLoadHandle>,
	language_handle: Option<AssetLoadHandle>,
}

//==============================================================================
/// 언어 묶음 전역 상태. (원본 모듈 변수)
//==============================================================================
struct LanguagePackState {
	language_list_path: String,
	language_directory_path: String,
	document_title_key: String,
	preferred_language_tag: String,
	fallback_language_code: Option<String>,
	language_list: Option<Vec<LanguageEntry>>,
	language_list_handle: Option<AssetLoadHandle>,
	current_language: Option<LanguageEntry>,
	current_texts: serde_json::Map<String, serde_json::Value>,
	fallback_texts: serde_json::Map<String, serde_json::Value>,
	pending_load: Option<PendingLanguageLoad>,
	language_loaded_handler: Option<LanguageLoadedHandler>,
	language_loaded_async_handler: Option<LanguageLoadedAsyncHandler>,
	/// async 로 건 불러온 뒤 함수의 작업. (끝날 때까지 불러오는 중이다)
	language_loaded_task: Option<TaskHandle>,
}

thread_local! {
	static LANGUAGE_PACK_STATE: RefCell<LanguagePackState> = RefCell::new(LanguagePackState {
		language_list_path: String::from("./assets/localization/languages.json"),
		language_directory_path: String::from("./assets/localization/"),
		document_title_key: String::from("game.title"),
		preferred_language_tag: String::new(),
		fallback_language_code: None,
		language_list: None,
		language_list_handle: None,
		current_language: None,
		current_texts: serde_json::Map::new(),
		fallback_texts: serde_json::Map::new(),
		pending_load: None,
		language_loaded_handler: None,
		language_loaded_async_handler: None,
		language_loaded_task: None,
	});
}

//==============================================================================
/// 전역 상태 읽고 쓰기.
//==============================================================================
fn with_state<R>(visitor: impl FnOnce(&mut LanguagePackState) -> R) -> R {
	LANGUAGE_PACK_STATE.with(|state_cell| visitor(&mut state_cell.borrow_mut()))
}

//==============================================================================
/// JSON 바이트 풀기. (객체가 아니면 None)
//==============================================================================
fn parse_json_object(bytes: &[u8]) -> Option<serde_json::Map<String, serde_json::Value>> {
	match serde_json::from_slice::<serde_json::Value>(bytes) {
		Ok(serde_json::Value::Object(object)) => {
			Some(object)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 언어의 목록 JSON 넣기. (load_language_list 가 받은 것을 넣는다. 파일 없이 바로 줄 때도 쓴다, 읽었으면 true)
/// - code 가 없는 줄은 뺀다. fallbackLanguage 가 없으면 기본 언어는 "en" 이다.
///
/// # 인자
/// - `json_text`: languages.json 의 글. ({ fallbackLanguage, languages: [...] })
///
/// # 돌려줌
/// 읽어 들였으면 true. JSON 객체가 아니거나 languages 배열이 없으면 false 이고 앞의 목록을 그대로 둔다.
//==============================================================================
pub fn set_language_list_from_json(json_text: &str) -> bool {
	let object = match parse_json_object(json_text.as_bytes()) {
		Some(object) => {
			object
		}
		None => {
			return false;
		}
	};
	apply_language_list_object(&object)
}

//==============================================================================
/// 언어 목록 객체 들기. (languages 배열이 없으면 false)
//==============================================================================
fn apply_language_list_object(object: &serde_json::Map<String, serde_json::Value>) -> bool {
	let language_array = match object.get("languages").and_then(|languages| languages.as_array()) {
		Some(language_array) => {
			language_array
		}
		None => {
			return false;
		}
	};
	let language_list: Vec<LanguageEntry> = language_array.iter().filter_map(LanguageEntry::create_from_json).collect();
	let fallback_language_code = object.get("fallbackLanguage").and_then(|fallback| fallback.as_str()).map(|fallback| fallback.to_string());
	with_state(|state| {
		state.language_list = Some(language_list);
		state.fallback_language_code = fallback_language_code;
	});
	true
}

//==============================================================================
/// 언어의 목록 불러오기. (한 번만, 원본 loadLanguageList. 받은 뒤 update 가 든다)
/// - 이미 목록이 있거나 받는 중이면 다시 요청하지 않는다. 경로는 set_language_paths 의 목록 파일이다.
///
/// # 인자
/// - `engine`: 애셋을 받을 엔진. (데스크탑은 그 자리에서 읽혀 이 함수 안의 update 에서 든다)
///
/// # 돌려줌
/// await 할 수 있는 불러오기 손잡이.
//==============================================================================
pub fn load_language_list(engine: &Engine) -> LanguageLoadHandle {
	let language_list_path = with_state(|state| {
		if state.language_list.is_some() || state.language_list_handle.is_some() {
			return None;
		}
		Some(state.language_list_path.clone())
	});
	if let Some(language_list_path) = language_list_path {
		let load_handle = engine.request_asset_bytes(&language_list_path);
		with_state(|state| {
			state.language_list_handle = Some(load_handle);
		});
	}
	update(engine);
	LanguageLoadHandle
}

//==============================================================================
/// 언어의 목록을 받는 중이거나 언어를 불러오는 중인지. (async 로 건 불러온 뒤 함수가 도는 중도 든다)
///
/// # 돌려줌
/// 목록을 받는 중이거나, 언어 표를 불러오는 중이거나, async 로 건 불러온 뒤 함수의 작업이 끝나지 않았으면 true.
//==============================================================================
pub fn is_loading() -> bool {
	with_state(|state| state.language_list_handle.is_some() || state.pending_load.is_some() || state.language_loaded_task.as_ref().map(|task_handle| !task_handle.is_finished()).unwrap_or(false))
}

//==============================================================================
/// 불러오기를 마치기를 기다리는 퓨처. (원본 await loadLanguageList(), await loadLanguage(코드), await applyLanguageSetting(코드) 의 await 자리)
/// - 요청 함수가 돌려준 손잡이를 await 하는 것과 같다. 기다리는 동안 프레임마다 update 를 부른다.
///
/// # 돌려줌
/// 불러오기가 끝나면 끝나는 퓨처. 불러오는 것이 없으면 첫 폴링에 끝난다.
//==============================================================================
pub fn wait_for_load() -> LanguageLoadFuture {
	LanguageLoadFuture
}

//==============================================================================
/// 언어의 목록 반환. (못 읽었으면 빈 목록, 원본 readLanguages)
///
/// # 돌려줌
/// 언어 목록의 복사본. 아직 읽지 않았거나 못 읽었으면 빈 목록.
//==============================================================================
pub fn read_languages() -> Vec<LanguageEntry> {
	with_state(|state| state.language_list.clone().unwrap_or_default())
}

//==============================================================================
/// 언어 찾기. (코드는 대소문자를 가리지 않는다, 없으면 None. 원본 findLanguage)
///
/// # 인자
/// - `language_code`: 찾을 언어 코드. (빈 글이면 None)
///
/// # 돌려줌
/// 코드가 같은 언어 한 줄의 복사본. 목록이 없거나 없는 코드면 None.
//==============================================================================
pub fn find_language(language_code: &str) -> Option<LanguageEntry> {
	if language_code.is_empty() {
		return None;
	}
	let lower_code = language_code.to_lowercase();
	with_state(|state| state.language_list.as_ref()?.iter().find(|language| language.code.to_lowercase() == lower_code).cloned())
}

//==============================================================================
/// 언어 태그에 맞는 언어. ("pt-BR" → pt, "zh-Hant-TW" → zh-Hant, 뒤의 조각을 하나씩 떼며 코드와 별칭을 본다, 없으면 None. 원본 matchLanguageTag)
/// - 밑줄은 하이픈으로 바꾸고 대소문자를 가리지 않는다.
///
/// # 인자
/// - `language_tag`: 언어 태그. (빈 글이면 None)
///
/// # 돌려줌
/// 처음 맞은 언어 한 줄의 복사본. 없으면 None.
///
/// # 예제
/// 지역 태그와 별칭으로 맞추기
/// ```rust
/// use vanilla::languagepack;
///
/// languagepack::set_language_list_from_json(r#"{ "fallbackLanguage": "en", "languages": [
/// 	{ "code": "en", "file": "en.json" },
/// 	{ "code": "zh-Hant", "aliases": ["zh-tw", "zh-hk"], "file": "zh-Hant.json" }
/// ] }"#);
/// assert_eq!(languagepack::match_language_tag("en-US").unwrap().code, "en");
/// assert_eq!(languagepack::match_language_tag("zh_Hant_TW").unwrap().code, "zh-Hant");
/// assert_eq!(languagepack::match_language_tag("zh-HK").unwrap().code, "zh-Hant");
/// assert!(languagepack::match_language_tag("fr").is_none());
/// ```
//==============================================================================
pub fn match_language_tag(language_tag: &str) -> Option<LanguageEntry> {
	if language_tag.is_empty() {
		return None;
	}
	let lowered_tag = language_tag.to_lowercase().replace('_', "-");
	let subtag_list: Vec<&str> = lowered_tag.split('-').collect();
	let language_list = read_languages();
	for subtag_count in (1..=subtag_list.len()).rev() {
		let candidate_tag = subtag_list[..subtag_count].join("-");
		for language in &language_list {
			if language.code.to_lowercase() == candidate_tag || language.aliases.contains(&candidate_tag) {
				return Some(language.clone());
			}
		}
	}
	None
}

//==============================================================================
/// 앞세울 언어 태그 넣기. (플랫폼이 알려 준 사용자의 언어, 기기의 언어보다 먼저 본다, 없으면 "". 원본 setPreferredLanguageTag)
///
/// # 인자
/// - `language_tag`: 앞세울 언어 태그. (빈 글이면 앞세우지 않는다)
//==============================================================================
pub fn set_preferred_language_tag(language_tag: &str) {
	with_state(|state| {
		state.preferred_language_tag = language_tag.to_string();
	});
}

//==============================================================================
/// 기기의 언어. (앞세울 태그, 기기가 앞세우는 언어 차례대로 목록에 있는 첫 언어, 없으면 기본 언어. 원본 resolveDeviceLanguageCode)
/// - 웹은 navigator.languages, 데스크탑은 sys-locale 로 시스템의 선호 언어 차례를 읽는다. (Localization.detect_language 와 같다)
///
/// # 돌려줌
/// 목록에서 맞은 언어 코드. (목록에 적힌 대로) 맞는 것이 없으면 기본 언어의 코드.
//==============================================================================
pub fn resolve_device_language_code() -> String {
	let mut language_tag_list: Vec<String> = Vec::new();
	let preferred_language_tag = with_state(|state| state.preferred_language_tag.clone());
	if !preferred_language_tag.is_empty() {
		language_tag_list.push(preferred_language_tag);
	}
	language_tag_list.extend(crate::misc::localization::read_preferred_language_tags());
	for language_tag in &language_tag_list {
		if let Some(language) = match_language_tag(language_tag) {
			return language.code;
		}
	}
	read_fallback_language_code()
}

//==============================================================================
/// 기본 언어의 코드. (목록의 fallbackLanguage, 없으면 "en". 원본 readFallbackLanguageCode)
///
/// # 돌려줌
/// 기본 언어의 코드.
//==============================================================================
pub fn read_fallback_language_code() -> String {
	with_state(|state| state.fallback_language_code.clone().unwrap_or_else(|| String::from("en")))
}

//==============================================================================
/// 설정의 언어 → 쓸 언어의 코드. (비었거나 목록에 없으면 기기의 언어, 원본 resolveLanguageCode)
///
/// # 인자
/// - `setting_language_code`: 설정에 적힌 언어 코드. (대소문자를 가리지 않는다)
///
/// # 돌려줌
/// 쓸 언어 코드. (목록에 적힌 대로)
//==============================================================================
pub fn resolve_language_code(setting_language_code: &str) -> String {
	match find_language(setting_language_code) {
		Some(language) => {
			language.code
		}
		None => {
			resolve_device_language_code()
		}
	}
}

//==============================================================================
/// 언어 불러오기. (그 언어의 표와 기본 언어의 표, 목록에 없는 코드면 기본 언어. 원본 loadLanguage)
/// - 목록이 없으면 먼저 받는다. 불러온 뒤 함수는 부르지 않는다. (apply_language_setting 만 부른다)
///
/// # 인자
/// - `engine`: 애셋을 받을 엔진.
/// - `language_code`: 불러올 언어 코드.
///
/// # 돌려줌
/// await 할 수 있는 불러오기 손잡이.
//==============================================================================
pub fn load_language(engine: &Engine, language_code: &str) -> LanguageLoadHandle {
	start_language_load(engine, language_code, false)
}

//==============================================================================
/// 설정의 언어 적용하기. (쓸 언어를 정해 그 표를 불러오고, set_language_loaded_handler 로 건 함수를 부른다. "" 이면 기기의 언어. 원본 applyLanguageSetting)
///
/// # 인자
/// - `engine`: 애셋을 받을 엔진.
/// - `setting_language_code`: 설정에 적힌 언어 코드. (빈 글이거나 목록에 없으면 기기의 언어)
///
/// # 돌려줌
/// await 할 수 있는 불러오기 손잡이. (async 로 건 불러온 뒤 함수가 끝날 때까지 기다린다)
///
/// # 예제
/// 불러온 뒤 함수로 언어와 글자 받기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{languagepack, Engine, EngineConfiguration, LanguageEntry};
///
/// let asset_directory_path = std::env::temp_dir().join(format!("vanillaApplyLanguageDoc-{}", std::process::id()));
/// std::fs::create_dir_all(asset_directory_path.join("localization")).unwrap();
/// std::fs::write(asset_directory_path.join("localization/languages.json"), r#"{ "fallbackLanguage": "en", "languages": [{ "code": "en", "file": "en.json" }] }"#).unwrap();
/// std::fs::write(asset_directory_path.join("localization/en.json"), r#"{ "greeting": "Hello" }"#).unwrap();
/// let mut engine_configuration = EngineConfiguration::new();
/// engine_configuration.asset_directory_paths = vec![asset_directory_path.clone()];
/// let engine = Engine::new(engine_configuration);
/// languagepack::set_language_paths("localization/languages.json", "localization/");
///
/// let loaded_text = Rc::new(RefCell::new(String::new()));
/// let loaded_text_for_handler = loaded_text.clone();
/// languagepack::set_language_loaded_handler(Some(Box::new(move |_engine: &Engine, language: &LanguageEntry, all_texts: &str| {
/// 	*loaded_text_for_handler.borrow_mut() = format!("{} {}", language.code, all_texts);
/// })));
/// languagepack::apply_language_setting(engine, "en");
/// assert!(!languagepack::is_loading());
/// assert_eq!(*loaded_text.borrow(), "en Hello");
/// std::fs::remove_dir_all(&asset_directory_path).unwrap();
/// ```
//==============================================================================
pub fn apply_language_setting(engine: &Engine, setting_language_code: &str) -> LanguageLoadHandle {
	start_language_load(engine, setting_language_code, true)
}

//==============================================================================
/// 언어 불러오기 시작. (목록을 먼저 받는다)
//==============================================================================
fn start_language_load(engine: &Engine, language_code: &str, is_setting: bool) -> LanguageLoadHandle {
	with_state(|state| {
		state.pending_load = Some(PendingLanguageLoad {
			setting_language_code: language_code.to_string(),
			is_setting,
			language: None,
			fallback_handle: None,
			language_handle: None,
		});
	});
	load_language_list(engine)
}

//==============================================================================
/// 이어 가기. (받은 목록과 표를 들고, 다 받으면 언어를 바꾸고 불러온 뒤 함수를 부른다. 프레임마다 부른다)
/// - 언어를 바꾸면 문서 언어를 그 코드로, 창 제목을 제목 열쇠의 글자로 바꾼다.
///
/// # 인자
/// - `engine`: 애셋 손잡이를 받고 불러온 뒤 함수에 넘길 엔진.
//==============================================================================
pub fn update(engine: &Engine) {
	// 목록.
	let list_bytes = with_state(|state| {
		let load_handle = state.language_list_handle.as_ref()?;
		if !load_handle.is_finished() {
			return None;
		}
		let bytes = load_handle.get_bytes();
		state.language_list_handle = None;
		Some(bytes)
	});
	if let Some(bytes) = list_bytes {
		let is_loaded = match bytes.as_deref().and_then(|bytes| parse_json_object(bytes)) {
			Some(object) => {
				apply_language_list_object(&object)
			}
			None => {
				false
			}
		};
		if !is_loaded {
			eprintln!("[localization] 언어의 목록을 읽지 못했습니다.");
		}
	}
	let is_list_pending = with_state(|state| state.language_list_handle.is_some());
	if is_list_pending {
		return;
	}

	// 불러오는 언어: 쓸 언어를 정하고 표를 요청한다.
	let request = with_state(|state| {
		let pending_load = state.pending_load.as_ref()?;
		if pending_load.language.is_some() {
			return None;
		}
		Some((pending_load.setting_language_code.clone(), pending_load.is_setting))
	});
	if let Some((setting_language_code, is_setting)) = request {
		let language_code = if is_setting {
			resolve_language_code(&setting_language_code)
		}
		else {
			setting_language_code
		};
		let fallback_language_code = read_fallback_language_code();
		let language = find_language(&language_code).or_else(|| find_language(&fallback_language_code));
		let language = match language {
			Some(language) => {
				language
			}
			None => {
				with_state(|state| {
					state.pending_load = None;
				});
				return;
			}
		};
		let fallback_language = find_language(&fallback_language_code);
		let (language_directory_path, is_fallback_loaded) = with_state(|state| (state.language_directory_path.clone(), !state.fallback_texts.is_empty()));
		let fallback_handle = match &fallback_language {
			Some(fallback_language) if !is_fallback_loaded => {
				Some(engine.request_asset_bytes(&format!("{}{}", language_directory_path, fallback_language.file)))
			}
			_ => {
				None
			}
		};
		let is_same_as_fallback = fallback_language.as_ref().map(|fallback_language| fallback_language.code == language.code).unwrap_or(false);
		let language_handle = if is_same_as_fallback {
			None
		}
		else {
			Some(engine.request_asset_bytes(&format!("{}{}", language_directory_path, language.file)))
		};
		with_state(|state| {
			if let Some(pending_load) = state.pending_load.as_mut() {
				pending_load.language = Some(language);
				pending_load.fallback_handle = fallback_handle;
				pending_load.language_handle = language_handle;
			}
		});
	}

	// 표를 다 받았으면 든다.
	let is_ready = with_state(|state| {
		let pending_load = match state.pending_load.as_ref() {
			Some(pending_load) => {
				pending_load
			}
			None => {
				return false;
			}
		};
		pending_load.language.is_some() && pending_load.fallback_handle.as_ref().map(|load_handle| load_handle.is_finished()).unwrap_or(true) && pending_load.language_handle.as_ref().map(|load_handle| load_handle.is_finished()).unwrap_or(true)
	});
	if !is_ready {
		return;
	}
	let pending_load = match with_state(|state| state.pending_load.take()) {
		Some(pending_load) => {
			pending_load
		}
		None => {
			return;
		}
	};
	let language = match pending_load.language {
		Some(language) => {
			language
		}
		None => {
			return;
		}
	};
	let read_texts = |load_handle: &AssetLoadHandle, code: &str| -> serde_json::Map<String, serde_json::Value> {
		match load_handle.get_bytes().as_deref().and_then(|bytes| parse_json_object(bytes)) {
			Some(object) => {
				object
			}
			None => {
				eprintln!("[localization] 언어의 표를 읽지 못했습니다: {}", code);
				serde_json::Map::new()
			}
		}
	};
	let fallback_texts = pending_load.fallback_handle.as_ref().map(|load_handle| read_texts(load_handle, &read_fallback_language_code()));
	let language_texts = pending_load.language_handle.as_ref().map(|load_handle| read_texts(load_handle, &language.code));
	with_state(|state| {
		if let Some(fallback_texts) = fallback_texts {
			state.fallback_texts = fallback_texts;
		}
		state.current_texts = match language_texts {
			Some(language_texts) => {
				language_texts
			}
			None => {
				state.fallback_texts.clone()
			}
		};
		state.current_language = Some(language.clone());
	});
	apply_document_language(engine, &language);
	if pending_load.is_setting {
		let handler = with_state(|state| state.language_loaded_handler.take());
		if let Some(mut handler) = handler {
			let all_texts = read_all_texts();
			handler(engine, &language, &all_texts);
			with_state(|state| {
				if state.language_loaded_handler.is_none() {
					state.language_loaded_handler = Some(handler);
				}
			});
		}
		let async_handler = with_state(|state| state.language_loaded_async_handler.take());
		if let Some(mut async_handler) = async_handler {
			let all_texts = read_all_texts();
			let handler_future = async_handler(engine, &language, &all_texts);
			let language_loaded_task = engine.spawn_task(move |_engine_context| handler_future);
			with_state(|state| {
				state.language_loaded_task = Some(language_loaded_task);
				if state.language_loaded_async_handler.is_none() {
					state.language_loaded_async_handler = Some(async_handler);
				}
			});
		}
	}
}

//==============================================================================
/// 페이지의 언어와 제목을 지금 언어로. (원본 applyDocumentLanguage: html 의 lang 과 탭의 제목, 데스크탑은 엔진이 든 문서 언어와 창 제목)
//==============================================================================
fn apply_document_language(engine: &Engine, language: &LanguageEntry) {
	Document::set_language(&language.code);
	let document_title_key = with_state(|state| state.document_title_key.clone());
	if document_title_key.is_empty() {
		return;
	}
	let document_title = translate(&document_title_key);
	if document_title != document_title_key {
		engine.set_title(&document_title);
	}
}

//==============================================================================
/// 지금 언어. (목록의 한 줄, 아직 없으면 None. 원본 readLanguage)
///
/// # 돌려줌
/// 지금 언어 한 줄의 복사본. 아직 불러오지 않았으면 None.
//==============================================================================
pub fn read_language() -> Option<LanguageEntry> {
	with_state(|state| state.current_language.clone())
}

//==============================================================================
/// 지금 언어의 코드. (아직 없으면 기본 언어, 원본 readLanguageCode)
///
/// # 돌려줌
/// 지금 언어 코드. 아직 없으면 기본 언어의 코드.
//==============================================================================
pub fn read_language_code() -> String {
	match read_language() {
		Some(language) => {
			language.code
		}
		None => {
			read_fallback_language_code()
		}
	}
}

//==============================================================================
/// 지금 언어의 글자 방향. ("ltr" 또는 "rtl", 언어 목록의 direction, 아직 없으면 "ltr". 원본 readTextDirection)
///
/// # 돌려줌
/// direction 이 "rtl" 이면 "rtl", 그 밖은 "ltr".
//==============================================================================
pub fn read_text_direction() -> &'static str {
	match read_language() {
		Some(language) if language.direction == "rtl" => {
			"rtl"
		}
		_ => {
			"ltr"
		}
	}
}

//==============================================================================
/// 지금 언어가 오른쪽에서 왼쪽으로 쓰는지. (원본 isRightToLeft)
///
/// # 돌려줌
/// 지금 언어의 direction 이 "rtl" 이면 true.
//==============================================================================
pub fn is_right_to_left() -> bool {
	read_text_direction() == "rtl"
}

//==============================================================================
/// 배치의 가로 자리. (오른쪽에서 왼쪽 언어면 부모 너비 안에서 좌우를 뒤집은 왼쪽, 아니면 그대로. 원본 mirrorLayoutX)
///
/// # 인자
/// - `x`: 왼쪽에서 오른쪽 언어일 때의 왼쪽 자리. (픽셀, 부모 왼쪽 기준)
/// - `width`: 배치할 것의 너비. (픽셀)
/// - `parent_width`: 부모 너비. (픽셀)
///
/// # 돌려줌
/// 오른쪽에서 왼쪽 언어면 parent_width - x - width, 아니면 x.
///
/// # 예제
/// 아랍어 표를 넣으면 좌우를 뒤집는다
/// ```rust
/// use vanilla::{languagepack, TextAlign};
///
/// languagepack::set_language_list_from_json(r#"{ "fallbackLanguage": "en", "languages": [
/// 	{ "code": "en", "direction": "ltr", "file": "en.json" },
/// 	{ "code": "ar", "direction": "rtl", "file": "ar.json" }
/// ] }"#);
/// languagepack::set_language_texts_from_json("ar", "{}", "{}");
/// assert!(languagepack::is_right_to_left());
/// assert_eq!(languagepack::mirror_layout_x(10.0, 20.0, 100.0), 70.0);
/// assert_eq!(languagepack::read_layout_text_align(TextAlign::Left), TextAlign::Right);
/// ```
//==============================================================================
pub fn mirror_layout_x(x: f64, width: f64, parent_width: f64) -> f64 {
	if !is_right_to_left() {
		return x;
	}
	parent_width - x - width
}

//==============================================================================
/// 배치의 글자 정렬. (오른쪽에서 왼쪽 언어면 Left 와 Right 를 바꾸고, 나머지는 그대로. 원본 readLayoutTextAlign)
///
/// # 인자
/// - `text_align`: 왼쪽에서 오른쪽 언어일 때의 정렬.
///
/// # 돌려줌
/// 지금 언어 방향에 맞춘 정렬.
//==============================================================================
pub fn read_layout_text_align(text_align: TextAlign) -> TextAlign {
	if !is_right_to_left() {
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
/// 글 사이에 끼우는 값 묶기. (오른쪽에서 왼쪽 언어면 방향 격리 문자 FSI ... PDI 로 묶고, 아니면 그대로. 원본 isolateText)
///
/// # 인자
/// - `text`: 끼울 값의 글.
///
/// # 돌려줌
/// 오른쪽에서 왼쪽 언어면 U+2068 과 U+2069 로 묶은 글. 아니거나 빈 글이면 그대로.
//==============================================================================
pub fn isolate_text(text: &str) -> String {
	if text.is_empty() || !is_right_to_left() {
		return text.to_string();
	}
	format!("{}{}{}", FIRST_STRONG_ISOLATE, text, POP_DIRECTIONAL_ISOLATE)
}

//==============================================================================
/// 열쇠의 글자. (지금 언어 → 기본 언어 → 열쇠 그대로, 원본 translate)
///
/// # 인자
/// - `key`: 글자의 열쇠. (값이 글이 아닌 칸은 없는 것으로 본다)
///
/// # 돌려줌
/// 찾은 글자. 두 표에 모두 없으면 열쇠 그대로.
//==============================================================================
pub fn translate(key: &str) -> String {
	with_state(|state| {
		if let Some(text) = state.current_texts.get(key).and_then(|value| value.as_str()) {
			return text.to_string();
		}
		if let Some(text) = state.fallback_texts.get(key).and_then(|value| value.as_str()) {
			return text.to_string();
		}
		key.to_string()
	})
}

//==============================================================================
/// 열쇠의 글자에 값 넣기. ("Restart {seconds}" + [("seconds", "3")] → "Restart 3", 원본 formatText)
/// - 오른쪽에서 왼쪽 언어면 넣는 값을 방향 격리 문자로 묶는다. 빗금으로만 이은 값들("{count}/{limit}")은 통째로 한 번 묶는다.
/// - 이름이 values 에 없는 자리는 그대로 둔다. 이름은 영문과 숫자만 된다.
///
/// # 인자
/// - `key`: 글자의 열쇠. (translate 로 찾는다)
/// - `values`: (이름, 값) 목록. 같은 이름이 여럿이면 처음 것을 쓴다.
///
/// # 돌려줌
/// 자리를 채운 글자.
///
/// # 예제
/// 이름 자리 채우기
/// ```rust
/// use vanilla::languagepack;
///
/// languagepack::set_language_list_from_json(r#"{ "fallbackLanguage": "en", "languages": [{ "code": "en", "file": "en.json" }] }"#);
/// languagepack::set_language_texts_from_json("en", r#"{ "restart": "Restart {seconds}", "count": "{count}/{limit} workers" }"#, "{}");
/// assert_eq!(languagepack::format_text("restart", &[("seconds", &3)]), "Restart 3");
/// assert_eq!(languagepack::format_text("count", &[("count", &1), ("limit", &40)]), "1/40 workers");
/// ```
//==============================================================================
pub fn format_text(key: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
	let text = translate(key);
	let characters: Vec<char> = text.chars().collect();
	let mut result = String::new();
	let mut index = 0;
	while index < characters.len() {
		match read_placeholder_group(&characters, index, values) {
			Some((group_end, group_text, is_filled)) => {
				if is_filled {
					result.push_str(&isolate_text(&group_text));
				}
				else {
					result.push_str(&group_text);
				}
				index = group_end;
			}
			None => {
				result.push(characters[index]);
				index += 1;
			}
		}
	}
	result
}

//==============================================================================
/// index 에서 시작하는 자리 무리 읽기. ({이름} 하나, 또는 빗금(앞뒤 빈칸 허용)으로만 이은 여럿. (끝, 채운 글, 하나라도 채웠는지))
//==============================================================================
fn read_placeholder_group(characters: &[char], index: usize, values: &[(&str, &dyn std::fmt::Display)]) -> Option<(usize, String, bool)> {
	let (first_end, first_name) = read_placeholder(characters, index)?;
	let mut group_text = String::new();
	let mut is_filled = false;
	let mut append_value = |group_text: &mut String, name: &str| {
		match values.iter().find(|(value_name, _)| *value_name == name) {
			Some((_, value)) => {
				group_text.push_str(&value.to_string());
				is_filled = true;
			}
			None => {
				group_text.push('{');
				group_text.push_str(name);
				group_text.push('}');
			}
		}
	};
	append_value(&mut group_text, &first_name);
	let mut group_end = first_end;
	loop {
		let mut next = group_end;
		while next < characters.len() && characters[next].is_whitespace() {
			next += 1;
		}
		if next >= characters.len() || characters[next] != '/' {
			break;
		}
		next += 1;
		while next < characters.len() && characters[next].is_whitespace() {
			next += 1;
		}
		let (placeholder_end, name) = match read_placeholder(characters, next) {
			Some(placeholder) => {
				placeholder
			}
			None => {
				break;
			}
		};
		let separator: String = characters[group_end..next].iter().collect();
		group_text.push_str(&separator);
		append_value(&mut group_text, &name);
		group_end = placeholder_end;
	}
	Some((group_end, group_text, is_filled))
}

//==============================================================================
/// index 에서 시작하는 {이름} 하나 읽기. (이름은 영문과 숫자, (끝, 이름))
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
/// 지금 언어의 글자 모두. (글꼴의 조각을 미리 받을 때 쓴다, 원본 readAllTexts)
///
/// # 돌려줌
/// 지금 언어 표의 글 값을 모두 이은 글. (기본 언어 표는 넣지 않는다)
//==============================================================================
pub fn read_all_texts() -> String {
	with_state(|state| state.current_texts.values().filter_map(|value| value.as_str()).collect::<Vec<&str>>().join(""))
}

//==============================================================================
/// 지금 언어의 표 넣기. (파일 없이 바로 줄 때, 기본 언어 표도 함께. 언어가 목록에 없으면 false)
/// - 문서 언어, 창 제목, 불러온 뒤 함수는 건드리지 않는다.
///
/// # 인자
/// - `language_code`: 지금 언어로 할 코드. (목록에 있어야 한다)
/// - `language_json_text`: 그 언어의 표 JSON. (열쇠 → 글자, 객체가 아니면 빈 표)
/// - `fallback_json_text`: 기본 언어의 표 JSON. (객체가 아니면 빈 표)
///
/// # 돌려줌
/// 넣었으면 true. 언어가 목록에 없으면 false 이고 아무것도 바꾸지 않는다.
//==============================================================================
pub fn set_language_texts_from_json(language_code: &str, language_json_text: &str, fallback_json_text: &str) -> bool {
	let language = match find_language(language_code) {
		Some(language) => {
			language
		}
		None => {
			return false;
		}
	};
	let language_texts = parse_json_object(language_json_text.as_bytes()).unwrap_or_default();
	let fallback_texts = parse_json_object(fallback_json_text.as_bytes()).unwrap_or_default();
	with_state(|state| {
		state.current_texts = language_texts;
		state.fallback_texts = fallback_texts;
		state.current_language = Some(language);
	});
	true
}

//==============================================================================
/// 경로 정하기. (언어 목록 파일, 언어 파일들이 있는 폴더, 불러오기 전에 부른다. 원본 setLanguagePaths)
///
/// # 인자
/// - `list_path`: 언어 목록 파일 경로. (애셋 경로, 처음은 "./assets/localization/languages.json")
/// - `directory_path`: 언어 파일 폴더 경로. (뒤에 언어의 file 을 그대로 붙이므로 / 로 끝낸다, 처음은 "./assets/localization/")
//==============================================================================
pub fn set_language_paths(list_path: &str, directory_path: &str) {
	with_state(|state| {
		state.language_list_path = list_path.to_string();
		state.language_directory_path = directory_path.to_string();
	});
}

//==============================================================================
/// 제목이 될 글자의 열쇠 정하기. ("" 이면 제목을 바꾸지 않는다, 원본 setDocumentTitleKey. 웹은 탭 제목, 데스크탑은 창 제목)
/// - 불러온 언어의 표에서 찾은 글자가 열쇠와 같으면(없으면) 제목을 바꾸지 않는다.
///
/// # 인자
/// - `title_key`: 제목 글자의 열쇠. (처음은 "game.title")
//==============================================================================
pub fn set_document_title_key(title_key: &str) {
	with_state(|state| {
		state.document_title_key = title_key.to_string();
	});
}

//==============================================================================
/// 언어를 불러온 뒤 부를 함수 정하기. (apply_language_setting 이 다 불러온 뒤 부른다, None 이면 없음. 원본 setLanguageLoadedHandler)
///
/// # 인자
/// - `handler`: (엔진, 언어 한 줄, 그 언어의 모든 글자) 를 받는 함수. None 이면 지운다.
//==============================================================================
pub fn set_language_loaded_handler(handler: Option<LanguageLoadedHandler>) {
	with_state(|state| {
		state.language_loaded_handler = handler;
	});
}

//==============================================================================
/// 언어를 불러온 뒤 부를 async 함수 정하기. (원본 setLanguageLoadedHandler 에 Promise 를 돌려주는 함수를 건 자리, None 이면 없음)
/// - apply_language_setting 이 다 불러온 뒤 부르고, 돌려준 퓨처를 엔진 작업으로 돌려 끝날 때까지 불러오는 중으로 친다.
///
/// # 인자
/// - `handler`: (엔진, 언어 한 줄, 그 언어의 모든 글자) 를 받아 고정한 퓨처(`Pin<Box<dyn Future>>`)를 돌려주는 함수. None 이면 지운다.
///
/// # 예제
/// 불러온 뒤 0.5 초 쉬는 async 일 걸기
/// ```rust
/// use vanilla::{languagepack, wait, Engine, LanguageEntry};
///
/// languagepack::set_language_loaded_async_handler(Some(Box::new(|_engine: &Engine, _language: &LanguageEntry, _all_texts: &str| {
/// 	Box::pin(async move {
/// 		wait::seconds(0.5).await;
/// 	})
/// })));
/// assert!(!languagepack::is_loading());
/// ```
//==============================================================================
pub fn set_language_loaded_async_handler(handler: Option<LanguageLoadedAsyncHandler>) {
	with_state(|state| {
		state.language_loaded_async_handler = handler;
	});
}

#[cfg(test)]
mod tests {
	use super::*;

	const LANGUAGE_LIST_JSON: &str = r#"{ "fallbackLanguage": "en", "languages": [
		{ "code": "en", "name": "English", "englishName": "English", "fontGroup": "latin", "direction": "ltr", "aliases": [], "countries": [], "file": "en.json" },
		{ "code": "zh-Hant", "name": "繁體中文", "englishName": "Chinese (Traditional)", "fontGroup": "tc", "direction": "ltr", "aliases": ["zh-tw", "zh-hk"], "countries": [], "file": "zh-Hant.json" },
		{ "code": "ar", "name": "العربية", "englishName": "Arabic", "fontGroup": "arabic", "direction": "rtl", "aliases": [], "countries": [], "file": "ar.json" }
	] }"#;

	//==============================================================================
	/// 목록 읽기, 태그 맞추기(뒤 조각 떼기, 별칭), 표와 기본 언어, 오른쪽에서 왼쪽 값 묶기.
	//==============================================================================
	#[test]
	fn language_pack_matches_and_translates() {
		assert!(set_language_list_from_json(LANGUAGE_LIST_JSON));
		assert_eq!(read_languages().len(), 3);
		assert_eq!(match_language_tag("zh-TW").map(|language| language.code), Some(String::from("zh-Hant")));
		assert_eq!(match_language_tag("zh_Hant_TW").map(|language| language.code), Some(String::from("zh-Hant")));
		assert_eq!(match_language_tag("en-US").map(|language| language.code), Some(String::from("en")));
		assert!(match_language_tag("fr").is_none());
		assert_eq!(resolve_language_code("AR"), "ar");
		assert!(set_language_texts_from_json("ar", r#"{ "score": "{count}/{limit} نقاط" }"#, r#"{ "score": "{count}/{limit} points", "game.title": "Game" }"#));
		assert!(is_right_to_left());
		assert_eq!(read_text_direction(), "rtl");
		assert_eq!(translate("game.title"), "Game");
		assert_eq!(translate("missing"), "missing");
		let formatted = format_text("score", &[("count", &1), ("limit", &40)]);
		assert_eq!(formatted, "\u{2068}1/40\u{2069} نقاط");
		assert_eq!(mirror_layout_x(10.0, 20.0, 100.0), 70.0);
		assert_eq!(read_layout_text_align(TextAlign::Left), TextAlign::Right);
	}

	//==============================================================================
	/// 목록 파일에 aliases 가 없는 줄은 빈 별칭이고, 별칭은 소문자로 맞춘다. code 가 없는 줄은 뺀다. (원본 loadLanguageList, setLanguageList 와 같다)
	//==============================================================================
	#[test]
	fn language_list_without_aliases_is_accepted() {
		assert!(set_language_list_from_json(r#"{ "fallbackLanguage": "en", "languages": [
			{ "code": "en", "name": "English", "file": "en.json" },
			{ "code": "pt", "name": "Portugues", "aliases": ["PT-BR", 3], "file": "pt.json" },
			{ "name": "No Code", "file": "none.json" }
		] }"#));
		let languages = read_languages();
		assert_eq!(languages.len(), 2);
		assert!(languages[0].aliases.is_empty());
		assert!(languages[0].countries.is_empty());
		assert_eq!(languages[1].aliases, vec![String::from("pt-br")]);
		assert_eq!(match_language_tag("en-US").map(|language| language.code), Some(String::from("en")));
		assert_eq!(match_language_tag("pt_BR").map(|language| language.code), Some(String::from("pt")));
		assert!(match_language_tag("fr").is_none());
	}

	//==============================================================================
	/// 언어를 불러오면 문서 언어가 그 언어다. (원본 applyDocumentLanguage 의 html lang, 데스크탑은 엔진이 든 문서 언어)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	#[test]
	fn loaded_language_sets_document_language() {
		let asset_directory_path = std::env::temp_dir().join(format!("vanillaLanguagePackTest-{}", std::process::id()));
		let localization_directory_path = asset_directory_path.join("localization");
		std::fs::create_dir_all(&localization_directory_path).expect("localization directory");
		std::fs::write(localization_directory_path.join("languages.json"), LANGUAGE_LIST_JSON).expect("language list");
		std::fs::write(localization_directory_path.join("en.json"), r#"{ "game.title": "Game" }"#).expect("en");
		std::fs::write(localization_directory_path.join("zh-Hant.json"), r#"{ "game.title": "遊戲" }"#).expect("zh-Hant");
		let mut engine_configuration = crate::core::engine::EngineConfiguration::new();
		engine_configuration.asset_directory_paths = vec![asset_directory_path.clone()];
		engine_configuration.document_language = String::from("en");
		let engine = Engine::new(engine_configuration);
		assert_eq!(Document::get_language(), "en");
		set_language_paths("localization/languages.json", "localization/");
		load_language(engine, "zh-Hant");
		update(engine);
		assert!(!is_loading());
		assert_eq!(read_language_code(), "zh-Hant");
		assert_eq!(Document::get_language(), "zh-Hant");
		assert_eq!(translate("game.title"), "遊戲");
		let _ = std::fs::remove_dir_all(&asset_directory_path);
	}

	//==============================================================================
	/// 원본 await applyLanguageSetting: 손잡이를 await 하면 표를 불러오고, async 로 건 불러온 뒤 함수가 돌려준 퓨처가 끝날 때까지 기다린다.
	//==============================================================================
	#[test]
	fn apply_language_setting_awaits_async_handler() {
		use crate::core::engine::EngineConfiguration;
		use crate::core::task::Promise;
		use std::cell::Cell;
		use std::rc::Rc;
		let asset_directory_path = std::env::temp_dir().join("vanillaLanguagePackAsyncTest");
		std::fs::create_dir_all(asset_directory_path.join("localization")).expect("asset directory");
		std::fs::write(asset_directory_path.join("localization").join("languages.json"), LANGUAGE_LIST_JSON).expect("list");
		std::fs::write(asset_directory_path.join("localization").join("en.json"), r#"{ "greeting": "Hello" }"#).expect("en");
		let mut engine_configuration = EngineConfiguration::new();
		engine_configuration.asset_directory_paths = vec![asset_directory_path];
		let engine = Engine::new(engine_configuration);
		set_language_paths("localization/languages.json", "localization/");
		let (handler_promise, handler_resolver) = Promise::<()>::create();
		let handler_promise = Rc::new(std::cell::RefCell::new(Some(handler_promise)));
		let handler_called_texts = Rc::new(std::cell::RefCell::new(String::new()));
		let handler_called_texts_inner = handler_called_texts.clone();
		set_language_loaded_async_handler(Some(Box::new(move |_engine: &Engine, language: &LanguageEntry, _all_texts: &str| {
			*handler_called_texts_inner.borrow_mut() = format!("{} {}", language.code, translate("greeting"));
			let handler_promise = handler_promise.borrow_mut().take();
			Box::pin(async move {
				if let Some(handler_promise) = handler_promise {
					handler_promise.await;
				}
			})
		})));
		let is_applied = Rc::new(Cell::new(false));
		let task_is_applied = is_applied.clone();
		engine.spawn_task(move |mut engine_context| async move {
			engine_context.with(|engine| apply_language_setting(engine, "en")).await;
			task_is_applied.set(true);
		});
		let mut scene_list: Vec<Box<dyn crate::core::scene::Scene>> = Vec::new();
		assert_eq!(*handler_called_texts.borrow(), "en Hello");
		assert!(is_loading());
		crate::core::task::poll_tasks(engine, &mut scene_list, &mut Vec::new());
		assert!(!is_applied.get());
		handler_resolver.resolve(());
		crate::core::task::poll_tasks(engine, &mut scene_list, &mut Vec::new());
		assert!(is_applied.get());
		assert!(!is_loading());
		set_language_loaded_async_handler(None);
	}
}
