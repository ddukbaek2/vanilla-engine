//==============================================================================
//! 절차적 비프 재생기.
//! - 원본 misc/beepplayer.js. 오디오 파일 없이 UI 효과음을 합성한다.
//!   attack / sustain / release 엔벨로프로 클릭음이 나지 않게 한다.
//! - click / confirm / cancel / error / success / warning / notification 프리셋과
//!   임의 톤 시퀀스 재생(play_sequence)을 제공한다.
//! - 사용:
//!   ```text
//!     let beep_player = BeepPlayer::new(engine.get_audio_manager(), BeepPlayer::DEFAULT_VOLUME);
//!     beep_player.play_preset("confirm");
//!   ```
//! - 소리를 내는 곳:
//!   웹: 원본처럼 OscillatorNode + GainNode 를 만들어 오디오 컨텍스트의 출력에 바로 잇는다.
//!   데스크탑: 믹서에 같은 그래프(크롬과 같은 셈의 OscillatorNode → GainNode AudioParam 자동화 → 출력)를 짓는다. (platform/audio/oscillator.rs, mixer.rs)
//! - 원본과 다른 점:
//!   파형 이름은 문자열 대신 BeepWaveform, 시퀀스의 톤은 BeepTone 이다. 프리셋 이름은 원본처럼 문자열이다.
//!
//! # 예제
//! 엔진의 오디오 매니저로 확인음 내기
//! ```rust,no_run
//! use vanilla::{BeepPlayer, Engine};
//!
//! fn play_confirm(engine: &Engine) {
//! 	let beep_player = BeepPlayer::new(engine.get_audio_manager(), BeepPlayer::DEFAULT_VOLUME);
//! 	beep_player.play_preset("confirm");
//! }
//! ```
//==============================================================================
use crate::core::audiomanager::{resume_audio_manager_state, AudioManager, AudioManagerState};
use crate::platform::audio as audio_backend;
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
/// 파형. (원본 OscillatorNode.type 문자열)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeepWaveform {
	/// 사인파. (원본 "sine")
	Sine,
	/// 사각파. (원본 "square")
	Square,
	/// 톱니파. (원본 "sawtooth")
	Sawtooth,
	/// 삼각파. (원본 "triangle")
	Triangle,
}

//==============================================================================
/// 시퀀스의 톤 하나. (원본 { frequency, duration, waveform?, volume_scale? })
///
/// # 예제
/// 기본 톤과 파형을 정한 톤
/// ```rust
/// use vanilla::{BeepTone, BeepWaveform};
///
/// let tone = BeepTone::create(440.0, 0.1);
/// assert_eq!(tone.waveform, BeepWaveform::Sine);
/// assert_eq!(tone.volume_scale, 1.0);
///
/// let buzz_tone = BeepTone::create_with_waveform(220.0, 0.14, BeepWaveform::Square, 0.7);
/// assert_eq!(buzz_tone.frequency, 220.0);
/// assert_eq!(buzz_tone.waveform, BeepWaveform::Square);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeepTone {
	/// 주파수. (Hz)
	pub frequency: f32,
	/// 길이. (초)
	pub duration: f64,
	/// 파형.
	pub waveform: BeepWaveform,
	/// 이 톤만의 음량 배율.
	pub volume_scale: f32,
}

impl BeepTone {
	//==============================================================================
	/// 생성. (사인파, 배율 1. 원본에서 waveform, volume_scale 을 비운 꼴)
	///
	/// # 인자
	/// - `frequency`: 주파수. (Hz)
	/// - `duration`: 길이. (초)
	///
	/// # 돌려줌
	/// 사인파, 음량 배율 1 인 톤.
	//==============================================================================
	pub const fn create(frequency: f32, duration: f64) -> BeepTone {
		BeepTone {
			frequency,
			duration,
			waveform: BeepWaveform::Sine,
			volume_scale: 1.0,
		}
	}

	//==============================================================================
	/// 파형과 배율까지 정해 생성.
	///
	/// # 인자
	/// - `frequency`: 주파수. (Hz)
	/// - `duration`: 길이. (초)
	/// - `waveform`: 파형.
	/// - `volume_scale`: 이 톤만의 음량 배율. (재생기 음량에 곱한다)
	///
	/// # 돌려줌
	/// 받은 값을 그대로 든 톤.
	//==============================================================================
	pub const fn create_with_waveform(frequency: f32, duration: f64, waveform: BeepWaveform, volume_scale: f32) -> BeepTone {
		BeepTone {
			frequency,
			duration,
			waveform,
			volume_scale,
		}
	}
}

//==============================================================================
/// 톤 하나의 엔벨로프. (원본 play_tone 이 계산하는 값들. 시간은 톤 시작 기준 초)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BeepEnvelope {
	pub(crate) frequency: f32,
	pub(crate) waveform: BeepWaveform,
	pub(crate) delay_seconds: f64,
	pub(crate) duration_seconds: f64,
	pub(crate) attack_seconds: f64,
	pub(crate) release_seconds: f64,
	pub(crate) peak_volume: f32,
}

/// 엔벨로프의 바닥 음량. (원본 0.0001, 지수 램프는 0 으로 갈 수 없다)
pub(crate) const BEEP_FLOOR_VOLUME: f32 = 0.0001;
/// 오실레이터를 엔벨로프 끝보다 조금 더 돌리는 시간. (초, 원본 stop(end_time + 0.01))
pub(crate) const BEEP_STOP_MARGIN_SECONDS: f64 = 0.01;

impl BeepEnvelope {
	//==============================================================================
	/// 생성. (원본 play_tone 의 attack, release, peak 계산)
	//==============================================================================
	pub(crate) fn create(frequency: f32, duration_seconds: f64, waveform: BeepWaveform, delay_seconds: f64, volume: f32, volume_scale: f32) -> BeepEnvelope {
		BeepEnvelope {
			frequency,
			waveform,
			delay_seconds,
			duration_seconds,
			attack_seconds: 0.005f64.min(duration_seconds * 0.25),
			release_seconds: 0.02f64.min(duration_seconds * 0.5),
			peak_volume: BEEP_FLOOR_VOLUME.max(volume * volume_scale),
		}
	}

	//==============================================================================
	/// 컨텍스트 시계로 본 자동화 시각들. (current_time: 부른 때의 currentTime. 원본 play_tone 과 같은 차례로 더한다)
	//==============================================================================
	pub(crate) fn get_schedule(&self, current_time: f64) -> BeepSchedule {
		let start_time = current_time + self.delay_seconds;
		let end_time = start_time + self.duration_seconds;
		BeepSchedule {
			start_time,
			attack_end_time: start_time + self.attack_seconds,
			release_start_time: (start_time + self.attack_seconds).max(end_time - self.release_seconds),
			end_time,
			stop_time: end_time + BEEP_STOP_MARGIN_SECONDS,
		}
	}
}

//==============================================================================
/// 톤 하나의 자동화 시각. (컨텍스트 시계, 초)
/// - 오실레이터 start(start_time), stop(stop_time) 과 GainNode 의 setValueAtTime(바닥, start_time), linearRamp(최고, attack_end_time),
///   setValueAtTime(최고, release_start_time), exponentialRamp(바닥, end_time).
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BeepSchedule {
	pub(crate) start_time: f64,
	pub(crate) attack_end_time: f64,
	/// 최고 음량을 유지하다가 줄어들기 시작하는 시각. (원본 max(start_time + attack, end_time - release))
	pub(crate) release_start_time: f64,
	pub(crate) end_time: f64,
	/// 오실레이터를 멈추는 시각. (원본 end_time + 0.01)
	pub(crate) stop_time: f64,
}

//==============================================================================
/// 절차적 비프 재생기.
///
/// # 예제
/// 누를 때마다 프리셋 효과음 내기
/// ```rust,no_run
/// use vanilla::{BeepPlayer, Engine, EngineConfiguration, Scene, SceneBase, Vector2};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// 	beep_player: Option<BeepPlayer>,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn create(&mut self, engine: &mut Engine) {
/// 		self.beep_player = Some(BeepPlayer::new(engine.get_audio_manager(), 0.3));
/// 	}
///
/// 	fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
/// 		if let Some(beep_player) = &self.beep_player {
/// 			beep_player.play_preset("confirm");
/// 		}
/// 	}
/// }
///
/// let engine_configuration = EngineConfiguration::new();
/// let engine = Engine::new(engine_configuration);
/// let main_scene = MainScene {
/// 	scene_base: SceneBase::new(),
/// 	beep_player: None,
/// };
/// engine.run(Box::new(main_scene)).unwrap();
/// ```
///
/// 음량과 음소거 (음소거 중에는 소리 장치를 건드리지 않는다)
/// ```rust
/// use vanilla::{AudioManager, BeepPlayer};
///
/// let audio_manager = AudioManager::new();
/// let mut beep_player = BeepPlayer::new(&audio_manager, BeepPlayer::DEFAULT_VOLUME);
/// beep_player.set_volume(1.5);
/// assert_eq!(beep_player.get_volume(), 1.0);
///
/// beep_player.set_muted(true);
/// assert!(beep_player.is_muted());
/// beep_player.play_preset("success");
/// ```
//==============================================================================
pub struct BeepPlayer {
	audio_manager_state: Rc<RefCell<AudioManagerState>>,
	volume: f32,
	is_muted: bool,
}

impl BeepPlayer {
	/// 원본 생성자의 기본 음량.
	pub const DEFAULT_VOLUME: f32 = 0.35;

	//==============================================================================
	/// 생성. (volume: 0 ~ 1. 원본처럼 매니저를 참조로 든다)
	///
	/// # 인자
	/// - `audio_manager`: 소리를 낼 오디오 매니저. (engine.get_audio_manager(), 상태를 같이 든다)
	/// - `volume`: 기본 음량. (0 ~ 1, 여기서는 자르지 않고 그대로 둔다, 원본 기본값 DEFAULT_VOLUME)
	///
	/// # 돌려줌
	/// 음소거가 꺼진 새 재생기.
	//==============================================================================
	pub fn new(audio_manager: &AudioManager, volume: f32) -> BeepPlayer {
		let audio_manager_state = audio_manager.get_shared_state();
		BeepPlayer {
			audio_manager_state,
			volume,
			is_muted: false,
		}
	}

	//==============================================================================
	/// 단일 톤 재생.
	/// - frequency: 주파수(Hz), duration_seconds: 길이(초), delay_seconds: 시작 지연(초), volume_scale: 이 톤만의 음량 배율.
	/// - 컨텍스트가 없거나 돌고 있지 않으면 아무것도 하지 않는다.
	/// - 음소거 중이면 컨텍스트를 깨우지도 않고 돌아간다. 아니면 먼저 컨텍스트를 재개한다. (데스크탑은 소리 장치를 이때 연다)
	///
	/// # 인자
	/// - `frequency`: 주파수. (Hz)
	/// - `duration_seconds`: 길이. (초)
	/// - `waveform`: 파형.
	/// - `delay_seconds`: 시작 지연. (초, 지금부터)
	/// - `volume_scale`: 이 톤만의 음량 배율. (기본 음량에 곱한다, 곱이 0.0001 보다 작으면 0.0001)
	///
	/// # 예제
	/// 0.2 초 뒤에 낮은 사각파 한 번
	/// ```rust,no_run
	/// use vanilla::{BeepPlayer, BeepWaveform, Engine};
	///
	/// fn play_low_buzz(engine: &Engine) {
	/// 	let beep_player = BeepPlayer::new(engine.get_audio_manager(), BeepPlayer::DEFAULT_VOLUME);
	/// 	beep_player.play_tone(220.0, 0.1, BeepWaveform::Square, 0.2, 0.7);
	/// }
	/// ```
	//==============================================================================
	pub fn play_tone(&self, frequency: f32, duration_seconds: f64, waveform: BeepWaveform, delay_seconds: f64, volume_scale: f32) {
		if self.is_muted {
			return;
		}
		resume_audio_manager_state(&self.audio_manager_state);
		let mut audio_manager_state = self.audio_manager_state.borrow_mut();
		if !audio_manager_state.is_context_running() {
			return;
		}
		let beep_envelope = BeepEnvelope::create(frequency, duration_seconds, waveform, delay_seconds, self.volume, volume_scale);
		audio_backend::play_beep_tone(&mut audio_manager_state, &beep_envelope);
	}

	//==============================================================================
	/// 톤 시퀀스 재생. (차례로 이어 튼다)
	/// - 톤마다 앞 톤들의 길이 합만큼 늦춰 시작한다. (간격 없음)
	///
	/// # 인자
	/// - `tone_list`: 차례로 이어 틀 톤 목록. (길이는 초)
	///
	/// # 예제
	/// 도미솔 세 음을 이어 틀기
	/// ```rust,no_run
	/// use vanilla::{BeepPlayer, BeepTone, Engine};
	///
	/// fn play_chord_arpeggio(engine: &Engine) {
	/// 	let beep_player = BeepPlayer::new(engine.get_audio_manager(), BeepPlayer::DEFAULT_VOLUME);
	/// 	beep_player.play_sequence(&[BeepTone::create(523.0, 0.1), BeepTone::create(659.0, 0.1), BeepTone::create(784.0, 0.2)]);
	/// }
	/// ```
	//==============================================================================
	pub fn play_sequence(&self, tone_list: &[BeepTone]) {
		let mut delay_seconds = 0.0;
		for tone in tone_list {
			self.play_tone(tone.frequency, tone.duration, tone.waveform, delay_seconds, tone.volume_scale);
			delay_seconds += tone.duration;
		}
	}

	//==============================================================================
	/// 프리셋 재생. ("click" | "confirm" | "cancel" | "error" | "success" | "warning" | "notification", 모르는 이름은 click)
	///
	/// # 인자
	/// - `preset_name`: 프리셋 이름. (대소문자를 가린다)
	//==============================================================================
	pub fn play_preset(&self, preset_name: &str) {
		match preset_name {
			"confirm" => {
				self.play_sequence(&[BeepTone::create(660.0, 0.06), BeepTone::create(990.0, 0.09)]);
			}
			"cancel" => {
				self.play_sequence(&[BeepTone::create(520.0, 0.06), BeepTone::create(360.0, 0.09)]);
			}
			"error" => {
				self.play_sequence(&[
					BeepTone::create_with_waveform(220.0, 0.1, BeepWaveform::Square, 0.7),
					BeepTone::create_with_waveform(180.0, 0.14, BeepWaveform::Square, 0.7),
				]);
			}
			"success" => {
				self.play_sequence(&[BeepTone::create(523.0, 0.07), BeepTone::create(659.0, 0.07), BeepTone::create(784.0, 0.12)]);
			}
			"warning" => {
				self.play_sequence(&[
					BeepTone::create_with_waveform(740.0, 0.08, BeepWaveform::Triangle, 1.0),
					BeepTone::create_with_waveform(740.0, 0.08, BeepWaveform::Triangle, 1.0),
				]);
			}
			"notification" => {
				self.play_sequence(&[BeepTone::create(1047.0, 0.06), BeepTone::create(1319.0, 0.1)]);
			}
			_ => {
				// "click" 과 모르는 이름.
				self.play_tone(880.0, 0.045, BeepWaveform::Sine, 0.0, 1.0);
			}
		}
	}

	//==============================================================================
	/// 기본 음량 설정. (0 ~ 1)
	///
	/// # 인자
	/// - `volume`: 기본 음량. (0 ~ 1 로 자른다)
	//==============================================================================
	pub fn set_volume(&mut self, volume: f32) {
		self.volume = volume.clamp(0.0, 1.0);
	}

	//==============================================================================
	/// 기본 음량 반환.
	///
	/// # 돌려줌
	/// 기본 음량. (set_volume 을 거쳤으면 0 ~ 1)
	//==============================================================================
	pub fn get_volume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	/// 음소거 설정.
	///
	/// # 인자
	/// - `is_muted`: 참이면 소리를 내지 않는다.
	//==============================================================================
	pub fn set_muted(&mut self, is_muted: bool) {
		self.is_muted = is_muted;
	}

	//==============================================================================
	/// 음소거 여부.
	///
	/// # 돌려줌
	/// 음소거 중이면 true.
	//==============================================================================
	pub fn is_muted(&self) -> bool {
		self.is_muted
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 엔벨로프 값은 원본 play_tone 의 계산과 같다.
	//==============================================================================
	#[test]
	fn envelope_matches_original_formula() {
		let click_envelope = BeepEnvelope::create(880.0, 0.045, BeepWaveform::Sine, 0.0, 0.35, 1.0);
		assert!((click_envelope.attack_seconds - 0.005).abs() < 1e-9);
		assert!((click_envelope.release_seconds - 0.02).abs() < 1e-9);
		assert!((click_envelope.peak_volume - 0.35).abs() < 1e-6);
		let click_schedule = click_envelope.get_schedule(1.5);
		assert_eq!(click_schedule.start_time, 1.5);
		assert_eq!(click_schedule.attack_end_time, 1.5 + 0.005);
		assert_eq!(click_schedule.release_start_time, (1.5 + 0.045) - 0.02);
		assert_eq!(click_schedule.end_time, 1.5 + 0.045);
		assert_eq!(click_schedule.stop_time, (1.5 + 0.045) + 0.01);

		// 아주 짧은 톤은 attack 이 길이의 1/4, release 가 1/2 로 줄어든다.
		let short_envelope = BeepEnvelope::create(440.0, 0.01, BeepWaveform::Square, 0.2, 0.0, 1.0);
		assert!((short_envelope.attack_seconds - 0.0025).abs() < 1e-9);
		assert!((short_envelope.release_seconds - 0.005).abs() < 1e-9);
		assert_eq!(short_envelope.peak_volume, BEEP_FLOOR_VOLUME);
		assert!((short_envelope.delay_seconds - 0.2).abs() < 1e-9);
		// attack 끝이 release 시작보다 늦으면 attack 끝에서 줄어들기 시작한다.
		let short_schedule = short_envelope.get_schedule(0.0);
		assert_eq!(short_schedule.release_start_time, short_schedule.attack_end_time.max(short_schedule.end_time - 0.005));
	}
}
