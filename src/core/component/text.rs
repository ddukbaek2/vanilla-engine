//==============================================================================
// 텍스트 컴포넌트. (노드 content_size 영역 안에 정렬해 글자를 그린다)
// - 한 줄, 그리고 줄바꿈 문자나 word_wrap_width 가 있으면 여러 줄로 그린다.
// - 나눈 줄과 줄 폭은 글, 폰트 문자열, 줄바꿈 폭, 등록된 글꼴이 바뀔 때만 다시 센다.
// - 화면 밖 거르기는 정렬, 넘침, 줄바꿈 결과로 셈한 글 사각형으로 판정한다. (원본은 거르지 않는다)
// - 밑줄 / 취소선은 원본처럼 글자 색 선을 긋는다. (두께 max(1, 글자 크기 / 16), 자리는 원본 computeUnderlineOffsetY / computeStrikethroughOffsetY)
// - 줄바꿈 방식(set_wrap_mode): Word 는 공백 단위로 채우고, 넘치는 조각에 낱말이 여럿이면(공백 없이 쓰는 한중일, 태국어 등) 낱말 사이에서,
//   그래도 넘치는 낱말은 글자 묶음 단위로 나눈다. Character 는 처음부터 글자 묶음 단위. (원본 "word" / "char", 셈의 차례가 원본과 같다)
//   글자 묶음은 UAX #29(unicode-segmentation), 낱말은 ICU 사전(icu_segmenter)으로 찾는다. 원본 Intl.Segmenter 도 ICU 의 같은 규칙과 사전이다.
// - 기울임은 폰트 문자열에 italic 을 넣는다. 기울인 얼굴이 없으면 글자 캐시가 브라우저와 같은 가짜 기울임(기울기 1/4)으로 굽는다.
// - 글자 방향(set_text_direction, 원본 setTextDirection): Inherit 면 그리는 때의 Graphic 방향을 따른다. 정렬 Start / End 가 그 방향으로 풀리고,
//   글자 캐시가 그 문단 방향으로 양방향 정렬과 모양 짓기(아랍어 글자 잇기)를 한다.
// - 글자 수만큼 보이기(set_visible_character_count, 타자기 연출)는 앞에서부터 글자 묶음 단위로 자른다. (받침, 결합 부호, 이모지 묶음이 쪼개지지 않는다)
//   여러 줄이면 줄마다 남은 수만큼 그리고, 남은 수가 0 이 되면 다음 줄부터 그리지 않는다.
// - 한 줄 글을 너비에 맞추기(set_fit_width): 원본처럼 넘치면 처음 크기에서 최소 비율까지 0.1 단위로 줄여 그린다. 줄인 크기는 줄을 나눌 때(update_layout) 재므로
//   Graphic 이 있어야 안다. (get_fitted_font_size 는 마지막으로 잰 값, calculate_fitted_font_size 는 그 자리에서 잰다) 그리는 사각형은 줄인 글의 너비를 따른다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{detect_text_direction, Graphic, TextAlign, TextBaseline, TextDirection};
use unicode_segmentation::UnicodeSegmentation;
use crate::core::node::worldnode::NodeTransform;

// 폰트 이름을 정하지 않았을 때 쓰는 이름. (등록된 이름이 없으면 엔진 기본 폰트로 그린다)
const DEFAULT_FONT_FAMILY: &str = "default";

// 줄바꿈 없는 빈칸. (이 빈칸으로 이은 낱말 사이에서는 줄을 바꾸지 않는다, 원본 NO_BREAK_SPACE)
const NO_BREAK_SPACE: char = '\u{00A0}';

//==============================================================================
// 글자 묶음으로 나누기. (원본 splitGraphemes, UAX #29 확장 글자 묶음: 받침, 성조, 결합 부호, 이모지 묶음이 앞 글자에서 떨어지지 않는다)
//==============================================================================
pub fn split_graphemes(text: &str) -> Vec<&str> {
	text.graphemes(true).collect()
}

//==============================================================================
// 글자 묶음 수. (표시 글자 수가 세는 단위)
//==============================================================================
pub fn count_graphemes(text: &str) -> usize {
	text.graphemes(true).count()
}

//==============================================================================
// 앞에서 글자 묶음 count 개까지의 바이트 길이. (글이 짧으면 글 전체 길이)
//==============================================================================
pub fn find_grapheme_prefix_length(text: &str, count: usize) -> usize {
	match text.grapheme_indices(true).nth(count) {
		Some((byte_index, _)) => {
			byte_index
		}
		None => {
			text.len()
		}
	}
}

//==============================================================================
// 낱말로 나누기. (원본 splitWords, 줄바꿈 용)
// - 태국어, 라오어, 크메르어, 버마어, 한중일처럼 공백 없이 쓰는 글자도 낱말 사이를 찾는다. (ICU4X 낱말 나누기: 한중일은 사전, 동남아 글자는 LSTM)
//   브라우저(ICU4C)처럼 한글이 이어진 덩어리는 따로 한 낱말로 본다. ("Rust로" → "Rust", "로". ICU4X 는 한글과 라틴 글자를 한 낱말로 잇는다)
//   웹은 원본처럼 브라우저의 Intl.Segmenter 로 나눈다. (없는 브라우저면 원본처럼 통째로 한 조각)
// - 문장 부호처럼 낱말이 아닌 조각은 앞 낱말에 붙인다. (줄 맨 앞에 문장 부호가 오지 않게) 맨 앞의 것은 첫 낱말 앞에 붙인다.
// - 줄바꿈 없는 빈칸(U+00A0)으로 이은 낱말 사이는 나누지 않는다. (이름과 값, 숫자와 단위처럼 함께 있어야 하는 것)
//==============================================================================
pub fn split_words(text: &str) -> Vec<String> {
	let mut piece_list: Vec<String> = Vec::new();
	let mut pending_prefix = String::new();
	let mut is_glued_to_previous = false;
	for (segment, is_word_like) in segment_words(text) {
		if is_word_like {
			if is_glued_to_previous && !piece_list.is_empty() {
				if let Some(last_piece) = piece_list.last_mut() {
					last_piece.push_str(segment);
				}
			}
			else {
				piece_list.push(format!("{}{}", pending_prefix, segment));
			}
			pending_prefix.clear();
			is_glued_to_previous = false;
		}
		else if let Some(last_piece) = piece_list.last_mut() {
			last_piece.push_str(segment);
			if segment.contains(NO_BREAK_SPACE) {
				is_glued_to_previous = true;
			}
		}
		else {
			pending_prefix.push_str(segment);
		}
	}
	if !pending_prefix.is_empty() {
		piece_list.push(pending_prefix);
	}
	piece_list
}

//==============================================================================
// 한글 글자인지. (한글 음절, 자모, 호환 자모, 확장 자모)
//==============================================================================
fn is_hangul_character(character: char) -> bool {
	let code_point = character as u32;
	(0xAC00..=0xD7A3).contains(&code_point) || (0x1100..=0x11FF).contains(&code_point) || (0x3130..=0x318F).contains(&code_point) || (0xA960..=0xA97F).contains(&code_point) || (0xD7B0..=0xD7FF).contains(&code_point)
}

//==============================================================================
// 낱말 조각으로 나누기. ((조각, 낱말인지) 목록, ICU 낱말 경계에 한글 덩어리 경계를 더한다)
//==============================================================================
pub fn segment_words(text: &str) -> Vec<(&str, bool)> {
	let mut segment_list: Vec<(&str, bool)> = Vec::new();
	for (segment_start, segment_end, is_word_like) in find_word_boundaries(text) {
		let segment = &text[segment_start..segment_end];
		if !is_word_like {
			segment_list.push((segment, false));
			continue;
		}
		// 한글 덩어리와 다른 글자 사이를 나눈다. (결합 부호처럼 글자 묶음 안의 뒤 글자는 앞 글자를 따른다)
		let mut piece_start = 0;
		let mut previous_is_hangul: Option<bool> = None;
		for (grapheme_index, grapheme) in segment.grapheme_indices(true) {
			let is_hangul = grapheme.chars().next().map(is_hangul_character).unwrap_or(false);
			if let Some(previous_is_hangul) = previous_is_hangul {
				if previous_is_hangul != is_hangul {
					segment_list.push((&segment[piece_start..grapheme_index], true));
					piece_start = grapheme_index;
				}
			}
			previous_is_hangul = Some(is_hangul);
		}
		segment_list.push((&segment[piece_start..], true));
	}
	segment_list
}

//==============================================================================
// 낱말 경계 찾기. ((시작, 끝, 낱말인지) 목록, 데스크탑은 ICU4X)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn find_word_boundaries(text: &str) -> Vec<(usize, usize, bool)> {
	let word_segmenter = icu_segmenter::WordSegmenter::new_auto(icu_segmenter::options::WordBreakInvariantOptions::default());
	let mut boundary_list: Vec<(usize, usize, bool)> = Vec::new();
	let mut break_iterator = word_segmenter.segment_str(text);
	let mut previous_index = 0;
	while let Some(break_index) = break_iterator.next() {
		if break_index == 0 {
			continue;
		}
		boundary_list.push((previous_index, break_index, break_iterator.is_word_like()));
		previous_index = break_index;
	}
	boundary_list
}

#[cfg(target_arch = "wasm32")]
thread_local! {
	// 브라우저 낱말 나누개. (처음 쓸 때 만든다, 없는 브라우저면 null)
	static BROWSER_WORD_SEGMENTER: std::cell::RefCell<Option<wasm_bindgen::JsValue>> = const { std::cell::RefCell::new(None) };
}

//==============================================================================
// 낱말 경계 찾기. ((시작, 끝, 낱말인지) 목록, 웹은 원본처럼 Intl.Segmenter(granularity "word"), 없으면 통째로 한 조각)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn find_word_boundaries(text: &str) -> Vec<(usize, usize, bool)> {
	use wasm_bindgen::{JsCast, JsValue};
	let whole_text = vec![(0, text.len(), true)];
	let segmenter = BROWSER_WORD_SEGMENTER.with(|segmenter_cell| {
		let mut segmenter_slot = segmenter_cell.borrow_mut();
		if segmenter_slot.is_none() {
			let created_segmenter = (|| -> Option<JsValue> {
				let intl = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("Intl")).ok()?;
				let segmenter_class: js_sys::Function = js_sys::Reflect::get(&intl, &JsValue::from_str("Segmenter")).ok()?.dyn_into().ok()?;
				let options = js_sys::Object::new();
				js_sys::Reflect::set(&options, &JsValue::from_str("granularity"), &JsValue::from_str("word")).ok()?;
				js_sys::Reflect::construct(&segmenter_class, &js_sys::Array::of2(&JsValue::UNDEFINED, &options)).ok()
			})();
			*segmenter_slot = Some(created_segmenter.unwrap_or(JsValue::NULL));
		}
		segmenter_slot.clone().unwrap_or(JsValue::NULL)
	});
	if segmenter.is_null() {
		return whole_text;
	}
	let boundary_list = (|| -> Option<Vec<(usize, usize, bool)>> {
		let segment_function: js_sys::Function = js_sys::Reflect::get(&segmenter, &JsValue::from_str("segment")).ok()?.dyn_into().ok()?;
		let segments = segment_function.call1(&segmenter, &JsValue::from_str(text)).ok()?;
		let mut boundary_list: Vec<(usize, usize, bool)> = Vec::new();
		let mut segment_start = 0;
		for segment_data in js_sys::try_iter(&segments).ok()?? {
			let segment_data = segment_data.ok()?;
			let segment = js_sys::Reflect::get(&segment_data, &JsValue::from_str("segment")).ok()?.as_string()?;
			let is_word_like = js_sys::Reflect::get(&segment_data, &JsValue::from_str("isWordLike")).ok()?.as_bool().unwrap_or(false);
			let segment_end = (segment_start + segment.len()).min(text.len());
			boundary_list.push((segment_start, segment_end, is_word_like));
			segment_start = segment_end;
		}
		Some(boundary_list)
	})();
	boundary_list.unwrap_or(whole_text)
}

//==============================================================================
// 줄바꿈 반영 크기. (원본 measureWrappedSize 의 { width, height, lineCount })
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextWrappedSize {
	pub width: f32,
	pub height: f32,
	pub line_count: usize,
}

//==============================================================================
// 줄바꿈 방식. (원본 wrapMode "word" / "char")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextWrapMode {
	// 공백 단위로 채우고, 한 단어가 폭을 넘으면 글자 단위로 나눈다. (공백 없는 한중일 문장은 통째로 한 단어라 글자 단위가 된다)
	Word,
	// 처음부터 글자 단위로 나눈다.
	Character,
}

pub struct Text {
	component_base: ComponentBase,
	font_family: String,
	text: String,
	font_size: f32,
	text_color: Color,
	stroke_color: Color,
	stroke_width: f32,
	is_bold: bool,
	is_italic: bool,
	is_underline: bool,
	is_strikethrough: bool,
	text_align: TextAlign,
	text_baseline: TextBaseline,
	text_direction: TextDirection,
	word_wrap_width: f32,
	wrap_mode: TextWrapMode,
	line_spacing: f32,
	visible_character_count: i32,
	// 폰트 문자열. (폰트 이름, 크기, 굵게, 기울임이 바뀔 때만 다시 만든다)
	font_string: String,
	// 줄 나누기를 다시 셀지. (글이나 폰트 문자열이 바뀌면 켠다)
	is_layout_dirty: bool,
	// 마지막으로 줄을 나눈 폭과 그때의 글꼴 등록 횟수. (폭이 바뀌거나 글꼴이 새로 들어오면 다시 센다)
	layout_wrap_width: f32,
	layout_font_registration_count: u64,
	// 나눈 줄과 줄마다 잰 폭, 글꼴의 위아래 길이. (한 줄로 그릴 때는 글 전체가 한 줄)
	line_list: Vec<String>,
	line_width_list: Vec<f32>,
	layout_font_ascent: f32,
	layout_font_descent: f32,
	// 한 줄 글을 맞출 너비와 줄이는 가장 작은 비율. (너비가 0 이면 맞추지 않는다)
	fit_width: f32,
	fit_minimum_ratio: f32,
	// 그릴 때의 크기와 폰트 문자열. (맞춰 줄였으면 줄인 크기, update_layout 이 정한다)
	fitted_font_size: f32,
	draw_font_string: String,
}

impl Text {
	//==============================================================================
	// 생성. (기본값은 원본과 같다: 32px, 흰색, 가운데 정렬, 가운데 기준선, 줄 간격 1.25)
	//==============================================================================
	pub fn new() -> Text {
		let mut text = Text {
			component_base: ComponentBase::new(),
			font_family: String::from(DEFAULT_FONT_FAMILY),
			text: String::new(),
			font_size: 32.0,
			text_color: Color::white(),
			stroke_color: Color::white(),
			stroke_width: 0.0,
			is_bold: false,
			is_italic: false,
			is_underline: false,
			is_strikethrough: false,
			text_align: TextAlign::Center,
			text_baseline: TextBaseline::Middle,
			text_direction: TextDirection::Inherit,
			word_wrap_width: 0.0,
			wrap_mode: TextWrapMode::Word,
			line_spacing: 1.25,
			visible_character_count: -1,
			font_string: String::new(),
			is_layout_dirty: true,
			layout_wrap_width: 0.0,
			layout_font_registration_count: 0,
			line_list: Vec::new(),
			line_width_list: Vec::new(),
			layout_font_ascent: 0.0,
			layout_font_descent: 0.0,
			fit_width: 0.0,
			fit_minimum_ratio: 0.55,
			fitted_font_size: 32.0,
			draw_font_string: String::new(),
		};
		text.font_string = text.build_font_string();
		text.draw_font_string = text.font_string.clone();
		text
	}

	//==============================================================================
	// 글과 크기로 생성.
	//==============================================================================
	pub fn create_with_text(text: &str, font_size: f32) -> Text {
		let mut text_component = Text::new();
		text_component.set_text(text);
		text_component.set_font_size(font_size);
		text_component
	}

	//==============================================================================
	// Canvas2D font 꼴 문자열 만들기.
	//==============================================================================
	pub fn build_font_string(&self) -> String {
		self.build_font_string_with_size(self.font_size)
	}

	//==============================================================================
	// 크기를 주어 Canvas2D font 꼴 문자열 만들기. (이 글자의 폰트 이름, 굵게, 기울임으로)
	//==============================================================================
	pub fn build_font_string_with_size(&self, font_size: f32) -> String {
		let mut style_parts: Vec<String> = Vec::new();
		if self.is_italic {
			style_parts.push(String::from("italic"));
		}
		if self.is_bold {
			style_parts.push(String::from("bold"));
		}
		style_parts.push(format!("{}px", font_size));
		style_parts.push(self.font_family.clone());
		style_parts.join(" ")
	}

	//==============================================================================
	// 폰트 문자열 다시 만들기. (폰트 이름, 크기, 굵게, 기울임 세터가 부른다. 바뀌었으면 줄을 다시 센다)
	//==============================================================================
	fn update_font_string(&mut self) {
		let font_string = self.build_font_string();
		if font_string != self.font_string {
			self.draw_font_string = font_string.clone();
			self.fitted_font_size = self.font_size;
			self.font_string = font_string;
			self.is_layout_dirty = true;
		}
	}

	//==============================================================================
	// 여러 줄로 그리는지. (줄바꿈 폭이 있거나 글에 줄바꿈 문자가 있으면)
	//==============================================================================
	fn is_wrapped(&self) -> bool {
		self.word_wrap_width > 0.0 || self.text.contains('\n')
	}

	//==============================================================================
	// 나눈 줄과 줄 폭 갱신. (글, 폰트 문자열, 줄바꿈 폭, 등록된 글꼴이 그대로면 지난번 결과를 쓴다)
	//==============================================================================
	fn update_layout(&mut self, graphic: &mut Graphic, content_width: f32) {
		let is_wrapped = self.is_wrapped();
		let wrap_width = if !is_wrapped {
			0.0
		}
		else if self.word_wrap_width > 0.0 {
			self.word_wrap_width
		}
		else {
			content_width
		};
		let font_registration_count = graphic.get_text_string_texture_cache().get_font_registration_count();
		if !self.is_layout_dirty && self.layout_wrap_width == wrap_width && self.layout_font_registration_count == font_registration_count {
			return;
		}
		// 한 줄 글이 맞출 너비를 넘치면 처음 크기에서 최소 비율까지 줄인다. (원본 setFitWidth, 0.1 단위로 내림)
		let mut fitted_font_size = self.font_size;
		if !is_wrapped && self.fit_width > 0.0 && !self.text.is_empty() {
			let natural_width = graphic.measure_text_with_font_string(&self.font_string, &self.text).width;
			if natural_width > self.fit_width {
				let fit_ratio = self.fit_minimum_ratio.max(self.fit_width / natural_width);
				fitted_font_size = (self.font_size * fit_ratio * 10.0).floor() / 10.0;
			}
		}
		self.fitted_font_size = fitted_font_size;
		self.draw_font_string = if fitted_font_size == self.font_size {
			self.font_string.clone()
		}
		else {
			self.build_font_string_with_size(fitted_font_size)
		};
		graphic.set_font_string(&self.draw_font_string);
		let line_list = if is_wrapped {
			self.wrap_text_to_lines(graphic, &self.text, wrap_width)
		}
		else {
			vec![self.text.clone()]
		};
		self.line_width_list.clear();
		self.layout_font_ascent = 0.0;
		self.layout_font_descent = 0.0;
		for line_text in &line_list {
			let line_metrics = graphic.measure_text(line_text);
			self.line_width_list.push(line_metrics.width);
			self.layout_font_ascent = self.layout_font_ascent.max(line_metrics.font_ascent);
			self.layout_font_descent = self.layout_font_descent.max(line_metrics.font_descent);
		}
		self.line_list = line_list;
		self.layout_wrap_width = wrap_width;
		self.layout_font_registration_count = font_registration_count;
		self.is_layout_dirty = false;
	}

	//==============================================================================
	// 그리는 사각형 셈하기. (content 좌표. 정렬, 넘침, 줄바꿈 결과를 따르고, 글자 삐침, 외곽선, 기울임, 밑줄 몫을 넉넉히 더한다)
	//==============================================================================
	fn calculate_text_rect(&mut self, graphic: &mut Graphic, content_width: f32, content_height: f32) -> Rect {
		self.update_layout(graphic, content_width);
		let maximum_line_width = self.line_width_list.iter().fold(0.0f32, |maximum_width, line_width| maximum_width.max(*line_width));
		let text_direction = self.resolve_draw_direction(graphic, &self.text);
		let left_x = match graphic.resolve_text_align(self.text_align, text_direction) {
			TextAlign::Left => {
				0.0
			}
			TextAlign::Right => {
				content_width - maximum_line_width
			}
			_ => {
				(content_width - maximum_line_width) * 0.5
			}
		};
		let font_ascent = self.layout_font_ascent;
		let font_descent = self.layout_font_descent;
		let (top_y, bottom_y) = if self.is_wrapped() {
			// 줄마다 가운데 기준선으로 그린다. (draw_wrapped_text 와 같은 자리)
			let line_height = self.font_size * self.line_spacing;
			let line_count = self.line_list.len().max(1) as f32;
			let block_height = line_count * line_height;
			let block_top = match self.text_baseline {
				TextBaseline::Top | TextBaseline::Hanging => {
					0.0
				}
				TextBaseline::Bottom | TextBaseline::Ideographic | TextBaseline::Alphabetic => {
					content_height - block_height
				}
				TextBaseline::Middle => {
					(content_height - block_height) * 0.5
				}
			};
			let half_text_height = (font_ascent + font_descent) * 0.5;
			let first_line_y = block_top + 0.5 * line_height;
			let last_line_y = block_top + (line_count - 0.5) * line_height;
			(first_line_y.min(last_line_y) - half_text_height, first_line_y.max(last_line_y) + half_text_height)
		}
		else {
			// 기준선 자리는 draw_plain_text 와, 기준선 맞춤은 Graphic.draw_text_entry 와 같은 셈이다.
			let draw_y = match self.text_baseline {
				TextBaseline::Top | TextBaseline::Hanging => {
					0.0
				}
				TextBaseline::Bottom | TextBaseline::Ideographic | TextBaseline::Alphabetic => {
					content_height
				}
				TextBaseline::Middle => {
					content_height * 0.5
				}
			};
			let baseline_offset = match self.text_baseline {
				TextBaseline::Top => {
					font_ascent
				}
				TextBaseline::Hanging => {
					font_ascent * 0.8
				}
				TextBaseline::Middle => {
					(font_ascent - font_descent) * 0.5
				}
				TextBaseline::Ideographic | TextBaseline::Bottom => {
					-font_descent
				}
				TextBaseline::Alphabetic => {
					0.0
				}
			};
			let baseline_y = draw_y + baseline_offset;
			(baseline_y - font_ascent, baseline_y + font_descent)
		};
		let margin = self.font_size.abs() * 0.5 + self.stroke_width.max(0.0) + 2.0;
		Rect::create(left_x - margin, top_y - margin, maximum_line_width + margin * 2.0, (bottom_y - top_y).max(0.0) + margin * 2.0)
	}

	//==============================================================================
	// 폭에 맞춰 줄 나누기. (원본 wrapTextToLines, 줄바꿈 문자는 늘 지킨다, 재는 폰트는 Graphic 의 현재 폰트 문자열)
	// - Word: 공백 단위로 채운다. 넘치는 조각에 낱말이 여럿이면(공백 없이 쓰는 글) 낱말 사이에서 나눠 지금 줄의 남은 자리부터 채우고,
	//   한 낱말이 폭을 넘으면 글자 묶음 단위로 나눈다. (공백 없는 한중일 문장도 낱말 사이에서 나뉜다)
	// - Character: 처음부터 글자 묶음 단위로 나눈다.
	//==============================================================================
	pub fn wrap_text_to_lines(&self, graphic: &Graphic, text: &str, maximum_width: f32) -> Vec<String> {
		self.wrap_text_to_lines_with(text, maximum_width, &|candidate_text: &str| graphic.measure_text(candidate_text).width)
	}

	//==============================================================================
	// 폭에 맞춰 줄 나누기. (재는 함수를 받는다. wrap_text_to_lines 의 셈, Graphic 없이 시험하거나 다른 글꼴로 잴 때)
	//==============================================================================
	pub fn wrap_text_to_lines_with(&self, text: &str, maximum_width: f32, measure_width: &dyn Fn(&str) -> f32) -> Vec<String> {
		if maximum_width <= 0.0 {
			return text.split('\n').map(|paragraph| paragraph.to_string()).collect();
		}
		let mut line_list: Vec<String> = Vec::new();
		// seed_text 에 chunk_text 를 글자 묶음 단위로 이어 붙이며 넘칠 때마다 줄을 확정한다.
		let append_by_grapheme = |line_list: &mut Vec<String>, chunk_text: &str, seed_text: &str| -> String {
			let mut current_line = seed_text.to_string();
			for grapheme in chunk_text.graphemes(true) {
				let candidate = format!("{}{}", current_line, grapheme);
				if !current_line.is_empty() && measure_width(&candidate) > maximum_width {
					line_list.push(std::mem::replace(&mut current_line, grapheme.to_string()));
				}
				else {
					current_line = candidate;
				}
			}
			current_line
		};
		for paragraph in text.split('\n') {
			if paragraph.is_empty() {
				line_list.push(String::new());
				continue;
			}
			if self.wrap_mode == TextWrapMode::Character {
				let last_line = append_by_grapheme(&mut line_list, paragraph, "");
				line_list.push(last_line);
				continue;
			}
			let mut current_line = String::new();
			for word in paragraph.split(' ') {
				let candidate = if current_line.is_empty() {
					word.to_string()
				}
				else {
					format!("{} {}", current_line, word)
				};
				if measure_width(&candidate) <= maximum_width {
					current_line = candidate;
					continue;
				}
				// 공백 없이 이어 쓴 조각(태국어, 한중일 등)에 낱말이 여럿이면, 낱말 사이에서 나눠 지금 줄의 남은 자리부터 채운다.
				let word_piece_list = split_words(word);
				if word_piece_list.len() > 1 {
					let mut is_first_piece = true;
					for word_piece in &word_piece_list {
						let separator = if is_first_piece && !current_line.is_empty() {
							" "
						}
						else {
							""
						};
						is_first_piece = false;
						let piece_candidate = format!("{}{}{}", current_line, separator, word_piece);
						if measure_width(&piece_candidate) <= maximum_width {
							current_line = piece_candidate;
							continue;
						}
						if !current_line.is_empty() {
							line_list.push(std::mem::take(&mut current_line));
						}
						current_line = if measure_width(word_piece) <= maximum_width {
							word_piece.clone()
						}
						else {
							append_by_grapheme(&mut line_list, word_piece, "")
						};
					}
					continue;
				}
				if !current_line.is_empty() {
					line_list.push(std::mem::take(&mut current_line));
				}
				if measure_width(word) <= maximum_width {
					current_line = word.to_string();
				}
				else {
					current_line = append_by_grapheme(&mut line_list, word, "");
				}
			}
			line_list.push(current_line);
		}
		line_list
	}

	//==============================================================================
	// Graphic 없이 나눈 줄 목록. (지금 글과 폰트 문자열, 폭으로 나눈다, 글꼴이 없어 잴 수 없으면 None)
	// - 그리기 전에 줄 수가 필요한 레이아웃(창의 내용 놓기 같은)이 쓴다. (textstringtexturecache::measure_text_without_graphic)
	//==============================================================================
	pub fn wrap_lines_without_graphic(&self, maximum_width: f32) -> Option<Vec<String>> {
		crate::core::textstringtexturecache::measure_text_without_graphic(&self.font_string, "")?;
		let font_string = self.font_string.clone();
		Some(self.wrap_text_to_lines_with(&self.text, maximum_width, &|line_text: &str| crate::core::textstringtexturecache::measure_text_without_graphic(&font_string, line_text).map(|text_metrics| text_metrics.width).unwrap_or(0.0)))
	}

	//==============================================================================
	// 줄바꿈 반영 크기 재기. (원본 measureWrappedSize, 레이아웃 용. 줄바꿈 폭이 없으면 줄바꿈 문자로만 나눈다)
	//==============================================================================
	pub fn measure_wrapped_size(&self, graphic: &mut Graphic) -> TextWrappedSize {
		if self.text.is_empty() {
			return TextWrappedSize::default();
		}
		let line_height = self.font_size * self.line_spacing;
		if self.word_wrap_width <= 0.0 && !self.text.contains('\n') {
			return TextWrappedSize {
				width: self.measure_text(graphic, &self.text, self.font_size),
				height: line_height,
				line_count: 1,
			};
		}
		let wrap_width = if self.word_wrap_width > 0.0 {
			self.word_wrap_width
		}
		else {
			f32::INFINITY
		};
		graphic.push_state();
		graphic.set_font_string(&self.font_string);
		let line_list = self.wrap_text_to_lines(graphic, &self.text, wrap_width);
		graphic.pop_state();
		let maximum_line_width = line_list.iter().fold(0.0f32, |maximum_width, line_text| maximum_width.max(self.measure_text(graphic, line_text, self.font_size)));
		TextWrappedSize {
			width: maximum_line_width,
			height: line_list.len() as f32 * line_height,
			line_count: line_list.len(),
		}
	}

	//==============================================================================
	// 글의 자연 크기. (원본 getIntrinsicContentSize: 한 줄 너비와 글자 크기, 글이 없으면 (0, 글자 크기), 꺼져 있으면 (-1, -1))
	//==============================================================================
	pub fn calculate_intrinsic_content_size(&self, graphic: &Graphic) -> Vector2 {
		if !self.component_base.is_enable() {
			return Vector2::create(-1.0, -1.0);
		}
		if self.text.is_empty() {
			return Vector2::create(0.0, self.font_size);
		}
		Vector2::create(self.measure_text(graphic, &self.text, self.font_size), self.font_size)
	}

	//==============================================================================
	// 글이 차지하는 영역. (원본 Text.calculateTextBounds: 굵게, 기울임 없이 잰 너비와 실제 글리프의 위아래 높이)
	//==============================================================================
	pub fn calculate_text_bounds(graphic: &Graphic, text_component: &Text) -> Rect {
		let font_string = crate::core::component::richtext::build_font_string(&text_component.font_family, text_component.font_size, false, false);
		let text_metrics = graphic.measure_text_with_font_string(&font_string, &text_component.text);
		Rect::create(0.0, 0.0, text_metrics.width, text_metrics.actual_bounding_box_ascent + text_metrics.actual_bounding_box_descent)
	}

	//==============================================================================
	// 그릴 때의 실제 방향. (이 글자의 방향, Inherit 면 Graphic 의 방향으로, Auto 는 글의 첫 강한 글자로. Graphic 은 바꾸지 않는다)
	//==============================================================================
	fn resolve_draw_direction(&self, graphic: &Graphic, text: &str) -> TextDirection {
		let base_direction = if self.text_direction != TextDirection::Inherit {
			self.text_direction
		}
		else {
			graphic.get_text_direction()
		};
		match base_direction {
			TextDirection::Rtl => {
				TextDirection::Rtl
			}
			TextDirection::Auto => {
				detect_text_direction(text)
			}
			_ => {
				TextDirection::Ltr
			}
		}
	}

	//==============================================================================
	// 정렬에 맞는 가로 위치.
	//==============================================================================
	fn calculate_draw_x(&self, graphic: &Graphic, content_width: f32, text_direction: TextDirection) -> (f32, TextAlign) {
		let text_align = graphic.resolve_text_align(self.text_align, text_direction);
		let draw_x = match text_align {
			TextAlign::Left => {
				0.0
			}
			TextAlign::Right => {
				content_width
			}
			_ => {
				content_width * 0.5
			}
		};
		(draw_x, text_align)
	}

	//==============================================================================
	// 밑줄 시작 x. (실제 정렬 기준, 원본 computeUnderlineStartX)
	//==============================================================================
	fn calculate_underline_start_x(&self, draw_x: f32, text_width: f32, text_align: TextAlign) -> f32 {
		match text_align {
			TextAlign::Left | TextAlign::Start => {
				draw_x
			}
			TextAlign::Right | TextAlign::End => {
				draw_x - text_width
			}
			_ => {
				draw_x - text_width * 0.5
			}
		}
	}

	//==============================================================================
	// 밑줄 y 오프셋. (기준선에서 아래로, 원본 computeUnderlineOffsetY)
	//==============================================================================
	fn calculate_underline_offset_y(&self, font_size: f32) -> f32 {
		match self.text_baseline {
			TextBaseline::Top | TextBaseline::Hanging => {
				font_size + 2.0
			}
			TextBaseline::Middle => {
				font_size * 0.5 + 2.0
			}
			_ => {
				4.0
			}
		}
	}

	//==============================================================================
	// 취소선 y 오프셋. (원본 computeStrikethroughOffsetY)
	//==============================================================================
	fn calculate_strikethrough_offset_y(&self, font_size: f32) -> f32 {
		match self.text_baseline {
			TextBaseline::Top | TextBaseline::Hanging => {
				font_size * 0.55
			}
			TextBaseline::Middle => {
				0.0
			}
			_ => {
				-font_size * 0.35
			}
		}
	}

	//==============================================================================
	// 가로 선 긋기. (밑줄 / 취소선, 원본 strokeHorizontalLine. 선 색은 미리 set_stroke_color)
	//==============================================================================
	fn draw_horizontal_line(&self, graphic: &mut Graphic, x: f32, y: f32, width: f32, line_width: f32) {
		graphic.draw_line(&[Vector2::create(x, y), Vector2::create(x + width, y)], line_width);
	}

	//==============================================================================
	// 밑줄 / 취소선 두께. (원본과 같이 max(1, 글자 크기 / 16))
	//==============================================================================
	fn calculate_decoration_line_width(&self) -> f32 {
		(self.font_size / 16.0).max(1.0)
	}

	//==============================================================================
	// 한 줄 출력. (text_width 는 밑줄 / 취소선 길이, 꾸밈이 없으면 쓰지 않는다)
	//==============================================================================
	fn draw_plain_text(&self, graphic: &mut Graphic, content_width: f32, content_height: f32, text: &str, text_width: f32) {
		// 문단의 방향은 그리는 글로 정한다. (Start / End 정렬도 그 방향을 따른다)
		let text_direction = self.resolve_draw_direction(graphic, text);
		graphic.set_text_direction(text_direction);
		let (draw_x, text_align) = self.calculate_draw_x(graphic, content_width, text_direction);
		let draw_y = match self.text_baseline {
			TextBaseline::Top | TextBaseline::Hanging => {
				0.0
			}
			TextBaseline::Bottom | TextBaseline::Ideographic | TextBaseline::Alphabetic => {
				content_height
			}
			TextBaseline::Middle => {
				content_height * 0.5
			}
		};

		graphic.set_font_string(&self.draw_font_string);
		graphic.set_text_align(text_align);
		graphic.set_text_baseline(self.text_baseline);

		if self.stroke_width > 0.0 {
			graphic.set_stroke_color(self.stroke_color);
			graphic.draw_stroke_text(text, draw_x, draw_y, self.stroke_width);
		}
		graphic.set_fill_color(self.text_color);
		graphic.draw_fill_text(text, draw_x, draw_y);

		if self.is_underline || self.is_strikethrough {
			let start_x = self.calculate_underline_start_x(draw_x, text_width, text_align);
			let line_width = self.calculate_decoration_line_width();
			graphic.set_stroke_color(self.text_color);
			if self.is_underline {
				let underline_y = draw_y + self.calculate_underline_offset_y(self.fitted_font_size);
				self.draw_horizontal_line(graphic, start_x, underline_y, text_width, line_width);
			}
			if self.is_strikethrough {
				let strikethrough_y = draw_y + self.calculate_strikethrough_offset_y(self.fitted_font_size);
				self.draw_horizontal_line(graphic, start_x, strikethrough_y, text_width, line_width);
			}
		}
	}

	//==============================================================================
	// 여러 줄 출력. (update_layout 으로 나눈 줄을 그린다)
	//==============================================================================
	fn draw_wrapped_text(&self, graphic: &mut Graphic, content_width: f32, content_height: f32) {
		graphic.set_font_string(&self.font_string);

		let line_list = &self.line_list;
		let line_height = self.font_size * self.line_spacing;
		let block_height = line_list.len() as f32 * line_height;
		let block_top = match self.text_baseline {
			TextBaseline::Top | TextBaseline::Hanging => {
				0.0
			}
			TextBaseline::Bottom | TextBaseline::Ideographic | TextBaseline::Alphabetic => {
				content_height - block_height
			}
			TextBaseline::Middle => {
				(content_height - block_height) * 0.5
			}
		};

		// 문단의 방향은 글 전체로 정해 모든 줄에 쓴다. (Start / End 정렬도 그 방향을 따른다)
		let text_direction = self.resolve_draw_direction(graphic, &self.text);
		graphic.set_text_direction(text_direction);
		let (draw_x, text_align) = self.calculate_draw_x(graphic, content_width, text_direction);
		graphic.set_text_align(text_align);
		graphic.set_text_baseline(TextBaseline::Middle);

		// 보일 글자 수가 정해졌으면 줄마다 남은 수만큼만 그린다. (나눈 줄의 글자 묶음 수로 센다)
		let mut remain_count = self.visible_character_count;
		for (line_index, full_line_text) in line_list.iter().enumerate() {
			let mut line_text = full_line_text.as_str();
			if remain_count >= 0 {
				if remain_count == 0 {
					break;
				}
				let line_character_count = count_graphemes(full_line_text) as i32;
				if line_character_count > remain_count {
					line_text = &full_line_text[..find_grapheme_prefix_length(full_line_text, remain_count as usize)];
				}
				remain_count = (remain_count - line_character_count).max(0);
			}
			if line_text.is_empty() {
				continue;
			}
			let line_y = block_top + (line_index as f32 + 0.5) * line_height;
			if self.stroke_width > 0.0 {
				graphic.set_stroke_color(self.stroke_color);
				graphic.draw_stroke_text(line_text, draw_x, line_y, self.stroke_width);
			}
			graphic.set_fill_color(self.text_color);
			graphic.draw_fill_text(line_text, draw_x, line_y);

			// 밑줄 / 취소선. (원본처럼 밑줄은 줄 가운데에서 글자 크기 반과 선 두께만큼 아래, 취소선은 줄 가운데)
			if self.is_underline || self.is_strikethrough {
				let line_width = if line_text.len() == full_line_text.len() {
					self.line_width_list.get(line_index).copied().unwrap_or(0.0)
				}
				else {
					graphic.measure_text(line_text).width
				};
				let start_x = self.calculate_underline_start_x(draw_x, line_width, text_align);
				let decoration_line_width = self.calculate_decoration_line_width();
				graphic.set_stroke_color(self.text_color);
				if self.is_underline {
					self.draw_horizontal_line(graphic, start_x, line_y + self.font_size * 0.5 + decoration_line_width, line_width, decoration_line_width);
				}
				if self.is_strikethrough {
					self.draw_horizontal_line(graphic, start_x, line_y, line_width, decoration_line_width);
				}
			}
		}
	}

	//==============================================================================
	// 폰트 이름 설정. (Graphic 에 등록한 이름, 없으면 기본 폰트)
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		if self.font_family != font_family {
			self.font_family = font_family.to_string();
			self.update_font_string();
		}
	}

	//==============================================================================
	// 폰트 이름 반환.
	//==============================================================================
	pub fn get_font_family(&self) -> &str {
		&self.font_family
	}

	//==============================================================================
	// 글 설정.
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		if self.text != text {
			self.text = text.to_string();
			self.is_layout_dirty = true;
		}
	}

	//==============================================================================
	// 글 반환.
	//==============================================================================
	pub fn get_text(&self) -> &str {
		&self.text
	}

	//==============================================================================
	// 글자 크기 설정. (픽셀)
	//==============================================================================
	pub fn set_font_size(&mut self, font_size: f32) {
		if self.font_size != font_size {
			self.font_size = font_size;
			self.update_font_string();
		}
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn get_font_size(&self) -> f32 {
		self.font_size
	}

	//==============================================================================
	// 한 줄 글을 너비에 맞추기 설정. (넘치면 처음 크기에서 최소 비율까지 줄여 그린다, 원본 setFitWidth)
	// - 칸을 넓힐 수 없는 곳(단추, 줄 이름, 제목)에서 언어마다 길이가 다른 글이 넘칠 때 쓴다.
	// - 정한 크기(get_font_size)는 그대로 두고 그릴 때의 크기만 줄인다. 최소 비율로도 넘치면 넘친 채로 그리고, 화면 밖 거르기도 넘친 너비까지 본다.
	// - 줄을 바꾸는 글은 맞추지 않는다. width 가 0 이하면 맞추지 않는다. (기본) minimum_ratio 는 0 ~ 1 (원본 기본 0.55)
	//==============================================================================
	pub fn set_fit_width(&mut self, width: f32, minimum_ratio: f32) {
		let fit_width = width.max(0.0);
		let fit_minimum_ratio = math::clamp(minimum_ratio, 0.0, 1.0);
		if self.fit_width != fit_width || self.fit_minimum_ratio != fit_minimum_ratio {
			self.fit_width = fit_width;
			self.fit_minimum_ratio = fit_minimum_ratio;
			self.is_layout_dirty = true;
		}
	}

	//==============================================================================
	// 맞출 너비 반환. (0 이면 맞추지 않는다)
	//==============================================================================
	pub fn get_fit_width(&self) -> f32 {
		self.fit_width
	}

	//==============================================================================
	// 맞출 때 줄이는 가장 작은 비율 반환.
	//==============================================================================
	pub fn get_fit_minimum_ratio(&self) -> f32 {
		self.fit_minimum_ratio
	}

	//==============================================================================
	// 그릴 때의 크기 반환. (마지막으로 그리거나 잰 때의 값, 맞춰 줄였으면 줄인 크기)
	//==============================================================================
	pub fn get_fitted_font_size(&self) -> f32 {
		if self.is_layout_dirty {
			return self.font_size;
		}
		self.fitted_font_size
	}

	//==============================================================================
	// 그릴 때의 크기를 그 자리에서 재어 반환. (맞출 너비를 넘치면 줄인 크기)
	//==============================================================================
	pub fn calculate_fitted_font_size(&mut self, graphic: &mut Graphic) -> f32 {
		let content_width = self.layout_wrap_width;
		self.update_layout(graphic, content_width);
		self.fitted_font_size
	}

	//==============================================================================
	// 글의 너비 재기. (이 글자의 폰트 이름, 굵게, 기울임과 주어진 크기로, 원본 measureText)
	// - 글자 캐시가 폰트 문자열과 글이 같으면 다시 재지 않는다. (TextStringTextureCache.measure_text)
	//==============================================================================
	pub fn measure_text(&self, graphic: &Graphic, text: &str, font_size: f32) -> f32 {
		if text.is_empty() {
			return 0.0;
		}
		let font_string = self.build_font_string_with_size(font_size);
		graphic.measure_text_with_font_string(&font_string, text).width
	}

	//==============================================================================
	// 글자 색 설정.
	//==============================================================================
	pub fn set_text_color(&mut self, color: impl Into<Color>) {
		self.text_color = color.into();
	}

	//==============================================================================
	// 글자 색 반환.
	//==============================================================================
	pub fn get_text_color(&self) -> Color {
		self.text_color
	}

	//==============================================================================
	// 외곽선 색 설정.
	//==============================================================================
	pub fn set_stroke_color(&mut self, color: impl Into<Color>) {
		self.stroke_color = color.into();
	}

	//==============================================================================
	// 외곽선 색 반환.
	//==============================================================================
	pub fn get_stroke_color(&self) -> Color {
		self.stroke_color
	}

	//==============================================================================
	// 외곽선 두께 설정. (0 이면 외곽선 없음)
	//==============================================================================
	pub fn set_stroke_width(&mut self, stroke_width: f32) {
		self.stroke_width = stroke_width;
	}

	//==============================================================================
	// 외곽선 두께 반환.
	//==============================================================================
	pub fn get_stroke_width(&self) -> f32 {
		self.stroke_width
	}

	//==============================================================================
	// 굵게 설정.
	//==============================================================================
	pub fn set_bold(&mut self, is_bold: bool) {
		if self.is_bold != is_bold {
			self.is_bold = is_bold;
			self.update_font_string();
		}
	}

	//==============================================================================
	// 굵게 여부.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.is_bold
	}

	//==============================================================================
	// 기울임 설정. (폰트 문자열에 italic 을 넣는다)
	//==============================================================================
	pub fn set_italic(&mut self, is_italic: bool) {
		if self.is_italic != is_italic {
			self.is_italic = is_italic;
			self.update_font_string();
		}
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn is_italic(&self) -> bool {
		self.is_italic
	}

	//==============================================================================
	// 밑줄 설정.
	//==============================================================================
	pub fn set_underline(&mut self, is_underline: bool) {
		self.is_underline = is_underline;
	}

	//==============================================================================
	// 밑줄 여부.
	//==============================================================================
	pub fn is_underline(&self) -> bool {
		self.is_underline
	}

	//==============================================================================
	// 취소선 설정.
	//==============================================================================
	pub fn set_strikethrough(&mut self, is_strikethrough: bool) {
		self.is_strikethrough = is_strikethrough;
	}

	//==============================================================================
	// 취소선 여부.
	//==============================================================================
	pub fn is_strikethrough(&self) -> bool {
		self.is_strikethrough
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		self.text_align = text_align;
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn get_text_align(&self) -> TextAlign {
		self.text_align
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: TextBaseline) {
		self.text_baseline = text_baseline;
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn get_text_baseline(&self) -> TextBaseline {
		self.text_baseline
	}

	//==============================================================================
	// 글자 방향 설정. (Inherit | Ltr | Rtl | Auto, 원본 setTextDirection)
	// - Inherit 면 그리는 때의 Graphic 방향을 따른다. (기본)
	// - 정렬 Start / End 가 이 방향에 따라 왼쪽, 오른쪽이 된다. (오른쪽에서 왼쪽 언어의 화면은 Rtl 과 Start)
	//==============================================================================
	pub fn set_text_direction(&mut self, text_direction: TextDirection) {
		self.text_direction = text_direction;
	}

	//==============================================================================
	// 글자 방향 반환.
	//==============================================================================
	pub fn get_text_direction(&self) -> TextDirection {
		self.text_direction
	}

	//==============================================================================
	// 마지막으로 나눈 줄의 수. (그린 뒤 글, 폰트, 줄바꿈 폭이 그대로일 때만, 아니면 None)
	// - 줄은 그릴 때 Graphic 의 글꼴로 나눈다. Graphic 이 없는 레이아웃(창의 내용 놓기 같은)이 그린 결과를 읽어 다시 놓을 때 쓴다.
	// - 줄바꿈 폭(set_word_wrap_width)을 주지 않은 여러 줄 글은 노드 폭으로 나누므로 None 이다.
	//==============================================================================
	pub fn get_measured_line_count(&self) -> Option<usize> {
		if self.is_layout_dirty || self.line_list.is_empty() {
			return None;
		}
		let is_wrapped = self.is_wrapped();
		if is_wrapped && self.word_wrap_width <= 0.0 {
			return None;
		}
		let expected_wrap_width = if is_wrapped {
			self.word_wrap_width
		}
		else {
			0.0
		};
		if self.layout_wrap_width != expected_wrap_width {
			return None;
		}
		Some(self.line_list.len())
	}

	//==============================================================================
	// 줄바꿈 폭 설정. (0 이면 줄바꿈 문자로만 나눈다)
	//==============================================================================
	pub fn set_word_wrap_width(&mut self, word_wrap_width: f32) {
		if self.word_wrap_width != word_wrap_width {
			self.word_wrap_width = word_wrap_width;
			self.is_layout_dirty = true;
		}
	}

	//==============================================================================
	// 줄바꿈 폭 반환.
	//==============================================================================
	pub fn get_word_wrap_width(&self) -> f32 {
		self.word_wrap_width
	}

	//==============================================================================
	// 줄바꿈 방식 설정. (기본 Word)
	//==============================================================================
	pub fn set_wrap_mode(&mut self, wrap_mode: TextWrapMode) {
		if self.wrap_mode != wrap_mode {
			self.wrap_mode = wrap_mode;
			self.is_layout_dirty = true;
		}
	}

	//==============================================================================
	// 줄바꿈 방식 반환.
	//==============================================================================
	pub fn get_wrap_mode(&self) -> TextWrapMode {
		self.wrap_mode
	}

	//==============================================================================
	// 줄 간격 설정. (글자 크기의 배수)
	//==============================================================================
	pub fn set_line_spacing(&mut self, line_spacing: f32) {
		self.line_spacing = line_spacing;
	}

	//==============================================================================
	// 줄 간격 반환.
	//==============================================================================
	pub fn get_line_spacing(&self) -> f32 {
		self.line_spacing
	}

	//==============================================================================
	// 보일 글자 수 설정. (-1 이면 전체, 타자기 연출용. -1 아래는 -1 로 맞춘다)
	//==============================================================================
	pub fn set_visible_character_count(&mut self, visible_character_count: i32) {
		self.visible_character_count = visible_character_count.max(-1);
	}

	//==============================================================================
	// 보일 글자 수 반환.
	//==============================================================================
	pub fn get_visible_character_count(&self) -> i32 {
		self.visible_character_count
	}

	//==============================================================================
	// 보일 글자 수만큼 앞에서 자르기. (-1 이면 그대로)
	//==============================================================================
	fn apply_visible_character_count<'a>(&self, text: &'a str) -> &'a str {
		if self.visible_character_count < 0 {
			return text;
		}
		&text[..find_grapheme_prefix_length(text, self.visible_character_count as usize)]
	}
}

impl Default for Text {
	fn default() -> Text {
		Text::new()
	}
}

impl Component for Text {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Text"
	}

	//==============================================================================
	// 자연 크기. (글 폭 x 글자 크기, 꺼져 있거나 잴 글꼴이 없으면 -1, -1. 원본 getIntrinsicContentSize)
	//==============================================================================
	fn get_intrinsic_content_size(&self) -> Vector2 {
		if !self.is_enable() {
			return Vector2::create(-1.0, -1.0);
		}
		if self.text.is_empty() {
			return Vector2::create(0.0, self.font_size);
		}
		match crate::core::textstringtexturecache::measure_text_without_graphic(&self.font_string, &self.text) {
			Some(text_metrics) => {
				Vector2::create(text_metrics.width, self.font_size)
			}
			None => {
				Vector2::create(-1.0, -1.0)
			}
		}
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, graphic: &mut Graphic) -> Option<Rect> {
		if self.text.is_empty() {
			return Some(Rect::create(0.0, 0.0, 0.0, 0.0));
		}
		let content_size = transform.get_content_size();
		Some(self.calculate_text_rect(graphic, content_size.x, content_size.y))
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.text.is_empty() {
			return;
		}
		let content_size = transform.get_content_size();
		if self.is_wrapped() {
			self.update_layout(graphic, content_size.x);
			self.draw_wrapped_text(graphic, content_size.x, content_size.y);
			return;
		}
		let visible_length = self.apply_visible_character_count(&self.text).len();
		if visible_length == 0 {
			return;
		}

		// 맞출 너비가 있으면 그릴 크기를 먼저 정한다. (글이나 폰트가 그대로면 지난번 결과)
		if self.fit_width > 0.0 {
			self.update_layout(graphic, content_size.x);
		}

		// 밑줄 / 취소선 길이. (글 전체면 담아 둔 폭, 앞에서 잘랐으면 잘린 글을 잰다)
		let mut text_width = 0.0;
		if self.is_underline || self.is_strikethrough {
			self.update_layout(graphic, content_size.x);
			if visible_length == self.text.len() {
				text_width = self.line_width_list.first().copied().unwrap_or(0.0);
			}
			else {
				graphic.set_font_string(&self.draw_font_string);
				text_width = graphic.measure_text(&self.text[..visible_length]).width;
			}
		}
		self.draw_plain_text(graphic, content_size.x, content_size.y, &self.text[..visible_length], text_width);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 글자 묶음과 낱말 나누기: 브라우저 Intl.Segmenter 와 같은 결과. (한글 덩어리는 따로 한 낱말, 문장 부호는 앞 낱말에)
	//==============================================================================
	#[test]
	fn split_like_browser() {
		assert_eq!(split_graphemes("👨‍👩‍👧e\u{301}xy"), vec!["👨‍👩‍👧", "e\u{301}", "x", "y"]);
		assert_eq!(count_graphemes("👨‍👩‍👧e\u{301}xy"), 4);
		assert_eq!(&"👨‍👩‍👧e\u{301}xy"[..find_grapheme_prefix_length("👨‍👩‍👧e\u{301}xy", 2)], "👨‍👩‍👧e\u{301}");
		assert_eq!(split_words("Rust로 게임engine입니다."), vec!["Rust", "로 ", "게임", "engine", "입니다."]);
		assert_eq!(split_words("HP\u{00A0}100 MP"), vec!["HP\u{00A0}100 ", "MP"]);
		assert_eq!(split_words("我们今天去公园玩吧"), vec!["我们", "今天", "去", "公园", "玩", "吧"]);
	}

	//==============================================================================
	// 줄 나누기: 공백, 낱말 사이, 글자 묶음 차례. (글자 묶음마다 폭 10 으로 잰다)
	//==============================================================================
	#[test]
	fn wrap_text_like_original() {
		let measure_width = |text: &str| -> f32 {
			count_graphemes(text) as f32 * 10.0
		};
		let mut text = Text::new();
		assert_eq!(text.wrap_text_to_lines_with("ab cd ef", 50.0, &measure_width), vec!["ab cd", "ef"]);
		assert_eq!(text.wrap_text_to_lines_with("abcdefgh", 30.0, &measure_width), vec!["abc", "def", "gh"]);
		assert_eq!(text.wrap_text_to_lines_with("我们今天去公园", 50.0, &measure_width), vec!["我们今天去", "公园"]);
		assert_eq!(text.wrap_text_to_lines_with("a\n\nb", 50.0, &measure_width), vec!["a", "", "b"]);
		text.set_wrap_mode(TextWrapMode::Character);
		assert_eq!(text.wrap_text_to_lines_with("👨‍👩‍👧e\u{301}xy", 20.0, &measure_width), vec!["👨‍👩‍👧e\u{301}", "xy"]);
	}
}
