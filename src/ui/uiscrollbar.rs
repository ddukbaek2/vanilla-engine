//==============================================================================
// 스크롤바. (원본 ui/uiscrollbar.js 의 UIScrollBar, ScrollBarAxis)
// - 대상 스크롤 뷰의 스크롤 상태를 트랙(바탕) + 썸(끄는 손잡이)으로 보인다. 썸이나 트랙을 끌면 대상의 오프셋을 바로 바꾼다.
// - 매 tick 대상의 오프셋 / 콘텐트 크기 / 보이는 영역을 읽어 썸 위치와 크기를 맞춘다.
// - 원본과 다른 것:
//   - 원본 UIScrollBar 는 WorldNode 를 상속한 노드이고, 트랙 Paint 와 썸 노드(Paint)를 자기 아래에 만든다.
//     러스트 판은 노드에 붙는 컴포넌트이고, 트랙과 썸을 직접 그린다. (썸 노드 없이 썸 사각형만 든다)
//   - 대상은 스크롤 뷰 컴포넌트 대신 UIScrollView 가 붙은 노드의 핸들이다. (setScrollView / getScrollView)
//   - 트랙 누름 자리와 썸 안 판정은 노드 행렬의 역으로 바꾼 로컬 좌표로 한다. (원본은 월드 위치를 뺀다. 회전 / 크기가 없으면 같다)
//   - 썸 색은 원본이 0 ~ 255 를 섞어 쓴 Color(255, 255, 255, 0.6) 이다. 러스트 판은 같은 뜻의 (1, 1, 1, 0.6) 으로 둔다.
// - 보통은 UIScrollView::setShowsVerticalScrollBar 가 만든다. 직접 만들 때는 nodeTree.addComponent 로 붙인다. (피봇 / 앵커 왼쪽 위, interactable)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiscrollview::UIScrollView;

// 트랙 / 썸 둥근 크기.
const SCROLLBAR_ROUND_SIZE: f32 = 4.0;

//==============================================================================
// 스크롤바 축.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollBarAxis {
	horizontal,
	vertical,
}

//==============================================================================
// 대상 스크롤 상태. (스크롤바가 tick / 터치 때 대상 노드에서 읽는다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ScrollState {
	viewportSize: Vector2,
	scrollContentSize: Vector2,
	scrollOffset: Vector2,
}

//==============================================================================
// 축 길이 계산 결과.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ThumbMetrics {
	thumbLength: f32,
	scrollableContent: f32,
	scrollableTrack: f32,
}

//==============================================================================
// 스크롤바.
//==============================================================================
pub struct UIScrollBar {
	componentBase: ComponentBase,
	scrollViewNodeId: Option<NodeId>,
	axis: ScrollBarAxis,
	trackColor: Color,
	thumbColor: Color,
	thumbRect: Option<Rect>,
	isDraggingThumb: bool,
	dragStartViewPosition: Vector2,
	dragStartScrollOffset: Vector2,
	minThumbSize: f32,
	autoHide: bool,
}

impl UIScrollBar {
	//==============================================================================
	// 생성. (세로, 트랙 옅은 검정, 썸 반투명 흰색, 썸 최소 길이 24, 자동 숨김)
	//==============================================================================
	pub fn new() -> UIScrollBar {
		UIScrollBar {
			componentBase: ComponentBase::new(),
			scrollViewNodeId: None,
			axis: ScrollBarAxis::vertical,
			trackColor: Color::create(0.0, 0.0, 0.0, 0.15),
			thumbColor: Color::create(1.0, 1.0, 1.0, 0.6),
			thumbRect: None,
			isDraggingThumb: false,
			dragStartViewPosition: Vector2::zero(),
			dragStartScrollOffset: Vector2::zero(),
			minThumbSize: 24.0,
			autoHide: true,
		}
	}

	//==============================================================================
	// 대상 스크롤 뷰 설정. (UIScrollView 가 붙은 노드)
	//==============================================================================
	pub fn setScrollView(&mut self, scrollViewNodeId: Option<NodeId>) {
		self.scrollViewNodeId = scrollViewNodeId;
	}

	//==============================================================================
	// 대상 스크롤 뷰 반환. (TouchRecognizer 가 자기 스크롤 뷰로 드래그를 넘기지 않으려고 읽는다)
	//==============================================================================
	pub fn getScrollView(&self) -> Option<NodeId> {
		self.scrollViewNodeId
	}

	//==============================================================================
	// 축 설정.
	//==============================================================================
	pub fn setAxis(&mut self, axis: ScrollBarAxis) {
		self.axis = axis;
	}

	//==============================================================================
	// 축 반환.
	//==============================================================================
	pub fn getAxis(&self) -> ScrollBarAxis {
		self.axis
	}

	//==============================================================================
	// 트랙 색 설정.
	//==============================================================================
	pub fn setTrackColor(&mut self, color: impl Into<Color>) {
		self.trackColor = color.into();
	}

	//==============================================================================
	// 썸 색 설정.
	//==============================================================================
	pub fn setThumbColor(&mut self, color: impl Into<Color>) {
		self.thumbColor = color.into();
	}

	//==============================================================================
	// 썸 최소 길이 설정.
	//==============================================================================
	pub fn setMinThumbSize(&mut self, minThumbSize: f32) {
		self.minThumbSize = minThumbSize;
	}

	//==============================================================================
	// 자동 숨김 설정. (콘텐트가 보이는 영역에 들어오면 트랙까지 숨긴다)
	//==============================================================================
	pub fn setAutoHide(&mut self, autoHide: bool) {
		self.autoHide = autoHide;
	}

	//==============================================================================
	// 지금 썸 사각형. (스크롤바 노드 영역 안, 썸이 꺼져 있으면 None)
	//==============================================================================
	pub fn getThumbRect(&self) -> Option<Rect> {
		self.thumbRect
	}

	//==============================================================================
	// 대상 스크롤 상태 읽기.
	//==============================================================================
	fn readScrollState(&self, nodeTree: &NodeTree) -> Option<ScrollState> {
		let scrollViewNodeId = self.scrollViewNodeId?;
		let scrollNode = nodeTree.getNode(scrollViewNodeId)?;
		let scrollView = scrollNode.getComponent::<UIScrollView>()?;
		Some(ScrollState {
			viewportSize: scrollNode.getContentSize(),
			scrollContentSize: scrollView.getScrollContentSize(),
			scrollOffset: scrollView.getScrollOffset(),
		})
	}

	//==============================================================================
	// 축 방향 값 고르기.
	//==============================================================================
	fn pickAxis(&self, vector: Vector2) -> f32 {
		match self.axis {
			ScrollBarAxis::vertical => {
				vector.y
			}
			ScrollBarAxis::horizontal => {
				vector.x
			}
		}
	}

	//==============================================================================
	// 썸 길이와 스크롤 가능한 길이. (스크롤할 것이 없으면 None)
	//==============================================================================
	fn calculateThumbMetrics(&self, scrollState: &ScrollState, trackSize: Vector2) -> Option<ThumbMetrics> {
		let viewportLength = self.pickAxis(scrollState.viewportSize);
		let contentLength = self.pickAxis(scrollState.scrollContentSize);
		let trackLength = self.pickAxis(trackSize);
		if contentLength <= viewportLength || trackLength <= 0.0 {
			return None;
		}
		let ratio = viewportLength / contentLength;
		let thumbLength = self.minThumbSize.max(trackLength * ratio);
		Some(ThumbMetrics {
			thumbLength,
			scrollableContent: contentLength - viewportLength,
			scrollableTrack: trackLength - thumbLength,
		})
	}

	//==============================================================================
	// 트랙 길이 / 보이는 영역 / 콘텐트 크기로 썸 위치와 크기를 맞춘다.
	//==============================================================================
	fn updateThumb(&mut self, context: &mut ComponentContext) {
		let nodeTree = context.getNodeTree();
		let scrollState = match self.readScrollState(nodeTree) {
			Some(scrollState) => {
				scrollState
			}
			None => {
				self.thumbRect = None;
				return;
			}
		};
		let trackSize = context.getTransform().getContentSize();
		let thumbMetrics = match self.calculateThumbMetrics(&scrollState, trackSize) {
			Some(thumbMetrics) => {
				thumbMetrics
			}
			None => {
				// 콘텐트가 보이는 영역 안이면 스크롤할 것이 없다.
				self.thumbRect = None;
				if self.autoHide {
					context.getNodeMut().setActive(false);
				}
				return;
			}
		};
		context.getNodeMut().setActive(true);

		let offsetAlongAxis = self.pickAxis(scrollState.scrollOffset);
		let progress = if thumbMetrics.scrollableContent > 0.0 {
			math::clamp01(-offsetAlongAxis / thumbMetrics.scrollableContent)
		}
		else {
			0.0
		};
		let thumbPositionAlongAxis = thumbMetrics.scrollableTrack * progress;
		let thumbRect = match self.axis {
			ScrollBarAxis::vertical => {
				Rect::create(0.0, thumbPositionAlongAxis, trackSize.x, thumbMetrics.thumbLength)
			}
			ScrollBarAxis::horizontal => {
				Rect::create(thumbPositionAlongAxis, 0.0, thumbMetrics.thumbLength, trackSize.y)
			}
		};
		self.thumbRect = Some(thumbRect);
	}

	//==============================================================================
	// 뷰 좌표 → 스크롤바 노드 로컬 좌표.
	//==============================================================================
	fn convertToLocalPosition(context: &ComponentContext, viewInputPosition: Vector2) -> Option<Vector2> {
		let nodeId = context.getNodeId();
		context.getNodeTree().convertViewPositionToNodePosition(nodeId, viewInputPosition)
	}

	//==============================================================================
	// 트랙을 누른 자리로 콘텐트 오프셋을 옮긴다. (썸 가운데가 누른 자리에 오도록)
	//==============================================================================
	fn applyTrackHitPosition(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		let scrollViewNodeId = match self.scrollViewNodeId {
			Some(scrollViewNodeId) => {
				scrollViewNodeId
			}
			None => {
				return;
			}
		};
		let nodeTree = context.getNodeTree();
		let scrollState = match self.readScrollState(nodeTree) {
			Some(scrollState) => {
				scrollState
			}
			None => {
				return;
			}
		};
		let trackSize = context.getTransform().getContentSize();
		let thumbMetrics = match self.calculateThumbMetrics(&scrollState, trackSize) {
			Some(thumbMetrics) => {
				thumbMetrics
			}
			None => {
				return;
			}
		};
		if thumbMetrics.scrollableTrack <= 0.0 {
			return;
		}
		let localPosition = match UIScrollBar::convertToLocalPosition(context, viewInputPosition) {
			Some(localPosition) => {
				localPosition
			}
			None => {
				return;
			}
		};
		let localOnAxis = self.pickAxis(localPosition);
		let desiredThumbHead = math::clamp(localOnAxis - thumbMetrics.thumbLength * 0.5, 0.0, thumbMetrics.scrollableTrack);
		let progress = desiredThumbHead / thumbMetrics.scrollableTrack;
		let newOffsetOnAxis = -progress * thumbMetrics.scrollableContent;
		let currentOffset = scrollState.scrollOffset;
		let newOffset = match self.axis {
			ScrollBarAxis::vertical => {
				Vector2::create(currentOffset.x, newOffsetOnAxis)
			}
			ScrollBarAxis::horizontal => {
				Vector2::create(newOffsetOnAxis, currentOffset.y)
			}
		};
		let nodeTree = context.getNodeTreeMut();
		UIScrollView::setScrollOffsetInTree(nodeTree, scrollViewNodeId, newOffset);
	}

	//==============================================================================
	// 뷰 좌표가 썸 안인지.
	//==============================================================================
	fn isInsideThumb(&self, context: &ComponentContext, viewInputPosition: Vector2) -> bool {
		let thumbRect = match self.thumbRect {
			Some(thumbRect) => {
				thumbRect
			}
			None => {
				return false;
			}
		};
		match UIScrollBar::convertToLocalPosition(context, viewInputPosition) {
			Some(localPosition) => {
				thumbRect.contains(localPosition)
			}
			None => {
				false
			}
		}
	}
}

impl Default for UIScrollBar {
	fn default() -> UIScrollBar {
		UIScrollBar::new()
	}
}

impl Component for UIScrollBar {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIScrollBar"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자처럼 피봇 / 앵커 왼쪽 위, interactable)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		let scrollBarNode = &mut nodeTree[nodeId];
		scrollBarNode.setPivot(Pivot::topLeft);
		scrollBarNode.setAnchor(Pivot::topLeft);
		scrollBarNode.setInteractable(true);
	}

	fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
		self.updateThumb(context);
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	//==============================================================================
	// 출력. (트랙 → 썸)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let trackSize = transform.getContentSize();
		let trackRect = Rect::create(0.0, 0.0, trackSize.x, trackSize.y);
		graphic.setFillColor(self.trackColor);
		graphic.drawRoundRect(trackRect, SCROLLBAR_ROUND_SIZE);
		if let Some(thumbRect) = self.thumbRect {
			graphic.setFillColor(self.thumbColor);
			graphic.drawRoundRect(thumbRect, SCROLLBAR_ROUND_SIZE);
		}
	}

	//==============================================================================
	// 터치 누름. (썸이면 그 자리에서 끌기 시작, 트랙이면 그 자리로 뛴 뒤 끌기)
	//==============================================================================
	fn touchPress(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		if self.scrollViewNodeId.is_none() {
			return;
		}
		if !self.isInsideThumb(context, viewInputPosition) {
			self.applyTrackHitPosition(context, viewInputPosition);
		}
		self.isDraggingThumb = true;
		self.dragStartViewPosition = viewInputPosition;
		let nodeTree = context.getNodeTree();
		if let Some(scrollState) = self.readScrollState(nodeTree) {
			self.dragStartScrollOffset = scrollState.scrollOffset;
		}
	}

	//==============================================================================
	// 터치 이동. (끈 픽셀만큼 썸이 움직이고, 그 비율로 콘텐트가 반대로 움직인다)
	//==============================================================================
	fn touchMove(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		if !self.isDraggingThumb {
			return;
		}
		let scrollViewNodeId = match self.scrollViewNodeId {
			Some(scrollViewNodeId) => {
				scrollViewNodeId
			}
			None => {
				return;
			}
		};
		let nodeTree = context.getNodeTree();
		let scrollState = match self.readScrollState(nodeTree) {
			Some(scrollState) => {
				scrollState
			}
			None => {
				return;
			}
		};
		let trackSize = context.getTransform().getContentSize();
		let thumbMetrics = match self.calculateThumbMetrics(&scrollState, trackSize) {
			Some(thumbMetrics) => {
				thumbMetrics
			}
			None => {
				return;
			}
		};
		if thumbMetrics.scrollableTrack <= 0.0 {
			return;
		}
		let dragDelta = self.pickAxis(viewInputPosition.subtract(self.dragStartViewPosition));
		let offsetDelta = -dragDelta * (thumbMetrics.scrollableContent / thumbMetrics.scrollableTrack);
		let startOffset = self.dragStartScrollOffset;
		let newOffset = match self.axis {
			ScrollBarAxis::vertical => {
				Vector2::create(startOffset.x, startOffset.y + offsetDelta)
			}
			ScrollBarAxis::horizontal => {
				Vector2::create(startOffset.x + offsetDelta, startOffset.y)
			}
		};
		let nodeTree = context.getNodeTreeMut();
		UIScrollView::setScrollOffsetInTree(nodeTree, scrollViewNodeId, newOffset);
	}

	fn touchRelease(&mut self, _context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.isDraggingThumb = false;
	}

	fn touchCancel(&mut self, _context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.isDraggingThumb = false;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;

	//==============================================================================
	// 썸 크기와 자리는 보이는 비율과 오프셋을 따르고, 썸을 끌면 콘텐트가 반대로 움직인다.
	//==============================================================================
	#[test]
	fn thumbFollowsOffsetAndDragScrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let scrollId = nodeTree.createChild(rootId, "scroll");
		let scrollNode = &mut nodeTree[scrollId];
		scrollNode.setPivot(Pivot::topLeft);
		scrollNode.setContentSize(Vector2::create(200.0, 112.0));
		{
			let scrollView = nodeTree.addComponent(scrollId, UIScrollView::new()).expect("scroll view");
			scrollView.setScrollContentSize(Vector2::create(200.0, 448.0));
			scrollView.setShowsVerticalScrollBar(true);
		}
		nodeTree.tick(&engine, 0.016);
		let scrollBarId = nodeTree[scrollId].getComponent::<UIScrollView>().expect("scroll view").getVerticalScrollBar().expect("scroll bar");
		// 트랙 길이 100, 보이는 비율 1/4 → 썸 길이 25.
		let thumbRect = nodeTree[scrollBarId].getComponent::<UIScrollBar>().expect("scroll bar").getThumbRect().expect("thumb");
		assert_eq!(thumbRect, Rect::create(0.0, 0.0, 12.0, 25.0));

		// 썸을 끝까지 끌면 콘텐트가 끝까지 간다. (썸 75 픽셀 = 콘텐트 336 픽셀)
		let thumbCenter = Vector2::create(182.0 + 6.0, 6.0 + 12.0);
		nodeTree.dispatchTouch(scrollBarId, &engine, TouchKind::press, thumbCenter);
		nodeTree.dispatchTouch(scrollBarId, &engine, TouchKind::moved, Vector2::create(thumbCenter.x, thumbCenter.y + 75.0));
		let scrollOffset = nodeTree[scrollId].getComponent::<UIScrollView>().expect("scroll view").getScrollOffset();
		assert!((scrollOffset.y + 336.0).abs() < 0.001);
		nodeTree.dispatchTouch(scrollBarId, &engine, TouchKind::release, thumbCenter);
		nodeTree.tick(&engine, 0.016);
		let movedThumbRect = nodeTree[scrollBarId].getComponent::<UIScrollBar>().expect("scroll bar").getThumbRect().expect("thumb");
		assert!((movedThumbRect.position.y - 75.0).abs() < 0.001);

		// 트랙의 맨 위를 누르면 썸이 맨 위로 뛴다.
		nodeTree.dispatchTouch(scrollBarId, &engine, TouchKind::press, Vector2::create(188.0, 7.0));
		let jumpedOffset = nodeTree[scrollId].getComponent::<UIScrollView>().expect("scroll view").getScrollOffset();
		assert_eq!(jumpedOffset.y, 0.0);
	}
}
