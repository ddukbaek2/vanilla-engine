//==============================================================================
// 입력 관리자.
// - 키 이름은 원본과 같이 KeyboardEvent.code 꼴이다. ("KeyA", "ArrowLeft", "Space" ...)
//   winit 의 KeyCode 이름이 같은 W3C 규약을 따르므로 그대로 문자열로 바꿔 쓴다.
// - 마우스 왼쪽 버튼을 터치로 다룬다. (원본은 마우스와 터치를 하나로 모은다)
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
	pub const a: &'static str = "KeyA";
	pub const b: &'static str = "KeyB";
	pub const c: &'static str = "KeyC";
	pub const d: &'static str = "KeyD";
	pub const e: &'static str = "KeyE";
	pub const f: &'static str = "KeyF";
	pub const g: &'static str = "KeyG";
	pub const h: &'static str = "KeyH";
	pub const i: &'static str = "KeyI";
	pub const j: &'static str = "KeyJ";
	pub const k: &'static str = "KeyK";
	pub const l: &'static str = "KeyL";
	pub const m: &'static str = "KeyM";
	pub const n: &'static str = "KeyN";
	pub const o: &'static str = "KeyO";
	pub const p: &'static str = "KeyP";
	pub const q: &'static str = "KeyQ";
	pub const r: &'static str = "KeyR";
	pub const s: &'static str = "KeyS";
	pub const t: &'static str = "KeyT";
	pub const u: &'static str = "KeyU";
	pub const v: &'static str = "KeyV";
	pub const w: &'static str = "KeyW";
	pub const x: &'static str = "KeyX";
	pub const y: &'static str = "KeyY";
	pub const z: &'static str = "KeyZ";
	pub const digit0: &'static str = "Digit0";
	pub const digit1: &'static str = "Digit1";
	pub const digit2: &'static str = "Digit2";
	pub const digit3: &'static str = "Digit3";
	pub const digit4: &'static str = "Digit4";
	pub const digit5: &'static str = "Digit5";
	pub const digit6: &'static str = "Digit6";
	pub const digit7: &'static str = "Digit7";
	pub const digit8: &'static str = "Digit8";
	pub const digit9: &'static str = "Digit9";
	pub const backspace: &'static str = "Backspace";
	pub const tab: &'static str = "Tab";
	pub const enter: &'static str = "Enter";
	pub const shift: &'static str = "ShiftLeft";
	pub const ctrl: &'static str = "ControlLeft";
	pub const alt: &'static str = "AltLeft";
	pub const capslock: &'static str = "CapsLock";
	pub const escape: &'static str = "Escape";
	pub const space: &'static str = "Space";
	pub const pageup: &'static str = "PageUp";
	pub const pagedown: &'static str = "PageDown";
	pub const end: &'static str = "End";
	pub const home: &'static str = "Home";
	pub const left: &'static str = "ArrowLeft";
	pub const up: &'static str = "ArrowUp";
	pub const right: &'static str = "ArrowRight";
	pub const down: &'static str = "ArrowDown";
	pub const insert: &'static str = "Insert";
	pub const delete: &'static str = "Delete";
	pub const f1: &'static str = "F1";
	pub const f2: &'static str = "F2";
	pub const f3: &'static str = "F3";
	pub const f4: &'static str = "F4";
	pub const f5: &'static str = "F5";
	pub const f6: &'static str = "F6";
	pub const f7: &'static str = "F7";
	pub const f8: &'static str = "F8";
	pub const f9: &'static str = "F9";
	pub const f10: &'static str = "F10";
	pub const f11: &'static str = "F11";
	pub const f12: &'static str = "F12";
}

pub struct InputManager {
	pressedKeys: HashSet<String>,
	previousPressedKeys: HashSet<String>,
	isTouchPressed: bool,
	isTouchReleased: bool,
	isTouchCancelled: bool,
	isTouchMoved: bool,
	isTouchDown: bool,
	canvasNativeInputPosition: Vector2,
	viewInputPosition: Vector2,
	wheelDeltaX: f32,
	wheelDeltaY: f32,
	gamepadManager: GamepadManager,
	textInput: TextInput,
}

impl InputManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> InputManager {
		InputManager {
			pressedKeys: HashSet::new(),
			previousPressedKeys: HashSet::new(),
			isTouchPressed: false,
			isTouchReleased: false,
			isTouchCancelled: false,
			isTouchMoved: false,
			isTouchDown: false,
			canvasNativeInputPosition: Vector2::zero(),
			viewInputPosition: Vector2::zero(),
			wheelDeltaX: 0.0,
			wheelDeltaY: 0.0,
			gamepadManager: GamepadManager::new(),
			textInput: TextInput::new(),
		}
	}

	//==============================================================================
	// 주기적 갱신. (게임패드 폴링)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		let gamepadManager = self.getGamepadManagerMut();
		gamepadManager.tick(timeDelta);
	}

	//==============================================================================
	// 모든 입력 상태 비우기. (창이 포커스를 잃었을 때)
	//==============================================================================
	pub fn clear(&mut self) {
		self.pressedKeys.clear();
		self.isTouchDown = false;
		self.isTouchPressed = false;
		self.isTouchReleased = false;
		self.isTouchCancelled = false;
		self.isTouchMoved = false;
		self.clearWheelDelta();
	}

	//==============================================================================
	// 휠 이동량 누적.
	//==============================================================================
	pub fn addWheelDelta(&mut self, deltaX: f32, deltaY: f32) {
		self.wheelDeltaX += deltaX;
		self.wheelDeltaY += deltaY;
	}

	pub fn getWheelDeltaX(&self) -> f32 {
		self.wheelDeltaX
	}

	pub fn getWheelDeltaY(&self) -> f32 {
		self.wheelDeltaY
	}

	pub fn hasWheelDelta(&self) -> bool {
		self.wheelDeltaX != 0.0 || self.wheelDeltaY != 0.0
	}

	pub fn clearWheelDelta(&mut self) {
		self.wheelDeltaX = 0.0;
		self.wheelDeltaY = 0.0;
	}

	//==============================================================================
	// 키 눌림 기록.
	//==============================================================================
	pub fn pushKey(&mut self, key: &str) {
		self.pressedKeys.insert(key.to_string());
	}

	//==============================================================================
	// 키 뗌 기록.
	//==============================================================================
	pub fn popKey(&mut self, key: &str) {
		self.pressedKeys.remove(key);
	}

	//==============================================================================
	// 키가 눌려 있는지.
	//==============================================================================
	pub fn isKeyPressed(&self, key: &str) -> bool {
		self.pressedKeys.contains(key)
	}

	//==============================================================================
	// 이번 프레임에 눌렸는지.
	//==============================================================================
	pub fn isKeyJustPressed(&self, key: &str) -> bool {
		self.pressedKeys.contains(key) && !self.previousPressedKeys.contains(key)
	}

	//==============================================================================
	// 이번 프레임에 떼졌는지.
	//==============================================================================
	pub fn isKeyJustReleased(&self, key: &str) -> bool {
		!self.pressedKeys.contains(key) && self.previousPressedKeys.contains(key)
	}

	//==============================================================================
	// 프레임 끝 스냅샷. (다음 프레임의 Just 판정 기준)
	//==============================================================================
	pub fn updateFrameSnapshot(&mut self) {
		self.previousPressedKeys = self.pressedKeys.clone();
	}

	//==============================================================================
	// 터치 상태. (Pressed / Released / Cancelled / Moved 는 그 프레임에만 켜진다)
	//==============================================================================
	pub fn setTouchPressed(&mut self, value: bool) {
		self.isTouchPressed = value;
	}

	pub fn setTouchReleased(&mut self, value: bool) {
		self.isTouchReleased = value;
	}

	pub fn setTouchCancelled(&mut self, value: bool) {
		self.isTouchCancelled = value;
	}

	pub fn setTouchMoved(&mut self, value: bool) {
		self.isTouchMoved = value;
	}

	pub fn isTouchPressed(&self) -> bool {
		self.isTouchPressed
	}

	pub fn isTouchReleased(&self) -> bool {
		self.isTouchReleased
	}

	pub fn isTouchCancelled(&self) -> bool {
		self.isTouchCancelled
	}

	pub fn isTouchMoved(&self) -> bool {
		self.isTouchMoved
	}

	//==============================================================================
	// 터치가 눌린 채인지. (러스트 판에서 더한 것. 눌린 동안 계속 켜져 있다)
	//==============================================================================
	pub fn setTouchDown(&mut self, value: bool) {
		self.isTouchDown = value;
	}

	pub fn isTouchDown(&self) -> bool {
		self.isTouchDown
	}

	//==============================================================================
	// 입력 위치. (창 안쪽 논리 좌표 / 뷰 좌표)
	//==============================================================================
	pub fn setCanvasNativeInputPosition(&mut self, position: Vector2) {
		self.canvasNativeInputPosition = position;
	}

	pub fn getCanvasNativeInputPosition(&self) -> Vector2 {
		self.canvasNativeInputPosition
	}

	pub fn setViewInputPosition(&mut self, position: Vector2) {
		self.viewInputPosition = position;
	}

	pub fn getViewInputPosition(&self) -> Vector2 {
		self.viewInputPosition
	}

	//==============================================================================
	// 모든 게임패드 갱신.
	//==============================================================================
	pub fn updateAllGamepads(&mut self) {
		let gamepadManager = self.getGamepadManagerMut();
		gamepadManager.updateAllGamepads();
	}

	//==============================================================================
	// 게임패드 연결.
	//==============================================================================
	pub fn connectGamepad(&mut self, gamepad: &Gamepad) {
		let gamepadManager = self.getGamepadManagerMut();
		gamepadManager.connect(gamepad);
	}

	//==============================================================================
	// 게임패드 연결 해제.
	//==============================================================================
	pub fn disconnectGamepad(&mut self, gamepad: &Gamepad) {
		let gamepadManager = self.getGamepadManagerMut();
		gamepadManager.disconnect(gamepad);
	}

	//==============================================================================
	// 게임패드 매니저 반환.
	//==============================================================================
	pub fn getGamepadManager(&self) -> &GamepadManager {
		&self.gamepadManager
	}

	//==============================================================================
	// 게임패드 매니저 반환. (바꿀 때: setCallback, connect 들)
	//==============================================================================
	pub fn getGamepadManagerMut(&mut self) -> &mut GamepadManager {
		&mut self.gamepadManager
	}

	//==============================================================================
	// 글 입력 줄 반환. (입력 필드가 꺼내 쓰고 상태를 올린다. &self 로 된다)
	//==============================================================================
	pub fn getTextInput(&self) -> &TextInput {
		&self.textInput
	}

	//==============================================================================
	// 글 입력 줄 반환. (플랫폼이 이벤트를 넣을 때)
	//==============================================================================
	pub fn getTextInputMut(&mut self) -> &mut TextInput {
		&mut self.textInput
	}
}

impl Default for InputManager {
	fn default() -> InputManager {
		InputManager::new()
	}
}
