//==============================================================================
//! 게임패드 관리자.
//! - 원본 core/gamepadmanager.js 를 옮겼다. 공개 이름(GamepadButtonCode, GamepadAnalogStickCode, GamepadTriggerCode,
//!   connect, disconnect, set_callback, update_all_gamepads, is_button_pressed, get_button_value, get_axis_value,
//!   get_all_connected_gamepads, get_connected_gamepad_count, get_connected_gamepad)과 갱신 규칙은 원본 그대로다.
//!   (값은 소수점 둘째 자리로 반올림, 축은 0.01 넘게 바뀔 때만 받기, 아날로그 값은 0 보다 클 때만 알리기, 로그 문구)
//! - 원본은 navigator.getGamepads() 를 읽는다. 웹(wasm32)은 원본처럼 navigator.getGamepads() 를 직접 읽어 브라우저가 주는 버튼과 축을 모두 받고,
//!   연결과 해제는 원본 engine.js 처럼 gamepadconnected / gamepaddisconnected 로 받는다.
//!   데스크탑은 gilrs 가 OS 입력(윈도우는 Windows.Gaming.Input)을 읽고, 원본과 같은 W3C Standard Gamepad 꼴
//!   (버튼 0~16, 축 0~3, 축 Y 는 아래가 양수)의 Gamepad 목록으로 바꾼 뒤 같은 규칙을 돌린다.
//! - 원본과 다른 점:
//!   - gilrs 기본 필터(데드존, 떨림 거르기)는 끈다. 브라우저처럼 날값을 받고, 데드존은 원본처럼 게임이 건다.
//!   - 아날로그 버튼(트리거)의 눌림은 크로미엄과 같이 값이 30/255 를 넘을 때다. (gilrs 기본은 0.75 에 눌림, 0.65 에 뗌)
//!   - 축(햇 스위치)으로 오는 십자키는 버튼 12~15 로 바꾼다. (브라우저가 하는 일)
//!   - 데스크탑의 버튼은 표준 17개 뒤에 gilrs 가 표준 버튼으로 잇지 못한 단추(매핑에 없는 단추, gilrs 의 C, Z)를, 축은 표준 4개 뒤에
//!     매핑에 없는 축을 처음 움직인 차례로 붙인다. gilrs 가 장치의 단추 목록을 내주지 않고 한 번이라도 움직인 단추만 알려서, 그 단추를 처음 누른 뒤에야 생긴다.
//!     버튼이나 축 수가 늘면 그 패드의 상태 칸도 늘린다.
//!   - 플레이스테이션 패드(소니 054c 의 듀얼쇼크4 05c4, 09cc, 0ba0 와 듀얼센스 0ce6, 0df2, 0e5f)는 크롬(device/gamepad 의 MapperDualshock4, MapperDualSense)처럼
//!     버튼 18개이고 원시 단추 13번(HID 단추 14)이 TOUCHPAD(17)다. 누르기 전에도 늘 있고, 그 밖의 단추(듀얼센스 마이크 단추)와 축은 붙이지 않는다.
//!     gilrs 가 SDL 매핑의 touchpad:b13 을 이름 없는 단추(Button::Unknown, misc1 과 같은 이름)로 잇기만 해서, 그 단추의 원시 코드를 플랫폼마다 정해 찾는다.
//!     (맥 IOKit 단추 쪽의 쓰임 14, 윈도우 Windows.Gaming.Input 의 단추 13번, 리눅스 hid-generic 의 BTN_GAMEPAD + 13)
//!     리눅스의 hid-sony(커널 4.10 부터), hid-playstation(5.12 부터) 드라이버는 터치패드를 따로 장치로 내고 bcdHID 에 0x8000 을 붙인다.
//!     그때는 크롬(MapperPs4Ps5)처럼 버튼 17개다. (gilrs 의 uuid 13, 14번째 바이트가 그 판 번호다)
//!     맥 GameController 의 GCDualSenseGamepad.touchpadButton 도 있지만, GameController 는 패드의 제조사, 제품 번호를 내주지 않아 gilrs 패드와 짝짓지 못해 쓰지 않는다.
//!   - Gamepad.index 는 데스크탑에서는 gilrs 의 GamepadId 번호다. (처음 붙은 순서로 0 부터, 브라우저 index 와 같은 쓰임) 웹은 브라우저 index 다.
//!   - Gamepad.id 는 웹에서는 브라우저 id 그대로, 데스크탑에서는 크롬 꼴 "이름 (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)" 로 만든다.
//!   - 콜백의 button 값은 bool 대신 1.0(눌림) / 0.0(뗌) 이다.
//!   - gilrs(웹은 연결 리스너)는 처음 부를 때(갱신, 진동, 패드 목록 읽기) 만든다. (부르지 않는 엔진과 시험에서는 장치를 열지 않는다)
//! - get_all_connected_gamepads / get_connected_gamepad 는 원본 navigator.getGamepads() 처럼 부를 때 그 자리에서 읽는다. (데스크탑은 gilrs 사건을 비워 지금 상태,
//!   그 사이의 연결과 해제는 다음 갱신이 받는다) update_all_gamepads_with_list 로 목록을 넣은 관리자(시험, 다른 입력원)는 넣은 목록을 돌려준다.
//!   - 더한 것: set_log_enabled (로그 켜고 끄기),
//!     update_all_gamepads_with_list (gilrs 없이 목록을 넣는 길. 시험이나 다른 입력원)
//!   - 배터리 정보는 원본에 없어 옮기지 않는다.
//!   - 단추와 축이 바뀔 때마다 찍는 로그는 원본처럼 set_input_log_enabled 로 켜고, 기본은 끔이다. set_log_enabled 는 연결과 해제를 포함한 모든 로그의 큰 스위치다.
//!   - GamepadKind, read_gamepad_kind: 패드 이름으로 종류(엑스박스, 플레이스테이션, 닌텐도, 그 밖)를 가른다. get_active_gamepad_id 는 마지막 갱신에 단추를 누르거나 축을 크게 기울인 패드다.
//!   - vibrate(strong_magnitude, weak_magnitude, seconds): 연결된 패드를 모두 떤다.
//!     데스크탑은 gilrs force feedback(리눅스 evdev, 윈도우 XInput 과 Windows.Gaming.Input)으로 떨고, gilrs 가 진동을 대지 않는 macOS 는
//!     플랫폼 층(platform/desktop/gamepadhaptics.rs)이 GameController 와 CoreHaptics 로 gilrs 패드와 짝인 컨트롤러를 떤다. (macOS 는 길이나 세기가 0 이면 앞 진동을 멈춘다)
//!     그래서 gilrs 를 진동을 켠 채 만든다. (데스크탑에서 gilrs 의 진동 스레드 하나가 돈다) 웹은 원본처럼 navigator.getGamepads() 의 vibrationActuator 에 dual-rumble 을 건다.
//!     첫 갱신(tick) 전에 불러도 그 자리에서 gilrs 를 만들어 떤다. (원본처럼)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
use gilrs::{Axis, Button, GilrsBuilder};
use std::collections::HashMap;

/// W3C Standard Gamepad 의 버튼 수. (0~16)
const STANDARD_BUTTON_COUNT: usize = 17;
/// W3C Standard Gamepad 의 축 수. (0~3)
const STANDARD_AXIS_COUNT: usize = 4;
/// 아날로그 버튼을 눌림으로 보는 값. (크로미엄 GamepadButton::kDefaultButtonPressedThreshold, XInput 트리거 데드존과 같다)
#[cfg(not(target_arch = "wasm32"))]
const BUTTON_PRESSED_THRESHOLD: f32 = 30.0 / 255.0;
/// 축(햇 스위치)으로 오는 십자키를 눌림으로 보는 값.
#[cfg(not(target_arch = "wasm32"))]
const DIRECTIONAL_PAD_AXIS_THRESHOLD: f32 = 0.5;
/// 축 변화를 받는 최소 폭. (원본: 미세한 떨림을 거르려 0.01 넘는 변화만 받는다)
const AXIS_CHANGE_THRESHOLD: f64 = 0.01;
/// 플레이스테이션 패드의 USB 제조사 번호. (소니)
#[cfg(not(target_arch = "wasm32"))]
const PLAYSTATION_VENDOR_ID: u16 = 0x054c;
/// 터치패드 단추를 TOUCHPAD(17) 에 두는 플레이스테이션 패드의 제품 번호. (크롬 gamepad_standard_mappings 가 MapperDualshock4, MapperDualSense 로 잇는 것)
#[cfg(not(target_arch = "wasm32"))]
const PLAYSTATION_TOUCHPAD_PRODUCT_ID_LIST: [u16; 6] = [0x05c4, 0x09cc, 0x0ba0, 0x0ce6, 0x0df2, 0x0e5f];
/// 패드의 원시 단추 목록에서 터치패드의 자리. (크롬 `input.buttons[13]`, SDL 매핑 touchpad:b13)
#[cfg(not(target_arch = "wasm32"))]
const PLAYSTATION_TOUCHPAD_RAW_BUTTON_INDEX: u32 = 13;
/// 맥 IOKit 의 단추 쪽 번호. (kHIDPage_Button, gilrs 의 맥 원시 코드는 쪽 << 16 | 쓰임)
#[cfg(not(target_arch = "wasm32"))]
const HID_BUTTON_USAGE_PAGE: u32 = 0x09;
/// 리눅스 evdev 의 키 사건 종류와 첫 게임패드 단추. (EV_KEY, BTN_GAMEPAD. gilrs 의 리눅스 원시 코드는 종류 << 16 | 코드)
#[cfg(not(target_arch = "wasm32"))]
const LINUX_EVENT_KEY_TYPE: u32 = 0x01;
#[cfg(not(target_arch = "wasm32"))]
const LINUX_FIRST_GAMEPAD_BUTTON_CODE: u32 = 0x130;
/// 리눅스 드라이버가 플레이스테이션 패드를 고쳐 잡았다는 표시. (hid-sony, hid-playstation 이 bcdHID 에 붙이는 것, 크롬 kDualshockPatchedBcdHidMask)
#[cfg(not(target_arch = "wasm32"))]
const LINUX_PATCHED_HID_VERSION_MASK: u16 = 0x8000;

/// W3C Standard Gamepad 버튼 순서의 gilrs 버튼. (배열 번호가 GamepadButtonCode)
#[cfg(not(target_arch = "wasm32"))]
const STANDARD_BUTTON_LIST: [Button; STANDARD_BUTTON_COUNT] = [
	Button::South,
	Button::East,
	Button::West,
	Button::North,
	Button::LeftTrigger,
	Button::RightTrigger,
	Button::LeftTrigger2,
	Button::RightTrigger2,
	Button::Select,
	Button::Start,
	Button::LeftThumb,
	Button::RightThumb,
	Button::DPadUp,
	Button::DPadDown,
	Button::DPadLeft,
	Button::DPadRight,
	Button::Mode,
];

/// W3C Standard Gamepad 축 순서의 gilrs 축과 곱할 부호. (gilrs 의 Y 는 위가 양수, W3C 는 아래가 양수라 뒤집는다)
#[cfg(not(target_arch = "wasm32"))]
const STANDARD_AXIS_LIST: [(Axis, f32); STANDARD_AXIS_COUNT] = [
	(Axis::LeftStickX, 1.0),
	(Axis::LeftStickY, -1.0),
	(Axis::RightStickX, 1.0),
	(Axis::RightStickY, -1.0),
];

//==============================================================================
/// 게임패드 아날로그 스틱 식별자. (W3C Standard Gamepad 매핑 규격)
/// - 값은 Gamepad.axes 의 번호이고 get_axis_value 의 axis_index 로 쓴다.
///
/// # 예제
/// 왼쪽 스틱 축 읽기
/// ```rust
/// use vanilla::{Gamepad, GamepadAnalogStickCode, GamepadManager};
///
/// let mut gamepad_manager = GamepadManager::new();
/// gamepad_manager.set_log_enabled(false);
/// let mut gamepad = Gamepad::create(0, "Test Pad");
/// gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.75;
/// gamepad.axes[GamepadAnalogStickCode::LEFT_Y] = -1.0;
/// gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad)]);
/// assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_X), 0.75);
/// assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_Y), -1.0);
/// assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::RIGHT_X), 0.0);
/// ```
//==============================================================================
pub struct GamepadAnalogStickCode;

impl GamepadAnalogStickCode {
	/// 왼쪽 스틱 가로 축. (-1 왼쪽 ~ 1 오른쪽)
	pub const LEFT_X: usize = 0;
	/// 왼쪽 스틱 세로 축. (-1 위 ~ 1 아래)
	pub const LEFT_Y: usize = 1;
	/// 오른쪽 스틱 가로 축. (-1 왼쪽 ~ 1 오른쪽)
	pub const RIGHT_X: usize = 2;
	/// 오른쪽 스틱 세로 축. (-1 위 ~ 1 아래)
	pub const RIGHT_Y: usize = 3;
}

//==============================================================================
/// 게임패드 트리거 식별자. (W3C Standard Gamepad 매핑 규격)
/// - 값은 버튼 번호다. (GamepadButtonCode::L2, R2 와 같다) 아날로그 값은 get_button_value 로 읽는다.
///
/// # 예제
/// 오른쪽 트리거를 반쯤 당긴 값 읽기
/// ```rust
/// use vanilla::{Gamepad, GamepadButton, GamepadButtonCode, GamepadManager, GamepadTriggerCode};
///
/// let mut gamepad_manager = GamepadManager::new();
/// gamepad_manager.set_log_enabled(false);
/// let mut gamepad = Gamepad::create(0, "Test Pad");
/// gamepad.buttons[GamepadTriggerCode::R2] = GamepadButton { pressed: true, value: 0.5 };
/// gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad)]);
/// assert!(gamepad_manager.is_button_pressed(0, GamepadTriggerCode::R2));
/// assert_eq!(gamepad_manager.get_button_value(0, GamepadTriggerCode::R2), 0.5);
/// assert_eq!(GamepadTriggerCode::L2, GamepadButtonCode::L2);
/// ```
//==============================================================================
pub struct GamepadTriggerCode;

impl GamepadTriggerCode {
	/// 왼쪽 트리거. (버튼 6번, 값 0 ~ 1)
	pub const L2: usize = 6;
	/// 오른쪽 트리거. (버튼 7번, 값 0 ~ 1)
	pub const R2: usize = 7;
}

//==============================================================================
/// 게임패드 버튼 식별자. (W3C Standard Gamepad 매핑 규격)
/// - 값은 Gamepad.buttons 의 번호이고 is_button_pressed 들의 button_index 로 쓴다. 앞면 버튼은 자리로 정한다. (아래 0, 오른쪽 1, 왼쪽 2, 위 3)
///
/// # 예제
/// 두 번 갱신하며 방금 눌림과 눌린 채 가르기
/// ```rust
/// use vanilla::{Gamepad, GamepadButton, GamepadButtonCode, GamepadManager};
///
/// let mut gamepad_manager = GamepadManager::new();
/// gamepad_manager.set_log_enabled(false);
/// let mut gamepad = Gamepad::create(0, "Test Pad");
/// gamepad.buttons[GamepadButtonCode::A_CROSS] = GamepadButton { pressed: true, value: 1.0 };
/// gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
/// assert!(gamepad_manager.is_button_just_pressed(0, GamepadButtonCode::A_CROSS));
/// gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad)]);
/// assert!(gamepad_manager.is_button_pressed(0, GamepadButtonCode::A_CROSS));
/// assert!(!gamepad_manager.is_button_just_pressed(0, GamepadButtonCode::A_CROSS));
/// assert!(!gamepad_manager.is_button_pressed(0, GamepadButtonCode::B_CIRCLE));
/// ```
//==============================================================================
pub struct GamepadButtonCode;

impl GamepadButtonCode {
	/// 앞면 버튼.
	pub const A_CROSS: usize = 0;
	/// 앞면 오른쪽 버튼. (엑스박스 B, 플레이스테이션 동그라미)
	pub const B_CIRCLE: usize = 1;
	/// 앞면 왼쪽 버튼. (엑스박스 X, 플레이스테이션 네모)
	pub const X_SQUARE: usize = 2;
	/// 앞면 위쪽 버튼. (엑스박스 Y, 플레이스테이션 세모)
	pub const Y_TRIANGLE: usize = 3;

	/// 범퍼.
	pub const L1: usize = 4;
	/// 오른쪽 범퍼.
	pub const R1: usize = 5;

	/// 트리거. (버튼으로도 인식됨)
	pub const L2: usize = 6;
	/// 오른쪽 트리거. (값 0 ~ 1)
	pub const R2: usize = 7;

	/// 시스템.
	pub const SHARE_VIEW: usize = 8;
	/// 오른쪽 시스템 버튼. (엑스박스 메뉴, 플레이스테이션 옵션)
	pub const OPTIONS_MENU: usize = 9;

	/// 스틱 누름.
	pub const L3: usize = 10;
	/// 오른쪽 스틱 누름.
	pub const R3: usize = 11;

	/// 십자키.
	pub const DPAD_UP: usize = 12;
	/// 십자키 아래.
	pub const DPAD_DOWN: usize = 13;
	/// 십자키 왼쪽.
	pub const DPAD_LEFT: usize = 14;
	/// 십자키 오른쪽.
	pub const DPAD_RIGHT: usize = 15;

	/// 가운데.
	pub const HOME_PS: usize = 16;
	/// 플레이스테이션 터치패드 누름. (표준 17개 뒤의 버튼, 듀얼쇼크4와 듀얼센스만)
	pub const TOUCHPAD: usize = 17;
}

//==============================================================================
/// 게임패드 종류. (단추 안내를 그 패드의 표기로 보일 때 쓴다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadKind {
	/// 엑스박스 패드. (이름에 045e, xbox, xinput, microsoft)
	Xbox,
	/// 플레이스테이션 패드. (이름에 054c, playstation, dualshock, dualsense, sony)
	PlayStation,
	/// 닌텐도 패드. (이름에 057e, nintendo, joy-con, pro controller)
	Nintendo,
	/// 그 밖. (이름으로 가르지 못한 패드)
	Other,
}

/// 패드 이름(Gamepad.id)에 이것이 있으면 그 종류다. (소문자, 브라우저와 데스크탑 id 의 USB 제조사 번호와 흔한 이름)
const PLAYSTATION_GAMEPAD_ID_PARTS: [&str; 5] = ["054c", "playstation", "dualshock", "dualsense", "sony"];
const NINTENDO_GAMEPAD_ID_PARTS: [&str; 4] = ["057e", "nintendo", "joy-con", "pro controller"];
const XBOX_GAMEPAD_ID_PARTS: [&str; 4] = ["045e", "xbox", "xinput", "microsoft"];
/// 축이 이만큼 넘게 기울면 그 패드를 쓴 것으로 본다. (가운데의 떨림으로 마지막 장치가 바뀌지 않게)
const GAMEPAD_ACTIVE_AXIS_THRESHOLD: f64 = 0.5;

//==============================================================================
/// 패드 이름으로 종류 가르기. (플레이스테이션 054c, 닌텐도 057e, 엑스박스 045e 와 이름, 모르면 Other)
/// - 대소문자를 가리지 않고 플레이스테이션, 닌텐도, 엑스박스 차례로 본다.
///
/// # 인자
/// - `gamepad_id`: 패드 이름. (Gamepad.id)
///
/// # 돌려줌
/// 이름에 든 제조사 번호나 흔한 이름으로 가른 종류. 맞는 것이 없으면 Other.
///
/// # 예제
/// 브라우저와 데스크탑 꼴 이름 가르기
/// ```rust
/// use vanilla::{read_gamepad_kind, GamepadKind};
///
/// let dual_sense_id = "DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)";
/// assert_eq!(read_gamepad_kind(dual_sense_id), GamepadKind::PlayStation);
/// assert_eq!(read_gamepad_kind("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), GamepadKind::Xbox);
/// assert_eq!(read_gamepad_kind("Steam Virtual Gamepad"), GamepadKind::Other);
/// ```
//==============================================================================
pub fn read_gamepad_kind(gamepad_id: &str) -> GamepadKind {
	let lower_case_id = gamepad_id.to_lowercase();
	let kind_part_list: [(GamepadKind, &[&str]); 3] = [
		(GamepadKind::PlayStation, &PLAYSTATION_GAMEPAD_ID_PARTS),
		(GamepadKind::Nintendo, &NINTENDO_GAMEPAD_ID_PARTS),
		(GamepadKind::Xbox, &XBOX_GAMEPAD_ID_PARTS),
	];
	for (gamepad_kind, id_parts) in kind_part_list.iter() {
		for id_part in id_parts.iter() {
			if lower_case_id.contains(id_part) {
				return *gamepad_kind;
			}
		}
	}
	GamepadKind::Other
}

//==============================================================================
/// 입력 종류. (원본 콜백의 input_type 문자열 "button" / "value" / "axis")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamepadInputType {
	/// 눌림 변화. (값은 1.0 눌림, 0.0 뗌)
	Button,
	/// 버튼 아날로그 값 변화. (0 보다 클 때만 온다)
	Value,
	/// 스틱 축 값 변화.
	Axis,
}

/// 입력 콜백. (gamepad_index, input_type, input_index, value)
pub type GamepadInputCallback = Box<dyn FnMut(usize, GamepadInputType, usize, f32)>;

//==============================================================================
/// 게임패드 버튼 하나. (W3C GamepadButton)
///
/// # 예제
/// 패드 사본의 버튼 바꾸기
/// ```rust
/// use vanilla::{Gamepad, GamepadButton, GamepadButtonCode};
///
/// let mut gamepad = Gamepad::create(0, "Test Pad");
/// assert!(!gamepad.buttons[GamepadButtonCode::L2].pressed);
/// gamepad.buttons[GamepadButtonCode::L2] = GamepadButton { pressed: true, value: 0.4 };
/// assert_eq!(gamepad.buttons[GamepadButtonCode::L2].value, 0.4);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GamepadButton {
	/// 눌렸는지.
	pub pressed: bool,
	/// 값. (0.0 ~ 1.0, 트리거는 아날로그)
	pub value: f32,
}

//==============================================================================
/// 게임패드 하나의 사본. (W3C Gamepad, navigator.getGamepads() 가 주는 것과 같은 꼴)
///
/// # 예제
/// 패드를 만들어 연결된 패드 목록에서 읽기
/// ```rust
/// use vanilla::{read_gamepad_kind, Gamepad, GamepadKind, GamepadManager};
///
/// let gamepad = Gamepad::create(1, "Xbox Wireless Controller");
/// assert_eq!(gamepad.buttons.len(), 17);
/// assert_eq!(gamepad.axes.len(), 4);
/// assert!(gamepad.connected);
/// assert_eq!(read_gamepad_kind(&gamepad.id), GamepadKind::Xbox);
///
/// let mut gamepad_manager = GamepadManager::new();
/// gamepad_manager.set_log_enabled(false);
/// gamepad_manager.update_all_gamepads_with_list(vec![None, Some(gamepad)]);
/// let connected_gamepad = gamepad_manager.get_connected_gamepad(0);
/// assert_eq!(connected_gamepad.map(|gamepad| gamepad.index), Some(1));
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct Gamepad {
	/// 하드웨어 식별 번호. (is_button_pressed 들의 gamepad_index)
	pub index: usize,
	/// 장치 이름.
	pub id: String,
	/// 연결되어 있는지.
	pub connected: bool,
	/// 버튼 목록. (GamepadButtonCode 순서)
	pub buttons: Vec<GamepadButton>,
	/// 축 목록. (GamepadAnalogStickCode 순서, -1.0 ~ 1.0)
	pub axes: Vec<f32>,
}

impl Gamepad {
	//==============================================================================
	/// 생성. (버튼 17개는 뗀 상태, 축 4개는 0)
	///
	/// # 인자
	/// - `index`: 하드웨어 식별 번호. (update_all_gamepads_with_list 에 넘기는 목록의 번호와 같게)
	/// - `id`: 장치 이름.
	///
	/// # 돌려줌
	/// 연결된(connected 가 true) 패드 사본.
	//==============================================================================
	pub fn create(index: usize, id: &str) -> Gamepad {
		Gamepad {
			index,
			id: id.to_string(),
			connected: true,
			buttons: vec![GamepadButton { pressed: false, value: 0.0 }; STANDARD_BUTTON_COUNT],
			axes: vec![0.0; STANDARD_AXIS_COUNT],
		}
	}
}

//==============================================================================
/// 버튼 하나의 마지막 상태. (원본 { pressed, value })
//==============================================================================
#[derive(Clone, Copy)]
struct GamepadButtonState {
	pressed: bool,
	value: f64,
	/// 지난 갱신의 눌림. (러스트 판에서 더한 Just 판정용)
	previous_pressed: bool,
}

//==============================================================================
/// 게임패드 하나의 마지막 상태. (원본 { buttons: Map, axes: Map })
//==============================================================================
struct GamepadState {
	buttons: Vec<GamepadButtonState>,
	axes: Vec<f64>,
}

//==============================================================================
/// 게임패드 관리자.
/// - InputManager 가 하나 들고 tick 에서 폴링한다. 씬은 engine.get_input_manager().get_gamepad_manager() 로 읽는다. (패드 목록 읽기는 그 자리에서 읽어 get_gamepad_manager_mut)
///
/// # 예제
/// 콜백으로 버튼 변화 받기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{Gamepad, GamepadButton, GamepadButtonCode, GamepadInputType, GamepadManager};
///
/// let event_list = Rc::new(RefCell::new(Vec::new()));
/// let callback_event_list = event_list.clone();
/// let mut gamepad_manager = GamepadManager::new();
/// gamepad_manager.set_log_enabled(false);
/// gamepad_manager.set_callback(Some(Box::new(move |gamepad_index, input_type, input_index, value| {
/// 	callback_event_list.borrow_mut().push((gamepad_index, input_type, input_index, value));
/// })));
/// let mut gamepad = Gamepad::create(0, "Test Pad");
/// gamepad.buttons[GamepadButtonCode::A_CROSS] = GamepadButton { pressed: true, value: 1.0 };
/// gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad)]);
/// assert_eq!(gamepad_manager.get_connected_gamepad_count(), 1);
/// assert_eq!(event_list.borrow()[0], (0, GamepadInputType::Button, GamepadButtonCode::A_CROSS, 1.0));
/// ```
///
/// 씬에서 첫 패드의 단추와 스틱 읽기
/// ```rust,no_run
/// use vanilla::{Engine, GamepadAnalogStickCode, GamepadButtonCode};
///
/// fn read_first_gamepad(engine: &mut Engine) {
/// 	let gamepad_manager = engine.get_input_manager_mut().get_gamepad_manager_mut();
/// 	let gamepad_index = match gamepad_manager.get_connected_gamepad(0) {
/// 		Some(gamepad) => {
/// 			gamepad.index
/// 		}
/// 		None => {
/// 			return;
/// 		}
/// 	};
/// 	if gamepad_manager.is_button_just_pressed(gamepad_index, GamepadButtonCode::A_CROSS) {
/// 		println!("confirm");
/// 	}
/// 	let stick_x = gamepad_manager.get_axis_value(gamepad_index, GamepadAnalogStickCode::LEFT_X);
/// 	println!("{}", stick_x);
/// }
/// ```
//==============================================================================
pub struct GamepadManager {
	/// 연결된 게임패드 식별자(index) 목록.
	connected_gamepad_indices: Vec<usize>,
	/// key: index, value: 마지막 상태.
	connected_gamepad_states: HashMap<usize, GamepadState>,
	/// 입력 콜백 함수.
	input_event_callback: Option<GamepadInputCallback>,
	/// 마지막 갱신에 읽은 목록. (navigator.getGamepads() 자리, 번호가 index)
	gamepad_list: Vec<Option<Gamepad>>,
	/// gilrs 입력원. (처음 갱신할 때 만든다)
	gamepad_reader: Option<GamepadReader>,
	/// gilrs 를 만들어 봤는지.
	is_gamepad_reader_creation_tried: bool,
	/// update_all_gamepads_with_list 로 목록을 넣었는지. (그러면 장치 대신 넣은 목록을 읽는다)
	is_external_list_used: bool,
	/// 로그를 내는지.
	is_log_enabled: bool,
	/// 단추와 축이 바뀔 때마다 로그를 내는지.
	is_input_log_enabled: bool,
	/// 마지막 갱신에 단추를 누르거나 축을 크게 기울인 패드의 이름.
	active_gamepad_id: Option<String>,
}

impl GamepadManager {
	//==============================================================================
	/// 생성.
	/// - 로그는 켜고 입력 로그는 끈 채 시작한다. 장치(gilrs)는 처음 부를 때 연다.
	///
	/// # 돌려줌
	/// 연결된 패드가 없는 관리자.
	//==============================================================================
	pub fn new() -> GamepadManager {
		GamepadManager {
			connected_gamepad_indices: Vec::new(),
			connected_gamepad_states: HashMap::new(),
			input_event_callback: None,
			gamepad_list: Vec::new(),
			gamepad_reader: None,
			is_gamepad_reader_creation_tried: false,
			is_external_list_used: false,
			is_log_enabled: true,
			is_input_log_enabled: false,
			active_gamepad_id: None,
		}
	}

	//==============================================================================
	/// 갱신.
	/// - update_all_gamepads 를 부른다.
	///
	/// # 인자
	/// - `_time_delta`: 지난 프레임 뒤로 흐른 초. (쓰지 않는다)
	//==============================================================================
	pub fn tick(&mut self, _time_delta: f32) {
		self.update_all_gamepads();
	}

	//==============================================================================
	/// 게임패드 연결.
	/// - 상태 칸을 그 패드의 버튼, 축 수만큼 뗀 상태와 0 으로 만든다. 로그가 켜져 있으면 연결 로그를 낸다.
	///
	/// # 인자
	/// - `gamepad`: 붙일 패드. (같은 index 가 이미 붙어 있으면 건너뛴다)
	//==============================================================================
	pub fn connect(&mut self, gamepad: &Gamepad) {
		if self.connected_gamepad_indices.contains(&gamepad.index) {
			return;
		}

		// 버튼, 축 초기화.
		let button_count = gamepad.buttons.len();
		let axis_count = gamepad.axes.len();
		let button_state_list = vec![GamepadButtonState { pressed: false, value: 0.0, previous_pressed: false }; button_count];
		let axis_value_list = vec![0.0; axis_count];
		let state = GamepadState {
			buttons: button_state_list,
			axes: axis_value_list,
		};

		self.connected_gamepad_indices.push(gamepad.index);
		self.connected_gamepad_states.insert(gamepad.index, state);
		if self.is_log_enabled {
			write_log(&format!("[GamepadManager] Connected: {} at index {}", gamepad.id, gamepad.index));
		}
	}

	//==============================================================================
	/// 게임패드 연결 해제.
	///
	/// # 인자
	/// - `gamepad`: 뗄 패드. (index 로 찾는다, 붙어 있지 않으면 건너뛴다)
	//==============================================================================
	pub fn disconnect(&mut self, gamepad: &Gamepad) {
		let list_index = match self.connected_gamepad_indices.iter().position(|connected_gamepad_index| *connected_gamepad_index == gamepad.index) {
			Some(list_index) => {
				list_index
			}
			None => {
				return;
			}
		};

		self.connected_gamepad_indices.remove(list_index);
		self.connected_gamepad_states.remove(&gamepad.index);
		if self.is_log_enabled {
			write_log(&format!("[GamepadManager] Disconnected: index {}", gamepad.index));
		}
	}

	//==============================================================================
	/// 콜백 설정. (None 이면 끈다)
	///
	/// # 인자
	/// - `callback`: 갱신 중 입력이 바뀔 때마다 부를 함수. (gamepad_index 는 Gamepad.index, 버튼 눌림은 Button 과 1.0 / 0.0, 아날로그 값은 Value, 축은 Axis)
	//==============================================================================
	pub fn set_callback(&mut self, callback: Option<GamepadInputCallback>) {
		self.input_event_callback = callback;
	}

	//==============================================================================
	/// 로그 켜고 끄기. (러스트 판에서 더한 것. 기본은 원본처럼 켜짐)
	///
	/// # 인자
	/// - `is_enabled`: 연결, 해제, 입력 로그를 모두 내려면 true.
	//==============================================================================
	pub fn set_log_enabled(&mut self, is_enabled: bool) {
		self.is_log_enabled = is_enabled;
	}

	//==============================================================================
	/// 단추와 축이 바뀔 때마다 로그를 낼지. (기본은 끔, set_log_enabled 가 꺼져 있으면 켜도 내지 않는다)
	///
	/// # 인자
	/// - `is_enabled`: 입력 로그를 내려면 true.
	//==============================================================================
	pub fn set_input_log_enabled(&mut self, is_enabled: bool) {
		self.is_input_log_enabled = is_enabled;
	}

	//==============================================================================
	/// 단추와 축이 바뀔 때마다 로그를 내는지.
	///
	/// # 돌려줌
	/// 입력 로그를 켰으면 true. (set_log_enabled 는 보지 않는다)
	//==============================================================================
	pub fn is_input_log_enabled(&self) -> bool {
		self.is_input_log_enabled
	}

	//==============================================================================
	/// 마지막 갱신에 단추를 누르거나 축을 0.5 넘게 기울인 패드의 이름. (없으면 None, 마지막으로 쓴 장치를 가릴 때)
	///
	/// # 돌려줌
	/// 그 패드의 Gamepad.id. 마지막 갱신에 새로 누른 단추나 0.5 를 새로 넘긴 축이 없으면 None. (누른 채 두면 다음 갱신에 None)
	//==============================================================================
	pub fn get_active_gamepad_id(&self) -> Option<&str> {
		self.active_gamepad_id.as_deref()
	}

	//==============================================================================
	/// 연결된 패드를 모두 떨기. (세기는 0 ~ 1, 무거운 모터와 가벼운 모터, 초)
	/// - 데스크탑은 gilrs force feedback(macOS 는 GameController 와 CoreHaptics), 웹은 Gamepad.vibrationActuator 의 dual-rumble. 진동을 대지 않는 패드는 건너뛴다.
	///
	/// # 인자
	/// - `strong_magnitude`: 무거운(낮은 주파수) 모터 세기. (0 ~ 1 로 자르고, 수가 아니면 0)
	/// - `weak_magnitude`: 가벼운(높은 주파수) 모터 세기. (0 ~ 1 로 자르고, 수가 아니면 0)
	/// - `seconds`: 떠는 길이. (초, 음수나 수가 아니면 0, 밀리초로 반올림한다)
	///
	/// # 예제
	/// 맞았을 때 0.2 초 떨기
	/// ```rust,no_run
	/// use vanilla::Engine;
	///
	/// fn shake_on_hit(engine: &mut Engine) {
	/// 	let gamepad_manager = engine.get_input_manager_mut().get_gamepad_manager_mut();
	/// 	gamepad_manager.vibrate(1.0, 0.5, 0.2);
	/// }
	/// ```
	//==============================================================================
	pub fn vibrate(&mut self, strong_magnitude: f32, weak_magnitude: f32, seconds: f32) {
		let clamped_strong_magnitude = if strong_magnitude.is_finite() {
			strong_magnitude.clamp(0.0, 1.0)
		}
		else {
			0.0
		};
		let clamped_weak_magnitude = if weak_magnitude.is_finite() {
			weak_magnitude.clamp(0.0, 1.0)
		}
		else {
			0.0
		};
		let duration_milliseconds = if seconds.is_finite() {
			(seconds.max(0.0) * 1000.0).round() as u32
		}
		else {
			0
		};
		#[cfg(target_arch = "wasm32")]
		{
			vibrate_browser_gamepads(clamped_strong_magnitude, clamped_weak_magnitude, duration_milliseconds);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.ensure_gamepad_reader();
			if let Some(gamepad_reader) = self.gamepad_reader.as_mut() {
				gamepad_reader.vibrate(clamped_strong_magnitude, clamped_weak_magnitude, duration_milliseconds);
			}
		}
	}

	//==============================================================================
	/// 모든 게임패드 갱신. (gilrs 를 읽어 update_all_gamepads_with_list 로 넘긴다)
	//==============================================================================
	pub fn update_all_gamepads(&mut self) {
		self.ensure_gamepad_reader();
		let read_result = match self.gamepad_reader.as_mut() {
			Some(gamepad_reader) => {
				gamepad_reader.read_gamepads()
			}
			None => {
				GamepadReadResult::default()
			}
		};

		// 연결이 풀린 패드. (원본 engine.js 의 gamepaddisconnected 처리 자리, 이름은 지난 사본에서 찾는다)
		for hardware_index in read_result.disconnected_gamepad_index_list.iter() {
			let previous_gamepad = self.gamepad_list.get(*hardware_index).and_then(|gamepad| gamepad.clone());
			let gamepad = match previous_gamepad {
				Some(previous_gamepad) => {
					previous_gamepad
				}
				None => {
					Gamepad::create(*hardware_index, "")
				}
			};
			self.disconnect(&gamepad);
			if self.is_log_enabled {
				write_log(&format!("gamepaddisconnected: {}", gamepad.id));
			}
		}

		// 새로 붙은 패드. (원본 engine.js 의 gamepadconnected 처리 자리)
		for hardware_index in read_result.connected_gamepad_index_list.iter() {
			if let Some(Some(gamepad)) = read_result.gamepad_list.get(*hardware_index) {
				self.connect(gamepad);
				if self.is_log_enabled {
					write_log(&format!("gamepadconnected: {}", gamepad.id));
				}
			}
		}

		self.apply_gamepad_list(read_result.gamepad_list);
	}

	//==============================================================================
	/// 장치 입력원(gilrs, 웹은 연결 리스너)을 처음 부를 때 만든다.
	//==============================================================================
	fn ensure_gamepad_reader(&mut self) {
		if !self.is_gamepad_reader_creation_tried {
			self.is_gamepad_reader_creation_tried = true;
			self.gamepad_reader = GamepadReader::create();
		}
	}

	//==============================================================================
	/// 패드 목록을 그 자리에서 다시 읽는다. (원본 navigator.getGamepads(), 목록을 넣은 관리자는 그대로)
	//==============================================================================
	fn refresh_gamepad_list(&mut self) {
		if self.is_external_list_used {
			return;
		}
		self.ensure_gamepad_reader();
		if let Some(gamepad_reader) = self.gamepad_reader.as_mut() {
			self.gamepad_list = gamepad_reader.read_gamepad_list();
		}
	}

	//==============================================================================
	/// 주어진 목록으로 모든 게임패드 갱신. (원본 updateAllGamepads 의 본문, 목록 번호가 index)
	/// - gilrs 없이 상태를 넣을 때 쓴다. (시험, 다른 입력원)
	/// - 목록에 새로 나타난 패드는 붙이고 사라진 패드는 놓는다. 값은 소수점 둘째 자리로 반올림하고 바뀐 것만 콜백에 알린다.
	///
	/// # 인자
	/// - `gamepad_list`: 패드 목록. (번호가 Gamepad.index, 빈 자리는 None, navigator.getGamepads() 와 같은 꼴)
	///
	/// # 예제
	/// 둘째 자리에 패드를 넣었다가 빼기
	/// ```rust
	/// use vanilla::{Gamepad, GamepadManager};
	///
	/// let mut gamepad_manager = GamepadManager::new();
	/// gamepad_manager.set_log_enabled(false);
	/// let gamepad = Gamepad::create(1, "Second Pad");
	/// gamepad_manager.update_all_gamepads_with_list(vec![None, Some(gamepad)]);
	/// assert_eq!(gamepad_manager.get_connected_gamepad_count(), 1);
	/// assert_eq!(gamepad_manager.get_all_connected_gamepads()[0].id, "Second Pad");
	/// gamepad_manager.update_all_gamepads_with_list(Vec::new());
	/// assert_eq!(gamepad_manager.get_connected_gamepad_count(), 0);
	/// ```
	//==============================================================================
	pub fn update_all_gamepads_with_list(&mut self, gamepad_list: Vec<Option<Gamepad>>) {
		self.is_external_list_used = true;
		self.apply_gamepad_list(gamepad_list);
	}

	//==============================================================================
	/// 목록으로 갱신하는 본문. (원본 updateAllGamepads 의 본문)
	//==============================================================================
	fn apply_gamepad_list(&mut self, gamepad_list: Vec<Option<Gamepad>>) {
		// 목록에 있는데 아직 맞이하지 않은 패드를 여기서 붙인다.
		// (원본: gamepadconnected 이벤트를 놓쳐도 매 갱신마다 훑어 보완한다)
		for gamepad in gamepad_list.iter().flatten() {
			if self.connected_gamepad_indices.contains(&gamepad.index) {
				continue;
			}
			self.connect(gamepad);
		}

		// 뽑힌 패드를 놓아준다. (disconnect 이벤트도 같은 이유로 놓칠 수 있다)
		for list_index in (0..self.connected_gamepad_indices.len()).rev() {
			let hardware_index = self.connected_gamepad_indices[list_index];
			let is_gamepad_present = matches!(gamepad_list.get(hardware_index), Some(Some(_)));
			if is_gamepad_present {
				continue;
			}
			self.connected_gamepad_indices.remove(list_index);
			self.connected_gamepad_states.remove(&hardware_index);
		}

		let is_input_log_enabled = self.is_log_enabled && self.is_input_log_enabled;
		let mut active_gamepad_id: Option<String> = None;
		for hardware_index in self.connected_gamepad_indices.iter() {
			let hardware_index = *hardware_index;
			let gamepad = match gamepad_list.get(hardware_index) {
				Some(Some(gamepad)) => {
					gamepad
				}
				_ => {
					continue;
				}
			};

			let state = match self.connected_gamepad_states.get_mut(&hardware_index) {
				Some(state) => {
					state
				}
				None => {
					continue;
				}
			};

			// 버튼이나 축 수가 늘었으면 상태 칸을 늘린다. (데스크탑의 gilrs 는 처음 움직인 뒤에야 매핑에 없는 단추와 축을 알린다)
			if state.buttons.len() < gamepad.buttons.len() {
				state.buttons.resize(gamepad.buttons.len(), GamepadButtonState { pressed: false, value: 0.0, previous_pressed: false });
			}
			if state.axes.len() < gamepad.axes.len() {
				state.axes.resize(gamepad.axes.len(), 0.0);
			}

			// 버튼 및 트리거 상태 확인.
			for (button_index, current_button) in gamepad.buttons.iter().enumerate() {
				let last_button_state = match state.buttons.get_mut(button_index) {
					Some(last_button_state) => {
						last_button_state
					}
					None => {
						continue;
					}
				};
				last_button_state.previous_pressed = last_button_state.pressed;

				// 눌림 상태 변화 감지.
				if last_button_state.pressed != current_button.pressed {
					last_button_state.pressed = current_button.pressed;
					let input_text = get_button_input_text(button_index);
					if current_button.pressed {
						active_gamepad_id = Some(gamepad.id.clone());
					}

					// 로그 출력.
					if is_input_log_enabled {
						let pressed_text = if current_button.pressed {
							"Pressed"
						}
						else {
							"Released"
						};
						write_log(&format!("[GamepadManager] Pad {} {} {} {}", hardware_index, input_text, button_index, pressed_text));
					}

					// 콜백 호출.
					if let Some(input_event_callback) = self.input_event_callback.as_mut() {
						let pressed_value = if current_button.pressed {
							1.0
						}
						else {
							0.0
						};
						input_event_callback(hardware_index, GamepadInputType::Button, button_index, pressed_value);
					}
				}

				// 아날로그 값(트리거 등) 변화 감지. (소수점 2자리까지)
				let current_value = round_to_hundredths(current_button.value as f64);
				if last_button_state.value != current_value {
					last_button_state.value = current_value;

					// 트리거 등 값이 유의미할 때만 처리.
					if current_value > 0.0 {
						let input_text = get_button_input_text(button_index);

						// 로그 출력.
						if is_input_log_enabled {
							write_log(&format!("[GamepadManager] Pad {} {} {} Value: {}", hardware_index, input_text, button_index, current_value));
						}

						// 콜백 호출.
						if let Some(input_event_callback) = self.input_event_callback.as_mut() {
							input_event_callback(hardware_index, GamepadInputType::Value, button_index, current_value as f32);
						}
					}
				}
			}

			// 아날로그 스틱(축) 상태 확인.
			for (axis_index, axis_value) in gamepad.axes.iter().enumerate() {
				let current_axis_value = round_to_hundredths(*axis_value as f64);
				let last_axis_value = match state.axes.get_mut(axis_index) {
					Some(last_axis_value) => {
						last_axis_value
					}
					None => {
						continue;
					}
				};

				// 미세한 떨림(Deadzone) 고려하여 0.01 넘는 변화만 감지.
				if (*last_axis_value - current_axis_value).abs() > AXIS_CHANGE_THRESHOLD {
					let is_last_axis_beyond = last_axis_value.abs() >= GAMEPAD_ACTIVE_AXIS_THRESHOLD;
					let is_current_axis_beyond = current_axis_value.abs() >= GAMEPAD_ACTIVE_AXIS_THRESHOLD;
					if is_current_axis_beyond && !is_last_axis_beyond {
						active_gamepad_id = Some(gamepad.id.clone());
					}
					*last_axis_value = current_axis_value;

					// 로그 출력.
					if is_input_log_enabled {
						write_log(&format!("[GamepadManager] Pad {} Axis {} Value: {}", hardware_index, axis_index, current_axis_value));
					}

					// 콜백 호출.
					if let Some(input_event_callback) = self.input_event_callback.as_mut() {
						input_event_callback(hardware_index, GamepadInputType::Axis, axis_index, current_axis_value as f32);
					}
				}
			}
		}

		self.gamepad_list = gamepad_list;
		self.active_gamepad_id = active_gamepad_id;
	}

	//==============================================================================
	/// 버튼 눌림 상태 반환.
	///
	/// # 인자
	/// - `gamepad_index`: 패드의 하드웨어 식별 번호. (Gamepad.index, 연결 목록의 순번이 아니다)
	/// - `button_index`: 버튼 번호. (GamepadButtonCode)
	///
	/// # 돌려줌
	/// 눌린 채면 true. 패드가 붙어 있지 않거나 버튼 번호가 없으면 false.
	//==============================================================================
	pub fn is_button_pressed(&self, gamepad_index: usize, button_index: usize) -> bool {
		match self.find_button_state(gamepad_index, button_index) {
			Some(button_state) => {
				button_state.pressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 이번 갱신에 방금 눌린 버튼인지. (원본 isButtonJustPressed, 키보드 is_key_just_pressed 와 같은 쓰임)
	///
	/// # 인자
	/// - `gamepad_index`: 패드의 하드웨어 식별 번호. (Gamepad.index, 연결 목록의 순번이 아니다)
	/// - `button_index`: 버튼 번호. (GamepadButtonCode)
	///
	/// # 돌려줌
	/// 지난 갱신에 떼어 있다가 이번 갱신에 눌렸으면 true. 패드나 버튼이 없으면 false.
	//==============================================================================
	pub fn is_button_just_pressed(&self, gamepad_index: usize, button_index: usize) -> bool {
		match self.find_button_state(gamepad_index, button_index) {
			Some(button_state) => {
				button_state.pressed && !button_state.previous_pressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 이번 갱신에 방금 떼어진 버튼인지. (원본 isButtonJustReleased)
	///
	/// # 인자
	/// - `gamepad_index`: 패드의 하드웨어 식별 번호. (Gamepad.index, 연결 목록의 순번이 아니다)
	/// - `button_index`: 버튼 번호. (GamepadButtonCode)
	///
	/// # 돌려줌
	/// 지난 갱신에 눌려 있다가 이번 갱신에 떼어졌으면 true. 패드나 버튼이 없으면 false.
	//==============================================================================
	pub fn is_button_just_released(&self, gamepad_index: usize, button_index: usize) -> bool {
		match self.find_button_state(gamepad_index, button_index) {
			Some(button_state) => {
				!button_state.pressed && button_state.previous_pressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 버튼/트리거 아날로그 값 반환. (0.0 ~ 1.0)
	///
	/// # 인자
	/// - `gamepad_index`: 패드의 하드웨어 식별 번호. (Gamepad.index, 연결 목록의 순번이 아니다)
	/// - `button_index`: 버튼 번호. (GamepadButtonCode)
	///
	/// # 돌려줌
	/// 소수점 둘째 자리로 반올림한 값. 패드나 버튼이 없으면 0.0.
	//==============================================================================
	pub fn get_button_value(&self, gamepad_index: usize, button_index: usize) -> f32 {
		match self.find_button_state(gamepad_index, button_index) {
			Some(button_state) => {
				button_state.value as f32
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 아날로그 스틱 축 값 반환. (-1.0 ~ 1.0)
	/// - 0.01 넘게 바뀐 때만 새 값을 받는다. 죽은 구역은 빼지 않는다. (InputActions 가 뺀다)
	///
	/// # 인자
	/// - `gamepad_index`: 패드의 하드웨어 식별 번호. (Gamepad.index)
	/// - `axis_index`: 축 번호. (GamepadAnalogStickCode, 세로 축은 아래가 양수)
	///
	/// # 돌려줌
	/// 소수점 둘째 자리로 반올림한 값. 패드나 축이 없거나 NaN 이면 0.0.
	//==============================================================================
	pub fn get_axis_value(&self, gamepad_index: usize, axis_index: usize) -> f32 {
		let state = match self.connected_gamepad_states.get(&gamepad_index) {
			Some(state) => {
				state
			}
			None => {
				return 0.0;
			}
		};
		match state.axes.get(axis_index) {
			Some(axis_value) => {
				zero_if_falsy(*axis_value) as f32
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 연결된 모든 게임패드 반환. (원본처럼 그 자리에서 읽은 지금 상태)
	///
	/// # 돌려줌
	/// 붙은 차례의 패드 목록. 없으면 빈 목록.
	//==============================================================================
	pub fn get_all_connected_gamepads(&mut self) -> Vec<&Gamepad> {
		self.refresh_gamepad_list();
		let mut connected_gamepad_list = Vec::new();
		for hardware_index in self.connected_gamepad_indices.iter() {
			if let Some(Some(gamepad)) = self.gamepad_list.get(*hardware_index) {
				connected_gamepad_list.push(gamepad);
			}
		}
		connected_gamepad_list
	}

	//==============================================================================
	/// 연결된 게임패드 수 반환.
	///
	/// # 돌려줌
	/// 붙어 있는 패드 수.
	//==============================================================================
	pub fn get_connected_gamepad_count(&self) -> usize {
		self.connected_gamepad_indices.len()
	}

	//==============================================================================
	/// 연결된 게임패드 반환. (gamepad_index 는 연결 목록의 순번, 0 ~ get_connected_gamepad_count() - 1)
	///
	/// # 인자
	/// - `gamepad_index`: 연결 목록의 순번. (붙은 차례, Gamepad.index 가 아니다)
	///
	/// # 돌려줌
	/// 그 자리에서 읽은 패드. 순번이 범위를 넘거나 그 패드가 지금 목록에 없으면 None.
	//==============================================================================
	pub fn get_connected_gamepad(&mut self, gamepad_index: usize) -> Option<&Gamepad> {
		if gamepad_index >= self.connected_gamepad_indices.len() {
			return None;
		}
		self.refresh_gamepad_list();
		let hardware_index = *self.connected_gamepad_indices.get(gamepad_index)?;
		match self.gamepad_list.get(hardware_index) {
			Some(Some(gamepad)) => {
				Some(gamepad)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 버튼 상태 찾기.
	//==============================================================================
	fn find_button_state(&self, gamepad_index: usize, button_index: usize) -> Option<&GamepadButtonState> {
		let state = self.connected_gamepad_states.get(&gamepad_index)?;
		state.buttons.get(button_index)
	}
}

impl Default for GamepadManager {
	//==============================================================================
	/// 기본값. (GamepadManager::new 와 같다)
	//==============================================================================
	fn default() -> GamepadManager {
		GamepadManager::new()
	}
}

//==============================================================================
/// 로그 문구의 입력 이름. (L2, R2 는 Trigger)
//==============================================================================
fn get_button_input_text(button_index: usize) -> &'static str {
	if button_index == GamepadButtonCode::L2 || button_index == GamepadButtonCode::R2 {
		"Trigger"
	}
	else {
		"Button"
	}
}

//==============================================================================
/// 소수점 둘째 자리로 반올림. (원본 parseFloat(value.toFixed(2)) 자리)
//==============================================================================
fn round_to_hundredths(value: f64) -> f64 {
	(value * 100.0).round() / 100.0
}

//==============================================================================
/// 0, -0, NaN 을 0 으로 바꾼다. (원본 `value || 0` 자리)
//==============================================================================
fn zero_if_falsy(value: f64) -> f64 {
	if value == 0.0 || value.is_nan() {
		0.0
	}
	else {
		value
	}
}

//==============================================================================
/// 로그 출력. (원본 console.log 자리. 웹은 브라우저 콘솔, 데스크탑은 표준 오류)
//==============================================================================
fn write_log(message: &str) {
	#[cfg(target_arch = "wasm32")]
	{
		let message_value = wasm_bindgen::JsValue::from_str(message);
		web_sys::console::log_1(&message_value);
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		eprintln!("{}", message);
	}
}

//==============================================================================
/// 브라우저 패드 떨기. (원본처럼 navigator.getGamepads() 의 vibrationActuator.playEffect("dual-rumble"), 없으면 건너뛴다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn vibrate_browser_gamepads(strong_magnitude: f32, weak_magnitude: f32, duration_milliseconds: u32) {
	use wasm_bindgen::{JsCast, JsValue};
	let window = match web_sys::window() {
		Some(window) => {
			window
		}
		None => {
			return;
		}
	};
	let navigator = window.navigator();
	let get_gamepads_function: js_sys::Function = match js_sys::Reflect::get(&navigator, &JsValue::from_str("getGamepads")).ok().and_then(|value| value.dyn_into().ok()) {
		Some(get_gamepads_function) => {
			get_gamepads_function
		}
		None => {
			return;
		}
	};
	let gamepad_array: js_sys::Array = match get_gamepads_function.call0(&navigator).ok().and_then(|value| js_sys::Array::try_from(value).ok()) {
		Some(gamepad_array) => {
			gamepad_array
		}
		None => {
			return;
		}
	};
	let effect_parameters = js_sys::Object::new();
	let _ = js_sys::Reflect::set(&effect_parameters, &JsValue::from_str("duration"), &JsValue::from_f64(duration_milliseconds as f64));
	let _ = js_sys::Reflect::set(&effect_parameters, &JsValue::from_str("strongMagnitude"), &JsValue::from_f64(strong_magnitude as f64));
	let _ = js_sys::Reflect::set(&effect_parameters, &JsValue::from_str("weakMagnitude"), &JsValue::from_f64(weak_magnitude as f64));
	for gamepad in gamepad_array.iter() {
		if gamepad.is_null() || gamepad.is_undefined() {
			continue;
		}
		let vibration_actuator = match js_sys::Reflect::get(&gamepad, &JsValue::from_str("vibrationActuator")) {
			Ok(vibration_actuator) => {
				vibration_actuator
			}
			Err(_) => {
				continue;
			}
		};
		if vibration_actuator.is_null() || vibration_actuator.is_undefined() {
			continue;
		}
		let play_effect_function: js_sys::Function = match js_sys::Reflect::get(&vibration_actuator, &JsValue::from_str("playEffect")).ok().and_then(|value| value.dyn_into().ok()) {
			Some(play_effect_function) => {
				play_effect_function
			}
			None => {
				continue;
			}
		};
		// 앞 진동이 끝나기 전에 부르면 앞 약속이 거절된다. 이어서 떠는 것이라 흘린다.
		if let Ok(effect_promise) = play_effect_function.call2(&vibration_actuator, &JsValue::from_str("dual-rumble"), &effect_parameters) {
			if let Ok(effect_promise) = effect_promise.dyn_into::<js_sys::Promise>() {
				let ignore_rejection: wasm_bindgen::closure::Closure<dyn FnMut(JsValue)> = wasm_bindgen::closure::Closure::new(|_: JsValue| {
				});
				let _ = effect_promise.catch(&ignore_rejection);
				ignore_rejection.forget();
			}
		}
	}
}

//==============================================================================
/// 패드 하나에서 버튼과 축을 읽는 것. (gilrs 패드, 시험용 가짜 패드)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
trait GamepadElementReader {
	/// 버튼의 눌림과 값.
	fn read_button(&self, button: Button) -> GamepadButton;
	/// 축의 값. (gilrs 꼴, Y 는 위가 양수)
	fn read_axis(&self, axis: Axis) -> f32;
}

#[cfg(not(target_arch = "wasm32"))]
impl GamepadElementReader for gilrs::Gamepad<'_> {
	//==============================================================================
	/// 버튼 읽기. (gilrs 가 모르는 버튼은 뗀 상태)
	//==============================================================================
	fn read_button(&self, button: Button) -> GamepadButton {
		match self.button_data(button) {
			Some(button_data) => {
				GamepadButton {
					pressed: button_data.is_pressed(),
					value: button_data.value(),
				}
			}
			None => {
				GamepadButton {
					pressed: false,
					value: 0.0,
				}
			}
		}
	}

	//==============================================================================
	/// 축 읽기. (gilrs 가 모르는 축은 0)
	//==============================================================================
	fn read_axis(&self, axis: Axis) -> f32 {
		self.value(axis)
	}
}

//==============================================================================
/// 십자키 버튼에 겹쳐 읽을 축과 방향. (햇 스위치를 축으로 주는 패드. gilrs DPadX 는 오른쪽, DPadY 는 위가 양수)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn get_directional_pad_axis(button_index: usize) -> Option<(Axis, f32)> {
	match button_index {
		GamepadButtonCode::DPAD_UP => {
			Some((Axis::DPadY, 1.0))
		}
		GamepadButtonCode::DPAD_DOWN => {
			Some((Axis::DPadY, -1.0))
		}
		GamepadButtonCode::DPAD_LEFT => {
			Some((Axis::DPadX, -1.0))
		}
		GamepadButtonCode::DPAD_RIGHT => {
			Some((Axis::DPadX, 1.0))
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 패드 하나를 W3C Standard Gamepad 꼴로 읽는다.
/// - 아날로그 버튼은 값이 30/255 를 넘으면 눌림. 눌린 디지털 버튼의 값은 1.
/// - 축은 Y 를 뒤집어 아래가 양수가 되게 한다.
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn create_standard_gamepad(index: usize, id: &str, element_reader: &impl GamepadElementReader) -> Gamepad {
	let mut gamepad = Gamepad::create(index, id);
	for (button_index, standard_button) in STANDARD_BUTTON_LIST.iter().enumerate() {
		let raw_button = element_reader.read_button(*standard_button);
		let mut is_pressed = raw_button.pressed || raw_button.value > BUTTON_PRESSED_THRESHOLD;
		let mut button_value = raw_button.value.clamp(0.0, 1.0);
		if let Some((directional_pad_axis, direction)) = get_directional_pad_axis(button_index) {
			let directional_pad_axis_value = element_reader.read_axis(directional_pad_axis);
			if directional_pad_axis_value * direction > DIRECTIONAL_PAD_AXIS_THRESHOLD {
				is_pressed = true;
			}
		}
		if is_pressed && button_value <= 0.0 {
			button_value = 1.0;
		}
		gamepad.buttons[button_index] = GamepadButton {
			pressed: is_pressed,
			value: button_value,
		};
	}
	for (axis_index, (standard_axis, direction)) in STANDARD_AXIS_LIST.iter().enumerate() {
		let axis_value = element_reader.read_axis(*standard_axis) * direction;
		gamepad.axes[axis_index] = zero_if_falsy(axis_value as f64) as f32;
	}
	gamepad
}

//==============================================================================
/// 데스크탑 패드 이름. (크롬 id 꼴. 게임이 "054c" 같은 제조사 번호로 패드 종류를 가른다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn create_desktop_gamepad_identifier(gamepad_name: &str, vendor_id: Option<u16>, product_id: Option<u16>) -> String {
	match (vendor_id, product_id) {
		(Some(vendor_id), Some(product_id)) => {
			format!("{} (STANDARD GAMEPAD Vendor: {:04x} Product: {:04x})", gamepad_name, vendor_id, product_id)
		}
		_ => {
			format!("{} (STANDARD GAMEPAD)", gamepad_name)
		}
	}
}

//==============================================================================
/// gilrs 패드를 Gamepad 로. (표준 17 버튼, 4 축)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn create_gamepad_from_gilrs(gamepad_id: gilrs::GamepadId, gilrs_gamepad: &gilrs::Gamepad) -> Gamepad {
	let index = usize::from(gamepad_id);
	let vendor_id = gilrs_gamepad.vendor_id();
	let product_id = gilrs_gamepad.product_id();
	let id = create_desktop_gamepad_identifier(gilrs_gamepad.name(), vendor_id, product_id);
	let mut gamepad = create_standard_gamepad(index, &id, gilrs_gamepad);
	gamepad.connected = gilrs_gamepad.is_connected();
	gamepad
}

//==============================================================================
/// gilrs 의 단추 코드가 표준 버튼 뒤에 붙일 단추인지. (매핑에 없거나 표준 17 개가 아닌 gilrs 단추, 축으로 매핑된 것은 아니다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn is_extra_button_code(gilrs_gamepad: &gilrs::Gamepad, code: gilrs::ev::Code) -> bool {
	match gilrs_gamepad.axis_or_btn_name(code) {
		None => {
			true
		}
		Some(gilrs::ev::AxisOrBtn::Btn(button)) => {
			!STANDARD_BUTTON_LIST.contains(&button)
		}
		Some(gilrs::ev::AxisOrBtn::Axis(_)) => {
			false
		}
	}
}

//==============================================================================
/// gilrs 의 축 코드가 표준 축 뒤에 붙일 축인지. (매핑에 없는 축, 트리거와 십자키 축은 버튼으로 읽으므로 아니다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn is_extra_axis_code(gilrs_gamepad: &gilrs::Gamepad, code: gilrs::ev::Code) -> bool {
	match gilrs_gamepad.axis_or_btn_name(code) {
		None => {
			true
		}
		Some(gilrs::ev::AxisOrBtn::Axis(axis)) => {
			axis == Axis::Unknown
		}
		Some(gilrs::ev::AxisOrBtn::Btn(_)) => {
			false
		}
	}
}

//==============================================================================
/// 입력원에서 이번 갱신에 읽은 것.
//==============================================================================
#[derive(Default)]
struct GamepadReadResult {
	/// 이번에 붙은 패드 index.
	connected_gamepad_index_list: Vec<usize>,
	/// 이번에 떨어진 패드 index.
	disconnected_gamepad_index_list: Vec<usize>,
	/// 연결된 패드 목록. (번호가 index, 빈 칸은 None)
	gamepad_list: Vec<Option<Gamepad>>,
}

//==============================================================================
/// gilrs 입력원. (데스크탑, gilrs 를 들고 W3C 꼴 목록을 만든다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
struct GamepadReader {
	gilrs: gilrs::Gilrs,
	/// 떨고 있는 진동과 끝나는 때. (gilrs 는 효과 손잡이를 놓으면 바로 멈추므로 끝날 때까지 든다)
	vibration_effect_list: Vec<(gilrs::ff::Effect, web_time::Instant)>,
	/// 패드 index 마다 표준 버튼 뒤에 붙인 단추 코드. (처음 움직인 차례, 번호가 17 부터의 버튼 번호)
	extra_button_code_table: HashMap<usize, Vec<gilrs::ev::Code>>,
	/// 패드 index 마다 표준 축 뒤에 붙인 축 코드. (처음 움직인 차례, 번호가 4 부터의 축 번호)
	extra_axis_code_table: HashMap<usize, Vec<gilrs::ev::Code>>,
	/// 다음 갱신에 넘길 연결과 해제. (목록을 그 자리에서 읽을 때 비운 사건)
	pending_read_result: GamepadReadResult,
}

#[cfg(not(target_arch = "wasm32"))]
impl GamepadReader {
	//==============================================================================
	/// 생성. (기본 필터는 끄고 진동은 켠다. 게임패드를 모르는 플랫폼이면 패드가 없는 것처럼 돈다)
	//==============================================================================
	fn create() -> Option<GamepadReader> {
		let gilrs_builder = GilrsBuilder::new().with_default_filters(false).with_force_feedback(true);
		match gilrs_builder.build() {
			Ok(gilrs) => {
				Some(GamepadReader::from_gilrs(gilrs))
			}
			Err(gilrs::Error::NotImplemented(gilrs)) => {
				write_log("[GamepadManager] 이 플랫폼은 게임패드를 지원하지 않습니다.");
				Some(GamepadReader::from_gilrs(gilrs))
			}
			Err(error) => {
				write_log(&format!("[GamepadManager] gilrs 를 만들지 못했습니다: {}", error));
				None
			}
		}
	}

	//==============================================================================
	/// gilrs 로 만들기.
	//==============================================================================
	fn from_gilrs(gilrs: gilrs::Gilrs) -> GamepadReader {
		GamepadReader {
			gilrs,
			vibration_effect_list: Vec::new(),
			extra_button_code_table: HashMap::new(),
			extra_axis_code_table: HashMap::new(),
			pending_read_result: GamepadReadResult::default(),
		}
	}

	//==============================================================================
	/// 연결된 패드를 모두 떨기. (macOS: gilrs 패드의 OS 이름을 연결 차례로 넘겨 플랫폼 층이 짝인 GCController 를 떤다)
	//==============================================================================
	#[cfg(target_os = "macos")]
	fn vibrate(&mut self, strong_magnitude: f32, weak_magnitude: f32, duration_milliseconds: u32) {
		let gamepad_name_list: Vec<String> = self.gilrs.gamepads().map(|(_, gilrs_gamepad)| gilrs_gamepad.os_name().to_string()).collect();
		crate::platform::vibrate_gamepads(&gamepad_name_list, strong_magnitude, weak_magnitude, duration_milliseconds);
	}

	//==============================================================================
	/// 진동을 대는 연결된 패드를 모두 떨기. (gilrs force feedback, 무거운 모터 Strong 과 가벼운 모터 Weak)
	//==============================================================================
	#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
	fn vibrate(&mut self, strong_magnitude: f32, weak_magnitude: f32, duration_milliseconds: u32) {
		use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Repeat, Replay, Ticks};
		let gamepad_id_list: Vec<gilrs::GamepadId> = self.gilrs.gamepads().filter(|(_, gilrs_gamepad)| gilrs_gamepad.is_ff_supported()).map(|(gamepad_id, _)| gamepad_id).collect();
		if gamepad_id_list.is_empty() || duration_milliseconds == 0 {
			return;
		}
		let play_ticks = Ticks::from_ms(duration_milliseconds);
		let scheduling = Replay {
			play_for: play_ticks,
			..Default::default()
		};
		let strong_effect = BaseEffect {
			kind: BaseEffectType::Strong { magnitude: (strong_magnitude * u16::MAX as f32) as u16 },
			scheduling,
			..Default::default()
		};
		let weak_effect = BaseEffect {
			kind: BaseEffectType::Weak { magnitude: (weak_magnitude * u16::MAX as f32) as u16 },
			scheduling,
			..Default::default()
		};
		let effect_result = EffectBuilder::new().add_effect(strong_effect).add_effect(weak_effect).gamepads(&gamepad_id_list).repeat(Repeat::For(play_ticks)).finish(&mut self.gilrs);
		match effect_result {
			Ok(effect) => {
				if effect.play().is_ok() {
					let end_instant = web_time::Instant::now() + std::time::Duration::from_millis(duration_milliseconds as u64);
					self.vibration_effect_list.push((effect, end_instant));
				}
			}
			Err(error) => {
				write_log(&format!("[GamepadManager] 진동을 만들지 못했습니다: {}", error));
			}
		}
	}

	//==============================================================================
	/// 지금 목록 읽기. (이벤트를 비워야 gilrs 의 상태가 갱신된다, 연결과 해제는 다음 read_gamepads 에 넘긴다, 끝난 진동은 놓는다)
	//==============================================================================
	fn read_gamepad_list(&mut self) -> Vec<Option<Gamepad>> {
		let now = web_time::Instant::now();
		self.vibration_effect_list.retain(|(_, end_instant)| *end_instant > now);
		while let Some(event) = self.gilrs.next_event() {
			let hardware_index = usize::from(event.id);
			match event.event {
				gilrs::EventType::Connected => {
					self.pending_read_result.connected_gamepad_index_list.push(hardware_index);
				}
				gilrs::EventType::Disconnected => {
					self.pending_read_result.disconnected_gamepad_index_list.push(hardware_index);
					self.extra_button_code_table.remove(&hardware_index);
					self.extra_axis_code_table.remove(&hardware_index);
					#[cfg(target_os = "macos")]
					{
						crate::platform::release_disconnected_gamepad_haptics();
					}
				}
				_ => {
				}
			}
		}
		let mut gamepad_list: Vec<Option<Gamepad>> = Vec::new();
		for (gamepad_id, gilrs_gamepad) in self.gilrs.gamepads() {
			let mut gamepad = create_gamepad_from_gilrs(gamepad_id, &gilrs_gamepad);
			let hardware_index = gamepad.index;
			append_extra_elements(&mut self.extra_button_code_table, &mut self.extra_axis_code_table, &mut gamepad, &gilrs_gamepad);
			if gamepad_list.len() <= hardware_index {
				gamepad_list.resize(hardware_index + 1, None);
			}
			gamepad_list[hardware_index] = Some(gamepad);
		}
		gamepad_list
	}

	//==============================================================================
	/// 갱신 읽기. (지금 목록과, 지난 갱신 뒤의 연결과 해제)
	//==============================================================================
	fn read_gamepads(&mut self) -> GamepadReadResult {
		let gamepad_list = self.read_gamepad_list();
		let mut read_result = std::mem::take(&mut self.pending_read_result);
		read_result.gamepad_list = gamepad_list;
		read_result
	}
}

//==============================================================================
/// 플레이스테이션 패드의 터치패드 단추 자리. (크롬 MapperDualshock4, MapperDualSense, MapperPs4Ps5)
/// - 플레이스테이션 패드가 아니면 None. 맞으면 Some 이고, 안의 값은 터치패드 단추의 gilrs 원시 코드(Code::into_u32)다. 터치패드 단추가 없는 장치면 Some(None).
/// - operating_system 은 std::env::consts::OS 꼴이다. (시험이 플랫폼마다 부른다) hid_version 은 gilrs uuid 의 판 번호(리눅스 input_id.version, bcdHID)다.
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn find_playstation_touchpad_code(operating_system: &str, vendor_id: Option<u16>, product_id: Option<u16>, hid_version: u16) -> Option<Option<u32>> {
	if vendor_id != Some(PLAYSTATION_VENDOR_ID) {
		return None;
	}
	if !product_id.map(|product_id| PLAYSTATION_TOUCHPAD_PRODUCT_ID_LIST.contains(&product_id)).unwrap_or(false) {
		return None;
	}
	match operating_system {
		"macos" => {
			// IOKit 단추 쪽의 쓰임은 1 부터다. (gilrs 가 쓰임 차례로 단추 목록을 지어 b13 이 쓰임 14)
			Some(Some((HID_BUTTON_USAGE_PAGE << 16) | (PLAYSTATION_TOUCHPAD_RAW_BUTTON_INDEX + 1)))
		}
		"windows" => {
			// Windows.Gaming.Input RawGameController 의 단추 번호. (gilrs 의 단추 종류는 0)
			Some(Some(PLAYSTATION_TOUCHPAD_RAW_BUTTON_INDEX))
		}
		"linux" => {
			if (hid_version & LINUX_PATCHED_HID_VERSION_MASK) != 0 {
				return Some(None);
			}
			// hid-generic 은 게임패드 HID 단추 n 을 BTN_GAMEPAD + n - 1 로 낸다. (gilrs 와 joydev 가 코드 차례로 단추 목록을 지어 b13 이 BTN_GAMEPAD + 13)
			Some(Some((LINUX_EVENT_KEY_TYPE << 16) | (LINUX_FIRST_GAMEPAD_BUTTON_CODE + PLAYSTATION_TOUCHPAD_RAW_BUTTON_INDEX)))
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// gilrs uuid 의 판 번호. (SDL GUID 꼴, 13, 14번째 바이트가 작은 쪽부터의 판 번호)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_uuid_version(uuid: &[u8; 16]) -> u16 {
	u16::from_le_bytes([uuid[12], uuid[13]])
}

//==============================================================================
/// gilrs 단추 상태를 W3C 버튼으로. (표준 버튼처럼 값이 30/255 를 넘으면 눌림, 눌린 디지털 단추의 값은 1)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn convert_button_data(button_data: Option<&gilrs::ev::state::ButtonData>) -> GamepadButton {
	let (raw_pressed, raw_value) = match button_data {
		Some(button_data) => {
			(button_data.is_pressed(), button_data.value())
		}
		None => {
			(false, 0.0)
		}
	};
	let is_pressed = raw_pressed || raw_value > BUTTON_PRESSED_THRESHOLD;
	let mut button_value = raw_value.clamp(0.0, 1.0);
	if is_pressed && button_value <= 0.0 {
		button_value = 1.0;
	}
	GamepadButton {
		pressed: is_pressed,
		value: button_value,
	}
}

//==============================================================================
/// 표준 버튼과 축 뒤에 gilrs 가 알린 나머지 단추와 축 붙이기. (처음 움직인 차례로 번호를 매겨 그 뒤로는 번호가 바뀌지 않는다)
/// - 단추는 표준 버튼처럼 값이 30/255 를 넘으면 눌림, 눌린 디지털 단추의 값은 1 이다.
/// - 플레이스테이션 패드는 크롬처럼 터치패드 단추 하나만 TOUCHPAD(17) 에 붙인다. (누른 적이 없으면 뗀 상태, 터치패드가 없는 장치면 붙이지 않는다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn append_extra_elements(extra_button_code_table: &mut HashMap<usize, Vec<gilrs::ev::Code>>, extra_axis_code_table: &mut HashMap<usize, Vec<gilrs::ev::Code>>, gamepad: &mut Gamepad, gilrs_gamepad: &gilrs::Gamepad) {
	let gamepad_state = gilrs_gamepad.state();
	let hid_version = read_uuid_version(&gilrs_gamepad.uuid());
	if let Some(touchpad_code_value) = find_playstation_touchpad_code(std::env::consts::OS, gilrs_gamepad.vendor_id(), gilrs_gamepad.product_id(), hid_version) {
		if let Some(touchpad_code_value) = touchpad_code_value {
			let touchpad_button_data = gamepad_state.buttons().find(|(code, _)| code.into_u32() == touchpad_code_value).map(|(_, button_data)| button_data);
			gamepad.buttons.push(convert_button_data(touchpad_button_data));
		}
		return;
	}
	let extra_button_code_list = extra_button_code_table.entry(gamepad.index).or_default();
	let mut new_button_code_list: Vec<gilrs::ev::Code> = gamepad_state.buttons().map(|(code, _)| code).filter(|code| is_extra_button_code(gilrs_gamepad, *code) && !extra_button_code_list.contains(code)).collect();
	new_button_code_list.sort_by_key(|code| code.into_u32());
	extra_button_code_list.extend(new_button_code_list);
	for code in extra_button_code_list.iter() {
		gamepad.buttons.push(convert_button_data(gamepad_state.button_data(*code)));
	}
	let extra_axis_code_list = extra_axis_code_table.entry(gamepad.index).or_default();
	let mut new_axis_code_list: Vec<gilrs::ev::Code> = gamepad_state.axes().map(|(code, _)| code).filter(|code| is_extra_axis_code(gilrs_gamepad, *code) && !extra_axis_code_list.contains(code)).collect();
	new_axis_code_list.sort_by_key(|code| code.into_u32());
	extra_axis_code_list.extend(new_axis_code_list);
	for code in extra_axis_code_list.iter() {
		let axis_value = gamepad_state.axis_data(*code).map(|axis_data| axis_data.value()).unwrap_or(0.0);
		gamepad.axes.push(zero_if_falsy(axis_value as f64) as f32);
	}
}

//==============================================================================
/// 브라우저 입력원. (웹, 원본처럼 navigator.getGamepads() 를 직접 읽는다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
struct GamepadReader {
	/// gamepadconnected / gamepaddisconnected 로 받은 패드 index 와 붙었는지. (리스너가 넣고 읽을 때 꺼낸다)
	connection_event_list: std::rc::Rc<std::cell::RefCell<Vec<(usize, bool)>>>,
}

#[cfg(target_arch = "wasm32")]
impl GamepadReader {
	//==============================================================================
	/// 생성. (원본 engine.js 처럼 창에 gamepadconnected / gamepaddisconnected 리스너를 단다. 리스너는 페이지가 살아 있는 동안 산다)
	//==============================================================================
	fn create() -> Option<GamepadReader> {
		use wasm_bindgen::JsCast;
		let window = web_sys::window()?;
		let connection_event_list: std::rc::Rc<std::cell::RefCell<Vec<(usize, bool)>>> = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
		for (event_name, is_connected) in [("gamepadconnected", true), ("gamepaddisconnected", false)] {
			let connection_event_list_for_listener = connection_event_list.clone();
			let connection_listener: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)> = wasm_bindgen::closure::Closure::new(move |event: web_sys::Event| {
				let gamepad = js_sys::Reflect::get(&event, &wasm_bindgen::JsValue::from_str("gamepad")).unwrap_or(wasm_bindgen::JsValue::NULL);
				if let Some(hardware_index) = js_sys::Reflect::get(&gamepad, &wasm_bindgen::JsValue::from_str("index")).ok().and_then(|value| value.as_f64()) {
					connection_event_list_for_listener.borrow_mut().push((hardware_index as usize, is_connected));
				}
			});
			if window.add_event_listener_with_callback(event_name, connection_listener.as_ref().unchecked_ref()).is_err() {
				write_log(&format!("[GamepadManager] {} 리스너를 달지 못했습니다.", event_name));
			}
			connection_listener.forget();
		}
		Some(GamepadReader {
			connection_event_list,
		})
	}

	//==============================================================================
	/// 읽기. (연결 이벤트를 꺼내고, navigator.getGamepads() 의 패드마다 index, id, connected, 버튼 모두, 축 모두를 그대로 옮긴다)
	//==============================================================================
	fn read_gamepads(&mut self) -> GamepadReadResult {
		let mut read_result = GamepadReadResult::default();
		for (hardware_index, is_connected) in self.connection_event_list.borrow_mut().drain(..) {
			if is_connected {
				read_result.connected_gamepad_index_list.push(hardware_index);
			}
			else {
				read_result.disconnected_gamepad_index_list.push(hardware_index);
			}
		}
		read_result.gamepad_list = self.read_gamepad_list();
		read_result
	}

	//==============================================================================
	/// 지금 목록 읽기. (navigator.getGamepads(), 번호가 index)
	//==============================================================================
	fn read_gamepad_list(&mut self) -> Vec<Option<Gamepad>> {
		let mut gamepad_list: Vec<Option<Gamepad>> = Vec::new();
		for gamepad in read_browser_gamepads().into_iter().flatten() {
			let hardware_index = gamepad.index;
			if gamepad_list.len() <= hardware_index {
				gamepad_list.resize(hardware_index + 1, None);
			}
			gamepad_list[hardware_index] = Some(gamepad);
		}
		gamepad_list
	}
}

//==============================================================================
/// navigator.getGamepads() 읽기. (빈 칸은 None, 브라우저가 주는 버튼과 축을 모두 옮긴다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_browser_gamepads() -> Vec<Option<Gamepad>> {
	use wasm_bindgen::{JsCast, JsValue};
	let mut gamepad_list: Vec<Option<Gamepad>> = Vec::new();
	let navigator = match web_sys::window() {
		Some(window) => {
			window.navigator()
		}
		None => {
			return gamepad_list;
		}
	};
	let get_gamepads_function: js_sys::Function = match js_sys::Reflect::get(&navigator, &JsValue::from_str("getGamepads")).ok().and_then(|value| value.dyn_into().ok()) {
		Some(get_gamepads_function) => {
			get_gamepads_function
		}
		None => {
			return gamepad_list;
		}
	};
	let gamepad_array: js_sys::Array = match get_gamepads_function.call0(&navigator).ok().and_then(|value| js_sys::Array::try_from(value).ok()) {
		Some(gamepad_array) => {
			gamepad_array
		}
		None => {
			return gamepad_list;
		}
	};
	let read_property = |object: &JsValue, name: &str| -> JsValue {
		js_sys::Reflect::get(object, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED)
	};
	for browser_gamepad in gamepad_array.iter() {
		if browser_gamepad.is_null() || browser_gamepad.is_undefined() {
			gamepad_list.push(None);
			continue;
		}
		let index = read_property(&browser_gamepad, "index").as_f64().unwrap_or(0.0) as usize;
		let id = read_property(&browser_gamepad, "id").as_string().unwrap_or_default();
		let mut gamepad = Gamepad {
			index,
			id,
			connected: read_property(&browser_gamepad, "connected").as_bool().unwrap_or(false),
			buttons: Vec::new(),
			axes: Vec::new(),
		};
		if let Ok(button_array) = read_property(&browser_gamepad, "buttons").dyn_into::<js_sys::Array>() {
			for browser_button in button_array.iter() {
				gamepad.buttons.push(GamepadButton {
					pressed: read_property(&browser_button, "pressed").as_bool().unwrap_or(false),
					value: read_property(&browser_button, "value").as_f64().unwrap_or(0.0) as f32,
				});
			}
		}
		if let Ok(axis_array) = read_property(&browser_gamepad, "axes").dyn_into::<js_sys::Array>() {
			for axis_value in axis_array.iter() {
				gamepad.axes.push(axis_value.as_f64().unwrap_or(0.0) as f32);
			}
		}
		gamepad_list.push(Some(gamepad));
	}
	gamepad_list
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	/// 콜백으로 받은 입력 하나. (gamepad_index, input_type, input_index, value)
	type ReceivedInputEvent = (usize, GamepadInputType, usize, f32);

	//==============================================================================
	/// 시험용 가짜 패드. (눌린 버튼과 값, 축 값을 직접 준다)
	//==============================================================================
	struct TestElementReader {
		button_list: Vec<(Button, GamepadButton)>,
		axis_list: Vec<(Axis, f32)>,
	}

	impl GamepadElementReader for TestElementReader {
		//==============================================================================
		/// 버튼 읽기.
		//==============================================================================
		fn read_button(&self, button: Button) -> GamepadButton {
			for (listed_button, listed_state) in self.button_list.iter() {
				if *listed_button == button {
					return *listed_state;
				}
			}
			GamepadButton {
				pressed: false,
				value: 0.0,
			}
		}

		//==============================================================================
		/// 축 읽기.
		//==============================================================================
		fn read_axis(&self, axis: Axis) -> f32 {
			for (listed_axis, listed_value) in self.axis_list.iter() {
				if *listed_axis == axis {
					return *listed_value;
				}
			}
			0.0
		}
	}

	//==============================================================================
	/// 가짜 패드에서 버튼 하나만 디지털로 눌러 읽는다.
	//==============================================================================
	fn read_with_pressed_button(button: Button) -> Gamepad {
		let element_reader = TestElementReader {
			button_list: vec![(button, GamepadButton { pressed: true, value: 1.0 })],
			axis_list: Vec::new(),
		};
		create_standard_gamepad(0, "test", &element_reader)
	}

	//==============================================================================
	/// 시험용 관리자. (gilrs 없이, 로그 끔)
	//==============================================================================
	fn create_test_manager() -> GamepadManager {
		let mut gamepad_manager = GamepadManager::new();
		gamepad_manager.set_log_enabled(false);
		gamepad_manager
	}

	//==============================================================================
	/// gilrs 버튼이 W3C 번호(원본 GamepadButtonCode)로 간다.
	//==============================================================================
	#[test]
	fn button_index_mapping() {
		let expected_mapping_list = [
			(Button::South, GamepadButtonCode::A_CROSS),
			(Button::East, GamepadButtonCode::B_CIRCLE),
			(Button::West, GamepadButtonCode::X_SQUARE),
			(Button::North, GamepadButtonCode::Y_TRIANGLE),
			(Button::LeftTrigger, GamepadButtonCode::L1),
			(Button::RightTrigger, GamepadButtonCode::R1),
			(Button::LeftTrigger2, GamepadButtonCode::L2),
			(Button::RightTrigger2, GamepadButtonCode::R2),
			(Button::Select, GamepadButtonCode::SHARE_VIEW),
			(Button::Start, GamepadButtonCode::OPTIONS_MENU),
			(Button::LeftThumb, GamepadButtonCode::L3),
			(Button::RightThumb, GamepadButtonCode::R3),
			(Button::DPadUp, GamepadButtonCode::DPAD_UP),
			(Button::DPadDown, GamepadButtonCode::DPAD_DOWN),
			(Button::DPadLeft, GamepadButtonCode::DPAD_LEFT),
			(Button::DPadRight, GamepadButtonCode::DPAD_RIGHT),
			(Button::Mode, GamepadButtonCode::HOME_PS),
		];
		assert_eq!(expected_mapping_list.len(), STANDARD_BUTTON_COUNT);
		for (button, expected_button_index) in expected_mapping_list {
			let gamepad = read_with_pressed_button(button);
			assert_eq!(gamepad.buttons.len(), STANDARD_BUTTON_COUNT);
			for (button_index, gamepad_button) in gamepad.buttons.iter().enumerate() {
				let is_expected_pressed = button_index == expected_button_index;
				assert_eq!(gamepad_button.pressed, is_expected_pressed, "{:?} -> {}", button, button_index);
			}
		}
		assert_eq!(GamepadTriggerCode::L2, GamepadButtonCode::L2);
		assert_eq!(GamepadTriggerCode::R2, GamepadButtonCode::R2);
	}

	//==============================================================================
	/// 축은 W3C 순서이고 Y 는 아래가 양수다.
	//==============================================================================
	#[test]
	fn axis_index_mapping() {
		let element_reader = TestElementReader {
			button_list: Vec::new(),
			axis_list: vec![(Axis::LeftStickX, -0.25), (Axis::LeftStickY, 0.5), (Axis::RightStickX, 0.75), (Axis::RightStickY, -1.0)],
		};
		let gamepad = create_standard_gamepad(3, "test", &element_reader);
		assert_eq!(gamepad.index, 3);
		assert_eq!(gamepad.axes.len(), STANDARD_AXIS_COUNT);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::LEFT_X], -0.25);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::LEFT_Y], -0.5);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::RIGHT_X], 0.75);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::RIGHT_Y], 1.0);

		// 가만히 있는 Y 축은 -0 이 아니라 0.
		let resting_reader = TestElementReader {
			button_list: Vec::new(),
			axis_list: Vec::new(),
		};
		let resting_gamepad = create_standard_gamepad(0, "test", &resting_reader);
		assert!(resting_gamepad.axes[GamepadAnalogStickCode::LEFT_Y].is_sign_positive());
	}

	//==============================================================================
	/// 트리거는 값이 30/255 를 넘으면 눌림이고 값은 그대로 간다.
	//==============================================================================
	#[test]
	fn trigger_pressed_threshold() {
		let element_reader = TestElementReader {
			button_list: vec![
				(Button::LeftTrigger2, GamepadButton { pressed: false, value: 0.1 }),
				(Button::RightTrigger2, GamepadButton { pressed: false, value: 0.2 }),
			],
			axis_list: Vec::new(),
		};
		let gamepad = create_standard_gamepad(0, "test", &element_reader);
		let left_trigger = gamepad.buttons[GamepadButtonCode::L2];
		let right_trigger = gamepad.buttons[GamepadButtonCode::R2];
		assert!(!left_trigger.pressed);
		assert_eq!(left_trigger.value, 0.1);
		assert!(right_trigger.pressed);
		assert_eq!(right_trigger.value, 0.2);
	}

	//==============================================================================
	/// 축(햇 스위치)으로 오는 십자키는 버튼 12~15 가 된다.
	//==============================================================================
	#[test]
	fn directional_pad_from_hat_axis() {
		let element_reader = TestElementReader {
			button_list: Vec::new(),
			axis_list: vec![(Axis::DPadX, -1.0), (Axis::DPadY, 1.0)],
		};
		let gamepad = create_standard_gamepad(0, "test", &element_reader);
		assert!(gamepad.buttons[GamepadButtonCode::DPAD_LEFT].pressed);
		assert_eq!(gamepad.buttons[GamepadButtonCode::DPAD_LEFT].value, 1.0);
		assert!(gamepad.buttons[GamepadButtonCode::DPAD_UP].pressed);
		assert!(!gamepad.buttons[GamepadButtonCode::DPAD_RIGHT].pressed);
		assert!(!gamepad.buttons[GamepadButtonCode::DPAD_DOWN].pressed);
	}

	//==============================================================================
	/// 데스크탑 패드 이름은 크롬 id 꼴이다.
	//==============================================================================
	#[test]
	fn desktop_gamepad_identifier() {
		let play_station_identifier = create_desktop_gamepad_identifier("Wireless Controller", Some(0x054c), Some(0x09cc));
		assert_eq!(play_station_identifier, "Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)");
		let unknown_identifier = create_desktop_gamepad_identifier("Pad", None, None);
		assert_eq!(unknown_identifier, "Pad (STANDARD GAMEPAD)");
	}

	//==============================================================================
	/// 축은 소수점 둘째 자리로 반올림하고 0.01 넘게 바뀔 때만 받는다. (원본 데드존)
	//==============================================================================
	#[test]
	fn axis_dead_zone() {
		let mut gamepad_manager = create_test_manager();
		let mut gamepad = Gamepad::create(0, "test");

		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.004;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_X), 0.0);

		// 반올림하면 0.01, 바뀐 폭이 0.01 을 넘지 않아 그대로.
		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.011;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_X), 0.0);

		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.456;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_X), 0.46);

		// 반올림하면 같은 0.46.
		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.462;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_X), 0.46);

		gamepad.axes[GamepadAnalogStickCode::LEFT_Y] = -1.0;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_Y), -1.0);

		// 없는 패드, 없는 축은 0.
		assert_eq!(gamepad_manager.get_axis_value(5, GamepadAnalogStickCode::LEFT_X), 0.0);
		assert_eq!(gamepad_manager.get_axis_value(0, 9), 0.0);
	}

	//==============================================================================
	/// 방금 눌림 / 방금 뗌은 갱신 하나 동안만 켜진다.
	//==============================================================================
	#[test]
	fn button_edges_across_ticks() {
		let mut gamepad_manager = create_test_manager();
		let mut gamepad = Gamepad::create(0, "test");
		let button_index = GamepadButtonCode::A_CROSS;

		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert!(!gamepad_manager.is_button_pressed(0, button_index));
		assert!(!gamepad_manager.is_button_just_pressed(0, button_index));

		gamepad.buttons[button_index] = GamepadButton { pressed: true, value: 1.0 };
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert!(gamepad_manager.is_button_pressed(0, button_index));
		assert!(gamepad_manager.is_button_just_pressed(0, button_index));
		assert!(!gamepad_manager.is_button_just_released(0, button_index));

		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert!(gamepad_manager.is_button_pressed(0, button_index));
		assert!(!gamepad_manager.is_button_just_pressed(0, button_index));

		gamepad.buttons[button_index] = GamepadButton { pressed: false, value: 0.0 };
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert!(!gamepad_manager.is_button_pressed(0, button_index));
		assert!(gamepad_manager.is_button_just_released(0, button_index));

		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert!(!gamepad_manager.is_button_just_released(0, button_index));

		// 붙는 갱신에 이미 눌려 있으면 그 갱신에 방금 눌림.
		let mut other_gamepad = Gamepad::create(1, "other");
		other_gamepad.buttons[button_index] = GamepadButton { pressed: true, value: 1.0 };
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone()), Some(other_gamepad)]);
		assert!(gamepad_manager.is_button_just_pressed(1, button_index));
	}

	//==============================================================================
	/// 표준 뒤의 단추와 축(터치패드 17, 다섯째 축)도 받고, 붙은 뒤에 늘어나도 상태 칸을 늘려 받는다.
	//==============================================================================
	#[test]
	fn extra_buttons_and_axes_grow_state() {
		let mut gamepad_manager = create_test_manager();
		let mut gamepad = Gamepad::create(0, "test");
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert!(!gamepad_manager.is_button_pressed(0, GamepadButtonCode::TOUCHPAD));

		gamepad.buttons.push(GamepadButton { pressed: true, value: 1.0 });
		gamepad.axes.push(0.5);
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert!(gamepad_manager.is_button_pressed(0, GamepadButtonCode::TOUCHPAD));
		assert!(gamepad_manager.is_button_just_pressed(0, GamepadButtonCode::TOUCHPAD));
		assert_eq!(gamepad_manager.get_axis_value(0, 4), 0.5);
	}

	//==============================================================================
	/// 플레이스테이션 패드의 터치패드 단추 자리. (크롬 MapperDualshock4, MapperDualSense 의 `input.buttons[13]`, 리눅스 고친 드라이버는 MapperPs4Ps5 라 없음)
	//==============================================================================
	#[test]
	fn playstation_touchpad_code() {
		let dual_sense_product_id = Some(0x0ce6);
		let sony_vendor_id = Some(0x054c);
		assert_eq!(find_playstation_touchpad_code("macos", sony_vendor_id, dual_sense_product_id, 0), Some(Some(0x0009_000e)));
		assert_eq!(find_playstation_touchpad_code("windows", sony_vendor_id, Some(0x09cc), 0), Some(Some(13)));
		assert_eq!(find_playstation_touchpad_code("linux", sony_vendor_id, Some(0x05c4), 0x0111), Some(Some(0x0001_013d)));
		assert_eq!(find_playstation_touchpad_code("linux", sony_vendor_id, dual_sense_product_id, 0x8111), Some(None));
		// 듀얼쇼크3, 다른 제조사, 번호를 모르는 패드, 다른 OS 는 그대로 둔다.
		assert_eq!(find_playstation_touchpad_code("macos", sony_vendor_id, Some(0x0268), 0), None);
		assert_eq!(find_playstation_touchpad_code("windows", Some(0x045e), Some(0x0ce6), 0), None);
		assert_eq!(find_playstation_touchpad_code("macos", None, None, 0), None);
		assert_eq!(find_playstation_touchpad_code("freebsd", sony_vendor_id, dual_sense_product_id, 0), None);
		// SDL GUID 꼴 uuid 의 판 번호. (리눅스 PS5 고친 드라이버 030000004c050000e60c000011810000)
		let uuid: [u8; 16] = [0x03, 0x00, 0x00, 0x00, 0x4c, 0x05, 0x00, 0x00, 0xe6, 0x0c, 0x00, 0x00, 0x11, 0x81, 0x00, 0x00];
		assert_eq!(read_uuid_version(&uuid), 0x8111);
	}

	//==============================================================================
	/// 콜백: 눌림 변화는 button, 0 보다 큰 값 변화만 value, 축은 axis.
	//==============================================================================
	#[test]
	fn input_callback() {
		let mut gamepad_manager = create_test_manager();
		let received_event_list: Rc<RefCell<Vec<ReceivedInputEvent>>> = Rc::new(RefCell::new(Vec::new()));
		let received_event_list_for_callback = received_event_list.clone();
		gamepad_manager.set_callback(Some(Box::new(move |gamepad_index, input_type, input_index, value| {
			received_event_list_for_callback.borrow_mut().push((gamepad_index, input_type, input_index, value));
		})));

		let mut gamepad = Gamepad::create(2, "test");
		let mut gamepad_list = vec![None, None, Some(gamepad.clone())];
		gamepad_manager.update_all_gamepads_with_list(gamepad_list.clone());
		assert!(received_event_list.borrow().is_empty());

		gamepad.buttons[GamepadButtonCode::R2] = GamepadButton { pressed: true, value: 0.5 };
		gamepad.axes[GamepadAnalogStickCode::RIGHT_X] = 0.3;
		gamepad_list[2] = Some(gamepad.clone());
		gamepad_manager.update_all_gamepads_with_list(gamepad_list.clone());
		let expected_press_event_list = vec![
			(2, GamepadInputType::Button, GamepadButtonCode::R2, 1.0),
			(2, GamepadInputType::Value, GamepadButtonCode::R2, 0.5),
			(2, GamepadInputType::Axis, GamepadAnalogStickCode::RIGHT_X, 0.3),
		];
		assert_eq!(*received_event_list.borrow(), expected_press_event_list);
		assert_eq!(gamepad_manager.get_button_value(2, GamepadButtonCode::R2), 0.5);
		received_event_list.borrow_mut().clear();

		// 값이 0 으로 내려가면 value 는 알리지 않지만 상태는 0.
		gamepad.buttons[GamepadButtonCode::R2] = GamepadButton { pressed: false, value: 0.0 };
		gamepad_list[2] = Some(gamepad.clone());
		gamepad_manager.update_all_gamepads_with_list(gamepad_list.clone());
		let expected_release_event_list = vec![(2, GamepadInputType::Button, GamepadButtonCode::R2, 0.0)];
		assert_eq!(*received_event_list.borrow(), expected_release_event_list);
		assert_eq!(gamepad_manager.get_button_value(2, GamepadButtonCode::R2), 0.0);
	}

	//==============================================================================
	/// 목록에 나타나면 붙고, 사라지면 놓는다. (원본 updateAllGamepads 의 보완 훑기)
	//==============================================================================
	#[test]
	fn connect_and_disconnect_by_list() {
		let mut gamepad_manager = create_test_manager();
		let gamepad = Gamepad::create(1, "second slot");
		gamepad_manager.update_all_gamepads_with_list(vec![None, Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_connected_gamepad_count(), 1);
		let connected_gamepad = gamepad_manager.get_connected_gamepad(0);
		assert_eq!(connected_gamepad.map(|gamepad| gamepad.index), Some(1));
		assert!(gamepad_manager.get_connected_gamepad(1).is_none());
		let all_connected_gamepads = gamepad_manager.get_all_connected_gamepads();
		assert_eq!(all_connected_gamepads.len(), 1);
		assert_eq!(all_connected_gamepads[0].id, "second slot");

		gamepad_manager.update_all_gamepads_with_list(Vec::new());
		assert_eq!(gamepad_manager.get_connected_gamepad_count(), 0);
		assert!(gamepad_manager.get_connected_gamepad(0).is_none());
		assert!(gamepad_manager.get_all_connected_gamepads().is_empty());
		assert!(!gamepad_manager.is_button_pressed(1, GamepadButtonCode::A_CROSS));
	}

	//==============================================================================
	/// connect 는 같은 패드를 두 번 붙이지 않고, disconnect 는 붙은 패드만 뗀다.
	//==============================================================================
	#[test]
	fn connect_ignores_duplicate() {
		let mut gamepad_manager = create_test_manager();
		let gamepad = Gamepad::create(0, "test");
		gamepad_manager.connect(&gamepad);
		gamepad_manager.connect(&gamepad);
		assert_eq!(gamepad_manager.get_connected_gamepad_count(), 1);
		let other_gamepad = Gamepad::create(4, "other");
		gamepad_manager.disconnect(&other_gamepad);
		assert_eq!(gamepad_manager.get_connected_gamepad_count(), 1);
		gamepad_manager.disconnect(&gamepad);
		assert_eq!(gamepad_manager.get_connected_gamepad_count(), 0);
	}

	//==============================================================================
	/// 패드 이름으로 종류를 가른다.
	//==============================================================================
	#[test]
	fn gamepad_kind_from_identifier() {
		assert_eq!(read_gamepad_kind("DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)"), GamepadKind::PlayStation);
		assert_eq!(read_gamepad_kind("Xbox 360 Controller (XInput STANDARD GAMEPAD)"), GamepadKind::Xbox);
		assert_eq!(read_gamepad_kind("045e-02fd-Xbox Wireless Controller"), GamepadKind::Xbox);
		assert_eq!(read_gamepad_kind("Pro Controller (STANDARD GAMEPAD Vendor: 057e Product: 2009)"), GamepadKind::Nintendo);
		assert_eq!(read_gamepad_kind("Steam Virtual Gamepad"), GamepadKind::Other);
	}

	//==============================================================================
	/// 패드 목록 읽기: 범위 밖은 장치를 열지 않고 None, 목록을 넣은 관리자는 넣은 목록.
	//==============================================================================
	#[test]
	fn connected_gamepad_reads_given_list_without_device() {
		let mut gamepad_manager = GamepadManager::new();
		gamepad_manager.set_log_enabled(false);
		// 연결 목록 밖의 순번은 장치를 열지 않고 None. (원본 getConnectedGamepad 의 범위 검사)
		assert!(gamepad_manager.get_connected_gamepad(0).is_none());
		assert!(!gamepad_manager.is_gamepad_reader_creation_tried);
		// 목록을 넣은 관리자는 그 자리에서 읽어도 넣은 목록을 돌려준다.
		let mut gamepad = Gamepad::create(0, "Pad");
		gamepad.axes = vec![0.25, 0.0, 0.0, 0.0];
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		gamepad.axes[0] = 0.5;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad)]);
		assert_eq!(gamepad_manager.get_connected_gamepad(0).map(|gamepad| gamepad.axes[0]), Some(0.5));
		assert_eq!(gamepad_manager.get_all_connected_gamepads().len(), 1);
		assert!(!gamepad_manager.is_gamepad_reader_creation_tried);
	}

	//==============================================================================
	/// 단추를 누르거나 축을 0.5 넘게 기울인 갱신에만 그 패드가 활동한 패드다.
	//==============================================================================
	#[test]
	fn active_gamepad_identifier() {
		let mut gamepad_manager = create_test_manager();
		let mut gamepad = Gamepad::create(0, "Wireless Controller 054c");
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_active_gamepad_id(), None);
		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.3;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_active_gamepad_id(), None);
		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.7;
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_active_gamepad_id(), Some("Wireless Controller 054c"));
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_active_gamepad_id(), None);
		gamepad.buttons[GamepadButtonCode::A_CROSS] = GamepadButton { pressed: true, value: 1.0 };
		gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
		assert_eq!(gamepad_manager.get_active_gamepad_id(), Some("Wireless Controller 054c"));
		assert!(!gamepad_manager.is_input_log_enabled());
	}

	//==============================================================================
	/// 반올림은 원본 toFixed(2) 와 같은 값을 낸다.
	//==============================================================================
	#[test]
	fn round_to_hundredths_matches_to_fixed() {
		assert_eq!(round_to_hundredths(0.123), 0.12);
		assert_eq!(round_to_hundredths(0.125), 0.13);
		assert_eq!(round_to_hundredths(-0.456), -0.46);
		assert_eq!(round_to_hundredths(1.0), 1.0);
		assert_eq!(round_to_hundredths(0.29f32 as f64), 0.29);
	}
}
