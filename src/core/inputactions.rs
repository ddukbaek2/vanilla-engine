//==============================================================================
// 입력 동작.
// - 원본 core/inputactions.js 를 옮겼다. 동작 이름("confirm", "left" 같은 게임이 정한 글)에 키, 게임패드 단추, 게임패드 축의 한쪽을 묶는다.
//   한 동작에 여럿을 묶을 수 있고, 하나라도 눌렸으면 눌린 것이다.
// - 엔진의 InputManager 가 하나 들고(get_input_actions) 프레임 첫머리의 tick 에서 update 를 부른다. 게임은 읽기만 한다.
// - 키는 프레임 사이에 눌렀다 뗀 것도 그 프레임에 한 번 눌린 것으로 본다. (InputManager.is_key_just_pressed)
// - set_repeat 를 건 동작은 누르고 있으면 기다린 뒤부터 간격마다 is_just_pressed 가 다시 참이 된다. (메뉴 고르기)
// - 축은 죽은 구역(set_dead_zone)을 빼고 나머지를 0 ~ 1 로 다시 편다. 스틱은 길이로, 묶은 축은 한쪽 값으로 거른다.
// - 바깥 입력원(add_source)은 엔진이 모르는 입력(플랫폼 SDK 의 패드 같은 것)을 게임이 끼우는 자리다. InputActionSource 트레이트를 단다.
// - 글자 입력칸에 쓰는 동안은 키를 듣지 않는다. (set_keys_ignored_while_typing 으로 끈다)
// - 원본과 다른 점: 원본은 읽을 때 입력 매니저를 다시 보지만, 러스트 판은 입력 매니저를 들고 있을 수 없어
//   update 때 이동 키, 패드 축, 바깥 입력원의 이동 축을 적어 두고 read_gamepad_stick, read_move_axis 가 그 값을 읽는다. (같은 프레임의 값이다)
//   바깥 입력원은 JS 객체 대신 트레이트이고, 빼기는 add_source 가 준 번호로 한다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::gamepadmanager::{GamepadAnalogStickCode, GamepadKind};
use crate::core::inputmanager::{InputDevice, InputManager};
use std::collections::HashMap;

// 스틱과 축의 가운데 떨림을 거르는 길이의 처음 값. (이보다 덜 기울면 0)
const DEFAULT_DEAD_ZONE: f32 = 0.2;
// 축에 묶은 동작을 눌린 것으로 보는 기울기의 처음 값. (죽은 구역을 빼기 전 값)
const DEFAULT_AXIS_PRESS_THRESHOLD: f32 = 0.5;
// 바깥 입력원의 동작 값을 눌린 것으로 보는 값.
const SOURCE_PRESS_THRESHOLD: f32 = 0.5;
// 이동 축의 키. (KeyboardEvent.code, 자판 배열과 상관없이 같은 자리)
const MOVE_LEFT_KEYS: [&str; 2] = ["KeyA", "ArrowLeft"];
const MOVE_RIGHT_KEYS: [&str; 2] = ["KeyD", "ArrowRight"];
const MOVE_UP_KEYS: [&str; 2] = ["KeyW", "ArrowUp"];
const MOVE_DOWN_KEYS: [&str; 2] = ["KeyS", "ArrowDown"];

//==============================================================================
// 바깥 입력원. (게임이 플랫폼 SDK 의 패드 같은 것을 끼울 때 단다)
//==============================================================================
pub trait InputActionSource {
	// 동작의 값. (0 ~ 1, 0.5 이상이면 눌림, 모르는 동작이면 0)
	fn read_action_value(&mut self, action: &str) -> f32;

	// 이동 축. (오른쪽 +x 아래 +y, 길이 1 이하로 자른다, 없으면 None)
	fn read_move_axis(&mut self) -> Option<Vector2> {
		None
	}

	// 이 입력원이 입력을 넣은 프레임에 마지막 장치로 적을 패드 종류. (None 이면 적지 않는다)
	fn read_gamepad_kind(&mut self) -> Option<GamepadKind> {
		None
	}
}

//==============================================================================
// 게임패드 축의 한쪽.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
struct AxisBinding {
	axis_index: usize,
	direction: f32,
}

//==============================================================================
// 누르고 있을 때의 반복.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
struct ActionRepeat {
	delay_seconds: f32,
	interval_seconds: f32,
}

//==============================================================================
// 동작 하나의 이번 프레임 상태.
//==============================================================================
#[derive(Clone, Copy, Default, Debug)]
struct ActionState {
	is_pressed: bool,
	was_pressed: bool,
	value: f32,
	repeat_seconds: f32,
	is_repeat_fired: bool,
}

//==============================================================================
// 입력 동작.
//==============================================================================
pub struct InputActions {
	key_bindings: HashMap<String, Vec<String>>,
	gamepad_button_bindings: HashMap<String, Vec<usize>>,
	axis_bindings: HashMap<String, Vec<AxisBinding>>,
	repeats: HashMap<String, ActionRepeat>,
	states: HashMap<String, ActionState>,
	sources: Vec<(u64, Box<dyn InputActionSource>)>,
	next_source_id: u64,
	dead_zone: f32,
	axis_press_threshold: f32,
	is_keys_ignored_while_typing: bool,
	// 이번 프레임의 이동 키 방향. (길이 1 또는 0)
	key_move_axis: Vector2,
	// 이번 프레임의 연결된 패드마다의 축 값. (GamepadAnalogStickCode 차례)
	gamepad_axis_list: Vec<Vec<f32>>,
	// 이번 프레임에 바깥 입력원이 준 이동 축 가운데 가장 긴 것.
	source_move_axis: Vector2,
}

impl InputActions {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> InputActions {
		InputActions {
			key_bindings: HashMap::new(),
			gamepad_button_bindings: HashMap::new(),
			axis_bindings: HashMap::new(),
			repeats: HashMap::new(),
			states: HashMap::new(),
			sources: Vec::new(),
			next_source_id: 1,
			dead_zone: DEFAULT_DEAD_ZONE,
			axis_press_threshold: DEFAULT_AXIS_PRESS_THRESHOLD,
			is_keys_ignored_while_typing: true,
			key_move_axis: Vector2::zero(),
			gamepad_axis_list: Vec::new(),
			source_move_axis: Vector2::zero(),
		}
	}

	//==============================================================================
	// 키 묶기. (KeyboardEvent.code, KeyCode 의 값)
	//==============================================================================
	pub fn bind_key(&mut self, action: &str, key: &str) {
		self.states.entry(action.to_string()).or_default();
		let keys = self.key_bindings.entry(action.to_string()).or_default();
		if !keys.iter().any(|bound_key| bound_key == key) {
			keys.push(key.to_string());
		}
	}

	//==============================================================================
	// 게임패드 단추 묶기. (GamepadButtonCode, 연결된 패드 모두)
	//==============================================================================
	pub fn bind_gamepad_button(&mut self, action: &str, button_code: usize) {
		self.states.entry(action.to_string()).or_default();
		let button_codes = self.gamepad_button_bindings.entry(action.to_string()).or_default();
		if !button_codes.contains(&button_code) {
			button_codes.push(button_code);
		}
	}

	//==============================================================================
	// 게임패드 축의 한쪽 묶기. (GamepadAnalogStickCode, direction 이 양수면 + 쪽, 음수면 - 쪽, 축 Y 는 아래가 +)
	// - 그쪽으로 set_axis_press_threshold 만큼 기울면 눌린 것이고, 값(read_value)은 죽은 구역을 뺀 기울기다.
	//==============================================================================
	pub fn bind_axis(&mut self, action: &str, axis_index: usize, direction: f32) {
		self.states.entry(action.to_string()).or_default();
		let axis_direction = if direction < 0.0 {
			-1.0
		}
		else {
			1.0
		};
		let axis_binding = AxisBinding {
			axis_index,
			direction: axis_direction,
		};
		let axis_binding_list = self.axis_bindings.entry(action.to_string()).or_default();
		if !axis_binding_list.contains(&axis_binding) {
			axis_binding_list.push(axis_binding);
		}
	}

	//==============================================================================
	// 동작에 묶은 것 모두 풀기. (키 바꾸기 화면에서 다시 묶기 전에, 반복 설정은 남는다)
	//==============================================================================
	pub fn unbind_action(&mut self, action: &str) {
		self.key_bindings.remove(action);
		self.gamepad_button_bindings.remove(action);
		self.axis_bindings.remove(action);
	}

	//==============================================================================
	// 동작에 묶은 키 목록. (단추 안내를 그릴 때)
	//==============================================================================
	pub fn get_bound_keys(&self, action: &str) -> &[String] {
		match self.key_bindings.get(action) {
			Some(keys) => {
				keys
			}
			None => {
				&[]
			}
		}
	}

	//==============================================================================
	// 누르고 있으면 반복 설정. (기다리는 초 뒤부터 간격 초마다 is_just_pressed 가 참, 간격이 0 이하면 반복을 끈다)
	//==============================================================================
	pub fn set_repeat(&mut self, action: &str, delay_seconds: f32, interval_seconds: f32) {
		self.states.entry(action.to_string()).or_default();
		if interval_seconds <= 0.0 {
			self.repeats.remove(action);
			return;
		}
		self.repeats.insert(action.to_string(), ActionRepeat {
			delay_seconds: delay_seconds.max(0.0),
			interval_seconds,
		});
	}

	//==============================================================================
	// 죽은 구역 설정. (0 ~ 1 미만, 처음은 0.2)
	//==============================================================================
	pub fn set_dead_zone(&mut self, dead_zone: f32) {
		self.dead_zone = dead_zone.clamp(0.0, 0.99);
	}

	//==============================================================================
	// 죽은 구역.
	//==============================================================================
	pub fn get_dead_zone(&self) -> f32 {
		self.dead_zone
	}

	//==============================================================================
	// 축에 묶은 동작을 눌린 것으로 보는 기울기 설정. (0 ~ 1, 처음은 0.5)
	//==============================================================================
	pub fn set_axis_press_threshold(&mut self, axis_press_threshold: f32) {
		self.axis_press_threshold = axis_press_threshold.clamp(0.0, 1.0);
	}

	//==============================================================================
	// 축에 묶은 동작을 눌린 것으로 보는 기울기.
	//==============================================================================
	pub fn get_axis_press_threshold(&self) -> f32 {
		self.axis_press_threshold
	}

	//==============================================================================
	// 글자 입력칸에 쓰는 동안 키를 듣지 않을지 설정. (처음은 참)
	//==============================================================================
	pub fn set_keys_ignored_while_typing(&mut self, is_ignored: bool) {
		self.is_keys_ignored_while_typing = is_ignored;
	}

	//==============================================================================
	// 글자 입력칸에 쓰는 동안 키를 듣지 않는지.
	//==============================================================================
	pub fn is_keys_ignored_while_typing(&self) -> bool {
		self.is_keys_ignored_while_typing
	}

	//==============================================================================
	// 바깥 입력원 더하기. (돌려주는 번호로 remove_source 한다)
	//==============================================================================
	pub fn add_source(&mut self, source: Box<dyn InputActionSource>) -> u64 {
		let source_id = self.next_source_id;
		self.next_source_id += 1;
		self.sources.push((source_id, source));
		source_id
	}

	//==============================================================================
	// 바깥 입력원 빼기. (있었으면 true)
	//==============================================================================
	pub fn remove_source(&mut self, source_id: u64) -> bool {
		let source_count = self.sources.len();
		self.sources.retain(|(listed_source_id, _)| *listed_source_id != source_id);
		self.sources.len() != source_count
	}

	//==============================================================================
	// 프레임마다 갱신. (InputManager.tick 이 부른다)
	//==============================================================================
	pub fn update(&mut self, input_manager: &mut InputManager, time_delta: f32) {
		let is_typing = self.is_keys_ignored_while_typing && input_manager.is_typing_in_text_field();
		let mut source_gamepad_kind: Option<GamepadKind> = None;

		// 연결된 패드의 축. (read_gamepad_stick 이 읽는다)
		let gamepad_manager = input_manager.get_gamepad_manager();
		let gamepad_index_list: Vec<usize> = gamepad_manager.get_all_connected_gamepads().iter().map(|gamepad| gamepad.index).collect();
		let mut gamepad_axis_list = Vec::new();
		for gamepad_index in gamepad_index_list.iter() {
			let axis_value_list: Vec<f32> = (0..4).map(|axis_index| gamepad_manager.get_axis_value(*gamepad_index, axis_index)).collect();
			gamepad_axis_list.push(axis_value_list);
		}
		self.gamepad_axis_list = gamepad_axis_list;

		let action_list: Vec<String> = self.states.keys().cloned().collect();
		for action in action_list.iter() {
			let mut is_pressed = false;
			let mut value: f32 = 0.0;

			// 키. (프레임 사이에 눌렀다 뗀 것도 이번 프레임에 눌린 것으로)
			if !is_typing {
				if let Some(keys) = self.key_bindings.get(action) {
					for key in keys.iter() {
						if input_manager.is_key_pressed(key) || input_manager.is_key_just_pressed(key) {
							is_pressed = true;
							value = 1.0;
							break;
						}
					}
				}
			}

			// 게임패드 단추와 축. (연결된 패드 모두)
			let gamepad_manager = input_manager.get_gamepad_manager();
			for gamepad_index in gamepad_index_list.iter() {
				if let Some(button_codes) = self.gamepad_button_bindings.get(action) {
					for button_code in button_codes.iter() {
						if !gamepad_manager.is_button_pressed(*gamepad_index, *button_code) {
							continue;
						}
						is_pressed = true;
						let button_value = gamepad_manager.get_button_value(*gamepad_index, *button_code);
						let pressed_value = if button_value > 0.0 {
							button_value
						}
						else {
							1.0
						};
						value = value.max(pressed_value);
					}
				}
				if let Some(axis_binding_list) = self.axis_bindings.get(action) {
					for axis_binding in axis_binding_list.iter() {
						let axis_value = gamepad_manager.get_axis_value(*gamepad_index, axis_binding.axis_index);
						let directed_value = axis_value * axis_binding.direction;
						if directed_value >= self.axis_press_threshold && directed_value > 0.0 {
							is_pressed = true;
						}
						value = value.max(self.apply_dead_zone(directed_value));
					}
				}
			}

			// 바깥 입력원.
			for (_, source) in self.sources.iter_mut() {
				let source_value = source.read_action_value(action);
				let clamped_value = if source_value.is_finite() {
					source_value.clamp(0.0, 1.0)
				}
				else {
					0.0
				};
				if clamped_value <= 0.0 {
					continue;
				}
				value = value.max(clamped_value);
				if clamped_value >= SOURCE_PRESS_THRESHOLD {
					is_pressed = true;
					if let Some(gamepad_kind) = source.read_gamepad_kind() {
						source_gamepad_kind = Some(gamepad_kind);
					}
				}
			}

			// 상태와 반복.
			let repeat = self.repeats.get(action).copied();
			let state = self.states.entry(action.clone()).or_default();
			state.was_pressed = state.is_pressed;
			state.is_pressed = is_pressed;
			state.value = value;
			state.is_repeat_fired = false;
			if is_pressed && !state.was_pressed {
				state.repeat_seconds = match repeat {
					Some(repeat) => {
						repeat.delay_seconds
					}
					None => {
						0.0
					}
				};
			}
			else if is_pressed {
				if let Some(repeat) = repeat {
					state.repeat_seconds -= time_delta;
					if state.repeat_seconds <= 0.0 {
						state.is_repeat_fired = true;
						state.repeat_seconds += repeat.interval_seconds;
						// 프레임이 길게 끊겼으면 몰아서 반복하지 않고 한 간격을 다시 기다린다.
						if state.repeat_seconds <= 0.0 {
							state.repeat_seconds = repeat.interval_seconds;
						}
					}
				}
			}
		}

		// 이동 키. (대각선도 길이 1)
		let mut key_move_axis = Vector2::zero();
		if !is_typing {
			let is_any_key_pressed = |keys: &[&str]| -> bool {
				keys.iter().any(|key| input_manager.is_key_pressed(key))
			};
			let direction_x = (is_any_key_pressed(&MOVE_RIGHT_KEYS) as i32 - is_any_key_pressed(&MOVE_LEFT_KEYS) as i32) as f32;
			let direction_y = (is_any_key_pressed(&MOVE_DOWN_KEYS) as i32 - is_any_key_pressed(&MOVE_UP_KEYS) as i32) as f32;
			let key_length = direction_x.hypot(direction_y);
			if key_length > 0.0 {
				key_move_axis = Vector2::create(direction_x / key_length, direction_y / key_length);
			}
		}
		self.key_move_axis = key_move_axis;

		// 바깥 입력원의 이동 축.
		let mut source_move_axis = Vector2::zero();
		let dead_zone = self.dead_zone;
		for (_, source) in self.sources.iter_mut() {
			let move_axis = match source.read_move_axis() {
				Some(move_axis) => {
					move_axis
				}
				None => {
					continue;
				}
			};
			let clamped_move_axis = clamp_length(move_axis.x, move_axis.y);
			let clamped_length = clamped_move_axis.x.hypot(clamped_move_axis.y);
			if clamped_length > source_move_axis.x.hypot(source_move_axis.y) {
				source_move_axis = clamped_move_axis;
			}
			if clamped_length > dead_zone {
				if let Some(gamepad_kind) = source.read_gamepad_kind() {
					source_gamepad_kind = Some(gamepad_kind);
				}
			}
		}
		self.source_move_axis = source_move_axis;

		// 바깥 입력원이 입력을 넣었으면 마지막 장치는 그 패드다.
		if let Some(gamepad_kind) = source_gamepad_kind {
			input_manager.set_last_input_device(InputDevice::Gamepad);
			input_manager.set_last_gamepad_kind(gamepad_kind);
		}
	}

	//==============================================================================
	// 모두 뗀 것으로 되돌리기. (창이 포커스를 잃었을 때, 뗌도 알리지 않는다)
	//==============================================================================
	pub fn clear(&mut self) {
		for state in self.states.values_mut() {
			*state = ActionState::default();
		}
		self.key_move_axis = Vector2::zero();
		self.gamepad_axis_list.clear();
		self.source_move_axis = Vector2::zero();
	}

	//==============================================================================
	// 동작이 눌려 있는지.
	//==============================================================================
	pub fn is_pressed(&self, action: &str) -> bool {
		match self.states.get(action) {
			Some(state) => {
				state.is_pressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 이번 프레임에 눌렸는지. (set_repeat 를 건 동작은 반복 때도 참)
	//==============================================================================
	pub fn is_just_pressed(&self, action: &str) -> bool {
		match self.states.get(action) {
			Some(state) => {
				(state.is_pressed && !state.was_pressed) || state.is_repeat_fired
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 이번 프레임에 떼어졌는지.
	//==============================================================================
	pub fn is_just_released(&self, action: &str) -> bool {
		match self.states.get(action) {
			Some(state) => {
				!state.is_pressed && state.was_pressed
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 동작의 값. (0 ~ 1, 키와 디지털 단추는 1, 트리거와 축은 기운 만큼)
	//==============================================================================
	pub fn read_value(&self, action: &str) -> f32 {
		match self.states.get(action) {
			Some(state) => {
				state.value
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	// 두 동작으로 만든 축. (-1 ~ 1, 양쪽 동작의 값의 차)
	//==============================================================================
	pub fn read_axis(&self, negative_action: &str, positive_action: &str) -> f32 {
		self.read_value(positive_action) - self.read_value(negative_action)
	}

	//==============================================================================
	// 게임패드 스틱. (연결된 패드 가운데 가장 크게 기운 것, 죽은 구역을 길이로 빼고 길이 1 이하, 오른쪽 +x 아래 +y)
	//==============================================================================
	pub fn read_gamepad_stick(&self, x_axis_index: usize, y_axis_index: usize) -> Vector2 {
		let mut stick_vector = Vector2::zero();
		for axis_value_list in self.gamepad_axis_list.iter() {
			let axis_x = axis_value_list.get(x_axis_index).copied().unwrap_or(0.0);
			let axis_y = axis_value_list.get(y_axis_index).copied().unwrap_or(0.0);
			let length = axis_x.hypot(axis_y);
			let scaled_length = self.apply_dead_zone(length);
			if scaled_length > stick_vector.x.hypot(stick_vector.y) {
				stick_vector = Vector2::create(axis_x / length * scaled_length, axis_y / length * scaled_length);
			}
		}
		stick_vector
	}

	//==============================================================================
	// 이동 축. (WASD 와 방향키, 왼쪽 스틱, 바깥 입력원의 이동 축 가운데 가장 긴 것, 길이 1 이하, 오른쪽 +x 아래 +y)
	// - 키는 대각선도 길이 1 이다. 글자 입력칸에 쓰는 동안은 키를 듣지 않는다.
	//==============================================================================
	pub fn read_move_axis(&self) -> Vector2 {
		let mut move_vector = self.key_move_axis;
		let stick_vector = self.read_gamepad_stick(GamepadAnalogStickCode::LEFT_X, GamepadAnalogStickCode::LEFT_Y);
		for candidate_vector in [stick_vector, self.source_move_axis] {
			if candidate_vector.x.hypot(candidate_vector.y) > move_vector.x.hypot(move_vector.y) {
				move_vector = candidate_vector;
			}
		}
		move_vector
	}

	//==============================================================================
	// 죽은 구역 빼기. (죽은 구역 이하면 0, 나머지를 0 ~ 1 로 다시 펴서 조금 기울였을 때 튀지 않게)
	//==============================================================================
	fn apply_dead_zone(&self, value: f32) -> f32 {
		if !value.is_finite() || value <= self.dead_zone {
			return 0.0;
		}
		(value.min(1.0) - self.dead_zone) / (1.0 - self.dead_zone)
	}
}

impl Default for InputActions {
	fn default() -> InputActions {
		InputActions::new()
	}
}

//==============================================================================
// 길이 1 로 자르기.
//==============================================================================
fn clamp_length(x: f32, y: f32) -> Vector2 {
	let vector_x = if x.is_finite() {
		x
	}
	else {
		0.0
	};
	let vector_y = if y.is_finite() {
		y
	}
	else {
		0.0
	};
	let length = vector_x.hypot(vector_y);
	if length <= 1.0 {
		return Vector2::create(vector_x, vector_y);
	}
	Vector2::create(vector_x / length, vector_y / length)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::gamepadmanager::{Gamepad, GamepadButton, GamepadButtonCode};

	//==============================================================================
	// 시험용 입력 매니저. (게임패드 로그 끔)
	//==============================================================================
	fn create_test_input_manager() -> InputManager {
		let mut input_manager = InputManager::new();
		input_manager.get_gamepad_manager_mut().set_log_enabled(false);
		input_manager
	}

	//==============================================================================
	// 한 프레임. (사건을 넣은 뒤 tick 대신 동작만 갱신하고 프레임 끝을 부른다, gilrs 를 열지 않는다)
	//==============================================================================
	fn run_frame(input_manager: &mut InputManager, time_delta: f32) {
		input_manager.update_input_actions(time_delta);
	}

	//==============================================================================
	// 시험용 바깥 입력원.
	//==============================================================================
	struct TestSource {
		menu_value: f32,
	}

	impl InputActionSource for TestSource {
		fn read_action_value(&mut self, action: &str) -> f32 {
			if action == "menu" {
				self.menu_value
			}
			else {
				0.0
			}
		}

		fn read_move_axis(&mut self) -> Option<Vector2> {
			Some(Vector2::create(0.0, 3.0))
		}

		fn read_gamepad_kind(&mut self) -> Option<GamepadKind> {
			Some(GamepadKind::Nintendo)
		}
	}

	//==============================================================================
	// 프레임 사이에 눌렀다 뗀 키가 동작의 누름 한 번과 다음 프레임의 뗌이 된다.
	//==============================================================================
	#[test]
	fn key_tap_within_frame() {
		let mut input_manager = create_test_input_manager();
		input_manager.get_input_actions_mut().bind_key("confirm", "Space");
		input_manager.push_key("Space");
		input_manager.pop_key("Space");
		assert!(input_manager.is_key_just_pressed("Space"));
		assert!(input_manager.is_key_just_released("Space"));
		assert!(!input_manager.is_key_pressed("Space"));
		run_frame(&mut input_manager, 0.016);
		assert!(input_manager.get_input_actions().is_just_pressed("confirm"));
		input_manager.update_frame_snapshot();
		assert!(!input_manager.is_key_just_pressed("Space"));
		run_frame(&mut input_manager, 0.016);
		assert!(!input_manager.get_input_actions().is_pressed("confirm"));
		assert!(input_manager.get_input_actions().is_just_released("confirm"));
	}

	//==============================================================================
	// 반복: 기다린 뒤 간격마다, 끊긴 프레임에서는 몰아서 하지 않는다.
	//==============================================================================
	#[test]
	fn repeat_while_held() {
		let mut input_manager = create_test_input_manager();
		input_manager.get_input_actions_mut().bind_key("right", "ArrowRight");
		input_manager.get_input_actions_mut().set_repeat("right", 0.25, 0.1);
		input_manager.push_key("ArrowRight");
		let mut just_pressed_count = 0;
		for _ in 0..60 {
			run_frame(&mut input_manager, 1.0 / 60.0);
			if input_manager.get_input_actions().is_just_pressed("right") {
				just_pressed_count += 1;
			}
			input_manager.update_frame_snapshot();
		}
		// 1 초: 첫 누름 + 0.25, 0.35, ... 0.95 의 반복 여덟.
		assert_eq!(just_pressed_count, 9);
		run_frame(&mut input_manager, 5.0);
		assert!(input_manager.get_input_actions().is_just_pressed("right"));
		run_frame(&mut input_manager, 1.0 / 60.0);
		assert!(!input_manager.get_input_actions().is_just_pressed("right"));
	}

	//==============================================================================
	// 이동 축: WASD 대각선은 길이 1, 왼쪽 스틱은 죽은 구역을 빼고 다시 편다.
	//==============================================================================
	#[test]
	fn move_axis_from_keys_and_stick() {
		let mut input_manager = create_test_input_manager();
		input_manager.push_key("KeyW");
		input_manager.push_key("KeyD");
		run_frame(&mut input_manager, 0.016);
		let move_axis = input_manager.get_input_actions().read_move_axis();
		assert!((move_axis.x - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.0001);
		assert!((move_axis.y + std::f32::consts::FRAC_1_SQRT_2).abs() < 0.0001);
		input_manager.pop_key("KeyW");
		input_manager.pop_key("KeyD");

		let mut gamepad = Gamepad::create(0, "test 054c");
		gamepad.axes[GamepadAnalogStickCode::LEFT_X] = -0.6;
		gamepad.buttons[GamepadButtonCode::A_CROSS] = GamepadButton { pressed: true, value: 1.0 };
		input_manager.get_gamepad_manager_mut().update_all_gamepads_with_list(vec![Some(gamepad)]);
		input_manager.get_input_actions_mut().bind_gamepad_button("padConfirm", GamepadButtonCode::A_CROSS);
		input_manager.get_input_actions_mut().bind_axis("stickLeft", GamepadAnalogStickCode::LEFT_X, -1.0);
		run_frame(&mut input_manager, 0.016);
		let input_actions = input_manager.get_input_actions();
		let move_axis = input_actions.read_move_axis();
		assert!((move_axis.x + 0.5).abs() < 0.0001);
		assert_eq!(move_axis.y, 0.0);
		assert!(input_actions.is_pressed("padConfirm"));
		assert!(input_actions.is_pressed("stickLeft"));
		assert!((input_actions.read_value("stickLeft") - 0.5).abs() < 0.0001);
		assert!((input_actions.read_axis("stickLeft", "padConfirm") - 0.5).abs() < 0.0001);
	}

	//==============================================================================
	// 바깥 입력원: 동작 값, 이동 축(길이 1 로 자름), 마지막 장치.
	//==============================================================================
	#[test]
	fn external_source() {
		let mut input_manager = create_test_input_manager();
		input_manager.get_input_actions_mut().bind_key("menu", "Escape");
		let source_id = input_manager.get_input_actions_mut().add_source(Box::new(TestSource { menu_value: 1.0 }));
		run_frame(&mut input_manager, 0.016);
		assert!(input_manager.get_input_actions().is_pressed("menu"));
		assert_eq!(input_manager.get_input_actions().read_move_axis(), Vector2::create(0.0, 1.0));
		assert_eq!(input_manager.get_last_input_device(), InputDevice::Gamepad);
		assert_eq!(input_manager.get_last_gamepad_kind(), GamepadKind::Nintendo);
		assert!(input_manager.get_input_actions_mut().remove_source(source_id));
		assert!(!input_manager.get_input_actions_mut().remove_source(source_id));
		run_frame(&mut input_manager, 0.016);
		assert!(input_manager.get_input_actions().is_just_released("menu"));
	}
}
