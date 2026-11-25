//==============================================================================
// 카메라 손짓. (vanilla.js misc/cameragestures.js 의 CameraGestures)
// - 3D 판이나 지도를 돌리고 밀고 키우는 손짓이다. 엔진의 터치(왼쪽 단추, 첫 손가락) 밖의 입력을 쓴다.
//   - 마우스 오른쪽 단추를 누른 채 끌기: 가로로 끈 만큼 돌리고(캔버스 너비만큼이 한 바퀴), 세로로 끈 만큼 기울인다. 끌지 않고 떼면 오른쪽 클릭이다.
//   - 마우스 가운데 단추를 누른 채 끌기: 민다.
//   - 두 손가락: 같은 쪽으로 끌면 밀고, 오므리고 벌리면 두 손가락 가운데를 두고 키우고 줄이고, 비틀기 문턱을 넘게 비틀면 돌린다. (오므려 키우다가 조금씩 도는 것을 막는다)
//     원본처럼 모든 손가락을 뗄 때까지 손짓이 이어진다. 한 손가락만 남으면 움직임을 쓰지 않고, 손가락 수가 바뀌면 그 자리를 새 기준으로 잡는다.
// - 휠은 엔진이 넘긴다. 여기서는 Ctrl 을 눌렀는지만 적어 둔다. (is_last_wheel_ctrl, 노트북 터치패드의 오므리기도 Ctrl 휠로 온다)
// - 마지막 손이 손가락인지(is_touch_input, 처음에는 기기가 손가락이 주된 입력인지)도 읽는다. 손가락을 뗀 뒤 흉내 낸 마우스를 무시하는 시간(set_thresholds 의 넷째 인자, 처음 800 밀리초) 안의
//   마우스 움직임과 누름은 손을 바꾸지 않고, 그때 누른 오른쪽, 가운데 단추는 뗄 때까지 손짓을 시작하지 않는다.
// - 원본은 캔버스의 DOM 사건을 직접 듣는다(그래서 addListener, dispose 가 있다). 러스트 판은 매 프레임 update 로 입력 매니저(마우스 단추, 손가락 목록, 휠의 Ctrl)를 읽는다.
//   웹 플랫폼은 흉내 낸 마우스 사건을 엔진 쪽에서 먼저 거른다. (800 밀리초)
// - 손짓 중에는 엔진이 넘기는 누름과 뗌을 놀이에 쓰지 않도록 is_gesture_active 로 알린다.
// - 모인 양(밀기, 돌리기, 기울이기, 키우기)은 매 프레임 take 로 꺼내 간다. 밀기는 뷰 단위, 돌리기와 기울이기는 라디안, 키우기는 곱할 비다.
//==============================================================================
use std::f32::consts::PI;
use web_time::Instant;

use crate::base::vector2::Vector2;
use crate::core::inputmanager::{InputManager, MouseButton};
use crate::core::viewmanager::ViewManager;
use crate::platform;

pub struct CameraGestures {
	twist_start_radians: f32,
	tilt_per_half_height_radians: f32,
	right_click_distance: f32,
	is_right_button_down: bool,
	is_middle_button_down: bool,
	last_mouse_position: Vector2,
	right_press_position: Vector2,
	is_right_dragged: bool,
	is_right_clicked: bool,
	is_multi_touch: bool,
	last_touch_center: Vector2,
	last_touch_angle: f32,
	last_touch_distance: f32,
	twist_sum: f32,
	is_twisting: bool,
	pan: Vector2,
	rotation: f32,
	tilt: f32,
	zoom: f32,
	zoom_anchor: Option<Vector2>,
	is_rotation_released: bool,
	// 지난 프레임의 손가락 수. (바뀌면 두 손가락의 기준을 다시 잡는다, 원본 touchstart / touchend)
	last_touch_point_count: usize,
	// 마지막 휠이 Ctrl 을 누른 휠이었는지.
	is_last_wheel_ctrl: bool,
	// 마지막 손이 손가락인지.
	is_touch_input: bool,
	// 마지막 손가락 사건의 때. (없으면 흉내 낸 마우스 시간 밖)
	last_touch_instant: Option<Instant>,
	// 손가락을 뗀 뒤 흉내 낸 마우스 사건을 무시하는 시간. (밀리초)
	compatibility_mouse_milliseconds: f32,
	// 흉내 낸 마우스 시간 안에 눌러 뗄 때까지 무시하는 오른쪽, 가운데 단추인지.
	is_right_press_ignored: bool,
	is_middle_press_ignored: bool,
}

impl CameraGestures {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> CameraGestures {
		CameraGestures {
			twist_start_radians: 15.0 * PI / 180.0,
			tilt_per_half_height_radians: 35.0 * PI / 180.0,
			right_click_distance: 8.0,
			is_right_button_down: false,
			is_middle_button_down: false,
			last_mouse_position: Vector2::zero(),
			right_press_position: Vector2::zero(),
			is_right_dragged: false,
			is_right_clicked: false,
			is_multi_touch: false,
			last_touch_center: Vector2::zero(),
			last_touch_angle: 0.0,
			last_touch_distance: 1.0,
			twist_sum: 0.0,
			is_twisting: false,
			pan: Vector2::zero(),
			rotation: 0.0,
			tilt: 0.0,
			zoom: 1.0,
			zoom_anchor: None,
			is_rotation_released: false,
			last_touch_point_count: 0,
			is_last_wheel_ctrl: false,
			is_touch_input: platform::is_coarse_pointer(),
			last_touch_instant: None,
			compatibility_mouse_milliseconds: 800.0,
			is_right_press_ignored: false,
			is_middle_press_ignored: false,
		}
	}

	//==============================================================================
	// 매 프레임 입력 읽기. (씬의 tick 에서 부른다)
	//==============================================================================
	pub fn update(&mut self, input_manager: &InputManager, view_manager: &ViewManager) {
		let mouse_position = input_manager.get_canvas_native_input_position();
		let canvas_size = view_manager.get_canvas_native_size();
		let canvas_width = canvas_size.x.max(1.0);
		let canvas_height = canvas_size.y.max(1.0);
		let units_per_canvas_pixel = CameraGestures::read_units_per_canvas_pixel(view_manager);
		let touch_points = input_manager.get_touch_points();
		let touch_point_count = touch_points.len();
		let is_touch_point_count_changed = touch_point_count != self.last_touch_point_count;
		self.last_touch_point_count = touch_point_count;
		self.is_last_wheel_ctrl = input_manager.is_wheel_with_ctrl();

		// 손가락 사건. (닿아 있거나 막 뗀 손가락이 있으면 마지막 손은 손가락이고, 흉내 낸 마우스 시간을 다시 센다)
		if touch_point_count > 0 || !input_manager.get_ended_touch_points().is_empty() {
			self.is_touch_input = true;
			self.last_touch_instant = Some(Instant::now());
		}
		let is_compatibility_mouse = self.is_compatibility_mouse();

		// 두 손가락. (처음 둘이 되거나 손가락 수가 바뀐 프레임은 기준만 잡는다)
		if touch_point_count >= 2 {
			let first_position = touch_points[0].canvas_native_position;
			let second_position = touch_points[1].canvas_native_position;
			let center = Vector2::create((first_position.x + second_position.x) * 0.5, (first_position.y + second_position.y) * 0.5);
			let angle = (second_position.y - first_position.y).atan2(second_position.x - first_position.x);
			let distance = first_position.distance(second_position).max(1.0);
			if !self.is_multi_touch || is_touch_point_count_changed {
				if !self.is_multi_touch {
					self.twist_sum = 0.0;
					self.is_twisting = false;
				}
				self.is_multi_touch = true;
			}
			else {
				self.pan.x += (center.x - self.last_touch_center.x) * units_per_canvas_pixel;
				self.pan.y += (center.y - self.last_touch_center.y) * units_per_canvas_pixel;
				self.zoom *= distance / self.last_touch_distance;
				self.zoom_anchor = Some(view_manager.canvas_position_to_view_position(center));
				let mut angle_delta = angle - self.last_touch_angle;
				while angle_delta > PI {
					angle_delta -= PI * 2.0;
				}
				while angle_delta < -PI {
					angle_delta += PI * 2.0;
				}
				if self.is_twisting {
					self.rotation += angle_delta;
				}
				else {
					self.twist_sum += angle_delta;
					if self.twist_sum.abs() > self.twist_start_radians {
						self.is_twisting = true;
					}
				}
			}
			self.last_touch_center = center;
			self.last_touch_angle = angle;
			self.last_touch_distance = distance;
		}
		else if touch_point_count == 0 && self.is_multi_touch {
			self.is_multi_touch = false;
			self.is_rotation_released = true;
		}

		// 마우스를 움직이거나 누르면 마지막 손은 마우스다. (흉내 낸 마우스 시간 밖에서만)
		let mouse_delta = Vector2::create(mouse_position.x - self.last_mouse_position.x, mouse_position.y - self.last_mouse_position.y);
		let is_mouse_pressed = [MouseButton::Left, MouseButton::Middle, MouseButton::Right].iter().any(|mouse_button| input_manager.is_mouse_button_just_pressed(*mouse_button));
		if touch_point_count == 0 && !is_compatibility_mouse && (mouse_delta.x != 0.0 || mouse_delta.y != 0.0 || is_mouse_pressed) {
			self.is_touch_input = false;
		}

		// 마우스 오른쪽 단추. (가로는 돌리기, 세로는 기울이기, 끌지 않고 떼면 오른쪽 클릭, 흉내 낸 마우스 시간 안에 누른 것은 뗄 때까지 무시)
		let is_right_down = input_manager.is_mouse_button_down(MouseButton::Right);
		if !is_right_down {
			self.is_right_press_ignored = false;
		}
		if is_right_down && !self.is_right_button_down {
			if is_compatibility_mouse {
				self.is_right_press_ignored = true;
			}
			if !self.is_right_press_ignored {
				self.is_right_button_down = true;
				self.is_right_dragged = false;
				self.right_press_position = mouse_position;
			}
		}
		else if is_right_down {
			if mouse_position.distance(self.right_press_position) > self.right_click_distance {
				self.is_right_dragged = true;
			}
			self.rotation += mouse_delta.x / canvas_width * PI * 2.0;
			self.tilt -= mouse_delta.y / (canvas_height * 0.5) * self.tilt_per_half_height_radians;
		}
		else if self.is_right_button_down {
			self.is_right_button_down = false;
			self.is_rotation_released = true;
			if !self.is_right_dragged {
				self.is_right_clicked = true;
			}
		}

		// 마우스 가운데 단추. (민다, 흉내 낸 마우스 시간 안에 누른 것은 뗄 때까지 무시)
		let is_middle_down = input_manager.is_mouse_button_down(MouseButton::Middle);
		if !is_middle_down {
			self.is_middle_press_ignored = false;
		}
		else if !self.is_middle_button_down && is_compatibility_mouse {
			self.is_middle_press_ignored = true;
		}
		let is_middle_active = is_middle_down && !self.is_middle_press_ignored;
		if is_middle_active && self.is_middle_button_down {
			self.pan.x += mouse_delta.x * units_per_canvas_pixel;
			self.pan.y += mouse_delta.y * units_per_canvas_pixel;
		}
		self.is_middle_button_down = is_middle_active;
		self.last_mouse_position = mouse_position;
	}

	//==============================================================================
	// 손가락을 뗀 바로 뒤의 마우스 사건인지. (브라우저가 흉내 내 보내는 것, 원본 isCompatibilityMouse)
	//==============================================================================
	fn is_compatibility_mouse(&self) -> bool {
		match self.last_touch_instant {
			Some(last_touch_instant) => {
				(last_touch_instant.elapsed().as_secs_f64() * 1000.0) < self.compatibility_mouse_milliseconds as f64
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 마지막 손이 손가락인지. (처음에는 기기가 손가락이 주된 입력인지로)
	//==============================================================================
	pub fn is_touch_input(&self) -> bool {
		self.is_touch_input
	}

	//==============================================================================
	// 기기가 손가락이 주된 입력인지. (폰과 태블릿, 원본 matchMedia("(pointer: coarse)"))
	//==============================================================================
	pub fn is_coarse_pointer(&self) -> bool {
		platform::is_coarse_pointer()
	}

	//==============================================================================
	// 마지막 휠이 Ctrl 을 누른 휠이었는지. (휠로 밀기를 켰어도 그 휠은 키우기다)
	//==============================================================================
	pub fn is_last_wheel_ctrl(&self) -> bool {
		self.is_last_wheel_ctrl
	}

	//==============================================================================
	// 캔버스 논리 픽셀 하나가 뷰 단위로 얼마인지.
	//==============================================================================
	pub fn read_units_per_canvas_pixel(view_manager: &ViewManager) -> f32 {
		let target_resolution_scale = view_manager.get_target_resolution_scale();
		if target_resolution_scale <= 0.0 {
			return 1.0;
		}
		1.0 / target_resolution_scale
	}

	//==============================================================================
	// 손짓 중인지. (오른쪽이나 가운데 단추를 누르고 있거나 두 손가락, 이때 엔진의 누름은 놀이에 쓰지 않는다)
	//==============================================================================
	pub fn is_gesture_active(&self) -> bool {
		self.is_right_button_down || self.is_middle_button_down || self.is_multi_touch
	}

	//==============================================================================
	// 마우스로 끄는 동안의 커서 이름. (돌리거나 미는 중이면 grabbing, 오른쪽 단추를 누르고 아직 끌지 않았으면 grab, 아니면 빈 글)
	//==============================================================================
	pub fn read_mouse_drag_cursor(&self) -> &'static str {
		if self.is_middle_button_down || (self.is_right_button_down && self.is_right_dragged) {
			return "grabbing";
		}
		if self.is_right_button_down {
			return "grab";
		}
		""
	}

	//==============================================================================
	// 모인 밀기 꺼내기. (뷰 단위, 꺼내면 비운다)
	//==============================================================================
	pub fn take_pan(&mut self) -> Vector2 {
		let pan = self.pan;
		self.pan = Vector2::zero();
		pan
	}

	//==============================================================================
	// 모인 돌리기 꺼내기. (라디안, 꺼내면 비운다)
	//==============================================================================
	pub fn take_rotation(&mut self) -> f32 {
		let rotation = self.rotation;
		self.rotation = 0.0;
		rotation
	}

	//==============================================================================
	// 모인 기울이기 꺼내기. (라디안, + 가 위에서 보는 쪽, 꺼내면 비운다)
	//==============================================================================
	pub fn take_tilt(&mut self) -> f32 {
		let tilt = self.tilt;
		self.tilt = 0.0;
		tilt
	}

	//==============================================================================
	// 모인 키우기 꺼내기. (곱할 비, 꺼내면 1)
	//==============================================================================
	pub fn take_zoom(&mut self) -> f32 {
		let zoom = self.zoom;
		self.zoom = 1.0;
		zoom
	}

	//==============================================================================
	// 키우기의 가운데. (뷰 좌표, 두 손가락 가운데, 없으면 None)
	//==============================================================================
	pub fn get_zoom_anchor(&self) -> Option<Vector2> {
		self.zoom_anchor
	}

	//==============================================================================
	// 오른쪽 클릭이 있었는지 꺼내기. (끌지 않고 뗀 오른쪽 단추, 꺼내면 지운다)
	//==============================================================================
	pub fn take_right_click(&mut self) -> bool {
		let is_right_clicked = self.is_right_clicked;
		self.is_right_clicked = false;
		is_right_clicked
	}

	//==============================================================================
	// 돌리기를 놓았는지 꺼내기. (꺼내면 지운다)
	//==============================================================================
	pub fn take_rotation_released(&mut self) -> bool {
		let is_rotation_released = self.is_rotation_released;
		self.is_rotation_released = false;
		is_rotation_released
	}

	//==============================================================================
	// 문턱 설정. (비틀기를 시작하는 각도, 캔버스 높이의 반만큼 끌 때 기울어지는 각도, 오른쪽 클릭으로 보는 거리, 흉내 낸 마우스를 무시하는 밀리초)
	//==============================================================================
	pub fn set_thresholds(&mut self, twist_start_radians: f32, tilt_per_half_height_radians: f32, right_click_distance: f32, compatibility_mouse_milliseconds: f32) {
		self.twist_start_radians = twist_start_radians;
		self.tilt_per_half_height_radians = tilt_per_half_height_radians;
		self.right_click_distance = right_click_distance;
		self.compatibility_mouse_milliseconds = compatibility_mouse_milliseconds;
	}
}

impl Default for CameraGestures {
	fn default() -> CameraGestures {
		CameraGestures::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::viewmanager::ViewScaleMode;

	//==============================================================================
	// 두 손가락에서 하나가 되어도 원본처럼 손짓이 이어지고(움직임은 쓰지 않음), 다시 둘이 되면 그 자리를 기준으로 잇는다. 모두 떼면 끝난다.
	// 흉내 낸 마우스 시간 안의 오른쪽 단추는 뗄 때까지 무시하고, 마지막 손과 휠의 Ctrl 을 읽는다.
	//==============================================================================
	#[test]
	fn multi_touch_lasts_until_all_fingers_lift() {
		let view_manager = ViewManager::new(Vector2::create(800.0, 600.0), ViewScaleMode::ReferenceResolution);
		let mut input_manager = InputManager::new();
		let mut camera_gestures = CameraGestures::new();
		assert_eq!(camera_gestures.is_touch_input(), camera_gestures.is_coarse_pointer());
		input_manager.begin_touch_point(1, Vector2::create(100.0, 100.0), Vector2::create(100.0, 100.0));
		input_manager.begin_touch_point(2, Vector2::create(200.0, 100.0), Vector2::create(200.0, 100.0));
		camera_gestures.update(&input_manager, &view_manager);
		assert!(camera_gestures.is_gesture_active());
		assert!(camera_gestures.is_touch_input());
		input_manager.update_frame_snapshot();

		// 한 손가락을 떼고 남은 손가락을 움직여도 손짓은 이어지고 밀지 않는다.
		input_manager.end_touch_point(2, Vector2::create(200.0, 100.0), Vector2::create(200.0, 100.0), false);
		input_manager.move_touch_point(1, Vector2::create(150.0, 150.0), Vector2::create(150.0, 150.0));
		camera_gestures.update(&input_manager, &view_manager);
		assert!(camera_gestures.is_gesture_active());
		assert!(!camera_gestures.take_rotation_released());
		assert_eq!(camera_gestures.take_pan(), Vector2::zero());
		input_manager.update_frame_snapshot();

		// 다시 둘이 되면 그 프레임은 기준만 잡고, 다음 움직임부터 민다.
		input_manager.begin_touch_point(3, Vector2::create(350.0, 150.0), Vector2::create(350.0, 150.0));
		camera_gestures.update(&input_manager, &view_manager);
		assert_eq!(camera_gestures.take_pan(), Vector2::zero());
		input_manager.update_frame_snapshot();
		input_manager.move_touch_point(1, Vector2::create(160.0, 150.0), Vector2::create(160.0, 150.0));
		input_manager.move_touch_point(3, Vector2::create(360.0, 150.0), Vector2::create(360.0, 150.0));
		camera_gestures.update(&input_manager, &view_manager);
		let pan = camera_gestures.take_pan();
		assert!((pan.x - 10.0 * CameraGestures::read_units_per_canvas_pixel(&view_manager)).abs() < 1e-4);
		assert!((camera_gestures.take_zoom() - 1.0).abs() < 1e-6);
		input_manager.update_frame_snapshot();

		// 모두 떼면 끝난다.
		input_manager.end_touch_point(1, Vector2::create(160.0, 150.0), Vector2::create(160.0, 150.0), false);
		input_manager.end_touch_point(3, Vector2::create(360.0, 150.0), Vector2::create(360.0, 150.0), false);
		camera_gestures.update(&input_manager, &view_manager);
		assert!(!camera_gestures.is_gesture_active());
		assert!(camera_gestures.take_rotation_released());
		input_manager.update_frame_snapshot();

		// 손가락을 뗀 바로 뒤 누른 오른쪽 단추는 뗄 때까지 무시한다. (흉내 낸 마우스 시간 안)
		input_manager.set_mouse_button_down(MouseButton::Right, true);
		camera_gestures.update(&input_manager, &view_manager);
		assert!(!camera_gestures.is_gesture_active());
		assert!(camera_gestures.is_touch_input());
		input_manager.update_frame_snapshot();
		input_manager.set_mouse_button_down(MouseButton::Right, false);
		camera_gestures.update(&input_manager, &view_manager);
		assert!(!camera_gestures.take_right_click());
		input_manager.update_frame_snapshot();

		// 문턱을 0 밀리초로 하면 바로 마우스로 친다.
		camera_gestures.set_thresholds(15.0 * PI / 180.0, 35.0 * PI / 180.0, 8.0, 0.0);
		input_manager.set_mouse_button_down(MouseButton::Right, true);
		input_manager.set_wheel_with_ctrl(true);
		camera_gestures.update(&input_manager, &view_manager);
		assert!(camera_gestures.is_gesture_active());
		assert!(!camera_gestures.is_touch_input());
		assert!(camera_gestures.is_last_wheel_ctrl());
		input_manager.update_frame_snapshot();
		input_manager.set_mouse_button_down(MouseButton::Right, false);
		camera_gestures.update(&input_manager, &view_manager);
		assert!(camera_gestures.take_right_click());
	}
}
