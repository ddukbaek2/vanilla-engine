//==============================================================================
// 포인터 제스처 인식기. (노드 없이)
// - 원본 misc/pointergesture.js 를 옮긴 것이다. 탭 / 드래그 / 롱프레스를 임계값으로 나눈다. 노드에 얽매이지 않아 어디서나 쓴다.
// - 사용: press(x, y) → move(x, y) → tick(timeDelta) → release() 순으로 넘기고,
//   콜백(setTapEvent / setDragEvent / setDragEndEvent / setLongPressEvent)으로 결과를 받는다.
// - 원본과 다른 점:
//   이름 move 는 러스트 예약어라 r#move 로 부른다. (pointerGesture.r#move(x, y))
//   options 객체는 PointerGestureOptions 구조체다. 콜백은 Vector2 를 받는 FnMut 이다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
// 제스처 설정. (원본 options = { dragThreshold = 12, longPressSeconds = 0.6 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerGestureOptions {
	// 누른 곳에서 이만큼 움직이면 드래그로 친다. (픽셀)
	pub dragThreshold: f32,
	// 움직이지 않고 이만큼 누르고 있으면 롱프레스. (초)
	pub longPressSeconds: f32,
}

impl Default for PointerGestureOptions {
	//==============================================================================
	// 기본값. (12 픽셀, 0.6 초)
	//==============================================================================
	fn default() -> PointerGestureOptions {
		PointerGestureOptions {
			dragThreshold: 12.0,
			longPressSeconds: 0.6,
		}
	}
}

// 위치 하나를 받는 알림. (탭, 롱프레스)
type PositionEvent = Box<dyn FnMut(Vector2)>;
// 지금 위치와 누른 위치를 받는 알림. (드래그, 드래그 끝)
type DragEvent = Box<dyn FnMut(Vector2, Vector2)>;

//==============================================================================
// 포인터 제스처 인식기.
//==============================================================================
pub struct PointerGesture {
	isPressed: bool,
	isDragging: bool,
	isLongPressed: bool,
	pressPosition: Vector2,
	currentPosition: Vector2,
	pressedSeconds: f32,
	dragThreshold: f32,
	longPressSeconds: f32,
	tapEvent: Option<PositionEvent>,
	dragEvent: Option<DragEvent>,
	dragEndEvent: Option<DragEvent>,
	longPressEvent: Option<PositionEvent>,
}

impl PointerGesture {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(options: PointerGestureOptions) -> PointerGesture {
		PointerGesture {
			isPressed: false,
			isDragging: false,
			isLongPressed: false,
			pressPosition: Vector2::zero(),
			currentPosition: Vector2::zero(),
			pressedSeconds: 0.0,
			dragThreshold: options.dragThreshold,
			longPressSeconds: options.longPressSeconds,
			tapEvent: None,
			dragEvent: None,
			dragEndEvent: None,
			longPressEvent: None,
		}
	}

	//==============================================================================
	// 누름.
	//==============================================================================
	pub fn press(&mut self, x: f32, y: f32) {
		self.isPressed = true;
		self.isDragging = false;
		self.isLongPressed = false;
		self.pressedSeconds = 0.0;
		self.pressPosition = Vector2::create(x, y);
		self.currentPosition = Vector2::create(x, y);
	}

	//==============================================================================
	// 이동. (누른 곳에서 임계값을 넘으면 드래그로 정하고, 그 뒤 움직일 때마다 드래그 알림을 부른다)
	//==============================================================================
	pub fn r#move(&mut self, x: f32, y: f32) {
		if !self.isPressed {
			return;
		}
		self.currentPosition = Vector2::create(x, y);
		if !self.isDragging {
			let movedX = x - self.pressPosition.x;
			let movedY = y - self.pressPosition.y;
			if movedX * movedX + movedY * movedY >= self.dragThreshold * self.dragThreshold {
				self.isDragging = true;
			}
		}
		if self.isDragging {
			if let Some(dragEvent) = self.dragEvent.as_mut() {
				dragEvent(self.currentPosition, self.pressPosition);
			}
		}
	}

	//==============================================================================
	// 갱신. (드래그가 아닌 채 longPressSeconds 를 넘기면 롱프레스를 한 번 알린다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		if !self.isPressed || self.isDragging || self.isLongPressed {
			return;
		}
		self.pressedSeconds += timeDelta;
		if self.pressedSeconds >= self.longPressSeconds {
			self.isLongPressed = true;
			if let Some(longPressEvent) = self.longPressEvent.as_mut() {
				longPressEvent(self.pressPosition);
			}
		}
	}

	//==============================================================================
	// 뗌. (드래그였으면 드래그 끝, 롱프레스였으면 아무것도, 둘 다 아니면 탭으로 알린다)
	//==============================================================================
	pub fn release(&mut self) {
		if !self.isPressed {
			return;
		}
		let wasDragging = self.isDragging;
		let wasLongPressed = self.isLongPressed;
		self.isPressed = false;
		self.isDragging = false;
		self.isLongPressed = false;
		if wasDragging {
			if let Some(dragEndEvent) = self.dragEndEvent.as_mut() {
				dragEndEvent(self.currentPosition, self.pressPosition);
			}
			return;
		}
		if wasLongPressed {
			return;
		}
		if let Some(tapEvent) = self.tapEvent.as_mut() {
			tapEvent(self.currentPosition);
		}
	}

	//==============================================================================
	// 취소. (아무것도 알리지 않고 상태만 되돌린다)
	//==============================================================================
	pub fn cancel(&mut self) {
		self.isPressed = false;
		self.isDragging = false;
		self.isLongPressed = false;
	}

	//==============================================================================
	// 롱프레스 진행 비율. (게이지 표시용, 0 ~ 1)
	//==============================================================================
	pub fn getLongPressRatio(&self) -> f32 {
		if !self.isPressed || self.isDragging || self.longPressSeconds <= 0.0 {
			return 0.0;
		}
		(self.pressedSeconds / self.longPressSeconds).min(1.0)
	}

	//==============================================================================
	// 눌려 있는지.
	//==============================================================================
	pub fn isPressed(&self) -> bool {
		self.isPressed
	}

	//==============================================================================
	// 드래그 중인지.
	//==============================================================================
	pub fn isDragging(&self) -> bool {
		self.isDragging
	}

	//==============================================================================
	// 탭 알림 설정. (뗀 위치)
	//==============================================================================
	pub fn setTapEvent(&mut self, tapEvent: impl FnMut(Vector2) + 'static) {
		self.tapEvent = Some(Box::new(tapEvent));
	}

	//==============================================================================
	// 드래그 알림 설정. (지금 위치, 누른 위치)
	//==============================================================================
	pub fn setDragEvent(&mut self, dragEvent: impl FnMut(Vector2, Vector2) + 'static) {
		self.dragEvent = Some(Box::new(dragEvent));
	}

	//==============================================================================
	// 드래그 끝 알림 설정. (지금 위치, 누른 위치)
	//==============================================================================
	pub fn setDragEndEvent(&mut self, dragEndEvent: impl FnMut(Vector2, Vector2) + 'static) {
		self.dragEndEvent = Some(Box::new(dragEndEvent));
	}

	//==============================================================================
	// 롱프레스 알림 설정. (누른 위치)
	//==============================================================================
	pub fn setLongPressEvent(&mut self, longPressEvent: impl FnMut(Vector2) + 'static) {
		self.longPressEvent = Some(Box::new(longPressEvent));
	}

	//==============================================================================
	// 드래그 임계값 설정. (픽셀)
	//==============================================================================
	pub fn setDragThreshold(&mut self, dragThreshold: f32) {
		self.dragThreshold = dragThreshold;
	}

	//==============================================================================
	// 롱프레스 시간 설정. (초)
	//==============================================================================
	pub fn setLongPressSeconds(&mut self, longPressSeconds: f32) {
		self.longPressSeconds = longPressSeconds;
	}
}

impl Default for PointerGesture {
	//==============================================================================
	// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> PointerGesture {
		PointerGesture::new(PointerGestureOptions::default())
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
	// 탭, 드래그, 롱프레스, 취소를 알림 기록으로 확인한다.
	//==============================================================================
	#[test]
	fn classifiesTapDragAndLongPress() {
		let eventLog: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let mut pointerGesture = PointerGesture::default();
		let tapLog = eventLog.clone();
		pointerGesture.setTapEvent(move |position| {
			tapLog.borrow_mut().push(format!("tap {} {}", position.x, position.y));
		});
		let dragLog = eventLog.clone();
		pointerGesture.setDragEvent(move |currentPosition, pressPosition| {
			dragLog.borrow_mut().push(format!("drag {} from {}", currentPosition.x, pressPosition.x));
		});
		let dragEndLog = eventLog.clone();
		pointerGesture.setDragEndEvent(move |currentPosition, _| {
			dragEndLog.borrow_mut().push(format!("dragEnd {}", currentPosition.x));
		});
		let longPressLog = eventLog.clone();
		pointerGesture.setLongPressEvent(move |pressPosition| {
			longPressLog.borrow_mut().push(format!("longPress {}", pressPosition.x));
		});

		// 탭. (임계값 안의 움직임)
		pointerGesture.press(10.0, 10.0);
		pointerGesture.r#move(15.0, 10.0);
		pointerGesture.tick(0.3);
		assert!((pointerGesture.getLongPressRatio() - 0.5).abs() < 0.0001);
		pointerGesture.release();

		// 드래그.
		pointerGesture.press(0.0, 0.0);
		pointerGesture.r#move(12.0, 0.0);
		assert!(pointerGesture.isDragging());
		pointerGesture.r#move(20.0, 0.0);
		pointerGesture.tick(1.0);
		pointerGesture.release();

		// 롱프레스. (그 뒤 떼도 탭이 아니다)
		pointerGesture.press(5.0, 5.0);
		pointerGesture.tick(0.4);
		pointerGesture.tick(0.4);
		pointerGesture.tick(0.4);
		pointerGesture.release();

		// 취소는 아무것도 알리지 않는다.
		pointerGesture.press(1.0, 1.0);
		pointerGesture.cancel();
		pointerGesture.release();
		assert!(!pointerGesture.isPressed());

		assert_eq!(*eventLog.borrow(), vec!["tap 15 10", "drag 12 from 0", "drag 20 from 0", "dragEnd 20", "longPress 5"]);
	}
}
