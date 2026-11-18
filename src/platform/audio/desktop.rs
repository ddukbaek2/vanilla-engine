//==============================================================================
// 데스크탑 오디오 백엔드. (윈도우, 맥, 리눅스, kira 0.12)
// - AudioContext 자리: kira::AudioManager. 만들 때 cpal 로 기본 출력 장치를 연다. 장치가 없으면 한 번 알리고 다시 열지 않는다.
//   브라우저 자동재생 정책도, suspended 상태도 없어서 user gesture 는 처음부터 있는 것으로 두고 resume 은 할 일이 없다.
// - AudioBuffer 자리: kira 의 StaticSoundData. 파일 전체를 디코딩해 들고, 복제는 Arc 하나라 값싸다.
// - AudioPlayer 하나 = 믹서 하위 트랙 하나. (원본 GainNode 자리, 트랙 음량이 플레이어 음량이다)
//   재생할 때마다 버퍼를 복제해 그 트랙에 튼다. 트랙 핸들을 버리면 트랙과 그 위의 소리가 같이 없어진다.
// - 음량(0 ~ 1)은 데시벨로 바꿔 건다. kira 는 -60dB 이하를 무음으로 친다.
// - 원본 WebAudio 처럼 음량 바꾸기와 정지는 바로 한다. (트윈 0초)
// - BeepPlayer 의 톤은 엔벨로프를 곱한 파형을 샘플로 그려 메인 트랙(원본 audioContext.destination 자리)에 튼다.
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
	isAudioContextFailed: bool,
}

//==============================================================================
// 백엔드 상태 생성.
//==============================================================================
pub fn createAudioPlatformState() -> AudioPlatformState {
	AudioPlatformState {
		isAudioContextFailed: false,
	}
}

//==============================================================================
// 오디오 컨텍스트 생성. (kira 매니저와 기본 출력 장치)
//==============================================================================
pub fn createAudioContext(audioPlatformState: &mut AudioPlatformState) -> Option<AudioContext> {
	if audioPlatformState.isAudioContextFailed {
		return None;
	}
	let audioManagerSettings = AudioManagerSettings::<DefaultBackend> {
		capacities: Capacities {
			sub_track_capacity: SUB_TRACK_CAPACITY,
			..Capacities::default()
		},
		..AudioManagerSettings::default()
	};
	match AudioContext::new(audioManagerSettings) {
		Ok(audioContext) => {
			Some(audioContext)
		}
		Err(error) => {
			eprintln!("AudioManager: 소리 장치를 열지 못했습니다. 소리 없이 계속합니다. ({})", error);
			audioPlatformState.isAudioContextFailed = true;
			None
		}
	}
}

//==============================================================================
// 오디오 컨텍스트 재개. (kira 에는 멈춤 상태가 없어 할 일이 없다)
//==============================================================================
pub fn resumeAudioContext(_stateCell: &Rc<RefCell<AudioManagerState>>) {
}

//==============================================================================
// 오디오 컨텍스트가 돌고 있는지. (kira 매니저는 만든 뒤 늘 돈다)
//==============================================================================
pub fn isAudioContextRunning(_audioContext: &AudioContext) -> bool {
	true
}

//==============================================================================
// 바로 끝나는 트윈. (원본 gain.value 대입, source.stop() 처럼 바로 바꾼다)
//==============================================================================
fn createImmediateTween() -> Tween {
	Tween {
		duration: Duration::ZERO,
		..Tween::default()
	}
}

//==============================================================================
// 선형 음량(0 ~ 1)을 데시벨로. (0 이하는 무음)
//==============================================================================
pub(crate) fn amplitudeToDecibels(amplitude: f32) -> Decibels {
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
	trackHandle: Option<TrackHandle>,
	soundHandle: Option<StaticSoundHandle>,
}

impl AudioPlayerBackend {
	//==============================================================================
	// 생성. (컨텍스트가 있으면 하위 트랙을 하나 만든다. 없거나 트랙이 모자라면 소리 없는 플레이어)
	//==============================================================================
	pub fn create(audioContext: Option<&mut AudioContext>) -> AudioPlayerBackend {
		let trackHandle = match audioContext {
			Some(audioContext) => {
				let trackBuilder = TrackBuilder::new().sound_capacity(PLAYER_SOUND_CAPACITY);
				match audioContext.add_sub_track(trackBuilder) {
					Ok(trackHandle) => {
						Some(trackHandle)
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
			trackHandle,
			soundHandle: None,
		}
	}

	//==============================================================================
	// 소리를 낼 수 있는지. (원본 audioContext && gainNode)
	//==============================================================================
	pub fn isAvailable(&self) -> bool {
		self.trackHandle.is_some()
	}

	//==============================================================================
	// 재생 중인 소리를 멈춘다.
	//==============================================================================
	pub fn stopSource(&mut self) {
		if let Some(mut soundHandle) = self.soundHandle.take() {
			soundHandle.stop(createImmediateTween());
		}
	}

	//==============================================================================
	// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		self.stopSource();
	}

	//==============================================================================
	// 재생 시작. (버퍼를 복제해 time 초부터, 루프면 처음부터 끝까지 되풀이)
	//==============================================================================
	pub fn startPlayback(&mut self, audioBuffer: &AudioBuffer, isLoop: bool, time: f64) {
		let trackHandle = match self.trackHandle.as_mut() {
			Some(trackHandle) => {
				trackHandle
			}
			None => {
				return;
			}
		};
		let loopRegion: Option<Region> = if isLoop {
			Some(Region::from(..))
		}
		else {
			None
		};
		let soundData = audioBuffer.start_position(time).loop_region(loopRegion);
		match trackHandle.play(soundData) {
			Ok(soundHandle) => {
				self.soundHandle = Some(soundHandle);
			}
			Err(error) => {
				eprintln!("AudioPlayer: 재생하지 못했습니다. ({})", error);
			}
		}
	}

	//==============================================================================
	// 음량 설정. (트랙 음량, 0 ~ 1)
	//==============================================================================
	pub fn setGain(&mut self, gain: f32) {
		if let Some(trackHandle) = self.trackHandle.as_mut() {
			let decibels = amplitudeToDecibels(gain);
			trackHandle.set_volume(decibels, createImmediateTween());
		}
	}

	//==============================================================================
	// 재생 중인지. (소리 핸들이 있고 아직 멈추지 않았으면 참)
	//==============================================================================
	pub fn isPlaying(&self) -> bool {
		match &self.soundHandle {
			Some(soundHandle) => {
				soundHandle.state() != PlaybackState::Stopped
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
pub fn loadAudioFile(audioAsset: &Rc<AudioAsset>, assetPath: &Path) -> Result<(), String> {
	match StaticSoundData::from_file(assetPath) {
		Ok(audioBuffer) => {
			audioAsset.setAudioBuffer(audioBuffer);
			Ok(())
		}
		Err(error) => {
			let loadError = format!("Error loading sound: {} ({})", assetPath.display(), error);
			audioAsset.setLoadError(loadError.clone());
			Err(loadError)
		}
	}
}

//==============================================================================
// 메모리에서 읽기. (그 자리에서 디코딩한다)
//==============================================================================
pub fn loadAudioBytes(audioAsset: &Rc<AudioAsset>, bytes: Vec<u8>) -> Result<(), String> {
	match StaticSoundData::from_cursor(Cursor::new(bytes)) {
		Ok(audioBuffer) => {
			audioAsset.setAudioBuffer(audioBuffer);
			Ok(())
		}
		Err(error) => {
			let loadError = format!("Error loading sound from memory ({})", error);
			audioAsset.setLoadError(loadError.clone());
			Err(loadError)
		}
	}
}

//==============================================================================
// 버퍼의 재생 시간. (초)
//==============================================================================
pub fn getAudioBufferDuration(audioBuffer: &AudioBuffer) -> f64 {
	audioBuffer.duration().as_secs_f64()
}

//==============================================================================
// 비프 톤 하나 재생. (그린 샘플을 메인 트랙에, 지연이 있으면 kira 의 지연 시작으로)
//==============================================================================
pub fn playBeepTone(audioContext: &mut AudioContext, beepEnvelope: &BeepEnvelope) {
	let frameList = renderBeepFrames(beepEnvelope, BEEP_SAMPLE_RATE);
	let startTime = if beepEnvelope.delaySeconds > 0.0 {
		StartTime::Delayed(Duration::from_secs_f64(beepEnvelope.delaySeconds))
	}
	else {
		StartTime::Immediate
	};
	let beepSoundData = StaticSoundData {
		sample_rate: BEEP_SAMPLE_RATE,
		frames: Arc::from(frameList),
		settings: StaticSoundSettings::new().start_time(startTime),
		slice: None,
	};
	if let Err(error) = audioContext.play(beepSoundData) {
		eprintln!("BeepPlayer: 재생하지 못했습니다. ({})", error);
	}
}

//==============================================================================
// 비프 톤을 샘플로 그린다. (원본 오실레이터가 도는 길이 = 엔벨로프 길이 + 0.01초)
//==============================================================================
fn renderBeepFrames(beepEnvelope: &BeepEnvelope, sampleRate: u32) -> Vec<Frame> {
	let sampleRateValue = sampleRate as f64;
	let totalSeconds = beepEnvelope.durationSeconds + BEEP_STOP_MARGIN_SECONDS;
	let frameCount = (totalSeconds * sampleRateValue).ceil() as usize;
	let phaseStep = beepEnvelope.frequency as f64 / sampleRateValue;
	let mut phase = 0.0f64;
	let mut frameList = Vec::with_capacity(frameCount);
	for frameIndex in 0..frameCount {
		let elapsedSeconds = frameIndex as f64 / sampleRateValue;
		let gain = getBeepEnvelopeGain(beepEnvelope, elapsedSeconds);
		let sample = getWaveformSample(beepEnvelope.waveform, phase) * gain;
		frameList.push(Frame::from_mono(sample));
		phase += phaseStep;
		phase -= phase.floor();
	}
	frameList
}

//==============================================================================
// 엔벨로프 음량. (원본 GainNode 자동화와 같은 값, elapsedSeconds 는 톤 시작 기준)
// - 0 ~ attack: 바닥에서 최고까지 선형, attack ~ release 시작: 최고 유지, release 시작 ~ 끝: 바닥까지 지수, 그 뒤 바닥.
//==============================================================================
fn getBeepEnvelopeGain(beepEnvelope: &BeepEnvelope, elapsedSeconds: f64) -> f32 {
	let floorVolume = BEEP_FLOOR_VOLUME as f64;
	let peakVolume = beepEnvelope.peakVolume as f64;
	let releaseStartSeconds = beepEnvelope.getReleaseStartSeconds();
	if elapsedSeconds < beepEnvelope.attackSeconds {
		let progress = elapsedSeconds / beepEnvelope.attackSeconds;
		return (floorVolume + (peakVolume - floorVolume) * progress) as f32;
	}
	if elapsedSeconds < releaseStartSeconds {
		return peakVolume as f32;
	}
	if elapsedSeconds < beepEnvelope.durationSeconds {
		let progress = (elapsedSeconds - releaseStartSeconds) / (beepEnvelope.durationSeconds - releaseStartSeconds);
		return (peakVolume * (floorVolume / peakVolume).powf(progress)) as f32;
	}
	floorVolume as f32
}

//==============================================================================
// 파형 한 점. (phase: 0 ~ 1. WebAudio 오실레이터처럼 0 에서 시작해 위로 올라가는 꼴, 대역 제한은 없다)
//==============================================================================
fn getWaveformSample(beepWaveform: BeepWaveform, phase: f64) -> f32 {
	let sample = match beepWaveform {
		BeepWaveform::sine => {
			(phase * std::f64::consts::TAU).sin()
		}
		BeepWaveform::square => {
			if phase < 0.5 {
				1.0
			}
			else {
				-1.0
			}
		}
		BeepWaveform::sawtooth => {
			2.0 * (phase - (phase + 0.5).floor())
		}
		BeepWaveform::triangle => {
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
	fn amplitudeToDecibelsMapsVolume() {
		assert_eq!(amplitudeToDecibels(1.0), Decibels::IDENTITY);
		assert!((amplitudeToDecibels(0.5).0 - (-6.0206)).abs() < 1e-3);
		assert_eq!(amplitudeToDecibels(0.0), Decibels::SILENCE);
		assert_eq!(amplitudeToDecibels(0.00001), Decibels::SILENCE);
		assert!((amplitudeToDecibels(0.5).as_amplitude() - 0.5).abs() < 1e-5);
	}

	//==============================================================================
	// 엔벨로프는 바닥에서 올라 최고를 지키다 바닥으로 내려간다.
	//==============================================================================
	#[test]
	fn beepEnvelopeGainFollowsAutomation() {
		let beepEnvelope = BeepEnvelope::create(880.0, 0.045, BeepWaveform::sine, 0.0, 0.35, 1.0);
		assert!((getBeepEnvelopeGain(&beepEnvelope, 0.0) - BEEP_FLOOR_VOLUME).abs() < 1e-7);
		assert!((getBeepEnvelopeGain(&beepEnvelope, 0.0025) - (0.0001 + 0.3499 * 0.5)).abs() < 1e-5);
		assert!((getBeepEnvelopeGain(&beepEnvelope, 0.01) - 0.35).abs() < 1e-6);
		assert!((getBeepEnvelopeGain(&beepEnvelope, 0.024) - 0.35).abs() < 1e-6);
		let middleGain = getBeepEnvelopeGain(&beepEnvelope, 0.035);
		assert!((middleGain as f64 - (0.35f64 * 0.0001).sqrt()).abs() < 1e-5);
		assert!((getBeepEnvelopeGain(&beepEnvelope, 0.05) - BEEP_FLOOR_VOLUME).abs() < 1e-7);
	}

	//==============================================================================
	// 파형은 0 에서 시작해 위로 올라간다.
	//==============================================================================
	#[test]
	fn waveformSamplesStartAtZeroGoingUp() {
		for beepWaveform in [BeepWaveform::sine, BeepWaveform::sawtooth, BeepWaveform::triangle] {
			assert!(getWaveformSample(beepWaveform, 0.0).abs() < 1e-6);
			assert!(getWaveformSample(beepWaveform, 0.1) > 0.0);
		}
		assert_eq!(getWaveformSample(BeepWaveform::square, 0.1), 1.0);
		assert_eq!(getWaveformSample(BeepWaveform::square, 0.6), -1.0);
		assert!((getWaveformSample(BeepWaveform::triangle, 0.25) - 1.0).abs() < 1e-6);
		assert!((getWaveformSample(BeepWaveform::triangle, 0.75) + 1.0).abs() < 1e-6);
		assert!((getWaveformSample(BeepWaveform::sawtooth, 0.75) + 0.5).abs() < 1e-6);
	}

	//==============================================================================
	// 그린 샘플 수는 (길이 + 0.01초) x 샘플레이트다.
	//==============================================================================
	#[test]
	fn renderBeepFramesLength() {
		let beepEnvelope = BeepEnvelope::create(440.0, 0.09, BeepWaveform::square, 0.06, 0.35, 0.7);
		let frameList = renderBeepFrames(&beepEnvelope, 44100);
		// (0.09 + 0.01 의 부동소수 오차로 마지막 한 샘플은 올림에 따라 달라진다)
		let expectedFrameCount = (0.1f64 * 44100.0) as i64;
		assert!((frameList.len() as i64 - expectedFrameCount).abs() <= 1);
		let peakSample = frameList.iter().map(|frame| frame.left.abs()).fold(0.0f32, f32::max);
		assert!((peakSample - 0.35 * 0.7).abs() < 1e-5);
	}
}
