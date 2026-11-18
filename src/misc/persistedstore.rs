//==============================================================================
// 지속 저장소.
// - 원본 misc/persistedstore.js 를 옮긴 것이다. 키 하나에 상태 객체를 JSON 으로 저장하고, 바뀔 때 구독자에게 알린다.
// - 판 번호(version)가 다르거나 해석에 실패하면 기본값으로 돌아간다.
// - 저장은 LocalStorage 를 쓴다. 저장 꼴({"version":1,"state":{...}})이 원본과 같아서 웹에서는 원본 게임의 저장을 그대로 읽는다.
// - 사용:
//     let mut store = PersistedStore::new("game.settings", serde_json::json!({ "volume": 0.8, "isMuted": false }), 1);
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
	storageKey: String,
	version: i64,
	defaultState: Map<String, Value>,
	state: Value,
	subscriberList: Vec<(PersistedStoreSubscription, PersistedStoreSubscriber)>,
	nextSubscriptionNumber: u64,
}

//==============================================================================
// 값을 객체 맵으로. (객체가 아니면 빈 맵)
//==============================================================================
fn toObjectMap(value: Value) -> Map<String, Value> {
	match value {
		Value::Object(objectMap) => {
			objectMap
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
	pub fn new(storageKey: &str, defaultState: Value, version: i64) -> PersistedStore {
		let mut persistedStore = PersistedStore {
			storageKey: storageKey.to_string(),
			version,
			defaultState: toObjectMap(defaultState),
			state: Value::Object(Map::new()),
			subscriberList: Vec::new(),
			nextSubscriptionNumber: 1,
		};
		persistedStore.state = persistedStore.readFromStorage();
		persistedStore
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
	pub fn set(&mut self, partialState: Value) {
		let mut mergedState = toObjectMap(self.state.clone());
		for (key, value) in toObjectMap(partialState) {
			mergedState.insert(key, value);
		}
		self.state = Value::Object(mergedState);
		self.writeToStorage();
		self.notify();
	}

	//==============================================================================
	// 상태 전체 교체.
	//==============================================================================
	pub fn replace(&mut self, nextState: Value) {
		self.state = Value::Object(toObjectMap(nextState));
		self.writeToStorage();
		self.notify();
	}

	//==============================================================================
	// 기본값으로 초기화.
	//==============================================================================
	pub fn reset(&mut self) {
		let defaultState = Value::Object(self.defaultState.clone());
		self.replace(defaultState);
	}

	//==============================================================================
	// 구독. (구독 번호를 돌려준다)
	//==============================================================================
	pub fn subscribe(&mut self, subscriber: impl FnMut(&Value) + 'static) -> PersistedStoreSubscription {
		let subscription = PersistedStoreSubscription(self.nextSubscriptionNumber);
		self.nextSubscriptionNumber += 1;
		self.subscriberList.push((subscription, Box::new(subscriber)));
		subscription
	}

	//==============================================================================
	// 구독 해지. (원본 subscribe 가 돌려준 해지 함수 자리)
	//==============================================================================
	pub fn unsubscribe(&mut self, subscription: PersistedStoreSubscription) {
		self.subscriberList.retain(|(candidateSubscription, _)| *candidateSubscription != subscription);
	}

	//==============================================================================
	// 구독자 알림. (구독한 순서대로)
	//==============================================================================
	pub fn notify(&mut self) {
		for (_, subscriber) in self.subscriberList.iter_mut() {
			subscriber(&self.state);
		}
	}

	//==============================================================================
	// 저장소에서 읽기. (기본값 위에 저장된 state 를 덮는다. 기본값에 없던 키도 살리고, 새로 생긴 기본 키는 기본값으로 채운다)
	//==============================================================================
	pub fn readFromStorage(&self) -> Value {
		let defaultState = Value::Object(self.defaultState.clone());
		let storedText = LocalStorage::getString(&self.storageKey, "");
		if storedText.is_empty() {
			return defaultState;
		}
		let parsedValue: Value = match serde_json::from_str(&storedText) {
			Ok(parsedValue) => {
				parsedValue
			}
			Err(_) => {
				return defaultState;
			}
		};
		let storedVersion = parsedValue.get("version").and_then(|versionValue| versionValue.as_f64());
		if storedVersion != Some(self.version as f64) {
			return defaultState;
		}
		let storedState = match parsedValue.get("state") {
			Some(Value::Object(storedState)) => {
				storedState
			}
			_ => {
				return defaultState;
			}
		};
		let mut mergedState = self.defaultState.clone();
		for (key, value) in storedState {
			mergedState.insert(key.clone(), value.clone());
		}
		Value::Object(mergedState)
	}

	//==============================================================================
	// 저장소에 쓰기. ({"version": 판 번호, "state": 상태})
	//==============================================================================
	pub fn writeToStorage(&self) {
		let mut payload = Map::new();
		payload.insert(String::from("version"), Value::from(self.version));
		payload.insert(String::from("state"), self.state.clone());
		let payloadText = Value::Object(payload).to_string();
		LocalStorage::setString(&self.storageKey, &payloadText);
	}

	//==============================================================================
	// 저장 키 반환.
	//==============================================================================
	pub fn getStorageKey(&self) -> &str {
		&self.storageKey
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
	fn useTemporaryStorageDirectory(testName: &str) -> PathBuf {
		let uniqueSuffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0);
		let storageDirectoryPath = std::env::temp_dir().join(format!("vanilla-persistedstore-{}-{}-{}", testName, std::process::id(), uniqueSuffix));
		LocalStorage::setStorageDirectory(Some(storageDirectoryPath.clone()));
		storageDirectoryPath
	}

	//==============================================================================
	// 저장, 다시 읽기, 병합, 구독, 판 번호가 다르면 기본값.
	//==============================================================================
	#[test]
	fn storeRoundTripAndVersion() {
		let storageDirectoryPath = useTemporaryStorageDirectory("roundtrip");
		let mut store = PersistedStore::new("game.settings", json!({ "volume": 0.8, "isMuted": false }), 2);
		assert_eq!(store.get()["volume"], json!(0.8));

		let notifyCount = Rc::new(Cell::new(0));
		let notifyCountForSubscriber = notifyCount.clone();
		let subscription = store.subscribe(move |state| {
			assert!(state.get("volume").is_some());
			notifyCountForSubscriber.set(notifyCountForSubscriber.get() + 1);
		});
		store.set(json!({ "volume": 0.5, "language": "ko" }));
		assert_eq!(notifyCount.get(), 1);
		assert_eq!(store.get()["isMuted"], json!(false));

		// 다시 읽으면 저장한 값, 새로 생긴 기본 키는 기본값.
		LocalStorage::setStorageDirectory(Some(storageDirectoryPath.clone()));
		let reloadedStore = PersistedStore::new("game.settings", json!({ "volume": 0.8, "isMuted": false, "isVibration": true }), 2);
		assert_eq!(reloadedStore.get()["volume"], json!(0.5));
		assert_eq!(reloadedStore.get()["language"], json!("ko"));
		assert_eq!(reloadedStore.get()["isVibration"], json!(true));

		// 판 번호가 다르면 기본값.
		let newVersionStore = PersistedStore::new("game.settings", json!({ "volume": 0.8 }), 3);
		assert_eq!(newVersionStore.get(), &json!({ "volume": 0.8 }));

		// 깨진 글이면 기본값.
		LocalStorage::setString("game.broken", "{not json");
		let brokenStore = PersistedStore::new("game.broken", json!({ "count": 1 }), 1);
		assert_eq!(brokenStore.get(), &json!({ "count": 1 }));

		store.unsubscribe(subscription);
		store.reset();
		assert_eq!(notifyCount.get(), 1);
		assert_eq!(store.get(), &json!({ "volume": 0.8, "isMuted": false }));
		assert_eq!(store.getStorageKey(), "game.settings");

		let _ = std::fs::remove_dir_all(&storageDirectoryPath);
	}
}
