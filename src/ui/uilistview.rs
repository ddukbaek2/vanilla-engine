//==============================================================================
// 재활용 리스트 뷰. (원본 ui/uilistview.js 의 UIListView)
// - 항목 수만 받아, 화면에 보이는 범위(앞뒤 여유 칸 포함)만 실제 노드로 만들고 벗어난 노드는 회수해 다시 쓴다.
//   수백 ~ 수천 건이어도 노드 수는 화면 분량에 머문다. 항목 내용은 setBindItemEvent 로 코드에서 채운다. (데이터 주도)
// - 끝 근처에 닿으면 setReachEndEvent 알림을 한 번 보낸다. (무한 스크롤 / 더 불러오기) setItemCount 로 항목 수가 늘면 다시 무장된다.
// - 원본처럼 스크롤 뷰와 같은 노드에 붙는다. 노드에 스크롤 뷰가 없으면 붙을 때 UIScrollView 를 함께 붙인다. (원본 require)
//   UISnapScrollView 가 붙은 노드면 그 품은 뷰를 쓴다. (원본 getComponents(UIScrollView) 가 하위 클래스까지 잡는 자리)
//   스크롤 뷰가 먼저 붙어 먼저 tick 하므로, 이 위젯은 그 프레임의 스크롤 오프셋으로 보이는 범위를 고른다.
// - 사용:
//     let listView = nodeTree.addComponent(listId, UIListView::new()).expect("list view");
//     listView.setCreateItemEvent(Box::new(|tree: &mut NodeTree| { let itemId = tree.createNode("item"); ...; itemId }));
//     listView.setBindItemEvent(Box::new(|tree: &mut NodeTree, itemId: NodeId, itemIndex: usize| { ... }));
//     listView.setItemCount(500);
// - 원본과 다른 것:
//   - 원본 setTemplateNode 는 템플릿 노드를 UIDocument 로 직렬화해 두고 복제한다. 러스트 판은 컴포넌트를 복제할 수 없어(Clone 이 아니다)
//     항목 노드를 만드는 함수(setCreateItemEvent)를 받는다. 만든 노드는 content 노드의 자식으로 옮겨 붙인다.
//     항목 크기를 정하지 않았으면(0) 원본이 템플릿 크기를 읽듯 처음 만든 항목 노드의 진행 축 contentSize 를 읽는다.
//   - 바인드 이벤트는 (트리, 항목 노드, 항목 번호) 를 받는다. 끝 알림은 리스트 뷰와 문맥을 받는다.
//   - setItemCount, setItemSize, setSpacing, refresh 는 값을 정해 두고 다음 tick 에 스크롤 영역 갱신과 다시 바인딩을 한다.
//     (원본은 그 자리에서 한다. 같은 노드의 스크롤 뷰는 씬 코드가 이 위젯을 쥐고 있는 동안 닿을 수 없다)
//   - 활성 항목은 원본 Map 처럼 넣은 순서로 든다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uiscrollview::UIScrollView;
use crate::ui::uisnapscrollview::{findScrollView, findScrollViewMut, UISnapScrollView};

//==============================================================================
// 항목 노드 만들기 함수. (트리를 받아 새 항목 노드를 돌려준다. 원본 템플릿 복제 자리)
//==============================================================================
pub type UIListViewCreateItemEvent = Box<dyn FnMut(&mut NodeTree) -> NodeId>;

//==============================================================================
// 바인드 이벤트. (트리, 항목 노드, 항목 번호)
//==============================================================================
pub type UIListViewBindItemEvent = Box<dyn FnMut(&mut NodeTree, NodeId, usize)>;

//==============================================================================
// 끝 알림. (리스트 뷰와 문맥을 받는다)
//==============================================================================
pub type UIListViewReachEndEvent = Box<dyn FnMut(&mut UIListView, &mut ComponentContext)>;

//==============================================================================
// 재활용 리스트 뷰.
//==============================================================================
pub struct UIListView {
	componentBase: ComponentBase,
	createItemEvent: Option<UIListViewCreateItemEvent>,
	itemCount: usize,
	itemSize: f32,
	spacing: f32,
	isHorizontal: bool,
	bufferItemCount: usize,
	bindItemEvent: Option<UIListViewBindItemEvent>,
	reachEndEvent: Option<UIListViewReachEndEvent>,
	reachEndThreshold: f32,
	isReachEndArmed: bool,
	activeItemList: Vec<(usize, NodeId)>,
	freeItemList: Vec<NodeId>,
	isScrollContentSizePending: bool,
	isRefreshPending: bool,
}

impl UIListView {
	//==============================================================================
	// 생성. (세로, 여유 1 칸, 끝 알림 거리 120 픽셀)
	//==============================================================================
	pub fn new() -> UIListView {
		UIListView {
			componentBase: ComponentBase::new(),
			createItemEvent: None,
			itemCount: 0,
			itemSize: 0.0,
			spacing: 0.0,
			isHorizontal: false,
			bufferItemCount: 1,
			bindItemEvent: None,
			reachEndEvent: None,
			reachEndThreshold: 120.0,
			isReachEndArmed: true,
			activeItemList: Vec::new(),
			freeItemList: Vec::new(),
			isScrollContentSizePending: false,
			isRefreshPending: false,
		}
	}

	//==============================================================================
	// 항목 노드 만들기 함수 설정. (원본 setTemplateNode 자리)
	//==============================================================================
	pub fn setCreateItemEvent(&mut self, createItemEvent: UIListViewCreateItemEvent) {
		self.createItemEvent = Some(createItemEvent);
	}

	//==============================================================================
	// 항목 수 설정. (늘면 끝 알림을 다시 무장한다. 스크롤 영역 갱신과 다시 바인딩은 다음 tick)
	//==============================================================================
	pub fn setItemCount(&mut self, itemCount: usize) {
		let previousItemCount = self.itemCount;
		self.itemCount = itemCount;
		if self.itemCount > previousItemCount {
			self.isReachEndArmed = true;
		}
		self.isScrollContentSizePending = true;
		self.isRefreshPending = true;
	}

	//==============================================================================
	// 보이는 항목 모두 다시 바인딩. (데이터 내용이 바뀌었을 때. 다음 tick 에 한다)
	//==============================================================================
	pub fn refresh(&mut self) {
		self.isRefreshPending = true;
	}

	//==============================================================================
	// 보이는 항목 모두 지금 다시 바인딩. (트리를 쥔 씬 코드용. 원본 refresh 그대로)
	//==============================================================================
	pub fn refreshInTree(&mut self, nodeTree: &mut NodeTree) {
		self.isRefreshPending = false;
		let itemCount = self.itemCount;
		let bindItemEvent = match self.bindItemEvent.as_mut() {
			Some(bindItemEvent) => {
				bindItemEvent
			}
			None => {
				return;
			}
		};
		for (itemIndex, itemNodeId) in self.activeItemList.iter().copied() {
			if itemIndex >= itemCount || !nodeTree.contains(itemNodeId) {
				continue;
			}
			bindItemEvent(nodeTree, itemNodeId, itemIndex);
		}
	}

	//==============================================================================
	// 특정 항목의 실제 노드. (화면 밖이면 None)
	//==============================================================================
	pub fn getItemNode(&self, itemIndex: usize) -> Option<NodeId> {
		self.activeItemList.iter().find(|(activeIndex, _)| *activeIndex == itemIndex).map(|(_, itemNodeId)| *itemNodeId)
	}

	//==============================================================================
	// 바인드 이벤트 설정.
	//==============================================================================
	pub fn setBindItemEvent(&mut self, bindItemEvent: UIListViewBindItemEvent) {
		self.bindItemEvent = Some(bindItemEvent);
	}

	//==============================================================================
	// 끝 알림 설정.
	//==============================================================================
	pub fn setReachEndEvent(&mut self, reachEndEvent: UIListViewReachEndEvent) {
		self.reachEndEvent = Some(reachEndEvent);
	}

	//==============================================================================
	// 끝 알림 거리 설정. (픽셀)
	//==============================================================================
	pub fn setReachEndThreshold(&mut self, reachEndThreshold: f32) {
		self.reachEndThreshold = reachEndThreshold;
	}

	//==============================================================================
	// 진행 축 방향 항목 크기 설정. (스크롤 영역은 다음 tick 에 갱신)
	//==============================================================================
	pub fn setItemSize(&mut self, itemSize: f32) {
		self.itemSize = itemSize;
		self.isScrollContentSizePending = true;
	}

	//==============================================================================
	// 항목 사이 간격 설정. (스크롤 영역은 다음 tick 에 갱신)
	//==============================================================================
	pub fn setSpacing(&mut self, spacing: f32) {
		self.spacing = spacing;
		self.isScrollContentSizePending = true;
	}

	//==============================================================================
	// 가로 진행 설정. (기본 세로)
	//==============================================================================
	pub fn setHorizontal(&mut self, isHorizontal: bool) {
		self.isHorizontal = isHorizontal;
	}

	//==============================================================================
	// 가로 진행 여부.
	//==============================================================================
	pub fn isHorizontal(&self) -> bool {
		self.isHorizontal
	}

	//==============================================================================
	// 화면 밖 여유 항목 수 설정.
	//==============================================================================
	pub fn setBufferItemCount(&mut self, bufferItemCount: usize) {
		self.bufferItemCount = bufferItemCount;
	}

	//==============================================================================
	// 항목 수 반환.
	//==============================================================================
	pub fn getItemCount(&self) -> usize {
		self.itemCount
	}

	//==============================================================================
	// 항목 크기 반환.
	//==============================================================================
	pub fn getItemSize(&self) -> f32 {
		self.itemSize
	}

	//==============================================================================
	// 항목 사이 간격 반환.
	//==============================================================================
	pub fn getSpacing(&self) -> f32 {
		self.spacing
	}

	//==============================================================================
	// 지금 만들어 둔(보이는) 항목 수.
	//==============================================================================
	pub fn getActiveItemCount(&self) -> usize {
		self.activeItemList.len()
	}

	//==============================================================================
	// 진행 축의 값 고르기.
	//==============================================================================
	fn selectAxis(&self, vector: Vector2) -> f32 {
		if self.isHorizontal {
			vector.x
		}
		else {
			vector.y
		}
	}

	//==============================================================================
	// 스크롤 영역 크기 계산. (진행 축 = 항목 수 x 간격 포함 크기 - 마지막 간격, 다른 축 = 호스트 크기)
	//==============================================================================
	fn calculateScrollContentSize(&self, hostContentSize: Vector2) -> Vector2 {
		let pitch = self.itemSize + self.spacing;
		let totalLength = if self.itemCount > 0 {
			self.itemCount as f32 * pitch - self.spacing
		}
		else {
			0.0
		};
		if self.isHorizontal {
			Vector2::create(totalLength, hostContentSize.y)
		}
		else {
			Vector2::create(hostContentSize.x, totalLength)
		}
	}

	//==============================================================================
	// 항목 노드 확보. (회수분을 다시 쓰고, 없으면 만들어서 content 노드에 붙인다)
	//==============================================================================
	fn acquireItemNode(&mut self, nodeTree: &mut NodeTree, hostId: NodeId, contentId: Option<NodeId>) -> Option<NodeId> {
		// 밖에서 지운 노드는 버린다.
		while let Some(freeItemNodeId) = self.freeItemList.pop() {
			if nodeTree.contains(freeItemNodeId) {
				return Some(freeItemNodeId);
			}
		}
		let createItemEvent = self.createItemEvent.as_mut()?;
		let itemNodeId = createItemEvent(nodeTree);
		if !nodeTree.contains(itemNodeId) {
			return None;
		}
		let parentId = contentId.unwrap_or(hostId);
		nodeTree.addChild(parentId, itemNodeId);
		Some(itemNodeId)
	}

	//==============================================================================
	// 항목 크기를 처음 만든 항목 노드에서 읽는다. (원본이 템플릿 contentSize 를 읽는 자리. 노드는 회수 목록에 넣어 다시 쓴다)
	//==============================================================================
	fn measureItemSize(&mut self, nodeTree: &mut NodeTree, hostId: NodeId, contentId: Option<NodeId>) {
		let itemNodeId = match self.acquireItemNode(nodeTree, hostId, contentId) {
			Some(itemNodeId) => {
				itemNodeId
			}
			None => {
				return;
			}
		};
		let itemNode = &mut nodeTree[itemNodeId];
		let itemContentSize = itemNode.getContentSize();
		itemNode.setActive(false);
		self.itemSize = self.selectAxis(itemContentSize);
		self.freeItemList.push(itemNodeId);
		self.isScrollContentSizePending = true;
	}

	//==============================================================================
	// 끝 알림 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fireReachEndEvent(&mut self, context: &mut ComponentContext) {
		let mut reachEndEvent = match self.reachEndEvent.take() {
			Some(reachEndEvent) => {
				reachEndEvent
			}
			None => {
				return;
			}
		};
		reachEndEvent(self, context);
		if self.reachEndEvent.is_none() {
			self.reachEndEvent = Some(reachEndEvent);
		}
	}
}

impl Default for UIListView {
	fn default() -> UIListView {
		UIListView::new()
	}
}

impl Component for UIListView {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIListView"
	}

	//==============================================================================
	// 트리에 붙음. (노드에 스크롤 뷰가 없으면 UIScrollView 를 먼저 붙인다. 원본 require)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		let hasScrollView = nodeTree[nodeId].hasComponent::<UIScrollView>() || nodeTree[nodeId].hasComponent::<UISnapScrollView>();
		if !hasScrollView {
			nodeTree.addComponent(nodeId, UIScrollView::new());
		}
	}

	//==============================================================================
	// 갱신. (미뤄 둔 스크롤 영역 / 다시 바인딩 → 보이는 범위 계산 → 회수 / 생성 → 끝 알림)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
		if self.createItemEvent.is_none() {
			return;
		}
		let hostId = context.getNodeId();
		let hostContentSize = context.getTransform().getContentSize();
		let contentId = match findScrollView(context.getNode()) {
			Some(scrollView) => {
				scrollView.getContent()
			}
			None => {
				return;
			}
		};

		// 항목 크기를 정하지 않았으면 처음 만든 항목에서 읽는다.
		if self.itemSize <= 0.0 {
			self.measureItemSize(context.getNodeTreeMut(), hostId, contentId);
		}

		// 미뤄 둔 스크롤 영역 갱신, 그리고 지금 오프셋.
		let scrollContentSize = self.calculateScrollContentSize(hostContentSize);
		let isScrollContentSizePending = self.isScrollContentSizePending;
		self.isScrollContentSizePending = false;
		let scrollOffset = match findScrollViewMut(context.getNodeMut()) {
			Some(scrollView) => {
				if isScrollContentSizePending {
					scrollView.setScrollContentSize(scrollContentSize);
				}
				scrollView.getScrollOffset()
			}
			None => {
				return;
			}
		};

		// 미뤄 둔 다시 바인딩.
		if self.isRefreshPending {
			self.refreshInTree(context.getNodeTreeMut());
		}

		let viewLength = self.selectAxis(hostContentSize);
		let scrolledLength = -self.selectAxis(scrollOffset);
		let pitch = self.itemSize + self.spacing;
		if pitch <= 0.0 {
			return;
		}

		// 보이는 범위. (여유 칸 포함)
		let bufferItemCount = self.bufferItemCount as i64;
		let firstIndex = ((scrolledLength / pitch).floor() as i64 - bufferItemCount).max(0);
		let lastIndex = (((scrolledLength + viewLength) / pitch).ceil() as i64 + bufferItemCount - 1).min(self.itemCount as i64 - 1);

		// 범위를 벗어난 항목 회수.
		let nodeTree = context.getNodeTreeMut();
		let freeItemList = &mut self.freeItemList;
		self.activeItemList.retain(|(itemIndex, itemNodeId)| {
			let itemIndex = *itemIndex as i64;
			if itemIndex >= firstIndex && itemIndex <= lastIndex {
				return true;
			}
			if let Some(itemNode) = nodeTree.getNodeMut(*itemNodeId) {
				itemNode.setActive(false);
				freeItemList.push(*itemNodeId);
			}
			false
		});

		// 빠진 항목 생성 / 재사용.
		let mut itemIndex = firstIndex;
		while itemIndex <= lastIndex {
			let currentIndex = itemIndex as usize;
			itemIndex += 1;
			if self.getItemNode(currentIndex).is_some() {
				continue;
			}
			let itemNodeId = match self.acquireItemNode(nodeTree, hostId, contentId) {
				Some(itemNodeId) => {
					itemNodeId
				}
				None => {
					break;
				}
			};
			let itemPosition = if self.isHorizontal {
				Vector2::create(currentIndex as f32 * pitch, 0.0)
			}
			else {
				Vector2::create(0.0, currentIndex as f32 * pitch)
			};
			let itemName = format!("Item{}", currentIndex);
			let itemNode = &mut nodeTree[itemNodeId];
			itemNode.setName(&itemName);
			itemNode.setActive(true);
			itemNode.setLocalPosition(itemPosition);
			self.activeItemList.push((currentIndex, itemNodeId));
			if let Some(bindItemEvent) = self.bindItemEvent.as_mut() {
				bindItemEvent(nodeTree, itemNodeId, currentIndex);
			}
		}

		// 끝 근처 알림. (무한 스크롤)
		if self.reachEndEvent.is_some() && self.isReachEndArmed && self.itemCount > 0 {
			let totalLength = self.itemCount as f32 * pitch - self.spacing;
			let remainLength = totalLength - (scrolledLength + viewLength);
			if remainLength <= self.reachEndThreshold {
				self.isReachEndArmed = false;
				self.fireReachEndEvent(context);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use std::cell::{Cell, RefCell};
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (왼쪽 위 (0, 0), 200 x 100 인 리스트, 항목은 200 x 30. 만든 수를 센다)
	//==============================================================================
	fn createListTree(createdCount: &Rc<Cell<u32>>, boundIndexList: &Rc<RefCell<Vec<usize>>>) -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let listId = nodeTree.createChild(rootId, "list");
		let listNode = &mut nodeTree[listId];
		listNode.setPivot(Pivot::topLeft);
		listNode.setContentSize(Vector2::create(200.0, 100.0));
		let createdCounter = createdCount.clone();
		let boundIndexRecorder = boundIndexList.clone();
		let listView = nodeTree.addComponent(listId, UIListView::new()).expect("list view");
		listView.setCreateItemEvent(Box::new(move |tree: &mut NodeTree| {
			createdCounter.set(createdCounter.get() + 1);
			let itemId = tree.createNode("item");
			let itemNode = &mut tree[itemId];
			itemNode.setPivot(Pivot::topLeft);
			itemNode.setContentSize(Vector2::create(200.0, 30.0));
			itemId
		}));
		listView.setBindItemEvent(Box::new(move |_tree: &mut NodeTree, _itemId: NodeId, itemIndex: usize| {
			boundIndexRecorder.borrow_mut().push(itemIndex);
		}));
		listView.setItemCount(100);
		(nodeTree, listId)
	}

	//==============================================================================
	// 리스트 뷰 반환.
	//==============================================================================
	fn getListView(nodeTree: &mut NodeTree, listId: NodeId) -> &mut UIListView {
		nodeTree[listId].getComponentMut::<UIListView>().expect("list view")
	}

	//==============================================================================
	// 붙으면 UIScrollView 가 함께 붙고, 첫 tick 에 항목 크기와 스크롤 영역이 정해져 보이는 범위만 만든다.
	//==============================================================================
	#[test]
	fn createsOnlyVisibleRange() {
		let engine = Engine::new(EngineConfiguration::new());
		let createdCount = Rc::new(Cell::new(0));
		let boundIndexList = Rc::new(RefCell::new(Vec::new()));
		let (mut nodeTree, listId) = createListTree(&createdCount, &boundIndexList);
		assert!(nodeTree[listId].hasComponent::<UIScrollView>());
		nodeTree.tick(&engine, 0.016);
		assert_eq!(getListView(&mut nodeTree, listId).getItemSize(), 30.0);
		assert_eq!(nodeTree[listId].getComponent::<UIScrollView>().expect("scroll view").getScrollContentSize(), Vector2::create(200.0, 3000.0));
		// 0 ~ 100 픽셀 + 여유 1 칸: 0 ~ 4 번.
		assert_eq!(getListView(&mut nodeTree, listId).getActiveItemCount(), 5);
		assert_eq!(createdCount.get(), 5);
		assert_eq!(*boundIndexList.borrow(), vec![0, 1, 2, 3, 4]);
		let fourthId = getListView(&mut nodeTree, listId).getItemNode(4).expect("item 4");
		assert_eq!(nodeTree[fourthId].getLocalPosition(), Vector2::create(0.0, 120.0));
		assert_eq!(nodeTree[fourthId].getName(), "Item4");
		let contentId = nodeTree[listId].getComponent::<UIScrollView>().expect("scroll view").getContent().expect("content");
		assert_eq!(nodeTree.getParentId(fourthId), Some(contentId));
	}

	//==============================================================================
	// 스크롤하면 벗어난 항목을 회수해 다시 쓴다. (노드 수는 화면 분량에 머문다)
	//==============================================================================
	#[test]
	fn scrollingRecyclesItems() {
		let engine = Engine::new(EngineConfiguration::new());
		let createdCount = Rc::new(Cell::new(0));
		let boundIndexList = Rc::new(RefCell::new(Vec::new()));
		let (mut nodeTree, listId) = createListTree(&createdCount, &boundIndexList);
		nodeTree.tick(&engine, 0.016);
		nodeTree[listId].getComponentMut::<UIScrollView>().expect("scroll view").setScrollOffset(Vector2::create(0.0, -600.0));
		nodeTree.tick(&engine, 0.016);
		// 600 ~ 700 픽셀 + 여유: 19 ~ 24 번.
		assert_eq!(getListView(&mut nodeTree, listId).getActiveItemCount(), 6);
		assert!(getListView(&mut nodeTree, listId).getItemNode(0).is_none());
		let nineteenthId = getListView(&mut nodeTree, listId).getItemNode(19).expect("item 19");
		assert_eq!(nodeTree[nineteenthId].getLocalPosition(), Vector2::create(0.0, 570.0));
		assert!(nodeTree[nineteenthId].isActive());
		assert_eq!(createdCount.get(), 6);

		// 끝까지 갔다가 처음으로 돌아와도 만든 노드 수는 늘지 않는다.
		for offsetStep in 0..30 {
			let offsetY = -(offsetStep as f32) * 100.0;
			nodeTree[listId].getComponentMut::<UIScrollView>().expect("scroll view").setScrollOffset(Vector2::create(0.0, offsetY));
			nodeTree.tick(&engine, 0.016);
		}
		nodeTree[listId].getComponentMut::<UIScrollView>().expect("scroll view").setScrollOffset(Vector2::zero());
		nodeTree.tick(&engine, 0.016);
		assert!(createdCount.get() <= 8);
		assert_eq!(getListView(&mut nodeTree, listId).getActiveItemCount(), 5);
	}

	//==============================================================================
	// 끝 근처에서 알림이 한 번 오고, 항목 수가 늘면 다시 무장된다. 항목 수를 줄이면 넘친 항목은 회수된다.
	//==============================================================================
	#[test]
	fn reachEndFiresOnceAndRearms() {
		let engine = Engine::new(EngineConfiguration::new());
		let createdCount = Rc::new(Cell::new(0));
		let boundIndexList = Rc::new(RefCell::new(Vec::new()));
		let (mut nodeTree, listId) = createListTree(&createdCount, &boundIndexList);
		let reachEndCount = Rc::new(Cell::new(0));
		let reachEndCounter = reachEndCount.clone();
		getListView(&mut nodeTree, listId).setReachEndEvent(Box::new(move |listView: &mut UIListView, _context: &mut ComponentContext| {
			reachEndCounter.set(reachEndCounter.get() + 1);
			// 알림 안에서 항목을 더 불러온다.
			let itemCount = listView.getItemCount();
			listView.setItemCount(itemCount + 50);
		}));
		nodeTree.tick(&engine, 0.016);
		assert_eq!(reachEndCount.get(), 0);
		nodeTree[listId].getComponentMut::<UIScrollView>().expect("scroll view").setScrollOffset(Vector2::create(0.0, -2900.0));
		nodeTree.tick(&engine, 0.016);
		assert_eq!(reachEndCount.get(), 1);
		assert_eq!(getListView(&mut nodeTree, listId).getItemCount(), 150);
		nodeTree.tick(&engine, 0.016);
		assert_eq!(reachEndCount.get(), 1);
		assert_eq!(nodeTree[listId].getComponent::<UIScrollView>().expect("scroll view").getScrollContentSize(), Vector2::create(200.0, 4500.0));

		// 항목 수를 줄이면 그 범위 밖 항목은 회수되고, 남은 항목은 다시 바인딩된다.
		boundIndexList.borrow_mut().clear();
		nodeTree[listId].getComponentMut::<UIScrollView>().expect("scroll view").setScrollOffset(Vector2::zero());
		nodeTree.tick(&engine, 0.016);
		getListView(&mut nodeTree, listId).setItemCount(3);
		boundIndexList.borrow_mut().clear();
		nodeTree.tick(&engine, 0.016);
		assert_eq!(getListView(&mut nodeTree, listId).getActiveItemCount(), 3);
		assert_eq!(*boundIndexList.borrow(), vec![0, 1, 2]);
	}

	//==============================================================================
	// 가로 리스트는 x 축으로 늘어선다. 간격을 주면 간격만큼 벌어진다.
	//==============================================================================
	#[test]
	fn horizontalListWithSpacing() {
		let engine = Engine::new(EngineConfiguration::new());
		let createdCount = Rc::new(Cell::new(0));
		let boundIndexList = Rc::new(RefCell::new(Vec::new()));
		let (mut nodeTree, listId) = createListTree(&createdCount, &boundIndexList);
		{
			let listView = getListView(&mut nodeTree, listId);
			listView.setHorizontal(true);
			listView.setItemSize(50.0);
			listView.setSpacing(10.0);
			listView.setItemCount(10);
		}
		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[listId].getComponent::<UIScrollView>().expect("scroll view").getScrollContentSize(), Vector2::create(590.0, 100.0));
		// 0 ~ 200 픽셀, 간격 포함 60: ceil(200 / 60) = 4 → 0 ~ 4 번.
		assert_eq!(getListView(&mut nodeTree, listId).getActiveItemCount(), 5);
		let secondId = getListView(&mut nodeTree, listId).getItemNode(2).expect("item 2");
		assert_eq!(nodeTree[secondId].getLocalPosition(), Vector2::create(120.0, 0.0));
	}
}
