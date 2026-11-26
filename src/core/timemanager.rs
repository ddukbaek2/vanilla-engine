//==============================================================================
//! 시간 관리자. (원본 core/timemanager.js)
//! - 엔진이 프레임마다 경과 초를 넣고, 누적 시간, 프레임 간격, 초당 프레임 수, 시간 배율을 센다.
//! - 씬에서는 engine.get_time_manager() 로 읽고 engine.get_time_manager_mut() 로 배율과 상한을 바꾼다.
//==============================================================================

//==============================================================================
/// 시간 관리자.
/// - 원본은 request_animation_frame 의 timestamp(밀리초)를 받는다. 여기서는 엔진 시작 뒤 경과 초를 받는다.
/// - 한 프레임 간격의 상한은 원본처럼 0.25 초가 기본이고 set_maximum_time_delta 로 바꾼다.
///
/// # 예제
/// 경과 초를 넣고 프레임 간격과 배율 읽기
/// ```rust
/// use vanilla::TimeManager;
///
/// let mut time_manager = TimeManager::new();
/// time_manager.update(0.0);
/// time_manager.update(0.1);
/// assert!((time_manager.get_unscale_delta_time() - 0.1).abs() < 1e-6);
/// time_manager.set_time_scale(0.5);
/// assert!((time_manager.get_time_delta() - 0.05).abs() < 1e-6);
/// time_manager.update(2.0);
/// assert_eq!(time_manager.get_unscale_delta_time(), 0.25);
/// assert!((time_manager.get_time() - 0.35).abs() < 1e-9);
/// ```
///
/// 씬에서 느리게 돌리고 프레임 간격 읽기
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, Scene, SceneBase};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn create(&mut self, engine: &mut Engine) {
/// 		let time_manager = engine.get_time_manager_mut();
/// 		time_manager.set_time_scale(0.5);
/// 		time_manager.set_maximum_time_delta(0.1);
/// 	}
///
/// 	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
/// 		self.scene_base.tick(engine, time_delta);
/// 		let time_manager = engine.get_time_manager();
/// 		let unscaled_time_delta = time_manager.get_unscale_delta_time();
/// 		let frame_per_second = time_manager.get_frame_per_second();
/// 		println!("{} {} {}", time_delta, unscaled_time_delta, frame_per_second);
/// 	}
/// }
///
/// let engine_configuration = EngineConfiguration::new();
/// let engine = Engine::new(engine_configuration);
/// let main_scene = MainScene {
/// 	scene_base: SceneBase::new(),
/// };
/// engine.run(Box::new(main_scene)).unwrap();
/// ```
//==============================================================================
pub struct TimeManager {
	realtime_since_startup: f64,
	previous_frame_check_time: f64,
	time: f64,
	unscaled_time_delta: f64,
	time_scale: f64,
	frame_per_second: u32,
	frames_this_second: u32,
	has_started: bool,
	/// 한 프레임 간격의 상한. (초)
	maximum_time_delta: f64,
}

impl TimeManager {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 시간 관리자. (시간 0, 배율 1, 상한 0.25 초, 첫 update 를 기다린다)
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
	/// 갱신. (seconds_since_startup: 엔진 시작 뒤 경과 초)
	/// - 첫 호출은 그 시각을 시작으로 삼아 간격 0 이다. 그 뒤로는 지난 호출과의 차를 0 ~ 상한으로 잘라 누적 시간에 더한다.
	/// - 지난 초당 프레임 수 확정 뒤 1 초가 지나면 그동안 센 프레임 수를 초당 프레임 수로 확정한다.
	///
	/// # 인자
	/// - `seconds_since_startup`: 엔진 시작 뒤 경과 시간. (초, 프레임마다 늘어나는 값)
	///
	/// # 예제
	/// 첫 호출은 간격 0, 큰 간격은 상한에서 잘린다
	/// ```rust
	/// use vanilla::TimeManager;
	///
	/// let mut time_manager = TimeManager::new();
	/// time_manager.update(10.0);
	/// assert_eq!(time_manager.get_unscale_delta_time(), 0.0);
	/// time_manager.update(15.0);
	/// assert_eq!(time_manager.get_unscale_delta_time(), 0.25);
	/// assert_eq!(time_manager.get_realtime_since_startup(), 15.0);
	/// ```
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
		self.unscaled_time_delta = unscaled_time_delta;

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
	/// 엔진 시작 뒤 경과 시간. (초)
	///
	/// # 돌려줌
	/// 마지막 update 에 넣은 경과 초. (update 전에는 0)
	//==============================================================================
	pub fn get_realtime_since_startup(&self) -> f64 {
		self.realtime_since_startup
	}

	//==============================================================================
	/// 누적 게임 시간. (초, 프레임 간격 제한을 거친 값의 합)
	///
	/// # 돌려줌
	/// 누적 시간. (초, 배율을 곱하지 않는다)
	//==============================================================================
	pub fn get_time(&self) -> f64 {
		self.time
	}

	//==============================================================================
	/// 배율을 곱하지 않은 프레임 간격. (초)
	///
	/// # 돌려줌
	/// 이번 프레임 간격. (초, 0 ~ 상한)
	//==============================================================================
	pub fn get_unscale_delta_time(&self) -> f64 {
		self.unscaled_time_delta
	}

	//==============================================================================
	/// 배율을 곱한 프레임 간격. (초)
	///
	/// # 돌려줌
	/// 이번 프레임 간격 곱하기 시간 배율. (초, 배율이 음수면 음수)
	//==============================================================================
	pub fn get_time_delta(&self) -> f64 {
		self.unscaled_time_delta * self.time_scale
	}

	//==============================================================================
	/// 지난 1초 동안의 프레임 수.
	///
	/// # 돌려줌
	/// 초당 프레임 수. (첫 1 초가 지나기 전에는 0)
	//==============================================================================
	pub fn get_frame_per_second(&self) -> u32 {
		self.frame_per_second
	}

	//==============================================================================
	/// 시간 배율 설정. (원본처럼 받은 값 그대로, 음수면 get_time_delta 도 음수다)
	///
	/// # 인자
	/// - `time_scale`: 시간 배율. (1 이면 그대로, 0.5 면 절반 속도, 0 이면 멈춤, 기본 1)
	//==============================================================================
	pub fn set_time_scale(&mut self, time_scale: f64) {
		self.time_scale = time_scale;
	}

	//==============================================================================
	/// 시간 배율 반환.
	///
	/// # 돌려줌
	/// 지금 시간 배율. (기본 1)
	//==============================================================================
	pub fn get_time_scale(&self) -> f64 {
		self.time_scale
	}

	//==============================================================================
	/// 한 프레임 간격의 상한 설정. (초, 기본 0.25. 0 보다 작거나 유한하지 않으면 0.25 로 둔다)
	/// - 창이 가려졌다 돌아오거나 오래 멈춘 뒤의 첫 프레임 간격을 이 값으로 자른다. 한 번에 크게 나아가면 안 되는 게임은 0.1, 0.05 처럼 줄인다.
	///
	/// # 인자
	/// - `seconds`: 한 프레임 간격의 상한. (초, 0 이상의 유한한 값)
	///
	/// # 예제
	/// 상한을 줄이고, 잘못된 값은 기본값으로
	/// ```rust
	/// use vanilla::TimeManager;
	///
	/// let mut time_manager = TimeManager::new();
	/// time_manager.set_maximum_time_delta(0.05);
	/// assert_eq!(time_manager.get_maximum_time_delta(), 0.05);
	/// time_manager.set_maximum_time_delta(-1.0);
	/// assert_eq!(time_manager.get_maximum_time_delta(), 0.25);
	/// ```
	//==============================================================================
	pub fn set_maximum_time_delta(&mut self, seconds: f64) {
		if !seconds.is_finite() || seconds < 0.0 {
			self.maximum_time_delta = 0.25;
			return;
		}
		self.maximum_time_delta = seconds;
	}

	//==============================================================================
	/// 한 프레임 간격의 상한 반환. (초)
	///
	/// # 돌려줌
	/// 지금 상한. (초, 기본 0.25)
	//==============================================================================
	pub fn get_maximum_time_delta(&self) -> f64 {
		self.maximum_time_delta
	}
}

impl Default for TimeManager {
	//==============================================================================
	/// 기본값. (TimeManager::new 와 같다)
	//==============================================================================
	fn default() -> TimeManager {
		TimeManager::new()
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 프레임 간격은 기본 0.25 초, 바꾼 상한에서 잘린다.
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
		time_manager.set_maximum_time_delta(f64::NAN);
		assert_eq!(time_manager.get_maximum_time_delta(), 0.25);
		time_manager.set_time_scale(-2.0);
		assert_eq!(time_manager.get_time_scale(), -2.0);
		assert!((time_manager.get_time_delta() + 0.02).abs() < 1e-6);
	}
}
