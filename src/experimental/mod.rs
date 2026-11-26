//==============================================================================
//! 실험 기능. (0.4.0 뒤 기능의 초안, API 가 바뀔 수 있음, 원본 src/experimental/experimental.js)
//! - 다음 판에 넣을 기능의 초안을 모은다. 원본 import.js 의 Experimental 이름 공간 자리다. (vanilla::experimental::이름)
//! - 초안 하나는 src/experimental/<이름>/ 모듈로 두고, 여기에 `pub mod <이름>;` 한 줄을 더한다.
//! - 다 자란 기능은 제자리(base, core, misc, graphics3d 같은)로 옮기고 lib.rs 에서 바로 내보낸다.
//==============================================================================
pub mod tilemap;
pub mod spatialaudio;
pub mod savegame;
pub mod skeleton2d;
pub mod physics2d;
pub mod spatial;
pub(crate) mod jsmath;
