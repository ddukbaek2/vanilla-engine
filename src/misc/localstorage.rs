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
//   앱 이름은 기본으로 실행 파일 이름이다. setApplicationName 이나 setApplicationNameFromEngine(EngineConfiguration.title)로 바꾼다.
//   setStorageDirectory 로 폴더를 직접 정할 수도 있다. (시험, 들고 다니는 게임)
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
	pub fn setApplicationName(applicationName: &str) {
		storageBackend::setApplicationName(applicationName);
	}

	//==============================================================================
	// 엔진 설정의 창 제목을 앱 이름으로 쓴다. (EngineConfiguration.title)
	//==============================================================================
	pub fn setApplicationNameFromEngine(engine: &Engine) {
		let engineConfiguration = engine.getEngineConfiguration();
		storageBackend::setApplicationName(&engineConfiguration.title);
	}

	//==============================================================================
	// 저장 폴더 직접 설정. (None 이면 앱 이름으로 정한 기본 폴더. 웹은 쓰지 않는다)
	//==============================================================================
	pub fn setStorageDirectory(storageDirectoryPath: Option<PathBuf>) {
		storageBackend::setStorageDirectory(storageDirectoryPath);
	}

	//==============================================================================
	// 저장 파일 경로. (데스크탑만, 웹은 None)
	//==============================================================================
	pub fn getStorageFilePath() -> Option<PathBuf> {
		storageBackend::getStorageFilePath()
	}

	//==============================================================================
	// 전체 제거.
	//==============================================================================
	pub fn clear() {
		storageBackend::clearAll();
	}

	//==============================================================================
	// 문자열 값 설정.
	//==============================================================================
	pub fn setString(key: &str, stringValue: &str) {
		storageBackend::setItem(key, stringValue);
	}

	//==============================================================================
	// 논리 값 설정. ("true" / "false" 로 저장한다)
	//==============================================================================
	pub fn setBoolean(key: &str, booleanValue: bool) {
		let stringValue = if booleanValue {
			"true"
		}
		else {
			"false"
		};
		LocalStorage::setString(key, stringValue);
	}

	//==============================================================================
	// 숫자 값 설정. (원본 String(number) 꼴로 저장한다)
	//==============================================================================
	pub fn setNumber(key: &str, numberValue: f64) {
		let stringValue = formatNumber(numberValue);
		LocalStorage::setString(key, &stringValue);
	}

	//==============================================================================
	// 값 제거.
	//==============================================================================
	pub fn remove(key: &str) {
		storageBackend::removeItem(key);
	}

	//==============================================================================
	// 문자열 값 반환. (없으면 기본값을 저장하고 기본값을 돌려준다. 원본과 같음)
	//==============================================================================
	pub fn getString(key: &str, defaultStringValue: &str) -> String {
		match storageBackend::getItem(key) {
			Some(stringValue) => {
				stringValue
			}
			None => {
				LocalStorage::setString(key, defaultStringValue);
				defaultStringValue.to_string()
			}
		}
	}

	//==============================================================================
	// 논리 값 반환. (없거나 빈 글이면 기본값을 저장하고 돌려준다. "true" 만 참)
	//==============================================================================
	pub fn getBoolean(key: &str, defaultBooleanValue: bool) -> bool {
		let stringValue = LocalStorage::getString(key, "");
		if stringValue.is_empty() {
			LocalStorage::setBoolean(key, defaultBooleanValue);
			defaultBooleanValue
		}
		else {
			stringValue == "true"
		}
	}

	//==============================================================================
	// 숫자 값 반환. (없거나 빈 글이면 기본값을 저장하고 돌려준다. 숫자가 아니면 NaN, 원본 Number(text))
	//==============================================================================
	pub fn getNumber(key: &str, defaultNumberValue: f64) -> f64 {
		let stringValue = LocalStorage::getString(key, "");
		if stringValue.is_empty() {
			LocalStorage::setNumber(key, defaultNumberValue);
			defaultNumberValue
		}
		else {
			parseNumber(&stringValue)
		}
	}

	//==============================================================================
	// 접두사로 시작하는 키 한꺼번에 지우기. (게임 네임스페이스의 저장을 통째로 지울 때, 지운 키 수)
	//==============================================================================
	pub fn clearByPrefix(prefix: &str) -> usize {
		if prefix.is_empty() {
			return 0;
		}
		let removeKeyList: Vec<String> = storageBackend::getAllKeys().into_iter().filter(|key| key.starts_with(prefix)).collect();
		for key in removeKeyList.iter() {
			storageBackend::removeItem(key);
		}
		removeKeyList.len()
	}

	//==============================================================================
	// 키가 있는지.
	//==============================================================================
	pub fn containsKey(key: &str) -> bool {
		storageBackend::getItem(key).is_some()
	}

	//==============================================================================
	// 모든 키 반환. (정렬해서)
	//==============================================================================
	pub fn getKeys() -> Vec<String> {
		let mut keys = storageBackend::getAllKeys();
		keys.sort();
		keys
	}
}

//==============================================================================
// 숫자 → 글. (원본 String(number): NaN, Infinity, -Infinity, -0 은 "0")
//==============================================================================
fn formatNumber(numberValue: f64) -> String {
	if numberValue.is_nan() {
		return String::from("NaN");
	}
	if numberValue.is_infinite() {
		if numberValue > 0.0 {
			return String::from("Infinity");
		}
		return String::from("-Infinity");
	}
	if numberValue == 0.0 {
		return String::from("0");
	}
	format!("{}", numberValue)
}

//==============================================================================
// 글 → 숫자. (원본 Number(text): 앞뒤 공백을 무시하고, 빈 글은 0, 숫자가 아니면 NaN)
//==============================================================================
fn parseNumber(text: &str) -> f64 {
	let trimmedText = text.trim();
	if trimmedText.is_empty() {
		return 0.0;
	}
	match trimmedText {
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
	let loweredText = trimmedText.to_ascii_lowercase();
	if loweredText.contains("inf") || loweredText.contains("nan") {
		return f64::NAN;
	}
	trimmedText.parse::<f64>().unwrap_or(f64::NAN)
}

//==============================================================================
// 데스크탑 저장소. (앱 폴더의 JSON 파일 하나)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod storageBackend {
	use std::cell::RefCell;
	use std::collections::BTreeMap;
	use std::path::{Path, PathBuf};

	// 저장 파일 이름.
	const STORAGE_FILE_NAME: &str = "localstorage.json";

	//==============================================================================
	// 데스크탑 저장소 상태. (entries 는 처음 쓸 때 파일에서 읽는다)
	//==============================================================================
	struct DesktopStorageState {
		applicationName: Option<String>,
		storageDirectoryPath: Option<PathBuf>,
		entries: Option<BTreeMap<String, String>>,
	}

	thread_local! {
		static DESKTOP_STORAGE_STATE: RefCell<DesktopStorageState> = const {
			RefCell::new(DesktopStorageState {
				applicationName: None,
				storageDirectoryPath: None,
				entries: None,
			})
		};
	}

	//==============================================================================
	// 폴더 이름에 못 쓰는 글자를 '_' 로 바꾼다. (비면 "vanilla")
	//==============================================================================
	fn sanitizeFolderName(name: &str) -> String {
		let sanitizedName: String = name
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
		let trimmedName = sanitizedName.trim_end_matches(['.', ' ']).to_string();
		if trimmedName.is_empty() {
			return String::from("vanilla");
		}
		trimmedName
	}

	//==============================================================================
	// 기본 앱 이름. (실행 파일 이름, 못 읽으면 "vanilla")
	//==============================================================================
	fn getDefaultApplicationName() -> String {
		let executablePath = match std::env::current_exe() {
			Ok(executablePath) => {
				executablePath
			}
			Err(_) => {
				return String::from("vanilla");
			}
		};
		match executablePath.file_stem() {
			Some(fileStem) => {
				fileStem.to_string_lossy().into_owned()
			}
			None => {
				String::from("vanilla")
			}
		}
	}

	//==============================================================================
	// 환경 변수 경로. (비었거나 상대 경로면 None)
	//==============================================================================
	fn readDirectoryVariable(variableName: &str) -> Option<PathBuf> {
		let variableValue = std::env::var_os(variableName)?;
		let directoryPath = PathBuf::from(variableValue);
		if directoryPath.as_os_str().is_empty() || !directoryPath.is_absolute() {
			return None;
		}
		Some(directoryPath)
	}

	//==============================================================================
	// 운영체제별 앱 데이터 폴더. (앱 이름을 붙이기 전)
	//==============================================================================
	fn getApplicationDataDirectory() -> PathBuf {
		#[cfg(target_os = "windows")]
		let dataDirectoryPath = readDirectoryVariable("APPDATA").or_else(|| readDirectoryVariable("USERPROFILE").map(|userProfilePath| userProfilePath.join("AppData").join("Roaming")));
		#[cfg(target_os = "macos")]
		let dataDirectoryPath = readDirectoryVariable("HOME").map(|homePath| homePath.join("Library").join("Application Support"));
		#[cfg(not(any(target_os = "windows", target_os = "macos")))]
		let dataDirectoryPath = readDirectoryVariable("XDG_DATA_HOME").or_else(|| readDirectoryVariable("HOME").map(|homePath| homePath.join(".local").join("share")));
		match dataDirectoryPath {
			Some(dataDirectoryPath) => {
				dataDirectoryPath
			}
			None => {
				std::env::temp_dir()
			}
		}
	}

	//==============================================================================
	// 저장 파일 경로 계산.
	//==============================================================================
	fn resolveStorageFilePath(state: &DesktopStorageState) -> PathBuf {
		if let Some(storageDirectoryPath) = &state.storageDirectoryPath {
			return storageDirectoryPath.join(STORAGE_FILE_NAME);
		}
		let applicationName = match &state.applicationName {
			Some(applicationName) => {
				applicationName.clone()
			}
			None => {
				getDefaultApplicationName()
			}
		};
		let folderName = sanitizeFolderName(&applicationName);
		let applicationDataDirectory = getApplicationDataDirectory();
		applicationDataDirectory.join(folderName).join(STORAGE_FILE_NAME)
	}

	//==============================================================================
	// 파일에서 읽기. (없거나 깨졌으면 빈 저장소. 문자열이 아닌 값은 JSON 글로 바꿔 둔다)
	//==============================================================================
	fn readEntriesFromFile(storageFilePath: &Path) -> BTreeMap<String, String> {
		let mut entries: BTreeMap<String, String> = BTreeMap::new();
		let fileText = match std::fs::read_to_string(storageFilePath) {
			Ok(fileText) => {
				fileText
			}
			Err(_) => {
				return entries;
			}
		};
		let parsedValue: serde_json::Value = match serde_json::from_str(&fileText) {
			Ok(parsedValue) => {
				parsedValue
			}
			Err(error) => {
				eprintln!("LocalStorage: 저장 파일을 읽지 못했습니다 {} ({})", storageFilePath.display(), error);
				return entries;
			}
		};
		if let serde_json::Value::Object(objectMap) = parsedValue {
			for (key, value) in objectMap {
				let stringValue = match value {
					serde_json::Value::String(stringValue) => {
						stringValue
					}
					otherValue => {
						otherValue.to_string()
					}
				};
				entries.insert(key, stringValue);
			}
		}
		entries
	}

	//==============================================================================
	// 파일에 쓰기. (임시 파일에 쓰고 바꿔치기해서 중간에 꺼져도 깨지지 않게 한다)
	//==============================================================================
	fn writeEntriesToFile(storageFilePath: &Path, entries: &BTreeMap<String, String>) {
		if let Some(parentDirectoryPath) = storageFilePath.parent() {
			if let Err(error) = std::fs::create_dir_all(parentDirectoryPath) {
				eprintln!("LocalStorage: 저장 폴더를 만들지 못했습니다 {} ({})", parentDirectoryPath.display(), error);
				return;
			}
		}
		let fileText = match serde_json::to_string_pretty(entries) {
			Ok(fileText) => {
				fileText
			}
			Err(error) => {
				eprintln!("LocalStorage: 저장 내용을 만들지 못했습니다 ({})", error);
				return;
			}
		};
		let temporaryFilePath = storageFilePath.with_extension("json.tmp");
		let writeResult = std::fs::write(&temporaryFilePath, fileText).and_then(|_| std::fs::rename(&temporaryFilePath, storageFilePath));
		if let Err(error) = writeResult {
			eprintln!("LocalStorage: 저장하지 못했습니다 {} ({})", storageFilePath.display(), error);
		}
	}

	//==============================================================================
	// 저장 내용을 (필요하면 읽어 와서) 읽는다.
	//==============================================================================
	fn withEntries<R>(reader: impl FnOnce(&BTreeMap<String, String>) -> R) -> R {
		DESKTOP_STORAGE_STATE.with(|stateCell| {
			let mut state = stateCell.borrow_mut();
			if state.entries.is_none() {
				let storageFilePath = resolveStorageFilePath(&state);
				state.entries = Some(readEntriesFromFile(&storageFilePath));
			}
			let entries = state.entries.get_or_insert_with(BTreeMap::new);
			reader(entries)
		})
	}

	//==============================================================================
	// 저장 내용을 바꾸고, 바뀌었으면 파일에 쓴다. (writer 가 바뀌었는지를 돌려준다)
	//==============================================================================
	fn modifyEntries(writer: impl FnOnce(&mut BTreeMap<String, String>) -> bool) {
		DESKTOP_STORAGE_STATE.with(|stateCell| {
			let mut state = stateCell.borrow_mut();
			let storageFilePath = resolveStorageFilePath(&state);
			if state.entries.is_none() {
				state.entries = Some(readEntriesFromFile(&storageFilePath));
			}
			let entries = state.entries.get_or_insert_with(BTreeMap::new);
			let isChanged = writer(entries);
			if isChanged {
				writeEntriesToFile(&storageFilePath, entries);
			}
		});
	}

	//==============================================================================
	// 앱 이름 설정. (읽어 둔 내용은 버리고 다음에 새 폴더에서 읽는다)
	//==============================================================================
	pub fn setApplicationName(applicationName: &str) {
		DESKTOP_STORAGE_STATE.with(|stateCell| {
			let mut state = stateCell.borrow_mut();
			state.applicationName = Some(applicationName.to_string());
			state.entries = None;
		});
	}

	//==============================================================================
	// 저장 폴더 직접 설정. (읽어 둔 내용은 버린다)
	//==============================================================================
	pub fn setStorageDirectory(storageDirectoryPath: Option<PathBuf>) {
		DESKTOP_STORAGE_STATE.with(|stateCell| {
			let mut state = stateCell.borrow_mut();
			state.storageDirectoryPath = storageDirectoryPath;
			state.entries = None;
		});
	}

	//==============================================================================
	// 저장 파일 경로.
	//==============================================================================
	pub fn getStorageFilePath() -> Option<PathBuf> {
		DESKTOP_STORAGE_STATE.with(|stateCell| {
			let state = stateCell.borrow();
			Some(resolveStorageFilePath(&state))
		})
	}

	//==============================================================================
	// 값 읽기.
	//==============================================================================
	pub fn getItem(key: &str) -> Option<String> {
		withEntries(|entries| entries.get(key).cloned())
	}

	//==============================================================================
	// 값 쓰기.
	//==============================================================================
	pub fn setItem(key: &str, value: &str) {
		modifyEntries(|entries| {
			let previousValue = entries.insert(key.to_string(), value.to_string());
			previousValue.as_deref() != Some(value)
		});
	}

	//==============================================================================
	// 값 지우기.
	//==============================================================================
	pub fn removeItem(key: &str) {
		modifyEntries(|entries| entries.remove(key).is_some());
	}

	//==============================================================================
	// 전부 지우기.
	//==============================================================================
	pub fn clearAll() {
		modifyEntries(|entries| {
			entries.clear();
			true
		});
	}

	//==============================================================================
	// 모든 키. (저장 순서)
	//==============================================================================
	pub fn getAllKeys() -> Vec<String> {
		withEntries(|entries| entries.keys().cloned().collect())
	}
}

//==============================================================================
// 웹 저장소. (window.localStorage)
//==============================================================================
#[cfg(target_arch = "wasm32")]
mod storageBackend {
	use std::path::PathBuf;

	//==============================================================================
	// window.localStorage. (막혀 있으면 None)
	//==============================================================================
	fn getStorage() -> Option<web_sys::Storage> {
		let window = web_sys::window()?;
		window.local_storage().ok().flatten()
	}

	//==============================================================================
	// 앱 이름 설정. (웹은 출처마다 따로라 쓰지 않는다)
	//==============================================================================
	pub fn setApplicationName(_applicationName: &str) {
	}

	//==============================================================================
	// 저장 폴더 직접 설정. (웹은 쓰지 않는다)
	//==============================================================================
	pub fn setStorageDirectory(_storageDirectoryPath: Option<PathBuf>) {
	}

	//==============================================================================
	// 저장 파일 경로. (웹은 없다)
	//==============================================================================
	pub fn getStorageFilePath() -> Option<PathBuf> {
		None
	}

	//==============================================================================
	// 값 읽기.
	//==============================================================================
	pub fn getItem(key: &str) -> Option<String> {
		let storage = getStorage()?;
		storage.get_item(key).ok().flatten()
	}

	//==============================================================================
	// 값 쓰기. (용량이 넘치면 조용히 실패한다)
	//==============================================================================
	pub fn setItem(key: &str, value: &str) {
		if let Some(storage) = getStorage() {
			let _ = storage.set_item(key, value);
		}
	}

	//==============================================================================
	// 값 지우기.
	//==============================================================================
	pub fn removeItem(key: &str) {
		if let Some(storage) = getStorage() {
			let _ = storage.remove_item(key);
		}
	}

	//==============================================================================
	// 전부 지우기.
	//==============================================================================
	pub fn clearAll() {
		if let Some(storage) = getStorage() {
			let _ = storage.clear();
		}
	}

	//==============================================================================
	// 모든 키. (브라우저가 주는 순서)
	//==============================================================================
	pub fn getAllKeys() -> Vec<String> {
		let mut keys: Vec<String> = Vec::new();
		let storage = match getStorage() {
			Some(storage) => {
				storage
			}
			None => {
				return keys;
			}
		};
		let keyCount = storage.length().unwrap_or(0);
		for keyIndex in 0..keyCount {
			if let Ok(Some(key)) = storage.key(keyIndex) {
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
	fn useTemporaryStorageDirectory(testName: &str) -> PathBuf {
		let uniqueSuffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0);
		let storageDirectoryPath = std::env::temp_dir().join(format!("vanilla-localstorage-{}-{}-{}", testName, std::process::id(), uniqueSuffix));
		LocalStorage::setStorageDirectory(Some(storageDirectoryPath.clone()));
		storageDirectoryPath
	}

	//==============================================================================
	// 숫자 글 바꾸기는 원본 String / Number 와 같다.
	//==============================================================================
	#[test]
	fn numberTextMatchesJavaScript() {
		assert_eq!(formatNumber(1.0), "1");
		assert_eq!(formatNumber(1.5), "1.5");
		assert_eq!(formatNumber(-0.0), "0");
		assert_eq!(formatNumber(f64::NAN), "NaN");
		assert_eq!(formatNumber(f64::NEG_INFINITY), "-Infinity");
		assert_eq!(parseNumber(" 12 "), 12.0);
		assert_eq!(parseNumber("Infinity"), f64::INFINITY);
		assert!(parseNumber("abc").is_nan());
		assert!(parseNumber("inf").is_nan());
	}

	//==============================================================================
	// 쓰고, 파일에서 다시 읽고, 기본값 저장, 접두사 지우기, 키 목록.
	//==============================================================================
	#[test]
	fn storageRoundTripThroughFile() {
		let storageDirectoryPath = useTemporaryStorageDirectory("roundtrip");
		LocalStorage::setString("game.name", "바닐라");
		LocalStorage::setBoolean("game.sound", false);
		LocalStorage::setNumber("game.best", 1234.5);
		LocalStorage::setString("other", "x");
		let storageFilePath = LocalStorage::getStorageFilePath().expect("storage file path");
		assert_eq!(storageFilePath, storageDirectoryPath.join("localstorage.json"));
		assert!(storageFilePath.exists());

		// 다시 읽게 해도 값이 남아 있다.
		LocalStorage::setStorageDirectory(Some(storageDirectoryPath.clone()));
		assert_eq!(LocalStorage::getString("game.name", ""), "바닐라");
		assert!(!LocalStorage::getBoolean("game.sound", true));
		assert_eq!(LocalStorage::getNumber("game.best", 0.0), 1234.5);
		assert!(LocalStorage::containsKey("other"));

		// 없는 키는 기본값을 저장하고 돌려준다.
		assert_eq!(LocalStorage::getNumber("game.level", 3.0), 3.0);
		assert_eq!(LocalStorage::getString("game.level", "9"), "3");
		assert!(LocalStorage::getBoolean("game.vibration", true));
		assert_eq!(LocalStorage::getString("game.vibration", ""), "true");

		assert_eq!(LocalStorage::getKeys(), vec!["game.best", "game.level", "game.name", "game.sound", "game.vibration", "other"]);
		assert_eq!(LocalStorage::clearByPrefix("game."), 5);
		assert_eq!(LocalStorage::clearByPrefix(""), 0);
		assert_eq!(LocalStorage::getKeys(), vec!["other"]);
		LocalStorage::remove("other");
		assert!(!LocalStorage::containsKey("other"));
		LocalStorage::setString("again", "1");
		LocalStorage::clear();
		LocalStorage::setStorageDirectory(Some(storageDirectoryPath.clone()));
		assert!(LocalStorage::getKeys().is_empty());

		let _ = std::fs::remove_dir_all(&storageDirectoryPath);
	}
}
