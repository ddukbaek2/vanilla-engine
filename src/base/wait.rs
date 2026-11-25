//==============================================================================
// 비동기 기다리기. (원본 base/wait.js 의 Wait.seconds, Wait.nextFrame)
// - 엔진 작업(Engine::spawn_task, spawn_scene_task) 안에서 await 한다. 엔진은 작업을 프레임마다 한 번 폴링하므로,
//   아직이면 스스로 깨워 다음 프레임에 다시 본다.
// - seconds 는 원본처럼 벽시계 시간이다. (원본 setTimeout(seconds * 1000), 시간 배율(time_scale)을 곱하지 않고 멈춤에도 흐른다)
//   부른 때부터 잰다. (원본도 seconds 를 부를 때 setTimeout 을 건다) 웹은 performance.now(), 데스크탑은 std::time 이다. (web_time::Instant)
//   음수와 NaN, 무한은 원본 setTimeout 처럼 0 초로 친다. 시계로 나타낼 수 없을 만큼 긴 시간은 끝나지 않는다. (원본 setTimeout 은 2^31 밀리초를 넘으면 바로 부른다)
// - 원본과 다른 점:
//   원본 nextFrame 은 만든 Promise 를 기다리지 않아(await 가 빠짐) 바로 끝난다. 러스트 판은 이름대로 다음 프레임까지 기다린다.
//   원본은 await 할 때마다 마이크로태스크 한 번을 쉰다. 러스트 판 seconds(0) 은 쉬지 않고 바로 끝난다.
//==============================================================================
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web_time::Instant;

//==============================================================================
// 시간 기다리기 퓨처. (seconds 가 돌려준다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct WaitSeconds {
	// 끝날 시각. (시계로 나타낼 수 없을 만큼 길면 None 이고 끝나지 않는다)
	deadline_instant: Option<Instant>,
}

impl Future for WaitSeconds {
	type Output = ();

	//==============================================================================
	// 끝날 시각이 지났는지. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut WaitSeconds>, context: &mut Context<'_>) -> Poll<()> {
		if let Some(deadline_instant) = self.deadline_instant {
			if Instant::now() >= deadline_instant {
				return Poll::Ready(());
			}
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

//==============================================================================
// 한 프레임 기다리기 퓨처. (next_frame 이 돌려준다)
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct NextFrame {
	is_polled: bool,
}

impl Future for NextFrame {
	type Output = ();

	//==============================================================================
	// 처음 폴링은 쉬고 스스로 깨워, 엔진이 다음 프레임에 폴링할 때 끝난다.
	//==============================================================================
	fn poll(mut self: Pin<&mut NextFrame>, context: &mut Context<'_>) -> Poll<()> {
		if self.is_polled {
			return Poll::Ready(());
		}
		self.is_polled = true;
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

//==============================================================================
// 일정 시간 동안 비동기 대기. (원본 Wait.seconds, 벽시계 초)
//==============================================================================
pub fn seconds(seconds: f32) -> WaitSeconds {
	let wait_seconds = if seconds.is_finite() {
		seconds.max(0.0)
	}
	else {
		0.0
	};
	let now_instant = Instant::now();
	WaitSeconds {
		deadline_instant: Duration::try_from_secs_f32(wait_seconds).ok().and_then(|wait_duration| now_instant.checked_add(wait_duration)),
	}
}

//==============================================================================
// 한 프레임 비동기 대기. (원본 Wait.nextFrame)
//==============================================================================
pub fn next_frame() -> NextFrame {
	NextFrame {
		is_polled: false,
	}
}
