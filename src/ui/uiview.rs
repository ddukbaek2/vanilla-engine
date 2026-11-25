//==============================================================================
// 뷰 컴포넌트. (원본 ui/uiview.js 의 UIView)
// - 모든 UI 위젯의 바탕. 노드(호스트)에 붙으면 호스트 아래에 "content" 자식 노드를 만든다. (피봇 왼쪽 위, 앵커 0,0, 자리 0,0)
// - 배경색의 알파가 0 보다 크면 호스트 content_size 영역을 그 색으로 칠한다.
// - 원본은 UILabel, UIButton 같은 위젯이 UIView 를 상속한다. 러스트 판은 위젯이 UIView 를 품고(ui_view 필드) content, 배경색을 넘겨준다.
//   그래서 원본의 node.get_component(UIView) 로 하위 위젯까지 한 번에 찾는 쓰임은 없다. (get_component::<UIButton>() 처럼 구체 타입으로 찾는다)
// - content 노드는 attach_to_tree 에서 만든다. node_tree.add_component(node_id, UIView::new()) 로 붙여야 한다.
// - 원본의 setEngine / get_engine 은 옮기지 않았다. 엔진은 tick / 터치 때 ComponentContext::get_engine 으로 얻는다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
// 뷰 컴포넌트.
//==============================================================================
pub struct UIView {
	component_base: ComponentBase,
	content_id: Option<NodeId>,
	background_color: Color,
}

#[crate::reflect]
impl UIView {
	//==============================================================================
	// 생성. (배경은 투명한 흰색)
	//==============================================================================
	pub fn new() -> UIView {
		UIView {
			component_base: ComponentBase::new(),
			content_id: None,
			background_color: Color::create(1.0, 1.0, 1.0, 0.0),
		}
	}

	//==============================================================================
	// 컨텐트 노드 만들기. (호스트의 자식, 왼쪽 위 정렬. 이미 있으면 그대로 둔다)
	//==============================================================================
	pub fn create_content_node(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		if let Some(content_id) = self.content_id {
			if node_tree.contains(content_id) {
				return;
			}
		}
		let content_id = node_tree.create_child(node_id, "content");
		let content = &mut node_tree[content_id];
		content.set_pivot(Pivot::TOP_LEFT);
		content.set_anchor(Vector2::zero());
		content.set_local_position(Vector2::zero());
		self.content_id = Some(content_id);
	}

	//==============================================================================
	// 컨텐트 노드 핸들. (자식 노드를 여기에 붙이면 스크롤 대상이 된다. 트리에 붙기 전이면 None)
	//==============================================================================
	pub fn get_content(&self) -> Option<NodeId> {
		self.content_id
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.background_color = background_color.into();
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	pub fn get_background_color(&self) -> Color {
		self.background_color
	}

	//==============================================================================
	// 배경 출력. (투명한 바탕은 그리지 않는다. 보이지 않는 드로우가 묶어 그리기를 끊지 않게)
	//==============================================================================
	pub fn draw_background(&self, transform: &NodeTransform, graphic: &mut Graphic) {
		let background_color = self.get_background_color();
		if background_color.alpha <= 0.0 {
			return;
		}
		let content_size = transform.get_content_size();
		let background_rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);
		graphic.set_fill_color(background_color);
		graphic.draw_rect(background_rect);
	}
}

impl Default for UIView {
	fn default() -> UIView {
		UIView::new()
	}
}

#[crate::reflect]
impl Component for UIView {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"View"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.create_content_node(node_tree, node_id);
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_background(transform, graphic);
	}
}

//==============================================================================
// 뷰 좌표의 점이 노드 content_size 영역 안에 있는지. (원본 WorldNode.contains)
//==============================================================================
pub fn is_view_position_inside_node(node_tree: &NodeTree, node_id: NodeId, view_position: Vector2) -> bool {
	let node = match node_tree.get_node(node_id) {
		Some(node) => {
			node
		}
		None => {
			return false;
		}
	};
	let world_matrix = node_tree.calculate_world_matrix(node_id);
	node.contains_point(&world_matrix, view_position)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 트리에 붙이면 호스트 아래에 content 노드가 생긴다.
	//==============================================================================
	#[test]
	fn content_node_is_created_on_attach() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let host_id = node_tree.create_child(root_id, "host");
		node_tree[host_id].set_content_size(Vector2::create(200.0, 100.0));
		let content_id = node_tree.add_component(host_id, UIView::new()).expect("view").get_content().expect("content");
		assert_eq!(node_tree.get_parent_id(content_id), Some(host_id));
		assert_eq!(node_tree[content_id].get_name(), "content");
		assert_eq!(node_tree[content_id].get_pivot(), Pivot::TOP_LEFT);
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::zero());
	}

	//==============================================================================
	// WorldNode::add_component 로 붙이면 content 노드가 없다.
	//==============================================================================
	#[test]
	fn plain_add_component_has_no_content() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let host_id = node_tree.create_child(root_id, "host");
		let content_id = node_tree[host_id].add_component(UIView::new()).get_content();
		assert!(content_id.is_none());
		assert_eq!(node_tree.get_child_count(host_id), 0);
	}

	//==============================================================================
	// 영역 안 판정. (피봇과 부모 위치를 따른다)
	//==============================================================================
	#[test]
	fn inside_test() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let host_id = node_tree.create_child(root_id, "host");
		let host = &mut node_tree[host_id];
		host.set_local_position(Vector2::create(100.0, 100.0));
		host.set_content_size(Vector2::create(40.0, 20.0));
		assert!(is_view_position_inside_node(&node_tree, host_id, Vector2::create(100.0, 100.0)));
		assert!(is_view_position_inside_node(&node_tree, host_id, Vector2::create(81.0, 91.0)));
		assert!(!is_view_position_inside_node(&node_tree, host_id, Vector2::create(79.0, 100.0)));
	}
}
