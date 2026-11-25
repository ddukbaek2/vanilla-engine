# 원본 vanilla.js 에서 옮겨 오기

바닐라엔진은 vanilla.js 를 러스트로 옮긴 엔진입니다. 두 엔진은 기능 차이를 두지 않고, 공개 API 는 원본의 쓰임새를 따릅니다. 다른 것은 이름 꼴과, 러스트의 소유와 빌림 규칙에 맞춰 바꾼 몇 가지 꼴뿐입니다. 이 장에서는 vanilla.js 로 쓴 게임 코드를 바닐라엔진으로 옮길 때 무엇을 어떻게 바꾸는지 봅니다. 이름 바꾸기, 클래스 상속 대신 트레이트와 공통 상태 구조체, 노드 참조 대신 `NodeId`, 전역 엔진 대신 인자로 받는 엔진, 콜백과 `Rc<RefCell>`, `null` 대신 `Option`, 예외 대신 `Result`, 선택 인자 객체 대신 옵션 구조체, 두 엔진이 같이 쓰는 파일 차례입니다. 이 장은 러스트 판에만 있습니다.

## 이름 바꾸기

이름은 러스트 규칙(Rust API Guidelines C-CASE)을 따릅니다. 원본 이름을 이 규칙으로 바꾸면 러스트 판의 이름이 됩니다. 이름을 줄이지 않는 것은 원본과 같습니다.

| 갈래 | 원본 (vanilla.js) | 바닐라엔진 |
| --- | --- | --- |
| 함수, 메서드 | `setFillColor`, `getLocalPosition` | `set_fill_color`, `get_local_position` (snake_case) |
| 필드, 변수, 모듈 | `referenceResolutionSize`, `autoResizeOnWindowResize` | `reference_resolution_size`, `auto_resize_on_window_resize` |
| 타입, 트레이트 | `WorldNode`, `UIButton` | `WorldNode`, `UIButton` (UpperCamelCase, 그대로) |
| 열거 값 | `ButtonState.normal` | `ButtonState::Normal` (UpperCamelCase) |
| 상수, 연관 상수 | `Colors.white`, `Pivot.topLeft` | `Colors::WHITE`, `Pivot::TOP_LEFT` (SCREAMING_SNAKE_CASE) |
| 생성자 | `new Engine(configuration)`, `Vector2.create(1, 2)` | `Engine::new(engine_configuration)`, `Vector2::create(1.0, 2.0)` |
| 러스트 예약어 | `cooldown.use()`, `fullscreenPass.use()`, `shaderProgram.use()` | `cooldown.r#use()` 들 (같은 일을 하는 `activate`, `use_program` 도 있습니다) |
| 노드 객체를 돌려주던 함수 | `getParent()`, `getChildren()`, `getSiblings()` | `get_parent_id`, `get_child_ids`, `get_sibling_ids` (핸들을 돌려줍니다) |

```rust
use vanilla::{Colors, Cooldown, Pivot, Vector2};

assert_eq!(Colors::WHITE, "#ffffff");
assert_eq!(Pivot::TOP_LEFT, Vector2::create(0.0, 0.0));
assert_eq!(Pivot::MIDDLE_CENTER, Vector2::create(0.5, 0.5));

let mut cooldown = Cooldown::new(0.5);
assert!(cooldown.r#use());
assert!(!cooldown.r#use());
cooldown.tick(0.5);
assert!(cooldown.activate());
```

- 수는 `f32` 입니다. 소수점 없는 정수 글자(`640`)는 `f32` 자리에 쓸 수 없으니 `640.0` 으로 씁니다. JS 수의 글을 그대로 내야 하는 곳(`Format`, `LocalStorage`)만 `f64` 입니다.
- 색을 받는 함수는 대부분 `impl Into<Color>` 를 받아 `"#ffcc33"` 같은 글과 `Color::create(0.2, 0.25, 0.35, 1.0)` 을 함께 받습니다. 원본 `new Color(r, g, b, a)` 는 `Color::create(r, g, b, a)` 입니다.
- 파일과 문서 안의 이름은 바꾸지 않습니다. UI 문서(`.scene.json`)의 `contentSize` 같은 키, 반사 저장의 `LineWidth` 같은 키는 원본 그대로라 두 엔진이 같은 파일을 읽습니다.

## 첫 씬 옮기기

원본의 노드와 컴포넌트 장에 있는 씬입니다.

```javascript
import { Engine, EngineConfiguration, Scene, WorldNode, Paint, Vector2, Color } from "./build/vanilla.js";

class NodeScene extends Scene {
	initialize(engine) {
		super.initialize(engine);
		const root = this.getRoot();

		const panel = new WorldNode();
		panel.setName("Panel");
		panel.setContentSize(Vector2.create(400, 240));
		panel.setLocalPosition(Vector2.create(640, 360));
		const panelPaint = panel.addComponent(Paint);
		panelPaint.setColor(new Color(0.2, 0.25, 0.35, 1));
		panelPaint.setRoundSize(16);
		root.addChild(panel);

		const badge = new WorldNode();
		badge.setName("Badge");
		badge.setContentSize(Vector2.create(64, 64));
		badge.setLocalPosition(Vector2.create(400, 0));
		const badgePaint = badge.addComponent(Paint);
		badgePaint.setColor("#ffcc33");
		badgePaint.setRoundSize(32);
		panel.addChild(badge);
	}

	preDraw(graphic) {
		super.preDraw(graphic);
		const engine = this.getEngine();
		const viewManager = engine.getViewManager();
		viewManager.applyViewRect(graphic);
	}
}

const engineConfiguration = new EngineConfiguration();
engineConfiguration.referenceResolutionSize = Vector2.create(1280, 720);
engineConfiguration.autoResizeOnWindowResize = true;
const engine = new Engine(engineConfiguration);
engine.run(new NodeScene());
```

바닐라엔진으로 옮기면 이렇습니다.

```rust,no_run
use vanilla::{Color, Engine, EngineConfiguration, Graphic, Paint, Scene, SceneBase, Vector2};

struct NodeScene {
	scene_base: SceneBase,
}

impl Scene for NodeScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();

		let panel_id = node_tree.create_child(root_id, "Panel");
		let panel = &mut node_tree[panel_id];
		panel.set_content_size(Vector2::create(400.0, 240.0));
		panel.set_local_position(Vector2::create(640.0, 360.0));
		let panel_paint = panel.add_component(Paint::new());
		panel_paint.set_color(Color::create(0.2, 0.25, 0.35, 1.0));
		panel_paint.set_round_size(16.0);

		let badge_id = node_tree.create_child(panel_id, "Badge");
		let badge = &mut node_tree[badge_id];
		badge.set_content_size(Vector2::create(64.0, 64.0));
		badge.set_local_position(Vector2::create(400.0, 0.0));
		let badge_paint = badge.add_component(Paint::new());
		badge_paint.set_color("#ffcc33");
		badge_paint.set_round_size(32.0);
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		engine.get_view_manager().apply_view_rect(graphic);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let node_scene = NodeScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(node_scene)) {
		eprintln!("{}", error);
	}
}
```

- `class NodeScene extends Scene` 은 `SceneBase` 를 품은 구조체와 `impl Scene` 입니다. 반드시 쓰는 것은 공통 상태를 내주는 `get_scene_base`, `get_scene_base_mut` 둘입니다.
- 훅은 `&mut Engine` 을 인자로 받습니다. 원본 `this.getEngine()` 자리입니다.
- 원본 `super.initialize(engine)` 같은 상위 호출은 없습니다. 트레이트의 기본 구현이 비어 있기 때문입니다. 다만 `tick` 과 `draw` 는 기본 구현이 노드 트리를 갱신하고 그리므로, 덮어쓸 때는 원본 `super.tick`, `super.draw` 자리에 `self.scene_base.tick(engine, time_delta)`, `self.scene_base.draw(graphic)` 을 부릅니다.
- `new WorldNode()` 와 `root.addChild(panel)` 은 `node_tree.create_child(root_id, "Panel")` 한 줄입니다. 부모 없이 만들려면 `create_node(이름)` 뒤에 `add_child(부모, 자식)` 입니다.
- `addComponent(Paint)` 는 클래스를 받지만 `add_component(Paint::new())` 는 값을 받고, 붙인 컴포넌트의 `&mut` 를 돌려줍니다.
- `engine.run` 은 창이나 GL 을 만들지 못하면 `Err(까닭)` 을 돌려줍니다.
- 원본의 씬 `load` 단계는 `create` 에서 애셋을 맡기는 꼴입니다. (`async load` 는 "async/await" 장)

## 클래스 상속 대신 트레이트와 공통 상태 구조체

러스트에는 클래스 상속이 없습니다. 공통 상태는 구조체로 품고, 덮어쓰기는 트레이트 함수로 둡니다.

| 원본 | 바닐라엔진 |
| --- | --- |
| `class X extends Scene` | 필드 `scene_base: SceneBase` 와 `impl Scene for X` |
| `class X extends GameScene` | 필드 `game_scene_base: GameSceneBase`, `impl GameScene for X`, `vanilla::game_scene!(X);` |
| `class X extends UIScene` | 필드 `UISceneBase`, `impl UIScene for X`, `vanilla::ui_scene!(X);` |
| `class X extends Component` | 필드 `component_base: ComponentBase` 와 `impl Component for X` (`get_component_base`, `get_component_base_mut`, `get_component_type`) |
| `class X extends Asset` | 필드 `AssetBase` 와 `impl Asset for X` (`get_asset_base`, `get_asset_base_mut`) |
| `Node`, `ComponentNode`, `TransformNode`, `WorldNode` | `WorldNode` 하나. 원본에서 노드인 위젯과 `UINode` 는 컴포넌트입니다 |
| 위젯의 상속 (`UIButton extends UIControl`) | 상위 몫을 필드로 품고 함수를 넘깁니다. (`UIButton` 은 `UIControl` 을, `ShaderSprite` 는 `Sprite` 를 품습니다) 반사는 `#[vanilla::reflect(base = 필드)]` 로 잇습니다 |

`game_scene!`, `ui_scene!` 매크로는 그 씬에 `Scene` 을 지어 주어, 바탕이 `resize`, `layout`, `post_draw` 를 스스로 부르게 합니다.

컴포넌트를 옮기면 이렇습니다.

```javascript
class Spin extends Component {
	#degreesPerSecond;

	constructor() {
		super();
		this.#degreesPerSecond = 90;
	}

	setDegreesPerSecond(degreesPerSecond) {
		this.#degreesPerSecond = degreesPerSecond;
	}

	tick(timeDelta) {
		const node = this.getNode();
		const rotation = node.getLocalRotation();
		node.setLocalRotation(rotation + this.#degreesPerSecond * timeDelta);
	}
}
```

```rust
use vanilla::{Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, NodeTree};

struct Spin {
	component_base: ComponentBase,
	degrees_per_second: f32,
}

impl Spin {
	pub fn new() -> Spin {
		Spin {
			component_base: ComponentBase::new(),
			degrees_per_second: 90.0,
		}
	}

	pub fn set_degrees_per_second(&mut self, degrees_per_second: f32) {
		self.degrees_per_second = degrees_per_second;
	}
}

impl Component for Spin {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Spin"
	}

	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		let transform = context.get_transform_mut();
		let local_rotation = transform.get_local_rotation();
		transform.set_local_rotation(local_rotation + self.degrees_per_second * time_delta);
	}
}

let engine = Engine::new(EngineConfiguration::new());
let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let spinner_id = node_tree.create_child(root_id, "Spinner");
node_tree[spinner_id].add_component(Spin::new()).set_degrees_per_second(180.0);
node_tree.tick(&engine, 0.5);
assert_eq!(node_tree[spinner_id].get_local_rotation(), 90.0);
```

- 원본 `#` 비공개 필드는 구조체 필드(`pub` 없이)입니다.
- 원본 `this.getNode()` 자리는 훅이 받는 `ComponentContext` 입니다. 자기 트랜스폼은 `get_transform_mut`, 자기 노드는 `get_node_mut`, 트리 전체는 `get_node_tree_mut`, 엔진은 `get_engine` 으로 꺼냅니다.
- 같은 노드의 다른 컴포넌트는 `context.get_node_mut().get_component_mut::<T>()` 로 찾습니다. 부르고 있는 컴포넌트 자신은 그동안 노드에서 잠깐 빠져 있어 이 길로는 찾을 수 없습니다.
- `draw` 는 노드 대신 자기 트랜스폼(`&NodeTransform`)을 받습니다. 원본 `this.getNode().getContentSize()` 는 `transform.get_content_size()` 입니다. 그리는 동안 트리는 바뀌지 않습니다.
- 붙을 때 자식 노드를 만드는 컴포넌트(위젯 같은 것)는 `node_tree.add_component(node_id, 컴포넌트)` 로 붙여야 `attach_to_tree` 가 불립니다. 원본 `attach(node)` 자리입니다.

## 노드 참조 대신 NodeId

원본은 부모가 `children[]` 으로 자식 객체를 들고 자식이 `parent` 로 거꾸로 가리킵니다. 바닐라엔진은 씬의 `NodeTree` 가 모든 노드를 저장소 하나에 담고, 부모와 자식을 `NodeId` 로 잇습니다. `NodeId` 는 칸 번호와 세대 번호라서 지운 노드의 핸들은 다시 쓰이지 않습니다.

```javascript
const root = this.getRoot();
const panel = new WorldNode();
panel.setName("Panel");
root.addChild(panel);
const label = new WorldNode();
label.setName("Label");
panel.addChild(label);
console.log(label.getParent() === panel);
panel.removeChild(label);
root.addChild(label);
```

```rust
use vanilla::NodeTree;

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let panel_id = node_tree.create_child(root_id, "Panel");
let label_id = node_tree.create_child(panel_id, "Label");
assert_eq!(node_tree.get_parent_id(label_id), Some(panel_id));
assert_eq!(node_tree.get_child_ids(panel_id), [label_id]);

node_tree.remove_child(panel_id, label_id);
assert_eq!(node_tree.get_parent_id(label_id), None);
assert!(node_tree.add_child(root_id, label_id));

node_tree.destroy_node(panel_id);
assert!(node_tree.get_node(panel_id).is_none());
assert!(node_tree.get_node(label_id).is_some());
```

- 노드는 `node_tree[node_id]` 로 꺼내 설정합니다. 지운 노드의 핸들을 이렇게 꺼내면 멈추므로, 있을지 모를 때는 `get_node`, `get_node_mut` 이 돌려주는 `Option` 을 봅니다.
- 원본에서 노드 객체를 필드에 들던 자리는 `NodeId` 를 듭니다. `NodeId` 는 `Copy` 라 마음대로 복사하고 콜백에 넘깁니다. 아직 만들지 않은 노드는 `Option<NodeId>` 로 둡니다.
- `remove_child`, `remove_from_parent` 는 떼어 내기만 하고, `destroy_node` 는 자손까지 지웁니다. `tick` 이나 터치를 도는 중에 부르면 비활성으로 두었다가 다 돈 뒤에 지웁니다.
- 원본 `getParent`, `getChildren`, `getSiblings` 는 핸들을 돌려주는 `get_parent_id`, `get_child_ids`, `get_sibling_ids` 입니다. 찾기(`find_child_by_name`, `find_descendant_by_name` 같은 것)도 핸들을 돌려줍니다.
- 씬 코드가 컴포넌트를 쥔 동안에는 같은 트리의 다른 노드를 함께 빌릴 수 없습니다. 둘을 함께 만질 때는 `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)` 를 쓰고, 자주 쓰는 것은 트리를 받는 `*_in_tree` 함수(`UIWindow::close_in_tree`, `ParticleSystem::emit_in_tree` 같은 것)로 따로 있습니다.

## 전역 엔진 대신 인자로 받는 엔진

원본은 `Engine.getEngine()`, `this.getEngine()` 으로 어디서든 엔진을 꺼냅니다. 바닐라엔진에는 전역 엔진이 없습니다. 엔진을 쓰는 곳은 인자로 드러납니다.

| 자리 | 엔진을 받는 길 |
| --- | --- |
| 씬의 훅 (`create`, `initialize`, `tick`, `pre_draw` 같은 것) | 인자 `engine: &mut Engine` |
| 컴포넌트의 `tick`, 터치, 휠 | `context.get_engine()` (`&Engine`) |
| 콜백 (`UIButtonEvent` 같은 것) | 콜백이 받는 `ComponentContext` |
| async 작업 | `engine_context.with(\|engine\| ...)`, `scene_context.with(\|scene, engine\| ...)` |

그래서 인스펙터의 게터 목록에 원본 `Component.getEngine` 이 없고, 반사 속성 목록에 `Engine` 이 없습니다.

## 콜백과 Rc<RefCell>

원본의 화살표 함수는 `this` 와 바깥 변수를 마음대로 붙잡습니다. 바닐라엔진의 콜백은 `Box<dyn FnMut(...)>` 로 받아 컴포넌트가 들고 있으므로, 씬의 `&mut self` 같은 빌린 값을 붙잡을 수 없습니다. 다른 노드는 `NodeId` 를 붙잡아 콜백이 받는 문맥으로 찾고, 씬과 나눠 써야 하는 값은 `Rc<Cell>` 이나 `Rc<RefCell>` 에 담습니다.

```javascript
let clickCount = 0;
const closeButton = close.addComponent(UIButton);
closeButton.setClickEvent(() => {
	panel.setActive(false);
	clickCount += 1;
});
```

```rust
use std::cell::Cell;
use std::rc::Rc;
use vanilla::{ComponentContext, Engine, EngineConfiguration, NodeTree, TouchKind, UIButton, Vector2};

let engine = Engine::new(EngineConfiguration::new());
let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let panel_id = node_tree.create_child(root_id, "Panel");
let close_id = node_tree.create_child(root_id, "Close");
node_tree[close_id].set_content_size(Vector2::create(40.0, 40.0));

let click_count = Rc::new(Cell::new(0));
let event_click_count = click_count.clone();
let close_button = node_tree.add_component(close_id, UIButton::new()).expect("button");
close_button.set_click_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
	context.get_node_tree_mut()[panel_id].set_active(false);
	event_click_count.set(event_click_count.get() + 1);
}));

node_tree.dispatch_touch(close_id, &engine, TouchKind::Press, Vector2::zero());
node_tree.dispatch_touch(close_id, &engine, TouchKind::Release, Vector2::zero());
assert!(!node_tree[panel_id].is_active());
assert_eq!(click_count.get(), 1);
```

- 원본 콜백이 받던 버튼은 그대로 첫 인자로 오고, 둘째 인자 `ComponentContext` 로 트리와 엔진에 닿습니다.
- 콜백 게터(`UIButton::get_click_event` 같은 것)와 갈아 끼운 저장소(`LocalStorage::get_backend`, `PersistedStore::get_storage_backend`)는 `Rc<RefCell<...>>` 로 돌려줍니다. 부르는 쪽이 `borrow_mut` 로 빌려 씁니다. 엔진과 부르는 쪽이 같은 함수를 나눠 쥐고, 부르는 동안 다시 부르는 것을 막습니다.
- 씬 구조체의 값을 콜백에서 바꾸고 싶으면 그 값을 `Rc<Cell<T>>` 나 `Rc<RefCell<T>>` 필드로 두고 복제본을 콜백에 넘깁니다.

## null 대신 Option

원본에서 `null` 이나 `undefined` 를 돌려주던 함수는 `Option` 을 돌려줍니다. 받는 쪽은 `if let`, `match` 로 가립니다.

```rust
use vanilla::{NodeTree, Paint, Text};

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let panel_id = node_tree.create_child(root_id, "Panel");
node_tree[panel_id].add_component(Paint::new());

if let Some(paint) = node_tree[panel_id].get_component_mut::<Paint>() {
	paint.set_color("#ffcc33");
}
assert!(node_tree[panel_id].get_component::<Text>().is_none());
assert_eq!(node_tree.find_child_by_name(root_id, "Panel"), Some(panel_id));
assert_eq!(node_tree.find_child_by_name(root_id, "Missing"), None);
```

- 원본 `getComponent(Paint)` 가 `null` 이던 자리는 `get_component::<Paint>()` 의 `None` 입니다.
- 원본에서 `null` 을 받던 세터는 `Option` 을 받습니다. 원본 `sprite.setImage(null)` 은 `sprite.set_image(None)` 입니다. 반사 값에서는 `None` 이 원본 `null` 자리인 `ReflectValue::Null` 입니다.

## 예외 대신 Result

러스트는 예외를 던지지 않습니다. 실패할 수 있는 함수는 `Result<T, String>` 을 돌려주고 `Err` 에 까닭을 담습니다. 원본 `try`/`catch` 자리는 `match` 이고, 같은 꼴의 함수 안에서는 `?` 로 넘깁니다.

```rust
use vanilla::{NodeTree, Timeline, UIDocument};

fn count_document_children(json_text: &str) -> Result<usize, String> {
	let mut document = UIDocument::new();
	let mut node_tree = NodeTree::new();
	let root_id = document.from_json_text(&mut node_tree, json_text)?;
	Ok(node_tree.get_child_count(root_id))
}

assert!(count_document_children("{").is_err());
match Timeline::load_from_text("not json") {
	Ok(_timeline) => {
		panic!("unexpected");
	}
	Err(reason) => {
		assert!(!reason.is_empty());
	}
}
```

- async 작업 안에서 원본 `await` 가 던지던 자리는 손잡이의 `await` 가 돌려주는 `Result` 입니다. 원본 `reject` 는 `Promise<Result<T, E>>` 의 `reject(까닭)` 입니다.
- 셰이더를 짓지 못하는 것처럼 한 효과만 실패하는 일은 한 번 알리고 건너뜁니다. 프레임은 멈추지 않습니다.
- 실패한 애셋도 끝난 것으로 쳐서 씬이 멈추지 않는 것은 원본과 같습니다.

## 선택 인자 객체 대신 옵션 구조체

원본은 `{ decayRate: 9, frequency: 42 }` 같은 객체로 선택 인자를 받습니다. 러스트에는 이름 붙은 선택 인자가 없어 기본값을 가진 옵션 구조체를 받습니다. 바꿀 것만 적고 나머지는 `..기본값` 으로 채웁니다.

```javascript
const shaker = new Shaker({ decayRate: 10 });
shaker.addShake(8);
```

```rust
use vanilla::{Shaker, ShakerOptions};

let mut shaker = Shaker::new(ShakerOptions {
	decay_rate: 10.0,
	..ShakerOptions::default()
});
shaker.add_shake(8.0);
assert!(shaker.is_shaking());
```

`ToonDrawOptions`, `RenderTargetOptions`, `UIConfirmWindowOptions`, `FloatingTextOptions` 같은 옵션 구조체가 모두 이 꼴입니다. 엔진이 필드를 더해도 `..Default::default()` 로 지은 코드는 그대로 돕니다.

## async/await 와 반사

- 원본 `async load`, `await asset.load(경로)`, `await Wait.seconds(1)`, `fadeScreen(async () => ...)` 는 같은 꼴의 러스트 `async`/`await` 로 옮깁니다. 다른 점은 `await` 다음 줄에서 `this` 를 바로 쓰지 않고 `scene_context.with(|scene, engine| ...)` 안에서 만지는 것입니다. 자세한 것은 "async/await" 장에 있습니다.
- 원본은 실행 중에 모든 객체의 `get`, `is`, `set` 함수를 훑습니다. 바닐라엔진은 컴포넌트의 `impl` 블록과 `impl Component` 블록에 `#[vanilla::reflect]` 를 붙인 타입만 반사됩니다. UI 문서에 저장하거나 개발자 도구 인스펙터에 보일 게임 컴포넌트는 매크로를 붙입니다. 자세한 것은 "반사 매크로 #[reflect]" 장에 있습니다.

## 두 엔진이 같이 쓰는 파일

애셋 파일과 저작 도구가 저장한 파일은 두 엔진이 같은 꼴로 읽고 씁니다. 옮길 때 파일은 그대로 둡니다.

| 파일 | 원본에서 짓는 곳 | 바닐라엔진에서 읽는 곳 |
| --- | --- | --- |
| UI 문서 `.scene.json` | UI 에디터(`tools/uieditor`), `UIDocument.toJsonText` | `UIDocument::from_json_text`, `to_json_text` (키 차례, 탭 들여쓰기, 수의 글꼴까지 원본이 낸 글과 같습니다). 에디터가 등록하는 종류는 `register_editor_types` 한 줄 |
| 타임라인 문서 `.timeline.json` | 타임라인 편집기(`tools/timelineeditor`) | `Timeline::load_from_text`, `Timeline::load_from_url` |
| 스프라이트 아틀라스 JSON 과 쪽 그림 | `tools/atlas.cjs` | `SpriteAtlas::load` (원본과 같은 두 꼴) |
| 언어 묶음 `languages.json` 과 언어 파일 | 게임의 애셋 | `languagepack` (`load_language_list`, `load_language`) |
| UI 글꼴 목록 `fonts.json`, `fonts-<묶음>.json` | 게임의 애셋 | `uifont` (`load_ui_font`) |
| 빌드 정보 `version.json` | 배포 빌드 | `BuildInfo::load` |

- 애셋 경로는 원본과 같은 상대 경로입니다. 데스크탑은 `EngineConfiguration.asset_directory_paths` 의 폴더에서 바로 읽고, 웹은 원본처럼 `asset_base_url` 과 경로를 이어 받습니다. 게임 코드는 같습니다.
- 저작 도구와 파일 꼴은 "저작 도구와 두 엔진이 같이 쓰는 파일" 장에 있습니다.

## 옮기지 않은 원본 파일

원본 `src/` 의 파일 가운데 러스트 판에 같은 이름의 파일이 없는 것은 다른 파일에 합쳤거나 일부러 옮기지 않은 것입니다. (자세한 목록은 저장소의 ARCHITECTURE.md 9장)

| 원본 | 바닐라엔진 |
| --- | --- |
| `base/node.js`, `core/node/componentnode.js`, `core/node/transformnode.js` | `WorldNode` 와 `NodeTree` 에 합침 |
| `core/scenemanager.js` | `Engine` 의 `load_scene`, `unload_scene`, `get_all_loaded_scenes` (씬 객체 대신 `SceneId`) |
| `core/touchraycaster.js` | `SceneBase::tick_touch`, `NodeTree` 의 누름 판정, `TouchRecognizer` 에 합침 |
| `resource/jsonasset.js` | `TextAsset` 과 같은 파일의 `JsonAsset` |
| `base/object.js`, `base/singleton.js`, `experimental/collection` 의 `Dictionary`, `List`, `Queue`, `Set`, `Stack` | 러스트 언어와 표준(트레이트, `Any`, `enum`, `Vec`, `HashMap` 같은 것)이 같은 일을 합니다 |
| `base/reflection.js` | 실행 중 반사는 `#[vanilla::reflect]`, `createGUID` 는 `create_guid` |
| `experimental/camera.js` | 회전과 `begin`/`end` 까지 `Camera2D` 가 덮습니다 |
| 브라우저가 하던 일 (캔버스 2D, `window.alert`, `confirm`, `prompt`, html lang 과 meta) | `Canvas2D`, `Engine::alert`, `confirm`, `prompt` (와 `_async`), `Document` |

## 관련 타입

- `Scene`, `SceneBase`, `GameScene`, `GameSceneBase`, `UIScene`, `UISceneBase`, `game_scene!`, `ui_scene!`
- `Component`, `ComponentBase`, `ComponentContext`, `Asset`, `AssetBase`
- `NodeTree`, `NodeId`, `WorldNode`, `NodeTransform`
- `Engine`, `EngineConfiguration`, `ViewManager`
- `UIButtonEvent`, `Shaker`, `ShakerOptions`, `Cooldown`, `Colors`, `Color`, `Pivot`
- `UIDocument`, `Timeline`, `SpriteAtlas`, `BuildInfo`, `languagepack`, `uifont`
