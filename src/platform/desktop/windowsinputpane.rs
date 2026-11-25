//==============================================================================
//! 윈도우 터치 키보드. (platform/desktop.rs 가 부른다)
//! - 크롬 윈도우가 하는 것을 옮겼다.
//!   (content/browser/renderer_host/virtual_keyboard_controller_win.cc, ui/base/ime/win/on_screen_keyboard_display_manager_input_pane.cc,
//!   ui/base/ime/win/virtual_keyboard_debounce_timer.cc, content/browser/renderer_host/render_widget_host_view_aura.cc)
//!   - 띄우기: 글자 입력칸에 포커스가 가거나 포커스된 입력칸을 누를 때, 마지막 포인터가 손가락이나 펜이면 터치 키보드를 띄운다.
//!   ```text
//!     (VirtualKeyboardControllerWin::UpdateTextInputState 의 IsPointerTypeValidForVirtualKeyboard 와 show_ime_if_needed)
//!   ```
//!   - 내리기: 포커스가 글자 입력칸을 떠나면, 엔진이 띄운 키보드만 내린다. (FocusedNodeChanged(false) 의 HideVirtualKeyboard)
//!   - 띄우기와 내리기 요청은 300 밀리초로 고른다. 마지막 실행에서 300 밀리초가 지나야 마지막 요청 하나를 낸다. (VirtualKeyboardDebounceTimer(300))
//!   - 윈도우 10 RS4 부터의 InputPane(IInputPaneInterop::GetForWindow 로 창에 묶은 것)의 TryShow, TryHide 를 COM STA 스레드에서 부르고,
//!   ```text
//!     Showing 에서 OccludedRect 를, Hiding 에서 빈 영역을 받는다. (크롬도 메시지를 돌리는 COM STA 작업 스레드에서 부른다)
//!   ```
//!   - 받은 가림 영역의 세로(DIP)를 visualViewport 아래 인셋으로 쓴다. (OnKeyboardVisible 의 SetInsets(0, 0, keyboard_rect.height(), 0),
//!   ```text
//!     RenderWidgetHostViewAura::GetVisibleViewportSize 가 창 크기에서 인셋을 뺀다. 창 크기, innerHeight 는 그대로다)
//!   ```
//!   - 사용자가 키보드를 손으로 닫거나 열면 그 상태를 따른다. (OnKeyboardHidden, OnKeyboardVisible 이 virtual_keyboard_shown_ 을 바꾼다)
//! - 크롬의 navigator.virtualKeyboard.overlaysContent(키보드가 화면을 덮게 하는 웹 API)는 원본이 쓰지 않아 옮기지 않았다.
//==============================================================================
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;
use web_time::Instant;
use windows::Foundation::TypedEventHandler;
use windows::UI::ViewManagement::{InputPane, InputPaneVisibilityEventArgs};
use windows::Win32::System::WinRT::{IInputPaneInterop, RoInitialize, RO_INIT_SINGLETHREADED};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW, TranslateMessage, MSG, PM_NOREMOVE, WM_APP, WM_USER};

/// 띄우기와 내리기 요청을 고르는 시간. (크롬 OnScreenKeyboardDisplayManagerInputPane 의 VirtualKeyboardDebounceTimer(300))
const DEBOUNCE_MILLISECONDS: u64 = 300;
/// 키보드 스레드에 보내는 띄우기, 내리기 메시지.
const SHOW_MESSAGE: u32 = WM_APP + 0x5641;
const HIDE_MESSAGE: u32 = WM_APP + 0x5642;

/// 키보드가 가린 영역의 세로. (DIP, f32 비트, Showing 과 Hiding 이 적는다)
static OCCLUDED_HEIGHT_BITS: AtomicU32 = AtomicU32::new(0);
/// 키보드가 떠 있는지. (Showing 과 Hiding 이 적는다)
static IS_INPUT_PANE_VISIBLE: AtomicBool = AtomicBool::new(false);
/// Showing, Hiding 이 온 횟수. (주 스레드가 바뀐 것을 알아챈다)
static INPUT_PANE_EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

//==============================================================================
/// 띄우기, 내리기 요청.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VirtualKeyboardRequest {
	Show,
	Hide,
}

//==============================================================================
/// 터치 키보드 제어. (주 스레드, 크롬 VirtualKeyboardControllerWin)
//==============================================================================
pub(super) struct VirtualKeyboardController {
	window_handle: HWND,
	/// 키보드 스레드 번호. (처음 띄울 때 만든다)
	thread_id: Option<u32>,
	/// 고르는 중인 마지막 요청.
	pending_request: Option<VirtualKeyboardRequest>,
	/// 마지막으로 요청을 낸 때.
	last_run_instant: Option<Instant>,
	/// 키보드를 띄운 것으로 보는지. (크롬 virtual_keyboard_shown_)
	is_virtual_keyboard_shown: bool,
	/// 지난 프레임에 포커스를 가진 글자 입력칸. (없으면 0)
	focused_field_id: u64,
	/// 마지막으로 본 Showing, Hiding 횟수.
	seen_event_count: u64,
}

impl VirtualKeyboardController {
	//==============================================================================
	/// 생성. (winit 창의 HWND, 못 읽으면 None)
	//==============================================================================
	pub(super) fn new(window: &winit::window::Window) -> Option<VirtualKeyboardController> {
		use raw_window_handle::{HasWindowHandle, RawWindowHandle};
		let window_handle = match window.window_handle().ok()?.as_raw() {
			RawWindowHandle::Win32(win32_window_handle) => {
				win32_window_handle.hwnd.get() as HWND
			}
			_ => {
				return None;
			}
		};
		Some(VirtualKeyboardController {
			window_handle,
			thread_id: None,
			pending_request: None,
			last_run_instant: None,
			is_virtual_keyboard_shown: false,
			focused_field_id: 0,
			seen_event_count: 0,
		})
	}

	//==============================================================================
	/// 프레임마다 부르기. (포커스를 가진 플랫폼 입력칸 번호(없으면 0), 지난 프레임 뒤에 그 입력칸을 손가락으로 눌렀는지, 마지막 장치가 손가락인지)
	//==============================================================================
	pub(super) fn update(&mut self, focused_field_id: u64, is_field_touched: bool, is_last_pointer_touch: bool) {
		// 사용자가 손으로 열고 닫은 것을 따른다. (크롬 OnKeyboardVisible, OnKeyboardHidden)
		let event_count = INPUT_PANE_EVENT_COUNT.load(Ordering::Acquire);
		if event_count != self.seen_event_count {
			self.seen_event_count = event_count;
			self.is_virtual_keyboard_shown = IS_INPUT_PANE_VISIBLE.load(Ordering::Acquire);
		}
		let is_focus_changed = focused_field_id != self.focused_field_id;
		self.focused_field_id = focused_field_id;
		if focused_field_id == 0 {
			// 포커스가 글자 입력칸을 떠났다. (크롬 FocusedNodeChanged(false) → HideVirtualKeyboard)
			if is_focus_changed && self.is_virtual_keyboard_shown {
				self.is_virtual_keyboard_shown = false;
				self.pending_request = Some(VirtualKeyboardRequest::Hide);
			}
		}
		else if (is_focus_changed || is_field_touched) && is_last_pointer_touch && !self.is_virtual_keyboard_shown {
			// 손가락으로 입력칸에 포커스하거나 눌렀다. (크롬 ShowVirtualKeyboard)
			self.is_virtual_keyboard_shown = true;
			self.pending_request = Some(VirtualKeyboardRequest::Show);
		}
		self.run_pending_request();
	}

	//==============================================================================
	/// 고른 요청 내기. (마지막 실행에서 300 밀리초가 지났을 때 마지막 요청 하나)
	//==============================================================================
	fn run_pending_request(&mut self) {
		let request = match self.pending_request {
			Some(request) => {
				request
			}
			None => {
				return;
			}
		};
		let now = Instant::now();
		if let Some(last_run_instant) = self.last_run_instant {
			if now.duration_since(last_run_instant) < Duration::from_millis(DEBOUNCE_MILLISECONDS) {
				return;
			}
		}
		self.pending_request = None;
		self.last_run_instant = Some(now);
		let thread_id = match self.thread_id {
			Some(thread_id) => {
				thread_id
			}
			None => {
				if request == VirtualKeyboardRequest::Hide {
					return;
				}
				match start_input_pane_thread(self.window_handle) {
					Some(thread_id) => {
						self.thread_id = Some(thread_id);
						thread_id
					}
					None => {
						return;
					}
				}
			}
		};
		let message = match request {
			VirtualKeyboardRequest::Show => {
				SHOW_MESSAGE
			}
			VirtualKeyboardRequest::Hide => {
				HIDE_MESSAGE
			}
		};
		unsafe {
			PostThreadMessageW(thread_id, message, 0, 0);
		}
	}
}

//==============================================================================
/// 키보드가 가린 영역의 세로. (DIP, 떠 있지 않으면 0, visualViewport 아래 인셋)
//==============================================================================
pub(super) fn read_occluded_height() -> f32 {
	f32::from_bits(OCCLUDED_HEIGHT_BITS.load(Ordering::Acquire))
}

//==============================================================================
/// 키보드 스레드 띄우기. (COM STA 를 열고 메시지를 돌리는 스레드, 메시지 큐가 생긴 뒤의 스레드 번호를 돌려준다)
//==============================================================================
fn start_input_pane_thread(window_handle: HWND) -> Option<u32> {
	let (thread_id_sender, thread_id_receiver) = mpsc::channel();
	let spawn_result = std::thread::Builder::new().name(String::from("vanilla-input-pane")).spawn(move || {
		run_input_pane_thread(window_handle, thread_id_sender);
	});
	if let Err(error) = spawn_result {
		eprintln!("Engine: 터치 키보드 스레드를 띄우지 못했습니다: {}", error);
		return None;
	}
	thread_id_receiver.recv().ok().flatten()
}

//==============================================================================
/// 키보드 스레드. (띄우기, 내리기 메시지를 받아 InputPane 을 부르고, 그 밖의 메시지는 돌려 WinRT 이벤트가 오게 한다)
//==============================================================================
fn run_input_pane_thread(window_handle: HWND, thread_id_sender: mpsc::Sender<Option<u32>>) {
	if let Err(error) = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) } {
		eprintln!("Engine: 터치 키보드 스레드의 WinRT 를 열지 못했습니다: {}", error);
		let _ = thread_id_sender.send(None);
		return;
	}
	// 메시지 큐를 만든 뒤에 번호를 넘긴다. (큐가 없는 스레드에는 PostThreadMessage 가 실패한다)
	let mut message: MSG = unsafe { std::mem::zeroed() };
	unsafe {
		PeekMessageW(&mut message, 0, WM_USER, WM_USER, PM_NOREMOVE);
	}
	let _ = thread_id_sender.send(Some(unsafe { GetCurrentThreadId() }));
	let mut input_pane: Option<InputPane> = None;
	loop {
		let message_result = unsafe { GetMessageW(&mut message, 0, 0, 0) };
		if message_result <= 0 {
			break;
		}
		if message.hwnd == 0 && (message.message == SHOW_MESSAGE || message.message == HIDE_MESSAGE) {
			if input_pane.is_none() {
				input_pane = create_input_pane(window_handle);
			}
			if let Some(input_pane) = &input_pane {
				let _ = if message.message == SHOW_MESSAGE {
					input_pane.TryShow()
				}
				else {
					input_pane.TryHide()
				};
			}
			continue;
		}
		unsafe {
			TranslateMessage(&message);
			DispatchMessageW(&message);
		}
	}
}

//==============================================================================
/// 창에 묶은 InputPane 만들기. (IInputPaneInterop::GetForWindow, Showing 과 Hiding 을 단다. 못 만들면 None)
//==============================================================================
fn create_input_pane(window_handle: HWND) -> Option<InputPane> {
	let input_pane_interop = windows_core::factory::<InputPane, IInputPaneInterop>().ok()?;
	let input_pane: InputPane = unsafe { input_pane_interop.GetForWindow(windows::Win32::Foundation::HWND(window_handle as *mut core::ffi::c_void)) }.ok()?;
	let showing_handler = TypedEventHandler::<InputPane, InputPaneVisibilityEventArgs>::new(|input_pane, _arguments| {
		let occluded_height = match input_pane.as_ref() {
			Some(input_pane) => {
				input_pane.OccludedRect().map(|occluded_rect| occluded_rect.Height.max(0.0)).unwrap_or(0.0)
			}
			None => {
				0.0
			}
		};
		OCCLUDED_HEIGHT_BITS.store(occluded_height.to_bits(), Ordering::Release);
		IS_INPUT_PANE_VISIBLE.store(true, Ordering::Release);
		INPUT_PANE_EVENT_COUNT.fetch_add(1, Ordering::AcqRel);
		Ok(())
	});
	let hiding_handler = TypedEventHandler::<InputPane, InputPaneVisibilityEventArgs>::new(|_input_pane, _arguments| {
		OCCLUDED_HEIGHT_BITS.store(0.0f32.to_bits(), Ordering::Release);
		IS_INPUT_PANE_VISIBLE.store(false, Ordering::Release);
		INPUT_PANE_EVENT_COUNT.fetch_add(1, Ordering::AcqRel);
		Ok(())
	});
	input_pane.Showing(&showing_handler).ok()?;
	input_pane.Hiding(&hiding_handler).ok()?;
	Some(input_pane)
}
