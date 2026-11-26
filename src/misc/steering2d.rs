//==============================================================================
//! 2D 조향 유틸.
//! - 원본 misc/steering2d.js 를 옮긴 것이다. "멀면 다가가고, 사거리 안이면 거리를 유지하며 좌우로 선회" 하는 전투 AI 이동처럼
//!   자주 다시 만들던 조향 계산을 순수 함수로 모았다. 돌려주는 값은 목표 속도 벡터다.
//! - 원본과 다른 점:
//!   orbit_at_range 의 strafe_rate 기본값(1.7)은 러스트에 기본 인자가 없어 DEFAULT_STRAFE_RATE 상수로 넘긴다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
/// 2D 조향 유틸. (정적 함수 묶음)
/// - 상태가 없다. 매 프레임 목표 속도를 받아 위치에 time_delta 를 곱해 더한다.
///
/// # 예제
/// 목표 쪽으로 한 걸음
/// ```rust
/// use vanilla::{Steering2D, Vector2};
///
/// let mut position = Vector2::create(0.0, 0.0);
/// let target_position = Vector2::create(30.0, 40.0);
/// let velocity = Steering2D::seek(position, target_position, 10.0);
/// assert!((velocity.x - 6.0).abs() < 0.0001);
/// assert!((velocity.y - 8.0).abs() < 0.0001);
/// let time_delta = 0.5;
/// position = position.add(velocity.multiply_scalar(time_delta));
/// assert!((position.x - 3.0).abs() < 0.0001);
/// ```
//==============================================================================
pub struct Steering2D;

impl Steering2D {
	/// orbit_at_range 의 기본 선회 파형 각속도. (원본 strafeRate = 1.7)
	pub const DEFAULT_STRAFE_RATE: f64 = 1.7;

	//==============================================================================
	/// 목표를 향해 다가가는 속도. (거의 같은 자리면 0)
	///
	/// # 인자
	/// - `position`: 지금 위치.
	/// - `target_position`: 목표 위치.
	/// - `speed`: 속력. (돌려줄 벡터의 길이, 초당 거리)
	///
	/// # 돌려줌
	/// 목표 쪽으로 길이가 speed 인 속도 벡터. 두 위치 사이 거리가 0.000001 보다 작으면 (0, 0).
	//==============================================================================
	pub fn seek(position: Vector2, target_position: Vector2, speed: f64) -> Vector2 {
		let difference_x = target_position.x - position.x;
		let difference_y = target_position.y - position.y;
		let distance = (difference_x * difference_x + difference_y * difference_y).sqrt();
		if distance < 0.000001 {
			return Vector2::zero();
		}
		Vector2::create(difference_x / distance * speed, difference_y / distance * speed)
	}

	//==============================================================================
	/// 사거리를 유지하며 선회하는 속도.
	/// - 사거리보다 멀면 접근, 가까우면 후퇴 성분을 섞고, 접선 방향으로 sin 파형 좌우 선회를 더해 살아 있는 움직임을 만든다.
	/// - elapsed_seconds 는 선회 위상에 쓰는 누적 시간이다.
	///
	/// # 인자
	/// - `position`: 지금 위치.
	/// - `target_position`: 목표 위치.
	/// - `preferred_range`: 유지할 거리. (거리 오차를 이 값으로 나눠 -1 ~ 1 로 자른다)
	/// - `speed`: 속력. (돌려줄 벡터의 길이)
	/// - `elapsed_seconds`: 누적 시간. (초, 선회 파형 sin(elapsed_seconds * strafe_rate) 의 위상)
	/// - `strafe_rate`: 선회 파형 각속도. (라디안/초, 보통 DEFAULT_STRAFE_RATE)
	///
	/// # 돌려줌
	/// 길이가 speed 인 속도 벡터. 두 위치가 거의 같으면 (speed, 0), 사거리에 있고 선회 성분도 0 이면 (0, 0).
	///
	/// # 예제
	/// 멀면 다가가고 가까우면 물러난다
	/// ```rust
	/// use vanilla::{Steering2D, Vector2};
	///
	/// let target_position = Vector2::create(100.0, 0.0);
	/// let far_velocity = Steering2D::orbit_at_range(Vector2::create(-200.0, 0.0), target_position, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
	/// assert!((far_velocity.x - 5.0).abs() < 0.0001);
	/// let near_velocity = Steering2D::orbit_at_range(Vector2::create(50.0, 0.0), target_position, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
	/// assert!((near_velocity.x + 5.0).abs() < 0.0001);
	/// ```
	//==============================================================================
	pub fn orbit_at_range(position: Vector2, target_position: Vector2, preferred_range: f64, speed: f64, elapsed_seconds: f64, strafe_rate: f64) -> Vector2 {
		let difference_x = target_position.x - position.x;
		let difference_y = target_position.y - position.y;
		let distance = (difference_x * difference_x + difference_y * difference_y).sqrt();
		if distance < 0.000001 {
			return Vector2::create(speed, 0.0);
		}
		let forward_x = difference_x / distance;
		let forward_y = difference_y / distance;
		let side_x = -forward_y;
		let side_y = forward_x;

		// 사거리 대비 오차를 -1 ~ 1 로 눌러 접근 / 후퇴 성분을 만든다.
		let range_error = ((distance - preferred_range) / preferred_range).clamp(-1.0, 1.0);
		let strafe_amount = (elapsed_seconds * strafe_rate).sin();
		let desired_x = forward_x * range_error + side_x * strafe_amount;
		let desired_y = forward_y * range_error + side_y * strafe_amount;
		let desired_length = (desired_x * desired_x + desired_y * desired_y).sqrt();
		if desired_length < 0.000001 {
			return Vector2::zero();
		}
		Vector2::create(desired_x / desired_length * speed, desired_y / desired_length * speed)
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 두 벡터가 거의 같은지.
	//==============================================================================
	fn is_near(left: Vector2, right: Vector2) -> bool {
		(left.x - right.x).abs() < 0.0001 && (left.y - right.y).abs() < 0.0001
	}

	//==============================================================================
	/// 접근 속도는 목표 쪽으로 speed 길이, 같은 자리면 0.
	//==============================================================================
	#[test]
	fn seek_points_to_target() {
		let velocity = Steering2D::seek(Vector2::create(0.0, 0.0), Vector2::create(3.0, 4.0), 10.0);
		assert!(is_near(velocity, Vector2::create(6.0, 8.0)));
		assert_eq!(Steering2D::seek(Vector2::create(1.0, 1.0), Vector2::create(1.0, 1.0), 10.0), Vector2::zero());
	}

	//==============================================================================
	/// 선회: 멀면 접근, 가까우면 후퇴, 사거리에서 위상 0 이면 멈춤, 위상이 있으면 접선 방향.
	//==============================================================================
	#[test]
	fn orbit_blends_range_and_strafe() {
		let target_position = Vector2::create(100.0, 0.0);
		let far_velocity = Steering2D::orbit_at_range(Vector2::create(-200.0, 0.0), target_position, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert!(is_near(far_velocity, Vector2::create(5.0, 0.0)));
		let near_velocity = Steering2D::orbit_at_range(Vector2::create(50.0, 0.0), target_position, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert!(is_near(near_velocity, Vector2::create(-5.0, 0.0)));
		let resting_velocity = Steering2D::orbit_at_range(Vector2::create(0.0, 0.0), target_position, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert_eq!(resting_velocity, Vector2::zero());
		let strafe_seconds = std::f64::consts::FRAC_PI_2 / Steering2D::DEFAULT_STRAFE_RATE;
		let strafe_velocity = Steering2D::orbit_at_range(Vector2::create(0.0, 0.0), target_position, 100.0, 5.0, strafe_seconds, Steering2D::DEFAULT_STRAFE_RATE);
		assert!(is_near(strafe_velocity, Vector2::create(0.0, 5.0)));
		let same_velocity = Steering2D::orbit_at_range(target_position, target_position, 100.0, 5.0, 0.0, Steering2D::DEFAULT_STRAFE_RATE);
		assert_eq!(same_velocity, Vector2::create(5.0, 0.0));
	}
}
