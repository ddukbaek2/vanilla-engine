//==============================================================================
// 컨텍스트 메뉴. (원본 ui/uicontextmenu.js 의 UIContextMenu, 우클릭 / 길게 누름 메뉴)
// - show(nodeTree, position, itemList) 로 항목 목록을 그 자리에 띄운다. 바깥을 누르면 닫히고, 항목을 누르면 닫힌 뒤 처리기를 부른다.
// - 메뉴가 부모 영역 밖으로 나가지 않게 오른쪽 / 아래 끝에 붙인다. (부모 크기를 모르면 960 x 640)
// - 씬 뿌리의 맨 위(마지막 자식)에 두고 쓴다.
// - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//   호스트를 꾸미고(왼쪽 위 피봇, 비활성) 바깥 누름 막(투명 UIButton)과 메뉴 판(둥근 Paint)을 자식으로 만든다.
//   UIContextMenu::create(nodeTree, parentId, options) 가 원본 new UIContextMenu(options) + addChild 자리다.
// - 사용:
//     let menuId = UIContextMenu::create(nodeTree, rootId, UIContextMenuOptions::default());
//     nodeTree.withComponentMut::<UIContextMenu, _>(menuId, |contextMenu, tree| {
//         contextMenu.show(tree, viewPosition, vec![UIContextMenuItem::create("복사", Box::new(|context| { ... })), UIContextMenuItem::createDisabled("삭제")]);
//     });
// - 원본과 다른 것:
//   - show / close 는 노드를 바꾸므로 트리를 받는다. 씬 코드는 nodeTree.withComponentMut 로 부른다.
//   - 항목은 UIContextMenuItem 구조체({ text, handler?, isDisabled }), 처리기는 누른 행 노드의 문맥을 받는 한 번짜리 함수(FnOnce)다.
//   - isShowing 은 노드 활성 대신 show / close 가 고치는 값을 돌려준다.
//   - 이전 행은 떼기(removeChild) 대신 지운다. 행 노드 이름은 원본처럼 "ContextMenuItem{번호}" 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::TextAlign;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uibutton::UIButton;
use crate::ui::uilabel::UILabel;

// 부모 크기를 모를 때 쓰는 영역. (원본과 같다)
const DEFAULT_AREA_WIDTH: f32 = 960.0;
const DEFAULT_AREA_HEIGHT: f32 = 640.0;

//==============================================================================
// 컨텍스트 메뉴 설정. (원본 options = { menuWidth = 168, itemHeight = 28, fontSize = 12, panelColor?, textColor? })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIContextMenuOptions {
	pub menuWidth: f32,
	pub itemHeight: f32,
	pub fontSize: f32,
	pub panelColor: Color,
	pub textColor: Color,
}

impl Default for UIContextMenuOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIContextMenuOptions {
		UIContextMenuOptions {
			menuWidth: 168.0,
			itemHeight: 28.0,
			fontSize: 12.0,
			panelColor: Color::create(0.086, 0.106, 0.165, 1.0),
			textColor: Color::create(0.91, 0.925, 0.957, 1.0),
		}
	}
}

//==============================================================================
// 항목 처리기. (누른 행 노드의 문맥을 받는다. 한 번만 불린다)
//==============================================================================
pub type UIContextMenuHandler = Box<dyn FnOnce(&mut ComponentContext)>;

//==============================================================================
// 메뉴 항목. (원본 { text, handler?, isDisabled? })
//==============================================================================
pub struct UIContextMenuItem {
	pub text: String,
	pub handler: Option<UIContextMenuHandler>,
	pub isDisabled: bool,
}

impl UIContextMenuItem {
	//==============================================================================
	// 처리기가 있는 항목 생성.
	//==============================================================================
	pub fn create(text: &str, handler: UIContextMenuHandler) -> UIContextMenuItem {
		UIContextMenuItem {
			text: text.to_string(),
			handler: Some(handler),
			isDisabled: false,
		}
	}

	//==============================================================================
	// 누를 수 없는 항목 생성. (회색 글자)
	//==============================================================================
	pub fn createDisabled(text: &str) -> UIContextMenuItem {
		UIContextMenuItem {
			text: text.to_string(),
			handler: None,
			isDisabled: true,
		}
	}
}

//==============================================================================
// 컨텍스트 메뉴.
//==============================================================================
pub struct UIContextMenu {
	componentBase: ComponentBase,
	options: UIContextMenuOptions,
	nodeId: Option<NodeId>,
	overlayNodeId: Option<NodeId>,
	panelNodeId: Option<NodeId>,
	rowNodeIds: Vec<NodeId>,
	isShowing: bool,
}

impl UIContextMenu {
	//==============================================================================
	// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UIContextMenu {
		UIContextMenu::createWithOptions(UIContextMenuOptions::default())
	}

	//==============================================================================
	// 설정으로 생성. (노드에는 nodeTree.addComponent 로 붙인다)
	//==============================================================================
	pub fn createWithOptions(options: UIContextMenuOptions) -> UIContextMenu {
		UIContextMenu {
			componentBase: ComponentBase::new(),
			options,
			nodeId: None,
			overlayNodeId: None,
			panelNodeId: None,
			rowNodeIds: Vec::new(),
			isShowing: false,
		}
	}

	//==============================================================================
	// 노드 만들기. (parentId 밑에 "UIContextMenu" 노드를 만들어 붙인다. 원본 new UIContextMenu(options) + addChild)
	//==============================================================================
	pub fn create(nodeTree: &mut NodeTree, parentId: NodeId, options: UIContextMenuOptions) -> NodeId {
		let menuId = nodeTree.createChild(parentId, "UIContextMenu");
		nodeTree.addComponent(menuId, UIContextMenu::createWithOptions(options));
		menuId
	}

	//==============================================================================
	// 메뉴 열기. (position 은 메뉴 왼쪽 위의 뷰 좌표. 행을 다시 만들고 영역 안에 붙여 띄운다)
	//==============================================================================
	pub fn show(&mut self, nodeTree: &mut NodeTree, position: Vector2, itemList: Vec<UIContextMenuItem>) {
		let (menuId, overlayId, panelId) = match (self.nodeId, self.overlayNodeId, self.panelNodeId) {
			(Some(menuId), Some(overlayId), Some(panelId)) => {
				(menuId, overlayId, panelId)
			}
			_ => {
				return;
			}
		};
		if !nodeTree.contains(menuId) || !nodeTree.contains(panelId) {
			return;
		}

		// 이전 행을 지우고 다시 만든다. (항목 수가 적어 부담 없다)
		for rowNodeId in self.rowNodeIds.drain(..) {
			nodeTree.destroyNode(rowNodeId);
		}
		let options = self.options;
		let panelHeight = itemList.len() as f32 * options.itemHeight + 8.0;
		nodeTree[panelId].setContentSize(Vector2::create(options.menuWidth, panelHeight));
		for (itemIndex, itemData) in itemList.into_iter().enumerate() {
			let rowName = format!("ContextMenuItem{}", itemIndex);
			let rowId = nodeTree.createChild(panelId, &rowName);
			let rowNode = &mut nodeTree[rowId];
			rowNode.setPivot(Pivot::topLeft);
			rowNode.setAnchor(Pivot::topLeft);
			rowNode.setContentSize(Vector2::create(options.menuWidth - 8.0, options.itemHeight));
			rowNode.setLocalPosition(Vector2::create(4.0, 4.0 + itemIndex as f32 * options.itemHeight));
			rowNode.setInteractable(true);
			let rowPaint = rowNode.addComponent(Paint::createWithColor(Color::transparent()));
			rowPaint.setRoundSize(6.0);
			let isDisabled = itemData.isDisabled;
			if let Some(rowButton) = nodeTree.addComponent(rowId, UIButton::new()) {
				rowButton.setPressedTintColor(Color::create(1.0, 1.0, 1.0, 0.08));
				if isDisabled {
					rowButton.setInteractable(false);
				}
				else {
					let mut handler = itemData.handler;
					rowButton.setClickedEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
						context.getNodeTreeMut().withComponentMut::<UIContextMenu, _>(menuId, |contextMenu, tree| {
							contextMenu.close(tree);
						});
						if let Some(handler) = handler.take() {
							handler(context);
						}
					}));
				}
			}
			if let Some(rowLabel) = nodeTree.addComponent(rowId, UILabel::new()) {
				rowLabel.setText(&itemData.text);
				rowLabel.setFontSize(options.fontSize);
				rowLabel.setTextAlign(TextAlign::left);
				let labelColor = if isDisabled {
					Color::create(0.4, 0.43, 0.5, 1.0)
				}
				else {
					options.textColor
				};
				rowLabel.setTextColor(labelColor);
			}
			self.rowNodeIds.push(rowId);
		}

		// 부모 크기에 맞춰 막을 깔고, 메뉴가 영역 밖으로 나가지 않게 붙인다.
		let mut areaWidth = DEFAULT_AREA_WIDTH;
		let mut areaHeight = DEFAULT_AREA_HEIGHT;
		if let Some(parentNode) = nodeTree.getParentId(menuId).and_then(|parentId| nodeTree.getNode(parentId)) {
			let parentSize = parentNode.getContentSize();
			if parentSize.x > 0.0 && parentSize.y > 0.0 {
				areaWidth = parentSize.x;
				areaHeight = parentSize.y;
			}
		}
		let areaSize = Vector2::create(areaWidth, areaHeight);
		nodeTree[menuId].setContentSize(areaSize);
		if let Some(overlayNode) = nodeTree.getNodeMut(overlayId) {
			overlayNode.setContentSize(areaSize);
		}
		let menuX = position.x.min(areaWidth - options.menuWidth - 4.0);
		let menuY = position.y.min(areaHeight - panelHeight - 4.0);
		nodeTree[panelId].setLocalPosition(Vector2::create(menuX, menuY));
		nodeTree[menuId].setActive(true);
		self.isShowing = true;
	}

	//==============================================================================
	// 닫기.
	//==============================================================================
	pub fn close(&mut self, nodeTree: &mut NodeTree) {
		if let Some(menuNode) = self.nodeId.and_then(|menuId| nodeTree.getNodeMut(menuId)) {
			menuNode.setActive(false);
		}
		self.isShowing = false;
	}

	//==============================================================================
	// 보이는 중인지.
	//==============================================================================
	pub fn isShowing(&self) -> bool {
		self.isShowing
	}

	//==============================================================================
	// 바깥 누름 막 노드 핸들.
	//==============================================================================
	pub fn getOverlayNode(&self) -> Option<NodeId> {
		self.overlayNodeId
	}

	//==============================================================================
	// 메뉴 판 노드 핸들.
	//==============================================================================
	pub fn getPanelNode(&self) -> Option<NodeId> {
		self.panelNodeId
	}

	//==============================================================================
	// 항목 행 노드 핸들 목록.
	//==============================================================================
	pub fn getItemNodes(&self) -> &[NodeId] {
		&self.rowNodeIds
	}
}

impl Default for UIContextMenu {
	fn default() -> UIContextMenu {
		UIContextMenu::new()
	}
}

impl Component for UIContextMenu {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIContextMenu"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 호스트 꾸미기, 바깥 누름 막, 메뉴 판)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.nodeId = Some(nodeId);
		let menuNode = &mut nodeTree[nodeId];
		menuNode.setPivot(Pivot::topLeft);
		menuNode.setAnchor(Pivot::topLeft);
		menuNode.setActive(false);

		// 바깥 누름 감지 막. (투명)
		let overlayId = nodeTree.createChild(nodeId, "ContextMenuOverlay");
		let overlayNode = &mut nodeTree[overlayId];
		overlayNode.setPivot(Pivot::topLeft);
		overlayNode.setAnchor(Pivot::topLeft);
		overlayNode.setInteractable(true);
		if let Some(overlayButton) = nodeTree.addComponent(overlayId, UIButton::new()) {
			overlayButton.setPressedTintColor(Color::transparent());
			overlayButton.setClickedEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				context.getNodeTreeMut().withComponentMut::<UIContextMenu, _>(nodeId, |contextMenu, tree| {
					contextMenu.close(tree);
				});
			}));
		}
		self.overlayNodeId = Some(overlayId);

		// 메뉴 판.
		let panelId = nodeTree.createChild(nodeId, "ContextMenuPanel");
		let panelNode = &mut nodeTree[panelId];
		panelNode.setPivot(Pivot::topLeft);
		panelNode.setAnchor(Pivot::topLeft);
		let panelPaint = panelNode.addComponent(Paint::createWithColor(self.options.panelColor));
		panelPaint.setRoundSize(8.0);
		self.panelNodeId = Some(panelId);
	}

	fn isTickNeeded(&self) -> bool {
		false
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
	// 시험 장면. (800 x 600 뿌리 밑의 메뉴)
	//==============================================================================
	fn createMenuTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		nodeTree[rootId].setContentSize(Vector2::create(800.0, 600.0));
		let menuId = UIContextMenu::create(&mut nodeTree, rootId, UIContextMenuOptions::default());
		(nodeTree, menuId)
	}

	//==============================================================================
	// 셈하는 항목.
	//==============================================================================
	fn createCountingItem(text: &str, counter: &Rc<Cell<u32>>) -> UIContextMenuItem {
		let counter = counter.clone();
		UIContextMenuItem::create(text, Box::new(move |_context: &mut ComponentContext| {
			counter.set(counter.get() + 1);
		}))
	}

	//==============================================================================
	// 메뉴 열기. (셈 항목 둘 + 누를 수 없는 항목 하나)
	//==============================================================================
	fn showMenu(nodeTree: &mut NodeTree, menuId: NodeId, position: Vector2, copyCount: &Rc<Cell<u32>>, pasteCount: &Rc<Cell<u32>>) {
		let itemList = vec![createCountingItem("복사", copyCount), createCountingItem("붙여넣기", pasteCount), UIContextMenuItem::createDisabled("삭제")];
		nodeTree.withComponentMut::<UIContextMenu, _>(menuId, |contextMenu, tree| {
			contextMenu.show(tree, position, itemList);
		});
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
	// 열면 영역 안으로 붙고, 항목을 누르면 처리기를 부르고 닫힌다. 바깥을 누르면 처리기 없이 닫힌다.
	//==============================================================================
	#[test]
	fn showClampItemClickAndOverlayClose() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, menuId) = createMenuTree();
		assert!(!nodeTree[menuId].isActive());
		let copyCount = Rc::new(Cell::new(0));
		let pasteCount = Rc::new(Cell::new(0));
		showMenu(&mut nodeTree, menuId, Vector2::create(700.0, 580.0), &copyCount, &pasteCount);
		let (panelId, overlayId, itemIds) = {
			let contextMenu = nodeTree[menuId].getComponent::<UIContextMenu>().expect("menu");
			assert!(contextMenu.isShowing());
			(contextMenu.getPanelNode().expect("panel"), contextMenu.getOverlayNode().expect("overlay"), contextMenu.getItemNodes().to_vec())
		};
		assert!(nodeTree[menuId].isActive());
		assert_eq!(nodeTree[overlayId].getContentSize(), Vector2::create(800.0, 600.0));
		assert_eq!(nodeTree[panelId].getContentSize(), Vector2::create(168.0, 92.0));
		assert_eq!(nodeTree[panelId].getLocalPosition(), Vector2::create(628.0, 504.0));
		assert_eq!(itemIds.len(), 3);
		assert_eq!(nodeTree[itemIds[1]].getName(), "ContextMenuItem1");

		clickNode(&mut nodeTree, &engine, itemIds[1]);
		assert_eq!(pasteCount.get(), 1);
		assert_eq!(copyCount.get(), 0);
		assert!(!nodeTree[menuId].isActive());
		assert!(!nodeTree[menuId].getComponent::<UIContextMenu>().expect("menu").isShowing());

		// 다시 열고 누를 수 없는 항목을 누르면 아무 일도 없다. 바깥을 누르면 닫힌다.
		showMenu(&mut nodeTree, menuId, Vector2::create(100.0, 100.0), &copyCount, &pasteCount);
		let itemIds = nodeTree[menuId].getComponent::<UIContextMenu>().expect("menu").getItemNodes().to_vec();
		assert_eq!(nodeTree[panelId].getLocalPosition(), Vector2::create(100.0, 100.0));
		clickNode(&mut nodeTree, &engine, itemIds[2]);
		assert!(nodeTree[menuId].isActive());
		nodeTree.dispatchTouch(overlayId, &engine, TouchKind::press, Vector2::create(10.0, 10.0));
		nodeTree.dispatchTouch(overlayId, &engine, TouchKind::release, Vector2::create(10.0, 10.0));
		assert!(!nodeTree[menuId].isActive());
		assert_eq!(copyCount.get() + pasteCount.get(), 1);
	}
}
