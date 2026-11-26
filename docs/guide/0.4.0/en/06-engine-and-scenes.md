# Engine and Scene Lifecycle

Starting from configuring the engine with `EngineConfiguration` and running the first scene with `Engine::run`, this chapter covers the order in which scene hooks are called, switching scenes, time, screen size and reference resolution, and page events.

## Engine Configuration (EngineConfiguration)

Create it with `EngineConfiguration::new()`, change its fields, and pass it to `Engine::new(engine_configuration)`. (`Default` gives the same values) The defaults are the values written in `new`.

| Field | Default | Meaning |
| --- | --- | --- |
| `reference_resolution_size` | `Vector2::create(1280.0, 720.0)` | Reference resolution. When the view scale mode is not `None`, scenes draw in this coordinate system |
| `view_scale_mode` | `ViewScaleMode::StretchShort` | View scale mode set on the view manager when the engine is created |
| `title` | The engine's Korean name | Desktop window title |
| `window_size` | `Vector2::zero()` | Initial desktop window size (logical size, 0 means the reference resolution) |
| `is_resizable` | `true` | Whether the desktop window can be resized |
| `use_statistics` | `false` | Whether to draw FPS, draw calls, vertex count and culled node count at the top left of the screen |
| `use_dev_tools` | `true` | Whether `GameSceneBase` (and `UISceneBase`, which embeds it) creates the developer tools (F2). Turn off in release builds |
| `maximum_frame_per_second` | `0.0` | Max FPS. 0 means no limit. (With vertical sync on, it follows the display refresh rate) At run time, change it with `Engine::set_maximum_frame_per_second` |
| `use_vertical_sync` | `true` | Desktop vertical sync |
| `default_font_paths`, `default_bold_font_paths` | Empty list | Files of the desktop default font chain (regular, bold). When empty, operating system fonts are searched and chained in the order of the original's system font names |
| `fallback_font_paths` | Empty list | Desktop fallback font candidates. When empty, the fallback font chosen by the operating system is used |
| `asset_directory_paths` | Empty list | Desktop asset folder candidates. `Engine::resolve_asset_path` searches them from the front |
| `window_icon_path` | `""` | Desktop window icon PNG (relative to the asset folder, empty means the default icon) |
| `default_font_bytes`, `default_bold_font_bytes` | `None` | Default font file contents. When present, used before the path candidates |
| `canvas_id` | `"mainCanvas"` | Id of the canvas to draw in on the web. If no such element exists, one is created with that id and appended to `body` |
| `default_font_url` | `""` | Web default font URL. Fetched when `default_font_bytes` is not set. When empty, nothing is fetched |
| `resume_audio_on_touch_end` | `false` | Whether `touchend` and `pointerup` also count as user gestures that wake the audio on the web |
| `asset_base_url` | `""` | Prefix for web asset URLs (empty means relative to the page) |
| `auto_resize_on_window_resize` | `false` | Whether the canvas follows the window when the window is resized. When on, it fits the size minus the area covered by the on-screen keyboard and applies `max_canvas_aspect_ratio` |
| `preserve_drawing_buffer` | `false` | Whether what was drawn stays after the frame ends. When on, the color is not cleared at the start of a frame, so drawing goes on top of the last frame |
| `ignore_virtual_keyboard_resize` | `false` | Whether to ignore a resize where the width stays the same and only the height shrinks (the on-screen keyboard) while a text input has focus (with `auto_resize_on_window_resize`) |
| `max_canvas_aspect_ratio` | `0.0` | Upper limit of the canvas aspect ratio. If the window is more elongated, the canvas is shrunk and centered. 0 means no limit |
| `rebake_text_on_font_load` | `false` | Whether, when a font arrives late, baked text within that font's character ranges is discarded and baked again |
| `document_language` | `""` | Document language (the html lang of the web build's `index.html`, `Document::get_language`) |
| `document_meta_list` | Empty list | Document meta list (name, content) (`Document::get_meta_content`; `BuildInfo` reads build-version) |

## Creating and Running the Engine

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, Vector2};

struct MainScene {
	scene_base: SceneBase,
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("My Game");
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	println!("{}", engine.get_version_string());
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

`Engine::new(engine_configuration)` does the following.

- It takes ownership of the configuration. Read it with `get_engine_configuration`.
- It creates the time, view, input, mouse cursor and audio managers and sets the document language and meta from the configuration. The sound device is opened on first use.
- It does not create the window or GL yet. So you can create an engine without a window and work with its managers and configuration.

```rust
use std::path::PathBuf;
use vanilla::{Engine, EngineConfiguration, Vector2, ViewScaleMode};

let mut engine_configuration = EngineConfiguration::new();
engine_configuration.reference_resolution_size = Vector2::create(1920.0, 1080.0);
engine_configuration.view_scale_mode = ViewScaleMode::StretchShortExpandLong;
engine_configuration.asset_directory_paths = vec![PathBuf::from("assets")];
let mut engine = Engine::new(engine_configuration);
assert_eq!(engine.get_frame_number(), 1);
assert!(engine.get_canvas().is_none());
let view_manager = engine.get_view_manager();
assert_eq!(view_manager.get_reference_resolution_size(), Vector2::create(1920.0, 1080.0));
assert_eq!(view_manager.get_view_scale_mode(), ViewScaleMode::StretchShortExpandLong);
engine.set_maximum_frame_per_second(30.0);
assert_eq!(engine.get_engine_configuration().maximum_frame_per_second, 30.0);
```

`engine.run(scene)` takes the engine (`self`), puts the first scene on the load list and hands everything to the platform. The platform creates the window or canvas and the GL context, loads the default fonts, loads the first scene and runs the frame loop.

- On desktop it does not return until the window is closed or `Engine::exit_application` is called. On the web it grabs the canvas and WebGL2, registers `requestAnimationFrame` and returns right away.
- If the platform fails, for example when it cannot create the window or GL context, it returns `Err(reason)`.
- `run` takes the engine, so call it only once. Later scenes are loaded with `load_scene` on the engine a scene hook receives.

What the engine holds is reached through `get_time_manager`, `get_view_manager`, `get_input_manager`, `get_audio_manager`, `get_pointer_cursor` and their `_mut` versions, `get_platform`, `get_canvas` (the window on desktop, the canvas on the web, `None` before `run`) and `get_engine_configuration`. Unlike the original there is no global engine (`Engine.getEngine()`); scene hooks receive `&mut Engine` as an argument. `Graphic` is not taken from the engine; the drawing hooks receive it separately.

## Scene Lifecycle

A scene implements the `Scene` trait and embeds a `SceneBase`, which holds the shared scene state (node tree, tweens, handed-over assets, background color), as a field. The only required functions are `get_scene_base` and `get_scene_base_mut`; override only the other hooks you need. They are called in this order.

1. Create the scene value. `SceneBase::new()` creates the root node, named "root", along with it. (The original creates the root in `create`)
2. When passed to `Engine::load_scene` or `Engine::run`, the scene gets a scene handle (`SceneId`), and `create(engine)` is called at the end of that frame. Right after, the engine fits the root to the view size. (Position 0, top-left pivot, content size equal to the view size)
3. When all assets and tasks handed over in `create` (`track_asset`, `track_load_handle`, `track_audio_asset`, `track_task`) have finished and 0.3 more seconds have passed, `initialize(engine)` is called, followed once by `resize(engine, canvas_native_size)`. With nothing handed over, that is 0.3 seconds after `create`.
4. Until `initialize`, the engine calls only `draw_on_load(engine, graphic)` every frame. (The loading screen)
5. After that, every frame calls `tick(engine, time_delta)`, the scene's touch hooks (`touch_press`, `touch_move`, `touch_release`, `touch_cancel`, `touch_wheel`), `pre_draw`, `draw` and `post_draw` in order. With `SceneBase::set_gizmo_visible(true)`, `draw_gizmos` is called after them.
6. When the window size changes, `resize(engine, canvas_native_size)` is called.
7. When unloading, `finalize(engine)` and then `unload(engine)` are called at the end of that frame. When the tasks handed over with `track_task` in `unload` finish, the scene leaves the list, and after all of the scene's async tasks finish, `destroy(engine)` is called and the scene value is dropped.

The default `tick` calls `SceneBase::tick`, which updates the node tree, dispatches touches (to the topmost pressable node under the press) and updates tweens, in order. The default `draw` calls `SceneBase::draw`, which draws the node tree. Where the original calls `super.tick()` and `super.draw()`, you call these two directly.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Graphic, Scene, SceneBase, Vector2};

struct LifecycleScene {
	scene_base: SceneBase,
	tick_count: u32,
}

impl Scene for LifecycleScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, _engine: &mut Engine) {
		let root_name = self.scene_base.get_root().get_name().to_string();
		println!("create {}", root_name);
	}

	fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		println!("initialize");
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.tick_count += 1;
		if self.tick_count == 1 {
			println!("first tick {}", time_delta);
		}
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
	}

	fn resize(&mut self, _engine: &mut Engine, canvas_native_size: Vector2) {
		println!("resize {} {}", canvas_native_size.x, canvas_native_size.y);
	}

	fn finalize(&mut self, _engine: &mut Engine) {
		println!("finalize");
	}

	fn unload(&mut self, _engine: &mut Engine) {
		println!("unload");
	}

	fn destroy(&mut self, _engine: &mut Engine) {
		println!("destroy");
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let lifecycle_scene = LifecycleScene {
		scene_base: SceneBase::new(),
		tick_count: 0,
	};
	if let Err(error) = engine.run(Box::new(lifecycle_scene)) {
		eprintln!("{}", error);
	}
}
```

`SceneBase` can be tried out without an engine. Before loading it has no handle, and with nothing handed over it counts as finished.

```rust
use vanilla::{Color, SceneBase};

let mut scene_base = SceneBase::new();
assert!(scene_base.get_scene_id().is_none());
assert!(!scene_base.is_loaded());
assert!(scene_base.are_tracked_assets_finished());
assert_eq!(scene_base.get_tracked_asset_progress(), 1.0);
assert_eq!(scene_base.get_scene_background_color(), Color::black());
scene_base.set_scene_background_color("#203040");
assert_eq!(scene_base.get_root().get_name(), "root");
```

### Waiting for Assets and Tasks (Where the Original Has async load)

The original awaits assets inside `async load(engine)`. In the Rust port, `create` hands a loading task to `Engine::spawn_scene_task` and gives it to the scene with `SceneBase::track_task`. The task runs up to the first await right where it is handed over, and inside the task the scene and engine are touched only inside `SceneContext::with`. The engine calls `initialize` only after the handed-over task finishes.

```rust,no_run
use vanilla::{Engine, Graphic, Scene, SceneBase, SceneContext};

struct StageScene {
	scene_base: SceneBase,
	stage_byte_count: usize,
}

impl Scene for StageScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		let load_task = engine.spawn_scene_task(self, |mut scene_context: SceneContext<StageScene>| async move {
			let load_handle = scene_context.with(|_scene, engine| engine.request_asset_bytes("examples/assets/scenedocument.scene.json"));
			if let Ok(bytes) = load_handle.await {
				scene_context.with(|scene, _engine| {
					scene.stage_byte_count = bytes.len();
				});
			}
		});
		self.scene_base.track_task(&load_task);
	}

	fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		println!("{}", self.stage_byte_count);
	}
}
```

Handing an asset's load handle to `track_asset` without await still works, and the two can be mixed. Desktop reads files on the spot; the web draws the loading screen with `draw_on_load` until the frame the fetch finishes. Asset kinds are in the "Assets" chapter, async tasks in the "async/await" chapter.

## One Frame

A frame from the engine's side looks like this: registering fonts, resetting `Graphic` state and clearing the screen, updating `TimeManager`, updating `InputManager` (gamepads, input actions), polling woken async tasks, step 4 (loading) or 5 above for each loaded scene, the statistics overlay (when on), the mouse cursor, clearing one-frame input flags, drawing the remaining batch, the verification screen capture (when requested), increasing the frame number, and updating the scene list (`finalize` and `unload` for scenes to unload, `create` for new scenes).

The engine does not catch panics raised in a scene. (The original catches exceptions per scene, prints them and keeps the loop going)

## Switching Scenes

The engine holds the scene list. (The engine does the original `SceneManager`'s work)

| Function | What it does |
| --- | --- |
| `load_scene(scene)` | Adds the scene to the list after this frame and returns its `SceneId` (numbered from 1) |
| `unload_scene(scene)` | Unloads one scene after this frame. Takes a `SceneId` or the shared scene state (`&SceneBase`). Does nothing for an unknown handle |
| `unload_all_scenes()` | Unloads every scene after this frame. Called together with `load_scene`, it unloads first and then loads |
| `load_scene_async(scene)` | Same as `load_scene`, and returns a `Promise` that resolves with the scene's handle when `initialize` is done |
| `unload_scene_async(scene)`, `unload_all_scenes_async()` | Return a `Promise` that resolves when unloading finishes |
| `get_all_loaded_scenes()` | List of handles of loaded scenes (those whose `create` was called), in load order |

Several scenes can be loaded together. All loaded scenes are updated every frame and drawn in load order. (A scene loaded later is on top) A scene knows its own handle through `self.scene_base.get_scene_id()` and unloads itself with `engine.unload_scene(&self.scene_base)`.

```rust
use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, SceneId};

struct EmptyScene {
	scene_base: SceneBase,
}

impl Scene for EmptyScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}
}

let mut engine = Engine::new(EngineConfiguration::new());
let first_scene_id: SceneId = engine.load_scene(Box::new(EmptyScene {
	scene_base: SceneBase::new(),
}));
let second_scene_id: SceneId = engine.load_scene(Box::new(EmptyScene {
	scene_base: SceneBase::new(),
}));
assert_ne!(first_scene_id, second_scene_id);
assert!(engine.get_all_loaded_scenes().is_empty());
engine.unload_scene(second_scene_id);
```

(An engine created without a window runs no frames, so `create` is not called and the loaded scene list stays empty)

To switch without a blank screen, load the next scene first and unload the current one once it is up. Awaiting the `Promise` returned by `load_scene_async` in an engine task gives the same flow as the original `await engine.loadScene(...)`.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase};

struct MainScene {
	scene_base: SceneBase,
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		graphic.set_fill_color("#3399ff");
		graphic.draw_rect(Rect::create(100.0, 100.0, 300.0, 200.0));
	}
}

struct SplashScene {
	scene_base: SceneBase,
	elapsed_seconds: f32,
	is_leaving: bool,
}

impl Scene for SplashScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.elapsed_seconds += time_delta;
		if self.is_leaving || self.elapsed_seconds < 0.5 {
			return;
		}
		let splash_scene_id = match self.scene_base.get_scene_id() {
			Some(splash_scene_id) => {
				splash_scene_id
			}
			None => {
				return;
			}
		};
		self.is_leaving = true;
		engine.spawn_task(move |mut engine_context| async move {
			let main_scene = MainScene {
				scene_base: SceneBase::new(),
			};
			engine_context.with(|engine| engine.load_scene_async(Box::new(main_scene))).await;
			engine_context.with(|engine| engine.unload_scene_async(splash_scene_id)).await;
		});
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		graphic.set_fill_color("#ffffff");
		graphic.draw_rect(Rect::create(540.0, 320.0, 200.0, 80.0));
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let splash_scene = SplashScene {
		scene_base: SceneBase::new(),
		elapsed_seconds: 0.0,
		is_leaving: false,
	};
	if let Err(error) = engine.run(Box::new(splash_scene)) {
		eprintln!("{}", error);
	}
}
```

- Unloading a scene loaded in the same frame (before `create`) removes it without calling `create` or `destroy`. That scene's `load_scene_async` `Promise` does not resolve.
- A scene unloaded during loading after `create` still receives `initialize` when loading finishes and is then dropped, as in the original. (Its `load_scene_async` `Promise` resolves at that point)
- The `unload_scene_async` `Promise` resolves even for an unknown handle.

## Time (TimeManager)

The `time_delta` in `tick(engine, time_delta)` is the seconds since the last frame multiplied by the time scale. One frame interval is clamped between 0 and an upper limit (0.25 seconds at first), so the first frame after the window was hidden or stalled for a long time does not jump far. In a scene, read it with `engine.get_time_manager()` and change the scale and limit with `engine.get_time_manager_mut()`.

| Function | Contents |
| --- | --- |
| `get_time_delta()` | Frame interval with the scale applied (seconds, `f32`). The value scene and component `tick` receive |
| `get_unscale_delta_time()` | Frame interval before the scale is applied (seconds, 0 to the limit) |
| `get_time()` | Accumulated time from adding the clamped intervals (seconds, `f64`, scale not applied) |
| `get_realtime_since_startup()` | Seconds since the engine started (the value at the last update, `f64`) |
| `get_frame_per_second()` | Frames counted during the last second (0 before the first second) |
| `set_time_scale(time_scale)`, `get_time_scale()` | Time scale (1 at first). At 0, `tick` is still called but `time_delta` is 0. Negative values are kept as given |
| `set_maximum_time_delta(seconds)`, `get_maximum_time_delta()` | Upper limit of one frame interval (0.25 seconds at first; below 0 or not finite becomes 0.25) |

`TimeManager::update` is called by the engine every frame with the seconds since the engine started. Creating a time manager separately and feeding it values directly shows how it counts.

```rust
use vanilla::TimeManager;

let mut time_manager = TimeManager::new();
time_manager.update(1.0);
assert_eq!(time_manager.get_time_delta(), 0.0);
time_manager.update(1.1);
time_manager.set_time_scale(0.5);
assert!((time_manager.get_unscale_delta_time() - 0.1).abs() < 1e-6);
assert!((time_manager.get_time_delta() - 0.05).abs() < 1e-6);
time_manager.update(5.0);
assert_eq!(time_manager.get_unscale_delta_time(), 0.25);
time_manager.set_maximum_time_delta(0.1);
time_manager.update(6.0);
assert!((time_manager.get_unscale_delta_time() - 0.1).abs() < 1e-6);
assert!((time_manager.get_time() - 0.45).abs() < 1e-6);
```

In a scene it looks like this.

```rust,no_run
use vanilla::{Engine, Scene, SceneBase};

struct TimeScene {
	scene_base: SceneBase,
}

impl Scene for TimeScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, engine: &mut Engine) {
		let time_manager = engine.get_time_manager_mut();
		time_manager.set_time_scale(0.5);
		time_manager.set_maximum_time_delta(0.1);
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		let time_manager = engine.get_time_manager();
		let frame_per_second = time_manager.get_frame_per_second();
		let time = time_manager.get_time();
		engine.set_title(&format!("{} FPS, {:.1} s", frame_per_second, time));
	}
}
```

## Screen and Reference Resolution (ViewManager, ViewScaleMode)

The engine holds one `ViewManager`. These are the sizes it handles.

| Name | Meaning |
| --- | --- |
| `get_canvas_native_size()` | Logical size of the canvas (the inside of the window on desktop) (physical pixels divided by the scale, rounded) |
| `get_device_pixel_ratio()` | Scale (the original `devicePixelRatio`; on desktop, the window's scale factor) |
| `get_canvas_pixel_size()` | Physical pixel size of the drawing buffer |
| `get_reference_resolution_size()` | Reference resolution |
| `get_view_native_rect()` | Rectangle the view occupies inside the canvas (logical pixels) |
| `get_view_size()` | View size used by game code. Node placement and input coordinates are in this unit |
| `get_target_resolution_scale()` | How many logical pixels one view unit is |

How the view is decided is chosen with `ViewScaleMode`. When the engine is created, `EngineConfiguration::view_scale_mode` (`StretchShort` at first) is applied.

| Mode | View size | Leftover |
| --- | --- | --- |
| `None` | Logical canvas size as is (no stretching) | None |
| `ReferenceResolution` | Reference resolution, centered without stretching | Both axes may be cut or leave space |
| `StretchWidth` | Reference resolution, width stretched to the canvas | Height is cut or leaves space |
| `StretchHeight` | Reference resolution, height stretched to the canvas | Width is cut or leaves space |
| `StretchShort` | Reference resolution, stretched to fit the short axis | Margins on the long axis |
| `StretchWidthExpandHeight` | Width is the reference resolution, height follows the screen ratio | None |
| `StretchHeightExpandWidth` | Height is the reference resolution, width follows the screen ratio | None |
| `StretchShortExpandLong` | Short axis is the reference resolution, long axis fills the screen | None |

`apply_aspect_view_scale_mode()` picks `StretchWidthExpandHeight` when the screen is taller than the reference resolution, otherwise `StretchHeightExpandWidth`, and recalculates the view right away.

```rust
use vanilla::{Rect, Vector2, ViewManager, ViewScaleMode};

let mut view_manager = ViewManager::new(Vector2::create(1280.0, 720.0), ViewScaleMode::StretchShort);
view_manager.calculate_view_rect(2560, 2000, 2.0);
assert_eq!(view_manager.get_canvas_native_size(), Vector2::create(1280.0, 1000.0));
assert_eq!(view_manager.get_canvas_pixel_size(), Vector2::create(2560.0, 2000.0));
assert_eq!(view_manager.get_view_size(), Vector2::create(1280.0, 720.0));
assert_eq!(view_manager.get_view_native_rect(), Rect::create(0.0, 140.0, 1280.0, 720.0));
assert_eq!(view_manager.canvas_position_to_view_position(Vector2::create(0.0, 140.0)), Vector2::zero());

view_manager.set_view_scale_mode(ViewScaleMode::StretchWidthExpandHeight);
view_manager.calculate_view_rect(2560, 2000, 2.0);
assert_eq!(view_manager.get_view_size(), Vector2::create(1280.0, 1000.0));

view_manager.set_view_scale_mode(ViewScaleMode::StretchShort);
view_manager.calculate_view_rect(2560, 2000, 2.0);
view_manager.apply_aspect_view_scale_mode();
assert_eq!(view_manager.get_view_scale_mode(), ViewScaleMode::StretchWidthExpandHeight);
```

`calculate_view_rect` is called by the platform when the window size or scale changes. Calling it directly as in the example above is for testing.

`Graphic`'s transform returns to identity at the start of every frame. Drawn as is, coordinates are physical pixels of the drawing buffer. A scene sets the coordinate system in `pre_draw`.

- `view_manager.apply_canvas_native_rect(graphic)`: the whole canvas in logical pixels. Used to fill the background to the canvas edges.
- `view_manager.apply_view_rect(graphic)`: view coordinates. (0, 0) is the top left of the view and the bottom right is `get_view_size()`. Touch coordinates a scene receives are in these coordinates too.
- `scene_base.draw_scene_background(engine, graphic)`: calls the two above in sequence. It fills the whole canvas with the scene background color and sets view coordinates. (Where the original has `GameScene.preDraw`)

```rust,no_run
use vanilla::{Color, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2, ViewScaleMode};

struct LetterboxScene {
	scene_base: SceneBase,
}

impl Scene for LetterboxScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color("#202028");
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		let view_size = engine.get_view_manager().get_view_size();
		graphic.set_fill_color(Color::create(0.15, 0.3, 0.5, 1.0));
		graphic.draw_rect(Rect::create(0.0, 0.0, view_size.x, view_size.y));
		graphic.set_fill_color("#ffffff");
		graphic.draw_rect(Rect::create(0.0, 0.0, 80.0, 80.0));
		graphic.draw_rect(Rect::create(view_size.x - 80.0, view_size.y - 80.0, 80.0, 80.0));
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let letterbox_scene = LetterboxScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(letterbox_scene)) {
		eprintln!("{}", error);
	}
}
```

Other uses:

- To change the mode or the reference resolution at run time, change them with `set_view_scale_mode` or `apply_reference_resolution_size`, then call `engine.resize()`. Both functions take effect from the next view calculation, and `Engine::resize` makes the platform recalculate the canvas and view at the start of the next frame and call `resize` on every scene.
- `canvas_position_to_view_position(canvas_position)`, `convert_client_to_view(client_x, client_y)`: convert canvas coordinates or screen (client) coordinates to view coordinates.
- `get_safe_area_insets()`: gives the margins that avoid notches and the home indicator as a `SafeAreaInsets` (`top`, `right`, `bottom`, `left`) in view coordinates. On desktop it is 0 like Chrome desktop, and it can be simulated with `set_safe_area_override` or the environment variable `VANILLA_SAFE_AREA=top,right,bottom,left`. (On the web, the URL's `?safeArea=`) Sides to ignore are set with `set_ignored_safe_area_sides`.
- `set_max_render_pixel_ratio(ratio)`: caps the scale so the drawing buffer does not get too large on high-density devices. 0 means no limit. On desktop it draws at the reduced size and then stretches to the window size.
- `get_css_pixel_scale()`: gives how many CSS pixels one view unit is on screen.

## Page Events

When the page is hidden and shown again, when the window loses and gains focus, and when the window is pressed, the engine immediately passes a `PageEvent` value to the registered listeners.

| Value | When |
| --- | --- |
| `PageEvent::Hidden` | The page is hidden (on the web another tab or app switch; on desktop the window is covered or minimized) |
| `PageEvent::Visible` | The page is visible again |
| `PageEvent::Blurred` | The window lost focus |
| `PageEvent::Focused` | The window gained focus |
| `PageEvent::PointerDown` | The window was pressed |

A listener is a `PageEventListener` (`Rc<RefCell<dyn FnMut(PageEvent)>>`), and the engine holds it by weak reference. The caller must keep the listener alive; once dropped, it is no longer called. A listener does not receive the engine, so share scene state through `Rc`. `engine.is_page_hidden()` tells whether the last event was `Hidden`.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use vanilla::{Engine, EngineConfiguration, PageEvent, PageEventListener};

let mut engine = Engine::new(EngineConfiguration::new());
let received_list: Rc<RefCell<Vec<PageEvent>>> = Rc::new(RefCell::new(Vec::new()));
let listener_received_list = received_list.clone();
let page_event_listener: PageEventListener = Rc::new(RefCell::new(move |page_event: PageEvent| {
	listener_received_list.borrow_mut().push(page_event);
}));
engine.add_page_event_listener(&page_event_listener);
engine.dispatch_page_event(PageEvent::Hidden);
assert!(engine.is_page_hidden());
engine.dispatch_page_event(PageEvent::Visible);
assert!(!engine.is_page_hidden());
engine.remove_page_event_listener(&page_event_listener);
engine.dispatch_page_event(PageEvent::Focused);
assert_eq!(*received_list.borrow(), vec![PageEvent::Hidden, PageEvent::Visible]);
```

`dispatch_page_event` is usually called by the platform, and a game may call it too. A scene has two hooks to override. `on_page_visibility_restored` is called when the page is visible again, and `on_page_process_restored` when a stopped page comes back to life (the web's `resume` event, a `pageshow` returning from the back-forward cache). Before calling either, the engine wakes the audio context and clears pressed keys and touch state.

```rust,no_run
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use vanilla::{Engine, PageEvent, PageEventListener, Scene, SceneBase};

struct PausingScene {
	scene_base: SceneBase,
	is_paused: Rc<Cell<bool>>,
	page_event_listener: Option<PageEventListener>,
	play_seconds: f32,
}

impl Scene for PausingScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, engine: &mut Engine) {
		let is_paused = self.is_paused.clone();
		let page_event_listener: PageEventListener = Rc::new(RefCell::new(move |page_event: PageEvent| {
			match page_event {
				PageEvent::Hidden => {
					is_paused.set(true);
				}
				PageEvent::Visible => {
					is_paused.set(false);
				}
				_ => {
				}
			}
		}));
		engine.add_page_event_listener(&page_event_listener);
		self.page_event_listener = Some(page_event_listener);
		println!("hidden at start {}", engine.is_page_hidden());
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		if self.is_paused.get() {
			return;
		}
		self.play_seconds += time_delta;
	}

	fn finalize(&mut self, engine: &mut Engine) {
		if let Some(page_event_listener) = self.page_event_listener.take() {
			engine.remove_page_event_listener(&page_event_listener);
		}
	}

	fn on_page_visibility_restored(&mut self, _engine: &mut Engine) {
		println!("page visible again");
	}
}
```

Some things differ by platform.

- Web: `visibilitychange` gives `Hidden` and `Visible`, `blur` gives `Blurred`, `focus` gives `Focused`, `pointerdown` gives `PointerDown`.
- Desktop: if any of these holds, it is `Hidden`: the window is covered (winit reports it on macOS and X11), minimized (how Windows reports it), or Wayland frame callbacks have not arrived for more than 3 seconds; when all clear, it is `Visible` and `on_page_visibility_restored`. winit on Windows, macOS and Linux does not send app suspension, so `on_page_process_restored` is not called on desktop.

## GameScene and UIScene

Instead of implementing `Scene` directly, a scene can embed a base that already does common work. Rust has no inheritance, so you embed the base struct as a field, implement the trait, and get `Scene` with one macro line.

| Base | Trait and macro | What it does |
| --- | --- | --- |
| `GameSceneBase` | `GameScene`, `vanilla::game_scene!` | Loading screen (center image, gauge, press to skip, at least 3 seconds), passing touches to nodes through `TouchRecognizer`, a safe area node, calling `layout` again when the size changes, filling the canvas background and setting view coordinates in `pre_draw`, screen fades, play input the UI did not take, developer tools |
| `UISceneBase` | `UIScene`, `vanilla::ui_scene!` | Embeds `GameSceneBase` and adds a constraint solver (`LayoutSolver`), a screen guide and a safe area guide (`UINode`) |

The `Scene` functions the macro creates call the base first and then the trait function of the same name. (Like overriding by calling `super` first in the original) Only `post_draw` calls the scene's first and draws the base (fade curtain, developer tools) on top.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, GameScene, GameSceneBase, Graphic, Rect, ViewScaleMode};

struct TitleScene {
	game_scene_base: GameSceneBase,
}

impl GameScene for TitleScene {
	fn get_game_scene_base(&self) -> &GameSceneBase {
		&self.game_scene_base
	}

	fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
		&mut self.game_scene_base
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.set_fill_color("#ffffff");
		graphic.draw_rect(Rect::create(540.0, 320.0, 200.0, 80.0));
	}
}

vanilla::game_scene!(TitleScene);

fn main() {
	let mut game_scene_base = GameSceneBase::new();
	game_scene_base.set_view_scale_mode(ViewScaleMode::StretchShort, None);
	game_scene_base.set_loading_min_duration_ms(0.0);
	let title_scene = TitleScene {
		game_scene_base,
	};
	let engine = Engine::new(EngineConfiguration::new());
	if let Err(error) = engine.run(Box::new(title_scene)) {
		eprintln!("{}", error);
	}
}
```

`UIScene` and constraint-based layout are in the "UI" chapter.

## Cautions

- The root node is created by `SceneBase::new()`. (The original creates it in `create()`) Fitting the root to the view size is done by the engine right after `create`.
- Even after the handed-over assets and tasks finish, `initialize` and `tick` start only 0.3 seconds later. Desktop reads assets on the spot, so with no handed-over tasks it is 0.3 seconds after `create`.
- On the web assets arrive later, so create nodes that use assets in `initialize`.
- Overriding `tick` or `draw` drops what the default did (updating and drawing the node tree, dispatching touches, tweens). Call `self.scene_base.tick(engine, time_delta)` and `self.scene_base.draw(graphic)` yourself when needed.
- The view scale mode of `GameSceneBase` and `UISceneBase` is `ViewScaleMode::None` at first and is applied to the view manager in `initialize`. Even if `EngineConfiguration::view_scale_mode` is set, these scenes switch to their own mode, so set it with `set_view_scale_mode` to draw in the reference resolution.
- Modes other than `ViewScaleMode::None` divide by the reference resolution. With a reference resolution of 0, the view cannot be calculated.
- Loading the next scene without unloading the current one makes the two scenes run and draw together.
- A listener dropped after being passed to `add_page_event_listener` is no longer called. Keep it somewhere alive, such as a scene field.
- The engine does not catch panics raised in a scene.

## Related Types

Engine, EngineConfiguration, Scene, SceneBase, SceneId, SceneContext, EngineContext, Promise, TimeManager, ViewManager, ViewScaleMode, SafeAreaInsets, SafeAreaSide, PageEvent, PageEventListener, GameScene, GameSceneBase, UIScene, UISceneBase, Graphic
