//==============================================================================
// 버튼. (원본 ui/uibutton.js 의 UIButton, ButtonState)
// - 누름 / 뗌 / 취소 / 클릭 / 길게 누름 / 마우스 오버 이벤트와, 누르거나 올렸을 때 색이 변하는 틴트 트랜지션.
// - 클릭은 눌렀다가 버튼 영역 안에서 뗄 때만 일어난다. (길게 누름이 발화한 누름은 클릭을 삼킨다)
// - 틴트는 원본처럼 호스트 노드와 모든 자손의 Sprite / UIImageView(덮는 색), Paint / Text / UILabel(원래 색 보간)에 건다.
//   호스트 노드의 다른 컴포넌트는 tick / 터치 중에 형제로 찾는다. (NodeTree 가 부르는 컴포넌트만 잠깐 꺼낸다)
//   원본은 Sprite 가 Paint 를 상속해 getComponents(Paint) 에도 잡힌다. 그래서 스프라이트에는 덮는 색 다음에 원래 색 보간이 한 번 더 걸려
//   뒤의 것이 이긴다. 같은 결과를 내려고 러스트 판도 스프라이트(와 UIImageView 의 스프라이트)를 두 번 담는다.
// - 원본과 다른 것:
//   - 이벤트는 Box<dyn FnMut(&mut UIButton, &mut ComponentContext)> 다. 원본 콜백은 버튼만 받는다.
//     문맥을 함께 넘겨 콜백에서 다른 노드를 바꾸거나 만들고 지울 수 있게 했다. (클릭 이벤트의 bool 반환은 원본에서도 쓰지 않아 뺐다)
//   - 원본 setInteractable / UIToggleButton.setIsOn 은 그 자리에서 색 대상을 모으고 틴트를 건다. 러스트 판은 노드에 닿을 수 없는 자리라
//     다음 tick 에 모으고 건다. (tick 이 매 프레임 틴트를 다시 거므로 보이는 결과는 같다)
//   - 제외 컴포넌트는 인스턴스 대신 (노드, 컴포넌트 타입) 으로 정한다: excludeComponentFromTint::<Paint>(nodeId).
//   - setStateEvent(원본도 빈 함수)와 이벤트 getter 는 옮기지 않았다.
// - attachToTree 에서 호스트 노드를 interactable 로 켠다. nodeTree.addComponent(nodeId, UIButton::new()) 로 붙인다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::sprite::Sprite;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uicontrol::UIControl;
use crate::ui::uiimageview::UIImageView;
use crate::ui::uilabel::UILabel;
use crate::ui::uiview::isViewPositionInsideNode;
use std::any::{Any, TypeId};

//==============================================================================
// 버튼 상태.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
	normal,
	hover,
	pressed,
	released,
	disabled,
}

//==============================================================================
// 버튼 이벤트. (버튼과 문맥을 받는다)
//==============================================================================
pub type UIButtonEvent = Box<dyn FnMut(&mut UIButton, &mut ComponentContext)>;

//==============================================================================
// 이벤트 종류. (이벤트 칸을 고를 때)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ButtonEventKind {
	click,
	pressed,
	released,
	clicked,
	longPressed,
	hover,
}

//==============================================================================
// 색 대상 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorEntryKind {
	// Sprite 에 덮는 색. (원본 "sprite")
	spriteOverlay,
	// UIImageView 에 덮는 색. (원본 "imageview")
	imageViewOverlay,
	// Paint 원래 색 보간. (원본 "paint")
	paint,
	// Sprite 원래 색 보간. (원본에서 Sprite 가 getComponents(Paint) 에 잡혀 생기는 "paint")
	spritePaint,
	// UIImageView 의 스프라이트 원래 색 보간. (원본에서 UIImageView 의 Sprite 가 getComponents(Paint) 에 잡혀 생기는 "paint")
	imageViewPaint,
	// Text 글자 색 보간. (원본 "text")
	text,
	// UILabel 글자 색 보간. (원본 "uilabel")
	label,
}

//==============================================================================
// 색 대상. (노드, 그 노드 컴포넌트 목록의 자리, 모을 때의 원래 색)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ColorEntry {
	nodeId: NodeId,
	componentIndex: usize,
	kind: ColorEntryKind,
	originalColor: Color,
}

//==============================================================================
// 최소 눌림 시간을 채우려고 미뤄 둔 뗌.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct PendingRelease {
	viewInputPosition: Vector2,
	remainSeconds: f32,
}

//==============================================================================
// 버튼.
//==============================================================================
pub struct UIButton {
	uiControl: UIControl,
	buttonState: ButtonState,
	clickEvent: Option<UIButtonEvent>,
	pressedEvent: Option<UIButtonEvent>,
	releasedEvent: Option<UIButtonEvent>,
	clickedEvent: Option<UIButtonEvent>,
	longPressedEvent: Option<UIButtonEvent>,
	hoverEvent: Option<UIButtonEvent>,
	isPressTracking: bool,
	minimumPressedSeconds: f32,
	pressedElapsedSeconds: f32,
	pendingRelease: Option<PendingRelease>,
	longPressSeconds: f32,
	hasLongPressFired: bool,
	pressedTintColor: Color,
	hoverTintColor: Color,
	transitionDuration: f32,
	tintProgress: f32,
	colorEntries: Vec<ColorEntry>,
	isInteractable: bool,
	disabledTintColor: Color,
	tintExcludedNodeIds: Vec<NodeId>,
	tintExcludedComponents: Vec<(NodeId, TypeId)>,
	isColorTargetCollectPending: bool,
}

impl UIButton {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIButton {
		UIButton {
			uiControl: UIControl::new(),
			buttonState: ButtonState::normal,
			clickEvent: None,
			pressedEvent: None,
			releasedEvent: None,
			clickedEvent: None,
			longPressedEvent: None,
			hoverEvent: None,
			isPressTracking: false,
			minimumPressedSeconds: 0.0,
			pressedElapsedSeconds: 0.0,
			pendingRelease: None,
			longPressSeconds: 0.5,
			hasLongPressFired: false,
			pressedTintColor: Color::create(0.0, 0.0, 0.0, 0.3),
			hoverTintColor: Color::create(1.0, 1.0, 1.0, 0.12),
			transitionDuration: 0.3,
			tintProgress: 0.0,
			colorEntries: Vec::new(),
			isInteractable: true,
			disabledTintColor: Color::create(0.0, 0.0, 0.0, 0.5),
			tintExcludedNodeIds: Vec::new(),
			tintExcludedComponents: Vec::new(),
			isColorTargetCollectPending: false,
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
	// 트랜지션 대상에서 노드를 제외. (그 노드와 모든 자손의 컴포넌트가 색 대상에서 빠진다)
	//==============================================================================
	pub fn excludeNodeFromTint(&mut self, nodeId: NodeId) {
		if !self.tintExcludedNodeIds.contains(&nodeId) {
			self.tintExcludedNodeIds.push(nodeId);
		}
	}

	//==============================================================================
	// 트랜지션 대상에서 그 노드의 T 타입 컴포넌트만 제외. (원본은 컴포넌트 인스턴스를 받는다)
	//==============================================================================
	pub fn excludeComponentFromTint<T: Component + 'static>(&mut self, nodeId: NodeId) {
		let excludedComponent = (nodeId, TypeId::of::<T>());
		if !self.tintExcludedComponents.contains(&excludedComponent) {
			self.tintExcludedComponents.push(excludedComponent);
		}
	}

	//==============================================================================
	// 제외 목록 비우기.
	//==============================================================================
	pub fn clearTintExclusions(&mut self) {
		self.tintExcludedNodeIds.clear();
		self.tintExcludedComponents.clear();
	}

	//==============================================================================
	// 이벤트 칸 고르기.
	//==============================================================================
	fn getEventSlot(&mut self, eventKind: ButtonEventKind) -> &mut Option<UIButtonEvent> {
		match eventKind {
			ButtonEventKind::click => {
				&mut self.clickEvent
			}
			ButtonEventKind::pressed => {
				&mut self.pressedEvent
			}
			ButtonEventKind::released => {
				&mut self.releasedEvent
			}
			ButtonEventKind::clicked => {
				&mut self.clickedEvent
			}
			ButtonEventKind::longPressed => {
				&mut self.longPressedEvent
			}
			ButtonEventKind::hover => {
				&mut self.hoverEvent
			}
		}
	}

	//==============================================================================
	// 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fireEvent(&mut self, context: &mut ComponentContext, eventKind: ButtonEventKind) {
		let mut event = match self.getEventSlot(eventKind).take() {
			Some(event) => {
				event
			}
			None => {
				return;
			}
		};
		event(self, context);
		let eventSlot = self.getEventSlot(eventKind);
		if eventSlot.is_none() {
			*eventSlot = Some(event);
		}
	}

	//==============================================================================
	// 누름 추적과 미뤄 둔 뗌 갱신. (뗌을 처리해 클릭이 났으면 true)
	// - UIToggleButton 이 클릭을 받아 켜짐을 뒤집으려고 따로 부른다.
	//==============================================================================
	pub(crate) fn tickPress(&mut self, context: &mut ComponentContext, timeDelta: f32) -> bool {
		if self.isColorTargetCollectPending {
			self.isColorTargetCollectPending = false;
			self.collectColorTargets(context);
		}
		if self.isPressTracking {
			self.pressedElapsedSeconds += timeDelta;

			// 길게 누름. (임계 시간을 넘기는 순간 한 번만 알린다)
			if self.longPressedEvent.is_some() && !self.hasLongPressFired && self.pressedElapsedSeconds >= self.longPressSeconds {
				self.hasLongPressFired = true;
				self.fireEvent(context, ButtonEventKind::longPressed);
			}
		}

		// 최소 눌림 표시 시간을 채우려고 미뤄 둔 뗌 처리를 마저 한다.
		if let Some(pendingRelease) = self.pendingRelease.as_mut() {
			pendingRelease.remainSeconds -= timeDelta;
			if pendingRelease.remainSeconds <= 0.0 {
				let releasePosition = pendingRelease.viewInputPosition;
				self.pendingRelease = None;
				return self.performRelease(context, releasePosition);
			}
		}
		false
	}

	//==============================================================================
	// 마우스 오버 상태 갱신. (누르고 있는 중이거나 사용 불가일 때는 건드리지 않는다)
	//==============================================================================
	pub(crate) fn updateHoverState(&mut self, context: &mut ComponentContext) {
		if !self.getInteractable() {
			return;
		}
		let currentButtonState = self.getButtonState();
		if currentButtonState == ButtonState::pressed || currentButtonState == ButtonState::disabled {
			return;
		}
		let viewInputPosition = context.getEngine().getInputManager().getViewInputPosition();
		let nodeId = context.getNodeId();
		let nodeTree = context.getNodeTree();
		let isHovering = isViewPositionInsideNode(nodeTree, nodeId, viewInputPosition);
		if isHovering && currentButtonState != ButtonState::hover {
			self.setButtonState(ButtonState::hover);
			self.collectColorTargets(context);
			self.tintProgress = 0.0;
			self.fireEvent(context, ButtonEventKind::hover);
		}
		else if !isHovering && currentButtonState == ButtonState::hover {
			self.setButtonState(ButtonState::normal);
			self.collectColorTargets(context);
			self.tintProgress = 0.0;
		}
	}

	//==============================================================================
	// 틴트 트랜지션 갱신. (누르거나 올려 두면 차오르고, 아니면 빠진다. 사용 불가면 어두운 틴트)
	//==============================================================================
	pub(crate) fn updateTintTransition(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		if !self.getInteractable() {
			self.applyDisabledTint(context);
			return;
		}
		let buttonState = self.getButtonState();
		let isPressed = buttonState == ButtonState::pressed;
		let isHovering = buttonState == ButtonState::hover;
		let transitionDuration = self.getTransitionDuration();
		if isPressed || isHovering {
			self.tintProgress = (self.tintProgress + timeDelta / transitionDuration).min(1.0);
		}
		else {
			self.tintProgress = (self.tintProgress - timeDelta / transitionDuration).max(0.0);
		}
		let tintProgress = self.tintProgress;
		self.applyTintProgress(context, tintProgress);
	}

	//==============================================================================
	// 뗌 처리 본문. (클릭이 났으면 true)
	//==============================================================================
	pub(crate) fn performRelease(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) -> bool {
		self.setButtonState(ButtonState::released);
		self.tintProgress = 0.0;
		self.applyTintProgress(context, 0.0);
		self.fireEvent(context, ButtonEventKind::released);
		let nodeId = context.getNodeId();
		let nodeTree = context.getNodeTree();
		let isInsideBounds = isViewPositionInsideNode(nodeTree, nodeId, viewInputPosition);
		let isClicked = isInsideBounds && !self.hasLongPressFired;
		if isClicked {
			self.fireEvent(context, ButtonEventKind::clicked);
			self.fireEvent(context, ButtonEventKind::click);
		}
		self.setButtonState(ButtonState::normal);
		isClicked
	}

	//==============================================================================
	// 터치 누름 처리.
	//==============================================================================
	pub(crate) fn handleTouchPress(&mut self, context: &mut ComponentContext) {
		if !self.getInteractable() {
			return;
		}
		self.isPressTracking = true;
		self.pressedElapsedSeconds = 0.0;
		self.pendingRelease = None;
		self.hasLongPressFired = false;
		self.setButtonState(ButtonState::pressed);
		self.collectColorTargets(context);
		self.fireEvent(context, ButtonEventKind::pressed);
	}

	//==============================================================================
	// 터치 뗌 처리. (바로 클릭이 났으면 true. 최소 눌림 시간이 남았으면 미루고 false)
	//==============================================================================
	pub(crate) fn handleTouchRelease(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) -> bool {
		if !self.getInteractable() {
			return false;
		}
		if !self.isPressTracking {
			return false;
		}
		self.isPressTracking = false;

		// 최소 눌림 표시 시간을 아직 못 채웠으면, 그만큼 눌린 모습을 유지한 뒤 처리한다.
		if self.minimumPressedSeconds > 0.0 && self.pressedElapsedSeconds < self.minimumPressedSeconds {
			self.pendingRelease = Some(PendingRelease {
				viewInputPosition,
				remainSeconds: self.minimumPressedSeconds - self.pressedElapsedSeconds,
			});
			return false;
		}
		self.performRelease(context, viewInputPosition)
	}

	//==============================================================================
	// 터치 취소 처리.
	//==============================================================================
	pub(crate) fn handleTouchCancel(&mut self, context: &mut ComponentContext) {
		self.pendingRelease = None;
		if !self.isPressTracking {
			return;
		}
		self.isPressTracking = false;
		self.tintProgress = 0.0;
		self.applyTintProgress(context, 0.0);
		self.setButtonState(ButtonState::normal);
	}

	//==============================================================================
	// 틴트 적용. (덮는 색은 알파만 차오르고, 원래 색은 틴트 색 쪽으로 보간한다)
	//==============================================================================
	pub(crate) fn applyTintProgress(&mut self, context: &mut ComponentContext, progress: f32) {
		let tintColor = if self.getButtonState() == ButtonState::hover {
			self.getHoverTintColor()
		}
		else {
			self.getPressedTintColor()
		};
		let overlayAlpha = math::lerp(0.0, tintColor.alpha, progress);
		let overlayColor = Color::create(tintColor.red, tintColor.green, tintColor.blue, overlayAlpha);
		let nodeTree = context.getNodeTreeMut();
		for colorEntry in self.colorEntries.iter() {
			let entryColor = match colorEntry.kind {
				ColorEntryKind::spriteOverlay | ColorEntryKind::imageViewOverlay => {
					overlayColor
				}
				_ => {
					let originalColor = colorEntry.originalColor;
					let tintedRed = math::lerp(originalColor.red, tintColor.red, tintColor.alpha * progress);
					let tintedGreen = math::lerp(originalColor.green, tintColor.green, tintColor.alpha * progress);
					let tintedBlue = math::lerp(originalColor.blue, tintColor.blue, tintColor.alpha * progress);
					Color::create(tintedRed, tintedGreen, tintedBlue, originalColor.alpha)
				}
			};
			setColorEntryColor(nodeTree, colorEntry, entryColor);
		}
	}

	//==============================================================================
	// 사용 불가 틴트 적용. (덮는 색 대상에만)
	//==============================================================================
	fn applyDisabledTint(&mut self, context: &mut ComponentContext) {
		let disabledTintColor = self.disabledTintColor;
		let nodeTree = context.getNodeTreeMut();
		for colorEntry in self.colorEntries.iter() {
			if colorEntry.kind == ColorEntryKind::spriteOverlay || colorEntry.kind == ColorEntryKind::imageViewOverlay {
				setColorEntryColor(nodeTree, colorEntry, disabledTintColor);
			}
		}
	}

	//==============================================================================
	// 색 대상 모으기. (호스트 노드부터 자손까지)
	//==============================================================================
	pub(crate) fn collectColorTargets(&mut self, context: &mut ComponentContext) {
		self.colorEntries.clear();
		let nodeId = context.getNodeId();
		let nodeTree = context.getNodeTree();
		self.collectFromNode(nodeTree, nodeId);
	}

	//==============================================================================
	// 그 노드의 T 타입 컴포넌트가 제외 목록에 있는지.
	//==============================================================================
	fn isComponentExcluded(&self, nodeId: NodeId, componentTypeId: TypeId) -> bool {
		self.tintExcludedComponents.contains(&(nodeId, componentTypeId))
	}

	//==============================================================================
	// 노드에서 색 대상을 재귀로 모은다. (원본 collectFromNode 와 같은 순서)
	// - excludeNodeFromTint 로 정한 노드는 자신과 자손까지 통째로 건너뛴다.
	//==============================================================================
	fn collectFromNode(&mut self, nodeTree: &NodeTree, nodeId: NodeId) {
		if self.tintExcludedNodeIds.contains(&nodeId) {
			return;
		}
		let node = match nodeTree.getNode(nodeId) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		let components = node.getAllComponents();

		// UIImageView 가 있으면 그것을 통해 색을 다루고, 없으면 Sprite 에 덮는 색을 건다.
		let hasImageView = components.iter().any(|component| {
			let componentReference: &dyn Component = component.as_ref();
			componentReference.asAny().is::<UIImageView>()
		});
		for (componentIndex, component) in components.iter().enumerate() {
			let componentReference: &dyn Component = component.as_ref();
			let anyReference: &dyn Any = componentReference.asAny();
			let componentTypeId = anyReference.type_id();
			if self.isComponentExcluded(nodeId, componentTypeId) {
				continue;
			}
			if hasImageView {
				if let Some(imageView) = anyReference.downcast_ref::<UIImageView>() {
					let originalColor = imageView.getColor();
					self.pushColorEntry(nodeId, componentIndex, ColorEntryKind::imageViewOverlay, originalColor);
				}
			}
			else if let Some(sprite) = anyReference.downcast_ref::<Sprite>() {
				let originalColor = sprite.getColor();
				self.pushColorEntry(nodeId, componentIndex, ColorEntryKind::spriteOverlay, originalColor);
			}
		}

		// Paint (단색 배경) 도 원래 색을 들고 보간한다. (원본은 Sprite 와 UIImageView 의 Sprite 도 여기 잡힌다)
		for (componentIndex, component) in components.iter().enumerate() {
			let componentReference: &dyn Component = component.as_ref();
			let anyReference: &dyn Any = componentReference.asAny();
			let componentTypeId = anyReference.type_id();
			if self.isComponentExcluded(nodeId, componentTypeId) {
				continue;
			}
			if let Some(paint) = anyReference.downcast_ref::<Paint>() {
				let originalColor = paint.getColor();
				self.pushColorEntry(nodeId, componentIndex, ColorEntryKind::paint, originalColor);
			}
			else if let Some(sprite) = anyReference.downcast_ref::<Sprite>() {
				let originalColor = sprite.getColor();
				self.pushColorEntry(nodeId, componentIndex, ColorEntryKind::spritePaint, originalColor);
			}
			else if let Some(imageView) = anyReference.downcast_ref::<UIImageView>() {
				let originalColor = imageView.getColor();
				self.pushColorEntry(nodeId, componentIndex, ColorEntryKind::imageViewPaint, originalColor);
			}
		}

		// 글자도 같다. UILabel 이 있으면 그것만, 없으면 Text.
		let hasLabel = components.iter().any(|component| {
			let componentReference: &dyn Component = component.as_ref();
			componentReference.asAny().is::<UILabel>()
		});
		for (componentIndex, component) in components.iter().enumerate() {
			let componentReference: &dyn Component = component.as_ref();
			let anyReference: &dyn Any = componentReference.asAny();
			let componentTypeId = anyReference.type_id();
			if self.isComponentExcluded(nodeId, componentTypeId) {
				continue;
			}
			if hasLabel {
				if let Some(label) = anyReference.downcast_ref::<UILabel>() {
					let originalColor = label.getTextColor();
					self.pushColorEntry(nodeId, componentIndex, ColorEntryKind::label, originalColor);
				}
			}
			else if let Some(text) = anyReference.downcast_ref::<Text>() {
				let originalColor = text.getTextColor();
				self.pushColorEntry(nodeId, componentIndex, ColorEntryKind::text, originalColor);
			}
		}

		for childId in nodeTree.getChildIds(nodeId).iter().copied() {
			self.collectFromNode(nodeTree, childId);
		}
	}

	//==============================================================================
	// 색 대상 하나 담기.
	//==============================================================================
	fn pushColorEntry(&mut self, nodeId: NodeId, componentIndex: usize, kind: ColorEntryKind, originalColor: Color) {
		self.colorEntries.push(ColorEntry {
			nodeId,
			componentIndex,
			kind,
			originalColor,
		});
	}

	//==============================================================================
	// 버튼 상태 설정.
	//==============================================================================
	pub fn setButtonState(&mut self, buttonState: ButtonState) {
		self.buttonState = buttonState;
	}

	//==============================================================================
	// 버튼 상태 반환.
	//==============================================================================
	pub fn getButtonState(&self) -> ButtonState {
		self.buttonState
	}

	//==============================================================================
	// 길게 누름 이벤트 설정. (임계 시간을 넘기면 한 번 알리고, 그 누름의 클릭은 삼킨다)
	//==============================================================================
	pub fn setLongPressedEvent(&mut self, longPressedEvent: UIButtonEvent) {
		self.longPressedEvent = Some(longPressedEvent);
	}

	//==============================================================================
	// 길게 누름 판정 시간 설정. (초)
	//==============================================================================
	pub fn setLongPressSeconds(&mut self, longPressSeconds: f32) {
		self.longPressSeconds = longPressSeconds;
	}

	//==============================================================================
	// 길게 누름 판정 시간 반환.
	//==============================================================================
	pub fn getLongPressSeconds(&self) -> f32 {
		self.longPressSeconds
	}

	//==============================================================================
	// 이번 누름이 이어진 시간 반환. (누르고 있지 않으면 0. 게이지 연출용)
	//==============================================================================
	pub fn getPressedElapsedSeconds(&self) -> f32 {
		if self.isPressTracking {
			self.pressedElapsedSeconds
		}
		else {
			0.0
		}
	}

	//==============================================================================
	// 최소 눌림 표시 시간 설정. (짧게 눌러도 이 시간만큼 눌린 모습을 보인 뒤 클릭한다. 기본 0 = 바로)
	//==============================================================================
	pub fn setMinimumPressedSeconds(&mut self, minimumPressedSeconds: f32) {
		self.minimumPressedSeconds = minimumPressedSeconds.max(0.0);
	}

	//==============================================================================
	// 최소 눌림 표시 시간 반환.
	//==============================================================================
	pub fn getMinimumPressedSeconds(&self) -> f32 {
		self.minimumPressedSeconds
	}

	//==============================================================================
	// 클릭 이벤트 설정. (클릭됨 이벤트 다음에 불린다)
	//==============================================================================
	pub fn setClickEvent(&mut self, clickEvent: UIButtonEvent) {
		self.clickEvent = Some(clickEvent);
	}

	//==============================================================================
	// 누름 이벤트 설정.
	//==============================================================================
	pub fn setPressedEvent(&mut self, pressedEvent: UIButtonEvent) {
		self.pressedEvent = Some(pressedEvent);
	}

	//==============================================================================
	// 뗌 이벤트 설정. (영역 밖에서 떼도 불린다)
	//==============================================================================
	pub fn setReleasedEvent(&mut self, releasedEvent: UIButtonEvent) {
		self.releasedEvent = Some(releasedEvent);
	}

	//==============================================================================
	// 클릭됨 이벤트 설정.
	//==============================================================================
	pub fn setClickedEvent(&mut self, clickedEvent: UIButtonEvent) {
		self.clickedEvent = Some(clickedEvent);
	}

	//==============================================================================
	// 오버 진입 이벤트 설정.
	//==============================================================================
	pub fn setHoverEvent(&mut self, hoverEvent: UIButtonEvent) {
		self.hoverEvent = Some(hoverEvent);
	}

	//==============================================================================
	// 오버 틴트 색 설정.
	//==============================================================================
	pub fn setHoverTintColor(&mut self, color: impl Into<Color>) {
		self.hoverTintColor = color.into();
	}

	//==============================================================================
	// 오버 틴트 색 반환.
	//==============================================================================
	pub fn getHoverTintColor(&self) -> Color {
		self.hoverTintColor
	}

	//==============================================================================
	// 눌림 틴트 색 설정.
	//==============================================================================
	pub fn setPressedTintColor(&mut self, color: impl Into<Color>) {
		self.pressedTintColor = color.into();
	}

	//==============================================================================
	// 눌림 틴트 색 반환.
	//==============================================================================
	pub fn getPressedTintColor(&self) -> Color {
		self.pressedTintColor
	}

	//==============================================================================
	// 트랜지션 시간 설정. (초)
	//==============================================================================
	pub fn setTransitionDuration(&mut self, transitionDuration: f32) {
		self.transitionDuration = transitionDuration;
	}

	//==============================================================================
	// 트랜지션 시간 반환.
	//==============================================================================
	pub fn getTransitionDuration(&self) -> f32 {
		self.transitionDuration
	}

	//==============================================================================
	// 사용 가능 설정. (끄면 다음 tick 에 색 대상을 모아 어두운 틴트를 건다)
	//==============================================================================
	pub fn setInteractable(&mut self, isInteractable: bool) {
		if self.getInteractable() == isInteractable {
			return;
		}
		self.isInteractable = isInteractable;
		if !isInteractable {
			self.isColorTargetCollectPending = true;
			self.setButtonState(ButtonState::disabled);
		}
		else {
			self.tintProgress = 0.0;
			self.setButtonState(ButtonState::normal);
		}
	}

	//==============================================================================
	// 사용 가능 여부.
	//==============================================================================
	pub fn getInteractable(&self) -> bool {
		self.isInteractable
	}

	//==============================================================================
	// 다음 tick 에 색 대상을 다시 모으게 한다. (UIToggleButton.setIsOn)
	//==============================================================================
	pub(crate) fn requestColorTargetCollect(&mut self) {
		self.isColorTargetCollectPending = true;
	}
}

impl Default for UIButton {
	fn default() -> UIButton {
		UIButton::new()
	}
}

//==============================================================================
// 색 대상 하나에 색 넣기. (자리의 컴포넌트가 바뀌었으면 건너뛴다)
//==============================================================================
fn setColorEntryColor(nodeTree: &mut NodeTree, colorEntry: &ColorEntry, color: Color) {
	let node = match nodeTree.getNodeMut(colorEntry.nodeId) {
		Some(node) => {
			node
		}
		None => {
			return;
		}
	};
	let component = match node.components.get_mut(colorEntry.componentIndex) {
		Some(component) => {
			component
		}
		None => {
			return;
		}
	};
	let componentReference: &mut dyn Component = component.as_mut();
	let anyReference: &mut dyn Any = componentReference.asAnyMut();
	match colorEntry.kind {
		ColorEntryKind::spriteOverlay | ColorEntryKind::spritePaint => {
			if let Some(sprite) = anyReference.downcast_mut::<Sprite>() {
				sprite.setColor(color);
			}
		}
		ColorEntryKind::imageViewOverlay | ColorEntryKind::imageViewPaint => {
			if let Some(imageView) = anyReference.downcast_mut::<UIImageView>() {
				imageView.setColor(color);
			}
		}
		ColorEntryKind::paint => {
			if let Some(paint) = anyReference.downcast_mut::<Paint>() {
				paint.setColor(color);
			}
		}
		ColorEntryKind::text => {
			if let Some(text) = anyReference.downcast_mut::<Text>() {
				text.setTextColor(color);
			}
		}
		ColorEntryKind::label => {
			if let Some(label) = anyReference.downcast_mut::<UILabel>() {
				label.setTextColor(color);
			}
		}
	}
}

impl Component for UIButton {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiControl.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiControl.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"Button"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiControl.getUIViewMut().createContentNode(nodeTree, nodeId);
		nodeTree[nodeId].setInteractable(true);
	}

	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		self.tickPress(context, timeDelta);
		self.updateHoverState(context);
		self.updateTintTransition(context, timeDelta);
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn draw(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	fn touchPress(&mut self, context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.handleTouchPress(context);
	}

	fn touchRelease(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		self.handleTouchRelease(context, viewInputPosition);
	}

	fn touchCancel(&mut self, context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.handleTouchCancel(context);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (가운데 (200, 100), 크기 100 x 40 인 버튼. 바탕은 Paint)
	//==============================================================================
	fn createButtonTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let buttonId = nodeTree.createChild(rootId, "button");
		let buttonNode = &mut nodeTree[buttonId];
		buttonNode.setLocalPosition(Vector2::create(200.0, 100.0));
		buttonNode.setContentSize(Vector2::create(100.0, 40.0));
		buttonNode.addComponent(Paint::createWithColor(Color::white()));
		nodeTree.addComponent(buttonId, UIButton::new());
		(nodeTree, buttonId)
	}

	//==============================================================================
	// 셈하는 이벤트.
	//==============================================================================
	fn createCountingEvent(counter: &Rc<Cell<u32>>) -> UIButtonEvent {
		let counter = counter.clone();
		Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
			counter.set(counter.get() + 1);
		})
	}

	//==============================================================================
	// 버튼 반환.
	//==============================================================================
	fn getButton(nodeTree: &mut NodeTree, buttonId: NodeId) -> &mut UIButton {
		nodeTree[buttonId].getComponentMut::<UIButton>().expect("button")
	}

	//==============================================================================
	// 붙이면 노드가 interactable 이 되고 content 가 생긴다.
	//==============================================================================
	#[test]
	fn attachMakesNodeInteractable() {
		let (mut nodeTree, buttonId) = createButtonTree();
		assert!(nodeTree[buttonId].isInteractable());
		let contentId = getButton(&mut nodeTree, buttonId).getContent().expect("content");
		assert_eq!(nodeTree.getParentId(contentId), Some(buttonId));
	}

	//==============================================================================
	// 누름 → 안에서 뗌: 클릭 한 번. 밖에서 뗌: 클릭 없음. 취소: 클릭 없음.
	//==============================================================================
	#[test]
	fn clickFiresOnlyOnReleaseInside() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, buttonId) = createButtonTree();
		let clickCount = Rc::new(Cell::new(0));
		let pressCount = Rc::new(Cell::new(0));
		let releaseCount = Rc::new(Cell::new(0));
		{
			let button = getButton(&mut nodeTree, buttonId);
			button.setClickEvent(createCountingEvent(&clickCount));
			button.setPressedEvent(createCountingEvent(&pressCount));
			button.setReleasedEvent(createCountingEvent(&releaseCount));
		}
		let inside = Vector2::create(210.0, 105.0);
		let outside = Vector2::create(400.0, 105.0);

		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		assert_eq!(getButton(&mut nodeTree, buttonId).getButtonState(), ButtonState::pressed);
		assert_eq!(pressCount.get(), 1);
		assert_eq!(clickCount.get(), 0);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, inside);
		assert_eq!(clickCount.get(), 1);
		assert_eq!(releaseCount.get(), 1);
		assert_eq!(getButton(&mut nodeTree, buttonId).getButtonState(), ButtonState::normal);

		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, outside);
		assert_eq!(clickCount.get(), 1);
		assert_eq!(releaseCount.get(), 2);

		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::cancel, inside);
		assert_eq!(getButton(&mut nodeTree, buttonId).getButtonState(), ButtonState::normal);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, inside);
		assert_eq!(clickCount.get(), 1);

		// 누르지 않은 뗌은 무시한다.
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, inside);
		assert_eq!(clickCount.get(), 1);
		assert_eq!(releaseCount.get(), 2);
	}

	//==============================================================================
	// 사용 불가면 누름을 받지 않는다.
	//==============================================================================
	#[test]
	fn disabledButtonIgnoresTouch() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, buttonId) = createButtonTree();
		let clickCount = Rc::new(Cell::new(0));
		{
			let button = getButton(&mut nodeTree, buttonId);
			button.setClickEvent(createCountingEvent(&clickCount));
			button.setInteractable(false);
			assert_eq!(button.getButtonState(), ButtonState::disabled);
		}
		let inside = Vector2::create(210.0, 105.0);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, inside);
		assert_eq!(clickCount.get(), 0);
	}

	//==============================================================================
	// 길게 누르면 한 번 알리고 그 누름의 클릭은 삼킨다.
	//==============================================================================
	#[test]
	fn longPressSwallowsClick() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, buttonId) = createButtonTree();
		let clickCount = Rc::new(Cell::new(0));
		let longPressCount = Rc::new(Cell::new(0));
		{
			let button = getButton(&mut nodeTree, buttonId);
			button.setClickEvent(createCountingEvent(&clickCount));
			button.setLongPressedEvent(createCountingEvent(&longPressCount));
		}
		let inside = Vector2::create(210.0, 105.0);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		nodeTree.tick(&engine, 0.3);
		assert_eq!(longPressCount.get(), 0);
		assert!((getButton(&mut nodeTree, buttonId).getPressedElapsedSeconds() - 0.3).abs() < 0.0001);
		nodeTree.tick(&engine, 0.3);
		nodeTree.tick(&engine, 0.3);
		assert_eq!(longPressCount.get(), 1);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, inside);
		assert_eq!(clickCount.get(), 0);
		assert_eq!(getButton(&mut nodeTree, buttonId).getPressedElapsedSeconds(), 0.0);
	}

	//==============================================================================
	// 최소 눌림 시간을 채우지 못한 뗌은 미뤘다가 tick 에서 클릭한다.
	//==============================================================================
	#[test]
	fn minimumPressedSecondsDelaysClick() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, buttonId) = createButtonTree();
		let clickCount = Rc::new(Cell::new(0));
		{
			let button = getButton(&mut nodeTree, buttonId);
			button.setClickEvent(createCountingEvent(&clickCount));
			button.setMinimumPressedSeconds(0.2);
		}
		let inside = Vector2::create(210.0, 105.0);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, inside);
		assert_eq!(clickCount.get(), 0);
		assert_eq!(getButton(&mut nodeTree, buttonId).getButtonState(), ButtonState::pressed);
		nodeTree.tick(&engine, 0.1);
		assert_eq!(clickCount.get(), 0);
		nodeTree.tick(&engine, 0.15);
		assert_eq!(clickCount.get(), 1);
		assert_eq!(getButton(&mut nodeTree, buttonId).getButtonState(), ButtonState::normal);
	}

	//==============================================================================
	// 누르고 있으면 형제 Paint 색이 눌림 틴트 쪽으로 차오르고, 떼면 원래 색으로 돌아온다. 제외한 컴포넌트는 그대로다.
	//==============================================================================
	#[test]
	fn pressTintReachesSiblingPaintAndChildText() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, buttonId) = createButtonTree();
		let labelId = nodeTree.createChild(buttonId, "label");
		nodeTree[labelId].addComponent(Text::createWithText("ok", 20.0));
		let excludedId = nodeTree.createChild(buttonId, "excluded");
		nodeTree[excludedId].addComponent(Paint::createWithColor(Color::white()));
		getButton(&mut nodeTree, buttonId).excludeNodeFromTint(excludedId);

		let inside = Vector2::create(210.0, 105.0);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		nodeTree.tick(&engine, 0.3);
		let pressedPaintColor = nodeTree[buttonId].getComponent::<Paint>().expect("paint").getColor();
		assert!((pressedPaintColor.red - 0.7).abs() < 0.001);
		assert_eq!(pressedPaintColor.alpha, 1.0);
		let pressedTextColor = nodeTree[labelId].getComponent::<Text>().expect("text").getTextColor();
		assert!((pressedTextColor.green - 0.7).abs() < 0.001);
		let excludedColor = nodeTree[excludedId].getComponent::<Paint>().expect("paint").getColor();
		assert_eq!(excludedColor, Color::white());

		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::release, inside);
		let releasedPaintColor = nodeTree[buttonId].getComponent::<Paint>().expect("paint").getColor();
		assert_eq!(releasedPaintColor, Color::white());
	}

	//==============================================================================
	// 형제 Paint 를 타입으로 제외하면 색이 바뀌지 않는다.
	//==============================================================================
	#[test]
	fn excludedSiblingComponentKeepsColor() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, buttonId) = createButtonTree();
		getButton(&mut nodeTree, buttonId).excludeComponentFromTint::<Paint>(buttonId);
		let inside = Vector2::create(210.0, 105.0);
		nodeTree.dispatchTouch(buttonId, &engine, TouchKind::press, inside);
		nodeTree.tick(&engine, 0.3);
		let paintColor = nodeTree[buttonId].getComponent::<Paint>().expect("paint").getColor();
		assert_eq!(paintColor, Color::white());
	}
}
