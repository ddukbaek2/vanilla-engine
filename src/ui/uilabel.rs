//==============================================================================
// UI 라벨. (원본 ui/uilabel.js 의 UILabel)
// - 표시 전용 위젯. 호스트 content_size 영역 안에 정렬해 글자를 그린다. 세터는 원본처럼 글자 컴포넌트에 넘긴다.
// - 원본은 require() 로 호스트에 Text 와 RichText 를 붙이고 그 참조를 든다. 러스트 판은 RichText 하나를 품고, RichText 가 Text 를 품는다.
//   (Sprite 가 Paint 를 품는 것과 같은 꼴) 씬 코드가 tick 밖에서 label.set_text 를 불러도 바로 반영되게 하려는 것이다.
//   형제 Text 를 두면 tick 안에서만 닿아 한 프레임 늦는다. 그래서 라벨 노드의 get_component::<Text>() 는 None 이다.
//   품은 Text 는 get_text_component / get_text_component_mut, RichText 는 get_rich_text_component / get_rich_text_component_mut 로 꺼낸다.
// - 원본은 Text 와 RichText 두 벌에 세터를 같이 걸고 use_rich_text 로 어느 쪽을 켤지 고른다. 러스트 판은 한 벌이라 맞출 일이 없다.
//   use_rich_text(true) 면 RichText 로(마크업 해석), 아니면 품은 Text 로 그린다. append_segment / clear_segments 는 RichText 에 넘긴다.
// - 원본 setFont(FontFace) 는 set_font_family(등록한 폰트 이름) 로 바꿨다.
// - 글자 방향(set_text_direction)과 표시 글자 수(set_visible_character_count)는 원본처럼 글자 컴포넌트에 넘긴다.
// - 원본에서 옮기지 않은 것: invalidateIntrinsicContentSize (UINode 노선 전용).
// - 배경색(UIView)은 글자보다 먼저 그린다. (원본도 UILabel 이 require 한 Text, RichText 보다 앞 차례로 붙어 배경을 먼저 그린다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::richtext::{RichText, RichTextAttributes};
use crate::core::component::text::{Text, TextWrapMode};
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline, TextDirection};
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiview::UIView;

//==============================================================================
// UI 라벨.
//==============================================================================
pub struct UILabel {
	ui_view: UIView,
	rich_text: RichText,
	use_rich_text: bool,
}

#[crate::reflect(base = ui_view)]
impl UILabel {
	//==============================================================================
	// 생성. (글자 기본값은 Text 와 같다: 32px, 흰색, 가운데 정렬, 가운데 기준선)
	//==============================================================================
	pub fn new() -> UILabel {
		UILabel {
			ui_view: UIView::new(),
			rich_text: RichText::new(),
			use_rich_text: false,
		}
	}

	//==============================================================================
	// 글과 크기로 생성.
	//==============================================================================
	pub fn create_with_text(text: &str, font_size: f32) -> UILabel {
		let mut label = UILabel::new();
		label.set_text(text);
		label.set_font_size(font_size);
		label
	}

	//==============================================================================
	// 품은 뷰 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uiview(&self) -> &UIView {
		&self.ui_view
	}

	//==============================================================================
	// 품은 뷰 반환. (변경용)
	//==============================================================================
	pub fn get_uiview_mut(&mut self) -> &mut UIView {
		&mut self.ui_view
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_view.get_content()
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.ui_view.set_background_color(background_color);
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_background_color(&self) -> Color {
		self.ui_view.get_background_color()
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (원본의 node.get_component(Text) 자리)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_text_component(&self) -> &Text {
		self.rich_text.get_text_component()
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (변경용)
	//==============================================================================
	pub fn get_text_component_mut(&mut self) -> &mut Text {
		self.rich_text.get_text_component_mut()
	}

	//==============================================================================
	// 품은 리치텍스트 컴포넌트 반환. (원본의 node.get_component(RichText) 자리)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_rich_text_component(&self) -> &RichText {
		&self.rich_text
	}

	//==============================================================================
	// 품은 리치텍스트 컴포넌트 반환. (변경용)
	//==============================================================================
	pub fn get_rich_text_component_mut(&mut self) -> &mut RichText {
		&mut self.rich_text
	}

	//==============================================================================
	// 리치텍스트 조각 하나 더하기. (use_rich_text(true) 일 때 보인다)
	//==============================================================================
	pub fn append_segment(&mut self, text: &str, attributes: RichTextAttributes) -> &mut UILabel {
		self.rich_text.append_segment(text, attributes);
		self
	}

	//==============================================================================
	// 리치텍스트 조각 모두 지우기.
	//==============================================================================
	pub fn clear_segments(&mut self) {
		self.rich_text.clear_segments();
	}

	//==============================================================================
	// 리치텍스트 모드 설정. (true 면 마크업을 풀어 조각마다 다르게 그린다)
	//==============================================================================
	pub fn use_rich_text(&mut self, use_rich_text: bool) {
		self.use_rich_text = use_rich_text;
	}

	//==============================================================================
	// 리치텍스트 모드 여부.
	//==============================================================================
	pub fn is_using_rich_text(&self) -> bool {
		self.use_rich_text
	}

	//==============================================================================
	// 글 설정.
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		// 같은 글이면 건너뛴다. (리치텍스트는 조각을 따로 쌓았을 수 있어 늘 넣는다)
		if !self.use_rich_text && self.rich_text.get_text() == text {
			return;
		}
		self.rich_text.set_text(text);
	}

	//==============================================================================
	// 한 줄 글을 너비에 맞추기 설정. (품은 Text 의 set_fit_width, 리치텍스트 모드는 맞추지 않는다)
	//==============================================================================
	pub fn set_fit_width(&mut self, width: f32, minimum_ratio: f32) {
		self.rich_text.get_text_component_mut().set_fit_width(width, minimum_ratio);
	}

	//==============================================================================
	// 맞출 너비 반환. (0 이면 맞추지 않는다)
	//==============================================================================
	pub fn get_fit_width(&self) -> f32 {
		self.rich_text.get_text_component().get_fit_width()
	}

	//==============================================================================
	// 그릴 때의 크기 반환. (마지막으로 그리거나 잰 때의 값)
	//==============================================================================
	pub fn get_fitted_font_size(&self) -> f32 {
		self.rich_text.get_text_component().get_fitted_font_size()
	}

	//==============================================================================
	// 그릴 때의 크기를 그 자리에서 재어 반환.
	//==============================================================================
	pub fn calculate_fitted_font_size(&mut self, graphic: &mut Graphic) -> f32 {
		self.rich_text.get_text_component_mut().calculate_fitted_font_size(graphic)
	}

	//==============================================================================
	// 글의 너비 재기. (품은 Text 의 measure_text)
	//==============================================================================
	pub fn measure_text(&self, graphic: &Graphic, text: &str, font_size: f32) -> f32 {
		self.rich_text.get_text_component().measure_text(graphic, text, font_size)
	}

	//==============================================================================
	// 글 반환.
	//==============================================================================
	pub fn get_text(&self) -> &str {
		self.rich_text.get_text()
	}

	//==============================================================================
	// 글자 크기 설정.
	//==============================================================================
	pub fn set_font_size(&mut self, font_size: f32) {
		self.rich_text.set_font_size(font_size);
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn get_font_size(&self) -> f32 {
		self.rich_text.get_font_size()
	}

	//==============================================================================
	// 폰트 이름 설정. (원본 setFont 자리. Graphic 에 등록한 이름, 없으면 기본 폰트)
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		self.rich_text.set_font_family(font_family);
	}

	//==============================================================================
	// 폰트 이름 반환.
	//==============================================================================
	#[reflect(transient)]
	pub fn get_font_family(&self) -> &str {
		self.rich_text.get_font_family()
	}

	//==============================================================================
	// 자동 줄바꿈 폭 설정. (0 이면 줄바꿈 문자로만 나눈다)
	//==============================================================================
	pub fn set_word_wrap_width(&mut self, word_wrap_width: f32) {
		self.rich_text.set_word_wrap_width(word_wrap_width);
	}

	//==============================================================================
	// 줄바꿈 방식 설정. (원본 setWrapMode, Word 또는 Character)
	//==============================================================================
	pub fn set_wrap_mode(&mut self, wrap_mode: TextWrapMode) {
		self.rich_text.set_wrap_mode(wrap_mode);
	}

	//==============================================================================
	// 줄바꿈 방식 반환.
	//==============================================================================
	pub fn get_wrap_mode(&self) -> TextWrapMode {
		self.rich_text.get_wrap_mode()
	}

	//==============================================================================
	// 줄 간격 배율 설정.
	//==============================================================================
	pub fn set_line_spacing(&mut self, line_spacing: f32) {
		self.rich_text.set_line_spacing(line_spacing);
	}

	//==============================================================================
	// 글자 색 설정.
	//==============================================================================
	pub fn set_text_color(&mut self, color: impl Into<Color>) {
		self.rich_text.set_text_color(color);
	}

	//==============================================================================
	// 글자 색 반환.
	//==============================================================================
	pub fn get_text_color(&self) -> Color {
		self.rich_text.get_text_color()
	}

	//==============================================================================
	// 외곽선 색 설정.
	//==============================================================================
	pub fn set_stroke_color(&mut self, color: impl Into<Color>) {
		self.rich_text.set_stroke_color(color);
	}

	//==============================================================================
	// 외곽선 두께 설정.
	//==============================================================================
	pub fn set_stroke_width(&mut self, stroke_width: f32) {
		self.rich_text.set_stroke_width(stroke_width);
	}

	//==============================================================================
	// 굵게 설정.
	//==============================================================================
	pub fn set_bold(&mut self, is_bold: bool) {
		self.rich_text.set_bold(is_bold);
	}

	//==============================================================================
	// 굵게 여부.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.rich_text.is_bold()
	}

	//==============================================================================
	// 기울임 설정.
	//==============================================================================
	pub fn set_italic(&mut self, is_italic: bool) {
		self.rich_text.set_italic(is_italic);
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn is_italic(&self) -> bool {
		self.rich_text.is_italic()
	}

	//==============================================================================
	// 밑줄 설정.
	//==============================================================================
	pub fn set_underline(&mut self, is_underline: bool) {
		self.rich_text.set_underline(is_underline);
	}

	//==============================================================================
	// 밑줄 여부.
	//==============================================================================
	pub fn is_underline(&self) -> bool {
		self.rich_text.is_underline()
	}

	//==============================================================================
	// 취소선 설정.
	//==============================================================================
	pub fn set_strikethrough(&mut self, is_strikethrough: bool) {
		self.rich_text.set_strikethrough(is_strikethrough);
	}

	//==============================================================================
	// 취소선 여부.
	//==============================================================================
	pub fn is_strikethrough(&self) -> bool {
		self.rich_text.is_strikethrough()
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		self.rich_text.set_text_align(text_align);
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn get_text_align(&self) -> TextAlign {
		self.rich_text.get_text_align()
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: TextBaseline) {
		self.rich_text.set_text_baseline(text_baseline);
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn get_text_baseline(&self) -> TextBaseline {
		self.rich_text.get_text_baseline()
	}

	//==============================================================================
	// 글자 방향 설정. (Inherit | Ltr | Rtl | Auto, 원본 setTextDirection. Inherit 면 그리는 때의 Graphic 방향)
	//==============================================================================
	pub fn set_text_direction(&mut self, text_direction: TextDirection) {
		self.rich_text.set_text_direction(text_direction);
	}

	//==============================================================================
	// 글자 방향 반환.
	//==============================================================================
	pub fn get_text_direction(&self) -> TextDirection {
		self.rich_text.get_text_direction()
	}

	//==============================================================================
	// 보일 글자 수 설정. (-1 이면 전체, 타자기 연출용. 원본 setVisibleCharacterCount)
	//==============================================================================
	pub fn set_visible_character_count(&mut self, visible_character_count: i32) {
		self.rich_text.set_visible_character_count(visible_character_count);
	}

	//==============================================================================
	// 보일 글자 수 반환.
	//==============================================================================
	pub fn get_visible_character_count(&self) -> i32 {
		self.rich_text.get_visible_character_count()
	}
}

impl Default for UILabel {
	fn default() -> UILabel {
		UILabel::new()
	}
}

#[crate::reflect]
impl Component for UILabel {
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_view.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_view.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"UILabel"
	}

	//==============================================================================
	// 자연 크기. (품은 글의 자연 크기, UINode 가 모은다. 원본은 노드에 붙인 Text, RichText 의 값)
	//==============================================================================
	fn get_intrinsic_content_size(&self) -> Vector2 {
		if !self.is_enable() {
			return Vector2::create(-1.0, -1.0);
		}
		self.rich_text.get_intrinsic_content_size()
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_view.create_content_node(node_tree, node_id);
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, graphic: &mut Graphic) -> Option<Rect> {
		// 리치텍스트는 조각 배치를 따로 재지 않아 거르지 않는다. 품은 Text 로 그릴 때는 배경(content 영역)과 글 사각형을 합친다.
		if self.use_rich_text {
			return None;
		}
		let text_rect = self.rich_text.get_text_component_mut().calculate_drawn_rect(transform, graphic)?;
		let content_size = transform.get_content_size();
		let left_x = text_rect.position.x.min(0.0);
		let top_y = text_rect.position.y.min(0.0);
		let right_x = (text_rect.position.x + text_rect.size.x).max(content_size.x);
		let bottom_y = (text_rect.position.y + text_rect.size.y).max(content_size.y);
		Some(Rect::create(left_x, top_y, right_x - left_x, bottom_y - top_y))
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.ui_view.draw_background(transform, graphic);
		if self.use_rich_text {
			self.rich_text.draw(transform, graphic);
		}
		else {
			self.rich_text.get_text_component_mut().draw(transform, graphic);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::vector2::Vector2;

	//==============================================================================
	// 세터가 품은 Text 에 그대로 반영되고 content 노드가 생긴다.
	//==============================================================================
	#[test]
	fn setters_reach_text_and_content_is_created() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let label_id = node_tree.create_child(root_id, "label");
		node_tree[label_id].set_content_size(Vector2::create(200.0, 40.0));
		let label = node_tree.add_component(label_id, UILabel::create_with_text("hello", 20.0)).expect("label");
		label.set_text_color("#ff0000");
		label.set_text_align(TextAlign::Left);
		label.set_bold(true);
		label.use_rich_text(true);
		assert_eq!(label.get_text(), "hello");
		assert_eq!(label.get_font_size(), 20.0);
		assert_eq!(label.get_text_component().get_text_color(), Color::create(1.0, 0.0, 0.0, 1.0));
		assert_eq!(label.get_text_align(), TextAlign::Left);
		assert!(label.is_bold());
		assert!(label.is_using_rich_text());
		let content_id = label.get_content().expect("content");
		assert_eq!(node_tree.get_parent_id(content_id), Some(label_id));
	}

	//==============================================================================
	// 리치텍스트: 글을 넣으면 품은 RichText 가 마크업을 풀고, 세터는 RichText 의 Text 에 닿는다. 조각을 코드로 더할 수도 있다.
	//==============================================================================
	#[test]
	fn rich_text_path_parses_markup() {
		let mut label = UILabel::create_with_text("<b>굵게</b> 보통 <color=red>빨강</color>", 24.0);
		label.use_rich_text(true);
		label.set_text_color("#00ff00");
		let segments = label.get_rich_text_component().get_segments();
		assert_eq!(segments.len(), 3);
		assert!(segments[0].attributes.is_bold);
		assert_eq!(segments[2].text, "빨강");
		assert_eq!(label.get_rich_text_component().get_text_color(), Color::create(0.0, 1.0, 0.0, 1.0));
		assert_eq!(label.get_text(), "<b>굵게</b> 보통 <color=red>빨강</color>");
		label.clear_segments();
		label.append_segment("조각", RichTextAttributes::new());
		assert_eq!(label.get_rich_text_component().get_segments().len(), 1);
	}
}
