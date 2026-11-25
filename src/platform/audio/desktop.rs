//==============================================================================
// 데스크탑 오디오 백엔드. (윈도우, 맥, 리눅스, kira 0.12)
// - AudioContext 자리: kira::AudioManager. 만들 때 cpal 로 기본 출력 장치를 연다. 장치가 없으면 한 번 알리고 다시 열지 않는다.
//   브라우저 자동재생 정책도, suspended 상태도 없어서 user gesture 는 처음부터 있는 것으로 둔다.
// - 컨텍스트 멈춤(SoundSystem 이 음소거 동안 부른다): kira 매니저에는 멈춤이 없어 메인 트랙을 무음으로 두고, resume 이 되돌린다.
//   (원본 AudioContext.suspend 와 달리 소리의 시간은 그동안에도 흐른다)
// - 좌우(pan): 재생할 때 소리 설정의 panning 으로 건다. (-1 왼쪽 ~ 1 오른쪽, 원본 StereoPannerNode 자리, 다음 재생부터)
// - AudioBuffer 자리: kira 의 StaticSoundData. 파일 전체를 디코딩해 들고, 복제는 Arc 하나라 값싸다.
// - AudioPlayer 하나 = 믹서 하위 트랙 하나. (원본 GainNode 자리, 트랙 음량이 플레이어 음량이다)
//   재생할 때마다 버퍼를 복제해 그 트랙에 튼다. 트랙 핸들을 버리면 트랙과 그 위의 소리가 같이 없어진다.
// - 음량(0 ~ 1)은 데시벨로 바꿔 건다. kira 는 -60dB 이하를 무음으로 친다.
// - 원본 WebAudio 처럼 음량 바꾸기와 정지는 바로 한다. (트윈 0초)
// - 컨텍스트 시계(get_audio_context_time)는 컨텍스트를 만든 뒤 흐른 시간이다.
// - BeepPlayer 의 톤은 엔벨로프를 곱한 파형을 샘플로 그려 메인 트랙(원본 audio_context.destination 자리)에 튼다.
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

// 데스크탑에는 자동재생 정책이 없다.
pub const HAS_USER_GESTURE_AT_START: bool = true;
// 하위 트랙 최대 수. (플레이어 하나가 하나씩 쓴다. kira 기본은 128)
const SUB_TRACK_CAPACITY: usize = 1024;
// 플레이어 트랙 하나에 동시에 있을 수 있는 소리 수. (다시 틀 때 앞 소리가 아직 빠지지 않았을 때를 위해 여유를 둔다)
const PLAYER_SOUND_CAPACITY: usize = 8;
// 비프 톤을 그리는 샘플레이트.
const BEEP_SAMPLE_RATE: u32 = 44100;

//==============================================================================
// 매니저가 드는 백엔드 상태.
//==============================================================================
pub struct AudioPlatformState {
	// 소리 장치를 열지 못했는지. (한 번 실패하면 다시 열지 않는다)
	is_audio_context_failed: bool,
	// 컨텍스트를 멈춰 두었는지. (메인 트랙을 무음으로 둔 동안)
	is_audio_context_suspended: bool,
	// 컨텍스트를 만든 때. (컨텍스트 시계의 0 초)
	audio_context_start_instant: Option<Instant>,
}

//==============================================================================
// 백엔드 상태 생성.
//==============================================================================
pub fn create_audio_platform_state() -> AudioPlatformState {
	AudioPlatformState {
		is_audio_context_failed: false,
		is_audio_context_suspended: false,
		audio_context_start_instant: None,
	}
}

//==============================================================================
// 오디오 컨텍스트 생성. (kira 매니저와 기본 출력 장치)
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
	match AudioContext::new(audio_manager_settings) {
		Ok(audio_context) => {
			audio_platform_state.audio_context_start_instant = Some(Instant::now());
			Some(audio_context)
		}
		Err(error) => {
			eprintln!("AudioManager: 소리 장치를 열지 못했습니다. 소리 없이 계속합니다. ({})", error);
			audio_platform_state.is_audio_context_failed = true;
			None
		}
	}
}

//==============================================================================
// 오디오 컨텍스트 재개. (suspend_audio_context 로 멈춰 둔 것만 메인 트랙 음량을 되돌린다)
//==============================================================================
pub fn resume_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let state = &mut *state_cell.borrow_mut();
	if !state.platform_state.is_audio_context_suspended {
		return;
	}
	if let Some(audio_context) = state.audio_context.as_mut() {
		audio_context.main_track().set_volume(Decibels::IDENTITY, create_immediate_tween());
		state.platform_state.is_audio_context_suspended = false;
	}
}

//==============================================================================
// 오디오 컨텍스트 멈춤. (메인 트랙을 무음으로, 원본 audio_context.suspend() 자리)
//==============================================================================
pub fn suspend_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let state = &mut *state_cell.borrow_mut();
	if let Some(audio_context) = state.audio_context.as_mut() {
		audio_context.main_track().set_volume(Decibels::SILENCE, create_immediate_tween());
		state.platform_state.is_audio_context_suspended = true;
	}
}

//==============================================================================
// 오디오 컨텍스트가 돌고 있는지. (kira 매니저는 만든 뒤 늘 돈다)
//==============================================================================
pub fn is_audio_context_running(_audio_context: &AudioContext) -> bool {
	true
}

//==============================================================================
// 오디오 컨텍스트 시계. (초, 원본 audio_context.currentTime 자리, 컨텍스트를 만든 뒤 흐른 시간, 없으면 0)
// - kira 매니저는 멈춤이 없어 컨텍스트를 멈춰 둔 동안에도 소리와 함께 흐른다.
//==============================================================================
pub fn get_audio_context_time(state: &AudioManagerState) -> f64 {
	match state.platform_state.audio_context_start_instant {
		Some(audio_context_start_instant) => {
			audio_context_start_instant.elapsed().as_secs_f64()
		}
		None => {
			0.0
		}
	}
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
	// 생성. (컨텍스트가 있으면 하위 트랙을 하나 만든다. 없거나 트랙이 모자라면 소리 없는 플레이어)
	//==============================================================================
	pub fn create(audio_context: Option<&mut AudioContext>) -> AudioPlayerBackend {
		let track_handle = match audio_context {
			Some(audio_context) => {
				let track_builder = TrackBuilder::new().sound_capacity(PLAYER_SOUND_CAPACITY);
				match audio_context.add_sub_track(track_builder) {
					Ok(track_handle) => {
						Some(track_handle)
					}
					Err(error) => {
						eprintln!("AudioPlayer: 믹서 트랙을 만들지 못했습니다. ({})", error);
						None
					}
				}
			}
			None => {
				None
			}
		};
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
// 비프 톤 하나 재생. (그린 샘플을 메인 트랙에, 지연이 있으면 kira 의 지연 시작으로)
//==============================================================================
pub fn play_beep_tone(audio_context: &mut AudioContext, beep_envelope: &BeepEnvelope) {
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
	if let Err(error) = audio_context.play(beep_sound_data) {
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
	let mut phase = 0.0f64;
	let mut frame_list = Vec::with_capacity(frame_count);
	for frame_index in 0..frame_count {
		let elapsed_seconds = frame_index as f64 / sample_rate_value;
		let gain = get_beep_envelope_gain(beep_envelope, elapsed_seconds);
		let sample = get_waveform_sample(beep_envelope.waveform, phase) * gain;
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
// 파형 한 점. (phase: 0 ~ 1. WebAudio 오실레이터처럼 0 에서 시작해 위로 올라가는 꼴, 대역 제한은 없다)
//==============================================================================
fn get_waveform_sample(beep_waveform: BeepWaveform, phase: f64) -> f32 {
	let sample = match beep_waveform {
		BeepWaveform::Sine => {
			(phase * std::f64::consts::TAU).sin()
		}
		BeepWaveform::Square => {
			if phase < 0.5 {
				1.0
			}
			else {
				-1.0
			}
		}
		BeepWaveform::Sawtooth => {
			2.0 * (phase - (phase + 0.5).floor())
		}
		BeepWaveform::Triangle => {
			if phase < 0.25 {
				4.0 * phase
			}
			else if phase < 0.75 {
				2.0 - 4.0 * phase
			}
			else {
				4.0 * phase - 4.0
			}
		}
	};
	sample as f32
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
	// 파형은 0 에서 시작해 위로 올라간다.
	//==============================================================================
	#[test]
	fn waveform_samples_start_at_zero_going_up() {
		for beep_waveform in [BeepWaveform::Sine, BeepWaveform::Sawtooth, BeepWaveform::Triangle] {
			assert!(get_waveform_sample(beep_waveform, 0.0).abs() < 1e-6);
			assert!(get_waveform_sample(beep_waveform, 0.1) > 0.0);
		}
		assert_eq!(get_waveform_sample(BeepWaveform::Square, 0.1), 1.0);
		assert_eq!(get_waveform_sample(BeepWaveform::Square, 0.6), -1.0);
		assert!((get_waveform_sample(BeepWaveform::Triangle, 0.25) - 1.0).abs() < 1e-6);
		assert!((get_waveform_sample(BeepWaveform::Triangle, 0.75) + 1.0).abs() < 1e-6);
		assert!((get_waveform_sample(BeepWaveform::Sawtooth, 0.75) + 0.5).abs() < 1e-6);
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
		let peak_sample = frame_list.iter().map(|frame| frame.left.abs()).fold(0.0f32, f32::max);
		assert!((peak_sample - 0.35 * 0.7).abs() < 1e-5);
	}
}
