//==============================================================================
// 뷰 컴포넌트. (원본 ui/uiview.js 의 UIView)
// - 모든 UI 위젯의 바탕. 노드(호스트)에 붙으면 호스트 아래에 "content" 자식 노드를 만든다. (피봇 왼쪽 위, 앵커 0,0, 자리 0,0)
// - 배경색의 알파가 0 보다 크면 호스트 contentSize 영역을 그 색으로 칠한다.
// - 원본은 UILabel, UIButton 같은 위젯이 UIView 를 상속한다. 러스트 판은 위젯이 UIView 를 품고(uiView 필드) content, 배경색을 넘겨준다.
//   그래서 원본의 node.getComponent(UIView) 로 하위 위젯까지 한 번에 찾는 쓰임은 없다. (getComponent::<UIButton>() 처럼 구체 타입으로 찾는다)
// - content 노드는 attachToTree 에서 만든다. nodeTree.addComponent(nodeId, UIView::new()) 로 붙여야 한다.
// - 원본의 setEngine / getEngine 은 옮기지 않았다. 엔진은 tick / 터치 때 ComponentContext::getEngine 으로 얻는다.
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
	componentBase: ComponentBase,
	contentId: Option<NodeId>,
	backgroundColor: Color,
}

impl UIView {
	//==============================================================================
	// 생성. (배경은 투명한 흰색)
	//==============================================================================
	pub fn new() -> UIView {
		UIView {
			componentBase: ComponentBase::new(),
			contentId: None,
			backgroundColor: Color::create(1.0, 1.0, 1.0, 0.0),
		}
	}

	//==============================================================================
	// 컨텐트 노드 만들기. (호스트의 자식, 왼쪽 위 정렬. 이미 있으면 그대로 둔다)
	//==============================================================================
	pub fn createContentNode(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		if let Some(contentId) = self.contentId {
			if nodeTree.contains(contentId) {
				return;
			}
		}
		let contentId = nodeTree.createChild(nodeId, "content");
		let content = &mut nodeTree[contentId];
		content.setPivot(Pivot::topLeft);
		content.setAnchor(Vector2::zero());
		content.setLocalPosition(Vector2::zero());
		self.contentId = Some(contentId);
	}

	//==============================================================================
	// 컨텐트 노드 핸들. (자식 노드를 여기에 붙이면 스크롤 대상이 된다. 트리에 붙기 전이면 None)
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.contentId
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	pub fn setBackgroundColor(&mut self, backgroundColor: impl Into<Color>) {
		self.backgroundColor = backgroundColor.into();
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	pub fn getBackgroundColor(&self) -> Color {
		self.backgroundColor
	}

	//==============================================================================
	// 배경 출력. (투명한 바탕은 그리지 않는다. 보이지 않는 드로우가 묶어 그리기를 끊지 않게)
	//==============================================================================
	pub fn drawBackground(&self, transform: &NodeTransform, graphic: &mut Graphic) {
		let backgroundColor = self.getBackgroundColor();
		if backgroundColor.alpha <= 0.0 {
			return;
		}
		let contentSize = transform.getContentSize();
		let backgroundRect = Rect::create(0.0, 0.0, contentSize.x, contentSize.y);
		graphic.setFillColor(backgroundColor);
		graphic.drawRect(backgroundRect);
	}
}

impl Default for UIView {
	fn default() -> UIView {
		UIView::new()
	}
}

impl Component for UIView {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"View"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.createContentNode(nodeTree, nodeId);
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.drawBackground(transform, graphic);
	}
}

//==============================================================================
// 뷰 좌표의 점이 노드 contentSize 영역 안에 있는지. (원본 WorldNode.contains)
//==============================================================================
pub fn isViewPositionInsideNode(nodeTree: &NodeTree, nodeId: NodeId, viewPosition: Vector2) -> bool {
	let node = match nodeTree.getNode(nodeId) {
		Some(node) => {
			node
		}
		None => {
			return false;
		}
	};
	let worldMatrix = nodeTree.calculateWorldMatrix(nodeId);
	node.containsPoint(&worldMatrix, viewPosition)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 트리에 붙이면 호스트 아래에 content 노드가 생긴다.
	//==============================================================================
	#[test]
	fn contentNodeIsCreatedOnAttach() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let hostId = nodeTree.createChild(rootId, "host");
		nodeTree[hostId].setContentSize(Vector2::create(200.0, 100.0));
		let contentId = nodeTree.addComponent(hostId, UIView::new()).expect("view").getContent().expect("content");
		assert_eq!(nodeTree.getParentId(contentId), Some(hostId));
		assert_eq!(nodeTree[contentId].getName(), "content");
		assert_eq!(nodeTree[contentId].getPivot(), Pivot::topLeft);
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::zero());
	}

	//==============================================================================
	// WorldNode::addComponent 로 붙이면 content 노드가 없다.
	//==============================================================================
	#[test]
	fn plainAddComponentHasNoContent() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let hostId = nodeTree.createChild(rootId, "host");
		let contentId = nodeTree[hostId].addComponent(UIView::new()).getContent();
		assert!(contentId.is_none());
		assert_eq!(nodeTree.getChildCount(hostId), 0);
	}

	//==============================================================================
	// 영역 안 판정. (피봇과 부모 위치를 따른다)
	//==============================================================================
	#[test]
	fn insideTest() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let hostId = nodeTree.createChild(rootId, "host");
		let host = &mut nodeTree[hostId];
		host.setLocalPosition(Vector2::create(100.0, 100.0));
		host.setContentSize(Vector2::create(40.0, 20.0));
		assert!(isViewPositionInsideNode(&nodeTree, hostId, Vector2::create(100.0, 100.0)));
		assert!(isViewPositionInsideNode(&nodeTree, hostId, Vector2::create(81.0, 91.0)));
		assert!(!isViewPositionInsideNode(&nodeTree, hostId, Vector2::create(79.0, 100.0)));
	}
}
