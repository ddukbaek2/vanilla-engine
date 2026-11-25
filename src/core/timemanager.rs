//==============================================================================
// 시간 관리자.
// - 원본은 request_animation_frame 의 timestamp(밀리초)를 받는다. 여기서는 엔진 시작 뒤 경과 초를 받는다.
// - 한 프레임 간격의 상한은 원본처럼 0.25 초가 기본이고 set_maximum_time_delta 로 바꾼다.
//==============================================================================
pub struct TimeManager {
	realtime_since_startup: f64,
	previous_frame_check_time: f64,
	time: f64,
	unscaled_time_delta: f32,
	time_scale: f32,
	frame_per_second: u32,
	frames_this_second: u32,
	has_started: bool,
	// 한 프레임 간격의 상한. (초)
	maximum_time_delta: f64,
}

impl TimeManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TimeManager {
		TimeManager {
			realtime_since_startup: 0.0,
			previous_frame_check_time: 0.0,
			time: 0.0,
			unscaled_time_delta: 0.0,
			time_scale: 1.0,
			frame_per_second: 0,
			frames_this_second: 0,
			has_started: false,
			maximum_time_delta: 0.25,
		}
	}

	//==============================================================================
	// 갱신. (seconds_since_startup: 엔진 시작 뒤 경과 초)
	//==============================================================================
	pub fn update(&mut self, seconds_since_startup: f64) {
		// 최초 시간 처리.
		if !self.has_started {
			self.has_started = true;
			self.realtime_since_startup = seconds_since_startup;
			self.previous_frame_check_time = seconds_since_startup;
		}

		// 시간 반영.
		// (시계 역행 시 음수, 창이 오래 멈췄다 돌아오면 거대 값이 들어올 수 있어 [0, 상한]으로 막는다. 상한은 기본 0.25초, set_maximum_time_delta)
		let raw_time_delta = seconds_since_startup - self.realtime_since_startup;
		let unscaled_time_delta = raw_time_delta.clamp(0.0, self.maximum_time_delta);
		self.realtime_since_startup = seconds_since_startup;
		self.time += unscaled_time_delta;
		self.unscaled_time_delta = unscaled_time_delta as f32;

		// 프레임 계산.
		if seconds_since_startup >= self.previous_frame_check_time + 1.0 {
			self.frame_per_second = self.frames_this_second;
			self.frames_this_second = 0;
			self.previous_frame_check_time = seconds_since_startup;
		}

		// 프레임 증가.
		self.frames_this_second += 1;
	}

	//==============================================================================
	// 엔진 시작 뒤 경과 시간. (초)
	//==============================================================================
	pub fn get_realtime_since_startup(&self) -> f64 {
		self.realtime_since_startup
	}

	//==============================================================================
	// 누적 게임 시간. (초, 프레임 간격 제한을 거친 값의 합)
	//==============================================================================
	pub fn get_time(&self) -> f64 {
		self.time
	}

	//==============================================================================
	// 배율을 곱하지 않은 프레임 간격. (초)
	//==============================================================================
	pub fn get_unscale_delta_time(&self) -> f32 {
		self.unscaled_time_delta
	}

	//==============================================================================
	// 배율을 곱한 프레임 간격. (초)
	//==============================================================================
	pub fn get_time_delta(&self) -> f32 {
		self.unscaled_time_delta * self.time_scale
	}

	//==============================================================================
	// 지난 1초 동안의 프레임 수.
	//==============================================================================
	pub fn get_frame_per_second(&self) -> u32 {
		self.frame_per_second
	}

	//==============================================================================
	// 시간 배율 설정.
	//==============================================================================
	pub fn set_time_scale(&mut self, time_scale: f32) {
		self.time_scale = time_scale.max(0.0);
	}

	//==============================================================================
	// 시간 배율 반환.
	//==============================================================================
	pub fn get_time_scale(&self) -> f32 {
		self.time_scale
	}

	//==============================================================================
	// 한 프레임 간격의 상한 설정. (초, 기본 0.25. 0 보다 작거나 유한하지 않으면 0.25 로 둔다)
	// - 창이 가려졌다 돌아오거나 오래 멈춘 뒤의 첫 프레임 간격을 이 값으로 자른다. 한 번에 크게 나아가면 안 되는 게임은 0.1, 0.05 처럼 줄인다.
	//==============================================================================
	pub fn set_maximum_time_delta(&mut self, seconds: f32) {
		if !seconds.is_finite() || seconds < 0.0 {
			self.maximum_time_delta = 0.25;
			return;
		}
		self.maximum_time_delta = seconds as f64;
	}

	//==============================================================================
	// 한 프레임 간격의 상한 반환. (초)
	//==============================================================================
	pub fn get_maximum_time_delta(&self) -> f32 {
		self.maximum_time_delta as f32
	}
}

impl Default for TimeManager {
	fn default() -> TimeManager {
		TimeManager::new()
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 프레임 간격은 기본 0.25 초, 바꾼 상한에서 잘린다.
	//==============================================================================
	#[test]
	fn time_delta_is_clamped_by_maximum() {
		let mut time_manager = TimeManager::new();
		time_manager.update(1.0);
		time_manager.update(3.0);
		assert_eq!(time_manager.get_unscale_delta_time(), 0.25);
		time_manager.set_maximum_time_delta(0.05);
		time_manager.update(4.0);
		assert_eq!(time_manager.get_unscale_delta_time(), 0.05);
		time_manager.update(4.01);
		assert!((time_manager.get_unscale_delta_time() - 0.01).abs() < 1e-6);
		time_manager.set_maximum_time_delta(f32::NAN);
		assert_eq!(time_manager.get_maximum_time_delta(), 0.25);
	}
}
