//==============================================================================
// 시간 관리자.
// - 원본은 request_animation_frame 의 timestamp(밀리초)를 받는다. 여기서는 엔진 시작 뒤 경과 초를 받는다.
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
		// (시계 역행 시 음수, 창이 오래 멈췄다 돌아오면 거대 값이 들어올 수 있어 [0, 0.25초]로 막는다)
		let raw_time_delta = seconds_since_startup - self.realtime_since_startup;
		let unscaled_time_delta = raw_time_delta.clamp(0.0, 0.25);
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
}

impl Default for TimeManager {
	fn default() -> TimeManager {
		TimeManager::new()
	}
}
