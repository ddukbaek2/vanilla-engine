//==============================================================================
//! 세이브 저장소와 클라우드 자리. (원본 src/experimental/savegame/savestorage.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - SaveStorage: 세이브 기록을 글로 두는 곳. 기본은 엔진 LocalStorage 위의 LocalSaveStorage 다.
//!   (웹 localStorage, 데스크탑 앱 폴더의 localstorage.json. 데스크탑 파일은 LocalStorage 가 임시 파일에 쓴 뒤 이름을 바꿔 끼운다)
//! - MemorySaveStorage: 메모리에만 두는 저장소. (시험, 저장하지 않는 체험판)
//! - SaveCloudProvider: 클라우드 저장(스팀 클라우드 같은)을 꽂을 자리. 엔진은 인터페이스만 두고, 게임이나 플랫폼 묶음이 구현한다.
//==============================================================================
use crate::misc::localstorage::LocalStorage;
use std::collections::BTreeMap;

//==============================================================================
/// 세이브 기록을 글로 두는 곳.
/// - 한 키에 쓰기는 다 되거나 안 되거나여야 한다. (localStorage.setItem, 임시 파일 뒤 이름 바꾸기) SaveGameManager 는 그 위에서 두 키를 번갈아 쓴다.
/// - 기본 구현은 아무것도 두지 않는다. (원본 SaveStorage 의 기본 메서드와 같다) 구현하는 쪽이 필요한 함수만 다시 정의한다.
/// # 예제
/// ```
/// use vanilla::experimental::savegame::SaveStorage;
///
/// struct PrintSaveStorage;
///
/// impl SaveStorage for PrintSaveStorage {
/// 	fn write_text(&mut self, key: &str, text: &str) -> bool {
/// 		println!("{} {}", key, text.len());
/// 		true
/// 	}
/// }
///
/// let mut storage = PrintSaveStorage;
/// assert!(storage.write_text("save.slot.a.0", "text"));
/// assert_eq!(storage.read_text("save.slot.a.0"), None);
/// ```
//==============================================================================
pub trait SaveStorage {
	//==============================================================================
	/// 읽기.
	///
	/// # 인자
	/// - `key`: 키.
	///
	/// # 돌려줌
	/// 저장된 글. (없거나 못 읽으면 None)
	//==============================================================================
	fn read_text(&self, key: &str) -> Option<String> {
		let _ = key;
		None
	}

	//==============================================================================
	/// 쓰기.
	///
	/// # 인자
	/// - `key`: 키.
	/// - `text`: 쓸 글.
	///
	/// # 돌려줌
	/// 오래 남는 곳에 썼으면 true.
	//==============================================================================
	fn write_text(&mut self, key: &str, text: &str) -> bool {
		let _ = (key, text);
		false
	}

	//==============================================================================
	/// 지우기.
	///
	/// # 인자
	/// - `key`: 지울 키.
	//==============================================================================
	fn remove_text(&mut self, key: &str) {
		let _ = key;
	}

	//==============================================================================
	/// 앞머리가 같은 키를 모두 읽기.
	///
	/// # 인자
	/// - `prefix`: 앞머리.
	///
	/// # 돌려줌
	/// 정렬한 키 목록.
	//==============================================================================
	fn list_keys(&self, prefix: &str) -> Vec<String> {
		let _ = prefix;
		Vec::new()
	}
}

//==============================================================================
/// 엔진 LocalStorage 위의 저장소. (기본)
/// # 예제
/// ```
/// use vanilla::experimental::savegame::{LocalSaveStorage, SaveStorage};
///
/// let mut storage = LocalSaveStorage::new();
/// storage.write_text("example.savestorage.key", "value");
/// assert_eq!(storage.read_text("example.savestorage.key").as_deref(), Some("value"));
/// storage.remove_text("example.savestorage.key");
/// ```
//==============================================================================
#[derive(Default)]
pub struct LocalSaveStorage;

impl LocalSaveStorage {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 저장소.
	//==============================================================================
	pub fn new() -> LocalSaveStorage {
		LocalSaveStorage
	}
}

impl SaveStorage for LocalSaveStorage {
	//==============================================================================
	/// 읽기.
	///
	/// # 인자
	/// - `key`: 키.
	///
	/// # 돌려줌
	/// 저장된 글. (없으면 None)
	//==============================================================================
	fn read_text(&self, key: &str) -> Option<String> {
		LocalStorage::get_item(key)
	}

	//==============================================================================
	/// 쓰기.
	///
	/// # 인자
	/// - `key`: 키.
	/// - `text`: 쓸 글.
	///
	/// # 돌려줌
	/// 오래 남는 곳에 썼으면 true.
	//==============================================================================
	fn write_text(&mut self, key: &str, text: &str) -> bool {
		LocalStorage::set_item(key, text)
	}

	//==============================================================================
	/// 지우기.
	///
	/// # 인자
	/// - `key`: 지울 키.
	//==============================================================================
	fn remove_text(&mut self, key: &str) {
		LocalStorage::remove_item(key);
	}

	//==============================================================================
	/// 앞머리가 같은 키를 모두 읽기.
	///
	/// # 인자
	/// - `prefix`: 앞머리.
	///
	/// # 돌려줌
	/// 정렬한 키 목록.
	//==============================================================================
	fn list_keys(&self, prefix: &str) -> Vec<String> {
		LocalStorage::get_keys().into_iter().filter(|key| key.starts_with(prefix)).collect()
	}
}

//==============================================================================
/// 메모리에만 두는 저장소. (시험, 끄면 없어진다)
/// # 예제
/// ```
/// use vanilla::experimental::savegame::{MemorySaveStorage, SaveStorage};
///
/// let mut storage = MemorySaveStorage::new();
/// storage.write_text("save.slot.a.0", "text");
/// assert_eq!(storage.list_keys("save.slot."), vec!["save.slot.a.0".to_string()]);
/// ```
//==============================================================================
#[derive(Default, Clone)]
pub struct MemorySaveStorage {
	text_by_key: BTreeMap<String, String>,
}

impl MemorySaveStorage {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 저장소.
	//==============================================================================
	pub fn new() -> MemorySaveStorage {
		MemorySaveStorage { text_by_key: BTreeMap::new() }
	}
}

impl SaveStorage for MemorySaveStorage {
	//==============================================================================
	/// 읽기.
	///
	/// # 인자
	/// - `key`: 키.
	///
	/// # 돌려줌
	/// 저장된 글. (없으면 None)
	//==============================================================================
	fn read_text(&self, key: &str) -> Option<String> {
		self.text_by_key.get(key).cloned()
	}

	//==============================================================================
	/// 쓰기.
	///
	/// # 인자
	/// - `key`: 키.
	/// - `text`: 쓸 글.
	///
	/// # 돌려줌
	/// 늘 true.
	//==============================================================================
	fn write_text(&mut self, key: &str, text: &str) -> bool {
		self.text_by_key.insert(key.to_string(), text.to_string());
		true
	}

	//==============================================================================
	/// 지우기.
	///
	/// # 인자
	/// - `key`: 지울 키.
	//==============================================================================
	fn remove_text(&mut self, key: &str) {
		self.text_by_key.remove(key);
	}

	//==============================================================================
	/// 앞머리가 같은 키를 모두 읽기.
	///
	/// # 인자
	/// - `prefix`: 앞머리.
	///
	/// # 돌려줌
	/// 정렬한 키 목록.
	//==============================================================================
	fn list_keys(&self, prefix: &str) -> Vec<String> {
		self.text_by_key.keys().filter(|key| key.starts_with(prefix)).cloned().collect()
	}
}

//==============================================================================
/// 클라우드 저장을 꽂을 자리. (인터페이스만, 스팀 클라우드 같은 것을 게임이나 플랫폼 묶음이 구현한다)
/// - 주고받는 글은 SaveGameManager::export_slot 의 JSON 이다. (체크섬이 들어 있어 받은 쪽이 깨졌는지 본다)
/// - 기본 구현은 아무것도 하지 않는다. (원본 SaveCloudProvider 의 기본 메서드와 같다, 올리기 false, 받기 None, 목록은 빈 것)
/// # 예제
/// ```
/// use vanilla::experimental::savegame::SaveCloudProvider;
/// use std::collections::HashMap;
///
/// struct MemoryCloud {
/// 	text_by_slot: HashMap<String, String>,
/// }
///
/// impl SaveCloudProvider for MemoryCloud {
/// 	fn upload(&mut self, slot_id: &str, exported_text: &str) -> bool {
/// 		self.text_by_slot.insert(slot_id.to_string(), exported_text.to_string());
/// 		true
/// 	}
/// 	fn download(&mut self, slot_id: &str) -> Option<String> {
/// 		self.text_by_slot.get(slot_id).cloned()
/// 	}
/// 	fn remove(&mut self, slot_id: &str) {
/// 		self.text_by_slot.remove(slot_id);
/// 	}
/// 	fn list_slot_ids(&mut self) -> Vec<String> {
/// 		self.text_by_slot.keys().cloned().collect()
/// 	}
/// }
///
/// let mut cloud = MemoryCloud { text_by_slot: HashMap::new() };
/// assert!(cloud.upload("slot1", "{}"));
/// assert_eq!(cloud.list_slot_ids(), vec!["slot1".to_string()]);
/// ```
//==============================================================================
pub trait SaveCloudProvider {
	//==============================================================================
	/// 올리기. (저장이 끝날 때마다 부른다)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	/// - `exported_text`: export_slot 의 JSON.
	///
	/// # 돌려줌
	/// 올렸으면 true.
	//==============================================================================
	fn upload(&mut self, slot_id: &str, exported_text: &str) -> bool {
		let _ = (slot_id, exported_text);
		false
	}

	//==============================================================================
	/// 내려받기.
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	///
	/// # 돌려줌
	/// export_slot 의 JSON. (없으면 None)
	//==============================================================================
	fn download(&mut self, slot_id: &str) -> Option<String> {
		let _ = slot_id;
		None
	}

	//==============================================================================
	/// 지우기. (칸을 지울 때 부른다)
	///
	/// # 인자
	/// - `slot_id`: 칸 이름.
	//==============================================================================
	fn remove(&mut self, slot_id: &str) {
		let _ = slot_id;
	}

	//==============================================================================
	/// 클라우드에 있는 칸 이름을 모두 읽기.
	///
	/// # 돌려줌
	/// 칸 이름 목록.
	//==============================================================================
	fn list_slot_ids(&mut self) -> Vec<String> {
		Vec::new()
	}
}
