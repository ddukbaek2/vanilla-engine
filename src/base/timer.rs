//==============================================================================
// 쿨다운과 반복 타이머. (원본 base/timer.js 의 Cooldown, RepeatTimer)
// - 시간은 초 단위 f32 다. (엔진 TimeManager::get_time_delta 와 같은 타입)
// - 원본 Cooldown.use() 는 러스트 예약어(use)와 겹쳐서 activate() 로 옮겼다. 뜻(준비되어 있으면 쿨다운을 시작하고 참)은 같다.
// - 원본 기본 인자(duration = 1, interval = 1, elapsed_event = null)는 Default 와 new 의 인자로 옮겼다.
// - RepeatTimer 의 콜백은 Box<dyn FnMut()> 다. (씬의 set_first_touch_callback 과 같은 꼴) 게임 상태를 바꾸려면
//   Rc<Cell> / Rc<RefCell> 을 잡아 넣는다.
//==============================================================================

// RepeatTimer 간격의 하한. (원본 0.000001)
const MINIMUM_INTERVAL: f32 = 0.000001;

//==============================================================================
// 쿨다운.
// - "timer -= dt; if (timer <= 0) ..." 를 매번 손으로 쓰던 것을 표준화했다.
// - activate() 로 발동하면 지정한 시간 동안 is_ready() 가 거짓이 된다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cooldown {
	duration: f32,
	remain_seconds: f32,
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
		Cooldown { duration: duration.max(0.0), remain_seconds: 0.0 }
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		if self.remain_seconds > 0.0 {
			self.remain_seconds = (self.remain_seconds - time_delta).max(0.0);
		}
	}

	//==============================================================================
	// 발동 가능 여부.
	//==============================================================================
	pub fn is_ready(&self) -> bool {
		self.remain_seconds <= 0.0
	}

	//==============================================================================
	// 발동한다. (원본 use, 준비되어 있으면 쿨다운을 시작하고 참을 돌려준다)
	//==============================================================================
	pub fn activate(&mut self) -> bool {
		if !self.is_ready() {
			return false;
		}
		self.remain_seconds = self.duration;
		true
	}

	//==============================================================================
	// 곧바로 준비 상태로 되돌린다.
	//==============================================================================
	pub fn reset(&mut self) {
		self.remain_seconds = 0.0;
	}

	//==============================================================================
	// 쿨다운 시간 설정. (0 보다 작으면 0)
	//==============================================================================
	pub fn set_duration(&mut self, duration: f32) {
		self.duration = duration.max(0.0);
	}

	//==============================================================================
	// 쿨다운 시간 반환.
	//==============================================================================
	pub fn get_duration(&self) -> f32 {
		self.duration
	}

	//==============================================================================
	// 남은 시간 반환.
	//==============================================================================
	pub fn get_remain_seconds(&self) -> f32 {
		self.remain_seconds
	}

	//==============================================================================
	// 진행 비율 반환. (0 = 방금 발동, 1 = 준비 완료)
	//==============================================================================
	pub fn get_progress(&self) -> f32 {
		if self.duration <= 0.0 {
			return 1.0;
		}
		1.0 - self.remain_seconds / self.duration
	}
}

//==============================================================================
// 반복 타이머.
// - 일정 간격마다 콜백을 부른다. 프레임이 길어져 여러 번 지나쳤으면 밀린 만큼 몰아서 부른다.
//==============================================================================
pub struct RepeatTimer {
	interval: f32,
	elapsed_seconds: f32,
	elapsed_event: Option<Box<dyn FnMut()>>,
	is_running: bool,
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
	// 생성. (interval 은 초, elapsed_event 는 간격마다 부르는 콜백, 만들자마자 돌아간다)
	//==============================================================================
	pub fn new(interval: f32, elapsed_event: Option<Box<dyn FnMut()>>) -> RepeatTimer {
		RepeatTimer {
			interval: interval.max(MINIMUM_INTERVAL),
			elapsed_seconds: 0.0,
			elapsed_event,
			is_running: true,
		}
	}

	//==============================================================================
	// 갱신. (지난 간격 수만큼 콜백을 부른다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		if !self.is_running {
			return;
		}
		self.elapsed_seconds += time_delta;
		while self.elapsed_seconds >= self.interval {
			self.elapsed_seconds -= self.interval;
			if let Some(elapsed_event) = self.elapsed_event.as_mut() {
				elapsed_event();
			}
		}
	}

	//==============================================================================
	// 시작.
	//==============================================================================
	pub fn start(&mut self) {
		self.is_running = true;
	}

	//==============================================================================
	// 정지. (누적 시간도 비운다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.is_running = false;
		self.elapsed_seconds = 0.0;
	}

	//==============================================================================
	// 일시 정지. (누적 시간은 남긴다)
	//==============================================================================
	pub fn pause(&mut self) {
		self.is_running = false;
	}

	//==============================================================================
	// 간격 설정. (하한 0.000001 초)
	//==============================================================================
	pub fn set_interval(&mut self, interval: f32) {
		self.interval = interval.max(MINIMUM_INTERVAL);
	}

	//==============================================================================
	// 간격 반환.
	//==============================================================================
	pub fn get_interval(&self) -> f32 {
		self.interval
	}

	//==============================================================================
	// 콜백 설정. (None 이면 콜백 없이 시간만 흐른다)
	//==============================================================================
	pub fn set_elapsed_event(&mut self, elapsed_event: Option<Box<dyn FnMut()>>) {
		self.elapsed_event = elapsed_event;
	}

	//==============================================================================
	// 동작 중 여부.
	//==============================================================================
	pub fn is_running(&self) -> bool {
		self.is_running
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
	fn cooldown_timing() {
		let mut cooldown = Cooldown::new(0.5);
		assert!(cooldown.is_ready());
		assert_eq!(cooldown.get_progress(), 1.0);
		assert!(cooldown.activate());
		assert!(!cooldown.is_ready());
		assert!(!cooldown.activate());
		assert_eq!(cooldown.get_remain_seconds(), 0.5);
		assert_eq!(cooldown.get_progress(), 0.0);

		cooldown.tick(0.125);
		assert_eq!(cooldown.get_remain_seconds(), 0.375);
		assert_eq!(cooldown.get_progress(), 0.25);
		assert!(!cooldown.is_ready());

		cooldown.tick(1.0);
		assert_eq!(cooldown.get_remain_seconds(), 0.0);
		assert!(cooldown.is_ready());
		assert!(cooldown.activate());
		cooldown.reset();
		assert!(cooldown.is_ready());

		let mut negative = Cooldown::new(-3.0);
		assert_eq!(negative.get_duration(), 0.0);
		assert_eq!(negative.get_progress(), 1.0);
		assert!(negative.activate());
		assert!(negative.is_ready());

		negative.set_duration(2.0);
		assert_eq!(negative.get_duration(), 2.0);
		assert_eq!(Cooldown::default().get_duration(), 1.0);
	}

	//==============================================================================
	// 반복 타이머: 몰아서 부르기, 일시 정지(누적 유지), 정지(누적 비움), 간격 하한.
	//==============================================================================
	#[test]
	fn repeat_timer_timing() {
		let call_count = Rc::new(Cell::new(0u32));
		let count_in_event = call_count.clone();
		let mut repeat_timer = RepeatTimer::new(0.25, Some(Box::new(move || {
			count_in_event.set(count_in_event.get() + 1);
		})));
		assert!(repeat_timer.is_running());

		repeat_timer.tick(0.125);
		assert_eq!(call_count.get(), 0);
		repeat_timer.tick(0.25);
		assert_eq!(call_count.get(), 1);
		repeat_timer.tick(0.625);
		assert_eq!(call_count.get(), 4);

		repeat_timer.tick(0.125);
		repeat_timer.pause();
		assert!(!repeat_timer.is_running());
		repeat_timer.tick(1.0);
		assert_eq!(call_count.get(), 4);
		repeat_timer.start();
		repeat_timer.tick(0.125);
		assert_eq!(call_count.get(), 5);

		repeat_timer.tick(0.125);
		repeat_timer.stop();
		repeat_timer.start();
		repeat_timer.tick(0.125);
		assert_eq!(call_count.get(), 5);

		repeat_timer.set_interval(0.0);
		assert_eq!(repeat_timer.get_interval(), MINIMUM_INTERVAL);
		repeat_timer.set_interval(0.5);
		repeat_timer.set_elapsed_event(None);
		repeat_timer.tick(1.0);
		assert_eq!(call_count.get(), 5);
		assert_eq!(RepeatTimer::default().get_interval(), 1.0);
	}
}
