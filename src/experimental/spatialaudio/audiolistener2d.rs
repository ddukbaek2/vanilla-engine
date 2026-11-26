//==============================================================================
//! 2D 노드를 따라가는 리스너 컴포넌트. (원본 src/experimental/spatialaudio/audiolistener2d.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 매 프레임(tick) 노드의 전역 자리를 WebAudio 좌표로 바꿔 리스너 자리에 건다. (바뀐 때만) 보통 카메라나 주인공 노드에 붙인다.
//!   앞은 화면 안쪽 (0, 0, -1)이고, 위는 노드의 전역 회전을 따라 돈다. (회전 0 이면 (0, 1, 0), 화면 위)
//! - 깊이(set_depth)를 주면 리스너가 화면에서 그만큼 떨어져 있어, 가까운 소리가 갑자기 한쪽으로 쏠리지 않는다. (기본 0)
//==============================================================================
use super::audiosource2d::convert_view_position_to_audio_position;
use super::spatialaudiolistener::SpatialAudioListener;
use crate::base::math;
use crate::base::vector3::Vector3;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::ComponentContext;

//==============================================================================
/// 2D 노드를 따라가는 리스너 컴포넌트.
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::{AudioListener2D, SpatialAudioListener};
/// use vanilla::{AudioManager, NodeTree};
///
/// let audio_manager = AudioManager::new();
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let camera_id = node_tree.create_child(root_id, "camera");
/// let audio_listener = node_tree[camera_id].add_component(AudioListener2D::new());
/// audio_listener.set_pixels_per_unit(100.0);
/// audio_listener.set_depth(3.0);
/// audio_listener.set_listener(SpatialAudioListener::new(&audio_manager));
/// assert_eq!(audio_listener.get_depth(), 3.0);
/// ```
//==============================================================================
pub struct AudioListener2D {
	component_base: ComponentBase,
	listener: Option<SpatialAudioListener>,
	pixels_per_unit: f64,
	depth: f64,
}

#[crate::reflect]
impl AudioListener2D {
	//==============================================================================
	/// 생성. (리스너 없음, 한 단위 1 픽셀, 깊이 0)
	///
	/// # 돌려줌
	/// 새 컴포넌트.
	//==============================================================================
	pub fn new() -> AudioListener2D {
		AudioListener2D {
			component_base: ComponentBase::new(),
			listener: None,
			pixels_per_unit: 1.0,
			depth: 0.0,
		}
	}

	//==============================================================================
	/// 따라갈 리스너 넣기.
	///
	/// # 인자
	/// - `listener`: 노드 자리를 걸 리스너.
	//==============================================================================
	pub fn set_listener(&mut self, listener: SpatialAudioListener) {
		self.listener = Some(listener);
	}

	//==============================================================================
	/// 리스너.
	///
	/// # 돌려줌
	/// 넣은 리스너. (없으면 None)
	//==============================================================================
	pub fn get_listener(&self) -> Option<&SpatialAudioListener> {
		self.listener.as_ref()
	}

	//==============================================================================
	/// 리스너. (변경용, 원본 getListener 가 돌려준 참조로 고치는 자리)
	///
	/// # 돌려줌
	/// 넣은 리스너. (없으면 None)
	//==============================================================================
	pub fn get_listener_mut(&mut self) -> Option<&mut SpatialAudioListener> {
		self.listener.as_mut()
	}

	//==============================================================================
	/// 리스너 떼기. (원본 setListener(null), 뗀 리스너를 돌려준다)
	///
	/// # 돌려줌
	/// 넣었던 리스너. (없으면 None)
	//==============================================================================
	pub fn take_listener(&mut self) -> Option<SpatialAudioListener> {
		self.listener.take()
	}

	//==============================================================================
	/// WebAudio 한 단위의 픽셀 수 설정. (0 보다 크고 유한해야 받는다, AudioSource2D 와 같은 값을 쓴다)
	///
	/// # 인자
	/// - `pixels_per_unit`: 한 단위가 몇 픽셀인지. (기본 1)
	//==============================================================================
	pub fn set_pixels_per_unit(&mut self, pixels_per_unit: f64) {
		if pixels_per_unit.is_finite() && pixels_per_unit > 0.0 {
			self.pixels_per_unit = pixels_per_unit;
		}
	}

	//==============================================================================
	/// WebAudio 한 단위의 픽셀 수.
	///
	/// # 돌려줌
	/// 한 단위가 몇 픽셀인지. (기본 1)
	//==============================================================================
	pub fn get_pixels_per_unit(&self) -> f64 {
		self.pixels_per_unit
	}

	//==============================================================================
	/// 깊이 설정. (리스너가 화면에서 떨어진 거리, WebAudio 단위, 유한해야 받는다)
	///
	/// # 인자
	/// - `depth`: 리스너의 Z. (기본 0, 소리는 Z 0 에 있다)
	//==============================================================================
	pub fn set_depth(&mut self, depth: f64) {
		if depth.is_finite() {
			self.depth = depth;
		}
	}

	//==============================================================================
	/// 깊이.
	///
	/// # 돌려줌
	/// 리스너의 Z. (기본 0)
	//==============================================================================
	pub fn get_depth(&self) -> f64 {
		self.depth
	}
}

impl Default for AudioListener2D {
	//==============================================================================
	/// 생성. (new 와 같다)
	///
	/// # 돌려줌
	/// 새 컴포넌트.
	//==============================================================================
	fn default() -> AudioListener2D {
		AudioListener2D::new()
	}
}

#[crate::reflect]
impl Component for AudioListener2D {
	//==============================================================================
	/// 공통 상태 반환.
	///
	/// # 돌려줌
	/// 컴포넌트 공통 상태.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	///
	/// # 돌려줌
	/// 컴포넌트 공통 상태. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 이름.
	///
	/// # 돌려줌
	/// "AudioListener2D".
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"AudioListener2D"
	}

	//==============================================================================
	/// 갱신. (노드의 전역 자리와 회전을 리스너에 건다)
	///
	/// # 인자
	/// - `context`: 자기 노드와 트리, 엔진.
	/// - `_time_delta`: 지난 프레임부터의 시간. (초, 쓰지 않는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f64) {
		let (pixels_per_unit, depth) = (self.pixels_per_unit, self.depth);
		let Some(listener) = self.listener.as_mut() else {
			return;
		};
		let node_tree = context.get_node_tree();
		let node_id = context.get_node_id();
		let view_position = node_tree.get_position(node_id);
		let radian = math::degree_to_radian(node_tree.get_rotation(node_id));
		listener.set_position(convert_view_position_to_audio_position(view_position, pixels_per_unit, depth));
		// 화면 위 (0, -1) 을 시계 방향으로 돌린 (sin, -cos) 의 Y 를 뒤집는다.
		listener.set_orientation(Vector3::create(0.0, 0.0, -1.0), Vector3::create(math::sin(radian), math::cos(radian), 0.0));
	}
}
