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
//   숫자는 원본(JS number)처럼 f64 다. 글로 바꾸기는 원본 String(number) 그대로다. (가장 짧은 표기, 1e21 이상과 1e-7 이하는 1e+21 같은 지수, NaN, Infinity, -0 → "0")
//     읽기는 원본 Number(text) 그대로다. (JS 빈칸을 떼고, 빈 글은 0, 0x, 0o, 0b 표기, "1.", ".5", 지수, Infinity, 그 밖은 NaN)
//   데스크탑 저장은 스레드마다 따로 들고 있다가 파일에 쓴다. (엔진은 한 스레드에서 돈다)
// - 저장소 갈아 끼우기: set_backend 에 LocalStorageBackend 를 넘기면 위 기본 저장소 대신 그것을 쓴다. (플랫폼 SDK 의 저장소 같은 것)
//   원본 setBackend 의 객체({ getItem, setItem, removeItem, getKeys, clear }) 자리가 트레이트다. 원본의 예외는 Err 로 알린다.
//   넘긴 저장소는 Rc<RefCell> 에 담는다. get_backend 는 그 Rc 를 돌려주고, 부르는 쪽은 borrow / borrow_mut 로 빌려 쓴다.
// - 막혀도 죽지 않는다: 저장소가 없거나(웹의 막힌 localStorage) 쓰기가 실패하면(가득 참, 폴더를 못 만듦, 바꿔 낀 저장소의 Err)
//   이 실행이 살아 있는 동안 메모리에 대신 둔다. (is_memory_fallback, 다시 켜면 없다)
// - 읽기(get_string, get_boolean, get_number, get_json)는 없는 키에 기본값을 써 넣지 않는다. (원본과 같다)
//==============================================================================
use crate::base::format::number_to_string;
use crate::core::engine::Engine;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

//==============================================================================
// 바꿔 끼우는 저장소. (원본 setBackend 에 넘기는 객체)
// - get_keys 와 clear 는 없어도 된다. get_keys 가 없으면 키 목록(get_keys, clear_by_prefix)은 메모리에 대신 둔 것만 보이고,
//   clear 가 없으면 get_keys 의 키를 하나씩 지운다.
// - 이 함수들 안에서 LocalStorage 를 다시 부르면 기본 저장소를 쓴다. (바꿔 낀 저장소는 부르는 동안 빌려 있다)
//   get_backend 로 꺼낸 저장소를 빌린 채로 LocalStorage 를 불러도 기본 저장소를 쓴다.
//==============================================================================
pub trait LocalStorageBackend {
	//==============================================================================
	// 값 읽기. (없으면 Ok(None))
	//==============================================================================
	fn get_item(&self, key: &str) -> Result<Option<String>, String>;

	//==============================================================================
	// 값 쓰기.
	//==============================================================================
	fn set_item(&mut self, key: &str, value: &str) -> Result<(), String>;

	//==============================================================================
	// 값 지우기.
	//==============================================================================
	fn remove_item(&mut self, key: &str) -> Result<(), String>;

	//==============================================================================
	// 모든 키. (기본은 빈 목록)
	//==============================================================================
	fn get_keys(&self) -> Result<Vec<String>, String> {
		Ok(Vec::new())
	}

	//==============================================================================
	// 전부 지우기. (기본은 get_keys 의 키를 하나씩 지운다)
	//==============================================================================
	fn clear(&mut self) -> Result<(), String> {
		let keys = self.get_keys()?;
		for key in keys.iter() {
			self.remove_item(key)?;
		}
		Ok(())
	}
}

//==============================================================================
// 바꿔 낀 저장소와 메모리 대신 둔 값.
//==============================================================================
struct StorageOverrideState {
	backend: Option<Rc<RefCell<Box<dyn LocalStorageBackend>>>>,
	memory_item_map: BTreeMap<String, String>,
	is_memory_fallback: bool,
}

thread_local! {
	static STORAGE_OVERRIDE_STATE: RefCell<StorageOverrideState> = const {
		RefCell::new(StorageOverrideState {
			backend: None,
			memory_item_map: BTreeMap::new(),
			is_memory_fallback: false,
		})
	};
}

//==============================================================================
// 바꿔 낀 저장소로 일하기. (바꿔 낀 것이 없거나 이미 빌려 있으면 None. 안에서 LocalStorage 를 다시 부르면 빌려 있어 기본 저장소를 쓴다)
//==============================================================================
fn with_backend<R>(worker: impl FnOnce(&mut dyn LocalStorageBackend) -> R) -> Option<R> {
	let backend = STORAGE_OVERRIDE_STATE.with(|state_cell| state_cell.borrow().backend.clone())?;
	let mut backend_reference = backend.try_borrow_mut().ok()?;
	let result = worker(backend_reference.as_mut());
	Some(result)
}

//==============================================================================
// 메모리에 대신 둔 값 만지기.
//==============================================================================
fn with_memory_item_map<R>(worker: impl FnOnce(&mut BTreeMap<String, String>) -> R) -> R {
	STORAGE_OVERRIDE_STATE.with(|state_cell| worker(&mut state_cell.borrow_mut().memory_item_map))
}

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
	// 저장소 갈아 끼우기. (None 이면 기본 저장소로 돌아간다. 메모리에 대신 둔 값은 버린다)
	//==============================================================================
	pub fn set_backend(backend: Option<Box<dyn LocalStorageBackend>>) {
		STORAGE_OVERRIDE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			state.backend = backend.map(|backend| Rc::new(RefCell::new(backend)));
			state.memory_item_map.clear();
			state.is_memory_fallback = false;
		});
	}

	//==============================================================================
	// 갈아 끼운 저장소 반환. (없으면 None, 빌릴 때는 borrow / borrow_mut)
	//==============================================================================
	pub fn get_backend() -> Option<Rc<RefCell<Box<dyn LocalStorageBackend>>>> {
		STORAGE_OVERRIDE_STATE.with(|state_cell| state_cell.borrow().backend.clone())
	}

	//==============================================================================
	// 저장소를 못 써서 메모리에 대신 둔 적이 있는지.
	//==============================================================================
	pub fn is_memory_fallback() -> bool {
		STORAGE_OVERRIDE_STATE.with(|state_cell| state_cell.borrow().is_memory_fallback)
	}

	//==============================================================================
	// 값 하나 읽기. (없거나 못 읽으면 None. 메모리에 대신 둔 값이 앞선다)
	//==============================================================================
	pub fn get_item(key: &str) -> Option<String> {
		if let Some(memory_value) = with_memory_item_map(|memory_item_map| memory_item_map.get(key).cloned()) {
			return Some(memory_value);
		}
		match with_backend(|backend| backend.get_item(key)) {
			Some(Ok(value)) => {
				value
			}
			Some(Err(_)) => {
				None
			}
			None => {
				storage_backend::get_item(key)
			}
		}
	}

	//==============================================================================
	// 값 하나 쓰기. (저장소에 썼으면 true, 못 써서 메모리에 대신 뒀으면 false)
	//==============================================================================
	pub fn set_item(key: &str, value: &str) -> bool {
		let is_written = match with_backend(|backend| backend.set_item(key, value)) {
			Some(Ok(())) => {
				true
			}
			Some(Err(error_message)) => {
				eprintln!("LocalStorage: 저장소에 쓰지 못해 메모리에 둡니다 {} ({})", key, error_message);
				false
			}
			None => {
				storage_backend::set_item(key, value)
			}
		};
		STORAGE_OVERRIDE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			if is_written {
				state.memory_item_map.remove(key);
			}
			else {
				state.memory_item_map.insert(key.to_string(), value.to_string());
				state.is_memory_fallback = true;
			}
		});
		is_written
	}

	//==============================================================================
	// 값 하나 지우기. (메모리에 대신 둔 값도 지운다)
	//==============================================================================
	pub fn remove_item(key: &str) {
		with_memory_item_map(|memory_item_map| {
			memory_item_map.remove(key);
		});
		match with_backend(|backend| backend.remove_item(key)) {
			Some(Ok(())) => {
			}
			Some(Err(error_message)) => {
				eprintln!("LocalStorage: 저장소에서 지우지 못했습니다 {} ({})", key, error_message);
			}
			None => {
				storage_backend::remove_item(key);
			}
		}
	}

	//==============================================================================
	// 전체 제거.
	//==============================================================================
	pub fn clear() {
		with_memory_item_map(|memory_item_map| {
			memory_item_map.clear();
		});
		match with_backend(|backend| backend.clear()) {
			Some(Ok(())) => {
			}
			Some(Err(error_message)) => {
				eprintln!("LocalStorage: 저장소를 비우지 못했습니다 ({})", error_message);
			}
			None => {
				storage_backend::clear_all();
			}
		}
	}

	//==============================================================================
	// 문자열 값 설정.
	//==============================================================================
	pub fn set_string(key: &str, string_value: &str) {
		LocalStorage::set_item(key, string_value);
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
		LocalStorage::remove_item(key);
	}

	//==============================================================================
	// 문자열 값 반환. (없으면 기본값. 저장소에는 쓰지 않는다)
	//==============================================================================
	pub fn get_string(key: &str, default_string_value: &str) -> String {
		match LocalStorage::get_item(key) {
			Some(string_value) => {
				string_value
			}
			None => {
				default_string_value.to_string()
			}
		}
	}

	//==============================================================================
	// 논리 값 반환. (없거나 빈 글이면 기본값, 저장소에는 쓰지 않는다. "true" 만 참)
	//==============================================================================
	pub fn get_boolean(key: &str, default_boolean_value: bool) -> bool {
		let string_value = LocalStorage::get_string(key, "");
		if string_value.is_empty() {
			default_boolean_value
		}
		else {
			string_value == "true"
		}
	}

	//==============================================================================
	// 숫자 값 반환. (없거나 빈 글이면 기본값, 저장소에는 쓰지 않는다. 숫자가 아니면 NaN, 원본 Number(text))
	//==============================================================================
	pub fn get_number(key: &str, default_number_value: f64) -> f64 {
		let string_value = LocalStorage::get_string(key, "");
		if string_value.is_empty() {
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
		let remove_key_list: Vec<String> = LocalStorage::get_keys().into_iter().filter(|key| key.starts_with(prefix)).collect();
		for key in remove_key_list.iter() {
			LocalStorage::remove_item(key);
		}
		remove_key_list.len()
	}

	//==============================================================================
	// 키가 있는지.
	//==============================================================================
	pub fn contains_key(key: &str) -> bool {
		LocalStorage::get_item(key).is_some()
	}

	//==============================================================================
	// 모든 키 반환. (정렬해서, 메모리에 대신 둔 키도 넣는다)
	//==============================================================================
	pub fn get_keys() -> Vec<String> {
		let mut keys = match with_backend(|backend| backend.get_keys()) {
			Some(Ok(backend_keys)) => {
				backend_keys
			}
			Some(Err(error_message)) => {
				eprintln!("LocalStorage: 키 목록을 읽지 못했습니다 ({})", error_message);
				Vec::new()
			}
			None => {
				storage_backend::get_all_keys()
			}
		};
		with_memory_item_map(|memory_item_map| {
			for key in memory_item_map.keys() {
				keys.push(key.clone());
			}
		});
		keys.sort();
		keys.dedup();
		keys
	}

	//==============================================================================
	// JSON 값 반환. (없거나 풀 수 없으면 기본값, 저장소에는 쓰지 않는다)
	//==============================================================================
	pub fn get_json(key: &str, default_value: serde_json::Value) -> serde_json::Value {
		let string_value = match LocalStorage::get_item(key) {
			Some(string_value) => {
				string_value
			}
			None => {
				return default_value;
			}
		};
		if string_value.is_empty() {
			return default_value;
		}
		serde_json::from_str(&string_value).unwrap_or(default_value)
	}

	//==============================================================================
	// JSON 값 설정. (JSON 글로 바꿔 쓴다. 저장소에 썼으면 true)
	//==============================================================================
	pub fn set_json(key: &str, value: &serde_json::Value) -> bool {
		let string_value = value.to_string();
		LocalStorage::set_item(key, &string_value)
	}
}

//==============================================================================
// 숫자 → 글. (원본 String(number), Number.prototype.toString 꼴)
//==============================================================================
fn format_number(number_value: f64) -> String {
	number_to_string(number_value)
}

//==============================================================================
// JS 빈칸인지. (원본 Number(text) 가 앞뒤에서 떼는 WhiteSpace 와 LineTerminator: 탭, VT, FF, BOM, Zs 빈칸, LF, CR, LS, PS)
//==============================================================================
fn is_java_script_whitespace(character: char) -> bool {
	matches!(character, '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | '\u{20}' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

//==============================================================================
// 글 → 숫자. (원본 Number(text): 앞뒤 JS 빈칸을 떼고, 빈 글은 0, 0x / 0o / 0b 정수, 부호 붙은 십진 표기와 Infinity, 그 밖은 NaN)
//==============================================================================
fn parse_number(text: &str) -> f64 {
	let trimmed_text = text.trim_matches(is_java_script_whitespace);
	if trimmed_text.is_empty() {
		return 0.0;
	}
	let text_bytes = trimmed_text.as_bytes();
	if text_bytes.len() > 2 && text_bytes[0] == b'0' {
		let radix = match text_bytes[1] {
			b'x' | b'X' => {
				16
			}
			b'o' | b'O' => {
				8
			}
			b'b' | b'B' => {
				2
			}
			_ => {
				0
			}
		};
		if radix != 0 {
			return parse_power_of_two_radix_integer(&trimmed_text[2..], radix);
		}
	}
	let (sign, unsigned_text) = match text_bytes[0] {
		b'+' => {
			(1.0, &trimmed_text[1..])
		}
		b'-' => {
			(-1.0, &trimmed_text[1..])
		}
		_ => {
			(1.0, trimmed_text)
		}
	};
	if unsigned_text == "Infinity" {
		return sign * f64::INFINITY;
	}
	if !is_unsigned_decimal_literal(unsigned_text) {
		return f64::NAN;
	}
	trimmed_text.parse::<f64>().unwrap_or(f64::NAN)
}

//==============================================================================
// 부호 없는 십진 표기인지. (원본 StrUnsignedDecimalLiteral: 숫자들 [. 숫자들] [지수] 또는 . 숫자들 [지수], 지수는 e 나 E 뒤 [+ -] 숫자들)
//==============================================================================
fn is_unsigned_decimal_literal(text: &str) -> bool {
	let text_bytes = text.as_bytes();
	let mut byte_index = 0;
	let mut digit_count = 0;
	while byte_index < text_bytes.len() && text_bytes[byte_index].is_ascii_digit() {
		byte_index += 1;
		digit_count += 1;
	}
	if byte_index < text_bytes.len() && text_bytes[byte_index] == b'.' {
		byte_index += 1;
		while byte_index < text_bytes.len() && text_bytes[byte_index].is_ascii_digit() {
			byte_index += 1;
			digit_count += 1;
		}
	}
	if digit_count == 0 {
		return false;
	}
	if byte_index < text_bytes.len() && (text_bytes[byte_index] == b'e' || text_bytes[byte_index] == b'E') {
		byte_index += 1;
		if byte_index < text_bytes.len() && (text_bytes[byte_index] == b'+' || text_bytes[byte_index] == b'-') {
			byte_index += 1;
		}
		let exponent_start_index = byte_index;
		while byte_index < text_bytes.len() && text_bytes[byte_index].is_ascii_digit() {
			byte_index += 1;
		}
		if byte_index == exponent_start_index {
			return false;
		}
	}
	byte_index == text_bytes.len()
}

//==============================================================================
// 2, 8, 16 진 정수 글 → 숫자. (원본처럼 정확한 값을 가장 가까운 f64 로, 딱 가운데면 짝수 쪽, 숫자가 아닌 글자가 있거나 비었으면 NaN)
//==============================================================================
fn parse_power_of_two_radix_integer(digit_text: &str, radix: u32) -> f64 {
	if digit_text.is_empty() {
		return f64::NAN;
	}
	let bits_per_digit = radix.trailing_zeros();
	let mut bit_list: Vec<u8> = Vec::with_capacity(digit_text.len() * bits_per_digit as usize);
	for digit_character in digit_text.chars() {
		let digit_value = match digit_character.to_digit(radix) {
			Some(digit_value) => {
				digit_value
			}
			None => {
				return f64::NAN;
			}
		};
		for bit_index in (0..bits_per_digit).rev() {
			bit_list.push(((digit_value >> bit_index) & 1) as u8);
		}
	}
	let first_one_index = match bit_list.iter().position(|bit| *bit == 1) {
		Some(first_one_index) => {
			first_one_index
		}
		None => {
			return 0.0;
		}
	};
	let significant_bit_list = &bit_list[first_one_index..];
	let mantissa_bit_count = 53usize.min(significant_bit_list.len());
	let mut mantissa: u64 = 0;
	for bit in significant_bit_list[..mantissa_bit_count].iter() {
		mantissa = (mantissa << 1) | *bit as u64;
	}
	let dropped_bit_count = significant_bit_list.len() - mantissa_bit_count;
	if dropped_bit_count > 0 {
		let is_round_bit_set = significant_bit_list[mantissa_bit_count] == 1;
		let is_sticky_bit_set = significant_bit_list[mantissa_bit_count + 1..].contains(&1);
		if is_round_bit_set && (is_sticky_bit_set || mantissa & 1 == 1) {
			mantissa += 1;
		}
	}
	let scale_exponent = dropped_bit_count.min(2000) as i32;
	mantissa as f64 * 2.0f64.powi(scale_exponent)
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
	fn write_entries_to_file(storage_file_path: &Path, entries: &BTreeMap<String, String>) -> bool {
		if let Some(parent_directory_path) = storage_file_path.parent() {
			if let Err(error) = std::fs::create_dir_all(parent_directory_path) {
				eprintln!("LocalStorage: 저장 폴더를 만들지 못했습니다 {} ({})", parent_directory_path.display(), error);
				return false;
			}
		}
		let file_text = match serde_json::to_string_pretty(entries) {
			Ok(file_text) => {
				file_text
			}
			Err(error) => {
				eprintln!("LocalStorage: 저장 내용을 만들지 못했습니다 ({})", error);
				return false;
			}
		};
		let temporary_file_path = storage_file_path.with_extension("json.tmp");
		let write_result = std::fs::write(&temporary_file_path, file_text).and_then(|_| std::fs::rename(&temporary_file_path, storage_file_path));
		if let Err(error) = write_result {
			eprintln!("LocalStorage: 저장하지 못했습니다 {} ({})", storage_file_path.display(), error);
			return false;
		}
		true
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
	// 저장 내용을 바꾸고, 바뀌었으면 파일에 쓴다. (writer 가 바뀌었는지를 돌려준다. 파일에 못 썼으면 false)
	//==============================================================================
	fn modify_entries(writer: impl FnOnce(&mut BTreeMap<String, String>) -> bool) -> bool {
		DESKTOP_STORAGE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			let storage_file_path = resolve_storage_file_path(&state);
			if state.entries.is_none() {
				state.entries = Some(read_entries_from_file(&storage_file_path));
			}
			let entries = state.entries.get_or_insert_with(BTreeMap::new);
			let is_changed = writer(entries);
			if is_changed {
				return write_entries_to_file(&storage_file_path, entries);
			}
			true
		})
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
	// 값 쓰기. (파일에 썼는지)
	//==============================================================================
	pub fn set_item(key: &str, value: &str) -> bool {
		modify_entries(|entries| {
			let previous_value = entries.insert(key.to_string(), value.to_string());
			previous_value.as_deref() != Some(value)
		})
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
	// 값 쓰기. (썼는지, 막혀 있거나 용량이 넘치면 false)
	//==============================================================================
	pub fn set_item(key: &str, value: &str) -> bool {
		match get_storage() {
			Some(storage) => {
				storage.set_item(key, value).is_ok()
			}
			None => {
				false
			}
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

		// 아래 기대값은 node 의 String(수), Number(글)로 찍은 것이다.
		assert_eq!(format_number(1e21), "1e+21");
		assert_eq!(format_number(1.5e21), "1.5e+21");
		assert_eq!(format_number(123456789012345680000.0), "123456789012345680000");
		assert_eq!(format_number(1e-7), "1e-7");
		assert_eq!(format_number(0.000001), "0.000001");
		assert_eq!(format_number(-2.5e-8), "-2.5e-8");
		assert_eq!(format_number(0.1 + 0.2), "0.30000000000000004");
		assert_eq!(parse_number("0x10"), 16.0);
		assert_eq!(parse_number("0X1f"), 31.0);
		assert_eq!(parse_number("0o17"), 15.0);
		assert_eq!(parse_number("0b101"), 5.0);
		assert!(parse_number("-0x10").is_nan());
		assert!(parse_number("0x").is_nan());
		assert!(parse_number("0x1g").is_nan());
		assert_eq!(parse_number("0x20000000000001"), 9007199254740992.0);
		assert_eq!(parse_number("0x20000000000003"), 9007199254740996.0);
		assert_eq!(parse_number("1."), 1.0);
		assert_eq!(parse_number(".5"), 0.5);
		assert_eq!(parse_number("+.5e1"), 5.0);
		assert_eq!(parse_number("1E+2"), 100.0);
		assert_eq!(parse_number("00012"), 12.0);
		assert_eq!(parse_number("-0").to_bits(), (-0.0f64).to_bits());
		assert_eq!(parse_number("-Infinity"), f64::NEG_INFINITY);
		assert!(parse_number("1e").is_nan());
		assert!(parse_number(".").is_nan());
		assert!(parse_number("1_000").is_nan());
		assert!(parse_number("12px").is_nan());
		assert!(parse_number("infinity").is_nan());
		assert_eq!(parse_number("\u{feff}\u{3000}7\u{2028}"), 7.0);
		assert!(parse_number("\u{85}7").is_nan());
		assert_eq!(parse_number("1e400"), f64::INFINITY);
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

		// 없는 키는 기본값을 돌려주고 저장하지 않는다.
		assert_eq!(LocalStorage::get_number("game.level", 3.0), 3.0);
		assert_eq!(LocalStorage::get_string("game.level", "9"), "9");
		assert!(LocalStorage::get_boolean("game.vibration", true));
		assert!(!LocalStorage::contains_key("game.vibration"));
		LocalStorage::set_number("game.level", 3.0);
		LocalStorage::set_boolean("game.vibration", true);

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

	//==============================================================================
	// JSON 읽기와 쓰기. (깨진 글은 기본값)
	//==============================================================================
	#[test]
	fn json_round_trip() {
		let storage_directory_path = use_temporary_storage_directory("json");
		assert!(LocalStorage::set_json("table", &serde_json::json!({ "a": 1, "b": [2, 3] })));
		assert_eq!(LocalStorage::get_json("table", serde_json::Value::Null)["b"][1], serde_json::json!(3));
		LocalStorage::set_string("broken", "{not json");
		assert_eq!(LocalStorage::get_json("broken", serde_json::json!("default")), serde_json::json!("default"));
		assert_eq!(LocalStorage::get_json("missing", serde_json::json!(7)), serde_json::json!(7));
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	// 시험용 바꿔 끼우는 저장소. (값은 나눠 든 맵, is_blocked 면 모두 Err)
	//==============================================================================
	struct TestBackend {
		item_map: std::rc::Rc<RefCell<BTreeMap<String, String>>>,
		is_blocked: bool,
	}

	impl LocalStorageBackend for TestBackend {
		fn get_item(&self, key: &str) -> Result<Option<String>, String> {
			if self.is_blocked {
				return Err(String::from("blocked"));
			}
			Ok(self.item_map.borrow().get(key).cloned())
		}

		fn set_item(&mut self, key: &str, value: &str) -> Result<(), String> {
			if self.is_blocked {
				return Err(String::from("quota"));
			}
			self.item_map.borrow_mut().insert(key.to_string(), value.to_string());
			Ok(())
		}

		fn remove_item(&mut self, key: &str) -> Result<(), String> {
			if self.is_blocked {
				return Err(String::from("blocked"));
			}
			self.item_map.borrow_mut().remove(key);
			Ok(())
		}

		fn get_keys(&self) -> Result<Vec<String>, String> {
			if self.is_blocked {
				return Err(String::from("blocked"));
			}
			Ok(self.item_map.borrow().keys().cloned().collect())
		}
	}

	//==============================================================================
	// 저장소 갈아 끼우기와 막혔을 때 메모리로 대신하기.
	//==============================================================================
	#[test]
	fn backend_and_memory_fallback() {
		let storage_directory_path = use_temporary_storage_directory("backend");
		let item_map = std::rc::Rc::new(RefCell::new(BTreeMap::new()));
		LocalStorage::set_backend(Some(Box::new(TestBackend { item_map: item_map.clone(), is_blocked: false })));
		let backend = LocalStorage::get_backend().expect("backend");
		assert!(backend.borrow_mut().set_item("direct", "0").is_ok());
		assert_eq!(item_map.borrow().get("direct").map(|value| value.as_str()), Some("0"));
		// 꺼낸 저장소를 빌린 동안에는 기본 저장소를 쓴다.
		{
			let _backend_reference = backend.borrow();
			assert!(LocalStorage::set_item("while.borrowed", "1"));
			assert!(!item_map.borrow().contains_key("while.borrowed"));
			LocalStorage::remove("while.borrowed");
		}
		assert!(backend.borrow_mut().remove_item("direct").is_ok());
		drop(backend);
		LocalStorage::set_string("game.a", "1");
		LocalStorage::set_number("game.b", 2.0);
		LocalStorage::set_string("other", "x");
		assert_eq!(item_map.borrow().get("game.a").map(|value| value.as_str()), Some("1"));
		assert_eq!(LocalStorage::get_keys(), vec!["game.a", "game.b", "other"]);
		assert_eq!(LocalStorage::clear_by_prefix("game."), 2);
		LocalStorage::clear();
		assert!(item_map.borrow().is_empty());

		LocalStorage::set_backend(Some(Box::new(TestBackend { item_map: item_map.clone(), is_blocked: true })));
		assert!(!LocalStorage::set_item("save", "slot1"));
		assert!(LocalStorage::is_memory_fallback());
		assert_eq!(LocalStorage::get_string("save", ""), "slot1");
		assert_eq!(LocalStorage::get_keys(), vec!["save"]);
		LocalStorage::remove("save");
		assert!(!LocalStorage::contains_key("save"));

		// 되돌리면 기본 저장소(파일)를 쓰고, 메모리에 대신 둔 것은 버린다.
		LocalStorage::set_backend(None);
		assert!(LocalStorage::get_backend().is_none());
		assert!(!LocalStorage::is_memory_fallback());
		assert!(LocalStorage::set_item("file.key", "1"));
		assert!(LocalStorage::get_storage_file_path().expect("storage file path").exists());

		// 파일을 못 쓰면(폴더 자리에 파일이 있음) 메모리에 대신 둔다.
		let blocked_directory_path = storage_directory_path.join("blocked");
		std::fs::write(&blocked_directory_path, "file").expect("blocking file");
		LocalStorage::set_storage_directory(Some(blocked_directory_path.join("inner")));
		assert!(!LocalStorage::set_item("desktop.key", "2"));
		assert!(LocalStorage::is_memory_fallback());
		assert_eq!(LocalStorage::get_string("desktop.key", ""), "2");
		LocalStorage::set_backend(None);
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}
}
