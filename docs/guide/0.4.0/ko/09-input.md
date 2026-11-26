# 입력

엔진은 마우스와 터치, 키보드, 게임패드를 받아 `InputManager` 에 담습니다. 데스크탑은 창(winit)과 게임패드 라이브러리(gilrs)가, 웹은 캔버스와 문서의 브라우저 사건이 채웁니다. 마우스와 터치는 "터치" 한 갈래로 합쳐 씬과 노드에 넘깁니다. 이 장은 씬에서 터치 받기, 노드로 넘기기, 손가락 여럿, 마우스 단추와 휠, 키보드, 입력 동작(`InputActions`), 게임패드, 마우스 커서를 봅니다.

`InputManager` 는 엔진이 하나 들고, 씬의 함수에서 `engine.get_input_manager()` (바꿀 때는 `get_input_manager_mut()`) 로 꺼냅니다. 컴포넌트는 `context.get_engine().get_input_manager()` 로 읽습니다.

## 씬에서 터치 받기

엔진이 매 프레임 씬의 `tick` 바로 뒤에 입력 상태를 보고 아래 함수를 부릅니다. 좌표는 모두 뷰 좌표(`draw_scene_background` 가 거는 좌표)입니다.

| 함수 | 불리는 때 |
| --- | --- |
| `touch_press(engine, view_input_position)` | 누른 프레임 |
| `touch_move(engine, view_input_position)` | 누르고 있는 동안 움직였을 때 (누른 프레임에는 부르지 않습니다) |
| `touch_release(engine, view_input_position)` | 뗀 프레임 (한 프레임 안에 눌렀다 뗐으면 `touch_press` 바로 뒤에) |
| `touch_cancel(engine, view_input_position)` | 터치가 취소된 프레임 |
| `touch_wheel(engine, view_input_position, wheel_delta)` | 이번 프레임에 휠을 굴렸을 때 (`wheel_delta` 는 이번 프레임에 쌓인 가로, 세로 이동량) |

마우스는 처음에 어느 단추든 터치로 넘깁니다. 왼쪽 단추만 터치로 넘기려면 `input_manager.set_touch_from_left_button_only(true)` 를 부릅니다.

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

씬의 터치 함수는 기본 구현이 비어 있어 필요한 것만 덮어씁니다. 엔진이 `tick` 과 따로 부르므로 `tick` 을 덮어써도 터치 함수는 불립니다. 다만 노드로 터치를 넘기는 일은 `scene_base.tick` 안에서 하므로, `tick` 을 덮어쓰면 `self.scene_base.tick(engine, time_delta)` 를 불러야 노드가 터치를 받습니다.

## 노드로 터치 넘기기

노드가 터치를 받으려면 둘이 필요합니다.

1. `WorldNode::set_interactable(true)`
2. `touch_press`, `touch_move`, `touch_release`, `touch_cancel` 을 덮어쓴 컴포넌트. 노드는 받은 터치를 켜진 컴포넌트 모두에 붙인 차례로 넘깁니다.

원본은 씬이 `TouchRecognizer` 를 만들어 연결해야 했지만, 러스트 판은 씬의 기본 `tick` (`SceneBase::tick`) 이 이 일을 합니다. 누를 때 루트부터 그리는 차례로 트리를 돌아, 활성이고 보이며 터치를 받고 그 점을 내용 영역에 품는 노드 가운데 가장 나중에 그리는(가장 위에 보이는) 노드를 고릅니다. `Mask` 가 붙은 노드 밖에 있는 자손은 고르지 않습니다. 고른 노드는 뗄 때까지 손가락이 밖으로 나가도 `touch_move`, `touch_release` 를 받습니다. (원본의 `TouchRaycaster` 와 같은 일입니다) 컴포넌트의 터치 함수도 `ComponentContext` 를 받으므로 자기 노드와 트리, 엔진에 닿습니다.

```rust
use vanilla::core::component::RequiredComponent;
use vanilla::{Color, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Paint, Pivot, SceneBase, Vector2};

#[derive(Default)]
struct PressHighlight {
	component_base: ComponentBase,
	click_count: u32,
}

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

위 예제는 창 없이 확인하려고 플랫폼이 넣는 입력(`set_view_input_position`, `set_touch_pressed`, `set_touch_released`)을 흉내 내고 `scene_base.tick` 을 직접 불렀습니다. 게임에서는 엔진이 이 일을 합니다.

- `PressHighlight` 는 `require()` 로 `Paint` 를 요구하므로 `add_component` 가 `Paint` 를 먼저 붙입니다.
- 휠은 누름과 상관없이 그 자리를 덮는 것 가운데 가장 앞의 스크롤 칸(`is_wheel_receiver` 가 `true` 인 컴포넌트의 노드)이나 터치 받는 노드부터 부모 쪽으로 올라가며 컴포넌트의 `wheel(context, wheel_delta)` 에 묻습니다. `true` 를 돌려주면 거기서 멈춥니다. 컴포넌트가 휠을 받아 썼는지는 `scene_base.is_wheel_handled()` 로 봅니다. (받지 않은 휠을 지도 확대 같은 데 쓸 때)
- `TouchRecognizer` 는 스크롤 칸 안의 단추를 위한 손짓입니다. 누른 노드의 조상에 `UIScrollView` 가 있으면 누름을 0.15 초 늦춰 넘기고, 10 픽셀 넘게 끌면 노드의 누름을 취소하고 스크롤 칸에 끌기를 넘깁니다. `UIScene` 과 게임 씬(`GameSceneBase`)은 이것을 스스로 씁니다. `Scene` 을 바로 구현한 씬에서 쓰려면 `TouchRecognizer` 를 들고, `scene_base.tick` 대신 `node_tree.tick(engine, time_delta)`, `touch_recognizer.tick_touch(node_tree, engine)`, `scene_base.tick_tweens(time_delta)` 를 부릅니다.
- 버튼, 스크롤 뷰 같은 UI 컴포넌트는 "UI" 장에서 봅니다.

## 손가락 여럿 (TouchPoint)

씬의 터치 함수는 한 자리만 받습니다. 손가락 여럿은 `InputManager` 에서 읽습니다. 터치 화면의 손가락만 들어가고 마우스는 들어가지 않습니다. (데스크탑 창의 터치도 들어갑니다)

- `get_touch_points()`: 지금 닿아 있는 손가락 목록 (닿은 차례). 단계는 `TouchPhase::Began`(이번 프레임에 닿음), `Moved`, `Stationary`
- `get_ended_touch_points()`: 이번 프레임에 떨어진 손가락 (단계 `Ended` 또는 `Cancelled`, 자리는 뗀 자리)
- `find_touch_point(identifier)`: 번호로 찾기 (없으면 `None`)

`TouchPoint` 는 `identifier`(닿아 있는 동안 같은 번호), `phase`(`TouchPhase`), `canvas_native_position`(창 안쪽 왼쪽 위 기준 논리 픽셀), `view_position`(뷰 좌표) 필드를 갖습니다.

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

위 예제의 `begin_touch_point`, `move_touch_point`, `end_touch_point`, `update_frame_snapshot` 은 플랫폼과 엔진이 부르는 함수이고, 여기서는 창 없이 흉내 내려고 불렀습니다. 게임은 씬의 `tick` 에서 `get_touch_points()` 를 읽기만 합니다.

## 마우스 단추와 휠

| 함수 | 내용 |
| --- | --- |
| `is_mouse_button_down(button)` | 눌려 있는지. 단추는 `MouseButton::Left`, `Middle`, `Right` (칸 번호는 `get_index()` 가 0, 1, 2) |
| `is_mouse_button_just_pressed(button)`, `is_mouse_button_just_released(button)` | 이번 프레임에 눌렀는지, 뗐는지 (프레임 사이에 눌렀다 뗀 것도 참) |
| `get_view_input_position()`, `get_canvas_native_input_position()` | 마지막 포인터 자리 (뷰 좌표, 창 안쪽 논리 픽셀) |
| `is_touch_down()` | 터치(손가락이나 터치로 넘기는 마우스 단추)가 눌린 채인지 |
| `has_wheel_delta()`, `get_wheel_delta_x()`, `get_wheel_delta_y()` | 이번 프레임에 쌓인 휠 값 (픽셀, 내용을 아래로 넘기면 Y 가 양수) |
| `is_wheel_with_ctrl()` | 마지막 휠을 Ctrl 과 함께 굴렸는지 |
| `set_context_menu_prevented(is_prevented)` | 웹에서 캔버스의 오른쪽 클릭 메뉴를 막을지 (처음 참, 데스크탑 창에는 그 메뉴가 없습니다) |
| `set_touch_from_left_button_only(is_left_button_only)` | 왼쪽 단추만 터치로 넘길지 (처음 거짓) |

## 키보드

키는 `KeyboardEvent.code` 글로 묻습니다. 자판 배열과 상관없이 자리로 정해지는 이름입니다. (`"KeyA"`, `"Digit1"`, `"ArrowLeft"`, `"Space"`, `"Enter"`, `"Escape"`, `"ShiftLeft"`, `"ControlLeft"` 같은 것) 원본의 `KeyCode` 표는 러스트 판에서 `KeyCode::A`, `KeyCode::SPACE`, `KeyCode::LEFT`(`KeyCode::ARROWLEFT` 와 같음), `KeyCode::SHIFT`(`"ShiftLeft"`) 같은 상수로 공개되어 있고, 값은 그 글입니다. 데스크탑은 winit 의 키 이름을 같은 글로 바꿔 넣습니다.

| 함수 | 내용 |
| --- | --- |
| `is_key_pressed(code)` | 지금 눌려 있는지 |
| `is_key_just_pressed(code)` | 이번 프레임에 눌렸는지 (프레임 사이에 눌렀다 뗀 키도 참) |
| `is_key_just_released(code)` | 이번 프레임에 떼어졌는지 |
| `set_prevented_keys(codes)` | 웹에서 브라우저 기본 동작을 막을 키 (Tab 의 초점 옮기기, Space 와 화살표의 쪽 넘기기 같은 것). 데스크탑 창에는 그런 동작이 없습니다 |
| `is_typing_in_text_field()` | 엔진 입력 필드가 활성이거나, 웹에서 문서의 초점이 글자 입력칸(input, textarea, contenteditable, select)에 있는지 |

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

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
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

## 입력 동작 (InputActions)

게임이 정한 동작 이름(`"confirm"`, `"jump"` 같은 글)에 키, 게임패드 단추, 게임패드 축의 한쪽을 여럿 묶고, 게임은 동작 이름으로 읽습니다. `input_manager.get_input_actions()` (묶을 때는 `get_input_actions_mut()`) 가 엔진이 하나 들고 있는 것을 주고, 엔진이 프레임 처음에 갱신합니다. 동작은 `bind_key`, `bind_gamepad_button`, `bind_axis`, `set_repeat` 가운데 하나로 한 번이라도 적어 둔 것만 갱신합니다.

| 함수 | 내용 |
| --- | --- |
| `bind_key(action, code)` | 키 묶기 (같은 키를 두 번 묶어도 하나) |
| `bind_gamepad_button(action, button_code)` | 게임패드 단추 묶기 (`GamepadButtonCode`, 연결된 패드 모두) |
| `bind_axis(action, axis_index, direction)` | 축의 한쪽 묶기 (`GamepadAnalogStickCode`, `direction` 이 양수면 + 쪽, 음수면 - 쪽) |
| `unbind_action(action)`, `get_bound_keys(action)` | 묶은 것 모두 풀기 (반복 설정은 남음), 묶은 키 목록 |
| `set_repeat(action, delay_seconds, interval_seconds)` | 누르고 있으면 기다린 뒤부터 간격마다 `is_just_pressed` 가 다시 참 (메뉴 고르기, 간격이 0 이하면 끔) |
| `is_pressed(action)`, `is_just_pressed(action)`, `is_just_released(action)` | 상태 |
| `read_value(action)` | 0 ~ 1. 키와 디지털 단추는 1, 트리거와 축은 기운 만큼 (축은 죽은 구역을 빼고 다시 편 값) |
| `read_axis(negative_action, positive_action)` | 두 동작으로 만든 -1 ~ 1 축 |
| `read_move_axis()` | WASD 와 방향키, 왼쪽 스틱, 바깥 입력원 가운데 가장 긴 이동 (길이 1 이하, 오른쪽 +x, 아래 +y, 키는 대각선도 길이 1) |
| `read_gamepad_stick(x_axis_index, y_axis_index)` | 연결된 패드 가운데 가장 크게 기운 스틱 (죽은 구역을 길이로 뺌) |
| `set_dead_zone(value)`, `set_axis_press_threshold(value)` | 스틱 가운데 떨림을 거르는 길이 (처음 0.2), 축을 눌린 것으로 보는 기울기 (처음 0.5) |
| `set_keys_ignored_while_typing(is_ignored)` | 글자 입력칸에 쓰는 동안 키를 듣지 않을지 (처음 참) |
| `add_source(source)`, `remove_source(source_id)` | 엔진이 모르는 입력원 끼우기와 빼기 (`InputActionSource` 트레이트를 구현한 값, `add_source` 가 돌려준 번호로 뺍니다) |

게임패드 단추 번호는 W3C 표준 배치를 따르고 `GamepadButtonCode` 상수로 씁니다. `A_CROSS`(0), `B_CIRCLE`(1), `X_SQUARE`(2), `Y_TRIANGLE`(3), `L1`(4), `R1`(5), `L2`(6), `R2`(7), `SHARE_VIEW`(8), `OPTIONS_MENU`(9), `L3`(10), `R3`(11), `DPAD_UP`, `DPAD_DOWN`, `DPAD_LEFT`, `DPAD_RIGHT`(12 ~ 15), `HOME_PS`(16), `TOUCHPAD`(17) 입니다. 축 번호는 `GamepadAnalogStickCode` 의 `LEFT_X`(0), `LEFT_Y`(1, 아래가 +), `RIGHT_X`(2), `RIGHT_Y`(3) 입니다.

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

`input_actions.update` 와 `update_frame_snapshot` 은 엔진이 부르는 함수이고, 위 예제는 창 없이 흉내 내려고 불렀습니다. 엔진의 입력 동작은 씬에서 이렇게 씁니다.

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

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
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
			graphic.draw_rect(Rect::create(40.0 + index as f32 * 120.0, 40.0, 100.0, 40.0));
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

바깥 입력원은 `InputActionSource` 트레이트를 구현합니다. `read_action_value(action)` 하나는 꼭 구현하고(0 ~ 1, 0.5 이상이면 눌림), 이동 축(`read_move_axis`)과 마지막 장치로 적을 패드 종류(`read_gamepad_kind`)는 필요할 때 덮어씁니다. 원본은 `readActionValue` 를 가진 객체를 받지만 러스트 판은 트레이트이고, 빼기는 `add_source` 가 준 번호로 합니다.

## 게임패드 (GamepadManager)

`input_manager.get_gamepad_manager()` (떨기처럼 바꿀 때는 `get_gamepad_manager_mut()`) 로 얻습니다. 엔진이 처음 갱신할 때 패드 입력원을 열고 매 프레임 상태를 읽습니다. 데스크탑은 gilrs 가 운영체제 입력을 읽어 W3C 표준 배치(단추 0 ~ 16, 축 0 ~ 3)로 바꾸고, 웹은 원본처럼 `navigator.getGamepads()` 를 읽습니다. 패드는 `Gamepad.index` 번호로 묻습니다. (데스크탑은 처음 붙은 차례로 0 부터, 웹은 브라우저 번호)

| 함수 | 내용 |
| --- | --- |
| `get_all_connected_gamepads()`, `get_connected_gamepad_count()`, `get_connected_gamepad(index)` | 연결된 패드(`Gamepad`, 마지막 갱신의 사본) 목록과 수, 번호로 하나 |
| `is_button_pressed(gamepad_index, button_index)` | 단추가 눌려 있는지 |
| `is_button_just_pressed(gamepad_index, button_index)`, `is_button_just_released(...)` | 이번 갱신에 눌렸는지, 떼어졌는지 |
| `get_button_value(gamepad_index, button_index)` | 단추와 트리거 값 (0 ~ 1, 소수점 둘째 자리로 반올림) |
| `get_axis_value(gamepad_index, axis_index)` | 스틱 축 값 (-1 ~ 1, 죽은 구역은 빼지 않음) |
| `vibrate(strong_magnitude, weak_magnitude, seconds)` | 연결된 패드를 모두 떨기 (세기 0 ~ 1, 진동 장치가 없는 패드는 건너뜀) |
| `set_input_log_enabled(is_enabled)` | 단추와 축이 바뀔 때마다 로그를 찍을지 (처음 끔) |
| `set_log_enabled(is_enabled)` | 연결과 해제를 포함한 모든 로그의 큰 스위치 |
| `get_active_gamepad_id()` | 마지막 갱신에 단추를 누르거나 축을 크게 기울인 패드의 이름 (없으면 `None`) |

`read_gamepad_kind(gamepad_id)` 는 패드 이름으로 `GamepadKind::Xbox`, `PlayStation`, `Nintendo`, `Other` 를 가릅니다. 단추 안내 그림을 패드에 맞출 때 씁니다. `input_manager.get_last_gamepad_kind()` 는 마지막으로 쓴 패드의 종류를 줍니다.

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

`update_all_gamepads_with_list` 는 장치 없이 패드 목록을 넣는 길입니다. (시험이나 다른 입력원) 게임에서는 엔진이 패드를 읽습니다.

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

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		let gamepad_manager = engine.get_input_manager_mut().get_gamepad_manager_mut();
		let mut is_confirm_pressed = false;
		// The list is read in place when called (like the original navigator.getGamepads()), so borrow it with &mut and keep copies.
		let gamepad_list: Vec<Gamepad> = gamepad_manager.get_all_connected_gamepads().into_iter().cloned().collect();
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

## 마지막 입력 장치

`input_manager.get_last_input_device()` 는 마지막으로 쓴 장치를 `InputDevice::Keyboard`, `Mouse`, `Touch`, `Gamepad` 가운데 하나로 줍니다. 처음 값은 손가락이 주된 기기(웹의 CSS `pointer: coarse`)면 `Touch`, 아니면 `Keyboard` 이고, 데스크탑은 늘 `Keyboard` 로 시작합니다. 바깥 입력원이 넣은 패드 입력도 `Gamepad` 로 적힙니다. `is_touch_input()` 은 마지막 포인터 입력이 손가락인지 줍니다. 화면에 "탭하세요" 와 "Enter 를 누르세요" 를 바꿔 보일 때 씁니다.

## 마우스 커서 (PointerCursor)

`PointerCursor` 는 엔진이 하나 들고 `engine.get_pointer_cursor_mut()` 로 꺼냅니다. (원본의 `PointerCursor.getInstance()` 자리) `set_enabled(true)` 로 켜면 엔진이 프레임 끝마다 커서를 정합니다. 꺼져 있으면(처음) 아무것도 하지 않습니다. 데스크탑은 창의 커서(winit 의 커서 아이콘), 웹은 캔버스의 CSS 커서를 바꿉니다.

- `UIButton` 위(누르는 중 포함)에 마우스가 있으면 단추의 커서(처음 `"pointer"`), 아니면 `set_screen_cursor(name)` 로 정한 커서입니다.
- `set_cursor_names(button_cursor_name, default_cursor_name)` 로 단추 위 커서와 기본 커서를 바꿉니다.
- 쓸 수 있는 이름은 CSS 커서 이름 `pointer`, `crosshair`, `not-allowed`, `grab`, `grabbing`, `text`, `move`, `help`, `wait`, `progress`, `none`, `default` 입니다.
- `register_image_cursor(name, url, url_2x, hotspot_x, hotspot_y, fallback_cursor_name)` 로 그림 커서를 더하고 그 이름을 씁니다. (1 배, 2 배 그림, 1 배 기준 핫스팟, 그림을 못 쓸 때의 CSS 커서) 웹은 그림 주소를 CSS 값으로 쓰고, 데스크탑은 애셋 폴더 기준의 그림 파일로 창의 커서를 만듭니다. (만드는 동안과 그림을 못 읽으면 함께 준 CSS 커서)
- 손가락이 주된 기기에서는 커서를 건드리지 않습니다.

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

## 주의할 점

- 씬의 터치 함수와 `get_view_input_position()` 은 자리 하나만 봅니다. 핀치처럼 손가락 여럿이 필요하면 `get_touch_points()` 를 씁니다.
- 원본과 달리 `Scene` 을 바로 구현한 씬도 기본 `tick` 이 노드로 터치를 넘깁니다. (맨 위 노드에 바로 넘기고 스크롤 손짓은 없습니다) 스크롤 칸 안의 단추처럼 늦춘 누름이 필요하면 `TouchRecognizer` 를 쓰거나 `UIScene` 을 씁니다.
- 원본에서 공개 이름에 없던 키 표와 게임패드 번호 표가 러스트 판에는 `KeyCode`, `GamepadButtonCode`, `GamepadAnalogStickCode`, `GamepadTriggerCode` 상수로 있습니다. 값은 원본과 같은 `KeyboardEvent.code` 글과 W3C 표준 배치 번호라서 글이나 수로 바로 써도 됩니다.
- 웹은 키를 창(`window`)에서 받으므로 페이지에 초점이 있어야 들어옵니다. iframe 안의 게임은 iframe 을 한 번 눌러야 키를 받습니다.
- 창이 초점을 잃으면 엔진은 `input_manager.clear()` 로 키, 마우스 단추, 손가락, 입력 동작을 모두 비우고 터치를 뗀 것으로 넘깁니다. (데스크탑과 웹 같음) 원본은 키를 그대로 두었다가 페이지가 다시 보일 때 지웠습니다.
- 한 프레임짜리 상태(`is_key_just_pressed`, `is_mouse_button_just_pressed`, 휠 값, 떨어진 손가락, 터치 누름과 뗌)는 엔진이 프레임 끝에 지웁니다. 씬의 `tick` 이나 터치 함수 안에서 읽습니다.
- 데스크탑에서 게임패드는 첫 갱신 때 열립니다. 그 전에 부른 `vibrate` 는 떨지 않습니다. gilrs 는 장치의 단추 목록을 내주지 않아, 표준 배치 밖의 단추는 처음 누른 뒤에야 패드의 단추 목록에 생깁니다.

## 관련 타입

InputManager, KeyCode, MouseButton, TouchPoint, TouchPhase, InputDevice, InputActions, InputActionSource, GamepadManager, Gamepad, GamepadButton, GamepadButtonCode, GamepadAnalogStickCode, GamepadTriggerCode, GamepadKind, PointerCursor, TouchRecognizer, ComponentContext, SceneBase, WorldNode, UIScene
