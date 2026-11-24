//==============================================================================
// 2차원 벡터. (원본 base/vector2.js)
// - 원본 add / subtract / multiply / divide / modulo 는 수와 Vector2 를 다 받는다. 러스트 판은 Vector2 를 받는 것과
//   수를 받는 ..._scalar 로 나눴다. 원본 수는 f64 이고 러스트 판은 엔진의 다른 값처럼 f32 다.
// - lerp 는 원본 math.lerp 식(from * (1 - t) + to * t)을 그대로 쓴다. (Vector3 와 같다)
// - 원본의 버그 둘을 고쳤다. modulo 가 나머지 대신 나누기를 하고, half 가 반 대신 두 배(0.5 로 나누기)를 돌려준다.
//   러스트 판은 modulo 가 나머지(%)를, half 가 반을 돌려준다. (vanilla.js 와 게임에서 둘을 쓰는 곳은 없다)
//==============================================================================
use crate::base::math;
use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector2 {
	pub x: f32,
	pub y: f32,
}

impl Vector2 {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub const fn create(x: f32, y: f32) -> Vector2 {
		Vector2 { x, y }
	}

	//==============================================================================
	// (0, 0).
	//==============================================================================
	pub const fn zero() -> Vector2 {
		Vector2 { x: 0.0, y: 0.0 }
	}

	//==============================================================================
	// (1, 1).
	//==============================================================================
	pub const fn one() -> Vector2 {
		Vector2 { x: 1.0, y: 1.0 }
	}

	//==============================================================================
	// (-1, -1).
	//==============================================================================
	pub const fn minus_one() -> Vector2 {
		Vector2 { x: -1.0, y: -1.0 }
	}

	//==============================================================================
	// (+무한대, +무한대).
	//==============================================================================
	pub const fn positive_infinity() -> Vector2 {
		Vector2 { x: f32::INFINITY, y: f32::INFINITY }
	}

	//==============================================================================
	// (-무한대, -무한대).
	//==============================================================================
	pub const fn negative_infinity() -> Vector2 {
		Vector2 { x: f32::NEG_INFINITY, y: f32::NEG_INFINITY }
	}

	//==============================================================================
	// 값 설정.
	//==============================================================================
	pub fn set(&mut self, x: f32, y: f32) {
		self.x = x;
		self.y = y;
	}

	//==============================================================================
	// x 반환.
	//==============================================================================
	pub fn get_x(&self) -> f32 {
		self.x
	}

	//==============================================================================
	// y 반환.
	//==============================================================================
	pub fn get_y(&self) -> f32 {
		self.y
	}

	//==============================================================================
	// 더하기.
	//==============================================================================
	pub fn add(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x + other.x, self.y + other.y)
	}

	//==============================================================================
	// 수 더하기. (두 성분에 같이)
	//==============================================================================
	pub fn add_scalar(&self, scalar: f32) -> Vector2 {
		Vector2::create(self.x + scalar, self.y + scalar)
	}

	//==============================================================================
	// 빼기.
	//==============================================================================
	pub fn subtract(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x - other.x, self.y - other.y)
	}

	//==============================================================================
	// 수 빼기. (두 성분에서 같이)
	//==============================================================================
	pub fn subtract_scalar(&self, scalar: f32) -> Vector2 {
		Vector2::create(self.x - scalar, self.y - scalar)
	}

	//==============================================================================
	// 성분별 곱하기.
	//==============================================================================
	pub fn multiply(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x * other.x, self.y * other.y)
	}

	//==============================================================================
	// 스칼라 곱하기.
	//==============================================================================
	pub fn multiply_scalar(&self, scalar: f32) -> Vector2 {
		Vector2::create(self.x * scalar, self.y * scalar)
	}

	//==============================================================================
	// 성분별 나누기.
	//==============================================================================
	pub fn divide(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x / other.x, self.y / other.y)
	}

	//==============================================================================
	// 스칼라 나누기.
	//==============================================================================
	pub fn divide_scalar(&self, scalar: f32) -> Vector2 {
		Vector2::create(self.x / scalar, self.y / scalar)
	}

	//==============================================================================
	// 성분별 나머지. (부호는 나뉘는 쪽을 따른다, 원본 % 와 같다)
	//==============================================================================
	pub fn modulo(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x % other.x, self.y % other.y)
	}

	//==============================================================================
	// 스칼라 나머지.
	//==============================================================================
	pub fn modulo_scalar(&self, scalar: f32) -> Vector2 {
		Vector2::create(self.x % scalar, self.y % scalar)
	}

	//==============================================================================
	// 반.
	//==============================================================================
	pub fn half(&self) -> Vector2 {
		self.multiply_scalar(0.5)
	}

	//==============================================================================
	// 길이.
	//==============================================================================
	pub fn length(&self) -> f32 {
		(self.x * self.x + self.y * self.y).sqrt()
	}

	//==============================================================================
	// 길이의 제곱.
	//==============================================================================
	pub fn length_squared(&self) -> f32 {
		self.x * self.x + self.y * self.y
	}

	//==============================================================================
	// 두 점 사이 거리.
	//==============================================================================
	pub fn distance(&self, other: Vector2) -> f32 {
		let delta = self.subtract(other);
		delta.length()
	}

	//==============================================================================
	// 단위 벡터. (길이가 0 이면 (0, 0))
	//==============================================================================
	pub fn normalize(&self) -> Vector2 {
		let length = self.length();
		if length <= 0.0 {
			return Vector2::zero();
		}
		Vector2::create(self.x / length, self.y / length)
	}

	//==============================================================================
	// 내적.
	//==============================================================================
	pub fn dot(&self, other: Vector2) -> f32 {
		self.x * other.x + self.y * other.y
	}

	//==============================================================================
	// 외적의 z. (self.x * other.y - self.y * other.x, 양수면 other 가 반시계 쪽)
	//==============================================================================
	pub fn cross(&self, other: Vector2) -> f32 {
		self.x * other.y - self.y * other.x
	}

	//==============================================================================
	// 두 벡터 사이 각. (라디안, 0 ~ PI, 길이가 0 인 벡터면 NaN)
	//==============================================================================
	pub fn angle(&self, other: Vector2) -> f32 {
		(self.dot(other) / (self.length() * other.length())).acos()
	}

	//==============================================================================
	// 선형 보간. (원본 math.lerp 식: from * (1 - t) + to * t)
	//==============================================================================
	pub fn lerp(from: Vector2, to: Vector2, normalized_time: f32) -> Vector2 {
		let from_weight = 1.0 - normalized_time;
		let lerp_x = from.x * from_weight + to.x * normalized_time;
		let lerp_y = from.y * from_weight + to.y * normalized_time;
		Vector2::create(lerp_x, lerp_y)
	}

	//==============================================================================
	// 성분별 범위 제한.
	//==============================================================================
	pub fn clamp(value: Vector2, minimum: Vector2, maximum: Vector2) -> Vector2 {
		Vector2::create(math::clamp(value.x, minimum.x, maximum.x), math::clamp(value.y, minimum.y, maximum.y))
	}
}

impl Add for Vector2 {
	type Output = Vector2;
	fn add(self, other: Vector2) -> Vector2 {
		Vector2::create(self.x + other.x, self.y + other.y)
	}
}

impl Sub for Vector2 {
	type Output = Vector2;
	fn sub(self, other: Vector2) -> Vector2 {
		Vector2::create(self.x - other.x, self.y - other.y)
	}
}

impl Mul<f32> for Vector2 {
	type Output = Vector2;
	fn mul(self, scalar: f32) -> Vector2 {
		Vector2::create(self.x * scalar, self.y * scalar)
	}
}

impl AddAssign for Vector2 {
	fn add_assign(&mut self, other: Vector2) {
		self.x += other.x;
		self.y += other.y;
	}
}

impl SubAssign for Vector2 {
	fn sub_assign(&mut self, other: Vector2) {
		self.x -= other.x;
		self.y -= other.y;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 나누기, 나머지, 반, 외적, 길이 제곱, 각. (원본 modulo 와 half 의 버그는 고친 값)
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
	// 상수 벡터, 보간(끝점이 정확하다), 범위 제한.
	//==============================================================================
	#[test]
	fn constants_lerp_and_clamp() {
		assert_eq!(Vector2::minus_one(), Vector2::create(-1.0, -1.0));
		assert_eq!(Vector2::positive_infinity().x, f32::INFINITY);
		assert_eq!(Vector2::negative_infinity().y, f32::NEG_INFINITY);
		let from = Vector2::create(0.1, 10.0);
		let to = Vector2::create(0.3, -4.0);
		assert_eq!(Vector2::lerp(from, to, 0.0), from);
		assert_eq!(Vector2::lerp(from, to, 1.0), to);
		assert_eq!(Vector2::lerp(Vector2::zero(), Vector2::create(8.0, -4.0), 0.25), Vector2::create(2.0, -1.0));
		let clamped = Vector2::clamp(Vector2::create(-5.0, 50.0), Vector2::zero(), Vector2::create(10.0, 10.0));
		assert_eq!(clamped, Vector2::create(0.0, 10.0));
	}
}
