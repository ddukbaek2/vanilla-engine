# 언어

화면 글자를 여러 언어로 보이고, 고른 언어를 바꾸고, 그 언어를 그릴 글꼴을 준비하는 방법을 다룹니다. 번역 표를 코드에 두는 `Localization`, 번역 표를 언어마다 JSON 파일로 두는 `languagepack` 모듈, 여러 문자의 글꼴 조각을 한 글꼴 이름으로 묶는 `uifont` 모듈, 자주 바뀌는 숫자를 글자 아틀라스로 그리는 `DynamicFont`, 엔진의 기본 글꼴 차례로 봅니다.

엔진의 글자 컴포넌트(`Text`, `UILabel`)는 번역 표를 스스로 읽지 않습니다. 번역한 글을 `set_text` 로 넣고, 언어를 바꾸면 다시 넣는 일은 게임이 합니다.

## Localization 으로 번역 표 쓰기

`Localization` 은 모든 함수가 연관 함수입니다. 언어마다 표(키 → 번역문)를 넣고, 지금 언어를 정한 뒤 `text(key, substitutions)` 로 꺼냅니다.

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

- `text(key, substitutions)` 는 지금 언어의 표, 폴백 언어의 표, 키 원문 차례로 찾습니다. 위 예제의 `hud.greeting` 은 프랑스어 표에 없어 영어 표의 글이 나오고, 어느 표에도 없는 `menu.quit` 은 키 그대로 나옵니다.
- 키는 원문을 그대로 써도 됩니다. 표에 없으면 원문이 나오므로 번역을 조금씩 채워 갈 수 있습니다.
- `{0}`, `{1}` 처럼 번호 자리는 `substitutions` 의 값으로 차례대로 바꿉니다. 원본의 가변 인자 자리가 러스트 판에서는 `Display` 참조의 조각(`&[&dyn Display]`)이라, 값이 없으면 `&[]` 를 넘깁니다.
- `set_table(language_code, table)` 의 표는 (키, 값) 쌍을 내는 것이면 됩니다. (배열, `Vec`, `HashMap`) 같은 언어의 표가 있으면 바꿉니다.
- 지금 언어와 폴백 언어는 처음에 둘 다 `"ko"` 입니다. 언어 코드는 모두 소문자로 바꿔 둡니다.
- `clear()` 는 모든 표를 비웁니다. 언어 설정은 그대로입니다. (시험용)
- 표와 언어는 스레드 하나의 전역 상태입니다. 엔진이 도는 스레드에서 씁니다.

## 이름 자리와 여러 줄 항목

번역문 안의 값 자리에 이름을 붙이면 언어마다 값의 차례가 달라도 됩니다. `text_named(key, values)` 는 번역문을 찾은 뒤 `{이름}` 자리를 `values` 의 같은 이름 값으로 채웁니다. `values` 는 (이름, 값) 쌍의 조각이고, 없는 이름의 자리는 그대로 둡니다. 이름은 영문과 숫자만 됩니다.

번역문은 글 목록이어도 됩니다. 표의 값은 `LocalizationValue` 로, 글 하나(`Text`)나 글 목록(`TextList`)입니다. 대사 목록처럼 여러 줄인 항목은 `text_list(key)` 로 `Vec<String>` 을 받습니다. 글 하나인 항목이면 그 글 하나를 담은 목록, 어느 표에도 없으면 키 하나를 담은 목록입니다. `lookup(key)` 는 바꾸지 않은 원래 값을 `Option<LocalizationValue>` 로 돌려줍니다.

`JsonAsset` 으로 읽은 표는 `set_table_from_json(language_code, &table_json)` 으로 넣습니다. 값이 글이면 글로, 배열이면 그 안의 글만 모아 넣고, 그 밖의 값은 건너뜁니다.

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

목록인 항목을 `text(key, ...)` 로 꺼내면 글이 아니므로 키가 나옵니다. 목록 항목은 `text_list` 로 꺼냅니다.

## 기기 언어 고르기

`detect_language(supported_language_codes, default_language_code)` 는 기기가 앞세우는 언어 목록을 차례로 보고 지원 목록에 있는 첫 언어를 돌려줍니다. 웹은 원본처럼 `navigator.languages` 를 읽고, 데스크탑은 운영체제의 선호 언어 차례를 읽습니다. (윈도우 `GetUserPreferredUILanguages`, 맥 `CFLocaleCopyPreferredLanguages`, 리눅스 `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG` 환경 변수) 대조하는 부분만 따로 쓰려면 `select_language(candidate_list, supported_language_codes, default_language_code)` 에 후보 목록을 직접 넘깁니다.

- 언어 태그는 소문자로 바꾸고 밑줄을 하이픈으로 바꾼 뒤, 뒤 조각부터 하나씩 떼며 맞춥니다. `"zh-Hant-TW"` 는 `"zh-hant-tw"`, `"zh-hant"`, `"zh"` 차례로 봅니다.
- `set_language_aliases(language_code, alias_list)` 로 별칭을 넣으면 그 별칭도 그 언어로 봅니다. 별칭도 소문자로 바꿔 둡니다.
- 돌려주는 값은 소문자 코드입니다. 맞는 것이 없으면 기본 언어 코드를 그대로 돌려줍니다.

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

## 화면에서 언어 바꾸기

언어를 바꾸는 일은 `set_language` 로 지금 언어를 바꾸고, 화면의 글을 모두 다시 넣는 것입니다. 엔진은 언어가 바뀐 것을 알리지 않으므로 글을 넣는 함수를 한 곳에 모아 두고 언어를 바꾼 뒤 부릅니다. 고른 언어는 `PersistedStore` 에 저장해 두면 다음에 켤 때 그대로 씁니다. (저장은 "저장" 장) 아래 예제는 화면을 누를 때마다 영어와 프랑스어를 바꿉니다.

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

## 오른쪽에서 왼쪽으로 쓰는 언어

아랍어, 히브리어처럼 오른쪽에서 왼쪽으로 쓰는 언어는 글자 방향과 함께 화면 배치도 좌우로 뒤집습니다. `Localization` 은 처음부터 `ar`, `fa`, `he`, `ur`, `ps`, `yi`, `dv` 를 그런 언어로 봅니다. 언어 코드의 첫 조각으로 판단하므로 `"ar-EG"` 도 같습니다.

- `is_right_to_left()` 는 지금 언어가 그런 언어인지 돌려줍니다. `set_right_to_left(language_code, is_right_to_left)` 로 목록에 더하거나 뺍니다.
- `mirror_layout_x(x, width, parent_width)` 는 왼쪽에서 오른쪽 언어일 때의 왼쪽 자리 `x` 를 받아, 오른쪽에서 왼쪽 언어면 부모 너비 안에서 뒤집은 자리(`parent_width - x - width`)를, 아니면 `x` 를 돌려줍니다. 창과 표처럼 뒤집을 배치에만 부르고, 화면 구석에 붙은 것이나 슬라이더의 방향처럼 뒤집지 않을 것에는 부르지 않습니다.
- `read_layout_text_align(text_align)` 는 오른쪽에서 왼쪽 언어면 `TextAlign::Left` 와 `TextAlign::Right` 를 바꿔 돌려줍니다.
- `isolate_text(text)` 는 오른쪽에서 왼쪽 언어면 글을 방향 격리 문자(U+2068, U+2069)로 감쌉니다. 숫자와 단위(`+60%`)나 사람 이름이 앞뒤 글과 섞여 흩어지지 않고 한 덩어리로 놓입니다. `text_named` 는 채운 값을 이것으로 감싸고, `"{count}/{limit}"` 처럼 빗금으로만 이은 자리들은 값마다가 아니라 통째로 한 번 감쌉니다. (값마다 감싸면 1/40 이 40/1 로 보입니다)

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

글자 자체의 방향은 글자 컴포넌트에서 정합니다. `Text::set_text_direction(text_direction)` 과 `UILabel::set_text_direction(text_direction)` 은 `TextDirection::Inherit`(처음 값, 그리는 때의 `Graphic` 방향을 따름), `Ltr`, `Rtl`, `Auto`(글의 첫 강한 글자로 정함)를 받고, 정렬 `TextAlign::Start` 와 `TextAlign::End` 가 이 방향에 따라 왼쪽이나 오른쪽이 됩니다. 오른쪽에서 왼쪽 언어의 화면은 `Rtl` 과 `Start` 를 씁니다.

```rust
use vanilla::{Text, TextAlign, TextDirection};

let mut text = Text::create_with_text("Score", 24.0);
text.set_text_direction(TextDirection::Rtl);
text.set_text_align(TextAlign::Start);
assert_eq!(text.get_text_direction(), TextDirection::Rtl);
assert_eq!(text.get_text_align(), TextAlign::Start);
```

## languagepack 으로 언어 파일 쓰기

언어가 많고 번역가가 파일을 따로 고치는 게임은 번역 표를 코드가 아닌 애셋의 JSON 파일로 둡니다. 원본의 `LanguagePack` 은 러스트 판에서 `languagepack` 모듈이고, `languagepack::translate(...)` 처럼 모듈 함수로 부릅니다. 상태는 스레드 하나에 하나뿐입니다.

파일은 언어 목록 하나와 언어마다 표 하나입니다. 경로는 처음에 `./assets/localization/languages.json` 과 그 폴더이고, 불러오기 전에 `set_language_paths(list_path, directory_path)` 로 바꿉니다. 경로는 애셋 경로(데스크탑은 `EngineConfiguration.asset_directory_paths` 기준, 웹은 `asset_base_url` 기준)이고, 폴더 경로는 `/` 로 끝납니다.

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

언어 파일(`en.json` 같은 것)은 키 → 글 표 하나입니다. 모든 언어 파일은 기본 언어 파일과 같은 키를 갖게 둡니다.

```json
{
	"game.title": "Dungeon Run",
	"menu.start": "Start",
	"hud.restart": "Restart {seconds}"
}
```

- 언어 한 줄은 `LanguageEntry` 구조체(`code`, `name`, `english_name`, `font_group`, `direction`, `aliases`, `countries`, `file`)로 읽습니다. `code` 가 없는 줄은 뺍니다. `fontGroup` 은 아래 `uifont` 의 글꼴 묶음 이름이고, `direction` 이 `"rtl"` 이면 오른쪽에서 왼쪽 언어입니다.
- `aliases` 와 `countries` 는 읽을 때 소문자로 바꿉니다. 대조할 때는 태그를 소문자로 바꿔 코드와 별칭을 봅니다.
- `fallbackLanguage` 가 없으면 기본 언어는 `"en"` 입니다.

불러오는 차례는 이렇습니다.

1. `apply_language_setting(engine, setting_language_code)` 를 부릅니다. 설정에 저장한 언어 코드를 넘기고, 비었으면(`""`) 기기의 언어를 씁니다. 목록을 아직 받지 않았으면 먼저 받습니다.
2. 쓸 언어는 설정의 코드가 목록에 있으면 그 언어, 없으면 `set_preferred_language_tag` 로 넣은 플랫폼의 언어, 그다음 기기 언어 목록 차례로 목록에서 찾고(`match_language_tag`, 뒤 조각을 떼며 코드와 별칭을 봄), 다 없으면 목록의 기본 언어입니다.
3. 그 언어의 표와 기본 언어의 표를 받습니다. 문서 언어를 그 언어 코드로 바꾸고(웹은 `<html lang>`, 데스크탑은 엔진이 든 문서 언어, `Document::get_language` 로 읽음), `set_document_title_key` 의 키(처음은 `"game.title"`, `""` 이면 바꾸지 않음)의 글이 있으면 제목으로 씁니다. (웹은 탭 제목, 데스크탑은 창 제목)
4. `set_language_loaded_handler` 로 건 함수를 `(엔진, 언어 한 줄, 그 언어의 모든 글)` 로 부릅니다. 퓨처를 돌려주는 함수는 `set_language_loaded_async_handler` 로 걸고, 그 퓨처가 끝날 때까지 불러오는 중으로 칩니다. 글꼴 받기를 여기에 겁니다.

원본은 async 함수로 기다립니다. 러스트 판의 `load_language_list`, `load_language`, `apply_language_setting` 은 읽기를 요청하고, 원본의 Promise 자리인 `LanguageLoadHandle` 을 돌려줍니다. 엔진 작업 안에서 이 손잡이를 await 하면 다 불러올 때까지 기다리고, 기다리는 동안 프레임마다 `update(engine)` 를 불러 줍니다. await 하지 않을 때는 프레임마다 `languagepack::update(engine)` 를 부르고 `is_loading()` 이 거짓이 될 때까지 기다립니다. 데스크탑은 파일을 그 자리에서 읽어 요청한 함수 안에서 끝납니다.

글은 `translate(key)`(지금 언어, 기본 언어, 키 차례)와 `format_text(key, values)`(이름 자리 채우기, `Localization::text_named` 와 같은 규칙)로 꺼냅니다. 방향은 `read_text_direction()`(`"ltr"` 또는 `"rtl"`), `is_right_to_left()` 로 읽고, `mirror_layout_x`, `read_layout_text_align`, `isolate_text` 는 `Localization` 의 같은 이름 함수와 같게 동작합니다. 다만 오른쪽에서 왼쪽인지는 언어 목록의 `direction` 으로 정합니다.

파일 없이 목록과 표를 코드로 바로 넣을 수도 있습니다. `set_language_list_from_json(json_text)` 는 `languages.json` 과 같은 꼴의 글을 받고, `set_language_texts_from_json(language_code, language_json_text, fallback_json_text)` 는 지금 언어의 표와 기본 언어의 표를 함께 넣습니다. (목록에 없는 언어면 `false`) 이 길은 문서 언어, 제목, 불러온 뒤 함수를 건드리지 않습니다.

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

`read_languages()` 는 목록의 언어들(언어 창을 만들 때), `read_language()` 는 지금 언어의 한 줄(아직 없으면 `None`), `find_language(code)` 는 코드로 찾은 한 줄(대소문자 가리지 않음), `read_all_texts()` 는 지금 언어의 글을 모두 이은 글입니다. 돌려주는 줄은 모두 복사본입니다.

파일에서 불러오는 씬은 `load` 에서 불러오기를 씬 작업으로 맡기고 `track_task` 로 기다리게 합니다. 엔진은 맡긴 작업이 끝난 뒤 `initialize` 를 부르므로, `initialize` 에서는 이미 글을 꺼낼 수 있습니다.

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

## uifont 로 여러 문자의 글꼴 묶기

여러 언어를 한 화면에 그리는 게임은 문자마다 다른 글꼴 파일이 필요하고, 한중일 글꼴은 파일이 매우 큽니다. 원본의 `UIFont` 는 러스트 판에서 `uifont` 모듈입니다. Google Fonts 처럼 글자 범위(`unicodeRange`)로 나눈 글꼴 조각마다 글꼴 얼굴을 만들어 한 글꼴 이름(처음은 `"GameUI"`) 아래에 올립니다. 화면에 쓰는 글자가 든 조각만 받습니다.

원본은 브라우저의 `FontFace` 를 만들어 `document.fonts` 에 넣습니다. 러스트 판은 데스크탑과 웹 모두 조각 파일의 바이트를 받아 엔진의 글자 캐시에 같은 이름의 얼굴로 올리고, 웹에서도 브라우저 글꼴이 아니라 엔진이 글자를 짓고 그립니다.

글꼴 목록은 색인 파일 하나와 묶음마다 조각 목록 파일 하나입니다. 처음 경로는 `./assets/fonts/fonts.json` 과 `./assets/fonts/` 입니다.

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

쓰는 차례는 이렇습니다.

1. 불러오기 전에 `configure_ui_font(options)` 로 설정합니다. `UiFontOptions` 의 칸은 모두 `Option` 이라 바꿀 칸만 `Some` 으로 채우고 `..UiFontOptions::default()` 로 나머지를 비웁니다. `None` 인 칸은 그대로입니다. 칸과 처음 값은 `family`(`"GameUI"`), `manifest_path`(`"./assets/fonts/fonts.json"`), `directory_path`(`"./assets/fonts/"`), `primary_group`(`"latin"`), `weights`(`[500, 700]`), `wait_milliseconds`(1500), `remaining_delay_milliseconds`(3000)입니다.
2. 언어를 불러온 뒤 `load_ui_font(engine, font_group, text)` 를 부릅니다. 고른 언어의 묶음과 맨 뒤 묶음(`primary_group`)을 올리고, 그 언어의 글자가 든 조각을 받습니다. 돌려준 `UiFontLoadHandle` 을 await 하면 처음 글자를 다 올렸거나 `wait_milliseconds` 가 지날 때까지 기다립니다. (`is_ui_font_ready()` 로도 봅니다) 나머지 묶음은 처음 한 번 `remaining_delay_milliseconds` 뒤에 받기 시작합니다. 목록을 읽지 못해도 게임은 기본 글꼴로 이어집니다.
3. 프레임마다 `uifont::update(engine)` 를 부릅니다. 받은 목록을 들고, 나머지 묶음을 받고, 미리 받지 않은 글자를 이 글꼴로 재거나 그렸을 때 그 글자의 조각을 받습니다. 엔진이 대신 부르지 않습니다.
4. 글자 컴포넌트에는 `read_ui_font_face()` 가 돌려준 글꼴 이름(못 불러왔으면 `None`)을 `set_font_family` 로 줍니다. 글꼴 글이 필요하면 `build_ui_font_string(font_weight, font_size)` 가 `700 24px "GameUI", sans-serif` 꼴을 만듭니다.
5. 사람 이름처럼 그때그때 나오는 글자는 `request_ui_font_text(engine, text)` 로 조각 받기를 청합니다. 처음 보는 글자만 청하고 기다리지 않습니다. 기다려야 하면 `load_ui_font_text(engine, text)` 가 돌려준 `UiFontTextLoadHandle` 을 await 합니다.
6. `EngineConfiguration.rebake_text_on_font_load` 를 켭니다. 엔진은 받지 않은 조각을 기다리지 않고 다른 글꼴로 그려 버리므로, 켜 두면 조각이 받아질 때마다 그 조각의 글자 범위로 구운 글자만 버려 다시 굽습니다.

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

`languagepack` 과 함께 쓸 때는 불러온 뒤 함수에서 글꼴을 불러옵니다. async 로 걸면 `apply_language_setting` 의 손잡이가 글꼴의 처음 글자까지 기다립니다. 씬은 위 "languagepack 으로 언어 파일 쓰기" 의 예제처럼 `load` 에서 `apply_language_setting` 을 맡기고, `tick` 에서 `uifont::update(engine)` 를 부르고, 글 노드의 `Text` 에 `read_ui_font_face()` 의 이름을 줍니다.

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

`uifont` 가 조각을 고르는 규칙은 이렇습니다.

- 한자처럼 여러 글꼴이 함께 가진 글자는 나중에 올린 묶음이 그립니다. 고른 언어의 묶음을 맨 뒤 묶음 바로 앞에 올리므로, 일본어면 일본 글꼴의, 중국어 번체면 번체 글꼴의 한자가 나옵니다. `apply_ui_font_group(engine, font_group)` 이 이 차례를 다시 맞추고 엔진의 글자 캐시를 비웁니다. (`load_ui_font` 가 부릅니다)
- 맨 뒤 묶음(처음은 라틴)은 늘 가장 높게 있어 숫자와 영문은 어느 언어에서나 그 글꼴입니다.
- 모든 조각은 `weights` 의 굵기로만 올립니다. 굵기 범위가 있는 조각(`"100 900"`)은 그 안의 굵기마다 한 번씩, 한 값뿐인 조각(`"400"`)은 가장 가까운 굵기 하나로 올립니다. (`read_target_weights(weight_descriptor, weights)`) 처음 값에서 화면이 400 을 청하면 500, 600 을 청하면 700 조각이 그립니다.

## DynamicFont 로 숫자 그리기

`Text` 는 문자열마다 글 전체를 텍스처로 굽습니다. 점수, 피해 숫자, 시계처럼 글이 자주 바뀌고 같은 글자를 많이 그리는 곳에는 `DynamicFont` 를 씁니다. 정한 글자들을 한 글꼴, 한 크기로 그림 한 장(가로 1024 픽셀, 글리프 사이 2 픽셀 여백)에 모아 굽고, 그릴 때 아틀라스에서 글자를 잘라 그립니다. 같은 그림을 쓰므로 묶어 그리기가 끊기지 않습니다.

러스트 판에서 굽기와 그리기는 `Graphic` 의 글자 캐시를 쓰므로, `build_atlas` 와 `draw_text` 는 `Graphic` 을 받습니다. 원본이 씬을 불러올 때 한 번 굽는 것처럼 `initialize` 에서 `engine.get_graphic_mut()` 으로 굽습니다. 아래 예제는 `pre_draw` 의 `draw_scene_background` 가 뷰 좌표계를 걸어 둔 위에 그립니다.

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

- 원본의 `setFontAsset(fontAsset)` 은 `set_font_family(font_family)` 입니다. `FontAsset` 으로 받은 글꼴이면 `font_asset.get_family()` 를 넘깁니다. 주지 않으면 글꼴 이름이 `"default"` 라 엔진의 기본 글꼴로 굽습니다. 글자 크기는 처음 32 픽셀, 자간은 0 입니다.
- `build_atlas(graphic, charset)` 는 아직 없는 글자를 더하고 든 글자를 모두 다시 굽습니다. 새 글자가 없고 아틀라스가 이미 있으면 아무것도 하지 않으므로 매 프레임 불러도 됩니다. 글꼴, 크기, 자간을 바꾼 뒤에는 `rebuild_atlas(graphic)` 를 부릅니다. `clear_atlas()` 는 글자를 모두 비웁니다.
- 글자는 글자 묶음(받침, 결합 부호, 이모지 묶음) 단위로 셉니다. 아틀라스에 없는 글자는 글자 크기의 반만큼 비우고 넘어갑니다.
- `draw_text(text, position, graphic)` 의 `position` 은 첫 글자의 왼쪽과 베이스라인입니다. `measure_text(text)` 는 전진폭(글자 너비 + 자간)의 합입니다.
- 글자는 흰색으로 굽습니다. 색을 입히려면 그리기 전에 `graphic.set_image_tint_color(Some(color))` 를 걸고 그린 뒤 `None` 으로 돌립니다.
- `get_glyph(character)` 는 글자 하나의 `GlyphInfo`(아틀라스 안 영역 `uv_rect`, 전진폭 `advance`, `bearing_x`, `bearing_y`)를, 없으면 `None` 을 돌려줍니다. 원본의 `getAtlasCanvas()` 는 `get_atlas_image()`(`Option<&Rc<Image>>`)입니다.
- 데스크탑은 글자 캐시의 글꼴 얼굴로 굽고, 글자를 덮는 얼굴이 하나도 없으면 굽지 못하고 그대로 둡니다. 웹은 원본처럼 브라우저 Canvas2D 로 굽습니다. 글꼴 파일을 받기 전에 구우면 다른 글꼴로 구워지므로, `FontAsset` 을 다 받은 뒤 굽습니다.

## 엔진의 기본 글꼴

`Text` 와 `UILabel` 은 `set_font_family` 로 글꼴을 주지 않으면 엔진의 기본 글꼴로 그립니다. 기본 글꼴은 플랫폼마다 이렇게 정합니다.

- 데스크탑: `EngineConfiguration.default_font_paths` 가 비어 있으면(처음 값) 원본 시스템 글꼴 목록(`-apple-system, "Segoe UI", Roboto, "Apple SD Gothic Neo", "Malgun Gothic", sans-serif`)의 이름을 차례로 운영체제 글꼴에서 찾아 잇습니다. 경로를 주면 읽히는 파일을 차례로 잇고, 앞 글꼴에 없는 글자는 뒤 글꼴로 그립니다. 굵은 얼굴은 `default_bold_font_paths` 입니다. 어느 글꼴에도 없는 글자는 운영체제가 고른 대체 글꼴로 그리고, `fallback_font_paths` 를 주면 그 파일을 씁니다.
- 웹: `EngineConfiguration.default_font_url` 이 처음에 빈 글입니다(원본 `defaultFontUrl` 과 같다). 아무 글꼴도 주지 않으면 원본처럼 브라우저 시스템 글꼴로 그립니다. 주소를 주면 받아 기본 글꼴로 쓰고, 받는 동안은 시스템 글꼴로 그립니다.
- 두 플랫폼 모두 `default_font_bytes`, `default_bold_font_bytes` 에 글꼴 파일 내용을 미리 넣으면 그것을 먼저 씁니다.

엔진 정보창(`use_statistics`)도 이 기본 글꼴을 씁니다. 다른 글꼴을 쓰려면 `FontAsset` 으로 받아 `set_font_family` 에 그 이름을 줍니다. ("애셋" 장)

## 주의할 점

- 엔진은 언어가 바뀐 것을 알리지 않습니다. 언어를 바꾸면 화면의 글을 게임이 다시 넣습니다.
- `Localization` 과 `languagepack` 은 서로 다른 표를 씁니다. `Localization::set_table` 로 넣은 글은 `languagepack::translate` 로 꺼낼 수 없고, 그 반대도 같습니다. 오른쪽에서 왼쪽인지도 `Localization` 은 언어 코드로, `languagepack` 은 목록의 `direction` 으로 따로 정합니다.
- `Localization::text` 는 목록 항목에 키를 돌려줍니다. 목록 항목은 `text_list` 로 꺼냅니다.
- `languagepack` 과 `uifont` 의 경로와 설정은 불러오기 전에 정합니다. 언어 목록과 글꼴 색인은 한 번만 읽습니다.
- 손잡이를 await 하지 않으면 `languagepack::update(engine)` 를, 글꼴을 쓰면 늘 `uifont::update(engine)` 를 프레임마다 부릅니다.
- 글꼴 조각을 그때그때 받는 게임은 `EngineConfiguration.rebake_text_on_font_load` 를 켭니다. 끄면 받기 전에 다른 글꼴로 구운 글자가 캐시에서 밀려날 때까지 남습니다.

## 관련 타입

Localization, LocalizationValue, LocalizationTable, LanguageEntry, LanguageLoadHandle, LanguageLoadFuture, UiFontOptions, UiFontLoadHandle, UiFontTextLoadHandle, DynamicFont, GlyphInfo, FontAsset, Text, UILabel, TextDirection, TextAlign, EngineConfiguration, PersistedStore
