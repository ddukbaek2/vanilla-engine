# 저장

설정, 최고 점수, 진행 상황처럼 다음에 켜도 남아 있어야 하는 값을 저장하는 방법을 다룹니다. 키 하나에 값 하나를 쓰고 읽는 `LocalStorage` 와, 상태 객체 하나를 JSON 으로 한 키에 두고 바뀔 때 알려 주는 `PersistedStore` 차례로 봅니다.

값이 저장되는 곳은 플랫폼마다 다릅니다. 웹(wasm32)은 원본처럼 브라우저의 `window.localStorage` 이고, 데스크탑은 앱 폴더의 `localstorage.json` 파일 하나입니다. 게임 코드는 어느 쪽이든 같습니다.

## LocalStorage 로 값 하나씩 저장하기

`LocalStorage` 는 모든 함수가 연관 함수입니다. 값을 만들지 않고 `LocalStorage::set_string(...)` 처럼 바로 부릅니다. 값은 모두 글로 저장됩니다.

아래 예제는 시험으로 돌려도 실제 저장 파일을 건드리지 않게 임시 폴더를 저장 폴더로 정하고 시작합니다. 게임에서는 이 줄 없이 쓰거나 아래 "저장되는 자리" 절처럼 앱 이름을 정합니다.

```rust
use vanilla::LocalStorage;

let storage_directory_path = std::env::temp_dir().join(format!("mygame-storage-{}", std::process::id()));
LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));

LocalStorage::set_string("mygame.playerName", "Hero");
LocalStorage::set_number("mygame.bestScore", 1200.0);
LocalStorage::set_boolean("mygame.isMuted", true);
LocalStorage::set_json("mygame.inventory", &serde_json::json!({ "potion": 3, "key": 1 }));

let player_name = LocalStorage::get_string("mygame.playerName", "Guest");
let best_score = LocalStorage::get_number("mygame.bestScore", 0.0);
let is_muted = LocalStorage::get_boolean("mygame.isMuted", false);
let inventory = LocalStorage::get_json("mygame.inventory", serde_json::json!({}));
assert_eq!(player_name, "Hero");
assert_eq!(best_score, 1200.0);
assert!(is_muted);
assert_eq!(inventory["potion"], 3);
assert_eq!(LocalStorage::get_item("mygame.bestScore").as_deref(), Some("1200"));

let has_level = LocalStorage::contains_key("mygame.level");
let level = LocalStorage::get_number("mygame.level", 1.0);
assert!(!has_level);
assert_eq!(level, 1.0);

let _ = std::fs::remove_dir_all(&storage_directory_path);
```

| 쓰기 | 읽기 | 저장되는 글 |
| --- | --- | --- |
| `set_string(key, string_value)` | `get_string(key, default_string_value)` | 그대로 |
| `set_number(key, number_value)` | `get_number(key, default_number_value)` | 원본 `String(number)` 와 같은 꼴 (`1200`, `1.5`, `1e+21`) |
| `set_boolean(key, boolean_value)` | `get_boolean(key, default_boolean_value)` | `"true"` 나 `"false"` |
| `set_json(key, &value)` | `get_json(key, default_value)` | `serde_json::Value` 의 짧은 JSON 글 |

- 러스트에는 기본 인자가 없어서 읽기 함수의 기본값은 늘 넘깁니다. 숫자는 원본의 JS 수처럼 `f64` 이고, JSON 값은 `serde_json::Value` 입니다.
- 읽기 함수는 키가 없을 때 기본값을 돌려주기만 하고 저장소에 써 넣지 않습니다.
- `get_number` 는 원본 `Number(text)` 처럼 읽습니다. 앞뒤 빈칸을 떼고, `0x`, `0o`, `0b` 표기와 `Infinity` 를 읽고, 숫자가 아닌 글이면 `NaN` 입니다. 키가 없거나 빈 글이면 기본값입니다.
- `get_boolean` 은 저장된 글이 `"true"` 일 때만 참이고, 비어 있지 않은 다른 글이면 거짓입니다. 키가 없거나 빈 글이면 기본값입니다.
- `get_json` 은 글이 없거나 비었거나 해석이 실패하면 기본값을 돌려줍니다.
- `get_item(key)`, `set_item(key, value)`, `remove_item(key)` 는 글을 그대로 다루는 낮은 단계의 함수입니다. `get_item` 은 키가 없으면 `None` 입니다.
- `set_item` 과 `set_json` 은 저장소에 썼으면 `true`, 쓰지 못해 메모리에 대신 뒀으면 `false` 를 돌려줍니다. `set_string`, `set_number`, `set_boolean` 은 돌려주는 것이 없습니다.

## 저장되는 자리

웹은 원본처럼 `window.localStorage` 에 키를 그대로 씁니다. 저장소는 출처(프로토콜, 호스트, 포트)마다 하나입니다.

데스크탑은 앱 폴더의 `localstorage.json` 하나에 모든 키를 둡니다. 처음 쓸 때 파일을 읽어 두고, 값이 바뀔 때마다 파일 전체를 바로 씁니다. (임시 파일에 쓴 뒤 바꿔치기해서 중간에 꺼져도 파일이 깨지지 않습니다)

| 운영체제 | 파일 |
| --- | --- |
| 윈도우 | `%APPDATA%\{앱 이름}\localstorage.json` |
| 맥 | `$HOME/Library/Application Support/{앱 이름}/localstorage.json` |
| 리눅스 | `$XDG_DATA_HOME/{앱 이름}/localstorage.json` (없으면 `$HOME/.local/share/{앱 이름}/localstorage.json`) |

- 환경 변수를 읽지 못하면 임시 폴더(`std::env::temp_dir()`) 아래에 둡니다.
- 앱 이름은 처음에 실행 파일 이름입니다. `set_application_name(name)` 으로 바꾸거나, `set_application_name_from_engine(&engine)` 으로 `EngineConfiguration.title` 을 씁니다. 폴더 이름에 못 쓰는 글자(`<>:"/\|?*` 와 제어 문자)는 `_` 로 바꾸고, 끝의 점과 빈칸은 떼고, 비면 `vanilla` 입니다.
- `set_storage_directory(Some(path))` 는 폴더를 직접 정합니다. (시험, 들고 다니는 게임) `None` 이면 앱 이름으로 정한 폴더로 돌아갑니다.
- 앱 이름이나 폴더를 바꾸면 읽어 둔 내용을 버리고 다음에 새 자리에서 읽습니다. 그래서 첫 저장과 읽기 전에 정합니다.
- `get_storage_file_path()` 는 지금 쓰는 파일 경로입니다. 웹은 `None` 입니다.
- 웹에서는 `set_application_name`, `set_storage_directory` 가 아무것도 하지 않습니다.

```rust
use std::path::Path;
use vanilla::{Engine, EngineConfiguration, LocalStorage};

let mut engine_configuration = EngineConfiguration::new();
engine_configuration.title = String::from("My Game");
let engine = Engine::new(engine_configuration);
LocalStorage::set_application_name_from_engine(&engine);

let storage_file_path = LocalStorage::get_storage_file_path().unwrap();
assert!(storage_file_path.ends_with(Path::new("My Game").join("localstorage.json")));
```

## 키 목록과 지우기

같은 저장소를 여러 게임이 함께 쓸 수 있습니다. (웹은 같은 출처의 페이지, 데스크탑은 같은 앱 이름) 키 앞에 게임 이름 같은 접두사를 붙여 두면 다른 앱의 값과 섞이지 않고, 게임의 값만 골라 지울 수 있습니다.

```rust
use vanilla::LocalStorage;

let storage_directory_path = std::env::temp_dir().join(format!("mygame-keys-{}", std::process::id()));
LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));

LocalStorage::set_number("mygame.stage", 3.0);
LocalStorage::set_number("mygame.gold", 250.0);
LocalStorage::set_string("othergame.name", "Other");

let keys = LocalStorage::get_keys();
assert_eq!(keys, vec!["mygame.gold", "mygame.stage", "othergame.name"]);

LocalStorage::remove("mygame.stage");
let removed_count = LocalStorage::clear_by_prefix("mygame.");
assert_eq!(removed_count, 1);
assert!(LocalStorage::contains_key("othergame.name"));

let _ = std::fs::remove_dir_all(&storage_directory_path);
```

- `get_keys()` 는 저장소의 키와 메모리에 대신 둔 키(아래 절)를 합쳐 이름 차례로 정렬해 돌려줍니다.
- `remove(key)` 는 키 하나를 지웁니다. `remove_item` 과 같습니다.
- `clear_by_prefix(prefix)` 는 그 접두사로 시작하는 키를 모두 지우고 지운 수를 돌려줍니다. 빈 접두사면 아무것도 지우지 않고 0 입니다.
- `clear()` 는 저장소 전체를 비웁니다. 웹은 같은 출처의 다른 앱 값까지, 데스크탑은 그 앱 폴더 파일의 모든 키를 지우고, 개발자 도구 설정(`hierarchy.settings` 키)도 함께 지워집니다. 게임 값만 지울 때는 `clear_by_prefix` 를 씁니다.

## 저장소를 쓰지 못할 때

웹에서는 사생활 보호 창이나 쿠키를 막은 브라우저에서 `window.localStorage` 를 쓰지 못하고, 저장소가 가득 차면 쓰기가 실패합니다. 데스크탑에서는 폴더를 만들지 못하거나 파일 쓰기가 실패할 수 있습니다. `LocalStorage` 는 이럴 때 멈추지 않고, 쓰지 못한 값을 이 실행이 살아 있는 동안 메모리에 대신 둡니다.

- `set_item` 은 저장소에 썼으면 `true`, 메모리에 대신 뒀으면 `false` 를 돌려줍니다. `set_json` 도 같습니다.
- 한 번이라도 메모리로 대신했으면 `is_memory_fallback()` 이 참입니다.
- 메모리에 둔 값은 다시 켜면 사라집니다. 저장이 중요한 게임은 `is_memory_fallback()` 을 보고 "저장되지 않습니다" 같은 안내를 띄웁니다.
- 메모리에 둔 값이 있으면 읽을 때 그 값이 저장소의 값보다 앞섭니다.

## 저장소 갈아 끼우기

플랫폼 SDK 가 따로 저장소를 주는 곳에서는 `LocalStorage::set_backend(Some(backend))` 로 갈아 끼웁니다. 원본에서 `getItem`, `setItem`, `removeItem` 을 가진 객체를 넘기던 자리가 러스트 판에서는 `LocalStorageBackend` 트레이트입니다. 원본의 예외는 `Err` 로 알립니다.

| 함수 | 하는 일 |
| --- | --- |
| `get_item(&self, key)` | 값 읽기. 없으면 `Ok(None)` |
| `set_item(&mut self, key, value)` | 값 쓰기 |
| `remove_item(&mut self, key)` | 값 지우기 |
| `get_keys(&self)` | 모든 키. 기본 구현은 빈 목록이라, 구현하지 않으면 `get_keys`, `clear_by_prefix` 에 메모리에 대신 둔 키만 보입니다 |
| `clear(&mut self)` | 전부 지우기. 기본 구현은 `get_keys` 의 키를 하나씩 지웁니다 |

```rust
use std::collections::BTreeMap;
use vanilla::{LocalStorage, LocalStorageBackend};

struct MemoryBackend {
	item_map: BTreeMap<String, String>,
}

impl LocalStorageBackend for MemoryBackend {
	fn get_item(&self, key: &str) -> Result<Option<String>, String> {
		Ok(self.item_map.get(key).cloned())
	}

	fn set_item(&mut self, key: &str, value: &str) -> Result<(), String> {
		self.item_map.insert(key.to_string(), value.to_string());
		Ok(())
	}

	fn remove_item(&mut self, key: &str) -> Result<(), String> {
		self.item_map.remove(key);
		Ok(())
	}

	fn get_keys(&self) -> Result<Vec<String>, String> {
		Ok(self.item_map.keys().cloned().collect())
	}
}

LocalStorage::set_backend(Some(Box::new(MemoryBackend {
	item_map: BTreeMap::new(),
})));

LocalStorage::set_number("mygame.coins", 50.0);
let coins = LocalStorage::get_number("mygame.coins", 0.0);
let backend_keys = LocalStorage::get_keys();
assert_eq!(coins, 50.0);
assert_eq!(backend_keys, vec!["mygame.coins"]);
assert!(LocalStorage::get_backend().is_some());

LocalStorage::set_backend(None);
assert!(LocalStorage::get_backend().is_none());
```

- `set_backend(None)` 이면 기본 저장소(웹 `window.localStorage`, 데스크탑 파일)로 돌아갑니다.
- 갈아 끼우면 메모리에 대신 둔 값은 버려지고 `is_memory_fallback()` 도 거짓으로 돌아갑니다.
- 갈아 낀 저장소의 `set_item` 이 `Err` 를 돌려주면 경고를 찍고 그 값을 메모리에 대신 둡니다. `get_item` 의 `Err` 는 값이 없는 것으로 봅니다.
- `get_backend()` 는 갈아 낀 저장소를 `Rc<RefCell<Box<dyn LocalStorageBackend>>>` 로 돌려줍니다. 부르는 쪽은 `borrow`, `borrow_mut` 로 빌려 씁니다. 빌린 채로 `LocalStorage` 를 부르거나, 트레이트 함수 안에서 `LocalStorage` 를 다시 부르면 기본 저장소를 씁니다.
- 원본의 `readStorage()` 는 러스트 판에 없습니다.

## PersistedStore 로 상태 묶음 저장하기

설정 화면이나 점수판처럼 여러 값을 한 덩어리로 다루고, 값이 바뀌면 화면을 고쳐야 할 때는 `PersistedStore` 를 씁니다. 상태 객체 하나(`serde_json::Value`)를 키 하나에 JSON 으로 저장하고, 바뀔 때마다 구독자에게 알립니다. 기본은 `LocalStorage` 에 저장합니다.

```rust
use std::cell::Cell;
use std::rc::Rc;
use vanilla::{LocalStorage, PersistedStore};

let storage_directory_path = std::env::temp_dir().join(format!("mygame-settings-{}", std::process::id()));
LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));

let default_settings = serde_json::json!({ "musicVolume": 0.8, "effectVolume": 1, "language": "" });
let mut settings_store = PersistedStore::new("mygame.settings", default_settings, 1);

let notified_count = Rc::new(Cell::new(0));
let notified_count_for_subscriber = notified_count.clone();
let subscription = settings_store.subscribe(move |state| {
	notified_count_for_subscriber.set(notified_count_for_subscriber.get() + 1);
	assert_eq!(state["musicVolume"], 0.5);
});

settings_store.set(serde_json::json!({ "musicVolume": 0.5 }));
let settings = settings_store.get();
assert_eq!(settings["musicVolume"], 0.5);
assert_eq!(settings["language"], "");
assert_eq!(settings_store.get_storage_key(), "mygame.settings");
assert_eq!(notified_count.get(), 1);
assert_eq!(LocalStorage::get_string("mygame.settings", ""), r#"{"version":1,"state":{"musicVolume":0.5,"effectVolume":1,"language":""}}"#);

settings_store.unsubscribe(subscription);
settings_store.reset();
assert_eq!(settings_store.get()["musicVolume"], 0.8);
assert_eq!(notified_count.get(), 1);

let _ = std::fs::remove_dir_all(&storage_directory_path);
```

- 생성은 `PersistedStore::new(storage_key, default_state, version)` 입니다. 러스트에는 기본 인자가 없어 기본 상태와 판 번호를 늘 넘깁니다. 만들 때 바로 저장소에서 읽습니다.
- `get()` 은 지금 상태의 참조(`&Value`)를 돌려줍니다. 고칠 수 없는 참조라서 원본처럼 돌려받은 객체를 고쳐 저장이 빠지는 일이 없습니다. 고칠 때는 `set` 이나 `replace` 를 씁니다.
- `set(partial_state)` 는 지금 상태에 얕게 합친 새 객체를 만들고, 저장하고, 구독자를 부릅니다. 합치기는 원본 `Object.assign` 과 같습니다. `replace(next_state)` 는 상태 전체를 바꿉니다. 객체도 배열도 아닌 값을 넘기면 빈 객체로 칩니다. 배열은 번호 키(`"0"`, `"1"` ...)로 합칩니다.
- `reset()` 은 기본 상태로 `replace` 합니다. 키를 지우는 것이 아니라 기본 상태를 저장합니다.
- `subscribe(subscriber)` 는 `FnMut(&Value)` 를 걸고, 원본의 해지 함수 대신 구독 번호(`PersistedStoreSubscription`)를 돌려줍니다. `unsubscribe(subscription)` 에 넘기면 풀립니다. 구독자는 씬을 빌릴 수 없으므로 씬과 나눌 값은 `Rc<Cell>` 이나 `Rc<RefCell>` 에 담습니다. `notify()` 를 직접 불러 구독자를 부를 수도 있습니다.
- `set`, `replace`, `reset` 은 부를 때마다 바로 저장합니다. 데스크탑은 그때마다 파일 전체를 씁니다. 매 프레임 바뀌는 값은 넣지 않습니다.

저장소에는 판 번호와 상태가 함께 들어갑니다. 꼴이 원본과 같아서, 웹에서는 같은 출처의 원본 vanilla.js 게임이 저장한 값을 그대로 읽습니다.

```json
{"version":1,"state":{"musicVolume":0.5,"effectVolume":1,"language":""}}
```

## 판 번호와 기본값

읽을 때 저장된 `version` 이 생성할 때의 판 번호와 다르거나, JSON 해석이 실패하거나, `state` 가 객체나 배열이 아니면 기본 상태로 시작합니다. 판 번호가 같으면 기본 상태 위에 저장된 상태를 덮어 씁니다. 그래서 기본 상태에 새 키를 더하면 예전 저장에도 그 키가 기본값으로 채워지고, 기본 상태에 없는 키가 저장돼 있어도 그대로 살아납니다.

구조를 크게 바꿔 판 번호를 올리면 예전 저장은 버려집니다. `PersistedStore` 에는 옮기는 기능이 없으므로, 옮겨야 하면 만들기 전에 `LocalStorage::get_json` 으로 예전 글을 직접 읽어 둡니다.

```rust
use vanilla::{LocalStorage, PersistedStore};

let storage_directory_path = std::env::temp_dir().join(format!("mygame-progress-{}", std::process::id()));
LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
LocalStorage::set_json("mygame.progress", &serde_json::json!({ "version": 1, "state": { "level": 4, "gold": 90 } }));

let stored_payload = LocalStorage::get_json("mygame.progress", serde_json::Value::Null);
let mut migrated_state = None;
if stored_payload["version"] == 1 {
	migrated_state = Some(serde_json::json!({ "stage": stored_payload["state"]["level"], "gold": stored_payload["state"]["gold"] }));
}

let mut progress_store = PersistedStore::new("mygame.progress", serde_json::json!({ "stage": 1, "gold": 0, "gems": 0 }), 2);
if let Some(migrated_state) = migrated_state {
	progress_store.set(migrated_state);
}
let progress = progress_store.get();
assert_eq!(progress["stage"], 4);
assert_eq!(progress["gold"], 90);
assert_eq!(progress["gems"], 0);

let _ = std::fs::remove_dir_all(&storage_directory_path);
```

## 상태 묶음마다 저장소 따로 두기

`set_storage_backend(Some(backend))` 는 그 `PersistedStore` 하나만 다른 저장소를 쓰게 합니다. 저장소는 `LocalStorage::set_backend` 와 같은 `LocalStorageBackend` 이고, `None` 이면 `LocalStorage` 로 돌아갑니다. 갈아 끼우면 새 저장소에서 상태를 다시 읽고(없으면 기본 상태) 구독자를 부릅니다. 갈아 끼운 저장소에 쓰다 실패하면 경고만 찍습니다. `get_storage_backend()` 는 그 저장소를 `Rc<RefCell<...>>` 로 돌려줍니다.

```rust
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use vanilla::{LocalStorageBackend, PersistedStore};

struct CloudBackend {
	item_map: Rc<RefCell<BTreeMap<String, String>>>,
}

impl LocalStorageBackend for CloudBackend {
	fn get_item(&self, key: &str) -> Result<Option<String>, String> {
		Ok(self.item_map.borrow().get(key).cloned())
	}

	fn set_item(&mut self, key: &str, value: &str) -> Result<(), String> {
		self.item_map.borrow_mut().insert(key.to_string(), value.to_string());
		Ok(())
	}

	fn remove_item(&mut self, key: &str) -> Result<(), String> {
		self.item_map.borrow_mut().remove(key);
		Ok(())
	}
}

let cloud_item_map = Rc::new(RefCell::new(BTreeMap::new()));
let mut ranking_store = PersistedStore::new("mygame.ranking", serde_json::json!({ "scores": [] }), 1);
ranking_store.set_storage_backend(Some(Box::new(CloudBackend {
	item_map: cloud_item_map.clone(),
})));
ranking_store.set(serde_json::json!({ "scores": [1200, 900, 450] }));
assert_eq!(cloud_item_map.borrow()["mygame.ranking"], r#"{"version":1,"state":{"scores":[1200,900,450]}}"#);
assert!(ranking_store.get_storage_backend().is_some());
```

## 지우기

- 값 하나: `LocalStorage::remove(key)`.
- 게임의 값 모두: `LocalStorage::clear_by_prefix("mygame.")`.
- `PersistedStore` 를 처음 상태로: `reset()`. 저장소의 키까지 없애려면 `LocalStorage::remove(store.get_storage_key())` 를 부릅니다. 이때 그 `PersistedStore` 가 들고 있는 상태는 그대로이고, 다음 `set` 에서 다시 저장됩니다.

개발 중에는 개발자 도구(F2)의 Local Storage 탭에서 키와 값을 보고, 더하고, 고치고, 지울 수 있습니다. "개발자 도구" 장에서 다룹니다.

## 원본 판과 주고받는 글

- 웹의 키와 값, 숫자 글(`String(number)` 꼴), `PersistedStore` 의 저장 꼴이 원본과 같습니다. 원본 게임을 러스트 판으로 옮겨도 사용자의 저장이 그대로 이어집니다.
- 원본의 저장소는 JS 글(UTF-16)이라 짝 없는 서로게이트(잘린 이모지 같은 것)가 든 값을 담습니다. 러스트 `String` 은 그런 글을 담지 못해 러스트 쪽에는 대체 문자(U+FFFD)로 보입니다. 읽은 값을 고치지 않고 다시 쓰면(`set_item`, `set_string`, `set_json`, `PersistedStore` 의 쓰기) 원래 UTF-16 단위 그대로 씁니다.
- 원래 단위가 필요하면 `get_item_code_units(key)`, `set_item_code_units(key, code_units)` 로 JS 글 그대로(`Vec<u16>`) 읽고 씁니다.

## 주의할 점

- 저장소는 웹은 출처마다, 데스크탑은 앱 이름마다 하나입니다. 키에 접두사를 꼭 붙입니다.
- 데스크탑의 앱 이름은 처음에 실행 파일 이름입니다. 실행 파일 이름이 바뀌면 다른 폴더를 읽으므로, 배포하는 게임은 첫 저장 전에 `set_application_name` 이나 `set_application_name_from_engine` 을 부릅니다.
- 값은 모두 글로 저장됩니다. 숫자와 참거짓은 읽을 때 바꾸므로, 다른 곳에서 글로 써 넣은 값은 `get_number` 가 `NaN` 을, `get_boolean` 이 거짓을 돌려줄 수 있습니다.
- 저장소를 쓰지 못해 메모리에 대신 둔 값은 다시 켜면 없습니다. (`is_memory_fallback`)
- `LocalStorage` 의 상태(갈아 낀 저장소, 메모리에 대신 둔 값, 데스크탑의 읽어 둔 내용)는 스레드마다 따로입니다. 엔진이 도는 스레드에서 씁니다.
- `PersistedStore` 의 판 번호를 올리면 예전 저장은 기본 상태로 바뀝니다. 옮길 것이 있으면 만들기 전에 읽어 둡니다.

## 관련 타입

LocalStorage, LocalStorageBackend, PersistedStore, PersistedStoreSubscription, DevTools
