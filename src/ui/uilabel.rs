//==============================================================================
// UI 라벨. (원본 ui/uilabel.js 의 UILabel)
// - 표시 전용 위젯. 호스트 contentSize 영역 안에 정렬해 글자를 그린다. 세터는 원본처럼 글자 컴포넌트에 넘긴다.
// - 원본은 require() 로 호스트에 Text 와 RichText 를 붙이고 그 참조를 든다. 러스트 판은 RichText 하나를 품고, RichText 가 Text 를 품는다.
//   (Sprite 가 Paint 를 품는 것과 같은 꼴) 씬 코드가 tick 밖에서 label.setText 를 불러도 바로 반영되게 하려는 것이다.
//   형제 Text 를 두면 tick 안에서만 닿아 한 프레임 늦는다. 그래서 라벨 노드의 getComponent::<Text>() 는 None 이다.
//   품은 Text 는 getTextComponent / getTextComponentMut, RichText 는 getRichTextComponent / getRichTextComponentMut 로 꺼낸다.
// - 원본은 Text 와 RichText 두 벌에 세터를 같이 걸고 useRichText 로 어느 쪽을 켤지 고른다. 러스트 판은 한 벌이라 맞출 일이 없다.
//   useRichText(true) 면 RichText 로(마크업 해석), 아니면 품은 Text 로 그린다. appendSegment / clearSegments 는 RichText 에 넘긴다.
// - 원본 setFont(FontFace) 는 setFontFamily(등록한 폰트 이름) 로 바꿨다.
// - 원본에서 옮기지 않은 것: 밑줄 / 취소선, 글자 방향, 줄바꿈 방식(word / char), 표시 글자 수 (Text 에 없다),
//   invalidateIntrinsicContentSize (UINode 노선 전용).
// - 배경색(UIView)은 글자보다 먼저 그린다. (원본은 Text 가 먼저 붙어 배경이 글자를 덮는다. 기본 배경이 투명이라 보통은 차이가 없다)
//==============================================================================
use crate::base::color::Color;
use crate::core::component::richtext::{RichText, RichTextAttributes};
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiview::UIView;

//==============================================================================
// UI 라벨.
//==============================================================================
pub struct UILabel {
	uiView: UIView,
	richText: RichText,
	useRichText: bool,
}

impl UILabel {
	//==============================================================================
	// 생성. (글자 기본값은 Text 와 같다: 32px, 흰색, 가운데 정렬, 가운데 기준선)
	//==============================================================================
	pub fn new() -> UILabel {
		UILabel {
			uiView: UIView::new(),
			richText: RichText::new(),
			useRichText: false,
		}
	}

	//==============================================================================
	// 글과 크기로 생성.
	//==============================================================================
	pub fn createWithText(text: &str, fontSize: f32) -> UILabel {
		let mut label = UILabel::new();
		label.setText(text);
		label.setFontSize(fontSize);
		label
	}

	//==============================================================================
	// 품은 뷰 반환.
	//==============================================================================
	pub fn getUIView(&self) -> &UIView {
		&self.uiView
	}

	//==============================================================================
	// 품은 뷰 반환. (변경용)
	//==============================================================================
	pub fn getUIViewMut(&mut self) -> &mut UIView {
		&mut self.uiView
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.uiView.getContent()
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	pub fn setBackgroundColor(&mut self, backgroundColor: impl Into<Color>) {
		self.uiView.setBackgroundColor(backgroundColor);
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	pub fn getBackgroundColor(&self) -> Color {
		self.uiView.getBackgroundColor()
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (원본의 node.getComponent(Text) 자리)
	//==============================================================================
	pub fn getTextComponent(&self) -> &Text {
		self.richText.getTextComponent()
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (변경용)
	//==============================================================================
	pub fn getTextComponentMut(&mut self) -> &mut Text {
		self.richText.getTextComponentMut()
	}

	//==============================================================================
	// 품은 리치텍스트 컴포넌트 반환. (원본의 node.getComponent(RichText) 자리)
	//==============================================================================
	pub fn getRichTextComponent(&self) -> &RichText {
		&self.richText
	}

	//==============================================================================
	// 품은 리치텍스트 컴포넌트 반환. (변경용)
	//==============================================================================
	pub fn getRichTextComponentMut(&mut self) -> &mut RichText {
		&mut self.richText
	}

	//==============================================================================
	// 리치텍스트 조각 하나 더하기. (useRichText(true) 일 때 보인다)
	//==============================================================================
	pub fn appendSegment(&mut self, text: &str, attributes: RichTextAttributes) -> &mut UILabel {
		self.richText.appendSegment(text, attributes);
		self
	}

	//==============================================================================
	// 리치텍스트 조각 모두 지우기.
	//==============================================================================
	pub fn clearSegments(&mut self) {
		self.richText.clearSegments();
	}

	//==============================================================================
	// 리치텍스트 모드 설정. (true 면 마크업을 풀어 조각마다 다르게 그린다)
	//==============================================================================
	pub fn useRichText(&mut self, useRichText: bool) {
		self.useRichText = useRichText;
	}

	//==============================================================================
	// 리치텍스트 모드 여부.
	//==============================================================================
	pub fn isUsingRichText(&self) -> bool {
		self.useRichText
	}

	//==============================================================================
	// 글 설정.
	//==============================================================================
	pub fn setText(&mut self, text: &str) {
		self.richText.setText(text);
	}

	//==============================================================================
	// 글 반환.
	//==============================================================================
	pub fn getText(&self) -> &str {
		self.richText.getText()
	}

	//==============================================================================
	// 글자 크기 설정.
	//==============================================================================
	pub fn setFontSize(&mut self, fontSize: f32) {
		self.richText.setFontSize(fontSize);
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn getFontSize(&self) -> f32 {
		self.richText.getFontSize()
	}

	//==============================================================================
	// 폰트 이름 설정. (원본 setFont 자리. Graphic 에 등록한 이름, 없으면 기본 폰트)
	//==============================================================================
	pub fn setFontFamily(&mut self, fontFamily: &str) {
		self.richText.setFontFamily(fontFamily);
	}

	//==============================================================================
	// 폰트 이름 반환.
	//==============================================================================
	pub fn getFontFamily(&self) -> &str {
		self.richText.getFontFamily()
	}

	//==============================================================================
	// 자동 줄바꿈 폭 설정. (0 이면 줄바꿈 문자로만 나눈다)
	//==============================================================================
	pub fn setWordWrapWidth(&mut self, wordWrapWidth: f32) {
		self.richText.setWordWrapWidth(wordWrapWidth);
	}

	//==============================================================================
	// 줄 간격 배율 설정.
	//==============================================================================
	pub fn setLineSpacing(&mut self, lineSpacing: f32) {
		self.richText.setLineSpacing(lineSpacing);
	}

	//==============================================================================
	// 글자 색 설정.
	//==============================================================================
	pub fn setTextColor(&mut self, color: impl Into<Color>) {
		self.richText.setTextColor(color);
	}

	//==============================================================================
	// 글자 색 반환.
	//==============================================================================
	pub fn getTextColor(&self) -> Color {
		self.richText.getTextColor()
	}

	//==============================================================================
	// 외곽선 색 설정.
	//==============================================================================
	pub fn setStrokeColor(&mut self, color: impl Into<Color>) {
		self.richText.setStrokeColor(color);
	}

	//==============================================================================
	// 외곽선 두께 설정.
	//==============================================================================
	pub fn setStrokeWidth(&mut self, strokeWidth: f32) {
		self.richText.setStrokeWidth(strokeWidth);
	}

	//==============================================================================
	// 굵게 설정.
	//==============================================================================
	pub fn setBold(&mut self, isBold: bool) {
		self.richText.setBold(isBold);
	}

	//==============================================================================
	// 굵게 여부.
	//==============================================================================
	pub fn isBold(&self) -> bool {
		self.richText.isBold()
	}

	//==============================================================================
	// 기울임 설정. (Text 처럼 값만 든다)
	//==============================================================================
	pub fn setItalic(&mut self, isItalic: bool) {
		self.richText.setItalic(isItalic);
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn isItalic(&self) -> bool {
		self.richText.isItalic()
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn setTextAlign(&mut self, textAlign: TextAlign) {
		self.richText.setTextAlign(textAlign);
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn getTextAlign(&self) -> TextAlign {
		self.richText.getTextAlign()
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn setTextBaseline(&mut self, textBaseline: TextBaseline) {
		self.richText.setTextBaseline(textBaseline);
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn getTextBaseline(&self) -> TextBaseline {
		self.richText.getTextBaseline()
	}
}

impl Default for UILabel {
	fn default() -> UILabel {
		UILabel::new()
	}
}

impl Component for UILabel {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiView.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiView.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"UILabel"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiView.createContentNode(nodeTree, nodeId);
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.uiView.drawBackground(transform, graphic);
		if self.useRichText {
			self.richText.draw(transform, graphic);
		}
		else {
			self.richText.getTextComponentMut().draw(transform, graphic);
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
	fn settersReachTextAndContentIsCreated() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let labelId = nodeTree.createChild(rootId, "label");
		nodeTree[labelId].setContentSize(Vector2::create(200.0, 40.0));
		let label = nodeTree.addComponent(labelId, UILabel::createWithText("hello", 20.0)).expect("label");
		label.setTextColor("#ff0000");
		label.setTextAlign(TextAlign::left);
		label.setBold(true);
		label.useRichText(true);
		assert_eq!(label.getText(), "hello");
		assert_eq!(label.getFontSize(), 20.0);
		assert_eq!(label.getTextComponent().getTextColor(), Color::create(1.0, 0.0, 0.0, 1.0));
		assert_eq!(label.getTextAlign(), TextAlign::left);
		assert!(label.isBold());
		assert!(label.isUsingRichText());
		let contentId = label.getContent().expect("content");
		assert_eq!(nodeTree.getParentId(contentId), Some(labelId));
	}

	//==============================================================================
	// 리치텍스트: 글을 넣으면 품은 RichText 가 마크업을 풀고, 세터는 RichText 의 Text 에 닿는다. 조각을 코드로 더할 수도 있다.
	//==============================================================================
	#[test]
	fn richTextPathParsesMarkup() {
		let mut label = UILabel::createWithText("<b>굵게</b> 보통 <color=red>빨강</color>", 24.0);
		label.useRichText(true);
		label.setTextColor("#00ff00");
		let segments = label.getRichTextComponent().getSegments();
		assert_eq!(segments.len(), 3);
		assert!(segments[0].attributes.isBold);
		assert_eq!(segments[2].text, "빨강");
		assert_eq!(label.getRichTextComponent().getTextColor(), Color::create(0.0, 1.0, 0.0, 1.0));
		assert_eq!(label.getText(), "<b>굵게</b> 보통 <color=red>빨강</color>");
		label.clearSegments();
		label.appendSegment("조각", RichTextAttributes::new());
		assert_eq!(label.getRichTextComponent().getSegments().len(), 1);
	}
}
