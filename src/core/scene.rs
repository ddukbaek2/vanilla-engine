//==============================================================================
// 씬.
// - 원본은 Scene 클래스를 상속해 create / tick / draw 를 덮어쓴다.
// - 러스트 판은 Scene 트레이트를 구현하고, 씬 공통 상태(노드 트리, 터치 대상)는 SceneBase 에 둔다.
// - 노드는 SceneBase 의 NodeTree 가 들고, 게임 코드는 NodeId 핸들로 가리킨다. (getNodeTreeMut().createChild(...))
//   원본의 super.tick() / super.draw() 자리는 getSceneBaseMut().tick() / .draw() 를 부른다.
// - 원본 GameScene 의 preDraw (캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 거는 것)는 SceneBase::drawSceneBackground 로 옮겼다.
// - 트윈은 원본처럼 씬이 목록으로 들고(startTween), tick 에서 노드 → 터치 다음에 돌리며 끝난 것을 뺀다.
//   원본은 트윈 객체로 가리키지만, 러스트 판은 트윈을 넘겨받고 TweenId 를 돌려준다. (stopTween(tweenId))
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::asset::{Asset, AssetLoadHandle};
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree, TouchKind};
use crate::core::node::worldnode::WorldNode;
use crate::core::tween::{Tween, TweenId};

//==============================================================================
// 씬이 기다리는 읽기 하나. (애셋 읽기 손잡이, 또는 오디오 애셋)
//==============================================================================
enum TrackedLoad {
	assetLoadHandle(AssetLoadHandle),
	audioAsset(Rc<AudioAsset>),
}

impl TrackedLoad {
	//==============================================================================
	// 끝났는지. (읽혔거나 실패했거나)
	//==============================================================================
	fn isFinished(&self) -> bool {
		match self {
			TrackedLoad::assetLoadHandle(loadHandle) => {
				loadHandle.isFinished()
			}
			TrackedLoad::audioAsset(audioAsset) => {
				audioAsset.isLoaded() || audioAsset.getLoadError().is_some()
			}
		}
	}
}

//==============================================================================
// 씬 공통 상태.
//==============================================================================
pub struct SceneBase {
	nodeTree: NodeTree,
	tweenList: Vec<Tween>,
	isLoaded: bool,
	isGizmoVisible: bool,
	sceneBackgroundColor: Color,
	pressedNodeId: Option<NodeId>,
	trackedLoadList: Vec<TrackedLoad>,
	hasEverTouched: bool,
	firstTouchCallback: Option<Box<dyn FnMut()>>,
}

impl SceneBase {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> SceneBase {
		SceneBase {
			nodeTree: NodeTree::new(),
			tweenList: Vec::new(),
			isLoaded: false,
			isGizmoVisible: false,
			sceneBackgroundColor: Color::black(),
			pressedNodeId: None,
			trackedLoadList: Vec::new(),
			hasEverTouched: false,
			firstTouchCallback: None,
		}
	}

	//==============================================================================
	// 첫 터치 콜백 설정. (처음 누를 때 한 번. 웹에서 오디오 잠금을 푸는 자리, 원본 TouchRaycaster)
	//==============================================================================
	pub fn setFirstTouchCallback(&mut self, firstTouchCallback: Box<dyn FnMut()>) {
		self.firstTouchCallback = Some(firstTouchCallback);
	}

	//==============================================================================
	// 지금 눌려 있는 노드. (원본 TouchRaycaster.getTouchTarget)
	//==============================================================================
	pub fn getTouchTarget(&self) -> Option<NodeId> {
		self.pressedNodeId
	}

	//==============================================================================
	// 애셋 맡기기. (create 에서 load 를 요청한 애셋. 맡긴 것이 다 읽히면 엔진이 initialize 를 부른다)
	//==============================================================================
	pub fn trackAsset(&mut self, asset: &dyn Asset) {
		if let Some(loadHandle) = asset.getAssetBase().getLoadHandle() {
			self.trackedLoadList.push(TrackedLoad::assetLoadHandle(loadHandle.clone()));
		}
	}

	//==============================================================================
	// 읽기 손잡이 맡기기. (애셋 꼴이 아닌 것)
	//==============================================================================
	pub fn trackLoadHandle(&mut self, loadHandle: &AssetLoadHandle) {
		self.trackedLoadList.push(TrackedLoad::assetLoadHandle(loadHandle.clone()));
	}

	//==============================================================================
	// 오디오 애셋 맡기기. (engine.loadAudioAsset 으로 받은 것. 웹은 디코딩이 끝나야 읽힌 것으로 친다)
	//==============================================================================
	pub fn trackAudioAsset(&mut self, audioAsset: &Rc<AudioAsset>) {
		self.trackedLoadList.push(TrackedLoad::audioAsset(audioAsset.clone()));
	}

	//==============================================================================
	// 맡긴 애셋이 다 끝났는지. (읽혔거나 실패했거나. 실패한 애셋 때문에 씬이 멈추지 않게 한다)
	//==============================================================================
	pub fn areTrackedAssetsFinished(&self) -> bool {
		self.trackedLoadList.iter().all(|trackedLoad| trackedLoad.isFinished())
	}

	//==============================================================================
	// 맡긴 애셋 가운데 끝난 비율. (로딩 화면 진행 막대용, 맡긴 것이 없으면 1)
	//==============================================================================
	pub fn getTrackedAssetProgress(&self) -> f32 {
		if self.trackedLoadList.is_empty() {
			return 1.0;
		}
		let finishedCount = self.trackedLoadList.iter().filter(|trackedLoad| trackedLoad.isFinished()).count();
		finishedCount as f32 / self.trackedLoadList.len() as f32
	}

	//==============================================================================
	// 맡긴 애셋 비우기. (initialize 직전에 엔진이 부른다)
	//==============================================================================
	pub fn clearTrackedAssets(&mut self) {
		self.trackedLoadList.clear();
	}

	//==============================================================================
	// 노드 트리.
	//==============================================================================
	pub fn getNodeTree(&self) -> &NodeTree {
		&self.nodeTree
	}

	//==============================================================================
	// 노드 트리. (변경용)
	//==============================================================================
	pub fn getNodeTreeMut(&mut self) -> &mut NodeTree {
		&mut self.nodeTree
	}

	//==============================================================================
	// 루트 노드 핸들.
	//==============================================================================
	pub fn getRootId(&self) -> NodeId {
		self.nodeTree.getRootId()
	}

	//==============================================================================
	// 루트 노드.
	//==============================================================================
	pub fn getRoot(&self) -> &WorldNode {
		let rootId = self.nodeTree.getRootId();
		&self.nodeTree[rootId]
	}

	//==============================================================================
	// 루트 노드. (변경용)
	//==============================================================================
	pub fn getRootMut(&mut self) -> &mut WorldNode {
		let rootId = self.nodeTree.getRootId();
		&mut self.nodeTree[rootId]
	}

	//==============================================================================
	// 로드 완료 설정.
	//==============================================================================
	pub fn setLoaded(&mut self, isLoaded: bool) {
		self.isLoaded = isLoaded;
	}

	//==============================================================================
	// 로드 완료 여부.
	//==============================================================================
	pub fn isLoaded(&self) -> bool {
		self.isLoaded
	}

	//==============================================================================
	// 기즈모 표시 설정.
	//==============================================================================
	pub fn setGizmoVisible(&mut self, isGizmoVisible: bool) {
		self.isGizmoVisible = isGizmoVisible;
	}

	//==============================================================================
	// 기즈모 표시 여부.
	//==============================================================================
	pub fn isGizmoVisible(&self) -> bool {
		self.isGizmoVisible
	}

	//==============================================================================
	// 배경색 설정. (drawSceneBackground 가 칠한다)
	//==============================================================================
	pub fn setSceneBackgroundColor(&mut self, color: impl Into<Color>) {
		self.sceneBackgroundColor = color.into();
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	pub fn getSceneBackgroundColor(&self) -> Color {
		self.sceneBackgroundColor
	}

	//==============================================================================
	// 기본 갱신. (루트 노드 → 터치 → 트윈)
	//==============================================================================
	pub fn tick(&mut self, engine: &Engine, timeDelta: f32) {
		self.nodeTree.tick(engine, timeDelta);
		self.tickTouch(engine);
		self.tickTweens(timeDelta);
	}

	//==============================================================================
	// 트윈 목록 갱신. (원본처럼 뒤에서부터 돌고, 끝난 트윈은 목록에서 뺀다)
	// - 같은 속성을 둘이 움직이면 먼저 시작한 트윈이 나중에 돌아 이긴다. (원본과 같음)
	//==============================================================================
	pub fn tickTweens(&mut self, timeDelta: f32) {
		let mut tweenIndex = self.tweenList.len();
		while tweenIndex > 0 {
			tweenIndex -= 1;
			let tween = &mut self.tweenList[tweenIndex];
			tween.tick(&mut self.nodeTree, timeDelta);
			if tween.isFinished() {
				self.tweenList.remove(tweenIndex);
			}
		}
	}

	//==============================================================================
	// 트윈 시작. (트윈을 넘겨받아 start 하고 목록 끝에 붙인다. 가리킬 핸들을 돌려준다)
	//==============================================================================
	pub fn startTween(&mut self, mut tween: Tween) -> TweenId {
		let tweenId = tween.getTweenId();
		tween.start();
		self.tweenList.push(tween);
		tweenId
	}

	//==============================================================================
	// 트윈 중단. (완료 콜백 없이 멈추고 목록에서 뺀다. 이미 끝나 빠졌으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn stopTween(&mut self, tweenId: TweenId) {
		let tweenIndex = match self.tweenList.iter().position(|tween| tween.getTweenId() == tweenId) {
			Some(tweenIndex) => {
				tweenIndex
			}
			None => {
				return;
			}
		};
		self.tweenList[tweenIndex].stop();
		self.tweenList.remove(tweenIndex);
	}

	//==============================================================================
	// 모든 트윈 중단.
	//==============================================================================
	pub fn stopAllTweens(&mut self) {
		for tween in self.tweenList.iter_mut() {
			tween.stop();
		}
		self.tweenList.clear();
	}

	//==============================================================================
	// 트윈 반환. (끝나서 목록에서 빠졌으면 None. 원본에서 트윈 객체를 들고 묻던 자리)
	//==============================================================================
	pub fn getTween(&self, tweenId: TweenId) -> Option<&Tween> {
		self.tweenList.iter().find(|tween| tween.getTweenId() == tweenId)
	}

	//==============================================================================
	// 트윈 반환. (변경용)
	//==============================================================================
	pub fn getTweenMut(&mut self, tweenId: TweenId) -> Option<&mut Tween> {
		self.tweenList.iter_mut().find(|tween| tween.getTweenId() == tweenId)
	}

	//==============================================================================
	// 돌고 있는 트윈 수.
	//==============================================================================
	pub fn getTweenCount(&self) -> usize {
		self.tweenList.len()
	}

	//==============================================================================
	// 터치 갱신. (눌린 곳 맨 위의 interactable 노드를 찾아 눌림 / 이동 / 뗌을 전달)
	//==============================================================================
	pub fn tickTouch(&mut self, engine: &Engine) {
		let inputManager = engine.getInputManager();
		let viewInputPosition = inputManager.getViewInputPosition();

		// 휠. (누름과 상관없이 그 자리 아래 노드부터 부모 쪽으로)
		let wheelDelta = Vector2::create(inputManager.getWheelDeltaX(), inputManager.getWheelDeltaY());
		if wheelDelta.x != 0.0 || wheelDelta.y != 0.0 {
			if let Some(hitId) = self.nodeTree.findTopmostInteractableNode(viewInputPosition) {
				self.nodeTree.dispatchWheel(hitId, engine, wheelDelta);
			}
		}

		if inputManager.isTouchPressed() {
			if !self.hasEverTouched {
				self.hasEverTouched = true;
				if let Some(firstTouchCallback) = self.firstTouchCallback.as_mut() {
					firstTouchCallback();
				}
			}
			let foundId = self.nodeTree.findTopmostInteractableNode(viewInputPosition);
			if let Some(foundId) = foundId {
				self.nodeTree.dispatchTouch(foundId, engine, TouchKind::press, viewInputPosition);
			}
			self.pressedNodeId = foundId;
		}

		let pressedNodeId = match self.pressedNodeId {
			Some(pressedNodeId) => {
				pressedNodeId
			}
			None => {
				return;
			}
		};
		if inputManager.isTouchMoved() {
			self.nodeTree.dispatchTouch(pressedNodeId, engine, TouchKind::moved, viewInputPosition);
		}
		if inputManager.isTouchReleased() {
			self.nodeTree.dispatchTouch(pressedNodeId, engine, TouchKind::release, viewInputPosition);
			self.pressedNodeId = None;
		}
		else if inputManager.isTouchCancelled() {
			self.nodeTree.dispatchTouch(pressedNodeId, engine, TouchKind::cancel, viewInputPosition);
			self.pressedNodeId = None;
		}
	}

	//==============================================================================
	// 기본 출력. (루트 노드 재귀)
	//==============================================================================
	pub fn draw(&mut self, graphic: &mut Graphic) {
		self.nodeTree.draw(graphic);
	}

	//==============================================================================
	// 캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 건다. (원본 GameScene.preDraw)
	//==============================================================================
	pub fn drawSceneBackground(&self, engine: &Engine, graphic: &mut Graphic) {
		let viewManager = engine.getViewManager();
		let canvasNativeSize = viewManager.getCanvasNativeSize();

		viewManager.applyCanvasNativeRect(graphic);
		graphic.setFillColor(self.sceneBackgroundColor);
		graphic.drawRect(Rect::create(0.0, 0.0, canvasNativeSize.x, canvasNativeSize.y));

		viewManager.applyViewRect(graphic);
	}
}

impl Default for SceneBase {
	fn default() -> SceneBase {
		SceneBase::new()
	}
}

//==============================================================================
// 씬 트레이트.
//==============================================================================
pub trait Scene {
	//==============================================================================
	// 씬 공통 상태 반환.
	//==============================================================================
	fn getSceneBase(&self) -> &SceneBase;

	//==============================================================================
	// 씬 공통 상태 반환. (변경용)
	//==============================================================================
	fn getSceneBaseMut(&mut self) -> &mut SceneBase;

	//==============================================================================
	// 생성. (엔진이 씬을 불러올 때 한 번. 노드 트리와 애셋을 여기서 만든다)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 초기화. (create 에서 맡긴 애셋이 다 읽힌 뒤 한 번. 원본 initialize. 맡긴 것이 없으면 create 바로 뒤)
	// - 웹은 애셋이 나중에 오므로, 애셋을 쓰는 노드는 여기서 만든다. 데스크탑은 create 에서 만들어도 된다.
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 로딩 중 출력. (initialize 전까지 tick / draw 대신 불린다. 원본 drawOnLoad)
	//==============================================================================
	fn drawOnLoad(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 해제. (엔진이 씬을 내릴 때 한 번)
	//==============================================================================
	fn destroy(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 화면 영역 변경. (창 크기가 바뀌었을 때, 불러온 직후에도 한 번)
	//==============================================================================
	fn resize(&mut self, _engine: &mut Engine, _canvasNativeSize: Vector2) {
	}

	//==============================================================================
	// 주기적 갱신.
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, timeDelta: f32) {
		self.getSceneBaseMut().tick(engine, timeDelta);
	}

	//==============================================================================
	// 이전 출력.
	//==============================================================================
	fn preDraw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.getSceneBaseMut().draw(graphic);
	}

	//==============================================================================
	// 이후 출력.
	//==============================================================================
	fn postDraw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 기즈모 출력.
	//==============================================================================
	fn drawGizmos(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}
}
