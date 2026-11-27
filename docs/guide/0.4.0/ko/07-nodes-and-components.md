# 노드와 컴포넌트

씬에 보이는 것은 노드 트리로 짓습니다. 노드는 자리와 계층을 갖고, 그리기나 움직임 같은 기능은 노드에 붙이는 컴포넌트가 맡습니다. 이 장에서는 노드 트리의 구조, 노드 만들고 붙이기, 자리와 변환, 컴포넌트 붙이기와 찾기, 직접 만드는 컴포넌트, 트리 돌기를 봅니다.

## 구조

```text
SceneBase
└── NodeTree
    └── WorldNode (NodeId)
        ├── NodeTransform
        └── Vec<Box<dyn Component>>
Component + ComponentBase
```

- `NodeTree` 는 씬의 노드 저장소입니다. 노드를 `NodeId` 로 가리킵니다.
- `WorldNode` 는 이름, 활성, 터치 받기, 부모와 자식의 `NodeId`, 컴포넌트 목록을 듭니다.
- `NodeTransform` 은 로컬 자리, 회전, 크기, 투명도, 기준점(pivot), 앵커, 내용 크기, 보이기를 듭니다.
- 노드는 `WorldNode` 한 가지입니다. 원본의 `Node`, `ComponentNode`, `TransformNode`, `WorldNode` 네 단계 상속을 하나로 합쳤습니다.
- 노드는 씬의 `NodeTree` 가 저장소 하나에 담고, 부모와 자식은 `NodeId` 로 잇습니다. 게임 코드는 `NodeId` 를 들고 다니다가 `node_tree[node_id]` 나 `get_node`, `get_node_mut` 로 노드를 꺼냅니다. `NodeId` 는 칸 번호와 세대 번호라서, 지운 노드의 `NodeId` 는 `get_node` 가 `None` 을 돌려줍니다.
- 씬의 노드 트리는 `scene_base.get_node_tree_mut()`, 루트 노드는 `scene_base.get_root_id()` 로 얻습니다.
- 원본의 `UINode` 는 따로 된 노드 갈래지만, 러스트 판의 `UINode` 는 `WorldNode` 에 붙는 컴포넌트입니다. 제약으로 자리를 정하는 방식은 "UI" 장에서 봅니다.
- `Component` 는 트레이트입니다. 켜짐과 기즈모 표시 같은 공통 상태는 `ComponentBase` 에 두고, 구현하는 타입이 필드로 들고 돌려줍니다.

## 노드 만들고 붙이기

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

	fn initialize(&mut self, _engine: &Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();

		let panel_id = node_tree.create_child(root_id, "Panel");
		let panel_node = &mut node_tree[panel_id];
		panel_node.set_content_size(Vector2::create(400.0, 240.0));
		panel_node.set_local_position(Vector2::create(640.0, 360.0));
		let panel_paint = panel_node.add_component(Paint::create_with_color(Color::create(0.2, 0.25, 0.35, 1.0)));
		panel_paint.set_round_size(16.0);

		let badge_id = node_tree.create_node("Badge");
		let badge_node = &mut node_tree[badge_id];
		badge_node.set_content_size(Vector2::create(64.0, 64.0));
		badge_node.set_local_position(Vector2::create(400.0, 0.0));
		let badge_paint = badge_node.add_component(Paint::create_with_color("#ffcc33"));
		badge_paint.set_round_size(32.0);
		node_tree.add_child(panel_id, badge_id);
	}

	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
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

- `create_child(parent_id, name)` 는 노드를 만들어 그 부모의 자식 목록 맨 뒤에 붙이고 `NodeId` 를 돌려줍니다.
- `create_node(name)` 은 어디에도 붙지 않은 노드를 만듭니다. `add_child(parent_id, child_id)` 로 붙여야 그려집니다. 다른 부모에 붙어 있던 노드면 거기서 떼어 옮깁니다.
- `add_child` 와 `insert_child(parent_id, child_id, index)` 는 붙였으면 `true` 를 돌려줍니다. 둘 가운데 하나가 없는 노드거나, 루트를 붙이려 하거나, 자기 자신이나 자기 조상 밑으로 붙이려 하면 `false` 입니다.
- `remove_child(parent_id, child_id)`, `remove_from_parent(child_id)` 는 떼어 내기만 하고 노드를 없애지 않습니다. 떼어 낸 노드는 다시 붙일 수 있습니다.
- `destroy_node(node_id)` 는 노드를 자손까지 지웁니다. 루트는 지우지 않습니다.
- `pre_draw` 의 `draw_scene_background` 는 캔버스를 씬 배경색으로 칠하고 뷰 좌표계를 겁니다. 노드 트리는 기본 `draw` 가 그립니다.
- 새 노드는 자리 (0, 0), 크기 배율 1, 회전 0, 투명도 1, 활성, 보이기 켬, 터치 받지 않음입니다. 내용 크기는 (0, 0), 기준점과 앵커는 가운데 (0.5, 0.5) 입니다.

## 노드의 자리

로컬 자리는 "부모의 내용 영역 왼쪽 위에서 잰, 이 노드 기준점의 자리" 입니다. 기준점은 내용 영역 안의 비율(0 ~ 1)이라 처음 값 (0.5, 0.5) 이면 노드의 가운데가 로컬 자리에 옵니다. 위 예제에서 `Badge` 는 `Panel` 의 오른쪽 위 모서리(400, 0)에 가운데가 놓입니다.

| 함수 | 내용 |
| --- | --- |
| `set_content_size(size)`, `set_content_size_xy(width, height)` | 내용 크기. `Paint` 와 `Sprite` 는 이 크기만큼 칠합니다 |
| `set_pivot(pivot)` | 기준점. `Pivot::TOP_LEFT`, `Pivot::MIDDLE_CENTER`, `Pivot::BOTTOM_RIGHT` 같은 아홉 상수가 있습니다 |
| `set_anchor(anchor)` | 앵커. 그리기에는 쓰이지 않고 앵커 기준 자리를 셀 때만 씁니다 |
| `set_anchored_position(position, parent_content_size)` | `NodeTransform` 의 함수. 부모 내용 영역 안의 앵커 점(부모 크기 x 앵커)에서 잰 자리로 로컬 자리를 정합니다 |
| `set_rect(x, y, width, height)` | 왼쪽 위가 (x, y) 에 오도록 내용 크기와 로컬 자리를 함께 정합니다 (기준점이 어디든) |

```rust
use vanilla::{NodeTree, Pivot, Vector2};

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let parent_id = node_tree.create_child(root_id, "parent");
node_tree[parent_id].set_pivot(Pivot::TOP_LEFT);
node_tree[parent_id].set_content_size(Vector2::create(800.0, 600.0));
let parent_content_size = node_tree[parent_id].get_content_size();

let corner_id = node_tree.create_child(parent_id, "corner");
let corner_transform = node_tree[corner_id].get_transform_mut();
corner_transform.set_anchor(Pivot::BOTTOM_RIGHT);
corner_transform.set_content_size(Vector2::create(100.0, 50.0));
corner_transform.set_anchored_position(Vector2::create(-60.0, -35.0), parent_content_size);
assert_eq!(node_tree[corner_id].get_local_position(), Vector2::create(740.0, 565.0));

let label_id = node_tree.create_child(parent_id, "label");
node_tree[label_id].set_pivot(Pivot::TOP_LEFT);
node_tree[label_id].set_rect(20.0, 20.0, 300.0, 40.0);
assert_eq!(node_tree[label_id].get_local_position(), Vector2::create(20.0, 20.0));

let badge_id = node_tree.create_child(parent_id, "badge");
node_tree[badge_id].set_rect(20.0, 20.0, 300.0, 40.0);
assert_eq!(node_tree[badge_id].get_local_position(), Vector2::create(170.0, 40.0));
```

`corner` 는 (800 x 1 - 60, 600 x 1 - 35) 인 (740, 565) 에 놓입니다. `label` 은 기준점이 왼쪽 위라 (20, 20) 이고, 기준점이 가운데인 `badge` 는 같은 사각형이라도 (20 + 300 x 0.5, 20 + 40 x 0.5) 인 (170, 40) 입니다.

`WorldNode` 의 자리 함수들은 `NodeTransform` 의 같은 이름 함수로 가는 바로가기입니다. 앵커 기준 자리처럼 바로가기가 없는 것은 `get_transform_mut()` 로 `NodeTransform` 을 꺼내 부릅니다. 앵커 기준 자리는 부모의 내용 크기를 알아야 하는데 노드는 부모를 들고 있지 않으므로 부모 크기를 인자로 받습니다.

## 변환

| 로컬 (`WorldNode`, 부모 기준) | 전체 (`NodeTree`, 루트 기준) | 내용 |
| --- | --- | --- |
| `set_local_position`, `set_local_position_xy`, `get_local_position` | `set_position(node_id, position)`, `get_position(node_id)` | 자리 |
| `set_local_rotation`, `get_local_rotation` | `set_rotation(node_id, rotation)`, `get_rotation(node_id)` | 회전 (도, 시계 방향) |
| `set_local_scale`, `set_local_scale_xy`, `get_local_scale` | `set_scale(node_id, scale)`, `get_scale(node_id)` | 크기 배율 |
| `set_local_opacity`, `get_local_opacity` | `get_opacity(node_id)` | 투명도 (0 ~ 1 로 자릅니다, 자식에게 곱해 내려갑니다) |

전체 값은 부모를 따라 올라가야 하므로 `NodeTree` 가 `NodeId` 를 받아 셉니다. 전체 자리는 기준점의 자리이고, 부모 기준점에서 부모의 전체 회전과 크기를 걸어 셉니다.

그릴 때 노드는 로컬 자리로 옮기고, 회전하고, 크기를 곱한 뒤, 기준점만큼(내용 크기 x 기준점) 되돌아가 내용 영역의 왼쪽 위를 원점으로 컴포넌트를 그립니다. 자식은 그 원점에서 다시 같은 차례를 밟습니다. 그래서 부모를 돌리고 키우면 자식도 함께 돌고 커집니다.

화면 쪽 판정도 `NodeTree` 가 줍니다.

- `get_world_corners(node_id)`: 부모들의 자리, 회전, 크기를 모두 거친 네 귀퉁이 (왼쪽 위, 오른쪽 위, 오른쪽 아래, 왼쪽 아래)
- `get_world_bounds(node_id)`: 그 네 점을 감싸는 축 정렬 사각형
- `convert_view_to_local(node_id, view_position)`: 뷰 좌표를 이 노드의 내용 영역 좌표로 (왼쪽 위가 0, 0). 크기 배율이 0 이라 되돌릴 수 없으면 `None`
- `convert_node_position_to_view_position(node_id, node_position)`: 거꾸로, 내용 영역 좌표를 뷰 좌표로
- `calculate_world_matrix(node_id)` 와 `WorldNode::contains_point(&world_matrix, view_position)`: 뷰 좌표의 점이 노드 안인지 (돌아간 노드도 맞게)

```rust
use vanilla::{NodeTree, Pivot, Rect, Vector2};

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let parent_id = node_tree.create_child(root_id, "parent");
node_tree[parent_id].set_pivot(Pivot::TOP_LEFT);
node_tree[parent_id].set_content_size(Vector2::create(200.0, 100.0));
node_tree[parent_id].set_local_position(Vector2::create(100.0, 50.0));
node_tree[parent_id].set_local_scale(Vector2::create(2.0, 2.0));
node_tree[parent_id].set_local_opacity(0.5);

let child_id = node_tree.create_child(parent_id, "child");
node_tree[child_id].set_content_size(Vector2::create(40.0, 40.0));
node_tree[child_id].set_local_position(Vector2::create(10.0, 20.0));
node_tree[child_id].set_local_opacity(0.5);

assert_eq!(node_tree.get_position(child_id), Vector2::create(120.0, 90.0));
assert_eq!(node_tree.get_scale(child_id), Vector2::create(2.0, 2.0));
assert_eq!(node_tree.get_opacity(child_id), 0.25);

let world_corners = node_tree.get_world_corners(child_id);
assert_eq!(world_corners[0], Vector2::create(80.0, 50.0));
assert_eq!(world_corners[2], Vector2::create(160.0, 130.0));
assert_eq!(node_tree.get_world_bounds(child_id), Rect::create(80.0, 50.0, 80.0, 80.0));
assert_eq!(node_tree.convert_view_to_local(child_id, Vector2::create(80.0, 50.0)), Some(Vector2::create(0.0, 0.0)));

let world_matrix = node_tree.calculate_world_matrix(child_id);
assert!(node_tree[child_id].contains_point(&world_matrix, Vector2::create(100.0, 100.0)));
assert!(!node_tree[child_id].contains_point(&world_matrix, Vector2::create(170.0, 100.0)));

node_tree.set_position(child_id, Vector2::create(300.0, 250.0));
assert_eq!(node_tree[child_id].get_local_position(), Vector2::create(100.0, 100.0));

node_tree[parent_id].set_local_rotation(30.0);
node_tree[child_id].set_local_rotation(15.0);
assert_eq!(node_tree.get_rotation(child_id), 45.0);
```

부모가 두 배라서 자식의 로컬 자리 (10, 20) 은 부모 원점 (100, 50) 에서 (20, 40) 떨어진 (120, 90) 이 됩니다. 40 x 40 인 자식은 화면에서 80 x 80 으로 덮입니다. `set_position` 은 그 거꾸로 셈해 로컬 자리를 정합니다.

## 활성, 보이기, 그리는 차례

- `set_active(false)` 인 노드와 그 자손은 갱신(`tick`)도, 그리기도, 터치 판정도 받지 않습니다. `NodeTree::is_active_in_hierarchy(node_id)` 는 루트까지 모두 활성인지 봅니다.
- `set_visible(false)` 인 노드와 그 자손은 그리지 않고 터치도 받지 않지만 갱신은 받습니다. 원본에 없는 플래그입니다. `NodeTree::is_visible_in_hierarchy(node_id)` 는 루트까지 모두 활성이고 보이기가 켜져 있고 투명도가 0 보다 큰지 봅니다.
- 투명도는 그릴 때 곱해질 뿐이라 투명도가 0 인 노드도 그리기 차례를 돌고 터치 판정도 받습니다. 그리기와 터치를 함께 끄려면 `set_visible(false)` 를 씁니다.
- 자식은 자식 목록 차례로 그립니다. 뒤에 있을수록 위에 보입니다. `NodeTree::set_sibling_index(node_id, index)` 로 형제 사이의 차례를 바꾸고 `get_sibling_index(node_id)` 로 읽습니다. 자리가 자식 수를 넘으면 맨 뒤로 갑니다.

```rust
use vanilla::NodeTree;

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let layer_id = node_tree.create_child(root_id, "layer");
let back_id = node_tree.create_child(layer_id, "back");
let front_id = node_tree.create_child(layer_id, "front");

node_tree[layer_id].set_active(false);
assert!(node_tree[front_id].is_active());
assert!(!node_tree.is_active_in_hierarchy(front_id));
node_tree[layer_id].set_active(true);

node_tree[back_id].set_visible(false);
assert!(!node_tree.is_visible_in_hierarchy(back_id));
assert!(node_tree.is_visible_in_hierarchy(front_id));

node_tree.set_sibling_index(front_id, 0);
assert_eq!(node_tree.get_child_ids(layer_id), &[front_id, back_id]);
node_tree.set_sibling_index(front_id, 100);
assert_eq!(node_tree.get_sibling_index(front_id), Some(1));
```

## 컴포넌트 붙이기와 찾기

| 함수 | 내용 |
| --- | --- |
| `add_component(component)` | 값을 받아 붙이고 붙인 컴포넌트(`&mut T`)를 돌려줍니다 |
| `get_or_add_component::<T>()` | 있으면 그것, 없으면 `T::default()` 로 새로 붙인 것 |
| `get_component::<T>()`, `get_component_mut::<T>()` | 그 타입의 첫 컴포넌트, 없으면 `None` |
| `get_components::<T>()` | 그 타입의 컴포넌트 모두 (붙인 차례) |
| `has_component::<T>()` | 있는지 |
| `get_all_components()` | 붙은 차례대로 전부 (`Box<dyn Component>` 목록) |
| `remove_component::<T>()` | 그 타입을 모두 떼어 내고 뗀 수를 돌려줍니다 |

```rust
use vanilla::{Color, Paint, Text, WorldNode};

let mut node = WorldNode::new("card");
let paint = node.add_component(Paint::create_with_color(Color::white()));
paint.set_round_size(12.0);
node.add_component(Text::create_with_text("Card", 24.0));

assert!(node.has_component::<Paint>());
assert_eq!(node.get_component::<Paint>().expect("paint").get_round_size(), 12.0);
node.get_component_mut::<Text>().expect("text").set_text("Ace");
assert_eq!(node.get_component::<Text>().expect("text").get_text(), "Ace");
assert_eq!(node.get_all_components().len(), 2);

node.get_or_add_component::<Paint>().set_round_size(4.0);
assert_eq!(node.get_components::<Paint>().len(), 1);

assert_eq!(node.remove_component::<Paint>(), 1);
assert!(!node.has_component::<Paint>());
```

컴포넌트는 `require()` 로 함께 있어야 할 컴포넌트를 알립니다. `RequiredComponent::of::<T>()` 목록을 돌려주면 `WorldNode::add_component` 가 붙이기 직전에 같은 노드에 그 타입이 없을 때만 `T::default()` 로 만들어 붙입니다. 붙일 때 차례는 자기 자리 잡기, 요구 컴포넌트 붙이기, `attach(transform)` 입니다. 떼어 낼 때는 목록에서 빼고 `detach(transform)` 를 부릅니다.

```rust
use vanilla::core::component::RequiredComponent;
use vanilla::{Component, ComponentBase, Paint, WorldNode};

#[derive(Default)]
struct Badge {
	component_base: ComponentBase,
}

#[vanilla::reflect]
impl Badge {}

#[vanilla::reflect]
impl Component for Badge {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Badge"
	}

	fn require(&self) -> Vec<RequiredComponent> {
		vec![RequiredComponent::of::<Paint>()]
	}
}

let mut node = WorldNode::new("badge");
node.add_component(Badge::default());
assert!(node.has_component::<Paint>());
node.add_component(Badge::default());
assert_eq!(node.get_components::<Paint>().len(), 1);
```

### 트리를 함께 쓰는 컴포넌트

- 붙을 때 자식 노드를 만드는 컴포넌트(`UIView` 같은 것)는 `node_tree.add_component(node_id, component)` 로 붙입니다. `attach` 다음에 `attach_to_tree(node_tree, node_id)` 가 불려 그 안에서 노드를 만들 수 있습니다. 이 길은 `require()` 를 붙이지 않습니다. 노드가 없으면 `None` 을 돌려줍니다.
- 씬 코드에서 컴포넌트와 트리를 함께 고칠 때는 `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)` 를 씁니다. 그 노드의 첫 `T` 를 잠깐 꺼내 트리와 함께 넘깁니다. 위젯처럼 다른 노드도 만지는 컴포넌트가 이 길을 씁니다.

```rust
use vanilla::{Component, ComponentBase, NodeId, NodeTree};

struct Labeled {
	component_base: ComponentBase,
	label_id: Option<NodeId>,
}

#[vanilla::reflect]
impl Component for Labeled {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Labeled"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.label_id = Some(node_tree.create_child(node_id, "label"));
	}
}

#[vanilla::reflect]
impl Labeled {
	fn rename_label(&self, node_tree: &mut NodeTree, name: &str) {
		if let Some(label_id) = self.label_id {
			node_tree[label_id].set_name(name);
		}
	}
}

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let node_id = node_tree.create_child(root_id, "button");
node_tree.add_component(node_id, Labeled {
	component_base: ComponentBase::new(),
	label_id: None,
});
node_tree.with_component_mut::<Labeled, _>(node_id, |labeled, tree| {
	labeled.rename_label(tree, "caption");
});
assert!(node_tree.find_child_by_name(node_id, "caption").is_some());
```

## 컴포넌트 만들기

`Component` 트레이트를 구현합니다. `get_component_base`, `get_component_base_mut`, `get_component_type` 셋은 꼭 구현하고, 나머지는 필요한 것만 덮어씁니다.

| 덮어쓰는 함수 | 불리는 때 |
| --- | --- |
| `attach(transform)`, `detach(transform)` | 노드에 붙을 때, 떨어질 때 |
| `attach_to_tree(node_tree, node_id)` | `NodeTree::add_component` 로 붙을 때 (`attach` 다음) |
| `tick(context, time_delta)` | 노드와 조상이 모두 활성이고 컴포넌트가 켜져 있는 동안 매 프레임 |
| `is_tick_needed()` | `false` 를 돌려주면 `tick` 을 부르지 않습니다 (기본 `true`, 그리기만 하는 `Paint`, `Sprite`, `Text`, `Mask` 는 `false`) |
| `draw(transform, graphic)` | 노드가 보이고 컴포넌트가 켜져 있을 때. 원점은 노드 내용 영역의 왼쪽 위 |
| `push_transform(transform, graphic)`, `pop_transform(transform, graphic)` | 이 노드의 컴포넌트와 자식, 기즈모를 그리는 앞뒤. 여기서 바꾼 `Graphic` 상태는 자손까지 걸립니다 |
| `require()` | 붙기 직전. 함께 붙일 컴포넌트 목록 |
| `is_drawn_inside_content()` | 내용 영역 안에만 그리면 `true`. 노드의 켜진 컴포넌트가 모두 그리는 사각형을 알려 주면 화면 밖 거르기가 그 노드의 컴포넌트 그리기를 건너뜁니다 |
| `calculate_drawn_rect(transform, graphic)` | 내용 영역 밖에도 그리는 컴포넌트가 실제로 그리는 사각형을 알려 줄 때 |
| `touch_press`, `touch_move`, `touch_release`, `touch_cancel`, `wheel` | 터치와 휠 ("입력" 장) |

`tick` 과 터치 함수는 `ComponentContext` 를 받습니다. 문맥에서 자기 노드(`get_node`, `get_node_mut`, `get_transform_mut`), 트리 전체(`get_node_tree_mut`, 다른 노드 찾기, 만들기, 지우기), 엔진(`get_engine`, 입력, 시간, 뷰)을 꺼냅니다. 부르는 컴포넌트 하나만 노드에서 잠깐 빠져 있으므로 같은 노드의 다른 컴포넌트는 `context.get_node_mut().get_component_mut::<T>()` 로 찾습니다. `draw` 는 문맥 없이 자기 노드의 `NodeTransform` 만 받습니다. 그리는 동안에는 트리가 바뀌지 않습니다.

```rust
use vanilla::{Color, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Graphic, NodeTransform, NodeTree, Paint, Vector2};

struct Spin {
	component_base: ComponentBase,
	degrees_per_second: f64,
}

#[vanilla::reflect]
impl Spin {}

#[vanilla::reflect]
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

	fn tick(&mut self, context: &mut ComponentContext, time_delta: f64) {
		let transform = context.get_transform_mut();
		let local_rotation = transform.get_local_rotation();
		transform.set_local_rotation(local_rotation + self.degrees_per_second * time_delta);
		if let Some(paint) = context.get_node_mut().get_component_mut::<Paint>() {
			paint.set_color(Color::create(0.3, 0.8, 0.3, 1.0));
		}
	}
}

struct CrossMark {
	component_base: ComponentBase,
}

#[vanilla::reflect]
impl CrossMark {}

#[vanilla::reflect]
impl Component for CrossMark {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"CrossMark"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		graphic.set_stroke_color("#ffffff");
		graphic.draw_line(&[Vector2::zero(), content_size], 4.0);
		graphic.draw_line(&[Vector2::create(content_size.x, 0.0), Vector2::create(0.0, content_size.y)], 4.0);
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}
}

let engine = Engine::new(EngineConfiguration::new());
let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let node_id = node_tree.create_child(root_id, "spinner");
let node = &mut node_tree[node_id];
node.set_content_size(Vector2::create(160.0, 160.0));
node.add_component(Paint::create_with_color(Color::create(0.8, 0.3, 0.3, 1.0)));
node.add_component(CrossMark {
	component_base: ComponentBase::new(),
});
node.add_component(Spin {
	component_base: ComponentBase::new(),
	degrees_per_second: 90.0,
});

node_tree.tick(&engine, 0.5);
assert_eq!(node_tree[node_id].get_local_rotation(), 45.0);
assert_eq!(node_tree[node_id].get_component::<Paint>().expect("paint").get_color(), Color::create(0.3, 0.8, 0.3, 1.0));
```

컴포넌트는 붙은 차례로 그립니다. 위 예제는 `Paint` 로 바탕을 칠한 뒤 `CrossMark` 가 그 위에 선을 긋습니다. 씬에서는 엔진이 씬의 `tick` 에서 노드 트리를 갱신하므로 `node_tree.tick` 을 직접 부르지 않습니다. 위 예제는 창 없이 확인하려고 직접 불렀습니다.

컴포넌트는 `tick` 안에서 자기 노드를 지워도 됩니다. 트리를 도는 중에 부른 `destroy_node` 는 노드를 비활성으로 두었다가 다 돈 뒤에 지웁니다.

```rust
use vanilla::{Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, NodeTree};

struct Lifetime {
	component_base: ComponentBase,
	remaining_seconds: f64,
}

#[vanilla::reflect]
impl Lifetime {}

#[vanilla::reflect]
impl Component for Lifetime {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Lifetime"
	}

	fn tick(&mut self, context: &mut ComponentContext, time_delta: f64) {
		self.remaining_seconds -= time_delta;
		if self.remaining_seconds <= 0.0 {
			let node_id = context.get_node_id();
			context.get_node_tree_mut().destroy_node(node_id);
		}
	}
}

let engine = Engine::new(EngineConfiguration::new());
let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let bullet_id = node_tree.create_child(root_id, "bullet");
node_tree[bullet_id].add_component(Lifetime {
	component_base: ComponentBase::new(),
	remaining_seconds: 1.0,
});

node_tree.tick(&engine, 0.6);
assert!(node_tree.contains(bullet_id));
node_tree.tick(&engine, 0.6);
assert!(node_tree.get_node(bullet_id).is_none());
```

## 노드 트리 돌기

| 함수 (`NodeTree`) | 내용 |
| --- | --- |
| `get_parent_id(node_id)`, `get_child_ids(node_id)`, `get_child_count(node_id)` | 부모, 직계 자식 (그리는 차례) |
| `WorldNode` 의 `is_root()`, `is_leaf()`, `get_child(index)` | 부모가 없는지, 자식이 없는지, 그 자리의 자식 |
| `find_child_by_name(parent_id, name)`, `find_child(parent_id, predicate)` | 직계 자식 가운데 첫 것 |
| `find_descendant_by_name(parent_id, name)`, `find_child_recursive(parent_id, &predicate)` | 자손 전체에서 깊이 우선으로 첫 것 (`find_child_recursive_by_name` 은 `find_descendant_by_name` 과 같습니다) |
| `remove_child_by_name(parent_id, name)`, `remove_child_at(parent_id, index)`, `remove_children(parent_id)` | 직계 자식 떼어 내기 |
| `destroy_children(parent_id)`, `destroy_children_where(parent_id, predicate)` | 직계 자식을 지웁니다. 조건판은 지운 수를 돌려줍니다 |
| `get_sibling_ids(node_id)`, `is_ancestor_or_self(ancestor_id, node_id)` | 자기를 포함한 형제, 조상인지 |

원본에서 노드 객체를 주고받던 자리는 `NodeId` 이고, 이름 끝에 `_id` 나 `_ids` 가 붙은 함수가 핸들을 돌려줍니다. (`getParent` 는 `get_parent_id`, `getChildren` 은 `get_child_ids`)

```rust
use vanilla::{NodeId, NodeTree, Paint};

fn visit(node_tree: &NodeTree, node_id: NodeId, depth: usize, lines: &mut Vec<String>) {
	let name = node_tree[node_id].get_name();
	lines.push(format!("{}{}", "  ".repeat(depth), name));
	for child_id in node_tree.get_child_ids(node_id).iter().copied() {
		visit(node_tree, child_id, depth + 1, lines);
	}
}

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let player_id = node_tree.create_child(root_id, "Player");
let weapon_id = node_tree.create_child(player_id, "Weapon");
let enemies_id = node_tree.create_child(root_id, "Enemies");
for index in 0..3 {
	let enemy_id = node_tree.create_child(enemies_id, &format!("Enemy{}", index));
	node_tree[enemy_id].add_component(Paint::new());
}

let mut lines = Vec::new();
visit(&node_tree, root_id, 0, &mut lines);
assert_eq!(lines.join("\n"), "root\n  Player\n    Weapon\n  Enemies\n    Enemy0\n    Enemy1\n    Enemy2");

assert_eq!(node_tree.find_descendant_by_name(root_id, "Weapon"), Some(weapon_id));
let painted_id = node_tree.find_child_recursive(root_id, &|node| node.has_component::<Paint>()).expect("painted");
assert_eq!(node_tree[painted_id].get_name(), "Enemy0");
let removed_count = node_tree.destroy_children_where(enemies_id, |node| node.get_name() != "Enemy0");
assert_eq!(removed_count, 2);
assert_eq!(node_tree.get_child_count(enemies_id), 1);
```

## 주의할 점

- `node_tree[node_id]` 는 지운 노드면 패닉합니다. 지워졌을지 모르는 노드는 `get_node`, `get_node_mut` 의 `Option` 으로 꺼내거나 `contains(node_id)` 로 먼저 봅니다.
- `get_local_position()`, `get_local_scale()`, `get_pivot()`, `get_content_size()` 는 `Vector2` 값(복사본)을 돌려줍니다. 받은 벡터를 고쳐도 노드는 그대로이므로 고친 값을 다시 `set` 합니다.
- `get_component::<T>()` 는 원본 `instanceof` 처럼 그 타입과, 그 타입을 기반으로 품은 컴포넌트를 찾습니다. 러스트 판에는 상속이 없어 원본 상위 클래스를 품고 `Component::get_base_component` 로 알리므로, 원본에서 `Paint` 를 이어받던 `Sprite` 도 `get_component::<Paint>()` 로 찾힙니다. 정확히 그 타입만 보려면 `as_any().downcast_ref::<T>()` 를 씁니다.
- `remove_component::<T>()` 는 원본처럼 인스턴스 하나가 아니라 그 타입의 컴포넌트를 모두 뗍니다.
- `set_enable(false)` 인 컴포넌트는 그리기뿐 아니라 `tick`, 터치, 휠에서도 빠집니다. 원본은 그리기만 멈추고 `tick` 은 계속 불렀습니다.
- `WorldNode::add_component` 는 `attach_to_tree` 를 부르지 않고, `NodeTree::add_component` 는 `require()` 를 붙이지 않습니다. 붙을 때 자식 노드를 만드는 컴포넌트는 `NodeTree::add_component` 로 붙입니다.
- `NodeTree` 를 `&mut` 로 쥔 채 `node_tree[node_id]` 로 꺼낸 노드 참조를 들고 다른 노드를 만들 수는 없습니다. (빌림 검사) 위 예제처럼 노드 하나를 다 설정한 뒤 다음 노드를 만들거나, `NodeId` 를 들고 그때그때 꺼냅니다.
- 노드가 터치를 받으려면 `set_interactable(true)` 를 켭니다. 씬의 기본 `tick` 이 눌린 자리 맨 위의 터치 받는 노드를 찾아 그 노드의 컴포넌트에 터치를 넘깁니다. ("입력" 장)
- 루트 노드는 `NodeTree::new` 가 만들고, 엔진이 씬의 `create` 와 `load` 사이에 자리 (0, 0), 왼쪽 위 기준점, 내용 크기는 뷰 크기로 맞춥니다. 노드를 붙이는 일은 `load` 나 `initialize` 에서 합니다.

## 관련 타입

NodeTree, NodeId, WorldNode, NodeTransform, Component, ComponentBase, ComponentContext, RequiredComponent, TouchKind, Pivot, Paint, SceneBase, UINode
