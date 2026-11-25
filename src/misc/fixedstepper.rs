//==============================================================================
// 고정 걸음.
// - 원본 misc/fixedstepper.js 를 옮긴 것이다. 프레임 간격이 들쭉날쭉해도 규칙(물리, 판의 흐름)은 늘 같은 걸음(step_seconds)으로 돌린다.
// - advance(time_delta, step_function) 가 받은 시간을 쌓아 두고 걸음만큼 찰 때마다 step_function(step_seconds) 을 부른다.
//   돌려주는 값은 남은 몫(0 ~ 1)이다. 앞 걸음과 지금 걸음 사이를 이 몫만큼 섞어 그리면 걸음이 화면 주사율과 어긋나 떨리지 않는다.
// - 한 프레임에 maximum_steps_per_frame 걸음까지만 돈다. 그만큼 돌고도 남았으면 남은 시간을 버린다.
//   (느린 기기에서 멈칫한 뒤 밀린 것을 한꺼번에 몰아 돌다 더 느려지지 않게)
// - 원본과 다른 점: 원본의 걸음 함수는 false 를 돌려줄 때만 멈추고 아무것도 돌려주지 않아도 된다. 러스트 판은 늘 bool 을 돌려준다.
//   (true 면 이어 가고, false 면 이번 프레임의 걸음을 거기서 멈추고 남은 시간은 쌓아 둔다)
//==============================================================================

// 한 걸음의 기본 시간. (초)
const DEFAULT_STEP_SECONDS: f32 = 1.0 / 60.0;

// 한 프레임에 도는 걸음의 기본 상한.
const DEFAULT_MAXIMUM_STEPS_PER_FRAME: u32 = 5;

//==============================================================================
// 고정 걸음.
//==============================================================================
#[derive(Clone, Debug)]
pub struct FixedStepper {
	step_seconds: f32,
	maximum_steps_per_frame: u32,
	accumulated_seconds: f32,
	last_step_count: u32,
}

impl FixedStepper {
	//==============================================================================
	// 생성. (한 걸음의 시간 초, 한 프레임에 도는 걸음의 상한. 잘못된 값이면 1 / 60 초, 5 걸음)
	//==============================================================================
	pub fn new(step_seconds: f32, maximum_steps_per_frame: u32) -> FixedStepper {
		let mut fixed_stepper = FixedStepper {
			step_seconds: DEFAULT_STEP_SECONDS,
			maximum_steps_per_frame: DEFAULT_MAXIMUM_STEPS_PER_FRAME,
			accumulated_seconds: 0.0,
			last_step_count: 0,
		};
		fixed_stepper.set_step_seconds(step_seconds);
		fixed_stepper.set_maximum_steps_per_frame(maximum_steps_per_frame);
		fixed_stepper
	}

	//==============================================================================
	// 한 걸음의 시간 설정. (초, 0 보다 크고 유한해야 한다. 아니면 그대로 둔다)
	//==============================================================================
	pub fn set_step_seconds(&mut self, step_seconds: f32) {
		if !step_seconds.is_finite() || step_seconds <= 0.0 {
			return;
		}
		self.step_seconds = step_seconds;
	}

	//==============================================================================
	// 한 걸음의 시간 반환. (초)
	//==============================================================================
	pub fn get_step_seconds(&self) -> f32 {
		self.step_seconds
	}

	//==============================================================================
	// 한 프레임에 도는 걸음의 상한 설정. (1 이상)
	//==============================================================================
	pub fn set_maximum_steps_per_frame(&mut self, maximum_steps_per_frame: u32) {
		self.maximum_steps_per_frame = maximum_steps_per_frame.max(1);
	}

	//==============================================================================
	// 한 프레임에 도는 걸음의 상한 반환.
	//==============================================================================
	pub fn get_maximum_steps_per_frame(&self) -> u32 {
		self.maximum_steps_per_frame
	}

	//==============================================================================
	// 나아가기. (받은 시간만큼 걸음을 돌리고 남은 몫(0 ~ 1)을 돌려준다. 음수나 유한하지 않은 시간은 0 으로 친다)
	//==============================================================================
	pub fn advance(&mut self, time_delta: f32, mut step_function: impl FnMut(f32) -> bool) -> f32 {
		let step_seconds = self.step_seconds;
		let maximum_steps_per_frame = self.maximum_steps_per_frame;
		if time_delta.is_finite() && time_delta > 0.0 {
			self.accumulated_seconds += time_delta;
		}
		let mut step_count = 0;
		let mut is_held = false;
		while self.accumulated_seconds >= step_seconds && step_count < maximum_steps_per_frame {
			let is_continued = step_function(step_seconds);
			if !is_continued {
				is_held = true;
				break;
			}
			self.accumulated_seconds -= step_seconds;
			step_count += 1;
		}
		if !is_held && step_count >= maximum_steps_per_frame && self.accumulated_seconds >= step_seconds {
			self.accumulated_seconds = 0.0;
		}
		self.last_step_count = step_count;
		self.get_alpha()
	}

	//==============================================================================
	// 남은 몫. (쌓인 시간 / 한 걸음, 0 ~ 1)
	//==============================================================================
	pub fn get_alpha(&self) -> f32 {
		(self.accumulated_seconds / self.step_seconds).clamp(0.0, 1.0)
	}

	//==============================================================================
	// 지난 advance 에서 돈 걸음 수.
	//==============================================================================
	pub fn get_last_step_count(&self) -> u32 {
		self.last_step_count
	}

	//==============================================================================
	// 쌓인 시간 비우기. (판을 새로 시작하거나 붙잡았다 풀 때)
	//==============================================================================
	pub fn reset(&mut self) {
		self.accumulated_seconds = 0.0;
		self.last_step_count = 0;
	}
}

impl Default for FixedStepper {
	fn default() -> FixedStepper {
		FixedStepper::new(DEFAULT_STEP_SECONDS, DEFAULT_MAXIMUM_STEPS_PER_FRAME)
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 걸음 수와 남은 몫, 상한에서 남은 시간 버리기, false 로 멈추기.
	//==============================================================================
	#[test]
	fn advance_steps_and_returns_alpha() {
		let mut fixed_stepper = FixedStepper::new(0.125, 3);
		let mut step_count = 0;
		let alpha = fixed_stepper.advance(0.3125, |_| {
			step_count += 1;
			true
		});
		assert_eq!(step_count, 2);
		assert!((alpha - 0.5).abs() < 1e-6);

		step_count = 0;
		let alpha = fixed_stepper.advance(1.0, |step_seconds| {
			assert_eq!(step_seconds, 0.125);
			step_count += 1;
			true
		});
		assert_eq!(step_count, 3);
		assert_eq!(alpha, 0.0);

		step_count = 0;
		let alpha = fixed_stepper.advance(0.375, |_| {
			step_count += 1;
			step_count < 2
		});
		assert_eq!(step_count, 2);
		assert_eq!(fixed_stepper.get_last_step_count(), 1);
		assert_eq!(alpha, 1.0);

		fixed_stepper.reset();
		assert_eq!(fixed_stepper.get_alpha(), 0.0);
		assert_eq!(fixed_stepper.advance(f32::NAN, |_| true), 0.0);
		let default_stepper = FixedStepper::new(-1.0, 0);
		assert_eq!(default_stepper.get_step_seconds(), DEFAULT_STEP_SECONDS);
		assert_eq!(default_stepper.get_maximum_steps_per_frame(), 1);
	}
}
