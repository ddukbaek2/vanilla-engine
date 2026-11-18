//==============================================================================
// 쿨다운과 반복 타이머. (원본 base/timer.js 의 Cooldown, RepeatTimer)
// - 시간은 초 단위 f32 다. (엔진 TimeManager::getTimeDelta 와 같은 타입)
// - 원본 Cooldown.use() 는 러스트 예약어(use)와 겹쳐서 activate() 로 옮겼다. 뜻(준비되어 있으면 쿨다운을 시작하고 참)은 같다.
// - 원본 기본 인자(duration = 1, interval = 1, elapsedEvent = null)는 Default 와 new 의 인자로 옮겼다.
// - RepeatTimer 의 콜백은 Box<dyn FnMut()> 다. (씬의 setFirstTouchCallback 과 같은 꼴) 게임 상태를 바꾸려면
//   Rc<Cell> / Rc<RefCell> 을 잡아 넣는다.
//==============================================================================

// RepeatTimer 간격의 하한. (원본 0.000001)
const MINIMUM_INTERVAL: f32 = 0.000001;

//==============================================================================
// 쿨다운.
// - "timer -= dt; if (timer <= 0) ..." 를 매번 손으로 쓰던 것을 표준화했다.
// - activate() 로 발동하면 지정한 시간 동안 isReady() 가 거짓이 된다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cooldown {
	duration: f32,
	remainSeconds: f32,
}

impl Default for Cooldown {
	//==============================================================================
	// 1 초 쿨다운. (원본 기본 인자)
	//==============================================================================
	fn default() -> Cooldown {
		Cooldown::new(1.0)
	}
}

impl Cooldown {
	//==============================================================================
	// 생성. (duration 은 초, 0 보다 작으면 0)
	//==============================================================================
	pub fn new(duration: f32) -> Cooldown {
		Cooldown { duration: duration.max(0.0), remainSeconds: 0.0 }
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		if self.remainSeconds > 0.0 {
			self.remainSeconds = (self.remainSeconds - timeDelta).max(0.0);
		}
	}

	//==============================================================================
	// 발동 가능 여부.
	//==============================================================================
	pub fn isReady(&self) -> bool {
		self.remainSeconds <= 0.0
	}

	//==============================================================================
	// 발동한다. (원본 use, 준비되어 있으면 쿨다운을 시작하고 참을 돌려준다)
	//==============================================================================
	pub fn activate(&mut self) -> bool {
		if !self.isReady() {
			return false;
		}
		self.remainSeconds = self.duration;
		true
	}

	//==============================================================================
	// 곧바로 준비 상태로 되돌린다.
	//==============================================================================
	pub fn reset(&mut self) {
		self.remainSeconds = 0.0;
	}

	//==============================================================================
	// 쿨다운 시간 설정. (0 보다 작으면 0)
	//==============================================================================
	pub fn setDuration(&mut self, duration: f32) {
		self.duration = duration.max(0.0);
	}

	//==============================================================================
	// 쿨다운 시간 반환.
	//==============================================================================
	pub fn getDuration(&self) -> f32 {
		self.duration
	}

	//==============================================================================
	// 남은 시간 반환.
	//==============================================================================
	pub fn getRemainSeconds(&self) -> f32 {
		self.remainSeconds
	}

	//==============================================================================
	// 진행 비율 반환. (0 = 방금 발동, 1 = 준비 완료)
	//==============================================================================
	pub fn getProgress(&self) -> f32 {
		if self.duration <= 0.0 {
			return 1.0;
		}
		1.0 - self.remainSeconds / self.duration
	}
}

//==============================================================================
// 반복 타이머.
// - 일정 간격마다 콜백을 부른다. 프레임이 길어져 여러 번 지나쳤으면 밀린 만큼 몰아서 부른다.
//==============================================================================
pub struct RepeatTimer {
	interval: f32,
	elapsedSeconds: f32,
	elapsedEvent: Option<Box<dyn FnMut()>>,
	isRunning: bool,
}

impl Default for RepeatTimer {
	//==============================================================================
	// 1 초 간격, 콜백 없음. (원본 기본 인자)
	//==============================================================================
	fn default() -> RepeatTimer {
		RepeatTimer::new(1.0, None)
	}
}

impl RepeatTimer {
	//==============================================================================
	// 생성. (interval 은 초, elapsedEvent 는 간격마다 부르는 콜백, 만들자마자 돌아간다)
	//==============================================================================
	pub fn new(interval: f32, elapsedEvent: Option<Box<dyn FnMut()>>) -> RepeatTimer {
		RepeatTimer {
			interval: interval.max(MINIMUM_INTERVAL),
			elapsedSeconds: 0.0,
			elapsedEvent,
			isRunning: true,
		}
	}

	//==============================================================================
	// 갱신. (지난 간격 수만큼 콜백을 부른다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		if !self.isRunning {
			return;
		}
		self.elapsedSeconds += timeDelta;
		while self.elapsedSeconds >= self.interval {
			self.elapsedSeconds -= self.interval;
			if let Some(elapsedEvent) = self.elapsedEvent.as_mut() {
				elapsedEvent();
			}
		}
	}

	//==============================================================================
	// 시작.
	//==============================================================================
	pub fn start(&mut self) {
		self.isRunning = true;
	}

	//==============================================================================
	// 정지. (누적 시간도 비운다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.isRunning = false;
		self.elapsedSeconds = 0.0;
	}

	//==============================================================================
	// 일시 정지. (누적 시간은 남긴다)
	//==============================================================================
	pub fn pause(&mut self) {
		self.isRunning = false;
	}

	//==============================================================================
	// 간격 설정. (하한 0.000001 초)
	//==============================================================================
	pub fn setInterval(&mut self, interval: f32) {
		self.interval = interval.max(MINIMUM_INTERVAL);
	}

	//==============================================================================
	// 간격 반환.
	//==============================================================================
	pub fn getInterval(&self) -> f32 {
		self.interval
	}

	//==============================================================================
	// 콜백 설정. (None 이면 콜백 없이 시간만 흐른다)
	//==============================================================================
	pub fn setElapsedEvent(&mut self, elapsedEvent: Option<Box<dyn FnMut()>>) {
		self.elapsedEvent = elapsedEvent;
	}

	//==============================================================================
	// 동작 중 여부.
	//==============================================================================
	pub fn isRunning(&self) -> bool {
		self.isRunning
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 쿨다운: 발동, 막힘, 시간 흐름, 진행 비율, 0 으로 깎기, 음수 시간, 되돌리기.
	// (시간 값은 2 진수로 딱 떨어지는 수만 써서 같음 비교를 한다)
	//==============================================================================
	#[test]
	fn cooldownTiming() {
		let mut cooldown = Cooldown::new(0.5);
		assert!(cooldown.isReady());
		assert_eq!(cooldown.getProgress(), 1.0);
		assert!(cooldown.activate());
		assert!(!cooldown.isReady());
		assert!(!cooldown.activate());
		assert_eq!(cooldown.getRemainSeconds(), 0.5);
		assert_eq!(cooldown.getProgress(), 0.0);

		cooldown.tick(0.125);
		assert_eq!(cooldown.getRemainSeconds(), 0.375);
		assert_eq!(cooldown.getProgress(), 0.25);
		assert!(!cooldown.isReady());

		cooldown.tick(1.0);
		assert_eq!(cooldown.getRemainSeconds(), 0.0);
		assert!(cooldown.isReady());
		assert!(cooldown.activate());
		cooldown.reset();
		assert!(cooldown.isReady());

		let mut negative = Cooldown::new(-3.0);
		assert_eq!(negative.getDuration(), 0.0);
		assert_eq!(negative.getProgress(), 1.0);
		assert!(negative.activate());
		assert!(negative.isReady());

		negative.setDuration(2.0);
		assert_eq!(negative.getDuration(), 2.0);
		assert_eq!(Cooldown::default().getDuration(), 1.0);
	}

	//==============================================================================
	// 반복 타이머: 몰아서 부르기, 일시 정지(누적 유지), 정지(누적 비움), 간격 하한.
	//==============================================================================
	#[test]
	fn repeatTimerTiming() {
		let callCount = Rc::new(Cell::new(0u32));
		let countInEvent = callCount.clone();
		let mut repeatTimer = RepeatTimer::new(0.25, Some(Box::new(move || {
			countInEvent.set(countInEvent.get() + 1);
		})));
		assert!(repeatTimer.isRunning());

		repeatTimer.tick(0.125);
		assert_eq!(callCount.get(), 0);
		repeatTimer.tick(0.25);
		assert_eq!(callCount.get(), 1);
		repeatTimer.tick(0.625);
		assert_eq!(callCount.get(), 4);

		repeatTimer.tick(0.125);
		repeatTimer.pause();
		assert!(!repeatTimer.isRunning());
		repeatTimer.tick(1.0);
		assert_eq!(callCount.get(), 4);
		repeatTimer.start();
		repeatTimer.tick(0.125);
		assert_eq!(callCount.get(), 5);

		repeatTimer.tick(0.125);
		repeatTimer.stop();
		repeatTimer.start();
		repeatTimer.tick(0.125);
		assert_eq!(callCount.get(), 5);

		repeatTimer.setInterval(0.0);
		assert_eq!(repeatTimer.getInterval(), MINIMUM_INTERVAL);
		repeatTimer.setInterval(0.5);
		repeatTimer.setElapsedEvent(None);
		repeatTimer.tick(1.0);
		assert_eq!(callCount.get(), 5);
		assert_eq!(RepeatTimer::default().getInterval(), 1.0);
	}
}
