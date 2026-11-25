//==============================================================================
// 글자 모양 짓기. (글꼴 얼굴 고르기, 양방향 정렬, 글자 모양 잇기, 글리프 래스터)
// - 원본은 브라우저 Canvas2D 가 이 일을 다 한다. (CSS 글꼴 고르기, 양방향 알고리즘, HarfBuzz 모양 짓기, Skia 래스터)
//   러스트 판은 같은 순서를 크레이트로 밟는다:
//   - 글꼴 얼굴(FontFaceDescriptors): CSS @font-face 처럼 이름 하나 아래에 굵기, 기울기, 글자 범위(unicode-range)가 다른 얼굴을 여럿 둔다.
//     고르기는 CSS Fonts 의 차례(기울기 → 굵기)를 따르고, 같은 굵기와 기울기의 얼굴들은 글자 범위로 나눠 그린다. 나중에 올린 얼굴이 먼저다.
//   - 양방향 정렬: unicode-bidi (UAX #9). 문단 방향은 굽는 쪽이 준다. (원본 canvas.direction)
//   - 모양 짓기: rustybuzz (HarfBuzz 를 옮긴 것). 아랍어 글자 잇기, 합자, 커닝, 결합 부호 자리를 브라우저처럼 잡는다.
//   - 래스터: fontdue 의 글리프 번호 래스터. 굵은 얼굴이 없으면 가짜 굵게, 기울인 얼굴이 없으면 가짜 기울임을 글리프마다 입힌다.
// - 가짜 기울임과 가짜 굵게는 Chromium 이 Skia 에 주는 값과 같다:
//   기울기 1/4 (font_platform_data.cc 의 setSkewX(-SK_Scalar1 / 4)), 굵게는 글자 크기의 1/24 (9px 이하) ~ 1/32 (36px 이상) 두께로 윤곽을 칠해 넓힌다.
//   (Skia SkTextFormatParams.h 의 kStdFakeBoldInterpKeys / Values) 둘 다 전진폭은 그대로다.
//   가짜 굵게를 쓰는 때는 청한 굵기가 600 이상이고 고른 얼굴이 600 아래일 때다. (Blink kBoldThreshold)
// - 글꼴 바이트는 등록할 때 한 번 'static 으로 붙잡아(Box::leak) rustybuzz 얼굴을 담아 둔다. 원본의 document.fonts 처럼 등록한 글꼴은
//   프로그램이 끝날 때까지 남는 것으로 본다. (등록을 지워도 바이트는 돌려주지 않는다)
//==============================================================================
use std::rc::Rc;
use unicode_bidi::{BidiInfo, Level};
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
// 읽은 글꼴 하나. (래스터용 fontdue, 모양 짓기용 rustybuzz 얼굴)
//==============================================================================
pub struct LoadedFont {
	pub raster_font: fontdue::Font,
	pub shaping_face: rustybuzz::Face<'static>,
	pub units_per_em: f32,
}

impl LoadedFont {
	//==============================================================================
	// 풀린 글꼴 바이트(ttf, otf, ttc)로 읽기. (바이트는 'static 으로 붙잡는다)
	//==============================================================================
	pub fn create_from_bytes(font_bytes: Vec<u8>) -> Result<LoadedFont, String> {
		let static_bytes: &'static [u8] = Box::leak(font_bytes.into_boxed_slice());
		let raster_font = fontdue::Font::from_bytes(static_bytes, fontdue::FontSettings::default()).map_err(|error| format!("폰트 읽기 실패: {}", error))?;
		let shaping_face = rustybuzz::Face::from_slice(static_bytes, 0).ok_or_else(|| String::from("폰트 읽기 실패: 모양 짓기 얼굴을 만들 수 없음"))?;
		let units_per_em = shaping_face.units_per_em() as f32;
		Ok(LoadedFont {
			raster_font,
			shaping_face,
			units_per_em,
		})
	}

	//==============================================================================
	// 글자를 가졌는지.
	//==============================================================================
	pub fn has_character(&self, character: char) -> bool {
		self.raster_font.lookup_glyph_index(character) != 0
	}
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
		let minimum_weight = face.descriptors.minimum_weight as i32;
		let maximum_weight = face.descriptors.maximum_weight as i32;
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
//==============================================================================
fn select_face_index(face_list: &[ResolvedFontFace], grapheme: &str) -> Option<usize> {
	let base_character = grapheme.chars().next()?;
	let has_character = |face: &ResolvedFontFace, character: char| -> bool {
		face.descriptors.contains_character(character) && face.font.has_character(character)
	};
	let is_ignorable = |character: char| -> bool {
		('\u{200B}'..='\u{200F}').contains(&character) || ('\u{FE00}'..='\u{FE0F}').contains(&character) || ('\u{E0100}'..='\u{E01EF}').contains(&character) || ('\u{2060}'..='\u{206F}').contains(&character)
	};
	if grapheme.chars().nth(1).is_some() {
		for (face_index, face) in face_list.iter().enumerate() {
			if grapheme.chars().filter(|character| !is_ignorable(*character)).all(|character| has_character(face, character)) {
				return Some(face_index);
			}
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

//==============================================================================
// 글 모양 짓기. (양방향 정렬 → 얼굴마다 나눈 조각을 rustybuzz 로 모양 짓기 → 왼쪽부터 놓기)
// - direction 은 문단의 방향이다. (원본 canvas.direction) 문단 안의 글자 순서는 양방향 알고리즘이 정한다.
// - 줄바꿈 문자는 한 줄로 보고 그대로 모양 짓는다. (원본 fillText 도 줄을 바꾸지 않는다)
// - system_font_measure 를 주면(웹) 올린 어느 얼굴에도 없는 글자 묶음은 브라우저 시스템 글꼴 조각(SYSTEM_FACE_INDEX)으로 둔다. 원본 브라우저의 글꼴 대체와 같다.
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
				face_index
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

	let bidi_info = BidiInfo::new(text, Some(paragraph_level));
	let mut pen_x: f32 = 0.0;
	for paragraph in &bidi_info.paragraphs {
		let (level_list, run_list) = bidi_info.visual_runs(paragraph, paragraph.range.clone());
		for run in run_list {
			let level = level_list[run.start];
			let is_right_to_left_run = level.is_rtl();
			// 얼굴이 같은 글자 묶음끼리 조각으로 나눈다. (논리 차례)
			let mut piece_list: Vec<(usize, usize, usize)> = Vec::new();
			for (start_index, face_index) in grapheme_face_list.iter().copied() {
				if start_index < run.start || start_index >= run.end {
					continue;
				}
				match piece_list.last_mut() {
					Some(last_piece) if last_piece.2 == face_index => {}
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
				unicode_buffer.push_str(&text[piece_start..piece_end]);
				unicode_buffer.set_direction(if is_right_to_left_run {
					rustybuzz::Direction::RightToLeft
				}
				else {
					rustybuzz::Direction::LeftToRight
				});
				unicode_buffer.guess_segment_properties();
				let glyph_buffer = rustybuzz::shape(&face.font.shaping_face, &[], unicode_buffer);
				let scale = pixel_size / face.font.units_per_em;
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
					pen_x += glyph_position.x_advance as f32 * scale;
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
//==============================================================================
pub struct GlyphRaster {
	pub coverage: Vec<u8>,
	pub width: i32,
	pub height: i32,
	pub left_x: i32,
	pub top_y: i32,
}

//==============================================================================
// 글리프 래스터. (가짜 굵게는 넓히고, 가짜 기울임은 기준선 기준으로 민다)
//==============================================================================
pub fn rasterize_glyph(face: &ResolvedFontFace, glyph_index: u16, pixel_size: f32) -> GlyphRaster {
	let (metrics, bitmap) = face.font.raster_font.rasterize_indexed(glyph_index, pixel_size);
	let mut glyph_raster = GlyphRaster {
		coverage: bitmap,
		width: metrics.width as i32,
		height: metrics.height as i32,
		left_x: metrics.xmin,
		top_y: metrics.ymin + metrics.height as i32,
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
		let top_extent = glyph_raster.top_y.max(0) as f32;
		let bottom_extent = (glyph_raster.height - glyph_raster.top_y).max(0) as f32;
		let left_margin = (bottom_extent * SYNTHETIC_ITALIC_SKEW).ceil() as i32 + 1;
		let right_margin = (top_extent * SYNTHETIC_ITALIC_SKEW).ceil() as i32 + 1;
		let width = glyph_raster.width + left_margin + right_margin;
		let height = glyph_raster.height;
		let mut padded = vec![0u8; (width * height) as usize];
		for position_y in 0..height {
			for position_x in 0..glyph_raster.width {
				padded[(position_y * width + position_x + left_margin) as usize] = glyph_raster.coverage[(position_y * glyph_raster.width + position_x) as usize];
			}
		}
		// 기준선은 이 비트맵의 top_y 줄이다.
		glyph_raster.coverage = shear_coverage(&padded, width, height, glyph_raster.top_y as f32, SYNTHETIC_ITALIC_SKEW);
		glyph_raster.width = width;
		glyph_raster.left_x -= left_margin;
	}
	glyph_raster
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
