# Architecture

What the engine is made of, what holds what, and in what order a frame runs. This is the repository's `ARCHITECTURE.md` trimmed down for people using the engine, focusing on what differs from the original vanilla.js.

## Module Layout

| Module | Contents |
| --- | --- |
| `base` | Base types that do not know the engine: `Vector2`, `Vector3`, `Matrix4`, `Quaternion`, `Rect`, `Color`, `Colors`, `Pivot`, `SeededRandom`, `FiniteStateMachine`, `ObjectPool`, `Cooldown`, `RepeatTimer`, `Version`, `format`, `math`, `wait` |
| `core` | The engine core: `Engine`, `Scene`, `SceneBase`, `Graphic`, `Component`, `TimeManager`, `ViewManager`, `InputManager`, `AudioManager`, the asset trait (`Asset`), `Tween`, `Animation`, the async executor (`EngineContext`, `SceneContext`, `TaskHandle`, `Promise`), `Canvas2D`, reflection values and traits |
| `core/node` | `NodeTree` (node storage), `WorldNode` |
| `core/component` | `Paint`, `Sprite`, `Text`, `RichText`, `Mask`, `Outline`, `SoftShade`, `ShadowText`, `ShadowImage` |
| `resource` | `ImageAsset`, `TextAsset`, `JsonAsset`, `BlobAsset`, `BunchAsset`, `FontAsset`, `AudioAsset`, `VisualAsset`, `AssetLoader`, `SpriteAtlas` |
| `effect` | `ParticleSystem`, `ShaderSprite`, `ScreenEffect`, `TrailRenderer`, `HeatHazeEffect` |
| `misc` | Game helpers such as `Camera2D`, `PathFinder`, `Collision2D`, `LocalStorage`, `Localization`, `DialogueRunner`, `DevTools` |
| `animation` | `Timeline`, `Animator`, `Action` |
| `graphics3d` | `SkinnedModel`, `SkinnedModelRenderer`, `HumanSkinRenderer`, `Material`, `ShadowMap`, `RenderTarget`, bloom, AO, SSS, `Camera3D`, toon 3D (`graphics3d/toon`) |
| `ui` | UI widgets, window stacking, `UIDocument`, `UINode`, `UIScene`, auto layout (`LayoutSolver` and others) |
| `game` | `GameScene`, `GameSceneBase` (a scene for games with a loading screen, touch routing, safe area, fades and developer tools) |
| `web` | `WebSocketClient`, `Pane` |
| `platform` | Windows, GL context, input events, frame loop, asset byte loading, sound. Desktop (winit, glutin) and web (canvas, WebGL2) |

- `src/lib.rs` re-exports the public names at the crate root, so you write `use vanilla::{Engine, Scene, SceneBase};`. Module paths (such as `vanilla::core::scene::SceneAny`) also work.
- As in the original, layers depend only upward. (`base` ← `core` ← upper layers such as `resource`, `effect`, `misc`) `platform` sits under `core` and provides the window, GL and input; the engine core does not know the platform.
- The reflection macro lives in a separate crate under `macros/` (`vanilla-engine-macros`) and is re-exported by the engine as `vanilla::reflect`. A game depends on the engine alone.

## Design Principles (What Differs from the Original)

1. **Nodes are referenced by handles.** In the original, a parent holds its child objects in an array and the child points back to the parent. In the Rust port, the scene's `NodeTree` puts every node in one storage (a slotmap) and links parents and children with `NodeId`. A `NodeId` is a slot index plus a generation, so even if a new node reuses a destroyed node's slot, the old handle returns `None`. `Rc<RefCell>` is not used.
2. **There is no global engine.** Instead of the original `Engine.getEngine()`, scene hooks and the component context (`ComponentContext`) receive the engine as an argument. Async tasks borrow the engine only inside the context's `with`.
3. **Traits and shared state structs instead of inheritance.** The forms are `Scene` + `SceneBase`, `Component` + `ComponentBase`, `Asset` + `AssetBase`. The original's four node classes (Node, ComponentNode, TransformNode, WorldNode) are merged into a single `WorldNode`, and widgets that were nodes in the original (input field, scroll bar, window and so on) and `UINode` are components in the Rust port.
4. **The platform layer is separate.** Windows, GL, input, the loop, asset bytes and sound live only under `platform`.
5. **Performance features are engine defaults.** Batching and off-screen culling are on from the start.
6. **It does not move to ECS.** The original's scene, node and component forms are kept so vanilla.js games are easy to port.
7. **2D and 3D share one GL context.** A scene's `pre_draw` draws the shadow, 3D scene and bloom passes, restores the 2D state with `Graphic::restore_render_state`, and then the UI is drawn. 3D inserted between 2D draws is wrapped in `Graphic::run_external_pass`.

How handles behave can be seen without a window.

```rust
use vanilla::{NodeTree, Paint, Vector2};

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let parent_id = node_tree.create_child(root_id, "parent");
let child_id = node_tree.create_child(parent_id, "child");
node_tree[child_id].set_local_position(Vector2::create(10.0, 20.0));
node_tree[child_id].add_component(Paint::create_with_color("#f3e5ab"));
assert_eq!(node_tree.get_parent_id(child_id), Some(parent_id));
assert_eq!(node_tree.get_child_ids(parent_id), &[child_id]);

node_tree.destroy_node(parent_id);
assert!(node_tree.get_node(parent_id).is_none());
assert!(node_tree.get_node(child_id).is_none());

let new_id = node_tree.create_child(root_id, "new");
assert_ne!(new_id, child_id);
assert!(node_tree.get_node(child_id).is_none());
assert!(node_tree.contains(new_id));
```

## Ownership

```text
EngineRunner
├── Engine
│   ├── EngineConfiguration
│   ├── TimeManager
│   ├── ViewManager
│   ├── InputManager
│   │   ├── GamepadManager
│   │   ├── InputActions
│   │   └── TextInput
│   ├── PointerCursor
│   ├── AudioManager
│   ├── Platform
│   ├── TaskExecutor
│   └── Graphic
│       ├── ShaderProgram
│       ├── ImageTextureCache
│       └── TextStringTextureCache
└── Vec<Box<dyn Scene>>
    └── SceneBase
        ├── NodeTree
        │   └── WorldNode
        │       ├── NodeTransform
        │       └── Vec<Box<dyn Component>>
        └── Vec<Tween>
```

- The platform holds the engine runner (`EngineRunner`) and calls it every frame. `Engine::run` creates this runner and hands it to the platform. Game code never handles the runner directly.
- `Graphic` is held by `Engine`. The platform passes it in after creating the GL context, and like the original `getGraphic()` you take it with `engine.get_graphic()` and `engine.get_graphic_mut()` in the scene's `create`, `initialize` and `tick` or in a component's `tick` (`context.get_engine()`). The scene's drawing hooks (`draw_on_load`, `pre_draw`, `draw`, `post_draw`, `draw_gizmos`) receive `&mut Graphic` as an argument. The engine takes the graphic out for a moment to pass it, so inside them `engine.get_graphic()` is `None` and you use the `graphic` argument. (This is the Rust borrowing form for passing `&mut Engine` and `&mut Graphic` together; to use both together outside drawing, call `engine.with_graphic(|engine, graphic| ...)`)
- The engine owns scenes as `Box<dyn Scene>`. To refer to another scene, use the `SceneId` returned by `Engine::load_scene` instead of the scene value. The original's `SceneManager` is merged into the engine.
- A scene being unloaded (waiting for the tasks it handed over in unload) stays in the list and receives `tick` and `draw`, as in the original. If the scene's async tasks remain after it leaves the list, the scene value is kept alive until they finish, then `destroy` is called and it is dropped.
- As in the original, assets are held as members by the scene, with no central manager. Drawing the same image many times creates only one GPU texture, in `Graphic`'s cache. To load several together, `AssetLoader` requests them up to a concurrency limit and counts progress.
- The sound device is opened by `AudioManager` on first use.

## Nodes and Components

- Creating nodes: `node_tree.create_child(parent_id, "name")` returns a `NodeId`, and `node_tree[node_id]` gives the node to configure. Unattached nodes are made with `create_node` and attached with `add_child`.
- Destroying: `destroy_node(node_id)` destroys the descendants too. Called in the middle of a `tick` or touch pass, it deactivates the node and destroys it after the pass.
- Attaching components: usually `node_tree[node_id].add_component(...)`. Components that create child nodes when attached (such as UI widgets) are attached with `node_tree.add_component(node_id, ...)`.
- In `tick`, touch and wheel, a component receives a `ComponentContext` that reaches its own node and transform, the whole tree and the engine (input, time, view, sound). Only the calling component is briefly taken out of its node, so while holding `&mut self` it can borrow the other components of the same node and the tree.
- To change a component and the tree together from scene code, use `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)`. Common operations have separate `*_in_tree` functions that take the tree.
- Nodes that carry only draw-only components (`Paint`, `Sprite`, `Text`, `Mask`) are skipped in `tick`. Nodes whose components all report the rectangle they draw are not drawn when off screen.

## Frame Loop

After the platform creates the window or canvas and GL and loads the first scene, every frame runs in this order.

1. Fonts whose bytes have arrived are registered in the glyph cache.
2. `Graphic` state is reset and the screen is cleared. (Viewport, projection, and the transform set to identity)
3. Time (`TimeManager`) and input (`InputManager`, including gamepads and input actions) are updated.
4. Woken async tasks are polled, and scenes that finished unloading are cleaned up.
5. For each scene: while loading only `draw_on_load` is called; otherwise `tick`, the scene's touch hooks, `pre_draw`, `draw`, `post_draw` and (if gizmos are on) `draw_gizmos` are called in order.
6. The statistics overlay (when `use_statistics` is on) and the mouse cursor are handled, and one-frame input flags are cleared.
7. The remaining batch is drawn, the verification screen capture is taken, and the frame number goes up by one. (The first frame is 1)
8. The scene list is updated. Scenes to unload get `finalize` and `unload`; newly loaded scenes get `create`.

Then the platform presents the screen. (Buffer swap on desktop; on the web the browser does it when the callback returns) On desktop, while one of the engine's dialogs (`Engine::alert` and others) is open, the scenes' `tick` and async tasks are skipped, the way JavaScript pauses in the original.

Scene loading maps the original's create, async load, initialize onto `create`, (waiting for the handed-over assets and tasks), `initialize`. The body of the original `async load` is a task handed over with `Engine::spawn_scene_task` in `create`; handing it to the scene with `SceneBase::track_task` makes the engine wait for that task. Even after everything handed over has finished, the loading screen stays for 0.3 more seconds, as in the original, before `initialize` is called. The detailed order is in the "Engine and Scene Lifecycle" chapter.

## Drawing

- All 2D drawing funnels into one place, `Graphic::draw_vertices`. When batching is possible, vertices are transformed on the CPU and collected, then drawn in one draw call while the same texture continues. Color and tint ride on each vertex, so draws with different colors go into the same batch.
- The collected batch is flushed first when the blend changes, when a clip is set or released, when particles are drawn, and at the end of the frame. Code that uses GL directly calls `Graphic::flush_batch` before that or wraps itself in `Graphic::run_external_pass`.
- Like the original, all 26 blend modes are drawn. 13 use fixed blending; the other 13 (Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity) copy the area under the draw region and blend in a shader following the W3C Compositing and Blending formulas. (These modes are not batched)
- Clipping is stencil-based, as in the original.
- Screen effects (`ScreenEffect`) are begun by the scene at the start of `pre_draw` and ended at the end of `post_draw`. Ending copies the screen into a texture and runs a chain of fullscreen passes.
- Batching and culling are turned off with `Graphic::set_batching_enabled` and `Graphic::set_culling_enabled`.

## Assets

- `ImageAsset`, `TextAsset`, `JsonAsset`, `BlobAsset` and `FontAsset` are requested with `load(engine, path)` and handed to the scene with `SceneBase::track_asset`. `load` returns a handle that can be awaited.
- Desktop reads on the spot from the folder candidates in `EngineConfiguration::asset_directory_paths`; the web fetches `asset_base_url` joined with the path. Game code is the same.
- Failed assets also count as finished, so the scene does not stall.
- On the web the browser decodes images, sound and fonts as in the original; on desktop the engine decodes them so the results match.
- No default font is built into the engine. Both platforms default to system fonts, and a game font is given with one of `default_font_url`, `default_font_bytes` or `FontAsset`.

## async/await

- The engine contains a single-threaded executor with no external runtime. Futures do not have to be `Send`, and desktop and web run the same code.
- `Engine::spawn_task` and `Engine::spawn_scene_task` run up to the first await right where they are called, like calling an original async function, and the rest runs every frame before the scenes' `tick`, in the order the tasks were woken.
- A future cannot hold `&mut` across an await, so a task borrows the scene and engine only inside the context's `with(|scene, engine| ...)` closure.
- The loads that returned a Promise in the original return handles that do not borrow the asset or scene. So the original `await this.atlas.load(path)` is the single line `scene_context.with(|scene, engine| scene.atlas.load(engine, path)).await`.
- Details are in the "async/await" chapter.

## Mapping to the Original

Of the 200 .js files in the original `src/`, 182 were ported. (176 under the same name, 6 merged into other files) The remaining 18 were left out on purpose: empty files, dead code, things the Rust language and standard library cover (object model helpers, collections), and what another feature covers (experimental/camera.js, covered by `Camera2D`).

What exists only in the Rust port is the work the browser does in the original: the platform layer, text shaping and color emoji, image decoding, the text input line, node storage, the async executor, and browser stand-ins (`Canvas2D`, dialogs, translations of text the browser draws).

## Differences Between the Two Engines

The two engines keep no feature differences. The remaining differences are only what truly cannot match because of the language or platform, and forms chosen on purpose to fit Rust.

**What truly cannot match**

- Reflection (reflective saving and loading in UI documents, the getter list in the developer tools inspector) works only for types marked with `#[vanilla::reflect]`. All engine components are marked; game components put it on their `impl` block and `impl Component` block. Rust has no way to scan methods at run time.
- The Chrome edit-field menu of desktop input fields has no items for Chrome's own browser features (spell check, translate, print and so on).
- On desktop `set_no_cache` has no meaning. (There is no browser cache)
- The memory field in the desktop developer tools shows the system allocator's heap instead of the JavaScript heap.
- On desktop, glyph raster pixels and shape edge antialiasing differ from Chrome. (Text widths and shape interior colors are the same)
- On desktop, the last digits of Opus and AAC sound samples differ very slightly from Chrome. (Length and trimming are the same)

**Forms chosen on purpose**

| Difference | Reason |
| --- | --- |
| There is no global engine. It is passed through scene functions and `ComponentContext` | So the places that use the engine show up as arguments |
| Nodes live in `NodeTree` and are referenced by `NodeId` | Pointing parents and children at each other with `Rc<RefCell>` brings reference cycles and run-time borrow checks |
| There is one node type, `WorldNode` | Rust has no class inheritance, so the node is one form and features are attached as components |
| Traits and embedding instead of inheritance (`Scene` + `SceneBase`, `Component` + `ComponentBase`) | Shared state goes in structs, overrides in trait functions |
| Numbers are `f32` (`f64` where JS number text must come out unchanged) | Kept at the same precision as the values uploaded to GL |
| Objects that use GL do not hold the context; each function receives gl | The context is held by `Graphic`, and sharing it would tangle lifetimes and borrows |
| Some places have separate `*_in_tree` functions that take the tree | While scene code holds a component, it cannot also borrow other nodes in the same tree |
| Option structs with defaults instead of optional argument objects (`RenderTargetOptions`, `UIConfirmWindowOptions` and others) | Rust has no named optional arguments |
| Callbacks are taken as `Box<dyn FnMut>`, and callback getters return `Rc<RefCell>` | The engine and the caller share the same function |
| `Result` and `Option` instead of exceptions | Rust does not throw exceptions |
| Names follow Rust conventions (snake_case, UpperCamelCase, SCREAMING_SNAKE_CASE) | Following Rust API Guidelines C-CASE |
| Async tasks touch the scene and engine only inside the context's `with` | Rust futures cannot hold `&mut` across an await |

Renaming and ownership rules from the porting side are in the "Porting from vanilla.js" chapter.

## Related Types

Engine, EngineConfiguration, Scene, SceneBase, SceneId, NodeTree, NodeId, WorldNode, Component, ComponentBase, ComponentContext, Graphic, EngineContext, SceneContext, TaskHandle, Promise
