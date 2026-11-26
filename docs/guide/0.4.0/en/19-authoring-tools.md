# Authoring Tools and Files Shared by Both Engines

This chapter covers how the Rust version loads files made by the authoring tools that come with the original vanilla.js. The Rust version has no editors of its own. It reads the files saved by the vanilla.js editors and command-line tools in the same format as the original. It walks through the UI Editor's `.uiasset.json`, the Timeline Editor's `.timeline.json`, the Particle Editor's `.vfx.json`, and the Visual Editor's `.scene.json` and `.prefab.json`, and ends with other files both engines share, such as sprite atlases, language files, font lists, and saved values. The tool list and where to open each tool are in the "Tools" chapter.

## Tools and files

| Tool | Demo | File it makes | Where the Rust version reads it |
| --- | --- | --- | --- |
| UI Editor | [Open](https://ddukbaek2.com/portfolio/uieditor/) | `.uiasset.json` (default name `ui.uiasset.json`) | `UIDocument::from_json_text` |
| Timeline Editor | [Open](https://ddukbaek2.com/portfolio/timelineeditor/) | `.timeline.json` (default name `titlecard.timeline.json`) | `Timeline::load_from_text`, `load_from_url`, `create_from_json_asset` |
| Particle Editor | [Open](https://ddukbaek2.com/portfolio/particleeditor/) | `.vfx.json` (default name `effect.vfx.json`) | `ParticleSystem::apply_description` |
| Visual Editor | [Open](https://ddukbaek2.com/portfolio/visualeditor/) | `scenes/*.scene.json`, `prefabs/*.prefab.json` | `UIDocument::from_json_text` |

- Each editor is `tools/<tool name>/<tool name>.html` in the [vanilla.js repository](https://github.com/ddukbaek2/vanilla.js). It is a build that bundles the engine into one file, so it works when opened as a file without a server.
- The UI Editor, Timeline Editor, and Particle Editor save through a browser download. The Visual Editor writes straight into a folder opened with the folder picker in Chromium-based browsers, and uses a download in other browsers.
- Put the saved files in your game's asset folder (relative to `EngineConfiguration.asset_directory_paths` on the desktop and `asset_base_url` on the web) and load them as shown below.

## Loading UI Editor documents

A `.uiasset.json` is a UI document that writes out a node tree as is. It consists of a version number and one root node, and each node lists its type, name, active state, position, scale, rotation, opacity, pivot, anchor, content size, touch handling, components, and children.

```json
{
	"version": 1,
	"root": {
		"type": "WorldNode",
		"name": "Root",
		"position": [0, 0],
		"contentSize": [960, 640],
		"components": [],
		"children": [
			{
				"type": "WorldNode",
				"name": "StartButton",
				"position": [40, 300],
				"contentSize": [240, 64],
				"components": [
					{ "type": "Paint", "color": [0.25, 0.5, 0.9, 1], "roundSize": 12 },
					{ "type": "UIButton" },
					{ "type": "UILabel", "text": "Start", "fontSize": 24 }
				],
				"children": []
			}
		]
	}
}
```

The original `UIDocument` is a static class with module-global tables. The Rust version creates a `UIDocument` value that holds the type tables. `UIDocument::new()` registers the engine components. `from_json_text(node_tree, json_text)` builds the document text into the node tree and returns the `NodeId` of a root that is not attached to any parent. It returns `Err` if the text is not JSON or has no `root` object. To show it, attach it to the scene's root with `add_child`, then find nodes by name and attach behavior. `UIDocument::find_node_by_path(node_tree, root_id, path_text)` finds a node by a path of names joined with `/` from the document root (leaving out the root's name and starting with `/`), and returns `None` if not found.

```rust
use vanilla::{NodeTree, Paint, UIButton, UIDocument, UILabel};

let json_text = r#"{
	"version": 1,
	"root": {
		"type": "WorldNode",
		"name": "Root",
		"contentSize": [960, 640],
		"components": [],
		"children": [
			{
				"type": "WorldNode",
				"name": "StartButton",
				"position": [40, 300],
				"contentSize": [240, 64],
				"components": [
					{ "type": "Paint", "color": [0.25, 0.5, 0.9, 1], "roundSize": 12 },
					{ "type": "UIButton" },
					{ "type": "UILabel", "text": "Start", "fontSize": 24 }
				],
				"children": []
			}
		]
	}
}"#;

let mut node_tree = NodeTree::new();
let mut document = UIDocument::new();
let document_root_id = document.from_json_text(&mut node_tree, json_text).unwrap();
let root_id = node_tree.get_root_id();
node_tree.add_child(root_id, document_root_id);

let start_button_id = UIDocument::find_node_by_path(&node_tree, document_root_id, "/StartButton").unwrap();
let start_button_node = &node_tree[start_button_id];
assert!(start_button_node.get_component::<Paint>().is_some());
assert!(start_button_node.get_component::<UIButton>().is_some());
assert_eq!(start_button_node.get_component::<UILabel>().unwrap().get_text(), "Start");
assert!(UIDocument::find_node_by_path(&node_tree, document_root_id, "/Missing").is_none());
```

- Widgets that create a content node, such as scroll views, create their inner content node themselves when attached. So the document lists only the widget, writes the children placed inside the content (list items and so on) separately as `contentChildren`, and attaches them under the content when restoring. The content node is named `content`, so include it in paths. (For example `/Panel/ItemList/content/Item0`)
- Values that point to other nodes (node references) are written in the document as `$nodeRef` with a path from the root, and are connected after the whole tree is built.
- On read, components and children that a node already has (built when a node type or widget was attached) receive the document values if their order and type match, and are not built again. Unlike the original, nothing is attached twice.
- Unknown keys such as the `frameSize` the editor adds are skipped.

### Resolving image paths

Documents contain only image paths (`imagePath`), not the images themselves. `Sprite`, `UIImageView`, and `ShadowImage` use this field. There are two ways to connect images.

- Reading from the asset folder: calling `load_images(engine, json_text)` before `from_json_text` requests an `ImageAsset` for every `imagePath` in the document, keeps them in the document, and returns the newly requested load handles. If a scene calls it in `create`, it can hand the handles to `track_load_handle` so that `initialize` waits for them. On the desktop they load on the spot. When they arrive later, as on the web, the document holds on to images that have not loaded yet and `bind_pending_images(node_tree)` attaches them. Call it from the scene's `tick` until the remaining count reaches 0.
- A resolver: registering a path to `Option<Rc<Image>>` function with `set_image_resolver(resolver)` makes lookups use it instead of the asset store. It must return right away, so load the images first.

Images that cannot be found print a warning and are dropped, and the component stays without an image, as in the original.

```rust
use std::rc::Rc;
use vanilla::{Image, NodeTree, Sprite, UIDocument};

let star_image = Image::create_from_pixels(1, 1, vec![255, 255, 255, 255]);
let resolved_image = star_image.clone();
let mut document = UIDocument::new();
document.set_image_resolver(move |image_path: &str| {
	if image_path == "images/star.png" {
		Some(resolved_image.clone())
	}
	else {
		None
	}
});

let json_text = r#"{"version":1,"root":{"name":"Star","components":[{"type":"Sprite","imagePath":"images/star.png"}]}}"#;
let mut node_tree = NodeTree::new();
let star_id = document.from_json_text(&mut node_tree, json_text).unwrap();
let sprite = node_tree[star_id].get_component::<Sprite>().unwrap();
assert!(sprite.get_image().is_some_and(|image| Rc::ptr_eq(image, &star_image)));
```

### Registering component and node types

The components `UIDocument::new()` knows from the start are those in the original table, `Paint`, `Sprite`, `Text`, `RichText`, `Mask`, `Outline`, `SoftShade`, `ShadowText`, `UIProgressRing`, `UILabel`, `UIImageView`, `UIButton`, `UIToggleButton`, `UIProgressView`, `UISlider`, `UIScrollView`, plus `ShadowImage`, which the Rust version adds (`get_component_type_names()`). There are no node types. The UI Editor registers every component and `WorldNode`-derived node the engine exports into the document format. So if a type used in the editor is missing from the game's table, that component prints a warning and is dropped on read, and that node comes back as a plain node.

The Rust version registers the same types as the editor (35 components, 12 node types) with one call to `register_editor_types()`. Games that read documents saved by the editors call it once. You can also register types one by one.

- The original node types (classes deriving from `WorldNode`, `registerNodeType`) are components attached to a node in the Rust version. Registered with `register_node_type(type_name, create_component)`, they are written and read in the same form as original documents: the `type` of a node with that component is that name, and the component is not listed among the components. (`UIInputField`, `UIScrollBar`, `UIWindow`, and so on)
- Register components with `register_component_type(type_name, create_component)`. Types with their own write and read functions use `register_component_type_with_handler`, and widgets that create a content node use `register_widget_type`.
- Components without dedicated save rules are saved and restored through values read and written by matching getters and setters (`save_by_reflection`, `load_by_reflection`). The original walks objects at run time, and the Rust version uses a list the `#[vanilla::reflect]` macro builds at compile time. Every engine component has it, and game components need the macro to be handled in documents. (See the "Reflection Macro" chapter.)

```rust
use vanilla::{NodeTree, UIDocument, UIInputField};

let mut document = UIDocument::new();
assert!(!document.get_component_type_names().contains(&"UISnapScrollView"));
document.register_editor_types();
let component_type_names = document.get_component_type_names();
assert!(component_type_names.contains(&"UISnapScrollView"));
assert!(component_type_names.contains(&"ParticleSystem"));

let json_text = r#"{"version":1,"root":{"type":"UIInputField","name":"NameField","contentSize":[320,48]}}"#;
let mut node_tree = NodeTree::new();
let field_id = document.from_json_text(&mut node_tree, json_text).unwrap();
assert!(node_tree[field_id].get_component::<UIInputField>().is_some());
```

### Saving documents from the game

`to_json_text(node_tree, root_id, is_pretty)` turns a node tree into text of the same form. Key order and indentation (tabs) match the original `JSON.stringify`, and numbers are written as the shortest decimal of an `f32`. Registered types are written under their registered names. Unregistered components are written through getters and setters as in the original, but without `#[vanilla::reflect]` they cannot be written, so a warning is printed and they are left out. There is no need to worry about a bundler renaming classes as in the original. To write components that have `&mut self` getters, use `to_json_text_mut`.

## Playing Timeline Editor documents

A `.timeline.json` is a document made of a list of stage nodes, tracks (target name, property, keys), and markers. At the top level it has `name`, `duration` (seconds), `frameRate`, `loop`, `stage` (`width`, `height`, `backgroundColor`, `nodes`), `tracks`, and `markers`.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use vanilla::{NodeTree, Timeline, TimelineMarker};

let timeline_text = r##"{
	"name": "Logo",
	"duration": 1,
	"frameRate": 30,
	"loop": false,
	"stage": { "width": 960, "height": 540, "backgroundColor": "#101114ff", "nodes": [
		{ "name": "Emblem", "type": "paint", "width": 120, "height": 120, "color": "#ffcc33ff" }
	] },
	"tracks": [
		{ "target": "Emblem", "property": "x", "keys": [{ "time": 0, "value": 0 }, { "time": 1, "value": 480 }] }
	],
	"markers": [{ "time": 0.5, "name": "half" }]
}"##;

let mut timeline = Timeline::load_from_text(timeline_text).unwrap();
let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let node_table = timeline.build_stage(&mut node_tree, root_id, None);

let event_list = Rc::new(RefCell::new(Vec::new()));
let marker_event_list = event_list.clone();
timeline.set_marker_handler(Some(Box::new(move |marker_name: &str, _marker: &TimelineMarker, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
	marker_event_list.borrow_mut().push(format!("marker {}", marker_name));
})));
let complete_event_list = event_list.clone();
timeline.set_complete_handler(Some(Box::new(move |_timeline: &mut Timeline, _node_tree: &mut NodeTree| {
	complete_event_list.borrow_mut().push(String::from("complete"));
})));

timeline.play();
timeline.tick(&mut node_tree, 0.6);
timeline.tick(&mut node_tree, 0.6);
assert_eq!(*event_list.borrow(), vec![String::from("marker half"), String::from("complete")]);
assert_eq!(node_tree[node_table["Emblem"]].get_local_position().x, 480.0);
assert!(!timeline.is_playing());
```

- Reading: `Timeline::load_from_text(text)` (from JSON text, `Err` on failure), `Timeline::create_from_json_asset(&mut json_asset)` (from a loaded `JsonAsset`, clones the description, `None` if not loaded yet), and `Timeline::load_from_url(engine, url)` (the original `loadFromUrl`, a future that yields `Result<Timeline, String>` when awaited inside an engine task). The easiest pattern is for the scene to request a `JsonAsset` in `create`, hand it to `track_asset`, and build the timeline with `create_from_json_asset` in `initialize`. Loading it together with other assets through `AssetLoader` also works.
- Stage: `build_stage(node_tree, parent_id, image_resolver)` creates the document's stage nodes, attaches them under `parent_id`, and returns a name to `NodeId` table (`HashMap`). To hook tracks onto a node tree you already built, `bind(node_tree, root_id)` finds and attaches the nodes whose names match the tracks' target names. Targets are `TimelineTarget`: nodes are `Node(NodeId)`, and things that are not nodes (such as screen effects) are `External`, set with `set_target(name, target)` or `set_target_resolver(resolver)`.
- Images: images imported in the editor go into the document's `image` field as `data:` URLs, and the runtime decodes them itself (on the spot on the desktop, and when the browser finishes decoding on the web). Images referenced by name must be returned by `image_resolver`. Pass the resolver as in `Some(&|image_name: &str| image_table.get(image_name).cloned())`.
- Playback: `play()`, `pause()`, `stop(node_tree)` (back to the start), `seek(node_tree, time)` (fires no events), and `tick(node_tree, time_delta)` every frame. Functions that move nodes also take the `NodeTree`. Change looping and speed with `set_loop` and `set_speed`.
- Notifications: `set_marker_handler`, `set_event_handler`, and `set_complete_handler` receive the timeline itself last, as in the original, followed by the node tree. Completion fires when the end is reached without looping. Handlers cannot borrow the scene, so put shared values in `Rc<RefCell>`.
- For documents with sound nodes, instead of the original `Timeline.setAudioManager`, call `load_sounds(node_tree, engine)` once after building the stage to load an audio asset for each sound.

The detailed rules for tracks, easing, and keys are covered in the "Tweens, Animation, and Timelines" chapter.

## Using Particle Editor assets

A `.vfx.json` is a list of particle system descriptions. Each system lists its name together with the description fields that `ParticleSystem::apply_description` accepts. The fields use the same names as the original (`looping`, `emissionRate`, `shape`, `lifetime`, `renderShape`, `blendMode`, and so on), and shapes and blend modes written as the original's strings are read into enums. Unknown shape and blend mode names are ignored.

```json
{
	"systems": [
		{ "name": "Fire", "looping": true, "duration": 1, "emissionRate": 60, "shape": "cone", "lifetime": [0.6, 1.2], "speed": [80, 180], "renderShape": "circle", "blendMode": "lighter" }
	]
}
```

In the engine, create one node per system, attach a `ParticleSystem`, and apply the description. A `ParticleSystem` is playing from the start. In a game, load the file as a `JsonAsset` and loop over `systems` from `get_data()` as below. The editor also reads the old form without `systems` (an object with just one description), so for such files use the single object as the description.

```rust
use vanilla::{NodeTree, ParticleSystem, Vector2};

let vfx_asset_text = r#"{
	"systems": [
		{
			"name": "Sparks",
			"looping": true,
			"duration": 1,
			"emissionRate": 80,
			"shape": "circle",
			"shapeRadius": 10,
			"lifetime": [0.4, 0.8],
			"speed": [120, 260],
			"size": [3, 6],
			"gravity": [0, 300],
			"startColorA": [1, 0.9, 0.4, 1],
			"startColorB": [1, 0.5, 0.1, 1],
			"colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 0.3, 0.05, 0] }],
			"renderShape": "streak",
			"blendMode": "lighter"
		}
	]
}"#;

let vfx_asset: serde_json::Value = serde_json::from_str(vfx_asset_text).unwrap();
let description_list = match vfx_asset.get("systems").and_then(|systems| systems.as_array()) {
	Some(system_array) => {
		system_array.clone()
	}
	None => {
		vec![vfx_asset.clone()]
	}
};

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
for description in description_list.iter() {
	let system_name = description["name"].as_str().unwrap_or("ParticleSystem");
	let system_id = node_tree.create_child(root_id, system_name);
	let system_node = &mut node_tree[system_id];
	system_node.set_local_position(Vector2::create(640.0, 360.0));
	let particle_system = system_node.add_component(ParticleSystem::new());
	particle_system.apply_description(description);
	particle_system.simulate(0.5);
	assert!(particle_system.get_particle_count() > 0);
}
assert!(node_tree.find_child_by_name(root_id, "Sparks").is_some());
```

If a particle image was imported in the editor, it goes into the description's `imageDataUrl` field as a `data:` URL. Like the original, `apply_description` does not read this field, so load the image with `ImageAsset::load(engine, data_url)` (a `data:` URL works in place of an asset path) and, once it has loaded, pass it with `set_image(image_asset.get_image())`.

```rust
use vanilla::{Engine, EngineConfiguration, ImageAsset, ParticleSystem};

let engine = Engine::new(EngineConfiguration::new());
let description = serde_json::json!({
	"name": "Dust",
	"renderShape": "image",
	"imageDataUrl": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
});

let mut particle_system = ParticleSystem::new();
particle_system.apply_description(&description);
let mut image_asset = ImageAsset::new();
if let Some(image_data_url) = description["imageDataUrl"].as_str() {
	image_asset.load(&engine, image_data_url);
}
particle_system.set_image(image_asset.get_image());
assert!(image_asset.get_image().is_some());
```

The desktop decodes `data:` URLs on the spot. On the web they load later, so set the image once `get_image()` returns `Some` (or the load handle finishes). The meaning of the description fields is covered in the "Particles" chapter.

## Loading Visual Editor scenes

The Visual Editor saves scenes in the same form as UI documents. The default save path is `scenes/untitled.scene.json`, and the editor-only frame size (`frameSize`) sits in the same JSON. `UIDocument` skips unknown keys, so the game reads the file as is with `from_json_text`. The scene root is a node named `sceneRoot` with its pivot at the top left. The Visual Editor's `Sprite` writes the image's project-relative path into `imagePath`, so if the asset folder is the same project folder, `load_images` reads the images by those paths.

The example below reads `examples/assets/scenedocument.scene.json` from the repository (a scene made in the Visual Editor save format). The scene requests the scene file as a `TextAsset` in `create` and tracks it, requests the images and builds the tree in `initialize`, and attaches late images in `tick`. The function given to `set_clicked_event` cannot borrow the scene, so the button press is signaled through an `Rc<Cell>` and picked up in `tick`. Some images in this scene file are not in the repository, so they print a warning and stay without an image.

```rust,no_run
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::{ComponentContext, Engine, EngineConfiguration, Graphic, NodeId, Scene, SceneBase, TextAsset, UIButton, UIDocument, UILabel};

struct VisualScene {
	scene_base: SceneBase,
	scene_asset: TextAsset,
	document: UIDocument,
	label_id: Option<NodeId>,
	is_button_clicked: Rc<Cell<bool>>,
}

impl Scene for VisualScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.scene_asset.load(engine, "scenedocument.scene.json");
		self.scene_base.track_asset(&self.scene_asset);
	}

	fn initialize(&mut self, engine: &mut Engine) {
		let scene_text = match self.scene_asset.get_text() {
			Some(scene_text) => {
				scene_text.to_string()
			}
			None => {
				return;
			}
		};
		self.document.register_editor_types();
		self.document.load_images(engine, &scene_text);
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let scene_root_id = match self.document.from_json_text(node_tree, &scene_text) {
			Ok(scene_root_id) => {
				scene_root_id
			}
			Err(error) => {
				eprintln!("{}", error);
				return;
			}
		};
		node_tree.add_child(root_id, scene_root_id);

		self.label_id = UIDocument::find_node_by_path(node_tree, scene_root_id, "/Panel/label");
		if let Some(button_id) = UIDocument::find_node_by_path(node_tree, scene_root_id, "/Panel/button") {
			if let Some(button) = node_tree[button_id].get_component_mut::<UIButton>() {
				let is_button_clicked = self.is_button_clicked.clone();
				button.set_clicked_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
					is_button_clicked.set(true);
				}));
			}
		}
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		let node_tree = self.scene_base.get_node_tree_mut();
		self.document.bind_pending_images(node_tree);
		if self.is_button_clicked.replace(false) {
			if let Some(label_id) = self.label_id {
				if let Some(label) = node_tree[label_id].get_component_mut::<UILabel>() {
					label.set_text("Clicked");
				}
			}
		}
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.asset_directory_paths = vec![PathBuf::from("examples/assets")];
	let engine = Engine::new(engine_configuration);
	let visual_scene = VisualScene {
		scene_base: SceneBase::new(),
		scene_asset: TextAsset::new(),
		document: UIDocument::new(),
		label_id: None,
		is_button_clicked: Rc::new(Cell::new(false)),
	};
	if let Err(error) = engine.run(Box::new(visual_scene)) {
		eprintln!("{}", error);
	}
}
```

Saving a selected node as a prefab creates a UI document at `prefabs/<name>.prefab.json` with that one node as its root. To use the same prefab many times, load the text once and call `from_json_text` each time you need one. Every call produces a new node tree.

```rust
use vanilla::{NodeTree, Paint, UIDocument, Vector2};

let prefab_json_text = r#"{"version":1,"root":{"type":"WorldNode","name":"Coin","contentSize":[80,80],"components":[{"type":"Paint","color":[1,0.8,0.2,1],"roundSize":40}],"children":[]}}"#;

let mut node_tree = NodeTree::new();
let root_id = node_tree.get_root_id();
let mut document = UIDocument::new();
for index in 0..3 {
	let coin_id = document.from_json_text(&mut node_tree, prefab_json_text).unwrap();
	node_tree[coin_id].set_local_position(Vector2::create(200.0 + index as f32 * 120.0, 120.0));
	node_tree.add_child(root_id, coin_id);
}
let coin_ids = node_tree.get_child_ids(root_id).to_vec();
assert_eq!(coin_ids.len(), 3);
assert!(node_tree[coin_ids[2]].get_component::<Paint>().is_some());
assert_eq!(node_tree[coin_ids[2]].get_local_position().x, 440.0);
```

`VisualAsset` reads scenes and prefabs saved by the Visual Editor (`.scene.json`, `.prefab.json`, in `UIDocument` form) and holds a `Visual` that carries the document. `VisualAsset::load(engine, path)` returns a load handle you can await; if the download fails or the document form (a JSON object with root) is wrong, it stays not loaded. Downloaded bytes also work with `load_from_bytes`, and text with `VisualAsset::create_visual_from_text`. `Visual::create_node(&mut document, &mut node_tree)` builds a new node tree from the document, sets the root's local position to `location`, and returns that root. (`Ok(None)` without a document, `Err` for a wrong document form; the caller attaches it to the parent it wants) This is the same as the original `VisualAsset` and `Visual.createNode`.

## Other files shared by both engines

| File | Made by | Where the Rust version reads it |
| --- | --- | --- |
| Sprite atlas JSON and page images | `tools/atlas.cjs` in vanilla.js | `SpriteAtlas::load`, `load_data` |
| Language list `languages.json` and language files | By hand or a translation tool | `languagepack` ("Languages" chapter) |
| Font index `fonts.json` and group lists | By hand or a font slicing tool | `uifont` ("Languages" chapter) |
| Saved values (`window.localStorage`) | The game | `LocalStorage`, `PersistedStore` ("Storage" chapter) |
| Build version `version.json` | The release build | `BuildInfo` ("Developer Tools" chapter) |
| Sound files `.webm` (Opus) | `tools/webm.cjs` in vanilla.js | `AudioAsset`, `AssetLoader` ("Sound" chapter) |

- Saved values on the web use the same key and value form, number text, and `PersistedStore` `{"version":..,"state":..}` as the original, so the Rust version reads and writes saves of an original game on the same origin as is.
- Sound decoding on the desktop uses the same set of decoders as Chrome, so it plays the Opus files made by `webm.cjs` as is.

### Sprite atlases

`SpriteAtlas` reads JSON in the same two forms as the original.

1. The form made by `tools/atlas.cjs`. `{ images: { "name.png": { x, y, width, height, page } }, meta: { imageFileName, pages: [{ imageFileName }] } }`. Without `page` the image is on page 0, and without `meta.pages` there is one page, `meta.imageFileName`.
2. Page paths and a table. `{ pagePaths: ["path"], frames: { "name": [page, x, y, width, height] } }`

Image names can also be looked up without the extension (`"coin.png"` is also found as `"coin"`). A page path without a slash (`/`) is read relative to the folder of the JSON, and a path with one is read as written. `load(engine, json_path)` returns a `SpriteAtlasLoadHandle` that can be awaited, as in the original. The desktop loads everything on the spot. On the web the JSON and the page images arrive one after another, so call `update(engine)` in `tick` until `is_loaded()` is true, or await the handle.

```rust,no_run
use std::path::PathBuf;
use vanilla::{Engine, EngineConfiguration, Sprite, SpriteAtlas};

let mut engine_configuration = EngineConfiguration::new();
engine_configuration.asset_directory_paths = vec![PathBuf::from("examples/assets")];
let engine = Engine::new(engine_configuration);

let mut sprite_atlas = SpriteAtlas::new();
sprite_atlas.load(&engine, "atlas/icons.json");
assert!(sprite_atlas.is_loaded());
assert!(sprite_atlas.has_frame("coin"));
assert!(sprite_atlas.has_frame("coin.png"));

let mut sprite = Sprite::new();
assert!(sprite_atlas.apply_frame(&mut sprite, "star"));
```

Detailed usage is covered in the "Assets" chapter.

## Command-line tools

The only command-line tool in the Rust repository is `vanilla-engine-apidoc`, which extracts the API documentation data. Full usage is in [`tools/TOOLS.md`](https://github.com/ddukbaek2/vanilla-engine/blob/dev/tools/TOOLS.md).

```bash
cargo run -p vanilla-engine-apidoc -- 0.4.0
cargo run -p vanilla-engine-apidoc -- 0.4.0 --check
cargo test --doc -p vanilla-engine-apidoc
```

- The first line writes `docs/api/<version>.json` from the doc comments in the engine source. The data format is the same as vanilla.js's `apidoc.cjs`.
- `--check` checks the data format and this guide (whether the ko and en file names and code blocks match).
- `cargo test --doc -p vanilla-engine-apidoc` compiles and runs this guide's rust code blocks as doctests.

The vanilla.js command-line tools run on Node.js, and of their outputs the Rust version uses the two in the table above. `SpriteAtlas` reads the atlases from `atlas.cjs`, and `AudioAsset` reads the `.webm` files from `webm.cjs`. `project.cjs` (bundling with esbuild) and `mangle.cjs` (shortening names in bundled JavaScript) are for JavaScript builds and have no use in the Rust version. Desktop builds of the Rust version use `cargo build`, and web builds use `cargo build --target wasm32-unknown-unknown`. (See the "Project Setup" chapter.)

## Things to watch for

- UI documents contain only image paths. Call `load_images` or set `set_image_resolver` before `from_json_text`. When images arrive late, as on the web, call `bind_pending_images` from `tick`.
- If component and node types used in an editor are missing from the game's `UIDocument` table, they print a warning and are dropped or become plain nodes. Games that read editor documents call `register_editor_types()`.
- To write and read game components in documents, add `#[vanilla::reflect]` and register them.
- A `Timeline` moves only if you call `tick` every frame. For documents with sound, call `load_sounds` after building the stage.
- `apply_description` does not read `imageDataUrl` in a `.vfx.json`. Load the image yourself and pass it with `set_image`.
- `Visual::create_node` builds with the tables of the `UIDocument` you pass. For documents saved by the editor, call `register_editor_types()` on that document first.

## Related types

UIDocument, UIDocumentSaveContext, UIDocumentLoadContext, NodeTree, TextAsset, JsonAsset, ImageAsset, Image, Timeline, TimelineTarget, TimelineMarker, ParticleSystem, WorldNode, UIButton, UILabel, UIInputField, UIScrollBar, UISnapScrollView, Visual, VisualAsset, SpriteAtlas, SpriteAtlasLoadHandle, AudioAsset, AssetLoader
