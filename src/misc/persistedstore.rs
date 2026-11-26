//==============================================================================
//! 지속 저장소.
//! - 원본 misc/persistedstore.js 를 옮긴 것이다. 키 하나에 상태 객체를 JSON 으로 저장하고, 바뀔 때 구독자에게 알린다.
//! - 판 번호(version)가 다르거나 해석에 실패하면 기본값으로 돌아간다.
//! - 저장은 LocalStorage 를 쓴다. 저장 꼴({"version":1,"state":{...}})이 원본과 같아서 웹에서는 원본 게임의 저장을 그대로 읽는다.
//!   set_storage_backend 로 이 저장소만 다른 곳(LocalStorageBackend, 플랫폼의 저장소 같은 것)에 둘 수 있다.
//!   넘긴 저장소는 `Rc<RefCell>` 에 담는다. get_storage_backend 는 그 Rc 를 돌려주고, 부르는 쪽은 borrow / borrow_mut 로 빌려 쓴다.
//!   (빌린 채로 이 저장소를 읽거나 쓰면 읽기는 빈 글, 쓰기는 알리고 넘어간다)
//! - 사용:
//!   ```text
//!     let mut store = PersistedStore::new("game.settings", serde_json::json!({ "volume": 0.8, "is_muted": false }), 1);
//!     store.set(serde_json::json!({ "volume": 0.5 }));
//!     let volume = store.get()["volume"].as_f64();
//!   ```
//! - 섞기는 원본 Object.assign 과 같다. 배열은 번호 키("0", "1" ...)로 섞고(저장된 state 가 배열이어도), 키 차례는 JS 객체처럼 번호 꼴 키를 작은 수부터 앞에 둔다.
//! - 원본과 다른 점:
//!   상태 객체는 serde_json::Value(객체)다. 객체도 배열도 아닌 값을 넘기면 빈 객체로 친다. (원본 Object.assign 은 글을 글자마다 번호 키로 섞는다)
//!   subscribe 는 해지 함수 대신 구독 번호(PersistedStoreSubscription)를 돌려주고, unsubscribe(번호) 로 푼다.
//!   러스트 String 은 짝 없는 서로게이트를 담지 못해, 원본이 저장한 그런 글(JSON.stringify 의 \ud83d 같은 것)은 get 의 상태에서 대체 문자(U+FFFD)로 보인다.
//!   그 글의 원래 JS 글 단위는 get_string_code_units 로 읽고 set_string_code_units 로 쓴다. (LocalStorage 의 *_code_units 와 같은 꼴, 원본 글과 같은 값)
//!   읽을 때 그 글의 자리를 남겨 두고, 고치지 않고 다시 쓰면 원본처럼 \udXXX 로 쓴다. (misc/storagetext.rs, 원본 판의 저장을 읽고 다시 써도 잃지 않는다)
//==============================================================================
use crate::misc::localstorage::{LocalStorage, LocalStorageBackend};
use crate::misc::storagetext::{self, JsonPathSegment, JsonSurrogateTable, PreservedText};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
/// 구독 번호. (unsubscribe 에 넘긴다)
///
/// # 예제
/// 구독 번호로 한 구독만 풀기
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{LocalStorage, PersistedStore, PersistedStoreSubscription};
///
/// let storage_directory_path = std::env::temp_dir().join(format!("vanillaPersistedStoreSubscriptionExample-{}", std::process::id()));
/// LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
/// let mut store = PersistedStore::new("example.progress", serde_json::json!({ "stage": 1 }), 1);
/// let notified_count = Rc::new(Cell::new(0));
/// let notified_count_for_first = notified_count.clone();
/// let notified_count_for_second = notified_count.clone();
/// let first_subscription: PersistedStoreSubscription = store.subscribe(move |_state| {
/// 	notified_count_for_first.set(notified_count_for_first.get() + 1);
/// });
/// let second_subscription = store.subscribe(move |_state| {
/// 	notified_count_for_second.set(notified_count_for_second.get() + 10);
/// });
/// assert_ne!(first_subscription, second_subscription);
/// store.unsubscribe(first_subscription);
/// store.set(serde_json::json!({ "stage": 3 }));
/// assert_eq!(notified_count.get(), 10);
/// let _ = std::fs::remove_dir_all(&storage_directory_path);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PersistedStoreSubscription(u64);

// 구독자. (상태를 받는다)
type PersistedStoreSubscriber = Box<dyn FnMut(&Value)>;

//==============================================================================
/// 지속 저장소.
///
/// # 예제
/// 설정을 저장하고 바뀔 때 알림 받기
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{LocalStorage, PersistedStore};
///
/// let storage_directory_path = std::env::temp_dir().join(format!("vanillaPersistedStoreExample-{}", std::process::id()));
/// LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
/// let mut store = PersistedStore::new("example.settings", serde_json::json!({ "volume": 1, "language": "en" }), 1);
/// let changed_count = Rc::new(Cell::new(0));
/// let changed_count_for_subscriber = changed_count.clone();
/// let subscription = store.subscribe(move |state| {
/// 	println!("changed {}", state["volume"]);
/// 	changed_count_for_subscriber.set(changed_count_for_subscriber.get() + 1);
/// });
/// store.set(serde_json::json!({ "volume": 0.5 }));
/// let state = store.get();
/// assert_eq!(state["volume"], 0.5);
/// assert_eq!(state["language"], "en");
/// assert_eq!(changed_count.get(), 1);
/// let reloaded_store = PersistedStore::new("example.settings", serde_json::json!({ "volume": 1, "language": "en" }), 1);
/// assert_eq!(reloaded_store.get()["volume"], 0.5);
/// store.unsubscribe(subscription);
/// store.reset();
/// assert_eq!(store.get()["volume"], 1);
/// assert_eq!(changed_count.get(), 1);
/// let _ = std::fs::remove_dir_all(&storage_directory_path);
/// ```
//==============================================================================
pub struct PersistedStore {
	storage_key: String,
	version: i64,
	default_state: Map<String, Value>,
	state: Value,
	subscriber_list: Vec<(PersistedStoreSubscription, PersistedStoreSubscriber)>,
	next_subscription_number: u64,
	/// 이 저장소만 쓰는 곳. (None 이면 LocalStorage. 쓰기가 &self 라 RefCell 에 두고, get_storage_backend 로 나눠 주려고 Rc 에 담는다)
	storage_backend: Option<Rc<RefCell<Box<dyn LocalStorageBackend>>>>,
	/// 마지막으로 읽은 저장 글의 짝 없는 서로게이트 표. (읽기가 &self 라 RefCell)
	surrogate_table: RefCell<JsonSurrogateTable>,
}

//==============================================================================
/// 저장 글 안의 상태 자리. (맨 위 {"version", "state"} 에서부터의 경로, 서로게이트 표의 자리)
//==============================================================================
fn create_state_path(path: &[JsonPathSegment]) -> Vec<JsonPathSegment> {
	let mut state_path = Vec::with_capacity(path.len() + 1);
	state_path.push(JsonPathSegment::Key(String::from("state")));
	state_path.extend_from_slice(path);
	state_path
}

//==============================================================================
/// 값을 객체 맵으로. (원본 Object.assign({}, 값): 객체는 그대로, 배열은 번호 키로, 그 밖은 빈 맵)
//==============================================================================
fn to_object_map(value: Value) -> Map<String, Value> {
	let mut object_map = Map::new();
	assign_object_value(&mut object_map, value);
	order_object_keys(object_map)
}

//==============================================================================
/// 원본 Object.assign 의 원천 하나를 맵에 덮기. (객체는 키마다, 배열은 번호 키("0", "1" ...)로, 그 밖의 값은 덮을 것이 없다)
//==============================================================================
fn assign_object_value(target_map: &mut Map<String, Value>, source_value: Value) {
	match source_value {
		Value::Object(object_map) => {
			for (key, value) in object_map {
				target_map.insert(key, value);
			}
		}
		Value::Array(value_list) => {
			for (value_index, value) in value_list.into_iter().enumerate() {
				target_map.insert(value_index.to_string(), value);
			}
		}
		_ => {
		}
	}
}

//==============================================================================
/// 배열 번호 꼴 키인지. (JS 객체가 앞에 두는 키: 앞에 0 이 붙지 않은 십진 정수, 2^32 - 2 까지)
//==============================================================================
fn is_array_index_key(key: &str) -> bool {
	if key.is_empty() || !key.bytes().all(|key_byte| key_byte.is_ascii_digit()) {
		return false;
	}
	if key.len() > 1 && key.starts_with('0') {
		return false;
	}
	match key.parse::<u64>() {
		Ok(index) => {
			index < 4294967295
		}
		Err(_) => {
			false
		}
	}
}

//==============================================================================
/// 키를 JS 객체의 차례로. (배열 번호 꼴 키를 작은 수부터 앞에, 나머지는 넣은 차례. JSON.stringify 가 쓰는 차례와 같다)
//==============================================================================
fn order_object_keys(object_map: Map<String, Value>) -> Map<String, Value> {
	if !object_map.keys().any(|key| is_array_index_key(key)) {
		return object_map;
	}
	let mut index_entry_list: Vec<(u64, String, Value)> = Vec::new();
	let mut other_entry_list: Vec<(String, Value)> = Vec::new();
	for (key, value) in object_map {
		if is_array_index_key(&key) {
			let index = key.parse::<u64>().unwrap_or(0);
			index_entry_list.push((index, key, value));
		}
		else {
			other_entry_list.push((key, value));
		}
	}
	index_entry_list.sort_by_key(|(index, _, _)| *index);
	let mut ordered_map = Map::new();
	for (_, key, value) in index_entry_list {
		ordered_map.insert(key, value);
	}
	for (key, value) in other_entry_list {
		ordered_map.insert(key, value);
	}
	ordered_map
}

impl PersistedStore {
	//==============================================================================
	/// 생성. (만들면서 저장소에서 읽는다)
	///
	/// # 인자
	/// - `storage_key`: 상태를 둘 저장소 키.
	/// - `default_state`: 기본 상태. (저장된 것이 없거나 판이 다르면 이것을 복사해 쓴다)
	/// - `version`: 저장 꼴의 판 번호. (저장된 판과 다르면 기본 상태로 되돌아간다)
	///
	/// # 돌려줌
	/// 저장소에서 상태를 읽어 둔 지속 저장소.
	//==============================================================================
	pub fn new(storage_key: &str, default_state: Value, version: i64) -> PersistedStore {
		let mut persisted_store = PersistedStore {
			storage_key: storage_key.to_string(),
			version,
			default_state: to_object_map(default_state),
			state: Value::Object(Map::new()),
			subscriber_list: Vec::new(),
			next_subscription_number: 1,
			storage_backend: None,
			surrogate_table: RefCell::new(JsonSurrogateTable::new()),
		};
		persisted_store.state = persisted_store.read_from_storage();
		persisted_store
	}

	//==============================================================================
	/// 상태 반환. (객체 Value. 고칠 때는 set / replace 를 쓴다)
	///
	/// # 돌려줌
	/// 지금 상태.
	//==============================================================================
	pub fn get(&self) -> &Value {
		&self.state
	}

	//==============================================================================
	/// 상태 일부 갱신. (얕은 병합 뒤 저장하고 구독자에게 알린다)
	///
	/// # 인자
	/// - `partial_state`: 바꿀 키와 값만 담은 객체.
	//==============================================================================
	pub fn set(&mut self, partial_state: Value) {
		let mut merged_state = to_object_map(self.state.clone());
		assign_object_value(&mut merged_state, partial_state);
		self.state = Value::Object(order_object_keys(merged_state));
		self.write_to_storage();
		self.notify();
	}

	//==============================================================================
	/// 상태 안 글의 JS 글 단위. (UTF-16, 짝 없는 서로게이트도 원본 글 그대로, LocalStorage::get_item_code_units 와 같은 꼴)
	///
	/// # 인자
	/// - `path`: 상태에서부터의 글 자리. (키와 배열 차례)
	///
	/// # 돌려줌
	/// 그 자리 글의 UTF-16 단위. (원본이 저장한 짝 없는 서로게이트가 있으면 그것, 아니면 글을 UTF-16 으로 편 것) 그 자리가 글이 아니면 None.
	///
	/// # 예제
	/// 글 자리의 JS 글 단위 읽고 쓰기
	/// ```rust
	/// use serde_json::json;
	/// use vanilla::misc::storagetext::JsonPathSegment;
	/// use vanilla::{LocalStorage, PersistedStore};
	///
	/// LocalStorage::set_storage_directory(Some(std::env::temp_dir().join("vanilla_doctest_persistedstore_code_units")));
	/// let mut store = PersistedStore::new("doctest.code_units", json!({ "name": "" }), 1);
	/// let name_path = [JsonPathSegment::Key(String::from("name"))];
	/// store.set_string_code_units(&name_path, &[0x61, 0xD83D]);
	/// assert_eq!(store.get()["name"], json!("a\u{fffd}"));
	/// assert_eq!(store.get_string_code_units(&name_path), Some(vec![0x61, 0xD83D]));
	/// assert_eq!(store.read_stored_text(), r#"{"version":1,"state":{"name":"a\ud83d"}}"#);
	/// ```
	//==============================================================================
	pub fn get_string_code_units(&self, path: &[JsonPathSegment]) -> Option<Vec<u16>> {
		let mut value = &self.state;
		for segment in path {
			value = match segment {
				JsonPathSegment::Key(key) => value.get(key.as_str())?,
				JsonPathSegment::Index(index) => value.get(*index)?,
			};
		}
		let text = value.as_str()?;
		let state_path = create_state_path(path);
		match self.surrogate_table.borrow().find_value_code_units(&state_path, text) {
			Some(code_units) => {
				Some(code_units.to_vec())
			}
			None => {
				Some(text.encode_utf16().collect())
			}
		}
	}

	//==============================================================================
	/// 상태 안 글을 JS 글 단위로 쓰기. (짝 없는 서로게이트도 원본처럼 \udXXX 로 저장한다, 저장하고 구독자에게 알린다)
	/// - 없는 키는 객체로 만들며 내려간다. 배열 차례가 없거나 객체도 배열도 아닌 값을 지나야 하면 쓰지 않는다.
	///
	/// # 인자
	/// - `path`: 상태에서부터의 글 자리. (키와 배열 차례, 비면 쓰지 않는다)
	/// - `code_units`: 쓸 UTF-16 단위.
	///
	/// # 돌려줌
	/// 썼으면 true.
	//==============================================================================
	pub fn set_string_code_units(&mut self, path: &[JsonPathSegment], code_units: &[u16]) -> bool {
		let preserved_text = PreservedText::from_code_units(code_units);
		let lossy_text = preserved_text.get_lossy_text().to_string();
		let (last_segment, parent_path) = match path.split_last() {
			Some(split_path) => {
				split_path
			}
			None => {
				return false;
			}
		};
		let mut value = &mut self.state;
		for segment in parent_path {
			let next_value = match segment {
				JsonPathSegment::Key(key) => {
					match value.as_object_mut() {
						Some(object_map) => {
							object_map.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()))
						}
						None => {
							return false;
						}
					}
				}
				JsonPathSegment::Index(index) => {
					match value.get_mut(*index) {
						Some(element_value) => {
							element_value
						}
						None => {
							return false;
						}
					}
				}
			};
			value = next_value;
		}
		match last_segment {
			JsonPathSegment::Key(key) => {
				match value.as_object_mut() {
					Some(object_map) => {
						object_map.insert(key.clone(), Value::String(lossy_text));
					}
					None => {
						return false;
					}
				}
			}
			JsonPathSegment::Index(index) => {
				match value.get_mut(*index) {
					Some(element_value) => {
						*element_value = Value::String(lossy_text);
					}
					None => {
						return false;
					}
				}
			}
		}
		let state_path = create_state_path(path);
		self.surrogate_table.borrow_mut().record(Some(&state_path), preserved_text);
		self.write_to_storage();
		self.notify();
		true
	}

	//==============================================================================
	/// 상태 전체 교체.
	///
	/// # 인자
	/// - `next_state`: 새 상태. (배열은 번호 키의 객체로 펴고, 객체도 배열도 아니면 빈 객체로 친다)
	//==============================================================================
	pub fn replace(&mut self, next_state: Value) {
		self.state = Value::Object(to_object_map(next_state));
		self.write_to_storage();
		self.notify();
	}

	//==============================================================================
	/// 기본값으로 초기화.
	//==============================================================================
	pub fn reset(&mut self) {
		let default_state = Value::Object(self.default_state.clone());
		self.replace(default_state);
	}

	//==============================================================================
	/// 구독. (구독 번호를 돌려준다)
	///
	/// # 인자
	/// - `subscriber`: 알릴 때마다(set, replace, reset, set_storage_backend, notify) 지금 상태를 받는 함수.
	///
	/// # 돌려줌
	/// 구독 번호. (unsubscribe 에 넘긴다)
	//==============================================================================
	pub fn subscribe(&mut self, subscriber: impl FnMut(&Value) + 'static) -> PersistedStoreSubscription {
		let subscription = PersistedStoreSubscription(self.next_subscription_number);
		self.next_subscription_number += 1;
		self.subscriber_list.push((subscription, Box::new(subscriber)));
		subscription
	}

	//==============================================================================
	/// 구독 해지. (원본 subscribe 가 돌려준 해지 함수 자리)
	///
	/// # 인자
	/// - `subscription`: subscribe 가 돌려준 구독 번호. (모르는 것이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn unsubscribe(&mut self, subscription: PersistedStoreSubscription) {
		self.subscriber_list.retain(|(candidate_subscription, _)| *candidate_subscription != subscription);
	}

	//==============================================================================
	/// 구독자 알림. (구독한 순서대로)
	//==============================================================================
	pub fn notify(&mut self) {
		for (_, subscriber) in self.subscriber_list.iter_mut() {
			subscriber(&self.state);
		}
	}

	//==============================================================================
	/// 저장소에서 읽기. (기본값 위에 저장된 state 를 덮는다. 기본값에 없던 키도 살리고, 새로 생긴 기본 키는 기본값으로 채운다)
	/// - state 가 객체나 배열이 아니면 기본값이다. (원본 typeof state === "object", 배열은 번호 키로 섞는다)
	///
	/// # 돌려줌
	/// 저장된 상태를 기본 상태 위에 덮은 새 객체. (없거나, 판이 다르거나, 풀 수 없으면 기본 상태의 복사본)
	//==============================================================================
	pub fn read_from_storage(&self) -> Value {
		let default_state = Value::Object(self.default_state.clone());
		*self.surrogate_table.borrow_mut() = JsonSurrogateTable::new();
		let stored_code_units = self.read_stored_code_units();
		if stored_code_units.is_empty() {
			return default_state;
		}
		let parsed_value: Value = match storagetext::parse_json_code_units(&stored_code_units) {
			Ok((parsed_value, surrogate_table)) => {
				*self.surrogate_table.borrow_mut() = surrogate_table;
				parsed_value
			}
			Err(_) => {
				return default_state;
			}
		};
		let stored_version = parsed_value.get("version").and_then(|version_value| version_value.as_f64());
		if stored_version != Some(self.version as f64) {
			return default_state;
		}
		let stored_state = match parsed_value.get("state") {
			Some(stored_state) if stored_state.is_object() || stored_state.is_array() => {
				stored_state.clone()
			}
			_ => {
				return default_state;
			}
		};
		let mut merged_state = self.default_state.clone();
		assign_object_value(&mut merged_state, stored_state);
		Value::Object(order_object_keys(merged_state))
	}

	//==============================================================================
	/// 저장소에 쓰기. ({"version": 판 번호, "state": 상태})
	//==============================================================================
	pub fn write_to_storage(&self) {
		let mut payload = Map::new();
		payload.insert(String::from("version"), Value::from(self.version));
		payload.insert(String::from("state"), self.state.clone());
		let payload_text = storagetext::stringify_json(&Value::Object(payload), &self.surrogate_table.borrow());
		match &self.storage_backend {
			Some(storage_backend) => {
				let write_result = match storage_backend.try_borrow_mut() {
					Ok(mut storage_backend) => {
						storage_backend.set_item(&self.storage_key, &payload_text)
					}
					Err(_) => {
						Err(String::from("저장소를 빌려 쓰는 중"))
					}
				};
				if let Err(error_message) = write_result {
					eprintln!("PersistedStore: 저장소에 쓰지 못했습니다 {} ({})", self.storage_key, error_message);
				}
			}
			None => {
				LocalStorage::set_string(&self.storage_key, &payload_text);
			}
		}
	}

	//==============================================================================
	/// 저장된 글 읽기. (없거나 못 읽으면 빈 글)
	///
	/// # 돌려줌
	/// 저장된 JSON 글. 없거나 못 읽으면 빈 글.
	//==============================================================================
	pub fn read_stored_text(&self) -> String {
		match &self.storage_backend {
			Some(storage_backend) => {
				let read_result = match storage_backend.try_borrow() {
					Ok(storage_backend) => {
						storage_backend.get_item(&self.storage_key)
					}
					Err(_) => {
						Ok(None)
					}
				};
				match read_result {
					Ok(Some(stored_text)) => {
						stored_text
					}
					_ => {
						String::new()
					}
				}
			}
			None => {
				LocalStorage::get_string(&self.storage_key, "")
			}
		}
	}

	//==============================================================================
	/// 저장된 글을 UTF-16 단위로 읽기. (LocalStorage 는 원래 단위 그대로, 갈아 끼운 저장소는 그 글. 없거나 못 읽으면 빈 것)
	//==============================================================================
	fn read_stored_code_units(&self) -> Vec<u16> {
		match &self.storage_backend {
			Some(_) => {
				self.read_stored_text().encode_utf16().collect()
			}
			None => {
				LocalStorage::get_item_code_units(&self.storage_key).unwrap_or_default()
			}
		}
	}

	//==============================================================================
	/// 이 저장소만 쓰는 곳 갈아 끼우기. (None 이면 LocalStorage. 그곳에서 상태를 다시 읽고 구독자에게 알린다)
	///
	/// # 인자
	/// - `storage_backend`: 이 저장소만 쓸 곳. (None 이면 LocalStorage)
	//==============================================================================
	pub fn set_storage_backend(&mut self, storage_backend: Option<Box<dyn LocalStorageBackend>>) {
		self.storage_backend = storage_backend.map(|storage_backend| Rc::new(RefCell::new(storage_backend)));
		self.state = self.read_from_storage();
		self.notify();
	}

	//==============================================================================
	/// 이 저장소만 쓰는 곳 반환. (없으면 None, 빌릴 때는 borrow / borrow_mut)
	///
	/// # 돌려줌
	/// 이 저장소만 쓰는 곳. 없으면 None.
	//==============================================================================
	pub fn get_storage_backend(&self) -> Option<Rc<RefCell<Box<dyn LocalStorageBackend>>>> {
		self.storage_backend.clone()
	}

	//==============================================================================
	/// 저장 키 반환.
	///
	/// # 돌려줌
	/// 저장소 키.
	//==============================================================================
	pub fn get_storage_key(&self) -> &str {
		&self.storage_key
	}
}

//==============================================================================
/// 시험. (데스크탑, 임시 폴더에 쓴다)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use serde_json::json;
	use std::cell::Cell;
	use std::path::PathBuf;
	use std::rc::Rc;

	//==============================================================================
	/// 시험마다 다른 임시 폴더를 저장 폴더로 정한다.
	//==============================================================================
	fn use_temporary_storage_directory(test_name: &str) -> PathBuf {
		let unique_suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0);
		let storage_directory_path = std::env::temp_dir().join(format!("vanilla-persistedstore-{}-{}-{}", test_name, std::process::id(), unique_suffix));
		LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
		storage_directory_path
	}

	//==============================================================================
	/// 저장, 다시 읽기, 병합, 구독, 판 번호가 다르면 기본값.
	//==============================================================================
	#[test]
	fn store_round_trip_and_version() {
		let storage_directory_path = use_temporary_storage_directory("roundtrip");
		let mut store = PersistedStore::new("game.settings", json!({ "volume": 0.8, "isMuted": false }), 2);
		assert_eq!(store.get()["volume"], json!(0.8));

		let notify_count = Rc::new(Cell::new(0));
		let notify_count_for_subscriber = notify_count.clone();
		let subscription = store.subscribe(move |state| {
			assert!(state.get("volume").is_some());
			notify_count_for_subscriber.set(notify_count_for_subscriber.get() + 1);
		});
		store.set(json!({ "volume": 0.5, "language": "ko" }));
		assert_eq!(notify_count.get(), 1);
		assert_eq!(store.get()["isMuted"], json!(false));

		// 다시 읽으면 저장한 값, 새로 생긴 기본 키는 기본값.
		LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
		let reloaded_store = PersistedStore::new("game.settings", json!({ "volume": 0.8, "isMuted": false, "isVibration": true }), 2);
		assert_eq!(reloaded_store.get()["volume"], json!(0.5));
		assert_eq!(reloaded_store.get()["language"], json!("ko"));
		assert_eq!(reloaded_store.get()["isVibration"], json!(true));

		// 판 번호가 다르면 기본값.
		let new_version_store = PersistedStore::new("game.settings", json!({ "volume": 0.8 }), 3);
		assert_eq!(new_version_store.get(), &json!({ "volume": 0.8 }));

		// 깨진 글이면 기본값.
		LocalStorage::set_string("game.broken", "{not json");
		let broken_store = PersistedStore::new("game.broken", json!({ "count": 1 }), 1);
		assert_eq!(broken_store.get(), &json!({ "count": 1 }));

		store.unsubscribe(subscription);
		store.reset();
		assert_eq!(notify_count.get(), 1);
		assert_eq!(store.get(), &json!({ "volume": 0.8, "isMuted": false }));
		assert_eq!(store.get_storage_key(), "game.settings");

		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	/// 시험용 바꿔 끼우는 저장소. (값은 나눠 든 맵)
	//==============================================================================
	struct MapBackend {
		item_map: Rc<std::cell::RefCell<std::collections::BTreeMap<String, String>>>,
	}

	impl LocalStorageBackend for MapBackend {
		//==============================================================================
		/// 값 읽기.
		//==============================================================================
		fn get_item(&self, key: &str) -> Result<Option<String>, String> {
			Ok(self.item_map.borrow().get(key).cloned())
		}

		//==============================================================================
		/// 값 쓰기.
		//==============================================================================
		fn set_item(&mut self, key: &str, value: &str) -> Result<(), String> {
			self.item_map.borrow_mut().insert(key.to_string(), value.to_string());
			Ok(())
		}

		//==============================================================================
		/// 값 지우기.
		//==============================================================================
		fn remove_item(&mut self, key: &str) -> Result<(), String> {
			self.item_map.borrow_mut().remove(key);
			Ok(())
		}
	}

	//==============================================================================
	/// 저장소 하나만 다른 곳에 두기. (갈아 끼우면 다시 읽고 알린다)
	//==============================================================================
	#[test]
	fn store_storage_backend() {
		let storage_directory_path = use_temporary_storage_directory("backend");
		let mut store = PersistedStore::new("settings", json!({ "volume": 1 }), 1);
		store.set(json!({ "volume": 0.5 }));
		let notified_volume = Rc::new(std::cell::RefCell::new(Value::Null));
		let notified_volume_for_subscriber = notified_volume.clone();
		store.subscribe(move |state| {
			*notified_volume_for_subscriber.borrow_mut() = state["volume"].clone();
		});
		let item_map = Rc::new(std::cell::RefCell::new(std::collections::BTreeMap::new()));
		store.set_storage_backend(Some(Box::new(MapBackend { item_map: item_map.clone() })));
		let storage_backend = store.get_storage_backend().expect("storage backend");
		assert_eq!(storage_backend.borrow().get_item("settings"), Ok(None));
		assert_eq!(store.get()["volume"], json!(1));
		assert_eq!(*notified_volume.borrow(), json!(1));
		store.set(json!({ "volume": 0.25 }));
		assert_eq!(item_map.borrow().get("settings").map(|text| text.as_str()), Some("{\"version\":1,\"state\":{\"volume\":0.25}}"));
		store.set_storage_backend(None);
		assert_eq!(store.get()["volume"], json!(0.5));
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	/// 배열은 원본 Object.assign 처럼 번호 키로 섞고, 번호 키를 앞에 둔다. 저장된 글을 그대로 읽을 수 있다.
	/// - 기대 글은 node 에서 JSON.stringify(Object.assign({}, { b: 1, z: 0 }, ["x", "y"])) 로 찍은 것과 같다.
	//==============================================================================
	#[test]
	fn array_state_merges_with_index_keys() {
		let storage_directory_path = use_temporary_storage_directory("array");
		LocalStorage::set_string("game.array", "{\"version\":1,\"state\":[\"x\",\"y\"]}");
		let mut store = PersistedStore::new("game.array", json!({ "b": 1, "z": 0 }), 1);
		assert_eq!(store.get().to_string(), "{\"0\":\"x\",\"1\":\"y\",\"b\":1,\"z\":0}");
		assert_eq!(store.read_stored_text(), "{\"version\":1,\"state\":[\"x\",\"y\"]}");
		store.set(json!(["w"]));
		assert_eq!(store.get()["0"], json!("w"));
		assert_eq!(store.get()["1"], json!("y"));
		store.set(json!({ "10": true, "2": false, "a": null }));
		assert_eq!(store.get().to_string(), "{\"0\":\"w\",\"1\":\"y\",\"2\":false,\"10\":true,\"b\":1,\"z\":0,\"a\":null}");
		store.replace(json!(7));
		assert_eq!(store.get(), &json!({}));
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	/// 원본이 저장한 짝 없는 서로게이트가 든 글. (상태에서는 대체 문자로 보이고, 다른 칸을 고쳐 다시 써도 그 글은 원본 꼴 그대로)
	/// - 저장 글은 node 에서 JSON.stringify({ version: 1, state: { name: "a" + "\ud83d", best: 3 } }) 로 찍은 것과 같다.
	//==============================================================================
	#[test]
	fn keeps_lone_surrogate_written_by_original() {
		let storage_directory_path = use_temporary_storage_directory("surrogate");
		LocalStorage::set_string("game.profile", "{\"version\":1,\"state\":{\"name\":\"a\\ud83d\",\"best\":3}}");
		let mut store = PersistedStore::new("game.profile", json!({ "name": "", "best": 0 }), 1);
		assert_eq!(store.get()["name"], json!("a\u{fffd}"));
		assert_eq!(store.get()["best"], json!(3));
		store.set(json!({ "best": 4 }));
		assert_eq!(store.read_stored_text(), "{\"version\":1,\"state\":{\"name\":\"a\\ud83d\",\"best\":4}}");
		store.set(json!({ "name": "b" }));
		assert_eq!(store.read_stored_text(), "{\"version\":1,\"state\":{\"name\":\"b\",\"best\":4}}");
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}
}
