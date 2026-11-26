# Storage

This chapter covers how to keep values that must survive a restart, such as settings, high scores, and progress. It walks through `LocalStorage`, which reads and writes one value per key, and `PersistedStore`, which keeps one state object as JSON under a single key and notifies you when it changes.

Where values end up depends on the platform. On the web (wasm32) it is the browser's `window.localStorage`, as in the original, and on the desktop it is a single `localstorage.json` file in the app folder. Game code is the same either way.

## Saving single values with LocalStorage

Every function of `LocalStorage` is an associated function. Call them directly, as in `LocalStorage::set_string(...)`, without creating a value. Every value is stored as a string.

The example below first points storage at a temporary folder so that running it as a test does not touch the real save file. In a game, leave that line out or set the app name as described in "Where values are stored" below.

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

| Write | Read | Stored string |
| --- | --- | --- |
| `set_string(key, string_value)` | `get_string(key, default_string_value)` | As is |
| `set_number(key, number_value)` | `get_number(key, default_number_value)` | Same form as the original `String(number)` (`1200`, `1.5`, `1e+21`) |
| `set_boolean(key, boolean_value)` | `get_boolean(key, default_boolean_value)` | `"true"` or `"false"` |
| `set_json(key, &value)` | `get_json(key, default_value)` | Compact JSON text of a `serde_json::Value` |

- Rust has no default arguments, so you always pass the default value to the read functions. Numbers are `f64`, like JS numbers in the original, and JSON values are `serde_json::Value`.
- The read functions only return the default when a key is missing. They never write it into storage.
- `get_number` reads like the original `Number(text)`. It trims surrounding whitespace, reads `0x`, `0o`, `0b` forms and `Infinity`, and returns `NaN` for text that is not a number. A missing key or empty text gives the default.
- `get_boolean` is true only when the stored text is `"true"`. Any other non-empty text is false. A missing key or empty text gives the default.
- `get_json` returns the default when the text is missing, empty, or fails to parse.
- `get_item(key)`, `set_item(key, value)`, and `remove_item(key)` are lower-level functions that handle raw text. `get_item` returns `None` when the key is missing.
- `set_item` and `set_json` return `true` when they wrote to storage and `false` when they could not and kept the value in memory instead. `set_string`, `set_number`, and `set_boolean` return nothing.

## Where values are stored

On the web, keys go into `window.localStorage` as is, as in the original. There is one storage per origin (protocol, host, and port).

On the desktop, every key lives in one `localstorage.json` file in the app folder. The file is read on first use, and the whole file is written right away whenever a value changes. (It writes to a temporary file and then swaps it in, so the file is not corrupted if the app quits midway.)

| OS | File |
| --- | --- |
| Windows | `%APPDATA%\{app name}\localstorage.json` |
| macOS | `$HOME/Library/Application Support/{app name}/localstorage.json` |
| Linux | `$XDG_DATA_HOME/{app name}/localstorage.json` (or `$HOME/.local/share/{app name}/localstorage.json` when unset) |

- If the environment variables cannot be read, the file goes under the temporary folder (`std::env::temp_dir()`).
- The app name starts as the executable name. Change it with `set_application_name(name)`, or use `EngineConfiguration.title` with `set_application_name_from_engine(&engine)`. Characters not allowed in folder names (`<>:"/\|?*` and control characters) become `_`, trailing dots and spaces are trimmed, and an empty name becomes `vanilla`.
- `set_storage_directory(Some(path))` picks the folder directly (for tests and portable games). `None` goes back to the folder chosen by the app name.
- Changing the app name or the folder drops the loaded contents, and the next access reads from the new place. Set them before the first save or read.
- `get_storage_file_path()` returns the file in use. It is `None` on the web.
- On the web, `set_application_name` and `set_storage_directory` do nothing.

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

## Listing and removing keys

Several games can share the same storage (pages of the same origin on the web, the same app name on the desktop). Prefix your keys with something like the game name so they do not mix with other apps' values and you can remove only your game's values.

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

- `get_keys()` merges the storage keys with keys kept in memory (see the next section) and returns them sorted by name.
- `remove(key)` removes one key. It is the same as `remove_item`.
- `clear_by_prefix(prefix)` removes every key starting with the prefix and returns how many it removed. An empty prefix removes nothing and returns 0.
- `clear()` empties the whole storage. On the web this includes other apps' values from the same origin, on the desktop every key in that app folder's file, and in both cases the developer tools settings (the `hierarchy.settings` key). To remove only your game's values, use `clear_by_prefix`.

## When storage is unavailable

On the web, `window.localStorage` cannot be used in private windows or browsers that block cookies, and writes fail when storage is full. On the desktop, the folder may not be creatable or the file write may fail. In these cases `LocalStorage` keeps going and holds the values it could not write in memory for as long as this run lasts.

- `set_item` returns `true` when it wrote to storage and `false` when it kept the value in memory instead. `set_json` does the same.
- Once any value has fallen back to memory, `is_memory_fallback()` is true.
- Values kept in memory are gone after a restart. Games where saving matters can check `is_memory_fallback()` and show a notice such as "Progress will not be saved".
- When a value is held in memory, reads return it ahead of the value in storage.

## Replacing the storage

Where a platform SDK provides its own storage, swap it in with `LocalStorage::set_backend(Some(backend))`. Where the original takes an object with `getItem`, `setItem`, and `removeItem`, the Rust version takes the `LocalStorageBackend` trait. Errors the original reports by throwing are reported as `Err`.

| Function | What it does |
| --- | --- |
| `get_item(&self, key)` | Reads a value. `Ok(None)` when missing |
| `set_item(&mut self, key, value)` | Writes a value |
| `remove_item(&mut self, key)` | Removes a value |
| `get_keys(&self)` | All keys. The default implementation returns an empty list, so without it `get_keys` and `clear_by_prefix` only see keys kept in memory |
| `clear(&mut self)` | Removes everything. The default implementation removes the keys from `get_keys` one by one |

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

- `set_backend(None)` goes back to the default storage (`window.localStorage` on the web, the file on the desktop).
- Swapping drops the values kept in memory and resets `is_memory_fallback()` to false.
- If the swapped-in storage's `set_item` returns `Err`, a warning is printed and the value is kept in memory. An `Err` from `get_item` is treated as a missing value.
- `get_backend()` returns the swapped-in storage as `Rc<RefCell<Box<dyn LocalStorageBackend>>>`. The caller borrows it with `borrow` or `borrow_mut`. Calling `LocalStorage` while it is borrowed, or from inside the trait functions, uses the default storage.
- The original `readStorage()` does not exist in the Rust version.

## Saving a state bundle with PersistedStore

When you handle several values as one bundle, as on a settings screen or a scoreboard, and need to refresh the screen when they change, use `PersistedStore`. It saves one state object (`serde_json::Value`) as JSON under a single key and notifies subscribers on every change. By default it saves to `LocalStorage`.

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

- Create one with `PersistedStore::new(storage_key, default_state, version)`. Rust has no default arguments, so you always pass the default state and the version. It reads from storage right away.
- `get()` returns a reference to the current state (`&Value`). The reference is immutable, so unlike the original you cannot edit the returned object and lose the save by accident. Use `set` or `replace` to change it.
- `set(partial_state)` builds a new object shallow-merged into the current state, saves it, and calls the subscribers. Merging works like the original `Object.assign`. `replace(next_state)` replaces the whole state. A value that is neither an object nor an array counts as an empty object. Arrays merge by index keys (`"0"`, `"1"`, ...).
- `reset()` calls `replace` with the default state. It does not remove the key. It saves the default state.
- `subscribe(subscriber)` registers an `FnMut(&Value)` and returns a subscription number (`PersistedStoreSubscription`) instead of the original's unsubscribe function. Pass it to `unsubscribe(subscription)` to release it. Subscribers cannot borrow the scene, so put values shared with the scene in `Rc<Cell>` or `Rc<RefCell>`. You can also call `notify()` yourself to call the subscribers.
- `set`, `replace`, and `reset` save immediately on every call. On the desktop that writes the whole file each time. Do not put values that change every frame in a store.

Storage holds the version number together with the state. The format is the same as the original, so on the web it reads values saved by an original vanilla.js game on the same origin as is.

```json
{"version":1,"state":{"musicVolume":0.5,"effectVolume":1,"language":""}}
```

## Version numbers and defaults

On read, if the stored `version` differs from the version given at creation, the JSON fails to parse, or `state` is neither an object nor an array, the store starts from the default state. If the versions match, the stored state is written over the default state. So when you add a new key to the default state, old saves get that key filled with its default, and keys that are saved but missing from the default state survive as is.

If you change the structure a lot and bump the version, old saves are discarded. `PersistedStore` has no migration feature, so if you need to carry data over, read the old text yourself with `LocalStorage::get_json` before creating the store.

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

## A separate storage per state bundle

`set_storage_backend(Some(backend))` makes only that one `PersistedStore` use a different storage. The storage is the same `LocalStorageBackend` as in `LocalStorage::set_backend`, and `None` goes back to `LocalStorage`. After swapping, the store re-reads its state from the new storage (or uses the default state if there is none) and calls the subscribers. If writing to the swapped-in storage fails, it only prints a warning. `get_storage_backend()` returns that storage as `Rc<RefCell<...>>`.

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

## Removing data

- One value: `LocalStorage::remove(key)`.
- All of the game's values: `LocalStorage::clear_by_prefix("mygame.")`.
- A `PersistedStore` back to its initial state: `reset()`. To remove the key from storage as well, call `LocalStorage::remove(store.get_storage_key())`. The state that `PersistedStore` holds stays as is, and the next `set` saves it again.

During development you can view, add, edit, and remove keys and values in the Local Storage tab of the developer tools (F2). The "Developer Tools" chapter covers it.

## Text shared with the original version

- Keys and values on the web, the number text (the `String(number)` form), and the `PersistedStore` format are the same as the original. When you port an original game to the Rust version, players' saves carry over as is.
- The original storage holds JS strings (UTF-16), so it can store values that contain lone surrogates (such as a cut-off emoji). A Rust `String` cannot hold such text, so on the Rust side it appears with the replacement character (U+FFFD). If you write back a value you read without changing it (`set_item`, `set_string`, `set_json`, or a `PersistedStore` write), the original UTF-16 code units are written as they were.
- When you need the original code units, read and write the JS string as is (`Vec<u16>`) with `get_item_code_units(key)` and `set_item_code_units(key, code_units)`.

## Things to watch for

- There is one storage per origin on the web and per app name on the desktop. Always prefix your keys.
- On the desktop the app name starts as the executable name. If the executable name changes, a different folder is read, so shipped games call `set_application_name` or `set_application_name_from_engine` before the first save.
- Every value is stored as a string. Numbers and booleans are converted when read, so values written as text elsewhere can make `get_number` return `NaN` and `get_boolean` return false.
- Values kept in memory because storage was unavailable are gone after a restart (`is_memory_fallback`).
- `LocalStorage` state (the swapped-in storage, values kept in memory, and the loaded desktop contents) is per thread. Use it from the thread the engine runs on.
- Bumping the version of a `PersistedStore` turns old saves into the default state. Read anything you need to carry over before creating the store.

## Related types

LocalStorage, LocalStorageBackend, PersistedStore, PersistedStoreSubscription, DevTools
