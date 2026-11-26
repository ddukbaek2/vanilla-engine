# Input

The engine receives mouse, touch, keyboard and gamepad input and stores it in `InputManager`. On desktop it is filled by the window (winit) and the gamepad library (gilrs); on the web, by browser events on the canvas and the document. Mouse and touch are merged into a single "touch" stream that is passed to scenes and nodes. This chapter covers receiving touches in a scene, passing them to nodes, multiple fingers, mouse buttons and the wheel, the keyboard, input actions (`InputActions`), gamepads and the mouse cursor.

The engine holds one `InputManager`; take it out in scene functions with `engine.get_input_manager()` (or `get_input_manager_mut()` to change it). Components read it with `context.get_engine().get_input_manager()`.

## Receiving touches in a scene

Every frame, right after the scene's `tick`, the engine looks at the input state and calls the functions below. All positions are view coordinates (the coordinates applied by `draw_scene_background`).

| Function | When it is called |
| --- | --- |
| `touch_press(engine, view_input_position)` | The frame of the press |
| `touch_move(engine, view_input_position)` | When the touch moves while held (not called on the press frame) |
| `touch_release(engine, view_input_position)` | The frame of the release (right after `touch_press` if pressed and released within one frame) |
| `touch_cancel(engine, view_input_position)` | The frame the touch was cancelled |
| `touch_wheel(engine, view_input_position, wheel_delta)` | When the wheel was rolled this frame (`wheel_delta` is the horizontal and vertical amount accumulated this frame) |

By default every mouse button is passed on as a touch. To pass only the left button as a touch, call `input_manager.set_touch_from_left_button_only(true)`.

```rust,no_run
use vanilla::{Color, Engine, EngineConfiguration, Graphic, Scene, SceneBase, Vector2};

struct DragScene {
	scene_base: SceneBase,
	ball_position: Vector2,
	grab_offset: Option<Vector2>,
}

impl Scene for DragScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn touch_press(&mut self, _engine: &mut Engine, view_input_position: Vector2) {
		let offset = self.ball_position.subtract(view_input_position);
		if offset.length() <= 60.0 {
			self.grab_offset = Some(offset);
		}
	}

	fn touch_move(&mut self, _engine: &mut Engine, view_input_position: Vector2) {
		if let Some(grab_offset) = self.grab_offset {
			self.ball_position = view_input_position.add(grab_offset);
		}
	}

	fn touch_release(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
		self.grab_offset = None;
	}

	fn touch_cancel(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
		self.grab_offset = None;
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		if self.grab_offset.is_some() {
			graphic.set_fill_color(Color::create(1.0, 0.6, 0.2, 1.0));
		}
		else {
			graphic.set_fill_color(Color::create(0.3, 0.7, 1.0, 1.0));
		}
		graphic.draw_circle(self.ball_position, 60.0);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let drag_scene = DragScene {
		scene_base: SceneBase::new(),
		ball_position: Vector2::create(640.0, 360.0),
		grab_offset: None,
	};
	if let Err(error) = engine.run(Box::new(drag_scene)) {
		eprintln!("{}", error);
	}
}
```

The scene's touch functions have empty default implementations, so override only what you need. The engine calls them separately from `tick`, so they are still called if you override `tick`. Passing touches to nodes, however, happens inside `scene_base.tick`, so if you override `tick` you must call `self.scene_base.tick(engine, time_delta)` for nodes to receive touches.

## Passing touches to nodes

A node needs two things to receive touches.

1. `WorldNode::set_interactable(true)`
2. A component that overrides `touch_press`, `touch_move`, `touch_release` or `touch_cancel`. The node passes each touch to all of its enabled components in attach order.

In the original, the scene had to create and connect a `TouchRecognizer`, but in the Rust version the scene's default `tick` (`SceneBase::tick`) does this. On a press it walks the tree from the root in draw order and picks, among nodes that are active, visible, receive touches and contain the point in their content area, the one drawn last (shown on top). Descendants outside a node with a `Mask` are not picked. The picked node keeps receiving `touch_move` and `touch_release` until the release, even if the finger leaves it. (This is what the original `TouchRaycaster` does) A component's touch functions also receive a `ComponentContext`, so they can reach their own node, the tree and the engine.

```rust
use vanilla::core::component::RequiredComponent;
use vanilla::{Color, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Paint, Pivot, SceneBase, Vector2};

#[derive(Default)]
struct PressHighlight {
	component_base: ComponentBase,
	click_count: u32,
}

#[vanilla::reflect]
impl PressHighlight {}

#[vanilla::reflect]
impl Component for PressHighlight {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"PressHighlight"
	}

	fn require(&self) -> Vec<RequiredComponent> {
		vec![RequiredComponent::of::<Paint>()]
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn touch_press(&mut self, context: &mut ComponentContext, _view_input_position: Vector2) {
		if let Some(paint) = context.get_node_mut().get_component_mut::<Paint>() {
			paint.set_color(Color::create(1.0, 0.8, 0.2, 1.0));
		}
	}

	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if let Some(paint) = context.get_node_mut().get_component_mut::<Paint>() {
			paint.set_color(Color::create(0.3, 0.5, 0.9, 1.0));
		}
		let node_id = context.get_node_id();
		let node_tree = context.get_node_tree();
		let world_matrix = node_tree.calculate_world_matrix(node_id);
		if node_tree[node_id].contains_point(&world_matrix, view_input_position) {
			self.click_count += 1;
		}
	}
}

let mut engine = Engine::new(EngineConfiguration::new());
let mut scene_base = SceneBase::new();
let root_id = scene_base.get_root_id();
let node_tree = scene_base.get_node_tree_mut();
let button_id = node_tree.create_child(root_id, "button");
let button_node = &mut node_tree[button_id];
button_node.set_pivot(Pivot::TOP_LEFT);
button_node.set_rect(100.0, 100.0, 200.0, 80.0);
button_node.set_interactable(true);
button_node.add_component(PressHighlight::default());
assert!(button_node.has_component::<Paint>());

let input_manager = engine.get_input_manager_mut();
input_manager.set_view_input_position(Vector2::create(150.0, 120.0));
input_manager.set_touch_pressed(true);
scene_base.tick(&engine, 1.0 / 60.0);
let button_node = &scene_base.get_node_tree()[button_id];
assert_eq!(button_node.get_component::<Paint>().expect("paint").get_color(), Color::create(1.0, 0.8, 0.2, 1.0));

let input_manager = engine.get_input_manager_mut();
input_manager.set_touch_pressed(false);
input_manager.set_touch_released(true);
scene_base.tick(&engine, 1.0 / 60.0);
let button_node = &scene_base.get_node_tree()[button_id];
assert_eq!(button_node.get_component::<PressHighlight>().expect("highlight").click_count, 1);
```

To check the result without a window, the example above imitates the input the platform feeds in (`set_view_input_position`, `set_touch_pressed`, `set_touch_released`) and calls `scene_base.tick` directly. In a game the engine does this.

- `PressHighlight` requires `Paint` through `require()`, so `add_component` attaches `Paint` first.
- The wheel, regardless of any press, goes to the frontmost scroll area covering that point (the node of a component whose `is_wheel_receiver` returns `true`) or touch receiving node, then upward through its parents, asking each component's `wheel(context, wheel_delta)`. It stops where one returns `true`. Whether a component used the wheel is available from `scene_base.is_wheel_handled()`. (for using unused wheel input for things like map zoom)
- `TouchRecognizer` is a gesture for buttons inside scroll areas. If an ancestor of the pressed node has a `UIScrollView`, it delays the press by 0.15 seconds, and when the touch is dragged more than 10 pixels it cancels the node's press and passes the drag to the scroll area. `UIScene` and game scenes (`GameSceneBase`) use it on their own. To use it in a scene that implements `Scene` directly, keep a `TouchRecognizer` and call `node_tree.tick(engine, time_delta)`, `touch_recognizer.tick_touch(node_tree, engine)` and `scene_base.tick_tweens(time_delta)` instead of `scene_base.tick`.
- UI components such as buttons and scroll views are covered in the "UI" chapter.

## Multiple fingers (TouchPoint)

The scene's touch functions receive only one position. Read multiple fingers from `InputManager`. Only fingers on a touch screen are included, not the mouse. (Touches on a desktop window are included too)

- `get_touch_points()`: the fingers currently down (in the order they touched). Phases are `TouchPhase::Began` (touched this frame), `Moved` and `Stationary`
- `get_ended_touch_points()`: fingers lifted this frame (phase `Ended` or `Cancelled`, position where they were lifted)
- `find_touch_point(identifier)`: find one by number (`None` if absent)

`TouchPoint` has the fields `identifier` (the same number while the finger is down), `phase` (`TouchPhase`), `canvas_native_position` (logical pixels from the top left inside the window) and `view_position` (view coordinates).

```rust
use vanilla::{InputManager, TouchPhase, Vector2};

let mut input_manager = InputManager::new();
input_manager.begin_touch_point(7, Vector2::create(10.0, 10.0), Vector2::create(20.0, 20.0));
input_manager.begin_touch_point(8, Vector2::create(50.0, 10.0), Vector2::create(100.0, 20.0));
assert_eq!(input_manager.get_touch_points().len(), 2);
assert_eq!(input_manager.get_touch_points()[0].phase, TouchPhase::Began);

input_manager.update_frame_snapshot();
input_manager.move_touch_point(8, Vector2::create(60.0, 10.0), Vector2::create(120.0, 20.0));
let moved_point = input_manager.find_touch_point(8).expect("finger");
assert_eq!(moved_point.phase, TouchPhase::Moved);
assert_eq!(moved_point.view_position, Vector2::create(120.0, 20.0));
assert_eq!(input_manager.find_touch_point(7).expect("finger").phase, TouchPhase::Stationary);

input_manager.end_touch_point(7, Vector2::create(10.0, 10.0), Vector2::create(20.0, 20.0), false);
assert_eq!(input_manager.get_touch_points().len(), 1);
assert_eq!(input_manager.get_ended_touch_points()[0].phase, TouchPhase::Ended);
assert!(input_manager.find_touch_point(7).is_none());
```

`begin_touch_point`, `move_touch_point`, `end_touch_point` and `update_frame_snapshot` in the example above are called by the platform and the engine; here they are called to imitate input without a window. A game only reads `get_touch_points()` in the scene's `tick`.

## Mouse buttons and the wheel

| Function | Description |
| --- | --- |
| `is_mouse_button_down(button)` | Whether it is held. Buttons are `MouseButton::Left`, `Middle` and `Right` (`get_index()` gives slot numbers 0, 1 and 2) |
| `is_mouse_button_just_pressed(button)`, `is_mouse_button_just_released(button)` | Whether it was pressed or released this frame (also true if pressed and released between frames) |
| `get_view_input_position()`, `get_canvas_native_input_position()` | The last pointer position (view coordinates, logical pixels inside the window) |
| `is_touch_down()` | Whether a touch (a finger, or a mouse button passed on as a touch) is held |
| `has_wheel_delta()`, `get_wheel_delta_x()`, `get_wheel_delta_y()` | Wheel amounts accumulated this frame (pixels; Y is positive when scrolling content down) |
| `is_wheel_with_ctrl()` | Whether the last wheel was rolled with Ctrl held |
| `set_context_menu_prevented(is_prevented)` | Whether to block the canvas right click menu on the web (true by default; desktop windows have no such menu) |
| `set_touch_from_left_button_only(is_left_button_only)` | Whether only the left button is passed on as a touch (false by default) |

## Keyboard

Keys are queried by `KeyboardEvent.code` strings. These names are fixed by key position regardless of keyboard layout. (such as `"KeyA"`, `"Digit1"`, `"ArrowLeft"`, `"Space"`, `"Enter"`, `"Escape"`, `"ShiftLeft"` and `"ControlLeft"`) The original's `KeyCode` table is public in the Rust version as constants such as `KeyCode::A`, `KeyCode::SPACE`, `KeyCode::LEFT` (same as `KeyCode::ARROWLEFT`) and `KeyCode::SHIFT` (`"ShiftLeft"`), whose values are those strings. On desktop, winit key names are converted to the same strings.

| Function | Description |
| --- | --- |
| `is_key_pressed(code)` | Whether it is held right now |
| `is_key_just_pressed(code)` | Whether it was pressed this frame (also true for keys pressed and released between frames) |
| `is_key_just_released(code)` | Whether it was released this frame |
| `set_prevented_keys(codes)` | Keys whose browser default action is blocked on the web (such as Tab moving focus, or Space and the arrows scrolling the page). Desktop windows have no such actions |
| `is_typing_in_text_field()` | Whether an engine input field is active or, on the web, the document focus is in a text field (input, textarea, contenteditable, select) |

```rust
use vanilla::{InputManager, KeyCode};

let mut input_manager = InputManager::new();
input_manager.push_key(KeyCode::SPACE);
assert!(input_manager.is_key_pressed("Space"));
assert!(input_manager.is_key_just_pressed(KeyCode::SPACE));

input_manager.update_frame_snapshot();
assert!(input_manager.is_key_pressed(KeyCode::SPACE));
assert!(!input_manager.is_key_just_pressed(KeyCode::SPACE));

input_manager.pop_key(KeyCode::SPACE);
assert!(input_manager.is_key_just_released(KeyCode::SPACE));
assert!(!input_manager.is_key_pressed(KeyCode::SPACE));

input_manager.set_prevented_keys(&[KeyCode::TAB, KeyCode::SPACE]);
assert!(input_manager.is_key_prevented("Tab"));
assert_eq!(KeyCode::LEFT, KeyCode::ARROWLEFT);
```

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Graphic, KeyCode, Rect, Scene, SceneBase, Vector2};

struct KeyboardScene {
	scene_base: SceneBase,
	position: Vector2,
}

impl Scene for KeyboardScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		engine.get_input_manager_mut().set_prevented_keys(&[KeyCode::SPACE, KeyCode::UP, KeyCode::DOWN]);
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.scene_base.tick(engine, time_delta);
		let input_manager = engine.get_input_manager();
		let speed = 400.0 * time_delta;
		if input_manager.is_key_pressed(KeyCode::LEFT) || input_manager.is_key_pressed(KeyCode::A) {
			self.position.x -= speed;
		}
		if input_manager.is_key_pressed(KeyCode::RIGHT) || input_manager.is_key_pressed(KeyCode::D) {
			self.position.x += speed;
		}
		if input_manager.is_key_pressed(KeyCode::UP) || input_manager.is_key_pressed(KeyCode::W) {
			self.position.y -= speed;
		}
		if input_manager.is_key_pressed(KeyCode::DOWN) || input_manager.is_key_pressed(KeyCode::S) {
			self.position.y += speed;
		}
		if input_manager.is_key_just_pressed(KeyCode::SPACE) {
			self.position = Vector2::create(640.0, 360.0);
		}
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		graphic.set_fill_color("#66ddaa");
		graphic.draw_rect(Rect::create(self.position.x - 40.0, self.position.y - 40.0, 80.0, 80.0));
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let keyboard_scene = KeyboardScene {
		scene_base: SceneBase::new(),
		position: Vector2::create(640.0, 360.0),
	};
	if let Err(error) = engine.run(Box::new(keyboard_scene)) {
		eprintln!("{}", error);
	}
}
```

## Input actions (InputActions)

Bind several keys, gamepad buttons and gamepad axis directions to action names the game defines (strings such as `"confirm"` and `"jump"`), and the game reads them by action name. `input_manager.get_input_actions()` (or `get_input_actions_mut()` to bind) gives the one the engine holds, and the engine updates it at the start of each frame. Only actions recorded at least once with `bind_key`, `bind_gamepad_button`, `bind_axis` or `set_repeat` are updated.

| Function | Description |
| --- | --- |
| `bind_key(action, code)` | Bind a key (binding the same key twice keeps one) |
| `bind_gamepad_button(action, button_code)` | Bind a gamepad button (`GamepadButtonCode`, on every connected pad) |
| `bind_axis(action, axis_index, direction)` | Bind one direction of an axis (`GamepadAnalogStickCode`; a positive `direction` is the + side, negative the - side) |
| `unbind_action(action)`, `get_bound_keys(action)` | Remove every binding (repeat settings stay), list the bound keys |
| `set_repeat(action, delay_seconds, interval_seconds)` | While held, `is_just_pressed` becomes true again every interval after the delay (menu selection; an interval of 0 or less turns it off) |
| `is_pressed(action)`, `is_just_pressed(action)`, `is_just_released(action)` | State |
| `read_value(action)` | 0 to 1. Keys and digital buttons give 1, triggers and axes give how far they are pushed (axes rescaled after removing the dead zone) |
| `read_axis(negative_action, positive_action)` | An axis from -1 to 1 built from two actions |
| `read_move_axis()` | The longest movement among WASD and the arrow keys, the left stick and external sources (length 1 or less, right is +x, down is +y; diagonals from keys also have length 1) |
| `read_gamepad_stick(x_axis_index, y_axis_index)` | The most tilted stick among connected pads (dead zone removed by length) |
| `set_dead_zone(value)`, `set_axis_press_threshold(value)` | The length that filters stick jitter around the center (default 0.2), the tilt at which an axis counts as pressed (default 0.5) |
| `set_keys_ignored_while_typing(is_ignored)` | Whether to ignore keys while typing in a text field (true by default) |
| `add_source(source)`, `remove_source(source_id)` | Plug in and remove an input source the engine does not know (a value implementing the `InputActionSource` trait; remove it with the number `add_source` returned) |

Gamepad button numbers follow the W3C standard layout and are written with `GamepadButtonCode` constants: `A_CROSS` (0), `B_CIRCLE` (1), `X_SQUARE` (2), `Y_TRIANGLE` (3), `L1` (4), `R1` (5), `L2` (6), `R2` (7), `SHARE_VIEW` (8), `OPTIONS_MENU` (9), `L3` (10), `R3` (11), `DPAD_UP`, `DPAD_DOWN`, `DPAD_LEFT`, `DPAD_RIGHT` (12 to 15), `HOME_PS` (16) and `TOUCHPAD` (17). Axis numbers are `GamepadAnalogStickCode::LEFT_X` (0), `LEFT_Y` (1, down is +), `RIGHT_X` (2) and `RIGHT_Y` (3).

```rust
use vanilla::{InputActions, InputManager, KeyCode};

let mut input_manager = InputManager::new();
let mut input_actions = InputActions::new();
input_actions.bind_key("left", KeyCode::LEFT);
input_actions.bind_key("right", KeyCode::RIGHT);
input_actions.bind_key("jump", KeyCode::SPACE);
input_actions.set_repeat("right", 0.25, 0.1);

input_manager.push_key(KeyCode::RIGHT);
input_manager.push_key(KeyCode::SPACE);
input_actions.update(&mut input_manager, 1.0 / 60.0);
assert!(input_actions.is_just_pressed("jump"));
assert_eq!(input_actions.read_value("jump"), 1.0);
assert_eq!(input_actions.read_axis("left", "right"), 1.0);
assert_eq!(input_actions.read_move_axis().x, 1.0);

let mut repeat_count = 0;
for _ in 0..60 {
	input_manager.update_frame_snapshot();
	input_actions.update(&mut input_manager, 1.0 / 60.0);
	if input_actions.is_just_pressed("right") {
		repeat_count += 1;
	}
}
assert_eq!(repeat_count, 8);

input_actions.unbind_action("jump");
input_actions.bind_key("jump", KeyCode::W);
assert_eq!(input_actions.get_bound_keys("jump"), ["KeyW"]);
```

`input_actions.update` and `update_frame_snapshot` are called by the engine; the example above calls them to imitate input without a window. In a scene, the engine's input actions are used like this.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, GamepadAnalogStickCode, GamepadButtonCode, Graphic, KeyCode, Rect, Scene, SceneBase, Vector2};

struct ActionScene {
	scene_base: SceneBase,
	position: Vector2,
	selected_index: i32,
}

impl Scene for ActionScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		let input_actions = engine.get_input_manager_mut().get_input_actions_mut();
		input_actions.bind_key("confirm", KeyCode::ENTER);
		input_actions.bind_key("confirm", KeyCode::SPACE);
		input_actions.bind_gamepad_button("confirm", GamepadButtonCode::A_CROSS);
		input_actions.bind_key("next", KeyCode::TAB);
		input_actions.bind_gamepad_button("next", GamepadButtonCode::DPAD_RIGHT);
		input_actions.bind_axis("next", GamepadAnalogStickCode::RIGHT_X, 1.0);
		input_actions.set_repeat("next", 0.4, 0.12);
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.scene_base.tick(engine, time_delta);
		let input_actions = engine.get_input_manager().get_input_actions();
		let move_axis = input_actions.read_move_axis();
		self.position = self.position.add(move_axis.multiply_scalar(400.0 * time_delta));
		if input_actions.is_just_pressed("next") {
			self.selected_index = (self.selected_index + 1) % 4;
		}
		if input_actions.is_just_pressed("confirm") {
			self.position = Vector2::create(640.0, 360.0);
		}
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		for index in 0..4 {
			if index == self.selected_index {
				graphic.set_fill_color("#ffcc33");
			}
			else {
				graphic.set_fill_color("#445566");
			}
			graphic.draw_rect(Rect::create(40.0 + index as f64 * 120.0, 40.0, 100.0, 40.0));
		}
		graphic.set_fill_color("#66ddaa");
		graphic.draw_circle(self.position, 30.0);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let action_scene = ActionScene {
		scene_base: SceneBase::new(),
		position: Vector2::create(640.0, 360.0),
		selected_index: 0,
	};
	if let Err(error) = engine.run(Box::new(action_scene)) {
		eprintln!("{}", error);
	}
}
```

An external source implements the `InputActionSource` trait. `read_action_value(action)` must be implemented (0 to 1; 0.5 or more counts as pressed), and the movement axis (`read_move_axis`) and the pad kind to record as the last device (`read_gamepad_kind`) are overridden when needed. The original accepts an object with `readActionValue`, but the Rust version uses a trait, and removal goes by the number `add_source` returned.

## Gamepads (GamepadManager)

Get it with `input_manager.get_gamepad_manager()` (or `get_gamepad_manager_mut()` for changes such as vibration). The engine opens the pad input source on its first update and reads the state every frame. On desktop, gilrs reads operating system input and converts it to the W3C standard layout (buttons 0 to 16, axes 0 to 3); on the web it reads `navigator.getGamepads()` like the original. Pads are queried by their `Gamepad.index` number. (On desktop, from 0 in the order they connected; on the web, the browser's number)

| Function | Description |
| --- | --- |
| `get_all_connected_gamepads()`, `get_connected_gamepad_count()`, `get_connected_gamepad(index)` | The list and count of connected pads (`Gamepad`, copies read in place when called), one by number |
| `is_button_pressed(gamepad_index, button_index)` | Whether a button is held |
| `is_button_just_pressed(gamepad_index, button_index)`, `is_button_just_released(...)` | Whether it was pressed or released in this update |
| `get_button_value(gamepad_index, button_index)` | Button and trigger value (0 to 1, rounded to two decimal places) |
| `get_axis_value(gamepad_index, axis_index)` | Stick axis value (-1 to 1, dead zone not removed) |
| `vibrate(strong_magnitude, weak_magnitude, seconds)` | Vibrates every connected pad (strength 0 to 1; pads without a vibration device are skipped) |
| `set_input_log_enabled(is_enabled)` | Whether to log every button and axis change (off by default) |
| `set_log_enabled(is_enabled)` | The master switch for all logs, including connection and disconnection |
| `get_active_gamepad_id()` | The name of the pad that pressed a button or tilted an axis strongly in the last update (`None` if none) |

`read_gamepad_kind(gamepad_id)` tells `GamepadKind::Xbox`, `PlayStation`, `Nintendo` and `Other` apart from the pad name. Use it to match button prompt images to the pad. `input_manager.get_last_gamepad_kind()` gives the kind of the pad used last.

```rust
use vanilla::{read_gamepad_kind, Gamepad, GamepadAnalogStickCode, GamepadButton, GamepadButtonCode, GamepadKind, GamepadManager};

let mut gamepad_manager = GamepadManager::new();
gamepad_manager.set_log_enabled(false);
let mut gamepad = Gamepad::create(0, "DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)");
gamepad.axes[GamepadAnalogStickCode::LEFT_X] = -0.75;
gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad.clone())]);
assert_eq!(gamepad_manager.get_connected_gamepad_count(), 1);
assert_eq!(gamepad_manager.get_axis_value(0, GamepadAnalogStickCode::LEFT_X), -0.75);

gamepad.buttons[GamepadButtonCode::A_CROSS] = GamepadButton {
	pressed: true,
	value: 1.0,
};
gamepad_manager.update_all_gamepads_with_list(vec![Some(gamepad)]);
assert!(gamepad_manager.is_button_pressed(0, GamepadButtonCode::A_CROSS));
assert!(gamepad_manager.is_button_just_pressed(0, GamepadButtonCode::A_CROSS));

let gamepad_id = gamepad_manager.get_all_connected_gamepads()[0].id.clone();
assert_eq!(read_gamepad_kind(&gamepad_id), GamepadKind::PlayStation);
```

`update_all_gamepads_with_list` feeds a pad list without a device. (for tests or other input sources) In a game the engine reads the pads.

```rust,no_run
use vanilla::{read_gamepad_kind, Engine, EngineConfiguration, Gamepad, GamepadAnalogStickCode, GamepadButtonCode, GamepadKind, Scene, SceneBase, Vector2};

struct GamepadScene {
	scene_base: SceneBase,
}

impl Scene for GamepadScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.scene_base.tick(engine, time_delta);
		let gamepad_manager = engine.get_input_manager_mut().get_gamepad_manager_mut();
		let mut is_confirm_pressed = false;
		// The list is a copy read in place when called (like the original navigator.getGamepads()).
		let gamepad_list: Vec<Gamepad> = gamepad_manager.get_all_connected_gamepads();
		for gamepad in gamepad_list.iter() {
			if !gamepad_manager.is_button_just_pressed(gamepad.index, GamepadButtonCode::A_CROSS) {
				continue;
			}
			let left_stick_x = gamepad_manager.get_axis_value(gamepad.index, GamepadAnalogStickCode::LEFT_X);
			let left_stick_y = gamepad_manager.get_axis_value(gamepad.index, GamepadAnalogStickCode::LEFT_Y);
			let right_trigger = gamepad_manager.get_button_value(gamepad.index, GamepadButtonCode::R2);
			let button_name = match read_gamepad_kind(&gamepad.id) {
				GamepadKind::PlayStation => {
					"Cross"
				}
				_ => {
					"A"
				}
			};
			println!("{} {} {} {}", button_name, left_stick_x, left_stick_y, right_trigger);
			is_confirm_pressed = true;
		}
		if is_confirm_pressed {
			engine.get_input_manager_mut().get_gamepad_manager_mut().vibrate(0.8, 0.4, 0.2);
		}
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let gamepad_scene = GamepadScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(gamepad_scene)) {
		eprintln!("{}", error);
	}
}
```

## Last input device

`input_manager.get_last_input_device()` gives the device used last as one of `InputDevice::Keyboard`, `Mouse`, `Touch` and `Gamepad`. It starts as `Touch` on devices where a finger is the primary pointer (CSS `pointer: coarse` on the web) and `Keyboard` otherwise; desktop always starts with `Keyboard`. Pad input from external sources is also recorded as `Gamepad`. `is_touch_input()` tells whether the last pointer input was a finger. Use it to switch on screen prompts between "Tap" and "Press Enter".

## Mouse cursor (PointerCursor)

The engine holds one `PointerCursor`; take it out with `engine.get_pointer_cursor_mut()`. (in place of the original `PointerCursor.getInstance()`) Turning it on with `set_enabled(true)` makes the engine set the cursor at the end of every frame. When it is off (the default) it does nothing. On desktop it changes the window cursor (winit cursor icons); on the web, the canvas CSS cursor.

- While the mouse is over a `UIButton` (including while pressing), it shows the button's cursor (default `"pointer"`); otherwise the cursor set with `set_screen_cursor(name)`.
- `set_cursor_names(button_cursor_name, default_cursor_name)` changes the cursor over buttons and the default cursor.
- Usable names are the CSS cursor names `pointer`, `crosshair`, `not-allowed`, `grab`, `grabbing`, `text`, `move`, `help`, `wait`, `progress`, `none` and `default`.
- `register_image_cursor(name, url, url_2x, hotspot_x, hotspot_y, fallback_cursor_name)` adds an image cursor that you then use by name. (1x and 2x images, the hotspot in 1x units, the CSS cursor for when the image cannot be used) The web uses the image addresses as a CSS value; desktop builds a window cursor from the image file relative to the asset folder. (using the given CSS cursor while building it and when the image cannot be read)
- On devices where a finger is the primary pointer, the cursor is left alone.

```rust
use vanilla::{Engine, EngineConfiguration};

let mut engine = Engine::new(EngineConfiguration::new());
let pointer_cursor = engine.get_pointer_cursor_mut();
assert!(!pointer_cursor.is_enabled());
pointer_cursor.set_enabled(true);
pointer_cursor.set_cursor_names("pointer", "default");
pointer_cursor.set_screen_cursor("grab");
assert_eq!(engine.get_pointer_cursor().get_screen_cursor(), "grab");
assert_eq!(engine.get_pointer_cursor().get_button_cursor_name(), "pointer");
```

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, Vector2};

struct CursorScene {
	scene_base: SceneBase,
}

impl Scene for CursorScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, engine: &mut Engine) {
		let pointer_cursor = engine.get_pointer_cursor_mut();
		pointer_cursor.set_enabled(true);
		pointer_cursor.set_cursor_names("pointer", "default");
		pointer_cursor.set_screen_cursor("grab");
	}

	fn touch_press(&mut self, engine: &mut Engine, _view_input_position: Vector2) {
		engine.get_pointer_cursor_mut().set_screen_cursor("grabbing");
	}

	fn touch_release(&mut self, engine: &mut Engine, _view_input_position: Vector2) {
		engine.get_pointer_cursor_mut().set_screen_cursor("grab");
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let cursor_scene = CursorScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(cursor_scene)) {
		eprintln!("{}", error);
	}
}
```

## Things to watch out for

- The scene's touch functions and `get_view_input_position()` see only one position. For gestures that need several fingers, such as pinching, use `get_touch_points()`.
- Unlike the original, even a scene that implements `Scene` directly passes touches to nodes through its default `tick`. (straight to the topmost node, without the scroll gesture) For delayed presses, such as buttons inside a scroll area, use `TouchRecognizer` or `UIScene`.
- The key table and gamepad number tables that were not public in the original exist in the Rust version as the `KeyCode`, `GamepadButtonCode`, `GamepadAnalogStickCode` and `GamepadTriggerCode` constants. The values are the same `KeyboardEvent.code` strings and W3C standard layout numbers as the original, so you can also use strings and numbers directly.
- The web receives keys on the window (`window`), so the page needs focus for keys to arrive. A game inside an iframe receives keys only after the iframe has been clicked once.
- When the window loses focus, the engine clears keys, mouse buttons, fingers and input actions with `input_manager.clear()` and passes the touch on as released. (the same on desktop and the web) The original kept the keys and cleared them when the page became visible again.
- One frame states (`is_key_just_pressed`, `is_mouse_button_just_pressed`, wheel amounts, lifted fingers, touch press and release) are cleared by the engine at the end of the frame. Read them inside the scene's `tick` or touch functions.
- On desktop, gamepads are opened on the first update. `vibrate` called before that does not vibrate. gilrs does not report a device's button list, so buttons outside the standard layout appear in the pad's button list only after they are first pressed.

## Related types

InputManager, KeyCode, MouseButton, TouchPoint, TouchPhase, InputDevice, InputActions, InputActionSource, GamepadManager, Gamepad, GamepadButton, GamepadButtonCode, GamepadAnalogStickCode, GamepadTriggerCode, GamepadKind, PointerCursor, TouchRecognizer, ComponentContext, SceneBase, WorldNode, UIScene
