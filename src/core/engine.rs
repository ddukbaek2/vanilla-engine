//==============================================================================
// 엔진.
// - 원본은 캔버스를 잡고 requestAnimationFrame 으로 프레임을 돈다.
//   러스트 판은 창, GL 컨텍스트, 입력 이벤트, 화면 넘기기를 플랫폼 층(crate::platform)이 맡고,
//   여기 EngineRunner 는 플랫폼을 모른 채 씬과 프레임 흐름만 돈다. (데스크탑: platform/desktop.rs, 웹: platform/web.rs)
// - 씬에는 &mut Engine 을 넘긴다. 씬은 여기서 매니저(시간, 뷰, 입력)를 꺼내 쓴다. (원본 Engine.getEngine() 자리)
// - Graphic 은 플랫폼이 GL 컨텍스트를 만든 뒤 attachGraphic 으로 넘겨받고, 씬의 draw 계열에 따로 넘긴다.
// - 오디오 매니저를 원본처럼 생성할 때 만들어 든다. AudioContext(소리 장치)는 처음 쓸 때 만든다. 프레임마다 하는 오디오 일은 없다.
//   웹은 플랫폼이 click / touchstart / keydown 에서 resumeOnUserGesture, focus / visibilitychange 에서 resume 을 부른다.
//
// 매 프레임 (updateEngine):
// 1. graphic.applySettings()      # 뷰포트/프로젝션 갱신 + 상태 리셋 + 화면 비우기
// 2. timeManager.update()         # timeDelta 산출
// 3. inputManager.tick()
// 4. 씬마다 tick → preDraw → draw → postDraw (→ drawGizmos)
// 5. 통계 출력(옵션) → 1회성 입력 플래그 클리어(글 입력 줄 마무리 포함) → 묶음 내보내기 → (검증용 화면 저장)
// 화면 넘기기는 플랫폼이 updateEngine 다음에 한다.
//==============================================================================
use crate::base::color::Color;
use crate::base::platform::Platform;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::base::version::Version;
use crate::core::asset::AssetLoadHandle;
use crate::core::audiomanager::AudioManager;
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::image::Image;
use crate::core::inputmanager::InputManager;
use crate::core::scene::Scene;
use crate::core::timemanager::TimeManager;
use crate::core::viewmanager::{ViewManager, ViewScaleMode};
use crate::platform::{self, PlatformWindow};
use crate::resource::audioasset::AudioAsset;
use std::path::PathBuf;
use std::rc::Rc;
use web_time::Instant;

// 엔진 판. (원본 vanilla.js 0.3.2 를 옮긴 첫 판)
const ENGINE_VERSION: Version = Version::create(0, 3, 2);

//==============================================================================
// 엔진 설정.
//==============================================================================
pub struct EngineConfiguration {
	// 기준 해상도. (뷰 스케일 모드가 none 이 아니면 씬은 이 좌표계로 그린다)
	pub referenceResolutionSize: Vector2,
	// 뷰 스케일 모드.
	pub viewScaleMode: ViewScaleMode,
	// 창 제목.
	pub title: String,
	// 처음 창 크기. (논리 크기, 0 이면 기준 해상도)
	pub windowSize: Vector2,
	// 창 크기를 바꿀 수 있는지.
	pub isResizable: bool,
	// 화면 왼쪽 위에 FPS / 드로우 콜 출력.
	pub useStatistics: bool,
	// 최대 FPS. 0 이면 제한하지 않는다. (수직 동기를 켜면 화면 주사율을 따른다)
	pub maximumFramePerSecond: f32,
	// 수직 동기.
	pub useVerticalSync: bool,
	// 기본 폰트 후보 경로. (앞에서부터 읽히는 첫 파일. 원본 defaultFontUrl 자리)
	pub defaultFontPaths: Vec<PathBuf>,
	// 굵은 기본 폰트 후보 경로.
	pub defaultBoldFontPaths: Vec<PathBuf>,
	// 애셋 폴더 후보. (resolveAssetPath 가 앞에서부터 찾는다)
	pub assetDirectoryPaths: Vec<PathBuf>,
	// 창 아이콘 PNG. (애셋 폴더 기준 상대 경로, 비우면 기본 아이콘)
	pub windowIconPath: String,
	// 기본 폰트 파일 내용. (있으면 경로 후보보다 먼저 쓴다. 파일을 못 읽는 웹에서 미리 받아 넘긴다)
	pub defaultFontBytes: Option<Vec<u8>>,
	// 굵은 기본 폰트 파일 내용.
	pub defaultBoldFontBytes: Option<Vec<u8>>,
	// 웹에서 그릴 캔버스 id. (없으면 만들어 body 에 붙인다)
	pub canvasId: String,
	// 웹 기본 폰트 주소. (원본 defaultFontUrl. 웹에서 defaultFontBytes 가 없으면 여기서 받아 기본 폰트로 쓴다. 빈 글이면 받지 않는다)
	// - 기본값은 빈 글이다. (원본 기본값 DOSGothic 은 넣지 않는다) 아무 글꼴도 주지 않으면 원본처럼 브라우저 시스템 글꼴로 그린다.
	// - 게임 글꼴을 쓰려면 이 주소나 defaultFontBytes, FontAsset 으로 준다.
	// - 데스크탑은 쓰지 않고 defaultFontPaths 를 본다.
	pub defaultFontUrl: String,
	// 웹에서 touchend 와 pointerup 도 user gesture 로 보고 오디오를 재개한다. (원본과 같이 기본은 끔)
	pub resumeAudioOnTouchEnd: bool,
	// 웹에서 애셋 주소 앞에 붙일 것. (비우면 페이지 기준 상대 주소. 데스크탑은 assetDirectoryPaths 를 쓴다)
	pub assetBaseUrl: String,
}

impl EngineConfiguration {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> EngineConfiguration {
		EngineConfiguration {
			referenceResolutionSize: Vector2::create(1280.0, 720.0),
			viewScaleMode: ViewScaleMode::stretchShort,
			title: String::from("바닐라엔진"),
			windowSize: Vector2::zero(),
			isResizable: true,
			useStatistics: false,
			maximumFramePerSecond: 0.0,
			useVerticalSync: true,
			defaultFontPaths: vec![
				PathBuf::from("C:/Windows/Fonts/malgun.ttf"),
				PathBuf::from("C:/Windows/Fonts/segoeui.ttf"),
				PathBuf::from("C:/Windows/Fonts/arial.ttf"),
				PathBuf::from("/System/Library/Fonts/AppleSDGothicNeo.ttc"),
				PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
			],
			defaultBoldFontPaths: vec![
				PathBuf::from("C:/Windows/Fonts/malgunbd.ttf"),
				PathBuf::from("C:/Windows/Fonts/segoeuib.ttf"),
				PathBuf::from("C:/Windows/Fonts/arialbd.ttf"),
			],
			assetDirectoryPaths: Vec::new(),
			windowIconPath: String::new(),
			defaultFontBytes: None,
			defaultBoldFontBytes: None,
			canvasId: String::from("mainCanvas"),
			defaultFontUrl: String::new(),
			resumeAudioOnTouchEnd: false,
			assetBaseUrl: String::new(),
		}
	}
}

impl Default for EngineConfiguration {
	fn default() -> EngineConfiguration {
		EngineConfiguration::new()
	}
}

//==============================================================================
// 엔진.
//==============================================================================
pub struct Engine {
	engineConfiguration: EngineConfiguration,
	timeManager: TimeManager,
	viewManager: ViewManager,
	inputManager: InputManager,
	audioManager: AudioManager,
	pub(crate) platformWindow: Option<PlatformWindow>,
	frameNumber: u64,
	version: Version,
	isExitRequested: bool,
	pendingSceneList: Vec<Box<dyn Scene>>,
	isUnloadAllScenesRequested: bool,
	pendingFontRegistrationList: Vec<(String, bool, AssetLoadHandle)>,
	pendingDefaultFontHandle: Option<AssetLoadHandle>,
	platform: Platform,
}

impl Engine {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(engineConfiguration: EngineConfiguration) -> Engine {
		let referenceResolutionSize = engineConfiguration.referenceResolutionSize;
		let viewScaleMode = engineConfiguration.viewScaleMode;
		Engine {
			engineConfiguration,
			timeManager: TimeManager::new(),
			viewManager: ViewManager::new(referenceResolutionSize, viewScaleMode),
			inputManager: InputManager::new(),
			audioManager: AudioManager::new(),
			platformWindow: None,
			frameNumber: 0,
			version: ENGINE_VERSION,
			isExitRequested: false,
			pendingSceneList: Vec::new(),
			isUnloadAllScenesRequested: false,
			pendingFontRegistrationList: Vec::new(),
			pendingDefaultFontHandle: None,
			platform: Platform::new(),
		}
	}

	//==============================================================================
	// 시작.
	// - 데스크탑: 창을 닫거나 exitApplication 을 부를 때까지 돌아오지 않는다.
	// - 웹: 캔버스와 WebGL2 를 잡고 requestAnimationFrame 을 걸어 둔 뒤 바로 돌아온다.
	//==============================================================================
	pub fn run(self, scene: Box<dyn Scene>) -> Result<(), String> {
		platform::run(EngineRunner::new(self, scene))
	}

	//==============================================================================
	// 씬 불러오기. (이번 프레임이 끝난 뒤 목록에 더한다)
	//==============================================================================
	pub fn loadScene(&mut self, scene: Box<dyn Scene>) {
		self.pendingSceneList.push(scene);
	}

	//==============================================================================
	// 모든 씬 내리기. (이번 프레임이 끝난 뒤, loadScene 과 같이 부르면 내린 다음 불러온다)
	//==============================================================================
	pub fn unloadAllScenes(&mut self) {
		self.isUnloadAllScenesRequested = true;
	}

	//==============================================================================
	// 애셋 경로 찾기. (설정의 애셋 폴더 후보에서 처음 있는 것, 없으면 첫 후보 기준)
	//==============================================================================
	pub fn resolveAssetPath(&self, relativePath: &str) -> PathBuf {
		for assetDirectoryPath in &self.engineConfiguration.assetDirectoryPaths {
			let candidatePath = assetDirectoryPath.join(relativePath);
			if candidatePath.exists() {
				return candidatePath;
			}
		}
		match self.engineConfiguration.assetDirectoryPaths.first() {
			Some(firstAssetDirectoryPath) => {
				firstAssetDirectoryPath.join(relativePath)
			}
			None => {
				PathBuf::from(relativePath)
			}
		}
	}

	//==============================================================================
	// 이미지 애셋 읽기. (원본 new ImageAsset() + load(path))
	//==============================================================================
	pub fn loadImage(&self, relativePath: &str) -> Result<Rc<Image>, String> {
		let imagePath = self.resolveAssetPath(relativePath);
		Image::load(&imagePath)
	}

	//==============================================================================
	// 애셋 바이트 부탁. (애셋들의 load 가 부른다)
	// - 데스크탑: 애셋 폴더 후보에서 파일을 찾아 그 자리에서 읽고 끝낸다.
	// - 웹: assetBaseUrl + 상대 경로를 fetch 하고, 받으면 끝낸다. (그 사이 프레임은 계속 돈다)
	//==============================================================================
	pub fn requestAssetBytes(&self, relativePath: &str) -> AssetLoadHandle {
		let loadHandle = AssetLoadHandle::createPending(relativePath);
		#[cfg(not(target_arch = "wasm32"))]
		let assetLocation = self.resolveAssetPath(relativePath).to_string_lossy().into_owned();
		#[cfg(target_arch = "wasm32")]
		let assetLocation = format!("{}{}", self.engineConfiguration.assetBaseUrl, relativePath);
		platform::requestAssetBytes(assetLocation, loadHandle.clone());
		loadHandle
	}

	//==============================================================================
	// 글꼴 바이트가 오면 이름으로 등록해 달라고 맡기기. (FontAsset 이 부른다. 등록은 프레임 시작에 한다)
	//==============================================================================
	pub fn registerFontWhenLoaded(&mut self, family: &str, isBold: bool, loadHandle: AssetLoadHandle) {
		self.pendingFontRegistrationList.push((family.to_string(), isBold, loadHandle));
	}

	//==============================================================================
	// 오디오 애셋 읽기. (원본 new AudioAsset() + load(path), 애셋 폴더 기준 경로)
	// - 데스크탑: 그 자리에서 읽고 디코딩한다. 실패하면 Err.
	// - 웹: fetch 와 디코딩을 걸어 두고 바로 애셋을 돌려준다. 끝나면 isLoaded() 가 참이 된다. (실패는 getLoadError())
	//==============================================================================
	pub fn loadAudioAsset(&self, relativePath: &str) -> Result<Rc<AudioAsset>, String> {
		#[cfg(not(target_arch = "wasm32"))]
		let audioAssetPath = self.resolveAssetPath(relativePath);
		#[cfg(target_arch = "wasm32")]
		let audioAssetPath = PathBuf::from(format!("{}{}", self.engineConfiguration.assetBaseUrl, relativePath));
		let audioAsset = Rc::new(AudioAsset::new());
		audioAsset.load(&audioAssetPath)?;
		Ok(audioAsset)
	}

	//==============================================================================
	// 오디오 컨텍스트 재개. (focus, visibilitychange 같은 user gesture 가 아닌 복귀. gesture 전이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn resume(&self) {
		let audioManager = self.getAudioManager();
		audioManager.resumeContext();
	}

	//==============================================================================
	// user gesture (click / touch / keydown) 핸들러.
	// AudioManager 에 user gesture 발생을 알리고 컨텍스트 재개를 시도한다.
	//==============================================================================
	pub fn resumeOnUserGesture(&self) {
		let audioManager = self.getAudioManager();
		audioManager.markUserGesture();
		audioManager.resumeContext();
	}

	//==============================================================================
	// 종료 요청.
	//==============================================================================
	pub fn exitApplication(&mut self) {
		self.isExitRequested = true;
	}

	//==============================================================================
	// 종료 요청 여부.
	//==============================================================================
	pub fn isExitRequested(&self) -> bool {
		self.isExitRequested
	}

	//==============================================================================
	// 커서 보이기 설정.
	//==============================================================================
	pub fn setVisibleCursor(&self, isVisible: bool) {
		platform::setCursorVisible(self.platformWindow.as_ref(), isVisible);
	}

	//==============================================================================
	// 창 제목 설정.
	//==============================================================================
	pub fn setTitle(&self, title: &str) {
		platform::setTitle(self.platformWindow.as_ref(), title);
	}

	//==============================================================================
	// 시스템 클립보드에 글 쓰기. (데스크탑 arboard, 웹 navigator.clipboard.writeText. 원본 UIInputField.writeClipboard 자리)
	//==============================================================================
	pub fn writeClipboardText(&self, text: &str) {
		platform::writeClipboardText(text);
	}

	//==============================================================================
	// 시스템 클립보드 글 읽기. (데스크탑만 그 자리에서 읽는다. 웹은 비동기라 None, 웹 붙여넣기는 플랫폼이 읽어 글 입력 줄에 넣는다)
	//==============================================================================
	pub fn readClipboardText(&self) -> Option<String> {
		platform::readClipboardText()
	}

	pub fn getEngineConfiguration(&self) -> &EngineConfiguration {
		&self.engineConfiguration
	}

	pub fn getTimeManager(&self) -> &TimeManager {
		&self.timeManager
	}

	pub fn getTimeManagerMut(&mut self) -> &mut TimeManager {
		&mut self.timeManager
	}

	pub fn getViewManager(&self) -> &ViewManager {
		&self.viewManager
	}

	pub fn getViewManagerMut(&mut self) -> &mut ViewManager {
		&mut self.viewManager
	}

	pub fn getInputManager(&self) -> &InputManager {
		&self.inputManager
	}

	pub fn getInputManagerMut(&mut self) -> &mut InputManager {
		&mut self.inputManager
	}

	//==============================================================================
	// 오디오 매니저. (메서드가 &self 라 컴포넌트가 받는 &Engine 으로도 플레이어를 만들 수 있다)
	//==============================================================================
	pub fn getAudioManager(&self) -> &AudioManager {
		&self.audioManager
	}

	//==============================================================================
	// 오디오 매니저. (가변)
	//==============================================================================
	pub fn getAudioManagerMut(&mut self) -> &mut AudioManager {
		&mut self.audioManager
	}

	pub fn getPlatform(&self) -> &Platform {
		&self.platform
	}

	pub fn getFrameNumber(&self) -> u64 {
		self.frameNumber
	}

	pub fn getVersionString(&self) -> String {
		self.version.toString()
	}
}

//==============================================================================
// 화면 저장 요청. (검증용, 환경 변수로 켠다)
// - VANILLA_CAPTURE_PATH:   저장 경로. "{frame}" 은 프레임 번호로 바뀐다. 예: shots/shot_{frame}.png
// - VANILLA_CAPTURE_FRAMES: 저장할 프레임 번호, 쉼표로 여럿. (기본 120)
// - VANILLA_CAPTURE_EXIT:   1 이면 마지막 저장 뒤 끝낸다.
//==============================================================================
struct CaptureRequest {
	outputPath: String,
	frameNumbers: Vec<u64>,
	isExitAfterLastCapture: bool,
}

impl CaptureRequest {
	//==============================================================================
	// 환경 변수에서 읽기. (경로가 없으면 None)
	//==============================================================================
	fn createFromEnvironment() -> Option<CaptureRequest> {
		let outputPath = std::env::var("VANILLA_CAPTURE_PATH").ok()?;
		let frameNumbersText = std::env::var("VANILLA_CAPTURE_FRAMES").unwrap_or_else(|_| String::from("120"));
		let mut frameNumbers: Vec<u64> = frameNumbersText.split(',').filter_map(|part| part.trim().parse::<u64>().ok()).collect();
		frameNumbers.sort();
		let isExitAfterLastCapture = std::env::var("VANILLA_CAPTURE_EXIT").map(|value| value == "1").unwrap_or(false);
		Some(CaptureRequest {
			outputPath,
			frameNumbers,
			isExitAfterLastCapture,
		})
	}
}

//==============================================================================
// 엔진 실행자. (엔진, 그래픽, 씬 목록을 들고 프레임을 돈다. 플랫폼이 들고 부른다)
//==============================================================================
pub struct EngineRunner {
	pub(crate) engine: Engine,
	graphic: Option<Graphic>,
	sceneList: Vec<Box<dyn Scene>>,
	startInstant: Instant,
	captureRequest: Option<CaptureRequest>,
}

impl EngineRunner {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub(crate) fn new(mut engine: Engine, scene: Box<dyn Scene>) -> EngineRunner {
		engine.loadScene(scene);
		EngineRunner {
			engine,
			graphic: None,
			sceneList: Vec::new(),
			startInstant: Instant::now(),
			captureRequest: CaptureRequest::createFromEnvironment(),
		}
	}

	//==============================================================================
	// 그래픽 넘겨받기. (플랫폼이 GL 컨텍스트를 만든 뒤 한 번. 기본 폰트도 이때 읽는다)
	//==============================================================================
	pub(crate) fn attachGraphic(&mut self, mut graphic: Graphic) {
		self.loadDefaultFonts(&mut graphic);

		// 웹: 기본 폰트가 아직 없고 defaultFontUrl 이 있으면 거기서 받는다. (받는 동안은 시스템 글꼴로 그린다)
		#[cfg(target_arch = "wasm32")]
		{
			let defaultFontUrl = self.engine.engineConfiguration.defaultFontUrl.clone();
			let hasDefaultFont = graphic.getTextStringTextureCache().hasDefaultFont();
			if !hasDefaultFont && !defaultFontUrl.is_empty() {
				let loadHandle = AssetLoadHandle::createPending(&defaultFontUrl);
				platform::requestAssetBytes(defaultFontUrl, loadHandle.clone());
				self.engine.pendingDefaultFontHandle = Some(loadHandle);
			}
		}
		self.graphic = Some(graphic);
	}

	//==============================================================================
	// 요청된 프레임이면 화면을 PNG 로 저장. (화면을 넘기기 전에 부른다)
	//==============================================================================
	fn captureFrameIfRequested(&mut self) {
		let frameNumber = self.engine.frameNumber;
		let captureRequest = match &mut self.captureRequest {
			Some(captureRequest) => {
				captureRequest
			}
			None => {
				return;
			}
		};
		if !captureRequest.frameNumbers.contains(&frameNumber) {
			return;
		}
		let graphic = match &self.graphic {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let (width, height, pixels) = graphic.readFramebufferPixels();
		let outputPath = captureRequest.outputPath.replace("{frame}", &frameNumber.to_string());
		match image::save_buffer(&outputPath, &pixels, width, height, image::ExtendedColorType::Rgba8) {
			Ok(()) => {
				eprintln!("Engine: 화면 저장 {} ({}x{})", outputPath, width, height);
			}
			Err(error) => {
				eprintln!("Engine: 화면 저장 실패 {}: {}", outputPath, error);
			}
		}
		captureRequest.frameNumbers.retain(|requestedFrameNumber| *requestedFrameNumber != frameNumber);
		if captureRequest.frameNumbers.is_empty() && captureRequest.isExitAfterLastCapture {
			self.engine.exitApplication();
		}
	}

	//==============================================================================
	// 기본 폰트 읽기. (후보 경로 가운데 처음 읽히는 것)
	//==============================================================================
	fn loadDefaultFonts(&self, graphic: &mut Graphic) {
		let engineConfiguration = self.engine.getEngineConfiguration();
		let textStringTextureCache = graphic.getTextStringTextureCache();
		let fontSourceList = [
			(false, &engineConfiguration.defaultFontBytes, &engineConfiguration.defaultFontPaths),
			(true, &engineConfiguration.defaultBoldFontBytes, &engineConfiguration.defaultBoldFontPaths),
		];
		for (isBold, fontBytes, fontPaths) in fontSourceList {
			// 미리 받은 내용이 있으면 그것부터.
			if let Some(fontBytes) = fontBytes {
				match textStringTextureCache.setDefaultFont(isBold, fontBytes.clone()) {
					Ok(()) => {
						continue;
					}
					Err(error) => {
						eprintln!("Engine: {} (미리 받은 폰트)", error);
					}
				}
			}
			for fontPath in fontPaths.iter() {
				let fontBytes = match std::fs::read(fontPath) {
					Ok(fontBytes) => {
						fontBytes
					}
					Err(_) => {
						continue;
					}
				};
				match textStringTextureCache.setDefaultFont(isBold, fontBytes) {
					Ok(()) => {
						break;
					}
					Err(error) => {
						eprintln!("Engine: {} ({})", error, fontPath.display());
					}
				}
			}
		}
		if !textStringTextureCache.hasDefaultFont() {
			eprintln!("Engine: 기본 폰트를 찾지 못했습니다. 글자가 보이지 않습니다.");
		}
	}

	//==============================================================================
	// 씬마다 해상도 변경 알림.
	//==============================================================================
	pub(crate) fn notifySceneResize(&mut self) {
		let canvasNativeSize = self.engine.viewManager.getCanvasNativeSize();
		let mut sceneList = std::mem::take(&mut self.sceneList);
		for scene in sceneList.iter_mut() {
			scene.resize(&mut self.engine, canvasNativeSize);
		}
		self.sceneList = sceneList;
	}

	//==============================================================================
	// 바이트가 온 글꼴을 글자 캐시에 등록. (프레임 시작에)
	//==============================================================================
	fn registerPendingFonts(&mut self) {
		self.registerPendingDefaultFont();
		if self.engine.pendingFontRegistrationList.is_empty() {
			return;
		}
		let graphic = match self.graphic.as_mut() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let pendingFontRegistrationList = std::mem::take(&mut self.engine.pendingFontRegistrationList);
		for (family, isBold, loadHandle) in pendingFontRegistrationList {
			if !loadHandle.isFinished() {
				self.engine.pendingFontRegistrationList.push((family, isBold, loadHandle));
				continue;
			}
			let fontBytes = match loadHandle.getBytes() {
				Some(fontBytes) => {
					fontBytes
				}
				None => {
					continue;
				}
			};
			let textStringTextureCache = graphic.getTextStringTextureCache();
			if let Err(error) = textStringTextureCache.registerFont(&family, isBold, fontBytes.to_vec()) {
				eprintln!("Engine: {} ({})", error, loadHandle.getAssetPath());
			}
		}
	}

	//==============================================================================
	// defaultFontUrl 에서 받은 바이트를 기본 폰트(보통, 굵게 둘 다)로 등록. (웹)
	//==============================================================================
	fn registerPendingDefaultFont(&mut self) {
		let isFinished = match &self.engine.pendingDefaultFontHandle {
			Some(loadHandle) => {
				loadHandle.isFinished()
			}
			None => {
				return;
			}
		};
		if !isFinished {
			return;
		}
		let loadHandle = match self.engine.pendingDefaultFontHandle.take() {
			Some(loadHandle) => {
				loadHandle
			}
			None => {
				return;
			}
		};
		let fontBytes = match loadHandle.getBytes() {
			Some(fontBytes) => {
				fontBytes
			}
			None => {
				return;
			}
		};
		let graphic = match self.graphic.as_mut() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let textStringTextureCache = graphic.getTextStringTextureCache();
		for isBold in [false, true] {
			if let Err(error) = textStringTextureCache.setDefaultFont(isBold, fontBytes.to_vec()) {
				eprintln!("Engine: {} ({})", error, loadHandle.getAssetPath());
			}
		}
	}

	//==============================================================================
	// 미뤄 둔 씬 내리기 / 불러오기.
	//==============================================================================
	pub(crate) fn applyPendingScenes(&mut self) {
		if self.engine.isUnloadAllScenesRequested {
			self.engine.isUnloadAllScenesRequested = false;
			let mut sceneList = std::mem::take(&mut self.sceneList);
			for scene in sceneList.iter_mut() {
				scene.destroy(&mut self.engine);
			}
		}
		while !self.engine.pendingSceneList.is_empty() {
			let pendingSceneList = std::mem::take(&mut self.engine.pendingSceneList);
			for mut scene in pendingSceneList {
				scene.create(&mut self.engine);
				initializeSceneIfReady(&mut self.engine, scene.as_mut());
				self.sceneList.push(scene);
			}
		}
	}

	//==============================================================================
	// 엔진 갱신. (한 프레임)
	//==============================================================================
	pub(crate) fn updateEngine(&mut self) {
		// 바이트가 온 글꼴 등록.
		self.registerPendingFonts();
		let graphic = match self.graphic.as_mut() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};

		// 렌더러 처리.
		let canvasPixelSize = self.engine.viewManager.getCanvasPixelSize();
		graphic.applySettings(canvasPixelSize.x as i32, canvasPixelSize.y as i32);

		// 시간 처리.
		let secondsSinceStartup = self.startInstant.elapsed().as_secs_f64();
		self.engine.timeManager.update(secondsSinceStartup);
		let timeDelta = self.engine.timeManager.getTimeDelta();

		// 입력 처리.
		self.engine.inputManager.tick(timeDelta);

		// 그리기 통계. (지난 프레임 값을 넘겨 두고 이번 프레임을 새로 센다)
		graphic.beginFrameStatistics();

		// 씬 처리.
		let mut sceneList = std::mem::take(&mut self.sceneList);
		for scene in sceneList.iter_mut() {
			// 맡긴 애셋을 기다리는 씬은 로딩 화면만. (다 읽혔으면 이번 프레임부터 돈다)
			if !scene.getSceneBase().isLoaded() {
				initializeSceneIfReady(&mut self.engine, scene.as_mut());
			}
			if !scene.getSceneBase().isLoaded() {
				scene.drawOnLoad(&mut self.engine, graphic);
				continue;
			}

			// 갱신.
			scene.tick(&mut self.engine, timeDelta);

			// 출력.
			scene.preDraw(&mut self.engine, graphic);
			scene.draw(&mut self.engine, graphic);
			scene.postDraw(&mut self.engine, graphic);

			let isGizmoVisible = scene.getSceneBase().isGizmoVisible();
			if isGizmoVisible {
				scene.drawGizmos(&mut self.engine, graphic);
			}
		}
		sceneList.append(&mut self.sceneList);
		self.sceneList = sceneList;

		// 개발 정보 출력.
		if self.engine.engineConfiguration.useStatistics {
			drawStatistics(&self.engine, graphic);
		}

		// 입력 관련해서 상태 유지가 아닌, 현재 프레임이 끝난 후에는 다음 프레임에서는 상태를 유지하지 않음. (1회성)
		let inputManager = &mut self.engine.inputManager;
		inputManager.setTouchPressed(false);
		inputManager.setTouchReleased(false);
		inputManager.setTouchCancelled(false);
		inputManager.setTouchMoved(false);
		inputManager.clearWheelDelta();
		inputManager.updateFrameSnapshot();
		inputManager.getTextInputMut().finishFrame();

		// 프레임 마무리. (묶음을 내보내고, 검증용 화면 저장은 플랫폼이 화면을 넘기기 전에 한다)
		graphic.endFrame();
		self.captureFrameIfRequested();
		self.engine.frameNumber += 1;

		// 씬 목록 바꾸기.
		self.applyPendingScenes();
	}
}

//==============================================================================
// 개발 정보 출력. (왼쪽 위에 FPS, 드로우 콜, 정점 수, 화면 밖이라 거른 노드 수)
//==============================================================================
fn drawStatistics(engine: &Engine, graphic: &mut Graphic) {
	let viewManager = engine.getViewManager();
	let timeManager = engine.getTimeManager();
	viewManager.applyCanvasNativeRect(graphic);

	let framePerSecond = timeManager.getFramePerSecond();
	let drawCallCount = graphic.getLastFrameDrawCallCount();
	let vertexCount = graphic.getLastFrameVertexCount();
	let culledNodeCount = graphic.getLastFrameCulledNodeCount();
	let statisticsText = format!("FPS {}  DrawCall {}  Vertex {}  Culled {}", framePerSecond, drawCallCount, vertexCount, culledNodeCount);

	graphic.setFontString("14px default");
	let textMetrics = graphic.measureText(&statisticsText);
	graphic.setFillColor(Color::create(0.0, 0.0, 0.0, 0.6));
	graphic.drawRect(Rect::create(4.0, 4.0, textMetrics.width + 12.0, 22.0));
	graphic.setTextAlign(TextAlign::left);
	graphic.setTextBaseline(TextBaseline::middle);
	graphic.setFillColor(Color::white());
	graphic.drawFillText(&statisticsText, 10.0, 15.0);
}

//==============================================================================
// 씬이 맡긴 애셋이 다 읽혔으면 initialize 하고 불러온 것으로 친다. (원본 load 다음 setLoaded, initialize)
//==============================================================================
fn initializeSceneIfReady(engine: &mut Engine, scene: &mut dyn Scene) {
	if scene.getSceneBase().isLoaded() || !scene.getSceneBase().areTrackedAssetsFinished() {
		return;
	}
	scene.getSceneBaseMut().clearTrackedAssets();
	scene.initialize(engine);
	scene.getSceneBaseMut().setLoaded(true);
	let canvasNativeSize = engine.viewManager.getCanvasNativeSize();
	scene.resize(engine, canvasNativeSize);
}
