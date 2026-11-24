//==============================================================================
// 플랫폼 층.
// - 창(또는 캔버스), GL 컨텍스트, 입력 이벤트, 프레임 루프, 화면 넘기기를 맡는다.
// - 엔진 본체(core::engine::EngineRunner)는 플랫폼을 모른다. 플랫폼마다 아래 것들을 같은 이름으로 낸다.
//   run(엔진 실행자), set_cursor_visible, set_cursor(커서 모양), is_coarse_pointer(주된 손이 손가락인지), set_title, request_asset_bytes(애셋 바이트 읽기), PlatformWindow(엔진이 들고 있는 창 손잡이),
//   write_clipboard_text / read_clipboard_text(시스템 클립보드)
//   read_page_query_value(페이지 주소의 쿼리 값), read_safe_area_insets(CSS env 세이프 에어리어 인셋)
// - 안드로이드, iOS 는 여기에 모듈을 더한다. (아직 없음)
//==============================================================================
// 오디오 백엔드. (데스크탑 kira, 웹 WebAudio. 두 갈래를 안에서 고른다)
pub mod audio;

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{is_coarse_pointer, read_clipboard_text, request_asset_bytes, run, set_cursor, set_cursor_visible, set_title, write_clipboard_text, PlatformWindow};
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{read_page_query_value, read_safe_area_insets};

#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub use web::{is_coarse_pointer, read_clipboard_text, request_asset_bytes, run, set_cursor, set_cursor_visible, set_title, write_clipboard_text, PlatformWindow};
#[cfg(target_arch = "wasm32")]
pub use web::{read_page_query_value, read_safe_area_insets};
