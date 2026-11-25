//==============================================================================
// 웨이랜드 창의 프레임 콜백. (platform/desktop.rs 가 부른다)
// - 페이지 가려짐: 크롬 리눅스 웨이랜드가 visibilitychange 를 내는 방법을 옮겼다.
//   (ui/ozone/platform/wayland/host/wayland_toplevel_window.cc MaybeUpdateOcclusionState, wayland_frame_manager.cc, ui/ozone/common/features.cc kWaylandWindowOcclusion 기본 켬)
//   크롬은 xdg_toplevel 의 suspended 상태이거나, 보낸 wl_surface.frame 콜백이 3 초 동안 오지 않으면 창을 가려짐(kOccluded)으로 본다.
//   가려진 창은 aura 가림 추적기와 WebContentsViewAura 를 거쳐 Visibility::OCCLUDED 가 되고 페이지는 hidden 이다. (web_contents_impl.cc CalculatePageVisibilityState)
//   wl_surface.frame 규약은 "보이지 않는 표면(화면 밖, 다른 창에 다 가림)에는 콜백을 보내지 않는 것이 좋다"고 하고, 크롬은 이 갈래를 모든 컴포지터에서 쓴다.
//   3 초는 크롬이 머터가 suspended 를 보내기까지 기다리는 시간에 맞춘 값이다. (kFrameCallbackOcclusionTimeout)
// - 엔진도 같은 콜백을 받는다. winit 창의 wl_display 와 wl_surface 를 raw-window-handle 로 얻어, wayland-client 를 손님(guest) 연결로 붙이고
//   제 이벤트 큐에서 wl_surface.frame 을 보낸다. (wayland-backend 의 손님 연결은 자식 객체를 래퍼로 제 큐에 단다)
//   소켓은 winit 이벤트 루프가 읽고, 엔진은 프레임마다 제 큐만 비운다.
// - suspended 상태는 받지 못한다. xdg_toplevel 은 winit(sctk)이 리스너를 달아 쥐고 있고, libwayland 는 프록시 하나에 리스너 하나만 단다.
//   winit 0.30 은 configure 의 suspended 를 받아 두기만 하고 알리지 않는다. (winit 0.31 베타 4 와 본줄기도 같다)
// - 수직 동기: 메사의 웨이랜드 EGL 은 스왑 간격이 1 이면 eglSwapBuffers 안에서 지난 프레임 콜백을 시간 제한 없이 기다린다.
//   (src/egl/drivers/dri2/platform_wayland.c throttle, loader_wayland_dispatch 에 끝 시각이 없으면 wl_display_dispatch_queue)
//   가려진 창에 콜백이 오지 않으면 주 스레드가 거기서 멈춰 가려짐도 알릴 수 없다. 그래서 SDL 처럼 EGL 스왑 간격을 0 으로 두고
//   콜백 기다림을 엔진이 맡는다. 지난 프레임 콜백을 1/20 초까지 기다리고, 안 오면 그대로 그린다. (SDL src/video/wayland Wayland_WaitForSwapInterval 의 20Hz)
//   콜백은 크롬처럼 하나만 띄워 둔다.
// - 가르는 부분(FrameCallbackOcclusion)은 다른 OS 에서도 시험으로 돈다.
//==============================================================================
use web_time::Instant;

// 프레임 콜백이 이만큼 오지 않으면 가려짐이다. (초, 크롬 kFrameCallbackOcclusionTimeout)
const FRAME_CALLBACK_OCCLUSION_SECONDS: f64 = 3.0;

//==============================================================================
// 프레임 콜백으로 본 가려짐. (크롬 WaylandFrameManager 의 frame_callback_occlusion_timer_)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FrameCallbackOcclusion {
	// 띄워 둔 콜백을 보낸 때. (없으면 None)
	requested_instant: Option<Instant>,
	is_occluded: bool,
}

impl FrameCallbackOcclusion {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub(super) fn new() -> FrameCallbackOcclusion {
		FrameCallbackOcclusion {
			requested_instant: None,
			is_occluded: false,
		}
	}

	//==============================================================================
	// 띄워 둔 콜백이 있는지.
	//==============================================================================
	pub(super) fn is_callback_pending(&self) -> bool {
		self.requested_instant.is_some()
	}

	//==============================================================================
	// 콜백을 보냈다. (띄워 둔 것이 없을 때만 보낸다)
	//==============================================================================
	pub(super) fn note_requested(&mut self, now: Instant) {
		if self.requested_instant.is_none() {
			self.requested_instant = Some(now);
		}
	}

	//==============================================================================
	// 콜백이 왔다. (보이므로 가려짐을 푼다, 크롬 ResetFrameCallback 의 SetFrameCallbackOccluded(false))
	//==============================================================================
	pub(super) fn note_done(&mut self) {
		self.requested_instant = None;
		self.is_occluded = false;
	}

	//==============================================================================
	// 가려졌는지. (띄워 둔 콜백이 3 초 넘게 오지 않았으면 가려짐, 크롬 FrameCallbackOcclusionTimeout)
	//==============================================================================
	pub(super) fn update(&mut self, now: Instant) -> bool {
		if let Some(requested_instant) = self.requested_instant {
			if now.duration_since(requested_instant).as_secs_f64() >= FRAME_CALLBACK_OCCLUSION_SECONDS {
				self.is_occluded = true;
			}
		}
		self.is_occluded
	}
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
pub(super) use self::wayland::WaylandFramePacer;

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
mod wayland {
	use super::FrameCallbackOcclusion;
	use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
	use std::os::fd::AsRawFd;
	use std::time::Duration;
	use wayland_client::backend::{Backend, ObjectId};
	use wayland_client::protocol::wl_callback::{self, WlCallback};
	use wayland_client::protocol::wl_surface::WlSurface;
	use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
	use web_time::Instant;

	// 수직 동기로 지난 프레임 콜백을 기다리는 가장 긴 시간. (밀리초, SDL 의 20Hz)
	const VERTICAL_SYNC_WAIT_MILLISECONDS: u64 = 50;
	// poll 의 읽을 것 있음. (POSIX POLLIN)
	const POLL_IN: std::os::raw::c_short = 0x0001;

	// poll 의 fd 수 형. (리눅스는 unsigned long, BSD 는 unsigned int)
	#[cfg(target_os = "linux")]
	type PollCount = std::os::raw::c_ulong;
	#[cfg(not(target_os = "linux"))]
	type PollCount = std::os::raw::c_uint;

	//==============================================================================
	// poll 의 fd 하나. (POSIX struct pollfd)
	//==============================================================================
	#[repr(C)]
	struct PollDescriptor {
		descriptor: std::os::raw::c_int,
		events: std::os::raw::c_short,
		returned_events: std::os::raw::c_short,
	}

	extern "C" {
		fn poll(descriptor_list: *mut PollDescriptor, descriptor_count: PollCount, timeout_milliseconds: std::os::raw::c_int) -> std::os::raw::c_int;
	}

	//==============================================================================
	// 큐가 받는 상태.
	//==============================================================================
	struct WaylandFrameState {
		occlusion: FrameCallbackOcclusion,
	}

	impl Dispatch<WlCallback, ()> for WaylandFrameState {
		//==============================================================================
		// 프레임 콜백이 왔다.
		//==============================================================================
		fn event(state: &mut WaylandFrameState, _callback: &WlCallback, event: wl_callback::Event, _data: &(), _connection: &Connection, _queue_handle: &QueueHandle<WaylandFrameState>) {
			if let wl_callback::Event::Done { .. } = event {
				state.occlusion.note_done();
			}
		}
	}

	//==============================================================================
	// 웨이랜드 프레임 콜백 맞추개. (winit 창의 표면에 손님 연결로 프레임 콜백을 보내고 받는다)
	// - 필드 차례대로 버린다. 표면 프록시와 큐를 연결보다 먼저 버린다. (창보다 먼저 버려야 한다, desktop.rs 의 WindowState 맨 앞에 둔다)
	//==============================================================================
	pub(in crate::platform::desktop) struct WaylandFramePacer {
		surface: WlSurface,
		event_queue: EventQueue<WaylandFrameState>,
		queue_handle: QueueHandle<WaylandFrameState>,
		state: WaylandFrameState,
		connection: Connection,
	}

	impl WaylandFramePacer {
		//==============================================================================
		// 생성. (웨이랜드 창이 아니거나 핸들을 못 읽으면 None)
		//==============================================================================
		pub(in crate::platform::desktop) fn new(window: &winit::window::Window) -> Option<WaylandFramePacer> {
			let display_pointer = match window.display_handle().ok()?.as_raw() {
				RawDisplayHandle::Wayland(wayland_display_handle) => {
					wayland_display_handle.display.as_ptr()
				}
				_ => {
					return None;
				}
			};
			let surface_pointer = match window.window_handle().ok()?.as_raw() {
				RawWindowHandle::Wayland(wayland_window_handle) => {
					wayland_window_handle.surface.as_ptr()
				}
				_ => {
					return None;
				}
			};
			// winit 의 wl_display 는 창이 살아 있는 동안 살아 있다. (이 값은 창보다 먼저 버린다)
			let backend = unsafe {
				Backend::from_foreign_display(display_pointer as *mut _)
			};
			let connection = Connection::from_backend(backend);
			let surface_id = unsafe {
				ObjectId::from_ptr(WlSurface::interface(), surface_pointer as *mut _)
			}.ok()?;
			let surface = WlSurface::from_id(&connection, surface_id).ok()?;
			let event_queue = connection.new_event_queue();
			let queue_handle = event_queue.handle();
			Some(WaylandFramePacer {
				surface,
				event_queue,
				queue_handle,
				state: WaylandFrameState {
					occlusion: FrameCallbackOcclusion::new(),
				},
				connection,
			})
		}

		//==============================================================================
		// 화면을 넘기기 전. (수직 동기면 지난 프레임 콜백을 기다리고, 띄워 둔 콜백이 없으면 새로 보낸다. 다음 eglSwapBuffers 의 commit 에 실린다)
		//==============================================================================
		pub(in crate::platform::desktop) fn prepare_swap(&mut self, use_vertical_sync: bool) {
			self.dispatch_pending_events();
			if use_vertical_sync && self.state.occlusion.is_callback_pending() {
				self.wait_frame_callback(Duration::from_millis(VERTICAL_SYNC_WAIT_MILLISECONDS));
			}
			if !self.state.occlusion.is_callback_pending() {
				self.surface.frame(&self.queue_handle, ());
				self.state.occlusion.note_requested(Instant::now());
			}
		}

		//==============================================================================
		// 화면을 넘긴 뒤. (보낸 요청을 내보낸다)
		//==============================================================================
		pub(in crate::platform::desktop) fn finish_swap(&mut self) {
			let _ = self.connection.flush();
		}

		//==============================================================================
		// 가려졌는지. (온 콜백을 받고 3 초 규칙으로 가른다)
		//==============================================================================
		pub(in crate::platform::desktop) fn is_occluded(&mut self) -> bool {
			self.dispatch_pending_events();
			self.state.occlusion.update(Instant::now())
		}

		//==============================================================================
		// 큐에 온 이벤트 비우기. (소켓은 winit 이 읽어 둔다)
		//==============================================================================
		fn dispatch_pending_events(&mut self) {
			let _ = self.event_queue.dispatch_pending(&mut self.state);
		}

		//==============================================================================
		// 띄워 둔 콜백이 올 때까지 기다리기. (시간이 다 되면 그만둔다, SDL Wayland_WaitForSwapInterval 과 같은 읽기 차례)
		//==============================================================================
		fn wait_frame_callback(&mut self, timeout: Duration) {
			let deadline = Instant::now() + timeout;
			let _ = self.connection.flush();
			while self.state.occlusion.is_callback_pending() {
				let read_guard = match self.event_queue.prepare_read() {
					Some(read_guard) => {
						read_guard
					}
					None => {
						// 큐에 이미 온 것이 있다.
						self.dispatch_pending_events();
						continue;
					}
				};
				let now = Instant::now();
				if now >= deadline {
					break;
				}
				let remaining_milliseconds = deadline.duration_since(now).as_millis().clamp(1, i32::MAX as u128) as std::os::raw::c_int;
				let mut poll_descriptor = PollDescriptor {
					descriptor: read_guard.connection_fd().as_raw_fd(),
					events: POLL_IN,
					returned_events: 0,
				};
				let poll_result = unsafe {
					poll(&mut poll_descriptor, 1, remaining_milliseconds)
				};
				if poll_result <= 0 {
					break;
				}
				let _ = read_guard.read();
				self.dispatch_pending_events();
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	//==============================================================================
	// 띄워 둔 콜백이 3 초 넘게 안 오면 가려짐이고, 오면 풀린다. (콜백은 하나만 띄운다)
	//==============================================================================
	#[test]
	fn frame_callback_occlusion_after_three_seconds() {
		let start_instant = Instant::now();
		let mut occlusion = FrameCallbackOcclusion::new();
		assert!(!occlusion.is_callback_pending());
		assert!(!occlusion.update(start_instant + Duration::from_secs(10)));
		occlusion.note_requested(start_instant);
		occlusion.note_requested(start_instant + Duration::from_secs(2));
		assert!(occlusion.is_callback_pending());
		assert!(!occlusion.update(start_instant + Duration::from_millis(2999)));
		assert!(occlusion.update(start_instant + Duration::from_secs(3)));
		assert!(occlusion.update(start_instant + Duration::from_secs(4)));
		occlusion.note_done();
		assert!(!occlusion.is_callback_pending());
		assert!(!occlusion.update(start_instant + Duration::from_secs(5)));
	}
}
