//==============================================================================
// 시간 관리자.
// - 원본은 requestAnimationFrame 의 timestamp(밀리초)를 받는다. 여기서는 엔진 시작 뒤 경과 초를 받는다.
//==============================================================================
pub struct TimeManager {
	realtimeSinceStartup: f64,
	previousFrameCheckTime: f64,
	time: f64,
	unscaledTimeDelta: f32,
	timeScale: f32,
	framePerSecond: u32,
	framesThisSecond: u32,
	hasStarted: bool,
}

impl TimeManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TimeManager {
		TimeManager {
			realtimeSinceStartup: 0.0,
			previousFrameCheckTime: 0.0,
			time: 0.0,
			unscaledTimeDelta: 0.0,
			timeScale: 1.0,
			framePerSecond: 0,
			framesThisSecond: 0,
			hasStarted: false,
		}
	}

	//==============================================================================
	// 갱신. (secondsSinceStartup: 엔진 시작 뒤 경과 초)
	//==============================================================================
	pub fn update(&mut self, secondsSinceStartup: f64) {
		// 최초 시간 처리.
		if !self.hasStarted {
			self.hasStarted = true;
			self.realtimeSinceStartup = secondsSinceStartup;
			self.previousFrameCheckTime = secondsSinceStartup;
		}

		// 시간 반영.
		// (시계 역행 시 음수, 창이 오래 멈췄다 돌아오면 거대 값이 들어올 수 있어 [0, 0.25초]로 막는다)
		let rawTimeDelta = secondsSinceStartup - self.realtimeSinceStartup;
		let unscaledTimeDelta = rawTimeDelta.clamp(0.0, 0.25);
		self.realtimeSinceStartup = secondsSinceStartup;
		self.time += unscaledTimeDelta;
		self.unscaledTimeDelta = unscaledTimeDelta as f32;

		// 프레임 계산.
		if secondsSinceStartup >= self.previousFrameCheckTime + 1.0 {
			self.framePerSecond = self.framesThisSecond;
			self.framesThisSecond = 0;
			self.previousFrameCheckTime = secondsSinceStartup;
		}

		// 프레임 증가.
		self.framesThisSecond += 1;
	}

	//==============================================================================
	// 엔진 시작 뒤 경과 시간. (초)
	//==============================================================================
	pub fn getRealtimeSinceStartup(&self) -> f64 {
		self.realtimeSinceStartup
	}

	//==============================================================================
	// 누적 게임 시간. (초, 프레임 간격 제한을 거친 값의 합)
	//==============================================================================
	pub fn getTime(&self) -> f64 {
		self.time
	}

	//==============================================================================
	// 배율을 곱하지 않은 프레임 간격. (초)
	//==============================================================================
	pub fn getUnscaleDeltaTime(&self) -> f32 {
		self.unscaledTimeDelta
	}

	//==============================================================================
	// 배율을 곱한 프레임 간격. (초)
	//==============================================================================
	pub fn getTimeDelta(&self) -> f32 {
		self.unscaledTimeDelta * self.timeScale
	}

	//==============================================================================
	// 지난 1초 동안의 프레임 수.
	//==============================================================================
	pub fn getFramePerSecond(&self) -> u32 {
		self.framePerSecond
	}

	//==============================================================================
	// 시간 배율 설정.
	//==============================================================================
	pub fn setTimeScale(&mut self, timeScale: f32) {
		self.timeScale = timeScale.max(0.0);
	}

	//==============================================================================
	// 시간 배율 반환.
	//==============================================================================
	pub fn getTimeScale(&self) -> f32 {
		self.timeScale
	}
}

impl Default for TimeManager {
	fn default() -> TimeManager {
		TimeManager::new()
	}
}
