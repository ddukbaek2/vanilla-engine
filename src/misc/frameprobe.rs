//==============================================================================
//! 처지는 기기 가리기. (vanilla.js misc/frameprobe.js 의 FrameProbe)
//! - 처음 몇 초의 프레임 간격을 재어, 무거운 효과를 끄는 가벼운 길을 저절로 켤 때 쓴다.
//! - 재기 시작하고 기다리는 초가 지난 뒤 재는 초 동안 매 프레임의 간격을 본다. (처음 몇 초는 모형과 그림을 받느라 끊긴다)
//! - 목표 간격(최대 프레임의 한 칸, 무제한이면 60 프레임의 한 칸)의 처진 배를 넘긴 프레임이 처진 몫 이상이면 처지는 기기다.
//!   - 한 칸이 아니라 1.4 배로 보는 까닭: 90 Hz 화면은 최대 60 에서 엔진이 한 장 걸러 그려 늘 22 ms 간격이고, 그것은 기기가 처지는 것이 아니다.
//!   - 60 Hz 와 120 Hz 화면에서 한 장을 놓친 프레임(33 ms, 25 ms)은 처진 것으로 센다. 넷에 하나 꼴이면 48 fps 밑이다.
//! - 처지는 기기로 정하면 다시 재지 않는다. 다 재기 전에 창이 가려지면 restart 로 처음부터 다시 잰다.
//!   처지지 않았으면 무거운 장면에서 measure_again 으로 한 번 더 잴 수 있다.
//! - 엔진을 들여오지 않는다. 프레임 간격과 최대 프레임은 부르는 쪽이 넘긴다.
//==============================================================================

//==============================================================================
/// 처지는 기기 가리기. (처음 몇 초의 프레임 간격으로 무거운 효과를 끌지 정한다)
/// - 처음 기다리는 초 2, 재는 초 4, 처진 배 1.4, 처진 몫 0.25, 무제한일 때 목표 60 프레임.
///
/// # 예제
/// 세 프레임에 하나씩 끊기는 기기는 처지는 기기로 정한다
/// ```rust
/// use vanilla::FrameProbe;
///
/// let mut frame_probe = FrameProbe::new();
/// let mut frame_index = 0;
/// let mut is_done = false;
/// while !is_done {
/// 	let frame_seconds = if frame_index % 3 == 0 {
/// 		1.0 / 20.0
/// 	}
/// 	else {
/// 		1.0 / 60.0
/// 	};
/// 	is_done = frame_probe.measure_frame(frame_seconds, 60.0);
/// 	frame_index += 1;
/// }
/// assert!(frame_probe.is_finished());
/// assert!(frame_probe.is_slow());
/// assert!(frame_probe.read_slow_frame_share() >= 0.25);
/// ```
///
/// 씬에서 다 잰 프레임에 한 번 가벼운 길을 켜기
/// ```rust,no_run
/// use vanilla::{Engine, FrameProbe};
///
/// fn tick_frame_probe(frame_probe: &mut FrameProbe, engine: &Engine, is_light_mode: &mut bool) {
/// 	let frame_seconds = engine.get_time_manager().get_unscale_delta_time();
/// 	let maximum_frame_per_second = engine.get_maximum_frame_per_second();
/// 	if frame_probe.measure_frame(frame_seconds, maximum_frame_per_second) && frame_probe.is_slow() {
/// 		*is_light_mode = true;
/// 	}
/// }
/// ```
//==============================================================================
pub struct FrameProbe {
	warmup_seconds: f32,
	probe_seconds: f32,
	slow_frame_ratio: f32,
	slow_frame_share: f32,
	unlimited_target_frame_per_second: f32,
	elapsed_seconds: f32,
	frame_count: u32,
	slow_frame_count: u32,
	is_finished: bool,
	is_slow: bool,
}

impl FrameProbe {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 아직 재지 않은 새 재기. (기다리는 초 2, 재는 초 4, 처진 배 1.4, 처진 몫 0.25, 무제한 목표 60)
	//==============================================================================
	pub fn new() -> FrameProbe {
		FrameProbe {
			warmup_seconds: 2.0,
			probe_seconds: 4.0,
			slow_frame_ratio: 1.4,
			slow_frame_share: 0.25,
			unlimited_target_frame_per_second: 60.0,
			elapsed_seconds: 0.0,
			frame_count: 0,
			slow_frame_count: 0,
			is_finished: false,
			is_slow: false,
		}
	}

	//==============================================================================
	/// 한 프레임 재기. (다 재는 프레임에 true 를 돌려준다)
	/// - 흐른 시간이 기다리는 초 이하인 프레임은 세지 않는다. 처진 간격은 처진 배 / 목표 프레임(초)이고, 그보다 긴 프레임을 처진 것으로 센다.
	///
	/// # 인자
	/// - `frame_seconds`: 지난 프레임부터 흐른 시간. (초, 0 이하이거나 NaN 이면 무시)
	/// - `maximum_frame_per_second`: 엔진의 최대 프레임. (0 이하면 무제한으로 보고 set_unlimited_target_frame_per_second 의 목표를 쓴다)
	///
	/// # 돌려줌
	/// 이 프레임으로 재기가 끝났으면 true. 아직이거나 이미 끝났으면 false.
	///
	/// # 예제
	/// 고른 60 프레임은 처지지 않는다
	/// ```rust
	/// use vanilla::FrameProbe;
	///
	/// let mut frame_probe = FrameProbe::new();
	/// frame_probe.set_seconds(0.5, 1.0);
	/// let mut frame_count = 0;
	/// while !frame_probe.measure_frame(1.0 / 60.0, 60.0) {
	/// 	frame_count += 1;
	/// }
	/// assert!(frame_count >= 89);
	/// assert!(!frame_probe.is_slow());
	/// assert!(!frame_probe.measure_frame(1.0 / 60.0, 60.0));
	/// ```
	//==============================================================================
	pub fn measure_frame(&mut self, frame_seconds: f32, maximum_frame_per_second: f32) -> bool {
		if self.is_finished || !(frame_seconds > 0.0) {
			return false;
		}
		self.elapsed_seconds += frame_seconds;
		if self.elapsed_seconds <= self.warmup_seconds {
			return false;
		}
		let target_frame_per_second = if maximum_frame_per_second > 0.0 {
			maximum_frame_per_second
		}
		else {
			self.unlimited_target_frame_per_second
		};
		let slow_frame_seconds = self.slow_frame_ratio / target_frame_per_second;
		self.frame_count += 1;
		if frame_seconds > slow_frame_seconds {
			self.slow_frame_count += 1;
		}
		if self.elapsed_seconds < self.warmup_seconds + self.probe_seconds {
			return false;
		}
		self.is_finished = true;
		self.is_slow = self.slow_frame_count as f32 >= self.frame_count as f32 * self.slow_frame_share;
		true
	}

	//==============================================================================
	/// 처음부터 다시 재기. (다 재기 전에 창이 가려졌을 때, 다 잰 뒤면 그대로 둔다)
	//==============================================================================
	pub fn restart(&mut self) {
		if self.is_finished {
			return;
		}
		self.elapsed_seconds = 0.0;
		self.frame_count = 0;
		self.slow_frame_count = 0;
	}

	//==============================================================================
	/// 다 잰 재기를 한 번 더 열기. (처지지 않았을 때만, 기다리는 초부터 처음처럼)
	//==============================================================================
	pub fn measure_again(&mut self) {
		if !self.is_finished || self.is_slow {
			return;
		}
		self.is_finished = false;
		self.elapsed_seconds = 0.0;
		self.frame_count = 0;
		self.slow_frame_count = 0;
	}

	//==============================================================================
	/// 다 쟀는지 반환.
	///
	/// # 돌려줌
	/// 재기가 끝났으면 true. (measure_again 으로 다시 열면 false)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.is_finished
	}

	//==============================================================================
	/// 처지는 기기인지 반환. (다 재기 전에는 false)
	///
	/// # 돌려줌
	/// 다 잰 결과 처진 프레임 몫이 처진 몫 이상이었으면 true.
	//==============================================================================
	pub fn is_slow(&self) -> bool {
		self.is_slow
	}

	//==============================================================================
	/// 지금까지의 처진 프레임 몫 반환. (잰 프레임이 없으면 0)
	///
	/// # 돌려줌
	/// 처진 프레임 수 / 잰 프레임 수. (0 ~ 1)
	//==============================================================================
	pub fn read_slow_frame_share(&self) -> f32 {
		if self.frame_count > 0 {
			self.slow_frame_count as f32 / self.frame_count as f32
		}
		else {
			0.0
		}
	}

	//==============================================================================
	/// 기다리는 초와 재는 초 설정. (재기 전에 정한다)
	///
	/// # 인자
	/// - `warmup_seconds`: 재기 전에 기다리는 시간. (초, 0 보다 작으면 0, 처음 2)
	/// - `probe_seconds`: 기다린 뒤 재는 시간. (초, 0 보다 작으면 0, 처음 4)
	//==============================================================================
	pub fn set_seconds(&mut self, warmup_seconds: f32, probe_seconds: f32) {
		self.warmup_seconds = warmup_seconds.max(0.0);
		self.probe_seconds = probe_seconds.max(0.0);
	}

	//==============================================================================
	/// 기다리는 초 반환.
	///
	/// # 돌려줌
	/// 재기 전에 기다리는 시간. (초)
	//==============================================================================
	pub fn get_warmup_seconds(&self) -> f32 {
		self.warmup_seconds
	}

	//==============================================================================
	/// 재는 초 반환.
	///
	/// # 돌려줌
	/// 기다린 뒤 재는 시간. (초)
	//==============================================================================
	pub fn get_probe_seconds(&self) -> f32 {
		self.probe_seconds
	}

	//==============================================================================
	/// 처진 프레임의 판정 설정. (목표 간격의 배, 처지는 기기로 볼 몫)
	///
	/// # 인자
	/// - `slow_frame_ratio`: 목표 간격의 몇 배보다 길면 처진 프레임으로 셀지. (처음 1.4)
	/// - `slow_frame_share`: 처진 프레임이 잰 프레임의 이 몫 이상이면 처지는 기기다. (0 ~ 1, 처음 0.25)
	//==============================================================================
	pub fn set_slow_frame_threshold(&mut self, slow_frame_ratio: f32, slow_frame_share: f32) {
		self.slow_frame_ratio = slow_frame_ratio;
		self.slow_frame_share = slow_frame_share;
	}

	//==============================================================================
	/// 처진 프레임으로 셀 간격의 배 반환.
	///
	/// # 돌려줌
	/// 목표 간격에 곱하는 배. (처음 1.4)
	//==============================================================================
	pub fn get_slow_frame_ratio(&self) -> f32 {
		self.slow_frame_ratio
	}

	//==============================================================================
	/// 처지는 기기로 볼 처진 프레임의 몫 반환.
	///
	/// # 돌려줌
	/// 처진 몫. (0 ~ 1, 처음 0.25)
	//==============================================================================
	pub fn get_slow_frame_share(&self) -> f32 {
		self.slow_frame_share
	}

	//==============================================================================
	/// 최대 프레임이 무제한일 때의 목표 설정.
	///
	/// # 인자
	/// - `unlimited_target_frame_per_second`: measure_frame 에 0 이하의 최대 프레임이 오면 쓸 목표 프레임. (초당 프레임, 처음 60)
	//==============================================================================
	pub fn set_unlimited_target_frame_per_second(&mut self, unlimited_target_frame_per_second: f32) {
		self.unlimited_target_frame_per_second = unlimited_target_frame_per_second;
	}

	//==============================================================================
	/// 최대 프레임이 무제한일 때의 목표 반환.
	///
	/// # 돌려줌
	/// 목표 프레임. (초당 프레임)
	//==============================================================================
	pub fn get_unlimited_target_frame_per_second(&self) -> f32 {
		self.unlimited_target_frame_per_second
	}
}

impl Default for FrameProbe {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> FrameProbe {
		FrameProbe::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 90 Hz 화면에서 최대 60 이라 늘 22 ms 간격인 것은 처진 것이 아니다.
	//==============================================================================
	#[test]
	fn steady_ninety_hertz_is_not_slow() {
		let mut frame_probe = FrameProbe::new();
		let mut is_done = false;
		while !is_done {
			is_done = frame_probe.measure_frame(1.0 / 45.0, 60.0);
		}
		assert!(!frame_probe.is_slow());
	}

	//==============================================================================
	/// 세 프레임에 하나가 50 ms 로 끊기면 처지는 기기다.
	//==============================================================================
	#[test]
	fn dropped_frames_are_slow() {
		let mut frame_probe = FrameProbe::new();
		let mut frame_index = 0;
		let mut is_done = false;
		while !is_done {
			let frame_seconds = if frame_index % 3 == 0 {
				1.0 / 20.0
			}
			else {
				1.0 / 60.0
			};
			is_done = frame_probe.measure_frame(frame_seconds, 60.0);
			frame_index += 1;
		}
		assert!(frame_probe.is_slow());
	}
}
