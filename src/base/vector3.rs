//==============================================================================
// 3차원 벡터. (원본 base/vector3.js)
// - 원본은 x, y, z 를 감춘 필드와 프로퍼티로 두지만, 러스트 판은 Vector2 처럼 공개 필드를 둔 Copy 값이다.
//   원본 clone 은 복사로, equals 는 PartialEq 로 갈음한다.
// - 원본 add / subtract / multiply / divide 는 수와 Vector3 를 다 받는다. 러스트 판은 Vector3 를 받는 것과
//   수를 받는 ...Scalar 로 나눴다. (Vector2 의 multiply_scalar 와 같은 꼴)
// - 원본 수는 f64 이고 러스트 판은 엔진의 다른 값처럼 f32 다.
// - lerp 는 원본 math.lerp 식(from * (1 - t) + to * t)을 그대로 쓴다. 러스트 math::lerp(from + (to - from) * t)와
//   끝자리가 다를 수 있어서 따로 계산한다.
//==============================================================================
use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector3 {
	pub x: f32,
	pub y: f32,
	pub z: f32,
}

impl Vector3 {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub const fn create(x: f32, y: f32, z: f32) -> Vector3 {
		Vector3 { x, y, z }
	}

	//==============================================================================
	// 영벡터. (0, 0, 0)
	//==============================================================================
	pub const fn zero() -> Vector3 {
		Vector3 { x: 0.0, y: 0.0, z: 0.0 }
	}

	//==============================================================================
	// 일벡터. (1, 1, 1)
	//==============================================================================
	pub const fn one() -> Vector3 {
		Vector3 { x: 1.0, y: 1.0, z: 1.0 }
	}

	//==============================================================================
	// 위쪽 단위벡터. (0, 1, 0)
	//==============================================================================
	pub const fn up() -> Vector3 {
		Vector3 { x: 0.0, y: 1.0, z: 0.0 }
	}

	//==============================================================================
	// 대입.
	//==============================================================================
	pub fn set(&mut self, x: f32, y: f32, z: f32) {
		self.x = x;
		self.y = y;
		self.z = z;
	}

	//==============================================================================
	// 가로축 반환.
	//==============================================================================
	pub fn get_x(&self) -> f32 {
		self.x
	}

	//==============================================================================
	// 세로축 반환.
	//==============================================================================
	pub fn get_y(&self) -> f32 {
		self.y
	}

	//==============================================================================
	// 깊이축 반환.
	//==============================================================================
	pub fn get_z(&self) -> f32 {
		self.z
	}

	//==============================================================================
	// 길이.
	//==============================================================================
	pub fn length(&self) -> f32 {
		(self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
	}

	//==============================================================================
	// 길이 제곱.
	//==============================================================================
	pub fn length_squared(&self) -> f32 {
		self.x * self.x + self.y * self.y + self.z * self.z
	}

	//==============================================================================
	// 정규화. (길이가 0 이하이면 그대로 복사해 돌려준다)
	//==============================================================================
	pub fn normalize(&self) -> Vector3 {
		let mut origin = *self;
		let length = self.length();
		if length > 0.0 {
			origin.x /= length;
			origin.y /= length;
			origin.z /= length;
		}
		origin
	}

	//==============================================================================
	// 더하기.
	//==============================================================================
	pub fn add(&self, other: Vector3) -> Vector3 {
		Vector3::create(self.x + other.x, self.y + other.y, self.z + other.z)
	}

	//==============================================================================
	// 세 성분에 같은 수를 더하기. (원본 add(number))
	//==============================================================================
	pub fn add_scalar(&self, scalar: f32) -> Vector3 {
		Vector3::create(self.x + scalar, self.y + scalar, self.z + scalar)
	}

	//==============================================================================
	// 빼기.
	//==============================================================================
	pub fn subtract(&self, other: Vector3) -> Vector3 {
		Vector3::create(self.x - other.x, self.y - other.y, self.z - other.z)
	}

	//==============================================================================
	// 세 성분에서 같은 수를 빼기. (원본 subtract(number))
	//==============================================================================
	pub fn subtract_scalar(&self, scalar: f32) -> Vector3 {
		Vector3::create(self.x - scalar, self.y - scalar, self.z - scalar)
	}

	//==============================================================================
	// 성분별 곱하기.
	//==============================================================================
	pub fn multiply(&self, other: Vector3) -> Vector3 {
		Vector3::create(self.x * other.x, self.y * other.y, self.z * other.z)
	}

	//==============================================================================
	// 스칼라 곱하기. (원본 multiply(number))
	//==============================================================================
	pub fn multiply_scalar(&self, scalar: f32) -> Vector3 {
		Vector3::create(self.x * scalar, self.y * scalar, self.z * scalar)
	}

	//==============================================================================
	// 성분별 나누기.
	//==============================================================================
	pub fn divide(&self, other: Vector3) -> Vector3 {
		Vector3::create(self.x / other.x, self.y / other.y, self.z / other.z)
	}

	//==============================================================================
	// 스칼라 나누기. (원본 divide(number))
	//==============================================================================
	pub fn divide_scalar(&self, scalar: f32) -> Vector3 {
		Vector3::create(self.x / scalar, self.y / scalar, self.z / scalar)
	}

	//==============================================================================
	// 내적.
	//==============================================================================
	pub fn dot(&self, other: Vector3) -> f32 {
		self.x * other.x + self.y * other.y + self.z * other.z
	}

	//==============================================================================
	// 외적. (오른손 좌표계: x 축 외적 y 축 = z 축)
	//==============================================================================
	pub fn cross(&self, other: Vector3) -> Vector3 {
		let cross_x = self.y * other.z - self.z * other.y;
		let cross_y = self.z * other.x - self.x * other.z;
		let cross_z = self.x * other.y - self.y * other.x;
		Vector3::create(cross_x, cross_y, cross_z)
	}

	//==============================================================================
	// 두 점 사이 거리.
	//==============================================================================
	pub fn distance(&self, other: Vector3) -> f32 {
		let difference = self.subtract(other);
		difference.length()
	}

	//==============================================================================
	// 선형 보간. (원본 math.lerp 식: from * (1 - t) + to * t)
	//==============================================================================
	pub fn lerp(from: Vector3, to: Vector3, normalized_time: f32) -> Vector3 {
		let from_weight = 1.0 - normalized_time;
		let lerp_x = from.x * from_weight + to.x * normalized_time;
		let lerp_y = from.y * from_weight + to.y * normalized_time;
		let lerp_z = from.z * from_weight + to.z * normalized_time;
		Vector3::create(lerp_x, lerp_y, lerp_z)
	}
}

impl Add for Vector3 {
	type Output = Vector3;

	//==============================================================================
	// + 연산자. (add 와 같다)
	//==============================================================================
	fn add(self, other: Vector3) -> Vector3 {
		Vector3::create(self.x + other.x, self.y + other.y, self.z + other.z)
	}
}

impl Sub for Vector3 {
	type Output = Vector3;

	//==============================================================================
	// - 연산자. (subtract 와 같다)
	//==============================================================================
	fn sub(self, other: Vector3) -> Vector3 {
		Vector3::create(self.x - other.x, self.y - other.y, self.z - other.z)
	}
}

impl Mul<f32> for Vector3 {
	type Output = Vector3;

	//==============================================================================
	// * 스칼라 연산자. (multiply_scalar 와 같다)
	//==============================================================================
	fn mul(self, scalar: f32) -> Vector3 {
		Vector3::create(self.x * scalar, self.y * scalar, self.z * scalar)
	}
}

impl AddAssign for Vector3 {
	//==============================================================================
	// += 연산자.
	//==============================================================================
	fn add_assign(&mut self, other: Vector3) {
		self.x += other.x;
		self.y += other.y;
		self.z += other.z;
	}
}

impl SubAssign for Vector3 {
	//==============================================================================
	// -= 연산자.
	//==============================================================================
	fn sub_assign(&mut self, other: Vector3) {
		self.x -= other.x;
		self.y -= other.y;
		self.z -= other.z;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 외적이 오른손 좌표계를 따르는지. (x 외적 y = z, y 외적 z = x, z 외적 x = y)
	//==============================================================================
	#[test]
	fn cross_is_right_handed() {
		let axis_x = Vector3::create(1.0, 0.0, 0.0);
		let axis_y = Vector3::up();
		let axis_z = Vector3::create(0.0, 0.0, 1.0);
		assert_eq!(axis_x.cross(axis_y), axis_z);
		assert_eq!(axis_y.cross(axis_z), axis_x);
		assert_eq!(axis_z.cross(axis_x), axis_y);
		assert_eq!(Vector3::create(1.0, 2.0, 3.0).cross(Vector3::create(4.0, 5.0, 6.0)), Vector3::create(-3.0, 6.0, -3.0));
	}

	//==============================================================================
	// 사칙연산과 스칼라 판, 길이, 정규화, 거리.
	//==============================================================================
	#[test]
	fn arithmetic_and_length() {
		let vector = Vector3::create(2.0, -4.0, 4.0);
		assert_eq!(vector.add(Vector3::one()), Vector3::create(3.0, -3.0, 5.0));
		assert_eq!(vector.add_scalar(1.0), Vector3::create(3.0, -3.0, 5.0));
		assert_eq!(vector.subtract(Vector3::one()), Vector3::create(1.0, -5.0, 3.0));
		assert_eq!(vector.subtract_scalar(2.0), Vector3::create(0.0, -6.0, 2.0));
		assert_eq!(vector.multiply(Vector3::create(0.5, 2.0, -1.0)), Vector3::create(1.0, -8.0, -4.0));
		assert_eq!(vector.multiply_scalar(-0.5), Vector3::create(-1.0, 2.0, -2.0));
		assert_eq!(vector.divide(Vector3::create(2.0, 4.0, -4.0)), Vector3::create(1.0, -1.0, -1.0));
		assert_eq!(vector.divide_scalar(2.0), Vector3::create(1.0, -2.0, 2.0));
		assert_eq!(vector.length_squared(), 36.0);
		assert_eq!(vector.length(), 6.0);
		assert_eq!(vector.normalize(), Vector3::create(1.0 / 3.0, -2.0 / 3.0, 2.0 / 3.0));
		assert_eq!(Vector3::zero().normalize(), Vector3::zero());
		assert_eq!(vector.distance(Vector3::create(2.0, -4.0, 1.0)), 3.0);
		assert_eq!(vector.dot(Vector3::create(1.0, 1.0, 1.0)), 2.0);
		assert_eq!(vector + Vector3::one(), vector.add(Vector3::one()));
		assert_eq!(vector - Vector3::one(), vector.subtract(Vector3::one()));
		assert_eq!(vector * 2.0, vector.multiply_scalar(2.0));
	}

	//==============================================================================
	// 선형 보간이 원본 식과 같은지.
	//==============================================================================
	#[test]
	fn lerp_matches_original_formula() {
		let from = Vector3::create(0.0, 10.0, -4.0);
		let to = Vector3::create(8.0, 0.0, 4.0);
		assert_eq!(Vector3::lerp(from, to, 0.0), from);
		assert_eq!(Vector3::lerp(from, to, 1.0), to);
		assert_eq!(Vector3::lerp(from, to, 0.25), Vector3::create(2.0, 7.5, -2.0));
	}
}
