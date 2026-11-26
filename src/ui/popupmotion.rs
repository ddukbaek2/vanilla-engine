//==============================================================================
//! 팝업 등장 / 퇴장 전환기. (원본 ui/popupmotion.js 의 PopupMotion, PopupMotionState)
//! - closed → opening → open → closing → closed 네 상태를 오가며 배율(등장: 0.45 → 1.18 되튕김 → 1, 퇴장: 1 → 1.08 → 0.08)과
//!   어둡게 가리는 비율(dim_ratio)을 산출한다. 노드도 컴포넌트도 아닌 계산기라서 원본 그대로 옮겼다.
//! - 매 프레임 tick(time_delta) 뒤 get_scale() / get_dim_ratio() 를 읽어 노드 배율 / 막 알파에 건다. set_instant_enabled(true) 면 연출 없이 바로 바뀐다.
//! - 원본과 다른 것:
//!   - 상태는 문자열 대신 PopupMotionState 열거형이다.
//!   - options 객체는 PopupMotionOptions 구조체다. 열림 / 닫힘 알림은 인자 없는 `Box<dyn FnMut()>` 다.
//!     (노드를 바꿔야 하는 UIDialog 는 알림 대신 tick 앞뒤의 상태를 비교해 처리한다)
//==============================================================================

//==============================================================================
/// 전환 상태.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupMotionState {
	/// 닫힘. (배율 0, 가림 0)
	Closed,
	/// 여는 중. (배율이 0.45 에서 1.18 로 부풀었다가 1 로)
	Opening,
	/// 열림. (배율 1, 가림 1, 이때만 조작할 수 있다)
	Open,
	/// 닫는 중. (배율이 1.08 로 부풀었다가 0.08 로)
	Closing,
}

//==============================================================================
/// 전환기 설정. (원본 options = { open_duration = 0.32, close_duration = 0.22 })
///
/// # 예제
/// 등장 시간만 줄이기
/// ```rust
/// use vanilla::{PopupMotion, PopupMotionOptions, PopupMotionState};
///
/// let options = PopupMotionOptions {
/// 	open_duration: 0.2,
/// 	..PopupMotionOptions::default()
/// };
/// assert_eq!(options.close_duration, 0.22);
/// let mut popup_motion = PopupMotion::create_with_options(options);
/// popup_motion.open();
/// popup_motion.tick(0.2);
/// assert_eq!(popup_motion.get_state(), PopupMotionState::Open);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupMotionOptions {
	/// 등장 시간. (초)
	pub open_duration: f64,
	/// 퇴장 시간. (초)
	pub close_duration: f64,
}

impl Default for PopupMotionOptions {
	//==============================================================================
	/// 기본값. (등장 0.32 초, 퇴장 0.22 초)
	//==============================================================================
	fn default() -> PopupMotionOptions {
		PopupMotionOptions {
			open_duration: 0.32,
			close_duration: 0.22,
		}
	}
}

//==============================================================================
/// 알림. (인자 없음)
//==============================================================================
pub type PopupMotionEvent = Box<dyn FnMut()>;

//==============================================================================
/// 팝업 전환기.
///
/// # 예제
/// 판의 배율과 막의 알파에 걸기
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{Color, NodeTree, Paint, PopupMotion, PopupMotionState, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let dim_id = node_tree.create_child(root_id, "Dim");
/// node_tree[dim_id].add_component(Paint::create_with_color(Color::create(0.0, 0.0, 0.0, 0.0)));
/// let panel_id = node_tree.create_child(root_id, "Panel");
/// let opened_count = Rc::new(Cell::new(0));
/// let opened_count_for_event = opened_count.clone();
/// let mut popup_motion = PopupMotion::new();
/// popup_motion.set_opened_event(Box::new(move || {
/// 	opened_count_for_event.set(opened_count_for_event.get() + 1);
/// }));
///
/// popup_motion.open();
/// for _frame_index in 0..30 {
/// 	popup_motion.tick(1.0 / 60.0);
/// 	let scale = popup_motion.get_scale();
/// 	node_tree[panel_id].set_local_scale(Vector2::create(scale, scale));
/// 	let dim_paint = node_tree[dim_id].get_component_mut::<Paint>().expect("paint");
/// 	dim_paint.set_color(Color::create(0.0, 0.0, 0.0, 0.5 * popup_motion.get_dim_ratio()));
/// }
/// assert_eq!(popup_motion.get_state(), PopupMotionState::Open);
/// assert_eq!(opened_count.get(), 1);
/// assert!(popup_motion.is_interactive());
/// assert_eq!(node_tree[panel_id].get_local_scale(), Vector2::create(1.0, 1.0));
///
/// popup_motion.close();
/// popup_motion.tick(0.3);
/// assert!(!popup_motion.is_visible());
/// assert_eq!(popup_motion.get_scale(), 0.0);
/// ```
//==============================================================================
pub struct PopupMotion {
	state: PopupMotionState,
	elapsed_seconds: f64,
	open_duration: f64,
	close_duration: f64,
	is_instant_enabled: bool,
	opened_event: Option<PopupMotionEvent>,
	closed_event: Option<PopupMotionEvent>,
}

impl PopupMotion {
	//==============================================================================
	/// 생성. (기본 시간)
	///
	/// # 돌려줌
	/// 닫힌 새 전환기. (등장 0.32 초, 퇴장 0.22 초)
	//==============================================================================
	pub fn new() -> PopupMotion {
		PopupMotion::create_with_options(PopupMotionOptions::default())
	}

	//==============================================================================
	/// 설정으로 생성.
	///
	/// # 인자
	/// - `options`: 등장, 퇴장 시간.
	///
	/// # 돌려줌
	/// 닫힌 새 전환기.
	//==============================================================================
	pub fn create_with_options(options: PopupMotionOptions) -> PopupMotion {
		PopupMotion {
			state: PopupMotionState::Closed,
			elapsed_seconds: 0.0,
			open_duration: options.open_duration,
			close_duration: options.close_duration,
			is_instant_enabled: false,
			opened_event: None,
			closed_event: None,
		}
	}

	//==============================================================================
	/// 열림 알림 부르기.
	//==============================================================================
	fn fire_opened_event(&mut self) {
		if let Some(opened_event) = self.opened_event.as_mut() {
			opened_event();
		}
	}

	//==============================================================================
	/// 닫힘 알림 부르기.
	//==============================================================================
	fn fire_closed_event(&mut self) {
		if let Some(closed_event) = self.closed_event.as_mut() {
			closed_event();
		}
	}

	//==============================================================================
	/// 열기. (이미 열렸거나 열리는 중이면 그대로)
	//==============================================================================
	pub fn open(&mut self) {
		if self.state == PopupMotionState::Open || self.state == PopupMotionState::Opening {
			return;
		}
		if self.is_instant_enabled {
			self.state = PopupMotionState::Open;
			self.fire_opened_event();
			return;
		}
		self.state = PopupMotionState::Opening;
		self.elapsed_seconds = 0.0;
	}

	//==============================================================================
	/// 닫기. (이미 닫혔거나 닫히는 중이면 그대로)
	//==============================================================================
	pub fn close(&mut self) {
		if self.state == PopupMotionState::Closed || self.state == PopupMotionState::Closing {
			return;
		}
		if self.is_instant_enabled {
			self.state = PopupMotionState::Closed;
			self.fire_closed_event();
			return;
		}
		self.state = PopupMotionState::Closing;
		self.elapsed_seconds = 0.0;
	}

	//==============================================================================
	/// 갱신. (등장 / 퇴장 시간이 다 차면 다음 상태로 넘기고 알린다)
	///
	/// # 인자
	/// - `time_delta`: 지난 시간. (초, 여는 중이나 닫는 중일 때만 쌓는다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f64) {
		if self.state == PopupMotionState::Opening {
			self.elapsed_seconds += time_delta;
			if self.elapsed_seconds >= self.open_duration {
				self.state = PopupMotionState::Open;
				self.fire_opened_event();
			}
		}
		else if self.state == PopupMotionState::Closing {
			self.elapsed_seconds += time_delta;
			if self.elapsed_seconds >= self.close_duration {
				self.state = PopupMotionState::Closed;
				self.fire_closed_event();
			}
		}
	}

	//==============================================================================
	/// 지금 배율 반환.
	///
	/// # 돌려줌
	/// 판에 걸 배율. (닫힘 0, 열림 1, 여는 중 0.45 에서 1.18 을 거쳐 1, 닫는 중 1 에서 1.08 을 거쳐 0.08)
	//==============================================================================
	pub fn get_scale(&self) -> f64 {
		match self.state {
			PopupMotionState::Open => {
				1.0
			}
			PopupMotionState::Closed => {
				0.0
			}
			PopupMotionState::Opening => {
				// 0.45 에서 시작해 1.18 로 부풀었다가 1 로 자리 잡는 되튕김.
				let ratio = (self.elapsed_seconds / self.open_duration).min(1.0);
				let smooth = ratio * ratio * (3.0 - 2.0 * ratio);
				if smooth < 0.72 {
					return 0.45 + (1.18 - 0.45) * (smooth / 0.72);
				}
				1.18 - (1.18 - 1.0) * ((smooth - 0.72) / 0.28)
			}
			PopupMotionState::Closing => {
				// 1 → 1.08 로 살짝 부풀었다가 0.08 로 수축.
				let ratio = (self.elapsed_seconds / self.close_duration).min(1.0);
				if ratio < 0.25 {
					return 1.0 + (1.08 - 1.0) * (ratio / 0.25);
				}
				let shrink_ratio = (ratio - 0.25) / 0.75;
				let eased = shrink_ratio * shrink_ratio * shrink_ratio;
				1.08 - (1.08 - 0.08) * eased
			}
		}
	}

	//==============================================================================
	/// 어둡게 가리는 비율 반환. (0 ~ 1)
	///
	/// # 돌려줌
	/// 막의 알파에 곱할 비율. (0 ~ 1, 여는 동안 0 에서 1 로, 닫는 동안 1 에서 0 으로 고르게)
	//==============================================================================
	pub fn get_dim_ratio(&self) -> f64 {
		match self.state {
			PopupMotionState::Open => {
				1.0
			}
			PopupMotionState::Closed => {
				0.0
			}
			PopupMotionState::Opening => {
				(self.elapsed_seconds / self.open_duration).min(1.0)
			}
			PopupMotionState::Closing => {
				1.0 - (self.elapsed_seconds / self.close_duration).min(1.0)
			}
		}
	}

	//==============================================================================
	/// 조작 가능 여부. (완전히 열린 뒤에만 참)
	///
	/// # 돌려줌
	/// 열림 상태면 true.
	//==============================================================================
	pub fn is_interactive(&self) -> bool {
		self.state == PopupMotionState::Open
	}

	//==============================================================================
	/// 상태 반환.
	///
	/// # 돌려줌
	/// 지금 상태.
	//==============================================================================
	pub fn get_state(&self) -> PopupMotionState {
		self.state
	}

	//==============================================================================
	/// 보이는 중인지. (닫힘이 아니면 참)
	///
	/// # 돌려줌
	/// 닫힘 상태가 아니면 true.
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.state != PopupMotionState::Closed
	}

	//==============================================================================
	/// 즉시 전환 설정. (연출 없는 UI 톤)
	///
	/// # 인자
	/// - `is_instant_enabled`: 연출 없이 open, close 에서 바로 바꾸려면 true. (기본 false, 이미 여는 중이거나 닫는 중인 전환은 tick 으로 이어진다)
	//==============================================================================
	pub fn set_instant_enabled(&mut self, is_instant_enabled: bool) {
		self.is_instant_enabled = is_instant_enabled;
	}

	//==============================================================================
	/// 즉시 전환 여부.
	///
	/// # 돌려줌
	/// 즉시 전환이면 true.
	//==============================================================================
	pub fn is_instant_enabled(&self) -> bool {
		self.is_instant_enabled
	}

	//==============================================================================
	/// 열림 알림 설정.
	///
	/// # 인자
	/// - `opened_event`: 열림 상태가 될 때 부를 함수. (즉시 전환이면 open 안에서 바로 불린다)
	//==============================================================================
	pub fn set_opened_event(&mut self, opened_event: PopupMotionEvent) {
		self.opened_event = Some(opened_event);
	}

	//==============================================================================
	/// 닫힘 알림 설정.
	///
	/// # 인자
	/// - `closed_event`: 닫힘 상태가 될 때 부를 함수. (즉시 전환이면 close 안에서 바로 불린다)
	//==============================================================================
	pub fn set_closed_event(&mut self, closed_event: PopupMotionEvent) {
		self.closed_event = Some(closed_event);
	}
}

impl Default for PopupMotion {
	//==============================================================================
	/// 기본값. (PopupMotion::new 와 같다)
	//==============================================================================
	fn default() -> PopupMotion {
		PopupMotion::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	/// 열기 → 0.32 초 뒤 열림 → 닫기 → 0.22 초 뒤 닫힘. 배율과 막 비율이 원본 공식대로 움직인다.
	//==============================================================================
	#[test]
	fn open_and_close_follow_original_curve() {
		let mut popup_motion = PopupMotion::new();
		let opened_count = Rc::new(Cell::new(0));
		let closed_count = Rc::new(Cell::new(0));
		let opened_counter = opened_count.clone();
		popup_motion.set_opened_event(Box::new(move || {
			opened_counter.set(opened_counter.get() + 1);
		}));
		let closed_counter = closed_count.clone();
		popup_motion.set_closed_event(Box::new(move || {
			closed_counter.set(closed_counter.get() + 1);
		}));
		assert_eq!(popup_motion.get_scale(), 0.0);
		assert!(!popup_motion.is_visible());

		popup_motion.open();
		assert_eq!(popup_motion.get_state(), PopupMotionState::Opening);
		assert!((popup_motion.get_scale() - 0.45).abs() < 0.0001);
		assert!(!popup_motion.is_interactive());
		popup_motion.tick(0.16);
		// 진행 0.5 → smooth 0.5 → 0.45 + 0.73 * (0.5 / 0.72)
		let expected_scale = 0.45 + 0.73 * (0.5 / 0.72);
		assert!((popup_motion.get_scale() - expected_scale).abs() < 0.0001);
		assert!((popup_motion.get_dim_ratio() - 0.5).abs() < 0.0001);
		popup_motion.tick(0.16);
		assert_eq!(popup_motion.get_state(), PopupMotionState::Open);
		assert_eq!(opened_count.get(), 1);
		assert!(popup_motion.is_interactive());
		assert_eq!(popup_motion.get_scale(), 1.0);

		popup_motion.close();
		popup_motion.tick(0.055);
		// 진행 0.25 → 1.08
		assert!((popup_motion.get_scale() - 1.08).abs() < 0.001);
		popup_motion.tick(0.2);
		assert_eq!(popup_motion.get_state(), PopupMotionState::Closed);
		assert_eq!(closed_count.get(), 1);
		assert_eq!(popup_motion.get_dim_ratio(), 0.0);
	}

	//==============================================================================
	/// 즉시 전환이면 열기 / 닫기가 그 자리에서 끝나고 알림이 바로 온다.
	//==============================================================================
	#[test]
	fn instant_mode_switches_immediately() {
		let mut popup_motion = PopupMotion::new();
		let closed_count = Rc::new(Cell::new(0));
		let closed_counter = closed_count.clone();
		popup_motion.set_closed_event(Box::new(move || {
			closed_counter.set(closed_counter.get() + 1);
		}));
		popup_motion.set_instant_enabled(true);
		popup_motion.open();
		assert!(popup_motion.is_interactive());
		popup_motion.open();
		popup_motion.close();
		assert_eq!(popup_motion.get_state(), PopupMotionState::Closed);
		assert_eq!(closed_count.get(), 1);
		popup_motion.close();
		assert_eq!(closed_count.get(), 1);
	}
}
