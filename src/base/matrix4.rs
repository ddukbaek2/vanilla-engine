//==============================================================================
// 4x4 변환 행렬. (원본 base/matrix4.js)
// - 원본과 같이 열 우선이다. 요소 자리는 elements[column * 4 + row] 이고, glUniformMatrix4fv 에 전치 없이 그대로 올린다.
// - 점 변환은 transformed = matrix x (x, y, z, 1) 이다. (열벡터를 오른쪽에 곱한다)
// - multiply / translate / rotateX ... / scale 은 원본처럼 자기 자신을 바꾼다. (this = this x other, 로컬 공간 오른쪽 곱)
//   create... 정적 함수는 원본처럼 새 행렬을 돌려준다.
// - 좌표계는 원본과 같이 오른손이다. createPerspective 는 OpenGL 식(카메라는 -Z 를 보고, 깊이는 -1 ~ 1),
//   createLookAt 은 눈에서 대상 반대쪽이 +Z(뒤쪽 축)인 뷰 행렬, 회전은 라디안이고 축을 바라볼 때 반시계가 양수다.
// - 원본 getElements() 는 Float32Array 를 내주고 호출자가 그 안을 직접 고친다. 러스트 판은 elements 를 공개 필드로 두고,
//   getElements() 는 읽기 참조를, writeToArray / writeToSlice 는 유니폼 배열에 옮겨 쓰는 일을 맡는다.
// - 원본은 Float32Array(f32)에 담되 계산은 f64 로 한다. 러스트 판은 담기와 계산 모두 f32 라 끝자리가 조금 다를 수 있다.
// - 원본 transformPoint 의 { x, y, z, w } 는 TransformedPoint 로, transformDirection 의 { x, y, z } 는 Vector3 로 돌려준다.
// - 원본 clone 은 Copy 로 갈음한다. 원본에 없는 전치(transpose)는 만들지 않았다.
//==============================================================================
use crate::base::quaternion::Quaternion;
use crate::base::vector3::Vector3;

//==============================================================================
// 동차 좌표 점. (원본 transformPoint 가 돌려주는 { x, y, z, w }, 원근 나누기는 호출자가 한다)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TransformedPoint {
	pub x: f32,
	pub y: f32,
	pub z: f32,
	pub w: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4 {
	// 열 우선 요소 배열. (elements[column * 4 + row])
	pub elements: [f32; 16],
}

impl Default for Matrix4 {
	//==============================================================================
	// 단위 행렬. (원본 new Matrix4())
	//==============================================================================
	fn default() -> Matrix4 {
		Matrix4::createIdentity()
	}
}

impl Matrix4 {
	//==============================================================================
	// 단위 행렬로 초기화.
	//==============================================================================
	pub fn setIdentity(&mut self) {
		*self = Matrix4::createIdentity();
	}

	//==============================================================================
	// 다른 행렬로부터 복사.
	//==============================================================================
	pub fn copyFrom(&mut self, other: &Matrix4) {
		self.elements = other.elements;
	}

	//==============================================================================
	// 다른 행렬을 오른쪽에 곱해 누적한다. (this = this x other)
	//==============================================================================
	pub fn multiply(&mut self, other: &Matrix4) {
		let elements = &self.elements;
		let otherElements = &other.elements;
		let mut resultElements = [0.0f32; 16];
		for columnIndex in 0..4 {
			for rowIndex in 0..4 {
				let mut sum = 0.0f32;
				for termIndex in 0..4 {
					sum += elements[termIndex * 4 + rowIndex] * otherElements[columnIndex * 4 + termIndex];
				}
				resultElements[columnIndex * 4 + rowIndex] = sum;
			}
		}
		self.elements = resultElements;
	}

	//==============================================================================
	// 이동 변환을 누적한다. (로컬 공간 오른쪽 곱)
	//==============================================================================
	pub fn translate(&mut self, x: f32, y: f32, z: f32) {
		let translationMatrix = Matrix4::createTranslation(x, y, z);
		self.multiply(&translationMatrix);
	}

	//==============================================================================
	// X 축 회전 변환을 누적한다. (라디안, 로컬 공간 오른쪽 곱)
	//==============================================================================
	pub fn rotateX(&mut self, radian: f32) {
		let rotationMatrix = Matrix4::createRotationX(radian);
		self.multiply(&rotationMatrix);
	}

	//==============================================================================
	// Y 축 회전 변환을 누적한다. (라디안, 로컬 공간 오른쪽 곱)
	//==============================================================================
	pub fn rotateY(&mut self, radian: f32) {
		let rotationMatrix = Matrix4::createRotationY(radian);
		self.multiply(&rotationMatrix);
	}

	//==============================================================================
	// Z 축 회전 변환을 누적한다. (라디안, 로컬 공간 오른쪽 곱)
	//==============================================================================
	pub fn rotateZ(&mut self, radian: f32) {
		let rotationMatrix = Matrix4::createRotationZ(radian);
		self.multiply(&rotationMatrix);
	}

	//==============================================================================
	// 크기 변환을 누적한다. (로컬 공간 오른쪽 곱)
	//==============================================================================
	pub fn scale(&mut self, x: f32, y: f32, z: f32) {
		let scaleMatrix = Matrix4::createScale(x, y, z);
		self.multiply(&scaleMatrix);
	}

	//==============================================================================
	// 점을 변환한다. (w 성분까지 돌려주고, 원근 나누기는 호출자가 한다)
	//==============================================================================
	pub fn transformPoint(&self, x: f32, y: f32, z: f32) -> TransformedPoint {
		let elements = &self.elements;
		let transformedX = elements[0] * x + elements[4] * y + elements[8] * z + elements[12];
		let transformedY = elements[1] * x + elements[5] * y + elements[9] * z + elements[13];
		let transformedZ = elements[2] * x + elements[6] * y + elements[10] * z + elements[14];
		let transformedW = elements[3] * x + elements[7] * y + elements[11] * z + elements[15];
		TransformedPoint { x: transformedX, y: transformedY, z: transformedZ, w: transformedW }
	}

	//==============================================================================
	// 방향을 변환한다. (이동 성분은 빼고 회전과 크기만 적용한다)
	//==============================================================================
	pub fn transformDirection(&self, x: f32, y: f32, z: f32) -> Vector3 {
		let elements = &self.elements;
		let transformedX = elements[0] * x + elements[4] * y + elements[8] * z;
		let transformedY = elements[1] * x + elements[5] * y + elements[9] * z;
		let transformedZ = elements[2] * x + elements[6] * y + elements[10] * z;
		Vector3::create(transformedX, transformedY, transformedZ)
	}

	//==============================================================================
	// 열 우선 요소 배열을 돌려준다. (고칠 때는 elements 필드를 직접 쓴다)
	//==============================================================================
	pub fn getElements(&self) -> &[f32; 16] {
		&self.elements
	}

	//==============================================================================
	// 셰이더 mat4 (열 우선) 배열로 기록한다.
	//==============================================================================
	pub fn writeToArray(&self, array: &mut [f32; 16]) {
		array.copy_from_slice(&self.elements);
	}

	//==============================================================================
	// 큰 배열의 offset 자리부터 16 개를 기록한다. (원본 jointMatrixArray.set(elements, jointIndex * 16) 자리)
	//==============================================================================
	pub fn writeToSlice(&self, array: &mut [f32], offset: usize) {
		array[offset..offset + 16].copy_from_slice(&self.elements);
	}

	//==============================================================================
	// 단위 행렬을 만든다.
	//==============================================================================
	pub const fn createIdentity() -> Matrix4 {
		Matrix4 {
			elements: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
		}
	}

	//==============================================================================
	// 이동 행렬을 만든다.
	//==============================================================================
	pub fn createTranslation(x: f32, y: f32, z: f32) -> Matrix4 {
		let mut matrix = Matrix4::createIdentity();
		matrix.elements[12] = x;
		matrix.elements[13] = y;
		matrix.elements[14] = z;
		matrix
	}

	//==============================================================================
	// X 축 회전 행렬을 만든다. (라디안)
	//==============================================================================
	pub fn createRotationX(radian: f32) -> Matrix4 {
		let cosine = radian.cos();
		let sine = radian.sin();
		let mut matrix = Matrix4::createIdentity();
		matrix.elements[5] = cosine;
		matrix.elements[6] = sine;
		matrix.elements[9] = -sine;
		matrix.elements[10] = cosine;
		matrix
	}

	//==============================================================================
	// Y 축 회전 행렬을 만든다. (라디안)
	//==============================================================================
	pub fn createRotationY(radian: f32) -> Matrix4 {
		let cosine = radian.cos();
		let sine = radian.sin();
		let mut matrix = Matrix4::createIdentity();
		matrix.elements[0] = cosine;
		matrix.elements[2] = -sine;
		matrix.elements[8] = sine;
		matrix.elements[10] = cosine;
		matrix
	}

	//==============================================================================
	// Z 축 회전 행렬을 만든다. (라디안)
	//==============================================================================
	pub fn createRotationZ(radian: f32) -> Matrix4 {
		let cosine = radian.cos();
		let sine = radian.sin();
		let mut matrix = Matrix4::createIdentity();
		matrix.elements[0] = cosine;
		matrix.elements[1] = sine;
		matrix.elements[4] = -sine;
		matrix.elements[5] = cosine;
		matrix
	}

	//==============================================================================
	// 쿼터니언 회전 행렬을 만든다. (단위 쿼터니언을 가정한다)
	//==============================================================================
	pub fn createRotationFromQuaternion(quaternion: Quaternion) -> Matrix4 {
		let x = quaternion.x;
		let y = quaternion.y;
		let z = quaternion.z;
		let w = quaternion.w;
		let mut matrix = Matrix4::createIdentity();
		let elements = &mut matrix.elements;
		elements[0] = 1.0 - 2.0 * (y * y + z * z);
		elements[1] = 2.0 * (x * y + z * w);
		elements[2] = 2.0 * (x * z - y * w);
		elements[4] = 2.0 * (x * y - z * w);
		elements[5] = 1.0 - 2.0 * (x * x + z * z);
		elements[6] = 2.0 * (y * z + x * w);
		elements[8] = 2.0 * (x * z + y * w);
		elements[9] = 2.0 * (y * z - x * w);
		elements[10] = 1.0 - 2.0 * (x * x + y * y);
		matrix
	}

	//==============================================================================
	// 크기 행렬을 만든다.
	//==============================================================================
	pub fn createScale(x: f32, y: f32, z: f32) -> Matrix4 {
		let mut matrix = Matrix4::createIdentity();
		matrix.elements[0] = x;
		matrix.elements[5] = y;
		matrix.elements[10] = z;
		matrix
	}

	//==============================================================================
	// 원근 투영 행렬을 만든다. (세로 시야각 라디안, OpenGL 식 깊이 -1 ~ 1)
	//==============================================================================
	pub fn createPerspective(fieldOfViewRadian: f32, aspectRatio: f32, nearDistance: f32, farDistance: f32) -> Matrix4 {
		let focalLength = 1.0 / (fieldOfViewRadian / 2.0).tan();
		let rangeInverse = 1.0 / (nearDistance - farDistance);
		let mut matrix = Matrix4 { elements: [0.0; 16] };
		let elements = &mut matrix.elements;
		elements[0] = focalLength / aspectRatio;
		elements[5] = focalLength;
		elements[10] = (nearDistance + farDistance) * rangeInverse;
		elements[11] = -1.0;
		elements[14] = nearDistance * farDistance * rangeInverse * 2.0;
		matrix
	}

	//==============================================================================
	// 직교 투영 행렬을 만든다. (OpenGL glOrtho 와 같은 식)
	//==============================================================================
	pub fn createOrthographic(left: f32, right: f32, bottom: f32, top: f32, nearDistance: f32, farDistance: f32) -> Matrix4 {
		let mut matrix = Matrix4 { elements: [0.0; 16] };
		let elements = &mut matrix.elements;
		elements[0] = 2.0 / (right - left);
		elements[5] = 2.0 / (top - bottom);
		elements[10] = -2.0 / (farDistance - nearDistance);
		elements[12] = -(right + left) / (right - left);
		elements[13] = -(top + bottom) / (top - bottom);
		elements[14] = -(farDistance + nearDistance) / (farDistance - nearDistance);
		elements[15] = 1.0;
		matrix
	}

	//==============================================================================
	// 시점 행렬을 만든다. (eyePosition 에서 targetPosition 을 바라보는 뷰 행렬, 뒤쪽 축 = 눈 - 대상)
	//==============================================================================
	pub fn createLookAt(eyePosition: Vector3, targetPosition: Vector3, upDirection: Vector3) -> Matrix4 {
		let eyeToTarget = eyePosition.subtract(targetPosition);
		let backAxis = eyeToTarget.normalize();
		let upCrossBack = upDirection.cross(backAxis);
		let rightAxis = upCrossBack.normalize();
		let upAxis = backAxis.cross(rightAxis);

		let mut matrix = Matrix4::createIdentity();
		let elements = &mut matrix.elements;
		elements[0] = rightAxis.x;
		elements[1] = upAxis.x;
		elements[2] = backAxis.x;
		elements[3] = 0.0;
		elements[4] = rightAxis.y;
		elements[5] = upAxis.y;
		elements[6] = backAxis.y;
		elements[7] = 0.0;
		elements[8] = rightAxis.z;
		elements[9] = upAxis.z;
		elements[10] = backAxis.z;
		elements[11] = 0.0;
		elements[12] = -rightAxis.dot(eyePosition);
		elements[13] = -upAxis.dot(eyePosition);
		elements[14] = -backAxis.dot(eyePosition);
		elements[15] = 1.0;
		matrix
	}

	//==============================================================================
	// 역행렬을 만든다. (일반 4x4 여인수 전개, 행렬식이 0 이면 단위 행렬)
	//==============================================================================
	pub fn createInverse(matrix: &Matrix4) -> Matrix4 {
		let sourceElements = &matrix.elements;
		let m00 = sourceElements[0];
		let m01 = sourceElements[1];
		let m02 = sourceElements[2];
		let m03 = sourceElements[3];
		let m04 = sourceElements[4];
		let m05 = sourceElements[5];
		let m06 = sourceElements[6];
		let m07 = sourceElements[7];
		let m08 = sourceElements[8];
		let m09 = sourceElements[9];
		let m10 = sourceElements[10];
		let m11 = sourceElements[11];
		let m12 = sourceElements[12];
		let m13 = sourceElements[13];
		let m14 = sourceElements[14];
		let m15 = sourceElements[15];

		let cofactor00 = m00 * m05 - m01 * m04;
		let cofactor01 = m00 * m06 - m02 * m04;
		let cofactor02 = m00 * m07 - m03 * m04;
		let cofactor03 = m01 * m06 - m02 * m05;
		let cofactor04 = m01 * m07 - m03 * m05;
		let cofactor05 = m02 * m07 - m03 * m06;
		let cofactor06 = m08 * m13 - m09 * m12;
		let cofactor07 = m08 * m14 - m10 * m12;
		let cofactor08 = m08 * m15 - m11 * m12;
		let cofactor09 = m09 * m14 - m10 * m13;
		let cofactor10 = m09 * m15 - m11 * m13;
		let cofactor11 = m10 * m15 - m11 * m14;

		let determinant = cofactor00 * cofactor11 - cofactor01 * cofactor10 + cofactor02 * cofactor09 + cofactor03 * cofactor08 - cofactor04 * cofactor07 + cofactor05 * cofactor06;
		if determinant == 0.0 {
			return Matrix4::createIdentity();
		}
		let inverseDeterminant = 1.0 / determinant;

		let mut resultMatrix = Matrix4::createIdentity();
		let resultElements = &mut resultMatrix.elements;
		resultElements[0] = (m05 * cofactor11 - m06 * cofactor10 + m07 * cofactor09) * inverseDeterminant;
		resultElements[1] = (m02 * cofactor10 - m01 * cofactor11 - m03 * cofactor09) * inverseDeterminant;
		resultElements[2] = (m13 * cofactor05 - m14 * cofactor04 + m15 * cofactor03) * inverseDeterminant;
		resultElements[3] = (m10 * cofactor04 - m09 * cofactor05 - m11 * cofactor03) * inverseDeterminant;
		resultElements[4] = (m06 * cofactor08 - m04 * cofactor11 - m07 * cofactor07) * inverseDeterminant;
		resultElements[5] = (m00 * cofactor11 - m02 * cofactor08 + m03 * cofactor07) * inverseDeterminant;
		resultElements[6] = (m14 * cofactor02 - m12 * cofactor05 - m15 * cofactor01) * inverseDeterminant;
		resultElements[7] = (m08 * cofactor05 - m10 * cofactor02 + m11 * cofactor01) * inverseDeterminant;
		resultElements[8] = (m04 * cofactor10 - m05 * cofactor08 + m07 * cofactor06) * inverseDeterminant;
		resultElements[9] = (m01 * cofactor08 - m00 * cofactor10 - m03 * cofactor06) * inverseDeterminant;
		resultElements[10] = (m12 * cofactor04 - m13 * cofactor02 + m15 * cofactor00) * inverseDeterminant;
		resultElements[11] = (m09 * cofactor02 - m08 * cofactor04 - m11 * cofactor00) * inverseDeterminant;
		resultElements[12] = (m05 * cofactor07 - m04 * cofactor09 - m06 * cofactor06) * inverseDeterminant;
		resultElements[13] = (m00 * cofactor09 - m01 * cofactor07 + m02 * cofactor06) * inverseDeterminant;
		resultElements[14] = (m13 * cofactor01 - m12 * cofactor03 - m14 * cofactor00) * inverseDeterminant;
		resultElements[15] = (m08 * cofactor03 - m09 * cofactor01 + m10 * cofactor00) * inverseDeterminant;
		resultMatrix
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// 원본(f64 계산 뒤 Float32Array)과 러스트 판(f32 계산)의 끝자리 차이를 받아 줄 허용 오차.
	const TOLERANCE: f32 = 1.0e-5;

	//==============================================================================
	// 요소 배열이 기대값(원본이 찍은 f64 값)과 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assertElementsNear(actual: &[f32; 16], expected: &[f64; 16]) {
		for (index, (actualValue, expectedValue)) in actual.iter().zip(expected.iter()).enumerate() {
			let difference = (*actualValue as f64 - *expectedValue).abs();
			assert!(difference <= TOLERANCE as f64, "elements[{}] = {}, expected {} (all: {:?})", index, actualValue, expectedValue, actual);
		}
	}

	//==============================================================================
	// 값 하나가 기대값(원본이 찍은 f64 값)과 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assertNear(actual: f32, expected: f64) {
		let difference = (actual as f64 - expected).abs();
		assert!(difference <= TOLERANCE as f64, "{} expected {}", actual, expected);
	}

	//==============================================================================
	// 1 ~ 16 과 17 ~ 32 행렬의 곱. (손 계산 [0] = 1*17 + 5*18 + 9*19 + 13*20 = 538, [5] = 2*21 + 6*22 + 10*23 + 14*24 = 740, 나머지는 node 로 돌린 원본 값)
	//==============================================================================
	#[test]
	fn multiplyMatchesHandAndOriginal() {
		let mut left = Matrix4::createIdentity();
		let mut right = Matrix4::createIdentity();
		for index in 0..16 {
			left.elements[index] = (index + 1) as f32;
			right.elements[index] = (index + 17) as f32;
		}
		left.multiply(&right);
		let expected = [538.0, 612.0, 686.0, 760.0, 650.0, 740.0, 830.0, 920.0, 762.0, 868.0, 974.0, 1080.0, 874.0, 996.0, 1118.0, 1240.0];
		assert_eq!(left.elements, expected);
	}

	//==============================================================================
	// 곱하는 순서: translate 뒤 scale 은 이동 x 크기 (크기 먼저 적용), 반대는 이동도 커진다.
	//==============================================================================
	#[test]
	fn multiplyOrderIsLocalRightMultiply() {
		let mut translateThenScale = Matrix4::createTranslation(1.0, 2.0, 3.0);
		translateThenScale.scale(2.0, 3.0, 4.0);
		assert_eq!(translateThenScale.elements, [2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 1.0, 2.0, 3.0, 1.0]);

		let mut scaleThenTranslate = Matrix4::createScale(2.0, 3.0, 4.0);
		scaleThenTranslate.translate(1.0, 2.0, 3.0);
		assert_eq!(scaleThenTranslate.elements, [2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 2.0, 6.0, 12.0, 1.0]);

		let point = translateThenScale.transformPoint(1.0, 1.0, 1.0);
		assert_eq!(point, TransformedPoint { x: 3.0, y: 5.0, z: 7.0, w: 1.0 });
		let direction = translateThenScale.transformDirection(1.0, 1.0, 1.0);
		assert_eq!(direction, Vector3::create(2.0, 3.0, 4.0));
	}

	//==============================================================================
	// 회전 방향: Z 축 90 도는 +X 를 +Y 로, X 축 90 도는 +Y 를 +Z 로, Y 축 90 도는 +Z 를 +X 로 돌린다. (오른손, 반시계)
	//==============================================================================
	#[test]
	fn rotationIsRightHanded() {
		let quarterTurn = std::f32::consts::FRAC_PI_2;
		let rotatedByZ = Matrix4::createRotationZ(quarterTurn).transformDirection(1.0, 0.0, 0.0);
		assertNear(rotatedByZ.x, 0.0);
		assertNear(rotatedByZ.y, 1.0);
		assertNear(rotatedByZ.z, 0.0);
		let rotatedByX = Matrix4::createRotationX(quarterTurn).transformDirection(0.0, 1.0, 0.0);
		assertNear(rotatedByX.x, 0.0);
		assertNear(rotatedByX.y, 0.0);
		assertNear(rotatedByX.z, 1.0);
		let rotatedByY = Matrix4::createRotationY(quarterTurn).transformDirection(0.0, 0.0, 1.0);
		assertNear(rotatedByY.x, 1.0);
		assertNear(rotatedByY.y, 0.0);
		assertNear(rotatedByY.z, 0.0);
	}

	//==============================================================================
	// translate, rotateY, rotateX, scale 누적과 점, 방향 변환이 원본과 같은지. (node 로 돌린 원본 값)
	//==============================================================================
	#[test]
	fn composeMatchesOriginal() {
		let mut compose = Matrix4::createTranslation(1.0, 2.0, 3.0);
		compose.rotateY(0.5);
		compose.rotateX(-0.25);
		compose.scale(2.0, 3.0, 4.0);
		let expected = [1.7551651000976562, 0.0, -0.9588510990142822, 0.0, -0.35583534836769104, 2.9067373275756836, -0.6513521671295166, 0.0, 1.8580855131149292, 0.9896158576011658, 3.401202440261841, 0.0, 1.0, 2.0, 3.0, 1.0];
		assertElementsNear(&compose.elements, &expected);

		let point = compose.transformPoint(1.0, -2.0, 0.5);
		assertNear(point.x, 4.395878553390503);
		assertNear(point.y, -3.3186667263507843);
		assertNear(point.z, 5.044454455375671);
		assert_eq!(point.w, 1.0);
		let direction = compose.transformDirection(1.0, -2.0, 0.5);
		assertNear(direction.x, 3.395878553390503);
		assertNear(direction.y, -5.318666726350784);
		assertNear(direction.z, 2.0444544553756714);
	}

	//==============================================================================
	// 역행렬: 손 계산 값, 원본 값, 곱하면 단위 행렬, 행렬식 0 이면 단위 행렬.
	//==============================================================================
	#[test]
	fn inverseMatchesHandAndOriginal() {
		// (이동 1, 2, 3 x 크기 2, 4, 8) 의 역은 (크기 1/2, 1/4, 1/8 x 이동 -1, -2, -3) 이다.
		let mut translateScale = Matrix4::createTranslation(1.0, 2.0, 3.0);
		translateScale.scale(2.0, 4.0, 8.0);
		let inverse = Matrix4::createInverse(&translateScale);
		assert_eq!(inverse.elements, [0.5, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 0.125, 0.0, -0.5, -0.5, -0.375, 1.0]);

		let mut compose = Matrix4::createTranslation(1.0, 2.0, 3.0);
		compose.rotateY(0.5);
		compose.rotateX(-0.25);
		compose.scale(2.0, 3.0, 4.0);
		let composeInverse = Matrix4::createInverse(&compose);
		let expected = [0.43879127502441406, -0.03953726217150688, 0.11613034456968307, 0.0, 1.2300102048001804e-9, 0.32297080755233765, 0.06185098737478256, 0.0, -0.23971278965473175, -0.07237246632575989, 0.21257516741752625, 0.0, 0.28034707903862, -0.38928693532943726, -0.8775578141212463, 1.0];
		assertElementsNear(&composeInverse.elements, &expected);

		let mut product = compose;
		product.multiply(&composeInverse);
		assertElementsNear(&product.elements, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);

		let mut singular = Matrix4::createIdentity();
		for index in 0..16 {
			singular.elements[index] = (index + 1) as f32;
		}
		assert_eq!(Matrix4::createInverse(&singular), Matrix4::createIdentity());
	}

	//==============================================================================
	// 원근 투영: 손 계산(시야 90 도, 가로세로 2, 가까이 1,멀리 3)과 원본 값.
	//==============================================================================
	#[test]
	fn perspectiveMatchesHandAndOriginal() {
		// 초점 거리 1 / tan(45 도) = 1, [10] = (1 + 3) / (1 - 3) = -2, [14] = 2 * 1 * 3 / (1 - 3) = -3.
		let handMatrix = Matrix4::createPerspective(std::f32::consts::FRAC_PI_2, 2.0, 1.0, 3.0);
		assertElementsNear(&handMatrix.elements, &[0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -2.0, -1.0, 0.0, 0.0, -3.0, 0.0]);

		let originalMatrix = Matrix4::createPerspective(1.0, 1.5, 0.5, 100.0);
		let expected = [1.2203251123428345, 0.0, 0.0, 0.0, 0.0, 1.8304877281188965, 0.0, 0.0, 0.0, 0.0, -1.0100502967834473, -1.0, 0.0, 0.0, -1.0050251483917236, 0.0];
		assertElementsNear(&originalMatrix.elements, &expected);

		// 가까운 면의 점은 깊이 -1, 먼 면의 점은 깊이 +1 로 간다. (카메라는 -Z 를 본다)
		let nearPoint = handMatrix.transformPoint(0.0, 0.0, -1.0);
		assertNear(nearPoint.z / nearPoint.w, -1.0);
		let farPoint = handMatrix.transformPoint(0.0, 0.0, -3.0);
		assertNear(farPoint.z / farPoint.w, 1.0);
	}

	//==============================================================================
	// 직교 투영: 손 계산(0 ~ 4, 0 ~ 2, 1 ~ 5)과 원본 값.
	//==============================================================================
	#[test]
	fn orthographicMatchesHandAndOriginal() {
		let handMatrix = Matrix4::createOrthographic(0.0, 4.0, 0.0, 2.0, 1.0, 5.0);
		assert_eq!(handMatrix.elements, [0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, -1.0, -1.0, -1.5, 1.0]);

		let originalMatrix = Matrix4::createOrthographic(-4.0, 6.0, -2.0, 3.0, 0.25, 50.0);
		let expected = [0.20000000298023224, 0.0, 0.0, 0.0, 0.0, 0.4000000059604645, 0.0, 0.0, 0.0, 0.0, -0.04020100459456444, 0.0, -0.20000000298023224, -0.20000000298023224, -1.0100502967834473, 1.0];
		assertElementsNear(&originalMatrix.elements, &expected);
	}

	//==============================================================================
	// 시점 행렬: 손 계산((0, 0, 5) 에서 원점을 보면 이동 (0, 0, -5))과 원본 값, 대상은 -Z 쪽에 놓인다.
	//==============================================================================
	#[test]
	fn lookAtMatchesHandAndOriginal() {
		let handMatrix = Matrix4::createLookAt(Vector3::create(0.0, 0.0, 5.0), Vector3::zero(), Vector3::up());
		assert_eq!(handMatrix, Matrix4::createTranslation(0.0, 0.0, -5.0));

		let eyePosition = Vector3::create(3.0, 4.0, 5.0);
		let targetPosition = Vector3::create(0.0, 1.0, 0.0);
		let originalMatrix = Matrix4::createLookAt(eyePosition, targetPosition, Vector3::up());
		let expected = [0.8574929237365723, -0.23537960648536682, 0.45749571919441223, 0.0, 0.0, 0.8892118334770203, 0.45749571919441223, 0.0, -0.5144957304000854, -0.3922993242740631, 0.762492835521698, 0.0, 0.0, -0.8892118334770203, -7.014934062957764, 1.0];
		assertElementsNear(&originalMatrix.elements, &expected);

		let targetInView = originalMatrix.transformPoint(targetPosition.x, targetPosition.y, targetPosition.z);
		let distance = eyePosition.distance(targetPosition);
		assertNear(targetInView.x, 0.0);
		assertNear(targetInView.y, 0.0);
		assertNear(targetInView.z, -distance as f64);
	}

	//==============================================================================
	// 유니폼 배열 기록. (writeToArray, writeToSlice)
	//==============================================================================
	#[test]
	fn writeToArrayAndSlice() {
		let matrix = Matrix4::createTranslation(7.0, 8.0, 9.0);
		let mut array = [0.0f32; 16];
		matrix.writeToArray(&mut array);
		assert_eq!(&array, matrix.getElements());

		let mut jointMatrixArray = vec![0.0f32; 32];
		matrix.writeToSlice(&mut jointMatrixArray, 16);
		assert_eq!(&jointMatrixArray[16..32], &matrix.elements[..]);
		assert_eq!(&jointMatrixArray[0..16], &[0.0f32; 16][..]);

		let mut copied = Matrix4::createIdentity();
		copied.copyFrom(&matrix);
		assert_eq!(copied, matrix);
		copied.setIdentity();
		assert_eq!(copied, Matrix4::default());
	}
}
