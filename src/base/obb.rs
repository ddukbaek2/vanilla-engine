//==============================================================================
// 회전된 바운딩 박스. (원본 base/obb.js)
// - 원본과 같이 2D 다. 꼭짓점 4 개(원본 이름은 edges)를 차례로 이은 볼록 사각형이다.
// - contains 는 원본과 같은 반직선 교차 판정이다. 왼쪽, 위 변 위의 점은 안이고 오른쪽, 아래 변 위의 점은 밖이다.
// - overlaps 는 원본과 다르다. 원본은 상대 꼭짓점을 other.length 로 돌아서(OBB 에는 length 가 없다) 상대 투영 범위가
//   비고, 그래서 언제나 false 를 돌려준다. (node 로 돌려 확인함) 러스트 판은 원본이 뜻한 분리축 정리(SAT)대로
//   상대의 getEdges() 를 투영한다. 맞닿기만 해도 겹친 것으로 친다. (원본과 같은 < 비교)
// - 원본 setEdges 는 null, 배열 아님, 길이 다름을 예외로 막는다. 러스트 판은 [Vector2; 4] 타입이 그 일을 한다.
//==============================================================================
use crate::base::vector2::Vector2;

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OBB {
	// 꼭짓점 4 개. (원본 이름 그대로 edges)
	edges: [Vector2; 4],
}

impl OBB {
	//==============================================================================
	// 생성. (꼭짓점은 모두 (0, 0))
	//==============================================================================
	pub fn new() -> OBB {
		OBB { edges: [Vector2::zero(); 4] }
	}

	//==============================================================================
	// 점이 안에 있는지. (반직선 교차 판정)
	//==============================================================================
	pub fn contains(&self, viewPosition: Vector2) -> bool {
		let edges = self.getEdges();
		let mut isInside = false;
		let mut previousIndex = edges.len() - 1;
		for (currentIndex, currentPoint) in edges.iter().enumerate() {
			let previousPoint = edges[previousIndex];
			let currentX = currentPoint.x;
			let currentY = currentPoint.y;
			let previousX = previousPoint.x;
			let previousY = previousPoint.y;
			let intersect = ((currentY > viewPosition.y) != (previousY > viewPosition.y)) && (viewPosition.x < (previousX - currentX) * (viewPosition.y - currentY) / (previousY - currentY) + currentX);
			if intersect {
				isInside = !isInside;
			}
			previousIndex = currentIndex;
		}
		isInside
	}

	//==============================================================================
	// 다른 OBB 와 겹치는지. (분리축 정리, 두 사각형의 모든 변 법선에 투영해 본다)
	//==============================================================================
	pub fn overlaps(&self, other: &OBB) -> bool {
		let edges = self.getEdges();
		let otherEdges = other.getEdges();
		let polygons = [edges, otherEdges];
		for polygon in polygons.iter() {
			for (pointIndex, firstPoint) in polygon.iter().enumerate() {
				let secondPoint = polygon[(pointIndex + 1) % polygon.len()];
				let edgeX = secondPoint.x - firstPoint.x;
				let edgeY = secondPoint.y - firstPoint.y;
				let axisX = -edgeY;
				let axisY = edgeX;
				let (minimum, maximum) = projectOntoAxis(edges, axisX, axisY);
				let (otherMinimum, otherMaximum) = projectOntoAxis(otherEdges, axisX, axisY);
				if maximum < otherMinimum || otherMaximum < minimum {
					return false;
				}
			}
		}
		true
	}

	//==============================================================================
	// 꼭짓점 목록 설정.
	//==============================================================================
	pub fn setEdges(&mut self, edges: [Vector2; 4]) {
		self.edges = edges;
	}

	//==============================================================================
	// 꼭짓점 목록 반환.
	//==============================================================================
	pub fn getEdges(&self) -> &[Vector2; 4] {
		&self.edges
	}
}

//==============================================================================
// 꼭짓점들을 축에 투영한 최소, 최대값. (원본 overlaps 안의 투영 반복을 떼어 낸 것)
//==============================================================================
fn projectOntoAxis(points: &[Vector2; 4], axisX: f32, axisY: f32) -> (f32, f32) {
	let mut minimum = f32::INFINITY;
	let mut maximum = f32::NEG_INFINITY;
	for point in points.iter() {
		let projection = (point.x * axisX) + (point.y * axisY);
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
	// 점 목록으로 OBB 를 만든다.
	//==============================================================================
	fn createObb(points: [(f32, f32); 4]) -> OBB {
		let mut obb = OBB::new();
		obb.setEdges(points.map(|(x, y)| Vector2::create(x, y)));
		obb
	}

	//==============================================================================
	// contains 가 원본과 같은지. (node 로 돌린 원본 결과)
	//==============================================================================
	#[test]
	fn containsMatchesOriginal() {
		let square = createObb([(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
		let squarePoints = [(5.0, 5.0), (0.0, 0.0), (10.0, 5.0), (11.0, 5.0), (-0.1, 5.0), (5.0, 10.0), (9.999, 9.999)];
		let squareExpected = [true, true, false, false, false, false, true];
		for ((x, y), expected) in squarePoints.iter().zip(squareExpected.iter()) {
			assert_eq!(square.contains(Vector2::create(*x, *y)), *expected, "square ({}, {})", x, y);
		}

		let diamond = createObb([(5.0, -2.0), (12.0, 5.0), (5.0, 12.0), (-2.0, 5.0)]);
		let diamondPoints = [(5.0, 5.0), (0.0, 0.0), (1.0, 1.0), (11.0, 5.0), (5.0, -1.0)];
		let diamondExpected = [true, false, false, true, true];
		for ((x, y), expected) in diamondPoints.iter().zip(diamondExpected.iter()) {
			assert_eq!(diamond.contains(Vector2::create(*x, *y)), *expected, "diamond ({}, {})", x, y);
		}
	}

	//==============================================================================
	// overlaps: 겹침, 떨어짐, 자기 자신, 맞닿음, 축 정렬 상자로는 겹쳐 보이지만 대각 변 축에서 갈리는 경우.
	//==============================================================================
	#[test]
	fn overlapsSeparatingAxis() {
		let square = createObb([(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
		let diamond = createObb([(5.0, -2.0), (12.0, 5.0), (5.0, 12.0), (-2.0, 5.0)]);
		let far = createObb([(20.0, 20.0), (30.0, 20.0), (30.0, 30.0), (20.0, 30.0)]);
		let touching = createObb([(10.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0)]);
		assert!(square.overlaps(&diamond));
		assert!(diamond.overlaps(&square));
		assert!(!square.overlaps(&far));
		assert!(!far.overlaps(&square));
		assert!(square.overlaps(&square));
		assert!(square.overlaps(&touching));

		// 원점 가운데 반지름 1 마름모(x + y <= 1)와 0.6 ~ 1.6 정사각형(x + y >= 1.2): 축 정렬 범위는 겹치지만 마름모 변의 법선에서 갈린다.
		let unitDiamond = createObb([(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)]);
		let corner = createObb([(0.6, 0.6), (1.6, 0.6), (1.6, 1.6), (0.6, 1.6)]);
		assert!(!unitDiamond.overlaps(&corner));
		assert!(!corner.overlaps(&unitDiamond));
	}
}
