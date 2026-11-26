//==============================================================================
//! 4x4 변환 행렬. (원본 base/matrix4.js)
//! - 원본과 같이 열 우선이다. 요소 자리는 elements[column * 4 + row] 이고, glUniformMatrix4fv 에 전치 없이 그대로 올린다.
//! - 점 변환은 transformed = matrix x (x, y, z, 1) 이다. (열벡터를 오른쪽에 곱한다)
//! - multiply / translate / rotate_x ... / scale 은 원본처럼 자기 자신을 바꾼다. (this = this x other, 로컬 공간 오른쪽 곱)
//!   create... 정적 함수는 원본처럼 새 행렬을 돌려준다.
//! - 좌표계는 원본과 같이 오른손이다. create_perspective 는 OpenGL 식(카메라는 -Z 를 보고, 깊이는 -1 ~ 1),
//!   create_look_at 은 눈에서 대상 반대쪽이 +Z(뒤쪽 축)인 뷰 행렬, 회전은 라디안이고 축을 바라볼 때 반시계가 양수다.
//! - 원본 getElements() 는 Float32Array 를 내주고 호출자가 그 안을 직접 고친다. 러스트 판은 elements 를 공개 필드로 두고,
//!   get_elements() 는 읽기 참조를, write_to_array / write_to_slice 는 유니폼 배열에 옮겨 쓰는 일을 맡는다.
//! - 원본은 Float32Array(f32)에 담되 계산은 f64 로 한다. 러스트 판은 담기와 계산 모두 f32 라 끝자리가 조금 다를 수 있다.
//! - 원본 transformPoint 의 { x, y, z, w } 는 TransformedPoint 로, transform_direction 의 { x, y, z } 는 Vector3 로 돌려준다.
//! - 원본 clone 은 Copy 로 갈음한다. 원본에 없는 전치(transpose)는 만들지 않았다.
//==============================================================================
use crate::base::quaternion::Quaternion;
use crate::base::vector3::Vector3;

//==============================================================================
/// 동차 좌표 점. (원본 transformPoint 가 돌려주는 { x, y, z, w }, 원근 나누기는 호출자가 한다)
///
/// # 예제
/// 원근 투영 뒤 깊이 구하기
/// ```rust
/// use vanilla::{math, Matrix4};
///
/// let projection = Matrix4::create_perspective(math::PI * 0.5, 2.0, 1.0, 3.0);
/// let clip_point = projection.transform_point(0.0, 0.0, -3.0);
/// assert_eq!(clip_point.w, 3.0);
/// assert_eq!(clip_point.z / clip_point.w, 1.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TransformedPoint {
	/// 변환한 x. (원근 나누기 전)
	pub x: f32,
	/// 변환한 y. (원근 나누기 전)
	pub y: f32,
	/// 변환한 z. (원근 나누기 전)
	pub z: f32,
	/// 동차 좌표 w. (원근 나누기에 쓴다, 아핀 변환이면 1)
	pub w: f32,
}

//==============================================================================
/// 4x4 변환 행렬. (열 우선, 오른손 좌표계, 기본값은 단위 행렬)
/// - translate, rotate_x, scale 같은 누적 함수는 자신을 바꾸고, create_... 함수는 새 행렬을 돌려준다.
///
/// # 예제
/// 모델 행렬로 점을 옮기고 역행렬로 되돌리기
/// ```rust
/// use vanilla::{Matrix4, Vector3};
///
/// let mut model = Matrix4::create_translation(1.0, 2.0, 3.0);
/// model.scale(2.0, 2.0, 2.0);
/// let point = model.transform_point(1.0, 1.0, 1.0);
/// assert_eq!((point.x, point.y, point.z, point.w), (3.0, 4.0, 5.0, 1.0));
///
/// let inverse = Matrix4::create_inverse(&model);
/// let back = inverse.transform_point(point.x, point.y, point.z);
/// assert_eq!((back.x, back.y, back.z), (1.0, 1.0, 1.0));
///
/// let view = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 5.0), Vector3::zero(), Vector3::up());
/// assert_eq!(view, Matrix4::create_translation(0.0, 0.0, -5.0));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4 {
	/// 열 우선 요소 배열. (elements[column * 4 + row])
	pub elements: [f32; 16],
}

impl Default for Matrix4 {
	//==============================================================================
	/// 단위 행렬. (원본 new Matrix4())
	//==============================================================================
	fn default() -> Matrix4 {
		Matrix4::create_identity()
	}
}

impl Matrix4 {
	//==============================================================================
	/// 단위 행렬로 초기화.
	//==============================================================================
	pub fn set_identity(&mut self) {
		*self = Matrix4::create_identity();
	}

	//==============================================================================
	/// 다른 행렬로부터 복사.
	///
	/// # 인자
	/// - `other`: 복사해 올 행렬.
	//==============================================================================
	pub fn copy_from(&mut self, other: &Matrix4) {
		self.elements = other.elements;
	}

	//==============================================================================
	/// 다른 행렬을 오른쪽에 곱해 누적한다. (this = this x other)
	///
	/// # 인자
	/// - `other`: 오른쪽에 곱할 행렬. (점에는 other 변환이 먼저 걸린다)
	///
	/// # 예제
	/// 이동 x 크기는 크기를 먼저 건다
	/// ```rust
	/// use vanilla::Matrix4;
	///
	/// let mut matrix = Matrix4::create_translation(1.0, 2.0, 3.0);
	/// matrix.multiply(&Matrix4::create_scale(2.0, 2.0, 2.0));
	/// let point = matrix.transform_point(1.0, 0.0, 0.0);
	/// assert_eq!((point.x, point.y, point.z), (3.0, 2.0, 3.0));
	/// ```
	//==============================================================================
	pub fn multiply(&mut self, other: &Matrix4) {
		let elements = &self.elements;
		let other_elements = &other.elements;
		let mut result_elements = [0.0f32; 16];
		for column_index in 0..4 {
			for row_index in 0..4 {
				let mut sum = 0.0f32;
				for term_index in 0..4 {
					sum += elements[term_index * 4 + row_index] * other_elements[column_index * 4 + term_index];
				}
				result_elements[column_index * 4 + row_index] = sum;
			}
		}
		self.elements = result_elements;
	}

	//==============================================================================
	/// 이동 변환을 누적한다. (로컬 공간 오른쪽 곱)
	///
	/// # 인자
	/// - `x`: x 이동량.
	/// - `y`: y 이동량.
	/// - `z`: z 이동량.
	//==============================================================================
	pub fn translate(&mut self, x: f32, y: f32, z: f32) {
		let translation_matrix = Matrix4::create_translation(x, y, z);
		self.multiply(&translation_matrix);
	}

	//==============================================================================
	/// X 축 회전 변환을 누적한다. (라디안, 로컬 공간 오른쪽 곱)
	///
	/// # 인자
	/// - `radian`: 회전 각. (라디안, 양수면 +Y 를 +Z 쪽으로 돌린다)
	//==============================================================================
	pub fn rotate_x(&mut self, radian: f32) {
		let rotation_matrix = Matrix4::create_rotation_x(radian);
		self.multiply(&rotation_matrix);
	}

	//==============================================================================
	/// Y 축 회전 변환을 누적한다. (라디안, 로컬 공간 오른쪽 곱)
	///
	/// # 인자
	/// - `radian`: 회전 각. (라디안, 양수면 +Z 를 +X 쪽으로 돌린다)
	//==============================================================================
	pub fn rotate_y(&mut self, radian: f32) {
		let rotation_matrix = Matrix4::create_rotation_y(radian);
		self.multiply(&rotation_matrix);
	}

	//==============================================================================
	/// Z 축 회전 변환을 누적한다. (라디안, 로컬 공간 오른쪽 곱)
	///
	/// # 인자
	/// - `radian`: 회전 각. (라디안, 양수면 +X 를 +Y 쪽으로 돌린다)
	//==============================================================================
	pub fn rotate_z(&mut self, radian: f32) {
		let rotation_matrix = Matrix4::create_rotation_z(radian);
		self.multiply(&rotation_matrix);
	}

	//==============================================================================
	/// 크기 변환을 누적한다. (로컬 공간 오른쪽 곱)
	///
	/// # 인자
	/// - `x`: x 배율. (1 이면 그대로)
	/// - `y`: y 배율.
	/// - `z`: z 배율.
	//==============================================================================
	pub fn scale(&mut self, x: f32, y: f32, z: f32) {
		let scale_matrix = Matrix4::create_scale(x, y, z);
		self.multiply(&scale_matrix);
	}

	//==============================================================================
	/// 점을 변환한다. (w 성분까지 돌려주고, 원근 나누기는 호출자가 한다)
	///
	/// # 인자
	/// - `x`: 점의 x.
	/// - `y`: 점의 y.
	/// - `z`: 점의 z.
	///
	/// # 돌려줌
	/// matrix x (x, y, z, 1) 의 동차 좌표. (w 로 나누지 않은 값)
	//==============================================================================
	pub fn transform_point(&self, x: f32, y: f32, z: f32) -> TransformedPoint {
		let elements = &self.elements;
		let transformed_x = elements[0] * x + elements[4] * y + elements[8] * z + elements[12];
		let transformed_y = elements[1] * x + elements[5] * y + elements[9] * z + elements[13];
		let transformed_z = elements[2] * x + elements[6] * y + elements[10] * z + elements[14];
		let transformed_w = elements[3] * x + elements[7] * y + elements[11] * z + elements[15];
		TransformedPoint { x: transformed_x, y: transformed_y, z: transformed_z, w: transformed_w }
	}

	//==============================================================================
	/// 방향을 변환한다. (이동 성분은 빼고 회전과 크기만 적용한다)
	///
	/// # 인자
	/// - `x`: 방향의 x.
	/// - `y`: 방향의 y.
	/// - `z`: 방향의 z.
	///
	/// # 돌려줌
	/// 변환한 방향. (길이를 다시 맞추지 않는다)
	///
	/// # 예제
	/// 이동은 방향에 걸리지 않는다
	/// ```rust
	/// use vanilla::{Matrix4, Vector3};
	///
	/// let mut matrix = Matrix4::create_translation(10.0, 0.0, 0.0);
	/// matrix.scale(2.0, 2.0, 2.0);
	/// assert_eq!(matrix.transform_direction(0.0, 1.0, 0.0), Vector3::create(0.0, 2.0, 0.0));
	/// ```
	//==============================================================================
	pub fn transform_direction(&self, x: f32, y: f32, z: f32) -> Vector3 {
		let elements = &self.elements;
		let transformed_x = elements[0] * x + elements[4] * y + elements[8] * z;
		let transformed_y = elements[1] * x + elements[5] * y + elements[9] * z;
		let transformed_z = elements[2] * x + elements[6] * y + elements[10] * z;
		Vector3::create(transformed_x, transformed_y, transformed_z)
	}

	//==============================================================================
	/// 열 우선 요소 배열을 돌려준다. (고칠 때는 elements 필드를 직접 쓴다)
	///
	/// # 돌려줌
	/// 열 우선 요소 16 개의 읽기 참조.
	//==============================================================================
	pub fn get_elements(&self) -> &[f32; 16] {
		&self.elements
	}

	//==============================================================================
	/// 셰이더 mat4 (열 우선) 배열로 기록한다.
	///
	/// # 인자
	/// - `array`: 옮겨 쓸 배열. (열 우선 16 개)
	//==============================================================================
	pub fn write_to_array(&self, array: &mut [f32; 16]) {
		array.copy_from_slice(&self.elements);
	}

	//==============================================================================
	/// 큰 배열의 offset 자리부터 16 개를 기록한다. (원본 jointMatrixArray.set(elements, joint_index * 16) 자리)
	///
	/// # 인자
	/// - `array`: 옮겨 쓸 큰 배열. (길이가 offset + 16 보다 짧으면 패닉)
	/// - `offset`: 쓰기 시작할 자리. (요소 번호, 관절 번호 x 16 같은)
	///
	/// # 예제
	/// 관절 행렬 배열의 두 번째 칸에 쓰기
	/// ```rust
	/// use vanilla::Matrix4;
	///
	/// let joint_matrix = Matrix4::create_translation(7.0, 8.0, 9.0);
	/// let mut joint_matrix_array = vec![0.0f32; 32];
	/// joint_matrix.write_to_slice(&mut joint_matrix_array, 16);
	/// assert_eq!(&joint_matrix_array[16..32], &joint_matrix.elements[..]);
	/// assert_eq!(joint_matrix_array[0], 0.0);
	/// ```
	//==============================================================================
	pub fn write_to_slice(&self, array: &mut [f32], offset: usize) {
		array[offset..offset + 16].copy_from_slice(&self.elements);
	}

	//==============================================================================
	/// 단위 행렬을 만든다.
	///
	/// # 돌려줌
	/// 단위 행렬.
	//==============================================================================
	pub const fn create_identity() -> Matrix4 {
		Matrix4 {
			elements: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
		}
	}

	//==============================================================================
	/// 이동 행렬을 만든다.
	///
	/// # 인자
	/// - `x`: x 이동량.
	/// - `y`: y 이동량.
	/// - `z`: z 이동량.
	///
	/// # 돌려줌
	/// 이동 행렬. (elements 12, 13, 14 에 x, y, z)
	//==============================================================================
	pub fn create_translation(x: f32, y: f32, z: f32) -> Matrix4 {
		let mut matrix = Matrix4::create_identity();
		matrix.elements[12] = x;
		matrix.elements[13] = y;
		matrix.elements[14] = z;
		matrix
	}

	//==============================================================================
	/// X 축 회전 행렬을 만든다. (라디안)
	///
	/// # 인자
	/// - `radian`: 회전 각. (라디안, 양수면 +Y 를 +Z 쪽으로 돌린다)
	///
	/// # 돌려줌
	/// X 축 회전 행렬.
	//==============================================================================
	pub fn create_rotation_x(radian: f32) -> Matrix4 {
		let cosine = radian.cos();
		let sine = radian.sin();
		let mut matrix = Matrix4::create_identity();
		matrix.elements[5] = cosine;
		matrix.elements[6] = sine;
		matrix.elements[9] = -sine;
		matrix.elements[10] = cosine;
		matrix
	}

	//==============================================================================
	/// Y 축 회전 행렬을 만든다. (라디안)
	///
	/// # 인자
	/// - `radian`: 회전 각. (라디안, 양수면 +Z 를 +X 쪽으로 돌린다)
	///
	/// # 돌려줌
	/// Y 축 회전 행렬.
	//==============================================================================
	pub fn create_rotation_y(radian: f32) -> Matrix4 {
		let cosine = radian.cos();
		let sine = radian.sin();
		let mut matrix = Matrix4::create_identity();
		matrix.elements[0] = cosine;
		matrix.elements[2] = -sine;
		matrix.elements[8] = sine;
		matrix.elements[10] = cosine;
		matrix
	}

	//==============================================================================
	/// Z 축 회전 행렬을 만든다. (라디안)
	///
	/// # 인자
	/// - `radian`: 회전 각. (라디안, 양수면 +X 를 +Y 쪽으로 돌린다)
	///
	/// # 돌려줌
	/// Z 축 회전 행렬.
	///
	/// # 예제
	/// 90 도 돌리면 +X 가 +Y 로 간다
	/// ```rust
	/// use vanilla::{math, Matrix4};
	///
	/// let rotation = Matrix4::create_rotation_z(math::degree_to_radian(90.0));
	/// let rotated = rotation.transform_direction(1.0, 0.0, 0.0);
	/// assert!(rotated.x.abs() < 1e-6);
	/// assert!((rotated.y - 1.0).abs() < 1e-6);
	/// ```
	//==============================================================================
	pub fn create_rotation_z(radian: f32) -> Matrix4 {
		let cosine = radian.cos();
		let sine = radian.sin();
		let mut matrix = Matrix4::create_identity();
		matrix.elements[0] = cosine;
		matrix.elements[1] = sine;
		matrix.elements[4] = -sine;
		matrix.elements[5] = cosine;
		matrix
	}

	//==============================================================================
	/// 쿼터니언 회전 행렬을 만든다. (단위 쿼터니언을 가정한다)
	///
	/// # 인자
	/// - `quaternion`: 회전 쿼터니언. (길이 1 로 정규화한 것)
	///
	/// # 돌려줌
	/// 회전 행렬. (이동 성분은 0)
	///
	/// # 예제
	/// Z 축 오일러 쿼터니언은 Z 축 회전 행렬과 같다
	/// ```rust
	/// use vanilla::{Matrix4, Quaternion};
	///
	/// let quaternion = Quaternion::create_from_euler(0.0, 0.0, 0.5);
	/// let from_quaternion = Matrix4::create_rotation_from_quaternion(quaternion);
	/// let from_angle = Matrix4::create_rotation_z(0.5);
	/// for index in 0..16 {
	/// 	assert!((from_quaternion.elements[index] - from_angle.elements[index]).abs() < 1e-5);
	/// }
	/// ```
	//==============================================================================
	pub fn create_rotation_from_quaternion(quaternion: Quaternion) -> Matrix4 {
		let x = quaternion.x;
		let y = quaternion.y;
		let z = quaternion.z;
		let w = quaternion.w;
		let mut matrix = Matrix4::create_identity();
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
	/// 크기 행렬을 만든다.
	///
	/// # 인자
	/// - `x`: x 배율.
	/// - `y`: y 배율.
	/// - `z`: z 배율.
	///
	/// # 돌려줌
	/// 크기 행렬.
	//==============================================================================
	pub fn create_scale(x: f32, y: f32, z: f32) -> Matrix4 {
		let mut matrix = Matrix4::create_identity();
		matrix.elements[0] = x;
		matrix.elements[5] = y;
		matrix.elements[10] = z;
		matrix
	}

	//==============================================================================
	/// 원근 투영 행렬을 만든다. (세로 시야각 라디안, OpenGL 식 깊이 -1 ~ 1)
	///
	/// # 인자
	/// - `field_of_view_radian`: 세로 시야각. (라디안)
	/// - `aspect_ratio`: 가로 나누기 세로 비.
	/// - `near_distance`: 카메라에서 가까운 면까지 거리. (카메라는 -Z 를 본다)
	/// - `far_distance`: 카메라에서 먼 면까지 거리.
	///
	/// # 돌려줌
	/// 원근 투영 행렬. (가까운 면은 깊이 -1, 먼 면은 깊이 1 로 간다)
	///
	/// # 예제
	/// 가까운 면의 점은 깊이 -1
	/// ```rust
	/// use vanilla::{math, Matrix4};
	///
	/// let projection = Matrix4::create_perspective(math::degree_to_radian(90.0), 2.0, 1.0, 3.0);
	/// let near_point = projection.transform_point(0.0, 0.0, -1.0);
	/// assert_eq!(near_point.z / near_point.w, -1.0);
	/// ```
	//==============================================================================
	pub fn create_perspective(field_of_view_radian: f32, aspect_ratio: f32, near_distance: f32, far_distance: f32) -> Matrix4 {
		let focal_length = 1.0 / (field_of_view_radian / 2.0).tan();
		let range_inverse = 1.0 / (near_distance - far_distance);
		let mut matrix = Matrix4 { elements: [0.0; 16] };
		let elements = &mut matrix.elements;
		elements[0] = focal_length / aspect_ratio;
		elements[5] = focal_length;
		elements[10] = (near_distance + far_distance) * range_inverse;
		elements[11] = -1.0;
		elements[14] = near_distance * far_distance * range_inverse * 2.0;
		matrix
	}

	//==============================================================================
	/// 직교 투영 행렬을 만든다. (OpenGL glOrtho 와 같은 식)
	///
	/// # 인자
	/// - `left`: 왼쪽 면의 x.
	/// - `right`: 오른쪽 면의 x.
	/// - `bottom`: 아래 면의 y.
	/// - `top`: 위 면의 y.
	/// - `near_distance`: 카메라에서 가까운 면까지 거리. (카메라는 -Z 를 본다)
	/// - `far_distance`: 카메라에서 먼 면까지 거리.
	///
	/// # 돌려줌
	/// 직교 투영 행렬. (상자 안을 -1 ~ 1 로 옮긴다)
	///
	/// # 예제
	/// 상자 모서리는 (1, 1, 1) 로 간다
	/// ```rust
	/// use vanilla::Matrix4;
	///
	/// let projection = Matrix4::create_orthographic(0.0, 4.0, 0.0, 2.0, 1.0, 5.0);
	/// let corner = projection.transform_point(4.0, 2.0, -5.0);
	/// assert_eq!((corner.x, corner.y, corner.z, corner.w), (1.0, 1.0, 1.0, 1.0));
	/// ```
	//==============================================================================
	pub fn create_orthographic(left: f32, right: f32, bottom: f32, top: f32, near_distance: f32, far_distance: f32) -> Matrix4 {
		let mut matrix = Matrix4 { elements: [0.0; 16] };
		let elements = &mut matrix.elements;
		elements[0] = 2.0 / (right - left);
		elements[5] = 2.0 / (top - bottom);
		elements[10] = -2.0 / (far_distance - near_distance);
		elements[12] = -(right + left) / (right - left);
		elements[13] = -(top + bottom) / (top - bottom);
		elements[14] = -(far_distance + near_distance) / (far_distance - near_distance);
		elements[15] = 1.0;
		matrix
	}

	//==============================================================================
	/// 시점 행렬을 만든다. (eye_position 에서 target_position 을 바라보는 뷰 행렬, 뒤쪽 축 = 눈 - 대상)
	///
	/// # 인자
	/// - `eye_position`: 눈(카메라) 위치.
	/// - `target_position`: 바라볼 점. (eye_position 과 같으면 쓸 수 없는 행렬이 된다)
	/// - `up_direction`: 위쪽 방향. (보통 Vector3::up(), 시선과 나란하면 오른쪽 축이 (0, 0, 0) 이 되어 쓸 수 없는 행렬이 된다)
	///
	/// # 돌려줌
	/// 월드 좌표를 뷰 좌표로 바꾸는 행렬. (대상은 -Z 쪽에 놓인다)
	///
	/// # 예제
	/// (0, 0, 5) 에서 원점 보기
	/// ```rust
	/// use vanilla::{Matrix4, Vector3};
	///
	/// let view = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 5.0), Vector3::zero(), Vector3::up());
	/// let target_in_view = view.transform_point(0.0, 0.0, 0.0);
	/// assert_eq!((target_in_view.x, target_in_view.y, target_in_view.z), (0.0, 0.0, -5.0));
	/// ```
	//==============================================================================
	pub fn create_look_at(eye_position: Vector3, target_position: Vector3, up_direction: Vector3) -> Matrix4 {
		let eye_to_target = eye_position.subtract(target_position);
		let back_axis = eye_to_target.normalize();
		let up_cross_back = up_direction.cross(back_axis);
		let right_axis = up_cross_back.normalize();
		let up_axis = back_axis.cross(right_axis);

		let mut matrix = Matrix4::create_identity();
		let elements = &mut matrix.elements;
		elements[0] = right_axis.x;
		elements[1] = up_axis.x;
		elements[2] = back_axis.x;
		elements[3] = 0.0;
		elements[4] = right_axis.y;
		elements[5] = up_axis.y;
		elements[6] = back_axis.y;
		elements[7] = 0.0;
		elements[8] = right_axis.z;
		elements[9] = up_axis.z;
		elements[10] = back_axis.z;
		elements[11] = 0.0;
		elements[12] = -right_axis.dot(eye_position);
		elements[13] = -up_axis.dot(eye_position);
		elements[14] = -back_axis.dot(eye_position);
		elements[15] = 1.0;
		matrix
	}

	//==============================================================================
	/// 역행렬을 만든다. (일반 4x4 여인수 전개, 행렬식이 0 이면 단위 행렬)
	///
	/// # 인자
	/// - `matrix`: 뒤집을 행렬.
	///
	/// # 돌려줌
	/// 역행렬. 행렬식이 0 이면 단위 행렬.
	///
	/// # 예제
	/// 되돌리기와 뒤집을 수 없는 행렬
	/// ```rust
	/// use vanilla::Matrix4;
	///
	/// let mut matrix = Matrix4::create_translation(1.0, 2.0, 3.0);
	/// matrix.scale(2.0, 4.0, 8.0);
	/// let inverse = Matrix4::create_inverse(&matrix);
	/// let point = inverse.transform_point(1.0, 2.0, 3.0);
	/// assert_eq!((point.x, point.y, point.z), (0.0, 0.0, 0.0));
	///
	/// let singular = Matrix4 { elements: [0.0; 16] };
	/// assert_eq!(Matrix4::create_inverse(&singular), Matrix4::create_identity());
	/// ```
	//==============================================================================
	pub fn create_inverse(matrix: &Matrix4) -> Matrix4 {
		let source_elements = &matrix.elements;
		let m00 = source_elements[0];
		let m01 = source_elements[1];
		let m02 = source_elements[2];
		let m03 = source_elements[3];
		let m04 = source_elements[4];
		let m05 = source_elements[5];
		let m06 = source_elements[6];
		let m07 = source_elements[7];
		let m08 = source_elements[8];
		let m09 = source_elements[9];
		let m10 = source_elements[10];
		let m11 = source_elements[11];
		let m12 = source_elements[12];
		let m13 = source_elements[13];
		let m14 = source_elements[14];
		let m15 = source_elements[15];

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
			return Matrix4::create_identity();
		}
		let inverse_determinant = 1.0 / determinant;

		let mut result_matrix = Matrix4::create_identity();
		let result_elements = &mut result_matrix.elements;
		result_elements[0] = (m05 * cofactor11 - m06 * cofactor10 + m07 * cofactor09) * inverse_determinant;
		result_elements[1] = (m02 * cofactor10 - m01 * cofactor11 - m03 * cofactor09) * inverse_determinant;
		result_elements[2] = (m13 * cofactor05 - m14 * cofactor04 + m15 * cofactor03) * inverse_determinant;
		result_elements[3] = (m10 * cofactor04 - m09 * cofactor05 - m11 * cofactor03) * inverse_determinant;
		result_elements[4] = (m06 * cofactor08 - m04 * cofactor11 - m07 * cofactor07) * inverse_determinant;
		result_elements[5] = (m00 * cofactor11 - m02 * cofactor08 + m03 * cofactor07) * inverse_determinant;
		result_elements[6] = (m14 * cofactor02 - m12 * cofactor05 - m15 * cofactor01) * inverse_determinant;
		result_elements[7] = (m08 * cofactor05 - m10 * cofactor02 + m11 * cofactor01) * inverse_determinant;
		result_elements[8] = (m04 * cofactor10 - m05 * cofactor08 + m07 * cofactor06) * inverse_determinant;
		result_elements[9] = (m01 * cofactor08 - m00 * cofactor10 - m03 * cofactor06) * inverse_determinant;
		result_elements[10] = (m12 * cofactor04 - m13 * cofactor02 + m15 * cofactor00) * inverse_determinant;
		result_elements[11] = (m09 * cofactor02 - m08 * cofactor04 - m11 * cofactor00) * inverse_determinant;
		result_elements[12] = (m05 * cofactor07 - m04 * cofactor09 - m06 * cofactor06) * inverse_determinant;
		result_elements[13] = (m00 * cofactor09 - m01 * cofactor07 + m02 * cofactor06) * inverse_determinant;
		result_elements[14] = (m13 * cofactor01 - m12 * cofactor03 - m14 * cofactor00) * inverse_determinant;
		result_elements[15] = (m08 * cofactor03 - m09 * cofactor01 + m10 * cofactor00) * inverse_determinant;
		result_matrix
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 원본(f64 계산 뒤 Float32Array)과 러스트 판(f32 계산)의 끝자리 차이를 받아 줄 허용 오차.
	const TOLERANCE: f32 = 1.0e-5;

	//==============================================================================
	/// 요소 배열이 기대값(원본이 찍은 f64 값)과 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assert_elements_near(actual: &[f32; 16], expected: &[f64; 16]) {
		for (index, (actual_value, expected_value)) in actual.iter().zip(expected.iter()).enumerate() {
			let difference = (*actual_value as f64 - *expected_value).abs();
			assert!(difference <= TOLERANCE as f64, "elements[{}] = {}, expected {} (all: {:?})", index, actual_value, expected_value, actual);
		}
	}

	//==============================================================================
	/// 값 하나가 기대값(원본이 찍은 f64 값)과 허용 오차 안에서 같은지 확인한다.
	//==============================================================================
	fn assert_near(actual: f32, expected: f64) {
		let difference = (actual as f64 - expected).abs();
		assert!(difference <= TOLERANCE as f64, "{} expected {}", actual, expected);
	}

	//==============================================================================
	/// 1 ~ 16 과 17 ~ 32 행렬의 곱. (손 계산 `[0]` = 1*17 + 5*18 + 9*19 + 13*20 = 538, `[5]` = 2*21 + 6*22 + 10*23 + 14*24 = 740, 나머지는 node 로 돌린 원본 값)
	//==============================================================================
	#[test]
	fn multiply_matches_hand_and_original() {
		let mut left = Matrix4::create_identity();
		let mut right = Matrix4::create_identity();
		for index in 0..16 {
			left.elements[index] = (index + 1) as f32;
			right.elements[index] = (index + 17) as f32;
		}
		left.multiply(&right);
		let expected = [538.0, 612.0, 686.0, 760.0, 650.0, 740.0, 830.0, 920.0, 762.0, 868.0, 974.0, 1080.0, 874.0, 996.0, 1118.0, 1240.0];
		assert_eq!(left.elements, expected);
	}

	//==============================================================================
	/// 곱하는 순서: translate 뒤 scale 은 이동 x 크기 (크기 먼저 적용), 반대는 이동도 커진다.
	//==============================================================================
	#[test]
	fn multiply_order_is_local_right_multiply() {
		let mut translate_then_scale = Matrix4::create_translation(1.0, 2.0, 3.0);
		translate_then_scale.scale(2.0, 3.0, 4.0);
		assert_eq!(translate_then_scale.elements, [2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 1.0, 2.0, 3.0, 1.0]);

		let mut scale_then_translate = Matrix4::create_scale(2.0, 3.0, 4.0);
		scale_then_translate.translate(1.0, 2.0, 3.0);
		assert_eq!(scale_then_translate.elements, [2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 2.0, 6.0, 12.0, 1.0]);

		let point = translate_then_scale.transform_point(1.0, 1.0, 1.0);
		assert_eq!(point, TransformedPoint { x: 3.0, y: 5.0, z: 7.0, w: 1.0 });
		let direction = translate_then_scale.transform_direction(1.0, 1.0, 1.0);
		assert_eq!(direction, Vector3::create(2.0, 3.0, 4.0));
	}

	//==============================================================================
	/// 회전 방향: Z 축 90 도는 +X 를 +Y 로, X 축 90 도는 +Y 를 +Z 로, Y 축 90 도는 +Z 를 +X 로 돌린다. (오른손, 반시계)
	//==============================================================================
	#[test]
	fn rotation_is_right_handed() {
		let quarter_turn = std::f32::consts::FRAC_PI_2;
		let rotated_by_z = Matrix4::create_rotation_z(quarter_turn).transform_direction(1.0, 0.0, 0.0);
		assert_near(rotated_by_z.x, 0.0);
		assert_near(rotated_by_z.y, 1.0);
		assert_near(rotated_by_z.z, 0.0);
		let rotated_by_x = Matrix4::create_rotation_x(quarter_turn).transform_direction(0.0, 1.0, 0.0);
		assert_near(rotated_by_x.x, 0.0);
		assert_near(rotated_by_x.y, 0.0);
		assert_near(rotated_by_x.z, 1.0);
		let rotated_by_y = Matrix4::create_rotation_y(quarter_turn).transform_direction(0.0, 0.0, 1.0);
		assert_near(rotated_by_y.x, 1.0);
		assert_near(rotated_by_y.y, 0.0);
		assert_near(rotated_by_y.z, 0.0);
	}

	//==============================================================================
	/// translate, rotate_y, rotate_x, scale 누적과 점, 방향 변환이 원본과 같은지. (node 로 돌린 원본 값)
	//==============================================================================
	#[test]
	fn compose_matches_original() {
		let mut compose = Matrix4::create_translation(1.0, 2.0, 3.0);
		compose.rotate_y(0.5);
		compose.rotate_x(-0.25);
		compose.scale(2.0, 3.0, 4.0);
		let expected = [1.7551651000976562, 0.0, -0.9588510990142822, 0.0, -0.35583534836769104, 2.9067373275756836, -0.6513521671295166, 0.0, 1.8580855131149292, 0.9896158576011658, 3.401202440261841, 0.0, 1.0, 2.0, 3.0, 1.0];
		assert_elements_near(&compose.elements, &expected);

		let point = compose.transform_point(1.0, -2.0, 0.5);
		assert_near(point.x, 4.395878553390503);
		assert_near(point.y, -3.3186667263507843);
		assert_near(point.z, 5.044454455375671);
		assert_eq!(point.w, 1.0);
		let direction = compose.transform_direction(1.0, -2.0, 0.5);
		assert_near(direction.x, 3.395878553390503);
		assert_near(direction.y, -5.318666726350784);
		assert_near(direction.z, 2.0444544553756714);
	}

	//==============================================================================
	/// 역행렬: 손 계산 값, 원본 값, 곱하면 단위 행렬, 행렬식 0 이면 단위 행렬.
	//==============================================================================
	#[test]
	fn inverse_matches_hand_and_original() {
		// (이동 1, 2, 3 x 크기 2, 4, 8) 의 역은 (크기 1/2, 1/4, 1/8 x 이동 -1, -2, -3) 이다.
		let mut translate_scale = Matrix4::create_translation(1.0, 2.0, 3.0);
		translate_scale.scale(2.0, 4.0, 8.0);
		let inverse = Matrix4::create_inverse(&translate_scale);
		assert_eq!(inverse.elements, [0.5, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0, 0.125, 0.0, -0.5, -0.5, -0.375, 1.0]);

		let mut compose = Matrix4::create_translation(1.0, 2.0, 3.0);
		compose.rotate_y(0.5);
		compose.rotate_x(-0.25);
		compose.scale(2.0, 3.0, 4.0);
		let compose_inverse = Matrix4::create_inverse(&compose);
		let expected = [0.43879127502441406, -0.03953726217150688, 0.11613034456968307, 0.0, 1.2300102048001804e-9, 0.32297080755233765, 0.06185098737478256, 0.0, -0.23971278965473175, -0.07237246632575989, 0.21257516741752625, 0.0, 0.28034707903862, -0.38928693532943726, -0.8775578141212463, 1.0];
		assert_elements_near(&compose_inverse.elements, &expected);

		let mut product = compose;
		product.multiply(&compose_inverse);
		assert_elements_near(&product.elements, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);

		let mut singular = Matrix4::create_identity();
		for index in 0..16 {
			singular.elements[index] = (index + 1) as f32;
		}
		assert_eq!(Matrix4::create_inverse(&singular), Matrix4::create_identity());
	}

	//==============================================================================
	/// 원근 투영: 손 계산(시야 90 도, 가로세로 2, 가까이 1,멀리 3)과 원본 값.
	//==============================================================================
	#[test]
	fn perspective_matches_hand_and_original() {
		// 초점 거리 1 / tan(45 도) = 1, [10] = (1 + 3) / (1 - 3) = -2, [14] = 2 * 1 * 3 / (1 - 3) = -3.
		let hand_matrix = Matrix4::create_perspective(std::f32::consts::FRAC_PI_2, 2.0, 1.0, 3.0);
		assert_elements_near(&hand_matrix.elements, &[0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -2.0, -1.0, 0.0, 0.0, -3.0, 0.0]);

		let original_matrix = Matrix4::create_perspective(1.0, 1.5, 0.5, 100.0);
		let expected = [1.2203251123428345, 0.0, 0.0, 0.0, 0.0, 1.8304877281188965, 0.0, 0.0, 0.0, 0.0, -1.0100502967834473, -1.0, 0.0, 0.0, -1.0050251483917236, 0.0];
		assert_elements_near(&original_matrix.elements, &expected);

		// 가까운 면의 점은 깊이 -1, 먼 면의 점은 깊이 +1 로 간다. (카메라는 -Z 를 본다)
		let near_point = hand_matrix.transform_point(0.0, 0.0, -1.0);
		assert_near(near_point.z / near_point.w, -1.0);
		let far_point = hand_matrix.transform_point(0.0, 0.0, -3.0);
		assert_near(far_point.z / far_point.w, 1.0);
	}

	//==============================================================================
	/// 직교 투영: 손 계산(0 ~ 4, 0 ~ 2, 1 ~ 5)과 원본 값.
	//==============================================================================
	#[test]
	fn orthographic_matches_hand_and_original() {
		let hand_matrix = Matrix4::create_orthographic(0.0, 4.0, 0.0, 2.0, 1.0, 5.0);
		assert_eq!(hand_matrix.elements, [0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, -1.0, -1.0, -1.5, 1.0]);

		let original_matrix = Matrix4::create_orthographic(-4.0, 6.0, -2.0, 3.0, 0.25, 50.0);
		let expected = [0.20000000298023224, 0.0, 0.0, 0.0, 0.0, 0.4000000059604645, 0.0, 0.0, 0.0, 0.0, -0.04020100459456444, 0.0, -0.20000000298023224, -0.20000000298023224, -1.0100502967834473, 1.0];
		assert_elements_near(&original_matrix.elements, &expected);
	}

	//==============================================================================
	/// 시점 행렬: 손 계산((0, 0, 5) 에서 원점을 보면 이동 (0, 0, -5))과 원본 값, 대상은 -Z 쪽에 놓인다.
	//==============================================================================
	#[test]
	fn look_at_matches_hand_and_original() {
		let hand_matrix = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 5.0), Vector3::zero(), Vector3::up());
		assert_eq!(hand_matrix, Matrix4::create_translation(0.0, 0.0, -5.0));

		let eye_position = Vector3::create(3.0, 4.0, 5.0);
		let target_position = Vector3::create(0.0, 1.0, 0.0);
		let original_matrix = Matrix4::create_look_at(eye_position, target_position, Vector3::up());
		let expected = [0.8574929237365723, -0.23537960648536682, 0.45749571919441223, 0.0, 0.0, 0.8892118334770203, 0.45749571919441223, 0.0, -0.5144957304000854, -0.3922993242740631, 0.762492835521698, 0.0, 0.0, -0.8892118334770203, -7.014934062957764, 1.0];
		assert_elements_near(&original_matrix.elements, &expected);

		let target_in_view = original_matrix.transform_point(target_position.x, target_position.y, target_position.z);
		let distance = eye_position.distance(target_position);
		assert_near(target_in_view.x, 0.0);
		assert_near(target_in_view.y, 0.0);
		assert_near(target_in_view.z, -distance as f64);
	}

	//==============================================================================
	/// 유니폼 배열 기록. (write_to_array, write_to_slice)
	//==============================================================================
	#[test]
	fn write_to_array_and_slice() {
		let matrix = Matrix4::create_translation(7.0, 8.0, 9.0);
		let mut array = [0.0f32; 16];
		matrix.write_to_array(&mut array);
		assert_eq!(&array, matrix.get_elements());

		let mut joint_matrix_array = vec![0.0f32; 32];
		matrix.write_to_slice(&mut joint_matrix_array, 16);
		assert_eq!(&joint_matrix_array[16..32], &matrix.elements[..]);
		assert_eq!(&joint_matrix_array[0..16], &[0.0f32; 16][..]);

		let mut copied = Matrix4::create_identity();
		copied.copy_from(&matrix);
		assert_eq!(copied, matrix);
		copied.set_identity();
		assert_eq!(copied, Matrix4::default());
	}
}
