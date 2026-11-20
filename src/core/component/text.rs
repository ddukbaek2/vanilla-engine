//==============================================================================
// 텍스트 컴포넌트. (노드 content_size 영역 안에 정렬해 글자를 그린다)
// - 한 줄, 그리고 줄바꿈 문자나 word_wrap_width 가 있으면 여러 줄로 그린다.
// - 원본에서 옮기지 않은 것: 밑줄 / 취소선, 오른쪽에서 왼쪽 방향, 기울임(italic 은 값만 든다).
// - 글자 수만큼 보이기(set_visible_character_count, 타자기 연출)는 원본처럼 앞에서부터 자른다. 원본은 UTF-16 단위로 세고, 여기서는 글자(char) 단위로 센다.
//   여러 줄일 때 원본은 남은 수가 음수로 떨어지면 다음 줄을 통째로 그린다. 여기서는 0 에서 멈춰 다음 줄을 그리지 않는다.
//==============================================================================
use crate::base::color::Color;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::worldnode::NodeTransform;

// 폰트 이름을 정하지 않았을 때 쓰는 이름. (등록된 이름이 없으면 엔진 기본 폰트로 그린다)
const DEFAULT_FONT_FAMILY: &str = "default";

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
	text_align: TextAlign,
	text_baseline: TextBaseline,
	word_wrap_width: f32,
	line_spacing: f32,
	visible_character_count: i32,
}

impl Text {
	//==============================================================================
	// 생성. (기본값은 원본과 같다: 32px, 흰색, 가운데 정렬, 가운데 기준선, 줄 간격 1.25)
	//==============================================================================
	pub fn new() -> Text {
		Text {
			component_base: ComponentBase::new(),
			font_family: String::from(DEFAULT_FONT_FAMILY),
			text: String::new(),
			font_size: 32.0,
			text_color: Color::white(),
			stroke_color: Color::white(),
			stroke_width: 0.0,
			is_bold: false,
			is_italic: false,
			text_align: TextAlign::Center,
			text_baseline: TextBaseline::Middle,
			word_wrap_width: 0.0,
			line_spacing: 1.25,
			visible_character_count: -1,
		}
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
		let mut style_parts: Vec<String> = Vec::new();
		if self.is_italic {
			style_parts.push(String::from("italic"));
		}
		if self.is_bold {
			style_parts.push(String::from("bold"));
		}
		style_parts.push(format!("{}px", self.font_size));
		style_parts.push(self.font_family.clone());
		style_parts.join(" ")
	}

	//==============================================================================
	// 폭에 맞춰 줄 나누기. (공백 단위로 채우고, 한 단어가 폭을 넘으면 글자 단위로 나눈다)
	//==============================================================================
	pub fn wrap_text_to_lines(&self, graphic: &Graphic, text: &str, maximum_width: f32) -> Vec<String> {
		let mut line_list: Vec<String> = Vec::new();
		for paragraph in text.split('\n') {
			if maximum_width <= 0.0 {
				line_list.push(paragraph.to_string());
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
	// 한 줄 출력.
	//==============================================================================
	fn draw_plain_text(&self, graphic: &mut Graphic, content_width: f32, content_height: f32, text: &str) {
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

		let font_string = self.build_font_string();
		graphic.set_font_string(&font_string);
		graphic.set_text_align(text_align);
		graphic.set_text_baseline(self.text_baseline);

		if self.stroke_width > 0.0 {
			graphic.set_stroke_color(self.stroke_color);
			graphic.draw_stroke_text(text, draw_x, draw_y, self.stroke_width);
		}
		graphic.set_fill_color(self.text_color);
		graphic.draw_fill_text(text, draw_x, draw_y);
	}

	//==============================================================================
	// 여러 줄 출력.
	//==============================================================================
	fn draw_wrapped_text(&self, graphic: &mut Graphic, content_width: f32, content_height: f32, full_text: &str) {
		let font_string = self.build_font_string();
		graphic.set_font_string(&font_string);

		let wrap_width = if self.word_wrap_width > 0.0 {
			self.word_wrap_width
		}
		else {
			content_width
		};
		let line_list = self.wrap_text_to_lines(graphic, full_text, wrap_width);
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
		}
	}

	//==============================================================================
	// 폰트 이름 설정. (Graphic 에 등록한 이름, 없으면 기본 폰트)
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		self.font_family = font_family.to_string();
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
		self.font_size = font_size;
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn get_font_size(&self) -> f32 {
		self.font_size
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
		self.is_bold = is_bold;
	}

	//==============================================================================
	// 굵게 여부.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.is_bold
	}

	//==============================================================================
	// 기울임 설정. (값만 든다)
	//==============================================================================
	pub fn set_italic(&mut self, is_italic: bool) {
		self.is_italic = is_italic;
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn is_italic(&self) -> bool {
		self.is_italic
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
		self.word_wrap_width = word_wrap_width;
	}

	//==============================================================================
	// 줄바꿈 폭 반환.
	//==============================================================================
	pub fn get_word_wrap_width(&self) -> f32 {
		self.word_wrap_width
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

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.text.is_empty() {
			return;
		}
		let content_size = transform.get_content_size();
		if self.word_wrap_width > 0.0 || self.text.contains('\n') {
			self.draw_wrapped_text(graphic, content_size.x, content_size.y, &self.text);
			return;
		}
		let visible_text = self.apply_visible_character_count(&self.text);
		if visible_text.is_empty() {
			return;
		}
		self.draw_plain_text(graphic, content_size.x, content_size.y, visible_text);
	}
}
