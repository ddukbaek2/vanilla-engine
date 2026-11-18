//==============================================================================
// 데스크탑 플랫폼. (윈도우, 맥, 리눅스)
// - 창(winit)과 GL 3.3 코어 컨텍스트(glutin)를 만들고, 창의 다시 그리기 요청마다 엔진 프레임을 하나 돈다.
// - 창 이벤트를 입력 관리자 꼴로 바꿔 넣는다. (키는 W3C KeyboardEvent.code 와 같은 이름, 마우스 왼쪽 단추는 터치)
// - 엔진 본체(EngineRunner)는 창을 모른다. 여기서 프레임을 부르고, 그린 뒤 화면을 넘긴다.
// - 글 입력(키 글자, 편집 키, IME 조합)과 클립보드는 desktop/textinput.rs 가 맡는다. 오른쪽 단추는 입력 필드 문맥 메뉴 요청이다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::EngineRunner;
use crate::core::graphic::Graphic;
use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version as GlVersion};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Duration;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Icon, Window, WindowId};

mod textinput;
pub use textinput::{readClipboardText, writeClipboardText};

// 엔진이 들고 있는 창 손잡이. (커서, 제목을 바꿀 때)
pub type PlatformWindow = Rc<Window>;

// 휠 한 줄을 픽셀로 볼 때의 값. (브라우저 deltaMode 줄 단위 환산과 같은 자리)
const WHEEL_LINE_PIXEL_SIZE: f32 = 40.0;

//==============================================================================
// 시작. (창을 닫거나 exitApplication 을 부를 때까지 돌아오지 않는다)
//==============================================================================
pub fn run(engineRunner: EngineRunner) -> Result<(), String> {
	let eventLoop = EventLoop::new().map_err(|error| format!("이벤트 루프 생성 실패: {}", error))?;
	eventLoop.set_control_flow(ControlFlow::Poll);
	let mut desktopApplication = DesktopApplication {
		engineRunner,
		windowState: None,
		nextFrameInstant: Instant::now(),
		fatalError: None,
		textInputState: textinput::DesktopTextInputState::new(),
	};
	eventLoop.run_app(&mut desktopApplication).map_err(|error| format!("이벤트 루프 실행 실패: {}", error))?;
	match desktopApplication.fatalError.take() {
		Some(fatalError) => {
			Err(fatalError)
		}
		None => {
			Ok(())
		}
	}
}

//==============================================================================
// 커서 보이기 설정.
//==============================================================================
pub fn setCursorVisible(platformWindow: Option<&PlatformWindow>, isVisible: bool) {
	if let Some(window) = platformWindow {
		window.set_cursor_visible(isVisible);
	}
}

//==============================================================================
// 창 제목 설정.
//==============================================================================
pub fn setTitle(platformWindow: Option<&PlatformWindow>, title: &str) {
	if let Some(window) = platformWindow {
		window.set_title(title);
	}
}

//==============================================================================
// 애셋 바이트 읽기. (파일을 그 자리에서 읽고 끝낸다)
//==============================================================================
pub fn requestAssetBytes(assetLocation: String, loadHandle: AssetLoadHandle) {
	let result = std::fs::read(&assetLocation).map_err(|error| format!("애셋 읽기 실패 ({}): {}", assetLocation, error));
	loadHandle.finish(result);
}

//==============================================================================
// 창과 GL 상태.
//==============================================================================
struct WindowState {
	window: Rc<Window>,
	surface: Surface<WindowSurface>,
	context: PossiblyCurrentContext,
}

//==============================================================================
// 이벤트 루프에 물리는 데스크탑 실행자.
//==============================================================================
struct DesktopApplication {
	engineRunner: EngineRunner,
	windowState: Option<WindowState>,
	nextFrameInstant: Instant,
	fatalError: Option<String>,
	textInputState: textinput::DesktopTextInputState,
}

impl DesktopApplication {
	//==============================================================================
	// 창과 GL 컨텍스트, 그래픽 만들기.
	//==============================================================================
	fn createWindow(&mut self, eventLoop: &ActiveEventLoop) -> Result<(), String> {
		let engine = &self.engineRunner.engine;
		let engineConfiguration = engine.getEngineConfiguration();
		let windowSize = if engineConfiguration.windowSize.x > 0.0 && engineConfiguration.windowSize.y > 0.0 {
			engineConfiguration.windowSize
		}
		else {
			engineConfiguration.referenceResolutionSize
		};
		let mut windowAttributes = Window::default_attributes()
			.with_title(engineConfiguration.title.clone())
			.with_inner_size(LogicalSize::new(windowSize.x as f64, windowSize.y as f64))
			.with_resizable(engineConfiguration.isResizable);
		// 검증용: VANILLA_WINDOW_POSITION=x,y 면 창을 그 자리(물리 픽셀)에 띄운다. (화면 밖에 두고 잴 때)
		if let Ok(windowPositionText) = std::env::var("VANILLA_WINDOW_POSITION") {
			let positionList: Vec<i32> = windowPositionText.split(',').filter_map(|part| part.trim().parse::<i32>().ok()).collect();
			if positionList.len() == 2 {
				windowAttributes = windowAttributes.with_position(winit::dpi::PhysicalPosition::new(positionList[0], positionList[1]));
			}
		}
		if !engineConfiguration.windowIconPath.is_empty() {
			let windowIconPath = engine.resolveAssetPath(&engineConfiguration.windowIconPath);
			match loadWindowIcon(&windowIconPath) {
				Ok(windowIcon) => {
					windowAttributes = windowAttributes.with_window_icon(Some(windowIcon));
				}
				Err(error) => {
					eprintln!("Engine: {}", error);
				}
			}
		}

		// 깊이 24 / 스텐실 8 비트가 있는 설정 가운데 멀티샘플 4 에 가까운 것. (원본 WebGL 컨텍스트의 기본 depth: true 와 안티에일리어싱 자리, 깊이는 3D 가 쓴다)
		let configTemplateBuilder = ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(24).with_stencil_size(8);
		let displayBuilder = DisplayBuilder::new().with_window_attributes(Some(windowAttributes));
		let (window, glConfig) = displayBuilder.build(eventLoop, configTemplateBuilder, pickConfig).map_err(|error| format!("GL 설정 고르기 실패: {}", error))?;
		let window = window.ok_or_else(|| String::from("창 생성 실패"))?;

		let rawWindowHandle = window.window_handle().ok().map(|windowHandle| windowHandle.as_raw());
		let glDisplay = glConfig.display();
		let contextAttributes = ContextAttributesBuilder::new()
			.with_context_api(ContextApi::OpenGl(Some(GlVersion::new(3, 3))))
			.with_profile(GlProfile::Core)
			.build(rawWindowHandle);
		let notCurrentContext = unsafe {
			glDisplay.create_context(&glConfig, &contextAttributes)
		}.map_err(|error| format!("GL 3.3 코어 컨텍스트 생성 실패: {}", error))?;
		let surfaceAttributes = window.build_surface_attributes(SurfaceAttributesBuilder::<WindowSurface>::new()).map_err(|error| format!("서피스 속성 실패: {}", error))?;
		let surface = unsafe {
			glDisplay.create_window_surface(&glConfig, &surfaceAttributes)
		}.map_err(|error| format!("서피스 생성 실패: {}", error))?;
		let context = notCurrentContext.make_current(&surface).map_err(|error| format!("컨텍스트 활성화 실패: {}", error))?;

		let useVerticalSync = engineConfiguration.useVerticalSync;
		let swapInterval = if useVerticalSync {
			SwapInterval::Wait(NonZeroU32::new(1).expect("one"))
		}
		else {
			SwapInterval::DontWait
		};
		if let Err(error) = surface.set_swap_interval(&context, swapInterval) {
			eprintln!("Engine: 수직 동기 설정 실패: {}", error);
		}

		let gl = unsafe {
			glow::Context::from_loader_function_cstr(|symbolName| glDisplay.get_proc_address(symbolName) as *const _)
		};
		let graphic = Graphic::new(gl)?;

		let window = Rc::new(window);
		self.engineRunner.attachGraphic(graphic);
		self.engineRunner.engine.platformWindow = Some(window.clone());
		self.windowState = Some(WindowState {
			window,
			surface,
			context,
		});
		self.resize();
		Ok(())
	}

	//==============================================================================
	// 해상도 변경됨. (서피스 크기를 맞추고 뷰 영역을 다시 계산한 뒤 씬에 알린다)
	//==============================================================================
	fn resize(&mut self) {
		let windowState = match &self.windowState {
			Some(windowState) => {
				windowState
			}
			None => {
				return;
			}
		};
		let physicalSize = windowState.window.inner_size();
		let scaleFactor = windowState.window.scale_factor() as f32;
		if let (Some(width), Some(height)) = (NonZeroU32::new(physicalSize.width), NonZeroU32::new(physicalSize.height)) {
			windowState.surface.resize(&windowState.context, width, height);
		}
		self.engineRunner.engine.getViewManagerMut().calculateViewRect(physicalSize.width, physicalSize.height, scaleFactor);
		self.engineRunner.notifySceneResize();
	}

	//==============================================================================
	// 창 안쪽 물리 좌표 → 입력 위치 갱신.
	//==============================================================================
	fn updateInputPosition(&mut self, physicalX: f64, physicalY: f64) {
		let engine = &mut self.engineRunner.engine;
		let devicePixelRatio = engine.getViewManager().getDevicePixelRatio();
		let canvasNativeInputPosition = Vector2::create(physicalX as f32 / devicePixelRatio, physicalY as f32 / devicePixelRatio);
		let viewInputPosition = engine.getViewManager().canvasPositionToViewPosition(canvasNativeInputPosition);
		let inputManager = engine.getInputManagerMut();
		inputManager.setCanvasNativeInputPosition(canvasNativeInputPosition);
		inputManager.setViewInputPosition(viewInputPosition);
	}

	//==============================================================================
	// 한 프레임 돌고 화면 넘기기.
	//==============================================================================
	fn updateFrame(&mut self) {
		if self.windowState.is_none() {
			return;
		}
		self.engineRunner.updateEngine();
		if let Some(windowState) = &self.windowState {
			self.textInputState.applyActiveField(&windowState.window, self.engineRunner.engine.getInputManager().getTextInput());
			if let Err(error) = windowState.surface.swap_buffers(&windowState.context) {
				eprintln!("Engine: 화면 넘기기 실패: {}", error);
			}
		}
	}
}

impl ApplicationHandler for DesktopApplication {
	//==============================================================================
	// 창 만들 차례. (데스크탑에서는 시작할 때 한 번)
	//==============================================================================
	fn resumed(&mut self, eventLoop: &ActiveEventLoop) {
		if self.windowState.is_some() {
			return;
		}
		if let Err(error) = self.createWindow(eventLoop) {
			eprintln!("Engine: {}", error);
			self.fatalError = Some(error);
			eventLoop.exit();
			return;
		}
		self.engineRunner.applyPendingScenes();
	}

	//==============================================================================
	// 창 이벤트.
	//==============================================================================
	fn window_event(&mut self, eventLoop: &ActiveEventLoop, _windowId: WindowId, windowEvent: WindowEvent) {
		match windowEvent {
			WindowEvent::CloseRequested => {
				eventLoop.exit();
			}
			WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
				self.resize();
			}
			WindowEvent::Focused(isFocused) => {
				if !isFocused {
					self.engineRunner.engine.getInputManagerMut().clear();
				}
			}
			WindowEvent::KeyboardInput { event: keyEvent, .. } => {
				if let PhysicalKey::Code(keyCode) = keyEvent.physical_key {
					let key = format!("{:?}", keyCode);
					let inputManager = self.engineRunner.engine.getInputManagerMut();
					match keyEvent.state {
						ElementState::Pressed => {
							inputManager.pushKey(&key);
						}
						ElementState::Released => {
							inputManager.popKey(&key);
						}
					}
				}
				self.textInputState.handleKeyEvent(&keyEvent, self.engineRunner.engine.getInputManagerMut().getTextInputMut());
			}
			WindowEvent::ModifiersChanged(modifiers) => {
				self.textInputState.setModifiers(modifiers.state());
			}
			WindowEvent::Ime(ime) => {
				self.textInputState.handleImeEvent(ime, self.engineRunner.engine.getInputManagerMut().getTextInputMut());
			}
			WindowEvent::CursorMoved { position, .. } => {
				self.updateInputPosition(position.x, position.y);
				self.engineRunner.engine.getInputManagerMut().setTouchMoved(true);
			}
			WindowEvent::CursorLeft { .. } => {
				let inputManager = self.engineRunner.engine.getInputManagerMut();
				if inputManager.isTouchDown() {
					inputManager.setTouchDown(false);
					inputManager.setTouchCancelled(true);
				}
			}
			WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
				let inputManager = self.engineRunner.engine.getInputManagerMut();
				match state {
					ElementState::Pressed => {
						inputManager.setTouchPressed(true);
						inputManager.setTouchDown(true);
					}
					ElementState::Released => {
						inputManager.setTouchReleased(true);
						inputManager.setTouchDown(false);
					}
				}
			}
			WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
				let inputManager = self.engineRunner.engine.getInputManagerMut();
				let viewInputPosition = inputManager.getViewInputPosition();
				inputManager.getTextInputMut().setContextMenuRequestPosition(Some(viewInputPosition));
			}
			WindowEvent::MouseWheel { delta, .. } => {
				let inputManager = self.engineRunner.engine.getInputManagerMut();
				match delta {
					MouseScrollDelta::LineDelta(lineX, lineY) => {
						inputManager.addWheelDelta(-lineX * WHEEL_LINE_PIXEL_SIZE, -lineY * WHEEL_LINE_PIXEL_SIZE);
					}
					MouseScrollDelta::PixelDelta(pixelDelta) => {
						inputManager.addWheelDelta(-pixelDelta.x as f32, -pixelDelta.y as f32);
					}
				}
			}
			WindowEvent::RedrawRequested => {
				self.updateFrame();
				if self.engineRunner.engine.isExitRequested() {
					eventLoop.exit();
				}
			}
			_ => {
			}
		}
	}

	//==============================================================================
	// 이벤트를 다 처리한 뒤. (다음 프레임 요청, 최대 FPS 제한)
	//==============================================================================
	fn about_to_wait(&mut self, eventLoop: &ActiveEventLoop) {
		if self.engineRunner.engine.isExitRequested() {
			eventLoop.exit();
			return;
		}
		let windowState = match &self.windowState {
			Some(windowState) => {
				windowState
			}
			None => {
				return;
			}
		};
		let maximumFramePerSecond = self.engineRunner.engine.getEngineConfiguration().maximumFramePerSecond;
		if maximumFramePerSecond <= 0.0 {
			eventLoop.set_control_flow(ControlFlow::Poll);
			windowState.window.request_redraw();
			return;
		}

		// 차례가 된 프레임만 그린다. (한 프레임 넘게 밀렸으면 지금부터 다시 센다)
		let frameInterval = Duration::from_secs_f64(1.0 / maximumFramePerSecond as f64);
		let now = Instant::now();
		if now >= self.nextFrameInstant {
			windowState.window.request_redraw();
			self.nextFrameInstant += frameInterval;
			if self.nextFrameInstant + frameInterval < now {
				self.nextFrameInstant = now + frameInterval;
			}
		}
		eventLoop.set_control_flow(ControlFlow::WaitUntil(self.nextFrameInstant));
	}
}

//==============================================================================
// GL 설정 고르기. (스텐실 8 비트 이상, 멀티샘플은 4 에 가까운 것)
//==============================================================================
fn pickConfig(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
	let configList: Vec<Config> = configs.collect();
	let mut bestConfig: Option<Config> = None;
	for config in configList.iter() {
		if config.stencil_size() < 8 {
			continue;
		}
		let sampleCount = config.num_samples();
		let isBetter = match &bestConfig {
			None => {
				true
			}
			Some(currentConfig) => {
				let currentSampleCount = currentConfig.num_samples();
				if sampleCount <= 4 {
					currentSampleCount > 4 || sampleCount > currentSampleCount
				}
				else {
					currentSampleCount > 4 && sampleCount < currentSampleCount
				}
			}
		};
		if isBetter {
			bestConfig = Some(config.clone());
		}
	}
	match bestConfig {
		Some(bestConfig) => {
			bestConfig
		}
		None => {
			configList.into_iter().next().expect("GL 설정이 하나도 없습니다")
		}
	}
}

//==============================================================================
// 창 아이콘 읽기. (PNG, 스트레이트 알파)
//==============================================================================
fn loadWindowIcon(iconPath: &std::path::Path) -> Result<Icon, String> {
	let iconBytes = std::fs::read(iconPath).map_err(|error| format!("창 아이콘 읽기 실패 ({}): {}", iconPath.display(), error))?;
	let iconImage = image::load_from_memory(&iconBytes).map_err(|error| format!("창 아이콘 해석 실패: {}", error))?.to_rgba8();
	let iconWidth = iconImage.width();
	let iconHeight = iconImage.height();
	Icon::from_rgba(iconImage.into_raw(), iconWidth, iconHeight).map_err(|error| format!("창 아이콘 생성 실패: {}", error))
}
