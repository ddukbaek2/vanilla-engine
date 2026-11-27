//==============================================================================
//! 글자 모양 짓기. (글꼴 얼굴 고르기, 양방향 정렬, 글자 모양 잇기, 글리프 래스터)
//! - 원본은 브라우저 Canvas2D 가 이 일을 다 한다. (CSS 글꼴 고르기, 양방향 알고리즘, HarfBuzz 모양 짓기, Skia 래스터)
//!   러스트 판은 같은 순서를 크레이트로 밟는다:
//!   - 글꼴 얼굴(FontFaceDescriptors): CSS @font-face 처럼 이름 하나 아래에 굵기, 기울기, 글자 범위(unicode-range)가 다른 얼굴을 여럿 둔다.
//!     고르기는 CSS Fonts 의 차례(기울기 → 굵기)를 따르고, 같은 굵기와 기울기의 얼굴들은 글자 범위로 나눠 그린다. 나중에 올린 얼굴이 먼저다.
//!   - 양방향 정렬: unicode-bidi (UAX #9). 문단 방향은 굽는 쪽이 준다. (원본 canvas.direction)
//!   - 모양 짓기: rustybuzz (HarfBuzz 를 옮긴 것). 아랍어 글자 잇기, 합자, 커닝, 결합 부호 자리를 브라우저처럼 잡는다.
//!     크롬 HarfBuzzFace::GetScaledFont 처럼 자리 배율을 글자 크기의 16.16 고정 소수로 건다. rustybuzz 0.20.1 에는 배율이 없어 저장소의 third_party/rustybuzz 에
//!     hb_font_set_scale(Face::set_scale)을 더한 판을 둔다. 글꼴 단위 값은 em_scale, 가변 GPOS 델타는 em_scalef 로 그 배율에서 반올림하고,
//!     축 값은 HarfBuzz hb_ot_var_normalize_coords 처럼 정규화한다. 같이 둔 third_party/ttf-parser 는 짝 커닝(PairPos 꼴 1)의 장치 표 기준을 바로잡은 판이다.
//!   - 래스터: fontdue 의 글리프 번호 래스터. 굵은 얼굴이 없으면 가짜 굵게, 기울인 얼굴이 없으면 가짜 기울임을 글리프마다 입힌다.
//!     색 글리프(sbix, CBDT, COLR)는 colorglyph.rs 가 제 색으로 그린다. 크롬처럼 가짜 기울임은 입히고 가짜 굵게는 입히지 않는다.
//! - 이모지 표시: 이름 목록의 얼굴은 색과 상관없이 글리프가 있으면 먼저 쓰고, 운영체제 대체 글꼴 단계에서 VS16(FE0F), Emoji_Presentation 글자, 피부색 조정자, 국기, 키캡, 태그 묶음은
//!   색 글리프를 가진 얼굴을, 그 밖(VS15 포함)은 색이 아닌 얼굴을 먼저 고른다. 이형 선택자가 붙은 묶음은 이름 목록에서도 그 표시에 맞는 얼굴만 받는다.
//!   크롬은 지정한 글꼴에 흑백 ❤ 가 있어도 ❤️ (FE0F) 는 색 이모지 글꼴로, 맨 ❤ 는 지정한 글꼴로 그린다. (맥 크롬에서 Canvas2D 로 확인)
//!   리눅스 크롬 157 은 "16px DejaVu Sans" 의 맨 😀 를 DejaVu Sans 의 흑백 글리프로 그린다. (CI 의 measureText)
//! - 가짜 기울임과 가짜 굵게는 Chromium 이 Skia 에 주는 값과 같다:
//!   기울기 1/4 (font_platform_data.cc 의 setSkewX(-SK_Scalar1 / 4)), 굵게는 글자 크기의 1/24 (9px 이하) ~ 1/32 (36px 이상) 두께로 윤곽을 칠해 넓힌다.
//!   (Skia SkTextFormatParams.h 의 kStdFakeBoldInterpKeys / Values) 둘 다 전진폭은 그대로다.
//!   가짜 굵게를 쓰는 때는 청한 굵기가 600 이상이고 고른 얼굴이 600 아래일 때다. (Blink kBoldThreshold)
//! - 글꼴 바이트는 LoadedFont 가 rustybuzz 얼굴과 함께 든다. (self_cell, 얼굴이 바이트를 빌린다) 등록을 지우고 쓰는 곳이 없어지면 바이트도 돌려준다.
//!   같은 파일의 여러 얼굴(TTC 의 굵기들, 가변 글꼴의 굵기 인스턴스)은 바이트(Rc)를 나눠 든다.
//! - TTC 같은 글꼴 모음은 얼굴 번호(face_index)를 골라 읽는다. (find_collection_face_index 가 굵기와 기울기로 고른다)
//! - 가변 글꼴(fvar 의 wght 축)은 굵기 인스턴스를 축 값으로 만든다. (create_from_shared_bytes 의 variation_list) 모양 짓기는 rustybuzz 가 축 값을 읽고,
//!   래스터는 fontdue 가 축 값을 읽지 못하고 맥 SFNS 같은 가변 글꼴의 윤곽을 그리지 못해, 가변 글꼴은 skrifa 로 축 값을 건 윤곽을 받아 colorglyph 의 넓이 누적 래스터로 그린다.
//!   (크롬도 가변 글꼴의 굵기를 축 값으로 그린다)
//! - 광학 크기(opsz 축)와 자간 표(trak): 크롬은 글꼴의 opsz 축을 글자 크기로 건다. (font-optical-sizing 의 처음 값 auto, 맥 font_platform_data_mac.mm 의
//!   FontPlatformDataFromCTFont, 모든 운영체제의 웹 글꼴 font_custom_platform_data.cc. 윈도우, 리눅스 시스템 글꼴에는 걸지 않는다)
//!   모양 짓기는 HarfBuzz 에 ptem 으로 글자 크기를 주어 AAT trak 표의 자간을 더한다. (harfbuzz_face.cc 의 GetScaledFont, 맥은 trak 이 있고 sbix 가 없는
//!   글꼴의 전진폭을 CoreText 대신 HarfBuzz 로 셈한다) 러스트 판도 굽는 크기(원본은 폰트 문자열의 px 를 굽는 배율로 키운다)로 opsz 를 걸고 trak 을 더한다.
//!   trak 은 rustybuzz 가 아니라 여기서 HarfBuzz 15 (hb-aat-layout-trak-table.hh) 처럼 더한다. 크기 사이는 반올림하지 않고 곧게 잇고, 표의 처음과 끝 밖은
//!   끝 값이며, 글자 묶음의 첫 글리프 전진폭에만 더한다. STAT 표가 없는 글꼴에는 걸지 않는다.
//!   (rustybuzz 0.20 은 글꼴 단위 정수로 반올림하고 표 밖을 늘여 잇고 x_offset 에도 반을 더하고 STAT 을 보지 않는다)
//!   맥 시스템 UI 글꼴(SFNS, opsz 17 ~ 96, 처음 28)이 16px 에서 opsz 17 과 trak -40 단위로 크롬과 같아진다. (색 이모지는 CoreText 의 전진폭 그대로 둔다)
//! - 낱말마다 모양 짓기: 크롬 캔버스(platform/fonts/plain_text_node.cc 의 SegmentWord)는 글을 낱말로 나눠 따로 모양 짓는다. 빈칸을 낀 커닝과 합자가 없다.
//!   빈칸(U+0020), 탭, ZWSP 는 하나씩, 한중일 글자와 이모지는 글자마다 따로다. (find_chrome_word_end) 캔버스의 탭, 줄바꿈, 폼 피드, 캐리지 리턴은 빈칸으로 바꾼다.
//! - 맥: 글리프를 원본 크롬 맥처럼 CoreText 로 그린다. (platform/desktop/coretextglyph.rs) 색이 아닌 글리프는 크롬 Canvas2D 와 같은 설정의 A8 마스크(글꼴 부드럽게,
//!   CoreGraphics 감마 풀기, 글자 색 밝기의 감마 전보정, 1/4 픽셀 서브픽셀 자리)이고, 파일에서 읽은 색 이모지 글꼴(sbix)은 CoreText 색 그림이다. (rasterize_glyph_at)
//!   색 이모지의 전진폭은 CoreText 가 작은 크기에서 키워 그리는 비를 곱하고 정수 픽셀로 반올림한다. (16px 에서 20, 13px 에서 16, 크롬 measureText 와 같다)
//! - 윈도우: 색이 아닌 글리프를 크롬 윈도우처럼 DirectWrite 로 그린다. (platform/desktop/directwriteglyph.rs, 스키아 SkScalerContext_DW 의 그레이스케일 A8 과 감마 전보정, 1/4 픽셀 서브픽셀 자리)
//! - 리눅스: 크롬 158 리눅스는 글꼴을 FreeType 이 아니라 스키아 Fontations(skrifa)로 그린다. 러스트 판도 skrifa 윤곽에 fontconfig 의 힌팅 단계로 힌팅을 걸고
//!   감마 1.2, 대비 0.2 로 전보정하며 1/4 픽셀 서브픽셀 자리를 쓴다. (rasterize_hinted_outline_mask)
//!
//! # 예제
//! 이모지 표시인지 글 표시인지 가리기
//! ```rust
//! use vanilla::core::textshaping::is_emoji_presentation;
//!
//! assert!(is_emoji_presentation("\u{1F600}"));
//! assert!(is_emoji_presentation("\u{2764}\u{FE0F}"));
//! assert!(!is_emoji_presentation("\u{2764}"));
//! assert!(!is_emoji_presentation("\u{1F600}\u{FE0E}"));
//! assert!(is_emoji_presentation("\u{1F1F0}\u{1F1F7}"));
//! assert!(!is_emoji_presentation("A"));
//! ```
//==============================================================================
// 옮긴 원문: 크로미엄 third_party/blink/renderer/platform/text/character_property_data.h 의 표, 스키아 src/core/SkTextFormatParams.h 의 가짜 굵게 표, src/ports/SkScalerContext_win_dw.cpp 의 DirectWrite 렌더링 고르기, src/ports/SkTypeface_fontations.cpp 와 src/ports/fontations/src/hinting.rs 의 힌팅 고르기에서 옮김, BSD-3-Clause, THIRD_PARTY_LICENSES.md
use crate::core::colorglyph;
use std::rc::Rc;
use unicode_bidi::{BidiInfo, Level};
use unicode_properties::{EmojiStatus, GeneralCategory, UnicodeEmoji, UnicodeGeneralCategory};
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

/// 가짜 기울임의 기울기. (기준선에서 1 픽셀 올라갈 때 오른쪽으로 밀리는 픽셀, Chromium setSkewX(-1/4))
pub const SYNTHETIC_ITALIC_SKEW: f32 = 0.25;

/// 가짜 굵게 두께 표. (글자 크기 → 크기에 곱할 비율, Skia kStdFakeBoldInterpKeys / Values)
const SYNTHETIC_BOLD_SIZE_KEYS: [f32; 2] = [9.0, 36.0];
const SYNTHETIC_BOLD_SIZE_RATIOS: [f32; 2] = [1.0 / 24.0, 1.0 / 32.0];

/// HarfBuzz 자리의 1 픽셀. (크롬은 hb_position_t 를 16.16 고정 소수로 쓴다, skia_text_metrics.h 의 kHbPosition1)
const HARFBUZZ_POSITION_ONE: f32 = 65536.0;

/// 굵게로 보는 굵기의 문턱. (Blink kBoldThreshold)
pub const BOLD_WEIGHT_THRESHOLD: u16 = 600;

/// 브라우저 시스템 글꼴로 그리는 조각의 얼굴 자리. (웹에서 올린 어느 글꼴에도 없는 글자, ShapedGlyph.face_index)
pub const SYSTEM_FACE_INDEX: usize = usize::MAX;

/// 시스템 글꼴 조각 재기. (조각 글, 오른쪽에서 왼쪽인지 → 전진폭 픽셀)
pub type SystemFontMeasure<'a> = &'a dyn Fn(&str, bool) -> f32;

//==============================================================================
/// 글 방향. (원본 Canvas2D direction 의 실제 값 "ltr" / "rtl")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapingDirection {
	/// 왼쪽에서 오른쪽. (원본 "ltr")
	LeftToRight,
	/// 오른쪽에서 왼쪽. (원본 "rtl")
	RightToLeft,
}

//==============================================================================
/// 글꼴 얼굴 서술자. (원본 FontFace 의 descriptors: weight, style, unicodeRange)
/// - 굵기는 범위로 든다. (가변 글꼴 "100 900" 처럼, 한 값이면 처음과 끝이 같다)
/// - 글자 범위가 비었으면 모든 글자다.
/// - source_key 는 같은 이름 아래에서 얼굴을 가리키는 열쇠다. (보통 파일 경로, 차례 정하기와 지우기에 쓴다)
///
/// # 예제
/// CSS 서술자 글로 만들고 글자 범위 보기
/// ```rust
/// use vanilla::FontFaceDescriptors;
///
/// let font_face_descriptors = FontFaceDescriptors::create_from_css("100 900", "italic", "U+AC00-D7A3");
/// assert_eq!((font_face_descriptors.minimum_weight, font_face_descriptors.maximum_weight), (100, 900));
/// assert!(font_face_descriptors.is_italic);
/// assert!(font_face_descriptors.contains_character('\u{AC00}'));
/// assert!(!font_face_descriptors.contains_character('A'));
/// assert!(FontFaceDescriptors::new().contains_character('A'));
/// assert_eq!(FontFaceDescriptors::create_with_bold(true).minimum_weight, 700);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFaceDescriptors {
	/// 굵기 범위의 처음. (CSS 굵기 1 ~ 1000, 기본 400)
	pub minimum_weight: u16,
	/// 굵기 범위의 끝. (한 값이면 minimum_weight 와 같다)
	pub maximum_weight: u16,
	/// 기울인 얼굴인지.
	pub is_italic: bool,
	/// 글자 범위 목록. ((처음, 끝) 코드 포인트, 끝 포함. 비었으면 모든 글자)
	pub unicode_range_list: Vec<(u32, u32)>,
	/// 얼굴 열쇠. (기본 빈 글)
	pub source_key: String,
	/// 운영체제가 고른 대체 글꼴의 얼굴인지. (대체 폰트와 마지막 대체 글꼴도, 글자마다 이름 목록의 얼굴을 다 본 뒤에 보는 대체 단계, 이모지 표시는 이 단계에서 색 글리프 얼굴을 먼저 고른다. 기본 false)
	pub is_system_fallback: bool,
}

impl FontFaceDescriptors {
	//==============================================================================
	/// 생성. (굵기 400, 기울이지 않음, 모든 글자. 원본 서술자를 주지 않은 FontFace 와 같다)
	///
	/// # 돌려줌
	/// 굵기 400, 기울이지 않음, 모든 글자, 빈 열쇠의 서술자.
	//==============================================================================
	pub fn new() -> FontFaceDescriptors {
		FontFaceDescriptors {
			minimum_weight: 400,
			maximum_weight: 400,
			is_italic: false,
			unicode_range_list: Vec::new(),
			source_key: String::new(),
			is_system_fallback: false,
		}
	}

	//==============================================================================
	/// 굵은 얼굴인지로 생성. (굵으면 700, 아니면 400)
	///
	/// # 인자
	/// - `is_bold`: true 면 굵기 700, false 면 400.
	///
	/// # 돌려줌
	/// 서술자. (굵기 밖은 new 와 같다)
	//==============================================================================
	pub fn create_with_bold(is_bold: bool) -> FontFaceDescriptors {
		let mut font_face_descriptors = FontFaceDescriptors::new();
		if is_bold {
			font_face_descriptors.set_weight(700);
		}
		font_face_descriptors
	}

	//==============================================================================
	/// CSS 서술자 글로 생성. (원본 { weight, style, unicodeRange }, 빈 글은 기본값)
	/// - weight: "400", "bold", "100 900". style: "normal", "italic", "oblique". unicode_range: "U+0000-00FF, U+0131, U+4??"
	///
	/// # 인자
	/// - `weight`: CSS font-weight 글. (빈 글이나 모르는 글이면 400)
	/// - `style`: CSS font-style 글. (italic 이나 oblique 로 시작하면 기울임, 대소문자는 가리지 않는다)
	/// - `unicode_range`: CSS unicode-range 글. (빈 글이면 모든 글자)
	///
	/// # 돌려줌
	/// 서술자. (source_key 는 빈 글)
	//==============================================================================
	pub fn create_from_css(weight: &str, style: &str, unicode_range: &str) -> FontFaceDescriptors {
		let mut font_face_descriptors = FontFaceDescriptors::new();
		font_face_descriptors.set_weight_from_css(weight);
		let lowercase_style = style.trim().to_lowercase();
		font_face_descriptors.is_italic = lowercase_style.starts_with("italic") || lowercase_style.starts_with("oblique");
		font_face_descriptors.unicode_range_list = parse_unicode_range(unicode_range);
		font_face_descriptors
	}

	//==============================================================================
	/// 굵기 한 값 설정.
	///
	/// # 인자
	/// - `weight`: 굵기. (처음과 끝을 이 값으로)
	//==============================================================================
	pub fn set_weight(&mut self, weight: u16) {
		self.minimum_weight = weight;
		self.maximum_weight = weight;
	}

	//==============================================================================
	/// CSS 굵기 글로 설정. ("normal", "bold", "500", "100 900", 모르는 글은 그대로 둔다)
	///
	/// # 인자
	/// - `weight`: CSS 굵기 글. (빈칸으로 나눈 값이 하나면 그 값, 둘 이상이면 앞의 둘로 범위)
	//==============================================================================
	pub fn set_weight_from_css(&mut self, weight: &str) {
		let weight_values: Vec<u16> = weight.split_whitespace().filter_map(parse_css_weight).collect();
		match weight_values.len() {
			0 => {}
			1 => {
				self.set_weight(weight_values[0]);
			}
			_ => {
				self.minimum_weight = weight_values[0].min(weight_values[1]);
				self.maximum_weight = weight_values[0].max(weight_values[1]);
			}
		}
	}

	//==============================================================================
	/// 글자가 글자 범위에 드는지. (범위가 비었으면 늘 든다)
	///
	/// # 인자
	/// - `character`: 볼 글자.
	///
	/// # 돌려줌
	/// 범위에 들면 true.
	//==============================================================================
	pub fn contains_character(&self, character: char) -> bool {
		if self.unicode_range_list.is_empty() {
			return true;
		}
		let code_point = character as u32;
		self.unicode_range_list.iter().any(|(start, end)| code_point >= *start && code_point <= *end)
	}

	//==============================================================================
	/// 글에 글자 범위에 드는 글자가 있는지.
	///
	/// # 인자
	/// - `text`: 볼 글.
	///
	/// # 돌려줌
	/// 범위에 드는 글자가 하나라도 있으면 true. (빈 글은 false)
	//==============================================================================
	pub fn contains_any_character(&self, text: &str) -> bool {
		text.chars().any(|character| self.contains_character(character))
	}
}

impl Default for FontFaceDescriptors {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> FontFaceDescriptors {
		FontFaceDescriptors::new()
	}
}

//==============================================================================
/// CSS 굵기 값 하나 읽기. ("normal" 400, "bold" 700, 숫자 1 ~ 1000)
///
/// # 인자
/// - `weight_text`: CSS 굵기 값 하나. (앞뒤 빈칸은 떼고 대소문자는 가리지 않는다)
///
/// # 돌려줌
/// 굵기. (소수는 반올림) 1 ~ 1000 밖이거나 읽지 못하면 None.
///
/// # 예제
/// 낱말, 숫자, 범위 밖
/// ```rust
/// use vanilla::core::textshaping::parse_css_weight;
///
/// assert_eq!(parse_css_weight("normal"), Some(400));
/// assert_eq!(parse_css_weight(" BOLD "), Some(700));
/// assert_eq!(parse_css_weight("450.6"), Some(451));
/// assert_eq!(parse_css_weight("1001"), None);
/// assert_eq!(parse_css_weight("heavy"), None);
/// ```
//==============================================================================
pub fn parse_css_weight(weight_text: &str) -> Option<u16> {
	let lowercase_weight_text = weight_text.trim().to_lowercase();
	match lowercase_weight_text.as_str() {
		"normal" => {
			Some(400)
		}
		"bold" => {
			Some(700)
		}
		_ => {
			let weight = lowercase_weight_text.parse::<f32>().ok()?;
			if (1.0..=1000.0).contains(&weight) {
				Some(weight.round() as u16)
			}
			else {
				None
			}
		}
	}
}

//==============================================================================
/// unicode-range 글 → 코드 포인트 범위 목록. ("U+0-FF, U+4??" → [(0, 255), (1024, 1279)], 비었으면 빈 목록 = 모든 글자)
/// - 원본 textstringtexturecache.js, uifont.js 의 parseUnicodeRange 와 같다.
///
/// # 인자
/// - `unicode_range`: 쉼표로 이은 범위 글. (U+ 나 u+ 로 시작하지 않거나 16 진수가 아닌 조각은 넘긴다)
///
/// # 돌려줌
/// (처음, 끝) 코드 포인트 목록. (끝 포함)
///
/// # 예제
/// 범위, 한 글자, ? 자리
/// ```rust
/// use vanilla::core::textshaping::parse_unicode_range;
///
/// assert_eq!(parse_unicode_range("U+0-FF, U+4??"), vec![(0, 255), (0x400, 0x4FF)]);
/// assert_eq!(parse_unicode_range("U+0131"), vec![(0x131, 0x131)]);
/// assert!(parse_unicode_range("").is_empty());
/// ```
//==============================================================================
pub fn parse_unicode_range(unicode_range: &str) -> Vec<(u32, u32)> {
	let mut range_list: Vec<(u32, u32)> = Vec::new();
	for range_text in unicode_range.split(',') {
		let range_text = range_text.trim();
		let body = match range_text.strip_prefix("U+").or_else(|| range_text.strip_prefix("u+")) {
			Some(body) => {
				body
			}
			None => {
				continue;
			}
		};
		let (start_text, end_text) = match body.split_once('-') {
			Some((start_text, end_text)) => {
				(start_text.to_string(), end_text.to_string())
			}
			None => {
				(body.replace('?', "0"), body.replace('?', "F"))
			}
		};
		let start_text = start_text.replace('?', "0");
		if let (Ok(start), Ok(end)) = (u32::from_str_radix(&start_text, 16), u32::from_str_radix(&end_text, 16)) {
			range_list.push((start, end));
		}
	}
	range_list
}

//==============================================================================
/// 글꼴 바이트와 그것을 빌린 모양 짓기 얼굴. (바이트를 같은 값 안에 들어 얼굴이 바이트보다 오래 살지 않는다)
//==============================================================================
type ShapingFace<'font> = rustybuzz::Face<'font>;

self_cell::self_cell!(
	struct ShapingFaceCell {
		owner: Rc<[u8]>,

		#[covariant]
		dependent: ShapingFace,
	}
);

//==============================================================================
/// 읽은 글꼴 하나. (래스터용 fontdue, 모양 짓기용 rustybuzz 얼굴과 그 바이트, 색 글리프 표가 있는지)
///
/// # 예제
/// 글꼴 바이트 읽기
/// ```rust
/// use vanilla::core::textshaping::LoadedFont;
/// use vanilla::platform::find_fallback_font_file;
///
/// assert!(LoadedFont::create_from_bytes(vec![0, 1, 2, 3]).is_err());
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let font = LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font");
/// 	assert!(font.has_character('A'));
/// 	assert!(font.units_per_em > 0.0);
/// 	assert_eq!(font.get_face_index(), 0);
/// 	assert!(!font.is_variation_applied());
/// }
/// ```
//==============================================================================
pub struct LoadedFont {
	/// 래스터용 fontdue 글꼴.
	pub raster_font: fontdue::Font,
	shaping_face_cell: ShapingFaceCell,
	/// 글꼴 단위의 em 크기. (head 표의 unitsPerEm)
	pub units_per_em: f32,
	/// 색 글리프 표(sbix, CBDT, COLR)가 있는지.
	pub has_color_tables: bool,
	/// 글꼴 모음 안의 얼굴 번호. (TTC 가 아니면 0)
	face_index: u32,
	/// 건 축 값. (가변 글꼴의 인스턴스, 비었으면 기본 인스턴스)
	variation_list: Vec<rustybuzz::Variation>,
	/// 가변 글꼴인지. (fvar 표가 있다, 래스터와 글리프 상자를 윤곽에서 직접 셈한다)
	is_variable_font: bool,
	/// 글꼴 파일 경로. (시스템 글꼴처럼 파일에서 읽었으면, 맥 CoreText 가 같은 파일로 그린다)
	source_path: Option<std::path::PathBuf>,
	/// 맥 CoreText 로 그리는지. (파일 경로가 있고 색 이모지(sbix)이거나 윤곽 표(glyf, CFF, CFF2)가 없는 글꼴)
	is_core_text_raster: bool,
	/// opsz 축의 처음과 끝. (축이 없으면 None)
	optical_size_range: Option<(f32, f32)>,
	/// opsz 축을 글자 크기로 거는지. (크롬 font-optical-sizing auto, 윈도우와 리눅스의 시스템 글꼴은 끈다)
	is_optical_sizing_auto: bool,
	/// trak 표의 가로 자간. (크기 목록과 자간 값(track) → 크기마다의 값(글꼴 단위), 표가 없으면 None)
	horizontal_track_table: Option<(Vec<f32>, Vec<(f32, Vec<i16>)>)>,
}

impl LoadedFont {
	//==============================================================================
	/// 풀린 글꼴 바이트(ttf, otf, ttc)로 읽기. (모음이면 0 번 얼굴)
	///
	/// # 인자
	/// - `font_bytes`: 글꼴 바이트. (WOFF, WOFF2 는 풀어서 줘야 한다)
	///
	/// # 돌려줌
	/// 읽은 글꼴. 읽지 못하면 Err(까닭).
	//==============================================================================
	pub fn create_from_bytes(font_bytes: Vec<u8>) -> Result<LoadedFont, String> {
		LoadedFont::create_from_shared_bytes(Rc::from(font_bytes), 0, &[])
	}

	//==============================================================================
	/// 나눠 든 바이트의 얼굴 하나 읽기. (모음 안의 얼굴 번호, 가변 글꼴의 축 값)
	///
	/// # 인자
	/// - `font_bytes`: 나눠 드는 글꼴 바이트.
	/// - `face_index`: 모음 안의 얼굴 번호. (모음이 아니면 0)
	/// - `variation_list`: 걸 축 값. (비었으면 기본 인스턴스)
	///
	/// # 돌려줌
	/// 읽은 글꼴. 읽지 못하거나 그 번호의 얼굴이 없으면 Err(까닭).
	//==============================================================================
	pub fn create_from_shared_bytes(font_bytes: Rc<[u8]>, face_index: u32, variation_list: &[rustybuzz::Variation]) -> Result<LoadedFont, String> {
		let font_settings = fontdue::FontSettings {
			collection_index: face_index,
			..fontdue::FontSettings::default()
		};
		let raster_font = fontdue::Font::from_bytes(&*font_bytes, font_settings).map_err(|error| format!("폰트 읽기 실패: {}", error))?;
		let shaping_face_cell = ShapingFaceCell::try_new(font_bytes, |font_bytes| {
			let mut shaping_face = rustybuzz::Face::from_slice(font_bytes, face_index).ok_or_else(|| String::from("폰트 읽기 실패: 모양 짓기 얼굴을 만들 수 없음"))?;
			shaping_face.set_variations(variation_list);
			Ok::<ShapingFace, String>(shaping_face)
		})?;
		let shaping_face = shaping_face_cell.borrow_dependent();
		let units_per_em = shaping_face.units_per_em() as f32;
		let has_color_tables = colorglyph::has_color_tables(shaping_face);
		let is_variable_font = shaping_face.tables().fvar.is_some();
		let optical_size_tag = rustybuzz::ttf_parser::Tag::from_bytes(b"opsz");
		let optical_size_range = shaping_face.variation_axes().into_iter().find(|variation_axis| variation_axis.tag == optical_size_tag).map(|variation_axis| (variation_axis.min_value, variation_axis.max_value));
		// HarfBuzz 15 는 STAT 표가 있는 글꼴(새 글꼴)에만 trak 을 건다. (hb-ot-shape.cc 의 apply_trak, Apple Chancery 처럼 STAT 이 없으면 자간 표가 있어도 걸지 않는다)
		let has_style_attribute_table = shaping_face.raw_face().table(rustybuzz::ttf_parser::Tag::from_bytes(b"STAT")).is_some();
		let horizontal_track_table = shaping_face.tables().trak.filter(|_| has_style_attribute_table).map(|track_table| {
			let size_list: Vec<f32> = track_table.horizontal.sizes.into_iter().map(|size| size.0).collect();
			let track_list: Vec<(f32, Vec<i16>)> = track_table.horizontal.tracks.into_iter().map(|track| (track.value, track.values.into_iter().collect())).collect();
			(size_list, track_list)
		}).filter(|(size_list, track_list)| !size_list.is_empty() && !track_list.is_empty());
		Ok(LoadedFont {
			raster_font,
			shaping_face_cell,
			units_per_em,
			has_color_tables,
			face_index,
			variation_list: variation_list.to_vec(),
			is_variable_font,
			source_path: None,
			is_core_text_raster: false,
			optical_size_range,
			is_optical_sizing_auto: true,
			horizontal_track_table,
		})
	}

	//==============================================================================
	/// trak 표의 가로 자간. (글꼴 단위, ptem 은 글자 크기 픽셀. 표나 STAT 표가 없거나 맥 색 이모지면 0. HarfBuzz 15 의 TrackData::get_tracking 에서 track 0)
	/// - 크기: 표의 크기 사이는 곧게 잇고, 처음보다 작으면 첫 값, 끝보다 크면 끝 값.
	/// - track: 0 과 같은 줄이 있으면 그 줄, 없으면 0 을 사이에 둔 두 줄을 곧게 잇는다.
	///
	/// # 인자
	/// - `ptem`: 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// 자간. (글꼴 단위)
	//==============================================================================
	pub fn read_horizontal_tracking(&self, ptem: f32) -> f32 {
		if self.is_integer_advance() {
			return 0.0;
		}
		let (size_list, track_list) = match self.horizontal_track_table.as_ref() {
			Some(horizontal_track_table) => {
				horizontal_track_table
			}
			None => {
				return 0.0;
			}
		};
		let read_track_value = |value_list: &Vec<i16>| -> f32 {
			let read_value = |value_index: usize| -> f32 {
				value_list.get(value_index).copied().unwrap_or(0) as f32
			};
			if size_list.len() == 1 {
				return read_value(0);
			}
			let size_index = match size_list.iter().position(|size| *size >= ptem) {
				Some(size_index) => {
					size_index
				}
				None => {
					return read_value(size_list.len() - 1);
				}
			};
			if size_index == 0 {
				return read_value(0);
			}
			if size_list[size_index] == ptem {
				return read_value(size_index);
			}
			let (mut first_size, mut second_size) = (size_list[size_index - 1], size_list[size_index]);
			let (mut first_value, mut second_value) = (read_value(size_index - 1), read_value(size_index));
			if second_size < first_size {
				std::mem::swap(&mut first_size, &mut second_size);
				std::mem::swap(&mut first_value, &mut second_value);
			}
			if ptem < first_size {
				return first_value;
			}
			if ptem > second_size {
				return second_value;
			}
			if first_size == second_size {
				return (first_value + second_value) * 0.5;
			}
			let interpolation = (ptem - first_size) / (second_size - first_size);
			first_value + interpolation * (second_value - first_value)
		};
		if track_list.len() == 1 {
			return read_track_value(&track_list[0].1);
		}
		let mut lower_index = 0;
		let mut upper_index = track_list.len() - 1;
		while lower_index + 1 < track_list.len() && track_list[lower_index + 1].0 <= 0.0 {
			lower_index += 1;
		}
		while upper_index > 0 && track_list[upper_index - 1].0 >= 0.0 {
			upper_index -= 1;
		}
		if lower_index == upper_index {
			return read_track_value(&track_list[lower_index].1);
		}
		let (lower_track, upper_track) = (track_list[lower_index].0, track_list[upper_index].0);
		let lower_value = read_track_value(&track_list[lower_index].1);
		let upper_value = read_track_value(&track_list[upper_index].1);
		if lower_track == upper_track {
			return (lower_value + upper_value) * 0.5;
		}
		let interpolation = (0.0 - lower_track) / (upper_track - lower_track);
		lower_value + interpolation * (upper_value - lower_value)
	}

	//==============================================================================
	/// opsz 축을 글자 크기로 걸지 정하기. (처음은 건다. 크롬은 윈도우와 리눅스의 시스템 글꼴에는 걸지 않는다)
	///
	/// # 인자
	/// - `is_optical_sizing_auto`: true 면 건다.
	///
	/// # 돌려줌
	/// 바꾼 글꼴.
	//==============================================================================
	pub fn with_optical_sizing_auto(mut self, is_optical_sizing_auto: bool) -> LoadedFont {
		self.is_optical_sizing_auto = is_optical_sizing_auto;
		self
	}

	//==============================================================================
	/// 글자 크기에 거는 opsz 축 값. (축이 있고 거는 글꼴이면 크기를 축 범위로 자른 값, 아니면 None)
	///
	/// # 인자
	/// - `pixel_size`: 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// 축 값. opsz 축이 없거나 걸지 않기로 한 글꼴이면 None.
	//==============================================================================
	pub fn read_optical_size(&self, pixel_size: f32) -> Option<f32> {
		if !self.is_optical_sizing_auto {
			return None;
		}
		let (minimum_value, maximum_value) = self.optical_size_range?;
		Some(pixel_size.clamp(minimum_value, maximum_value))
	}

	//==============================================================================
	/// 글자 크기의 축 값 목록. (건 축 값에 opsz 를 더한 것)
	//==============================================================================
	fn read_sized_variation_list(&self, pixel_size: f32) -> Vec<rustybuzz::Variation> {
		let mut variation_list = self.variation_list.clone();
		if let Some(optical_size) = self.read_optical_size(pixel_size) {
			let optical_size_tag = rustybuzz::ttf_parser::Tag::from_bytes(b"opsz");
			variation_list.retain(|variation| variation.tag != optical_size_tag);
			variation_list.push(rustybuzz::Variation {
				tag: optical_size_tag,
				value: optical_size,
			});
		}
		variation_list
	}

	//==============================================================================
	/// 글자 크기의 모양 짓기 얼굴. (opsz 축을 건다. trak 자간은 shape_text 가 따로 더하므로 rustybuzz 의 ptem 은 두지 않는다)
	///
	/// # 인자
	/// - `pixel_size`: 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// 모양 짓기 얼굴의 사본. (opsz 를 걸지 않는 글꼴이면 그대로)
	//==============================================================================
	pub fn create_sized_shaping_face(&self, pixel_size: f32) -> rustybuzz::Face<'_> {
		let mut shaping_face = self.get_shaping_face().clone();
		if let Some(optical_size) = self.read_optical_size(pixel_size) {
			shaping_face.set_variation(rustybuzz::ttf_parser::Tag::from_bytes(b"opsz"), optical_size);
		}
		shaping_face
	}

	//==============================================================================
	/// 글꼴 파일 경로 적기. (파일에서 읽은 글꼴. 맥은 색 이모지와 윤곽 표가 없는 글꼴을 CoreText 로 그리게 한다)
	///
	/// # 인자
	/// - `source_path`: 글꼴 파일 경로.
	///
	/// # 돌려줌
	/// 경로를 적은 글꼴.
	//==============================================================================
	pub fn with_source_path(mut self, source_path: std::path::PathBuf) -> LoadedFont {
		#[cfg(target_os = "macos")]
		{
			let tables = self.get_shaping_face().tables();
			let has_outline_table = tables.glyf.is_some() || tables.cff.is_some() || tables.cff2.is_some();
			self.is_core_text_raster = tables.sbix.is_some() || !has_outline_table;
		}
		self.source_path = Some(source_path);
		self
	}

	//==============================================================================
	/// 맥 CoreText 로 그리는지.
	///
	/// # 돌려줌
	/// CoreText 로 그리면 true. (맥 밖은 늘 false)
	//==============================================================================
	pub fn is_core_text_raster(&self) -> bool {
		self.is_core_text_raster
	}

	//==============================================================================
	/// 모양 짓기 전진폭에 곱할 비. (맥 색 이모지는 CoreText 가 작은 크기에서 키워 그리는 비, 그 밖은 1)
	///
	/// # 인자
	/// - `pixel_size`: 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// 비. (맥 색 이모지 밖은 1)
	//==============================================================================
	pub fn read_advance_scale(&self, pixel_size: f32) -> f32 {
		#[cfg(target_os = "macos")]
		{
			if self.is_core_text_raster && self.get_shaping_face().tables().sbix.is_some() {
				if let Some(source_path) = self.source_path.as_ref() {
					return crate::platform::desktop::coretextglyph::read_emoji_scale(source_path, self.face_index, pixel_size);
				}
			}
		}
		let _ = pixel_size;
		1.0
	}

	//==============================================================================
	/// 비트맵 그림만 있는 색 글꼴인지. (CBDT 나 sbix 가 있고 윤곽 표(glyf, CFF, CFF2)가 없는 글꼴, 맥 CoreText 로 그리는 글꼴은 아니다)
	/// - 크롬 리눅스(FreeType 과 스키아)처럼 전진폭과 픽셀 상자를 판의 글리프 수치와 그림 상자로 셈한다. (colorglyph::read_bitmap_glyph_advance, read_bitmap_glyph_bounds)
	///
	/// # 돌려줌
	/// 비트맵만 있으면 true.
	//==============================================================================
	pub fn is_bitmap_only_font(&self) -> bool {
		if self.is_core_text_raster {
			return false;
		}
		let tables = self.get_shaping_face().tables();
		(tables.cbdt.is_some() || tables.sbix.is_some()) && tables.glyf.is_none() && tables.cff.is_none() && tables.cff2.is_none()
	}

	//==============================================================================
	/// 전진폭을 정수 픽셀로 반올림하는지. (맥 색 이모지, 크롬 measureText 가 13px 에서 16, 17px 에서 21 처럼 정수를 준다)
	///
	/// # 돌려줌
	/// 반올림하면 true. (CoreText 로 그리는 맥 색 이모지만)
	//==============================================================================
	pub fn is_integer_advance(&self) -> bool {
		self.is_core_text_raster && self.get_shaping_face().tables().sbix.is_some()
	}

	//==============================================================================
	/// 맥 CoreText 에 줄 축 값. ((4 글자 태그의 수, 값) 목록)
	//==============================================================================
	#[cfg(target_os = "macos")]
	fn read_core_text_variation_list(&self, pixel_size: f32) -> Vec<(u32, f32)> {
		self.read_sized_variation_list(pixel_size).iter().map(|variation| (variation.tag.0, variation.value)).collect()
	}

	//==============================================================================
	/// 모양 짓기 얼굴. (축 값을 건 얼굴, 글꼴 표도 이것으로 읽는다)
	///
	/// # 돌려줌
	/// rustybuzz 얼굴.
	//==============================================================================
	pub fn get_shaping_face(&self) -> &rustybuzz::Face<'_> {
		self.shaping_face_cell.borrow_dependent()
	}

	//==============================================================================
	/// CSS line-height normal 의 줄 높이. (크롬 SimpleFontData::PlatformInit 의 LineSpacing)
	/// - 크롬은 Skia 가 주는 위아래 길이(fAscent, fDescent)를 각각 반올림하고(FontMetrics::AscentDescentWithHacks), 줄 높이는 lroundf(위) + lroundf(아래) + lroundf(줄 사이 fLeading)다.
	/// - Skia 가 읽는 표는 운영체제마다 다르다.
	///   - 리눅스(SkFontHost_FreeType.cpp): OS/2 의 USE_TYPO_METRICS 가 켜져 있으면 typo 값, 아니면 FreeType 의 hhea 값(hhea 가 0 이면 typo, 그것도 0 이면 win 값).
	///     ttf-parser 의 ascender, descender, line_gap 이 이 차례로 읽는다.
	///   - 맥(CoreText): USE_TYPO_METRICS 를 보지 않고 hhea 값이다. (헤드리스 크롬 맥에서 그 비트를 켠 글꼴의 줄 높이가 그대로인 것으로 확인)
	///   - 윈도우(DirectWrite 의 DWRITE_FONT_METRICS): USE_TYPO_METRICS 면 typo 값, 아니면 win 값과 hhea 줄 높이가 win 높이를 넘는 만큼의 줄 사이로 본다.
	///     (DirectWrite 는 고르는 표를 밝히지 않아, 글꼴 세로 수치 안내에 알려진 셈을 쓴다)
	/// - 맥 크롬은 Times, Helvetica, Courier 의 위 길이에 (위 + 아래) 의 15% 를 반올림해 더한다. (사파리의 SimpleFontDataMac 과 맞춘 값)
	///
	/// # 인자
	/// - `pixel_size`: 글자 크기. (px)
	///
	/// # 돌려줌
	/// 줄 높이. (px, 정수)
	//==============================================================================
	pub fn calculate_normal_line_height(&self, pixel_size: f32) -> f32 {
		let scale = pixel_size / self.units_per_em.max(1.0);
		let (ascender, descender, line_gap) = self.read_vertical_metric_units();
		let mut ascent = (ascender * scale).round();
		let descent = (-descender * scale).round();
		let line_gap = (line_gap * scale).round();
		if self.is_ascent_adjusted_family() {
			ascent += ((ascent + descent) * 0.15 + 0.5).floor();
		}
		ascent + descent + line_gap
	}

	//==============================================================================
	/// 캔버스의 글꼴 위아래 길이. (크롬 FontMetrics::AscentDescentWithHacks 에 캔버스가 켜는 subpixel_ascent_descent 를 준 값, TextMetrics 의 fontBoundingBoxAscent, fontBoundingBoxDescent)
	/// - 운영체제마다 읽는 표는 calculate_normal_line_height 와 같다.
	/// - 위아래 길이를 각각 반올림한다. 위가 3px 보다 작거나 위 + 아래가 2px 보다 작은 작은 글꼴은 반올림하지 않는다. (crbug.com/338908, 글자 기준선 종류마다 기준선이 같아지지 않게)
	/// - 맥은 Times, Helvetica, Courier 의 위 길이에 (위 + 아래) 의 15% 를 반올림해 더한다.
	/// - 리눅스의 VDMX 표와 서브픽셀 자리의 아래 길이 빌리기는 FreeType 바이트코드 힌팅(full, normal)과 서브픽셀 자리를 켰을 때만이라, 크롬 리눅스 기본(slight 힌팅, 서브픽셀 자리 꺼짐)처럼 보지 않는다.
	///
	/// # 인자
	/// - `pixel_size`: 글자 크기. (px)
	///
	/// # 돌려줌
	/// (위 길이, 아래 길이). (px, 둘 다 양수, 글자 크기가 0 이면 둘 다 0)
	//==============================================================================
	pub fn calculate_font_bounding_box(&self, pixel_size: f32) -> (f32, f32) {
		if pixel_size <= 0.0 {
			return (0.0, 0.0);
		}
		let scale = pixel_size / self.units_per_em.max(1.0);
		let (ascender, descender, _) = self.read_vertical_metric_units();
		let raw_ascent = ascender * scale;
		let raw_descent = -descender * scale;
		let (mut ascent, descent) = if raw_ascent < 3.0 || raw_ascent + raw_descent < 2.0 {
			(raw_ascent, raw_descent)
		}
		else {
			(raw_ascent.round(), raw_descent.round())
		};
		if self.is_ascent_adjusted_family() {
			ascent += ((ascent + descent) * 0.15 + 0.5).floor();
		}
		(ascent, descent)
	}

	//==============================================================================
	/// 크롬이 Skia 에서 받는 글꼴 세로 수치. (글꼴 단위의 위, 아래(음수), 줄 사이. 운영체제마다 읽는 표는 calculate_normal_line_height 의 설명)
	//==============================================================================
	fn read_vertical_metric_units(&self) -> (f32, f32, f32) {
		let shaping_face = self.get_shaping_face();
		let font_tables = shaping_face.tables();
		let horizontal_header = font_tables.hhea;
		if cfg!(target_os = "macos") && (horizontal_header.ascender != 0 || horizontal_header.descender != 0) {
			(horizontal_header.ascender as f32, horizontal_header.descender as f32, horizontal_header.line_gap as f32)
		}
		else if cfg!(target_os = "windows") {
			match font_tables.os2 {
				Some(os2_table) if !os2_table.use_typographic_metrics() => {
					let windows_ascender = os2_table.windows_ascender() as f32;
					let windows_descender = os2_table.windows_descender() as f32;
					let header_height = horizontal_header.ascender as f32 - horizontal_header.descender as f32 + horizontal_header.line_gap as f32;
					(windows_ascender, windows_descender, (header_height - (windows_ascender - windows_descender)).max(0.0))
				}
				_ => {
					(shaping_face.ascender() as f32, shaping_face.descender() as f32, shaping_face.line_gap() as f32)
				}
			}
		}
		else {
			(shaping_face.ascender() as f32, shaping_face.descender() as f32, shaping_face.line_gap() as f32)
		}
	}

	//==============================================================================
	/// 맥 크롬이 위 길이를 고치는 글꼴인지. (이름 표의 글꼴 모음 이름이 Times, Helvetica, Courier, 맥이 아니면 늘 false)
	//==============================================================================
	fn is_ascent_adjusted_family(&self) -> bool {
		if !cfg!(target_os = "macos") {
			return false;
		}
		// 이름 표의 글꼴 모음 이름. (맥 글꼴은 맥 로먼 글자만 있기도 해 그 바이트를 그대로 견준다)
		self.get_shaping_face().names().into_iter().any(|name| {
			if name.name_id != rustybuzz::ttf_parser::name_id::FAMILY {
				return false;
			}
			let family_name = if name.is_unicode() {
				name.to_string()
			}
			else if name.platform_id == rustybuzz::ttf_parser::PlatformId::Macintosh && name.encoding_id == 0 {
				String::from_utf8(name.name.to_vec()).ok()
			}
			else {
				None
			};
			matches!(family_name.as_deref(), Some("Times") | Some("Helvetica") | Some("Courier"))
		})
	}

	//==============================================================================
	/// 글꼴 바이트. (같은 파일의 다른 얼굴이 나눠 든다)
	///
	/// # 돌려줌
	/// 글꼴 바이트.
	//==============================================================================
	pub fn get_font_bytes(&self) -> &Rc<[u8]> {
		self.shaping_face_cell.borrow_owner()
	}

	//==============================================================================
	/// 글꼴 모음 안의 얼굴 번호.
	///
	/// # 돌려줌
	/// 얼굴 번호. (모음이 아니면 0)
	//==============================================================================
	pub fn get_face_index(&self) -> u32 {
		self.face_index
	}

	//==============================================================================
	/// 축 값을 건 가변 글꼴 인스턴스인지.
	///
	/// # 돌려줌
	/// 축 값을 걸었으면 true.
	//==============================================================================
	pub fn is_variation_applied(&self) -> bool {
		!self.variation_list.is_empty()
	}

	//==============================================================================
	/// 글리프를 윤곽에서 직접 그리는지. (가변 글꼴. fontdue 는 축 값을 읽지 못하고 맥 SFNS 같은 가변 글꼴의 윤곽을 그리지 못한다)
	///
	/// # 돌려줌
	/// 가변 글꼴이거나 축 값을 걸었으면 true.
	//==============================================================================
	pub fn is_outline_raster(&self) -> bool {
		self.is_variable_font || !self.variation_list.is_empty()
	}

	//==============================================================================
	/// 글리프가 덮는 픽셀 상자. (펜 원점에서 왼쪽, 아래, 너비, 높이. fontdue Metrics 의 xmin, ymin, width, height 와 같은 뜻, 비었으면 너비와 높이가 0)
	///
	/// # 인자
	/// - `glyph_index`: 글리프 번호.
	/// - `pixel_size`: 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// (왼쪽, 아래, 너비, 높이). (픽셀, 위가 양수)
	//==============================================================================
	pub fn read_glyph_metrics(&self, glyph_index: u16, pixel_size: f32) -> (i32, i32, i32, i32) {
		#[cfg(target_os = "macos")]
		{
			if self.is_core_text_raster {
				if let Some(source_path) = self.source_path.as_ref() {
					return match crate::platform::desktop::coretextglyph::read_glyph_bounds(source_path, self.face_index, glyph_index, pixel_size, &self.read_core_text_variation_list(pixel_size)) {
						Some(glyph_rect) => {
							let left_x = glyph_rect.origin.x.floor() as i32;
							let bottom_y = glyph_rect.origin.y.floor() as i32;
							let right_x = (glyph_rect.origin.x + glyph_rect.size.width).ceil() as i32;
							let top_y = (glyph_rect.origin.y + glyph_rect.size.height).ceil() as i32;
							(left_x, bottom_y, right_x - left_x, top_y - bottom_y)
						}
						None => {
							(0, 0, 0, 0)
						}
					};
				}
			}
		}
		if self.is_outline_raster() {
			let scale = pixel_size / self.units_per_em;
			return match colorglyph::read_outline_bounds(&|outline_builder| self.draw_glyph_outline(glyph_index, pixel_size, outline_builder), scale) {
				Some((left_x, top_y, width, height)) => {
					(left_x, top_y - height, width, height)
				}
				None => {
					(0, 0, 0, 0)
				}
			};
		}
		if self.is_bitmap_only_font() {
			return colorglyph::read_bitmap_glyph_bounds(self.get_shaping_face(), glyph_index, pixel_size).unwrap_or((0, 0, 0, 0));
		}
		let metrics = self.raster_font.metrics_indexed(glyph_index, pixel_size);
		(metrics.xmin, metrics.ymin, metrics.width as i32, metrics.height as i32)
	}

	//==============================================================================
	/// measureText 의 닿는 상자에 쓰는 글리프 상자. (맥 크롬처럼 윤곽 경로의 상자, 블링크 skia_text_metrics.cc 의 SkFontGetBoundsForGlyph 가 맥에서 부르는
	/// GetPathBoundsForGlyph. 경로는 CoreText 윤곽(CTFontCreatePathForGlyph)이고 상자는 곡선 조절점까지 넣은 SkPath::getBounds 다)
	/// - 윤곽 표(glyf, CFF, CFF2)의 점(곡선 조절점 포함)을 그 크기의 축 값으로 읽은 상자를 픽셀로 줄인다. 반올림하지 않는다.
	/// - 윤곽을 엔진이 읽지 못하는 글꼴(색 이모지 sbix, 윤곽 표가 없는 hvgl 같은)과 윤곽이 빈 글리프는 None 이다. (크롬도 경로가 없으면 정수 상자 getBounds 다)
	///
	/// # 인자
	/// - `glyph_index`: 글리프 번호.
	/// - `pixel_size`: 글자 크기. (픽셀)
	/// - `skew`: 가로 기울임. (x 에 y x skew 를 더한다, 가짜 기울임은 SYNTHETIC_ITALIC_SKEW, 아니면 0. 크롬처럼 경로의 점을 기울인 뒤 상자를 잰다)
	///
	/// # 돌려줌
	/// (왼쪽, 아래, 오른쪽, 위). (펜 원점에서 픽셀, 위가 양수)
	//==============================================================================
	pub fn read_glyph_path_bounds(&self, glyph_index: u16, pixel_size: f32, skew: f32) -> Option<(f32, f32, f32, f32)> {
		if self.is_core_text_raster || self.is_bitmap_only_font() {
			return None;
		}
		let mut bounds_builder = PathBoundsBuilder {
			bounds: None,
			skew,
		};
		if !self.draw_glyph_outline(glyph_index, pixel_size, &mut bounds_builder) {
			return None;
		}
		let (left_x, bottom_y, right_x, top_y) = bounds_builder.bounds?;
		let scale = pixel_size / self.units_per_em;
		Some((left_x * scale, bottom_y * scale, right_x * scale, top_y * scale))
	}

	//==============================================================================
	/// measureText 의 닿는 상자에 쓰는 경로 없는 글리프(맥 색 이모지 같은 CoreText 래스터 글꼴)의 정수 상자. (스키아 getBounds 와 같은 셈, 맥이 아니거나 CoreText 래스터 글꼴이 아니면 None)
	/// - 스키아 SkScalerContext_Mac::generateMetrics: 스키아가 지은 CTFont 의 상자를 바깥 정수로 넓히고 사방 1 픽셀 더 넓힌다(outset(1, 1)). 가짜 기울임이면 상자를 기울인 뒤 넓힌다.
	///
	/// # 인자
	/// - `glyph_index`: 글리프 번호.
	/// - `pixel_size`: 글자 크기. (픽셀)
	/// - `skew`: 가로 기울임. (x 에 y x skew 를 더한다, 가짜 기울임이 아니면 0)
	///
	/// # 돌려줌
	/// (왼쪽, 아래, 오른쪽, 위). (펜 원점에서 픽셀, 위가 양수)
	//==============================================================================
	pub fn read_glyph_skia_bounds(&self, glyph_index: u16, pixel_size: f32, skew: f32) -> Option<(f32, f32, f32, f32)> {
		#[cfg(target_os = "macos")]
		{
			if !self.is_core_text_raster {
				return None;
			}
			let source_path = self.source_path.as_ref()?;
			let glyph_rect = crate::platform::desktop::coretextglyph::read_glyph_skia_bounds(source_path, self.face_index, glyph_index, pixel_size, &self.read_core_text_variation_list(pixel_size))?;
			let bottom_y = glyph_rect.origin.y as f32;
			let top_y = (glyph_rect.origin.y + glyph_rect.size.height) as f32;
			let left_x = glyph_rect.origin.x as f32 + (bottom_y * skew).min(top_y * skew);
			let right_x = (glyph_rect.origin.x + glyph_rect.size.width) as f32 + (bottom_y * skew).max(top_y * skew);
			Some((left_x.floor() - 1.0, bottom_y.floor() - 1.0, right_x.ceil() + 1.0, top_y.ceil() + 1.0))
		}
		#[cfg(not(target_os = "macos"))]
		{
			let _ = (glyph_index, pixel_size, skew);
			None
		}
	}

	//==============================================================================
	/// 글리프 윤곽 그리기. (글꼴 단위, 그 글자 크기의 축 값(opsz 포함)을 건 윤곽. 가변 글꼴은 skrifa 로, 아니면 ttf-parser 로. 윤곽이 없으면 false)
	/// - ttf-parser 는 글리프마다 변형이 32 개를 넘는 가변 글꼴(맥 SFNS 같은)의 윤곽을 읽지 못한다.
	///
	/// # 인자
	/// - `glyph_index`: 글리프 번호.
	/// - `pixel_size`: 글자 크기. (픽셀, 가변 글꼴의 opsz 축 값을 고르는 데만 쓴다)
	/// - `outline_builder`: 윤곽을 받을 쪽.
	///
	/// # 돌려줌
	/// 윤곽을 그렸으면 true.
	//==============================================================================
	pub fn draw_glyph_outline(&self, glyph_index: u16, pixel_size: f32, outline_builder: &mut dyn rustybuzz::ttf_parser::OutlineBuilder) -> bool {
		if !self.is_variable_font {
			return self.get_shaping_face().outline_glyph(rustybuzz::ttf_parser::GlyphId(glyph_index), outline_builder).is_some();
		}
		use skrifa::MetadataProvider;
		let font_ref = match skrifa::FontRef::from_index(self.get_font_bytes(), self.face_index) {
			Ok(font_ref) => {
				font_ref
			}
			Err(_) => {
				return false;
			}
		};
		let location = font_ref.axes().location(self.read_sized_variation_list(pixel_size).iter().map(|variation| (skrifa::Tag::new(&variation.tag.to_bytes()), variation.value)));
		let outline_glyph_collection = font_ref.outline_glyphs();
		let outline_glyph = match outline_glyph_collection.get(skrifa::GlyphId::new(glyph_index as u32)) {
			Some(outline_glyph) => {
				outline_glyph
			}
			None => {
				return false;
			}
		};
		let mut outline_pen = SkrifaOutlinePen {
			outline_builder,
		};
		outline_glyph.draw(skrifa::outline::DrawSettings::unhinted(skrifa::instance::Size::unscaled(), &location), &mut outline_pen).is_ok()
	}

	//==============================================================================
	/// 글자를 가졌는지.
	///
	/// # 인자
	/// - `character`: 볼 글자.
	///
	/// # 돌려줌
	/// 글리프가 있으면 true.
	//==============================================================================
	pub fn has_character(&self, character: char) -> bool {
		self.raster_font.lookup_glyph_index(character) != 0
	}

	//==============================================================================
	/// 글자를 색 글리프로 가졌는지. (sbix, CBDT, COLR)
	///
	/// # 인자
	/// - `character`: 볼 글자.
	///
	/// # 돌려줌
	/// 색 글리프가 있으면 true. (색 글리프 표가 없는 글꼴은 늘 false)
	//==============================================================================
	pub fn has_color_character(&self, character: char) -> bool {
		if !self.has_color_tables {
			return false;
		}
		let shaping_face = self.get_shaping_face();
		match shaping_face.glyph_index(character) {
			Some(glyph_id) => {
				colorglyph::has_color_glyph(shaping_face, glyph_id.0)
			}
			None => {
				false
			}
		}
	}
}

//==============================================================================
/// skrifa 윤곽을 ttf-parser 윤곽 받개로 넘기는 펜.

//==============================================================================
/// 윤곽 점의 상자 모으기. (곡선 조절점까지 넣는다, 스키아 SkPath::getBounds 와 같은 뜻. 글꼴 단위, 위가 양수)
//==============================================================================
struct PathBoundsBuilder {
	/// (왼쪽, 아래, 오른쪽, 위). 점이 없으면 None.
	bounds: Option<(f32, f32, f32, f32)>,
	/// 가로 기울임. (점의 x 에 y x skew 를 더한다)
	skew: f32,
}

impl PathBoundsBuilder {
	//==============================================================================
	/// 점 하나를 상자에 넣기.
	//==============================================================================
	fn add_point(&mut self, x: f32, y: f32) {
		let x = x + y * self.skew;
		self.bounds = Some(match self.bounds {
			Some((left_x, bottom_y, right_x, top_y)) => {
				(left_x.min(x), bottom_y.min(y), right_x.max(x), top_y.max(y))
			}
			None => {
				(x, y, x, y)
			}
		});
	}
}

impl rustybuzz::ttf_parser::OutlineBuilder for PathBoundsBuilder {
	fn move_to(&mut self, x: f32, y: f32) {
		self.add_point(x, y);
	}

	fn line_to(&mut self, x: f32, y: f32) {
		self.add_point(x, y);
	}

	fn quad_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32) {
		self.add_point(control_x, control_y);
		self.add_point(x, y);
	}

	fn curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, x: f32, y: f32) {
		self.add_point(first_control_x, first_control_y);
		self.add_point(second_control_x, second_control_y);
		self.add_point(x, y);
	}

	fn close(&mut self) {
	}
}

//==============================================================================
struct SkrifaOutlinePen<'builder> {
	outline_builder: &'builder mut dyn rustybuzz::ttf_parser::OutlineBuilder,
}

impl skrifa::outline::OutlinePen for SkrifaOutlinePen<'_> {
	//==============================================================================
	/// 새 윤곽 시작.
	//==============================================================================
	fn move_to(&mut self, position_x: f32, position_y: f32) {
		self.outline_builder.move_to(position_x, position_y);
	}

	//==============================================================================
	/// 곧은 선.
	//==============================================================================
	fn line_to(&mut self, position_x: f32, position_y: f32) {
		self.outline_builder.line_to(position_x, position_y);
	}

	//==============================================================================
	/// 2 차 곡선.
	//==============================================================================
	fn quad_to(&mut self, control_x: f32, control_y: f32, position_x: f32, position_y: f32) {
		self.outline_builder.quad_to(control_x, control_y, position_x, position_y);
	}

	//==============================================================================
	/// 3 차 곡선.
	//==============================================================================
	fn curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, position_x: f32, position_y: f32) {
		self.outline_builder.curve_to(first_control_x, first_control_y, second_control_x, second_control_y, position_x, position_y);
	}

	//==============================================================================
	/// 윤곽 닫기.
	//==============================================================================
	fn close(&mut self) {
		self.outline_builder.close();
	}
}

//==============================================================================
/// 글자 묶음이 이모지 표시인지. (UTS #51, 아니면 글 표시)
/// - VS15(FE0E) 가 있으면 글 표시, VS16(FE0F) 이나 키캡(20E3)이 있으면 이모지 표시다.
/// - 첫 글자가 Emoji_Presentation 이거나(국기의 지역 표시 글자 포함), 피부색 조정자나 태그 글자가 붙었거나, 이음 문자로 이은 이모지 묶음이면 이모지 표시다.
///   ❤ 처럼 Emoji 이지만 Emoji_Presentation 이 아닌 글자 하나는 글 표시다. (크롬도 맨 ❤ 는 글 글꼴로 그린다)
///
/// # 인자
/// - `grapheme`: 글자 묶음 하나.
///
/// # 돌려줌
/// 이모지 표시면 true. (빈 글은 false)
//==============================================================================
pub fn is_emoji_presentation(grapheme: &str) -> bool {
	let base_character = match grapheme.chars().next() {
		Some(base_character) => {
			base_character
		}
		None => {
			return false;
		}
	};
	if grapheme.contains('\u{FE0E}') {
		return false;
	}
	if grapheme.contains('\u{FE0F}') || grapheme.contains('\u{20E3}') {
		return true;
	}
	let is_presentation = matches!(base_character.emoji_status(), EmojiStatus::EmojiPresentation | EmojiStatus::EmojiPresentationAndModifierBase | EmojiStatus::EmojiPresentationAndEmojiComponent | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent);
	if is_presentation {
		return true;
	}
	let has_sequence_part = grapheme.chars().skip(1).any(|character| ('\u{1F3FB}'..='\u{1F3FF}').contains(&character) || ('\u{E0020}'..='\u{E007F}').contains(&character) || character == '\u{200D}');
	has_sequence_part && base_character.is_emoji_char()
}

//==============================================================================
/// 등록된 글꼴 얼굴 하나.
///
/// # 예제
/// 보통과 굵은 얼굴 가운데 굵게 고르기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{select_font_faces, LoadedFont, RegisteredFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::FontFaceDescriptors;
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let font = Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font"));
/// 	let regular_face = RegisteredFontFace {
/// 		descriptors: FontFaceDescriptors::create_with_bold(false),
/// 		font: font.clone(),
/// 		order_number: 1,
/// 	};
/// 	let bold_face = RegisteredFontFace {
/// 		descriptors: FontFaceDescriptors::create_with_bold(true),
/// 		font,
/// 		order_number: 2,
/// 	};
/// 	let mut resolved_face_list = Vec::new();
/// 	select_font_faces(&[&regular_face, &bold_face], 700, false, &mut resolved_face_list);
/// 	assert_eq!(resolved_face_list.len(), 1);
/// 	assert_eq!(resolved_face_list[0].descriptors.minimum_weight, 700);
/// 	assert!(!resolved_face_list[0].is_synthetic_bold);
/// }
/// ```
//==============================================================================
#[derive(Clone)]
pub struct RegisteredFontFace {
	/// 얼굴 서술자.
	pub descriptors: FontFaceDescriptors,
	/// 읽은 글꼴. (같은 글꼴을 여러 얼굴이 나눠 들 수 있다)
	pub font: Rc<LoadedFont>,
	/// 올린 차례. (클수록 나중, 나중 것이 먼저 그린다)
	pub order_number: u64,
}

//==============================================================================
/// 그릴 때 고른 얼굴 하나. (가짜 굵게, 가짜 기울임을 입힐지 함께 든다)
///
/// # 예제
/// 가짜 기울임은 전진폭을 바꾸지 않는다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{shape_text, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::{FontFaceDescriptors, ShapingDirection};
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let font = Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font"));
/// 	let upright_face = ResolvedFontFace {
/// 		font: font.clone(),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	};
/// 	let italic_face = ResolvedFontFace {
/// 		is_synthetic_italic: true,
/// 		..upright_face.clone()
/// 	};
/// 	let upright_text = shape_text(&[upright_face], "Hello", 20.0, ShapingDirection::LeftToRight, None);
/// 	let italic_text = shape_text(&[italic_face], "Hello", 20.0, ShapingDirection::LeftToRight, None);
/// 	assert_eq!(upright_text.advance_width, italic_text.advance_width);
/// }
/// ```
//==============================================================================
#[derive(Clone)]
pub struct ResolvedFontFace {
	/// 읽은 글꼴.
	pub font: Rc<LoadedFont>,
	/// 얼굴 서술자.
	pub descriptors: FontFaceDescriptors,
	/// 가짜 굵게를 입힐지. (청한 굵기가 600 이상이고 얼굴 굵기가 600 아래)
	pub is_synthetic_bold: bool,
	/// 가짜 기울임을 입힐지. (기울임을 청했는데 바른 얼굴)
	pub is_synthetic_italic: bool,
}

//==============================================================================
/// 굵기 고르기 점수. (작을수록 먼저, CSS Fonts 4 의 차례. 굵기 범위가 청한 굵기를 품으면 (0, 0))
/// - 청한 값이 400 ~ 500 이면 그 값부터 500 까지 오름, 그 아래 내림, 500 위 오름. 400 아래면 아래 내림, 위 오름. 500 위면 위 오름, 아래 내림.
///
/// # 인자
/// - `face_minimum_weight`: 얼굴 굵기 범위의 처음.
/// - `face_maximum_weight`: 얼굴 굵기 범위의 끝.
/// - `desired_weight`: 청한 굵기.
///
/// # 돌려줌
/// (갈래, 굵기 차이). (튜플로 견주어 작은 것이 먼저)
///
/// # 예제
/// 400 을 청하면 500, 300, 600 차례
/// ```rust
/// use vanilla::core::textshaping::calculate_weight_score;
///
/// assert_eq!(calculate_weight_score(100, 900, 700), (0, 0));
/// assert!(calculate_weight_score(500, 500, 400) < calculate_weight_score(300, 300, 400));
/// assert!(calculate_weight_score(300, 300, 400) < calculate_weight_score(600, 600, 400));
/// ```
//==============================================================================
pub fn calculate_weight_score(face_minimum_weight: u16, face_maximum_weight: u16, desired_weight: u16) -> (u32, u32) {
	let minimum_weight = face_minimum_weight as i32;
	let maximum_weight = face_maximum_weight as i32;
	let desired = desired_weight as i32;
	if desired >= minimum_weight && desired <= maximum_weight {
		return (0, 0);
	}
	if (400..=500).contains(&desired) {
		if minimum_weight > desired && minimum_weight <= 500 {
			return (1, (minimum_weight - desired) as u32);
		}
		if maximum_weight < desired {
			return (2, (desired - maximum_weight) as u32);
		}
		return (3, (minimum_weight - desired) as u32);
	}
	if desired < 400 {
		if maximum_weight < desired {
			return (1, (desired - maximum_weight) as u32);
		}
		return (2, (minimum_weight - desired) as u32);
	}
	if minimum_weight > desired {
		return (1, (minimum_weight - desired) as u32);
	}
	(2, (desired - maximum_weight) as u32)
}

//==============================================================================
/// 글꼴 모음(TTC)에서 청한 굵기와 기울기에 가장 맞는 얼굴 번호. (모음이 아니면 0)
/// - 굵기는 OS/2 의 usWeightClass, 기울기는 OS/2 와 head 의 기울임 표시로 본다. 기울기를 먼저, 그다음 굵기를 select_font_faces 와 같은 차례로 고른다.
/// - PostScript 이름이 점으로 시작하는 얼굴(맥 시스템 글꼴이 안에서만 쓰는 얼굴)은 다른 얼굴이 없을 때만 고른다. 점수가 같으면 앞 번호.
///
/// # 인자
/// - `font_bytes`: 글꼴 바이트. (모음이나 글꼴 하나)
/// - `desired_weight`: 청한 굵기.
/// - `is_italic_desired`: 기울임을 청하는지.
///
/// # 돌려줌
/// 얼굴 번호. (읽을 수 있는 얼굴이 없으면 0)
//==============================================================================
pub fn find_collection_face_index(font_bytes: &[u8], desired_weight: u16, is_italic_desired: bool) -> u32 {
	let face_count = rustybuzz::ttf_parser::fonts_in_collection(font_bytes).unwrap_or(1);
	let mut best_face: Option<((bool, bool, (u32, u32)), u32)> = None;
	for face_index in 0..face_count {
		let face = match rustybuzz::ttf_parser::Face::parse(font_bytes, face_index) {
			Ok(face) => {
				face
			}
			Err(_) => {
				continue;
			}
		};
		let is_hidden = face.names().into_iter().any(|name| name.name_id == rustybuzz::ttf_parser::name_id::POST_SCRIPT_NAME && name.to_string().map(|text| text.starts_with('.')).unwrap_or(false));
		let weight = face.weight().to_number();
		let score = (is_hidden, face.is_italic() != is_italic_desired, calculate_weight_score(weight, weight, desired_weight));
		let is_better = match &best_face {
			Some((best_score, _)) => {
				score < *best_score
			}
			None => {
				true
			}
		};
		if is_better {
			best_face = Some((score, face_index));
		}
	}
	best_face.map(|(_, face_index)| face_index).unwrap_or(0)
}

//==============================================================================
/// 가변 글꼴의 굵기 축 값. (wght 축이 있고 청한 굵기가 기본값과 다르면 축 범위로 자른 값 하나, 아니면 빈 목록)
///
/// # 인자
/// - `font_bytes`: 글꼴 바이트.
/// - `face_index`: 모음 안의 얼굴 번호. (모음이 아니면 0)
/// - `desired_weight`: 청한 굵기.
///
/// # 돌려줌
/// wght 축 값 하나의 목록. 축이 없거나, 자른 값이 기본값과 0.5 안으로 같거나, 글꼴을 읽지 못하면 빈 목록.
//==============================================================================
pub fn read_weight_variation(font_bytes: &[u8], face_index: u32, desired_weight: u16) -> Vec<rustybuzz::Variation> {
	let face = match rustybuzz::ttf_parser::Face::parse(font_bytes, face_index) {
		Ok(face) => {
			face
		}
		Err(_) => {
			return Vec::new();
		}
	};
	let weight_tag = rustybuzz::ttf_parser::Tag::from_bytes(b"wght");
	for variation_axis in face.variation_axes() {
		if variation_axis.tag != weight_tag {
			continue;
		}
		let value = (desired_weight as f32).clamp(variation_axis.min_value, variation_axis.max_value);
		if (value - variation_axis.def_value).abs() < 0.5 {
			return Vec::new();
		}
		return vec![rustybuzz::Variation {
			tag: weight_tag,
			value,
		}];
	}
	Vec::new()
}

//==============================================================================
/// 가변 글꼴의 굵기 범위. (wght 축의 처음과 끝, 축이 없으면 None)
///
/// # 인자
/// - `font_bytes`: 글꼴 바이트.
/// - `face_index`: 모음 안의 얼굴 번호. (모음이 아니면 0)
///
/// # 돌려줌
/// (처음, 끝) 굵기. (반올림) 축이 없거나 글꼴을 읽지 못하면 None.
//==============================================================================
pub fn read_weight_axis_range(font_bytes: &[u8], face_index: u32) -> Option<(u16, u16)> {
	let face = rustybuzz::ttf_parser::Face::parse(font_bytes, face_index).ok()?;
	let weight_tag = rustybuzz::ttf_parser::Tag::from_bytes(b"wght");
	face.variation_axes().into_iter().find(|variation_axis| variation_axis.tag == weight_tag).map(|variation_axis| (variation_axis.min_value.round() as u16, variation_axis.max_value.round() as u16))
}

//==============================================================================
/// 한 이름 아래의 얼굴들에서 청한 굵기와 기울기에 맞는 얼굴들 고르기. (CSS Fonts 4 의 글꼴 고르기 차례)
/// - 기울기를 먼저 고른다. 기울임을 청하면 기울인 얼굴, 없으면 바른 얼굴(가짜 기울임). 바르게를 청하면 바른 얼굴, 없으면 기울인 얼굴.
/// - 굵기: 청한 값이 400 ~ 500 이면 그 값부터 500 까지 오름, 그 아래 내림, 500 위 오름. 400 아래면 아래 내림, 위 오름. 500 위면 위 오름, 아래 내림.
/// - 고른 굵기와 기울기의 얼굴을 모두 나중 것부터 돌려준다. (글자 범위로 나눠 그린다)
///
/// # 인자
/// - `face_list`: 한 이름 아래의 얼굴들. (비었으면 아무것도 더하지 않는다)
/// - `desired_weight`: 청한 굵기.
/// - `is_italic_desired`: 기울임을 청하는지.
/// - `output_list`: 고른 얼굴을 뒤에 더할 목록. (있던 것은 그대로 둔다)
///
/// # 예제
/// 바른 보통 얼굴 하나에 굵은 기울임을 청하면 가짜로 입힌다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{select_font_faces, LoadedFont, RegisteredFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::FontFaceDescriptors;
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let regular_face = RegisteredFontFace {
/// 		descriptors: FontFaceDescriptors::new(),
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		order_number: 1,
/// 	};
/// 	let mut resolved_face_list = Vec::new();
/// 	select_font_faces(&[&regular_face], 700, true, &mut resolved_face_list);
/// 	assert!(resolved_face_list[0].is_synthetic_bold);
/// 	assert!(resolved_face_list[0].is_synthetic_italic);
/// }
/// ```
//==============================================================================
pub fn select_font_faces(face_list: &[&RegisteredFontFace], desired_weight: u16, is_italic_desired: bool, output_list: &mut Vec<ResolvedFontFace>) {
	if face_list.is_empty() {
		return;
	}
	let has_desired_style = face_list.iter().any(|face| face.descriptors.is_italic == is_italic_desired);
	let style_face_list: Vec<&&RegisteredFontFace> = face_list.iter().filter(|face| !has_desired_style || face.descriptors.is_italic == is_italic_desired).collect();
	let calculate_weight_score = |face: &RegisteredFontFace| -> (u32, u32) {
		calculate_weight_score(face.descriptors.minimum_weight, face.descriptors.maximum_weight, desired_weight)
	};
	let best_score = match style_face_list.iter().map(|face| calculate_weight_score(face)).min() {
		Some(best_score) => {
			best_score
		}
		None => {
			return;
		}
	};
	let mut selected_list: Vec<&&RegisteredFontFace> = style_face_list.into_iter().filter(|face| calculate_weight_score(face) == best_score).collect();
	selected_list.sort_by(|first_face, second_face| second_face.order_number.cmp(&first_face.order_number));
	for face in selected_list {
		output_list.push(ResolvedFontFace {
			font: face.font.clone(),
			descriptors: face.descriptors.clone(),
			is_synthetic_bold: desired_weight >= BOLD_WEIGHT_THRESHOLD && face.descriptors.maximum_weight < BOLD_WEIGHT_THRESHOLD,
			is_synthetic_italic: is_italic_desired && !face.descriptors.is_italic,
		});
	}
}

//==============================================================================
/// 모양 지은 글리프 하나. (길이 단위는 픽셀, y 는 위가 양수)
///
/// # 예제
/// 글리프는 왼쪽부터 놓인다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{shape_text, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::{FontFaceDescriptors, ShapingDirection};
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let face_list = vec![ResolvedFontFace {
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	}];
/// 	let shaped_text = shape_text(&face_list, "ab", 20.0, ShapingDirection::LeftToRight, None);
/// 	let first_glyph = shaped_text.glyph_list[0];
/// 	let second_glyph = shaped_text.glyph_list[1];
/// 	assert_eq!(first_glyph.face_index, 0);
/// 	assert!(second_glyph.position_x > first_glyph.position_x);
/// 	assert!(!second_glyph.is_right_to_left);
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
	/// 얼굴 목록 안의 자리. (SYSTEM_FACE_INDEX 면 브라우저 시스템 글꼴 조각)
	pub face_index: usize,
	/// 글리프 번호. (시스템 글꼴 조각이면 0)
	pub glyph_index: u16,
	/// 글리프 원점 x. (글 왼쪽 끝에서, 모양 짓기의 x 오프셋 포함)
	pub position_x: f32,
	/// 글리프 원점 y. (기준선에서, 모양 짓기의 y 오프셋)
	pub position_y: f32,
	/// 시스템 글꼴 조각이면 그 조각의 글 자리. (바이트, face_index 가 SYSTEM_FACE_INDEX 일 때만)
	pub text_start: usize,
	/// 시스템 글꼴 조각의 글 끝 자리. (바이트, 끝은 빼고. 조각이 아니면 0)
	pub text_end: usize,
	/// 든 줄기(run)가 오른쪽에서 왼쪽인지.
	pub is_right_to_left: bool,
}

//==============================================================================
/// 모양 지은 글. (글리프는 왼쪽에서 오른쪽 그리는 차례)
///
/// # 예제
/// 글 전체 전진폭
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{shape_text, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::{FontFaceDescriptors, ShapingDirection};
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let face_list = vec![ResolvedFontFace {
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	}];
/// 	let shaped_text = shape_text(&face_list, "abc", 20.0, ShapingDirection::LeftToRight, None);
/// 	assert_eq!(shaped_text.glyph_list.len(), 3);
/// 	let last_glyph = shaped_text.glyph_list[2];
/// 	assert!(shaped_text.advance_width > last_glyph.position_x);
/// }
/// ```
//==============================================================================
pub struct ShapedText {
	/// 글리프 목록. (왼쪽부터 그리는 차례)
	pub glyph_list: Vec<ShapedGlyph>,
	/// 글 전체 전진폭. (픽셀)
	pub advance_width: f32,
}

//==============================================================================
/// 글꼴이 글자를 그릴 수 있는지. (글자가 있거나, 없으면 정준 분해한 글자들이 있으면. HarfBuzz 정규화의 decompose 와 같은 셈)
/// - 크롬은 첫 글꼴로 HarfBuzz 모양을 짓고 .notdef 가 남은 글자만 대체로 넘긴다. HarfBuzz 는 글꼴에 합친 글자(ï)가 없고 분해한 글자(i, U+0308)가 있으면 분해해 그린다.
///   (맥 Apple SD Gothic Neo 에 ï 가 없어 크롬은 i 와 결합 분음 부호로 그리고, 대체 글꼴로 넘기지 않는다)
///
/// # 인자
/// - `font`: 볼 글꼴.
/// - `character`: 볼 글자.
///
/// # 돌려줌
/// 글자의 글리프가 있거나, 분해의 앞 글자를 (거듭) 그릴 수 있고 뒤 글자의 글리프가 있으면 true.
//==============================================================================
pub(crate) fn has_character_or_decomposition(font: &LoadedFont, character: char) -> bool {
	if font.has_character(character) {
		return true;
	}
	match rustybuzz::decompose_character(character) {
		Some((first_character, second_character)) => {
			(second_character == '\0' || font.has_character(second_character)) && has_character_or_decomposition(font, first_character)
		}
		None => {
			false
		}
	}
}

//==============================================================================
/// 글자 묶음에 쓸 얼굴 고르기. (묶음의 글자를 모두 가진 첫 얼굴, 없으면 첫 글자를 가진 첫 얼굴, 그것도 없으면 0)
/// - 브라우저처럼 결합 부호까지 한 얼굴로 그린다. 이음 문자, 이형 선택자처럼 보이지 않는 글자는 따지지 않는다.
/// - 크롬 FontFallbackIterator 처럼 이름 목록의 얼굴(운영체제 대체 글꼴이 아닌 것)을 먼저 본다. 색인지는 따지지 않는다.
///   다만 이형 선택자가 붙은 묶음(❤️ 의 FE0F, FE0E)은 그 표시에 맞는 얼굴만 받는다. (크롬은 글꼴에 이형 글리프가 없으면 다음 글꼴로 넘어간다)
/// - 운영체제 대체 글꼴의 얼굴(FontFaceDescriptors 의 is_system_fallback)에서는 이모지 표시 묶음은 첫 글자를 색 글리프로 가진 얼굴을,
///   글 표시 묶음은 색이 아닌 글리프로 가진 얼굴을 같은 조건 안에서 먼저 고른다. (is_emoji_presentation, 크롬의 이모지 우선 글꼴 단계)
///   차례: 이름 목록에서 글자를 모두 가진 얼굴 → 대체에서 글자를 모두 가지고 색이 맞는 얼굴 → 대체에서 글자를 모두 가진 얼굴
///   → 이름 목록에서 첫 글자를 가진 얼굴 → 대체에서 첫 글자를 가지고 색이 맞는 얼굴 → 대체에서 첫 글자를 가진 얼굴.
//==============================================================================
fn select_face_index(face_list: &[ResolvedFontFace], grapheme: &str) -> Option<usize> {
	let base_character = grapheme.chars().next()?;
	let has_character = |face: &ResolvedFontFace, character: char| -> bool {
		face.descriptors.contains_character(character) && has_character_or_decomposition(&face.font, character)
	};
	let is_ignorable = |character: char| -> bool {
		('\u{200B}'..='\u{200F}').contains(&character) || ('\u{FE00}'..='\u{FE0F}').contains(&character) || ('\u{E0100}'..='\u{E01EF}').contains(&character) || ('\u{2060}'..='\u{206F}').contains(&character)
	};
	let is_color_wanted = is_emoji_presentation(grapheme);
	let is_color_matched = |face: &ResolvedFontFace| -> bool {
		face.font.has_color_character(base_character) == is_color_wanted
	};
	let has_presentation_selector = grapheme.contains('\u{FE0F}') || grapheme.contains('\u{FE0E}');
	let is_list_face_accepted = |face: &ResolvedFontFace| -> bool {
		!face.descriptors.is_system_fallback && (!has_presentation_selector || is_color_matched(face))
	};
	let has_all_characters = |face: &ResolvedFontFace| -> bool {
		grapheme.chars().filter(|character| !is_ignorable(*character)).all(|character| has_character(face, character))
	};
	let is_multiple_character = grapheme.chars().nth(1).is_some();
	if is_multiple_character {
		for (face_index, face) in face_list.iter().enumerate() {
			if is_list_face_accepted(face) && has_all_characters(face) {
				return Some(face_index);
			}
		}
		for (face_index, face) in face_list.iter().enumerate() {
			if face.descriptors.is_system_fallback && has_all_characters(face) && is_color_matched(face) {
				return Some(face_index);
			}
		}
		for (face_index, face) in face_list.iter().enumerate() {
			if face.descriptors.is_system_fallback && has_all_characters(face) {
				return Some(face_index);
			}
		}
	}
	for (face_index, face) in face_list.iter().enumerate() {
		if is_list_face_accepted(face) && has_character(face, base_character) {
			return Some(face_index);
		}
	}
	for (face_index, face) in face_list.iter().enumerate() {
		if face.descriptors.is_system_fallback && has_character(face, base_character) && is_color_matched(face) {
			return Some(face_index);
		}
	}
	for (face_index, face) in face_list.iter().enumerate() {
		if has_character(face, base_character) {
			return Some(face_index);
		}
	}
	// 공백과 보이지 않는 글자는 첫 얼굴로. (시스템 글꼴로 따로 굽지 않는다)
	if base_character.is_whitespace() || is_ignorable(base_character) || base_character.is_control() {
		return Some(0);
	}
	None
}

/// 크롬 IsCjkIdeographOrSymbol 의 범위. (platform/text/character_property_data.h 의 kIsCjkIdeographOrSymbolRanges, 처음과 끝)
const CJK_IDEOGRAPH_OR_SYMBOL_RANGE_LIST: [(u32, u32); 67] = [
	(0x2E80, 0x2FDF), (0x31C0, 0x31EF), (0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xF900, 0xFAFF), (0x20000, 0x2FFFF),
	(0x2156, 0x215A), (0x2160, 0x216B), (0x2170, 0x217B), (0x23BE, 0x23CC), (0x2460, 0x2492), (0x249C, 0x24FF), (0x25CE, 0x25D3), (0x25E2, 0x25E6),
	(0x2600, 0x2603), (0x2660, 0x266F), (0x2672, 0x267D), (0x2776, 0x277F), (0x2FF0, 0x302D), (0x3031, 0x312F), (0x3190, 0x31BF), (0x3200, 0x33FF),
	(0x4DC0, 0x4DFF), (0xF860, 0xF862), (0xFE30, 0xFE6F), (0xFF00, 0xFF0C), (0xFF0E, 0xFF1A), (0xFF1F, 0xFFEF), (0x16FE0, 0x16FFF), (0x17000, 0x187FF),
	(0x18800, 0x18AFF), (0x1B000, 0x1B0FF), (0x1B100, 0x1B12F), (0x1B170, 0x1B2FF),
	(0x1F110, 0x1F129), (0x1F130, 0x1F149), (0x1F150, 0x1F169), (0x1F170, 0x1F189), (0x1F202, 0x1F219), (0x1F21B, 0x1F22E), (0x1F230, 0x1F231), (0x1F23B, 0x1F24F),
	(0x1F252, 0x1F2FF), (0x1F321, 0x1F32A), (0x1F394, 0x1F39F), (0x1F3CD, 0x1F3CE), (0x1F3D4, 0x1F3DF), (0x1F3F1, 0x1F3F2), (0x1F3F5, 0x1F3F7), (0x1F4FD, 0x1F4FE),
	(0x1F53E, 0x1F54A), (0x1F568, 0x1F573), (0x1F576, 0x1F579), (0x1F57B, 0x1F58F), (0x1F591, 0x1F594), (0x1F597, 0x1F5A3), (0x1F5A5, 0x1F5E7), (0x1F5E9, 0x1F5FA),
	(0x1F650, 0x1F67F), (0x1F6C6, 0x1F6CB), (0x1F6CD, 0x1F6CF), (0x1F6D3, 0x1F6D4), (0x1F6D9, 0x1F6DB), (0x1F6E0, 0x1F6EA), (0x1F6ED, 0x1F6F3), (0x1F6FD, 0x1F6FF),
	(0x1F900, 0x1F90B),
];
/// 크롬 IsCjkIdeographOrSymbol 의 낱글자. (kIsCjkIdeographOrSymbolArray 와 범위 (0x1FAC9, 0x1FACC))
const CJK_IDEOGRAPH_OR_SYMBOL_LIST: [u32; 85] = [
	0x2C7, 0x2CA, 0x2CB, 0x2D9, 0x2020, 0x2021, 0x2030, 0x203B, 0x203C, 0x2042, 0x2047, 0x2048, 0x2049, 0x2051, 0x20DD, 0x20DE, 0x2100, 0x2103, 0x2105, 0x2109,
	0x210A, 0x2113, 0x2116, 0x2121, 0x212B, 0x213B, 0x2150, 0x2151, 0x2152, 0x217F, 0x2189, 0x2307, 0x2312, 0x23CE, 0x2423, 0x25A0, 0x25A1, 0x25A2, 0x25AA, 0x25AB,
	0x25B1, 0x25B2, 0x25B3, 0x25B6, 0x25B7, 0x25BC, 0x25BD, 0x25C0, 0x25C1, 0x25C6, 0x25C7, 0x25C9, 0x25CB, 0x25CC, 0x25EF, 0x2605, 0x2606, 0x260E, 0x2616, 0x2617,
	0x26A0, 0x2713, 0x271A, 0x273F, 0x2740, 0x2756, 0x2763, 0x2B1A, 0xFE10, 0xFE11, 0xFE12, 0xFE19, 0xFF1D, 0x1F100, 0x1F200, 0x1F237, 0x1F32C, 0x1F336, 0x1F37D, 0x1F43F,
	0x1F54F, 0x1F93B, 0x1F946, 0x1FAC9, 0x1FACA,
];
/// 이모지 ZWJ 묶음에 드는 글 표시 그림 문자. (크롬은 RGI 이모지 ZWJ, 수식 묶음의 Extended_Pictographic 을 한중일 기호로 본다. Emoji_Presentation 과 Emoji_Modifier_Base 밖의 것)
const EMOJI_SEQUENCE_PICTOGRAPH_LIST: [u32; 15] = [0x2620, 0x2640, 0x2642, 0x2695, 0x2696, 0x26A7, 0x2708, 0x2744, 0x2764, 0x27A1, 0x1F32B, 0x1F3F3, 0x1F441, 0x1F5E8, 0x1FACB];

//==============================================================================
/// 한중일 글자나 기호인지. (크롬 unicode::IsCjkIdeographOrSymbol: 위 표, Emoji_Presentation 글자, 이모지 묶음에 드는 그림 문자)
/// - 크롬은 RGI 이모지 묶음에서 그림 문자를 뽑는다. 여기서는 Emoji_Modifier_Base 와 ZWJ 묶음의 글 표시 그림 문자 표로 갈음한다. (짐작에 가까운 몫이다)
//==============================================================================
fn is_cjk_ideograph_or_symbol(character: char) -> bool {
	let code_point = character as u32;
	if code_point < 0x2C7 {
		return false;
	}
	if CJK_IDEOGRAPH_OR_SYMBOL_RANGE_LIST.iter().any(|(first, last)| (*first..=*last).contains(&code_point)) || CJK_IDEOGRAPH_OR_SYMBOL_LIST.contains(&code_point) || code_point == 0x1FACB || code_point == 0x1FACC {
		return true;
	}
	let is_emoji_listed = matches!(character.emoji_status(), EmojiStatus::EmojiPresentation | EmojiStatus::EmojiPresentationAndModifierBase | EmojiStatus::EmojiPresentationAndEmojiComponent | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent | EmojiStatus::EmojiModifierBase);
	is_emoji_listed || EMOJI_SEQUENCE_PICTOGRAPH_LIST.contains(&code_point)
}

//==============================================================================
/// 결합 부호, 수식 글자, 수식 기호인지. (일반 범주 M, Lm, Sk)
//==============================================================================
fn is_mark_or_modifier(character: char) -> bool {
	matches!(character.general_category(), GeneralCategory::NonspacingMark | GeneralCategory::SpacingMark | GeneralCategory::EnclosingMark | GeneralCategory::ModifierLetter | GeneralCategory::ModifierSymbol)
}

//==============================================================================
/// 크롬 캔버스의 낱말 끝. (platform/fonts/plain_text_node.cc 의 NextWordEndIndex. text 의 바이트 자리 start 에서 시작하는 낱말의 끝 바이트 자리)
/// - 빈칸(U+0020), 탭, ZWSP 는 하나씩 따로 낱말이다. is_latin1 (크롬의 8 비트 글, 모든 글자가 U+00FF 이하)이면 빈칸과 탭에서만 끊는다.
/// - 그 밖: 한중일 글자나 기호가 아닌 글자로 시작하면 빈칸이나 한중일 글자(결합 부호 따위가 아닌 것) 앞에서 끊는다.
///   한중일이나 이모지로 시작하면 글자마다 끊되 결합 부호, 수식 글자, ZWJ, 이모지 성분, 그림 문자는 붙이고, 문자가 Common, Inherited 인 한중일 기호와
///   첫 글자가 Common 일 때 다음 한 글자도 붙인다.
///
/// # 인자
/// - `text`: 글.
/// - `start`: 낱말이 시작하는 바이트 자리. (글자 경계여야 한다, 아니면 패닉)
/// - `is_latin1`: 글의 모든 글자가 U+00FF 이하인지.
///
/// # 돌려줌
/// 낱말 끝 바이트 자리. (끝은 빼고) start 가 글 끝이면 start.
///
/// # 예제
/// 빈칸은 따로, 한자는 글자마다
/// ```rust
/// use vanilla::core::textshaping::find_chrome_word_end;
///
/// let text = "ab cd";
/// assert_eq!(find_chrome_word_end(text, 0, true), 2);
/// assert_eq!(find_chrome_word_end(text, 2, true), 3);
/// assert_eq!(find_chrome_word_end(text, 3, true), 5);
/// let ideograph_text = "\u{4E00}\u{4E8C}";
/// assert_eq!(find_chrome_word_end(ideograph_text, 0, false), 3);
/// assert_eq!(find_chrome_word_end(ideograph_text, 3, false), 6);
/// ```
//==============================================================================
pub fn find_chrome_word_end(text: &str, start: usize, is_latin1: bool) -> usize {
	let is_word_delimiter = |character: char, is_zero_width_space_delimiter: bool| -> bool {
		character == ' ' || character == '\t' || (is_zero_width_space_delimiter && character == '\u{200B}')
	};
	let mut character_iterator = text[start..].char_indices().map(|(byte_index, character)| (start + byte_index, character));
	let (_, first_character) = match character_iterator.next() {
		Some(first_entry) => {
			first_entry
		}
		None => {
			return start;
		}
	};
	let first_end = start + first_character.len_utf8();
	if first_end == text.len() || is_word_delimiter(first_character, true) {
		return first_end;
	}
	if is_latin1 {
		for (byte_index, character) in character_iterator {
			if is_word_delimiter(character, false) {
				return byte_index;
			}
		}
		return text.len();
	}
	if !is_cjk_ideograph_or_symbol(first_character) {
		for (byte_index, character) in character_iterator {
			if is_word_delimiter(character, true) || (is_cjk_ideograph_or_symbol(character) && !is_mark_or_modifier(character)) {
				return byte_index;
			}
		}
		return text.len();
	}
	let is_common_or_inherited = |character: char| -> bool {
		matches!(character.script(), Script::Common | Script::Inherited)
	};
	let mut has_any_script = !is_common_or_inherited(first_character);
	for (byte_index, character) in character_iterator {
		let is_attached = is_mark_or_modifier(character) || character == '\u{200D}' || character.is_emoji_component() || (character.is_emoji_char() && !character.is_ascii());
		if is_attached {
			continue;
		}
		if is_cjk_ideograph_or_symbol(character) {
			if is_common_or_inherited(character) {
				continue;
			}
			if !has_any_script {
				has_any_script = true;
				continue;
			}
		}
		return byte_index;
	}
	text.len()
}

//==============================================================================
/// 글 모양 짓기. (양방향 정렬 → 얼굴마다 나눈 조각을 rustybuzz 로 모양 짓기 → 왼쪽부터 놓기)
/// - direction 은 문단의 방향이다. (원본 canvas.direction) 문단 안의 글자 순서는 양방향 알고리즘이 정한다.
/// - 줄바꿈 문자는 한 줄로 보고 그대로 모양 짓는다. (원본 fillText 도 줄을 바꾸지 않는다)
/// - system_font_measure 를 주면(웹) 올린 어느 얼굴에도 없는 글자 묶음은 브라우저 시스템 글꼴 조각(SYSTEM_FACE_INDEX)으로 둔다. 원본 브라우저의 글꼴 대체와 같다.
///   이모지 표시 묶음인데 올린 얼굴에 색 글리프가 없으면 흑백 글리프가 있어도 브라우저 조각(색 이모지)으로 둔다. (크롬도 ❤️ 는 색 이모지 글꼴로 그린다)
///   주지 않으면 첫 얼굴의 빈 글리프로 그린다.
///
/// # 인자
/// - `face_list`: 고른 얼굴들. (글자 묶음마다 앞 얼굴부터 찾는다)
/// - `text`: 모양 지을 글.
/// - `pixel_size`: 글자 크기. (픽셀)
/// - `direction`: 문단 방향.
/// - `system_font_measure`: 시스템 글꼴 조각 재기. (웹, None 이면 조각을 두지 않는다)
///
/// # 돌려줌
/// 모양 지은 글. 얼굴이 없거나 글이 비었으면 글리프 없이 전진폭 0.
///
/// # 예제
/// 크기가 커지면 전진폭도 커진다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{shape_text, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::{FontFaceDescriptors, ShapingDirection};
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let face_list = vec![ResolvedFontFace {
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	}];
/// 	let small_text = shape_text(&face_list, "Hello", 16.0, ShapingDirection::LeftToRight, None);
/// 	let large_text = shape_text(&face_list, "Hello", 32.0, ShapingDirection::LeftToRight, None);
/// 	assert!(large_text.advance_width > small_text.advance_width);
/// 	let empty_text = shape_text(&face_list, "", 16.0, ShapingDirection::LeftToRight, None);
/// 	assert!(empty_text.glyph_list.is_empty());
/// 	assert_eq!(empty_text.advance_width, 0.0);
/// }
/// ```
//==============================================================================
pub fn shape_text(face_list: &[ResolvedFontFace], text: &str, pixel_size: f32, direction: ShapingDirection, system_font_measure: Option<SystemFontMeasure>) -> ShapedText {
	let mut shaped_text = ShapedText {
		glyph_list: Vec::new(),
		advance_width: 0.0,
	};
	if face_list.is_empty() || text.is_empty() {
		return shaped_text;
	}
	let paragraph_level = match direction {
		ShapingDirection::LeftToRight => {
			Level::ltr()
		}
		ShapingDirection::RightToLeft => {
			Level::rtl()
		}
	};
	// 글자 묶음마다 얼굴을 정한다. (바이트 시작 자리, 얼굴)
	let grapheme_face_list: Vec<(usize, usize)> = text.grapheme_indices(true).map(|(byte_index, grapheme)| {
		let face_index = match select_face_index(face_list, grapheme) {
			Some(face_index) => {
				let is_color_missing = system_font_measure.is_some() && is_emoji_presentation(grapheme) && grapheme.chars().next().is_some_and(|base_character| !face_list[face_index].font.has_color_character(base_character));
				if is_color_missing {
					SYSTEM_FACE_INDEX
				}
				else {
					face_index
				}
			}
			None => {
				if system_font_measure.is_some() {
					SYSTEM_FACE_INDEX
				}
				else {
					0
				}
			}
		};
		(byte_index, face_index)
	}).collect();
	let find_face_at = |byte_index: usize| -> usize {
		match grapheme_face_list.binary_search_by(|(start_index, _)| start_index.cmp(&byte_index)) {
			Ok(found_index) => {
				grapheme_face_list[found_index].1
			}
			Err(insert_index) => {
				grapheme_face_list[insert_index.saturating_sub(1)].1
			}
		}
	};

	// 캔버스의 빈칸 글자(탭, 줄바꿈, 세로 탭, 폼 피드, 캐리지 리턴)는 빈칸으로 바꿔 모양 짓는다. (크롬 IsNormalizedCanvasSpaceCharacter, 바이트 길이는 같다)
	let shaping_text: String = text.chars().map(|character| {
		if matches!(character, '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r') {
			' '
		}
		else {
			character
		}
	}).collect();
	let is_latin1 = shaping_text.chars().all(|character| (character as u32) <= 0xFF);
	let bidi_info = BidiInfo::new(text, Some(paragraph_level));
	let mut pen_x: f32 = 0.0;
	for paragraph in &bidi_info.paragraphs {
		let (level_list, run_list) = bidi_info.visual_runs(paragraph, paragraph.range.clone());
		for run in run_list {
			let level = level_list[run.start];
			let is_right_to_left_run = level.is_rtl();
			// 크롬처럼 낱말의 시작 자리. (조각을 낱말마다 따로 모양 짓는다)
			let mut word_start_list: Vec<usize> = Vec::new();
			let mut word_start = run.start;
			while word_start < run.end {
				word_start_list.push(word_start);
				let word_end = run.start + find_chrome_word_end(&shaping_text[run.start..run.end], word_start - run.start, is_latin1);
				if word_end <= word_start {
					break;
				}
				word_start = word_end;
			}
			// 얼굴이 같은 글자 묶음끼리, 낱말마다 조각으로 나눈다. (논리 차례)
			let mut piece_list: Vec<(usize, usize, usize)> = Vec::new();
			for (start_index, face_index) in grapheme_face_list.iter().copied() {
				if start_index < run.start || start_index >= run.end {
					continue;
				}
				let is_word_start = word_start_list.binary_search(&start_index).is_ok();
				match piece_list.last_mut() {
					Some(last_piece) if last_piece.2 == face_index && !is_word_start => {}
					_ => {
						piece_list.push((start_index, run.end, face_index));
					}
				}
			}
			if piece_list.is_empty() {
				// 묶음 시작이 이 조각 밖에 있으면(묶음 가운데에서 방향이 바뀐 경우) 조각 전체를 그 묶음의 얼굴로.
				piece_list.push((run.start, run.end, find_face_at(run.start)));
			}
			for piece_index in 0..piece_list.len() {
				let next_start = if piece_index + 1 < piece_list.len() {
					piece_list[piece_index + 1].0
				}
				else {
					run.end
				};
				piece_list[piece_index].1 = next_start;
			}
			// 오른쪽에서 왼쪽 조각은 논리 차례의 뒤 조각이 왼쪽에 온다.
			if is_right_to_left_run {
				piece_list.reverse();
			}
			for (piece_start, piece_end, face_index) in piece_list {
				if face_index == SYSTEM_FACE_INDEX {
					let piece_width = match system_font_measure {
						Some(system_font_measure) => {
							system_font_measure(&text[piece_start..piece_end], is_right_to_left_run)
						}
						None => {
							0.0
						}
					};
					shaped_text.glyph_list.push(ShapedGlyph {
						face_index,
						glyph_index: 0,
						position_x: pen_x,
						position_y: 0.0,
						text_start: piece_start,
						text_end: piece_end,
						is_right_to_left: is_right_to_left_run,
					});
					pen_x += piece_width;
					continue;
				}
				let face = &face_list[face_index];
				let mut unicode_buffer = rustybuzz::UnicodeBuffer::new();
				unicode_buffer.push_str(&shaping_text[piece_start..piece_end]);
				unicode_buffer.set_direction(if is_right_to_left_run {
					rustybuzz::Direction::RightToLeft
				}
				else {
					rustybuzz::Direction::LeftToRight
				});
				// 크롬은 8 비트 글(모든 글자가 U+00FF 이하)을 문자 나누기 없이 라틴 문자로 모양 짓는다. (harfbuzz_shaper.cc 의 HarfBuzzShaper::Shape, "8-bit text is guaranteed to be horizontal latin-1")
				// 숫자처럼 Common 뿐인 낱말도 GSUB, GPOS 의 latn 을 쓴다. (리눅스 Norasi 는 DFLT 에만 숫자 커닝이 있어 16px "0123456789" 가 3.8px 좁았다)
				if is_latin1 {
					unicode_buffer.set_script(rustybuzz::script::LATIN);
				}
				unicode_buffer.guess_segment_properties();
				// 크롬 HarfBuzzFace::GetScaledFont 처럼 자리 배율을 글자 크기의 16.16 고정 소수로 건다. (SkiaScalarToHarfBuzzPosition)
				// 글꼴 단위 값은 HarfBuzz 의 em_scale 로, 가변 글꼴의 GPOS 델타는 em_scalef 로 이 배율에서 반올림된다.
				let mut sized_shaping_face = face.font.create_sized_shaping_face(pixel_size);
				let harfbuzz_scale = (pixel_size * HARFBUZZ_POSITION_ONE) as i32;
				sized_shaping_face.set_scale(harfbuzz_scale, harfbuzz_scale);
				let glyph_buffer = rustybuzz::shape(&sized_shaping_face, &[], unicode_buffer);
				let scale = face.font.read_advance_scale(pixel_size) / HARFBUZZ_POSITION_ONE;
				let is_integer_advance = face.font.is_integer_advance();
				let is_bitmap_only_font = face.font.is_bitmap_only_font();
				// trak 자간. (글자 묶음의 첫 글리프에 더한다, HarfBuzz 의 foreach_grapheme 자리를 묶음 번호(cluster)가 바뀌는 글리프로 본다. HarfBuzz 처럼 em_scalef_x 로 배율에서 반올림한다)
				let tracking = (face.font.read_horizontal_tracking(pixel_size) * (harfbuzz_scale as f32 / face.font.units_per_em)).round() * scale;
				let mut previous_cluster: Option<u32> = None;
				for (glyph_info, glyph_position) in glyph_buffer.glyph_infos().iter().zip(glyph_buffer.glyph_positions().iter()) {
					shaped_text.glyph_list.push(ShapedGlyph {
						face_index,
						glyph_index: glyph_info.glyph_id as u16,
						position_x: pen_x + glyph_position.x_offset as f32 * scale,
						position_y: glyph_position.y_offset as f32 * scale,
						text_start: 0,
						text_end: 0,
						is_right_to_left: is_right_to_left_run,
					});
					// 맥 색 이모지는 정수 픽셀로 반올림한다. (곱셈 오차로 0.5 가 조금 모자라 내려가지 않게 작은 값을 더한다)
					let mut advance = if is_integer_advance {
						(glyph_position.x_advance as f32 * scale + 0.001).round()
					}
					else {
						glyph_position.x_advance as f32 * scale
					};
					// 비트맵만 있는 글꼴은 판의 글리프 수치 전진폭에 모양 짓기가 더한 몫(GPOS)을 얹는다. (크롬의 HarfBuzz 는 스키아가 준 전진폭에서 시작한다)
					if is_bitmap_only_font {
						let shaping_face = face.font.get_shaping_face();
						let glyph_id = rustybuzz::ttf_parser::GlyphId(glyph_info.glyph_id as u16);
						if let (Some(bitmap_advance), Some(font_advance)) = (colorglyph::read_bitmap_glyph_advance(shaping_face, glyph_id.0, pixel_size), shaping_face.glyph_hor_advance(glyph_id)) {
							// hmtx 전진폭을 모양 짓기와 같은 16.16 배율로 옮겨(HarfBuzz em_scale) 모양 짓기가 더한 몫만 남긴다.
							let scaled_font_advance = (font_advance as f32 * harfbuzz_scale as f32 / face.font.units_per_em).round() as i32;
							advance = bitmap_advance + (glyph_position.x_advance - scaled_font_advance) as f32 * scale;
						}
					}
					if previous_cluster != Some(glyph_info.cluster) {
						advance += tracking;
						previous_cluster = Some(glyph_info.cluster);
					}
					pen_x += advance;
				}
			}
		}
	}
	shaped_text.advance_width = pen_x;
	shaped_text
}

//==============================================================================
/// 가짜 굵게의 넓히는 반지름. (픽셀, Skia 의 칠하는 두께의 반)
///
/// # 인자
/// - `pixel_size`: 글자 크기. (픽셀)
///
/// # 돌려줌
/// 반지름. (픽셀, 9px 이하는 크기의 1/48, 36px 이상은 1/64, 사이는 곧게 잇는다)
///
/// # 예제
/// 36px 이상은 크기의 1/64
/// ```rust
/// use vanilla::core::textshaping::calculate_synthetic_bold_radius;
///
/// assert_eq!(calculate_synthetic_bold_radius(48.0), 0.75);
/// assert!((calculate_synthetic_bold_radius(6.0) - 0.125).abs() < 0.00001);
/// ```
//==============================================================================
pub fn calculate_synthetic_bold_radius(pixel_size: f32) -> f32 {
	let ratio = if pixel_size <= SYNTHETIC_BOLD_SIZE_KEYS[0] {
		SYNTHETIC_BOLD_SIZE_RATIOS[0]
	}
	else if pixel_size >= SYNTHETIC_BOLD_SIZE_KEYS[1] {
		SYNTHETIC_BOLD_SIZE_RATIOS[1]
	}
	else {
		let interpolation = (pixel_size - SYNTHETIC_BOLD_SIZE_KEYS[0]) / (SYNTHETIC_BOLD_SIZE_KEYS[1] - SYNTHETIC_BOLD_SIZE_KEYS[0]);
		SYNTHETIC_BOLD_SIZE_RATIOS[0] + (SYNTHETIC_BOLD_SIZE_RATIOS[1] - SYNTHETIC_BOLD_SIZE_RATIOS[0]) * interpolation
	};
	pixel_size * ratio * 0.5
}

//==============================================================================
/// 글리프 래스터 하나. (덮인 정도, 왼쪽 위 자리는 펜 원점에서 오른쪽 xmin, 기준선에서 위로 top)
/// - coverage 는 색이 아닌 글자의 덮인 정도다. 색 글리프 몫은 color_pixels 에 제 색으로 든다. (프리멀티플라이드 RGBA, 같은 크기, 없으면 None)
///
/// # 예제
/// 글리프 하나 래스터
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{rasterize_glyph, shape_text, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::{FontFaceDescriptors, ShapingDirection};
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let face = ResolvedFontFace {
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	};
/// 	let shaped_text = shape_text(&[face.clone()], "A", 32.0, ShapingDirection::LeftToRight, None);
/// 	let glyph_raster = rasterize_glyph(&face, shaped_text.glyph_list[0].glyph_index, 32.0);
/// 	assert!(glyph_raster.width > 0 && glyph_raster.height > 0);
/// 	assert_eq!(glyph_raster.coverage.len(), (glyph_raster.width * glyph_raster.height) as usize);
/// 	assert!(glyph_raster.top_y > 0);
/// 	assert!(glyph_raster.color_pixels.is_none());
/// }
/// ```
//==============================================================================
pub struct GlyphRaster {
	/// 덮인 정도. (0 ~ 255, width * height 개, 위 줄부터)
	pub coverage: Vec<u8>,
	/// 너비. (픽셀)
	pub width: i32,
	/// 높이. (픽셀)
	pub height: i32,
	/// 펜 원점에서 래스터 왼쪽까지. (픽셀, 오른쪽이 양수)
	pub left_x: i32,
	/// 기준선에서 래스터 위까지. (픽셀, 위가 양수)
	pub top_y: i32,
	/// 색 글리프 층. (프리멀티플라이드 RGBA, width * height * 4 바이트, 색 글리프가 아니면 None)
	pub color_pixels: Option<Vec<u8>>,
}

//==============================================================================
/// 글리프 래스터. (가짜 굵게는 넓히고, 가짜 기울임은 기준선 기준으로 민다. 색 글리프는 제 색으로, 가짜 굵게 없이)
///
/// # 인자
/// - `face`: 그릴 얼굴. (가짜 굵게, 가짜 기울임을 입힐지 함께)
/// - `glyph_index`: 글리프 번호.
/// - `pixel_size`: 글자 크기. (픽셀)
///
/// # 돌려줌
/// 래스터. 윤곽이 없는 글리프(빈칸)는 너비와 높이가 0.
///
/// # 예제
/// 가짜 굵게는 래스터를 넓힌다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{rasterize_glyph, shape_text, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::{FontFaceDescriptors, ShapingDirection};
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let face = ResolvedFontFace {
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	};
/// 	let bold_face = ResolvedFontFace {
/// 		is_synthetic_bold: true,
/// 		..face.clone()
/// 	};
/// 	let glyph_index = shape_text(&[face.clone()], "H", 32.0, ShapingDirection::LeftToRight, None).glyph_list[0].glyph_index;
/// 	let glyph_raster = rasterize_glyph(&face, glyph_index, 32.0);
/// 	let bold_glyph_raster = rasterize_glyph(&bold_face, glyph_index, 32.0);
/// 	assert!(bold_glyph_raster.width > glyph_raster.width);
/// 	assert!(bold_glyph_raster.left_x < glyph_raster.left_x);
/// }
/// ```
//==============================================================================
pub fn rasterize_glyph(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32) -> GlyphRaster {
	rasterize_glyph_at(face, glyph_index, pixel_size, 0.0, [255, 255, 255])
}

//==============================================================================
/// 글리프 자리를 1/4 픽셀 서브픽셀로 나눌지. (크롬 블링크가 SkFont::setSubpixel 에 주는 값)
/// - 맥, 윈도우: 늘 켠다. (안티에일리어스 글)
/// - 리눅스: fontconfig 의 힌팅이 꽉(hintfull)이면 끄고 그 밖은 켠다. (WebFontRenderStyle::ApplyToSkFont 의 force_subpixel_positioning 은 힌팅이 kFull 이 아닐 때 참이고,
///   use_subpixel_positioning 은 기기 배율이 1 보다 클 때만 참이다. 안티에일리어스가 꺼진 글꼴은 크롬 GetFontRenderParams 가 힌팅을 꽉으로 바꾼다)
///   얼굴의 가족 이름, 크기, 굵기, 기울기로 묻는다. (rasterize_glyph_at 의 힌팅과 같은 물음)
/// - 그 밖(웹): 끈다. (브라우저 캔버스가 정한다)
///
/// # 인자
/// - `face`: 그릴 얼굴.
/// - `pixel_size`: 글자 크기. (픽셀)
///
/// # 돌려줌
/// 켜면 true. 끄면 글리프 자리를 반올림한 정수 자리에 두고 서브픽셀 몫은 0 이다.
///
/// # 예제
/// 맥과 윈도우는 늘 켠다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{is_subpixel_positioned, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::FontFaceDescriptors;
///
/// if let Some(source) = find_fallback_font_file("A", 400, false, false) {
/// 	let face = ResolvedFontFace {
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	};
/// 	let is_subpixel = is_subpixel_positioned(&face, 16.0);
/// 	if cfg!(any(target_os = "macos", target_os = "windows")) {
/// 		assert!(is_subpixel);
/// 	}
/// }
/// ```
//==============================================================================
pub fn is_subpixel_positioned(face: &ResolvedFontFace, pixel_size: f32) -> bool {
	#[cfg(any(target_os = "macos", target_os = "windows"))]
	{
		let _ = (face, pixel_size);
		true
	}
	#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
	{
		let shaping_face = face.font.get_shaping_face();
		let (is_antialiased, hint_level, _) = crate::platform::desktop::systemfont::read_font_render_style(&read_family_name(shaping_face), pixel_size, shaping_face.weight().to_number(), shaping_face.is_italic());
		is_antialiased && hint_level != 3
	}
	#[cfg(not(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android")))))]
	{
		let _ = (face, pixel_size);
		false
	}
}

//==============================================================================
/// 서브픽셀 자리와 글자 색 밝기로 글리프 래스터. (맥은 CoreText, 윈도우는 DirectWrite, 리눅스는 skrifa 힌팅 윤곽으로 크롬처럼 그린다)
/// - 맥: 색이 아닌 글리프는 크롬 맥 Canvas2D 와 같은 길로 그린다. (스키아 SkScalerContext_Mac 의 A8 마스크: 글꼴 부드럽게, CoreGraphics 감마 풀기, 글자 색 밝기의 감마 전보정)
///   글자 색 밝기는 맥 스키아처럼 글꼴 부드럽게가 회색이면 1/2 을 곱한 색의 3 비트 단계이고, 감마는 크롬 맥의 sRGB, 대비는 0 이다. (SkTypeface_Mac::onFilterRec, 크로미엄 skia/BUILD.gn)
///   가짜 기울임은 CoreText 의 글자 행렬 기울기로 그린다. 색 글리프는 rasterize_glyph 와 같다.
/// - 윈도우: 색이 아닌 글리프는 크롬 윈도우 Canvas2D 와 같은 길로 DirectWrite 가 그린다. (스키아 SkScalerContext_DW 의 A8: 격자 맞춤, gasp 로 고른 렌더링 모드, ClearType 이 켜졌으면 크롬처럼 3x1 텍스처의 세 채널 평균, 꺼졌으면 그레이스케일)
///   감마 전보정은 글자 색 밝기 단계의 표로, 대비와 감마는 크롬처럼 Avalon.Graphics 키가 있으면 시스템 값, 없으면 대비 1, sRGB 다. 가짜 기울임은 DirectWrite 글자 행렬의 기울기다.
/// - 리눅스: 색이 아닌 글리프는 크롬 리눅스처럼 skrifa 윤곽에 fontconfig 의 힌팅 단계로 힌팅을 걸어 스키아와 같은 해석적 AA 로 칠하고 감마 1.2, 대비 0.2 로 전보정한다. (스키아 SkTypeface_Fontations, SkScan_AAAPath)
///   fontconfig 힌팅이 꽉(hintfull)인 글꼴은 서브픽셀 자리를 끈다. (is_subpixel_positioned, 부르는 쪽이 subpixel_x 를 0 으로 준다)
/// - 그 밖은 서브픽셀 자리와 밝기를 쓰지 않는다.
///
/// # 인자
/// - `face`: 그릴 얼굴. (가짜 굵게, 가짜 기울임을 입힐지 함께)
/// - `glyph_index`: 글리프 번호.
/// - `pixel_size`: 글자 크기. (픽셀)
/// - `subpixel_x`: 서브픽셀 가로 자리. (0, 0.25, 0.5, 0.75, 스키아처럼 글리프 자리에 1/8 을 더해 내린 정수 자리의 나머지를 1/4 단위로 내린 값)
/// - `luminance_color`: 글자 색. (sRGB 의 빨강, 초록, 파랑, 원본처럼 흰색으로 굽는 글은 [255, 255, 255])
///
/// # 돌려줌
/// 래스터. 윤곽이 없는 글리프(빈칸)는 너비와 높이가 0. left_x 는 서브픽셀 몫을 뺀 정수 자리에서 잰다.
///
/// # 예제
/// 서브픽셀 자리를 옮겨도 덮인 정도의 합은 거의 같다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::{rasterize_glyph_at, shape_text, LoadedFont, ResolvedFontFace};
/// use vanilla::platform::find_fallback_font_file;
/// use vanilla::{FontFaceDescriptors, ShapingDirection};
///
/// if let Some(source) = find_fallback_font_file("H", 400, false, false) {
/// 	let face = ResolvedFontFace {
/// 		font: Rc::new(LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font")),
/// 		descriptors: FontFaceDescriptors::new(),
/// 		is_synthetic_bold: false,
/// 		is_synthetic_italic: false,
/// 	};
/// 	let glyph_index = shape_text(&[face.clone()], "H", 24.0, ShapingDirection::LeftToRight, None).glyph_list[0].glyph_index;
/// 	let glyph_raster = rasterize_glyph_at(&face, glyph_index, 24.0, 0.0, [255, 255, 255]);
/// 	let shifted_glyph_raster = rasterize_glyph_at(&face, glyph_index, 24.0, 0.5, [255, 255, 255]);
/// 	let sum = |coverage: &[u8]| coverage.iter().map(|value| *value as u32).sum::<u32>() as f32;
/// 	assert!(glyph_raster.width > 0 && shifted_glyph_raster.width > 0);
/// 	assert!((sum(&glyph_raster.coverage) / sum(&shifted_glyph_raster.coverage) - 1.0).abs() < 0.1);
/// }
/// ```
//==============================================================================
pub fn rasterize_glyph_at(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32, subpixel_x: f32, luminance_color: [u8; 3]) -> GlyphRaster {
	#[cfg(target_os = "macos")]
	{
		if let Some(glyph_raster) = rasterize_core_text_glyph(face, glyph_index, pixel_size) {
			return glyph_raster;
		}
		let is_color_glyph = face.font.has_color_tables && colorglyph::has_color_glyph(face.font.get_shaping_face(), glyph_index);
		if !is_color_glyph {
			if let Some(glyph_raster) = rasterize_core_text_mask(face, glyph_index, pixel_size, subpixel_x, luminance_color) {
				return glyph_raster;
			}
		}
	}
	#[cfg(target_os = "windows")]
	{
		let is_color_glyph = face.font.has_color_tables && colorglyph::has_color_glyph(face.font.get_shaping_face(), glyph_index);
		if !is_color_glyph {
			if let Some(glyph_raster) = rasterize_direct_write_mask(face, glyph_index, pixel_size, subpixel_x, luminance_color) {
				return glyph_raster;
			}
		}
	}
	#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
	{
		let is_color_glyph = face.font.has_color_tables && colorglyph::has_color_glyph(face.font.get_shaping_face(), glyph_index);
		if !is_color_glyph {
			if let Some(glyph_raster) = rasterize_hinted_outline_mask(face, glyph_index, pixel_size, subpixel_x, luminance_color) {
				return glyph_raster;
			}
		}
	}
	#[cfg(not(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android")))))]
	{
		let _ = (subpixel_x, luminance_color);
	}
	if face.font.has_color_tables {
		if let Some(color_glyph_raster) = colorglyph::rasterize_color_glyph(face.font.get_shaping_face(), glyph_index, pixel_size) {
			let mut glyph_raster = GlyphRaster {
				coverage: vec![0u8; (color_glyph_raster.width * color_glyph_raster.height) as usize],
				width: color_glyph_raster.width,
				height: color_glyph_raster.height,
				left_x: color_glyph_raster.left_x,
				top_y: color_glyph_raster.top_y,
				color_pixels: Some(color_glyph_raster.pixels),
			};
			if face.is_synthetic_italic {
				apply_synthetic_italic(&mut glyph_raster);
			}
			return glyph_raster;
		}
	}
	let mut glyph_raster = if face.font.is_outline_raster() {
		// 가변 글꼴: 축 값을 건 윤곽으로. (fontdue 는 기본 인스턴스만 그리고 SFNS 같은 글꼴은 그리지 못한다)
		let scale = pixel_size / face.font.units_per_em;
		match colorglyph::rasterize_outline(&|outline_builder| face.font.draw_glyph_outline(glyph_index, pixel_size, outline_builder), scale) {
			Some((coverage, width, height, left_x, top_y)) => {
				GlyphRaster {
					coverage,
					width,
					height,
					left_x,
					top_y,
					color_pixels: None,
				}
			}
			None => {
				GlyphRaster {
					coverage: Vec::new(),
					width: 0,
					height: 0,
					left_x: 0,
					top_y: 0,
					color_pixels: None,
				}
			}
		}
	}
	else {
		let (metrics, bitmap) = face.font.raster_font.rasterize_indexed(glyph_index, pixel_size);
		GlyphRaster {
			coverage: bitmap,
			width: metrics.width as i32,
			height: metrics.height as i32,
			left_x: metrics.xmin,
			top_y: metrics.ymin + metrics.height as i32,
			color_pixels: None,
		}
	};
	if glyph_raster.width == 0 || glyph_raster.height == 0 {
		return glyph_raster;
	}
	if face.is_synthetic_bold {
		let radius = calculate_synthetic_bold_radius(pixel_size);
		let margin = radius.ceil() as i32 + 1;
		let width = glyph_raster.width + margin * 2;
		let height = glyph_raster.height + margin * 2;
		let mut padded = vec![0u8; (width * height) as usize];
		for position_y in 0..glyph_raster.height {
			for position_x in 0..glyph_raster.width {
				padded[((position_y + margin) * width + position_x + margin) as usize] = glyph_raster.coverage[(position_y * glyph_raster.width + position_x) as usize];
			}
		}
		glyph_raster.coverage = dilate_coverage(&padded, width, height, radius);
		glyph_raster.width = width;
		glyph_raster.height = height;
		glyph_raster.left_x -= margin;
		glyph_raster.top_y += margin;
	}
	if face.is_synthetic_italic {
		apply_synthetic_italic(&mut glyph_raster);
	}
	glyph_raster
}

//==============================================================================
/// 맥 CoreText 로 글리프 그리기. (CoreText 로 그리는 글꼴이 아니면 None. 색 글리프는 제 색, 그 밖은 흰색의 알파를 덮인 정도로)
/// - 가짜 기울임은 색 글리프에도 입히고, 가짜 굵게는 색이 아닌 글리프에만 입힌다. (rasterize_glyph 와 같다)
//==============================================================================
#[cfg(target_os = "macos")]
fn rasterize_core_text_glyph(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32) -> Option<GlyphRaster> {
	// 색 이모지 글꼴(sbix)만. (윤곽 표가 없는 글꼴은 rasterize_core_text_mask 가 그린다)
	if !face.font.is_core_text_raster() || face.font.get_shaping_face().tables().sbix.is_none() {
		return None;
	}
	let source_path = face.font.source_path.as_ref()?;
	let is_color_glyph = face.font.has_color_tables && colorglyph::has_color_glyph(face.font.get_shaping_face(), glyph_index);
	let core_text_raster = match crate::platform::desktop::coretextglyph::rasterize_glyph(source_path, face.font.face_index, glyph_index, pixel_size, &face.font.read_core_text_variation_list(pixel_size)) {
		Some(core_text_raster) => {
			core_text_raster
		}
		None => {
			return Some(GlyphRaster {
				coverage: Vec::new(),
				width: 0,
				height: 0,
				left_x: 0,
				top_y: 0,
				color_pixels: None,
			});
		}
	};
	let pixel_count = (core_text_raster.width * core_text_raster.height) as usize;
	let mut glyph_raster = if is_color_glyph {
		GlyphRaster {
			coverage: vec![0u8; pixel_count],
			width: core_text_raster.width,
			height: core_text_raster.height,
			left_x: core_text_raster.left_x,
			top_y: core_text_raster.top_y,
			color_pixels: Some(core_text_raster.pixels),
		}
	}
	else {
		GlyphRaster {
			coverage: core_text_raster.pixels.chunks_exact(4).map(|pixel| pixel[3]).collect(),
			width: core_text_raster.width,
			height: core_text_raster.height,
			left_x: core_text_raster.left_x,
			top_y: core_text_raster.top_y,
			color_pixels: None,
		}
	};
	if face.is_synthetic_bold && !is_color_glyph {
		let radius = calculate_synthetic_bold_radius(pixel_size);
		let margin = radius.ceil() as i32 + 1;
		let width = glyph_raster.width + margin * 2;
		let height = glyph_raster.height + margin * 2;
		let mut padded = vec![0u8; (width * height) as usize];
		for position_y in 0..glyph_raster.height {
			for position_x in 0..glyph_raster.width {
				padded[((position_y + margin) * width + position_x + margin) as usize] = glyph_raster.coverage[(position_y * glyph_raster.width + position_x) as usize];
			}
		}
		glyph_raster.coverage = dilate_coverage(&padded, width, height, radius);
		glyph_raster.width = width;
		glyph_raster.height = height;
		glyph_raster.left_x -= margin;
		glyph_raster.top_y += margin;
	}
	if face.is_synthetic_italic {
		apply_synthetic_italic(&mut glyph_raster);
	}
	Some(glyph_raster)
}

//==============================================================================
/// 맥 CoreText 로 색이 아닌 글리프의 덮인 정도 그리기. (크롬 맥 Canvas2D 의 A8 마스크, CoreText 가 글꼴을 짓지 못하거나 그릴 것이 없으면 None)
/// - 파일에서 읽은 글꼴은 그 파일로, 바이트로 올린 글꼴은 그 바이트로 CoreText 글꼴을 짓는다. (크롬도 받은 웹 글꼴을 CoreText 로 그린다)
/// - 가짜 굵게는 rasterize_glyph 처럼 덮인 정도를 넓힌다.
//==============================================================================
#[cfg(target_os = "macos")]
fn rasterize_core_text_mask(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32, subpixel_x: f32, luminance_color: [u8; 3]) -> Option<GlyphRaster> {
	use crate::core::maskgamma;
	use crate::platform::desktop::coretextglyph::{self, CoreTextFontSource, SmoothBehavior};
	let font_source = match face.font.source_path.as_ref() {
		Some(source_path) => {
			CoreTextFontSource::Path(source_path)
		}
		None => {
			CoreTextFontSource::Bytes(face.font.get_font_bytes())
		}
	};
	// 감마 전보정 표. (SkTypeface_Mac::onFilterRec: 글꼴 부드럽게가 회색이면 밝기 색에 1/2, LCD 면 3/4 를 곱하고 대비 0, 부드럽게가 없으면 고치지 않는다. 감마는 크롬 맥의 SK_GAMMA_SRGB)
	let smooth_behavior = coretextglyph::read_smooth_behavior();
	let correcting_table = match smooth_behavior {
		SmoothBehavior::None => {
			None
		}
		SmoothBehavior::Some | SmoothBehavior::Subpixel => {
			let scale_channel = |value: u8| -> u8 {
				let canonical_value = maskgamma::canonicalize_channel(value) as u32;
				if smooth_behavior == SmoothBehavior::Some {
					(canonical_value / 2) as u8
				}
				else {
					(canonical_value * 3 / 4) as u8
				}
			};
			let luminance = maskgamma::resolve_a8_luminance(scale_channel(luminance_color[0]), scale_channel(luminance_color[1]), scale_channel(luminance_color[2]));
			maskgamma::find_a8_correcting_table(luminance, 0.0, 0.0)
		}
	};
	let skew_x = if face.is_synthetic_italic {
		SYNTHETIC_ITALIC_SKEW
	}
	else {
		0.0
	};
	let glyph_mask = coretextglyph::rasterize_glyph_mask(font_source, face.font.face_index, glyph_index, pixel_size, &face.font.read_core_text_variation_list(pixel_size), subpixel_x, skew_x, correcting_table.as_ref())?;
	let mut glyph_raster = GlyphRaster {
		coverage: glyph_mask.coverage,
		width: glyph_mask.width,
		height: glyph_mask.height,
		left_x: glyph_mask.left_x,
		top_y: glyph_mask.top_y,
		color_pixels: None,
	};
	if face.is_synthetic_bold {
		let radius = calculate_synthetic_bold_radius(pixel_size);
		let margin = radius.ceil() as i32 + 1;
		let width = glyph_raster.width + margin * 2;
		let height = glyph_raster.height + margin * 2;
		let mut padded = vec![0u8; (width * height) as usize];
		for position_y in 0..glyph_raster.height {
			for position_x in 0..glyph_raster.width {
				padded[((position_y + margin) * width + position_x + margin) as usize] = glyph_raster.coverage[(position_y * glyph_raster.width + position_x) as usize];
			}
		}
		glyph_raster.coverage = dilate_coverage(&padded, width, height, radius);
		glyph_raster.width = width;
		glyph_raster.height = height;
		glyph_raster.left_x -= margin;
		glyph_raster.top_y += margin;
	}
	Some(glyph_raster)
}

//==============================================================================
/// 가짜 굵게 입히기. (덮인 정도를 반지름만큼 넓히고 그만큼 상자를 키운다, 맥 CoreText 마스크와 같은 셈)
//==============================================================================
#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
fn apply_synthetic_bold(glyph_raster: &mut GlyphRaster, pixel_size: f32) {
	if glyph_raster.width == 0 || glyph_raster.height == 0 {
		return;
	}
	let radius = calculate_synthetic_bold_radius(pixel_size);
	let margin = radius.ceil() as i32 + 1;
	let width = glyph_raster.width + margin * 2;
	let height = glyph_raster.height + margin * 2;
	let mut padded = vec![0u8; (width * height) as usize];
	for position_y in 0..glyph_raster.height {
		for position_x in 0..glyph_raster.width {
			padded[((position_y + margin) * width + position_x + margin) as usize] = glyph_raster.coverage[(position_y * glyph_raster.width + position_x) as usize];
		}
	}
	glyph_raster.coverage = dilate_coverage(&padded, width, height, radius);
	glyph_raster.width = width;
	glyph_raster.height = height;
	glyph_raster.left_x -= margin;
	glyph_raster.top_y += margin;
}

/// DirectWrite 렌더링 고르기. (스키아 SkScalerContext_DW 생성자가 고르는 렌더링 모드, 측정 모드, 그릴 글자 크기)
#[cfg(any(test, target_os = "windows"))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DirectWriteRenderChoice {
	/// DWRITE_RENDERING_MODE 값. (1 ALIASED, 2 GDI_CLASSIC, 4 NATURAL, 5 NATURAL_SYMMETRIC)
	pub rendering_mode: i32,
	/// DWRITE_MEASURING_MODE 값. (0 NATURAL, 1 GDI_CLASSIC)
	pub measuring_mode: i32,
	/// 그릴 글자 크기. (gdi 호환이면 1/64 로 반올림한 크기)
	pub render_size: f32,
}

//==============================================================================
/// 글꼴 표의 큰 끝 u16, u32 읽기. (모자라면 None)
//==============================================================================
#[cfg(any(test, target_os = "windows"))]
fn read_table_u16(table: &[u8], offset: usize) -> Option<u16> {
	Some(u16::from_be_bytes([*table.get(offset)?, *table.get(offset + 1)?]))
}

#[cfg(any(test, target_os = "windows"))]
fn read_table_u32(table: &[u8], offset: usize) -> Option<u32> {
	Some(u32::from_be_bytes([*table.get(offset)?, *table.get(offset + 1)?, *table.get(offset + 2)?, *table.get(offset + 3)?]))
}

//==============================================================================
/// gasp 표에서 크기의 범위 찾기. ((가장 작은 ppem, 가장 큰 ppem, 판, 깃발), 스키아 SkScalerContext_win_dw.cpp 의 get_gasp_range)
//==============================================================================
#[cfg(any(test, target_os = "windows"))]
fn find_gasp_range(face: &rustybuzz::Face<'_>, size: i32) -> Option<(i32, i32, u16, u16)> {
	let gasp = face.raw_face().table(rustybuzz::ttf_parser::Tag::from_bytes(b"gasp"))?;
	let version = read_table_u16(gasp, 0)?;
	if version > 1 {
		return None;
	}
	let range_count = read_table_u16(gasp, 2)? as usize;
	if range_count > 1024 || gasp.len() < 4 + range_count * 4 {
		return None;
	}
	let mut minimum_ppem = -1i32;
	for range_index in 0..range_count {
		let maximum_ppem = read_table_u16(gasp, 4 + range_index * 4)? as i32;
		if minimum_ppem < size && size <= maximum_ppem {
			return Some((minimum_ppem + 1, maximum_ppem, version, read_table_u16(gasp, 6 + range_index * 4)?));
		}
		minimum_ppem = maximum_ppem;
	}
	None
}

//==============================================================================
/// 범위 안의 비트맵 글리프가 있는지. (스키아 has_bitmap_strike: EBLC 의 같은 가로세로 ppem 에 글리프가 넷 넘게, 없으면 EBSC 의 같은 ppem)
//==============================================================================
#[cfg(any(test, target_os = "windows"))]
fn has_bitmap_strike(face: &rustybuzz::Face<'_>, minimum_ppem: i32, maximum_ppem: i32) -> bool {
	let raw_face = face.raw_face();
	let eblc = match raw_face.table(rustybuzz::ttf_parser::Tag::from_bytes(b"EBLC")) {
		Some(eblc) => {
			eblc
		}
		None => {
			return false;
		}
	};
	if read_table_u32(eblc, 0) != Some(0x0002_0000) {
		return false;
	}
	let size_count = read_table_u32(eblc, 4).unwrap_or(0) as usize;
	if size_count > 1024 || eblc.len() < 8 + size_count * 48 {
		return false;
	}
	for size_index in 0..size_count {
		let record = &eblc[8 + size_index * 48..8 + size_index * 48 + 48];
		let (ppem_x, ppem_y) = (record[44] as i32, record[45] as i32);
		let start_glyph = read_table_u16(record, 40).unwrap_or(0) as u32;
		let end_glyph = read_table_u16(record, 42).unwrap_or(0) as u32;
		if ppem_x == ppem_y && minimum_ppem <= ppem_x && ppem_x <= maximum_ppem && end_glyph >= start_glyph + 3 {
			return true;
		}
	}
	let ebsc = match raw_face.table(rustybuzz::ttf_parser::Tag::from_bytes(b"EBSC")) {
		Some(ebsc) => {
			ebsc
		}
		None => {
			return false;
		}
	};
	if read_table_u32(ebsc, 0) != Some(0x0002_0000) {
		return false;
	}
	let scale_count = read_table_u32(ebsc, 4).unwrap_or(0) as usize;
	if scale_count > 1024 || ebsc.len() < 8 + scale_count * 28 {
		return false;
	}
	(0..scale_count).any(|scale_index| {
		let (ppem_x, ppem_y) = (ebsc[8 + scale_index * 28 + 24] as i32, ebsc[8 + scale_index * 28 + 25] as i32);
		ppem_x == ppem_y && minimum_ppem <= ppem_x && ppem_x <= maximum_ppem
	})
}

//==============================================================================
/// DirectWrite 렌더링 고르기. (스키아 SkScalerContext_DW 생성자의 A8 길: 비트맵 글꼴, gasp 판 1, 20 픽셀 넘거나 바이트코드 힌트 없음, 그 밖)
/// - 크롬은 글꼴에 비트맵을 허락하고(setEmbeddedBitmaps), 힌팅은 kNormal(격자 맞춤 켬), 선형 측정은 끈다. CBDT 가 있으면 측정은 NATURAL 이다.
/// - `is_axis_aligned`: 기울임이 없는지. (가짜 기울임은 fPreSkewX 라 아니다)
//==============================================================================
#[cfg(any(test, target_os = "windows"))]
fn choose_direct_write_render(face: &rustybuzz::Face<'_>, pixel_size: f32, is_axis_aligned: bool) -> DirectWriteRenderChoice {
	const ALIASED_MODE: i32 = 1;
	const GDI_CLASSIC_MODE: i32 = 2;
	const NATURAL_MODE: i32 = 4;
	const NATURAL_SYMMETRIC_MODE: i32 = 5;
	const NATURAL_MEASURE: i32 = 0;
	const GDI_CLASSIC_MEASURE: i32 = 1;
	let _ = ALIASED_MODE;
	let real_size = pixel_size;
	let mut gdi_size = (real_size * 64.0).round() / 64.0;
	if gdi_size == 0.0 {
		gdi_size = 1.0;
	}
	let bitmap_ppem = gdi_size.trunc() as i32;
	let (mut minimum_ppem, mut maximum_ppem) = (bitmap_ppem, bitmap_ppem);
	if let Some((range_minimum, range_maximum, _, flags)) = find_gasp_range(face, bitmap_ppem) {
		// 격자 맞춤만 켠 범위면 그 범위를 통째로 비트맵 범위로 본다. (is_gridfit_only)
		if flags == 0x0001 {
			minimum_ppem = range_minimum;
			maximum_ppem = range_maximum;
		}
	}
	let is_bitmap_like = has_bitmap_strike(face, minimum_ppem, maximum_ppem);
	let is_hinted = face
		.raw_face()
		.table(rustybuzz::ttf_parser::Tag::from_bytes(b"maxp"))
		.filter(|maxp| maxp.len() >= 32 && read_table_u32(maxp, 0) == Some(0x0001_0000))
		.and_then(|maxp| read_table_u16(maxp, 26))
		.is_some_and(|instruction_size| instruction_size != 0);
	let mut choice = if is_bitmap_like && is_axis_aligned {
		DirectWriteRenderChoice {
			rendering_mode: GDI_CLASSIC_MODE,
			measuring_mode: GDI_CLASSIC_MEASURE,
			render_size: gdi_size,
		}
	}
	else if is_bitmap_like {
		DirectWriteRenderChoice {
			rendering_mode: NATURAL_SYMMETRIC_MODE,
			measuring_mode: GDI_CLASSIC_MEASURE,
			render_size: gdi_size,
		}
	}
	else if let Some((_, _, 1, flags)) = find_gasp_range(face, gdi_size.round() as i32) {
		// gasp 판 1 의 SymmetricSmoothing(비트 3).
		DirectWriteRenderChoice {
			rendering_mode: if flags & 0x0008 != 0 {
				NATURAL_SYMMETRIC_MODE
			}
			else {
				NATURAL_MODE
			},
			measuring_mode: NATURAL_MEASURE,
			render_size: real_size,
		}
	}
	else if real_size > 20.0 || !is_hinted {
		DirectWriteRenderChoice {
			rendering_mode: NATURAL_SYMMETRIC_MODE,
			measuring_mode: NATURAL_MEASURE,
			render_size: real_size,
		}
	}
	else {
		DirectWriteRenderChoice {
			rendering_mode: NATURAL_MODE,
			measuring_mode: NATURAL_MEASURE,
			render_size: gdi_size,
		}
	};
	if choice.measuring_mode != NATURAL_MEASURE && face.raw_face().table(rustybuzz::ttf_parser::Tag::from_bytes(b"CBDT")).is_some() {
		choice.measuring_mode = NATURAL_MEASURE;
	}
	choice
}

//==============================================================================
/// 윈도우 DirectWrite 로 글리프 마스크 그리기. (크롬 윈도우 Canvas2D 와 같은 길, DirectWrite 가 그리지 못하면 None)
/// - 감마 전보정: 글자 색의 A8 밝기 단계 표, 대비와 감마는 크롬처럼 Avalon.Graphics 키가 있을 때만 시스템 값이고 없으면 대비 1, sRGB 다.
/// - 가짜 굵게는 맥처럼 덮인 정도를 넓힌다. (스키아는 경로 획으로 그린다)
//==============================================================================
#[cfg(target_os = "windows")]
fn rasterize_direct_write_mask(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32, subpixel_x: f32, luminance_color: [u8; 3]) -> Option<GlyphRaster> {
	use crate::core::maskgamma;
	use crate::platform::desktop::directwriteglyph;
	let (contrast, gamma) = directwriteglyph::read_text_contrast_and_gamma();
	let luminance = maskgamma::resolve_a8_luminance(luminance_color[0], luminance_color[1], luminance_color[2]);
	let correcting_table = maskgamma::find_a8_correcting_table(luminance, contrast, gamma);
	let skew_x = if face.is_synthetic_italic {
		-SYNTHETIC_ITALIC_SKEW
	}
	else {
		0.0
	};
	let render_choice = choose_direct_write_render(face.font.get_shaping_face(), pixel_size, !face.is_synthetic_italic);
	let variation_list: Vec<(u32, f32)> = if face.font.is_outline_raster() {
		face.font.read_sized_variation_list(pixel_size).iter().map(|variation| (variation.tag.0, variation.value)).collect()
	}
	else {
		Vec::new()
	};
	let glyph_mask = directwriteglyph::rasterize_glyph_mask(face.font.get_font_bytes(), face.font.source_path.as_deref(), face.font.face_index, &variation_list, glyph_index, render_choice.rendering_mode, render_choice.measuring_mode, render_choice.render_size, subpixel_x, skew_x, correcting_table.as_ref())?;
	let mut glyph_raster = GlyphRaster {
		coverage: glyph_mask.coverage,
		width: glyph_mask.width,
		height: glyph_mask.height,
		left_x: glyph_mask.left_x,
		top_y: glyph_mask.top_y,
		color_pixels: None,
	};
	if face.is_synthetic_bold {
		apply_synthetic_bold(&mut glyph_raster, pixel_size);
	}
	Some(glyph_raster)
}

/// 리눅스 힌팅 캐시 열쇠. (글꼴 바이트 자리, 얼굴 번호, 크기 비트, 축 값, 힌팅 단계, 자동 힌팅 강제)
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
type HintingCacheKey = (usize, u32, u32, Vec<(u32, u32)>, u8, bool);

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
thread_local! {
	/// 힌팅 인스턴스 캐시. (바이트를 붙잡은 Rc 와 함께, None 은 힌팅 없음)
	static HINTING_INSTANCE_CACHE: std::cell::RefCell<std::collections::HashMap<HintingCacheKey, (Rc<[u8]>, Option<skrifa::outline::HintingInstance>)>> = std::cell::RefCell::new(std::collections::HashMap::new());
	/// 자동 힌팅 글리프 모양 캐시. ((글꼴 바이트 자리, 얼굴 번호) → 모양, 스키아 BridgeGlyphStyles 처럼 글꼴마다 한 번)
	static GLYPH_STYLES_CACHE: std::cell::RefCell<std::collections::HashMap<(usize, u32), (Rc<[u8]>, skrifa::outline::GlyphStyles)>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// 리눅스 힌팅 캐시 상한. (넘으면 비운다)
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
const MAXIMUM_HINTING_INSTANCE_COUNT: usize = 256;

//==============================================================================
/// skrifa 윤곽을 옮겨 ttf-parser 윤곽 받개로 넘기는 펜. (x' = x + 기울기 * y + 서브픽셀 자리, 픽셀 단위 위가 양수)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
struct ShiftedOutlinePen<'builder> {
	outline_builder: &'builder mut dyn rustybuzz::ttf_parser::OutlineBuilder,
	skew_x: f32,
	shift_x: f32,
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
impl ShiftedOutlinePen<'_> {
	//==============================================================================
	/// 점 옮기기.
	//==============================================================================
	fn shift(&self, position_x: f32, position_y: f32) -> f32 {
		position_x + self.skew_x * position_y + self.shift_x
	}
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
impl skrifa::outline::OutlinePen for ShiftedOutlinePen<'_> {
	//==============================================================================
	/// 새 윤곽 시작.
	//==============================================================================
	fn move_to(&mut self, position_x: f32, position_y: f32) {
		let shifted_x = self.shift(position_x, position_y);
		self.outline_builder.move_to(shifted_x, position_y);
	}

	//==============================================================================
	/// 곧은 선.
	//==============================================================================
	fn line_to(&mut self, position_x: f32, position_y: f32) {
		let shifted_x = self.shift(position_x, position_y);
		self.outline_builder.line_to(shifted_x, position_y);
	}

	//==============================================================================
	/// 2 차 곡선.
	//==============================================================================
	fn quad_to(&mut self, control_x: f32, control_y: f32, position_x: f32, position_y: f32) {
		let (shifted_control_x, shifted_x) = (self.shift(control_x, control_y), self.shift(position_x, position_y));
		self.outline_builder.quad_to(shifted_control_x, control_y, shifted_x, position_y);
	}

	//==============================================================================
	/// 3 차 곡선.
	//==============================================================================
	fn curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, position_x: f32, position_y: f32) {
		let (shifted_first_x, shifted_second_x, shifted_x) = (self.shift(first_control_x, first_control_y), self.shift(second_control_x, second_control_y), self.shift(position_x, position_y));
		self.outline_builder.curve_to(shifted_first_x, first_control_y, shifted_second_x, second_control_y, shifted_x, position_y);
	}

	//==============================================================================
	/// 윤곽 닫기.
	//==============================================================================
	fn close(&mut self) {
		self.outline_builder.close();
	}
}

//==============================================================================
/// 글꼴의 이름 표 가족 이름. (fontconfig 에 렌더 설정을 물을 이름, 없으면 빈 글)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
fn read_family_name(face: &rustybuzz::Face<'_>) -> String {
	face.names()
		.into_iter()
		.find(|name| name.name_id == rustybuzz::ttf_parser::name_id::FAMILY && name.is_unicode())
		.and_then(|name| name.to_string())
		.unwrap_or_default()
}

//==============================================================================
/// 리눅스: 크롬처럼 skrifa 윤곽에 힌팅을 걸어 그리기. (크롬 158 리눅스는 시스템 글꼴과 보통 웹 글꼴도 스키아 SkTypeface_Fontations 로 그린다)
/// - 크롬 skia/ext/font_utils.cc 의 SkFontMgr_New_FCI(SkFontScanner_Make_Fontations()) 라 서체가 Fontations 이고, 글리프는 skrifa 윤곽을 스키아 경로로 칠한다. (SkScalerContext_Fontations::generateImage 의 generateImageFromPath)
/// - 힌팅: fontconfig 의 hintstyle 을 크롬처럼 읽어(QueryFontconfig) 없음은 힌팅 없음, 가볍게는 Light 에 glyf 자동 힌팅, 보통과 꽉은(LCD 가 아니면 꽉을 보통으로) Normal 에 자동 힌팅 대체다.
///   skrifa 의 Target::Smooth 에 symmetric_rendering 참, preserve_linear_metrics 거짓. 힌팅이 꼭 있어야 하는 글꼴은 Strong. (스키아 src/ports/fontations/src/hinting.rs, SkTypeface_fontations.cpp)
/// - 서브픽셀 자리: 블링크는 힌팅이 꽉이 아니면 서브픽셀 자리를 켠다. (WebFontRenderStyle::ApplyToSkFont)
/// - 감마 전보정: 크롬 리눅스의 감마 1.2, 대비 0.2. (크로미엄 skia/BUILD.gn, Fontations 의 onFilterRec 은 FreeType 과 달리 A8 의 전보정을 끄지 않는다)
/// - 칠하기는 스키아 SkScan_AAAPath 와 같은 해석적 AA 다. (core/analyticrasterizer.rs, 상자는 조절점까지 넣은 경로 경계를 내림, 올림한 것)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
fn rasterize_hinted_outline_mask(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32, subpixel_x: f32, luminance_color: [u8; 3]) -> Option<GlyphRaster> {
	use crate::core::maskgamma;
	use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingMode, HintingOptions, OutlineGlyphFormat, SmoothMode, Target};
	use skrifa::MetadataProvider;
	let font = &face.font;
	let shaping_face = font.get_shaping_face();
	let (_, mut hint_level, is_autohint_forced) = crate::platform::desktop::systemfont::read_font_render_style(&read_family_name(shaping_face), pixel_size, shaping_face.weight().to_number(), shaping_face.is_italic());
	// 기울임은 축에 맞지 않아 힌팅을 끈다. (onFilterRec 의 isAxisAligned) 꽉은 LCD 가 아니라 보통으로.
	if face.is_synthetic_italic {
		hint_level = 0;
	}
	if hint_level == 3 {
		hint_level = 2;
	}
	let font_bytes = font.get_font_bytes();
	let font_ref = skrifa::FontRef::from_index(font_bytes, font.face_index).ok()?;
	let sized_variation_list = font.read_sized_variation_list(pixel_size);
	let location = font_ref.axes().location(sized_variation_list.iter().map(|variation| (skrifa::Tag::new(&variation.tag.to_bytes()), variation.value)));
	let outline_glyph_collection = font_ref.outline_glyphs();
	let outline_glyph = outline_glyph_collection.get(skrifa::GlyphId::new(glyph_index as u32))?;
	let key: HintingCacheKey = (font_bytes.as_ptr() as usize, font.face_index, pixel_size.to_bits(), sized_variation_list.iter().map(|variation| (variation.tag.0, variation.value.to_bits())).collect(), hint_level, is_autohint_forced);
	let cached_instance = HINTING_INSTANCE_CACHE.with(|cache| cache.borrow().get(&key).map(|(_, hinting_instance)| hinting_instance.clone()));
	let hinting_instance = match cached_instance {
		Some(hinting_instance) => {
			hinting_instance
		}
		None => {
			let size = skrifa::instance::Size::new(pixel_size);
			let hinting_instance = if outline_glyph_collection.require_interpreter() {
				HintingInstance::new(&outline_glyph_collection, size, &location, HintingMode::Strong).ok()
			}
			else if hint_level == 0 {
				None
			}
			else {
				let is_glyf = outline_glyph_collection.format() == Some(OutlineGlyphFormat::Glyf);
				let is_auto = is_autohint_forced || (hint_level == 1 && is_glyf);
				let engine = if is_auto {
					let glyph_styles = GLYPH_STYLES_CACHE.with(|cache| {
						let mut cache = cache.borrow_mut();
						cache.entry((font_bytes.as_ptr() as usize, font.face_index)).or_insert_with(|| (font_bytes.clone(), skrifa::outline::GlyphStyles::new(&outline_glyph_collection))).1.clone()
					});
					Engine::Auto(Some(glyph_styles))
				}
				else {
					Engine::AutoFallback
				};
				let target = Target::Smooth {
					mode: if hint_level == 1 {
						SmoothMode::Light
					}
					else {
						SmoothMode::Normal
					},
					symmetric_rendering: true,
					preserve_linear_metrics: false,
				};
				HintingInstance::new(
					&outline_glyph_collection,
					size,
					&location,
					HintingOptions {
						engine,
						target,
					},
				)
				.ok()
			};
			HINTING_INSTANCE_CACHE.with(|cache| {
				let mut cache = cache.borrow_mut();
				if cache.len() >= MAXIMUM_HINTING_INSTANCE_COUNT {
					cache.clear();
				}
				cache.insert(key, (font_bytes.clone(), hinting_instance.clone()));
			});
			hinting_instance
		}
	};
	let skew_x = if face.is_synthetic_italic {
		SYNTHETIC_ITALIC_SKEW
	}
	else {
		0.0
	};
	let draw_outline = |outline_builder: &mut dyn rustybuzz::ttf_parser::OutlineBuilder| -> bool {
		let mut outline_pen = ShiftedOutlinePen {
			outline_builder,
			skew_x,
			shift_x: subpixel_x,
		};
		let settings = match &hinting_instance {
			Some(hinting_instance) => {
				DrawSettings::hinted(hinting_instance, false)
			}
			None => {
				DrawSettings::unhinted(skrifa::instance::Size::new(pixel_size), &location)
			}
		};
		outline_glyph.draw(settings, &mut outline_pen).is_ok()
	};
	let mut glyph_raster = match crate::core::analyticrasterizer::rasterize_outline(&draw_outline, 1.0) {
		Some((coverage, width, height, left_x, top_y)) => {
			GlyphRaster {
				coverage,
				width,
				height,
				left_x,
				top_y,
				color_pixels: None,
			}
		}
		None => {
			GlyphRaster {
				coverage: Vec::new(),
				width: 0,
				height: 0,
				left_x: 0,
				top_y: 0,
				color_pixels: None,
			}
		}
	};
	let luminance = maskgamma::resolve_a8_luminance(luminance_color[0], luminance_color[1], luminance_color[2]);
	if let Some(correcting_table) = maskgamma::find_a8_correcting_table(luminance, LINUX_TEXT_CONTRAST, LINUX_TEXT_GAMMA) {
		for value in glyph_raster.coverage.iter_mut() {
			*value = correcting_table[*value as usize];
		}
	}
	if face.is_synthetic_bold {
		apply_synthetic_bold(&mut glyph_raster, pixel_size);
	}
	Some(glyph_raster)
}

/// 크롬 리눅스의 글자 대비. (크로미엄 skia/BUILD.gn 의 SK_GAMMA_CONTRAST=0.2)
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
const LINUX_TEXT_CONTRAST: f32 = 0.2;
/// 크롬 리눅스의 글자 감마. (크로미엄 skia/BUILD.gn 의 SK_GAMMA_EXPONENT=1.2)
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
const LINUX_TEXT_GAMMA: f32 = 1.2;

//==============================================================================
/// 가짜 기울임 입히기. (기준선 위는 오른쪽, 아래는 왼쪽으로 밀고 그만큼 넓힌다. 색 그림이 있으면 채널마다 같이 민다)
//==============================================================================
fn apply_synthetic_italic(glyph_raster: &mut GlyphRaster) {
	if glyph_raster.width == 0 || glyph_raster.height == 0 {
		return;
	}
	let top_extent = glyph_raster.top_y.max(0) as f32;
	let bottom_extent = (glyph_raster.height - glyph_raster.top_y).max(0) as f32;
	let left_margin = (bottom_extent * SYNTHETIC_ITALIC_SKEW).ceil() as i32 + 1;
	let right_margin = (top_extent * SYNTHETIC_ITALIC_SKEW).ceil() as i32 + 1;
	let width = glyph_raster.width + left_margin + right_margin;
	let height = glyph_raster.height;
	// 기준선은 이 비트맵의 top_y 줄이다.
	let baseline_y = glyph_raster.top_y as f32;
	let shear_channel = |source: &[u8], channel_count: usize, channel_index: usize| -> Vec<u8> {
		let mut padded = vec![0u8; (width * height) as usize];
		for position_y in 0..height {
			for position_x in 0..glyph_raster.width {
				padded[(position_y * width + position_x + left_margin) as usize] = source[(position_y * glyph_raster.width + position_x) as usize * channel_count + channel_index];
			}
		}
		shear_coverage(&padded, width, height, baseline_y, SYNTHETIC_ITALIC_SKEW)
	};
	let sheared_coverage = shear_channel(&glyph_raster.coverage, 1, 0);
	if let Some(color_pixels) = glyph_raster.color_pixels.as_ref() {
		let channel_list: Vec<Vec<u8>> = (0..4).map(|channel_index| shear_channel(color_pixels, 4, channel_index)).collect();
		let mut sheared_pixels = vec![0u8; (width * height * 4) as usize];
		for pixel_index in 0..(width * height) as usize {
			for channel_index in 0..4 {
				sheared_pixels[pixel_index * 4 + channel_index] = channel_list[channel_index][pixel_index];
			}
		}
		glyph_raster.color_pixels = Some(sheared_pixels);
	}
	glyph_raster.coverage = sheared_coverage;
	glyph_raster.width = width;
	glyph_raster.left_x -= left_margin;
}

//==============================================================================
/// 덮인 정도를 원 모양으로 넓히기. (가짜 굵게, 윤곽을 바깥으로 radius 픽셀 민 것)
/// - 덮인 정도를 픽셀 가운데에서 윤곽까지의 거리로 본다. (덮인 정도 - 0.5, 안이 양수)
///   이웃 픽셀의 거리에서 이웃까지의 거리를 뺀 가장 큰 값에 radius 를 더해 다시 덮인 정도로 바꾼다. 곧은 윤곽은 정확히 radius 만큼 밀린다.
///
/// # 인자
/// - `coverage`: 덮인 정도. (0 ~ 255, width * height 개)
/// - `width`: 너비. (픽셀)
/// - `height`: 높이. (픽셀)
/// - `radius`: 넓힐 반지름. (픽셀, 0 이하면 그대로 복사)
///
/// # 돌려줌
/// 넓힌 덮인 정도. (같은 크기, 원래 값보다 작아지지 않는다)
///
/// # 예제
/// 두 열짜리 막대를 반 픽셀 넓히기
/// ```rust
/// use vanilla::core::textshaping::dilate_coverage;
///
/// let width = 8;
/// let height = 3;
/// let mut coverage = vec![0u8; (width * height) as usize];
/// for position_y in 0..height {
/// 	coverage[(position_y * width + 3) as usize] = 255;
/// 	coverage[(position_y * width + 4) as usize] = 255;
/// }
/// let dilated_coverage = dilate_coverage(&coverage, width, height, 0.5);
/// assert_eq!(dilated_coverage[(width + 3) as usize], 255);
/// assert!((dilated_coverage[(width + 2) as usize] as i32 - 128).abs() <= 2);
/// assert_eq!(dilated_coverage[width as usize], 0);
/// ```
//==============================================================================
pub fn dilate_coverage(coverage: &[u8], width: i32, height: i32, radius: f32) -> Vec<u8> {
	if radius <= 0.0 {
		return coverage.to_vec();
	}
	let radius_ceiling = radius.ceil() as i32 + 1;
	let mut offset_list: Vec<(i32, i32, f32)> = Vec::new();
	for offset_y in -radius_ceiling..=radius_ceiling {
		for offset_x in -radius_ceiling..=radius_ceiling {
			let distance = ((offset_x * offset_x + offset_y * offset_y) as f32).sqrt();
			if distance <= radius + 1.0 {
				offset_list.push((offset_x, offset_y, distance));
			}
		}
	}
	let mut dilated = vec![0u8; coverage.len()];
	for position_y in 0..height {
		for position_x in 0..width {
			let mut maximum_signed_distance: f32 = f32::NEG_INFINITY;
			for (offset_x, offset_y, distance) in &offset_list {
				let source_x = position_x + offset_x;
				let source_y = position_y + offset_y;
				if source_x < 0 || source_y < 0 || source_x >= width || source_y >= height {
					continue;
				}
				let source_value = coverage[(source_y * width + source_x) as usize];
				if source_value == 0 {
					continue;
				}
				let signed_distance = source_value as f32 / 255.0 - 0.5 - distance;
				if signed_distance > maximum_signed_distance {
					maximum_signed_distance = signed_distance;
				}
			}
			if maximum_signed_distance == f32::NEG_INFINITY {
				continue;
			}
			let value = ((maximum_signed_distance + radius + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
			dilated[(position_y * width + position_x) as usize] = value.max(coverage[(position_y * width + position_x) as usize]);
		}
	}
	dilated
}

//==============================================================================
/// 덮인 정도를 가로로 기울이기. (기준선 위 줄은 오른쪽, 아래 줄은 왼쪽으로 줄 높이 × 기울기만큼 밀고, 두 픽셀 사이는 섞는다)
///
/// # 인자
/// - `coverage`: 덮인 정도. (0 ~ 255, bitmap_width * bitmap_height 개)
/// - `bitmap_width`: 너비. (픽셀)
/// - `bitmap_height`: 높이. (픽셀)
/// - `baseline_y`: 기준선 자리. (위에서 픽셀)
/// - `shear`: 기울기. (1 픽셀 올라갈 때 오른쪽으로 밀리는 픽셀)
///
/// # 돌려줌
/// 기울인 덮인 정도. (같은 크기, 밖으로 밀린 몫은 잘린다)
///
/// # 예제
/// 기준선 위 세로 막대가 오른쪽으로 밀린다
/// ```rust
/// use vanilla::core::textshaping::shear_coverage;
///
/// let width = 12;
/// let height = 12;
/// let mut coverage = vec![0u8; (width * height) as usize];
/// for position_y in 0..10 {
/// 	coverage[(position_y * width + 5) as usize] = 255;
/// }
/// let sheared_coverage = shear_coverage(&coverage, width, height, 10.0, 0.5);
/// let top_row = &sheared_coverage[0..width as usize];
/// assert_eq!(top_row[5], 0);
/// assert!(top_row[9] > 0 && top_row[10] > 0);
/// ```
//==============================================================================
pub fn shear_coverage(coverage: &[u8], bitmap_width: i32, bitmap_height: i32, baseline_y: f32, shear: f32) -> Vec<u8> {
	let read_coverage = |position_x: i32, position_y: i32| -> f32 {
		if position_x < 0 || position_x >= bitmap_width {
			return 0.0;
		}
		coverage[(position_y * bitmap_width + position_x) as usize] as f32
	};
	let mut sheared_coverage = vec![0u8; coverage.len()];
	for position_y in 0..bitmap_height {
		let shift = (baseline_y - (position_y as f32 + 0.5)) * shear;
		let shift_floor = shift.floor();
		let fraction = shift - shift_floor;
		let shift_integer = shift_floor as i32;
		for position_x in 0..bitmap_width {
			// 밀린 자리 x 의 값은 원래 자리 x - shift 에서 온다.
			let source_x = position_x - shift_integer;
			let value = read_coverage(source_x, position_y) * (1.0 - fraction) + read_coverage(source_x - 1, position_y) * fraction;
			sheared_coverage[(position_y * bitmap_width + position_x) as usize] = value.round().clamp(0.0, 255.0) as u8;
		}
	}
	sheared_coverage
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// line-height normal 의 줄 높이: 맥 크롬이 놓은 값과 같다. (헤드리스 크롬 맥에서 같은 글꼴 파일을 @font-face 로 올린 div 한 줄의 높이)
	/// - Geneva 13, 14, 16, 20px 은 17, 19, 21, 27, Arial 은 15, 16, 18, 23, Verdana 는 16, 17, 19, 24, Helvetica(맥 크롬의 15% 보정)는 15, 16, 18, 23.
	/// - USE_TYPO_METRICS 를 켠 Geneva 도 맥에서는 hhea 값이라 같다. 리눅스는 typo 값을 쓴다.
	//==============================================================================
	#[test]
	#[cfg(target_os = "macos")]
	fn normal_line_height_matches_chrome_mac() {
		let case_list: [(&str, u32, [f32; 4]); 4] = [
			("/System/Library/Fonts/Geneva.ttf", 0, [17.0, 19.0, 21.0, 27.0]),
			("/System/Library/Fonts/Supplemental/Arial.ttf", 0, [15.0, 16.0, 18.0, 23.0]),
			("/System/Library/Fonts/Supplemental/Verdana.ttf", 0, [16.0, 17.0, 19.0, 24.0]),
			("/System/Library/Fonts/Helvetica.ttc", 0, [15.0, 16.0, 18.0, 23.0]),
		];
		for (font_path, face_index, expected_list) in case_list {
			let font_bytes = match std::fs::read(font_path) {
				Ok(font_bytes) => {
					font_bytes
				}
				Err(_) => {
					continue;
				}
			};
			let loaded_font = LoadedFont::create_from_shared_bytes(Rc::from(font_bytes), face_index, &[]).expect("font");
			for (pixel_size, expected) in [13.0, 14.0, 16.0, 20.0].into_iter().zip(expected_list) {
				assert_eq!(loaded_font.calculate_normal_line_height(pixel_size), expected, "{} {}", font_path, pixel_size);
			}
		}
		// USE_TYPO_METRICS 를 켠 Geneva. (OS/2 fsSelection 의 7 번 비트)
		if let Ok(mut font_bytes) = std::fs::read("/System/Library/Fonts/Geneva.ttf") {
			let table_count = u16::from_be_bytes([font_bytes[4], font_bytes[5]]) as usize;
			for table_index in 0..table_count {
				let record = 12 + table_index * 16;
				if &font_bytes[record..record + 4] == b"OS/2" {
					let table_offset = u32::from_be_bytes([font_bytes[record + 8], font_bytes[record + 9], font_bytes[record + 10], font_bytes[record + 11]]) as usize;
					font_bytes[table_offset + 63] |= 0x80;
				}
			}
			let loaded_font = LoadedFont::create_from_shared_bytes(Rc::from(font_bytes), 0, &[]).expect("font");
			assert_eq!(loaded_font.calculate_normal_line_height(13.0), 17.0);
		}
	}

	//==============================================================================
	/// unicode-range 글 읽기. (원본 parseUnicodeRange 와 같은 결과)
	//==============================================================================
	#[test]
	fn parse_unicode_range_like_original() {
		assert_eq!(parse_unicode_range("U+0-FF, U+4??"), vec![(0, 255), (0x400, 0x4FF)]);
		assert_eq!(parse_unicode_range("U+0131"), vec![(0x131, 0x131)]);
		assert!(parse_unicode_range("").is_empty());
		let font_face_descriptors = FontFaceDescriptors::create_from_css("100 900", "italic", "U+AC00-D7A3");
		assert_eq!((font_face_descriptors.minimum_weight, font_face_descriptors.maximum_weight), (100, 900));
		assert!(font_face_descriptors.is_italic);
		assert!(font_face_descriptors.contains_character('가'));
		assert!(!font_face_descriptors.contains_character('A'));
	}

	//==============================================================================
	/// 가짜 굵게 두께. (9px 이하 1/24, 36px 이상 1/32, 사이는 섞는다. 반지름은 그 반)
	//==============================================================================
	#[test]
	fn synthetic_bold_radius_follows_skia_table() {
		assert!((calculate_synthetic_bold_radius(6.0) - 6.0 / 24.0 * 0.5).abs() < 1e-5);
		assert!((calculate_synthetic_bold_radius(48.0) - 48.0 / 32.0 * 0.5).abs() < 1e-5);
		let middle_radius = calculate_synthetic_bold_radius(22.5);
		assert!(middle_radius > 22.5 / 32.0 * 0.5 && middle_radius < 22.5 / 24.0 * 0.5);
	}

	//==============================================================================
	/// 덮인 정도 넓히기: 반지름 0.5 면 막대 양옆 열이 반쯤 덮이고 먼 곳은 비어 있다.
	//==============================================================================
	#[test]
	fn dilate_moves_edge_by_radius() {
		// 가로 4 픽셀 막대. (가운데 두 열이 덮임) 반지름 0.5 면 양옆 열이 반쯤 덮이고, 먼 곳은 비어 있다.
		let width = 8;
		let height = 3;
		let mut coverage = vec![0u8; (width * height) as usize];
		for position_y in 0..height {
			coverage[(position_y * width + 3) as usize] = 255;
			coverage[(position_y * width + 4) as usize] = 255;
		}
		let dilated = dilate_coverage(&coverage, width, height, 0.5);
		assert_eq!(dilated[(width + 3) as usize], 255);
		assert!((dilated[(width + 2) as usize] as i32 - 128).abs() <= 2);
		assert_eq!(dilated[(width) as usize], 0);
	}

	//==============================================================================
	/// 이모지 표시 판정. (UTS #51, FE0F 와 Emoji_Presentation, 국기, 키캡, 피부색은 이모지, 맨 ❤ 와 FE0E 는 글)
	//==============================================================================
	#[test]
	fn emoji_presentation_follows_uts51() {
		assert!(is_emoji_presentation("\u{1F600}"));
		assert!(is_emoji_presentation("\u{2764}\u{FE0F}"));
		assert!(!is_emoji_presentation("\u{2764}"));
		assert!(!is_emoji_presentation("\u{1F600}\u{FE0E}"));
		assert!(is_emoji_presentation("\u{1F1F0}\u{1F1F7}"));
		assert!(is_emoji_presentation("1\u{FE0F}\u{20E3}"));
		assert!(is_emoji_presentation("\u{261D}\u{1F3FD}"));
		assert!(is_emoji_presentation("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"));
		assert!(!is_emoji_presentation("A"));
		assert!(!is_emoji_presentation("가"));
		assert!(!is_emoji_presentation("1"));
	}

	//==============================================================================
	/// 얼굴 고르기: 대체 단계에서 ❤️ 는 색 글리프 얼굴로, 맨 ❤ 는 흑백 얼굴로. 시스템 조각을 쓸 수 있으면(웹) 색 얼굴이 없을 때 ❤️ 는 브라우저 조각으로.
	/// 이름 목록의 흑백 얼굴은 맨 ❤ 를 그리고 ❤️ 는 넘긴다.
	/// (운영체제가 고른 ❤ 의 흑백 대체 글꼴과 색 이모지 글꼴이 있을 때만 본다, 맥은 ZapfDingbats 와 Apple Color Emoji)
	//==============================================================================
	#[test]
	fn emoji_presentation_selects_color_face() {
		let find_font = |is_color: bool| -> Option<LoadedFont> {
			crate::platform::find_fallback_font_file("\u{2764}", 400, false, is_color).and_then(|source| {
				let font = LoadedFont::create_from_bytes(std::fs::read(source.path).ok()?).ok()?;
				let is_matched = if is_color {
					font.has_color_character('\u{2764}')
				}
				else {
					font.has_character('\u{2764}') && !font.has_color_character('\u{2764}')
				};
				if is_matched {
					Some(font)
				}
				else {
					None
				}
			})
		};
		let (text_font, color_font) = match (find_font(false), find_font(true)) {
			(Some(text_font), Some(color_font)) => {
				(text_font, color_font)
			}
			_ => {
				return;
			}
		};
		let create_face = |font: Rc<LoadedFont>, is_system_fallback: bool| ResolvedFontFace { font, descriptors: FontFaceDescriptors { is_system_fallback, ..FontFaceDescriptors::new() }, is_synthetic_bold: false, is_synthetic_italic: false };
		let text_font = Rc::new(text_font);
		let color_font = Rc::new(color_font);
		let face_list = vec![create_face(text_font.clone(), true), create_face(color_font.clone(), true)];
		let emoji_shaped_text = shape_text(&face_list, "\u{2764}\u{FE0F}", 32.0, ShapingDirection::LeftToRight, None);
		assert_eq!(emoji_shaped_text.glyph_list[0].face_index, 1);
		let text_shaped_text = shape_text(&face_list, "\u{2764}", 32.0, ShapingDirection::LeftToRight, None);
		assert_eq!(text_shaped_text.glyph_list[0].face_index, 0);
		let color_raster = rasterize_glyph(&face_list[1], emoji_shaped_text.glyph_list[0].glyph_index, 32.0);
		assert!(color_raster.color_pixels.is_some() && color_raster.width > 0);
		let text_raster = rasterize_glyph(&face_list[0], text_shaped_text.glyph_list[0].glyph_index, 32.0);
		assert!(text_raster.color_pixels.is_none());
		let system_font_measure = |_piece_text: &str, _is_right_to_left: bool| -> f32 {
			32.0
		};
		let text_face_list = vec![face_list[0].clone()];
		let system_shaped_text = shape_text(&text_face_list, "\u{2764}\u{FE0F}", 32.0, ShapingDirection::LeftToRight, Some(&system_font_measure));
		assert_eq!(system_shaped_text.glyph_list[0].face_index, SYSTEM_FACE_INDEX);
		// 이름 목록의 흑백 글꼴은 맨 ❤ 를 그리고, ❤️ 는 이형 글리프가 없어 대체의 색 글꼴로 넘긴다. (크롬 맥 Canvas2D)
		let list_face_list = vec![create_face(text_font, false), create_face(color_font, true)];
		assert_eq!(shape_text(&list_face_list, "\u{2764}", 32.0, ShapingDirection::LeftToRight, None).glyph_list[0].face_index, 0);
		assert_eq!(shape_text(&list_face_list, "\u{2764}\u{FE0F}", 32.0, ShapingDirection::LeftToRight, None).glyph_list[0].face_index, 1);
	}

	//==============================================================================
	/// 기울이면 기준선 위는 오른쪽으로 아래는 왼쪽으로 밀린다.
	//==============================================================================
	#[test]
	fn shear_moves_above_baseline_right() {
		let width = 12;
		let height = 12;
		let mut coverage = vec![0u8; (width * height) as usize];
		for position_y in 0..10 {
			coverage[(position_y * width + 5) as usize] = 255;
		}
		let sheared_coverage = shear_coverage(&coverage, width, height, 10.0, 0.5);
		let find_center_x = |position_y: i32| -> f32 {
			let mut weighted_sum = 0.0;
			let mut weight_sum = 0.0;
			for position_x in 0..width {
				let value = sheared_coverage[(position_y * width + position_x) as usize] as f32;
				weighted_sum += value * position_x as f32;
				weight_sum += value;
			}
			weighted_sum / weight_sum
		};
		assert!((find_center_x(9) - 5.25).abs() < 0.05);
		assert!((find_center_x(0) - 9.75).abs() < 0.05);
	}

	//==============================================================================
	/// DirectWrite 렌더링 고르기가 스키아 SkScalerContext_DW 의 차례를 따르는지. (비트맵 글꼴이 아니면 20 픽셀 넘는 글은 gasp 판 1 의 깃발이나 NATURAL_SYMMETRIC 에 실제 크기)
	//==============================================================================
	#[test]
	fn direct_write_render_choice_follows_skia_rules() {
		let source = match crate::platform::find_fallback_font_file("A", 400, false, false) {
			Some(source) => {
				source
			}
			None => {
				return;
			}
		};
		let font = LoadedFont::create_from_bytes(std::fs::read(&source.path).expect("font file")).expect("font");
		let face = font.get_shaping_face();
		if !has_bitmap_strike(face, 48, 48) && find_gasp_range(face, 48).is_none_or(|(_, _, _, flags)| flags != 0x0001) {
			let choice = choose_direct_write_render(face, 48.3, true);
			let expected_mode = match find_gasp_range(face, 48) {
				Some((_, _, 1, flags)) => {
					if flags & 0x0008 != 0 {
						5
					}
					else {
						4
					}
				}
				_ => {
					5
				}
			};
			assert_eq!(choice, DirectWriteRenderChoice { rendering_mode: expected_mode, measuring_mode: 0, render_size: 48.3 });
		}
		assert!(choose_direct_write_render(face, 12.3, false).render_size > 0.0);
	}
}
