//==============================================================================
// 로컬 스토리지.
// - 원본 misc/localstorage.js 를 옮긴 것이다. 문자열 키에 문자열 값을 영구 저장하는 정적 함수 묶음이다.
// - 저장하는 곳:
//   웹(wasm32): 원본처럼 window.localStorage. (출처(origin)마다 따로, 키를 그대로 쓴다)
//   데스크탑: 앱 폴더의 localstorage.json 하나. (키 → 문자열 값의 JSON 객체, 쓸 때마다 바로 저장한다)
//     윈도우   %APPDATA%\{앱 이름}\localstorage.json
//     맥       $HOME/Library/Application Support/{앱 이름}/localstorage.json
//     리눅스   $XDG_DATA_HOME/{앱 이름}/localstorage.json (없으면 $HOME/.local/share/{앱 이름}/...)
//     환경 변수를 못 읽으면 임시 폴더(std::env::temp_dir) 아래에 둔다.
//   앱 이름은 기본으로 실행 파일 이름이다. set_application_name 이나 set_application_name_from_engine(EngineConfiguration.title)로 바꾼다.
//   set_storage_directory 로 폴더를 직접 정할 수도 있다. (시험, 들고 다니는 게임)
// - 원본과 다른 점:
//   숫자는 원본(JS number)처럼 f64 다. 글로 바꿀 때 원본 String(number) 와 맞춰 NaN, Infinity, -0 → "0" 을 따른다.
//     아주 크거나 작은 수는 지수 표기(1e+21) 대신 자리를 다 쓴다. 읽을 때 "0x10" 같은 16진 표기는 받지 않는다.
//   데스크탑 저장은 스레드마다 따로 들고 있다가 파일에 쓴다. (엔진은 한 스레드에서 돈다)
//==============================================================================
use crate::core::engine::Engine;
use std::path::PathBuf;

//==============================================================================
// 로컬 스토리지. (정적 함수 묶음)
//==============================================================================
pub struct LocalStorage;

impl LocalStorage {
	//==============================================================================
	// 앱 이름 설정. (데스크탑 저장 폴더 이름. 웹은 출처마다 따로라 쓰지 않는다)
	//==============================================================================
	pub fn set_application_name(application_name: &str) {
		storage_backend::set_application_name(application_name);
	}

	//==============================================================================
	// 엔진 설정의 창 제목을 앱 이름으로 쓴다. (EngineConfiguration.title)
	//==============================================================================
	pub fn set_application_name_from_engine(engine: &Engine) {
		let engine_configuration = engine.get_engine_configuration();
		storage_backend::set_application_name(&engine_configuration.title);
	}

	//==============================================================================
	// 저장 폴더 직접 설정. (None 이면 앱 이름으로 정한 기본 폴더. 웹은 쓰지 않는다)
	//==============================================================================
	pub fn set_storage_directory(storage_directory_path: Option<PathBuf>) {
		storage_backend::set_storage_directory(storage_directory_path);
	}

	//==============================================================================
	// 저장 파일 경로. (데스크탑만, 웹은 None)
	//==============================================================================
	pub fn get_storage_file_path() -> Option<PathBuf> {
		storage_backend::get_storage_file_path()
	}

	//==============================================================================
	// 전체 제거.
	//==============================================================================
	pub fn clear() {
		storage_backend::clear_all();
	}

	//==============================================================================
	// 문자열 값 설정.
	//==============================================================================
	pub fn set_string(key: &str, string_value: &str) {
		storage_backend::set_item(key, string_value);
	}

	//==============================================================================
	// 논리 값 설정. ("true" / "false" 로 저장한다)
	//==============================================================================
	pub fn set_boolean(key: &str, boolean_value: bool) {
		let string_value = if boolean_value {
			"true"
		}
		else {
			"false"
		};
		LocalStorage::set_string(key, string_value);
	}

	//==============================================================================
	// 숫자 값 설정. (원본 String(number) 꼴로 저장한다)
	//==============================================================================
	pub fn set_number(key: &str, number_value: f64) {
		let string_value = format_number(number_value);
		LocalStorage::set_string(key, &string_value);
	}

	//==============================================================================
	// 값 제거.
	//==============================================================================
	pub fn remove(key: &str) {
		storage_backend::remove_item(key);
	}

	//==============================================================================
	// 문자열 값 반환. (없으면 기본값을 저장하고 기본값을 돌려준다. 원본과 같음)
	//==============================================================================
	pub fn get_string(key: &str, default_string_value: &str) -> String {
		match storage_backend::get_item(key) {
			Some(string_value) => {
				string_value
			}
			None => {
				LocalStorage::set_string(key, default_string_value);
				default_string_value.to_string()
			}
		}
	}

	//==============================================================================
	// 논리 값 반환. (없거나 빈 글이면 기본값을 저장하고 돌려준다. "true" 만 참)
	//==============================================================================
	pub fn get_boolean(key: &str, default_boolean_value: bool) -> bool {
		let string_value = LocalStorage::get_string(key, "");
		if string_value.is_empty() {
			LocalStorage::set_boolean(key, default_boolean_value);
			default_boolean_value
		}
		else {
			string_value == "true"
		}
	}

	//==============================================================================
	// 숫자 값 반환. (없거나 빈 글이면 기본값을 저장하고 돌려준다. 숫자가 아니면 NaN, 원본 Number(text))
	//==============================================================================
	pub fn get_number(key: &str, default_number_value: f64) -> f64 {
		let string_value = LocalStorage::get_string(key, "");
		if string_value.is_empty() {
			LocalStorage::set_number(key, default_number_value);
			default_number_value
		}
		else {
			parse_number(&string_value)
		}
	}

	//==============================================================================
	// 접두사로 시작하는 키 한꺼번에 지우기. (게임 네임스페이스의 저장을 통째로 지울 때, 지운 키 수)
	//==============================================================================
	pub fn clear_by_prefix(prefix: &str) -> usize {
		if prefix.is_empty() {
			return 0;
		}
		let remove_key_list: Vec<String> = storage_backend::get_all_keys().into_iter().filter(|key| key.starts_with(prefix)).collect();
		for key in remove_key_list.iter() {
			storage_backend::remove_item(key);
		}
		remove_key_list.len()
	}

	//==============================================================================
	// 키가 있는지.
	//==============================================================================
	pub fn contains_key(key: &str) -> bool {
		storage_backend::get_item(key).is_some()
	}

	//==============================================================================
	// 모든 키 반환. (정렬해서)
	//==============================================================================
	pub fn get_keys() -> Vec<String> {
		let mut keys = storage_backend::get_all_keys();
		keys.sort();
		keys
	}
}

//==============================================================================
// 숫자 → 글. (원본 String(number): NaN, Infinity, -Infinity, -0 은 "0")
//==============================================================================
fn format_number(number_value: f64) -> String {
	if number_value.is_nan() {
		return String::from("NaN");
	}
	if number_value.is_infinite() {
		if number_value > 0.0 {
			return String::from("Infinity");
		}
		return String::from("-Infinity");
	}
	if number_value == 0.0 {
		return String::from("0");
	}
	format!("{}", number_value)
}

//==============================================================================
// 글 → 숫자. (원본 Number(text): 앞뒤 공백을 무시하고, 빈 글은 0, 숫자가 아니면 NaN)
//==============================================================================
fn parse_number(text: &str) -> f64 {
	let trimmed_text = text.trim();
	if trimmed_text.is_empty() {
		return 0.0;
	}
	match trimmed_text {
		"Infinity" | "+Infinity" => {
			return f64::INFINITY;
		}
		"-Infinity" => {
			return f64::NEG_INFINITY;
		}
		_ => {
		}
	}
	// 러스트는 "inf", "nan" 도 받지만 원본 Number 는 NaN 이다.
	let lowered_text = trimmed_text.to_ascii_lowercase();
	if lowered_text.contains("inf") || lowered_text.contains("nan") {
		return f64::NAN;
	}
	trimmed_text.parse::<f64>().unwrap_or(f64::NAN)
}

//==============================================================================
// 데스크탑 저장소. (앱 폴더의 JSON 파일 하나)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod storage_backend {
	use std::cell::RefCell;
	use std::collections::BTreeMap;
	use std::path::{Path, PathBuf};

	// 저장 파일 이름.
	const STORAGE_FILE_NAME: &str = "localstorage.json";

	//==============================================================================
	// 데스크탑 저장소 상태. (entries 는 처음 쓸 때 파일에서 읽는다)
	//==============================================================================
	struct DesktopStorageState {
		application_name: Option<String>,
		storage_directory_path: Option<PathBuf>,
		entries: Option<BTreeMap<String, String>>,
	}

	thread_local! {
		static DESKTOP_STORAGE_STATE: RefCell<DesktopStorageState> = const {
			RefCell::new(DesktopStorageState {
				application_name: None,
				storage_directory_path: None,
				entries: None,
			})
		};
	}

	//==============================================================================
	// 폴더 이름에 못 쓰는 글자를 '_' 로 바꾼다. (비면 "vanilla")
	//==============================================================================
	fn sanitize_folder_name(name: &str) -> String {
		let sanitized_name: String = name
			.trim()
			.chars()
			.map(|character| {
				if character.is_control() || matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
					'_'
				}
				else {
					character
				}
			})
			.collect();
		let trimmed_name = sanitized_name.trim_end_matches(['.', ' ']).to_string();
		if trimmed_name.is_empty() {
			return String::from("vanilla");
		}
		trimmed_name
	}

	//==============================================================================
	// 기본 앱 이름. (실행 파일 이름, 못 읽으면 "vanilla")
	//==============================================================================
	fn get_default_application_name() -> String {
		let executable_path = match std::env::current_exe() {
			Ok(executable_path) => {
				executable_path
			}
			Err(_) => {
				return String::from("vanilla");
			}
		};
		match executable_path.file_stem() {
			Some(file_stem) => {
				file_stem.to_string_lossy().into_owned()
			}
			None => {
				String::from("vanilla")
			}
		}
	}

	//==============================================================================
	// 환경 변수 경로. (비었거나 상대 경로면 None)
	//==============================================================================
	fn read_directory_variable(variable_name: &str) -> Option<PathBuf> {
		let variable_value = std::env::var_os(variable_name)?;
		let directory_path = PathBuf::from(variable_value);
		if directory_path.as_os_str().is_empty() || !directory_path.is_absolute() {
			return None;
		}
		Some(directory_path)
	}

	//==============================================================================
	// 운영체제별 앱 데이터 폴더. (앱 이름을 붙이기 전)
	//==============================================================================
	fn get_application_data_directory() -> PathBuf {
		#[cfg(target_os = "windows")]
		let data_directory_path = read_directory_variable("APPDATA").or_else(|| read_directory_variable("USERPROFILE").map(|userProfilePath| userProfilePath.join("AppData").join("Roaming")));
		#[cfg(target_os = "macos")]
		let data_directory_path = read_directory_variable("HOME").map(|home_path| home_path.join("Library").join("Application Support"));
		#[cfg(not(any(target_os = "windows", target_os = "macos")))]
		let data_directory_path = read_directory_variable("XDG_DATA_HOME").or_else(|| read_directory_variable("HOME").map(|home_path| home_path.join(".local").join("share")));
		match data_directory_path {
			Some(data_directory_path) => {
				data_directory_path
			}
			None => {
				std::env::temp_dir()
			}
		}
	}

	//==============================================================================
	// 저장 파일 경로 계산.
	//==============================================================================
	fn resolve_storage_file_path(state: &DesktopStorageState) -> PathBuf {
		if let Some(storage_directory_path) = &state.storage_directory_path {
			return storage_directory_path.join(STORAGE_FILE_NAME);
		}
		let application_name = match &state.application_name {
			Some(application_name) => {
				application_name.clone()
			}
			None => {
				get_default_application_name()
			}
		};
		let folder_name = sanitize_folder_name(&application_name);
		let application_data_directory = get_application_data_directory();
		application_data_directory.join(folder_name).join(STORAGE_FILE_NAME)
	}

	//==============================================================================
	// 파일에서 읽기. (없거나 깨졌으면 빈 저장소. 문자열이 아닌 값은 JSON 글로 바꿔 둔다)
	//==============================================================================
	fn read_entries_from_file(storage_file_path: &Path) -> BTreeMap<String, String> {
		let mut entries: BTreeMap<String, String> = BTreeMap::new();
		let file_text = match std::fs::read_to_string(storage_file_path) {
			Ok(file_text) => {
				file_text
			}
			Err(_) => {
				return entries;
			}
		};
		let parsed_value: serde_json::Value = match serde_json::from_str(&file_text) {
			Ok(parsed_value) => {
				parsed_value
			}
			Err(error) => {
				eprintln!("LocalStorage: 저장 파일을 읽지 못했습니다 {} ({})", storage_file_path.display(), error);
				return entries;
			}
		};
		if let serde_json::Value::Object(object_map) = parsed_value {
			for (key, value) in object_map {
				let string_value = match value {
					serde_json::Value::String(string_value) => {
						string_value
					}
					other_value => {
						other_value.to_string()
					}
				};
				entries.insert(key, string_value);
			}
		}
		entries
	}

	//==============================================================================
	// 파일에 쓰기. (임시 파일에 쓰고 바꿔치기해서 중간에 꺼져도 깨지지 않게 한다)
	//==============================================================================
	fn write_entries_to_file(storage_file_path: &Path, entries: &BTreeMap<String, String>) {
		if let Some(parent_directory_path) = storage_file_path.parent() {
			if let Err(error) = std::fs::create_dir_all(parent_directory_path) {
				eprintln!("LocalStorage: 저장 폴더를 만들지 못했습니다 {} ({})", parent_directory_path.display(), error);
				return;
			}
		}
		let file_text = match serde_json::to_string_pretty(entries) {
			Ok(file_text) => {
				file_text
			}
			Err(error) => {
				eprintln!("LocalStorage: 저장 내용을 만들지 못했습니다 ({})", error);
				return;
			}
		};
		let temporary_file_path = storage_file_path.with_extension("json.tmp");
		let write_result = std::fs::write(&temporary_file_path, file_text).and_then(|_| std::fs::rename(&temporary_file_path, storage_file_path));
		if let Err(error) = write_result {
			eprintln!("LocalStorage: 저장하지 못했습니다 {} ({})", storage_file_path.display(), error);
		}
	}

	//==============================================================================
	// 저장 내용을 (필요하면 읽어 와서) 읽는다.
	//==============================================================================
	fn with_entries<R>(reader: impl FnOnce(&BTreeMap<String, String>) -> R) -> R {
		DESKTOP_STORAGE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			if state.entries.is_none() {
				let storage_file_path = resolve_storage_file_path(&state);
				state.entries = Some(read_entries_from_file(&storage_file_path));
			}
			let entries = state.entries.get_or_insert_with(BTreeMap::new);
			reader(entries)
		})
	}

	//==============================================================================
	// 저장 내용을 바꾸고, 바뀌었으면 파일에 쓴다. (writer 가 바뀌었는지를 돌려준다)
	//==============================================================================
	fn modify_entries(writer: impl FnOnce(&mut BTreeMap<String, String>) -> bool) {
		DESKTOP_STORAGE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			let storage_file_path = resolve_storage_file_path(&state);
			if state.entries.is_none() {
				state.entries = Some(read_entries_from_file(&storage_file_path));
			}
			let entries = state.entries.get_or_insert_with(BTreeMap::new);
			let is_changed = writer(entries);
			if is_changed {
				write_entries_to_file(&storage_file_path, entries);
			}
		});
	}

	//==============================================================================
	// 앱 이름 설정. (읽어 둔 내용은 버리고 다음에 새 폴더에서 읽는다)
	//==============================================================================
	pub fn set_application_name(application_name: &str) {
		DESKTOP_STORAGE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			state.application_name = Some(application_name.to_string());
			state.entries = None;
		});
	}

	//==============================================================================
	// 저장 폴더 직접 설정. (읽어 둔 내용은 버린다)
	//==============================================================================
	pub fn set_storage_directory(storage_directory_path: Option<PathBuf>) {
		DESKTOP_STORAGE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			state.storage_directory_path = storage_directory_path;
			state.entries = None;
		});
	}

	//==============================================================================
	// 저장 파일 경로.
	//==============================================================================
	pub fn get_storage_file_path() -> Option<PathBuf> {
		DESKTOP_STORAGE_STATE.with(|state_cell| {
			let state = state_cell.borrow();
			Some(resolve_storage_file_path(&state))
		})
	}

	//==============================================================================
	// 값 읽기.
	//==============================================================================
	pub fn get_item(key: &str) -> Option<String> {
		with_entries(|entries| entries.get(key).cloned())
	}

	//==============================================================================
	// 값 쓰기.
	//==============================================================================
	pub fn set_item(key: &str, value: &str) {
		modify_entries(|entries| {
			let previous_value = entries.insert(key.to_string(), value.to_string());
			previous_value.as_deref() != Some(value)
		});
	}

	//==============================================================================
	// 값 지우기.
	//==============================================================================
	pub fn remove_item(key: &str) {
		modify_entries(|entries| entries.remove(key).is_some());
	}

	//==============================================================================
	// 전부 지우기.
	//==============================================================================
	pub fn clear_all() {
		modify_entries(|entries| {
			entries.clear();
			true
		});
	}

	//==============================================================================
	// 모든 키. (저장 순서)
	//==============================================================================
	pub fn get_all_keys() -> Vec<String> {
		with_entries(|entries| entries.keys().cloned().collect())
	}
}

//==============================================================================
// 웹 저장소. (window.localStorage)
//==============================================================================
#[cfg(target_arch = "wasm32")]
mod storage_backend {
	use std::path::PathBuf;

	//==============================================================================
	// window.localStorage. (막혀 있으면 None)
	//==============================================================================
	fn get_storage() -> Option<web_sys::Storage> {
		let window = web_sys::window()?;
		window.local_storage().ok().flatten()
	}

	//==============================================================================
	// 앱 이름 설정. (웹은 출처마다 따로라 쓰지 않는다)
	//==============================================================================
	pub fn set_application_name(_application_name: &str) {
	}

	//==============================================================================
	// 저장 폴더 직접 설정. (웹은 쓰지 않는다)
	//==============================================================================
	pub fn set_storage_directory(_storage_directory_path: Option<PathBuf>) {
	}

	//==============================================================================
	// 저장 파일 경로. (웹은 없다)
	//==============================================================================
	pub fn get_storage_file_path() -> Option<PathBuf> {
		None
	}

	//==============================================================================
	// 값 읽기.
	//==============================================================================
	pub fn get_item(key: &str) -> Option<String> {
		let storage = get_storage()?;
		storage.get_item(key).ok().flatten()
	}

	//==============================================================================
	// 값 쓰기. (용량이 넘치면 조용히 실패한다)
	//==============================================================================
	pub fn set_item(key: &str, value: &str) {
		if let Some(storage) = get_storage() {
			let _ = storage.set_item(key, value);
		}
	}

	//==============================================================================
	// 값 지우기.
	//==============================================================================
	pub fn remove_item(key: &str) {
		if let Some(storage) = get_storage() {
			let _ = storage.remove_item(key);
		}
	}

	//==============================================================================
	// 전부 지우기.
	//==============================================================================
	pub fn clear_all() {
		if let Some(storage) = get_storage() {
			let _ = storage.clear();
		}
	}

	//==============================================================================
	// 모든 키. (브라우저가 주는 순서)
	//==============================================================================
	pub fn get_all_keys() -> Vec<String> {
		let mut keys: Vec<String> = Vec::new();
		let storage = match get_storage() {
			Some(storage) => {
				storage
			}
			None => {
				return keys;
			}
		};
		let key_count = storage.length().unwrap_or(0);
		for key_index in 0..key_count {
			if let Ok(Some(key)) = storage.key(key_index) {
				keys.push(key);
			}
		}
		keys
	}
}

//==============================================================================
// 시험. (데스크탑, 임시 폴더에 쓴다)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;

	//==============================================================================
	// 시험마다 다른 임시 폴더를 저장 폴더로 정한다.
	//==============================================================================
	fn use_temporary_storage_directory(test_name: &str) -> PathBuf {
		let unique_suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0);
		let storage_directory_path = std::env::temp_dir().join(format!("vanilla-localstorage-{}-{}-{}", test_name, std::process::id(), unique_suffix));
		LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
		storage_directory_path
	}

	//==============================================================================
	// 숫자 글 바꾸기는 원본 String / Number 와 같다.
	//==============================================================================
	#[test]
	fn number_text_matches_java_script() {
		assert_eq!(format_number(1.0), "1");
		assert_eq!(format_number(1.5), "1.5");
		assert_eq!(format_number(-0.0), "0");
		assert_eq!(format_number(f64::NAN), "NaN");
		assert_eq!(format_number(f64::NEG_INFINITY), "-Infinity");
		assert_eq!(parse_number(" 12 "), 12.0);
		assert_eq!(parse_number("Infinity"), f64::INFINITY);
		assert!(parse_number("abc").is_nan());
		assert!(parse_number("inf").is_nan());
	}

	//==============================================================================
	// 쓰고, 파일에서 다시 읽고, 기본값 저장, 접두사 지우기, 키 목록.
	//==============================================================================
	#[test]
	fn storage_round_trip_through_file() {
		let storage_directory_path = use_temporary_storage_directory("roundtrip");
		LocalStorage::set_string("game.name", "바닐라");
		LocalStorage::set_boolean("game.sound", false);
		LocalStorage::set_number("game.best", 1234.5);
		LocalStorage::set_string("other", "x");
		let storage_file_path = LocalStorage::get_storage_file_path().expect("storage file path");
		assert_eq!(storage_file_path, storage_directory_path.join("localstorage.json"));
		assert!(storage_file_path.exists());

		// 다시 읽게 해도 값이 남아 있다.
		LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
		assert_eq!(LocalStorage::get_string("game.name", ""), "바닐라");
		assert!(!LocalStorage::get_boolean("game.sound", true));
		assert_eq!(LocalStorage::get_number("game.best", 0.0), 1234.5);
		assert!(LocalStorage::contains_key("other"));

		// 없는 키는 기본값을 저장하고 돌려준다.
		assert_eq!(LocalStorage::get_number("game.level", 3.0), 3.0);
		assert_eq!(LocalStorage::get_string("game.level", "9"), "3");
		assert!(LocalStorage::get_boolean("game.vibration", true));
		assert_eq!(LocalStorage::get_string("game.vibration", ""), "true");

		assert_eq!(LocalStorage::get_keys(), vec!["game.best", "game.level", "game.name", "game.sound", "game.vibration", "other"]);
		assert_eq!(LocalStorage::clear_by_prefix("game."), 5);
		assert_eq!(LocalStorage::clear_by_prefix(""), 0);
		assert_eq!(LocalStorage::get_keys(), vec!["other"]);
		LocalStorage::remove("other");
		assert!(!LocalStorage::contains_key("other"));
		LocalStorage::set_string("again", "1");
		LocalStorage::clear();
		LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
		assert!(LocalStorage::get_keys().is_empty());

		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}
}
