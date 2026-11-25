//==============================================================================
//! UI 컨트롤. (원본 ui/uicontrol.js 의 UIControl)
//! - 사용자 입력(터치)을 받는 위젯의 바탕. UIKit 의 UIControl 처럼 UIView 위에 선다.
//! - 원본은 UIView 를 상속하고 이름만 바꾼다. 러스트 판은 UIView 를 품고 content, 배경색을 넘겨준다.
//!   UIButton, UISlider 가 이것을 품는다.
//==============================================================================
use crate::base::color::Color;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiview::UIView;

//==============================================================================
/// UI 컨트롤.
//==============================================================================
pub struct UIControl {
	ui_view: UIView,
}

#[crate::reflect(base = ui_view)]
impl UIControl {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> UIControl {
		UIControl {
			ui_view: UIView::new(),
		}
	}

	//==============================================================================
	/// 품은 뷰 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uiview(&self) -> &UIView {
		&self.ui_view
	}

	//==============================================================================
	/// 품은 뷰 반환. (변경용)
	//==============================================================================
	pub fn get_uiview_mut(&mut self) -> &mut UIView {
		&mut self.ui_view
	}

	//==============================================================================
	/// 컨텐트 노드 핸들.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_view.get_content()
	}

	//==============================================================================
	/// 배경색 설정.
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.ui_view.set_background_color(background_color);
	}

	//==============================================================================
	/// 배경색 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_background_color(&self) -> Color {
		self.ui_view.get_background_color()
	}
}

impl Default for UIControl {
	fn default() -> UIControl {
		UIControl::new()
	}
}

#[crate::reflect]
impl Component for UIControl {
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_view.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_view.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"UIControl"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_view.create_content_node(node_tree, node_id);
	}

	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.ui_view.draw_background(transform, graphic);
	}
}
