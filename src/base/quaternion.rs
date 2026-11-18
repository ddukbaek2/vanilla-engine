//==============================================================================
// 쿼터니언. (원본 base/quaternion.js, 3차원 회전이고 골격 애니메이션 보간의 기본 단위다)
// - 성분 순서는 원본과 같이 (x, y, z, w) 이고, w 가 실수부다. 기본값은 단위 쿼터니언 (0, 0, 0, 1) 이다.
// - multiply 는 해밀턴 곱 this x other 다. 행렬로 바꾸면 R(this x other) = R(this) x R(other) 라서,
//   열벡터에는 other 회전이 먼저 걸린다. (원본 말로 "this 회전 뒤에 other 회전을 로컬로 누적")
// - createFromEuler(x, y, z) 는 라디안이고 qX x qY x qZ 와 같다. 행렬로는 Rx x Ry x Rz 라서 점에는 Z, Y, X 순서로 걸린다.
//   (three.js 의 'XYZ' 순서와 같은 식이다)
// - 원본은 x, y, z, w 를 감춘 필드와 프로퍼티로 두지만, 러스트 판은 공개 필드를 둔 Copy 값이다. 원본 clone 은 복사로 갈음한다.
// - 원본 수는 f64 이고 러스트 판은 f32 다.
// - 원본에 없는 행렬에서 쿼터니언으로 바꾸기, 오일러로 되돌리기는 만들지 않았다. (행렬로는 Matrix4::createRotationFromQuaternion)
//==============================================================================
use crate::base::math;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
	pub x: f32,
	pub y: f32,
	pub z: f32,
	pub w: f32,
}

impl Default for Quaternion {
	//==============================================================================
	// 단위 쿼터니언. (원본 new Quaternion())
	//==============================================================================
	fn default() -> Quaternion {
		Quaternion::identity()
	}
}

impl Quaternion {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub const fn create(x: f32, y: f32, z: f32, w: f32) -> Quaternion {
		Quaternion { x, y, z, w }
	}

	//==============================================================================
	// 단위 쿼터니언. (0, 0, 0, 1)
	//==============================================================================
	pub const fn identity() -> Quaternion {
		Quaternion { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
	}

	//==============================================================================
	// 대입.
	//==============================================================================
	pub fn set(&mut self, x: f32, y: f32, z: f32, w: f32) {
		self.x = x;
		self.y = y;
		self.z = z;
		self.w = w;
	}

	//==============================================================================
	// 정규화. (길이가 0 이하이면 단위 쿼터니언)
	//==============================================================================
	pub fn normalize(&self) -> Quaternion {
		let length = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
		if length > 0.0 {
			return Quaternion::create(self.x / length, self.y / length, self.z / length, self.w / length);
		}
		Quaternion::identity()
	}

	//==============================================================================
	// 곱하기. (해밀턴 곱 this x other, this 회전 뒤에 other 회전을 로컬로 누적한다)
	//==============================================================================
	pub fn multiply(&self, other: Quaternion) -> Quaternion {
		let resultX = self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y;
		let resultY = self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x;
		let resultZ = self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w;
		let resultW = self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z;
		Quaternion::create(resultX, resultY, resultZ, resultW)
	}

	//==============================================================================
	// 오일러 각으로 만든다. (XYZ 순서 라디안, qX x qY x qZ 와 같다)
	//==============================================================================
	pub fn createFromEuler(xRadian: f32, yRadian: f32, zRadian: f32) -> Quaternion {
		let halfX = xRadian * 0.5;
		let halfY = yRadian * 0.5;
		let halfZ = zRadian * 0.5;
		let cosineX = halfX.cos();
		let sineX = halfX.sin();
		let cosineY = halfY.cos();
		let sineY = halfY.sin();
		let cosineZ = halfZ.cos();
		let sineZ = halfZ.sin();
		Quaternion::create(
			sineX * cosineY * cosineZ + cosineX * sineY * sineZ,
			cosineX * sineY * cosineZ - sineX * cosineY * sineZ,
			cosineX * cosineY * sineZ + sineX * sineY * cosineZ,
			cosineX * cosineY * cosineZ - sineX * sineY * sineZ,
		)
	}

	//==============================================================================
	// 구면 선형 보간. (최단 경로, 거의 같은 방향이면 선형 보간 뒤 정규화한다)
	//==============================================================================
	pub fn slerp(from: Quaternion, to: Quaternion, normalizedTime: f32) -> Quaternion {
		let mut dotProduct = from.x * to.x + from.y * to.y + from.z * to.z + from.w * to.w;
		let mut toX = to.x;
		let mut toY = to.y;
		let mut toZ = to.z;
		let mut toW = to.w;
		if dotProduct < 0.0 {
			dotProduct = -dotProduct;
			toX = -toX;
			toY = -toY;
			toZ = -toZ;
			toW = -toW;
		}

		let mut fromWeight = 1.0 - normalizedTime;
		let mut toWeight = normalizedTime;
		if dotProduct < 0.9995 {
			let angle = math::clamp(dotProduct, -1.0, 1.0).acos();
			let sineAngle = angle.sin();
			fromWeight = ((1.0 - normalizedTime) * angle).sin() / sineAngle;
			toWeight = (normalizedTime * angle).sin() / sineAngle;
		}

		let blended = Quaternion::create(
			from.x * fromWeight + toX * toWeight,
			from.y * fromWeight + toY * toWeight,
			from.z * fromWeight + toZ * toWeight,
			from.w * fromWeight + toW * toWeight,
		);
		blended.normalize()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::matrix4::Matrix4;

	// 원본(f64)과 러스트 판(f32)의 끝자리 차이를 받아 줄 허용 오차.
	const TOLERANCE: f32 = 1.0e-5;

	//==============================================================================
	// 쿼터니언이 기대값(원본이 찍은 f64 값)과 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assertQuaternionNear(actual: Quaternion, expected: [f64; 4]) {
		let actualValues = [actual.x, actual.y, actual.z, actual.w];
		for (index, (actualValue, expectedValue)) in actualValues.iter().zip(expected.iter()).enumerate() {
			let difference = (*actualValue as f64 - *expectedValue).abs();
			assert!(difference <= TOLERANCE as f64, "component {} = {}, expected {} (all: {:?})", index, actualValue, expectedValue, actual);
		}
	}

	//==============================================================================
	// 두 행렬이 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assertMatrixNear(actual: &Matrix4, expected: &Matrix4) {
		for (index, (actualValue, expectedValue)) in actual.elements.iter().zip(expected.elements.iter()).enumerate() {
			let difference = (actualValue - expectedValue).abs();
			assert!(difference <= TOLERANCE, "elements[{}] = {}, expected {}", index, actualValue, expectedValue);
		}
	}

	//==============================================================================
	// 오일러 변환이 원본과 같은지. (node 로 돌린 원본 값)
	//==============================================================================
	#[test]
	fn createFromEulerMatchesOriginal() {
		let quaternion = Quaternion::createFromEuler(0.3, -0.7, 1.1);
		assertQuaternionNear(quaternion, [-0.057539988180335344, -0.36242009435522565, 0.4417996722272437, 0.818629265655496]);

		let product = Quaternion::createFromEuler(0.3, 0.0, 0.0).multiply(Quaternion::createFromEuler(0.0, 0.4, 0.0));
		assertQuaternionNear(product, [0.14645931909238652, 0.19643848836306488, 0.029688773773793673, 0.9690614866211726]);
	}

	//==============================================================================
	// 쿼터니언 → 행렬: 축 하나짜리 오일러는 createRotationX / Y / Z 와 같고, 원본 값과도 같다.
	//==============================================================================
	#[test]
	fn quaternionToMatrix() {
		let radian = 0.83;
		assertMatrixNear(&Matrix4::createRotationFromQuaternion(Quaternion::createFromEuler(radian, 0.0, 0.0)), &Matrix4::createRotationX(radian));
		assertMatrixNear(&Matrix4::createRotationFromQuaternion(Quaternion::createFromEuler(0.0, radian, 0.0)), &Matrix4::createRotationY(radian));
		assertMatrixNear(&Matrix4::createRotationFromQuaternion(Quaternion::createFromEuler(0.0, 0.0, radian)), &Matrix4::createRotationZ(radian));
		assert_eq!(Matrix4::createRotationFromQuaternion(Quaternion::identity()), Matrix4::createIdentity());

		let matrix = Matrix4::createRotationFromQuaternion(Quaternion::createFromEuler(0.3, -0.7, 1.1));
		let expected = [0.3469294607639313, 0.765047550201416, 0.5425330996513367, 0.0, -0.6816329956054688, 0.6030043959617615, -0.4144420027732849, 0.0, -0.6442176699638367, -0.22602632641792297, 0.7306816577911377, 0.0, 0.0, 0.0, 0.0, 1.0];
		for (index, (actualValue, expectedValue)) in matrix.elements.iter().zip(expected.iter()).enumerate() {
			let difference = (*actualValue as f64 - *expectedValue).abs();
			assert!(difference <= TOLERANCE as f64, "elements[{}] = {}, expected {}", index, actualValue, expectedValue);
		}
	}

	//==============================================================================
	// 오일러 순서 왕복: createFromEuler(x, y, z) 의 행렬은 Rx x Ry x Rz (rotateX, rotateY, rotateZ 를 차례로 누적한 것)와 같다.
	// 쿼터니언 곱의 행렬은 행렬 곱과 같다.
	//==============================================================================
	#[test]
	fn eulerOrderRoundTrip() {
		let angles = [(0.3f32, -0.7f32, 1.1f32), (-1.2, 0.4, 2.5), (2.0, 1.0, -0.6)];
		for (xRadian, yRadian, zRadian) in angles {
			let mut composed = Matrix4::createIdentity();
			composed.rotateX(xRadian);
			composed.rotateY(yRadian);
			composed.rotateZ(zRadian);
			let fromQuaternion = Matrix4::createRotationFromQuaternion(Quaternion::createFromEuler(xRadian, yRadian, zRadian));
			assertMatrixNear(&fromQuaternion, &composed);

			let quaternionX = Quaternion::createFromEuler(xRadian, 0.0, 0.0);
			let quaternionY = Quaternion::createFromEuler(0.0, yRadian, 0.0);
			let quaternionZ = Quaternion::createFromEuler(0.0, 0.0, zRadian);
			let multiplied = quaternionX.multiply(quaternionY).multiply(quaternionZ);
			let fromEuler = Quaternion::createFromEuler(xRadian, yRadian, zRadian);
			assertQuaternionNear(multiplied, [fromEuler.x as f64, fromEuler.y as f64, fromEuler.z as f64, fromEuler.w as f64]);

			let mut matrixProduct = Matrix4::createRotationFromQuaternion(quaternionZ);
			matrixProduct.multiply(&Matrix4::createRotationFromQuaternion(quaternionX));
			assertMatrixNear(&Matrix4::createRotationFromQuaternion(quaternionZ.multiply(quaternionX)), &matrixProduct);
		}
	}

	//==============================================================================
	// 구면 선형 보간: 원본 값(일반, 거의 같은 방향, 반대 부호 최단 경로)과 양 끝.
	//==============================================================================
	#[test]
	fn slerpMatchesOriginal() {
		let from = Quaternion::createFromEuler(0.1, 0.2, 0.3);
		let to = Quaternion::createFromEuler(1.0, -0.5, 2.0);
		assertQuaternionNear(Quaternion::slerp(from, to, 0.35), [0.07247359830822624, -0.13756324880336232, 0.3675390346169097, 0.9169182014551747]);
		assertQuaternionNear(Quaternion::slerp(from, to, 0.0), [from.x as f64, from.y as f64, from.z as f64, from.w as f64]);
		assertQuaternionNear(Quaternion::slerp(from, to, 1.0), [to.x as f64, to.y as f64, to.z as f64, to.w as f64]);

		let near = Quaternion::slerp(Quaternion::identity(), Quaternion::create(0.01, 0.0, 0.0, 0.99995), 0.5);
		assertQuaternionNear(near, [0.0050000624996093404, 0.0, 0.0, 0.9999874996093701]);

		let flip = Quaternion::slerp(Quaternion::identity(), Quaternion::create(0.0, 0.0, -std::f32::consts::FRAC_1_SQRT_2, -std::f32::consts::FRAC_1_SQRT_2), 0.5);
		assertQuaternionNear(flip, [0.0, 0.0, 0.3826834323650898, 0.9238795325112867]);
	}

	//==============================================================================
	// 정규화: 길이 1, 영 쿼터니언은 단위 쿼터니언.
	//==============================================================================
	#[test]
	fn normalizeAndDefault() {
		let normalized = Quaternion::create(0.0, 3.0, 0.0, 4.0).normalize();
		assert_eq!(normalized, Quaternion::create(0.0, 0.6, 0.0, 0.8));
		assert_eq!(Quaternion::create(0.0, 0.0, 0.0, 0.0).normalize(), Quaternion::identity());
		assert_eq!(Quaternion::default(), Quaternion::identity());
		let mut quaternion = Quaternion::default();
		quaternion.set(1.0, 2.0, 3.0, 4.0);
		assert_eq!(quaternion, Quaternion::create(1.0, 2.0, 3.0, 4.0));
	}
}
