//==============================================================================
//! 2D 물리의 결정론 수학.
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/physicsmath2d.js 와 같은 셈, 같은 차례라 두 엔진의 결과가 비트까지 같다.
//! - 코사인, 사인, 아크탄젠트를 더하기, 빼기, 곱하기, 나누기, 제곱근만으로 짠다. (모두 IEEE 754 가 정확히 반올림하라고 정한 셈)
//!   러스트의 f64::sin 은 운영체제의 수학 라이브러리를 불러 플랫폼마다, 그리고 브라우저의 Math.sin 과 끝자리가 달라 물리가 갈라진다.
//!   Box2D 도 같은 까닭으로 b2ComputeCosSin, b2Atan2 를 직접 짰다. (box2d.org/posts/2024/08/determinism)
//==============================================================================
use std::f64::consts::PI;

/// 2 파이.
const TWO_PI: f64 = 2.0 * PI;
/// 파이 / 2.
const HALF_PI: f64 = 0.5 * PI;
/// 파이 / 4.
const QUARTER_PI: f64 = 0.25 * PI;
/// tan(파이 / 8). (아크탄젠트를 이 값 위에서는 파이 / 4 쪽으로 옮겨 줄인다)
const TANGENT_EIGHTH_PI: f64 = 0.414213562373095;

//==============================================================================
/// 2D 물리의 결정론 수학. (정적 함수 묶음)
///
/// # 예제
/// 결정론 코사인, 사인, 아크탄젠트
/// ```rust
/// use vanilla::experimental::physics2d::PhysicsMath2D;
///
/// let rotation = PhysicsMath2D::compute_rotation(1.0);
/// assert!((rotation.cosine - 1.0f64.cos()).abs() < 1e-14);
/// assert!((rotation.sine - 1.0f64.sin()).abs() < 1e-14);
/// assert!((PhysicsMath2D::compute_atan2(rotation.sine, rotation.cosine) - 1.0).abs() < 1e-14);
/// assert!((PhysicsMath2D::unwind_angle(7.0) - (7.0 - 2.0 * std::f64::consts::PI)).abs() < 1e-14);
/// ```
//==============================================================================
pub struct PhysicsMath2D;

impl PhysicsMath2D {
	//==============================================================================
	/// 각도의 회전 반환. (결정론 코사인과 사인)
	///
	/// # 인자
	/// - `angle`: 각도. (라디안)
	///
	/// # 돌려줌
	/// 단위 길이로 맞춘 (코사인, 사인).
	//==============================================================================
	pub fn compute_rotation(angle: f64) -> Rotation2D {
		let turns = (angle / TWO_PI + 0.5).floor();
		let reduced = angle - turns * TWO_PI;
		let quadrant = (reduced / HALF_PI + 0.5).floor();
		let remainder = reduced - quadrant * HALF_PI;
		let remainder_squared = remainder * remainder;
		let sine_remainder = remainder * (1.0 + remainder_squared * (-1.0 / 6.0 + remainder_squared * (1.0 / 120.0 + remainder_squared * (-1.0 / 5040.0 + remainder_squared * (1.0 / 362880.0 + remainder_squared * (-1.0 / 39916800.0 + remainder_squared * (1.0 / 6227020800.0 + remainder_squared * (-1.0 / 1307674368000.0))))))));
		let cosine_remainder = 1.0 + remainder_squared * (-1.0 / 2.0 + remainder_squared * (1.0 / 24.0 + remainder_squared * (-1.0 / 720.0 + remainder_squared * (1.0 / 40320.0 + remainder_squared * (-1.0 / 3628800.0 + remainder_squared * (1.0 / 479001600.0 + remainder_squared * (-1.0 / 87178291200.0 + remainder_squared * (1.0 / 20922789888000.0))))))));
		let quadrant_index = (quadrant as i64).rem_euclid(4);
		let (cosine, sine) = match quadrant_index {
			1 => (-sine_remainder, cosine_remainder),
			2 => (-cosine_remainder, -sine_remainder),
			3 => (sine_remainder, -cosine_remainder),
			_ => (cosine_remainder, sine_remainder),
		};
		let magnitude = (cosine * cosine + sine * sine).sqrt();
		Rotation2D::create(cosine / magnitude, sine / magnitude)
	}

	//==============================================================================
	/// 결정론 코사인.
	///
	/// # 인자
	/// - `angle`: 각도. (라디안)
	///
	/// # 돌려줌
	/// 코사인.
	//==============================================================================
	pub fn compute_cosine(angle: f64) -> f64 {
		PhysicsMath2D::compute_rotation(angle).cosine
	}

	//==============================================================================
	/// 결정론 사인.
	///
	/// # 인자
	/// - `angle`: 각도. (라디안)
	///
	/// # 돌려줌
	/// 사인.
	//==============================================================================
	pub fn compute_sine(angle: f64) -> f64 {
		PhysicsMath2D::compute_rotation(angle).sine
	}

	//==============================================================================
	/// 결정론 아크탄젠트. (atan2(y, x), 둘 다 0 이면 0)
	/// - 0 ~ 1 로 접은 뒤 파이 / 8 위는 파이 / 4 쪽으로 옮기고, 반각 공식 두 번으로 0.1 아래까지 줄여 급수로 셈한다.
	///
	/// # 인자
	/// - `y`: 세로 값.
	/// - `x`: 가로 값.
	///
	/// # 돌려줌
	/// 각도. (라디안, -파이 ~ 파이)
	//==============================================================================
	pub fn compute_atan2(y: f64, x: f64) -> f64 {
		if x == 0.0 && y == 0.0 {
			return 0.0;
		}
		let absolute_x = x.abs();
		let absolute_y = y.abs();
		let maximum = absolute_x.max(absolute_y);
		let minimum = absolute_x.min(absolute_y);
		let ratio = minimum / maximum;
		let mut reduced = ratio;
		let mut offset = 0.0;
		if ratio > TANGENT_EIGHTH_PI {
			reduced = (ratio - 1.0) / (ratio + 1.0);
			offset = QUARTER_PI;
		}
		let half = reduced / (1.0 + (1.0 + reduced * reduced).sqrt());
		let quarter = half / (1.0 + (1.0 + half * half).sqrt());
		let quarter_squared = quarter * quarter;
		let series = quarter * (1.0 + quarter_squared * (-1.0 / 3.0 + quarter_squared * (1.0 / 5.0 + quarter_squared * (-1.0 / 7.0 + quarter_squared * (1.0 / 9.0 + quarter_squared * (-1.0 / 11.0 + quarter_squared * (1.0 / 13.0 + quarter_squared * (-1.0 / 15.0))))))));
		let mut result = offset + 4.0 * series;
		if absolute_y > absolute_x {
			result = HALF_PI - result;
		}
		if x < 0.0 {
			result = PI - result;
		}
		if y < 0.0 {
			result = -result;
		}
		result
	}

	//==============================================================================
	/// 각도를 -파이 ~ 파이 로 접기.
	///
	/// # 인자
	/// - `angle`: 각도. (라디안)
	///
	/// # 돌려줌
	/// 같은 방향의 -파이 이상 파이 미만 각도.
	//==============================================================================
	pub fn unwind_angle(angle: f64) -> f64 {
		if (-PI..PI).contains(&angle) {
			return angle;
		}
		angle - TWO_PI * ((angle + PI) / TWO_PI).floor()
	}
}

//==============================================================================
/// 2D 회전. (코사인과 사인)
/// - 물리는 각도 대신 이 쌍을 들고, 걸음마다 각속도로 돌린 뒤 길이를 1 로 맞춘다. (Box2D 3 의 b2Rot 과 같은 꼴, 삼각 함수를 부르지 않는다)
///
/// # 예제
/// 회전을 만들고 각도로 되돌리기
/// ```rust
/// use vanilla::experimental::physics2d::Rotation2D;
///
/// let rotation = Rotation2D::from_angle(0.5);
/// assert!((rotation.get_angle() - 0.5).abs() < 1e-15);
/// let doubled = rotation.multiply(&rotation);
/// assert!((doubled.get_angle() - 1.0).abs() < 1e-14);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation2D {
	/// 코사인.
	pub cosine: f64,
	/// 사인.
	pub sine: f64,
}

impl Default for Rotation2D {
	//==============================================================================
	/// 기본값. (회전 없음)
	//==============================================================================
	fn default() -> Rotation2D {
		Rotation2D::new()
	}
}

impl Rotation2D {
	//==============================================================================
	/// 생성. (회전 없음)
	///
	/// # 돌려줌
	/// (1, 0).
	//==============================================================================
	pub const fn new() -> Rotation2D {
		Rotation2D {
			cosine: 1.0,
			sine: 0.0,
		}
	}

	//==============================================================================
	/// 코사인과 사인으로 만들기. (길이를 맞추지 않는다)
	///
	/// # 인자
	/// - `cosine`: 코사인.
	/// - `sine`: 사인.
	///
	/// # 돌려줌
	/// 새 회전.
	//==============================================================================
	pub const fn create(cosine: f64, sine: f64) -> Rotation2D {
		Rotation2D {
			cosine,
			sine,
		}
	}

	//==============================================================================
	/// 각도로 만들기. (결정론 코사인과 사인)
	///
	/// # 인자
	/// - `angle`: 각도. (라디안)
	///
	/// # 돌려줌
	/// 새 회전.
	//==============================================================================
	pub fn from_angle(angle: f64) -> Rotation2D {
		PhysicsMath2D::compute_rotation(angle)
	}

	//==============================================================================
	/// 각도 반환. (결정론 아크탄젠트)
	///
	/// # 돌려줌
	/// 각도. (라디안, -파이 ~ 파이)
	//==============================================================================
	pub fn get_angle(&self) -> f64 {
		PhysicsMath2D::compute_atan2(self.sine, self.cosine)
	}

	//==============================================================================
	/// 두 회전을 잇기. (other 를 먼저 하고 이 회전을 한 것)
	///
	/// # 인자
	/// - `other`: 먼저 할 회전.
	///
	/// # 돌려줌
	/// 합친 회전.
	//==============================================================================
	pub fn multiply(&self, other: &Rotation2D) -> Rotation2D {
		Rotation2D::create(self.cosine * other.cosine - self.sine * other.sine, self.sine * other.cosine + self.cosine * other.sine)
	}
}
