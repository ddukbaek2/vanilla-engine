//==============================================================================
//! UI 라벨. (원본 ui/uilabel.js 의 UILabel)
//! - 표시 전용 위젯. 호스트 content_size 영역 안에 정렬해 글자를 그린다. 세터는 원본처럼 글자 컴포넌트에 넘긴다.
//! - 원본은 require() 로 호스트에 Text 와 RichText 를 붙이고 그 참조를 든다. 러스트 판은 RichText 하나를 품고, RichText 가 Text 를 품는다.
//!   (Sprite 가 Paint 를 품는 것과 같은 꼴) 씬 코드가 tick 밖에서 label.set_text 를 불러도 바로 반영되게 하려는 것이다.
//!   형제 Text 를 두면 tick 안에서만 닿아 한 프레임 늦는다. 그래서 라벨 노드의 `get_component::<Text>()` 는 None 이다.
//!   품은 Text 는 get_text_component / get_text_component_mut, RichText 는 get_rich_text_component / get_rich_text_component_mut 로 꺼낸다.
//! - 원본은 Text 와 RichText 두 벌에 세터를 같이 걸고 use_rich_text 로 어느 쪽을 켤지 고른다. 러스트 판은 한 벌이라 맞출 일이 없다.
//!   use_rich_text(true) 면 RichText 로(마크업 해석), 아니면 품은 Text 로 그린다. append_segment / clear_segments 는 RichText 에 넘긴다.
//! - 원본 setFont(FontFace) 는 set_font_family(등록한 폰트 이름) 로 바꿨다.
//! - 글자 방향(set_text_direction)과 표시 글자 수(set_visible_character_count)는 원본처럼 글자 컴포넌트에 넘긴다.
//! - 원본에서 옮기지 않은 것: invalidateIntrinsicContentSize (UINode 노선 전용).
//! - 배경색(UIView)은 글자보다 먼저 그린다. (원본도 UILabel 이 require 한 Text, RichText 보다 앞 차례로 붙어 배경을 먼저 그린다)
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
/// UI 라벨.
/// - RichText 를 품고(RichText 가 Text 를 품는다) 세터를 넘긴다. 호스트 content_size 영역 안에 정렬해 그리고, 배경색을 먼저 칠한다.
///
/// # 예제
/// 노드에 붙인 점수 라벨
/// ```rust
/// use vanilla::{NodeTree, Text, TextAlign, UILabel, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let label_id = node_tree.create_child(root_id, "score");
/// node_tree[label_id].set_content_size(Vector2::create(240.0, 48.0));
/// let label = node_tree.add_component(label_id, UILabel::create_with_text("Score 0", 28.0)).unwrap();
/// label.set_text_color("#ffd166");
/// label.set_text_align(TextAlign::Left);
/// label.set_text("Score 120");
/// assert_eq!(label.get_text(), "Score 120");
/// assert_eq!(label.get_text_component().get_font_size(), 28.0);
/// assert!(node_tree[label_id].get_component::<Text>().is_none());
/// ```
///
/// 마크업으로 조각마다 다르게 그리기
/// ```rust
/// use vanilla::UILabel;
///
/// let mut label = UILabel::create_with_text("<b>Bold</b> and <color=red>red</color>", 24.0);
/// label.use_rich_text(true);
/// assert!(label.is_using_rich_text());
/// let segments = label.get_rich_text_component().get_segments();
/// assert_eq!(segments.len(), 3);
/// assert!(segments[0].attributes.is_bold);
/// assert_eq!(segments[2].text, "red");
/// ```
//==============================================================================
pub struct UILabel {
	ui_view: UIView,
	rich_text: RichText,
	use_rich_text: bool,
}

#[crate::reflect(base = ui_view)]
impl UILabel {
	//==============================================================================
	/// 생성. (글자 기본값은 Text 와 같다: 32px, 흰색, 가운데 정렬, 가운데 기준선)
	///
	/// # 돌려줌
	/// 글이 빈 새 라벨. (리치텍스트 모드 꺼짐, content 노드는 트리에 붙일 때 만든다)
	//==============================================================================
	pub fn new() -> UILabel {
		UILabel {
			ui_view: UIView::new(),
			rich_text: RichText::new(),
			use_rich_text: false,
		}
	}

	//==============================================================================
	/// 글과 크기로 생성.
	///
	/// # 인자
	/// - `text`: 보일 글. (마크업을 풀어 두지만 use_rich_text(true) 일 때만 조각으로 그린다)
	/// - `font_size`: 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// 그 글과 크기의 새 라벨.
	//==============================================================================
	pub fn create_with_text(text: &str, font_size: f64) -> UILabel {
		let mut label = UILabel::new();
		label.set_text(text);
		label.set_font_size(font_size);
		label
	}

	//==============================================================================
	/// 품은 뷰 반환.
	///
	/// # 돌려줌
	/// 품은 UIView 의 참조.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uiview(&self) -> &UIView {
		&self.ui_view
	}

	//==============================================================================
	/// 품은 뷰 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 UIView 의 바꿀 수 있는 참조.
	//==============================================================================
	pub fn get_uiview_mut(&mut self) -> &mut UIView {
		&mut self.ui_view
	}

	//==============================================================================
	/// 컨텐트 노드 핸들.
	///
	/// # 돌려줌
	/// content 노드. 트리에 붙기 전이면 None.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_view.get_content()
	}

	//==============================================================================
	/// 배경색 설정.
	///
	/// # 인자
	/// - `background_color`: 배경색. (글자보다 먼저 그린다, 알파가 0 이면 칠하지 않는다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		Component::mark_drawn_rect_dirty(self);
		self.ui_view.set_background_color(background_color);
	}

	//==============================================================================
	/// 배경색 반환.
	///
	/// # 돌려줌
	/// 배경색. (기본은 알파 0 인 흰색)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_background_color(&self) -> Color {
		self.ui_view.get_background_color()
	}

	//==============================================================================
	/// 품은 글자 컴포넌트 반환. (원본의 node.get_component(Text) 자리)
	///
	/// # 돌려줌
	/// 품은 Text 의 참조. (품은 RichText 안의 것)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_text_component(&self) -> &Text {
		self.rich_text.get_text_component()
	}

	//==============================================================================
	/// 품은 글자 컴포넌트 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 Text 의 바꿀 수 있는 참조.
	//==============================================================================
	pub fn get_text_component_mut(&mut self) -> &mut Text {
		self.rich_text.get_text_component_mut()
	}

	//==============================================================================
	/// 품은 리치텍스트 컴포넌트 반환. (원본의 node.get_component(RichText) 자리)
	///
	/// # 돌려줌
	/// 품은 RichText 의 참조.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_rich_text_component(&self) -> &RichText {
		&self.rich_text
	}

	//==============================================================================
	/// 품은 리치텍스트 컴포넌트 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 RichText 의 바꿀 수 있는 참조.
	//==============================================================================
	pub fn get_rich_text_component_mut(&mut self) -> &mut RichText {
		&mut self.rich_text
	}

	//==============================================================================
	/// 리치텍스트 조각 하나 더하기. (use_rich_text(true) 일 때 보인다)
	///
	/// # 인자
	/// - `text`: 조각 글. (마크업을 풀지 않는다)
	/// - `attributes`: 조각의 꾸밈. (정하지 않은 칸은 라벨의 값을 따른다)
	///
	/// # 돌려줌
	/// 이어 부를 수 있게 이 라벨.
	///
	/// # 예제
	/// 코드로 조각을 이어 쌓기
	/// ```rust
	/// use vanilla::{RichTextAttributes, UILabel};
	///
	/// let mut label = UILabel::new();
	/// label.use_rich_text(true);
	/// let mut bold_attributes = RichTextAttributes::new();
	/// bold_attributes.is_bold = true;
	/// label.append_segment("Level ", RichTextAttributes::new()).append_segment("3", bold_attributes);
	/// assert_eq!(label.get_rich_text_component().get_segments().len(), 2);
	/// label.clear_segments();
	/// assert!(label.get_rich_text_component().get_segments().is_empty());
	/// ```
	//==============================================================================
	pub fn append_segment(&mut self, text: &str, attributes: RichTextAttributes) -> &mut UILabel {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.append_segment(text, attributes);
		self
	}

	//==============================================================================
	/// 리치텍스트 조각 모두 지우기.
	//==============================================================================
	pub fn clear_segments(&mut self) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.clear_segments();
	}

	//==============================================================================
	/// 리치텍스트 모드 설정. (true 면 마크업을 풀어 조각마다 다르게 그린다)
	///
	/// # 인자
	/// - `use_rich_text`: true 면 품은 RichText 로(조각마다), false 면 품은 Text 로 그린다.
	//==============================================================================
	pub fn use_rich_text(&mut self, use_rich_text: bool) {
		self.use_rich_text = use_rich_text;
	}

	//==============================================================================
	/// 리치텍스트 모드 여부.
	///
	/// # 돌려줌
	/// 리치텍스트 모드면 true.
	//==============================================================================
	pub fn is_using_rich_text(&self) -> bool {
		self.use_rich_text
	}

	//==============================================================================
	/// 글 설정.
	///
	/// # 인자
	/// - `text`: 새 글. (리치텍스트 모드가 아니고 지금 글과 같으면 건너뛴다, 마크업은 늘 풀어 조각으로 둔다)
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		Component::mark_drawn_rect_dirty(self);
		// 같은 글이면 건너뛴다. (리치텍스트는 조각을 따로 쌓았을 수 있어 늘 넣는다)
		if !self.use_rich_text && self.rich_text.get_text() == text {
			return;
		}
		self.rich_text.set_text(text);
	}

	//==============================================================================
	/// 글 반환.
	///
	/// # 돌려줌
	/// 지금 글. (마크업이 든 원래 글)
	//==============================================================================
	pub fn get_text(&self) -> &str {
		self.rich_text.get_text()
	}

	//==============================================================================
	/// 한 줄 글을 너비에 맞추기 설정. (품은 Text 의 set_fit_width, 리치텍스트 모드는 맞추지 않는다)
	///
	/// # 인자
	/// - `width`: 맞출 너비. (픽셀, 0 이하면 맞추지 않는다)
	/// - `minimum_ratio`: 글자 크기를 줄일 수 있는 가장 작은 비율. (0 ~ 1 로 자른다)
	///
	/// # 예제
	/// 단추 칸 180 픽셀에 맞추되 60% 아래로는 줄이지 않기
	/// ```rust
	/// use vanilla::UILabel;
	///
	/// let mut label = UILabel::create_with_text("Start", 32.0);
	/// label.set_fit_width(180.0, 0.6);
	/// assert_eq!(label.get_fit_width(), 180.0);
	/// assert_eq!(label.get_font_size(), 32.0);
	/// label.set_fit_width(-5.0, 0.6);
	/// assert_eq!(label.get_fit_width(), 0.0);
	/// ```
	//==============================================================================
	pub fn set_fit_width(&mut self, width: f64, minimum_ratio: f64) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.get_text_component_mut().set_fit_width(width, minimum_ratio);
	}

	//==============================================================================
	/// 맞출 너비 반환. (0 이면 맞추지 않는다)
	///
	/// # 돌려줌
	/// 맞출 너비. (픽셀, 0 이면 맞추지 않는다)
	//==============================================================================
	pub fn get_fit_width(&self) -> f64 {
		self.rich_text.get_text_component().get_fit_width()
	}

	//==============================================================================
	/// 그릴 때의 크기를 그 자리에서 재어 반환.
	///
	/// # 인자
	/// - `graphic`: 글을 잴 그래픽.
	///
	/// # 돌려줌
	/// 그릴 때의 글자 크기. (픽셀, 맞출 너비를 넘치면 줄인 크기)
	//==============================================================================
	pub fn calculate_fitted_font_size(&mut self, graphic: &mut Graphic) -> f64 {
		self.rich_text.get_text_component_mut().calculate_fitted_font_size(graphic)
	}

	//==============================================================================
	/// 글의 너비 재기. (품은 Text 의 measure_text)
	///
	/// # 인자
	/// - `graphic`: 글을 잴 그래픽.
	/// - `text`: 잴 글. (라벨의 글과 따로)
	/// - `font_size`: 잴 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// 글의 너비. (픽셀, 빈 글이면 0)
	//==============================================================================
	pub fn measure_text(&self, graphic: &Graphic, text: &str, font_size: f64) -> f64 {
		self.rich_text.get_text_component().measure_text(graphic, text, font_size)
	}

	//==============================================================================
	/// 그릴 때의 크기 반환. (마지막으로 그리거나 잰 때의 값)
	///
	/// # 돌려줌
	/// 그릴 때의 글자 크기. (픽셀, 아직 배치를 다시 재지 않았으면 정한 크기)
	//==============================================================================
	pub fn get_fitted_font_size(&self) -> f64 {
		self.rich_text.get_text_component().get_fitted_font_size()
	}

	//==============================================================================
	/// 글자 크기 설정.
	///
	/// # 인자
	/// - `font_size`: 글자 크기. (픽셀)
	//==============================================================================
	pub fn set_font_size(&mut self, font_size: f64) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_font_size(font_size);
	}

	//==============================================================================
	/// 글자 크기 반환.
	///
	/// # 돌려줌
	/// 정한 글자 크기. (픽셀, 맞춰 줄여도 그대로)
	//==============================================================================
	pub fn get_font_size(&self) -> f64 {
		self.rich_text.get_font_size()
	}

	//==============================================================================
	/// 폰트 이름 설정. (원본 setFont 자리. Graphic 에 등록한 이름, 없으면 기본 폰트)
	///
	/// # 인자
	/// - `font_family`: 폰트 이름. (Graphic 에 등록한 이름)
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_font_family(font_family);
	}

	//==============================================================================
	/// 폰트 이름 반환.
	///
	/// # 돌려줌
	/// 폰트 이름.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_font_family(&self) -> &str {
		self.rich_text.get_font_family()
	}

	//==============================================================================
	/// 자동 줄바꿈 폭 설정. (0 이면 줄바꿈 문자로만 나눈다)
	///
	/// # 인자
	/// - `word_wrap_width`: 자동 줄바꿈 폭. (픽셀, 0 이면 줄바꿈 문자로만 나눈다)
	//==============================================================================
	pub fn set_word_wrap_width(&mut self, word_wrap_width: f64) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_word_wrap_width(word_wrap_width);
	}

	//==============================================================================
	/// 줄바꿈 방식 설정. (원본 setWrapMode, Word 또는 Character)
	///
	/// # 인자
	/// - `wrap_mode`: 줄바꿈 방식. (Word 는 낱말 단위, Character 는 글자 단위)
	//==============================================================================
	pub fn set_wrap_mode(&mut self, wrap_mode: TextWrapMode) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_wrap_mode(wrap_mode);
	}

	//==============================================================================
	/// 줄바꿈 방식 반환.
	///
	/// # 돌려줌
	/// 줄바꿈 방식.
	//==============================================================================
	pub fn get_wrap_mode(&self) -> TextWrapMode {
		self.rich_text.get_wrap_mode()
	}

	//==============================================================================
	/// 줄 간격 배율 설정.
	///
	/// # 인자
	/// - `line_spacing`: 줄 간격. (글자 크기의 배수)
	//==============================================================================
	pub fn set_line_spacing(&mut self, line_spacing: f64) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_line_spacing(line_spacing);
	}

	//==============================================================================
	/// 보일 글자 수 설정. (-1 이면 전체, 타자기 연출용. 원본 setVisibleCharacterCount)
	///
	/// # 인자
	/// - `visible_character_count`: 보일 글자 수. (-1 이면 전체, -1 아래는 -1 로 맞춘다. 마크업이 든 글은 자르지 않는다)
	//==============================================================================
	pub fn set_visible_character_count(&mut self, visible_character_count: i32) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_visible_character_count(visible_character_count);
	}

	//==============================================================================
	/// 보일 글자 수 반환.
	///
	/// # 돌려줌
	/// 보일 글자 수. (-1 이면 전체)
	//==============================================================================
	pub fn get_visible_character_count(&self) -> i32 {
		self.rich_text.get_visible_character_count()
	}

	//==============================================================================
	/// 외곽선 색 설정.
	///
	/// # 인자
	/// - `color`: 외곽선 색.
	//==============================================================================
	pub fn set_stroke_color(&mut self, color: impl Into<Color>) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_stroke_color(color);
	}

	//==============================================================================
	/// 외곽선 두께 설정.
	///
	/// # 인자
	/// - `stroke_width`: 외곽선 두께. (픽셀, 0 이면 외곽선 없음)
	//==============================================================================
	pub fn set_stroke_width(&mut self, stroke_width: f64) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_stroke_width(stroke_width);
	}

	//==============================================================================
	/// 글자 색 설정.
	///
	/// # 인자
	/// - `color`: 글자 색. (Color 나 "#rrggbb" 같은 글)
	//==============================================================================
	pub fn set_text_color(&mut self, color: impl Into<Color>) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_text_color(color);
	}

	//==============================================================================
	/// 글자 색 반환.
	///
	/// # 돌려줌
	/// 글자 색.
	//==============================================================================
	pub fn get_text_color(&self) -> Color {
		self.rich_text.get_text_color()
	}

	//==============================================================================
	/// 굵게 설정.
	///
	/// # 인자
	/// - `is_bold`: true 면 굵게.
	//==============================================================================
	pub fn set_bold(&mut self, is_bold: bool) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_bold(is_bold);
	}

	//==============================================================================
	/// 굵게 여부.
	///
	/// # 돌려줌
	/// 굵게면 true.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.rich_text.is_bold()
	}

	//==============================================================================
	/// 기울임 설정.
	///
	/// # 인자
	/// - `is_italic`: true 면 기울임.
	//==============================================================================
	pub fn set_italic(&mut self, is_italic: bool) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_italic(is_italic);
	}

	//==============================================================================
	/// 기울임 여부.
	///
	/// # 돌려줌
	/// 기울임이면 true.
	//==============================================================================
	pub fn is_italic(&self) -> bool {
		self.rich_text.is_italic()
	}

	//==============================================================================
	/// 밑줄 설정.
	///
	/// # 인자
	/// - `is_underline`: true 면 밑줄.
	//==============================================================================
	pub fn set_underline(&mut self, is_underline: bool) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_underline(is_underline);
	}

	//==============================================================================
	/// 밑줄 여부.
	///
	/// # 돌려줌
	/// 밑줄이면 true.
	//==============================================================================
	pub fn is_underline(&self) -> bool {
		self.rich_text.is_underline()
	}

	//==============================================================================
	/// 취소선 설정.
	///
	/// # 인자
	/// - `is_strikethrough`: true 면 취소선.
	//==============================================================================
	pub fn set_strikethrough(&mut self, is_strikethrough: bool) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_strikethrough(is_strikethrough);
	}

	//==============================================================================
	/// 취소선 여부.
	///
	/// # 돌려줌
	/// 취소선이면 true.
	//==============================================================================
	pub fn is_strikethrough(&self) -> bool {
		self.rich_text.is_strikethrough()
	}

	//==============================================================================
	/// 글자 방향 설정. (Inherit | Ltr | Rtl | Auto, 원본 setTextDirection. Inherit 면 그리는 때의 Graphic 방향)
	///
	/// # 인자
	/// - `text_direction`: 글자 방향.
	//==============================================================================
	pub fn set_text_direction(&mut self, text_direction: TextDirection) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_text_direction(text_direction);
	}

	//==============================================================================
	/// 글자 방향 반환.
	///
	/// # 돌려줌
	/// 글자 방향.
	//==============================================================================
	pub fn get_text_direction(&self) -> TextDirection {
		self.rich_text.get_text_direction()
	}

	//==============================================================================
	/// 가로 정렬 설정.
	///
	/// # 인자
	/// - `text_align`: content 영역 안의 가로 정렬.
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_text_align(text_align);
	}

	//==============================================================================
	/// 가로 정렬 반환.
	///
	/// # 돌려줌
	/// 가로 정렬.
	//==============================================================================
	pub fn get_text_align(&self) -> TextAlign {
		self.rich_text.get_text_align()
	}

	//==============================================================================
	/// 세로 기준선 설정.
	///
	/// # 인자
	/// - `text_baseline`: content 영역 안의 세로 기준선.
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: TextBaseline) {
		Component::mark_drawn_rect_dirty(self);
		self.rich_text.set_text_baseline(text_baseline);
	}

	//==============================================================================
	/// 세로 기준선 반환.
	///
	/// # 돌려줌
	/// 세로 기준선.
	//==============================================================================
	pub fn get_text_baseline(&self) -> TextBaseline {
		self.rich_text.get_text_baseline()
	}
}

impl Default for UILabel {
	//==============================================================================
	/// 생성. (UILabel::new 와 같다)
	//==============================================================================
	fn default() -> UILabel {
		UILabel::new()
	}
}

#[crate::reflect]
impl Component for UILabel {
	//==============================================================================
	/// 품은 기반 컴포넌트. (원본 상위 클래스 UIView 자리, get_component::<UIView>() 로도 찾힌다)
	//==============================================================================
	fn get_base_component(&self) -> Option<&dyn Component> {
		Some(&self.ui_view)
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (변경용)
	//==============================================================================
	fn get_base_component_mut(&mut self) -> Option<&mut dyn Component> {
		Some(&mut self.ui_view)
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (품은 뷰의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_view.get_component_base()
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (품은 뷰의 것, 변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_view.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("UILabel")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UILabel"
	}

	//==============================================================================
	/// 자연 크기. (품은 글의 자연 크기, UINode 가 모은다. 원본은 노드에 붙인 Text, RichText 의 값)
	//==============================================================================
	#[reflect(skip)]
	fn get_intrinsic_content_size(&self) -> Vector2 {
		if !self.is_enable() {
			return Vector2::create(-1.0, -1.0);
		}
		self.rich_text.get_intrinsic_content_size()
	}

	//==============================================================================
	/// 트리에 붙음. (호스트 아래 content 노드를 만든다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_view.create_content_node(node_tree, node_id);
	}

	//==============================================================================
	/// tick 이 필요 없다. (false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 노드 content_size 영역 안에만 그리는지. (원본 isDrawnInsideContent 처럼 true. 화면 밖 거르기는 calculate_drawn_rect 가 글 사각형으로 한다)
	//==============================================================================
	#[reflect(after = "getFittedFontSize")]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 그릴 영역 셈. (품은 Text 로 그리면 content 영역과 글 사각형을 합친 것, 리치텍스트 모드면 None 이라 거르지 않는다)
	//==============================================================================
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

	//==============================================================================
	/// 출력. (배경색을 먼저, 리치텍스트 모드면 RichText, 아니면 품은 Text)
	//==============================================================================
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
	/// 세터가 품은 Text 에 그대로 반영되고 content 노드가 생긴다.
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
	/// 리치텍스트: 글을 넣으면 품은 RichText 가 마크업을 풀고, 세터는 RichText 의 Text 에 닿는다. 조각을 코드로 더할 수도 있다.
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
