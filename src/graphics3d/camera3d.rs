//==============================================================================
// 3D 카메라. (vanilla.js experimental/graphics/camera3d.js 의 Camera3D)
// - 원근이나 직교 투영과 보는 자리를 들고, 보기와 투영 행렬을 바뀔 때만 다시 셈한다.
// - 좌표는 엔진 Matrix4 와 같은 y 가 위인 오른손 좌표다. 카메라는 -Z 를 본다. (create_look_at, create_perspective)
// - 원근: set_perspective(시야각, 가까운 거리, 먼 거리), 직교: set_orthographic(보이는 높이, 가까운 거리, 먼 거리).
//   가로 세로 비는 set_aspect_ratio 로 따로 준다. (그리는 사각형이 바뀔 때마다 고쳐 넣는다, 직교의 너비는 높이 x 비)
// - 행렬은 Cell 에 담아 두고 &self 로 읽을 때 바뀐 값이 있으면 다시 셈한다. (원본은 getter 가 #isMatrixDirty 를 보고 셈한다)
// - 같은 카메라를 렌더러(ToonRenderer::begin_frame_with_camera)와 2D 표시(project_to_view)가 함께 써, 3D 점 위에 글이나 단추를 얹는다.
// - project_to_view(x, y, z, view_rect): 월드 점을 view_rect 안의 자리로. view_rect 는 3D 를 그리는 사각형을 부르는 쪽의 좌표(보통 뷰 좌표)로 준 것이다.
//   원본의 { x, y, depth, isInFront } 는 ProjectedPoint 로 돌려준다.
// - unproject_to_plane(view_position, plane_height, view_rect): view_rect 안의 자리에서 나간 시선이 수평면(y = plane_height)과 만나는 점. 없으면 None.
//==============================================================================
use std::cell::Cell;

use crate::base::matrix4::Matrix4;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::base::vector3::Vector3;

// 카메라 앞으로 칠 가장 작은 깊이. (이보다 가까우면 투영하지 않는다)
const MINIMUM_FRONT_DEPTH: f32 = 0.0001;

//==============================================================================
// 투영한 점. (view_rect 의 좌표, 카메라 앞 깊이, 눈의 면보다 앞인지, 앞이 아니면 x, y 는 0)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProjectedPoint {
	pub x: f32,
	pub y: f32,
	pub depth: f32,
	pub is_in_front: bool,
}

//==============================================================================
// 담아 둔 행렬 넷.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct CameraMatrices {
	view_matrix: Matrix4,
	projection_matrix: Matrix4,
	view_projection_matrix: Matrix4,
	inverse_view_projection_matrix: Matrix4,
}

#[derive(Clone, Debug)]
pub struct Camera3D {
	is_orthographic: bool,
	field_of_view_radian: f32,
	orthographic_height: f32,
	aspect_ratio: f32,
	near_distance: f32,
	far_distance: f32,
	eye_position: Vector3,
	target_position: Vector3,
	up_direction: Vector3,
	matrices: Cell<CameraMatrices>,
	is_matrix_dirty: Cell<bool>,
}

impl Default for Camera3D {
	fn default() -> Camera3D {
		Camera3D::new()
	}
}

impl Camera3D {
	//==============================================================================
	// 생성. (원근 시야각 0.8 라디안, 비 1, 거리 0.1 ~ 1000, 눈 (0, 0, 10) 에서 원점을 본다)
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
		}
	}

	//==============================================================================
	// 원근 투영 설정. (세로 시야각)
	//==============================================================================
	pub fn set_perspective(&mut self, field_of_view_radian: f32, near_distance: f32, far_distance: f32) {
		self.is_orthographic = false;
		self.field_of_view_radian = field_of_view_radian;
		self.near_distance = near_distance;
		self.far_distance = far_distance;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	// 직교 투영 설정. (보이는 높이, 너비는 높이 x 가로 세로 비)
	//==============================================================================
	pub fn set_orthographic(&mut self, orthographic_height: f32, near_distance: f32, far_distance: f32) {
		self.is_orthographic = true;
		self.orthographic_height = orthographic_height;
		self.near_distance = near_distance;
		self.far_distance = far_distance;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	// 직교 투영인지.
	//==============================================================================
	pub fn is_orthographic(&self) -> bool {
		self.is_orthographic
	}

	//==============================================================================
	// 가로 세로 비 설정. (너비 / 높이, 0 이하이거나 같으면 그대로)
	//==============================================================================
	pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
		if aspect_ratio.is_nan() || aspect_ratio <= 0.0 || aspect_ratio == self.aspect_ratio {
			return;
		}
		self.aspect_ratio = aspect_ratio;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	// 가로 세로 비 반환.
	//==============================================================================
	pub fn get_aspect_ratio(&self) -> f32 {
		self.aspect_ratio
	}

	//==============================================================================
	// 원근의 세로 시야각 반환.
	//==============================================================================
	pub fn get_field_of_view(&self) -> f32 {
		self.field_of_view_radian
	}

	//==============================================================================
	// 직교의 보이는 높이 반환.
	//==============================================================================
	pub fn get_orthographic_height(&self) -> f32 {
		self.orthographic_height
	}

	//==============================================================================
	// 가까운 거리 반환.
	//==============================================================================
	pub fn get_near_distance(&self) -> f32 {
		self.near_distance
	}

	//==============================================================================
	// 먼 거리 반환.
	//==============================================================================
	pub fn get_far_distance(&self) -> f32 {
		self.far_distance
	}

	//==============================================================================
	// 보는 자리 설정.
	//==============================================================================
	pub fn look_at(&mut self, eye_position: Vector3, target_position: Vector3, up_direction: Vector3) {
		self.eye_position = eye_position;
		self.target_position = target_position;
		self.up_direction = up_direction;
		self.is_matrix_dirty.set(true);
	}

	//==============================================================================
	// 눈의 자리 반환.
	//==============================================================================
	pub fn get_eye_position(&self) -> Vector3 {
		self.eye_position
	}

	//==============================================================================
	// 보는 곳 반환.
	//==============================================================================
	pub fn get_target_position(&self) -> Vector3 {
		self.target_position
	}

	//==============================================================================
	// 위쪽 반환.
	//==============================================================================
	pub fn get_up_direction(&self) -> Vector3 {
		self.up_direction
	}

	//==============================================================================
	// 행렬 넷 읽기. (바뀐 값이 있으면 다시 셈해 담는다)
	//==============================================================================
	fn read_matrices(&self) -> CameraMatrices {
		if !self.is_matrix_dirty.get() {
			return self.matrices.get();
		}
		let view_matrix = Matrix4::create_look_at(self.eye_position, self.target_position, self.up_direction);
		let projection_matrix = if self.is_orthographic {
			let half_height = self.orthographic_height * 0.5;
			let half_width = half_height * self.aspect_ratio;
			Matrix4::create_orthographic(-half_width, half_width, -half_height, half_height, self.near_distance, self.far_distance)
		}
		else {
			Matrix4::create_perspective(self.field_of_view_radian, self.aspect_ratio, self.near_distance, self.far_distance)
		};
		let mut view_projection_matrix = projection_matrix;
		view_projection_matrix.multiply(&view_matrix);
		let matrices = CameraMatrices { view_matrix, projection_matrix, view_projection_matrix, inverse_view_projection_matrix: Matrix4::create_inverse(&view_projection_matrix) };
		self.matrices.set(matrices);
		self.is_matrix_dirty.set(false);
		matrices
	}

	//==============================================================================
	// 보기 행렬 반환. (월드 → 카메라)
	//==============================================================================
	pub fn get_view_matrix(&self) -> Matrix4 {
		self.read_matrices().view_matrix
	}

	//==============================================================================
	// 투영 행렬 반환. (카메라 → 클립)
	//==============================================================================
	pub fn get_projection_matrix(&self) -> Matrix4 {
		self.read_matrices().projection_matrix
	}

	//==============================================================================
	// 보기와 투영 행렬 반환. (월드 → 클립)
	//==============================================================================
	pub fn get_view_projection_matrix(&self) -> Matrix4 {
		self.read_matrices().view_projection_matrix
	}

	//==============================================================================
	// 보기와 투영의 역행렬 반환. (클립 → 월드)
	//==============================================================================
	pub fn get_inverse_view_projection_matrix(&self) -> Matrix4 {
		self.read_matrices().inverse_view_projection_matrix
	}

	//==============================================================================
	// 월드 점의 카메라 앞 깊이. (보는 쪽으로 잰 거리, 눈의 면보다 뒤면 음수, 비치는 것을 먼 것부터 그릴 때)
	//==============================================================================
	pub fn read_view_depth(&self, x: f32, y: f32, z: f32) -> f32 {
		let view_elements = self.read_matrices().view_matrix.elements;
		-(view_elements[2] * x + view_elements[6] * y + view_elements[10] * z + view_elements[14])
	}

	//==============================================================================
	// 월드 점을 view_rect 안의 자리로. (view_rect 의 좌표, 왼쪽 위가 원점이고 아래로 늘어남, 화면 밖인지는 부르는 쪽이 정한다)
	//==============================================================================
	pub fn project_to_view(&self, x: f32, y: f32, z: f32, view_rect: Rect) -> ProjectedPoint {
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
	// view_rect 안의 자리에서 나간 시선이 수평면 y = plane_height 와 만나는 점. (나란하거나 눈 뒤에서 만나면 None)
	//==============================================================================
	pub fn unproject_to_plane(&self, view_position: Vector2, plane_height: f32, view_rect: Rect) -> Option<Vector3> {
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
	// 시험용 카메라. (눈 (0, 5, 10) 에서 원점, 비 2)
	//==============================================================================
	fn create_test_camera() -> Camera3D {
		let mut camera = Camera3D::new();
		camera.set_perspective(0.8, 0.1, 100.0);
		camera.set_aspect_ratio(2.0);
		camera.look_at(Vector3::create(0.0, 5.0, 10.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		camera
	}

	#[test]
	fn project_target_to_rect_center() {
		let camera = create_test_camera();
		let view_rect = Rect::create(100.0, 50.0, 400.0, 200.0);
		let projected = camera.project_to_view(0.0, 0.0, 0.0, view_rect);
		assert!(projected.is_in_front);
		assert!((projected.x - 300.0).abs() < 0.01);
		assert!((projected.y - 150.0).abs() < 0.01);
		assert!((projected.depth - 125.0f32.sqrt()).abs() < 0.001);
		let behind = camera.project_to_view(0.0, 5.0, 20.0, view_rect);
		assert!(!behind.is_in_front);
	}

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
}
