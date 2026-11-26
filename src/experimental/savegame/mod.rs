//==============================================================================
//! 세이브 슬롯. (원본 src/experimental/savegame/)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - SaveGameManager: 여러 칸 저장(칸마다 이름, 저장 시각, 판, 미리보기 그림 자리, 크기, metadata), 판 옮기기(판마다 함수 사슬),
//!   원자적 쓰기(두 키 번갈아 쓰기와 순번), 깨진 기록 알아보기(CRC-32), 자동 저장, 내보내기와 들여오기(JSON), 클라우드 자리(SaveCloudProvider, 인터페이스만).
//! - 저장소는 엔진 LocalStorage 위의 LocalSaveStorage 가 기본이다. (웹 localStorage, 데스크탑 localstorage.json, 원본과 같은 키와 기록 꼴)
//!   SaveStorage 를 구현해 바꿔 낄 수 있다. (MemorySaveStorage 는 메모리만)
//! - 유니티는 엔진에 칸 체계가 없고(PlayerPrefs 의 키 값), 언리얼은 USaveGame 과 UGameplayStatics::SaveGameToSlot(칸 이름, 사용자 번호)이다.
//!   판 옮기기, 두 벌 쓰기, 체크섬, 자동 저장, 내보내기는 두 엔진 모두 게임이 짓는 몫이라 여기서 엔진이 맡는다.
//! # 예제
//! ```
//! use vanilla::experimental::savegame::{SaveGameManager, SaveSlotOptions};
//! use serde_json::json;
//!
//! let mut save_game_manager = SaveGameManager::new("example.savegame", 1);
//! let options = SaveSlotOptions { name: "Chapter 1".to_string(), metadata: json!({ "playSeconds": 600 }), ..SaveSlotOptions::default() };
//! save_game_manager.save("slot1", &json!({ "hp": 30, "position": [3, 4] }), &options).expect("save");
//! let result = save_game_manager.load("slot1").expect("load");
//! assert_eq!(result.data["hp"], 30);
//! assert_eq!(result.info.metadata["playSeconds"], 600);
//! save_game_manager.delete("slot1");
//! ```
//==============================================================================
mod savegamemanager;
mod savestorage;

pub use savegamemanager::{calculate_crc32, SaveGameError, SaveGameLoadResult, SaveGameManager, SaveSlotInfo, SaveSlotOptions};
pub use savestorage::{LocalSaveStorage, MemorySaveStorage, SaveCloudProvider, SaveStorage};
