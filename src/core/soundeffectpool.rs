//==============================================================================
// 효과음 풀.
// - 원본 core/soundeffectpool.js. 같은 효과음이 연달아 겹쳐 나도 끊기지 않도록
//   여러 AudioPlayer 를 돌려 가며(라운드로빈) 재생한다.
// - 백엔드를 직접 부르지 않는다. AudioPlayer 위에서 차례와 음량만 센다.
// - 사용:
//     let mut sound_effect_pool = SoundEffectPool::new(engine.get_audio_manager(), 6);
//     sound_effect_pool.play(&hitAudioAsset, 1.0);
//     sound_effect_pool.play(&hitAudioAsset, 0.5);   // 이 재생만 절반 음량.
//     sound_effect_pool.set_volume(0.8);              // 풀 전체의 음량. (재생 중인 것에도 곱해진다)
// - 보이스 수가 0 이면 play 는 원본처럼 아무것도 하지 않는다.
// - 원본과 다른 점:
//   play 의 음량은 기본값이 없어 늘 넘긴다. (원본 기본 1)
//==============================================================================
use crate::core::audiomanager::AudioManager;
use crate::core::audioplayer::{clamp_volume, AudioPlayer};
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;

// 원본 생성자의 기본 보이스 수.
pub const DEFAULT_VOICE_COUNT: usize = 6;

//==============================================================================
// 효과음 풀.
//==============================================================================
pub struct SoundEffectPool {
	player_list: Vec<AudioPlayer>,
	// 플레이어마다 마지막 재생의 음량. (풀의 음량을 곱하기 전)
	player_volume_list: Vec<f32>,
	next_index: usize,
	is_muted: bool,
	// 풀 전체의 음량. (0 ~ 1)
	volume: f32,
}

impl SoundEffectPool {
	//==============================================================================
	// 생성. (voice_count: 동시에 겹칠 수 있는 최대 재생 수)
	//==============================================================================
	pub fn new(audio_manager: &AudioManager, voice_count: usize) -> SoundEffectPool {
		let mut player_list = Vec::with_capacity(voice_count);
		for _ in 0..voice_count {
			let audio_player = audio_manager.create_audio_player();
			player_list.push(audio_player);
		}
		SoundEffectPool::create_from_player_list(player_list)
	}

	//==============================================================================
	// 만들어 둔 플레이어 목록으로 생성.
	//==============================================================================
	fn create_from_player_list(player_list: Vec<AudioPlayer>) -> SoundEffectPool {
		let player_volume_list = vec![1.0; player_list.len()];
		SoundEffectPool {
			player_list,
			player_volume_list,
			next_index: 0,
			is_muted: false,
			volume: 1.0,
		}
	}

	//==============================================================================
	// 효과음 재생.
	// - 다음 차례의 플레이어에 애셋을 실어 재생한다. 그 플레이어가 재생 중이었다면 끊고 새로 튼다.
	// - 음량은 이 재생의 음량(0 ~ 1)에 풀의 음량을 곱한 것이다.
	//==============================================================================
	pub fn play(&mut self, audio_asset: &Rc<AudioAsset>, volume: f32) {
		if self.is_muted || self.player_list.is_empty() {
			return;
		}
		let player_index = self.next_index;
		self.next_index = (self.next_index + 1) % self.player_list.len();
		self.player_volume_list[player_index] = volume;
		let player_volume = volume * self.volume;
		let audio_player = &mut self.player_list[player_index];
		if audio_player.is_playing() {
			audio_player.stop();
		}
		audio_player.set_volume(player_volume);
		audio_player.set_audio_asset(audio_asset.clone());
		audio_player.play(false);
	}

	//==============================================================================
	// 모두 정지.
	//==============================================================================
	pub fn stop_all(&mut self) {
		for audio_player in self.player_list.iter_mut() {
			if audio_player.is_playing() {
				audio_player.stop();
			}
		}
	}

	//==============================================================================
	// 음소거 설정.
	//==============================================================================
	pub fn set_muted(&mut self, is_muted: bool) {
		self.is_muted = is_muted;
		for audio_player in self.player_list.iter_mut() {
			if is_muted {
				audio_player.mute();
			}
			else {
				audio_player.unmute();
			}
		}
	}

	//==============================================================================
	// 풀 전체의 음량 설정. (0 ~ 1, 재생 중인 것에도 바로 곱해진다)
	//==============================================================================
	pub fn set_volume(&mut self, volume: f32) {
		self.volume = clamp_volume(volume);
		for (audio_player, player_volume) in self.player_list.iter_mut().zip(self.player_volume_list.iter()) {
			audio_player.set_volume(player_volume * self.volume);
		}
	}

	//==============================================================================
	// 풀 전체의 음량.
	//==============================================================================
	pub fn get_volume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	// 음소거 여부.
	//==============================================================================
	pub fn is_muted(&self) -> bool {
		self.is_muted
	}

	//==============================================================================
	// 보이스 수 반환.
	//==============================================================================
	pub fn get_voice_count(&self) -> usize {
		self.player_list.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::platform::audio::AudioPlayerBackend;

	//==============================================================================
	// 소리 장치 없이 시험할 풀. (플레이어는 컨텍스트 없이 만든다)
	//==============================================================================
	fn create_silent_pool(voice_count: usize) -> SoundEffectPool {
		let mut player_list = Vec::new();
		for _ in 0..voice_count {
			player_list.push(AudioPlayer::from_backend(AudioPlayerBackend::create_silent()));
		}
		SoundEffectPool::create_from_player_list(player_list)
	}

	//==============================================================================
	// 차례가 돌아가며 끝에서 처음으로 넘어간다.
	//==============================================================================
	#[test]
	fn play_rotates_round_robin() {
		let mut sound_effect_pool = create_silent_pool(3);
		let audio_asset = Rc::new(AudioAsset::new());
		assert_eq!(sound_effect_pool.get_voice_count(), 3);
		for expected_next_index in [1, 2, 0, 1] {
			sound_effect_pool.play(&audio_asset, 1.0);
			assert_eq!(sound_effect_pool.next_index, expected_next_index);
		}
	}

	//==============================================================================
	// 플레이어 음량은 재생 음량과 풀 음량의 곱이고, 풀 음량을 바꾸면 다시 곱한다.
	//==============================================================================
	#[test]
	fn player_volume_is_play_volume_times_pool_volume() {
		let mut sound_effect_pool = create_silent_pool(2);
		let audio_asset = Rc::new(AudioAsset::new());
		sound_effect_pool.set_volume(0.8);
		sound_effect_pool.play(&audio_asset, 0.5);
		sound_effect_pool.play(&audio_asset, 1.0);
		assert!((sound_effect_pool.player_list[0].get_volume() - 0.4).abs() < 1e-6);
		assert!((sound_effect_pool.player_list[1].get_volume() - 0.8).abs() < 1e-6);
		sound_effect_pool.set_volume(0.5);
		assert!((sound_effect_pool.player_list[0].get_volume() - 0.25).abs() < 1e-6);
		assert!((sound_effect_pool.player_list[1].get_volume() - 0.5).abs() < 1e-6);
		sound_effect_pool.set_volume(f32::NAN);
		assert_eq!(sound_effect_pool.get_volume(), 1.0);
	}

	//==============================================================================
	// 음소거 중에는 재생도 차례도 넘어가지 않는다.
	//==============================================================================
	#[test]
	fn muted_pool_skips_play() {
		let mut sound_effect_pool = create_silent_pool(2);
		let audio_asset = Rc::new(AudioAsset::new());
		sound_effect_pool.set_muted(true);
		assert!(sound_effect_pool.is_muted());
		sound_effect_pool.play(&audio_asset, 1.0);
		assert_eq!(sound_effect_pool.next_index, 0);
		sound_effect_pool.set_muted(false);
		sound_effect_pool.play(&audio_asset, 1.0);
		assert_eq!(sound_effect_pool.next_index, 1);
	}

	//==============================================================================
	// 보이스가 없으면 재생은 아무것도 하지 않는다.
	//==============================================================================
	#[test]
	fn empty_pool_ignores_play() {
		let mut sound_effect_pool = create_silent_pool(0);
		let audio_asset = Rc::new(AudioAsset::new());
		sound_effect_pool.play(&audio_asset, 1.0);
		sound_effect_pool.stop_all();
		assert_eq!(sound_effect_pool.get_voice_count(), 0);
	}
}
