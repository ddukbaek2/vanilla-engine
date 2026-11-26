//==============================================================================
//! 세이브 슬롯 관리. (원본 src/experimental/savegame/savegamemanager.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 칸(slot)마다 JSON 자료 하나와 머리(이름, 저장 시각, 판, 미리보기 그림, 크기, 게임이 정한 metadata)를 둔다.
//! - 기록 꼴: `"VSAVE1 <CRC-32 16진 8자>\n<머리 JSON>\n<자료 JSON>"`. CRC-32(IEEE 802.3, zlib 과 같은 다항식)는 둘째 줄부터 끝까지의 UTF-8 에 대해 셈한다.
//!   원본(JS)과 같은 꼴이라 두 엔진이 서로의 세이브를 읽는다. (같은 키를 쓰는 저장소일 때)
//!   머리와 자료 JSON 은 원본 JSON.stringify 와 글자까지 같다. (수는 원본 String(number) 꼴, 1.0 → 1, -0 → 0, 1e21 → 1e+21, NaN 과 Infinity 는 null, 객체 칸은 배열 번호 꼴 이름을 먼저)
//!   그래서 같은 자료면 byteSize 와 CRC 도 같다. 머리의 savedTime, byteSize, sequence 는 0 이상 정수, version 은 u32 안의 0 이상 정수여야 받는다. (원본과 같은 검사)
//! - save 는 Serialize 를 받는다. (Value, 게임의 구조체) 바꾸지 못하면 InvalidData. (원본은 순환, BigInt, 맨 위 undefined)
//! - 원자적 쓰기: 칸마다 키 두 개(`<이름공간>.slot.<칸>.0` 과 `.1`)를 번갈아 쓴다. 새 기록은 앞 기록이 없는 쪽 키에 순번을 하나 올려 쓰고,
//!   읽을 때는 체크섬이 맞는 것 가운데 순번이 가장 큰 것을 고른다. 쓰다 끊기거나(앱이 죽음, 저장소가 가득 참) 기록이 깨져도 앞 저장이 남는다.
//!   한 키 쓰기는 저장소가 다 되거나 안 되게 한다. (웹 localStorage.setItem, 데스크탑 LocalStorage 는 임시 파일에 쓴 뒤 이름을 바꿔 끼운다)
//! - 판 옮기기: 자료에 판(version)을 적고, 읽을 때 지금 판보다 낮으면 add_migration 으로 건 함수를 판마다 차례로 부른다. (v → v + 1)
//!   지금 판보다 높은 세이브(새 판 게임이 쓴 것)는 읽지 않는다.
//! - 자동 저장: set_autosave 로 칸, 간격, 자료를 주는 함수를 걸고 매 프레임 update 를 부른다.
//! - 내보내기와 들여오기: export_slot 은 기록을 그대로 담은 JSON 글, import_slot 은 체크섬을 보고 다른 칸에도 들인다.
//! - 클라우드: set_cloud_provider 로 SaveCloudProvider 를 꽂으면 저장마다 올리고, pull_from_cloud 로 더 새 것을 받아 들인다. (인터페이스만)
//==============================================================================
use super::savestorage::{LocalSaveStorage, SaveCloudProvider, SaveStorage};
use crate::misc::storagetext::{self, JsonSurrogateTable};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

// 기록 머리 줄의 표시와 형식 판.
const RECORD_MAGIC: &str = "VSAVE1";
const RECORD_FORMAT: &str = "vanilla-save";
const EXPORT_FORMAT: &str = "vanilla-save-export";
const FORMAT_VERSION: u64 = 1;
// 칸 이름의 가장 긴 길이.
const MAXIMUM_SLOT_ID_LENGTH: usize = 64;

//==============================================================================
/// CRC-32. (IEEE 802.3, 비트를 거꾸로 돌린 꼴의 다항식 0xEDB88320, 처음과 끝에 0xFFFFFFFF, zlib crc32 와 같다)
///
/// # 인자
/// - `bytes`: 셀 바이트.
///
/// # 돌려줌
/// CRC-32 값.
///
/// # 예제
/// ```
/// use vanilla::experimental::savegame::calculate_crc32;
///
/// assert_eq!(calculate_crc32(b"123456789"), 0xCBF43926);
/// assert_eq!(calculate_crc32(b""), 0);
/// ```
//==============================================================================
pub fn calculate_crc32(bytes: &[u8]) -> u32 {
	const TABLE: [u32; 256] = {
		let mut table = [0u32; 256];
		let mut index = 0;
		while index < 256 {
			let mut value = index as u32;
			let mut bit = 0;
			while bit < 8 {
				value = if value & 1 != 0 { 0xEDB88320 ^ (value >> 1) } else { value >> 1 };
				bit += 1;
			}
			table[index] = value;
			index += 1;
		}
		table
	};
	let mut crc = 0xFFFFFFFFu32;
	for byte in bytes {
		crc = TABLE[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
	}
	crc ^ 0xFFFFFFFF
}

//==============================================================================
/// 칸 하나의 머리.
/// # 예제
/// ```
/// use vanilla::experimental::savegame::{MemorySaveStorage, SaveGameManager, SaveSlotOptions};
/// use serde_json::json;
///
/// let mut save_game_manager = SaveGameManager::new("example.info", 1);
/// save_game_manager.set_storage(Box::new(MemorySaveStorage::new()));
/// let info = save_game_manager.save("slot1", &json!({ "hp": 3 }), &SaveSlotOptions::default()).expect("save");
/// assert_eq!(info.slot_id, "slot1");
/// assert_eq!(info.byte_size, 8);
/// assert!(!info.is_corrupted);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct SaveSlotInfo {
	/// 칸 이름. (영문, 숫자, _, - 로 64 자까지)
	pub slot_id: String,
	/// 보이는 이름. (저장할 때 준 것)
	pub name: String,
	/// 저장한 시각. (1970-01-01 UTC 부터의 밀리초, 원본 Date.now())
	pub saved_time: f64,
	/// 자료의 판.
	pub version: u32,
	/// 미리보기 그림 자리. (데이터 URL 이나 애셋 경로 같은 글, 없으면 빈 글)
	pub preview_image: String,
	/// 자료 JSON 의 UTF-8 바이트 수.
	pub byte_size: usize,
	/// 쓴 차례. (칸마다 1 부터 오른다)
	pub sequence: u64,
	/// 게임이 정한 값. (놀이 시간, 장 이름 같은 것, 없으면 빈 객체)
	pub metadata: Value,
	/// 읽을 수 있는 기록이 없는 칸인지. (두 키가 모두 깨짐, 나머지 칸은 빈 값)
	pub is_corrupted: bool,
}

//==============================================================================
/// 저장할 때 주는 머리 값.
/// # 예제
/// ```
/// use vanilla::experimental::savegame::SaveSlotOptions;
/// use serde_json::json;
///
/// let options = SaveSlotOptions { name: "Chapter 1".to_string(), metadata: json!({ "playSeconds": 600 }), ..SaveSlotOptions::default() };
/// assert_eq!(options.preview_image, "");
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct SaveSlotOptions {
	/// 보이는 이름.
	pub name: String,
	/// 미리보기 그림 자리. (데이터 URL 이나 애셋 경로 같은 글)
	pub preview_image: String,
	/// 게임이 정한 값. (JSON 객체)
	pub metadata: Value,
}

impl Default for SaveSlotOptions {
	//==============================================================================
	/// 빈 이름, 그림 없음, 빈 metadata.
	///
	/// # 돌려줌
	/// 빈 값.
	//==============================================================================
	fn default() -> SaveSlotOptions {
		SaveSlotOptions { name: String::new(), preview_image: String::new(), metadata: json!({}) }
	}
}

//==============================================================================
// JSON 쓰기. (원본 JSON.stringify 와 글자까지 같다, 엔진 공용 storagetext::stringify_json)
///
/// # 인자
/// - `value`: 쓸 값.
///
/// # 돌려줌
/// JSON 글.
//==============================================================================
fn stringify_json(value: &Value) -> String {
	storagetext::stringify_json(value, &JsonSurrogateTable::new())
}

//==============================================================================
// 머리의 0 이상 정수 읽기. (원본 Number.isInteger(value) && value >= 0 과 같다)
///
/// # 인자
/// - `value`: 머리 칸.
///
/// # 돌려줌
/// 수. (아니면 None)
//==============================================================================
fn read_header_integer(value: &Value) -> Option<f64> {
	let number_value = value.as_f64()?;
	if number_value.is_finite() && number_value >= 0.0 && number_value.fract() == 0.0 {
		Some(number_value)
	}
	else {
		None
	}
}

//==============================================================================
// 머리의 판 읽기. (0 이상 정수이고 u32 안이어야 한다, 원본도 같은 검사)
///
/// # 인자
/// - `value`: 머리 칸.
///
/// # 돌려줌
/// 판. (아니면 None)
//==============================================================================
fn read_header_version(value: &Value) -> Option<u32> {
	let number_value = read_header_integer(value)?;
	if number_value <= f64::from(u32::MAX) {
		Some(number_value as u32)
	}
	else {
		None
	}
}

//==============================================================================
// 저장 시각 정하기. (내림, 유한한 0 이상이 아니면 0. 원본과 같다)
///
/// # 인자
/// - `time`: 시각을 주는 함수의 값. (밀리초)
///
/// # 돌려줌
/// 머리에 쓸 저장 시각.
//==============================================================================
fn read_saved_time(time: f64) -> f64 {
	let saved_time = time.floor();
	if saved_time.is_finite() && saved_time > 0.0 {
		saved_time
	}
	else {
		0.0
	}
}

//==============================================================================
/// 세이브 실패 까닭. (원본 SaveGameError 의 이름과 같다)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum SaveGameError {
	/// 칸 이름이 비었거나 쓸 수 없는 글자가 있다.
	InvalidSlotId,
	/// 칸에 기록이 없다.
	NotFound,
	/// 기록이 있으나 모두 깨졌다. (체크섬이 맞지 않거나 꼴이 틀림)
	Corrupted,
	/// 지금 판보다 새 판의 세이브다.
	NewerVersion {
		/// 세이브의 판.
		version: u32,
		/// 지금 판.
		current_version: u32,
	},
	/// 이 판에서 다음 판으로 옮기는 함수가 없다.
	MissingMigration {
		/// 옮길 판.
		from_version: u32,
	},
	/// 판 옮기는 함수가 실패했다.
	MigrationFailed {
		/// 옮기던 판.
		from_version: u32,
		/// 함수가 돌려준 글.
		message: String,
	},
	/// 저장소에 쓰지 못했다. (웹 LocalStorage 는 이 실행 동안 메모리에 대신 둔다)
	WriteFailed,
	/// 들여올 글이 내보낸 꼴이 아니거나 깨졌다.
	InvalidImport,
	/// 자료를 JSON 으로 바꿀 수 없다. (Serialize 가 실패함: 글이 아닌 맵 키, 구현이 낸 오류. 원본은 순환, BigInt, 맨 위 undefined)
	InvalidData,
}

//==============================================================================
/// 읽은 결과.
/// # 예제
/// ```
/// use vanilla::experimental::savegame::{MemorySaveStorage, SaveGameManager, SaveSlotOptions};
/// use serde_json::json;
///
/// let mut save_game_manager = SaveGameManager::new("example.result", 1);
/// save_game_manager.set_storage(Box::new(MemorySaveStorage::new()));
/// save_game_manager.save("slot1", &json!([1, 2]), &SaveSlotOptions::default()).expect("save");
/// let result = save_game_manager.load("slot1").expect("load");
/// assert_eq!(result.data, json!([1, 2]));
/// assert!(!result.is_recovered_from_backup);
/// assert_eq!(result.migrated_from_version, None);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct SaveGameLoadResult {
	/// 자료. (지금 판으로 옮긴 것)
	pub data: Value,
	/// 고른 기록의 머리. (판은 옮기기 전 판)
	pub info: SaveSlotInfo,
	/// 다른 키의 기록이 깨져 남은 쪽을 읽었는지.
	pub is_recovered_from_backup: bool,
	/// 판을 옮겼으면 옮기기 전 판.
	pub migrated_from_version: Option<u32>,
}

// 읽은 기록 하나.
struct SaveRecord {
	info: SaveSlotInfo,
	body_text: String,
	record_text: String,
}

// 판 옮기는 함수.
type MigrationFunction = Box<dyn Fn(Value) -> Result<Value, String>>;
// 자동 저장 자료를 주는 함수.
type AutosaveDataProvider = Box<dyn FnMut() -> Option<Value>>;
// 시각을 주는 함수. (밀리초)
type TimeProvider = Box<dyn Fn() -> f64>;

// 자동 저장 설정.
struct AutosaveState {
	slot_id: String,
	interval_seconds: f64,
	elapsed_seconds: f64,
	options: SaveSlotOptions,
	data_provider: AutosaveDataProvider,
}

//==============================================================================
// 칸 이름 검사. (영문, 숫자, _, - 로 1 ~ 64 자)
///
/// # 인자
/// - `slot_id`: 칸 이름.
///
/// # 돌려줌
/// 쓸 수 있으면 true.
//==============================================================================
fn is_valid_slot_id(slot_id: &str) -> bool {
	!slot_id.is_empty() && slot_id.len() <= MAXIMUM_SLOT_ID_LENGTH && slot_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

//==============================================================================
// 지금 시각. (밀리초, 웹은 Date.now())
///
/// # 돌려줌
/// 1970-01-01 UTC 부터의 밀리초.
//==============================================================================
fn read_system_time() -> f64 {
	web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|duration| duration.as_millis() as f64).unwrap_or(0.0)
}

//==============================================================================
// 머리 JSON 짓기. (원본 JSON.stringify 와 같은 칸 차례)
///
/// # 인자
/// - `info`: 머리.
///
/// # 돌려줌
/// 머리 JSON.
//==============================================================================
fn build_header_text(info: &SaveSlotInfo) -> String {
	let mut header = Map::new();
	header.insert("format".to_string(), json!(RECORD_FORMAT));
	header.insert("formatVersion".to_string(), json!(FORMAT_VERSION));
	header.insert("slotId".to_string(), json!(info.slot_id));
	header.insert("sequence".to_string(), json!(info.sequence));
	header.insert("name".to_string(), json!(info.name));
	header.insert("savedTime".to_string(), json!(info.saved_time));
	header.insert("version".to_string(), json!(info.version));
	header.insert("previewImage".to_string(), json!(info.preview_image));
	header.insert("byteSize".to_string(), json!(info.byte_size));
	header.insert("metadata".to_string(), info.metadata.clone());
	stringify_json(&Value::Object(header))
}

//==============================================================================
// 기록 글 짓기.
///
/// # 인자
/// - `info`: 머리.
/// - `body_text`: 자료 JSON.
///
/// # 돌려줌
/// 기록 글.
//==============================================================================
fn build_record_text(info: &SaveSlotInfo, body_text: &str) -> String {
	let content = format!("{}\n{}", build_header_text(info), body_text);
	format!("{} {:08x}\n{}", RECORD_MAGIC, calculate_crc32(content.as_bytes()), content)
}

//==============================================================================
// 기록 글 풀기. (체크섬, 꼴, 칸 이름이 맞아야 한다. expected_slot_id 가 None 이면 칸 이름은 보지 않는다)
///
/// # 인자
/// - `record_text`: 기록 글.
/// - `expected_slot_id`: 맞아야 할 칸 이름. (None 이면 보지 않는다)
///
/// # 돌려줌
/// 푼 기록. (틀리면 None)
//==============================================================================
fn parse_record_text(record_text: &str, expected_slot_id: Option<&str>) -> Option<SaveRecord> {
	let (first_line, content) = record_text.split_once('\n')?;
	let (magic, checksum_text) = first_line.split_once(' ')?;
	if magic != RECORD_MAGIC || checksum_text.len() != 8 {
		return None;
	}
	let checksum = u32::from_str_radix(checksum_text, 16).ok()?;
	if calculate_crc32(content.as_bytes()) != checksum {
		return None;
	}
	let (header_text, body_text) = content.split_once('\n')?;
	let header: Value = serde_json::from_str(header_text).ok()?;
	if header["format"].as_str() != Some(RECORD_FORMAT) || header["formatVersion"].as_u64() != Some(FORMAT_VERSION) {
		return None;
	}
	let slot_id = header["slotId"].as_str()?.to_string();
	if let Some(expected_slot_id) = expected_slot_id {
		if slot_id != expected_slot_id {
			return None;
		}
	}
	let info = SaveSlotInfo {
		slot_id,
		name: header["name"].as_str()?.to_string(),
		saved_time: read_header_integer(&header["savedTime"])?,
		version: read_header_version(&header["version"])?,
		preview_image: header["previewImage"].as_str()?.to_string(),
		byte_size: read_header_integer(&header["byteSize"])? as usize,
		sequence: read_header_integer(&header["sequence"])? as u64,
		metadata: header.get("metadata").cloned().unwrap_or_else(|| json!({})),
		is_corrupted: false,
	};
	Some(SaveRecord { info, body_text: body_text.to_string(), record_text: record_text.to_string() })
}

//==============================================================================
/// 세이브 슬롯 관리.
/// # 예제
/// 칸에 저장하고 판 옮겨 읽기
/// ```
/// use vanilla::experimental::savegame::{MemorySaveStorage, SaveGameManager, SaveSlotOptions};
/// use serde_json::json;
///
/// let mut save_game_manager = SaveGameManager::new("example", 2);
/// save_game_manager.set_storage(Box::new(MemorySaveStorage::new()));
/// save_game_manager.add_migration(1, |mut data| {
/// 	let gold = data["gold"].take();
/// 	data["coins"] = gold;
/// 	Ok(data)
/// });
/// let options = SaveSlotOptions { name: "Forest Gate".to_string(), ..SaveSlotOptions::default() };
/// let info = save_game_manager.save("slot1", &json!({ "coins": 120, "level": 3 }), &options).expect("save");
/// assert_eq!(info.version, 2);
/// let result = save_game_manager.load("slot1").expect("load");
/// assert_eq!(result.data["coins"], 120);
/// assert_eq!(save_game_manager.list_slots()[0].name, "Forest Gate");
/// let exported_text = save_game_manager.export_slot("slot1").expect("export");
/// save_game_manager.import_slot("slot2", &exported_text).expect("import");
/// assert!(save_game_manager.exists("slot2"));
/// ```
//==============================================================================
pub struct SaveGameManager {
	namespace: String,
	current_version: u32,
	storage: Box<dyn SaveStorage>,
	migration_by_version: BTreeMap<u32, MigrationFunction>,
	autosave: Option<AutosaveState>,
	cloud_provider: Option<Box<dyn SaveCloudProvider>>,
	time_provider: Option<TimeProvider>,
}

impl SaveGameManager {
	//==============================================================================
	/// 생성. (저장소는 엔진 LocalStorage)
	///
	/// # 인자
	/// - `namespace`: 키 앞머리. (게임마다 다르게, 키는 `<namespace>.slot.<칸>.0` 과 `.1`)
	/// - `current_version`: 지금 자료의 판. (1 부터, 자료 꼴을 바꿀 때마다 올리고 옮기는 함수를 건다)
	///
	/// # 돌려줌
	/// 새 관리자.
	//==============================================================================
	pub fn new(namespace: &str, current_version: u32) -> SaveGameManager {
		SaveGameManager {
			namespace: namespace.to_string(),
			current_version,
			storage: Box::new(LocalSaveStorage::new()),
			migration_by_version: BTreeMap::new(),
			autosave: None,
			cloud_provider: None,
			time_provider: None,
		}
	}

	//==============================================================================
	/// 저장소 바꾸기.
	///
	/// # 인자
	/// - `storage`: 새 저장소. (앞 저장소의 기록은 옮기지 않는다)
	//==============================================================================
	pub fn set_storage(&mut self, storage: Box<dyn SaveStorage>) {
		self.storage = storage;
	}

	//==============================================================================
	/// 클라우드 꽂기. (None 이면 뺀다)
	///
	/// # 인자
	/// - `cloud_provider`: 클라우드 저장 구현.
	//==============================================================================
	pub fn set_cloud_provider(&mut self, cloud_provider: Option<Box<dyn SaveCloudProvider>>) {
		self.cloud_provider = cloud_provider;
	}

	//==============================================================================
	/// 시각 함수 바꾸기. (None 이면 시스템 시계, 시험에서 고정한 시각을 줄 때)
	///
	/// # 인자
	/// - `time_provider`: 1970-01-01 UTC 부터의 밀리초를 주는 함수.
	//==============================================================================
	pub fn set_time_provider(&mut self, time_provider: Option<Box<dyn Fn() -> f64>>) {
		self.time_provider = time_provider;
	}

	//==============================================================================
	/// 지금 자료의 판.
	///
	/// # 돌려줌
	/// 만들 때 준 판.
	//==============================================================================
	pub fn get_current_version(&self) -> u32 {
		self.current_version
	}

	//==============================================================================
	/// 판 옮기는 함수 걸기. (from_version 의 자료를 받아 from_version + 1 의 자료를 돌려준다, 같은 판에 다시 걸면 바꾼다)
	///
	/// # 인자
	/// - `from_version`: 옮길 판.
	/// - `migration`: 옮기는 함수. (실패하면 Err 에 까닭)
	//==============================================================================
	pub fn add_migration(&mut self, from_version: u32, migration: impl Fn(Value) -> Result<Value, String> + 'static) {
		self.migration_by_version.insert(from_version, Box::new(migration));
	}

	//==============================================================================
	/// 칸 키.
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	/// - `copy_index`: 키 번호. (0 이나 1)
	///
	/// # 돌려줌
	/// 키.
	//==============================================================================
	fn create_slot_key(&self, slot_id: &str, copy_index: usize) -> String {
		format!("{}.slot.{}.{}", self.namespace, slot_id, copy_index)
	}

	//==============================================================================
	/// 지금 시각. (밀리초)
	///
	/// # 돌려줌
	/// 시각 함수의 값, 없으면 시스템 시계.
	//==============================================================================
	fn read_time(&self) -> f64 {
		match &self.time_provider {
			Some(time_provider) => {
				time_provider()
			}
			None => {
				read_system_time()
			}
		}
	}

	//==============================================================================
	/// 두 키의 기록. (깨졌거나 없으면 None, 글이 있었는지는 둘째 값)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	///
	/// # 돌려줌
	/// 키 두 개의 읽은 결과.
	//==============================================================================
	fn read_copies(&self, slot_id: &str) -> [(Option<SaveRecord>, bool); 2] {
		[0, 1].map(|copy_index| {
			match self.storage.read_text(&self.create_slot_key(slot_id, copy_index)) {
				Some(record_text) => {
					(parse_record_text(&record_text, Some(slot_id)), true)
				}
				None => {
					(None, false)
				}
			}
		})
	}

	//==============================================================================
	/// 순번이 가장 큰 성한 기록의 키 번호.
	///
	/// # 인자
	/// - `copies`: 두 키의 기록.
	///
	/// # 돌려줌
	/// 키 번호. (성한 기록이 없으면 None)
	//==============================================================================
	fn find_newest_copy(copies: &[(Option<SaveRecord>, bool); 2]) -> Option<usize> {
		let mut newest: Option<(usize, u64)> = None;
		for (copy_index, (record, _)) in copies.iter().enumerate() {
			if let Some(record) = record {
				if newest.map(|(_, sequence)| record.info.sequence > sequence).unwrap_or(true) {
					newest = Some((copy_index, record.info.sequence));
				}
			}
		}
		newest.map(|(copy_index, _)| copy_index)
	}

	//==============================================================================
	/// 저장. (앞 기록이 없는 쪽 키에 순번을 올려 쓴다, 클라우드가 있으면 올린다)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름. (영문, 숫자, _, - 로 64 자까지)
	/// - `data`: 자료. (지금 판의 꼴)
	/// - `options`: 이름, 미리보기 그림, metadata.
	///
	/// # 돌려줌
	/// 쓴 기록의 머리. 칸 이름이 틀리면 InvalidSlotId, 자료를 JSON 으로 못 바꾸면 InvalidData, 저장소에 쓰지 못하면 WriteFailed.
	//==============================================================================
	pub fn save<T: Serialize + ?Sized>(&mut self, slot_id: &str, data: &T, options: &SaveSlotOptions) -> Result<SaveSlotInfo, SaveGameError> {
		if !is_valid_slot_id(slot_id) {
			return Err(SaveGameError::InvalidSlotId);
		}
		let data_value = serde_json::to_value(data).map_err(|_| SaveGameError::InvalidData)?;
		let body_text = stringify_json(&data_value);
		let saved_time = read_saved_time(self.read_time());
		let metadata = if options.metadata.is_object() { options.metadata.clone() } else { json!({}) };
		let info = SaveSlotInfo {
			slot_id: slot_id.to_string(),
			name: options.name.clone(),
			saved_time,
			version: self.current_version,
			preview_image: options.preview_image.clone(),
			byte_size: body_text.len(),
			sequence: 0,
			metadata,
			is_corrupted: false,
		};
		self.write_record(info, &body_text)
	}

	//==============================================================================
	/// 다음 순번으로 앞 기록이 없는 쪽 키에 쓴다.
	///
	/// # 인자
	/// - `info`: 머리. (순번은 여기서 정한다)
	/// - `body_text`: 자료 JSON.
	///
	/// # 돌려줌
	/// 쓴 기록의 머리. 저장소에 쓰지 못하면 WriteFailed.
	//==============================================================================
	fn write_record(&mut self, mut info: SaveSlotInfo, body_text: &str) -> Result<SaveSlotInfo, SaveGameError> {
		let slot_id = info.slot_id.clone();
		let copies = self.read_copies(&slot_id);
		let newest_index = SaveGameManager::find_newest_copy(&copies);
		let (target_index, sequence) = match newest_index {
			Some(index) => {
				(1 - index, copies[index].0.as_ref().map(|record| record.info.sequence).unwrap_or(0) + 1)
			}
			None => {
				(0, 1)
			}
		};
		info.sequence = sequence;
		let record_text = build_record_text(&info, body_text);
		if !self.storage.write_text(&self.create_slot_key(&slot_id, target_index), &record_text) {
			return Err(SaveGameError::WriteFailed);
		}
		if self.cloud_provider.is_some() {
			let exported_text = build_export_text(&record_text);
			if let Some(cloud_provider) = self.cloud_provider.as_mut() {
				cloud_provider.upload(&slot_id, &exported_text);
			}
		}
		Ok(info)
	}

	//==============================================================================
	/// 읽기. (성한 기록 가운데 순번이 가장 큰 것, 판이 낮으면 옮긴다)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	///
	/// # 돌려줌
	/// 자료와 머리. 실패하면 InvalidSlotId, NotFound, Corrupted, NewerVersion, MissingMigration, MigrationFailed 가운데 하나.
	//==============================================================================
	pub fn load(&self, slot_id: &str) -> Result<SaveGameLoadResult, SaveGameError> {
		if !is_valid_slot_id(slot_id) {
			return Err(SaveGameError::InvalidSlotId);
		}
		let copies = self.read_copies(slot_id);
		let Some(newest_index) = SaveGameManager::find_newest_copy(&copies) else {
			return Err(if copies.iter().any(|(_, is_present)| *is_present) { SaveGameError::Corrupted } else { SaveGameError::NotFound });
		};
		let is_recovered_from_backup = copies.iter().any(|(record, is_present)| *is_present && record.is_none());
		let record = copies[newest_index].0.as_ref().ok_or(SaveGameError::Corrupted)?;
		let data: Value = serde_json::from_str(&record.body_text).map_err(|_| SaveGameError::Corrupted)?;
		let (data, migrated_from_version) = self.migrate(data, record.info.version)?;
		Ok(SaveGameLoadResult { data, info: record.info.clone(), is_recovered_from_backup, migrated_from_version })
	}

	//==============================================================================
	/// 판 옮기기.
	///
	/// # 인자
	/// - `data`: 자료.
	/// - `version`: 자료의 판.
	///
	/// # 돌려줌
	/// 옮긴 자료와 옮겼으면 옮기기 전 판.
	//==============================================================================
	fn migrate(&self, mut data: Value, version: u32) -> Result<(Value, Option<u32>), SaveGameError> {
		if version > self.current_version {
			return Err(SaveGameError::NewerVersion { version, current_version: self.current_version });
		}
		if version == self.current_version {
			return Ok((data, None));
		}
		for from_version in version..self.current_version {
			let migration = self.migration_by_version.get(&from_version).ok_or(SaveGameError::MissingMigration { from_version })?;
			data = migration(data).map_err(|message| SaveGameError::MigrationFailed { from_version, message })?;
		}
		Ok((data, Some(version)))
	}

	//==============================================================================
	/// 칸 지우기. (두 키 모두, 클라우드가 있으면 거기서도)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	//==============================================================================
	pub fn delete(&mut self, slot_id: &str) {
		if !is_valid_slot_id(slot_id) {
			return;
		}
		for copy_index in 0..2 {
			let key = self.create_slot_key(slot_id, copy_index);
			self.storage.remove_text(&key);
		}
		if let Some(cloud_provider) = self.cloud_provider.as_mut() {
			cloud_provider.remove(slot_id);
		}
	}

	//==============================================================================
	/// 칸에 기록이 있는지. (깨진 기록도 있는 것으로 친다)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	///
	/// # 돌려줌
	/// 두 키 가운데 하나라도 글이 있으면 true.
	//==============================================================================
	pub fn exists(&self, slot_id: &str) -> bool {
		is_valid_slot_id(slot_id) && (0..2).any(|copy_index| self.storage.read_text(&self.create_slot_key(slot_id, copy_index)).is_some())
	}

	//==============================================================================
	/// 칸 목록. (저장 시각이 늦은 차례, 같으면 칸 이름 차례. 읽을 기록이 없는 칸은 is_corrupted 로 맨 뒤에)
	///
	/// # 돌려줌
	/// 칸마다 고른 기록의 머리.
	//==============================================================================
	pub fn list_slots(&self) -> Vec<SaveSlotInfo> {
		let prefix = format!("{}.slot.", self.namespace);
		let mut slot_id_list: Vec<String> = Vec::new();
		for key in self.storage.list_keys(&prefix) {
			let rest = &key[prefix.len()..];
			let Some((slot_id, copy_text)) = rest.rsplit_once('.') else {
				continue;
			};
			if (copy_text == "0" || copy_text == "1") && is_valid_slot_id(slot_id) && !slot_id_list.iter().any(|existing| existing == slot_id) {
				slot_id_list.push(slot_id.to_string());
			}
		}
		let mut info_list: Vec<SaveSlotInfo> = slot_id_list.iter().map(|slot_id| {
			let copies = self.read_copies(slot_id);
			match SaveGameManager::find_newest_copy(&copies).and_then(|index| copies[index].0.as_ref().map(|record| record.info.clone())) {
				Some(info) => {
					info
				}
				None => {
					SaveSlotInfo { slot_id: slot_id.clone(), name: String::new(), saved_time: 0.0, version: 0, preview_image: String::new(), byte_size: 0, sequence: 0, metadata: json!({}), is_corrupted: true }
				}
			}
		}).collect();
		info_list.sort_by(|left, right| {
			left.is_corrupted.cmp(&right.is_corrupted).then(right.saved_time.partial_cmp(&left.saved_time).unwrap_or(std::cmp::Ordering::Equal)).then(left.slot_id.cmp(&right.slot_id))
		});
		info_list
	}

	//==============================================================================
	/// 내보내기. (고른 기록을 그대로 담은 JSON 글, {"format":"vanilla-save-export","formatVersion":1,"record":"..."})
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	///
	/// # 돌려줌
	/// JSON 글. 칸이 없거나 깨졌으면 NotFound 나 Corrupted.
	//==============================================================================
	pub fn export_slot(&self, slot_id: &str) -> Result<String, SaveGameError> {
		if !is_valid_slot_id(slot_id) {
			return Err(SaveGameError::InvalidSlotId);
		}
		let copies = self.read_copies(slot_id);
		match SaveGameManager::find_newest_copy(&copies).and_then(|index| copies[index].0.as_ref()) {
			Some(record) => {
				Ok(build_export_text(&record.record_text))
			}
			None => {
				Err(if copies.iter().any(|(_, is_present)| *is_present) { SaveGameError::Corrupted } else { SaveGameError::NotFound })
			}
		}
	}

	//==============================================================================
	/// 들여오기. (export_slot 의 글을 체크섬으로 보고 slot_id 칸의 새 기록으로 쓴다, 판은 그대로 두고 읽을 때 옮긴다)
	///
	/// # 인자
	/// - `slot_id`: 들일 칸 이름. (내보낸 칸과 달라도 된다)
	/// - `exported_text`: export_slot 의 JSON 글.
	///
	/// # 돌려줌
	/// 쓴 기록의 머리. 글이 틀리거나 깨졌으면 InvalidImport, 지금 판보다 새 판이면 NewerVersion.
	//==============================================================================
	pub fn import_slot(&mut self, slot_id: &str, exported_text: &str) -> Result<SaveSlotInfo, SaveGameError> {
		if !is_valid_slot_id(slot_id) {
			return Err(SaveGameError::InvalidSlotId);
		}
		let record = parse_export_text(exported_text).ok_or(SaveGameError::InvalidImport)?;
		if record.info.version > self.current_version {
			return Err(SaveGameError::NewerVersion { version: record.info.version, current_version: self.current_version });
		}
		let info = SaveSlotInfo { slot_id: slot_id.to_string(), ..record.info };
		self.write_record(info, &record.body_text)
	}

	//==============================================================================
	/// 클라우드에서 받기. (클라우드 기록이 칸의 기록보다 늦게 저장됐거나 칸이 비었으면 들인다)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	///
	/// # 돌려줌
	/// 들였으면 true, 클라우드가 없거나 더 새 것이 없으면 false. 받은 글이 깨졌으면 InvalidImport.
	//==============================================================================
	pub fn pull_from_cloud(&mut self, slot_id: &str) -> Result<bool, SaveGameError> {
		if !is_valid_slot_id(slot_id) {
			return Err(SaveGameError::InvalidSlotId);
		}
		let Some(cloud_provider) = self.cloud_provider.as_mut() else {
			return Ok(false);
		};
		let Some(exported_text) = cloud_provider.download(slot_id) else {
			return Ok(false);
		};
		let record = parse_export_text(&exported_text).ok_or(SaveGameError::InvalidImport)?;
		let copies = self.read_copies(slot_id);
		let local_saved_time = SaveGameManager::find_newest_copy(&copies).and_then(|index| copies[index].0.as_ref().map(|local| local.info.saved_time));
		if local_saved_time.map(|saved_time| record.info.saved_time <= saved_time).unwrap_or(false) {
			return Ok(false);
		}
		// 받은 것을 다시 올리지 않게 잠깐 뺀다.
		let cloud_provider = self.cloud_provider.take();
		let result = self.import_slot(slot_id, &exported_text);
		self.cloud_provider = cloud_provider;
		result.map(|_| true)
	}

	//==============================================================================
	/// 자동 저장 걸기. (update 가 간격마다 data_provider 의 자료를 slot_id 칸에 저장한다, 앞 설정은 바꾼다)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	/// - `interval_seconds`: 간격. (초, 0 보다 커야 한다)
	/// - `options`: 저장할 때 쓸 이름, 미리보기 그림, metadata.
	/// - `data_provider`: 저장할 자료를 주는 함수. (None 이면 이번에는 건너뛴다)
	//==============================================================================
	pub fn set_autosave(&mut self, slot_id: &str, interval_seconds: f64, options: SaveSlotOptions, data_provider: impl FnMut() -> Option<Value> + 'static) {
		if !(interval_seconds.is_finite() && interval_seconds > 0.0) {
			return;
		}
		self.autosave = Some(AutosaveState { slot_id: slot_id.to_string(), interval_seconds, elapsed_seconds: 0.0, options, data_provider: Box::new(data_provider) });
	}

	//==============================================================================
	/// 자동 저장 끄기.
	//==============================================================================
	pub fn clear_autosave(&mut self) {
		self.autosave = None;
	}

	//==============================================================================
	/// 자동 저장이 걸렸는지.
	///
	/// # 돌려줌
	/// 걸려 있으면 true.
	//==============================================================================
	pub fn is_autosave_enabled(&self) -> bool {
		self.autosave.is_some()
	}

	//==============================================================================
	/// 매 프레임 갱신. (자동 저장 시간을 재고 간격이 차면 저장한다)
	///
	/// # 인자
	/// - `time_delta`: 지난 프레임부터의 시간. (초)
	///
	/// # 돌려줌
	/// 이번에 저장했으면 그 결과, 아니면 None.
	//==============================================================================
	pub fn update(&mut self, time_delta: f64) -> Option<Result<SaveSlotInfo, SaveGameError>> {
		let autosave = self.autosave.as_mut()?;
		if time_delta.is_finite() && time_delta > 0.0 {
			autosave.elapsed_seconds += time_delta;
		}
		if autosave.elapsed_seconds < autosave.interval_seconds {
			return None;
		}
		self.save_autosave_now()
	}

	//==============================================================================
	/// 자동 저장을 지금 하기. (창을 닫거나 내리기 전에, 시간은 처음부터 다시 잰다)
	///
	/// # 돌려줌
	/// 저장했으면 그 결과, 자동 저장이 없거나 자료가 None 이면 None.
	//==============================================================================
	pub fn save_autosave_now(&mut self) -> Option<Result<SaveSlotInfo, SaveGameError>> {
		let autosave = self.autosave.as_mut()?;
		autosave.elapsed_seconds = 0.0;
		let data = (autosave.data_provider)()?;
		let slot_id = autosave.slot_id.clone();
		let options = autosave.options.clone();
		Some(self.save(&slot_id, &data, &options))
	}
}

//==============================================================================
// 내보내기 글 짓기.
///
/// # 인자
/// - `record_text`: 기록 글.
///
/// # 돌려줌
/// 내보내기 JSON.
//==============================================================================
fn build_export_text(record_text: &str) -> String {
	let mut export = Map::new();
	export.insert("format".to_string(), json!(EXPORT_FORMAT));
	export.insert("formatVersion".to_string(), json!(FORMAT_VERSION));
	export.insert("record".to_string(), json!(record_text));
	stringify_json(&Value::Object(export))
}

//==============================================================================
// 내보내기 글 풀기. (꼴과 체크섬이 맞아야 한다)
///
/// # 인자
/// - `exported_text`: 내보내기 JSON.
///
/// # 돌려줌
/// 푼 기록. (틀리면 None)
//==============================================================================
fn parse_export_text(exported_text: &str) -> Option<SaveRecord> {
	let export: Value = serde_json::from_str(exported_text).ok()?;
	if export["format"].as_str() != Some(EXPORT_FORMAT) || export["formatVersion"].as_u64() != Some(FORMAT_VERSION) {
		return None;
	}
	parse_record_text(export["record"].as_str()?, None)
}

#[cfg(test)]
mod tests {
	use super::super::savestorage::MemorySaveStorage;
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	// 바깥에서 들여다보는 메모리 저장소. (쓰기 끊김과 반쯤 쓰기를 흉내 낸다)
	#[derive(Default)]
	struct SharedStorageState {
		storage: MemorySaveStorage,
		is_write_failing: bool,
		torn_write_length: Option<usize>,
	}

	struct SharedStorage {
		state: Rc<RefCell<SharedStorageState>>,
	}

	impl SaveStorage for SharedStorage {
		//==============================================================================
		/// 읽기.
		///
		/// # 인자
		/// - `key`: 키.
		///
		/// # 돌려줌
		/// 저장된 글.
		//==============================================================================
		fn read_text(&self, key: &str) -> Option<String> {
			self.state.borrow().storage.read_text(key)
		}

		//==============================================================================
		/// 쓰기. (실패나 반쯤 쓰기를 흉내 낸다)
		///
		/// # 인자
		/// - `key`: 키.
		/// - `text`: 쓸 글.
		///
		/// # 돌려줌
		/// 썼으면 true.
		//==============================================================================
		fn write_text(&mut self, key: &str, text: &str) -> bool {
			let mut state = self.state.borrow_mut();
			if state.is_write_failing {
				return false;
			}
			if let Some(length) = state.torn_write_length {
				// 앱이 쓰다 죽은 꼴. (앞쪽만 남는다)
				let torn_text: String = text.chars().take(length).collect();
				state.storage.write_text(key, &torn_text);
				return true;
			}
			state.storage.write_text(key, text)
		}

		//==============================================================================
		/// 지우기.
		///
		/// # 인자
		/// - `key`: 지울 키.
		//==============================================================================
		fn remove_text(&mut self, key: &str) {
			self.state.borrow_mut().storage.remove_text(key);
		}

		//==============================================================================
		/// 앞머리가 같은 키를 모두 읽기.
		///
		/// # 인자
		/// - `prefix`: 앞머리.
		///
		/// # 돌려줌
		/// 키 목록.
		//==============================================================================
		fn list_keys(&self, prefix: &str) -> Vec<String> {
			self.state.borrow().storage.list_keys(prefix)
		}
	}

	//==============================================================================
	/// 시험 관리자와 들여다볼 저장소. (시각은 부를 때마다 1 초씩 간다)
	///
	/// # 인자
	/// - `current_version`: 지금 판.
	///
	/// # 돌려줌
	/// 관리자와 저장소 상태.
	//==============================================================================
	fn create_manager(current_version: u32) -> (SaveGameManager, Rc<RefCell<SharedStorageState>>) {
		let state = Rc::new(RefCell::new(SharedStorageState::default()));
		let mut save_game_manager = SaveGameManager::new("test", current_version);
		save_game_manager.set_storage(Box::new(SharedStorage { state: state.clone() }));
		let clock = Rc::new(RefCell::new(1_000_000.0));
		save_game_manager.set_time_provider(Some(Box::new(move || {
			let mut time = clock.borrow_mut();
			*time += 1000.0;
			*time
		})));
		(save_game_manager, state)
	}

	//==============================================================================
	// 칸 쓰기와 읽기, 두 키 번갈아 쓰기, 목록 차례, 지우기.
	//==============================================================================
	#[test]
	fn slots_write_read_list_and_delete() {
		let (mut save_game_manager, state) = create_manager(1);
		let options = SaveSlotOptions { name: "첫 칸".to_string(), preview_image: "data:image/png;base64,AA==".to_string(), metadata: json!({ "playSeconds": 75 }) };
		let first = save_game_manager.save("slot1", &json!({ "hp": 10 }), &options).expect("save");
		assert_eq!(first.sequence, 1);
		assert_eq!(first.byte_size, "{\"hp\":10}".len());
		let second = save_game_manager.save("slot1", &json!({ "hp": 7 }), &options).expect("save");
		assert_eq!(second.sequence, 2);
		assert!(state.borrow().storage.read_text("test.slot.slot1.0").is_some());
		assert!(state.borrow().storage.read_text("test.slot.slot1.1").is_some());
		save_game_manager.save("slot2", &json!([1, 2, 3]), &SaveSlotOptions::default()).expect("save");
		let result = save_game_manager.load("slot1").expect("load");
		assert_eq!(result.data, json!({ "hp": 7 }));
		assert_eq!(result.info.metadata["playSeconds"], 75);
		assert!(!result.is_recovered_from_backup);
		let slot_list = save_game_manager.list_slots();
		assert_eq!(slot_list.iter().map(|info| info.slot_id.as_str()).collect::<Vec<_>>(), vec!["slot2", "slot1"]);
		assert_eq!(save_game_manager.load("slot3"), Err(SaveGameError::NotFound));
		assert_eq!(save_game_manager.save("bad id", &json!(1), &options), Err(SaveGameError::InvalidSlotId));
		save_game_manager.delete("slot1");
		assert!(!save_game_manager.exists("slot1"));
	}

	//==============================================================================
	// 판 옮기기 사슬, 새 판 세이브 거절, 빠진 함수와 실패한 함수.
	//==============================================================================
	#[test]
	fn migrations_chain_and_refuse() {
		let (mut old_manager, state) = create_manager(1);
		old_manager.save("slot1", &json!({ "gold": 5 }), &SaveSlotOptions::default()).expect("save");
		let mut new_manager = SaveGameManager::new("test", 3);
		new_manager.set_storage(Box::new(SharedStorage { state: state.clone() }));
		assert_eq!(new_manager.load("slot1"), Err(SaveGameError::MissingMigration { from_version: 1 }));
		new_manager.add_migration(1, |mut data| {
			let gold = data["gold"].take();
			data["coins"] = gold;
			Ok(data)
		});
		new_manager.add_migration(2, |mut data| {
			data["gems"] = json!(0);
			Ok(data)
		});
		let result = new_manager.load("slot1").expect("load");
		assert_eq!(result.data, json!({ "gold": null, "coins": 5, "gems": 0 }));
		assert_eq!(result.migrated_from_version, Some(1));
		new_manager.add_migration(2, |_| Err("깨진 판".to_string()));
		assert_eq!(new_manager.load("slot1"), Err(SaveGameError::MigrationFailed { from_version: 2, message: "깨진 판".to_string() }));
		new_manager.save("slot2", &json!({}), &SaveSlotOptions::default()).expect("save");
		assert_eq!(old_manager.load("slot2"), Err(SaveGameError::NewerVersion { version: 3, current_version: 1 }));
	}

	//==============================================================================
	// 원자적 쓰기: 쓰기 실패와 쓰다 끊긴 기록에도 앞 저장이 남는다. 체크섬이 바뀐 바이트를 잡는다.
	//==============================================================================
	#[test]
	fn interrupted_writes_keep_previous_save() {
		let (mut save_game_manager, state) = create_manager(1);
		save_game_manager.save("slot1", &json!({ "stage": 1 }), &SaveSlotOptions::default()).expect("save");
		state.borrow_mut().is_write_failing = true;
		assert_eq!(save_game_manager.save("slot1", &json!({ "stage": 2 }), &SaveSlotOptions::default()), Err(SaveGameError::WriteFailed));
		state.borrow_mut().is_write_failing = false;
		assert_eq!(save_game_manager.load("slot1").expect("load").data, json!({ "stage": 1 }));
		// 쓰다 끊김. (새 기록이 반쯤만 남는다)
		state.borrow_mut().torn_write_length = Some(30);
		save_game_manager.save("slot1", &json!({ "stage": 3 }), &SaveSlotOptions::default()).expect("save");
		state.borrow_mut().torn_write_length = None;
		let result = save_game_manager.load("slot1").expect("load");
		assert_eq!(result.data, json!({ "stage": 1 }));
		assert!(result.is_recovered_from_backup);
		// 다음 저장은 깨진 쪽 키에 쓰고 순번을 잇는다.
		let info = save_game_manager.save("slot1", &json!({ "stage": 4 }), &SaveSlotOptions::default()).expect("save");
		assert_eq!(info.sequence, 2);
		assert_eq!(save_game_manager.load("slot1").expect("load").data, json!({ "stage": 4 }));
		// 바이트 하나를 바꾸면 그 기록은 깨진 것으로 본다.
		let key = "test.slot.slot1.1";
		let text = state.borrow().storage.read_text(key).expect("record");
		let changed_text = text.replace("\"stage\":4", "\"stage\":5");
		state.borrow_mut().storage.write_text(key, &changed_text);
		let result = save_game_manager.load("slot1").expect("load");
		assert_eq!(result.data, json!({ "stage": 1 }));
		state.borrow_mut().storage.write_text("test.slot.slot1.0", "VSAVE1 00000000\n{}\n{}");
		assert_eq!(save_game_manager.load("slot1"), Err(SaveGameError::Corrupted));
		assert!(save_game_manager.list_slots()[0].is_corrupted);
	}

	//==============================================================================
	// 내보내기와 들여오기, 깨진 글 거절, 자동 저장 간격, 클라우드 올리기와 받기.
	//==============================================================================
	#[test]
	fn export_import_autosave_and_cloud() {
		let (mut save_game_manager, _) = create_manager(1);
		let options = SaveSlotOptions { name: "내보낼 칸".to_string(), ..SaveSlotOptions::default() };
		save_game_manager.save("slot1", &json!({ "name": "모험가", "items": ["검", "방패"] }), &options).expect("save");
		let exported_text = save_game_manager.export_slot("slot1").expect("export");
		let imported = save_game_manager.import_slot("slot9", &exported_text).expect("import");
		assert_eq!(imported.name, "내보낼 칸");
		assert_eq!(save_game_manager.load("slot9").expect("load").data["items"][1], "방패");
		assert_eq!(save_game_manager.import_slot("slot8", &exported_text.replace("모험가", "도적")), Err(SaveGameError::InvalidImport));
		assert_eq!(save_game_manager.import_slot("slot8", "{}"), Err(SaveGameError::InvalidImport));
		// 자동 저장.
		let counter = Rc::new(RefCell::new(0));
		let counter_for_provider = counter.clone();
		save_game_manager.set_autosave("auto", 10.0, SaveSlotOptions::default(), move || {
			*counter_for_provider.borrow_mut() += 1;
			Some(json!({ "count": *counter_for_provider.borrow() }))
		});
		assert!(save_game_manager.update(4.0).is_none());
		assert!(save_game_manager.update(4.0).is_none());
		assert!(save_game_manager.update(4.0).expect("autosave").is_ok());
		assert!(save_game_manager.update(9.0).is_none());
		assert_eq!(save_game_manager.load("auto").expect("load").data["count"], 1);
		save_game_manager.save_autosave_now().expect("autosave").expect("save");
		assert_eq!(save_game_manager.load("auto").expect("load").data["count"], 2);
		// 클라우드.
		#[derive(Default)]
		struct MemoryCloud {
			text_by_slot: Rc<RefCell<BTreeMap<String, String>>>,
		}
		impl SaveCloudProvider for MemoryCloud {
			//==============================================================================
			/// 올리기.
			///
			/// # 인자
			/// - `slot_id`: 칸 이름.
			/// - `exported_text`: 내보내기 JSON.
			///
			/// # 돌려줌
			/// 늘 true.
			//==============================================================================
			fn upload(&mut self, slot_id: &str, exported_text: &str) -> bool {
				self.text_by_slot.borrow_mut().insert(slot_id.to_string(), exported_text.to_string());
				true
			}
			//==============================================================================
			/// 내려받기.
			///
			/// # 인자
			/// - `slot_id`: 칸 이름.
			///
			/// # 돌려줌
			/// 내보내기 JSON.
			//==============================================================================
			fn download(&mut self, slot_id: &str) -> Option<String> {
				self.text_by_slot.borrow().get(slot_id).cloned()
			}
			//==============================================================================
			/// 지우기.
			///
			/// # 인자
			/// - `slot_id`: 칸 이름.
			//==============================================================================
			fn remove(&mut self, slot_id: &str) {
				self.text_by_slot.borrow_mut().remove(slot_id);
			}
			//==============================================================================
			/// 칸 이름을 모두 읽기.
			///
			/// # 돌려줌
			/// 칸 이름 목록.
			//==============================================================================
			fn list_slot_ids(&mut self) -> Vec<String> {
				self.text_by_slot.borrow().keys().cloned().collect()
			}
		}
		let cloud_texts = Rc::new(RefCell::new(BTreeMap::new()));
		save_game_manager.set_cloud_provider(Some(Box::new(MemoryCloud { text_by_slot: cloud_texts.clone() })));
		save_game_manager.save("cloud", &json!({ "from": "this" }), &SaveSlotOptions::default()).expect("save");
		assert!(cloud_texts.borrow().contains_key("cloud"));
		assert_eq!(save_game_manager.pull_from_cloud("cloud"), Ok(false));
		// 다른 기기가 더 늦게 올린 세이브.
		let (mut other_manager, _) = create_manager(1);
		other_manager.set_time_provider(Some(Box::new(|| 9_000_000.0)));
		other_manager.save("cloud", &json!({ "from": "other" }), &SaveSlotOptions::default()).expect("save");
		cloud_texts.borrow_mut().insert("cloud".to_string(), other_manager.export_slot("cloud").expect("export"));
		assert_eq!(save_game_manager.pull_from_cloud("cloud"), Ok(true));
		assert_eq!(save_game_manager.load("cloud").expect("load").data["from"], "other");
		save_game_manager.delete("cloud");
		assert!(!cloud_texts.borrow().contains_key("cloud"));
	}

	// 원본(JS) SaveGameManager 가 같은 값으로 쓴 기록. (vanilla.js tests/experimental/savegame.test.mjs 에도 같은 글이 있다)
	const ORIGINAL_RECORD_TEXT: &str = "VSAVE1 08bf5351\n{\"format\":\"vanilla-save\",\"formatVersion\":1,\"slotId\":\"slot1\",\"sequence\":1,\"name\":\"첫 칸\",\"savedTime\":1764100000000,\"version\":1,\"previewImage\":\"\",\"byteSize\":48,\"metadata\":{\"playSeconds\":75}}\n{\"hp\":10,\"name\":\"모험가\",\"items\":[\"검\",0.5]}";

	//==============================================================================
	// 기록 꼴이 원본과 글자까지 같고, 원본이 쓴 기록을 읽는다.
	//==============================================================================
	#[test]
	fn record_format_matches_original() {
		let state = Rc::new(RefCell::new(SharedStorageState::default()));
		let mut save_game_manager = SaveGameManager::new("fixture", 1);
		save_game_manager.set_storage(Box::new(SharedStorage { state: state.clone() }));
		save_game_manager.set_time_provider(Some(Box::new(|| 1764100000000.0)));
		let options = SaveSlotOptions { name: "첫 칸".to_string(), preview_image: String::new(), metadata: json!({ "playSeconds": 75 }) };
		save_game_manager.save("slot1", &json!({ "hp": 10, "name": "모험가", "items": ["검", 0.5] }), &options).expect("save");
		assert_eq!(state.borrow().storage.read_text("fixture.slot.slot1.0").expect("record"), ORIGINAL_RECORD_TEXT);
		state.borrow_mut().storage.write_text("fixture.slot.copy.1", &ORIGINAL_RECORD_TEXT.replace("\"slotId\":\"slot1\"", "\"slotId\":\"copy\""));
		assert_eq!(save_game_manager.load("copy"), Err(SaveGameError::Corrupted));
		state.borrow_mut().storage.write_text("fixture.slot.slot2.1", ORIGINAL_RECORD_TEXT);
		assert_eq!(save_game_manager.load("slot2"), Err(SaveGameError::Corrupted));
		let result = save_game_manager.load("slot1").expect("load");
		assert_eq!(result.data["items"][1], 0.5);
	}

	// 원본(JS)이 수 글꼴, 객체 칸 차례, 음수 시각을 담아 쓴 기록. (vanilla.js tests/experimental/savegame.test.mjs 의 NUMBER_RECORD_TEXT 와 같은 글)
	const NUMBER_RECORD_TEXT: &str = "VSAVE1 7e16634d\n{\"format\":\"vanilla-save\",\"formatVersion\":1,\"slotId\":\"slot1\",\"sequence\":1,\"name\":\"수\",\"savedTime\":0,\"version\":2,\"previewImage\":\"\",\"byteSize\":144,\"metadata\":{\"2\":3,\"10\":2,\"b\":1}}\n{\"7\":\"seven\",\"whole\":1,\"negativeZero\":0,\"large\":1e+21,\"small\":1.5e-7,\"big\":123456789012345680000,\"01\":\"zero one\",\"fraction\":0.30000000000000004}";

	// 기록 머리 한 군데를 바꾸고 체크섬을 다시 셈한다.
	fn replace_record_header(record_text: &str, from_text: &str, to_text: &str) -> String {
		let content = record_text.split_once('\n').expect("first line").1.replacen(from_text, to_text, 1);
		format!("VSAVE1 {:08x}\n{}", calculate_crc32(content.as_bytes()), content)
	}

	//==============================================================================
	// 수 글꼴(원본 String(number)), 객체 칸 차례, 음수 시각, 머리 검사가 원본과 같다.
	//==============================================================================
	#[test]
	fn number_format_and_header_checks_match_original() {
		let state = Rc::new(RefCell::new(SharedStorageState::default()));
		let mut save_game_manager = SaveGameManager::new("numbers", 2);
		save_game_manager.set_storage(Box::new(SharedStorage { state: state.clone() }));
		save_game_manager.set_time_provider(Some(Box::new(|| -5.5)));
		let options = SaveSlotOptions { name: "수".to_string(), preview_image: String::new(), metadata: json!({ "b": 1.0, "10": 2, "2": 3 }) };
		let data = json!({ "whole": 1.0, "negativeZero": -0.0, "large": 1e21, "small": 1.5e-7, "big": 123456789012345678901.0, "7": "seven", "01": "zero one", "fraction": 0.1 + 0.2 });
		let info = save_game_manager.save("slot1", &data, &options).expect("save");
		assert_eq!(info.saved_time, 0.0);
		assert_eq!(state.borrow().storage.read_text("numbers.slot.slot1.0").expect("record"), NUMBER_RECORD_TEXT);
		assert_eq!(save_game_manager.load("slot1").expect("load").data["large"], 1e21);
		let fraction_time = replace_record_header(NUMBER_RECORD_TEXT, "\"savedTime\":0", "\"savedTime\":1.5").replace("\"slotId\":\"slot1\"", "\"slotId\":\"fraction\"");
		state.borrow_mut().storage.write_text("numbers.slot.fraction.0", &fraction_time);
		assert_eq!(save_game_manager.load("fraction"), Err(SaveGameError::Corrupted));
		let big_version = replace_record_header(&replace_record_header(NUMBER_RECORD_TEXT, "\"version\":2", "\"version\":4294967296"), "\"sequence\":1", "\"sequence\":2");
		state.borrow_mut().storage.write_text("numbers.slot.slot1.1", &big_version);
		assert_eq!(save_game_manager.load("slot1").expect("load").info.sequence, 1);
		save_game_manager.save("slot5", &json!({ "value": f64::NAN, "list": [f64::INFINITY] }), &SaveSlotOptions::default()).expect("save");
		assert!(state.borrow().storage.read_text("numbers.slot.slot5.0").expect("record").ends_with("{\"value\":null,\"list\":[null]}"));
		let mut non_text_keys = std::collections::HashMap::new();
		non_text_keys.insert(vec![1u8], 1);
		assert_eq!(save_game_manager.save("slot6", &non_text_keys, &SaveSlotOptions::default()), Err(SaveGameError::InvalidData));
	}
}
