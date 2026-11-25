//==============================================================================
//! 최대 FPS 제한의 프레임 세기. (원본 engine.js updateEngine 의 최대 FPS 제한)
//! - 세기 시작한 때부터 처리한 프레임 수로 다음 프레임의 차례를 정하고, 차례보다 일찍 온 프레임은 건너뛴다.
//!   밀린 것은 다음 프레임에 처리하므로 주사율이 최대 FPS 로 나누어떨어지지 않아도(90 Hz 에서 60 FPS 등) 평균이 최대 FPS 가 된다.
//! - 웹 플랫폼이 request_animation_frame 의 시각(밀리초)으로 부른다.
//==============================================================================

/// 제 차례보다 일찍 온 프레임도 처리하는 여유. (밀리초) 주사율의 흔들림 때문에 제 차례보다 조금 일찍 온 프레임을 건너뛰지 않게 한다.
const FRAME_LIMIT_TOLERANCE_MILLISECONDS: f64 = 4.0;

/// 제 차례보다 이만큼 안쪽으로 일찍 온 프레임을 처리하면 그 시각부터 다시 센다. (밀리초)
/// 주사율이 최대 FPS 와 거의 같은 화면(60.05 Hz 에서 60 FPS 등)에서 조금씩 앞서 간 것이 쌓여 가끔 한 프레임을 건너뛰지 않게 한다.
const FRAME_LIMIT_RESYNC_MILLISECONDS: f64 = 1.0;

//==============================================================================
/// 프레임 세기 상태.
//==============================================================================
pub(crate) struct FrameLimiter {
	/// 프레임 수를 세기 시작한 시각. (밀리초, 아직 없으면 None)
	start_timestamp: Option<f64>,
	/// 세기 시작한 뒤 처리한 프레임 수.
	frame_count: u64,
	/// 세고 있는 최대 FPS. (바뀌면 다시 센다)
	frame_per_second: f64,
}

impl FrameLimiter {
	//==============================================================================
	/// 생성. (아직 세지 않음)
	//==============================================================================
	pub(crate) fn new() -> FrameLimiter {
		FrameLimiter {
			start_timestamp: None,
			frame_count: 0,
			frame_per_second: 0.0,
		}
	}

	//==============================================================================
	/// 이 시각의 프레임을 처리할 차례인지. (최대 FPS 가 0 이하면 늘 참)
	/// - 한 프레임 넘게 밀렸거나(창이 가려졌다 돌아옴 등) 차례와 거의 같게 왔으면 이 시각부터 다시 센다.
	///   (밀린 것을 몰아서 처리하지 않고, 조금씩 앞서 간 것이 쌓이지 않게)
	//==============================================================================
	pub(crate) fn is_frame_turn(&mut self, timestamp: f64, maximum_frame_per_second: f64) -> bool {
		if maximum_frame_per_second <= 0.0 {
			return true;
		}
		let frame_interval = 1000.0 / maximum_frame_per_second;
		let start_timestamp = match self.start_timestamp {
			Some(start_timestamp) if self.frame_per_second == maximum_frame_per_second => {
				start_timestamp
			}
			_ => {
				self.frame_count = 0;
				self.frame_per_second = maximum_frame_per_second;
				timestamp
			}
		};
		self.start_timestamp = Some(start_timestamp);
		let due_timestamp = start_timestamp + self.frame_count as f64 * frame_interval;
		let early_milliseconds = due_timestamp - timestamp;
		if early_milliseconds > FRAME_LIMIT_TOLERANCE_MILLISECONDS {
			return false;
		}
		if early_milliseconds < -frame_interval || (early_milliseconds > 0.0 && early_milliseconds <= FRAME_LIMIT_RESYNC_MILLISECONDS) {
			self.start_timestamp = Some(timestamp);
			self.frame_count = 0;
		}
		self.frame_count += 1;
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 주어진 주사율로 1 초 동안 처리한 프레임 수.
	//==============================================================================
	fn count_frames_in_one_second(refresh_rate: f64, maximum_frame_per_second: f64) -> u32 {
		let mut frame_limiter = FrameLimiter::new();
		let mut frame_count = 0;
		let refresh_interval = 1000.0 / refresh_rate;
		let mut refresh_index = 0;
		loop {
			let timestamp = refresh_index as f64 * refresh_interval;
			if timestamp >= 1000.0 {
				break;
			}
			if frame_limiter.is_frame_turn(timestamp, maximum_frame_per_second) {
				frame_count += 1;
			}
			refresh_index += 1;
		}
		frame_count
	}

	//==============================================================================
	/// 주사율이 최대 FPS 로 나누어떨어지지 않아도 평균이 최대 FPS 다. (90 Hz 에서 60 FPS 가 45 가 되지 않는다)
	//==============================================================================
	#[test]
	fn average_follows_maximum_frame_per_second() {
		assert_eq!(count_frames_in_one_second(90.0, 60.0), 60);
		assert_eq!(count_frames_in_one_second(144.0, 60.0), 60);
		assert_eq!(count_frames_in_one_second(120.0, 30.0), 30);
		assert_eq!(count_frames_in_one_second(60.05, 60.0), 61);
		assert_eq!(count_frames_in_one_second(60.0, 0.0), 60);
	}
}
