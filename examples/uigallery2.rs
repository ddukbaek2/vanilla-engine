//==============================================================================
// UI 위젯 갤러리 2. (바닐라엔진 ui 모듈의 나머지 위젯 예제)
// - 새로 옮긴 위젯을 한 화면에 놓는다: UIListView, UISnapScrollView, UIDropdown, UIDraggable, UILineChart, UIBarChart, UISpinner,
//   UIToast, UIContextMenu, UIDialog(PopupMotion).
// - 터치는 원본 GameScene 처럼 TouchRecognizer 로 받는다.
// - 사람이 없어도 확인할 수 있게, 정해 둔 프레임에 입력을 흉내 낸다:
//   목록 휠 → 스냅 뷰 넘기기 → 드롭다운 열고 고르기 → 토스트 단추 → 칩 둘 끌기 → 드롭다운 열기 → 메뉴 단추 (90 프레임 화면)
//   → 메뉴 바깥 누름 → 드롭다운 고르기 → 대화 상자 단추 → 토스트 (180 프레임 화면) → 확인 단추.
// - 240 프레임이 지나면 스스로 끝난다. (화면 저장: VANILLA_CAPTURE_PATH / VANILLA_CAPTURE_FRAMES / VANILLA_CAPTURE_EXIT)
//==============================================================================
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(clippy::too_many_arguments)]

use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::ui::uiview::isViewPositionInsideNode;
use vanilla::{
	Color, ComponentContext, Engine, EngineConfiguration, Graphic, NodeId, NodeTree, Paint, Pivot, Scene, SceneBase, TouchRecognizer, UIBarChart, UIButton, UIContextMenu, UIContextMenuItem, UIContextMenuOptions, UIDialog, UIDialogOptions, UIDialogShowOptions, UIDraggable, UIDropdown, UIDropdownOptions, UILabel, UILineChart, UIListView, UIScrollView, UISnapScrollView, UISpinner, UISpinnerOptions, UIToast, UIToastOptions, UIView, Vector2, ViewScaleMode,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#14171f";
const PANEL_COLOR: &str = "#1d2230";
const ACCENT_COLOR: &str = "#6c7bff";
const LIGHT_TEXT_COLOR: &str = "#e8ecf4";
const CAPTION_TEXT_COLOR: &str = "#8d97ad";
const LIST_ITEM_COUNT: usize = 500;
const LIST_ITEM_HEIGHT: f32 = 40.0;
const SNAP_PAGE_WIDTH: f32 = 260.0;
const SNAP_PAGE_GAP: f32 = 10.0;
const SNAP_PAGE_COUNT: usize = 5;
const EXIT_FRAME_COUNT: u32 = 240;

//==============================================================================
// 왼쪽 위 기준 상자 노드 만들기.
//==============================================================================
fn createBoxNode(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2) -> NodeId {
	let nodeId = nodeTree.createChild(parentId, name);
	let node = &mut nodeTree[nodeId];
	node.setPivot(Pivot::topLeft);
	node.setLocalPosition(position);
	node.setContentSize(size);
	nodeId
}

//==============================================================================
// 글자 노드 만들기. (UILabel)
//==============================================================================
fn createLabelNode(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, fontSize: f32, color: &str, textAlign: TextAlign) -> NodeId {
	let labelId = createBoxNode(nodeTree, parentId, name, position, size);
	let label = nodeTree.addComponent(labelId, UILabel::createWithText(text, fontSize)).expect("label");
	label.setTextColor(color);
	label.setTextAlign(textAlign);
	label.setTextBaseline(TextBaseline::middle);
	labelId
}

//==============================================================================
// 글자 단추 만들기. (둥근 Paint 바탕 + UILabel + UIButton)
//==============================================================================
fn createTextButtonNode(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2, text: &str) -> NodeId {
	let buttonId = createBoxNode(nodeTree, parentId, name, position, size);
	let buttonPaint = nodeTree[buttonId].addComponent(Paint::createWithColor(ACCENT_COLOR));
	buttonPaint.setRoundSize(10.0);
	let label = nodeTree.addComponent(buttonId, UILabel::createWithText(text, 17.0)).expect("label");
	label.setTextColor(LIGHT_TEXT_COLOR);
	label.setBold(true);
	nodeTree.addComponent(buttonId, UIButton::new());
	buttonId
}

//==============================================================================
// 구역 판 만들기. (배경색을 정한 UIView + 구역 이름)
//==============================================================================
fn createSectionPanel(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2, caption: &str) -> NodeId {
	let panelId = createBoxNode(nodeTree, parentId, name, position, size);
	let panelView = nodeTree.addComponent(panelId, UIView::new()).expect("view");
	panelView.setBackgroundColor(PANEL_COLOR);
	createLabelNode(nodeTree, panelId, "caption", Vector2::create(14.0, 6.0), Vector2::create(size.x - 28.0, 26.0), caption, 16.0, CAPTION_TEXT_COLOR, TextAlign::left);
	panelId
}

//==============================================================================
// 노드 가운데의 뷰 좌표. (흉내 낸 입력이 누를 자리)
//==============================================================================
fn calculateNodeCenter(nodeTree: &NodeTree, nodeId: NodeId) -> Vector2 {
	let contentSize = nodeTree[nodeId].getContentSize();
	nodeTree.convertNodePositionToViewPosition(nodeId, contentSize.multiplyScalar(0.5))
}

//==============================================================================
// 토스트 띄우기. (토스트 노드가 있으면)
//==============================================================================
fn showToast(nodeTree: &mut NodeTree, toastId: NodeId, messageText: &str) {
	if let Some(toast) = nodeTree.getNodeMut(toastId).and_then(|toastNode| toastNode.getComponentMut::<UIToast>()) {
		toast.show(messageText);
	}
}

//==============================================================================
// 흉내 낸 입력 하나. (그 프레임에 할 일)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputAction {
	none,
	press,
	moved,
	release,
}

//==============================================================================
// 갤러리 씬.
//==============================================================================
struct UIGallery2Scene {
	sceneBase: SceneBase,
	touchRecognizer: TouchRecognizer,
	frameCount: u32,
	elapsedSeconds: f32,
	listId: Option<NodeId>,
	listCountLabelId: Option<NodeId>,
	snapId: Option<NodeId>,
	snapIndexLabelId: Option<NodeId>,
	dropdownId: Option<NodeId>,
	chipIds: Vec<NodeId>,
	slotId: Option<NodeId>,
	lineChartId: Option<NodeId>,
	dialogButtonId: Option<NodeId>,
	toastButtonId: Option<NodeId>,
	menuButtonId: Option<NodeId>,
	dialogId: Option<NodeId>,
	toastId: Option<NodeId>,
	menuId: Option<NodeId>,
	inputPosition: Vector2,
	dragStartPosition: Vector2,
}

impl UIGallery2Scene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> UIGallery2Scene {
		UIGallery2Scene {
			sceneBase: SceneBase::new(),
			touchRecognizer: TouchRecognizer::new(),
			frameCount: 0,
			elapsedSeconds: 0.0,
			listId: None,
			listCountLabelId: None,
			snapId: None,
			snapIndexLabelId: None,
			dropdownId: None,
			chipIds: Vec::new(),
			slotId: None,
			lineChartId: None,
			dialogButtonId: None,
			toastButtonId: None,
			menuButtonId: None,
			dialogId: None,
			toastId: None,
			menuId: None,
			inputPosition: Vector2::create(-100.0, -100.0),
			dragStartPosition: Vector2::zero(),
		}
	}

	//==============================================================================
	// 목록 구역. (500 줄 재활용 목록 + 세로 스크롤바 + 만든 노드 수)
	//==============================================================================
	fn createListSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = createSectionPanel(nodeTree, rootId, "listSection", Vector2::create(40.0, 90.0), Vector2::create(380.0, 380.0), "UIListView (500줄, 보이는 만큼만 노드)");
		let listWidth = 352.0;
		let listId = createBoxNode(nodeTree, panelId, "list", Vector2::create(14.0, 40.0), Vector2::create(listWidth, 290.0));
		{
			let listView = nodeTree.addComponent(listId, UIListView::new()).expect("list view");
			listView.setSpacing(4.0);
			listView.setCreateItemEvent(Box::new(move |tree: &mut NodeTree| {
				let itemId = tree.createNode("item");
				let itemNode = &mut tree[itemId];
				itemNode.setPivot(Pivot::topLeft);
				itemNode.setContentSize(Vector2::create(listWidth - 24.0, LIST_ITEM_HEIGHT));
				let itemPaint = itemNode.addComponent(Paint::createWithColor("#2a3145"));
				itemPaint.setRoundSize(6.0);
				let itemLabel = tree.addComponent(itemId, UILabel::createWithText("", 16.0)).expect("label");
				itemLabel.setTextAlign(TextAlign::left);
				itemLabel.setTextColor(LIGHT_TEXT_COLOR);
				itemId
			}));
			listView.setBindItemEvent(Box::new(|tree: &mut NodeTree, itemId: NodeId, itemIndex: usize| {
				let itemNode = &mut tree[itemId];
				let itemColor = if itemIndex.is_multiple_of(2) {
					"#2a3145"
				}
				else {
					"#323a52"
				};
				if let Some(itemPaint) = itemNode.getComponentMut::<Paint>() {
					itemPaint.setColor(itemColor);
				}
				if let Some(itemLabel) = itemNode.getComponentMut::<UILabel>() {
					let itemText = format!("   {}번 항목", itemIndex + 1);
					itemLabel.setText(&itemText);
				}
			}));
			listView.setItemCount(LIST_ITEM_COUNT);
		}
		if let Some(scrollView) = nodeTree[listId].getComponentMut::<UIScrollView>() {
			scrollView.setHorizontal(false);
			scrollView.setShowsVerticalScrollBar(true);
		}
		let listCountLabelId = createLabelNode(nodeTree, panelId, "listCount", Vector2::create(14.0, 338.0), Vector2::create(352.0, 30.0), "", 15.0, CAPTION_TEXT_COLOR, TextAlign::left);
		self.listId = Some(listId);
		self.listCountLabelId = Some(listCountLabelId);
	}

	//==============================================================================
	// 스냅 뷰 구역. (가운데로 맞춰지는 칸 다섯)
	//==============================================================================
	fn createSnapSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = createSectionPanel(nodeTree, rootId, "snapSection", Vector2::create(440.0, 90.0), Vector2::create(400.0, 230.0), "UISnapScrollView (한 번에 한 칸)");
		let snapWidth = 372.0;
		let sidePadding = (snapWidth - SNAP_PAGE_WIDTH) * 0.5;
		let snapId = createBoxNode(nodeTree, panelId, "snap", Vector2::create(14.0, 40.0), Vector2::create(snapWidth, 150.0));
		let contentId = {
			let snapView = nodeTree.addComponent(snapId, UISnapScrollView::new()).expect("snap view");
			snapView.setBackgroundColor("#161a25");
			let contentWidth = sidePadding * 2.0 + SNAP_PAGE_COUNT as f32 * (SNAP_PAGE_WIDTH + SNAP_PAGE_GAP) - SNAP_PAGE_GAP;
			snapView.setScrollContentSize(Vector2::create(contentWidth, 150.0));
			snapView.getContent().expect("content")
		};
		let pageColors = ["#ff7a7a", "#ffb35c", "#7fd38a", "#5cc8ff", "#b48cff"];
		for (pageIndex, pageColor) in pageColors.iter().enumerate() {
			let pagePosition = Vector2::create(sidePadding + pageIndex as f32 * (SNAP_PAGE_WIDTH + SNAP_PAGE_GAP), 12.0);
			let pageId = createBoxNode(nodeTree, contentId, "page", pagePosition, Vector2::create(SNAP_PAGE_WIDTH, 126.0));
			let pagePaint = nodeTree[pageId].addComponent(Paint::createWithColor(*pageColor));
			pagePaint.setRoundSize(14.0);
			let pageText = format!("{}", pageIndex + 1);
			let pageLabel = nodeTree.addComponent(pageId, UILabel::createWithText(&pageText, 48.0)).expect("label");
			pageLabel.setTextColor("#14171f");
			pageLabel.setBold(true);
		}
		let snapIndexLabelId = createLabelNode(nodeTree, panelId, "snapIndex", Vector2::create(14.0, 196.0), Vector2::create(372.0, 26.0), "", 15.0, CAPTION_TEXT_COLOR, TextAlign::left);
		self.snapId = Some(snapId);
		self.snapIndexLabelId = Some(snapIndexLabelId);
	}

	//==============================================================================
	// 드롭다운 구역. (고르면 문맥으로 글자를 바꾼다)
	//==============================================================================
	fn createDropdownSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = createSectionPanel(nodeTree, rootId, "dropdownSection", Vector2::create(40.0, 490.0), Vector2::create(380.0, 140.0), "UIDropdown");
		let selectionLabelId = createLabelNode(nodeTree, panelId, "selection", Vector2::create(14.0, 96.0), Vector2::create(352.0, 30.0), "고른 값: 어두운 테마", 15.0, CAPTION_TEXT_COLOR, TextAlign::left);
		let dropdownOptions = UIDropdownOptions {
			width: 220.0,
			height: 36.0,
			optionHeight: 32.0,
			fontSize: 16.0,
			..UIDropdownOptions::default()
		};
		let dropdownId = UIDropdown::create(nodeTree, panelId, dropdownOptions);
		nodeTree[dropdownId].setLocalPosition(Vector2::create(14.0, 48.0));
		nodeTree.withComponentMut::<UIDropdown, _>(dropdownId, |dropdown, tree| {
			dropdown.setOptionList(tree, &["어두운 테마", "밝은 테마", "시스템 설정", "고대비"]);
			dropdown.setChangedEvent(Box::new(move |dropdown: &mut UIDropdown, context: &mut ComponentContext| {
				let selectionText = format!("고른 값: {}", dropdown.getSelectedText());
				if let Some(selectionLabel) = context.getNodeTreeMut().getNodeMut(selectionLabelId).and_then(|labelNode| labelNode.getComponentMut::<UILabel>()) {
					selectionLabel.setText(&selectionText);
				}
			}));
		});
		self.dropdownId = Some(dropdownId);
	}

	//==============================================================================
	// 끌기 구역. (칸에 놓으면 눌러앉고, 밖에 놓으면 돌아가는 칩 둘)
	//==============================================================================
	fn createDraggableSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = createSectionPanel(nodeTree, rootId, "dragSection", Vector2::create(440.0, 340.0), Vector2::create(400.0, 290.0), "UIDraggable (칸에 놓으면 남고, 밖이면 돌아감)");
		let slotId = createBoxNode(nodeTree, panelId, "slot", Vector2::create(230.0, 70.0), Vector2::create(150.0, 150.0));
		let slotPaint = nodeTree[slotId].addComponent(Paint::createWithColor("#262d40"));
		slotPaint.setRoundSize(16.0);
		createLabelNode(nodeTree, slotId, "slotCaption", Vector2::create(0.0, 110.0), Vector2::create(150.0, 30.0), "놓는 칸", 15.0, CAPTION_TEXT_COLOR, TextAlign::center);
		let chipColors = ["#ffb35c", "#7fd38a"];
		for (chipIndex, chipColor) in chipColors.iter().enumerate() {
			let chipPosition = Vector2::create(70.0, 90.0 + chipIndex as f32 * 100.0);
			let chipId = nodeTree.createChild(panelId, "chip");
			let chipNode = &mut nodeTree[chipId];
			chipNode.setLocalPosition(chipPosition);
			chipNode.setContentSize(Vector2::create(96.0, 60.0));
			let chipPaint = chipNode.addComponent(Paint::createWithColor(*chipColor));
			chipPaint.setRoundSize(12.0);
			let chipText = format!("칩 {}", chipIndex + 1);
			let chipLabel = nodeTree.addComponent(chipId, UILabel::createWithText(&chipText, 18.0)).expect("label");
			chipLabel.setTextColor("#14171f");
			chipLabel.setBold(true);
			let draggable = nodeTree.addComponent(chipId, UIDraggable::new()).expect("draggable");
			draggable.setSnapBackEnabled(true);
			draggable.setDragEndEvent(Box::new(move |draggable: &mut UIDraggable, context: &mut ComponentContext, viewPosition: Vector2| {
				if isViewPositionInsideNode(context.getNodeTree(), slotId, viewPosition) {
					draggable.settleHere(context.getNodeTreeMut());
				}
			}));
			self.chipIds.push(chipId);
		}
		self.slotId = Some(slotId);
	}

	//==============================================================================
	// 차트 / 스피너 / 단추 구역.
	//==============================================================================
	fn createChartSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = createSectionPanel(nodeTree, rootId, "chartSection", Vector2::create(860.0, 90.0), Vector2::create(380.0, 540.0), "UILineChart / UIBarChart / UISpinner");

		// 실시간 라인 차트.
		let lineChartId = createBoxNode(nodeTree, panelId, "lineChart", Vector2::create(14.0, 40.0), Vector2::create(352.0, 120.0));
		nodeTree[lineChartId].addComponent(Paint::createWithColor("#161a25"));
		let lineChart = nodeTree[lineChartId].addComponent(UILineChart::new());
		lineChart.setMaxSampleCount(90);
		lineChart.setLineColor(ACCENT_COLOR);
		lineChart.setLineWidth(2.0);
		lineChart.setValueRange(Some(0.0), Some(260.0));
		let historyValues: Vec<f32> = (0..60).map(|sampleIndex| 130.0 + (sampleIndex as f32 * 0.35).sin() * 60.0 + ((sampleIndex * 37) % 23) as f32).collect();
		lineChart.setValueList(&historyValues);

		// 바 차트. (마지막 막대 강조)
		let barChartId = createBoxNode(nodeTree, panelId, "barChart", Vector2::create(14.0, 176.0), Vector2::create(352.0, 110.0));
		let barChart = nodeTree[barChartId].addComponent(UIBarChart::new());
		let barValues: Vec<f32> = (0..12).map(|dayIndex| 2400.0 + ((dayIndex * 1733) % 2800) as f32).collect();
		barChart.setValueList(&barValues);
		barChart.setBarColor(Color::create(0.357, 0.424, 1.0, 0.45));
		barChart.setHighlightColor(ACCENT_COLOR);
		barChart.setHighlightIndex(Some(11));

		// 스피너 둘.
		createLabelNode(nodeTree, panelId, "spinnerCaption", Vector2::create(14.0, 300.0), Vector2::create(200.0, 30.0), "UISpinner", 15.0, CAPTION_TEXT_COLOR, TextAlign::left);
		let smallSpinnerId = UISpinner::create(nodeTree, panelId, UISpinnerOptions {
			radius: 10.0,
			dotRadius: 2.2,
			color: Color::from(CAPTION_TEXT_COLOR),
			..UISpinnerOptions::default()
		});
		nodeTree[smallSpinnerId].setLocalPosition(Vector2::create(130.0, 315.0));
		let largeSpinnerId = UISpinner::create(nodeTree, panelId, UISpinnerOptions {
			radius: 18.0,
			dotCount: 12,
			dotRadius: 3.2,
			color: Color::from(ACCENT_COLOR),
			rotationSpeed: 0.8,
		});
		nodeTree[largeSpinnerId].setLocalPosition(Vector2::create(190.0, 315.0));

		// 대화 상자 / 토스트 / 메뉴 단추. (누름 처리는 겹 위젯을 만든 뒤 bindOverlayButtons 에서 건다)
		let dialogButtonId = createTextButtonNode(nodeTree, panelId, "dialogButton", Vector2::create(14.0, 360.0), Vector2::create(352.0, 46.0), "대화 상자 열기");
		let toastButtonId = createTextButtonNode(nodeTree, panelId, "toastButton", Vector2::create(14.0, 418.0), Vector2::create(352.0, 46.0), "토스트 두 개 띄우기");
		let menuButtonId = createTextButtonNode(nodeTree, panelId, "menuButton", Vector2::create(14.0, 476.0), Vector2::create(352.0, 46.0), "컨텍스트 메뉴 열기");
		self.lineChartId = Some(lineChartId);
		self.dialogButtonId = Some(dialogButtonId);
		self.toastButtonId = Some(toastButtonId);
		self.menuButtonId = Some(menuButtonId);
	}

	//==============================================================================
	// 겹 위젯. (뿌리의 마지막 자식들: 대화 상자, 컨텍스트 메뉴, 토스트)
	//==============================================================================
	fn createOverlays(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let dialogId = UIDialog::create(nodeTree, rootId, UIDialogOptions::default());
		let menuId = UIContextMenu::create(nodeTree, rootId, UIContextMenuOptions {
			menuWidth: 200.0,
			itemHeight: 34.0,
			fontSize: 16.0,
			..UIContextMenuOptions::default()
		});
		// 줄 선 토스트가 화면 저장 프레임 안에 차례로 보이도록 머무는 시간을 줄인다.
		let toastId = UIToast::create(nodeTree, rootId, UIToastOptions {
			restY: 660.0,
			holdSeconds: 0.8,
			fontSize: 17.0,
			backgroundColor: Color::create(0.24, 0.28, 0.4, 0.95),
			..UIToastOptions::default()
		});
		self.dialogId = Some(dialogId);
		self.menuId = Some(menuId);
		self.toastId = Some(toastId);
	}

	//==============================================================================
	// 단추 누름 처리 걸기. (대화 상자 열기, 토스트 띄우기, 메뉴 열기)
	//==============================================================================
	fn bindOverlayButtons(&mut self) {
		let (dialogId, menuId, toastId) = match (self.dialogId, self.menuId, self.toastId) {
			(Some(dialogId), Some(menuId), Some(toastId)) => {
				(dialogId, menuId, toastId)
			}
			_ => {
				return;
			}
		};
		let nodeTree = self.sceneBase.getNodeTreeMut();
		if let Some(dialogButton) = self.dialogButtonId.and_then(|buttonId| nodeTree[buttonId].getComponentMut::<UIButton>()) {
			dialogButton.setClickedEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				let showOptions = UIDialogShowOptions {
					onConfirm: Some(Box::new(move |_dialog: &mut UIDialog, dialogContext: &mut ComponentContext| {
						showToast(dialogContext.getNodeTreeMut(), toastId, "확인을 눌렀어요");
					})),
					onCancel: Some(Box::new(move |_dialog: &mut UIDialog, dialogContext: &mut ComponentContext| {
						showToast(dialogContext.getNodeTreeMut(), toastId, "취소했어요");
					})),
					confirmText: Some(String::from("지우기")),
					..UIDialogShowOptions::default()
				};
				context.getNodeTreeMut().withComponentMut::<UIDialog, _>(dialogId, |dialog, tree| {
					dialog.show(tree, "이 항목을 정말 지울까요?\n지운 뒤에는 되돌릴 수 없어요.", showOptions);
				});
			}));
		}
		if let Some(toastButton) = self.toastButtonId.and_then(|buttonId| nodeTree[buttonId].getComponentMut::<UIButton>()) {
			toastButton.setClickedEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				showToast(context.getNodeTreeMut(), toastId, "저장했어요");
				showToast(context.getNodeTreeMut(), toastId, "두 번째 알림은 줄을 서요");
			}));
		}
		if let Some(menuButton) = self.menuButtonId.and_then(|buttonId| nodeTree[buttonId].getComponentMut::<UIButton>()) {
			menuButton.setClickedEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				let menuPosition = context.getEngine().getInputManager().getViewInputPosition();
				let itemList = vec![
					UIContextMenuItem::create("  복사", Box::new(move |itemContext: &mut ComponentContext| {
						showToast(itemContext.getNodeTreeMut(), toastId, "복사했어요");
					})),
					UIContextMenuItem::create("  붙여넣기", Box::new(move |itemContext: &mut ComponentContext| {
						showToast(itemContext.getNodeTreeMut(), toastId, "붙여넣었어요");
					})),
					UIContextMenuItem::createDisabled("  지우기 (쓸 수 없음)"),
				];
				context.getNodeTreeMut().withComponentMut::<UIContextMenu, _>(menuId, |contextMenu, tree| {
					contextMenu.show(tree, menuPosition, itemList);
				});
			}));
		}
	}

	//==============================================================================
	// 흉내 낸 입력. (정해 둔 프레임에 누름 / 이동 / 뗌 / 휠)
	//==============================================================================
	fn simulateInput(&mut self, engine: &mut Engine) {
		let frameCount = self.frameCount;
		let nodeTree = self.sceneBase.getNodeTree();
		let mut inputAction = InputAction::none;
		let mut wheelDeltaY = 0.0;

		// 목록 휠.
		if frameCount == 8 {
			if let Some(listId) = self.listId {
				self.inputPosition = calculateNodeCenter(nodeTree, listId);
			}
		}
		if frameCount == 10 {
			wheelDeltaY = 1500.0;
		}

		// 스냅 뷰를 왼쪽으로 200 픽셀 끈다.
		if frameCount == 15 {
			if let Some(snapId) = self.snapId {
				self.dragStartPosition = calculateNodeCenter(nodeTree, snapId).add(Vector2::create(100.0, 0.0));
				self.inputPosition = self.dragStartPosition;
				inputAction = InputAction::press;
			}
		}
		if (16..=25).contains(&frameCount) {
			let dragFrameCount = (frameCount - 15) as f32;
			self.inputPosition = self.dragStartPosition.add(Vector2::create(-20.0 * dragFrameCount, 0.0));
			inputAction = InputAction::moved;
		}
		if frameCount == 28 {
			inputAction = InputAction::release;
		}

		// 드롭다운을 열고 셋째 줄을 고른다.
		let dropdownOptionIds: Vec<NodeId> = self.dropdownId.and_then(|dropdownId| nodeTree[dropdownId].getComponent::<UIDropdown>()).map(|dropdown| dropdown.getOptionNodes().to_vec()).unwrap_or_default();
		if frameCount == 34 || frameCount == 84 {
			if let Some(dropdownId) = self.dropdownId {
				self.inputPosition = calculateNodeCenter(nodeTree, dropdownId);
				inputAction = InputAction::press;
			}
		}
		if frameCount == 36 || frameCount == 86 {
			inputAction = InputAction::release;
		}
		if frameCount == 42 || frameCount == 106 {
			let optionIndex = if frameCount == 42 {
				2
			}
			else {
				1
			};
			if let Some(optionId) = dropdownOptionIds.get(optionIndex) {
				self.inputPosition = calculateNodeCenter(nodeTree, *optionId);
				inputAction = InputAction::press;
			}
		}
		if frameCount == 44 || frameCount == 108 {
			inputAction = InputAction::release;
		}

		// 토스트 단추.
		if frameCount == 48 {
			if let Some(toastButtonId) = self.toastButtonId {
				self.inputPosition = calculateNodeCenter(nodeTree, toastButtonId);
				inputAction = InputAction::press;
			}
		}
		if frameCount == 50 {
			inputAction = InputAction::release;
		}

		// 칩 1 은 칸 밖에, 칩 2 는 칸 안에 놓는다.
		for (chipIndex, startFrame) in [56u32, 70u32].iter().copied().enumerate() {
			if frameCount == startFrame {
				if let Some(chipId) = self.chipIds.get(chipIndex) {
					self.dragStartPosition = calculateNodeCenter(nodeTree, *chipId);
					self.inputPosition = self.dragStartPosition;
					inputAction = InputAction::press;
				}
			}
			if frameCount > startFrame && frameCount <= startFrame + 8 {
				let dragFrameCount = (frameCount - startFrame) as f32;
				let dragStep = if chipIndex == 0 {
					Vector2::create(6.0, -4.0)
				}
				else {
					Vector2::create(22.0, -8.0)
				};
				self.inputPosition = self.dragStartPosition.add(dragStep.multiplyScalar(dragFrameCount));
				inputAction = InputAction::moved;
			}
			if frameCount == startFrame + 10 {
				inputAction = InputAction::release;
			}
		}

		// 메뉴 단추. (메뉴는 누른 자리에 뜬다. 90 프레임 화면에 메뉴와 펼친 드롭다운이 함께 보인다)
		if frameCount == 87 {
			if let Some(menuButtonId) = self.menuButtonId {
				self.inputPosition = calculateNodeCenter(nodeTree, menuButtonId).add(Vector2::create(-120.0, 0.0));
				inputAction = InputAction::press;
			}
		}
		if frameCount == 89 {
			inputAction = InputAction::release;
		}

		// 메뉴 바깥을 눌러 닫는다.
		if frameCount == 100 {
			self.inputPosition = Vector2::create(1250.0, 700.0);
			inputAction = InputAction::press;
		}
		if frameCount == 102 {
			inputAction = InputAction::release;
		}

		// 대화 상자 단추. (180 프레임 화면에 다 열린 대화 상자와 토스트)
		if frameCount == 120 {
			if let Some(dialogButtonId) = self.dialogButtonId {
				self.inputPosition = calculateNodeCenter(nodeTree, dialogButtonId);
				inputAction = InputAction::press;
			}
		}
		if frameCount == 122 {
			inputAction = InputAction::release;
		}

		// 확인 단추.
		if frameCount == 200 {
			let confirmId = self.dialogId.and_then(|dialogId| nodeTree[dialogId].getComponent::<UIDialog>()).and_then(|dialog| dialog.getConfirmNode());
			if let Some(confirmId) = confirmId {
				self.inputPosition = calculateNodeCenter(nodeTree, confirmId);
				inputAction = InputAction::press;
			}
		}
		if frameCount == 202 {
			inputAction = InputAction::release;
		}

		let inputPosition = self.inputPosition;
		let inputManager = engine.getInputManagerMut();
		inputManager.setViewInputPosition(inputPosition);
		if wheelDeltaY != 0.0 {
			inputManager.addWheelDelta(0.0, wheelDeltaY);
		}
		match inputAction {
			InputAction::press => {
				inputManager.setTouchPressed(true);
				inputManager.setTouchDown(true);
			}
			InputAction::moved => {
				inputManager.setTouchMoved(true);
			}
			InputAction::release => {
				inputManager.setTouchReleased(true);
				inputManager.setTouchDown(false);
			}
			InputAction::none => {
			}
		}
	}

	//==============================================================================
	// 보이는 상태 글자와 실시간 차트 갱신.
	//==============================================================================
	fn updateStatus(&mut self) {
		let elapsedSeconds = self.elapsedSeconds;
		let frameCount = self.frameCount;
		let (listId, listCountLabelId, snapId, snapIndexLabelId, lineChartId, toastId) = (self.listId, self.listCountLabelId, self.snapId, self.snapIndexLabelId, self.lineChartId, self.toastId);
		let nodeTree = self.sceneBase.getNodeTreeMut();
		if let (Some(listId), Some(listCountLabelId)) = (listId, listCountLabelId) {
			let activeItemCount = nodeTree[listId].getComponent::<UIListView>().map(|listView| listView.getActiveItemCount()).unwrap_or(0);
			let contentId = nodeTree[listId].getComponent::<UIScrollView>().and_then(|scrollView| scrollView.getContent());
			let contentChildCount = contentId.map(|contentId| nodeTree.getChildCount(contentId)).unwrap_or(0);
			let countText = format!("500줄 가운데 보이는 {}줄, 만든 항목 노드 {}개", activeItemCount, contentChildCount);
			if let Some(countLabel) = nodeTree[listCountLabelId].getComponentMut::<UILabel>() {
				countLabel.setText(&countText);
			}
		}
		if let (Some(snapId), Some(snapIndexLabelId)) = (snapId, snapIndexLabelId) {
			let snapIndex = nodeTree[snapId].getComponent::<UISnapScrollView>().map(|snapView| snapView.getSnapCurrentIndex()).unwrap_or(0);
			let snapText = format!("지금 칸: {} / {}", snapIndex + 1, SNAP_PAGE_COUNT);
			if let Some(snapLabel) = nodeTree[snapIndexLabelId].getComponentMut::<UILabel>() {
				snapLabel.setText(&snapText);
			}
		}
		if let Some(lineChart) = lineChartId.and_then(|lineChartId| nodeTree[lineChartId].getComponentMut::<UILineChart>()) {
			let liveValue = 130.0 + (elapsedSeconds * 4.0).sin() * 60.0 + ((frameCount * 37) % 23) as f32;
			lineChart.pushValue(liveValue);
		}
		if frameCount == 150 {
			if let Some(toastId) = toastId {
				showToast(nodeTree, toastId, "대화 상자 위에도 토스트가 떠요");
			}
		}
	}
}

impl Scene for UIGallery2Scene {
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 생성. (위젯을 모두 놓고, 겹 위젯을 맨 뒤에 붙인다)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
		self.sceneBase.setSceneBackgroundColor(BACKGROUND_COLOR);
		let rootId = self.sceneBase.getRootId();
		self.sceneBase.getRootMut().setContentSize(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let titleId = createLabelNode(nodeTree, rootId, "title", Vector2::create(40.0, 24.0), Vector2::create(900.0, 50.0), "바닐라엔진 UI 위젯 갤러리 2", 32.0, ACCENT_COLOR, TextAlign::left);
		if let Some(titleLabel) = nodeTree[titleId].getComponentMut::<UILabel>() {
			titleLabel.setBold(true);
		}
		createLabelNode(nodeTree, rootId, "help", Vector2::create(40.0, 650.0), Vector2::create(1200.0, 40.0), "입력은 흉내 낸 것: 목록 휠 → 스냅 넘김 → 드롭다운 → 토스트 → 칩 끌기 → 메뉴 → 대화 상자 → 확인", 16.0, CAPTION_TEXT_COLOR, TextAlign::left);
		self.createListSection(rootId);
		self.createSnapSection(rootId);
		self.createDraggableSection(rootId);
		self.createDropdownSection(rootId);
		self.createChartSection(rootId);
		self.createOverlays(rootId);
		self.bindOverlayButtons();
	}

	//==============================================================================
	// 주기적 갱신. (흉내 낸 입력 → 상태 글자 → 노드 갱신 → TouchRecognizer)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, timeDelta: f32) {
		self.frameCount += 1;
		self.elapsedSeconds += timeDelta;
		if self.frameCount >= EXIT_FRAME_COUNT {
			engine.exitApplication();
			return;
		}
		self.simulateInput(engine);
		self.updateStatus();
		self.sceneBase.getNodeTreeMut().tick(engine, timeDelta);
		self.touchRecognizer.tickTouch(self.sceneBase.getNodeTreeMut(), engine);
	}

	//==============================================================================
	// 이전 출력. (배경 칠하고 뷰 좌표계 걸기)
	//==============================================================================
	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.sceneBase.drawSceneBackground(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("UI 위젯 갤러리 2 (바닐라엔진)");
	engineConfiguration.referenceResolutionSize = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engineConfiguration.viewScaleMode = ViewScaleMode::stretchShort;
	// 화면 저장 때도 프레임 간격이 일정하도록 60 FPS 로 묶는다.
	engineConfiguration.maximumFramePerSecond = 60.0;
	let engine = Engine::new(engineConfiguration);
	if let Err(error) = engine.run(Box::new(UIGallery2Scene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
