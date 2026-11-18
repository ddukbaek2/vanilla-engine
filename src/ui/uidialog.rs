//==============================================================================
// 확인 대화 상자. (원본 ui/uidialog.js 의 UIDialog)
// - 어둡게 가리는 막 + 가운데 카드 + 확인 / 취소 단추를 show 한 번으로 띄운다. 등장 / 퇴장은 PopupMotion(되튕김 확대 / 수축)으로 움직인다.
// - 막을 누르면 취소로 처리한다. onCancel 을 넘기지 않으면 취소 단추를 감추고 확인 단추만 넓게 둔 알림이 된다.
// - 씬 뿌리의 맨 위(마지막 자식)에 두고 쓴다. 부모 contentSize 를 막 크기로 쓴다. (없으면 960 x 640)
// - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼 호스트를 꾸미고
//   (왼쪽 위 피봇, 비활성) 막, 카드, 메시지, 단추 노드를 자식으로 만든다. UIDialog::create(nodeTree, parentId, options) 가 new + addChild 자리다.
// - 사용:
//     let dialogId = UIDialog::create(nodeTree, rootId, UIDialogOptions::default());
//     nodeTree.withComponentMut::<UIDialog, _>(dialogId, |dialog, tree| {
//         dialog.show(tree, "정말 삭제할까요?", UIDialogShowOptions { onConfirm: Some(Box::new(|dialog, context| { ... })), onCancel: Some(Box::new(|_, _| {})), ..UIDialogShowOptions::default() });
//     });
// - 원본과 다른 것:
//   - show 는 노드를 바꾸므로 트리를 받는다. 씬 코드는 nodeTree.withComponentMut 로, 콜백에서는 context.getNodeTreeMut() 로 부른다.
//   - 확인 / 취소 처리기는 대화 상자와 그 호스트의 문맥을 받는 한 번짜리 함수(FnOnce)다. dismiss 는 처리기에 문맥을 넘기려고 문맥을 받는다.
//     씬 코드에서 닫을 때는 requestDismiss 를 부르면 다음 tick 에 dismiss 한다.
//   - 원본은 PopupMotion 닫힘 알림에서 setActive(false) 한다. 러스트 판은 tick 에서 보이던 것이 닫힘이 되면(즉시 전환이면 dismiss 에서) 끈다.
//   - 원본 tick 은 자식 tick 다음에 전환기를 돌린다. 러스트 판은 컴포넌트가 자식보다 먼저 돈다. (보이는 차이는 없다)
//   - 전환 시간은 PopupMotion 그대로다. (등장 0.32 초, 퇴장 0.22 초. Tween 은 쓰지 않는다)
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::popupmotion::PopupMotion;
use crate::ui::uibutton::UIButton;
use crate::ui::uilabel::UILabel;

// 부모 크기를 모를 때 쓰는 영역. (원본과 같다)
const DEFAULT_AREA_WIDTH: f32 = 960.0;
const DEFAULT_AREA_HEIGHT: f32 = 640.0;

// 단추 높이와 카드 안쪽 여백. (원본과 같다)
const BUTTON_HEIGHT: f32 = 52.0;
const PANEL_PADDING: f32 = 28.0;
const BUTTON_GAP: f32 = 16.0;
const BUTTON_BOTTOM_OFFSET: f32 = 76.0;

//==============================================================================
// 대화 상자 설정. (원본 options = { width = 420, height = 240, confirmText = "확인", cancelText = "취소", panelColor?, accentColor?, textColor? })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct UIDialogOptions {
	pub width: f32,
	pub height: f32,
	pub confirmText: String,
	pub cancelText: String,
	// 카드 바탕색.
	pub panelColor: Color,
	// 확인 단추 색.
	pub accentColor: Color,
	pub textColor: Color,
}

impl Default for UIDialogOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIDialogOptions {
		UIDialogOptions {
			width: 420.0,
			height: 240.0,
			confirmText: String::from("확인"),
			cancelText: String::from("취소"),
			panelColor: Color::create(0.13, 0.15, 0.2, 1.0),
			accentColor: Color::create(0.23, 0.51, 0.96, 1.0),
			textColor: Color::create(0.96, 0.97, 1.0, 1.0),
		}
	}
}

//==============================================================================
// 확인 / 취소 처리기. (대화 상자와 그 호스트의 문맥을 받는다. 한 번만 불린다)
//==============================================================================
pub type UIDialogHandler = Box<dyn FnOnce(&mut UIDialog, &mut ComponentContext)>;

//==============================================================================
// 열기 설정. (원본 show 의 options = { onConfirm?, onCancel?, confirmText?, cancelText? })
//==============================================================================
#[derive(Default)]
pub struct UIDialogShowOptions {
	pub onConfirm: Option<UIDialogHandler>,
	// 없으면 취소 단추를 감춘 알림이 된다.
	pub onCancel: Option<UIDialogHandler>,
	pub confirmText: Option<String>,
	pub cancelText: Option<String>,
}

//==============================================================================
// 확인 대화 상자.
//==============================================================================
pub struct UIDialog {
	componentBase: ComponentBase,
	options: UIDialogOptions,
	nodeId: Option<NodeId>,
	popupMotion: PopupMotion,
	overlayNodeId: Option<NodeId>,
	panelNodeId: Option<NodeId>,
	messageNodeId: Option<NodeId>,
	confirmNodeId: Option<NodeId>,
	cancelNodeId: Option<NodeId>,
	confirmHandler: Option<UIDialogHandler>,
	cancelHandler: Option<UIDialogHandler>,
	pendingDismiss: Option<bool>,
}

impl UIDialog {
	//==============================================================================
	// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UIDialog {
		UIDialog::createWithOptions(UIDialogOptions::default())
	}

	//==============================================================================
	// 설정으로 생성. (노드에는 nodeTree.addComponent 로 붙인다)
	//==============================================================================
	pub fn createWithOptions(options: UIDialogOptions) -> UIDialog {
		UIDialog {
			componentBase: ComponentBase::new(),
			options,
			nodeId: None,
			popupMotion: PopupMotion::new(),
			overlayNodeId: None,
			panelNodeId: None,
			messageNodeId: None,
			confirmNodeId: None,
			cancelNodeId: None,
			confirmHandler: None,
			cancelHandler: None,
			pendingDismiss: None,
		}
	}

	//==============================================================================
	// 노드 만들기. (parentId 밑에 "UIDialog" 노드를 만들어 붙인다. 원본 new UIDialog(options) + addChild)
	//==============================================================================
	pub fn create(nodeTree: &mut NodeTree, parentId: NodeId, options: UIDialogOptions) -> NodeId {
		let dialogId = nodeTree.createChild(parentId, "UIDialog");
		nodeTree.addComponent(dialogId, UIDialog::createWithOptions(options));
		dialogId
	}

	//==============================================================================
	// 단추 노드 만들기. (둥근 Paint + UIButton + UILabel)
	//==============================================================================
	fn createButtonNode(nodeTree: &mut NodeTree, parentId: NodeId, nodeName: &str, labelText: &str, fillColor: Color, textColor: Color, buttonWidth: f32) -> NodeId {
		let buttonId = nodeTree.createChild(parentId, nodeName);
		let buttonNode = &mut nodeTree[buttonId];
		buttonNode.setPivot(Pivot::topLeft);
		buttonNode.setAnchor(Pivot::topLeft);
		buttonNode.setContentSize(Vector2::create(buttonWidth, BUTTON_HEIGHT));
		buttonNode.setInteractable(true);
		let paint = buttonNode.addComponent(Paint::createWithColor(fillColor));
		paint.setRoundSize(14.0);
		nodeTree.addComponent(buttonId, UIButton::new());
		if let Some(label) = nodeTree.addComponent(buttonId, UILabel::new()) {
			label.setText(labelText);
			label.setFontSize(16.0);
			label.setTextColor(textColor);
		}
		buttonId
	}

	//==============================================================================
	// 단추 클릭이 대화 상자를 닫게 한다. (isConfirmed 에 따라 확인 / 취소)
	//==============================================================================
	fn bindDismissButton(nodeTree: &mut NodeTree, buttonId: NodeId, dialogId: NodeId, isConfirmed: bool) {
		if let Some(button) = nodeTree[buttonId].getComponentMut::<UIButton>() {
			button.setClickedEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				context.withComponentMut::<UIDialog, _>(dialogId, |dialog, dialogContext| {
					dialog.dismiss(dialogContext, isConfirmed);
				});
			}));
		}
	}

	//==============================================================================
	// 노드의 UILabel 글 바꾸기.
	//==============================================================================
	fn setNodeLabelText(nodeTree: &mut NodeTree, nodeId: Option<NodeId>, text: &str) {
		if let Some(label) = nodeId.and_then(|nodeId| nodeTree.getNodeMut(nodeId)).and_then(|node| node.getComponentMut::<UILabel>()) {
			label.setText(text);
		}
	}

	//==============================================================================
	// 대화 상자 열기. (onCancel 이 없으면 취소 단추를 감추고 확인 단추를 가운데로 넓힌다)
	//==============================================================================
	pub fn show(&mut self, nodeTree: &mut NodeTree, messageText: &str, options: UIDialogShowOptions) {
		let (hostId, panelId) = match (self.nodeId, self.panelNodeId) {
			(Some(hostId), Some(panelId)) => {
				(hostId, panelId)
			}
			_ => {
				return;
			}
		};
		if !nodeTree.contains(hostId) || !nodeTree.contains(panelId) {
			return;
		}
		UIDialog::setNodeLabelText(nodeTree, self.messageNodeId, messageText);
		let hasCancel = options.onCancel.is_some();
		self.confirmHandler = options.onConfirm;
		self.cancelHandler = options.onCancel;
		self.pendingDismiss = None;
		if let Some(confirmText) = options.confirmText.as_deref() {
			UIDialog::setNodeLabelText(nodeTree, self.confirmNodeId, confirmText);
		}
		if let Some(cancelText) = options.cancelText.as_deref() {
			UIDialog::setNodeLabelText(nodeTree, self.cancelNodeId, cancelText);
		}

		let panelSize = nodeTree[panelId].getContentSize();
		let buttonWidth = (panelSize.x - PANEL_PADDING * 2.0 - BUTTON_GAP) * 0.5;
		if let Some(cancelNode) = self.cancelNodeId.and_then(|cancelId| nodeTree.getNodeMut(cancelId)) {
			cancelNode.setActive(hasCancel);
		}
		if let Some(confirmNode) = self.confirmNodeId.and_then(|confirmId| nodeTree.getNodeMut(confirmId)) {
			if hasCancel {
				confirmNode.setContentSize(Vector2::create(buttonWidth, BUTTON_HEIGHT));
				confirmNode.setLocalPosition(Vector2::create(PANEL_PADDING + buttonWidth + BUTTON_GAP, panelSize.y - BUTTON_BOTTOM_OFFSET));
			}
			else {
				confirmNode.setContentSize(Vector2::create(panelSize.x - PANEL_PADDING * 2.0, BUTTON_HEIGHT));
				confirmNode.setLocalPosition(Vector2::create(PANEL_PADDING, panelSize.y - BUTTON_BOTTOM_OFFSET));
			}
		}

		nodeTree[hostId].setActive(true);
		self.updateLayoutToParent(nodeTree);
		self.popupMotion.open();
		self.applyMotion(nodeTree);
	}

	//==============================================================================
	// 닫기. (다 열린 뒤에만. isConfirmed 에 따라 확인 / 취소 처리기를 부른다. 문맥은 대화 상자 호스트의 것)
	//==============================================================================
	pub fn dismiss(&mut self, context: &mut ComponentContext, isConfirmed: bool) {
		if !self.popupMotion.isInteractive() {
			return;
		}
		let handler = if isConfirmed {
			self.confirmHandler.take()
		}
		else {
			self.cancelHandler.take()
		};
		self.confirmHandler = None;
		self.cancelHandler = None;
		self.popupMotion.close();
		// 즉시 전환이면 바로 닫혔으므로 호스트를 끈다. (원본 닫힘 알림)
		if !self.popupMotion.isVisible() {
			if let Some(hostNode) = self.nodeId.and_then(|hostId| context.getNodeTreeMut().getNodeMut(hostId)) {
				hostNode.setActive(false);
			}
		}
		if let Some(handler) = handler {
			handler(self, context);
		}
	}

	//==============================================================================
	// 다음 tick 에 닫기. (문맥이 없는 씬 코드용)
	//==============================================================================
	pub fn requestDismiss(&mut self, isConfirmed: bool) {
		self.pendingDismiss = Some(isConfirmed);
	}

	//==============================================================================
	// 전환기 산출값을 노드에 건다. (막 투명도, 카드 배율)
	//==============================================================================
	fn applyMotion(&self, nodeTree: &mut NodeTree) {
		let dimRatio = self.popupMotion.getDimRatio();
		if let Some(overlayNode) = self.overlayNodeId.and_then(|overlayId| nodeTree.getNodeMut(overlayId)) {
			overlayNode.setLocalOpacity(dimRatio);
		}
		let panelScale = self.popupMotion.getScale();
		if let Some(panelNode) = self.panelNodeId.and_then(|panelId| nodeTree.getNodeMut(panelId)) {
			panelNode.setLocalScale(Vector2::create(panelScale, panelScale));
		}
	}

	//==============================================================================
	// 부모 크기에 맞춰 막과 카드 자리를 고친다.
	//==============================================================================
	fn updateLayoutToParent(&self, nodeTree: &mut NodeTree) {
		let hostId = match self.nodeId {
			Some(hostId) => {
				hostId
			}
			None => {
				return;
			}
		};
		let mut areaWidth = DEFAULT_AREA_WIDTH;
		let mut areaHeight = DEFAULT_AREA_HEIGHT;
		if let Some(parentNode) = nodeTree.getParentId(hostId).and_then(|parentId| nodeTree.getNode(parentId)) {
			let parentSize = parentNode.getContentSize();
			if parentSize.x > 0.0 && parentSize.y > 0.0 {
				areaWidth = parentSize.x;
				areaHeight = parentSize.y;
			}
		}
		let areaSize = Vector2::create(areaWidth, areaHeight);
		nodeTree[hostId].setContentSize(areaSize);
		if let Some(overlayNode) = self.overlayNodeId.and_then(|overlayId| nodeTree.getNodeMut(overlayId)) {
			overlayNode.setContentSize(areaSize);
			overlayNode.setLocalPosition(Vector2::zero());
		}
		if let Some(panelNode) = self.panelNodeId.and_then(|panelId| nodeTree.getNodeMut(panelId)) {
			panelNode.setLocalPosition(Vector2::create(areaWidth * 0.5, areaHeight * 0.5));
		}
	}

	//==============================================================================
	// 보이는 중인지. (등장 / 열림 / 퇴장 중이면 참)
	//==============================================================================
	pub fn isShowing(&self) -> bool {
		self.popupMotion.isVisible()
	}

	//==============================================================================
	// 즉시 전환 설정. (연출 없는 UI 톤)
	//==============================================================================
	pub fn setInstantEnabled(&mut self, isInstantEnabled: bool) {
		self.popupMotion.setInstantEnabled(isInstantEnabled);
	}

	//==============================================================================
	// 전환기 반환.
	//==============================================================================
	pub fn getPopupMotion(&self) -> &PopupMotion {
		&self.popupMotion
	}

	//==============================================================================
	// 막 노드 핸들.
	//==============================================================================
	pub fn getOverlayNode(&self) -> Option<NodeId> {
		self.overlayNodeId
	}

	//==============================================================================
	// 카드 노드 핸들.
	//==============================================================================
	pub fn getPanelNode(&self) -> Option<NodeId> {
		self.panelNodeId
	}

	//==============================================================================
	// 확인 단추 노드 핸들.
	//==============================================================================
	pub fn getConfirmNode(&self) -> Option<NodeId> {
		self.confirmNodeId
	}

	//==============================================================================
	// 취소 단추 노드 핸들.
	//==============================================================================
	pub fn getCancelNode(&self) -> Option<NodeId> {
		self.cancelNodeId
	}
}

impl Default for UIDialog {
	fn default() -> UIDialog {
		UIDialog::new()
	}
}

impl Component for UIDialog {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIDialog"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 호스트 꾸미기, 막, 카드, 메시지, 단추)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.nodeId = Some(nodeId);
		let panelWidth = self.options.width;
		let panelHeight = self.options.height;
		let textColor = self.options.textColor;
		let hostNode = &mut nodeTree[nodeId];
		hostNode.setPivot(Pivot::topLeft);
		hostNode.setAnchor(Pivot::topLeft);
		hostNode.setActive(false);

		// 어둡게 가리는 막. (누르면 취소)
		let overlayId = nodeTree.createChild(nodeId, "DialogOverlay");
		let overlayNode = &mut nodeTree[overlayId];
		overlayNode.setPivot(Pivot::topLeft);
		overlayNode.setAnchor(Pivot::topLeft);
		overlayNode.setInteractable(true);
		overlayNode.addComponent(Paint::createWithColor(Color::create(0.0, 0.0, 0.0, 0.55)));
		if let Some(overlayButton) = nodeTree.addComponent(overlayId, UIButton::new()) {
			overlayButton.setPressedTintColor(Color::transparent());
		}
		UIDialog::bindDismissButton(nodeTree, overlayId, nodeId, false);
		self.overlayNodeId = Some(overlayId);

		// 카드.
		let panelId = nodeTree.createChild(nodeId, "DialogPanel");
		let panelNode = &mut nodeTree[panelId];
		panelNode.setPivot(Pivot::middleCenter);
		panelNode.setAnchor(Pivot::topLeft);
		panelNode.setContentSize(Vector2::create(panelWidth, panelHeight));
		let panelPaint = panelNode.addComponent(Paint::createWithColor(self.options.panelColor));
		panelPaint.setRoundSize(18.0);
		self.panelNodeId = Some(panelId);

		// 메시지.
		let messageId = nodeTree.createChild(panelId, "DialogMessage");
		let messageNode = &mut nodeTree[messageId];
		messageNode.setPivot(Pivot::topLeft);
		messageNode.setAnchor(Pivot::topLeft);
		messageNode.setContentSize(Vector2::create(panelWidth - PANEL_PADDING * 2.0, panelHeight - 120.0));
		messageNode.setLocalPosition(Vector2::create(PANEL_PADDING, 24.0));
		if let Some(messageLabel) = nodeTree.addComponent(messageId, UILabel::new()) {
			messageLabel.setText("");
			messageLabel.setFontSize(17.0);
			messageLabel.setTextColor(textColor);
			messageLabel.setWordWrapWidth(panelWidth - PANEL_PADDING * 2.0);
		}
		self.messageNodeId = Some(messageId);

		// 단추.
		let buttonWidth = (panelWidth - PANEL_PADDING * 2.0 - BUTTON_GAP) * 0.5;
		let cancelId = UIDialog::createButtonNode(nodeTree, panelId, "DialogCancel", &self.options.cancelText, Color::create(0.22, 0.25, 0.32, 1.0), textColor, buttonWidth);
		nodeTree[cancelId].setLocalPosition(Vector2::create(PANEL_PADDING, panelHeight - BUTTON_BOTTOM_OFFSET));
		let confirmId = UIDialog::createButtonNode(nodeTree, panelId, "DialogConfirm", &self.options.confirmText, self.options.accentColor, textColor, buttonWidth);
		nodeTree[confirmId].setLocalPosition(Vector2::create(PANEL_PADDING + buttonWidth + BUTTON_GAP, panelHeight - BUTTON_BOTTOM_OFFSET));
		UIDialog::bindDismissButton(nodeTree, cancelId, nodeId, false);
		UIDialog::bindDismissButton(nodeTree, confirmId, nodeId, true);
		self.cancelNodeId = Some(cancelId);
		self.confirmNodeId = Some(confirmId);
	}

	//==============================================================================
	// 갱신. (미뤄 둔 닫기 → 전환기 → 노드에 반영 → 다 닫혔으면 호스트를 끈다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		self.nodeId = Some(context.getNodeId());
		if let Some(isConfirmed) = self.pendingDismiss.take() {
			self.dismiss(context, isConfirmed);
		}
		let wasVisible = self.popupMotion.isVisible();
		self.popupMotion.tick(timeDelta);
		self.applyMotion(context.getNodeTreeMut());
		if wasVisible && !self.popupMotion.isVisible() {
			context.getNodeMut().setActive(false);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use crate::ui::popupmotion::PopupMotionState;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (800 x 600 뿌리 밑의 대화 상자)
	//==============================================================================
	fn createDialogTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		nodeTree[rootId].setContentSize(Vector2::create(800.0, 600.0));
		let dialogId = UIDialog::create(&mut nodeTree, rootId, UIDialogOptions::default());
		(nodeTree, dialogId)
	}

	//==============================================================================
	// 셈하는 처리기.
	//==============================================================================
	fn createCountingHandler(counter: &Rc<Cell<u32>>) -> UIDialogHandler {
		let counter = counter.clone();
		Box::new(move |_dialog: &mut UIDialog, _context: &mut ComponentContext| {
			counter.set(counter.get() + 1);
		})
	}

	//==============================================================================
	// 노드 가운데를 누르고 뗀다.
	//==============================================================================
	fn clickNode(nodeTree: &mut NodeTree, engine: &Engine, nodeId: NodeId) {
		let contentSize = nodeTree[nodeId].getContentSize();
		let center = nodeTree.convertNodePositionToViewPosition(nodeId, contentSize.multiplyScalar(0.5));
		nodeTree.dispatchTouch(nodeId, engine, TouchKind::press, center);
		nodeTree.dispatchTouch(nodeId, engine, TouchKind::release, center);
	}

	//==============================================================================
	// 대화 상자 반환.
	//==============================================================================
	fn getDialog(nodeTree: &mut NodeTree, dialogId: NodeId) -> &mut UIDialog {
		nodeTree[dialogId].getComponentMut::<UIDialog>().expect("dialog")
	}

	//==============================================================================
	// 열기 → 다 열리기 전 누름은 무시 → 확인 → 퇴장 뒤 호스트가 꺼진다.
	//==============================================================================
	#[test]
	fn showConfirmAndClose() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, dialogId) = createDialogTree();
		assert!(!nodeTree[dialogId].isActive());
		let confirmCount = Rc::new(Cell::new(0));
		let cancelCount = Rc::new(Cell::new(0));
		let showOptions = UIDialogShowOptions {
			onConfirm: Some(createCountingHandler(&confirmCount)),
			onCancel: Some(createCountingHandler(&cancelCount)),
			confirmText: Some(String::from("삭제")),
			..UIDialogShowOptions::default()
		};
		nodeTree.withComponentMut::<UIDialog, _>(dialogId, |dialog, tree| {
			dialog.show(tree, "정말 삭제할까요?", showOptions);
		});
		let (overlayId, panelId, confirmId, cancelId) = {
			let dialog = getDialog(&mut nodeTree, dialogId);
			(dialog.getOverlayNode().expect("overlay"), dialog.getPanelNode().expect("panel"), dialog.getConfirmNode().expect("confirm"), dialog.getCancelNode().expect("cancel"))
		};
		assert!(nodeTree[dialogId].isActive());
		assert!(getDialog(&mut nodeTree, dialogId).isShowing());
		assert_eq!(nodeTree[dialogId].getContentSize(), Vector2::create(800.0, 600.0));
		assert_eq!(nodeTree[overlayId].getContentSize(), Vector2::create(800.0, 600.0));
		assert_eq!(nodeTree[panelId].getLocalPosition(), Vector2::create(400.0, 300.0));
		assert_eq!(nodeTree[overlayId].getLocalOpacity(), 0.0);
		assert!((nodeTree[panelId].getLocalScale().x - 0.45).abs() < 0.0001);
		assert!(nodeTree[cancelId].isActive());
		assert_eq!(nodeTree[confirmId].getComponent::<UILabel>().expect("label").getText(), "삭제");

		// 다 열리기 전에는 눌러도 닫히지 않는다.
		nodeTree.tick(&engine, 0.1);
		clickNode(&mut nodeTree, &engine, confirmId);
		assert_eq!(confirmCount.get(), 0);
		nodeTree.tick(&engine, 0.25);
		assert_eq!(getDialog(&mut nodeTree, dialogId).getPopupMotion().getState(), PopupMotionState::open);
		assert_eq!(nodeTree[overlayId].getLocalOpacity(), 1.0);
		assert_eq!(nodeTree[panelId].getLocalScale(), Vector2::create(1.0, 1.0));

		clickNode(&mut nodeTree, &engine, confirmId);
		assert_eq!(confirmCount.get(), 1);
		assert_eq!(cancelCount.get(), 0);
		assert_eq!(getDialog(&mut nodeTree, dialogId).getPopupMotion().getState(), PopupMotionState::closing);
		assert!(nodeTree[dialogId].isActive());
		nodeTree.tick(&engine, 0.25);
		assert!(!getDialog(&mut nodeTree, dialogId).isShowing());
		assert!(!nodeTree[dialogId].isActive());
	}

	//==============================================================================
	// 막을 누르면 취소. 취소 처리기가 없으면 취소 단추를 감추고 확인 단추를 넓힌다.
	//==============================================================================
	#[test]
	fn overlayCancelsAndAlertLayout() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, dialogId) = createDialogTree();
		let cancelCount = Rc::new(Cell::new(0));
		let showOptions = UIDialogShowOptions {
			onCancel: Some(createCountingHandler(&cancelCount)),
			..UIDialogShowOptions::default()
		};
		nodeTree.withComponentMut::<UIDialog, _>(dialogId, |dialog, tree| {
			dialog.show(tree, "가운데를 벗어나 누르면 취소", showOptions);
		});
		nodeTree.tick(&engine, 0.4);
		let overlayId = getDialog(&mut nodeTree, dialogId).getOverlayNode().expect("overlay");
		nodeTree.dispatchTouch(overlayId, &engine, TouchKind::press, Vector2::create(20.0, 20.0));
		nodeTree.dispatchTouch(overlayId, &engine, TouchKind::release, Vector2::create(20.0, 20.0));
		assert_eq!(cancelCount.get(), 1);
		nodeTree.tick(&engine, 0.3);
		assert!(!nodeTree[dialogId].isActive());

		// 알림 꼴: 취소 단추 없음, 확인 단추가 넓다.
		nodeTree.withComponentMut::<UIDialog, _>(dialogId, |dialog, tree| {
			dialog.show(tree, "저장했어요", UIDialogShowOptions::default());
		});
		let (confirmId, cancelId) = {
			let dialog = getDialog(&mut nodeTree, dialogId);
			(dialog.getConfirmNode().expect("confirm"), dialog.getCancelNode().expect("cancel"))
		};
		assert!(!nodeTree[cancelId].isActive());
		assert_eq!(nodeTree[confirmId].getContentSize(), Vector2::create(364.0, 52.0));
		assert_eq!(nodeTree[confirmId].getLocalPosition(), Vector2::create(28.0, 164.0));
	}

	//==============================================================================
	// 즉시 전환: 열면 바로 열리고, 닫으면 바로 호스트가 꺼진다. requestDismiss 는 다음 tick 에 닫는다.
	//==============================================================================
	#[test]
	fn instantModeAndRequestDismiss() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, dialogId) = createDialogTree();
		let confirmCount = Rc::new(Cell::new(0));
		getDialog(&mut nodeTree, dialogId).setInstantEnabled(true);
		let showOptions = UIDialogShowOptions {
			onConfirm: Some(createCountingHandler(&confirmCount)),
			..UIDialogShowOptions::default()
		};
		nodeTree.withComponentMut::<UIDialog, _>(dialogId, |dialog, tree| {
			dialog.show(tree, "바로 열림", showOptions);
		});
		assert!(getDialog(&mut nodeTree, dialogId).getPopupMotion().isInteractive());
		getDialog(&mut nodeTree, dialogId).requestDismiss(true);
		assert!(nodeTree[dialogId].isActive());
		nodeTree.tick(&engine, 0.016);
		assert_eq!(confirmCount.get(), 1);
		assert!(!nodeTree[dialogId].isActive());
		assert!(!getDialog(&mut nodeTree, dialogId).isShowing());
	}
}
