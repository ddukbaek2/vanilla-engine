//==============================================================================
// 오디오 버스. (원본 core/audiobus.js 의 AudioBus)
// - 여러 플레이어의 소리를 한 길로 모아 크기와 음소거를 한꺼번에 건다. (SoundSystem 의 배경음악 버스와 효과음 버스)
// - 만드는 곳은 AudioManager::create_audio_bus() 나 AudioBus::new(매니저) 다. (원본 new AudioBus(audioContext)) 그 버스로 트는 플레이어는 create_audio_player() 로 만든다.
// - 버스 자리는 백엔드가 채운다. (platform/audio 의 AudioBusBackend)
//   데스크탑: 엔진 믹서에서 출력에 이은 GainNode 하나. 버스로 만든 플레이어가 여기로 소리를 낸다. (셈은 크롬 WebAudio 와 같다)
//   웹: 원본처럼 출력에 이은 GainNode 하나. 버스로 만든 플레이어의 GainNode 가 여기에 이어진다.
// - AudioContext 가 없을 때 만든 버스는 소리가 없고, 그 버스로 만든 플레이어도 소리를 내지 않는다. (원본과 같다)
// - 크기와 음소거는 AudioPlayer 와 같은 규칙이다. (0 ~ 1 로 자르고, 음소거를 풀면 설정한 크기로 돌아간다)
// - 원본과 다른 것:
//   - 상태를 Rc<RefCell> 로 나눠 든다. SoundSystem 이 든 버스를 get_music_bus 로 빌려 준 채 크기를 바꾸고 플레이어를 만들 수 있게
//     메서드는 모두 &self 다. Clone 하면 같은 버스를 보는 손잡이가 하나 더 생긴다.
//   - 버스로 만든 플레이어는 버스 손잡이를 하나 든다. 손잡이를 모두 버리면 버스가 출력에서 떨어진다. (원본은 이은 노드를 브라우저가 살려 둔다)
//   - get_gain_node(원본 getGainNode)는 AudioGainNode 를 돌려준다. 웹은 GainNode, 데스크탑은 믹서의 버스 노드를 나눠 드는 Rc<MixerGainNode> 다.
//     어느 쪽이든 as_deref() 가 AudioPlayer::new 의 내보낼 곳이다. (AudioPlayer::new(&audio_manager, audio_bus.get_gain_node().as_deref()), 두 플랫폼이 같다)
//==============================================================================
use crate::core::audiomanager::{AudioManager, AudioManagerState};
use crate::core::audioplayer::{clamp_volume, AudioPlayer};
use crate::platform::audio::{AudioBusBackend, AudioGainNode, GainAutomation};
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
// 버스가 나눠 드는 상태.
//==============================================================================
struct AudioBusState {
	audio_bus_backend: AudioBusBackend,
	is_muted: bool,
	// 크기. (0 ~ 1, 음소거를 풀면 이 크기로 돌아간다)
	volume: f32,
}

//==============================================================================
// 오디오 버스.
//==============================================================================
#[derive(Clone)]
pub struct AudioBus {
	state: Rc<RefCell<AudioBusState>>,
	// 만든 매니저가 나눠 든 상태. (버스로 만든 플레이어의 get_audio_context)
	audio_manager_state: Rc<RefCell<AudioManagerState>>,
}

impl AudioBus {
	//==============================================================================
	// 생성. (원본 new AudioBus(audioContext), 매니저의 AudioContext 가 없으면 만든다, 만들지 못하면 소리 없는 버스)
	//==============================================================================
	pub fn new(audio_manager: &AudioManager) -> AudioBus {
		let audio_manager_state = audio_manager.get_shared_state();
		let audio_bus_backend = {
			let mut state = audio_manager_state.borrow_mut();
			state.ensure_audio_context();
			AudioBusBackend::create(&mut state)
		};
		AudioBus {
			state: Rc::new(RefCell::new(AudioBusState {
				audio_bus_backend,
				is_muted: false,
				volume: 1.0,
			})),
			audio_manager_state,
		}
	}

	//==============================================================================
	// 이 버스로 트는 오디오 플레이어 생성. (버스가 소리 없으면 소리 없는 플레이어)
	//==============================================================================
	pub fn create_audio_player(&self) -> AudioPlayer {
		let audio_player_backend = self.state.borrow_mut().audio_bus_backend.create_player_backend();
		let mut audio_player = AudioPlayer::from_backend(audio_player_backend);
		audio_player.set_audio_manager_state(self.audio_manager_state.clone());
		audio_player.set_audio_bus(self.clone());
		audio_player
	}

	//==============================================================================
	// 음소거. (소리 없는 버스면 아무것도 하지 않는다)
	//==============================================================================
	pub fn mute(&self) {
		let mut state = self.state.borrow_mut();
		if state.audio_bus_backend.is_available() {
			state.audio_bus_backend.set_gain(0.0);
			state.is_muted = true;
		}
	}

	//==============================================================================
	// 음소거 해제. (설정한 크기로 돌아간다)
	//==============================================================================
	pub fn unmute(&self) {
		let mut state = self.state.borrow_mut();
		if state.audio_bus_backend.is_available() {
			let volume = state.volume;
			state.audio_bus_backend.set_gain(volume);
			state.is_muted = false;
		}
	}

	//==============================================================================
	// 크기 설정. (0 ~ 1, 음소거 중이면 음소거를 풀 때 적용된다)
	//==============================================================================
	pub fn set_volume(&self, volume: f32) {
		let clamped_volume = clamp_volume(volume);
		let mut state = self.state.borrow_mut();
		state.volume = clamped_volume;
		if state.audio_bus_backend.is_available() && !state.is_muted {
			state.audio_bus_backend.set_gain(clamped_volume);
		}
	}

	//==============================================================================
	// 크기 반환.
	//==============================================================================
	pub fn get_volume(&self) -> f32 {
		self.state.borrow().volume
	}

	//==============================================================================
	// 음소거 여부 반환.
	//==============================================================================
	pub fn is_muted(&self) -> bool {
		self.state.borrow().is_muted
	}

	//==============================================================================
	// 소리를 모을 수 있는지. (AudioContext 가 있을 때 만든 버스)
	//==============================================================================
	pub fn is_available(&self) -> bool {
		self.state.borrow().audio_bus_backend.is_available()
	}

	//==============================================================================
	// 버스 노드의 gain 에 자동화 걸기. (SoundSystem 이 설정 크기와 음소거를 걸 때, 원본 gainNode.gain.setTargetAtTime)
	// - 크기 칸과 음소거 칸은 건드리지 않는다. 그래서 set_volume, mute 로 건 값은 다음 자동화가 덮을 때까지만 간다. (원본도 같은 노드에 건다)
	//==============================================================================
	pub(crate) fn apply_gain_automation(&self, automation: GainAutomation) {
		self.state.borrow_mut().audio_bus_backend.apply_gain_automation(automation);
	}

	//==============================================================================
	// 버스의 GainNode. (원본 getGainNode, 웹 GainNode, 데스크탑 버스 트랙을 나눠 드는 Rc<RefCell<TrackHandle>>, AudioContext 가 없을 때 만든 버스는 None)
	// - 노드에 직접 건 크기는 크기 칸과 음소거 칸을 바꾸지 않는다. (원본과 같다) 데스크탑은 트랙을 borrow_mut 로 빌린 동안 이 버스의 함수를 부르지 않는다.
	//==============================================================================
	pub fn get_gain_node(&self) -> Option<AudioGainNode> {
		self.state.borrow().audio_bus_backend.get_gain_node().cloned()
	}
}

#[cfg(test)]
mod tests {
	use crate::core::audiobus::AudioBus;
	use crate::core::audiomanager::AudioManager;
	use crate::core::audioplayer::AudioPlayer;

	//==============================================================================
	// 크기는 0 ~ 1 로 잘리고, 음소거는 소리를 모을 수 있는 버스에만 걸린다. 버스로 만든 플레이어는 버스와 같은 컨텍스트를 본다.
	//==============================================================================
	#[test]
	fn volume_and_mute_follow_player_rule() {
		let audio_manager = AudioManager::new();
		let audio_bus = audio_manager.create_audio_bus();
		assert_eq!(audio_bus.get_volume(), 1.0);
		audio_bus.set_volume(1.5);
		assert_eq!(audio_bus.get_volume(), 1.0);
		audio_bus.set_volume(0.25);
		assert_eq!(audio_bus.get_volume(), 0.25);
		audio_bus.mute();
		assert_eq!(audio_bus.is_muted(), audio_bus.is_available());
		audio_bus.set_volume(0.5);
		assert_eq!(audio_bus.get_volume(), 0.5);
		audio_bus.unmute();
		assert!(!audio_bus.is_muted());
		let shared_audio_bus = audio_bus.clone();
		shared_audio_bus.set_volume(0.75);
		assert_eq!(audio_bus.get_volume(), 0.75);
		let audio_player = audio_bus.create_audio_player();
		assert_eq!(audio_player.get_audio_context().is_some(), audio_manager.has_audio_context());
		assert!(!audio_player.is_playing());

		// 원본 new AudioBus(audioContext), new AudioPlayer(audioContext, bus.getGainNode()) 처럼 만든다.
		let other_audio_bus = AudioBus::new(&audio_manager);
		let is_bus_available = other_audio_bus.is_available();
		assert_eq!(is_bus_available, audio_manager.has_audio_context());
		let has_gain_node = other_audio_bus.get_gain_node().is_some();
		assert_eq!(has_gain_node, is_bus_available);
		let mut bus_audio_player = AudioPlayer::new(&audio_manager, other_audio_bus.get_gain_node().as_deref());
		bus_audio_player.set_volume(0.5);
		assert_eq!(bus_audio_player.get_volume(), 0.5);
		bus_audio_player.mute();
		assert_eq!(bus_audio_player.is_muted(), is_bus_available);
	}
}
