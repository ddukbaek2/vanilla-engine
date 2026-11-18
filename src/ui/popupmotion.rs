//==============================================================================
// 팝업 등장 / 퇴장 전환기. (원본 ui/popupmotion.js 의 PopupMotion, PopupMotionState)
// - closed → opening → open → closing → closed 네 상태를 오가며 배율(등장: 0.45 → 1.18 되튕김 → 1, 퇴장: 1 → 1.08 → 0.08)과
//   어둡게 가리는 비율(dimRatio)을 산출한다. 노드도 컴포넌트도 아닌 계산기라서 원본 그대로 옮겼다.
// - 매 프레임 tick(timeDelta) 뒤 getScale() / getDimRatio() 를 읽어 노드 배율 / 막 알파에 건다. setInstantEnabled(true) 면 연출 없이 바로 바뀐다.
// - 원본과 다른 것:
//   - 상태는 문자열 대신 PopupMotionState 열거형이다.
//   - options 객체는 PopupMotionOptions 구조체다. 열림 / 닫힘 알림은 인자 없는 Box<dyn FnMut()> 다.
//     (노드를 바꿔야 하는 UIDialog 는 알림 대신 tick 앞뒤의 상태를 비교해 처리한다)
//==============================================================================

//==============================================================================
// 전환 상태.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupMotionState {
	closed,
	opening,
	open,
	closing,
}

//==============================================================================
// 전환기 설정. (원본 options = { openDuration = 0.32, closeDuration = 0.22 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupMotionOptions {
	// 등장 시간. (초)
	pub openDuration: f32,
	// 퇴장 시간. (초)
	pub closeDuration: f32,
}

impl Default for PopupMotionOptions {
	//==============================================================================
	// 기본값. (등장 0.32 초, 퇴장 0.22 초)
	//==============================================================================
	fn default() -> PopupMotionOptions {
		PopupMotionOptions {
			openDuration: 0.32,
			closeDuration: 0.22,
		}
	}
}

//==============================================================================
// 알림. (인자 없음)
//==============================================================================
pub type PopupMotionEvent = Box<dyn FnMut()>;

//==============================================================================
// 팝업 전환기.
//==============================================================================
pub struct PopupMotion {
	state: PopupMotionState,
	elapsedSeconds: f32,
	openDuration: f32,
	closeDuration: f32,
	isInstantEnabled: bool,
	openedEvent: Option<PopupMotionEvent>,
	closedEvent: Option<PopupMotionEvent>,
}

impl PopupMotion {
	//==============================================================================
	// 생성. (기본 시간)
	//==============================================================================
	pub fn new() -> PopupMotion {
		PopupMotion::createWithOptions(PopupMotionOptions::default())
	}

	//==============================================================================
	// 설정으로 생성.
	//==============================================================================
	pub fn createWithOptions(options: PopupMotionOptions) -> PopupMotion {
		PopupMotion {
			state: PopupMotionState::closed,
			elapsedSeconds: 0.0,
			openDuration: options.openDuration,
			closeDuration: options.closeDuration,
			isInstantEnabled: false,
			openedEvent: None,
			closedEvent: None,
		}
	}

	//==============================================================================
	// 열림 알림 부르기.
	//==============================================================================
	fn fireOpenedEvent(&mut self) {
		if let Some(openedEvent) = self.openedEvent.as_mut() {
			openedEvent();
		}
	}

	//==============================================================================
	// 닫힘 알림 부르기.
	//==============================================================================
	fn fireClosedEvent(&mut self) {
		if let Some(closedEvent) = self.closedEvent.as_mut() {
			closedEvent();
		}
	}

	//==============================================================================
	// 열기. (이미 열렸거나 열리는 중이면 그대로)
	//==============================================================================
	pub fn open(&mut self) {
		if self.state == PopupMotionState::open || self.state == PopupMotionState::opening {
			return;
		}
		if self.isInstantEnabled {
			self.state = PopupMotionState::open;
			self.fireOpenedEvent();
			return;
		}
		self.state = PopupMotionState::opening;
		self.elapsedSeconds = 0.0;
	}

	//==============================================================================
	// 닫기. (이미 닫혔거나 닫히는 중이면 그대로)
	//==============================================================================
	pub fn close(&mut self) {
		if self.state == PopupMotionState::closed || self.state == PopupMotionState::closing {
			return;
		}
		if self.isInstantEnabled {
			self.state = PopupMotionState::closed;
			self.fireClosedEvent();
			return;
		}
		self.state = PopupMotionState::closing;
		self.elapsedSeconds = 0.0;
	}

	//==============================================================================
	// 갱신. (등장 / 퇴장 시간이 다 차면 다음 상태로 넘기고 알린다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		if self.state == PopupMotionState::opening {
			self.elapsedSeconds += timeDelta;
			if self.elapsedSeconds >= self.openDuration {
				self.state = PopupMotionState::open;
				self.fireOpenedEvent();
			}
		}
		else if self.state == PopupMotionState::closing {
			self.elapsedSeconds += timeDelta;
			if self.elapsedSeconds >= self.closeDuration {
				self.state = PopupMotionState::closed;
				self.fireClosedEvent();
			}
		}
	}

	//==============================================================================
	// 지금 배율 반환.
	//==============================================================================
	pub fn getScale(&self) -> f32 {
		match self.state {
			PopupMotionState::open => {
				1.0
			}
			PopupMotionState::closed => {
				0.0
			}
			PopupMotionState::opening => {
				// 0.45 에서 시작해 1.18 로 부풀었다가 1 로 자리 잡는 되튕김.
				let ratio = (self.elapsedSeconds / self.openDuration).min(1.0);
				let smooth = ratio * ratio * (3.0 - 2.0 * ratio);
				if smooth < 0.72 {
					return 0.45 + (1.18 - 0.45) * (smooth / 0.72);
				}
				1.18 - (1.18 - 1.0) * ((smooth - 0.72) / 0.28)
			}
			PopupMotionState::closing => {
				// 1 → 1.08 로 살짝 부풀었다가 0.08 로 수축.
				let ratio = (self.elapsedSeconds / self.closeDuration).min(1.0);
				if ratio < 0.25 {
					return 1.0 + (1.08 - 1.0) * (ratio / 0.25);
				}
				let shrinkRatio = (ratio - 0.25) / 0.75;
				let eased = shrinkRatio * shrinkRatio * shrinkRatio;
				1.08 - (1.08 - 0.08) * eased
			}
		}
	}

	//==============================================================================
	// 어둡게 가리는 비율 반환. (0 ~ 1)
	//==============================================================================
	pub fn getDimRatio(&self) -> f32 {
		match self.state {
			PopupMotionState::open => {
				1.0
			}
			PopupMotionState::closed => {
				0.0
			}
			PopupMotionState::opening => {
				(self.elapsedSeconds / self.openDuration).min(1.0)
			}
			PopupMotionState::closing => {
				1.0 - (self.elapsedSeconds / self.closeDuration).min(1.0)
			}
		}
	}

	//==============================================================================
	// 조작 가능 여부. (완전히 열린 뒤에만 참)
	//==============================================================================
	pub fn isInteractive(&self) -> bool {
		self.state == PopupMotionState::open
	}

	//==============================================================================
	// 상태 반환.
	//==============================================================================
	pub fn getState(&self) -> PopupMotionState {
		self.state
	}

	//==============================================================================
	// 보이는 중인지. (닫힘이 아니면 참)
	//==============================================================================
	pub fn isVisible(&self) -> bool {
		self.state != PopupMotionState::closed
	}

	//==============================================================================
	// 즉시 전환 설정. (연출 없는 UI 톤)
	//==============================================================================
	pub fn setInstantEnabled(&mut self, isInstantEnabled: bool) {
		self.isInstantEnabled = isInstantEnabled;
	}

	//==============================================================================
	// 즉시 전환 여부.
	//==============================================================================
	pub fn isInstantEnabled(&self) -> bool {
		self.isInstantEnabled
	}

	//==============================================================================
	// 열림 알림 설정.
	//==============================================================================
	pub fn setOpenedEvent(&mut self, openedEvent: PopupMotionEvent) {
		self.openedEvent = Some(openedEvent);
	}

	//==============================================================================
	// 닫힘 알림 설정.
	//==============================================================================
	pub fn setClosedEvent(&mut self, closedEvent: PopupMotionEvent) {
		self.closedEvent = Some(closedEvent);
	}
}

impl Default for PopupMotion {
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
	// 열기 → 0.32 초 뒤 열림 → 닫기 → 0.22 초 뒤 닫힘. 배율과 막 비율이 원본 공식대로 움직인다.
	//==============================================================================
	#[test]
	fn openAndCloseFollowOriginalCurve() {
		let mut popupMotion = PopupMotion::new();
		let openedCount = Rc::new(Cell::new(0));
		let closedCount = Rc::new(Cell::new(0));
		let openedCounter = openedCount.clone();
		popupMotion.setOpenedEvent(Box::new(move || {
			openedCounter.set(openedCounter.get() + 1);
		}));
		let closedCounter = closedCount.clone();
		popupMotion.setClosedEvent(Box::new(move || {
			closedCounter.set(closedCounter.get() + 1);
		}));
		assert_eq!(popupMotion.getScale(), 0.0);
		assert!(!popupMotion.isVisible());

		popupMotion.open();
		assert_eq!(popupMotion.getState(), PopupMotionState::opening);
		assert!((popupMotion.getScale() - 0.45).abs() < 0.0001);
		assert!(!popupMotion.isInteractive());
		popupMotion.tick(0.16);
		// 진행 0.5 → smooth 0.5 → 0.45 + 0.73 * (0.5 / 0.72)
		let expectedScale = 0.45 + 0.73 * (0.5 / 0.72);
		assert!((popupMotion.getScale() - expectedScale).abs() < 0.0001);
		assert!((popupMotion.getDimRatio() - 0.5).abs() < 0.0001);
		popupMotion.tick(0.16);
		assert_eq!(popupMotion.getState(), PopupMotionState::open);
		assert_eq!(openedCount.get(), 1);
		assert!(popupMotion.isInteractive());
		assert_eq!(popupMotion.getScale(), 1.0);

		popupMotion.close();
		popupMotion.tick(0.055);
		// 진행 0.25 → 1.08
		assert!((popupMotion.getScale() - 1.08).abs() < 0.001);
		popupMotion.tick(0.2);
		assert_eq!(popupMotion.getState(), PopupMotionState::closed);
		assert_eq!(closedCount.get(), 1);
		assert_eq!(popupMotion.getDimRatio(), 0.0);
	}

	//==============================================================================
	// 즉시 전환이면 열기 / 닫기가 그 자리에서 끝나고 알림이 바로 온다.
	//==============================================================================
	#[test]
	fn instantModeSwitchesImmediately() {
		let mut popupMotion = PopupMotion::new();
		let closedCount = Rc::new(Cell::new(0));
		let closedCounter = closedCount.clone();
		popupMotion.setClosedEvent(Box::new(move || {
			closedCounter.set(closedCounter.get() + 1);
		}));
		popupMotion.setInstantEnabled(true);
		popupMotion.open();
		assert!(popupMotion.isInteractive());
		popupMotion.open();
		popupMotion.close();
		assert_eq!(popupMotion.getState(), PopupMotionState::closed);
		assert_eq!(closedCount.get(), 1);
		popupMotion.close();
		assert_eq!(closedCount.get(), 1);
	}
}
