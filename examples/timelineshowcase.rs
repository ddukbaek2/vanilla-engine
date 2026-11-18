//==============================================================================
// 타임라인 쇼케이스 예제. (Timeline + GameSceneBase 눈으로 확인)
// - 원본 examples/timelineshowcase 의 문서 8 개(*.timeline.json)와 그림을 그대로 읽어 무대를 만들고 차례로 튼다.
//   애셋은 원본 저장소 폴더에서 읽기만 한다. (modelviewer 예제와 같은 방식)
// - 씬은 GameSceneBase 를 품는다: 로딩 화면, 세이프 에어리어 노드, 뷰 크기가 바뀌면 다시 놓기, 배경 칠하기.
// - "screen" 트랙은 external 대상이라 tick 뒤에 applyExternalProperties 로 ScreenEffect 에 넘긴다.
// - 무대 오른쪽 아래에 HeatHazeEffect 노드(아지랑이), 머리 오른쪽에 Indicator(돌아가는 톱니)를 같이 띄운다.
// - 화면 저장이 같은 모양으로 나오게 타임라인은 프레임마다 1/60 초씩 흘린다. 150 프레임마다 다음 문서로, 누르면 바로 다음으로 넘어간다.
// - 실행: cargo run --release --example timelineshowcase
//   화면 저장: VANILLA_CAPTURE_PATH=shots/timeline_{frame}.png VANILLA_CAPTURE_FRAMES=90,240,390 VANILLA_CAPTURE_EXIT=1
//==============================================================================
#![allow(non_snake_case)]

use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::core::graphic::TextAlign;
use vanilla::core::component::mask::Mask;
use vanilla::{Color, HeatHazeEffect, Indicator, Engine, EngineConfiguration, GameSceneBase, Graphic, Image, ImageAsset, JsonAsset, NodeId, NodeTree, Paint, Pivot, Scene, SceneBase, ScreenEffect, Text, Timeline, TimelineKey, TimelineMarker, TimelineTarget, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/timelineshowcase/assets";
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const DOCUMENT_WIDTH: f32 = 960.0;
const DOCUMENT_HEIGHT: f32 = 540.0;
const HEADER_HEIGHT: f32 = 64.0;
const MODE_FRAME_COUNT: u64 = 150;
const TIMELINE_FRAME_STEP: f32 = 1.0 / 60.0;
const IMAGE_NAME_LIST: [&str; 7] = ["emblem.png", "slime.png", "star.png", "coin.png", "bat.png", "gem.png", "cover.png"];
// (문서 파일, 제목) 목록. (원본 MODE_DEFINITIONS 차례)
const MODE_DEFINITIONS: [(&str, &str); 8] = [
	("title.timeline.json", "TITLE REVEAL"),
	("logo.timeline.json", "LOGO STING"),
	("hud.timeline.json", "HUD INTRO"),
	("cutscene.timeline.json", "CUTSCENE"),
	("combo.timeline.json", "COMBO POPUP"),
	("transition.timeline.json", "SCENE TRANSITION"),
	("idle.timeline.json", "LOOPING IDLE"),
	("parallax.timeline.json", "PARALLAX FLYBY"),
];

//==============================================================================
// 모드 하나. (무대 뿌리 노드, 타임라인, 제목)
//==============================================================================
struct TimelineMode {
	rootId: NodeId,
	timeline: Timeline,
	title: String,
}

//==============================================================================
// 타임라인 쇼케이스 씬.
//==============================================================================
struct TimelineShowcaseScene {
	gameSceneBase: GameSceneBase,
	timelineAssetList: Vec<JsonAsset>,
	imageAssetList: Vec<(String, ImageAsset)>,
	modeList: Vec<TimelineMode>,
	currentModeIndex: usize,
	modeFrameCount: u64,
	screenEffect: ScreenEffect,
	stageId: Option<NodeId>,
	titleLabelId: Option<NodeId>,
	statusLabelId: Option<NodeId>,
	// 마지막 마커 / 이벤트. (핸들러가 적고 상태 줄이 읽는다)
	lastSignal: Rc<RefCell<String>>,
	indicator: Indicator,
}

impl TimelineShowcaseScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> TimelineShowcaseScene {
		TimelineShowcaseScene {
			gameSceneBase: GameSceneBase::new(),
			timelineAssetList: Vec::new(),
			imageAssetList: Vec::new(),
			modeList: Vec::new(),
			currentModeIndex: 0,
			modeFrameCount: 0,
			screenEffect: ScreenEffect::new(),
			stageId: None,
			titleLabelId: None,
			statusLabelId: None,
			lastSignal: Rc::new(RefCell::new(String::new())),
			indicator: Indicator::new(),
		}
	}

	//==============================================================================
	// 라벨 노드 만들기. (왼쪽 정렬 글)
	//==============================================================================
	fn createLabel(nodeTree: &mut NodeTree, parentId: NodeId, name: &str, position: Vector2, fontSize: f32, color: Color) -> NodeId {
		let labelId = nodeTree.createChild(parentId, name);
		let label = &mut nodeTree[labelId];
		label.setPivot(Pivot::topLeft);
		label.setAnchor(Pivot::topLeft);
		label.setLocalPosition(position);
		label.setContentSize(Vector2::create(900.0, fontSize * 1.4));
		let text = label.addComponent(Text::createWithText("", fontSize));
		text.setTextAlign(TextAlign::left);
		text.setTextColor(color);
		labelId
	}

	//==============================================================================
	// 모드 고르기. (모두 멈추고 화면 효과를 되돌린 뒤 고른 문서를 처음부터 튼다. 원본 selectMode)
	//==============================================================================
	fn selectMode(&mut self, modeIndex: usize) {
		let nodeTree = self.gameSceneBase.getSceneBaseMut().getNodeTreeMut();
		for mode in self.modeList.iter_mut() {
			mode.timeline.stop(nodeTree);
			nodeTree[mode.rootId].setActive(false);
		}
		self.screenEffect.resetAll();
		self.currentModeIndex = modeIndex;
		self.modeFrameCount = 0;
		self.lastSignal.borrow_mut().clear();
		if let Some(mode) = self.modeList.get_mut(modeIndex) {
			nodeTree[mode.rootId].setActive(true);
			mode.timeline.seek(nodeTree, 0.0);
			mode.timeline.play();
			mode.timeline.applyExternalProperties("screen", &mut self.screenEffect);
			if let Some(titleLabelId) = self.titleLabelId {
				if let Some(text) = nodeTree[titleLabelId].getComponentMut::<Text>() {
					text.setText(&format!("TIMELINE SHOWCASE  {} / {}  {}", modeIndex + 1, MODE_DEFINITIONS.len(), mode.title));
				}
			}
		}
	}

	//==============================================================================
	// 무대를 세이프 에어리어 안에 맞춰 놓는다. (씬의 layout, 원본 layout 재정의 자리)
	//==============================================================================
	fn layout(&mut self) {
		let safeAreaRect = self.gameSceneBase.getSafeAreaRect();
		let stageId = match self.stageId {
			Some(stageId) => {
				stageId
			}
			None => {
				return;
			}
		};
		let availableWidth = (safeAreaRect.size.x - 32.0).max(1.0);
		let availableHeight = (safeAreaRect.size.y - HEADER_HEIGHT - 16.0).max(1.0);
		let stageScale = (availableWidth / DOCUMENT_WIDTH).min(availableHeight / DOCUMENT_HEIGHT);
		let stageX = (safeAreaRect.size.x - DOCUMENT_WIDTH * stageScale) * 0.5;
		let stage = &mut self.gameSceneBase.getSceneBaseMut().getNodeTreeMut()[stageId];
		stage.setLocalScale(Vector2::create(stageScale, stageScale));
		stage.setLocalPosition(Vector2::create(stageX, HEADER_HEIGHT));
	}
}

impl Scene for TimelineShowcaseScene {
	fn getSceneBase(&self) -> &SceneBase {
		self.gameSceneBase.getSceneBase()
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		self.gameSceneBase.getSceneBaseMut()
	}

	//==============================================================================
	// 생성. (로딩 화면을 시작하고 문서와 그림을 요청해 맡긴다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.gameSceneBase.setViewScaleMode(ViewScaleMode::stretchShort, None);
		self.gameSceneBase.setLoadingMinDurationMs(0.0);
		self.gameSceneBase.setSceneBackgroundColor(Color::createFromHEX("#14171e"));
		self.gameSceneBase.create(engine);
		for (fileName, _) in MODE_DEFINITIONS.iter() {
			let mut timelineAsset = JsonAsset::new();
			timelineAsset.load(engine, fileName);
			self.gameSceneBase.getSceneBaseMut().trackAsset(&timelineAsset);
			self.timelineAssetList.push(timelineAsset);
		}
		for imageName in IMAGE_NAME_LIST.iter() {
			let mut imageAsset = ImageAsset::new();
			imageAsset.load(engine, imageName);
			self.gameSceneBase.getSceneBaseMut().trackAsset(&imageAsset);
			self.imageAssetList.push((imageName.to_string(), imageAsset));
		}
	}

	//==============================================================================
	// 로딩 화면.
	//==============================================================================
	fn drawOnLoad(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.gameSceneBase.drawOnLoad(engine, graphic);
	}

	//==============================================================================
	// 초기화. (머리 글, 무대, 모드마다 무대 뿌리와 타임라인)
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		self.gameSceneBase.initialize(engine);
		let safeAreaId = self.gameSceneBase.getSafeAreaNodeId().expect("safe area node");
		let mut imageTable: HashMap<String, Rc<Image>> = HashMap::new();
		for (imageName, imageAsset) in self.imageAssetList.iter_mut() {
			if let Some(image) = imageAsset.getImage() {
				imageTable.insert(imageName.clone(), image);
			}
		}
		let descriptionList: Vec<Option<Value>> = self.timelineAssetList.iter_mut().map(|timelineAsset| timelineAsset.getData().cloned()).collect();

		let nodeTree = self.gameSceneBase.getSceneBaseMut().getNodeTreeMut();
		self.titleLabelId = Some(TimelineShowcaseScene::createLabel(nodeTree, safeAreaId, "Title", Vector2::create(24.0, 10.0), 22.0, Color::createFromHEX("#e8ebf2")));
		self.statusLabelId = Some(TimelineShowcaseScene::createLabel(nodeTree, safeAreaId, "Status", Vector2::create(24.0, 38.0), 14.0, Color::createFromHEX("#9aa2b2")));

		// 무대. (문서 크기, 둥근 모서리로 자른다)
		let stageId = nodeTree.createChild(safeAreaId, "Stage");
		{
			let stage = &mut nodeTree[stageId];
			stage.setPivot(Pivot::topLeft);
			stage.setAnchor(Pivot::topLeft);
			stage.setContentSize(Vector2::create(DOCUMENT_WIDTH, DOCUMENT_HEIGHT));
			stage.addComponent(Paint::createWithColor(Color::createFromHEX("#0d1017")));
			let mut mask = Mask::new();
			mask.setRoundSize(12.0);
			stage.addComponent(mask);
		}
		self.stageId = Some(stageId);

		let imageResolver = |imageName: &str| -> Option<Rc<Image>> {
			imageTable.get(imageName).cloned()
		};
		for ((_, title), description) in MODE_DEFINITIONS.iter().zip(descriptionList) {
			let description = match description {
				Some(description) => {
					description
				}
				None => {
					eprintln!("timelineshowcase: {} 문서를 읽지 못했습니다.", title);
					continue;
				}
			};
			let rootId = nodeTree.createChild(stageId, title);
			{
				let root = &mut nodeTree[rootId];
				root.setPivot(Pivot::topLeft);
				root.setAnchor(Pivot::topLeft);
				root.setContentSize(Vector2::create(DOCUMENT_WIDTH, DOCUMENT_HEIGHT));
				root.setActive(false);
			}
			let mut timeline = Timeline::new(Some(description));
			timeline.buildStage(nodeTree, rootId, Some(&imageResolver));
			timeline.setTargetResolver(Some(Box::new(|targetName: &str| {
				if targetName == "screen" {
					Some(TimelineTarget::external)
				}
				else {
					None
				}
			})));
			let markerSignal = self.lastSignal.clone();
			timeline.setMarkerHandler(Some(Box::new(move |markerName: &str, _marker: &TimelineMarker, _nodeTree: &mut NodeTree| {
				*markerSignal.borrow_mut() = format!("marker {}", markerName);
			})));
			let eventSignal = self.lastSignal.clone();
			timeline.setEventHandler(Some(Box::new(move |targetName: &str, eventName: &str, _key: &TimelineKey, _nodeTree: &mut NodeTree| {
				*eventSignal.borrow_mut() = format!("event {} {}", targetName, eventName);
			})));
			self.modeList.push(TimelineMode { rootId, timeline, title: title.to_string() });
		}
		// 아지랑이. (무대의 마지막 자식이라 문서 위에 그려진다)
		let hazeId = nodeTree.createChild(stageId, "HeatHaze");
		{
			let haze = &mut nodeTree[hazeId];
			haze.setPivot(Pivot::topLeft);
			haze.setAnchor(Pivot::topLeft);
			haze.setLocalPosition(Vector2::create(DOCUMENT_WIDTH - 340.0, DOCUMENT_HEIGHT - 220.0));
			haze.setContentSize(Vector2::create(320.0, 200.0));
			let heatHazeEffect = haze.addComponent(HeatHazeEffect::new());
			heatHazeEffect.setAmplitude(6.0);
			heatHazeEffect.setFrequency(0.08);
		}
		self.indicator.setRadius(14.0);
		self.indicator.setColor(Color::createFromHEX("#d4b06a"));
		self.selectMode(0);
	}

	//==============================================================================
	// 화면 크기 변경.
	//==============================================================================
	fn resize(&mut self, engine: &mut Engine, canvasNativeSize: Vector2) {
		self.gameSceneBase.resize(engine, canvasNativeSize);
		self.layout();
	}

	//==============================================================================
	// 갱신. (노드 / 터치 / 레이아웃 → 타임라인 → 화면 효과, 150 프레임마다 또는 누르면 다음 문서)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, _timeDelta: f32) {
		if self.gameSceneBase.tick(engine, TIMELINE_FRAME_STEP) {
			self.layout();
		}
		self.modeFrameCount += 1;
		let isTouchPressed = engine.getInputManager().isTouchPressed();
		if (self.modeFrameCount >= MODE_FRAME_COUNT || isTouchPressed) && !self.modeList.is_empty() {
			let nextModeIndex = (self.currentModeIndex + 1) % self.modeList.len();
			self.selectMode(nextModeIndex);
		}
		let nodeTree = self.gameSceneBase.getSceneBaseMut().getNodeTreeMut();
		if let Some(mode) = self.modeList.get_mut(self.currentModeIndex) {
			mode.timeline.tick(nodeTree, TIMELINE_FRAME_STEP);
			mode.timeline.applyExternalProperties("screen", &mut self.screenEffect);
			let statusText = format!("{:.2} / {:.2} s  {}  {}", mode.timeline.getTime(), mode.timeline.getDuration(), if mode.timeline.isLoop() { "loop" } else { "once" }, self.lastSignal.borrow());
			if let Some(statusLabelId) = self.statusLabelId {
				if let Some(text) = nodeTree[statusLabelId].getComponentMut::<Text>() {
					text.setText(&statusText);
				}
			}
		}
		self.screenEffect.tick(TIMELINE_FRAME_STEP);
		self.indicator.tick(TIMELINE_FRAME_STEP);
	}

	//==============================================================================
	// 이전 출력. (화면 효과 시작 → 배경)
	//==============================================================================
	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.screenEffect.begin(graphic);
		self.gameSceneBase.preDraw(engine, graphic);
	}

	//==============================================================================
	// 이후 출력. (화면 효과 끝)
	//==============================================================================
	fn postDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		let viewSize = engine.getViewManager().getViewSize();
		self.indicator.setPosition(viewSize.x - 40.0, 36.0);
		self.indicator.draw(graphic);
		self.screenEffect.end(graphic);
	}

	//==============================================================================
	// 해제.
	//==============================================================================
	fn destroy(&mut self, _engine: &mut Engine) {
	}
}

//==============================================================================
// 진입점.
//==============================================================================
fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("타임라인 쇼케이스 (바닐라엔진)");
	engineConfiguration.referenceResolutionSize = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engineConfiguration.viewScaleMode = ViewScaleMode::stretchShort;
	engineConfiguration.windowSize = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engineConfiguration.assetDirectoryPaths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engineConfiguration);
	if let Err(error) = engine.run(Box::new(TimelineShowcaseScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
