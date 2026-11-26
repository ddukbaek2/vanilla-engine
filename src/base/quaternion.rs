//==============================================================================
//! 쿼터니언. (원본 base/quaternion.js, 3차원 회전이고 골격 애니메이션 보간의 기본 단위다)
//! - 성분 순서는 원본과 같이 (x, y, z, w) 이고, w 가 실수부다. 기본값은 단위 쿼터니언 (0, 0, 0, 1) 이다.
//! - multiply 는 해밀턴 곱 this x other 다. 행렬로 바꾸면 R(this x other) = R(this) x R(other) 라서,
//!   열벡터에는 other 회전이 먼저 걸린다. (원본 말로 "this 회전 뒤에 other 회전을 로컬로 누적")
//! - create_from_euler(x, y, z) 는 라디안이고 qX x qY x qZ 와 같다. 행렬로는 Rx x Ry x Rz 라서 점에는 Z, Y, X 순서로 걸린다.
//!   (three.js 의 'XYZ' 순서와 같은 식이다)
//! - 원본은 x, y, z, w 를 감춘 필드와 프로퍼티로 두지만, 러스트 판은 공개 필드를 둔 Copy 값이다. 원본 clone 은 복사로 갈음한다.
//! - 수는 원본 자바스크립트 수처럼 f64 다.
//! - 원본에 없는 행렬에서 쿼터니언으로 바꾸기, 오일러로 되돌리기는 만들지 않았다. (행렬로는 Matrix4::create_rotation_from_quaternion)
//==============================================================================
use crate::base::math;

//==============================================================================
/// 쿼터니언. (3차원 회전, x, y, z, w 를 공개 필드로 둔 Copy 값, 기본값은 단위 쿼터니언)
///
/// # 예제
/// 두 회전 사이를 구면 보간하기
/// ```rust
/// use vanilla::{math, Matrix4, Quaternion};
///
/// let start = Quaternion::identity();
/// let end = Quaternion::create_from_euler(0.0, math::PI * 0.5, 0.0);
/// let middle = Quaternion::slerp(start, end, 0.5);
/// let expected = Quaternion::create_from_euler(0.0, math::PI * 0.25, 0.0);
/// assert!((middle.y - expected.y).abs() < 1e-5);
/// assert!((middle.w - expected.w).abs() < 1e-5);
///
/// let rotation_matrix = Matrix4::create_rotation_from_quaternion(middle);
/// let forward = rotation_matrix.transform_direction(0.0, 0.0, 1.0);
/// assert!((forward.x - forward.z).abs() < 1e-5);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
	/// 벡터부 x. (회전 축 x 에 sin(각 / 2) 를 곱한 값)
	pub x: f64,
	/// 벡터부 y. (회전 축 y 에 sin(각 / 2) 를 곱한 값)
	pub y: f64,
	/// 벡터부 z. (회전 축 z 에 sin(각 / 2) 를 곱한 값)
	pub z: f64,
	/// 실수부. (cos(각 / 2))
	pub w: f64,
}

impl Default for Quaternion {
	//==============================================================================
	/// 단위 쿼터니언. (원본 new Quaternion())
	//==============================================================================
	fn default() -> Quaternion {
		Quaternion::identity()
	}
}

impl Quaternion {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `x`: 벡터부 x.
	/// - `y`: 벡터부 y.
	/// - `z`: 벡터부 z.
	/// - `w`: 실수부.
	///
	/// # 돌려줌
	/// (x, y, z, w) 쿼터니언. (정규화하지 않는다)
	//==============================================================================
	pub const fn create(x: f64, y: f64, z: f64, w: f64) -> Quaternion {
		Quaternion { x, y, z, w }
	}

	//==============================================================================
	/// 단위 쿼터니언. (0, 0, 0, 1)
	///
	/// # 돌려줌
	/// 회전이 없는 쿼터니언.
	//==============================================================================
	pub const fn identity() -> Quaternion {
		Quaternion { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
	}

	//==============================================================================
	/// 대입.
	///
	/// # 인자
	/// - `x`: 넣을 벡터부 x.
	/// - `y`: 넣을 벡터부 y.
	/// - `z`: 넣을 벡터부 z.
	/// - `w`: 넣을 실수부.
	//==============================================================================
	pub fn set(&mut self, x: f64, y: f64, z: f64, w: f64) {
		self.x = x;
		self.y = y;
		self.z = z;
		self.w = w;
	}

	//==============================================================================
	/// 정규화. (길이가 0 이하이면 단위 쿼터니언)
	///
	/// # 돌려줌
	/// 길이가 1 인 새 쿼터니언. 길이가 0 이하이면 단위 쿼터니언.
	//==============================================================================
	pub fn normalize(&self) -> Quaternion {
		let length = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
		if length > 0.0 {
			return Quaternion::create(self.x / length, self.y / length, self.z / length, self.w / length);
		}
		Quaternion::identity()
	}

	//==============================================================================
	/// 곱하기. (해밀턴 곱 this x other, this 회전 뒤에 other 회전을 로컬로 누적한다)
	///
	/// # 인자
	/// - `other`: 오른쪽에 곱할 쿼터니언. (점에는 other 회전이 먼저 걸린다)
	///
	/// # 돌려줌
	/// self x other 인 새 쿼터니언. (정규화하지 않는다)
	///
	/// # 예제
	/// Z 축 90 도를 두 번 곱하면 180 도
	/// ```rust
	/// use vanilla::{math, Quaternion};
	///
	/// let quarter_turn = Quaternion::create_from_euler(0.0, 0.0, math::PI * 0.5);
	/// let half_turn = quarter_turn.multiply(quarter_turn);
	/// assert!((half_turn.z - 1.0).abs() < 1e-6);
	/// assert!(half_turn.w.abs() < 1e-6);
	/// ```
	//==============================================================================
	pub fn multiply(&self, other: Quaternion) -> Quaternion {
		let result_x = self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y;
		let result_y = self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x;
		let result_z = self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w;
		let result_w = self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z;
		Quaternion::create(result_x, result_y, result_z, result_w)
	}

	//==============================================================================
	/// 오일러 각으로 만든다. (XYZ 순서 라디안, qX x qY x qZ 와 같다)
	///
	/// # 인자
	/// - `x_radian`: X 축 회전 각. (라디안)
	/// - `y_radian`: Y 축 회전 각. (라디안)
	/// - `z_radian`: Z 축 회전 각. (라디안)
	///
	/// # 돌려줌
	/// 세 축 회전을 합친 단위 쿼터니언. (점에는 Z, Y, X 순서로 걸린다)
	///
	/// # 예제
	/// 축마다 만든 쿼터니언을 X, Y, Z 차례로 곱한 것과 같다
	/// ```rust
	/// use vanilla::Quaternion;
	///
	/// let from_euler = Quaternion::create_from_euler(0.3, -0.7, 1.1);
	/// let quaternion_x = Quaternion::create_from_euler(0.3, 0.0, 0.0);
	/// let quaternion_y = Quaternion::create_from_euler(0.0, -0.7, 0.0);
	/// let quaternion_z = Quaternion::create_from_euler(0.0, 0.0, 1.1);
	/// let multiplied = quaternion_x.multiply(quaternion_y).multiply(quaternion_z);
	/// assert!((from_euler.x - multiplied.x).abs() < 1e-5);
	/// assert!((from_euler.y - multiplied.y).abs() < 1e-5);
	/// assert!((from_euler.z - multiplied.z).abs() < 1e-5);
	/// assert!((from_euler.w - multiplied.w).abs() < 1e-5);
	/// ```
	//==============================================================================
	pub fn create_from_euler(x_radian: f64, y_radian: f64, z_radian: f64) -> Quaternion {
		let half_x = x_radian * 0.5;
		let half_y = y_radian * 0.5;
		let half_z = z_radian * 0.5;
		let cosine_x = half_x.cos();
		let sine_x = half_x.sin();
		let cosine_y = half_y.cos();
		let sine_y = half_y.sin();
		let cosine_z = half_z.cos();
		let sine_z = half_z.sin();
		Quaternion::create(
			sine_x * cosine_y * cosine_z + cosine_x * sine_y * sine_z,
			cosine_x * sine_y * cosine_z - sine_x * cosine_y * sine_z,
			cosine_x * cosine_y * sine_z + sine_x * sine_y * cosine_z,
			cosine_x * cosine_y * cosine_z - sine_x * sine_y * sine_z,
		)
	}

	//==============================================================================
	/// 구면 선형 보간. (최단 경로, 거의 같은 방향이면 선형 보간 뒤 정규화한다)
	///
	/// # 인자
	/// - `from`: 시작 회전.
	/// - `to`: 끝 회전. (from 과 내적이 음수면 부호를 뒤집어 짧은 쪽으로 간다)
	/// - `normalized_time`: 보간 비율. (0 이면 from, 1 이면 to 와 같은 회전)
	///
	/// # 돌려줌
	/// 보간한 회전. (정규화해 돌려준다)
	///
	/// # 예제
	/// 부호가 반대인 끝 회전도 짧은 쪽으로 간다
	/// ```rust
	/// use vanilla::Quaternion;
	///
	/// let diagonal_component = std::f64::consts::FRAC_1_SQRT_2;
	/// let to = Quaternion::create(0.0, 0.0, -diagonal_component, -diagonal_component);
	/// let middle = Quaternion::slerp(Quaternion::identity(), to, 0.5);
	/// assert!((middle.z - 0.38268343).abs() < 1e-5);
	/// assert!((middle.w - 0.9238795).abs() < 1e-5);
	/// ```
	//==============================================================================
	pub fn slerp(from: Quaternion, to: Quaternion, normalized_time: f64) -> Quaternion {
		let mut dot_product = from.x * to.x + from.y * to.y + from.z * to.z + from.w * to.w;
		let mut to_x = to.x;
		let mut to_y = to.y;
		let mut to_z = to.z;
		let mut to_w = to.w;
		if dot_product < 0.0 {
			dot_product = -dot_product;
			to_x = -to_x;
			to_y = -to_y;
			to_z = -to_z;
			to_w = -to_w;
		}

		let mut from_weight = 1.0 - normalized_time;
		let mut to_weight = normalized_time;
		if dot_product < 0.9995 {
			let angle = math::clamp(dot_product, -1.0, 1.0).acos();
			let sine_angle = angle.sin();
			from_weight = ((1.0 - normalized_time) * angle).sin() / sine_angle;
			to_weight = (normalized_time * angle).sin() / sine_angle;
		}

		let blended = Quaternion::create(
			from.x * from_weight + to_x * to_weight,
			from.y * from_weight + to_y * to_weight,
			from.z * from_weight + to_z * to_weight,
			from.w * from_weight + to_w * to_weight,
		);
		blended.normalize()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::matrix4::Matrix4;

	/// 셈 차례가 다른 기대값과의 끝자리 차이를 받아 줄 허용 오차.
	const TOLERANCE: f64 = 1.0e-5;

	//==============================================================================
	/// 쿼터니언이 기대값(원본이 찍은 f64 값)과 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assert_quaternion_near(actual: Quaternion, expected: [f64; 4]) {
		let actual_values = [actual.x, actual.y, actual.z, actual.w];
		for (index, (actual_value, expected_value)) in actual_values.iter().zip(expected.iter()).enumerate() {
			let difference = (*actual_value - *expected_value).abs();
			assert!(difference <= TOLERANCE, "component {} = {}, expected {} (all: {:?})", index, actual_value, expected_value, actual);
		}
	}

	//==============================================================================
	/// 두 행렬이 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assert_matrix_near(actual: &Matrix4, expected: &Matrix4) {
		for (index, (actual_value, expected_value)) in actual.elements.iter().zip(expected.elements.iter()).enumerate() {
			let difference = (actual_value - expected_value).abs();
			assert!(difference <= TOLERANCE as f32, "elements[{}] = {}, expected {}", index, actual_value, expected_value);
		}
	}

	//==============================================================================
	/// 오일러 변환이 원본과 같은지. (node 로 돌린 원본 값)
	//==============================================================================
	#[test]
	fn create_from_euler_matches_original() {
		let quaternion = Quaternion::create_from_euler(0.3, -0.7, 1.1);
		assert_quaternion_near(quaternion, [-0.057539988180335344, -0.36242009435522565, 0.4417996722272437, 0.818629265655496]);

		let product = Quaternion::create_from_euler(0.3, 0.0, 0.0).multiply(Quaternion::create_from_euler(0.0, 0.4, 0.0));
		assert_quaternion_near(product, [0.14645931909238652, 0.19643848836306488, 0.029688773773793673, 0.9690614866211726]);
	}

	//==============================================================================
	/// 쿼터니언 → 행렬: 축 하나짜리 오일러는 create_rotation_x / Y / Z 와 같고, 원본 값과도 같다.
	//==============================================================================
	#[test]
	fn quaternion_to_matrix() {
		let radian = 0.83;
		assert_matrix_near(&Matrix4::create_rotation_from_quaternion(Quaternion::create_from_euler(radian, 0.0, 0.0)), &Matrix4::create_rotation_x(radian));
		assert_matrix_near(&Matrix4::create_rotation_from_quaternion(Quaternion::create_from_euler(0.0, radian, 0.0)), &Matrix4::create_rotation_y(radian));
		assert_matrix_near(&Matrix4::create_rotation_from_quaternion(Quaternion::create_from_euler(0.0, 0.0, radian)), &Matrix4::create_rotation_z(radian));
		assert_eq!(Matrix4::create_rotation_from_quaternion(Quaternion::identity()), Matrix4::create_identity());

		let matrix = Matrix4::create_rotation_from_quaternion(Quaternion::create_from_euler(0.3, -0.7, 1.1));
		let expected = [0.3469294607639313, 0.765047550201416, 0.5425330996513367, 0.0, -0.6816329956054688, 0.6030043959617615, -0.4144420027732849, 0.0, -0.6442176699638367, -0.22602632641792297, 0.7306816577911377, 0.0, 0.0, 0.0, 0.0, 1.0];
		for (index, (actual_value, expected_value)) in matrix.elements.iter().zip(expected.iter()).enumerate() {
			let difference = (*actual_value as f64 - *expected_value).abs();
			assert!(difference <= TOLERANCE, "elements[{}] = {}, expected {}", index, actual_value, expected_value);
		}
	}

	//==============================================================================
	/// 오일러 순서 왕복: create_from_euler(x, y, z) 의 행렬은 Rx x Ry x Rz (rotate_x, rotate_y, rotate_z 를 차례로 누적한 것)와 같다.
	/// 쿼터니언 곱의 행렬은 행렬 곱과 같다.
	//==============================================================================
	#[test]
	fn euler_order_round_trip() {
		let angles = [(0.3f64, -0.7f64, 1.1f64), (-1.2, 0.4, 2.5), (2.0, 1.0, -0.6)];
		for (x_radian, y_radian, z_radian) in angles {
			let mut composed = Matrix4::create_identity();
			composed.rotate_x(x_radian);
			composed.rotate_y(y_radian);
			composed.rotate_z(z_radian);
			let from_quaternion = Matrix4::create_rotation_from_quaternion(Quaternion::create_from_euler(x_radian, y_radian, z_radian));
			assert_matrix_near(&from_quaternion, &composed);

			let quaternion_x = Quaternion::create_from_euler(x_radian, 0.0, 0.0);
			let quaternion_y = Quaternion::create_from_euler(0.0, y_radian, 0.0);
			let quaternion_z = Quaternion::create_from_euler(0.0, 0.0, z_radian);
			let multiplied = quaternion_x.multiply(quaternion_y).multiply(quaternion_z);
			let from_euler = Quaternion::create_from_euler(x_radian, y_radian, z_radian);
			assert_quaternion_near(multiplied, [from_euler.x, from_euler.y, from_euler.z, from_euler.w]);

			let mut matrix_product = Matrix4::create_rotation_from_quaternion(quaternion_z);
			matrix_product.multiply(&Matrix4::create_rotation_from_quaternion(quaternion_x));
			assert_matrix_near(&Matrix4::create_rotation_from_quaternion(quaternion_z.multiply(quaternion_x)), &matrix_product);
		}
	}

	//==============================================================================
	/// 구면 선형 보간: 원본 값(일반, 거의 같은 방향, 반대 부호 최단 경로)과 양 끝.
	//==============================================================================
	#[test]
	fn slerp_matches_original() {
		let from = Quaternion::create_from_euler(0.1, 0.2, 0.3);
		let to = Quaternion::create_from_euler(1.0, -0.5, 2.0);
		assert_quaternion_near(Quaternion::slerp(from, to, 0.35), [0.07247359830822624, -0.13756324880336232, 0.3675390346169097, 0.9169182014551747]);
		assert_quaternion_near(Quaternion::slerp(from, to, 0.0), [from.x, from.y, from.z, from.w]);
		assert_quaternion_near(Quaternion::slerp(from, to, 1.0), [to.x, to.y, to.z, to.w]);

		let near = Quaternion::slerp(Quaternion::identity(), Quaternion::create(0.01, 0.0, 0.0, 0.99995), 0.5);
		assert_quaternion_near(near, [0.0050000624996093404, 0.0, 0.0, 0.9999874996093701]);

		let flip = Quaternion::slerp(Quaternion::identity(), Quaternion::create(0.0, 0.0, -std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2), 0.5);
		assert_quaternion_near(flip, [0.0, 0.0, 0.3826834323650898, 0.9238795325112867]);
	}

	//==============================================================================
	/// 정규화: 길이 1, 영 쿼터니언은 단위 쿼터니언.
	//==============================================================================
	#[test]
	fn normalize_and_default() {
		let normalized = Quaternion::create(0.0, 3.0, 0.0, 4.0).normalize();
		assert_eq!(normalized, Quaternion::create(0.0, 0.6, 0.0, 0.8));
		assert_eq!(Quaternion::create(0.0, 0.0, 0.0, 0.0).normalize(), Quaternion::identity());
		assert_eq!(Quaternion::default(), Quaternion::identity());
		let mut quaternion = Quaternion::default();
		quaternion.set(1.0, 2.0, 3.0, 4.0);
		assert_eq!(quaternion, Quaternion::create(1.0, 2.0, 3.0, 4.0));
	}
}
