//==============================================================================
// 스냅 스크롤 뷰. (원본 ui/uisnapscrollview.js 의 UISnapScrollView)
// - 드래그를 놓으면 content 자식(아이템) 하나의 가운데가 보이는 영역 가운데에 오도록 미끄러진다. 아이템 크기가 달라도 된다.
// - 가로 스냅: setHorizontal(true), setVertical(false) (기본). 세로 스냅: setHorizontal(false), setVertical(true).
// - 한 번 끌면 최대 한 칸만 움직인다. 끈 거리가 다음 아이템 크기의 setPageChangeThreshold 비율(기본 0.35) 이상이면 넘어간다.
//   끄는 동안에는 오프셋이 지금 칸 ±1 칸 범위를 넘지 않는다.
// - pageTo(nodeTree, index, smooth): 지정 칸으로 이동. smooth 면 0.3 초 cubic.out 으로 미끄러지고, 아니면 바로 옮긴다.
// - 원본은 UIScrollView 를 상속한다. 러스트 판은 UIScrollView 를 품고(scrollView 필드) 터치, 휠, 그리기, 붙기를 넘겨준다.
//   스크롤 뷰 설정은 getScrollViewMut 로 품은 뷰에 한다. 자주 쓰는 것(콘텐트 크기, 오프셋, 축)은 이 위젯에도 있다.
//   nodeTree.addComponent(nodeId, UISnapScrollView::new()) 로 붙인다. (content 노드, Mask, interactable 은 품은 뷰가 만든다)
// - 원본과 다른 것:
//   - 칸을 바꾸는 함수(pageTo, setSnapCurrentIndex, playSnapTween 등)는 content 자식을 읽어야 해서 트리를 받는다.
//     씬 코드에서는 nodeTree.withComponentMut::<UISnapScrollView, _>(snapId, |snapView, tree| snapView.pageTo(tree, 2, true)) 로 부른다.
//   - 스냅 미끄러짐은 원본 Tween 객체 대신 같은 계산(0.3 초, cubic.out, 만든 tick 에 첫 걸음)을 이 위젯 안에서 한다.
//     (원본 트윈의 갱신 콜백이 스크롤 뷰 자신을 부르는데, 러스트 트윈 콜백은 트리만 받아 tick 중에 꺼내진 이 위젯에 닿지 않는다)
//   - 미끄러지는 동안 content 노드는 그 tick 안에서 바로 옮긴다. (품은 뷰의 반영은 다음 tick 이라 한 프레임 늦지 않게)
//   - setSnapToNearest, setVelocityThreshold 는 원본처럼 값만 든다. (원본 playSnapTween 도 이 값을 읽지 않는다)
//   - 원본은 스냅 뷰가 UIScrollView 이기도 해서 TouchRecognizer, UIScrollBar, UIListView 가 그대로 알아본다.
//     러스트 판은 TouchRecognizer 와 UIListView 가 findScrollView 로 알아본다. UIScrollBar 는 UIScrollView 컴포넌트만 찾아서
//     스냅 뷰에는 스크롤바를 붙일 수 없다. (setShows...ScrollBar 는 품은 뷰에 값만 들어간다)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::{NodeTransform, WorldNode};
use crate::core::tween::easingFunction;
use crate::ui::uiscrollview::{ScrollMode, UIScrollView};

// 스냅 미끄러짐 시간. (초, 원본 트윈 0.3)
const SNAP_TWEEN_DURATION: f32 = 0.3;

//==============================================================================
// 스냅 미끄러짐 하나. (원본 #snapTween 자리)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct SnapTween {
	startOffset: Vector2,
	targetOffset: Vector2,
	elapsedSeconds: f32,
}

//==============================================================================
// 스냅 스크롤 뷰.
//==============================================================================
pub struct UISnapScrollView {
	scrollView: UIScrollView,
	nodeId: Option<NodeId>,
	snapTween: Option<SnapTween>,
	snapCurrentIndex: usize,
	previousIsDragging: bool,
	isSnapToNearest: bool,
	velocityThreshold: f32,
	pageChangeThreshold: f32,
}

impl UISnapScrollView {
	//==============================================================================
	// 생성. (clamp 모드, 가로만 스크롤)
	//==============================================================================
	pub fn new() -> UISnapScrollView {
		let mut scrollView = UIScrollView::new();
		scrollView.setScrollMode(ScrollMode::clamp);
		scrollView.setHorizontal(true);
		scrollView.setVertical(false);
		UISnapScrollView {
			scrollView,
			nodeId: None,
			snapTween: None,
			snapCurrentIndex: 0,
			previousIsDragging: false,
			isSnapToNearest: false,
			velocityThreshold: 200.0,
			pageChangeThreshold: 0.35,
		}
	}

	//==============================================================================
	// 품은 스크롤 뷰 반환.
	//==============================================================================
	pub fn getScrollView(&self) -> &UIScrollView {
		&self.scrollView
	}

	//==============================================================================
	// 품은 스크롤 뷰 반환. (변경용)
	//==============================================================================
	pub fn getScrollViewMut(&mut self) -> &mut UIScrollView {
		&mut self.scrollView
	}

	//==============================================================================
	// 컨텐트 노드 핸들. (아이템 노드를 여기에 붙인다)
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.scrollView.getContent()
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	pub fn setBackgroundColor(&mut self, backgroundColor: impl Into<Color>) {
		self.scrollView.setBackgroundColor(backgroundColor);
	}

	//==============================================================================
	// 스크롤 콘텐츠 크기 설정.
	//==============================================================================
	pub fn setScrollContentSize(&mut self, scrollContentSize: Vector2) {
		self.scrollView.setScrollContentSize(scrollContentSize);
	}

	//==============================================================================
	// 스크롤 콘텐츠 크기 반환.
	//==============================================================================
	pub fn getScrollContentSize(&self) -> Vector2 {
		self.scrollView.getScrollContentSize()
	}

	//==============================================================================
	// 오프셋 설정. (범위로 자른다)
	//==============================================================================
	pub fn setScrollOffset(&mut self, offset: Vector2) {
		self.scrollView.setScrollOffset(offset);
	}

	//==============================================================================
	// 오프셋 반환.
	//==============================================================================
	pub fn getScrollOffset(&self) -> Vector2 {
		self.scrollView.getScrollOffset()
	}

	//==============================================================================
	// 가로 스크롤 켜기 설정.
	//==============================================================================
	pub fn setHorizontal(&mut self, isHorizontal: bool) {
		self.scrollView.setHorizontal(isHorizontal);
	}

	//==============================================================================
	// 가로 스크롤 켜짐 여부.
	//==============================================================================
	pub fn isHorizontal(&self) -> bool {
		self.scrollView.isHorizontal()
	}

	//==============================================================================
	// 세로 스크롤 켜기 설정.
	//==============================================================================
	pub fn setVertical(&mut self, isVertical: bool) {
		self.scrollView.setVertical(isVertical);
	}

	//==============================================================================
	// 세로 스크롤 켜짐 여부.
	//==============================================================================
	pub fn isVertical(&self) -> bool {
		self.scrollView.isVertical()
	}

	//==============================================================================
	// 드래그 중인지.
	//==============================================================================
	pub fn isDragging(&self) -> bool {
		self.scrollView.isDragging()
	}

	//==============================================================================
	// 스냅 모드 설정. (원본처럼 값만 든다)
	//==============================================================================
	pub fn setSnapToNearest(&mut self, isSnapToNearest: bool) {
		self.isSnapToNearest = isSnapToNearest;
	}

	//==============================================================================
	// 스냅 모드 반환.
	//==============================================================================
	pub fn getSnapToNearest(&self) -> bool {
		self.isSnapToNearest
	}

	//==============================================================================
	// 방향성 스냅 최소 속도 설정. (px/s, 원본처럼 값만 든다)
	//==============================================================================
	pub fn setVelocityThreshold(&mut self, velocityThreshold: f32) {
		self.velocityThreshold = velocityThreshold;
	}

	//==============================================================================
	// 방향성 스냅 최소 속도 반환.
	//==============================================================================
	pub fn getVelocityThreshold(&self) -> f32 {
		self.velocityThreshold
	}

	//==============================================================================
	// 칸을 넘길 끈 거리 비율 설정. (다음 아이템 크기 대비, 기본 0.35)
	//==============================================================================
	pub fn setPageChangeThreshold(&mut self, pageChangeThreshold: f32) {
		self.pageChangeThreshold = pageChangeThreshold;
	}

	//==============================================================================
	// 칸을 넘길 끈 거리 비율 반환.
	//==============================================================================
	pub fn getPageChangeThreshold(&self) -> f32 {
		self.pageChangeThreshold
	}

	//==============================================================================
	// 지금 스냅 칸 반환.
	//==============================================================================
	pub fn getSnapCurrentIndex(&self) -> usize {
		self.snapCurrentIndex
	}

	//==============================================================================
	// 미끄러지는 중인지.
	//==============================================================================
	pub fn isSnapping(&self) -> bool {
		self.snapTween.is_some()
	}

	//==============================================================================
	// content 자식 수. (트리에 붙기 전이면 0)
	//==============================================================================
	fn getItemCount(&self, nodeTree: &NodeTree) -> usize {
		match self.getContent() {
			Some(contentId) => {
				nodeTree.getChildCount(contentId)
			}
			None => {
				0
			}
		}
	}

	//==============================================================================
	// 진행 축의 값 고르기.
	//==============================================================================
	fn selectAxis(&self, vector: Vector2) -> f32 {
		if self.isHorizontal() {
			vector.x
		}
		else {
			vector.y
		}
	}

	//==============================================================================
	// 진행 축만 바꾼 오프셋. (다른 축은 지금 오프셋 그대로)
	//==============================================================================
	fn buildAxisOffset(&self, axisOffset: f32) -> Vector2 {
		let currentOffset = self.getScrollOffset();
		if self.isHorizontal() {
			Vector2::create(axisOffset, currentOffset.y)
		}
		else {
			Vector2::create(currentOffset.x, axisOffset)
		}
	}

	//==============================================================================
	// content 노드를 지금 오프셋 자리로 바로 옮긴다.
	//==============================================================================
	fn syncContentPosition(&self, nodeTree: &mut NodeTree) {
		let scrollOffset = self.getScrollOffset();
		if let Some(content) = self.getContent().and_then(|contentId| nodeTree.getNodeMut(contentId)) {
			content.setLocalPosition(scrollOffset);
		}
	}

	//==============================================================================
	// content 자식마다 스냅 오프셋 계산. (각 아이템 가운데가 보이는 영역 가운데에 오는 오프셋, 크기가 달라도 맞다)
	//==============================================================================
	pub fn computeSnapOffsets(&self, nodeTree: &NodeTree) -> Vec<f32> {
		let mut snapOffsets: Vec<f32> = Vec::new();
		let viewportSize = match self.nodeId.and_then(|nodeId| nodeTree.getNode(nodeId)) {
			Some(node) => {
				node.getContentSize()
			}
			None => {
				return snapOffsets;
			}
		};
		let contentId = match self.getContent() {
			Some(contentId) => {
				contentId
			}
			None => {
				return snapOffsets;
			}
		};
		let childIds = nodeTree.getChildIds(contentId);
		snapOffsets.reserve(childIds.len());
		for childId in childIds.iter().copied() {
			let child = &nodeTree[childId];
			let childLocalPosition = child.getLocalPosition();
			let childContentSize = child.getContentSize();
			let snapOffset = self.selectAxis(viewportSize) * 0.5 - self.selectAxis(childLocalPosition) - self.selectAxis(childContentSize) * 0.5;
			snapOffsets.push(snapOffset);
		}
		snapOffsets
	}

	//==============================================================================
	// 끄는 동안 오프셋을 지금 칸의 앞뒤 칸 범위로 자른다.
	//==============================================================================
	pub fn clampScrollOffsetToAdjacentPages(&mut self, nodeTree: &mut NodeTree) {
		let snapOffsets = self.computeSnapOffsets(nodeTree);
		if snapOffsets.is_empty() {
			return;
		}
		let maximumIndex = snapOffsets.len() - 1;
		let snapCurrentIndex = self.snapCurrentIndex.min(maximumIndex);
		let previousIndex = snapCurrentIndex.saturating_sub(1);
		let nextIndex = (snapCurrentIndex + 1).min(maximumIndex);
		let maximumOffset = snapOffsets[previousIndex];
		let minimumOffset = snapOffsets[nextIndex];
		let currentAxisOffset = self.selectAxis(self.getScrollOffset());
		let clampedAxisOffset = math::clamp(currentAxisOffset, minimumOffset, maximumOffset);
		if clampedAxisOffset != currentAxisOffset {
			let clampedOffset = self.buildAxisOffset(clampedAxisOffset);
			self.setScrollOffset(clampedOffset);
		}
	}

	//==============================================================================
	// 지금 오프셋으로 목표 칸을 정하고 미끄러짐을 시작한다. (끈 방향으로 최대 한 칸, 다음 칸이 비율 이상 드러나면 넘김)
	//==============================================================================
	pub fn playSnapTween(&mut self, nodeTree: &mut NodeTree) {
		let snapOffsets = self.computeSnapOffsets(nodeTree);
		if snapOffsets.is_empty() {
			return;
		}
		let contentId = match self.getContent() {
			Some(contentId) => {
				contentId
			}
			None => {
				return;
			}
		};
		let maximumIndex = snapOffsets.len() - 1;
		let currentAxisOffset = self.selectAxis(self.getScrollOffset());
		let snapCurrentIndex = self.snapCurrentIndex.min(maximumIndex);
		let currentSnapOffset = snapOffsets[snapCurrentIndex];
		let dragAmount = currentAxisOffset - currentSnapOffset;
		let direction: i64 = if dragAmount < 0.0 {
			1
		}
		else {
			-1
		};
		let candidateIndex = (snapCurrentIndex as i64 + direction).clamp(0, maximumIndex as i64) as usize;
		let targetIndex = if candidateIndex != snapCurrentIndex {
			let candidateChildId = nodeTree.getChildIds(contentId)[candidateIndex];
			let candidateChildContentSize = nodeTree[candidateChildId].getContentSize();
			let targetItemSize = self.selectAxis(candidateChildContentSize);
			if dragAmount.abs() >= targetItemSize * self.getPageChangeThreshold() {
				candidateIndex
			}
			else {
				snapCurrentIndex
			}
		}
		else {
			snapCurrentIndex
		};
		self.setSnapCurrentIndex(nodeTree, targetIndex);
	}

	//==============================================================================
	// 지정 칸으로 이동. (smooth 면 미끄러지고, 아니면 바로 옮긴다)
	//==============================================================================
	pub fn pageTo(&mut self, nodeTree: &mut NodeTree, index: usize, smooth: bool) {
		let itemCount = self.getItemCount(nodeTree);
		if itemCount == 0 {
			return;
		}
		let clampedIndex = index.min(itemCount - 1);
		if smooth {
			self.setSnapCurrentIndex(nodeTree, clampedIndex);
			return;
		}
		self.snapCurrentIndex = clampedIndex;
		self.snapTween = None;
		let snapOffsets = self.computeSnapOffsets(nodeTree);
		let targetSnapOffset = snapOffsets[self.snapCurrentIndex];
		let targetOffset = self.buildAxisOffset(targetSnapOffset);
		self.setScrollOffset(targetOffset);
		self.syncContentPosition(nodeTree);
	}

	//==============================================================================
	// 지정 칸으로 미끄러짐 시작. (지금 오프셋에서 그 칸의 스냅 오프셋까지)
	//==============================================================================
	pub fn setSnapCurrentIndex(&mut self, nodeTree: &mut NodeTree, index: usize) {
		let itemCount = self.getItemCount(nodeTree);
		if itemCount == 0 {
			return;
		}
		self.snapCurrentIndex = index.min(itemCount - 1);
		let snapOffsets = self.computeSnapOffsets(nodeTree);
		let targetSnapOffset = snapOffsets[self.snapCurrentIndex];
		let startOffset = self.getScrollOffset();
		let targetOffset = self.buildAxisOffset(targetSnapOffset);
		self.snapTween = Some(SnapTween {
			startOffset,
			targetOffset,
			elapsedSeconds: 0.0,
		});
	}

	//==============================================================================
	// 미끄러짐 한 걸음. (원본 Tween.tick 과 같은 계산: 진행률 = 지난 시간 / 0.3, cubic.out, 다 차면 끝)
	//==============================================================================
	fn tickSnapTween(&mut self, timeDelta: f32) {
		let mut snapTween = match self.snapTween {
			Some(snapTween) => {
				snapTween
			}
			None => {
				return;
			}
		};
		snapTween.elapsedSeconds += timeDelta;
		let progress = math::clamp(snapTween.elapsedSeconds / SNAP_TWEEN_DURATION, 0.0, 1.0);
		let easedProgress = easingFunction::cubic::out(progress);
		let offsetX = math::lerp(snapTween.startOffset.x, snapTween.targetOffset.x, easedProgress);
		let offsetY = math::lerp(snapTween.startOffset.y, snapTween.targetOffset.y, easedProgress);
		self.setScrollOffset(Vector2::create(offsetX, offsetY));
		if progress >= 1.0 {
			self.snapTween = None;
		}
		else {
			self.snapTween = Some(snapTween);
		}
	}
}

impl Default for UISnapScrollView {
	fn default() -> UISnapScrollView {
		UISnapScrollView::new()
	}
}

impl Component for UISnapScrollView {
	fn getComponentBase(&self) -> &ComponentBase {
		self.scrollView.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.scrollView.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"SnapScrollView"
	}

	//==============================================================================
	// 노드에 붙음. (품은 뷰가 보이는 영역 크기를 읽는다)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		self.scrollView.attach(transform);
	}

	//==============================================================================
	// 트리에 붙음. (품은 뷰가 content, Mask, interactable 을 만든다. 호스트 핸들을 기억한다)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.scrollView.attachToTree(nodeTree, nodeId);
		self.nodeId = Some(nodeId);
	}

	//==============================================================================
	// 갱신. (품은 뷰 → 드래그 시작이면 미끄러짐 취소 → 끄는 중 칸 범위 자르기 → 놓았으면 스냅 시작 → 미끄러짐)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		self.nodeId = Some(context.getNodeId());
		let previousIsDragging = self.previousIsDragging;
		self.scrollView.tick(context, timeDelta);
		let currentIsDragging = self.isDragging();
		self.previousIsDragging = currentIsDragging;

		// 드래그 시작이면 미끄러짐 취소.
		if !previousIsDragging && currentIsDragging {
			self.snapTween = None;
		}

		// 끄는 중에는 지금 칸 앞뒤 범위로 자른다.
		let mut isOffsetChanged = false;
		if currentIsDragging {
			self.clampScrollOffsetToAdjacentPages(context.getNodeTreeMut());
			isOffsetChanged = true;
		}

		// 놓았으면 목표 칸을 정하고 미끄러짐 시작.
		if previousIsDragging && !currentIsDragging {
			self.playSnapTween(context.getNodeTreeMut());
		}

		// 미끄러짐 한 걸음. (만든 tick 에도 바로 한 걸음 간다. 원본과 같음)
		if self.snapTween.is_some() {
			self.tickSnapTween(timeDelta);
			isOffsetChanged = true;
		}
		if isOffsetChanged {
			self.syncContentPosition(context.getNodeTreeMut());
		}
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.scrollView.draw(transform, graphic);
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn touchPress(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.scrollView.touchPress(context, viewInputPosition);
	}

	fn touchMove(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.scrollView.touchMove(context, viewInputPosition);
	}

	fn touchRelease(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.scrollView.touchRelease(context, viewInputPosition);
	}

	fn touchCancel(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.scrollView.touchCancel(context, viewInputPosition);
	}

	//==============================================================================
	// 휠. (원본처럼 품은 뷰 그대로: 자유롭게 움직이고 스냅하지 않는다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheelDelta: Vector2) -> bool {
		self.scrollView.wheel(context, wheelDelta)
	}
}

//==============================================================================
// 노드의 스크롤 뷰 찾기. (UIScrollView, 없으면 UISnapScrollView 가 품은 뷰. 원본 getComponents(UIScrollView) 가 하위 클래스까지 잡는 자리)
//==============================================================================
pub fn findScrollView(node: &WorldNode) -> Option<&UIScrollView> {
	if let Some(scrollView) = node.getComponent::<UIScrollView>() {
		return Some(scrollView);
	}
	node.getComponent::<UISnapScrollView>().map(|snapScrollView| snapScrollView.getScrollView())
}

//==============================================================================
// 노드의 스크롤 뷰 찾기. (변경용)
//==============================================================================
pub fn findScrollViewMut(node: &mut WorldNode) -> Option<&mut UIScrollView> {
	if node.hasComponent::<UIScrollView>() {
		return node.getComponentMut::<UIScrollView>();
	}
	node.getComponentMut::<UISnapScrollView>().map(|snapScrollView| snapScrollView.getScrollViewMut())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use crate::ui::touchrecognizer::TouchRecognizer;
	use crate::ui::uibutton::UIButton;

	//==============================================================================
	// 시험 장면. (왼쪽 위 (0, 0), 200 x 100 인 가로 스냅 뷰, 200 x 100 칸 다섯 개)
	//==============================================================================
	fn createSnapTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let snapId = nodeTree.createChild(rootId, "snap");
		let snapNode = &mut nodeTree[snapId];
		snapNode.setPivot(Pivot::topLeft);
		snapNode.setContentSize(Vector2::create(200.0, 100.0));
		let contentId = {
			let snapView = nodeTree.addComponent(snapId, UISnapScrollView::new()).expect("snap view");
			snapView.setScrollContentSize(Vector2::create(1000.0, 100.0));
			snapView.getContent().expect("content")
		};
		for pageIndex in 0..5 {
			let pageId = nodeTree.createChild(contentId, "page");
			let pageNode = &mut nodeTree[pageId];
			pageNode.setPivot(Pivot::topLeft);
			pageNode.setLocalPosition(Vector2::create(pageIndex as f32 * 200.0, 0.0));
			pageNode.setContentSize(Vector2::create(200.0, 100.0));
		}
		(nodeTree, snapId)
	}

	//==============================================================================
	// 스냅 뷰 반환.
	//==============================================================================
	fn getSnapView(nodeTree: &mut NodeTree, snapId: NodeId) -> &mut UISnapScrollView {
		nodeTree[snapId].getComponentMut::<UISnapScrollView>().expect("snap view")
	}

	//==============================================================================
	// 끌고 놓기. (누름 → 이동 → tick → 뗌 → 0.5 초 동안 tick)
	//==============================================================================
	fn dragAndRelease(nodeTree: &mut NodeTree, engine: &Engine, snapId: NodeId, fromX: f32, toX: f32) {
		nodeTree.dispatchTouch(snapId, engine, TouchKind::press, Vector2::create(fromX, 50.0));
		nodeTree.dispatchTouch(snapId, engine, TouchKind::moved, Vector2::create(toX, 50.0));
		nodeTree.tick(engine, 0.016);
		nodeTree.dispatchTouch(snapId, engine, TouchKind::release, Vector2::create(toX, 50.0));
		for _ in 0..30 {
			nodeTree.tick(engine, 0.016);
		}
	}

	//==============================================================================
	// 스냅 오프셋은 칸 가운데가 영역 가운데에 오는 값이다.
	//==============================================================================
	#[test]
	fn snapOffsetsCenterEachItem() {
		let (nodeTree, snapId) = createSnapTree();
		let snapOffsets = nodeTree[snapId].getComponent::<UISnapScrollView>().expect("snap view").computeSnapOffsets(&nodeTree);
		assert_eq!(snapOffsets, vec![0.0, -200.0, -400.0, -600.0, -800.0]);
	}

	//==============================================================================
	// 칸 크기의 35% 이상 끌면 한 칸 넘어가고, 덜 끌면 제자리로 돌아온다. 아무리 멀리 끌어도 한 칸만 간다.
	//==============================================================================
	#[test]
	fn dragSelectsAtMostOneNeighbor() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, snapId) = createSnapTree();
		let contentId = getSnapView(&mut nodeTree, snapId).getContent().expect("content");

		// 100 픽셀 왼쪽으로 → 다음 칸.
		dragAndRelease(&mut nodeTree, &engine, snapId, 150.0, 50.0);
		assert_eq!(getSnapView(&mut nodeTree, snapId).getSnapCurrentIndex(), 1);
		assert_eq!(getSnapView(&mut nodeTree, snapId).getScrollOffset(), Vector2::create(-200.0, 0.0));
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::create(-200.0, 0.0));
		assert!(!getSnapView(&mut nodeTree, snapId).isSnapping());

		// 40 픽셀만 → 제자리.
		dragAndRelease(&mut nodeTree, &engine, snapId, 150.0, 110.0);
		assert_eq!(getSnapView(&mut nodeTree, snapId).getSnapCurrentIndex(), 1);
		assert_eq!(getSnapView(&mut nodeTree, snapId).getScrollOffset(), Vector2::create(-200.0, 0.0));

		// 490 픽셀 → 끄는 동안 다음 칸(-400)에서 막히고, 놓으면 한 칸만 간다.
		nodeTree.dispatchTouch(snapId, &engine, TouchKind::press, Vector2::create(190.0, 50.0));
		nodeTree.dispatchTouch(snapId, &engine, TouchKind::moved, Vector2::create(-300.0, 50.0));
		nodeTree.tick(&engine, 0.016);
		assert_eq!(getSnapView(&mut nodeTree, snapId).getScrollOffset(), Vector2::create(-400.0, 0.0));
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::create(-400.0, 0.0));
		nodeTree.dispatchTouch(snapId, &engine, TouchKind::release, Vector2::create(-300.0, 50.0));
		for _ in 0..30 {
			nodeTree.tick(&engine, 0.016);
		}
		assert_eq!(getSnapView(&mut nodeTree, snapId).getSnapCurrentIndex(), 2);

		// 오른쪽으로 끌면 앞 칸.
		dragAndRelease(&mut nodeTree, &engine, snapId, 50.0, 150.0);
		assert_eq!(getSnapView(&mut nodeTree, snapId).getSnapCurrentIndex(), 1);
	}

	//==============================================================================
	// 미끄러짐은 0.3 초 cubic.out 이다. (놓은 tick 에 첫 걸음)
	//==============================================================================
	#[test]
	fn snapTweenFollowsCubicOut() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, snapId) = createSnapTree();
		nodeTree.dispatchTouch(snapId, &engine, TouchKind::press, Vector2::create(150.0, 50.0));
		nodeTree.dispatchTouch(snapId, &engine, TouchKind::moved, Vector2::create(50.0, 50.0));
		nodeTree.tick(&engine, 0.016);
		nodeTree.dispatchTouch(snapId, &engine, TouchKind::release, Vector2::create(50.0, 50.0));
		nodeTree.tick(&engine, 0.15);
		let expectedX = math::lerp(-100.0, -200.0, easingFunction::cubic::out(0.5));
		let offset = getSnapView(&mut nodeTree, snapId).getScrollOffset();
		assert!((offset.x - expectedX).abs() < 0.001);
		assert!(getSnapView(&mut nodeTree, snapId).isSnapping());
		nodeTree.tick(&engine, 0.15);
		assert_eq!(getSnapView(&mut nodeTree, snapId).getScrollOffset(), Vector2::create(-200.0, 0.0));
		assert!(!getSnapView(&mut nodeTree, snapId).isSnapping());
	}

	//==============================================================================
	// 칸 위의 버튼을 끌면 TouchRecognizer 가 스냅 뷰로 넘기고, 놓으면 다음 칸으로 간다.
	//==============================================================================
	#[test]
	fn touchRecognizerHandsDragToSnapView() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, snapId) = createSnapTree();
		let contentId = getSnapView(&mut nodeTree, snapId).getContent().expect("content");
		let pageId = nodeTree.getChildIds(contentId)[0];
		nodeTree.addComponent(pageId, UIButton::new());
		let pressPosition = Vector2::create(150.0, 50.0);
		assert_eq!(nodeTree.findTopmostInteractableNode(pressPosition), Some(pageId));
		let mut touchRecognizer = TouchRecognizer::new();
		touchRecognizer.touchPress(&mut nodeTree, &engine, pressPosition);
		touchRecognizer.touchMove(&mut nodeTree, &engine, Vector2::create(50.0, 50.0));
		assert!(getSnapView(&mut nodeTree, snapId).isDragging());
		nodeTree.tick(&engine, 0.016);
		touchRecognizer.touchRelease(&mut nodeTree, &engine, Vector2::create(50.0, 50.0));
		for _ in 0..30 {
			nodeTree.tick(&engine, 0.016);
		}
		assert_eq!(getSnapView(&mut nodeTree, snapId).getSnapCurrentIndex(), 1);
		// 미룬 누름은 드래그가 되며 버려져 버튼은 눌리지 않았다.
		assert_ne!(nodeTree[pageId].getComponent::<UIButton>().expect("button").getButtonState(), crate::ui::uibutton::ButtonState::pressed);
	}

	//==============================================================================
	// pageTo: 바로 옮기기는 content 까지 그 자리에서, 미끄러지기는 tick 으로.
	//==============================================================================
	#[test]
	fn pageToMovesImmediatelyOrSmoothly() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, snapId) = createSnapTree();
		let contentId = getSnapView(&mut nodeTree, snapId).getContent().expect("content");
		nodeTree.withComponentMut::<UISnapScrollView, _>(snapId, |snapView, tree| {
			snapView.pageTo(tree, 9, false);
		});
		assert_eq!(getSnapView(&mut nodeTree, snapId).getSnapCurrentIndex(), 4);
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::create(-800.0, 0.0));
		nodeTree.withComponentMut::<UISnapScrollView, _>(snapId, |snapView, tree| {
			snapView.pageTo(tree, 2, true);
		});
		assert_eq!(getSnapView(&mut nodeTree, snapId).getScrollOffset(), Vector2::create(-800.0, 0.0));
		for _ in 0..20 {
			nodeTree.tick(&engine, 0.016);
		}
		assert_eq!(getSnapView(&mut nodeTree, snapId).getScrollOffset(), Vector2::create(-400.0, 0.0));
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::create(-400.0, 0.0));
	}
}
