//==============================================================================
// 절차적 비프 재생기.
// - 원본 misc/beepplayer.js. 오디오 파일 없이 UI 효과음을 합성한다.
//   attack / sustain / release 엔벨로프로 클릭음이 나지 않게 한다.
// - click / confirm / cancel / error / success / warning / notification 프리셋과
//   임의 톤 시퀀스 재생(playSequence)을 제공한다.
// - 사용:
//     let beepPlayer = BeepPlayer::new(engine.getAudioManager(), BeepPlayer::DEFAULT_VOLUME);
//     beepPlayer.playPreset("confirm");
// - 소리를 내는 곳:
//   웹: 원본처럼 OscillatorNode + GainNode 를 만들어 오디오 컨텍스트의 출력에 바로 잇는다.
//   데스크탑: 같은 엔벨로프로 파형을 샘플로 그려 kira 메인 트랙에 바로 튼다. (지연은 kira 의 지연 시작)
// - 원본과 다른 점:
//   파형 이름은 문자열 대신 BeepWaveform, 시퀀스의 톤은 BeepTone 이다. 프리셋 이름은 원본처럼 문자열이다.
//   데스크탑의 파형은 대역 제한 없이 그린 것이라 square, sawtooth 의 높은 배음이 WebAudio 와 조금 다르다.
//==============================================================================
use crate::core::audiomanager::{resumeAudioManagerState, AudioManager, AudioManagerState};
use crate::platform::audio as audioBackend;
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
// 파형. (원본 OscillatorNode.type 문자열)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeepWaveform {
	sine,
	square,
	sawtooth,
	triangle,
}

//==============================================================================
// 시퀀스의 톤 하나. (원본 { frequency, duration, waveform?, volumeScale? })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeepTone {
	// 주파수. (Hz)
	pub frequency: f32,
	// 길이. (초)
	pub duration: f64,
	pub waveform: BeepWaveform,
	// 이 톤만의 음량 배율.
	pub volumeScale: f32,
}

impl BeepTone {
	//==============================================================================
	// 생성. (사인파, 배율 1. 원본에서 waveform, volumeScale 을 비운 꼴)
	//==============================================================================
	pub const fn create(frequency: f32, duration: f64) -> BeepTone {
		BeepTone {
			frequency,
			duration,
			waveform: BeepWaveform::sine,
			volumeScale: 1.0,
		}
	}

	//==============================================================================
	// 파형과 배율까지 정해 생성.
	//==============================================================================
	pub const fn createWithWaveform(frequency: f32, duration: f64, waveform: BeepWaveform, volumeScale: f32) -> BeepTone {
		BeepTone {
			frequency,
			duration,
			waveform,
			volumeScale,
		}
	}
}

//==============================================================================
// 톤 하나의 엔벨로프. (원본 playTone 이 계산하는 값들. 시간은 톤 시작 기준 초)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BeepEnvelope {
	pub(crate) frequency: f32,
	pub(crate) waveform: BeepWaveform,
	pub(crate) delaySeconds: f64,
	pub(crate) durationSeconds: f64,
	pub(crate) attackSeconds: f64,
	pub(crate) releaseSeconds: f64,
	pub(crate) peakVolume: f32,
}

// 엔벨로프의 바닥 음량. (원본 0.0001, 지수 램프는 0 으로 갈 수 없다)
pub(crate) const BEEP_FLOOR_VOLUME: f32 = 0.0001;
// 오실레이터를 엔벨로프 끝보다 조금 더 돌리는 시간. (초, 원본 stop(endTime + 0.01))
pub(crate) const BEEP_STOP_MARGIN_SECONDS: f64 = 0.01;

impl BeepEnvelope {
	//==============================================================================
	// 생성. (원본 playTone 의 attack, release, peak 계산)
	//==============================================================================
	pub(crate) fn create(frequency: f32, durationSeconds: f64, waveform: BeepWaveform, delaySeconds: f64, volume: f32, volumeScale: f32) -> BeepEnvelope {
		BeepEnvelope {
			frequency,
			waveform,
			delaySeconds,
			durationSeconds,
			attackSeconds: 0.005f64.min(durationSeconds * 0.25),
			releaseSeconds: 0.02f64.min(durationSeconds * 0.5),
			peakVolume: BEEP_FLOOR_VOLUME.max(volume * volumeScale),
		}
	}

	//==============================================================================
	// 최고 음량을 유지하다가 줄어들기 시작하는 시각. (원본 max(startTime + attack, endTime - release))
	//==============================================================================
	pub(crate) fn getReleaseStartSeconds(&self) -> f64 {
		self.attackSeconds.max(self.durationSeconds - self.releaseSeconds)
	}
}

//==============================================================================
// 절차적 비프 재생기.
//==============================================================================
pub struct BeepPlayer {
	audioManagerState: Rc<RefCell<AudioManagerState>>,
	volume: f32,
	isMuted: bool,
}

impl BeepPlayer {
	// 원본 생성자의 기본 음량.
	pub const DEFAULT_VOLUME: f32 = 0.35;

	//==============================================================================
	// 생성. (volume: 0 ~ 1. 원본처럼 매니저를 참조로 든다)
	//==============================================================================
	pub fn new(audioManager: &AudioManager, volume: f32) -> BeepPlayer {
		let audioManagerState = audioManager.getSharedState();
		BeepPlayer {
			audioManagerState,
			volume,
			isMuted: false,
		}
	}

	//==============================================================================
	// 단일 톤 재생.
	// - frequency: 주파수(Hz), durationSeconds: 길이(초), delaySeconds: 시작 지연(초), volumeScale: 이 톤만의 음량 배율.
	// - 컨텍스트가 없거나 돌고 있지 않으면 아무것도 하지 않는다.
	//==============================================================================
	pub fn playTone(&self, frequency: f32, durationSeconds: f64, waveform: BeepWaveform, delaySeconds: f64, volumeScale: f32) {
		if self.isMuted {
			return;
		}
		resumeAudioManagerState(&self.audioManagerState);
		let mut audioManagerState = self.audioManagerState.borrow_mut();
		let audioContext = match audioManagerState.audioContext.as_mut() {
			Some(audioContext) => {
				audioContext
			}
			None => {
				return;
			}
		};
		if !audioBackend::isAudioContextRunning(audioContext) {
			return;
		}
		let beepEnvelope = BeepEnvelope::create(frequency, durationSeconds, waveform, delaySeconds, self.volume, volumeScale);
		audioBackend::playBeepTone(audioContext, &beepEnvelope);
	}

	//==============================================================================
	// 톤 시퀀스 재생. (차례로 이어 튼다)
	//==============================================================================
	pub fn playSequence(&self, toneList: &[BeepTone]) {
		let mut delaySeconds = 0.0;
		for tone in toneList {
			self.playTone(tone.frequency, tone.duration, tone.waveform, delaySeconds, tone.volumeScale);
			delaySeconds += tone.duration;
		}
	}

	//==============================================================================
	// 프리셋 재생. ("click" | "confirm" | "cancel" | "error" | "success" | "warning" | "notification", 모르는 이름은 click)
	//==============================================================================
	pub fn playPreset(&self, presetName: &str) {
		match presetName {
			"confirm" => {
				self.playSequence(&[BeepTone::create(660.0, 0.06), BeepTone::create(990.0, 0.09)]);
			}
			"cancel" => {
				self.playSequence(&[BeepTone::create(520.0, 0.06), BeepTone::create(360.0, 0.09)]);
			}
			"error" => {
				self.playSequence(&[
					BeepTone::createWithWaveform(220.0, 0.1, BeepWaveform::square, 0.7),
					BeepTone::createWithWaveform(180.0, 0.14, BeepWaveform::square, 0.7),
				]);
			}
			"success" => {
				self.playSequence(&[BeepTone::create(523.0, 0.07), BeepTone::create(659.0, 0.07), BeepTone::create(784.0, 0.12)]);
			}
			"warning" => {
				self.playSequence(&[
					BeepTone::createWithWaveform(740.0, 0.08, BeepWaveform::triangle, 1.0),
					BeepTone::createWithWaveform(740.0, 0.08, BeepWaveform::triangle, 1.0),
				]);
			}
			"notification" => {
				self.playSequence(&[BeepTone::create(1047.0, 0.06), BeepTone::create(1319.0, 0.1)]);
			}
			_ => {
				// "click" 과 모르는 이름.
				self.playTone(880.0, 0.045, BeepWaveform::sine, 0.0, 1.0);
			}
		}
	}

	//==============================================================================
	// 기본 음량 설정. (0 ~ 1)
	//==============================================================================
	pub fn setVolume(&mut self, volume: f32) {
		self.volume = volume.clamp(0.0, 1.0);
	}

	//==============================================================================
	// 기본 음량 반환.
	//==============================================================================
	pub fn getVolume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	// 음소거 설정.
	//==============================================================================
	pub fn setMuted(&mut self, isMuted: bool) {
		self.isMuted = isMuted;
	}

	//==============================================================================
	// 음소거 여부.
	//==============================================================================
	pub fn isMuted(&self) -> bool {
		self.isMuted
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 엔벨로프 값은 원본 playTone 의 계산과 같다.
	//==============================================================================
	#[test]
	fn envelopeMatchesOriginalFormula() {
		let clickEnvelope = BeepEnvelope::create(880.0, 0.045, BeepWaveform::sine, 0.0, 0.35, 1.0);
		assert!((clickEnvelope.attackSeconds - 0.005).abs() < 1e-9);
		assert!((clickEnvelope.releaseSeconds - 0.02).abs() < 1e-9);
		assert!((clickEnvelope.peakVolume - 0.35).abs() < 1e-6);
		assert!((clickEnvelope.getReleaseStartSeconds() - 0.025).abs() < 1e-9);

		// 아주 짧은 톤은 attack 이 길이의 1/4, release 가 1/2 로 줄어든다.
		let shortEnvelope = BeepEnvelope::create(440.0, 0.01, BeepWaveform::square, 0.2, 0.0, 1.0);
		assert!((shortEnvelope.attackSeconds - 0.0025).abs() < 1e-9);
		assert!((shortEnvelope.releaseSeconds - 0.005).abs() < 1e-9);
		assert_eq!(shortEnvelope.peakVolume, BEEP_FLOOR_VOLUME);
		assert!((shortEnvelope.delaySeconds - 0.2).abs() < 1e-9);
	}
}
