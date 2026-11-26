//==============================================================================
//! 2D 물리의 관절. (거리, 회전, 용접, 바퀴)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/joint2d.js 와 같은 API, 같은 셈이다.
//! - 원본과 다른 점: 정의의 몸 아이디는 Option 이다. (원본은 null) 사용자 값은 u64 다.
//==============================================================================
use super::body2d::{Body2D, NO_INDEX};
use super::physicsid2d::{BodyId2D, JointId2D};
use super::physicsmath2d::PhysicsMath2D;
use super::physicsvector2::PhysicsVector2;
use super::solver2d::{make_softness, read_solver_body, write_solver_velocity, Softness, SolverBody, StepContext};

/// 길이 한도의 기본 위쪽. (사실상 끝이 없다)
const HUGE_LENGTH: f64 = 1e18;

//==============================================================================
/// 관절의 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JointType2D {
	/// 거리. (두 점 사이 길이를 지키거나 용수철, 밧줄)
	Distance = 0,
	/// 회전. (한 점을 공유하고 돈다, 경첩, 모터와 각도 한도)
	Revolute = 1,
	/// 용접. (자리와 각도를 함께 묶는다, 부드럽게 할 수 있다)
	Weld = 2,
	/// 바퀴. (축 위에서 용수철로 오르내리고 돈다, 수레의 바퀴)
	Wheel = 3,
}

//==============================================================================
/// 거리 관절 정의.
/// - enable_spring 이 false 면 두 닻 사이 길이를 단단히 지킨다. true 면 hertz, damping_ratio 의 용수철이고, hertz 가 0 이면 한도(min_length, max_length)만 남는 밧줄이다.
///
/// # 예제
/// 천장에 매단 추
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, Circle2D, DistanceJointDefinition2D, PhysicsVector2, PhysicsWorld2D, ShapeDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let ceiling_id = world.create_body(&BodyDefinition2D::new());
/// let mut bob_definition = BodyDefinition2D::new();
/// bob_definition.body_type = BodyType2D::Dynamic;
/// bob_definition.position = PhysicsVector2::create(3.0, 0.0);
/// let bob_id = world.create_body(&bob_definition);
/// world.create_shape(bob_id, &ShapeDefinition2D::new(), Circle2D::create(PhysicsVector2::zero(), 0.25));
/// let mut joint_definition = DistanceJointDefinition2D::new();
/// joint_definition.body_id_a = Some(ceiling_id);
/// joint_definition.body_id_b = Some(bob_id);
/// joint_definition.length = 3.0;
/// world.create_distance_joint(&joint_definition);
/// for _ in 0..60 {
/// 	world.step(1.0 / 60.0, 4);
/// }
/// assert!((world[bob_id].get_position().length() - 3.0).abs() < 0.01);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceJointDefinition2D {
	/// 첫 몸.
	pub body_id_a: Option<BodyId2D>,
	/// 둘째 몸.
	pub body_id_b: Option<BodyId2D>,
	/// 첫 몸 기준 닻.
	pub local_anchor_a: PhysicsVector2,
	/// 둘째 몸 기준 닻.
	pub local_anchor_b: PhysicsVector2,
	/// 쉬는 길이. (기본 1)
	pub length: f64,
	/// 용수철로 할지. (기본 false)
	pub enable_spring: bool,
	/// 용수철 진동수. (헤르츠, 기본 0)
	pub hertz: f64,
	/// 용수철 감쇠비. (기본 0)
	pub damping_ratio: f64,
	/// 길이 한도를 쓸지. (용수철일 때만, 기본 false)
	pub enable_limit: bool,
	/// 가장 짧은 길이.
	pub min_length: f64,
	/// 가장 긴 길이.
	pub max_length: f64,
	/// 두 몸끼리 부딪힐지. (기본 false)
	pub collide_connected: bool,
	/// 사용자 값.
	pub user_data: u64,
}

impl Default for DistanceJointDefinition2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> DistanceJointDefinition2D {
		DistanceJointDefinition2D::new()
	}
}

impl DistanceJointDefinition2D {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new() -> DistanceJointDefinition2D {
		DistanceJointDefinition2D {
			body_id_a: None,
			body_id_b: None,
			local_anchor_a: PhysicsVector2::zero(),
			local_anchor_b: PhysicsVector2::zero(),
			length: 1.0,
			enable_spring: false,
			hertz: 0.0,
			damping_ratio: 0.0,
			enable_limit: false,
			min_length: 0.0,
			max_length: HUGE_LENGTH,
			collide_connected: false,
			user_data: 0,
		}
	}
}

//==============================================================================
/// 회전 관절 정의.
/// - 두 몸의 닻이 한 점에 겹치고 그 둘레로 돈다. 관절 각도는 (B 의 돌림 - A 의 돌림 - reference_angle) 이다.
///
/// # 예제
/// 모터로 도는 바퀴
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, Circle2D, PhysicsVector2, PhysicsWorld2D, RevoluteJointDefinition2D, ShapeDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let ground_id = world.create_body(&BodyDefinition2D::new());
/// let mut wheel_definition = BodyDefinition2D::new();
/// wheel_definition.body_type = BodyType2D::Dynamic;
/// let wheel_id = world.create_body(&wheel_definition);
/// world.create_shape(wheel_id, &ShapeDefinition2D::new(), Circle2D::create(PhysicsVector2::zero(), 0.5));
/// let mut joint_definition = RevoluteJointDefinition2D::new();
/// joint_definition.body_id_a = Some(ground_id);
/// joint_definition.body_id_b = Some(wheel_id);
/// joint_definition.enable_motor = true;
/// joint_definition.motor_speed = 2.0;
/// joint_definition.max_motor_torque = 100.0;
/// world.create_revolute_joint(&joint_definition);
/// for _ in 0..30 {
/// 	world.step(1.0 / 60.0, 4);
/// }
/// assert!((world[wheel_id].get_angular_velocity() - 2.0).abs() < 1e-6);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RevoluteJointDefinition2D {
	/// 첫 몸.
	pub body_id_a: Option<BodyId2D>,
	/// 둘째 몸.
	pub body_id_b: Option<BodyId2D>,
	/// 첫 몸 기준 닻.
	pub local_anchor_a: PhysicsVector2,
	/// 둘째 몸 기준 닻.
	pub local_anchor_b: PhysicsVector2,
	/// 관절 각도 0 일 때 B 와 A 의 돌림 차이. (라디안)
	pub reference_angle: f64,
	/// 각도 용수철을 쓸지. (관절 각도 0 쪽으로)
	pub enable_spring: bool,
	/// 용수철 진동수. (헤르츠)
	pub hertz: f64,
	/// 용수철 감쇠비.
	pub damping_ratio: f64,
	/// 모터를 쓸지.
	pub enable_motor: bool,
	/// 모터의 가장 큰 돌림힘.
	pub max_motor_torque: f64,
	/// 모터 각속도. (초당 라디안)
	pub motor_speed: f64,
	/// 각도 한도를 쓸지.
	pub enable_limit: bool,
	/// 가장 작은 관절 각도. (라디안)
	pub lower_angle: f64,
	/// 가장 큰 관절 각도. (라디안)
	pub upper_angle: f64,
	/// 두 몸끼리 부딪힐지. (기본 false)
	pub collide_connected: bool,
	/// 사용자 값.
	pub user_data: u64,
}

impl Default for RevoluteJointDefinition2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> RevoluteJointDefinition2D {
		RevoluteJointDefinition2D::new()
	}
}

impl RevoluteJointDefinition2D {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new() -> RevoluteJointDefinition2D {
		RevoluteJointDefinition2D {
			body_id_a: None,
			body_id_b: None,
			local_anchor_a: PhysicsVector2::zero(),
			local_anchor_b: PhysicsVector2::zero(),
			reference_angle: 0.0,
			enable_spring: false,
			hertz: 0.0,
			damping_ratio: 0.0,
			enable_motor: false,
			max_motor_torque: 0.0,
			motor_speed: 0.0,
			enable_limit: false,
			lower_angle: 0.0,
			upper_angle: 0.0,
			collide_connected: false,
			user_data: 0,
		}
	}
}

//==============================================================================
/// 용접 관절 정의.
/// - 헤르츠가 0 이면 단단히 묶는다. 0 보다 크면 그 진동수의 용수철로 휜다. (나뭇가지, 출렁이는 다리)
///
/// # 예제
/// 두 상자를 붙이기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, PhysicsVector2, PhysicsWorld2D, Polygon2D, ShapeDefinition2D, WeldJointDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let first_id = world.create_body(&BodyDefinition2D::new());
/// let mut second_definition = BodyDefinition2D::new();
/// second_definition.body_type = BodyType2D::Dynamic;
/// second_definition.position = PhysicsVector2::create(1.0, 0.0);
/// let second_id = world.create_body(&second_definition);
/// world.create_shape(second_id, &ShapeDefinition2D::new(), Polygon2D::make_box(0.5, 0.5));
/// let mut joint_definition = WeldJointDefinition2D::new();
/// joint_definition.body_id_a = Some(first_id);
/// joint_definition.body_id_b = Some(second_id);
/// joint_definition.local_anchor_a = PhysicsVector2::create(0.5, 0.0);
/// joint_definition.local_anchor_b = PhysicsVector2::create(-0.5, 0.0);
/// world.create_weld_joint(&joint_definition);
/// for _ in 0..60 {
/// 	world.step(1.0 / 60.0, 4);
/// }
/// assert!(world[second_id].get_position().y.abs() < 0.01);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeldJointDefinition2D {
	/// 첫 몸.
	pub body_id_a: Option<BodyId2D>,
	/// 둘째 몸.
	pub body_id_b: Option<BodyId2D>,
	/// 첫 몸 기준 닻.
	pub local_anchor_a: PhysicsVector2,
	/// 둘째 몸 기준 닻.
	pub local_anchor_b: PhysicsVector2,
	/// 묶을 때 B 와 A 의 돌림 차이. (라디안)
	pub reference_angle: f64,
	/// 자리 용수철 진동수. (0 이면 단단함)
	pub linear_hertz: f64,
	/// 자리 용수철 감쇠비.
	pub linear_damping_ratio: f64,
	/// 각도 용수철 진동수. (0 이면 단단함)
	pub angular_hertz: f64,
	/// 각도 용수철 감쇠비.
	pub angular_damping_ratio: f64,
	/// 두 몸끼리 부딪힐지. (기본 false)
	pub collide_connected: bool,
	/// 사용자 값.
	pub user_data: u64,
}

impl Default for WeldJointDefinition2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> WeldJointDefinition2D {
		WeldJointDefinition2D::new()
	}
}

impl WeldJointDefinition2D {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new() -> WeldJointDefinition2D {
		WeldJointDefinition2D {
			body_id_a: None,
			body_id_b: None,
			local_anchor_a: PhysicsVector2::zero(),
			local_anchor_b: PhysicsVector2::zero(),
			reference_angle: 0.0,
			linear_hertz: 0.0,
			linear_damping_ratio: 0.0,
			angular_hertz: 0.0,
			angular_damping_ratio: 0.0,
			collide_connected: false,
			user_data: 0,
		}
	}
}

//==============================================================================
/// 바퀴 관절 정의.
/// - B 의 닻이 A 의 축(local_axis_a) 위에만 있고 축을 따라 용수철로 오르내린다. B 는 자유롭게 돌고 모터로 돌릴 수 있다.
///
/// # 예제
/// 수레 바퀴
/// ```rust
/// use vanilla::experimental::physics2d::{PhysicsVector2, WheelJointDefinition2D};
///
/// let mut joint_definition = WheelJointDefinition2D::new();
/// joint_definition.local_axis_a = PhysicsVector2::create(0.0, 1.0);
/// joint_definition.hertz = 4.0;
/// joint_definition.enable_motor = true;
/// joint_definition.motor_speed = 10.0;
/// assert!(joint_definition.enable_spring);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelJointDefinition2D {
	/// 차체 몸.
	pub body_id_a: Option<BodyId2D>,
	/// 바퀴 몸.
	pub body_id_b: Option<BodyId2D>,
	/// 차체 기준 닻.
	pub local_anchor_a: PhysicsVector2,
	/// 바퀴 기준 닻.
	pub local_anchor_b: PhysicsVector2,
	/// 차체 기준 오르내림 축. (단위 길이로 맞춘다)
	pub local_axis_a: PhysicsVector2,
	/// 축 용수철을 쓸지. (기본 true)
	pub enable_spring: bool,
	/// 용수철 진동수. (기본 1)
	pub hertz: f64,
	/// 용수철 감쇠비. (기본 0.7)
	pub damping_ratio: f64,
	/// 모터를 쓸지.
	pub enable_motor: bool,
	/// 모터의 가장 큰 돌림힘.
	pub max_motor_torque: f64,
	/// 모터 각속도. (초당 라디안)
	pub motor_speed: f64,
	/// 축 위 자리 한도를 쓸지.
	pub enable_limit: bool,
	/// 가장 작은 축 위 자리.
	pub lower_translation: f64,
	/// 가장 큰 축 위 자리.
	pub upper_translation: f64,
	/// 두 몸끼리 부딪힐지. (기본 false)
	pub collide_connected: bool,
	/// 사용자 값.
	pub user_data: u64,
}

impl Default for WheelJointDefinition2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> WheelJointDefinition2D {
		WheelJointDefinition2D::new()
	}
}

impl WheelJointDefinition2D {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new() -> WheelJointDefinition2D {
		WheelJointDefinition2D {
			body_id_a: None,
			body_id_b: None,
			local_anchor_a: PhysicsVector2::zero(),
			local_anchor_b: PhysicsVector2::zero(),
			local_axis_a: PhysicsVector2::create(0.0, 1.0),
			enable_spring: true,
			hertz: 1.0,
			damping_ratio: 0.7,
			enable_motor: false,
			max_motor_torque: 0.0,
			motor_speed: 0.0,
			enable_limit: false,
			lower_translation: 0.0,
			upper_translation: 0.0,
			collide_connected: false,
			user_data: 0,
		}
	}
}

//==============================================================================
/// 관절. (두 몸을 잇는 구속 하나)
/// - PhysicsWorld2D::create_distance_joint 같은 함수로 만들고 get_joint(joint_id) 로 얻는다.
/// - 설정을 바꾸면 다음 걸음 앞에서 두 몸을 깨운다.
///
/// # 예제
/// 모터 속도 바꾸기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, PhysicsWorld2D, RevoluteJointDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let ground_id = world.create_body(&BodyDefinition2D::new());
/// let mut body_definition = BodyDefinition2D::new();
/// body_definition.body_type = BodyType2D::Dynamic;
/// let body_id = world.create_body(&body_definition);
/// let mut joint_definition = RevoluteJointDefinition2D::new();
/// joint_definition.body_id_a = Some(ground_id);
/// joint_definition.body_id_b = Some(body_id);
/// let joint_id = world.create_revolute_joint(&joint_definition).expect("관절");
/// let joint = world.get_joint_mut(joint_id).expect("관절");
/// joint.set_motor_enabled(true);
/// joint.set_max_motor_torque(10.0);
/// joint.set_motor_speed(-1.5);
/// assert!(joint.is_motor_enabled());
/// assert_eq!(joint.get_motor_speed(), -1.5);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Joint2D {
	pub(crate) id: JointId2D,
	pub(crate) joint_type: JointType2D,
	pub(crate) body_index_a: usize,
	pub(crate) body_index_b: usize,
	pub(crate) body_id_a: BodyId2D,
	pub(crate) body_id_b: BodyId2D,
	pub(crate) local_anchor_a_x: f64,
	pub(crate) local_anchor_a_y: f64,
	pub(crate) local_anchor_b_x: f64,
	pub(crate) local_anchor_b_y: f64,
	pub(crate) reference_angle: f64,
	pub(crate) collide_connected: bool,
	pub(crate) user_data: u64,
	pub(crate) is_wake_requested: bool,
	pub(crate) length: f64,
	pub(crate) min_length: f64,
	pub(crate) max_length: f64,
	pub(crate) enable_spring: bool,
	pub(crate) hertz: f64,
	pub(crate) damping_ratio: f64,
	pub(crate) enable_motor: bool,
	pub(crate) max_motor_torque: f64,
	pub(crate) motor_speed: f64,
	pub(crate) enable_limit: bool,
	pub(crate) lower_limit: f64,
	pub(crate) upper_limit: f64,
	pub(crate) linear_hertz: f64,
	pub(crate) linear_damping_ratio: f64,
	pub(crate) angular_hertz: f64,
	pub(crate) angular_damping_ratio: f64,
	pub(crate) local_axis_a_x: f64,
	pub(crate) local_axis_a_y: f64,
	pub(crate) linear_impulse_x: f64,
	pub(crate) linear_impulse_y: f64,
	pub(crate) angular_impulse: f64,
	pub(crate) axial_impulse: f64,
	pub(crate) spring_impulse: f64,
	pub(crate) motor_impulse: f64,
	pub(crate) lower_impulse: f64,
	pub(crate) upper_impulse: f64,
	pub(crate) perpendicular_impulse: f64,
	pub(crate) solver_index_a: usize,
	pub(crate) solver_index_b: usize,
	pub(crate) inverse_mass_a: f64,
	pub(crate) inverse_mass_b: f64,
	pub(crate) inverse_inertia_a: f64,
	pub(crate) inverse_inertia_b: f64,
	pub(crate) frame_a_x: f64,
	pub(crate) frame_a_y: f64,
	pub(crate) frame_b_x: f64,
	pub(crate) frame_b_y: f64,
	pub(crate) delta_center_x: f64,
	pub(crate) delta_center_y: f64,
	pub(crate) delta_angle: f64,
	pub(crate) axis_a_x: f64,
	pub(crate) axis_a_y: f64,
	pub(crate) axial_mass: f64,
	pub(crate) perpendicular_mass: f64,
	pub(crate) motor_mass: f64,
	pub(crate) spring_softness: Softness,
	pub(crate) linear_softness: Softness,
	pub(crate) angular_softness: Softness,
	pub(crate) distance_softness: Softness,
}

impl Joint2D {
	//==============================================================================
	/// 빈 관절 자료. (월드가 채운다)
	//==============================================================================
	pub(crate) fn new_empty(id: JointId2D, joint_type: JointType2D, body_id_a: BodyId2D, body_id_b: BodyId2D) -> Joint2D {
		Joint2D {
			id,
			joint_type,
			body_index_a: body_id_a.get_index(),
			body_index_b: body_id_b.get_index(),
			body_id_a,
			body_id_b,
			local_anchor_a_x: 0.0,
			local_anchor_a_y: 0.0,
			local_anchor_b_x: 0.0,
			local_anchor_b_y: 0.0,
			reference_angle: 0.0,
			collide_connected: false,
			user_data: 0,
			is_wake_requested: false,
			length: 1.0,
			min_length: 0.0,
			max_length: HUGE_LENGTH,
			enable_spring: false,
			hertz: 0.0,
			damping_ratio: 0.0,
			enable_motor: false,
			max_motor_torque: 0.0,
			motor_speed: 0.0,
			enable_limit: false,
			lower_limit: 0.0,
			upper_limit: 0.0,
			linear_hertz: 0.0,
			linear_damping_ratio: 0.0,
			angular_hertz: 0.0,
			angular_damping_ratio: 0.0,
			local_axis_a_x: 0.0,
			local_axis_a_y: 1.0,
			linear_impulse_x: 0.0,
			linear_impulse_y: 0.0,
			angular_impulse: 0.0,
			axial_impulse: 0.0,
			spring_impulse: 0.0,
			motor_impulse: 0.0,
			lower_impulse: 0.0,
			upper_impulse: 0.0,
			perpendicular_impulse: 0.0,
			solver_index_a: NO_INDEX,
			solver_index_b: NO_INDEX,
			inverse_mass_a: 0.0,
			inverse_mass_b: 0.0,
			inverse_inertia_a: 0.0,
			inverse_inertia_b: 0.0,
			frame_a_x: 0.0,
			frame_a_y: 0.0,
			frame_b_x: 0.0,
			frame_b_y: 0.0,
			delta_center_x: 0.0,
			delta_center_y: 0.0,
			delta_angle: 0.0,
			axis_a_x: 0.0,
			axis_a_y: 0.0,
			axial_mass: 0.0,
			perpendicular_mass: 0.0,
			motor_mass: 0.0,
			spring_softness: Softness::new(),
			linear_softness: Softness::new(),
			angular_softness: Softness::new(),
			distance_softness: Softness::new(),
		}
	}

	//==============================================================================
	/// 아이디 반환.
	///
	/// # 돌려줌
	/// 이 관절의 아이디.
	//==============================================================================
	pub fn get_id(&self) -> JointId2D {
		self.id
	}

	//==============================================================================
	/// 종류 반환.
	///
	/// # 돌려줌
	/// 종류.
	//==============================================================================
	pub fn get_type(&self) -> JointType2D {
		self.joint_type
	}

	//==============================================================================
	/// 첫 몸 아이디 반환.
	///
	/// # 돌려줌
	/// 첫 몸 아이디.
	//==============================================================================
	pub fn get_body_id_a(&self) -> BodyId2D {
		self.body_id_a
	}

	//==============================================================================
	/// 둘째 몸 아이디 반환.
	///
	/// # 돌려줌
	/// 둘째 몸 아이디.
	//==============================================================================
	pub fn get_body_id_b(&self) -> BodyId2D {
		self.body_id_b
	}

	//==============================================================================
	/// 첫 몸 기준 닻 반환.
	///
	/// # 돌려줌
	/// 첫 몸 기준 닻.
	//==============================================================================
	pub fn get_local_anchor_a(&self) -> PhysicsVector2 {
		PhysicsVector2::create(self.local_anchor_a_x, self.local_anchor_a_y)
	}

	//==============================================================================
	/// 둘째 몸 기준 닻 반환.
	///
	/// # 돌려줌
	/// 둘째 몸 기준 닻.
	//==============================================================================
	pub fn get_local_anchor_b(&self) -> PhysicsVector2 {
		PhysicsVector2::create(self.local_anchor_b_x, self.local_anchor_b_y)
	}

	//==============================================================================
	/// 쉬는 길이 설정. (거리 관절)
	///
	/// # 인자
	/// - `length`: 쉬는 길이.
	//==============================================================================
	pub fn set_length(&mut self, length: f64) {
		self.length = length.max(0.0);
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 쉬는 길이 반환. (거리 관절)
	///
	/// # 돌려줌
	/// 쉬는 길이.
	//==============================================================================
	pub fn get_length(&self) -> f64 {
		self.length
	}

	//==============================================================================
	/// 용수철을 쓸지 설정. (거리, 회전, 바퀴 관절)
	///
	/// # 인자
	/// - `enable_spring`: 용수철을 쓸지.
	//==============================================================================
	pub fn set_spring_enabled(&mut self, enable_spring: bool) {
		self.enable_spring = enable_spring;
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 용수철을 쓰는지.
	///
	/// # 돌려줌
	/// 용수철을 쓰면 true.
	//==============================================================================
	pub fn is_spring_enabled(&self) -> bool {
		self.enable_spring
	}

	//==============================================================================
	/// 용수철 진동수 설정. (헤르츠)
	///
	/// # 인자
	/// - `hertz`: 진동수.
	//==============================================================================
	pub fn set_spring_hertz(&mut self, hertz: f64) {
		self.hertz = hertz.max(0.0);
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 용수철 감쇠비 설정.
	///
	/// # 인자
	/// - `damping_ratio`: 감쇠비.
	//==============================================================================
	pub fn set_spring_damping_ratio(&mut self, damping_ratio: f64) {
		self.damping_ratio = damping_ratio.max(0.0);
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 모터를 쓸지 설정. (회전, 바퀴 관절)
	///
	/// # 인자
	/// - `enable_motor`: 모터를 쓸지.
	//==============================================================================
	pub fn set_motor_enabled(&mut self, enable_motor: bool) {
		self.enable_motor = enable_motor;
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 모터를 쓰는지.
	///
	/// # 돌려줌
	/// 모터를 쓰면 true.
	//==============================================================================
	pub fn is_motor_enabled(&self) -> bool {
		self.enable_motor
	}

	//==============================================================================
	/// 모터 각속도 설정. (초당 라디안)
	///
	/// # 인자
	/// - `motor_speed`: 모터 각속도.
	//==============================================================================
	pub fn set_motor_speed(&mut self, motor_speed: f64) {
		self.motor_speed = motor_speed;
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 모터 각속도 반환.
	///
	/// # 돌려줌
	/// 모터 각속도.
	//==============================================================================
	pub fn get_motor_speed(&self) -> f64 {
		self.motor_speed
	}

	//==============================================================================
	/// 모터의 가장 큰 돌림힘 설정.
	///
	/// # 인자
	/// - `max_motor_torque`: 가장 큰 돌림힘.
	//==============================================================================
	pub fn set_max_motor_torque(&mut self, max_motor_torque: f64) {
		self.max_motor_torque = max_motor_torque.max(0.0);
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 지난 걸음의 모터 돌림힘 반환. (모터 충격 / 걸음 시간)
	///
	/// # 인자
	/// - `inverse_time_step`: 1 / 걸음 시간.
	///
	/// # 돌려줌
	/// 모터 돌림힘.
	//==============================================================================
	pub fn get_motor_torque(&self, inverse_time_step: f64) -> f64 {
		inverse_time_step * self.motor_impulse
	}

	//==============================================================================
	/// 한도를 쓸지 설정. (거리, 회전, 바퀴 관절)
	///
	/// # 인자
	/// - `enable_limit`: 한도를 쓸지.
	//==============================================================================
	pub fn set_limit_enabled(&mut self, enable_limit: bool) {
		self.enable_limit = enable_limit;
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 한도를 쓰는지.
	///
	/// # 돌려줌
	/// 한도를 쓰면 true.
	//==============================================================================
	pub fn is_limit_enabled(&self) -> bool {
		self.enable_limit
	}

	//==============================================================================
	/// 한도 설정. (거리 관절은 길이, 회전 관절은 각도(라디안), 바퀴 관절은 축 위 자리)
	///
	/// # 인자
	/// - `lower`: 아래 한도.
	/// - `upper`: 위 한도. (lower 보다 작으면 둘을 바꾼다)
	//==============================================================================
	pub fn set_limits(&mut self, lower: f64, upper: f64) {
		let minimum = lower.min(upper);
		let maximum = lower.max(upper);
		if self.joint_type == JointType2D::Distance {
			self.min_length = minimum.max(0.0);
			self.max_length = maximum;
		}
		else {
			self.lower_limit = minimum;
			self.upper_limit = maximum;
		}
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 아래 한도 반환.
	///
	/// # 돌려줌
	/// 아래 한도.
	//==============================================================================
	pub fn get_lower_limit(&self) -> f64 {
		if self.joint_type == JointType2D::Distance {
			return self.min_length;
		}
		self.lower_limit
	}

	//==============================================================================
	/// 위 한도 반환.
	///
	/// # 돌려줌
	/// 위 한도.
	//==============================================================================
	pub fn get_upper_limit(&self) -> f64 {
		if self.joint_type == JointType2D::Distance {
			return self.max_length;
		}
		self.upper_limit
	}

	//==============================================================================
	/// 용접의 자리 용수철 설정. (헤르츠가 0 이면 단단함)
	///
	/// # 인자
	/// - `hertz`: 진동수.
	/// - `damping_ratio`: 감쇠비.
	//==============================================================================
	pub fn set_linear_spring(&mut self, hertz: f64, damping_ratio: f64) {
		self.linear_hertz = hertz.max(0.0);
		self.linear_damping_ratio = damping_ratio.max(0.0);
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 용접의 각도 용수철 설정. (헤르츠가 0 이면 단단함)
	///
	/// # 인자
	/// - `hertz`: 진동수.
	/// - `damping_ratio`: 감쇠비.
	//==============================================================================
	pub fn set_angular_spring(&mut self, hertz: f64, damping_ratio: f64) {
		self.angular_hertz = hertz.max(0.0);
		self.angular_damping_ratio = damping_ratio.max(0.0);
		self.is_wake_requested = true;
	}

	//==============================================================================
	/// 사용자 값 설정.
	///
	/// # 인자
	/// - `user_data`: 사용자 값.
	//==============================================================================
	pub fn set_user_data(&mut self, user_data: u64) {
		self.user_data = user_data;
	}

	//==============================================================================
	/// 사용자 값 반환.
	///
	/// # 돌려줌
	/// 사용자 값.
	//==============================================================================
	pub fn get_user_data(&self) -> u64 {
		self.user_data
	}
}

//==============================================================================
/// B 회전과 A 회전 사이 각도. (결정론 아크탄젠트)
//==============================================================================
pub(crate) fn compute_relative_angle(cosine_b: f64, sine_b: f64, cosine_a: f64, sine_a: f64) -> f64 {
	let sine = sine_b * cosine_a - cosine_b * sine_a;
	let cosine = cosine_b * cosine_a + sine_b * sine_a;
	PhysicsMath2D::compute_atan2(sine, cosine)
}

//==============================================================================
/// 2x2 대칭 행렬 풀기. ((x, y))
//==============================================================================
fn solve_symmetric22(a11: f64, a12: f64, a22: f64, bx: f64, by: f64) -> (f64, f64) {
	let mut determinant = a11 * a22 - a12 * a12;
	if determinant != 0.0 {
		determinant = 1.0 / determinant;
	}
	(determinant * (a22 * bx - a12 * by), determinant * (a11 * by - a12 * bx))
}

//==============================================================================
/// 관절 풀이 준비.
//==============================================================================
pub(crate) fn prepare_joint(joint: &mut Joint2D, body_data_a: &Body2D, body_data_b: &Body2D, solver_body_list: &[SolverBody], context: &StepContext) {
	joint.solver_index_a = body_data_a.solver_index;
	joint.solver_index_b = body_data_b.solver_index;
	let solver_body_a = read_solver_body(solver_body_list, joint.solver_index_a);
	let solver_body_b = read_solver_body(solver_body_list, joint.solver_index_b);
	let inverse_mass_a = solver_body_a.inverse_mass;
	let inverse_mass_b = solver_body_b.inverse_mass;
	let inverse_inertia_a = solver_body_a.inverse_inertia;
	let inverse_inertia_b = solver_body_b.inverse_inertia;
	joint.inverse_mass_a = inverse_mass_a;
	joint.inverse_mass_b = inverse_mass_b;
	joint.inverse_inertia_a = inverse_inertia_a;
	joint.inverse_inertia_b = inverse_inertia_b;

	// 닻. (질량 중심 기준, 월드 방향)
	let local_offset_a_x = joint.local_anchor_a_x - body_data_a.local_center_x;
	let local_offset_a_y = joint.local_anchor_a_y - body_data_a.local_center_y;
	let local_offset_b_x = joint.local_anchor_b_x - body_data_b.local_center_x;
	let local_offset_b_y = joint.local_anchor_b_y - body_data_b.local_center_y;
	joint.frame_a_x = body_data_a.rotation_cosine * local_offset_a_x - body_data_a.rotation_sine * local_offset_a_y;
	joint.frame_a_y = body_data_a.rotation_sine * local_offset_a_x + body_data_a.rotation_cosine * local_offset_a_y;
	joint.frame_b_x = body_data_b.rotation_cosine * local_offset_b_x - body_data_b.rotation_sine * local_offset_b_y;
	joint.frame_b_y = body_data_b.rotation_sine * local_offset_b_x + body_data_b.rotation_cosine * local_offset_b_y;
	joint.delta_center_x = body_data_b.center_x - body_data_a.center_x;
	joint.delta_center_y = body_data_b.center_y - body_data_a.center_y;
	let relative_angle = compute_relative_angle(body_data_b.rotation_cosine, body_data_b.rotation_sine, body_data_a.rotation_cosine, body_data_a.rotation_sine);
	joint.delta_angle = PhysicsMath2D::unwind_angle(relative_angle - joint.reference_angle);
	let angular_mass_inverse = inverse_inertia_a + inverse_inertia_b;
	let sub_step_time = context.sub_step_time;

	match joint.joint_type {
		JointType2D::Distance => {
			let separation_x = joint.frame_b_x - joint.frame_a_x + joint.delta_center_x;
			let separation_y = joint.frame_b_y - joint.frame_a_y + joint.delta_center_y;
			let separation_length = (separation_x * separation_x + separation_y * separation_y).sqrt();
			let mut axis_x = 0.0;
			let mut axis_y = 0.0;
			if separation_length > 0.0 {
				axis_x = separation_x / separation_length;
				axis_y = separation_y / separation_length;
			}
			let cross_a = joint.frame_a_x * axis_y - joint.frame_a_y * axis_x;
			let cross_b = joint.frame_b_x * axis_y - joint.frame_b_y * axis_x;
			let mass_inverse = inverse_mass_a + inverse_mass_b + inverse_inertia_a * cross_a * cross_a + inverse_inertia_b * cross_b * cross_b;
			joint.axial_mass = if mass_inverse > 0.0 {
				1.0 / mass_inverse
			}
			else {
				0.0
			};
			joint.distance_softness = make_softness(2.0 * context.contact_hertz, context.joint_damping_ratio, sub_step_time);
			joint.spring_softness = make_softness(joint.hertz, joint.damping_ratio, sub_step_time);
		}
		JointType2D::Revolute => {
			joint.axial_mass = if angular_mass_inverse > 0.0 {
				1.0 / angular_mass_inverse
			}
			else {
				0.0
			};
			joint.spring_softness = make_softness(joint.hertz, joint.damping_ratio, sub_step_time);
		}
		JointType2D::Weld => {
			joint.axial_mass = if angular_mass_inverse > 0.0 {
				1.0 / angular_mass_inverse
			}
			else {
				0.0
			};
			joint.linear_softness = if joint.linear_hertz == 0.0 {
				context.joint_softness
			}
			else {
				make_softness(joint.linear_hertz, joint.linear_damping_ratio, sub_step_time)
			};
			joint.angular_softness = if joint.angular_hertz == 0.0 {
				context.joint_softness
			}
			else {
				make_softness(joint.angular_hertz, joint.angular_damping_ratio, sub_step_time)
			};
		}
		JointType2D::Wheel => {
			joint.axis_a_x = body_data_a.rotation_cosine * joint.local_axis_a_x - body_data_a.rotation_sine * joint.local_axis_a_y;
			joint.axis_a_y = body_data_a.rotation_sine * joint.local_axis_a_x + body_data_a.rotation_cosine * joint.local_axis_a_y;
			let offset_x = joint.delta_center_x + joint.frame_b_x - joint.frame_a_x;
			let offset_y = joint.delta_center_y + joint.frame_b_y - joint.frame_a_y;
			let perpendicular_x = -joint.axis_a_y;
			let perpendicular_y = joint.axis_a_x;
			let perpendicular_cross_a = (offset_x + joint.frame_a_x) * perpendicular_y - (offset_y + joint.frame_a_y) * perpendicular_x;
			let perpendicular_cross_b = joint.frame_b_x * perpendicular_y - joint.frame_b_y * perpendicular_x;
			let perpendicular_mass_inverse = inverse_mass_a + inverse_mass_b + inverse_inertia_a * perpendicular_cross_a * perpendicular_cross_a + inverse_inertia_b * perpendicular_cross_b * perpendicular_cross_b;
			joint.perpendicular_mass = if perpendicular_mass_inverse > 0.0 {
				1.0 / perpendicular_mass_inverse
			}
			else {
				0.0
			};
			let axial_cross_a = (offset_x + joint.frame_a_x) * joint.axis_a_y - (offset_y + joint.frame_a_y) * joint.axis_a_x;
			let axial_cross_b = joint.frame_b_x * joint.axis_a_y - joint.frame_b_y * joint.axis_a_x;
			let axial_mass_inverse = inverse_mass_a + inverse_mass_b + inverse_inertia_a * axial_cross_a * axial_cross_a + inverse_inertia_b * axial_cross_b * axial_cross_b;
			joint.axial_mass = if axial_mass_inverse > 0.0 {
				1.0 / axial_mass_inverse
			}
			else {
				0.0
			};
			joint.motor_mass = if angular_mass_inverse > 0.0 {
				1.0 / angular_mass_inverse
			}
			else {
				0.0
			};
			joint.spring_softness = make_softness(joint.hertz, joint.damping_ratio, sub_step_time);
		}
	}
}

//==============================================================================
/// 관절 웜 스타트.
//==============================================================================
pub(crate) fn warm_start_joint(joint: &Joint2D, solver_body_list: &mut [SolverBody]) {
	let solver_body_a = read_solver_body(solver_body_list, joint.solver_index_a);
	let solver_body_b = read_solver_body(solver_body_list, joint.solver_index_b);
	let inverse_mass_a = joint.inverse_mass_a;
	let inverse_mass_b = joint.inverse_mass_b;
	let inverse_inertia_a = joint.inverse_inertia_a;
	let inverse_inertia_b = joint.inverse_inertia_b;
	let anchor_a_x = solver_body_a.delta_rotation_cosine * joint.frame_a_x - solver_body_a.delta_rotation_sine * joint.frame_a_y;
	let anchor_a_y = solver_body_a.delta_rotation_sine * joint.frame_a_x + solver_body_a.delta_rotation_cosine * joint.frame_a_y;
	let anchor_b_x = solver_body_b.delta_rotation_cosine * joint.frame_b_x - solver_body_b.delta_rotation_sine * joint.frame_b_y;
	let anchor_b_y = solver_body_b.delta_rotation_sine * joint.frame_b_x + solver_body_b.delta_rotation_cosine * joint.frame_b_y;
	let impulse_x;
	let impulse_y;
	let angular_impulse_a;
	let angular_impulse_b;

	match joint.joint_type {
		JointType2D::Distance => {
			let separation_x = solver_body_b.delta_position_x - solver_body_a.delta_position_x + joint.delta_center_x + anchor_b_x - anchor_a_x;
			let separation_y = solver_body_b.delta_position_y - solver_body_a.delta_position_y + joint.delta_center_y + anchor_b_y - anchor_a_y;
			let separation_length = (separation_x * separation_x + separation_y * separation_y).sqrt();
			let mut axis_x = 0.0;
			let mut axis_y = 0.0;
			if separation_length > 0.0 {
				axis_x = separation_x / separation_length;
				axis_y = separation_y / separation_length;
			}
			let axial_impulse = joint.axial_impulse + joint.lower_impulse - joint.upper_impulse;
			impulse_x = axial_impulse * axis_x;
			impulse_y = axial_impulse * axis_y;
			angular_impulse_a = anchor_a_x * impulse_y - anchor_a_y * impulse_x;
			angular_impulse_b = anchor_b_x * impulse_y - anchor_b_y * impulse_x;
		}
		JointType2D::Revolute => {
			let axial_impulse = joint.spring_impulse + joint.motor_impulse + joint.lower_impulse - joint.upper_impulse;
			impulse_x = joint.linear_impulse_x;
			impulse_y = joint.linear_impulse_y;
			angular_impulse_a = anchor_a_x * impulse_y - anchor_a_y * impulse_x + axial_impulse;
			angular_impulse_b = anchor_b_x * impulse_y - anchor_b_y * impulse_x + axial_impulse;
		}
		JointType2D::Weld => {
			impulse_x = joint.linear_impulse_x;
			impulse_y = joint.linear_impulse_y;
			angular_impulse_a = anchor_a_x * impulse_y - anchor_a_y * impulse_x + joint.angular_impulse;
			angular_impulse_b = anchor_b_x * impulse_y - anchor_b_y * impulse_x + joint.angular_impulse;
		}
		JointType2D::Wheel => {
			let offset_x = solver_body_b.delta_position_x - solver_body_a.delta_position_x + joint.delta_center_x + anchor_b_x - anchor_a_x;
			let offset_y = solver_body_b.delta_position_y - solver_body_a.delta_position_y + joint.delta_center_y + anchor_b_y - anchor_a_y;
			let axis_x = solver_body_a.delta_rotation_cosine * joint.axis_a_x - solver_body_a.delta_rotation_sine * joint.axis_a_y;
			let axis_y = solver_body_a.delta_rotation_sine * joint.axis_a_x + solver_body_a.delta_rotation_cosine * joint.axis_a_y;
			let perpendicular_x = -axis_y;
			let perpendicular_y = axis_x;
			let axial_cross_a = (offset_x + anchor_a_x) * axis_y - (offset_y + anchor_a_y) * axis_x;
			let axial_cross_b = anchor_b_x * axis_y - anchor_b_y * axis_x;
			let perpendicular_cross_a = (offset_x + anchor_a_x) * perpendicular_y - (offset_y + anchor_a_y) * perpendicular_x;
			let perpendicular_cross_b = anchor_b_x * perpendicular_y - anchor_b_y * perpendicular_x;
			let axial_impulse = joint.spring_impulse + joint.lower_impulse - joint.upper_impulse;
			impulse_x = axial_impulse * axis_x + joint.perpendicular_impulse * perpendicular_x;
			impulse_y = axial_impulse * axis_y + joint.perpendicular_impulse * perpendicular_y;
			angular_impulse_a = axial_impulse * axial_cross_a + joint.perpendicular_impulse * perpendicular_cross_a + joint.motor_impulse;
			angular_impulse_b = axial_impulse * axial_cross_b + joint.perpendicular_impulse * perpendicular_cross_b + joint.motor_impulse;
		}
	}

	if joint.solver_index_a != NO_INDEX {
		let solver_body = &mut solver_body_list[joint.solver_index_a];
		solver_body.linear_velocity_x -= inverse_mass_a * impulse_x;
		solver_body.linear_velocity_y -= inverse_mass_a * impulse_y;
		solver_body.angular_velocity -= inverse_inertia_a * angular_impulse_a;
	}
	if joint.solver_index_b != NO_INDEX {
		let solver_body = &mut solver_body_list[joint.solver_index_b];
		solver_body.linear_velocity_x += inverse_mass_b * impulse_x;
		solver_body.linear_velocity_y += inverse_mass_b * impulse_y;
		solver_body.angular_velocity += inverse_inertia_b * angular_impulse_b;
	}
}

//==============================================================================
/// 관절 풀기. (use_bias 면 어긋남을 부드럽게 바로잡고, 아니면 속도만 맞춘다)
//==============================================================================
pub(crate) fn solve_joint(joint: &mut Joint2D, solver_body_list: &mut [SolverBody], context: &StepContext, use_bias: bool) {
	let solver_body_a = read_solver_body(solver_body_list, joint.solver_index_a);
	let solver_body_b = read_solver_body(solver_body_list, joint.solver_index_b);
	let inverse_mass_a = joint.inverse_mass_a;
	let inverse_mass_b = joint.inverse_mass_b;
	let inverse_inertia_a = joint.inverse_inertia_a;
	let inverse_inertia_b = joint.inverse_inertia_b;
	let mut velocity_a_x = solver_body_a.linear_velocity_x;
	let mut velocity_a_y = solver_body_a.linear_velocity_y;
	let mut angular_velocity_a = solver_body_a.angular_velocity;
	let mut velocity_b_x = solver_body_b.linear_velocity_x;
	let mut velocity_b_y = solver_body_b.linear_velocity_y;
	let mut angular_velocity_b = solver_body_b.angular_velocity;
	let anchor_a_x = solver_body_a.delta_rotation_cosine * joint.frame_a_x - solver_body_a.delta_rotation_sine * joint.frame_a_y;
	let anchor_a_y = solver_body_a.delta_rotation_sine * joint.frame_a_x + solver_body_a.delta_rotation_cosine * joint.frame_a_y;
	let anchor_b_x = solver_body_b.delta_rotation_cosine * joint.frame_b_x - solver_body_b.delta_rotation_sine * joint.frame_b_y;
	let anchor_b_y = solver_body_b.delta_rotation_sine * joint.frame_b_x + solver_body_b.delta_rotation_cosine * joint.frame_b_y;
	let delta_position_x = solver_body_b.delta_position_x - solver_body_a.delta_position_x;
	let delta_position_y = solver_body_b.delta_position_y - solver_body_a.delta_position_y;
	let is_fixed_rotation = inverse_inertia_a + inverse_inertia_b == 0.0;
	let joint_softness = context.joint_softness;
	let inverse_sub_step_time = context.inverse_sub_step_time;
	let sub_step_time = context.sub_step_time;

	match joint.joint_type {
		JointType2D::Distance => {
			let separation_x = delta_position_x + joint.delta_center_x + anchor_b_x - anchor_a_x;
			let separation_y = delta_position_y + joint.delta_center_y + anchor_b_y - anchor_a_y;
			let length = (separation_x * separation_x + separation_y * separation_y).sqrt();
			let mut axis_x = 0.0;
			let mut axis_y = 0.0;
			if length > 0.0 {
				axis_x = separation_x / length;
				axis_y = separation_y / length;
			}
			let cross_a = anchor_a_x * axis_y - anchor_a_y * axis_x;
			let cross_b = anchor_b_x * axis_y - anchor_b_y * axis_x;
			let axial_mass = joint.axial_mass;
			let is_soft = joint.enable_spring && (joint.min_length < joint.max_length || !joint.enable_limit);
			if is_soft {
				if joint.hertz > 0.0 {
					let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
					let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
					let speed = axis_x * relative_velocity_x + axis_y * relative_velocity_y;
					let spring_softness = joint.spring_softness;
					let bias = spring_softness.bias_rate * (length - joint.length);
					let impulse = -spring_softness.mass_scale * axial_mass * (speed + bias) - spring_softness.impulse_scale * joint.axial_impulse;
					joint.axial_impulse += impulse;
					velocity_a_x -= inverse_mass_a * impulse * axis_x;
					velocity_a_y -= inverse_mass_a * impulse * axis_y;
					angular_velocity_a -= inverse_inertia_a * impulse * cross_a;
					velocity_b_x += inverse_mass_b * impulse * axis_x;
					velocity_b_y += inverse_mass_b * impulse * axis_y;
					angular_velocity_b += inverse_inertia_b * impulse * cross_b;
				}
				if joint.enable_limit {
					for side in 0..2 {
						let sign = if side == 0 {
							1.0
						}
						else {
							-1.0
						};
						let constraint = if side == 0 {
							length - joint.min_length
						}
						else {
							joint.max_length - length
						};
						let mut bias = 0.0;
						let mut mass_scale = 1.0;
						let mut impulse_scale = 0.0;
						if constraint > 0.0 {
							bias = constraint * inverse_sub_step_time;
						}
						else if use_bias {
							bias = joint_softness.bias_rate * constraint;
							mass_scale = joint_softness.mass_scale;
							impulse_scale = joint_softness.impulse_scale;
						}
						let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
						let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
						let speed = sign * (axis_x * relative_velocity_x + axis_y * relative_velocity_y);
						let old_impulse = if side == 0 {
							joint.lower_impulse
						}
						else {
							joint.upper_impulse
						};
						let mut impulse = -mass_scale * axial_mass * (speed + bias) - impulse_scale * old_impulse;
						let new_impulse = (old_impulse + impulse).max(0.0);
						impulse = new_impulse - old_impulse;
						if side == 0 {
							joint.lower_impulse = new_impulse;
						}
						else {
							joint.upper_impulse = new_impulse;
						}
						let applied_impulse = sign * impulse;
						velocity_a_x -= inverse_mass_a * applied_impulse * axis_x;
						velocity_a_y -= inverse_mass_a * applied_impulse * axis_y;
						angular_velocity_a -= inverse_inertia_a * applied_impulse * cross_a;
						velocity_b_x += inverse_mass_b * applied_impulse * axis_x;
						velocity_b_y += inverse_mass_b * applied_impulse * axis_y;
						angular_velocity_b += inverse_inertia_b * applied_impulse * cross_b;
					}
				}
			}
			else {
				let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
				let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
				let speed = axis_x * relative_velocity_x + axis_y * relative_velocity_y;
				let mut bias = 0.0;
				let mut mass_scale = 1.0;
				let mut impulse_scale = 0.0;
				if use_bias {
					let distance_softness = joint.distance_softness;
					bias = distance_softness.bias_rate * (length - joint.length);
					mass_scale = distance_softness.mass_scale;
					impulse_scale = distance_softness.impulse_scale;
				}
				let impulse = -mass_scale * axial_mass * (speed + bias) - impulse_scale * joint.axial_impulse;
				joint.axial_impulse += impulse;
				velocity_a_x -= inverse_mass_a * impulse * axis_x;
				velocity_a_y -= inverse_mass_a * impulse * axis_y;
				angular_velocity_a -= inverse_inertia_a * impulse * cross_a;
				velocity_b_x += inverse_mass_b * impulse * axis_x;
				velocity_b_y += inverse_mass_b * impulse * axis_y;
				angular_velocity_b += inverse_inertia_b * impulse * cross_b;
			}
		}
		JointType2D::Revolute | JointType2D::Weld => {
			let relative_angle = compute_relative_angle(solver_body_b.delta_rotation_cosine, solver_body_b.delta_rotation_sine, solver_body_a.delta_rotation_cosine, solver_body_a.delta_rotation_sine);
			if joint.joint_type == JointType2D::Revolute {
				// 각도 용수철.
				if joint.enable_spring && !is_fixed_rotation {
					let spring_softness = joint.spring_softness;
					let angle = relative_angle + joint.delta_angle;
					let bias = spring_softness.bias_rate * angle;
					let angular_speed = angular_velocity_b - angular_velocity_a;
					let impulse = -spring_softness.mass_scale * joint.axial_mass * (angular_speed + bias) - spring_softness.impulse_scale * joint.spring_impulse;
					joint.spring_impulse += impulse;
					angular_velocity_a -= inverse_inertia_a * impulse;
					angular_velocity_b += inverse_inertia_b * impulse;
				}
				// 모터.
				if joint.enable_motor && !is_fixed_rotation {
					let angular_speed = angular_velocity_b - angular_velocity_a - joint.motor_speed;
					let mut impulse = -joint.axial_mass * angular_speed;
					let old_impulse = joint.motor_impulse;
					let maximum_impulse = sub_step_time * joint.max_motor_torque;
					joint.motor_impulse = (-maximum_impulse).max((old_impulse + impulse).min(maximum_impulse));
					impulse = joint.motor_impulse - old_impulse;
					angular_velocity_a -= inverse_inertia_a * impulse;
					angular_velocity_b += inverse_inertia_b * impulse;
				}
				// 각도 한도.
				if joint.enable_limit && !is_fixed_rotation {
					let joint_angle = PhysicsMath2D::unwind_angle(joint.delta_angle + relative_angle);
					for side in 0..2 {
						let sign = if side == 0 {
							1.0
						}
						else {
							-1.0
						};
						let constraint = if side == 0 {
							joint_angle - joint.lower_limit
						}
						else {
							joint.upper_limit - joint_angle
						};
						let mut bias = 0.0;
						let mut mass_scale = 1.0;
						let mut impulse_scale = 0.0;
						if constraint > 0.0 {
							bias = constraint * inverse_sub_step_time;
						}
						else if use_bias {
							bias = joint_softness.bias_rate * constraint;
							mass_scale = joint_softness.mass_scale;
							impulse_scale = joint_softness.impulse_scale;
						}
						let angular_speed = sign * (angular_velocity_b - angular_velocity_a);
						let old_impulse = if side == 0 {
							joint.lower_impulse
						}
						else {
							joint.upper_impulse
						};
						let mut impulse = -joint.axial_mass * mass_scale * (angular_speed + bias) - impulse_scale * old_impulse;
						let new_impulse = (old_impulse + impulse).max(0.0);
						impulse = new_impulse - old_impulse;
						if side == 0 {
							joint.lower_impulse = new_impulse;
						}
						else {
							joint.upper_impulse = new_impulse;
						}
						angular_velocity_a -= inverse_inertia_a * sign * impulse;
						angular_velocity_b += inverse_inertia_b * sign * impulse;
					}
				}
			}
			else {
				// 용접 각도.
				let mut bias = 0.0;
				let mut mass_scale = 1.0;
				let mut impulse_scale = 0.0;
				if use_bias || joint.angular_hertz > 0.0 {
					let angular_softness = joint.angular_softness;
					bias = angular_softness.bias_rate * (relative_angle + joint.delta_angle);
					mass_scale = angular_softness.mass_scale;
					impulse_scale = angular_softness.impulse_scale;
				}
				let angular_speed = angular_velocity_b - angular_velocity_a;
				let impulse = -joint.axial_mass * mass_scale * (angular_speed + bias) - impulse_scale * joint.angular_impulse;
				joint.angular_impulse += impulse;
				angular_velocity_a -= inverse_inertia_a * impulse;
				angular_velocity_b += inverse_inertia_b * impulse;
			}

			// 점 맞추기. (두 닻이 한 점)
			let mut bias_x = 0.0;
			let mut bias_y = 0.0;
			let mut mass_scale = 1.0;
			let mut impulse_scale = 0.0;
			let point_softness = if joint.joint_type == JointType2D::Weld {
				joint.linear_softness
			}
			else {
				joint_softness
			};
			let is_point_biased = use_bias || (joint.joint_type == JointType2D::Weld && joint.linear_hertz > 0.0);
			if is_point_biased {
				let separation_x = delta_position_x + anchor_b_x - anchor_a_x + joint.delta_center_x;
				let separation_y = delta_position_y + anchor_b_y - anchor_a_y + joint.delta_center_y;
				bias_x = point_softness.bias_rate * separation_x;
				bias_y = point_softness.bias_rate * separation_y;
				mass_scale = point_softness.mass_scale;
				impulse_scale = point_softness.impulse_scale;
			}
			let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
			let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
			let k11 = inverse_mass_a + inverse_mass_b + anchor_a_y * anchor_a_y * inverse_inertia_a + anchor_b_y * anchor_b_y * inverse_inertia_b;
			let k12 = -anchor_a_y * anchor_a_x * inverse_inertia_a - anchor_b_y * anchor_b_x * inverse_inertia_b;
			let k22 = inverse_mass_a + inverse_mass_b + anchor_a_x * anchor_a_x * inverse_inertia_a + anchor_b_x * anchor_b_x * inverse_inertia_b;
			let (solution_x, solution_y) = solve_symmetric22(k11, k12, k22, relative_velocity_x + bias_x, relative_velocity_y + bias_y);
			let impulse_x = -mass_scale * solution_x - impulse_scale * joint.linear_impulse_x;
			let impulse_y = -mass_scale * solution_y - impulse_scale * joint.linear_impulse_y;
			joint.linear_impulse_x += impulse_x;
			joint.linear_impulse_y += impulse_y;
			velocity_a_x -= inverse_mass_a * impulse_x;
			velocity_a_y -= inverse_mass_a * impulse_y;
			angular_velocity_a -= inverse_inertia_a * (anchor_a_x * impulse_y - anchor_a_y * impulse_x);
			velocity_b_x += inverse_mass_b * impulse_x;
			velocity_b_y += inverse_mass_b * impulse_y;
			angular_velocity_b += inverse_inertia_b * (anchor_b_x * impulse_y - anchor_b_y * impulse_x);
		}
		JointType2D::Wheel => {
			let offset_x = delta_position_x + joint.delta_center_x + anchor_b_x - anchor_a_x;
			let offset_y = delta_position_y + joint.delta_center_y + anchor_b_y - anchor_a_y;
			let axis_x = solver_body_a.delta_rotation_cosine * joint.axis_a_x - solver_body_a.delta_rotation_sine * joint.axis_a_y;
			let axis_y = solver_body_a.delta_rotation_sine * joint.axis_a_x + solver_body_a.delta_rotation_cosine * joint.axis_a_y;
			let translation = axis_x * offset_x + axis_y * offset_y;
			let axial_cross_a = (offset_x + anchor_a_x) * axis_y - (offset_y + anchor_a_y) * axis_x;
			let axial_cross_b = anchor_b_x * axis_y - anchor_b_y * axis_x;

			// 모터.
			if joint.enable_motor && !is_fixed_rotation {
				let angular_speed = angular_velocity_b - angular_velocity_a - joint.motor_speed;
				let mut impulse = -joint.motor_mass * angular_speed;
				let old_impulse = joint.motor_impulse;
				let maximum_impulse = sub_step_time * joint.max_motor_torque;
				joint.motor_impulse = (-maximum_impulse).max((old_impulse + impulse).min(maximum_impulse));
				impulse = joint.motor_impulse - old_impulse;
				angular_velocity_a -= inverse_inertia_a * impulse;
				angular_velocity_b += inverse_inertia_b * impulse;
			}
			// 축 용수철. (진짜 용수철이라 이완에도 준다)
			if joint.enable_spring {
				let spring_softness = joint.spring_softness;
				let bias = spring_softness.bias_rate * translation;
				let speed = axis_x * (velocity_b_x - velocity_a_x) + axis_y * (velocity_b_y - velocity_a_y) + axial_cross_b * angular_velocity_b - axial_cross_a * angular_velocity_a;
				let impulse = -spring_softness.mass_scale * joint.axial_mass * (speed + bias) - spring_softness.impulse_scale * joint.spring_impulse;
				joint.spring_impulse += impulse;
				velocity_a_x -= inverse_mass_a * impulse * axis_x;
				velocity_a_y -= inverse_mass_a * impulse * axis_y;
				angular_velocity_a -= inverse_inertia_a * impulse * axial_cross_a;
				velocity_b_x += inverse_mass_b * impulse * axis_x;
				velocity_b_y += inverse_mass_b * impulse * axis_y;
				angular_velocity_b += inverse_inertia_b * impulse * axial_cross_b;
			}
			// 축 위 자리 한도.
			if joint.enable_limit {
				for side in 0..2 {
					let sign = if side == 0 {
						1.0
					}
					else {
						-1.0
					};
					let constraint = if side == 0 {
						translation - joint.lower_limit
					}
					else {
						joint.upper_limit - translation
					};
					let mut bias = 0.0;
					let mut mass_scale = 1.0;
					let mut impulse_scale = 0.0;
					if constraint > 0.0 {
						bias = constraint * inverse_sub_step_time;
					}
					else if use_bias {
						bias = joint_softness.bias_rate * constraint;
						mass_scale = joint_softness.mass_scale;
						impulse_scale = joint_softness.impulse_scale;
					}
					let speed = sign * (axis_x * (velocity_b_x - velocity_a_x) + axis_y * (velocity_b_y - velocity_a_y) + axial_cross_b * angular_velocity_b - axial_cross_a * angular_velocity_a);
					let old_impulse = if side == 0 {
						joint.lower_impulse
					}
					else {
						joint.upper_impulse
					};
					let mut impulse = -joint.axial_mass * mass_scale * (speed + bias) - impulse_scale * old_impulse;
					let new_impulse = (old_impulse + impulse).max(0.0);
					impulse = new_impulse - old_impulse;
					if side == 0 {
						joint.lower_impulse = new_impulse;
					}
					else {
						joint.upper_impulse = new_impulse;
					}
					let applied_impulse = sign * impulse;
					velocity_a_x -= inverse_mass_a * applied_impulse * axis_x;
					velocity_a_y -= inverse_mass_a * applied_impulse * axis_y;
					angular_velocity_a -= inverse_inertia_a * applied_impulse * axial_cross_a;
					velocity_b_x += inverse_mass_b * applied_impulse * axis_x;
					velocity_b_y += inverse_mass_b * applied_impulse * axis_y;
					angular_velocity_b += inverse_inertia_b * applied_impulse * axial_cross_b;
				}
			}
			// 축에서 벗어나지 않게.
			let perpendicular_x = -axis_y;
			let perpendicular_y = axis_x;
			let mut bias = 0.0;
			let mut mass_scale = 1.0;
			let mut impulse_scale = 0.0;
			if use_bias {
				bias = joint_softness.bias_rate * (perpendicular_x * offset_x + perpendicular_y * offset_y);
				mass_scale = joint_softness.mass_scale;
				impulse_scale = joint_softness.impulse_scale;
			}
			let perpendicular_cross_a = (offset_x + anchor_a_x) * perpendicular_y - (offset_y + anchor_a_y) * perpendicular_x;
			let perpendicular_cross_b = anchor_b_x * perpendicular_y - anchor_b_y * perpendicular_x;
			let speed = perpendicular_x * (velocity_b_x - velocity_a_x) + perpendicular_y * (velocity_b_y - velocity_a_y) + perpendicular_cross_b * angular_velocity_b - perpendicular_cross_a * angular_velocity_a;
			let impulse = -mass_scale * joint.perpendicular_mass * (speed + bias) - impulse_scale * joint.perpendicular_impulse;
			joint.perpendicular_impulse += impulse;
			velocity_a_x -= inverse_mass_a * impulse * perpendicular_x;
			velocity_a_y -= inverse_mass_a * impulse * perpendicular_y;
			angular_velocity_a -= inverse_inertia_a * impulse * perpendicular_cross_a;
			velocity_b_x += inverse_mass_b * impulse * perpendicular_x;
			velocity_b_y += inverse_mass_b * impulse * perpendicular_y;
			angular_velocity_b += inverse_inertia_b * impulse * perpendicular_cross_b;
		}
	}

	write_solver_velocity(solver_body_list, joint.solver_index_a, velocity_a_x, velocity_a_y, angular_velocity_a);
	write_solver_velocity(solver_body_list, joint.solver_index_b, velocity_b_x, velocity_b_y, angular_velocity_b);
}
