//==============================================================================
// 게임패드 관리자.
// - 원본 core/gamepadmanager.js 를 옮겼다. 공개 이름(GamepadButtonCode, GamepadAnalogStickCode, GamepadTriggerCode,
//   connect, disconnect, setCallback, updateAllGamepads, isButtonPressed, getButtonValue, getAxisValue,
//   getAllConnectedGamepads, getConnectedGamepadCount, getConnectedGamepad)과 갱신 규칙은 원본 그대로다.
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
//   - getAllConnectedGamepads / getConnectedGamepad 는 마지막 갱신(tick)에 읽은 사본을 돌려준다. (원본은 부를 때마다 새로 읽는다)
//   - 콜백의 button 값은 bool 대신 1.0(눌림) / 0.0(뗌) 이다.
//   - gilrs 는 처음 갱신할 때 만든다. (갱신하지 않는 엔진과 시험에서는 장치를 열지 않는다)
//   - 더한 것: isButtonJustPressed / isButtonJustReleased (갱신 사이의 눌림 변화), setLogEnabled (로그 켜고 끄기),
//     updateAllGamepadsWithList (gilrs 없이 목록을 넣는 길. 시험이나 다른 입력원)
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
// 입력 종류. (원본 콜백의 inputType 문자열 "button" / "value" / "axis")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamepadInputType {
	// 눌림 변화. (값은 1.0 눌림, 0.0 뗌)
	button,
	// 버튼 아날로그 값 변화. (0 보다 클 때만 온다)
	value,
	// 스틱 축 값 변화.
	axis,
}

// 입력 콜백. (gamepadIndex, inputType, inputIndex, value)
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
	// 하드웨어 식별 번호. (isButtonPressed 들의 gamepadIndex)
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
	previousPressed: bool,
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
	connectedGamepadIndices: Vec<usize>,
	// key: index, value: 마지막 상태.
	connectedGamepadStates: HashMap<usize, GamepadState>,
	// 입력 콜백 함수.
	inputEventCallback: Option<GamepadInputCallback>,
	// 마지막 갱신에 읽은 목록. (navigator.getGamepads() 자리, 번호가 index)
	gamepadList: Vec<Option<Gamepad>>,
	// gilrs 입력원. (처음 갱신할 때 만든다)
	gamepadReader: Option<GamepadReader>,
	// gilrs 를 만들어 봤는지.
	isGamepadReaderCreationTried: bool,
	// 로그를 내는지.
	isLogEnabled: bool,
}

impl GamepadManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> GamepadManager {
		GamepadManager {
			connectedGamepadIndices: Vec::new(),
			connectedGamepadStates: HashMap::new(),
			inputEventCallback: None,
			gamepadList: Vec::new(),
			gamepadReader: None,
			isGamepadReaderCreationTried: false,
			isLogEnabled: true,
		}
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	pub fn tick(&mut self, _timeDelta: f32) {
		self.updateAllGamepads();
	}

	//==============================================================================
	// 게임패드 연결.
	//==============================================================================
	pub fn connect(&mut self, gamepad: &Gamepad) {
		if self.connectedGamepadIndices.contains(&gamepad.index) {
			return;
		}

		// 버튼, 축 초기화.
		let buttonCount = gamepad.buttons.len();
		let axisCount = gamepad.axes.len();
		let buttonStateList = vec![GamepadButtonState { pressed: false, value: 0.0, previousPressed: false }; buttonCount];
		let axisValueList = vec![0.0; axisCount];
		let state = GamepadState {
			buttons: buttonStateList,
			axes: axisValueList,
		};

		self.connectedGamepadIndices.push(gamepad.index);
		self.connectedGamepadStates.insert(gamepad.index, state);
		if self.isLogEnabled {
			writeLog(&format!("[GamepadManager] Connected: {} at index {}", gamepad.id, gamepad.index));
		}
	}

	//==============================================================================
	// 게임패드 연결 해제.
	//==============================================================================
	pub fn disconnect(&mut self, gamepad: &Gamepad) {
		let listIndex = match self.connectedGamepadIndices.iter().position(|connectedGamepadIndex| *connectedGamepadIndex == gamepad.index) {
			Some(listIndex) => {
				listIndex
			}
			None => {
				return;
			}
		};

		self.connectedGamepadIndices.remove(listIndex);
		self.connectedGamepadStates.remove(&gamepad.index);
		if self.isLogEnabled {
			writeLog(&format!("[GamepadManager] Disconnected: index {}", gamepad.index));
		}
	}

	//==============================================================================
	// 콜백 설정. (None 이면 끈다)
	//==============================================================================
	pub fn setCallback(&mut self, callback: Option<GamepadInputCallback>) {
		self.inputEventCallback = callback;
	}

	//==============================================================================
	// 로그 켜고 끄기. (러스트 판에서 더한 것. 기본은 원본처럼 켜짐)
	//==============================================================================
	pub fn setLogEnabled(&mut self, isEnabled: bool) {
		self.isLogEnabled = isEnabled;
	}

	//==============================================================================
	// 모든 게임패드 갱신. (gilrs 를 읽어 updateAllGamepadsWithList 로 넘긴다)
	//==============================================================================
	pub fn updateAllGamepads(&mut self) {
		if !self.isGamepadReaderCreationTried {
			self.isGamepadReaderCreationTried = true;
			self.gamepadReader = GamepadReader::create();
		}
		let readResult = match self.gamepadReader.as_mut() {
			Some(gamepadReader) => {
				gamepadReader.readGamepads()
			}
			None => {
				GamepadReadResult::default()
			}
		};

		// 연결이 풀린 패드. (원본 engine.js 의 gamepaddisconnected 처리 자리, 이름은 지난 사본에서 찾는다)
		for hardwareIndex in readResult.disconnectedGamepadIndexList.iter() {
			let previousGamepad = self.gamepadList.get(*hardwareIndex).and_then(|gamepad| gamepad.clone());
			let gamepad = match previousGamepad {
				Some(previousGamepad) => {
					previousGamepad
				}
				None => {
					Gamepad::create(*hardwareIndex, "")
				}
			};
			self.disconnect(&gamepad);
			if self.isLogEnabled {
				writeLog(&format!("gamepaddisconnected: {}", gamepad.id));
			}
		}

		// 새로 붙은 패드. (원본 engine.js 의 gamepadconnected 처리 자리)
		for hardwareIndex in readResult.connectedGamepadIndexList.iter() {
			if let Some(Some(gamepad)) = readResult.gamepadList.get(*hardwareIndex) {
				self.connect(gamepad);
				if self.isLogEnabled {
					writeLog(&format!("gamepadconnected: {}", gamepad.id));
				}
			}
		}

		self.updateAllGamepadsWithList(readResult.gamepadList);
	}

	//==============================================================================
	// 주어진 목록으로 모든 게임패드 갱신. (원본 updateAllGamepads 의 본문, 목록 번호가 index)
	// - gilrs 없이 상태를 넣을 때 쓴다. (시험, 다른 입력원)
	//==============================================================================
	pub fn updateAllGamepadsWithList(&mut self, gamepadList: Vec<Option<Gamepad>>) {
		// 목록에 있는데 아직 맞이하지 않은 패드를 여기서 붙인다.
		// (원본: gamepadconnected 이벤트를 놓쳐도 매 갱신마다 훑어 보완한다)
		for gamepad in gamepadList.iter().flatten() {
			if self.connectedGamepadIndices.contains(&gamepad.index) {
				continue;
			}
			self.connect(gamepad);
		}

		// 뽑힌 패드를 놓아준다. (disconnect 이벤트도 같은 이유로 놓칠 수 있다)
		for listIndex in (0..self.connectedGamepadIndices.len()).rev() {
			let hardwareIndex = self.connectedGamepadIndices[listIndex];
			let isGamepadPresent = matches!(gamepadList.get(hardwareIndex), Some(Some(_)));
			if isGamepadPresent {
				continue;
			}
			self.connectedGamepadIndices.remove(listIndex);
			self.connectedGamepadStates.remove(&hardwareIndex);
		}

		let isLogEnabled = self.isLogEnabled;
		for hardwareIndex in self.connectedGamepadIndices.iter() {
			let hardwareIndex = *hardwareIndex;
			let gamepad = match gamepadList.get(hardwareIndex) {
				Some(Some(gamepad)) => {
					gamepad
				}
				_ => {
					continue;
				}
			};

			let state = match self.connectedGamepadStates.get_mut(&hardwareIndex) {
				Some(state) => {
					state
				}
				None => {
					continue;
				}
			};

			// 버튼 및 트리거 상태 확인.
			for (buttonIndex, currentButton) in gamepad.buttons.iter().enumerate() {
				let lastButtonState = match state.buttons.get_mut(buttonIndex) {
					Some(lastButtonState) => {
						lastButtonState
					}
					None => {
						continue;
					}
				};
				lastButtonState.previousPressed = lastButtonState.pressed;

				// 눌림 상태 변화 감지.
				if lastButtonState.pressed != currentButton.pressed {
					lastButtonState.pressed = currentButton.pressed;
					let inputText = getButtonInputText(buttonIndex);

					// 로그 출력.
					if isLogEnabled {
						let pressedText = if currentButton.pressed {
							"Pressed"
						}
						else {
							"Released"
						};
						writeLog(&format!("[GamepadManager] Pad {} {} {} {}", hardwareIndex, inputText, buttonIndex, pressedText));
					}

					// 콜백 호출.
					if let Some(inputEventCallback) = self.inputEventCallback.as_mut() {
						let pressedValue = if currentButton.pressed {
							1.0
						}
						else {
							0.0
						};
						inputEventCallback(hardwareIndex, GamepadInputType::button, buttonIndex, pressedValue);
					}
				}

				// 아날로그 값(트리거 등) 변화 감지. (소수점 2자리까지)
				let currentValue = roundToHundredths(currentButton.value as f64);
				if lastButtonState.value != currentValue {
					lastButtonState.value = currentValue;

					// 트리거 등 값이 유의미할 때만 처리.
					if currentValue > 0.0 {
						let inputText = getButtonInputText(buttonIndex);

						// 로그 출력.
						if isLogEnabled {
							writeLog(&format!("[GamepadManager] Pad {} {} {} Value: {}", hardwareIndex, inputText, buttonIndex, currentValue));
						}

						// 콜백 호출.
						if let Some(inputEventCallback) = self.inputEventCallback.as_mut() {
							inputEventCallback(hardwareIndex, GamepadInputType::value, buttonIndex, currentValue as f32);
						}
					}
				}
			}

			// 아날로그 스틱(축) 상태 확인.
			for (axisIndex, axisValue) in gamepad.axes.iter().enumerate() {
				let currentAxisValue = roundToHundredths(*axisValue as f64);
				let lastAxisValue = match state.axes.get_mut(axisIndex) {
					Some(lastAxisValue) => {
						lastAxisValue
					}
					None => {
						continue;
					}
				};

				// 미세한 떨림(Deadzone) 고려하여 0.01 넘는 변화만 감지.
				if (*lastAxisValue - currentAxisValue).abs() > AXIS_CHANGE_THRESHOLD {
					*lastAxisValue = currentAxisValue;

					// 로그 출력.
					if isLogEnabled {
						writeLog(&format!("[GamepadManager] Pad {} Axis {} Value: {}", hardwareIndex, axisIndex, currentAxisValue));
					}

					// 콜백 호출.
					if let Some(inputEventCallback) = self.inputEventCallback.as_mut() {
						inputEventCallback(hardwareIndex, GamepadInputType::axis, axisIndex, currentAxisValue as f32);
					}
				}
			}
		}

		self.gamepadList = gamepadList;
	}

	//==============================================================================
	// 버튼 눌림 상태 반환.
	//==============================================================================
	pub fn isButtonPressed(&self, gamepadIndex: usize, buttonIndex: usize) -> bool {
		match self.findButtonState(gamepadIndex, buttonIndex) {
			Some(buttonState) => {
				buttonState.pressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 이번 갱신에 방금 눌린 버튼인지. (러스트 판에서 더한 것. 키보드 isKeyJustPressed 와 같은 쓰임)
	//==============================================================================
	pub fn isButtonJustPressed(&self, gamepadIndex: usize, buttonIndex: usize) -> bool {
		match self.findButtonState(gamepadIndex, buttonIndex) {
			Some(buttonState) => {
				buttonState.pressed && !buttonState.previousPressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 이번 갱신에 방금 떼어진 버튼인지. (러스트 판에서 더한 것)
	//==============================================================================
	pub fn isButtonJustReleased(&self, gamepadIndex: usize, buttonIndex: usize) -> bool {
		match self.findButtonState(gamepadIndex, buttonIndex) {
			Some(buttonState) => {
				!buttonState.pressed && buttonState.previousPressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 버튼/트리거 아날로그 값 반환. (0.0 ~ 1.0)
	//==============================================================================
	pub fn getButtonValue(&self, gamepadIndex: usize, buttonIndex: usize) -> f32 {
		match self.findButtonState(gamepadIndex, buttonIndex) {
			Some(buttonState) => {
				buttonState.value as f32
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	// 아날로그 스틱 축 값 반환. (-1.0 ~ 1.0)
	//==============================================================================
	pub fn getAxisValue(&self, gamepadIndex: usize, axisIndex: usize) -> f32 {
		let state = match self.connectedGamepadStates.get(&gamepadIndex) {
			Some(state) => {
				state
			}
			None => {
				return 0.0;
			}
		};
		match state.axes.get(axisIndex) {
			Some(axisValue) => {
				zeroIfFalsy(*axisValue) as f32
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	// 연결된 모든 게임패드 반환. (마지막 갱신의 사본)
	//==============================================================================
	pub fn getAllConnectedGamepads(&self) -> Vec<&Gamepad> {
		let mut connectedGamepadList = Vec::new();
		for hardwareIndex in self.connectedGamepadIndices.iter() {
			if let Some(Some(gamepad)) = self.gamepadList.get(*hardwareIndex) {
				connectedGamepadList.push(gamepad);
			}
		}
		connectedGamepadList
	}

	//==============================================================================
	// 연결된 게임패드 수 반환.
	//==============================================================================
	pub fn getConnectedGamepadCount(&self) -> usize {
		self.connectedGamepadIndices.len()
	}

	//==============================================================================
	// 연결된 게임패드 반환. (gamepadIndex 는 연결 목록의 순번, 0 ~ getConnectedGamepadCount() - 1)
	//==============================================================================
	pub fn getConnectedGamepad(&self, gamepadIndex: usize) -> Option<&Gamepad> {
		let hardwareIndex = *self.connectedGamepadIndices.get(gamepadIndex)?;
		match self.gamepadList.get(hardwareIndex) {
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
	fn findButtonState(&self, gamepadIndex: usize, buttonIndex: usize) -> Option<&GamepadButtonState> {
		let state = self.connectedGamepadStates.get(&gamepadIndex)?;
		state.buttons.get(buttonIndex)
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
fn getButtonInputText(buttonIndex: usize) -> &'static str {
	if buttonIndex == GamepadButtonCode::L2 || buttonIndex == GamepadButtonCode::R2 {
		"Trigger"
	}
	else {
		"Button"
	}
}

//==============================================================================
// 소수점 둘째 자리로 반올림. (원본 parseFloat(value.toFixed(2)) 자리)
//==============================================================================
fn roundToHundredths(value: f64) -> f64 {
	(value * 100.0).round() / 100.0
}

//==============================================================================
// 0, -0, NaN 을 0 으로 바꾼다. (원본 `value || 0` 자리)
//==============================================================================
fn zeroIfFalsy(value: f64) -> f64 {
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
fn writeLog(message: &str) {
	#[cfg(target_arch = "wasm32")]
	{
		let messageValue = wasm_bindgen::JsValue::from_str(message);
		web_sys::console::log_1(&messageValue);
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
	fn readButton(&self, button: Button) -> GamepadButton;
	// 축의 값. (gilrs 꼴, Y 는 위가 양수)
	fn readAxis(&self, axis: Axis) -> f32;
}

impl GamepadElementReader for gilrs::Gamepad<'_> {
	//==============================================================================
	// 버튼 읽기. (gilrs 가 모르는 버튼은 뗀 상태)
	//==============================================================================
	fn readButton(&self, button: Button) -> GamepadButton {
		match self.button_data(button) {
			Some(buttonData) => {
				GamepadButton {
					pressed: buttonData.is_pressed(),
					value: buttonData.value(),
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
	fn readAxis(&self, axis: Axis) -> f32 {
		self.value(axis)
	}
}

//==============================================================================
// 십자키 버튼에 겹쳐 읽을 축과 방향. (햇 스위치를 축으로 주는 패드. gilrs DPadX 는 오른쪽, DPadY 는 위가 양수)
//==============================================================================
fn getDirectionalPadAxis(buttonIndex: usize) -> Option<(Axis, f32)> {
	match buttonIndex {
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
fn createStandardGamepad(index: usize, id: &str, elementReader: &impl GamepadElementReader) -> Gamepad {
	let mut gamepad = Gamepad::create(index, id);
	for (buttonIndex, standardButton) in STANDARD_BUTTON_LIST.iter().enumerate() {
		let rawButton = elementReader.readButton(*standardButton);
		let mut isPressed = rawButton.pressed || rawButton.value > BUTTON_PRESSED_THRESHOLD;
		let mut buttonValue = rawButton.value.clamp(0.0, 1.0);
		if let Some((directionalPadAxis, direction)) = getDirectionalPadAxis(buttonIndex) {
			let directionalPadAxisValue = elementReader.readAxis(directionalPadAxis);
			if directionalPadAxisValue * direction > DIRECTIONAL_PAD_AXIS_THRESHOLD {
				isPressed = true;
			}
		}
		if isPressed && buttonValue <= 0.0 {
			buttonValue = 1.0;
		}
		gamepad.buttons[buttonIndex] = GamepadButton {
			pressed: isPressed,
			value: buttonValue,
		};
	}
	for (axisIndex, (standardAxis, direction)) in STANDARD_AXIS_LIST.iter().enumerate() {
		let axisValue = elementReader.readAxis(*standardAxis) * direction;
		gamepad.axes[axisIndex] = zeroIfFalsy(axisValue as f64) as f32;
	}
	gamepad
}

//==============================================================================
// 데스크탑 패드 이름. (크롬 id 꼴. 게임이 "054c" 같은 제조사 번호로 패드 종류를 가른다)
//==============================================================================
fn createDesktopGamepadIdentifier(gamepadName: &str, vendorId: Option<u16>, productId: Option<u16>) -> String {
	match (vendorId, productId) {
		(Some(vendorId), Some(productId)) => {
			format!("{} (STANDARD GAMEPAD Vendor: {:04x} Product: {:04x})", gamepadName, vendorId, productId)
		}
		_ => {
			format!("{} (STANDARD GAMEPAD)", gamepadName)
		}
	}
}

//==============================================================================
// gilrs 패드를 Gamepad 로.
//==============================================================================
fn createGamepadFromGilrs(gamepadId: gilrs::GamepadId, gilrsGamepad: &gilrs::Gamepad) -> Gamepad {
	let index = usize::from(gamepadId);
	let gamepadName = gilrsGamepad.name();
	// 웹은 gilrs 가 브라우저 id 를 이름으로 준다.
	let id = if cfg!(target_arch = "wasm32") {
		gamepadName.to_string()
	}
	else {
		let vendorId = gilrsGamepad.vendor_id();
		let productId = gilrsGamepad.product_id();
		createDesktopGamepadIdentifier(gamepadName, vendorId, productId)
	};
	let mut gamepad = createStandardGamepad(index, &id, gilrsGamepad);
	gamepad.connected = gilrsGamepad.is_connected();
	gamepad
}

//==============================================================================
// gilrs 에서 이번 갱신에 읽은 것.
//==============================================================================
#[derive(Default)]
struct GamepadReadResult {
	// 이번에 붙은 패드 index.
	connectedGamepadIndexList: Vec<usize>,
	// 이번에 떨어진 패드 index.
	disconnectedGamepadIndexList: Vec<usize>,
	// 연결된 패드 목록. (번호가 index, 빈 칸은 None)
	gamepadList: Vec<Option<Gamepad>>,
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
		let gilrsBuilder = GilrsBuilder::new().with_default_filters(false).with_force_feedback(false);
		match gilrsBuilder.build() {
			Ok(gilrs) => {
				Some(GamepadReader { gilrs })
			}
			Err(gilrs::Error::NotImplemented(gilrs)) => {
				writeLog("[GamepadManager] 이 플랫폼은 게임패드를 지원하지 않습니다.");
				Some(GamepadReader { gilrs })
			}
			Err(error) => {
				writeLog(&format!("[GamepadManager] gilrs 를 만들지 못했습니다: {}", error));
				None
			}
		}
	}

	//==============================================================================
	// 읽기. (이벤트를 비워야 gilrs 의 상태가 갱신된다. 웹은 이때 navigator.getGamepads() 를 읽는다)
	//==============================================================================
	fn readGamepads(&mut self) -> GamepadReadResult {
		let mut readResult = GamepadReadResult::default();
		while let Some(event) = self.gilrs.next_event() {
			let hardwareIndex = usize::from(event.id);
			match event.event {
				gilrs::EventType::Connected => {
					readResult.connectedGamepadIndexList.push(hardwareIndex);
				}
				gilrs::EventType::Disconnected => {
					readResult.disconnectedGamepadIndexList.push(hardwareIndex);
				}
				_ => {
				}
			}
		}
		for (gamepadId, gilrsGamepad) in self.gilrs.gamepads() {
			let gamepad = createGamepadFromGilrs(gamepadId, &gilrsGamepad);
			let hardwareIndex = gamepad.index;
			if readResult.gamepadList.len() <= hardwareIndex {
				readResult.gamepadList.resize(hardwareIndex + 1, None);
			}
			readResult.gamepadList[hardwareIndex] = Some(gamepad);
		}
		readResult
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	// 콜백으로 받은 입력 하나. (gamepadIndex, inputType, inputIndex, value)
	type ReceivedInputEvent = (usize, GamepadInputType, usize, f32);

	//==============================================================================
	// 시험용 가짜 패드. (눌린 버튼과 값, 축 값을 직접 준다)
	//==============================================================================
	struct TestElementReader {
		buttonList: Vec<(Button, GamepadButton)>,
		axisList: Vec<(Axis, f32)>,
	}

	impl GamepadElementReader for TestElementReader {
		//==============================================================================
		// 버튼 읽기.
		//==============================================================================
		fn readButton(&self, button: Button) -> GamepadButton {
			for (listedButton, listedState) in self.buttonList.iter() {
				if *listedButton == button {
					return *listedState;
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
		fn readAxis(&self, axis: Axis) -> f32 {
			for (listedAxis, listedValue) in self.axisList.iter() {
				if *listedAxis == axis {
					return *listedValue;
				}
			}
			0.0
		}
	}

	//==============================================================================
	// 가짜 패드에서 버튼 하나만 디지털로 눌러 읽는다.
	//==============================================================================
	fn readWithPressedButton(button: Button) -> Gamepad {
		let elementReader = TestElementReader {
			buttonList: vec![(button, GamepadButton { pressed: true, value: 1.0 })],
			axisList: Vec::new(),
		};
		createStandardGamepad(0, "test", &elementReader)
	}

	//==============================================================================
	// 시험용 관리자. (gilrs 없이, 로그 끔)
	//==============================================================================
	fn createTestManager() -> GamepadManager {
		let mut gamepadManager = GamepadManager::new();
		gamepadManager.setLogEnabled(false);
		gamepadManager
	}

	//==============================================================================
	// gilrs 버튼이 W3C 번호(원본 GamepadButtonCode)로 간다.
	//==============================================================================
	#[test]
	fn buttonIndexMapping() {
		let expectedMappingList = [
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
		assert_eq!(expectedMappingList.len(), STANDARD_BUTTON_COUNT);
		for (button, expectedButtonIndex) in expectedMappingList {
			let gamepad = readWithPressedButton(button);
			assert_eq!(gamepad.buttons.len(), STANDARD_BUTTON_COUNT);
			for (buttonIndex, gamepadButton) in gamepad.buttons.iter().enumerate() {
				let isExpectedPressed = buttonIndex == expectedButtonIndex;
				assert_eq!(gamepadButton.pressed, isExpectedPressed, "{:?} -> {}", button, buttonIndex);
			}
		}
		assert_eq!(GamepadTriggerCode::L2, GamepadButtonCode::L2);
		assert_eq!(GamepadTriggerCode::R2, GamepadButtonCode::R2);
	}

	//==============================================================================
	// 축은 W3C 순서이고 Y 는 아래가 양수다.
	//==============================================================================
	#[test]
	fn axisIndexMapping() {
		let elementReader = TestElementReader {
			buttonList: Vec::new(),
			axisList: vec![(Axis::LeftStickX, -0.25), (Axis::LeftStickY, 0.5), (Axis::RightStickX, 0.75), (Axis::RightStickY, -1.0)],
		};
		let gamepad = createStandardGamepad(3, "test", &elementReader);
		assert_eq!(gamepad.index, 3);
		assert_eq!(gamepad.axes.len(), STANDARD_AXIS_COUNT);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::LEFT_X], -0.25);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::LEFT_Y], -0.5);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::RIGHT_X], 0.75);
		assert_eq!(gamepad.axes[GamepadAnalogStickCode::RIGHT_Y], 1.0);

		// 가만히 있는 Y 축은 -0 이 아니라 0.
		let restingReader = TestElementReader {
			buttonList: Vec::new(),
			axisList: Vec::new(),
		};
		let restingGamepad = createStandardGamepad(0, "test", &restingReader);
		assert!(restingGamepad.axes[GamepadAnalogStickCode::LEFT_Y].is_sign_positive());
	}

	//==============================================================================
	// 트리거는 값이 30/255 를 넘으면 눌림이고 값은 그대로 간다.
	//==============================================================================
	#[test]
	fn triggerPressedThreshold() {
		let elementReader = TestElementReader {
			buttonList: vec![
				(Button::LeftTrigger2, GamepadButton { pressed: false, value: 0.1 }),
				(Button::RightTrigger2, GamepadButton { pressed: false, value: 0.2 }),
			],
			axisList: Vec::new(),
		};
		let gamepad = createStandardGamepad(0, "test", &elementReader);
		let leftTrigger = gamepad.buttons[GamepadButtonCode::L2];
		let rightTrigger = gamepad.buttons[GamepadButtonCode::R2];
		assert!(!leftTrigger.pressed);
		assert_eq!(leftTrigger.value, 0.1);
		assert!(rightTrigger.pressed);
		assert_eq!(rightTrigger.value, 0.2);
	}

	//==============================================================================
	// 축(햇 스위치)으로 오는 십자키는 버튼 12~15 가 된다.
	//==============================================================================
	#[test]
	fn directionalPadFromHatAxis() {
		let elementReader = TestElementReader {
			buttonList: Vec::new(),
			axisList: vec![(Axis::DPadX, -1.0), (Axis::DPadY, 1.0)],
		};
		let gamepad = createStandardGamepad(0, "test", &elementReader);
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
	fn desktopGamepadIdentifier() {
		let playStationIdentifier = createDesktopGamepadIdentifier("Wireless Controller", Some(0x054c), Some(0x09cc));
		assert_eq!(playStationIdentifier, "Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)");
		let unknownIdentifier = createDesktopGamepadIdentifier("Pad", None, None);
		assert_eq!(unknownIdentifier, "Pad (STANDARD GAMEPAD)");
	}

	//==============================================================================
	// 축은 소수점 둘째 자리로 반올림하고 0.01 넘게 바뀔 때만 받는다. (원본 데드존)
	//==============================================================================
	#[test]
	fn axisDeadZone() {
		let mut gamepadManager = createTestManager();
		let mut gamepad = Gamepad::create(0, "test");

		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.004;
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert_eq!(gamepadManager.getAxisValue(0, GamepadAnalogStickCode::LEFT_X), 0.0);

		// 반올림하면 0.01, 바뀐 폭이 0.01 을 넘지 않아 그대로.
		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.011;
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert_eq!(gamepadManager.getAxisValue(0, GamepadAnalogStickCode::LEFT_X), 0.0);

		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.456;
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert_eq!(gamepadManager.getAxisValue(0, GamepadAnalogStickCode::LEFT_X), 0.46);

		// 반올림하면 같은 0.46.
		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = 0.462;
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert_eq!(gamepadManager.getAxisValue(0, GamepadAnalogStickCode::LEFT_X), 0.46);

		gamepad.axes[GamepadAnalogStickCode::LEFT_Y] = -1.0;
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert_eq!(gamepadManager.getAxisValue(0, GamepadAnalogStickCode::LEFT_Y), -1.0);

		// 없는 패드, 없는 축은 0.
		assert_eq!(gamepadManager.getAxisValue(5, GamepadAnalogStickCode::LEFT_X), 0.0);
		assert_eq!(gamepadManager.getAxisValue(0, 9), 0.0);
	}

	//==============================================================================
	// 방금 눌림 / 방금 뗌은 갱신 하나 동안만 켜진다.
	//==============================================================================
	#[test]
	fn buttonEdgesAcrossTicks() {
		let mut gamepadManager = createTestManager();
		let mut gamepad = Gamepad::create(0, "test");
		let buttonIndex = GamepadButtonCode::A_CROSS;

		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert!(!gamepadManager.isButtonPressed(0, buttonIndex));
		assert!(!gamepadManager.isButtonJustPressed(0, buttonIndex));

		gamepad.buttons[buttonIndex] = GamepadButton { pressed: true, value: 1.0 };
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert!(gamepadManager.isButtonPressed(0, buttonIndex));
		assert!(gamepadManager.isButtonJustPressed(0, buttonIndex));
		assert!(!gamepadManager.isButtonJustReleased(0, buttonIndex));

		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert!(gamepadManager.isButtonPressed(0, buttonIndex));
		assert!(!gamepadManager.isButtonJustPressed(0, buttonIndex));

		gamepad.buttons[buttonIndex] = GamepadButton { pressed: false, value: 0.0 };
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert!(!gamepadManager.isButtonPressed(0, buttonIndex));
		assert!(gamepadManager.isButtonJustReleased(0, buttonIndex));

		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone())]);
		assert!(!gamepadManager.isButtonJustReleased(0, buttonIndex));

		// 붙는 갱신에 이미 눌려 있으면 그 갱신에 방금 눌림.
		let mut otherGamepad = Gamepad::create(1, "other");
		otherGamepad.buttons[buttonIndex] = GamepadButton { pressed: true, value: 1.0 };
		gamepadManager.updateAllGamepadsWithList(vec![Some(gamepad.clone()), Some(otherGamepad)]);
		assert!(gamepadManager.isButtonJustPressed(1, buttonIndex));
	}

	//==============================================================================
	// 콜백: 눌림 변화는 button, 0 보다 큰 값 변화만 value, 축은 axis.
	//==============================================================================
	#[test]
	fn inputCallback() {
		let mut gamepadManager = createTestManager();
		let receivedEventList: Rc<RefCell<Vec<ReceivedInputEvent>>> = Rc::new(RefCell::new(Vec::new()));
		let receivedEventListForCallback = receivedEventList.clone();
		gamepadManager.setCallback(Some(Box::new(move |gamepadIndex, inputType, inputIndex, value| {
			receivedEventListForCallback.borrow_mut().push((gamepadIndex, inputType, inputIndex, value));
		})));

		let mut gamepad = Gamepad::create(2, "test");
		let mut gamepadList = vec![None, None, Some(gamepad.clone())];
		gamepadManager.updateAllGamepadsWithList(gamepadList.clone());
		assert!(receivedEventList.borrow().is_empty());

		gamepad.buttons[GamepadButtonCode::R2] = GamepadButton { pressed: true, value: 0.5 };
		gamepad.axes[GamepadAnalogStickCode::RIGHT_X] = 0.3;
		gamepadList[2] = Some(gamepad.clone());
		gamepadManager.updateAllGamepadsWithList(gamepadList.clone());
		let expectedPressEventList = vec![
			(2, GamepadInputType::button, GamepadButtonCode::R2, 1.0),
			(2, GamepadInputType::value, GamepadButtonCode::R2, 0.5),
			(2, GamepadInputType::axis, GamepadAnalogStickCode::RIGHT_X, 0.3),
		];
		assert_eq!(*receivedEventList.borrow(), expectedPressEventList);
		assert_eq!(gamepadManager.getButtonValue(2, GamepadButtonCode::R2), 0.5);
		receivedEventList.borrow_mut().clear();

		// 값이 0 으로 내려가면 value 는 알리지 않지만 상태는 0.
		gamepad.buttons[GamepadButtonCode::R2] = GamepadButton { pressed: false, value: 0.0 };
		gamepadList[2] = Some(gamepad.clone());
		gamepadManager.updateAllGamepadsWithList(gamepadList.clone());
		let expectedReleaseEventList = vec![(2, GamepadInputType::button, GamepadButtonCode::R2, 0.0)];
		assert_eq!(*receivedEventList.borrow(), expectedReleaseEventList);
		assert_eq!(gamepadManager.getButtonValue(2, GamepadButtonCode::R2), 0.0);
	}

	//==============================================================================
	// 목록에 나타나면 붙고, 사라지면 놓는다. (원본 updateAllGamepads 의 보완 훑기)
	//==============================================================================
	#[test]
	fn connectAndDisconnectByList() {
		let mut gamepadManager = createTestManager();
		let gamepad = Gamepad::create(1, "second slot");
		gamepadManager.updateAllGamepadsWithList(vec![None, Some(gamepad.clone())]);
		assert_eq!(gamepadManager.getConnectedGamepadCount(), 1);
		let connectedGamepad = gamepadManager.getConnectedGamepad(0);
		assert_eq!(connectedGamepad.map(|gamepad| gamepad.index), Some(1));
		assert!(gamepadManager.getConnectedGamepad(1).is_none());
		let allConnectedGamepads = gamepadManager.getAllConnectedGamepads();
		assert_eq!(allConnectedGamepads.len(), 1);
		assert_eq!(allConnectedGamepads[0].id, "second slot");

		gamepadManager.updateAllGamepadsWithList(Vec::new());
		assert_eq!(gamepadManager.getConnectedGamepadCount(), 0);
		assert!(gamepadManager.getConnectedGamepad(0).is_none());
		assert!(gamepadManager.getAllConnectedGamepads().is_empty());
		assert!(!gamepadManager.isButtonPressed(1, GamepadButtonCode::A_CROSS));
	}

	//==============================================================================
	// connect 는 같은 패드를 두 번 붙이지 않고, disconnect 는 붙은 패드만 뗀다.
	//==============================================================================
	#[test]
	fn connectIgnoresDuplicate() {
		let mut gamepadManager = createTestManager();
		let gamepad = Gamepad::create(0, "test");
		gamepadManager.connect(&gamepad);
		gamepadManager.connect(&gamepad);
		assert_eq!(gamepadManager.getConnectedGamepadCount(), 1);
		let otherGamepad = Gamepad::create(4, "other");
		gamepadManager.disconnect(&otherGamepad);
		assert_eq!(gamepadManager.getConnectedGamepadCount(), 1);
		gamepadManager.disconnect(&gamepad);
		assert_eq!(gamepadManager.getConnectedGamepadCount(), 0);
	}

	//==============================================================================
	// 반올림은 원본 toFixed(2) 와 같은 값을 낸다.
	//==============================================================================
	#[test]
	fn roundToHundredthsMatchesToFixed() {
		assert_eq!(roundToHundredths(0.123), 0.12);
		assert_eq!(roundToHundredths(0.125), 0.13);
		assert_eq!(roundToHundredths(-0.456), -0.46);
		assert_eq!(roundToHundredths(1.0), 1.0);
		assert_eq!(roundToHundredths(0.29f32 as f64), 0.29);
	}
}
