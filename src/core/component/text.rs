//==============================================================================
// 텍스트 컴포넌트. (노드 content_size 영역 안에 정렬해 글자를 그린다)
// - 한 줄, 그리고 줄바꿈 문자나 word_wrap_width 가 있으면 여러 줄로 그린다.
// - 나눈 줄과 줄 폭은 글, 폰트 문자열, 줄바꿈 폭, 등록된 글꼴이 바뀔 때만 다시 센다.
// - 화면 밖 거르기는 정렬, 넘침, 줄바꿈 결과로 셈한 글 사각형으로 판정한다. (원본은 거르지 않는다)
// - 밑줄 / 취소선은 원본처럼 글자 색 선을 긋는다. (두께 max(1, 글자 크기 / 16), 자리는 원본 computeUnderlineOffsetY / computeStrikethroughOffsetY)
// - 줄바꿈 방식(set_wrap_mode): Word 는 공백 단위로 채우고 넘치는 단어는 글자 단위로 나눈다. Character 는 처음부터 글자 단위. (원본 "word" / "char")
//   원본은 Intl.Segmenter 로 글자 묶음과 단어(공백 없이 쓰는 글자 포함)를 찾는다. 여기서는 글자(char) 단위와 공백 단위로만 나눈다.
// - 기울임은 폰트 문자열에 italic 을 넣는다. 글자 캐시가 fontdue 래스터를 기울여 흉내 내고, 웹 시스템 글꼴은 Canvas2D 가 italic 으로 굽는다.
// - 원본에서 옮기지 않은 것: 오른쪽에서 왼쪽 방향.
// - 글자 수만큼 보이기(set_visible_character_count, 타자기 연출)는 원본처럼 앞에서부터 자른다. 원본은 UTF-16 단위로 세고, 여기서는 글자(char) 단위로 센다.
//   여러 줄일 때 원본은 남은 수가 음수로 떨어지면 다음 줄을 통째로 그린다. 여기서는 0 에서 멈춰 다음 줄을 그리지 않는다.
// - 한 줄 글을 너비에 맞추기(set_fit_width): 원본처럼 넘치면 처음 크기에서 최소 비율까지 0.1 단위로 줄여 그린다. 줄인 크기는 줄을 나눌 때(update_layout) 재므로
//   Graphic 이 있어야 안다. (get_fitted_font_size 는 마지막으로 잰 값, calculate_fitted_font_size 는 그 자리에서 잰다) 그리는 사각형은 줄인 글의 너비를 따른다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::worldnode::NodeTransform;

// 폰트 이름을 정하지 않았을 때 쓰는 이름. (등록된 이름이 없으면 엔진 기본 폰트로 그린다)
const DEFAULT_FONT_FAMILY: &str = "default";

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
		let left_x = match graphic.resolve_text_align(self.text_align) {
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
	// 폭에 맞춰 줄 나누기. (줄바꿈 방식을 따른다. 줄바꿈 문자는 늘 지킨다, 재는 폰트는 Graphic 의 현재 폰트 문자열)
	// - Word: 공백 단위로 채우고, 한 단어가 폭을 넘으면 글자 단위로 나눈다.
	// - Character: 처음부터 글자 단위로 나눈다.
	//==============================================================================
	pub fn wrap_text_to_lines(&self, graphic: &Graphic, text: &str, maximum_width: f32) -> Vec<String> {
		let mut line_list: Vec<String> = Vec::new();
		for paragraph in text.split('\n') {
			if maximum_width <= 0.0 {
				line_list.push(paragraph.to_string());
				continue;
			}
			if self.wrap_mode == TextWrapMode::Character {
				let mut current_line = String::new();
				for character in paragraph.chars() {
					let mut candidate_line = current_line.clone();
					candidate_line.push(character);
					if !current_line.is_empty() && graphic.measure_text(&candidate_line).width > maximum_width {
						line_list.push(current_line);
						current_line = character.to_string();
					}
					else {
						current_line = candidate_line;
					}
				}
				line_list.push(current_line);
				continue;
			}
			let mut current_line = String::new();
			for word in paragraph.split(' ') {
				let candidate_line = if current_line.is_empty() {
					word.to_string()
				}
				else {
					format!("{} {}", current_line, word)
				};
				let candidate_metrics = graphic.measure_text(&candidate_line);
				if candidate_metrics.width <= maximum_width {
					current_line = candidate_line;
					continue;
				}
				if !current_line.is_empty() {
					line_list.push(current_line.clone());
					current_line.clear();
				}

				// 한 단어가 폭을 넘으면 글자 단위로 나눈다.
				let word_metrics = graphic.measure_text(word);
				if word_metrics.width <= maximum_width {
					current_line = word.to_string();
					continue;
				}
				for character in word.chars() {
					let mut character_candidate_line = current_line.clone();
					character_candidate_line.push(character);
					let character_candidate_metrics = graphic.measure_text(&character_candidate_line);
					if character_candidate_metrics.width > maximum_width && !current_line.is_empty() {
						line_list.push(current_line.clone());
						current_line = character.to_string();
					}
					else {
						current_line = character_candidate_line;
					}
				}
			}
			line_list.push(current_line);
		}
		line_list
	}

	//==============================================================================
	// 정렬에 맞는 가로 위치.
	//==============================================================================
	fn calculate_draw_x(&self, graphic: &Graphic, content_width: f32) -> (f32, TextAlign) {
		let text_align = graphic.resolve_text_align(self.text_align);
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
		let (draw_x, text_align) = self.calculate_draw_x(graphic, content_width);
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

		let (draw_x, text_align) = self.calculate_draw_x(graphic, content_width);
		graphic.set_text_align(text_align);
		graphic.set_text_baseline(TextBaseline::Middle);

		// 보일 글자 수가 정해졌으면 줄마다 남은 수만큼만 그린다. (원본처럼 나눈 줄의 글자 수로 센다)
		let mut remain_count = self.visible_character_count;
		for (line_index, full_line_text) in line_list.iter().enumerate() {
			let mut line_text = full_line_text.as_str();
			if remain_count >= 0 {
				if remain_count == 0 {
					break;
				}
				let line_character_count = full_line_text.chars().count() as i32;
				if line_character_count > remain_count {
					let cut_index = full_line_text.char_indices().nth(remain_count as usize).map(|(byte_index, _)| byte_index).unwrap_or(full_line_text.len());
					line_text = &full_line_text[..cut_index];
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
		match text.char_indices().nth(self.visible_character_count as usize) {
			Some((byte_index, _)) => {
				&text[..byte_index]
			}
			None => {
				text
			}
		}
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
