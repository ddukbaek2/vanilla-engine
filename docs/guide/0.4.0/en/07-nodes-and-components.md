# Nodes and Components

What you see in a scene is built as a node tree. Nodes have a position and a place in the hierarchy, while features such as drawing and movement are handled by components attached to nodes. This chapter covers how the node tree is organized, creating and attaching nodes, positions and transforms, adding and finding components, writing your own components, and walking the tree.

## How it is organized

```text
SceneBase
└── NodeTree
    └── WorldNode (NodeId)
        ├── NodeTransform
        └── Vec<Box<dyn Component>>
Component + ComponentBase
```

- `NodeTree` is the scene's node storage. It refers to nodes by `NodeId`.
- `WorldNode` holds a name, the active flag, whether it receives touches, the `NodeId` of its parent and children, and its component list.
- `NodeTransform` holds the local position, rotation, scale, opacity, pivot, anchor, content size and the visible flag.
- There is only one kind of node, `WorldNode`. The original's four levels of inheritance (`Node`, `ComponentNode`, `TransformNode`, `WorldNode`) are merged into one.
- The scene's `NodeTree` keeps all nodes in a single storage and links parents and children by `NodeId`. Game code keeps `NodeId` values around and takes nodes out with `node_tree[node_id]`, `get_node` or `get_node_mut`. A `NodeId` is a slot number plus a generation number, so `get_node` returns `None` for the `NodeId` of a destroyed node.
- Get the scene's node tree with `scene_base.get_node_tree_mut()` and the root node with `scene_base.get_root_id()`.
- The original `UINode` is a separate kind of node, but in the Rust version `UINode` is a component attached to a `WorldNode`. Placing nodes with constraints is covered in the "UI" chapter.
- `Component` is a trait. Shared state such as the enabled flag and gizmo visibility lives in `ComponentBase`, which the implementing type keeps as a field and hands back.

## Creating and attaching nodes

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

	fn pre_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
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

- `create_child(parent_id, name)` creates a node, appends it to the end of that parent's child list and returns its `NodeId`.
- `create_node(name)` creates a node that is not attached anywhere. It is drawn only after you attach it with `add_child(parent_id, child_id)`. If the node was attached to another parent, it is detached from there and moved.
- `add_child` and `insert_child(parent_id, child_id, index)` return `true` when they attach. They return `false` if either node does not exist, if you try to attach the root, or if you try to attach a node under itself or one of its own descendants.
- `remove_child(parent_id, child_id)` and `remove_from_parent(child_id)` only detach; they do not destroy the node. A detached node can be attached again.
- `destroy_node(node_id)` destroys the node together with its descendants. The root is never destroyed.
- `draw_scene_background` in `pre_draw` fills the canvas with the scene background color and applies the view coordinate system. The default `draw` draws the node tree.
- A new node has position (0, 0), scale 1, rotation 0, opacity 1, and is active, visible and not receiving touches. Its content size is (0, 0) and its pivot and anchor are at the center (0.5, 0.5).

## Node positions

The local position is "where this node's pivot is, measured from the top left of the parent's content area". The pivot is a ratio inside the content area (0 to 1), so with the default (0.5, 0.5) the center of the node sits on its local position. In the example above, the center of `Badge` sits on the top right corner of `Panel` (400, 0).

| Function | Description |
| --- | --- |
| `set_content_size(size)`, `set_content_size_xy(width, height)` | Content size. `Paint` and `Sprite` fill this size |
| `set_pivot(pivot)` | Pivot. There are nine constants such as `Pivot::TOP_LEFT`, `Pivot::MIDDLE_CENTER` and `Pivot::BOTTOM_RIGHT` |
| `set_anchor(anchor)` | Anchor. Not used for drawing; only used to compute anchored positions |
| `set_anchored_position(position, parent_content_size)` | A `NodeTransform` function. Sets the local position measured from the anchor point (parent size x anchor) inside the parent's content area |
| `set_rect(x, y, width, height)` | Sets the content size and local position together so the top left lands on (x, y), wherever the pivot is |

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

`corner` sits at (740, 565), which is (800 x 1 - 60, 600 x 1 - 35). `label` sits at (20, 20) because its pivot is the top left, while `badge`, whose pivot is the center, sits at (20 + 300 x 0.5, 20 + 40 x 0.5), that is (170, 40), for the same rectangle.

The position functions on `WorldNode` are shortcuts to the functions of the same name on `NodeTransform`. For things without a shortcut, such as the anchored position, take the `NodeTransform` out with `get_transform_mut()` and call it there. The anchored position needs the parent's content size, and since a node does not hold its parent, it takes the parent size as an argument.

## Transforms

| Local (`WorldNode`, relative to parent) | Global (`NodeTree`, relative to root) | Description |
| --- | --- | --- |
| `set_local_position`, `set_local_position_xy`, `get_local_position` | `set_position(node_id, position)`, `get_position(node_id)` | Position |
| `set_local_rotation`, `get_local_rotation` | `set_rotation(node_id, rotation)`, `get_rotation(node_id)` | Rotation (degrees, clockwise) |
| `set_local_scale`, `set_local_scale_xy`, `get_local_scale` | `set_scale(node_id, scale)`, `get_scale(node_id)` | Scale |
| `set_local_opacity`, `get_local_opacity` | `get_opacity(node_id)` | Opacity (clamped to 0 to 1, multiplied down to children) |

Global values need to walk up through the parents, so `NodeTree` computes them from a `NodeId`. The global position is the position of the pivot, computed from the parent's pivot with the parent's global rotation and scale applied.

When drawing, a node moves to its local position, rotates, applies its scale, then moves back by the pivot (content size x pivot) and draws its components with the top left of the content area as the origin. Children repeat the same steps from that origin. So rotating and scaling a parent rotates and scales its children with it.

`NodeTree` also provides screen side tests.

- `get_world_corners(node_id)`: the four corners after all parents' position, rotation and scale (top left, top right, bottom right, bottom left)
- `get_world_bounds(node_id)`: the axis aligned rectangle around those four points
- `convert_view_to_local(node_id, view_position)`: converts view coordinates to this node's content area coordinates (top left is 0, 0). `None` when a scale of 0 makes it impossible to invert
- `convert_node_position_to_view_position(node_id, node_position)`: the other way round, content area coordinates to view coordinates
- `calculate_world_matrix(node_id)` with `WorldNode::contains_point(&world_matrix, view_position)`: whether a point in view coordinates is inside the node (correct for rotated nodes too)

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

Because the parent is scaled by two, the child's local position (10, 20) ends up (20, 40) away from the parent's origin (100, 50), at (120, 90). The 40 x 40 child covers 80 x 80 on screen. `set_position` does the reverse calculation and sets the local position.

## Active, visible and draw order

- A node with `set_active(false)` and its descendants receive no updates (`tick`), no drawing and no touch hit tests. `NodeTree::is_active_in_hierarchy(node_id)` checks that every node up to the root is active.
- A node with `set_visible(false)` and its descendants are not drawn and receive no touches, but they still receive updates. This flag does not exist in the original. `NodeTree::is_visible_in_hierarchy(node_id)` checks that every node up to the root is active, visible and has an opacity above 0.
- Opacity is only multiplied in when drawing, so a node with opacity 0 still goes through drawing and still receives touch hit tests. To turn off both drawing and touches, use `set_visible(false)`.
- Children are drawn in child list order. Later children appear on top. Change the order among siblings with `NodeTree::set_sibling_index(node_id, index)` and read it with `get_sibling_index(node_id)`. An index past the child count moves the node to the end.

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

## Adding and finding components

| Function | Description |
| --- | --- |
| `add_component(component)` | Takes a value, attaches it and returns the attached component (`&mut T`) |
| `get_or_add_component::<T>()` | The existing one if there is one, otherwise a new one attached from `T::default()` |
| `get_component::<T>()`, `get_component_mut::<T>()` | The first component of that type, or `None` |
| `get_components::<T>()` | Every component of that type (in attach order) |
| `has_component::<T>()` | Whether there is one |
| `get_all_components()` | All of them in attach order (a list of `Box<dyn Component>`) |
| `remove_component::<T>()` | Detaches every component of that type and returns how many were detached |

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

A component declares the components it needs with `require()`. When it returns a list of `RequiredComponent::of::<T>()`, `WorldNode::add_component` creates each one from `T::default()` and attaches it right before attaching, but only if the node does not already have that type. Attaching goes: reserve its own slot, attach the required components, then `attach(transform)`. Detaching removes it from the list and then calls `detach(transform)`.

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

### Components that use the tree

- Components that create child nodes when attached (such as `UIView`) are attached with `node_tree.add_component(node_id, component)`. After `attach`, `attach_to_tree(node_tree, node_id)` is called so the component can create nodes there. This path does not attach `require()`. It returns `None` if the node does not exist.
- To change a component and the tree together from scene code, use `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)`. It briefly takes out the node's first `T` and hands it over together with the tree. Components that also touch other nodes, such as widgets, use this path.

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

## Writing a component

Implement the `Component` trait. `get_component_base`, `get_component_base_mut` and `get_component_type` must be implemented; override the rest only as needed.

| Function to override | When it is called |
| --- | --- |
| `attach(transform)`, `detach(transform)` | When attached to a node, when detached |
| `attach_to_tree(node_tree, node_id)` | When attached with `NodeTree::add_component` (after `attach`) |
| `tick(context, time_delta)` | Every frame while the node and all its ancestors are active and the component is enabled |
| `is_tick_needed()` | Returning `false` stops `tick` from being called (default `true`; `Paint`, `Sprite`, `Text` and `Mask`, which only draw, return `false`) |
| `draw(transform, graphic)` | When the node is visible and the component is enabled. The origin is the top left of the node's content area |
| `push_transform(transform, graphic)`, `pop_transform(transform, graphic)` | Before and after drawing this node's components, children and gizmos. `Graphic` state changed here applies to the descendants too |
| `require()` | Right before attaching. The list of components to attach with it |
| `is_drawn_inside_content()` | `true` if you draw only inside the content area. When every enabled component of a node reports its drawn rectangle, off-screen culling skips drawing that node's components while it is off screen |
| `calculate_drawn_rect(transform, graphic)` | For components that also draw outside the content area, to report the rectangle they actually draw |
| `touch_press`, `touch_move`, `touch_release`, `touch_cancel`, `wheel` | Touch and wheel (the "Input" chapter) |

`tick` and the touch functions receive a `ComponentContext`. From the context you get your own node (`get_node`, `get_node_mut`, `get_transform_mut`), the whole tree (`get_node_tree_mut`, to find, create and destroy other nodes) and the engine (`get_engine`, for input, time and the view). Only the component being called is briefly taken out of the node, so other components on the same node are found with `context.get_node_mut().get_component_mut::<T>()`. `draw` receives no context, only its own node's `NodeTransform`. The tree does not change while drawing.

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

Components draw in attach order. In the example above, `Paint` fills the background and then `CrossMark` draws lines on top of it. In a scene, the engine updates the node tree from the scene's `tick`, so you do not call `node_tree.tick` yourself. The example calls it directly to check the result without a window.

A component may destroy its own node inside `tick`. `destroy_node` called while the tree is being walked makes the node inactive and destroys it after the walk is over.

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

## Walking the node tree

| Function (`NodeTree`) | Description |
| --- | --- |
| `get_parent_id(node_id)`, `get_child_ids(node_id)`, `get_child_count(node_id)` | Parent and direct children (in draw order) |
| `is_root()`, `is_leaf()`, `get_child(index)` on `WorldNode` | Whether it has no parent, whether it has no children, the child at that index |
| `find_child_by_name(parent_id, name)`, `find_child(parent_id, predicate)` | The first match among direct children |
| `find_descendant_by_name(parent_id, name)`, `find_child_recursive(parent_id, &predicate)` | The first match among all descendants, depth first (`find_child_recursive_by_name` is the same as `find_descendant_by_name`) |
| `remove_child_by_name(parent_id, name)`, `remove_child_at(parent_id, index)`, `remove_children(parent_id)` | Detach direct children |
| `destroy_children(parent_id)`, `destroy_children_where(parent_id, predicate)` | Destroy direct children. The predicate version returns how many were destroyed |
| `get_sibling_ids(node_id)`, `is_ancestor_or_self(ancestor_id, node_id)` | Siblings including itself, whether it is an ancestor |

Where the original passed node objects around, the Rust version uses `NodeId`, and the functions ending in `_id` or `_ids` return handles. (`getParent` becomes `get_parent_id`, `getChildren` becomes `get_child_ids`)

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

## Things to watch out for

- `node_tree[node_id]` panics for a destroyed node. For nodes that may have been destroyed, take them out as an `Option` with `get_node` or `get_node_mut`, or check `contains(node_id)` first.
- `get_local_position()`, `get_local_scale()`, `get_pivot()` and `get_content_size()` return `Vector2` values (copies). Changing the returned vector leaves the node unchanged, so `set` the changed value back.
- `get_component::<T>()` finds that type and components that embed it as a base, like the original `instanceof`. The Rust version has no inheritance, so a component embeds the original superclass and reports it with `Component::get_base_component`; `Sprite`, which derived from `Paint` in the original, is found by `get_component::<Paint>()` too. To look for that exact type only, use `as_any().downcast_ref::<T>()`.
- `remove_component::<T>()` detaches every component of that type, not a single instance as in the original.
- A component with `set_enable(false)` is left out not only of drawing but also of `tick`, touches and the wheel. The original stopped only drawing and kept calling `tick`.
- `WorldNode::add_component` does not call `attach_to_tree`, and `NodeTree::add_component` does not attach `require()`. Attach components that create child nodes with `NodeTree::add_component`.
- While you hold a node reference taken with `node_tree[node_id]` from a `&mut NodeTree`, you cannot create another node (the borrow checker). Finish setting up one node before creating the next, as in the examples above, or keep the `NodeId` and take the node out when needed.
- For a node to receive touches, turn on `set_interactable(true)`. The scene's default `tick` finds the topmost touch receiving node under the press and passes the touch to that node's components. (the "Input" chapter)
- The root node is created by `NodeTree::new`, and right after the scene's `create` the engine sets it to position (0, 0), a top left pivot and the view size as its content size. Attach nodes in `create` or `initialize`.

## Related types

NodeTree, NodeId, WorldNode, NodeTransform, Component, ComponentBase, ComponentContext, RequiredComponent, TouchKind, Pivot, Paint, SceneBase, UINode
