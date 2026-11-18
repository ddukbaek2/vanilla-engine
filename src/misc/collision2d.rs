//==============================================================================
// 2D 충돌 유틸.
// - 원본 misc/collision2d.js 를 옮긴 것이다. 원-원 겹침 분리(밀어내기)와 반경 안 최근접 찾기처럼 액션 게임마다 다시 만들던 판정을 모았다.
// - 모두 정적 함수다.
// - 원본과 다른 점:
//   resolveCircleOverlap 은 null 대신 None, { pushA, pushB } 대신 CircleOverlapPush 를 돌려준다.
//   findNearestInRadius 는 대상의 참조를 돌려주고, 고칠 때 쓰도록 번호를 돌려주는 findNearestIndexInRadius 를 더했다.
//   isStaticB 기본값(false)은 러스트에 기본 인자가 없어 늘 넘긴다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
// 원-원 겹침을 풀 이동량. (원본 { pushA, pushB })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleOverlapPush {
	pub pushA: Vector2,
	pub pushB: Vector2,
}

//==============================================================================
// 2D 충돌 유틸. (정적 함수 묶음)
//==============================================================================
pub struct Collision2D;

impl Collision2D {
	//==============================================================================
	// 원-원 겹침 검사. (닿기만 한 것은 겹친 것이 아니다)
	//==============================================================================
	pub fn testCircleOverlap(positionA: Vector2, radiusA: f32, positionB: Vector2, radiusB: f32) -> bool {
		let differenceX = positionB.x - positionA.x;
		let differenceY = positionB.y - positionA.y;
		let radiusSum = radiusA + radiusB;
		differenceX * differenceX + differenceY * differenceY < radiusSum * radiusSum
	}

	//==============================================================================
	// 원-원 겹침 분리. (겹쳐 있으면 서로 절반씩 밀어낼 이동량, 안 겹치면 None)
	// - isStaticB 면 B 는 고정물로 보고 A 만 겹친 깊이 전체를 밀린다.
	// - 두 중심이 거의 같으면 x 축으로 밀어낸다.
	//==============================================================================
	pub fn resolveCircleOverlap(positionA: Vector2, radiusA: f32, positionB: Vector2, radiusB: f32, isStaticB: bool) -> Option<CircleOverlapPush> {
		let differenceX = positionB.x - positionA.x;
		let differenceY = positionB.y - positionA.y;
		let radiusSum = radiusA + radiusB;
		let distanceSquared = differenceX * differenceX + differenceY * differenceY;
		if distanceSquared >= radiusSum * radiusSum {
			return None;
		}
		let distance = distanceSquared.sqrt();
		let mut directionX = 1.0;
		let mut directionY = 0.0;
		if distance > 0.000001 {
			directionX = differenceX / distance;
			directionY = differenceY / distance;
		}
		let overlapDepth = radiusSum - distance;
		if isStaticB {
			return Some(CircleOverlapPush {
				pushA: Vector2::create(-directionX * overlapDepth, -directionY * overlapDepth),
				pushB: Vector2::zero(),
			});
		}
		let halfDepth = overlapDepth * 0.5;
		Some(CircleOverlapPush {
			pushA: Vector2::create(-directionX * halfDepth, -directionY * halfDepth),
			pushB: Vector2::create(directionX * halfDepth, directionY * halfDepth),
		})
	}

	//==============================================================================
	// 반경 안 최근접 대상 번호 찾기. (중심에서 radius 안에 있는 것 가운데 가장 가까운 것, 거리가 같으면 뒤의 것)
	//==============================================================================
	pub fn findNearestIndexInRadius<T>(targetList: &[T], getPositionHandler: impl Fn(&T) -> Vector2, center: Vector2, radius: f32) -> Option<usize> {
		let mut nearestIndex: Option<usize> = None;
		let mut nearestDistanceSquared = radius * radius;
		for (targetIndex, target) in targetList.iter().enumerate() {
			let targetPosition = getPositionHandler(target);
			let differenceX = targetPosition.x - center.x;
			let differenceY = targetPosition.y - center.y;
			let distanceSquared = differenceX * differenceX + differenceY * differenceY;
			if distanceSquared <= nearestDistanceSquared {
				nearestDistanceSquared = distanceSquared;
				nearestIndex = Some(targetIndex);
			}
		}
		nearestIndex
	}

	//==============================================================================
	// 반경 안 최근접 대상 찾기. (투사체 명중 판정처럼)
	//==============================================================================
	pub fn findNearestInRadius<T>(targetList: &[T], getPositionHandler: impl Fn(&T) -> Vector2, center: Vector2, radius: f32) -> Option<&T> {
		let nearestIndex = Collision2D::findNearestIndexInRadius(targetList, getPositionHandler, center, radius)?;
		targetList.get(nearestIndex)
	}

	//==============================================================================
	// 점-원 포함 검사. (경계 위도 안으로 친다)
	//==============================================================================
	pub fn testPointInCircle(point: Vector2, center: Vector2, radius: f32) -> bool {
		let differenceX = point.x - center.x;
		let differenceY = point.y - center.y;
		differenceX * differenceX + differenceY * differenceY <= radius * radius
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 겹침 검사, 분리량, 고정물, 같은 중심.
	//==============================================================================
	#[test]
	fn circleOverlapCases() {
		let positionA = Vector2::create(0.0, 0.0);
		let positionB = Vector2::create(6.0, 0.0);
		assert!(Collision2D::testCircleOverlap(positionA, 4.0, positionB, 4.0));
		assert!(!Collision2D::testCircleOverlap(positionA, 3.0, positionB, 3.0));
		assert!(Collision2D::resolveCircleOverlap(positionA, 3.0, positionB, 3.0, false).is_none());

		let push = Collision2D::resolveCircleOverlap(positionA, 4.0, positionB, 4.0, false).expect("push");
		assert_eq!(push.pushA, Vector2::create(-1.0, 0.0));
		assert_eq!(push.pushB, Vector2::create(1.0, 0.0));
		let staticPush = Collision2D::resolveCircleOverlap(positionA, 4.0, positionB, 4.0, true).expect("push");
		assert_eq!(staticPush.pushA, Vector2::create(-2.0, 0.0));
		assert_eq!(staticPush.pushB, Vector2::zero());
		let samePush = Collision2D::resolveCircleOverlap(positionA, 1.0, positionA, 1.0, false).expect("push");
		assert_eq!(samePush.pushA, Vector2::create(-1.0, 0.0));
		assert_eq!(samePush.pushB, Vector2::create(1.0, 0.0));
	}

	//==============================================================================
	// 최근접 찾기와 점 포함.
	//==============================================================================
	#[test]
	fn nearestAndPointInCircle() {
		let targetList = [Vector2::create(10.0, 0.0), Vector2::create(3.0, 4.0), Vector2::create(0.0, 30.0), Vector2::create(-4.0, 3.0)];
		let nearestIndex = Collision2D::findNearestIndexInRadius(&targetList, |target| *target, Vector2::zero(), 12.0);
		// 거리 5 가 둘이면 뒤의 것. (원본 <= 비교)
		assert_eq!(nearestIndex, Some(3));
		let nearestTarget = Collision2D::findNearestInRadius(&targetList, |target| *target, Vector2::zero(), 12.0);
		assert_eq!(nearestTarget, Some(&Vector2::create(-4.0, 3.0)));
		assert!(Collision2D::findNearestInRadius(&targetList, |target| *target, Vector2::create(100.0, 100.0), 5.0).is_none());
		assert!(Collision2D::testPointInCircle(Vector2::create(3.0, 4.0), Vector2::zero(), 5.0));
		assert!(!Collision2D::testPointInCircle(Vector2::create(3.0, 4.1), Vector2::zero(), 5.0));
	}
}
