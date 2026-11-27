//==============================================================================
//! 뷰 컴포넌트. (원본 ui/uiview.js 의 UIView)
//! - 모든 UI 위젯의 바탕. 노드(호스트)에 붙으면 호스트 아래에 "content" 자식 노드를 만든다. (피봇 왼쪽 위, 앵커 0,0, 자리 0,0)
//! - 배경색의 알파가 0 보다 크면 호스트 content_size 영역을 그 색으로 칠한다.
//! - 원본은 UILabel, UIButton 같은 위젯이 UIView 를 상속한다. 러스트 판은 위젯이 UIView 를 품고(ui_view 필드) content, 배경색을 넘겨준다.
//!   그래서 원본의 node.get_component(UIView) 로 하위 위젯까지 한 번에 찾는 쓰임은 없다. (`get_component::<UIButton>()` 처럼 구체 타입으로 찾는다)
//! - content 노드는 attach_to_tree 에서 만든다. node_tree.add_component(node_id, UIView::new()) 로 붙여야 한다.
//! - 원본의 setEngine / get_engine 은 옮기지 않았다. 엔진은 tick / 터치 때 ComponentContext::get_engine 으로 얻는다.
//! - 원본 setBackgroundColor(null) 은 배경을 지운다. (처음 값인 알파 0 흰색으로 돌린다) 러스트 판은 None 자리가 없어 알파 0 색(처음 값이나 Color::transparent())을 준다.
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
/// 뷰 컴포넌트.
/// - 노드에 붙이면 호스트 아래 content 자식 노드를 만들고, 배경색으로 호스트 content_size 영역을 칠한다.
///
/// # 예제
/// 반투명 배경 판과 그 content 노드
/// ```rust
/// use vanilla::{Color, NodeTree, UIView, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let panel_id = node_tree.create_child(root_id, "panel");
/// node_tree[panel_id].set_content_size(Vector2::create(200.0, 100.0));
/// let panel_view = node_tree.add_component(panel_id, UIView::new()).unwrap();
/// panel_view.set_background_color(Color::create(0.1, 0.1, 0.2, 0.8));
/// let content_id = panel_view.get_content().unwrap();
/// assert_eq!(node_tree.get_parent_id(content_id), Some(panel_id));
/// assert_eq!(node_tree[content_id].get_name(), "content");
/// ```
//==============================================================================
pub struct UIView {
	component_base: ComponentBase,
	content_id: Option<NodeId>,
	background_color: Color,
}

#[crate::reflect]
impl UIView {
	//==============================================================================
	/// 생성. (배경은 투명한 흰색)
	///
	/// # 돌려줌
	/// 새 뷰. (content 노드는 트리에 붙일 때 만든다)
	//==============================================================================
	pub fn new() -> UIView {
		UIView {
			component_base: ComponentBase::new(),
			content_id: None,
			background_color: Color::create(1.0, 1.0, 1.0, 0.0),
		}
	}

	//==============================================================================
	/// 컨텐트 노드 만들기. (호스트의 자식, 왼쪽 위 정렬. 이미 있으면 그대로 둔다)
	///
	/// # 인자
	/// - `node_tree`: 호스트가 든 트리.
	/// - `node_id`: 호스트 노드. (content 노드가 이 노드의 자식이 된다)
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
	/// 컨텐트 노드 핸들. (자식 노드를 여기에 붙이면 스크롤 대상이 된다. 트리에 붙기 전이면 None)
	///
	/// # 돌려줌
	/// content 노드. 트리에 붙기 전이면 None.
	//==============================================================================
	pub fn get_content(&self) -> Option<NodeId> {
		self.content_id
	}

	//==============================================================================
	/// 배경색 설정. (알파가 0 이면 그리지 않는다. 원본 null 처럼 배경을 지우려면 알파 0 색을 준다)
	///
	/// # 인자
	/// - `background_color`: 배경색. (Color 나 "#rrggbb" 같은 글, 알파가 0 이면 칠하지 않는다)
	//==============================================================================
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.background_color = background_color.into();
	}

	//==============================================================================
	/// 배경색 반환.
	///
	/// # 돌려줌
	/// 배경색. (기본은 알파 0 인 흰색)
	//==============================================================================
	pub fn get_background_color(&self) -> Color {
		self.background_color
	}

	//==============================================================================
	/// 엔진 반환. (원본 getEngine, 원본 setEngine 으로 준 엔진 자리. 러스트 판은 엔진이 하나라 전역 엔진을 준다)
	/// - Engine::get_engine 과 같다. 엔진이 &Engine 으로 빌려 준 동안(tick, 터치, 휠, 위젯 콜백) 꺼내고, 그 밖은 까닭을 돌려준다.
	///
	/// # 돌려줌
	/// 전역 엔진, 못 꺼내면 그 까닭.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_engine(&self) -> Result<crate::core::globalengine::EngineRef, crate::core::globalengine::EngineAccessError> {
		crate::core::globalengine::get_global_engine()
	}

	//==============================================================================
	/// 배경 출력. (투명한 바탕은 그리지 않는다. 보이지 않는 드로우가 묶어 그리기를 끊지 않게)
	///
	/// # 인자
	/// - `transform`: 호스트 노드의 변환. (content_size 영역 (0, 0) ~ (너비, 높이) 를 칠한다)
	/// - `graphic`: 그릴 그래픽.
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
	//==============================================================================
	/// 생성. (UIView::new 와 같다)
	//==============================================================================
	fn default() -> UIView {
		UIView::new()
	}
}

#[crate::reflect]
impl Component for UIView {
	//==============================================================================
	/// 컴포넌트 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("View")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"View"
	}

	//==============================================================================
	/// 트리에 붙음. (호스트 아래 content 노드를 만든다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.create_content_node(node_tree, node_id);
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
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (배경색)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_background(transform, graphic);
	}
}

//==============================================================================
/// 뷰 좌표의 점이 노드 content_size 영역 안에 있는지. (원본 WorldNode.contains)
///
/// # 인자
/// - `node_tree`: 노드가 든 트리.
/// - `node_id`: 볼 노드.
/// - `view_position`: 뷰 좌표의 점. (픽셀)
///
/// # 돌려줌
/// 노드까지 누적한 월드 행렬로 본 노드 content_size 영역 안이면 true. 노드가 없으면 false.
///
/// # 예제
/// 가운데 피봇(기본)인 40 x 20 노드의 안과 밖
/// ```rust
/// use vanilla::ui::uiview::is_view_position_inside_node;
/// use vanilla::{NodeTree, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let button_id = node_tree.create_child(root_id, "button");
/// node_tree[button_id].set_local_position(Vector2::create(100.0, 100.0));
/// node_tree[button_id].set_content_size(Vector2::create(40.0, 20.0));
/// assert!(is_view_position_inside_node(&node_tree, button_id, Vector2::create(100.0, 100.0)));
/// assert!(is_view_position_inside_node(&node_tree, button_id, Vector2::create(81.0, 91.0)));
/// assert!(!is_view_position_inside_node(&node_tree, button_id, Vector2::create(79.0, 100.0)));
/// ```
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
	/// 트리에 붙이면 호스트 아래에 content 노드가 생긴다.
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
	/// WorldNode::add_component 로 붙이면 content 노드가 없다.
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
	/// 배경색은 처음에 알파 0 흰색(그리지 않음)이고, 칠한 뒤 알파 0 색을 주면 다시 지워진다. (원본 setBackgroundColor(null) 자리)
	//==============================================================================
	#[test]
	fn background_color_can_be_cleared() {
		let mut view = UIView::new();
		let initial_color = view.get_background_color();
		assert_eq!(initial_color, Color::create(1.0, 1.0, 1.0, 0.0));
		view.set_background_color(Color::create(0.1, 0.12, 0.16, 1.0));
		assert_eq!(view.get_background_color().alpha, 1.0);
		view.set_background_color(initial_color);
		assert_eq!(view.get_background_color(), Color::create(1.0, 1.0, 1.0, 0.0));
		view.set_background_color(Color::transparent());
		assert!(view.get_background_color().alpha <= 0.0);
	}

	//==============================================================================
	/// 영역 안 판정. (피봇과 부모 위치를 따른다)
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
