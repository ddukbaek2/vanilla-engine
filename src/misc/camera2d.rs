//==============================================================================
//! 2D 카메라.
//! - 원본 misc/camera2d.js 를 옮긴 것이다. 추적, 월드 경계 클램프, 드래그 팬, 커서 고정 줌, 관성, 이동 트윈을 한 곳에 모았다.
//! - position 은 카메라가 바라보는 월드 좌표(화면 가운데), zoom 은 확대 배율, rotation 은 화면 가운데를 축으로 도는 각도(도)다.
//!   rotation 만큼 월드가 화면에서 돈다. (원본 experimental/camera.js 의 begin 이 scale 뒤 rotate 를 거는 것과 같은 방향)
//! - 원본처럼 컴포넌트가 아니라 그냥 값이다. 들고 있는 쪽(씬 등)이 tick 을 부르고, 월드를 그리기 전에 apply_transform 을 건다.
//! - 사용:
//!   ```text
//!     camera.set_view_size(view_size);                    // 매 프레임 또는 리사이즈 시
//!     camera.follow(&[hero_position], 6.0, time_delta);  // 추적 (선택)
//!     camera.tick(time_delta);
//!     camera.begin(graphic); ...월드 렌더... camera.end(graphic);   // push_state + apply_transform / pop_state
//!   ```
//! - 원본과 다른 점:
//!   이름을 줄이지 않는 규칙에 따라 minZoom / maxZoom 을 minimum_zoom / maximum_zoom 으로 쓴다. (set_zoom_range 의 뜻은 같다)
//!   tween_to 의 기본 길이 0.4 초는 DEFAULT_TWEEN_DURATION 상수로 넘긴다. (러스트는 기본 인자가 없다)
//==============================================================================
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

//==============================================================================
/// 이동 트윈 상태. (원본 #tween_state 객체)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct CameraTweenState {
	from_position: Vector2,
	to_position: Vector2,
	from_zoom: f32,
	to_zoom: f32,
	duration: f32,
	elapsed_seconds: f32,
}

//==============================================================================
/// 2D 카메라.
//==============================================================================
#[derive(Clone, Debug)]
pub struct Camera2D {
	position: Vector2,
	zoom: f32,
	rotation: f32,
	view_size: Vector2,
	world_bounds: Option<Rect>,
	velocity: Vector2,
	inertia_damping: f32,
	minimum_zoom: f32,
	maximum_zoom: f32,
	tween_state: Option<CameraTweenState>,
}

impl Camera2D {
	/// tween_to 의 기본 길이. (초, 원본 duration = 0.4)
	pub const DEFAULT_TWEEN_DURATION: f32 = 0.4;

	//==============================================================================
	/// 생성. (위치 0, 배율 1, 회전 0, 뷰 960x640, 경계 없음, 관성 감쇠 6, 배율 범위 0.1 ~ 8)
	//==============================================================================
	pub fn new() -> Camera2D {
		Camera2D {
			position: Vector2::zero(),
			zoom: 1.0,
			rotation: 0.0,
			view_size: Vector2::create(960.0, 640.0),
			world_bounds: None,
			velocity: Vector2::zero(),
			inertia_damping: 6.0,
			minimum_zoom: 0.1,
			maximum_zoom: 8.0,
			tween_state: None,
		}
	}

	//==============================================================================
	/// 갱신. (관성 이동, 이동 트윈, 경계 클램프 순서)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		// 관성.
		if self.velocity.x != 0.0 || self.velocity.y != 0.0 {
			self.position = Vector2::create(self.position.x + self.velocity.x * time_delta, self.position.y + self.velocity.y * time_delta);
			let damping = (-self.inertia_damping * time_delta).exp();
			self.velocity = Vector2::create(self.velocity.x * damping, self.velocity.y * damping);
			if self.velocity.x.abs() < 1.0 && self.velocity.y.abs() < 1.0 {
				self.velocity = Vector2::zero();
			}
		}

		// 이동 트윈. (줌은 등비 보간으로 미끄러짐 없이)
		if let Some(mut tween_state) = self.tween_state {
			tween_state.elapsed_seconds += time_delta;
			let linear_ratio = (tween_state.elapsed_seconds / tween_state.duration).min(1.0);
			let eased_ratio = 1.0 - (1.0 - linear_ratio).powi(3);
			self.zoom = tween_state.from_zoom * (tween_state.to_zoom / tween_state.from_zoom).powf(eased_ratio);
			let position_x = math::lerp(tween_state.from_position.x, tween_state.to_position.x, eased_ratio);
			let position_y = math::lerp(tween_state.from_position.y, tween_state.to_position.y, eased_ratio);
			self.position = Vector2::create(position_x, position_y);
			if linear_ratio >= 1.0 {
				self.tween_state = None;
			}
			else {
				self.tween_state = Some(tween_state);
			}
		}

		self.clamp_to_bounds();
	}

	//==============================================================================
	/// 대상 추적. (여러 대상이면 중점을 따라간다. 프레임 독립 지수 감쇠, rate 가 클수록 빨리 붙는다)
	//==============================================================================
	pub fn follow(&mut self, target_positions: &[Vector2], rate: f32, time_delta: f32) {
		if target_positions.is_empty() {
			return;
		}
		let mut center_x = 0.0;
		let mut center_y = 0.0;
		for target_position in target_positions {
			center_x += target_position.x;
			center_y += target_position.y;
		}
		let target_count = target_positions.len() as f32;
		center_x /= target_count;
		center_y /= target_count;
		let position_x = math::approach(self.position.x, center_x, rate, time_delta);
		let position_y = math::approach(self.position.y, center_y, rate, time_delta);
		self.position = Vector2::create(position_x, position_y);
	}

	//==============================================================================
	/// 화면 픽셀 단위 팬. (드래그 팬. 줌과 회전을 반영해 월드 이동량으로 바꾼다)
	//==============================================================================
	pub fn pan_by_screen(&mut self, screen_delta_x: f32, screen_delta_y: f32) {
		let world_delta = self.convert_screen_delta_to_world(screen_delta_x, screen_delta_y);
		self.position = Vector2::create(self.position.x - world_delta.x, self.position.y - world_delta.y);
		self.clamp_to_bounds();
	}

	//==============================================================================
	/// 관성 던지기. (드래그를 놓을 때 화면 픽셀 속도를 넘긴다)
	//==============================================================================
	pub fn fling(&mut self, screen_velocity_x: f32, screen_velocity_y: f32) {
		let world_velocity = self.convert_screen_delta_to_world(screen_velocity_x, screen_velocity_y);
		self.velocity = Vector2::create(-world_velocity.x, -world_velocity.y);
	}

	//==============================================================================
	/// 화면 픽셀 이동량 → 월드 이동량. (회전을 되돌리고 줌으로 나눈다)
	//==============================================================================
	pub fn convert_screen_delta_to_world(&self, screen_delta_x: f32, screen_delta_y: f32) -> Vector2 {
		let radian = math::degree_to_radian(self.rotation);
		let cosine = radian.cos();
		let sine = radian.sin();
		let unrotated_x = screen_delta_x * cosine + screen_delta_y * sine;
		let unrotated_y = -screen_delta_x * sine + screen_delta_y * cosine;
		Vector2::create(unrotated_x / self.zoom, unrotated_y / self.zoom)
	}

	//==============================================================================
	/// 화면 기준점을 고정한 줌. (휠 / 핀치. anchor_screen 아래의 월드 지점이 움직이지 않는다, 1.1 = 10% 확대)
	//==============================================================================
	pub fn zoom_at(&mut self, anchor_screen: Vector2, zoom_factor: f32) {
		let previous_zoom = self.zoom;
		let next_zoom = math::clamp(previous_zoom * zoom_factor, self.minimum_zoom, self.maximum_zoom);
		if next_zoom == previous_zoom {
			return;
		}
		let anchor_world = self.screen_to_world(anchor_screen);
		self.zoom = next_zoom;
		let anchor_world_after = self.screen_to_world(anchor_screen);
		self.position = Vector2::create(self.position.x + (anchor_world.x - anchor_world_after.x), self.position.y + (anchor_world.y - anchor_world_after.y));
		self.clamp_to_bounds();
	}

	//==============================================================================
	/// 위치 / 줌으로 이동 트윈 시작. (관성은 멈춘다. 기본 길이는 DEFAULT_TWEEN_DURATION)
	//==============================================================================
	pub fn tween_to(&mut self, target_position: Vector2, target_zoom: f32, duration: f32) {
		self.velocity = Vector2::zero();
		self.tween_state = Some(CameraTweenState {
			from_position: self.position,
			to_position: target_position,
			from_zoom: self.zoom,
			to_zoom: math::clamp(target_zoom, self.minimum_zoom, self.maximum_zoom),
			duration: duration.max(0.0001),
			elapsed_seconds: 0.0,
		});
	}

	//==============================================================================
	/// 그리기 변환 적용. (부른 쪽에서 push_state / pop_state 로 감싼다)
	//==============================================================================
	pub fn apply_transform(&self, graphic: &mut Graphic) {
		graphic.translate(self.view_size.x * 0.5, self.view_size.y * 0.5);
		graphic.scale(self.zoom, self.zoom);
		graphic.rotate(math::degree_to_radian(self.rotation));
		graphic.translate(-self.position.x, -self.position.y);
	}

	//==============================================================================
	/// 그리기 시작. (상태를 쌓고 변환을 건다, 원본 experimental/camera.js 의 begin)
	//==============================================================================
	pub fn begin(&self, graphic: &mut Graphic) {
		graphic.push_state();
		self.apply_transform(graphic);
	}

	//==============================================================================
	/// 그리기 끝. (begin 이 쌓은 상태를 되돌린다, 원본 experimental/camera.js 의 end)
	//==============================================================================
	pub fn end(&self, graphic: &mut Graphic) {
		graphic.pop_state();
	}

	//==============================================================================
	/// 화면 좌표 → 월드 좌표.
	//==============================================================================
	pub fn screen_to_world(&self, screen_position: Vector2) -> Vector2 {
		let screen_delta_x = screen_position.x - self.view_size.x * 0.5;
		let screen_delta_y = screen_position.y - self.view_size.y * 0.5;
		let world_delta = self.convert_screen_delta_to_world(screen_delta_x, screen_delta_y);
		Vector2::create(world_delta.x + self.position.x, world_delta.y + self.position.y)
	}

	//==============================================================================
	/// 월드 좌표 → 화면 좌표.
	//==============================================================================
	pub fn world_to_screen(&self, world_position: Vector2) -> Vector2 {
		let radian = math::degree_to_radian(self.rotation);
		let cosine = radian.cos();
		let sine = radian.sin();
		let scaled_x = (world_position.x - self.position.x) * self.zoom;
		let scaled_y = (world_position.y - self.position.y) * self.zoom;
		let screen_x = scaled_x * cosine - scaled_y * sine + self.view_size.x * 0.5;
		let screen_y = scaled_x * sine + scaled_y * cosine + self.view_size.y * 0.5;
		Vector2::create(screen_x, screen_y)
	}

	//==============================================================================
	/// 화면에 보이는 월드 영역의 반 너비, 반 높이. (돌아간 뷰를 감싸는 축 정렬 상자)
	//==============================================================================
	pub fn get_visible_half_extent(&self) -> Vector2 {
		let half_view_width = self.view_size.x * 0.5 / self.zoom;
		let half_view_height = self.view_size.y * 0.5 / self.zoom;
		let radian = math::degree_to_radian(self.rotation);
		let cosine = radian.cos().abs();
		let sine = radian.sin().abs();
		Vector2::create(half_view_width * cosine + half_view_height * sine, half_view_width * sine + half_view_height * cosine)
	}

	//==============================================================================
	/// 월드 경계로 클램프. (보이는 영역이 경계 안에 들게 한다. 보이는 영역이 경계보다 크면(줌아웃) 그 축은 가운데에 맞춘다)
	/// - 돌아가 있으면 돌아간 뷰를 감싸는 축 정렬 상자로 잰다.
	//==============================================================================
	pub fn clamp_to_bounds(&mut self) {
		let bounds = match self.world_bounds {
			Some(bounds) => {
				bounds
			}
			None => {
				return;
			}
		};
		let visible_half_extent = self.get_visible_half_extent();
		let half_view_width = visible_half_extent.x;
		let half_view_height = visible_half_extent.y;
		let clamped_x = if half_view_width * 2.0 >= bounds.size.x {
			bounds.position.x + bounds.size.x * 0.5
		}
		else {
			math::clamp(self.position.x, bounds.position.x + half_view_width, bounds.position.x + bounds.size.x - half_view_width)
		};
		let clamped_y = if half_view_height * 2.0 >= bounds.size.y {
			bounds.position.y + bounds.size.y * 0.5
		}
		else {
			math::clamp(self.position.y, bounds.position.y + half_view_height, bounds.position.y + bounds.size.y - half_view_height)
		};
		self.position = Vector2::create(clamped_x, clamped_y);
	}

	//==============================================================================
	/// 위치 설정. (경계로 클램프한다)
	//==============================================================================
	pub fn set_position(&mut self, position: Vector2) {
		self.position = position;
		self.clamp_to_bounds();
	}

	//==============================================================================
	/// 위치 반환.
	//==============================================================================
	pub fn get_position(&self) -> Vector2 {
		self.position
	}

	//==============================================================================
	/// 배율 설정. (배율 범위로 자르고 경계로 클램프한다)
	//==============================================================================
	pub fn set_zoom(&mut self, zoom: f32) {
		self.zoom = math::clamp(zoom, self.minimum_zoom, self.maximum_zoom);
		self.clamp_to_bounds();
	}

	//==============================================================================
	/// 배율 반환.
	//==============================================================================
	pub fn get_zoom(&self) -> f32 {
		self.zoom
	}

	//==============================================================================
	/// 회전 설정. (도, 화면 가운데를 축으로 돈다. 경계로 클램프한다)
	//==============================================================================
	pub fn set_rotation(&mut self, rotation: f32) {
		self.rotation = rotation;
		self.clamp_to_bounds();
	}

	//==============================================================================
	/// 회전 반환. (도)
	//==============================================================================
	pub fn get_rotation(&self) -> f32 {
		self.rotation
	}

	//==============================================================================
	/// 뷰 크기 설정. (화면 가운데를 정하는 크기)
	//==============================================================================
	pub fn set_view_size(&mut self, view_size: Vector2) {
		self.view_size = view_size;
	}

	//==============================================================================
	/// 뷰 크기 반환.
	//==============================================================================
	pub fn get_view_size(&self) -> Vector2 {
		self.view_size
	}

	//==============================================================================
	/// 월드 경계 설정. (None 이면 경계 없음)
	//==============================================================================
	pub fn set_world_bounds(&mut self, world_bounds: Option<Rect>) {
		self.world_bounds = world_bounds;
		self.clamp_to_bounds();
	}

	//==============================================================================
	/// 월드 경계 반환.
	//==============================================================================
	pub fn get_world_bounds(&self) -> Option<Rect> {
		self.world_bounds
	}

	//==============================================================================
	/// 배율 범위 설정. (지금 배율도 새 범위로 다시 자른다)
	//==============================================================================
	pub fn set_zoom_range(&mut self, minimum_zoom: f32, maximum_zoom: f32) {
		self.minimum_zoom = minimum_zoom;
		self.maximum_zoom = maximum_zoom;
		let zoom = self.zoom;
		self.set_zoom(zoom);
	}

	//==============================================================================
	/// 관성 감쇠 설정. (클수록 빨리 멈춘다)
	//==============================================================================
	pub fn set_inertia_damping(&mut self, inertia_damping: f32) {
		self.inertia_damping = inertia_damping;
	}

	//==============================================================================
	/// 이동 트윈 중인지.
	//==============================================================================
	pub fn is_tweening(&self) -> bool {
		self.tween_state.is_some()
	}

	//==============================================================================
	/// 관성 정지.
	//==============================================================================
	pub fn stop_inertia(&mut self) {
		self.velocity = Vector2::zero();
	}
}

impl Default for Camera2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Camera2D {
		Camera2D::new()
	}
}

//==============================================================================
/// 시험. (GL 없이 돈다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 두 값이 거의 같은지.
	//==============================================================================
	fn is_near(left: f32, right: f32) -> bool {
		(left - right).abs() < 0.001
	}

	//==============================================================================
	/// 추적은 1 - exp(-rate * dt) 만큼 중점으로 다가간다.
	//==============================================================================
	#[test]
	fn follow_approaches_center() {
		let mut camera = Camera2D::new();
		let target_positions = [Vector2::create(100.0, 0.0), Vector2::create(300.0, 200.0)];
		camera.follow(&target_positions, 6.0, 0.1);
		let expected_ratio = 1.0 - (-0.6f32).exp();
		let position = camera.get_position();
		assert!(is_near(position.x, 200.0 * expected_ratio));
		assert!(is_near(position.y, 100.0 * expected_ratio));

		// 대상이 없으면 그대로.
		camera.follow(&[], 6.0, 0.1);
		assert_eq!(camera.get_position(), position);
	}

	//==============================================================================
	/// 경계 클램프. (뷰 절반만큼 안쪽, 뷰가 경계보다 크면 가운데)
	//==============================================================================
	#[test]
	fn clamp_keeps_view_inside_bounds() {
		let mut camera = Camera2D::new();
		camera.set_view_size(Vector2::create(200.0, 100.0));
		camera.set_world_bounds(Some(Rect::create(0.0, 0.0, 1000.0, 500.0)));
		camera.set_position(Vector2::create(-50.0, 900.0));
		assert_eq!(camera.get_position(), Vector2::create(100.0, 450.0));

		// 줌아웃해서 뷰(200/0.1 = 2000)가 경계보다 커지면 가운데.
		camera.set_zoom(0.1);
		assert_eq!(camera.get_position(), Vector2::create(500.0, 250.0));
	}

	//==============================================================================
	/// 화면 → 월드 → 화면 왕복과 커서 고정 줌.
	//==============================================================================
	#[test]
	fn zoom_at_keeps_anchor_world_point() {
		let mut camera = Camera2D::new();
		camera.set_view_size(Vector2::create(800.0, 600.0));
		camera.set_position(Vector2::create(50.0, 70.0));
		let anchor_screen = Vector2::create(600.0, 150.0);
		let anchor_world_before = camera.screen_to_world(anchor_screen);
		let screen_again = camera.world_to_screen(anchor_world_before);
		assert!(is_near(screen_again.x, anchor_screen.x) && is_near(screen_again.y, anchor_screen.y));

		camera.zoom_at(anchor_screen, 2.0);
		assert!(is_near(camera.get_zoom(), 2.0));
		let anchor_world_after = camera.screen_to_world(anchor_screen);
		assert!(is_near(anchor_world_after.x, anchor_world_before.x));
		assert!(is_near(anchor_world_after.y, anchor_world_before.y));

		// 범위 끝에서는 움직이지 않는다.
		camera.set_zoom_range(0.5, 2.0);
		let position_before = camera.get_position();
		camera.zoom_at(anchor_screen, 3.0);
		assert_eq!(camera.get_position(), position_before);
	}

	//==============================================================================
	/// 회전: 화면 ↔ 월드 왕복, 그리기 변환과 같은 자리, 팬 방향, 커서 고정 줌, 돌아간 뷰의 경계 클램프.
	//==============================================================================
	#[test]
	fn rotation_converts_coordinates_and_clamps() {
		let mut camera = Camera2D::new();
		camera.set_view_size(Vector2::create(800.0, 600.0));
		camera.set_position(Vector2::create(100.0, 50.0));
		camera.set_rotation(90.0);
		assert_eq!(camera.get_rotation(), 90.0);

		// 90 도면 월드 +x 가 화면 +y 로 간다. (Graphic.rotate 와 같은 방향)
		let screen_position = camera.world_to_screen(Vector2::create(110.0, 50.0));
		assert!(is_near(screen_position.x, 400.0) && is_near(screen_position.y, 310.0));
		let mut transform_matrix = crate::base::transformmatrix::TransformMatrix::identity();
		transform_matrix.translate(400.0, 300.0);
		transform_matrix.scale(1.0, 1.0);
		transform_matrix.rotate(math::degree_to_radian(90.0));
		transform_matrix.translate(-100.0, -50.0);
		let matrix_position = transform_matrix.transform_point(110.0, 50.0);
		assert!(is_near(matrix_position.x, screen_position.x) && is_near(matrix_position.y, screen_position.y));
		let world_again = camera.screen_to_world(screen_position);
		assert!(is_near(world_again.x, 110.0) && is_near(world_again.y, 50.0));

		// 화면을 아래로 끌면 월드에서는 -x 쪽을 보게 된다.
		camera.pan_by_screen(0.0, 10.0);
		assert!(is_near(camera.get_position().x, 90.0) && is_near(camera.get_position().y, 50.0));

		// 돌아가 있어도 커서 아래 월드 지점은 그대로다.
		camera.set_rotation(30.0);
		let anchor_screen = Vector2::create(650.0, 120.0);
		let anchor_world_before = camera.screen_to_world(anchor_screen);
		camera.zoom_at(anchor_screen, 1.5);
		let anchor_world_after = camera.screen_to_world(anchor_screen);
		assert!(is_near(anchor_world_after.x, anchor_world_before.x) && is_near(anchor_world_after.y, anchor_world_before.y));

		// 90 도로 돌리면 보이는 월드 영역은 600x800 이다.
		camera.set_zoom(1.0);
		camera.set_rotation(90.0);
		let visible_half_extent = camera.get_visible_half_extent();
		assert!(is_near(visible_half_extent.x, 300.0) && is_near(visible_half_extent.y, 400.0));
		camera.set_world_bounds(Some(Rect::create(0.0, 0.0, 2000.0, 1000.0)));
		camera.set_position(Vector2::create(0.0, 0.0));
		assert!(is_near(camera.get_position().x, 300.0) && is_near(camera.get_position().y, 400.0));
	}

	//==============================================================================
	/// 팬, 관성, 트윈.
	//==============================================================================
	#[test]
	fn pan_fling_and_tween() {
		let mut camera = Camera2D::new();
		camera.set_zoom(2.0);
		camera.pan_by_screen(20.0, -10.0);
		assert_eq!(camera.get_position(), Vector2::create(-10.0, 5.0));

		camera.fling(100.0, 0.0);
		camera.tick(0.1);
		assert!(is_near(camera.get_position().x, -10.0 - 50.0 * 0.1));
		for _ in 0..200 {
			camera.tick(0.05);
		}
		let resting_position = camera.get_position();
		camera.tick(0.05);
		assert_eq!(camera.get_position(), resting_position);

		camera.tween_to(Vector2::create(300.0, 400.0), 4.0, 0.5);
		assert!(camera.is_tweening());
		camera.tick(0.25);
		assert!(camera.is_tweening());
		camera.tick(0.25);
		assert!(!camera.is_tweening());
		assert!(is_near(camera.get_position().x, 300.0));
		assert!(is_near(camera.get_position().y, 400.0));
		assert!(is_near(camera.get_zoom(), 4.0));
	}
}
