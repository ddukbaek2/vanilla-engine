//==============================================================================
//! 2D 물리의 좁은 단계. (안에서 쓴다, 공개 API 아님)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/narrowphase2d.js 와 같은 셈, 같은 차례다.
//! - 모양 짝의 닿음점(매니폴드), 광선, 점 판정을 월드 좌표에서 셈한다.
//! - 캡슐과 선분은 꼭짓점 2 개짜리 둥근 다각형으로 보고, 다각형끼리는 분리축(SAT)으로 기준 변을 고른 뒤 맞은편 변을 잘라 닿음점을 낸다.
//!   떨어진 채 꼭짓점끼리 가장 가까우면 선분 거리로 꼭짓점과 꼭짓점 사이 법선을 쓴다. (Erin Catto, GDC 2007 Contact Manifolds 와 Box2D 3 의 방식)
//==============================================================================
// 옮긴 원문: Box2D 3 의 셈에서 옮김, MIT, THIRD_PARTY_LICENSES.md
use super::geometry2d::ShapeType2D;
use super::shape2d::Shape2D;

/// 길이를 나눌 때 0 으로 보는 아주 작은 값.
const NORMALIZE_EPSILON: f64 = 1e-12;

//==============================================================================
/// 월드 좌표의 모양. (매 걸음 몸의 자리로 채운다)
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) struct WorldShape {
	pub(crate) shape_type: ShapeType2D,
	pub(crate) count: usize,
	pub(crate) vertex_x: [f64; 8],
	pub(crate) vertex_y: [f64; 8],
	pub(crate) normal_x: [f64; 8],
	pub(crate) normal_y: [f64; 8],
	pub(crate) radius: f64,
	pub(crate) center_x: f64,
	pub(crate) center_y: f64,
}

impl WorldShape {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub(crate) fn new() -> WorldShape {
		WorldShape {
			shape_type: ShapeType2D::Circle,
			count: 0,
			vertex_x: [0.0; 8],
			vertex_y: [0.0; 8],
			normal_x: [0.0; 8],
			normal_y: [0.0; 8],
			radius: 0.0,
			center_x: 0.0,
			center_y: 0.0,
		}
	}
}

//==============================================================================
/// 모양 자료를 몸의 자리와 회전으로 월드 모양에 채우기.
//==============================================================================
pub(crate) fn fill_world_shape(shape_data: &Shape2D, position_x: f64, position_y: f64, cosine: f64, sine: f64, world_shape: &mut WorldShape) {
	world_shape.shape_type = shape_data.shape_type;
	world_shape.count = shape_data.count;
	world_shape.radius = shape_data.radius;
	world_shape.center_x = cosine * shape_data.center_x - sine * shape_data.center_y + position_x;
	world_shape.center_y = sine * shape_data.center_x + cosine * shape_data.center_y + position_y;
	for index in 0..shape_data.count {
		world_shape.vertex_x[index] = cosine * shape_data.vertex_x[index] - sine * shape_data.vertex_y[index] + position_x;
		world_shape.vertex_y[index] = sine * shape_data.vertex_x[index] + cosine * shape_data.vertex_y[index] + position_y;
		world_shape.normal_x[index] = cosine * shape_data.normal_x[index] - sine * shape_data.normal_y[index];
		world_shape.normal_y[index] = sine * shape_data.normal_x[index] + cosine * shape_data.normal_y[index];
	}
}

//==============================================================================
/// 닿음점 하나.
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ManifoldPoint {
	/// 두 겉면 사이 가운데의 월드 점.
	pub(crate) point_x: f64,
	pub(crate) point_y: f64,
	/// 겉면 사이 거리. (음수면 파고듦)
	pub(crate) separation: f64,
	/// 모양 특징 번호. (걸음 사이 웜 스타트 짝짓기)
	pub(crate) id: u32,
	pub(crate) normal_impulse: f64,
	pub(crate) tangent_impulse: f64,
	pub(crate) maximum_normal_impulse: f64,
	pub(crate) anchor_a_x: f64,
	pub(crate) anchor_a_y: f64,
	pub(crate) anchor_b_x: f64,
	pub(crate) anchor_b_y: f64,
	pub(crate) base_separation: f64,
	pub(crate) normal_mass: f64,
	pub(crate) tangent_mass: f64,
	pub(crate) relative_velocity: f64,
}

//==============================================================================
/// 매니폴드. (모양 짝의 법선과 닿음점 0 ~ 2 개, 법선은 A 에서 B 쪽)
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Manifold {
	pub(crate) normal_x: f64,
	pub(crate) normal_y: f64,
	pub(crate) point_count: usize,
	pub(crate) points: [ManifoldPoint; 2],
}

//==============================================================================
/// 매니폴드 점 하나 적기.
//==============================================================================
fn push_manifold_point(manifold: &mut Manifold, point_x: f64, point_y: f64, separation: f64, id: u32) {
	let manifold_point = &mut manifold.points[manifold.point_count];
	manifold_point.point_x = point_x;
	manifold_point.point_y = point_y;
	manifold_point.separation = separation;
	manifold_point.id = id;
	manifold.point_count += 1;
}

//==============================================================================
/// 두 월드 모양의 매니폴드. (A 가 원이면 B 도 원이어야 한다. 원은 늘 B 쪽)
//==============================================================================
pub(crate) fn collide_shapes(shape_a: &WorldShape, shape_b: &WorldShape, speculative_distance: f64, linear_slop: f64, manifold: &mut Manifold) {
	manifold.point_count = 0;
	if shape_a.shape_type == ShapeType2D::Circle {
		collide_circles(shape_a, shape_b, speculative_distance, manifold);
		return;
	}
	if shape_b.shape_type == ShapeType2D::Circle {
		collide_polygon_and_circle(shape_a, shape_b, speculative_distance, manifold);
		return;
	}
	collide_polygons(shape_a, shape_b, speculative_distance, linear_slop, manifold);
}

//==============================================================================
/// 원과 원.
//==============================================================================
fn collide_circles(shape_a: &WorldShape, shape_b: &WorldShape, speculative_distance: f64, manifold: &mut Manifold) {
	let difference_x = shape_b.center_x - shape_a.center_x;
	let difference_y = shape_b.center_y - shape_a.center_y;
	let distance = (difference_x * difference_x + difference_y * difference_y).sqrt();
	let separation = distance - shape_a.radius - shape_b.radius;
	if separation > speculative_distance {
		return;
	}
	let mut normal_x = 1.0;
	let mut normal_y = 0.0;
	if distance > NORMALIZE_EPSILON {
		normal_x = difference_x / distance;
		normal_y = difference_y / distance;
	}
	let surface_a_x = shape_a.center_x + shape_a.radius * normal_x;
	let surface_a_y = shape_a.center_y + shape_a.radius * normal_y;
	let surface_b_x = shape_b.center_x - shape_b.radius * normal_x;
	let surface_b_y = shape_b.center_y - shape_b.radius * normal_y;
	manifold.normal_x = normal_x;
	manifold.normal_y = normal_y;
	push_manifold_point(manifold, 0.5 * (surface_a_x + surface_b_x), 0.5 * (surface_a_y + surface_b_y), separation, 0);
}

//==============================================================================
/// 둥근 다각형(캡슐, 선분 포함)과 원.
//==============================================================================
fn collide_polygon_and_circle(shape_a: &WorldShape, shape_b: &WorldShape, speculative_distance: f64, manifold: &mut Manifold) {
	let center_x = shape_b.center_x;
	let center_y = shape_b.center_y;
	let radius = shape_a.radius + shape_b.radius;
	let mut separation = f64::NEG_INFINITY;
	let mut normal_index = 0;
	for index in 0..shape_a.count {
		let edge_separation = shape_a.normal_x[index] * (center_x - shape_a.vertex_x[index]) + shape_a.normal_y[index] * (center_y - shape_a.vertex_y[index]);
		if edge_separation > separation {
			separation = edge_separation;
			normal_index = index;
		}
	}
	if separation > radius + speculative_distance {
		return;
	}
	let next_index = (normal_index + 1) % shape_a.count;
	let vertex1_x = shape_a.vertex_x[normal_index];
	let vertex1_y = shape_a.vertex_y[normal_index];
	let vertex2_x = shape_a.vertex_x[next_index];
	let vertex2_y = shape_a.vertex_y[next_index];
	let fraction1 = (center_x - vertex1_x) * (vertex2_x - vertex1_x) + (center_y - vertex1_y) * (vertex2_y - vertex1_y);
	let fraction2 = (center_x - vertex2_x) * (vertex1_x - vertex2_x) + (center_y - vertex2_y) * (vertex1_y - vertex2_y);

	let normal_x;
	let normal_y;
	let surface_a_x;
	let surface_a_y;
	if (fraction1 < 0.0 || fraction2 < 0.0) && separation > NORMALIZE_EPSILON {
		// 꼭짓점 쪽.
		let vertex_x = if fraction1 < 0.0 {
			vertex1_x
		}
		else {
			vertex2_x
		};
		let vertex_y = if fraction1 < 0.0 {
			vertex1_y
		}
		else {
			vertex2_y
		};
		let difference_x = center_x - vertex_x;
		let difference_y = center_y - vertex_y;
		let distance = (difference_x * difference_x + difference_y * difference_y).sqrt();
		if distance > radius + speculative_distance {
			return;
		}
		normal_x = difference_x / distance;
		normal_y = difference_y / distance;
		surface_a_x = vertex_x + shape_a.radius * normal_x;
		surface_a_y = vertex_y + shape_a.radius * normal_y;
	}
	else {
		// 변 쪽.
		normal_x = shape_a.normal_x[normal_index];
		normal_y = shape_a.normal_y[normal_index];
		let center_distance = (center_x - vertex1_x) * normal_x + (center_y - vertex1_y) * normal_y;
		surface_a_x = center_x + (shape_a.radius - center_distance) * normal_x;
		surface_a_y = center_y + (shape_a.radius - center_distance) * normal_y;
	}
	let surface_b_x = center_x - shape_b.radius * normal_x;
	let surface_b_y = center_y - shape_b.radius * normal_y;
	manifold.normal_x = normal_x;
	manifold.normal_y = normal_y;
	let point_separation = (surface_b_x - surface_a_x) * normal_x + (surface_b_y - surface_a_y) * normal_y;
	push_manifold_point(manifold, 0.5 * (surface_a_x + surface_b_x), 0.5 * (surface_a_y + surface_b_y), point_separation, 0);
}

//==============================================================================
/// shape1 의 변 가운데 shape2 의 꼭짓점이 가장 멀리 떨어진 변. ((변 번호, 떨어진 거리))
//==============================================================================
fn find_maximum_separation(shape1: &WorldShape, shape2: &WorldShape) -> (usize, f64) {
	let mut best_index = 0;
	let mut maximum_separation = f64::NEG_INFINITY;
	for index in 0..shape1.count {
		let normal_x = shape1.normal_x[index];
		let normal_y = shape1.normal_y[index];
		let vertex_x = shape1.vertex_x[index];
		let vertex_y = shape1.vertex_y[index];
		let mut minimum_separation = f64::INFINITY;
		for other_index in 0..shape2.count {
			let separation = normal_x * (shape2.vertex_x[other_index] - vertex_x) + normal_y * (shape2.vertex_y[other_index] - vertex_y);
			if separation < minimum_separation {
				minimum_separation = separation;
			}
		}
		if minimum_separation > maximum_separation {
			maximum_separation = minimum_separation;
			best_index = index;
		}
	}
	(best_index, maximum_separation)
}

//==============================================================================
/// 두 선분의 가장 가까운 점. (Ericson, Real-Time Collision Detection 5.1.9, (몫1, 몫2, 거리 제곱))
//==============================================================================
pub(crate) fn compute_segment_distance(point1_x: f64, point1_y: f64, end1_x: f64, end1_y: f64, point2_x: f64, point2_y: f64, end2_x: f64, end2_y: f64) -> (f64, f64, f64) {
	let direction1_x = end1_x - point1_x;
	let direction1_y = end1_y - point1_y;
	let direction2_x = end2_x - point2_x;
	let direction2_y = end2_y - point2_y;
	let offset_x = point1_x - point2_x;
	let offset_y = point1_y - point2_y;
	let length_squared1 = direction1_x * direction1_x + direction1_y * direction1_y;
	let length_squared2 = direction2_x * direction2_x + direction2_y * direction2_y;
	let offset_dot2 = direction2_x * offset_x + direction2_y * offset_y;
	let mut fraction1;
	let mut fraction2;
	if length_squared1 <= NORMALIZE_EPSILON && length_squared2 <= NORMALIZE_EPSILON {
		fraction1 = 0.0;
		fraction2 = 0.0;
	}
	else if length_squared1 <= NORMALIZE_EPSILON {
		fraction1 = 0.0;
		fraction2 = clamp(offset_dot2 / length_squared2, 0.0, 1.0);
	}
	else {
		let offset_dot1 = direction1_x * offset_x + direction1_y * offset_y;
		if length_squared2 <= NORMALIZE_EPSILON {
			fraction2 = 0.0;
			fraction1 = clamp(-offset_dot1 / length_squared1, 0.0, 1.0);
		}
		else {
			let direction_dot = direction1_x * direction2_x + direction1_y * direction2_y;
			let denominator = length_squared1 * length_squared2 - direction_dot * direction_dot;
			if denominator != 0.0 {
				fraction1 = clamp((direction_dot * offset_dot2 - offset_dot1 * length_squared2) / denominator, 0.0, 1.0);
			}
			else {
				fraction1 = 0.0;
			}
			fraction2 = (direction_dot * fraction1 + offset_dot2) / length_squared2;
			if fraction2 < 0.0 {
				fraction2 = 0.0;
				fraction1 = clamp(-offset_dot1 / length_squared1, 0.0, 1.0);
			}
			else if fraction2 > 1.0 {
				fraction2 = 1.0;
				fraction1 = clamp((direction_dot - offset_dot1) / length_squared1, 0.0, 1.0);
			}
		}
	}
	let closest1_x = point1_x + direction1_x * fraction1;
	let closest1_y = point1_y + direction1_y * fraction1;
	let closest2_x = point2_x + direction2_x * fraction2;
	let closest2_y = point2_y + direction2_y * fraction2;
	let difference_x = closest2_x - closest1_x;
	let difference_y = closest2_y - closest1_y;
	(fraction1, fraction2, difference_x * difference_x + difference_y * difference_y)
}

//==============================================================================
/// 값을 범위로 자르기.
//==============================================================================
fn clamp(value: f64, minimum: f64, maximum: f64) -> f64 {
	if value < minimum {
		return minimum;
	}
	if value > maximum {
		return maximum;
	}
	value
}

//==============================================================================
/// 둥근 다각형과 둥근 다각형. (캡슐과 선분 포함)
//==============================================================================
fn collide_polygons(shape_a: &WorldShape, shape_b: &WorldShape, speculative_distance: f64, linear_slop: f64, manifold: &mut Manifold) {
	let (edge_a, separation_a) = find_maximum_separation(shape_a, shape_b);
	let (edge_b, separation_b) = find_maximum_separation(shape_b, shape_a);
	let radius = shape_a.radius + shape_b.radius;
	if separation_a > speculative_distance + radius || separation_b > speculative_distance + radius {
		return;
	}

	// 기준 변. (B 쪽이 확실히 더 떨어졌을 때만 B 를 기준으로 삼아 걸음마다 바뀌어 떨지 않게)
	let mut is_flipped = false;
	let mut shape1 = shape_a;
	let mut shape2 = shape_b;
	let mut edge1 = edge_a;
	let mut separation = separation_a;
	if separation_b > separation_a + 0.1 * linear_slop {
		is_flipped = true;
		shape1 = shape_b;
		shape2 = shape_a;
		edge1 = edge_b;
		separation = separation_b;
	}

	// 맞은편 변. (기준 법선과 가장 반대로 향한 변)
	let reference_normal_x = shape1.normal_x[edge1];
	let reference_normal_y = shape1.normal_y[edge1];
	let mut edge2 = 0;
	let mut minimum_dot = f64::INFINITY;
	for index in 0..shape2.count {
		let dot = reference_normal_x * shape2.normal_x[index] + reference_normal_y * shape2.normal_y[index];
		if dot < minimum_dot {
			minimum_dot = dot;
			edge2 = index;
		}
	}

	if separation > 0.1 * linear_slop {
		// 떨어져 있으면 두 변의 가장 가까운 점을 보고, 꼭짓점끼리면 그 사이 방향을 법선으로 쓴다.
		let index11 = edge1;
		let index12 = (edge1 + 1) % shape1.count;
		let index21 = edge2;
		let index22 = (edge2 + 1) % shape2.count;
		let (fraction1, fraction2, distance_squared) = compute_segment_distance(shape1.vertex_x[index11], shape1.vertex_y[index11], shape1.vertex_x[index12], shape1.vertex_y[index12], shape2.vertex_x[index21], shape2.vertex_y[index21], shape2.vertex_x[index22], shape2.vertex_y[index22]);
		let is_vertex1 = fraction1 == 0.0 || fraction1 == 1.0;
		let is_vertex2 = fraction2 == 0.0 || fraction2 == 1.0;
		if is_vertex1 && is_vertex2 {
			let vertex_index1 = if fraction1 == 0.0 {
				index11
			}
			else {
				index12
			};
			let vertex_index2 = if fraction2 == 0.0 {
				index21
			}
			else {
				index22
			};
			let vertex1_x = shape1.vertex_x[vertex_index1];
			let vertex1_y = shape1.vertex_y[vertex_index1];
			let vertex2_x = shape2.vertex_x[vertex_index2];
			let vertex2_y = shape2.vertex_y[vertex_index2];
			let distance = distance_squared.sqrt();
			if distance > speculative_distance + radius {
				return;
			}
			if distance > NORMALIZE_EPSILON {
				let mut normal_x = (vertex2_x - vertex1_x) / distance;
				let mut normal_y = (vertex2_y - vertex1_y) / distance;
				let surface1_x = vertex1_x + shape1.radius * normal_x;
				let surface1_y = vertex1_y + shape1.radius * normal_y;
				let surface2_x = vertex2_x - shape2.radius * normal_x;
				let surface2_y = vertex2_y - shape2.radius * normal_y;
				let mut id = (vertex_index1 * 256 + vertex_index2) as u32;
				if is_flipped {
					normal_x = -normal_x;
					normal_y = -normal_y;
					id = (vertex_index2 * 256 + vertex_index1) as u32;
				}
				manifold.normal_x = normal_x;
				manifold.normal_y = normal_y;
				push_manifold_point(manifold, 0.5 * (surface1_x + surface2_x), 0.5 * (surface1_y + surface2_y), distance - radius, id);
				return;
			}
		}
	}
	clip_polygons(shape1, shape2, edge1, edge2, is_flipped, speculative_distance, manifold);
}

//==============================================================================
/// 기준 변의 옆면으로 맞은편 변을 잘라 닿음점 두 개를 내기.
//==============================================================================
fn clip_polygons(shape1: &WorldShape, shape2: &WorldShape, edge1: usize, edge2: usize, is_flipped: bool, speculative_distance: f64, manifold: &mut Manifold) {
	let index11 = edge1;
	let index12 = (edge1 + 1) % shape1.count;
	let index21 = edge2;
	let index22 = (edge2 + 1) % shape2.count;
	let vertex11_x = shape1.vertex_x[index11];
	let vertex11_y = shape1.vertex_y[index11];
	let vertex12_x = shape1.vertex_x[index12];
	let vertex12_y = shape1.vertex_y[index12];
	let vertex21_x = shape2.vertex_x[index21];
	let vertex21_y = shape2.vertex_y[index21];
	let vertex22_x = shape2.vertex_x[index22];
	let vertex22_y = shape2.vertex_y[index22];
	let normal_x = shape1.normal_x[index11];
	let normal_y = shape1.normal_y[index11];
	let tangent_x = -normal_y;
	let tangent_y = normal_x;

	let lower1 = 0.0;
	let upper1 = (vertex12_x - vertex11_x) * tangent_x + (vertex12_y - vertex11_y) * tangent_y;
	let upper2 = (vertex21_x - vertex11_x) * tangent_x + (vertex21_y - vertex11_y) * tangent_y;
	let lower2 = (vertex22_x - vertex11_x) * tangent_x + (vertex22_y - vertex11_y) * tangent_y;
	let span = upper2 - lower2;

	let mut lower_x = vertex22_x;
	let mut lower_y = vertex22_y;
	if lower2 < lower1 && span > NORMALIZE_EPSILON {
		let fraction = clamp((lower1 - lower2) / span, 0.0, 1.0);
		lower_x = vertex22_x + fraction * (vertex21_x - vertex22_x);
		lower_y = vertex22_y + fraction * (vertex21_y - vertex22_y);
	}
	let mut upper_x = vertex21_x;
	let mut upper_y = vertex21_y;
	if upper2 > upper1 && span > NORMALIZE_EPSILON {
		let fraction = clamp((upper1 - lower2) / span, 0.0, 1.0);
		upper_x = vertex22_x + fraction * (vertex21_x - vertex22_x);
		upper_y = vertex22_y + fraction * (vertex21_y - vertex22_y);
	}
	let separation_lower = (lower_x - vertex11_x) * normal_x + (lower_y - vertex11_y) * normal_y;
	let separation_upper = (upper_x - vertex11_x) * normal_x + (upper_y - vertex11_y) * normal_y;
	// 두 겉면의 가운데로 옮긴다.
	let shift_lower = 0.5 * (shape1.radius - shape2.radius - separation_lower);
	let shift_upper = 0.5 * (shape1.radius - shape2.radius - separation_upper);
	lower_x += shift_lower * normal_x;
	lower_y += shift_lower * normal_y;
	upper_x += shift_upper * normal_x;
	upper_y += shift_upper * normal_y;
	let radius = shape1.radius + shape2.radius;

	if !is_flipped {
		manifold.normal_x = normal_x;
		manifold.normal_y = normal_y;
		if separation_lower - radius <= speculative_distance {
			push_manifold_point(manifold, lower_x, lower_y, separation_lower - radius, (index11 * 256 + index22) as u32);
		}
		if separation_upper - radius <= speculative_distance {
			push_manifold_point(manifold, upper_x, upper_y, separation_upper - radius, (index12 * 256 + index21) as u32);
		}
	}
	else {
		manifold.normal_x = -normal_x;
		manifold.normal_y = -normal_y;
		if separation_upper - radius <= speculative_distance {
			push_manifold_point(manifold, upper_x, upper_y, separation_upper - radius, (index21 * 256 + index12) as u32);
		}
		if separation_lower - radius <= speculative_distance {
			push_manifold_point(manifold, lower_x, lower_y, separation_lower - radius, (index22 * 256 + index11) as u32);
		}
	}
}

//==============================================================================
/// 광선이 월드 모양에 맞는지. (맞으면 [몫, 맞은 점 x, y, 법선 x, y], 시작점이 안에 있으면 맞지 않은 것으로 친다)
//==============================================================================
pub(crate) fn ray_cast_world_shape(shape: &WorldShape, origin_x: f64, origin_y: f64, translation_x: f64, translation_y: f64, maximum_fraction: f64) -> Option<[f64; 5]> {
	if shape.shape_type == ShapeType2D::Circle {
		return ray_cast_circle(shape.center_x, shape.center_y, shape.radius, origin_x, origin_y, translation_x, translation_y, maximum_fraction);
	}
	if shape.radius > 0.0 {
		if test_point_in_world_shape(shape, origin_x, origin_y) {
			return None;
		}
		let mut best_result = None;
		let mut best_fraction = maximum_fraction;
		for index in 0..shape.count {
			if let Some(circle_result) = ray_cast_circle(shape.vertex_x[index], shape.vertex_y[index], shape.radius, origin_x, origin_y, translation_x, translation_y, best_fraction) {
				if circle_result[0] <= best_fraction {
					best_fraction = circle_result[0];
					best_result = Some(circle_result);
				}
			}
			let next_index = (index + 1) % shape.count;
			let normal_x = shape.normal_x[index];
			let normal_y = shape.normal_y[index];
			let edge_start_x = shape.vertex_x[index] + shape.radius * normal_x;
			let edge_start_y = shape.vertex_y[index] + shape.radius * normal_y;
			let edge_end_x = shape.vertex_x[next_index] + shape.radius * normal_x;
			let edge_end_y = shape.vertex_y[next_index] + shape.radius * normal_y;
			if let Some(edge_result) = ray_cast_one_sided_edge(edge_start_x, edge_start_y, edge_end_x, edge_end_y, normal_x, normal_y, origin_x, origin_y, translation_x, translation_y, best_fraction) {
				if edge_result[0] <= best_fraction {
					best_fraction = edge_result[0];
					best_result = Some(edge_result);
				}
			}
		}
		return best_result;
	}
	if shape.count == 2 {
		return ray_cast_segment(shape.vertex_x[0], shape.vertex_y[0], shape.vertex_x[1], shape.vertex_y[1], origin_x, origin_y, translation_x, translation_y, maximum_fraction);
	}

	// 뾰족한 다각형. (Cyrus-Beck 자르기)
	let mut lower = 0.0;
	let mut upper = maximum_fraction;
	let mut hit_index = None;
	for index in 0..shape.count {
		let numerator = shape.normal_x[index] * (shape.vertex_x[index] - origin_x) + shape.normal_y[index] * (shape.vertex_y[index] - origin_y);
		let denominator = shape.normal_x[index] * translation_x + shape.normal_y[index] * translation_y;
		if denominator == 0.0 {
			if numerator < 0.0 {
				return None;
			}
		}
		else if denominator < 0.0 && numerator < lower * denominator {
			lower = numerator / denominator;
			hit_index = Some(index);
		}
		else if denominator > 0.0 && numerator < upper * denominator {
			upper = numerator / denominator;
		}
		if upper < lower {
			return None;
		}
	}
	let hit_index = hit_index?;
	Some([lower, origin_x + lower * translation_x, origin_y + lower * translation_y, shape.normal_x[hit_index], shape.normal_y[hit_index]])
}

//==============================================================================
/// 광선과 원.
//==============================================================================
fn ray_cast_circle(center_x: f64, center_y: f64, radius: f64, origin_x: f64, origin_y: f64, translation_x: f64, translation_y: f64, maximum_fraction: f64) -> Option<[f64; 5]> {
	let offset_x = origin_x - center_x;
	let offset_y = origin_y - center_y;
	let length = (translation_x * translation_x + translation_y * translation_y).sqrt();
	if length <= NORMALIZE_EPSILON {
		return None;
	}
	let direction_x = translation_x / length;
	let direction_y = translation_y / length;
	// 광선 위에서 원 중심에 가장 가까운 점.
	let closest_distance = -(offset_x * direction_x + offset_y * direction_y);
	let closest_x = offset_x + closest_distance * direction_x;
	let closest_y = offset_y + closest_distance * direction_y;
	let closest_squared = closest_x * closest_x + closest_y * closest_y;
	let radius_squared = radius * radius;
	if closest_squared > radius_squared {
		return None;
	}
	let half_chord = (radius_squared - closest_squared).sqrt();
	let hit_distance = closest_distance - half_chord;
	if hit_distance < 0.0 || hit_distance > maximum_fraction * length {
		return None;
	}
	let hit_x = origin_x + hit_distance * direction_x;
	let hit_y = origin_y + hit_distance * direction_y;
	let normal_x = (hit_x - center_x) / radius;
	let normal_y = (hit_y - center_y) / radius;
	Some([hit_distance / length, hit_x, hit_y, normal_x, normal_y])
}

//==============================================================================
/// 광선과 양면 선분. (법선은 광선 쪽)
//==============================================================================
fn ray_cast_segment(point1_x: f64, point1_y: f64, point2_x: f64, point2_y: f64, origin_x: f64, origin_y: f64, translation_x: f64, translation_y: f64, maximum_fraction: f64) -> Option<[f64; 5]> {
	let edge_x = point2_x - point1_x;
	let edge_y = point2_y - point1_y;
	let edge_length = (edge_x * edge_x + edge_y * edge_y).sqrt();
	if edge_length <= NORMALIZE_EPSILON {
		return None;
	}
	let mut normal_x = edge_y / edge_length;
	let mut normal_y = -edge_x / edge_length;
	let numerator = normal_x * (point1_x - origin_x) + normal_y * (point1_y - origin_y);
	let denominator = normal_x * translation_x + normal_y * translation_y;
	if denominator == 0.0 {
		return None;
	}
	let fraction = numerator / denominator;
	if fraction < 0.0 || fraction > maximum_fraction {
		return None;
	}
	let hit_x = origin_x + fraction * translation_x;
	let hit_y = origin_y + fraction * translation_y;
	let edge_fraction = ((hit_x - point1_x) * edge_x + (hit_y - point1_y) * edge_y) / (edge_length * edge_length);
	if !(0.0..=1.0).contains(&edge_fraction) {
		return None;
	}
	if numerator > 0.0 {
		normal_x = -normal_x;
		normal_y = -normal_y;
	}
	Some([fraction, hit_x, hit_y, normal_x, normal_y])
}

//==============================================================================
/// 광선과 한쪽 면 변. (normal 쪽 밖에서 들어오는 광선만)
//==============================================================================
fn ray_cast_one_sided_edge(start_x: f64, start_y: f64, end_x: f64, end_y: f64, normal_x: f64, normal_y: f64, origin_x: f64, origin_y: f64, translation_x: f64, translation_y: f64, maximum_fraction: f64) -> Option<[f64; 5]> {
	let numerator = normal_x * (start_x - origin_x) + normal_y * (start_y - origin_y);
	let denominator = normal_x * translation_x + normal_y * translation_y;
	if denominator >= 0.0 || numerator > 0.0 {
		return None;
	}
	let fraction = numerator / denominator;
	if fraction < 0.0 || fraction > maximum_fraction {
		return None;
	}
	let hit_x = origin_x + fraction * translation_x;
	let hit_y = origin_y + fraction * translation_y;
	let edge_x = end_x - start_x;
	let edge_y = end_y - start_y;
	let edge_length_squared = edge_x * edge_x + edge_y * edge_y;
	let edge_fraction = ((hit_x - start_x) * edge_x + (hit_y - start_y) * edge_y) / edge_length_squared;
	if !(0.0..=1.0).contains(&edge_fraction) {
		return None;
	}
	Some([fraction, hit_x, hit_y, normal_x, normal_y])
}

//==============================================================================
/// 점이 월드 모양 안에 있는지. (경계 위도 안)
//==============================================================================
pub(crate) fn test_point_in_world_shape(shape: &WorldShape, point_x: f64, point_y: f64) -> bool {
	if shape.shape_type == ShapeType2D::Circle {
		let difference_x = point_x - shape.center_x;
		let difference_y = point_y - shape.center_y;
		return difference_x * difference_x + difference_y * difference_y <= shape.radius * shape.radius;
	}
	if shape.count >= 3 {
		let mut maximum_separation = f64::NEG_INFINITY;
		for index in 0..shape.count {
			let separation = shape.normal_x[index] * (point_x - shape.vertex_x[index]) + shape.normal_y[index] * (point_y - shape.vertex_y[index]);
			maximum_separation = maximum_separation.max(separation);
		}
		if maximum_separation <= 0.0 {
			return true;
		}
		if shape.radius <= 0.0 {
			return false;
		}
	}
	else if shape.radius <= 0.0 {
		return false;
	}
	// 둥근 겉면. (가장 가까운 변까지의 거리가 반지름 안)
	let radius_squared = shape.radius * shape.radius;
	for index in 0..shape.count {
		let next_index = (index + 1) % shape.count;
		let (_, _, distance_squared) = compute_segment_distance(shape.vertex_x[index], shape.vertex_y[index], shape.vertex_x[next_index], shape.vertex_y[next_index], point_x, point_y, point_x, point_y);
		if distance_squared <= radius_squared {
			return true;
		}
	}
	false
}
