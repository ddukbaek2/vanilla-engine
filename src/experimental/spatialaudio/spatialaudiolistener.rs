//==============================================================================
//! 공간 소리 리스너. (원본 src/experimental/spatialaudio/spatialaudiolistener.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - AudioContext 의 리스너(AudioListener)는 컨텍스트에 하나다. 이 값을 여럿 만들어도 같은 리스너를 고친다.
//!   (값은 이 값이 기억하는 것이라, 여럿이 서로 다른 값을 들면 마지막에 건 것이 리스너에 남는다)
//! - 자리, 앞, 위는 리스너의 positionX ~ upZ 에 값으로 대입한다. (바뀐 칸만) 웹에서 그 AudioParam 이 없는 옛 브라우저는 setPosition, setOrientation 을 쓴다.
//! - 만들 때 AudioContext 가 없으면 만든다. (AudioManager::create_audio_player 와 같다)
//==============================================================================
use crate::base::vector3::Vector3;
use crate::core::audiomanager::{AudioManager, AudioManagerState};
use crate::platform::audio::{self as audio_backend, ListenerParam};
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
/// 공간 소리 리스너.
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::SpatialAudioListener;
/// use vanilla::{AudioManager, Vector3};
///
/// let audio_manager = AudioManager::new();
/// let mut listener = SpatialAudioListener::new(&audio_manager);
/// listener.set_position(Vector3::create(1.0, 2.0, 3.0));
/// listener.set_orientation(Vector3::create(1.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// assert_eq!(listener.get_forward(), Vector3::create(1.0, 0.0, 0.0));
/// assert_eq!(listener.get_up(), Vector3::create(0.0, 1.0, 0.0));
/// ```
//==============================================================================
pub struct SpatialAudioListener {
	audio_manager_state: Rc<RefCell<AudioManagerState>>,
	// 자리, 앞, 위. (리스너에 건 값)
	value_list: [f32; 9],
}

impl SpatialAudioListener {
	//==============================================================================
	/// 생성. (자리 (0, 0, 0), 앞 (0, 0, -1), 위 (0, 1, 0), 리스너에는 아직 아무것도 걸지 않는다)
	///
	/// # 인자
	/// - `audio_manager`: 엔진의 AudioManager. (AudioContext 가 없으면 만든다)
	///
	/// # 돌려줌
	/// 새 리스너.
	//==============================================================================
	pub fn new(audio_manager: &AudioManager) -> SpatialAudioListener {
		let audio_manager_state = audio_manager.get_shared_state();
		audio_manager_state.borrow_mut().ensure_audio_context();
		SpatialAudioListener {
			audio_manager_state,
			value_list: [0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0],
		}
	}

	//==============================================================================
	/// 자리 설정. (WebAudio 좌표, 유한하지 않은 칸은 받지 않는다)
	///
	/// # 인자
	/// - `position`: 자리.
	//==============================================================================
	pub fn set_position(&mut self, position: Vector3) {
		self.write_values(0, position);
	}

	//==============================================================================
	/// 자리.
	///
	/// # 돌려줌
	/// 자리.
	//==============================================================================
	pub fn get_position(&self) -> Vector3 {
		self.read_values(0)
	}

	//==============================================================================
	/// 방향 설정. (앞과 위, 길이는 상관없다. 기본 앞 (0, 0, -1), 위 (0, 1, 0))
	///
	/// # 인자
	/// - `forward`: 앞.
	/// - `up`: 위.
	//==============================================================================
	pub fn set_orientation(&mut self, forward: Vector3, up: Vector3) {
		self.write_values(3, forward);
		self.write_values(6, up);
	}

	//==============================================================================
	/// 앞.
	///
	/// # 돌려줌
	/// 앞.
	//==============================================================================
	pub fn get_forward(&self) -> Vector3 {
		self.read_values(3)
	}

	//==============================================================================
	/// 위.
	///
	/// # 돌려줌
	/// 위.
	//==============================================================================
	pub fn get_up(&self) -> Vector3 {
		self.read_values(6)
	}

	//==============================================================================
	/// 값 셋 읽기.
	///
	/// # 인자
	/// - `first_index`: 첫 칸.
	///
	/// # 돌려줌
	/// 값.
	//==============================================================================
	fn read_values(&self, first_index: usize) -> Vector3 {
		Vector3::create(self.value_list[first_index], self.value_list[first_index + 1], self.value_list[first_index + 2])
	}

	//==============================================================================
	/// 바뀐 칸만 리스너에 대입한다.
	///
	/// # 인자
	/// - `first_index`: 첫 칸.
	/// - `vector`: 새 값.
	//==============================================================================
	fn write_values(&mut self, first_index: usize, vector: Vector3) {
		const PARAM_LIST: [ListenerParam; 9] = [ListenerParam::PositionX, ListenerParam::PositionY, ListenerParam::PositionZ, ListenerParam::ForwardX, ListenerParam::ForwardY, ListenerParam::ForwardZ, ListenerParam::UpX, ListenerParam::UpY, ListenerParam::UpZ];
		let mut state = self.audio_manager_state.borrow_mut();
		for (offset, value) in [vector.x, vector.y, vector.z].into_iter().enumerate() {
			let index = first_index + offset;
			if !value.is_finite() || value == self.value_list[index] {
				continue;
			}
			self.value_list[index] = value;
			audio_backend::set_listener_param_value(&mut state, PARAM_LIST[index], value);
		}
	}
}
