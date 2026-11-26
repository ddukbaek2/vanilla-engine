//==============================================================================
//! 다시 들어와도 안전한 사건 보내기. (웹 플랫폼의 키, 마우스, 손가락, 페이지 리스너가 쓴다)
//! - 브라우저는 프레임 콜백 안에서도 사건 리스너를 그 자리에서 부른다. (게임 코드의 dispatchEvent, focus(), blur(), click() 같은 것)
//!   그때 프레임 콜백은 이미 웹 실행자를 빌려 쥐고 있어, 리스너가 RefCell 을 또 빌리면 패닉하고 wasm 은 unreachable 로 멈춘다.
//! - 원본은 사건을 받는 자리에서 바로 처리한다. 여기서도 바로 빌릴 수 있으면 쌓인 것을 먼저 처리한 뒤 그 자리에서 처리하고,
//!   못 빌리면 줄에 쌓아 두었다가 프레임 콜백이 끝날 때(빌린 채로) 온 차례대로 처리한다. 버리는 사건이 없고 차례도 바뀌지 않는다.
//==============================================================================
use std::cell::RefCell;
use std::collections::VecDeque;

/// 줄에 쌓아 둔 사건 처리. (받는 쪽을 빌려 넘기면 한 번 부른다)
pub(crate) type PendingEventHandler<T> = Box<dyn FnOnce(&mut T)>;

//==============================================================================
/// 사건 줄.
//==============================================================================
pub(crate) struct EventQueue<T> {
	/// 받는 쪽을 빌리지 못해 쌓아 둔 사건 처리. (온 차례)
	pending_handler_list: RefCell<VecDeque<PendingEventHandler<T>>>,
}

impl<T> EventQueue<T> {
	//==============================================================================
	/// 생성. (빈 줄)
	//==============================================================================
	pub(crate) fn new() -> EventQueue<T> {
		EventQueue {
			pending_handler_list: RefCell::new(VecDeque::new()),
		}
	}

	//==============================================================================
	/// 사건 보내기. (받는 쪽을 빌릴 수 있으면 쌓인 것부터 처리하고 이것을 그 자리에서 처리, 못 빌리면 줄에 쌓는다)
	///
	/// # 인자
	/// - `target_cell`: 받는 쪽. (웹 실행자)
	/// - `handler`: 받는 쪽을 빌려 할 일.
	///
	/// # 돌려줌
	/// 그 자리에서 처리했으면 true, 줄에 쌓았으면 false.
	//==============================================================================
	pub(crate) fn dispatch(&self, target_cell: &RefCell<T>, handler: impl FnOnce(&mut T) + 'static) -> bool {
		match target_cell.try_borrow_mut() {
			Ok(mut target) => {
				self.drain(&mut target);
				handler(&mut target);
				true
			}
			Err(_) => {
				self.pending_handler_list.borrow_mut().push_back(Box::new(handler));
				false
			}
		}
	}

	//==============================================================================
	/// 쌓인 사건 처리. (프레임 콜백이 받는 쪽을 빌린 채 끝에서 부른다, 처리하다 새로 쌓인 것도 이어 처리한다)
	///
	/// # 인자
	/// - `target`: 빌린 받는 쪽.
	//==============================================================================
	pub(crate) fn drain(&self, target: &mut T) {
		loop {
			// 처리하는 동안 사건이 다시 쌓일 수 있어 하나씩 꺼낸다. (줄은 꺼낼 때만 빌린다)
			let pending_handler = self.pending_handler_list.borrow_mut().pop_front();
			match pending_handler {
				Some(pending_handler) => {
					pending_handler(target);
				}
				None => {
					break;
				}
			}
		}
	}

	//==============================================================================
	/// 쌓인 사건 수.
	///
	/// # 돌려줌
	/// 아직 처리하지 않은 사건 수.
	//==============================================================================
	#[cfg(test)]
	pub(crate) fn get_pending_count(&self) -> usize {
		self.pending_handler_list.borrow().len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::rc::Rc;

	//==============================================================================
	/// 받는 쪽을 빌려 쥔 동안(프레임 안) 보낸 사건은 패닉 없이 쌓였다가 끝에서 온 차례대로 처리되는지.
	//==============================================================================
	#[test]
	fn dispatch_while_borrowed_is_queued_in_order() {
		let event_queue: EventQueue<Vec<String>> = EventQueue::new();
		let target_cell = RefCell::new(Vec::new());
		{
			// 프레임 콜백이 빌려 쥔 동안 키 사건 둘이 다시 들어온다.
			let mut target = target_cell.borrow_mut();
			target.push(String::from("frame"));
			assert!(!event_queue.dispatch(&target_cell, |target: &mut Vec<String>| target.push(String::from("KeyDown KeyA"))));
			assert!(!event_queue.dispatch(&target_cell, |target: &mut Vec<String>| target.push(String::from("KeyUp KeyA"))));
			assert_eq!(event_queue.get_pending_count(), 2);
			event_queue.drain(&mut target);
		}
		assert_eq!(event_queue.get_pending_count(), 0);
		assert_eq!(*target_cell.borrow(), vec![String::from("frame"), String::from("KeyDown KeyA"), String::from("KeyUp KeyA")]);
	}

	//==============================================================================
	/// 빌릴 수 있으면 쌓인 것을 먼저 처리하고 그 자리에서 처리하는지. (차례가 바뀌지 않는다)
	//==============================================================================
	#[test]
	fn dispatch_when_free_drains_pending_first() {
		let event_queue: EventQueue<Vec<i32>> = EventQueue::new();
		let target_cell = RefCell::new(Vec::new());
		{
			let _target = target_cell.borrow_mut();
			event_queue.dispatch(&target_cell, |target: &mut Vec<i32>| target.push(1));
		}
		assert!(event_queue.dispatch(&target_cell, |target: &mut Vec<i32>| target.push(2)));
		assert_eq!(*target_cell.borrow(), vec![1, 2]);
		assert_eq!(event_queue.get_pending_count(), 0);
	}

	//==============================================================================
	/// 처리하다 그 안에서 사건이 또 들어와도(받는 쪽은 빌려 있다) 같은 처리에서 이어 처리되는지.
	//==============================================================================
	#[test]
	fn reentrant_dispatch_during_handler_is_drained() {
		let event_queue: Rc<EventQueue<Vec<i32>>> = Rc::new(EventQueue::new());
		let target_cell = Rc::new(RefCell::new(Vec::new()));
		let event_queue_for_handler = event_queue.clone();
		let target_cell_for_handler = target_cell.clone();
		let is_handled_now = event_queue.dispatch(&target_cell, move |target: &mut Vec<i32>| {
			target.push(1);
			// 처리 안에서 다시 보낸 사건. (받는 쪽은 빌려 있어 쌓인다)
			let is_nested_handled_now = event_queue_for_handler.dispatch(&target_cell_for_handler, |target: &mut Vec<i32>| target.push(2));
			assert!(!is_nested_handled_now);
		});
		assert!(is_handled_now);
		assert_eq!(*target_cell.borrow(), vec![1]);
		let mut target = target_cell.borrow_mut();
		event_queue.drain(&mut target);
		assert_eq!(*target, vec![1, 2]);
	}
}
