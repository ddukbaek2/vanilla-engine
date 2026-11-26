//==============================================================================
//! 2D 물리의 풀이기. (안에서 쓴다, 공개 API 아님)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/solver2d.js 와 같은 셈, 같은 차례다.
//! - Box2D 3 의 부드러운 걸음(Soft Step)과 같은 꼴이다. 한 걸음을 하위 걸음 여럿으로 나누고, 하위 걸음마다
//!   속도 적분, 웜 스타트, 부드러운 구속으로 풀기(겹침을 밀어내는 치우침 포함), 자리 적분, 치우침 없이 이완하기를 한다.
//!   하위 걸음이 끝나면 반발을 한 번 주고 충격을 다음 걸음의 웜 스타트로 남긴다. (box2d.org/posts/2024/02/solver2d)
//! - 부드러운 구속은 고유 진동수(헤르츠)와 감쇠비로 정한다. (Erin Catto, GDC 2011 Soft Constraints)
//==============================================================================
// 옮긴 원문: Box2D 3 의 셈에서 옮김, MIT, THIRD_PARTY_LICENSES.md
use super::body2d::{Body2D, NO_INDEX};
use super::narrowphase2d::Manifold;

//==============================================================================
/// 부드러움. (치우침 비율, 질량 배율, 충격 배율)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub(crate) struct Softness {
	pub(crate) bias_rate: f64,
	pub(crate) mass_scale: f64,
	pub(crate) impulse_scale: f64,
}

impl Softness {
	//==============================================================================
	/// 생성. (단단함)
	//==============================================================================
	pub(crate) const fn new() -> Softness {
		Softness {
			bias_rate: 0.0,
			mass_scale: 1.0,
			impulse_scale: 0.0,
		}
	}
}

//==============================================================================
/// 헤르츠와 감쇠비로 부드러움 짓기. (헤르츠가 0 이면 단단함)
//==============================================================================
pub(crate) fn make_softness(hertz: f64, damping_ratio: f64, sub_step_time: f64) -> Softness {
	let mut softness = Softness::new();
	if hertz == 0.0 {
		return softness;
	}
	let omega = 2.0 * std::f64::consts::PI * hertz;
	let factor1 = 2.0 * damping_ratio + sub_step_time * omega;
	let factor2 = sub_step_time * omega * factor1;
	let factor3 = 1.0 / (1.0 + factor2);
	softness.bias_rate = omega / factor1;
	softness.mass_scale = factor2 * factor3;
	softness.impulse_scale = factor3;
	softness
}

//==============================================================================
/// 걸음 값. (한 걸음 동안 같은 값)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub(crate) struct StepContext {
	pub(crate) time_step: f64,
	pub(crate) inverse_time_step: f64,
	pub(crate) sub_step_time: f64,
	pub(crate) inverse_sub_step_time: f64,
	pub(crate) contact_softness: Softness,
	pub(crate) static_softness: Softness,
	pub(crate) joint_softness: Softness,
	pub(crate) restitution_threshold: f64,
	pub(crate) maximum_bias_velocity: f64,
	pub(crate) maximum_linear_speed: f64,
	pub(crate) maximum_angular_speed: f64,
	pub(crate) gravity_x: f64,
	pub(crate) gravity_y: f64,
	pub(crate) contact_hertz: f64,
	pub(crate) joint_damping_ratio: f64,
}

//==============================================================================
/// 풀이 몸. (깨어 있는 정적 아닌 몸 하나의 걸음 동안 값)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub(crate) struct SolverBody {
	pub(crate) body_index: usize,
	pub(crate) linear_velocity_x: f64,
	pub(crate) linear_velocity_y: f64,
	pub(crate) angular_velocity: f64,
	pub(crate) delta_position_x: f64,
	pub(crate) delta_position_y: f64,
	pub(crate) delta_rotation_cosine: f64,
	pub(crate) delta_rotation_sine: f64,
	pub(crate) inverse_mass: f64,
	pub(crate) inverse_inertia: f64,
}

/// 정적 몸 자리의 풀이 몸. (읽기만 한다)
pub(crate) const STATIC_SOLVER_BODY: SolverBody = SolverBody {
	body_index: NO_INDEX,
	linear_velocity_x: 0.0,
	linear_velocity_y: 0.0,
	angular_velocity: 0.0,
	delta_position_x: 0.0,
	delta_position_y: 0.0,
	delta_rotation_cosine: 1.0,
	delta_rotation_sine: 0.0,
	inverse_mass: 0.0,
	inverse_inertia: 0.0,
};

//==============================================================================
/// 풀이 몸 읽기. (번호가 없으면 정적 자리)
//==============================================================================
pub(crate) fn read_solver_body(solver_body_list: &[SolverBody], solver_index: usize) -> SolverBody {
	if solver_index != NO_INDEX {
		return solver_body_list[solver_index];
	}
	STATIC_SOLVER_BODY
}

//==============================================================================
/// 풀이 몸에 속도 쓰기. (번호가 없으면 쓰지 않는다)
//==============================================================================
pub(crate) fn write_solver_velocity(solver_body_list: &mut [SolverBody], solver_index: usize, linear_velocity_x: f64, linear_velocity_y: f64, angular_velocity: f64) {
	if solver_index == NO_INDEX {
		return;
	}
	let solver_body = &mut solver_body_list[solver_index];
	solver_body.linear_velocity_x = linear_velocity_x;
	solver_body.linear_velocity_y = linear_velocity_y;
	solver_body.angular_velocity = angular_velocity;
}

//==============================================================================
/// 닿음. (넓은 단계가 찾은 모양 짝 하나)
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) struct Contact {
	pub(crate) shape_index_a: usize,
	pub(crate) shape_index_b: usize,
	pub(crate) body_index_a: usize,
	pub(crate) body_index_b: usize,
	pub(crate) key: u64,
	pub(crate) is_sensor: bool,
	pub(crate) is_touching: bool,
	pub(crate) is_alive: bool,
	pub(crate) manifold: Manifold,
	pub(crate) friction: f64,
	pub(crate) restitution: f64,
	pub(crate) softness: Softness,
	pub(crate) solver_index_a: usize,
	pub(crate) solver_index_b: usize,
	pub(crate) inverse_mass_a: f64,
	pub(crate) inverse_mass_b: f64,
	pub(crate) inverse_inertia_a: f64,
	pub(crate) inverse_inertia_b: f64,
}

impl Contact {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub(crate) fn new() -> Contact {
		Contact {
			shape_index_a: NO_INDEX,
			shape_index_b: NO_INDEX,
			body_index_a: NO_INDEX,
			body_index_b: NO_INDEX,
			key: 0,
			is_sensor: false,
			is_touching: false,
			is_alive: false,
			manifold: Manifold::default(),
			friction: 0.0,
			restitution: 0.0,
			softness: Softness::new(),
			solver_index_a: NO_INDEX,
			solver_index_b: NO_INDEX,
			inverse_mass_a: 0.0,
			inverse_mass_b: 0.0,
			inverse_inertia_a: 0.0,
			inverse_inertia_b: 0.0,
		}
	}
}

//==============================================================================
/// 닿음 풀이 준비. (닿음점의 팔, 유효 질량, 반발용 처음 속도)
//==============================================================================
pub(crate) fn prepare_contact(contact: &mut Contact, body_data_a: &Body2D, body_data_b: &Body2D, solver_body_list: &[SolverBody], context: &StepContext) {
	contact.solver_index_a = body_data_a.solver_index;
	contact.solver_index_b = body_data_b.solver_index;
	let solver_body_a = read_solver_body(solver_body_list, contact.solver_index_a);
	let solver_body_b = read_solver_body(solver_body_list, contact.solver_index_b);
	let inverse_mass_a = solver_body_a.inverse_mass;
	let inverse_mass_b = solver_body_b.inverse_mass;
	let inverse_inertia_a = solver_body_a.inverse_inertia;
	let inverse_inertia_b = solver_body_b.inverse_inertia;
	contact.inverse_mass_a = inverse_mass_a;
	contact.inverse_mass_b = inverse_mass_b;
	contact.inverse_inertia_a = inverse_inertia_a;
	contact.inverse_inertia_b = inverse_inertia_b;
	contact.softness = if inverse_mass_a == 0.0 || inverse_mass_b == 0.0 {
		context.static_softness
	}
	else {
		context.contact_softness
	};

	let manifold = &mut contact.manifold;
	let normal_x = manifold.normal_x;
	let normal_y = manifold.normal_y;
	let tangent_x = normal_y;
	let tangent_y = -normal_x;
	let velocity_a_x = solver_body_a.linear_velocity_x;
	let velocity_a_y = solver_body_a.linear_velocity_y;
	let angular_velocity_a = solver_body_a.angular_velocity;
	let velocity_b_x = solver_body_b.linear_velocity_x;
	let velocity_b_y = solver_body_b.linear_velocity_y;
	let angular_velocity_b = solver_body_b.angular_velocity;
	for point_index in 0..manifold.point_count {
		let manifold_point = &mut manifold.points[point_index];
		let anchor_a_x = manifold_point.point_x - body_data_a.center_x;
		let anchor_a_y = manifold_point.point_y - body_data_a.center_y;
		let anchor_b_x = manifold_point.point_x - body_data_b.center_x;
		let anchor_b_y = manifold_point.point_y - body_data_b.center_y;
		manifold_point.anchor_a_x = anchor_a_x;
		manifold_point.anchor_a_y = anchor_a_y;
		manifold_point.anchor_b_x = anchor_b_x;
		manifold_point.anchor_b_y = anchor_b_y;
		manifold_point.base_separation = manifold_point.separation - ((anchor_b_x - anchor_a_x) * normal_x + (anchor_b_y - anchor_a_y) * normal_y);
		let cross_normal_a = anchor_a_x * normal_y - anchor_a_y * normal_x;
		let cross_normal_b = anchor_b_x * normal_y - anchor_b_y * normal_x;
		let normal_mass_inverse = inverse_mass_a + inverse_mass_b + inverse_inertia_a * cross_normal_a * cross_normal_a + inverse_inertia_b * cross_normal_b * cross_normal_b;
		manifold_point.normal_mass = if normal_mass_inverse > 0.0 {
			1.0 / normal_mass_inverse
		}
		else {
			0.0
		};
		let cross_tangent_a = anchor_a_x * tangent_y - anchor_a_y * tangent_x;
		let cross_tangent_b = anchor_b_x * tangent_y - anchor_b_y * tangent_x;
		let tangent_mass_inverse = inverse_mass_a + inverse_mass_b + inverse_inertia_a * cross_tangent_a * cross_tangent_a + inverse_inertia_b * cross_tangent_b * cross_tangent_b;
		manifold_point.tangent_mass = if tangent_mass_inverse > 0.0 {
			1.0 / tangent_mass_inverse
		}
		else {
			0.0
		};
		let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
		let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
		manifold_point.relative_velocity = normal_x * relative_velocity_x + normal_y * relative_velocity_y;
		manifold_point.maximum_normal_impulse = 0.0;
	}
}

//==============================================================================
/// 닿음 웜 스타트. (앞 걸음의 충격을 먼저 준다)
//==============================================================================
pub(crate) fn warm_start_contact(contact: &Contact, solver_body_list: &mut [SolverBody]) {
	let solver_body_a = read_solver_body(solver_body_list, contact.solver_index_a);
	let solver_body_b = read_solver_body(solver_body_list, contact.solver_index_b);
	let inverse_mass_a = contact.inverse_mass_a;
	let inverse_mass_b = contact.inverse_mass_b;
	let inverse_inertia_a = contact.inverse_inertia_a;
	let inverse_inertia_b = contact.inverse_inertia_b;
	let mut velocity_a_x = solver_body_a.linear_velocity_x;
	let mut velocity_a_y = solver_body_a.linear_velocity_y;
	let mut angular_velocity_a = solver_body_a.angular_velocity;
	let mut velocity_b_x = solver_body_b.linear_velocity_x;
	let mut velocity_b_y = solver_body_b.linear_velocity_y;
	let mut angular_velocity_b = solver_body_b.angular_velocity;
	let manifold = &contact.manifold;
	let normal_x = manifold.normal_x;
	let normal_y = manifold.normal_y;
	let tangent_x = normal_y;
	let tangent_y = -normal_x;
	for point_index in 0..manifold.point_count {
		let manifold_point = &manifold.points[point_index];
		let impulse_x = manifold_point.normal_impulse * normal_x + manifold_point.tangent_impulse * tangent_x;
		let impulse_y = manifold_point.normal_impulse * normal_y + manifold_point.tangent_impulse * tangent_y;
		angular_velocity_a -= inverse_inertia_a * (manifold_point.anchor_a_x * impulse_y - manifold_point.anchor_a_y * impulse_x);
		velocity_a_x -= inverse_mass_a * impulse_x;
		velocity_a_y -= inverse_mass_a * impulse_y;
		angular_velocity_b += inverse_inertia_b * (manifold_point.anchor_b_x * impulse_y - manifold_point.anchor_b_y * impulse_x);
		velocity_b_x += inverse_mass_b * impulse_x;
		velocity_b_y += inverse_mass_b * impulse_y;
	}
	write_solver_velocity(solver_body_list, contact.solver_index_a, velocity_a_x, velocity_a_y, angular_velocity_a);
	write_solver_velocity(solver_body_list, contact.solver_index_b, velocity_b_x, velocity_b_y, angular_velocity_b);
}

//==============================================================================
/// 닿음 풀기. (use_bias 면 겹침을 부드럽게 밀어내고, 아니면 속도만 맞춘다(이완))
//==============================================================================
pub(crate) fn solve_contact(contact: &mut Contact, solver_body_list: &mut [SolverBody], context: &StepContext, use_bias: bool) {
	let solver_body_a = read_solver_body(solver_body_list, contact.solver_index_a);
	let solver_body_b = read_solver_body(solver_body_list, contact.solver_index_b);
	let inverse_mass_a = contact.inverse_mass_a;
	let inverse_mass_b = contact.inverse_mass_b;
	let inverse_inertia_a = contact.inverse_inertia_a;
	let inverse_inertia_b = contact.inverse_inertia_b;
	let mut velocity_a_x = solver_body_a.linear_velocity_x;
	let mut velocity_a_y = solver_body_a.linear_velocity_y;
	let mut angular_velocity_a = solver_body_a.angular_velocity;
	let mut velocity_b_x = solver_body_b.linear_velocity_x;
	let mut velocity_b_y = solver_body_b.linear_velocity_y;
	let mut angular_velocity_b = solver_body_b.angular_velocity;
	let delta_position_x = solver_body_b.delta_position_x - solver_body_a.delta_position_x;
	let delta_position_y = solver_body_b.delta_position_y - solver_body_a.delta_position_y;
	let rotation_cosine_a = solver_body_a.delta_rotation_cosine;
	let rotation_sine_a = solver_body_a.delta_rotation_sine;
	let rotation_cosine_b = solver_body_b.delta_rotation_cosine;
	let rotation_sine_b = solver_body_b.delta_rotation_sine;
	let softness = contact.softness;
	let friction = contact.friction;
	let manifold = &mut contact.manifold;
	let normal_x = manifold.normal_x;
	let normal_y = manifold.normal_y;
	let tangent_x = normal_y;
	let tangent_y = -normal_x;
	let inverse_sub_step_time = context.inverse_sub_step_time;
	let maximum_bias_velocity = context.maximum_bias_velocity;

	// 밀어내기.
	for point_index in 0..manifold.point_count {
		let manifold_point = &mut manifold.points[point_index];
		let anchor_a_x = manifold_point.anchor_a_x;
		let anchor_a_y = manifold_point.anchor_a_y;
		let anchor_b_x = manifold_point.anchor_b_x;
		let anchor_b_y = manifold_point.anchor_b_y;
		// 지금 떨어진 거리. (걸음 처음부터 몸이 옮기고 돈 만큼)
		let rotated_a_x = rotation_cosine_a * anchor_a_x - rotation_sine_a * anchor_a_y;
		let rotated_a_y = rotation_sine_a * anchor_a_x + rotation_cosine_a * anchor_a_y;
		let rotated_b_x = rotation_cosine_b * anchor_b_x - rotation_sine_b * anchor_b_y;
		let rotated_b_y = rotation_sine_b * anchor_b_x + rotation_cosine_b * anchor_b_y;
		let current_separation = (delta_position_x + rotated_b_x - rotated_a_x) * normal_x + (delta_position_y + rotated_b_y - rotated_a_y) * normal_y + manifold_point.base_separation;
		let mut velocity_bias = 0.0;
		let mut mass_scale = 1.0;
		let mut impulse_scale = 0.0;
		if current_separation > 0.0 {
			// 앞질러 보기. (닿기 전이면 닿을 만큼만 다가가게 한다)
			velocity_bias = current_separation * inverse_sub_step_time;
		}
		else if use_bias {
			velocity_bias = (softness.bias_rate * current_separation).max(-maximum_bias_velocity);
			mass_scale = softness.mass_scale;
			impulse_scale = softness.impulse_scale;
		}
		let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
		let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
		let normal_velocity = normal_x * relative_velocity_x + normal_y * relative_velocity_y;
		let mut impulse = -manifold_point.normal_mass * mass_scale * (normal_velocity + velocity_bias) - impulse_scale * manifold_point.normal_impulse;
		let new_impulse = (manifold_point.normal_impulse + impulse).max(0.0);
		impulse = new_impulse - manifold_point.normal_impulse;
		manifold_point.normal_impulse = new_impulse;
		manifold_point.maximum_normal_impulse = manifold_point.maximum_normal_impulse.max(impulse);
		let impulse_x = impulse * normal_x;
		let impulse_y = impulse * normal_y;
		velocity_a_x -= inverse_mass_a * impulse_x;
		velocity_a_y -= inverse_mass_a * impulse_y;
		angular_velocity_a -= inverse_inertia_a * (anchor_a_x * impulse_y - anchor_a_y * impulse_x);
		velocity_b_x += inverse_mass_b * impulse_x;
		velocity_b_y += inverse_mass_b * impulse_y;
		angular_velocity_b += inverse_inertia_b * (anchor_b_x * impulse_y - anchor_b_y * impulse_x);
	}

	// 마찰.
	for point_index in 0..manifold.point_count {
		let manifold_point = &mut manifold.points[point_index];
		let anchor_a_x = manifold_point.anchor_a_x;
		let anchor_a_y = manifold_point.anchor_a_y;
		let anchor_b_x = manifold_point.anchor_b_x;
		let anchor_b_y = manifold_point.anchor_b_y;
		let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
		let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
		let tangent_velocity = tangent_x * relative_velocity_x + tangent_y * relative_velocity_y;
		let mut impulse = -manifold_point.tangent_mass * tangent_velocity;
		let maximum_friction = friction * manifold_point.normal_impulse;
		let new_impulse = (-maximum_friction).max((manifold_point.tangent_impulse + impulse).min(maximum_friction));
		impulse = new_impulse - manifold_point.tangent_impulse;
		manifold_point.tangent_impulse = new_impulse;
		let impulse_x = impulse * tangent_x;
		let impulse_y = impulse * tangent_y;
		velocity_a_x -= inverse_mass_a * impulse_x;
		velocity_a_y -= inverse_mass_a * impulse_y;
		angular_velocity_a -= inverse_inertia_a * (anchor_a_x * impulse_y - anchor_a_y * impulse_x);
		velocity_b_x += inverse_mass_b * impulse_x;
		velocity_b_y += inverse_mass_b * impulse_y;
		angular_velocity_b += inverse_inertia_b * (anchor_b_x * impulse_y - anchor_b_y * impulse_x);
	}

	write_solver_velocity(solver_body_list, contact.solver_index_a, velocity_a_x, velocity_a_y, angular_velocity_a);
	write_solver_velocity(solver_body_list, contact.solver_index_b, velocity_b_x, velocity_b_y, angular_velocity_b);
}

//==============================================================================
/// 반발. (하위 걸음이 끝난 뒤 한 번, 처음 다가오던 속력이 문턱보다 크고 실제로 밀어낸 점만)
//==============================================================================
pub(crate) fn apply_contact_restitution(contact: &mut Contact, solver_body_list: &mut [SolverBody], context: &StepContext) {
	let restitution = contact.restitution;
	if restitution == 0.0 {
		return;
	}
	let solver_body_a = read_solver_body(solver_body_list, contact.solver_index_a);
	let solver_body_b = read_solver_body(solver_body_list, contact.solver_index_b);
	let inverse_mass_a = contact.inverse_mass_a;
	let inverse_mass_b = contact.inverse_mass_b;
	let inverse_inertia_a = contact.inverse_inertia_a;
	let inverse_inertia_b = contact.inverse_inertia_b;
	let mut velocity_a_x = solver_body_a.linear_velocity_x;
	let mut velocity_a_y = solver_body_a.linear_velocity_y;
	let mut angular_velocity_a = solver_body_a.angular_velocity;
	let mut velocity_b_x = solver_body_b.linear_velocity_x;
	let mut velocity_b_y = solver_body_b.linear_velocity_y;
	let mut angular_velocity_b = solver_body_b.angular_velocity;
	let manifold = &mut contact.manifold;
	let normal_x = manifold.normal_x;
	let normal_y = manifold.normal_y;
	let threshold = context.restitution_threshold;
	for point_index in 0..manifold.point_count {
		let manifold_point = &mut manifold.points[point_index];
		if manifold_point.relative_velocity > -threshold || manifold_point.maximum_normal_impulse == 0.0 {
			continue;
		}
		let anchor_a_x = manifold_point.anchor_a_x;
		let anchor_a_y = manifold_point.anchor_a_y;
		let anchor_b_x = manifold_point.anchor_b_x;
		let anchor_b_y = manifold_point.anchor_b_y;
		let relative_velocity_x = velocity_b_x - angular_velocity_b * anchor_b_y - velocity_a_x + angular_velocity_a * anchor_a_y;
		let relative_velocity_y = velocity_b_y + angular_velocity_b * anchor_b_x - velocity_a_y - angular_velocity_a * anchor_a_x;
		let normal_velocity = normal_x * relative_velocity_x + normal_y * relative_velocity_y;
		let mut impulse = -manifold_point.normal_mass * (normal_velocity + restitution * manifold_point.relative_velocity);
		let new_impulse = (manifold_point.normal_impulse + impulse).max(0.0);
		impulse = new_impulse - manifold_point.normal_impulse;
		manifold_point.normal_impulse = new_impulse;
		manifold_point.maximum_normal_impulse = manifold_point.maximum_normal_impulse.max(impulse);
		let impulse_x = impulse * normal_x;
		let impulse_y = impulse * normal_y;
		velocity_a_x -= inverse_mass_a * impulse_x;
		velocity_a_y -= inverse_mass_a * impulse_y;
		angular_velocity_a -= inverse_inertia_a * (anchor_a_x * impulse_y - anchor_a_y * impulse_x);
		velocity_b_x += inverse_mass_b * impulse_x;
		velocity_b_y += inverse_mass_b * impulse_y;
		angular_velocity_b += inverse_inertia_b * (anchor_b_x * impulse_y - anchor_b_y * impulse_x);
	}
	write_solver_velocity(solver_body_list, contact.solver_index_a, velocity_a_x, velocity_a_y, angular_velocity_a);
	write_solver_velocity(solver_body_list, contact.solver_index_b, velocity_b_x, velocity_b_y, angular_velocity_b);
}

//==============================================================================
/// 회전을 각도 변화만큼 돌리고 길이를 1 로 맞추기. ((코사인, 사인))
//==============================================================================
pub(crate) fn integrate_rotation(cosine: f64, sine: f64, delta_angle: f64) -> (f64, f64) {
	let next_cosine = cosine - delta_angle * sine;
	let next_sine = sine + delta_angle * cosine;
	let magnitude = (next_cosine * next_cosine + next_sine * next_sine).sqrt();
	let inverse_magnitude = if magnitude > 0.0 {
		1.0 / magnitude
	}
	else {
		0.0
	};
	(next_cosine * inverse_magnitude, next_sine * inverse_magnitude)
}
