//==============================================================================
//! 회전된 바운딩 박스. (원본 base/obb.js)
//! - 원본과 같이 2D 다. 꼭짓점 4 개(원본 이름은 edges)를 차례로 이은 볼록 사각형이다.
//! - contains 는 원본과 같은 반직선 교차 판정이다. 왼쪽, 위 변 위의 점은 안이고 오른쪽, 아래 변 위의 점은 밖이다.
//! - overlaps 는 원본과 같은 분리축 정리(SAT)다. 두 사각형의 변 법선마다 양쪽 꼭짓점(get_edges)을 투영하고, 맞닿기만 해도 겹친 것으로 친다. (< 비교)
//! - 원본 set_edges 는 null, 배열 아님, 길이 다름을 예외로 막는다. 러스트 판은 [Vector2; 4] 타입이 그 일을 한다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
/// 회전된 바운딩 박스. (2D, 꼭짓점 4 개를 차례로 이은 볼록 사각형)
///
/// # 예제
/// 점 판정과 겹침 판정
/// ```rust
/// use vanilla::{Vector2, OBB};
///
/// let mut square = OBB::new();
/// square.set_edges([Vector2::create(0.0, 0.0), Vector2::create(10.0, 0.0), Vector2::create(10.0, 10.0), Vector2::create(0.0, 10.0)]);
/// let mut diamond = OBB::new();
/// diamond.set_edges([Vector2::create(5.0, -2.0), Vector2::create(12.0, 5.0), Vector2::create(5.0, 12.0), Vector2::create(-2.0, 5.0)]);
///
/// assert!(square.contains(Vector2::create(5.0, 5.0)));
/// assert!(!square.contains(Vector2::create(11.0, 5.0)));
/// assert!(square.overlaps(&diamond));
/// ```
//==============================================================================
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OBB {
	/// 꼭짓점 4 개. (원본 이름 그대로 edges)
	edges: [Vector2; 4],
}

impl OBB {
	//==============================================================================
	/// 생성. (꼭짓점은 모두 (0, 0))
	///
	/// # 돌려줌
	/// 꼭짓점 4 개가 모두 (0, 0) 인 OBB.
	//==============================================================================
	pub fn new() -> OBB {
		OBB { edges: [Vector2::zero(); 4] }
	}

	//==============================================================================
	/// 점이 안에 있는지. (반직선 교차 판정)
	///
	/// # 인자
	/// - `view_position`: 판정할 점. (꼭짓점과 같은 좌표계)
	///
	/// # 돌려줌
	/// 안에 있으면 true. (축 정렬 사각형이면 왼쪽, 위 변 위의 점은 안, 오른쪽, 아래 변 위의 점은 밖)
	///
	/// # 예제
	/// 변 위의 점
	/// ```rust
	/// use vanilla::{Vector2, OBB};
	///
	/// let mut square = OBB::new();
	/// square.set_edges([Vector2::create(0.0, 0.0), Vector2::create(10.0, 0.0), Vector2::create(10.0, 10.0), Vector2::create(0.0, 10.0)]);
	/// assert!(square.contains(Vector2::create(0.0, 5.0)));
	/// assert!(!square.contains(Vector2::create(10.0, 5.0)));
	/// assert!(!square.contains(Vector2::create(5.0, 10.0)));
	/// ```
	//==============================================================================
	pub fn contains(&self, view_position: Vector2) -> bool {
		let edges = self.get_edges();
		let mut is_inside = false;
		let mut previous_index = edges.len() - 1;
		for (current_index, current_point) in edges.iter().enumerate() {
			let previous_point = edges[previous_index];
			let current_x = current_point.x;
			let current_y = current_point.y;
			let previous_x = previous_point.x;
			let previous_y = previous_point.y;
			let intersect = ((current_y > view_position.y) != (previous_y > view_position.y)) && (view_position.x < (previous_x - current_x) * (view_position.y - current_y) / (previous_y - current_y) + current_x);
			if intersect {
				is_inside = !is_inside;
			}
			previous_index = current_index;
		}
		is_inside
	}

	//==============================================================================
	/// 다른 OBB 와 겹치는지. (분리축 정리, 두 사각형의 모든 변 법선에 투영해 본다)
	///
	/// # 인자
	/// - `other`: 겹침을 볼 OBB.
	///
	/// # 돌려줌
	/// 겹치거나 맞닿으면 true. 두 사각형의 변 법선 가운데 하나라도 분리축이면 false.
	///
	/// # 예제
	/// 맞닿은 두 사각형과 떨어진 사각형
	/// ```rust
	/// use vanilla::{Vector2, OBB};
	///
	/// let mut left = OBB::new();
	/// left.set_edges([Vector2::create(0.0, 0.0), Vector2::create(10.0, 0.0), Vector2::create(10.0, 10.0), Vector2::create(0.0, 10.0)]);
	/// let mut right = OBB::new();
	/// right.set_edges([Vector2::create(10.0, 0.0), Vector2::create(20.0, 0.0), Vector2::create(20.0, 10.0), Vector2::create(10.0, 10.0)]);
	/// let mut far = OBB::new();
	/// far.set_edges([Vector2::create(30.0, 0.0), Vector2::create(40.0, 0.0), Vector2::create(40.0, 10.0), Vector2::create(30.0, 10.0)]);
	/// assert!(left.overlaps(&right));
	/// assert!(!left.overlaps(&far));
	/// ```
	//==============================================================================
	pub fn overlaps(&self, other: &OBB) -> bool {
		let edges = self.get_edges();
		let other_edges = other.get_edges();
		let polygons = [edges, other_edges];
		for polygon in polygons.iter() {
			for (point_index, first_point) in polygon.iter().enumerate() {
				let second_point = polygon[(point_index + 1) % polygon.len()];
				let edge_x = second_point.x - first_point.x;
				let edge_y = second_point.y - first_point.y;
				let axis_x = -edge_y;
				let axis_y = edge_x;
				let (minimum, maximum) = project_onto_axis(edges, axis_x, axis_y);
				let (other_minimum, other_maximum) = project_onto_axis(other_edges, axis_x, axis_y);
				if maximum < other_minimum || other_maximum < minimum {
					return false;
				}
			}
		}
		true
	}

	//==============================================================================
	/// 꼭짓점 목록 설정.
	///
	/// # 인자
	/// - `edges`: 꼭짓점 4 개. (차례로 이으면 볼록 사각형이 되는 순서)
	//==============================================================================
	pub fn set_edges(&mut self, edges: [Vector2; 4]) {
		self.edges = edges;
	}

	//==============================================================================
	/// 꼭짓점 목록 반환.
	///
	/// # 돌려줌
	/// 꼭짓점 4 개의 읽기 참조.
	//==============================================================================
	pub fn get_edges(&self) -> &[Vector2; 4] {
		&self.edges
	}
}

//==============================================================================
/// 꼭짓점들을 축에 투영한 최소, 최대값. (원본 overlaps 안의 투영 반복을 떼어 낸 것)
//==============================================================================
fn project_onto_axis(points: &[Vector2; 4], axis_x: f32, axis_y: f32) -> (f32, f32) {
	let mut minimum = f32::INFINITY;
	let mut maximum = f32::NEG_INFINITY;
	for point in points.iter() {
		let projection = (point.x * axis_x) + (point.y * axis_y);
		if projection < minimum {
			minimum = projection;
		}
		if projection > maximum {
			maximum = projection;
		}
	}
	(minimum, maximum)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 점 목록으로 OBB 를 만든다.
	//==============================================================================
	fn create_obb(points: [(f32, f32); 4]) -> OBB {
		let mut obb = OBB::new();
		obb.set_edges(points.map(|(x, y)| Vector2::create(x, y)));
		obb
	}

	//==============================================================================
	/// contains 가 원본과 같은지. (node 로 돌린 원본 결과)
	//==============================================================================
	#[test]
	fn contains_matches_original() {
		let square = create_obb([(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
		let square_points = [(5.0, 5.0), (0.0, 0.0), (10.0, 5.0), (11.0, 5.0), (-0.1, 5.0), (5.0, 10.0), (9.999, 9.999)];
		let square_expected = [true, true, false, false, false, false, true];
		for ((x, y), expected) in square_points.iter().zip(square_expected.iter()) {
			assert_eq!(square.contains(Vector2::create(*x, *y)), *expected, "square ({}, {})", x, y);
		}

		let diamond = create_obb([(5.0, -2.0), (12.0, 5.0), (5.0, 12.0), (-2.0, 5.0)]);
		let diamond_points = [(5.0, 5.0), (0.0, 0.0), (1.0, 1.0), (11.0, 5.0), (5.0, -1.0)];
		let diamond_expected = [true, false, false, true, true];
		for ((x, y), expected) in diamond_points.iter().zip(diamond_expected.iter()) {
			assert_eq!(diamond.contains(Vector2::create(*x, *y)), *expected, "diamond ({}, {})", x, y);
		}
	}

	//==============================================================================
	/// overlaps: 겹침, 떨어짐, 자기 자신, 맞닿음, 축 정렬 상자로는 겹쳐 보이지만 대각 변 축에서 갈리는 경우.
	//==============================================================================
	#[test]
	fn overlaps_separating_axis() {
		let square = create_obb([(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
		let diamond = create_obb([(5.0, -2.0), (12.0, 5.0), (5.0, 12.0), (-2.0, 5.0)]);
		let far = create_obb([(20.0, 20.0), (30.0, 20.0), (30.0, 30.0), (20.0, 30.0)]);
		let touching = create_obb([(10.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0)]);
		assert!(square.overlaps(&diamond));
		assert!(diamond.overlaps(&square));
		assert!(!square.overlaps(&far));
		assert!(!far.overlaps(&square));
		assert!(square.overlaps(&square));
		assert!(square.overlaps(&touching));

		// 원점 가운데 반지름 1 마름모(x + y <= 1)와 0.6 ~ 1.6 정사각형(x + y >= 1.2): 축 정렬 범위는 겹치지만 마름모 변의 법선에서 갈린다.
		let unit_diamond = create_obb([(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)]);
		let corner = create_obb([(0.6, 0.6), (1.6, 0.6), (1.6, 1.6), (0.6, 1.6)]);
		assert!(!unit_diamond.overlaps(&corner));
		assert!(!corner.overlaps(&unit_diamond));
	}
}
