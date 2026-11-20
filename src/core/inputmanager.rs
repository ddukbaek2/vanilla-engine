//==============================================================================
// 입력 관리자.
// - 키 이름은 원본과 같이 KeyboardEvent.code 꼴이다. ("KeyA", "ArrowLeft", "Space" ...)
//   winit 의 KeyCode 이름이 같은 W3C 규약을 따르므로 그대로 문자열로 바꿔 쓴다.
// - 마우스 왼쪽 버튼(또는 첫 손가락)을 터치로 다룬다. (원본은 마우스와 터치를 하나로 모은다)
// - 그 밖에 마우스 단추 셋(왼쪽, 가운데, 오른쪽)의 눌림, 닿아 있는 손가락 목록, 마지막 입력이 손가락인지, 휠을 Ctrl 과 함께 굴렸는지를 둔다.
//   카메라 손짓(CameraGestures)처럼 단추 하나 터치 밖의 입력을 쓰는 곳이 읽는다. 플랫폼이 채운다. (vanilla.js 는 캔버스 DOM 사건을 직접 읽던 것)
// - 게임패드는 원본과 같이 InputManager 가 GamepadManager 를 들고 tick 에서 폴링한다. (core/gamepadmanager.rs)
// - 글 입력(확정 글, IME 조합, 편집 키)은 TextInput 줄에 모은다. 입력 필드가 꺼내 쓴다. (core/textinput.rs, 원본에 없는 층)
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::gamepadmanager::{Gamepad, GamepadManager};
use crate::core::textinput::TextInput;
use std::collections::HashSet;

//==============================================================================
// 키 이름. (원본 KeyCode)
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
}

//==============================================================================
// 마우스 단추.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseButton {
	Left,
	Middle,
	Right,
}

impl MouseButton {
	//==============================================================================
	// 단추 칸 번호.
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
// 닿아 있는 손가락 하나. (자리는 캔버스 논리 좌표, 플랫폼의 손가락 번호)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TouchPoint {
	pub identifier: i64,
	pub canvas_native_position: Vector2,
}

pub struct InputManager {
	pressed_keys: HashSet<String>,
	previous_pressed_keys: HashSet<String>,
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
	previous_mouse_buttons_down: [bool; 3],
	touch_points: Vec<TouchPoint>,
	is_touch_input: bool,
	is_wheel_with_ctrl: bool,
}

impl InputManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> InputManager {
		InputManager {
			pressed_keys: HashSet::new(),
			previous_pressed_keys: HashSet::new(),
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
			previous_mouse_buttons_down: [false; 3],
			touch_points: Vec::new(),
			is_touch_input: false,
			is_wheel_with_ctrl: false,
		}
	}

	//==============================================================================
	// 주기적 갱신. (게임패드 폴링)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.tick(time_delta);
	}

	//==============================================================================
	// 모든 입력 상태 비우기. (창이 포커스를 잃었을 때)
	//==============================================================================
	pub fn clear(&mut self) {
		self.pressed_keys.clear();
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
	// 휠 이동량 누적.
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
	// 키 눌림 기록.
	//==============================================================================
	pub fn push_key(&mut self, key: &str) {
		self.pressed_keys.insert(key.to_string());
	}

	//==============================================================================
	// 키 뗌 기록.
	//==============================================================================
	pub fn pop_key(&mut self, key: &str) {
		self.pressed_keys.remove(key);
	}

	//==============================================================================
	// 키가 눌려 있는지.
	//==============================================================================
	pub fn is_key_pressed(&self, key: &str) -> bool {
		self.pressed_keys.contains(key)
	}

	//==============================================================================
	// 이번 프레임에 눌렸는지.
	//==============================================================================
	pub fn is_key_just_pressed(&self, key: &str) -> bool {
		self.pressed_keys.contains(key) && !self.previous_pressed_keys.contains(key)
	}

	//==============================================================================
	// 이번 프레임에 떼졌는지.
	//==============================================================================
	pub fn is_key_just_released(&self, key: &str) -> bool {
		!self.pressed_keys.contains(key) && self.previous_pressed_keys.contains(key)
	}

	//==============================================================================
	// 프레임 끝 스냅샷. (다음 프레임의 Just 판정 기준)
	//==============================================================================
	pub fn update_frame_snapshot(&mut self) {
		self.previous_pressed_keys = self.pressed_keys.clone();
		self.previous_mouse_buttons_down = self.mouse_buttons_down;
	}

	//==============================================================================
	// 마우스 단추 눌림 설정. (플랫폼이 부른다)
	//==============================================================================
	pub fn set_mouse_button_down(&mut self, mouse_button: MouseButton, is_down: bool) {
		self.mouse_buttons_down[mouse_button.get_index()] = is_down;
	}

	//==============================================================================
	// 마우스 단추가 눌려 있는지.
	//==============================================================================
	pub fn is_mouse_button_down(&self, mouse_button: MouseButton) -> bool {
		self.mouse_buttons_down[mouse_button.get_index()]
	}

	//==============================================================================
	// 이번 프레임에 마우스 단추를 눌렀는지.
	//==============================================================================
	pub fn is_mouse_button_just_pressed(&self, mouse_button: MouseButton) -> bool {
		let button_index = mouse_button.get_index();
		self.mouse_buttons_down[button_index] && !self.previous_mouse_buttons_down[button_index]
	}

	//==============================================================================
	// 이번 프레임에 마우스 단추를 뗐는지.
	//==============================================================================
	pub fn is_mouse_button_just_released(&self, mouse_button: MouseButton) -> bool {
		let button_index = mouse_button.get_index();
		!self.mouse_buttons_down[button_index] && self.previous_mouse_buttons_down[button_index]
	}

	//==============================================================================
	// 닿아 있는 손가락 목록 설정. (플랫폼이 손가락이 바뀔 때마다 통째로 넘긴다)
	//==============================================================================
	pub fn set_touch_points(&mut self, touch_points: Vec<TouchPoint>) {
		self.touch_points = touch_points;
	}

	//==============================================================================
	// 닿아 있는 손가락 목록.
	//==============================================================================
	pub fn get_touch_points(&self) -> &[TouchPoint] {
		&self.touch_points
	}

	//==============================================================================
	// 마지막 입력이 손가락인지 설정. (플랫폼이 부른다)
	//==============================================================================
	pub fn set_touch_input(&mut self, is_touch_input: bool) {
		self.is_touch_input = is_touch_input;
	}

	//==============================================================================
	// 마지막 입력이 손가락인지. (마우스면 false)
	//==============================================================================
	pub fn is_touch_input(&self) -> bool {
		self.is_touch_input
	}

	//==============================================================================
	// 마지막 휠을 Ctrl 과 함께 굴렸는지 설정. (노트북 터치패드의 오므리기도 Ctrl 휠로 온다)
	//==============================================================================
	pub fn set_wheel_with_ctrl(&mut self, is_wheel_with_ctrl: bool) {
		self.is_wheel_with_ctrl = is_wheel_with_ctrl;
	}

	//==============================================================================
	// 마지막 휠을 Ctrl 과 함께 굴렸는지.
	//==============================================================================
	pub fn is_wheel_with_ctrl(&self) -> bool {
		self.is_wheel_with_ctrl
	}

	//==============================================================================
	// 터치 상태. (Pressed / Released / Cancelled / Moved 는 그 프레임에만 켜진다)
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
	// 터치가 눌린 채인지. (러스트 판에서 더한 것. 눌린 동안 계속 켜져 있다)
	//==============================================================================
	pub fn set_touch_down(&mut self, value: bool) {
		self.is_touch_down = value;
	}

	pub fn is_touch_down(&self) -> bool {
		self.is_touch_down
	}

	//==============================================================================
	// 입력 위치. (창 안쪽 논리 좌표 / 뷰 좌표)
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
	// 모든 게임패드 갱신.
	//==============================================================================
	pub fn update_all_gamepads(&mut self) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.update_all_gamepads();
	}

	//==============================================================================
	// 게임패드 연결.
	//==============================================================================
	pub fn connect_gamepad(&mut self, gamepad: &Gamepad) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.connect(gamepad);
	}

	//==============================================================================
	// 게임패드 연결 해제.
	//==============================================================================
	pub fn disconnect_gamepad(&mut self, gamepad: &Gamepad) {
		let gamepad_manager = self.get_gamepad_manager_mut();
		gamepad_manager.disconnect(gamepad);
	}

	//==============================================================================
	// 게임패드 매니저 반환.
	//==============================================================================
	pub fn get_gamepad_manager(&self) -> &GamepadManager {
		&self.gamepad_manager
	}

	//==============================================================================
	// 게임패드 매니저 반환. (바꿀 때: set_callback, connect 들)
	//==============================================================================
	pub fn get_gamepad_manager_mut(&mut self) -> &mut GamepadManager {
		&mut self.gamepad_manager
	}

	//==============================================================================
	// 글 입력 줄 반환. (입력 필드가 꺼내 쓰고 상태를 올린다. &self 로 된다)
	//==============================================================================
	pub fn get_text_input(&self) -> &TextInput {
		&self.text_input
	}

	//==============================================================================
	// 글 입력 줄 반환. (플랫폼이 이벤트를 넣을 때)
	//==============================================================================
	pub fn get_text_input_mut(&mut self) -> &mut TextInput {
		&mut self.text_input
	}
}

impl Default for InputManager {
	fn default() -> InputManager {
		InputManager::new()
	}
}
