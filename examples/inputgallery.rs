//==============================================================================
// 입력 필드 갤러리. (바닐라엔진 UIInputField, RichText 예제)
// - 입력 필드 넷: 포커스를 받아 한글을 치는 칸, 안내 글만 있는 칸, 선택 영역이 남은 칸, 캔버스 모드(useDOMInput 꺼짐) 칸.
// - 리치텍스트 라벨 셋: 굵게, 색, 크기, 밑줄, 취소선, 기울임, 모르는 태그. 정렬을 달리한다.
// - 사람이 없어도 확인할 수 있게, 정해 둔 프레임에 입력을 흉내 낸다. (엔진 글 입력 줄에 직접 넣는다)
//   5 프레임: 첫 칸을 누름 → 12, 16: 확정 글 "안녕하세요 ", "vanilla" → 30 ~ 36: 조합 "ㅎ" → "하" → "한" (밑줄과 커서)
//   130: 셋째 칸에 오른쪽 단추(문맥 메뉴) → 140 부터: 메뉴의 "복사" 위에 마우스.
// - 200 프레임이 지나면 스스로 끝난다. (화면 저장: VANILLA_CAPTURE_PATH / VANILLA_CAPTURE_FRAMES / VANILLA_CAPTURE_EXIT)
//==============================================================================
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{Engine, EngineConfiguration, Graphic, NodeId, NodeTree, Paint, Pivot, Scene, SceneBase, TextInputEventKind, UIInputField, UILabel, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#e9edf3";
const PANEL_COLOR: &str = "#ffffff";
const TITLE_COLOR: &str = "#1f2937";
const CAPTION_COLOR: &str = "#6b7280";
const ACCENT_COLOR: &str = "#2563eb";
const FIELD_WIDTH: f32 = 520.0;
const FIELD_HEIGHT: f32 = 64.0;
const EXIT_FRAME_COUNT: u32 = 200;

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
fn createLabelNode(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, fontSize: f32, color: &str) -> NodeId {
	let labelId = createBoxNode(nodeTree, parentId, name, position, size);
	let label = nodeTree.addComponent(labelId, UILabel::createWithText(text, fontSize)).expect("label");
	label.setTextColor(color);
	label.setTextAlign(TextAlign::left);
	label.setTextBaseline(TextBaseline::middle);
	labelId
}

//==============================================================================
// 입력 필드 노드 만들기. (위에 설명 글, DOM 입력 여부, attach)
//==============================================================================
fn createFieldNode(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, caption: &str, useDOMInput: bool) -> NodeId {
	createLabelNode(nodeTree, parentId, "caption", Vector2::create(position.x, position.y - 28.0), Vector2::create(FIELD_WIDTH, 24.0), caption, 16.0, CAPTION_COLOR);
	let fieldId = createBoxNode(nodeTree, parentId, name, position, Vector2::create(FIELD_WIDTH, FIELD_HEIGHT));
	let field = nodeTree.addComponent(fieldId, UIInputField::new()).expect("field");
	field.setUseDOMInput(useDOMInput);
	field.setBackgroundColor("#f8fafc");
	field.setCursorColor(ACCENT_COLOR);
	field.setFocusBorderColor(ACCENT_COLOR);
	field.setFocusBorderWidth(2.0);
	field.setRoundSize(12.0);
	field.attach();
	fieldId
}

//==============================================================================
// 리치텍스트 라벨 만들기.
//==============================================================================
fn createRichLabelNode(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, size: Vector2, markup: &str, textAlign: TextAlign) -> NodeId {
	let labelId = createBoxNode(nodeTree, parentId, name, position, size);
	nodeTree[labelId].addComponent(Paint::createWithColor("#f1f5f9")).setRoundSize(10.0);
	let label = nodeTree.addComponent(labelId, UILabel::createWithText(markup, 26.0)).expect("label");
	label.useRichText(true);
	label.setTextColor(TITLE_COLOR);
	label.setTextAlign(textAlign);
	label.setTextBaseline(TextBaseline::middle);
	labelId
}

//==============================================================================
// 노드 안의 점 → 뷰 좌표.
//==============================================================================
fn calculateNodePoint(nodeTree: &NodeTree, nodeId: NodeId, localPosition: Vector2) -> Vector2 {
	nodeTree.convertNodePositionToViewPosition(nodeId, localPosition)
}

//==============================================================================
// 입력 필드 갤러리 씬.
//==============================================================================
struct InputGalleryScene {
	sceneBase: SceneBase,
	frameCount: u32,
	typingFieldId: Option<NodeId>,
	selectionFieldId: Option<NodeId>,
}

impl InputGalleryScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> InputGalleryScene {
		InputGalleryScene {
			sceneBase: SceneBase::new(),
			frameCount: 0,
			typingFieldId: None,
			selectionFieldId: None,
		}
	}

	//==============================================================================
	// 흉내 낸 입력. (누름, 확정 글, 조합 글, 문맥 메뉴)
	//==============================================================================
	fn simulateInput(&mut self, engine: &mut Engine) {
		let frameCount = self.frameCount;
		let nodeTree = self.sceneBase.getNodeTree();
		let typingPoint = self.typingFieldId.map(|fieldId| calculateNodePoint(nodeTree, fieldId, Vector2::create(FIELD_WIDTH - 40.0, FIELD_HEIGHT * 0.5)));
		let selectionPoint = self.selectionFieldId.map(|fieldId| calculateNodePoint(nodeTree, fieldId, Vector2::create(160.0, FIELD_HEIGHT * 0.5)));

		let inputManager = engine.getInputManagerMut();
		let inputPosition = if frameCount < 20 {
			typingPoint
		}
		else if frameCount >= 140 {
			// 문맥 메뉴의 둘째 항목(복사) 위. (메뉴 왼쪽 위는 오른쪽 단추를 누른 자리)
			selectionPoint.map(|point| Vector2::create(point.x + 40.0, point.y + 4.0 + 34.0 * 1.5))
		}
		else {
			None
		};
		inputManager.setViewInputPosition(inputPosition.unwrap_or(Vector2::create(-100.0, -100.0)));
		if frameCount == 5 {
			inputManager.setTouchPressed(true);
			inputManager.setTouchDown(true);
		}
		if frameCount == 7 {
			inputManager.setTouchReleased(true);
			inputManager.setTouchDown(false);
		}

		let textInput = inputManager.getTextInputMut();
		match frameCount {
			12 => {
				textInput.pushFocusedFieldEvent(TextInputEventKind::commit(String::from("안녕하세요 ")));
			}
			16 => {
				textInput.pushFocusedFieldEvent(TextInputEventKind::commit(String::from("vanilla")));
			}
			30 => {
				textInput.pushFocusedFieldEvent(TextInputEventKind::compositionStart);
				textInput.pushFocusedFieldEvent(TextInputEventKind::preedit {
					text: String::from("ㅎ"),
					cursorIndex: Some(1),
				});
			}
			33 => {
				textInput.pushFocusedFieldEvent(TextInputEventKind::preedit {
					text: String::from("하"),
					cursorIndex: Some(1),
				});
			}
			36 => {
				textInput.pushFocusedFieldEvent(TextInputEventKind::preedit {
					text: String::from("한"),
					cursorIndex: Some(1),
				});
			}
			130 => {
				textInput.setContextMenuRequestPosition(selectionPoint);
			}
			_ => {
			}
		}
	}
}

impl Scene for InputGalleryScene {
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 생성. (필드와 리치텍스트 라벨을 놓는다)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
		self.sceneBase.setSceneBackgroundColor(BACKGROUND_COLOR);
		let rootId = self.sceneBase.getRootId();
		self.sceneBase.getRootMut().setContentSize(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let titleId = createLabelNode(nodeTree, rootId, "title", Vector2::create(48.0, 24.0), Vector2::create(900.0, 50.0), "바닐라엔진 입력 필드와 리치텍스트", 32.0, TITLE_COLOR);
		if let Some(titleLabel) = nodeTree[titleId].getComponentMut::<UILabel>() {
			titleLabel.setBold(true);
		}
		let leftPanelId = createBoxNode(nodeTree, rootId, "leftPanel", Vector2::create(32.0, 96.0), Vector2::create(584.0, 580.0));
		nodeTree[leftPanelId].addComponent(Paint::createWithColor(PANEL_COLOR)).setRoundSize(16.0);
		let rightPanelId = createBoxNode(nodeTree, rootId, "rightPanel", Vector2::create(648.0, 96.0), Vector2::create(600.0, 580.0));
		nodeTree[rightPanelId].addComponent(Paint::createWithColor(PANEL_COLOR)).setRoundSize(16.0);

		// 입력 필드.
		let typingFieldId = createFieldNode(nodeTree, leftPanelId, "typingField", Vector2::create(32.0, 60.0), "포커스 + 확정 글 + 조합 중인 글 (useDOMInput, maxLength 20)", true);
		if let Some(field) = nodeTree[typingFieldId].getComponentMut::<UIInputField>() {
			field.setPlaceholder("닉네임");
			field.setMaxLength(20);
		}
		let placeholderFieldId = createFieldNode(nodeTree, leftPanelId, "placeholderField", Vector2::create(32.0, 196.0), "빈 칸 (안내 글)", true);
		if let Some(field) = nodeTree[placeholderFieldId].getComponentMut::<UIInputField>() {
			field.setPlaceholder("여기에 입력하세요");
		}
		let selectionFieldId = createFieldNode(nodeTree, leftPanelId, "selectionField", Vector2::create(32.0, 332.0), "선택 영역 (오른쪽 단추 → 문맥 메뉴)", true);
		if let Some(field) = nodeTree[selectionFieldId].getComponentMut::<UIInputField>() {
			field.setText("선택된 글자 영역입니다");
			field.moveCursor(3, false);
			field.moveCursor(8, true);
		}
		let canvasFieldId = createFieldNode(nodeTree, leftPanelId, "canvasField", Vector2::create(32.0, 468.0), "캔버스 모드 (useDOMInput 꺼짐, 색 바꿈)", false);
		if let Some(field) = nodeTree[canvasFieldId].getComponentMut::<UIInputField>() {
			field.setText("canvas mode ABC 123");
			field.setBackgroundColor("#1f2937");
			field.setTextColor("#f9fafb");
			field.setFontSize(24.0);
			field.setRoundSize(24.0);
			field.setPadding(24.0);
		}
		self.typingFieldId = Some(typingFieldId);
		self.selectionFieldId = Some(selectionFieldId);

		// 리치텍스트 라벨.
		createLabelNode(nodeTree, rightPanelId, "richCaption", Vector2::create(32.0, 24.0), Vector2::create(536.0, 24.0), "RichText (UILabel.useRichText)", 16.0, CAPTION_COLOR);
		createRichLabelNode(nodeTree, rightPanelId, "richLeft", Vector2::create(32.0, 60.0), Vector2::create(536.0, 80.0), "<b>굵게</b> 보통 <color=#ef4444>빨강</color> <size=40>크게</size> 끝", TextAlign::left);
		createRichLabelNode(nodeTree, rightPanelId, "richCenter", Vector2::create(32.0, 160.0), Vector2::create(536.0, 80.0), "<u>밑줄</u> <s>취소선</s> <color=teal><i>기울임</i></color> <color=#2563eb80>반투명</color>", TextAlign::center);
		createRichLabelNode(nodeTree, rightPanelId, "richRight", Vector2::create(32.0, 260.0), Vector2::create(536.0, 80.0), "<unknown>모르는 태그</unknown> <size=18>작게</size> <b><color=orange>굵은 주황</color></b>", TextAlign::right);
		createRichLabelNode(nodeTree, rightPanelId, "richPlain", Vector2::create(32.0, 360.0), Vector2::create(536.0, 80.0), "마크업 없는 글은 Text 로 그린다", TextAlign::center);
	}

	//==============================================================================
	// 주기적 갱신. (흉내 낸 입력 → 노드 갱신과 터치)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, timeDelta: f32) {
		self.frameCount += 1;
		if self.frameCount >= EXIT_FRAME_COUNT {
			engine.exitApplication();
			return;
		}
		self.simulateInput(engine);
		self.sceneBase.tick(engine, timeDelta);
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
	engineConfiguration.title = String::from("입력 필드 갤러리 (바닐라엔진)");
	engineConfiguration.referenceResolutionSize = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engineConfiguration.viewScaleMode = ViewScaleMode::stretchShort;
	// 화면 저장 때도 프레임 간격이 일정하도록 60 FPS 로 묶는다. (커서 깜빡임이 프레임 수와 맞게)
	engineConfiguration.maximumFramePerSecond = 60.0;
	let engine = Engine::new(engineConfiguration);
	if let Err(error) = engine.run(Box::new(InputGalleryScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
