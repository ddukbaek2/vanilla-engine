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
// - 데스크탑: kira 0.12. (desktop.rs)
// - 웹: 원본과 같은 WebAudio 를 web-sys 로 직접 부른다. (web.rs, kira 는 wasm 에서 파일 읽기와 스트리밍이 안 된다)
// - 게임 코드가 이름을 부를 수 있게 AudioContext, AudioBuffer 두 타입만 밖으로 낸다. 나머지는 엔진 안에서만 쓴다.
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod desktop;
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

// 플레이어가 소리를 내보낼 곳과 버스의 GainNode. (AudioPlayer::new 의 둘째 인자, 원본 outputNode 자리와 AudioBus.getGainNode)
// - AudioGainNode 는 데스크탑이 버스 트랙을 나눠 드는 Rc<RefCell<TrackHandle>>, 웹이 GainNode 다. 그 as_deref 가 AudioOutputNode 라서
//   AudioPlayer::new(&audio_manager, audio_bus.get_gain_node().as_deref()) 가 두 플랫폼에서 같다. (웹은 다른 &AudioNode 도 넘길 수 있다)
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{AudioGainNode, AudioOutputNode};
#[cfg(target_arch = "wasm32")]
pub use web::{AudioGainNode, AudioOutputNode};
