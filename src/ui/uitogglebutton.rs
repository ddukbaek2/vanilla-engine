//==============================================================================
// 토글 버튼. (원본 ui/uitogglebutton.js 의 UIToggleButton)
// - 클릭할 때마다 켜짐 / 꺼짐이 바뀐다. 켜져 있는 동안은 눌림 틴트를 끝까지 건 채로 둔다.
// - 원본은 UIButton 을 상속하고 자기 클릭 처리를 super.setClickedEvent 에 건다. 러스트 판은 UIButton 을 품고,
//   품은 버튼이 클릭을 알려 주면(뗌 처리의 반환값) 켜짐을 뒤집는다.
//   그래서 순서가 조금 다르다: 원본은 뗌 → [뒤집기, 토글 이벤트, 클릭됨 이벤트] → 클릭 이벤트, 러스트 판은 뗌 → 클릭 이벤트 → [뒤집기, 토글 이벤트, 클릭됨 이벤트].
// - 토글 / 클릭됨 이벤트는 토글 버튼과 문맥을 받는다. 그 밖의 버튼 이벤트와 설정은 getUIButtonMut 로 품은 버튼에 한다.
// - setIsOn 의 틴트는 다음 tick 에 걸린다. (UIButton.setInteractable 과 같은 까닭)
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uibutton::UIButton;

//==============================================================================
// 토글 버튼 이벤트. (토글 버튼과 문맥을 받는다)
//==============================================================================
pub type UIToggleButtonEvent = Box<dyn FnMut(&mut UIToggleButton, &mut ComponentContext)>;

//==============================================================================
// 토글 버튼.
//==============================================================================
pub struct UIToggleButton {
	uiButton: UIButton,
	isOn: bool,
	onTintColor: Color,
	toggledEvent: Option<UIToggleButtonEvent>,
	externalClickedEvent: Option<UIToggleButtonEvent>,
}

impl UIToggleButton {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIToggleButton {
		UIToggleButton {
			uiButton: UIButton::new(),
			isOn: false,
			onTintColor: Color::create(0.0, 0.0, 0.0, 0.3),
			toggledEvent: None,
			externalClickedEvent: None,
		}
	}

	//==============================================================================
	// 품은 버튼 반환.
	//==============================================================================
	pub fn getUIButton(&self) -> &UIButton {
		&self.uiButton
	}

	//==============================================================================
	// 품은 버튼 반환. (변경용)
	//==============================================================================
	pub fn getUIButtonMut(&mut self) -> &mut UIButton {
		&mut self.uiButton
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.uiButton.getContent()
	}

	//==============================================================================
	// 틴트 트랜지션 갱신. (켜져 있으면 끝까지 건 채로 둔다)
	//==============================================================================
	fn updateTintTransition(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		if self.getIsOn() {
			self.uiButton.applyTintProgress(context, 1.0);
			return;
		}
		self.uiButton.updateTintTransition(context, timeDelta);
	}

	//==============================================================================
	// 클릭 이벤트 설정. (원본처럼 품은 버튼이 아니라 토글 버튼이 따로 든다)
	//==============================================================================
	pub fn setClickedEvent(&mut self, clickedEvent: UIToggleButtonEvent) {
		self.externalClickedEvent = Some(clickedEvent);
	}

	//==============================================================================
	// 클릭 처리. (뒤집고 틴트를 건 뒤 토글 이벤트, 클릭됨 이벤트)
	//==============================================================================
	fn onClicked(&mut self, context: &mut ComponentContext) {
		self.isOn = !self.getIsOn();
		self.uiButton.collectColorTargets(context);
		let tintProgress = if self.getIsOn() {
			1.0
		}
		else {
			0.0
		};
		self.uiButton.applyTintProgress(context, tintProgress);
		if let Some(mut toggledEvent) = self.toggledEvent.take() {
			toggledEvent(self, context);
			if self.toggledEvent.is_none() {
				self.toggledEvent = Some(toggledEvent);
			}
		}
		if let Some(mut externalClickedEvent) = self.externalClickedEvent.take() {
			externalClickedEvent(self, context);
			if self.externalClickedEvent.is_none() {
				self.externalClickedEvent = Some(externalClickedEvent);
			}
		}
	}

	//==============================================================================
	// 켜짐 설정. (이벤트는 부르지 않는다. 틴트는 다음 tick 에 걸린다)
	//==============================================================================
	pub fn setIsOn(&mut self, isOn: bool) {
		if self.getIsOn() == isOn {
			return;
		}
		self.isOn = isOn;
		self.uiButton.requestColorTargetCollect();
	}

	//==============================================================================
	// 켜짐 여부.
	//==============================================================================
	pub fn getIsOn(&self) -> bool {
		self.isOn
	}

	//==============================================================================
	// 켜짐 틴트 색 설정. (품은 버튼의 눌림 틴트 색도 같이 바꾼다)
	//==============================================================================
	pub fn setOnTintColor(&mut self, color: impl Into<Color>) {
		let onTintColor: Color = color.into();
		self.onTintColor = onTintColor;
		self.uiButton.setPressedTintColor(onTintColor);
	}

	//==============================================================================
	// 켜짐 틴트 색 반환.
	//==============================================================================
	pub fn getOnTintColor(&self) -> Color {
		self.onTintColor
	}

	//==============================================================================
	// 토글 이벤트 설정.
	//==============================================================================
	pub fn setToggledEvent(&mut self, toggledEvent: UIToggleButtonEvent) {
		self.toggledEvent = Some(toggledEvent);
	}
}

impl Default for UIToggleButton {
	fn default() -> UIToggleButton {
		UIToggleButton::new()
	}
}

impl Component for UIToggleButton {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiButton.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiButton.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"ToggleButton"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiButton.attachToTree(nodeTree, nodeId);
	}

	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		let isClicked = self.uiButton.tickPress(context, timeDelta);
		if isClicked {
			self.onClicked(context);
		}
		self.uiButton.updateHoverState(context);
		self.updateTintTransition(context, timeDelta);
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn draw(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	fn touchPress(&mut self, context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.uiButton.handleTouchPress(context);
	}

	fn touchRelease(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		let isClicked = self.uiButton.handleTouchRelease(context, viewInputPosition);
		if isClicked {
			self.onClicked(context);
		}
	}

	fn touchCancel(&mut self, context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.uiButton.handleTouchCancel(context);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::component::paint::Paint;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (가운데 (100, 100), 크기 60 x 60 인 토글 버튼. 바탕은 흰 Paint)
	//==============================================================================
	fn createToggleTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let toggleId = nodeTree.createChild(rootId, "toggle");
		let toggleNode = &mut nodeTree[toggleId];
		toggleNode.setLocalPosition(Vector2::create(100.0, 100.0));
		toggleNode.setContentSize(Vector2::create(60.0, 60.0));
		toggleNode.addComponent(Paint::createWithColor(Color::white()));
		nodeTree.addComponent(toggleId, UIToggleButton::new());
		(nodeTree, toggleId)
	}

	//==============================================================================
	// 바탕 색 반환.
	//==============================================================================
	fn getPaintColor(nodeTree: &NodeTree, toggleId: NodeId) -> Color {
		nodeTree[toggleId].getComponent::<Paint>().expect("paint").getColor()
	}

	//==============================================================================
	// 클릭마다 켜짐이 뒤집히고 토글 이벤트가 난다. 켜져 있는 동안 바탕은 눌림 틴트를 끝까지 건 색이다.
	//==============================================================================
	#[test]
	fn clickTogglesAndHoldsTint() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, toggleId) = createToggleTree();
		let toggledCount = Rc::new(Cell::new(0));
		let lastIsOn = Rc::new(Cell::new(false));
		{
			let toggleButton = nodeTree[toggleId].getComponentMut::<UIToggleButton>().expect("toggle");
			let toggledCountForEvent = toggledCount.clone();
			let lastIsOnForEvent = lastIsOn.clone();
			toggleButton.setToggledEvent(Box::new(move |toggleButton: &mut UIToggleButton, _context: &mut ComponentContext| {
				toggledCountForEvent.set(toggledCountForEvent.get() + 1);
				lastIsOnForEvent.set(toggleButton.getIsOn());
			}));
		}
		let inside = Vector2::create(100.0, 100.0);
		nodeTree.dispatchTouch(toggleId, &engine, TouchKind::press, inside);
		nodeTree.dispatchTouch(toggleId, &engine, TouchKind::release, inside);
		assert_eq!(toggledCount.get(), 1);
		assert!(lastIsOn.get());
		nodeTree.tick(&engine, 1.0);
		let onColor = getPaintColor(&nodeTree, toggleId);
		assert!((onColor.red - 0.7).abs() < 0.001);

		nodeTree.dispatchTouch(toggleId, &engine, TouchKind::press, inside);
		nodeTree.dispatchTouch(toggleId, &engine, TouchKind::release, inside);
		assert_eq!(toggledCount.get(), 2);
		assert!(!lastIsOn.get());
		// 원본 버릇: 켜진 채로 누르면 틴트가 걸린 색을 원래 색으로 다시 모아, 끈 뒤에도 그 색(0.7)에 남는다.
		nodeTree.tick(&engine, 1.0);
		let offColor = getPaintColor(&nodeTree, toggleId);
		assert!((offColor.red - 0.7).abs() < 0.001);
	}

	//==============================================================================
	// setIsOn 은 이벤트 없이 바꾸고 다음 tick 에 틴트를 건다.
	//==============================================================================
	#[test]
	fn setIsOnAppliesTintNextTick() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, toggleId) = createToggleTree();
		let toggledCount = Rc::new(Cell::new(0));
		{
			let toggleButton = nodeTree[toggleId].getComponentMut::<UIToggleButton>().expect("toggle");
			let toggledCountForEvent = toggledCount.clone();
			toggleButton.setToggledEvent(Box::new(move |_toggleButton: &mut UIToggleButton, _context: &mut ComponentContext| {
				toggledCountForEvent.set(toggledCountForEvent.get() + 1);
			}));
			toggleButton.setIsOn(true);
			assert!(toggleButton.getIsOn());
		}
		assert_eq!(getPaintColor(&nodeTree, toggleId), Color::white());
		nodeTree.tick(&engine, 0.016);
		let onColor = getPaintColor(&nodeTree, toggleId);
		assert!((onColor.red - 0.7).abs() < 0.001);
		assert_eq!(toggledCount.get(), 0);
	}
}
