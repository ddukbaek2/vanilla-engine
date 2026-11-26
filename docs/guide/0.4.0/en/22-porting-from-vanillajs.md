# Porting from vanilla.js

Vanilla Engine is vanilla.js ported to Rust. The two engines keep no feature gap, and the public API follows the original's usage. What differs is the naming style and a few patterns changed to fit Rust's ownership and borrowing rules. This chapter shows what to change, and how, when porting game code written for vanilla.js to Vanilla Engine: renaming, traits and shared state structs instead of class inheritance, `NodeId` instead of node references, the engine passed as an argument instead of a global engine, callbacks and `Rc<RefCell>`, `Option` instead of `null`, `Result` instead of exceptions, option structs instead of option objects, and the files both engines share. This chapter exists only in the Rust edition.

## Renaming

Names follow the Rust rules (Rust API Guidelines C-CASE). Converting an original name with these rules gives the Rust edition's name. Not abbreviating names is the same as the original.

| Kind | Original (vanilla.js) | Vanilla Engine |
| --- | --- | --- |
| Functions, methods | `setFillColor`, `getLocalPosition` | `set_fill_color`, `get_local_position` (snake_case) |
| Fields, variables, modules | `referenceResolutionSize`, `autoResizeOnWindowResize` | `reference_resolution_size`, `auto_resize_on_window_resize` |
| Types, traits | `WorldNode`, `UIButton` | `WorldNode`, `UIButton` (UpperCamelCase, unchanged) |
| Enum values | `ButtonState.normal` | `ButtonState::Normal` (UpperCamelCase) |
| Constants, associated constants | `Colors.white`, `Pivot.topLeft` | `Colors::WHITE`, `Pivot::TOP_LEFT` (SCREAMING_SNAKE_CASE) |
| Constructors | `new Engine(configuration)`, `Vector2.create(1, 2)` | `Engine::new(engine_configuration)`, `Vector2::create(1.0, 2.0)` |
| Rust keywords | `cooldown.use()`, `fullscreenPass.use()`, `shaderProgram.use()` | `cooldown.r#use()` and so on (`activate` and `use_program`, which do the same, also exist) |
| Functions that returned node objects | `getParent()`, `getChildren()`, `getSiblings()` | `get_parent_id`, `get_child_ids`, `get_sibling_ids` (they return handles) |

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

- Numbers are `f32`. An integer literal without a decimal point (`640`) cannot be used where an `f32` is expected, so write `640.0`. Only places that must produce JS number text exactly (`Format`, `LocalStorage`) use `f64`.
- Most functions that take a color take `impl Into<Color>`, so they accept both text such as `"#ffcc33"` and `Color::create(0.2, 0.25, 0.35, 1.0)`. The original `new Color(r, g, b, a)` is `Color::create(r, g, b, a)`.
- Names inside files and documents do not change. Keys such as `contentSize` in the UI document (`.scene.json`) and keys such as `LineWidth` in reflection saving stay as in the original, so both engines read the same files.

## Porting a First Scene

This is the scene from the original's nodes and components chapter.

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

Ported to Vanilla Engine, it looks like this.

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

- `class NodeScene extends Scene` becomes a struct holding a `SceneBase` plus `impl Scene`. The two functions you must write are `get_scene_base` and `get_scene_base_mut`, which hand out the shared state.
- Hooks receive `&mut Engine` as an argument, in place of the original `this.getEngine()`.
- There are no superclass calls such as the original `super.initialize(engine)`, because the trait's default implementations are empty. However, the defaults of `tick` and `draw` update and draw the node tree, so when you override them, call `self.scene_base.tick(engine, time_delta)` and `self.scene_base.draw(graphic)` where the original called `super.tick` and `super.draw`.
- `new WorldNode()` plus `root.addChild(panel)` is the single line `node_tree.create_child(root_id, "Panel")`. To create a node without a parent, use `create_node(name)` and then `add_child(parent, child)`.
- `addComponent(Paint)` takes a class, while `add_component(Paint::new())` takes a value and returns a `&mut` to the attached component.
- `engine.run` returns `Err(reason)` when it cannot create the window or GL.
- The original scene `load` step becomes handing assets over in `create` (`async load` is covered in the "async/await" chapter).

## Traits and Shared State Structs Instead of Class Inheritance

Rust has no class inheritance. Shared state is embedded as a struct, and overrides are trait functions.

| Original | Vanilla Engine |
| --- | --- |
| `class X extends Scene` | A `scene_base: SceneBase` field and `impl Scene for X` |
| `class X extends GameScene` | A `game_scene_base: GameSceneBase` field, `impl GameScene for X`, `vanilla::game_scene!(X);` |
| `class X extends UIScene` | A `UISceneBase` field, `impl UIScene for X`, `vanilla::ui_scene!(X);` |
| `class X extends Component` | A `component_base: ComponentBase` field and `impl Component for X` (`get_component_base`, `get_component_base_mut`, `get_component_type`) |
| `class X extends Asset` | An `AssetBase` field and `impl Asset for X` (`get_asset_base`, `get_asset_base_mut`) |
| `Node`, `ComponentNode`, `TransformNode`, `WorldNode` | A single `WorldNode`. Widgets that are nodes in the original, and `UINode`, are components |
| Widget inheritance (`UIButton extends UIControl`) | Embed the superclass part as a field and forward functions (`UIButton` embeds `UIControl`, `ShaderSprite` embeds `Sprite`). Reflection is chained with `#[vanilla::reflect(base = field)]` |

The `game_scene!` and `ui_scene!` macros generate `Scene` for that scene, so the base calls `resize`, `layout`, and `post_draw` on its own.

Porting a component looks like this.

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

- The original's `#` private fields are struct fields (without `pub`).
- In place of the original `this.getNode()`, hooks receive a `ComponentContext`. Get your own transform with `get_transform_mut`, your own node with `get_node_mut`, the whole tree with `get_node_tree_mut`, and the engine with `get_engine`.
- Other components on the same node are found with `context.get_node_mut().get_component_mut::<T>()`. The component being called is briefly taken out of the node meanwhile, so it cannot find itself this way.
- `draw` receives its own transform (`&NodeTransform`) instead of the node. The original `this.getNode().getContentSize()` is `transform.get_content_size()`. The tree does not change while drawing.
- Components that create child nodes when attached (such as widgets) must be attached with `node_tree.add_component(node_id, component)` so that `attach_to_tree` is called. This is where the original had `attach(node)`.

## NodeId Instead of Node References

In the original, a parent holds its child objects in `children[]` and each child points back with `parent`. In Vanilla Engine, the scene's `NodeTree` keeps every node in a single store and links parents and children with `NodeId`. A `NodeId` is a slot number plus a generation number, so the handle of a destroyed node is never reused.

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

- Nodes are fetched and configured with `node_tree[node_id]`. Fetching a destroyed node's handle this way panics, so when the node may be gone, check the `Option` returned by `get_node` or `get_node_mut`.
- Where the original kept node objects in fields, keep a `NodeId`. `NodeId` is `Copy`, so copy it freely and pass it into callbacks. Nodes not created yet are held as `Option<NodeId>`.
- `remove_child` and `remove_from_parent` only detach, while `destroy_node` destroys the node with its descendants. Called during `tick` or touch handling, it deactivates the node and destroys it after the traversal ends.
- The original `getParent`, `getChildren`, and `getSiblings` are `get_parent_id`, `get_child_ids`, and `get_sibling_ids`, which return handles. Searches (`find_child_by_name`, `find_descendant_by_name`, and so on) also return handles.
- While scene code holds a component, it cannot borrow other nodes of the same tree at the same time. To touch both, use `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)`; common cases have separate `*_in_tree` functions that take the tree (such as `UIWindow::close_in_tree` and `ParticleSystem::emit_in_tree`).

## The Engine as an Argument Instead of a Global Engine

The original gets the engine from anywhere with `Engine.getEngine()` or `this.getEngine()`. Vanilla Engine has no global engine. Every place that uses the engine shows it as an argument.

| Place | How it receives the engine |
| --- | --- |
| Scene hooks (`create`, `initialize`, `tick`, `pre_draw`, and so on) | The `engine: &mut Engine` argument |
| A component's `tick`, touch, wheel | `context.get_engine()` (`&Engine`) |
| Callbacks (such as `UIButtonEvent`) | The `ComponentContext` the callback receives |
| async tasks | `engine_context.with(\|engine\| ...)`, `scene_context.with(\|scene, engine\| ...)` |

This is why the original `Component.getEngine` is missing from the inspector's getter list, and `Engine` is missing from the reflected property list.

## Callbacks and Rc<RefCell>

The original's arrow functions capture `this` and outer variables freely. Vanilla Engine takes callbacks as `Box<dyn FnMut(...)>` and the component keeps them, so they cannot capture borrowed values such as the scene's `&mut self`. Capture a `NodeId` and reach other nodes through the context the callback receives, and put values shared with the scene in an `Rc<Cell>` or `Rc<RefCell>`.

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

- The button the original callback received still comes as the first argument, and the second argument, `ComponentContext`, reaches the tree and the engine.
- Callback getters (such as `UIButton::get_click_event`) and swapped-in storage backends (`LocalStorage::get_backend`, `PersistedStore::get_storage_backend`) are returned as `Rc<RefCell<...>>`. The caller borrows them with `borrow_mut`. The engine and the caller share the same function, and calling it again while it is being called is prevented.
- To change a value of the scene struct from a callback, keep that value in an `Rc<Cell<T>>` or `Rc<RefCell<T>>` field and pass a clone to the callback.

## Option Instead of null

Functions that returned `null` or `undefined` in the original return an `Option`. The receiving side tells the cases apart with `if let` or `match`.

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

- Where the original `getComponent(Paint)` was `null`, `get_component::<Paint>()` returns `None`.
- Setters that accepted `null` in the original take an `Option`. The original `sprite.setImage(null)` is `sprite.set_image(None)`. In reflected values, `None` is `ReflectValue::Null`, the place of the original `null`.

## Result Instead of Exceptions

Rust does not throw exceptions. Functions that can fail return `Result<T, String>` and put the reason in `Err`. The original `try`/`catch` becomes `match`, and inside a function of the same shape, `?` passes the error on.

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

- Inside an async task, where the original `await` threw, the `await` of a handle returns a `Result`. The original `reject` is `reject(reason)` on a `Promise<Result<T, E>>`.
- When only one effect fails, such as a shader that cannot be built, it is reported once and skipped. The frame does not stop.
- Failed assets still count as finished so the scene does not get stuck, the same as the original.

## Option Structs Instead of Option Objects

The original takes optional arguments as an object such as `{ decayRate: 9, frequency: 42 }`. Rust has no named optional arguments, so functions take an option struct with default values. Write only what you change and fill the rest with `..default`.

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

Option structs such as `ToonDrawOptions`, `RenderTargetOptions`, `UIConfirmWindowOptions`, and `FloatingTextOptions` all follow this pattern. When the engine adds a field, code built with `..Default::default()` keeps working.

## async/await and Reflection

- The original `async load`, `await asset.load(path)`, `await Wait.seconds(1)`, and `fadeScreen(async () => ...)` port to Rust `async`/`await` of the same shape. The difference is that instead of using `this` right on the line after `await`, you touch it inside `scene_context.with(|scene, engine| ...)`. The details are in the "async/await" chapter.
- The original scans the `get`, `is`, and `set` functions of every object at run time. In Vanilla Engine only types with `#[vanilla::reflect]` on the component's `impl` block and `impl Component` block are reflected. Add the macro to game components that you save in UI documents or show in the developer tools inspector. The details are in the "Reflection Macro #[reflect]" chapter.

## Files Both Engines Share

Asset files and files saved by the authoring tools are read and written in the same format by both engines. Leave the files as they are when porting.

| File | Made by (original side) | Read in Vanilla Engine by |
| --- | --- | --- |
| UI document `.scene.json` | The UI editor (`tools/uieditor`), `UIDocument.toJsonText` | `UIDocument::from_json_text`, `to_json_text` (the output matches the original's text down to key order, tab indentation, and number formatting). The kinds the editor registers take one line: `register_editor_types` |
| Timeline document `.timeline.json` | The timeline editor (`tools/timelineeditor`) | `Timeline::load_from_text`, `Timeline::load_from_url` |
| Sprite atlas JSON and page images | `tools/atlas.cjs` | `SpriteAtlas::load` (the same two formats as the original) |
| Language packs: `languages.json` and per-language files | The game's assets | `languagepack` (`load_language_list`, `load_language`) |
| UI font lists `fonts.json`, `fonts-<group>.json` | The game's assets | `uifont` (`load_ui_font`) |
| Build information `version.json` | The release build | `BuildInfo::load` |

- Asset paths are the same relative paths as in the original. The desktop reads straight from the folders in `EngineConfiguration.asset_directory_paths`, and the web fetches `asset_base_url` joined with the path, like the original. The game code is the same.
- The authoring tools and file formats are covered in the "Authoring Tools and Files Shared by Both Engines" chapter.

## Original Files That Were Not Ported

Original `src/` files with no Rust file of the same name were either merged into other files or deliberately not ported (the full list is in chapter 9 of the repository's ARCHITECTURE.md).

| Original | Vanilla Engine |
| --- | --- |
| `base/node.js`, `core/node/componentnode.js`, `core/node/transformnode.js` | Merged into `WorldNode` and `NodeTree` |
| `core/scenemanager.js` | `Engine`'s `load_scene`, `unload_scene`, `get_all_loaded_scenes` (`SceneId` instead of scene objects) |
| `core/touchraycaster.js` | Merged into `SceneBase::tick_touch`, `NodeTree`'s press test, and `TouchRecognizer` |
| `resource/jsonasset.js` | `JsonAsset` in the same file as `TextAsset` |
| `base/object.js`, `base/singleton.js`, `Dictionary`, `List`, `Queue`, `Set`, `Stack` in `experimental/collection` | The Rust language and standard library (traits, `Any`, `enum`, `Vec`, `HashMap`, and so on) do the same job |
| `base/reflection.js` | Run-time reflection is `#[vanilla::reflect]`; `createGUID` is `create_guid` |
| `experimental/camera.js` | `Camera2D` covers it, including rotation and `begin`/`end` |
| Work the browser did (2D canvas, `window.alert`, `confirm`, `prompt`, html lang and meta) | `Canvas2D`, `Engine::alert`, `confirm`, `prompt` (and `_async`), `Document` |

## Related Types

- `Scene`, `SceneBase`, `GameScene`, `GameSceneBase`, `UIScene`, `UISceneBase`, `game_scene!`, `ui_scene!`
- `Component`, `ComponentBase`, `ComponentContext`, `Asset`, `AssetBase`
- `NodeTree`, `NodeId`, `WorldNode`, `NodeTransform`
- `Engine`, `EngineConfiguration`, `ViewManager`
- `UIButtonEvent`, `Shaker`, `ShakerOptions`, `Cooldown`, `Colors`, `Color`, `Pivot`
- `UIDocument`, `Timeline`, `SpriteAtlas`, `BuildInfo`, `languagepack`, `uifont`
