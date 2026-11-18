//==============================================================================
// UI 위젯 갤러리. (바닐라엔진 ui 모듈 예제)
// - 옮긴 위젯을 한 화면에 모두 놓는다: UIView, UILabel, UIButton, UIToggleButton, UISlider, UIProgressView, UIImageView, UIScrollView + UIScrollBar.
// - 터치는 원본 GameScene 처럼 TouchRecognizer 로 받는다. (스크롤 뷰 안의 버튼을 끌어도 스크롤된다)
// - 사람이 없어도 확인할 수 있게, 정해 둔 프레임에 입력을 흉내 낸다:
//   휠로 스크롤 → 버튼 클릭 → 슬라이더 누름 → 토글 클릭 → 스크롤 뷰 안 버튼을 위로 끌기 → 버튼을 누른 채 두기.
// - 240 프레임이 지나면 스스로 끝난다. (화면 저장: VANILLA_CAPTURE_PATH / VANILLA_CAPTURE_FRAMES / VANILLA_CAPTURE_EXIT)
//==============================================================================
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use std::cell::Cell;
use std::rc::Rc;
use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{
	Color, ComponentContext, Engine, EngineConfiguration, Graphic, Image, NodeId, NodeTree, Paint, Pivot, ProgressDirection, Scene, SceneBase, Sprite, TouchRecognizer, UIButton, UIImageView, UILabel, UIProgressView, UIScrollView, UISlider, UIToggleButton, UIView, Vector2, ViewScaleMode,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#1d1814";
const PANEL_COLOR: &str = "#2c241e";
const ACCENT_COLOR: &str = "#f3e5ab";
const DARK_TEXT_COLOR: &str = "#2b2118";
const LIGHT_TEXT_COLOR: &str = "#f1e9d8";
const CAPTION_TEXT_COLOR: &str = "#b8a690";
const SCROLL_ROW_COUNT: usize = 14;
const SCROLL_ROW_HEIGHT: f32 = 52.0;
const SCROLL_ROW_GAP: f32 = 8.0;
const EXIT_FRAME_COUNT: u32 = 240;

//==============================================================================
// 원 그림 만들기. (가운데가 밝은 둥근 무늬. 코드로 픽셀을 찍는다)
//==============================================================================
fn createDiscImage(imageSize: u32, innerColor: Color, outerColor: Color) -> Rc<Image> {
	let mut pixels = vec![0u8; (imageSize * imageSize * 4) as usize];
	let halfSize = imageSize as f32 * 0.5;
	for pixelY in 0..imageSize {
		for pixelX in 0..imageSize {
			let deltaX = (pixelX as f32 + 0.5 - halfSize) / halfSize;
			let deltaY = (pixelY as f32 + 0.5 - halfSize) / halfSize;
			let distance = (deltaX * deltaX + deltaY * deltaY).sqrt();
			let coverage = ((1.0 - distance) * halfSize).clamp(0.0, 1.0);
			if coverage <= 0.0 {
				continue;
			}
			let stripe = if ((deltaX + deltaY) * 4.0).floor() as i32 % 2 == 0 {
				1.0
			}
			else {
				0.85
			};
			let pixelColor = Color::lerp(innerColor, outerColor, distance);
			let pixelIndex = ((pixelY * imageSize + pixelX) * 4) as usize;
			pixels[pixelIndex] = (pixelColor.red * stripe * 255.0) as u8;
			pixels[pixelIndex + 1] = (pixelColor.green * stripe * 255.0) as u8;
			pixels[pixelIndex + 2] = (pixelColor.blue * stripe * 255.0) as u8;
			pixels[pixelIndex + 3] = (coverage * 255.0) as u8;
		}
	}
	Image::createFromPixels(imageSize, imageSize, pixels)
}

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
// 글자 버튼 만들기. (둥근 Paint 바탕 + UIButton + 같은 노드의 UILabel)
//==============================================================================
fn createTextButtonNode(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, fillColor: &str) -> NodeId {
	let buttonId = createBoxNode(nodeTree, parentId, name, position, size);
	let buttonPaint = nodeTree[buttonId].addComponent(Paint::createWithColor(fillColor));
	buttonPaint.setRoundSize(size.y * 0.5);
	let label = nodeTree.addComponent(buttonId, UILabel::createWithText(text, 22.0)).expect("label");
	label.setTextColor(DARK_TEXT_COLOR);
	label.setBold(true);
	nodeTree.addComponent(buttonId, UIButton::new());
	buttonId
}

//==============================================================================
// 노드 가운데의 뷰 좌표. (흉내 낸 입력이 누를 자리)
//==============================================================================
fn calculateNodeCenter(nodeTree: &NodeTree, nodeId: NodeId) -> Vector2 {
	let contentSize = nodeTree[nodeId].getContentSize();
	nodeTree.convertNodePositionToViewPosition(nodeId, contentSize.multiplyScalar(0.5))
}

//==============================================================================
// 갤러리 씬.
//==============================================================================
struct UIGalleryScene {
	sceneBase: SceneBase,
	touchRecognizer: TouchRecognizer,
	frameCount: u32,
	elapsedSeconds: f32,
	clickCount: Rc<Cell<u32>>,
	clickButtonId: Option<NodeId>,
	holdButtonId: Option<NodeId>,
	sliderId: Option<NodeId>,
	toggleId: Option<NodeId>,
	scrollId: Option<NodeId>,
	scrollRowId: Option<NodeId>,
	dragStartPosition: Option<Vector2>,
	progressId: Option<NodeId>,
	verticalProgressId: Option<NodeId>,
}

impl UIGalleryScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> UIGalleryScene {
		UIGalleryScene {
			sceneBase: SceneBase::new(),
			touchRecognizer: TouchRecognizer::new(),
			frameCount: 0,
			elapsedSeconds: 0.0,
			clickCount: Rc::new(Cell::new(0)),
			clickButtonId: None,
			holdButtonId: None,
			sliderId: None,
			toggleId: None,
			scrollId: None,
			scrollRowId: None,
			dragStartPosition: None,
			progressId: None,
			verticalProgressId: None,
		}
	}

	//==============================================================================
	// 구역 판 만들기. (배경색을 정한 UIView + 구역 이름)
	//==============================================================================
	fn createSectionPanel(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2, caption: &str) -> NodeId {
		let panelId = createBoxNode(nodeTree, parentId, name, position, size);
		let panelView = nodeTree.addComponent(panelId, UIView::new()).expect("view");
		panelView.setBackgroundColor(PANEL_COLOR);
		createLabelNode(nodeTree, panelId, "caption", Vector2::create(14.0, 6.0), Vector2::create(size.x - 28.0, 26.0), caption, 17.0, CAPTION_TEXT_COLOR, TextAlign::left);
		panelId
	}

	//==============================================================================
	// 버튼 구역. (클릭 셈, 누른 채 두는 버튼, 쓸 수 없는 그림 버튼)
	//==============================================================================
	fn createButtonSection(&mut self, rootId: NodeId) {
		let clickCount = self.clickCount.clone();
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = UIGalleryScene::createSectionPanel(nodeTree, rootId, "buttonSection", Vector2::create(40.0, 90.0), Vector2::create(380.0, 250.0), "UIButton");

		let counterLabelId = createLabelNode(nodeTree, panelId, "clickCounter", Vector2::create(210.0, 44.0), Vector2::create(160.0, 48.0), "클릭 0번", 22.0, LIGHT_TEXT_COLOR, TextAlign::left);
		let clickButtonId = createTextButtonNode(nodeTree, panelId, "clickButton", Vector2::create(20.0, 44.0), Vector2::create(180.0, 48.0), "눌러 보세요", ACCENT_COLOR);
		let clickButton = nodeTree[clickButtonId].getComponentMut::<UIButton>().expect("button");
		// 클릭 이벤트는 문맥을 받으므로, 콜백 안에서 다른 노드(셈 글자)를 바로 바꾼다.
		clickButton.setClickEvent(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
			let nextClickCount = clickCount.get() + 1;
			clickCount.set(nextClickCount);
			let counterLabel = context.getNodeTreeMut().getNodeMut(counterLabelId).and_then(|counterNode| counterNode.getComponentMut::<UILabel>());
			if let Some(counterLabel) = counterLabel {
				let counterText = format!("클릭 {}번", nextClickCount);
				counterLabel.setText(&counterText);
			}
		}));

		let holdButtonId = createTextButtonNode(nodeTree, panelId, "holdButton", Vector2::create(20.0, 108.0), Vector2::create(180.0, 48.0), "누르고 있어요", ACCENT_COLOR);
		createLabelNode(nodeTree, panelId, "holdCaption", Vector2::create(210.0, 108.0), Vector2::create(160.0, 48.0), "눌림 틴트", 18.0, CAPTION_TEXT_COLOR, TextAlign::left);

		// 쓸 수 없는 그림 버튼. (스프라이트에 어두운 덮는 색)
		let discImage = createDiscImage(96, Color::from("#9ad0ff"), Color::from("#3060c0"));
		let disabledButtonId = createBoxNode(nodeTree, panelId, "disabledButton", Vector2::create(20.0, 172.0), Vector2::create(64.0, 64.0));
		nodeTree[disabledButtonId].addComponent(Sprite::createWithImage(discImage.clone()));
		let disabledButton = nodeTree.addComponent(disabledButtonId, UIButton::new()).expect("button");
		disabledButton.setInteractable(false);
		let enabledButtonId = createBoxNode(nodeTree, panelId, "enabledButton", Vector2::create(100.0, 172.0), Vector2::create(64.0, 64.0));
		nodeTree[enabledButtonId].addComponent(Sprite::createWithImage(discImage));
		nodeTree.addComponent(enabledButtonId, UIButton::new());
		createLabelNode(nodeTree, panelId, "disabledCaption", Vector2::create(180.0, 180.0), Vector2::create(190.0, 48.0), "왼쪽: 쓸 수 없음", 18.0, CAPTION_TEXT_COLOR, TextAlign::left);

		self.clickButtonId = Some(clickButtonId);
		self.holdButtonId = Some(holdButtonId);
	}

	//==============================================================================
	// 토글 구역. (처음부터 켠 것, 흉내 낸 클릭으로 켜지는 것)
	//==============================================================================
	fn createToggleSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = UIGalleryScene::createSectionPanel(nodeTree, rootId, "toggleSection", Vector2::create(40.0, 360.0), Vector2::create(380.0, 130.0), "UIToggleButton");
		let mut toggleIds: Vec<NodeId> = Vec::new();
		for (toggleIndex, toggleText) in ["켜 둠", "클릭으로 켬", "꺼짐"].iter().enumerate() {
			let toggleId = createBoxNode(nodeTree, panelId, "toggle", Vector2::create(20.0 + toggleIndex as f32 * 118.0, 50.0), Vector2::create(106.0, 56.0));
			let togglePaint = nodeTree[toggleId].addComponent(Paint::createWithColor("#e8d9c4"));
			togglePaint.setRoundSize(12.0);
			let label = nodeTree.addComponent(toggleId, UILabel::createWithText(toggleText, 18.0)).expect("label");
			label.setTextColor(DARK_TEXT_COLOR);
			let toggleButton = nodeTree.addComponent(toggleId, UIToggleButton::new()).expect("toggle");
			toggleButton.setOnTintColor(Color::create(0.85, 0.45, 0.1, 0.6));
			// 글자까지 같은 색으로 물들면 읽기 어려워 글자는 틴트에서 뺀다.
			toggleButton.getUIButtonMut().excludeComponentFromTint::<UILabel>(toggleId);
			if toggleIndex == 0 {
				toggleButton.setIsOn(true);
			}
			toggleIds.push(toggleId);
		}
		self.toggleId = Some(toggleIds[1]);
	}

	//==============================================================================
	// 슬라이더 / 진행률 구역.
	//==============================================================================
	fn createValueSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let panelId = UIGalleryScene::createSectionPanel(nodeTree, rootId, "valueSection", Vector2::create(440.0, 90.0), Vector2::create(380.0, 400.0), "UISlider / UIProgressView");

		// 가로 슬라이더. (값이 바뀌면 문맥으로 값 글자를 바꾼다)
		let valueLabelId = createLabelNode(nodeTree, panelId, "sliderValue", Vector2::create(250.0, 48.0), Vector2::create(110.0, 40.0), "값 0.30", 20.0, LIGHT_TEXT_COLOR, TextAlign::left);
		let sliderId = createBoxNode(nodeTree, panelId, "slider", Vector2::create(30.0, 58.0), Vector2::create(200.0, 20.0));
		let slider = nodeTree.addComponent(sliderId, UISlider::new()).expect("slider");
		slider.setValue(0.3);
		slider.setTrackThickness(8.0);
		slider.setCornerRadius(4.0);
		slider.setThumbRadius(12.0);
		slider.setFillColor(ACCENT_COLOR);
		slider.setTrackColor("#5a4a3c");
		slider.setValueChangedEvent(Box::new(move |slider: &mut UISlider, context: &mut ComponentContext| {
			let valueText = format!("값 {:.2}", slider.getValue());
			if let Some(valueLabel) = context.getNodeTreeMut().getNodeMut(valueLabelId).and_then(|valueNode| valueNode.getComponentMut::<UILabel>()) {
				valueLabel.setText(&valueText);
			}
		}));

		// 손잡이 노드를 쓰는 세로 슬라이더.
		let verticalSliderId = createBoxNode(nodeTree, panelId, "verticalSlider", Vector2::create(330.0, 110.0), Vector2::create(12.0, 260.0));
		let thumbId = createBoxNode(nodeTree, verticalSliderId, "thumb", Vector2::zero(), Vector2::create(28.0, 28.0));
		let thumbPaint = nodeTree[thumbId].addComponent(Paint::createWithColor("#ff9a5a"));
		thumbPaint.setRoundSize(8.0);
		let verticalSlider = nodeTree.addComponent(verticalSliderId, UISlider::new()).expect("slider");
		verticalSlider.setDirection(ProgressDirection::vertical);
		verticalSlider.setCornerRadius(6.0);
		verticalSlider.setValue(0.65);
		verticalSlider.setThumbNode(Some(thumbId));

		// 가로 진행률. (반대편 값도 함께)
		let progressId = createBoxNode(nodeTree, panelId, "progress", Vector2::create(30.0, 120.0), Vector2::create(260.0, 24.0));
		let progressView = nodeTree.addComponent(progressId, UIProgressView::new()).expect("progress");
		progressView.setCornerRadius(12.0);
		progressView.setTrackColor("#5a4a3c");
		progressView.setFillColor("#7fd38a");
		progressView.setSecondaryValue(Some(0.2));
		createLabelNode(nodeTree, panelId, "progressCaption", Vector2::create(30.0, 150.0), Vector2::create(260.0, 24.0), "초록은 차오르고, 빨강은 반대편 값", 15.0, CAPTION_TEXT_COLOR, TextAlign::left);

		// 세로 진행률 셋.
		let mut verticalProgressId: Option<NodeId> = None;
		for (progressIndex, progressValue) in [0.25, 0.6, 0.9].iter().enumerate() {
			let barId = createBoxNode(nodeTree, panelId, "verticalProgress", Vector2::create(40.0 + progressIndex as f32 * 50.0, 190.0), Vector2::create(28.0, 180.0));
			let barView = nodeTree.addComponent(barId, UIProgressView::new()).expect("progress");
			barView.setDirection(ProgressDirection::vertical);
			barView.setCornerRadius(6.0);
			barView.setTrackColor("#5a4a3c");
			barView.setValue(*progressValue);
			if progressIndex == 0 {
				verticalProgressId = Some(barId);
			}
		}

		// 이미지 뷰 둘. (그대로, 좌우 뒤집고 색 입힘. 뒤집은 스프라이트는 원본처럼 노드 왼쪽으로 그려져 노드를 한 폭 오른쪽에 둔다)
		let imageViewImage = createDiscImage(128, Color::from("#ffe6a0"), Color::from("#d0702a"));
		let imageViewId = createBoxNode(nodeTree, panelId, "imageView", Vector2::create(190.0, 200.0), Vector2::create(110.0, 80.0));
		let imageView = nodeTree.addComponent(imageViewId, UIImageView::createWithImage(imageViewImage.clone())).expect("image view");
		imageView.setBackgroundColor(Color::create(1.0, 1.0, 1.0, 0.08));
		let tintedImageViewId = createBoxNode(nodeTree, panelId, "tintedImageView", Vector2::create(300.0, 290.0), Vector2::create(110.0, 80.0));
		let tintedImageView = nodeTree.addComponent(tintedImageViewId, UIImageView::createWithImage(imageViewImage)).expect("image view");
		tintedImageView.setHorizontalFlip(true);
		tintedImageView.setColor(Color::create(0.3, 0.6, 1.0, 0.5));

		self.sliderId = Some(sliderId);
		self.progressId = Some(progressId);
		self.verticalProgressId = verticalProgressId;
	}

	//==============================================================================
	// 스크롤 뷰 구역. (줄 버튼 목록 + 세로 스크롤바)
	//==============================================================================
	fn createScrollSection(&mut self, rootId: NodeId) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		createLabelNode(nodeTree, rootId, "scrollCaption", Vector2::create(840.0, 90.0), Vector2::create(400.0, 26.0), "UIScrollView + UIScrollBar (휠, 끌기)", 17.0, CAPTION_TEXT_COLOR, TextAlign::left);
		let scrollWidth = 400.0;
		let scrollHeight = 470.0;
		let scrollId = createBoxNode(nodeTree, rootId, "scroll", Vector2::create(840.0, 120.0), Vector2::create(scrollWidth, scrollHeight));
		let scrollContentHeight = SCROLL_ROW_GAP + SCROLL_ROW_COUNT as f32 * (SCROLL_ROW_HEIGHT + SCROLL_ROW_GAP);
		let contentId = {
			let scrollView = nodeTree.addComponent(scrollId, UIScrollView::new()).expect("scroll view");
			scrollView.setBackgroundColor(PANEL_COLOR);
			scrollView.setScrollContentSize(Vector2::create(scrollWidth, scrollContentHeight));
			scrollView.setShowsVerticalScrollBar(true);
			scrollView.setHorizontal(false);
			scrollView.getContent().expect("content")
		};
		let rowWidth = scrollWidth - 24.0 - SCROLL_ROW_GAP * 2.0;
		let mut firstRowId: Option<NodeId> = None;
		for rowIndex in 0..SCROLL_ROW_COUNT {
			let rowColor = if rowIndex % 2 == 0 {
				"#4a3c30"
			}
			else {
				"#5a4a3c"
			};
			let rowPosition = Vector2::create(SCROLL_ROW_GAP, SCROLL_ROW_GAP + rowIndex as f32 * (SCROLL_ROW_HEIGHT + SCROLL_ROW_GAP));
			let rowId = createBoxNode(nodeTree, contentId, "row", rowPosition, Vector2::create(rowWidth, SCROLL_ROW_HEIGHT));
			let rowPaint = nodeTree[rowId].addComponent(Paint::createWithColor(rowColor));
			rowPaint.setRoundSize(8.0);
			let rowText = format!("  {}번째 줄 (눌러도 되고 끌어도 돼요)", rowIndex + 1);
			let rowLabel = nodeTree.addComponent(rowId, UILabel::createWithText(&rowText, 18.0)).expect("label");
			rowLabel.setTextAlign(TextAlign::left);
			rowLabel.setTextColor(LIGHT_TEXT_COLOR);
			nodeTree.addComponent(rowId, UIButton::new());
			if rowIndex == 3 {
				firstRowId = Some(rowId);
			}
		}
		self.scrollId = Some(scrollId);
		self.scrollRowId = firstRowId;
	}

	//==============================================================================
	// 흉내 낸 입력. (정해 둔 프레임에 누름 / 이동 / 뗌 / 휠)
	//==============================================================================
	fn simulateInput(&mut self, engine: &mut Engine) {
		let frameCount = self.frameCount;
		let nodeTree = self.sceneBase.getNodeTree();
		let scrollCenter = self.scrollId.map(|scrollId| calculateNodeCenter(nodeTree, scrollId));
		let clickButtonCenter = self.clickButtonId.map(|clickButtonId| calculateNodeCenter(nodeTree, clickButtonId));
		let holdButtonCenter = self.holdButtonId.map(|holdButtonId| calculateNodeCenter(nodeTree, holdButtonId));
		let toggleCenter = self.toggleId.map(|toggleId| calculateNodeCenter(nodeTree, toggleId));
		let sliderPosition = self.sliderId.map(|sliderId| nodeTree.convertNodePositionToViewPosition(sliderId, Vector2::create(200.0 * 0.7, 10.0)));
		let scrollRowCenter = self.scrollRowId.map(|scrollRowId| calculateNodeCenter(nodeTree, scrollRowId));
		// 끄는 동안 줄도 따라 움직이므로, 누른 자리는 누를 때 한 번만 정한다.
		if frameCount == 58 {
			self.dragStartPosition = scrollRowCenter;
		}
		let dragStartPosition = self.dragStartPosition;

		let inputManager = engine.getInputManagerMut();
		let mut inputPosition: Option<Vector2> = None;
		if (20..30).contains(&frameCount) {
			inputPosition = scrollCenter;
			if frameCount == 20 {
				inputManager.addWheelDelta(0.0, 120.0);
			}
		}
		else if (30..40).contains(&frameCount) {
			inputPosition = clickButtonCenter;
		}
		else if (40..50).contains(&frameCount) {
			inputPosition = sliderPosition;
		}
		else if (50..58).contains(&frameCount) {
			inputPosition = toggleCenter;
		}
		else if (58..75).contains(&frameCount) {
			// 줄 버튼을 누르고 10 프레임 동안 60 픽셀 위로 끈다. (임계를 넘으면 스크롤 뷰가 가져간다)
			let dragFrameCount = frameCount.saturating_sub(59).min(10) as f32;
			inputPosition = dragStartPosition.map(|dragStartPosition| Vector2::create(dragStartPosition.x, dragStartPosition.y - dragFrameCount * 6.0));
		}
		else if frameCount >= 75 {
			inputPosition = holdButtonCenter;
		}
		if let Some(inputPosition) = inputPosition {
			inputManager.setViewInputPosition(inputPosition);
		}
		else {
			inputManager.setViewInputPosition(Vector2::create(-100.0, -100.0));
		}

		let pressFrames = [30, 40, 50, 58, 75];
		let releaseFrames = [33, 43, 53, 70];
		if pressFrames.contains(&frameCount) {
			inputManager.setTouchPressed(true);
			inputManager.setTouchDown(true);
		}
		if (59..=69).contains(&frameCount) {
			inputManager.setTouchMoved(true);
		}
		if releaseFrames.contains(&frameCount) {
			inputManager.setTouchReleased(true);
			inputManager.setTouchDown(false);
		}
	}

	//==============================================================================
	// 진행률을 시간에 따라 움직인다.
	//==============================================================================
	fn animateProgress(&mut self) {
		let progressValue = 0.15 + 0.5 * (0.5 + 0.5 * (self.elapsedSeconds * 1.5).sin());
		let progressId = self.progressId;
		let verticalProgressId = self.verticalProgressId;
		let nodeTree = self.sceneBase.getNodeTreeMut();
		if let Some(progressView) = progressId.and_then(|progressId| nodeTree.getNodeMut(progressId)).and_then(|progressNode| progressNode.getComponentMut::<UIProgressView>()) {
			progressView.setValue(progressValue);
		}
		if let Some(progressView) = verticalProgressId.and_then(|progressId| nodeTree.getNodeMut(progressId)).and_then(|progressNode| progressNode.getComponentMut::<UIProgressView>()) {
			progressView.setValue(1.0 - progressValue);
		}
	}
}

impl Scene for UIGalleryScene {
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 생성. (위젯을 모두 놓는다)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
		self.sceneBase.setSceneBackgroundColor(BACKGROUND_COLOR);
		let rootId = self.sceneBase.getRootId();
		self.sceneBase.getRootMut().setContentSize(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let titleId = createLabelNode(nodeTree, rootId, "title", Vector2::create(40.0, 24.0), Vector2::create(800.0, 50.0), "바닐라엔진 UI 위젯 갤러리", 34.0, ACCENT_COLOR, TextAlign::left);
		if let Some(titleLabel) = nodeTree[titleId].getComponentMut::<UILabel>() {
			titleLabel.setBold(true);
		}
		createLabelNode(nodeTree, rootId, "help", Vector2::create(40.0, 650.0), Vector2::create(1200.0, 40.0), "입력은 흉내 낸 것: 휠 스크롤 → 버튼 클릭 → 슬라이더 70% → 토글 클릭 → 목록 끌기 → 버튼 누른 채", 17.0, CAPTION_TEXT_COLOR, TextAlign::left);
		self.createButtonSection(rootId);
		self.createToggleSection(rootId);
		self.createValueSection(rootId);
		self.createScrollSection(rootId);
	}

	//==============================================================================
	// 주기적 갱신. (흉내 낸 입력 → 노드 갱신 → TouchRecognizer)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, timeDelta: f32) {
		self.frameCount += 1;
		self.elapsedSeconds += timeDelta;
		if self.frameCount >= EXIT_FRAME_COUNT {
			engine.exitApplication();
			return;
		}
		self.simulateInput(engine);
		self.animateProgress();
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
	engineConfiguration.title = String::from("UI 위젯 갤러리 (바닐라엔진)");
	engineConfiguration.referenceResolutionSize = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engineConfiguration.viewScaleMode = ViewScaleMode::stretchShort;
	// 화면 저장 때도 프레임 간격이 일정하도록 60 FPS 로 묶는다. (틴트 트랜지션이 프레임 수와 맞게)
	engineConfiguration.maximumFramePerSecond = 60.0;
	let engine = Engine::new(engineConfiguration);
	if let Err(error) = engine.run(Box::new(UIGalleryScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
