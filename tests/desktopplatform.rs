//==============================================================================
//! 데스크탑 플랫폼 층 시험. (운영체제가 해 주는 글꼴 찾기, 대체 글꼴, 클립보드를 실제로 부르고 같은 기계의 크롬과 견준다)
//! - 원본에 없는 파일이다. 원본은 브라우저가 하는 일이라 시험이 없다.
//! - 깔린 글꼴, 화면 서버, 같은 기계의 크롬이 있어야 해서 #[ignore] 로 둔다. CI 의 platforms 워크플로(.github/workflows/platforms.yml)가 윈도우, 리눅스(X11), 맥에서 돌린다.
//! - 실행: cargo test --test desktopplatform -- --ignored --nocapture
//! - 크롬 견주기는 같은 기계에서 tests/assets/systemfont/chromemeasure.cjs 가 찍은 JSON 을 읽는다. (VANILLA_CHROME_MEASURE_PATH, 없으면 그 폴더의 chrome_measure.json)
//==============================================================================
#![cfg(not(target_arch = "wasm32"))]

use skrifa::MetadataProvider;
use vanilla::base::platform::SYSTEM_FONT_STRING;
use vanilla::core::textshaping::ShapingDirection;
use vanilla::core::textstringtexturecache::{parse_font_string, FontFileSource, TextStringTextureCache};
use vanilla::platform::desktop::systemfont::{find_last_resort_font_file, read_browser_locale, resolve_generic_family_name, GenericFamily};
use vanilla::platform::{find_fallback_font_file, find_family_font_file, read_clipboard_text, write_clipboard_text};

/// 크롬과 같다고 볼 차이. (픽셀, 맥의 크롬 견주기 시험과 같은 자리 수)
const CHROME_TOLERANCE: f64 = 0.02;

//==============================================================================
/// 원본 SYSTEM_FONT_STRING 사슬과 운영체제 대체 글꼴을 건 글자 캐시. (엔진이 기본 글꼴 경로 없이 뜰 때와 같다)
///
/// # 돌려줌
/// 글자 캐시.
//==============================================================================
fn create_system_text_cache() -> TextStringTextureCache {
	let mut text_string_texture_cache = TextStringTextureCache::new();
	let system_font_description = parse_font_string(&format!("10px {}", SYSTEM_FONT_STRING));
	text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
	text_string_texture_cache.set_system_font_fallback_enabled(true);
	text_string_texture_cache
}

//==============================================================================
/// 글꼴 파일에 글자가 있는지. (얼굴 번호를 모르면 모음의 얼굴을 모두 본다)
///
/// # 인자
/// - `source`: 운영체제가 찾아 준 파일과 얼굴.
/// - `character`: 볼 글자.
///
/// # 돌려줌
/// 그 글자의 글리프가 있으면 true.
//==============================================================================
fn has_character(source: &FontFileSource, character: char) -> bool {
	let font_bytes = std::fs::read(&source.path).unwrap_or_else(|error| panic!("{}: {}", source.path.display(), error));
	let face_index_list: Vec<u32> = match source.face_index {
		Some(face_index) => {
			vec![face_index]
		}
		None => {
			(0..64).collect()
		}
	};
	for face_index in face_index_list {
		match skrifa::FontRef::from_index(&font_bytes, face_index) {
			Ok(font_ref) => {
				if font_ref.charmap().map(character).is_some() {
					return true;
				}
			}
			Err(_) => {
				break;
			}
		}
	}
	false
}

//==============================================================================
/// 일반 이름, 원본 사슬, 마지막 대체 글꼴, 글자마다의 대체 글꼴을 운영체제(맥 CoreText, 윈도우 DirectWrite, 리눅스 fontconfig)가 실제로 찾는다.
/// - 대체 글꼴은 찾은 파일에 그 글자가 있어야 한다. (라틴, 한글, 한자, 아랍어, 태국어, 색 이모지)
//==============================================================================
#[test]
#[ignore = "운영체제 글꼴이 깔린 기계에서 돌린다 (CI platforms 워크플로)"]
fn finds_system_and_fallback_fonts() {
	println!("화면 언어: {}", read_browser_locale());
	for generic_family in [GenericFamily::Standard, GenericFamily::SansSerif, GenericFamily::Serif, GenericFamily::Monospace] {
		let family_name = resolve_generic_family_name(generic_family).unwrap_or_else(|| panic!("{:?} 의 글꼴 이름이 없다", generic_family));
		let source = find_family_font_file(&family_name, 400, false).unwrap_or_else(|| panic!("{:?} ({}) 의 파일을 찾지 못했다", generic_family, family_name));
		assert!(source.path.exists(), "{}", source.path.display());
		println!("일반 이름 {:?}: {} → {} (얼굴 {:?})", generic_family, family_name, source.path.display(), source.face_index);
	}
	let family_name_list = parse_font_string(&format!("10px {}", SYSTEM_FONT_STRING)).family_name_list;
	let mut found_count = 0;
	for family_name in family_name_list.iter() {
		match find_family_font_file(family_name, 400, false) {
			Some(source) => {
				found_count += 1;
				println!("사슬 {}: {} (얼굴 {:?})", family_name, source.path.display(), source.face_index);
			}
			None => {
				println!("사슬 {}: 없음", family_name);
			}
		}
	}
	assert!(found_count > 0, "원본 사슬의 글꼴을 하나도 찾지 못했다");
	let (last_resort_source, last_resort_family_name) = find_last_resort_font_file().expect("마지막 대체 글꼴");
	println!("마지막 대체 글꼴: {} → {}", last_resort_family_name, last_resort_source.path.display());
	for (text, is_color_wanted) in [("A", false), ("가", false), ("日", false), ("ب", false), ("ภ", false), ("\u{1F600}", true)] {
		let source = find_fallback_font_file(text, 400, false, is_color_wanted).unwrap_or_else(|| panic!("{} 의 대체 글꼴을 찾지 못했다", text));
		let character = text.chars().next().expect("character");
		assert!(has_character(&source, character), "{} 의 대체 글꼴 {} 에 그 글자가 없다", text, source.path.display());
		println!("대체 글꼴 {} (색 {}): {} (얼굴 {:?})", text, is_color_wanted, source.path.display(), source.face_index);
	}
	assert!(find_family_font_file("NoSuchFamily VanillaEngine", 400, false).is_none());
}

//==============================================================================
/// 원본 사슬과 대체 글꼴로 한글, 아랍어, 태국어를 그리고 이모지는 색으로 그린다.
//==============================================================================
#[test]
#[ignore = "운영체제 글꼴이 깔린 기계에서 돌린다 (CI platforms 워크플로)"]
fn rasterizes_fallback_text_and_color_emoji() {
	let text_string_texture_cache = create_system_text_cache();
	for text in ["Hello", "가나다", "日本語", "مرحبا", "ภาษาไทย"] {
		let text_raster = text_string_texture_cache.rasterize_text(text, "32px default", 1.0, 2, ShapingDirection::LeftToRight).unwrap_or_else(|| panic!("{} 래스터", text));
		let covered_count = text_raster.coverage.iter().filter(|value| **value > 0).count();
		assert!(covered_count > 0, "{} 가 비었다", text);
		println!("{}: {} x {}, 덮인 픽셀 {}, 전진폭 {}", text, text_raster.width, text_raster.height, covered_count, text_raster.advance_width);
	}
	let emoji_raster = text_string_texture_cache.rasterize_text("\u{1F600}", "32px default", 1.0, 2, ShapingDirection::LeftToRight).expect("emoji raster");
	let color_pixels = emoji_raster.color_pixels.expect("색 이모지 층이 없다");
	let mut opaque_count = 0;
	let mut colored_count = 0;
	for pixel in color_pixels.chunks(4) {
		if pixel[3] > 0 {
			opaque_count += 1;
			if pixel[0] != pixel[1] || pixel[1] != pixel[2] {
				colored_count += 1;
			}
		}
	}
	assert!(opaque_count > 0 && colored_count > 0, "색 이모지가 비었거나 회색이다 (보이는 픽셀 {}, 색 픽셀 {})", opaque_count, colored_count);
	println!("이모지: {} x {}, 보이는 픽셀 {}, 색 픽셀 {}, 전진폭 {}", emoji_raster.width, emoji_raster.height, opaque_count, colored_count, emoji_raster.advance_width);
}

//==============================================================================
/// 같은 기계의 크롬 Canvas2D measureText 와 견준다. (전진폭과 글꼴 위아래 길이, 실제로 닿는 거리는 찍기만 한다)
//==============================================================================
#[test]
#[ignore = "같은 기계의 크롬이 찍은 chrome_measure.json 이 있어야 한다 (CI platforms 워크플로)"]
fn text_widths_match_chrome() {
	let json_path = std::env::var("VANILLA_CHROME_MEASURE_PATH").unwrap_or_else(|_| String::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/systemfont/chrome_measure.json")));
	let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap_or_else(|error| panic!("{}: {}", json_path, error))).expect("json");
	let chrome_platform = json["platform"].as_str().expect("platform");
	let platform = match std::env::consts::OS {
		"windows" => {
			"win32"
		}
		"macos" => {
			"darwin"
		}
		other => {
			other
		}
	};
	assert_eq!(chrome_platform, platform, "다른 운영체제에서 찍은 값이다");
	println!("{} ({} {}), 화면 언어 {:?}, 엔진 화면 언어 {}", json["chrome"].as_str().unwrap_or(""), chrome_platform, json["architecture"].as_str().unwrap_or(""), json["languages"], read_browser_locale());
	let text_string_texture_cache = create_system_text_cache();
	let mut mismatch_list = Vec::new();
	let case_list = json["cases"].as_array().expect("cases");
	for test_case in case_list {
		let font = test_case["font"].as_str().expect("font");
		let text = test_case["text"].as_str().expect("text");
		let read_number = |name: &str| -> f64 {
			test_case[name].as_f64().unwrap_or(f64::NAN)
		};
		let text_metrics = text_string_texture_cache.measure_text(font, text);
		let width_difference = (text_metrics.width - read_number("width")).abs();
		let ascent_difference = (text_metrics.font_ascent - read_number("fontBoundingBoxAscent")).abs();
		let descent_difference = (text_metrics.font_descent - read_number("fontBoundingBoxDescent")).abs();
		let is_matched = width_difference < CHROME_TOLERANCE && ascent_difference < CHROME_TOLERANCE && descent_difference < CHROME_TOLERANCE;
		println!(
			"{} [{}] {}: 폭 {} / {}, 위 {} / {}, 아래 {} / {}, 닿는 왼 {} / {}, 오른 {} / {}, 위 {} / {}, 아래 {} / {}",
			if is_matched { "같음" } else { "다름" },
			font,
			text,
			text_metrics.width,
			read_number("width"),
			text_metrics.font_ascent,
			read_number("fontBoundingBoxAscent"),
			text_metrics.font_descent,
			read_number("fontBoundingBoxDescent"),
			text_metrics.actual_bounding_box_left,
			read_number("actualBoundingBoxLeft"),
			text_metrics.actual_bounding_box_right,
			read_number("actualBoundingBoxRight"),
			text_metrics.actual_bounding_box_ascent,
			read_number("actualBoundingBoxAscent"),
			text_metrics.actual_bounding_box_descent,
			read_number("actualBoundingBoxDescent"),
		);
		if !is_matched {
			mismatch_list.push(format!("[{}] {}: 폭 차이 {}, 위 차이 {}, 아래 차이 {}", font, text, width_difference, ascent_difference, descent_difference));
		}
	}
	println!("크롬과 같은 것 {} / {}", case_list.len() - mismatch_list.len(), case_list.len());
	assert!(mismatch_list.is_empty(), "크롬과 다른 것 {} 개:\n{}", mismatch_list.len(), mismatch_list.join("\n"));
}

//==============================================================================
/// 시스템 클립보드에 쓴 글을 그대로 읽는다. (한글과 이모지 포함)
//==============================================================================
#[test]
#[ignore = "화면 서버의 클립보드가 있어야 한다 (CI platforms 워크플로)"]
fn clipboard_round_trip() {
	let text = "vanilla clipboard 클립보드 \u{1F600}";
	write_clipboard_text(text);
	assert_eq!(read_clipboard_text().as_deref(), Some(text));
}
