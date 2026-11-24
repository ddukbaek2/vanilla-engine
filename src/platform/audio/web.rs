//==============================================================================
// 웹 오디오 백엔드. (wasm32, WebAudio 를 web-sys 로 직접 부른다)
// - 원본 audiomanager.js, audioplayer.js, audioasset.js, beepplayer.js 의 WebAudio 호출을 그대로 옮겼다.
//   (kira 는 wasm 에서 정적 소리를 파일에서 못 읽고 스트리밍 소리가 안 된다)
// - AudioContext: 원본처럼 AudioContext, 없으면 webkitAudioContext. user gesture 뒤에 만든다. (create_audio_player 는 그 전에도 만든다)
// - resume: "suspended" 나 "interrupted"(iOS WebKit 의 비표준 상태) 일 때만 resume() 하고 거부는 넘긴다.
//   상태는 문자열로 읽는다. (web-sys 의 AudioContextState 에 "interrupted" 가 없다)
// - iOS 시계 확인: 원본 verify_context_clock 그대로. (2026-10-22 03:41, iOS 26.6.2 버전업 대응)
//   복원 뒤 "running" 인데 0.5초 동안 current_time 이 실제 시간의 1/4 보다 덜 흐르면 suspend() 후 resume() 하고 다시 확인한다.
//   그 resume() 이 거부되면 같은 간격 뒤 resume_context() 로 다시 복원한다. 한 번에 하나만 돈다.
// - 디코딩: 원본처럼 OfflineAudioContext(1, 1, 44100) 로 한다. 없으면 잠시 쓰는 AudioContext 로 하고 닫는다.
// - 컨텍스트 멈춤: 원본 SoundSystem.applyContextPause 처럼 "running" 일 때만 suspend() 하고, 거부되면 알리고 넘긴다.
// - 좌우(pan): 원본 SoundSystem.playEffect 처럼 0 이 아니면 StereoPannerNode 를 GainNode 와 출력 사이에 끼운다. (createStereoPanner 가 없으면 끼우지 않는다)
// - 비동기: 프로미스는 wasm-bindgen-futures 의 spawn_local 과 JsFuture 로 기다린다. (원본 then / await 자리)
//   콜백은 매니저나 플레이어의 상태를 Weak 로 들어, 그 사이 버려졌으면 아무것도 하지 않는다.
// - 원본과 다른 점:
//   플레이어를 버리면 재생 중인 소스 노드를 멈추고 GainNode 를 뗀다. (onended 콜백이 버려진 클로저를 부르지 않게)
//   일시 중단 상태에서 play 한 뒤 재개를 기다리는 사이 다른 소스가 시작됐으면 그것을 멈추고 튼다. (원본은 둘 다 운다)
//   fetch 응답이 실패 코드면 디코딩 전에 실패로 친다.
//==============================================================================
use crate::core::audiomanager::{resume_audio_manager_state, AudioManagerState};
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
// iOS 에서 복원 뒤 시계(current_time)가 흐르는지 보는 간격. (초, 2026-10-22 03:41, iOS 26.6.2 버전업 대응)
const CONTEXT_CLOCK_CHECK_INTERVAL: f64 = 0.5;
// 그 간격 동안 시계가 이 비율보다 덜 흐르면 멈춘 것으로 본다. (2026-10-22 03:41, iOS 26.6.2 버전업 대응)
const CONTEXT_CLOCK_MINIMUM_RATIO: f64 = 0.25;

//==============================================================================
// 매니저가 드는 백엔드 상태.
//==============================================================================
pub struct AudioPlatformState {
	// iOS(iPadOS 포함) 인지.
	is_ios: bool,
	// 시계 확인이 돌고 있는지. (한 번에 하나만)
	is_verifying_context_clock: bool,
}

//==============================================================================
// 백엔드 상태 생성. (iOS 판별)
//==============================================================================
pub fn create_audio_platform_state() -> AudioPlatformState {
	AudioPlatformState {
		is_ios: detect_ios(),
		is_verifying_context_clock: false,
	}
}

//==============================================================================
// iOS(iPadOS 포함) 판별. (원본 platform.js 의 platform_name 순서 + 데스크탑 모드 iPad 는 터치 지점 수로)
//==============================================================================
fn detect_ios() -> bool {
	let window = match web_sys::window() {
		Some(window) => {
			window
		}
		None => {
			return false;
		}
	};
	let navigator = window.navigator();
	let user_agent = navigator.user_agent().unwrap_or_default().to_lowercase();
	if user_agent.contains("windows") || user_agent.contains("valve steam gameoverlay") {
		return false;
	}
	if user_agent.contains("ipad") || user_agent.contains("iphone") || user_agent.contains("ipod") {
		return true;
	}
	let is_mac_os = user_agent.contains("macintosh") || user_agent.contains("mac os x");
	is_mac_os && navigator.max_touch_points() > 1
}

//==============================================================================
// 전역 이름으로 생성자를 불러 객체를 만든다. (webkit 접두 생성자)
//==============================================================================
fn construct_global(global_name: &str, argument_list: &js_sys::Array) -> Option<JsValue> {
	let window = web_sys::window()?;
	let constructor_value = js_sys::Reflect::get(&window, &JsValue::from_str(global_name)).ok()?;
	let constructor_function = constructor_value.dyn_into::<js_sys::Function>().ok()?;
	js_sys::Reflect::construct(&constructor_function, argument_list).ok()
}

//==============================================================================
// 오디오 컨텍스트 생성. (AudioContext, 없으면 webkitAudioContext)
//==============================================================================
pub fn create_audio_context(_audio_platform_state: &mut AudioPlatformState) -> Option<AudioContext> {
	match web_sys::AudioContext::new() {
		Ok(audio_context) => {
			Some(audio_context)
		}
		Err(_) => {
			let empty_argument_list = js_sys::Array::new();
			let audio_context_value = construct_global("webkitAudioContext", &empty_argument_list)?;
			Some(audio_context_value.unchecked_into::<AudioContext>())
		}
	}
}

//==============================================================================
// 오디오 컨텍스트 상태 문자열. ("suspended", "running", "closed", iOS 의 "interrupted")
//==============================================================================
fn get_audio_context_state_name(audio_context: &AudioContext) -> String {
	match js_sys::Reflect::get(audio_context, &JsValue::from_str("state")) {
		Ok(state_value) => {
			state_value.as_string().unwrap_or_default()
		}
		Err(_) => {
			String::new()
		}
	}
}

//==============================================================================
// 오디오 컨텍스트가 돌고 있는지.
//==============================================================================
pub fn is_audio_context_running(audio_context: &AudioContext) -> bool {
	get_audio_context_state_name(audio_context) == "running"
}

//==============================================================================
// 지금 시각. (초, performance.now())
//==============================================================================
fn get_performance_seconds() -> f64 {
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
fn is_document_visible() -> bool {
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
fn set_timeout_seconds(delay_seconds: f64, callback: impl FnOnce() + 'static) {
	let window = match web_sys::window() {
		Some(window) => {
			window
		}
		None => {
			return;
		}
	};
	let callback_function = Closure::once_into_js(callback);
	let delay_milliseconds = (delay_seconds * 1000.0) as i32;
	let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(callback_function.unchecked_ref(), delay_milliseconds);
}

//==============================================================================
// 오디오 컨텍스트 재개. (user gesture 뒤, 컨텍스트가 있을 때 AudioManager 가 부른다)
// - "suspended" 나 "interrupted" 일 때만 재개를 시도한다. 거부되면 다음 호출에서 다시 하므로 넘긴다.
// - iOS 는 복원된 뒤 "running" 이어도 시계가 멈춰 소리가 나지 않는 일이 있어 확인한다.
//==============================================================================
pub fn resume_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let (audio_context, is_ios) = {
		let state = state_cell.borrow();
		match &state.audio_context {
			Some(audio_context) => {
				(audio_context.clone(), state.platform_state.is_ios)
			}
			None => {
				return;
			}
		}
	};
	let state_name = get_audio_context_state_name(&audio_context);
	if state_name != "suspended" && state_name != "interrupted" {
		return;
	}
	let resume_promise = match audio_context.resume() {
		Ok(resume_promise) => {
			resume_promise
		}
		Err(_) => {
			return;
		}
	};
	let weak_state_cell = Rc::downgrade(state_cell);
	spawn_local(async move {
		if JsFuture::from(resume_promise).await.is_err() {
			return;
		}
		if is_ios && is_audio_context_running(&audio_context) {
			if let Some(state_cell) = weak_state_cell.upgrade() {
				verify_context_clock(&state_cell, audio_context);
			}
		}
	});
}

//==============================================================================
// 오디오 컨텍스트 멈춤. ("running" 일 때만 suspend(), 거부되면 알린다)
//==============================================================================
pub fn suspend_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let audio_context = match &state_cell.borrow().audio_context {
		Some(audio_context) => {
			audio_context.clone()
		}
		None => {
			return;
		}
	};
	if !is_audio_context_running(&audio_context) {
		return;
	}
	let suspend_promise = match audio_context.suspend() {
		Ok(suspend_promise) => {
			suspend_promise
		}
		Err(error) => {
			web_sys::console::warn_2(&JsValue::from_str("[sound] AudioContext 를 멈추지 못했습니다."), &error);
			return;
		}
	};
	spawn_local(async move {
		if let Err(error) = JsFuture::from(suspend_promise).await {
			web_sys::console::warn_2(&JsValue::from_str("[sound] AudioContext 를 멈추지 못했습니다."), &error);
		}
	});
}

//==============================================================================
// 복원 뒤 시계 확인. (iOS 전용, 원본 verify_context_clock)
// - CONTEXT_CLOCK_CHECK_INTERVAL 동안 current_time 이 흐르는지 보고, 멈춰 있으면 suspend() 후 resume() 하고 다시 확인한다.
// - 그 resume() 이 거부되면 같은 간격 뒤 resume_context() 로 다시 복원한다.
// - 페이지가 가려지거나 컨텍스트가 "running" 이 아니게 되면 그만둔다.
//==============================================================================
fn verify_context_clock(state_cell: &Rc<RefCell<AudioManagerState>>, audio_context: AudioContext) {
	{
		let mut state = state_cell.borrow_mut();
		if state.platform_state.is_verifying_context_clock {
			return;
		}
		state.platform_state.is_verifying_context_clock = true;
	}
	let start_time = get_performance_seconds();
	let start_context_time = audio_context.current_time();
	let weak_state_cell = Rc::downgrade(state_cell);
	set_timeout_seconds(CONTEXT_CLOCK_CHECK_INTERVAL, move || {
		let state_cell = match weak_state_cell.upgrade() {
			Some(state_cell) => {
				state_cell
			}
			None => {
				return;
			}
		};
		let is_current_audio_context = {
			let mut state = state_cell.borrow_mut();
			state.platform_state.is_verifying_context_clock = false;
			state.audio_context.as_ref() == Some(&audio_context)
		};
		if !is_current_audio_context || !is_audio_context_running(&audio_context) || !is_document_visible() {
			return;
		}
		let elapsed_time = get_performance_seconds() - start_time;
		let context_elapsed_time = audio_context.current_time() - start_context_time;
		if context_elapsed_time >= elapsed_time * CONTEXT_CLOCK_MINIMUM_RATIO {
			return;
		}
		let warning_message = format!("[AudioManager] iOS: 복원 뒤 running 인데 시계가 멈춰 있어 suspend 후 resume 한다. ({:.2}초 동안 {:.2}초)", elapsed_time, context_elapsed_time);
		web_sys::console::warn_1(&JsValue::from_str(&warning_message));
		spawn_local(async move {
			let is_restarted = restart_audio_context(&audio_context).await;
			if is_restarted {
				if is_audio_context_running(&audio_context) {
					verify_context_clock(&state_cell, audio_context);
				}
			}
			else {
				// 거부되면 같은 간격 뒤 다시 복원한다. (스스로 멈춘 것이므로)
				let weak_state_cell = Rc::downgrade(&state_cell);
				set_timeout_seconds(CONTEXT_CLOCK_CHECK_INTERVAL, move || {
					if let Some(state_cell) = weak_state_cell.upgrade() {
						resume_audio_manager_state(&state_cell);
					}
				});
			}
		});
	});
}

//==============================================================================
// suspend() 후 resume(). (둘 다 이루어지면 참)
//==============================================================================
async fn restart_audio_context(audio_context: &AudioContext) -> bool {
	let suspend_promise = match audio_context.suspend() {
		Ok(suspend_promise) => {
			suspend_promise
		}
		Err(_) => {
			return false;
		}
	};
	if JsFuture::from(suspend_promise).await.is_err() {
		return false;
	}
	let resume_promise = match audio_context.resume() {
		Ok(resume_promise) => {
			resume_promise
		}
		Err(_) => {
			return false;
		}
	};
	JsFuture::from(resume_promise).await.is_ok()
}

//==============================================================================
// 플레이어 하나의 재생 상태. (onended 콜백과 재개 뒤 콜백이 같이 본다)
//==============================================================================
struct PlaybackState {
	audio_source: Option<web_sys::AudioBufferSourceNode>,
	// 지금 소스의 onended 콜백. (소스를 바꾸거나 멈출 때 떼고 버린다)
	ended_callback: Option<Closure<dyn FnMut()>>,
	is_playing: bool,
}

impl PlaybackState {
	//==============================================================================
	// 지금 소스를 멈추고 뗀다. (onended 를 먼저 비워 콜백이 오지 않게 한다)
	//==============================================================================
	fn stop_source(&mut self) {
		if let Some(audio_source) = self.audio_source.take() {
			let scheduled_source_node: &web_sys::AudioScheduledSourceNode = &audio_source;
			scheduled_source_node.set_onended(None);
			let _ = scheduled_source_node.stop();
		}
		self.ended_callback = None;
	}
}

//==============================================================================
// 플레이어 하나의 소리 내는 부분. (원본 AudioPlayer 의 audio_context, gain_node, audio_source)
//==============================================================================
pub struct AudioPlayerBackend {
	audio_context: Option<AudioContext>,
	gain_node: Option<web_sys::GainNode>,
	// 좌우 노드. (StereoPannerNode, 0 이 아닌 좌우를 처음 걸 때 만든다)
	panner_node: Option<web_sys::AudioNode>,
	playback_state: Rc<RefCell<PlaybackState>>,
	playback_rate: f64,
}

impl AudioPlayerBackend {
	//==============================================================================
	// 생성. (컨텍스트가 있으면 GainNode 를 만들어 출력에 잇는다)
	//==============================================================================
	pub fn create(audio_context: Option<&mut AudioContext>) -> AudioPlayerBackend {
		let audio_context = audio_context.cloned();
		let mut gain_node = None;
		if let Some(audio_context) = &audio_context {
			if let Ok(created_gain_node) = audio_context.create_gain() {
				let destination = audio_context.destination();
				let _ = created_gain_node.connect_with_audio_node(&destination);
				gain_node = Some(created_gain_node);
			}
		}
		AudioPlayerBackend {
			audio_context,
			gain_node,
			panner_node: None,
			playback_state: Rc::new(RefCell::new(PlaybackState {
				audio_source: None,
				ended_callback: None,
				is_playing: false,
			})),
			playback_rate: 1.0,
		}
	}

	//==============================================================================
	// 소리를 낼 수 있는지. (원본 audio_context && gain_node)
	//==============================================================================
	pub fn is_available(&self) -> bool {
		self.audio_context.is_some() && self.gain_node.is_some()
	}

	//==============================================================================
	// 재생 중인 소스를 멈춘다. (재생 여부 플래그는 그대로, 원본 play 의 앞부분)
	//==============================================================================
	pub fn stop_source(&mut self) {
		self.playback_state.borrow_mut().stop_source();
	}

	//==============================================================================
	// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		let mut playback_state = self.playback_state.borrow_mut();
		playback_state.stop_source();
		playback_state.is_playing = false;
	}

	//==============================================================================
	// 재생 시작. (컨텍스트가 일시 중단돼 있으면 재개한 뒤에 튼다)
	//==============================================================================
	pub fn start_playback(&mut self, audio_buffer: &AudioBuffer, is_loop: bool, time: f64) {
		let (audio_context, gain_node) = match (&self.audio_context, &self.gain_node) {
			(Some(audio_context), Some(gain_node)) => {
				(audio_context.clone(), gain_node.clone())
			}
			_ => {
				return;
			}
		};
		if get_audio_context_state_name(&audio_context) == "suspended" {
			let resume_promise = match audio_context.resume() {
				Ok(resume_promise) => {
					resume_promise
				}
				Err(_) => {
					return;
				}
			};
			let weak_playback_state = Rc::downgrade(&self.playback_state);
			let audio_buffer = audio_buffer.clone();
			let playback_rate = self.playback_rate;
			spawn_local(async move {
				if JsFuture::from(resume_promise).await.is_err() {
					return;
				}
				if let Some(playback_state_cell) = weak_playback_state.upgrade() {
					start_source(&playback_state_cell, &audio_context, &gain_node, &audio_buffer, is_loop, time, playback_rate);
				}
			});
			return;
		}
		start_source(&self.playback_state, &audio_context, &gain_node, audio_buffer, is_loop, time, self.playback_rate);
	}

	//==============================================================================
	// 재생 빠르기 설정. (1 이면 그대로, 음 높이도 같이 바뀐다, 다음 재생부터)
	//==============================================================================
	pub fn set_playback_rate(&mut self, playback_rate: f64) {
		self.playback_rate = playback_rate;
	}

	//==============================================================================
	// 좌우 설정. (-1 왼쪽 ~ 1 오른쪽, 0 이 아니면 GainNode → StereoPannerNode → 출력으로 잇는다)
	//==============================================================================
	pub fn set_pan(&mut self, pan: f32) {
		if self.panner_node.is_none() && pan != 0.0 {
			self.panner_node = self.create_panner_node();
		}
		if let Some(panner_node) = &self.panner_node {
			let pan_parameter = js_sys::Reflect::get(panner_node, &JsValue::from_str("pan")).ok().and_then(|value| value.dyn_into::<web_sys::AudioParam>().ok());
			if let Some(pan_parameter) = pan_parameter {
				pan_parameter.set_value(pan);
			}
		}
	}

	//==============================================================================
	// StereoPannerNode 를 만들어 GainNode 와 출력 사이에 끼운다. (createStereoPanner 가 없으면 None)
	//==============================================================================
	fn create_panner_node(&self) -> Option<web_sys::AudioNode> {
		let audio_context = self.audio_context.as_ref()?;
		let gain_node = self.gain_node.as_ref()?;
		let create_function: js_sys::Function = js_sys::Reflect::get(audio_context, &JsValue::from_str("createStereoPanner")).ok()?.dyn_into().ok()?;
		let panner_node: web_sys::AudioNode = create_function.call0(audio_context).ok()?.dyn_into().ok()?;
		let destination = audio_context.destination();
		let _ = gain_node.disconnect();
		let _ = gain_node.connect_with_audio_node(&panner_node);
		let _ = panner_node.connect_with_audio_node(&destination);
		Some(panner_node)
	}

	//==============================================================================
	// 음량 설정. (gain_node.gain.value)
	//==============================================================================
	pub fn set_gain(&mut self, gain: f32) {
		if let Some(gain_node) = &self.gain_node {
			let gain_parameter = gain_node.gain();
			gain_parameter.set_value(gain);
		}
	}

	//==============================================================================
	// 재생 중인지. (start_playback 에서 참, onended 나 stop 에서 거짓)
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.playback_state.borrow().is_playing
	}
}

impl Drop for AudioPlayerBackend {
	//==============================================================================
	// 버릴 때 소스를 멈추고 GainNode 를 뗀다.
	//==============================================================================
	fn drop(&mut self) {
		self.playback_state.borrow_mut().stop_source();
		if let Some(gain_node) = &self.gain_node {
			let _ = gain_node.disconnect();
		}
		if let Some(panner_node) = &self.panner_node {
			let _ = panner_node.disconnect();
		}
	}
}

//==============================================================================
// 소스 노드를 만들어 튼다. (원본 start_playback)
//==============================================================================
fn start_source(playback_state_cell: &Rc<RefCell<PlaybackState>>, audio_context: &AudioContext, gain_node: &web_sys::GainNode, audio_buffer: &AudioBuffer, is_loop: bool, time: f64, playback_rate: f64) {
	let audio_source = match audio_context.create_buffer_source() {
		Ok(audio_source) => {
			audio_source
		}
		Err(_) => {
			return;
		}
	};
	audio_source.set_buffer(Some(audio_buffer));
	audio_source.set_loop(is_loop);
	audio_source.playback_rate().set_value(playback_rate as f32);
	if audio_source.connect_with_audio_node(gain_node).is_err() {
		return;
	}

	// 끝나면 재생 여부를 내린다. (콜백 안에서는 자기 클로저를 버리지 않는다. 다음 stop_source 가 버린다)
	let weak_playback_state = Rc::downgrade(playback_state_cell);
	let ended_callback: Closure<dyn FnMut()> = Closure::new(move || {
		if let Some(playback_state_cell) = weak_playback_state.upgrade() {
			let mut playback_state = playback_state_cell.borrow_mut();
			playback_state.is_playing = false;
			playback_state.audio_source = None;
		}
	});
	let scheduled_source_node: &web_sys::AudioScheduledSourceNode = &audio_source;
	scheduled_source_node.set_onended(Some(ended_callback.as_ref().unchecked_ref()));

	let mut playback_state = playback_state_cell.borrow_mut();
	playback_state.stop_source();
	if audio_source.start_with_when_and_grain_offset(0.0, time).is_err() {
		scheduled_source_node.set_onended(None);
		return;
	}
	playback_state.audio_source = Some(audio_source);
	playback_state.ended_callback = Some(ended_callback);
	playback_state.is_playing = true;
}

//==============================================================================
// 경로를 주소 글로. (애셋 폴더 기준 상대 주소, 역슬래시는 슬래시로)
//==============================================================================
fn path_to_url(asset_path: &Path) -> String {
	asset_path.to_string_lossy().replace('\\', "/")
}

//==============================================================================
// 자바스크립트 오류를 글로.
//==============================================================================
fn describe_js_error(error: JsValue) -> String {
	format!("{:?}", error)
}

//==============================================================================
// 파일 읽기. (fetch 와 디코딩을 걸어 두고 바로 돌아온다. 끝나면 애셋이 채워진다)
//==============================================================================
pub fn load_audio_file(audio_asset: &Rc<AudioAsset>, asset_path: &Path) -> Result<(), String> {
	let asset_url = path_to_url(asset_path);
	let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
	let fetch_promise = window.fetch_with_str(&asset_url);
	let audio_asset = audio_asset.clone();
	spawn_local(async move {
		match fetch_audio_buffer(fetch_promise).await {
			Ok(audio_buffer) => {
				audio_asset.set_audio_buffer(audio_buffer);
			}
			Err(error) => {
				let load_error = format!("Error loading sound: {} ({})", asset_url, error);
				web_sys::console::error_1(&JsValue::from_str(&load_error));
				audio_asset.set_load_error(load_error);
			}
		}
	});
	Ok(())
}

//==============================================================================
// 메모리에서 읽기. (디코딩을 걸어 두고 바로 돌아온다)
//==============================================================================
pub fn load_audio_bytes(audio_asset: &Rc<AudioAsset>, bytes: Vec<u8>) -> Result<(), String> {
	let byte_array = js_sys::Uint8Array::from(bytes.as_slice());
	let array_buffer = byte_array.buffer();
	let audio_asset = audio_asset.clone();
	spawn_local(async move {
		match decode_array_buffer(&array_buffer).await {
			Ok(audio_buffer) => {
				audio_asset.set_audio_buffer(audio_buffer);
			}
			Err(error) => {
				let load_error = format!("Error loading sound from memory ({})", error);
				web_sys::console::error_1(&JsValue::from_str(&load_error));
				audio_asset.set_load_error(load_error);
			}
		}
	});
	Ok(())
}

//==============================================================================
// fetch 응답을 받아 디코딩한다.
//==============================================================================
async fn fetch_audio_buffer(fetch_promise: js_sys::Promise) -> Result<AudioBuffer, String> {
	let response_value = JsFuture::from(fetch_promise).await.map_err(describe_js_error)?;
	let response = response_value.dyn_into::<web_sys::Response>().map_err(describe_js_error)?;
	if !response.ok() {
		return Err(format!("HTTP {}", response.status()));
	}
	let array_buffer_promise = response.array_buffer().map_err(describe_js_error)?;
	let array_buffer_value = JsFuture::from(array_buffer_promise).await.map_err(describe_js_error)?;
	let array_buffer = array_buffer_value.dyn_into::<js_sys::ArrayBuffer>().map_err(describe_js_error)?;
	decode_array_buffer(&array_buffer).await
}

//==============================================================================
// 디코딩. (OfflineAudioContext 로. 스피커를 잡지 않고, user gesture 전에 만들어도 경고가 없으며,
//  여러 애셋을 한꺼번에 받아도 실시간 AudioContext 의 개수 제한에 걸리지 않는다. 없으면 잠시 쓰는 AudioContext 로 하고 닫는다)
//==============================================================================
async fn decode_array_buffer(array_buffer: &js_sys::ArrayBuffer) -> Result<AudioBuffer, String> {
	if let Ok(offline_audio_context) = web_sys::OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(1, 1, DECODE_SAMPLE_RATE) {
		let decode_promise = offline_audio_context.decode_audio_data(array_buffer).map_err(describe_js_error)?;
		let audio_buffer_value = JsFuture::from(decode_promise).await.map_err(describe_js_error)?;
		return audio_buffer_value.dyn_into::<AudioBuffer>().map_err(describe_js_error);
	}
	let temporary_audio_context = web_sys::AudioContext::new().map_err(|_| String::from("AudioContext is not supported."))?;
	let decode_result = match temporary_audio_context.decode_audio_data(array_buffer) {
		Ok(decode_promise) => {
			JsFuture::from(decode_promise).await.map_err(describe_js_error)
		}
		Err(error) => {
			Err(describe_js_error(error))
		}
	};
	if let Ok(close_promise) = temporary_audio_context.close() {
		let _ = JsFuture::from(close_promise).await;
	}
	let audio_buffer_value = decode_result?;
	audio_buffer_value.dyn_into::<AudioBuffer>().map_err(describe_js_error)
}

//==============================================================================
// 버퍼의 재생 시간. (초)
//==============================================================================
pub fn get_audio_buffer_duration(audio_buffer: &AudioBuffer) -> f64 {
	audio_buffer.duration()
}

//==============================================================================
// 파형을 오실레이터 타입으로.
//==============================================================================
fn to_oscillator_type(beep_waveform: BeepWaveform) -> web_sys::OscillatorType {
	match beep_waveform {
		BeepWaveform::Sine => {
			web_sys::OscillatorType::Sine
		}
		BeepWaveform::Square => {
			web_sys::OscillatorType::Square
		}
		BeepWaveform::Sawtooth => {
			web_sys::OscillatorType::Sawtooth
		}
		BeepWaveform::Triangle => {
			web_sys::OscillatorType::Triangle
		}
	}
}

//==============================================================================
// 비프 톤 하나 재생. (원본 play_tone 의 OscillatorNode + GainNode 스케줄링)
//==============================================================================
pub fn play_beep_tone(audio_context: &mut AudioContext, beep_envelope: &BeepEnvelope) {
	let start_time = audio_context.current_time() + beep_envelope.delay_seconds;
	let end_time = start_time + beep_envelope.duration_seconds;
	let attack_end_time = start_time + beep_envelope.attack_seconds;
	let release_start_time = start_time + beep_envelope.get_release_start_seconds();

	let oscillator_node = match audio_context.create_oscillator() {
		Ok(oscillator_node) => {
			oscillator_node
		}
		Err(_) => {
			return;
		}
	};
	let oscillator_type = to_oscillator_type(beep_envelope.waveform);
	oscillator_node.set_type(oscillator_type);
	let frequency_parameter = oscillator_node.frequency();
	let _ = frequency_parameter.set_value_at_time(beep_envelope.frequency, start_time);

	let gain_node = match audio_context.create_gain() {
		Ok(gain_node) => {
			gain_node
		}
		Err(_) => {
			return;
		}
	};
	let gain_parameter = gain_node.gain();
	let _ = gain_parameter.set_value_at_time(BEEP_FLOOR_VOLUME, start_time);
	let _ = gain_parameter.linear_ramp_to_value_at_time(beep_envelope.peak_volume, attack_end_time);
	let _ = gain_parameter.set_value_at_time(beep_envelope.peak_volume, release_start_time);
	let _ = gain_parameter.exponential_ramp_to_value_at_time(BEEP_FLOOR_VOLUME, end_time);

	let destination = audio_context.destination();
	let _ = oscillator_node.connect_with_audio_node(&gain_node);
	let _ = gain_node.connect_with_audio_node(&destination);
	let _ = oscillator_node.start_with_when(start_time);
	let _ = oscillator_node.stop_with_when(end_time + BEEP_STOP_MARGIN_SECONDS);

	// 끝나면 노드를 뗀다. (한 번 불리고 스스로 풀리는 콜백)
	let oscillator_node_for_ended = oscillator_node.clone();
	let ended_callback = Closure::once_into_js(move || {
		let _ = oscillator_node_for_ended.disconnect();
		let _ = gain_node.disconnect();
	});
	oscillator_node.set_onended(Some(ended_callback.unchecked_ref()));
}
