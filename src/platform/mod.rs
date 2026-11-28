//==============================================================================
//! 플랫폼 층.
//! - 창(또는 캔버스), GL 컨텍스트, 입력 이벤트, 프레임 루프, 화면 넘기기를 맡는다.
//! - 엔진 본체(core::engine::EngineRunner)는 플랫폼을 모른다. 플랫폼마다 아래 것들을 같은 이름으로 낸다.
//!   run(엔진 실행자), set_cursor_visible, set_cursor(커서 모양), is_coarse_pointer(주된 손이 손가락인지), set_title, request_asset_bytes(애셋 바이트 읽기), PlatformWindow(엔진이 들고 있는 창 손잡이),
//!   write_clipboard_text / read_clipboard_text(시스템 클립보드)
//!   read_page_query_value(페이지 주소의 쿼리 값), read_safe_area_insets(CSS env 세이프 에어리어 인셋)
//!   read_canvas_client_position(화면 좌표에서 캔버스의 왼쪽 위), read_client_native_size(페이지 전체 크기, 데스크탑은 None)
//!   save_capture_file(검증용 화면 저장, 데스크탑은 파일, 웹은 내려받기)
//! - 웹만 is_gl_binding_present(GL 의 묶인 것이 있는지, glow 웹이 null 을 읽으면 패닉해서)를 낸다. (core::graphic 의 read_*_binding 이 쓴다)
//! - 맥만 vibrate_gamepads / release_disconnected_gamepad_haptics(게임패드 진동, desktop/gamepadhaptics.rs)를 낸다.
//! - 데스크탑만 desktop::read_heap_usage(시스템 할당자의 힙 사용량, 개발자 도구의 메모리 칸)를 낸다. (웹은 원본처럼 performance.memory 를 읽는다)
//! - 안드로이드, iOS 는 여기에 모듈을 더한다. (아직 없음)
//==============================================================================
// 오디오 백엔드. (데스크탑 cpal 위의 엔진 믹서, 웹 WebAudio. 두 갈래를 안에서 고른다)
pub mod audio;
// 문서. (html lang 과 head 의 meta, 웹은 브라우저 문서이고 데스크탑은 엔진이 든 값)
pub mod document;

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{is_coarse_pointer, read_clipboard_text, request_asset_bytes, run, set_cursor, set_cursor_visible, set_title, write_clipboard_text, PlatformWindow};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use desktop::prepare_graphic;
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
#[cfg(target_arch = "wasm32")]
pub(crate) use web::is_default_framebuffer_antialiased;
#[cfg(target_os = "macos")]
pub use desktop::gamepadhaptics::{release_disconnected_gamepad_haptics, vibrate_gamepads};
// 최대 FPS 제한의 프레임 세기. (웹이 쓴다, 원본 engine.js 의 프레임 세기)
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) mod framelimiter;
// 다시 들어와도 안전한 사건 보내기. (웹 리스너가 쓴다, 프레임 안에서 온 사건은 쌓았다가 프레임 끝에 처리한다)
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) mod eventqueue;
// 응용 프로그램 종료. (원본 exitApplication, 웹은 Capacitor App.exitApp 이나 window.close, 데스크탑은 이벤트 루프가 끝난다)
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::exit_application;
#[cfg(target_arch = "wasm32")]
pub use web::exit_application;
// 바깥 주소 열기. (Engine::open_url, 웹은 window.open 새 탭, 데스크탑은 운영체제의 기본 프로그램)
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::open_url;
#[cfg(target_arch = "wasm32")]
pub use web::open_url;

// 현지 시각 읽기. (Engine::read_local_time, 웹은 Date, 맥과 리눅스는 localtime_r, 윈도우는 SystemTimeToTzSpecificLocalTime)
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::read_local_time;
#[cfg(target_arch = "wasm32")]
pub use web::read_local_time;

//==============================================================================
/// 현지 시각. (이 기계의 시간대로 본 달력 시각, 원본 Engine.readLocalTime 이 돌려주는 {year, month, day, hour, minute, second, weekday} 와 같다)
///
/// # 예제
/// 지금의 현지 시각 읽기
/// ```rust
/// use vanilla::platform::read_local_time;
///
/// let local_time = read_local_time(0.0);
/// assert!(local_time.year >= 2026);
/// assert!((1..=12).contains(&local_time.month));
/// assert!((1..=31).contains(&local_time.day));
/// assert!(local_time.hour < 24);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
	/// 해. (예: 2026)
	pub year: i32,
	/// 달. (1 에서 12)
	pub month: u32,
	/// 날. (1 에서 31)
	pub day: u32,
	/// 시. (0 에서 23)
	pub hour: u32,
	/// 분. (0 에서 59)
	pub minute: u32,
	/// 초. (0 에서 59)
	pub second: u32,
	/// 요일. (0 이 일요일, 6 이 토요일)
	pub weekday: u32,
}

//==============================================================================
/// 에포크 초를 달력으로. (협정 세계시 기준, 윈도우 현지 시각 바꾸기와 시험이 쓴다. 1970-01-01 이 목요일)
//==============================================================================
#[cfg(any(target_os = "windows", test))]
pub(crate) fn civil_from_epoch_seconds(epoch_seconds: i64) -> LocalTime {
	let days = epoch_seconds.div_euclid(86400);
	let seconds_of_day = epoch_seconds.rem_euclid(86400);
	// Howard Hinnant 의 civil_from_days.
	let shifted_days = days + 719468;
	let era = shifted_days.div_euclid(146097);
	let day_of_era = shifted_days.rem_euclid(146097);
	let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
	let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
	let month_index = (5 * day_of_year + 2) / 153;
	let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
	let month = if month_index < 10 { month_index + 3 } else { month_index - 9 } as u32;
	let year = (year_of_era + era * 400 + if month <= 2 { 1 } else { 0 }) as i32;
	LocalTime { year, month, day, hour: (seconds_of_day / 3600) as u32, minute: ((seconds_of_day / 60) % 60) as u32, second: (seconds_of_day % 60) as u32, weekday: (days + 4).rem_euclid(7) as u32 }
}

//==============================================================================
/// 열 수 있는 주소인지. (http, https, mailto, tel 만, 원본 Engine.openUrl 과 같은 검사. file: 이나 javascript: 같은 꼴이 운영체제 프로그램을 띄우지 못하게 한다)
//==============================================================================
pub(crate) fn is_openable_url(url: &str) -> bool {
	let lower_url = url.to_ascii_lowercase();
	lower_url.starts_with("http://") || lower_url.starts_with("https://") || lower_url.starts_with("mailto:") || lower_url.starts_with("tel:")
}

// 시스템 클립보드 글을 읽어 콜백에 넘기기. (웹은 navigator.clipboard.readText 가 끝난 뒤, 데스크탑은 그 자리에서)
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::read_clipboard_text_with_callback;
#[cfg(target_arch = "wasm32")]
pub use web::read_clipboard_text_with_callback;
// 시스템 클립보드 글을 읽어 콜백에 넘기기. (못 읽거나 거부되면 None, Engine::read_clipboard_text_async)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use desktop::read_optional_clipboard_text_with_callback;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::read_optional_clipboard_text_with_callback;
// 웹 클립보드를 비동기로 읽어 그 입력 필드에 붙일 글로 넣기. (UIInputField.paste)
#[cfg(target_arch = "wasm32")]
pub(crate) use web::request_clipboard_paste;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::read_renderbuffer_sample_counts;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::systemfont::{find_fallback_font_file, find_family_font_file, list_family_font_faces};

#[cfg(test)]
mod local_time_tests {
	use super::*;

	//==============================================================================
	// 달력 변환이 알려진 시각(1970-01-01 등)과 맞는지 본다.
	//==============================================================================
	#[test]
	fn civil_from_epoch_seconds_matches_known_instants() {
		assert_eq!(civil_from_epoch_seconds(0), LocalTime { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0, weekday: 4 });
		assert_eq!(civil_from_epoch_seconds(1_000_000_000), LocalTime { year: 2001, month: 9, day: 9, hour: 1, minute: 46, second: 40, weekday: 0 });
		assert_eq!(civil_from_epoch_seconds(-1), LocalTime { year: 1969, month: 12, day: 31, hour: 23, minute: 59, second: 59, weekday: 3 });
		assert_eq!(civil_from_epoch_seconds(1_709_164_800), LocalTime { year: 2024, month: 2, day: 29, hour: 0, minute: 0, second: 0, weekday: 4 });
	}
}
