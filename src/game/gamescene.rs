//==============================================================================
// 게임 공통 베이스 씬. (원본 game/gamescene.js 의 GameScene)
// - 로딩 화면 (가운데 이미지 + 아래 게이지 + 터치로 건너뛰기, 최소 노출 시간).
// - 터치 입력은 TouchRecognizer 로 돌린다. (스크롤 뷰 안의 버튼을 끌어도 스크롤되는 손짓)
// - 세이프 에어리어 노드 "safeArea" + 세이프 에어리어 영역 계산.
// - 뷰 크기가 바뀌면 다음 tick 에서 layout 을 다시 부른다.
// - 캔버스 전체를 배경색으로 칠한다. (preDraw)
// - 러스트 판은 상속 대신 씬이 GameSceneBase 를 품고 Scene 함수마다 이쪽을 부른다:
//     struct TitleScene { gameSceneBase: GameSceneBase, logoAsset: ImageAsset }
//     impl Scene for TitleScene {
//         fn getSceneBase(&self) -> &SceneBase { self.gameSceneBase.getSceneBase() }
//         fn getSceneBaseMut(&mut self) -> &mut SceneBase { self.gameSceneBase.getSceneBaseMut() }
//         fn create(&mut self, engine: &mut Engine) {
//             self.gameSceneBase.setLoadingImageUrl("loading.png");
//             self.gameSceneBase.create(engine);                       // 원본 load + super.loadAssets
//             self.logoAsset.load(engine, "logo.png");                 // 원본 loadAssets 재정의 자리
//             self.gameSceneBase.getSceneBaseMut().trackAsset(&self.logoAsset);
//         }
//         fn drawOnLoad(&mut self, engine: &mut Engine, graphic: &mut Graphic) { self.gameSceneBase.drawOnLoad(engine, graphic); }
//         fn initialize(&mut self, engine: &mut Engine) {
//             self.gameSceneBase.initialize(engine);
//             let safeAreaId = self.gameSceneBase.getSafeAreaNodeId().unwrap();   // 콘텐츠는 여기 아래에 붙인다
//         }
//         fn resize(&mut self, engine: &mut Engine, canvasNativeSize: Vector2) { self.gameSceneBase.resize(engine, canvasNativeSize); self.layout(engine); }
//         fn tick(&mut self, engine: &mut Engine, timeDelta: f32) { if self.gameSceneBase.tick(engine, timeDelta) { self.layout(engine); } }
//         fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) { self.gameSceneBase.preDraw(engine, graphic); }
//     }
//   씬의 layout 은 원본 layout 재정의 자리다. (gameSceneBase.getSafeAreaRect() 를 읽어 콘텐츠를 놓는다)
// - 원본과 다른 점:
//   - 원본 load(비동기)의 "loadAssets 를 기다린 뒤 최소 시간 / 터치까지 기다림"은 러스트 애셋 흐름으로 옮겼다.
//     create 에서 "로딩 문" 읽기 손잡이 하나를 맡겨 두고, drawOnLoad 가 최소 시간이 지나거나 화면을 누르면 문을 연다.
//     엔진은 맡긴 애셋이 모두(문 포함) 끝나야 initialize 를 부르므로 결과는 원본과 같다. (애셋 다 읽힘 + (최소 시간 또는 터치))
//   - 시간은 원본 Date.now() 대신 엔진 시작 뒤 실제 경과 시간(getRealtimeSinceStartup)으로 잰다.
//   - 화면 누름은 원본 window pointerdown 대신 입력 매니저의 isTouchPressed 로 본다.
//   - 원본은 TouchRecognizer 를 initialize 에서 만든다. 러스트 판은 생성할 때 만든다. 터치는 원본 touchPress 같은 함수 대신 tick 이 넘긴다.
//     (SceneBase 의 터치 레이캐스터와 첫 터치 콜백은 쓰지 않는다)
//   - tick 과 resize 는 원본처럼 layout 을 부르되, 씬의 layout 은 부를 수 없어서 bool(tick) 로 알리거나 씬이 resize 뒤에 직접 부른다.
//   - 세이프 에어리어: 웹은 원본처럼 CSS env(safe-area-inset-*) 를 읽는다. 데스크탑에는 그런 값이 없어서 0 이고,
//     setSafeAreaInsets 로 정해 줄 수 있다. (정하면 웹에서도 그 값을 쓴다)
//   - setViewScaleMode 는 엔진을 같이 받는다. (전역 엔진이 없다) 엔진을 주면 원본처럼 바로 뷰 영역을 다시 계산한다.
//   - 개발자 도구(DEVTools)는 옮기지 않았다. isDevToolsCapturingInput 은 늘 false 이고, postDraw 의 도구 그리기는 없다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::NodeId;
use crate::core::scene::SceneBase;
use crate::core::viewmanager::ViewScaleMode;
use crate::resource::imageasset::ImageAsset;
use crate::ui::touchrecognizer::TouchRecognizer;

// 로딩 화면 최소 노출 시간 기본값. (밀리초)
const DEFAULT_LOAD_MIN_DURATION_MS: f32 = 3000.0;

// 로딩 게이지 높이. (뷰 좌표)
const LOADING_BAR_HEIGHT: f32 = 24.0;

// 로딩 문 손잡이 경로. (애셋이 아닌 대기 표시)
const LOADING_GATE_PATH: &str = "GameSceneBase.loadingGate";

//==============================================================================
// 게임 공통 베이스 씬.
//==============================================================================
pub struct GameSceneBase {
	sceneBase: SceneBase,
	touchRecognizer: TouchRecognizer,
	safeAreaNodeId: Option<NodeId>,
	lastSafeAreaRect: Rect,
	lastViewSizeX: f32,
	lastViewSizeY: f32,
	loadingImageAsset: Option<ImageAsset>,
	loadingImageUrl: String,
	// 로딩 시작 시각. (엔진 시작 뒤 초)
	loadStartTime: f64,
	loadMinDurationMs: f32,
	loadTouchedToSkip: bool,
	loadGateHandle: Option<AssetLoadHandle>,
	viewScaleMode: ViewScaleMode,
	// 세이프 에어리어 인셋. (CSS 픽셀, 위 / 오른쪽 / 아래 / 왼쪽. 없으면 플랫폼에서 읽는다)
	safeAreaInsets: Option<[f32; 4]>,
}

impl GameSceneBase {
	//==============================================================================
	// 생성. (최소 노출 3 초, 검은 배경, ViewScaleMode::none)
	//==============================================================================
	pub fn new() -> GameSceneBase {
		let mut sceneBase = SceneBase::new();
		sceneBase.setSceneBackgroundColor(Color::black());
		GameSceneBase {
			sceneBase,
			touchRecognizer: TouchRecognizer::new(),
			safeAreaNodeId: None,
			lastSafeAreaRect: Rect::zero(),
			lastViewSizeX: 0.0,
			lastViewSizeY: 0.0,
			loadingImageAsset: None,
			loadingImageUrl: String::new(),
			loadStartTime: 0.0,
			loadMinDurationMs: DEFAULT_LOAD_MIN_DURATION_MS,
			loadTouchedToSkip: false,
			loadGateHandle: None,
			viewScaleMode: ViewScaleMode::none,
			safeAreaInsets: None,
		}
	}

	//==============================================================================
	// 씬 공통 상태 반환. (Scene::getSceneBase 에서 돌려준다)
	//==============================================================================
	pub fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	//==============================================================================
	// 씬 공통 상태 반환. (변경용)
	//==============================================================================
	pub fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 터치 인식기 반환.
	//==============================================================================
	pub fn getTouchRecognizer(&self) -> &TouchRecognizer {
		&self.touchRecognizer
	}

	//==============================================================================
	// 터치 인식기 반환. (변경용)
	//==============================================================================
	pub fn getTouchRecognizerMut(&mut self) -> &mut TouchRecognizer {
		&mut self.touchRecognizer
	}

	//==============================================================================
	// ViewScaleMode 설정. (기본 none. 엔진을 주면 바로 뷰 영역에 반영하고, 아니면 initialize 에서 적용한다)
	//==============================================================================
	pub fn setViewScaleMode(&mut self, viewScaleMode: ViewScaleMode, engine: Option<&mut Engine>) {
		self.viewScaleMode = viewScaleMode;
		if let Some(engine) = engine {
			self.applyViewScaleMode(engine);
		}
	}

	//==============================================================================
	// 지금 ViewScaleMode 반환.
	//==============================================================================
	pub fn getViewScaleMode(&self) -> ViewScaleMode {
		self.viewScaleMode
	}

	//==============================================================================
	// 뷰 매니저에 ViewScaleMode 반영. (바뀌었으면 지금 캔버스 크기로 뷰 영역을 다시 계산한다. 원본 viewManager.setViewScaleMode)
	//==============================================================================
	fn applyViewScaleMode(&self, engine: &mut Engine) {
		let viewManager = engine.getViewManagerMut();
		if viewManager.getViewScaleMode() == self.viewScaleMode {
			return;
		}
		viewManager.setViewScaleMode(self.viewScaleMode);
		let canvasPixelSize = viewManager.getCanvasPixelSize();
		if canvasPixelSize.x > 0.0 && canvasPixelSize.y > 0.0 {
			let devicePixelRatio = viewManager.getDevicePixelRatio();
			viewManager.calculateViewRect(canvasPixelSize.x as u32, canvasPixelSize.y as u32, devicePixelRatio);
		}
	}

	//==============================================================================
	// 로딩 화면 가운데 이미지 경로 설정. (빈 글이면 이미지 없이 게이지만. create 전에 정한다)
	//==============================================================================
	pub fn setLoadingImageUrl(&mut self, url: &str) {
		self.loadingImageUrl = url.to_string();
	}

	//==============================================================================
	// 로딩 화면 최소 노출 시간 설정. (밀리초. create 전에 정한다)
	//==============================================================================
	pub fn setLoadingMinDurationMs(&mut self, milliseconds: f32) {
		self.loadMinDurationMs = milliseconds;
	}

	//==============================================================================
	// 캔버스 배경색 설정. (preDraw 에서 캔버스 전체에 칠한다)
	//==============================================================================
	pub fn setSceneBackgroundColor(&mut self, color: impl Into<Color>) {
		self.sceneBase.setSceneBackgroundColor(color);
	}

	//==============================================================================
	// 세이프 에어리어 노드 반환. (씬이 콘텐츠를 여기에 붙인다. initialize 전이면 None)
	//==============================================================================
	pub fn getSafeAreaNodeId(&self) -> Option<NodeId> {
		self.safeAreaNodeId
	}

	//==============================================================================
	// 마지막으로 계산한 세이프 에어리어 영역 반환. (뷰 좌표)
	//==============================================================================
	pub fn getSafeAreaRect(&self) -> Rect {
		self.lastSafeAreaRect
	}

	//==============================================================================
	// 세이프 에어리어 인셋 직접 설정. (CSS 픽셀: 위, 오른쪽, 아래, 왼쪽. None 이면 플랫폼에서 읽는다)
	//==============================================================================
	pub fn setSafeAreaInsets(&mut self, insets: Option<[f32; 4]>) {
		self.safeAreaInsets = insets;
	}

	//==============================================================================
	// 생성. (씬 create 에서 맨 먼저 부른다. 원본 load 의 앞부분 + loadAssets)
	//==============================================================================
	pub fn create(&mut self, engine: &mut Engine) {
		let currentTime = engine.getTimeManager().getRealtimeSinceStartup();
		self.beginLoading(currentTime);
		self.loadAssets(engine);
	}

	//==============================================================================
	// 로딩 시작. (시작 시각을 적고, 최소 시간 / 터치를 기다리는 로딩 문을 씬에 맡긴다)
	//==============================================================================
	pub fn beginLoading(&mut self, currentTime: f64) {
		self.loadStartTime = currentTime;
		self.loadTouchedToSkip = false;
		let loadGateHandle = AssetLoadHandle::createPending(LOADING_GATE_PATH);
		self.sceneBase.trackLoadHandle(&loadGateHandle);
		if self.loadMinDurationMs <= 0.0 {
			loadGateHandle.finish(Ok(Vec::new()));
		}
		self.loadGateHandle = Some(loadGateHandle);
	}

	//==============================================================================
	// 자산 읽기. (원본 loadAssets 의 기본 부분: 로딩 이미지. 씬의 자체 애셋은 씬 create 에서 요청하고 trackAsset 으로 맡긴다)
	//==============================================================================
	pub fn loadAssets(&mut self, engine: &Engine) {
		if self.loadingImageUrl.is_empty() {
			return;
		}
		let mut loadingImageAsset = ImageAsset::new();
		loadingImageAsset.load(engine, &self.loadingImageUrl);
		self.sceneBase.trackAsset(&loadingImageAsset);
		self.loadingImageAsset = Some(loadingImageAsset);
	}

	//==============================================================================
	// 로딩 갱신. (누르면 건너뛰기로 적고, 최소 시간이 지났거나 건너뛰었으면 로딩 문을 연다)
	//==============================================================================
	pub fn updateLoading(&mut self, currentTime: f64, isTouchPressed: bool) {
		if isTouchPressed {
			self.loadTouchedToSkip = true;
		}
		let loadGateHandle = match self.loadGateHandle.as_ref() {
			Some(loadGateHandle) => {
				loadGateHandle
			}
			None => {
				return;
			}
		};
		if loadGateHandle.isFinished() {
			return;
		}
		let elapsedMilliseconds = (currentTime - self.loadStartTime) * 1000.0;
		if elapsedMilliseconds >= self.loadMinDurationMs as f64 || self.loadTouchedToSkip {
			loadGateHandle.finish(Ok(Vec::new()));
		}
	}

	//==============================================================================
	// 로딩 게이지 진행률. (경과 시간 / 최소 노출 시간, 0 ~ 1)
	//==============================================================================
	pub fn getLoadingProgress(&self, currentTime: f64) -> f32 {
		if self.loadMinDurationMs <= 0.0 {
			return 1.0;
		}
		let elapsedMilliseconds = ((currentTime - self.loadStartTime) * 1000.0) as f32;
		(elapsedMilliseconds / self.loadMinDurationMs).clamp(0.0, 1.0)
	}

	//==============================================================================
	// 건너뛰기 눌렸는지.
	//==============================================================================
	pub fn isLoadTouchedToSkip(&self) -> bool {
		self.loadTouchedToSkip
	}

	//==============================================================================
	// 로딩 화면 출력. (엔진이 initialize 전까지 매 프레임 부른다. 검은 배경 + 가운데 로딩 이미지 + 아래 게이지)
	//==============================================================================
	pub fn drawOnLoad(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		let currentTime = engine.getTimeManager().getRealtimeSinceStartup();
		let isTouchPressed = engine.getInputManager().isTouchPressed();
		self.updateLoading(currentTime, isTouchPressed);

		let viewManager = engine.getViewManager();
		let canvasNativeSize = viewManager.getCanvasNativeSize();
		let viewSize = viewManager.getViewSize();

		// 캔버스 전체 검은색.
		viewManager.applyCanvasNativeRect(graphic);
		graphic.setFillColor(Color::black());
		graphic.drawRect(Rect::create(0.0, 0.0, canvasNativeSize.x, canvasNativeSize.y));

		// 뷰 좌표계 적용.
		viewManager.applyViewRect(graphic);
		graphic.setFillColor(Color::black());
		graphic.drawRect(Rect::create(0.0, 0.0, viewSize.x, viewSize.y));

		// 가운데 로딩 이미지. (가로 70%, 비율 유지)
		let loadingImage = match self.loadingImageAsset.as_mut() {
			Some(loadingImageAsset) => {
				loadingImageAsset.getImage()
			}
			None => {
				None
			}
		};
		if let Some(image) = loadingImage {
			let targetWidth = viewSize.x * 0.7;
			let ratio = image.getHeight() as f32 / (image.getWidth() as f32).max(1.0);
			let targetHeight = targetWidth * ratio;
			let positionX = (viewSize.x - targetWidth) * 0.5;
			let positionY = (viewSize.y - targetHeight) * 0.5;
			graphic.drawImage(&image, Vector2::create(positionX, positionY), Vector2::create(targetWidth, targetHeight));
		}

		// 아래 로딩 게이지.
		let progress = self.getLoadingProgress(currentTime);
		let barWidth = viewSize.x * 0.6;
		let barX = (viewSize.x - barWidth) * 0.5;
		let barY = viewSize.y * 0.78;
		graphic.setFillColor(Color::createFromHEX("#333333"));
		graphic.drawRect(Rect::create(barX, barY, barWidth, LOADING_BAR_HEIGHT));
		graphic.setFillColor(Color::createFromHEX("#ffffff"));
		graphic.drawRect(Rect::create(barX, barY, barWidth * progress, LOADING_BAR_HEIGHT));
	}

	//==============================================================================
	// 초기화. (씬 initialize 에서 맨 먼저 부른다. ViewScaleMode 적용, 세이프 에어리어 노드 생성)
	//==============================================================================
	pub fn initialize(&mut self, engine: &mut Engine) {
		self.applyViewScaleMode(engine);
		self.createSafeAreaNode();
	}

	//==============================================================================
	// 세이프 에어리어 노드 생성. (루트 아래 "safeArea", 피봇 / 앵커 왼쪽 위)
	//==============================================================================
	fn createSafeAreaNode(&mut self) {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let rootId = nodeTree.getRootId();
		let safeAreaNodeId = nodeTree.createChild(rootId, "safeArea");
		nodeTree[safeAreaNodeId].setPivot(Pivot::topLeft);
		nodeTree[safeAreaNodeId].setAnchor(Pivot::topLeft);
		self.safeAreaNodeId = Some(safeAreaNodeId);
		self.lastSafeAreaRect = Rect::zero();
		self.lastViewSizeX = 0.0;
		self.lastViewSizeY = 0.0;
	}

	//==============================================================================
	// 화면 크기 변경됨. (기본 레이아웃을 다시 한다. 씬은 이 뒤에 자기 layout 을 부른다)
	//==============================================================================
	pub fn resize(&mut self, engine: &Engine, _canvasNativeSize: Vector2) {
		self.layout(engine);
	}

	//==============================================================================
	// 레이아웃. (세이프 에어리어 영역을 다시 계산해 노드에 건다)
	//==============================================================================
	pub fn layout(&mut self, engine: &Engine) {
		let safeAreaRect = self.computeSafeAreaRect(engine);
		self.applySafeAreaRect(safeAreaRect);
	}

	//==============================================================================
	// 세이프 에어리어 영역을 노드에 건다. (노드가 아직 없으면 영역만 적는다)
	//==============================================================================
	fn applySafeAreaRect(&mut self, safeAreaRect: Rect) {
		self.lastSafeAreaRect = safeAreaRect;
		if let Some(safeAreaNodeId) = self.safeAreaNodeId {
			if let Some(safeAreaNode) = self.sceneBase.getNodeTreeMut().getNodeMut(safeAreaNodeId) {
				safeAreaNode.setLocalPosition(safeAreaRect.position);
				safeAreaNode.setContentSize(safeAreaRect.size);
			}
		}
	}

	//==============================================================================
	// 세이프 에어리어 계산. (뷰 좌표)
	//==============================================================================
	pub fn computeSafeAreaRect(&self, engine: &Engine) -> Rect {
		let viewManager = engine.getViewManager();
		let insets = match self.safeAreaInsets {
			Some(insets) => {
				insets
			}
			None => {
				readPlatformSafeAreaInsets()
			}
		};
		GameSceneBase::calculateSafeAreaRect(viewManager.getViewSize(), viewManager.getTargetResolutionScale(), insets)
	}

	//==============================================================================
	// 세이프 에어리어 계산. (정적. CSS 픽셀 인셋 [위, 오른쪽, 아래, 왼쪽] 을 뷰 배율로 나눠 뷰 영역에서 뺀다)
	//==============================================================================
	pub fn calculateSafeAreaRect(viewSize: Vector2, targetResolutionScale: f32, insets: [f32; 4]) -> Rect {
		let scale = if targetResolutionScale > 0.0 {
			targetResolutionScale
		}
		else {
			1.0
		};
		let insetTop = insets[0] / scale;
		let insetRight = insets[1] / scale;
		let insetBottom = insets[2] / scale;
		let insetLeft = insets[3] / scale;
		let width = (viewSize.x - insetLeft - insetRight).max(0.0);
		let height = (viewSize.y - insetTop - insetBottom).max(0.0);
		Rect::create(insetLeft, insetTop, width, height)
	}

	//==============================================================================
	// 뷰 크기가 바뀌었으면 기본 레이아웃을 다시 한다. (바뀌었으면 true)
	//==============================================================================
	pub fn updateLayoutIfViewSizeChanged(&mut self, engine: &Engine) -> bool {
		let viewSize = engine.getViewManager().getViewSize();
		if viewSize.x == self.lastViewSizeX && viewSize.y == self.lastViewSizeY {
			return false;
		}
		self.lastViewSizeX = viewSize.x;
		self.lastViewSizeY = viewSize.y;
		self.layout(engine);
		true
	}

	//==============================================================================
	// 갱신. (노드 → TouchRecognizer 터치 → 트윈, 그다음 뷰 크기 변화를 보고 레이아웃. 레이아웃을 다시 했으면 true)
	// - 모바일 주소창이 나타나고 사라질 때처럼 resize 만으로는 놓칠 수 있어 매 tick 확인한다. (원본과 같음)
	//==============================================================================
	pub fn tick(&mut self, engine: &Engine, timeDelta: f32) -> bool {
		self.sceneBase.getNodeTreeMut().tick(engine, timeDelta);
		self.touchRecognizer.tickTouch(self.sceneBase.getNodeTreeMut(), engine);
		self.sceneBase.tickTweens(timeDelta);
		self.updateLayoutIfViewSizeChanged(engine)
	}

	//==============================================================================
	// 개발자 도구가 입력을 가져가는 중인지. (DEVTools 를 옮기지 않아 늘 false)
	//==============================================================================
	pub fn isDevToolsCapturingInput(&self) -> bool {
		false
	}

	//==============================================================================
	// 이전 출력. (캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 건다. 세이프 에어리어 바깥 포함)
	//==============================================================================
	pub fn preDraw(&self, engine: &Engine, graphic: &mut Graphic) {
		self.sceneBase.drawSceneBackground(engine, graphic);
	}
}

impl Default for GameSceneBase {
	fn default() -> GameSceneBase {
		GameSceneBase::new()
	}
}

//==============================================================================
// 글 앞의 정수 읽기. (원본 parseInt(글) || 0. "34px" → 34)
//==============================================================================
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn parseLeadingInteger(text: &str) -> f32 {
	let trimmedText = text.trim_start();
	let (sign, unsignedText) = match trimmedText.strip_prefix('-') {
		Some(rest) => {
			(-1.0, rest)
		}
		None => {
			(1.0, trimmedText.strip_prefix('+').unwrap_or(trimmedText))
		}
	};
	let digitText: String = unsignedText.chars().take_while(|character| character.is_ascii_digit()).collect();
	match digitText.parse::<f32>() {
		Ok(value) => {
			sign * value
		}
		Err(_) => {
			0.0
		}
	}
}

//==============================================================================
// 플랫폼의 세이프 에어리어 인셋 읽기. (웹: CSS env(safe-area-inset-*), 위 / 오른쪽 / 아래 / 왼쪽 CSS 픽셀)
// - 원본처럼 보이지 않는 div 에 패딩으로 걸고 계산된 스타일을 읽은 뒤 뗀다.
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn readPlatformSafeAreaInsets() -> [f32; 4] {
	use wasm_bindgen::JsCast;
	let readInsets = || -> Option<[f32; 4]> {
		let window = web_sys::window()?;
		let document = window.document()?;
		let body = document.body()?;
		let element: web_sys::HtmlElement = document.create_element("div").ok()?.dyn_into().ok()?;
		let style = element.style();
		style.set_property("position", "absolute").ok()?;
		style.set_property("visibility", "hidden").ok()?;
		style.set_property("padding-top", "env(safe-area-inset-top)").ok()?;
		style.set_property("padding-right", "env(safe-area-inset-right)").ok()?;
		style.set_property("padding-bottom", "env(safe-area-inset-bottom)").ok()?;
		style.set_property("padding-left", "env(safe-area-inset-left)").ok()?;
		body.insert_adjacent_element("beforeend", &element).ok()?;
		let insets = match window.get_computed_style(&element) {
			Ok(Some(computedStyle)) => {
				let readPadding = |propertyName: &str| parseLeadingInteger(&computedStyle.get_property_value(propertyName).unwrap_or_default());
				Some([readPadding("padding-top"), readPadding("padding-right"), readPadding("padding-bottom"), readPadding("padding-left")])
			}
			_ => {
				None
			}
		};
		element.remove();
		insets
	};
	readInsets().unwrap_or([0.0, 0.0, 0.0, 0.0])
}

//==============================================================================
// 플랫폼의 세이프 에어리어 인셋 읽기. (데스크탑에는 그런 값이 없어 0)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn readPlatformSafeAreaInsets() -> [f32; 4] {
	[0.0, 0.0, 0.0, 0.0]
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;

	//==============================================================================
	// 로딩 문은 최소 노출 시간이 지나야 열리고, 게이지는 경과 시간 비율이다. 다른 애셋이 남아 있으면 씬은 계속 기다린다.
	//==============================================================================
	#[test]
	fn loadingGateWaitsForMinimumDuration() {
		let mut gameSceneBase = GameSceneBase::new();
		gameSceneBase.setLoadingMinDurationMs(3000.0);
		let otherAssetHandle = AssetLoadHandle::createPending("other.png");
		gameSceneBase.beginLoading(10.0);
		gameSceneBase.getSceneBaseMut().trackLoadHandle(&otherAssetHandle);
		assert!(!gameSceneBase.getSceneBase().areTrackedAssetsFinished());
		assert_eq!(gameSceneBase.getLoadingProgress(10.0), 0.0);
		assert_eq!(gameSceneBase.getLoadingProgress(11.5), 0.5);
		assert_eq!(gameSceneBase.getLoadingProgress(20.0), 1.0);

		gameSceneBase.updateLoading(12.9, false);
		assert_eq!(gameSceneBase.getSceneBase().getTrackedAssetProgress(), 0.0);
		gameSceneBase.updateLoading(13.0, false);
		assert_eq!(gameSceneBase.getSceneBase().getTrackedAssetProgress(), 0.5);
		assert!(!gameSceneBase.getSceneBase().areTrackedAssetsFinished());
		otherAssetHandle.finish(Ok(vec![1, 2, 3]));
		assert!(gameSceneBase.getSceneBase().areTrackedAssetsFinished());
	}

	//==============================================================================
	// 화면을 누르면 최소 시간 전이라도 로딩 문이 열린다. 최소 시간이 0 이면 처음부터 열려 있다.
	//==============================================================================
	#[test]
	fn touchSkipsLoadingAndZeroDurationOpensImmediately() {
		let mut gameSceneBase = GameSceneBase::new();
		gameSceneBase.beginLoading(0.0);
		gameSceneBase.updateLoading(0.1, false);
		assert!(!gameSceneBase.getSceneBase().areTrackedAssetsFinished());
		gameSceneBase.updateLoading(0.2, true);
		assert!(gameSceneBase.isLoadTouchedToSkip());
		assert!(gameSceneBase.getSceneBase().areTrackedAssetsFinished());

		let mut immediateSceneBase = GameSceneBase::new();
		immediateSceneBase.setLoadingMinDurationMs(0.0);
		immediateSceneBase.beginLoading(5.0);
		assert!(immediateSceneBase.getSceneBase().areTrackedAssetsFinished());
		assert_eq!(immediateSceneBase.getLoadingProgress(5.0), 1.0);
	}

	//==============================================================================
	// 뷰 크기가 바뀌면 tick 이 세이프 에어리어를 다시 계산해 노드에 걸고 true 를 돌려준다. (인셋은 뷰 배율로 나눈다)
	//==============================================================================
	#[test]
	fn layoutFollowsViewSizeChanges() {
		let mut engine = Engine::new(EngineConfiguration::new());
		engine.getViewManagerMut().calculateViewRect(800, 600, 1.0);
		let mut gameSceneBase = GameSceneBase::new();
		gameSceneBase.setSafeAreaInsets(Some([10.0, 0.0, 20.0, 5.0]));
		gameSceneBase.initialize(&mut engine);
		assert_eq!(engine.getViewManager().getViewScaleMode(), ViewScaleMode::none);
		assert_eq!(engine.getViewManager().getViewSize(), Vector2::create(800.0, 600.0));
		let safeAreaNodeId = gameSceneBase.getSafeAreaNodeId().expect("safe area node");
		assert_eq!(gameSceneBase.getSceneBase().getNodeTree()[safeAreaNodeId].getName(), "safeArea");

		assert!(gameSceneBase.tick(&engine, 0.016));
		assert_eq!(gameSceneBase.getSafeAreaRect(), Rect::create(5.0, 10.0, 795.0, 570.0));
		let safeAreaNode = &gameSceneBase.getSceneBase().getNodeTree()[safeAreaNodeId];
		assert_eq!(safeAreaNode.getLocalPosition(), Vector2::create(5.0, 10.0));
		assert_eq!(safeAreaNode.getContentSize(), Vector2::create(795.0, 570.0));
		assert!(!gameSceneBase.tick(&engine, 0.016));

		engine.getViewManagerMut().calculateViewRect(1024, 768, 1.0);
		assert!(gameSceneBase.tick(&engine, 0.016));
		assert_eq!(gameSceneBase.getSafeAreaRect(), Rect::create(5.0, 10.0, 1019.0, 738.0));
		assert!(!gameSceneBase.tick(&engine, 0.016));

		gameSceneBase.setViewScaleMode(ViewScaleMode::referenceResolution, Some(&mut engine));
		assert_eq!(engine.getViewManager().getViewSize(), Vector2::create(1280.0, 720.0));
		assert!(gameSceneBase.tick(&engine, 0.016));
		assert_eq!(gameSceneBase.getSafeAreaRect(), Rect::create(5.0, 10.0, 1275.0, 690.0));
	}

	//==============================================================================
	// 세이프 에어리어 계산은 뷰 배율로 인셋을 나누고 음수 크기를 0 으로 막는다. CSS 패딩 글은 앞 정수만 읽는다.
	//==============================================================================
	#[test]
	fn safeAreaCalculation() {
		let safeAreaRect = GameSceneBase::calculateSafeAreaRect(Vector2::create(400.0, 300.0), 2.0, [40.0, 10.0, 20.0, 30.0]);
		assert_eq!(safeAreaRect, Rect::create(15.0, 20.0, 380.0, 270.0));
		let collapsedRect = GameSceneBase::calculateSafeAreaRect(Vector2::create(10.0, 10.0), 0.0, [20.0, 20.0, 20.0, 20.0]);
		assert_eq!(collapsedRect, Rect::create(20.0, 20.0, 0.0, 0.0));
		assert_eq!(parseLeadingInteger("34px"), 34.0);
		assert_eq!(parseLeadingInteger("12.7px"), 12.0);
		assert_eq!(parseLeadingInteger(""), 0.0);
		assert_eq!(parseLeadingInteger("-3px"), -3.0);
	}
}
