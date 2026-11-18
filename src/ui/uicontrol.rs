//==============================================================================
// UI 컨트롤. (원본 ui/uicontrol.js 의 UIControl)
// - 사용자 입력(터치)을 받는 위젯의 바탕. UIKit 의 UIControl 처럼 UIView 위에 선다.
// - 원본은 UIView 를 상속하고 이름만 바꾼다. 러스트 판은 UIView 를 품고 content, 배경색을 넘겨준다.
//   UIButton, UISlider 가 이것을 품는다.
//==============================================================================
use crate::base::color::Color;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiview::UIView;

//==============================================================================
// UI 컨트롤.
//==============================================================================
pub struct UIControl {
	uiView: UIView,
}

impl UIControl {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIControl {
		UIControl {
			uiView: UIView::new(),
		}
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
}

impl Default for UIControl {
	fn default() -> UIControl {
		UIControl::new()
	}
}

impl Component for UIControl {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiView.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiView.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"UIControl"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiView.createContentNode(nodeTree, nodeId);
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.uiView.drawBackground(transform, graphic);
	}
}
