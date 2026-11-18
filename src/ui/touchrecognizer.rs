//==============================================================================
// 터치 인식기. (원본 ui/touchrecognizer.js 의 TouchRecognizer)
// - 누른 노드의 부모 쪽에 UIScrollView(또는 UISnapScrollView)가 있으면, 끈 거리가 임계(10px)를 넘을 때 누른 노드의 누름을 취소하고
//   스크롤 뷰에 처음 누른 자리부터 누름 → 이동을 다시 보내 스크롤을 넘긴다. (스크롤 뷰 안의 버튼을 끌어도 스크롤된다)
// - 부모에 스크롤 뷰가 있으면 누른 노드에 가는 누름을 150ms 미룬다. 그 사이 드래그가 되면 누름 자체를 보내지 않는다.
//   짧게 톡 친 것은 뗄 때 미룬 누름을 바로 보낸 뒤 뗌을 잇는다. (UIKit 의 delaysContentTouches)
// - 스크롤 뷰가 setPassesCrossAxisDrag(true) 면 스크롤하지 않는 축으로 더 많이 끈 손가락은 넘기지 않는다.
// - 원본과 다른 것:
//   - 원본은 GameScene 이 늘 쓴다. 러스트 판의 SceneBase::tickTouch 는 원본 TouchRaycaster(맨 위 노드에 그대로 보냄)다.
//     이 손짓이 필요한 씬은 TouchRecognizer 를 들고, SceneBase::tick 대신 노드 갱신과 이것을 부른다:
//       self.sceneBase.getNodeTreeMut().tick(engine, timeDelta);
//       self.touchRecognizer.tickTouch(self.sceneBase.getNodeTreeMut(), engine);
//     그때 SceneBase 의 첫 터치 콜백과 getTouchTarget 은 쓰지 않는다. (이쪽 getTouchTarget 을 쓴다)
//   - 원본 setTimeout(150ms) 은 프레임 간격(배율을 곱하지 않은 시간)을 쌓아 센다.
//   - 스크롤 뷰에 넘기는 터치는 그 노드의 컴포넌트 모두에게 간다. (NodeTree::dispatchTouch)
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::engine::Engine;
use crate::core::node::nodetree::{NodeId, NodeTree, TouchKind};
use crate::ui::uiscrollbar::UIScrollBar;
use crate::ui::uisnapscrollview::findScrollView;

// 드래그로 바뀌었다고 볼 거리. (px)
const DRAG_THRESHOLD: f32 = 10.0;

// 부모에 스크롤 뷰가 있을 때 누른 노드에 가는 누름을 미루는 시간. (초, 원본 150ms)
const DELAY_PRESS_SECONDS: f32 = 0.15;

//==============================================================================
// 터치 인식기.
//==============================================================================
pub struct TouchRecognizer {
	targetId: Option<NodeId>,
	scrollViewNodeId: Option<NodeId>,
	pressPosition: Option<Vector2>,
	isDragging: bool,
	pressDelayRemainSeconds: Option<f32>,
	isPressDelivered: bool,
	isTouching: bool,
}

impl TouchRecognizer {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TouchRecognizer {
		TouchRecognizer {
			targetId: None,
			scrollViewNodeId: None,
			pressPosition: None,
			isDragging: false,
			pressDelayRemainSeconds: None,
			isPressDelivered: false,
			isTouching: false,
		}
	}

	//==============================================================================
	// 지금 누름을 받는 노드. (드래그를 넘겼으면 스크롤 뷰 노드)
	//==============================================================================
	pub fn getTouchTarget(&self) -> Option<NodeId> {
		if self.isDragging {
			return self.scrollViewNodeId;
		}
		self.targetId
	}

	//==============================================================================
	// 터치 갱신. (SceneBase::tickTouch 자리. 휠 → 미룬 누름 → 누름 / 이동 / 뗌 / 취소)
	//==============================================================================
	pub fn tickTouch(&mut self, nodeTree: &mut NodeTree, engine: &Engine) {
		let elapsedSeconds = engine.getTimeManager().getUnscaleDeltaTime();
		self.updatePressDelay(nodeTree, engine, elapsedSeconds);

		let inputManager = engine.getInputManager();
		let viewInputPosition = inputManager.getViewInputPosition();

		// 휠. (누름과 상관없이 그 자리 아래 노드부터 부모 쪽으로)
		let wheelDeltaX = inputManager.getWheelDeltaX();
		let wheelDeltaY = inputManager.getWheelDeltaY();
		let wheelDelta = Vector2::create(wheelDeltaX, wheelDeltaY);
		if wheelDelta.x != 0.0 || wheelDelta.y != 0.0 {
			if let Some(hitId) = nodeTree.findTopmostInteractableNode(viewInputPosition) {
				nodeTree.dispatchWheel(hitId, engine, wheelDelta);
			}
		}

		if inputManager.isTouchPressed() {
			self.touchPress(nodeTree, engine, viewInputPosition);
		}
		if !self.isTouching {
			return;
		}
		if inputManager.isTouchMoved() {
			self.touchMove(nodeTree, engine, viewInputPosition);
		}
		if inputManager.isTouchReleased() {
			self.touchRelease(nodeTree, engine, viewInputPosition);
		}
		else if inputManager.isTouchCancelled() {
			self.touchCancel(nodeTree, engine, viewInputPosition);
		}
	}

	//==============================================================================
	// 미룬 누름의 남은 시간을 줄이고, 다 되면 누른 노드에 누름을 보낸다. (원본 setTimeout 자리)
	//==============================================================================
	pub fn updatePressDelay(&mut self, nodeTree: &mut NodeTree, engine: &Engine, elapsedSeconds: f32) {
		let remainSeconds = match self.pressDelayRemainSeconds {
			Some(remainSeconds) => {
				remainSeconds - elapsedSeconds
			}
			None => {
				return;
			}
		};
		if remainSeconds > 0.0 {
			self.pressDelayRemainSeconds = Some(remainSeconds);
			return;
		}
		self.pressDelayRemainSeconds = None;
		if self.isDragging {
			return;
		}
		if let (Some(targetId), Some(pressPosition)) = (self.targetId, self.pressPosition) {
			self.isPressDelivered = true;
			nodeTree.dispatchTouch(targetId, engine, TouchKind::press, pressPosition);
		}
	}

	//==============================================================================
	// 터치 누름. (맞은 노드와 그 부모 쪽 스크롤 뷰를 찾아 두고, 스크롤 뷰가 있으면 누름을 미룬다)
	//==============================================================================
	pub fn touchPress(&mut self, nodeTree: &mut NodeTree, engine: &Engine, viewInputPosition: Vector2) {
		let targetId = nodeTree.findTopmostInteractableNode(viewInputPosition);
		self.targetId = targetId;
		self.pressPosition = Some(viewInputPosition);
		self.isDragging = false;
		self.scrollViewNodeId = self.findAncestorScrollView(nodeTree, targetId);
		self.isPressDelivered = false;
		self.pressDelayRemainSeconds = None;
		self.isTouching = true;

		let targetId = match targetId {
			Some(targetId) => {
				targetId
			}
			None => {
				return;
			}
		};
		if self.scrollViewNodeId.is_some() {
			self.pressDelayRemainSeconds = Some(DELAY_PRESS_SECONDS);
		}
		else {
			self.isPressDelivered = true;
			nodeTree.dispatchTouch(targetId, engine, TouchKind::press, viewInputPosition);
		}
	}

	//==============================================================================
	// 터치 이동.
	// - 드래그를 넘겼으면 스크롤 뷰에만 보낸다.
	// - 스크롤 뷰가 있고 임계를 넘으면 넘긴다: 미룬 누름은 버리고(이미 보냈으면 누른 노드에 취소), 스크롤 뷰에 누름(처음 자리) → 이동.
	// - 그 밖에는 누른 노드에 이동. (누름을 아직 보내지 않았으면 이동도 보내지 않는다)
	//==============================================================================
	pub fn touchMove(&mut self, nodeTree: &mut NodeTree, engine: &Engine, viewInputPosition: Vector2) {
		if self.isDragging {
			if let Some(scrollViewNodeId) = self.scrollViewNodeId {
				nodeTree.dispatchTouch(scrollViewNodeId, engine, TouchKind::moved, viewInputPosition);
			}
			return;
		}

		if let (Some(scrollViewNodeId), Some(pressPosition)) = (self.scrollViewNodeId, self.pressPosition) {
			let deltaX = viewInputPosition.x - pressPosition.x;
			let deltaY = viewInputPosition.y - pressPosition.y;
			let isOverThreshold = deltaX.abs() > DRAG_THRESHOLD || deltaY.abs() > DRAG_THRESHOLD;
			let isScrollDrag = match nodeTree.getNode(scrollViewNodeId).and_then(findScrollView) {
				Some(scrollView) => {
					scrollView.isScrollDrag(deltaX, deltaY)
				}
				None => {
					true
				}
			};
			if isOverThreshold && !isScrollDrag {
				// 스크롤하지 않는 축의 드래그. 이 손가락은 끝까지 누른 노드의 것이다.
				self.scrollViewNodeId = None;
				if self.pressDelayRemainSeconds.is_some() {
					self.pressDelayRemainSeconds = None;
					if let Some(targetId) = self.targetId {
						self.isPressDelivered = true;
						nodeTree.dispatchTouch(targetId, engine, TouchKind::press, pressPosition);
					}
				}
			}
			else if isOverThreshold {
				self.isDragging = true;
				// 미룬 누름이 있으면 버린다. 누름 자체를 보내지 않았으니 취소도 필요 없다.
				if self.pressDelayRemainSeconds.is_some() {
					self.pressDelayRemainSeconds = None;
				}
				else if self.isPressDelivered {
					if let Some(targetId) = self.targetId {
						nodeTree.dispatchTouch(targetId, engine, TouchKind::cancel, viewInputPosition);
					}
				}
				// 누른 노드는 더 이상 따라가지 않는다.
				self.targetId = None;
				self.isPressDelivered = false;
				// 스크롤 뷰가 자연스러운 시작 자리를 갖도록 처음 누른 자리부터 다시 보낸다.
				nodeTree.dispatchTouch(scrollViewNodeId, engine, TouchKind::press, pressPosition);
				nodeTree.dispatchTouch(scrollViewNodeId, engine, TouchKind::moved, viewInputPosition);
				return;
			}
		}

		if self.isPressDelivered {
			if let Some(targetId) = self.targetId {
				nodeTree.dispatchTouch(targetId, engine, TouchKind::moved, viewInputPosition);
			}
		}
	}

	//==============================================================================
	// 터치 뗌. (드래그면 스크롤 뷰에, 아니면 누른 노드에. 미룬 누름이 남았으면 그것을 먼저 보낸다)
	//==============================================================================
	pub fn touchRelease(&mut self, nodeTree: &mut NodeTree, engine: &Engine, viewInputPosition: Vector2) {
		if self.isDragging {
			if let Some(scrollViewNodeId) = self.scrollViewNodeId {
				nodeTree.dispatchTouch(scrollViewNodeId, engine, TouchKind::release, viewInputPosition);
			}
		}
		else if let Some(targetId) = self.targetId {
			if self.pressDelayRemainSeconds.is_some() {
				self.pressDelayRemainSeconds = None;
				let pressPosition = self.pressPosition.unwrap_or(viewInputPosition);
				nodeTree.dispatchTouch(targetId, engine, TouchKind::press, pressPosition);
				self.isPressDelivered = true;
			}
			nodeTree.dispatchTouch(targetId, engine, TouchKind::release, viewInputPosition);
		}
		self.reset();
	}

	//==============================================================================
	// 터치 취소. (미룬 누름이 남았으면 누른 노드에는 취소를 보내지 않는다)
	//==============================================================================
	pub fn touchCancel(&mut self, nodeTree: &mut NodeTree, engine: &Engine, viewInputPosition: Vector2) {
		if self.isDragging {
			if let Some(scrollViewNodeId) = self.scrollViewNodeId {
				nodeTree.dispatchTouch(scrollViewNodeId, engine, TouchKind::cancel, viewInputPosition);
			}
		}
		else if let Some(targetId) = self.targetId {
			if self.isPressDelivered {
				nodeTree.dispatchTouch(targetId, engine, TouchKind::cancel, viewInputPosition);
			}
		}
		self.reset();
	}

	//==============================================================================
	// 추적 상태 비우기.
	//==============================================================================
	pub fn reset(&mut self) {
		self.pressDelayRemainSeconds = None;
		self.targetId = None;
		self.scrollViewNodeId = None;
		self.pressPosition = None;
		self.isDragging = false;
		self.isPressDelivered = false;
		self.isTouching = false;
	}

	//==============================================================================
	// 노드의 부모 쪽에서 UIScrollView(또는 UISnapScrollView)가 붙은 가장 가까운 노드. (없으면 None)
	// - 누른 노드가 스크롤바면 자기가 다루는 스크롤 뷰는 빼고 찾는다. (스크롤바 드래그가 그 스크롤 뷰로 넘어가면 안 된다)
	//==============================================================================
	pub fn findAncestorScrollView(&self, nodeTree: &NodeTree, targetId: Option<NodeId>) -> Option<NodeId> {
		let targetId = targetId?;
		let ownedScrollViewNodeId = nodeTree.getNode(targetId).and_then(|targetNode| targetNode.getComponent::<UIScrollBar>()).and_then(|scrollBar| scrollBar.getScrollView());
		let mut currentId = nodeTree.getParentId(targetId);
		while let Some(candidateId) = currentId {
			let hasScrollView = nodeTree.getNode(candidateId).map(|candidateNode| findScrollView(candidateNode).is_some()).unwrap_or(false);
			if hasScrollView && Some(candidateId) != ownedScrollViewNodeId {
				return Some(candidateId);
			}
			currentId = nodeTree.getParentId(candidateId);
		}
		None
	}
}

impl Default for TouchRecognizer {
	fn default() -> TouchRecognizer {
		TouchRecognizer::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::component::paint::Paint;
	use crate::core::node::nodetree::ComponentContext;
	use crate::core::engine::EngineConfiguration;
	use crate::ui::uibutton::UIButton;
	use crate::ui::uiscrollview::UIScrollView;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (스크롤 뷰 (0,0 ~ 200,100), 콘텐트 안에 버튼 (0,0 ~ 200,40))
	//==============================================================================
	fn createScrollWithButton(clickCount: &Rc<Cell<u32>>) -> (NodeTree, NodeId, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let scrollId = nodeTree.createChild(rootId, "scroll");
		let scrollNode = &mut nodeTree[scrollId];
		scrollNode.setPivot(Pivot::topLeft);
		scrollNode.setContentSize(Vector2::create(200.0, 100.0));
		let contentId = {
			let scrollView = nodeTree.addComponent(scrollId, UIScrollView::new()).expect("scroll view");
			scrollView.setScrollContentSize(Vector2::create(200.0, 400.0));
			scrollView.getContent().expect("content")
		};
		let buttonId = nodeTree.createChild(contentId, "button");
		let buttonNode = &mut nodeTree[buttonId];
		buttonNode.setPivot(Pivot::topLeft);
		buttonNode.setContentSize(Vector2::create(200.0, 40.0));
		buttonNode.addComponent(Paint::new());
		let clickCountForEvent = clickCount.clone();
		nodeTree.addComponent(buttonId, UIButton::new()).expect("button").setClickEvent(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
			clickCountForEvent.set(clickCountForEvent.get() + 1);
		}));
		(nodeTree, scrollId, buttonId)
	}

	//==============================================================================
	// 짧게 톡 치면 미룬 누름을 보낸 뒤 뗌을 이어 버튼이 클릭된다.
	//==============================================================================
	#[test]
	fn shortTapClicksButtonInsideScrollView() {
		let engine = Engine::new(EngineConfiguration::new());
		let clickCount = Rc::new(Cell::new(0));
		let (mut nodeTree, scrollId, buttonId) = createScrollWithButton(&clickCount);
		let mut touchRecognizer = TouchRecognizer::new();
		let tapPosition = Vector2::create(50.0, 20.0);
		touchRecognizer.touchPress(&mut nodeTree, &engine, tapPosition);
		assert_eq!(touchRecognizer.findAncestorScrollView(&nodeTree, Some(buttonId)), Some(scrollId));
		assert_eq!(touchRecognizer.getTouchTarget(), Some(buttonId));
		touchRecognizer.touchRelease(&mut nodeTree, &engine, tapPosition);
		assert_eq!(clickCount.get(), 1);
	}

	//==============================================================================
	// 버튼을 누르고 임계를 넘게 끌면 버튼은 클릭되지 않고 스크롤 뷰가 움직인다.
	//==============================================================================
	#[test]
	fn dragOverButtonScrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let clickCount = Rc::new(Cell::new(0));
		let (mut nodeTree, scrollId, _buttonId) = createScrollWithButton(&clickCount);
		let mut touchRecognizer = TouchRecognizer::new();
		touchRecognizer.touchPress(&mut nodeTree, &engine, Vector2::create(50.0, 30.0));
		// 미룬 시간이 지나 누름이 버튼에 간 뒤에 끌어도 취소되고 넘어간다.
		touchRecognizer.updatePressDelay(&mut nodeTree, &engine, 0.2);
		touchRecognizer.touchMove(&mut nodeTree, &engine, Vector2::create(50.0, 0.0));
		assert_eq!(touchRecognizer.getTouchTarget(), Some(scrollId));
		nodeTree.tick(&engine, 0.016);
		let scrollOffset = nodeTree[scrollId].getComponent::<UIScrollView>().expect("scroll view").getScrollOffset();
		assert_eq!(scrollOffset, Vector2::create(0.0, -30.0));
		touchRecognizer.touchRelease(&mut nodeTree, &engine, Vector2::create(50.0, 0.0));
		assert_eq!(clickCount.get(), 0);
	}
}
