//==============================================================================
//! 문자열 텍스처 캐시.
//! - 원본은 오프스크린 2D 캔버스에 fillText / strokeText 로 문자열을 구워 텍스처로 쓴다. (LRU 상한 512)
//! - 데스크탑은 글자 모양 짓기(textshaping.rs: 양방향 정렬, rustybuzz 모양 짓기)로 글리프를 놓고 래스터화해(fontdue, 가변 글꼴은 skrifa 윤곽, 맥 색 이모지는 CoreText) 문자열 한 장으로 굽는다.
//!   상한과 키 구성(방식, 문단 방향, 폰트 문자열, 선 두께, 배율, 글)은 원본과 같다.
//!   원본은 키를 문자열로 이어 붙인다. 여기서는 찾을 때마다 문자열을 만들지 않게 키의 해시로 찾고, 같은 해시 안에서 키를 하나씩 맞춰 본다.
//! - 글자는 흰색으로 굽고, 그릴 때 색을 곱한다. (색이 바뀌어도 다시 굽지 않는다)
//!   색 글리프(이모지)는 제 색으로 흰 글자 위에 얹어 굽는다. 원본도 흰색으로 fillText 한 캔버스를 올리므로 이모지는 제 색으로 구워지고 그릴 때 같은 색이 곱해진다.
//!   외곽선(Stroke)으로 구울 때도 색 글리프는 테를 긋지 않고 색 그대로 얹는다. (크롬 strokeText 와 같다)
//! - 폰트 문자열은 Canvas2D font 와 같은 꼴(CSS font 줄임 꼴)을 읽는다. 예: "16px DOSGothic", "bold 24px default", "italic 500 16px default", "12pt 'A B', C, sans-serif"
//!   크기는 px 밖의 CSS 단위(ex, ch, cap, ic, lh, rem, 뷰포트 단위 포함), 이름 크기, calc() 도 크롬 캔버스처럼 읽고 "/줄높이"는 넘긴다.
//!   쉼표로 이은 이름 목록은 글자마다 앞 이름의 얼굴부터 찾는다. (브라우저의 글꼴 목록과 같다, parse_font_string)
//! - 글꼴은 원본 document.fonts 처럼 이름 하나 아래에 얼굴(굵기, 기울기, 글자 범위)을 여럿 올린다. (register_font_face)
//!   고르기는 CSS 의 차례를 따르고, 기울인 얼굴이 없으면 가짜 기울임(기울기 1/4), 굵은 얼굴이 없으면 가짜 굵게를 글리프마다 입힌다. (textshaping.rs)
//! - 이름의 얼굴들에 없는 글자는 기본 폰트, 시스템 글꼴 사슬, 표준 글꼴(데스크탑, 크롬의 -webkit-standard), 대체 폰트 순으로 찾는다. (브라우저의 글꼴 대체와 같은 자리)
//!   이름 목록에서 얼굴을 하나도 찾지 못하면(default 가 아닌 없는 이름뿐이면) 크롬처럼 표준 글꼴을 기본 폰트보다 먼저 둔다.
//!   데스크탑 시스템 글꼴 사슬(set_system_font_chain)은 원본 SYSTEM_FONT_STRING 의 이름을 운영체제 글꼴에서 찾은 것이고, 기본 글꼴 이름 default 를 청했을 때만 기본 폰트 뒤에 잇는다.
//!   켜면 폰트 문자열의 올리지 않은 이름도 운영체제 글꼴에서 찾는다. 얼굴은 그 굵기와 기울기의 글자를 처음 쓸 때 읽는다. (platform/desktop/systemfont.rs)
//!   대체 글꼴은 운영체제가 고른다. (set_system_font_fallback_enabled, 굵게를 청하면 굵은 얼굴을 따로 묻는다) 크롬처럼 첫 글꼴(빈칸을 가진 첫 얼굴)과 폰트 문자열마다 따로 묻고,
//!   대체 얼굴은 운영체제가 그 얼굴을 고른 글자만 그린다. 이름 목록의 얼굴이 정준 분해로 그리는 글자(ï 를 i 와 U+0308 로)는 묻지 않는다. 후보 파일을 주면(add_fallback_font_candidate) 그것을 먼저 쓴다.
//!   후보 파일은 쓸 때 읽는다. 이모지 표시 글자는 색 글리프 표가 있는 후보를, 그 밖의 글자는 색 글리프 표가 없는 후보를 먼저 읽는다. (표 목록만 먼저 엿본다)
//! - 글꼴 얼굴을 새로 올리면 그 이름을 쓰는 폰트 문자열로 구웠고 그 얼굴의 글자 범위에 든 글자가 있는 문자열만 버린다. (원본 invalidateFontFaces)
//!   원본처럼 set_rebake_text_on_font_load 로 켰을 때만 한다. (엔진이 EngineConfiguration.rebake_text_on_font_load 로 켠다, 기본 끔)
//!   끄면 받기 전에 다른 글꼴로 구운 글자는 캐시에서 밀려날 때까지 그대로 남는다. 얼굴 지우기와 차례 바꾸기는 켜짐과 상관없이 버린다.
//!   기본 폰트와 대체 폰트를 바꾸면 모두 버린다.
//! - 웹은 원본처럼 늘 브라우저 Canvas2D 로 굽고 잰다. (platform/web/systemfont.rs) 올린 글꼴 바이트는 document.fonts 에 FontFace 로 올려 브라우저가 고르고 그린다.
//!   올린 글꼴이 받아지면(loadingdone) 프레임 시작에 구운 글을 버린다. 데스크탑은 위의 모양 짓기와 래스터로 굽는다.
//! - 글자 안티앨리어싱(set_text_antialias_enabled, 원본 setTextAntialiasEnabled): 끄면 구운 글자의 알파를 문턱값 128 로 0 과 255 로 가르고 텍스처를 NEAREST 로 뽑는다.
//!   도트 글꼴을 설계 크기의 정수 배로 그리는 프로젝트가 끈다. 원본은 모듈 전역 값이고 이미 구운 글자는 그대로 둔다.
//!   여기서는 캐시가 들고, 켜짐 여부를 키에 넣어 바꾼 다음 그리는 글자부터 바로 그 방식으로 굽는다. (rasterize_text 는 원본 ShadowText 처럼 받지 않는다)
//! - 글 재기(measure_text)는 폰트 문자열과 글이 같으면 다시 재지 않는다. (원본 measureTextMetrics 의 캐시, 2048 개)
//!   원본은 넘칠 때마다 가장 오래 안 쓴 하나를 버린다. 여기서는 넘치면 오래 안 쓴 반을 한 번에 버린다. (하나씩 찾으면 넘친 뒤로 잴 때마다 2048 개를 훑는다)
//!   글꼴을 새로 등록하면 비운다. (원본은 웹 글꼴이 받아지면 비운다) measure_text 가 &self 라 캐시는 RefCell 에 든다.
//! - 얼굴을 그때그때 받기(watch_unresolved_family_characters, take_unresolved_family_characters): 원본 브라우저는 올려 둔 FontFace 의 글자 범위에 든 글자를 그릴 때 그 얼굴을 받는다.
//!   러스트 판은 지켜보는 이름으로 재거나 구운 글에서 올린 얼굴의 범위에 들지 않은 글자를 모으고, UIFont 가 update 에서 꺼내 그 글자의 조각을 받는다.
//! - Graphic 없이 재기(measure_text_without_graphic): 원본은 어디서든 공용 2D 캔버스로 글을 잰다. 러스트 판은 글꼴을 등록할 때마다
//!   글꼴 목록(Rc)을 스레드 하나의 전역에 알려 두고, 그 목록으로 잰다. UI 레이아웃이 그리기 전에 글 줄 수를 알아야 할 때 쓴다. (UIConfirmWindow)
//==============================================================================
use crate::base::color::Color;
use crate::core::textshaping::{self, FontFaceDescriptors, LoadedFont, RegisteredFontFace, ResolvedFontFace, ShapedText, ShapingDirection};
use unicode_segmentation::UnicodeSegmentation;
use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

//==============================================================================
// Graphic 없이 재기에 쓰는 캐시. (마지막으로 글꼴을 등록한 캐시의 얼굴 표를 옮겨 담는다, GL 은 쓰지 않는다)
//==============================================================================
thread_local! {
	static SHARED_MEASURE_CACHE: RefCell<Option<TextStringTextureCache>> = const { RefCell::new(None) };
	/// 지켜보는 이름(소문자) → 그 이름으로 재거나 구운 글 가운데 그 이름의 어느 얼굴의 글자 범위에도 들지 않은 글자. (UIFont 가 조각을 받는다)
	static UNRESOLVED_FAMILY_CHARACTER_TABLE: RefCell<HashMap<String, std::collections::BTreeSet<char>>> = RefCell::new(HashMap::new());
}

//==============================================================================
/// 글꼴 이름의 열쇠. (앞뒤 빈칸과 감싼 따옴표를 떼고 소문자, 폰트 문자열 "'My Font', serif" 의 이름을 읽은 것과 맞춘다)
//==============================================================================
fn create_family_key(family_name: &str) -> String {
	family_name.trim().trim_matches(|character| character == '"' || character == '\'').to_lowercase()
}

//==============================================================================
/// 이름의 얼굴이 없는 글자 지켜보기 시작. (원본 브라우저는 FontFace 를 document.fonts 에 올려 두면 캔버스가 그 글꼴로 글을 그릴 때 그 글자의 범위에 든 얼굴을 받기 시작한다)
/// - 러스트 판은 얼굴을 받은 뒤에 올리므로, 지켜보는 이름으로 재거나 구운 글에서 올린 얼굴의 글자 범위에 들지 않은 글자를 모아 받는 쪽(UIFont)이 꺼내 받는다.
///
/// # 인자
/// - `family_name`: 지켜볼 이름. (앞뒤 빈칸은 떼고 대소문자는 가리지 않는다)
//==============================================================================
pub fn watch_unresolved_family_characters(family_name: &str) {
	let family_key = create_family_key(family_name);
	UNRESOLVED_FAMILY_CHARACTER_TABLE.with(|table_cell| {
		table_cell.borrow_mut().entry(family_key).or_default();
	});
}

//==============================================================================
/// 지켜보는 이름에서 얼굴이 없던 글자 꺼내기. (꺼내면 비운다, 지켜보지 않는 이름이면 빈 글)
///
/// # 인자
/// - `family_name`: 지켜보는 이름. (앞뒤 빈칸은 떼고 대소문자는 가리지 않는다)
///
/// # 돌려줌
/// 모인 글자를 코드 포인트 차례로 이은 글. (빈칸과 제어 글자는 모으지 않는다)
///
/// # 예제
/// 올린 얼굴이 없는 이름으로 잰 글자 모으기
/// ```rust
/// use vanilla::core::textstringtexturecache::{take_unresolved_family_characters, watch_unresolved_family_characters, TextStringTextureCache};
///
/// watch_unresolved_family_characters("GameFont");
/// let text_string_texture_cache = TextStringTextureCache::new();
/// text_string_texture_cache.measure_text("16px GameFont", "b a");
/// assert_eq!(take_unresolved_family_characters("gamefont"), "ab");
/// assert_eq!(take_unresolved_family_characters("GameFont"), "");
/// assert_eq!(take_unresolved_family_characters("OtherFont"), "");
/// ```
//==============================================================================
pub fn take_unresolved_family_characters(family_name: &str) -> String {
	let family_key = create_family_key(family_name);
	UNRESOLVED_FAMILY_CHARACTER_TABLE.with(|table_cell| {
		match table_cell.borrow_mut().get_mut(&family_key) {
			Some(character_set) => {
				std::mem::take(character_set).into_iter().collect()
			}
			None => {
				String::new()
			}
		}
	})
}

//==============================================================================
/// Graphic 없이 CSS line-height normal 의 줄 높이 재기. (폰트 문자열, 크롬은 첫째로 쓸 수 있는 글꼴(primary font)의 LineSpacing)
/// - 데스크탑은 그릴 때와 같은 얼굴 고르기의 첫 얼굴로 셈한다. (LoadedFont::calculate_normal_line_height) 글꼴이 하나도 없으면 None 이다.
/// - 웹은 브라우저가 놓은 줄 높이를 읽는다. (platform/web/systemfont.rs)
///
/// # 인자
/// - `font_string`: Canvas2D font 꼴 글. ("bold 16px sans-serif" 처럼)
///
/// # 돌려줌
/// 줄 높이. (CSS 픽셀) 데스크탑은 글꼴을 등록한 글자 캐시가 없거나 고를 얼굴이 없으면, 웹은 문서가 없거나 잰 높이가 0 이하면 None.
//==============================================================================
pub fn measure_normal_line_height_without_graphic(font_string: &str) -> Option<f32> {
	let font_description = parse_font_string(font_string);
	#[cfg(target_arch = "wasm32")]
	{
		crate::platform::web::systemfont::measure_normal_line_height(&font_description)
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		SHARED_MEASURE_CACHE.with(|shared_measure_cache_cell| {
			let shared_measure_cache = shared_measure_cache_cell.borrow();
			let face_list = shared_measure_cache.as_ref()?.resolve_face_list(&font_description);
			let primary_face = face_list.first()?;
			Some(primary_face.font.calculate_normal_line_height(font_description.font_size))
		})
	}
}

//==============================================================================
/// Graphic 없이 글 재기. (폰트 문자열과 글, 원본의 공용 캔버스 measureText 자리)
/// - 그릴 때와 같은 얼굴 고르기와 모양 잇기로 잰다.
/// - 웹은 브라우저 Canvas2D 로 잰다. 데스크탑은 글꼴이 하나도 없으면 None 이다. (그릴 때도 글이 나오지 않는 상태)
///
/// # 인자
/// - `font_string`: 폰트 문자열. (Canvas2D font 꼴, parse_font_string)
/// - `text`: 잴 글.
///
/// # 돌려줌
/// 잰 값. (픽셀) 데스크탑에서 이 스레드에 글꼴을 등록한 캐시가 아직 없거나, 그 폰트 문자열로 고를 얼굴이 하나도 없으면 None.
///
/// # 예제
/// 글꼴을 등록한 뒤 Graphic 없이 재기
/// ```rust
/// use vanilla::core::textstringtexturecache::{measure_text_without_graphic, TextStringTextureCache};
/// use vanilla::platform::find_fallback_font_file;
///
/// assert!(measure_text_without_graphic("16px default", "Hello").is_none());
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let font_bytes = std::fs::read(&source.path).expect("font file");
/// 	let mut text_string_texture_cache = TextStringTextureCache::new();
/// 	text_string_texture_cache.set_default_font(false, font_bytes).expect("font");
/// 	let text_metrics = measure_text_without_graphic("16px default", "Hello").expect("metrics");
/// 	assert!(text_metrics.width > 0.0);
/// }
/// ```
//==============================================================================
pub fn measure_text_without_graphic(font_string: &str, text: &str) -> Option<TextMetrics> {
	SHARED_MEASURE_CACHE.with(|shared_measure_cache_cell| {
		let shared_measure_cache = shared_measure_cache_cell.borrow();
		match shared_measure_cache.as_ref() {
			Some(shared_measure_cache) => {
				let font_description = parse_font_string(font_string);
				if shared_measure_cache.resolve_face_list(&font_description).is_empty() {
					#[cfg(target_arch = "wasm32")]
					{
						return Some(crate::platform::web::systemfont::measure_text(&font_description, text));
					}
					#[cfg(not(target_arch = "wasm32"))]
					{
						return None;
					}
				}
				Some(shared_measure_cache.measure_text(font_string, text))
			}
			None => {
				#[cfg(target_arch = "wasm32")]
				{
					Some(crate::platform::web::systemfont::measure_text(&parse_font_string(font_string), text))
				}
				#[cfg(not(target_arch = "wasm32"))]
				{
					None
				}
			}
		}
	})
}

//==============================================================================
/// Graphic 없이 글 래스터 받기. (2D 그리기 대상 Canvas2D 의 fillText 가 쓴다, 그릴 때와 같은 얼굴 고르기와 모양 짓기)
/// - 데스크탑만. 글꼴이 하나도 없거나 아직 등록한 캐시가 없으면 None 이다.
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn rasterize_text_without_graphic(text: &str, font_string: &str, bake_scale: f32, padding: i32, direction: ShapingDirection, color: Color) -> Option<TextRaster> {
	SHARED_MEASURE_CACHE.with(|shared_measure_cache_cell| {
		let shared_measure_cache = shared_measure_cache_cell.borrow();
		shared_measure_cache.as_ref()?.rasterize_text_with_color(text, font_string, bake_scale, padding, direction, color)
	})
}

/// 흰 글자 색. (원본처럼 흰색으로 굽고 그릴 때 색을 곱하는 글의 감마 전보정 밝기)
const WHITE_LUMINANCE_COLOR: [u8; 3] = [255, 255, 255];
/// 서브픽셀 자리 반올림 몫. (스키아 SkPackedGlyphID::kSubpixelRound, 1/8)
#[cfg(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android"), not(target_arch = "wasm32"))))]
const SUBPIXEL_ROUND: f32 = 0.125;
/// 서브픽셀 자리의 단계 수. (스키아 kSubPixelPosLen 2 비트)
#[cfg(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android"), not(target_arch = "wasm32"))))]
const SUBPIXEL_STEP_COUNT: f32 = 4.0;
/// 캐시 상한. (원본과 같음)
const MAXIMUM_ENTRY_COUNT: usize = 512;
/// 글 재기 캐시 상한. (원본과 같음)
const MAXIMUM_MEASURE_COUNT: usize = 2048;

/// 안티앨리어싱을 끈 글자의 알파 문턱값. (원본과 같음)
const ALPHA_THRESHOLD: u8 = 128;

//==============================================================================
/// 굽는 방식.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextDrawKind {
	/// 채워 굽기. (원본 fillText)
	Fill,
	/// 외곽선으로 굽기. (원본 strokeText, 두께는 line_width)
	Stroke,
}

//==============================================================================
/// 구운 문자열 하나. (길이 단위는 모두 논리 픽셀, 베이크 배율로 나눈 값)
/// - 텍스처 왼쪽 위는 (펜 x - pen_offset_x, 기준선 y - baseline_offset_y) 에 놓고 quad_width, quad_height 크기로 그린다.
///
/// # 예제
/// 구운 문자열을 펜 자리와 기준선에 놓을 사각형
/// ```rust,no_run
/// use vanilla::core::textstringtexturecache::{TextDrawKind, TextStringTextureCache};
/// use vanilla::{Rect, ShapingDirection};
///
/// fn calculate_quad(text_string_texture_cache: &mut TextStringTextureCache, gl: &glow::Context, pen_x: f32, baseline_y: f32) -> Option<Rect> {
/// 	let entry = text_string_texture_cache.get_entry(gl, TextDrawKind::Fill, "Score", "24px default", 0.0, 2.0, ShapingDirection::LeftToRight)?;
/// 	Some(Rect::create(pen_x - entry.pen_offset_x, baseline_y - entry.baseline_offset_y, entry.quad_width, entry.quad_height))
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct TextEntry {
	/// 구운 텍스처. (흰 글자의 프리멀티플라이드 RGBA, 색 글리프는 제 색)
	pub texture: crate::core::graphic::GlTexture,
	/// 전진폭.
	pub advance_width: f32,
	/// 글꼴의 기준선 위 길이. (양수)
	pub font_ascent: f32,
	/// 글꼴의 기준선 아래 길이. (양수)
	pub font_descent: f32,
	/// 텍스처 왼쪽에서 펜 원점까지의 거리.
	pub pen_offset_x: f32,
	/// 텍스처 위에서 기준선까지의 거리.
	pub baseline_offset_y: f32,
	/// 그릴 사각형 너비. (텍스처 너비를 굽는 배율로 나눈 값)
	pub quad_width: f32,
	/// 그릴 사각형 높이. (텍스처 높이를 굽는 배율로 나눈 값)
	pub quad_height: f32,
}

//==============================================================================
/// 글 모양 래스터. (덮인 정도 0 ~ 255, 길이 단위는 굽는 배율을 곱한 픽셀)
/// - coverage 는 색 글리프를 뺀 글자의 덮인 정도다. 색 글리프(이모지)가 있으면 color_pixels 에 제 색으로 든다. (프리멀티플라이드 RGBA, 같은 크기)
///
/// # 예제
/// 글 모양 래스터 받기
/// ```rust
/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::ShapingDirection;
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let font_bytes = std::fs::read(&source.path).expect("font file");
/// 	let mut text_string_texture_cache = TextStringTextureCache::new();
/// 	text_string_texture_cache.set_default_font(false, font_bytes).expect("font");
/// 	let text_raster = text_string_texture_cache.rasterize_text("Hi", "20px default", 2.0, 2, ShapingDirection::LeftToRight).expect("raster");
/// 	assert_eq!(text_raster.coverage.len(), (text_raster.width * text_raster.height) as usize);
/// 	assert!(text_raster.origin_x >= 2.0);
/// 	assert!(text_raster.baseline_y > 0.0);
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct TextRaster {
	/// 덮인 정도. (0 ~ 255, width * height 개, 위 줄부터)
	pub coverage: Vec<u8>,
	/// 색 글리프 층. (프리멀티플라이드 RGBA, width * height * 4 바이트, 색 글리프가 없으면 None)
	pub color_pixels: Option<Vec<u8>>,
	/// 래스터 너비. (픽셀)
	pub width: i32,
	/// 래스터 높이. (픽셀)
	pub height: i32,
	/// 래스터 왼쪽에서 펜 원점까지의 거리. (픽셀)
	pub origin_x: f32,
	/// 래스터 위에서 기준선까지의 거리. (픽셀)
	pub baseline_y: f32,
	/// 전진폭. (픽셀)
	pub advance_width: f32,
	/// 글꼴의 기준선 위 길이. (픽셀, 양수)
	pub font_ascent: f32,
	/// 글꼴의 기준선 아래 길이. (픽셀, 양수)
	pub font_descent: f32,
}

//==============================================================================
/// 문자열 측정 결과. (Canvas2D TextMetrics)
/// - actual_bounding_box_* 는 펜 원점(왼쪽 정렬, 알파벳 기준선)에서 글리프가 실제로 닿는 거리다. (왼쪽, 위, 아래는 그쪽으로 양수)
///
/// # 예제
/// 글을 재고 실제로 닿는 거리 보기
/// ```rust
/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
/// use vanilla::platform::find_fallback_font_file;
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let font_bytes = std::fs::read(&source.path).expect("font file");
/// 	let mut text_string_texture_cache = TextStringTextureCache::new();
/// 	text_string_texture_cache.set_default_font(false, font_bytes).expect("font");
/// 	let text_metrics = text_string_texture_cache.measure_text("20px default", "Hg");
/// 	assert!(text_metrics.width > 0.0);
/// 	assert!(text_metrics.font_ascent > 0.0);
/// 	assert!(text_metrics.actual_bounding_box_ascent > 0.0);
/// 	assert!(text_metrics.actual_bounding_box_descent > 0.0);
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct TextMetrics {
	/// 전진폭. (픽셀)
	pub width: f32,
	/// 글꼴의 기준선 위 길이. (첫 얼굴의 줄 높이 값, 양수)
	pub font_ascent: f32,
	/// 글꼴의 기준선 아래 길이. (첫 얼굴의 줄 높이 값, 양수)
	pub font_descent: f32,
	/// 펜 원점에서 글리프 왼쪽 끝까지. (왼쪽이 양수)
	pub actual_bounding_box_left: f32,
	/// 펜 원점에서 글리프 오른쪽 끝까지. (오른쪽이 양수)
	pub actual_bounding_box_right: f32,
	/// 기준선에서 글리프 위 끝까지. (위가 양수)
	pub actual_bounding_box_ascent: f32,
	/// 기준선에서 글리프 아래 끝까지. (아래가 양수)
	pub actual_bounding_box_descent: f32,
}

//==============================================================================
/// 폰트 문자열 해석 결과.
/// - font_weight 는 CSS 굵기다. ("bold" 700, 숫자, 없으면 400) is_bold 는 굵기가 600 이상인지다.
///
/// # 예제
/// 폰트 문자열 해석하기
/// ```rust
/// use vanilla::core::textstringtexturecache::parse_font_string;
///
/// let font_description = parse_font_string("italic bold 20px \"My Font\", sans-serif");
/// assert_eq!(font_description.font_size, 20.0);
/// assert_eq!(font_description.font_weight, 700);
/// assert!(font_description.is_bold);
/// assert!(font_description.is_italic);
/// assert_eq!(font_description.family_name, "My Font");
/// assert_eq!(font_description.family_name_list, vec!["My Font", "sans-serif"]);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FontDescription {
	/// 글자 크기. (픽셀)
	pub font_size: f32,
	/// CSS 굵기. (1 ~ 1000, 없으면 400)
	pub font_weight: u16,
	/// 굵기가 600 이상인지.
	pub is_bold: bool,
	/// italic 이나 oblique 인지.
	pub is_italic: bool,
	/// 목록의 첫 이름.
	pub family_name: String,
	/// 쉼표로 이은 이름 목록. (앞 이름부터 찾는다)
	/// - 따옴표 친 이름이 일반 이름(serif, sans-serif, cursive, fantasy, monospace, system-ui, -webkit-body, math)과 같은 글자면 따옴표를 남긴다. ("'monospace'" → "\"monospace\"")
	///   크롬처럼 따옴표 친 것은 그 이름의 글꼴로, 따옴표 없는 것은 일반 이름으로 찾는다.
	pub family_name_list: Vec<String>,
}

//==============================================================================
/// 글꼴 파일 하나의 얼굴. (데스크탑 시스템 글꼴, platform 이 찾아 준다)
/// - face_index 를 모르면(맥) post_script_name 으로 모음 안의 얼굴을 찾고, 그것도 없으면 굵기와 기울기로 고른다.
///
/// # 예제
/// 운영체제가 고른 글꼴 파일을 시스템 글꼴 이름의 얼굴 후보로 더하기
/// ```rust
/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
/// use vanilla::platform::find_fallback_font_file;
///
/// let mut text_string_texture_cache = TextStringTextureCache::new();
/// assert!(!text_string_texture_cache.has_system_font());
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	assert!(source.path.exists());
/// 	text_string_texture_cache.add_system_font_candidate("GameSans", 400, false, source);
/// 	assert!(text_string_texture_cache.has_system_font());
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFileSource {
	/// 글꼴 파일 경로.
	pub path: std::path::PathBuf,
	/// 글꼴 모음(TTC) 안의 얼굴 번호. (모르면 None)
	pub face_index: Option<u32>,
	/// 얼굴의 PostScript 이름. (모르면 None)
	pub post_script_name: Option<String>,
}

//==============================================================================
/// 데스크탑 시스템 글꼴의 얼굴 후보 하나. (청한 굵기와 기울기로 운영체제가 찾아 준 파일, 쓸 때 읽는다)
//==============================================================================
struct SystemFontCandidate {
	weight: u16,
	is_italic: bool,
	source: FontFileSource,
	is_loaded: bool,
}

//==============================================================================
/// 데스크탑 시스템 글꼴 이름 하나. (얼굴 후보들과 읽은 얼굴들)
//==============================================================================
#[derive(Default)]
struct SystemFontFamily {
	candidate_list: Vec<SystemFontCandidate>,
	face_list: Vec<RegisteredFontFace>,
}

//==============================================================================
/// 데스크탑 시스템 글꼴 상태. (원본 브라우저가 CSS 이름을 운영체제 글꼴로 찾고, 없는 글자를 운영체제의 대체 글꼴로 그리는 자리)
/// - Graphic 없이 재기의 사본과 나눠 들어(Rc) 같은 파일을 두 번 읽지 않는다.
//==============================================================================
#[derive(Default)]
struct SystemFontState {
	/// 올리지 않은 이름을 운영체제 글꼴에서 찾는지. (set_system_font_chain 이 켠다)
	is_lookup_enabled: bool,
	/// 글꼴에 없는 글자를 운영체제가 고른 대체 글꼴로 그리는지.
	is_fallback_enabled: bool,
	/// 기본 글꼴 사슬. (원본 SYSTEM_FONT_STRING 의 이름 차례, default 를 청하면 기본 폰트 뒤에 찾는다)
	chain_name_list: Vec<String>,
	/// 이름(소문자) → 찾은 글꼴. (None 은 운영체제에 없는 이름)
	family_table: HashMap<String, Option<SystemFontFamily>>,
	/// 이미 물어본 대체 글꼴. (바탕 열쇠, 글자, 굵게, 기울임, 색)
	fallback_query_set: std::collections::HashSet<(String, char, bool, bool, bool)>,
	/// 운영체제가 고른 대체 글꼴 얼굴. (source_key 가 같은 것끼리 한 이름으로 고른다)
	fallback_face_list: Vec<RegisteredFontFace>,
	/// 대체 글꼴 얼굴마다 물은 바탕의 열쇠. (fallback_face_list 와 같은 차례, 크롬은 첫 글꼴과 폰트 문자열마다 대체를 따로 고른다)
	fallback_base_key_list: Vec<String>,
	/// 운영체제가 준 파일과 얼굴을 읽은 자리. (바탕 열쇠, 준 파일과 얼굴, 굵기, 기울임, fallback_face_list 의 차례. 같은 답이면 다시 읽지 않고 글자 범위만 더한다)
	fallback_source_memo_list: Vec<(String, FontFileSource, u16, bool, usize)>,
	/// 읽은 파일 바이트. (같은 파일의 여러 얼굴이 나눠 든다)
	shared_bytes_table: HashMap<std::path::PathBuf, std::rc::Weak<[u8]>>,
}

struct CachedTextEntry {
	text_draw_kind: TextDrawKind,
	is_antialias: bool,
	direction: ShapingDirection,
	text: String,
	font_string: String,
	line_width: f32,
	bake_scale: f32,
	entry: TextEntry,
	last_used_frame_number: u64,
}

impl CachedTextEntry {
	//==============================================================================
	/// 키가 같은지. (선 두께와 배율은 비트로 맞춘다)
	//==============================================================================
	fn is_same_key(&self, text_draw_kind: TextDrawKind, is_antialias: bool, direction: ShapingDirection, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> bool {
		self.text_draw_kind == text_draw_kind && self.is_antialias == is_antialias && self.direction == direction && self.line_width.to_bits() == line_width.to_bits() && self.bake_scale.to_bits() == bake_scale.to_bits() && self.font_string == font_string && self.text == text
	}
}

struct CachedTextMetrics {
	font_string: String,
	text: String,
	text_metrics: TextMetrics,
	last_used_count: u64,
}

//==============================================================================
/// 래스터로 놓은 글. (덮인 정도와 자리, 길이 단위는 배율을 곱한 픽셀)
/// - coverage 는 색 글리프를 뺀 글자, color_pixels 는 색 글리프 층이다. (프리멀티플라이드 RGBA, 색 글리프가 없으면 None)
//==============================================================================
struct LaidOutText {
	coverage: Vec<u8>,
	color_pixels: Option<Vec<u8>>,
	width: i32,
	height: i32,
	origin_x: f32,
	baseline_y: i32,
	advance_width: f32,
	font_ascent: f32,
	font_descent: f32,
}

//==============================================================================
/// 문자열 텍스처 캐시. (원본 TextStringTextureCache, 글꼴 얼굴 등록, 글 재기, 문자열을 텍스처로 굽기)
/// - Graphic 이 하나 든다. (get_text_string_texture_cache) 굽기(get_entry)는 GL 이 있어야 하고, 글꼴 등록과 재기와 래스터는 GL 없이 된다.
///
/// # 예제
/// 글꼴을 올리고 글 재기
/// ```rust
/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
/// use vanilla::platform::find_fallback_font_file;
///
/// let mut text_string_texture_cache = TextStringTextureCache::new();
/// assert!(text_string_texture_cache.register_font("GameFont", false, vec![0, 1, 2, 3]).is_err());
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let font_bytes = std::fs::read(&source.path).expect("font file");
/// 	text_string_texture_cache.register_font("GameFont", false, font_bytes).expect("font");
/// 	assert_eq!(text_string_texture_cache.get_font_face_count("gamefont"), 1);
/// 	let text_metrics = text_string_texture_cache.measure_text("20px GameFont", "Hello");
/// 	assert!(text_metrics.width > 0.0);
/// }
/// ```
//==============================================================================
pub struct TextStringTextureCache {
	/// 이름(소문자) → 그 이름의 얼굴들. (원본 document.fonts)
	font_family_table: HashMap<String, Vec<RegisteredFontFace>>,
	/// 이름(소문자) → 얼굴 열쇠(source_key)의 차례. (뒤가 먼저 그린다, set_font_face_order)
	font_face_order_table: HashMap<String, Vec<String>>,
	/// 기본 폰트의 얼굴들. (보통, 굵게)
	default_face_list: Vec<RegisteredFontFace>,
	/// 대체 폰트들. (앞의 것이 먼저, 후보 파일을 쓸 때 읽어 더하므로 RefCell)
	fallback_face_list: RefCell<Vec<RegisteredFontFace>>,
	/// 얼굴을 올린 차례 번호.
	face_order_number: u64,
	/// 키의 해시 → 그 해시를 가진 구운 문자열들.
	entry_table: HashMap<u64, Vec<CachedTextEntry>>,
	entry_count: usize,
	frame_number: u64,
	/// 글꼴을 등록한 횟수. (늘면 재 둔 글 폭이 바뀌었을 수 있다)
	font_registration_count: Cell<u64>,
	/// 글자가 없을 때 차례로 읽어 볼 대체 글꼴 파일. (원본 브라우저의 시스템 글꼴 대체 자리, 데스크탑, 쓸 때 읽는다)
	/// 앞의 경로와 그 파일에 색 글리프 표가 있는지. (더할 때 한 번 엿본다)
	fallback_font_candidate_list: RefCell<Vec<(std::path::PathBuf, bool)>>,
	/// 데스크탑 시스템 글꼴. (기본 글꼴 사슬, 이름 찾기, 대체 글꼴)
	system_font_state: Rc<RefCell<SystemFontState>>,
	/// 브라우저 Canvas2D 로 굽고 재는지. (웹, 원본과 같다)
	is_browser_raster: bool,
	/// 마지막으로 본 브라우저 글꼴 바뀐 횟수. (웹, 늘었으면 구운 글을 버린다)
	#[cfg(target_arch = "wasm32")]
	browser_font_change_count: u64,
	/// 버릴 구운 문자열의 텍스처. (등록할 때는 GL 이 없어서 다음 굽기 때 지운다)
	pending_deleted_texture_list: Vec<crate::core::graphic::GlTexture>,
	/// 글자 안티앨리어싱. (기본 켬)
	is_text_antialias_enabled: bool,
	/// 글꼴 얼굴을 새로 올리면 그 얼굴의 글자로 구운 문자열을 버릴지. (원본 rebakeTextOnFontLoad, 기본 끔)
	is_rebake_text_on_font_load: bool,
	/// 글 재기 캐시. (폰트 문자열과 글의 해시 → 잰 것들, 쓸 때마다 센 번호, 그때의 글꼴 등록 횟수)
	measure_table: RefCell<HashMap<u64, Vec<CachedTextMetrics>>>,
	measure_count: Cell<usize>,
	measure_use_count: Cell<u64>,
	measured_font_registration_count: Cell<u64>,
}

impl TextStringTextureCache {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 글꼴도 구운 문자열도 없는 캐시. (안티앨리어싱 켬, 다시 굽기 끔, 웹은 브라우저로 굽는다)
	//==============================================================================
	pub fn new() -> TextStringTextureCache {
		TextStringTextureCache {
			font_family_table: HashMap::new(),
			font_face_order_table: HashMap::new(),
			default_face_list: Vec::new(),
			fallback_face_list: RefCell::new(Vec::new()),
			face_order_number: 0,
			entry_table: HashMap::new(),
			entry_count: 0,
			frame_number: 0,
			font_registration_count: Cell::new(0),
			fallback_font_candidate_list: RefCell::new(Vec::new()),
			system_font_state: Rc::new(RefCell::new(SystemFontState::default())),
			is_browser_raster: cfg!(target_arch = "wasm32"),
			#[cfg(target_arch = "wasm32")]
			browser_font_change_count: 0,
			pending_deleted_texture_list: Vec::new(),
			is_text_antialias_enabled: true,
			is_rebake_text_on_font_load: false,
			measure_table: RefCell::new(HashMap::new()),
			measure_count: Cell::new(0),
			measure_use_count: Cell::new(0),
			measured_font_registration_count: Cell::new(0),
		}
	}

	//==============================================================================
	/// 프레임 시작. (LRU 순서를 프레임 번호로 센다)
	//==============================================================================
	pub fn begin_frame(&mut self) {
		self.frame_number += 1;
		// 웹: 올린 글꼴이 바뀌거나 받아졌으면 구운 글과 잰 글을 버린다. (원본 loadingdone)
		#[cfg(target_arch = "wasm32")]
		{
			let browser_font_change_count = crate::platform::web::systemfont::read_font_change_count();
			if browser_font_change_count != self.browser_font_change_count {
				self.browser_font_change_count = browser_font_change_count;
				self.clear();
				self.font_registration_count.set(self.font_registration_count.get() + 1);
			}
		}
	}

	//==============================================================================
	/// 브라우저 Canvas2D 로 굽고 재는지. (웹은 원본처럼 늘 참, 데스크탑은 거짓)
	///
	/// # 돌려줌
	/// 웹이면 true, 데스크탑이면 false.
	//==============================================================================
	pub fn is_browser_raster(&self) -> bool {
		self.is_browser_raster
	}

	//==============================================================================
	/// 폰트 등록. (폰트 문자열의 이름으로 찾는다. 대소문자 구분 없음, 굵으면 굵기 700 얼굴, 아니면 400)
	///
	/// # 인자
	/// - `family_name`: 폰트 문자열에서 쓸 이름. (앞뒤 빈칸과 따옴표는 뗀다)
	/// - `is_bold`: true 면 굵기 700 얼굴, false 면 400 얼굴.
	/// - `font_bytes`: 글꼴 파일 바이트. (ttf, otf, ttc 는 0 번 얼굴, woff, woff2 는 풀어서 읽는다)
	///
	/// # 돌려줌
	/// 올렸으면 Ok. 글꼴을 읽지 못하면 Err(까닭) 이고 아무것도 바뀌지 않는다.
	//==============================================================================
	pub fn register_font(&mut self, family_name: &str, is_bold: bool, font_bytes: Vec<u8>) -> Result<(), String> {
		self.register_font_face(family_name, FontFaceDescriptors::create_with_bold(is_bold), font_bytes)
	}

	//==============================================================================
	/// 글꼴 얼굴 등록. (원본 new FontFace(family, url, descriptors) 를 document.fonts 에 더하기)
	/// - 같은 이름에 같은 열쇠(source_key), 굵기, 기울기의 얼굴이 있으면 바꾼다. 없으면 맨 뒤(가장 먼저 그리는 자리)에 둔다.
	///   set_font_face_order 로 차례를 정한 이름이면 그 차례에 맞춰 놓는다.
	/// - set_rebake_text_on_font_load 로 켰으면 그 이름을 쓰는 폰트 문자열로 구웠고 이 얼굴의 글자 범위에 든 글자가 있는 문자열은 버린다. (다음에 그릴 때 이 얼굴로 다시 굽는다)
	///
	/// # 인자
	/// - `family_name`: 이름. (앞뒤 빈칸과 따옴표는 떼고 대소문자는 가리지 않는다)
	/// - `font_face_descriptors`: 얼굴의 굵기, 기울기, 글자 범위, 열쇠.
	/// - `font_bytes`: 글꼴 파일 바이트. (ttf, otf, ttc 는 0 번 얼굴, woff, woff2 는 풀어서 읽는다)
	///
	/// # 돌려줌
	/// 올렸으면 Ok. 글꼴을 읽지 못하면 Err(까닭) 이고 아무것도 바뀌지 않는다.
	///
	/// # 예제
	/// 라틴 글자 범위 얼굴 올리고 지우기
	/// ```rust
	/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
	/// use vanilla::platform::find_fallback_font_file;
	/// use vanilla::FontFaceDescriptors;
	///
	/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
	/// 	let font_bytes = std::fs::read(&source.path).expect("font file");
	/// 	let mut text_string_texture_cache = TextStringTextureCache::new();
	/// 	let mut font_face_descriptors = FontFaceDescriptors::create_from_css("400", "normal", "U+0000-00FF");
	/// 	font_face_descriptors.source_key = String::from("latin");
	/// 	text_string_texture_cache.register_font_face("GameFont", font_face_descriptors, font_bytes).expect("font");
	/// 	assert_eq!(text_string_texture_cache.get_font_face_count("GameFont"), 1);
	/// 	assert!(text_string_texture_cache.unregister_font_face("GameFont", "latin"));
	/// 	assert_eq!(text_string_texture_cache.get_font_face_count("GameFont"), 0);
	/// }
	/// ```
	//==============================================================================
	pub fn register_font_face(&mut self, family_name: &str, font_face_descriptors: FontFaceDescriptors, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(load_font(font_bytes)?);
		let family_key = create_family_key(family_name);
		self.face_order_number += 1;
		let order_number = self.face_order_number;
		let face_list = self.font_family_table.entry(family_key.clone()).or_default();
		face_list.retain(|face| !(face.descriptors.source_key == font_face_descriptors.source_key && face.descriptors.minimum_weight == font_face_descriptors.minimum_weight && face.descriptors.maximum_weight == font_face_descriptors.maximum_weight && face.descriptors.is_italic == font_face_descriptors.is_italic));
		face_list.push(RegisteredFontFace {
			descriptors: font_face_descriptors.clone(),
			font,
			order_number,
		});
		self.apply_font_face_order(&family_key);
		if self.is_rebake_text_on_font_load {
			self.invalidate_entries_for_face(&family_key, &font_face_descriptors);
		}
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	/// 글꼴 얼굴 지우기. (원본 document.fonts.delete, 열쇠가 같은 얼굴 모두, 지운 것이 있으면 true)
	///
	/// # 인자
	/// - `family_name`: 이름. (앞뒤 빈칸은 떼고 대소문자는 가리지 않는다)
	/// - `source_key`: 지울 얼굴의 열쇠. (FontFaceDescriptors 의 source_key)
	///
	/// # 돌려줌
	/// 지운 얼굴이 있으면 true.
	//==============================================================================
	pub fn unregister_font_face(&mut self, family_name: &str, source_key: &str) -> bool {
		let family_key = create_family_key(family_name);
		let mut removed_descriptor_list: Vec<FontFaceDescriptors> = Vec::new();
		if let Some(face_list) = self.font_family_table.get_mut(&family_key) {
			face_list.retain(|face| {
				if face.descriptors.source_key == source_key {
					removed_descriptor_list.push(face.descriptors.clone());
					return false;
				}
				true
			});
		}
		for removed_descriptors in &removed_descriptor_list {
			self.invalidate_entries_for_face(&family_key, removed_descriptors);
		}
		if removed_descriptor_list.is_empty() {
			return false;
		}
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		true
	}

	//==============================================================================
	/// 한 이름의 얼굴 차례 정하기. (얼굴 열쇠 목록, 뒤가 먼저 그린다. 원본 uifont 가 document.fonts 에서 빼고 차례대로 다시 더하는 것)
	/// - 목록에 없는 얼굴은 목록의 얼굴보다 뒤로(나중에 그리게) 밀리지 않고 앞에 남는다. 아직 받지 않은 얼굴의 열쇠도 넣어 둘 수 있다. (받으면 그 자리에 놓인다)
	/// - 차례가 바뀌면 그 이름으로 구운 문자열을 모두 버린다.
	///
	/// # 인자
	/// - `family_name`: 이름. (앞뒤 빈칸은 떼고 대소문자는 가리지 않는다)
	/// - `source_key_list`: 얼굴 열쇠 목록. (뒤가 먼저 그린다)
	//==============================================================================
	pub fn set_font_face_order(&mut self, family_name: &str, source_key_list: &[String]) {
		let family_key = create_family_key(family_name);
		self.font_face_order_table.insert(family_key.clone(), source_key_list.to_vec());
		self.apply_font_face_order(&family_key);
		let all_descriptors = FontFaceDescriptors::new();
		self.invalidate_entries_for_face(&family_key, &all_descriptors);
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	/// 정한 차례대로 얼굴의 차례 번호 다시 매기기.
	//==============================================================================
	fn apply_font_face_order(&mut self, family_key: &str) {
		let source_key_list = match self.font_face_order_table.get(family_key) {
			Some(source_key_list) => {
				source_key_list
			}
			None => {
				return;
			}
		};
		let face_list = match self.font_family_table.get_mut(family_key) {
			Some(face_list) => {
				face_list
			}
			None => {
				return;
			}
		};
		// 목록에 있는 얼굴은 목록 차례 뒤로, 없는 얼굴은 지금 차례 그대로 앞에.
		face_list.sort_by_key(|face| {
			match source_key_list.iter().position(|source_key| *source_key == face.descriptors.source_key) {
				Some(position) => {
					(1, position as u64, face.order_number)
				}
				None => {
					(0, 0, face.order_number)
				}
			}
		});
		for (face_index, face) in face_list.iter_mut().enumerate() {
			face.order_number = face_index as u64 + 1;
		}
		self.face_order_number = self.face_order_number.max(face_list.len() as u64);
	}

	//==============================================================================
	/// 얼굴이 바뀐 이름으로 구운 문자열 버리기. (그 얼굴의 글자 범위에 든 글자가 있는 것만, 텍스처는 다음 굽기 때 지운다)
	//==============================================================================
	fn invalidate_entries_for_face(&mut self, family_key: &str, font_face_descriptors: &FontFaceDescriptors) {
		let pending_deleted_texture_list = &mut self.pending_deleted_texture_list;
		let mut removed_count = 0;
		self.entry_table.retain(|_, cached_text_entry_list| {
			cached_text_entry_list.retain(|cached_text_entry| {
				let font_description = parse_font_string(&cached_text_entry.font_string);
				let is_affected = font_description.family_name_list.iter().any(|family_name| read_registered_family_key(family_name).as_deref() == Some(family_key)) && font_face_descriptors.contains_any_character(&cached_text_entry.text);
				if is_affected {
					pending_deleted_texture_list.push(cached_text_entry.entry.texture);
					removed_count += 1;
				}
				!is_affected
			});
			!cached_text_entry_list.is_empty()
		});
		self.entry_count -= removed_count;
	}

	//==============================================================================
	/// 구운 문자열 모두 버리기. (원본 clear, 텍스처는 다음 굽기 때 지운다)
	//==============================================================================
	pub fn clear(&mut self) {
		for (_, cached_text_entry_list) in self.entry_table.drain() {
			for cached_text_entry in cached_text_entry_list {
				self.pending_deleted_texture_list.push(cached_text_entry.entry.texture);
			}
		}
		self.entry_count = 0;
	}

	//==============================================================================
	/// 기본 폰트 설정. (이름을 찾지 못한 폰트 문자열과 글자 대체에 쓴다, 굵으면 굵기 700 얼굴)
	///
	/// # 인자
	/// - `is_bold`: true 면 굵기 700 얼굴, false 면 400 얼굴. (같은 굵기의 기울이지 않은 얼굴은 바꾼다)
	/// - `font_bytes`: 글꼴 파일 바이트. (ttf, otf, ttc 는 0 번 얼굴, woff, woff2 는 풀어서 읽는다)
	///
	/// # 돌려줌
	/// 설정했으면 Ok. (구운 문자열은 모두 버린다) 글꼴을 읽지 못하면 Err(까닭).
	//==============================================================================
	pub fn set_default_font(&mut self, is_bold: bool, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(load_font(font_bytes)?);
		let font_face_descriptors = FontFaceDescriptors::create_with_bold(is_bold);
		self.default_face_list.retain(|face| face.descriptors.is_italic || face.descriptors.minimum_weight != font_face_descriptors.minimum_weight);
		self.face_order_number += 1;
		self.default_face_list.push(RegisteredFontFace {
			descriptors: font_face_descriptors,
			font,
			order_number: self.face_order_number,
		});
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	/// 대체 폰트 추가. (앞 폰트들에 없는 글자를 찾는다)
	///
	/// # 인자
	/// - `font_bytes`: 글꼴 파일 바이트. (ttf, otf, ttc 는 0 번 얼굴, woff, woff2 는 풀어서 읽는다)
	///
	/// # 돌려줌
	/// 더했으면 Ok. (구운 문자열은 모두 버린다) 글꼴을 읽지 못하면 Err(까닭).
	//==============================================================================
	pub fn add_fallback_font(&mut self, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(load_font(font_bytes)?);
		self.face_order_number += 1;
		self.fallback_face_list.borrow_mut().push(RegisteredFontFace {
			descriptors: FontFaceDescriptors { is_system_fallback: true, ..FontFaceDescriptors::new() },
			font,
			order_number: self.face_order_number,
		});
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	/// 글꼴 목록을 Graphic 없이 재기에 알리기. (measure_text_without_graphic)
	//==============================================================================
	fn publish_shared_font_set(&self) {
		let mut shared_measure_cache = TextStringTextureCache::new();
		shared_measure_cache.font_family_table = self.font_family_table.clone();
		shared_measure_cache.font_face_order_table = self.font_face_order_table.clone();
		shared_measure_cache.default_face_list = self.default_face_list.clone();
		shared_measure_cache.fallback_face_list = RefCell::new(self.fallback_face_list.borrow().clone());
		shared_measure_cache.fallback_font_candidate_list = RefCell::new(self.fallback_font_candidate_list.borrow().clone());
		shared_measure_cache.system_font_state = self.system_font_state.clone();
		SHARED_MEASURE_CACHE.with(|shared_measure_cache_cell| {
			*shared_measure_cache_cell.borrow_mut() = Some(shared_measure_cache);
		});
		#[cfg(target_arch = "wasm32")]
		self.sync_browser_font_faces();
	}

	//==============================================================================
	/// 올린 글꼴 얼굴을 문서의 FontFace 로 맞추기. (웹, 이름의 얼굴은 그 이름으로, 기본 폰트는 VanillaEngineDefault, 대체 폰트는 VanillaEngineFallback1, 2 …)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	fn sync_browser_font_faces(&self) {
		let mut request_list: Vec<(String, FontFaceDescriptors, Rc<LoadedFont>)> = Vec::new();
		let mut family_key_list: Vec<&String> = self.font_family_table.keys().collect();
		family_key_list.sort();
		for family_key in family_key_list {
			let mut face_list: Vec<&RegisteredFontFace> = self.font_family_table[family_key].iter().collect();
			face_list.sort_by_key(|face| face.order_number);
			for face in face_list {
				request_list.push((family_key.clone(), face.descriptors.clone(), face.font.clone()));
			}
		}
		let mut extra_family_list: Vec<String> = Vec::new();
		if !self.default_face_list.is_empty() {
			extra_family_list.push(String::from("VanillaEngineDefault"));
			for face in self.default_face_list.iter() {
				request_list.push((String::from("VanillaEngineDefault"), face.descriptors.clone(), face.font.clone()));
			}
		}
		for (fallback_index, face) in self.fallback_face_list.borrow().iter().enumerate() {
			let family_name = format!("VanillaEngineFallback{}", fallback_index + 1);
			request_list.push((family_name.clone(), face.descriptors.clone(), face.font.clone()));
			extra_family_list.push(family_name);
		}
		crate::platform::web::systemfont::sync_font_faces(&request_list, extra_family_list);
	}

	//==============================================================================
	/// 글자 안티앨리어싱 켬 / 끔. (원본 setTextAntialiasEnabled, 바꾼 다음 그리는 글자부터 그 방식으로 굽는다)
	///
	/// # 인자
	/// - `value`: true 면 켠다. (기본 켬)
	//==============================================================================
	pub fn set_text_antialias_enabled(&mut self, value: bool) {
		self.is_text_antialias_enabled = value;
	}

	//==============================================================================
	/// 글자 안티앨리어싱이 켜져 있는지. (원본 isTextAntialiasOn)
	///
	/// # 돌려줌
	/// 켜져 있으면 true.
	//==============================================================================
	pub fn is_text_antialias_on(&self) -> bool {
		self.is_text_antialias_enabled
	}

	//==============================================================================
	/// 글꼴 얼굴을 새로 올리면 그 얼굴의 글자로 구운 문자열을 버릴지 설정. (원본 EngineConfiguration.rebakeTextOnFontLoad, 엔진이 그래픽을 넘겨받을 때 정한다)
	///
	/// # 인자
	/// - `is_rebake_text_on_font_load`: true 면 버린다. (기본 끔)
	//==============================================================================
	pub fn set_rebake_text_on_font_load(&mut self, is_rebake_text_on_font_load: bool) {
		self.is_rebake_text_on_font_load = is_rebake_text_on_font_load;
	}

	//==============================================================================
	/// 글꼴을 등록한 횟수. (등록, 지우기, 차례, 기본 글꼴, 대체 글꼴. 재 둔 글 폭을 다시 잴지 가린다)
	///
	/// # 돌려줌
	/// 등록한 횟수. (new 뒤 0 에서 늘기만 한다)
	//==============================================================================
	pub fn get_font_registration_count(&self) -> u64 {
		self.font_registration_count.get()
	}

	//==============================================================================
	/// 시스템 글꼴 사슬 정하기. (데스크탑, 원본 SYSTEM_FONT_STRING 처럼 기본 글꼴 이름 default 를 청한 폰트 문자열에서 기본 폰트 뒤에 이름 차례로 찾는다)
	/// - 켜면 폰트 문자열의 올리지 않은 이름도 운영체제 글꼴에서 찾는다. ("16px Arial" 이 시스템의 Arial 로 그려진다, 브라우저와 같다)
	/// - 얼굴은 그 굵기와 기울기의 글자를 처음 그리거나 잴 때 읽는다.
	///
	/// # 인자
	/// - `family_name_list`: 기본 폰트 뒤에 찾을 이름 차례. (대소문자는 가리지 않는다)
	//==============================================================================
	pub fn set_system_font_chain(&mut self, family_name_list: Vec<String>) {
		{
			let mut system_font_state = self.system_font_state.borrow_mut();
			system_font_state.is_lookup_enabled = true;
			system_font_state.chain_name_list = family_name_list.iter().map(|family_name| family_name.trim().to_lowercase()).collect();
		}
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	/// 시스템 글꼴 이름에 얼굴 후보 파일 더하기. (데스크탑, 그 이름을 사슬 맨 뒤에 둔다. 운영체제에서 찾지 않고 파일을 주는 경우)
	/// - 얼굴 번호가 없으면 PostScript 이름, 그것도 없으면 청한 굵기와 기울기로 모음 안의 얼굴을 고른다. 가변 글꼴은 굵기를 축 값으로 만든다.
	///
	/// # 인자
	/// - `family_name`: 시스템 글꼴 이름. (앞뒤 빈칸은 떼고 대소문자는 가리지 않는다)
	/// - `weight`: 이 후보의 굵기. (CSS 굵기 1 ~ 1000)
	/// - `is_italic`: 기울인 얼굴인지.
	/// - `source`: 글꼴 파일과 얼굴. (같은 파일, 굵기, 기울기의 후보는 한 번만 든다)
	//==============================================================================
	pub fn add_system_font_candidate(&mut self, family_name: &str, weight: u16, is_italic: bool, source: FontFileSource) {
		{
			let mut system_font_state = self.system_font_state.borrow_mut();
			let family_key = family_name.trim().to_lowercase();
			if !system_font_state.chain_name_list.contains(&family_key) {
				system_font_state.chain_name_list.push(family_key.clone());
			}
			let system_font_family = system_font_state.family_table.entry(family_key).or_insert_with(|| Some(SystemFontFamily::default())).get_or_insert_with(SystemFontFamily::default);
			if !system_font_family.candidate_list.iter().any(|candidate| candidate.source == source && candidate.weight == weight && candidate.is_italic == is_italic) {
				system_font_family.candidate_list.push(SystemFontCandidate {
					weight,
					is_italic,
					source,
					is_loaded: false,
				});
			}
		}
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	/// 운영체제 대체 글꼴 켬 / 끔. (데스크탑, 켜면 어느 글꼴에도 없는 글자를 운영체제가 고른 글꼴로 그린다. 대체 글꼴 후보 파일이 있으면 그것이 먼저다)
	///
	/// # 인자
	/// - `value`: true 면 켠다. (기본 끔)
	//==============================================================================
	pub fn set_system_font_fallback_enabled(&mut self, value: bool) {
		self.system_font_state.borrow_mut().is_fallback_enabled = value;
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	/// 시스템 글꼴 사슬이나 후보가 있는지. (데스크탑)
	///
	/// # 돌려줌
	/// 사슬에 찾은 이름이나 후보가 있는 이름이 있거나, 사슬을 켰고 이름이 하나라도 있으면 true.
	//==============================================================================
	pub fn has_system_font(&self) -> bool {
		let system_font_state = self.system_font_state.borrow();
		system_font_state.chain_name_list.iter().any(|family_key| matches!(system_font_state.family_table.get(family_key), Some(Some(_)))) || (system_font_state.is_lookup_enabled && !system_font_state.chain_name_list.is_empty())
	}

	//==============================================================================
	/// 시스템 글꼴 이름의 얼굴들을 고른 목록에 더하기. (처음 보는 이름이면 운영체제에서 찾고, 청한 굵기와 기울기에 맞는 후보를 읽는다, 없으면 false)
	//==============================================================================
	fn append_system_family_faces(&self, family_key: &str, desired_weight: u16, is_italic_desired: bool, output_list: &mut Vec<ResolvedFontFace>) -> bool {
		let mut system_font_state = self.system_font_state.borrow_mut();
		if !system_font_state.family_table.contains_key(family_key) {
			if !system_font_state.is_lookup_enabled {
				return false;
			}
			let system_font_family = find_system_font_family(family_key);
			system_font_state.family_table.insert(family_key.to_string(), system_font_family);
		}
		let state = &mut *system_font_state;
		let system_font_family = match state.family_table.get_mut(family_key) {
			Some(Some(system_font_family)) => {
				system_font_family
			}
			_ => {
				return false;
			}
		};
		// 청한 기울기를 먼저, 그다음 굵기로 가장 맞는 후보를 아직 읽지 않았으면 읽는다.
		let best_candidate_index = (0..system_font_family.candidate_list.len()).min_by_key(|candidate_index| {
			let candidate = &system_font_family.candidate_list[*candidate_index];
			(candidate.is_italic != is_italic_desired, textshaping::calculate_weight_score(candidate.weight, candidate.weight, desired_weight))
		});
		if let Some(candidate_index) = best_candidate_index {
			if !system_font_family.candidate_list[candidate_index].is_loaded {
				system_font_family.candidate_list[candidate_index].is_loaded = true;
				let candidate = &system_font_family.candidate_list[candidate_index];
				match load_system_font_face(&mut state.shared_bytes_table, &candidate.source, candidate.weight, candidate.is_italic) {
					Ok(registered_font_face) => {
						let is_duplicate = system_font_family.face_list.iter().any(|face| Rc::ptr_eq(face.font.get_font_bytes(), registered_font_face.font.get_font_bytes()) && face.font.get_face_index() == registered_font_face.font.get_face_index() && face.descriptors == registered_font_face.descriptors);
						if !is_duplicate {
							system_font_family.face_list.push(registered_font_face);
						}
					}
					Err(error) => {
						eprintln!("TextStringTextureCache: {} ({})", error, candidate.source.path.display());
					}
				}
			}
		}
		// 맥 시스템 UI 글꼴은 청한 굵기를 그대로 wght 축 값으로 건 인스턴스를 따로 읽는다. (크롬 font_matcher_mac.mm 의 MatchSystemUIFont, 400 과 700 후보만으로는 300 이 400 으로 그려진다)
		if cfg!(target_os = "macos") && matches!(family_key, "system-ui" | "blinkmacsystemfont") {
			if let Some(candidate_index) = best_candidate_index {
				let candidate = &system_font_family.candidate_list[candidate_index];
				let weight_axis_range = system_font_family.face_list.iter().find(|face| face.descriptors.is_italic == candidate.is_italic).and_then(|face| textshaping::read_weight_axis_range(face.font.get_font_bytes(), face.font.get_face_index()));
				if let Some((minimum_weight, maximum_weight)) = weight_axis_range {
					let instance_weight = desired_weight.clamp(minimum_weight, maximum_weight);
					let has_instance = system_font_family.face_list.iter().any(|face| face.descriptors.is_italic == candidate.is_italic && face.descriptors.minimum_weight == instance_weight);
					if !has_instance {
						if let Ok(registered_font_face) = load_system_font_face(&mut state.shared_bytes_table, &candidate.source, instance_weight, candidate.is_italic) {
							system_font_family.face_list.push(registered_font_face);
						}
					}
				}
			}
		}
		if system_font_family.face_list.is_empty() {
			return false;
		}
		let face_reference_list: Vec<&RegisteredFontFace> = system_font_family.face_list.iter().collect();
		textshaping::select_font_faces(&face_reference_list, desired_weight, is_italic_desired, output_list);
		true
	}

	//==============================================================================
	/// 운영체제 대체 글꼴을 물을 바탕 읽기. (첫 글꼴은 대체 단계가 아닌 얼굴 가운데 빈칸을 가진 첫 것, 크롬 FontFallbackList::PrimarySimpleFontDataWithSpace)
	//==============================================================================
	fn read_fallback_base_info(&self, font_description: &FontDescription, face_list: &[ResolvedFontFace]) -> FallbackBaseInfo {
		let first_family_name = font_description.family_name_list.first().cloned().unwrap_or_default();
		let last_generic_family = font_description.family_name_list.iter().rev().find(|family_name| matches!(family_name.to_ascii_lowercase().as_str(), "serif" | "sans-serif" | "cursive" | "fantasy" | "monospace" | "-webkit-body"));
		let is_monospace = last_generic_family.is_some_and(|family_name| family_name.eq_ignore_ascii_case("monospace"));
		let primary_face = face_list.iter().find(|face| !face.descriptors.is_system_fallback && face.descriptors.contains_character(' '));
		let mut primary_family_name: Option<String> = None;
		let mut primary_font: Option<Rc<LoadedFont>> = None;
		if let Some(primary_face) = primary_face {
			let system_font_state = self.system_font_state.borrow();
			primary_family_name = system_font_state.family_table.iter().find(|(_, system_font_family)| system_font_family.as_ref().is_some_and(|system_font_family| system_font_family.face_list.iter().any(|face| Rc::ptr_eq(&face.font, &primary_face.font)))).map(|(family_key, _)| family_key.clone());
			if primary_family_name.is_none() {
				primary_font = Some(primary_face.font.clone());
			}
		}
		let primary_identity = match (&primary_family_name, &primary_font) {
			(Some(family_name), _) => {
				family_name.clone()
			}
			(None, Some(font)) => {
				format!("@{:p}", Rc::as_ptr(font))
			}
			(None, None) => {
				String::new()
			}
		};
		// 대체가 무엇에 따라 달라지는지가 운영체제마다 다르다. (맥은 첫 글꼴, 윈도우는 폰트 문자열의 첫 이름과 monospace, 리눅스는 글자만. 같은 답을 다시 묻지 않는다)
		let key = if cfg!(target_os = "macos") {
			primary_identity
		}
		else if cfg!(target_os = "windows") {
			format!("{}\u{1f}{}", first_family_name.to_lowercase(), is_monospace)
		}
		else {
			String::new()
		};
		FallbackBaseInfo {
			key,
			primary_family_name,
			primary_font,
			first_family_name,
			is_monospace,
		}
	}

	//==============================================================================
	/// 글의 글자가 운영체제 대체 글꼴로 그려지게 하기. (어느 얼굴에도 없는 글자를 운영체제에 묻는다, 같은 물음은 한 번)
	/// - 굵게를 청했는데 그 글자를 가진 대체 얼굴이 모두 600 아래면 굵은 대체 얼굴을 따로 묻는다. (크롬도 굵기를 주고 대체 글꼴을 고른다)
	//==============================================================================
	fn ensure_system_fallback_coverage(&self, text: &str, font_description: &FontDescription, resolved_face_list: &[ResolvedFontFace]) {
		if !self.system_font_state.borrow().is_fallback_enabled {
			return;
		}
		let is_bold_desired = font_description.font_weight >= textshaping::BOLD_WEIGHT_THRESHOLD;
		let fallback_base_info = self.read_fallback_base_info(font_description, resolved_face_list);
		for grapheme in text.graphemes(true) {
			let is_emoji_grapheme = textshaping::is_emoji_presentation(grapheme);
			let character = match grapheme.chars().next() {
				Some(character) => {
					character
				}
				None => {
					continue;
				}
			};
			if character.is_whitespace() || character.is_control() || ('\u{200B}'..='\u{200F}').contains(&character) || ('\u{2028}'..='\u{206F}').contains(&character) || ('\u{FE00}'..='\u{FE0F}').contains(&character) {
				continue;
			}
			let is_covered_by_face = |face: &RegisteredFontFace| -> bool {
				if is_emoji_grapheme {
					face.font.has_color_character(character)
				}
				else {
					face.font.has_character(character) && !face.font.has_color_character(character)
				}
			};
			// 이름 목록의 얼굴이 그리면 묻지 않는다. (정준 분해로 그리는 것도, 운영체제 대체 얼굴은 아래에서 굵기까지 본다)
			let is_resolved_covered = resolved_face_list.iter().any(|face| {
				if face.descriptors.is_system_fallback {
					return false;
				}
				let has_character = if is_emoji_grapheme {
					face.font.has_color_character(character)
				}
				else {
					textshaping::has_character_or_decomposition(&face.font, character) && !face.font.has_color_character(character)
				};
				face.descriptors.contains_character(character) && has_character
			});
			if is_resolved_covered {
				continue;
			}
			let query_key = (fallback_base_info.key.clone(), character, is_bold_desired, font_description.is_italic, is_emoji_grapheme);
			{
				let system_font_state = self.system_font_state.borrow();
				if system_font_state.fallback_query_set.contains(&query_key) {
					continue;
				}
				let covering_face_list: Vec<&RegisteredFontFace> = system_font_state.fallback_face_list.iter().zip(system_font_state.fallback_base_key_list.iter()).filter(|(face, base_key)| **base_key == fallback_base_info.key && face.descriptors.contains_character(character) && is_covered_by_face(face)).map(|(face, _)| face).collect();
				if !covering_face_list.is_empty() && (!is_bold_desired || covering_face_list.iter().any(|face| face.descriptors.maximum_weight >= textshaping::BOLD_WEIGHT_THRESHOLD)) {
					continue;
				}
			}
			self.system_font_state.borrow_mut().fallback_query_set.insert(query_key);
			let source = match find_system_fallback_font(grapheme, font_description.font_weight, font_description.is_italic, is_emoji_grapheme, &fallback_base_info) {
				Some(source) => {
					source
				}
				None => {
					continue;
				}
			};
			// 대체 얼굴은 운영체제가 그 얼굴을 고른 글자만 그린다. (글자 범위로 든다, 크롬도 글자마다 대체를 묻고 다른 글자를 위해 고른 글꼴을 넘겨 쓰지 않는다)
			let grapheme_range_list: Vec<(u32, u32)> = grapheme.chars().map(|grapheme_character| (grapheme_character as u32, grapheme_character as u32)).collect();
			let mut system_font_state = self.system_font_state.borrow_mut();
			let state = &mut *system_font_state;
			let add_range_list = |face: &mut RegisteredFontFace, range_list: &[(u32, u32)]| {
				for range in range_list {
					if !face.descriptors.unicode_range_list.contains(range) {
						face.descriptors.unicode_range_list.push(*range);
					}
				}
			};
			let memo_position = state.fallback_source_memo_list.iter().find(|(base_key, memo_source, weight, is_italic, _)| *base_key == fallback_base_info.key && *memo_source == source && *weight == font_description.font_weight && *is_italic == font_description.is_italic).map(|memo| memo.4);
			if let Some(face_position) = memo_position {
				add_range_list(&mut state.fallback_face_list[face_position], &grapheme_range_list);
				continue;
			}
			match load_system_font_face(&mut state.shared_bytes_table, &source, font_description.font_weight, font_description.is_italic) {
				Ok(mut registered_font_face) => {
					registered_font_face.descriptors.source_key = read_fallback_source_key(&source, &registered_font_face);
					registered_font_face.descriptors.is_system_fallback = true;
					// 같은 바탕에서 이미 읽은 같은 얼굴이면 그 얼굴의 글자 범위에 더한다.
					let loaded_position = state.fallback_face_list.iter().zip(state.fallback_base_key_list.iter()).position(|(face, base_key)| {
						*base_key == fallback_base_info.key && Rc::ptr_eq(face.font.get_font_bytes(), registered_font_face.font.get_font_bytes()) && face.font.get_face_index() == registered_font_face.font.get_face_index() && face.descriptors.minimum_weight == registered_font_face.descriptors.minimum_weight && face.descriptors.maximum_weight == registered_font_face.descriptors.maximum_weight && face.descriptors.is_italic == registered_font_face.descriptors.is_italic
					});
					let face_position = match loaded_position {
						Some(face_position) => {
							add_range_list(&mut state.fallback_face_list[face_position], &grapheme_range_list);
							face_position
						}
						None => {
							registered_font_face.descriptors.unicode_range_list = grapheme_range_list;
							state.fallback_face_list.push(registered_font_face);
							state.fallback_base_key_list.push(fallback_base_info.key.clone());
							state.fallback_face_list.len() - 1
						}
					};
					state.fallback_source_memo_list.push((fallback_base_info.key.clone(), source, font_description.font_weight, font_description.is_italic, face_position));
				}
				Err(error) => {
					eprintln!("TextStringTextureCache: {} ({})", error, source.path.display());
				}
			}
		}
	}

	//==============================================================================
	/// 대체 글꼴 후보 파일 더하기. (데스크탑, 원본 브라우저가 글꼴에 없는 글자를 시스템 글꼴로 그리는 자리)
	/// - 바로 읽지 않는다. 등록된 어느 글꼴에도 없는 글자를 재거나 구울 때 앞의 후보부터 읽어 대체 폰트로 더한다. (큰 한중일 글꼴을 처음에 읽지 않게)
	/// - 없는 파일, 읽지 못하는 파일은 넘긴다.
	///
	/// # 인자
	/// - `font_path`: 글꼴 파일 경로. (더할 때 색 글리프 표가 있는지만 엿본다)
	//==============================================================================
	pub fn add_fallback_font_candidate(&mut self, font_path: std::path::PathBuf) {
		let has_color_tables = peek_color_font_tables(&font_path);
		self.fallback_font_candidate_list.borrow_mut().push((font_path, has_color_tables));
	}

	//==============================================================================
	/// 글의 글자가 모두 그려지게 대체 글꼴 후보 읽기. (어느 글꼴에도 없는 글자가 있으면 그 글자를 가진 후보가 나올 때까지)
	/// - 이모지 표시 글자 묶음의 첫 글자는 색 글리프로, 그 밖의 글자는 색이 아닌 글리프로 가진 글꼴을 찾는다. (브라우저가 이모지를 색 이모지 글꼴로, 맨 ❤ 는 글 글꼴로 그리는 자리)
	///   바라는 쪽의 후보가 다 떨어졌으면 다른 쪽이라도 가진 글꼴이 있으면 그것으로 그린다.
	//==============================================================================
	fn ensure_fallback_coverage(&self, text: &str) {
		if self.fallback_font_candidate_list.borrow().is_empty() {
			return;
		}
		for grapheme in text.graphemes(true) {
			let is_emoji_grapheme = textshaping::is_emoji_presentation(grapheme);
			for (character_index, character) in grapheme.chars().enumerate() {
				if character.is_whitespace() || character.is_control() || ('\u{200B}'..='\u{200F}').contains(&character) || ('\u{2028}'..='\u{206F}').contains(&character) || ('\u{FE00}'..='\u{FE0F}').contains(&character) {
					continue;
				}
				let is_color_wanted = is_emoji_grapheme && character_index == 0;
				loop {
					let is_wanted_covered = if is_color_wanted {
						self.is_color_character_covered(character)
					}
					else {
						self.is_text_character_covered(character)
					};
					if is_wanted_covered {
						break;
					}
					let has_wanted_candidate = self.fallback_font_candidate_list.borrow().iter().any(|(_, has_color_tables)| *has_color_tables == is_color_wanted);
					if !has_wanted_candidate && self.is_character_covered(character) {
						break;
					}
					if !self.load_next_fallback_candidate(is_color_wanted) {
						return;
					}
				}
			}
		}
	}

	//==============================================================================
	/// 대체 글꼴 후보 하나 읽어 대체 폰트로 더하기. (색을 바라면 색 글리프 표가 있는 후보, 아니면 없는 후보부터. 후보가 없으면 false)
	/// - 못 읽는 파일은 버리고 다음 후보를 본다.
	//==============================================================================
	fn load_next_fallback_candidate(&self, is_color_wanted: bool) -> bool {
		loop {
			let font_path = {
				let mut fallback_font_candidate_list = self.fallback_font_candidate_list.borrow_mut();
				if fallback_font_candidate_list.is_empty() {
					return false;
				}
				let preferred_index = fallback_font_candidate_list.iter().position(|(_, has_color_tables)| *has_color_tables == is_color_wanted).unwrap_or(0);
				fallback_font_candidate_list.remove(preferred_index).0
			};
			let font = match std::fs::read(&font_path).map_err(|error| error.to_string()).and_then(load_font) {
				Ok(font) => {
					Rc::new(font.with_source_path(font_path.clone()))
				}
				Err(_) => {
					continue;
				}
			};
			self.fallback_face_list.borrow_mut().push(RegisteredFontFace {
				descriptors: FontFaceDescriptors { is_system_fallback: true, ..FontFaceDescriptors::new() },
				font,
				order_number: 0,
			});
			self.font_registration_count.set(self.font_registration_count.get() + 1);
			self.publish_shared_font_set();
			return true;
		}
	}

	//==============================================================================
	/// 등록된 어느 글꼴이 그 글자를 가졌는지.
	//==============================================================================
	fn is_character_covered(&self, character: char) -> bool {
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && face.font.has_character(character)));
		is_in_family || self.default_face_list.iter().any(|face| face.font.has_character(character)) || self.fallback_face_list.borrow().iter().any(|face| face.font.has_character(character)) || self.is_system_character_covered(&|face| face.font.has_character(character))
	}

	//==============================================================================
	/// 읽은 시스템 글꼴 얼굴(사슬, 운영체제 대체 글꼴) 가운데 조건에 맞는 것이 있는지.
	//==============================================================================
	fn is_system_character_covered(&self, is_covered: &dyn Fn(&RegisteredFontFace) -> bool) -> bool {
		let system_font_state = self.system_font_state.borrow();
		let is_in_chain = system_font_state.chain_name_list.iter().any(|family_key| match system_font_state.family_table.get(family_key) {
			Some(Some(system_font_family)) => {
				system_font_family.face_list.iter().any(is_covered)
			}
			_ => {
				false
			}
		});
		is_in_chain || system_font_state.fallback_face_list.iter().any(is_covered)
	}

	//==============================================================================
	/// 등록된 어느 글꼴이 그 글자를 색이 아닌 글리프로 가졌는지.
	//==============================================================================
	fn is_text_character_covered(&self, character: char) -> bool {
		let has_text_character = |face: &RegisteredFontFace| face.font.has_character(character) && !face.font.has_color_character(character);
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && has_text_character(face)));
		is_in_family || self.default_face_list.iter().any(has_text_character) || self.fallback_face_list.borrow().iter().any(has_text_character) || self.is_system_character_covered(&has_text_character)
	}

	//==============================================================================
	/// 등록된 어느 글꼴이 그 글자를 색 글리프로 가졌는지.
	//==============================================================================
	fn is_color_character_covered(&self, character: char) -> bool {
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && face.font.has_color_character(character)));
		is_in_family || self.default_face_list.iter().any(|face| face.font.has_color_character(character)) || self.fallback_face_list.borrow().iter().any(|face| face.font.has_color_character(character)) || self.is_system_character_covered(&|face| face.font.has_color_character(character))
	}

	//==============================================================================
	/// 기본 폰트가 있는지.
	///
	/// # 돌려줌
	/// 기본 폰트 얼굴이 하나라도 있으면 true.
	//==============================================================================
	pub fn has_default_font(&self) -> bool {
		!self.default_face_list.is_empty()
	}

	//==============================================================================
	/// 이름에 올린 얼굴 수. (그 이름이 없으면 0)
	///
	/// # 인자
	/// - `family_name`: 이름. (앞뒤 빈칸은 떼고 대소문자는 가리지 않는다)
	///
	/// # 돌려줌
	/// 얼굴 수.
	//==============================================================================
	pub fn get_font_face_count(&self, family_name: &str) -> usize {
		self.font_family_table.get(&create_family_key(family_name)).map(|face_list| face_list.len()).unwrap_or(0)
	}

	//==============================================================================
	/// 문자열 측정. (문단 방향은 왼쪽에서 오른쪽, 원본 measureTextMetrics 도 방향을 주지 않는다)
	///
	/// # 인자
	/// - `font_string`: 폰트 문자열. (Canvas2D font 꼴, parse_font_string)
	/// - `text`: 잴 글.
	///
	/// # 돌려줌
	/// 잰 값. (픽셀) 데스크탑에서 그릴 얼굴이 하나도 없으면 모두 0 이다.
	///
	/// # 예제
	/// 같은 글은 한 번만 잰다
	/// ```rust
	/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
	/// use vanilla::platform::find_fallback_font_file;
	///
	/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
	/// 	let font_bytes = std::fs::read(&source.path).expect("font file");
	/// 	let mut text_string_texture_cache = TextStringTextureCache::new();
	/// 	text_string_texture_cache.set_default_font(false, font_bytes).expect("font");
	/// 	let small_metrics = text_string_texture_cache.measure_text("20px default", "abc");
	/// 	text_string_texture_cache.measure_text("20px default", "abc");
	/// 	assert_eq!(text_string_texture_cache.get_measure_count(), 1);
	/// 	let large_metrics = text_string_texture_cache.measure_text("40px default", "abc");
	/// 	assert!(large_metrics.width > small_metrics.width);
	/// 	assert_eq!(text_string_texture_cache.get_measure_count(), 2);
	/// }
	/// ```
	//==============================================================================
	pub fn measure_text(&self, font_string: &str, text: &str) -> TextMetrics {
		// 글꼴이 새로 들어왔으면 잰 것을 비운다.
		if self.measured_font_registration_count.get() != self.font_registration_count.get() {
			self.measure_table.borrow_mut().clear();
			self.measure_count.set(0);
			self.measured_font_registration_count.set(self.font_registration_count.get());
		}
		let use_count = self.measure_use_count.get() + 1;
		self.measure_use_count.set(use_count);
		let mut hasher = DefaultHasher::new();
		font_string.hash(&mut hasher);
		text.hash(&mut hasher);
		let measure_key = hasher.finish();
		if let Some(cached_list) = self.measure_table.borrow_mut().get_mut(&measure_key) {
			for cached in cached_list.iter_mut() {
				if cached.font_string == font_string && cached.text == text {
					cached.last_used_count = use_count;
					return cached.text_metrics;
				}
			}
		}
		let text_metrics = self.measure_text_uncached(font_string, text);
		if self.measure_count.get() >= MAXIMUM_MEASURE_COUNT {
			self.evict_old_measures();
		}
		self.measure_table.borrow_mut().entry(measure_key).or_default().push(CachedTextMetrics {
			font_string: font_string.to_string(),
			text: text.to_string(),
			text_metrics,
			last_used_count: use_count,
		});
		self.measure_count.set(self.measure_count.get() + 1);
		text_metrics
	}

	//==============================================================================
	/// 잰 것 가운데 오래 안 쓴 반 버리기. (쓴 번호가 가운데보다 작은 것)
	//==============================================================================
	fn evict_old_measures(&self) {
		let mut measure_table = self.measure_table.borrow_mut();
		let mut last_used_list: Vec<u64> = measure_table.values().flat_map(|cached_list| cached_list.iter().map(|cached| cached.last_used_count)).collect();
		if last_used_list.is_empty() {
			return;
		}
		let middle_index = last_used_list.len() / 2;
		let (_, middle_count, _) = last_used_list.select_nth_unstable(middle_index);
		let threshold_count = *middle_count;
		let mut remain_count = 0;
		measure_table.retain(|_, cached_list| {
			cached_list.retain(|cached| cached.last_used_count >= threshold_count);
			remain_count += cached_list.len();
			!cached_list.is_empty()
		});
		self.measure_count.set(remain_count);
	}

	//==============================================================================
	/// 잰 글 수. (캐시에 든 것, 시험과 통계용)
	///
	/// # 돌려줌
	/// 캐시에 든 잰 글 수. (넘치면 반을 버린다, 상한 2048)
	//==============================================================================
	pub fn get_measure_count(&self) -> usize {
		self.measure_count.get()
	}

	//==============================================================================
	/// 문자열 측정. (캐시를 거치지 않는다)
	//==============================================================================
	fn measure_text_uncached(&self, font_string: &str, text: &str) -> TextMetrics {
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(text, &font_description);
		if face_list.is_empty() || self.is_browser_raster {
			// 웹: 원본처럼 브라우저 Canvas2D 로 잰다. (올린 글꼴은 document.fonts 의 FontFace, 없으면 시스템 글꼴)
			#[cfg(target_arch = "wasm32")]
			{
				return crate::platform::web::systemfont::measure_text(&font_description, text);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return TextMetrics::default();
			}
		}
		let font_size = font_description.font_size;
		let system_font_measure = create_system_font_measure(&font_description, font_size);
		let shaped_text = textshaping::shape_text(&face_list, text, font_size, ShapingDirection::LeftToRight, system_font_measure.as_deref());
		let (font_ascent, font_descent) = read_line_metrics(&face_list[0].font, font_size);
		let mut text_metrics = TextMetrics {
			width: shaped_text.advance_width,
			font_ascent,
			font_descent,
			actual_bounding_box_left: 0.0,
			actual_bounding_box_right: 0.0,
			actual_bounding_box_ascent: 0.0,
			actual_bounding_box_descent: 0.0,
		};
		let mut is_first_glyph = true;
		for shaped_glyph in &shaped_text.glyph_list {
			if shaped_glyph.face_index == textshaping::SYSTEM_FACE_INDEX {
				continue;
			}
			let face = &face_list[shaped_glyph.face_index];
			let (glyph_left_x, glyph_bottom_y, glyph_width, glyph_height) = face.font.read_glyph_metrics(shaped_glyph.glyph_index, font_size);
			if glyph_width == 0 || glyph_height == 0 {
				continue;
			}
			let mut left_x = shaped_glyph.position_x + glyph_left_x as f32;
			let mut right_x = left_x + glyph_width as f32;
			let mut top_y = shaped_glyph.position_y + (glyph_bottom_y + glyph_height) as f32;
			let mut bottom_y = shaped_glyph.position_y + glyph_bottom_y as f32;
			if face.is_synthetic_bold {
				let radius = textshaping::calculate_synthetic_bold_radius(font_size);
				left_x -= radius;
				right_x += radius;
				top_y += radius;
				bottom_y -= radius;
			}
			if face.is_synthetic_italic {
				right_x += top_y.max(0.0) * textshaping::SYNTHETIC_ITALIC_SKEW;
				left_x += bottom_y.min(0.0) * textshaping::SYNTHETIC_ITALIC_SKEW;
			}
			if is_first_glyph {
				text_metrics.actual_bounding_box_left = -left_x;
				text_metrics.actual_bounding_box_right = right_x;
				text_metrics.actual_bounding_box_ascent = top_y;
				text_metrics.actual_bounding_box_descent = -bottom_y;
				is_first_glyph = false;
			}
			else {
				text_metrics.actual_bounding_box_left = text_metrics.actual_bounding_box_left.max(-left_x);
				text_metrics.actual_bounding_box_right = text_metrics.actual_bounding_box_right.max(right_x);
				text_metrics.actual_bounding_box_ascent = text_metrics.actual_bounding_box_ascent.max(top_y);
				text_metrics.actual_bounding_box_descent = text_metrics.actual_bounding_box_descent.max(-bottom_y);
			}
		}
		text_metrics
	}

	//==============================================================================
	/// 구운 문자열 반환. (없으면 굽는다, direction 은 문단 방향. 원본 getEntry 의 textDirection)
	///
	/// # 인자
	/// - `gl`: 텍스처를 만들 GL 컨텍스트.
	/// - `text_draw_kind`: 굽는 방식.
	/// - `text`: 구울 글.
	/// - `font_string`: 폰트 문자열. (Canvas2D font 꼴, parse_font_string)
	/// - `line_width`: 외곽선 두께. (논리 픽셀, Stroke 로 구울 때만 쓰지만 키에는 늘 든다)
	/// - `bake_scale`: 굽는 배율. (글자 크기에 곱한 크기로 굽고, TextEntry 의 길이는 이것으로 나눈 값이다)
	/// - `direction`: 문단 방향.
	///
	/// # 돌려줌
	/// 구운 문자열. (같은 키면 캐시의 것) 글이 비었거나, 데스크탑에서 그릴 얼굴이 하나도 없거나, 텍스처를 만들지 못하면 None.
	///
	/// # 예제
	/// 같은 키로 다시 부르면 굽지 않는다
	/// ```rust,no_run
	/// use vanilla::core::textstringtexturecache::{TextDrawKind, TextStringTextureCache};
	/// use vanilla::ShapingDirection;
	///
	/// fn bake_outline(text_string_texture_cache: &mut TextStringTextureCache, gl: &glow::Context) {
	/// 	let first_entry = text_string_texture_cache.get_entry(gl, TextDrawKind::Stroke, "Hello", "bold 20px default", 3.0, 1.5, ShapingDirection::LeftToRight);
	/// 	let entry_count = text_string_texture_cache.get_entry_count();
	/// 	let second_entry = text_string_texture_cache.get_entry(gl, TextDrawKind::Stroke, "Hello", "bold 20px default", 3.0, 1.5, ShapingDirection::LeftToRight);
	/// 	assert_eq!(text_string_texture_cache.get_entry_count(), entry_count);
	/// 	assert_eq!(first_entry.map(|entry| entry.texture), second_entry.map(|entry| entry.texture));
	/// }
	/// ```
	//==============================================================================
	pub fn get_entry(&mut self, gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<TextEntry> {
		if text.is_empty() {
			return None;
		}
		self.delete_pending_textures(gl);
		if let Some(entry) = self.find_entry(text_draw_kind, text, font_string, line_width, bake_scale, direction) {
			return Some(entry);
		}

		let entry = self.bake_entry(gl, text_draw_kind, text, font_string, line_width, bake_scale, direction)?;
		if self.entry_count >= MAXIMUM_ENTRY_COUNT {
			self.evict_oldest_entry(gl);
		}
		let is_antialias = self.is_text_antialias_enabled;
		let entry_hash = create_entry_hash(text_draw_kind, is_antialias, direction, text, font_string, line_width, bake_scale);
		self.entry_table.entry(entry_hash).or_default().push(CachedTextEntry {
			text_draw_kind,
			is_antialias,
			direction,
			text: text.to_string(),
			font_string: font_string.to_string(),
			line_width,
			bake_scale,
			entry,
			last_used_frame_number: self.frame_number,
		});
		self.entry_count += 1;
		Some(entry)
	}

	//==============================================================================
	/// 버리기로 한 텍스처 지우기.
	//==============================================================================
	fn delete_pending_textures(&mut self, gl: &glow::Context) {
		for texture in self.pending_deleted_texture_list.drain(..) {
			unsafe {
				gl.delete_texture(texture);
			}
		}
	}

	//==============================================================================
	/// 버리기로 한 텍스처가 있는지. (있으면 굽는 길로 가서 지운다)
	//==============================================================================
	pub(crate) fn has_pending_deleted_textures(&self) -> bool {
		!self.pending_deleted_texture_list.is_empty()
	}

	//==============================================================================
	/// 이미 구운 문자열 찾기. (GL 을 건드리지 않는다, 없으면 None)
	//==============================================================================
	pub(crate) fn find_entry(&mut self, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<TextEntry> {
		let is_antialias = self.is_text_antialias_enabled;
		let entry_hash = create_entry_hash(text_draw_kind, is_antialias, direction, text, font_string, line_width, bake_scale);
		let frame_number = self.frame_number;
		let cached_text_entry_list = self.entry_table.get_mut(&entry_hash)?;
		let cached_text_entry = cached_text_entry_list.iter_mut().find(|cached_text_entry| cached_text_entry.is_same_key(text_draw_kind, is_antialias, direction, text, font_string, line_width, bake_scale))?;
		cached_text_entry.last_used_frame_number = frame_number;
		Some(cached_text_entry.entry)
	}

	//==============================================================================
	/// 가장 오래 안 쓴 문자열 하나 버리기.
	//==============================================================================
	fn evict_oldest_entry(&mut self, gl: &glow::Context) {
		let mut oldest_position: Option<(u64, usize, u64)> = None;
		for (entry_hash, cached_text_entry_list) in &self.entry_table {
			for (entry_index, cached_text_entry) in cached_text_entry_list.iter().enumerate() {
				let is_older = match oldest_position {
					Some((_, _, oldest_frame_number)) => {
						cached_text_entry.last_used_frame_number < oldest_frame_number
					}
					None => {
						true
					}
				};
				if is_older {
					oldest_position = Some((*entry_hash, entry_index, cached_text_entry.last_used_frame_number));
				}
			}
		}
		let (entry_hash, entry_index, _) = match oldest_position {
			Some(oldest_position) => {
				oldest_position
			}
			None => {
				return;
			}
		};
		if let Some(cached_text_entry_list) = self.entry_table.get_mut(&entry_hash) {
			let cached_text_entry = cached_text_entry_list.swap_remove(entry_index);
			unsafe {
				gl.delete_texture(cached_text_entry.entry.texture);
			}
			if cached_text_entry_list.is_empty() {
				self.entry_table.remove(&entry_hash);
			}
			self.entry_count -= 1;
		}
	}

	//==============================================================================
	/// 글의 모양만 래스터로 받기. (텍스처로 올리지 않는다, 그림자를 깐 글처럼 따로 굽는 쪽이 쓴다)
	/// - 웹은 원본처럼 브라우저 Canvas2D 로 받는다. 데스크탑에서 글꼴이 하나도 없으면 None.
	///
	/// # 인자
	/// - `text`: 글.
	/// - `font_string`: 폰트 문자열. (Canvas2D font 꼴, parse_font_string)
	/// - `bake_scale`: 굽는 배율. (글자 크기에 곱한다)
	/// - `padding`: 가장자리 여백. (픽셀, 1 보다 작으면 1)
	/// - `direction`: 문단 방향.
	///
	/// # 돌려줌
	/// 래스터. 글이 비었거나 데스크탑에서 그릴 얼굴이 하나도 없으면 None.
	//==============================================================================
	pub fn rasterize_text(&self, text: &str, font_string: &str, bake_scale: f32, padding: i32, direction: ShapingDirection) -> Option<TextRaster> {
		self.rasterize_text_with_color(text, font_string, bake_scale, padding, direction, Color::white())
	}

	//==============================================================================
	/// 글자 색을 알려 글의 모양만 래스터로 받기. (rasterize_text 와 같고, 맥 데스크탑은 크롬처럼 글자 색 밝기로 덮인 정도를 감마 전보정한다)
	/// - 원본이 글 색 그대로 Canvas2D 에 굽는 쪽(그림자를 깐 글, 2D 캔버스의 fillText)이 쓴다. 흰색으로 굽고 색을 곱하는 글은 rasterize_text 와 같다.
	///
	/// # 인자
	/// - `text`: 글.
	/// - `font_string`: 폰트 문자열. (Canvas2D font 꼴, parse_font_string)
	/// - `bake_scale`: 굽는 배율. (글자 크기에 곱한다)
	/// - `padding`: 가장자리 여백. (픽셀, 1 보다 작으면 1)
	/// - `direction`: 문단 방향.
	/// - `color`: 글자 색. (알파는 쓰지 않는다, 그라데이션이나 패턴으로 칠하면 원본 스키아처럼 회색 0.5)
	///
	/// # 돌려줌
	/// 래스터. 글이 비었거나 데스크탑에서 그릴 얼굴이 하나도 없으면 None.
	///
	/// # 예제
	/// 검은 글의 덮인 정도 받기
	/// ```rust
	/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
	/// use vanilla::platform::find_fallback_font_file;
	/// use vanilla::{Color, ShapingDirection};
	///
	/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
	/// 	let mut text_string_texture_cache = TextStringTextureCache::new();
	/// 	text_string_texture_cache.set_default_font(false, std::fs::read(&source.path).expect("font file")).expect("font");
	/// 	let text_raster = text_string_texture_cache.rasterize_text_with_color("Ag", "24px default", 1.0, 2, ShapingDirection::LeftToRight, Color::black()).expect("raster");
	/// 	assert_eq!(text_raster.coverage.len(), (text_raster.width * text_raster.height) as usize);
	/// 	assert!(text_raster.coverage.iter().any(|value| *value == 255));
	/// }
	/// ```
	//==============================================================================
	pub fn rasterize_text_with_color(&self, text: &str, font_string: &str, bake_scale: f32, padding: i32, direction: ShapingDirection, color: Color) -> Option<TextRaster> {
		let luminance_color = [(color.red * 255.0).round().clamp(0.0, 255.0) as u8, (color.green * 255.0).round().clamp(0.0, 255.0) as u8, (color.blue * 255.0).round().clamp(0.0, 255.0) as u8];
		if text.is_empty() {
			return None;
		}
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(text, &font_description);
		if face_list.is_empty() || self.is_browser_raster {
			// 웹: 원본처럼 브라우저 Canvas2D 로 그린 덮인 정도를 받는다.
			#[cfg(target_arch = "wasm32")]
			{
				return rasterize_system_font_text(&font_description, text, bake_scale, padding.max(1), direction);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}
		let laid_out_text = lay_out_text(&face_list, &font_description, text, font_description.font_size * bake_scale, direction, padding.max(1), luminance_color)?;
		Some(TextRaster {
			coverage: laid_out_text.coverage,
			color_pixels: laid_out_text.color_pixels,
			width: laid_out_text.width,
			height: laid_out_text.height,
			origin_x: laid_out_text.origin_x,
			baseline_y: laid_out_text.baseline_y as f32,
			advance_width: laid_out_text.advance_width,
			font_ascent: laid_out_text.font_ascent,
			font_descent: laid_out_text.font_descent,
		})
	}

	//==============================================================================
	/// 글리프 하나 래스터. (글자 아틀라스가 쓴다, 글리프 번호는 글자로 찾는다. 웹은 브라우저 Canvas2D 로, 데스크탑에서 글꼴이 하나도 없으면 None)
	/// - 돌려주는 것: (래스터, 전진폭, 글꼴 위 길이, 글꼴 아래 길이) 길이 단위는 픽셀.
	///
	/// # 인자
	/// - `character`: 글자 묶음 하나. (결합 부호가 붙어 여러 글리프가 되어도 한 장에 모은다)
	/// - `font_string`: 폰트 문자열. (Canvas2D font 꼴, parse_font_string)
	///
	/// # 돌려줌
	/// (래스터, 전진폭, 글꼴 위 길이, 글꼴 아래 길이). 그릴 글리프가 없으면(빈칸) 빈 래스터. 데스크탑에서 그릴 얼굴이 하나도 없으면 None.
	///
	/// # 예제
	/// 글자 하나의 래스터와 전진폭
	/// ```rust
	/// use vanilla::core::textstringtexturecache::TextStringTextureCache;
	/// use vanilla::platform::find_fallback_font_file;
	///
	/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
	/// 	let font_bytes = std::fs::read(&source.path).expect("font file");
	/// 	let mut text_string_texture_cache = TextStringTextureCache::new();
	/// 	text_string_texture_cache.set_default_font(false, font_bytes).expect("font");
	/// 	let (glyph_raster, advance_width, font_ascent, _) = text_string_texture_cache.rasterize_character("A", "32px default").expect("glyph");
	/// 	assert!(glyph_raster.width > 0 && glyph_raster.height > 0);
	/// 	assert!(advance_width > 0.0 && font_ascent > 0.0);
	/// 	let (space_raster, space_advance_width, _, _) = text_string_texture_cache.rasterize_character(" ", "32px default").expect("space");
	/// 	assert_eq!(space_raster.width, 0);
	/// 	assert!(space_advance_width > 0.0);
	/// }
	/// ```
	//==============================================================================
	pub fn rasterize_character(&self, character: &str, font_string: &str) -> Option<(textshaping::GlyphRaster, f32, f32, f32)> {
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(character, &font_description);
		let font_size = font_description.font_size;
		if face_list.is_empty() || self.is_browser_raster {
			// 웹: 원본처럼 브라우저 Canvas2D 로 굽는다.
			#[cfg(target_arch = "wasm32")]
			{
				let glyph_raster = crate::platform::web::systemfont::rasterize_piece(&font_description, font_size, character, false)?;
				let text_metrics = crate::platform::web::systemfont::measure_text(&font_description, character);
				return Some((glyph_raster, text_metrics.width, text_metrics.font_ascent, text_metrics.font_descent));
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}
		let system_font_measure = create_system_font_measure(&font_description, font_size);
		let shaped_text = textshaping::shape_text(&face_list, character, font_size, ShapingDirection::LeftToRight, system_font_measure.as_deref());
		let (font_ascent, font_descent) = read_line_metrics(&face_list[0].font, font_size);
		// 여러 글리프로 지어지는 글자 묶음(결합 부호)도 한 장에 모은다.
		let mut minimum_x = i32::MAX;
		let mut maximum_x = i32::MIN;
		let mut maximum_top = i32::MIN;
		let mut minimum_bottom = i32::MAX;
		let mut glyph_raster_list: Vec<(textshaping::GlyphRaster, i32, i32)> = Vec::new();
		for shaped_glyph in &shaped_text.glyph_list {
			let (integer_x, subpixel_x, integer_y) = split_glyph_position(shaped_glyph.position_x, shaped_glyph.position_y);
			let glyph_raster = match rasterize_shaped_glyph(&face_list, &font_description, character, shaped_glyph, font_size, subpixel_x, WHITE_LUMINANCE_COLOR) {
				Some(glyph_raster) => {
					glyph_raster
				}
				None => {
					continue;
				}
			};
			let left_x = integer_x + glyph_raster.left_x;
			let top_y = integer_y + glyph_raster.top_y;
			minimum_x = minimum_x.min(left_x);
			maximum_x = maximum_x.max(left_x + glyph_raster.width);
			maximum_top = maximum_top.max(top_y);
			minimum_bottom = minimum_bottom.min(top_y - glyph_raster.height);
			glyph_raster_list.push((glyph_raster, left_x, top_y));
		}
		if glyph_raster_list.is_empty() {
			let empty_raster = textshaping::GlyphRaster {
				coverage: Vec::new(),
				width: 0,
				height: 0,
				left_x: 0,
				top_y: 0,
				color_pixels: None,
			};
			return Some((empty_raster, shaped_text.advance_width, font_ascent, font_descent));
		}
		let width = maximum_x - minimum_x;
		let height = maximum_top - minimum_bottom;
		let mut coverage = vec![0u8; (width * height) as usize];
		let mut color_pixels: Option<Vec<u8>> = None;
		for (glyph_raster, left_x, top_y) in &glyph_raster_list {
			let offset_x = left_x - minimum_x;
			let offset_y = maximum_top - top_y;
			if let Some(glyph_color_pixels) = glyph_raster.color_pixels.as_ref() {
				let layer_pixels = color_pixels.get_or_insert_with(|| vec![0u8; (width * height * 4) as usize]);
				draw_color_pixels(layer_pixels, width, height, glyph_color_pixels, glyph_raster.width, glyph_raster.height, offset_x, offset_y);
			}
			for glyph_y in 0..glyph_raster.height {
				for glyph_x in 0..glyph_raster.width {
					let index = ((offset_y + glyph_y) * width + offset_x + glyph_x) as usize;
					coverage[index] = blend_coverage(coverage[index], glyph_raster.coverage[(glyph_y * glyph_raster.width + glyph_x) as usize]);
				}
			}
		}
		let glyph_raster = textshaping::GlyphRaster {
			coverage,
			width,
			height,
			left_x: minimum_x,
			top_y: maximum_top,
			color_pixels,
		};
		Some((glyph_raster, shaped_text.advance_width, font_ascent, font_descent))
	}

	//==============================================================================
	/// 구운 문자열 수.
	///
	/// # 돌려줌
	/// 캐시에 든 구운 문자열 수. (상한 512)
	//==============================================================================
	pub fn get_entry_count(&self) -> usize {
		self.entry_count
	}

	//==============================================================================
	/// 폰트 문자열에 맞는 얼굴 목록. (이름 목록의 차례로 이름마다 그 얼굴들, 기본 폰트, 시스템 글꼴 사슬, 표준 글꼴, 대체 폰트 차례. 글자마다 앞에서부터 찾는다, 올리지 않은 이름은 넘긴다)
	/// - 이름 목록에서 얼굴을 하나도 찾지 못하고 default 도 없으면 표준 글꼴이 기본 폰트보다 앞이다. (크롬은 그때 표준 글꼴이 첫 글꼴이다)
	//==============================================================================
	fn resolve_face_list(&self, font_description: &FontDescription) -> Vec<ResolvedFontFace> {
		let mut face_list: Vec<ResolvedFontFace> = Vec::new();
		let mut used_system_family_list: Vec<String> = Vec::new();
		// 목록의 마지막 일반 이름(serif, sans-serif, cursive, fantasy, monospace, -webkit-body)이 -webkit-body 면 올린 얼굴이 없는 이름은 모두 표준 글꼴로 찾는다.
		// (크롬 ConvertFontFamily 가 그것을 FontDescription 의 일반 이름으로 두고, FontSelector::FamilyNameFromSettings 가 어느 이름이든 표준 글꼴을 돌려준다)
		let last_generic_family = font_description.family_name_list.iter().rev().find(|family_name| matches!(family_name.to_ascii_lowercase().as_str(), "serif" | "sans-serif" | "cursive" | "fantasy" | "monospace" | "-webkit-body"));
		let is_body_family = last_generic_family.is_some_and(|family_name| family_name.eq_ignore_ascii_case("-webkit-body"));
		for family_name in &font_description.family_name_list {
			if let Some(registered_family_key) = read_registered_family_key(family_name) {
				if let Some(family_face_list) = self.font_family_table.get(&registered_family_key) {
					let family_face_reference_list: Vec<&RegisteredFontFace> = family_face_list.iter().collect();
					textshaping::select_font_faces(&family_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
					continue;
				}
			}
			// 올리지 않은 이름은 운영체제 글꼴에서 찾는다. (데스크탑, 사슬을 켰을 때. 따옴표 친 일반 이름은 따옴표째 찾아 글꼴 이름이 된다)
			let family_key = if is_body_family {
				String::from("-webkit-body")
			}
			else {
				family_name.to_lowercase()
			};
			if !used_system_family_list.contains(&family_key) && self.append_system_family_faces(&family_key, font_description.font_weight, font_description.is_italic, &mut face_list) {
				used_system_family_list.push(family_key);
			}
		}
		// 이름 목록에서 얼굴을 하나도 찾지 못했으면 크롬처럼 표준 글꼴이 첫 글꼴이다. (FontFallbackList 는 목록이 다 떨어지면 -webkit-standard 를 쓴다, 엔진 기본 글꼴 이름 default 는 넘긴다)
		if face_list.is_empty() && !font_description.family_name_list.iter().any(|family_name| family_name.eq_ignore_ascii_case("default")) {
			self.append_standard_family_faces(font_description, &mut face_list, &mut used_system_family_list);
		}
		let default_face_reference_list: Vec<&RegisteredFontFace> = self.default_face_list.iter().collect();
		textshaping::select_font_faces(&default_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		// 기본 글꼴 사슬. (원본 SYSTEM_FONT_STRING 의 차례, 기본 글꼴 이름 default 를 청했을 때만. 원본도 기본 글꼴일 때만 폰트 문자열이 그 사슬이고, 다른 이름은 크롬이 사슬 없이 운영체제 대체로 간다)
		let is_default_requested = font_description.family_name_list.iter().any(|family_name| family_name.eq_ignore_ascii_case("default"));
		let chain_name_list = if is_default_requested {
			self.system_font_state.borrow().chain_name_list.clone()
		}
		else {
			Vec::new()
		};
		for family_key in chain_name_list {
			if used_system_family_list.contains(&family_key) {
				continue;
			}
			if self.append_system_family_faces(&family_key, font_description.font_weight, font_description.is_italic, &mut face_list) {
				used_system_family_list.push(family_key);
			}
		}
		self.append_standard_family_faces(font_description, &mut face_list, &mut used_system_family_list);
		for fallback_face in self.fallback_face_list.borrow().iter() {
			let fallback_face_reference_list = [fallback_face];
			textshaping::select_font_faces(&fallback_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		}
		// 운영체제가 고른 대체 글꼴. (이 첫 글꼴과 폰트 문자열로 물은 것만, 같은 글꼴의 굵기들은 한 이름으로 고른다)
		let fallback_base_key = self.read_fallback_base_info(font_description, &face_list).key;
		let system_font_state = self.system_font_state.borrow();
		let mut grouped_key_list: Vec<&str> = Vec::new();
		for (fallback_face, base_key) in system_font_state.fallback_face_list.iter().zip(system_font_state.fallback_base_key_list.iter()) {
			if *base_key != fallback_base_key {
				continue;
			}
			let source_key = fallback_face.descriptors.source_key.as_str();
			if grouped_key_list.contains(&source_key) {
				continue;
			}
			grouped_key_list.push(source_key);
			let group_face_list: Vec<&RegisteredFontFace> = system_font_state.fallback_face_list.iter().zip(system_font_state.fallback_base_key_list.iter()).filter(|(face, base_key)| **base_key == fallback_base_key && face.descriptors.source_key == source_key).map(|(face, _)| face).collect();
			textshaping::select_font_faces(&group_face_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		}
		face_list
	}

	//==============================================================================
	/// 표준 글꼴의 얼굴 더하기. (크롬 FontFallbackList 는 이름 목록이 다 떨어지면 화면 언어의 표준 글꼴(-webkit-standard)을 쓴다. 데스크탑, 이름 찾기를 켰을 때, 이미 더한 이름은 넘긴다)
	//==============================================================================
	fn append_standard_family_faces(&self, font_description: &FontDescription, face_list: &mut Vec<ResolvedFontFace>, used_system_family_list: &mut Vec<String>) {
		#[cfg(not(target_arch = "wasm32"))]
		{
			let is_lookup_enabled = self.system_font_state.borrow().is_lookup_enabled;
			if is_lookup_enabled {
				if let Some(standard_family_name) = crate::platform::desktop::systemfont::resolve_generic_family_name(crate::platform::desktop::systemfont::GenericFamily::Standard) {
					let standard_family_key = standard_family_name.to_lowercase();
					if !used_system_family_list.contains(&standard_family_key) && self.append_system_family_faces(&standard_family_key, font_description.font_weight, font_description.is_italic, face_list) {
						used_system_family_list.push(standard_family_key);
					}
				}
			}
		}
		#[cfg(target_arch = "wasm32")]
		{
			let _ = (font_description, face_list, used_system_family_list);
		}
	}

	//==============================================================================
	/// 지켜보는 이름의 얼굴이 없는 글자 모으기. (폰트 문자열의 이름 가운데 지켜보는 것마다, 올린 얼굴의 글자 범위에 들지 않은 글자)
	//==============================================================================
	fn note_unresolved_family_characters(&self, text: &str, font_description: &FontDescription) {
		UNRESOLVED_FAMILY_CHARACTER_TABLE.with(|table_cell| {
			let mut table = table_cell.borrow_mut();
			if table.is_empty() {
				return;
			}
			for family_name in &font_description.family_name_list {
				let family_key = match read_registered_family_key(family_name) {
					Some(family_key) => {
						family_key
					}
					None => {
						continue;
					}
				};
				let character_set = match table.get_mut(&family_key) {
					Some(character_set) => {
						character_set
					}
					None => {
						continue;
					}
				};
				let face_list = self.font_family_table.get(&family_key);
				for character in text.chars() {
					if character.is_whitespace() || character.is_control() {
						continue;
					}
					let is_covered = face_list.map(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character))).unwrap_or(false);
					if !is_covered {
						character_set.insert(character);
					}
				}
			}
		});
	}

	//==============================================================================
	/// 글자를 그릴 얼굴 목록. (대체 글꼴 후보와 운영체제 대체 글꼴을 먼저 채우고 고른다)
	//==============================================================================
	fn resolve_covering_face_list(&self, text: &str, font_description: &FontDescription) -> Vec<ResolvedFontFace> {
		self.note_unresolved_family_characters(text, font_description);
		self.ensure_fallback_coverage(text);
		let mut face_list = self.resolve_face_list(font_description);
		if self.system_font_state.borrow().is_fallback_enabled {
			self.ensure_system_fallback_coverage(text, font_description, &face_list);
			face_list = self.resolve_face_list(font_description);
		}
		self.append_last_resort_faces(font_description, &mut face_list);
		face_list
	}

	//==============================================================================
	/// 마지막 대체 글꼴의 얼굴을 맨 뒤에 더하기. (데스크탑, 이름 찾기를 켰을 때. 운영체제 대체 글꼴을 물은 뒤라 그 물음에는 끼지 않는다)
	/// - 크롬 FontFallbackIterator 는 시스템 대체로도 다 그리지 못한 글자 묶음을 GetLastResortFallbackFont(맥 Times)로 그린다.
	///   기본 글꼴에 결합 부호가 없는 "é"(e + U+0301) 가 맥 크롬에서 Times 로 그려지는 자리다.
	//==============================================================================
	fn append_last_resort_faces(&self, font_description: &FontDescription, face_list: &mut Vec<ResolvedFontFace>) {
		#[cfg(not(target_arch = "wasm32"))]
		{
			if !self.system_font_state.borrow().is_lookup_enabled {
				return;
			}
			if let Some(last_resort_family_name) = crate::platform::desktop::systemfont::read_last_resort_family_name() {
				let family_key = last_resort_family_name.to_lowercase();
				let first_face_index = face_list.len();
				self.append_system_family_faces(&family_key, font_description.font_weight, font_description.is_italic, face_list);
				// 마지막 대체 글꼴은 대체 단계다. (이름 목록처럼 먼저 고르면 리눅스 DejaVu Sans 의 흑백 😀 가 색 이모지보다 앞선다)
				for face in face_list[first_face_index..].iter_mut() {
					face.descriptors.is_system_fallback = true;
				}
			}
		}
		#[cfg(target_arch = "wasm32")]
		{
			let _ = (font_description, face_list);
		}
	}

	//==============================================================================
	/// 문자열 굽기.
	//==============================================================================
	fn bake_entry(&self, gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<TextEntry> {
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(text, &font_description);
		if face_list.is_empty() || self.is_browser_raster {
			// 웹: 원본처럼 브라우저 Canvas2D 로 굽는다.
			#[cfg(target_arch = "wasm32")]
			{
				return bake_system_font_entry(gl, text_draw_kind, text, &font_description, line_width, bake_scale, self.is_text_antialias_enabled, direction);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}

		// 비트맵 여백. (외곽선 두께만큼)
		let stroke_radius = match text_draw_kind {
			TextDrawKind::Stroke => {
				(line_width * bake_scale * 0.5).max(0.5)
			}
			TextDrawKind::Fill => {
				0.0
			}
		};
		let padding = stroke_radius.ceil() as i32 + 1;
		let pixel_size = font_description.font_size * bake_scale;
		let laid_out_text = lay_out_text(&face_list, &font_description, text, pixel_size, direction, padding, WHITE_LUMINANCE_COLOR)?;
		let mut coverage = laid_out_text.coverage;
		let mut color_pixels = laid_out_text.color_pixels;
		let bitmap_width = laid_out_text.width;
		let bitmap_height = laid_out_text.height;

		// 외곽선: 윤곽 바깥으로 반두께 넓힌 것에서 안으로 반두께 좁힌 것을 뺀 띠. (Canvas2D strokeText 는 윤곽선 가운데에 선을 긋는다, 색 글리프는 테 없이 그대로)
		if text_draw_kind == TextDrawKind::Stroke {
			coverage = build_stroke_coverage(&coverage, bitmap_width, bitmap_height, stroke_radius);
		}

		// 안티앨리어싱을 끈 경우: 문턱값으로 갈라 가장자리의 반투명 픽셀을 없앤다.
		let is_antialias = self.is_text_antialias_enabled;
		if !is_antialias {
			apply_alpha_threshold(&mut coverage);
			if let Some(color_pixels) = color_pixels.as_mut() {
				apply_premultiplied_alpha_threshold(color_pixels);
			}
		}

		// 흰색 프리멀티플라이드 RGBA 위에 색 글리프를 얹어 올리기.
		let pixels = compose_text_pixels(&coverage, color_pixels.as_deref());
		let texture = upload_text_texture(gl, bitmap_width, bitmap_height, &pixels, is_antialias)?;

		Some(TextEntry {
			texture,
			advance_width: laid_out_text.advance_width / bake_scale,
			font_ascent: laid_out_text.font_ascent / bake_scale,
			font_descent: laid_out_text.font_descent / bake_scale,
			pen_offset_x: laid_out_text.origin_x / bake_scale,
			baseline_offset_y: laid_out_text.baseline_y as f32 / bake_scale,
			quad_width: bitmap_width as f32 / bake_scale,
			quad_height: bitmap_height as f32 / bake_scale,
		})
	}
}

impl Default for TextStringTextureCache {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TextStringTextureCache {
		TextStringTextureCache::new()
	}
}

//==============================================================================
/// 글을 모양 짓고 글리프를 래스터해 한 장에 놓기. (외곽선, 그림자 몫의 여백 padding)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn lay_out_text(face_list: &[ResolvedFontFace], font_description: &FontDescription, text: &str, pixel_size: f32, direction: ShapingDirection, padding: i32, luminance_color: [u8; 3]) -> Option<LaidOutText> {
	let system_font_measure = create_system_font_measure(font_description, pixel_size);
	let shaped_text: ShapedText = textshaping::shape_text(face_list, text, pixel_size, direction, system_font_measure.as_deref());
	let advance_width = shaped_text.advance_width;
	let (font_ascent, font_descent) = read_line_metrics(&face_list[0].font, pixel_size);

	// 글리프 래스터와 경계. (펜 원점 기준, 위가 양수)
	let mut glyph_raster_list: Vec<(textshaping::GlyphRaster, i32, i32)> = Vec::with_capacity(shaped_text.glyph_list.len());
	let mut minimum_x: f32 = 0.0;
	let mut maximum_x: f32 = advance_width;
	let mut top_extent: f32 = font_ascent;
	let mut bottom_extent: f32 = font_descent;
	for shaped_glyph in &shaped_text.glyph_list {
		let (integer_x, subpixel_x, integer_y) = split_glyph_position(shaped_glyph.position_x, shaped_glyph.position_y);
		let glyph_raster = match rasterize_shaped_glyph(face_list, font_description, text, shaped_glyph, pixel_size, subpixel_x, luminance_color) {
			Some(glyph_raster) => {
				glyph_raster
			}
			None => {
				continue;
			}
		};
		let left_x = integer_x + glyph_raster.left_x;
		let top_y = integer_y + glyph_raster.top_y;
		minimum_x = minimum_x.min(left_x as f32);
		maximum_x = maximum_x.max((left_x + glyph_raster.width) as f32);
		top_extent = top_extent.max(top_y as f32);
		bottom_extent = bottom_extent.max((glyph_raster.height - top_y) as f32);
		glyph_raster_list.push((glyph_raster, left_x, top_y));
	}

	let origin_x = padding as f32 - minimum_x.floor();
	let baseline_y = padding + top_extent.ceil() as i32;
	let width = (origin_x + maximum_x).ceil() as i32 + padding;
	let height = baseline_y + bottom_extent.ceil() as i32 + padding;
	if width <= 0 || height <= 0 {
		return None;
	}

	// 겹치는 곳은 원본 캔버스처럼 뒤 글리프를 위에 얹는다. (src-over) 색 글리프는 따로 색 층에 얹는다.
	let mut coverage = vec![0u8; (width * height) as usize];
	let mut color_pixels: Option<Vec<u8>> = None;
	let origin_x_integer = origin_x as i32;
	for (glyph_raster, left_x, top_y) in &glyph_raster_list {
		let glyph_left_x = origin_x_integer + left_x;
		let glyph_top_y = baseline_y - top_y;
		if let Some(glyph_color_pixels) = glyph_raster.color_pixels.as_ref() {
			let layer_pixels = color_pixels.get_or_insert_with(|| vec![0u8; (width * height * 4) as usize]);
			draw_color_pixels(layer_pixels, width, height, glyph_color_pixels, glyph_raster.width, glyph_raster.height, glyph_left_x, glyph_top_y);
		}
		for glyph_y in 0..glyph_raster.height {
			let bitmap_y = glyph_top_y + glyph_y;
			if bitmap_y < 0 || bitmap_y >= height {
				continue;
			}
			for glyph_x in 0..glyph_raster.width {
				let bitmap_x = glyph_left_x + glyph_x;
				if bitmap_x < 0 || bitmap_x >= width {
					continue;
				}
				let glyph_value = glyph_raster.coverage[(glyph_y * glyph_raster.width + glyph_x) as usize];
				let bitmap_index = (bitmap_y * width + bitmap_x) as usize;
				coverage[bitmap_index] = blend_coverage(coverage[bitmap_index], glyph_value);
			}
		}
	}
	Some(LaidOutText {
		coverage,
		color_pixels,
		width,
		height,
		origin_x,
		baseline_y,
		advance_width,
		font_ascent,
		font_descent,
	})
}

//==============================================================================
/// 브라우저 시스템 글꼴로 글 모양 래스터 받기. (웹, 글꼴 바이트가 없을 때 rasterize_text 자리)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn rasterize_system_font_text(font_description: &FontDescription, text: &str, bake_scale: f32, padding: i32, direction: ShapingDirection) -> Option<TextRaster> {
	let pixel_size = font_description.font_size * bake_scale;
	let glyph_raster = crate::platform::web::systemfont::rasterize_piece(font_description, pixel_size, text, direction == ShapingDirection::RightToLeft)?;
	let mut scaled_description = font_description.clone();
	scaled_description.font_size = pixel_size;
	let text_metrics = crate::platform::web::systemfont::measure_text(&scaled_description, text);
	let width = glyph_raster.width + padding * 2;
	let height = glyph_raster.height + padding * 2;
	let mut coverage = vec![0u8; (width * height) as usize];
	for position_y in 0..glyph_raster.height {
		for position_x in 0..glyph_raster.width {
			coverage[((position_y + padding) * width + position_x + padding) as usize] = glyph_raster.coverage[(position_y * glyph_raster.width + position_x) as usize];
		}
	}
	let color_pixels = glyph_raster.color_pixels.as_ref().map(|glyph_color_pixels| {
		let mut layer_pixels = vec![0u8; (width * height * 4) as usize];
		draw_color_pixels(&mut layer_pixels, width, height, glyph_color_pixels, glyph_raster.width, glyph_raster.height, padding, padding);
		layer_pixels
	});
	Some(TextRaster {
		coverage,
		color_pixels,
		width,
		height,
		origin_x: (padding - glyph_raster.left_x) as f32,
		baseline_y: (padding + glyph_raster.top_y) as f32,
		advance_width: text_metrics.width,
		font_ascent: text_metrics.font_ascent,
		font_descent: text_metrics.font_descent,
	})
}

//==============================================================================
/// 모양 지은 글리프 하나 래스터. (시스템 글꼴 조각은 브라우저 캔버스로, 비었으면 None)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn rasterize_shaped_glyph(face_list: &[ResolvedFontFace], font_description: &FontDescription, text: &str, shaped_glyph: &textshaping::ShapedGlyph, pixel_size: f32, subpixel_x: f32, luminance_color: [u8; 3]) -> Option<textshaping::GlyphRaster> {
	if shaped_glyph.face_index == textshaping::SYSTEM_FACE_INDEX {
		#[cfg(target_arch = "wasm32")]
		{
			return crate::platform::web::systemfont::rasterize_piece(font_description, pixel_size, &text[shaped_glyph.text_start..shaped_glyph.text_end], shaped_glyph.is_right_to_left);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			let _ = (font_description, text);
			return None;
		}
	}
	let glyph_raster = textshaping::rasterize_glyph_at(&face_list[shaped_glyph.face_index], shaped_glyph.glyph_index, pixel_size, subpixel_x, luminance_color);
	if glyph_raster.width == 0 || glyph_raster.height == 0 {
		return None;
	}
	Some(glyph_raster)
}

//==============================================================================
/// 덮인 정도 위에 덮인 정도 얹기. (원본 캔버스가 글리프 마스크를 차례로 칠하는 src-over, 8 비트로 반올림)
//==============================================================================
fn blend_coverage(destination: u8, source: u8) -> u8 {
	((source as u32 * 255 + destination as u32 * (255 - source as u32) + 127) / 255) as u8
}

//==============================================================================
/// 글리프 자리를 정수 자리와 서브픽셀 몫으로 나누기. ((정수 x, 서브픽셀 x, 정수 y), y 는 위가 양수)
/// - 맥, 윈도우, 리눅스: 크롬 스키아처럼 x 에 1/8 을 더해 내린 것이 정수 자리이고 나머지를 1/4 단위로 내린 것이 서브픽셀 몫이다. y 는 아래가 양수인 기기 좌표에서 1/2 을 더해 내린다.
///   (SkGlyphPositionRoundingSpec 의 가로 정렬 글: halfAxisSampleFreq (1/8, 1/2), SkPackedGlyphID 의 서브픽셀 2 비트, SubRunContainer 의 floor)
///   블링크는 윈도우에서 안티에일리어스면(font_platform_data_win.cc), 리눅스에서 힌팅이 꽉이 아니면(WebFontRenderStyle::ApplyToSkFont) 서브픽셀 자리를 켠다.
/// - 그 밖은 둘 다 반올림하고 서브픽셀 몫이 없다.
//==============================================================================
fn split_glyph_position(position_x: f32, position_y: f32) -> (i32, f32, i32) {
	#[cfg(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android"), not(target_arch = "wasm32"))))]
	{
		let rounded_x = position_x + SUBPIXEL_ROUND;
		let integer_x = rounded_x.floor();
		let subpixel_x = (((rounded_x - integer_x) * SUBPIXEL_STEP_COUNT).floor() / SUBPIXEL_STEP_COUNT).min(1.0 - 1.0 / SUBPIXEL_STEP_COUNT);
		let integer_y = -((-position_y + 0.5).floor());
		(integer_x as i32, subpixel_x, integer_y as i32)
	}
	#[cfg(not(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android"), not(target_arch = "wasm32")))))]
	{
		(position_x.round() as i32, 0.0, position_y.round() as i32)
	}
}

//==============================================================================
/// 시스템 글꼴 조각 재는 함수. (웹이면 브라우저 캔버스로 재고, 데스크탑은 없다)
//==============================================================================
#[allow(clippy::type_complexity)]
fn create_system_font_measure(font_description: &FontDescription, pixel_size: f32) -> Option<Box<dyn Fn(&str, bool) -> f32>> {
	#[cfg(target_arch = "wasm32")]
	{
		let font_description = font_description.clone();
		return Some(Box::new(move |piece_text: &str, is_right_to_left: bool| crate::platform::web::systemfont::measure_piece(&font_description, pixel_size, piece_text, is_right_to_left)));
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		let _ = (font_description, pixel_size);
		None
	}
}

//==============================================================================
/// 브라우저 시스템 글꼴로 굽기. (웹. 글꼴 바이트가 하나도 없을 때)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn bake_system_font_entry(gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_description: &FontDescription, line_width: f32, bake_scale: f32, is_antialias: bool, direction: ShapingDirection) -> Option<TextEntry> {
	let mut system_font_bitmap = crate::platform::web::systemfont::bake_text(text_draw_kind, text, font_description, line_width, bake_scale, direction)?;
	if !is_antialias {
		// 프리멀티플라이드라 알파로 가르고 색은 곧은 색으로 채운다. (흰 글자는 네 칸 모두 덮인 정도, 색 이모지는 제 색)
		apply_premultiplied_alpha_threshold(&mut system_font_bitmap.pixels);
	}
	let texture = upload_text_texture(gl, system_font_bitmap.bitmap_width, system_font_bitmap.bitmap_height, &system_font_bitmap.pixels, is_antialias)?;
	Some(TextEntry {
		texture,
		advance_width: system_font_bitmap.advance_width / bake_scale,
		font_ascent: system_font_bitmap.font_ascent / bake_scale,
		font_descent: system_font_bitmap.font_descent / bake_scale,
		pen_offset_x: system_font_bitmap.pen_x / bake_scale,
		baseline_offset_y: system_font_bitmap.baseline_y / bake_scale,
		quad_width: system_font_bitmap.bitmap_width as f32 / bake_scale,
		quad_height: system_font_bitmap.bitmap_height as f32 / bake_scale,
	})
}

//==============================================================================
/// 문턱값으로 0 과 255 로 가르기. (안티앨리어싱을 끈 글자)
//==============================================================================
fn apply_alpha_threshold(values: &mut [u8]) {
	for value in values.iter_mut() {
		*value = if *value >= ALPHA_THRESHOLD {
			255
		}
		else {
			0
		};
	}
}

//==============================================================================
/// 프리멀티플라이드 RGBA 를 알파 문턱값으로 가르기. (원본처럼 곧은 알파의 알파 칸만 0 과 255 로, 남는 픽셀은 곧은 색 그대로 불투명)
//==============================================================================
fn apply_premultiplied_alpha_threshold(pixels: &mut [u8]) {
	for pixel in pixels.chunks_exact_mut(4) {
		let alpha = pixel[3] as u32;
		if alpha >= ALPHA_THRESHOLD as u32 {
			for channel_index in 0..3 {
				pixel[channel_index] = ((pixel[channel_index] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
			}
			pixel[3] = 255;
		}
		else {
			pixel.fill(0);
		}
	}
}

//==============================================================================
/// 색 그림을 색 층에 얹기. (프리멀티플라이드 RGBA, 원본 위에 얹기. 왼쪽 위 자리 offset 부터, 층 밖은 버린다)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn draw_color_pixels(layer_pixels: &mut [u8], layer_width: i32, layer_height: i32, source_pixels: &[u8], source_width: i32, source_height: i32, offset_x: i32, offset_y: i32) {
	for source_y in 0..source_height {
		let layer_y = offset_y + source_y;
		if layer_y < 0 || layer_y >= layer_height {
			continue;
		}
		for source_x in 0..source_width {
			let layer_x = offset_x + source_x;
			if layer_x < 0 || layer_x >= layer_width {
				continue;
			}
			let source_index = ((source_y * source_width + source_x) * 4) as usize;
			let source_alpha = source_pixels[source_index + 3] as u32;
			if source_alpha == 0 {
				continue;
			}
			let layer_index = ((layer_y * layer_width + layer_x) * 4) as usize;
			for channel_index in 0..4 {
				let layer_value = layer_pixels[layer_index + channel_index] as u32;
				layer_pixels[layer_index + channel_index] = (source_pixels[source_index + channel_index] as u32 + (layer_value * (255 - source_alpha) + 127) / 255).min(255) as u8;
			}
		}
	}
}

//==============================================================================
/// 흰 글자와 색 층을 한 장으로. (흰색 프리멀티플라이드 RGBA 위에 색 층을 얹는다, 색 층이 없으면 흰 글자만)
//==============================================================================
pub(crate) fn compose_text_pixels(coverage: &[u8], color_pixels: Option<&[u8]>) -> Vec<u8> {
	let mut pixels = Vec::with_capacity(coverage.len() * 4);
	match color_pixels {
		Some(color_pixels) => {
			for (pixel_index, value) in coverage.iter().enumerate() {
				let color_alpha = color_pixels[pixel_index * 4 + 3] as u32;
				let white_value = (*value as u32 * (255 - color_alpha) + 127) / 255;
				for channel_index in 0..4 {
					pixels.push((color_pixels[pixel_index * 4 + channel_index] as u32 + white_value).min(255) as u8);
				}
			}
		}
		None => {
			for value in coverage {
				pixels.push(*value);
				pixels.push(*value);
				pixels.push(*value);
				pixels.push(*value);
			}
		}
	}
	pixels
}

//==============================================================================
/// 글꼴 파일에 색 글리프 표(sbix, CBDT, COLR)가 있는지 엿보기. (파일 앞머리의 표 목록만 읽는다, 못 읽으면 false)
/// - 대체 글꼴 후보를 고를 때 쓴다. 큰 색 이모지 글꼴(맥 Apple Color Emoji 는 190MB 남짓)을 이모지가 아닌 글자 때문에 읽지 않게 한다.
//==============================================================================
fn peek_color_font_tables(font_path: &std::path::Path) -> bool {
	use std::io::Read;
	let mut header_bytes: Vec<u8> = Vec::new();
	let is_read = std::fs::File::open(font_path).and_then(|file| file.take(65536).read_to_end(&mut header_bytes)).is_ok();
	if !is_read {
		return false;
	}
	match rustybuzz::ttf_parser::RawFace::parse(&header_bytes, 0) {
		Ok(raw_face) => {
			raw_face.table_records.into_iter().any(|table_record| matches!(&table_record.tag.to_bytes(), b"sbix" | b"CBDT" | b"COLR"))
		}
		Err(_) => {
			false
		}
	}
}

//==============================================================================
/// 구운 픽셀을 텍스처로 올리기. (흰색 프리멀티플라이드 RGBA, 안티앨리어싱을 끄면 NEAREST)
//==============================================================================
fn upload_text_texture(gl: &glow::Context, bitmap_width: i32, bitmap_height: i32, pixels: &[u8], is_antialias: bool) -> Option<crate::core::graphic::GlTexture> {
	let texture_filter = if is_antialias {
		glow::LINEAR
	}
	else {
		glow::NEAREST
	};
	unsafe {
		let texture = gl.create_texture().ok()?;
		gl.bind_texture(glow::TEXTURE_2D, Some(texture));
		gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
		gl.tex_image_2d(
			glow::TEXTURE_2D,
			0,
			glow::RGBA8 as i32,
			bitmap_width,
			bitmap_height,
			0,
			glow::RGBA,
			glow::UNSIGNED_BYTE,
			glow::PixelUnpackData::Slice(Some(pixels)),
		);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, texture_filter as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, texture_filter as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
		Some(texture)
	}
}

//==============================================================================
/// 구운 문자열 키의 해시. (방식, 안티앨리어싱, 문단 방향, 폰트 문자열, 선 두께, 배율, 글)
//==============================================================================
fn create_entry_hash(text_draw_kind: TextDrawKind, is_antialias: bool, direction: ShapingDirection, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> u64 {
	let mut hasher = DefaultHasher::new();
	text_draw_kind.hash(&mut hasher);
	is_antialias.hash(&mut hasher);
	direction.hash(&mut hasher);
	font_string.hash(&mut hasher);
	line_width.to_bits().hash(&mut hasher);
	bake_scale.to_bits().hash(&mut hasher);
	text.hash(&mut hasher);
	hasher.finish()
}

//==============================================================================
/// 운영체제 글꼴에서 이름 하나 찾기. (보통, 굵게, 기울임, 굵은 기울임 네 가지를 묻고 같은 얼굴은 하나로, 하나도 없으면 None. 데스크탑만)
//==============================================================================
fn find_system_font_family(family_key: &str) -> Option<SystemFontFamily> {
	#[cfg(not(target_arch = "wasm32"))]
	{
		let mut system_font_family = SystemFontFamily::default();
		for (weight, is_italic) in [(400, false), (700, false), (400, true), (700, true)] {
			let source = match crate::platform::find_family_font_file(family_key, weight, is_italic) {
				Some(source) => {
					source
				}
				None => {
					continue;
				}
			};
			// 같은 얼굴이면 앞(보통 굵기, 바른 꼴)의 것만 둔다. (기울인 얼굴이 없는 글꼴은 바른 얼굴에 가짜 기울임)
			if system_font_family.candidate_list.iter().any(|candidate| candidate.source == source) {
				continue;
			}
			system_font_family.candidate_list.push(SystemFontCandidate {
				weight,
				is_italic,
				source,
				is_loaded: false,
			});
		}
		if system_font_family.candidate_list.is_empty() {
			return None;
		}
		Some(system_font_family)
	}
	#[cfg(target_arch = "wasm32")]
	{
		let _ = family_key;
		None
	}
}

//==============================================================================
/// 운영체제가 고른 대체 글꼴 파일. (데스크탑만, 웹은 None)
//==============================================================================
fn find_system_fallback_font(grapheme: &str, weight: u16, is_italic: bool, is_color_wanted: bool, fallback_base_info: &FallbackBaseInfo) -> Option<FontFileSource> {
	#[cfg(not(target_arch = "wasm32"))]
	{
		let fallback_base = crate::platform::desktop::systemfont::FallbackBase {
			primary_family_name: fallback_base_info.primary_family_name.as_deref(),
			primary_font_bytes: fallback_base_info.primary_font.as_ref().map(|font| (&font.get_font_bytes()[..], font.get_face_index())),
			first_family_name: &fallback_base_info.first_family_name,
			is_monospace: fallback_base_info.is_monospace,
		};
		crate::platform::desktop::systemfont::find_fallback_font_file_for_base(grapheme, weight, is_italic, is_color_wanted, &fallback_base)
	}
	#[cfg(target_arch = "wasm32")]
	{
		let _ = (grapheme, weight, is_italic, is_color_wanted, fallback_base_info);
		None
	}
}

//==============================================================================
/// 운영체제 대체 글꼴을 물을 바탕. (크롬 PlatformFallbackFontForCharacter 의 FontDescription 과 첫 글꼴)
//==============================================================================
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct FallbackBaseInfo {
	/// 같은 대체를 나눠 쓰는 열쇠. (맥은 첫 글꼴, 윈도우는 첫 이름과 monospace, 리눅스는 빈 글)
	key: String,
	/// 첫 글꼴을 운영체제에서 찾은 이름. (시스템 글꼴 이름표의 열쇠, 올린 글꼴이면 None)
	primary_family_name: Option<String>,
	/// 첫 글꼴이 올린 글꼴이면 그 글꼴.
	primary_font: Option<Rc<LoadedFont>>,
	/// 폰트 문자열의 첫 이름.
	first_family_name: String,
	/// 마지막 일반 이름이 monospace 인지.
	is_monospace: bool,
}

//==============================================================================
/// 운영체제 대체 글꼴을 한 이름으로 묶는 열쇠. (같은 열쇠의 굵기 얼굴들이 함께 고른다)
/// - 맥, 윈도우: 글꼴 이름 표의 무리 이름, 없으면 파일 경로. 윈도우 Tahoma(tahoma.ttf, tahomabd.ttf)처럼 굵기마다 파일이 다른 글꼴도 한 무리로 묶어, 굵게를 청하면 굵은 얼굴을 고른다.
/// - 리눅스: 파일 경로. 크롬 리눅스는 fontconfig 가 고른 파일을 그대로 쓰고, 그 얼굴이 굵지 않으면 가짜 굵게로 그린다. (font_cache_linux.cc, 무리로 묶으면 굵은 아랍어에 DejaVuSans-Bold.ttf 를 골라 크롬의 DejaVuSans.ttf 가짜 굵게보다 16px 글에서 16.2px 넓다)
//==============================================================================
fn read_fallback_source_key(source: &FontFileSource, registered_font_face: &RegisteredFontFace) -> String {
	if cfg!(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))) {
		return source.path.to_string_lossy().into_owned();
	}
	let family_name = rustybuzz::ttf_parser::Face::parse(registered_font_face.font.get_font_bytes(), registered_font_face.font.get_face_index()).ok().and_then(|face| {
		let read_name = |name_id: u16| face.names().into_iter().filter(|name| name.name_id == name_id).find_map(|name| name.to_string());
		read_name(rustybuzz::ttf_parser::name_id::TYPOGRAPHIC_FAMILY).or_else(|| read_name(rustybuzz::ttf_parser::name_id::FAMILY))
	});
	match family_name {
		Some(family_name) => {
			format!("family:{}", family_name.to_lowercase())
		}
		None => {
			source.path.to_string_lossy().into_owned()
		}
	}
}

//==============================================================================
/// 시스템 글꼴 파일의 얼굴 하나 읽기. (같은 파일의 바이트는 나눠 든다)
/// - 얼굴 번호: 준 번호, 없으면 PostScript 이름이 같은 얼굴, 그것도 없으면 청한 굵기와 기울기로 고른 얼굴.
/// - 서술자: 가변 글꼴(wght 축)이면 청한 굵기를 축 범위로 자른 값의 인스턴스, 아니면 그 얼굴의 OS/2 굵기와 기울임 표시.
/// - opsz 축은 맥에서만 글자 크기로 건다. (크롬 font_platform_data_mac.mm, 윈도우와 리눅스의 font_cache_skia 는 시스템 글꼴의 축을 바꾸지 않는다)
//==============================================================================
fn load_system_font_face(shared_bytes_table: &mut HashMap<std::path::PathBuf, std::rc::Weak<[u8]>>, source: &FontFileSource, weight: u16, is_italic: bool) -> Result<RegisteredFontFace, String> {
	let font_bytes: Rc<[u8]> = match shared_bytes_table.get(&source.path).and_then(|weak_bytes| weak_bytes.upgrade()) {
		Some(font_bytes) => {
			font_bytes
		}
		None => {
			let font_bytes: Rc<[u8]> = Rc::from(std::fs::read(&source.path).map_err(|error| format!("글꼴 파일 읽기 실패: {}", error))?);
			shared_bytes_table.insert(source.path.clone(), Rc::downgrade(&font_bytes));
			font_bytes
		}
	};
	let face_index = match source.face_index {
		Some(face_index) => {
			face_index
		}
		None => {
			let post_script_index = source.post_script_name.as_ref().and_then(|post_script_name| find_post_script_face_index(&font_bytes, post_script_name));
			post_script_index.unwrap_or_else(|| textshaping::find_collection_face_index(&font_bytes, weight, is_italic))
		}
	};
	let mut font_face_descriptors = FontFaceDescriptors::new();
	let variation_list = match textshaping::read_weight_axis_range(&font_bytes, face_index) {
		Some((minimum_weight, maximum_weight)) => {
			font_face_descriptors.set_weight(weight.clamp(minimum_weight, maximum_weight));
			textshaping::read_weight_variation(&font_bytes, face_index, weight)
		}
		None => {
			Vec::new()
		}
	};
	// 크롬은 맥의 시스템 글꼴에만 opsz 축을 글자 크기로 건다. (윈도우, 리눅스 시스템 글꼴은 축 값을 바꾸지 않는다)
	let font = LoadedFont::create_from_shared_bytes(font_bytes, face_index, &variation_list)?.with_source_path(source.path.clone()).with_optical_sizing_auto(cfg!(target_os = "macos"));
	if variation_list.is_empty() && textshaping::read_weight_axis_range(font.get_font_bytes(), face_index).is_none() {
		let shaping_face = font.get_shaping_face();
		font_face_descriptors.set_weight(shaping_face.weight().to_number());
	}
	font_face_descriptors.is_italic = font.get_shaping_face().is_italic();
	Ok(RegisteredFontFace {
		descriptors: font_face_descriptors,
		font: Rc::new(font),
		order_number: 0,
	})
}

//==============================================================================
/// 글꼴 모음에서 PostScript 이름이 같은 얼굴 번호. (없으면 None)
/// - 이름 표의 유니코드, 윈도우 기록과 맥 기록을 본다. 맥 기록(Mac Roman)은 ttf-parser 가 풀지 않아 바이트를 그대로 글자로 읽는다. (PostScript 이름은 ASCII 다)
///   맥 Papyrus.ttc 처럼 맥 기록만 있는 모음에서 Condensed 얼굴로 잘못 고르지 않는다.
//==============================================================================
fn find_post_script_face_index(font_bytes: &[u8], post_script_name: &str) -> Option<u32> {
	let face_count = rustybuzz::ttf_parser::fonts_in_collection(font_bytes).unwrap_or(1);
	(0..face_count).find(|face_index| {
		rustybuzz::ttf_parser::Face::parse(font_bytes, *face_index).map(|face| {
			face.names().into_iter().any(|name| {
				if name.name_id != rustybuzz::ttf_parser::name_id::POST_SCRIPT_NAME {
					return false;
				}
				let name_text = name.to_string().or_else(|| {
					if name.platform_id == rustybuzz::ttf_parser::PlatformId::Macintosh {
						Some(name.name.iter().map(|byte| *byte as char).collect())
					}
					else {
						None
					}
				});
				name_text.as_deref() == Some(post_script_name)
			})
		}).unwrap_or(false)
	})
}

//==============================================================================
/// 폰트 문자열 해석. (Canvas2D font 의 CSS font 줄임 꼴, "bold 16px DOSGothic" → 크기 16, 굵기 700, 이름 DOSGothic)
/// - 크기 앞의 낱말: normal, italic / oblique(뒤의 각도는 넘긴다)는 기울임, small-caps, 늘이기 낱말(condensed 등)은 넘긴다.
///   bold / bolder 는 700, lighter 는 100, 숫자는 굵기(1 ~ 1000). 그 밖의 낱말이 오면 틀린 문자열이다.
/// - 크기: 절대 길이(px, pt, pc, in, cm, mm, q), 글꼴에 대는 길이(em, ex, ch, cap, ic, lh, rem, rex, rch, rcap, ric, rlh), %, 뷰포트 단위, 이름 크기, larger, smaller,
///   calc() 같은 CSS 수학 함수. 뒤의 "/줄높이"는 읽고 넘긴다. (캔버스는 줄높이를 쓰지 않는다) 셈은 parse_css_font_size 가 크롬과 같게 한다.
///   원본이 글을 굽고 재는 캔버스는 문서에 붙이지 않아 부모 글꼴이 크롬의 마지막 대체 글꼴(맥 Times) 10px 이고, 뿌리는 문서의 표준 글꼴 16px 이다.
///   웹은 절대 길이 밖의 크기를 브라우저 캔버스에 넣고 다시 읽어 받는다. (platform/web/systemfont.rs 의 resolve_font_size)
/// - 크기 뒤는 쉼표로 이은 이름 목록이다. 따옴표 이름은 따옴표를 떼고, 따옴표 없는 이름은 낱말을 빈칸 하나로 잇는다. 그리는 쪽이 목록의 앞 이름부터 찾는다.
/// - 틀린 문자열(크기나 이름이 없음, 음수 크기, 모르는 낱말)은 캔버스의 처음 값 "10px sans-serif" 로 본다.
///
/// # 인자
/// - `font_string`: 폰트 문자열.
///
/// # 돌려줌
/// 해석 결과. 틀린 문자열이면 크기 10, 굵기 400, 이름 sans-serif.
///
/// # 예제
/// CSS 단위, 줄높이, 틀린 문자열
/// ```rust
/// use vanilla::core::textstringtexturecache::parse_font_string;
///
/// assert_eq!(parse_font_string("12pt a").font_size, 16.0);
/// assert_eq!(parse_font_string("1in a").font_size, 96.0);
/// let line_height_description = parse_font_string("bold 16px/20px a, b");
/// assert_eq!(line_height_description.font_size, 16.0);
/// assert_eq!(line_height_description.family_name_list, vec!["a", "b"]);
/// let invalid_description = parse_font_string("16px");
/// assert_eq!(invalid_description.font_size, 10.0);
/// assert_eq!(invalid_description.family_name, "sans-serif");
/// ```
//==============================================================================
pub fn parse_font_string(font_string: &str) -> FontDescription {
	parse_font_string_strict(font_string).unwrap_or_else(|| FontDescription {
		font_size: 10.0,
		font_weight: 400,
		is_bold: false,
		is_italic: false,
		family_name: String::from("sans-serif"),
		family_name_list: vec![String::from("sans-serif")],
	})
}

//==============================================================================
/// 폰트 문자열 해석. (틀리면 None)
//==============================================================================
fn parse_font_string_strict(font_string: &str) -> Option<FontDescription> {
	let mut font_weight: u16 = 400;
	let mut is_italic = false;
	let mut remainder = font_string.trim_start();
	let mut is_after_oblique = false;
	let mut size_text = String::new();
	while !remainder.is_empty() {
		let token_end = remainder.find(char::is_whitespace).unwrap_or(remainder.len());
		let token = &remainder[..token_end];
		let lowercase_token = token.to_lowercase();
		// 크기. ("16px", "16px/1.5" 처럼 줄높이가 붙어 와도 된다. "calc(1em + 2px)" 처럼 함수면 닫는 괄호까지)
		let size_end = match find_css_function_end(remainder) {
			Some(function_end) => {
				function_end
			}
			None => {
				lowercase_token.find('/').unwrap_or(lowercase_token.len())
			}
		};
		let size_candidate = remainder[..size_end].to_lowercase();
		if is_css_font_size_text(&size_candidate) {
			size_text = size_candidate;
			remainder = remainder[size_end..].trim_start();
			// 줄높이. ("/1.5", "/ 20px", "/calc(1em + 2px)")
			if let Some(line_height_text) = remainder.strip_prefix('/') {
				let line_height_text = line_height_text.trim_start();
				let line_height_end = find_css_function_end(line_height_text).unwrap_or_else(|| line_height_text.find(char::is_whitespace).unwrap_or(line_height_text.len()));
				if line_height_end == 0 {
					return None;
				}
				remainder = line_height_text[line_height_end..].trim_start();
			}
			break;
		}
		// oblique 뒤의 각도. ("oblique 10deg")
		let is_angle = is_after_oblique && ["deg", "grad", "rad", "turn"].iter().any(|unit| lowercase_token.strip_suffix(unit).map(|number_text| number_text.parse::<f32>().is_ok()).unwrap_or(false));
		is_after_oblique = false;
		if is_angle {
			remainder = remainder[token_end..].trim_start();
			continue;
		}
		match lowercase_token.as_str() {
			"normal" | "small-caps" | "ultra-condensed" | "extra-condensed" | "condensed" | "semi-condensed" | "semi-expanded" | "expanded" | "extra-expanded" | "ultra-expanded" => {}
			"italic" => {
				is_italic = true;
			}
			"oblique" => {
				is_italic = true;
				is_after_oblique = true;
			}
			"bold" | "bolder" => {
				font_weight = 700;
			}
			"lighter" => {
				font_weight = 100;
			}
			_ => {
				if let Ok(parsed_weight) = lowercase_token.parse::<f32>() {
					if !(1.0..=1000.0).contains(&parsed_weight) {
						return None;
					}
					font_weight = parsed_weight.round() as u16;
				}
				else {
					return None;
				}
			}
		}
		remainder = remainder[token_end..].trim_start();
	}
	if size_text.is_empty() {
		return None;
	}
	let family_name_list = parse_css_family_list(remainder)?;
	// 이름 목록이 따옴표 없는 monospace 하나뿐이면 상대 크기를 고정폭 기본 크기로 줄인다. (크롬 FontBuilder 의 일반 이름 바뀜)
	let is_monospace_only = remainder.trim().eq_ignore_ascii_case("monospace");
	#[cfg(target_arch = "wasm32")]
	let font_size = if is_absolute_css_length_text(&size_text) {
		parse_css_font_size(&size_text, is_monospace_only, &FontSizeContext::create_absolute())
	}
	else {
		crate::platform::web::systemfont::resolve_font_size(font_string)
	};
	#[cfg(not(target_arch = "wasm32"))]
	let font_size = parse_css_font_size(&size_text, is_monospace_only, &FontSizeContext::create());
	let font_size = font_size?;
	Some(FontDescription {
		font_size,
		font_weight,
		is_bold: font_weight >= textshaping::BOLD_WEIGHT_THRESHOLD,
		is_italic,
		family_name: family_name_list[0].clone(),
		family_name_list,
	})
}

//==============================================================================
/// 폰트 문자열 크기의 단위가 대는 글꼴 치수 하나. (픽셀)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct FontUnitMetrics {
	font_size: f32,
	x_height: f32,
	cap_height: f32,
	zero_advance: f32,
	ideographic_advance: f32,
	line_spacing: f32,
}

impl FontUnitMetrics {
	//==============================================================================
	/// 글꼴을 찾지 못했을 때의 치수. (CSS Values 의 대신 쓰는 값: ex 와 ch 는 0.5em, ic 는 1em. cap 은 0.7em, lh 는 1.2em 으로 둔다)
	//==============================================================================
	fn create_fallback(font_size: f32) -> FontUnitMetrics {
		FontUnitMetrics {
			font_size,
			x_height: font_size * 0.5,
			cap_height: font_size * 0.7,
			zero_advance: font_size * 0.5,
			ideographic_advance: font_size,
			line_spacing: (font_size * 1.2).round(),
		}
	}
}

//==============================================================================
/// 폰트 문자열 크기를 셀 때의 기준. (원본이 글을 굽고 재는 캔버스는 문서에 붙이지 않은 HTMLCanvasElement 다. 크롬 158 에서 그 캔버스로 잰 기준)
/// - 부모: 크롬의 마지막 대체 글꼴(맥 Times) 10px. em, %, ex, ch, cap, ic, lh, larger, smaller 가 댄다.
/// - 뿌리: 문서 뿌리의 글꼴, 곧 화면 언어의 표준 글꼴(맥 한국어 Apple SD Gothic Neo, 영어 Times) 16px. rem, rex, rch, rcap, ric, rlh 가 댄다.
/// - 창: 원본 window.innerWidth, innerHeight. vw, vh, vi, vb, vmin, vmax 와 sv, lv, dv 앞붙이, 컨테이너가 없는 cq 단위가 댄다.
//==============================================================================
struct FontSizeContext {
	parent: FontUnitMetrics,
	root: FontUnitMetrics,
	viewport_width: f32,
	viewport_height: f32,
}

thread_local! {
	/// 뷰포트 단위가 대는 창 크기. (논리 픽셀, 데스크탑이 창 크기가 바뀔 때 알린다)
	static FONT_VIEWPORT_SIZE: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
	/// 부모와 뿌리 글꼴의 치수. (처음 쓸 때 한 번 센다, 데스크탑)
	static FONT_UNIT_REFERENCE: Cell<Option<(FontUnitMetrics, FontUnitMetrics)>> = const { Cell::new(None) };
}

//==============================================================================
/// 폰트 문자열의 뷰포트 단위(vw, vh 같은)가 대는 창 크기 정하기. (논리 픽셀, 원본 window.innerWidth / innerHeight 자리. 데스크탑 플랫폼이 창 크기가 바뀔 때 부른다)
///
/// # 인자
/// - `width`: 창 너비. (논리 픽셀)
/// - `height`: 창 높이. (논리 픽셀)
//==============================================================================
pub fn set_font_viewport_size(width: f32, height: f32) {
	FONT_VIEWPORT_SIZE.with(|viewport_size| viewport_size.set((width, height)));
}

impl FontSizeContext {
	//==============================================================================
	/// 지금의 기준. (데스크탑은 부모와 뿌리 글꼴을 운영체제에서 찾아 처음 한 번 센다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn create() -> FontSizeContext {
		let (parent, root) = FONT_UNIT_REFERENCE.with(|reference_cell| {
			if let Some(reference) = reference_cell.get() {
				return reference;
			}
			let parent = crate::platform::desktop::systemfont::find_last_resort_font_file().and_then(|(source, family_name)| read_font_unit_metrics(&source, &family_name, PARENT_FONT_SIZE)).unwrap_or_else(|| FontUnitMetrics::create_fallback(PARENT_FONT_SIZE));
			let root = crate::platform::desktop::systemfont::resolve_generic_family_name(crate::platform::desktop::systemfont::GenericFamily::Standard).and_then(|family_name| crate::platform::find_family_font_file(&family_name, 400, false).and_then(|source| read_font_unit_metrics(&source, &family_name, ROOT_FONT_SIZE))).unwrap_or_else(|| FontUnitMetrics::create_fallback(ROOT_FONT_SIZE));
			reference_cell.set(Some((parent, root)));
			(parent, root)
		});
		let (viewport_width, viewport_height) = FONT_VIEWPORT_SIZE.with(|viewport_size| viewport_size.get());
		FontSizeContext {
			parent,
			root,
			viewport_width,
			viewport_height,
		}
	}

	//==============================================================================
	/// 절대 길이만 셀 때의 기준. (웹은 그 밖의 크기를 브라우저에 맡긴다)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	fn create_absolute() -> FontSizeContext {
		FontSizeContext {
			parent: FontUnitMetrics::create_fallback(PARENT_FONT_SIZE),
			root: FontUnitMetrics::create_fallback(ROOT_FONT_SIZE),
			viewport_width: 0.0,
			viewport_height: 0.0,
		}
	}

	//==============================================================================
	/// 단위 1 의 픽셀과 고정폭 배율을 받는지. (모르는 단위면 None. % 는 따로 센다)
	//==============================================================================
	fn read_unit_pixel(&self, unit: &str) -> Option<(f32, bool)> {
		let viewport_minimum = self.viewport_width.min(self.viewport_height);
		let viewport_maximum = self.viewport_width.max(self.viewport_height);
		let unit_pixel = match unit {
			"px" => {
				(1.0, false)
			}
			"pt" => {
				(96.0 / 72.0, false)
			}
			"pc" => {
				(16.0, false)
			}
			"in" => {
				(96.0, false)
			}
			"cm" => {
				(96.0 / 2.54, false)
			}
			"mm" => {
				(96.0 / 25.4, false)
			}
			"q" => {
				(96.0 / 101.6, false)
			}
			"em" => {
				(self.parent.font_size, true)
			}
			"ex" => {
				(self.parent.x_height, true)
			}
			"ch" => {
				(self.parent.zero_advance, true)
			}
			"cap" => {
				(self.parent.cap_height, true)
			}
			"ic" => {
				(self.parent.ideographic_advance, true)
			}
			"lh" => {
				(self.parent.line_spacing, true)
			}
			"rem" => {
				(self.root.font_size, false)
			}
			"rex" => {
				(self.root.x_height, true)
			}
			"rch" => {
				(self.root.zero_advance, true)
			}
			"rcap" => {
				(self.root.cap_height, true)
			}
			"ric" => {
				(self.root.ideographic_advance, true)
			}
			"rlh" => {
				(self.root.line_spacing, true)
			}
			"vw" | "svw" | "lvw" | "dvw" | "vi" | "svi" | "lvi" | "dvi" | "cqw" | "cqi" => {
				(self.viewport_width / 100.0, false)
			}
			"vh" | "svh" | "lvh" | "dvh" | "vb" | "svb" | "lvb" | "dvb" | "cqh" | "cqb" => {
				(self.viewport_height / 100.0, false)
			}
			"vmin" | "svmin" | "lvmin" | "dvmin" | "cqmin" => {
				(viewport_minimum / 100.0, false)
			}
			"vmax" | "svmax" | "lvmax" | "dvmax" | "cqmax" => {
				(viewport_maximum / 100.0, false)
			}
			_ => {
				return None;
			}
		};
		Some(unit_pixel)
	}
}

/// 문서에 붙이지 않은 캔버스의 부모 글꼴 크기. (캔버스 처음 값 "10px sans-serif")
const PARENT_FONT_SIZE: f32 = 10.0;
/// 문서 뿌리의 글꼴 크기. (크롬 기본 글꼴 크기 16)
const ROOT_FONT_SIZE: f32 = 16.0;
/// 고정폭 일반 이름의 상대 크기 배율. (크롬 기본 고정폭 글꼴 크기 13 / 기본 글꼴 크기 16)
const MONOSPACE_SIZE_RATIO: f32 = 13.0 / 16.0;
/// 글자 크기의 상한. (크롬 FontDescription 의 최대 글자 크기 10000)
const MAXIMUM_FONT_SIZE: f32 = 10000.0;

//==============================================================================
/// 글꼴 파일의 단위 치수. (크롬 SimpleFontData::PlatformInit 과 같은 셈, 읽지 못하면 None)
/// - ex: 맥은 'x' 글리프 상자의 위 (CTFontGetXHeight 대신 글리프를 잰다), 그 밖은 OS/2 의 x 높이, 없으면 'x' 상자의 위, 그것도 없으면 올라간 길이 x 0.56.
/// - cap: OS/2 의 대문자 높이, 없으면 'H' 상자의 위. ch: '0' 의 전진폭, 없으면 0.5em. ic: '水'(U+6C34) 의 전진폭, 없으면 1em.
/// - lh: 줄높이 normal = 반올림한 올라간 길이 + 내려간 길이 + 줄 사이. 맥은 hhea, 윈도우는 OS/2 USE_TYPO_METRICS 면 typo, 아니면 win 값,
///   리눅스는 hhea 다. 맥은 Times, Helvetica, Courier 의 올라간 길이에 (올라간 + 내려간) x 0.15 를 반올림해 더한다. (font_metrics.cc 의 AscentDescentWithHacks)
///   윈도우와 리눅스는 크롬과 견주어 보지 못했다.
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_font_unit_metrics(source: &FontFileSource, family_name: &str, font_size: f32) -> Option<FontUnitMetrics> {
	let font_bytes = std::fs::read(&source.path).ok()?;
	let face_index = match source.face_index {
		Some(face_index) => {
			face_index
		}
		None => {
			source.post_script_name.as_ref().and_then(|post_script_name| find_post_script_face_index(&font_bytes, post_script_name)).unwrap_or(0)
		}
	};
	let face = rustybuzz::ttf_parser::Face::parse(&font_bytes, face_index).ok()?;
	let scale = font_size / face.units_per_em() as f32;
	let read_glyph_top = |character: char| -> Option<f32> {
		let glyph_id = face.glyph_index(character)?;
		face.glyph_bounding_box(glyph_id).map(|glyph_rect| glyph_rect.y_max as f32 * scale)
	};
	let read_glyph_advance = |character: char| -> Option<f32> {
		let glyph_id = face.glyph_index(character)?;
		face.glyph_hor_advance(glyph_id).map(|advance| advance as f32 * scale)
	};
	let hhea_table = face.tables().hhea;
	let os2_table = face.tables().os2;
	let (ascent_units, descent_units, line_gap_units) = if cfg!(target_os = "windows") {
		match os2_table {
			Some(os2_table) if os2_table.use_typographic_metrics() => {
				(os2_table.typographic_ascender() as f32, -(os2_table.typographic_descender() as f32), os2_table.typographic_line_gap() as f32)
			}
			Some(os2_table) => {
				let windows_ascent = os2_table.windows_ascender() as f32;
				let windows_descent = os2_table.windows_descender().unsigned_abs() as f32;
				let hhea_height = hhea_table.ascender as f32 - hhea_table.descender as f32 + hhea_table.line_gap as f32;
				(windows_ascent, windows_descent, (hhea_height - windows_ascent - windows_descent).max(0.0))
			}
			None => {
				(hhea_table.ascender as f32, -(hhea_table.descender as f32), hhea_table.line_gap as f32)
			}
		}
	}
	else {
		(hhea_table.ascender as f32, -(hhea_table.descender as f32), hhea_table.line_gap as f32)
	};
	let mut ascent = (ascent_units * scale + 0.5).floor();
	let descent = (descent_units * scale + 0.5).floor();
	if cfg!(target_os = "macos") && matches!(family_name, "Times" | "Helvetica" | "Courier") {
		ascent += ((ascent + descent) * 0.15 + 0.5).floor();
	}
	let line_gap = line_gap_units * scale;
	let os2_x_height = os2_table.and_then(|os2_table| os2_table.x_height()).filter(|x_height| *x_height > 0).map(|x_height| x_height as f32 * scale);
	let x_height = if cfg!(target_os = "macos") {
		read_glyph_top('x').or(os2_x_height)
	}
	else {
		os2_x_height.or_else(|| read_glyph_top('x'))
	}.unwrap_or(ascent * 0.56);
	let cap_height = os2_table.and_then(|os2_table| os2_table.capital_height()).filter(|cap_height| *cap_height > 0).map(|cap_height| cap_height as f32 * scale).or_else(|| read_glyph_top('H')).unwrap_or(ascent);
	Some(FontUnitMetrics {
		font_size,
		x_height,
		cap_height,
		zero_advance: read_glyph_advance('0').unwrap_or(font_size * 0.5),
		ideographic_advance: read_glyph_advance('\u{6C34}').unwrap_or(font_size),
		line_spacing: ascent.round() + descent.round() + line_gap.round(),
	})
}

//==============================================================================
/// 글 맨 앞이 CSS 함수면 닫는 괄호 다음 자리. ("calc(1px + 2px) a" → 15, 함수가 아니거나 괄호가 닫히지 않으면 None)
//==============================================================================
fn find_css_function_end(text: &str) -> Option<usize> {
	let name_end = text.find('(')?;
	let function_name = &text[..name_end];
	if function_name.is_empty() || !function_name.chars().all(|character| character.is_ascii_alphabetic() || character == '-') {
		return None;
	}
	let mut depth = 0;
	for (byte_index, character) in text.char_indices().skip(name_end) {
		if character == '(' {
			depth += 1;
		}
		else if character == ')' {
			depth -= 1;
			if depth == 0 {
				return Some(byte_index + 1);
			}
		}
	}
	None
}

//==============================================================================
/// 글자 크기 자리의 글인지. (이름 크기, 길이 단위나 % 가 붙은 수, CSS 함수. 셈은 parse_css_font_size 가 한다. oblique 뒤의 "10deg" 같은 각도는 크기가 아니다)
//==============================================================================
fn is_css_font_size_text(size_text: &str) -> bool {
	if matches!(size_text, "xx-small" | "x-small" | "small" | "medium" | "large" | "x-large" | "xx-large" | "xxx-large" | "larger" | "smaller") {
		return true;
	}
	if find_css_function_end(size_text) == Some(size_text.len()) {
		return true;
	}
	const UNIT_LIST: [&str; 50] = ["px", "pt", "pc", "in", "cm", "mm", "q", "%", "em", "ex", "ch", "cap", "ic", "lh", "rem", "rex", "rch", "rcap", "ric", "rlh", "vw", "vh", "vi", "vb", "vmin", "vmax", "svw", "svh", "svi", "svb", "svmin", "svmax", "lvw", "lvh", "lvi", "lvb", "lvmin", "lvmax", "dvw", "dvh", "dvi", "dvb", "dvmin", "dvmax", "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax"];
	match parse_css_number_prefix(size_text) {
		Some((_, unit_text)) => {
			UNIT_LIST.contains(&unit_text)
		}
		None => {
			false
		}
	}
}

//==============================================================================
/// 절대 길이인지. (px, pt, pc, in, cm, mm, q 가 붙은 수)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn is_absolute_css_length_text(size_text: &str) -> bool {
	match parse_css_number_prefix(size_text) {
		Some((_, unit_text)) => {
			matches!(unit_text, "px" | "pt" | "pc" | "in" | "cm" | "mm" | "q")
		}
		None => {
			false
		}
	}
}

//==============================================================================
/// 글 맨 앞의 CSS 수와 나머지. ("+1.5e1px" → (15, "px"), 수가 아니면 None)
/// - CSS 수의 꼴: 부호, 숫자(소수점 뒤에 숫자가 있어야 한다, ".5" 는 되고 "2." 는 안 된다), 지수.
//==============================================================================
fn parse_css_number_prefix(text: &str) -> Option<(f64, &str)> {
	let bytes = text.as_bytes();
	let mut position = 0;
	if position < bytes.len() && (bytes[position] == b'+' || bytes[position] == b'-') {
		position += 1;
	}
	let integer_start = position;
	while position < bytes.len() && bytes[position].is_ascii_digit() {
		position += 1;
	}
	let mut has_digit = position > integer_start;
	if position + 1 < bytes.len() && bytes[position] == b'.' && bytes[position + 1].is_ascii_digit() {
		position += 1;
		while position < bytes.len() && bytes[position].is_ascii_digit() {
			position += 1;
		}
		has_digit = true;
	}
	if !has_digit {
		return None;
	}
	if position < bytes.len() && (bytes[position] == b'e' || bytes[position] == b'E') {
		let mut exponent_position = position + 1;
		if exponent_position < bytes.len() && (bytes[exponent_position] == b'+' || bytes[exponent_position] == b'-') {
			exponent_position += 1;
		}
		if exponent_position < bytes.len() && bytes[exponent_position].is_ascii_digit() {
			while exponent_position < bytes.len() && bytes[exponent_position].is_ascii_digit() {
				exponent_position += 1;
			}
			position = exponent_position;
		}
	}
	let number = text[..position].parse::<f64>().ok()?;
	Some((number, &text[position..]))
}

//==============================================================================
/// CSS 글자 크기 하나 읽기. (픽셀, 소문자 글. 크기가 아니거나 음수면 None. 크롬 158 의 문서에 붙이지 않은 캔버스와 같은 셈)
/// - 이름 크기: xx-small 9, x-small 10, small 13, medium 16, large 18, x-large 24, xx-large 32, xxx-large 48. 고정폭은 9, 10, 12, 13, 16, 20, 26, 39.
///   larger 와 smaller 는 부모 10px 의 1.2 배와 1 / 1.2 배.
/// - 단위: px, pt, pc, in, cm, mm, q 와 em, %, ex, ch, cap, ic, lh(부모 글꼴), rem, rex, rch, rcap, ric, rlh(뿌리 글꼴), 뷰포트 단위. (FontSizeContext)
/// - 이름 목록이 따옴표 없는 monospace 하나뿐이면 부모와 뿌리 글꼴에 대는 단위(rem 은 빼고)와 %, larger, smaller 에 13 / 16 을 곱한다.
/// - calc(), min(), max(), clamp(), abs(), sign(), round(), mod(), rem(), hypot(), sqrt(), pow(), exp(), log() 를 셈한다. (evaluate_css_math_function)
///   함수 안의 길이에는 고정폭 배율을 걸지 않고, 퍼센트만으로 된 값이면 건다. 음수는 0 으로 자르고 수가 아니면(NaN) 0 이다.
/// - 10000 을 넘으면 10000 이다.
//==============================================================================
fn parse_css_font_size(size_text: &str, is_monospace_only: bool, font_size_context: &FontSizeContext) -> Option<f32> {
	const KEYWORD_LIST: [&str; 8] = ["xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "xxx-large"];
	const KEYWORD_SIZE_TABLE: [f32; 8] = [9.0, 10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0];
	const MONOSPACE_KEYWORD_SIZE_TABLE: [f32; 8] = [9.0, 10.0, 12.0, 13.0, 16.0, 20.0, 26.0, 39.0];
	let relative_scale = if is_monospace_only {
		MONOSPACE_SIZE_RATIO
	}
	else {
		1.0
	};
	if let Some(keyword_index) = KEYWORD_LIST.iter().position(|keyword| *keyword == size_text) {
		return Some(if is_monospace_only {
			MONOSPACE_KEYWORD_SIZE_TABLE[keyword_index]
		}
		else {
			KEYWORD_SIZE_TABLE[keyword_index]
		});
	}
	match size_text {
		"larger" => {
			return Some(font_size_context.parent.font_size * 1.2 * relative_scale);
		}
		"smaller" => {
			return Some(font_size_context.parent.font_size / 1.2 * relative_scale);
		}
		_ => {}
	}
	if find_css_function_end(size_text) == Some(size_text.len()) {
		let mut math_parser = CssMathParser {
			text: size_text,
			position: 0,
			font_size_context,
			is_angle: false,
		};
		let math_value = math_parser.parse_function()?;
		if math_parser.position != size_text.len() {
			return None;
		}
		let (pixels, is_pure_percent) = match math_value {
			CssMathValue::Length { pixels, is_pure_percent } => {
				(pixels, is_pure_percent)
			}
			CssMathValue::Number(_) => {
				return None;
			}
		};
		let scaled_pixels = if is_pure_percent {
			pixels * relative_scale as f64
		}
		else {
			pixels
		};
		if scaled_pixels.is_nan() {
			return Some(0.0);
		}
		return Some(scaled_pixels.clamp(0.0, MAXIMUM_FONT_SIZE as f64) as f32);
	}
	let (number, unit_text) = parse_css_number_prefix(size_text)?;
	if number < 0.0 {
		return None;
	}
	let pixels = if unit_text == "%" {
		number as f32 * font_size_context.parent.font_size / 100.0 * relative_scale
	}
	else {
		let (unit_pixel, is_relative) = font_size_context.read_unit_pixel(unit_text)?;
		let scale = if is_relative {
			relative_scale
		}
		else {
			1.0
		};
		number as f32 * unit_pixel * scale
	};
	Some(pixels.min(MAXIMUM_FONT_SIZE))
}

//==============================================================================
/// 캔버스 font 의 글자 크기 셈. (픽셀, parse_css_font_size 를 데스크탑의 지금 기준으로 부른다. 크기가 아니거나 음수면 None)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn compute_canvas_font_size(size_text: &str, is_monospace_only: bool) -> Option<f32> {
	parse_css_font_size(size_text, is_monospace_only, &FontSizeContext::create())
}

//==============================================================================
/// CSS 수학 함수 하나를 수로 셈. ("calc(300 + 400)" 은 700, 수가 아니거나 틀리면 None. NaN 은 그대로 돌려준다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn evaluate_css_math_number(function_text: &str) -> Option<f64> {
	match evaluate_css_math_function(function_text, false)? {
		CssMathValue::Number(number) => {
			Some(number)
		}
		CssMathValue::Length { .. } => {
			None
		}
	}
}

//==============================================================================
/// CSS 수학 함수 하나를 각도로 셈. (도, "calc(7deg * 2)" 는 14. 각도가 아니거나 틀리면 None)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn evaluate_css_math_angle(function_text: &str) -> Option<f64> {
	match evaluate_css_math_function(function_text, true)? {
		CssMathValue::Length { pixels, .. } => {
			Some(pixels)
		}
		CssMathValue::Number(_) => {
			None
		}
	}
}

//==============================================================================
/// CSS 수학 함수 하나 셈. (글 전체가 함수 하나여야 한다, 틀리면 None)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn evaluate_css_math_function(function_text: &str, is_angle: bool) -> Option<CssMathValue> {
	if find_css_function_end(function_text) != Some(function_text.len()) {
		return None;
	}
	let font_size_context = FontSizeContext::create();
	let mut math_parser = CssMathParser {
		text: function_text,
		position: 0,
		font_size_context: &font_size_context,
		is_angle,
	};
	let math_value = math_parser.parse_function()?;
	if math_parser.position != function_text.len() {
		return None;
	}
	Some(math_value)
}

//==============================================================================
/// CSS 수학 함수의 값. (수, 또는 길이(픽셀로 푼 값과 퍼센트로만 된 값인지))
//==============================================================================
#[derive(Clone, Copy, Debug)]
enum CssMathValue {
	Number(f64),
	Length {
		pixels: f64,
		is_pure_percent: bool,
	},
}

//==============================================================================
/// CSS 수학 함수 해석기. (CSS Values 4 의 calc 문법: 더하기와 빼기는 앞뒤에 빈칸이 있어야 하고, 곱하기와 나누기는 없어도 된다)
/// - 퍼센트는 부모 글자 크기에 댄다. 형이 맞지 않으면(길이 + 수, 길이 x 길이) 틀린 값이다.
/// - 각도 셈이면 deg, grad, rad, turn 을 도로 풀어 Length 칸에 담고, 길이 단위와 퍼센트는 틀린 값이다. (캔버스 font 의 oblique 각도)
//==============================================================================
struct CssMathParser<'text> {
	text: &'text str,
	position: usize,
	font_size_context: &'text FontSizeContext,
	/// 각도 셈인지.
	is_angle: bool,
}

impl CssMathParser<'_> {
	//==============================================================================
	/// 빈칸 넘기기. (넘긴 것이 있으면 true)
	//==============================================================================
	fn skip_whitespace(&mut self) -> bool {
		let start_position = self.position;
		while self.position < self.text.len() && self.text.as_bytes()[self.position].is_ascii_whitespace() {
			self.position += 1;
		}
		self.position > start_position
	}

	//==============================================================================
	/// 다음 글자가 그것이면 먹기.
	//==============================================================================
	fn consume_byte(&mut self, expected_byte: u8) -> bool {
		if self.position < self.text.len() && self.text.as_bytes()[self.position] == expected_byte {
			self.position += 1;
			return true;
		}
		false
	}

	//==============================================================================
	/// 이름 하나 읽기. (영문자와 하이픈, 소문자로)
	//==============================================================================
	fn read_identifier(&mut self) -> String {
		let start_position = self.position;
		while self.position < self.text.len() {
			let byte = self.text.as_bytes()[self.position];
			if byte.is_ascii_alphabetic() || (byte == b'-' && self.position > start_position) {
				self.position += 1;
			}
			else {
				break;
			}
		}
		self.text[start_position..self.position].to_ascii_lowercase()
	}

	//==============================================================================
	/// 함수 하나. ("이름(" 부터 닫는 괄호까지)
	//==============================================================================
	fn parse_function(&mut self) -> Option<CssMathValue> {
		let function_name = self.read_identifier();
		if !self.consume_byte(b'(') {
			return None;
		}
		let argument_list = match function_name.as_str() {
			"round" => {
				return self.parse_round_function();
			}
			"clamp" => {
				return self.parse_clamp_function();
			}
			_ => {
				self.parse_argument_list()?
			}
		};
		match (function_name.as_str(), argument_list.as_slice()) {
			("calc", [value]) => {
				Some(*value)
			}
			("min", _) | ("max", _) if !argument_list.is_empty() => {
				let is_minimum = function_name == "min";
				let mut result = argument_list[0];
				for value in &argument_list[1..] {
					result = combine_css_math_values(result, *value, |first, second| {
						if is_minimum {
							first.min(second)
						}
						else {
							first.max(second)
						}
					})?;
				}
				Some(result)
			}
			("abs", [value]) => {
				Some(map_css_math_value(*value, f64::abs))
			}
			("sign", [value]) => {
				let number = read_css_math_number(*value);
				Some(CssMathValue::Number(if number.is_nan() || number == 0.0 {
					number
				}
				else {
					number.signum()
				}))
			}
			("mod", [dividend, divisor]) | ("rem", [dividend, divisor]) => {
				let is_modulo = function_name == "mod";
				combine_css_math_values(*dividend, *divisor, |first, second| {
					let quotient = first / second;
					let rounded_quotient = if is_modulo {
						quotient.floor()
					}
					else {
						quotient.trunc()
					};
					first - second * rounded_quotient
				})
			}
			("hypot", _) if !argument_list.is_empty() => {
				let mut result = map_css_math_value(argument_list[0], |number| number * number);
				for value in &argument_list[1..] {
					result = combine_css_math_values(result, map_css_math_value(*value, |number| number * number), |first, second| first + second)?;
				}
				Some(map_css_math_value(result, f64::sqrt))
			}
			("sqrt", [CssMathValue::Number(number)]) => {
				Some(CssMathValue::Number(number.sqrt()))
			}
			("exp", [CssMathValue::Number(number)]) => {
				Some(CssMathValue::Number(number.exp()))
			}
			("pow", [CssMathValue::Number(base), CssMathValue::Number(exponent)]) => {
				Some(CssMathValue::Number(base.powf(*exponent)))
			}
			("log", [CssMathValue::Number(number)]) => {
				Some(CssMathValue::Number(number.ln()))
			}
			("log", [CssMathValue::Number(number), CssMathValue::Number(base)]) => {
				Some(CssMathValue::Number(number.ln() / base.ln()))
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 쉼표로 이은 인자들과 닫는 괄호.
	//==============================================================================
	fn parse_argument_list(&mut self) -> Option<Vec<CssMathValue>> {
		let mut argument_list = Vec::new();
		loop {
			self.skip_whitespace();
			argument_list.push(self.parse_sum()?);
			self.skip_whitespace();
			if self.consume_byte(b',') {
				continue;
			}
			if self.consume_byte(b')') {
				return Some(argument_list);
			}
			return None;
		}
	}

	//==============================================================================
	/// round(방식?, 값, 단위?). (방식: nearest(처음, 반은 위로), up, down, to-zero. 단위를 빼면 값이 수여야 하고 1 로 본다)
	//==============================================================================
	fn parse_round_function(&mut self) -> Option<CssMathValue> {
		self.skip_whitespace();
		let saved_position = self.position;
		let strategy = self.read_identifier();
		let strategy = if matches!(strategy.as_str(), "nearest" | "up" | "down" | "to-zero") && {
			self.skip_whitespace();
			self.consume_byte(b',')
		} {
			strategy
		}
		else {
			self.position = saved_position;
			String::from("nearest")
		};
		let argument_list = self.parse_argument_list()?;
		let (value, step) = match argument_list.as_slice() {
			[value] => {
				match value {
					CssMathValue::Number(_) => {
						(*value, CssMathValue::Number(1.0))
					}
					_ => {
						return None;
					}
				}
			}
			[value, step] => {
				(*value, *step)
			}
			_ => {
				return None;
			}
		};
		combine_css_math_values(value, step, |number, step_number| {
			let quotient = number / step_number;
			let rounded_quotient = match strategy.as_str() {
				"up" => {
					quotient.ceil()
				}
				"down" => {
					quotient.floor()
				}
				"to-zero" => {
					quotient.trunc()
				}
				_ => {
					(quotient + 0.5).floor()
				}
			};
			rounded_quotient * step_number
		})
	}

	//==============================================================================
	/// clamp(최소, 값, 최대). (최소나 최대 자리에 none 이 오면 그쪽은 자르지 않는다. 최소가 최대보다 크면 최소)
	//==============================================================================
	fn parse_clamp_function(&mut self) -> Option<CssMathValue> {
		let mut bound_list: Vec<Option<CssMathValue>> = Vec::new();
		for argument_index in 0..3 {
			self.skip_whitespace();
			let saved_position = self.position;
			let is_none = self.read_identifier() == "none" && argument_index != 1;
			if is_none {
				bound_list.push(None);
			}
			else {
				self.position = saved_position;
				bound_list.push(Some(self.parse_sum()?));
			}
			self.skip_whitespace();
			let separator = if argument_index < 2 {
				b','
			}
			else {
				b')'
			};
			if !self.consume_byte(separator) {
				return None;
			}
		}
		let mut result = bound_list[1]?;
		if let Some(maximum_value) = bound_list[2] {
			result = combine_css_math_values(result, maximum_value, f64::min)?;
		}
		if let Some(minimum_value) = bound_list[0] {
			result = combine_css_math_values(result, minimum_value, f64::max)?;
		}
		Some(result)
	}

	//==============================================================================
	/// 더하기, 빼기. (연산자 앞뒤에 빈칸이 있어야 한다)
	//==============================================================================
	fn parse_sum(&mut self) -> Option<CssMathValue> {
		let mut result = self.parse_product()?;
		loop {
			let saved_position = self.position;
			if !self.skip_whitespace() {
				return Some(result);
			}
			let operator = if self.consume_byte(b'+') {
				b'+'
			}
			else if self.consume_byte(b'-') {
				b'-'
			}
			else {
				self.position = saved_position;
				return Some(result);
			};
			if !self.skip_whitespace() {
				return None;
			}
			let operand = self.parse_product()?;
			result = combine_css_math_values(result, operand, |first, second| {
				if operator == b'+' {
					first + second
				}
				else {
					first - second
				}
			})?;
		}
	}

	//==============================================================================
	/// 곱하기, 나누기. (길이 x 수, 수 x 길이, 길이 / 수, 길이 / 길이 = 수)
	//==============================================================================
	fn parse_product(&mut self) -> Option<CssMathValue> {
		let mut result = self.parse_value()?;
		loop {
			let saved_position = self.position;
			self.skip_whitespace();
			let is_multiply = if self.consume_byte(b'*') {
				true
			}
			else if self.consume_byte(b'/') {
				false
			}
			else {
				self.position = saved_position;
				return Some(result);
			};
			self.skip_whitespace();
			let operand = self.parse_value()?;
			result = match (result, operand, is_multiply) {
				(CssMathValue::Number(first), CssMathValue::Number(second), true) => {
					CssMathValue::Number(first * second)
				}
				(CssMathValue::Number(first), CssMathValue::Number(second), false) => {
					CssMathValue::Number(first / second)
				}
				(CssMathValue::Length { pixels, is_pure_percent }, CssMathValue::Number(number), true) | (CssMathValue::Number(number), CssMathValue::Length { pixels, is_pure_percent }, true) => {
					CssMathValue::Length {
						pixels: pixels * number,
						is_pure_percent,
					}
				}
				(CssMathValue::Length { pixels, is_pure_percent }, CssMathValue::Number(number), false) => {
					CssMathValue::Length {
						pixels: pixels / number,
						is_pure_percent,
					}
				}
				(CssMathValue::Length { pixels: first_pixels, .. }, CssMathValue::Length { pixels: second_pixels, .. }, false) => {
					CssMathValue::Number(first_pixels / second_pixels)
				}
				_ => {
					return None;
				}
			};
		}
	}

	//==============================================================================
	/// 값 하나. (단위가 붙은 수, 괄호, 함수, 상수 pi, e, infinity, -infinity, nan)
	//==============================================================================
	fn parse_value(&mut self) -> Option<CssMathValue> {
		if self.consume_byte(b'(') {
			self.skip_whitespace();
			let value = self.parse_sum()?;
			self.skip_whitespace();
			if !self.consume_byte(b')') {
				return None;
			}
			return Some(value);
		}
		let remainder = &self.text[self.position..];
		if let Some((number, unit_remainder)) = parse_css_number_prefix(remainder) {
			let unit_length = unit_remainder.bytes().take_while(|byte| byte.is_ascii_alphabetic() || *byte == b'%').count();
			let unit_text = &unit_remainder[..unit_length];
			self.position += remainder.len() - unit_remainder.len() + unit_length;
			if unit_text.is_empty() {
				return Some(CssMathValue::Number(number));
			}
			if self.is_angle {
				let unit_degree = match unit_text.to_ascii_lowercase().as_str() {
					"deg" => {
						1.0
					}
					"grad" => {
						0.9
					}
					"rad" => {
						180.0 / std::f64::consts::PI
					}
					"turn" => {
						360.0
					}
					_ => {
						return None;
					}
				};
				return Some(CssMathValue::Length {
					pixels: number * unit_degree,
					is_pure_percent: false,
				});
			}
			if unit_text == "%" {
				return Some(CssMathValue::Length {
					pixels: number * self.font_size_context.parent.font_size as f64 / 100.0,
					is_pure_percent: true,
				});
			}
			let (unit_pixel, _) = self.font_size_context.read_unit_pixel(&unit_text.to_ascii_lowercase())?;
			return Some(CssMathValue::Length {
				pixels: number * unit_pixel as f64,
				is_pure_percent: false,
			});
		}
		let saved_position = self.position;
		let is_negative = self.consume_byte(b'-');
		let identifier = self.read_identifier();
		if self.position < self.text.len() && self.text.as_bytes()[self.position] == b'(' && !is_negative {
			self.position = saved_position;
			return self.parse_function();
		}
		let constant = match identifier.as_str() {
			"pi" if !is_negative => {
				std::f64::consts::PI
			}
			"e" if !is_negative => {
				std::f64::consts::E
			}
			"infinity" => {
				f64::INFINITY
			}
			"nan" if !is_negative => {
				f64::NAN
			}
			_ => {
				return None;
			}
		};
		Some(CssMathValue::Number(if is_negative {
			-constant
		}
		else {
			constant
		}))
	}
}

//==============================================================================
/// 같은 형의 두 값 합치기. (수와 수, 길이와 길이. 형이 다르면 None, 퍼센트로만 된 값인지는 둘 다 그럴 때만)
//==============================================================================
fn combine_css_math_values(first_value: CssMathValue, second_value: CssMathValue, combine: impl Fn(f64, f64) -> f64) -> Option<CssMathValue> {
	match (first_value, second_value) {
		(CssMathValue::Number(first), CssMathValue::Number(second)) => {
			Some(CssMathValue::Number(combine(first, second)))
		}
		(CssMathValue::Length { pixels: first_pixels, is_pure_percent: is_first_pure_percent }, CssMathValue::Length { pixels: second_pixels, is_pure_percent: is_second_pure_percent }) => {
			Some(CssMathValue::Length {
				pixels: combine(first_pixels, second_pixels),
				is_pure_percent: is_first_pure_percent && is_second_pure_percent,
			})
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 값의 수에 함수 걸기. (형은 그대로)
//==============================================================================
fn map_css_math_value(value: CssMathValue, map: impl Fn(f64) -> f64) -> CssMathValue {
	match value {
		CssMathValue::Number(number) => {
			CssMathValue::Number(map(number))
		}
		CssMathValue::Length { pixels, is_pure_percent } => {
			CssMathValue::Length {
				pixels: map(pixels),
				is_pure_percent,
			}
		}
	}
}

//==============================================================================
/// 값의 수. (길이는 픽셀)
//==============================================================================
fn read_css_math_number(value: CssMathValue) -> f64 {
	match value {
		CssMathValue::Number(number) => {
			number
		}
		CssMathValue::Length { pixels, .. } => {
			pixels
		}
	}
}

//==============================================================================
/// CSS 글꼴 이름 목록 읽기. ("'A B', C D, sans-serif" → ["A B", "C D", "sans-serif"], 비었거나 틀리면 None)
/// - 따옴표 친 이름이 일반 이름과 같은 글자면 따옴표째 둔다. ("'monospace'" → "\"monospace\"", 크롬은 따옴표 친 이름을 글꼴 이름으로 본다)
//==============================================================================
fn parse_css_family_list(family_list_text: &str) -> Option<Vec<String>> {
	let mut family_name_list: Vec<String> = Vec::new();
	let mut characters = family_list_text.chars().peekable();
	loop {
		while characters.peek().map(|character| character.is_whitespace()).unwrap_or(false) {
			characters.next();
		}
		let family_name = match characters.peek().copied() {
			Some(quote) if quote == '"' || quote == '\'' => {
				characters.next();
				let mut quoted_name = String::new();
				loop {
					match characters.next() {
						Some(character) if character == quote => {
							break;
						}
						Some(character) => {
							quoted_name.push(character);
						}
						None => {
							return None;
						}
					}
				}
				while characters.peek().map(|character| character.is_whitespace()).unwrap_or(false) {
					characters.next();
				}
				if !matches!(characters.peek(), None | Some(',')) {
					return None;
				}
				if is_generic_family_keyword(&quoted_name) {
					format!("\"{}\"", quoted_name)
				}
				else {
					quoted_name
				}
			}
			_ => {
				let mut unquoted_text = String::new();
				while let Some(character) = characters.peek().copied() {
					if character == ',' {
						break;
					}
					if character == '"' || character == '\'' {
						return None;
					}
					unquoted_text.push(character);
					characters.next();
				}
				let word_list: Vec<&str> = unquoted_text.split_whitespace().collect();
				if word_list.is_empty() {
					return None;
				}
				word_list.join(" ")
			}
		};
		family_name_list.push(family_name);
		match characters.next() {
			Some(',') => {
				continue;
			}
			_ => {
				break;
			}
		}
	}
	Some(family_name_list)
}

//==============================================================================
/// 따옴표 없이 적으면 일반 이름이 되는 낱말인지. (블링크 ConsumeGenericFamily 의 serif 부터 math 까지, 대소문자를 가리지 않는다)
//==============================================================================
fn is_generic_family_keyword(family_name: &str) -> bool {
	matches!(family_name.to_ascii_lowercase().as_str(), "serif" | "sans-serif" | "cursive" | "fantasy" | "monospace" | "system-ui" | "-webkit-body" | "math")
}

//==============================================================================
/// 이름 목록의 이름 하나로 올린 글꼴 얼굴을 찾는 열쇠. (따옴표 없는 일반 이름이면 None)
/// - 크롬 CSSFontSelector::GetFontData 는 일반 이름에 @font-face 와 FontFace 를 찾지 않는다. 따옴표 친 일반 이름은 그 이름의 얼굴을 찾는다.
/// - 맥 크롬은 BlinkMacSystemFont 를 이름 "system-ui" 로 바꿔 찾는다. (style_builder_converter.cc 의 ConvertFontFamily)
//==============================================================================
fn read_registered_family_key(family_name: &str) -> Option<String> {
	if is_generic_family_keyword(family_name) {
		return None;
	}
	let family_key = create_family_key(family_name);
	if cfg!(target_os = "macos") && family_key == "blinkmacsystemfont" {
		return Some(String::from("system-ui"));
	}
	Some(family_key)
}

//==============================================================================
/// 폰트 읽기. (웹 글꼴이면 풀고, 래스터와 모양 짓기 얼굴을 만든다)
//==============================================================================
fn load_font(font_bytes: Vec<u8>) -> Result<LoadedFont, String> {
	let font_bytes = decompress_web_font(font_bytes)?;
	LoadedFont::create_from_bytes(font_bytes)
}

//==============================================================================
/// 웹 글꼴이면 TTF 로 풀기. (머리 4 바이트가 "wOFF" 면 WOFF, "wOF2" 면 WOFF2. 아니면 그대로)
/// - fontdue 는 ttf, ttc, otf 만 읽는다. 원본 기본 글꼴 DOSGothic 이 .woff 라서 푼다.
//==============================================================================
fn decompress_web_font(font_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
	let signature = font_bytes.get(0..4).unwrap_or(&[]);
	if signature == b"wOFF" {
		return wuff::decompress_woff1(&font_bytes).map_err(|error| format!("WOFF 풀기 실패: {:?}", error));
	}
	if signature == b"wOF2" {
		return wuff::decompress_woff2(&font_bytes).map_err(|error| format!("WOFF2 풀기 실패: {:?}", error));
	}
	Ok(font_bytes)
}

//==============================================================================
/// 줄 높이 값. (위로 올라간 길이, 아래로 내려간 길이. 둘 다 양수, 크롬 캔버스 TextMetrics 의 fontBoundingBoxAscent, fontBoundingBoxDescent 와 같은 셈)
//==============================================================================
fn read_line_metrics(font: &LoadedFont, pixel_size: f32) -> (f32, f32) {
	font.calculate_font_bounding_box(pixel_size)
}

//==============================================================================
/// 외곽선 띠 만들기. (원 모양으로 넓힌 값 - 원 모양으로 좁힌 값)
//==============================================================================
pub(crate) fn build_stroke_coverage(coverage: &[u8], bitmap_width: i32, bitmap_height: i32, stroke_radius: f32) -> Vec<u8> {
	let radius_ceiling = stroke_radius.ceil() as i32;
	let mut offset_list: Vec<(i32, i32)> = Vec::new();
	for offset_y in -radius_ceiling..=radius_ceiling {
		for offset_x in -radius_ceiling..=radius_ceiling {
			let distance = ((offset_x * offset_x + offset_y * offset_y) as f32).sqrt();
			if distance <= stroke_radius + 0.5 {
				offset_list.push((offset_x, offset_y));
			}
		}
	}

	let read_coverage = |position_x: i32, position_y: i32| -> u8 {
		if position_x < 0 || position_y < 0 || position_x >= bitmap_width || position_y >= bitmap_height {
			return 0;
		}
		coverage[(position_y * bitmap_width + position_x) as usize]
	};

	let mut stroke_coverage = vec![0u8; coverage.len()];
	for position_y in 0..bitmap_height {
		for position_x in 0..bitmap_width {
			let mut dilated_value: u8 = 0;
			let mut eroded_value: u8 = 255;
			for (offset_x, offset_y) in &offset_list {
				let value = read_coverage(position_x + offset_x, position_y + offset_y);
				dilated_value = dilated_value.max(value);
				eroded_value = eroded_value.min(value);
			}
			stroke_coverage[(position_y * bitmap_width + position_x) as usize] = dilated_value.saturating_sub(eroded_value);
		}
	}
	stroke_coverage
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// WOFF 글꼴이 풀려서 읽히는지. (VANILLA_TEST_WOFF_PATH 에 .woff 경로를 줄 때만 본다)
	//==============================================================================
	#[test]
	fn load_woff_font() {
		let font_path = match std::env::var("VANILLA_TEST_WOFF_PATH") {
			Ok(font_path) => {
				font_path
			}
			Err(_) => {
				return;
			}
		};
		let font_bytes = std::fs::read(&font_path).expect("woff file");
		let font = load_font(font_bytes).expect("woff font");
		let glyph_count = font.raster_font.glyph_count();
		let hangul_glyph_index = font.raster_font.lookup_glyph_index('가');
		assert!(glyph_count > 0);
		assert!(hangul_glyph_index != 0);
	}

	//==============================================================================
	/// 글 재기 캐시: 같은 폰트 문자열과 글은 한 번만 재 두고, 글꼴을 새로 등록하면 비운다.
	/// (엔진 기본 글꼴 후보 가운데 이 PC 에 있는 것으로 본다, 하나도 없으면 건너뛴다)
	//==============================================================================
	#[test]
	fn measure_cache_reuses_and_clears_on_font_registration() {
		let font_bytes = match crate::platform::find_fallback_font_file("가", 400, false, false).and_then(|source| std::fs::read(source.path).ok()) {
			Some(font_bytes) => {
				font_bytes
			}
			None => {
				return;
			}
		};
		let mut text_string_texture_cache = TextStringTextureCache::new();
		text_string_texture_cache.set_default_font(false, font_bytes.clone()).expect("default font");
		let first_metrics = text_string_texture_cache.measure_text("20px default", "가나다 abc");
		assert!(first_metrics.width > 0.0);
		assert_eq!(text_string_texture_cache.get_measure_count(), 1);
		let second_metrics = text_string_texture_cache.measure_text("20px default", "가나다 abc");
		assert_eq!(first_metrics.width, second_metrics.width);
		assert_eq!(text_string_texture_cache.get_measure_count(), 1);
		let bold_metrics = text_string_texture_cache.measure_text("40px default", "가나다 abc");
		assert!(bold_metrics.width > first_metrics.width);
		assert_eq!(text_string_texture_cache.get_measure_count(), 2);
		text_string_texture_cache.add_fallback_font(font_bytes).expect("fallback font");
		text_string_texture_cache.measure_text("20px default", "가나다 abc");
		assert_eq!(text_string_texture_cache.get_measure_count(), 1);
		for text_index in 0..(MAXIMUM_MEASURE_COUNT + 10) {
			text_string_texture_cache.measure_text("20px default", &text_index.to_string());
		}
		assert!(text_string_texture_cache.get_measure_count() <= MAXIMUM_MEASURE_COUNT);
	}

	//==============================================================================
	/// 구운 문자열 키: 같은 키는 같은 해시, 하나라도 다르면 맞지 않는다.
	//==============================================================================
	#[test]
	fn entry_key_matches_only_same_key() {
		let first_hash = create_entry_hash(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5);
		let second_hash = create_entry_hash(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5);
		assert_eq!(first_hash, second_hash);
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Stroke, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 2.0));
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Fill, false, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		let texture_entry = TextEntry {
			texture: glow::NativeTexture(std::num::NonZeroU32::new(1).expect("one")),
			advance_width: 0.0,
			font_ascent: 0.0,
			font_descent: 0.0,
			pen_offset_x: 0.0,
			baseline_offset_y: 0.0,
			quad_width: 0.0,
			quad_height: 0.0,
		};
		let cached_text_entry = CachedTextEntry {
			text_draw_kind: TextDrawKind::Fill,
			is_antialias: true,
			direction: ShapingDirection::LeftToRight,
			text: String::from("가나다"),
			font_string: String::from("16px default"),
			line_width: 0.0,
			bake_scale: 1.5,
			entry: texture_entry,
			last_used_frame_number: 0,
		};
		assert!(cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, false, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "bold 16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 2.0, 1.5));
	}

	//==============================================================================
	/// 안티앨리어싱을 끄면 문턱값 128 로 0 과 255 로 갈린다. (원본과 같은 문턱)
	//==============================================================================
	#[test]
	fn alpha_threshold_splits_at_128() {
		let mut values: Vec<u8> = vec![0, 1, 127, 128, 200, 255];
		apply_alpha_threshold(&mut values);
		assert_eq!(values, vec![0, 0, 0, 255, 255, 255]);
		let mut text_string_texture_cache = TextStringTextureCache::new();
		assert!(text_string_texture_cache.is_text_antialias_on());
		text_string_texture_cache.set_text_antialias_enabled(false);
		assert!(!text_string_texture_cache.is_text_antialias_on());
	}

	//==============================================================================
	/// 폰트 문자열: italic, 굵기, 따옴표 이름, 이름 목록의 첫 이름을 읽는다.
	//==============================================================================
	#[test]
	fn parse_font_string_reads_style_weight_and_family() {
		let font_description = parse_font_string("italic bold 20px \"My Font\"");
		assert!(font_description.is_italic);
		assert!(font_description.is_bold);
		assert_eq!(font_description.font_weight, 700);
		assert_eq!(font_description.family_name, "My Font");
		let weighted_description = parse_font_string("500 16px \"GameUI\", sans-serif");
		assert_eq!(weighted_description.font_weight, 500);
		assert!(!weighted_description.is_bold);
		assert_eq!(weighted_description.family_name, "GameUI");
		assert!(!parse_font_string("20px default").is_italic);
	}

	//==============================================================================
	/// 폰트 문자열: CSS 단위와 이름 크기, 줄높이, 이름 목록, 틀린 문자열. (크롬 Canvas2D 의 font 가 돌려준 값과 대었다)
	//==============================================================================
	#[test]
	fn parse_font_string_reads_css_units_and_family_list() {
		let read_size = |font_string: &str| -> f32 {
			parse_font_string(font_string).font_size
		};
		assert_eq!(read_size("12pt a"), 16.0);
		assert_eq!(read_size("2em a"), 20.0);
		assert_eq!(read_size("2rem a"), 32.0);
		assert_eq!(read_size("150% a"), 15.0);
		assert_eq!(read_size("medium a"), 16.0);
		assert_eq!(read_size("x-large a"), 24.0);
		assert_eq!(read_size("xx-small a"), 9.0);
		assert_eq!(read_size("larger a"), 12.0);
		assert!((read_size("smaller a") - 8.33333).abs() < 0.001);
		assert_eq!(read_size("1in a"), 96.0);
		assert!((read_size("1cm a") - 37.7953).abs() < 0.001);
		assert!((read_size("10q a") - 9.44882).abs() < 0.001);
		assert_eq!(read_size("1pc a"), 16.0);
		assert_eq!(read_size("16.5px a"), 16.5);
		assert_eq!(read_size("16PX A"), 16.0);
		let line_height_description = parse_font_string("bold 16px/20px a, b");
		assert_eq!(line_height_description.font_size, 16.0);
		assert_eq!(line_height_description.font_weight, 700);
		assert_eq!(line_height_description.family_name_list, vec!["a", "b"]);
		assert_eq!(parse_font_string("16px / 1.5 a").family_name, "a");
		let list_description = parse_font_string("500 16px 'A B', \"C\", sans-serif");
		assert_eq!(list_description.family_name_list, vec!["A B", "C", "sans-serif"]);
		assert_eq!(parse_font_string("italic 16px a  b c").family_name, "a b c");
		let keyword_description = parse_font_string("italic small-caps bold condensed 16px a");
		assert!(keyword_description.is_italic && keyword_description.font_weight == 700);
		assert!(parse_font_string("oblique 10deg 16px a").is_italic);
		assert_eq!(parse_font_string("lighter 16px a").font_weight, 100);
		for invalid_font_string in ["16px", "-5px a", "bold a 16px", "x 16px a", "16px 'a", "2.px a", "2x a", "calc(1px -2px) a", "calc(1px+2px) a", "round(1.5px) a", "sqrt(4) a", "sign(1px) a", "progress(1px from 0px to 2px) a"] {
			let invalid_description = parse_font_string(invalid_font_string);
			assert_eq!(invalid_description.font_size, 10.0, "{}", invalid_font_string);
			assert_eq!(invalid_description.family_name, "sans-serif", "{}", invalid_font_string);
		}
	}

	//==============================================================================
	/// 시스템 글꼴 사슬과 운영체제 이름 찾기, 대체 글꼴: 맥 크롬 Canvas2D 의 measureText 값과 같다. (맥만, 같은 글꼴이 있을 때)
	/// - 한글은 사슬의 Apple SD Gothic Neo, "16px Helvetica" 와 "16px Menlo" 는 올리지 않은 이름을 운영체제 글꼴에서 찾은 것, 태국어는 운영체제 대체 글꼴.
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn system_font_chain_matches_chrome_widths_on_macos() {
		let mut text_string_texture_cache = TextStringTextureCache::new();
		let system_font_description = parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
		text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
		text_string_texture_cache.set_system_font_fallback_enabled(true);
		if crate::platform::find_family_font_file("Apple SD Gothic Neo", 400, false).is_none() || crate::platform::find_family_font_file("Menlo", 400, false).is_none() {
			return;
		}
		let measure_width = |font_string: &str, text: &str| -> f32 {
			text_string_texture_cache.measure_text(font_string, text).width
		};
		assert!((measure_width("16px default", "가나다라") - 55.360).abs() < 0.01);
		assert!((measure_width("bold 16px default", "가나다라") - 55.360).abs() < 0.01);
		assert!((measure_width("16px Helvetica", "Hello World Wifi") - 113.508).abs() < 0.01);
		assert!((measure_width("16px Menlo", "Hello World Wifi") - 154.125).abs() < 0.01);
		assert!(measure_width("16px default", "ภาษาไทย") > 50.0);
		assert!(measure_width("bold 16px default", "Hello World Wifi") > measure_width("16px default", "Hello World Wifi"));
	}

	//==============================================================================
	/// 맥 시스템 UI 글꼴(SFNS)의 광학 크기와 자간 표, 크롬이 모르는 이름: 맥 크롬 158 Canvas2D 의 measureText 값과 같다. (맥만, SFNS 가 있을 때)
	/// - system-ui 와 BlinkMacSystemFont 는 opsz 축을 글자 크기로 걸고(17 ~ 96 으로 자른다) trak 표의 자간을 더한다. 8 ~ 128px, 굵기 300, 400, 700 과 기울임(SFNSItalic).
	/// - 글자 자리는 크롬 HarfBuzz 처럼 16.16 배율로 셈해 가변 GPOS 델타가 크롬과 같다. (축 값 정규화, 짝 커닝의 장치 표도 하프버즈와 같다) 크롬 값과 1/10000 px 안.
	/// - -apple-system 은 크롬이 모르는 이름이라 다음 이름으로 넘어간다. (사슬에서는 Apple SD Gothic Neo, 하나뿐이면 화면 언어의 표준 글꼴)
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn system_ui_font_matches_chrome_on_macos() {
		if !std::path::Path::new("/System/Library/Fonts/SFNS.ttf").exists() {
			return;
		}
		let mut text_string_texture_cache = TextStringTextureCache::new();
		let system_font_description = parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
		text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
		text_string_texture_cache.set_system_font_fallback_enabled(true);
		let size_list = [8.0, 10.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 20.0, 21.0, 24.0, 28.0, 32.0, 40.0, 48.0, 64.0, 96.0, 128.0];
		let text_list = ["Hello World Wifi", "AVATAR Type Wolf"];
		let chrome_width_table: [(&str, usize, [f32; 19]); 8] = [
			("", 0, [63.72534, 77.41699, 90.65039, 96.98584, 103.35254, 109.56299, 115.86719, 121.51514, 128.19327, 139.69373, 145.7919, 164.97443, 192.80078, 219.84375, 272.61719, 325.64062, 430.1875, 640.03125, 853.375]),
			("bold ", 0, [67.71362, 82.40234, 96.63281, 103.4668, 110.33203, 117.04102, 123.84375, 129.99023, 137.23242, 149.91211, 156.7002, 178.03125, 208.31836, 237.57812, 294.78516, 352.24219, 465.65625, 693.23438, 924.3125]),
			("300 ", 0, [62.7253, 76.16693, 89.15031, 95.36076, 101.60246, 107.6879, 113.8671, 119.39003, 125.92758, 137.00986, 142.81726, 161.04968, 187.93359, 214.28125, 265.66406, 317.29688, 419.0625, 623.34375, 831.125]),
			("italic ", 0, [64.14722, 77.94434, 91.2832, 97.67139, 104.09082, 110.354, 116.71094, 122.41162, 129.26233, 141.24557, 147.69449, 168.30789, 197.12109, 224.78125, 278.78906, 333.04688, 440.0625, 654.84375, 873.125]),
			("", 1, [72.66284, 88.58887, 104.05664, 111.50928, 118.99316, 126.3208, 133.74219, 140.50732, 148.23708, 161.60435, 168.54726, 190.06213, 221.60742, 252.76562, 313.76953, 375.02344, 496.03125, 738.79688, 985.0625]),
			("bold ", 1, [77.35034, 94.44824, 111.08789, 119.12646, 127.19629, 135.10986, 143.11719, 150.46826, 158.83867, 173.59828, 181.2612, 205.15698, 239.49023, 273.20312, 339.31641, 405.67969, 536.90625, 800.10938, 1066.8125]),
			("300 ", 1, [71.70775, 87.39499, 102.62398, 109.95724, 117.32175, 124.52998, 131.83199, 138.47772, 146.0793, 159.06029, 165.68126, 186.27104, 216.92474, 247.41397, 307.07996, 366.99597, 485.32794, 722.74194, 963.65588]),
			("italic ", 1, [73.67847, 89.8584, 105.58008, 113.15967, 120.77051, 128.2251, 135.77344, 142.66553, 150.56776, 164.32457, 171.44641, 193.79195, 226.07812, 257.875, 320.15625, 382.6875, 506.25, 754.125, 1005.5]),
		];
		for (style_prefix, text_index, chrome_width_list) in chrome_width_table {
			for (size_index, font_size) in size_list.iter().enumerate() {
				let font_string = format!("{}{}px system-ui", style_prefix, font_size);
				let width = text_string_texture_cache.measure_text(&font_string, text_list[text_index]).width;
				assert!((width - chrome_width_list[size_index]).abs() < 0.0001, "{} {} {} {}", font_string, text_list[text_index], width, chrome_width_list[size_index]);
			}
		}
		let measure_width = |font_string: &str| -> f32 {
			text_string_texture_cache.measure_text(font_string, "Hello World Wifi").width
		};
		assert!((measure_width("16px BlinkMacSystemFont") - 115.867).abs() < 0.01);
		// -apple-system 하나뿐이면 맞는 이름이 없어 화면 언어의 표준 글꼴이다. (한국어 Apple SD Gothic Neo, 영어 Times)
		if let Some(standard_family_name) = crate::platform::desktop::systemfont::resolve_generic_family_name(crate::platform::desktop::systemfont::GenericFamily::Standard) {
			assert!((measure_width("16px -apple-system") - measure_width(&format!("16px '{}'", standard_family_name))).abs() < 0.001);
		}
		if crate::platform::find_family_font_file("Apple SD Gothic Neo", 400, false).is_some() {
			assert!((measure_width("16px default") - 109.072).abs() < 0.01);
		}
	}

	//==============================================================================
	/// 폰트 문자열 크기의 단위와 calc: 맥 크롬 158 의 문서에 붙이지 않은 캔버스가 font 를 다시 읽은 값과 같다. (창 1000 x 700, 맥만)
	/// - 부모 글꼴은 Times 10px, 뿌리 글꼴은 화면 언어의 표준 글꼴 16px(한국어 Apple SD Gothic Neo, 뿌리 값은 한국어일 때만 본다).
	/// - 이름 목록이 monospace 하나뿐이면 상대 크기에 13 / 16, 이름 크기는 고정폭 표.
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn font_size_units_match_chrome_on_macos() {
		if crate::platform::find_family_font_file("Times", 400, false).is_none() {
			return;
		}
		set_font_viewport_size(1000.0, 700.0);
		let chrome_size_list: [(&str, f32); 75] = [
			("2ex serif", 8.97461), ("2ch serif", 10.0), ("2cap serif", 13.2324), ("2ic serif", 20.0), ("2lh serif", 26.0), ("0.5ex serif", 2.24365), ("2EX serif", 8.97461), ("bold 2ex serif", 8.97461),
			("2em monospace", 16.25), ("2em MONOSPACE", 16.25), ("2em monospace, serif", 20.0), ("2em \"monospace\"", 20.0), ("20px monospace", 20.0), ("medium monospace", 13.0), ("large monospace", 16.0), ("small monospace", 12.0),
			("x-small monospace", 10.0), ("x-large monospace", 20.0), ("xx-large monospace", 26.0), ("xxx-large monospace", 39.0), ("larger monospace", 9.75), ("smaller monospace", 6.77083), ("150% monospace", 12.1875), ("2ex monospace", 7.29187),
			("2rem monospace", 32.0), ("2vw monospace", 20.0), ("2lh monospace", 21.125), ("2ic monospace", 16.25), ("2ch monospace", 8.125),
			("2vw serif", 20.0), ("2vh serif", 14.0), ("2vmin serif", 14.0), ("2vmax serif", 20.0), ("2vi serif", 20.0), ("2vb serif", 14.0), ("2svw serif", 20.0), ("2lvh serif", 14.0), ("2dvmax serif", 20.0),
			("2cqw serif", 20.0), ("2cqi serif", 20.0), ("2cqb serif", 14.0), ("2cqmin serif", 14.0), ("2cqmax serif", 20.0),
			("calc(2px + 1em) serif", 12.0), ("min(2em, 30px) serif", 20.0), ("max(1em, 3px) serif", 10.0), ("clamp(1px, 2em, 50px) serif", 20.0), ("calc((1em + 2px) / 2) serif", 6.0), ("calc(2em) monospace", 20.0), ("calc(1em + 1px) monospace", 11.0),
			("calc(-1px) serif", 0.0), ("calc( 1px + 2px ) serif", 3.0), ("abs(-2px) serif", 2.0), ("calc(3px / 2) serif", 1.5), ("calc(infinity * 1px) serif", 10000.0), ("CALC(1px + 2px) serif", 3.0), ("calc(1ex + 1ch) serif", 9.4873),
			("calc(100%) monospace", 8.125), ("calc(50% + 1px) monospace", 6.0), ("calc(2 * 50%) monospace", 8.125), ("max(100%, 1px) monospace", 10.0), ("calc(1em + 10%) monospace", 11.0), ("calc(pi * 1px) serif", 3.14159),
			("clamp(5px, 1px, 3px) serif", 5.0), ("clamp(none, 1px, 3px) serif", 1.0), ("mod(5px, 2px) serif", 1.0), ("round(1.5px, 1px) serif", 2.0), ("hypot(3px, 4px) serif", 5.0), ("calc(sign(-1px) * 1px) serif", 0.0), ("calc(0px / 0) serif", 0.0),
			("20000px serif", 10000.0), ("+2px serif", 2.0), (".5px serif", 0.5), ("1Q serif", 0.944882), ("0px serif", 0.0),
		];
		for (font_string, chrome_size) in chrome_size_list {
			let font_description = parse_font_string(font_string);
			assert!((font_description.font_size - chrome_size).abs() < 0.0005, "{} {} {}", font_string, font_description.font_size, chrome_size);
			assert!(font_description.family_name != "sans-serif" || font_string.contains("sans-serif"), "{}", font_string);
		}
		if crate::platform::desktop::systemfont::read_browser_locale() == "ko" && crate::platform::find_family_font_file("Apple SD Gothic Neo", 400, false).is_some() {
			let chrome_root_size_list: [(&str, f32); 12] = [("2rem serif", 32.0), ("2rex serif", 17.216), ("2rch serif", 20.16), ("2rcap serif", 22.4), ("2ric serif", 27.68), ("2rlh serif", 38.0), ("2rex monospace", 13.988), ("1rlh monospace", 15.4375), ("calc(1rem) serif", 16.0), ("calc(1rex) serif", 8.608), ("calc(1rlh) serif", 19.0), ("calc(1rem + 1%) serif", 16.1)];
			for (font_string, chrome_size) in chrome_root_size_list {
				let font_size = parse_font_string(font_string).font_size;
				assert!((font_size - chrome_size).abs() < 0.0005, "{} {} {}", font_string, font_size, chrome_size);
			}
		}
	}

	//==============================================================================
	/// 일반 이름의 기본 글꼴: 맥 크롬 158 (화면 언어 한국어)의 measureText 값과 같다. (맥만, 화면 언어가 한국어일 때)
	/// - 크롬 맥 한국어 표: 표준 Apple SD Gothic Neo, serif AppleMyungjo, sans-serif Apple SD Gothic Neo, monospace Menlo, cursive Apple Chancery, fantasy Papyrus, math STIX Two Math.
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn generic_families_match_chrome_korean_defaults_on_macos() {
		if crate::platform::desktop::systemfont::read_browser_locale() != "ko" || crate::platform::find_family_font_file("AppleMyungjo", 400, false).is_none() {
			return;
		}
		let mut text_string_texture_cache = TextStringTextureCache::new();
		let system_font_description = parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
		text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
		text_string_texture_cache.set_system_font_fallback_enabled(true);
		let measure_width = |font_string: &str, text: &str| -> f32 {
			text_string_texture_cache.measure_text(font_string, text).width
		};
		let chrome_width_list: [(&str, f32, f32); 9] = [("serif", 124.097, 63.938), ("sans-serif", 109.072, 55.36), ("monospace", 154.125, 55.36), ("cursive", 111.352, 55.36), ("fantasy", 109.156, 55.36), ("math", 115.472, 55.36), ("Times", 110.961, 55.36), ("Courier", 153.625, 55.36), ("NoSuchFamily", 109.072, 55.36)];
		for (family_name, latin_width, korean_width) in chrome_width_list {
			let font_string = format!("16px {}", family_name);
			assert!((measure_width(&font_string, "Hello World Wifi") - latin_width).abs() < 0.01, "{} {}", font_string, measure_width(&font_string, "Hello World Wifi"));
			assert!((measure_width(&font_string, "가나다라") - korean_width).abs() < 0.01, "{} {}", font_string, measure_width(&font_string, "가나다라"));
		}
	}

	//==============================================================================
	/// 따옴표 친 일반 이름: 따옴표를 남겨 글꼴 이름으로 읽는다. (따옴표 없는 것은 일반 이름, 일반 이름이 아닌 이름은 따옴표를 뗀다)
	//==============================================================================
	#[test]
	fn quoted_generic_family_keeps_quotes() {
		let font_description = parse_font_string("16px 'monospace', \"Serif\", monospace, 'Menlo', -webkit-body");
		assert_eq!(font_description.family_name_list, vec!["\"monospace\"", "\"Serif\"", "monospace", "Menlo", "-webkit-body"]);
		assert_eq!(read_registered_family_key("\"monospace\""), Some(String::from("monospace")));
		assert_eq!(read_registered_family_key("MONOSPACE"), None);
		assert_eq!(read_registered_family_key("Menlo"), Some(String::from("menlo")));
	}

	//==============================================================================
	/// 따옴표 친 일반 이름과 올린 얼굴, -webkit-body: 맥 크롬 158 (화면 언어 한국어)의 measureText 값과 같다. (맥만, 화면 언어가 한국어이고 Georgia 가 있을 때)
	/// - 따옴표 친 'monospace', 'serif' 는 그 이름의 글꼴이 없어 표준 글꼴(Apple SD Gothic Neo)이고, 'system-ui' 는 맥 크롬 FontCache 처럼 시스템 UI 글꼴이다.
	/// - 마지막 일반 이름이 -webkit-body 면 앞 이름(Menlo, serif)도 표준 글꼴이다. (FontSelector::FamilyNameFromSettings)
	/// - Georgia 를 monospace, system-ui, Menlo, -webkit-body 이름으로 올리면 따옴표 친 이름과 보통 이름만 그것을 쓰고, 따옴표 없는 일반 이름은 쓰지 않는다. (CSSFontSelector::GetFontData)
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn quoted_generic_family_matches_chrome_on_macos() {
		let georgia_path = std::path::Path::new("/System/Library/Fonts/Supplemental/Georgia.ttf");
		if crate::platform::desktop::systemfont::read_browser_locale() != "ko" || !georgia_path.exists() || crate::platform::find_family_font_file("Apple SD Gothic Neo", 400, false).is_none() {
			return;
		}
		let mut text_string_texture_cache = TextStringTextureCache::new();
		let system_font_description = parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
		text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
		text_string_texture_cache.set_system_font_fallback_enabled(true);
		let before_width_list: [(&str, f32); 14] = [("16px monospace", 154.125), ("16px 'monospace'", 109.07187), ("16px \"MONOSPACE\"", 109.07187), ("16px 'monospace', monospace", 154.125), ("16px 'monospace', serif", 124.09744), ("16px 'serif'", 109.07187), ("16px 'cursive'", 109.07187), ("16px 'math'", 109.07187), ("16px 'system-ui'", 115.86719), ("16px -webkit-body", 109.07187), ("16px '-webkit-body'", 109.07187), ("16px Menlo, -webkit-body", 109.07187), ("16px serif, -webkit-body", 109.07187), ("16px -webkit-body, serif", 109.07187)];
		for (font_string, chrome_width) in before_width_list {
			let width = text_string_texture_cache.measure_text(font_string, "Hello World Wifi").width;
			assert!((width - chrome_width).abs() < 0.001, "{} {} {}", font_string, width, chrome_width);
		}
		let georgia_bytes = std::fs::read(georgia_path).expect("Georgia");
		for family_name in ["monospace", "system-ui", "Menlo", "-webkit-body"] {
			text_string_texture_cache.register_font_face(family_name, FontFaceDescriptors::new(), georgia_bytes.clone()).expect("register");
		}
		let after_width_list: [(&str, f32); 10] = [("16px monospace", 154.125), ("16px 'monospace'", 121.01563), ("16px 'monospace', monospace", 121.01563), ("16px system-ui", 115.86719), ("16px 'system-ui'", 121.01563), ("16px BlinkMacSystemFont", 121.01563), ("16px -webkit-body", 109.07187), ("16px '-webkit-body'", 121.01563), ("16px Menlo, -webkit-body", 121.01563), ("16px serif, -webkit-body", 109.07187)];
		for (font_string, chrome_width) in after_width_list {
			let width = text_string_texture_cache.measure_text(font_string, "Hello World Wifi").width;
			assert!((width - chrome_width).abs() < 0.001, "{} {} {}", font_string, width, chrome_width);
		}
	}

	//==============================================================================
	/// 맥 색 이모지와 hvgl 글꼴: CoreText 로 그려 맥 크롬과 같다. (맥만, Apple Color Emoji 가 있을 때)
	/// - 이모지 전진폭은 크롬 measureText 값, 16px 이모지의 그림 상자와 알파 합은 크롬 캔버스 getImageData 로 잰 값이다.
	/// - 중국어 간체(PingFangUI, 윤곽이 애플 전용 hvgl 표에 있다)도 그려진다.
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn color_emoji_matches_chrome_on_macos() {
		if crate::platform::find_family_font_file("Apple Color Emoji", 400, false).is_none() {
			return;
		}
		let mut text_string_texture_cache = TextStringTextureCache::new();
		let system_font_description = parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
		text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
		text_string_texture_cache.set_system_font_fallback_enabled(true);
		for (font_size, chrome_advance) in [(8.0, 10.0), (10.0, 13.0), (13.0, 16.0), (16.0, 20.0), (17.0, 21.0), (21.0, 23.0), (24.0, 24.0), (32.0, 32.0), (64.0, 64.0)] {
			let text_metrics = text_string_texture_cache.measure_text(&format!("{}px default", font_size), "\u{1F600}");
			assert_eq!(text_metrics.width, chrome_advance, "{}px", font_size);
		}
		let text_raster = text_string_texture_cache.rasterize_text("\u{1F600}", "16px default", 1.0, 2, ShapingDirection::LeftToRight).expect("emoji raster");
		let color_pixels = text_raster.color_pixels.expect("color pixels");
		let mut bounds = [i32::MAX, i32::MAX, -1, -1];
		let mut alpha_sum: u64 = 0;
		for position_y in 0..text_raster.height {
			for position_x in 0..text_raster.width {
				let alpha = color_pixels[((position_y * text_raster.width + position_x) * 4 + 3) as usize];
				if alpha > 0 {
					bounds = [bounds[0].min(position_x), bounds[1].min(position_y), bounds[2].max(position_x), bounds[3].max(position_y)];
					alpha_sum += alpha as u64;
				}
			}
		}
		assert_eq!(bounds[0] as f32 - text_raster.origin_x, 0.0);
		assert_eq!(bounds[2] as f32 + 1.0 - text_raster.origin_x, 20.0);
		assert_eq!(text_raster.baseline_y - bounds[1] as f32, 16.0);
		assert_eq!(bounds[3] as f32 + 1.0 - text_raster.baseline_y, 4.0);
		assert_eq!(alpha_sum, 66360);
		let chinese_raster = text_string_texture_cache.rasterize_text("\u{4EEC}", "32px default", 1.0, 2, ShapingDirection::LeftToRight).expect("chinese raster");
		assert!(chinese_raster.coverage.iter().any(|value| *value > 0));
	}

	//==============================================================================
	/// 지켜보는 이름: 올린 얼굴의 글자 범위에 들지 않은 글자만 모으고, 꺼내면 비운다. (지켜보지 않는 이름은 모으지 않는다)
	//==============================================================================
	#[test]
	fn unresolved_family_characters_are_collected() {
		let font_bytes = match crate::platform::find_fallback_font_file("가", 400, false, false).and_then(|source| std::fs::read(source.path).ok()) {
			Some(font_bytes) => {
				font_bytes
			}
			None => {
				return;
			}
		};
		let mut text_string_texture_cache = TextStringTextureCache::new();
		text_string_texture_cache.register_font_face("WatchedFamily", FontFaceDescriptors::create_from_css("400", "normal", "U+0000-00FF"), font_bytes.clone()).expect("latin face");
		text_string_texture_cache.register_font_face("OtherFamily", FontFaceDescriptors::new(), font_bytes).expect("other face");
		watch_unresolved_family_characters("WatchedFamily");
		text_string_texture_cache.measure_text("16px WatchedFamily", "Ab 가나");
		text_string_texture_cache.measure_text("16px OtherFamily", "다");
		assert_eq!(take_unresolved_family_characters("watchedfamily"), "가나");
		assert_eq!(take_unresolved_family_characters("WatchedFamily"), "");
		assert_eq!(take_unresolved_family_characters("OtherFamily"), "");
	}

	//==============================================================================
	/// 글꼴 이름 열쇠: 따옴표로 감싸 올린 이름도 올리기, 세기, 차례, 지우기가 같은 이름으로 찾는다.
	/// (얼굴을 올리는 부분은 엔진 기본 글꼴 후보 가운데 이 PC 에 있는 것으로 본다, 하나도 없으면 열쇠만 본다)
	//==============================================================================
	#[test]
	fn quoted_family_name_matches_everywhere() {
		assert_eq!(create_family_key(" \"My Font\" "), "my font");
		assert_eq!(create_family_key("'My Font'"), "my font");
		assert_eq!(create_family_key("My Font"), "my font");
		let font_bytes = match crate::platform::find_fallback_font_file("가", 400, false, false).and_then(|source| std::fs::read(source.path).ok()) {
			Some(font_bytes) => {
				font_bytes
			}
			None => {
				return;
			}
		};
		let mut text_string_texture_cache = TextStringTextureCache::new();
		text_string_texture_cache.register_font_face("\"Quoted Family\"", FontFaceDescriptors::new(), font_bytes).expect("quoted face");
		assert_eq!(text_string_texture_cache.get_font_face_count("Quoted Family"), 1);
		assert_eq!(text_string_texture_cache.get_font_face_count("'Quoted Family'"), 1);
		text_string_texture_cache.set_font_face_order("'Quoted Family'", &[String::new()]);
		assert_eq!(text_string_texture_cache.font_face_order_table.get("quoted family").map(|source_key_list| source_key_list.len()), Some(1));
		assert!(text_string_texture_cache.unregister_font_face("\"Quoted Family\"", ""));
		assert_eq!(text_string_texture_cache.get_font_face_count("Quoted Family"), 0);
	}

	//==============================================================================
	/// 웹 글꼴이 아닌 바이트는 그대로 둔다.
	//==============================================================================
	#[test]
	fn keep_plain_font_bytes() {
		let plain_bytes: Vec<u8> = vec![0, 1, 0, 0, 9, 9];
		let result_bytes = decompress_web_font(plain_bytes.clone()).expect("plain bytes");
		assert_eq!(result_bytes, plain_bytes);
	}
}
