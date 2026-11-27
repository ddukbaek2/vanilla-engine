//==============================================================================
//! 포인터 제스처 인식기. (노드 없이)
//! - 원본 misc/pointergesture.js 를 옮긴 것이다. 탭 / 드래그 / 롱프레스를 임계값으로 나눈다. 노드에 얽매이지 않아 어디서나 쓴다.
//! - 사용: press(x, y) → move(x, y) → tick(time_delta) → release() 순으로 넘기고,
//!   콜백(set_tap_event / set_drag_event / set_drag_end_event / set_long_press_event)으로 결과를 받는다.
//! - 원본과 다른 점:
//!   이동은 두 엔진 모두 move_pointer 다. (원본 movePointer, 예약어 move 를 이름으로 쓰지 않는다)
//!   options 객체는 PointerGestureOptions 구조체다. 콜백은 Vector2 를 받는 FnMut 이다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
/// 제스처 설정. (원본 options = { drag_threshold = 12, long_press_seconds = 0.6 })
///
/// # 예제
/// 롱프레스 시간만 바꾸기
/// ```rust
/// use vanilla::{PointerGesture, PointerGestureOptions};
///
/// let pointer_gesture_options = PointerGestureOptions {
/// 	long_press_seconds: 1.0,
/// 	..PointerGestureOptions::default()
/// };
/// assert_eq!(pointer_gesture_options.drag_threshold, 12.0);
/// let pointer_gesture = PointerGesture::new(pointer_gesture_options);
/// assert!(!pointer_gesture.is_pressed());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerGestureOptions {
	/// 누른 곳에서 이만큼 움직이면 드래그로 친다. (픽셀)
	pub drag_threshold: f64,
	/// 움직이지 않고 이만큼 누르고 있으면 롱프레스. (초)
	pub long_press_seconds: f64,
}

impl Default for PointerGestureOptions {
	//==============================================================================
	/// 기본값. (12 픽셀, 0.6 초)
	//==============================================================================
	fn default() -> PointerGestureOptions {
		PointerGestureOptions {
			drag_threshold: 12.0,
			long_press_seconds: 0.6,
		}
	}
}

/// 위치 하나를 받는 알림. (탭, 롱프레스)
type PositionEvent = Box<dyn FnMut(Vector2)>;
/// 지금 위치와 누른 위치를 받는 알림. (드래그, 드래그 끝)
type DragEvent = Box<dyn FnMut(Vector2, Vector2)>;

//==============================================================================
/// 포인터 제스처 인식기.
/// - press, move_pointer, tick, release 차례로 넘기고 콜백으로 탭, 드래그, 드래그 끝, 롱프레스를 받는다.
///
/// # 예제
/// 탭과 드래그 가르기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{PointerGesture, PointerGestureOptions, Vector2};
///
/// let event_list = Rc::new(RefCell::new(Vec::new()));
/// let mut pointer_gesture = PointerGesture::new(PointerGestureOptions::default());
/// let tap_event_list = event_list.clone();
/// pointer_gesture.set_tap_event(move |position| {
/// 	tap_event_list.borrow_mut().push(("tap", position));
/// });
/// let drag_end_event_list = event_list.clone();
/// pointer_gesture.set_drag_end_event(move |position, _press_position| {
/// 	drag_end_event_list.borrow_mut().push(("drag_end", position));
/// });
///
/// pointer_gesture.press(10.0, 10.0);
/// pointer_gesture.move_pointer(14.0, 10.0);
/// pointer_gesture.tick(0.1);
/// pointer_gesture.release();
///
/// pointer_gesture.press(10.0, 10.0);
/// pointer_gesture.move_pointer(40.0, 10.0);
/// assert!(pointer_gesture.is_dragging());
/// pointer_gesture.release();
///
/// assert_eq!(*event_list.borrow(), vec![("tap", Vector2::create(14.0, 10.0)), ("drag_end", Vector2::create(40.0, 10.0))]);
/// ```
//==============================================================================
pub struct PointerGesture {
	is_pressed: bool,
	is_dragging: bool,
	is_long_pressed: bool,
	press_position: Vector2,
	current_position: Vector2,
	pressed_seconds: f64,
	drag_threshold: f64,
	long_press_seconds: f64,
	tap_event: Option<PositionEvent>,
	drag_event: Option<DragEvent>,
	drag_end_event: Option<DragEvent>,
	long_press_event: Option<PositionEvent>,
}

impl PointerGesture {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `options`: 드래그 임계값과 롱프레스 시간.
	///
	/// # 돌려줌
	/// 눌리지 않고 콜백이 없는 새 인식기.
	//==============================================================================
	pub fn new(options: PointerGestureOptions) -> PointerGesture {
		PointerGesture {
			is_pressed: false,
			is_dragging: false,
			is_long_pressed: false,
			press_position: Vector2::zero(),
			current_position: Vector2::zero(),
			pressed_seconds: 0.0,
			drag_threshold: options.drag_threshold,
			long_press_seconds: options.long_press_seconds,
			tap_event: None,
			drag_event: None,
			drag_end_event: None,
			long_press_event: None,
		}
	}

	//==============================================================================
	/// 누름.
	/// - 드래그, 롱프레스 상태와 누른 시간을 비우고 누른 위치를 기억한다.
	///
	/// # 인자
	/// - `x`: 누른 위치 x. (픽셀, 좌표계는 넘기는 쪽이 정한다)
	/// - `y`: 누른 위치 y. (픽셀)
	//==============================================================================
	pub fn press(&mut self, x: f64, y: f64) {
		self.is_pressed = true;
		self.is_dragging = false;
		self.is_long_pressed = false;
		self.pressed_seconds = 0.0;
		self.press_position = Vector2::create(x, y);
		self.current_position = Vector2::create(x, y);
	}

	//==============================================================================
	/// 이동. (누른 곳에서 임계값을 넘으면 드래그로 정하고, 그 뒤 움직일 때마다 드래그 알림을 부른다)
	/// - 눌려 있지 않으면 아무것도 하지 않는다. 누른 곳에서 거리가 임계값 이상이면 드래그다.
	///
	/// # 인자
	/// - `x`: 지금 위치 x. (픽셀, press 와 같은 좌표계)
	/// - `y`: 지금 위치 y. (픽셀)
	//==============================================================================
	pub fn move_pointer(&mut self, x: f64, y: f64) {
		if !self.is_pressed {
			return;
		}
		self.current_position = Vector2::create(x, y);
		if !self.is_dragging {
			let moved_x = x - self.press_position.x;
			let moved_y = y - self.press_position.y;
			if moved_x * moved_x + moved_y * moved_y >= self.drag_threshold * self.drag_threshold {
				self.is_dragging = true;
			}
		}
		if self.is_dragging {
			if let Some(drag_event) = self.drag_event.as_mut() {
				drag_event(self.current_position, self.press_position);
			}
		}
	}

	//==============================================================================
	/// 갱신. (드래그가 아닌 채 long_press_seconds 를 넘기면 롱프레스를 한 번 알린다)
	///
	/// # 인자
	/// - `time_delta`: 지난 시간. (초, 누른 시간에 더한다)
	///
	/// # 예제
	/// 0.6 초 누르고 있으면 롱프레스, 뗄 때 탭은 없다
	/// ```rust
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	/// use vanilla::{PointerGesture, PointerGestureOptions};
	///
	/// let long_press_count = Rc::new(Cell::new(0));
	/// let tap_count = Rc::new(Cell::new(0));
	/// let mut pointer_gesture = PointerGesture::new(PointerGestureOptions::default());
	/// let long_press_count_for_event = long_press_count.clone();
	/// pointer_gesture.set_long_press_event(move |_position| {
	/// 	long_press_count_for_event.set(long_press_count_for_event.get() + 1);
	/// });
	/// let tap_count_for_event = tap_count.clone();
	/// pointer_gesture.set_tap_event(move |_position| {
	/// 	tap_count_for_event.set(tap_count_for_event.get() + 1);
	/// });
	///
	/// pointer_gesture.press(50.0, 50.0);
	/// pointer_gesture.tick(0.3);
	/// assert!((pointer_gesture.get_long_press_ratio() - 0.5).abs() < 0.0001);
	/// pointer_gesture.tick(0.3);
	/// pointer_gesture.tick(0.3);
	/// pointer_gesture.release();
	/// assert_eq!(long_press_count.get(), 1);
	/// assert_eq!(tap_count.get(), 0);
	/// ```
	//==============================================================================
	pub fn tick(&mut self, time_delta: f64) {
		if !self.is_pressed || self.is_dragging || self.is_long_pressed {
			return;
		}
		self.pressed_seconds += time_delta;
		if self.pressed_seconds >= self.long_press_seconds {
			self.is_long_pressed = true;
			if let Some(long_press_event) = self.long_press_event.as_mut() {
				long_press_event(self.press_position);
			}
		}
	}

	//==============================================================================
	/// 뗌. (드래그였으면 드래그 끝, 롱프레스였으면 아무것도, 둘 다 아니면 탭으로 알린다)
	//==============================================================================
	pub fn release(&mut self) {
		if !self.is_pressed {
			return;
		}
		let was_dragging = self.is_dragging;
		let was_long_pressed = self.is_long_pressed;
		self.is_pressed = false;
		self.is_dragging = false;
		self.is_long_pressed = false;
		if was_dragging {
			if let Some(drag_end_event) = self.drag_end_event.as_mut() {
				drag_end_event(self.current_position, self.press_position);
			}
			return;
		}
		if was_long_pressed {
			return;
		}
		if let Some(tap_event) = self.tap_event.as_mut() {
			tap_event(self.current_position);
		}
	}

	//==============================================================================
	/// 취소. (아무것도 알리지 않고 상태만 되돌린다)
	//==============================================================================
	pub fn cancel(&mut self) {
		self.is_pressed = false;
		self.is_dragging = false;
		self.is_long_pressed = false;
	}

	//==============================================================================
	/// 롱프레스 진행 비율. (게이지 표시용, 0 ~ 1)
	///
	/// # 돌려줌
	/// 누른 시간 / 롱프레스 시간. (1 에서 자른다) 눌려 있지 않거나, 드래그 중이거나, 롱프레스 시간이 0 이하면 0.
	//==============================================================================
	pub fn get_long_press_ratio(&self) -> f64 {
		if !self.is_pressed || self.is_dragging || self.long_press_seconds <= 0.0 {
			return 0.0;
		}
		(self.pressed_seconds / self.long_press_seconds).min(1.0)
	}

	//==============================================================================
	/// 눌려 있는지.
	///
	/// # 돌려줌
	/// press 뒤 release 나 cancel 전이면 true.
	//==============================================================================
	pub fn is_pressed(&self) -> bool {
		self.is_pressed
	}

	//==============================================================================
	/// 드래그 중인지.
	///
	/// # 돌려줌
	/// 이번 누름에서 임계값을 넘게 움직였으면 true.
	//==============================================================================
	pub fn is_dragging(&self) -> bool {
		self.is_dragging
	}

	//==============================================================================
	/// 탭 알림 설정. (뗀 위치)
	///
	/// # 인자
	/// - `tap_event`: 탭일 때 부를 함수. 마지막 위치를 받는다. (앞의 알림은 바꾼다)
	//==============================================================================
	pub fn set_tap_event(&mut self, tap_event: impl FnMut(Vector2) + 'static) {
		self.tap_event = Some(Box::new(tap_event));
	}

	//==============================================================================
	/// 드래그 알림 설정. (지금 위치, 누른 위치)
	///
	/// # 인자
	/// - `drag_event`: 드래그 중 움직일 때마다 부를 함수. 지금 위치와 누른 위치를 받는다.
	//==============================================================================
	pub fn set_drag_event(&mut self, drag_event: impl FnMut(Vector2, Vector2) + 'static) {
		self.drag_event = Some(Box::new(drag_event));
	}

	//==============================================================================
	/// 드래그 끝 알림 설정. (지금 위치, 누른 위치)
	///
	/// # 인자
	/// - `drag_end_event`: 드래그하다 뗄 때 부를 함수. 마지막 위치와 누른 위치를 받는다.
	//==============================================================================
	pub fn set_drag_end_event(&mut self, drag_end_event: impl FnMut(Vector2, Vector2) + 'static) {
		self.drag_end_event = Some(Box::new(drag_end_event));
	}

	//==============================================================================
	/// 롱프레스 알림 설정. (누른 위치)
	///
	/// # 인자
	/// - `long_press_event`: 롱프레스가 되는 순간 한 번 부를 함수. 누른 위치를 받는다.
	//==============================================================================
	pub fn set_long_press_event(&mut self, long_press_event: impl FnMut(Vector2) + 'static) {
		self.long_press_event = Some(Box::new(long_press_event));
	}

	//==============================================================================
	/// 드래그 임계값 설정. (픽셀)
	///
	/// # 인자
	/// - `drag_threshold`: 드래그로 칠 거리. (픽셀)
	//==============================================================================
	pub fn set_drag_threshold(&mut self, drag_threshold: f64) {
		self.drag_threshold = drag_threshold;
	}

	//==============================================================================
	/// 롱프레스 시간 설정. (초)
	///
	/// # 인자
	/// - `long_press_seconds`: 롱프레스로 칠 누른 시간. (초)
	//==============================================================================
	pub fn set_long_press_seconds(&mut self, long_press_seconds: f64) {
		self.long_press_seconds = long_press_seconds;
	}
}

impl Default for PointerGesture {
	//==============================================================================
	/// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> PointerGesture {
		PointerGesture::new(PointerGestureOptions::default())
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
	/// 탭, 드래그, 롱프레스, 취소를 알림 기록으로 확인한다.
	//==============================================================================
	#[test]
	fn classifies_tap_drag_and_long_press() {
		let event_log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let mut pointer_gesture = PointerGesture::default();
		let tap_log = event_log.clone();
		pointer_gesture.set_tap_event(move |position| {
			tap_log.borrow_mut().push(format!("tap {} {}", position.x, position.y));
		});
		let drag_log = event_log.clone();
		pointer_gesture.set_drag_event(move |current_position, press_position| {
			drag_log.borrow_mut().push(format!("drag {} from {}", current_position.x, press_position.x));
		});
		let drag_end_log = event_log.clone();
		pointer_gesture.set_drag_end_event(move |current_position, _| {
			drag_end_log.borrow_mut().push(format!("dragEnd {}", current_position.x));
		});
		let long_press_log = event_log.clone();
		pointer_gesture.set_long_press_event(move |press_position| {
			long_press_log.borrow_mut().push(format!("longPress {}", press_position.x));
		});

		// 탭. (임계값 안의 움직임)
		pointer_gesture.press(10.0, 10.0);
		pointer_gesture.move_pointer(15.0, 10.0);
		pointer_gesture.tick(0.3);
		assert!((pointer_gesture.get_long_press_ratio() - 0.5).abs() < 0.0001);
		pointer_gesture.release();

		// 드래그.
		pointer_gesture.press(0.0, 0.0);
		pointer_gesture.move_pointer(12.0, 0.0);
		assert!(pointer_gesture.is_dragging());
		pointer_gesture.move_pointer(20.0, 0.0);
		pointer_gesture.tick(1.0);
		pointer_gesture.release();

		// 롱프레스. (그 뒤 떼도 탭이 아니다)
		pointer_gesture.press(5.0, 5.0);
		pointer_gesture.tick(0.4);
		pointer_gesture.tick(0.4);
		pointer_gesture.tick(0.4);
		pointer_gesture.release();

		// 취소는 아무것도 알리지 않는다.
		pointer_gesture.press(1.0, 1.0);
		pointer_gesture.cancel();
		pointer_gesture.release();
		assert!(!pointer_gesture.is_pressed());

		assert_eq!(*event_log.borrow(), vec!["tap 15 10", "drag 12 from 0", "drag 20 from 0", "dragEnd 20", "longPress 5"]);
	}
}
