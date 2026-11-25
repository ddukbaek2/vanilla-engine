//==============================================================================
// 오디오 백엔드.
// - AudioManager, AudioPlayer, AudioAsset, BeepPlayer 가 소리를 낼 때 부르는 아래층이다. 공통 코드는 core, resource, misc 에 있다.
// - 백엔드마다 아래 이름을 같은 꼴로 낸다.
//   AudioContext(공유 컨텍스트), AudioBuffer(디코딩한 소리), AudioPlatformState(매니저가 드는 백엔드 상태),
//   AudioPlayerBackend(플레이어 하나의 소리 내는 부분), AudioBusBackend(버스 하나의 소리 모으는 부분, create_player_backend 로 그 버스로 트는 플레이어),
//   HAS_USER_GESTURE_AT_START,
//   create_audio_platform_state, create_audio_context, resume_audio_context, is_audio_context_running,
//   load_audio_file, load_audio_bytes, get_audio_buffer_duration, play_beep_tone
//   suspend_audio_context(컨텍스트 멈춤), get_audio_context_time(컨텍스트 시계), AudioPlayerBackend::set_pan(좌우)
// - 데스크탑: cpal 출력 위의 엔진 믹서. (desktop.rs, mixer.rs. 노드 그래프와 셈이 크롬 WebAudio 와 같다) 소리 파일 해석은 크롬 decodeAudioData 와 같게 decode/ 가 한다. (symphonia 0.6, Opus 는 opus-pure)
// - 웹: 원본과 같은 WebAudio 를 web-sys 로 직접 부른다. (web.rs)
// - 게임 코드가 이름을 부를 수 있게 AudioContext, AudioBuffer 두 타입만 밖으로 낸다. 나머지는 엔진 안에서만 쓴다.
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod audioparam;
#[cfg(not(target_arch = "wasm32"))]
mod decode;
#[cfg(not(target_arch = "wasm32"))]
mod desktop;
#[cfg(not(target_arch = "wasm32"))]
mod mixer;
#[cfg(not(target_arch = "wasm32"))]
mod oscillator;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use desktop::*;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{AudioBuffer, AudioContext};

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub(crate) use web::*;
#[cfg(target_arch = "wasm32")]
pub use web::{AudioBuffer, AudioContext};

// GainNode 의 gain 자동화. (원본 AudioParam 의 setValueAtTime, linearRampToValueAtTime, exponentialRampToValueAtTime, setTargetAtTime, cancelScheduledValues.
// 웹은 WebAudio AudioParam 에, 데스크탑은 믹서의 AudioParam(크로미엄과 같은 셈)에 건다. SoundSystem 이 원본처럼 쓴다)
#[derive(Clone, Copy, Debug)]
pub(crate) enum GainAutomation {
	SetValueAtTime {
		value: f32,
		time: f64,
	},
	LinearRampToValueAtTime {
		value: f32,
		time: f64,
	},
	#[allow(dead_code)]
	ExponentialRampToValueAtTime {
		value: f32,
		time: f64,
	},
	SetTargetAtTime {
		target: f32,
		time: f64,
		time_constant: f64,
	},
	CancelScheduledValues {
		time: f64,
	},
}

// 플레이어가 소리를 내보낼 곳과 버스의 GainNode. (AudioPlayer::new 의 둘째 인자, 원본 outputNode 자리와 AudioBus.getGainNode)
// - AudioGainNode 는 데스크탑이 믹서의 버스 노드를 나눠 드는 Rc<MixerGainNode>, 웹이 GainNode 다. 그 as_deref 가 AudioOutputNode 라서
//   AudioPlayer::new(&audio_manager, audio_bus.get_gain_node().as_deref()) 가 두 플랫폼에서 같다. (웹은 다른 &AudioNode 도 넘길 수 있다)
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{AudioGainNode, AudioOutputNode};
#[cfg(target_arch = "wasm32")]
pub use web::{AudioGainNode, AudioOutputNode};
