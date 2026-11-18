//==============================================================================
// 효과음 풀.
// - 원본 core/soundeffectpool.js. 같은 효과음이 연달아 겹쳐 나도 끊기지 않도록
//   여러 AudioPlayer 를 돌려 가며(라운드로빈) 재생한다.
// - 백엔드를 직접 부르지 않는다. AudioPlayer 위에서 차례와 음량만 센다.
// - 사용:
//     let mut soundEffectPool = SoundEffectPool::new(engine.getAudioManager(), 6);
//     soundEffectPool.play(&hitAudioAsset, 1.0);
//     soundEffectPool.play(&hitAudioAsset, 0.5);   // 이 재생만 절반 음량.
//     soundEffectPool.setVolume(0.8);              // 풀 전체의 음량. (재생 중인 것에도 곱해진다)
// - 원본과 다른 점:
//   play 의 음량은 기본값이 없어 늘 넘긴다. (원본 기본 1)
//   보이스 수가 0 이면 play 는 아무것도 하지 않는다. (원본은 0 으로 나눠 깨진다)
//==============================================================================
use crate::core::audiomanager::AudioManager;
use crate::core::audioplayer::{clampVolume, AudioPlayer};
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;

// 원본 생성자의 기본 보이스 수.
pub const DEFAULT_VOICE_COUNT: usize = 6;

//==============================================================================
// 효과음 풀.
//==============================================================================
pub struct SoundEffectPool {
	playerList: Vec<AudioPlayer>,
	// 플레이어마다 마지막 재생의 음량. (풀의 음량을 곱하기 전)
	playerVolumeList: Vec<f32>,
	nextIndex: usize,
	isMuted: bool,
	// 풀 전체의 음량. (0 ~ 1)
	volume: f32,
}

impl SoundEffectPool {
	//==============================================================================
	// 생성. (voiceCount: 동시에 겹칠 수 있는 최대 재생 수)
	//==============================================================================
	pub fn new(audioManager: &AudioManager, voiceCount: usize) -> SoundEffectPool {
		let mut playerList = Vec::with_capacity(voiceCount);
		for _ in 0..voiceCount {
			let audioPlayer = audioManager.createAudioPlayer();
			playerList.push(audioPlayer);
		}
		SoundEffectPool::createFromPlayerList(playerList)
	}

	//==============================================================================
	// 만들어 둔 플레이어 목록으로 생성.
	//==============================================================================
	fn createFromPlayerList(playerList: Vec<AudioPlayer>) -> SoundEffectPool {
		let playerVolumeList = vec![1.0; playerList.len()];
		SoundEffectPool {
			playerList,
			playerVolumeList,
			nextIndex: 0,
			isMuted: false,
			volume: 1.0,
		}
	}

	//==============================================================================
	// 효과음 재생.
	// - 다음 차례의 플레이어에 애셋을 실어 재생한다. 그 플레이어가 재생 중이었다면 끊고 새로 튼다.
	// - 음량은 이 재생의 음량(0 ~ 1)에 풀의 음량을 곱한 것이다.
	//==============================================================================
	pub fn play(&mut self, audioAsset: &Rc<AudioAsset>, volume: f32) {
		if self.isMuted || self.playerList.is_empty() {
			return;
		}
		let playerIndex = self.nextIndex;
		self.nextIndex = (self.nextIndex + 1) % self.playerList.len();
		self.playerVolumeList[playerIndex] = volume;
		let playerVolume = volume * self.volume;
		let audioPlayer = &mut self.playerList[playerIndex];
		if audioPlayer.isPlaying() {
			audioPlayer.stop();
		}
		audioPlayer.setVolume(playerVolume);
		audioPlayer.setAudioAsset(audioAsset.clone());
		audioPlayer.play(false);
	}

	//==============================================================================
	// 모두 정지.
	//==============================================================================
	pub fn stopAll(&mut self) {
		for audioPlayer in self.playerList.iter_mut() {
			if audioPlayer.isPlaying() {
				audioPlayer.stop();
			}
		}
	}

	//==============================================================================
	// 음소거 설정.
	//==============================================================================
	pub fn setMuted(&mut self, isMuted: bool) {
		self.isMuted = isMuted;
		for audioPlayer in self.playerList.iter_mut() {
			if isMuted {
				audioPlayer.mute();
			}
			else {
				audioPlayer.unmute();
			}
		}
	}

	//==============================================================================
	// 풀 전체의 음량 설정. (0 ~ 1, 재생 중인 것에도 바로 곱해진다)
	//==============================================================================
	pub fn setVolume(&mut self, volume: f32) {
		self.volume = clampVolume(volume);
		for (audioPlayer, playerVolume) in self.playerList.iter_mut().zip(self.playerVolumeList.iter()) {
			audioPlayer.setVolume(playerVolume * self.volume);
		}
	}

	//==============================================================================
	// 풀 전체의 음량.
	//==============================================================================
	pub fn getVolume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	// 음소거 여부.
	//==============================================================================
	pub fn isMuted(&self) -> bool {
		self.isMuted
	}

	//==============================================================================
	// 보이스 수 반환.
	//==============================================================================
	pub fn getVoiceCount(&self) -> usize {
		self.playerList.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::platform::audio::AudioPlayerBackend;

	//==============================================================================
	// 소리 장치 없이 시험할 풀. (플레이어는 컨텍스트 없이 만든다)
	//==============================================================================
	fn createSilentPool(voiceCount: usize) -> SoundEffectPool {
		let mut playerList = Vec::new();
		for _ in 0..voiceCount {
			playerList.push(AudioPlayer::new(AudioPlayerBackend::create(None)));
		}
		SoundEffectPool::createFromPlayerList(playerList)
	}

	//==============================================================================
	// 차례가 돌아가며 끝에서 처음으로 넘어간다.
	//==============================================================================
	#[test]
	fn playRotatesRoundRobin() {
		let mut soundEffectPool = createSilentPool(3);
		let audioAsset = Rc::new(AudioAsset::new());
		assert_eq!(soundEffectPool.getVoiceCount(), 3);
		for expectedNextIndex in [1, 2, 0, 1] {
			soundEffectPool.play(&audioAsset, 1.0);
			assert_eq!(soundEffectPool.nextIndex, expectedNextIndex);
		}
	}

	//==============================================================================
	// 플레이어 음량은 재생 음량과 풀 음량의 곱이고, 풀 음량을 바꾸면 다시 곱한다.
	//==============================================================================
	#[test]
	fn playerVolumeIsPlayVolumeTimesPoolVolume() {
		let mut soundEffectPool = createSilentPool(2);
		let audioAsset = Rc::new(AudioAsset::new());
		soundEffectPool.setVolume(0.8);
		soundEffectPool.play(&audioAsset, 0.5);
		soundEffectPool.play(&audioAsset, 1.0);
		assert!((soundEffectPool.playerList[0].getVolume() - 0.4).abs() < 1e-6);
		assert!((soundEffectPool.playerList[1].getVolume() - 0.8).abs() < 1e-6);
		soundEffectPool.setVolume(0.5);
		assert!((soundEffectPool.playerList[0].getVolume() - 0.25).abs() < 1e-6);
		assert!((soundEffectPool.playerList[1].getVolume() - 0.5).abs() < 1e-6);
		soundEffectPool.setVolume(f32::NAN);
		assert_eq!(soundEffectPool.getVolume(), 1.0);
	}

	//==============================================================================
	// 음소거 중에는 재생도 차례도 넘어가지 않는다.
	//==============================================================================
	#[test]
	fn mutedPoolSkipsPlay() {
		let mut soundEffectPool = createSilentPool(2);
		let audioAsset = Rc::new(AudioAsset::new());
		soundEffectPool.setMuted(true);
		assert!(soundEffectPool.isMuted());
		soundEffectPool.play(&audioAsset, 1.0);
		assert_eq!(soundEffectPool.nextIndex, 0);
		soundEffectPool.setMuted(false);
		soundEffectPool.play(&audioAsset, 1.0);
		assert_eq!(soundEffectPool.nextIndex, 1);
	}

	//==============================================================================
	// 보이스가 없으면 재생은 아무것도 하지 않는다.
	//==============================================================================
	#[test]
	fn emptyPoolIgnoresPlay() {
		let mut soundEffectPool = createSilentPool(0);
		let audioAsset = Rc::new(AudioAsset::new());
		soundEffectPool.play(&audioAsset, 1.0);
		soundEffectPool.stopAll();
		assert_eq!(soundEffectPool.getVoiceCount(), 0);
	}
}
