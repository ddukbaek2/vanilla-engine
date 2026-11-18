//==============================================================================
// 드롭다운. (원본 ui/uidropdown.js 의 UIDropdown, 콤보박스)
// - 닫힌 머리 단추에 지금 값이 보이고, 누르면 아래로 선택지 목록이 펼쳐진다. 행을 누르면 고르고 접힌다.
// - 펼칠 때 목록을 트리 뿌리의 마지막 자식으로 옮겨, 나중에 그려지는 형제 노드에 가리지 않게 한다. (접으면 머리 단추 아래로 되돌린다)
// - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//   호스트를 꾸미고(왼쪽 위 피봇, 크기, interactable) Paint + UIButton + UILabel(머리 글)을 호스트에, 화살표 노드와 펼침 목록 노드를 자식으로 만든다.
//   UIDropdown::create(nodeTree, parentId, options) 가 원본 new UIDropdown(options) + addChild 자리다.
// - 사용:
//     let dropdownId = UIDropdown::create(nodeTree, panelId, UIDropdownOptions { width: 180.0, ..UIDropdownOptions::default() });
//     nodeTree.withComponentMut::<UIDropdown, _>(dropdownId, |dropdown, tree| dropdown.setOptionList(tree, &["Dark", "Light", "System"]));
//     nodeTree[dropdownId].getComponentMut::<UIDropdown>().unwrap().setChangedEvent(Box::new(|dropdown, context| { dropdown.getSelectedText(); }));
// - 원본과 다른 것:
//   - 트리를 바꾸는 함수(setOptionList, setOpen, setSelectedIndex)는 트리를 받는다. 씬 코드는 nodeTree.withComponentMut 로 부른다.
//     selectIndex 는 바뀜 이벤트에 문맥을 넘기려고 문맥을 받는다. (행 단추가 ComponentContext::withComponentMut 로 부른다)
//   - 바뀜 이벤트는 드롭다운과 문맥을 받는다. options 객체는 UIDropdownOptions 구조체다.
//   - 머리 단추의 틴트 대상에서 펼침 목록 노드를 뺀다. 원본은 목록 행까지 머리 단추 틴트에 잡혀, 고른 뒤 머리 단추가 매 tick
//     눌렀을 때 담아 둔 옛 행 색을 다시 써서 선택 강조가 덮인다. (excludeNodeFromTint 로 막았다)
//     같은 까닭으로 강조를 바꾼 뒤에는 행 단추들이 틴트 원래 색을 다음 tick 에 다시 담게 한다. (원본은 다시 펼치면 행 단추가 옛 색을 쓴다)
//   - 행 강조는 펼침 목록의 자식 순서 대신 만든 행 노드 목록으로 고친다. (tick 중에 다시 만들면 지운 행이 잠깐 자식으로 남는다)
//   - 화살표 글자는 원본의 작은 삼각형(▾ ▴, U+25BE / U+25B4)이 기본 글꼴(맑은 고딕, Segoe UI, Arial)에 없어
//     WGL4 에 든 ▼ ▲ 를 글자 크기 0.75 배로 쓴다.
//   - 노드 이름은 원본처럼 "DropdownArrow", "DropdownPopup", "DropdownOption{번호}" 다. 호스트 이름은 만든 쪽이 정한다. (create 는 "UIDropdown")
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uibutton::UIButton;
use crate::ui::uilabel::UILabel;

// 펼친 화살표 / 접힌 화살표. (원본 ▴ ▾ 는 기본 글꼴에 없어 ▲ ▼ 를 작게 쓴다)
const ARROW_OPEN_TEXT: &str = "▲";
const ARROW_CLOSED_TEXT: &str = "▼";
const ARROW_FONT_SCALE: f32 = 0.75;

//==============================================================================
// 드롭다운 설정. (원본 options = { width = 180, height = 30, optionHeight = 28, fontSize = 12, baseColor?, popupColor?, highlightColor?, textColor? })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIDropdownOptions {
	pub width: f32,
	pub height: f32,
	// 선택지 행 높이.
	pub optionHeight: f32,
	pub fontSize: f32,
	// 머리 단추 바탕색.
	pub baseColor: Color,
	// 펼침 목록 바탕색.
	pub popupColor: Color,
	// 고른 행 강조색.
	pub highlightColor: Color,
	pub textColor: Color,
}

impl Default for UIDropdownOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIDropdownOptions {
		UIDropdownOptions {
			width: 180.0,
			height: 30.0,
			optionHeight: 28.0,
			fontSize: 12.0,
			baseColor: Color::create(0.114, 0.141, 0.212, 1.0),
			popupColor: Color::create(0.086, 0.106, 0.165, 1.0),
			highlightColor: Color::create(0.424, 0.482, 1.0, 0.25),
			textColor: Color::create(0.91, 0.925, 0.957, 1.0),
		}
	}
}

//==============================================================================
// 드롭다운 이벤트. (드롭다운과 문맥을 받는다)
//==============================================================================
pub type UIDropdownEvent = Box<dyn FnMut(&mut UIDropdown, &mut ComponentContext)>;

//==============================================================================
// 드롭다운.
//==============================================================================
pub struct UIDropdown {
	componentBase: ComponentBase,
	options: UIDropdownOptions,
	nodeId: Option<NodeId>,
	optionList: Vec<String>,
	selectedIndex: usize,
	isOpen: bool,
	changedEvent: Option<UIDropdownEvent>,
	arrowNodeId: Option<NodeId>,
	popupNodeId: Option<NodeId>,
	rowNodeIds: Vec<NodeId>,
}

impl UIDropdown {
	//==============================================================================
	// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UIDropdown {
		UIDropdown::createWithOptions(UIDropdownOptions::default())
	}

	//==============================================================================
	// 설정으로 생성. (노드에는 nodeTree.addComponent 로 붙인다)
	//==============================================================================
	pub fn createWithOptions(options: UIDropdownOptions) -> UIDropdown {
		UIDropdown {
			componentBase: ComponentBase::new(),
			options,
			nodeId: None,
			optionList: Vec::new(),
			selectedIndex: 0,
			isOpen: false,
			changedEvent: None,
			arrowNodeId: None,
			popupNodeId: None,
			rowNodeIds: Vec::new(),
		}
	}

	//==============================================================================
	// 노드 만들기. (parentId 밑에 "UIDropdown" 노드를 만들어 붙인다. 원본 new UIDropdown(options) + addChild)
	//==============================================================================
	pub fn create(nodeTree: &mut NodeTree, parentId: NodeId, options: UIDropdownOptions) -> NodeId {
		let dropdownId = nodeTree.createChild(parentId, "UIDropdown");
		nodeTree.addComponent(dropdownId, UIDropdown::createWithOptions(options));
		dropdownId
	}

	//==============================================================================
	// 선택지 목록 설정. (펼침 목록 행을 다시 만든다)
	//==============================================================================
	pub fn setOptionList(&mut self, nodeTree: &mut NodeTree, optionList: &[impl AsRef<str>]) {
		self.optionList = optionList.iter().map(|optionText| optionText.as_ref().to_string()).collect();
		self.selectedIndex = self.selectedIndex.min(self.optionList.len().saturating_sub(1));
		let (hostId, popupId) = match (self.nodeId, self.popupNodeId) {
			(Some(hostId), Some(popupId)) => {
				(hostId, popupId)
			}
			_ => {
				return;
			}
		};
		if !nodeTree.contains(hostId) || !nodeTree.contains(popupId) {
			return;
		}

		// 기존 행 지우기.
		for rowNodeId in self.rowNodeIds.drain(..) {
			nodeTree.destroyNode(rowNodeId);
		}

		let contentSize = nodeTree[hostId].getContentSize();
		let optionHeight = self.options.optionHeight;
		let popupHeight = self.optionList.len() as f32 * optionHeight + 8.0;
		nodeTree[popupId].setContentSize(Vector2::create(contentSize.x, popupHeight));
		for optionIndex in 0..self.optionList.len() {
			let rowName = format!("DropdownOption{}", optionIndex);
			let rowId = nodeTree.createChild(popupId, &rowName);
			let rowNode = &mut nodeTree[rowId];
			rowNode.setPivot(Pivot::topLeft);
			rowNode.setAnchor(Pivot::topLeft);
			rowNode.setContentSize(Vector2::create(contentSize.x - 8.0, optionHeight));
			rowNode.setLocalPosition(Vector2::create(4.0, 4.0 + optionIndex as f32 * optionHeight));
			rowNode.setInteractable(true);
			let rowPaint = rowNode.addComponent(Paint::createWithColor(Color::transparent()));
			rowPaint.setRoundSize(6.0);
			if let Some(rowButton) = nodeTree.addComponent(rowId, UIButton::new()) {
				rowButton.setPressedTintColor(Color::create(1.0, 1.0, 1.0, 0.08));
				rowButton.setClickedEvent(Box::new(move |button: &mut UIButton, context: &mut ComponentContext| {
					context.withComponentMut::<UIDropdown, _>(hostId, |dropdown, dropdownContext| {
						dropdown.selectIndex(dropdownContext, optionIndex);
					});
					// 누를 때 담은 행 색은 강조를 바꾸기 전 것이라, 다음 tick 에 다시 담는다.
					button.requestColorTargetCollect();
				}));
			}
			if let Some(rowLabel) = nodeTree.addComponent(rowId, UILabel::new()) {
				rowLabel.setText(&self.optionList[optionIndex]);
				rowLabel.setFontSize(self.options.fontSize);
				rowLabel.setTextColor(self.options.textColor);
			}
			self.rowNodeIds.push(rowId);
		}
		self.refreshHeader(nodeTree);
		self.refreshHighlight(nodeTree);
	}

	//==============================================================================
	// 고르기. (접고, 머리 글과 강조를 고치고, 바뀌었으면 바뀜 이벤트를 부른다. 문맥은 드롭다운 호스트의 것)
	//==============================================================================
	pub fn selectIndex(&mut self, context: &mut ComponentContext, optionIndex: usize) {
		if optionIndex >= self.optionList.len() {
			return;
		}
		let isChanged = self.selectedIndex != optionIndex;
		self.selectedIndex = optionIndex;
		let nodeTree = context.getNodeTreeMut();
		self.setOpen(nodeTree, false);
		self.refreshHeader(nodeTree);
		self.refreshHighlight(nodeTree);
		if isChanged {
			self.fireChangedEvent(context);
		}
	}

	//==============================================================================
	// 바뀜 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fireChangedEvent(&mut self, context: &mut ComponentContext) {
		let mut changedEvent = match self.changedEvent.take() {
			Some(changedEvent) => {
				changedEvent
			}
			None => {
				return;
			}
		};
		changedEvent(self, context);
		if self.changedEvent.is_none() {
			self.changedEvent = Some(changedEvent);
		}
	}

	//==============================================================================
	// 펼침 / 접힘. (펼치면 목록을 트리 뿌리로 옮기고, 접으면 되돌린다)
	//==============================================================================
	pub fn setOpen(&mut self, nodeTree: &mut NodeTree, isOpen: bool) {
		self.isOpen = isOpen;
		if isOpen {
			self.attachPopupToRoot(nodeTree);
		}
		else {
			self.detachPopupFromRoot(nodeTree);
		}
		if let Some(popupNode) = self.popupNodeId.and_then(|popupId| nodeTree.getNodeMut(popupId)) {
			popupNode.setActive(isOpen);
		}
		let arrowText = if isOpen {
			ARROW_OPEN_TEXT
		}
		else {
			ARROW_CLOSED_TEXT
		};
		if let Some(arrowLabel) = self.arrowNodeId.and_then(|arrowId| nodeTree.getNodeMut(arrowId)).and_then(|arrowNode| arrowNode.getComponentMut::<UILabel>()) {
			arrowLabel.setText(arrowText);
		}
	}

	//==============================================================================
	// 펼침 목록을 트리 뿌리로 옮긴다. (머리 단추 왼쪽 아래 + 4 의 뷰 좌표를 뿌리 로컬 좌표로 바꾼다)
	//==============================================================================
	fn attachPopupToRoot(&mut self, nodeTree: &mut NodeTree) {
		let (hostId, popupId) = match (self.nodeId, self.popupNodeId) {
			(Some(hostId), Some(popupId)) => {
				(hostId, popupId)
			}
			_ => {
				return;
			}
		};
		if !nodeTree.contains(hostId) || !nodeTree.contains(popupId) {
			return;
		}
		let mut rootId = hostId;
		while let Some(parentId) = nodeTree.getParentId(rootId) {
			rootId = parentId;
		}
		if rootId == hostId {
			return;
		}
		let hostContentSize = nodeTree[hostId].getContentSize();
		let bottomLeft = nodeTree.convertNodePositionToViewPosition(hostId, Vector2::create(0.0, hostContentSize.y));
		let popupViewPosition = Vector2::create(bottomLeft.x, bottomLeft.y + 4.0);
		let popupLocalPosition = nodeTree.convertViewPositionToNodePosition(rootId, popupViewPosition).unwrap_or(popupViewPosition);
		nodeTree.addChild(rootId, popupId);
		nodeTree[popupId].setLocalPosition(popupLocalPosition);
	}

	//==============================================================================
	// 펼침 목록을 머리 단추 아래로 되돌린다.
	//==============================================================================
	fn detachPopupFromRoot(&mut self, nodeTree: &mut NodeTree) {
		let (hostId, popupId) = match (self.nodeId, self.popupNodeId) {
			(Some(hostId), Some(popupId)) => {
				(hostId, popupId)
			}
			_ => {
				return;
			}
		};
		if !nodeTree.contains(hostId) || !nodeTree.contains(popupId) {
			return;
		}
		if nodeTree.getParentId(popupId) == Some(hostId) {
			return;
		}
		nodeTree.addChild(hostId, popupId);
		let hostContentSize = nodeTree[hostId].getContentSize();
		nodeTree[popupId].setLocalPosition(Vector2::create(0.0, hostContentSize.y + 4.0));
	}

	//==============================================================================
	// 머리 글 갱신. (호스트의 UILabel)
	//==============================================================================
	fn refreshHeader(&mut self, nodeTree: &mut NodeTree) {
		let selectedText = self.getSelectedText().to_string();
		if let Some(headerLabel) = self.nodeId.and_then(|hostId| nodeTree.getNodeMut(hostId)).and_then(|hostNode| hostNode.getComponentMut::<UILabel>()) {
			headerLabel.setText(&selectedText);
		}
	}

	//==============================================================================
	// 고른 행 강조 갱신. (행 단추가 틴트 원래 색을 다음 tick 에 다시 담게 한다)
	//==============================================================================
	fn refreshHighlight(&mut self, nodeTree: &mut NodeTree) {
		for (rowIndex, rowNodeId) in self.rowNodeIds.iter().copied().enumerate() {
			let rowColor = if rowIndex == self.selectedIndex {
				self.options.highlightColor
			}
			else {
				Color::transparent()
			};
			let rowNode = match nodeTree.getNodeMut(rowNodeId) {
				Some(rowNode) => {
					rowNode
				}
				None => {
					continue;
				}
			};
			if let Some(rowPaint) = rowNode.getComponentMut::<Paint>() {
				rowPaint.setColor(rowColor);
			}
			if let Some(rowButton) = rowNode.getComponentMut::<UIButton>() {
				rowButton.requestColorTargetCollect();
			}
		}
	}

	//==============================================================================
	// 바뀜 이벤트 설정.
	//==============================================================================
	pub fn setChangedEvent(&mut self, changedEvent: UIDropdownEvent) {
		self.changedEvent = Some(changedEvent);
	}

	//==============================================================================
	// 고른 번호 설정. (범위로 자른다. 바뀜 이벤트는 부르지 않는다. 원본과 같음)
	//==============================================================================
	pub fn setSelectedIndex(&mut self, nodeTree: &mut NodeTree, selectedIndex: usize) {
		self.selectedIndex = selectedIndex.min(self.optionList.len().saturating_sub(1));
		self.refreshHeader(nodeTree);
		self.refreshHighlight(nodeTree);
	}

	//==============================================================================
	// 고른 번호 반환.
	//==============================================================================
	pub fn getSelectedIndex(&self) -> usize {
		self.selectedIndex
	}

	//==============================================================================
	// 고른 글 반환. (선택지가 없으면 빈 글)
	//==============================================================================
	pub fn getSelectedText(&self) -> &str {
		self.optionList.get(self.selectedIndex).map(String::as_str).unwrap_or("")
	}

	//==============================================================================
	// 선택지 목록 반환.
	//==============================================================================
	pub fn getOptionList(&self) -> &[String] {
		&self.optionList
	}

	//==============================================================================
	// 펼쳐져 있는지.
	//==============================================================================
	pub fn isOpen(&self) -> bool {
		self.isOpen
	}

	//==============================================================================
	// 펼침 목록 노드 핸들. (펼친 동안은 트리 뿌리의 자식)
	//==============================================================================
	pub fn getPopupNode(&self) -> Option<NodeId> {
		self.popupNodeId
	}

	//==============================================================================
	// 선택지 행 노드 핸들 목록.
	//==============================================================================
	pub fn getOptionNodes(&self) -> &[NodeId] {
		&self.rowNodeIds
	}
}

impl Default for UIDropdown {
	fn default() -> UIDropdown {
		UIDropdown::new()
	}
}

impl Component for UIDropdown {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIDropdown"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 호스트 꾸미기, 머리 단추, 화살표, 펼침 목록)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.nodeId = Some(nodeId);
		let options = self.options;
		let hostNode = &mut nodeTree[nodeId];
		hostNode.setPivot(Pivot::topLeft);
		hostNode.setAnchor(Pivot::topLeft);
		hostNode.setContentSize(Vector2::create(options.width, options.height));
		hostNode.setInteractable(true);

		// 머리 단추.
		let headerPaint = hostNode.addComponent(Paint::createWithColor(options.baseColor));
		headerPaint.setRoundSize(8.0);
		if let Some(headerButton) = nodeTree.addComponent(nodeId, UIButton::new()) {
			headerButton.setClickedEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				context.withComponentMut::<UIDropdown, _>(nodeId, |dropdown, dropdownContext| {
					let nextIsOpen = !dropdown.isOpen();
					dropdown.setOpen(dropdownContext.getNodeTreeMut(), nextIsOpen);
				});
			}));
		}
		if let Some(headerLabel) = nodeTree.addComponent(nodeId, UILabel::new()) {
			headerLabel.setText("");
			headerLabel.setFontSize(options.fontSize);
			headerLabel.setTextColor(options.textColor);
		}

		// 펼침 화살표.
		let arrowId = nodeTree.createChild(nodeId, "DropdownArrow");
		let arrowNode = &mut nodeTree[arrowId];
		arrowNode.setPivot(Pivot::topLeft);
		arrowNode.setAnchor(Pivot::topLeft);
		arrowNode.setContentSize(Vector2::create(20.0, options.height));
		arrowNode.setLocalPosition(Vector2::create(options.width - 22.0, 0.0));
		if let Some(arrowLabel) = nodeTree.addComponent(arrowId, UILabel::new()) {
			arrowLabel.setText(ARROW_CLOSED_TEXT);
			arrowLabel.setFontSize(options.fontSize * ARROW_FONT_SCALE);
			arrowLabel.setTextColor(options.textColor);
		}
		self.arrowNodeId = Some(arrowId);

		// 펼침 목록. (닫혀 있으면 비활성)
		let popupId = nodeTree.createChild(nodeId, "DropdownPopup");
		let popupNode = &mut nodeTree[popupId];
		popupNode.setPivot(Pivot::topLeft);
		popupNode.setAnchor(Pivot::topLeft);
		popupNode.setLocalPosition(Vector2::create(0.0, options.height + 4.0));
		popupNode.setActive(false);
		let popupPaint = popupNode.addComponent(Paint::createWithColor(options.popupColor));
		popupPaint.setRoundSize(8.0);
		self.popupNodeId = Some(popupId);

		// 머리 단추 틴트가 펼침 목록 행까지 닿지 않게 한다.
		if let Some(headerButton) = nodeTree[nodeId].getComponentMut::<UIButton>() {
			headerButton.excludeNodeFromTint(popupId);
		}

		// 붙기 전에 정해 둔 선택지가 있으면 행을 만든다.
		if !self.optionList.is_empty() {
			let optionList = self.optionList.clone();
			self.setOptionList(nodeTree, &optionList);
		}
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
	// 시험 장면. (800 x 600 뿌리, (100, 50) 에 놓은 드롭다운, 선택지 셋)
	//==============================================================================
	fn createDropdownTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		nodeTree[rootId].setContentSize(Vector2::create(800.0, 600.0));
		let dropdownId = UIDropdown::create(&mut nodeTree, rootId, UIDropdownOptions::default());
		nodeTree[dropdownId].setLocalPosition(Vector2::create(100.0, 50.0));
		nodeTree.withComponentMut::<UIDropdown, _>(dropdownId, |dropdown, tree| {
			dropdown.setOptionList(tree, &["Dark", "Light", "System"]);
		});
		(nodeTree, dropdownId)
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
	// 드롭다운 반환.
	//==============================================================================
	fn getDropdown(nodeTree: &mut NodeTree, dropdownId: NodeId) -> &mut UIDropdown {
		nodeTree[dropdownId].getComponentMut::<UIDropdown>().expect("dropdown")
	}

	//==============================================================================
	// 붙으면 호스트가 꾸며지고, 선택지 행과 머리 글이 생긴다.
	//==============================================================================
	#[test]
	fn optionListBuildsRows() {
		let (mut nodeTree, dropdownId) = createDropdownTree();
		assert_eq!(nodeTree[dropdownId].getContentSize(), Vector2::create(180.0, 30.0));
		assert!(nodeTree[dropdownId].isInteractable());
		assert_eq!(nodeTree[dropdownId].getComponent::<UILabel>().expect("header").getText(), "Dark");
		let popupId = getDropdown(&mut nodeTree, dropdownId).getPopupNode().expect("popup");
		assert!(!nodeTree[popupId].isActive());
		assert_eq!(nodeTree[popupId].getContentSize(), Vector2::create(180.0, 92.0));
		let rowIds = getDropdown(&mut nodeTree, dropdownId).getOptionNodes().to_vec();
		assert_eq!(rowIds.len(), 3);
		assert_eq!(nodeTree[rowIds[2]].getLocalPosition(), Vector2::create(4.0, 60.0));
		assert_eq!(nodeTree[rowIds[0]].getComponent::<Paint>().expect("paint").getColor(), UIDropdownOptions::default().highlightColor);
		assert_eq!(nodeTree[rowIds[1]].getComponent::<Paint>().expect("paint").getColor(), Color::transparent());
	}

	//==============================================================================
	// 머리를 누르면 펼쳐져 목록이 뿌리로 옮겨 가고, 행을 누르면 고르고 접히며 바뀜 이벤트가 한 번 온다.
	//==============================================================================
	#[test]
	fn openSelectAndClose() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, dropdownId) = createDropdownTree();
		let rootId = nodeTree.getRootId();
		let changedCount = Rc::new(Cell::new(0));
		let changedCounter = changedCount.clone();
		getDropdown(&mut nodeTree, dropdownId).setChangedEvent(Box::new(move |dropdown: &mut UIDropdown, _context: &mut ComponentContext| {
			assert_eq!(dropdown.getSelectedText(), "System");
			changedCounter.set(changedCounter.get() + 1);
		}));
		let popupId = getDropdown(&mut nodeTree, dropdownId).getPopupNode().expect("popup");

		clickNode(&mut nodeTree, &engine, dropdownId);
		assert!(getDropdown(&mut nodeTree, dropdownId).isOpen());
		assert!(nodeTree[popupId].isActive());
		assert_eq!(nodeTree.getParentId(popupId), Some(rootId));
		assert_eq!(nodeTree[popupId].getLocalPosition(), Vector2::create(100.0, 84.0));

		// 펼친 목록의 행은 맨 위에 그려지므로 그 자리의 맨 위 노드다.
		let rowIds = getDropdown(&mut nodeTree, dropdownId).getOptionNodes().to_vec();
		let rowCenter = nodeTree.convertNodePositionToViewPosition(rowIds[2], Vector2::create(86.0, 14.0));
		assert_eq!(nodeTree.findTopmostInteractableNode(rowCenter), Some(rowIds[2]));

		clickNode(&mut nodeTree, &engine, rowIds[2]);
		assert!(!getDropdown(&mut nodeTree, dropdownId).isOpen());
		assert_eq!(getDropdown(&mut nodeTree, dropdownId).getSelectedIndex(), 2);
		assert_eq!(changedCount.get(), 1);
		assert!(!nodeTree[popupId].isActive());
		assert_eq!(nodeTree.getParentId(popupId), Some(dropdownId));
		assert_eq!(nodeTree[popupId].getLocalPosition(), Vector2::create(0.0, 34.0));
		assert_eq!(nodeTree[dropdownId].getComponent::<UILabel>().expect("header").getText(), "System");
		assert_eq!(nodeTree[rowIds[2]].getComponent::<Paint>().expect("paint").getColor(), UIDropdownOptions::default().highlightColor);
		assert_eq!(nodeTree[rowIds[0]].getComponent::<Paint>().expect("paint").getColor(), Color::transparent());

		// 머리 단추가 tick 을 돌아도 행 강조는 그대로다.
		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[rowIds[2]].getComponent::<Paint>().expect("paint").getColor(), UIDropdownOptions::default().highlightColor);

		// 다시 펼쳐 행 단추들이 tick 을 돌아도 강조는 그대로다.
		clickNode(&mut nodeTree, &engine, dropdownId);
		nodeTree.tick(&engine, 0.016);
		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[rowIds[2]].getComponent::<Paint>().expect("paint").getColor(), UIDropdownOptions::default().highlightColor);
		assert_eq!(nodeTree[rowIds[0]].getComponent::<Paint>().expect("paint").getColor(), Color::transparent());

		// 같은 행을 다시 고르면 이벤트가 오지 않는다.
		clickNode(&mut nodeTree, &engine, rowIds[2]);
		assert_eq!(changedCount.get(), 1);
	}

	//==============================================================================
	// setSelectedIndex 는 범위로 자르고 이벤트 없이 머리 글만 고친다.
	//==============================================================================
	#[test]
	fn setSelectedIndexClampsWithoutEvent() {
		let (mut nodeTree, dropdownId) = createDropdownTree();
		nodeTree.withComponentMut::<UIDropdown, _>(dropdownId, |dropdown, tree| {
			dropdown.setSelectedIndex(tree, 7);
		});
		assert_eq!(getDropdown(&mut nodeTree, dropdownId).getSelectedIndex(), 2);
		assert_eq!(getDropdown(&mut nodeTree, dropdownId).getSelectedText(), "System");
		assert_eq!(nodeTree[dropdownId].getComponent::<UILabel>().expect("header").getText(), "System");
	}
}
