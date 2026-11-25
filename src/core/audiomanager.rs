//==============================================================================
//! 오디오 매니저.
//! - 원본 core/audiomanager.js. 공유 AudioContext 를 들고 AudioPlayer 생성을 맡는다.
//! - AudioContext 자리는 백엔드가 채운다. (platform/audio)
//!   데스크탑: cpal 출력 위의 엔진 믹서(platform/audio/mixer.rs, 크롬 WebAudio 와 같은 셈). 처음 쓸 때 소리 장치를 연다. 멈추면(SoundSystem 음소거) 소리와 시계가 원본처럼 그 자리에서 선다.
//!   웹: 원본과 같은 WebAudio AudioContext. 원본처럼 user gesture 뒤에 만들고, iOS 에서는 복원 뒤 시계를 확인한다.
//! - 원본처럼 AudioContext 는 늦게 만든다. resume_context() 는 user gesture 전에는 아무것도 하지 않고,
//!   create_audio_player() 는 user gesture 전이라도 (마지못해) 만든다.
//!   데스크탑에는 브라우저 자동재생 정책이 없어 처음부터 user gesture 가 있었던 것으로 둔다.
//! - 원본과 다른 점:
//!   상태를 `Rc<RefCell>` 로 나눠 든다. 웹의 시계 확인 타이머와 프로미스 콜백이 원본처럼 매니저를 다시 부르고,
//!   BeepPlayer 가 원본처럼 매니저를 참조로 들기 때문이다. 그래서 메서드는 모두 &self 로 부른다. (컴포넌트는 &Engine 만 받는다)
//!   AudioPlayer 는 원본처럼 만든 뒤 부른 쪽이 소유한다. 매니저는 만든 플레이어를 들고 있지 않는다.
//!   원본 getAudioContext() 는 빌린 AudioContext(웹 WebAudio AudioContext, 데스크탑 엔진 믹서의 AudioContext, 둘 다 sample_rate()와 current_time() 이 있다)를 돌려준다. 없으면 None, has_audio_context() 도 있다.
//!   AudioManager 는 나눠 든 상태의 손잡이라 Clone 하면 같은 AudioContext 를 보는 손잡이가 하나 더 생긴다. (SoundSystem 이 원본처럼 매니저를 들 때)
//!   생성자는 엔진을 받지 않는다. (원본은 iOS 판별에 엔진의 플랫폼 정보를 쓰고, 여기서는 웹 백엔드가 같은 규칙으로 직접 판별한다)
//!   create_audio_bus(원본 createAudioBus)는 AudioBus 를 만든다. 버스도 플레이어처럼 부른 쪽이 소유한다.
//!   create_audio_player 와 create_audio_bus 는 원본처럼 AudioPlayer::new(매니저, None), AudioBus::new(매니저) 와 같다.
//==============================================================================
use crate::core::audiobus::AudioBus;
use crate::core::audioplayer::AudioPlayer;
use crate::platform::audio::{self as audio_backend, AudioContext, AudioPlatformState};
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
/// 오디오 매니저가 나눠 드는 상태. (웹 백엔드의 타이머와 콜백, BeepPlayer 가 같이 본다)
//==============================================================================
pub(crate) struct AudioManagerState {
	pub(crate) audio_context: Option<AudioContext>,
	pub(crate) has_user_gesture: bool,
	pub(crate) platform_state: AudioPlatformState,
}

impl AudioManagerState {
	//==============================================================================
	/// AudioContext 를 보장한다. (없으면 만든다. 만들지 못하면 None)
	//==============================================================================
	pub(crate) fn ensure_audio_context(&mut self) -> Option<&mut AudioContext> {
		if self.audio_context.is_none() {
			self.audio_context = audio_backend::create_audio_context(&mut self.platform_state);
		}
		self.audio_context.as_mut()
	}

	//==============================================================================
	/// AudioContext 가 있고 돌고 있는지. (원본 audio_context !== null && state === "running", 데스크탑은 멈춰 두지 않았으면)
	//==============================================================================
	pub(crate) fn is_context_running(&self) -> bool {
		audio_backend::is_audio_context_running(self)
	}
}

//==============================================================================
/// 오디오 컨텍스트 재개. (AudioManager::resume_context 의 몸통. 웹 백엔드의 타이머도 이걸 다시 부른다)
/// - user gesture 전에는 아무것도 하지 않는다. (브라우저 경고 회피)
//==============================================================================
pub(crate) fn resume_audio_manager_state(state_cell: &Rc<RefCell<AudioManagerState>>) {
	{
		let mut state = state_cell.borrow_mut();
		if !state.has_user_gesture {
			return;
		}
		if state.ensure_audio_context().is_none() {
			return;
		}
	}
	audio_backend::resume_audio_context(state_cell);
}

//==============================================================================
/// 오디오 매니저.
//==============================================================================
#[derive(Clone)]
pub struct AudioManager {
	state: Rc<RefCell<AudioManagerState>>,
}

impl AudioManager {
	//==============================================================================
	/// 생성. (AudioContext 는 아직 만들지 않는다)
	//==============================================================================
	pub fn new() -> AudioManager {
		let platform_state = audio_backend::create_audio_platform_state();
		AudioManager {
			state: Rc::new(RefCell::new(AudioManagerState {
				audio_context: None,
				has_user_gesture: audio_backend::HAS_USER_GESTURE_AT_START,
				platform_state,
			})),
		}
	}

	//==============================================================================
	/// 첫 user gesture 가 발생했음을 알린다. (Engine::resume_on_user_gesture 에서 부른다)
	//==============================================================================
	pub fn mark_user_gesture(&self) {
		self.state.borrow_mut().has_user_gesture = true;
	}

	//==============================================================================
	/// user gesture 이후인지 여부. (데스크탑은 늘 참)
	//==============================================================================
	pub fn has_user_gesture(&self) -> bool {
		self.state.borrow().has_user_gesture
	}

	//==============================================================================
	/// 오디오 컨텍스트 재개.
	/// - user gesture 전에는 아무것도 하지 않는다.
	/// - 웹: "suspended" 나 "interrupted" 일 때만 resume() 하고, 거부는 조용히 넘긴다. iOS 는 복원 뒤 시계를 확인한다.
	/// - 데스크탑: 소리 장치를 아직 열지 않았으면 열고, 멈춰 두었으면 다시 돌린다.
	//==============================================================================
	pub fn resume_context(&self) {
		resume_audio_manager_state(&self.state);
	}

	//==============================================================================
	/// 오디오 플레이어 생성. (AudioContext 가 없으면 만든다. 만들지 못하면 소리 없는 플레이어)
	//==============================================================================
	pub fn create_audio_player(&self) -> AudioPlayer {
		AudioPlayer::new(self, None)
	}

	//==============================================================================
	/// 오디오 버스 생성. (AudioContext 가 없으면 만든다. 만들지 못하면 소리 없는 버스)
	//==============================================================================
	pub fn create_audio_bus(&self) -> AudioBus {
		AudioBus::new(self)
	}

	//==============================================================================
	/// AudioContext 가 있고 돌고 있는지. (SoundSystem 이 소리를 낼지 정할 때)
	//==============================================================================
	pub fn is_context_running(&self) -> bool {
		self.state.borrow().is_context_running()
	}

	//==============================================================================
	/// AudioContext 시계. (초, 원본 audio_context.currentTime, AudioContext 가 없으면 0)
	//==============================================================================
	pub fn get_context_time(&self) -> f64 {
		audio_backend::get_audio_context_time(&self.state.borrow())
	}

	//==============================================================================
	/// AudioContext. (원본 getAudioContext, 아직 만들지 않았으면 None, 빌린 동안 매니저의 다른 함수를 부르지 않는다)
	//==============================================================================
	pub fn get_audio_context(&self) -> Option<std::cell::Ref<'_, AudioContext>> {
		std::cell::Ref::filter_map(self.state.borrow(), |state| state.audio_context.as_ref()).ok()
	}

	//==============================================================================
	/// AudioContext 가 만들어져 있는지. (원본 getAudioContext() !== null 자리)
	//==============================================================================
	pub fn has_audio_context(&self) -> bool {
		self.state.borrow().audio_context.is_some()
	}

	//==============================================================================
	/// 나눠 드는 상태. (BeepPlayer 가 원본처럼 매니저를 참조로 들 때)
	//==============================================================================
	pub(crate) fn get_shared_state(&self) -> Rc<RefCell<AudioManagerState>> {
		self.state.clone()
	}
}

impl Default for AudioManager {
	fn default() -> AudioManager {
		AudioManager::new()
	}
}
