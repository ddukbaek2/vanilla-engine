//==============================================================================
//! 리눅스와 그 밖의 유닉스(X11, 웨이랜드) 전용 데스크탑 일. (platform/desktop.rs 가 부른다)
//! - 주된 손이 손가락인지: 크로미엄 데스크탑 리눅스가 CSS (pointer: coarse) 를 정하는 방법을 옮겼다. (ui/base/pointer/pointer_device_linux.cc)
//!   터치스크린이 있고, 켜진 마우스와 터치패드와 포인팅 스틱이 하나도 없으면 손가락이다. (마우스가 있으면 마우스가 주된 손이다)
//!   장치 목록은 크로미엄처럼 화면 서버에서 읽는다. (크로미엄의 evdev 장치 분류 InputDeviceFactoryEvdev 는 크롬OS 와 화면 서버 없이 도는 판의 것이다)
//!   - 웨이랜드(ui/ozone/platform/wayland/host/wayland_connection.cc): wl_seat 에 pointer 능력이 있으면 마우스 하나, touch 능력이 있으면 터치스크린 하나다.
//!   - X11(ui/events/platform/x11/x11_hotplug_event_handler.cc): XListInputDevices 의 장치 형(MOUSE, TOUCHPAD, TOUCHSCREEN 과 XWayland 의 xwayland-pointer, xwayland-touch)과
//!     XIQueryDevice 의 쓰임과 켜짐으로 가른다. 터치스크린은 직접 터치이고 Abs MT Position X, Y 값 축(0 번과 1 번, 절대, 최소 0, 최대 0 초과)이 있어야 한다.
//!     XTEST 장치는 뺀다. 실제 포인터가 하나도 없으면 "Virtual core pointer" 를 마우스로 친다. X11 에는 포인팅 스틱 목록이 없다. (크로미엄도 같다)
//! - 어느 서버에서 읽을지는 winit 처럼 WAYLAND_DISPLAY 가 있으면 웨이랜드, 아니면 X11(DISPLAY) 이다. 못 읽으면 거짓이다.
//!   WAYLAND_SOCKET(넘겨받은 소켓)만 있으면 그 소켓은 winit 의 것이라 따로 열지 않고 X11 로 간다.
//! - 크로미엄은 장치를 꽂고 뺄 때마다 다시 읽는다. 엔진은 이것을 매 프레임 불러서, 읽은 값을 들고 있다가 2 초가 지나면 다시 읽는다.
//! - 라이브러리(libX11, libXi)는 빌드할 때 잇지 않고 실행할 때 연다. (x11-dl, glutin 이 쓰는 같은 판) 웨이랜드는 wayland-client(winit 과 같은 판)다.
//! - 가르는 함수(classify_x11_input_devices, PointerDevicePresence)는 다른 OS 에서도 시험으로 돈다.
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
use std::cell::Cell;
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
use web_time::Instant;

/// 읽은 장치 목록을 다시 읽기까지의 시간. (초)
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
const DEVICE_LIST_REFRESH_SECONDS: f32 = 2.0;
/// X11 장치 형을 볼 장치 번호의 끝. (크로미엄 kMaxDeviceNum)
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
const X11_MAX_DEVICE_COUNT: u64 = 128;
/// 실제 포인터가 없을 때 마우스로 치는 X11 의 주 포인터 이름.
const X11_VIRTUAL_CORE_POINTER_NAME: &str = "Virtual core pointer";
/// 빼는 X11 시험 장치 이름의 조각. (크로미엄 IsTestDevice)
const X11_TEST_DEVICE_NAME_PART: &str = "XTEST";

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
thread_local! {
	/// 마지막으로 읽은 손가락 여부와 읽은 때.
	static COARSE_POINTER_CACHE: Cell<Option<(bool, Instant)>> = const { Cell::new(None) };
}

//==============================================================================
/// 화면 서버에 있는 포인터 장치. (크로미엄 DeviceDataManager 의 터치스크린 목록과 마우스, 터치패드, 포인팅 스틱 목록)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PointerDevicePresence {
	pub has_touchscreen: bool,
	pub has_mouse_or_touchpad: bool,
}

impl PointerDevicePresence {
	//==============================================================================
	/// 주된 손이 손가락인지. (크로미엄 GetPrimaryPointerType: 정밀한 손이 있으면 그것, 없고 터치스크린만 있으면 손가락)
	//==============================================================================
	pub fn is_coarse_pointer(&self) -> bool {
		self.has_touchscreen && !self.has_mouse_or_touchpad
	}
}

//==============================================================================
/// X11 장치 형. (XListInputDevices 의 형 아톰, 크로미엄 DeviceType)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum X11DeviceType {
	Keyboard,
	Mouse,
	Touchpad,
	Touchscreen,
	Other,
}

//==============================================================================
/// X11 XI2 장치의 쓰임. (XIMasterPointer, XIMasterKeyboard, XISlavePointer, XISlaveKeyboard, XIFloatingSlave)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum X11DeviceUse {
	MasterPointer,
	MasterKeyboard,
	SlavePointer,
	SlaveKeyboard,
	FloatingSlave,
}

//==============================================================================
/// X11 값 축의 이름. (크로미엄이 보는 두 아톰, 그 밖은 Other)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum X11ValuatorLabel {
	MultitouchPositionX,
	MultitouchPositionY,
	Other,
}

//==============================================================================
/// X11 값 축 하나. (XIValuatorClassInfo)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct X11ValuatorRecord {
	pub label: X11ValuatorLabel,
	pub number: i32,
	pub is_absolute: bool,
	pub minimum: f64,
	pub maximum: f64,
}

//==============================================================================
/// X11 입력 장치 하나. (XIQueryDevice 의 장치에 XListInputDevices 의 형을 붙인 것)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub(super) struct X11InputDeviceRecord {
	pub name: String,
	pub device_type: X11DeviceType,
	pub device_use: X11DeviceUse,
	pub is_enabled: bool,
	/// 터치 갈래가 직접 터치(XIDirectTouch)인지.
	pub is_direct_touch: bool,
	pub valuator_list: Vec<X11ValuatorRecord>,
}

//==============================================================================
/// X11 장치 목록 가르기. (크로미엄 X11HotplugEventHandler 의 HandleMouseDevicesInWorker, HandleTouchpadDevicesInWorker, HandleTouchscreenDevicesInWorker)
//==============================================================================
pub(super) fn classify_x11_input_devices(device_list: &[X11InputDeviceRecord]) -> PointerDevicePresence {
	let mut has_touchscreen = false;
	let mut has_mouse = false;
	let mut has_touchpad = false;
	let mut has_physical_pointer = false;
	let mut has_virtual_core_pointer = false;
	for device in device_list.iter().filter(|device| device.is_enabled && !device.name.contains(X11_TEST_DEVICE_NAME_PART)) {
		if device.device_use == X11DeviceUse::SlavePointer || device.device_use == X11DeviceUse::FloatingSlave {
			has_physical_pointer = true;
		}
		if device.device_use == X11DeviceUse::MasterPointer && device.name == X11_VIRTUAL_CORE_POINTER_NAME {
			has_virtual_core_pointer = true;
		}
		if device.device_type == X11DeviceType::Mouse && device.device_use == X11DeviceUse::SlavePointer {
			has_mouse = true;
		}
		if device.device_type == X11DeviceType::Touchpad && device.device_use == X11DeviceUse::SlavePointer {
			has_touchpad = true;
		}
		let is_touchscreen_use = device.device_use == X11DeviceUse::FloatingSlave || device.device_use == X11DeviceUse::SlavePointer;
		if device.device_type == X11DeviceType::Touchscreen && is_touchscreen_use && device.is_direct_touch && has_touchscreen_axes(&device.valuator_list) {
			has_touchscreen = true;
		}
	}
	if !has_physical_pointer && has_virtual_core_pointer {
		has_mouse = true;
	}
	PointerDevicePresence {
		has_touchscreen,
		has_mouse_or_touchpad: has_mouse || has_touchpad,
	}
}

//==============================================================================
/// 터치스크린의 절대 X, Y 축이 있는지. (Abs MT Position X 는 0 번, Y 는 1 번이고 절대 축, 최소 0, 최대가 0 보다 커야 한다)
//==============================================================================
fn has_touchscreen_axes(valuator_list: &[X11ValuatorRecord]) -> bool {
	let mut maximum_x = -1.0;
	let mut maximum_y = -1.0;
	for valuator in valuator_list.iter() {
		if valuator.label == X11ValuatorLabel::MultitouchPositionX {
			if valuator.number == 0 && valuator.is_absolute && valuator.minimum == 0.0 {
				maximum_x = valuator.maximum;
			}
		}
		else if valuator.label == X11ValuatorLabel::MultitouchPositionY {
			if valuator.number == 1 && valuator.is_absolute && valuator.minimum == 0.0 {
				maximum_y = valuator.maximum;
			}
		}
	}
	maximum_x > 0.0 && maximum_y > 0.0
}

//==============================================================================
/// 주된 손이 손가락인지. (읽은 값을 2 초 동안 쓰고, 지나면 화면 서버에서 다시 읽는다)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
pub(super) fn is_coarse_pointer() -> bool {
	COARSE_POINTER_CACHE.with(|coarse_pointer_cache| {
		if let Some((is_coarse_pointer, read_instant)) = coarse_pointer_cache.get() {
			if read_instant.elapsed().as_secs_f32() < DEVICE_LIST_REFRESH_SECONDS {
				return is_coarse_pointer;
			}
		}
		let is_coarse_pointer = read_pointer_device_presence().map(|pointer_device_presence| pointer_device_presence.is_coarse_pointer()).unwrap_or(false);
		coarse_pointer_cache.set(Some((is_coarse_pointer, Instant::now())));
		is_coarse_pointer
	})
}

//==============================================================================
/// 화면 서버의 포인터 장치 읽기. (WAYLAND_DISPLAY 가 있으면 웨이랜드, 못 읽거나 없으면 X11)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
fn read_pointer_device_presence() -> Option<PointerDevicePresence> {
	if let Some(wayland_display) = std::env::var_os("WAYLAND_DISPLAY") {
		if let Some(pointer_device_presence) = read_wayland_seat_capabilities(&wayland_display) {
			return Some(pointer_device_presence);
		}
	}
	read_x11_input_devices().map(|device_list| classify_x11_input_devices(&device_list))
}

//==============================================================================
/// 웨이랜드 wl_seat 의 능력을 읽는 동안의 상태.
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
struct WaylandSeatState {
	seat: Option<wayland_client::protocol::wl_seat::WlSeat>,
	capabilities: Option<wayland_client::protocol::wl_seat::Capability>,
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
impl wayland_client::Dispatch<wayland_client::protocol::wl_registry::WlRegistry, ()> for WaylandSeatState {
	//==============================================================================
	/// 전역 목록 사건. (첫 wl_seat 를 묶는다, 능력 사건은 판 1 부터 있다)
	//==============================================================================
	fn event(state: &mut WaylandSeatState, registry: &wayland_client::protocol::wl_registry::WlRegistry, event: wayland_client::protocol::wl_registry::Event, _data: &(), _connection: &wayland_client::Connection, queue_handle: &wayland_client::QueueHandle<WaylandSeatState>) {
		if let wayland_client::protocol::wl_registry::Event::Global { name, interface, .. } = event {
			if interface == "wl_seat" && state.seat.is_none() {
				state.seat = Some(registry.bind::<wayland_client::protocol::wl_seat::WlSeat, (), WaylandSeatState>(name, 1, queue_handle, ()));
			}
		}
	}
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
impl wayland_client::Dispatch<wayland_client::protocol::wl_seat::WlSeat, ()> for WaylandSeatState {
	//==============================================================================
	/// 자리 사건. (능력을 적는다)
	//==============================================================================
	fn event(state: &mut WaylandSeatState, _seat: &wayland_client::protocol::wl_seat::WlSeat, event: wayland_client::protocol::wl_seat::Event, _data: &(), _connection: &wayland_client::Connection, _queue_handle: &wayland_client::QueueHandle<WaylandSeatState>) {
		if let wayland_client::protocol::wl_seat::Event::Capabilities { capabilities: wayland_client::WEnum::Value(capabilities) } = event {
			state.capabilities = Some(capabilities);
		}
	}
}

//==============================================================================
/// 웨이랜드 wl_seat 의 pointer, touch 능력 읽기. (WAYLAND_DISPLAY 의 소켓에 따로 붙어 두 번 왕복하고 닫는다, 못 읽으면 None)
/// - WAYLAND_DISPLAY 가 절대 경로면 그대로, 아니면 XDG_RUNTIME_DIR 아래의 이름이다. (wayland-client connect_to_env 와 같은 규칙)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
fn read_wayland_seat_capabilities(wayland_display: &std::ffi::OsStr) -> Option<PointerDevicePresence> {
	use wayland_client::protocol::wl_seat::Capability;
	let display_path = std::path::Path::new(wayland_display);
	let socket_path = if display_path.is_absolute() {
		display_path.to_path_buf()
	}
	else {
		std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join(display_path)
	};
	let stream = std::os::unix::net::UnixStream::connect(socket_path).ok()?;
	let connection = wayland_client::Connection::from_socket(stream).ok()?;
	let mut event_queue = connection.new_event_queue();
	let queue_handle = event_queue.handle();
	let _registry = connection.display().get_registry(&queue_handle, ());
	let mut state = WaylandSeatState {
		seat: None,
		capabilities: None,
	};
	event_queue.roundtrip(&mut state).ok()?;
	event_queue.roundtrip(&mut state).ok()?;
	let capabilities = state.capabilities?;
	Some(PointerDevicePresence {
		has_touchscreen: capabilities.contains(Capability::Touch),
		has_mouse_or_touchpad: capabilities.contains(Capability::Pointer),
	})
}

//==============================================================================
/// X11 입력 장치 목록 읽기. (DISPLAY 에 따로 붙어 읽고 닫는다, libX11, libXi 가 없거나 XI 2.2 가 안 되면 None)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
fn read_x11_input_devices() -> Option<Vec<X11InputDeviceRecord>> {
	let xlib_library = x11_dl::xlib::Xlib::open().ok()?;
	let xinput_library = x11_dl::xinput::XInput::open().ok()?;
	let xinput2_library = x11_dl::xinput2::XInput2::open().ok()?;
	unsafe {
		let display = (xlib_library.XOpenDisplay)(std::ptr::null());
		if display.is_null() {
			return None;
		}
		let device_list = read_x11_input_devices_from_display(&xlib_library, &xinput_library, &xinput2_library, display);
		(xlib_library.XCloseDisplay)(display);
		device_list
	}
}

//==============================================================================
/// 열린 X11 연결에서 입력 장치 목록 읽기.
/// - 터치 갈래는 XI 2.2 를 알린 클라이언트에만 오므로 XIQueryVersion(2, 2) 를 먼저 부른다. (크로미엄도 2.2 이상을 알린다)
/// - 형 아톰과 값 축 이름 아톰은 크로미엄 GetAtom 처럼 만들어서라도 받아 비교한다. (한 번의 XInternAtoms)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
unsafe fn read_x11_input_devices_from_display(xlib_library: &x11_dl::xlib::Xlib, xinput_library: &x11_dl::xinput::XInput, xinput2_library: &x11_dl::xinput2::XInput2, display: *mut x11_dl::xlib::Display) -> Option<Vec<X11InputDeviceRecord>> {
	use std::collections::HashMap;
	use std::ffi::{CStr, CString};
	use x11_dl::xinput2::{XIAllDevices, XIDirectTouch, XIFloatingSlave, XIMasterKeyboard, XIMasterPointer, XIModeAbsolute, XISlaveKeyboard, XISlavePointer, XITouchClass, XITouchClassInfo, XIValuatorClass, XIValuatorClassInfo};
	let mut major_version = 2;
	let mut minor_version = 2;
	if unsafe { (xinput2_library.XIQueryVersion)(display, &mut major_version, &mut minor_version) } != 0 {
		return None;
	}
	let atom_name_list = ["KEYBOARD", "xwayland-keyboard", "MOUSE", "xwayland-pointer", "TOUCHPAD", "TOUCHSCREEN", "xwayland-touch", "Abs MT Position X", "Abs MT Position Y"];
	let atom_name_c_string_list: Vec<CString> = atom_name_list.iter().map(|atom_name| CString::new(*atom_name).expect("아톰 이름")).collect();
	let mut atom_name_pointer_list: Vec<*mut std::os::raw::c_char> = atom_name_c_string_list.iter().map(|atom_name| atom_name.as_ptr() as *mut std::os::raw::c_char).collect();
	let mut atom_list: Vec<x11_dl::xlib::Atom> = vec![0; atom_name_list.len()];
	if unsafe { (xlib_library.XInternAtoms)(display, atom_name_pointer_list.as_mut_ptr(), atom_name_list.len() as i32, x11_dl::xlib::False, atom_list.as_mut_ptr()) } == 0 {
		return None;
	}
	let read_device_type = |type_atom: x11_dl::xlib::Atom| -> X11DeviceType {
		if type_atom == atom_list[0] || type_atom == atom_list[1] {
			X11DeviceType::Keyboard
		}
		else if type_atom == atom_list[2] || type_atom == atom_list[3] {
			X11DeviceType::Mouse
		}
		else if type_atom == atom_list[4] {
			X11DeviceType::Touchpad
		}
		else if type_atom == atom_list[5] || type_atom == atom_list[6] {
			X11DeviceType::Touchscreen
		}
		else {
			X11DeviceType::Other
		}
	};

	// 옛 XInput 목록에서 장치 번호마다 형.
	let mut device_type_table: HashMap<u64, X11DeviceType> = HashMap::new();
	let mut xinput_device_count = 0;
	let xinput_device_list = unsafe { (xinput_library.XListInputDevices)(display, &mut xinput_device_count) };
	if !xinput_device_list.is_null() {
		for device_index in 0..xinput_device_count.max(0) as usize {
			let xinput_device = unsafe { &*xinput_device_list.add(device_index) };
			if xinput_device.id < X11_MAX_DEVICE_COUNT {
				device_type_table.insert(xinput_device.id, read_device_type(xinput_device.type_));
			}
		}
		unsafe {
			(xinput_library.XFreeDeviceList)(xinput_device_list);
		}
	}

	// XI2 목록.
	let mut device_list: Vec<X11InputDeviceRecord> = Vec::new();
	let mut xinput2_device_count = 0;
	let xinput2_device_list = unsafe { (xinput2_library.XIQueryDevice)(display, XIAllDevices, &mut xinput2_device_count) };
	if xinput2_device_list.is_null() {
		return Some(device_list);
	}
	for device_index in 0..xinput2_device_count.max(0) as usize {
		let xinput2_device = unsafe { &*xinput2_device_list.add(device_index) };
		let device_use = if xinput2_device._use == XIMasterPointer {
			X11DeviceUse::MasterPointer
		}
		else if xinput2_device._use == XIMasterKeyboard {
			X11DeviceUse::MasterKeyboard
		}
		else if xinput2_device._use == XISlavePointer {
			X11DeviceUse::SlavePointer
		}
		else if xinput2_device._use == XISlaveKeyboard {
			X11DeviceUse::SlaveKeyboard
		}
		else if xinput2_device._use == XIFloatingSlave {
			X11DeviceUse::FloatingSlave
		}
		else {
			continue;
		};
		let name = if xinput2_device.name.is_null() {
			String::new()
		}
		else {
			unsafe { CStr::from_ptr(xinput2_device.name) }.to_string_lossy().into_owned()
		};
		let mut is_direct_touch = false;
		let mut valuator_list: Vec<X11ValuatorRecord> = Vec::new();
		for class_index in 0..xinput2_device.num_classes.max(0) as usize {
			let class_info = unsafe { *xinput2_device.classes.add(class_index) };
			if class_info.is_null() {
				continue;
			}
			let class_type = unsafe { (*class_info)._type };
			if class_type == XITouchClass {
				let touch_class_info = unsafe { &*(class_info as *const XITouchClassInfo) };
				is_direct_touch = touch_class_info.mode == XIDirectTouch;
			}
			else if class_type == XIValuatorClass {
				let valuator_class_info = unsafe { &*(class_info as *const XIValuatorClassInfo) };
				let label = if valuator_class_info.label == atom_list[7] {
					X11ValuatorLabel::MultitouchPositionX
				}
				else if valuator_class_info.label == atom_list[8] {
					X11ValuatorLabel::MultitouchPositionY
				}
				else {
					X11ValuatorLabel::Other
				};
				valuator_list.push(X11ValuatorRecord {
					label,
					number: valuator_class_info.number,
					is_absolute: valuator_class_info.mode == XIModeAbsolute,
					minimum: valuator_class_info.min,
					maximum: valuator_class_info.max,
				});
			}
		}
		let device_type = device_type_table.get(&(xinput2_device.deviceid as u64)).copied().unwrap_or(X11DeviceType::Other);
		device_list.push(X11InputDeviceRecord {
			name,
			device_type,
			device_use,
			is_enabled: xinput2_device.enabled != 0,
			is_direct_touch,
			valuator_list,
		});
	}
	unsafe {
		(xinput2_library.XIFreeDeviceInfo)(xinput2_device_list);
	}
	Some(device_list)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 시험용 장치. (켜짐, 직접 터치 아님, 값 축 없음)
	//==============================================================================
	fn create_device(name: &str, device_type: X11DeviceType, device_use: X11DeviceUse) -> X11InputDeviceRecord {
		X11InputDeviceRecord {
			name: String::from(name),
			device_type,
			device_use,
			is_enabled: true,
			is_direct_touch: false,
			valuator_list: Vec::new(),
		}
	}

	//==============================================================================
	/// 시험용 터치스크린. (직접 터치, Abs MT Position X 0 번, Y 1 번 절대 축)
	//==============================================================================
	fn create_touchscreen(name: &str) -> X11InputDeviceRecord {
		let mut device = create_device(name, X11DeviceType::Touchscreen, X11DeviceUse::SlavePointer);
		device.is_direct_touch = true;
		device.valuator_list = vec![
			X11ValuatorRecord { label: X11ValuatorLabel::MultitouchPositionX, number: 0, is_absolute: true, minimum: 0.0, maximum: 4095.0 },
			X11ValuatorRecord { label: X11ValuatorLabel::MultitouchPositionY, number: 1, is_absolute: true, minimum: 0.0, maximum: 4095.0 },
			X11ValuatorRecord { label: X11ValuatorLabel::Other, number: 2, is_absolute: true, minimum: 0.0, maximum: 255.0 },
		];
		device
	}

	//==============================================================================
	/// 크로미엄 X11 판정: 터치스크린만 있으면 손가락, 마우스나 터치패드가 있으면 마우스. 터치스크린의 조건과 XTEST, 꺼진 장치, 주 포인터 갈래.
	//==============================================================================
	#[test]
	fn x11_pointer_classification() {
		let virtual_core_pointer = create_device("Virtual core pointer", X11DeviceType::Other, X11DeviceUse::MasterPointer);
		let virtual_core_keyboard = create_device("Virtual core keyboard", X11DeviceType::Other, X11DeviceUse::MasterKeyboard);
		let test_pointer = create_device("Virtual core XTEST pointer", X11DeviceType::Mouse, X11DeviceUse::SlavePointer);
		let keyboard = create_device("AT Translated Set 2 keyboard", X11DeviceType::Keyboard, X11DeviceUse::SlaveKeyboard);

		// 터치스크린 태블릿. (XTEST 포인터는 실제 포인터로 치지 않는다, 키보드는 상관없다)
		let tablet_list = vec![virtual_core_pointer.clone(), virtual_core_keyboard.clone(), test_pointer.clone(), keyboard.clone(), create_touchscreen("ELAN Touchscreen")];
		assert_eq!(classify_x11_input_devices(&tablet_list), PointerDevicePresence { has_touchscreen: true, has_mouse_or_touchpad: false });
		assert!(classify_x11_input_devices(&tablet_list).is_coarse_pointer());

		// 터치스크린 노트북. (터치패드가 있으면 마우스가 주된 손)
		let mut laptop_list = tablet_list.clone();
		laptop_list.push(create_device("SynPS/2 Synaptics TouchPad", X11DeviceType::Touchpad, X11DeviceUse::SlavePointer));
		assert!(!classify_x11_input_devices(&laptop_list).is_coarse_pointer());

		// 꺼진 마우스는 없는 것과 같다. 떠 있는(floating) 마우스는 마우스 목록에 들지 않는다.
		let mut disabled_mouse = create_device("Logitech USB Mouse", X11DeviceType::Mouse, X11DeviceUse::SlavePointer);
		disabled_mouse.is_enabled = false;
		let floating_mouse = create_device("Floating Mouse", X11DeviceType::Mouse, X11DeviceUse::FloatingSlave);
		let mut tablet_with_unused_mouse_list = tablet_list.clone();
		tablet_with_unused_mouse_list.push(disabled_mouse);
		tablet_with_unused_mouse_list.push(floating_mouse);
		assert!(classify_x11_input_devices(&tablet_with_unused_mouse_list).is_coarse_pointer());

		// XWayland 의 xwayland-touch 와 xwayland-pointer 는 형 아톰에서 Touchscreen, Mouse 로 온다.
		let xwayland_list = vec![virtual_core_pointer.clone(), create_touchscreen("xwayland-touch:13"), create_device("xwayland-pointer:13", X11DeviceType::Mouse, X11DeviceUse::SlavePointer)];
		assert!(!classify_x11_input_devices(&xwayland_list).is_coarse_pointer());

		// 직접 터치가 아니거나(그림판 같은 간접 터치) 축이 맞지 않으면 터치스크린이 아니다.
		let mut dependent_touch = create_touchscreen("Dependent Touch");
		dependent_touch.is_direct_touch = false;
		let mut wrong_axis_touch = create_touchscreen("Wrong Axis Touch");
		wrong_axis_touch.valuator_list[0].minimum = -1.0;
		let mut relative_axis_touch = create_touchscreen("Relative Axis Touch");
		relative_axis_touch.valuator_list[1].is_absolute = false;
		for not_touchscreen in [dependent_touch, wrong_axis_touch, relative_axis_touch] {
			let device_list = vec![virtual_core_pointer.clone(), not_touchscreen];
			assert!(!classify_x11_input_devices(&device_list).has_touchscreen);
		}

		// 실제 포인터가 하나도 없으면 주 포인터를 마우스로 친다. (터치스크린이 떠 있는 장치라도 실제 포인터로 센다)
		let only_virtual_list = vec![virtual_core_pointer.clone()];
		assert_eq!(classify_x11_input_devices(&only_virtual_list), PointerDevicePresence { has_touchscreen: false, has_mouse_or_touchpad: true });
		let mut floating_touchscreen = create_touchscreen("Floating Touchscreen");
		floating_touchscreen.device_use = X11DeviceUse::FloatingSlave;
		let floating_touchscreen_list = vec![virtual_core_pointer.clone(), floating_touchscreen];
		assert!(classify_x11_input_devices(&floating_touchscreen_list).is_coarse_pointer());

		// 웨이랜드 wl_seat: touch 만 있으면 손가락, pointer 가 있으면 마우스.
		assert!(PointerDevicePresence { has_touchscreen: true, has_mouse_or_touchpad: false }.is_coarse_pointer());
		assert!(!PointerDevicePresence { has_touchscreen: true, has_mouse_or_touchpad: true }.is_coarse_pointer());
		assert!(!PointerDevicePresence { has_touchscreen: false, has_mouse_or_touchpad: false }.is_coarse_pointer());
	}
}
