//==============================================================================
// 절차적 비프 재생기.
// - 원본 misc/beepplayer.js. 오디오 파일 없이 UI 효과음을 합성한다.
//   attack / sustain / release 엔벨로프로 클릭음이 나지 않게 한다.
// - click / confirm / cancel / error / success / warning / notification 프리셋과
//   임의 톤 시퀀스 재생(play_sequence)을 제공한다.
// - 사용:
//     let beep_player = BeepPlayer::new(engine.get_audio_manager(), BeepPlayer::DEFAULT_VOLUME);
//     beep_player.play_preset("confirm");
// - 소리를 내는 곳:
//   웹: 원본처럼 OscillatorNode + GainNode 를 만들어 오디오 컨텍스트의 출력에 바로 잇는다.
//   데스크탑: 같은 엔벨로프로 파형을 샘플로 그려 kira 뿌리 트랙(출력 자리)에 바로 튼다. (지연은 kira 의 지연 시작)
//     파형은 크롬 WebAudio 오실레이터처럼 대역을 제한한 푸리에 급수다. (square, sawtooth, triangle 의 높은 배음을 주파수에 따라 덜어 내고 크기를 맞춘다)
// - 원본과 다른 점:
//   파형 이름은 문자열 대신 BeepWaveform, 시퀀스의 톤은 BeepTone 이다. 프리셋 이름은 원본처럼 문자열이다.
//==============================================================================
use crate::core::audiomanager::{resume_audio_manager_state, AudioManager, AudioManagerState};
use crate::platform::audio as audio_backend;
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
// 파형. (원본 OscillatorNode.type 문자열)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeepWaveform {
	Sine,
	Square,
	Sawtooth,
	Triangle,
}

//==============================================================================
// 시퀀스의 톤 하나. (원본 { frequency, duration, waveform?, volume_scale? })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeepTone {
	// 주파수. (Hz)
	pub frequency: f32,
	// 길이. (초)
	pub duration: f64,
	pub waveform: BeepWaveform,
	// 이 톤만의 음량 배율.
	pub volume_scale: f32,
}

impl BeepTone {
	//==============================================================================
	// 생성. (사인파, 배율 1. 원본에서 waveform, volume_scale 을 비운 꼴)
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
	// 파형과 배율까지 정해 생성.
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
// 톤 하나의 엔벨로프. (원본 play_tone 이 계산하는 값들. 시간은 톤 시작 기준 초)
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

// 엔벨로프의 바닥 음량. (원본 0.0001, 지수 램프는 0 으로 갈 수 없다)
pub(crate) const BEEP_FLOOR_VOLUME: f32 = 0.0001;
// 오실레이터를 엔벨로프 끝보다 조금 더 돌리는 시간. (초, 원본 stop(end_time + 0.01))
pub(crate) const BEEP_STOP_MARGIN_SECONDS: f64 = 0.01;

impl BeepEnvelope {
	//==============================================================================
	// 생성. (원본 play_tone 의 attack, release, peak 계산)
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
	// 최고 음량을 유지하다가 줄어들기 시작하는 시각. (원본 max(start_time + attack, end_time - release))
	//==============================================================================
	pub(crate) fn get_release_start_seconds(&self) -> f64 {
		self.attack_seconds.max(self.duration_seconds - self.release_seconds)
	}
}

//==============================================================================
// 절차적 비프 재생기.
//==============================================================================
pub struct BeepPlayer {
	audio_manager_state: Rc<RefCell<AudioManagerState>>,
	volume: f32,
	is_muted: bool,
}

impl BeepPlayer {
	// 원본 생성자의 기본 음량.
	pub const DEFAULT_VOLUME: f32 = 0.35;

	//==============================================================================
	// 생성. (volume: 0 ~ 1. 원본처럼 매니저를 참조로 든다)
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
	// 단일 톤 재생.
	// - frequency: 주파수(Hz), duration_seconds: 길이(초), delay_seconds: 시작 지연(초), volume_scale: 이 톤만의 음량 배율.
	// - 컨텍스트가 없거나 돌고 있지 않으면 아무것도 하지 않는다.
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
	// 톤 시퀀스 재생. (차례로 이어 튼다)
	//==============================================================================
	pub fn play_sequence(&self, tone_list: &[BeepTone]) {
		let mut delay_seconds = 0.0;
		for tone in tone_list {
			self.play_tone(tone.frequency, tone.duration, tone.waveform, delay_seconds, tone.volume_scale);
			delay_seconds += tone.duration;
		}
	}

	//==============================================================================
	// 프리셋 재생. ("click" | "confirm" | "cancel" | "error" | "success" | "warning" | "notification", 모르는 이름은 click)
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
	// 기본 음량 설정. (0 ~ 1)
	//==============================================================================
	pub fn set_volume(&mut self, volume: f32) {
		self.volume = volume.clamp(0.0, 1.0);
	}

	//==============================================================================
	// 기본 음량 반환.
	//==============================================================================
	pub fn get_volume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	// 음소거 설정.
	//==============================================================================
	pub fn set_muted(&mut self, is_muted: bool) {
		self.is_muted = is_muted;
	}

	//==============================================================================
	// 음소거 여부.
	//==============================================================================
	pub fn is_muted(&self) -> bool {
		self.is_muted
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 엔벨로프 값은 원본 play_tone 의 계산과 같다.
	//==============================================================================
	#[test]
	fn envelope_matches_original_formula() {
		let click_envelope = BeepEnvelope::create(880.0, 0.045, BeepWaveform::Sine, 0.0, 0.35, 1.0);
		assert!((click_envelope.attack_seconds - 0.005).abs() < 1e-9);
		assert!((click_envelope.release_seconds - 0.02).abs() < 1e-9);
		assert!((click_envelope.peak_volume - 0.35).abs() < 1e-6);
		assert!((click_envelope.get_release_start_seconds() - 0.025).abs() < 1e-9);

		// 아주 짧은 톤은 attack 이 길이의 1/4, release 가 1/2 로 줄어든다.
		let short_envelope = BeepEnvelope::create(440.0, 0.01, BeepWaveform::Square, 0.2, 0.0, 1.0);
		assert!((short_envelope.attack_seconds - 0.0025).abs() < 1e-9);
		assert!((short_envelope.release_seconds - 0.005).abs() < 1e-9);
		assert_eq!(short_envelope.peak_volume, BEEP_FLOOR_VOLUME);
		assert!((short_envelope.delay_seconds - 0.2).abs() < 1e-9);
	}
}
