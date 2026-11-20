//==============================================================================
// 지속 저장소.
// - 원본 misc/persistedstore.js 를 옮긴 것이다. 키 하나에 상태 객체를 JSON 으로 저장하고, 바뀔 때 구독자에게 알린다.
// - 판 번호(version)가 다르거나 해석에 실패하면 기본값으로 돌아간다.
// - 저장은 LocalStorage 를 쓴다. 저장 꼴({"version":1,"state":{...}})이 원본과 같아서 웹에서는 원본 게임의 저장을 그대로 읽는다.
// - 사용:
//     let mut store = PersistedStore::new("game.settings", serde_json::json!({ "volume": 0.8, "is_muted": false }), 1);
//     store.set(serde_json::json!({ "volume": 0.5 }));
//     let volume = store.get()["volume"].as_f64();
// - 원본과 다른 점:
//   상태 객체는 serde_json::Value(객체)다. 객체가 아닌 값을 넘기면 빈 객체로 친다.
//   subscribe 는 해지 함수 대신 구독 번호(PersistedStoreSubscription)를 돌려주고, unsubscribe(번호) 로 푼다.
//   저장된 state 가 배열이면 원본은 번호 키로 섞지만, 러스트 판은 기본값으로 돌아간다.
//==============================================================================
use crate::misc::localstorage::LocalStorage;
use serde_json::{Map, Value};

//==============================================================================
// 구독 번호. (unsubscribe 에 넘긴다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PersistedStoreSubscription(u64);

// 구독자. (상태를 받는다)
type PersistedStoreSubscriber = Box<dyn FnMut(&Value)>;

//==============================================================================
// 지속 저장소.
//==============================================================================
pub struct PersistedStore {
	storage_key: String,
	version: i64,
	default_state: Map<String, Value>,
	state: Value,
	subscriber_list: Vec<(PersistedStoreSubscription, PersistedStoreSubscriber)>,
	next_subscription_number: u64,
}

//==============================================================================
// 값을 객체 맵으로. (객체가 아니면 빈 맵)
//==============================================================================
fn to_object_map(value: Value) -> Map<String, Value> {
	match value {
		Value::Object(object_map) => {
			object_map
		}
		_ => {
			Map::new()
		}
	}
}

impl PersistedStore {
	//==============================================================================
	// 생성. (만들면서 저장소에서 읽는다)
	//==============================================================================
	pub fn new(storage_key: &str, default_state: Value, version: i64) -> PersistedStore {
		let mut persisted_store = PersistedStore {
			storage_key: storage_key.to_string(),
			version,
			default_state: to_object_map(default_state),
			state: Value::Object(Map::new()),
			subscriber_list: Vec::new(),
			next_subscription_number: 1,
		};
		persisted_store.state = persisted_store.read_from_storage();
		persisted_store
	}

	//==============================================================================
	// 상태 반환. (객체 Value. 고칠 때는 set / replace 를 쓴다)
	//==============================================================================
	pub fn get(&self) -> &Value {
		&self.state
	}

	//==============================================================================
	// 상태 일부 갱신. (얕은 병합 뒤 저장하고 구독자에게 알린다)
	//==============================================================================
	pub fn set(&mut self, partial_state: Value) {
		let mut merged_state = to_object_map(self.state.clone());
		for (key, value) in to_object_map(partial_state) {
			merged_state.insert(key, value);
		}
		self.state = Value::Object(merged_state);
		self.write_to_storage();
		self.notify();
	}

	//==============================================================================
	// 상태 전체 교체.
	//==============================================================================
	pub fn replace(&mut self, next_state: Value) {
		self.state = Value::Object(to_object_map(next_state));
		self.write_to_storage();
		self.notify();
	}

	//==============================================================================
	// 기본값으로 초기화.
	//==============================================================================
	pub fn reset(&mut self) {
		let default_state = Value::Object(self.default_state.clone());
		self.replace(default_state);
	}

	//==============================================================================
	// 구독. (구독 번호를 돌려준다)
	//==============================================================================
	pub fn subscribe(&mut self, subscriber: impl FnMut(&Value) + 'static) -> PersistedStoreSubscription {
		let subscription = PersistedStoreSubscription(self.next_subscription_number);
		self.next_subscription_number += 1;
		self.subscriber_list.push((subscription, Box::new(subscriber)));
		subscription
	}

	//==============================================================================
	// 구독 해지. (원본 subscribe 가 돌려준 해지 함수 자리)
	//==============================================================================
	pub fn unsubscribe(&mut self, subscription: PersistedStoreSubscription) {
		self.subscriber_list.retain(|(candidate_subscription, _)| *candidate_subscription != subscription);
	}

	//==============================================================================
	// 구독자 알림. (구독한 순서대로)
	//==============================================================================
	pub fn notify(&mut self) {
		for (_, subscriber) in self.subscriber_list.iter_mut() {
			subscriber(&self.state);
		}
	}

	//==============================================================================
	// 저장소에서 읽기. (기본값 위에 저장된 state 를 덮는다. 기본값에 없던 키도 살리고, 새로 생긴 기본 키는 기본값으로 채운다)
	//==============================================================================
	pub fn read_from_storage(&self) -> Value {
		let default_state = Value::Object(self.default_state.clone());
		let stored_text = LocalStorage::get_string(&self.storage_key, "");
		if stored_text.is_empty() {
			return default_state;
		}
		let parsed_value: Value = match serde_json::from_str(&stored_text) {
			Ok(parsed_value) => {
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
			Some(Value::Object(stored_state)) => {
				stored_state
			}
			_ => {
				return default_state;
			}
		};
		let mut merged_state = self.default_state.clone();
		for (key, value) in stored_state {
			merged_state.insert(key.clone(), value.clone());
		}
		Value::Object(merged_state)
	}

	//==============================================================================
	// 저장소에 쓰기. ({"version": 판 번호, "state": 상태})
	//==============================================================================
	pub fn write_to_storage(&self) {
		let mut payload = Map::new();
		payload.insert(String::from("version"), Value::from(self.version));
		payload.insert(String::from("state"), self.state.clone());
		let payload_text = Value::Object(payload).to_string();
		LocalStorage::set_string(&self.storage_key, &payload_text);
	}

	//==============================================================================
	// 저장 키 반환.
	//==============================================================================
	pub fn get_storage_key(&self) -> &str {
		&self.storage_key
	}
}

//==============================================================================
// 시험. (데스크탑, 임시 폴더에 쓴다)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use serde_json::json;
	use std::cell::Cell;
	use std::path::PathBuf;
	use std::rc::Rc;

	//==============================================================================
	// 시험마다 다른 임시 폴더를 저장 폴더로 정한다.
	//==============================================================================
	fn use_temporary_storage_directory(test_name: &str) -> PathBuf {
		let unique_suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0);
		let storage_directory_path = std::env::temp_dir().join(format!("vanilla-persistedstore-{}-{}-{}", test_name, std::process::id(), unique_suffix));
		LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
		storage_directory_path
	}

	//==============================================================================
	// 저장, 다시 읽기, 병합, 구독, 판 번호가 다르면 기본값.
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
}
