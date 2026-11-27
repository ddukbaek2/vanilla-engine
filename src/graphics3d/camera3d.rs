//==============================================================================
//! 3D 카메라. (vanilla.js graphics3d/camera3d.js 의 Camera3D)
//! - 원근이나 직교 투영과 보는 자리를 들고, 보기와 투영 행렬을 바뀔 때만 다시 셈한다.
//! - 좌표는 엔진 Matrix4 와 같은 y 가 위인 오른손 좌표다. 카메라는 -Z 를 본다. (create_look_at, create_perspective)
//! - 원근: set_perspective(시야각, 가까운 거리, 먼 거리), 직교: set_orthographic(보이는 높이, 가까운 거리, 먼 거리).
//!   가로 세로 비는 set_aspect_ratio 로 따로 준다. (그리는 사각형이 바뀔 때마다 고쳐 넣는다, 직교의 너비는 높이 x 비)
//! - 행렬은 Cell 에 담아 두고 &self 로 읽을 때 바뀐 값이 있으면 다시 셈한다. (원본은 getter 가 #isMatrixDirty 를 보고 셈한다)
//! - 같은 카메라를 렌더러(ToonRenderer::begin_frame_with_camera)와 2D 표시(project_to_view)가 함께 써, 3D 점 위에 글이나 단추를 얹는다.
//! - project_to_view(x, y, z, view_rect): 월드 점을 view_rect 안의 자리로. view_rect 는 3D 를 그리는 사각형을 부르는 쪽의 좌표(보통 뷰 좌표)로 준 것이다.
//!   원본의 { x, y, depth, isInFront } 는 ProjectedPoint 로 돌려준다.
//! - unproject_to_plane(view_position, plane_height, view_rect): view_rect 안의 자리에서 나간 시선이 수평면(y = plane_height)과 만나는 점. 없으면 None.
//! - 원 행렬: set_matrices(보기, 투영) 로 게임이 셈한 행렬을 그대로 쓴다. (기울인 직교, 화면 변환을 곱한 투영 같은 것) 위 함수들이 모두 그 행렬로 돈다.
//!   보기 행렬은 월드 → 카메라의 강체 변환(카메라는 -Z 를 봄)으로 준다. 눈, 보는 곳(눈 앞 1), 위쪽과 is_orthographic 은 그 행렬에서 뽑는다.
//!   시야각, 보이는 높이, 가까운 거리, 먼 거리, 가로 세로 비는 마지막으로 준 값 그대로다. set_perspective, set_orthographic, look_at 을 부르면 원 행렬을 버리고 셈하는 길로 돌아간다.
//! - 보기 덧붙임: set_view_offset(자리, 돌림) 은 카메라 지역 축의 덧붙임을 마지막에 건다. (CameraShake 의 흔들림, 걸음 흔들림, 반동 같은 것, 셈하는 길과 원 행렬 모두)
//!   보기 행렬과 그 뒤의 행렬, project_to_view, unproject_to_plane 은 덧붙인 카메라로 돈다. get_eye_position, get_target_position, get_up_direction 은 덧붙이기 앞 값이다.
//==============================================================================
use std::cell::Cell;

use crate::base::matrix4::Matrix4;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::base::vector3::Vector3;
use crate::graphics3d::camerashake::create_offset_view_matrix;

/// 카메라 앞으로 칠 가장 작은 깊이. (이보다 가까우면 투영하지 않는다)
const MINIMUM_FRONT_DEPTH: f64 = 0.0001;

//==============================================================================
/// 투영한 점. (view_rect 의 좌표, 카메라 앞 깊이, 눈의 면보다 앞인지, 앞이 아니면 x, y 는 0)
///
/// # 예제
/// 눈 뒤의 점은 is_in_front 가 false 이고 자리는 0
/// ```rust
/// use vanilla::{Camera3D, ProjectedPoint, Rect};
///
/// let camera = Camera3D::new();
/// let view_rect = Rect::create(0.0, 0.0, 640.0, 480.0);
/// let behind_point: ProjectedPoint = camera.project_to_view(0.0, 0.0, 20.0, view_rect);
/// assert!(!behind_point.is_in_front);
/// assert_eq!((behind_point.x, behind_point.y), (0.0, 0.0));
/// assert!((behind_point.depth + 10.0).abs() < 0.001);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProjectedPoint {
	/// view_rect 좌표의 가로 자리. (앞이 아니면 0)
	pub x: f64,
	/// view_rect 좌표의 세로 자리. (위가 0 이고 아래로 늘어남, 앞이 아니면 0)
	pub y: f64,
	/// 카메라 앞 깊이. (보는 쪽으로 잰 거리, 눈의 면보다 뒤면 음수, read_view_depth 와 같다)
	pub depth: f64,
	/// 눈의 면보다 앞인지. (깊이가 0.0001 이상이고 클립 w 가 0 이 아니면 true)
	pub is_in_front: bool,
}

//==============================================================================
/// 담아 둔 행렬 넷.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct CameraMatrices {
	view_matrix: Matrix4,
	projection_matrix: Matrix4,
	view_projection_matrix: Matrix4,
	inverse_view_projection_matrix: Matrix4,
}

//==============================================================================
/// 3D 카메라. (원근이나 직교 투영과 보는 자리, 보기와 투영 행렬은 바뀐 뒤 처음 읽을 때 다시 셈한다)
/// - 기본은 원근 시야각 0.8 라디안, 가로 세로 비 1, 거리 0.1 ~ 1000, 눈 (0, 0, 10) 에서 원점을 본다.
///
/// # 예제
/// 원근 카메라로 원점을 뷰 사각형에 투영하고, 그 자리를 다시 바닥에 되돌리기
/// ```rust
/// use vanilla::{Camera3D, Rect, Vector2, Vector3};
///
/// let mut camera = Camera3D::new();
/// camera.set_perspective(0.8, 0.1, 100.0);
/// camera.set_aspect_ratio(2.0);
/// camera.look_at(Vector3::create(0.0, 5.0, 10.0), Vector3::zero(), Vector3::up());
///
/// let view_rect = Rect::create(0.0, 0.0, 400.0, 200.0);
/// let projected_point = camera.project_to_view(0.0, 0.0, 0.0, view_rect);
/// assert!(projected_point.is_in_front);
/// assert!((projected_point.x - 200.0).abs() < 0.01);
/// assert!((projected_point.y - 100.0).abs() < 0.01);
///
/// let ground_point = camera.unproject_to_plane(Vector2::create(projected_point.x, projected_point.y), 0.0, view_rect).unwrap();
/// assert!(ground_point.x.abs() < 0.01);
/// assert!(ground_point.z.abs() < 0.01);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Camera3D {
	is_orthographic: bool,
	field_of_view_radian: f64,
	orthographic_height: f64,
	aspect_ratio: f64,
	near_distance: f64,
	far_distance: f64,
	eye_position: Vector3,
	target_position: Vector3,
	up_direction: Vector3,
	matrices: Cell<CameraMatrices>,
	is_matrix_dirty: Cell<bool>,
	/// set_matrices 로 받은 보기와 투영. (None 이면 셈하는 길)
	raw_matrices: Option<[Matrix4; 2]>,
	/// 카메라 지역 자리 덧붙임. (x 오른쪽, y 위, z 뒤)
	view_offset_position: Vector3,
	/// 카메라 지역 돌림 덧붙임. (라디안, x pitch, y yaw, z roll)
	view_offset_rotation: Vector3,
}

impl Default for Camera3D {
	//==============================================================================
	/// 기본 카메라. (Camera3D::new 와 같다)
	//==============================================================================
	fn default() -> Camera3D {
		Camera3D::new()
	}
}

impl Camera3D {
	//==============================================================================
	/// 생성. (원근 시야각 0.8 라디안, 비 1, 거리 0.1 ~ 1000, 눈 (0, 0, 10) 에서 원점을 본다)
	///
	/// # 돌려줌
	/// 원근 시야각 0.8 라디안, 가로 세로 비 1, 거리 0.1 ~ 1000, 눈 (0, 0, 10) 에서 원점을 보는(위쪽 (0, 1, 0)) 새 카메라.
	//==============================================================================
	pub fn new() -> Camera3D {
		let identity_matrix = Matrix4::create_identity();
		Camera3D {
			is_orthographic: false,
			field_of_view_radian: 0.8,
			orthographic_height: 10.0,
			aspect_ratio: 1.0,
			near_distance: 0.1,
			far_distance: 1000.0,
			eye_position: Vector3::create(0.0, 0.0, 10.0),
			target_position: Vector3::create(0.0, 0.0, 0.0),
			up_direction: Vector3::create(0.0, 1.0, 0.0),
			matrices: Cell::new(CameraMatrices { view_matrix: identity_matrix, projection_matrix: identity_matrix, view_projection_matrix: identity_matrix, inverse_view_projection_matrix: identity_matrix }),
			is_matrix_dirty: Cell::new(true),
			raw_matrices: None,
			view_offset_position: Vector3::zero(),
			view_offset_rotation: Vector3::zero(),
		}
	}

	//==============================================================================
	/// 원근 투영 설정. (세로 시야각)
	/// - set_matrices 로 준 원 행렬이 있으면 버리고 셈하는 길로 돌아간다.
	///
	/// # 인자
	/// - `field_of_view_radian`: 세로 시야각. (라디안)
	/// - `near_distance`: 가까운 거리. (카메라 앞, 월드 단위)
	/// - `far_distance`: 먼 거리. (월드 단위)
	//==============================================================================
	pub fn set_perspective(&mut self, field_of_view_radian: f64, near_distance: f64, far_distance: f64) {
		self.clear_raw_matrices();
		self.is_orthographic = false;
		self.field_of_view_radian = field_of_view_radian;
		self.near_distance = near_distance;
		self.far_distance = far_distance;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	/// 직교 투영 설정. (보이는 높이, 너비는 높이 x 가로 세로 비)
	/// - set_matrices 로 준 원 행렬이 있으면 버리고 셈하는 길로 돌아간다.
	///
	/// # 인자
	/// - `orthographic_height`: 보이는 높이. (월드 단위, 가운데에서 위아래로 절반씩)
	/// - `near_distance`: 가까운 거리. (카메라 앞, 월드 단위)
	/// - `far_distance`: 먼 거리. (월드 단위)
	///
	/// # 예제
	/// 높이 10, 비 2 면 너비 20 이 보여 (10, 5) 가 오른쪽 위 구석으로 간다
	/// ```rust
	/// use vanilla::{Camera3D, Rect, Vector3};
	///
	/// let mut camera = Camera3D::new();
	/// camera.set_orthographic(10.0, 0.1, 100.0);
	/// camera.set_aspect_ratio(2.0);
	/// camera.look_at(Vector3::create(0.0, 0.0, 10.0), Vector3::zero(), Vector3::up());
	/// assert!(camera.is_orthographic());
	///
	/// let projected_point = camera.project_to_view(10.0, 5.0, 0.0, Rect::create(0.0, 0.0, 200.0, 100.0));
	/// assert!((projected_point.x - 200.0).abs() < 0.01);
	/// assert!(projected_point.y.abs() < 0.01);
	/// ```
	//==============================================================================
	pub fn set_orthographic(&mut self, orthographic_height: f64, near_distance: f64, far_distance: f64) {
		self.clear_raw_matrices();
		self.is_orthographic = true;
		self.orthographic_height = orthographic_height;
		self.near_distance = near_distance;
		self.far_distance = far_distance;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	/// 직교 투영인지.
	///
	/// # 돌려줌
	/// 직교 투영이면 true. (원 행렬이면 투영 행렬에서 가린 값)
	//==============================================================================
	pub fn is_orthographic(&self) -> bool {
		self.is_orthographic
	}

	//==============================================================================
	/// 가로 세로 비 설정. (너비 / 높이, 0 이하이거나 같으면 그대로)
	///
	/// # 인자
	/// - `aspect_ratio`: 가로 세로 비. (너비 / 높이, NaN 이나 0 이하는 무시한다)
	//==============================================================================
	pub fn set_aspect_ratio(&mut self, aspect_ratio: f64) {
		if aspect_ratio.is_nan() || aspect_ratio <= 0.0 || aspect_ratio == self.aspect_ratio {
			return;
		}
		self.aspect_ratio = aspect_ratio;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	/// 가로 세로 비 반환.
	///
	/// # 돌려줌
	/// 가로 세로 비. (너비 / 높이, 기본 1)
	//==============================================================================
	pub fn get_aspect_ratio(&self) -> f64 {
		self.aspect_ratio
	}

	//==============================================================================
	/// 원근의 세로 시야각 반환.
	///
	/// # 돌려줌
	/// 세로 시야각. (라디안, 기본 0.8, 원 행렬을 써도 마지막으로 준 값)
	//==============================================================================
	pub fn get_field_of_view(&self) -> f64 {
		self.field_of_view_radian
	}

	//==============================================================================
	/// 직교의 보이는 높이 반환.
	///
	/// # 돌려줌
	/// 보이는 높이. (월드 단위, 기본 10)
	//==============================================================================
	pub fn get_orthographic_height(&self) -> f64 {
		self.orthographic_height
	}

	//==============================================================================
	/// 가까운 거리 반환.
	///
	/// # 돌려줌
	/// 가까운 거리. (월드 단위, 기본 0.1)
	//==============================================================================
	pub fn get_near_distance(&self) -> f64 {
		self.near_distance
	}

	//==============================================================================
	/// 먼 거리 반환.
	///
	/// # 돌려줌
	/// 먼 거리. (월드 단위, 기본 1000)
	//==============================================================================
	pub fn get_far_distance(&self) -> f64 {
		self.far_distance
	}

	//==============================================================================
	/// 보는 자리 설정.
	/// - set_matrices 로 준 원 행렬이 있으면 버리고 셈하는 길로 돌아간다.
	///
	/// # 인자
	/// - `eye_position`: 눈의 자리. (월드)
	/// - `target_position`: 보는 곳. (월드)
	/// - `up_direction`: 위쪽. (보통 Vector3::up())
	//==============================================================================
	pub fn look_at(&mut self, eye_position: Vector3, target_position: Vector3, up_direction: Vector3) {
		self.clear_raw_matrices();
		self.eye_position = eye_position;
		self.target_position = target_position;
		self.up_direction = up_direction;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	/// 눈의 자리 반환.
	///
	/// # 돌려줌
	/// 눈의 자리. (월드, 원 행렬이면 보기 행렬에서 뽑은 자리)
	//==============================================================================
	pub fn get_eye_position(&self) -> Vector3 {
		self.eye_position
	}

	//==============================================================================
	/// 보는 곳 반환.
	///
	/// # 돌려줌
	/// 보는 곳. (월드, 원 행렬이면 눈 앞 1 의 자리)
	//==============================================================================
	pub fn get_target_position(&self) -> Vector3 {
		self.target_position
	}

	//==============================================================================
	/// 위쪽 반환.
	///
	/// # 돌려줌
	/// 위쪽. (원 행렬이면 보기 행렬의 위쪽 축)
	//==============================================================================
	pub fn get_up_direction(&self) -> Vector3 {
		self.up_direction
	}

	//==============================================================================
	/// 원 행렬 설정. (게임이 셈한 보기와 투영을 그대로 쓴다, 머리말)
	///
	/// # 인자
	/// - `view_matrix`: 보기 행렬. (월드 → 카메라의 강체 변환, 카메라는 -Z 를 본다)
	/// - `projection_matrix`: 투영 행렬. (카메라 → 클립, `elements[11]` 의 절댓값이 0.5 밑이면 직교로 본다)
	///
	/// # 예제
	/// 기울인 직교 투영을 그대로 쓰다가 set_perspective 로 셈하는 길로 돌아가기
	/// ```rust
	/// use vanilla::{Camera3D, Matrix4, Vector3};
	///
	/// let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 5.0, 10.0), Vector3::zero(), Vector3::up());
	/// let mut projection_matrix = Matrix4::create_orthographic(-4.0, 4.0, -2.0, 2.0, 0.1, 50.0);
	/// projection_matrix.elements[8] = 0.3;
	///
	/// let mut camera = Camera3D::new();
	/// camera.set_matrices(&view_matrix, &projection_matrix);
	/// assert!(camera.has_raw_matrices());
	/// assert!(camera.is_orthographic());
	/// assert!((camera.get_eye_position().y - 5.0).abs() < 0.001);
	/// assert_eq!(camera.get_projection_matrix(), projection_matrix);
	///
	/// camera.set_perspective(0.8, 0.1, 100.0);
	/// assert!(!camera.has_raw_matrices());
	/// assert!(!camera.is_orthographic());
	/// ```
	//==============================================================================
	pub fn set_matrices(&mut self, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		let view_elements = view_matrix.read_elements();
		let translation_x = view_elements[12];
		let translation_y = view_elements[13];
		let translation_z = view_elements[14];
		let eye_x = -(view_elements[0] * translation_x + view_elements[1] * translation_y + view_elements[2] * translation_z);
		let eye_y = -(view_elements[4] * translation_x + view_elements[5] * translation_y + view_elements[6] * translation_z);
		let eye_z = -(view_elements[8] * translation_x + view_elements[9] * translation_y + view_elements[10] * translation_z);
		self.raw_matrices = Some([*view_matrix, *projection_matrix]);
		self.is_orthographic = projection_matrix.elements[11].abs() < 0.5;
		self.eye_position = Vector3::create(eye_x, eye_y, eye_z);
		self.target_position = Vector3::create(eye_x - view_elements[2], eye_y - view_elements[6], eye_z - view_elements[10]);
		self.up_direction = Vector3::create(view_elements[1], view_elements[5], view_elements[9]);
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	/// 원 행렬을 쓰는지.
	///
	/// # 돌려줌
	/// set_matrices 로 준 원 행렬을 쓰고 있으면 true.
	//==============================================================================
	pub fn has_raw_matrices(&self) -> bool {
		self.raw_matrices.is_some()
	}

	//==============================================================================
	/// 원 행렬 버리기. (셈하는 길로)
	//==============================================================================
	pub fn clear_raw_matrices(&mut self) {
		if self.raw_matrices.is_none() {
			return;
		}
		self.raw_matrices = None;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	/// 보기 덧붙임 설정. (카메라 지역 축의 자리와 돌림을 보기 행렬 마지막에 건다, 둘 다 영 벡터면 덧붙임 없음)
	///
	/// # 인자
	/// - `position`: 카메라 지역 자리 덧붙임. (장면 길이, x 오른쪽, y 위, z 뒤)
	/// - `rotation`: 카메라 지역 돌림 덧붙임. (라디안, x pitch, y yaw, z roll, 원본 기본 영 벡터는 Vector3::zero() 를 준다)
	///
	/// # 예제
	/// 카메라 흔들림을 걸고 다시 풀기
	/// ```rust
	/// use vanilla::{Camera3D, CameraShake, CameraShakeOptions, Vector3};
	///
	/// let mut camera = Camera3D::new();
	/// camera.look_at(Vector3::create(0.0, 4.0, 10.0), Vector3::zero(), Vector3::up());
	/// let plain_view_matrix = camera.get_view_matrix();
	/// let mut camera_shake = CameraShake::default();
	/// camera_shake.add(CameraShakeOptions { amplitude: 0.1, rotation_amplitude: 0.02, duration: 0.3, ..Default::default() });
	/// camera_shake.update(1.0 / 60.0);
	/// let shake = camera_shake.read();
	/// camera.set_view_offset(shake.position, shake.rotation);
	/// let shaken_view_matrix = camera.get_view_matrix();
	/// camera.set_view_offset(Vector3::zero(), Vector3::zero());
	/// assert_ne!(shaken_view_matrix.get_elements()[12], plain_view_matrix.get_elements()[12]);
	/// assert_eq!(camera.get_view_matrix().get_elements(), plain_view_matrix.get_elements());
	/// ```
	//==============================================================================
	pub fn set_view_offset(&mut self, position: Vector3, rotation: Vector3) {
		self.view_offset_position = position;
		self.view_offset_rotation = rotation;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	/// 행렬 넷 읽기. (바뀐 값이 있으면 다시 셈해 담는다, 원 행렬이 있으면 그것으로, 보기 덧붙임은 마지막에)
	//==============================================================================
	fn read_matrices(&self) -> CameraMatrices {
		if !self.is_matrix_dirty.get() {
			return self.matrices.get();
		}
		let (mut view_matrix, projection_matrix) = match self.raw_matrices {
			Some([raw_view_matrix, raw_projection_matrix]) => {
				(raw_view_matrix, raw_projection_matrix)
			}
			None => {
				(Matrix4::create_look_at(self.eye_position, self.target_position, self.up_direction), self.create_projection_matrix())
			}
		};
		let offset_position = self.view_offset_position;
		let offset_rotation = self.view_offset_rotation;
		let has_view_offset = offset_position.x != 0.0 || offset_position.y != 0.0 || offset_position.z != 0.0 || offset_rotation.x != 0.0 || offset_rotation.y != 0.0 || offset_rotation.z != 0.0;
		if has_view_offset {
			view_matrix = create_offset_view_matrix(&view_matrix, offset_position, offset_rotation);
		}
		let mut view_projection_matrix = projection_matrix;
		view_projection_matrix.multiply(&view_matrix);
		let matrices = CameraMatrices { view_matrix, projection_matrix, view_projection_matrix, inverse_view_projection_matrix: Matrix4::create_inverse(&view_projection_matrix) };
		self.matrices.set(matrices);
		self.is_matrix_dirty.set(false);
		matrices
	}

	//==============================================================================
	/// 셈하는 길의 투영 행렬. (직교나 원근)
	//==============================================================================
	fn create_projection_matrix(&self) -> Matrix4 {
		if self.is_orthographic {
			let half_height = self.orthographic_height * 0.5;
			let half_width = half_height * self.aspect_ratio;
			Matrix4::create_orthographic(-half_width, half_width, -half_height, half_height, self.near_distance, self.far_distance)
		}
		else {
			Matrix4::create_perspective(self.field_of_view_radian, self.aspect_ratio, self.near_distance, self.far_distance)
		}
	}

	//==============================================================================
	/// 보기 행렬 반환. (월드 → 카메라)
	///
	/// # 돌려줌
	/// 보기 행렬. (원 행렬이면 그 행렬, 아니면 눈, 보는 곳, 위쪽으로 셈한 것)
	//==============================================================================
	pub fn get_view_matrix(&self) -> Matrix4 {
		self.read_matrices().view_matrix
	}

	//==============================================================================
	/// 투영 행렬 반환. (카메라 → 클립)
	///
	/// # 돌려줌
	/// 투영 행렬. (원 행렬이면 그 행렬, 아니면 원근이나 직교로 셈한 것)
	//==============================================================================
	pub fn get_projection_matrix(&self) -> Matrix4 {
		self.read_matrices().projection_matrix
	}

	//==============================================================================
	/// 보기와 투영 행렬 반환. (월드 → 클립)
	///
	/// # 돌려줌
	/// 투영 x 보기 행렬. (월드 점을 클립 좌표로)
	//==============================================================================
	pub fn get_view_projection_matrix(&self) -> Matrix4 {
		self.read_matrices().view_projection_matrix
	}

	//==============================================================================
	/// 보기와 투영의 역행렬 반환. (클립 → 월드)
	///
	/// # 돌려줌
	/// 보기와 투영 행렬의 역행렬. (클립 좌표를 월드로)
	//==============================================================================
	pub fn get_inverse_view_projection_matrix(&self) -> Matrix4 {
		self.read_matrices().inverse_view_projection_matrix
	}

	//==============================================================================
	/// 월드 점의 카메라 앞 깊이. (보는 쪽으로 잰 거리, 눈의 면보다 뒤면 음수, 비치는 것을 먼 것부터 그릴 때)
	///
	/// # 인자
	/// - `x`: 월드 점의 x.
	/// - `y`: 월드 점의 y.
	/// - `z`: 월드 점의 z.
	///
	/// # 돌려줌
	/// 카메라 앞 깊이. (보는 쪽으로 잰 거리, 눈의 면보다 뒤면 음수)
	///
	/// # 예제
	/// 반투명한 것을 먼 것부터 그리도록 깊이로 줄 세우기
	/// ```rust
	/// use vanilla::{Camera3D, Vector3};
	///
	/// let camera = Camera3D::new();
	/// let mut position_list = vec![Vector3::create(0.0, 0.0, 5.0), Vector3::create(0.0, 0.0, -5.0), Vector3::zero()];
	/// position_list.sort_by(|left, right| camera.read_view_depth(right.x, right.y, right.z).total_cmp(&camera.read_view_depth(left.x, left.y, left.z)));
	/// assert_eq!(position_list[0], Vector3::create(0.0, 0.0, -5.0));
	/// assert!((camera.read_view_depth(0.0, 0.0, 0.0) - 10.0).abs() < 0.001);
	/// ```
	//==============================================================================
	pub fn read_view_depth(&self, x: f64, y: f64, z: f64) -> f64 {
		let view_elements = self.read_matrices().view_matrix.read_elements();
		-(view_elements[2] * x + view_elements[6] * y + view_elements[10] * z + view_elements[14])
	}

	//==============================================================================
	/// 월드 점을 view_rect 안의 자리로. (view_rect 의 좌표, 왼쪽 위가 원점이고 아래로 늘어남, 화면 밖인지는 부르는 쪽이 정한다)
	///
	/// # 인자
	/// - `x`: 월드 점의 x.
	/// - `y`: 월드 점의 y.
	/// - `z`: 월드 점의 z.
	/// - `view_rect`: 3D 를 그리는 사각형. (부르는 쪽의 좌표, 보통 뷰 좌표)
	///
	/// # 돌려줌
	/// view_rect 안의 자리와 깊이. 깊이가 0.0001 밑(눈의 면 뒤)이거나 클립 w 가 0 이면 is_in_front 가 false 이고 x, y 는 0.
	///
	/// # 예제
	/// 화면 오른쪽 절반에 그리는 3D 의 원점 위에 이름표를 얹을 자리
	/// ```rust
	/// use vanilla::{Camera3D, Rect};
	///
	/// let camera = Camera3D::new();
	/// let view_rect = Rect::create(640.0, 0.0, 640.0, 720.0);
	/// let projected_point = camera.project_to_view(0.0, 0.0, 0.0, view_rect);
	/// if projected_point.is_in_front {
	/// 	let label_x = projected_point.x;
	/// 	let label_y = projected_point.y - 24.0;
	/// 	assert!((label_x - 960.0).abs() < 0.01);
	/// 	assert!((label_y - 336.0).abs() < 0.01);
	/// }
	/// ```
	//==============================================================================
	pub fn project_to_view(&self, x: f64, y: f64, z: f64, view_rect: Rect) -> ProjectedPoint {
		let clip = self.read_matrices().view_projection_matrix.transform_point(x, y, z);
		let depth = self.read_view_depth(x, y, z);
		if depth < MINIMUM_FRONT_DEPTH || clip.w.abs() < MINIMUM_FRONT_DEPTH {
			return ProjectedPoint { x: 0.0, y: 0.0, depth, is_in_front: false };
		}
		let normalized_x = clip.x / clip.w;
		let normalized_y = clip.y / clip.w;
		ProjectedPoint {
			x: view_rect.position.x + (normalized_x * 0.5 + 0.5) * view_rect.size.x,
			y: view_rect.position.y + (0.5 - normalized_y * 0.5) * view_rect.size.y,
			depth,
			is_in_front: true,
		}
	}

	//==============================================================================
	/// view_rect 안의 자리에서 나간 시선이 수평면 y = plane_height 와 만나는 점. (나란하거나 눈 뒤에서 만나면 None)
	///
	/// # 인자
	/// - `view_position`: 시선이 나가는 자리. (view_rect 와 같은 좌표, 누른 자리 같은 것)
	/// - `plane_height`: 수평면의 높이. (월드 y)
	/// - `view_rect`: 3D 를 그리는 사각형. (부르는 쪽의 좌표, 보통 뷰 좌표)
	///
	/// # 돌려줌
	/// 시선이 수평면과 만나는 월드 점. (y 는 plane_height) view_rect 가 비었거나, 역투영의 w 가 0 이거나, 시선이 수평면과 나란하거나, 눈 뒤에서 만나면 None.
	///
	/// # 예제
	/// 앞을 보는 카메라에서 아래 끝은 바닥에 닿고, 위 끝은 하늘이라 None
	/// ```rust
	/// use vanilla::{Camera3D, Rect, Vector2, Vector3};
	///
	/// let mut camera = Camera3D::new();
	/// camera.look_at(Vector3::create(0.0, 1.0, 0.0), Vector3::create(0.0, 1.0, -10.0), Vector3::up());
	/// let view_rect = Rect::create(0.0, 0.0, 400.0, 400.0);
	///
	/// let ground_point = camera.unproject_to_plane(Vector2::create(200.0, 400.0), 0.0, view_rect).unwrap();
	/// assert_eq!(ground_point.y, 0.0);
	/// assert!(ground_point.z < 0.0);
	/// assert_eq!(camera.unproject_to_plane(Vector2::create(200.0, 0.0), 0.0, view_rect), None);
	/// ```
	//==============================================================================
	pub fn unproject_to_plane(&self, view_position: Vector2, plane_height: f64, view_rect: Rect) -> Option<Vector3> {
		if view_rect.size.x <= 0.0 || view_rect.size.y <= 0.0 {
			return None;
		}
		let normalized_x = (view_position.x - view_rect.position.x) / view_rect.size.x * 2.0 - 1.0;
		let normalized_y = 1.0 - (view_position.y - view_rect.position.y) / view_rect.size.y * 2.0;
		let inverse_matrix = self.read_matrices().inverse_view_projection_matrix;
		let near_point = inverse_matrix.transform_point(normalized_x, normalized_y, -1.0);
		let far_point = inverse_matrix.transform_point(normalized_x, normalized_y, 1.0);
		if near_point.w == 0.0 || far_point.w == 0.0 {
			return None;
		}
		let near = Vector3::create(near_point.x / near_point.w, near_point.y / near_point.w, near_point.z / near_point.w);
		let far = Vector3::create(far_point.x / far_point.w, far_point.y / far_point.w, far_point.z / far_point.w);
		let delta_y = far.y - near.y;
		if delta_y.abs() < 0.000001 {
			return None;
		}
		let ratio = (plane_height - near.y) / delta_y;
		if ratio < 0.0 {
			return None;
		}
		Some(Vector3::create(near.x + (far.x - near.x) * ratio, plane_height, near.z + (far.z - near.z) * ratio))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 시험용 카메라. (눈 (0, 5, 10) 에서 원점, 비 2)
	//==============================================================================
	fn create_test_camera() -> Camera3D {
		let mut camera = Camera3D::new();
		camera.set_perspective(0.8, 0.1, 100.0);
		camera.set_aspect_ratio(2.0);
		camera.look_at(Vector3::create(0.0, 5.0, 10.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		camera
	}

	//==============================================================================
	/// 보는 곳은 사각형 가운데로 가고, 눈 뒤의 점은 앞이 아니다.
	//==============================================================================
	#[test]
	fn project_target_to_rect_center() {
		let camera = create_test_camera();
		let view_rect = Rect::create(100.0, 50.0, 400.0, 200.0);
		let projected = camera.project_to_view(0.0, 0.0, 0.0, view_rect);
		assert!(projected.is_in_front);
		assert!((projected.x - 300.0).abs() < 0.01);
		assert!((projected.y - 150.0).abs() < 0.01);
		assert!((projected.depth - 125.0f64.sqrt()).abs() < 0.001);
		let behind = camera.project_to_view(0.0, 5.0, 20.0, view_rect);
		assert!(!behind.is_in_front);
	}

	//==============================================================================
	/// 투영한 자리를 같은 높이의 수평면에 되돌리면 처음 점이 나온다.
	//==============================================================================
	#[test]
	fn unproject_returns_projected_point() {
		let camera = create_test_camera();
		let view_rect = Rect::create(100.0, 50.0, 400.0, 200.0);
		let projected = camera.project_to_view(1.5, 0.7, -2.0, view_rect);
		let unprojected = camera.unproject_to_plane(Vector2::create(projected.x, projected.y), 0.7, view_rect).unwrap();
		assert!((unprojected.x - 1.5).abs() < 0.01);
		assert!((unprojected.z + 2.0).abs() < 0.01);
		let sky = camera.unproject_to_plane(Vector2::create(300.0, 50.0), 0.0, Rect::create(100.0, 50.0, 400.0, 200.0));
		assert!(sky.is_some());
	}

	//==============================================================================
	/// 행렬은 값이 바뀐 뒤에만 다시 셈한다.
	//==============================================================================
	#[test]
	fn matrices_refresh_only_after_change() {
		let mut camera = create_test_camera();
		let first_matrix = camera.get_view_projection_matrix();
		assert!(!camera.is_matrix_dirty.get());
		assert_eq!(first_matrix, camera.get_view_projection_matrix());
		camera.set_aspect_ratio(1.0);
		assert!(camera.is_matrix_dirty.get());
		assert_ne!(first_matrix, camera.get_view_projection_matrix());
	}

	//==============================================================================
	/// 직교 투영의 보이는 구석이 사각형 구석으로 간다. (너비는 높이 x 비)
	//==============================================================================
	#[test]
	fn orthographic_corner_maps_to_rect_corner() {
		let mut camera = Camera3D::new();
		camera.set_orthographic(10.0, 0.1, 100.0);
		camera.set_aspect_ratio(2.0);
		camera.look_at(Vector3::create(0.0, 0.0, 10.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let projected = camera.project_to_view(10.0, 5.0, 0.0, Rect::create(0.0, 0.0, 200.0, 100.0));
		assert!((projected.x - 200.0).abs() < 0.01);
		assert!(projected.y.abs() < 0.01);
	}

	//==============================================================================
	/// 원 행렬이 투영을 맡고, set_perspective 를 부르면 셈하는 길로 돌아간다.
	//==============================================================================
	#[test]
	fn raw_matrices_drive_projection_until_cleared() {
		let computed_camera = create_test_camera();
		let view_rect = Rect::create(100.0, 50.0, 400.0, 200.0);
		let mut raw_camera = Camera3D::new();
		raw_camera.set_matrices(&computed_camera.get_view_matrix(), &computed_camera.get_projection_matrix());
		assert!(raw_camera.has_raw_matrices());
		assert!(!raw_camera.is_orthographic());
		let computed_point = computed_camera.project_to_view(1.5, 0.7, -2.0, view_rect);
		let raw_point = raw_camera.project_to_view(1.5, 0.7, -2.0, view_rect);
		assert!((computed_point.x - raw_point.x).abs() < 0.001 && (computed_point.y - raw_point.y).abs() < 0.001);
		assert!((computed_point.depth - raw_point.depth).abs() < 0.001);
		let eye_position = raw_camera.get_eye_position();
		assert!(eye_position.x.abs() < 0.001 && (eye_position.y - 5.0).abs() < 0.001 && (eye_position.z - 10.0).abs() < 0.001);
		let mut orthographic_projection = Matrix4::create_orthographic(-4.0, 4.0, -2.0, 2.0, 0.1, 50.0);
		orthographic_projection.elements[8] = 0.3;
		raw_camera.set_matrices(&computed_camera.get_view_matrix(), &orthographic_projection);
		assert!(raw_camera.is_orthographic());
		let mut expected_matrix = orthographic_projection;
		expected_matrix.multiply(&computed_camera.get_view_matrix());
		assert_eq!(raw_camera.get_view_projection_matrix(), expected_matrix);
		raw_camera.set_perspective(0.8, 0.1, 100.0);
		assert!(!raw_camera.has_raw_matrices());
		assert!(!raw_camera.is_orthographic());
	}
}
