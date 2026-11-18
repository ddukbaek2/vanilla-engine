//==============================================================================
// 드래그 가능 컴포넌트. (원본 ui/uidraggable.js 의 UIDraggable)
// - 노드에 붙이면 눌러서 끌 수 있다. 놓으면 끝 이벤트로 알리고, setSnapBackEnabled(true) 면 잡기 전 자리로 미끄러져 돌아간다.
// - 이동량은 뷰 좌표 차이를 그대로 로컬 좌표에 더한다. (조상 노드에 배율이 있으면 감도가 달라진다. 원본과 같음)
// - 원본은 UIControl 을 상속한다. 러스트 판은 UIControl 을 품고(uiControl 필드) content, 배경색을 넘겨준다.
//   attachToTree 에서 호스트 노드를 interactable 로 켠다. nodeTree.addComponent(nodeId, UIDraggable::new()) 로 붙인다.
// - 사용:
//     let draggable = nodeTree.addComponent(cardId, UIDraggable::new()).expect("draggable");
//     draggable.setSnapBackEnabled(true);
//     draggable.setDragEndEvent(Box::new(|draggable, context, viewPosition| { if 놓을 자리면 { draggable.settleHere(context.getNodeTreeMut()); } }));
// - 원본과 다른 것:
//   - 이벤트는 드래그 컴포넌트와 문맥을 받는다. (이동 / 끝은 뷰 좌표도) 원본 콜백은 컴포넌트(와 뷰 좌표)만 받는다.
//   - settleHere / snapHome 은 노드 자리를 읽고 써야 해서 트리를 받는다. (끝 이벤트 안에서는 context.getNodeTreeMut())
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uicontrol::UIControl;

//==============================================================================
// 드래그 이벤트. (드래그 컴포넌트와 문맥을 받는다. 시작 이벤트)
//==============================================================================
pub type UIDraggableEvent = Box<dyn FnMut(&mut UIDraggable, &mut ComponentContext)>;

//==============================================================================
// 드래그 자리 이벤트. (드래그 컴포넌트, 문맥, 뷰 좌표. 이동 / 끝 이벤트)
//==============================================================================
pub type UIDraggablePositionEvent = Box<dyn FnMut(&mut UIDraggable, &mut ComponentContext, Vector2)>;

//==============================================================================
// 드래그 가능 컴포넌트.
//==============================================================================
pub struct UIDraggable {
	uiControl: UIControl,
	nodeId: Option<NodeId>,
	isDragging: bool,
	previousViewPosition: Option<Vector2>,
	homePosition: Option<Vector2>,
	isSnapBackEnabled: bool,
	isReturning: bool,
	snapBackSpeed: f32,
	dragStartEvent: Option<UIDraggableEvent>,
	dragMoveEvent: Option<UIDraggablePositionEvent>,
	dragEndEvent: Option<UIDraggablePositionEvent>,
}

impl UIDraggable {
	//==============================================================================
	// 생성. (복귀 꺼짐, 복귀 속도 14)
	//==============================================================================
	pub fn new() -> UIDraggable {
		UIDraggable {
			uiControl: UIControl::new(),
			nodeId: None,
			isDragging: false,
			previousViewPosition: None,
			homePosition: None,
			isSnapBackEnabled: false,
			isReturning: false,
			snapBackSpeed: 14.0,
			dragStartEvent: None,
			dragMoveEvent: None,
			dragEndEvent: None,
		}
	}

	//==============================================================================
	// 품은 컨트롤 반환.
	//==============================================================================
	pub fn getUIControl(&self) -> &UIControl {
		&self.uiControl
	}

	//==============================================================================
	// 품은 컨트롤 반환. (변경용)
	//==============================================================================
	pub fn getUIControlMut(&mut self) -> &mut UIControl {
		&mut self.uiControl
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.uiControl.getContent()
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	pub fn setBackgroundColor(&mut self, backgroundColor: impl Into<Color>) {
		self.uiControl.setBackgroundColor(backgroundColor);
	}

	//==============================================================================
	// 시작 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fireDragStartEvent(&mut self, context: &mut ComponentContext) {
		let mut dragStartEvent = match self.dragStartEvent.take() {
			Some(dragStartEvent) => {
				dragStartEvent
			}
			None => {
				return;
			}
		};
		dragStartEvent(self, context);
		if self.dragStartEvent.is_none() {
			self.dragStartEvent = Some(dragStartEvent);
		}
	}

	//==============================================================================
	// 이동 이벤트 부르기.
	//==============================================================================
	fn fireDragMoveEvent(&mut self, context: &mut ComponentContext, viewPosition: Vector2) {
		let mut dragMoveEvent = match self.dragMoveEvent.take() {
			Some(dragMoveEvent) => {
				dragMoveEvent
			}
			None => {
				return;
			}
		};
		dragMoveEvent(self, context, viewPosition);
		if self.dragMoveEvent.is_none() {
			self.dragMoveEvent = Some(dragMoveEvent);
		}
	}

	//==============================================================================
	// 끝 이벤트 부르기.
	//==============================================================================
	fn fireDragEndEvent(&mut self, context: &mut ComponentContext, viewPosition: Vector2) {
		let mut dragEndEvent = match self.dragEndEvent.take() {
			Some(dragEndEvent) => {
				dragEndEvent
			}
			None => {
				return;
			}
		};
		dragEndEvent(self, context, viewPosition);
		if self.dragEndEvent.is_none() {
			self.dragEndEvent = Some(dragEndEvent);
		}
	}

	//==============================================================================
	// 지금 자리에 눌러앉기. (끝 이벤트에서 놓기에 성공했을 때 부른다. 복귀를 멈춘다)
	//==============================================================================
	pub fn settleHere(&mut self, nodeTree: &mut NodeTree) {
		if let Some(node) = self.nodeId.and_then(|nodeId| nodeTree.getNode(nodeId)) {
			self.homePosition = Some(node.getLocalPosition());
		}
		self.isReturning = false;
	}

	//==============================================================================
	// 제자리로 바로 되돌리기.
	//==============================================================================
	pub fn snapHome(&mut self, nodeTree: &mut NodeTree) {
		if let Some(homePosition) = self.homePosition {
			if let Some(node) = self.nodeId.and_then(|nodeId| nodeTree.getNodeMut(nodeId)) {
				node.setLocalPosition(homePosition);
			}
		}
		self.isReturning = false;
	}

	//==============================================================================
	// 놓으면 제자리로 돌아갈지 설정.
	//==============================================================================
	pub fn setSnapBackEnabled(&mut self, isSnapBackEnabled: bool) {
		self.isSnapBackEnabled = isSnapBackEnabled;
	}

	//==============================================================================
	// 복귀 속도 설정. (초당 남은 거리를 줄이는 비율 계수)
	//==============================================================================
	pub fn setSnapBackSpeed(&mut self, snapBackSpeed: f32) {
		self.snapBackSpeed = snapBackSpeed;
	}

	//==============================================================================
	// 시작 이벤트 설정.
	//==============================================================================
	pub fn setDragStartEvent(&mut self, dragStartEvent: UIDraggableEvent) {
		self.dragStartEvent = Some(dragStartEvent);
	}

	//==============================================================================
	// 이동 이벤트 설정.
	//==============================================================================
	pub fn setDragMoveEvent(&mut self, dragMoveEvent: UIDraggablePositionEvent) {
		self.dragMoveEvent = Some(dragMoveEvent);
	}

	//==============================================================================
	// 끝 이벤트 설정.
	//==============================================================================
	pub fn setDragEndEvent(&mut self, dragEndEvent: UIDraggablePositionEvent) {
		self.dragEndEvent = Some(dragEndEvent);
	}

	//==============================================================================
	// 끄는 중인지.
	//==============================================================================
	pub fn isDragging(&self) -> bool {
		self.isDragging
	}

	//==============================================================================
	// 돌아가는 중인지.
	//==============================================================================
	pub fn isReturning(&self) -> bool {
		self.isReturning
	}

	//==============================================================================
	// 잡기 전 로컬 자리. (한 번도 잡지 않았으면 None)
	//==============================================================================
	pub fn getHomePosition(&self) -> Option<Vector2> {
		self.homePosition
	}
}

impl Default for UIDraggable {
	fn default() -> UIDraggable {
		UIDraggable::new()
	}
}

impl Component for UIDraggable {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiControl.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiControl.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"Draggable"
	}

	//==============================================================================
	// 트리에 붙음. (content 노드, interactable)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiControl.getUIViewMut().createContentNode(nodeTree, nodeId);
		nodeTree[nodeId].setInteractable(true);
		self.nodeId = Some(nodeId);
	}

	//==============================================================================
	// 갱신. (제자리로 미끄러지기. 0.5 픽셀 안으로 들어오면 딱 맞춘다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		self.nodeId = Some(context.getNodeId());
		let homePosition = match self.homePosition {
			Some(homePosition) => {
				homePosition
			}
			None => {
				return;
			}
		};
		if !self.isReturning {
			return;
		}
		let node = context.getNodeMut();
		let localPosition = node.getLocalPosition();
		let blend = (self.snapBackSpeed * timeDelta).min(1.0);
		let nextX = localPosition.x + (homePosition.x - localPosition.x) * blend;
		let nextY = localPosition.y + (homePosition.y - localPosition.y) * blend;
		node.setLocalPosition(Vector2::create(nextX, nextY));
		let remainX = homePosition.x - nextX;
		let remainY = homePosition.y - nextY;
		if remainX * remainX + remainY * remainY < 0.25 {
			node.setLocalPosition(homePosition);
			self.isReturning = false;
		}
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.uiControl.draw(transform, graphic);
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	//==============================================================================
	// 터치 눌림. (잡기 전 자리를 기억하고 시작 이벤트)
	//==============================================================================
	fn touchPress(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.isDragging = true;
		self.isReturning = false;
		self.previousViewPosition = Some(viewInputPosition);
		self.homePosition = Some(context.getNode().getLocalPosition());
		self.fireDragStartEvent(context);
	}

	//==============================================================================
	// 터치 이동. (뷰 좌표 차이만큼 옮기고 이동 이벤트)
	//==============================================================================
	fn touchMove(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		if !self.isDragging {
			return;
		}
		let previousViewPosition = self.previousViewPosition.unwrap_or(viewInputPosition);
		let deltaX = viewInputPosition.x - previousViewPosition.x;
		let deltaY = viewInputPosition.y - previousViewPosition.y;
		self.previousViewPosition = Some(viewInputPosition);
		let node = context.getNodeMut();
		let localPosition = node.getLocalPosition();
		node.setLocalPosition(Vector2::create(localPosition.x + deltaX, localPosition.y + deltaY));
		self.fireDragMoveEvent(context, viewInputPosition);
	}

	//==============================================================================
	// 터치 뗌. (복귀를 켰으면 돌아가기 시작하고 끝 이벤트)
	//==============================================================================
	fn touchRelease(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		if !self.isDragging {
			return;
		}
		self.isDragging = false;
		if self.isSnapBackEnabled {
			self.isReturning = true;
		}
		self.fireDragEndEvent(context, viewInputPosition);
	}

	//==============================================================================
	// 터치 취소. (뗌과 같다)
	//==============================================================================
	fn touchCancel(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.touchRelease(context, viewInputPosition);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. ((100, 100) 에 놓은 50 x 50 카드)
	//==============================================================================
	fn createCardTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let cardId = nodeTree.createChild(rootId, "card");
		let cardNode = &mut nodeTree[cardId];
		cardNode.setPivot(Pivot::middleCenter);
		cardNode.setLocalPosition(Vector2::create(100.0, 100.0));
		cardNode.setContentSize(Vector2::create(50.0, 50.0));
		nodeTree.addComponent(cardId, UIDraggable::new());
		(nodeTree, cardId)
	}

	//==============================================================================
	// 드래그 컴포넌트 반환.
	//==============================================================================
	fn getDraggable(nodeTree: &mut NodeTree, cardId: NodeId) -> &mut UIDraggable {
		nodeTree[cardId].getComponentMut::<UIDraggable>().expect("draggable")
	}

	//==============================================================================
	// 끌면 끈 만큼 옮겨지고, 이벤트가 차례로 온다. 복귀를 끄면 놓은 자리에 남는다.
	//==============================================================================
	#[test]
	fn dragMovesNodeAndFiresEvents() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, cardId) = createCardTree();
		assert!(nodeTree[cardId].isInteractable());
		let eventLog = Rc::new(Cell::new(0));
		let startLog = eventLog.clone();
		let moveLog = eventLog.clone();
		let endLog = eventLog.clone();
		{
			let draggable = getDraggable(&mut nodeTree, cardId);
			draggable.setDragStartEvent(Box::new(move |_draggable: &mut UIDraggable, _context: &mut ComponentContext| {
				startLog.set(startLog.get() + 1);
			}));
			draggable.setDragMoveEvent(Box::new(move |_draggable: &mut UIDraggable, _context: &mut ComponentContext, _viewPosition: Vector2| {
				moveLog.set(moveLog.get() + 10);
			}));
			draggable.setDragEndEvent(Box::new(move |_draggable: &mut UIDraggable, _context: &mut ComponentContext, viewPosition: Vector2| {
				assert_eq!(viewPosition, Vector2::create(160.0, 80.0));
				endLog.set(endLog.get() + 100);
			}));
		}
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::press, Vector2::create(100.0, 100.0));
		assert!(getDraggable(&mut nodeTree, cardId).isDragging());
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::moved, Vector2::create(130.0, 90.0));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::moved, Vector2::create(160.0, 80.0));
		assert_eq!(nodeTree[cardId].getLocalPosition(), Vector2::create(160.0, 80.0));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::release, Vector2::create(160.0, 80.0));
		assert_eq!(eventLog.get(), 121);
		assert!(!getDraggable(&mut nodeTree, cardId).isDragging());
		for _ in 0..10 {
			nodeTree.tick(&engine, 0.016);
		}
		assert_eq!(nodeTree[cardId].getLocalPosition(), Vector2::create(160.0, 80.0));
		assert_eq!(getDraggable(&mut nodeTree, cardId).getHomePosition(), Some(Vector2::create(100.0, 100.0)));
	}

	//==============================================================================
	// 복귀를 켜면 놓은 뒤 제자리로 미끄러져 딱 맞춰진다. 끝 이벤트에서 눌러앉으면 돌아가지 않는다.
	//==============================================================================
	#[test]
	fn snapBackAndSettle() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, cardId) = createCardTree();
		getDraggable(&mut nodeTree, cardId).setSnapBackEnabled(true);
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::press, Vector2::create(100.0, 100.0));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::moved, Vector2::create(200.0, 100.0));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::release, Vector2::create(200.0, 100.0));
		assert!(getDraggable(&mut nodeTree, cardId).isReturning());
		nodeTree.tick(&engine, 0.05);
		// 0.05 x 14 = 0.7 만큼 다가간다: 200 → 130.
		assert!((nodeTree[cardId].getLocalPosition().x - 130.0).abs() < 0.001);
		for _ in 0..30 {
			nodeTree.tick(&engine, 0.05);
		}
		assert_eq!(nodeTree[cardId].getLocalPosition(), Vector2::create(100.0, 100.0));
		assert!(!getDraggable(&mut nodeTree, cardId).isReturning());

		// 끝 이벤트에서 눌러앉기.
		getDraggable(&mut nodeTree, cardId).setDragEndEvent(Box::new(|draggable: &mut UIDraggable, context: &mut ComponentContext, _viewPosition: Vector2| {
			draggable.settleHere(context.getNodeTreeMut());
		}));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::press, Vector2::create(100.0, 100.0));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::moved, Vector2::create(100.0, 300.0));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::release, Vector2::create(100.0, 300.0));
		assert!(!getDraggable(&mut nodeTree, cardId).isReturning());
		nodeTree.tick(&engine, 0.05);
		assert_eq!(nodeTree[cardId].getLocalPosition(), Vector2::create(100.0, 300.0));
		assert_eq!(getDraggable(&mut nodeTree, cardId).getHomePosition(), Some(Vector2::create(100.0, 300.0)));

		// 바로 되돌리기.
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::press, Vector2::create(100.0, 300.0));
		nodeTree.dispatchTouch(cardId, &engine, TouchKind::moved, Vector2::create(150.0, 300.0));
		nodeTree.withComponentMut::<UIDraggable, _>(cardId, |draggable, tree| {
			draggable.snapHome(tree);
		});
		assert_eq!(nodeTree[cardId].getLocalPosition(), Vector2::create(100.0, 300.0));
	}
}
