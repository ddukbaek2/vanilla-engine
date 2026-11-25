//==============================================================================
//! 입력 관리자.
//! - 키 이름은 원본과 같이 KeyboardEvent.code 꼴이다. ("KeyA", "ArrowLeft", "Space" ...)
//!   winit 의 KeyCode 이름이 같은 W3C 규약을 따르므로 그대로 문자열로 바꿔 쓴다. 윈도우 키(커맨드 키)만 winit 이 SuperLeft, SuperRight 라 부르고
//!   W3C 의 옛 Super 키를 Meta 라 불러서, 데스크탑 플랫폼이 MetaLeft, MetaRight, Super 로 바꿔 넣는다.
//! - KeyCode 의 상수 이름은 원본 KeyCode 의 키 이름을 러스트 상수 꼴로 바꾼 것이다. ("0" 처럼 이름이 될 수 없는 것은 DIGIT0)
//!   원본에 없던 LEFT, UP, RIGHT, DOWN 은 ARROWLEFT 들과 같은 값으로 남긴다.
//! - 마우스 단추(또는 첫 손가락)를 터치로 다룬다. (원본은 마우스와 터치를 하나로 모은다) 원본처럼 처음에는 어느 단추든 터치이고,
//!   set_touch_from_left_button_only(true) 면 왼쪽 단추만 터치다. 캔버스의 오른쪽 클릭 메뉴를 막을지(set_context_menu_prevented, 처음은 참)는 웹에서 브라우저 문맥 메뉴를 막는다.
//!   데스크탑 창에는 브라우저 문맥 메뉴가 없어 크롬에서 막은 것과 같은 결과다.
//! - 그 밖에 마우스 단추 셋(왼쪽, 가운데, 오른쪽)의 눌림, 닿아 있는 손가락 목록, 마지막 입력이 손가락인지, 휠을 Ctrl 과 함께 굴렸는지를 둔다.
//!   카메라 손짓(CameraGestures)처럼 단추 하나 터치 밖의 입력을 쓰는 곳이 읽는다. 플랫폼이 채운다. (vanilla.js 는 캔버스 DOM 사건을 직접 읽던 것)
//! - 게임패드는 원본과 같이 InputManager 가 GamepadManager 를 들고 tick 에서 폴링한다. (core/gamepadmanager.rs)
//! - 글 입력(확정 글, IME 조합, 편집 키)은 TextInput 줄에 모은다. 입력 필드가 꺼내 쓴다. (core/textinput.rs, 원본에 없는 층)
//! - 키와 마우스 단추의 이번 프레임 누름과 뗌은 원본처럼 지난 프레임 끝 뒤로 온 사건을 쌓아 본다. 프레임 사이에 눌렀다 뗀 것도 잡힌다.
//! - 손가락은 원본처럼 닿음(begin_touch_point), 움직임, 뗌을 받아 단계(TouchPhase)를 매기고, 떨어진 손가락은 한 프레임 get_ended_touch_points 에 남는다.
//! - 마지막으로 쓴 장치(InputDevice)와 패드 종류, 막을 키(set_prevented_keys), 글자 입력칸에 쓰는 중인지(is_typing_in_text_field), 입력 동작(InputActions)을 둔다.
//!   막을 키는 웹에서 브라우저 기본 동작(Tab 의 포커스 옮기기, Space 와 화살표의 스크롤, F5 새로 고침 같은 것)을 막는다. 데스크탑 창에는 그런 기본 동작이 없어 막은 것과 같은 결과다.
//!   글자 입력칸은 엔진 입력 필드(TextInput 의 활성 필드)와, 웹에서는 문서의 초점(글자 input, textarea, contenteditable, select)을 본다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::gamepadmanager::{read_gamepad_kind, Gamepad, GamepadKind, GamepadManager};
use crate::core::inputactions::InputActions;
use crate::core::textinput::TextInput;
use std::collections::HashSet;

//==============================================================================
/// 키 이름. (원본 KeyCode)
//==============================================================================
pub struct KeyCode;

impl KeyCode {
	pub const A: &'static str = "KeyA";
	pub const B: &'static str = "KeyB";
	pub const C: &'static str = "KeyC";
	pub const D: &'static str = "KeyD";
	pub const E: &'static str = "KeyE";
	pub const F: &'static str = "KeyF";
	pub const G: &'static str = "KeyG";
	pub const H: &'static str = "KeyH";
	pub const I: &'static str = "KeyI";
	pub const J: &'static str = "KeyJ";
	pub const K: &'static str = "KeyK";
	pub const L: &'static str = "KeyL";
	pub const M: &'static str = "KeyM";
	pub const N: &'static str = "KeyN";
	pub const O: &'static str = "KeyO";
	pub const P: &'static str = "KeyP";
	pub const Q: &'static str = "KeyQ";
	pub const R: &'static str = "KeyR";
	pub const S: &'static str = "KeyS";
	pub const T: &'static str = "KeyT";
	pub const U: &'static str = "KeyU";
	pub const V: &'static str = "KeyV";
	pub const W: &'static str = "KeyW";
	pub const X: &'static str = "KeyX";
	pub const Y: &'static str = "KeyY";
	pub const Z: &'static str = "KeyZ";
	pub const DIGIT0: &'static str = "Digit0";
	pub const DIGIT1: &'static str = "Digit1";
	pub const DIGIT2: &'static str = "Digit2";
	pub const DIGIT3: &'static str = "Digit3";
	pub const DIGIT4: &'static str = "Digit4";
	pub const DIGIT5: &'static str = "Digit5";
	pub const DIGIT6: &'static str = "Digit6";
	pub const DIGIT7: &'static str = "Digit7";
	pub const DIGIT8: &'static str = "Digit8";
	pub const DIGIT9: &'static str = "Digit9";
	pub const BACKSPACE: &'static str = "Backspace";
	pub const TAB: &'static str = "Tab";
	pub const ENTER: &'static str = "Enter";
	pub const SHIFT: &'static str = "ShiftLeft";
	pub const CTRL: &'static str = "ControlLeft";
	pub const ALT: &'static str = "AltLeft";
	pub const CAPSLOCK: &'static str = "CapsLock";
	pub const ESCAPE: &'static str = "Escape";
	pub const SPACE: &'static str = "Space";
	pub const PAGEUP: &'static str = "PageUp";
	pub const PAGEDOWN: &'static str = "PageDown";
	pub const END: &'static str = "End";
	pub const HOME: &'static str = "Home";
	pub const LEFT: &'static str = "ArrowLeft";
	pub const UP: &'static str = "ArrowUp";
	pub const RIGHT: &'static str = "ArrowRight";
	pub const DOWN: &'static str = "ArrowDown";
	pub const INSERT: &'static str = "Insert";
	pub const DELETE: &'static str = "Delete";
	pub const F1: &'static str = "F1";
	pub const F2: &'static str = "F2";
	pub const F3: &'static str = "F3";
	pub const F4: &'static str = "F4";
	pub const F5: &'static str = "F5";
	pub const F6: &'static str = "F6";
	pub const F7: &'static str = "F7";
	pub const F8: &'static str = "F8";
	pub const F9: &'static str = "F9";
	pub const F10: &'static str = "F10";
	pub const F11: &'static str = "F11";
	pub const F12: &'static str = "F12";
	pub const ARROWLEFT: &'static str = "ArrowLeft";
	pub const ARROWUP: &'static str = "ArrowUp";
	pub const ARROWRIGHT: &'static str = "ArrowRight";
	pub const ARROWDOWN: &'static str = "ArrowDown";
	pub const SCROLLLOCK: &'static str = "ScrollLock";
	pub const SEMICOLON: &'static str = "Semicolon";
	pub const EQUAL: &'static str = "Equal";
	pub const COMMA: &'static str = "Comma";
	pub const MINUS: &'static str = "Minus";
	pub const PERIOD: &'static str = "Period";
	pub const SLASH: &'static str = "Slash";
	pub const BACKQUOTE: &'static str = "Backquote";
	pub const BRACKETLEFT: &'static str = "BracketLeft";
	pub const BACKSLASH: &'static str = "Backslash";
	pub const BRACKETRIGHT: &'static str = "BracketRight";
	pub const QUOTE: &'static str = "Quote";
	pub const META: &'static str = "MetaLeft";
	pub const COMMAND: &'static str = "MetaLeft";
	pub const PAUSE: &'static str = "Pause";
	pub const AUDIOVOLUMEUP: &'static str = "AudioVolumeUp";
	pub const AUDIOVOLUMEDOWN: &'static str = "AudioVolumeDown";
	/// 음소거. (W3C code 표의 이름 AudioVolumeMute)
	pub const AUDIOMUTE: &'static str = "AudioVolumeMute";
	/// 재생/멈춤. (W3C code 표의 이름 MediaPlayPause, 표에 MediaPlay 는 없다)
	pub const AUDIOPLAY: &'static str = "MediaPlayPause";
	pub const AUDIOSTOP: &'static str = "MediaStop";
	pub const AUDIOPREV: &'static str = "MediaTrackPrevious";
	pub const AUDIONEXT: &'static str = "MediaTrackNext";
	pub const NUM_LOCK: &'static str = "NumLock";
	/// 텐키 나누기.
	pub const NUMPAD_DIVIDE: &'static str = "NumpadDivide";
	/// 텐키 곱하기.
	pub const NUMPAD_MULTIPLY: &'static str = "NumpadMultiply";
	/// 텐키 빼기.
	pub const NUMPAD_SUBTRACT: &'static str = "NumpadSubtract";
	/// 텐키 점.
	pub const NUMPAD_DECIMAL: &'static str = "NumpadDecimal";
	/// 텐키 더하기.
	pub const NUMPAD_ADD: &'static str = "NumpadAdd";
	/// 텐키 엔터.
	pub const NUMPAD_ENTER: &'static str = "NumpadEnter";
	pub const NUMPAD0: &'static str = "Numpad0";
	pub const NUMPAD1: &'static str = "Numpad1";
	pub const NUMPAD2: &'static str = "Numpad2";
	pub const NUMPAD3: &'static str = "Numpad3";
	pub const NUMPAD4: &'static str = "Numpad4";
	pub const NUMPAD5: &'static str = "Numpad5";
	pub const NUMPAD6: &'static str = "Numpad6";
	pub const NUMPAD7: &'static str = "Numpad7";
	pub const NUMPAD8: &'static str = "Numpad8";
	pub const NUMPAD9: &'static str = "Numpad9";
}

//==============================================================================
/// 마우스 단추.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseButton {
	Left,
	Middle,
	Right,
}

impl MouseButton {
	//==============================================================================
	/// 단추 칸 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		match self {
			MouseButton::Left => {
				0
			}
			MouseButton::Middle => {
				1
			}
			MouseButton::Right => {
				2
			}
		}
	}
}

//==============================================================================
/// 손가락의 단계. (Began 이번 프레임에 닿음, Moved 움직임, Stationary 가만히, Ended 뗌, Cancelled 취소)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TouchPhase {
	Began,
	Moved,
	Stationary,
	Ended,
	Cancelled,
}

//==============================================================================
/// 마지막으로 쓴 입력 장치.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum InputDevice {
	Keyboard,
	Mouse,
	Touch,
	Gamepad,
}

//==============================================================================
/// 닿아 있는 손가락 하나. (플랫폼의 손가락 번호, 단계, 캔버스 논리 좌표와 뷰 좌표)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TouchPoint {
	pub identifier: i64,
	pub phase: TouchPhase,
	pub canvas_native_position: Vector2,
	pub view_position: Vector2,
}

pub struct InputManager {
	pressed_keys: HashSet<String>,
	just_pressed_keys: HashSet<String>,
	just_released_keys: HashSet<String>,
	is_touch_pressed: bool,
	is_touch_released: bool,
	is_touch_cancelled: bool,
	is_touch_moved: bool,
	is_touch_down: bool,
	canvas_native_input_position: Vector2,
	view_input_position: Vector2,
	wheel_delta_x: f32,
	wheel_delta_y: f32,
	gamepad_manager: GamepadManager,
	text_input: TextInput,
	mouse_buttons_down: [bool; 3],
	mouse_buttons_just_pressed: [bool; 3],
	mouse_buttons_just_released: [bool; 3],
	touch_points: Vec<TouchPoint>,
	ended_touch_points: Vec<TouchPoint>,
	is_touch_input: bool,
	is_wheel_with_ctrl: bool,
	last_input_device: InputDevice,
	last_gamepad_kind: GamepadKind,
	prevented_keys: HashSet<String>,
	input_actions: InputActions,
	/// 캔버스의 오른쪽 클릭 메뉴를 막는지.
	is_context_menu_prevented: bool,
	/// 마우스 왼쪽 단추만 터치로 넘기는지.
	is_touch_from_left_button_only: bool,
}

impl InputManager {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> InputManager {
		InputManager {
			pressed_keys: HashSet::new(),
			just_pressed_keys: HashSet::new(),
			just_released_keys: HashSet::new(),
			is_touch_pressed: false,
			is_touch_released: false,
			is_touch_cancelled: false,
			is_touch_moved: false,
			is_touch_down: false,
			canvas_native_input_position: Vector2::zero(),
			view_input_position: Vector2::zero(),
			wheel_delta_x: 0.0,
			wheel_delta_y: 0.0,
			gamepad_manager: GamepadManager::new(),
			text_input: TextInput::new(),
			mouse_buttons_down: [false; 3],
			mouse_buttons_just_pressed: [false; 3],
			mouse_buttons_just_released: [false; 3],
			touch_points: Vec::new(),
			ended_touch_points: Vec::new(),
			is_touch_input: crate::platform::is_coarse_pointer(),
			is_wheel_with_ctrl: false,
			last_input_device: if crate::platform::is_coarse_pointer() {
				InputDevice::Touch
			}
			else {
				InputDevice::Keyboard
			},
			last_gamepad_kind: GamepadKind::Other,
			prevented_keys: HashSet::new(),
			input_actions: InputActions::new(),
			is_context_menu_prevented: true,
			is_touch_from_left_button_only: false,
		}
	}

	//==============================================================================
	/// 주기적 갱신. (게임패드 폴링, 패드를 썼으면 마지막 장치를 게임패드로, 입력 동작 갱신)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.tick(time_delta);
		let active_gamepad_kind = gamepad_manager.get_active_gamepad_id().map(read_gamepad_kind);
		if let Some(gamepad_kind) = active_gamepad_kind {
			self.set_last_input_device(InputDevice::Gamepad);
			self.set_last_gamepad_kind(gamepad_kind);
		}
		self.update_input_actions(time_delta);
	}

	//==============================================================================
	/// 입력 동작 갱신. (동작이 입력 매니저를 읽고 마지막 장치를 적을 수 있게 잠깐 꺼낸다)
	//==============================================================================
	pub(crate) fn update_input_actions(&mut self, time_delta: f32) {
		let mut input_actions = std::mem::take(&mut self.input_actions);
		input_actions.update(self, time_delta);
		self.input_actions = input_actions;
	}

	//==============================================================================
	/// 모든 입력 상태 비우기. (창이 포커스를 잃었을 때)
	//==============================================================================
	pub fn clear(&mut self) {
		self.pressed_keys.clear();
		self.just_pressed_keys.clear();
		self.just_released_keys.clear();
		self.mouse_buttons_just_pressed = [false; 3];
		self.mouse_buttons_just_released = [false; 3];
		self.ended_touch_points.clear();
		self.input_actions.clear();
		self.is_touch_down = false;
		self.is_touch_pressed = false;
		self.is_touch_released = false;
		self.is_touch_cancelled = false;
		self.is_touch_moved = false;
		self.clear_wheel_delta();
		self.mouse_buttons_down = [false; 3];
		self.touch_points.clear();
	}

	//==============================================================================
	/// 휠 이동량 누적.
	//==============================================================================
	pub fn add_wheel_delta(&mut self, delta_x: f32, delta_y: f32) {
		self.wheel_delta_x += delta_x;
		self.wheel_delta_y += delta_y;
	}

	pub fn get_wheel_delta_x(&self) -> f32 {
		self.wheel_delta_x
	}

	pub fn get_wheel_delta_y(&self) -> f32 {
		self.wheel_delta_y
	}

	pub fn has_wheel_delta(&self) -> bool {
		self.wheel_delta_x != 0.0 || self.wheel_delta_y != 0.0
	}

	pub fn clear_wheel_delta(&mut self) {
		self.wheel_delta_x = 0.0;
		self.wheel_delta_y = 0.0;
	}

	//==============================================================================
	/// 키 눌림 기록.
	//==============================================================================
	pub fn push_key(&mut self, key: &str) {
		if self.pressed_keys.insert(key.to_string()) {
			self.just_pressed_keys.insert(key.to_string());
		}
	}

	//==============================================================================
	/// 키 뗌 기록.
	//==============================================================================
	pub fn pop_key(&mut self, key: &str) {
		if self.pressed_keys.remove(key) {
			self.just_released_keys.insert(key.to_string());
		}
	}

	//==============================================================================
	/// 키가 눌려 있는지.
	//==============================================================================
	pub fn is_key_pressed(&self, key: &str) -> bool {
		self.pressed_keys.contains(key)
	}

	//==============================================================================
	/// 이번 프레임에 눌렸는지. (지난 프레임 끝 뒤로 온 사건, 프레임 사이에 눌렀다 뗀 키도 참이고 그때는 is_key_pressed 가 거짓)
	//==============================================================================
	pub fn is_key_just_pressed(&self, key: &str) -> bool {
		self.just_pressed_keys.contains(key)
	}

	//==============================================================================
	/// 이번 프레임에 떼졌는지.
	//==============================================================================
	pub fn is_key_just_released(&self, key: &str) -> bool {
		self.just_released_keys.contains(key)
	}

	//==============================================================================
	/// 프레임 끝 스냅샷. (쌓인 키와 마우스 단추의 누름과 뗌을 비우고, 떨어진 손가락을 지우고 남은 손가락을 가만히로)
	//==============================================================================
	pub fn update_frame_snapshot(&mut self) {
		self.just_pressed_keys.clear();
		self.just_released_keys.clear();
		self.mouse_buttons_just_pressed = [false; 3];
		self.mouse_buttons_just_released = [false; 3];
		self.ended_touch_points.clear();
		for touch_point in self.touch_points.iter_mut() {
			touch_point.phase = TouchPhase::Stationary;
		}
	}

	//==============================================================================
	/// 마우스 단추 눌림 설정. (플랫폼이 부른다)
	//==============================================================================
	pub fn set_mouse_button_down(&mut self, mouse_button: MouseButton, is_down: bool) {
		let button_index = mouse_button.get_index();
		let was_down = self.mouse_buttons_down[button_index];
		if is_down && !was_down {
			self.mouse_buttons_just_pressed[button_index] = true;
		}
		if !is_down && was_down {
			self.mouse_buttons_just_released[button_index] = true;
		}
		self.mouse_buttons_down[button_index] = is_down;
	}

	//==============================================================================
	/// 마우스 단추가 눌려 있는지.
	//==============================================================================
	pub fn is_mouse_button_down(&self, mouse_button: MouseButton) -> bool {
		self.mouse_buttons_down[mouse_button.get_index()]
	}

	//==============================================================================
	/// 이번 프레임에 마우스 단추를 눌렀는지.
	//==============================================================================
	pub fn is_mouse_button_just_pressed(&self, mouse_button: MouseButton) -> bool {
		self.mouse_buttons_just_pressed[mouse_button.get_index()]
	}

	//==============================================================================
	/// 이번 프레임에 마우스 단추를 뗐는지.
	//==============================================================================
	pub fn is_mouse_button_just_released(&self, mouse_button: MouseButton) -> bool {
		self.mouse_buttons_just_released[mouse_button.get_index()]
	}

	//==============================================================================
	/// 마우스 왼쪽 단추만 터치로 넘길지 설정. (끄면(기본) 어느 단추든 터치다)
	/// - 오른쪽이나 가운데 끌기를 카메라에 쓰는 게임이 켜면 그 단추로 UI 가 눌리지 않는다.
	//==============================================================================
	pub fn set_touch_from_left_button_only(&mut self, is_left_button_only: bool) {
		self.is_touch_from_left_button_only = is_left_button_only;
	}

	//==============================================================================
	/// 마우스 왼쪽 단추만 터치로 넘기는지 반환.
	//==============================================================================
	pub fn is_touch_from_left_button_only(&self) -> bool {
		self.is_touch_from_left_button_only
	}

	//==============================================================================
	/// 그 마우스 단추를 터치로 넘기는지. (플랫폼이 마우스 누름과 뗌에서 부른다)
	//==============================================================================
	pub fn is_touch_button(&self, mouse_button: MouseButton) -> bool {
		!self.is_touch_from_left_button_only || mouse_button == MouseButton::Left
	}

	//==============================================================================
	/// 캔버스의 오른쪽 클릭 메뉴를 막을지 설정. (처음은 참, 데스크탑 창에는 막을 메뉴가 없다)
	//==============================================================================
	pub fn set_context_menu_prevented(&mut self, is_prevented: bool) {
		self.is_context_menu_prevented = is_prevented;
	}

	//==============================================================================
	/// 캔버스의 오른쪽 클릭 메뉴를 막는지 반환.
	//==============================================================================
	pub fn is_context_menu_prevented(&self) -> bool {
		self.is_context_menu_prevented
	}

	//==============================================================================
	/// 닿아 있는 손가락 찾기. (없으면 None)
	//==============================================================================
	pub fn find_touch_point(&self, identifier: i64) -> Option<&TouchPoint> {
		self.touch_points.iter().find(|touch_point| touch_point.identifier == identifier)
	}

	//==============================================================================
	/// 손가락이 닿음. (플랫폼이 부른다, 이미 있는 번호면 움직임으로 본다)
	//==============================================================================
	pub fn begin_touch_point(&mut self, identifier: i64, canvas_native_position: Vector2, view_position: Vector2) {
		if self.find_touch_point(identifier).is_some() {
			self.move_touch_point(identifier, canvas_native_position, view_position);
			return;
		}
		self.touch_points.push(TouchPoint {
			identifier,
			phase: TouchPhase::Began,
			canvas_native_position,
			view_position,
		});
	}

	//==============================================================================
	/// 손가락이 움직임. (이번 프레임에 닿은 손가락은 Began 그대로)
	//==============================================================================
	pub fn move_touch_point(&mut self, identifier: i64, canvas_native_position: Vector2, view_position: Vector2) {
		if let Some(touch_point) = self.touch_points.iter_mut().find(|touch_point| touch_point.identifier == identifier) {
			touch_point.canvas_native_position = canvas_native_position;
			touch_point.view_position = view_position;
			if touch_point.phase != TouchPhase::Began {
				touch_point.phase = TouchPhase::Moved;
			}
		}
	}

	//==============================================================================
	/// 손가락이 떨어짐. (닿은 목록에서 빼서 이번 프레임의 떨어진 목록에 둔다)
	//==============================================================================
	pub fn end_touch_point(&mut self, identifier: i64, canvas_native_position: Vector2, view_position: Vector2, is_cancelled: bool) {
		let touch_index = match self.touch_points.iter().position(|touch_point| touch_point.identifier == identifier) {
			Some(touch_index) => {
				touch_index
			}
			None => {
				return;
			}
		};
		let mut touch_point = self.touch_points.remove(touch_index);
		touch_point.canvas_native_position = canvas_native_position;
		touch_point.view_position = view_position;
		touch_point.phase = if is_cancelled {
			TouchPhase::Cancelled
		}
		else {
			TouchPhase::Ended
		};
		self.ended_touch_points.push(touch_point);
	}

	//==============================================================================
	/// 이번 프레임에 떨어진 손가락 목록. (단계는 Ended, Cancelled, 자리는 뗀 자리)
	//==============================================================================
	pub fn get_ended_touch_points(&self) -> &[TouchPoint] {
		&self.ended_touch_points
	}

	//==============================================================================
	/// 닿아 있는 손가락 목록 설정. (통째로 바꾼다, 단계는 넘긴 그대로)
	//==============================================================================
	pub fn set_touch_points(&mut self, touch_points: Vec<TouchPoint>) {
		self.touch_points = touch_points;
	}

	//==============================================================================
	/// 닿아 있는 손가락 목록. (닿은 차례, 단계는 Began, Moved, Stationary)
	//==============================================================================
	pub fn get_touch_points(&self) -> &[TouchPoint] {
		&self.touch_points
	}

	//==============================================================================
	/// 마지막 입력이 손가락인지 설정. (플랫폼이 부른다)
	//==============================================================================
	pub fn set_touch_input(&mut self, is_touch_input: bool) {
		self.is_touch_input = is_touch_input;
	}

	//==============================================================================
	/// 마지막 입력이 손가락인지. (마우스면 false, 처음에는 기기가 손가락이 주된 입력인지로. 원본 CameraGestures.isTouchInput 과 같다)
	//==============================================================================
	pub fn is_touch_input(&self) -> bool {
		self.is_touch_input
	}

	//==============================================================================
	/// 마지막 휠을 Ctrl 과 함께 굴렸는지 설정. (노트북 터치패드의 오므리기도 Ctrl 휠로 온다)
	//==============================================================================
	pub fn set_wheel_with_ctrl(&mut self, is_wheel_with_ctrl: bool) {
		self.is_wheel_with_ctrl = is_wheel_with_ctrl;
	}

	//==============================================================================
	/// 마지막 휠을 Ctrl 과 함께 굴렸는지.
	//==============================================================================
	pub fn is_wheel_with_ctrl(&self) -> bool {
		self.is_wheel_with_ctrl
	}

	//==============================================================================
	/// 터치 상태. (Pressed / Released / Cancelled / Moved 는 그 프레임에만 켜진다)
	//==============================================================================
	pub fn set_touch_pressed(&mut self, value: bool) {
		self.is_touch_pressed = value;
	}

	pub fn set_touch_released(&mut self, value: bool) {
		self.is_touch_released = value;
	}

	pub fn set_touch_cancelled(&mut self, value: bool) {
		self.is_touch_cancelled = value;
	}

	//==============================================================================
	/// 누르고 있는지. (원본처럼 누를 때 켜고 뗄 때 끈다, 프레임마다 지우지 않는다, 원본 주석 "입력시 뗄 때까지 계속 눌림")
	//==============================================================================
	pub fn set_touch_moved(&mut self, value: bool) {
		self.is_touch_moved = value;
	}

	pub fn is_touch_pressed(&self) -> bool {
		self.is_touch_pressed
	}

	pub fn is_touch_released(&self) -> bool {
		self.is_touch_released
	}

	pub fn is_touch_cancelled(&self) -> bool {
		self.is_touch_cancelled
	}

	pub fn is_touch_moved(&self) -> bool {
		self.is_touch_moved
	}

	//==============================================================================
	/// 터치가 눌린 채인지. (러스트 판에서 더한 것. 눌린 동안 계속 켜져 있다)
	//==============================================================================
	pub fn set_touch_down(&mut self, value: bool) {
		self.is_touch_down = value;
	}

	pub fn is_touch_down(&self) -> bool {
		self.is_touch_down
	}

	//==============================================================================
	/// 입력 위치. (창 안쪽 논리 좌표 / 뷰 좌표)
	//==============================================================================
	pub fn set_canvas_native_input_position(&mut self, position: Vector2) {
		self.canvas_native_input_position = position;
	}

	pub fn get_canvas_native_input_position(&self) -> Vector2 {
		self.canvas_native_input_position
	}

	pub fn set_view_input_position(&mut self, position: Vector2) {
		self.view_input_position = position;
	}

	pub fn get_view_input_position(&self) -> Vector2 {
		self.view_input_position
	}

	//==============================================================================
	/// 모든 게임패드 갱신.
	//==============================================================================
	pub fn update_all_gamepads(&mut self) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.update_all_gamepads();
	}

	//==============================================================================
	/// 게임패드 연결.
	//==============================================================================
	pub fn connect_gamepad(&mut self, gamepad: &Gamepad) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.connect(gamepad);
	}

	//==============================================================================
	/// 게임패드 연결 해제.
	//==============================================================================
	pub fn disconnect_gamepad(&mut self, gamepad: &Gamepad) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.disconnect(gamepad);
	}

	//==============================================================================
	/// 게임패드 매니저 반환.
	//==============================================================================
	pub fn get_gamepad_manager(&self) -> &GamepadManager {
		&self.gamepad_manager
	}

	//==============================================================================
	/// 게임패드 매니저 반환. (바꿀 때: set_callback, connect 들)
	//==============================================================================
	pub fn get_gamepad_manager_mut(&mut self) -> &mut GamepadManager {
		&mut self.gamepad_manager
	}

	//==============================================================================
	/// 글 입력 줄 반환. (입력 필드가 꺼내 쓰고 상태를 올린다. &self 로 된다)
	//==============================================================================
	pub fn get_text_input(&self) -> &TextInput {
		&self.text_input
	}

	//==============================================================================
	/// 글 입력 줄 반환. (플랫폼이 이벤트를 넣을 때)
	//==============================================================================
	pub fn get_text_input_mut(&mut self) -> &mut TextInput {
		&mut self.text_input
	}

	//==============================================================================
	/// 마지막으로 쓴 입력 장치 설정. (플랫폼이 키, 마우스, 손가락 입력에서 부른다, 게임이 바깥 입력원을 쓸 때도)
	//==============================================================================
	pub fn set_last_input_device(&mut self, input_device: InputDevice) {
		self.last_input_device = input_device;
	}

	//==============================================================================
	/// 마지막으로 쓴 입력 장치. (처음은 손가락 기기면 Touch, 아니면 Keyboard)
	//==============================================================================
	pub fn get_last_input_device(&self) -> InputDevice {
		self.last_input_device
	}

	//==============================================================================
	/// 마지막으로 쓴 게임패드의 종류 설정.
	//==============================================================================
	pub fn set_last_gamepad_kind(&mut self, gamepad_kind: GamepadKind) {
		self.last_gamepad_kind = gamepad_kind;
	}

	//==============================================================================
	/// 마지막으로 쓴 게임패드의 종류. (단추를 누르거나 스틱을 크게 기울인 패드의 이름으로 가른다, 처음은 Other)
	//==============================================================================
	pub fn get_last_gamepad_kind(&self) -> GamepadKind {
		self.last_gamepad_kind
	}

	//==============================================================================
	/// 브라우저 기본 동작을 막을 키 설정. (KeyboardEvent.code 목록, 웹에서만 뜻이 있다, 글자 입력칸에 쓰는 중이면 막지 않는다)
	//==============================================================================
	pub fn set_prevented_keys(&mut self, keys: &[&str]) {
		self.prevented_keys = keys.iter().map(|key| key.to_string()).collect();
	}

	//==============================================================================
	/// 브라우저 기본 동작을 막을 키 목록.
	//==============================================================================
	pub fn get_prevented_keys(&self) -> Vec<String> {
		self.prevented_keys.iter().cloned().collect()
	}

	//==============================================================================
	/// 그 키의 기본 동작을 막는지.
	//==============================================================================
	pub fn is_key_prevented(&self, key: &str) -> bool {
		self.prevented_keys.contains(key)
	}

	//==============================================================================
	/// 글자 입력칸에 쓰는 중인지. (엔진 입력 필드의 포커스, 웹은 문서의 초점이 글자 input, textarea, contenteditable, select 에 있음)
	//==============================================================================
	pub fn is_typing_in_text_field(&self) -> bool {
		self.text_input.get_active_field_state().is_some() || crate::platform::is_text_field_focused()
	}

	//==============================================================================
	/// 입력 동작. (동작 이름에 키, 패드 단추, 축을 묶어 읽는다, tick 마다 갱신된다)
	//==============================================================================
	pub fn get_input_actions(&self) -> &InputActions {
		&self.input_actions
	}

	//==============================================================================
	/// 입력 동작. (묶기, 반복, 바깥 입력원)
	//==============================================================================
	pub fn get_input_actions_mut(&mut self) -> &mut InputActions {
		&mut self.input_actions
	}
}

impl Default for InputManager {
	fn default() -> InputManager {
		InputManager::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 마우스 단추: 프레임 사이에 눌렀다 뗀 것도 그 프레임의 누름과 뗌이다.
	//==============================================================================
	#[test]
	fn mouse_button_edges_within_frame() {
		let mut input_manager = InputManager::new();
		input_manager.set_mouse_button_down(MouseButton::Right, true);
		input_manager.set_mouse_button_down(MouseButton::Right, false);
		assert!(!input_manager.is_mouse_button_down(MouseButton::Right));
		assert!(input_manager.is_mouse_button_just_pressed(MouseButton::Right));
		assert!(input_manager.is_mouse_button_just_released(MouseButton::Right));
		assert!(!input_manager.is_mouse_button_just_pressed(MouseButton::Left));
		input_manager.update_frame_snapshot();
		assert!(!input_manager.is_mouse_button_just_pressed(MouseButton::Right));
		input_manager.set_mouse_button_down(MouseButton::Left, true);
		input_manager.set_mouse_button_down(MouseButton::Left, true);
		assert!(input_manager.is_mouse_button_just_pressed(MouseButton::Left));
		input_manager.update_frame_snapshot();
		assert!(input_manager.is_mouse_button_down(MouseButton::Left));
		assert!(!input_manager.is_mouse_button_just_pressed(MouseButton::Left));
	}

	//==============================================================================
	/// 손가락 단계: 닿음 Began → 프레임 끝 Stationary → 움직임 Moved, 뗀 손가락은 한 프레임 떨어진 목록에.
	//==============================================================================
	#[test]
	fn touch_point_phases() {
		let mut input_manager = InputManager::new();
		input_manager.begin_touch_point(1, Vector2::create(10.0, 10.0), Vector2::create(20.0, 20.0));
		input_manager.begin_touch_point(2, Vector2::create(30.0, 30.0), Vector2::create(60.0, 60.0));
		input_manager.move_touch_point(2, Vector2::create(31.0, 30.0), Vector2::create(62.0, 60.0));
		let touch_points = input_manager.get_touch_points();
		assert_eq!(touch_points.len(), 2);
		assert!(touch_points.iter().all(|touch_point| touch_point.phase == TouchPhase::Began));
		assert_eq!(touch_points[1].view_position, Vector2::create(62.0, 60.0));
		input_manager.update_frame_snapshot();
		input_manager.move_touch_point(2, Vector2::create(40.0, 30.0), Vector2::create(80.0, 60.0));
		let touch_points = input_manager.get_touch_points();
		assert_eq!(touch_points[0].phase, TouchPhase::Stationary);
		assert_eq!(touch_points[1].phase, TouchPhase::Moved);
		input_manager.end_touch_point(1, Vector2::create(11.0, 10.0), Vector2::create(22.0, 20.0), false);
		input_manager.end_touch_point(9, Vector2::zero(), Vector2::zero(), true);
		assert_eq!(input_manager.get_touch_points().len(), 1);
		let ended_touch_points = input_manager.get_ended_touch_points();
		assert_eq!(ended_touch_points.len(), 1);
		assert_eq!(ended_touch_points[0].identifier, 1);
		assert_eq!(ended_touch_points[0].phase, TouchPhase::Ended);
		input_manager.update_frame_snapshot();
		assert!(input_manager.get_ended_touch_points().is_empty());
		input_manager.end_touch_point(2, Vector2::zero(), Vector2::zero(), true);
		assert_eq!(input_manager.get_ended_touch_points()[0].phase, TouchPhase::Cancelled);
	}

	//==============================================================================
	/// 키 되풀이는 새 누름이 아니고, 막을 키는 정한 것만이다.
	//==============================================================================
	#[test]
	fn key_repeat_and_prevented_keys() {
		let mut input_manager = InputManager::new();
		input_manager.push_key("KeyA");
		input_manager.update_frame_snapshot();
		input_manager.push_key("KeyA");
		assert!(!input_manager.is_key_just_pressed("KeyA"));
		assert!(input_manager.is_key_pressed("KeyA"));
		input_manager.set_prevented_keys(&["Tab", "Space"]);
		assert!(input_manager.is_key_prevented("Tab"));
		assert!(!input_manager.is_key_prevented("KeyQ"));
		assert_eq!(input_manager.get_prevented_keys().len(), 2);
		assert!(!input_manager.is_typing_in_text_field());
		assert_eq!(input_manager.get_last_input_device(), InputDevice::Keyboard);
	}

	//==============================================================================
	/// 원본처럼 처음에는 어느 단추든 터치, 왼쪽만으로 바꿀 수 있다. 오른쪽 클릭 메뉴는 처음에 막는다. 닿은 손가락 찾기.
	//==============================================================================
	#[test]
	fn touch_buttons_context_menu_and_find_touch_point() {
		let mut input_manager = InputManager::new();
		assert!(!input_manager.is_touch_from_left_button_only());
		assert!(input_manager.is_touch_button(MouseButton::Right));
		input_manager.set_touch_from_left_button_only(true);
		assert!(input_manager.is_touch_button(MouseButton::Left));
		assert!(!input_manager.is_touch_button(MouseButton::Middle));
		assert!(input_manager.is_context_menu_prevented());
		input_manager.set_context_menu_prevented(false);
		assert!(!input_manager.is_context_menu_prevented());
		input_manager.begin_touch_point(7, Vector2::create(1.0, 2.0), Vector2::create(3.0, 4.0));
		assert_eq!(input_manager.find_touch_point(7).map(|touch_point| touch_point.view_position), Some(Vector2::create(3.0, 4.0)));
		assert!(input_manager.find_touch_point(8).is_none());
	}
}
