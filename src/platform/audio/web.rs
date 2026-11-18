//==============================================================================
// 웹 오디오 백엔드. (wasm32, WebAudio 를 web-sys 로 직접 부른다)
// - 원본 audiomanager.js, audioplayer.js, audioasset.js, beepplayer.js 의 WebAudio 호출을 그대로 옮겼다.
//   (kira 는 wasm 에서 정적 소리를 파일에서 못 읽고 스트리밍 소리가 안 된다)
// - AudioContext: 원본처럼 AudioContext, 없으면 webkitAudioContext. user gesture 뒤에 만든다. (createAudioPlayer 는 그 전에도 만든다)
// - resume: "suspended" 나 "interrupted"(iOS WebKit 의 비표준 상태) 일 때만 resume() 하고 거부는 넘긴다.
//   상태는 문자열로 읽는다. (web-sys 의 AudioContextState 에 "interrupted" 가 없다)
// - iOS 시계 확인: 원본 verifyContextClock 그대로. (2026-10-22 03:41, iOS 26.6.2 버전업 대응)
//   복원 뒤 "running" 인데 0.5초 동안 currentTime 이 실제 시간의 1/4 보다 덜 흐르면 suspend() 후 resume() 하고 다시 확인한다.
//   그 resume() 이 거부되면 같은 간격 뒤 resumeContext() 로 다시 복원한다. 한 번에 하나만 돈다.
// - 디코딩: 원본처럼 OfflineAudioContext(1, 1, 44100) 로 한다. 없으면 잠시 쓰는 AudioContext 로 하고 닫는다.
// - 비동기: 프로미스는 wasm-bindgen-futures 의 spawn_local 과 JsFuture 로 기다린다. (원본 then / await 자리)
//   콜백은 매니저나 플레이어의 상태를 Weak 로 들어, 그 사이 버려졌으면 아무것도 하지 않는다.
// - 원본과 다른 점:
//   플레이어를 버리면 재생 중인 소스 노드를 멈추고 GainNode 를 뗀다. (onended 콜백이 버려진 클로저를 부르지 않게)
//   일시 중단 상태에서 play 한 뒤 재개를 기다리는 사이 다른 소스가 시작됐으면 그것을 멈추고 튼다. (원본은 둘 다 운다)
//   fetch 응답이 실패 코드면 디코딩 전에 실패로 친다.
//==============================================================================
use crate::core::audiomanager::{resumeAudioManagerState, AudioManagerState};
use crate::misc::beepplayer::{BeepEnvelope, BeepWaveform, BEEP_FLOOR_VOLUME, BEEP_STOP_MARGIN_SECONDS};
use crate::resource::audioasset::AudioAsset;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};

// 공유 오디오 컨텍스트.
pub type AudioContext = web_sys::AudioContext;
// 디코딩한 소리.
pub type AudioBuffer = web_sys::AudioBuffer;

// 브라우저는 첫 user gesture 전에 AudioContext 를 만들면 경고를 띄운다.
pub const HAS_USER_GESTURE_AT_START: bool = false;
// OfflineAudioContext 로 디코딩할 때의 샘플레이트. (재생하는 AudioContext 의 샘플레이트가 다르면 재생할 때 맞춰진다)
const DECODE_SAMPLE_RATE: f32 = 44100.0;
// iOS 에서 복원 뒤 시계(currentTime)가 흐르는지 보는 간격. (초, 2026-10-22 03:41, iOS 26.6.2 버전업 대응)
const CONTEXT_CLOCK_CHECK_INTERVAL: f64 = 0.5;
// 그 간격 동안 시계가 이 비율보다 덜 흐르면 멈춘 것으로 본다. (2026-10-22 03:41, iOS 26.6.2 버전업 대응)
const CONTEXT_CLOCK_MINIMUM_RATIO: f64 = 0.25;

//==============================================================================
// 매니저가 드는 백엔드 상태.
//==============================================================================
pub struct AudioPlatformState {
	// iOS(iPadOS 포함) 인지.
	isIOS: bool,
	// 시계 확인이 돌고 있는지. (한 번에 하나만)
	isVerifyingContextClock: bool,
}

//==============================================================================
// 백엔드 상태 생성. (iOS 판별)
//==============================================================================
pub fn createAudioPlatformState() -> AudioPlatformState {
	AudioPlatformState {
		isIOS: detectIOS(),
		isVerifyingContextClock: false,
	}
}

//==============================================================================
// iOS(iPadOS 포함) 판별. (원본 platform.js 의 platformName 순서 + 데스크탑 모드 iPad 는 터치 지점 수로)
//==============================================================================
fn detectIOS() -> bool {
	let window = match web_sys::window() {
		Some(window) => {
			window
		}
		None => {
			return false;
		}
	};
	let navigator = window.navigator();
	let userAgent = navigator.user_agent().unwrap_or_default().to_lowercase();
	if userAgent.contains("windows") || userAgent.contains("valve steam gameoverlay") {
		return false;
	}
	if userAgent.contains("ipad") || userAgent.contains("iphone") || userAgent.contains("ipod") {
		return true;
	}
	let isMacOS = userAgent.contains("macintosh") || userAgent.contains("mac os x");
	isMacOS && navigator.max_touch_points() > 1
}

//==============================================================================
// 전역 이름으로 생성자를 불러 객체를 만든다. (webkit 접두 생성자)
//==============================================================================
fn constructGlobal(globalName: &str, argumentList: &js_sys::Array) -> Option<JsValue> {
	let window = web_sys::window()?;
	let constructorValue = js_sys::Reflect::get(&window, &JsValue::from_str(globalName)).ok()?;
	let constructorFunction = constructorValue.dyn_into::<js_sys::Function>().ok()?;
	js_sys::Reflect::construct(&constructorFunction, argumentList).ok()
}

//==============================================================================
// 오디오 컨텍스트 생성. (AudioContext, 없으면 webkitAudioContext)
//==============================================================================
pub fn createAudioContext(_audioPlatformState: &mut AudioPlatformState) -> Option<AudioContext> {
	match web_sys::AudioContext::new() {
		Ok(audioContext) => {
			Some(audioContext)
		}
		Err(_) => {
			let emptyArgumentList = js_sys::Array::new();
			let audioContextValue = constructGlobal("webkitAudioContext", &emptyArgumentList)?;
			Some(audioContextValue.unchecked_into::<AudioContext>())
		}
	}
}

//==============================================================================
// 오디오 컨텍스트 상태 문자열. ("suspended", "running", "closed", iOS 의 "interrupted")
//==============================================================================
fn getAudioContextStateName(audioContext: &AudioContext) -> String {
	match js_sys::Reflect::get(audioContext, &JsValue::from_str("state")) {
		Ok(stateValue) => {
			stateValue.as_string().unwrap_or_default()
		}
		Err(_) => {
			String::new()
		}
	}
}

//==============================================================================
// 오디오 컨텍스트가 돌고 있는지.
//==============================================================================
pub fn isAudioContextRunning(audioContext: &AudioContext) -> bool {
	getAudioContextStateName(audioContext) == "running"
}

//==============================================================================
// 지금 시각. (초, performance.now())
//==============================================================================
fn getPerformanceSeconds() -> f64 {
	let performance = web_sys::window().and_then(|window| window.performance());
	match performance {
		Some(performance) => {
			performance.now() / 1000.0
		}
		None => {
			0.0
		}
	}
}

//==============================================================================
// 문서가 보이는지. (document.visibilityState === "visible")
//==============================================================================
fn isDocumentVisible() -> bool {
	let document = web_sys::window().and_then(|window| window.document());
	match document {
		Some(document) => {
			document.visibility_state() == web_sys::VisibilityState::Visible
		}
		None => {
			false
		}
	}
}

//==============================================================================
// 몇 초 뒤에 한 번 부른다. (setTimeout)
//==============================================================================
fn setTimeoutSeconds(delaySeconds: f64, callback: impl FnOnce() + 'static) {
	let window = match web_sys::window() {
		Some(window) => {
			window
		}
		None => {
			return;
		}
	};
	let callbackFunction = Closure::once_into_js(callback);
	let delayMilliseconds = (delaySeconds * 1000.0) as i32;
	let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(callbackFunction.unchecked_ref(), delayMilliseconds);
}

//==============================================================================
// 오디오 컨텍스트 재개. (user gesture 뒤, 컨텍스트가 있을 때 AudioManager 가 부른다)
// - "suspended" 나 "interrupted" 일 때만 재개를 시도한다. 거부되면 다음 호출에서 다시 하므로 넘긴다.
// - iOS 는 복원된 뒤 "running" 이어도 시계가 멈춰 소리가 나지 않는 일이 있어 확인한다.
//==============================================================================
pub fn resumeAudioContext(stateCell: &Rc<RefCell<AudioManagerState>>) {
	let (audioContext, isIOS) = {
		let state = stateCell.borrow();
		match &state.audioContext {
			Some(audioContext) => {
				(audioContext.clone(), state.platformState.isIOS)
			}
			None => {
				return;
			}
		}
	};
	let stateName = getAudioContextStateName(&audioContext);
	if stateName != "suspended" && stateName != "interrupted" {
		return;
	}
	let resumePromise = match audioContext.resume() {
		Ok(resumePromise) => {
			resumePromise
		}
		Err(_) => {
			return;
		}
	};
	let weakStateCell = Rc::downgrade(stateCell);
	spawn_local(async move {
		if JsFuture::from(resumePromise).await.is_err() {
			return;
		}
		if isIOS && isAudioContextRunning(&audioContext) {
			if let Some(stateCell) = weakStateCell.upgrade() {
				verifyContextClock(&stateCell, audioContext);
			}
		}
	});
}

//==============================================================================
// 복원 뒤 시계 확인. (iOS 전용, 원본 verifyContextClock)
// - CONTEXT_CLOCK_CHECK_INTERVAL 동안 currentTime 이 흐르는지 보고, 멈춰 있으면 suspend() 후 resume() 하고 다시 확인한다.
// - 그 resume() 이 거부되면 같은 간격 뒤 resumeContext() 로 다시 복원한다.
// - 페이지가 가려지거나 컨텍스트가 "running" 이 아니게 되면 그만둔다.
//==============================================================================
fn verifyContextClock(stateCell: &Rc<RefCell<AudioManagerState>>, audioContext: AudioContext) {
	{
		let mut state = stateCell.borrow_mut();
		if state.platformState.isVerifyingContextClock {
			return;
		}
		state.platformState.isVerifyingContextClock = true;
	}
	let startTime = getPerformanceSeconds();
	let startContextTime = audioContext.current_time();
	let weakStateCell = Rc::downgrade(stateCell);
	setTimeoutSeconds(CONTEXT_CLOCK_CHECK_INTERVAL, move || {
		let stateCell = match weakStateCell.upgrade() {
			Some(stateCell) => {
				stateCell
			}
			None => {
				return;
			}
		};
		let isCurrentAudioContext = {
			let mut state = stateCell.borrow_mut();
			state.platformState.isVerifyingContextClock = false;
			state.audioContext.as_ref() == Some(&audioContext)
		};
		if !isCurrentAudioContext || !isAudioContextRunning(&audioContext) || !isDocumentVisible() {
			return;
		}
		let elapsedTime = getPerformanceSeconds() - startTime;
		let contextElapsedTime = audioContext.current_time() - startContextTime;
		if contextElapsedTime >= elapsedTime * CONTEXT_CLOCK_MINIMUM_RATIO {
			return;
		}
		let warningMessage = format!("[AudioManager] iOS: 복원 뒤 running 인데 시계가 멈춰 있어 suspend 후 resume 한다. ({:.2}초 동안 {:.2}초)", elapsedTime, contextElapsedTime);
		web_sys::console::warn_1(&JsValue::from_str(&warningMessage));
		spawn_local(async move {
			let isRestarted = restartAudioContext(&audioContext).await;
			if isRestarted {
				if isAudioContextRunning(&audioContext) {
					verifyContextClock(&stateCell, audioContext);
				}
			}
			else {
				// 거부되면 같은 간격 뒤 다시 복원한다. (스스로 멈춘 것이므로)
				let weakStateCell = Rc::downgrade(&stateCell);
				setTimeoutSeconds(CONTEXT_CLOCK_CHECK_INTERVAL, move || {
					if let Some(stateCell) = weakStateCell.upgrade() {
						resumeAudioManagerState(&stateCell);
					}
				});
			}
		});
	});
}

//==============================================================================
// suspend() 후 resume(). (둘 다 이루어지면 참)
//==============================================================================
async fn restartAudioContext(audioContext: &AudioContext) -> bool {
	let suspendPromise = match audioContext.suspend() {
		Ok(suspendPromise) => {
			suspendPromise
		}
		Err(_) => {
			return false;
		}
	};
	if JsFuture::from(suspendPromise).await.is_err() {
		return false;
	}
	let resumePromise = match audioContext.resume() {
		Ok(resumePromise) => {
			resumePromise
		}
		Err(_) => {
			return false;
		}
	};
	JsFuture::from(resumePromise).await.is_ok()
}

//==============================================================================
// 플레이어 하나의 재생 상태. (onended 콜백과 재개 뒤 콜백이 같이 본다)
//==============================================================================
struct PlaybackState {
	audioSource: Option<web_sys::AudioBufferSourceNode>,
	// 지금 소스의 onended 콜백. (소스를 바꾸거나 멈출 때 떼고 버린다)
	endedCallback: Option<Closure<dyn FnMut()>>,
	isPlaying: bool,
}

impl PlaybackState {
	//==============================================================================
	// 지금 소스를 멈추고 뗀다. (onended 를 먼저 비워 콜백이 오지 않게 한다)
	//==============================================================================
	fn stopSource(&mut self) {
		if let Some(audioSource) = self.audioSource.take() {
			let scheduledSourceNode: &web_sys::AudioScheduledSourceNode = &audioSource;
			scheduledSourceNode.set_onended(None);
			let _ = scheduledSourceNode.stop();
		}
		self.endedCallback = None;
	}
}

//==============================================================================
// 플레이어 하나의 소리 내는 부분. (원본 AudioPlayer 의 audioContext, gainNode, audioSource)
//==============================================================================
pub struct AudioPlayerBackend {
	audioContext: Option<AudioContext>,
	gainNode: Option<web_sys::GainNode>,
	playbackState: Rc<RefCell<PlaybackState>>,
}

impl AudioPlayerBackend {
	//==============================================================================
	// 생성. (컨텍스트가 있으면 GainNode 를 만들어 출력에 잇는다)
	//==============================================================================
	pub fn create(audioContext: Option<&mut AudioContext>) -> AudioPlayerBackend {
		let audioContext = audioContext.cloned();
		let mut gainNode = None;
		if let Some(audioContext) = &audioContext {
			if let Ok(createdGainNode) = audioContext.create_gain() {
				let destination = audioContext.destination();
				let _ = createdGainNode.connect_with_audio_node(&destination);
				gainNode = Some(createdGainNode);
			}
		}
		AudioPlayerBackend {
			audioContext,
			gainNode,
			playbackState: Rc::new(RefCell::new(PlaybackState {
				audioSource: None,
				endedCallback: None,
				isPlaying: false,
			})),
		}
	}

	//==============================================================================
	// 소리를 낼 수 있는지. (원본 audioContext && gainNode)
	//==============================================================================
	pub fn isAvailable(&self) -> bool {
		self.audioContext.is_some() && self.gainNode.is_some()
	}

	//==============================================================================
	// 재생 중인 소스를 멈춘다. (재생 여부 플래그는 그대로, 원본 play 의 앞부분)
	//==============================================================================
	pub fn stopSource(&mut self) {
		self.playbackState.borrow_mut().stopSource();
	}

	//==============================================================================
	// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		let mut playbackState = self.playbackState.borrow_mut();
		playbackState.stopSource();
		playbackState.isPlaying = false;
	}

	//==============================================================================
	// 재생 시작. (컨텍스트가 일시 중단돼 있으면 재개한 뒤에 튼다)
	//==============================================================================
	pub fn startPlayback(&mut self, audioBuffer: &AudioBuffer, isLoop: bool, time: f64) {
		let (audioContext, gainNode) = match (&self.audioContext, &self.gainNode) {
			(Some(audioContext), Some(gainNode)) => {
				(audioContext.clone(), gainNode.clone())
			}
			_ => {
				return;
			}
		};
		if getAudioContextStateName(&audioContext) == "suspended" {
			let resumePromise = match audioContext.resume() {
				Ok(resumePromise) => {
					resumePromise
				}
				Err(_) => {
					return;
				}
			};
			let weakPlaybackState = Rc::downgrade(&self.playbackState);
			let audioBuffer = audioBuffer.clone();
			spawn_local(async move {
				if JsFuture::from(resumePromise).await.is_err() {
					return;
				}
				if let Some(playbackStateCell) = weakPlaybackState.upgrade() {
					startSource(&playbackStateCell, &audioContext, &gainNode, &audioBuffer, isLoop, time);
				}
			});
			return;
		}
		startSource(&self.playbackState, &audioContext, &gainNode, audioBuffer, isLoop, time);
	}

	//==============================================================================
	// 음량 설정. (gainNode.gain.value)
	//==============================================================================
	pub fn setGain(&mut self, gain: f32) {
		if let Some(gainNode) = &self.gainNode {
			let gainParameter = gainNode.gain();
			gainParameter.set_value(gain);
		}
	}

	//==============================================================================
	// 재생 중인지. (startPlayback 에서 참, onended 나 stop 에서 거짓)
	//==============================================================================
	pub fn isPlaying(&self) -> bool {
		self.playbackState.borrow().isPlaying
	}
}

impl Drop for AudioPlayerBackend {
	//==============================================================================
	// 버릴 때 소스를 멈추고 GainNode 를 뗀다.
	//==============================================================================
	fn drop(&mut self) {
		self.playbackState.borrow_mut().stopSource();
		if let Some(gainNode) = &self.gainNode {
			let _ = gainNode.disconnect();
		}
	}
}

//==============================================================================
// 소스 노드를 만들어 튼다. (원본 startPlayback)
//==============================================================================
fn startSource(playbackStateCell: &Rc<RefCell<PlaybackState>>, audioContext: &AudioContext, gainNode: &web_sys::GainNode, audioBuffer: &AudioBuffer, isLoop: bool, time: f64) {
	let audioSource = match audioContext.create_buffer_source() {
		Ok(audioSource) => {
			audioSource
		}
		Err(_) => {
			return;
		}
	};
	audioSource.set_buffer(Some(audioBuffer));
	audioSource.set_loop(isLoop);
	if audioSource.connect_with_audio_node(gainNode).is_err() {
		return;
	}

	// 끝나면 재생 여부를 내린다. (콜백 안에서는 자기 클로저를 버리지 않는다. 다음 stopSource 가 버린다)
	let weakPlaybackState = Rc::downgrade(playbackStateCell);
	let endedCallback: Closure<dyn FnMut()> = Closure::new(move || {
		if let Some(playbackStateCell) = weakPlaybackState.upgrade() {
			let mut playbackState = playbackStateCell.borrow_mut();
			playbackState.isPlaying = false;
			playbackState.audioSource = None;
		}
	});
	let scheduledSourceNode: &web_sys::AudioScheduledSourceNode = &audioSource;
	scheduledSourceNode.set_onended(Some(endedCallback.as_ref().unchecked_ref()));

	let mut playbackState = playbackStateCell.borrow_mut();
	playbackState.stopSource();
	if audioSource.start_with_when_and_grain_offset(0.0, time).is_err() {
		scheduledSourceNode.set_onended(None);
		return;
	}
	playbackState.audioSource = Some(audioSource);
	playbackState.endedCallback = Some(endedCallback);
	playbackState.isPlaying = true;
}

//==============================================================================
// 경로를 주소 글로. (애셋 폴더 기준 상대 주소, 역슬래시는 슬래시로)
//==============================================================================
fn pathToUrl(assetPath: &Path) -> String {
	assetPath.to_string_lossy().replace('\\', "/")
}

//==============================================================================
// 자바스크립트 오류를 글로.
//==============================================================================
fn describeJsError(error: JsValue) -> String {
	format!("{:?}", error)
}

//==============================================================================
// 파일 읽기. (fetch 와 디코딩을 걸어 두고 바로 돌아온다. 끝나면 애셋이 채워진다)
//==============================================================================
pub fn loadAudioFile(audioAsset: &Rc<AudioAsset>, assetPath: &Path) -> Result<(), String> {
	let assetUrl = pathToUrl(assetPath);
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let fetchPromise = window.fetch_with_str(&assetUrl);
	let audioAsset = audioAsset.clone();
	spawn_local(async move {
		match fetchAudioBuffer(fetchPromise).await {
			Ok(audioBuffer) => {
				audioAsset.setAudioBuffer(audioBuffer);
			}
			Err(error) => {
				let loadError = format!("Error loading sound: {} ({})", assetUrl, error);
				web_sys::console::error_1(&JsValue::from_str(&loadError));
				audioAsset.setLoadError(loadError);
			}
		}
	});
	Ok(())
}

//==============================================================================
// 메모리에서 읽기. (디코딩을 걸어 두고 바로 돌아온다)
//==============================================================================
pub fn loadAudioBytes(audioAsset: &Rc<AudioAsset>, bytes: Vec<u8>) -> Result<(), String> {
	let byteArray = js_sys::Uint8Array::from(bytes.as_slice());
	let arrayBuffer = byteArray.buffer();
	let audioAsset = audioAsset.clone();
	spawn_local(async move {
		match decodeArrayBuffer(&arrayBuffer).await {
			Ok(audioBuffer) => {
				audioAsset.setAudioBuffer(audioBuffer);
			}
			Err(error) => {
				let loadError = format!("Error loading sound from memory ({})", error);
				web_sys::console::error_1(&JsValue::from_str(&loadError));
				audioAsset.setLoadError(loadError);
			}
		}
	});
	Ok(())
}

//==============================================================================
// fetch 응답을 받아 디코딩한다.
//==============================================================================
async fn fetchAudioBuffer(fetchPromise: js_sys::Promise) -> Result<AudioBuffer, String> {
	let responseValue = JsFuture::from(fetchPromise).await.map_err(describeJsError)?;
	let response = responseValue.dyn_into::<web_sys::Response>().map_err(describeJsError)?;
	if !response.ok() {
		return Err(format!("HTTP {}", response.status()));
	}
	let arrayBufferPromise = response.array_buffer().map_err(describeJsError)?;
	let arrayBufferValue = JsFuture::from(arrayBufferPromise).await.map_err(describeJsError)?;
	let arrayBuffer = arrayBufferValue.dyn_into::<js_sys::ArrayBuffer>().map_err(describeJsError)?;
	decodeArrayBuffer(&arrayBuffer).await
}

//==============================================================================
// 디코딩. (OfflineAudioContext 로. 스피커를 잡지 않고, user gesture 전에 만들어도 경고가 없으며,
//  여러 애셋을 한꺼번에 받아도 실시간 AudioContext 의 개수 제한에 걸리지 않는다. 없으면 잠시 쓰는 AudioContext 로 하고 닫는다)
//==============================================================================
async fn decodeArrayBuffer(arrayBuffer: &js_sys::ArrayBuffer) -> Result<AudioBuffer, String> {
	if let Ok(offlineAudioContext) = web_sys::OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(1, 1, DECODE_SAMPLE_RATE) {
		let decodePromise = offlineAudioContext.decode_audio_data(arrayBuffer).map_err(describeJsError)?;
		let audioBufferValue = JsFuture::from(decodePromise).await.map_err(describeJsError)?;
		return audioBufferValue.dyn_into::<AudioBuffer>().map_err(describeJsError);
	}
	let temporaryAudioContext = web_sys::AudioContext::new().map_err(|_| String::from("AudioContext is not supported."))?;
	let decodeResult = match temporaryAudioContext.decode_audio_data(arrayBuffer) {
		Ok(decodePromise) => {
			JsFuture::from(decodePromise).await.map_err(describeJsError)
		}
		Err(error) => {
			Err(describeJsError(error))
		}
	};
	if let Ok(closePromise) = temporaryAudioContext.close() {
		let _ = JsFuture::from(closePromise).await;
	}
	let audioBufferValue = decodeResult?;
	audioBufferValue.dyn_into::<AudioBuffer>().map_err(describeJsError)
}

//==============================================================================
// 버퍼의 재생 시간. (초)
//==============================================================================
pub fn getAudioBufferDuration(audioBuffer: &AudioBuffer) -> f64 {
	audioBuffer.duration()
}

//==============================================================================
// 파형을 오실레이터 타입으로.
//==============================================================================
fn toOscillatorType(beepWaveform: BeepWaveform) -> web_sys::OscillatorType {
	match beepWaveform {
		BeepWaveform::sine => {
			web_sys::OscillatorType::Sine
		}
		BeepWaveform::square => {
			web_sys::OscillatorType::Square
		}
		BeepWaveform::sawtooth => {
			web_sys::OscillatorType::Sawtooth
		}
		BeepWaveform::triangle => {
			web_sys::OscillatorType::Triangle
		}
	}
}

//==============================================================================
// 비프 톤 하나 재생. (원본 playTone 의 OscillatorNode + GainNode 스케줄링)
//==============================================================================
pub fn playBeepTone(audioContext: &mut AudioContext, beepEnvelope: &BeepEnvelope) {
	let startTime = audioContext.current_time() + beepEnvelope.delaySeconds;
	let endTime = startTime + beepEnvelope.durationSeconds;
	let attackEndTime = startTime + beepEnvelope.attackSeconds;
	let releaseStartTime = startTime + beepEnvelope.getReleaseStartSeconds();

	let oscillatorNode = match audioContext.create_oscillator() {
		Ok(oscillatorNode) => {
			oscillatorNode
		}
		Err(_) => {
			return;
		}
	};
	let oscillatorType = toOscillatorType(beepEnvelope.waveform);
	oscillatorNode.set_type(oscillatorType);
	let frequencyParameter = oscillatorNode.frequency();
	let _ = frequencyParameter.set_value_at_time(beepEnvelope.frequency, startTime);

	let gainNode = match audioContext.create_gain() {
		Ok(gainNode) => {
			gainNode
		}
		Err(_) => {
			return;
		}
	};
	let gainParameter = gainNode.gain();
	let _ = gainParameter.set_value_at_time(BEEP_FLOOR_VOLUME, startTime);
	let _ = gainParameter.linear_ramp_to_value_at_time(beepEnvelope.peakVolume, attackEndTime);
	let _ = gainParameter.set_value_at_time(beepEnvelope.peakVolume, releaseStartTime);
	let _ = gainParameter.exponential_ramp_to_value_at_time(BEEP_FLOOR_VOLUME, endTime);

	let destination = audioContext.destination();
	let _ = oscillatorNode.connect_with_audio_node(&gainNode);
	let _ = gainNode.connect_with_audio_node(&destination);
	let _ = oscillatorNode.start_with_when(startTime);
	let _ = oscillatorNode.stop_with_when(endTime + BEEP_STOP_MARGIN_SECONDS);

	// 끝나면 노드를 뗀다. (한 번 불리고 스스로 풀리는 콜백)
	let oscillatorNodeForEnded = oscillatorNode.clone();
	let endedCallback = Closure::once_into_js(move || {
		let _ = oscillatorNodeForEnded.disconnect();
		let _ = gainNode.disconnect();
	});
	oscillatorNode.set_onended(Some(endedCallback.unchecked_ref()));
}
