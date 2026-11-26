//==============================================================================
//! 2차원 벡터. (원본 base/vector2.js)
//! - 원본 add / subtract / multiply / divide / modulo 는 수와 Vector2 를 다 받는다. 러스트 판은 Vector2 를 받는 것과
//!   수를 받는 ..._scalar 로 나눴다. 수는 원본 자바스크립트 수처럼 f64 다.
//! - lerp 는 원본 math.lerp 식(from * (1 - t) + to * t)을 그대로 쓴다. (Vector3 와 같다)
//! - 원본에 있던 버그 둘(modulo 가 나머지 대신 나누기를 하고, half 가 반 대신 두 배(0.5 로 나누기)를 돌려주던 것)을 두 엔진에서 같이 고쳤다.
//!   modulo 는 나머지(%)를, half 는 반을 돌려준다. (vanilla.js 와 게임에서 둘을 쓰는 곳은 없다)
//==============================================================================
use crate::base::math;
use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

//==============================================================================
/// 2차원 벡터. (x, y 를 공개 필드로 둔 Copy 값, 계산 함수는 자신을 바꾸지 않고 새 벡터를 돌려준다)
///
/// # 예제
/// 만들고 더하고 길이 재기
/// ```rust
/// use vanilla::Vector2;
///
/// let start = Vector2::create(1.0, 2.0);
/// let velocity = Vector2::create(3.0, 4.0);
/// let moved = start.add(velocity);
/// assert_eq!(moved, Vector2::create(4.0, 6.0));
/// assert_eq!(velocity.length(), 5.0);
/// assert_eq!(start + velocity, moved);
///
/// let mut position = Vector2::zero();
/// position += velocity * 2.0;
/// assert_eq!(position, Vector2::create(6.0, 8.0));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector2 {
	/// x 성분.
	pub x: f64,
	/// y 성분.
	pub y: f64,
}

impl Vector2 {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `x`: x 성분.
	/// - `y`: y 성분.
	///
	/// # 돌려줌
	/// (x, y) 벡터.
	//==============================================================================
	pub const fn create(x: f64, y: f64) -> Vector2 {
		Vector2 { x, y }
	}

	//==============================================================================
	/// (0, 0).
	///
	/// # 돌려줌
	/// 두 성분이 0 인 벡터.
	//==============================================================================
	pub const fn zero() -> Vector2 {
		Vector2 { x: 0.0, y: 0.0 }
	}

	//==============================================================================
	/// (1, 1).
	///
	/// # 돌려줌
	/// 두 성분이 1 인 벡터.
	//==============================================================================
	pub const fn one() -> Vector2 {
		Vector2 { x: 1.0, y: 1.0 }
	}

	//==============================================================================
	/// (-1, -1).
	///
	/// # 돌려줌
	/// 두 성분이 -1 인 벡터.
	//==============================================================================
	pub const fn minus_one() -> Vector2 {
		Vector2 { x: -1.0, y: -1.0 }
	}

	//==============================================================================
	/// (+무한대, +무한대).
	///
	/// # 돌려줌
	/// 두 성분이 f64::INFINITY 인 벡터.
	//==============================================================================
	pub const fn positive_infinity() -> Vector2 {
		Vector2 { x: f64::INFINITY, y: f64::INFINITY }
	}

	//==============================================================================
	/// (-무한대, -무한대).
	///
	/// # 돌려줌
	/// 두 성분이 f64::NEG_INFINITY 인 벡터.
	//==============================================================================
	pub const fn negative_infinity() -> Vector2 {
		Vector2 { x: f64::NEG_INFINITY, y: f64::NEG_INFINITY }
	}

	//==============================================================================
	/// 값 설정.
	///
	/// # 인자
	/// - `x`: 넣을 x 성분.
	/// - `y`: 넣을 y 성분.
	//==============================================================================
	pub fn set(&mut self, x: f64, y: f64) {
		self.x = x;
		self.y = y;
	}

	//==============================================================================
	/// x 반환.
	///
	/// # 돌려줌
	/// x 성분.
	//==============================================================================
	pub fn get_x(&self) -> f64 {
		self.x
	}

	//==============================================================================
	/// y 반환.
	///
	/// # 돌려줌
	/// y 성분.
	//==============================================================================
	pub fn get_y(&self) -> f64 {
		self.y
	}

	//==============================================================================
	/// 더하기.
	///
	/// # 인자
	/// - `other`: 더할 벡터.
	///
	/// # 돌려줌
	/// 성분별로 더한 새 벡터. (자신은 바뀌지 않는다)
	//==============================================================================
	pub fn add(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x + other.x, self.y + other.y)
	}

	//==============================================================================
	/// 수 더하기. (두 성분에 같이)
	///
	/// # 인자
	/// - `scalar`: 두 성분에 더할 수.
	///
	/// # 돌려줌
	/// 더한 새 벡터.
	//==============================================================================
	pub fn add_scalar(&self, scalar: f64) -> Vector2 {
		Vector2::create(self.x + scalar, self.y + scalar)
	}

	//==============================================================================
	/// 빼기.
	///
	/// # 인자
	/// - `other`: 뺄 벡터.
	///
	/// # 돌려줌
	/// 성분별로 뺀 새 벡터. (self - other)
	//==============================================================================
	pub fn subtract(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x - other.x, self.y - other.y)
	}

	//==============================================================================
	/// 수 빼기. (두 성분에서 같이)
	///
	/// # 인자
	/// - `scalar`: 두 성분에서 뺄 수.
	///
	/// # 돌려줌
	/// 뺀 새 벡터.
	//==============================================================================
	pub fn subtract_scalar(&self, scalar: f64) -> Vector2 {
		Vector2::create(self.x - scalar, self.y - scalar)
	}

	//==============================================================================
	/// 성분별 곱하기.
	///
	/// # 인자
	/// - `other`: 성분별로 곱할 벡터.
	///
	/// # 돌려줌
	/// 성분별로 곱한 새 벡터.
	//==============================================================================
	pub fn multiply(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x * other.x, self.y * other.y)
	}

	//==============================================================================
	/// 스칼라 곱하기.
	///
	/// # 인자
	/// - `scalar`: 두 성분에 곱할 수.
	///
	/// # 돌려줌
	/// 곱한 새 벡터.
	//==============================================================================
	pub fn multiply_scalar(&self, scalar: f64) -> Vector2 {
		Vector2::create(self.x * scalar, self.y * scalar)
	}

	//==============================================================================
	/// 성분별 나누기.
	///
	/// # 인자
	/// - `other`: 성분별로 나눌 벡터. (0 인 성분은 무한대나 NaN 이 된다)
	///
	/// # 돌려줌
	/// 성분별로 나눈 새 벡터.
	//==============================================================================
	pub fn divide(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x / other.x, self.y / other.y)
	}

	//==============================================================================
	/// 스칼라 나누기.
	///
	/// # 인자
	/// - `scalar`: 두 성분을 나눌 수. (0 이면 무한대나 NaN 이 된다)
	///
	/// # 돌려줌
	/// 나눈 새 벡터.
	//==============================================================================
	pub fn divide_scalar(&self, scalar: f64) -> Vector2 {
		Vector2::create(self.x / scalar, self.y / scalar)
	}

	//==============================================================================
	/// 성분별 나머지. (부호는 나뉘는 쪽을 따른다, 원본 % 와 같다)
	///
	/// # 인자
	/// - `other`: 성분별로 나눌 벡터.
	///
	/// # 돌려줌
	/// 성분별 나머지를 담은 새 벡터.
	///
	/// # 예제
	/// 음수의 나머지는 음수
	/// ```rust
	/// use vanilla::Vector2;
	///
	/// let remainder = Vector2::create(7.0, -9.0).modulo(Vector2::create(4.0, 4.0));
	/// assert_eq!(remainder, Vector2::create(3.0, -1.0));
	/// ```
	//==============================================================================
	pub fn modulo(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x % other.x, self.y % other.y)
	}

	//==============================================================================
	/// 스칼라 나머지.
	///
	/// # 인자
	/// - `scalar`: 두 성분을 나눌 수.
	///
	/// # 돌려줌
	/// 성분별 나머지를 담은 새 벡터.
	//==============================================================================
	pub fn modulo_scalar(&self, scalar: f64) -> Vector2 {
		Vector2::create(self.x % scalar, self.y % scalar)
	}

	//==============================================================================
	/// 반.
	///
	/// # 돌려줌
	/// 두 성분에 0.5 를 곱한 새 벡터.
	//==============================================================================
	pub fn half(&self) -> Vector2 {
		self.multiply_scalar(0.5)
	}

	//==============================================================================
	/// 길이.
	///
	/// # 돌려줌
	/// 벡터의 길이. (0 이상)
	//==============================================================================
	pub fn length(&self) -> f64 {
		(self.x * self.x + self.y * self.y).sqrt()
	}

	//==============================================================================
	/// 길이의 제곱.
	///
	/// # 돌려줌
	/// x * x + y * y. (제곱근을 셈하지 않는다)
	//==============================================================================
	pub fn length_squared(&self) -> f64 {
		self.x * self.x + self.y * self.y
	}

	//==============================================================================
	/// 두 점 사이 거리.
	///
	/// # 인자
	/// - `other`: 거리를 잴 점.
	///
	/// # 돌려줌
	/// 두 점 사이 거리. (0 이상)
	//==============================================================================
	pub fn distance(&self, other: Vector2) -> f64 {
		let delta = self.subtract(other);
		delta.length()
	}

	//==============================================================================
	/// 단위 벡터. (길이가 0 이면 (0, 0))
	///
	/// # 돌려줌
	/// 길이가 1 인 새 벡터. 길이가 0 이하이면 (0, 0).
	///
	/// # 예제
	/// 방향만 남기기
	/// ```rust
	/// use vanilla::Vector2;
	///
	/// let direction = Vector2::create(3.0, 4.0).normalize();
	/// assert_eq!(direction, Vector2::create(0.6, 0.8));
	/// assert_eq!(Vector2::zero().normalize(), Vector2::zero());
	/// ```
	//==============================================================================
	pub fn normalize(&self) -> Vector2 {
		let length = self.length();
		if length <= 0.0 {
			return Vector2::zero();
		}
		Vector2::create(self.x / length, self.y / length)
	}

	//==============================================================================
	/// 내적.
	///
	/// # 인자
	/// - `other`: 내적할 벡터.
	///
	/// # 돌려줌
	/// 내적 값. (self.x * other.x + self.y * other.y)
	//==============================================================================
	pub fn dot(&self, other: Vector2) -> f64 {
		self.x * other.x + self.y * other.y
	}

	//==============================================================================
	/// 외적의 z. (self.x * other.y - self.y * other.x, 양수면 other 가 반시계 쪽)
	///
	/// # 인자
	/// - `other`: 외적할 벡터.
	///
	/// # 돌려줌
	/// 외적의 z 값.
	///
	/// # 예제
	/// 순서를 바꾸면 부호가 바뀐다
	/// ```rust
	/// use vanilla::Vector2;
	///
	/// let axis_x = Vector2::create(1.0, 0.0);
	/// let axis_y = Vector2::create(0.0, 1.0);
	/// assert_eq!(axis_x.cross(axis_y), 1.0);
	/// assert_eq!(axis_y.cross(axis_x), -1.0);
	/// ```
	//==============================================================================
	pub fn cross(&self, other: Vector2) -> f64 {
		self.x * other.y - self.y * other.x
	}

	//==============================================================================
	/// 두 벡터 사이 각. (라디안, 0 ~ PI, 길이가 0 인 벡터면 NaN)
	///
	/// # 인자
	/// - `other`: 사이 각을 잴 벡터.
	///
	/// # 돌려줌
	/// 두 벡터 사이 각. (라디안, 0 ~ PI, 어느 한쪽 길이가 0 이면 NaN)
	///
	/// # 예제
	/// 직각과 길이 0 인 벡터
	/// ```rust
	/// use vanilla::{math, Vector2};
	///
	/// let angle = Vector2::create(1.0, 0.0).angle(Vector2::create(0.0, 2.0));
	/// assert!((angle - math::PI * 0.5).abs() < 1e-6);
	/// assert!(Vector2::zero().angle(Vector2::one()).is_nan());
	/// ```
	//==============================================================================
	pub fn angle(&self, other: Vector2) -> f64 {
		(self.dot(other) / (self.length() * other.length())).acos()
	}

	//==============================================================================
	/// 선형 보간. (원본 math.lerp 식: from * (1 - t) + to * t)
	///
	/// # 인자
	/// - `from`: 시작 벡터.
	/// - `to`: 끝 벡터.
	/// - `normalized_time`: 보간 비율. (0 이면 from, 1 이면 to, 0 ~ 1 밖도 자르지 않고 늘여 잇는다)
	///
	/// # 돌려줌
	/// 보간한 새 벡터.
	///
	/// # 예제
	/// 4 분의 1 지점
	/// ```rust
	/// use vanilla::Vector2;
	///
	/// let from = Vector2::create(0.0, 10.0);
	/// let to = Vector2::create(8.0, -6.0);
	/// assert_eq!(Vector2::lerp(from, to, 0.25), Vector2::create(2.0, 6.0));
	/// assert_eq!(Vector2::lerp(from, to, 1.0), to);
	/// ```
	//==============================================================================
	pub fn lerp(from: Vector2, to: Vector2, normalized_time: f64) -> Vector2 {
		let from_weight = 1.0 - normalized_time;
		let lerp_x = from.x * from_weight + to.x * normalized_time;
		let lerp_y = from.y * from_weight + to.y * normalized_time;
		Vector2::create(lerp_x, lerp_y)
	}

	//==============================================================================
	/// 성분별 범위 제한.
	///
	/// # 인자
	/// - `value`: 제한할 벡터.
	/// - `minimum`: 성분별 최소값.
	/// - `maximum`: 성분별 최대값.
	///
	/// # 돌려줌
	/// 성분마다 minimum ~ maximum 으로 자른 새 벡터. (math::clamp 를 성분마다 쓴다)
	///
	/// # 예제
	/// 범위 안으로 자르기
	/// ```rust
	/// use vanilla::Vector2;
	///
	/// let clamped = Vector2::clamp(Vector2::create(-5.0, 50.0), Vector2::zero(), Vector2::create(10.0, 10.0));
	/// assert_eq!(clamped, Vector2::create(0.0, 10.0));
	/// ```
	//==============================================================================
	pub fn clamp(value: Vector2, minimum: Vector2, maximum: Vector2) -> Vector2 {
		Vector2::create(math::clamp(value.x, minimum.x, maximum.x), math::clamp(value.y, minimum.y, maximum.y))
	}
}

impl Add for Vector2 {
	type Output = Vector2;
	//==============================================================================
	/// `+` 연산자. (add 와 같다)
	//==============================================================================
	fn add(self, other: Vector2) -> Vector2 {
		Vector2::create(self.x + other.x, self.y + other.y)
	}
}

impl Sub for Vector2 {
	type Output = Vector2;
	//==============================================================================
	/// `-` 연산자. (subtract 와 같다)
	//==============================================================================
	fn sub(self, other: Vector2) -> Vector2 {
		Vector2::create(self.x - other.x, self.y - other.y)
	}
}

impl Mul<f64> for Vector2 {
	type Output = Vector2;
	//==============================================================================
	/// `*` 스칼라 연산자. (multiply_scalar 와 같다)
	//==============================================================================
	fn mul(self, scalar: f64) -> Vector2 {
		Vector2::create(self.x * scalar, self.y * scalar)
	}
}

impl AddAssign for Vector2 {
	//==============================================================================
	/// `+=` 연산자. (성분별로 더해 자신을 바꾼다)
	//==============================================================================
	fn add_assign(&mut self, other: Vector2) {
		self.x += other.x;
		self.y += other.y;
	}
}

impl SubAssign for Vector2 {
	//==============================================================================
	/// `-=` 연산자. (성분별로 빼 자신을 바꾼다)
	//==============================================================================
	fn sub_assign(&mut self, other: Vector2) {
		self.x -= other.x;
		self.y -= other.y;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 나누기, 나머지, 반, 외적, 길이 제곱, 각. (원본 modulo 와 half 의 버그는 고친 값)
	//==============================================================================
	#[test]
	fn arithmetic_helpers() {
		let vector = Vector2::create(7.0, -9.0);
		assert_eq!(vector.add_scalar(1.0), Vector2::create(8.0, -8.0));
		assert_eq!(vector.subtract_scalar(1.0), Vector2::create(6.0, -10.0));
		assert_eq!(vector.divide(Vector2::create(2.0, 3.0)), Vector2::create(3.5, -3.0));
		assert_eq!(vector.divide_scalar(2.0), Vector2::create(3.5, -4.5));
		assert_eq!(vector.modulo(Vector2::create(4.0, 4.0)), Vector2::create(3.0, -1.0));
		assert_eq!(vector.modulo_scalar(5.0), Vector2::create(2.0, -4.0));
		assert_eq!(vector.half(), Vector2::create(3.5, -4.5));
		assert_eq!(vector.length_squared(), 130.0);
		assert_eq!(vector.get_x(), 7.0);
		assert_eq!(vector.get_y(), -9.0);
		assert_eq!(Vector2::create(1.0, 0.0).cross(Vector2::create(0.0, 1.0)), 1.0);
		assert_eq!(Vector2::create(0.0, 1.0).cross(Vector2::create(1.0, 0.0)), -1.0);
		assert!((Vector2::create(1.0, 0.0).angle(Vector2::create(0.0, 2.0)) - math::PI * 0.5).abs() < 1e-6);
		assert!(Vector2::zero().angle(Vector2::one()).is_nan());
	}

	//==============================================================================
	/// 상수 벡터, 보간(끝점이 정확하다), 범위 제한.
	//==============================================================================
	#[test]
	fn constants_lerp_and_clamp() {
		assert_eq!(Vector2::minus_one(), Vector2::create(-1.0, -1.0));
		assert_eq!(Vector2::positive_infinity().x, f64::INFINITY);
		assert_eq!(Vector2::negative_infinity().y, f64::NEG_INFINITY);
		let from = Vector2::create(0.1, 10.0);
		let to = Vector2::create(0.3, -4.0);
		assert_eq!(Vector2::lerp(from, to, 0.0), from);
		assert_eq!(Vector2::lerp(from, to, 1.0), to);
		assert_eq!(Vector2::lerp(Vector2::zero(), Vector2::create(8.0, -4.0), 0.25), Vector2::create(2.0, -1.0));
		let clamped = Vector2::clamp(Vector2::create(-5.0, 50.0), Vector2::zero(), Vector2::create(10.0, 10.0));
		assert_eq!(clamped, Vector2::create(0.0, 10.0));
	}
}
