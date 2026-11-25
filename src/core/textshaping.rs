//==============================================================================
// 글자 모양 짓기. (글꼴 얼굴 고르기, 양방향 정렬, 글자 모양 잇기, 글리프 래스터)
// - 원본은 브라우저 Canvas2D 가 이 일을 다 한다. (CSS 글꼴 고르기, 양방향 알고리즘, HarfBuzz 모양 짓기, Skia 래스터)
//   러스트 판은 같은 순서를 크레이트로 밟는다:
//   - 글꼴 얼굴(FontFaceDescriptors): CSS @font-face 처럼 이름 하나 아래에 굵기, 기울기, 글자 범위(unicode-range)가 다른 얼굴을 여럿 둔다.
//     고르기는 CSS Fonts 의 차례(기울기 → 굵기)를 따르고, 같은 굵기와 기울기의 얼굴들은 글자 범위로 나눠 그린다. 나중에 올린 얼굴이 먼저다.
//   - 양방향 정렬: unicode-bidi (UAX #9). 문단 방향은 굽는 쪽이 준다. (원본 canvas.direction)
//   - 모양 짓기: rustybuzz (HarfBuzz 를 옮긴 것). 아랍어 글자 잇기, 합자, 커닝, 결합 부호 자리를 브라우저처럼 잡는다.
//   - 래스터: fontdue 의 글리프 번호 래스터. 굵은 얼굴이 없으면 가짜 굵게, 기울인 얼굴이 없으면 가짜 기울임을 글리프마다 입힌다.
//     색 글리프(sbix, CBDT, COLR)는 colorglyph.rs 가 제 색으로 그린다. 크롬처럼 가짜 기울임은 입히고 가짜 굵게는 입히지 않는다.
// - 이모지 표시: VS16(FE0F), Emoji_Presentation 글자, 피부색 조정자, 국기, 키캡, 태그 묶음은 색 글리프를 가진 얼굴을 먼저 고른다. 그 밖(VS15 포함)은 색이 아닌 얼굴을 먼저.
//   크롬은 지정한 글꼴에 흑백 ❤ 가 있어도 ❤️ (FE0F) 는 색 이모지 글꼴로, 맨 ❤ 는 지정한 글꼴로 그린다. (맥 크롬에서 Canvas2D 로 확인)
// - 가짜 기울임과 가짜 굵게는 Chromium 이 Skia 에 주는 값과 같다:
//   기울기 1/4 (font_platform_data.cc 의 setSkewX(-SK_Scalar1 / 4)), 굵게는 글자 크기의 1/24 (9px 이하) ~ 1/32 (36px 이상) 두께로 윤곽을 칠해 넓힌다.
//   (Skia SkTextFormatParams.h 의 kStdFakeBoldInterpKeys / Values) 둘 다 전진폭은 그대로다.
//   가짜 굵게를 쓰는 때는 청한 굵기가 600 이상이고 고른 얼굴이 600 아래일 때다. (Blink kBoldThreshold)
// - 글꼴 바이트는 LoadedFont 가 rustybuzz 얼굴과 함께 든다. (self_cell, 얼굴이 바이트를 빌린다) 등록을 지우고 쓰는 곳이 없어지면 바이트도 돌려준다.
//   같은 파일의 여러 얼굴(TTC 의 굵기들, 가변 글꼴의 굵기 인스턴스)은 바이트(Rc)를 나눠 든다.
// - TTC 같은 글꼴 모음은 얼굴 번호(face_index)를 골라 읽는다. (find_collection_face_index 가 굵기와 기울기로 고른다)
// - 가변 글꼴(fvar 의 wght 축)은 굵기 인스턴스를 축 값으로 만든다. (create_from_shared_bytes 의 variation_list) 모양 짓기는 rustybuzz 가 축 값을 읽고,
//   래스터는 fontdue 가 축 값을 읽지 못하고 맥 SFNS 같은 가변 글꼴의 윤곽을 그리지 못해, 가변 글꼴은 skrifa 로 축 값을 건 윤곽을 받아 colorglyph 의 넓이 누적 래스터로 그린다.
//   (크롬도 가변 글꼴의 굵기를 축 값으로 그린다)
// - 광학 크기(opsz 축)와 자간 표(trak): 크롬은 글꼴의 opsz 축을 글자 크기로 건다. (font-optical-sizing 의 처음 값 auto, 맥 font_platform_data_mac.mm 의
//   FontPlatformDataFromCTFont, 모든 운영체제의 웹 글꼴 font_custom_platform_data.cc. 윈도우, 리눅스 시스템 글꼴에는 걸지 않는다)
//   모양 짓기는 HarfBuzz 에 ptem 으로 글자 크기를 주어 AAT trak 표의 자간을 더한다. (harfbuzz_face.cc 의 GetScaledFont, 맥은 trak 이 있고 sbix 가 없는
//   글꼴의 전진폭을 CoreText 대신 HarfBuzz 로 셈한다) 러스트 판도 굽는 크기(원본은 폰트 문자열의 px 를 굽는 배율로 키운다)로 opsz 를 걸고 trak 을 더한다.
//   trak 은 rustybuzz 가 아니라 여기서 HarfBuzz 15 (hb-aat-layout-trak-table.hh) 처럼 더한다. 크기 사이는 반올림하지 않고 곧게 잇고, 표의 처음과 끝 밖은
//   끝 값이며, 글자 묶음의 첫 글리프 전진폭에만 더한다. STAT 표가 없는 글꼴에는 걸지 않는다.
//   (rustybuzz 0.20 은 글꼴 단위 정수로 반올림하고 표 밖을 늘여 잇고 x_offset 에도 반을 더하고 STAT 을 보지 않는다)
//   맥 시스템 UI 글꼴(SFNS, opsz 17 ~ 96, 처음 28)이 16px 에서 opsz 17 과 trak -40 단위로 크롬과 같아진다. (색 이모지는 CoreText 의 전진폭 그대로 둔다)
// - 낱말마다 모양 짓기: 크롬 캔버스(platform/fonts/plain_text_node.cc 의 SegmentWord)는 글을 낱말로 나눠 따로 모양 짓는다. 빈칸을 낀 커닝과 합자가 없다.
//   빈칸(U+0020), 탭, ZWSP 는 하나씩, 한중일 글자와 이모지는 글자마다 따로다. (find_chrome_word_end) 캔버스의 탭, 줄바꿈, 폼 피드, 캐리지 리턴은 빈칸으로 바꾼다.
// - 맥: 파일에서 읽은 색 이모지 글꼴(sbix)과 윤곽 표가 없는 글꼴(PingFang 의 hvgl)은 CoreText 로 그린다. (platform/desktop/coretextglyph.rs, 원본 크롬 맥도 CoreText 로 그린다)
//   색 이모지의 전진폭은 CoreText 가 작은 크기에서 키워 그리는 비를 곱하고 정수 픽셀로 반올림한다. (16px 에서 20, 13px 에서 16, 크롬 measureText 와 같다)
//==============================================================================
use crate::core::colorglyph;
use std::rc::Rc;
use unicode_bidi::{BidiInfo, Level};
use unicode_properties::{EmojiStatus, GeneralCategory, UnicodeEmoji, UnicodeGeneralCategory};
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

// 가짜 기울임의 기울기. (기준선에서 1 픽셀 올라갈 때 오른쪽으로 밀리는 픽셀, Chromium setSkewX(-1/4))
pub const SYNTHETIC_ITALIC_SKEW: f32 = 0.25;

// 가짜 굵게 두께 표. (글자 크기 → 크기에 곱할 비율, Skia kStdFakeBoldInterpKeys / Values)
const SYNTHETIC_BOLD_SIZE_KEYS: [f32; 2] = [9.0, 36.0];
const SYNTHETIC_BOLD_SIZE_RATIOS: [f32; 2] = [1.0 / 24.0, 1.0 / 32.0];

// 굵게로 보는 굵기의 문턱. (Blink kBoldThreshold)
pub const BOLD_WEIGHT_THRESHOLD: u16 = 600;

// 브라우저 시스템 글꼴로 그리는 조각의 얼굴 자리. (웹에서 올린 어느 글꼴에도 없는 글자, ShapedGlyph.face_index)
pub const SYSTEM_FACE_INDEX: usize = usize::MAX;

// 시스템 글꼴 조각 재기. (조각 글, 오른쪽에서 왼쪽인지 → 전진폭 픽셀)
pub type SystemFontMeasure<'a> = &'a dyn Fn(&str, bool) -> f32;

//==============================================================================
// 글 방향. (원본 Canvas2D direction 의 실제 값 "ltr" / "rtl")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapingDirection {
	LeftToRight,
	RightToLeft,
}

//==============================================================================
// 글꼴 얼굴 서술자. (원본 FontFace 의 descriptors: weight, style, unicodeRange)
// - 굵기는 범위로 든다. (가변 글꼴 "100 900" 처럼, 한 값이면 처음과 끝이 같다)
// - 글자 범위가 비었으면 모든 글자다.
// - source_key 는 같은 이름 아래에서 얼굴을 가리키는 열쇠다. (보통 파일 경로, 차례 정하기와 지우기에 쓴다)
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFaceDescriptors {
	pub minimum_weight: u16,
	pub maximum_weight: u16,
	pub is_italic: bool,
	pub unicode_range_list: Vec<(u32, u32)>,
	pub source_key: String,
}

impl FontFaceDescriptors {
	//==============================================================================
	// 생성. (굵기 400, 기울이지 않음, 모든 글자. 원본 서술자를 주지 않은 FontFace 와 같다)
	//==============================================================================
	pub fn new() -> FontFaceDescriptors {
		FontFaceDescriptors {
			minimum_weight: 400,
			maximum_weight: 400,
			is_italic: false,
			unicode_range_list: Vec::new(),
			source_key: String::new(),
		}
	}

	//==============================================================================
	// 굵은 얼굴인지로 생성. (굵으면 700, 아니면 400)
	//==============================================================================
	pub fn create_with_bold(is_bold: bool) -> FontFaceDescriptors {
		let mut font_face_descriptors = FontFaceDescriptors::new();
		if is_bold {
			font_face_descriptors.set_weight(700);
		}
		font_face_descriptors
	}

	//==============================================================================
	// CSS 서술자 글로 생성. (원본 { weight, style, unicodeRange }, 빈 글은 기본값)
	// - weight: "400", "bold", "100 900". style: "normal", "italic", "oblique". unicode_range: "U+0000-00FF, U+0131, U+4??"
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
	// 굵기 한 값 설정.
	//==============================================================================
	pub fn set_weight(&mut self, weight: u16) {
		self.minimum_weight = weight;
		self.maximum_weight = weight;
	}

	//==============================================================================
	// CSS 굵기 글로 설정. ("normal", "bold", "500", "100 900", 모르는 글은 그대로 둔다)
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
	// 글자가 글자 범위에 드는지. (범위가 비었으면 늘 든다)
	//==============================================================================
	pub fn contains_character(&self, character: char) -> bool {
		if self.unicode_range_list.is_empty() {
			return true;
		}
		let code_point = character as u32;
		self.unicode_range_list.iter().any(|(start, end)| code_point >= *start && code_point <= *end)
	}

	//==============================================================================
	// 글에 글자 범위에 드는 글자가 있는지.
	//==============================================================================
	pub fn contains_any_character(&self, text: &str) -> bool {
		text.chars().any(|character| self.contains_character(character))
	}
}

impl Default for FontFaceDescriptors {
	fn default() -> FontFaceDescriptors {
		FontFaceDescriptors::new()
	}
}

//==============================================================================
// CSS 굵기 값 하나 읽기. ("normal" 400, "bold" 700, 숫자 1 ~ 1000)
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
// unicode-range 글 → 코드 포인트 범위 목록. ("U+0-FF, U+4??" → [(0, 255), (1024, 1279)], 비었으면 빈 목록 = 모든 글자)
// - 원본 textstringtexturecache.js, uifont.js 의 parseUnicodeRange 와 같다.
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
// 글꼴 바이트와 그것을 빌린 모양 짓기 얼굴. (바이트를 같은 값 안에 들어 얼굴이 바이트보다 오래 살지 않는다)
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
// 읽은 글꼴 하나. (래스터용 fontdue, 모양 짓기용 rustybuzz 얼굴과 그 바이트, 색 글리프 표가 있는지)
//==============================================================================
pub struct LoadedFont {
	pub raster_font: fontdue::Font,
	shaping_face_cell: ShapingFaceCell,
	pub units_per_em: f32,
	pub has_color_tables: bool,
	// 글꼴 모음 안의 얼굴 번호. (TTC 가 아니면 0)
	face_index: u32,
	// 건 축 값. (가변 글꼴의 인스턴스, 비었으면 기본 인스턴스)
	variation_list: Vec<rustybuzz::Variation>,
	// 가변 글꼴인지. (fvar 표가 있다, 래스터와 글리프 상자를 윤곽에서 직접 셈한다)
	is_variable_font: bool,
	// 글꼴 파일 경로. (시스템 글꼴처럼 파일에서 읽었으면, 맥 CoreText 가 같은 파일로 그린다)
	source_path: Option<std::path::PathBuf>,
	// 맥 CoreText 로 그리는지. (파일 경로가 있고 색 이모지(sbix)이거나 윤곽 표(glyf, CFF, CFF2)가 없는 글꼴)
	is_core_text_raster: bool,
	// opsz 축의 처음과 끝. (축이 없으면 None)
	optical_size_range: Option<(f32, f32)>,
	// opsz 축을 글자 크기로 거는지. (크롬 font-optical-sizing auto, 윈도우와 리눅스의 시스템 글꼴은 끈다)
	is_optical_sizing_auto: bool,
	// trak 표의 가로 자간. (크기 목록과 자간 값(track) → 크기마다의 값(글꼴 단위), 표가 없으면 None)
	horizontal_track_table: Option<(Vec<f32>, Vec<(f32, Vec<i16>)>)>,
}

impl LoadedFont {
	//==============================================================================
	// 풀린 글꼴 바이트(ttf, otf, ttc)로 읽기. (모음이면 0 번 얼굴)
	//==============================================================================
	pub fn create_from_bytes(font_bytes: Vec<u8>) -> Result<LoadedFont, String> {
		LoadedFont::create_from_shared_bytes(Rc::from(font_bytes), 0, &[])
	}

	//==============================================================================
	// 나눠 든 바이트의 얼굴 하나 읽기. (모음 안의 얼굴 번호, 가변 글꼴의 축 값)
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
	// trak 표의 가로 자간. (글꼴 단위, ptem 은 글자 크기 픽셀. 표나 STAT 표가 없거나 맥 색 이모지면 0. HarfBuzz 15 의 TrackData::get_tracking 에서 track 0)
	// - 크기: 표의 크기 사이는 곧게 잇고, 처음보다 작으면 첫 값, 끝보다 크면 끝 값.
	// - track: 0 과 같은 줄이 있으면 그 줄, 없으면 0 을 사이에 둔 두 줄을 곧게 잇는다.
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
	// opsz 축을 글자 크기로 걸지 정하기. (처음은 건다. 크롬은 윈도우와 리눅스의 시스템 글꼴에는 걸지 않는다)
	//==============================================================================
	pub fn with_optical_sizing_auto(mut self, is_optical_sizing_auto: bool) -> LoadedFont {
		self.is_optical_sizing_auto = is_optical_sizing_auto;
		self
	}

	//==============================================================================
	// 글자 크기에 거는 opsz 축 값. (축이 있고 거는 글꼴이면 크기를 축 범위로 자른 값, 아니면 None)
	//==============================================================================
	pub fn read_optical_size(&self, pixel_size: f32) -> Option<f32> {
		if !self.is_optical_sizing_auto {
			return None;
		}
		let (minimum_value, maximum_value) = self.optical_size_range?;
		Some(pixel_size.clamp(minimum_value, maximum_value))
	}

	//==============================================================================
	// 글자 크기의 축 값 목록. (건 축 값에 opsz 를 더한 것)
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
	// 글자 크기의 모양 짓기 얼굴. (opsz 축을 건다. trak 자간은 shape_text 가 따로 더하므로 rustybuzz 의 ptem 은 두지 않는다)
	//==============================================================================
	pub fn create_sized_shaping_face(&self, pixel_size: f32) -> rustybuzz::Face<'_> {
		let mut shaping_face = self.get_shaping_face().clone();
		if let Some(optical_size) = self.read_optical_size(pixel_size) {
			shaping_face.set_variation(rustybuzz::ttf_parser::Tag::from_bytes(b"opsz"), optical_size);
		}
		shaping_face
	}

	//==============================================================================
	// 글꼴 파일 경로 적기. (파일에서 읽은 글꼴. 맥은 색 이모지와 윤곽 표가 없는 글꼴을 CoreText 로 그리게 한다)
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
	// 맥 CoreText 로 그리는지.
	//==============================================================================
	pub fn is_core_text_raster(&self) -> bool {
		self.is_core_text_raster
	}

	//==============================================================================
	// 모양 짓기 전진폭에 곱할 비. (맥 색 이모지는 CoreText 가 작은 크기에서 키워 그리는 비, 그 밖은 1)
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
	// 전진폭을 정수 픽셀로 반올림하는지. (맥 색 이모지, 크롬 measureText 가 13px 에서 16, 17px 에서 21 처럼 정수를 준다)
	//==============================================================================
	pub fn is_integer_advance(&self) -> bool {
		self.is_core_text_raster && self.get_shaping_face().tables().sbix.is_some()
	}

	//==============================================================================
	// 맥 CoreText 에 줄 축 값. ((4 글자 태그의 수, 값) 목록)
	//==============================================================================
	#[cfg(target_os = "macos")]
	fn read_core_text_variation_list(&self, pixel_size: f32) -> Vec<(u32, f32)> {
		self.read_sized_variation_list(pixel_size).iter().map(|variation| (variation.tag.0, variation.value)).collect()
	}

	//==============================================================================
	// 모양 짓기 얼굴. (축 값을 건 얼굴, 글꼴 표도 이것으로 읽는다)
	//==============================================================================
	pub fn get_shaping_face(&self) -> &rustybuzz::Face<'_> {
		self.shaping_face_cell.borrow_dependent()
	}

	//==============================================================================
	// 글꼴 바이트. (같은 파일의 다른 얼굴이 나눠 든다)
	//==============================================================================
	pub fn get_font_bytes(&self) -> &Rc<[u8]> {
		self.shaping_face_cell.borrow_owner()
	}

	//==============================================================================
	// 글꼴 모음 안의 얼굴 번호.
	//==============================================================================
	pub fn get_face_index(&self) -> u32 {
		self.face_index
	}

	//==============================================================================
	// 축 값을 건 가변 글꼴 인스턴스인지.
	//==============================================================================
	pub fn is_variation_applied(&self) -> bool {
		!self.variation_list.is_empty()
	}

	//==============================================================================
	// 글리프를 윤곽에서 직접 그리는지. (가변 글꼴. fontdue 는 축 값을 읽지 못하고 맥 SFNS 같은 가변 글꼴의 윤곽을 그리지 못한다)
	//==============================================================================
	pub fn is_outline_raster(&self) -> bool {
		self.is_variable_font || !self.variation_list.is_empty()
	}

	//==============================================================================
	// 글리프가 덮는 픽셀 상자. (펜 원점에서 왼쪽, 아래, 너비, 높이. fontdue Metrics 의 xmin, ymin, width, height 와 같은 뜻, 비었으면 너비와 높이가 0)
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
		let metrics = self.raster_font.metrics_indexed(glyph_index, pixel_size);
		(metrics.xmin, metrics.ymin, metrics.width as i32, metrics.height as i32)
	}

	//==============================================================================
	// 글리프 윤곽 그리기. (글꼴 단위, 그 글자 크기의 축 값(opsz 포함)을 건 윤곽. 가변 글꼴은 skrifa 로, 아니면 ttf-parser 로. 윤곽이 없으면 false)
	// - ttf-parser 는 글리프마다 변형이 32 개를 넘는 가변 글꼴(맥 SFNS 같은)의 윤곽을 읽지 못한다.
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
	// 글자를 가졌는지.
	//==============================================================================
	pub fn has_character(&self, character: char) -> bool {
		self.raster_font.lookup_glyph_index(character) != 0
	}

	//==============================================================================
	// 글자를 색 글리프로 가졌는지. (sbix, CBDT, COLR)
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
// skrifa 윤곽을 ttf-parser 윤곽 받개로 넘기는 펜.
//==============================================================================
struct SkrifaOutlinePen<'builder> {
	outline_builder: &'builder mut dyn rustybuzz::ttf_parser::OutlineBuilder,
}

impl skrifa::outline::OutlinePen for SkrifaOutlinePen<'_> {
	//==============================================================================
	// 새 윤곽 시작.
	//==============================================================================
	fn move_to(&mut self, position_x: f32, position_y: f32) {
		self.outline_builder.move_to(position_x, position_y);
	}

	//==============================================================================
	// 곧은 선.
	//==============================================================================
	fn line_to(&mut self, position_x: f32, position_y: f32) {
		self.outline_builder.line_to(position_x, position_y);
	}

	//==============================================================================
	// 2 차 곡선.
	//==============================================================================
	fn quad_to(&mut self, control_x: f32, control_y: f32, position_x: f32, position_y: f32) {
		self.outline_builder.quad_to(control_x, control_y, position_x, position_y);
	}

	//==============================================================================
	// 3 차 곡선.
	//==============================================================================
	fn curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, position_x: f32, position_y: f32) {
		self.outline_builder.curve_to(first_control_x, first_control_y, second_control_x, second_control_y, position_x, position_y);
	}

	//==============================================================================
	// 윤곽 닫기.
	//==============================================================================
	fn close(&mut self) {
		self.outline_builder.close();
	}
}

//==============================================================================
// 글자 묶음이 이모지 표시인지. (UTS #51, 아니면 글 표시)
// - VS15(FE0E) 가 있으면 글 표시, VS16(FE0F) 이나 키캡(20E3)이 있으면 이모지 표시다.
// - 첫 글자가 Emoji_Presentation 이거나(국기의 지역 표시 글자 포함), 피부색 조정자나 태그 글자가 붙었거나, 이음 문자로 이은 이모지 묶음이면 이모지 표시다.
//   ❤ 처럼 Emoji 이지만 Emoji_Presentation 이 아닌 글자 하나는 글 표시다. (크롬도 맨 ❤ 는 글 글꼴로 그린다)
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
// 등록된 글꼴 얼굴 하나.
//==============================================================================
#[derive(Clone)]
pub struct RegisteredFontFace {
	pub descriptors: FontFaceDescriptors,
	pub font: Rc<LoadedFont>,
	// 올린 차례. (클수록 나중, 나중 것이 먼저 그린다)
	pub order_number: u64,
}

//==============================================================================
// 그릴 때 고른 얼굴 하나. (가짜 굵게, 가짜 기울임을 입힐지 함께 든다)
//==============================================================================
#[derive(Clone)]
pub struct ResolvedFontFace {
	pub font: Rc<LoadedFont>,
	pub descriptors: FontFaceDescriptors,
	pub is_synthetic_bold: bool,
	pub is_synthetic_italic: bool,
}

//==============================================================================
// 굵기 고르기 점수. (작을수록 먼저, CSS Fonts 4 의 차례. 굵기 범위가 청한 굵기를 품으면 (0, 0))
// - 청한 값이 400 ~ 500 이면 그 값부터 500 까지 오름, 그 아래 내림, 500 위 오름. 400 아래면 아래 내림, 위 오름. 500 위면 위 오름, 아래 내림.
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
// 글꼴 모음(TTC)에서 청한 굵기와 기울기에 가장 맞는 얼굴 번호. (모음이 아니면 0)
// - 굵기는 OS/2 의 usWeightClass, 기울기는 OS/2 와 head 의 기울임 표시로 본다. 기울기를 먼저, 그다음 굵기를 select_font_faces 와 같은 차례로 고른다.
// - PostScript 이름이 점으로 시작하는 얼굴(맥 시스템 글꼴이 안에서만 쓰는 얼굴)은 다른 얼굴이 없을 때만 고른다. 점수가 같으면 앞 번호.
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
// 가변 글꼴의 굵기 축 값. (wght 축이 있고 청한 굵기가 기본값과 다르면 축 범위로 자른 값 하나, 아니면 빈 목록)
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
// 가변 글꼴의 굵기 범위. (wght 축의 처음과 끝, 축이 없으면 None)
//==============================================================================
pub fn read_weight_axis_range(font_bytes: &[u8], face_index: u32) -> Option<(u16, u16)> {
	let face = rustybuzz::ttf_parser::Face::parse(font_bytes, face_index).ok()?;
	let weight_tag = rustybuzz::ttf_parser::Tag::from_bytes(b"wght");
	face.variation_axes().into_iter().find(|variation_axis| variation_axis.tag == weight_tag).map(|variation_axis| (variation_axis.min_value.round() as u16, variation_axis.max_value.round() as u16))
}

//==============================================================================
// 한 이름 아래의 얼굴들에서 청한 굵기와 기울기에 맞는 얼굴들 고르기. (CSS Fonts 4 의 글꼴 고르기 차례)
// - 기울기를 먼저 고른다. 기울임을 청하면 기울인 얼굴, 없으면 바른 얼굴(가짜 기울임). 바르게를 청하면 바른 얼굴, 없으면 기울인 얼굴.
// - 굵기: 청한 값이 400 ~ 500 이면 그 값부터 500 까지 오름, 그 아래 내림, 500 위 오름. 400 아래면 아래 내림, 위 오름. 500 위면 위 오름, 아래 내림.
// - 고른 굵기와 기울기의 얼굴을 모두 나중 것부터 돌려준다. (글자 범위로 나눠 그린다)
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
// 모양 지은 글리프 하나. (길이 단위는 픽셀, y 는 위가 양수)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
	pub face_index: usize,
	pub glyph_index: u16,
	pub position_x: f32,
	pub position_y: f32,
	// 시스템 글꼴 조각이면 그 조각의 글 자리. (바이트, face_index 가 SYSTEM_FACE_INDEX 일 때만)
	pub text_start: usize,
	pub text_end: usize,
	pub is_right_to_left: bool,
}

//==============================================================================
// 모양 지은 글. (글리프는 왼쪽에서 오른쪽 그리는 차례)
//==============================================================================
pub struct ShapedText {
	pub glyph_list: Vec<ShapedGlyph>,
	pub advance_width: f32,
}

//==============================================================================
// 글자 묶음에 쓸 얼굴 고르기. (묶음의 글자를 모두 가진 첫 얼굴, 없으면 첫 글자를 가진 첫 얼굴, 그것도 없으면 0)
// - 브라우저처럼 결합 부호까지 한 얼굴로 그린다. 이음 문자, 이형 선택자처럼 보이지 않는 글자는 따지지 않는다.
// - 이모지 표시 묶음은 첫 글자를 색 글리프로 가진 얼굴을, 글 표시 묶음은 첫 글자를 색이 아닌 글리프로 가진 얼굴을 같은 조건 안에서 먼저 고른다. (is_emoji_presentation)
//   차례: 글자를 모두 가지고 색이 맞는 얼굴 → 글자를 모두 가진 얼굴 → 첫 글자를 가지고 색이 맞는 얼굴 → 첫 글자를 가진 얼굴.
//==============================================================================
fn select_face_index(face_list: &[ResolvedFontFace], grapheme: &str) -> Option<usize> {
	let base_character = grapheme.chars().next()?;
	let has_character = |face: &ResolvedFontFace, character: char| -> bool {
		face.descriptors.contains_character(character) && face.font.has_character(character)
	};
	let is_ignorable = |character: char| -> bool {
		('\u{200B}'..='\u{200F}').contains(&character) || ('\u{FE00}'..='\u{FE0F}').contains(&character) || ('\u{E0100}'..='\u{E01EF}').contains(&character) || ('\u{2060}'..='\u{206F}').contains(&character)
	};
	let is_color_wanted = is_emoji_presentation(grapheme);
	let is_color_matched = |face: &ResolvedFontFace| -> bool {
		face.font.has_color_character(base_character) == is_color_wanted
	};
	if grapheme.chars().nth(1).is_some() {
		let has_all_characters = |face: &ResolvedFontFace| -> bool {
			grapheme.chars().filter(|character| !is_ignorable(*character)).all(|character| has_character(face, character))
		};
		for (face_index, face) in face_list.iter().enumerate() {
			if has_all_characters(face) && is_color_matched(face) {
				return Some(face_index);
			}
		}
		for (face_index, face) in face_list.iter().enumerate() {
			if has_all_characters(face) {
				return Some(face_index);
			}
		}
	}
	for (face_index, face) in face_list.iter().enumerate() {
		if has_character(face, base_character) && is_color_matched(face) {
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

// 크롬 IsCjkIdeographOrSymbol 의 범위. (platform/text/character_property_data.h 의 kIsCjkIdeographOrSymbolRanges, 처음과 끝)
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
// 크롬 IsCjkIdeographOrSymbol 의 낱글자. (kIsCjkIdeographOrSymbolArray 와 범위 (0x1FAC9, 0x1FACC))
const CJK_IDEOGRAPH_OR_SYMBOL_LIST: [u32; 85] = [
	0x2C7, 0x2CA, 0x2CB, 0x2D9, 0x2020, 0x2021, 0x2030, 0x203B, 0x203C, 0x2042, 0x2047, 0x2048, 0x2049, 0x2051, 0x20DD, 0x20DE, 0x2100, 0x2103, 0x2105, 0x2109,
	0x210A, 0x2113, 0x2116, 0x2121, 0x212B, 0x213B, 0x2150, 0x2151, 0x2152, 0x217F, 0x2189, 0x2307, 0x2312, 0x23CE, 0x2423, 0x25A0, 0x25A1, 0x25A2, 0x25AA, 0x25AB,
	0x25B1, 0x25B2, 0x25B3, 0x25B6, 0x25B7, 0x25BC, 0x25BD, 0x25C0, 0x25C1, 0x25C6, 0x25C7, 0x25C9, 0x25CB, 0x25CC, 0x25EF, 0x2605, 0x2606, 0x260E, 0x2616, 0x2617,
	0x26A0, 0x2713, 0x271A, 0x273F, 0x2740, 0x2756, 0x2763, 0x2B1A, 0xFE10, 0xFE11, 0xFE12, 0xFE19, 0xFF1D, 0x1F100, 0x1F200, 0x1F237, 0x1F32C, 0x1F336, 0x1F37D, 0x1F43F,
	0x1F54F, 0x1F93B, 0x1F946, 0x1FAC9, 0x1FACA,
];
// 이모지 ZWJ 묶음에 드는 글 표시 그림 문자. (크롬은 RGI 이모지 ZWJ, 수식 묶음의 Extended_Pictographic 을 한중일 기호로 본다. Emoji_Presentation 과 Emoji_Modifier_Base 밖의 것)
const EMOJI_SEQUENCE_PICTOGRAPH_LIST: [u32; 15] = [0x2620, 0x2640, 0x2642, 0x2695, 0x2696, 0x26A7, 0x2708, 0x2744, 0x2764, 0x27A1, 0x1F32B, 0x1F3F3, 0x1F441, 0x1F5E8, 0x1FACB];

//==============================================================================
// 한중일 글자나 기호인지. (크롬 unicode::IsCjkIdeographOrSymbol: 위 표, Emoji_Presentation 글자, 이모지 묶음에 드는 그림 문자)
// - 크롬은 RGI 이모지 묶음에서 그림 문자를 뽑는다. 여기서는 Emoji_Modifier_Base 와 ZWJ 묶음의 글 표시 그림 문자 표로 갈음한다. (짐작에 가까운 몫이다)
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
// 결합 부호, 수식 글자, 수식 기호인지. (일반 범주 M, Lm, Sk)
//==============================================================================
fn is_mark_or_modifier(character: char) -> bool {
	matches!(character.general_category(), GeneralCategory::NonspacingMark | GeneralCategory::SpacingMark | GeneralCategory::EnclosingMark | GeneralCategory::ModifierLetter | GeneralCategory::ModifierSymbol)
}

//==============================================================================
// 크롬 캔버스의 낱말 끝. (platform/fonts/plain_text_node.cc 의 NextWordEndIndex. text 의 바이트 자리 start 에서 시작하는 낱말의 끝 바이트 자리)
// - 빈칸(U+0020), 탭, ZWSP 는 하나씩 따로 낱말이다. is_latin1 (크롬의 8 비트 글, 모든 글자가 U+00FF 이하)이면 빈칸과 탭에서만 끊는다.
// - 그 밖: 한중일 글자나 기호가 아닌 글자로 시작하면 빈칸이나 한중일 글자(결합 부호 따위가 아닌 것) 앞에서 끊는다.
//   한중일이나 이모지로 시작하면 글자마다 끊되 결합 부호, 수식 글자, ZWJ, 이모지 성분, 그림 문자는 붙이고, 문자가 Common, Inherited 인 한중일 기호와
//   첫 글자가 Common 일 때 다음 한 글자도 붙인다.
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
// 글 모양 짓기. (양방향 정렬 → 얼굴마다 나눈 조각을 rustybuzz 로 모양 짓기 → 왼쪽부터 놓기)
// - direction 은 문단의 방향이다. (원본 canvas.direction) 문단 안의 글자 순서는 양방향 알고리즘이 정한다.
// - 줄바꿈 문자는 한 줄로 보고 그대로 모양 짓는다. (원본 fillText 도 줄을 바꾸지 않는다)
// - system_font_measure 를 주면(웹) 올린 어느 얼굴에도 없는 글자 묶음은 브라우저 시스템 글꼴 조각(SYSTEM_FACE_INDEX)으로 둔다. 원본 브라우저의 글꼴 대체와 같다.
//   이모지 표시 묶음인데 올린 얼굴에 색 글리프가 없으면 흑백 글리프가 있어도 브라우저 조각(색 이모지)으로 둔다. (크롬도 ❤️ 는 색 이모지 글꼴로 그린다)
//   주지 않으면 첫 얼굴의 빈 글리프로 그린다.
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
				unicode_buffer.guess_segment_properties();
				let sized_shaping_face = face.font.create_sized_shaping_face(pixel_size);
				let glyph_buffer = rustybuzz::shape(&sized_shaping_face, &[], unicode_buffer);
				let scale = pixel_size / face.font.units_per_em * face.font.read_advance_scale(pixel_size);
				let is_integer_advance = face.font.is_integer_advance();
				// trak 자간. (글자 묶음의 첫 글리프에 더한다, HarfBuzz 의 foreach_grapheme 자리를 묶음 번호(cluster)가 바뀌는 글리프로 본다)
				let tracking = face.font.read_horizontal_tracking(pixel_size) * scale;
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
// 가짜 굵게의 넓히는 반지름. (픽셀, Skia 의 칠하는 두께의 반)
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
// 글리프 래스터 하나. (덮인 정도, 왼쪽 위 자리는 펜 원점에서 오른쪽 xmin, 기준선에서 위로 top)
// - coverage 는 색이 아닌 글자의 덮인 정도다. 색 글리프 몫은 color_pixels 에 제 색으로 든다. (프리멀티플라이드 RGBA, 같은 크기, 없으면 None)
//==============================================================================
pub struct GlyphRaster {
	pub coverage: Vec<u8>,
	pub width: i32,
	pub height: i32,
	pub left_x: i32,
	pub top_y: i32,
	pub color_pixels: Option<Vec<u8>>,
}

//==============================================================================
// 글리프 래스터. (가짜 굵게는 넓히고, 가짜 기울임은 기준선 기준으로 민다. 색 글리프는 제 색으로, 가짜 굵게 없이)
//==============================================================================
pub fn rasterize_glyph(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32) -> GlyphRaster {
	#[cfg(target_os = "macos")]
	{
		if let Some(glyph_raster) = rasterize_core_text_glyph(face, glyph_index, pixel_size) {
			return glyph_raster;
		}
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
// 맥 CoreText 로 글리프 그리기. (CoreText 로 그리는 글꼴이 아니면 None. 색 글리프는 제 색, 그 밖은 흰색의 알파를 덮인 정도로)
// - 가짜 기울임은 색 글리프에도 입히고, 가짜 굵게는 색이 아닌 글리프에만 입힌다. (rasterize_glyph 와 같다)
//==============================================================================
#[cfg(target_os = "macos")]
fn rasterize_core_text_glyph(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32) -> Option<GlyphRaster> {
	if !face.font.is_core_text_raster() {
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
// 가짜 기울임 입히기. (기준선 위는 오른쪽, 아래는 왼쪽으로 밀고 그만큼 넓힌다. 색 그림이 있으면 채널마다 같이 민다)
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
// 덮인 정도를 원 모양으로 넓히기. (가짜 굵게, 윤곽을 바깥으로 radius 픽셀 민 것)
// - 덮인 정도를 픽셀 가운데에서 윤곽까지의 거리로 본다. (덮인 정도 - 0.5, 안이 양수)
//   이웃 픽셀의 거리에서 이웃까지의 거리를 뺀 가장 큰 값에 radius 를 더해 다시 덮인 정도로 바꾼다. 곧은 윤곽은 정확히 radius 만큼 밀린다.
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
// 덮인 정도를 가로로 기울이기. (기준선 위 줄은 오른쪽, 아래 줄은 왼쪽으로 줄 높이 × 기울기만큼 밀고, 두 픽셀 사이는 섞는다)
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
	// unicode-range 글 읽기. (원본 parseUnicodeRange 와 같은 결과)
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
	// 가짜 굵게 두께. (9px 이하 1/24, 36px 이상 1/32, 사이는 섞는다. 반지름은 그 반)
	//==============================================================================
	#[test]
	fn synthetic_bold_radius_follows_skia_table() {
		assert!((calculate_synthetic_bold_radius(6.0) - 6.0 / 24.0 * 0.5).abs() < 1e-5);
		assert!((calculate_synthetic_bold_radius(48.0) - 48.0 / 32.0 * 0.5).abs() < 1e-5);
		let middle_radius = calculate_synthetic_bold_radius(22.5);
		assert!(middle_radius > 22.5 / 32.0 * 0.5 && middle_radius < 22.5 / 24.0 * 0.5);
	}

	//==============================================================================
	// 기울이면 기준선 위는 오른쪽으로 아래는 왼쪽으로 밀린다.
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
	// 이모지 표시 판정. (UTS #51, FE0F 와 Emoji_Presentation, 국기, 키캡, 피부색은 이모지, 맨 ❤ 와 FE0E 는 글)
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
	// 얼굴 고르기: ❤️ 는 색 글리프 얼굴로, 맨 ❤ 는 흑백 얼굴로. 시스템 조각을 쓸 수 있으면(웹) 색 얼굴이 없을 때 ❤️ 는 브라우저 조각으로.
	// (운영체제가 고른 ❤ 의 흑백 대체 글꼴과 색 이모지 글꼴이 있을 때만 본다, 맥은 ZapfDingbats 와 Apple Color Emoji)
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
		let create_face = |font: LoadedFont| ResolvedFontFace { font: Rc::new(font), descriptors: FontFaceDescriptors::new(), is_synthetic_bold: false, is_synthetic_italic: false };
		let face_list = vec![create_face(text_font), create_face(color_font)];
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
	}

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
}
