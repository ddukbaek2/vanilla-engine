//==============================================================================
// 2D 조향 유틸.
// - 원본 misc/steering2d.js 를 옮긴 것이다. "멀면 다가가고, 사거리 안이면 거리를 유지하며 좌우로 선회" 하는 전투 AI 이동처럼
//   자주 다시 만들던 조향 계산을 순수 함수로 모았다. 돌려주는 값은 목표 속도 벡터다.
// - 원본과 다른 점:
//   orbitAtRange 의 strafeRate 기본값(1.7)은 러스트에 기본 인자가 없어 DEFAULT_STRAFE_RATE 상수로 넘긴다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
// 2D 조향 유틸. (정적 함수 묶음)
//==============================================================================
pub struct Steering2D;

impl Steering2D {
	// orbitAtRange 의 기본 선회 파형 각속도. (원본 strafeRate = 1.7)
	pub const DEFAULT_STRAFE_RATE: f32 = 1.7;

	//==============================================================================
	// 목표를 향해 다가가는 속도. (거의 같은 자리면 0)
	//==============================================================================
	pub fn seek(position: Vector2, targetPosition: Vector2, speed: f32) -> Vector2 {
		let differenceX = targetPosition.x - position.x;
		let differenceY = targetPosition.y - position.y;
		let distance = (differenceX * differenceX + differenceY * differenceY).sqrt();
		if distance < 0.000001 {
			return Vector2::zero();
		}
		Vector2::create(differenceX / distance * speed, differenceY / distance * speed)
	}

	//==============================================================================
	// 사거리를 유지하며 선회하는 속도.
	// - 사거리보다 멀면 접근, 가까우면 후퇴 성분을 섞고, 접선 방향으로 sin 파형 좌우 선회를 더해 살아 있는 움직임을 만든다.
	// - elapsedSeconds 는 선회 위상에 쓰는 누적 시간이다.
	//==============================================================================
	pub fn orbitAtRange(position: Vector2, targetPosition: Vector2, preferredRange: f32, speed: f32, elapsedSeconds: f32, strafeRate: f32) -> Vector2 {
		let differenceX = targetPosition.x - position.x;
		let differenceY = targetPosition.y - position.y;
		let distance = (differenceX * differenceX + differenceY * differenceY).sqrt();
		if distance < 0.000001 {
			return Vector2::create(speed, 0.0);
		}
		let forwardX = differenceX / distance;
		let forwardY = differenceY / distance;
		let sideX = -forwardY;
		let sideY = forwardX;

		// 사거리 대비 오차를 -1 ~ 1 로 눌러 접근 / 후퇴 성분을 만든다.
		let rangeError = ((distance - preferredRange) / preferredRange).clamp(-1.0, 1.0);
		let strafeAmount = (elapsedSeconds * strafeRate).sin();
		let desiredX = forwardX * rangeError + sideX * strafeAmount;
		let desiredY = forwardY * rangeError + sideY * strafeAmount;
		let desiredLength = (desiredX * desiredX + desiredY * desiredY).sqrt();
		if desiredLength < 0.000001 {
			return Vector2::zero();
		}
		Vector2::create(desiredX / desiredLength * speed, desiredY / desiredLength * speed)
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 두 벡터가 거의 같은지.
	//==============================================================================
	fn isNear(left: Vector2, right: Vector2) -> bool {
		(left.x - right.x).abs() < 0.0001 && (left.y - right.y).abs() < 0.0001
	}

	//==============================================================================
	// 접근 속도는 목표 쪽으로 speed 길이, 같은 자리면 0.
	//==============================================================================
	#[test]
	fn seekPointsToTarget() {
		let velocity = Steering2D::seek(Vector2::create(0.0, 0.0), Vector2::create(3.0, 4.0), 10.0);
		assert!(isNear(velocity, Vector2::create(6.0, 8.0)));
		assert_eq!(Steering2D::seek(Vector2::create(1.0, 1.0), Vector2::create(1.0, 1.0), 10.0), Vector2::zero());
	}

	//==============================================================================
	// 선회: 멀면 접근, 가까우면 후퇴, 사거리에서 위상 0 이면 멈춤, 위상이 있으면 접선 방향.
	//==============================================================================
	#[test]
	fn orbitBlendsRangeAndStrafe() {
		let targetPosition = Vector2::create(100.0, 0.0);
		let farVelocity = Steering2D::orbitAtRange(Vector2::create(-200.0, 0.0), targetPosition, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert!(isNear(farVelocity, Vector2::create(5.0, 0.0)));
		let nearVelocity = Steering2D::orbitAtRange(Vector2::create(50.0, 0.0), targetPosition, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert!(isNear(nearVelocity, Vector2::create(-5.0, 0.0)));
		let restingVelocity = Steering2D::orbitAtRange(Vector2::create(0.0, 0.0), targetPosition, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert_eq!(restingVelocity, Vector2::zero());
		let strafeSeconds = std::f32::consts::FRAC_PI_2 / Steering2D::DEFAULT_STRAFE_RATE;
		let strafeVelocity = Steering2D::orbitAtRange(Vector2::create(0.0, 0.0), targetPosition, 100.0, 5.0, strafeSeconds, Steering2D::DEFAULT_STRAFE_RATE);
		assert!(isNear(strafeVelocity, Vector2::create(0.0, 5.0)));
		let sameVelocity = Steering2D::orbitAtRange(targetPosition, targetPosition, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert_eq!(sameVelocity, Vector2::create(5.0, 0.0));
	}
}
