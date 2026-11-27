# Project Setup

This chapter covers adding the engine to a game crate as a dependency, feature flags and build profiles, the project folder layout and asset folders, the smallest `main.rs`, and running on desktop and building for the web.

## Engine Dependency

The engine is a single crate. Add it to the game's `Cargo.toml`.

```toml
[package]
name = "mygame"
version = "0.1.0"
edition = "2021"

[dependencies]
vanilla-engine = { git = "https://github.com/ddukbaek2/vanilla-engine" }
```

- The crate name is `vanilla-engine`, and the name used in code is `vanilla`. (`use vanilla::{Engine, Scene};`)
- The reflection macro is in a separate crate (`vanilla-engine-macros`), but the engine re-exports it as `vanilla::reflect`. A game depends on the engine alone.
- A git dependency fetches the repository's default branch. To pin a version, use Cargo's `branch`, `tag` or `rev` keys. To use an engine repository cloned next to your game while changing it, point to it with `path`.

```toml
[dependencies]
vanilla-engine = { path = "../vanilla-engine" }
```

## Feature Flags

| Feature | Default | What it does |
| --- | --- | --- |
| `tls` | On | The desktop `WebSocketClient` opens `wss://`. (ewebsock's tls, rustls with the ring crypto provider) The web uses the browser's WebSocket, so it is unaffected |

Turned off with `default-features = false`, the desktop opens only `ws://`. Because ring builds C code, also turn it off when checking the Windows target on a Mac without Windows C headers.

```toml
[dependencies]
vanilla-engine = { git = "https://github.com/ddukbaek2/vanilla-engine", default-features = false }
```

```bash
rustup target add x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc --no-default-features
```

## Build Profiles

The `[profile.release]` and `[profile.wasm-release]` in the engine's `Cargo.toml` apply only when the engine repository itself is the top-level build. Cargo does not read the profiles of dependency crates, so to use the same settings, copy them into the game's `Cargo.toml`.

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"

[profile.wasm-release]
inherits = "release"
strip = false
```

- `release` optimizes everything as one unit and strips debug symbols.
- `wasm-release` is the same as `release` but keeps symbols so wasm-bindgen can read the wasm feature information. wasm-bindgen does the size reduction.
- Desktop AVIF images are decoded with rav1d. rav1d is slow at opt-level 0 (one 1600 x 1200 image takes 2.9 seconds in a dev build, 0.9 seconds with only rav1d at opt-level 3), so optimizing just rav1d is recommended even in dev builds.

```toml
[profile.dev.package.rav1d]
opt-level = 3
```

## Project Folder Layout

The engine does not impose a layout. Below is one example.

```text
mygame/
	Cargo.toml
	src/
		main.rs
	assets/
		images/
			logo.png
	web/
		index.html
```

| Location | Use |
| --- | --- |
| `src/main.rs` | Entry file. Creates the engine configuration and calls `Engine::run` with the first scene |
| `assets/` | Asset folder. Desktop points to it with `EngineConfiguration::asset_directory_paths`, the web with `asset_base_url` |
| `web/` | Output location of the web build. Put `index.html` next to the `.js` and `.wasm` produced by wasm-bindgen |

## Asset Folders

Assets are requested by paths relative to the asset folder (such as `"images/logo.png"`). The same game code finds them like this on each platform.

| Platform | How it finds them |
| --- | --- |
| Desktop | Checks the candidates in `asset_directory_paths` from the front and reads from the first candidate that has the file. If none has it, the path is based on the first candidate; with no candidates, the relative path is used as is |
| Web | Fetches the URL made by joining `asset_base_url` and the relative path. When empty (the default), it is a URL relative to the page |

Relative candidates on desktop are based on the working directory at run time. `cargo run` keeps the directory it was called from as the working directory, so to cover opening the executable from elsewhere you can also add the folder next to the executable as a candidate.

```rust
use std::path::PathBuf;
use vanilla::{Engine, EngineConfiguration};

let mut engine_configuration = EngineConfiguration::new();
let mut asset_directory_paths = vec![PathBuf::from("assets")];
if let Ok(executable_path) = std::env::current_exe() {
	if let Some(executable_directory) = executable_path.parent() {
		asset_directory_paths.push(executable_directory.join("assets"));
	}
}
engine_configuration.asset_directory_paths = asset_directory_paths;
let engine = Engine::new(engine_configuration);
assert_eq!(engine.resolve_asset_path("images/logo.png"), PathBuf::from("assets/images/logo.png"));
```

`Engine::resolve_asset_path` returns the path actually read on desktop. In this example no candidate has the file, so the path based on the first candidate comes out.

## The Smallest main.rs

```rust,no_run
use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};

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

	fn pre_draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		engine.get_view_manager().apply_view_rect(graphic);
	}

	fn draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		graphic.set_fill_color(Colors::VANILLA);
		graphic.draw_rect(Rect::create(40.0, 40.0, 240.0, 120.0));
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("My Game");
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- `Graphic`'s transform returns to identity at the start of every frame. Drawn as is, coordinates are actual pixels of the drawing buffer.
- Calling `ViewManager::apply_view_rect` in `pre_draw` switches the following drawing to view coordinates. (0, 0) is the top left of the view, and the bottom right is `ViewManager::get_view_size`.
- This example paints no background, so the screen stays the black cleared at the start of the frame. To fill the background color to the window edges, call `SceneBase::draw_scene_background`. (It sets view coordinates after filling)
- Calling `self.scene_base.draw(graphic)` in the overridden `draw` also draws the scene's node tree.
- Reference resolution, view scale modes and the order of scene hooks are in the "Engine and Scene Lifecycle" chapter.

## Running and Shipping on Desktop

```bash
cargo run
cargo run --release
cargo build --release
```

- The release executable is created in `target/release/`. Ship the asset folder along with the executable and make `asset_directory_paths` point to it.
- Window settings are fields of `EngineConfiguration`.

| Field | Default | Meaning |
| --- | --- | --- |
| `title` | The engine's Korean name | Window title |
| `window_size` | `Vector2::zero()` | Initial window size (logical size, 0 means the reference resolution) |
| `is_resizable` | `true` | Whether the window can be resized |
| `use_vertical_sync` | `true` | Vertical sync |
| `window_icon_path` | `""` | Window icon PNG (relative to the asset folder, empty means the default icon) |
| `default_font_paths` | Empty list | Default font files. When empty, operating system fonts are searched in the order of the original's system font names |

## Web Build

You need the `wasm32-unknown-unknown` target and the wasm-bindgen command-line tool. The version of the wasm-bindgen command-line tool must match the `wasm-bindgen` crate version locked in `Cargo.lock`.

```bash
rustup target add wasm32-unknown-unknown
cargo build --profile wasm-release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web target/wasm32-unknown-unknown/wasm-release/mygame.wasm
```

`web/index.html`:

```html
<!doctype html>
<html>
<head><meta charset="utf-8"><style>html, body { margin: 0; overflow: hidden; background: #000; }</style></head>
<body>
<script type="module">
import init from "./mygame.js";
init();
</script>
</body>
</html>
```

- Place assets where the page reaches them at the URL joined with `asset_base_url`. To use the same paths in game code (`"images/logo.png"`) on both platforms, set `asset_directory_paths` to `assets` on desktop and `asset_base_url` to `"assets/"` on the web, and copy the assets into `web/assets/`.
- The canvas is looked up by `canvas_id` (`mainCanvas` at first); if missing, the engine creates one with that id and appends it to the end of `body`. To use a canvas already in the page, give it the same id.
- Browsers block module scripts and asset fetches on a page opened from `file://`, so open it through a local web server.

Fields used only on the web:

| Field | Default | Meaning |
| --- | --- | --- |
| `canvas_id` | `"mainCanvas"` | Id of the canvas to draw in |
| `asset_base_url` | `""` | Prefix for asset URLs |
| `default_font_url` | `""` | Default font URL. Fetched when `default_font_bytes` is not set. When empty, nothing is fetched and the browser's system font is used |
| `resume_audio_on_touch_end` | `false` | Whether `touchend` and `pointerup` also count as user gestures that wake the audio |

## Cautions

- Profiles in the engine's `Cargo.toml` do not carry over to the game build. Put the release settings and `wasm-release` in the game's `Cargo.toml`.
- Relative desktop asset folder paths are based on the working directory at run time. Opening the executable from another location may fail to find assets.
- The engine ships and fetches no font of its own. The default of `default_font_url` is an empty string (same as the original `defaultFontUrl`). Give a game font with one of `default_font_url`, `default_font_bytes` or `FontAsset`.
- `title` is the desktop window title. On the web this field is not written to the document title at startup, so set it with `<title>` in `index.html` or call `Engine::set_title` at run time.
- When checking the Linux target, cpal's alsa-sys and gilrs's libudev-sys call pkg-config. To check on a Mac, pass a pkg-config that prints empty flags through `PKG_CONFIG` and check with `PKG_CONFIG_ALLOW_CROSS=1`.

## Related Types

Engine, EngineConfiguration, Scene, SceneBase, ViewManager, Graphic, Colors
