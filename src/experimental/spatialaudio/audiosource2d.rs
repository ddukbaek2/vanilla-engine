//==============================================================================
//! 2D 노드를 따라가는 공간 소리 컴포넌트. (원본 src/experimental/spatialaudio/audiosource2d.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 매 프레임(tick) 노드의 전역 자리(피벗 점)를 WebAudio 좌표로 바꿔 SpatialAudioPlayer 의 자리에 건다. (바뀐 때만)
//!   화면 좌표는 아래가 +Y 라 WebAudio 좌표(위가 +Y)로 Y 를 뒤집고, 픽셀을 pixels_per_unit 으로 나눈다. Z 는 0 이다.
//! - 플레이어는 set_player 로 넣는다. (AudioContext 를 든 매니저가 필요해서 컴포넌트가 만들지 않는다)
//==============================================================================
use super::spatialaudioplayer::SpatialAudioPlayer;
use crate::base::vector2::Vector2;
use crate::base::vector3::Vector3;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::ComponentContext;

//==============================================================================
/// 화면 자리를 WebAudio 자리로. ((x / pixels_per_unit, -y / pixels_per_unit, depth))
/// - 원본처럼 f64 로 나눈 뒤 f32 로 반올림한다. (원본 fround(x / pixelsPerUnit), 배율과 깊이도 원본 number 처럼 f64)
///
/// # 인자
/// - `view_position`: 화면(뷰) 좌표의 자리. (아래가 +Y, 픽셀)
/// - `pixels_per_unit`: WebAudio 한 단위가 몇 픽셀인지. (0 보다 큼)
/// - `depth`: Z 값.
///
/// # 돌려줌
/// WebAudio 좌표의 자리. (위가 +Y)
///
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::convert_view_position_to_audio_position;
/// use vanilla::{Vector2, Vector3};
///
/// let position = convert_view_position_to_audio_position(Vector2::create(200.0, 100.0), 100.0, 0.0);
/// assert_eq!(position, Vector3::create(2.0, -1.0, 0.0));
/// ```
//==============================================================================
pub fn convert_view_position_to_audio_position(view_position: Vector2, pixels_per_unit: f64, depth: f64) -> Vector3 {
	Vector3::create((f64::from(view_position.x) / pixels_per_unit) as f32, (-f64::from(view_position.y) / pixels_per_unit) as f32, depth as f32)
}

//==============================================================================
/// 2D 노드를 따라가는 공간 소리 컴포넌트.
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::{AudioSource2D, SpatialAudioPlayer};
/// use vanilla::{AudioManager, NodeTree, Vector2};
///
/// let audio_manager = AudioManager::new();
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let torch_id = node_tree.create_child(root_id, "torch");
/// node_tree[torch_id].set_local_position(Vector2::create(300.0, 200.0));
/// let audio_source = node_tree[torch_id].add_component(AudioSource2D::new());
/// audio_source.set_pixels_per_unit(100.0);
/// audio_source.set_player(SpatialAudioPlayer::new(&audio_manager, None));
/// assert!(audio_source.get_player().is_some());
/// assert_eq!(audio_source.get_pixels_per_unit(), 100.0);
/// ```
//==============================================================================
pub struct AudioSource2D {
	component_base: ComponentBase,
	player: Option<SpatialAudioPlayer>,
	pixels_per_unit: f64,
}

#[crate::reflect]
impl AudioSource2D {
	//==============================================================================
	/// 생성. (플레이어 없음, 한 단위 1 픽셀)
	///
	/// # 돌려줌
	/// 새 컴포넌트.
	//==============================================================================
	pub fn new() -> AudioSource2D {
		AudioSource2D {
			component_base: ComponentBase::new(),
			player: None,
			pixels_per_unit: 1.0,
		}
	}

	//==============================================================================
	/// 따라갈 플레이어 넣기. (앞 플레이어는 버린다)
	///
	/// # 인자
	/// - `player`: 노드 자리를 걸 공간 소리 플레이어.
	//==============================================================================
	pub fn set_player(&mut self, player: SpatialAudioPlayer) {
		self.player = Some(player);
	}

	//==============================================================================
	/// 플레이어.
	///
	/// # 돌려줌
	/// 넣은 플레이어. (없으면 None)
	//==============================================================================
	pub fn get_player(&self) -> Option<&SpatialAudioPlayer> {
		self.player.as_ref()
	}

	//==============================================================================
	/// 플레이어. (변경용, 틀기와 설정)
	///
	/// # 돌려줌
	/// 넣은 플레이어. (없으면 None)
	//==============================================================================
	pub fn get_player_mut(&mut self) -> Option<&mut SpatialAudioPlayer> {
		self.player.as_mut()
	}

	//==============================================================================
	/// 플레이어 떼기. (원본 setPlayer(null), 뗀 플레이어를 돌려준다)
	///
	/// # 돌려줌
	/// 넣었던 플레이어. (없으면 None)
	//==============================================================================
	pub fn take_player(&mut self) -> Option<SpatialAudioPlayer> {
		self.player.take()
	}

	//==============================================================================
	/// WebAudio 한 단위의 픽셀 수 설정. (0 보다 크고 유한해야 받는다)
	///
	/// # 인자
	/// - `pixels_per_unit`: 한 단위가 몇 픽셀인지. (기본 1, 거리 모델의 기준과 최대 거리가 이 단위다)
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
}

impl Default for AudioSource2D {
	//==============================================================================
	/// 생성. (new 와 같다)
	///
	/// # 돌려줌
	/// 새 컴포넌트.
	//==============================================================================
	fn default() -> AudioSource2D {
		AudioSource2D::new()
	}
}

#[crate::reflect]
impl Component for AudioSource2D {
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
	/// "AudioSource2D".
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"AudioSource2D"
	}

	//==============================================================================
	/// 갱신. (노드의 전역 자리를 플레이어 자리에 건다)
	///
	/// # 인자
	/// - `context`: 자기 노드와 트리, 엔진.
	/// - `_time_delta`: 지난 프레임부터의 시간. (초, 쓰지 않는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let pixels_per_unit = self.pixels_per_unit;
		let Some(player) = self.player.as_mut() else {
			return;
		};
		let view_position = context.get_node_tree().get_position(context.get_node_id());
		player.set_position(convert_view_position_to_audio_position(view_position, pixels_per_unit, 0.0));
	}
}

#[cfg(test)]
mod tests {
	use super::super::audiolistener2d::AudioListener2D;
	use super::super::spatialaudiolistener::SpatialAudioListener;
	use super::*;
	use crate::core::audiomanager::AudioManager;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::NodeTree;

	//==============================================================================
	// 소리와 리스너 노드의 자리를 WebAudio 좌표로 따라간다. (Y 뒤집기, 단위, 깊이, 회전한 위)
	//==============================================================================
	#[test]
	fn components_follow_node_positions() {
		let engine = Engine::new(EngineConfiguration::new());
		let audio_manager = AudioManager::new();
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let source_id = node_tree.create_child(root_id, "source");
		node_tree[source_id].set_local_position(Vector2::create(300.0, 100.0));
		let audio_source = node_tree[source_id].add_component(AudioSource2D::new());
		audio_source.set_pixels_per_unit(100.0);
		audio_source.set_player(SpatialAudioPlayer::new(&audio_manager, None));
		let listener_id = node_tree.create_child(root_id, "listener");
		node_tree[listener_id].set_local_position(Vector2::create(100.0, 100.0));
		node_tree[listener_id].set_local_rotation(90.0);
		let audio_listener = node_tree[listener_id].add_component(AudioListener2D::new());
		audio_listener.set_pixels_per_unit(100.0);
		audio_listener.set_depth(2.0);
		audio_listener.set_listener(SpatialAudioListener::new(&audio_manager));
		node_tree.tick(&engine, 0.016);
		let player = node_tree[source_id].get_component::<AudioSource2D>().expect("source").get_player().expect("player");
		assert_eq!(player.get_position(), Vector3::create(3.0, -1.0, 0.0));
		let listener = node_tree[listener_id].get_component::<AudioListener2D>().expect("listener").get_listener().expect("listener");
		assert_eq!(listener.get_position(), Vector3::create(1.0, -1.0, 2.0));
		let up = listener.get_up();
		assert!((up.x - 1.0).abs() < 1e-6 && up.y.abs() < 1e-6);
		assert_eq!(listener.get_forward(), Vector3::create(0.0, 0.0, -1.0));
		assert!(node_tree[listener_id].get_component_mut::<AudioListener2D>().expect("listener").get_listener_mut().is_some());
		assert!(node_tree[listener_id].get_component_mut::<AudioListener2D>().expect("listener").take_listener().is_some());
		assert!(node_tree[listener_id].get_component::<AudioListener2D>().expect("listener").get_listener().is_none());
		assert!(node_tree[source_id].get_component_mut::<AudioSource2D>().expect("source").take_player().is_some());
	}

	//==============================================================================
	// 화면 자리 바꾸기는 원본처럼 f64 로 나눈 뒤 f32 로 반올림한다. (f32 끼리 나누면 23.333332, 원본 시험과 같은 값)
	//==============================================================================
	#[test]
	fn view_position_divides_in_f64() {
		let position = convert_view_position_to_audio_position(Vector2::create(7.0, 7.0), 0.3, 1.0 / 3.0);
		assert_eq!(position, Vector3::create(23.333334, -23.333334, 0.33333334));
	}

	//==============================================================================
	// 크롬처럼 길이 0 의 사이 각은 0 이다. (fmin(NaN, 1) 이 1, chrome_spatial.json 의 static_same_position_cone, hrtf_listener_forward_parallel_up_44100)
	//==============================================================================
	#[test]
	fn zero_length_angles_follow_chrome() {
		use super::super::{calculate_azimuth_elevation, calculate_cone_gain};
		let origin = Vector3::create(0.0, 0.0, 0.0);
		assert_eq!(calculate_cone_gain(origin, Vector3::create(1.0, 0.0, 0.0), origin, 30.0, 90.0, 0.1), 1.0);
		let (azimuth, elevation) = calculate_azimuth_elevation(Vector3::create(2.0, 1.0, -1.0), origin, Vector3::create(0.0, 1.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		assert_eq!((azimuth, elevation), (0.0, 90.0));
	}
}
