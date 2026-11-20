//==============================================================================
// 카메라 손짓. (vanilla.js misc/cameragestures.js 의 CameraGestures)
// - 3D 판이나 지도를 돌리고 밀고 키우는 손짓이다. 엔진의 터치(왼쪽 단추, 첫 손가락) 밖의 입력을 쓴다.
//   - 마우스 오른쪽 단추를 누른 채 끌기: 가로로 끈 만큼 돌리고(캔버스 너비만큼이 한 바퀴), 세로로 끈 만큼 기울인다. 끌지 않고 떼면 오른쪽 클릭이다.
//   - 마우스 가운데 단추를 누른 채 끌기: 민다.
//   - 두 손가락: 같은 쪽으로 끌면 밀고, 오므리고 벌리면 두 손가락 가운데를 두고 키우고 줄이고, 비틀기 문턱을 넘게 비틀면 돌린다. (오므려 키우다가 조금씩 도는 것을 막는다)
// - 원본은 캔버스의 DOM 사건을 직접 듣는다. 러스트 판은 매 프레임 update 로 입력 매니저(마우스 단추, 손가락 목록)를 읽는다.
// - 손짓 중에는 엔진이 넘기는 누름과 뗌을 놀이에 쓰지 않도록 is_gesture_active 로 알린다.
// - 모인 양(밀기, 돌리기, 기울이기, 키우기)은 매 프레임 take 로 꺼내 간다. 밀기는 뷰 단위, 돌리기와 기울이기는 라디안, 키우기는 곱할 비다.
//==============================================================================
use std::f32::consts::PI;

use crate::base::vector2::Vector2;
use crate::core::inputmanager::{InputManager, MouseButton};
use crate::core::viewmanager::ViewManager;

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

		// 두 손가락.
		if touch_points.len() >= 2 {
			let first_position = touch_points[0].canvas_native_position;
			let second_position = touch_points[1].canvas_native_position;
			let center = Vector2::create((first_position.x + second_position.x) * 0.5, (first_position.y + second_position.y) * 0.5);
			let angle = (second_position.y - first_position.y).atan2(second_position.x - first_position.x);
			let distance = first_position.distance(second_position).max(1.0);
			if !self.is_multi_touch {
				self.is_multi_touch = true;
				self.twist_sum = 0.0;
				self.is_twisting = false;
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
		else if touch_points.is_empty() && self.is_multi_touch {
			self.is_multi_touch = false;
			self.is_rotation_released = true;
		}
		else if touch_points.len() == 1 && self.is_multi_touch {
			// 한 손가락을 먼저 떼면 남은 손가락으로 기준을 다시 잡는다. (다시 둘이 되면 처음처럼)
			self.is_multi_touch = false;
			self.is_rotation_released = true;
		}

		// 마우스 오른쪽 단추. (가로는 돌리기, 세로는 기울이기, 끌지 않고 떼면 오른쪽 클릭)
		let is_right_down = input_manager.is_mouse_button_down(MouseButton::Right);
		let mouse_delta = Vector2::create(mouse_position.x - self.last_mouse_position.x, mouse_position.y - self.last_mouse_position.y);
		if is_right_down && !self.is_right_button_down {
			self.is_right_button_down = true;
			self.is_right_dragged = false;
			self.right_press_position = mouse_position;
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

		// 마우스 가운데 단추. (민다)
		let is_middle_down = input_manager.is_mouse_button_down(MouseButton::Middle);
		if is_middle_down && self.is_middle_button_down {
			self.pan.x += mouse_delta.x * units_per_canvas_pixel;
			self.pan.y += mouse_delta.y * units_per_canvas_pixel;
		}
		self.is_middle_button_down = is_middle_down;
		self.last_mouse_position = mouse_position;
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
	// 문턱 설정. (비틀기를 시작하는 각도, 캔버스 높이의 반만큼 끌 때 기울어지는 각도, 오른쪽 클릭으로 보는 거리)
	//==============================================================================
	pub fn set_thresholds(&mut self, twist_start_radians: f32, tilt_per_half_height_radians: f32, right_click_distance: f32) {
		self.twist_start_radians = twist_start_radians;
		self.tilt_per_half_height_radians = tilt_per_half_height_radians;
		self.right_click_distance = right_click_distance;
	}
}

impl Default for CameraGestures {
	fn default() -> CameraGestures {
		CameraGestures::new()
	}
}
