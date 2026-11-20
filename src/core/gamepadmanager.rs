//==============================================================================
// 게임패드 관리자.
// - 원본 core/gamepadmanager.js 를 옮겼다. 공개 이름(GamepadButtonCode, GamepadAnalogStickCode, GamepadTriggerCode,
//   connect, disconnect, set_callback, update_all_gamepads, is_button_pressed, get_button_value, get_axis_value,
//   get_all_connected_gamepads, get_connected_gamepad_count, get_connected_gamepad)과 갱신 규칙은 원본 그대로다.
//   (값은 소수점 둘째 자리로 반올림, 축은 0.01 넘게 바뀔 때만 받기, 아날로그 값은 0 보다 클 때만 알리기, 로그 문구)
// - 원본은 navigator.getGamepads() 를 읽는다. 러스트 판은 gilrs 를 읽어 원본과 같은 W3C Standard Gamepad 꼴
//   (버튼 0~16, 축 0~3, 축 Y 는 아래가 양수)의 Gamepad 목록으로 바꾼 뒤 같은 규칙을 돌린다.
//   데스크탑은 gilrs 가 OS 입력(윈도우는 Windows.Gaming.Input)을, 웹(wasm32)은 gilrs 가 navigator.getGamepads() 를 읽는다.
// - 원본과 다른 점:
//   - gilrs 기본 필터(데드존, 떨림 거르기)는 끈다. 브라우저처럼 날값을 받고, 데드존은 원본처럼 게임이 건다.
//   - 아날로그 버튼(트리거)의 눌림은 크로미엄과 같이 값이 30/255 를 넘을 때다. (gilrs 기본은 0.75 에 눌림, 0.65 에 뗌)
//   - 축(햇 스위치)으로 오는 십자키는 버튼 12~15 로 바꾼다. (브라우저가 하는 일)
//   - 버튼은 17개, 축은 4개다. TOUCHPAD(17) 는 gilrs 가 주지 않아 늘 false 다.
//   - Gamepad.index 는 gilrs 의 GamepadId 번호다. (처음 붙은 순서로 0 부터, 브라우저 index 와 같은 쓰임)
//   - Gamepad.id 는 웹에서는 브라우저 id 그대로, 데스크탑에서는 크롬 꼴 "이름 (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)" 로 만든다.
//   - get_all_connected_gamepads / get_connected_gamepad 는 마지막 갱신(tick)에 읽은 사본을 돌려준다. (원본은 부를 때마다 새로 읽는다)
//   - 콜백의 button 값은 bool 대신 1.0(눌림) / 0.0(뗌) 이다.
//   - gilrs 는 처음 갱신할 때 만든다. (갱신하지 않는 엔진과 시험에서는 장치를 열지 않는다)
//   - 더한 것: is_button_just_pressed / is_button_just_released (갱신 사이의 눌림 변화), set_log_enabled (로그 켜고 끄기),
//     update_all_gamepads_with_list (gilrs 없이 목록을 넣는 길. 시험이나 다른 입력원)
//   - 진동, 배터리 정보는 원본에 없어 옮기지 않는다.
//==============================================================================
use gilrs::{Axis, Button, GilrsBuilder};
use std::collections::HashMap;

// W3C Standard Gamepad 의 버튼 수. (0~16)
const STANDARD_BUTTON_COUNT: usize = 17;
// W3C Standard Gamepad 의 축 수. (0~3)
const STANDARD_AXIS_COUNT: usize = 4;
// 아날로그 버튼을 눌림으로 보는 값. (크로미엄 GamepadButton::kDefaultButtonPressedThreshold, XInput 트리거 데드존과 같다)
const BUTTON_PRESSED_THRESHOLD: f32 = 30.0 / 255.0;
// 축(햇 스위치)으로 오는 십자키를 눌림으로 보는 값.
const DIRECTIONAL_PAD_AXIS_THRESHOLD: f32 = 0.5;
// 축 변화를 받는 최소 폭. (원본: 미세한 떨림을 거르려 0.01 넘는 변화만 받는다)
const AXIS_CHANGE_THRESHOLD: f64 = 0.01;

// W3C Standard Gamepad 버튼 순서의 gilrs 버튼. (배열 번호가 GamepadButtonCode)
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

// W3C Standard Gamepad 축 순서의 gilrs 축과 곱할 부호. (gilrs 의 Y 는 위가 양수, W3C 는 아래가 양수라 뒤집는다)
const STANDARD_AXIS_LIST: [(Axis, f32); STANDARD_AXIS_COUNT] = [
	(Axis::LeftStickX, 1.0),
	(Axis::LeftStickY, -1.0),
	(Axis::RightStickX, 1.0),
	(Axis::RightStickY, -1.0),
];

//==============================================================================
// 게임패드 아날로그 스틱 식별자. (W3C Standard Gamepad 매핑 규격)
//==============================================================================
pub struct GamepadAnalogStickCode;

impl GamepadAnalogStickCode {
	pub const LEFT_X: usize = 0;
	pub const LEFT_Y: usize = 1;
	pub const RIGHT_X: usize = 2;
	pub const RIGHT_Y: usize = 3;
}

//==============================================================================
// 게임패드 트리거 식별자. (W3C Standard Gamepad 매핑 규격)
//==============================================================================
pub struct GamepadTriggerCode;

impl GamepadTriggerCode {
	pub const L2: usize = 6;
	pub const R2: usize = 7;
}

//==============================================================================
// 게임패드 버튼 식별자. (W3C Standard Gamepad 매핑 규격)
//==============================================================================
pub struct GamepadButtonCode;

impl GamepadButtonCode {
	// 앞면 버튼.
	pub const A_CROSS: usize = 0;
	pub const B_CIRCLE: usize = 1;
	pub const X_SQUARE: usize = 2;
	pub const Y_TRIANGLE: usize = 3;

	// 범퍼.
	pub const L1: usize = 4;
	pub const R1: usize = 5;

	// 트리거. (버튼으로도 인식됨)
	pub const L2: usize = 6;
	pub const R2: usize = 7;

	// 시스템.
	pub const SHARE_VIEW: usize = 8;
	pub const OPTIONS_MENU: usize = 9;

	// 스틱 누름.
	pub const L3: usize = 10;
	pub const R3: usize = 11;

	// 십자키.
	pub const DPAD_UP: usize = 12;
	pub const DPAD_DOWN: usize = 13;
	pub const DPAD_LEFT: usize = 14;
	pub const DPAD_RIGHT: usize = 15;

	// 가운데.
	pub const HOME_PS: usize = 16;
	pub const TOUCHPAD: usize = 17;
}

//==============================================================================
// 입력 종류. (원본 콜백의 input_type 문자열 "button" / "value" / "axis")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamepadInputType {
	// 눌림 변화. (값은 1.0 눌림, 0.0 뗌)
	Button,
	// 버튼 아날로그 값 변화. (0 보다 클 때만 온다)
	Value,
	// 스틱 축 값 변화.
	Axis,
}

// 입력 콜백. (gamepad_index, input_type, input_index, value)
pub type GamepadInputCallback = Box<dyn FnMut(usize, GamepadInputType, usize, f32)>;

//==============================================================================
// 게임패드 버튼 하나. (W3C GamepadButton)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GamepadButton {
	// 눌렸는지.
	pub pressed: bool,
	// 값. (0.0 ~ 1.0, 트리거는 아날로그)
	pub value: f32,
}

//==============================================================================
// 게임패드 하나의 사본. (W3C Gamepad, navigator.getGamepads() 가 주는 것과 같은 꼴)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct Gamepad {
	// 하드웨어 식별 번호. (is_button_pressed 들의 gamepad_index)
	pub index: usize,
	// 장치 이름.
	pub id: String,
	// 연결되어 있는지.
	pub connected: bool,
	// 버튼 목록. (GamepadButtonCode 순서)
	pub buttons: Vec<GamepadButton>,
	// 축 목록. (GamepadAnalogStickCode 순서, -1.0 ~ 1.0)
	pub axes: Vec<f32>,
}

impl Gamepad {
	//==============================================================================
	// 생성. (버튼 17개는 뗀 상태, 축 4개는 0)
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
// 버튼 하나의 마지막 상태. (원본 { pressed, value })
//==============================================================================
#[derive(Clone, Copy)]
struct GamepadButtonState {
	pressed: bool,
	value: f64,
	// 지난 갱신의 눌림. (러스트 판에서 더한 Just 판정용)
	previous_pressed: bool,
}

//==============================================================================
// 게임패드 하나의 마지막 상태. (원본 { buttons: Map, axes: Map })
//==============================================================================
struct GamepadState {
	buttons: Vec<GamepadButtonState>,
	axes: Vec<f64>,
}

//==============================================================================
// 게임패드 관리자.
//==============================================================================
pub struct GamepadManager {
	// 연결된 게임패드 식별자(index) 목록.
	connected_gamepad_indices: Vec<usize>,
	// key: index, value: 마지막 상태.
	connected_gamepad_states: HashMap<usize, GamepadState>,
	// 입력 콜백 함수.
	input_event_callback: Option<GamepadInputCallback>,
	// 마지막 갱신에 읽은 목록. (navigator.getGamepads() 자리, 번호가 index)
	gamepad_list: Vec<Option<Gamepad>>,
	// gilrs 입력원. (처음 갱신할 때 만든다)
	gamepad_reader: Option<GamepadReader>,
	// gilrs 를 만들어 봤는지.
	is_gamepad_reader_creation_tried: bool,
	// 로그를 내는지.
	is_log_enabled: bool,
}

impl GamepadManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> GamepadManager {
		GamepadManager {
			connected_gamepad_indices: Vec::new(),
			connected_gamepad_states: HashMap::new(),
			input_event_callback: None,
			gamepad_list: Vec::new(),
			gamepad_reader: None,
			is_gamepad_reader_creation_tried: false,
			is_log_enabled: true,
		}
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	pub fn tick(&mut self, _time_delta: f32) {
		self.update_all_gamepads();
	}

	//==============================================================================
	// 게임패드 연결.
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
	// 게임패드 연결 해제.
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
	// 콜백 설정. (None 이면 끈다)
	//==============================================================================
	pub fn set_callback(&mut self, callback: Option<GamepadInputCallback>) {
		self.input_event_callback = callback;
	}

	//==============================================================================
	// 로그 켜고 끄기. (러스트 판에서 더한 것. 기본은 원본처럼 켜짐)
	//==============================================================================
	pub fn set_log_enabled(&mut self, is_enabled: bool) {
		self.is_log_enabled = is_enabled;
	}

	//==============================================================================
	// 모든 게임패드 갱신. (gilrs 를 읽어 update_all_gamepads_with_list 로 넘긴다)
	//==============================================================================
	pub fn update_all_gamepads(&mut self) {
		if !self.is_gamepad_reader_creation_tried {
			self.is_gamepad_reader_creation_tried = true;
			self.gamepad_reader = GamepadReader::create();
		}
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

		self.update_all_gamepads_with_list(read_result.gamepad_list);
	}

	//==============================================================================
	// 주어진 목록으로 모든 게임패드 갱신. (원본 update_all_gamepads 의 본문, 목록 번호가 index)
	// - gilrs 없이 상태를 넣을 때 쓴다. (시험, 다른 입력원)
	//==============================================================================
	pub fn update_all_gamepads_with_list(&mut self, gamepad_list: Vec<Option<Gamepad>>) {
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

		let is_log_enabled = self.is_log_enabled;
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

					// 로그 출력.
					if is_log_enabled {
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
						if is_log_enabled {
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
					*last_axis_value = current_axis_value;

					// 로그 출력.
					if is_log_enabled {
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
	}

	//==============================================================================
	// 버튼 눌림 상태 반환.
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
	// 이번 갱신에 방금 눌린 버튼인지. (러스트 판에서 더한 것. 키보드 is_key_just_pressed 와 같은 쓰임)
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
	// 이번 갱신에 방금 떼어진 버튼인지. (러스트 판에서 더한 것)
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
	// 버튼/트리거 아날로그 값 반환. (0.0 ~ 1.0)
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
	// 아날로그 스틱 축 값 반환. (-1.0 ~ 1.0)
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
	// 연결된 모든 게임패드 반환. (마지막 갱신의 사본)
	//==============================================================================
	pub fn get_all_connected_gamepads(&self) -> Vec<&Gamepad> {
		let mut connected_gamepad_list = Vec::new();
		for hardware_index in self.connected_gamepad_indices.iter() {
			if let Some(Some(gamepad)) = self.gamepad_list.get(*hardware_index) {
				connected_gamepad_list.push(gamepad);
			}
		}
		connected_gamepad_list
	}

	//==============================================================================
	// 연결된 게임패드 수 반환.
	//==============================================================================
	pub fn get_connected_gamepad_count(&self) -> usize {
		self.connected_gamepad_indices.len()
	}

	//==============================================================================
	// 연결된 게임패드 반환. (gamepad_index 는 연결 목록의 순번, 0 ~ get_connected_gamepad_count() - 1)
	//==============================================================================
	pub fn get_connected_gamepad(&self, gamepad_index: usize) -> Option<&Gamepad> {
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
	// 버튼 상태 찾기.
	//==============================================================================
	fn find_button_state(&self, gamepad_index: usize, button_index: usize) -> Option<&GamepadButtonState> {
		let state = self.connected_gamepad_states.get(&gamepad_index)?;
		state.buttons.get(button_index)
	}
}

impl Default for GamepadManager {
	fn default() -> GamepadManager {
		GamepadManager::new()
	}
}

//==============================================================================
// 로그 문구의 입력 이름. (L2, R2 는 Trigger)
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
// 소수점 둘째 자리로 반올림. (원본 parseFloat(value.toFixed(2)) 자리)
//==============================================================================
fn round_to_hundredths(value: f64) -> f64 {
	(value * 100.0).round() / 100.0
}

//==============================================================================
// 0, -0, NaN 을 0 으로 바꾼다. (원본 `value || 0` 자리)
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
// 로그 출력. (원본 console.log 자리. 웹은 브라우저 콘솔, 데스크탑은 표준 오류)
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
// 패드 하나에서 버튼과 축을 읽는 것. (gilrs 패드, 시험용 가짜 패드)
//==============================================================================
trait GamepadElementReader {
	// 버튼의 눌림과 값.
	fn read_button(&self, button: Button) -> GamepadButton;
	// 축의 값. (gilrs 꼴, Y 는 위가 양수)
	fn read_axis(&self, axis: Axis) -> f32;
}

impl GamepadElementReader for gilrs::Gamepad<'_> {
	//==============================================================================
	// 버튼 읽기. (gilrs 가 모르는 버튼은 뗀 상태)
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
	// 축 읽기. (gilrs 가 모르는 축은 0)
	//==============================================================================
	fn read_axis(&self, axis: Axis) -> f32 {
		self.value(axis)
	}
}

//==============================================================================
// 십자키 버튼에 겹쳐 읽을 축과 방향. (햇 스위치를 축으로 주는 패드. gilrs DPadX 는 오른쪽, DPadY 는 위가 양수)
//==============================================================================
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
// 패드 하나를 W3C Standard Gamepad 꼴로 읽는다.
// - 아날로그 버튼은 값이 30/255 를 넘으면 눌림. 눌린 디지털 버튼의 값은 1.
// - 축은 Y 를 뒤집어 아래가 양수가 되게 한다.
//==============================================================================
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
// 데스크탑 패드 이름. (크롬 id 꼴. 게임이 "054c" 같은 제조사 번호로 패드 종류를 가른다)
//==============================================================================
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
// gilrs 패드를 Gamepad 로.
//==============================================================================
fn create_gamepad_from_gilrs(gamepad_id: gilrs::GamepadId, gilrs_gamepad: &gilrs::Gamepad) -> Gamepad {
	let index = usize::from(gamepad_id);
	let gamepad_name = gilrs_gamepad.name();
	// 웹은 gilrs 가 브라우저 id 를 이름으로 준다.
	let id = if cfg!(target_arch = "wasm32") {
		gamepad_name.to_string()
	}
	else {
		let vendor_id = gilrs_gamepad.vendor_id();
		let product_id = gilrs_gamepad.product_id();
		create_desktop_gamepad_identifier(gamepad_name, vendor_id, product_id)
	};
	let mut gamepad = create_standard_gamepad(index, &id, gilrs_gamepad);
	gamepad.connected = gilrs_gamepad.is_connected();
	gamepad
}

//==============================================================================
// gilrs 에서 이번 갱신에 읽은 것.
//==============================================================================
#[derive(Default)]
struct GamepadReadResult {
	// 이번에 붙은 패드 index.
	connected_gamepad_index_list: Vec<usize>,
	// 이번에 떨어진 패드 index.
	disconnected_gamepad_index_list: Vec<usize>,
	// 연결된 패드 목록. (번호가 index, 빈 칸은 None)
	gamepad_list: Vec<Option<Gamepad>>,
}

//==============================================================================
// gilrs 입력원. (gilrs 를 들고 W3C 꼴 목록을 만든다)
//==============================================================================
struct GamepadReader {
	gilrs: gilrs::Gilrs,
}

impl GamepadReader {
	//==============================================================================
	// 생성. (기본 필터와 진동은 끈다. 게임패드를 모르는 플랫폼이면 패드가 없는 것처럼 돈다)
	//==============================================================================
	fn create() -> Option<GamepadReader> {
		let gilrs_builder = GilrsBuilder::new().with_default_filters(false).with_force_feedback(false);
		match gilrs_builder.build() {
			Ok(gilrs) => {
				Some(GamepadReader { gilrs })
			}
			Err(gilrs::Error::NotImplemented(gilrs)) => {
				write_log("[GamepadManager] 이 플랫폼은 게임패드를 지원하지 않습니다.");
				Some(GamepadReader { gilrs })
			}
			Err(error) => {
				write_log(&format!("[GamepadManager] gilrs 를 만들지 못했습니다: {}", error));
				None
			}
		}
	}

	//==============================================================================
	// 읽기. (이벤트를 비워야 gilrs 의 상태가 갱신된다. 웹은 이때 navigator.getGamepads() 를 읽는다)
	//==============================================================================
	fn read_gamepads(&mut self) -> GamepadReadResult {
		let mut read_result = GamepadReadResult::default();
		while let Some(event) = self.gilrs.next_event() {
			let hardware_index = usize::from(event.id);
			match event.event {
				gilrs::EventType::Connected => {
					read_result.connected_gamepad_index_list.push(hardware_index);
				}
				gilrs::EventType::Disconnected => {
					read_result.disconnected_gamepad_index_list.push(hardware_index);
				}
				_ => {
				}
			}
		}
		for (gamepad_id, gilrs_gamepad) in self.gilrs.gamepads() {
			let gamepad = create_gamepad_from_gilrs(gamepad_id, &gilrs_gamepad);
			let hardware_index = gamepad.index;
			if read_result.gamepad_list.len() <= hardware_index {
				read_result.gamepad_list.resize(hardware_index + 1, None);
			}
			read_result.gamepad_list[hardware_index] = Some(gamepad);
		}
		read_result
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	// 콜백으로 받은 입력 하나. (gamepad_index, input_type, input_index, value)
	type ReceivedInputEvent = (usize, GamepadInputType, usize, f32);

	//==============================================================================
	// 시험용 가짜 패드. (눌린 버튼과 값, 축 값을 직접 준다)
	//==============================================================================
	struct TestElementReader {
		button_list: Vec<(Button, GamepadButton)>,
		axis_list: Vec<(Axis, f32)>,
	}

	impl GamepadElementReader for TestElementReader {
		//==============================================================================
		// 버튼 읽기.
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
		// 축 읽기.
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
	// 가짜 패드에서 버튼 하나만 디지털로 눌러 읽는다.
	//==============================================================================
	fn read_with_pressed_button(button: Button) -> Gamepad {
		let element_reader = TestElementReader {
			button_list: vec![(button, GamepadButton { pressed: true, value: 1.0 })],
			axis_list: Vec::new(),
		};
		create_standard_gamepad(0, "test", &element_reader)
	}

	//==============================================================================
	// 시험용 관리자. (gilrs 없이, 로그 끔)
	//==============================================================================
	fn create_test_manager() -> GamepadManager {
		let mut gamepad_manager = GamepadManager::new();
		gamepad_manager.set_log_enabled(false);
		gamepad_manager
	}

	//==============================================================================
	// gilrs 버튼이 W3C 번호(원본 GamepadButtonCode)로 간다.
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
	// 축은 W3C 순서이고 Y 는 아래가 양수다.
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
	// 트리거는 값이 30/255 를 넘으면 눌림이고 값은 그대로 간다.
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
	// 축(햇 스위치)으로 오는 십자키는 버튼 12~15 가 된다.
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
	// 데스크탑 패드 이름은 크롬 id 꼴이다.
	//==============================================================================
	#[test]
	fn desktop_gamepad_identifier() {
		let play_station_identifier = create_desktop_gamepad_identifier("Wireless Controller", Some(0x054c), Some(0x09cc));
		assert_eq!(play_station_identifier, "Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)");
		let unknown_identifier = create_desktop_gamepad_identifier("Pad", None, None);
		assert_eq!(unknown_identifier, "Pad (STANDARD GAMEPAD)");
	}

	//==============================================================================
	// 축은 소수점 둘째 자리로 반올림하고 0.01 넘게 바뀔 때만 받는다. (원본 데드존)
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
	// 방금 눌림 / 방금 뗌은 갱신 하나 동안만 켜진다.
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
	// 콜백: 눌림 변화는 button, 0 보다 큰 값 변화만 value, 축은 axis.
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
	// 목록에 나타나면 붙고, 사라지면 놓는다. (원본 update_all_gamepads 의 보완 훑기)
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
	// connect 는 같은 패드를 두 번 붙이지 않고, disconnect 는 붙은 패드만 뗀다.
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
	// 반올림은 원본 toFixed(2) 와 같은 값을 낸다.
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
