# Languages

This chapter covers showing on-screen text in several languages, switching the chosen language, and preparing fonts to draw it. It walks through `Localization`, which keeps translation tables in code, the `languagepack` module, which keeps one JSON file per language, the `uifont` module, which groups font slices for many scripts under one font name, `DynamicFont`, which draws frequently changing numbers from a glyph atlas, and the engine's default font.

The engine's text components (`Text`, `UILabel`) do not read translation tables on their own. The game puts translated text in with `set_text` and puts it in again when the language changes.

## Translation tables with Localization

Every function of `Localization` is an associated function. Put a table (key to translation) in for each language, set the current language, and read text with `text(key, substitutions)`.

```rust
use vanilla::Localization;

Localization::set_table("en", [
	("menu.start", "Start"),
	("hud.score", "Score {0}"),
	("hud.greeting", "Hello, {0}. You have {1} coins."),
]);
Localization::set_table("fr", [
	("menu.start", "Commencer"),
	("hud.score", "Score {0}"),
]);
Localization::set_fallback_language("en");
Localization::set_language("fr");

assert_eq!(Localization::get_language(), "fr");
assert_eq!(Localization::text("menu.start", &[]), "Commencer");
assert_eq!(Localization::text("hud.greeting", &[&"Mina", &30]), "Hello, Mina. You have 30 coins.");
assert_eq!(Localization::text("menu.quit", &[]), "menu.quit");
```

- `text(key, substitutions)` looks in the current language's table, then the fallback language's table, then returns the key itself. In the example, `hud.greeting` is missing from the French table so the English text comes out, and `menu.quit`, which is in no table, comes out as the key.
- You can use the source text itself as the key. Missing entries show the source text, so you can fill in translations little by little.
- Numbered slots such as `{0}` and `{1}` are replaced in order with the values in `substitutions`. Where the original takes variadic arguments, the Rust version takes a slice of `Display` references (`&[&dyn Display]`), so pass `&[]` when there are no values.
- The table in `set_table(language_code, table)` can be anything that yields (key, value) pairs (an array, a `Vec`, a `HashMap`). It replaces any existing table for the same language.
- Both the current language and the fallback language start as `"ko"`. Language codes are always stored in lowercase.
- `clear()` empties every table. Language settings stay as they are. (For tests)
- Tables and languages are global state for one thread. Use them from the thread the engine runs on.

## Named slots and multi-line entries

Naming the value slots in a translation lets each language order the values differently. `text_named(key, values)` looks up the translation and fills each `{name}` slot with the value of the same name in `values`. `values` is a slice of (name, value) pairs, and slots whose name is not given stay as they are. Names may contain only ASCII letters and digits.

A translation can also be a list of strings. Table values are `LocalizationValue`, either one string (`Text`) or a list of strings (`TextList`). For multi-line entries such as dialogue lines, `text_list(key)` returns a `Vec<String>`. A single-string entry gives a list with that one string, and a key that is in no table gives a list with just the key. `lookup(key)` returns the raw value as `Option<LocalizationValue>`.

Put in a table read with `JsonAsset` using `set_table_from_json(language_code, &table_json)`. String values go in as strings, arrays go in with only the strings inside them, and any other value is skipped.

```rust
use vanilla::{Localization, LocalizationValue};

Localization::set_table_from_json("en", &serde_json::json!({
	"hud.restart": "Restart in {seconds}",
	"hud.ammo": "Ammo {count}/{limit}",
	"intro.lines": ["Long ago, the tower fell.", "One hero climbed again."],
	"meta.version": 3,
}));
Localization::set_language("en");

assert_eq!(Localization::text_named("hud.restart", &[("seconds", &3)]), "Restart in 3");
assert_eq!(Localization::text_named("hud.ammo", &[("count", &12)]), "Ammo 12/{limit}");
let intro_lines = Localization::text_list("intro.lines");
assert_eq!(intro_lines.len(), 2);
assert_eq!(intro_lines[0], "Long ago, the tower fell.");
assert_eq!(Localization::lookup("meta.version"), None);
assert_eq!(Localization::lookup("hud.restart"), Some(LocalizationValue::from("Restart in {seconds}")));
assert_eq!(Localization::text("intro.lines", &[]), "intro.lines");
```

Reading a list entry with `text(key, ...)` returns the key, because the value is not a single string. Read list entries with `text_list`.

## Choosing the device language

`detect_language(supported_language_codes, default_language_code)` walks the device's preferred language list in order and returns the first language that is in the supported list. On the web it reads `navigator.languages`, as in the original, and on the desktop it reads the operating system's preferred language order (Windows `GetUserPreferredUILanguages`, macOS `CFLocaleCopyPreferredLanguages`, Linux the `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, and `LANG` environment variables). To use only the matching part, pass a candidate list directly to `select_language(candidate_list, supported_language_codes, default_language_code)`.

- Language tags are lowercased and underscores become hyphens, then trailing subtags are dropped one at a time while matching. `"zh-Hant-TW"` is tried as `"zh-hant-tw"`, `"zh-hant"`, then `"zh"`.
- Aliases added with `set_language_aliases(language_code, alias_list)` also count as that language. Aliases are stored in lowercase too.
- The returned value is a lowercase code. If nothing matches, the default language code is returned as is.

```rust
use vanilla::Localization;

Localization::set_language_aliases("zh-hant", &["zh-TW", "zh-HK"]);
let supported_language_codes = ["en", "ko", "zh-hant", "pt"];

let candidate_list = vec![String::from("pt-BR"), String::from("en-US")];
assert_eq!(Localization::select_language(&candidate_list, &supported_language_codes, "en"), "pt");
assert_eq!(Localization::select_language(&[String::from("zh-TW")], &supported_language_codes, "en"), "zh-hant");
assert_eq!(Localization::select_language(&[String::from("de-DE")], &supported_language_codes, "en"), "en");

let language_code = Localization::detect_language(&supported_language_codes, "en");
Localization::set_language(&language_code);
assert!(supported_language_codes.contains(&language_code.as_str()));
```

## Switching languages on screen

Switching languages means changing the current language with `set_language` and putting all on-screen text in again. The engine does not announce language changes, so keep the code that sets text in one place and call it after changing the language. If you save the chosen language in a `PersistedStore`, the next launch uses it as is. (Saving is covered in the "Storage" chapter.) The example below switches between English and French every time the screen is pressed.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Graphic, Localization, NodeId, PersistedStore, Pivot, Scene, SceneBase, Text, TextAlign, Vector2};

struct MenuScene {
	scene_base: SceneBase,
	settings_store: PersistedStore,
	label_ids: Vec<NodeId>,
}

impl MenuScene {
	fn create_label(&mut self, position: Vector2, font_size: f64) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let label_id = node_tree.create_child(root_id, "Label");
		let label_node = &mut node_tree[label_id];
		label_node.set_pivot(Pivot::TOP_LEFT);
		label_node.set_local_position(position);
		label_node.set_content_size(Vector2::create(560.0, font_size + 16.0));
		let text = label_node.add_component(Text::create_with_text("", font_size));
		text.set_text_align(TextAlign::Left);
		self.label_ids.push(label_id);
	}

	fn refresh_texts(&mut self) {
		let label_texts = [
			Localization::text("menu.title", &[]),
			Localization::text_named("menu.coins", &[("coins", &120)]),
			Localization::text("menu.language", &[]),
		];
		let node_tree = self.scene_base.get_node_tree_mut();
		for (label_id, label_text) in self.label_ids.iter().zip(label_texts.iter()) {
			if let Some(text) = node_tree[*label_id].get_component_mut::<Text>() {
				text.set_text(label_text);
			}
		}
	}
}

impl Scene for MenuScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, _engine: &Engine) {
		let saved_language = self.settings_store.get()["language"].as_str().unwrap_or("").to_string();
		let language_code = if saved_language.is_empty() {
			Localization::detect_language(&["en", "fr"], "en")
		}
		else {
			saved_language
		};
		Localization::set_language(&language_code);

		self.create_label(Vector2::create(40.0, 40.0), 40.0);
		self.create_label(Vector2::create(40.0, 110.0), 24.0);
		self.create_label(Vector2::create(40.0, 180.0), 22.0);
		self.refresh_texts();
	}

	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		self.scene_base.tick(time_delta);
		if engine.get_input_manager().is_touch_pressed() {
			let next_language = if Localization::get_language() == "en" {
				"fr"
			}
			else {
				"en"
			};
			Localization::set_language(next_language);
			self.settings_store.set(serde_json::json!({ "language": next_language }));
			self.refresh_texts();
		}
	}

	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
	}
}

fn main() {
	Localization::set_table("en", [
		("menu.title", "Dungeon Run"),
		("menu.language", "Language: English (tap to switch)"),
		("menu.coins", "Coins {coins}"),
	]);
	Localization::set_table("fr", [
		("menu.title", "Course au donjon"),
		("menu.language", "Langue : Francais (touchez pour changer)"),
		("menu.coins", "Pieces {coins}"),
	]);
	Localization::set_fallback_language("en");

	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(640.0, 360.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let menu_scene = MenuScene {
		scene_base: SceneBase::new(),
		settings_store: PersistedStore::new("mygame.settings", serde_json::json!({ "language": "" }), 1),
		label_ids: Vec::new(),
	};
	if let Err(error) = engine.run(Box::new(menu_scene)) {
		eprintln!("{}", error);
	}
}
```

## Right-to-left languages

Languages written right to left, such as Arabic and Hebrew, flip the screen layout as well as the text direction. `Localization` treats `ar`, `fa`, `he`, `ur`, `ps`, `yi`, and `dv` as such languages from the start. It decides by the first subtag of the language code, so `"ar-EG"` counts too.

- `is_right_to_left()` returns whether the current language is one of them. Add or remove languages with `set_right_to_left(language_code, is_right_to_left)`.
- `mirror_layout_x(x, width, parent_width)` takes the left position `x` used for left-to-right languages and returns the position mirrored within the parent width (`parent_width - x - width`) for right-to-left languages, or `x` otherwise. Call it only for layouts that should flip, such as windows and tables, not for things pinned to a screen corner or for the direction of a slider.
- `read_layout_text_align(text_align)` swaps `TextAlign::Left` and `TextAlign::Right` for right-to-left languages.
- `isolate_text(text)` wraps text in directional isolate characters (U+2068, U+2069) for right-to-left languages. Numbers with units (`+60%`) and personal names then stay together as one unit instead of scattering among the surrounding text. `text_named` wraps filled values this way, and slots joined only by slashes, as in `"{count}/{limit}"`, are wrapped once as a whole rather than one by one. (Wrapping each value would make 1/40 look like 40/1.)

```rust
use vanilla::{Localization, TextAlign};

Localization::set_language("ar");
let panel_left = Localization::mirror_layout_x(20.0, 300.0, 1280.0);
let text_align = Localization::read_layout_text_align(TextAlign::Left);
let isolated_text = Localization::isolate_text("+60%");
assert!(Localization::is_right_to_left());
assert_eq!(panel_left, 960.0);
assert_eq!(text_align, TextAlign::Right);
assert_eq!(isolated_text.chars().count(), 6);

Localization::set_right_to_left("ar", false);
assert!(!Localization::is_right_to_left());
assert_eq!(Localization::mirror_layout_x(20.0, 300.0, 1280.0), 20.0);
```

The direction of the text itself is set on the text components. `Text::set_text_direction(text_direction)` and `UILabel::set_text_direction(text_direction)` take `TextDirection::Inherit` (the initial value, follows the `Graphic` direction at draw time), `Ltr`, `Rtl`, or `Auto` (decided by the first strong character of the text), and the alignments `TextAlign::Start` and `TextAlign::End` become left or right according to this direction. Screens in right-to-left languages use `Rtl` and `Start`.

```rust
use vanilla::{Text, TextAlign, TextDirection};

let mut text = Text::create_with_text("Score", 24.0);
text.set_text_direction(TextDirection::Rtl);
text.set_text_align(TextAlign::Start);
assert_eq!(text.get_text_direction(), TextDirection::Rtl);
assert_eq!(text.get_text_align(), TextAlign::Start);
```

## Language files with languagepack

Games with many languages, where translators edit files separately, keep translation tables in JSON asset files instead of code. The original `LanguagePack` is the `languagepack` module in the Rust version, called through module functions such as `languagepack::translate(...)`. There is only one state per thread.

The files are one language list plus one table per language. The paths start as `./assets/localization/languages.json` and its folder, and you change them with `set_language_paths(list_path, directory_path)` before loading. They are asset paths (relative to `EngineConfiguration.asset_directory_paths` on the desktop and `asset_base_url` on the web), and the folder path ends with `/`.

```json
{
	"fallbackLanguage": "en",
	"languages": [
		{ "code": "en", "name": "English", "englishName": "English", "fontGroup": "latin", "direction": "ltr", "aliases": [], "file": "en.json" },
		{ "code": "zh-Hant", "name": "繁體中文", "englishName": "Chinese (Traditional)", "fontGroup": "tc", "direction": "ltr", "aliases": ["zh-tw", "zh-hk"], "file": "zh-hant.json" },
		{ "code": "ar", "name": "العربية", "englishName": "Arabic", "fontGroup": "arabic", "direction": "rtl", "aliases": [], "file": "ar.json" }
	]
}
```

A language file (such as `en.json`) is a single key-to-text table. Give every language file the same keys as the default language file.

```json
{
	"game.title": "Dungeon Run",
	"menu.start": "Start",
	"hud.restart": "Restart {seconds}"
}
```

- Each language line is read into a `LanguageEntry` struct (`code`, `name`, `english_name`, `font_group`, `direction`, `aliases`, `countries`, `file`). Lines without a `code` are dropped. `fontGroup` is the font group name used by `uifont` below, and a `direction` of `"rtl"` marks a right-to-left language.
- `aliases` and `countries` are lowercased when read. Matching lowercases the tag and compares it against codes and aliases.
- Without `fallbackLanguage`, the default language is `"en"`.

Loading goes in this order.

1. Call `apply_language_setting(engine, setting_language_code)`. Pass the language code saved in your settings, and if it is empty (`""`) the device language is used. If the list has not been fetched yet, it is fetched first.
2. The language to use is the setting's code if it is in the list. Otherwise the list is searched with the platform language given by `set_preferred_language_tag`, then the device language list in order (`match_language_tag`, dropping trailing subtags and checking codes and aliases), and if nothing matches it is the list's default language.
3. The tables of that language and the default language are fetched. The document language is set to that language code (`<html lang>` on the web, the document language the engine keeps on the desktop, read with `Document::get_language`), and if the key from `set_document_title_key` (initially `"game.title"`, `""` leaves it alone) has text, it becomes the title (the tab title on the web, the window title on the desktop).
4. The function registered with `set_language_loaded_handler` is called with `(engine, language line, all text of that language)`. A function that returns a future is registered with `set_language_loaded_async_handler`, and loading counts as in progress until that future finishes. Hook font loading in here.

The original waits with async functions. In the Rust version, `load_language_list`, `load_language`, and `apply_language_setting` request the reads and return a `LanguageLoadHandle` in place of the original Promise. Awaiting this handle inside an engine task waits until everything is loaded, and it calls `update(engine)` every frame while waiting. When you do not await, call `languagepack::update(engine)` every frame and wait until `is_loading()` is false. On the desktop the files are read on the spot, so loading finishes inside the requesting function.

Read text with `translate(key)` (current language, default language, then the key) and `format_text(key, values)` (fills named slots with the same rules as `Localization::text_named`). Read the direction with `read_text_direction()` (`"ltr"` or `"rtl"`) and `is_right_to_left()`, and `mirror_layout_x`, `read_layout_text_align`, and `isolate_text` behave like the functions of the same name in `Localization`. The difference is that right-to-left is decided by the list's `direction`.

You can also put the list and tables in from code without files. `set_language_list_from_json(json_text)` takes text in the same form as `languages.json`, and `set_language_texts_from_json(language_code, language_json_text, fallback_json_text)` puts in the current language's table together with the default language's table (`false` if the language is not in the list). This path does not touch the document language, the title, or the loaded handler.

```rust
use vanilla::languagepack;

let language_list_json = r#"{
	"fallbackLanguage": "en",
	"languages": [
		{ "code": "en", "name": "English", "englishName": "English", "fontGroup": "latin", "direction": "ltr", "aliases": [], "file": "en.json" },
		{ "code": "fr", "name": "Francais", "englishName": "French", "fontGroup": "latin", "direction": "ltr", "aliases": ["fr-ca"], "file": "fr.json" }
	]
}"#;
assert!(languagepack::set_language_list_from_json(language_list_json));

let english_texts = r#"{ "game.title": "Dungeon Run", "menu.start": "Start", "hud.restart": "Restart {seconds}", "hud.ammo": "Ammo {count}/{limit}" }"#;
let french_texts = r#"{ "game.title": "Course au donjon", "menu.start": "Commencer" }"#;
assert!(languagepack::set_language_texts_from_json("fr", french_texts, english_texts));

assert_eq!(languagepack::read_language_code(), "fr");
assert_eq!(languagepack::read_text_direction(), "ltr");
assert_eq!(languagepack::translate("menu.start"), "Commencer");
assert_eq!(languagepack::format_text("hud.restart", &[("seconds", &3)]), "Restart 3");
assert_eq!(languagepack::format_text("hud.ammo", &[("count", &12), ("limit", &40)]), "Ammo 12/40");

let matched_language = languagepack::match_language_tag("fr-CA").unwrap();
assert_eq!(matched_language.code, "fr");
assert_eq!(languagepack::read_languages().len(), 2);
```

`read_languages()` returns the languages in the list (for building a language menu), `read_language()` the current language's line (`None` until loaded), `find_language(code)` the line found by code (case-insensitive), and `read_all_texts()` all of the current language's text joined together. Every returned line is a copy.

A scene that loads from files hands the load to a scene task in `load` and makes the scene wait for it with `track_task`. The engine calls `initialize` after the tracked task finishes, so text is already available in `initialize`.

```rust,no_run
use std::path::PathBuf;
use vanilla::{languagepack, Engine, EngineConfiguration, Graphic, Scene, SceneBase, Text, Vector2};

struct TitleScene {
	scene_base: SceneBase,
}

impl Scene for TitleScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn load(&mut self, engine: &Engine) {
		let load_task_handle = engine.spawn_scene_task(self, |mut scene_context| async move {
			scene_context.with(|_scene, engine| languagepack::apply_language_setting(engine, "")).await;
		});
		self.scene_base.track_task(&load_task_handle);
	}

	fn initialize(&mut self, _engine: &Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = node_tree.create_child(root_id, "Title");
		let title_node = &mut node_tree[title_id];
		title_node.set_local_position(Vector2::create(640.0, 300.0));
		title_node.set_content_size(Vector2::create(800.0, 80.0));
		title_node.add_component(Text::create_with_text(&languagepack::translate("game.title"), 48.0));
	}

	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
	}
}

fn main() {
	languagepack::set_language_paths("localization/languages.json", "localization/");
	languagepack::set_document_title_key("game.title");

	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.asset_directory_paths = vec![PathBuf::from("assets")];
	let engine = Engine::new(engine_configuration);
	let title_scene = TitleScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(title_scene)) {
		eprintln!("{}", error);
	}
}
```

## Grouping fonts for many scripts with uifont

Games that draw many languages on one screen need different font files per script, and CJK fonts are very large. The original `UIFont` is the `uifont` module in the Rust version. Like Google Fonts, it creates a font face for each font slice split by character range (`unicodeRange`) and registers them all under one font name (initially `"GameUI"`). Only the slices containing characters used on screen are fetched.

The original creates browser `FontFace` objects and adds them to `document.fonts`. The Rust version fetches the bytes of the slice files on both desktop and web and registers them as faces with the same name in the engine's text cache, so even on the web the engine, not the browser font system, shapes and draws the text.

The font list is one index file plus one slice list file per group. The initial paths are `./assets/fonts/fonts.json` and `./assets/fonts/`.

```json
{
	"fonts": [
		{ "group": "latin", "manifest": "fonts-latin.json" },
		{ "group": "tc", "manifest": "fonts-tc.json" }
	]
}
```

```json
{
	"faces": [
		{ "file": "exo2-latin.woff2", "weight": "100 900", "style": "normal", "unicodeRange": "U+0000-00FF, U+0131, U+0152-0153" }
	]
}
```

Use it in this order.

1. Configure it with `configure_ui_font(options)` before loading. Every field of `UiFontOptions` is an `Option`, so fill only the fields to change with `Some` and leave the rest with `..UiFontOptions::default()`. Fields left as `None` stay unchanged. The fields and their initial values are `family` (`"GameUI"`), `manifest_path` (`"./assets/fonts/fonts.json"`), `directory_path` (`"./assets/fonts/"`), `primary_group` (`"latin"`), `weights` (`[500, 700]`), `wait_milliseconds` (1500), and `remaining_delay_milliseconds` (3000).
2. After the language is loaded, call `load_ui_font(engine, font_group, text)`. It registers the chosen language's group and the last group (`primary_group`) and fetches the slices containing that language's characters. Awaiting the returned `UiFontLoadHandle` waits until the initial characters are all registered or `wait_milliseconds` passes (`is_ui_font_ready()` reports the same). The remaining groups start downloading once, `remaining_delay_milliseconds` after the first load. Even if the lists cannot be read, the game continues with the default font.
3. Call `uifont::update(engine)` every frame. It takes in the fetched lists, fetches the remaining groups, and fetches the slices for characters that were measured or drawn with this font without being fetched in advance. The engine does not call it for you.
4. Give text components the font name returned by `read_ui_font_face()` (`None` if it failed to load) through `set_font_family`. If you need a font string, `build_ui_font_string(font_weight, font_size)` builds one in the form `700 24px "GameUI", sans-serif`.
5. For characters that appear on the fly, such as personal names, request their slices with `request_ui_font_text(engine, text)`. It requests only characters not seen before and does not wait. If you need to wait, await the `UiFontTextLoadHandle` returned by `load_ui_font_text(engine, text)`.
6. Turn on `EngineConfiguration.rebake_text_on_font_load`. The engine does not wait for slices that have not arrived and draws with another font instead, so with this on, each time a slice arrives only the text baked within that slice's character range is thrown away and baked again.

```rust
use vanilla::{uifont, UiFontOptions};

uifont::configure_ui_font(UiFontOptions {
	family: Some(String::from("GameUI")),
	manifest_path: Some(String::from("fonts/fonts.json")),
	directory_path: Some(String::from("fonts/")),
	primary_group: Some(String::from("latin")),
	weights: Some(vec![500, 700]),
	wait_milliseconds: Some(1500.0),
	..UiFontOptions::default()
});
assert_eq!(uifont::read_ui_font_family(), "GameUI");
assert_eq!(uifont::build_ui_font_string(700, 24.0), "700 24px \"GameUI\", sans-serif");
assert!(uifont::read_ui_font_face().is_none());
```

When used with `languagepack`, load the font in the loaded handler. Registered as async, the handle of `apply_language_setting` waits until the font's initial characters are ready. The scene hands `apply_language_setting` over in `load` as in the example in "Language files with languagepack" above, calls `uifont::update(engine)` in `tick`, and gives text nodes' `Text` the name from `read_ui_font_face()`.

```rust,no_run
use vanilla::{languagepack, uifont, Engine, EngineConfiguration, LanguageEntry, UiFontOptions};

let mut engine_configuration = EngineConfiguration::new();
engine_configuration.rebake_text_on_font_load = true;

languagepack::set_language_paths("localization/languages.json", "localization/");
uifont::configure_ui_font(UiFontOptions {
	manifest_path: Some(String::from("fonts/fonts.json")),
	directory_path: Some(String::from("fonts/")),
	..UiFontOptions::default()
});
languagepack::set_language_loaded_async_handler(Some(Box::new(|engine: &Engine, language: &LanguageEntry, all_texts: &str| {
	let font_load_handle = uifont::load_ui_font(engine, &language.font_group, all_texts);
	Box::pin(async move {
		font_load_handle.await;
	})
})));
let engine = Engine::new(engine_configuration);
```

These are the rules `uifont` follows when picking slices.

- Characters shared by several fonts, such as Han characters, are drawn by the group registered later. The chosen language's group is placed right before the last group, so Japanese gets Han characters from the Japanese font and Traditional Chinese from the Traditional font. `apply_ui_font_group(engine, font_group)` reorders this and clears the engine's text cache (`load_ui_font` calls it).
- The last group (Latin at first) always stays on top, so digits and Latin letters use that font in every language.
- Every slice is registered only at the weights in `weights`. A slice with a weight range (`"100 900"`) is registered once per weight inside it, and a slice with a single value (`"400"`) at the nearest weight (`read_target_weights(weight_descriptor, weights)`). With the initial values, a request for 400 is drawn by the 500 slice and a request for 600 by the 700 slice.

## Drawing numbers with DynamicFont

`Text` bakes the whole text into a texture for each string. For text that changes often and repeats the same characters a lot, such as scores, damage numbers, and clocks, use `DynamicFont`. It bakes the chosen characters in one font and size into one image (1024 pixels wide, with 2 pixels of padding between glyphs) and draws characters by cutting them out of the atlas. Because they share one image, batching is not broken.

In the Rust version, baking and drawing use the text cache of `Graphic`, so `build_atlas` and `draw_text` take a `Graphic`. Just as the original bakes once while the scene loads, bake in `initialize` with `engine.get_graphic_mut()`. The example below draws on top of the view coordinate system that `draw_scene_background` in `pre_draw` has set up.

```rust,no_run
use vanilla::{DynamicFont, Engine, EngineConfiguration, Graphic, Scene, SceneBase, Vector2};

struct ScoreScene {
	scene_base: SceneBase,
	dynamic_font: DynamicFont,
	score: u64,
}

impl Scene for ScoreScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, engine: &Engine) {
		if let Some(mut graphic) = engine.get_graphic_mut() {
			self.dynamic_font.build_atlas(&mut graphic, "0123456789SCORE ");
		}
	}

	fn tick(&mut self, time_delta: f64) {
		self.scene_base.tick(time_delta);
		self.score += (time_delta * 1000.0) as u64;
	}

	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
	}

	fn draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		let score_text = format!("SCORE {}", self.score);
		let text_width = self.dynamic_font.measure_text(&score_text);
		let position = Vector2::create(640.0 - text_width * 0.5, 360.0);
		self.dynamic_font.draw_text(&score_text, position, graphic);
	}
}

fn main() {
	let mut dynamic_font = DynamicFont::new();
	dynamic_font.set_font_size(48.0);
	dynamic_font.set_letter_spacing(2.0);

	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let score_scene = ScoreScene {
		scene_base: SceneBase::new(),
		dynamic_font,
		score: 0,
	};
	if let Err(error) = engine.run(Box::new(score_scene)) {
		eprintln!("{}", error);
	}
}
```

- The original `setFontAsset(fontAsset)` is `set_font_family(font_family)`. For a font loaded with `FontAsset`, pass `font_asset.get_family()`. Without it the font name is `"default"`, so the atlas is baked with the engine's default font. The font size starts at 32 pixels and the letter spacing at 0.
- `build_atlas(graphic, charset)` adds characters that are not there yet and bakes all contained characters again. With no new characters and an existing atlas it does nothing, so calling it every frame is fine. After changing the font, size, or letter spacing, call `rebuild_atlas(graphic)`. `clear_atlas()` removes every character.
- Characters are counted in grapheme clusters (final consonants, combining marks, emoji sequences). Characters missing from the atlas leave a gap of half the font size.
- The `position` of `draw_text(text, position, graphic)` is the left edge and baseline of the first character. `measure_text(text)` is the sum of advances (glyph width plus letter spacing).
- Glyphs are baked in white. To tint them, set `graphic.set_image_tint_color(Some(color))` before drawing and set it back to `None` afterwards.
- `get_glyph(character)` returns the `GlyphInfo` of one character (`uv_rect`, the area inside the atlas, `advance`, `bearing_x`, `bearing_y`), or `None` if it is missing. The original `getAtlasCanvas()` is `get_atlas_image()` (`Option<&Rc<Image>>`).
- On the desktop, glyphs are baked with the font faces in the text cache, and if no face covers a character the atlas is left unbaked. On the web they are baked with the browser's Canvas2D, as in the original. Baking before the font file arrives bakes with a different font, so bake after the `FontAsset` has finished loading.

## The engine's default font

`Text` and `UILabel` draw with the engine's default font unless you give them a font with `set_font_family`. The default font is chosen per platform as follows.

- Desktop: when `EngineConfiguration.default_font_paths` is empty (the initial value), the names from the original system font list (`-apple-system, "Segoe UI", Roboto, "Apple SD Gothic Neo", "Malgun Gothic", sans-serif`) are looked up in order among the operating system's fonts and chained. If you give paths, the readable files are chained in order and characters missing from an earlier font are drawn with a later one. Bold faces come from `default_bold_font_paths`. Characters found in no font are drawn with a fallback font chosen by the operating system, or with the files in `fallback_font_paths` if you give them.
- Web: `EngineConfiguration.default_font_url` starts as empty text (same as the original `defaultFontUrl`). If no font is given, text is drawn with the browser's system fonts, as in the original. If you give an address, the font is fetched and used as the default font, and text is drawn with system fonts while it downloads.
- On both platforms, font file contents placed in `default_font_bytes` and `default_bold_font_bytes` are used first.

The engine statistics overlay (`use_statistics`) uses this default font too. To use another font, load it with `FontAsset` and pass its name to `set_font_family`. (See the "Assets" chapter.)

## Things to watch for

- The engine does not announce language changes. When the language changes, the game puts the on-screen text in again.
- `Localization` and `languagepack` use separate tables. Text put in with `Localization::set_table` cannot be read with `languagepack::translate`, and vice versa. Right-to-left is also decided separately: `Localization` by the language code, `languagepack` by the list's `direction`.
- `Localization::text` returns the key for list entries. Read list entries with `text_list`.
- Set the paths and settings of `languagepack` and `uifont` before loading. The language list and the font index are read only once.
- If you do not await the handles, call `languagepack::update(engine)` every frame, and when you use fonts always call `uifont::update(engine)` every frame.
- Games that fetch font slices on the fly should turn on `EngineConfiguration.rebake_text_on_font_load`. With it off, text baked with another font before a slice arrives stays until it is pushed out of the cache.

## Related types

Localization, LocalizationValue, LocalizationTable, LanguageEntry, LanguageLoadHandle, LanguageLoadFuture, UiFontOptions, UiFontLoadHandle, UiFontTextLoadHandle, DynamicFont, GlyphInfo, FontAsset, Text, UILabel, TextDirection, TextAlign, EngineConfiguration, PersistedStore
