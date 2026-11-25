//==============================================================================
// 플랫폼 층.
// - 창(또는 캔버스), GL 컨텍스트, 입력 이벤트, 프레임 루프, 화면 넘기기를 맡는다.
// - 엔진 본체(core::engine::EngineRunner)는 플랫폼을 모른다. 플랫폼마다 아래 것들을 같은 이름으로 낸다.
//   run(엔진 실행자), set_cursor_visible, set_cursor(커서 모양), is_coarse_pointer(주된 손이 손가락인지), set_title, request_asset_bytes(애셋 바이트 읽기), PlatformWindow(엔진이 들고 있는 창 손잡이),
//   write_clipboard_text / read_clipboard_text(시스템 클립보드)
//   read_page_query_value(페이지 주소의 쿼리 값), read_safe_area_insets(CSS env 세이프 에어리어 인셋)
//   read_canvas_client_position(화면 좌표에서 캔버스의 왼쪽 위), read_client_native_size(페이지 전체 크기, 데스크탑은 None)
//   save_capture_file(검증용 화면 저장, 데스크탑은 파일, 웹은 내려받기)
// - 웹만 is_gl_binding_present(GL 의 묶인 것이 있는지, glow 웹이 null 을 읽으면 패닉해서)를 낸다. (core::graphic 의 read_*_binding 이 쓴다)
// - 맥만 vibrate_gamepads / release_disconnected_gamepad_haptics(게임패드 진동, desktop/gamepadhaptics.rs)를 낸다.
// - 안드로이드, iOS 는 여기에 모듈을 더한다. (아직 없음)
//==============================================================================
// 오디오 백엔드. (데스크탑 kira, 웹 WebAudio. 두 갈래를 안에서 고른다)
pub mod audio;

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{is_coarse_pointer, read_clipboard_text, request_asset_bytes, run, set_cursor, set_cursor_visible, set_title, write_clipboard_text, PlatformWindow};
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{read_canvas_client_position, read_client_native_size, read_page_query_value, read_safe_area_insets, save_capture_file};

#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub use web::{is_coarse_pointer, read_clipboard_text, request_asset_bytes, run, set_cursor, set_cursor_visible, set_title, write_clipboard_text, PlatformWindow};
#[cfg(target_arch = "wasm32")]
pub use web::{read_canvas_client_position, read_client_native_size, read_page_query_value, read_safe_area_insets, save_capture_file};
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::is_text_field_focused;
#[cfg(target_arch = "wasm32")]
pub use web::is_text_field_focused;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::is_gl_binding_present;
#[cfg(target_os = "macos")]
pub use desktop::gamepadhaptics::{release_disconnected_gamepad_haptics, vibrate_gamepads};
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::systemfont::{find_fallback_font_file, find_family_font_file};
