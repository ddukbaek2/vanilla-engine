//==============================================================================
//! 소리 표로 다루는 배경음악과 효과음. (vanilla.js core/soundsystem.js 의 SoundSystem)
//! - AudioManager 와 AudioAsset 을 쓴다. 소리를 합성하지 않고 파일만 튼다.
//! - 소리 표의 줄마다 SoundRow { id, paths, volume, minimum_interval, is_music, maximum_voices, pitch_variation, play_seconds, fade_out_seconds }.
//!   paths: 같은 소리를 여러 번 녹음한 무리. 낼 때마다 받아 둔 것 가운데 하나를 고르고 바로 앞과 같은 것은 고르지 않는다.
//!   minimum_interval: 같은 소리를 다시 낼 수 있는 간격. (초, 한꺼번에 몰려 시끄럽지 않게)
//!   maximum_voices: 같은 소리가 한꺼번에 날 수 있는 수. (넘으면 가장 오래된 것을 끊는다, None 이면 효과음 전체의 수까지, 0 이면 원본처럼 하나)
//!   pitch_variation: 낼 때마다 음 높이를 흔드는 비. (0.05 면 ±5%)
//!   play_seconds, fade_out_seconds: 효과음을 앞의 play_seconds 초만 틀고 fade_out_seconds 초 동안 줄여 끈다. (0 이면 끝까지)
//! - 크기: 배경음악 = 음악 크기 × 음악의 비 × 소리의 크기, 효과음 = 효과음 크기 × 소리의 크기 × 부를 때의 크기.
//!   음악과 효과음의 크기는 버스(get_music_bus, get_effect_bus 의 AudioBus)에 걸고, 바꿀 때 0.05 초를 시간 상수로 따라간다. (원본 setTargetAtTime)
//!   버스는 첫 누름 뒤에 만든다. (그 전에는 None) 버스로 만든 플레이어(AudioBus::create_audio_player)도 설정 크기와 음소거를 따른다.
//! - 배경음악은 틀어 달라고 한 것(get_music_id)을 기억해 두고, 받기 전이거나 첫 누름 전이면 그때 튼다. ("" 이면 끈다)
//!   겹쳐 바꿀 수 있고(fade_seconds), 멈췄던 자리를 소리마다 기억해 이어 틀 수 있다. (is_resumed) 첫 파일을 받지 못했으면 틀지 않는다.
//! - 음소거는 설정한 크기보다 먼저다. 아래 가운데 하나라도 맞으면 소리가 나지 않는다.
//!   바깥의 음소거(set_external_mute_provider, 플랫폼 SDK 나 광고), 사용자의 음소거(set_user_muted), 페이지가 가려짐(다른 탭, 앱 전환),
//!   터치가 주된 기기에서 창이 포커스를 잃음(blur). 아이폰은 페이지가 가려졌다는 알림이 앱이 내려간 뒤에 와서 오디오가 멈추지 못한 채 얼어
//!   마지막 소리 조각이 되풀이되는데, 포커스를 잃는 알림은 그보다 먼저 온다. 데스크탑(마우스)은 캔버스 바깥을 누를 때도 포커스를 잃어 따르지 않는다.
//!   포커스가 돌아오거나 화면을 누르거나 페이지가 다시 보이면 풀린다.
//! - 음소거인 동안은 AudioContext 도 멈춘다. (풀리면 깨운다) 아이폰은 돌던 AudioContext 를 앱 전환이나 전면 광고가 가로채면 돌아와서 깨워도
//!   소리가 나지 않아, 그 전에 스스로 멈춰 둔다. 풀린 뒤에도 돌지 않으면 0.5 초마다 다시 깨워 본다.
//! - 허락하면(set_autoplay_allowed_provider, 주지 않으면 허락) 누르기 전에도 첫 update 에서 한 번 깨워 본다. 막히면 엔진이 첫 누름에 깨운다.
//!   첫 누름(user gesture) 전이거나 AudioContext 가 돌지 않을 때는 효과음을 내지 않는다. (멈춘 동안 부른 소리가 깨어날 때 한꺼번에 나지 않게)
//! - 소리 파일은 놀이를 막지 않게 뒤에서 받는다. (웹은 디코딩이 끝나면 난다) 못 받은 소리는 나지 않을 뿐이다.
//! - 시각: 같은 소리 간격과 다시 깨우기 간격은 원본 performance.now() 처럼 벽시계(web_time::Instant), 크기 따라가기와 줄이기,
//!   음악의 멈춘 자리는 원본처럼 AudioContext 시계(AudioManager::get_context_time)로 센다.
//! - 함께 쓰는 소리: set_sound_system, read_sound_system. (원본 모듈 변수, 스레드 하나에 하나)
//! - 원본과 다른 것:
//!   - 만들 때 AudioManager 대신 엔진을 받는다. 원본은 document 와 window 에 리스너를 다는데, 러스트 판은 엔진의 페이지 이벤트(PageEvent)로 받기 때문이다.
//!     이벤트가 온 그 자리에서는 AudioContext 를 멈추거나 깨우고(소리가 바로 멎는다), 효과음 끊기는 다음 update 에서 한다.
//!   - load_sounds, load_sound 는 애셋 폴더 기준 경로를 풀 엔진을 받는다. (원본은 브라우저가 페이지 주소로 푼다) 데스크탑은 그 자리에서 읽는다.
//!     load_sounds 는 원본처럼 await 할 수 있는 SoundLoadHandle 을 돌려준다. (걸어 둔 소리를 다 받거나 실패할 때까지 기다린다, wait_for_load().await 도 같다)
//!   - 크기 바꾸기와 줄이기는 원본처럼 GainNode 의 AudioParam 자동화(value 대입, setValueAtTime, linearRampToValueAtTime, setTargetAtTime,
//!     cancelScheduledValues)와 source.stop(when)으로 소리 장치에 맡긴다. 웹은 WebAudio 에, 데스크탑은 믹서(크로미엄과 같은 셈)에 건다.
//!     원본 { source, gainNode } 자리는 버스로 만든 AudioPlayer 하나이고, 끝난 소리(원본 onended)는 update 에서 목록에서 뺀다.
//!     원본처럼 버스의 set_volume, mute 로 건 값은 SoundSystem 이 크기를 다시 걸 때까지 간다.
//!   - 무리에서 고르기와 음 높이 흔들기는 원본 Math.random 자리인 엔진 math::random(처음 쓸 때 시각으로 씨앗을 정한다)을 쓴다.
//!   - add_sound_asset(파일로 받지 않는 소리 더하기)과 get_effect_voice_count 는 러스트 판에서 먼저 만들었다. (원본에도 같은 이름으로 넣었다)
//==============================================================================
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use web_time::Instant;

use crate::base::math;
use crate::core::audiobus::AudioBus;
use crate::core::audiomanager::{resume_audio_manager_state, AudioManager, AudioManagerState};
use crate::core::audioplayer::AudioPlayer;
use crate::core::engine::{Engine, PageEvent, PageEventListener};
use crate::platform;
use crate::platform::audio::{self as audio_backend, GainAutomation};
use crate::resource::audioasset::{AudioAsset, AudioLoadHandle};

//==============================================================================
/// 소리 받기 손잡이. (SoundSystem::load_sounds 가 돌려준다. 원본 loadSounds 의 Promise 자리, 걸어 둔 소리 애셋을 나눠 든다)
/// - 그대로 await 하면 걸어 둔 소리를 다 받거나 실패할 때까지 기다린다. (못 받은 소리는 건너뛰고, 돌려주는 값은 없다)
///
/// # 예제
/// 받기를 다 기다린 뒤 배경음악 틀기
/// ```rust,no_run
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{Engine, EngineConfiguration, SoundRow, SoundSystem};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let sound_table = vec![SoundRow::create_music("field", "tests/assets/audio/vorbis_stereo.ogg", 0.6)];
/// let sound_system = Rc::new(RefCell::new(SoundSystem::new(&engine, sound_table)));
/// let sound_load_handle = sound_system.borrow_mut().load_sounds(&engine);
/// let task_sound_system = sound_system.clone();
/// engine.spawn_task(move |_engine_context| async move {
/// 	sound_load_handle.await;
/// 	task_sound_system.borrow_mut().play_music("field", 0.5, false);
/// });
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct SoundLoadHandle {
	audio_load_handle_list: Vec<AudioLoadHandle>,
}

impl std::future::IntoFuture for SoundLoadHandle {
	type Output = ();
	type IntoFuture = SoundLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> SoundLoadFuture {
		SoundLoadFuture {
			audio_load_handle_list: self.audio_load_handle_list,
		}
	}
}

//==============================================================================
/// 소리 받기를 기다리는 퓨처. (다 받거나 실패할 때까지, 원본처럼 못 받은 소리는 건너뛴다)
/// - SoundLoadHandle 을 await 하거나 SoundSystem::wait_for_load 가 돌려준다. 엔진 작업 안에서 await 한다.
///
/// # 예제
/// 못 받은 파일이 섞여 있어도 끝난다 (데스크탑은 그 자리에서 읽어 첫 폴링에 끝난다)
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, SoundRow, SoundSystem};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let hit_row = SoundRow::create_effect("hit", &["tests/assets/audio/wav_s16_stereo.wav", "tests/assets/audio/missing.wav"], 0.8);
/// let mut sound_system = SoundSystem::new(&engine, vec![hit_row.clone()]);
/// sound_system.load_sound(&engine, &hit_row);
/// let sound_load_future = sound_system.wait_for_load();
/// let task_handle = engine.spawn_task(|_engine_context| async move {
/// 	sound_load_future.await;
/// });
/// assert!(task_handle.is_finished());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct SoundLoadFuture {
	audio_load_handle_list: Vec<AudioLoadHandle>,
}

impl std::future::Future for SoundLoadFuture {
	type Output = ();

	//==============================================================================
	/// 다 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: std::pin::Pin<&mut SoundLoadFuture>, context: &mut std::task::Context<'_>) -> std::task::Poll<()> {
		if self.audio_load_handle_list.iter().all(|audio_load_handle| audio_load_handle.is_finished()) {
			return std::task::Poll::Ready(());
		}
		context.waker().wake_by_ref();
		std::task::Poll::Pending
	}
}

/// 효과음을 동시에 낼 수 있는 수. (넘으면 가장 오래된 것을 끊는다)
const EFFECT_VOICE_COUNT: usize = 12;
/// 음소거가 아닌데 AudioContext 가 돌지 않을 때 다시 깨워 보는 간격. (초)
const CONTEXT_RESUME_RETRY_INTERVAL: f64 = 0.5;
/// 크기를 바꿀 때 따라가는 시간. (초, 툭 끊기지 않게)
const VOLUME_SMOOTHING_SECONDS: f64 = 0.05;
/// 효과음을 좌우로 나누는 가장 큰 값. (-1 ~ 1)
const MAXIMUM_PAN: f64 = 1.0;
/// 이 크기 이하의 효과음은 내지 않는다.
const MINIMUM_EFFECT_VOLUME: f64 = 0.001;
/// 줄여 끈 효과음을 다 줄어든 뒤 멈추기까지 남기는 틈. (초)
const EFFECT_STOP_MARGIN_SECONDS: f64 = 0.02;

thread_local! {
	/// 함께 쓰는 소리. (set_sound_system 으로 정한다)
	static CURRENT_SOUND_SYSTEM: RefCell<Option<Rc<RefCell<SoundSystem>>>> = const { RefCell::new(None) };
}

//==============================================================================
/// 소리 표의 한 줄.
/// - create_effect, create_music 으로 만들고 나머지 칸은 필드에 바로 쓴다.
///
/// # 예제
/// 녹음 무리가 있는 효과음과 배경음악
/// ```rust
/// use vanilla::SoundRow;
///
/// let mut hit_row = SoundRow::create_effect("hit", &["tests/assets/audio/wav_s16_stereo.wav", "tests/assets/audio/mp3_stereo.mp3"], 0.8);
/// hit_row.minimum_interval = 0.05;
/// hit_row.maximum_voices = Some(3);
/// hit_row.pitch_variation = 0.05;
/// assert_eq!(hit_row.paths.len(), 2);
/// assert!(!hit_row.is_music);
///
/// let field_row = SoundRow::create_music("field", "tests/assets/audio/vorbis_stereo.ogg", 0.6);
/// assert!(field_row.is_music);
/// assert_eq!(field_row.paths, vec![String::from("tests/assets/audio/vorbis_stereo.ogg")]);
/// assert_eq!(field_row.maximum_voices, None);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct SoundRow {
	/// 소리 이름. (play_effect, play_music 에 넘기는 id)
	pub id: String,
	/// 같은 소리를 여러 번 녹음한 무리의 파일 경로. (애셋 폴더 기준, 효과음은 낼 때마다 받아 둔 것 가운데 바로 앞과 다른 것을 고르고, 배경음악은 첫 파일만 튼다)
	pub paths: Vec<String>,
	/// 소리의 크기. (0 ~ 1, 효과음은 부를 때의 크기와 곱한 뒤 0 ~ 1 로 다듬는다)
	pub volume: f64,
	/// 같은 효과음을 다시 낼 수 있는 간격. (초, 0 이면 막지 않는다)
	pub minimum_interval: f64,
	/// 배경음악 줄인지. (load_sounds 가 효과음을 먼저, 배경음악을 나중에 받는다)
	pub is_music: bool,
	/// 같은 효과음이 한꺼번에 날 수 있는 수. (넘으면 가장 오래된 것을 끊는다, None 이면 효과음 전체의 수 12 까지, 0 이면 하나)
	pub maximum_voices: Option<usize>,
	/// 낼 때마다 음 높이를 흔드는 비. (0.05 면 ±5%, 0 이면 흔들지 않는다)
	pub pitch_variation: f64,
	/// 효과음을 앞에서부터 틀 시간. (초, 0 이면 끝까지)
	pub play_seconds: f64,
	/// play_seconds 뒤에 줄여 끄는 시간. (초, play_seconds 가 0 이면 쓰지 않는다)
	pub fade_out_seconds: f64,
}

impl SoundRow {
	//==============================================================================
	/// 효과음 한 줄. (나머지 칸은 기본)
	///
	/// # 인자
	/// - `id`: 소리 이름.
	/// - `paths`: 같은 소리를 녹음한 파일 경로들. (애셋 폴더 기준)
	/// - `volume`: 소리의 크기. (0 ~ 1)
	///
	/// # 돌려줌
	/// 효과음 줄. (minimum_interval, pitch_variation, play_seconds, fade_out_seconds 는 0, maximum_voices 는 None)
	//==============================================================================
	pub fn create_effect(id: &str, paths: &[&str], volume: f64) -> SoundRow {
		SoundRow {
			id: String::from(id),
			paths: paths.iter().map(|path| String::from(*path)).collect(),
			volume,
			minimum_interval: 0.0,
			is_music: false,
			maximum_voices: None,
			pitch_variation: 0.0,
			play_seconds: 0.0,
			fade_out_seconds: 0.0,
		}
	}

	//==============================================================================
	/// 배경음악 한 줄.
	///
	/// # 인자
	/// - `id`: 소리 이름.
	/// - `path`: 음악 파일 경로. (애셋 폴더 기준)
	/// - `volume`: 소리의 크기. (0 ~ 1)
	///
	/// # 돌려줌
	/// 배경음악 줄. (is_music 이 true, 경로 하나, 나머지 칸은 create_effect 와 같다)
	//==============================================================================
	pub fn create_music(id: &str, path: &str, volume: f64) -> SoundRow {
		let mut sound_row = SoundRow::create_effect(id, &[path], volume);
		sound_row.is_music = true;
		sound_row
	}
}

//==============================================================================
/// 나고 있는 효과음 하나. (원본 { soundId, source, gainNode, startTime }, 원본도 startTime 은 읽지 않아 뺐다)
//==============================================================================
struct EffectVoice {
	sound_id: String,
	audio_player: AudioPlayer,
}

//==============================================================================
/// 틀고 있는 배경음악 하나. (원본 { soundId, source, gain, startTime, offset, duration })
//==============================================================================
struct MusicVoice {
	sound_id: String,
	audio_player: AudioPlayer,
	/// 튼 때. (컨텍스트 시계 초)
	start_time: f64,
	/// 튼 자리와 소리 길이. (초)
	offset: f64,
	duration: f64,
}


//==============================================================================
/// 페이지 이벤트 리스너와 나눠 드는 음소거 상태. (이벤트가 온 그 자리에서 AudioContext 를 멈추거나 깨운다)
//==============================================================================
struct SoundMuteState {
	external_mute_provider: Option<Rc<dyn Fn() -> bool>>,
	is_user_muted: bool,
	is_page_hidden: bool,
	is_blur_mute_enabled: bool,
	is_window_blurred: bool,
	is_context_paused: bool,
	/// 이벤트로 음소거가 되어 다음 update 에서 효과음을 끊을 차례인지.
	is_stop_effects_requested: bool,
	audio_manager_state: Rc<RefCell<AudioManagerState>>,
}

impl SoundMuteState {
	//==============================================================================
	/// 지금 음소거여야 하는지. (바깥의 음소거, 사용자의 음소거, 페이지 가려짐, 창이 포커스를 잃음)
	//==============================================================================
	fn read_muted(&self) -> bool {
		let is_platform_muted = match &self.external_mute_provider {
			Some(external_mute_provider) => {
				external_mute_provider()
			}
			None => {
				false
			}
		};
		is_platform_muted || self.is_user_muted || self.is_page_hidden || self.is_window_blurred
	}

	//==============================================================================
	/// 음소거인 동안 AudioContext 멈추기와 풀리면 깨우기. (바뀔 때만, 첫 누름 전에는 AudioContext 가 없어 하지 않는다)
	//==============================================================================
	fn apply_context_pause(&mut self) {
		let audio_manager_state = self.audio_manager_state.clone();
		if audio_manager_state.borrow().audio_context.is_none() {
			return;
		}
		let is_paused = self.read_muted();
		if is_paused == self.is_context_paused {
			return;
		}
		self.is_context_paused = is_paused;
		if is_paused {
			audio_backend::suspend_audio_context(&audio_manager_state);
		}
		else {
			resume_audio_manager_state(&audio_manager_state);
		}
	}

	//==============================================================================
	/// 페이지 이벤트. (원본 생성자의 visibilitychange, blur, focus, pointerdown 리스너)
	//==============================================================================
	fn handle_page_event(&mut self, page_event: PageEvent) {
		let was_muted = self.read_muted();
		match page_event {
			PageEvent::Hidden => {
				self.is_page_hidden = true;
			}
			PageEvent::Visible => {
				self.is_page_hidden = false;
				self.is_window_blurred = false;
			}
			PageEvent::Blurred => {
				if !self.is_blur_mute_enabled {
					return;
				}
				self.is_window_blurred = true;
			}
			PageEvent::Focused | PageEvent::PointerDown => {
				if !self.is_window_blurred {
					return;
				}
				self.is_window_blurred = false;
			}
		}
		if !was_muted && self.read_muted() {
			self.is_stop_effects_requested = true;
		}
		self.apply_context_pause();
	}
}

//==============================================================================
/// 소리 표로 다루는 배경음악과 효과음. (원본 SoundSystem, 엔진의 AudioManager 를 나눠 들고 페이지 이벤트로 음소거한다)
/// - 매 프레임 update 를 부른다. 버스는 첫 누름 뒤의 update 에서 만든다.
///
/// # 예제
/// 만들고 크기 정하기 (소리 장치는 첫 update 에서 연다)
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, SoundRow, SoundSystem};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let sound_table = vec![
/// 	SoundRow::create_effect("hit", &["tests/assets/audio/wav_s16_stereo.wav"], 0.8),
/// 	SoundRow::create_music("field", "tests/assets/audio/vorbis_stereo.ogg", 0.6),
/// ];
/// let mut sound_system = SoundSystem::new(&engine, sound_table);
/// sound_system.set_music_volume(0.5);
/// sound_system.set_sound_effect_volume(2.0);
/// assert_eq!(sound_system.get_music_volume(), 0.5);
/// assert_eq!(sound_system.get_sound_effect_volume(), 1.0);
/// assert!(sound_system.get_music_bus().is_none());
/// assert_eq!(sound_system.get_music_id(), "");
/// ```
///
/// 씬에서 배경음악을 틀고 누르면 효과음 내기
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, SoundRow, SoundSystem, Vector2};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// 	sound_system: Option<SoundSystem>,
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
/// 	fn create(&mut self, engine: &Engine) {
/// 		let sound_table = vec![
/// 			SoundRow::create_effect("hit", &["tests/assets/audio/wav_s16_stereo.wav"], 0.8),
/// 			SoundRow::create_music("field", "tests/assets/audio/vorbis_stereo.ogg", 0.6),
/// 		];
/// 		let mut sound_system = SoundSystem::new(engine, sound_table);
/// 		sound_system.load_sounds(engine);
/// 		sound_system.play_music("field", 1.0, false);
/// 		self.sound_system = Some(sound_system);
/// 	}
///
/// 	fn tick(&mut self, engine: &Engine, time_delta: f64) {
/// 		self.scene_base.tick(engine, time_delta);
/// 		if let Some(sound_system) = &mut self.sound_system {
/// 			sound_system.update();
/// 		}
/// 	}
///
/// 	fn touch_press(&mut self, _engine: &Engine, _view_input_position: Vector2) {
/// 		if let Some(sound_system) = &mut self.sound_system {
/// 			sound_system.play_effect("hit", 1.0, 0.0);
/// 		}
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let main_scene = MainScene {
/// 	scene_base: SceneBase::new(),
/// 	sound_system: None,
/// };
/// engine.run(Box::new(main_scene)).unwrap();
/// ```
//==============================================================================
pub struct SoundSystem {
	autoplay_allowed_provider: Option<Rc<dyn Fn() -> bool>>,
	audio_manager: AudioManager,
	sound_by_id: HashMap<String, SoundRow>,
	/// 받은 소리. (무리의 차례대로, 못 받은 것은 None)
	asset_by_id: HashMap<String, Vec<Option<Rc<AudioAsset>>>>,
	last_play_time_by_id: HashMap<String, f64>,
	last_variant_by_id: HashMap<String, usize>,
	/// 배경음악과 효과음의 버스. (원본 #musicBus, #effectBus, 첫 누름 전에는 None)
	music_bus: Option<AudioBus>,
	effect_bus: Option<AudioBus>,
	/// 나고 있는 효과음. (원본 #voices)
	voice_list: Vec<EffectVoice>,
	/// 줄여 끄는 중인 효과음. (원본은 목록에서 빼고 소리 장치가 끝까지 줄인 뒤 멈춘다. 러스트 판은 플레이어를 버리면 소리가 멎어 끝날 때까지 든다)
	releasing_voice_list: Vec<EffectVoice>,
	/// 틀어 달라고 한 배경음악. (없으면 "")
	music_id: String,
	music_fade_seconds: f64,
	is_music_resumed: bool,
	playing_music: Option<MusicVoice>,
	/// 줄여 끄는 중인 배경음악. (끝날 때까지 든다)
	fading_music_list: Vec<MusicVoice>,
	music_position_by_id: HashMap<String, f64>,
	music_volume: f64,
	music_scale: f64,
	sound_effect_volume: f64,
	/// 소리의 길에 건 음소거.
	is_muted: bool,
	is_autoplay_tried: bool,
	last_context_resume_time: f64,
	mute_state: Rc<RefCell<SoundMuteState>>,
	/// 엔진이 약한 참조로 들어 SoundSystem 이 사는 동안만 부른다.
	_page_event_listener: PageEventListener,
	clock_origin: Instant,
	/// 시험에서 시계를 앞으로 돌리는 초.
	#[cfg(test)]
	test_time_offset: f64,
}

impl SoundSystem {
	//==============================================================================
	/// 생성. (소리 표를 받는다, 받기는 load_sounds 가 한다, 창이 포커스를 잃으면 음소거할지는 지금의 주된 손으로 정한다)
	/// - 엔진의 AudioManager 를 나눠 들고 페이지 이벤트 리스너를 단다.
	///
	/// # 인자
	/// - `engine`: AudioManager 를 나눠 주고 페이지 이벤트를 보낼 엔진. (페이지가 가려졌는지도 여기서 읽는다)
	/// - `sound_table`: 소리 표. (같은 id 는 뒤의 줄이 이긴다)
	///
	/// # 돌려줌
	/// 새 소리 체계. (아무것도 받지 않았고 버스도 없다, 크기는 모두 1)
	///
	/// # 예제
	/// 빈 표로 만들고 나중에 줄 더하기
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, SoundRow, SoundSystem};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut sound_system = SoundSystem::new(&engine, Vec::new());
	/// sound_system.add_sounds(vec![SoundRow::create_effect("coin", &["tests/assets/audio/wav_s16_stereo.wav"], 1.0)]);
	/// assert_eq!(sound_system.get_effect_voice_count(), 0);
	/// assert!(!sound_system.is_user_muted());
	/// ```
	//==============================================================================
	pub fn new(engine: &Engine, sound_table: Vec<SoundRow>) -> SoundSystem {
		let audio_manager = engine.get_audio_manager().clone();
		let mute_state = Rc::new(RefCell::new(SoundMuteState {
			external_mute_provider: None,
			is_user_muted: false,
			is_page_hidden: engine.is_page_hidden(),
			is_blur_mute_enabled: platform::is_coarse_pointer(),
			is_window_blurred: false,
			is_context_paused: false,
			is_stop_effects_requested: false,
			audio_manager_state: audio_manager.get_shared_state(),
		}));
		let mute_state_for_listener = mute_state.clone();
		let page_event_listener: PageEventListener = Rc::new(RefCell::new(move |page_event: PageEvent| {
			if let Ok(mut mute_state) = mute_state_for_listener.try_borrow_mut() {
				mute_state.handle_page_event(page_event);
			}
		}));
		engine.add_page_event_listener(&page_event_listener);
		let mut sound_system = SoundSystem {
			autoplay_allowed_provider: None,
			audio_manager,
			sound_by_id: HashMap::new(),
			asset_by_id: HashMap::new(),
			last_play_time_by_id: HashMap::new(),
			last_variant_by_id: HashMap::new(),
			music_bus: None,
			effect_bus: None,
			voice_list: Vec::new(),
			releasing_voice_list: Vec::new(),
			music_id: String::new(),
			music_fade_seconds: 0.0,
			is_music_resumed: false,
			playing_music: None,
			fading_music_list: Vec::new(),
			music_position_by_id: HashMap::new(),
			music_volume: 1.0,
			music_scale: 1.0,
			sound_effect_volume: 1.0,
			is_muted: false,
			is_autoplay_tried: false,
			last_context_resume_time: f64::NEG_INFINITY,
			mute_state,
			_page_event_listener: page_event_listener,
			clock_origin: Instant::now(),
			#[cfg(test)]
			test_time_offset: 0.0,
		};
		sound_system.add_sounds(sound_table);
		sound_system
	}

	//==============================================================================
	/// 벽시계. (초, 원본 performance.now() / 1000 자리)
	//==============================================================================
	fn read_now(&self) -> f64 {
		let now = self.clock_origin.elapsed().as_secs_f64();
		#[cfg(test)]
		let now = now + self.test_time_offset;
		now
	}

	//==============================================================================
	/// AudioContext 시계. (초, 원본 audioContext.currentTime)
	//==============================================================================
	fn read_context_time(&self) -> f64 {
		self.audio_manager.get_context_time()
	}

	//==============================================================================
	/// 소리 표의 줄 더하기. (같은 id 는 바꾼다, 받기는 load_sounds 가 한다)
	///
	/// # 인자
	/// - `sound_table`: 더할 줄들. (같은 id 의 줄이 있으면 바꾼다)
	//==============================================================================
	pub fn add_sounds(&mut self, sound_table: Vec<SoundRow>) {
		for sound_row in sound_table {
			self.sound_by_id.insert(sound_row.id.clone(), sound_row);
		}
	}

	//==============================================================================
	/// 바깥의 음소거를 읽는 함수 설정. (플랫폼 SDK 의 음소거, 보상형 광고를 보이는 동안 등, 매 프레임과 페이지 이벤트 때 읽는다, None 이면 없음)
	///
	/// # 인자
	/// - `external_mute_provider`: 음소거여야 하면 true 를 돌려주는 함수. (None 이면 바깥 음소거를 보지 않는다)
	///
	/// # 예제
	/// 광고를 보이는 동안 음소거
	/// ```rust
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	/// use vanilla::{Engine, EngineConfiguration, SoundSystem};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut sound_system = SoundSystem::new(&engine, Vec::new());
	/// let is_advertisement_showing = Rc::new(Cell::new(false));
	/// let is_advertisement_showing_for_provider = is_advertisement_showing.clone();
	/// sound_system.set_external_mute_provider(Some(Rc::new(move || is_advertisement_showing_for_provider.get())));
	/// assert!(!sound_system.read_muted());
	/// is_advertisement_showing.set(true);
	/// assert!(sound_system.read_muted());
	/// ```
	//==============================================================================
	pub fn set_external_mute_provider(&mut self, external_mute_provider: Option<Rc<dyn Fn() -> bool>>) {
		self.mute_state.borrow_mut().external_mute_provider = external_mute_provider;
	}

	//==============================================================================
	/// 바깥의 음소거를 읽는 함수 반환.
	///
	/// # 돌려줌
	/// 설정한 함수. (없으면 None)
	//==============================================================================
	pub fn get_external_mute_provider(&self) -> Option<Rc<dyn Fn() -> bool>> {
		self.mute_state.borrow().external_mute_provider.clone()
	}

	//==============================================================================
	/// 누르기 전에 깨워 봐도 되는지 읽는 함수 설정. (웹뷰가 막는 플랫폼이면 false 를 돌려주는 함수, None 이면 허락, 첫 update 에서 한 번 읽는다)
	///
	/// # 인자
	/// - `autoplay_allowed_provider`: 깨워 봐도 되면 true 를 돌려주는 함수. (None 이면 허락)
	///
	/// # 예제
	/// 누르기 전에는 깨우지 않기 (첫 update 전에 건다)
	/// ```rust
	/// use std::rc::Rc;
	/// use vanilla::{Engine, EngineConfiguration, SoundSystem};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut sound_system = SoundSystem::new(&engine, Vec::new());
	/// sound_system.set_autoplay_allowed_provider(Some(Rc::new(|| false)));
	/// let autoplay_allowed_provider = sound_system.get_autoplay_allowed_provider().unwrap();
	/// assert!(!autoplay_allowed_provider());
	/// ```
	//==============================================================================
	pub fn set_autoplay_allowed_provider(&mut self, autoplay_allowed_provider: Option<Rc<dyn Fn() -> bool>>) {
		self.autoplay_allowed_provider = autoplay_allowed_provider;
	}

	//==============================================================================
	/// 누르기 전에 깨워 봐도 되는지 읽는 함수 반환.
	///
	/// # 돌려줌
	/// 설정한 함수. (없으면 None, 허락으로 친다)
	//==============================================================================
	pub fn get_autoplay_allowed_provider(&self) -> Option<Rc<dyn Fn() -> bool>> {
		self.autoplay_allowed_provider.clone()
	}

	//==============================================================================
	/// 엔진의 AudioManager. (나눠 든 손잡이)
	///
	/// # 돌려줌
	/// 엔진과 같은 AudioContext 를 보는 AudioManager.
	//==============================================================================
	pub fn get_audio_manager(&self) -> &AudioManager {
		&self.audio_manager
	}

	//==============================================================================
	/// 배경음악과 효과음의 버스. (원본 getMusicBus, getEffectBus, 첫 누름 전에는 None)
	///
	/// # 돌려줌
	/// 배경음악 버스. (첫 누름 뒤의 update 전이거나 AudioContext 를 만들지 못했으면 None)
	//==============================================================================
	pub fn get_music_bus(&self) -> Option<&AudioBus> {
		self.music_bus.as_ref()
	}

	//==============================================================================
	/// 효과음 버스. (원본 getEffectBus, 첫 누름 전에는 None)
	///
	/// # 돌려줌
	/// 효과음 버스. (첫 누름 뒤의 update 전이거나 AudioContext 를 만들지 못했으면 None)
	//==============================================================================
	pub fn get_effect_bus(&self) -> Option<&AudioBus> {
		self.effect_bus.as_ref()
	}

	//==============================================================================
	/// 틀어 달라고 한 배경음악. (없으면 "")
	///
	/// # 돌려줌
	/// 틀어 달라고 한 배경음악 id. (없으면 "")
	//==============================================================================
	pub fn get_music_id(&self) -> &str {
		&self.music_id
	}

	//==============================================================================
	/// 설정의 음악 크기.
	///
	/// # 돌려줌
	/// 설정의 음악 크기. (0 ~ 1, 처음은 1)
	//==============================================================================
	pub fn get_music_volume(&self) -> f64 {
		self.music_volume
	}

	//==============================================================================
	/// 음악의 비. (멈춤 창의 반 크기 등)
	///
	/// # 돌려줌
	/// 음악의 비. (0 ~ 1, 처음은 1)
	//==============================================================================
	pub fn get_music_scale(&self) -> f64 {
		self.music_scale
	}

	//==============================================================================
	/// 설정의 효과음 크기.
	///
	/// # 돌려줌
	/// 설정의 효과음 크기. (0 ~ 1, 처음은 1)
	//==============================================================================
	pub fn get_sound_effect_volume(&self) -> f64 {
		self.sound_effect_volume
	}

	//==============================================================================
	/// 사용자의 음소거 단추를 눌러 두었는지.
	///
	/// # 돌려줌
	/// 사용자 음소거를 걸어 두었으면 true.
	//==============================================================================
	pub fn is_user_muted(&self) -> bool {
		self.mute_state.borrow().is_user_muted
	}

	//==============================================================================
	/// 페이지가 가려져 있는지.
	///
	/// # 돌려줌
	/// 페이지가 가려져 있으면 true. (만들 때 엔진의 상태에서 시작해 페이지 이벤트로 바뀐다)
	//==============================================================================
	pub fn is_page_hidden(&self) -> bool {
		self.mute_state.borrow().is_page_hidden
	}

	//==============================================================================
	/// 창이 포커스를 잃으면 음소거하는지. (터치가 주된 기기)
	///
	/// # 돌려줌
	/// 터치가 주된 기기라 포커스를 잃으면 음소거하면 true. (만들 때 정한다)
	//==============================================================================
	pub fn is_blur_mute_enabled(&self) -> bool {
		self.mute_state.borrow().is_blur_mute_enabled
	}

	//==============================================================================
	/// 창이 포커스를 잃었는지. (음소거하는 기기에서만 true)
	///
	/// # 돌려줌
	/// 포커스를 잃어 음소거 중이면 true.
	//==============================================================================
	pub fn is_window_blurred(&self) -> bool {
		self.mute_state.borrow().is_window_blurred
	}

	//==============================================================================
	/// 음소거로 AudioContext 를 멈춰 두었는지.
	///
	/// # 돌려줌
	/// 음소거로 AudioContext 를 멈춰 두었으면 true.
	//==============================================================================
	pub fn is_context_paused(&self) -> bool {
		self.mute_state.borrow().is_context_paused
	}

	//==============================================================================
	/// 마지막으로 다시 깨워 본 때. (초, 아직 없으면 -무한대)
	///
	/// # 돌려줌
	/// 마지막으로 다시 깨워 본 때. (SoundSystem 을 만든 때부터 센 벽시계 초, 아직 없으면 f64::NEG_INFINITY)
	//==============================================================================
	pub fn get_last_context_resume_time(&self) -> f64 {
		self.last_context_resume_time
	}

	//==============================================================================
	/// 지금 음소거여야 하는지. (플랫폼의 음소거, 사용자의 음소거 단추, 페이지가 가려짐, 창이 포커스를 잃음, 설정한 크기보다 먼저이다)
	///
	/// # 돌려줌
	/// 하나라도 맞아 음소거여야 하면 true.
	//==============================================================================
	pub fn read_muted(&self) -> bool {
		self.mute_state.borrow().read_muted()
	}

	//==============================================================================
	/// AudioContext 가 돌고 있는지. (돌지 않으면 소리를 내지 않는다)
	///
	/// # 돌려줌
	/// AudioContext 가 있고 돌고 있으면 true.
	//==============================================================================
	pub fn is_context_running(&self) -> bool {
		self.audio_manager.is_context_running()
	}

	//==============================================================================
	/// 소리 받기. (효과음을 먼저, 배경음악을 나중에, 못 받은 소리는 건너뛴다, 이미 받은 것은 다시 받지 않는다, 애셋 폴더 기준 경로)
	/// - 끝에 update 를 한 번 부른다. (데스크탑은 이때 소리 장치를 연다)
	///
	/// # 인자
	/// - `engine`: 애셋 폴더 기준 경로를 풀 엔진.
	///
	/// # 돌려줌
	/// 걸어 둔 모든 소리 애셋(앞서 받은 것까지)을 기다리는 손잡이. (await 하면 다 받거나 실패할 때까지 기다린다)
	///
	/// # 예제
	/// 받기 전에 틀어 달라고 해 두면 받은 뒤 튼다
	/// ```rust,no_run
	/// use vanilla::{Engine, EngineConfiguration, SoundRow, SoundSystem};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut sound_system = SoundSystem::new(&engine, vec![SoundRow::create_music("field", "tests/assets/audio/vorbis_stereo.ogg", 0.6)]);
	/// sound_system.play_music("field", 0.0, false);
	/// sound_system.load_sounds(&engine);
	/// assert_eq!(sound_system.get_music_id(), "field");
	/// ```
	//==============================================================================
	pub fn load_sounds(&mut self, engine: &Engine) -> SoundLoadHandle {
		let mut effect_row_list = Vec::new();
		let mut music_row_list = Vec::new();
		for sound_row in self.sound_by_id.values() {
			if self.asset_by_id.contains_key(&sound_row.id) {
				continue;
			}
			if sound_row.is_music {
				music_row_list.push(sound_row.clone());
			}
			else {
				effect_row_list.push(sound_row.clone());
			}
		}
		for sound_row in effect_row_list.iter().chain(music_row_list.iter()) {
			self.load_sound(engine, sound_row);
		}
		self.update();
		SoundLoadHandle {
			audio_load_handle_list: self.collect_audio_load_handles(),
		}
	}

	//==============================================================================
	/// 걸어 둔 소리 애셋의 읽기 손잡이. (나눠 든다)
	//==============================================================================
	fn collect_audio_load_handles(&self) -> Vec<AudioLoadHandle> {
		self.asset_by_id.values().flat_map(|asset_list| asset_list.iter().flatten()).map(|audio_asset| audio_asset.get_load_handle()).collect()
	}

	//==============================================================================
	/// 받기를 건 소리가 다 끝나기를 기다리는 퓨처. (원본 await loadSounds() 의 await 자리, load_sounds 뒤에 엔진 작업 안에서 await 한다)
	/// - 부를 때 든 소리 애셋을 나눠 들어 소리 체계를 빌리지 않는다. 못 받은 소리는 건너뛴다. (원본처럼 거부되지 않는다)
	///
	/// # 돌려줌
	/// 부를 때 걸려 있는 소리 애셋을 다 받거나 실패하면 끝나는 퓨처. (걸린 것이 없으면 바로 끝난다)
	///
	/// # 예제
	/// 받기를 건 소리가 없으면 첫 폴링에 끝난다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, SoundRow, SoundSystem};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let sound_system = SoundSystem::new(&engine, vec![SoundRow::create_effect("hit", &["tests/assets/audio/wav_s16_stereo.wav"], 1.0)]);
	/// let sound_load_future = sound_system.wait_for_load();
	/// let task_handle = engine.spawn_task(|_engine_context| async move {
	/// 	sound_load_future.await;
	/// });
	/// assert!(task_handle.is_finished());
	/// ```
	//==============================================================================
	pub fn wait_for_load(&self) -> SoundLoadFuture {
		SoundLoadFuture {
			audio_load_handle_list: self.collect_audio_load_handles(),
		}
	}

	//==============================================================================
	/// 소리 하나 받기. (무리면 모두, 못 받은 것은 None 으로 두고 알린 뒤 건너뛴다)
	/// - 소리 표에는 더하지 않는다. (play_effect, play_music 은 표에 있는 id 만 낸다) 같은 id 로 받아 둔 것은 바꾼다.
	///
	/// # 인자
	/// - `engine`: 애셋 폴더 기준 경로를 풀 엔진.
	/// - `sound_row`: 받을 소리 줄. (paths 를 모두 받는다)
	//==============================================================================
	pub fn load_sound(&mut self, engine: &Engine, sound_row: &SoundRow) {
		let mut asset_list = Vec::new();
		for path in sound_row.paths.iter() {
			match engine.load_audio_asset(path) {
				Ok(audio_asset) => {
					asset_list.push(Some(audio_asset));
				}
				Err(error) => {
					eprintln!("[sound] 소리를 받지 못했습니다. {} ({})", path, error);
					asset_list.push(None);
				}
			}
		}
		self.asset_by_id.insert(sound_row.id.clone(), asset_list);
	}

	//==============================================================================
	/// 소리 애셋 직접 더하기. (메모리에서 지은 소리처럼 파일로 받지 않는 것, 같은 id 의 녹음 무리에 덧붙인다)
	///
	/// # 인자
	/// - `sound_id`: 덧붙일 소리 id. (소리 표에 그 id 의 줄이 있어야 난다)
	/// - `audio_asset`: 더할 소리 애셋. (읽힌 것만 고른다)
	///
	/// # 예제
	/// 메모리의 바이트로 읽은 소리 더하기
	/// ```rust
	/// use std::rc::Rc;
	/// use vanilla::{AudioAsset, Engine, EngineConfiguration, SoundRow, SoundSystem};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut sound_system = SoundSystem::new(&engine, vec![SoundRow::create_effect("chime", &[], 1.0)]);
	/// let audio_asset = Rc::new(AudioAsset::new());
	/// let bytes = std::fs::read("tests/assets/audio/wav_s16_stereo.wav").unwrap();
	/// assert!(audio_asset.load_from_bytes(bytes).get_load_error().is_none());
	/// sound_system.add_sound_asset("chime", audio_asset);
	/// ```
	//==============================================================================
	pub fn add_sound_asset(&mut self, sound_id: &str, audio_asset: Rc<AudioAsset>) {
		self.asset_by_id.entry(String::from(sound_id)).or_default().push(Some(audio_asset));
	}

	//==============================================================================
	/// 설정의 음악 크기. (0 ~ 1)
	///
	/// # 인자
	/// - `music_volume`: 음악 크기. (0 ~ 1 로 자르고, 유한한 수가 아니면 0)
	//==============================================================================
	pub fn set_music_volume(&mut self, music_volume: f64) {
		self.music_volume = clamp_sound_volume(music_volume);
		self.apply_volumes();
	}

	//==============================================================================
	/// 음악의 비. (0 ~ 1, 멈춤 창이 떠 있는 동안 반 크기처럼 설정과 따로 줄일 때)
	///
	/// # 인자
	/// - `music_scale`: 음악 크기에 곱할 비. (0 ~ 1 로 자르고, 유한한 수가 아니면 0)
	///
	/// # 예제
	/// 멈춤 창이 떠 있는 동안 반 크기
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, SoundSystem};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut sound_system = SoundSystem::new(&engine, Vec::new());
	/// sound_system.set_music_volume(0.8);
	/// sound_system.set_music_scale(0.5);
	/// assert_eq!(sound_system.get_music_volume(), 0.8);
	/// assert_eq!(sound_system.get_music_volume() * sound_system.get_music_scale(), 0.4);
	/// ```
	//==============================================================================
	pub fn set_music_scale(&mut self, music_scale: f64) {
		self.music_scale = clamp_sound_volume(music_scale);
		self.apply_volumes();
	}

	//==============================================================================
	/// 설정의 효과음 크기. (0 ~ 1)
	///
	/// # 인자
	/// - `sound_effect_volume`: 효과음 크기. (0 ~ 1 로 자르고, 유한한 수가 아니면 0)
	//==============================================================================
	pub fn set_sound_effect_volume(&mut self, sound_effect_volume: f64) {
		self.sound_effect_volume = clamp_sound_volume(sound_effect_volume);
		self.apply_volumes();
	}

	//==============================================================================
	/// 사용자의 음소거 단추. (음악과 효과음을 한꺼번에)
	///
	/// # 인자
	/// - `is_user_muted`: true 면 음소거, false 면 풀기. (다른 음소거가 남아 있으면 그대로 음소거)
	//==============================================================================
	pub fn set_user_muted(&mut self, is_user_muted: bool) {
		self.mute_state.borrow_mut().is_user_muted = is_user_muted;
		self.apply_mute();
		self.apply_context_pause();
	}

	//==============================================================================
	/// 배경음악 틀기. (반복한다, "" 이면 끈다, 받기 전이거나 첫 누름 전이면 그때 튼다)
	/// - fade_seconds 동안 앞 음악은 줄고 새 음악은 0 에서 오른다. (0 이면 바로)
	/// - is_resumed 면 그 음악을 멈췄던 자리에서 잇는다. (아니면 처음부터)
	///
	/// # 인자
	/// - `sound_id`: 틀 배경음악 id. ("" 이면 끈다, 지금 틀어 달라고 한 것과 같으면 아무것도 하지 않는다)
	/// - `fade_seconds`: 겹쳐 바꾸는 시간. (초, 0 이면 바로, 음수는 0)
	/// - `is_resumed`: true 면 그 음악을 멈췄던 자리에서 잇는다. (멈춘 자리는 소리마다 기억한다)
	///
	/// # 예제
	/// 전투에 들어갈 때 1 초 겹쳐 바꾸고, 나올 때 필드 음악을 멈췄던 자리에서 잇기
	/// ```rust,no_run
	/// use vanilla::SoundSystem;
	///
	/// fn enter_battle(sound_system: &mut SoundSystem) {
	/// 	sound_system.play_music("battle", 1.0, false);
	/// }
	///
	/// fn leave_battle(sound_system: &mut SoundSystem) {
	/// 	sound_system.play_music("field", 1.0, true);
	/// }
	/// ```
	//==============================================================================
	pub fn play_music(&mut self, sound_id: &str, fade_seconds: f64, is_resumed: bool) {
		if sound_id == self.music_id {
			return;
		}
		self.music_id = String::from(sound_id);
		self.music_fade_seconds = fade_seconds.max(0.0);
		self.is_music_resumed = is_resumed;
		self.update();
	}

	//==============================================================================
	/// 효과음 내기. (음소거, 첫 누름 전, 받기 전, 다시 낼 간격 안이면 내지 않는다)
	/// - volume: 이번의 크기. (0 ~ 1, 소리의 크기와 설정의 효과음 크기에 곱한다)  pan: 좌우. (-1 왼쪽 ~ 1 오른쪽, 0 가운데)
	///
	/// # 인자
	/// - `sound_id`: 낼 효과음 id. (소리 표에 있고 받아 둔 것)
	/// - `volume`: 이번의 크기. (0 ~ 1, 소리의 크기와 곱해 0 ~ 1 로 자르고 0.001 이하면 내지 않는다)
	/// - `pan`: 좌우. (-1 왼쪽 ~ 1 오른쪽, 0 가운데, 범위 밖은 자르고 유한한 수가 아니면 0)
	///
	/// # 예제
	/// 화면 왼쪽에서 맞은 소리를 반 크기로
	/// ```rust,no_run
	/// use vanilla::SoundSystem;
	///
	/// fn play_hit(sound_system: &mut SoundSystem) {
	/// 	sound_system.play_effect("hit", 0.5, -0.8);
	/// }
	/// ```
	//==============================================================================
	pub fn play_effect(&mut self, sound_id: &str, volume: f64, pan: f64) {
		let effect_bus = match &self.effect_bus {
			Some(effect_bus) => {
				effect_bus.clone()
			}
			None => {
				return;
			}
		};
		if self.is_muted || !self.is_context_running() {
			return;
		}
		let sound_row = match self.sound_by_id.get(sound_id) {
			Some(sound_row) => {
				sound_row.clone()
			}
			None => {
				return;
			}
		};
		let asset_list = match self.asset_by_id.get(sound_id) {
			Some(asset_list) => {
				asset_list.clone()
			}
			None => {
				return;
			}
		};
		let effect_volume = clamp_sound_volume(sound_row.volume * volume);
		if effect_volume <= MINIMUM_EFFECT_VOLUME {
			return;
		}
		let now = self.read_now();
		let last_play_time = self.last_play_time_by_id.get(sound_id).copied().unwrap_or(f64::NEG_INFINITY);
		if now - last_play_time < sound_row.minimum_interval {
			return;
		}
		let audio_asset = match self.pick_variant(sound_id, &asset_list) {
			Some(audio_asset) => {
				audio_asset
			}
			None => {
				return;
			}
		};
		self.last_play_time_by_id.insert(String::from(sound_id), now);
		// 같은 소리가 maximum_voices 를 넘거나 효과음 전체가 EFFECT_VOICE_COUNT 를 넘으면 가장 오래된 것을 끊는다.
		let maximum_voices = sound_row.maximum_voices.unwrap_or(EFFECT_VOICE_COUNT);
		self.trim_voices(sound_id, maximum_voices.saturating_sub(1));
		self.trim_voices("", EFFECT_VOICE_COUNT - 1);
		let mut audio_player = effect_bus.create_audio_player();
		audio_player.set_audio_asset(audio_asset);
		if sound_row.pitch_variation > 0.0 {
			let pitch_ratio = 1.0 + (math::random() * 2.0 - 1.0) * sound_row.pitch_variation;
			audio_player.set_playback_rate(pitch_ratio);
		}
		let clamped_pan = if pan.is_finite() {
			pan.clamp(-MAXIMUM_PAN, MAXIMUM_PAN)
		}
		else {
			0.0
		};
		if clamped_pan != 0.0 {
			audio_player.set_pan(clamped_pan);
		}
		// 원본 gainNode.gain.value = effectVolume, source.start(0).
		audio_player.set_volume(effect_volume);
		audio_player.play(false);
		// 앞만 틀고 줄여 끄는 소리. (원본 setValueAtTime, linearRampToValueAtTime, source.stop(when))
		if sound_row.play_seconds > 0.0 {
			let fade_out_seconds = sound_row.fade_out_seconds.max(0.0);
			let fade_start_time = self.read_context_time() + sound_row.play_seconds;
			audio_player.apply_gain_automation(GainAutomation::SetValueAtTime { value: effect_volume as f32, time: fade_start_time });
			audio_player.apply_gain_automation(GainAutomation::LinearRampToValueAtTime { value: 0.0, time: fade_start_time + fade_out_seconds });
			audio_player.stop_at_time(fade_start_time + fade_out_seconds + EFFECT_STOP_MARGIN_SECONDS);
		}
		self.voice_list.push(EffectVoice {
			sound_id: String::from(sound_id),
			audio_player,
		});
	}

	//==============================================================================
	/// 무리에서 하나 고르기. (받은 것 가운데 바로 앞과 다른 것, 하나뿐이면 그것, 없으면 None)
	//==============================================================================
	fn pick_variant(&mut self, sound_id: &str, asset_list: &[Option<Rc<AudioAsset>>]) -> Option<Rc<AudioAsset>> {
		let loaded_index_list: Vec<usize> = asset_list
			.iter()
			.enumerate()
			.filter(|(_, audio_asset)| audio_asset.as_ref().map(|audio_asset| audio_asset.is_loaded()).unwrap_or(false))
			.map(|(asset_index, _)| asset_index)
			.collect();
		if loaded_index_list.is_empty() {
			return None;
		}
		let last_index = self.last_variant_by_id.get(sound_id).copied();
		let candidate_index_list: Vec<usize> = if loaded_index_list.len() > 1 {
			loaded_index_list.iter().copied().filter(|asset_index| Some(*asset_index) != last_index).collect()
		}
		else {
			loaded_index_list
		};
		let candidate_count = candidate_index_list.len();
		let picked_index = candidate_index_list[((math::random() * candidate_count as f64) as usize).min(candidate_count - 1)];
		self.last_variant_by_id.insert(String::from(sound_id), picked_index);
		asset_list[picked_index].clone()
	}

	//==============================================================================
	/// 나는 효과음을 그 수까지 줄이기. (오래된 것부터 끊음, sound_id 가 "" 이면 모든 효과음)
	//==============================================================================
	fn trim_voices(&mut self, sound_id: &str, keep_count: usize) {
		let matching_count = self.voice_list.iter().filter(|voice| sound_id.is_empty() || voice.sound_id == sound_id).count();
		let mut remove_count = matching_count.saturating_sub(keep_count);
		self.voice_list.retain_mut(|voice| {
			if remove_count == 0 || !(sound_id.is_empty() || voice.sound_id == sound_id) {
				return true;
			}
			voice.audio_player.stop();
			remove_count -= 1;
			false
		});
	}

	//==============================================================================
	/// 나는 효과음을 줄여 끄기. (sound_id 가 "" 이면 모든 효과음, 줄이는 초가 0 이면 바로 끊음)
	/// - 원본처럼 지금 크기에서 0 으로 줄이고 목록에서 뺀다. (앞만 트는 소리가 줄어드는 중이어도 지금 크기에서 새로 줄인다)
	///
	/// # 인자
	/// - `sound_id`: 줄여 끌 효과음 id. ("" 이면 모든 효과음)
	/// - `fade_seconds`: 줄이는 시간. (초, 0 이하이거나 AudioContext 가 없으면 바로 끊는다)
	///
	/// # 예제
	/// 장면이 바뀔 때 모든 효과음을 0.3 초 동안 줄여 끄기
	/// ```rust,no_run
	/// use vanilla::SoundSystem;
	///
	/// fn leave_stage(sound_system: &mut SoundSystem) {
	/// 	sound_system.fade_out_effects("", 0.3);
	/// 	assert_eq!(sound_system.get_effect_voice_count(), 0);
	/// }
	/// ```
	//==============================================================================
	pub fn fade_out_effects(&mut self, sound_id: &str, fade_seconds: f64) {
		let clamped_fade_seconds = fade_seconds.max(0.0);
		let has_audio_context = self.audio_manager.has_audio_context();
		let context_time = self.read_context_time();
		let mut voice_index = 0;
		while voice_index < self.voice_list.len() {
			let is_matching = sound_id.is_empty() || self.voice_list[voice_index].sound_id == sound_id;
			if !is_matching {
				voice_index += 1;
				continue;
			}
			let mut voice = self.voice_list.remove(voice_index);
			if clamped_fade_seconds <= 0.0 || !has_audio_context {
				voice.audio_player.stop();
				continue;
			}
			// 원본: 지금 gain.value 에서 cancelScheduledValues, setValueAtTime, linearRampToValueAtTime(0), source.stop(when).
			let current_gain = voice.audio_player.get_gain_value();
			voice.audio_player.apply_gain_automation(GainAutomation::CancelScheduledValues { time: context_time });
			voice.audio_player.apply_gain_automation(GainAutomation::SetValueAtTime { value: current_gain as f32, time: context_time });
			voice.audio_player.apply_gain_automation(GainAutomation::LinearRampToValueAtTime { value: 0.0, time: context_time + clamped_fade_seconds });
			voice.audio_player.stop_at_time(context_time + clamped_fade_seconds + EFFECT_STOP_MARGIN_SECONDS);
			self.releasing_voice_list.push(voice);
		}
	}

	//==============================================================================
	/// 나는 효과음을 모두 끊기. (멈춤 창이 뜰 때 등)
	//==============================================================================
	pub fn stop_effects(&mut self) {
		for voice in self.voice_list.iter_mut() {
			voice.audio_player.stop();
		}
		self.voice_list.clear();
	}

	//==============================================================================
	/// 나는 효과음 수. (줄여 끄는 중인 것은 빼고)
	///
	/// # 돌려줌
	/// 나고 있는 효과음 수. (끝난 것은 update 가 목록에서 뺀다)
	//==============================================================================
	pub fn get_effect_voice_count(&self) -> usize {
		self.voice_list.len()
	}

	//==============================================================================
	/// 매 프레임. (첫 누름 뒤 소리의 길 만들기, 플랫폼의 음소거 따르기, 배경음악 틀기, 크기 걸기)
	//==============================================================================
	pub fn update(&mut self) {
		self.try_autoplay();
		self.ensure_players();
		let is_stop_effects_requested = std::mem::take(&mut self.mute_state.borrow_mut().is_stop_effects_requested);
		if is_stop_effects_requested {
			self.stop_effects();
		}
		self.apply_mute();
		self.apply_context_pause();
		self.retry_context_resume();
		self.update_music();
		self.update_voices();
	}

	//==============================================================================
	/// 다시 깨워 보기. (음소거가 아닌데 AudioContext 가 돌지 않으면 CONTEXT_RESUME_RETRY_INTERVAL 마다)
	/// - 아이폰은 앱으로 돌아온 순간(다시 보임)에 부른 resume() 을 오디오가 아직 돌아오기 전이라 듣지 않는 일이 있다.
	//==============================================================================
	fn retry_context_resume(&mut self) {
		let is_context_paused = self.is_context_paused();
		if !self.audio_manager.has_audio_context() || is_context_paused || self.audio_manager.is_context_running() {
			return;
		}
		let now = self.read_now();
		if now - self.last_context_resume_time < CONTEXT_RESUME_RETRY_INTERVAL {
			return;
		}
		self.last_context_resume_time = now;
		self.audio_manager.resume_context();
	}

	//==============================================================================
	/// 누르기 전에 깨워 보기. (허락할 때 처음 한 번, 웹뷰가 막으면 AudioContext 가 멈춘 채로 남고 첫 누름에 엔진이 깨운다)
	//==============================================================================
	fn try_autoplay(&mut self) {
		if self.is_autoplay_tried {
			return;
		}
		self.is_autoplay_tried = true;
		if let Some(autoplay_allowed_provider) = &self.autoplay_allowed_provider {
			if !autoplay_allowed_provider() {
				return;
			}
		}
		self.audio_manager.mark_user_gesture();
		self.audio_manager.resume_context();
	}

	//==============================================================================
	/// 소리의 길 만들기. (첫 누름 뒤 한 번, AudioContext 를 그 전에 만들지 않게, 버스를 만들 때 AudioContext 도 만든다)
	//==============================================================================
	fn ensure_players(&mut self) {
		if self.effect_bus.is_some() {
			return;
		}
		if !self.audio_manager.has_user_gesture() {
			return;
		}
		let music_bus = self.audio_manager.create_audio_bus();
		if !self.audio_manager.has_audio_context() {
			return;
		}
		self.music_bus = Some(music_bus);
		self.effect_bus = Some(self.audio_manager.create_audio_bus());
		self.is_muted = false;
		self.apply_volumes();
	}

	//==============================================================================
	/// 크기 적용. (음소거면 0, 원본처럼 버스 gain 에 setTargetAtTime(크기, 지금, 0.05))
	//==============================================================================
	fn apply_volumes(&mut self) {
		let (Some(music_bus), Some(effect_bus)) = (&self.music_bus, &self.effect_bus) else {
			return;
		};
		let mute_scale = if self.is_muted { 0.0 } else { 1.0 };
		let context_time = self.read_context_time();
		music_bus.apply_gain_automation(GainAutomation::SetTargetAtTime { target: (self.music_volume * self.music_scale * mute_scale) as f32, time: context_time, time_constant: VOLUME_SMOOTHING_SECONDS });
		effect_bus.apply_gain_automation(GainAutomation::SetTargetAtTime { target: (self.sound_effect_volume * mute_scale) as f32, time: context_time, time_constant: VOLUME_SMOOTHING_SECONDS });
	}

	//==============================================================================
	/// 음소거 적용. (바뀔 때만, 음소거가 되면 나는 효과음을 끊음)
	//==============================================================================
	fn apply_mute(&mut self) {
		if self.effect_bus.is_none() {
			return;
		}
		let is_muted = self.read_muted();
		if is_muted == self.is_muted {
			return;
		}
		self.is_muted = is_muted;
		if is_muted {
			self.stop_effects();
		}
		self.apply_volumes();
	}

	//==============================================================================
	/// 음소거인 동안 AudioContext 멈추기와 풀리면 깨우기. (바뀔 때만, 첫 누름 전에는 AudioContext 가 없어 하지 않는다)
	//==============================================================================
	fn apply_context_pause(&mut self) {
		self.mute_state.borrow_mut().apply_context_pause();
	}

	//==============================================================================
	/// 배경음악 틀기와 바꾸기. (AudioContext 가 돌고 받아 둔 것만, 음소거 중에도 소리 없이 돌아 풀면 이어 들린다)
	/// - 바꾸라고 하면 앞 음악은 곧바로 줄이기 시작하고(멈춘 자리를 기억), 새 음악은 받아 두었으면 0 에서 올린다.
	//==============================================================================
	fn update_music(&mut self) {
		let music_bus = match &self.music_bus {
			Some(music_bus) => {
				music_bus.clone()
			}
			None => {
				return;
			}
		};
		if !self.is_context_running() {
			return;
		}
		if let Some(playing_music) = &self.playing_music {
			if playing_music.sound_id == self.music_id {
				return;
			}
		}
		let fade_seconds = self.music_fade_seconds;
		if let Some(playing_music) = self.playing_music.take() {
			self.fade_out_music(playing_music, fade_seconds);
		}
		if self.music_id.is_empty() {
			return;
		}
		let sound_row = match self.sound_by_id.get(&self.music_id) {
			Some(sound_row) => {
				sound_row.clone()
			}
			None => {
				return;
			}
		};
		// 첫 파일만 튼다. (받지 못했으면 틀지 않는다)
		let audio_asset = match self.asset_by_id.get(&self.music_id).and_then(|asset_list| asset_list.first()) {
			Some(Some(audio_asset)) if audio_asset.is_loaded() => {
				audio_asset.clone()
			}
			_ => {
				return;
			}
		};
		let context_time = self.read_context_time();
		let duration = audio_asset.get_duration();
		let stored_position = self.music_position_by_id.get(&self.music_id).copied().unwrap_or(0.0);
		let offset = if self.is_music_resumed && duration > 0.0 {
			stored_position % duration
		}
		else {
			0.0
		};
		let row_volume = clamp_sound_volume(sound_row.volume);
		let mut audio_player = music_bus.create_audio_player();
		audio_player.set_audio_asset(audio_asset);
		audio_player.set_time(offset);
		// 원본: 겹쳐 바꾸면 setValueAtTime(0, 지금), linearRampToValueAtTime(크기, 지금 + 초), 아니면 gain.value = 크기.
		if fade_seconds > 0.0 {
			audio_player.apply_gain_automation(GainAutomation::SetValueAtTime { value: 0.0, time: context_time });
			audio_player.apply_gain_automation(GainAutomation::LinearRampToValueAtTime { value: row_volume as f32, time: context_time + fade_seconds });
		}
		else {
			audio_player.set_volume(row_volume);
		}
		audio_player.play(true);
		self.playing_music = Some(MusicVoice {
			sound_id: self.music_id.clone(),
			audio_player,
			start_time: context_time,
			offset,
			duration,
		});
	}

	//==============================================================================
	/// 틀던 배경음악을 줄여 끄기. (fade_seconds 동안, 다 줄어든 때의 자리를 기억)
	//==============================================================================
	fn fade_out_music(&mut self, mut playing_music: MusicVoice, fade_seconds: f64) {
		let context_time = self.read_context_time();
		let stop_time = context_time + fade_seconds;
		if playing_music.duration > 0.0 {
			let stop_position = (playing_music.offset + stop_time - playing_music.start_time) % playing_music.duration;
			self.music_position_by_id.insert(playing_music.sound_id.clone(), stop_position);
		}
		// 원본: 지금 gain.value 에서 cancelScheduledValues, setValueAtTime, 줄이면 linearRampToValueAtTime(0) 아니면 setValueAtTime(0), source.stop(when).
		let audio_player = &mut playing_music.audio_player;
		let current_gain = audio_player.get_gain_value();
		audio_player.apply_gain_automation(GainAutomation::CancelScheduledValues { time: context_time });
		audio_player.apply_gain_automation(GainAutomation::SetValueAtTime { value: current_gain as f32, time: context_time });
		if fade_seconds > 0.0 {
			audio_player.apply_gain_automation(GainAutomation::LinearRampToValueAtTime { value: 0.0, time: stop_time });
		}
		else {
			audio_player.apply_gain_automation(GainAutomation::SetValueAtTime { value: 0.0, time: context_time });
		}
		audio_player.stop_at_time(stop_time + VOLUME_SMOOTHING_SECONDS);
		self.fading_music_list.push(playing_music);
	}

	//==============================================================================
	/// 끝난 소리 치우기. (원본 onended 가 목록에서 빼는 자리, 크기와 멈춤은 소리 장치가 자동화대로 한다)
	//==============================================================================
	fn update_voices(&mut self) {
		self.voice_list.retain(|voice| voice.audio_player.is_playing());
		self.releasing_voice_list.retain(|voice| voice.audio_player.is_playing());
		self.fading_music_list.retain(|music_voice| music_voice.audio_player.is_playing());
	}
}

//==============================================================================
/// 0 ~ 1 로 다듬기. (유한한 수가 아니면 0, 원본 clampVolume)
//==============================================================================
fn clamp_sound_volume(volume: f64) -> f64 {
	if !volume.is_finite() {
		return 0.0;
	}
	volume.clamp(0.0, 1.0)
}

//==============================================================================
/// 함께 쓰는 소리 정하기. (None 이면 비운다)
///
/// # 인자
/// - `sound_system`: 함께 쓸 소리 체계. (None 이면 비운다, 스레드마다 따로 든다)
///
/// # 예제
/// 정해 두고 다른 곳에서 읽어 쓰기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{read_sound_system, set_sound_system, Engine, EngineConfiguration, SoundSystem};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let sound_system = Rc::new(RefCell::new(SoundSystem::new(&engine, Vec::new())));
/// set_sound_system(Some(sound_system.clone()));
/// if let Some(shared_sound_system) = read_sound_system() {
/// 	shared_sound_system.borrow_mut().set_sound_effect_volume(0.5);
/// }
/// assert_eq!(sound_system.borrow().get_sound_effect_volume(), 0.5);
/// set_sound_system(None);
/// assert!(read_sound_system().is_none());
/// ```
//==============================================================================
pub fn set_sound_system(sound_system: Option<Rc<RefCell<SoundSystem>>>) {
	CURRENT_SOUND_SYSTEM.with(|current_sound_system| {
		*current_sound_system.borrow_mut() = sound_system;
	});
}

//==============================================================================
/// 함께 쓰는 소리. (아직 없으면 None)
///
/// # 돌려줌
/// 이 스레드에 set_sound_system 으로 정한 소리 체계. (정하지 않았거나 비웠으면 None)
//==============================================================================
pub fn read_sound_system() -> Option<Rc<RefCell<SoundSystem>>> {
	CURRENT_SOUND_SYSTEM.with(|current_sound_system| current_sound_system.borrow().clone())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;
	use std::cell::Cell;

	//==============================================================================
	/// 무음 WAV. (16 비트 모노 8000 Hz, sample_count 개)
	//==============================================================================
	fn create_silent_wav(sample_count: u32) -> Vec<u8> {
		let data_size = sample_count * 2;
		let mut bytes = Vec::new();
		bytes.extend_from_slice(b"RIFF");
		bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
		bytes.extend_from_slice(b"WAVEfmt ");
		bytes.extend_from_slice(&16u32.to_le_bytes());
		bytes.extend_from_slice(&1u16.to_le_bytes());
		bytes.extend_from_slice(&1u16.to_le_bytes());
		bytes.extend_from_slice(&8000u32.to_le_bytes());
		bytes.extend_from_slice(&16000u32.to_le_bytes());
		bytes.extend_from_slice(&2u16.to_le_bytes());
		bytes.extend_from_slice(&16u16.to_le_bytes());
		bytes.extend_from_slice(b"data");
		bytes.extend_from_slice(&data_size.to_le_bytes());
		bytes.extend(std::iter::repeat_n(0u8, data_size as usize));
		bytes
	}

	//==============================================================================
	/// 소리 장치 시각이 그 시각을 넘을 때까지 기다리기. (3 초까지)
	/// - 가상 소리 장치(CI 의 PulseAudio 같은)는 시작이 늦거나 버퍼 단위로 흘러, 벽시계로 정한 만큼 자면 소리 시각이 모자랄 수 있다.
	//==============================================================================
	fn wait_for_context_time(sound_system: &SoundSystem, context_time: f64) {
		let start_instant = std::time::Instant::now();
		while sound_system.read_context_time() < context_time && start_instant.elapsed() < std::time::Duration::from_secs(3) {
			std::thread::sleep(std::time::Duration::from_millis(10));
		}
	}

	//==============================================================================
	/// 읽힌 무음 소리 애셋. (seconds 초)
	//==============================================================================
	fn create_silent_asset(seconds: f64) -> Rc<AudioAsset> {
		let audio_asset = Rc::new(AudioAsset::new());
		assert!(audio_asset.load_from_bytes(create_silent_wav((seconds * 8000.0) as u32)).get_load_error().is_none());
		assert!(audio_asset.is_loaded());
		audio_asset
	}

	//==============================================================================
	/// 같은 소리 간격과 수 제한. (간격 안에서는 내지 않고, maximum_voices 를 넘으면 오래된 것을 끊는다, 0 이면 하나만)
	//==============================================================================
	#[test]
	fn interval_and_voice_limit() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut hit_row = SoundRow::create_effect("hit", &[], 1.0);
		hit_row.minimum_interval = 0.1;
		hit_row.maximum_voices = Some(2);
		let mut sound_system = SoundSystem::new(&engine, vec![hit_row]);
		sound_system.add_sound_asset("hit", create_silent_asset(1.0));
		sound_system.update();
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_effect("hit", 1.0, 0.0);
		sound_system.play_effect("hit", 1.0, 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), 1);
		for _ in 0..4 {
			sound_system.test_time_offset += 0.11;
			sound_system.play_effect("hit", 1.0, -0.5);
		}
		assert_eq!(sound_system.get_effect_voice_count(), 2);

		// 너무 작은 크기는 내지 않고, "" 와 0 초면 모두 바로 끊는다.
		sound_system.test_time_offset += 0.2;
		let voice_count = sound_system.get_effect_voice_count();
		sound_system.play_effect("hit", 0.0005, 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), voice_count);
		sound_system.fade_out_effects("", 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), 0);

		// 0 이면 원본처럼 앞의 것을 모두 끊어 하나만 난다. (keepCount 가 -1 이라 0 으로 친다)
		let mut single_row = SoundRow::create_effect("single", &[], 1.0);
		single_row.maximum_voices = Some(0);
		sound_system.add_sounds(vec![single_row]);
		sound_system.add_sound_asset("single", create_silent_asset(1.0));
		sound_system.play_effect("single", 1.0, 0.0);
		sound_system.play_effect("single", 1.0, 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), 1);
	}

	//==============================================================================
	/// 줄여 끄기: 목록에서 빠지고 지금 gain.value 에서 0 으로 줄인 뒤 멈춘다. (자동화와 stop(when)은 소리 장치가 한다, 셈은 platform/audio 시험이 크롬과 견준다)
	/// - 줄여 끄는 소리는 stop_effects 로도 끊기지 않고, 멈춘 뒤 update 가 치운다.
	//==============================================================================
	#[test]
	fn fade_out_moves_voice_to_release_until_stopped() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut short_row = SoundRow::create_effect("short", &[], 1.0);
		short_row.play_seconds = 1.0;
		short_row.fade_out_seconds = 1.0;
		let mut sound_system = SoundSystem::new(&engine, vec![short_row]);
		sound_system.add_sound_asset("short", create_silent_asset(5.0));
		sound_system.update();
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_effect("short", 0.8, 0.0);
		// 이득은 WebAudio AudioParam 의 float 칸이라 0.8 의 f32 값을 읽는다. (원본 gain.value 도 같다)
		assert_eq!(sound_system.voice_list[0].audio_player.get_gain_value(), 0.8f32 as f64);
		let fade_start_time = sound_system.read_context_time();
		sound_system.fade_out_effects("short", 0.05);
		assert_eq!(sound_system.get_effect_voice_count(), 0);
		assert_eq!(sound_system.releasing_voice_list.len(), 1);
		sound_system.stop_effects();
		assert_eq!(sound_system.releasing_voice_list.len(), 1);
		wait_for_context_time(&sound_system, fade_start_time + 0.15);
		sound_system.update();
		assert!(sound_system.releasing_voice_list.is_empty());
	}

	//==============================================================================
	/// 누르기 전에 깨워 보기는 첫 update 에서 한 번만 허락을 묻는다.
	//==============================================================================
	#[test]
	fn autoplay_provider_is_asked_once() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(&engine, Vec::new());
		let asked_count = Rc::new(Cell::new(0));
		let asked_count_for_provider = asked_count.clone();
		sound_system.set_autoplay_allowed_provider(Some(Rc::new(move || {
			asked_count_for_provider.set(asked_count_for_provider.get() + 1);
			false
		})));
		assert!(sound_system.get_autoplay_allowed_provider().is_some());
		sound_system.update();
		sound_system.update();
		assert_eq!(asked_count.get(), 1);
	}

	//==============================================================================
	/// 페이지 이벤트: 가려지면 음소거, 터치가 주된 기기에서 포커스를 잃으면 음소거, 누르거나 포커스를 얻거나 다시 보이면 풀린다.
	/// - 마우스가 주된 기기(데스크탑)는 포커스를 잃어도 음소거하지 않는다.
	//==============================================================================
	#[test]
	fn page_events_mute_and_unmute() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(&engine, Vec::new());
		sound_system.update();
		assert!(!sound_system.is_blur_mute_enabled());
		engine.dispatch_page_event(PageEvent::Blurred);
		assert!(!sound_system.is_window_blurred());
		assert!(!sound_system.read_muted());

		// 가려지면 AudioContext 를 멈추고(원본 suspend, 데스크탑도 소리와 시계가 선다) 다시 보이면 깨운다.
		let has_audio_context = sound_system.get_audio_manager().has_audio_context();
		engine.dispatch_page_event(PageEvent::Hidden);
		assert!(sound_system.is_page_hidden());
		assert!(sound_system.read_muted());
		if has_audio_context {
			assert!(sound_system.is_context_paused());
			assert!(!sound_system.is_context_running());
			let paused_context_time = sound_system.get_audio_manager().get_context_time();
			std::thread::sleep(std::time::Duration::from_millis(30));
			assert_eq!(sound_system.get_audio_manager().get_context_time(), paused_context_time);
		}
		engine.dispatch_page_event(PageEvent::Visible);
		assert!(!sound_system.read_muted());
		assert!(!sound_system.is_context_paused());
		assert_eq!(sound_system.is_context_running(), has_audio_context);

		sound_system.mute_state.borrow_mut().is_blur_mute_enabled = true;
		engine.dispatch_page_event(PageEvent::Blurred);
		assert!(sound_system.is_window_blurred());
		assert!(sound_system.read_muted());
		engine.dispatch_page_event(PageEvent::PointerDown);
		assert!(!sound_system.is_window_blurred());
		engine.dispatch_page_event(PageEvent::Blurred);
		engine.dispatch_page_event(PageEvent::Focused);
		assert!(!sound_system.read_muted());
		engine.dispatch_page_event(PageEvent::Blurred);
		engine.dispatch_page_event(PageEvent::Visible);
		assert!(!sound_system.is_window_blurred());

		// 사용자 음소거와 바깥 음소거도 같은 판정에 든다.
		sound_system.set_user_muted(true);
		assert!(sound_system.read_muted());
		sound_system.set_user_muted(false);
		sound_system.set_external_mute_provider(Some(Rc::new(|| true)));
		assert!(sound_system.read_muted());
		assert!(sound_system.get_external_mute_provider().is_some());
	}

	//==============================================================================
	/// 가려졌다 다시 보이는 사이에 update 가 없어도 내던 효과음은 끊긴다. (원본은 가려지는 순간 끊는다)
	//==============================================================================
	#[test]
	fn hidden_page_stops_effects() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(&engine, vec![SoundRow::create_effect("hit", &[], 1.0)]);
		sound_system.add_sound_asset("hit", create_silent_asset(1.0));
		sound_system.update();
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_effect("hit", 1.0, 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), 1);
		engine.dispatch_page_event(PageEvent::Hidden);
		engine.dispatch_page_event(PageEvent::Visible);
		sound_system.update();
		assert_eq!(sound_system.get_effect_voice_count(), 0);
	}

	//==============================================================================
	/// 배경음악: 받기 전에 틀어 달라고 하면 기억했다가 받으면 튼다, "" 면 끈다, 멈췄던 자리에서 잇는다, 첫 파일을 못 받았으면 틀지 않는다.
	//==============================================================================
	#[test]
	fn music_request_waits_and_resumes() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(&engine, vec![SoundRow::create_music("field", "field.wav", 0.8), SoundRow::create_music("battle", "battle.wav", 1.0), SoundRow::create_music("broken", "broken.wav", 1.0)]);
		sound_system.update();
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_music("field", 0.0, false);
		assert_eq!(sound_system.get_music_id(), "field");
		assert!(sound_system.playing_music.is_none());
		sound_system.add_sound_asset("field", create_silent_asset(4.0));
		sound_system.add_sound_asset("battle", create_silent_asset(4.0));
		sound_system.update();
		assert_eq!(sound_system.playing_music.as_ref().map(|music_voice| music_voice.sound_id.as_str()), Some("field"));

		// 0.1 초 겹쳐 바꾸면, 앞 음악은 다 줄어든 때의 자리를 기억한다. (튼 뒤 흐른 시간 + 0.1 초) 다 줄어 멈추면 치운다.
		let start_time = sound_system.playing_music.as_ref().map(|music_voice| music_voice.start_time).unwrap_or(0.0);
		std::thread::sleep(std::time::Duration::from_millis(50));
		// 소리 장치 시각은 버퍼 단위로 뛰어, 부르기 앞뒤에 읽은 시각 사이에 드는지로 본다. (느린 기계에서 뒤에 한 번 읽은 값과 견주면 어긋난다)
		let elapsed_seconds_before = sound_system.read_context_time() - start_time;
		sound_system.play_music("battle", 0.1, false);
		let elapsed_seconds_after = sound_system.read_context_time() - start_time;
		let stored_position = sound_system.music_position_by_id.get("field").copied().unwrap_or(-1.0);
		assert!(stored_position >= elapsed_seconds_before + 0.1 - 1e-6 && stored_position <= elapsed_seconds_after + 0.1 + 1e-6, "기억한 자리 {} / 흐른 시간 {} ~ {} + 0.1", stored_position, elapsed_seconds_before, elapsed_seconds_after);
		assert_eq!(sound_system.fading_music_list.len(), 1);
		wait_for_context_time(&sound_system, start_time + elapsed_seconds_after + 0.2);
		sound_system.update();
		assert!(sound_system.fading_music_list.is_empty());
		assert_eq!(sound_system.playing_music.as_ref().map(|music_voice| music_voice.sound_id.as_str()), Some("battle"));

		// 이어 틀기는 기억한 자리에서.
		sound_system.play_music("field", 0.0, true);
		let field_offset = sound_system.playing_music.as_ref().map(|music_voice| music_voice.offset).unwrap_or(-1.0);
		assert!((field_offset - stored_position % 4.0).abs() < 1e-6);

		// 첫 파일을 받지 못한 음악은 틀지 않는다.
		sound_system.asset_by_id.insert(String::from("broken"), vec![None, Some(create_silent_asset(1.0))]);
		sound_system.play_music("broken", 0.0, false);
		assert!(sound_system.playing_music.is_none());

		// "" 면 끈다.
		sound_system.play_music("", 0.0, false);
		assert_eq!(sound_system.get_music_id(), "");
		sound_system.update();
		assert!(sound_system.playing_music.is_none());
		std::thread::sleep(std::time::Duration::from_millis(300));
		sound_system.update();
		assert!(sound_system.fading_music_list.is_empty());
	}

	//==============================================================================
	/// 크기는 0 ~ 1, 수가 아니면 0. (원본 clampVolume) 함께 쓰는 소리는 정한 것을 돌려준다.
	//==============================================================================
	#[test]
	fn volume_setters_clamp_and_shared_sound_system() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(&engine, Vec::new());
		sound_system.set_music_volume(2.0);
		assert_eq!(sound_system.get_music_volume(), 1.0);
		sound_system.set_music_scale(f64::NAN);
		assert_eq!(sound_system.get_music_scale(), 0.0);
		sound_system.set_sound_effect_volume(-1.0);
		assert_eq!(sound_system.get_sound_effect_volume(), 0.0);
		assert_eq!(sound_system.get_last_context_resume_time(), f64::NEG_INFINITY);
		assert!(read_sound_system().is_none());
		let shared_sound_system = Rc::new(RefCell::new(sound_system));
		set_sound_system(Some(shared_sound_system.clone()));
		assert!(read_sound_system().map(|sound_system| Rc::ptr_eq(&sound_system, &shared_sound_system)).unwrap_or(false));
		set_sound_system(None);
		assert!(read_sound_system().is_none());
	}

	//==============================================================================
	/// 버스는 첫 update 뒤에 생긴다. 버스의 크기 칸은 SoundSystem 이 건드리지 않는다. (설정 크기는 버스 gain 의 setTargetAtTime 으로 소리 장치가 따라간다)
	//==============================================================================
	#[test]
	fn buses_follow_volume_settings() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(&engine, Vec::new());
		assert!(sound_system.get_music_bus().is_none());
		assert!(sound_system.get_effect_bus().is_none());
		sound_system.update();
		if !sound_system.audio_manager.has_audio_context() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		assert!(sound_system.get_music_bus().map(|music_bus| music_bus.is_available()).unwrap_or(false));
		assert!(sound_system.get_effect_bus().is_some());
		sound_system.set_music_volume(0.5);
		sound_system.set_sound_effect_volume(0.25);
		sound_system.update();
		let effect_bus = sound_system.get_effect_bus().expect("effect bus");
		assert_eq!(effect_bus.get_volume(), 1.0);
		let mut audio_player = effect_bus.create_audio_player();
		assert!(audio_player.get_audio_context().is_some());
		audio_player.set_audio_asset(create_silent_asset(1.0));
		audio_player.play(false);
		assert!(audio_player.is_playing());
		audio_player.stop();
	}
}
