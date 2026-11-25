//==============================================================================
// 데스크탑 오디오 백엔드. (윈도우, 맥, 리눅스, kira 0.12)
// - AudioContext 자리: kira::AudioManager. 만들 때 cpal 로 기본 출력 장치를 연다. 장치가 없으면 한 번 알리고 다시 열지 않는다.
//   브라우저 자동재생 정책이 없어서 user gesture 는 처음부터 있는 것으로 둔다.
// - 출력 자리(원본 audio_context.destination): 컨텍스트를 만들 때 메인 트랙 아래에 뿌리 트랙 하나를 만들고, 플레이어, 버스, 비프는 모두 그 아래에 단다.
// - 컨텍스트 멈춤(SoundSystem 이 음소거 동안 부른다): 뿌리 트랙을 kira pause 로 멈춘다. 그 아래 트랙과 소리가 그 자리에서 서고(원본 AudioContext.suspend 처럼
//   다시 돌리면 멈춘 자리부터 운다), 컨텍스트 시계도 서며, is_audio_context_running 은 거짓이다. resume 이 다시 돌린다.
//   (게임 코드가 get_audio_context 로 받은 kira 매니저에 직접 단 트랙은 뿌리 트랙 밖이라 멈추지 않는다)
// - 좌우(pan): 재생할 때 소리 설정의 panning 으로 건다. (-1 왼쪽 ~ 1 오른쪽, 원본 StereoPannerNode 자리, 다음 재생부터)
// - AudioBuffer 자리: kira 의 StaticSoundData. 파일 전체를 디코딩해 들고, 복제는 Arc 하나라 값싸다. (ogg vorbis, mp3, wav, flac, m4a(aac))
// - AudioPlayer 하나 = 믹서 하위 트랙 하나. (원본 GainNode 자리, 트랙 음량이 플레이어 음량이다)
//   내보낼 곳(AudioOutputNode, 원본 outputNode)을 주면 그 트랙 아래, 없으면 뿌리 트랙 아래에 단다.
//   재생할 때마다 버퍼를 복제해 그 트랙에 튼다. 트랙 핸들을 버리면 트랙과 그 위의 소리가 같이 없어진다.
// - AudioBus 하나 = 뿌리 트랙 아래 하위 트랙 하나. (원본 출력에 이은 GainNode 자리) 버스로 만든 플레이어의 트랙은 그 아래에 달려
//   버스 트랙의 음량이 함께 곱해진다. 버스 트랙 핸들을 버리면 그 아래 플레이어 트랙도 같이 없어진다.
// - 음량(0 ~ 1)은 데시벨로 바꿔 건다. kira 는 -60dB 이하를 무음으로 친다.
// - 원본 WebAudio 처럼 음량 바꾸기와 정지는 바로 한다. (트윈 0초)
// - 컨텍스트 시계(get_audio_context_time)는 컨텍스트를 만든 뒤 돌고 있던 시간이다. (멈춘 동안은 세지 않는다)
// - BeepPlayer 의 톤은 엔벨로프를 곱한 파형을 샘플로 그려 뿌리 트랙에 튼다.
//   파형은 크롬 WebAudio 오실레이터(PeriodicWave)와 같은 셈이다. 푸리에 계수를 표 크기의 반 아래까지 두고 주파수에 따라 1/3 옥타브마다 높은 배음을 덜어 낸
//   두 표 사이를 섞으며(대역 제한), 배음을 다 둔 첫 표의 가장 큰 값이 1 이 되게 크기를 맞춘다.
//==============================================================================
use crate::core::audiomanager::AudioManagerState;
use crate::misc::beepplayer::{BeepEnvelope, BeepWaveform, BEEP_FLOOR_VOLUME, BEEP_STOP_MARGIN_SECONDS};
use crate::resource::audioasset::AudioAsset;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::sound::{PlaybackState, Region};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManagerSettings, Capacities, Decibels, DefaultBackend, Frame, StartTime, Tween};
use std::cell::RefCell;
use std::io::Cursor;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

// 공유 오디오 컨텍스트. (원본 AudioContext 자리)
pub type AudioContext = kira::AudioManager<DefaultBackend>;
// 디코딩한 소리. (원본 AudioBuffer 자리)
pub type AudioBuffer = StaticSoundData;
// 플레이어가 소리를 내보낼 곳. (원본 AudioPlayer 생성자의 outputNode 자리, 그 트랙 아래에 플레이어 트랙을 단다)
pub type AudioOutputNode<'a> = &'a mut TrackHandle;

// 데스크탑에는 자동재생 정책이 없다.
pub const HAS_USER_GESTURE_AT_START: bool = true;
// 하위 트랙 최대 수. (플레이어 하나가 하나씩 쓴다. kira 기본은 128)
const SUB_TRACK_CAPACITY: usize = 1024;
// 플레이어 트랙 하나에 동시에 있을 수 있는 소리 수. (다시 틀 때 앞 소리가 아직 빠지지 않았을 때를 위해 여유를 둔다)
const PLAYER_SOUND_CAPACITY: usize = 8;
// 비프 톤을 그리는 샘플레이트.
const BEEP_SAMPLE_RATE: u32 = 44100;
// 대역 제한 표의 한 옥타브를 나누는 구간 수. (크롬 PeriodicWave 의 kNumberOfOctaveBands)
const PERIODIC_WAVE_OCTAVE_BAND_COUNT: f64 = 3.0;
// 한 옥타브의 센트.
const CENTS_PER_OCTAVE: f64 = 1200.0;

//==============================================================================
// 매니저가 드는 백엔드 상태.
//==============================================================================
pub struct AudioPlatformState {
	// 소리 장치를 열지 못했는지. (한 번 실패하면 다시 열지 않는다)
	is_audio_context_failed: bool,
	// 컨텍스트를 멈춰 두었는지. (뿌리 트랙을 멈춘 동안)
	is_audio_context_suspended: bool,
	// 뿌리 트랙. (원본 audio_context.destination 자리, 플레이어, 버스, 비프가 그 아래에 달린다)
	root_track_handle: Option<TrackHandle>,
	// 컨텍스트가 마지막으로 돌기 시작한 때. (멈춰 있으면 None)
	audio_context_running_instant: Option<Instant>,
	// 그 전까지 돈 시간. (초, 멈출 때 더한다)
	audio_context_elapsed_seconds: f64,
}

//==============================================================================
// 백엔드 상태 생성.
//==============================================================================
pub fn create_audio_platform_state() -> AudioPlatformState {
	AudioPlatformState {
		is_audio_context_failed: false,
		is_audio_context_suspended: false,
		root_track_handle: None,
		audio_context_running_instant: None,
		audio_context_elapsed_seconds: 0.0,
	}
}

//==============================================================================
// 오디오 컨텍스트 생성. (kira 매니저와 기본 출력 장치, 메인 트랙 아래 뿌리 트랙)
//==============================================================================
pub fn create_audio_context(audio_platform_state: &mut AudioPlatformState) -> Option<AudioContext> {
	if audio_platform_state.is_audio_context_failed {
		return None;
	}
	let audio_manager_settings = AudioManagerSettings::<DefaultBackend> {
		capacities: Capacities {
			sub_track_capacity: SUB_TRACK_CAPACITY,
			..Capacities::default()
		},
		..AudioManagerSettings::default()
	};
	let mut audio_context = match AudioContext::new(audio_manager_settings) {
		Ok(audio_context) => {
			audio_context
		}
		Err(error) => {
			eprintln!("AudioManager: 소리 장치를 열지 못했습니다. 소리 없이 계속합니다. ({})", error);
			audio_platform_state.is_audio_context_failed = true;
			return None;
		}
	};
	let root_track_builder = TrackBuilder::new().sub_track_capacity(SUB_TRACK_CAPACITY);
	match audio_context.add_sub_track(root_track_builder) {
		Ok(root_track_handle) => {
			audio_platform_state.root_track_handle = Some(root_track_handle);
		}
		Err(error) => {
			eprintln!("AudioManager: 뿌리 믹서 트랙을 만들지 못했습니다. 소리 없이 계속합니다. ({})", error);
			audio_platform_state.is_audio_context_failed = true;
			return None;
		}
	}
	audio_platform_state.is_audio_context_suspended = false;
	audio_platform_state.audio_context_running_instant = Some(Instant::now());
	audio_platform_state.audio_context_elapsed_seconds = 0.0;
	Some(audio_context)
}

//==============================================================================
// 오디오 컨텍스트 재개. (suspend_audio_context 로 멈춰 둔 것만 뿌리 트랙을 다시 돌린다)
//==============================================================================
pub fn resume_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let state = &mut *state_cell.borrow_mut();
	let platform_state = &mut state.platform_state;
	if state.audio_context.is_none() || !platform_state.is_audio_context_suspended {
		return;
	}
	if let Some(root_track_handle) = platform_state.root_track_handle.as_mut() {
		root_track_handle.resume(create_immediate_tween());
	}
	platform_state.is_audio_context_suspended = false;
	platform_state.audio_context_running_instant = Some(Instant::now());
}

//==============================================================================
// 오디오 컨텍스트 멈춤. (돌고 있을 때만 뿌리 트랙을 멈추고 시계를 세운다, 원본 audio_context.suspend() 자리)
//==============================================================================
pub fn suspend_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let state = &mut *state_cell.borrow_mut();
	let platform_state = &mut state.platform_state;
	if state.audio_context.is_none() || platform_state.is_audio_context_suspended {
		return;
	}
	if let Some(root_track_handle) = platform_state.root_track_handle.as_mut() {
		root_track_handle.pause(create_immediate_tween());
	}
	if let Some(audio_context_running_instant) = platform_state.audio_context_running_instant.take() {
		platform_state.audio_context_elapsed_seconds += audio_context_running_instant.elapsed().as_secs_f64();
	}
	platform_state.is_audio_context_suspended = true;
}

//==============================================================================
// 재생하기 전에 멈춘 컨텍스트 깨우기. (원본 AudioPlayer.play 가 "suspended" 면 resume() 뒤에 트는 자리, 데스크탑은 그 자리에서 돈다)
//==============================================================================
pub fn resume_audio_context_before_playback(state_cell: &Rc<RefCell<AudioManagerState>>) {
	resume_audio_context(state_cell);
}

//==============================================================================
// 오디오 컨텍스트가 있고 돌고 있는지. (멈춰 둔 동안은 거짓)
//==============================================================================
pub fn is_audio_context_running(state: &AudioManagerState) -> bool {
	state.audio_context.is_some() && !state.platform_state.is_audio_context_suspended
}

//==============================================================================
// 오디오 컨텍스트 시계. (초, 원본 audio_context.currentTime 자리, 컨텍스트를 만든 뒤 돌고 있던 시간, 없으면 0)
//==============================================================================
pub fn get_audio_context_time(state: &AudioManagerState) -> f64 {
	let platform_state = &state.platform_state;
	let running_seconds = match platform_state.audio_context_running_instant {
		Some(audio_context_running_instant) => {
			audio_context_running_instant.elapsed().as_secs_f64()
		}
		None => {
			0.0
		}
	};
	platform_state.audio_context_elapsed_seconds + running_seconds
}

//==============================================================================
// 바로 끝나는 트윈. (원본 gain.value 대입, source.stop() 처럼 바로 바꾼다)
//==============================================================================
fn create_immediate_tween() -> Tween {
	Tween {
		duration: Duration::ZERO,
		..Tween::default()
	}
}

//==============================================================================
// 선형 음량(0 ~ 1)을 데시벨로. (0 이하는 무음)
//==============================================================================
pub(crate) fn amplitude_to_decibels(amplitude: f32) -> Decibels {
	if amplitude <= 0.0 {
		return Decibels::SILENCE;
	}
	let decibels = 20.0 * amplitude.log10();
	Decibels(decibels.max(Decibels::SILENCE.0))
}

//==============================================================================
// 부모 트랙 아래에 플레이어 트랙 하나 만들기. (트랙이 모자라면 알리고 None)
//==============================================================================
fn create_player_track(parent_track_handle: &mut TrackHandle) -> Option<TrackHandle> {
	let track_builder = TrackBuilder::new().sound_capacity(PLAYER_SOUND_CAPACITY);
	match parent_track_handle.add_sub_track(track_builder) {
		Ok(track_handle) => {
			Some(track_handle)
		}
		Err(error) => {
			eprintln!("AudioPlayer: 믹서 트랙을 만들지 못했습니다. ({})", error);
			None
		}
	}
}

//==============================================================================
// 플레이어 하나의 소리 내는 부분. (하위 트랙 하나와 지금 재생 중인 소리 핸들)
//==============================================================================
pub struct AudioPlayerBackend {
	track_handle: Option<TrackHandle>,
	sound_handle: Option<StaticSoundHandle>,
	playback_rate: f64,
	panning: f32,
}

impl AudioPlayerBackend {
	//==============================================================================
	// 생성. (내보낼 트랙을 주면 그 아래, 없으면 컨텍스트의 뿌리 트랙 아래에 하위 트랙을 하나 만든다. 컨텍스트가 없거나 트랙이 모자라면 소리 없는 플레이어)
	//==============================================================================
	pub fn create(state: &mut AudioManagerState, output_node: Option<AudioOutputNode<'_>>) -> AudioPlayerBackend {
		let track_handle = match output_node {
			Some(output_track_handle) => {
				create_player_track(output_track_handle)
			}
			None => {
				match state.platform_state.root_track_handle.as_mut() {
					Some(root_track_handle) => {
						create_player_track(root_track_handle)
					}
					None => {
						None
					}
				}
			}
		};
		AudioPlayerBackend::create_with_track(track_handle)
	}

	//==============================================================================
	// 소리 없는 플레이어 생성. (시험)
	//==============================================================================
	#[cfg(test)]
	pub fn create_silent() -> AudioPlayerBackend {
		AudioPlayerBackend::create_with_track(None)
	}

	//==============================================================================
	// 만든 트랙으로 생성. (트랙이 없으면 소리 없는 플레이어)
	//==============================================================================
	fn create_with_track(track_handle: Option<TrackHandle>) -> AudioPlayerBackend {
		AudioPlayerBackend {
			track_handle,
			sound_handle: None,
			playback_rate: 1.0,
			panning: 0.0,
		}
	}

	//==============================================================================
	// 소리를 낼 수 있는지. (원본 audio_context && gain_node)
	//==============================================================================
	pub fn is_available(&self) -> bool {
		self.track_handle.is_some()
	}

	//==============================================================================
	// 재생 중인 소리를 멈춘다.
	//==============================================================================
	pub fn stop_source(&mut self) {
		if let Some(mut sound_handle) = self.sound_handle.take() {
			sound_handle.stop(create_immediate_tween());
		}
	}

	//==============================================================================
	// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		self.stop_source();
	}

	//==============================================================================
	// 재생 시작. (버퍼를 복제해 time 초부터, 루프면 처음부터 끝까지 되풀이)
	//==============================================================================
	pub fn start_playback(&mut self, audio_buffer: &AudioBuffer, is_loop: bool, time: f64) {
		let track_handle = match self.track_handle.as_mut() {
			Some(track_handle) => {
				track_handle
			}
			None => {
				return;
			}
		};
		let loop_region: Option<Region> = if is_loop {
			Some(Region::from(..))
		}
		else {
			None
		};
		let sound_data = audio_buffer.start_position(time).loop_region(loop_region).playback_rate(self.playback_rate).panning(self.panning);
		match track_handle.play(sound_data) {
			Ok(sound_handle) => {
				self.sound_handle = Some(sound_handle);
			}
			Err(error) => {
				eprintln!("AudioPlayer: 재생하지 못했습니다. ({})", error);
			}
		}
	}

	//==============================================================================
	// 재생 빠르기 설정. (1 이면 그대로, 음 높이도 같이 바뀐다, 다음 재생부터)
	//==============================================================================
	pub fn set_playback_rate(&mut self, playback_rate: f64) {
		self.playback_rate = playback_rate;
	}

	//==============================================================================
	// 좌우 설정. (-1 왼쪽 ~ 1 오른쪽, 다음 재생부터)
	//==============================================================================
	pub fn set_pan(&mut self, pan: f32) {
		self.panning = pan;
	}

	//==============================================================================
	// 음량 설정. (트랙 음량, 0 ~ 1)
	//==============================================================================
	pub fn set_gain(&mut self, gain: f32) {
		if let Some(track_handle) = self.track_handle.as_mut() {
			let decibels = amplitude_to_decibels(gain);
			track_handle.set_volume(decibels, create_immediate_tween());
		}
	}

	//==============================================================================
	// 재생 중인지. (소리 핸들이 있고 아직 멈추지 않았으면 참)
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		match &self.sound_handle {
			Some(sound_handle) => {
				sound_handle.state() != PlaybackState::Stopped
			}
			None => {
				false
			}
		}
	}
}

//==============================================================================
// 버스 하나의 소리 모으는 부분. (뿌리 트랙 아래 하위 트랙 하나, 버스로 만든 플레이어의 트랙이 그 아래에 달린다)
//==============================================================================
pub struct AudioBusBackend {
	track_handle: Option<TrackHandle>,
}

impl AudioBusBackend {
	//==============================================================================
	// 생성. (컨텍스트가 있으면 뿌리 트랙 아래 하위 트랙을 하나 만든다. 없거나 트랙이 모자라면 소리 없는 버스)
	//==============================================================================
	pub fn create(state: &mut AudioManagerState) -> AudioBusBackend {
		let track_handle = match state.platform_state.root_track_handle.as_mut() {
			Some(root_track_handle) => {
				let track_builder = TrackBuilder::new().sub_track_capacity(SUB_TRACK_CAPACITY);
				match root_track_handle.add_sub_track(track_builder) {
					Ok(track_handle) => {
						Some(track_handle)
					}
					Err(error) => {
						eprintln!("AudioBus: 믹서 트랙을 만들지 못했습니다. ({})", error);
						None
					}
				}
			}
			None => {
				None
			}
		};
		AudioBusBackend {
			track_handle,
		}
	}

	//==============================================================================
	// 소리를 모을 수 있는지. (원본 audio_context && gain_node)
	//==============================================================================
	pub fn is_available(&self) -> bool {
		self.track_handle.is_some()
	}

	//==============================================================================
	// 음량 설정. (버스 트랙 음량, 0 ~ 1)
	//==============================================================================
	pub fn set_gain(&mut self, gain: f32) {
		if let Some(track_handle) = self.track_handle.as_mut() {
			let decibels = amplitude_to_decibels(gain);
			track_handle.set_volume(decibels, create_immediate_tween());
		}
	}

	//==============================================================================
	// 이 버스로 트는 플레이어 백엔드 생성. (버스 트랙 아래 하위 트랙 하나, 버스가 소리 없으면 소리 없는 플레이어)
	//==============================================================================
	pub fn create_player_backend(&mut self) -> AudioPlayerBackend {
		let track_handle = match self.track_handle.as_mut() {
			Some(bus_track_handle) => {
				create_player_track(bus_track_handle)
			}
			None => {
				None
			}
		};
		AudioPlayerBackend::create_with_track(track_handle)
	}

	//==============================================================================
	// 버스 트랙. (원본 getGainNode 자리, 소리 없는 버스는 None)
	//==============================================================================
	pub fn get_gain_node_mut(&mut self) -> Option<&mut TrackHandle> {
		self.track_handle.as_mut()
	}
}

//==============================================================================
// 파일 읽기. (그 자리에서 읽고 디코딩한다)
//==============================================================================
pub fn load_audio_file(audio_asset: &Rc<AudioAsset>, asset_path: &Path) -> Result<(), String> {
	match StaticSoundData::from_file(asset_path) {
		Ok(audio_buffer) => {
			audio_asset.set_audio_buffer(audio_buffer);
			Ok(())
		}
		Err(error) => {
			let load_error = format!("Error loading sound: {} ({})", asset_path.display(), error);
			audio_asset.set_load_error(load_error.clone());
			Err(load_error)
		}
	}
}

//==============================================================================
// 메모리에서 읽기. (그 자리에서 디코딩한다)
//==============================================================================
pub fn load_audio_bytes(audio_asset: &Rc<AudioAsset>, bytes: Vec<u8>) -> Result<(), String> {
	match StaticSoundData::from_cursor(Cursor::new(bytes)) {
		Ok(audio_buffer) => {
			audio_asset.set_audio_buffer(audio_buffer);
			Ok(())
		}
		Err(error) => {
			let load_error = format!("Error loading sound from memory ({})", error);
			audio_asset.set_load_error(load_error.clone());
			Err(load_error)
		}
	}
}

//==============================================================================
// 버퍼의 재생 시간. (초)
//==============================================================================
pub fn get_audio_buffer_duration(audio_buffer: &AudioBuffer) -> f64 {
	audio_buffer.duration().as_secs_f64()
}

//==============================================================================
// 비프 톤 하나 재생. (그린 샘플을 뿌리 트랙에, 지연이 있으면 kira 의 지연 시작으로)
//==============================================================================
pub fn play_beep_tone(state: &mut AudioManagerState, beep_envelope: &BeepEnvelope) {
	let root_track_handle = match state.platform_state.root_track_handle.as_mut() {
		Some(root_track_handle) => {
			root_track_handle
		}
		None => {
			return;
		}
	};
	let frame_list = render_beep_frames(beep_envelope, BEEP_SAMPLE_RATE);
	let start_time = if beep_envelope.delay_seconds > 0.0 {
		StartTime::Delayed(Duration::from_secs_f64(beep_envelope.delay_seconds))
	}
	else {
		StartTime::Immediate
	};
	let beep_sound_data = StaticSoundData {
		sample_rate: BEEP_SAMPLE_RATE,
		frames: Arc::from(frame_list),
		settings: StaticSoundSettings::new().start_time(start_time),
		slice: None,
	};
	if let Err(error) = root_track_handle.play(beep_sound_data) {
		eprintln!("BeepPlayer: 재생하지 못했습니다. ({})", error);
	}
}

//==============================================================================
// 비프 톤을 샘플로 그린다. (원본 오실레이터가 도는 길이 = 엔벨로프 길이 + 0.01초)
//==============================================================================
fn render_beep_frames(beep_envelope: &BeepEnvelope, sample_rate: u32) -> Vec<Frame> {
	let sample_rate_value = sample_rate as f64;
	let total_seconds = beep_envelope.duration_seconds + BEEP_STOP_MARGIN_SECONDS;
	let frame_count = (total_seconds * sample_rate_value).ceil() as usize;
	let phase_step = beep_envelope.frequency as f64 / sample_rate_value;
	let band_limited_waveform = BandLimitedWaveform::create(beep_envelope.waveform, beep_envelope.frequency as f64, sample_rate_value);
	let mut phase = 0.0f64;
	let mut frame_list = Vec::with_capacity(frame_count);
	for frame_index in 0..frame_count {
		let elapsed_seconds = frame_index as f64 / sample_rate_value;
		let gain = get_beep_envelope_gain(beep_envelope, elapsed_seconds);
		let sample = band_limited_waveform.read_sample(phase) * gain;
		frame_list.push(Frame::from_mono(sample));
		phase += phase_step;
		phase -= phase.floor();
	}
	frame_list
}

//==============================================================================
// 엔벨로프 음량. (원본 GainNode 자동화와 같은 값, elapsed_seconds 는 톤 시작 기준)
// - 0 ~ attack: 바닥에서 최고까지 선형, attack ~ release 시작: 최고 유지, release 시작 ~ 끝: 바닥까지 지수, 그 뒤 바닥.
//==============================================================================
fn get_beep_envelope_gain(beep_envelope: &BeepEnvelope, elapsed_seconds: f64) -> f32 {
	let floor_volume = BEEP_FLOOR_VOLUME as f64;
	let peak_volume = beep_envelope.peak_volume as f64;
	let release_start_seconds = beep_envelope.get_release_start_seconds();
	if elapsed_seconds < beep_envelope.attack_seconds {
		let progress = elapsed_seconds / beep_envelope.attack_seconds;
		return (floor_volume + (peak_volume - floor_volume) * progress) as f32;
	}
	if elapsed_seconds < release_start_seconds {
		return peak_volume as f32;
	}
	if elapsed_seconds < beep_envelope.duration_seconds {
		let progress = (elapsed_seconds - release_start_seconds) / (beep_envelope.duration_seconds - release_start_seconds);
		return (peak_volume * (floor_volume / peak_volume).powf(progress)) as f32;
	}
	floor_volume as f32
}

//==============================================================================
// 대역 제한 표 크기. (크롬 PeriodicWave: 샘플레이트 24000 이하 2048, 88200 이하 4096, 그 위 16384)
//==============================================================================
fn get_periodic_wave_size(sample_rate: f64) -> usize {
	if sample_rate <= 24000.0 {
		return 2048;
	}
	if sample_rate <= 88200.0 {
		return 4096;
	}
	16384
}

//==============================================================================
// 파형의 푸리에 계수. (사인 항 b[harmonic_index], 크롬 GenerateBasicWaveform 과 같은 식, 0 번은 0)
//==============================================================================
fn get_fourier_coefficient(beep_waveform: BeepWaveform, harmonic_index: usize) -> f64 {
	if harmonic_index == 0 {
		return 0.0;
	}
	let is_odd = harmonic_index % 2 == 1;
	let pi_factor = 2.0 / (harmonic_index as f64 * std::f64::consts::PI);
	match beep_waveform {
		BeepWaveform::Sine => {
			if harmonic_index == 1 {
				1.0
			}
			else {
				0.0
			}
		}
		BeepWaveform::Square => {
			if is_odd {
				2.0 * pi_factor
			}
			else {
				0.0
			}
		}
		BeepWaveform::Sawtooth => {
			if is_odd {
				pi_factor
			}
			else {
				-pi_factor
			}
		}
		BeepWaveform::Triangle => {
			if !is_odd {
				return 0.0;
			}
			let sign = if ((harmonic_index - 1) / 2) % 2 == 1 {
				-1.0
			}
			else {
				1.0
			};
			2.0 * pi_factor * pi_factor * sign
		}
	}
}

//==============================================================================
// 정규화 배율. (배음을 다 둔 첫 표의 표 칸 값 가운데 가장 큰 크기의 역수, 크롬 normalization_scale, 파형과 표 크기마다 한 번 셈해 둔다)
// - 네 파형은 모두 f(1 - x) = -f(x) 라서 표의 앞 반과 가운데만 본다.
//==============================================================================
fn get_normalization_scale(beep_waveform: BeepWaveform, periodic_wave_size: usize) -> f64 {
	thread_local! {
		static NORMALIZATION_SCALE_LIST: RefCell<Vec<(BeepWaveform, usize, f64)>> = const { RefCell::new(Vec::new()) };
	}
	let cached_scale = NORMALIZATION_SCALE_LIST.with(|normalization_scale_list| {
		normalization_scale_list.borrow().iter().find(|(cached_waveform, cached_size, _)| *cached_waveform == beep_waveform && *cached_size == periodic_wave_size).map(|(_, _, cached_scale)| *cached_scale)
	});
	if let Some(cached_scale) = cached_scale {
		return cached_scale;
	}
	let half_size = periodic_wave_size / 2;
	let index_mask = periodic_wave_size - 1;
	let sine_table: Vec<f64> = (0..periodic_wave_size).map(|table_index| (std::f64::consts::TAU * table_index as f64 / periodic_wave_size as f64).sin()).collect();
	let harmonic_list: Vec<(usize, f64)> = (1..half_size).map(|harmonic_index| (harmonic_index, get_fourier_coefficient(beep_waveform, harmonic_index))).filter(|(_, coefficient)| *coefficient != 0.0).collect();
	let mut maximum_value = 0.0f64;
	for table_index in 0..=half_size {
		let mut value = 0.0f64;
		for (harmonic_index, coefficient) in harmonic_list.iter() {
			value += coefficient * sine_table[(harmonic_index * table_index) & index_mask];
		}
		maximum_value = maximum_value.max(value.abs());
	}
	let normalization_scale = if maximum_value > 0.0 {
		1.0 / maximum_value
	}
	else {
		1.0
	};
	NORMALIZATION_SCALE_LIST.with(|normalization_scale_list| {
		normalization_scale_list.borrow_mut().push((beep_waveform, periodic_wave_size, normalization_scale));
	});
	normalization_scale
}

//==============================================================================
// 대역을 제한한 파형 하나. (주파수 하나에 맞춘 크롬 OscillatorNode 의 두 표 섞기)
//==============================================================================
struct BandLimitedWaveform {
	// 배음을 더 둔 표의 계수. (1 번부터, 배음을 덜 둔 표는 앞의 lower_partial_count 개)
	coefficient_list: Vec<f64>,
	lower_partial_count: usize,
	// 두 표를 섞는 비. (0 이면 배음을 더 둔 표, 1 이면 덜 둔 표)
	table_interpolation_factor: f64,
	normalization_scale: f64,
}

impl BandLimitedWaveform {
	//==============================================================================
	// 생성. (크롬 WaveDataForFundamentalFrequency 처럼 주파수로 표 둘과 섞는 비를 고른다)
	// - 표 r 의 배음 수는 표 크기의 반 x 2^(-r/3) 을 내린 것이다. (표 크기의 반 - 1 을 넘지 않는다)
	//==============================================================================
	fn create(beep_waveform: BeepWaveform, frequency: f64, sample_rate: f64) -> BandLimitedWaveform {
		let periodic_wave_size = get_periodic_wave_size(sample_rate);
		let half_size = periodic_wave_size / 2;
		let range_count = (PERIODIC_WAVE_OCTAVE_BAND_COUNT * (periodic_wave_size as f64).log2()).round() as usize;
		let cents_per_range = CENTS_PER_OCTAVE / PERIODIC_WAVE_OCTAVE_BAND_COUNT;
		let lowest_fundamental_frequency = sample_rate / periodic_wave_size as f64;
		let absolute_frequency = frequency.abs();
		let frequency_ratio = if absolute_frequency > 0.0 {
			absolute_frequency / lowest_fundamental_frequency
		}
		else {
			0.5
		};
		let cents_above_lowest_frequency = frequency_ratio.log2() * CENTS_PER_OCTAVE;
		let pitch_range = (1.0 + cents_above_lowest_frequency / cents_per_range).clamp(0.0, (range_count - 1) as f64);
		let higher_range_index = pitch_range as usize;
		let lower_range_index = if higher_range_index < range_count - 1 {
			higher_range_index + 1
		}
		else {
			higher_range_index
		};
		let read_partial_count = |range_index: usize| -> usize {
			let culling_scale = 2.0f64.powf(-(range_index as f64 * cents_per_range) / CENTS_PER_OCTAVE);
			let partial_count = (culling_scale * half_size as f64) as usize;
			partial_count.min(half_size - 1)
		};
		let higher_partial_count = read_partial_count(higher_range_index);
		let lower_partial_count = read_partial_count(lower_range_index);
		BandLimitedWaveform {
			coefficient_list: (1..=higher_partial_count).map(|harmonic_index| get_fourier_coefficient(beep_waveform, harmonic_index)).collect(),
			lower_partial_count,
			table_interpolation_factor: pitch_range - higher_range_index as f64,
			normalization_scale: get_normalization_scale(beep_waveform, periodic_wave_size),
		}
	}

	//==============================================================================
	// 파형 한 점. (phase: 0 ~ 1, 0 에서 시작해 위로 올라가는 꼴, 사인 배음은 점화식 sin(nθ) = 2cos(θ)sin((n-1)θ) - sin((n-2)θ) 로 잇는다)
	//==============================================================================
	fn read_sample(&self, phase: f64) -> f32 {
		let angle = phase * std::f64::consts::TAU;
		let doubled_cosine = 2.0 * angle.cos();
		let mut previous_sine = 0.0f64;
		let mut current_sine = angle.sin();
		let mut higher_sum = 0.0f64;
		let mut lower_sum = 0.0f64;
		for (coefficient_index, coefficient) in self.coefficient_list.iter().enumerate() {
			let term = coefficient * current_sine;
			higher_sum += term;
			if coefficient_index < self.lower_partial_count {
				lower_sum += term;
			}
			let next_sine = doubled_cosine * current_sine - previous_sine;
			previous_sine = current_sine;
			current_sine = next_sine;
		}
		let sample = ((1.0 - self.table_interpolation_factor) * higher_sum + self.table_interpolation_factor * lower_sum) * self.normalization_scale;
		sample as f32
	}

	//==============================================================================
	// 배음을 더 둔 표의 배음 수.
	//==============================================================================
	#[cfg(test)]
	fn get_higher_partial_count(&self) -> usize {
		self.coefficient_list.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 음량을 데시벨로 바꾸면 1 은 0dB, 0.5 는 약 -6dB, 0 과 아주 작은 값은 무음이다.
	//==============================================================================
	#[test]
	fn amplitude_to_decibels_maps_volume() {
		assert_eq!(amplitude_to_decibels(1.0), Decibels::IDENTITY);
		assert!((amplitude_to_decibels(0.5).0 - (-6.0206)).abs() < 1e-3);
		assert_eq!(amplitude_to_decibels(0.0), Decibels::SILENCE);
		assert_eq!(amplitude_to_decibels(0.00001), Decibels::SILENCE);
		assert!((amplitude_to_decibels(0.5).as_amplitude() - 0.5).abs() < 1e-5);
	}

	//==============================================================================
	// 엔벨로프는 바닥에서 올라 최고를 지키다 바닥으로 내려간다.
	//==============================================================================
	#[test]
	fn beep_envelope_gain_follows_automation() {
		let beep_envelope = BeepEnvelope::create(880.0, 0.045, BeepWaveform::Sine, 0.0, 0.35, 1.0);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.0) - BEEP_FLOOR_VOLUME).abs() < 1e-7);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.0025) - (0.0001 + 0.3499 * 0.5)).abs() < 1e-5);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.01) - 0.35).abs() < 1e-6);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.024) - 0.35).abs() < 1e-6);
		let middle_gain = get_beep_envelope_gain(&beep_envelope, 0.035);
		assert!((middle_gain as f64 - (0.35f64 * 0.0001).sqrt()).abs() < 1e-5);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.05) - BEEP_FLOOR_VOLUME).abs() < 1e-7);
	}

	//==============================================================================
	// 파형은 0 에서 시작해 위로 올라가고, 사인은 그대로 사인이다. 정규화 배율은 크롬처럼 배음을 다 둔 표의 가장 큰 값으로 맞춘다.
	// - 기대값은 같은 식(표 4096 칸, 배음 2047 개)을 파이썬으로 따로 셈한 것이다. (사각 1.17898, 톱니 1.17849, 삼각 0.99980)
	//==============================================================================
	#[test]
	fn band_limited_waveform_matches_periodic_wave() {
		let sample_rate = BEEP_SAMPLE_RATE as f64;
		for beep_waveform in [BeepWaveform::Sine, BeepWaveform::Square, BeepWaveform::Sawtooth, BeepWaveform::Triangle] {
			let band_limited_waveform = BandLimitedWaveform::create(beep_waveform, 440.0, sample_rate);
			assert!(band_limited_waveform.read_sample(0.0).abs() < 1e-6);
			assert!(band_limited_waveform.read_sample(0.1) > 0.0);
		}
		let sine_waveform = BandLimitedWaveform::create(BeepWaveform::Sine, 880.0, sample_rate);
		for step_index in 0..16 {
			let phase = step_index as f64 / 16.0;
			assert!((sine_waveform.read_sample(phase) as f64 - (phase * std::f64::consts::TAU).sin()).abs() < 1e-6);
		}
		assert!((get_normalization_scale(BeepWaveform::Square, 4096) - 1.0 / 1.1789798).abs() < 1e-5);
		assert!((get_normalization_scale(BeepWaveform::Sawtooth, 4096) - 1.0 / 1.1784914).abs() < 1e-5);
		assert!((get_normalization_scale(BeepWaveform::Triangle, 4096) - 1.0 / 0.9998021).abs() < 1e-5);

		// 사각파의 가운데는 정규화 배율 근처, 삼각파의 꼭대기는 1 근처다.
		let square_waveform = BandLimitedWaveform::create(BeepWaveform::Square, 440.0, sample_rate);
		assert!((square_waveform.read_sample(0.25) - 0.848).abs() < 0.03);
		assert!((square_waveform.read_sample(0.75) + 0.848).abs() < 0.03);
		let triangle_waveform = BandLimitedWaveform::create(BeepWaveform::Triangle, 440.0, sample_rate);
		assert!((triangle_waveform.read_sample(0.25) - 1.0).abs() < 0.02);
	}

	//==============================================================================
	// 높은 배음은 나이퀴스트 아래에서 끊긴다. (크롬처럼 440 Hz 는 40 개와 32 개 사이, 880 Hz 는 20 개와 16 개 사이)
	//==============================================================================
	#[test]
	fn band_limited_waveform_culls_partials() {
		let sample_rate = BEEP_SAMPLE_RATE as f64;
		let waveform_440 = BandLimitedWaveform::create(BeepWaveform::Sawtooth, 440.0, sample_rate);
		assert_eq!(waveform_440.get_higher_partial_count(), 40);
		assert_eq!(waveform_440.lower_partial_count, 32);
		let waveform_880 = BandLimitedWaveform::create(BeepWaveform::Square, 880.0, sample_rate);
		assert_eq!(waveform_880.get_higher_partial_count(), 20);
		assert_eq!(waveform_880.lower_partial_count, 16);
		for frequency in [60.0, 220.0, 1000.0, 3000.0, 8000.0] {
			let band_limited_waveform = BandLimitedWaveform::create(BeepWaveform::Sawtooth, frequency, sample_rate);
			assert!(band_limited_waveform.get_higher_partial_count() as f64 * frequency < sample_rate * 0.5);
		}
	}

	//==============================================================================
	// 그린 샘플 수는 (길이 + 0.01초) x 샘플레이트다.
	//==============================================================================
	#[test]
	fn render_beep_frames_length() {
		let beep_envelope = BeepEnvelope::create(440.0, 0.09, BeepWaveform::Square, 0.06, 0.35, 0.7);
		let frame_list = render_beep_frames(&beep_envelope, 44100);
		// (0.09 + 0.01 의 부동소수 오차로 마지막 한 샘플은 올림에 따라 달라진다)
		let expected_frame_count = (0.1f64 * 44100.0) as i64;
		assert!((frame_list.len() as i64 - expected_frame_count).abs() <= 1);
		// 대역을 제한한 사각파는 깁스 너울 꼭대기가 1 근처다.
		let peak_sample = frame_list.iter().map(|frame| frame.left.abs()).fold(0.0f32, f32::max);
		assert!((peak_sample - 0.35 * 0.7).abs() < 0.35 * 0.7 * 0.05);
	}
}
