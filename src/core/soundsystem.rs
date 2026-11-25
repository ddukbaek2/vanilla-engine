//==============================================================================
// 소리 표로 다루는 배경음악과 효과음. (vanilla.js core/soundsystem.js 의 SoundSystem)
// - AudioManager 와 AudioAsset 을 쓴다. 소리를 합성하지 않고 파일만 튼다.
// - 소리 표의 줄마다 SoundRow { id, paths, volume, minimum_interval, is_music, maximum_voices, pitch_variation, play_seconds, fade_out_seconds }.
//   paths: 같은 소리를 여러 번 녹음한 무리. 낼 때마다 받아 둔 것 가운데 하나를 고르고 바로 앞과 같은 것은 고르지 않는다.
//   minimum_interval: 같은 소리를 다시 낼 수 있는 간격. (초, 한꺼번에 몰려 시끄럽지 않게)
//   maximum_voices: 같은 소리가 한꺼번에 날 수 있는 수. (넘으면 가장 오래된 것을 끊는다, None 이면 효과음 전체의 수까지, 0 이면 원본처럼 하나)
//   pitch_variation: 낼 때마다 음 높이를 흔드는 비. (0.05 면 ±5%)
//   play_seconds, fade_out_seconds: 효과음을 앞의 play_seconds 초만 틀고 fade_out_seconds 초 동안 줄여 끈다. (0 이면 끝까지)
// - 크기: 배경음악 = 음악 크기 × 음악의 비 × 소리의 크기, 효과음 = 효과음 크기 × 소리의 크기 × 부를 때의 크기.
//   음악과 효과음의 크기는 버스(get_music_bus, get_effect_bus 의 AudioBus)에 걸고, 바꿀 때 0.05 초를 시간 상수로 따라간다. (원본 setTargetAtTime)
//   버스는 첫 누름 뒤에 만든다. (그 전에는 None) 버스로 만든 플레이어(AudioBus::create_audio_player)도 설정 크기와 음소거를 따른다.
// - 배경음악은 틀어 달라고 한 것(get_music_id)을 기억해 두고, 받기 전이거나 첫 누름 전이면 그때 튼다. ("" 이면 끈다)
//   겹쳐 바꿀 수 있고(fade_seconds), 멈췄던 자리를 소리마다 기억해 이어 틀 수 있다. (is_resumed) 첫 파일을 받지 못했으면 틀지 않는다.
// - 음소거는 설정한 크기보다 먼저다. 아래 가운데 하나라도 맞으면 소리가 나지 않는다.
//   바깥의 음소거(set_external_mute_provider, 플랫폼 SDK 나 광고), 사용자의 음소거(set_user_muted), 페이지가 가려짐(다른 탭, 앱 전환),
//   터치가 주된 기기에서 창이 포커스를 잃음(blur). 아이폰은 페이지가 가려졌다는 알림이 앱이 내려간 뒤에 와서 오디오가 멈추지 못한 채 얼어
//   마지막 소리 조각이 되풀이되는데, 포커스를 잃는 알림은 그보다 먼저 온다. 데스크탑(마우스)은 캔버스 바깥을 누를 때도 포커스를 잃어 따르지 않는다.
//   포커스가 돌아오거나 화면을 누르거나 페이지가 다시 보이면 풀린다.
// - 음소거인 동안은 AudioContext 도 멈춘다. (풀리면 깨운다) 아이폰은 돌던 AudioContext 를 앱 전환이나 전면 광고가 가로채면 돌아와서 깨워도
//   소리가 나지 않아, 그 전에 스스로 멈춰 둔다. 풀린 뒤에도 돌지 않으면 0.5 초마다 다시 깨워 본다.
// - 허락하면(set_autoplay_allowed_provider, 주지 않으면 허락) 누르기 전에도 첫 update 에서 한 번 깨워 본다. 막히면 엔진이 첫 누름에 깨운다.
//   첫 누름(user gesture) 전이거나 AudioContext 가 돌지 않을 때는 효과음을 내지 않는다. (멈춘 동안 부른 소리가 깨어날 때 한꺼번에 나지 않게)
// - 소리 파일은 놀이를 막지 않게 뒤에서 받는다. (웹은 디코딩이 끝나면 난다) 못 받은 소리는 나지 않을 뿐이다.
// - 시각: 같은 소리 간격과 다시 깨우기 간격은 원본 performance.now() 처럼 벽시계(web_time::Instant), 크기 따라가기와 줄이기,
//   음악의 멈춘 자리는 원본처럼 AudioContext 시계(AudioManager::get_context_time)로 센다.
// - 함께 쓰는 소리: set_sound_system, read_sound_system. (원본 모듈 변수, 스레드 하나에 하나)
// - 원본과 다른 것:
//   - 만들 때 AudioManager 대신 엔진을 받는다. 원본은 document 와 window 에 리스너를 다는데, 러스트 판은 엔진의 페이지 이벤트(PageEvent)로 받기 때문이다.
//     이벤트가 온 그 자리에서는 AudioContext 를 멈추거나 깨우고(소리가 바로 멎는다), 효과음 끊기는 다음 update 에서 한다.
//   - load_sounds, load_sound 는 애셋 폴더 기준 경로를 풀 엔진을 받는다. (원본은 브라우저가 페이지 주소로 푼다) 데스크탑은 그 자리에서 읽는다.
//     load_sounds 는 원본처럼 await 할 수 있는 SoundLoadHandle 을 돌려준다. (걸어 둔 소리를 다 받거나 실패할 때까지 기다린다, wait_for_load().await 도 같다)
//   - WebAudio 는 크기 바꾸기와 줄이기를 소리 장치가 맡지만, 러스트 판은 같은 식(선형 줄이기, 지수 따라가기)을 update 마다 셈해
//     소리마다의 크기는 플레이어에, 음악과 효과음의 크기는 버스에 건다. 그래서 update 를 매 프레임 불러야 한다. (원본도 매 프레임 부른다)
//     버스 크기는 셈한 값이 바뀔 때만 건다. (원본처럼 버스의 set_volume, mute 로 건 값은 SoundSystem 이 크기를 다시 걸 때까지 간다)
//   - 무리에서 고르기와 음 높이 흔들기는 원본 Math.random 자리인 엔진 math::random(처음 쓸 때 시각으로 씨앗을 정한다)을 쓴다.
//   - add_sound_asset(파일로 받지 않는 소리 더하기)과 get_effect_voice_count 는 러스트 판에서 먼저 만들었다. (원본에도 같은 이름으로 넣었다)
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
use crate::platform::audio as audio_backend;
use crate::resource::audioasset::{AudioAsset, AudioLoadHandle};

//==============================================================================
// 소리 받기 손잡이. (SoundSystem::load_sounds 가 돌려준다. 원본 loadSounds 의 Promise 자리, 걸어 둔 소리 애셋을 나눠 든다)
//==============================================================================
#[derive(Clone, Debug)]
pub struct SoundLoadHandle {
	audio_load_handle_list: Vec<AudioLoadHandle>,
}

impl std::future::IntoFuture for SoundLoadHandle {
	type Output = ();
	type IntoFuture = SoundLoadFuture;

	//==============================================================================
	// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> SoundLoadFuture {
		SoundLoadFuture {
			audio_load_handle_list: self.audio_load_handle_list,
		}
	}
}

//==============================================================================
// 소리 받기를 기다리는 퓨처. (다 받거나 실패할 때까지, 원본처럼 못 받은 소리는 건너뛴다)
//==============================================================================
#[derive(Clone, Debug)]
pub struct SoundLoadFuture {
	audio_load_handle_list: Vec<AudioLoadHandle>,
}

impl std::future::Future for SoundLoadFuture {
	type Output = ();

	//==============================================================================
	// 다 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: std::pin::Pin<&mut SoundLoadFuture>, context: &mut std::task::Context<'_>) -> std::task::Poll<()> {
		if self.audio_load_handle_list.iter().all(|audio_load_handle| audio_load_handle.is_finished()) {
			return std::task::Poll::Ready(());
		}
		context.waker().wake_by_ref();
		std::task::Poll::Pending
	}
}

// 효과음을 동시에 낼 수 있는 수. (넘으면 가장 오래된 것을 끊는다)
const EFFECT_VOICE_COUNT: usize = 12;
// 음소거가 아닌데 AudioContext 가 돌지 않을 때 다시 깨워 보는 간격. (초)
const CONTEXT_RESUME_RETRY_INTERVAL: f64 = 0.5;
// 크기를 바꿀 때 따라가는 시간. (초, 툭 끊기지 않게)
const VOLUME_SMOOTHING_SECONDS: f64 = 0.05;
// 효과음을 좌우로 나누는 가장 큰 값. (-1 ~ 1)
const MAXIMUM_PAN: f32 = 1.0;
// 이 크기 이하의 효과음은 내지 않는다.
const MINIMUM_EFFECT_VOLUME: f32 = 0.001;
// 줄여 끈 효과음을 다 줄어든 뒤 멈추기까지 남기는 틈. (초)
const EFFECT_STOP_MARGIN_SECONDS: f64 = 0.02;
// 버스 크기를 다시 거는 가장 작은 차이. (따라가기가 거의 끝나면 더 걸지 않는다)
const BUS_GAIN_EPSILON: f32 = 0.0001;

thread_local! {
	// 함께 쓰는 소리. (set_sound_system 으로 정한다)
	static CURRENT_SOUND_SYSTEM: RefCell<Option<Rc<RefCell<SoundSystem>>>> = const { RefCell::new(None) };
}

//==============================================================================
// 소리 표의 한 줄.
//==============================================================================
#[derive(Clone, Debug)]
pub struct SoundRow {
	pub id: String,
	pub paths: Vec<String>,
	pub volume: f32,
	pub minimum_interval: f32,
	pub is_music: bool,
	pub maximum_voices: Option<usize>,
	pub pitch_variation: f32,
	pub play_seconds: f32,
	pub fade_out_seconds: f32,
}

impl SoundRow {
	//==============================================================================
	// 효과음 한 줄. (나머지 칸은 기본)
	//==============================================================================
	pub fn create_effect(id: &str, paths: &[&str], volume: f32) -> SoundRow {
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
	// 배경음악 한 줄.
	//==============================================================================
	pub fn create_music(id: &str, path: &str, volume: f32) -> SoundRow {
		let mut sound_row = SoundRow::create_effect(id, &[path], volume);
		sound_row.is_music = true;
		sound_row
	}
}

//==============================================================================
// 시각에 따라 선형으로 바뀌는 크기. (WebAudio 의 setValueAtTime 뒤 linearRampToValueAtTime 한 번)
// - start_time 전에는 start_value, end_time 뒤에는 end_value, 그 사이는 선형이다.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct GainRamp {
	start_time: f64,
	start_value: f32,
	end_time: f64,
	end_value: f32,
}

impl GainRamp {
	//==============================================================================
	// 늘 같은 크기.
	//==============================================================================
	fn create_constant(value: f32) -> GainRamp {
		GainRamp {
			start_time: 0.0,
			start_value: value,
			end_time: 0.0,
			end_value: value,
		}
	}

	//==============================================================================
	// start_time 에 start_value 에서 end_time 에 end_value 로.
	//==============================================================================
	fn create_ramp(start_time: f64, start_value: f32, end_time: f64, end_value: f32) -> GainRamp {
		GainRamp {
			start_time,
			start_value,
			end_time,
			end_value,
		}
	}

	//==============================================================================
	// 그 시각의 크기.
	//==============================================================================
	fn read_value(&self, time: f64) -> f32 {
		if time <= self.start_time {
			return self.start_value;
		}
		if time >= self.end_time {
			return self.end_value;
		}
		let ratio = ((time - self.start_time) / (self.end_time - self.start_time)) as f32;
		self.start_value + (self.end_value - self.start_value) * ratio
	}
}

//==============================================================================
// 목표로 지수처럼 따라가는 크기. (WebAudio 의 setTargetAtTime, 시간 상수 VOLUME_SMOOTHING_SECONDS)
// - GainNode 처럼 처음 크기는 1 이다.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct SmoothedGain {
	start_time: f64,
	start_value: f32,
	target_value: f32,
}

impl SmoothedGain {
	//==============================================================================
	// 그 시각의 크기.
	//==============================================================================
	fn read_value(&self, time: f64) -> f32 {
		if time <= self.start_time {
			return self.start_value;
		}
		let decay = (-(time - self.start_time) / VOLUME_SMOOTHING_SECONDS).exp() as f32;
		self.target_value + (self.start_value - self.target_value) * decay
	}

	//==============================================================================
	// 그 시각의 크기. (목표에 BUS_GAIN_EPSILON 안으로 다가왔으면 목표, 버스에 걸 때)
	//==============================================================================
	fn read_settled_value(&self, time: f64) -> f32 {
		let value = self.read_value(time);
		if (value - self.target_value).abs() <= BUS_GAIN_EPSILON {
			return self.target_value;
		}
		value
	}

	//==============================================================================
	// 그 시각부터 새 목표로 따라가기.
	//==============================================================================
	fn set_target(&mut self, time: f64, target_value: f32) {
		self.start_value = self.read_value(time);
		self.start_time = time;
		self.target_value = target_value;
	}
}

//==============================================================================
// 나고 있는 효과음 하나. (원본 { soundId, source, gainNode, startTime }, 원본도 startTime 은 읽지 않아 뺐다)
//==============================================================================
struct EffectVoice {
	sound_id: String,
	audio_player: AudioPlayer,
	gain: GainRamp,
	// 멈출 때. (컨텍스트 시계 초, 없으면 끝까지)
	stop_time: Option<f64>,
}

//==============================================================================
// 틀고 있는 배경음악 하나. (원본 { soundId, source, gain, startTime, offset, duration })
//==============================================================================
struct MusicVoice {
	sound_id: String,
	audio_player: AudioPlayer,
	gain: GainRamp,
	// 튼 때. (컨텍스트 시계 초)
	start_time: f64,
	// 튼 자리와 소리 길이. (초)
	offset: f64,
	duration: f64,
	// 멈출 때. (컨텍스트 시계 초, 줄여 끄기 전에는 없음)
	stop_time: Option<f64>,
}


//==============================================================================
// 페이지 이벤트 리스너와 나눠 드는 음소거 상태. (이벤트가 온 그 자리에서 AudioContext 를 멈추거나 깨운다)
//==============================================================================
struct SoundMuteState {
	external_mute_provider: Option<Rc<dyn Fn() -> bool>>,
	is_user_muted: bool,
	is_page_hidden: bool,
	is_blur_mute_enabled: bool,
	is_window_blurred: bool,
	is_context_paused: bool,
	// 이벤트로 음소거가 되어 다음 update 에서 효과음을 끊을 차례인지.
	is_stop_effects_requested: bool,
	audio_manager_state: Rc<RefCell<AudioManagerState>>,
}

impl SoundMuteState {
	//==============================================================================
	// 지금 음소거여야 하는지. (바깥의 음소거, 사용자의 음소거, 페이지 가려짐, 창이 포커스를 잃음)
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
	// 음소거인 동안 AudioContext 멈추기와 풀리면 깨우기. (바뀔 때만, 첫 누름 전에는 AudioContext 가 없어 하지 않는다)
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
	// 페이지 이벤트. (원본 생성자의 visibilitychange, blur, focus, pointerdown 리스너)
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

pub struct SoundSystem {
	autoplay_allowed_provider: Option<Rc<dyn Fn() -> bool>>,
	audio_manager: AudioManager,
	sound_by_id: HashMap<String, SoundRow>,
	// 받은 소리. (무리의 차례대로, 못 받은 것은 None)
	asset_by_id: HashMap<String, Vec<Option<Rc<AudioAsset>>>>,
	last_play_time_by_id: HashMap<String, f64>,
	last_variant_by_id: HashMap<String, usize>,
	// 배경음악과 효과음의 버스. (원본 #musicBus, #effectBus, 첫 누름 전에는 None)
	music_bus: Option<AudioBus>,
	effect_bus: Option<AudioBus>,
	music_bus_gain: SmoothedGain,
	effect_bus_gain: SmoothedGain,
	// 버스에 마지막으로 건 크기.
	applied_music_bus_gain: f32,
	applied_effect_bus_gain: f32,
	// 나고 있는 효과음. (원본 #voices)
	voice_list: Vec<EffectVoice>,
	// 줄여 끄는 중인 효과음. (원본은 목록에서 빼고 소리 장치가 끝까지 줄인다, 러스트 판은 다 줄어들 때까지 크기를 셈해 걸어야 해서 따로 든다)
	releasing_voice_list: Vec<EffectVoice>,
	// 틀어 달라고 한 배경음악. (없으면 "")
	music_id: String,
	music_fade_seconds: f32,
	is_music_resumed: bool,
	playing_music: Option<MusicVoice>,
	// 줄여 끄는 중인 배경음악.
	fading_music_list: Vec<MusicVoice>,
	music_position_by_id: HashMap<String, f64>,
	music_volume: f32,
	music_scale: f32,
	sound_effect_volume: f32,
	// 소리의 길에 건 음소거.
	is_muted: bool,
	is_autoplay_tried: bool,
	last_context_resume_time: f64,
	mute_state: Rc<RefCell<SoundMuteState>>,
	// 엔진이 약한 참조로 들어 SoundSystem 이 사는 동안만 부른다.
	_page_event_listener: PageEventListener,
	clock_origin: Instant,
	// 시험에서 시계를 앞으로 돌리는 초.
	#[cfg(test)]
	test_time_offset: f64,
}

impl SoundSystem {
	//==============================================================================
	// 생성. (소리 표를 받는다, 받기는 load_sounds 가 한다, 창이 포커스를 잃으면 음소거할지는 지금의 주된 손으로 정한다)
	// - 엔진의 AudioManager 를 나눠 들고 페이지 이벤트 리스너를 단다.
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
			music_bus_gain: SmoothedGain { start_time: 0.0, start_value: 1.0, target_value: 1.0 },
			effect_bus_gain: SmoothedGain { start_time: 0.0, start_value: 1.0, target_value: 1.0 },
			applied_music_bus_gain: 1.0,
			applied_effect_bus_gain: 1.0,
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
	// 벽시계. (초, 원본 performance.now() / 1000 자리)
	//==============================================================================
	fn read_now(&self) -> f64 {
		let now = self.clock_origin.elapsed().as_secs_f64();
		#[cfg(test)]
		let now = now + self.test_time_offset;
		now
	}

	//==============================================================================
	// AudioContext 시계. (초, 원본 audioContext.currentTime)
	//==============================================================================
	fn read_context_time(&self) -> f64 {
		let context_time = self.audio_manager.get_context_time();
		#[cfg(test)]
		let context_time = context_time + self.test_time_offset;
		context_time
	}

	//==============================================================================
	// 소리 표의 줄 더하기. (같은 id 는 바꾼다, 받기는 load_sounds 가 한다)
	//==============================================================================
	pub fn add_sounds(&mut self, sound_table: Vec<SoundRow>) {
		for sound_row in sound_table {
			self.sound_by_id.insert(sound_row.id.clone(), sound_row);
		}
	}

	//==============================================================================
	// 바깥의 음소거를 읽는 함수 설정. (플랫폼 SDK 의 음소거, 보상형 광고를 보이는 동안 등, 매 프레임과 페이지 이벤트 때 읽는다, None 이면 없음)
	//==============================================================================
	pub fn set_external_mute_provider(&mut self, external_mute_provider: Option<Rc<dyn Fn() -> bool>>) {
		self.mute_state.borrow_mut().external_mute_provider = external_mute_provider;
	}

	//==============================================================================
	// 바깥의 음소거를 읽는 함수 반환.
	//==============================================================================
	pub fn get_external_mute_provider(&self) -> Option<Rc<dyn Fn() -> bool>> {
		self.mute_state.borrow().external_mute_provider.clone()
	}

	//==============================================================================
	// 누르기 전에 깨워 봐도 되는지 읽는 함수 설정. (웹뷰가 막는 플랫폼이면 false 를 돌려주는 함수, None 이면 허락, 첫 update 에서 한 번 읽는다)
	//==============================================================================
	pub fn set_autoplay_allowed_provider(&mut self, autoplay_allowed_provider: Option<Rc<dyn Fn() -> bool>>) {
		self.autoplay_allowed_provider = autoplay_allowed_provider;
	}

	//==============================================================================
	// 누르기 전에 깨워 봐도 되는지 읽는 함수 반환.
	//==============================================================================
	pub fn get_autoplay_allowed_provider(&self) -> Option<Rc<dyn Fn() -> bool>> {
		self.autoplay_allowed_provider.clone()
	}

	//==============================================================================
	// 엔진의 AudioManager. (나눠 든 손잡이)
	//==============================================================================
	pub fn get_audio_manager(&self) -> &AudioManager {
		&self.audio_manager
	}

	//==============================================================================
	// 배경음악과 효과음의 버스. (원본 getMusicBus, getEffectBus, 첫 누름 전에는 None)
	//==============================================================================
	pub fn get_music_bus(&self) -> Option<&AudioBus> {
		self.music_bus.as_ref()
	}

	pub fn get_effect_bus(&self) -> Option<&AudioBus> {
		self.effect_bus.as_ref()
	}

	//==============================================================================
	// 틀어 달라고 한 배경음악. (없으면 "")
	//==============================================================================
	pub fn get_music_id(&self) -> &str {
		&self.music_id
	}

	//==============================================================================
	// 설정의 음악 크기.
	//==============================================================================
	pub fn get_music_volume(&self) -> f32 {
		self.music_volume
	}

	//==============================================================================
	// 음악의 비. (멈춤 창의 반 크기 등)
	//==============================================================================
	pub fn get_music_scale(&self) -> f32 {
		self.music_scale
	}

	//==============================================================================
	// 설정의 효과음 크기.
	//==============================================================================
	pub fn get_sound_effect_volume(&self) -> f32 {
		self.sound_effect_volume
	}

	//==============================================================================
	// 사용자의 음소거 단추를 눌러 두었는지.
	//==============================================================================
	pub fn is_user_muted(&self) -> bool {
		self.mute_state.borrow().is_user_muted
	}

	//==============================================================================
	// 페이지가 가려져 있는지.
	//==============================================================================
	pub fn is_page_hidden(&self) -> bool {
		self.mute_state.borrow().is_page_hidden
	}

	//==============================================================================
	// 창이 포커스를 잃으면 음소거하는지. (터치가 주된 기기)
	//==============================================================================
	pub fn is_blur_mute_enabled(&self) -> bool {
		self.mute_state.borrow().is_blur_mute_enabled
	}

	//==============================================================================
	// 창이 포커스를 잃었는지. (음소거하는 기기에서만 true)
	//==============================================================================
	pub fn is_window_blurred(&self) -> bool {
		self.mute_state.borrow().is_window_blurred
	}

	//==============================================================================
	// 음소거로 AudioContext 를 멈춰 두었는지.
	//==============================================================================
	pub fn is_context_paused(&self) -> bool {
		self.mute_state.borrow().is_context_paused
	}

	//==============================================================================
	// 마지막으로 다시 깨워 본 때. (초, 아직 없으면 -무한대)
	//==============================================================================
	pub fn get_last_context_resume_time(&self) -> f64 {
		self.last_context_resume_time
	}

	//==============================================================================
	// 지금 음소거여야 하는지. (플랫폼의 음소거, 사용자의 음소거 단추, 페이지가 가려짐, 창이 포커스를 잃음, 설정한 크기보다 먼저이다)
	//==============================================================================
	pub fn read_muted(&self) -> bool {
		self.mute_state.borrow().read_muted()
	}

	//==============================================================================
	// AudioContext 가 돌고 있는지. (돌지 않으면 소리를 내지 않는다)
	//==============================================================================
	pub fn is_context_running(&self) -> bool {
		self.audio_manager.is_context_running()
	}

	//==============================================================================
	// 소리 받기. (효과음을 먼저, 배경음악을 나중에, 못 받은 소리는 건너뛴다, 이미 받은 것은 다시 받지 않는다, 애셋 폴더 기준 경로)
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
	// 걸어 둔 소리 애셋의 읽기 손잡이. (나눠 든다)
	//==============================================================================
	fn collect_audio_load_handles(&self) -> Vec<AudioLoadHandle> {
		self.asset_by_id.values().flat_map(|asset_list| asset_list.iter().flatten()).map(|audio_asset| audio_asset.get_load_handle()).collect()
	}

	//==============================================================================
	// 받기를 건 소리가 다 끝나기를 기다리는 퓨처. (원본 await loadSounds() 의 await 자리, load_sounds 뒤에 엔진 작업 안에서 await 한다)
	// - 부를 때 든 소리 애셋을 나눠 들어 소리 체계를 빌리지 않는다. 못 받은 소리는 건너뛴다. (원본처럼 거부되지 않는다)
	//==============================================================================
	pub fn wait_for_load(&self) -> SoundLoadFuture {
		SoundLoadFuture {
			audio_load_handle_list: self.collect_audio_load_handles(),
		}
	}

	//==============================================================================
	// 소리 하나 받기. (무리면 모두, 못 받은 것은 None 으로 두고 알린 뒤 건너뛴다)
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
	// 소리 애셋 직접 더하기. (메모리에서 지은 소리처럼 파일로 받지 않는 것, 같은 id 의 녹음 무리에 덧붙인다)
	//==============================================================================
	pub fn add_sound_asset(&mut self, sound_id: &str, audio_asset: Rc<AudioAsset>) {
		self.asset_by_id.entry(String::from(sound_id)).or_default().push(Some(audio_asset));
	}

	//==============================================================================
	// 설정의 음악 크기. (0 ~ 1)
	//==============================================================================
	pub fn set_music_volume(&mut self, music_volume: f32) {
		self.music_volume = clamp_sound_volume(music_volume);
		self.apply_volumes();
	}

	//==============================================================================
	// 음악의 비. (0 ~ 1, 멈춤 창이 떠 있는 동안 반 크기처럼 설정과 따로 줄일 때)
	//==============================================================================
	pub fn set_music_scale(&mut self, music_scale: f32) {
		self.music_scale = clamp_sound_volume(music_scale);
		self.apply_volumes();
	}

	//==============================================================================
	// 설정의 효과음 크기. (0 ~ 1)
	//==============================================================================
	pub fn set_sound_effect_volume(&mut self, sound_effect_volume: f32) {
		self.sound_effect_volume = clamp_sound_volume(sound_effect_volume);
		self.apply_volumes();
	}

	//==============================================================================
	// 사용자의 음소거 단추. (음악과 효과음을 한꺼번에)
	//==============================================================================
	pub fn set_user_muted(&mut self, is_user_muted: bool) {
		self.mute_state.borrow_mut().is_user_muted = is_user_muted;
		self.apply_mute();
		self.apply_context_pause();
	}

	//==============================================================================
	// 배경음악 틀기. (반복한다, "" 이면 끈다, 받기 전이거나 첫 누름 전이면 그때 튼다)
	// - fade_seconds 동안 앞 음악은 줄고 새 음악은 0 에서 오른다. (0 이면 바로)
	// - is_resumed 면 그 음악을 멈췄던 자리에서 잇는다. (아니면 처음부터)
	//==============================================================================
	pub fn play_music(&mut self, sound_id: &str, fade_seconds: f32, is_resumed: bool) {
		if sound_id == self.music_id {
			return;
		}
		self.music_id = String::from(sound_id);
		self.music_fade_seconds = fade_seconds.max(0.0);
		self.is_music_resumed = is_resumed;
		self.update();
	}

	//==============================================================================
	// 효과음 내기. (음소거, 첫 누름 전, 받기 전, 다시 낼 간격 안이면 내지 않는다)
	// - volume: 이번의 크기. (0 ~ 1, 소리의 크기와 설정의 효과음 크기에 곱한다)  pan: 좌우. (-1 왼쪽 ~ 1 오른쪽, 0 가운데)
	//==============================================================================
	pub fn play_effect(&mut self, sound_id: &str, volume: f32, pan: f32) {
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
		if now - last_play_time < sound_row.minimum_interval as f64 {
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
			audio_player.set_playback_rate(pitch_ratio as f64);
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
		// 앞만 틀고 줄여 끄는 소리.
		let context_time = self.read_context_time();
		let mut gain = GainRamp::create_constant(effect_volume);
		let mut stop_time = None;
		if sound_row.play_seconds > 0.0 {
			let fade_out_seconds = sound_row.fade_out_seconds.max(0.0) as f64;
			let fade_start_time = context_time + sound_row.play_seconds as f64;
			gain = GainRamp::create_ramp(fade_start_time, effect_volume, fade_start_time + fade_out_seconds, 0.0);
			stop_time = Some(fade_start_time + fade_out_seconds + EFFECT_STOP_MARGIN_SECONDS);
		}
		audio_player.set_volume(gain.read_value(context_time));
		audio_player.play(false);
		self.voice_list.push(EffectVoice {
			sound_id: String::from(sound_id),
			audio_player,
			gain,
			stop_time,
		});
	}

	//==============================================================================
	// 무리에서 하나 고르기. (받은 것 가운데 바로 앞과 다른 것, 하나뿐이면 그것, 없으면 None)
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
		let picked_index = candidate_index_list[((math::random() as f64 * candidate_count as f64) as usize).min(candidate_count - 1)];
		self.last_variant_by_id.insert(String::from(sound_id), picked_index);
		asset_list[picked_index].clone()
	}

	//==============================================================================
	// 나는 효과음을 그 수까지 줄이기. (오래된 것부터 끊음, sound_id 가 "" 이면 모든 효과음)
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
	// 나는 효과음을 줄여 끄기. (sound_id 가 "" 이면 모든 효과음, 줄이는 초가 0 이면 바로 끊음)
	// - 원본처럼 지금 크기에서 0 으로 줄이고 목록에서 뺀다. (앞만 트는 소리가 줄어드는 중이어도 지금 크기에서 새로 줄인다)
	//==============================================================================
	pub fn fade_out_effects(&mut self, sound_id: &str, fade_seconds: f32) {
		let clamped_fade_seconds = fade_seconds.max(0.0) as f64;
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
			let current_gain = voice.gain.read_value(context_time);
			voice.gain = GainRamp::create_ramp(context_time, current_gain, context_time + clamped_fade_seconds, 0.0);
			voice.stop_time = Some(context_time + clamped_fade_seconds + EFFECT_STOP_MARGIN_SECONDS);
			self.releasing_voice_list.push(voice);
		}
	}

	//==============================================================================
	// 나는 효과음을 모두 끊기. (멈춤 창이 뜰 때 등)
	//==============================================================================
	pub fn stop_effects(&mut self) {
		for voice in self.voice_list.iter_mut() {
			voice.audio_player.stop();
		}
		self.voice_list.clear();
	}

	//==============================================================================
	// 나는 효과음 수. (줄여 끄는 중인 것은 빼고)
	//==============================================================================
	pub fn get_effect_voice_count(&self) -> usize {
		self.voice_list.len()
	}

	//==============================================================================
	// 매 프레임. (첫 누름 뒤 소리의 길 만들기, 플랫폼의 음소거 따르기, 배경음악 틀기, 크기 걸기)
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
	// 다시 깨워 보기. (음소거가 아닌데 AudioContext 가 돌지 않으면 CONTEXT_RESUME_RETRY_INTERVAL 마다)
	// - 아이폰은 앱으로 돌아온 순간(다시 보임)에 부른 resume() 을 오디오가 아직 돌아오기 전이라 듣지 않는 일이 있다.
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
	// 누르기 전에 깨워 보기. (허락할 때 처음 한 번, 웹뷰가 막으면 AudioContext 가 멈춘 채로 남고 첫 누름에 엔진이 깨운다)
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
	// 소리의 길 만들기. (첫 누름 뒤 한 번, AudioContext 를 그 전에 만들지 않게, 버스를 만들 때 AudioContext 도 만든다)
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
		let context_time = self.read_context_time();
		self.music_bus_gain = SmoothedGain { start_time: context_time, start_value: 1.0, target_value: 1.0 };
		self.effect_bus_gain = SmoothedGain { start_time: context_time, start_value: 1.0, target_value: 1.0 };
		self.applied_music_bus_gain = 1.0;
		self.applied_effect_bus_gain = 1.0;
		self.music_bus = Some(music_bus);
		self.effect_bus = Some(self.audio_manager.create_audio_bus());
		self.is_muted = false;
		self.apply_volumes();
	}

	//==============================================================================
	// 크기 적용. (음소거면 0, 원본처럼 0.05 초를 시간 상수로 따라간다)
	//==============================================================================
	fn apply_volumes(&mut self) {
		if self.effect_bus.is_none() {
			return;
		}
		let mute_scale = if self.is_muted { 0.0 } else { 1.0 };
		let context_time = self.read_context_time();
		self.music_bus_gain.set_target(context_time, self.music_volume * self.music_scale * mute_scale);
		self.effect_bus_gain.set_target(context_time, self.sound_effect_volume * mute_scale);
	}

	//==============================================================================
	// 음소거 적용. (바뀔 때만, 음소거가 되면 나는 효과음을 끊음)
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
	// 음소거인 동안 AudioContext 멈추기와 풀리면 깨우기. (바뀔 때만, 첫 누름 전에는 AudioContext 가 없어 하지 않는다)
	//==============================================================================
	fn apply_context_pause(&mut self) {
		self.mute_state.borrow_mut().apply_context_pause();
	}

	//==============================================================================
	// 배경음악 틀기와 바꾸기. (AudioContext 가 돌고 받아 둔 것만, 음소거 중에도 소리 없이 돌아 풀면 이어 들린다)
	// - 바꾸라고 하면 앞 음악은 곧바로 줄이기 시작하고(멈춘 자리를 기억), 새 음악은 받아 두었으면 0 에서 올린다.
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
		let fade_seconds = self.music_fade_seconds as f64;
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
		let gain = if fade_seconds > 0.0 {
			GainRamp::create_ramp(context_time, 0.0, context_time + fade_seconds, row_volume)
		}
		else {
			GainRamp::create_constant(row_volume)
		};
		let mut audio_player = music_bus.create_audio_player();
		audio_player.set_audio_asset(audio_asset);
		audio_player.set_time(offset);
		audio_player.set_volume(gain.read_value(context_time));
		audio_player.play(true);
		self.playing_music = Some(MusicVoice {
			sound_id: self.music_id.clone(),
			audio_player,
			gain,
			start_time: context_time,
			offset,
			duration,
			stop_time: None,
		});
	}

	//==============================================================================
	// 틀던 배경음악을 줄여 끄기. (fade_seconds 동안, 다 줄어든 때의 자리를 기억)
	//==============================================================================
	fn fade_out_music(&mut self, mut playing_music: MusicVoice, fade_seconds: f64) {
		let context_time = self.read_context_time();
		let stop_time = context_time + fade_seconds;
		if playing_music.duration > 0.0 {
			let stop_position = (playing_music.offset + stop_time - playing_music.start_time) % playing_music.duration;
			self.music_position_by_id.insert(playing_music.sound_id.clone(), stop_position);
		}
		let current_gain = playing_music.gain.read_value(context_time);
		playing_music.gain = if fade_seconds > 0.0 {
			GainRamp::create_ramp(context_time, current_gain, stop_time, 0.0)
		}
		else {
			GainRamp::create_constant(0.0)
		};
		playing_music.stop_time = Some(stop_time + VOLUME_SMOOTHING_SECONDS);
		self.fading_music_list.push(playing_music);
	}

	//==============================================================================
	// 버스와 플레이어마다 지금 크기 걸기와 끝난 소리 치우기. (원본은 소리 장치가 크기를 바꾸고 onended 로 목록에서 뺀다)
	//==============================================================================
	fn update_voices(&mut self) {
		let context_time = self.read_context_time();
		self.apply_bus_gains(context_time);
		let update_effect_voice = |voice: &mut EffectVoice| -> bool {
			let is_stop_time = voice.stop_time.map(|stop_time| context_time >= stop_time).unwrap_or(false);
			if is_stop_time || !voice.audio_player.is_playing() {
				voice.audio_player.stop();
				return false;
			}
			voice.audio_player.set_volume(voice.gain.read_value(context_time));
			true
		};
		self.voice_list.retain_mut(update_effect_voice);
		self.releasing_voice_list.retain_mut(update_effect_voice);
		if let Some(playing_music) = self.playing_music.as_mut() {
			playing_music.audio_player.set_volume(playing_music.gain.read_value(context_time));
		}
		self.fading_music_list.retain_mut(|music_voice| {
			let is_stop_time = music_voice.stop_time.map(|stop_time| context_time >= stop_time).unwrap_or(false);
			if is_stop_time {
				music_voice.audio_player.stop();
				return false;
			}
			music_voice.audio_player.set_volume(music_voice.gain.read_value(context_time));
			true
		});
	}

	//==============================================================================
	// 버스에 지금 크기 걸기. (따라가는 크기가 마지막으로 건 것과 달라졌을 때만, 다 따라가면 목표 그대로)
	//==============================================================================
	fn apply_bus_gains(&mut self, context_time: f64) {
		if let Some(music_bus) = &self.music_bus {
			let music_bus_gain = self.music_bus_gain.read_settled_value(context_time);
			if (music_bus_gain - self.applied_music_bus_gain).abs() > BUS_GAIN_EPSILON {
				music_bus.apply_gain(music_bus_gain);
				self.applied_music_bus_gain = music_bus_gain;
			}
		}
		if let Some(effect_bus) = &self.effect_bus {
			let effect_bus_gain = self.effect_bus_gain.read_settled_value(context_time);
			if (effect_bus_gain - self.applied_effect_bus_gain).abs() > BUS_GAIN_EPSILON {
				effect_bus.apply_gain(effect_bus_gain);
				self.applied_effect_bus_gain = effect_bus_gain;
			}
		}
	}
}

//==============================================================================
// 0 ~ 1 로 다듬기. (유한한 수가 아니면 0, 원본 clampVolume)
//==============================================================================
fn clamp_sound_volume(volume: f32) -> f32 {
	if !volume.is_finite() {
		return 0.0;
	}
	volume.clamp(0.0, 1.0)
}

//==============================================================================
// 함께 쓰는 소리 정하기. (None 이면 비운다)
//==============================================================================
pub fn set_sound_system(sound_system: Option<Rc<RefCell<SoundSystem>>>) {
	CURRENT_SOUND_SYSTEM.with(|current_sound_system| {
		*current_sound_system.borrow_mut() = sound_system;
	});
}

//==============================================================================
// 함께 쓰는 소리. (아직 없으면 None)
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
	// 무음 WAV. (16 비트 모노 8000 Hz, sample_count 개)
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
	// 읽힌 무음 소리 애셋. (seconds 초)
	//==============================================================================
	fn create_silent_asset(seconds: f32) -> Rc<AudioAsset> {
		let audio_asset = Rc::new(AudioAsset::new());
		assert!(audio_asset.load_from_bytes(create_silent_wav((seconds * 8000.0) as u32)).get_load_error().is_none());
		assert!(audio_asset.is_loaded());
		audio_asset
	}

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
	// 줄여 끄기: 목록에서 빠지고 지금 크기에서 0 으로, 앞만 트는 소리가 줄어드는 중이어도 지금 크기에서 새로 줄인다.
	//==============================================================================
	#[test]
	fn fade_out_restarts_from_current_gain() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut short_row = SoundRow::create_effect("short", &[], 1.0);
		short_row.play_seconds = 0.1;
		short_row.fade_out_seconds = 1.0;
		let mut sound_system = SoundSystem::new(&engine, vec![short_row]);
		sound_system.add_sound_asset("short", create_silent_asset(5.0));
		sound_system.update();
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_effect("short", 0.8, 0.0);
		let context_time = sound_system.read_context_time();
		let voice_gain = sound_system.voice_list[0].gain;
		assert_eq!(voice_gain.read_value(context_time), 0.8);
		assert!((voice_gain.read_value(context_time + 0.6) - 0.4).abs() < 1e-3);

		// 줄어드는 한가운데(0.4)에서 2 초 줄이기를 걸면 0.4 에서 새로 줄인다.
		sound_system.test_time_offset += 0.6;
		sound_system.fade_out_effects("short", 2.0);
		assert_eq!(sound_system.get_effect_voice_count(), 0);
		assert_eq!(sound_system.releasing_voice_list.len(), 1);
		let context_time = sound_system.read_context_time();
		let release_gain = sound_system.releasing_voice_list[0].gain;
		assert!((release_gain.read_value(context_time) - 0.4).abs() < 1e-2);
		assert!((release_gain.read_value(context_time + 1.0) - 0.2).abs() < 1e-2);

		// 줄여 끄는 소리는 stop_effects 로도 끊기지 않고, 다 줄어든 뒤 치운다.
		sound_system.stop_effects();
		assert_eq!(sound_system.releasing_voice_list.len(), 1);
		sound_system.test_time_offset += 2.1;
		sound_system.update();
		assert!(sound_system.releasing_voice_list.is_empty());
	}

	//==============================================================================
	// 크기는 0.05 초 시간 상수로 따라간다. (원본 setTargetAtTime)
	//==============================================================================
	#[test]
	fn volume_follows_with_time_constant() {
		let mut smoothed_gain = SmoothedGain { start_time: 0.0, start_value: 1.0, target_value: 1.0 };
		smoothed_gain.set_target(1.0, 0.0);
		assert_eq!(smoothed_gain.read_value(1.0), 1.0);
		assert!((smoothed_gain.read_value(1.05) - (-1.0f32).exp()).abs() < 1e-5);
		assert!(smoothed_gain.read_value(1.5) < 1e-4);
		smoothed_gain.set_target(1.05, 1.0);
		assert!((smoothed_gain.read_value(1.05) - (-1.0f32).exp()).abs() < 1e-5);
	}

	//==============================================================================
	// 누르기 전에 깨워 보기는 첫 update 에서 한 번만 허락을 묻는다.
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
	// 페이지 이벤트: 가려지면 음소거, 터치가 주된 기기에서 포커스를 잃으면 음소거, 누르거나 포커스를 얻거나 다시 보이면 풀린다.
	// - 마우스가 주된 기기(데스크탑)는 포커스를 잃어도 음소거하지 않는다.
	//==============================================================================
	#[test]
	fn page_events_mute_and_unmute() {
		let mut engine = Engine::new(EngineConfiguration::new());
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
	// 가려졌다 다시 보이는 사이에 update 가 없어도 내던 효과음은 끊긴다. (원본은 가려지는 순간 끊는다)
	//==============================================================================
	#[test]
	fn hidden_page_stops_effects() {
		let mut engine = Engine::new(EngineConfiguration::new());
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
	// 배경음악: 받기 전에 틀어 달라고 하면 기억했다가 받으면 튼다, "" 면 끈다, 멈췄던 자리에서 잇는다, 첫 파일을 못 받았으면 틀지 않는다.
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

		// 1 초 겹쳐 바꾸면, 앞 음악은 다 줄어든 때의 자리를 기억한다. (튼 뒤 흐른 시간 + 1 초, 실제 시계도 흘러 오차를 둔다)
		let start_time = sound_system.playing_music.as_ref().map(|music_voice| music_voice.start_time).unwrap_or(0.0);
		sound_system.test_time_offset += 0.5;
		sound_system.play_music("battle", 1.0, false);
		let elapsed_seconds = sound_system.read_context_time() - start_time;
		let stored_position = sound_system.music_position_by_id.get("field").copied().unwrap_or(-1.0);
		assert!((stored_position - (elapsed_seconds + 1.0)).abs() < 0.05);
		assert_eq!(sound_system.fading_music_list.len(), 1);
		sound_system.test_time_offset += 1.1;
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
		sound_system.test_time_offset += 0.1;
		sound_system.update();
		assert!(sound_system.playing_music.is_none());
		assert!(sound_system.fading_music_list.is_empty());
	}

	//==============================================================================
	// 크기는 0 ~ 1, 수가 아니면 0. (원본 clampVolume) 함께 쓰는 소리는 정한 것을 돌려준다.
	//==============================================================================
	#[test]
	fn volume_setters_clamp_and_shared_sound_system() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(&engine, Vec::new());
		sound_system.set_music_volume(2.0);
		assert_eq!(sound_system.get_music_volume(), 1.0);
		sound_system.set_music_scale(f32::NAN);
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
	// 버스는 첫 update 뒤에 생기고, 설정 크기를 따라가 다 따라가면 목표 그대로 걸린다. 버스의 크기 칸은 SoundSystem 이 건드리지 않는다.
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
		sound_system.test_time_offset += 0.05;
		sound_system.update();
		assert!(sound_system.applied_music_bus_gain < 1.0 && sound_system.applied_music_bus_gain > 0.5);
		sound_system.test_time_offset += 1.0;
		sound_system.update();
		assert_eq!(sound_system.applied_music_bus_gain, 0.5);
		assert_eq!(sound_system.applied_effect_bus_gain, 0.25);
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
