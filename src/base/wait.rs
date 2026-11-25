//==============================================================================
//! 비동기 기다리기. (원본 base/wait.js 의 Wait.seconds, Wait.nextFrame)
//! - 엔진 작업(Engine::spawn_task, spawn_scene_task) 안에서 await 한다. 엔진은 작업을 프레임마다 한 번 폴링하므로,
//!   아직이면 스스로 깨워 다음 프레임에 다시 본다.
//! - seconds 는 원본처럼 벽시계 시간이다. (원본 setTimeout(seconds * 1000), 시간 배율(time_scale)을 곱하지 않고 멈춤에도 흐른다)
//!   부른 때부터 잰다. (원본도 seconds 를 부를 때 setTimeout 을 건다) 웹은 performance.now(), 데스크탑은 std::time 이다. (web_time::Instant)
//!   원본 setTimeout 이 0 초라도 다음 차례에 부르듯, 부른 뒤 엔진의 폴링 차례가 적어도 한 번 지나야 끝난다. (seconds(0) 은 한 번 쉬고 끝난다)
//!   시간이 이미 지난 뒤에 await 하면 원본처럼 바로 끝난다. 시간은 원본 setTimeout 처럼 밀리초를 WebIDL long(ToInt32)으로 바꾸고 음수는 0 이다.
//!   (NaN 과 무한은 0, 2^31 밀리초 이상은 2^32 로 감겨 돈다)
//! - next_frame 은 원본(requestAnimationFrame 의 Promise 를 await)처럼 다음 프레임까지 기다린다. 부른 뒤 엔진의 폴링 차례가 한 번 지나면 끝난다.
//! - 폴링 차례 번호는 스레드 하나에 하나이고, 엔진이 폴링 차례마다 올린다. (core/task.rs 의 poll_tasks)
//==============================================================================
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web_time::Instant;

thread_local! {
	/// 엔진의 폴링 차례 번호. (차례마다 하나 오른다)
	static POLL_FRAME_NUMBER: Cell<u64> = const { Cell::new(0) };
}

//==============================================================================
/// 폴링 차례 번호 올리기. (엔진이 폴링 차례 첫머리에 부른다)
//==============================================================================
pub(crate) fn advance_poll_frame() {
	POLL_FRAME_NUMBER.with(|poll_frame_number| poll_frame_number.set(poll_frame_number.get() + 1));
}

//==============================================================================
/// 지금 폴링 차례 번호.
//==============================================================================
fn read_poll_frame_number() -> u64 {
	POLL_FRAME_NUMBER.with(|poll_frame_number| poll_frame_number.get())
}

//==============================================================================
/// 시간 기다리기 퓨처. (seconds 가 돌려준다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct WaitSeconds {
	/// 끝날 시각.
	deadline_instant: Instant,
	/// 부른 때의 폴링 차례 번호. (원본 setTimeout 처럼 적어도 한 차례 뒤에 끝난다)
	created_poll_frame_number: u64,
}

impl Future for WaitSeconds {
	type Output = ();

	//==============================================================================
	/// 끝날 시각이 지났고 부른 뒤 폴링 차례가 지났는지. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut WaitSeconds>, context: &mut Context<'_>) -> Poll<()> {
		if read_poll_frame_number() > self.created_poll_frame_number && Instant::now() >= self.deadline_instant {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

//==============================================================================
/// 한 프레임 기다리기 퓨처. (next_frame 이 돌려준다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct NextFrame {
	/// 부른 때의 폴링 차례 번호.
	created_poll_frame_number: u64,
}

impl Future for NextFrame {
	type Output = ();

	//==============================================================================
	/// 부른 뒤 폴링 차례가 지났는지. (아직이면 스스로 깨워 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut NextFrame>, context: &mut Context<'_>) -> Poll<()> {
		if read_poll_frame_number() > self.created_poll_frame_number {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

//==============================================================================
/// 원본 setTimeout 의 지연 밀리초. (초 x 1000 을 WebIDL long 으로 바꾸듯 ToInt32 로 감고, 음수는 0. NaN 과 무한은 0)
//==============================================================================
fn convert_to_timeout_milliseconds(seconds: f32) -> u64 {
	let milliseconds = seconds as f64 * 1000.0;
	if !milliseconds.is_finite() {
		return 0;
	}
	let wrapped_milliseconds = milliseconds.trunc().rem_euclid(4294967296.0);
	let signed_milliseconds = if wrapped_milliseconds >= 2147483648.0 {
		wrapped_milliseconds - 4294967296.0
	}
	else {
		wrapped_milliseconds
	};
	if signed_milliseconds < 0.0 {
		return 0;
	}
	signed_milliseconds as u64
}

//==============================================================================
/// 일정 시간 동안 비동기 대기. (원본 Wait.seconds, 벽시계 초)
//==============================================================================
pub fn seconds(seconds: f32) -> WaitSeconds {
	WaitSeconds {
		deadline_instant: Instant::now() + Duration::from_millis(convert_to_timeout_milliseconds(seconds)),
		created_poll_frame_number: read_poll_frame_number(),
	}
}

//==============================================================================
/// 한 프레임 비동기 대기. (원본 Wait.nextFrame)
//==============================================================================
pub fn next_frame() -> NextFrame {
	NextFrame {
		created_poll_frame_number: read_poll_frame_number(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 원본 setTimeout 의 지연 바꾸기: 밀리초로 내림, 음수와 NaN 과 무한은 0, 2^31 밀리초 이상은 2^32 로 감긴다.
	//==============================================================================
	#[test]
	fn timeout_milliseconds_follow_set_timeout() {
		assert_eq!(convert_to_timeout_milliseconds(1.5), 1500);
		assert_eq!(convert_to_timeout_milliseconds(0.0009), 0);
		assert_eq!(convert_to_timeout_milliseconds(-1.0), 0);
		assert_eq!(convert_to_timeout_milliseconds(f32::NAN), 0);
		assert_eq!(convert_to_timeout_milliseconds(f32::INFINITY), 0);
		assert_eq!(convert_to_timeout_milliseconds(2147483.5), 2147483500);
		assert_eq!(convert_to_timeout_milliseconds(2147483.75), 0);
		assert_eq!(convert_to_timeout_milliseconds(4294968.0), 704);
	}
}
