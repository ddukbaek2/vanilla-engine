//==============================================================================
//! 2D 물리의 모양. (원, 캡슐, 선분, 볼록 다각형, 몸 기준 좌표)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/geometry2d.js 와 같은 API, 같은 셈이다.
//! - 원본과 다른 점: createShape 가 받는 모양 하나를 ShapeGeometry2D 열거로 묶었다. (원본은 네 클래스 가운데 하나)
//!   make_polygon 은 null 대신 None 을 돌려준다.
//==============================================================================
// 옮긴 원문: Box2D 3 의 셈에서 옮김, MIT, THIRD_PARTY_LICENSES.md
use super::physicsmath2d::PhysicsMath2D;
use super::physicsvector2::PhysicsVector2;

/// 볼록 다각형 꼭짓점 수의 상한. (Box2D 의 B2_MAX_POLYGON_VERTICES 와 같은 8)
pub const PHYSICS2D_MAXIMUM_POLYGON_VERTICES: usize = 8;

//==============================================================================
/// 모양의 종류. (값 차례가 충돌 짝의 차례다. 원이 늘 뒤에 온다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeType2D {
	/// 원.
	Circle = 0,
	/// 캡슐. (두 중심을 잇는 선분에 반지름을 두른 것)
	Capsule = 1,
	/// 선분. (두께 없음, 보통 정적 몸의 땅과 벽)
	Segment = 2,
	/// 볼록 다각형. (꼭짓점 3 ~ 8 개, 둥근 반지름을 둘 수 있다)
	Polygon = 3,
}

//==============================================================================
/// 원. (몸 기준 좌표)
///
/// # 예제
/// 몸 중심에서 오른쪽으로 비킨 원
/// ```rust
/// use vanilla::experimental::physics2d::{Circle2D, PhysicsVector2};
///
/// let circle = Circle2D::create(PhysicsVector2::create(10.0, 0.0), 25.0);
/// assert_eq!(circle.center.x, 10.0);
/// assert_eq!(circle.radius, 25.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle2D {
	/// 중심. (몸 기준)
	pub center: PhysicsVector2,
	/// 반지름.
	pub radius: f64,
}

impl Default for Circle2D {
	//==============================================================================
	/// 기본값. (중심 0, 반지름 0.5)
	//==============================================================================
	fn default() -> Circle2D {
		Circle2D::create(PhysicsVector2::zero(), 0.5)
	}
}

impl Circle2D {
	//==============================================================================
	/// 중심과 반지름으로 만들기.
	///
	/// # 인자
	/// - `center`: 중심. (몸 기준)
	/// - `radius`: 반지름.
	///
	/// # 돌려줌
	/// 새 원.
	//==============================================================================
	pub fn create(center: impl Into<PhysicsVector2>, radius: f64) -> Circle2D {
		Circle2D {
			center: center.into(),
			radius,
		}
	}
}

//==============================================================================
/// 캡슐. (두 중심을 잇는 선분에 반지름을 두른 것, 몸 기준 좌표)
///
/// # 예제
/// 가로로 누운 캡슐
/// ```rust
/// use vanilla::experimental::physics2d::{Capsule2D, PhysicsVector2};
///
/// let capsule = Capsule2D::create(PhysicsVector2::create(-20.0, 0.0), PhysicsVector2::create(20.0, 0.0), 10.0);
/// assert_eq!(capsule.center2.x - capsule.center1.x, 40.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule2D {
	/// 첫 반원의 중심. (몸 기준)
	pub center1: PhysicsVector2,
	/// 둘째 반원의 중심. (몸 기준)
	pub center2: PhysicsVector2,
	/// 반지름.
	pub radius: f64,
}

impl Default for Capsule2D {
	//==============================================================================
	/// 기본값. (세로 길이 1, 반지름 0.25)
	//==============================================================================
	fn default() -> Capsule2D {
		Capsule2D::create(PhysicsVector2::create(0.0, -0.5), PhysicsVector2::create(0.0, 0.5), 0.25)
	}
}

impl Capsule2D {
	//==============================================================================
	/// 두 중심과 반지름으로 만들기.
	///
	/// # 인자
	/// - `center1`: 첫 반원의 중심. (몸 기준)
	/// - `center2`: 둘째 반원의 중심. (몸 기준, center1 과 달라야 한다)
	/// - `radius`: 반지름.
	///
	/// # 돌려줌
	/// 새 캡슐.
	//==============================================================================
	pub fn create(center1: impl Into<PhysicsVector2>, center2: impl Into<PhysicsVector2>, radius: f64) -> Capsule2D {
		Capsule2D {
			center1: center1.into(),
			center2: center2.into(),
			radius,
		}
	}
}

//==============================================================================
/// 선분. (두께 없음, 몸 기준 좌표)
/// - 질량이 없어 보통 정적 몸의 땅과 벽에 쓴다. 선분끼리는 부딪히지 않는다.
///
/// # 예제
/// 땅 선분
/// ```rust
/// use vanilla::experimental::physics2d::{PhysicsVector2, Segment2D};
///
/// let segment = Segment2D::create(PhysicsVector2::create(0.0, 400.0), PhysicsVector2::create(800.0, 400.0));
/// assert_eq!(segment.point2.x, 800.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment2D {
	/// 첫 점. (몸 기준)
	pub point1: PhysicsVector2,
	/// 둘째 점. (몸 기준)
	pub point2: PhysicsVector2,
}

impl Default for Segment2D {
	//==============================================================================
	/// 기본값. (가로 길이 1)
	//==============================================================================
	fn default() -> Segment2D {
		Segment2D::create(PhysicsVector2::create(-0.5, 0.0), PhysicsVector2::create(0.5, 0.0))
	}
}

impl Segment2D {
	//==============================================================================
	/// 두 점으로 만들기.
	///
	/// # 인자
	/// - `point1`: 첫 점. (몸 기준)
	/// - `point2`: 둘째 점. (몸 기준, point1 과 달라야 한다)
	///
	/// # 돌려줌
	/// 새 선분.
	//==============================================================================
	pub fn create(point1: impl Into<PhysicsVector2>, point2: impl Into<PhysicsVector2>) -> Segment2D {
		Segment2D {
			point1: point1.into(),
			point2: point2.into(),
		}
	}
}

//==============================================================================
/// 볼록 다각형. (꼭짓점 3 ~ 8 개, 둥근 반지름을 둘 수 있다, 몸 기준 좌표)
/// - 꼭짓점은 수학 좌표의 반시계 방향(넓이가 양수인 쪽)으로 들고, `normals[i]` 는 꼭짓점 i 에서 i + 1 로 가는 변의 바깥쪽 단위 법선이다.
///   엔진 화면은 y 가 아래로 커서 화면에서는 시계 방향으로 보인다. 셈은 같다.
/// - 직접 채우지 말고 make_box, make_offset_box, make_polygon 으로 만든다.
///
/// # 예제
/// 상자와 점 무리의 볼록 껍질
/// ```rust
/// use vanilla::experimental::physics2d::{PhysicsVector2, Polygon2D};
///
/// let box_polygon = Polygon2D::make_box(20.0, 10.0);
/// assert_eq!(box_polygon.vertices.len(), 4);
/// assert_eq!(box_polygon.normals[0], PhysicsVector2::create(0.0, -1.0));
/// let point_list = [PhysicsVector2::create(0.0, 0.0), PhysicsVector2::create(30.0, 0.0), PhysicsVector2::create(15.0, 5.0), PhysicsVector2::create(15.0, 25.0)];
/// let hull = Polygon2D::make_polygon(&point_list, 0.0).expect("hull");
/// assert_eq!(hull.vertices.len(), 3);
/// assert!((hull.centroid.x - 15.0).abs() < 1e-12);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct Polygon2D {
	/// 꼭짓점. (몸 기준, 반시계)
	pub vertices: Vec<PhysicsVector2>,
	/// 변의 바깥쪽 단위 법선.
	pub normals: Vec<PhysicsVector2>,
	/// 넓이 중심.
	pub centroid: PhysicsVector2,
	/// 둥근 반지름. (0 이면 모서리가 뾰족하다)
	pub radius: f64,
}

impl Default for Polygon2D {
	//==============================================================================
	/// 기본값. (한 변이 1 인 정사각형)
	//==============================================================================
	fn default() -> Polygon2D {
		Polygon2D::make_box(0.5, 0.5)
	}
}

impl Polygon2D {
	//==============================================================================
	/// 몸 중심의 상자.
	///
	/// # 인자
	/// - `half_width`: 너비의 절반.
	/// - `half_height`: 높이의 절반.
	///
	/// # 돌려줌
	/// 새 상자.
	//==============================================================================
	pub fn make_box(half_width: f64, half_height: f64) -> Polygon2D {
		Polygon2D {
			vertices: vec![PhysicsVector2::create(-half_width, -half_height), PhysicsVector2::create(half_width, -half_height), PhysicsVector2::create(half_width, half_height), PhysicsVector2::create(-half_width, half_height)],
			normals: vec![PhysicsVector2::create(0.0, -1.0), PhysicsVector2::create(1.0, 0.0), PhysicsVector2::create(0.0, 1.0), PhysicsVector2::create(-1.0, 0.0)],
			centroid: PhysicsVector2::zero(),
			radius: 0.0,
		}
	}

	//==============================================================================
	/// 둥근 모서리 상자. (모서리를 radius 로 두른다. 겉 크기는 절반 크기 + radius)
	///
	/// # 인자
	/// - `half_width`: 안쪽 상자 너비의 절반.
	/// - `half_height`: 안쪽 상자 높이의 절반.
	/// - `radius`: 둥근 반지름.
	///
	/// # 돌려줌
	/// 새 상자.
	//==============================================================================
	pub fn make_rounded_box(half_width: f64, half_height: f64, radius: f64) -> Polygon2D {
		let mut polygon = Polygon2D::make_box(half_width, half_height);
		polygon.radius = radius;
		polygon
	}

	//==============================================================================
	/// 비키고 돌린 상자. (몸 기준 center 에 angle 만큼 돌려 둔다)
	///
	/// # 인자
	/// - `half_width`: 너비의 절반.
	/// - `half_height`: 높이의 절반.
	/// - `center`: 상자 중심. (몸 기준)
	/// - `angle`: 돌림. (라디안)
	///
	/// # 돌려줌
	/// 새 상자.
	//==============================================================================
	pub fn make_offset_box(half_width: f64, half_height: f64, center: impl Into<PhysicsVector2>, angle: f64) -> Polygon2D {
		let center = center.into();
		let rotation = PhysicsMath2D::compute_rotation(angle);
		let cosine = rotation.cosine;
		let sine = rotation.sine;
		let mut polygon = Polygon2D::make_box(half_width, half_height);
		for index in 0..4 {
			let vertex = polygon.vertices[index];
			let normal = polygon.normals[index];
			polygon.vertices[index] = PhysicsVector2::create(cosine * vertex.x - sine * vertex.y + center.x, sine * vertex.x + cosine * vertex.y + center.y);
			polygon.normals[index] = PhysicsVector2::create(cosine * normal.x - sine * normal.y, sine * normal.x + cosine * normal.y);
		}
		polygon.centroid = center;
		polygon
	}

	//==============================================================================
	/// 점 무리의 볼록 껍질로 만들기. (앤드루의 단조 사슬)
	/// - 한 줄 위의 점과 겹친 점은 뺀다.
	///
	/// # 인자
	/// - `points`: 점 무리. (몸 기준, 차례는 상관없다)
	/// - `radius`: 둥근 반지름.
	///
	/// # 돌려줌
	/// 새 다각형. (껍질의 꼭짓점이 3 개보다 적거나 8 개보다 많으면 None)
	//==============================================================================
	pub fn make_polygon(points: &[PhysicsVector2], radius: f64) -> Option<Polygon2D> {
		let mut sorted_list: Vec<PhysicsVector2> = points.to_vec();
		sorted_list.sort_by(|first, second| {
			if first.x != second.x {
				return if first.x < second.x {
					std::cmp::Ordering::Less
				}
				else {
					std::cmp::Ordering::Greater
				};
			}
			if first.y != second.y {
				return if first.y < second.y {
					std::cmp::Ordering::Less
				}
				else {
					std::cmp::Ordering::Greater
				};
			}
			std::cmp::Ordering::Equal
		});
		let mut hull_list: Vec<PhysicsVector2> = Vec::new();
		for pass in 0..2 {
			let mut chain_list: Vec<PhysicsVector2> = Vec::new();
			for index in 0..sorted_list.len() {
				let point = if pass == 0 {
					sorted_list[index]
				}
				else {
					sorted_list[sorted_list.len() - 1 - index]
				};
				while chain_list.len() >= 2 {
					let origin = chain_list[chain_list.len() - 2];
					let middle = chain_list[chain_list.len() - 1];
					let cross = (middle.x - origin.x) * (point.y - origin.y) - (middle.y - origin.y) * (point.x - origin.x);
					if cross > 0.0 {
						break;
					}
					chain_list.pop();
				}
				chain_list.push(point);
			}
			chain_list.pop();
			hull_list.extend_from_slice(&chain_list);
		}
		if hull_list.len() < 3 || hull_list.len() > PHYSICS2D_MAXIMUM_POLYGON_VERTICES {
			return None;
		}
		let mut normals = Vec::with_capacity(hull_list.len());
		for index in 0..hull_list.len() {
			let vertex1 = hull_list[index];
			let vertex2 = hull_list[(index + 1) % hull_list.len()];
			let edge_x = vertex2.x - vertex1.x;
			let edge_y = vertex2.y - vertex1.y;
			let length = (edge_x * edge_x + edge_y * edge_y).sqrt();
			normals.push(PhysicsVector2::create(edge_y / length, -edge_x / length));
		}
		let centroid = Polygon2D::compute_centroid(&hull_list);
		Some(Polygon2D {
			vertices: hull_list,
			normals,
			centroid,
			radius,
		})
	}

	//==============================================================================
	/// 꼭짓점들의 넓이 중심. (첫 꼭짓점을 기준으로 삼각형 부채꼴)
	///
	/// # 인자
	/// - `vertices`: 반시계 꼭짓점.
	///
	/// # 돌려줌
	/// 넓이 중심.
	//==============================================================================
	pub fn compute_centroid(vertices: &[PhysicsVector2]) -> PhysicsVector2 {
		let origin_x = vertices[0].x;
		let origin_y = vertices[0].y;
		let mut center_x = 0.0;
		let mut center_y = 0.0;
		let mut area = 0.0;
		for index in 1..vertices.len() - 1 {
			let edge1_x = vertices[index].x - origin_x;
			let edge1_y = vertices[index].y - origin_y;
			let edge2_x = vertices[index + 1].x - origin_x;
			let edge2_y = vertices[index + 1].y - origin_y;
			let triangle_area = 0.5 * (edge1_x * edge2_y - edge1_y * edge2_x);
			center_x += triangle_area * (edge1_x + edge2_x) / 3.0;
			center_y += triangle_area * (edge1_y + edge2_y) / 3.0;
			area += triangle_area;
		}
		PhysicsVector2::create(origin_x + center_x / area, origin_y + center_y / area)
	}
}

//==============================================================================
/// 모양 하나. (create_shape 가 받는 원, 캡슐, 선분, 볼록 다각형 가운데 하나)
///
/// # 예제
/// 네 모양을 하나의 꼴로
/// ```rust
/// use vanilla::experimental::physics2d::{Circle2D, PhysicsVector2, Polygon2D, ShapeGeometry2D, ShapeType2D};
///
/// let geometry: ShapeGeometry2D = Circle2D::create(PhysicsVector2::zero(), 1.0).into();
/// assert_eq!(geometry.get_type(), ShapeType2D::Circle);
/// let geometry: ShapeGeometry2D = Polygon2D::make_box(1.0, 1.0).into();
/// assert_eq!(geometry.get_type(), ShapeType2D::Polygon);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeGeometry2D {
	/// 원.
	Circle(Circle2D),
	/// 캡슐.
	Capsule(Capsule2D),
	/// 선분.
	Segment(Segment2D),
	/// 볼록 다각형.
	Polygon(Polygon2D),
}

impl ShapeGeometry2D {
	//==============================================================================
	/// 모양의 종류 반환.
	///
	/// # 돌려줌
	/// 종류.
	//==============================================================================
	pub fn get_type(&self) -> ShapeType2D {
		match self {
			ShapeGeometry2D::Circle(_) => ShapeType2D::Circle,
			ShapeGeometry2D::Capsule(_) => ShapeType2D::Capsule,
			ShapeGeometry2D::Segment(_) => ShapeType2D::Segment,
			ShapeGeometry2D::Polygon(_) => ShapeType2D::Polygon,
		}
	}
}

impl From<Circle2D> for ShapeGeometry2D {
	//==============================================================================
	/// 원에서.
	//==============================================================================
	fn from(circle: Circle2D) -> ShapeGeometry2D {
		ShapeGeometry2D::Circle(circle)
	}
}

impl From<Capsule2D> for ShapeGeometry2D {
	//==============================================================================
	/// 캡슐에서.
	//==============================================================================
	fn from(capsule: Capsule2D) -> ShapeGeometry2D {
		ShapeGeometry2D::Capsule(capsule)
	}
}

impl From<Segment2D> for ShapeGeometry2D {
	//==============================================================================
	/// 선분에서.
	//==============================================================================
	fn from(segment: Segment2D) -> ShapeGeometry2D {
		ShapeGeometry2D::Segment(segment)
	}
}

impl From<Polygon2D> for ShapeGeometry2D {
	//==============================================================================
	/// 볼록 다각형에서.
	//==============================================================================
	fn from(polygon: Polygon2D) -> ShapeGeometry2D {
		ShapeGeometry2D::Polygon(polygon)
	}
}
