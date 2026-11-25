//==============================================================================
// 리치텍스트 컴포넌트. (원본 core/component/richtext.js 의 RichText, parse_markup_to_segments)
// - 마크업(Unity TMP 와 같은 꼴)을 풀어 조각(segment)마다 폰트, 크기, 색, 굵게, 기울임, 밑줄, 취소선을 달리해 한 줄로 잇는다.
//   지원 태그: <b>, <i>, <u>, <s>, <color=#RRGGBB[AA]>, <color=이름>, <size=N>. 모르는 태그는 글 그대로 보인다.
// - 원본은 Text 를 상속한다. 러스트 판은 Text 를 품고(UILabel 이 Text 를 품는 것과 같은 꼴) 세터를 넘긴다.
//   마크업이 없으면 품은 Text 의 draw 로 그린다. (여러 줄, 줄바꿈 폭 그대로) 조각의 정하지 않은 속성은 Text 의 값을 쓴다.
// - 조각 배치(layout_segments)는 Graphic 없이 재는 함수를 받아 한다. draw 는 글자 텍스처 캐시(textstringtexturecache)의 measure_text 로 잰다.
// - 원본과 다른 것:
//   - 조각 속성 fontFace(FontFace / FontAsset) 는 font_family(등록한 폰트 이름) 다.
//   - 조각의 밑줄, 취소선, 기울임은 원본처럼 조각 속성이 없으면 Text 의 값(set_underline, set_strikethrough, set_italic)을 쓴다.
//   - 잘못된 #색(16진수가 아닌 글자)은 흰색이다. (원본은 NaN 색)
//   - getIntrinsicContentSize 는 재는 함수를 받는 calculate_intrinsic_content_size 다.
// - 글자 방향: 조각을 이은 글로 문단 방향을 정하고(품은 Text 의 방향, Inherit 면 Graphic 의 방향), 오른쪽에서 왼쪽이면 조각을 오른쪽부터 놓는다.
//   조각마다 그 방향으로 모양 짓고, 정렬 Start / End 도 그 방향으로 푼다. (원본도 같다)
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::text::{Text, TextWrapMode};
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{detect_text_direction, Graphic, TextAlign, TextBaseline, TextDirection};
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
// 마크업 색 이름 → HEX. (원본 NAMED_COLORS, Unity TMP 와 호환)
//==============================================================================
const NAMED_COLOR_LIST: [(&str, &str); 22] = [
	("red", "#ff0000"),
	("green", "#00ff00"),
	("blue", "#0000ff"),
	("white", "#ffffff"),
	("black", "#000000"),
	("yellow", "#ffff00"),
	("cyan", "#00ffff"),
	("magenta", "#ff00ff"),
	("gray", "#808080"),
	("grey", "#808080"),
	("orange", "#ffa500"),
	("purple", "#800080"),
	("brown", "#a52a2a"),
	("pink", "#ffc0cb"),
	("silver", "#c0c0c0"),
	("maroon", "#800000"),
	("olive", "#808000"),
	("navy", "#000080"),
	("teal", "#008080"),
	("lime", "#00ff00"),
	("aqua", "#00ffff"),
	("fuchsia", "#ff00ff"),
];

//==============================================================================
// 조각 속성. (None / false 면 Text 의 값을 쓴다)
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichTextAttributes {
	pub text_color: Option<Color>,
	pub font_size: Option<f32>,
	pub font_family: Option<String>,
	pub stroke_color: Option<Color>,
	pub stroke_width: Option<f32>,
	pub is_bold: bool,
	pub is_italic: bool,
	pub is_underline: bool,
	pub is_strikethrough: bool,
}

impl RichTextAttributes {
	//==============================================================================
	// 생성. (아무것도 정하지 않음)
	//==============================================================================
	pub fn new() -> RichTextAttributes {
		RichTextAttributes::default()
	}

	//==============================================================================
	// 안쪽 속성을 덮어쓰기. (정한 것만, 원본 mergeAttributeStack)
	//==============================================================================
	fn merge_from(&mut self, inner_attributes: &RichTextAttributes) {
		if inner_attributes.text_color.is_some() {
			self.text_color = inner_attributes.text_color;
		}
		if inner_attributes.font_size.is_some() {
			self.font_size = inner_attributes.font_size;
		}
		if inner_attributes.font_family.is_some() {
			self.font_family = inner_attributes.font_family.clone();
		}
		if inner_attributes.stroke_color.is_some() {
			self.stroke_color = inner_attributes.stroke_color;
		}
		if inner_attributes.stroke_width.is_some() {
			self.stroke_width = inner_attributes.stroke_width;
		}
		self.is_bold = self.is_bold || inner_attributes.is_bold;
		self.is_italic = self.is_italic || inner_attributes.is_italic;
		self.is_underline = self.is_underline || inner_attributes.is_underline;
		self.is_strikethrough = self.is_strikethrough || inner_attributes.is_strikethrough;
	}
}

//==============================================================================
// 조각. (글 + 속성)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextSegment {
	pub text: String,
	pub attributes: RichTextAttributes,
}

//==============================================================================
// 배치된 조각 하나. (그릴 자리와 모든 속성을 푼 값)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextRun {
	pub text: String,
	pub font_string: String,
	pub font_size: f32,
	pub x: f32,
	pub width: f32,
	pub text_color: Color,
	pub stroke_color: Color,
	pub stroke_width: f32,
	pub is_underline: bool,
	pub is_strikethrough: bool,
}

//==============================================================================
// 조각 배치 결과.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextLayout {
	pub run_list: Vec<RichTextRun>,
	pub total_width: f32,
	pub maximum_font_size: f32,
	pub baseline_y: f32,
}

//==============================================================================
// 리치텍스트 컴포넌트.
//==============================================================================
pub struct RichText {
	text: Text,
	segments: Vec<RichTextSegment>,
}

impl RichText {
	//==============================================================================
	// 생성. (기본값은 Text 와 같다)
	//==============================================================================
	pub fn new() -> RichText {
		RichText {
			text: Text::new(),
			segments: Vec::new(),
		}
	}

	//==============================================================================
	// 글과 크기로 생성.
	//==============================================================================
	pub fn create_with_text(text: &str, font_size: f32) -> RichText {
		let mut rich_text = RichText::new();
		rich_text.set_text(text);
		rich_text.set_font_size(font_size);
		rich_text
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (전역 속성과 마크업이 없을 때의 그리기)
	//==============================================================================
	pub fn get_text_component(&self) -> &Text {
		&self.text
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (변경용)
	//==============================================================================
	pub fn get_text_component_mut(&mut self) -> &mut Text {
		&mut self.text
	}

	//==============================================================================
	// 글 설정. (마크업을 풀어 조각으로 둔다. 알려진 태그가 없으면 조각을 비우고 일반 글로 그린다)
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		self.text.set_text(text);
		self.segments = parse_markup_to_segments(text).unwrap_or_default();
	}

	//==============================================================================
	// 글 반환. (마크업이 든 원래 글)
	//==============================================================================
	pub fn get_text(&self) -> &str {
		self.text.get_text()
	}

	//==============================================================================
	// 조각 하나 더하기. (마크업 없이 코드로 조각을 쌓을 때)
	//==============================================================================
	pub fn append_segment(&mut self, text: &str, attributes: RichTextAttributes) -> &mut RichText {
		self.segments.push(RichTextSegment {
			text: text.to_string(),
			attributes,
		});
		self
	}

	//==============================================================================
	// 조각 모두 지우기.
	//==============================================================================
	pub fn clear_segments(&mut self) {
		self.segments.clear();
	}

	//==============================================================================
	// 조각 목록.
	//==============================================================================
	pub fn get_segments(&self) -> &[RichTextSegment] {
		&self.segments
	}

	//==============================================================================
	// 글이 있는 조각이 하나라도 있는지.
	//==============================================================================
	pub fn has_segments(&self) -> bool {
		self.segments.iter().any(|segment| !segment.text.is_empty())
	}

	//==============================================================================
	// 글자 크기 설정.
	//==============================================================================
	pub fn set_font_size(&mut self, font_size: f32) {
		self.text.set_font_size(font_size);
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn get_font_size(&self) -> f32 {
		self.text.get_font_size()
	}

	//==============================================================================
	// 폰트 이름 설정.
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		self.text.set_font_family(font_family);
	}

	//==============================================================================
	// 폰트 이름 반환.
	//==============================================================================
	pub fn get_font_family(&self) -> &str {
		self.text.get_font_family()
	}

	//==============================================================================
	// 글자 색 설정.
	//==============================================================================
	pub fn set_text_color(&mut self, color: impl Into<Color>) {
		self.text.set_text_color(color);
	}

	//==============================================================================
	// 글자 색 반환.
	//==============================================================================
	pub fn get_text_color(&self) -> Color {
		self.text.get_text_color()
	}

	//==============================================================================
	// 외곽선 색 설정.
	//==============================================================================
	pub fn set_stroke_color(&mut self, color: impl Into<Color>) {
		self.text.set_stroke_color(color);
	}

	//==============================================================================
	// 외곽선 색 반환.
	//==============================================================================
	pub fn get_stroke_color(&self) -> Color {
		self.text.get_stroke_color()
	}

	//==============================================================================
	// 외곽선 두께 설정.
	//==============================================================================
	pub fn set_stroke_width(&mut self, stroke_width: f32) {
		self.text.set_stroke_width(stroke_width);
	}

	//==============================================================================
	// 외곽선 두께 반환.
	//==============================================================================
	pub fn get_stroke_width(&self) -> f32 {
		self.text.get_stroke_width()
	}

	//==============================================================================
	// 굵게 설정.
	//==============================================================================
	pub fn set_bold(&mut self, is_bold: bool) {
		self.text.set_bold(is_bold);
	}

	//==============================================================================
	// 굵게 여부.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.text.is_bold()
	}

	//==============================================================================
	// 기울임 설정.
	//==============================================================================
	pub fn set_italic(&mut self, is_italic: bool) {
		self.text.set_italic(is_italic);
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn is_italic(&self) -> bool {
		self.text.is_italic()
	}

	//==============================================================================
	// 밑줄 설정. (조각에 <u> 가 없어도 모든 조각에 긋는다)
	//==============================================================================
	pub fn set_underline(&mut self, is_underline: bool) {
		self.text.set_underline(is_underline);
	}

	//==============================================================================
	// 밑줄 여부.
	//==============================================================================
	pub fn is_underline(&self) -> bool {
		self.text.is_underline()
	}

	//==============================================================================
	// 취소선 설정. (조각에 <s> 가 없어도 모든 조각에 긋는다)
	//==============================================================================
	pub fn set_strikethrough(&mut self, is_strikethrough: bool) {
		self.text.set_strikethrough(is_strikethrough);
	}

	//==============================================================================
	// 취소선 여부.
	//==============================================================================
	pub fn is_strikethrough(&self) -> bool {
		self.text.is_strikethrough()
	}

	//==============================================================================
	// 줄바꿈 방식 설정. (마크업이 없을 때만 쓰인다. 조각은 원본처럼 한 줄)
	//==============================================================================
	pub fn set_wrap_mode(&mut self, wrap_mode: TextWrapMode) {
		self.text.set_wrap_mode(wrap_mode);
	}

	//==============================================================================
	// 줄바꿈 방식 반환.
	//==============================================================================
	pub fn get_wrap_mode(&self) -> TextWrapMode {
		self.text.get_wrap_mode()
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		self.text.set_text_align(text_align);
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn get_text_align(&self) -> TextAlign {
		self.text.get_text_align()
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: TextBaseline) {
		self.text.set_text_baseline(text_baseline);
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn get_text_baseline(&self) -> TextBaseline {
		self.text.get_text_baseline()
	}

	//==============================================================================
	// 줄바꿈 폭 설정. (마크업이 없을 때만 쓰인다. 조각은 원본처럼 한 줄)
	//==============================================================================
	pub fn set_word_wrap_width(&mut self, word_wrap_width: f32) {
		self.text.set_word_wrap_width(word_wrap_width);
	}

	//==============================================================================
	// 줄바꿈 폭 반환.
	//==============================================================================
	pub fn get_word_wrap_width(&self) -> f32 {
		self.text.get_word_wrap_width()
	}

	//==============================================================================
	// 줄 간격 설정.
	//==============================================================================
	pub fn set_line_spacing(&mut self, line_spacing: f32) {
		self.text.set_line_spacing(line_spacing);
	}

	//==============================================================================
	// 줄 간격 반환.
	//==============================================================================
	pub fn get_line_spacing(&self) -> f32 {
		self.text.get_line_spacing()
	}

	//==============================================================================
	// 조각의 폰트 문자열과 크기. (Canvas2D font 꼴)
	//==============================================================================
	fn resolve_segment_font(&self, attributes: &RichTextAttributes) -> (String, f32) {
		let font_size = attributes.font_size.unwrap_or(self.text.get_font_size());
		let font_family = attributes.font_family.as_deref().unwrap_or(self.text.get_font_family());
		let is_bold = attributes.is_bold || self.text.is_bold();
		let is_italic = attributes.is_italic || self.text.is_italic();
		(build_font_string(font_family, font_size, is_bold, is_italic), font_size)
	}

	//==============================================================================
	// 조각 배치. (합친 폭과 가장 큰 글자 크기로 정렬, 기준선은 Text 와 같은 규칙. 원본 draw_segments 의 자리 계산)
	// - measure_width(폰트 문자열, 글) 은 글 폭을 돌려준다.
	//==============================================================================
	// - text_direction 은 실제 문단 방향이다. (Ltr 또는 Rtl, Rtl 이면 조각을 오른쪽부터 놓고 Start 는 오른쪽 정렬)
	pub fn layout_segments(&self, content_size: Vector2, text_direction: TextDirection, measure_width: &mut dyn FnMut(&str, &str) -> f32) -> RichTextLayout {
		let mut run_list: Vec<RichTextRun> = Vec::new();
		let mut total_width = 0.0;
		let mut maximum_font_size: f32 = 0.0;
		for segment in self.segments.iter() {
			if segment.text.is_empty() {
				continue;
			}
			let attributes = &segment.attributes;
			let (font_string, font_size) = self.resolve_segment_font(attributes);
			let width = measure_width(&font_string, &segment.text);
			run_list.push(RichTextRun {
				text: segment.text.clone(),
				font_string,
				font_size,
				x: total_width,
				width,
				text_color: attributes.text_color.unwrap_or(self.text.get_text_color()),
				stroke_color: attributes.stroke_color.unwrap_or(self.text.get_stroke_color()),
				stroke_width: attributes.stroke_width.unwrap_or(self.text.get_stroke_width()),
				is_underline: attributes.is_underline || self.text.is_underline(),
				is_strikethrough: attributes.is_strikethrough || self.text.is_strikethrough(),
			});
			total_width += width;
			maximum_font_size = maximum_font_size.max(font_size);
		}

		// 시작 x. (Start / End 는 문단 방향으로 푼다)
		let is_right_to_left = text_direction == TextDirection::Rtl;
		let start_x = match self.text.get_text_align() {
			TextAlign::Left => {
				0.0
			}
			TextAlign::Right => {
				content_size.x - total_width
			}
			TextAlign::Start => {
				if is_right_to_left {
					content_size.x - total_width
				}
				else {
					0.0
				}
			}
			TextAlign::End => {
				if is_right_to_left {
					0.0
				}
				else {
					content_size.x - total_width
				}
			}
			TextAlign::Center => {
				(content_size.x - total_width) * 0.5
			}
		};
		for run in run_list.iter_mut() {
			// 오른쪽에서 왼쪽이면 앞 조각이 오른쪽에 온다.
			if is_right_to_left {
				run.x = total_width - run.x - run.width;
			}
			run.x += start_x;
		}

		// 기준선 y.
		let baseline_y = match self.text.get_text_baseline() {
			TextBaseline::Top | TextBaseline::Hanging => {
				0.0
			}
			TextBaseline::Bottom | TextBaseline::Ideographic | TextBaseline::Alphabetic => {
				content_size.y
			}
			TextBaseline::Middle => {
				content_size.y * 0.5
			}
		};
		RichTextLayout {
			run_list,
			total_width,
			maximum_font_size,
			baseline_y,
		}
	}

	//==============================================================================
	// 글의 자연 크기. (조각이 있으면 합친 폭 x 가장 큰 글자 크기, 없으면 일반 글 폭 x 글자 크기. 원본 getIntrinsicContentSize)
	//==============================================================================
	pub fn calculate_intrinsic_content_size(&self, measure_width: &mut dyn FnMut(&str, &str) -> f32) -> Vector2 {
		if !self.has_segments() {
			let font_string = self.text.build_font_string();
			let width = measure_width(&font_string, self.text.get_text());
			return Vector2::create(width, self.text.get_font_size());
		}
		let layout = self.layout_segments(Vector2::zero(), TextDirection::Ltr, measure_width);
		let maximum_font_size = if layout.maximum_font_size > 0.0 {
			layout.maximum_font_size
		}
		else {
			self.text.get_font_size()
		};
		Vector2::create(layout.total_width, maximum_font_size)
	}

	//==============================================================================
	// 조각을 이은 글의 실제 방향. (품은 Text 의 방향, Inherit 면 Graphic 의 방향. Auto 는 이은 글의 첫 강한 글자로)
	//==============================================================================
	pub fn resolve_segment_text_direction(&self, graphic: &Graphic) -> TextDirection {
		let base_direction = if self.text.get_text_direction() != TextDirection::Inherit {
			self.text.get_text_direction()
		}
		else {
			graphic.get_text_direction()
		};
		match base_direction {
			TextDirection::Rtl => {
				TextDirection::Rtl
			}
			TextDirection::Auto => {
				let joined_text: String = self.segments.iter().map(|segment| segment.text.as_str()).collect();
				detect_text_direction(&joined_text)
			}
			_ => {
				TextDirection::Ltr
			}
		}
	}

	//==============================================================================
	// 글자 방향 설정. (품은 Text 에 넘긴다, 원본 setTextDirection)
	//==============================================================================
	pub fn set_text_direction(&mut self, text_direction: TextDirection) {
		self.text.set_text_direction(text_direction);
	}

	//==============================================================================
	// 글자 방향 반환.
	//==============================================================================
	pub fn get_text_direction(&self) -> TextDirection {
		self.text.get_text_direction()
	}

	//==============================================================================
	// 보일 글자 수 설정. (품은 Text 에 넘긴다, -1 이면 전체. 마크업이 있는 글은 원본처럼 자르지 않는다)
	//==============================================================================
	pub fn set_visible_character_count(&mut self, visible_character_count: i32) {
		self.text.set_visible_character_count(visible_character_count);
	}

	//==============================================================================
	// 보일 글자 수 반환.
	//==============================================================================
	pub fn get_visible_character_count(&self) -> i32 {
		self.text.get_visible_character_count()
	}

	//==============================================================================
	// 밑줄 y 오프셋. (원본 Text.computeUnderlineOffsetY)
	//==============================================================================
	fn calculate_underline_offset_y(&self, font_size: f32) -> f32 {
		match self.text.get_text_baseline() {
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
	// 취소선 y 오프셋. (원본 Text.computeStrikethroughOffsetY)
	//==============================================================================
	fn calculate_strikethrough_offset_y(&self, font_size: f32) -> f32 {
		match self.text.get_text_baseline() {
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
	// 조각 그리기. (원본 draw_segments)
	//==============================================================================
	pub fn draw_segments(&self, graphic: &mut Graphic, content_size: Vector2) {
		let text_direction = self.resolve_segment_text_direction(graphic);
		graphic.set_text_direction(text_direction);
		let layout = {
			let text_string_texture_cache = graphic.get_text_string_texture_cache();
			self.layout_segments(content_size, text_direction, &mut |font_string: &str, text: &str| text_string_texture_cache.measure_text(font_string, text).width)
		};
		let baseline_y = layout.baseline_y;
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(self.text.get_text_baseline());
		for run in layout.run_list.iter() {
			graphic.set_font_string(&run.font_string);
			if run.stroke_width > 0.0 {
				graphic.set_stroke_color(run.stroke_color);
				graphic.draw_stroke_text(&run.text, run.x, baseline_y, run.stroke_width);
			}
			graphic.set_fill_color(run.text_color);
			graphic.draw_fill_text(&run.text, run.x, baseline_y);

			let decoration_line_width = (run.font_size / 16.0).max(1.0);
			if run.is_underline {
				let underline_y = baseline_y + self.calculate_underline_offset_y(run.font_size);
				graphic.set_stroke_color(run.text_color);
				graphic.draw_line(&[Vector2::create(run.x, underline_y), Vector2::create(run.x + run.width, underline_y)], decoration_line_width);
			}
			if run.is_strikethrough {
				let strikethrough_y = baseline_y + self.calculate_strikethrough_offset_y(run.font_size);
				graphic.set_stroke_color(run.text_color);
				graphic.draw_line(&[Vector2::create(run.x, strikethrough_y), Vector2::create(run.x + run.width, strikethrough_y)], decoration_line_width);
			}
		}
	}
}

impl Default for RichText {
	fn default() -> RichText {
		RichText::new()
	}
}

impl Component for RichText {
	fn get_component_base(&self) -> &ComponentBase {
		self.text.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.text.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"RichText"
	}

	//==============================================================================
	// 자연 크기. (조각을 합친 폭 x 가장 큰 글자 크기, 꺼져 있거나 잴 글꼴이 없으면 -1, -1. 원본 getIntrinsicContentSize)
	//==============================================================================
	fn get_intrinsic_content_size(&self) -> Vector2 {
		if !self.is_enable() {
			return Vector2::create(-1.0, -1.0);
		}
		if crate::core::textstringtexturecache::measure_text_without_graphic(&self.text.build_font_string(), "").is_none() {
			return Vector2::create(-1.0, -1.0);
		}
		self.calculate_intrinsic_content_size(&mut |font_string: &str, text: &str| crate::core::textstringtexturecache::measure_text_without_graphic(font_string, text).map(|text_metrics| text_metrics.width).unwrap_or(0.0))
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if !self.has_segments() {
			self.text.draw(transform, graphic);
			return;
		}
		self.draw_segments(graphic, transform.get_content_size());
	}
}

//==============================================================================
// Canvas2D font 꼴 문자열. (원본 text.js 의 build_font_string)
//==============================================================================
pub fn build_font_string(font_family: &str, font_size: f32, is_bold: bool, is_italic: bool) -> String {
	let mut style_parts: Vec<String> = Vec::new();
	if is_italic {
		style_parts.push(String::from("italic"));
	}
	if is_bold {
		style_parts.push(String::from("bold"));
	}
	style_parts.push(format!("{}px", font_size));
	style_parts.push(font_family.to_string());
	style_parts.join(" ")
}

//==============================================================================
// 색 값(#RRGGBB / #RRGGBBAA / 이름) 풀기. (원본 parse_color_value)
//==============================================================================
fn parse_color_value(value: Option<&str>) -> Option<Color> {
	let trimmed_value = value?.trim();
	if trimmed_value.is_empty() {
		return None;
	}
	if let Some(hex_text) = trimmed_value.strip_prefix('#') {
		if !hex_text.is_ascii() {
			return Some(Color::white());
		}
		return Some(Color::create_from_hex(trimmed_value));
	}
	let lowercase_value = trimmed_value.to_lowercase();
	NAMED_COLOR_LIST.iter().find(|(color_name, _)| *color_name == lowercase_value).map(|(_, hex_text)| Color::create_from_hex(hex_text))
}

//==============================================================================
// 자바스크립트 parseInt(텍스트, 10) 처럼 앞쪽 정수 읽기. (부호와 숫자까지, 숫자가 없으면 None)
//==============================================================================
fn parse_leading_integer(text: &str) -> Option<i64> {
	let trimmed_text = text.trim();
	let mut digit_text = String::new();
	for (character_index, character) in trimmed_text.chars().enumerate() {
		if character_index == 0 && (character == '+' || character == '-') {
			digit_text.push(character);
			continue;
		}
		if character.is_ascii_digit() {
			digit_text.push(character);
			continue;
		}
		break;
	}
	digit_text.parse::<i64>().ok()
}

//==============================================================================
// 태그 하나 → 속성. (모르는 태그나 값이 틀리면 None. 원본 build_attributes_for_tag)
//==============================================================================
fn build_attributes_for_tag(tag_name: &str, tag_value: Option<&str>) -> Option<RichTextAttributes> {
	let mut attributes = RichTextAttributes::new();
	match tag_name {
		"color" => {
			attributes.text_color = Some(parse_color_value(tag_value)?);
		}
		"size" => {
			let font_size = parse_leading_integer(tag_value.unwrap_or(""))?;
			if font_size <= 0 {
				return None;
			}
			attributes.font_size = Some(font_size as f32);
		}
		"b" => {
			attributes.is_bold = true;
		}
		"i" => {
			attributes.is_italic = true;
		}
		"u" => {
			attributes.is_underline = true;
		}
		"s" => {
			attributes.is_strikethrough = true;
		}
		_ => {
			return None;
		}
	}
	Some(attributes)
}

//==============================================================================
// 마크업 태그 하나 맞추기. (원본 정규식 /<(\/?)([a-zA-Z]+)(?:=([^>]+))?>/ 와 같다)
// - 맞으면 (끝 바이트 자리, 닫는 태그인지, 소문자 태그 이름, 값).
//==============================================================================
fn match_tag_at(input: &str, start_byte_index: usize) -> Option<(usize, bool, String, Option<String>)> {
	let bytes = input.as_bytes();
	let mut position = start_byte_index + 1;
	let is_close = bytes.get(position) == Some(&b'/');
	if is_close {
		position += 1;
	}
	let name_start = position;
	while position < bytes.len() && bytes[position].is_ascii_alphabetic() {
		position += 1;
	}
	if position == name_start {
		return None;
	}
	let tag_name = input[name_start..position].to_lowercase();
	match bytes.get(position) {
		Some(b'>') => {
			Some((position + 1, is_close, tag_name, None))
		}
		Some(b'=') => {
			let value_start = position + 1;
			let value_length = input[value_start..].find('>')?;
			if value_length == 0 {
				return None;
			}
			let value_end = value_start + value_length;
			Some((value_end + 1, is_close, tag_name, Some(input[value_start..value_end].to_string())))
		}
		_ => {
			None
		}
	}
}

//==============================================================================
// 마크업 → 조각 목록. (알려진 태그가 하나도 없으면 None. 원본 parse_markup_to_segments)
// - 모르는 여는 태그는 글로 남긴다. 닫는 태그는 짝이 없어도 글에서 빠진다. (원본과 같음)
//==============================================================================
pub fn parse_markup_to_segments(input: &str) -> Option<Vec<RichTextSegment>> {
	if input.is_empty() || !input.contains('<') {
		return None;
	}
	let mut segments: Vec<RichTextSegment> = Vec::new();
	let mut stack: Vec<(String, RichTextAttributes)> = Vec::new();
	let mut last_index = 0;
	let mut search_index = 0;
	let mut has_found_known_tag = false;
	let merge_stack = |stack: &Vec<(String, RichTextAttributes)>| -> RichTextAttributes {
		let mut merged_attributes = RichTextAttributes::new();
		for (_, attributes) in stack.iter() {
			merged_attributes.merge_from(attributes);
		}
		merged_attributes
	};
	while let Some(found_offset) = input[search_index..].find('<') {
		let match_start = search_index + found_offset;
		let (match_end, is_close, tag_name, tag_value) = match match_tag_at(input, match_start) {
			Some(tag_match) => {
				tag_match
			}
			None => {
				search_index = match_start + 1;
				continue;
			}
		};
		search_index = match_end;

		let preceding_text = &input[last_index..match_start];
		if !preceding_text.is_empty() {
			segments.push(RichTextSegment {
				text: preceding_text.to_string(),
				attributes: merge_stack(&stack),
			});
		}

		if is_close {
			if let Some(stack_index) = stack.iter().rposition(|(stack_tag_name, _)| *stack_tag_name == tag_name) {
				stack.remove(stack_index);
				has_found_known_tag = true;
			}
			last_index = match_end;
			continue;
		}

		match build_attributes_for_tag(&tag_name, tag_value.as_deref()) {
			Some(tag_attributes) => {
				stack.push((tag_name, tag_attributes));
				has_found_known_tag = true;
			}
			None => {
				segments.push(RichTextSegment {
					text: input[match_start..match_end].to_string(),
					attributes: merge_stack(&stack),
				});
			}
		}
		last_index = match_end;
	}

	let trailing_text = &input[last_index..];
	if !trailing_text.is_empty() {
		segments.push(RichTextSegment {
			text: trailing_text.to_string(),
			attributes: merge_stack(&stack),
		});
	}
	if !has_found_known_tag {
		return None;
	}
	Some(segments)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 시험용 재기. (글자 하나 = 크기의 절반 폭. 폰트 문자열의 "Npx" 를 읽는다)
	//==============================================================================
	fn measure_by_font_size(font_string: &str, text: &str) -> f32 {
		let font_description = crate::core::textstringtexturecache::parse_font_string(font_string);
		text.chars().count() as f32 * font_description.font_size * 0.5
	}

	//==============================================================================
	// 조각 글 목록.
	//==============================================================================
	fn collect_segment_texts(segments: &[RichTextSegment]) -> Vec<&str> {
		segments.iter().map(|segment| segment.text.as_str()).collect()
	}

	//==============================================================================
	// 태그 풀기: 굵게, 색(HEX, 이름), 크기, 겹침, 모르는 태그, 짝 없는 닫는 태그.
	//==============================================================================
	#[test]
	fn parse_markup() {
		assert!(parse_markup_to_segments("plain text").is_none());
		assert!(parse_markup_to_segments("a < b and <unknown>tag</unknown>").is_none());
		assert!(parse_markup_to_segments("").is_none());

		let segments = parse_markup_to_segments("A<b>B<color=#ff000080>C</color></b><size=20px>D</size>E").expect("segments");
		assert_eq!(collect_segment_texts(&segments), vec!["A", "B", "C", "D", "E"]);
		assert_eq!(segments[0].attributes, RichTextAttributes::new());
		assert!(segments[1].attributes.is_bold);
		assert!(segments[2].attributes.is_bold);
		assert_eq!(segments[2].attributes.text_color, Some(Color::create_from_bytes(255, 0, 0, 128)));
		assert_eq!(segments[3].attributes.font_size, Some(20.0));
		assert!(!segments[3].attributes.is_bold);
		assert_eq!(segments[4].attributes, RichTextAttributes::new());

		// 안쪽 색이 이긴다. 이름 색은 대소문자를 가리지 않는다.
		let nested = parse_markup_to_segments("<color=red>R<COLOR=Navy>N</color>R</color>").expect("segments");
		assert_eq!(collect_segment_texts(&nested), vec!["R", "N", "R"]);
		assert_eq!(nested[1].attributes.text_color, Some(Color::create_from_hex("#000080")));
		assert_eq!(nested[2].attributes.text_color, Some(Color::create_from_hex("#ff0000")));

		// 모르는 여는 태그와 틀린 값은 글로 남고, 짝 없는 닫는 태그는 빠진다.
		let unknown = parse_markup_to_segments("<u>x</u><foo>y</foo><size=0>z<color=zzz>w<b=>").expect("segments");
		assert_eq!(collect_segment_texts(&unknown), vec!["x", "<foo>", "y", "<size=0>", "z", "<color=zzz>", "w<b=>"]);
		assert!(unknown[0].attributes.is_underline);

		// 값에 '<' 가 들어가도 첫 '>' 까지가 값이다. 16진수가 아닌 # 색은 흰색.
		let strange = parse_markup_to_segments("<s>취소</s><color=#가나>한</color>").expect("segments");
		assert!(strange[0].attributes.is_strikethrough);
		assert_eq!(strange[1].attributes.text_color, Some(Color::white()));
	}

	//==============================================================================
	// 배치: 조각이 왼쪽부터 이어지고, 정렬은 합친 폭 기준, 기준선은 Text 규칙. 속성이 없으면 Text 값을 쓴다.
	//==============================================================================
	#[test]
	fn layout_runs() {
		let mut rich_text = RichText::create_with_text("ab<size=40>cd</size><b><color=blue>e</color></b>", 20.0);
		rich_text.set_text_color("#00ff00");
		rich_text.set_text_align(TextAlign::Center);
		rich_text.set_text_baseline(TextBaseline::Middle);
		let layout = rich_text.layout_segments(Vector2::create(200.0, 60.0), TextDirection::Ltr, &mut measure_by_font_size);
		assert_eq!(layout.run_list.len(), 3);
		assert_eq!(layout.total_width, 20.0 + 40.0 + 10.0);
		assert_eq!(layout.maximum_font_size, 40.0);
		assert_eq!(layout.baseline_y, 30.0);
		let start_x = (200.0 - 70.0) * 0.5;
		assert_eq!(layout.run_list[0].x, start_x);
		assert_eq!(layout.run_list[0].font_string, "20px default");
		assert_eq!(layout.run_list[0].text_color, Color::create_from_hex("#00ff00"));
		assert_eq!(layout.run_list[1].x, start_x + 20.0);
		assert_eq!(layout.run_list[1].font_size, 40.0);
		assert_eq!(layout.run_list[2].x, start_x + 60.0);
		assert_eq!(layout.run_list[2].font_string, "bold 20px default");
		assert_eq!(layout.run_list[2].text_color, Color::create_from_hex("#0000ff"));

		rich_text.set_text_align(TextAlign::Right);
		rich_text.set_text_baseline(TextBaseline::Top);
		let right_layout = rich_text.layout_segments(Vector2::create(200.0, 60.0), TextDirection::Ltr, &mut measure_by_font_size);
		assert_eq!(right_layout.run_list[0].x, 130.0);
		assert_eq!(right_layout.baseline_y, 0.0);
		assert_eq!(rich_text.calculate_intrinsic_content_size(&mut measure_by_font_size), Vector2::create(70.0, 40.0));

		// 마크업이 없으면 조각이 없고 일반 글 크기를 잰다. 코드로 조각을 쌓을 수도 있다.
		rich_text.set_text("plain");
		assert!(!rich_text.has_segments());
		assert_eq!(rich_text.calculate_intrinsic_content_size(&mut measure_by_font_size), Vector2::create(50.0, 20.0));
		let mut underline_attributes = RichTextAttributes::new();
		underline_attributes.is_underline = true;
		underline_attributes.font_family = Some(String::from("DOSGothic"));
		rich_text.append_segment("x", underline_attributes);
		assert!(rich_text.has_segments());
		let appended_layout = rich_text.layout_segments(Vector2::create(100.0, 20.0), TextDirection::Ltr, &mut measure_by_font_size);
		assert_eq!(appended_layout.run_list[0].font_string, "20px DOSGothic");
		assert!(appended_layout.run_list[0].is_underline);
		rich_text.clear_segments();
		assert!(rich_text.get_segments().is_empty());
	}

	//==============================================================================
	// 오른쪽에서 왼쪽 문단: 앞 조각이 오른쪽에 오고, Start 정렬은 오른쪽이다.
	//==============================================================================
	#[test]
	fn layout_runs_right_to_left() {
		let mut rich_text = RichText::new();
		rich_text.set_font_size(10.0);
		rich_text.set_text_align(TextAlign::Start);
		rich_text.set_text("ab<b>cd</b>");
		let layout = rich_text.layout_segments(Vector2::create(100.0, 20.0), TextDirection::Rtl, &mut measure_by_font_size);
		assert_eq!(layout.run_list.len(), 2);
		assert_eq!(layout.run_list[0].x, 90.0);
		assert_eq!(layout.run_list[1].x, 80.0);
		let left_to_right_layout = rich_text.layout_segments(Vector2::create(100.0, 20.0), TextDirection::Ltr, &mut measure_by_font_size);
		assert_eq!(left_to_right_layout.run_list[0].x, 0.0);
		assert_eq!(left_to_right_layout.run_list[1].x, 10.0);
	}
}
