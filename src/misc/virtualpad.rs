//==============================================================================
//! 가상 패드.
//! - 원본 misc/virtualpad.js 를 옮긴 것이다. 화면 조이스틱의 패드(바깥 원)와 구슬(안쪽 원) 움직임을 계산하고 그린다.
//! - 원본처럼 컴포넌트가 아니라 그냥 값이다. 들고 있는 쪽이 touch_press / touch_move / touch_release 와 tick, draw 를 불러 준다.
//! - 구슬을 움직이는 동안 구슬 이동 이벤트로 방향(0 ~ 1, 0 ~ 1 로 바꾼 단위 벡터)과 당긴 세기(0 ~ 1)를 알린다.
//!   떼면 구슬이 초당 1600 픽셀로 패드 가운데로 돌아가며 알리고, 다 돌아오면 (0.5, 0.5), 0 을 한 번 알린다.
//! - 원본과 다른 점:
//!   상태 이름 move 는 러스트 예약어라 moved 다. 원본 상태 값의 글자("wait", "pressed", "move", "released")는 get_name 으로 얻는다.
//!   이벤트 콜백은 FnMut(Vector2, f32) 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

//==============================================================================
/// 가상 패드 상태.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualPadState {
	None,
	Pressed,
	Moved,
	Released,
}

impl VirtualPadState {
	//==============================================================================
	/// 원본 상태 값의 글자.
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		match self {
			VirtualPadState::None => {
				"wait"
			}
			VirtualPadState::Pressed => {
				"pressed"
			}
			VirtualPadState::Moved => {
				"move"
			}
			VirtualPadState::Released => {
				"released"
			}
		}
	}
}

/// 구슬 이동 이벤트. (방향 0 ~ 1, 당긴 세기 0 ~ 1)
type BallMoveEvent = Box<dyn FnMut(Vector2, f32)>;

//==============================================================================
/// 가상 패드.
//==============================================================================
pub struct VirtualPad {
	virtual_pad_state: VirtualPadState,
	pad_position: Vector2,
	pad_radius: f32,
	pad_color: Color,
	ball_position: Vector2,
	ball_radius: f32,
	ball_color: Color,
	ball_return_speed: f32,
	ball_move_event: Option<BallMoveEvent>,
}

impl VirtualPad {
	//==============================================================================
	/// 생성. (패드 반지름 200 검은색, 구슬 반지름 50 흰색, 돌아가는 속도 초당 1600)
	//==============================================================================
	pub fn new() -> VirtualPad {
		VirtualPad {
			virtual_pad_state: VirtualPadState::None,
			pad_position: Vector2::zero(),
			pad_radius: 200.0,
			pad_color: Color::create(0.0, 0.0, 0.0, 1.0),
			ball_position: Vector2::zero(),
			ball_radius: 50.0,
			ball_color: Color::create(1.0, 1.0, 1.0, 1.0),
			ball_return_speed: 1600.0,
			ball_move_event: None,
		}
	}

	//==============================================================================
	/// 구슬 이동 이벤트 부르기. (방향을 0 ~ 1 범위로 바꿔 넘긴다)
	//==============================================================================
	fn fire_ball_move_event(&mut self, direction: Vector2, pull_strength: f32) {
		if let Some(ball_move_event) = self.ball_move_event.as_mut() {
			let normalized_direction_x = (direction.x + 1.0) / 2.0;
			let normalized_direction_y = (direction.y + 1.0) / 2.0;
			let normalized_direction = Vector2::create(normalized_direction_x, normalized_direction_y);
			ball_move_event(normalized_direction, pull_strength);
		}
	}

	//==============================================================================
	/// 갱신. (누르고 끄는 동안 구슬을 패드 안에 가두고, 떼면 가운데로 돌려보낸다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		let virtual_pad_state = self.get_virtual_pad_state();
		if virtual_pad_state == VirtualPadState::None {
			return;
		}

		let pad_position = self.get_pad_position();
		let pad_radius = self.get_pad_radius();
		let ball_position = self.get_ball_position();
		let move_delta = ball_position.subtract(pad_position);
		let distance = move_delta.length();
		let direction = move_delta.normalize();

		// 터치 누르고 끌기: 구슬 이동.
		if virtual_pad_state == VirtualPadState::Pressed || virtual_pad_state == VirtualPadState::Moved {
			// 구슬 중심이 패드를 넘어가지 않도록 가두기.
			if distance > pad_radius {
				self.ball_position = pad_position.add(direction.multiply_scalar(pad_radius));
			}
			let clamped_distance = distance.min(pad_radius);
			let pull_strength = clamped_distance / pad_radius;
			self.fire_ball_move_event(direction, pull_strength);
		}
		// 터치 뗌: 패드 가운데로 빠르게 돌아가기.
		else if virtual_pad_state == VirtualPadState::Released {
			let return_speed_delta = self.ball_return_speed * time_delta;
			if distance > return_speed_delta {
				let return_move_delta = direction.multiply_scalar(return_speed_delta);
				let current_ball_position = self.get_ball_position();
				self.ball_position = current_ball_position.subtract(return_move_delta);

				let updated_ball_position = self.get_ball_position();
				let returned_move_delta = updated_ball_position.subtract(pad_position);
				let returned_distance = returned_move_delta.length();
				let pull_strength = math::clamp01(returned_distance / pad_radius);
				self.fire_ball_move_event(direction, pull_strength);
			}
			else {
				self.ball_position = pad_position;
				self.virtual_pad_state = VirtualPadState::None;
				if let Some(ball_move_event) = self.ball_move_event.as_mut() {
					let neutral_direction = Vector2::create(0.5, 0.5);
					ball_move_event(neutral_direction, 0.0);
				}
			}
		}
	}

	//==============================================================================
	/// 출력. (바깥 원 다음 안쪽 원, 부른 쪽의 좌표계 그대로)
	//==============================================================================
	pub fn draw(&self, graphic: &mut Graphic) {
		let pad_position = self.get_pad_position();
		let pad_radius = self.get_pad_radius();
		let pad_color = self.get_pad_color();
		graphic.set_fill_color(pad_color);
		graphic.draw_circle(pad_position, pad_radius);

		let ball_position = self.get_ball_position();
		let ball_radius = self.get_ball_radius();
		let ball_color = self.get_ball_color();
		graphic.set_fill_color(ball_color);
		graphic.draw_circle(ball_position, ball_radius);
	}

	//==============================================================================
	/// 터치 누름. (구슬을 누른 곳으로)
	//==============================================================================
	pub fn touch_press(&mut self, view_input_position: Vector2) {
		self.virtual_pad_state = VirtualPadState::Pressed;
		self.ball_position = view_input_position;
	}

	//==============================================================================
	/// 터치 끌기. (구슬을 끈 곳으로)
	//==============================================================================
	pub fn touch_move(&mut self, view_input_position: Vector2) {
		self.virtual_pad_state = VirtualPadState::Moved;
		self.ball_position = view_input_position;
	}

	//==============================================================================
	/// 터치 뗌. (구슬은 그 자리에서 가운데로 돌아가기 시작한다)
	//==============================================================================
	pub fn touch_release(&mut self, _view_input_position: Vector2) {
		self.virtual_pad_state = VirtualPadState::Released;
	}

	//==============================================================================
	/// 터치 취소. (뗌과 같다)
	//==============================================================================
	pub fn touch_cancel(&mut self) {
		self.virtual_pad_state = VirtualPadState::Released;
	}

	//==============================================================================
	/// 가상 패드 상태 반환.
	//==============================================================================
	pub fn get_virtual_pad_state(&self) -> VirtualPadState {
		self.virtual_pad_state
	}

	//==============================================================================
	/// 구슬 이동 이벤트 설정. (방향 0 ~ 1, 당긴 세기 0 ~ 1)
	//==============================================================================
	pub fn set_ball_move_event(&mut self, callback: impl FnMut(Vector2, f32) + 'static) {
		self.ball_move_event = Some(Box::new(callback));
	}

	//==============================================================================
	/// 패드 위치 설정. (구슬도 그 자리로)
	//==============================================================================
	pub fn set_pad_position(&mut self, pad_position: Vector2) {
		self.pad_position = pad_position;
		self.ball_position = pad_position;
	}

	//==============================================================================
	/// 패드 위치 반환.
	//==============================================================================
	pub fn get_pad_position(&self) -> Vector2 {
		self.pad_position
	}

	//==============================================================================
	/// 패드 반지름 설정.
	//==============================================================================
	pub fn set_pad_radius(&mut self, pad_radius: f32) {
		self.pad_radius = pad_radius;
	}

	//==============================================================================
	/// 패드 반지름 반환.
	//==============================================================================
	pub fn get_pad_radius(&self) -> f32 {
		self.pad_radius
	}

	//==============================================================================
	/// 구슬 위치 반환.
	//==============================================================================
	pub fn get_ball_position(&self) -> Vector2 {
		self.ball_position
	}

	//==============================================================================
	/// 구슬 반지름 설정.
	//==============================================================================
	pub fn set_ball_radius(&mut self, ball_radius: f32) {
		self.ball_radius = ball_radius;
	}

	//==============================================================================
	/// 구슬 반지름 반환.
	//==============================================================================
	pub fn get_ball_radius(&self) -> f32 {
		self.ball_radius
	}

	//==============================================================================
	/// 패드 색 설정.
	//==============================================================================
	pub fn set_pad_color(&mut self, pad_color: impl Into<Color>) {
		self.pad_color = pad_color.into();
	}

	//==============================================================================
	/// 패드 색 반환.
	//==============================================================================
	pub fn get_pad_color(&self) -> Color {
		self.pad_color
	}

	//==============================================================================
	/// 구슬 색 설정.
	//==============================================================================
	pub fn set_ball_color(&mut self, ball_color: impl Into<Color>) {
		self.ball_color = ball_color.into();
	}

	//==============================================================================
	/// 구슬 색 반환.
	//==============================================================================
	pub fn get_ball_color(&self) -> Color {
		self.ball_color
	}
}

impl Default for VirtualPad {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> VirtualPad {
		VirtualPad::new()
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	//==============================================================================
	/// 끌면 구슬이 패드 안에 갇히고, 떼면 가운데로 돌아와 중립을 알린다.
	//==============================================================================
	#[test]
	fn ball_is_clamped_and_returns() {
		let event_list: Rc<RefCell<Vec<(Vector2, f32)>>> = Rc::new(RefCell::new(Vec::new()));
		let event_list_for_callback = event_list.clone();
		let mut virtual_pad = VirtualPad::new();
		virtual_pad.set_pad_radius(100.0);
		virtual_pad.set_pad_position(Vector2::create(200.0, 200.0));
		virtual_pad.set_ball_move_event(move |direction, pull_strength| {
			event_list_for_callback.borrow_mut().push((direction, pull_strength));
		});
		virtual_pad.tick(0.016);
		assert!(event_list.borrow().is_empty());

		virtual_pad.touch_press(Vector2::create(200.0, 200.0));
		virtual_pad.touch_move(Vector2::create(500.0, 200.0));
		assert_eq!(virtual_pad.get_virtual_pad_state().get_name(), "move");
		virtual_pad.tick(0.016);
		assert_eq!(virtual_pad.get_ball_position(), Vector2::create(300.0, 200.0));
		assert_eq!(event_list.borrow()[0], (Vector2::create(1.0, 0.5), 1.0));

		virtual_pad.touch_release(Vector2::create(500.0, 200.0));
		virtual_pad.tick(0.025);
		assert_eq!(virtual_pad.get_ball_position(), Vector2::create(260.0, 200.0));
		let (_, returning_strength) = event_list.borrow()[1];
		assert!((returning_strength - 0.6).abs() < 0.0001);
		virtual_pad.tick(0.1);
		assert_eq!(virtual_pad.get_ball_position(), Vector2::create(200.0, 200.0));
		assert_eq!(virtual_pad.get_virtual_pad_state(), VirtualPadState::None);
		assert_eq!(event_list.borrow()[2], (Vector2::create(0.5, 0.5), 0.0));
	}
}
