//==============================================================================
// 플랫폼 층.
// - 창(또는 캔버스), GL 컨텍스트, 입력 이벤트, 프레임 루프, 화면 넘기기를 맡는다.
// - 엔진 본체(core::engine::EngineRunner)는 플랫폼을 모른다. 플랫폼마다 아래 것들을 같은 이름으로 낸다.
//   run(엔진 실행자), setCursorVisible, setTitle, requestAssetBytes(애셋 바이트 읽기), PlatformWindow(엔진이 들고 있는 창 손잡이),
//   writeClipboardText / readClipboardText(시스템 클립보드)
// - 안드로이드, iOS 는 여기에 모듈을 더한다. (아직 없음)
//==============================================================================
// 오디오 백엔드. (데스크탑 kira, 웹 WebAudio. 두 갈래를 안에서 고른다)
pub mod audio;

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{readClipboardText, requestAssetBytes, run, setCursorVisible, setTitle, writeClipboardText, PlatformWindow};

#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub use web::{readClipboardText, requestAssetBytes, run, setCursorVisible, setTitle, writeClipboardText, PlatformWindow};
