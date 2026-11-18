//==============================================================================
// 가상 패드.
// - 원본 misc/virtualpad.js 를 옮긴 것이다. 화면 조이스틱의 패드(바깥 원)와 구슬(안쪽 원) 움직임을 계산하고 그린다.
// - 원본처럼 컴포넌트가 아니라 그냥 값이다. 들고 있는 쪽이 touchPress / touchMove / touchRelease 와 tick, draw 를 불러 준다.
// - 구슬을 움직이는 동안 구슬 이동 이벤트로 방향(0 ~ 1, 0 ~ 1 로 바꾼 단위 벡터)과 당긴 세기(0 ~ 1)를 알린다.
//   떼면 구슬이 초당 1600 픽셀로 패드 가운데로 돌아가며 알리고, 다 돌아오면 (0.5, 0.5), 0 을 한 번 알린다.
// - 원본과 다른 점:
//   상태 이름 move 는 러스트 예약어라 moved 다. 원본 상태 값의 글자("wait", "pressed", "move", "released")는 getName 으로 얻는다.
//   이벤트 콜백은 FnMut(Vector2, f32) 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

//==============================================================================
// 가상 패드 상태.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualPadState {
	none,
	pressed,
	moved,
	released,
}

impl VirtualPadState {
	//==============================================================================
	// 원본 상태 값의 글자.
	//==============================================================================
	pub fn getName(&self) -> &'static str {
		match self {
			VirtualPadState::none => {
				"wait"
			}
			VirtualPadState::pressed => {
				"pressed"
			}
			VirtualPadState::moved => {
				"move"
			}
			VirtualPadState::released => {
				"released"
			}
		}
	}
}

// 구슬 이동 이벤트. (방향 0 ~ 1, 당긴 세기 0 ~ 1)
type BallMoveEvent = Box<dyn FnMut(Vector2, f32)>;

//==============================================================================
// 가상 패드.
//==============================================================================
pub struct VirtualPad {
	virtualPadState: VirtualPadState,
	padPosition: Vector2,
	padRadius: f32,
	padColor: Color,
	ballPosition: Vector2,
	ballRadius: f32,
	ballColor: Color,
	ballReturnSpeed: f32,
	ballMoveEvent: Option<BallMoveEvent>,
}

impl VirtualPad {
	//==============================================================================
	// 생성. (패드 반지름 200 검은색, 구슬 반지름 50 흰색, 돌아가는 속도 초당 1600)
	//==============================================================================
	pub fn new() -> VirtualPad {
		VirtualPad {
			virtualPadState: VirtualPadState::none,
			padPosition: Vector2::zero(),
			padRadius: 200.0,
			padColor: Color::create(0.0, 0.0, 0.0, 1.0),
			ballPosition: Vector2::zero(),
			ballRadius: 50.0,
			ballColor: Color::create(1.0, 1.0, 1.0, 1.0),
			ballReturnSpeed: 1600.0,
			ballMoveEvent: None,
		}
	}

	//==============================================================================
	// 구슬 이동 이벤트 부르기. (방향을 0 ~ 1 범위로 바꿔 넘긴다)
	//==============================================================================
	fn fireBallMoveEvent(&mut self, direction: Vector2, pullStrength: f32) {
		if let Some(ballMoveEvent) = self.ballMoveEvent.as_mut() {
			let normalizedDirectionX = (direction.x + 1.0) / 2.0;
			let normalizedDirectionY = (direction.y + 1.0) / 2.0;
			let normalizedDirection = Vector2::create(normalizedDirectionX, normalizedDirectionY);
			ballMoveEvent(normalizedDirection, pullStrength);
		}
	}

	//==============================================================================
	// 갱신. (누르고 끄는 동안 구슬을 패드 안에 가두고, 떼면 가운데로 돌려보낸다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		let virtualPadState = self.getVirtualPadState();
		if virtualPadState == VirtualPadState::none {
			return;
		}

		let padPosition = self.getPadPosition();
		let padRadius = self.getPadRadius();
		let ballPosition = self.getBallPosition();
		let moveDelta = ballPosition.subtract(padPosition);
		let distance = moveDelta.length();
		let direction = moveDelta.normalize();

		// 터치 누르고 끌기: 구슬 이동.
		if virtualPadState == VirtualPadState::pressed || virtualPadState == VirtualPadState::moved {
			// 구슬 중심이 패드를 넘어가지 않도록 가두기.
			if distance > padRadius {
				self.ballPosition = padPosition.add(direction.multiplyScalar(padRadius));
			}
			let clampedDistance = distance.min(padRadius);
			let pullStrength = clampedDistance / padRadius;
			self.fireBallMoveEvent(direction, pullStrength);
		}
		// 터치 뗌: 패드 가운데로 빠르게 돌아가기.
		else if virtualPadState == VirtualPadState::released {
			let returnSpeedDelta = self.ballReturnSpeed * timeDelta;
			if distance > returnSpeedDelta {
				let returnMoveDelta = direction.multiplyScalar(returnSpeedDelta);
				let currentBallPosition = self.getBallPosition();
				self.ballPosition = currentBallPosition.subtract(returnMoveDelta);

				let updatedBallPosition = self.getBallPosition();
				let returnedMoveDelta = updatedBallPosition.subtract(padPosition);
				let returnedDistance = returnedMoveDelta.length();
				let pullStrength = math::clamp01(returnedDistance / padRadius);
				self.fireBallMoveEvent(direction, pullStrength);
			}
			else {
				self.ballPosition = padPosition;
				self.virtualPadState = VirtualPadState::none;
				if let Some(ballMoveEvent) = self.ballMoveEvent.as_mut() {
					let neutralDirection = Vector2::create(0.5, 0.5);
					ballMoveEvent(neutralDirection, 0.0);
				}
			}
		}
	}

	//==============================================================================
	// 출력. (바깥 원 다음 안쪽 원, 부른 쪽의 좌표계 그대로)
	//==============================================================================
	pub fn draw(&self, graphic: &mut Graphic) {
		let padPosition = self.getPadPosition();
		let padRadius = self.getPadRadius();
		let padColor = self.getPadColor();
		graphic.setFillColor(padColor);
		graphic.drawCircle(padPosition, padRadius);

		let ballPosition = self.getBallPosition();
		let ballRadius = self.getBallRadius();
		let ballColor = self.getBallColor();
		graphic.setFillColor(ballColor);
		graphic.drawCircle(ballPosition, ballRadius);
	}

	//==============================================================================
	// 터치 누름. (구슬을 누른 곳으로)
	//==============================================================================
	pub fn touchPress(&mut self, viewInputPosition: Vector2) {
		self.virtualPadState = VirtualPadState::pressed;
		self.ballPosition = viewInputPosition;
	}

	//==============================================================================
	// 터치 끌기. (구슬을 끈 곳으로)
	//==============================================================================
	pub fn touchMove(&mut self, viewInputPosition: Vector2) {
		self.virtualPadState = VirtualPadState::moved;
		self.ballPosition = viewInputPosition;
	}

	//==============================================================================
	// 터치 뗌. (구슬은 그 자리에서 가운데로 돌아가기 시작한다)
	//==============================================================================
	pub fn touchRelease(&mut self, _viewInputPosition: Vector2) {
		self.virtualPadState = VirtualPadState::released;
	}

	//==============================================================================
	// 터치 취소. (뗌과 같다)
	//==============================================================================
	pub fn touchCancel(&mut self) {
		self.virtualPadState = VirtualPadState::released;
	}

	//==============================================================================
	// 가상 패드 상태 반환.
	//==============================================================================
	pub fn getVirtualPadState(&self) -> VirtualPadState {
		self.virtualPadState
	}

	//==============================================================================
	// 구슬 이동 이벤트 설정. (방향 0 ~ 1, 당긴 세기 0 ~ 1)
	//==============================================================================
	pub fn setBallMoveEvent(&mut self, callback: impl FnMut(Vector2, f32) + 'static) {
		self.ballMoveEvent = Some(Box::new(callback));
	}

	//==============================================================================
	// 패드 위치 설정. (구슬도 그 자리로)
	//==============================================================================
	pub fn setPadPosition(&mut self, padPosition: Vector2) {
		self.padPosition = padPosition;
		self.ballPosition = padPosition;
	}

	//==============================================================================
	// 패드 위치 반환.
	//==============================================================================
	pub fn getPadPosition(&self) -> Vector2 {
		self.padPosition
	}

	//==============================================================================
	// 패드 반지름 설정.
	//==============================================================================
	pub fn setPadRadius(&mut self, padRadius: f32) {
		self.padRadius = padRadius;
	}

	//==============================================================================
	// 패드 반지름 반환.
	//==============================================================================
	pub fn getPadRadius(&self) -> f32 {
		self.padRadius
	}

	//==============================================================================
	// 구슬 위치 반환.
	//==============================================================================
	pub fn getBallPosition(&self) -> Vector2 {
		self.ballPosition
	}

	//==============================================================================
	// 구슬 반지름 설정.
	//==============================================================================
	pub fn setBallRadius(&mut self, ballRadius: f32) {
		self.ballRadius = ballRadius;
	}

	//==============================================================================
	// 구슬 반지름 반환.
	//==============================================================================
	pub fn getBallRadius(&self) -> f32 {
		self.ballRadius
	}

	//==============================================================================
	// 패드 색 설정.
	//==============================================================================
	pub fn setPadColor(&mut self, padColor: impl Into<Color>) {
		self.padColor = padColor.into();
	}

	//==============================================================================
	// 패드 색 반환.
	//==============================================================================
	pub fn getPadColor(&self) -> Color {
		self.padColor
	}

	//==============================================================================
	// 구슬 색 설정.
	//==============================================================================
	pub fn setBallColor(&mut self, ballColor: impl Into<Color>) {
		self.ballColor = ballColor.into();
	}

	//==============================================================================
	// 구슬 색 반환.
	//==============================================================================
	pub fn getBallColor(&self) -> Color {
		self.ballColor
	}
}

impl Default for VirtualPad {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> VirtualPad {
		VirtualPad::new()
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	//==============================================================================
	// 끌면 구슬이 패드 안에 갇히고, 떼면 가운데로 돌아와 중립을 알린다.
	//==============================================================================
	#[test]
	fn ballIsClampedAndReturns() {
		let eventList: Rc<RefCell<Vec<(Vector2, f32)>>> = Rc::new(RefCell::new(Vec::new()));
		let eventListForCallback = eventList.clone();
		let mut virtualPad = VirtualPad::new();
		virtualPad.setPadRadius(100.0);
		virtualPad.setPadPosition(Vector2::create(200.0, 200.0));
		virtualPad.setBallMoveEvent(move |direction, pullStrength| {
			eventListForCallback.borrow_mut().push((direction, pullStrength));
		});
		virtualPad.tick(0.016);
		assert!(eventList.borrow().is_empty());

		virtualPad.touchPress(Vector2::create(200.0, 200.0));
		virtualPad.touchMove(Vector2::create(500.0, 200.0));
		assert_eq!(virtualPad.getVirtualPadState().getName(), "move");
		virtualPad.tick(0.016);
		assert_eq!(virtualPad.getBallPosition(), Vector2::create(300.0, 200.0));
		assert_eq!(eventList.borrow()[0], (Vector2::create(1.0, 0.5), 1.0));

		virtualPad.touchRelease(Vector2::create(500.0, 200.0));
		virtualPad.tick(0.025);
		assert_eq!(virtualPad.getBallPosition(), Vector2::create(260.0, 200.0));
		let (_, returningStrength) = eventList.borrow()[1];
		assert!((returningStrength - 0.6).abs() < 0.0001);
		virtualPad.tick(0.1);
		assert_eq!(virtualPad.getBallPosition(), Vector2::create(200.0, 200.0));
		assert_eq!(virtualPad.getVirtualPadState(), VirtualPadState::none);
		assert_eq!(eventList.borrow()[2], (Vector2::create(0.5, 0.5), 0.0));
	}
}
