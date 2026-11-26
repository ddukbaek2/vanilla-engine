//==============================================================================
//! 공간 소리. (원본 src/experimental/spatialaudio/)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - WebAudio PannerNode 와 AudioListener 그대로의 3D 소리다. 웹은 브라우저 PannerNode 를 그대로 쓰고,
//!   데스크탑은 엔진 믹서에 크롬 PannerHandler 의 셈을 옮긴 PannerNode 를 끼운다. (크롬 158 OfflineAudioContext 와 표본마다 같다)
//! - SpatialAudioPlayer: AudioPlayer 의 GainNode 뒤에 PannerNode 를 끼운 플레이어. AudioBus(SoundSystem 의 효과음 버스 같은)로 낼 수 있다.
//!   좌우 방식(등력, HRTF), 거리 모델(linear, inverse, exponential), 기준과 최대 거리, rolloff, 원뿔(안쪽, 바깥쪽 각과 바깥 이득), 자리와 방향.
//! - SpatialAudioListener: 컨텍스트의 리스너 하나. (자리, 앞, 위)
//! - AudioSource2D, AudioListener2D: 2D 노드의 전역 자리를 따라가는 컴포넌트. 화면 좌표(아래가 +Y)를 WebAudio 좌표(위가 +Y)로 바꾼다.
//! - 셈 함수(calculate_distance_gain, calculate_cone_gain, calculate_azimuth_elevation, calculate_equal_power_gains, calculate_distance_cone_gain)는
//!   크롬과 같은 셈이라 들리기 전에 음량을 어림하거나(멀어 안 들리는 소리 거르기) 시험에 쓴다.
//! - 도플러는 없다. WebAudio 가 명세에서 뺐다. (web-platform-tests webaudio/historical.html 이 AudioListener 의 dopplerFactor, speedOfSound, setVelocity 와
//!   PannerNode 의 setVelocity 가 없어야 한다고 본다) 필요하면 게임이 재생 빠르기로 흉내 낸다.
//! - HRTF: 웹은 브라우저가, 데스크탑은 엔진 믹서가 크롬 HRTFPanner 와 같은 셈과 크롬이 싣는 IRCAM 자료(Composite.flac)로 낸다.
//!   데스크탑은 처음 고를 때 그 표본율의 자료를 따로 스레드에서 짓고, 다 지을 때까지 크롬 실시간 컨텍스트처럼 좌우 없이 낸다.
//!   자료의 출처와 IRCAM 고지는 엔진 저장소의 src/platform/audio/hrtf/resources/README.md 에 있다.
//! # 예제
//! ```
//! use vanilla::experimental::spatialaudio::{DistanceModel, PannerSettings, SpatialAudioListener, SpatialAudioPlayer};
//! use vanilla::{AudioManager, Vector3};
//!
//! let audio_manager = AudioManager::new();
//! let mut listener = SpatialAudioListener::new(&audio_manager);
//! listener.set_position(Vector3::create(0.0, 0.0, 0.0));
//! let mut player = SpatialAudioPlayer::new(&audio_manager, None);
//! let mut settings = PannerSettings::default();
//! settings.distance_model = DistanceModel::Linear;
//! settings.maximum_distance = 30.0;
//! assert!(player.set_settings(settings));
//! player.set_position(Vector3::create(4.0, 0.0, -3.0));
//! assert!((player.calculate_gain(&listener) - (1.0 - 4.0 / 29.0)).abs() < 1e-6);
//! ```
//==============================================================================
mod audiolistener2d;
mod audiosource2d;
mod spatialaudiolistener;
mod spatialaudioplayer;

pub use crate::platform::audio::pannermath::{calculate_azimuth_elevation, calculate_cone_gain, calculate_distance_cone_gain, calculate_distance_gain, calculate_equal_power_gains, DistanceModel, PannerSettings, PanningModel};
pub use audiolistener2d::AudioListener2D;
pub use audiosource2d::{convert_view_position_to_audio_position, AudioSource2D};
pub use spatialaudiolistener::SpatialAudioListener;
pub use spatialaudioplayer::SpatialAudioPlayer;
