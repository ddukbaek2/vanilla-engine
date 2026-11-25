//==============================================================================
// 윈도우 전용 데스크탑 일. (platform/desktop.rs 가 부른다)
// - 잠자기에서 깸: winit 은 WM_POWERBROADCAST 를 넘기지 않아 창을 서브클래싱(SetWindowSubclass)해 받는다.
//   마이크로소프트 문서에 따르면 시스템은 깰 때마다 PBT_APMRESUMEAUTOMATIC 을 모든 앱에 방송하고, 창은 그것을 WindowProc 으로 받는다.
//   (메시지 전용 창은 방송을 받지 않는다. winit 의 창은 보통 창이다)
//   SetWindowSubclass 는 창을 만든 스레드에서만 부른다. (winit 이벤트 루프 스레드가 창을 만들고 여기서 단다) 창이 없어질 때(WM_NCDESTROY) 서브클래스를 뗀다.
//   잠들었다가 바로 최대 절전으로 넘어가면 깸 알림이 오지 않는다(윈도우 10 1507 부터). 모던 스탠바이에서 오는지는 문서에 없다.
// - 주된 손이 손가락인지: 크로미엄이 윈도우에서 CSS (pointer: coarse) 를 정하는 방법을 옮겼다.
//   (ui/base/pointer/pointer_device_win.cc, ui/base/pointer/pointer_device.cc, base/win/win_util.cc 의 IsDeviceUsedAsATablet)
//   태블릿으로 쓰는 중이면 손가락이다. 아니면 마우스(SM_MOUSEPRESENT)가 있으면 마우스, 손가락 입력(SM_DIGITIZER)만 있으면 손가락이다.
//==============================================================================
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use windows_sys::Win32::Devices::Display::{GetAutoRotationState, AR_ENABLED, AR_LAPTOP, AR_NOSENSOR, AR_NOT_SUPPORTED, AR_STATE};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Power::{PlatformRoleMobile, PlatformRoleSlate, PowerDeterminePlatformRoleEx, POWER_PLATFORM_ROLE, POWER_PLATFORM_ROLE_V2};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, NID_EXTERNAL_TOUCH, NID_INTEGRATED_TOUCH, NID_READY, PBT_APMRESUMEAUTOMATIC, SM_CONVERTIBLESLATEMODE, SM_DIGITIZER, SM_MAXIMUMTOUCHES, SM_MOUSEPRESENT, SM_SYSTEMDOCKED, WM_NCDESTROY, WM_POWERBROADCAST};

// 엔진이 단 서브클래스의 번호. (같은 창에 단 다른 서브클래스와 가른다)
const POWER_BROADCAST_SUBCLASS_ID: usize = 0x5641_4e49;

// 잠자기에서 깼다는 알림을 받았는지. (서브클래스가 적고, 프레임마다 꺼내 비운다)
static IS_SYSTEM_RESUMED: AtomicBool = AtomicBool::new(false);

// 전원 역할. (노트북, 태블릿 같은 기기 역할은 실행 중에 바뀌지 않아 한 번 읽는다)
static PLATFORM_ROLE: OnceLock<POWER_PLATFORM_ROLE> = OnceLock::new();

//==============================================================================
// 창에 잠자기 깸 알림 서브클래스 달기. (창을 만든 스레드에서 부른다)
//==============================================================================
pub(super) fn install_power_broadcast_hook(window: &winit::window::Window) {
	let raw_window_handle = match window.window_handle() {
		Ok(window_handle) => {
			window_handle.as_raw()
		}
		Err(_) => {
			return;
		}
	};
	let window_handle = match raw_window_handle {
		RawWindowHandle::Win32(win32_window_handle) => {
			win32_window_handle.hwnd.get() as HWND
		}
		_ => {
			return;
		}
	};
	let is_installed = unsafe {
		SetWindowSubclass(window_handle, Some(power_broadcast_subclass_procedure), POWER_BROADCAST_SUBCLASS_ID, 0)
	};
	if is_installed == 0 {
		eprintln!("Engine: 잠자기 깸 알림을 받을 창 서브클래스를 달지 못했습니다.");
	}
}

//==============================================================================
// 서브클래스 처리 함수. (PBT_APMRESUMEAUTOMATIC 이면 깸을 적고, 창이 없어지면 서브클래스를 떼고, 메시지는 그대로 다음으로 넘긴다)
//==============================================================================
unsafe extern "system" fn power_broadcast_subclass_procedure(window_handle: HWND, message: u32, word_parameter: WPARAM, long_parameter: LPARAM, _subclass_id: usize, _reference_data: usize) -> LRESULT {
	if message == WM_POWERBROADCAST && word_parameter == PBT_APMRESUMEAUTOMATIC as WPARAM {
		IS_SYSTEM_RESUMED.store(true, Ordering::Relaxed);
	}
	if message == WM_NCDESTROY {
		unsafe {
			RemoveWindowSubclass(window_handle, Some(power_broadcast_subclass_procedure), POWER_BROADCAST_SUBCLASS_ID);
		}
	}
	unsafe {
		DefSubclassProc(window_handle, message, word_parameter, long_parameter)
	}
}

//==============================================================================
// 잠자기에서 깼다는 알림을 받았는지 꺼내기. (꺼내면 비운다)
//==============================================================================
pub(super) fn take_system_resumed() -> bool {
	IS_SYSTEM_RESUMED.swap(false, Ordering::Relaxed)
}

//==============================================================================
// 주된 손이 손가락인지. (크로미엄 GetAvailablePointerAndHoverTypesImpl 과 GetPrimaryPointerType)
// - 태블릿으로 쓰는 중이면 손가락만 있는 것으로 본다. 아니면 마우스가 있으면 마우스(정밀)가 주된 손이다.
//==============================================================================
pub(super) fn is_coarse_pointer() -> bool {
	if is_device_used_as_tablet() {
		return true;
	}
	let is_mouse_present = unsafe {
		GetSystemMetrics(SM_MOUSEPRESENT)
	} != 0;
	is_touch_device_present() && !is_mouse_present
}

//==============================================================================
// 손가락 입력 장치가 있는지. (크로미엄 IsTouchDevicePresent: 디지타이저가 준비되었고 내장 또는 외장 터치)
//==============================================================================
fn is_touch_device_present() -> bool {
	let digitizer_flags = unsafe {
		GetSystemMetrics(SM_DIGITIZER)
	} as u32;
	(digitizer_flags & NID_READY) != 0 && ((digitizer_flags & NID_INTEGRATED_TOUCH) != 0 || (digitizer_flags & NID_EXTERNAL_TOUCH) != 0)
}

//==============================================================================
// 태블릿으로 쓰는 중인지. (크로미엄 base::win::IsDeviceUsedAsATablet)
// - 터치가 없거나, 도킹되었거나, 화면 회전을 못 하는(노트북, 센서 없음) 기기는 아니다.
// - 전원 역할이 모바일이나 슬레이트일 때 슬레이트 모드(SM_CONVERTIBLESLATEMODE 가 0)면 태블릿이다.
//==============================================================================
fn is_device_used_as_tablet() -> bool {
	unsafe {
		if GetSystemMetrics(SM_MAXIMUMTOUCHES) == 0 {
			return false;
		}
		if GetSystemMetrics(SM_SYSTEMDOCKED) != 0 {
			return false;
		}
		let mut rotation_state: AR_STATE = AR_ENABLED;
		if GetAutoRotationState(&mut rotation_state) != 0 && (rotation_state & (AR_NOT_SUPPORTED | AR_LAPTOP | AR_NOSENSOR)) != 0 {
			return false;
		}
	}
	let platform_role = *PLATFORM_ROLE.get_or_init(|| unsafe {
		PowerDeterminePlatformRoleEx(POWER_PLATFORM_ROLE_V2)
	});
	if platform_role == PlatformRoleMobile || platform_role == PlatformRoleSlate {
		let slate_mode = unsafe {
			GetSystemMetrics(SM_CONVERTIBLESLATEMODE)
		};
		return slate_mode == 0;
	}
	false
}
