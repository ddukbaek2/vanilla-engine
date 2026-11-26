//==============================================================================
//! 2D 물리의 몸. (강체)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/body2d.js 와 같은 API, 같은 셈이다.
//! - 자리는 몸의 원점, 속도는 질량 중심의 속도다. (Box2D, 유니티 Rigidbody2D 와 같다)
//! - 원본과 다른 점: 사용자 값은 PhysicsUserData2D(Rc<dyn Any>, 견줌은 같은 값을 가리키는지)다. 벡터는 f64 의 PhysicsVector2 다.
//==============================================================================
// 옮긴 원문: Box2D 3 의 셈에서 옮김, MIT, THIRD_PARTY_LICENSES.md
use crate::experimental::jsmath::js_max;
use super::physicsid2d::{BodyId2D, ShapeId2D};
use super::physicsmath2d::{PhysicsMath2D, Rotation2D};
use super::physicsuserdata2d::PhysicsUserData2D;
use super::physicsvector2::PhysicsVector2;

//==============================================================================
/// 몸의 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyType2D {
	/// 정적. (움직이지 않고 질량이 무한한 것처럼 군다, 땅과 벽)
	Static = 0,
	/// 운동학. (속도대로만 움직이고 힘과 부딪힘에 밀리지 않는다, 움직이는 발판)
	Kinematic = 1,
	/// 동적. (힘, 중력, 부딪힘으로 움직인다)
	Dynamic = 2,
}

//==============================================================================
/// 몸 정의. (create_body 에 넘기는 값)
///
/// # 예제
/// 떨어지는 동적 몸 정의
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, PhysicsVector2};
///
/// let mut body_definition = BodyDefinition2D::new();
/// body_definition.body_type = BodyType2D::Dynamic;
/// body_definition.position = PhysicsVector2::create(100.0, 50.0);
/// body_definition.angle = 0.3;
/// assert_eq!(body_definition.gravity_scale, 1.0);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct BodyDefinition2D {
	/// 종류. (기본 정적)
	pub body_type: BodyType2D,
	/// 몸의 원점 자리. (월드)
	pub position: PhysicsVector2,
	/// 돌림. (라디안)
	pub angle: f64,
	/// 처음 선속도. (질량 중심의 속도)
	pub linear_velocity: PhysicsVector2,
	/// 처음 각속도. (초당 라디안)
	pub angular_velocity: f64,
	/// 선속도 감쇠. (기본 0)
	pub linear_damping: f64,
	/// 각속도 감쇠. (기본 0)
	pub angular_damping: f64,
	/// 중력 배율. (기본 1)
	pub gravity_scale: f64,
	/// 잠들 수 있는지. (기본 true)
	pub enable_sleep: bool,
	/// 깨어 있는 채로 시작할지. (기본 true)
	pub is_awake: bool,
	/// 돌지 않게 할지. (기본 false)
	pub is_fixed_rotation: bool,
	/// 사용자 값.
	pub user_data: PhysicsUserData2D,
}

impl Default for BodyDefinition2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> BodyDefinition2D {
		BodyDefinition2D::new()
	}
}

impl BodyDefinition2D {
	//==============================================================================
	/// 생성. (원점의 정적 몸)
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new() -> BodyDefinition2D {
		BodyDefinition2D {
			body_type: BodyType2D::Static,
			position: PhysicsVector2::zero(),
			angle: 0.0,
			linear_velocity: PhysicsVector2::zero(),
			angular_velocity: 0.0,
			linear_damping: 0.0,
			angular_damping: 0.0,
			gravity_scale: 1.0,
			enable_sleep: true,
			is_awake: true,
			is_fixed_rotation: false,
			user_data: PhysicsUserData2D::default(),
		}
	}
}

//==============================================================================
/// 몸. (강체 하나)
/// - PhysicsWorld2D::create_body 로 만들고 `get_body(body_id)`, `world[body_id]` 로 얻는다.
/// - 속도, 힘, 자리를 바꾸면 몸이 깬다. 잠든 섬의 다른 몸도 다음 걸음에 같이 깬다.
///
/// # 예제
/// 몸에 충격 주기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, PhysicsVector2, PhysicsWorld2D, Polygon2D, ShapeDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let mut body_definition = BodyDefinition2D::new();
/// body_definition.body_type = BodyType2D::Dynamic;
/// let body_id = world.create_body(&body_definition);
/// world.create_shape(body_id, &ShapeDefinition2D::new(), Polygon2D::make_box(0.5, 0.5));
/// world[body_id].apply_linear_impulse_to_center(PhysicsVector2::create(2.0, 0.0), true);
/// world.step(1.0 / 60.0, 4);
/// assert_eq!(world[body_id].get_mass(), 1.0);
/// assert!((world[body_id].get_linear_velocity().x - 2.0).abs() < 1e-12);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Body2D {
	pub(crate) id: BodyId2D,
	pub(crate) body_type: BodyType2D,
	pub(crate) position_x: f64,
	pub(crate) position_y: f64,
	pub(crate) rotation_cosine: f64,
	pub(crate) rotation_sine: f64,
	pub(crate) center_x: f64,
	pub(crate) center_y: f64,
	pub(crate) local_center_x: f64,
	pub(crate) local_center_y: f64,
	pub(crate) previous_position_x: f64,
	pub(crate) previous_position_y: f64,
	pub(crate) previous_rotation_cosine: f64,
	pub(crate) previous_rotation_sine: f64,
	pub(crate) linear_velocity_x: f64,
	pub(crate) linear_velocity_y: f64,
	pub(crate) angular_velocity: f64,
	pub(crate) force_x: f64,
	pub(crate) force_y: f64,
	pub(crate) torque: f64,
	pub(crate) mass: f64,
	pub(crate) inverse_mass: f64,
	pub(crate) rotational_inertia: f64,
	pub(crate) inverse_inertia: f64,
	pub(crate) linear_damping: f64,
	pub(crate) angular_damping: f64,
	pub(crate) gravity_scale: f64,
	pub(crate) is_awake: bool,
	pub(crate) enable_sleep: bool,
	pub(crate) sleep_time: f64,
	pub(crate) is_fixed_rotation: bool,
	pub(crate) is_transform_dirty: bool,
	pub(crate) is_mass_dirty: bool,
	pub(crate) maximum_extent: f64,
	pub(crate) shape_id_list: Vec<ShapeId2D>,
	pub(crate) user_data: PhysicsUserData2D,
	pub(crate) solver_index: usize,
	pub(crate) island_parent: usize,
}

/// 풀이 몸이 없거나 섬에 들지 않은 몸의 번호.
pub(crate) const NO_INDEX: usize = usize::MAX;

impl Body2D {
	//==============================================================================
	/// 빈 몸 자료. (월드가 채운다)
	//==============================================================================
	pub(crate) fn new_empty(id: BodyId2D) -> Body2D {
		Body2D {
			id,
			body_type: BodyType2D::Static,
			position_x: 0.0,
			position_y: 0.0,
			rotation_cosine: 1.0,
			rotation_sine: 0.0,
			center_x: 0.0,
			center_y: 0.0,
			local_center_x: 0.0,
			local_center_y: 0.0,
			previous_position_x: 0.0,
			previous_position_y: 0.0,
			previous_rotation_cosine: 1.0,
			previous_rotation_sine: 0.0,
			linear_velocity_x: 0.0,
			linear_velocity_y: 0.0,
			angular_velocity: 0.0,
			force_x: 0.0,
			force_y: 0.0,
			torque: 0.0,
			mass: 0.0,
			inverse_mass: 0.0,
			rotational_inertia: 0.0,
			inverse_inertia: 0.0,
			linear_damping: 0.0,
			angular_damping: 0.0,
			gravity_scale: 1.0,
			is_awake: true,
			enable_sleep: true,
			sleep_time: 0.0,
			is_fixed_rotation: false,
			is_transform_dirty: false,
			is_mass_dirty: false,
			maximum_extent: 0.0,
			shape_id_list: Vec::new(),
			user_data: PhysicsUserData2D::default(),
			solver_index: NO_INDEX,
			island_parent: NO_INDEX,
		}
	}

	//==============================================================================
	/// 질량 중심을 원점과 회전에서 다시 셈.
	//==============================================================================
	pub(crate) fn update_center(&mut self) {
		self.center_x = self.rotation_cosine * self.local_center_x - self.rotation_sine * self.local_center_y + self.position_x;
		self.center_y = self.rotation_sine * self.local_center_x + self.rotation_cosine * self.local_center_y + self.position_y;
	}

	//==============================================================================
	/// 아이디 반환.
	///
	/// # 돌려줌
	/// 이 몸의 아이디.
	//==============================================================================
	pub fn get_id(&self) -> BodyId2D {
		self.id
	}

	//==============================================================================
	/// 종류 반환.
	///
	/// # 돌려줌
	/// 종류.
	//==============================================================================
	pub fn get_type(&self) -> BodyType2D {
		self.body_type
	}

	//==============================================================================
	/// 원점 자리 반환.
	///
	/// # 돌려줌
	/// 몸 원점의 월드 자리.
	//==============================================================================
	pub fn get_position(&self) -> PhysicsVector2 {
		PhysicsVector2::create(self.position_x, self.position_y)
	}

	//==============================================================================
	/// 돌림 반환. (결정론 아크탄젠트)
	///
	/// # 돌려줌
	/// 각도. (라디안, -파이 ~ 파이)
	//==============================================================================
	pub fn get_angle(&self) -> f64 {
		PhysicsMath2D::compute_atan2(self.rotation_sine, self.rotation_cosine)
	}

	//==============================================================================
	/// 회전 반환. (코사인, 사인)
	///
	/// # 돌려줌
	/// 회전.
	//==============================================================================
	pub fn get_rotation(&self) -> Rotation2D {
		Rotation2D::create(self.rotation_cosine, self.rotation_sine)
	}

	//==============================================================================
	/// 자리와 돌림을 바로 옮기기. (순간 이동, 몸을 깨운다)
	///
	/// # 인자
	/// - `position`: 원점의 새 자리.
	/// - `angle`: 새 돌림. (라디안)
	//==============================================================================
	pub fn set_transform(&mut self, position: impl Into<PhysicsVector2>, angle: f64) {
		let position = position.into();
		let rotation = PhysicsMath2D::compute_rotation(angle);
		self.position_x = position.x;
		self.position_y = position.y;
		self.rotation_cosine = rotation.cosine;
		self.rotation_sine = rotation.sine;
		self.previous_position_x = self.position_x;
		self.previous_position_y = self.position_y;
		self.previous_rotation_cosine = self.rotation_cosine;
		self.previous_rotation_sine = self.rotation_sine;
		self.update_center();
		self.is_transform_dirty = true;
		self.wake();
	}

	//==============================================================================
	/// 운동학 몸을 다음 걸음에 목표 자리로 옮길 속도 정하기. (유니티 MovePosition, Box2D b2Body_SetTargetTransform 과 같은 쓰임)
	///
	/// # 인자
	/// - `position`: 목표 원점 자리.
	/// - `angle`: 목표 돌림. (라디안)
	/// - `time_step`: 다음 걸음의 시간. (초)
	//==============================================================================
	pub fn set_target_transform(&mut self, position: impl Into<PhysicsVector2>, angle: f64, time_step: f64) {
		let position = position.into();
		if self.body_type == BodyType2D::Static || time_step <= 0.0 || time_step.is_nan() {
			return;
		}
		let rotation = PhysicsMath2D::compute_rotation(angle);
		let target_center_x = rotation.cosine * self.local_center_x - rotation.sine * self.local_center_y + position.x;
		let target_center_y = rotation.sine * self.local_center_x + rotation.cosine * self.local_center_y + position.y;
		let inverse_time_step = 1.0 / time_step;
		self.linear_velocity_x = inverse_time_step * (target_center_x - self.center_x);
		self.linear_velocity_y = inverse_time_step * (target_center_y - self.center_y);
		let relative_sine = rotation.sine * self.rotation_cosine - rotation.cosine * self.rotation_sine;
		let relative_cosine = rotation.cosine * self.rotation_cosine + rotation.sine * self.rotation_sine;
		self.angular_velocity = inverse_time_step * PhysicsMath2D::compute_atan2(relative_sine, relative_cosine);
		self.wake();
	}

	//==============================================================================
	/// 질량 중심의 월드 자리 반환.
	///
	/// # 돌려줌
	/// 질량 중심. (월드)
	//==============================================================================
	pub fn get_world_center_of_mass(&self) -> PhysicsVector2 {
		PhysicsVector2::create(self.center_x, self.center_y)
	}

	//==============================================================================
	/// 질량 중심의 몸 기준 자리 반환.
	///
	/// # 돌려줌
	/// 질량 중심. (몸 기준)
	//==============================================================================
	pub fn get_local_center_of_mass(&self) -> PhysicsVector2 {
		PhysicsVector2::create(self.local_center_x, self.local_center_y)
	}

	//==============================================================================
	/// 선속도 설정. (정적 몸은 무시, 0 이 아니면 깨운다)
	///
	/// # 인자
	/// - `linear_velocity`: 질량 중심의 새 속도.
	//==============================================================================
	pub fn set_linear_velocity(&mut self, linear_velocity: impl Into<PhysicsVector2>) {
		let linear_velocity = linear_velocity.into();
		if self.body_type == BodyType2D::Static {
			return;
		}
		if linear_velocity.x * linear_velocity.x + linear_velocity.y * linear_velocity.y > 0.0 {
			self.wake();
		}
		self.linear_velocity_x = linear_velocity.x;
		self.linear_velocity_y = linear_velocity.y;
	}

	//==============================================================================
	/// 선속도 반환.
	///
	/// # 돌려줌
	/// 질량 중심의 속도.
	//==============================================================================
	pub fn get_linear_velocity(&self) -> PhysicsVector2 {
		PhysicsVector2::create(self.linear_velocity_x, self.linear_velocity_y)
	}

	//==============================================================================
	/// 각속도 설정. (정적 몸과 돌지 않는 몸은 무시, 0 이 아니면 깨운다)
	///
	/// # 인자
	/// - `angular_velocity`: 새 각속도. (초당 라디안)
	//==============================================================================
	pub fn set_angular_velocity(&mut self, angular_velocity: f64) {
		if self.body_type == BodyType2D::Static || self.is_fixed_rotation {
			return;
		}
		if angular_velocity != 0.0 {
			self.wake();
		}
		self.angular_velocity = angular_velocity;
	}

	//==============================================================================
	/// 각속도 반환.
	///
	/// # 돌려줌
	/// 각속도. (초당 라디안)
	//==============================================================================
	pub fn get_angular_velocity(&self) -> f64 {
		self.angular_velocity
	}

	//==============================================================================
	/// 월드 점의 속도 반환. (선속도 + 각속도 x 질량 중심에서 그 점까지)
	///
	/// # 인자
	/// - `world_point`: 월드 점.
	///
	/// # 돌려줌
	/// 그 점의 속도.
	//==============================================================================
	pub fn get_world_point_velocity(&self, world_point: impl Into<PhysicsVector2>) -> PhysicsVector2 {
		let world_point = world_point.into();
		let offset_x = world_point.x - self.center_x;
		let offset_y = world_point.y - self.center_y;
		PhysicsVector2::create(self.linear_velocity_x - self.angular_velocity * offset_y, self.linear_velocity_y + self.angular_velocity * offset_x)
	}

	//==============================================================================
	/// 힘을 월드 점에 주기. (동적 몸만, 다음 걸음 동안 걸리고 걸음 뒤에 지운다)
	/// - wake 가 false 이고 몸이 잠들어 있으면 주지 않는다.
	///
	/// # 인자
	/// - `force`: 힘. (월드)
	/// - `world_point`: 힘을 주는 월드 점.
	/// - `wake`: 몸을 깨울지.
	//==============================================================================
	pub fn apply_force(&mut self, force: impl Into<PhysicsVector2>, world_point: impl Into<PhysicsVector2>, wake: bool) {
		let force = force.into();
		let world_point = world_point.into();
		if !self.prepare_apply(wake) {
			return;
		}
		self.force_x += force.x;
		self.force_y += force.y;
		self.torque += (world_point.x - self.center_x) * force.y - (world_point.y - self.center_y) * force.x;
	}

	//==============================================================================
	/// 힘을 질량 중심에 주기. (동적 몸만)
	///
	/// # 인자
	/// - `force`: 힘. (월드)
	/// - `wake`: 몸을 깨울지.
	//==============================================================================
	pub fn apply_force_to_center(&mut self, force: impl Into<PhysicsVector2>, wake: bool) {
		let force = force.into();
		if !self.prepare_apply(wake) {
			return;
		}
		self.force_x += force.x;
		self.force_y += force.y;
	}

	//==============================================================================
	/// 돌림힘 주기. (동적 몸만)
	///
	/// # 인자
	/// - `torque`: 돌림힘.
	/// - `wake`: 몸을 깨울지.
	//==============================================================================
	pub fn apply_torque(&mut self, torque: f64, wake: bool) {
		if !self.prepare_apply(wake) {
			return;
		}
		self.torque += torque;
	}

	//==============================================================================
	/// 충격을 월드 점에 주기. (동적 몸만, 속도를 바로 바꾼다)
	///
	/// # 인자
	/// - `impulse`: 충격. (월드)
	/// - `world_point`: 충격을 주는 월드 점.
	/// - `wake`: 몸을 깨울지.
	//==============================================================================
	pub fn apply_linear_impulse(&mut self, impulse: impl Into<PhysicsVector2>, world_point: impl Into<PhysicsVector2>, wake: bool) {
		let impulse = impulse.into();
		let world_point = world_point.into();
		if !self.prepare_apply(wake) {
			return;
		}
		self.linear_velocity_x += self.inverse_mass * impulse.x;
		self.linear_velocity_y += self.inverse_mass * impulse.y;
		self.angular_velocity += self.inverse_inertia * ((world_point.x - self.center_x) * impulse.y - (world_point.y - self.center_y) * impulse.x);
	}

	//==============================================================================
	/// 충격을 질량 중심에 주기. (동적 몸만)
	///
	/// # 인자
	/// - `impulse`: 충격. (월드)
	/// - `wake`: 몸을 깨울지.
	//==============================================================================
	pub fn apply_linear_impulse_to_center(&mut self, impulse: impl Into<PhysicsVector2>, wake: bool) {
		let impulse = impulse.into();
		if !self.prepare_apply(wake) {
			return;
		}
		self.linear_velocity_x += self.inverse_mass * impulse.x;
		self.linear_velocity_y += self.inverse_mass * impulse.y;
	}

	//==============================================================================
	/// 각 충격 주기. (동적 몸만)
	///
	/// # 인자
	/// - `impulse`: 각 충격.
	/// - `wake`: 몸을 깨울지.
	//==============================================================================
	pub fn apply_angular_impulse(&mut self, impulse: f64, wake: bool) {
		if !self.prepare_apply(wake) {
			return;
		}
		self.angular_velocity += self.inverse_inertia * impulse;
	}

	//==============================================================================
	/// 힘이나 충격을 줄 수 있는지 보고 필요하면 깨우기.
	//==============================================================================
	fn prepare_apply(&mut self, wake: bool) -> bool {
		if self.body_type != BodyType2D::Dynamic {
			return false;
		}
		if wake {
			self.wake();
		}
		self.is_awake
	}

	//==============================================================================
	/// 질량 반환.
	///
	/// # 돌려줌
	/// 질량. (정적, 운동학 몸은 0)
	//==============================================================================
	pub fn get_mass(&self) -> f64 {
		self.mass
	}

	//==============================================================================
	/// 질량 중심에 대한 회전 관성 반환.
	///
	/// # 돌려줌
	/// 회전 관성.
	//==============================================================================
	pub fn get_rotational_inertia(&self) -> f64 {
		self.rotational_inertia
	}

	//==============================================================================
	/// 선속도 감쇠 설정.
	///
	/// # 인자
	/// - `linear_damping`: 선속도 감쇠. (0 이상)
	//==============================================================================
	pub fn set_linear_damping(&mut self, linear_damping: f64) {
		self.linear_damping = js_max(linear_damping, 0.0);
	}

	//==============================================================================
	/// 선속도 감쇠 반환.
	///
	/// # 돌려줌
	/// 선속도 감쇠.
	//==============================================================================
	pub fn get_linear_damping(&self) -> f64 {
		self.linear_damping
	}

	//==============================================================================
	/// 각속도 감쇠 설정.
	///
	/// # 인자
	/// - `angular_damping`: 각속도 감쇠. (0 이상)
	//==============================================================================
	pub fn set_angular_damping(&mut self, angular_damping: f64) {
		self.angular_damping = js_max(angular_damping, 0.0);
	}

	//==============================================================================
	/// 각속도 감쇠 반환.
	///
	/// # 돌려줌
	/// 각속도 감쇠.
	//==============================================================================
	pub fn get_angular_damping(&self) -> f64 {
		self.angular_damping
	}

	//==============================================================================
	/// 중력 배율 설정.
	///
	/// # 인자
	/// - `gravity_scale`: 중력 배율.
	//==============================================================================
	pub fn set_gravity_scale(&mut self, gravity_scale: f64) {
		self.gravity_scale = gravity_scale;
	}

	//==============================================================================
	/// 중력 배율 반환.
	///
	/// # 돌려줌
	/// 중력 배율.
	//==============================================================================
	pub fn get_gravity_scale(&self) -> f64 {
		self.gravity_scale
	}

	//==============================================================================
	/// 깨어 있는지.
	///
	/// # 돌려줌
	/// 깨어 있으면 true.
	//==============================================================================
	pub fn is_awake(&self) -> bool {
		self.is_awake
	}

	//==============================================================================
	/// 깨우거나 재우기. (재우면 속도를 0 으로 두고, 섬의 다른 몸이 깨어 있으면 다음 걸음에 다시 깬다)
	///
	/// # 인자
	/// - `is_awake`: 깨울지.
	//==============================================================================
	pub fn set_awake(&mut self, is_awake: bool) {
		if self.body_type == BodyType2D::Static {
			return;
		}
		if is_awake {
			self.wake();
			return;
		}
		self.is_awake = false;
		self.sleep_time = 0.0;
		self.linear_velocity_x = 0.0;
		self.linear_velocity_y = 0.0;
		self.angular_velocity = 0.0;
		self.force_x = 0.0;
		self.force_y = 0.0;
		self.torque = 0.0;
	}

	//==============================================================================
	/// 깨우기. (잠든 시간을 지운다)
	//==============================================================================
	pub(crate) fn wake(&mut self) {
		if self.body_type == BodyType2D::Static {
			return;
		}
		self.is_awake = true;
		self.sleep_time = 0.0;
	}

	//==============================================================================
	/// 잠들 수 있게 할지 설정. (끄면 깨운다)
	///
	/// # 인자
	/// - `enable_sleep`: 잠들 수 있게 할지.
	//==============================================================================
	pub fn set_sleep_enabled(&mut self, enable_sleep: bool) {
		self.enable_sleep = enable_sleep;
		if !enable_sleep {
			self.wake();
		}
	}

	//==============================================================================
	/// 잠들 수 있는지.
	///
	/// # 돌려줌
	/// 잠들 수 있으면 true.
	//==============================================================================
	pub fn is_sleep_enabled(&self) -> bool {
		self.enable_sleep
	}

	//==============================================================================
	/// 돌지 않게 할지 설정. (다음 걸음 앞에서 회전 관성을 다시 셈한다)
	///
	/// # 인자
	/// - `is_fixed_rotation`: 돌지 않게 할지.
	//==============================================================================
	pub fn set_fixed_rotation(&mut self, is_fixed_rotation: bool) {
		self.is_fixed_rotation = is_fixed_rotation;
		self.is_mass_dirty = true;
		if is_fixed_rotation {
			self.angular_velocity = 0.0;
		}
		self.wake();
	}

	//==============================================================================
	/// 돌지 않는지.
	///
	/// # 돌려줌
	/// 돌지 않게 했으면 true.
	//==============================================================================
	pub fn is_fixed_rotation(&self) -> bool {
		self.is_fixed_rotation
	}

	//==============================================================================
	/// 붙은 모양 아이디 목록 반환. (붙인 차례)
	///
	/// # 돌려줌
	/// 모양 아이디 목록.
	//==============================================================================
	pub fn get_shape_ids(&self) -> Vec<ShapeId2D> {
		self.shape_id_list.clone()
	}

	//==============================================================================
	/// 몸 기준 점을 월드 점으로.
	///
	/// # 인자
	/// - `local_point`: 몸 기준 점.
	///
	/// # 돌려줌
	/// 월드 점.
	//==============================================================================
	pub fn get_world_point(&self, local_point: impl Into<PhysicsVector2>) -> PhysicsVector2 {
		let local_point = local_point.into();
		PhysicsVector2::create(self.rotation_cosine * local_point.x - self.rotation_sine * local_point.y + self.position_x, self.rotation_sine * local_point.x + self.rotation_cosine * local_point.y + self.position_y)
	}

	//==============================================================================
	/// 월드 점을 몸 기준 점으로.
	///
	/// # 인자
	/// - `world_point`: 월드 점.
	///
	/// # 돌려줌
	/// 몸 기준 점.
	//==============================================================================
	pub fn get_local_point(&self, world_point: impl Into<PhysicsVector2>) -> PhysicsVector2 {
		let world_point = world_point.into();
		let offset_x = world_point.x - self.position_x;
		let offset_y = world_point.y - self.position_y;
		PhysicsVector2::create(self.rotation_cosine * offset_x + self.rotation_sine * offset_y, -self.rotation_sine * offset_x + self.rotation_cosine * offset_y)
	}

	//==============================================================================
	/// 몸 기준 방향을 월드 방향으로.
	///
	/// # 인자
	/// - `local_vector`: 몸 기준 방향.
	///
	/// # 돌려줌
	/// 월드 방향.
	//==============================================================================
	pub fn get_world_vector(&self, local_vector: impl Into<PhysicsVector2>) -> PhysicsVector2 {
		let local_vector = local_vector.into();
		PhysicsVector2::create(self.rotation_cosine * local_vector.x - self.rotation_sine * local_vector.y, self.rotation_sine * local_vector.x + self.rotation_cosine * local_vector.y)
	}

	//==============================================================================
	/// 월드 방향을 몸 기준 방향으로.
	///
	/// # 인자
	/// - `world_vector`: 월드 방향.
	///
	/// # 돌려줌
	/// 몸 기준 방향.
	//==============================================================================
	pub fn get_local_vector(&self, world_vector: impl Into<PhysicsVector2>) -> PhysicsVector2 {
		let world_vector = world_vector.into();
		PhysicsVector2::create(self.rotation_cosine * world_vector.x + self.rotation_sine * world_vector.y, -self.rotation_sine * world_vector.x + self.rotation_cosine * world_vector.y)
	}

	//==============================================================================
	/// 앞 걸음과 지금 걸음 사이의 원점 자리. (그리기 보간, alpha 는 PhysicsWorld2D::advance 가 돌려준 몫)
	///
	/// # 인자
	/// - `alpha`: 섞는 몫. (0 이면 앞 걸음, 1 이면 지금 걸음)
	///
	/// # 돌려줌
	/// 보간한 자리.
	//==============================================================================
	pub fn get_interpolated_position(&self, alpha: f64) -> PhysicsVector2 {
		PhysicsVector2::create(self.previous_position_x + alpha * (self.position_x - self.previous_position_x), self.previous_position_y + alpha * (self.position_y - self.previous_position_y))
	}

	//==============================================================================
	/// 앞 걸음과 지금 걸음 사이의 돌림. (회전을 섞어 길이를 맞춘 뒤 각도로)
	///
	/// # 인자
	/// - `alpha`: 섞는 몫. (0 이면 앞 걸음, 1 이면 지금 걸음)
	///
	/// # 돌려줌
	/// 보간한 각도. (라디안)
	//==============================================================================
	pub fn get_interpolated_angle(&self, alpha: f64) -> f64 {
		let cosine = self.previous_rotation_cosine + alpha * (self.rotation_cosine - self.previous_rotation_cosine);
		let sine = self.previous_rotation_sine + alpha * (self.rotation_sine - self.previous_rotation_sine);
		PhysicsMath2D::compute_atan2(sine, cosine)
	}

	//==============================================================================
	/// 사용자 값 설정.
	///
	/// # 인자
	/// - `user_data`: 사용자 값.
	//==============================================================================
	pub fn set_user_data(&mut self, user_data: PhysicsUserData2D) {
		self.user_data = user_data;
	}

	//==============================================================================
	/// 사용자 값 반환.
	///
	/// # 돌려줌
	/// 사용자 값.
	//==============================================================================
	pub fn get_user_data(&self) -> &PhysicsUserData2D {
		&self.user_data
	}
}
