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
/// - UIView 를 품고 content 노드와 배경색을 넘겨준다. 터치를 받는 위젯(UIButton, UISlider)이 이것을 품는다.
///
/// # 예제
/// 노드에 붙여 content 노드와 배경색 쓰기
/// ```rust
/// use vanilla::{Color, NodeTree, UIControl};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let control_id = node_tree.create_child(root_id, "control");
/// let control = node_tree.add_component(control_id, UIControl::new()).unwrap();
/// control.set_background_color("#334455");
/// assert!(control.get_content().is_some());
/// assert_eq!(control.get_background_color(), Color::from("#334455"));
/// assert_eq!(control.get_uiview().get_background_color(), control.get_background_color());
/// ```
//==============================================================================
pub struct UIControl {
	ui_view: UIView,
}

#[crate::reflect(base = ui_view)]
impl UIControl {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 컨트롤. (content 노드는 트리에 붙일 때 만든다)
	//==============================================================================
	pub fn new() -> UIControl {
		UIControl {
			ui_view: UIView::new(),
		}
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
	/// - `background_color`: 배경색. (알파가 0 이면 칠하지 않는다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
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
}

impl Default for UIControl {
	//==============================================================================
	/// 생성. (UIControl::new 와 같다)
	//==============================================================================
	fn default() -> UIControl {
		UIControl::new()
	}
}

#[crate::reflect]
impl Component for UIControl {
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
	/// 컴포넌트 종류 이름 반환. ("UIControl")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIControl"
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
	/// 노드 content_size 영역 안에만 그린다. (true)
	//==============================================================================
	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (배경색)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.ui_view.draw_background(transform, graphic);
	}
}
