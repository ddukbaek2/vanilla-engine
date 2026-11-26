//==============================================================================
//! 오디오 플레이어.
//! - 원본 core/audioplayer.js. AudioAsset 에 로드된 오디오 버퍼를 재생한다.
//! - 음량, 음소거, 루프, 재생 위치는 여기서 들고, 소리를 내는 일은 백엔드(platform/audio 의 AudioPlayerBackend)가 한다.
//!   원본의 GainNode 자리: 두 플랫폼 모두 원본처럼 GainNode 하나. (데스크탑은 엔진 믹서의 노드, 셈은 크롬 WebAudio 와 같다)
//! - 만드는 곳은 AudioManager::create_audio_player() 나 AudioPlayer::new(매니저, 내보낼 곳) 이다. AudioContext 가 없으면 아무 소리도 내지 않는 플레이어가 된다. (원본과 같다)
//!   new 는 원본 new AudioPlayer(audioContext, outputNode) 자리다. 첫 인자는 AudioContext 를 든 매니저이고(AudioContext 가 없으면 만든다),
//!   내보낼 곳(AudioOutputNode)은 버스의 get_gain_node()(AudioGainNode)의 as_deref() 다. None 이면 출력이다. 두 플랫폼에서 같은 코드다.
//!     AudioPlayer::new(&audio_manager, audio_bus.get_gain_node().as_deref())
//!   (웹은 &AudioNode 라 다른 AudioNode 도 넘길 수 있고, 데스크탑은 믹서 버스의 &MixerGainNode 다)
//!   데스크탑은 내보낼 곳을 직접 넘겨 만든 플레이어도 원본 WebAudio 처럼 이어진 동안 그 버스 노드를 살려 둔다.
//! - play 는 원본처럼 AudioContext 가 멈춰 있으면 깨운 뒤 튼다. (웹은 resume() 을 기다렸다가, 데스크탑은 그 자리에서)
//! - 플레이어를 버려도 트는 소리는 원본 WebAudio 소스 노드처럼 끝까지 운다. (데스크탑은 믹서가 소스가 끝날 때까지 노드를 들고, 웹은 소스 노드가 운다)
//!   루프 소리는 원본처럼 멈추지 않으니 버리기 전에 stop 한다.
//! - 원본과 다른 점:
//!   애셋은 `Rc<AudioAsset>` 로 나눠 든다.
//!   데스크탑의 is_playing() 은 onended 콜백 대신 믹서가 소스를 끝냈는지 읽는다. (끝나면 거짓)
//!   get_audio_context 는 만든 매니저의 AudioContext 를 빌려 준다. (웹 WebAudio AudioContext, 데스크탑 엔진 믹서의 AudioContext, 매니저 없이 만든 플레이어는 None)
//!   set_playback_rate 는 원본에 없던 것을 러스트 판에서 먼저 만들었다. (원본에도 넣었다)
//!   AudioBus::create_audio_player 로 만든 플레이어는 출력 대신 그 버스로 소리를 낸다. (원본 AudioPlayer 의 출력 노드 인자) 버스 손잡이를 하나 들어 버스를 살려 둔다.
//! - get_time 은 원본처럼 재생 중이면 시작 위치에 오디오 컨텍스트 시계로 흐른 시간을 더한 지금 위치다. (웹 currentTime, 데스크탑 믹서 시계)
//==============================================================================
use crate::core::audiobus::AudioBus;
use crate::core::audiomanager::{AudioManager, AudioManagerState};
use crate::platform::audio::{self as audio_backend, AudioContext, AudioOutputNode, AudioPlayerBackend, GainAutomation};
use std::cell::{Ref, RefCell};
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;

//==============================================================================
/// 음량을 0 ~ 1 로 자른다. (유한하지 않으면 1, 원본 setVolume 의 규칙)
//==============================================================================
pub(crate) fn clamp_volume(volume: f64) -> f64 {
	if volume.is_finite() {
		volume.clamp(0.0, 1.0)
	}
	else {
		1.0
	}
}

//==============================================================================
/// 재생 위치 셈. (시작 위치에 흐른 시간을 더하고, 루프면 길이로 되감고 아니면 길이에서 멈춘다, 원본 readPlaybackPosition)
///
/// # 인자
/// - `start_time`: 시작 위치. (초)
/// - `elapsed_seconds`: 시작한 뒤 재생한 길이. (초, 재생 빠르기를 곱한 것)
/// - `duration`: 버퍼 길이. (초, 0 이하면 시작 위치에 흐른 시간을 더한 그대로)
/// - `is_loop`: 루프인지.
///
/// # 돌려줌
/// 재생 위치. (초)
//==============================================================================
fn read_playback_position(start_time: f64, elapsed_seconds: f64, duration: f64, is_loop: bool) -> f64 {
	let position = start_time + elapsed_seconds;
	if !(duration > 0.0) {
		return position;
	}
	if is_loop {
		return position % duration;
	}
	position.min(duration)
}

//==============================================================================
/// 오디오 플레이어.
/// - 애셋 하나를 실어 틀고, 음량, 음소거, 루프, 재생 위치를 든다. 만든 쪽이 소유하고 버리면 소리도 멈춘다.
///
/// # 예제
/// 애셋을 읽어 1 초 지점부터 반복 재생
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let audio_asset = engine.load_audio_asset("tests/assets/audio/vorbis_stereo.ogg").unwrap();
/// let mut audio_player = engine.get_audio_manager().create_audio_player();
/// audio_player.set_audio_asset(audio_asset);
/// audio_player.set_volume(0.8);
/// audio_player.set_time(1.0);
/// audio_player.play(true);
/// assert!(audio_player.get_duration() > 1.0);
/// audio_player.mute();
/// audio_player.stop();
/// ```
//==============================================================================
pub struct AudioPlayer {
	audio_player_backend: AudioPlayerBackend,
	/// 만든 매니저가 나눠 든 상태. (get_audio_context)
	audio_manager_state: Option<Rc<RefCell<AudioManagerState>>>,
	/// 소리를 내보내는 버스. (버스로 만든 플레이어만, 버스를 살려 둔다)
	_audio_bus: Option<AudioBus>,
	audio_asset: Option<Rc<AudioAsset>>,
	is_muted: bool,
	/// 음량. (0 ~ 1, 음소거를 풀면 이 음량으로 돌아간다)
	volume: f64,
	/// 루프 여부.
	is_loop: bool,
	/// 재생 위치. (초, play() 를 부르면 이 위치부터 재생)
	time: f64,
	/// 재생 빠르기. (다음 play 부터, 백엔드에 넘긴 값과 같다)
	playback_rate: f64,
	/// 지금 소리를 시작한 오디오 컨텍스트 시각. (초)
	start_context_time: f64,
	/// 지금 소리를 시작할 때의 재생 빠르기.
	start_playback_rate: f64,
	/// 지금 소리의 버퍼 길이. (초)
	start_duration: f64,
}

impl AudioPlayer {
	//==============================================================================
	/// 생성. (원본 new AudioPlayer(audioContext, outputNode), 매니저의 AudioContext 가 없으면 만든다, 내보낼 곳이 None 이면 출력)
	///
	/// # 인자
	/// - `audio_manager`: AudioContext 를 든 매니저. (보통 engine.get_audio_manager())
	/// - `output_node`: 소리를 내보낼 노드. (버스의 get_gain_node().as_deref()) None 이면 출력.
	///
	/// # 돌려줌
	/// 새 플레이어. (음량 1, 애셋 없음, AudioContext 를 만들지 못했으면 소리 없는 플레이어)
	///
	/// # 예제
	/// 버스 노드로 내보내는 플레이어
	/// ```rust,no_run
	/// use vanilla::{AudioManager, AudioPlayer};
	///
	/// let audio_manager = AudioManager::new();
	/// let audio_bus = audio_manager.create_audio_bus();
	/// let mut audio_player = AudioPlayer::new(&audio_manager, audio_bus.get_gain_node().as_deref());
	/// audio_player.set_volume(0.5);
	/// assert_eq!(audio_player.get_volume(), 0.5);
	/// ```
	//==============================================================================
	pub fn new(audio_manager: &AudioManager, output_node: Option<AudioOutputNode<'_>>) -> AudioPlayer {
		let audio_manager_state = audio_manager.get_shared_state();
		let audio_player_backend = {
			let mut state = audio_manager_state.borrow_mut();
			state.ensure_audio_context();
			AudioPlayerBackend::create(&mut state, output_node)
		};
		let mut audio_player = AudioPlayer::from_backend(audio_player_backend);
		audio_player.set_audio_manager_state(audio_manager_state);
		audio_player
	}

	//==============================================================================
	/// 만든 백엔드로 생성. (AudioBus::create_audio_player 와 시험이 쓴다)
	//==============================================================================
	pub(crate) fn from_backend(audio_player_backend: AudioPlayerBackend) -> AudioPlayer {
		AudioPlayer {
			audio_player_backend,
			audio_manager_state: None,
			_audio_bus: None,
			audio_asset: None,
			is_muted: false,
			volume: 1.0,
			is_loop: false,
			time: 0.0,
			playback_rate: 1.0,
			start_context_time: 0.0,
			start_playback_rate: 1.0,
			start_duration: 0.0,
		}
	}

	//==============================================================================
	/// 재생. (재생 중이면 멈추고 재생 위치부터 다시 튼다)
	/// - 오디오 컨텍스트가 일시 중단돼 있으면 재개한 뒤에 튼다. (웹은 resume() 을 기다렸다가, 데스크탑은 그 자리에서)
	/// - 소리 없는 플레이어이거나 애셋이 없거나 애셋이 아직 읽히지 않았으면 아무것도 하지 않는다.
	///
	/// # 인자
	/// - `is_loop`: true 면 끝에서 처음으로 돌아가 반복한다.
	//==============================================================================
	pub fn play(&mut self, is_loop: bool) {
		if !self.audio_player_backend.is_available() {
			return;
		}
		let audio_asset = match &self.audio_asset {
			Some(audio_asset) => {
				audio_asset
			}
			None => {
				return;
			}
		};
		let audio_buffer = match audio_asset.get_audio_buffer() {
			Some(audio_buffer) => {
				audio_buffer
			}
			None => {
				return;
			}
		};
		let start_duration = audio_asset.get_duration();

		// 재생 중일 경우 정지.
		self.audio_player_backend.stop_source();

		// 오디오 컨텍스트가 일시 중단된 경우 재개.
		if let Some(audio_manager_state) = &self.audio_manager_state {
			audio_backend::resume_audio_context_before_playback(audio_manager_state);
		}

		// 내부 재생 시작.
		self.is_loop = is_loop;
		self.audio_player_backend.start_playback(&audio_buffer, is_loop, self.time);
		self.start_context_time = self.read_audio_context_time();
		self.start_playback_rate = self.playback_rate;
		self.start_duration = start_duration;
	}

	//==============================================================================
	/// 오디오 컨텍스트 시계. (초, 만든 매니저가 없으면 0)
	//==============================================================================
	fn read_audio_context_time(&self) -> f64 {
		match &self.audio_manager_state {
			Some(audio_manager_state) => {
				audio_backend::get_audio_context_time(&audio_manager_state.borrow())
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 재생 빠르기 설정. (1 이면 그대로, 음 높이도 같이 바뀐다, 다음 play 부터, 원본 setPlaybackRate)
	///
	/// # 인자
	/// - `playback_rate`: 재생 빠르기 배율. (2 면 두 배 빠르게, 0 이하이거나 유한하지 않으면 1)
	//==============================================================================
	pub fn set_playback_rate(&mut self, playback_rate: f64) {
		let playback_rate = if playback_rate.is_finite() && playback_rate > 0.0 { playback_rate } else { 1.0 };
		self.playback_rate = playback_rate;
		self.audio_player_backend.set_playback_rate(playback_rate);
	}

	//==============================================================================
	/// 좌우 설정. (-1 왼쪽 ~ 1 오른쪽, 다음 play 부터 데스크탑, 웹은 바로. SoundSystem 의 효과음 좌우, 원본 AudioPlayer 에는 없다)
	//==============================================================================
	pub(crate) fn set_pan(&mut self, pan: f64) {
		self.audio_player_backend.set_pan(pan as f32);
	}

	//==============================================================================
	/// 공간 소리 설정. (처음이면 GainNode 뒤에 PannerNode 를 끼운다, experimental::spatialaudio 의 SpatialAudioPlayer 가 쓴다, 초안)
	//==============================================================================
	pub(crate) fn set_panner_settings(&mut self, settings: crate::platform::audio::pannermath::PannerSettings) {
		self.audio_player_backend.set_panner_settings(settings);
	}

	//==============================================================================
	/// PannerNode 의 자리나 방향 값 대입. (원본 panner.positionX.value = 값, 초안)
	//==============================================================================
	pub(crate) fn set_panner_param_value(&mut self, param: crate::platform::audio::PannerParam, value: f64) {
		self.audio_player_backend.set_panner_param_value(param, value as f32);
	}

	//==============================================================================
	/// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		self.audio_player_backend.stop();
	}

	//==============================================================================
	/// 그 시각에 멈추기. (원본 SoundSystem 이 소스에 거는 source.stop(when), 끝나면 is_playing 이 거짓이 된다)
	//==============================================================================
	pub(crate) fn stop_at_time(&mut self, time: f64) {
		self.audio_player_backend.stop_source_at_time(time);
	}

	//==============================================================================
	/// GainNode 의 gain 에 자동화 걸기. (원본 SoundSystem 이 gainNode.gain 에 거는 setValueAtTime 들, 음량과 음소거 칸은 건드리지 않는다)
	//==============================================================================
	pub(crate) fn apply_gain_automation(&mut self, automation: GainAutomation) {
		self.audio_player_backend.apply_gain_automation(automation);
	}

	//==============================================================================
	/// GainNode 의 지금 gain. (원본 gainNode.gain.value 읽기, 자동화로 바뀌는 중이면 그 순간의 값)
	//==============================================================================
	pub(crate) fn get_gain_value(&self) -> f64 {
		self.audio_player_backend.get_gain_value() as f64
	}

	//==============================================================================
	/// 음소거. (백엔드가 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn mute(&mut self) {
		if self.audio_player_backend.is_available() {
			self.audio_player_backend.set_gain(0.0);
			self.is_muted = true;
		}
	}

	//==============================================================================
	/// 음소거 해제. (설정한 음량으로 돌아간다)
	//==============================================================================
	pub fn unmute(&mut self) {
		if self.audio_player_backend.is_available() {
			self.audio_player_backend.set_gain(self.volume as f32);
			self.is_muted = false;
		}
	}

	//==============================================================================
	/// 음량 설정. (0 ~ 1, 음소거 중이면 음소거를 풀 때 적용된다)
	///
	/// # 인자
	/// - `volume`: 음량. (0 ~ 1 로 자른다, 유한하지 않으면 1)
	//==============================================================================
	pub fn set_volume(&mut self, volume: f64) {
		let clamped_volume = clamp_volume(volume);
		self.volume = clamped_volume;
		if self.audio_player_backend.is_available() && !self.is_muted {
			self.audio_player_backend.set_gain(clamped_volume as f32);
		}
	}

	//==============================================================================
	/// 음량 반환.
	///
	/// # 돌려줌
	/// 설정한 음량. (0 ~ 1, 음소거해도 바뀌지 않는다)
	//==============================================================================
	pub fn get_volume(&self) -> f64 {
		self.volume
	}

	//==============================================================================
	/// 재생 위치 설정. (초, 재생 중이면 그 위치부터 바로 다시 튼다)
	///
	/// # 인자
	/// - `time`: 재생 위치. (초, 애셋 처음부터)
	//==============================================================================
	pub fn set_time(&mut self, time: f64) {
		self.time = time;
		if self.is_playing() {
			let is_loop = self.is_loop;
			self.play(is_loop);
		}
	}

	//==============================================================================
	/// 오디오 애셋 설정.
	/// - 재생 중인 소리는 바꾸지 않는다. 다음 play 부터 이 애셋을 튼다.
	///
	/// # 인자
	/// - `audio_asset`: 틀 애셋. (engine.load_audio_asset 으로 읽은 것)
	//==============================================================================
	pub fn set_audio_asset(&mut self, audio_asset: Rc<AudioAsset>) {
		self.audio_asset = Some(audio_asset);
	}

	//==============================================================================
	/// 오디오 애셋 반환.
	///
	/// # 돌려줌
	/// 실은 애셋. (없으면 None)
	//==============================================================================
	pub fn get_audio_asset(&self) -> Option<&Rc<AudioAsset>> {
		self.audio_asset.as_ref()
	}

	//==============================================================================
	/// 재생 여부 반환.
	///
	/// # 돌려줌
	/// 지금 소리를 내고 있으면 true. (끝났거나 멈췄으면 false)
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.audio_player_backend.is_playing()
	}

	//==============================================================================
	/// 음소거 여부 반환.
	///
	/// # 돌려줌
	/// 음소거 중이면 true. (소리 없는 플레이어는 mute 를 불러도 false)
	//==============================================================================
	pub fn is_muted(&self) -> bool {
		self.is_muted
	}

	//==============================================================================
	/// 재생 위치 반환. (초, 재생 중이면 흐르는 지금 위치, 멈췄거나 끝났으면 다음 play 가 시작할 set_time 의 위치)
	/// - 재생 중이면 시작 위치에 오디오 컨텍스트 시계로 흐른 시간과 재생 빠르기를 곱해 더한다. (컨텍스트가 멈춰 있으면 같이 멈춘다)
	/// - 루프면 길이로 되감고, 아니면 길이에서 멈춘다.
	///
	/// # 돌려줌
	/// 재생 위치. (초, 기본 0)
	//==============================================================================
	pub fn get_time(&self) -> f64 {
		if !self.is_playing() || self.audio_manager_state.is_none() {
			return self.time;
		}
		let elapsed_seconds = (self.read_audio_context_time() - self.start_context_time).max(0.0) * self.start_playback_rate;
		read_playback_position(self.time, elapsed_seconds, self.start_duration, self.is_loop)
	}

	//==============================================================================
	/// 만든 매니저 상태 넣기. (AudioManager::create_audio_player 가 부른다)
	//==============================================================================
	pub(crate) fn set_audio_manager_state(&mut self, audio_manager_state: Rc<RefCell<AudioManagerState>>) {
		self.audio_manager_state = Some(audio_manager_state);
	}

	//==============================================================================
	/// 소리를 내보내는 버스 넣기. (AudioBus::create_audio_player 가 부른다)
	//==============================================================================
	pub(crate) fn set_audio_bus(&mut self, audio_bus: AudioBus) {
		self._audio_bus = Some(audio_bus);
	}

	//==============================================================================
	/// AudioContext. (원본 getAudioContext, 만든 매니저의 것을 빌려 준다, 없으면 None)
	///
	/// # 돌려줌
	/// 빌린 AudioContext. (매니저에 AudioContext 가 없으면 None)
	//==============================================================================
	pub fn get_audio_context(&self) -> Option<Ref<'_, AudioContext>> {
		let audio_manager_state = self.audio_manager_state.as_ref()?;
		Ref::filter_map(audio_manager_state.borrow(), |state| state.audio_context.as_ref()).ok()
	}

	//==============================================================================
	/// 재생 시간 반환. (초, 애셋이 없으면 0)
	///
	/// # 돌려줌
	/// 실은 애셋의 길이. (초, 애셋이 없거나 아직 읽히지 않았으면 0)
	//==============================================================================
	pub fn get_duration(&self) -> f64 {
		match &self.audio_asset {
			Some(audio_asset) => {
				audio_asset.get_duration()
			}
			None => {
				0.0
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 음량 자르기는 원본 규칙을 따른다.
	//==============================================================================
	#[test]
	fn clamp_volume_follows_original_rule() {
		assert_eq!(clamp_volume(0.5), 0.5);
		assert_eq!(clamp_volume(-0.2), 0.0);
		assert_eq!(clamp_volume(1.7), 1.0);
		assert_eq!(clamp_volume(f64::NAN), 1.0);
		assert_eq!(clamp_volume(f64::INFINITY), 1.0);
	}

	//==============================================================================
	/// 컨텍스트가 없는 플레이어는 음량만 기억하고 음소거는 걸리지 않는다. (원본: gain_node 가 없으면 mute 가 무시된다)
	//==============================================================================
	#[test]
	fn player_without_context_keeps_volume_only() {
		let mut audio_player = AudioPlayer::from_backend(AudioPlayerBackend::create_silent());
		audio_player.set_volume(0.25);
		assert_eq!(audio_player.get_volume(), 0.25);
		audio_player.mute();
		assert!(!audio_player.is_muted());
		let audio_asset = Rc::new(AudioAsset::new());
		audio_player.set_audio_asset(audio_asset);
		audio_player.play(false);
		assert!(!audio_player.is_playing());
		assert_eq!(audio_player.get_duration(), 0.0);
		audio_player.set_time(1.5);
		assert_eq!(audio_player.get_time(), 1.5);
	}

	//==============================================================================
	/// 재생 위치는 시작 위치에 흐른 시간을 더하고, 루프면 길이로 되감고 아니면 길이에서 멈춘다. (원본 getTime 과 같다)
	//==============================================================================
	#[test]
	fn playback_position_flows_and_wraps() {
		assert_eq!(read_playback_position(2.0, 0.0, 10.0, false), 2.0);
		assert_eq!(read_playback_position(2.0, 3.0, 10.0, false), 5.0);
		assert_eq!(read_playback_position(2.0, 30.0, 10.0, false), 10.0);
		assert!((read_playback_position(2.0, 9.0, 10.0, true) - 1.0).abs() < 1e-9);
		assert!((read_playback_position(0.0, 25.0, 10.0, true) - 5.0).abs() < 1e-9);
		assert_eq!(read_playback_position(1.0, 2.0, 0.0, false), 3.0);
	}
}
