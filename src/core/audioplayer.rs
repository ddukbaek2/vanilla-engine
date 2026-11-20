//==============================================================================
// 오디오 플레이어.
// - 원본 core/audioplayer.js. AudioAsset 에 로드된 오디오 버퍼를 재생한다.
// - 음량, 음소거, 루프, 재생 위치는 여기서 들고, 소리를 내는 일은 백엔드(platform/audio 의 AudioPlayerBackend)가 한다.
//   원본의 GainNode 자리: 데스크탑은 플레이어마다 kira 믹서 하위 트랙 하나, 웹은 원본처럼 GainNode 하나.
// - 만드는 곳은 AudioManager::create_audio_player() 다. AudioContext 가 없으면 아무 소리도 내지 않는 플레이어가 된다. (원본과 같다)
// - 원본과 다른 점:
//   애셋은 Rc<AudioAsset> 로 나눠 든다.
//   플레이어를 버리면 재생 중인 소리도 멈춘다. (원본은 가비지 수거와 상관없이 소스 노드가 끝까지 운다)
//   데스크탑의 is_playing() 은 onended 콜백 대신 kira 소리 핸들의 상태를 읽는다. (끝나면 거짓)
//   데스크탑의 음량은 데시벨로 바꿔 트랙에 건다. 0.001(-60dB) 아래는 kira 가 무음으로 친다.
//   원본 getAudioContext() 는 옮기지 않았다. (백엔드마다 타입이 다르다)
//==============================================================================
use crate::platform::audio::AudioPlayerBackend;
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;

//==============================================================================
// 음량을 0 ~ 1 로 자른다. (유한하지 않으면 1, 원본 set_volume 의 규칙)
//==============================================================================
pub(crate) fn clamp_volume(volume: f32) -> f32 {
	if volume.is_finite() {
		volume.clamp(0.0, 1.0)
	}
	else {
		1.0
	}
}

//==============================================================================
// 오디오 플레이어.
//==============================================================================
pub struct AudioPlayer {
	audio_player_backend: AudioPlayerBackend,
	audio_asset: Option<Rc<AudioAsset>>,
	is_muted: bool,
	// 음량. (0 ~ 1, 음소거를 풀면 이 음량으로 돌아간다)
	volume: f32,
	// 루프 여부.
	is_loop: bool,
	// 재생 위치. (초, play() 를 부르면 이 위치부터 재생)
	time: f64,
}

impl AudioPlayer {
	//==============================================================================
	// 생성. (AudioManager::create_audio_player 가 백엔드를 만들어 넘긴다)
	//==============================================================================
	pub(crate) fn new(audio_player_backend: AudioPlayerBackend) -> AudioPlayer {
		AudioPlayer {
			audio_player_backend,
			audio_asset: None,
			is_muted: false,
			volume: 1.0,
			is_loop: false,
			time: 0.0,
		}
	}

	//==============================================================================
	// 재생. (재생 중이면 멈추고 재생 위치부터 다시 튼다)
	// - 웹: 오디오 컨텍스트가 일시 중단돼 있으면 재개한 뒤에 튼다.
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

		// 재생 중일 경우 정지.
		self.audio_player_backend.stop_source();

		// 내부 재생 시작.
		self.is_loop = is_loop;
		self.audio_player_backend.start_playback(&audio_buffer, is_loop, self.time);
	}

	//==============================================================================
	// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		self.audio_player_backend.stop();
	}

	//==============================================================================
	// 음소거. (백엔드가 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn mute(&mut self) {
		if self.audio_player_backend.is_available() {
			self.audio_player_backend.set_gain(0.0);
			self.is_muted = true;
		}
	}

	//==============================================================================
	// 음소거 해제. (설정한 음량으로 돌아간다)
	//==============================================================================
	pub fn unmute(&mut self) {
		if self.audio_player_backend.is_available() {
			self.audio_player_backend.set_gain(self.volume);
			self.is_muted = false;
		}
	}

	//==============================================================================
	// 음량 설정. (0 ~ 1, 음소거 중이면 음소거를 풀 때 적용된다)
	//==============================================================================
	pub fn set_volume(&mut self, volume: f32) {
		let clamped_volume = clamp_volume(volume);
		self.volume = clamped_volume;
		if self.audio_player_backend.is_available() && !self.is_muted {
			self.audio_player_backend.set_gain(clamped_volume);
		}
	}

	//==============================================================================
	// 음량 반환.
	//==============================================================================
	pub fn get_volume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	// 재생 위치 설정. (초, 재생 중이면 그 위치부터 바로 다시 튼다)
	//==============================================================================
	pub fn set_time(&mut self, time: f64) {
		self.time = time;
		if self.is_playing() {
			let is_loop = self.is_loop;
			self.play(is_loop);
		}
	}

	//==============================================================================
	// 오디오 애셋 설정.
	//==============================================================================
	pub fn set_audio_asset(&mut self, audio_asset: Rc<AudioAsset>) {
		self.audio_asset = Some(audio_asset);
	}

	//==============================================================================
	// 오디오 애셋 반환.
	//==============================================================================
	pub fn get_audio_asset(&self) -> Option<&Rc<AudioAsset>> {
		self.audio_asset.as_ref()
	}

	//==============================================================================
	// 재생 여부 반환.
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.audio_player_backend.is_playing()
	}

	//==============================================================================
	// 음소거 여부 반환.
	//==============================================================================
	pub fn is_muted(&self) -> bool {
		self.is_muted
	}

	//==============================================================================
	// 재생 위치 반환. (초, set_time 으로 정한 값. 원본처럼 재생 중에 흐르지 않는다)
	//==============================================================================
	pub fn get_time(&self) -> f64 {
		self.time
	}

	//==============================================================================
	// 재생 시간 반환. (초, 애셋이 없으면 0)
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
	// 음량 자르기는 원본 규칙을 따른다.
	//==============================================================================
	#[test]
	fn clamp_volume_follows_original_rule() {
		assert_eq!(clamp_volume(0.5), 0.5);
		assert_eq!(clamp_volume(-0.2), 0.0);
		assert_eq!(clamp_volume(1.7), 1.0);
		assert_eq!(clamp_volume(f32::NAN), 1.0);
		assert_eq!(clamp_volume(f32::INFINITY), 1.0);
	}

	//==============================================================================
	// 컨텍스트가 없는 플레이어는 음량만 기억하고 음소거는 걸리지 않는다. (원본: gain_node 가 없으면 mute 가 무시된다)
	//==============================================================================
	#[test]
	fn player_without_context_keeps_volume_only() {
		let mut audio_player = AudioPlayer::new(AudioPlayerBackend::create(None));
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
}
