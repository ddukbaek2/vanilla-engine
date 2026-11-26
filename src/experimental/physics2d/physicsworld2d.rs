//==============================================================================
//! 2D 물리 월드.
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/physicsworld2d.js 와 같은 API, 같은 셈이다.
//! - 강체(정적, 운동학, 동적), 모양(원, 캡슐, 선분, 볼록 다각형, 사슬), 관절(거리, 회전, 용접, 바퀴)을 들고 걸음을 돌린다.
//!   기능은 Box2D 3(Erin Catto)와 유니티 2D 물리(Rigidbody2D, Collider2D)를 기준으로 했다.
//! - 넓은 단계는 동적 AABB 나무(DynamicTree), 좁은 단계는 분리축과 변 자르기, 풀이기는 부드러운 걸음(하위 걸음, 웜 스타트, 부드러운 구속, 이완, 반발),
//!   붙어 있는 몸끼리 섬으로 묶어 가만히 있으면 섬째 재운다.
//! - 결정론: 셈은 모두 f64 이고 삼각 함수는 직접 짰다. 같은 차례로 만들고 같은 step 을 부르면 원본 vanilla.js 의 같은 월드와 결과가 비트까지 같다.
//!   (짝 찾기는 열쇠 차례로 닿음을 만들고, 해시 표는 찾기에만 쓰며 훑기는 늘 칸 차례다)
//! - 원본과 다른 점: 질의(cast_ray, overlap_*)는 순간 이동을 먼저 반영하느라 &mut self 다. 몸과 모양은 get_body_mut, `world[body_id]` 로 바꾼다.
//!   advance 의 고정 걸음 누적은 원본 FixedStepper 와 같은 셈을 f64 로 월드 안에 둔다. (러스트 FixedStepper 는 f32 라 걸음 시간이 원본과 달라진다)
//! - 아직 없는 것: 연속 충돌(빠른 총알이 얇은 벽을 뚫지 않게 하는 TOI), 한쪽 면 사슬, 프리즘, 모터, 마우스 관절, 몸 종류 바꾸기, 여러 스레드.
//==============================================================================
// 옮긴 원문: Box2D 3 의 셈에서 옮김, MIT, THIRD_PARTY_LICENSES.md
use std::collections::HashMap;
use std::ops::{Index, IndexMut};

use super::body2d::{Body2D, BodyDefinition2D, BodyType2D, NO_INDEX};
use super::geometry2d::{Segment2D, ShapeGeometry2D, ShapeType2D};
use super::joint2d::{prepare_joint, solve_joint, warm_start_joint, DistanceJointDefinition2D, Joint2D, JointType2D, RevoluteJointDefinition2D, WeldJointDefinition2D, WheelJointDefinition2D};
use super::narrowphase2d::{collide_shapes, fill_world_shape, ray_cast_world_shape, test_point_in_world_shape, Manifold, WorldShape};
use super::physicsid2d::{BodyId2D, JointId2D, ShapeId2D};
use super::physicsmath2d::PhysicsMath2D;
use super::physicsvector2::PhysicsVector2;
use super::shape2d::{compute_shape_aabb, compute_shape_extent, compute_shape_mass, set_shape_geometry, should_filters_collide, ChainDefinition2D, QueryFilter2D, Shape2D, ShapeDefinition2D};
use super::solver2d::{apply_contact_restitution, integrate_rotation, make_softness, prepare_contact, solve_contact, warm_start_contact, Contact, SolverBody, StepContext, STATIC_SOLVER_BODY};
use crate::experimental::spatial::aabb2d::AABB2D;
use crate::experimental::spatial::dynamictree::DynamicTree;

/// 닿음 짝 열쇠의 위쪽 배수. (작은 모양 번호 x 이 값 + 큰 모양 번호)
const CONTACT_KEY_MULTIPLIER: u64 = 4294967296;
/// 한 하위 걸음에 도는 가장 큰 각도. (Box2D 의 B2_MAX_ROTATION = 0.25 파이)
const MAXIMUM_ROTATION: f64 = 0.25 * std::f64::consts::PI;
/// 잠들기 전에 가만히 있어야 하는 시간. (초, Box2D 의 B2_TIME_TO_SLEEP)
const TIME_TO_SLEEP: f64 = 0.5;

//==============================================================================
/// 월드 정의. (PhysicsWorld2D 에 넘기는 값)
/// - length_units_per_meter 는 1 미터가 몇 길이 단위인지다. 화소로 쓰면 50 처럼 준다. 허용 오차(0.005 m), 앞질러 보기 거리, 뚱뚱한 상자 여유,
///   잠들 속력 같은 안쪽 길이와 아래 기본값이 이 값에 맞춰진다. (Box2D 의 b2SetLengthUnitsPerMeter 와 같은 뜻)
/// - 엔진 화면은 y 가 아래로 커서 기본 중력은 (0, 10 x length_units_per_meter) 다.
///
/// # 예제
/// 화소 단위 월드 정의
/// ```rust
/// use vanilla::experimental::physics2d::WorldDefinition2D;
///
/// let world_definition = WorldDefinition2D::new(50.0);
/// assert_eq!(world_definition.gravity.y, 500.0);
/// assert_eq!(world_definition.restitution_threshold, 50.0);
/// assert_eq!(world_definition.contact_hertz, 30.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldDefinition2D {
	/// 1 미터의 길이 단위 수.
	pub length_units_per_meter: f64,
	/// 중력 가속도. (길이 단위 / 초의 제곱)
	pub gravity: PhysicsVector2,
	/// 잠자기를 쓸지. (기본 true)
	pub enable_sleep: bool,
	/// 닿음의 부드러움 진동수. (기본 30)
	pub contact_hertz: f64,
	/// 닿음의 감쇠비. (기본 10)
	pub contact_damping_ratio: f64,
	/// 관절의 부드러움 진동수. (기본 60)
	pub joint_hertz: f64,
	/// 관절의 감쇠비. (기본 2)
	pub joint_damping_ratio: f64,
	/// 이 속력보다 느리게 부딪히면 튀지 않는다. (기본 1 m/s)
	pub restitution_threshold: f64,
	/// 겹침을 밀어내는 가장 큰 속력. (기본 3 m/s)
	pub maximum_contact_push_speed: f64,
	/// 몸의 가장 큰 속력. (기본 400 m/s)
	pub maximum_linear_speed: f64,
}

impl Default for WorldDefinition2D {
	//==============================================================================
	/// 기본값. (1 미터가 1 길이 단위)
	//==============================================================================
	fn default() -> WorldDefinition2D {
		WorldDefinition2D::new(1.0)
	}
}

impl WorldDefinition2D {
	//==============================================================================
	/// 생성. (Box2D 3.0 의 기본값을 length_units_per_meter 에 맞춘다)
	///
	/// # 인자
	/// - `length_units_per_meter`: 1 미터의 길이 단위 수. (0 보다 커야 한다, 아니면 1)
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new(length_units_per_meter: f64) -> WorldDefinition2D {
		let units_per_meter = if length_units_per_meter > 0.0 {
			length_units_per_meter
		}
		else {
			1.0
		};
		WorldDefinition2D {
			length_units_per_meter: units_per_meter,
			gravity: PhysicsVector2::create(0.0, 10.0 * units_per_meter),
			enable_sleep: true,
			contact_hertz: 30.0,
			contact_damping_ratio: 10.0,
			joint_hertz: 60.0,
			joint_damping_ratio: 2.0,
			restitution_threshold: 1.0 * units_per_meter,
			maximum_contact_push_speed: 3.0 * units_per_meter,
			maximum_linear_speed: 400.0 * units_per_meter,
		}
	}
}

//==============================================================================
/// 닿음 사건. (닿기 시작하거나 끝난 모양 짝)
///
/// # 예제
/// 닿기 시작한 짝 세기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, Circle2D, PhysicsVector2, PhysicsWorld2D, Polygon2D, ShapeDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let ground_id = world.create_body(&BodyDefinition2D::new());
/// world.create_shape(ground_id, &ShapeDefinition2D::new(), Polygon2D::make_box(5.0, 0.5));
/// let mut ball_definition = BodyDefinition2D::new();
/// ball_definition.body_type = BodyType2D::Dynamic;
/// ball_definition.position = PhysicsVector2::create(0.0, -1.0);
/// let ball_id = world.create_body(&ball_definition);
/// world.create_shape(ball_id, &ShapeDefinition2D::new(), Circle2D::create(PhysicsVector2::zero(), 0.25));
/// let mut begin_count = 0;
/// for _ in 0..60 {
/// 	world.step(1.0 / 60.0, 4);
/// 	begin_count += world.get_contact_begin_events().len();
/// }
/// assert_eq!(begin_count, 1);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactEvent2D {
	/// 첫 모양.
	pub shape_id_a: ShapeId2D,
	/// 둘째 모양.
	pub shape_id_b: ShapeId2D,
}

//==============================================================================
/// 센서 사건. (센서에 들어오거나 나간 모양)
///
/// # 예제
/// 센서 사건 만들기
/// ```rust
/// use vanilla::experimental::physics2d::{SensorEvent2D, ShapeId2D};
///
/// let sensor_event = SensorEvent2D { sensor_shape_id: ShapeId2D::new(0, 0), visitor_shape_id: ShapeId2D::new(1, 0) };
/// assert_eq!(sensor_event.visitor_shape_id.get_index(), 1);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensorEvent2D {
	/// 센서 모양.
	pub sensor_shape_id: ShapeId2D,
	/// 들어오거나 나간 모양.
	pub visitor_shape_id: ShapeId2D,
}

//==============================================================================
/// 광선 결과.
///
/// # 예제
/// 광선 결과 만들기
/// ```rust
/// use vanilla::experimental::physics2d::{PhysicsVector2, RayResult2D, ShapeId2D};
///
/// let ray_result = RayResult2D { shape_id: ShapeId2D::new(0, 0), point: PhysicsVector2::create(1.0, 2.0), normal: PhysicsVector2::create(0.0, -1.0), fraction: 0.5 };
/// assert_eq!(ray_result.fraction, 0.5);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayResult2D {
	/// 맞은 모양.
	pub shape_id: ShapeId2D,
	/// 맞은 점. (월드)
	pub point: PhysicsVector2,
	/// 맞은 겉면의 바깥쪽 법선.
	pub normal: PhysicsVector2,
	/// 광선 위 몫. (0 이면 시작점, 1 이면 끝점)
	pub fraction: f64,
}

//==============================================================================
/// 2D 물리 월드.
///
/// # 예제
/// 상자를 떨어뜨려 땅에 쌓기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, PhysicsVector2, PhysicsWorld2D, Polygon2D, Segment2D, ShapeDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let ground_id = world.create_body(&BodyDefinition2D::new());
/// world.create_shape(ground_id, &ShapeDefinition2D::new(), Segment2D::create(PhysicsVector2::create(-10.0, 0.0), PhysicsVector2::create(10.0, 0.0)));
/// let mut box_id_list = Vec::new();
/// for index in 0..3 {
/// 	let mut body_definition = BodyDefinition2D::new();
/// 	body_definition.body_type = BodyType2D::Dynamic;
/// 	body_definition.position = PhysicsVector2::create(0.0, -0.5 - index as f64 * 1.1);
/// 	let body_id = world.create_body(&body_definition);
/// 	world.create_shape(body_id, &ShapeDefinition2D::new(), Polygon2D::make_box(0.5, 0.5));
/// 	box_id_list.push(body_id);
/// }
/// for _ in 0..180 {
/// 	world.step(1.0 / 60.0, 4);
/// }
/// let top_body = &world[box_id_list[2]];
/// assert!((top_body.get_position().y + 2.5).abs() < 0.02);
/// assert!(!top_body.is_awake());
/// ```
//==============================================================================
pub struct PhysicsWorld2D {
	length_units_per_meter: f64,
	gravity_x: f64,
	gravity_y: f64,
	enable_sleep: bool,
	contact_hertz: f64,
	contact_damping_ratio: f64,
	joint_hertz: f64,
	joint_damping_ratio: f64,
	restitution_threshold: f64,
	maximum_contact_push_speed: f64,
	maximum_linear_speed: f64,
	linear_slop: f64,
	speculative_distance: f64,
	aabb_margin: f64,
	sleep_threshold: f64,
	pub(crate) body_list: Vec<Option<Body2D>>,
	body_generation_list: Vec<u32>,
	body_free_list: Vec<usize>,
	pub(crate) shape_list: Vec<Option<Shape2D>>,
	shape_generation_list: Vec<u32>,
	shape_free_list: Vec<usize>,
	pub(crate) joint_list: Vec<Option<Joint2D>>,
	joint_generation_list: Vec<u32>,
	joint_free_list: Vec<usize>,
	pub(crate) contact_list: Vec<Contact>,
	contact_free_list: Vec<usize>,
	contact_table: HashMap<u64, usize>,
	tree: DynamicTree<usize>,
	move_buffer: Vec<usize>,
	shape_moved_list: Vec<bool>,
	solver_body_list: Vec<SolverBody>,
	solver_body_count: usize,
	world_shape_a: WorldShape,
	world_shape_b: WorldShape,
	scratch_manifold: Manifold,
	contact_begin_event_list: Vec<ContactEvent2D>,
	contact_end_event_list: Vec<ContactEvent2D>,
	sensor_begin_event_list: Vec<SensorEvent2D>,
	sensor_end_event_list: Vec<SensorEvent2D>,
	fixed_step_seconds: f64,
	maximum_steps_per_frame: u32,
	accumulated_seconds: f64,
	sub_step_count: u32,
	interpolation_alpha: f64,
}

impl PhysicsWorld2D {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `world_definition`: 월드 정의.
	///
	/// # 돌려줌
	/// 빈 월드.
	//==============================================================================
	pub fn new(world_definition: &WorldDefinition2D) -> PhysicsWorld2D {
		let units_per_meter = if world_definition.length_units_per_meter > 0.0 {
			world_definition.length_units_per_meter
		}
		else {
			1.0
		};
		let linear_slop = 0.005 * units_per_meter;
		PhysicsWorld2D {
			length_units_per_meter: units_per_meter,
			gravity_x: world_definition.gravity.x,
			gravity_y: world_definition.gravity.y,
			enable_sleep: world_definition.enable_sleep,
			contact_hertz: world_definition.contact_hertz.max(0.0),
			contact_damping_ratio: world_definition.contact_damping_ratio.max(0.0),
			joint_hertz: world_definition.joint_hertz.max(0.0),
			joint_damping_ratio: world_definition.joint_damping_ratio.max(0.0),
			restitution_threshold: world_definition.restitution_threshold.max(0.0),
			maximum_contact_push_speed: world_definition.maximum_contact_push_speed.max(0.0),
			maximum_linear_speed: world_definition.maximum_linear_speed.max(0.0),
			linear_slop,
			speculative_distance: 4.0 * linear_slop,
			aabb_margin: 0.05 * units_per_meter,
			sleep_threshold: 0.05 * units_per_meter,
			body_list: Vec::new(),
			body_generation_list: Vec::new(),
			body_free_list: Vec::new(),
			shape_list: Vec::new(),
			shape_generation_list: Vec::new(),
			shape_free_list: Vec::new(),
			joint_list: Vec::new(),
			joint_generation_list: Vec::new(),
			joint_free_list: Vec::new(),
			contact_list: Vec::new(),
			contact_free_list: Vec::new(),
			contact_table: HashMap::new(),
			tree: DynamicTree::new(),
			move_buffer: Vec::new(),
			shape_moved_list: Vec::new(),
			solver_body_list: Vec::new(),
			solver_body_count: 0,
			world_shape_a: WorldShape::new(),
			world_shape_b: WorldShape::new(),
			scratch_manifold: Manifold::default(),
			contact_begin_event_list: Vec::new(),
			contact_end_event_list: Vec::new(),
			sensor_begin_event_list: Vec::new(),
			sensor_end_event_list: Vec::new(),
			fixed_step_seconds: 1.0 / 60.0,
			maximum_steps_per_frame: 5,
			accumulated_seconds: 0.0,
			sub_step_count: 4,
			interpolation_alpha: 1.0,
		}
	}

	//==============================================================================
	/// 1 미터의 길이 단위 수 반환.
	///
	/// # 돌려줌
	/// 1 미터의 길이 단위 수.
	//==============================================================================
	pub fn get_length_units_per_meter(&self) -> f64 {
		self.length_units_per_meter
	}

	//==============================================================================
	/// 중력 설정. (잠든 몸은 깨우지 않는다)
	///
	/// # 인자
	/// - `gravity`: 중력 가속도.
	//==============================================================================
	pub fn set_gravity(&mut self, gravity: impl Into<PhysicsVector2>) {
		let gravity = gravity.into();
		self.gravity_x = gravity.x;
		self.gravity_y = gravity.y;
	}

	//==============================================================================
	/// 중력 반환.
	///
	/// # 돌려줌
	/// 중력 가속도.
	//==============================================================================
	pub fn get_gravity(&self) -> PhysicsVector2 {
		PhysicsVector2::create(self.gravity_x, self.gravity_y)
	}

	//==============================================================================
	/// 잠자기를 쓸지 설정. (끄면 모든 몸을 깨운다)
	///
	/// # 인자
	/// - `enable_sleep`: 잠자기를 쓸지.
	//==============================================================================
	pub fn set_sleep_enabled(&mut self, enable_sleep: bool) {
		self.enable_sleep = enable_sleep;
		if !enable_sleep {
			for body in self.body_list.iter_mut().flatten() {
				if body.body_type != BodyType2D::Static {
					body.is_awake = true;
					body.sleep_time = 0.0;
				}
			}
		}
	}

	//==============================================================================
	/// 잠자기를 쓰는지.
	///
	/// # 돌려줌
	/// 잠자기를 쓰면 true.
	//==============================================================================
	pub fn is_sleep_enabled(&self) -> bool {
		self.enable_sleep
	}

	//==============================================================================
	/// 넓은 단계 나무 반환. (살펴보기용, 사용자 값은 모양 칸 번호)
	///
	/// # 돌려줌
	/// 모양의 뚱뚱한 상자를 든 나무.
	//==============================================================================
	pub fn get_broad_phase_tree(&self) -> &DynamicTree<usize> {
		&self.tree
	}

	//==============================================================================
	/// 몸 만들기.
	///
	/// # 인자
	/// - `body_definition`: 몸 정의.
	///
	/// # 돌려줌
	/// 새 몸의 아이디.
	//==============================================================================
	pub fn create_body(&mut self, body_definition: &BodyDefinition2D) -> BodyId2D {
		let index = if let Some(free_index) = self.body_free_list.pop() {
			free_index
		}
		else {
			self.body_list.push(None);
			self.body_generation_list.push(0);
			self.body_list.len() - 1
		};
		let generation = self.body_generation_list[index];
		let mut data = Body2D::new_empty(BodyId2D::new(index, generation));
		data.body_type = body_definition.body_type;
		let rotation = PhysicsMath2D::compute_rotation(body_definition.angle);
		data.position_x = body_definition.position.x;
		data.position_y = body_definition.position.y;
		data.rotation_cosine = rotation.cosine;
		data.rotation_sine = rotation.sine;
		data.previous_position_x = data.position_x;
		data.previous_position_y = data.position_y;
		data.previous_rotation_cosine = data.rotation_cosine;
		data.previous_rotation_sine = data.rotation_sine;
		data.update_center();
		if data.body_type != BodyType2D::Static {
			data.linear_velocity_x = body_definition.linear_velocity.x;
			data.linear_velocity_y = body_definition.linear_velocity.y;
			data.angular_velocity = if body_definition.is_fixed_rotation {
				0.0
			}
			else {
				body_definition.angular_velocity
			};
		}
		data.linear_damping = body_definition.linear_damping.max(0.0);
		data.angular_damping = body_definition.angular_damping.max(0.0);
		data.gravity_scale = body_definition.gravity_scale;
		data.enable_sleep = body_definition.enable_sleep;
		data.is_awake = data.body_type != BodyType2D::Static && body_definition.is_awake;
		data.is_fixed_rotation = body_definition.is_fixed_rotation;
		data.user_data = body_definition.user_data;
		let body_id = data.id;
		self.body_list[index] = Some(data);
		self.update_body_mass(index);
		body_id
	}

	//==============================================================================
	/// 몸 없애기. (붙은 관절과 모양도 없앤다)
	///
	/// # 인자
	/// - `body_id`: 없앨 몸.
	//==============================================================================
	pub fn destroy_body(&mut self, body_id: BodyId2D) {
		let Some(index) = self.resolve_body_index(body_id) else {
			return;
		};
		for joint_index in 0..self.joint_list.len() {
			let joint_id = match &self.joint_list[joint_index] {
				Some(joint) if joint.body_index_a == index || joint.body_index_b == index => joint.id,
				_ => continue,
			};
			self.destroy_joint(joint_id);
		}
		let shape_id_list = self.body_list[index].as_ref().map(|body| body.shape_id_list.clone()).unwrap_or_default();
		for shape_id in shape_id_list {
			self.destroy_shape_internal(shape_id, false);
		}
		self.body_list[index] = None;
		self.body_generation_list[index] += 1;
		self.body_free_list.push(index);
	}

	//==============================================================================
	/// 몸 아이디의 칸 번호. (없거나 세대가 다르면 None)
	//==============================================================================
	fn resolve_body_index(&self, body_id: BodyId2D) -> Option<usize> {
		let index = body_id.get_index();
		if index >= self.body_list.len() || self.body_list[index].is_none() || self.body_generation_list[index] != body_id.get_generation() {
			return None;
		}
		Some(index)
	}

	//==============================================================================
	/// 몸 반환.
	///
	/// # 인자
	/// - `body_id`: 몸 아이디.
	///
	/// # 돌려줌
	/// 몸. (없으면 None)
	//==============================================================================
	pub fn get_body(&self, body_id: BodyId2D) -> Option<&Body2D> {
		let index = self.resolve_body_index(body_id)?;
		self.body_list[index].as_ref()
	}

	//==============================================================================
	/// 몸 반환. (변경용)
	///
	/// # 인자
	/// - `body_id`: 몸 아이디.
	///
	/// # 돌려줌
	/// 몸. (없으면 None)
	//==============================================================================
	pub fn get_body_mut(&mut self, body_id: BodyId2D) -> Option<&mut Body2D> {
		let index = self.resolve_body_index(body_id)?;
		self.body_list[index].as_mut()
	}

	//==============================================================================
	/// 살아 있는 몸 아이디 목록. (칸 차례)
	///
	/// # 돌려줌
	/// 몸 아이디 목록.
	//==============================================================================
	pub fn get_body_ids(&self) -> Vec<BodyId2D> {
		self.body_list.iter().flatten().map(|body| body.id).collect()
	}

	//==============================================================================
	/// 몸의 질량, 질량 중심, 회전 관성 다시 셈.
	//==============================================================================
	fn update_body_mass(&mut self, body_index: usize) {
		let shape_list = &self.shape_list;
		let Some(data) = self.body_list[body_index].as_mut() else {
			return;
		};
		data.is_mass_dirty = false;
		data.mass = 0.0;
		data.inverse_mass = 0.0;
		data.rotational_inertia = 0.0;
		data.inverse_inertia = 0.0;
		data.maximum_extent = 0.0;
		let old_center_x = data.center_x;
		let old_center_y = data.center_y;
		data.local_center_x = 0.0;
		data.local_center_y = 0.0;
		if data.body_type != BodyType2D::Dynamic {
			data.update_center();
			for shape_id in data.shape_id_list.iter() {
				let shape_data = shape_list[shape_id.get_index()].as_ref().expect("모양");
				data.maximum_extent = data.maximum_extent.max(compute_shape_extent(shape_data, 0.0, 0.0));
			}
			return;
		}
		let mut mass = 0.0;
		let mut local_center_x = 0.0;
		let mut local_center_y = 0.0;
		let mut rotational_inertia = 0.0;
		for shape_id in data.shape_id_list.iter() {
			let shape_data = shape_list[shape_id.get_index()].as_ref().expect("모양");
			if shape_data.density == 0.0 {
				continue;
			}
			let mass_data = compute_shape_mass(shape_data);
			mass += mass_data[0];
			local_center_x += mass_data[0] * mass_data[1];
			local_center_y += mass_data[0] * mass_data[2];
			rotational_inertia += mass_data[3] + mass_data[0] * (mass_data[1] * mass_data[1] + mass_data[2] * mass_data[2]);
		}
		if mass > 0.0 {
			data.mass = mass;
			data.inverse_mass = 1.0 / mass;
			local_center_x *= data.inverse_mass;
			local_center_y *= data.inverse_mass;
		}
		else {
			// 질량이 없는 동적 몸은 1 로 둔다. (떨어지기는 하되 돌지 않는다)
			data.mass = 1.0;
			data.inverse_mass = 1.0;
			rotational_inertia = 0.0;
		}
		if rotational_inertia > 0.0 && !data.is_fixed_rotation {
			data.rotational_inertia = rotational_inertia - data.mass * (local_center_x * local_center_x + local_center_y * local_center_y);
			data.inverse_inertia = if data.rotational_inertia > 0.0 {
				1.0 / data.rotational_inertia
			}
			else {
				0.0
			};
		}
		data.local_center_x = local_center_x;
		data.local_center_y = local_center_y;
		data.update_center();
		// 질량 중심이 옮겨졌으면 원점의 속도가 그대로이게 질량 중심 속도를 고친다.
		data.linear_velocity_x += -data.angular_velocity * (data.center_y - old_center_y);
		data.linear_velocity_y += data.angular_velocity * (data.center_x - old_center_x);
		for shape_id in data.shape_id_list.iter() {
			let shape_data = shape_list[shape_id.get_index()].as_ref().expect("모양");
			data.maximum_extent = data.maximum_extent.max(compute_shape_extent(shape_data, local_center_x, local_center_y));
		}
	}

	//==============================================================================
	/// 모양 만들기.
	///
	/// # 인자
	/// - `body_id`: 붙일 몸.
	/// - `shape_definition`: 모양 정의.
	/// - `geometry`: 모양. (원, 캡슐, 선분, 볼록 다각형, 몸 기준)
	///
	/// # 돌려줌
	/// 새 모양 아이디. (몸이 없거나 모양이 잘못되면 None)
	//==============================================================================
	pub fn create_shape(&mut self, body_id: BodyId2D, shape_definition: &ShapeDefinition2D, geometry: impl Into<ShapeGeometry2D>) -> Option<ShapeId2D> {
		let geometry = geometry.into();
		let body_index = self.resolve_body_index(body_id)?;
		let mut data = Shape2D::new_empty();
		if !set_shape_geometry(&mut data, &geometry) {
			return None;
		}
		let index = if let Some(free_index) = self.shape_free_list.pop() {
			free_index
		}
		else {
			self.shape_list.push(None);
			self.shape_generation_list.push(0);
			self.shape_moved_list.push(false);
			self.shape_list.len() - 1
		};
		let generation = self.shape_generation_list[index];
		let (body_type, position_x, position_y, rotation_cosine, rotation_sine) = {
			let body_data = self.body_list[body_index].as_ref().expect("몸");
			(body_data.body_type, body_data.position_x, body_data.position_y, body_data.rotation_cosine, body_data.rotation_sine)
		};
		data.id = ShapeId2D::new(index, generation);
		data.body_id = body_id;
		data.body_index = body_index;
		data.density = shape_definition.density.max(0.0);
		data.friction = shape_definition.friction.max(0.0);
		data.restitution = shape_definition.restitution.max(0.0);
		data.category_bits = shape_definition.filter.category_bits;
		data.mask_bits = shape_definition.filter.mask_bits;
		data.group_index = shape_definition.filter.group_index;
		data.is_sensor = shape_definition.is_sensor;
		data.enable_sensor_events = shape_definition.enable_sensor_events;
		data.enable_contact_events = shape_definition.enable_contact_events;
		data.user_data = shape_definition.user_data;
		let mut aabb = AABB2D::new();
		compute_shape_aabb(&data, position_x, position_y, rotation_cosine, rotation_sine, &mut aabb);
		data.aabb = aabb;
		let margin = if body_type == BodyType2D::Static {
			0.0
		}
		else {
			self.aabb_margin
		};
		data.fat_aabb.set(aabb.min_x - margin, aabb.min_y - margin, aabb.max_x + margin, aabb.max_y + margin);
		data.proxy_id = self.tree.create_proxy(&data.fat_aabb, u32::MAX, index);
		let shape_id = data.id;
		self.shape_list[index] = Some(data);
		self.body_list[body_index].as_mut().expect("몸").shape_id_list.push(shape_id);
		self.shape_moved_list[index] = false;
		self.push_moved_shape(index);
		self.update_body_mass(body_index);
		Some(shape_id)
	}

	//==============================================================================
	/// 사슬 만들기. (이은 선분 모양들, 선분마다 모양 아이디)
	///
	/// # 인자
	/// - `body_id`: 붙일 몸.
	/// - `chain_definition`: 사슬 정의.
	///
	/// # 돌려줌
	/// 선분 모양 아이디 목록. (몸이 없거나 점이 2 개보다 적으면 빈 목록)
	//==============================================================================
	pub fn create_chain(&mut self, body_id: BodyId2D, chain_definition: &ChainDefinition2D) -> Vec<ShapeId2D> {
		let mut shape_id_list = Vec::new();
		let point_count = chain_definition.points.len();
		if point_count < 2 {
			return shape_id_list;
		}
		let mut shape_definition = ShapeDefinition2D::new();
		shape_definition.density = 0.0;
		shape_definition.friction = chain_definition.friction;
		shape_definition.restitution = chain_definition.restitution;
		shape_definition.filter = chain_definition.filter;
		shape_definition.user_data = chain_definition.user_data;
		let segment_count = if chain_definition.is_loop && point_count > 2 {
			point_count
		}
		else {
			point_count - 1
		};
		for index in 0..segment_count {
			let point1 = chain_definition.points[index];
			let point2 = chain_definition.points[(index + 1) % point_count];
			if let Some(shape_id) = self.create_shape(body_id, &shape_definition, Segment2D::create(point1, point2)) {
				shape_id_list.push(shape_id);
			}
		}
		shape_id_list
	}

	//==============================================================================
	/// 모양 없애기. (닿음을 지우고 몸의 질량을 다시 셈한다)
	///
	/// # 인자
	/// - `shape_id`: 없앨 모양.
	//==============================================================================
	pub fn destroy_shape(&mut self, shape_id: ShapeId2D) {
		self.destroy_shape_internal(shape_id, true);
	}

	//==============================================================================
	/// 모양 없애기. (update_mass 가 false 면 질량을 다시 셈하지 않는다)
	//==============================================================================
	fn destroy_shape_internal(&mut self, shape_id: ShapeId2D, update_mass: bool) {
		let Some(index) = self.resolve_shape_index(shape_id) else {
			return;
		};
		for contact_index in 0..self.contact_list.len() {
			let contact = &self.contact_list[contact_index];
			if contact.is_alive && (contact.shape_index_a == index || contact.shape_index_b == index) {
				let body_index_a = contact.body_index_a;
				let body_index_b = contact.body_index_b;
				self.wake_body_index(body_index_a);
				self.wake_body_index(body_index_b);
				self.destroy_contact(contact_index);
			}
		}
		let (proxy_id, body_index) = {
			let data = self.shape_list[index].as_ref().expect("모양");
			(data.proxy_id, data.body_index)
		};
		self.tree.destroy_proxy(proxy_id);
		if let Some(body_data) = self.body_list[body_index].as_mut() {
			if let Some(list_index) = body_data.shape_id_list.iter().position(|candidate| *candidate == shape_id) {
				body_data.shape_id_list.remove(list_index);
			}
		}
		self.shape_list[index] = None;
		self.shape_generation_list[index] += 1;
		self.shape_moved_list[index] = false;
		self.shape_free_list.push(index);
		if update_mass {
			self.update_body_mass(body_index);
		}
	}

	//==============================================================================
	/// 모양 아이디의 칸 번호. (없거나 세대가 다르면 None)
	//==============================================================================
	fn resolve_shape_index(&self, shape_id: ShapeId2D) -> Option<usize> {
		let index = shape_id.get_index();
		if index >= self.shape_list.len() || self.shape_list[index].is_none() || self.shape_generation_list[index] != shape_id.get_generation() {
			return None;
		}
		Some(index)
	}

	//==============================================================================
	/// 모양 반환.
	///
	/// # 인자
	/// - `shape_id`: 모양 아이디.
	///
	/// # 돌려줌
	/// 모양. (없으면 None)
	//==============================================================================
	pub fn get_shape(&self, shape_id: ShapeId2D) -> Option<&Shape2D> {
		let index = self.resolve_shape_index(shape_id)?;
		self.shape_list[index].as_ref()
	}

	//==============================================================================
	/// 모양 반환. (변경용)
	///
	/// # 인자
	/// - `shape_id`: 모양 아이디.
	///
	/// # 돌려줌
	/// 모양. (없으면 None)
	//==============================================================================
	pub fn get_shape_mut(&mut self, shape_id: ShapeId2D) -> Option<&mut Shape2D> {
		let index = self.resolve_shape_index(shape_id)?;
		self.shape_list[index].as_mut()
	}

	//==============================================================================
	/// 옮겨진 모양을 짝 찾기 목록에 넣기.
	//==============================================================================
	fn push_moved_shape(&mut self, shape_index: usize) {
		if self.shape_moved_list[shape_index] {
			return;
		}
		self.shape_moved_list[shape_index] = true;
		self.move_buffer.push(shape_index);
	}

	//==============================================================================
	/// 몸 깨우기. (칸 번호)
	//==============================================================================
	fn wake_body_index(&mut self, body_index: usize) {
		if let Some(data) = self.body_list[body_index].as_mut() {
			if data.body_type != BodyType2D::Static {
				data.is_awake = true;
				data.sleep_time = 0.0;
			}
		}
	}

	//==============================================================================
	/// 관절 만들기. (공통, 새 관절 칸 번호)
	//==============================================================================
	fn create_joint_internal(&mut self, joint_type: JointType2D, body_id_a: Option<BodyId2D>, body_id_b: Option<BodyId2D>, local_anchor_a: PhysicsVector2, local_anchor_b: PhysicsVector2, collide_connected: bool, user_data: u64) -> Option<usize> {
		let body_index_a = self.resolve_body_index(body_id_a?)?;
		let body_index_b = self.resolve_body_index(body_id_b?)?;
		if body_index_a == body_index_b {
			return None;
		}
		let index = if let Some(free_index) = self.joint_free_list.pop() {
			free_index
		}
		else {
			self.joint_list.push(None);
			self.joint_generation_list.push(0);
			self.joint_list.len() - 1
		};
		let generation = self.joint_generation_list[index];
		let body_id_a = self.body_list[body_index_a].as_ref().expect("몸").id;
		let body_id_b = self.body_list[body_index_b].as_ref().expect("몸").id;
		let mut data = Joint2D::new_empty(JointId2D::new(index, generation), joint_type, body_id_a, body_id_b);
		data.local_anchor_a_x = local_anchor_a.x;
		data.local_anchor_a_y = local_anchor_a.y;
		data.local_anchor_b_x = local_anchor_b.x;
		data.local_anchor_b_y = local_anchor_b.y;
		data.collide_connected = collide_connected;
		data.user_data = user_data;
		self.joint_list[index] = Some(data);

		// 서로 부딪히지 않는 관절이면 이미 있는 두 몸 사이 닿음을 지운다.
		if !collide_connected {
			for contact_index in 0..self.contact_list.len() {
				let contact = &self.contact_list[contact_index];
				if !contact.is_alive {
					continue;
				}
				let is_pair = (contact.body_index_a == body_index_a && contact.body_index_b == body_index_b) || (contact.body_index_a == body_index_b && contact.body_index_b == body_index_a);
				if is_pair {
					self.destroy_contact(contact_index);
				}
			}
		}
		self.wake_body_index(body_index_a);
		self.wake_body_index(body_index_b);
		Some(index)
	}

	//==============================================================================
	/// 거리 관절 만들기.
	///
	/// # 인자
	/// - `definition`: 거리 관절 정의.
	///
	/// # 돌려줌
	/// 새 관절 아이디. (몸이 없으면 None)
	//==============================================================================
	pub fn create_distance_joint(&mut self, definition: &DistanceJointDefinition2D) -> Option<JointId2D> {
		let index = self.create_joint_internal(JointType2D::Distance, definition.body_id_a, definition.body_id_b, definition.local_anchor_a, definition.local_anchor_b, definition.collide_connected, definition.user_data)?;
		let linear_slop = self.linear_slop;
		let data = self.joint_list[index].as_mut().expect("관절");
		data.length = linear_slop.max(definition.length);
		data.min_length = 0.0f64.max(definition.min_length.min(definition.max_length));
		data.max_length = definition.min_length.max(definition.max_length);
		data.enable_spring = definition.enable_spring;
		data.hertz = definition.hertz.max(0.0);
		data.damping_ratio = definition.damping_ratio.max(0.0);
		data.enable_limit = definition.enable_limit;
		Some(data.id)
	}

	//==============================================================================
	/// 회전 관절 만들기.
	///
	/// # 인자
	/// - `definition`: 회전 관절 정의.
	///
	/// # 돌려줌
	/// 새 관절 아이디. (몸이 없으면 None)
	//==============================================================================
	pub fn create_revolute_joint(&mut self, definition: &RevoluteJointDefinition2D) -> Option<JointId2D> {
		let index = self.create_joint_internal(JointType2D::Revolute, definition.body_id_a, definition.body_id_b, definition.local_anchor_a, definition.local_anchor_b, definition.collide_connected, definition.user_data)?;
		let data = self.joint_list[index].as_mut().expect("관절");
		data.reference_angle = PhysicsMath2D::unwind_angle(definition.reference_angle);
		data.enable_spring = definition.enable_spring;
		data.hertz = definition.hertz.max(0.0);
		data.damping_ratio = definition.damping_ratio.max(0.0);
		data.enable_motor = definition.enable_motor;
		data.max_motor_torque = definition.max_motor_torque.max(0.0);
		data.motor_speed = definition.motor_speed;
		data.enable_limit = definition.enable_limit;
		data.lower_limit = definition.lower_angle.min(definition.upper_angle);
		data.upper_limit = definition.lower_angle.max(definition.upper_angle);
		Some(data.id)
	}

	//==============================================================================
	/// 용접 관절 만들기.
	///
	/// # 인자
	/// - `definition`: 용접 관절 정의.
	///
	/// # 돌려줌
	/// 새 관절 아이디. (몸이 없으면 None)
	//==============================================================================
	pub fn create_weld_joint(&mut self, definition: &WeldJointDefinition2D) -> Option<JointId2D> {
		let index = self.create_joint_internal(JointType2D::Weld, definition.body_id_a, definition.body_id_b, definition.local_anchor_a, definition.local_anchor_b, definition.collide_connected, definition.user_data)?;
		let data = self.joint_list[index].as_mut().expect("관절");
		data.reference_angle = PhysicsMath2D::unwind_angle(definition.reference_angle);
		data.linear_hertz = definition.linear_hertz.max(0.0);
		data.linear_damping_ratio = definition.linear_damping_ratio.max(0.0);
		data.angular_hertz = definition.angular_hertz.max(0.0);
		data.angular_damping_ratio = definition.angular_damping_ratio.max(0.0);
		Some(data.id)
	}

	//==============================================================================
	/// 바퀴 관절 만들기.
	///
	/// # 인자
	/// - `definition`: 바퀴 관절 정의.
	///
	/// # 돌려줌
	/// 새 관절 아이디. (몸이 없거나 축 길이가 0 이면 None)
	//==============================================================================
	pub fn create_wheel_joint(&mut self, definition: &WheelJointDefinition2D) -> Option<JointId2D> {
		let axis_length = (definition.local_axis_a.x * definition.local_axis_a.x + definition.local_axis_a.y * definition.local_axis_a.y).sqrt();
		if axis_length <= 0.0 || axis_length.is_nan() {
			return None;
		}
		let index = self.create_joint_internal(JointType2D::Wheel, definition.body_id_a, definition.body_id_b, definition.local_anchor_a, definition.local_anchor_b, definition.collide_connected, definition.user_data)?;
		let data = self.joint_list[index].as_mut().expect("관절");
		data.local_axis_a_x = definition.local_axis_a.x / axis_length;
		data.local_axis_a_y = definition.local_axis_a.y / axis_length;
		data.enable_spring = definition.enable_spring;
		data.hertz = definition.hertz.max(0.0);
		data.damping_ratio = definition.damping_ratio.max(0.0);
		data.enable_motor = definition.enable_motor;
		data.max_motor_torque = definition.max_motor_torque.max(0.0);
		data.motor_speed = definition.motor_speed;
		data.enable_limit = definition.enable_limit;
		data.lower_limit = definition.lower_translation.min(definition.upper_translation);
		data.upper_limit = definition.lower_translation.max(definition.upper_translation);
		Some(data.id)
	}

	//==============================================================================
	/// 관절 없애기. (두 몸을 깨우고, 서로 부딪히지 않던 관절이면 다시 짝을 찾는다)
	///
	/// # 인자
	/// - `joint_id`: 없앨 관절.
	//==============================================================================
	pub fn destroy_joint(&mut self, joint_id: JointId2D) {
		let Some(index) = self.resolve_joint_index(joint_id) else {
			return;
		};
		let (body_index_a, body_index_b, collide_connected) = {
			let data = self.joint_list[index].as_ref().expect("관절");
			(data.body_index_a, data.body_index_b, data.collide_connected)
		};
		self.wake_body_index(body_index_a);
		self.wake_body_index(body_index_b);
		if !collide_connected {
			for body_index in [body_index_a, body_index_b] {
				let shape_id_list = self.body_list[body_index].as_ref().map(|body| body.shape_id_list.clone()).unwrap_or_default();
				for shape_id in shape_id_list {
					self.push_moved_shape(shape_id.get_index());
				}
			}
		}
		self.joint_list[index] = None;
		self.joint_generation_list[index] += 1;
		self.joint_free_list.push(index);
	}

	//==============================================================================
	/// 관절 아이디의 칸 번호. (없거나 세대가 다르면 None)
	//==============================================================================
	fn resolve_joint_index(&self, joint_id: JointId2D) -> Option<usize> {
		let index = joint_id.get_index();
		if index >= self.joint_list.len() || self.joint_list[index].is_none() || self.joint_generation_list[index] != joint_id.get_generation() {
			return None;
		}
		Some(index)
	}

	//==============================================================================
	/// 관절 반환.
	///
	/// # 인자
	/// - `joint_id`: 관절 아이디.
	///
	/// # 돌려줌
	/// 관절. (없으면 None)
	//==============================================================================
	pub fn get_joint(&self, joint_id: JointId2D) -> Option<&Joint2D> {
		let index = self.resolve_joint_index(joint_id)?;
		self.joint_list[index].as_ref()
	}

	//==============================================================================
	/// 관절 반환. (변경용)
	///
	/// # 인자
	/// - `joint_id`: 관절 아이디.
	///
	/// # 돌려줌
	/// 관절. (없으면 None)
	//==============================================================================
	pub fn get_joint_mut(&mut self, joint_id: JointId2D) -> Option<&mut Joint2D> {
		let index = self.resolve_joint_index(joint_id)?;
		self.joint_list[index].as_mut()
	}

	//==============================================================================
	/// 살아 있는 관절 아이디 목록. (칸 차례)
	///
	/// # 돌려줌
	/// 관절 아이디 목록.
	//==============================================================================
	pub fn get_joint_ids(&self) -> Vec<JointId2D> {
		self.joint_list.iter().flatten().map(|joint| joint.id).collect()
	}

	//==============================================================================
	/// 몸 수 반환.
	///
	/// # 돌려줌
	/// 살아 있는 몸 수.
	//==============================================================================
	pub fn get_body_count(&self) -> usize {
		self.body_list.len() - self.body_free_list.len()
	}

	//==============================================================================
	/// 깨어 있는 몸 수 반환. (정적 몸 빼고)
	///
	/// # 돌려줌
	/// 깨어 있는 몸 수.
	//==============================================================================
	pub fn get_awake_body_count(&self) -> usize {
		self.body_list.iter().flatten().filter(|body| body.body_type != BodyType2D::Static && body.is_awake).count()
	}

	//==============================================================================
	/// 모양 수 반환.
	///
	/// # 돌려줌
	/// 살아 있는 모양 수.
	//==============================================================================
	pub fn get_shape_count(&self) -> usize {
		self.shape_list.len() - self.shape_free_list.len()
	}

	//==============================================================================
	/// 관절 수 반환.
	///
	/// # 돌려줌
	/// 살아 있는 관절 수.
	//==============================================================================
	pub fn get_joint_count(&self) -> usize {
		self.joint_list.len() - self.joint_free_list.len()
	}

	//==============================================================================
	/// 닿음 수 반환. (뚱뚱한 상자가 겹친 짝, 닿지 않은 것과 센서 포함)
	///
	/// # 돌려줌
	/// 닿음 수.
	//==============================================================================
	pub fn get_contact_count(&self) -> usize {
		self.contact_list.len() - self.contact_free_list.len()
	}

	//==============================================================================
	/// 지난 걸음의 닿기 시작 사건 목록. (다음 step 까지 쓴다)
	///
	/// # 돌려줌
	/// 닿기 시작 사건.
	//==============================================================================
	pub fn get_contact_begin_events(&self) -> &[ContactEvent2D] {
		&self.contact_begin_event_list
	}

	//==============================================================================
	/// 지난 걸음의 닿기 끝 사건 목록. (모양을 없애 끝난 것 포함)
	///
	/// # 돌려줌
	/// 닿기 끝 사건.
	//==============================================================================
	pub fn get_contact_end_events(&self) -> &[ContactEvent2D] {
		&self.contact_end_event_list
	}

	//==============================================================================
	/// 지난 걸음의 센서 들어옴 사건 목록. (다음 step 까지 쓴다)
	///
	/// # 돌려줌
	/// 센서 들어옴 사건.
	//==============================================================================
	pub fn get_sensor_begin_events(&self) -> &[SensorEvent2D] {
		&self.sensor_begin_event_list
	}

	//==============================================================================
	/// 지난 걸음의 센서 나감 사건 목록. (센서나 모양을 없애 나간 것 포함)
	///
	/// # 돌려줌
	/// 센서 나감 사건.
	//==============================================================================
	pub fn get_sensor_end_events(&self) -> &[SensorEvent2D] {
		&self.sensor_end_event_list
	}

	//==============================================================================
	/// 고정 걸음 시간 설정. (advance 가 쓴다, 기본 1 / 60, 0 보다 커야 한다)
	///
	/// # 인자
	/// - `step_seconds`: 한 걸음의 시간. (초)
	//==============================================================================
	pub fn set_fixed_step_seconds(&mut self, step_seconds: f64) {
		if !step_seconds.is_finite() || step_seconds <= 0.0 {
			return;
		}
		self.fixed_step_seconds = step_seconds;
	}

	//==============================================================================
	/// 고정 걸음 시간 반환.
	///
	/// # 돌려줌
	/// 한 걸음의 시간. (초)
	//==============================================================================
	pub fn get_fixed_step_seconds(&self) -> f64 {
		self.fixed_step_seconds
	}

	//==============================================================================
	/// 하위 걸음 수 설정. (advance 가 쓴다, 기본 4, 1 이상)
	///
	/// # 인자
	/// - `sub_step_count`: 한 걸음의 하위 걸음 수.
	//==============================================================================
	pub fn set_sub_step_count(&mut self, sub_step_count: u32) {
		self.sub_step_count = sub_step_count.max(1);
	}

	//==============================================================================
	/// 하위 걸음 수 반환.
	///
	/// # 돌려줌
	/// 한 걸음의 하위 걸음 수.
	//==============================================================================
	pub fn get_sub_step_count(&self) -> u32 {
		self.sub_step_count
	}

	//==============================================================================
	/// 프레임 시간만큼 고정 걸음으로 나아가기. (그리기 보간 몫을 돌려준다)
	/// - 원본 FixedStepper 와 같은 셈이다. 한 프레임에 5 걸음까지 돌고, 그러고도 남은 시간은 버린다.
	///
	/// # 인자
	/// - `time_delta`: 이번 프레임의 시간. (초)
	///
	/// # 돌려줌
	/// 앞 걸음과 지금 걸음 사이 몫. (0 ~ 1, Body2D::get_interpolated_position 에 준다)
	//==============================================================================
	pub fn advance(&mut self, time_delta: f64) -> f64 {
		let step_seconds = self.fixed_step_seconds;
		if time_delta.is_finite() && time_delta > 0.0 {
			self.accumulated_seconds += time_delta;
		}
		let mut step_count = 0;
		while self.accumulated_seconds >= step_seconds && step_count < self.maximum_steps_per_frame {
			let sub_step_count = self.sub_step_count;
			self.step(step_seconds, sub_step_count);
			self.accumulated_seconds -= step_seconds;
			step_count += 1;
		}
		if step_count >= self.maximum_steps_per_frame && self.accumulated_seconds >= step_seconds {
			self.accumulated_seconds = 0.0;
		}
		let alpha = self.accumulated_seconds / step_seconds;
		self.interpolation_alpha = 0.0f64.max(1.0f64.min(alpha));
		self.interpolation_alpha
	}

	//==============================================================================
	/// 지난 advance 의 보간 몫 반환.
	///
	/// # 돌려줌
	/// 보간 몫. (0 ~ 1)
	//==============================================================================
	pub fn get_interpolation_alpha(&self) -> f64 {
		self.interpolation_alpha
	}

	//==============================================================================
	/// 한 걸음. (짝 찾기, 닿음점, 섬과 깨우기, 풀이, 잠자기)
	///
	/// # 인자
	/// - `time_step`: 걸음 시간. (초, 0 이면 닿음만 고친다)
	/// - `sub_step_count`: 하위 걸음 수. (1 이상, 보통 4)
	//==============================================================================
	pub fn step(&mut self, time_step: f64, sub_step_count: u32) {
		self.contact_begin_event_list.clear();
		self.contact_end_event_list.clear();
		self.sensor_begin_event_list.clear();
		self.sensor_end_event_list.clear();
		for data in self.body_list.iter_mut().flatten() {
			data.previous_position_x = data.position_x;
			data.previous_position_y = data.position_y;
			data.previous_rotation_cosine = data.rotation_cosine;
			data.previous_rotation_sine = data.rotation_sine;
		}
		self.apply_pending_changes();
		self.update_pairs();
		self.collide();
		if time_step <= 0.0 || time_step.is_nan() {
			return;
		}
		let step_count = sub_step_count.max(1);
		let inverse_time_step = 1.0 / time_step;
		let sub_step_time = time_step / step_count as f64;
		let inverse_sub_step_time = step_count as f64 * inverse_time_step;
		let contact_hertz = self.contact_hertz.min(0.25 * inverse_sub_step_time);
		let joint_hertz = self.joint_hertz.min(0.125 * inverse_sub_step_time);
		let context = StepContext {
			time_step,
			inverse_time_step,
			sub_step_time,
			inverse_sub_step_time,
			contact_softness: make_softness(contact_hertz, self.contact_damping_ratio, sub_step_time),
			static_softness: make_softness(2.0 * contact_hertz, self.contact_damping_ratio, sub_step_time),
			joint_softness: make_softness(joint_hertz, self.joint_damping_ratio, sub_step_time),
			restitution_threshold: self.restitution_threshold,
			maximum_bias_velocity: self.maximum_contact_push_speed,
			maximum_linear_speed: self.maximum_linear_speed,
			maximum_angular_speed: MAXIMUM_ROTATION * inverse_sub_step_time,
			gravity_x: self.gravity_x,
			gravity_y: self.gravity_y,
			contact_hertz,
			joint_damping_ratio: self.joint_damping_ratio,
		};
		self.solve(&context, step_count);
	}

	//==============================================================================
	/// 걸음 사이에 바뀐 것 반영. (질량, 관절 깨우기, 거르기, 순간 이동)
	//==============================================================================
	fn apply_pending_changes(&mut self) {
		for body_index in 0..self.body_list.len() {
			let is_mass_dirty = self.body_list[body_index].as_ref().map(|body| body.is_mass_dirty).unwrap_or(false);
			if is_mass_dirty {
				self.update_body_mass(body_index);
			}
		}
		for joint_index in 0..self.joint_list.len() {
			let wake_pair = match self.joint_list[joint_index].as_mut() {
				Some(joint) if joint.is_wake_requested => {
					joint.is_wake_requested = false;
					Some((joint.body_index_a, joint.body_index_b))
				}
				_ => None,
			};
			if let Some((body_index_a, body_index_b)) = wake_pair {
				self.wake_body_index(body_index_a);
				self.wake_body_index(body_index_b);
			}
		}
		for shape_index in 0..self.shape_list.len() {
			let is_filter_dirty = match self.shape_list[shape_index].as_mut() {
				Some(shape) if shape.is_filter_dirty => {
					shape.is_filter_dirty = false;
					true
				}
				_ => false,
			};
			if !is_filter_dirty {
				continue;
			}
			for contact_index in 0..self.contact_list.len() {
				let contact = &self.contact_list[contact_index];
				if contact.is_alive && (contact.shape_index_a == shape_index || contact.shape_index_b == shape_index) {
					let should_collide = self.should_shapes_collide(contact.shape_index_a, contact.shape_index_b);
					if !should_collide {
						self.destroy_contact(contact_index);
					}
				}
			}
			self.push_moved_shape(shape_index);
		}
		self.synchronize_transform_changes();
	}

	//==============================================================================
	/// 순간 이동한 몸의 모양 상자와 나무 고치기.
	//==============================================================================
	fn synchronize_transform_changes(&mut self) {
		for body_index in 0..self.body_list.len() {
			let shape_id_list = match self.body_list[body_index].as_mut() {
				Some(body) if body.is_transform_dirty => {
					body.is_transform_dirty = false;
					body.shape_id_list.clone()
				}
				_ => continue,
			};
			for shape_id in shape_id_list {
				self.synchronize_shape(shape_id.get_index(), true);
			}
		}
	}

	//==============================================================================
	/// 모양의 상자를 몸 자리로 고치고, 뚱뚱한 상자를 벗어났으면 나무에서 옮기기.
	//==============================================================================
	fn synchronize_shape(&mut self, shape_index: usize, is_forced: bool) {
		let aabb_margin = self.aabb_margin;
		let Some(shape_data) = self.shape_list[shape_index].as_mut() else {
			return;
		};
		let body_data = self.body_list[shape_data.body_index].as_ref().expect("몸");
		let mut aabb = AABB2D::new();
		compute_shape_aabb(shape_data, body_data.position_x, body_data.position_y, body_data.rotation_cosine, body_data.rotation_sine, &mut aabb);
		shape_data.aabb = aabb;
		let is_contained = shape_data.fat_aabb.contains(&shape_data.aabb);
		if is_contained && !is_forced {
			return;
		}
		let margin = if body_data.body_type == BodyType2D::Static {
			0.0
		}
		else {
			aabb_margin
		};
		shape_data.fat_aabb.set(aabb.min_x - margin, aabb.min_y - margin, aabb.max_x + margin, aabb.max_y + margin);
		let proxy_id = shape_data.proxy_id;
		let fat_aabb = shape_data.fat_aabb;
		self.tree.move_proxy(proxy_id, &fat_aabb);
		self.push_moved_shape(shape_index);
	}

	//==============================================================================
	/// 두 모양이 닿음을 가질 수 있는지.
	//==============================================================================
	fn should_shapes_collide(&self, shape_index_a: usize, shape_index_b: usize) -> bool {
		let shape_a = self.shape_list[shape_index_a].as_ref().expect("모양");
		let shape_b = self.shape_list[shape_index_b].as_ref().expect("모양");
		if shape_a.body_index == shape_b.body_index {
			return false;
		}
		if shape_a.shape_type == ShapeType2D::Segment && shape_b.shape_type == ShapeType2D::Segment {
			return false;
		}
		let body_a = self.body_list[shape_a.body_index].as_ref().expect("몸");
		let body_b = self.body_list[shape_b.body_index].as_ref().expect("몸");
		if body_a.body_type != BodyType2D::Dynamic && body_b.body_type != BodyType2D::Dynamic {
			return false;
		}
		if shape_a.is_sensor && shape_b.is_sensor {
			return false;
		}
		if (shape_a.is_sensor && !shape_b.enable_sensor_events) || (shape_b.is_sensor && !shape_a.enable_sensor_events) {
			return false;
		}
		if !should_filters_collide(shape_a.category_bits, shape_a.mask_bits, shape_a.group_index, shape_b.category_bits, shape_b.mask_bits, shape_b.group_index) {
			return false;
		}
		for joint in self.joint_list.iter().flatten() {
			if joint.collide_connected {
				continue;
			}
			if (joint.body_index_a == shape_a.body_index && joint.body_index_b == shape_b.body_index) || (joint.body_index_a == shape_b.body_index && joint.body_index_b == shape_a.body_index) {
				return false;
			}
		}
		true
	}

	//==============================================================================
	/// 옮겨진 모양의 새 짝 찾기. (나무 질의, 열쇠 차례로 닿음을 만든다)
	//==============================================================================
	fn update_pairs(&mut self) {
		let mut key_list: Vec<u64> = Vec::new();
		{
			let tree = &self.tree;
			let shape_moved_list = &self.shape_moved_list;
			for &shape_index in self.move_buffer.iter() {
				let Some(shape_data) = self.shape_list[shape_index].as_ref() else {
					continue;
				};
				if !shape_moved_list[shape_index] {
					continue;
				}
				tree.query(&shape_data.fat_aabb, u32::MAX, |proxy_id| {
					let other_index = *tree.get_user_data(proxy_id).expect("모양 번호");
					if other_index == shape_index {
						return true;
					}
					// 둘 다 옮겨졌으면 번호가 작은 쪽에서 한 번만 넣는다.
					if shape_moved_list[other_index] && other_index < shape_index {
						return true;
					}
					let minimum_index = shape_index.min(other_index) as u64;
					let maximum_index = shape_index.max(other_index) as u64;
					key_list.push(minimum_index * CONTACT_KEY_MULTIPLIER + maximum_index);
					true
				});
			}
		}
		for &shape_index in self.move_buffer.iter() {
			self.shape_moved_list[shape_index] = false;
		}
		self.move_buffer.clear();
		key_list.sort_unstable();
		let mut previous_key = u64::MAX;
		for key in key_list {
			if key == previous_key {
				continue;
			}
			previous_key = key;
			if self.contact_table.contains_key(&key) {
				continue;
			}
			let shape_index_a = (key / CONTACT_KEY_MULTIPLIER) as usize;
			let shape_index_b = (key - (shape_index_a as u64) * CONTACT_KEY_MULTIPLIER) as usize;
			if !self.should_shapes_collide(shape_index_a, shape_index_b) {
				continue;
			}
			self.create_contact(shape_index_a, shape_index_b, key);
		}
	}

	//==============================================================================
	/// 닿음 만들기. (원은 늘 B 쪽)
	//==============================================================================
	fn create_contact(&mut self, shape_index1: usize, shape_index2: usize, key: u64) {
		let mut shape_index_a = shape_index1;
		let mut shape_index_b = shape_index2;
		let type1 = self.shape_list[shape_index1].as_ref().expect("모양").shape_type;
		let type2 = self.shape_list[shape_index2].as_ref().expect("모양").shape_type;
		if type1 == ShapeType2D::Circle && type2 != ShapeType2D::Circle {
			shape_index_a = shape_index2;
			shape_index_b = shape_index1;
		}
		let index = if let Some(free_index) = self.contact_free_list.pop() {
			free_index
		}
		else {
			self.contact_list.push(Contact::new());
			self.contact_list.len() - 1
		};
		let shape_a = self.shape_list[shape_index_a].as_ref().expect("모양");
		let shape_b = self.shape_list[shape_index_b].as_ref().expect("모양");
		let contact = &mut self.contact_list[index];
		contact.shape_index_a = shape_index_a;
		contact.shape_index_b = shape_index_b;
		contact.body_index_a = shape_a.body_index;
		contact.body_index_b = shape_b.body_index;
		contact.key = key;
		contact.is_sensor = shape_a.is_sensor || shape_b.is_sensor;
		contact.is_touching = false;
		contact.is_alive = true;
		contact.manifold.point_count = 0;
		self.contact_table.insert(key, index);
	}

	//==============================================================================
	/// 닿음 지우기. (닿아 있었으면 끝 사건을 낸다)
	//==============================================================================
	fn destroy_contact(&mut self, contact_index: usize) {
		if self.contact_list[contact_index].is_touching {
			push_end_event(&self.shape_list, &self.contact_list[contact_index], &mut self.contact_end_event_list, &mut self.sensor_end_event_list);
		}
		let contact = &mut self.contact_list[contact_index];
		self.contact_table.remove(&contact.key);
		contact.is_alive = false;
		contact.is_touching = false;
		contact.manifold.point_count = 0;
		self.contact_free_list.push(contact_index);
	}

	//==============================================================================
	/// 닿음점 고치기. (깨어 있는 몸이 낀 닿음만)
	//==============================================================================
	fn collide(&mut self) {
		let mut old_id_list = [0u32; 2];
		let mut old_normal_impulse_list = [0.0; 2];
		let mut old_tangent_impulse_list = [0.0; 2];
		for contact_index in 0..self.contact_list.len() {
			if !self.contact_list[contact_index].is_alive {
				continue;
			}
			let (shape_index_a, shape_index_b, body_index_a, body_index_b) = {
				let contact = &self.contact_list[contact_index];
				(contact.shape_index_a, contact.shape_index_b, contact.body_index_a, contact.body_index_b)
			};
			let is_overlapped = {
				let shape_a = self.shape_list[shape_index_a].as_ref().expect("모양");
				let shape_b = self.shape_list[shape_index_b].as_ref().expect("모양");
				let body_a = self.body_list[body_index_a].as_ref().expect("몸");
				let body_b = self.body_list[body_index_b].as_ref().expect("몸");
				let is_active_a = body_a.body_type != BodyType2D::Static && body_a.is_awake;
				let is_active_b = body_b.body_type != BodyType2D::Static && body_b.is_awake;
				if !is_active_a && !is_active_b {
					continue;
				}
				shape_a.fat_aabb.overlaps(&shape_b.fat_aabb)
			};
			if !is_overlapped {
				self.destroy_contact(contact_index);
				continue;
			}
			let shape_a = self.shape_list[shape_index_a].as_ref().expect("모양");
			let shape_b = self.shape_list[shape_index_b].as_ref().expect("모양");
			let body_a = self.body_list[body_index_a].as_ref().expect("몸");
			let body_b = self.body_list[body_index_b].as_ref().expect("몸");
			fill_world_shape(shape_a, body_a.position_x, body_a.position_y, body_a.rotation_cosine, body_a.rotation_sine, &mut self.world_shape_a);
			fill_world_shape(shape_b, body_b.position_x, body_b.position_y, body_b.rotation_cosine, body_b.rotation_sine, &mut self.world_shape_b);
			let contact = &mut self.contact_list[contact_index];
			let was_touching = contact.is_touching;
			if contact.is_sensor {
				let manifold = &mut self.scratch_manifold;
				collide_shapes(&self.world_shape_a, &self.world_shape_b, 0.0, self.linear_slop, manifold);
				let mut is_touching = false;
				for point_index in 0..manifold.point_count {
					if manifold.points[point_index].separation < 0.0 {
						is_touching = true;
					}
				}
				contact.is_touching = is_touching;
				if is_touching && !was_touching {
					let (sensor_shape, visitor_shape) = if shape_a.is_sensor {
						(shape_a, shape_b)
					}
					else {
						(shape_b, shape_a)
					};
					if visitor_shape.enable_sensor_events {
						self.sensor_begin_event_list.push(SensorEvent2D {
							sensor_shape_id: sensor_shape.id,
							visitor_shape_id: visitor_shape.id,
						});
					}
				}
				else if !is_touching && was_touching {
					push_end_event(&self.shape_list, contact, &mut self.contact_end_event_list, &mut self.sensor_end_event_list);
				}
				continue;
			}
			let manifold = &mut contact.manifold;
			let old_point_count = manifold.point_count;
			for point_index in 0..old_point_count {
				old_id_list[point_index] = manifold.points[point_index].id;
				old_normal_impulse_list[point_index] = manifold.points[point_index].normal_impulse;
				old_tangent_impulse_list[point_index] = manifold.points[point_index].tangent_impulse;
			}
			collide_shapes(&self.world_shape_a, &self.world_shape_b, self.speculative_distance, self.linear_slop, manifold);
			for point_index in 0..manifold.point_count {
				let manifold_point = &mut manifold.points[point_index];
				manifold_point.normal_impulse = 0.0;
				manifold_point.tangent_impulse = 0.0;
				for old_index in 0..old_point_count {
					if old_id_list[old_index] == manifold_point.id {
						manifold_point.normal_impulse = old_normal_impulse_list[old_index];
						manifold_point.tangent_impulse = old_tangent_impulse_list[old_index];
						break;
					}
				}
			}
			let is_touching = manifold.point_count > 0;
			contact.is_touching = is_touching;
			if is_touching && !was_touching {
				if shape_a.enable_contact_events || shape_b.enable_contact_events {
					self.contact_begin_event_list.push(ContactEvent2D {
						shape_id_a: shape_a.id,
						shape_id_b: shape_b.id,
					});
				}
			}
			else if !is_touching && was_touching {
				push_end_event(&self.shape_list, contact, &mut self.contact_end_event_list, &mut self.sensor_end_event_list);
			}
		}
	}

	//==============================================================================
	/// 풀이. (섬 짓기와 깨우기, 하위 걸음, 반발, 몸 마무리, 잠자기)
	//==============================================================================
	fn solve(&mut self, context: &StepContext, sub_step_count: u32) {
		let body_count = self.body_list.len();

		// 섬. (닿은 닿음과 관절로 이은 정적 아닌 몸들)
		for body_index in 0..body_count {
			if let Some(data) = self.body_list[body_index].as_mut() {
				data.island_parent = if data.body_type == BodyType2D::Static {
					NO_INDEX
				}
				else {
					body_index
				};
			}
		}
		for contact_index in 0..self.contact_list.len() {
			let contact = &self.contact_list[contact_index];
			if !contact.is_alive || contact.is_sensor || !contact.is_touching {
				continue;
			}
			let (body_index_a, body_index_b) = (contact.body_index_a, contact.body_index_b);
			if island_parent_of(&self.body_list, body_index_a) == NO_INDEX || island_parent_of(&self.body_list, body_index_b) == NO_INDEX {
				continue;
			}
			link_islands(&mut self.body_list, body_index_a, body_index_b);
		}
		for joint_index in 0..self.joint_list.len() {
			let Some(joint) = self.joint_list[joint_index].as_ref() else {
				continue;
			};
			let (body_index_a, body_index_b) = (joint.body_index_a, joint.body_index_b);
			if island_parent_of(&self.body_list, body_index_a) == NO_INDEX || island_parent_of(&self.body_list, body_index_b) == NO_INDEX {
				continue;
			}
			link_islands(&mut self.body_list, body_index_a, body_index_b);
		}
		// 깨어 있는 몸이 하나라도 있는 섬은 모두 깨운다.
		let mut island_awake_list = vec![false; body_count];
		for body_index in 0..body_count {
			let is_awake_member = match self.body_list[body_index].as_ref() {
				Some(data) => data.island_parent != NO_INDEX && data.is_awake,
				None => false,
			};
			if is_awake_member {
				let root = find_island_root(&mut self.body_list, body_index);
				island_awake_list[root] = true;
			}
		}
		for body_index in 0..body_count {
			let is_sleeping_member = match self.body_list[body_index].as_ref() {
				Some(data) => data.island_parent != NO_INDEX && !data.is_awake,
				None => false,
			};
			if is_sleeping_member {
				let root = find_island_root(&mut self.body_list, body_index);
				if island_awake_list[root] {
					let data = self.body_list[body_index].as_mut().expect("몸");
					data.is_awake = true;
					data.sleep_time = 0.0;
				}
			}
		}

		// 풀이 몸.
		let mut solver_body_count = 0;
		for body_index in 0..body_count {
			let Some(data) = self.body_list[body_index].as_mut() else {
				continue;
			};
			data.solver_index = NO_INDEX;
			if data.body_type == BodyType2D::Static || !data.is_awake {
				continue;
			}
			if solver_body_count >= self.solver_body_list.len() {
				self.solver_body_list.push(STATIC_SOLVER_BODY);
			}
			let solver_body = &mut self.solver_body_list[solver_body_count];
			solver_body.body_index = body_index;
			solver_body.linear_velocity_x = data.linear_velocity_x;
			solver_body.linear_velocity_y = data.linear_velocity_y;
			solver_body.angular_velocity = data.angular_velocity;
			solver_body.delta_position_x = 0.0;
			solver_body.delta_position_y = 0.0;
			solver_body.delta_rotation_cosine = 1.0;
			solver_body.delta_rotation_sine = 0.0;
			solver_body.inverse_mass = data.inverse_mass;
			solver_body.inverse_inertia = data.inverse_inertia;
			data.solver_index = solver_body_count;
			solver_body_count += 1;
		}
		self.solver_body_count = solver_body_count;

		// 닿음과 관절 준비.
		let mut active_contact_list: Vec<usize> = Vec::new();
		for contact_index in 0..self.contact_list.len() {
			let contact = &mut self.contact_list[contact_index];
			if !contact.is_alive || contact.is_sensor || !contact.is_touching {
				continue;
			}
			let body_a = self.body_list[contact.body_index_a].as_ref().expect("몸");
			let body_b = self.body_list[contact.body_index_b].as_ref().expect("몸");
			if body_a.solver_index == NO_INDEX && body_b.solver_index == NO_INDEX {
				continue;
			}
			let shape_a = self.shape_list[contact.shape_index_a].as_ref().expect("모양");
			let shape_b = self.shape_list[contact.shape_index_b].as_ref().expect("모양");
			contact.friction = (shape_a.friction * shape_b.friction).sqrt();
			contact.restitution = shape_a.restitution.max(shape_b.restitution);
			prepare_contact(contact, body_a, body_b, &self.solver_body_list, context);
			active_contact_list.push(contact_index);
		}
		let mut active_joint_list: Vec<usize> = Vec::new();
		for joint_index in 0..self.joint_list.len() {
			let Some(joint) = self.joint_list[joint_index].as_mut() else {
				continue;
			};
			let body_a = self.body_list[joint.body_index_a].as_ref().expect("몸");
			let body_b = self.body_list[joint.body_index_b].as_ref().expect("몸");
			if body_a.solver_index == NO_INDEX && body_b.solver_index == NO_INDEX {
				continue;
			}
			prepare_joint(joint, body_a, body_b, &self.solver_body_list, context);
			active_joint_list.push(joint_index);
		}

		// 하위 걸음.
		let sub_step_time = context.sub_step_time;
		for _ in 0..sub_step_count {
			integrate_velocities(&self.body_list, &mut self.solver_body_list[..solver_body_count], context);
			for &joint_index in active_joint_list.iter() {
				warm_start_joint(self.joint_list[joint_index].as_ref().expect("관절"), &mut self.solver_body_list);
			}
			for &contact_index in active_contact_list.iter() {
				warm_start_contact(&self.contact_list[contact_index], &mut self.solver_body_list);
			}
			for &joint_index in active_joint_list.iter() {
				solve_joint(self.joint_list[joint_index].as_mut().expect("관절"), &mut self.solver_body_list, context, true);
			}
			for &contact_index in active_contact_list.iter() {
				solve_contact(&mut self.contact_list[contact_index], &mut self.solver_body_list, context, true);
			}
			for solver_body in self.solver_body_list[..solver_body_count].iter_mut() {
				let (rotation_cosine, rotation_sine) = integrate_rotation(solver_body.delta_rotation_cosine, solver_body.delta_rotation_sine, sub_step_time * solver_body.angular_velocity);
				solver_body.delta_rotation_cosine = rotation_cosine;
				solver_body.delta_rotation_sine = rotation_sine;
				solver_body.delta_position_x += sub_step_time * solver_body.linear_velocity_x;
				solver_body.delta_position_y += sub_step_time * solver_body.linear_velocity_y;
			}
			for &joint_index in active_joint_list.iter() {
				solve_joint(self.joint_list[joint_index].as_mut().expect("관절"), &mut self.solver_body_list, context, false);
			}
			for &contact_index in active_contact_list.iter() {
				solve_contact(&mut self.contact_list[contact_index], &mut self.solver_body_list, context, false);
			}
		}
		for &contact_index in active_contact_list.iter() {
			apply_contact_restitution(&mut self.contact_list[contact_index], &mut self.solver_body_list, context);
		}

		// 몸 마무리. (자리, 상자, 잠들 시간)
		let time_step = context.time_step;
		let inverse_time_step = context.inverse_time_step;
		for solver_index in 0..solver_body_count {
			let solver_body = self.solver_body_list[solver_index];
			let shape_id_list = {
				let data = self.body_list[solver_body.body_index].as_mut().expect("몸");
				data.linear_velocity_x = solver_body.linear_velocity_x;
				data.linear_velocity_y = solver_body.linear_velocity_y;
				data.angular_velocity = solver_body.angular_velocity;
				data.center_x += solver_body.delta_position_x;
				data.center_y += solver_body.delta_position_y;
				let cosine = solver_body.delta_rotation_cosine * data.rotation_cosine - solver_body.delta_rotation_sine * data.rotation_sine;
				let sine = solver_body.delta_rotation_sine * data.rotation_cosine + solver_body.delta_rotation_cosine * data.rotation_sine;
				let magnitude = (cosine * cosine + sine * sine).sqrt();
				let inverse_magnitude = if magnitude > 0.0 {
					1.0 / magnitude
				}
				else {
					0.0
				};
				data.rotation_cosine = cosine * inverse_magnitude;
				data.rotation_sine = sine * inverse_magnitude;
				data.position_x = data.center_x - (data.rotation_cosine * data.local_center_x - data.rotation_sine * data.local_center_y);
				data.position_y = data.center_y - (data.rotation_sine * data.local_center_x + data.rotation_cosine * data.local_center_y);
				data.force_x = 0.0;
				data.force_y = 0.0;
				data.torque = 0.0;

				let speed = (data.linear_velocity_x * data.linear_velocity_x + data.linear_velocity_y * data.linear_velocity_y).sqrt();
				let maximum_velocity = speed + data.angular_velocity.abs() * data.maximum_extent;
				let delta_length = (solver_body.delta_position_x * solver_body.delta_position_x + solver_body.delta_position_y * solver_body.delta_position_y).sqrt();
				let maximum_delta_position = delta_length + solver_body.delta_rotation_sine.abs() * data.maximum_extent;
				// 밀어내기로 움직인 것도 본다. (다른 구속이 계속 밀고 있을 수 있다)
				let sleep_velocity = maximum_velocity.max(0.5 * inverse_time_step * maximum_delta_position);
				if !self.enable_sleep || !data.enable_sleep || sleep_velocity > self.sleep_threshold {
					data.sleep_time = 0.0;
				}
				else {
					data.sleep_time += time_step;
				}
				data.shape_id_list.clone()
			};
			for shape_id in shape_id_list {
				self.synchronize_shape(shape_id.get_index(), false);
			}
		}

		// 잠자기. (섬의 모든 몸이 충분히 가만히 있었으면 섬째 재운다)
		if !self.enable_sleep {
			return;
		}
		let mut island_sleep_time_list = vec![f64::INFINITY; body_count];
		for body_index in 0..body_count {
			let sleep_time = match self.body_list[body_index].as_ref() {
				Some(data) if data.island_parent != NO_INDEX => data.sleep_time,
				_ => continue,
			};
			let root = find_island_root(&mut self.body_list, body_index);
			island_sleep_time_list[root] = island_sleep_time_list[root].min(sleep_time);
		}
		for body_index in 0..body_count {
			let is_awake_member = match self.body_list[body_index].as_ref() {
				Some(data) => data.island_parent != NO_INDEX && data.is_awake,
				None => false,
			};
			if !is_awake_member {
				continue;
			}
			let root = find_island_root(&mut self.body_list, body_index);
			if island_sleep_time_list[root] >= TIME_TO_SLEEP {
				let data = self.body_list[body_index].as_mut().expect("몸");
				data.is_awake = false;
				data.linear_velocity_x = 0.0;
				data.linear_velocity_y = 0.0;
				data.angular_velocity = 0.0;
			}
		}
	}

	//==============================================================================
	/// 모양이 질의 거르기를 지나는지.
	//==============================================================================
	fn is_query_passed(shape_data: &Shape2D, query_filter: &QueryFilter2D) -> bool {
		(shape_data.category_bits & query_filter.mask_bits) != 0 && (shape_data.mask_bits & query_filter.category_bits) != 0
	}

	//==============================================================================
	/// 광선 쏘기. (지나는 모양마다 callback, 센서는 맞지 않는다)
	/// - callback(shape_id, point, normal, fraction) 이 돌려주는 값: 0 이면 멈춘다, 음수면 그 모양을 무시한다,
	///   그 밖에는 그 몫으로 광선을 줄인다. 가장 가까운 것만 원하면 fraction 을, 모두 원하면 1 을 돌려준다.
	///
	/// # 인자
	/// - `origin`: 시작점.
	/// - `translation`: 끝점까지의 이동.
	/// - `query_filter`: 질의 거르기.
	/// - `callback`: (shape_id, point, normal, fraction) -> 새 몫.
	//==============================================================================
	pub fn cast_ray(&mut self, origin: impl Into<PhysicsVector2>, translation: impl Into<PhysicsVector2>, query_filter: &QueryFilter2D, mut callback: impl FnMut(ShapeId2D, PhysicsVector2, PhysicsVector2, f64) -> f64) {
		let origin = origin.into();
		let translation = translation.into();
		self.synchronize_transform_changes();
		let tree = &self.tree;
		let shape_list = &self.shape_list;
		let body_list = &self.body_list;
		let mut world_shape = WorldShape::new();
		tree.ray_cast(origin.x, origin.y, translation.x, translation.y, u32::MAX, |proxy_id, maximum_fraction| {
			let shape_index = *tree.get_user_data(proxy_id).expect("모양 번호");
			let shape_data = shape_list[shape_index].as_ref().expect("모양");
			let is_passed = PhysicsWorld2D::is_query_passed(shape_data, query_filter);
			if shape_data.is_sensor || !is_passed {
				return -1.0;
			}
			let body_data = body_list[shape_data.body_index].as_ref().expect("몸");
			fill_world_shape(shape_data, body_data.position_x, body_data.position_y, body_data.rotation_cosine, body_data.rotation_sine, &mut world_shape);
			let Some(hit) = ray_cast_world_shape(&world_shape, origin.x, origin.y, translation.x, translation.y, maximum_fraction) else {
				return -1.0;
			};
			callback(shape_data.id, PhysicsVector2::create(hit[1], hit[2]), PhysicsVector2::create(hit[3], hit[4]), hit[0])
		});
	}

	//==============================================================================
	/// 가장 가까운 맞음 찾기.
	///
	/// # 인자
	/// - `origin`: 시작점.
	/// - `translation`: 끝점까지의 이동.
	/// - `query_filter`: 질의 거르기.
	///
	/// # 돌려줌
	/// 가장 가까운 맞음. (없으면 None)
	//==============================================================================
	pub fn cast_ray_closest(&mut self, origin: impl Into<PhysicsVector2>, translation: impl Into<PhysicsVector2>, query_filter: &QueryFilter2D) -> Option<RayResult2D> {
		let mut closest_result = None;
		self.cast_ray(origin, translation, query_filter, |shape_id, point, normal, fraction| {
			closest_result = Some(RayResult2D {
				shape_id,
				point,
				normal,
				fraction,
			});
			fraction
		});
		closest_result
	}

	//==============================================================================
	/// 상자와 겹치는 모양 찾기. (모양의 경계 상자로 본다, callback 이 false 면 멈춘다)
	///
	/// # 인자
	/// - `aabb`: 찾을 상자.
	/// - `query_filter`: 질의 거르기.
	/// - `callback`: (shape_id) -> 이어 갈지.
	//==============================================================================
	pub fn overlap_aabb(&mut self, aabb: &AABB2D, query_filter: &QueryFilter2D, mut callback: impl FnMut(ShapeId2D) -> bool) {
		self.synchronize_transform_changes();
		let tree = &self.tree;
		let shape_list = &self.shape_list;
		tree.query(aabb, u32::MAX, |proxy_id| {
			let shape_index = *tree.get_user_data(proxy_id).expect("모양 번호");
			let shape_data = shape_list[shape_index].as_ref().expect("모양");
			let is_passed = PhysicsWorld2D::is_query_passed(shape_data, query_filter);
			if !is_passed || !shape_data.aabb.overlaps(aabb) {
				return true;
			}
			callback(shape_data.id)
		});
	}

	//==============================================================================
	/// 점을 담은 모양 찾기. (callback 이 false 면 멈춘다)
	///
	/// # 인자
	/// - `point`: 월드 점.
	/// - `query_filter`: 질의 거르기.
	/// - `callback`: (shape_id) -> 이어 갈지.
	//==============================================================================
	pub fn overlap_point(&mut self, point: impl Into<PhysicsVector2>, query_filter: &QueryFilter2D, mut callback: impl FnMut(ShapeId2D) -> bool) {
		let point = point.into();
		self.synchronize_transform_changes();
		let tree = &self.tree;
		let shape_list = &self.shape_list;
		let body_list = &self.body_list;
		let mut world_shape = WorldShape::new();
		let point_aabb = AABB2D::create(point.x, point.y, point.x, point.y);
		tree.query(&point_aabb, u32::MAX, |proxy_id| {
			let shape_index = *tree.get_user_data(proxy_id).expect("모양 번호");
			let shape_data = shape_list[shape_index].as_ref().expect("모양");
			if !PhysicsWorld2D::is_query_passed(shape_data, query_filter) {
				return true;
			}
			let body_data = body_list[shape_data.body_index].as_ref().expect("몸");
			fill_world_shape(shape_data, body_data.position_x, body_data.position_y, body_data.rotation_cosine, body_data.rotation_sine, &mut world_shape);
			if !test_point_in_world_shape(&world_shape, point.x, point.y) {
				return true;
			}
			callback(shape_data.id)
		});
	}

	//==============================================================================
	/// 모양과 겹치는 모양 찾기. (닿기만 한 것은 빼고 파고든 것만, callback 이 false 면 멈춘다)
	///
	/// # 인자
	/// - `geometry`: 질의 모양. (position, angle 기준)
	/// - `position`: 질의 모양의 자리.
	/// - `angle`: 질의 모양의 돌림. (라디안)
	/// - `query_filter`: 질의 거르기.
	/// - `callback`: (shape_id) -> 이어 갈지.
	//==============================================================================
	pub fn overlap_shape(&mut self, geometry: impl Into<ShapeGeometry2D>, position: impl Into<PhysicsVector2>, angle: f64, query_filter: &QueryFilter2D, mut callback: impl FnMut(ShapeId2D) -> bool) {
		let geometry = geometry.into();
		let position = position.into();
		self.synchronize_transform_changes();
		let mut query_data = Shape2D::new_empty();
		if !set_shape_geometry(&mut query_data, &geometry) {
			return;
		}
		let rotation = PhysicsMath2D::compute_rotation(angle);
		let mut query_shape = WorldShape::new();
		fill_world_shape(&query_data, position.x, position.y, rotation.cosine, rotation.sine, &mut query_shape);
		let mut query_aabb = AABB2D::new();
		compute_shape_aabb(&query_data, position.x, position.y, rotation.cosine, rotation.sine, &mut query_aabb);
		let tree = &self.tree;
		let shape_list = &self.shape_list;
		let body_list = &self.body_list;
		let linear_slop = self.linear_slop;
		let mut world_shape = WorldShape::new();
		let mut manifold = Manifold::default();
		tree.query(&query_aabb, u32::MAX, |proxy_id| {
			let shape_index = *tree.get_user_data(proxy_id).expect("모양 번호");
			let shape_data = shape_list[shape_index].as_ref().expect("모양");
			if !PhysicsWorld2D::is_query_passed(shape_data, query_filter) {
				return true;
			}
			if shape_data.shape_type == ShapeType2D::Segment && query_data.shape_type == ShapeType2D::Segment {
				return true;
			}
			let body_data = body_list[shape_data.body_index].as_ref().expect("몸");
			fill_world_shape(shape_data, body_data.position_x, body_data.position_y, body_data.rotation_cosine, body_data.rotation_sine, &mut world_shape);
			if shape_data.shape_type == ShapeType2D::Circle && query_data.shape_type != ShapeType2D::Circle {
				collide_shapes(&query_shape, &world_shape, 0.0, linear_slop, &mut manifold);
			}
			else {
				collide_shapes(&world_shape, &query_shape, 0.0, linear_slop, &mut manifold);
			}
			let mut is_overlapped = false;
			for point_index in 0..manifold.point_count {
				if manifold.points[point_index].separation < 0.0 {
					is_overlapped = true;
				}
			}
			if !is_overlapped {
				return true;
			}
			callback(shape_data.id)
		});
	}
}

//==============================================================================
/// 닿음이 끝난 사건 넣기. (센서면 센서 나감)
//==============================================================================
fn push_end_event(shape_list: &[Option<Shape2D>], contact: &Contact, contact_end_event_list: &mut Vec<ContactEvent2D>, sensor_end_event_list: &mut Vec<SensorEvent2D>) {
	let shape_a = shape_list[contact.shape_index_a].as_ref().expect("모양");
	let shape_b = shape_list[contact.shape_index_b].as_ref().expect("모양");
	if contact.is_sensor {
		let (sensor_shape, visitor_shape) = if shape_a.is_sensor {
			(shape_a, shape_b)
		}
		else {
			(shape_b, shape_a)
		};
		if visitor_shape.enable_sensor_events {
			sensor_end_event_list.push(SensorEvent2D {
				sensor_shape_id: sensor_shape.id,
				visitor_shape_id: visitor_shape.id,
			});
		}
		return;
	}
	if shape_a.enable_contact_events || shape_b.enable_contact_events {
		contact_end_event_list.push(ContactEvent2D {
			shape_id_a: shape_a.id,
			shape_id_b: shape_b.id,
		});
	}
}

//==============================================================================
/// 몸의 섬 부모. (없거나 정적이면 NO_INDEX)
//==============================================================================
fn island_parent_of(body_list: &[Option<Body2D>], body_index: usize) -> usize {
	body_list[body_index].as_ref().map(|body| body.island_parent).unwrap_or(NO_INDEX)
}

//==============================================================================
/// 섬의 뿌리 찾기. (경로 반 줄이기)
//==============================================================================
fn find_island_root(body_list: &mut [Option<Body2D>], body_index: usize) -> usize {
	let mut index = body_index;
	loop {
		let parent = island_parent_of(body_list, index);
		if parent == index {
			return index;
		}
		let grand_parent = island_parent_of(body_list, parent);
		body_list[index].as_mut().expect("몸").island_parent = grand_parent;
		index = grand_parent;
	}
}

//==============================================================================
/// 두 몸의 섬 잇기. (번호가 작은 뿌리 쪽으로)
//==============================================================================
fn link_islands(body_list: &mut [Option<Body2D>], body_index_a: usize, body_index_b: usize) {
	let root_a = find_island_root(body_list, body_index_a);
	let root_b = find_island_root(body_list, body_index_b);
	if root_a == root_b {
		return;
	}
	if root_a < root_b {
		body_list[root_b].as_mut().expect("몸").island_parent = root_a;
	}
	else {
		body_list[root_a].as_mut().expect("몸").island_parent = root_b;
	}
}

//==============================================================================
/// 속도 적분. (중력, 힘, 감쇠, 속력 한도, 동적 몸만)
//==============================================================================
fn integrate_velocities(body_list: &[Option<Body2D>], solver_body_list: &mut [SolverBody], context: &StepContext) {
	let sub_step_time = context.sub_step_time;
	let maximum_linear_speed = context.maximum_linear_speed;
	let maximum_angular_speed = context.maximum_angular_speed;
	for solver_body in solver_body_list.iter_mut() {
		if solver_body.inverse_mass == 0.0 {
			continue;
		}
		let data = body_list[solver_body.body_index].as_ref().expect("몸");
		let linear_damping_factor = 1.0 / (1.0 + sub_step_time * data.linear_damping);
		let angular_damping_factor = 1.0 / (1.0 + sub_step_time * data.angular_damping);
		let linear_velocity_delta_x = sub_step_time * solver_body.inverse_mass * (data.force_x + data.mass * data.gravity_scale * context.gravity_x);
		let linear_velocity_delta_y = sub_step_time * solver_body.inverse_mass * (data.force_y + data.mass * data.gravity_scale * context.gravity_y);
		let angular_velocity_delta = sub_step_time * solver_body.inverse_inertia * data.torque;
		let mut velocity_x = linear_velocity_delta_x + linear_damping_factor * solver_body.linear_velocity_x;
		let mut velocity_y = linear_velocity_delta_y + linear_damping_factor * solver_body.linear_velocity_y;
		let mut angular_velocity = angular_velocity_delta + angular_damping_factor * solver_body.angular_velocity;
		let speed_squared = velocity_x * velocity_x + velocity_y * velocity_y;
		if speed_squared > maximum_linear_speed * maximum_linear_speed {
			let ratio = maximum_linear_speed / speed_squared.sqrt();
			velocity_x *= ratio;
			velocity_y *= ratio;
		}
		if angular_velocity * angular_velocity > maximum_angular_speed * maximum_angular_speed {
			angular_velocity = if angular_velocity > 0.0 {
				maximum_angular_speed
			}
			else {
				-maximum_angular_speed
			};
		}
		solver_body.linear_velocity_x = velocity_x;
		solver_body.linear_velocity_y = velocity_y;
		solver_body.angular_velocity = angular_velocity;
	}
}

impl Index<BodyId2D> for PhysicsWorld2D {
	type Output = Body2D;

	//==============================================================================
	/// 몸 찾기. (없는 아이디면 멈춘다)
	//==============================================================================
	fn index(&self, body_id: BodyId2D) -> &Body2D {
		self.get_body(body_id).expect("없는 몸 아이디")
	}
}

impl IndexMut<BodyId2D> for PhysicsWorld2D {
	//==============================================================================
	/// 몸 찾기. (변경용, 없는 아이디면 멈춘다)
	//==============================================================================
	fn index_mut(&mut self, body_id: BodyId2D) -> &mut Body2D {
		self.get_body_mut(body_id).expect("없는 몸 아이디")
	}
}

impl Index<ShapeId2D> for PhysicsWorld2D {
	type Output = Shape2D;

	//==============================================================================
	/// 모양 찾기. (없는 아이디면 멈춘다)
	//==============================================================================
	fn index(&self, shape_id: ShapeId2D) -> &Shape2D {
		self.get_shape(shape_id).expect("없는 모양 아이디")
	}
}

impl IndexMut<ShapeId2D> for PhysicsWorld2D {
	//==============================================================================
	/// 모양 찾기. (변경용, 없는 아이디면 멈춘다)
	//==============================================================================
	fn index_mut(&mut self, shape_id: ShapeId2D) -> &mut Shape2D {
		self.get_shape_mut(shape_id).expect("없는 모양 아이디")
	}
}

impl Index<JointId2D> for PhysicsWorld2D {
	type Output = Joint2D;

	//==============================================================================
	/// 관절 찾기. (없는 아이디면 멈춘다)
	//==============================================================================
	fn index(&self, joint_id: JointId2D) -> &Joint2D {
		self.get_joint(joint_id).expect("없는 관절 아이디")
	}
}

impl IndexMut<JointId2D> for PhysicsWorld2D {
	//==============================================================================
	/// 관절 찾기. (변경용, 없는 아이디면 멈춘다)
	//==============================================================================
	fn index_mut(&mut self, joint_id: JointId2D) -> &mut Joint2D {
		self.get_joint_mut(joint_id).expect("없는 관절 아이디")
	}
}
