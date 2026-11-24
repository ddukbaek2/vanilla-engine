//==============================================================================
// 소리 표로 다루는 배경음악과 효과음. (vanilla.js core/soundsystem.js 의 SoundSystem)
// - AudioManager 의 플레이어와 AudioAsset 을 쓴다. 소리를 합성하지 않고 파일만 튼다.
// - 소리 표의 줄마다 SoundRow { id, paths, volume, minimum_interval, is_music, maximum_voices, pitch_variation, play_seconds, fade_out_seconds }.
//   paths: 같은 소리를 여러 번 녹음한 무리. 낼 때마다 받아 둔 것 가운데 하나를 고르고 바로 앞과 같은 것은 고르지 않는다.
//   minimum_interval: 같은 소리를 다시 낼 수 있는 간격. (초, 한꺼번에 몰려 시끄럽지 않게)
//   maximum_voices: 같은 소리가 한꺼번에 날 수 있는 수. (넘으면 가장 오래된 것을 끊는다, 0 이면 효과음 전체의 수까지)
//   pitch_variation: 낼 때마다 음 높이를 흔드는 비. (0.05 면 ±5%)
//   play_seconds, fade_out_seconds: 효과음을 앞의 play_seconds 초만 틀고 fade_out_seconds 초 동안 줄여 끈다. (0 이면 끝까지)
// - 크기: 배경음악 = 음악 크기 × 음악의 비 × 소리의 크기, 효과음 = 효과음 크기 × 소리의 크기 × 부를 때의 크기.
// - 배경음악은 틀어 달라고 한 것(get_music_id)을 기억해 두고, 받기 전이거나 첫 누름 전이면 그때 튼다. ("" 이면 끈다)
//   겹쳐 바꿀 수 있고(fade_seconds), 멈췄던 자리를 소리마다 기억해 이어 틀 수 있다. (is_resumed)
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
// - 원본과 다른 것:
//   - 페이지 가려짐과 창 포커스는 엔진의 페이지 이벤트(PageEvent)로 받는다. 엔진을 처음 받는 update, load_sounds, play_effect, play_music 에서 리스너를 단다.
//     이벤트가 온 그 자리에서는 AudioContext 를 멈추거나 깨우고(소리가 바로 멎는다), 효과음 끊기와 크기 걸기는 다음 update 에서 한다.
//   - 데스크탑의 AudioContext 멈춤은 kira 에 멈춤이 없어 메인 트랙을 무음으로 둔다. (그동안에도 소리의 시간은 흐른다)
//   - 음악과 효과음 버스(GainNode) 대신 플레이어마다 버스 크기를 곱해 건다. 크기는 원본의 0.05 초 따라가기 없이 바로 바뀐다.
//   - 시각은 performance.now() 대신 update 에 넘긴 time_delta 를 더한 것이다. (같은 소리 간격, 다시 깨우기 간격, 음악의 멈춘 자리)
//   - 무리에서 고르기와 음 높이 흔들기는 Math.random 대신 SeededRandom 을 쓴다.
//   - update 는 엔진을 받는다. (원본은 만들 때 AudioManager 를 받아 든다)
//   - set_page_hidden 은 러스트 판에만 있다. (부르는 쪽이 직접 알릴 때, 페이지 이벤트가 오면 그것이 덮는다)
//     fade_out_music 도 러스트 판에만 있다. (play_music("", 초) 처럼 배경음악을 줄여 끈다)
//   - 원본 getExternalMuteProvider, getAutoplayAllowedProvider(함수를 돌려준다), getAudioManager, getMusicBus, getEffectBus 는 옮기지 않았다.
//==============================================================================
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::base::seededrandom::SeededRandom;
use crate::core::audiomanager::{resume_audio_manager_state, AudioManagerState};
use crate::core::audioplayer::AudioPlayer;
use crate::core::engine::{Engine, PageEvent, PageEventListener};
use crate::platform;
use crate::platform::audio as audio_backend;
use crate::resource::audioasset::AudioAsset;

// 효과음을 동시에 낼 수 있는 수. (넘으면 가장 오래된 것을 끊는다)
const EFFECT_VOICE_COUNT: usize = 12;
// 음소거가 아닌데 AudioContext 가 돌지 않을 때 다시 깨워 보는 간격. (초)
const CONTEXT_RESUME_RETRY_INTERVAL: f32 = 0.5;
// 효과음을 좌우로 나누는 가장 큰 값. (-1 ~ 1)
const MAXIMUM_PAN: f32 = 1.0;
// 이 크기 이하의 효과음은 내지 않는다.
const MINIMUM_EFFECT_VOLUME: f32 = 0.001;

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
	pub maximum_voices: usize,
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
			maximum_voices: 0,
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
// 내고 있는 효과음 하나.
//==============================================================================
struct EffectVoice {
	audio_player: AudioPlayer,
	sound_id: String,
	volume: f32,
	elapsed_seconds: f32,
	play_seconds: f32,
	fade_out_seconds: f32,
	fade_start_seconds: f32,
}

//==============================================================================
// 내고 있는 배경음악 하나. (겹쳐 바꾸는 동안 둘)
//==============================================================================
struct MusicVoice {
	audio_player: AudioPlayer,
	sound_id: String,
	volume: f32,
	fade_ratio: f32,
	fade_seconds: f32,
	is_fading_in: bool,
	// 튼 자리와 그 뒤로 흐른 시간, 소리 길이. (멈춘 자리를 셈한다, 초)
	start_offset: f64,
	elapsed_seconds: f64,
	duration: f64,
}

//==============================================================================
// 페이지 이벤트 리스너와 나눠 드는 음소거 상태. (이벤트가 온 그 자리에서 AudioContext 를 멈추거나 깨운다)
//==============================================================================
struct SoundMuteState {
	external_mute_provider: Option<Box<dyn Fn() -> bool>>,
	is_user_muted: bool,
	is_page_hidden: bool,
	is_blur_mute_enabled: bool,
	is_window_blurred: bool,
	is_context_paused: bool,
	// 이벤트로 음소거가 되어 다음 update 에서 효과음을 끊을 차례인지.
	is_stop_effects_requested: bool,
	audio_manager_state: Option<Rc<RefCell<AudioManagerState>>>,
}

impl SoundMuteState {
	//==============================================================================
	// 지금 음소거여야 하는지. (바깥의 음소거, 사용자의 음소거, 페이지 가려짐, 창이 포커스를 잃음)
	//==============================================================================
	fn read_muted(&self) -> bool {
		let is_external_muted = self.external_mute_provider.as_ref().map(|provider| provider()).unwrap_or(false);
		is_external_muted || self.is_user_muted || self.is_page_hidden || self.is_window_blurred
	}

	//==============================================================================
	// 음소거인 동안 AudioContext 멈추기와 풀리면 깨우기. (바뀔 때만, 첫 누름 전에는 AudioContext 가 없어 하지 않는다)
	//==============================================================================
	fn apply_context_pause(&mut self) {
		let audio_manager_state = match &self.audio_manager_state {
			Some(audio_manager_state) => {
				audio_manager_state.clone()
			}
			None => {
				return;
			}
		};
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
	sound_by_id: HashMap<String, SoundRow>,
	asset_by_id: HashMap<String, Vec<Rc<AudioAsset>>>,
	last_variant_by_id: HashMap<String, usize>,
	last_play_time_by_id: HashMap<String, f32>,
	effect_voice_list: Vec<EffectVoice>,
	music_voice_list: Vec<MusicVoice>,
	music_resume_time_by_id: HashMap<String, f64>,
	// 틀어 달라고 한 배경음악. (없으면 "")
	music_id: String,
	music_fade_seconds: f32,
	is_music_resumed: bool,
	music_volume: f32,
	music_scale: f32,
	sound_effect_volume: f32,
	// 플레이어에 건 음소거.
	is_muted: bool,
	// 첫 누름 뒤 소리의 길을 만들었는지. (원본 effect_bus !== null)
	is_players_ready: bool,
	autoplay_allowed_provider: Option<Box<dyn Fn() -> bool>>,
	is_autoplay_tried: bool,
	last_context_resume_time: f32,
	mute_state: Rc<RefCell<SoundMuteState>>,
	page_event_listener: Option<PageEventListener>,
	elapsed_seconds: f32,
	random: SeededRandom,
}

impl SoundSystem {
	//==============================================================================
	// 생성. (소리 표를 받는다, 받기는 load_sounds 가 한다, 창이 포커스를 잃으면 음소거할지는 지금의 주된 손으로 정한다)
	//==============================================================================
	pub fn new(sound_table: Vec<SoundRow>) -> SoundSystem {
		let mut sound_system = SoundSystem {
			sound_by_id: HashMap::new(),
			asset_by_id: HashMap::new(),
			last_variant_by_id: HashMap::new(),
			last_play_time_by_id: HashMap::new(),
			effect_voice_list: Vec::new(),
			music_voice_list: Vec::new(),
			music_resume_time_by_id: HashMap::new(),
			music_id: String::new(),
			music_fade_seconds: 0.0,
			is_music_resumed: false,
			music_volume: 1.0,
			music_scale: 1.0,
			sound_effect_volume: 1.0,
			is_muted: false,
			is_players_ready: false,
			autoplay_allowed_provider: None,
			is_autoplay_tried: false,
			last_context_resume_time: f32::NEG_INFINITY,
			mute_state: Rc::new(RefCell::new(SoundMuteState {
				external_mute_provider: None,
				is_user_muted: false,
				is_page_hidden: false,
				is_blur_mute_enabled: platform::is_coarse_pointer(),
				is_window_blurred: false,
				is_context_paused: false,
				is_stop_effects_requested: false,
				audio_manager_state: None,
			})),
			page_event_listener: None,
			elapsed_seconds: 0.0,
			random: SeededRandom::new(0x50f7),
		};
		sound_system.add_sounds(sound_table);
		sound_system
	}

	//==============================================================================
	// 엔진에 붙기. (처음 한 번, 오디오 매니저를 나눠 들고 페이지 이벤트 리스너를 단다, 지금 페이지가 가려졌는지 읽는다)
	//==============================================================================
	fn attach_engine(&mut self, engine: &Engine) {
		if self.page_event_listener.is_some() {
			return;
		}
		{
			let mut mute_state = self.mute_state.borrow_mut();
			mute_state.audio_manager_state = Some(engine.get_audio_manager().get_shared_state());
			mute_state.is_page_hidden = engine.is_page_hidden();
		}
		let mute_state_for_listener = self.mute_state.clone();
		let page_event_listener: PageEventListener = Rc::new(RefCell::new(move |page_event: PageEvent| {
			if let Ok(mut mute_state) = mute_state_for_listener.try_borrow_mut() {
				mute_state.handle_page_event(page_event);
			}
		}));
		engine.add_page_event_listener(&page_event_listener);
		self.page_event_listener = Some(page_event_listener);
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
	// 소리 파일 받기. (아직 받지 않은 줄만, 효과음을 먼저 배경음악을 나중에, 애셋 폴더 기준 경로, 못 받은 파일은 알리고 지나간다)
	//==============================================================================
	pub fn load_sounds(&mut self, engine: &Engine) {
		self.attach_engine(engine);
		for is_music_pass in [false, true] {
			for (sound_id, sound_row) in self.sound_by_id.iter() {
				if sound_row.is_music != is_music_pass || self.asset_by_id.contains_key(sound_id) {
					continue;
				}
				let mut asset_list = Vec::new();
				for path in sound_row.paths.iter() {
					match engine.load_audio_asset(path) {
						Ok(audio_asset) => {
							asset_list.push(audio_asset);
						}
						Err(error) => {
							eprintln!("[sound] 소리를 받지 못했습니다. {} ({})", path, error);
						}
					}
				}
				self.asset_by_id.insert(sound_id.clone(), asset_list);
			}
		}
	}

	//==============================================================================
	// 소리 애셋 직접 더하기. (메모리에서 지은 소리처럼 파일로 받지 않는 것, 같은 id 의 녹음 무리에 덧붙인다)
	//==============================================================================
	pub fn add_sound_asset(&mut self, sound_id: &str, audio_asset: Rc<AudioAsset>) {
		self.asset_by_id.entry(String::from(sound_id)).or_default().push(audio_asset);
	}

	//==============================================================================
	// 바깥의 음소거를 읽는 함수 설정. (플랫폼 SDK 의 음소거, 보상형 광고를 보이는 동안 등, 매 프레임과 페이지 이벤트 때 읽는다)
	//==============================================================================
	pub fn set_external_mute_provider(&mut self, external_mute_provider: Option<Box<dyn Fn() -> bool>>) {
		self.mute_state.borrow_mut().external_mute_provider = external_mute_provider;
	}

	//==============================================================================
	// 누르기 전에 깨워 봐도 되는지 읽는 함수 설정. (웹뷰가 막는 플랫폼이면 false 를 돌려주는 함수, None 이면 허락, 첫 update 에서 한 번 읽는다)
	//==============================================================================
	pub fn set_autoplay_allowed_provider(&mut self, autoplay_allowed_provider: Option<Box<dyn Fn() -> bool>>) {
		self.autoplay_allowed_provider = autoplay_allowed_provider;
	}

	//==============================================================================
	// 사용자의 음소거 설정. (음악과 효과음을 한꺼번에, 바로 건다)
	//==============================================================================
	pub fn set_user_muted(&mut self, is_user_muted: bool) {
		self.mute_state.borrow_mut().is_user_muted = is_user_muted;
		self.apply_mute();
		self.apply_context_pause();
	}

	pub fn is_user_muted(&self) -> bool {
		self.mute_state.borrow().is_user_muted
	}

	//==============================================================================
	// 페이지가 가려졌는지 설정. (러스트 판에만 있다, 엔진의 페이지 이벤트와 같게 다시 보이면 포커스 음소거도 푼다)
	//==============================================================================
	pub fn set_page_hidden(&mut self, is_page_hidden: bool) {
		let page_event = if is_page_hidden {
			PageEvent::Hidden
		}
		else {
			PageEvent::Visible
		};
		self.mute_state.borrow_mut().handle_page_event(page_event);
		self.apply_mute();
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
	// 마지막으로 다시 깨워 본 때. (update 로 흐른 초, 아직 없으면 -무한대)
	//==============================================================================
	pub fn get_last_context_resume_time(&self) -> f32 {
		self.last_context_resume_time
	}

	//==============================================================================
	// AudioContext 가 돌고 있는지. (돌지 않으면 효과음을 내지 않는다, 엔진에 붙기 전에는 false)
	//==============================================================================
	pub fn is_context_running(&self) -> bool {
		match &self.mute_state.borrow().audio_manager_state {
			Some(audio_manager_state) => {
				audio_manager_state.borrow().is_context_running()
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 지금 음소거여야 하는지. (바깥의 음소거, 사용자의 음소거, 페이지 가려짐, 창이 포커스를 잃음)
	//==============================================================================
	pub fn read_muted(&self) -> bool {
		self.mute_state.borrow().read_muted()
	}

	//==============================================================================
	// 크기 설정. (음악 크기, 음악의 비(멈춤 창의 반 크기 등), 효과음 크기, 모두 0 ~ 1, 수가 아니면 0)
	//==============================================================================
	pub fn set_music_volume(&mut self, music_volume: f32) {
		self.music_volume = clamp_sound_volume(music_volume);
		self.apply_volumes();
	}

	pub fn get_music_volume(&self) -> f32 {
		self.music_volume
	}

	pub fn set_music_scale(&mut self, music_scale: f32) {
		self.music_scale = clamp_sound_volume(music_scale);
		self.apply_volumes();
	}

	pub fn get_music_scale(&self) -> f32 {
		self.music_scale
	}

	pub fn set_sound_effect_volume(&mut self, sound_effect_volume: f32) {
		self.sound_effect_volume = clamp_sound_volume(sound_effect_volume);
		self.apply_volumes();
	}

	pub fn get_sound_effect_volume(&self) -> f32 {
		self.sound_effect_volume
	}

	//==============================================================================
	// 틀어 달라고 한 배경음악의 id. (없으면 "")
	//==============================================================================
	pub fn get_music_id(&self) -> &str {
		&self.music_id
	}

	//==============================================================================
	// 녹음 무리에서 하나 고르기. (받아 둔 것 가운데 바로 앞과 다른 것, 하나뿐이면 그것, 없으면 None)
	//==============================================================================
	fn pick_variant(&mut self, sound_id: &str, asset_list: &[Rc<AudioAsset>]) -> Option<usize> {
		let loaded_index_list: Vec<usize> = asset_list.iter().enumerate().filter(|(_, audio_asset)| audio_asset.is_loaded()).map(|(asset_index, _)| asset_index).collect();
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
		let picked_index = candidate_index_list[((self.random.next_value() * candidate_count as f64) as usize).min(candidate_count - 1)];
		self.last_variant_by_id.insert(String::from(sound_id), picked_index);
		Some(picked_index)
	}

	//==============================================================================
	// 효과음 내기. (부를 때의 크기 0 ~ 1, 좌우 -1 왼쪽 ~ 1 오른쪽)
	// - 첫 누름 전, 음소거, AudioContext 가 돌지 않음, 받기 전, 다시 낼 간격 안이면 내지 않는다.
	//==============================================================================
	pub fn play_effect(&mut self, engine: &Engine, sound_id: &str, volume: f32, pan: f32) {
		self.attach_engine(engine);
		if !self.is_players_ready || self.is_muted || !self.is_context_running() {
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
		if let Some(last_play_time) = self.last_play_time_by_id.get(sound_id) {
			if self.elapsed_seconds - last_play_time < sound_row.minimum_interval {
				return;
			}
		}
		let variant_index = match self.pick_variant(sound_id, &asset_list) {
			Some(variant_index) => {
				variant_index
			}
			None => {
				return;
			}
		};
		let audio_asset = asset_list[variant_index].clone();
		self.last_play_time_by_id.insert(String::from(sound_id), self.elapsed_seconds);

		// 같은 소리의 수와 전체 수를 넘으면 가장 오래된 것을 끊는다.
		let maximum_voices = if sound_row.maximum_voices > 0 {
			sound_row.maximum_voices
		}
		else {
			EFFECT_VOICE_COUNT
		};
		self.trim_voices(Some(sound_id), maximum_voices - 1);
		self.trim_voices(None, EFFECT_VOICE_COUNT - 1);
		let mut audio_player = engine.get_audio_manager().create_audio_player();
		audio_player.set_audio_asset(audio_asset);
		if sound_row.pitch_variation > 0.0 {
			let pitch_ratio = 1.0 + (self.random.next_value() as f32 * 2.0 - 1.0) * sound_row.pitch_variation;
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
		audio_player.set_volume(effect_volume * self.sound_effect_volume);
		audio_player.play(false);
		self.effect_voice_list.push(EffectVoice {
			audio_player,
			sound_id: String::from(sound_id),
			volume: effect_volume,
			elapsed_seconds: 0.0,
			play_seconds: sound_row.play_seconds,
			fade_out_seconds: sound_row.fade_out_seconds,
			fade_start_seconds: -1.0,
		});
	}

	//==============================================================================
	// 내는 효과음을 keep_count 개만 남기고 오래된 것부터 끊기. (sound_id 가 없으면 모두에서)
	//==============================================================================
	fn trim_voices(&mut self, sound_id: Option<&str>, keep_count: usize) {
		let voice_count = self.effect_voice_list.iter().filter(|voice| sound_id.map(|id| voice.sound_id == id).unwrap_or(true)).count();
		let mut remove_count = voice_count.saturating_sub(keep_count);
		self.effect_voice_list.retain_mut(|voice| {
			if remove_count == 0 || !sound_id.map(|id| voice.sound_id == id).unwrap_or(true) {
				return true;
			}
			voice.audio_player.stop();
			remove_count -= 1;
			false
		});
	}

	//==============================================================================
	// 효과음을 줄여 끄기. (sound_id 가 "" 이면 모든 효과음, fade_seconds 가 0 이하면 바로 끊는다)
	//==============================================================================
	pub fn fade_out_effects(&mut self, sound_id: &str, fade_seconds: f32) {
		let is_all_effects = sound_id.is_empty();
		if fade_seconds <= 0.0 {
			self.effect_voice_list.retain_mut(|voice| {
				if !is_all_effects && voice.sound_id != sound_id {
					return true;
				}
				voice.audio_player.stop();
				false
			});
			return;
		}
		for voice in self.effect_voice_list.iter_mut() {
			if (is_all_effects || voice.sound_id == sound_id) && voice.fade_start_seconds < 0.0 {
				voice.fade_start_seconds = voice.elapsed_seconds;
				voice.fade_out_seconds = fade_seconds;
			}
		}
	}

	//==============================================================================
	// 효과음 모두 끄기. (멈춤 창이 뜰 때 등)
	//==============================================================================
	pub fn stop_effects(&mut self) {
		for voice in self.effect_voice_list.iter_mut() {
			voice.audio_player.stop();
		}
		self.effect_voice_list.clear();
	}

	//==============================================================================
	// 배경음악 틀기. (반복한다, "" 이면 끈다, 받기 전이거나 첫 누름 전이면 그때 튼다)
	// - fade_seconds 동안 앞 음악은 줄고 새 음악은 0 에서 오른다. (0 이면 바로)
	// - is_resumed 면 그 음악을 멈췄던 자리에서 잇는다. (아니면 처음부터)
	//==============================================================================
	pub fn play_music(&mut self, engine: &Engine, sound_id: &str, fade_seconds: f32, is_resumed: bool) {
		self.attach_engine(engine);
		if sound_id == self.music_id {
			return;
		}
		self.music_id = String::from(sound_id);
		self.music_fade_seconds = fade_seconds.max(0.0);
		self.is_music_resumed = is_resumed;
		self.refresh(engine);
	}

	//==============================================================================
	// 배경음악 줄여 끄기. (러스트 판에만 있다, play_music("", fade_seconds) 와 같다)
	//==============================================================================
	pub fn fade_out_music(&mut self, fade_seconds: f32) {
		self.music_id = String::new();
		self.music_fade_seconds = fade_seconds.max(0.0);
		self.fade_out_playing_music(self.music_fade_seconds);
		self.apply_volumes();
	}

	//==============================================================================
	// 틀던 배경음악을 줄여 끄기. (fade_seconds 동안, 다 줄어든 때의 자리를 기억한다)
	//==============================================================================
	fn fade_out_playing_music(&mut self, fade_seconds: f32) {
		for music_voice in self.music_voice_list.iter_mut() {
			if !music_voice.is_fading_in {
				continue;
			}
			music_voice.is_fading_in = false;
			music_voice.fade_seconds = fade_seconds;
			if fade_seconds <= 0.0 {
				music_voice.fade_ratio = 0.0;
			}
			if music_voice.duration > 0.0 {
				let stop_position = (music_voice.start_offset + music_voice.elapsed_seconds + fade_seconds as f64) % music_voice.duration;
				self.music_resume_time_by_id.insert(music_voice.sound_id.clone(), stop_position);
			}
		}
	}

	//==============================================================================
	// 배경음악 틀기와 바꾸기. (첫 누름 뒤, AudioContext 가 돌고 받아 둔 것만, 음소거 중에도 소리 없이 돌아 풀면 이어 들린다)
	// - 바꾸라고 하면 앞 음악은 곧바로 줄이기 시작하고(멈춘 자리를 기억), 새 음악은 받아 두었으면 0 에서 올린다.
	//==============================================================================
	fn update_music(&mut self, engine: &Engine) {
		if !self.is_players_ready || !self.is_context_running() {
			return;
		}
		let is_requested_music_playing = self.music_voice_list.iter().any(|music_voice| music_voice.is_fading_in && music_voice.sound_id == self.music_id);
		if is_requested_music_playing {
			return;
		}
		self.fade_out_playing_music(self.music_fade_seconds);
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
		let audio_asset = match self.asset_by_id.get(&self.music_id).and_then(|asset_list| asset_list.first()) {
			Some(audio_asset) if audio_asset.is_loaded() => {
				audio_asset.clone()
			}
			_ => {
				return;
			}
		};
		let duration = audio_asset.get_duration();
		let stored_position = self.music_resume_time_by_id.get(&self.music_id).copied().unwrap_or(0.0);
		let start_offset = if self.is_music_resumed && duration > 0.0 {
			stored_position % duration
		}
		else {
			0.0
		};
		let mut audio_player = engine.get_audio_manager().create_audio_player();
		audio_player.set_audio_asset(audio_asset);
		audio_player.set_time(start_offset);
		audio_player.set_volume(0.0);
		audio_player.play(true);
		let fade_seconds = self.music_fade_seconds;
		let start_ratio = if fade_seconds > 0.0 {
			0.0
		}
		else {
			1.0
		};
		self.music_voice_list.push(MusicVoice {
			audio_player,
			sound_id: self.music_id.clone(),
			volume: clamp_sound_volume(sound_row.volume),
			fade_ratio: start_ratio,
			fade_seconds,
			is_fading_in: true,
			start_offset,
			elapsed_seconds: 0.0,
			duration,
		});
	}

	//==============================================================================
	// 매 프레임. (효과음의 앞 몇 초와 줄여 끄기, 음악 겹쳐 바꾸기, 끝난 소리 치우기, 그다음 원본 update)
	//==============================================================================
	pub fn update(&mut self, engine: &Engine, time_delta: f32) {
		self.attach_engine(engine);
		self.elapsed_seconds += time_delta;
		for voice in self.effect_voice_list.iter_mut() {
			voice.elapsed_seconds += time_delta;
			if voice.play_seconds > 0.0 && voice.fade_start_seconds < 0.0 && voice.elapsed_seconds >= voice.play_seconds {
				voice.fade_start_seconds = voice.elapsed_seconds;
				voice.fade_out_seconds = voice.fade_out_seconds.max(0.001);
			}
		}
		for music_voice in self.music_voice_list.iter_mut() {
			music_voice.elapsed_seconds += time_delta as f64;
			let step = if music_voice.fade_seconds > 0.0 { time_delta / music_voice.fade_seconds } else { 1.0 };
			music_voice.fade_ratio = if music_voice.is_fading_in { (music_voice.fade_ratio + step).min(1.0) } else { (music_voice.fade_ratio - step).max(0.0) };
		}
		self.effect_voice_list.retain_mut(|voice| {
			let is_faded_out = voice.fade_start_seconds >= 0.0 && voice.elapsed_seconds - voice.fade_start_seconds >= voice.fade_out_seconds;
			if is_faded_out {
				voice.audio_player.stop();
				return false;
			}
			voice.audio_player.is_playing() || voice.elapsed_seconds < 0.5
		});
		self.music_voice_list.retain_mut(|music_voice| {
			if !music_voice.is_fading_in && music_voice.fade_ratio <= 0.0 {
				music_voice.audio_player.stop();
				return false;
			}
			true
		});
		self.refresh(engine);
	}

	//==============================================================================
	// 원본 update. (깨워 보기, 첫 누름 뒤 소리의 길 만들기, 음소거와 컨텍스트 멈춤 따르기, 다시 깨우기, 배경음악 틀기, 크기 걸기)
	//==============================================================================
	fn refresh(&mut self, engine: &Engine) {
		self.try_autoplay(engine);
		self.ensure_players(engine);
		let is_stop_effects_requested = std::mem::take(&mut self.mute_state.borrow_mut().is_stop_effects_requested);
		if is_stop_effects_requested {
			self.stop_effects();
		}
		self.apply_mute();
		self.apply_context_pause();
		self.retry_context_resume(engine);
		self.update_music(engine);
		self.apply_volumes();
	}

	//==============================================================================
	// 누르기 전에 깨워 보기. (허락할 때 처음 한 번, 웹뷰가 막으면 AudioContext 가 멈춘 채로 남고 첫 누름에 엔진이 깨운다)
	//==============================================================================
	fn try_autoplay(&mut self, engine: &Engine) {
		if self.is_autoplay_tried {
			return;
		}
		self.is_autoplay_tried = true;
		if let Some(autoplay_allowed_provider) = &self.autoplay_allowed_provider {
			if !autoplay_allowed_provider() {
				return;
			}
		}
		let audio_manager = engine.get_audio_manager();
		audio_manager.mark_user_gesture();
		audio_manager.resume_context();
	}

	//==============================================================================
	// 소리의 길 만들기. (첫 누름 뒤 한 번, AudioContext 를 그 전에 만들지 않게, 없으면 플레이어를 하나 만들어 AudioContext 를 만든다)
	//==============================================================================
	fn ensure_players(&mut self, engine: &Engine) {
		if self.is_players_ready {
			return;
		}
		let audio_manager = engine.get_audio_manager();
		if !audio_manager.has_user_gesture() {
			return;
		}
		if !audio_manager.has_audio_context() {
			let _ = audio_manager.create_audio_player();
		}
		if !audio_manager.has_audio_context() {
			return;
		}
		self.is_players_ready = true;
		self.is_muted = false;
		self.apply_volumes();
	}

	//==============================================================================
	// 음소거 적용. (바뀔 때만, 음소거가 되면 내는 효과음을 끊는다, 소리의 길을 만들기 전에는 하지 않는다)
	//==============================================================================
	fn apply_mute(&mut self) {
		if !self.is_players_ready {
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
	// 음소거인 동안 AudioContext 멈추기와 풀리면 깨우기.
	//==============================================================================
	fn apply_context_pause(&mut self) {
		self.mute_state.borrow_mut().apply_context_pause();
	}

	//==============================================================================
	// 다시 깨워 보기. (음소거가 아닌데 AudioContext 가 돌지 않으면 CONTEXT_RESUME_RETRY_INTERVAL 마다)
	// - 아이폰은 앱으로 돌아온 순간(다시 보임)에 부른 resume() 을 오디오가 아직 돌아오기 전이라 듣지 않는 일이 있다.
	//==============================================================================
	fn retry_context_resume(&mut self, engine: &Engine) {
		let audio_manager = engine.get_audio_manager();
		let is_context_paused = self.mute_state.borrow().is_context_paused;
		if !audio_manager.has_audio_context() || is_context_paused || audio_manager.is_context_running() {
			return;
		}
		if self.elapsed_seconds - self.last_context_resume_time < CONTEXT_RESUME_RETRY_INTERVAL {
			return;
		}
		self.last_context_resume_time = self.elapsed_seconds;
		audio_manager.resume_context();
	}

	//==============================================================================
	// 플레이어마다 크기 걸기. (음소거면 0)
	//==============================================================================
	fn apply_volumes(&mut self) {
		let mute_scale = if self.is_muted { 0.0 } else { 1.0 };
		let sound_effect_volume = self.sound_effect_volume * mute_scale;
		for voice in self.effect_voice_list.iter_mut() {
			let fade_scale = if voice.fade_start_seconds >= 0.0 { (1.0 - (voice.elapsed_seconds - voice.fade_start_seconds) / voice.fade_out_seconds).clamp(0.0, 1.0) } else { 1.0 };
			voice.audio_player.set_volume(voice.volume * sound_effect_volume * fade_scale);
		}
		let music_volume = self.music_volume * self.music_scale * mute_scale;
		for music_voice in self.music_voice_list.iter_mut() {
			music_voice.audio_player.set_volume(music_voice.volume * music_volume * music_voice.fade_ratio);
		}
	}

	//==============================================================================
	// 내고 있는 효과음 수. (시험과 기록용)
	//==============================================================================
	pub fn get_effect_voice_count(&self) -> usize {
		self.effect_voice_list.len()
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
		assert!(audio_asset.load_from_bytes(create_silent_wav((seconds * 8000.0) as u32)).is_ok());
		assert!(audio_asset.is_loaded());
		audio_asset
	}

	#[test]
	fn interval_and_voice_limit() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut hit_row = SoundRow::create_effect("hit", &[], 1.0);
		hit_row.minimum_interval = 0.1;
		hit_row.maximum_voices = 2;
		let mut sound_system = SoundSystem::new(vec![hit_row]);
		sound_system.add_sound_asset("hit", create_silent_asset(0.2));
		sound_system.update(&engine, 0.0);
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_effect(&engine, "hit", 1.0, 0.0);
		sound_system.play_effect(&engine, "hit", 1.0, 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), 1);
		for _ in 0..4 {
			sound_system.update(&engine, 0.11);
			sound_system.play_effect(&engine, "hit", 1.0, -0.5);
		}
		assert!(sound_system.get_effect_voice_count() <= 2);

		// 너무 작은 크기는 내지 않고, "" 와 0 초면 모두 바로 끊는다.
		sound_system.update(&engine, 0.2);
		let voice_count = sound_system.get_effect_voice_count();
		sound_system.play_effect(&engine, "hit", 0.0005, 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), voice_count);
		sound_system.fade_out_effects("", 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), 0);
	}

	//==============================================================================
	// 누르기 전에 깨워 보기는 첫 update 에서 한 번만 허락을 묻는다.
	//==============================================================================
	#[test]
	fn autoplay_provider_is_asked_once() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(Vec::new());
		let asked_count = Rc::new(Cell::new(0));
		let asked_count_for_provider = asked_count.clone();
		sound_system.set_autoplay_allowed_provider(Some(Box::new(move || {
			asked_count_for_provider.set(asked_count_for_provider.get() + 1);
			false
		})));
		sound_system.update(&engine, 0.016);
		sound_system.update(&engine, 0.016);
		assert_eq!(asked_count.get(), 1);
	}

	//==============================================================================
	// 페이지 이벤트: 가려지면 음소거, 터치가 주된 기기에서 포커스를 잃으면 음소거, 누르거나 포커스를 얻거나 다시 보이면 풀린다.
	// - 마우스가 주된 기기(데스크탑)는 포커스를 잃어도 음소거하지 않는다.
	//==============================================================================
	#[test]
	fn page_events_mute_and_unmute() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(Vec::new());
		sound_system.update(&engine, 0.0);
		assert!(!sound_system.is_blur_mute_enabled());
		engine.dispatch_page_event(PageEvent::Blurred);
		assert!(!sound_system.is_window_blurred());
		assert!(!sound_system.read_muted());

		engine.dispatch_page_event(PageEvent::Hidden);
		assert!(sound_system.is_page_hidden());
		assert!(sound_system.read_muted());
		if sound_system.is_context_running() {
			assert!(sound_system.is_context_paused());
		}
		engine.dispatch_page_event(PageEvent::Visible);
		assert!(!sound_system.read_muted());
		assert!(!sound_system.is_context_paused());

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
		sound_system.set_external_mute_provider(Some(Box::new(|| true)));
		assert!(sound_system.read_muted());
	}

	//==============================================================================
	// 가려졌다 다시 보이는 사이에 update 가 없어도 내던 효과음은 끊긴다. (원본은 가려지는 순간 끊는다)
	//==============================================================================
	#[test]
	fn hidden_page_stops_effects() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(vec![SoundRow::create_effect("hit", &[], 1.0)]);
		sound_system.add_sound_asset("hit", create_silent_asset(1.0));
		sound_system.update(&engine, 0.0);
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_effect(&engine, "hit", 1.0, 0.0);
		assert_eq!(sound_system.get_effect_voice_count(), 1);
		engine.dispatch_page_event(PageEvent::Hidden);
		engine.dispatch_page_event(PageEvent::Visible);
		sound_system.update(&engine, 0.016);
		assert_eq!(sound_system.get_effect_voice_count(), 0);
	}

	//==============================================================================
	// 배경음악: 받기 전에 틀어 달라고 하면 기억했다가 받으면 튼다, "" 면 끈다, 멈췄던 자리에서 잇는다.
	//==============================================================================
	#[test]
	fn music_request_waits_and_resumes() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sound_system = SoundSystem::new(vec![SoundRow::create_music("field", "field.wav", 0.8), SoundRow::create_music("battle", "battle.wav", 1.0)]);
		sound_system.update(&engine, 0.0);
		if !sound_system.is_context_running() {
			eprintln!("소리 장치가 없어 건너뜁니다.");
			return;
		}
		sound_system.play_music(&engine, "field", 0.0, false);
		assert_eq!(sound_system.get_music_id(), "field");
		assert!(sound_system.music_voice_list.is_empty());
		sound_system.add_sound_asset("field", create_silent_asset(2.0));
		sound_system.add_sound_asset("battle", create_silent_asset(2.0));
		sound_system.update(&engine, 0.0);
		assert_eq!(sound_system.music_voice_list.len(), 1);
		assert_eq!(sound_system.music_voice_list[0].volume, 0.8);

		// 0.5 초 뒤 1 초 겹쳐 바꾸면, 앞 음악은 다 줄어든 1.5 초 자리를 기억한다.
		sound_system.update(&engine, 0.5);
		sound_system.play_music(&engine, "battle", 1.0, false);
		assert_eq!(sound_system.music_resume_time_by_id.get("field").copied(), Some(1.5));
		assert_eq!(sound_system.music_voice_list.len(), 2);
		sound_system.update(&engine, 1.1);
		assert_eq!(sound_system.music_voice_list.len(), 1);
		assert_eq!(sound_system.music_voice_list[0].sound_id, "battle");

		// 이어 틀기는 기억한 자리에서.
		sound_system.play_music(&engine, "field", 0.0, true);
		let field_voice = sound_system.music_voice_list.iter().find(|music_voice| music_voice.is_fading_in).expect("음악");
		assert_eq!(field_voice.sound_id, "field");
		assert_eq!(field_voice.start_offset, 1.5);

		// "" 면 끈다.
		sound_system.play_music(&engine, "", 0.0, false);
		assert_eq!(sound_system.get_music_id(), "");
		sound_system.update(&engine, 0.016);
		assert!(sound_system.music_voice_list.is_empty());
	}

	//==============================================================================
	// 크기는 0 ~ 1, 수가 아니면 0. (원본 clampVolume)
	//==============================================================================
	#[test]
	fn volume_setters_clamp() {
		let mut sound_system = SoundSystem::new(Vec::new());
		sound_system.set_music_volume(2.0);
		assert_eq!(sound_system.get_music_volume(), 1.0);
		sound_system.set_music_scale(f32::NAN);
		assert_eq!(sound_system.get_music_scale(), 0.0);
		sound_system.set_sound_effect_volume(-1.0);
		assert_eq!(sound_system.get_sound_effect_volume(), 0.0);
		assert_eq!(sound_system.get_last_context_resume_time(), f32::NEG_INFINITY);
		assert!(!sound_system.is_context_running());
	}
}
