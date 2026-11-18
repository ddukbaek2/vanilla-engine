//==============================================================================
// 스크롤 뷰. (원본 ui/uiscrollview.js 의 UIScrollView, ScrollMode)
// - 호스트 contentSize 를 보이는 영역으로 쓰고, content 노드를 끌거나 휠을 굴려 움직인다. 스크롤 오프셋 = content 노드의 로컬 위치.
// - 원본처럼 호스트에 Mask 를 붙여(require) content 를 자르고, 호스트를 interactable 로 켠다. nodeTree.addComponent(nodeId, UIScrollView::new()) 로 붙인다.
// - 스크롤바(UIScrollBar 가 붙은 노드)는 원본 코드처럼 호스트의 자식(content 의 형제)으로 만든다.
// - 휠: 원본 TouchRaycaster.touchWheel → UIScrollView.wheel 길. 엔진이 터치 아래 노드부터 부모 쪽으로 wheel 을 묻고, 이 컴포넌트가 받아 쓴다.
// - 원본과 다른 것:
//   - 원본은 setScrollOffset / setScrollContentSize / setShows...ScrollBar 에서 바로 노드를 바꾼다. 러스트 판의 씬 코드는 컴포넌트만 쥐고 있어
//     값을 정해 두고 다음 tick 에 content 노드와 스크롤바 노드에 반영한다. 같은 프레임에 바로 옮기려면 UIScrollView::setScrollOffsetInTree 를 쓴다.
//   - 그래서 getVerticalScrollBar / getHorizontalScrollBar 는 보이기로 정한 뒤 첫 tick 이 지나야 노드 핸들을 돌려준다.
//   - 보이는 영역 크기(호스트 contentSize)는 붙을 때와 tick / 터치 / 휠 때 읽어 둔다. 씬 코드에서 정한 오프셋은 다음 tick 에 새 크기로 다시 자른다.
//   - 드래그를 위 스크롤 뷰로 넘기는 손짓(원본 TouchRecognizer)은 ui/touchrecognizer.rs 에 따로 있다. 씬이 골라 쓴다.
// - 원본 그대로인 버릇: 콘텐트가 영역 안에 들어와 스스로 꺼진(자동 숨김) 스크롤바 노드는 tick 이 돌지 않아 저절로 다시 켜지지 않는다.
//   setShows...ScrollBar(true) 를 다시 부르면 켜진다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::mask::Mask;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiscrollbar::{ScrollBarAxis, UIScrollBar};
use crate::ui::uiview::UIView;

// 스크롤바 기본 두께 / 여백. (UIKit 표시기 꼴)
const SCROLLBAR_THICKNESS: f32 = 12.0;
const SCROLLBAR_MARGIN: f32 = 6.0;
const WHEEL_SCROLL_SCALE: f32 = 1.0;

// 탄성 모드 상수. (원본과 같다)
const ELASTIC_RESISTANCE: f32 = 0.5;
const SPRING_CONSTANT: f32 = 1200.0;
const DAMPING_COEFFICIENT: f32 = 30.0;
const FRICTION_COEFFICIENT: f32 = 5.0;

//==============================================================================
// 스크롤 모드.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMode {
	// 경계를 넘지 않는 기본 모드.
	clamp,
	// 경계를 넘으면 저항이 생기고, 놓으면 스프링처럼 튕겨 돌아온다.
	elastic,
}

//==============================================================================
// 스크롤 뷰.
//==============================================================================
pub struct UIScrollView {
	uiView: UIView,
	scrollContentSize: Vector2,
	scrollMode: ScrollMode,
	scrollVelocity: Vector2,
	previousViewInputPosition: Vector2,
	currentViewInputPosition: Vector2,
	horizontalEnabled: bool,
	verticalEnabled: bool,
	scrollOffset: Vector2,
	isDragging: bool,
	dragStartViewPosition: Vector2,
	dragStartOffset: Vector2,
	dragSensitivity: f32,
	verticalScrollBarId: Option<NodeId>,
	horizontalScrollBarId: Option<NodeId>,
	showsVerticalScrollBar: bool,
	showsHorizontalScrollBar: bool,
	pendingVerticalScrollBarActive: Option<bool>,
	pendingHorizontalScrollBarActive: Option<bool>,
	wheelScrollScale: f32,
	passesCrossAxisDrag: bool,
	viewportSize: Vector2,
	isOffsetClampPending: bool,
	isContentPositionSyncPending: bool,
	isContentSizeSyncPending: bool,
}

impl UIScrollView {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIScrollView {
		UIScrollView {
			uiView: UIView::new(),
			scrollContentSize: Vector2::zero(),
			scrollMode: ScrollMode::clamp,
			scrollVelocity: Vector2::zero(),
			previousViewInputPosition: Vector2::zero(),
			currentViewInputPosition: Vector2::zero(),
			horizontalEnabled: true,
			verticalEnabled: true,
			scrollOffset: Vector2::zero(),
			isDragging: false,
			dragStartViewPosition: Vector2::zero(),
			dragStartOffset: Vector2::zero(),
			dragSensitivity: 1.0,
			verticalScrollBarId: None,
			horizontalScrollBarId: None,
			showsVerticalScrollBar: false,
			showsHorizontalScrollBar: false,
			pendingVerticalScrollBarActive: None,
			pendingHorizontalScrollBarActive: None,
			wheelScrollScale: WHEEL_SCROLL_SCALE,
			passesCrossAxisDrag: false,
			viewportSize: Vector2::zero(),
			isOffsetClampPending: false,
			isContentPositionSyncPending: false,
			isContentSizeSyncPending: false,
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
	// 컨텐트 노드 핸들. (스크롤할 자식 노드를 여기에 붙인다)
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

	//==============================================================================
	// 트리 안에서 오프셋을 정하고 content 노드를 바로 옮긴다. (스크롤 뷰가 없거나 지금 꺼내져 있으면 false)
	// - 스크롤바가 끌 때, 그리고 씬 코드가 같은 프레임에 바로 옮기고 싶을 때 쓴다.
	//==============================================================================
	pub fn setScrollOffsetInTree(nodeTree: &mut NodeTree, scrollNodeId: NodeId, offset: Vector2) -> bool {
		let viewportSize = match nodeTree.getNode(scrollNodeId) {
			Some(scrollNode) => {
				scrollNode.getContentSize()
			}
			None => {
				return false;
			}
		};
		let (contentId, scrollOffset) = match nodeTree[scrollNodeId].getComponentMut::<UIScrollView>() {
			Some(scrollView) => {
				scrollView.viewportSize = viewportSize;
				scrollView.applyScrollOffset(offset);
				scrollView.isContentPositionSyncPending = false;
				let contentId = scrollView.getContent();
				let scrollOffset = scrollView.getScrollOffset();
				(contentId, scrollOffset)
			}
			None => {
				return false;
			}
		};
		if let Some(content) = contentId.and_then(|contentId| nodeTree.getNodeMut(contentId)) {
			content.setLocalPosition(scrollOffset);
		}
		true
	}

	//==============================================================================
	// 미뤄 둔 content 노드 위치 / 크기 반영. (tick / 터치 / 휠 때)
	//==============================================================================
	fn syncContentNode(&mut self, context: &mut ComponentContext) {
		if !self.isContentPositionSyncPending && !self.isContentSizeSyncPending {
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
		let scrollOffset = self.getScrollOffset();
		let scrollContentSize = self.getScrollContentSize();
		if let Some(content) = context.getNodeTreeMut().getNodeMut(contentId) {
			if self.isContentPositionSyncPending {
				content.setLocalPosition(scrollOffset);
			}
			if self.isContentSizeSyncPending {
				content.setContentSize(scrollContentSize);
			}
		}
		self.isContentPositionSyncPending = false;
		self.isContentSizeSyncPending = false;
	}

	//==============================================================================
	// 보이는 영역 크기 읽어 두기. (호스트 contentSize)
	//==============================================================================
	fn updateViewportSize(&mut self, context: &ComponentContext) {
		self.viewportSize = context.getTransform().getContentSize();
	}

	//==============================================================================
	// 오프셋 범위. (최소 x, 최소 y. 최대는 둘 다 0. 끈 축은 0)
	//==============================================================================
	fn calculateMinimumOffset(&self) -> Vector2 {
		let viewportSize = self.viewportSize;
		let scrollContentSize = self.getScrollContentSize();
		let minimumX = if self.isHorizontal() {
			(viewportSize.x - scrollContentSize.x).min(0.0)
		}
		else {
			0.0
		};
		let minimumY = if self.isVertical() {
			(viewportSize.y - scrollContentSize.y).min(0.0)
		}
		else {
			0.0
		};
		Vector2::create(minimumX, minimumY)
	}

	//==============================================================================
	// 드래그 중 오프셋과 속도 계산. (tick)
	//==============================================================================
	fn updateDrag(&mut self, timeDelta: f32) {
		let viewInputPosition = self.currentViewInputPosition;
		let dragSensitivity = self.getDragSensitivity();
		let rawDeltaX = (viewInputPosition.x - self.dragStartViewPosition.x) * dragSensitivity;
		let rawDeltaY = (viewInputPosition.y - self.dragStartViewPosition.y) * dragSensitivity;
		let deltaX = if self.isHorizontal() {
			rawDeltaX
		}
		else {
			0.0
		};
		let deltaY = if self.isVertical() {
			rawDeltaY
		}
		else {
			0.0
		};
		let proposedOffset = Vector2::create(self.dragStartOffset.x + deltaX, self.dragStartOffset.y + deltaY);
		if self.getScrollMode() == ScrollMode::elastic {
			// 원본처럼 탄성 범위는 끈 축도 계산한다. (끈 축은 deltaX / deltaY 가 0 이라 넘지 않는다)
			let viewportSize = self.viewportSize;
			let scrollContentSize = self.getScrollContentSize();
			let elasticMaximumX = 0.0;
			let elasticMinimumX = (viewportSize.x - scrollContentSize.x).min(0.0);
			let elasticMaximumY = 0.0;
			let elasticMinimumY = (viewportSize.y - scrollContentSize.y).min(0.0);
			let mut elasticOffsetX = proposedOffset.x;
			let mut elasticOffsetY = proposedOffset.y;
			if elasticOffsetX > elasticMaximumX {
				elasticOffsetX = elasticMaximumX + (elasticOffsetX - elasticMaximumX) * ELASTIC_RESISTANCE;
			}
			else if elasticOffsetX < elasticMinimumX {
				elasticOffsetX = elasticMinimumX + (elasticOffsetX - elasticMinimumX) * ELASTIC_RESISTANCE;
			}
			if elasticOffsetY > elasticMaximumY {
				elasticOffsetY = elasticMaximumY + (elasticOffsetY - elasticMaximumY) * ELASTIC_RESISTANCE;
			}
			else if elasticOffsetY < elasticMinimumY {
				elasticOffsetY = elasticMinimumY + (elasticOffsetY - elasticMinimumY) * ELASTIC_RESISTANCE;
			}
			self.scrollOffset = Vector2::create(elasticOffsetX, elasticOffsetY);
			self.isContentPositionSyncPending = true;

			if timeDelta > 0.0 {
				let rawVelocityX = (viewInputPosition.x - self.previousViewInputPosition.x) / timeDelta;
				let rawVelocityY = (viewInputPosition.y - self.previousViewInputPosition.y) / timeDelta;
				let velocityX = if self.isHorizontal() {
					rawVelocityX
				}
				else {
					0.0
				};
				let velocityY = if self.isVertical() {
					rawVelocityY
				}
				else {
					0.0
				};
				self.scrollVelocity = Vector2::create(velocityX, velocityY);
			}
		}
		else {
			self.applyScrollOffset(proposedOffset);
		}
		self.previousViewInputPosition = viewInputPosition;
	}

	//==============================================================================
	// 탄성 모드에서 놓은 뒤 스프링 / 마찰 물리. (tick)
	//==============================================================================
	fn updateElasticPhysics(&mut self, timeDelta: f32) {
		let minimumOffset = self.calculateMinimumOffset();
		let boundsMaximumX = 0.0;
		let boundsMinimumX = minimumOffset.x;
		let boundsMaximumY = 0.0;
		let boundsMinimumY = minimumOffset.y;
		let scrollVelocity = self.getScrollVelocity();
		let mut velocityX = scrollVelocity.x;
		let mut velocityY = scrollVelocity.y;
		let currentScrollOffset = self.getScrollOffset();
		let mut offsetX = currentScrollOffset.x;
		let mut offsetY = currentScrollOffset.y;
		let clampedX = math::clamp(offsetX, boundsMinimumX, boundsMaximumX);
		let clampedY = math::clamp(offsetY, boundsMinimumY, boundsMaximumY);
		let displacementX = offsetX - clampedX;
		let displacementY = offsetY - clampedY;
		let isOutOfBounds = displacementX != 0.0 || displacementY != 0.0;
		if isOutOfBounds {
			let springForceX = -displacementX * SPRING_CONSTANT;
			let springForceY = -displacementY * SPRING_CONSTANT;
			let dampingForceX = -velocityX * DAMPING_COEFFICIENT;
			let dampingForceY = -velocityY * DAMPING_COEFFICIENT;
			velocityX += (springForceX + dampingForceX) * timeDelta;
			velocityY += (springForceY + dampingForceY) * timeDelta;
		}
		else {
			let frictionFactor = (1.0 - FRICTION_COEFFICIENT * timeDelta).max(0.0);
			velocityX *= frictionFactor;
			velocityY *= frictionFactor;
		}
		offsetX += velocityX * timeDelta;
		offsetY += velocityY * timeDelta;

		// 경계 밖에서 돌아오다 경계를 넘어 안으로 들어오면 경계에서 멈춘다.
		if displacementX > 0.0 && offsetX < boundsMaximumX {
			offsetX = boundsMaximumX;
			velocityX = 0.0;
		}
		else if displacementX < 0.0 && offsetX > boundsMinimumX {
			offsetX = boundsMinimumX;
			velocityX = 0.0;
		}
		if displacementY > 0.0 && offsetY < boundsMaximumY {
			offsetY = boundsMaximumY;
			velocityY = 0.0;
		}
		else if displacementY < 0.0 && offsetY > boundsMinimumY {
			offsetY = boundsMinimumY;
			velocityY = 0.0;
		}

		let newClampedX = math::clamp(offsetX, boundsMinimumX, boundsMaximumX);
		let newClampedY = math::clamp(offsetY, boundsMinimumY, boundsMaximumY);
		let newDisplacementX = offsetX - newClampedX;
		let newDisplacementY = offsetY - newClampedY;
		let speedSquared = velocityX * velocityX + velocityY * velocityY;
		if speedSquared < 1.0 && newDisplacementX.abs() < 0.5 && newDisplacementY.abs() < 0.5 {
			offsetX = newClampedX;
			offsetY = newClampedY;
			velocityX = 0.0;
			velocityY = 0.0;
		}
		self.scrollOffset = Vector2::create(offsetX, offsetY);
		self.isContentPositionSyncPending = true;
		self.scrollVelocity = Vector2::create(velocityX, velocityY);
	}

	//==============================================================================
	// 오프셋 적용. (범위로 자른다. content 노드는 다음 반영 때 옮긴다)
	//==============================================================================
	pub fn applyScrollOffset(&mut self, offset: Vector2) {
		let minimumOffset = self.calculateMinimumOffset();
		self.scrollOffset = Vector2::create(math::clamp(offset.x, minimumOffset.x, 0.0), math::clamp(offset.y, minimumOffset.y, 0.0));
		self.isContentPositionSyncPending = true;
	}

	//==============================================================================
	// 오프셋 설정. (씬 코드용. 다음 tick 에 그때의 영역 크기로 다시 잘라 content 를 옮긴다)
	//==============================================================================
	pub fn setScrollOffset(&mut self, offset: Vector2) {
		self.applyScrollOffset(offset);
		self.isOffsetClampPending = true;
	}

	//==============================================================================
	// 오프셋 반환. (content 노드의 로컬 위치, 아래로 스크롤하면 y 가 음수)
	//==============================================================================
	pub fn getScrollOffset(&self) -> Vector2 {
		self.scrollOffset
	}

	//==============================================================================
	// 가로 스크롤 켜기 설정.
	//==============================================================================
	pub fn setHorizontal(&mut self, isHorizontal: bool) {
		self.horizontalEnabled = isHorizontal;
	}

	//==============================================================================
	// 가로 스크롤 켜짐 여부.
	//==============================================================================
	pub fn isHorizontal(&self) -> bool {
		self.horizontalEnabled
	}

	//==============================================================================
	// 세로 스크롤 켜기 설정.
	//==============================================================================
	pub fn setVertical(&mut self, isVertical: bool) {
		self.verticalEnabled = isVertical;
	}

	//==============================================================================
	// 세로 스크롤 켜짐 여부.
	//==============================================================================
	pub fn isVertical(&self) -> bool {
		self.verticalEnabled
	}

	//==============================================================================
	// 스크롤하지 않는 축으로 끄는 손가락을 누른 노드에 남길지 설정. (기본 false, TouchRecognizer 가 읽는다)
	//==============================================================================
	pub fn setPassesCrossAxisDrag(&mut self, passesCrossAxisDrag: bool) {
		self.passesCrossAxisDrag = passesCrossAxisDrag;
	}

	//==============================================================================
	// 스크롤하지 않는 축으로 끄는 손가락을 누른 노드에 남기는지.
	//==============================================================================
	pub fn isPassingCrossAxisDrag(&self) -> bool {
		self.passesCrossAxisDrag
	}

	//==============================================================================
	// 끈 방향이 이 뷰가 가져갈 드래그인지. (스크롤하지 않는 축의 드래그를 남기는 옵션을 켰을 때만 가려낸다)
	//==============================================================================
	pub fn isScrollDrag(&self, deltaX: f32, deltaY: f32) -> bool {
		if !self.passesCrossAxisDrag {
			return true;
		}
		if self.horizontalEnabled && self.verticalEnabled {
			return true;
		}
		let absoluteX = deltaX.abs();
		let absoluteY = deltaY.abs();
		if self.verticalEnabled {
			return absoluteY >= absoluteX;
		}
		if self.horizontalEnabled {
			return absoluteX >= absoluteY;
		}
		false
	}

	//==============================================================================
	// 드래그 반영량 배율 설정. (1 = 기본, 2 = 두 배 빠르게)
	//==============================================================================
	pub fn setDragSensitivity(&mut self, dragSensitivity: f32) {
		self.dragSensitivity = dragSensitivity;
	}

	//==============================================================================
	// 드래그 반영량 배율 반환.
	//==============================================================================
	pub fn getDragSensitivity(&self) -> f32 {
		self.dragSensitivity
	}

	//==============================================================================
	// 스크롤 모드 설정.
	//==============================================================================
	pub fn setScrollMode(&mut self, scrollMode: ScrollMode) {
		self.scrollMode = scrollMode;
	}

	//==============================================================================
	// 스크롤 모드 반환.
	//==============================================================================
	pub fn getScrollMode(&self) -> ScrollMode {
		self.scrollMode
	}

	//==============================================================================
	// 스크롤 콘텐츠 크기 설정. (보이는 영역보다 커야 스크롤된다. content 노드 크기는 다음 tick 에 맞춘다)
	//==============================================================================
	pub fn setScrollContentSize(&mut self, scrollContentSize: Vector2) {
		self.scrollContentSize = scrollContentSize;
		self.isContentSizeSyncPending = true;
	}

	//==============================================================================
	// 스크롤 콘텐츠 크기 반환.
	//==============================================================================
	pub fn getScrollContentSize(&self) -> Vector2 {
		self.scrollContentSize
	}

	//==============================================================================
	// 드래그 중인지.
	//==============================================================================
	pub fn isDragging(&self) -> bool {
		self.isDragging
	}

	//==============================================================================
	// 지금 스크롤 속도 반환.
	//==============================================================================
	pub fn getScrollVelocity(&self) -> Vector2 {
		self.scrollVelocity
	}

	//==============================================================================
	// 세로 스크롤바 보이기 설정. (true 면 다음 tick 에 호스트의 자식으로 만들어 켠다)
	//==============================================================================
	pub fn setShowsVerticalScrollBar(&mut self, shows: bool) {
		self.showsVerticalScrollBar = shows;
		self.pendingVerticalScrollBarActive = Some(shows);
	}

	//==============================================================================
	// 가로 스크롤바 보이기 설정.
	//==============================================================================
	pub fn setShowsHorizontalScrollBar(&mut self, shows: bool) {
		self.showsHorizontalScrollBar = shows;
		self.pendingHorizontalScrollBarActive = Some(shows);
	}

	//==============================================================================
	// 세로 스크롤바 노드 핸들. (꾸밀 때 nodeTree[id].getComponentMut::<UIScrollBar>())
	//==============================================================================
	pub fn getVerticalScrollBar(&self) -> Option<NodeId> {
		self.verticalScrollBarId
	}

	//==============================================================================
	// 가로 스크롤바 노드 핸들.
	//==============================================================================
	pub fn getHorizontalScrollBar(&self) -> Option<NodeId> {
		self.horizontalScrollBarId
	}

	//==============================================================================
	// 세로 스크롤바가 차지하는 폭. (보이는 중일 때만)
	//==============================================================================
	pub fn getVerticalScrollBarReservedWidth(&self) -> f32 {
		if self.showsVerticalScrollBar && self.verticalScrollBarId.is_some() {
			SCROLLBAR_THICKNESS + SCROLLBAR_MARGIN * 2.0
		}
		else {
			0.0
		}
	}

	//==============================================================================
	// 가로 스크롤바가 차지하는 높이. (보이는 중일 때만)
	//==============================================================================
	pub fn getHorizontalScrollBarReservedHeight(&self) -> f32 {
		if self.showsHorizontalScrollBar && self.horizontalScrollBarId.is_some() {
			SCROLLBAR_THICKNESS + SCROLLBAR_MARGIN * 2.0
		}
		else {
			0.0
		}
	}

	//==============================================================================
	// 스크롤바를 뺀, 콘텐트가 그려질 수 있는 보이는 영역 크기.
	//==============================================================================
	pub fn getInnerContentSize(&self) -> Vector2 {
		let viewportSize = self.viewportSize;
		let reservedX = self.getVerticalScrollBarReservedWidth();
		let reservedY = self.getHorizontalScrollBarReservedHeight();
		Vector2::create((viewportSize.x - reservedX).max(0.0), (viewportSize.y - reservedY).max(0.0))
	}

	//==============================================================================
	// 휠 스크롤 배율 설정.
	//==============================================================================
	pub fn setWheelScrollScale(&mut self, wheelScrollScale: f32) {
		self.wheelScrollScale = wheelScrollScale;
	}

	//==============================================================================
	// 스크롤바 노드 하나 만들기. (호스트의 자식)
	//==============================================================================
	fn createScrollBar(context: &mut ComponentContext, name: &str, axis: ScrollBarAxis) -> NodeId {
		let scrollNodeId = context.getNodeId();
		let nodeTree = context.getNodeTreeMut();
		let scrollBarId = nodeTree.createChild(scrollNodeId, name);
		let mut scrollBar = UIScrollBar::new();
		scrollBar.setAxis(axis);
		scrollBar.setScrollView(Some(scrollNodeId));
		nodeTree.addComponent(scrollBarId, scrollBar);
		scrollBarId
	}

	//==============================================================================
	// 보이기로 정한 스크롤바를 만들고, 미뤄 둔 켜기 / 끄기를 반영한다.
	//==============================================================================
	fn updateScrollBarNodes(&mut self, context: &mut ComponentContext) {
		if self.showsVerticalScrollBar && self.verticalScrollBarId.is_none() {
			let scrollBarId = UIScrollView::createScrollBar(context, "verticalScrollBar", ScrollBarAxis::vertical);
			self.verticalScrollBarId = Some(scrollBarId);
		}
		if self.showsHorizontalScrollBar && self.horizontalScrollBarId.is_none() {
			let scrollBarId = UIScrollView::createScrollBar(context, "horizontalScrollBar", ScrollBarAxis::horizontal);
			self.horizontalScrollBarId = Some(scrollBarId);
		}
		let nodeTree = context.getNodeTreeMut();
		if let Some(isActive) = self.pendingVerticalScrollBarActive.take() {
			if let Some(scrollBar) = self.verticalScrollBarId.and_then(|scrollBarId| nodeTree.getNodeMut(scrollBarId)) {
				scrollBar.setActive(isActive);
			}
		}
		if let Some(isActive) = self.pendingHorizontalScrollBarActive.take() {
			if let Some(scrollBar) = self.horizontalScrollBarId.and_then(|scrollBarId| nodeTree.getNodeMut(scrollBarId)) {
				scrollBar.setActive(isActive);
			}
		}
	}

	//==============================================================================
	// 매 tick 스크롤바 위치 / 크기 갱신.
	//==============================================================================
	fn layoutScrollBars(&mut self, context: &mut ComponentContext) {
		let viewportSize = self.viewportSize;
		let reservedX = self.getVerticalScrollBarReservedWidth();
		let reservedY = self.getHorizontalScrollBarReservedHeight();
		let showsVerticalScrollBar = self.showsVerticalScrollBar;
		let showsHorizontalScrollBar = self.showsHorizontalScrollBar;
		let nodeTree = context.getNodeTreeMut();
		if showsVerticalScrollBar {
			if let Some(scrollBar) = self.verticalScrollBarId.and_then(|scrollBarId| nodeTree.getNodeMut(scrollBarId)) {
				let positionX = viewportSize.x - SCROLLBAR_THICKNESS - SCROLLBAR_MARGIN;
				let positionY = SCROLLBAR_MARGIN;
				let height = (viewportSize.y - SCROLLBAR_MARGIN * 2.0 - reservedY).max(0.0);
				scrollBar.setLocalPosition(Vector2::create(positionX, positionY));
				scrollBar.setContentSize(Vector2::create(SCROLLBAR_THICKNESS, height));
			}
		}
		if showsHorizontalScrollBar {
			if let Some(scrollBar) = self.horizontalScrollBarId.and_then(|scrollBarId| nodeTree.getNodeMut(scrollBarId)) {
				let positionX = SCROLLBAR_MARGIN;
				let positionY = viewportSize.y - SCROLLBAR_THICKNESS - SCROLLBAR_MARGIN;
				let width = (viewportSize.x - SCROLLBAR_MARGIN * 2.0 - reservedX).max(0.0);
				scrollBar.setLocalPosition(Vector2::create(positionX, positionY));
				scrollBar.setContentSize(Vector2::create(width, SCROLLBAR_THICKNESS));
			}
		}
	}
}

impl Default for UIScrollView {
	fn default() -> UIScrollView {
		UIScrollView::new()
	}
}

impl Component for UIScrollView {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiView.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiView.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"ScrollView"
	}

	fn attach(&mut self, transform: &mut NodeTransform) {
		self.viewportSize = transform.getContentSize();
	}

	//==============================================================================
	// 트리에 붙음. (content 노드, Mask, interactable)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiView.createContentNode(nodeTree, nodeId);
		let scrollNode = &mut nodeTree[nodeId];
		if !scrollNode.hasComponent::<Mask>() {
			scrollNode.addComponent(Mask::new());
		}
		scrollNode.setInteractable(true);
	}

	//==============================================================================
	// 갱신. (스크롤바 → 드래그 → 탄성 물리 → content 노드 반영)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		self.updateViewportSize(context);
		if self.isOffsetClampPending {
			self.isOffsetClampPending = false;
			let scrollOffset = self.scrollOffset;
			self.applyScrollOffset(scrollOffset);
		}

		// 스크롤바 자동 레이아웃.
		self.updateScrollBarNodes(context);
		self.layoutScrollBars(context);

		// 드래그 중 오프셋과 속도.
		if self.isDragging() {
			self.updateDrag(timeDelta);
		}

		// 탄성 모드에서 놓은 뒤.
		if self.getScrollMode() == ScrollMode::elastic && !self.isDragging() {
			self.updateElasticPhysics(timeDelta);
		}
		self.syncContentNode(context);
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.uiView.drawBackground(transform, graphic);
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn touchPress(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.updateViewportSize(context);
		self.isDragging = true;
		self.dragStartViewPosition = viewInputPosition;
		self.dragStartOffset = self.getScrollOffset();
		self.scrollVelocity = Vector2::zero();
		self.previousViewInputPosition = viewInputPosition;
		self.currentViewInputPosition = viewInputPosition;
	}

	fn touchMove(&mut self, _context: &mut ComponentContext, viewInputPosition: Vector2) {
		if self.isDragging() {
			self.currentViewInputPosition = viewInputPosition;
		}
	}

	fn touchRelease(&mut self, _context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.isDragging = false;
	}

	fn touchCancel(&mut self, _context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.isDragging = false;
	}

	//==============================================================================
	// 휠. (DOM WheelEvent 와 같은 부호: 아래로 굴리면 +y → 콘텐트가 위로. 늘 받아 쓴다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheelDelta: Vector2) -> bool {
		self.updateViewportSize(context);
		let offset = self.getScrollOffset();
		let deltaX = if self.isHorizontal() {
			wheelDelta.x * self.wheelScrollScale
		}
		else {
			0.0
		};
		let deltaY = if self.isVertical() {
			wheelDelta.y * self.wheelScrollScale
		}
		else {
			0.0
		};
		self.applyScrollOffset(Vector2::create(offset.x - deltaX, offset.y - deltaY));
		// 휠 다음에는 탄성 모드의 남은 속도를 비워 튕기지 않게 한다.
		self.scrollVelocity = Vector2::zero();
		self.syncContentNode(context);
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;

	//==============================================================================
	// 시험 장면. (왼쪽 위 (100, 100), 크기 200 x 100 인 스크롤 뷰, 콘텐트 200 x 400)
	//==============================================================================
	fn createScrollTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let scrollId = nodeTree.createChild(rootId, "scroll");
		let scrollNode = &mut nodeTree[scrollId];
		scrollNode.setPivot(Pivot::topLeft);
		scrollNode.setLocalPosition(Vector2::create(100.0, 100.0));
		scrollNode.setContentSize(Vector2::create(200.0, 100.0));
		let scrollView = nodeTree.addComponent(scrollId, UIScrollView::new()).expect("scroll view");
		scrollView.setScrollContentSize(Vector2::create(200.0, 400.0));
		(nodeTree, scrollId)
	}

	//==============================================================================
	// 스크롤 뷰 반환.
	//==============================================================================
	fn getScrollView(nodeTree: &mut NodeTree, scrollId: NodeId) -> &mut UIScrollView {
		nodeTree[scrollId].getComponentMut::<UIScrollView>().expect("scroll view")
	}

	//==============================================================================
	// 붙이면 content, Mask, interactable 이 생기고 tick 에 content 크기가 맞춰진다.
	//==============================================================================
	#[test]
	fn attachCreatesContentAndMask() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, scrollId) = createScrollTree();
		assert!(nodeTree[scrollId].hasComponent::<Mask>());
		assert!(nodeTree[scrollId].isInteractable());
		let contentId = getScrollView(&mut nodeTree, scrollId).getContent().expect("content");
		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[contentId].getContentSize(), Vector2::create(200.0, 400.0));
	}

	//==============================================================================
	// 오프셋은 [보이는 크기 - 콘텐트 크기, 0] 으로 잘린다. 끈 축은 0.
	//==============================================================================
	#[test]
	fn offsetClamping() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, scrollId) = createScrollTree();
		{
			let scrollView = getScrollView(&mut nodeTree, scrollId);
			scrollView.setScrollOffset(Vector2::create(-50.0, -1000.0));
			assert_eq!(scrollView.getScrollOffset(), Vector2::create(0.0, -300.0));
			scrollView.setScrollOffset(Vector2::create(0.0, 30.0));
			assert_eq!(scrollView.getScrollOffset(), Vector2::create(0.0, 0.0));
			scrollView.setScrollOffset(Vector2::create(0.0, -120.0));
		}
		let contentId = getScrollView(&mut nodeTree, scrollId).getContent().expect("content");
		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::create(0.0, -120.0));

		// 세로를 끄면 y 는 0 으로 잘린다.
		getScrollView(&mut nodeTree, scrollId).setVertical(false);
		getScrollView(&mut nodeTree, scrollId).setScrollOffset(Vector2::create(0.0, -120.0));
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getScrollOffset(), Vector2::zero());

		// 트리 안에서 정하면 content 가 바로 옮겨진다.
		getScrollView(&mut nodeTree, scrollId).setVertical(true);
		assert!(UIScrollView::setScrollOffsetInTree(&mut nodeTree, scrollId, Vector2::create(0.0, -80.0)));
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::create(0.0, -80.0));
	}

	//==============================================================================
	// 휠은 content 의 자식에서 스크롤 뷰까지 올라와 받아 쓰이고, 경계에서 멈춘다.
	//==============================================================================
	#[test]
	fn wheelScrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, scrollId) = createScrollTree();
		let contentId = getScrollView(&mut nodeTree, scrollId).getContent().expect("content");
		let itemId = nodeTree.createChild(contentId, "item");
		let isHandled = nodeTree.dispatchWheel(itemId, &engine, Vector2::create(0.0, 120.0));
		assert!(isHandled);
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getScrollOffset(), Vector2::create(0.0, -120.0));
		assert_eq!(nodeTree[contentId].getLocalPosition(), Vector2::create(0.0, -120.0));
		nodeTree.dispatchWheel(itemId, &engine, Vector2::create(0.0, 1000.0));
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getScrollOffset(), Vector2::create(0.0, -300.0));
		nodeTree.dispatchWheel(itemId, &engine, Vector2::create(0.0, -1000.0));
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getScrollOffset(), Vector2::zero());
	}

	//==============================================================================
	// 누르고 끌면 tick 에서 끈 만큼 움직인다.
	//==============================================================================
	#[test]
	fn dragScrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, scrollId) = createScrollTree();
		nodeTree.dispatchTouch(scrollId, &engine, TouchKind::press, Vector2::create(150.0, 180.0));
		nodeTree.dispatchTouch(scrollId, &engine, TouchKind::moved, Vector2::create(150.0, 140.0));
		nodeTree.tick(&engine, 0.016);
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getScrollOffset(), Vector2::create(0.0, -40.0));
		assert!(getScrollView(&mut nodeTree, scrollId).isDragging());
		nodeTree.dispatchTouch(scrollId, &engine, TouchKind::release, Vector2::create(150.0, 140.0));
		assert!(!getScrollView(&mut nodeTree, scrollId).isDragging());
	}

	//==============================================================================
	// 탄성 모드: 끌어서 경계를 넘으면 절반만 따라오고, 놓으면 경계로 돌아온다.
	//==============================================================================
	#[test]
	fn elasticSpringsBack() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, scrollId) = createScrollTree();
		getScrollView(&mut nodeTree, scrollId).setScrollMode(ScrollMode::elastic);
		nodeTree.dispatchTouch(scrollId, &engine, TouchKind::press, Vector2::create(150.0, 120.0));
		nodeTree.dispatchTouch(scrollId, &engine, TouchKind::moved, Vector2::create(150.0, 180.0));
		nodeTree.tick(&engine, 0.016);
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getScrollOffset(), Vector2::create(0.0, 30.0));
		nodeTree.dispatchTouch(scrollId, &engine, TouchKind::release, Vector2::create(150.0, 180.0));
		for _ in 0..300 {
			nodeTree.tick(&engine, 0.016);
		}
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getScrollOffset(), Vector2::zero());
	}

	//==============================================================================
	// 세로 스크롤바: 첫 tick 에 호스트의 자식으로 만들어져 오른쪽에 놓인다.
	//==============================================================================
	#[test]
	fn verticalScrollBarIsCreatedAndLaidOut() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, scrollId) = createScrollTree();
		getScrollView(&mut nodeTree, scrollId).setShowsVerticalScrollBar(true);
		assert!(getScrollView(&mut nodeTree, scrollId).getVerticalScrollBar().is_none());
		nodeTree.tick(&engine, 0.016);
		let scrollBarId = getScrollView(&mut nodeTree, scrollId).getVerticalScrollBar().expect("scroll bar");
		assert_eq!(nodeTree.getParentId(scrollBarId), Some(scrollId));
		assert_eq!(nodeTree[scrollBarId].getLocalPosition(), Vector2::create(200.0 - 12.0 - 6.0, 6.0));
		assert_eq!(nodeTree[scrollBarId].getContentSize(), Vector2::create(12.0, 88.0));
		assert!(nodeTree[scrollBarId].isActive());
		assert_eq!(getScrollView(&mut nodeTree, scrollId).getInnerContentSize(), Vector2::create(176.0, 100.0));

		getScrollView(&mut nodeTree, scrollId).setShowsVerticalScrollBar(false);
		nodeTree.tick(&engine, 0.016);
		assert!(!nodeTree[scrollBarId].isActive());
	}
}
