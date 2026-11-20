//==============================================================================
// 2차원 벡터.
//==============================================================================
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
	// 값 설정.
	//==============================================================================
	pub fn set(&mut self, x: f32, y: f32) {
		self.x = x;
		self.y = y;
	}

	//==============================================================================
	// 더하기.
	//==============================================================================
	pub fn add(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x + other.x, self.y + other.y)
	}

	//==============================================================================
	// 빼기.
	//==============================================================================
	pub fn subtract(&self, other: Vector2) -> Vector2 {
		Vector2::create(self.x - other.x, self.y - other.y)
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
	// 길이.
	//==============================================================================
	pub fn length(&self) -> f32 {
		(self.x * self.x + self.y * self.y).sqrt()
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
