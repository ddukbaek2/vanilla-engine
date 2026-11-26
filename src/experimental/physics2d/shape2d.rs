//==============================================================================
//! 2D 물리의 모양 정의, 거르기, 모양.
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/shape2d.js 와 같은 API, 같은 셈이다.
//! - 원본과 다른 점: 사용자 값(user_data)은 u64 다. (원본은 아무 값) 거르기 비트는 u32, 무리 번호는 i32 다.
//==============================================================================
use super::geometry2d::{Capsule2D, Circle2D, Polygon2D, Segment2D, ShapeGeometry2D, ShapeType2D};
use super::physicsid2d::{BodyId2D, ShapeId2D};
use super::physicsvector2::PhysicsVector2;
use crate::experimental::spatial::aabb2d::AABB2D;

//==============================================================================
/// 충돌 거르기. (분류, 마스크, 무리)
/// - 두 모양은 (A 분류 & B 마스크) 와 (B 분류 & A 마스크) 가 모두 0 이 아닐 때 부딪힌다. 비트는 32 개다. (유니티 2D 의 층 32 개와 같은 수)
/// - 무리 번호가 0 이 아니고 서로 같으면 분류, 마스크보다 먼저 본다. 양수면 늘 부딪히고 음수면 늘 지나친다. (Box2D 의 groupIndex 와 같은 규칙)
///
/// # 예제
/// 플레이어와 적만 부딪히게
/// ```rust
/// use vanilla::experimental::physics2d::CollisionFilter2D;
///
/// let mut player_filter = CollisionFilter2D::new();
/// player_filter.category_bits = 0x0002;
/// player_filter.mask_bits = 0x0004;
/// let mut enemy_filter = CollisionFilter2D::new();
/// enemy_filter.category_bits = 0x0004;
/// enemy_filter.mask_bits = 0x0002;
/// assert!(player_filter.should_collide(&enemy_filter));
/// assert!(!player_filter.should_collide(&player_filter));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionFilter2D {
	/// 이 모양의 분류 비트. (기본 1)
	pub category_bits: u32,
	/// 부딪힐 분류 비트. (기본 모두)
	pub mask_bits: u32,
	/// 무리 번호. (0 이면 쓰지 않는다)
	pub group_index: i32,
}

impl Default for CollisionFilter2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> CollisionFilter2D {
		CollisionFilter2D::new()
	}
}

impl CollisionFilter2D {
	//==============================================================================
	/// 생성. (분류 1, 마스크 모두, 무리 0)
	///
	/// # 돌려줌
	/// 새 거르기.
	//==============================================================================
	pub fn new() -> CollisionFilter2D {
		CollisionFilter2D {
			category_bits: 1,
			mask_bits: u32::MAX,
			group_index: 0,
		}
	}

	//==============================================================================
	/// 다른 거르기와 부딪히는지.
	///
	/// # 인자
	/// - `other`: 견줄 거르기.
	///
	/// # 돌려줌
	/// 부딪히면 true.
	//==============================================================================
	pub fn should_collide(&self, other: &CollisionFilter2D) -> bool {
		should_filters_collide(self.category_bits, self.mask_bits, self.group_index, other.category_bits, other.mask_bits, other.group_index)
	}
}

//==============================================================================
/// 두 거르기 값이 부딪히는지.
//==============================================================================
pub(crate) fn should_filters_collide(category_bits_a: u32, mask_bits_a: u32, group_index_a: i32, category_bits_b: u32, mask_bits_b: u32, group_index_b: i32) -> bool {
	if group_index_a == group_index_b && group_index_a != 0 {
		return group_index_a > 0;
	}
	(category_bits_a & mask_bits_b) != 0 && (category_bits_b & mask_bits_a) != 0
}

//==============================================================================
/// 질의 거르기. (광선, 겹침 질의가 찾을 모양)
/// - (모양 분류 & 질의 마스크) 와 (모양 마스크 & 질의 분류) 가 모두 0 이 아닌 모양만 찾는다. (Box2D 의 b2QueryFilter 와 같다)
///
/// # 예제
/// 2 번 분류만 찾는 질의
/// ```rust
/// use vanilla::experimental::physics2d::QueryFilter2D;
///
/// let mut query_filter = QueryFilter2D::new();
/// query_filter.mask_bits = 0x0002;
/// assert_eq!(query_filter.category_bits, 1);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryFilter2D {
	/// 질의의 분류 비트. (기본 1)
	pub category_bits: u32,
	/// 찾을 분류 비트. (기본 모두)
	pub mask_bits: u32,
}

impl Default for QueryFilter2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> QueryFilter2D {
		QueryFilter2D::new()
	}
}

impl QueryFilter2D {
	//==============================================================================
	/// 생성. (분류 1, 마스크 모두)
	///
	/// # 돌려줌
	/// 새 질의 거르기.
	//==============================================================================
	pub fn new() -> QueryFilter2D {
		QueryFilter2D {
			category_bits: 1,
			mask_bits: u32::MAX,
		}
	}
}

//==============================================================================
/// 모양 정의. (create_shape 에 넘기는 값)
///
/// # 예제
/// 잘 튀는 모양
/// ```rust
/// use vanilla::experimental::physics2d::ShapeDefinition2D;
///
/// let mut shape_definition = ShapeDefinition2D::new();
/// shape_definition.density = 2.0;
/// shape_definition.restitution = 0.8;
/// assert_eq!(shape_definition.friction, 0.6);
/// assert!(!shape_definition.is_sensor);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeDefinition2D {
	/// 밀도. (넓이당 질량, 기본 1)
	pub density: f64,
	/// 마찰 계수. (기본 0.6, 두 모양의 값은 곱의 제곱근으로 섞는다)
	pub friction: f64,
	/// 반발 계수. (기본 0, 두 모양의 값은 큰 쪽을 쓴다)
	pub restitution: f64,
	/// 충돌 거르기.
	pub filter: CollisionFilter2D,
	/// 센서인지. (부딪히지 않고 겹침 사건만 낸다, 기본 false)
	pub is_sensor: bool,
	/// 센서에 들어가고 나갈 때 사건을 낼지. (기본 true)
	pub enable_sensor_events: bool,
	/// 닿기 시작과 끝 사건을 낼지. (기본 true)
	pub enable_contact_events: bool,
	/// 사용자 값.
	pub user_data: u64,
}

impl Default for ShapeDefinition2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ShapeDefinition2D {
		ShapeDefinition2D::new()
	}
}

impl ShapeDefinition2D {
	//==============================================================================
	/// 생성. (밀도 1, 마찰 0.6, 반발 0)
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new() -> ShapeDefinition2D {
		ShapeDefinition2D {
			density: 1.0,
			friction: 0.6,
			restitution: 0.0,
			filter: CollisionFilter2D::new(),
			is_sensor: false,
			enable_sensor_events: true,
			enable_contact_events: true,
			user_data: 0,
		}
	}
}

//==============================================================================
/// 사슬 정의. (create_chain 에 넘기는 값, 이은 선분들)
/// - 점 i 와 i + 1 을 잇는 선분 모양을 하나씩 만든다. is_loop 면 마지막 점과 첫 점도 잇는다. (땅의 굴곡, 방의 벽)
/// - 선분은 양면이다. Box2D 사슬의 한쪽 면과 유령 꼭짓점(이음매에서 걸리지 않게 하는 것)은 아직 없다.
///
/// # 예제
/// 굽은 땅 사슬
/// ```rust
/// use vanilla::experimental::physics2d::{ChainDefinition2D, PhysicsVector2};
///
/// let mut chain_definition = ChainDefinition2D::new();
/// chain_definition.points = vec![PhysicsVector2::create(0.0, 300.0), PhysicsVector2::create(200.0, 320.0), PhysicsVector2::create(400.0, 300.0)];
/// assert!(!chain_definition.is_loop);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct ChainDefinition2D {
	/// 점들. (몸 기준, 2 개 이상)
	pub points: Vec<PhysicsVector2>,
	/// 끝과 처음을 이을지. (기본 false)
	pub is_loop: bool,
	/// 마찰 계수. (기본 0.6)
	pub friction: f64,
	/// 반발 계수. (기본 0)
	pub restitution: f64,
	/// 충돌 거르기.
	pub filter: CollisionFilter2D,
	/// 사용자 값. (선분마다 같은 값)
	pub user_data: u64,
}

impl Default for ChainDefinition2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ChainDefinition2D {
		ChainDefinition2D::new()
	}
}

impl ChainDefinition2D {
	//==============================================================================
	/// 생성. (점 없음)
	///
	/// # 돌려줌
	/// 새 정의.
	//==============================================================================
	pub fn new() -> ChainDefinition2D {
		ChainDefinition2D {
			points: Vec::new(),
			is_loop: false,
			friction: 0.6,
			restitution: 0.0,
			filter: CollisionFilter2D::new(),
			user_data: 0,
		}
	}
}

//==============================================================================
/// 모양. (몸에 붙은 충돌 모양 하나)
/// - PhysicsWorld2D::create_shape 로 만들고 get_shape(shape_id) 로 얻는다.
///
/// # 예제
/// 모양의 재질 바꾸기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, Circle2D, PhysicsVector2, PhysicsWorld2D, ShapeDefinition2D, ShapeType2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let body_id = world.create_body(&BodyDefinition2D::new());
/// let shape_id = world.create_shape(body_id, &ShapeDefinition2D::new(), Circle2D::create(PhysicsVector2::zero(), 2.0)).expect("모양");
/// let shape = world.get_shape_mut(shape_id).expect("모양");
/// shape.set_friction(0.2);
/// shape.set_restitution(0.5);
/// assert_eq!(shape.get_type(), ShapeType2D::Circle);
/// assert_eq!(shape.get_friction(), 0.2);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Shape2D {
	pub(crate) id: ShapeId2D,
	pub(crate) body_id: BodyId2D,
	pub(crate) body_index: usize,
	pub(crate) shape_type: ShapeType2D,
	pub(crate) center_x: f64,
	pub(crate) center_y: f64,
	pub(crate) count: usize,
	pub(crate) vertex_x: [f64; 8],
	pub(crate) vertex_y: [f64; 8],
	pub(crate) normal_x: [f64; 8],
	pub(crate) normal_y: [f64; 8],
	pub(crate) radius: f64,
	pub(crate) density: f64,
	pub(crate) friction: f64,
	pub(crate) restitution: f64,
	pub(crate) category_bits: u32,
	pub(crate) mask_bits: u32,
	pub(crate) group_index: i32,
	pub(crate) is_sensor: bool,
	pub(crate) enable_sensor_events: bool,
	pub(crate) enable_contact_events: bool,
	pub(crate) user_data: u64,
	pub(crate) proxy_id: usize,
	pub(crate) aabb: AABB2D,
	pub(crate) fat_aabb: AABB2D,
	pub(crate) is_filter_dirty: bool,
}

impl Shape2D {
	//==============================================================================
	/// 빈 모양 자료. (월드가 채운다)
	//==============================================================================
	pub(crate) fn new_empty() -> Shape2D {
		Shape2D {
			id: ShapeId2D::new(0, 0),
			body_id: BodyId2D::new(0, 0),
			body_index: 0,
			shape_type: ShapeType2D::Circle,
			center_x: 0.0,
			center_y: 0.0,
			count: 0,
			vertex_x: [0.0; 8],
			vertex_y: [0.0; 8],
			normal_x: [0.0; 8],
			normal_y: [0.0; 8],
			radius: 0.0,
			density: 1.0,
			friction: 0.6,
			restitution: 0.0,
			category_bits: 1,
			mask_bits: u32::MAX,
			group_index: 0,
			is_sensor: false,
			enable_sensor_events: true,
			enable_contact_events: true,
			user_data: 0,
			proxy_id: usize::MAX,
			aabb: AABB2D::new(),
			fat_aabb: AABB2D::new(),
			is_filter_dirty: false,
		}
	}

	//==============================================================================
	/// 아이디 반환.
	///
	/// # 돌려줌
	/// 이 모양의 아이디.
	//==============================================================================
	pub fn get_id(&self) -> ShapeId2D {
		self.id
	}

	//==============================================================================
	/// 붙은 몸의 아이디 반환.
	///
	/// # 돌려줌
	/// 몸 아이디.
	//==============================================================================
	pub fn get_body_id(&self) -> BodyId2D {
		self.body_id
	}

	//==============================================================================
	/// 모양의 종류 반환.
	///
	/// # 돌려줌
	/// 종류.
	//==============================================================================
	pub fn get_type(&self) -> ShapeType2D {
		self.shape_type
	}

	//==============================================================================
	/// 모양 반환. (몸 기준 좌표의 복사본)
	///
	/// # 돌려줌
	/// 원, 캡슐, 선분, 볼록 다각형 가운데 하나.
	//==============================================================================
	pub fn get_geometry(&self) -> ShapeGeometry2D {
		match self.shape_type {
			ShapeType2D::Circle => ShapeGeometry2D::Circle(Circle2D::create(PhysicsVector2::create(self.center_x, self.center_y), self.radius)),
			ShapeType2D::Capsule => ShapeGeometry2D::Capsule(Capsule2D::create(PhysicsVector2::create(self.vertex_x[0], self.vertex_y[0]), PhysicsVector2::create(self.vertex_x[1], self.vertex_y[1]), self.radius)),
			ShapeType2D::Segment => ShapeGeometry2D::Segment(Segment2D::create(PhysicsVector2::create(self.vertex_x[0], self.vertex_y[0]), PhysicsVector2::create(self.vertex_x[1], self.vertex_y[1]))),
			ShapeType2D::Polygon => {
				let mut vertices = Vec::with_capacity(self.count);
				let mut normals = Vec::with_capacity(self.count);
				for index in 0..self.count {
					vertices.push(PhysicsVector2::create(self.vertex_x[index], self.vertex_y[index]));
					normals.push(PhysicsVector2::create(self.normal_x[index], self.normal_y[index]));
				}
				let centroid = Polygon2D::compute_centroid(&vertices);
				ShapeGeometry2D::Polygon(Polygon2D {
					vertices,
					normals,
					centroid,
					radius: self.radius,
				})
			}
		}
	}

	//==============================================================================
	/// 밀도 반환.
	///
	/// # 돌려줌
	/// 밀도.
	//==============================================================================
	pub fn get_density(&self) -> f64 {
		self.density
	}

	//==============================================================================
	/// 마찰 계수 설정. (다음 걸음부터)
	///
	/// # 인자
	/// - `friction`: 마찰 계수. (0 이상)
	//==============================================================================
	pub fn set_friction(&mut self, friction: f64) {
		self.friction = friction.max(0.0);
	}

	//==============================================================================
	/// 마찰 계수 반환.
	///
	/// # 돌려줌
	/// 마찰 계수.
	//==============================================================================
	pub fn get_friction(&self) -> f64 {
		self.friction
	}

	//==============================================================================
	/// 반발 계수 설정. (다음 걸음부터)
	///
	/// # 인자
	/// - `restitution`: 반발 계수. (0 이상, 1 이면 튄 속력을 거의 그대로 돌려준다)
	//==============================================================================
	pub fn set_restitution(&mut self, restitution: f64) {
		self.restitution = restitution.max(0.0);
	}

	//==============================================================================
	/// 반발 계수 반환.
	///
	/// # 돌려줌
	/// 반발 계수.
	//==============================================================================
	pub fn get_restitution(&self) -> f64 {
		self.restitution
	}

	//==============================================================================
	/// 센서인지.
	///
	/// # 돌려줌
	/// 센서면 true.
	//==============================================================================
	pub fn is_sensor(&self) -> bool {
		self.is_sensor
	}

	//==============================================================================
	/// 충돌 거르기 설정. (다음 걸음 앞에서 닿음을 다시 고른다)
	///
	/// # 인자
	/// - `filter`: 새 거르기.
	//==============================================================================
	pub fn set_filter(&mut self, filter: &CollisionFilter2D) {
		self.category_bits = filter.category_bits;
		self.mask_bits = filter.mask_bits;
		self.group_index = filter.group_index;
		self.is_filter_dirty = true;
	}

	//==============================================================================
	/// 충돌 거르기 반환.
	///
	/// # 돌려줌
	/// 거르기.
	//==============================================================================
	pub fn get_filter(&self) -> CollisionFilter2D {
		CollisionFilter2D {
			category_bits: self.category_bits,
			mask_bits: self.mask_bits,
			group_index: self.group_index,
		}
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

	//==============================================================================
	/// 월드 좌표의 경계 상자 반환. (마지막 걸음의 자리)
	///
	/// # 돌려줌
	/// 경계 상자.
	//==============================================================================
	pub fn get_aabb(&self) -> AABB2D {
		self.aabb
	}
}

//==============================================================================
/// 모양 자료에 모양을 넣기. (받을 수 있는 모양이면 true)
//==============================================================================
pub(crate) fn set_shape_geometry(data: &mut Shape2D, geometry: &ShapeGeometry2D) -> bool {
	match geometry {
		ShapeGeometry2D::Circle(circle) => {
			data.shape_type = ShapeType2D::Circle;
			data.center_x = circle.center.x;
			data.center_y = circle.center.y;
			data.radius = circle.radius;
			data.count = 0;
			true
		}
		ShapeGeometry2D::Capsule(_) | ShapeGeometry2D::Segment(_) => {
			let (is_capsule, point1, point2, radius) = match geometry {
				ShapeGeometry2D::Capsule(capsule) => (true, capsule.center1, capsule.center2, capsule.radius),
				ShapeGeometry2D::Segment(segment) => (false, segment.point1, segment.point2, 0.0),
				_ => return false,
			};
			let axis_x = point2.x - point1.x;
			let axis_y = point2.y - point1.y;
			let length = (axis_x * axis_x + axis_y * axis_y).sqrt();
			if length <= 0.0 || length.is_nan() {
				return false;
			}
			data.shape_type = if is_capsule {
				ShapeType2D::Capsule
			}
			else {
				ShapeType2D::Segment
			};
			data.count = 2;
			data.vertex_x[0] = point1.x;
			data.vertex_y[0] = point1.y;
			data.vertex_x[1] = point2.x;
			data.vertex_y[1] = point2.y;
			data.normal_x[0] = axis_y / length;
			data.normal_y[0] = -axis_x / length;
			data.normal_x[1] = -data.normal_x[0];
			data.normal_y[1] = -data.normal_y[0];
			data.radius = radius;
			data.center_x = 0.5 * (point1.x + point2.x);
			data.center_y = 0.5 * (point1.y + point2.y);
			true
		}
		ShapeGeometry2D::Polygon(polygon) => {
			let count = polygon.vertices.len();
			if !(3..=8).contains(&count) || polygon.normals.len() != count {
				return false;
			}
			data.shape_type = ShapeType2D::Polygon;
			data.count = count;
			for index in 0..count {
				data.vertex_x[index] = polygon.vertices[index].x;
				data.vertex_y[index] = polygon.vertices[index].y;
				data.normal_x[index] = polygon.normals[index].x;
				data.normal_y[index] = polygon.normals[index].y;
			}
			data.radius = polygon.radius;
			data.center_x = polygon.centroid.x;
			data.center_y = polygon.centroid.y;
			true
		}
	}
}

//==============================================================================
/// 모양의 질량. ([질량, 중심 x, 중심 y, 질량 중심에 대한 회전 관성])
//==============================================================================
pub(crate) fn compute_shape_mass(data: &Shape2D) -> [f64; 4] {
	let density = data.density;
	match data.shape_type {
		ShapeType2D::Circle => {
			let radius_squared = data.radius * data.radius;
			let mass = density * std::f64::consts::PI * radius_squared;
			return [mass, data.center_x, data.center_y, mass * 0.5 * radius_squared];
		}
		ShapeType2D::Segment => {
			return [0.0, data.center_x, data.center_y, 0.0];
		}
		ShapeType2D::Capsule => {
			let radius = data.radius;
			let radius_squared = radius * radius;
			let axis_x = data.vertex_x[1] - data.vertex_x[0];
			let axis_y = data.vertex_y[1] - data.vertex_y[0];
			let length = (axis_x * axis_x + axis_y * axis_y).sqrt();
			let box_mass = density * (2.0 * radius * length);
			let circle_mass = density * std::f64::consts::PI * radius_squared;
			let mass = box_mass + circle_mass;
			// 반원 둘은 각자 중심이 원 중심에서 4r / (3 파이) 만큼 바깥이다. (평행축 정리로 캡슐 중심에 옮긴다)
			let semicircle_offset = 4.0 * radius / (3.0 * std::f64::consts::PI);
			let half_length = 0.5 * length;
			let circle_inertia = circle_mass * (0.5 * radius_squared + half_length * half_length + 2.0 * half_length * semicircle_offset);
			let box_inertia = box_mass * (4.0 * radius_squared + length * length) / 12.0;
			return [mass, data.center_x, data.center_y, circle_inertia + box_inertia];
		}
		ShapeType2D::Polygon => {}
	}

	// 다각형. (둥근 반지름이 있으면 꼭짓점을 이웃 법선의 가운데 쪽으로 반지름 x 루트 2 만큼 밀어 어림한다)
	let count = data.count;
	let mut point_x = [0.0; 8];
	let mut point_y = [0.0; 8];
	for index in 0..count {
		point_x[index] = data.vertex_x[index];
		point_y[index] = data.vertex_y[index];
		if data.radius > 0.0 {
			let previous = (index + count - 1) % count;
			let middle_x = data.normal_x[previous] + data.normal_x[index];
			let middle_y = data.normal_y[previous] + data.normal_y[index];
			let middle_length = (middle_x * middle_x + middle_y * middle_y).sqrt();
			let push_distance = std::f64::consts::SQRT_2 * data.radius;
			point_x[index] += push_distance * middle_x / middle_length;
			point_y[index] += push_distance * middle_y / middle_length;
		}
	}
	let origin_x = point_x[0];
	let origin_y = point_y[0];
	let mut area = 0.0;
	let mut center_x = 0.0;
	let mut center_y = 0.0;
	let mut rotational_inertia = 0.0;
	for index in 1..count - 1 {
		let edge1_x = point_x[index] - origin_x;
		let edge1_y = point_y[index] - origin_y;
		let edge2_x = point_x[index + 1] - origin_x;
		let edge2_y = point_y[index + 1] - origin_y;
		let cross = edge1_x * edge2_y - edge1_y * edge2_x;
		let triangle_area = 0.5 * cross;
		area += triangle_area;
		center_x += triangle_area * (edge1_x + edge2_x) / 3.0;
		center_y += triangle_area * (edge1_y + edge2_y) / 3.0;
		let integral_x = edge1_x * edge1_x + edge2_x * edge1_x + edge2_x * edge2_x;
		let integral_y = edge1_y * edge1_y + edge2_y * edge1_y + edge2_y * edge2_y;
		rotational_inertia += (0.25 / 3.0 * cross) * (integral_x + integral_y);
	}
	let mass = density * area;
	center_x /= area;
	center_y /= area;
	// 첫 꼭짓점에 대한 관성을 질량 중심으로 옮긴다.
	let inertia = density * rotational_inertia - mass * (center_x * center_x + center_y * center_y);
	[mass, origin_x + center_x, origin_y + center_y, inertia]
}

//==============================================================================
/// 모양의 월드 경계 상자를 aabb 에 적기.
//==============================================================================
pub(crate) fn compute_shape_aabb(data: &Shape2D, position_x: f64, position_y: f64, cosine: f64, sine: f64, aabb: &mut AABB2D) {
	if data.shape_type == ShapeType2D::Circle {
		let center_x = cosine * data.center_x - sine * data.center_y + position_x;
		let center_y = sine * data.center_x + cosine * data.center_y + position_y;
		aabb.set(center_x - data.radius, center_y - data.radius, center_x + data.radius, center_y + data.radius);
		return;
	}
	let mut minimum_x = f64::INFINITY;
	let mut minimum_y = f64::INFINITY;
	let mut maximum_x = f64::NEG_INFINITY;
	let mut maximum_y = f64::NEG_INFINITY;
	for index in 0..data.count {
		let vertex_x = cosine * data.vertex_x[index] - sine * data.vertex_y[index] + position_x;
		let vertex_y = sine * data.vertex_x[index] + cosine * data.vertex_y[index] + position_y;
		minimum_x = minimum_x.min(vertex_x);
		minimum_y = minimum_y.min(vertex_y);
		maximum_x = maximum_x.max(vertex_x);
		maximum_y = maximum_y.max(vertex_y);
	}
	aabb.set(minimum_x - data.radius, minimum_y - data.radius, maximum_x + data.radius, maximum_y + data.radius);
}

//==============================================================================
/// 몸 질량 중심에서 모양의 가장 먼 거리. (잠자기 속력에 쓴다)
//==============================================================================
pub(crate) fn compute_shape_extent(data: &Shape2D, local_center_x: f64, local_center_y: f64) -> f64 {
	if data.shape_type == ShapeType2D::Circle {
		let difference_x = data.center_x - local_center_x;
		let difference_y = data.center_y - local_center_y;
		return (difference_x * difference_x + difference_y * difference_y).sqrt() + data.radius;
	}
	let mut maximum_distance_squared: f64 = 0.0;
	for index in 0..data.count {
		let difference_x = data.vertex_x[index] - local_center_x;
		let difference_y = data.vertex_y[index] - local_center_y;
		maximum_distance_squared = maximum_distance_squared.max(difference_x * difference_x + difference_y * difference_y);
	}
	maximum_distance_squared.sqrt() + data.radius
}
