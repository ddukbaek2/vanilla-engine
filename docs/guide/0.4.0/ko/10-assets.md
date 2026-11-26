# 애셋

그림, 글, JSON, 바이너리, 글꼴, 소리를 받아 엔진에서 쓰는 방법을 다룹니다. 애셋 하나를 받는 애셋 구조체(`ImageAsset`, `TextAsset`, `JsonAsset` 같은 것), 여러 애셋을 함께 받으며 진행률과 실패 목록을 주는 `AssetLoader`, 로딩 막대를 부드럽게 움직이는 `LoadingProgress`, 묶은 그림을 이름으로 꺼내는 `SpriteAtlas` 차례로 봅니다.

## 씬의 create 에서 불러오기

애셋은 씬이 멤버로 듭니다. 중앙 리소스 매니저는 없습니다. 씬의 `create` 에서 `load(engine, 경로)` 로 읽기를 요청하고 `scene_base.track_asset` 으로 씬에 맡기면, 맡긴 것이 다 끝난 뒤 엔진이 `initialize` 를 부릅니다. 애셋을 쓰는 노드는 `initialize` 에서 만듭니다.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Graphic, ImageAsset, Scene, SceneBase, Sprite, Vector2};

struct AssetScene {
	scene_base: SceneBase,
	icon_asset: ImageAsset,
}

impl Scene for AssetScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.icon_asset.load(engine, "examples/assets/atlas/icons.png");
		self.scene_base.track_asset(&self.icon_asset);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let icon_image = match self.icon_asset.get_image() {
			Some(icon_image) => {
				icon_image
			}
			None => {
				return;
			}
		};
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let icon_id = node_tree.create_child(root_id, "icon");
		let icon_node = &mut node_tree[icon_id];
		icon_node.set_local_position(Vector2::create(640.0, 360.0));
		icon_node.set_content_size(Vector2::create(256.0, 256.0));
		icon_node.add_component(Sprite::create_with_image(icon_image));
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let asset_scene = AssetScene {
		scene_base: SceneBase::new(),
		icon_asset: ImageAsset::new(),
	};
	if let Err(error) = engine.run(Box::new(asset_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- 데스크탑은 `load` 를 부른 자리에서 파일을 읽어 끝냅니다. 웹은 `fetch` 가 끝난 뒤 다음 프레임들 사이에 끝납니다. 그래서 받은 것을 읽는 코드는 `create` 가 아니라 `initialize` 에 둡니다.
- 맡긴 것이 다 끝날 때까지 엔진은 씬의 `tick`, `draw` 대신 `draw_on_load` 를 매 프레임 부릅니다. 다 끝나면 0.3 초를 더 기다린 뒤 `initialize` 를 부릅니다. 맡긴 것이 없으면 `create` 뒤 0.3 초에 불립니다.
- 받기에 실패한 애셋도 끝난 것으로 칩니다. 씬은 멈추지 않고, 실패한 애셋은 읽어도 `None` 입니다.
- 이미 요청한 애셋에 `load` 를 다시 부르면 다시 읽지 않고 처음 손잡이를 돌려줍니다. 실패한 애셋도 같습니다. 다시 받으려면 `unload` 뒤에 부르거나 새 애셋을 만듭니다.
- 씬에 맡기는 함수는 넷입니다. `track_asset(&애셋)`, `track_load_handle(&읽기 손잡이)`, `track_audio_asset(&Rc<AudioAsset>)`, `track_task(&작업 손잡이)`. `get_tracked_asset_progress()` 는 맡긴 것 가운데 끝난 비율(0 ~ 1, 실패 포함, 맡긴 것이 없으면 1)입니다.

## await 로 불러오기

원본의 `async load(engine) { await asset.load(경로); }` 는 `create` 에서 씬 작업을 맡기고 `track_task` 로 씬에 맡기는 꼴로 옮깁니다. 작업은 맡긴 자리에서 첫 `await` 까지 돌고, 씬과 엔진은 `scene_context.with` 의 클로저 안에서만 빌립니다. `load` 가 돌려준 읽기 손잡이(`AssetLoadHandle`)는 애셋을 빌리지 않으므로 클로저 밖에서 `.await` 합니다.

```rust,no_run
use vanilla::{wait, Engine, EngineConfiguration, JsonAsset, Scene, SceneBase};

struct StageScene {
	scene_base: SceneBase,
	stage_asset: JsonAsset,
	is_stage_ready: bool,
}

impl Scene for StageScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		let load_task = engine.spawn_scene_task(self, |mut scene_context| async move {
			let load_result = scene_context.with(|scene, engine| scene.stage_asset.load(engine, "examples/assets/scenedocument.scene.json")).await;
			if let Err(error_message) = load_result {
				eprintln!("{}", error_message);
			}
			wait::seconds(0.5).await;
			scene_context.with(|scene, _engine| {
				scene.is_stage_ready = true;
			});
		});
		self.scene_base.track_task(&load_task);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		if let Some(stage_data) = self.stage_asset.get_data() {
			println!("{} {}", stage_data["version"], self.is_stage_ready);
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let stage_scene = StageScene {
		scene_base: SceneBase::new(),
		stage_asset: JsonAsset::new(),
		is_stage_ready: false,
	};
	if let Err(error) = engine.run(Box::new(stage_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- 읽기 손잡이를 `await` 하면 `Result<Rc<Vec<u8>>, String>` 이 나옵니다. 받은 바이트이거나 실패 까닭입니다. (원본의 거부 자리)
- `ImageAsset` 의 손잡이는 원본 `img.onload` 처럼 그림을 풀기까지 끝나야 끝나고, 풀지 못하면 실패로 끝납니다. `JsonAsset` 의 손잡이는 원본처럼 받기만 끝나면 끝나고, 풀지 못한 JSON 은 `get_data()` 가 `None` 입니다.
- 이미 `load` 를 부른 애셋은 `wait_for_load()` 가 돌려준 퓨처를 나중에 `await` 해도 같습니다. (`Asset` 트레이트)
- async/await 의 자세한 쓰임은 "async/await" 장에서 봅니다.

## 애셋 경로

- 데스크탑은 `EngineConfiguration.asset_directory_paths` 의 폴더 후보에서 앞에서부터 파일을 찾습니다. 어디에도 없으면 첫 후보 기준 경로로 읽다가 실패하고, 후보가 비었으면(기본) 경로를 실행한 폴더 기준 그대로 씁니다. `engine.resolve_asset_path(경로)` 가 고른 경로를 돌려줍니다.
- 웹은 `asset_base_url + 경로` 를 `fetch` 합니다. `asset_base_url` 이 비었으면(기본) 페이지 기준 상대 주소입니다.
- `"data:"` 주소는 두 곳 모두 그대로 읽힙니다. 데스크탑은 엔진이 그 자리에서 풀고(퍼센트 꼴과 `;base64`), 웹은 기준 주소를 붙이지 않고 브라우저 `fetch` 에 맡깁니다.
- 게임 코드는 같고, 플랫폼마다 위 설정만 다릅니다.

```rust
use std::path::PathBuf;
use vanilla::{Engine, EngineConfiguration, TextAsset};

let mut engine_configuration = EngineConfiguration::new();
engine_configuration.asset_directory_paths = vec![PathBuf::from("examples/assets")];
engine_configuration.asset_base_url = String::from("assets/");
let engine = Engine::new(engine_configuration);

let mut text_asset = TextAsset::new();
let load_handle = text_asset.load(&engine, "data:,hello%20vanilla");
assert!(load_handle.is_loaded());
assert_eq!(text_asset.get_text(), Some("hello vanilla"));

let mut missing_asset = TextAsset::new();
let missing_handle = missing_asset.load(&engine, "missing.txt");
assert!(missing_handle.is_failed());
assert!(missing_handle.get_error_message().is_some());
assert_eq!(missing_asset.get_text(), None);
```

## 애셋 종류

| 구조체 | 받는 것 | 받은 뒤 읽는 곳 |
| --- | --- | --- |
| `ImageAsset` | 그림 (png, jpeg, webp, gif 첫 장, bmp, ico, svg, avif) | `get_image()` (`Option<Rc<Image>>`), `get_image_size()` (픽셀, 없으면 0) |
| `TextAsset` | 글 파일 | `get_text()` (UTF-8 로 풂) |
| `JsonAsset` | JSON 파일 | `get_data()` (`serde_json::Value`), `get_text()` |
| `BlobAsset` | 바이너리 | `get_blob()` (`Rc<Vec<u8>>`) |
| `BunchAsset` | 바이너리 (애셋 타입만 `Bunch`) | `get_blob()` |
| `FontAsset` | 글꼴 (ttf, otf, woff, woff2) | `get_family()`, `get_descriptors()`, `is_bold()` |
| `AudioAsset` | 소리 (wav, mp3, ogg 와 webm 의 Opus 와 Vorbis, flac, m4a 와 aac 의 AAC) | `get_audio_buffer()`, `get_duration()` (초) |
| `VisualAsset` | 받지 않습니다 (원본처럼 경로만 적음) | `set_visual`, `get_visual` |

- `AudioAsset` 을 뺀 애셋은 `Asset` 트레이트를 구현합니다. `is_loaded()`, `get_asset_path()`(요청 전이면 빈 글), `wait_for_load()`, `get_asset_base()` 가 있고, 애셋 타입은 `get_asset_base().get_asset_type()` 으로 읽습니다. `unload()` 는 애셋마다 있습니다.
- 받은 바이트는 처음 읽을 때 풉니다. 그래서 `get_image()` 와 `get_text()` 는 `&mut self` 를 받습니다. 그림을 풀지 못하면 표준 에러에 까닭을 찍고 다시 풀지 않습니다.
- `JsonAsset` 은 풀지 못한 JSON 을 읽히지 않은 것으로 칩니다. (`is_loaded()` 가 거짓, `get_data()` 가 `None`, `get_text()` 는 남음)
- `TextAsset` 과 `JsonAsset` 은 `set_no_cache(true)` 를 `load` 전에 주면 웹 `fetch` 에 `cache: "no-cache"` 를 붙입니다. 배포한 뒤에도 브라우저가 옛 데이터 표를 쓰는 일을 막습니다. 데스크탑은 보지 않습니다.
- `get_data()` 와 `SpriteAtlas::load_data` 가 쓰는 `serde_json::Value` 는 엔진이 다시 내보내지 않으므로, 게임의 `Cargo.toml` 에 `serde_json` 을 더해 씁니다.

```rust
use vanilla::{Asset, AssetType, BlobAsset, Engine, EngineConfiguration, JsonAsset, TextAsset};

let engine = Engine::new(EngineConfiguration::new());

let mut json_asset = JsonAsset::new();
json_asset.set_no_cache(true);
json_asset.load_from_bytes("stage.json", br#"{ "name": "forest", "enemies": [3, 5] }"#.to_vec());
assert!(json_asset.is_loaded());
let stage_data = json_asset.get_data().expect("json data");
assert_eq!(stage_data["name"], "forest");
assert_eq!(stage_data["enemies"][1], 5);

let mut text_asset = TextAsset::new();
text_asset.load(&engine, "data:,line%20one");
assert_eq!(text_asset.get_text(), Some("line one"));
assert_eq!(text_asset.get_asset_path(), "data:,line%20one");

let mut blob_asset = BlobAsset::new();
blob_asset.load(&engine, "data:application/octet-stream;base64,AAEC");
assert_eq!(blob_asset.get_blob().expect("bytes").as_slice(), &[0, 1, 2]);
assert_eq!(blob_asset.get_asset_base().get_asset_type(), AssetType::Blob);
blob_asset.unload();
assert!(!blob_asset.is_loaded());
```

## 글꼴

`FontAsset` 은 `load` 가 아니라 `load_font(engine, 글꼴 이름, 경로)` 로 받습니다. 바이트가 오면 엔진이 다음 프레임 시작에 글자 캐시의 글꼴 목록에 그 이름으로 올리고, 그 뒤로 `Text.set_font_family(글꼴 이름)` 이나 폰트 문자열(`"32px 글꼴 이름"`)에서 그 이름을 씁니다. `load_font` 들은 `&mut Engine` 을 받으므로 `create` 에서 부릅니다.

| 메서드 | 내용 |
| --- | --- |
| `load_font(engine, family, path)` | 굵기 400 얼굴로 올립니다 |
| `load_font_with_bold(engine, family, path, is_bold)` | `true` 면 굵기 700 얼굴 |
| `load_font_with_descriptors(engine, family, path, descriptors)` | `FontFaceDescriptors`(굵기 범위, 기울기, 글자 범위)대로 올립니다 |
| `load_font_from_bytes(engine, family, bytes)` | 이미 가진 바이트로 |

같은 이름에 굵기나 기울기가 다른 파일을 여러 개 올리면 글자 캐시가 CSS 처럼 골라 그립니다. 서술자는 `FontFaceDescriptors::create_from_css(weight, style, unicode_range)` 로 원본 서술자 글(`"700"`, `"italic"`, `"U+AC00-D7A3"`)에서 짓습니다. 아래 예제의 글꼴 파일은 저장소에 없으니 게임이 가진 파일 경로로 바꿉니다.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, FontAsset, FontFaceDescriptors, Graphic, Scene, SceneBase, Text, Vector2};

struct FontScene {
	scene_base: SceneBase,
	regular_font_asset: FontAsset,
	bold_font_asset: FontAsset,
}

impl Scene for FontScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.regular_font_asset.load_font(engine, "GameFont", "fonts/GameFont-Regular.ttf");
		let bold_descriptors = FontFaceDescriptors::create_from_css("700", "normal", "");
		self.bold_font_asset.load_font_with_descriptors(engine, "GameFont", "fonts/GameFont-Bold.ttf", bold_descriptors);
		self.scene_base.track_asset(&self.regular_font_asset);
		self.scene_base.track_asset(&self.bold_font_asset);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = node_tree.create_child(root_id, "title");
		let title_node = &mut node_tree[title_id];
		title_node.set_local_position(Vector2::create(640.0, 360.0));
		title_node.set_content_size(Vector2::create(800.0, 80.0));
		let title_text = title_node.add_component(Text::create_with_text("Hello, Vanilla Engine", 48.0));
		title_text.set_font_family("GameFont");
		title_text.set_bold(true);
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let font_scene = FontScene {
		scene_base: SceneBase::new(),
		regular_font_asset: FontAsset::new(),
		bold_font_asset: FontAsset::new(),
	};
	if let Err(error) = engine.run(Box::new(font_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- `load_font` 들은 `FontLoadHandle` 을 돌려줍니다. `await` 하면 글꼴 목록에 올린 뒤(또는 받기나 올리기에 실패하면 까닭으로) 끝납니다. `track_asset` 은 바이트 받기만 기다리지만, 올리기는 바로 다음 프레임 시작이라 0.3 초 뒤의 `initialize` 에서는 이미 올라 있습니다.
- 이미 요청한 `FontAsset` 에 `load_font` 를 다시 부르면 그 손잡이를 돌려줍니다. `unload(engine)` 는 원본처럼 그 얼굴을 글꼴 목록에서 뺍니다.
- 엔진이 처음 쓰는 기본 글꼴(`default_font_paths`, `default_font_bytes`, `default_font_url`)과 여러 언어 글꼴은 "언어 묶음" 장에서 다룹니다.

## 소리

`AudioAsset` 은 다른 애셋과 달리 `Asset` 트레이트가 없고, 플레이어들이 함께 들 수 있게 `Rc<AudioAsset>` 로 씁니다. 애셋 경로로 읽을 때는 `engine.load_audio_asset(경로)` 를 부르고 `track_audio_asset` 으로 맡깁니다.

```rust,no_run
use std::rc::Rc;
use vanilla::{AudioAsset, Engine, EngineConfiguration, Scene, SceneBase};

struct SoundScene {
	scene_base: SceneBase,
	hit_sound: Option<Rc<AudioAsset>>,
}

impl Scene for SoundScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		match engine.load_audio_asset("tests/assets/audio/wav_s16_stereo.wav") {
			Ok(audio_asset) => {
				self.scene_base.track_audio_asset(&audio_asset);
				self.hit_sound = Some(audio_asset);
			}
			Err(error_message) => {
				eprintln!("{}", error_message);
			}
		}
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		if let Some(hit_sound) = &self.hit_sound {
			println!("{}", hit_sound.get_duration());
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let sound_scene = SoundScene {
		scene_base: SceneBase::new(),
		hit_sound: None,
	};
	if let Err(error) = engine.run(Box::new(sound_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- 데스크탑은 `load_audio_asset` 이 그 자리에서 파일을 읽고 풀어 돌아오고, 실패하면 `Err(까닭)` 입니다. 소리는 원본의 크롬 `decodeAudioData` 와 같은 길이와 표본으로 풀고, 표본율은 44100 으로 바꿉니다. 꼴은 확장자가 아니라 내용으로 가립니다.
- 웹은 `fetch` 와 풀기를 걸어 두고 바로 애셋을 돌려줍니다. 끝나면 `is_loaded()` 가 참이 되고, 실패하면 `get_load_error()` 에 까닭이 남습니다. 그 자리에서 알 수 있는 실패만 `Err` 입니다.
- `Rc<AudioAsset>` 에 바로 `load_from_asset(engine, 경로)`, `load(파일 경로)`, `load_from_bytes(바이트)` 를 불러도 됩니다. 모두 `await` 할 수 있는 `AudioLoadHandle` 을 돌려주고, 이미 읽힌 애셋이면 아무것도 하지 않습니다.
- 받은 소리를 내는 방법은 "소리" 장에서 다룹니다.

```rust
use std::rc::Rc;
use vanilla::AudioAsset;

let audio_asset = Rc::new(AudioAsset::new());
let audio_load_handle = audio_asset.load_from_bytes(vec![0, 1, 2, 3]);
assert!(audio_load_handle.is_finished());
assert!(audio_asset.get_load_error().is_some());
assert!(!audio_asset.is_loaded());
assert_eq!(audio_asset.get_duration(), 0.0);
```

## AssetLoader 로 함께 받기

`AssetLoader` 는 목록에 넣은 것을 동시에 받는 수만큼씩 함께 받습니다.

- `add_image(path)`, `add_json(path)`, `add_text(path)`, `add_blob(path)` 는 아직 비어 있는 애셋을 만들어 목록에 넣고 그 애셋을 돌려줍니다. 다 받은 뒤 그 애셋에서 바로 읽습니다. `add_audio(path)` 는 `Rc<AudioAsset>` 을, `add_font(engine, family, path, descriptors)` 는 글꼴 바이트의 읽기 손잡이를 돌려줍니다. (원본은 `FontAsset` 을 돌려줍니다)
- 원본 `addTask` 자리는 `add_load_handle(&읽기 손잡이)` 입니다. 직접 요청했거나 직접 끝낼 손잡이(`AssetLoadHandle::create_pending(이름)` 으로 만든 것)를 넣으면 진행률에 같이 셉니다. 넣은 때부터 받는 중으로 치고, 끝나기 전까지 동시 수의 한 자리를 씁니다.
- `set_concurrency(count)` 로 동시에 받는 수를 정합니다. 기본은 6 이고, 1 보다 작으면 1 입니다.
- `load(engine)` 가 받기를 시작합니다. 데스크탑은 여기서 다 끝납니다. 웹은 그 뒤로 프레임마다 `update(engine)` 를 불러야 앞의 것이 끝난 만큼 다음 것을 요청합니다. `load` 가 돌려준 손잡이를 엔진 작업 안에서 `await` 하면 기다리는 동안 `update` 를 불러 주고 실패 목록(`Vec<AssetLoadFailure>`)으로 끝납니다.
- 실패한 것도 끝난 것으로 쳐서 멈추지 않습니다. 실패마다 표준 에러에 까닭을 찍고 `get_failed_list()` 에 `AssetLoadFailure { path, error_message }` 로 남깁니다. 그림은 다 받았을 때 풀어 보아 깨진 것을, JSON 은 풀지 못한 것을, 소리는 풀지 못한 것을 실패로 칩니다.
- 진행률은 끝난 수(실패 포함) 나누기 전체 수입니다. `get_progress()`(0 ~ 1, 넣은 것이 없으면 1), `get_finished_count()`, `get_loaded_count()`(실패 뺀 수), `get_total_count()`, `get_in_flight_count()`, `is_finished()` 로 읽습니다.
- `set_progress_handler(Some(Box::new(|progress, finished_count, total_count| { ... })))` 는 진행이 바뀔 때마다, `set_complete_handler(Some(Box::new(|failed_list| { ... })))` 는 다 끝났을 때 불립니다.
- `AssetLoader` 는 목록을 나눠 드는 값싼 복제 꼴입니다. 복제하면 같은 목록을 가리킵니다.

```rust
use std::cell::Cell;
use std::rc::Rc;
use vanilla::{Asset, AssetLoader, Engine, EngineConfiguration};

let engine = Engine::new(EngineConfiguration::new());
let asset_loader = AssetLoader::new();
asset_loader.set_concurrency(4);
let mut text_asset = asset_loader.add_text("data:,hello");
let json_asset = asset_loader.add_json("data:application/json,{\"level\":3}");
let blob_asset = asset_loader.add_blob("data:,abc");
let missing_asset = asset_loader.add_image("missing.png");
let last_progress = Rc::new(Cell::new(0.0));
let progress_reader = last_progress.clone();
asset_loader.set_progress_handler(Some(Box::new(move |progress: f32, _finished_count: usize, _total_count: usize| {
	progress_reader.set(progress);
})));
asset_loader.load(&engine);
assert!(asset_loader.is_finished());
assert_eq!(asset_loader.get_total_count(), 4);
assert_eq!(asset_loader.get_loaded_count(), 3);
assert_eq!(last_progress.get(), 1.0);
let failed_list = asset_loader.get_failed_list();
assert_eq!(failed_list.len(), 1);
assert_eq!(failed_list[0].path, "missing.png");
assert_eq!(text_asset.get_text(), Some("hello"));
assert_eq!(json_asset.get_data().expect("json data")["level"], 3);
assert_eq!(blob_asset.get_blob().expect("bytes").len(), 3);
assert!(!missing_asset.is_loaded());
```

## 로딩 막대

씬에 맡길 때는 `asset_loader.get_completion_handle()` 을 `track_load_handle` 로 맡깁니다. 이 손잡이는 다 받으면 끝나므로 엔진이 그때까지 `initialize` 를 미룹니다. 그동안 `draw_on_load` 에서 `update` 를 부르고 로딩 막대를 그립니다.

`LoadingProgress` 는 알려 준 정도(`set_progress`, 0 ~ 1 로 자르고 줄어들지 않음)를 보이는 정도(`get_shown_progress`)가 부드럽게 따라가게 합니다. 매 프레임 `update(time_delta)` 를 부르고, 따라가는 빠르기는 `set_follow_rate`(초당 남은 거리의 비율, 기본 6)로 줍니다. 받기가 몰려 진행률이 한 번에 뛰어도 막대가 미끄러집니다.

```rust,no_run
use vanilla::{AssetLoader, Colors, Engine, EngineConfiguration, Graphic, ImageAsset, JsonAsset, LoadingProgress, Rect, Scene, SceneBase};

struct LoadingScene {
	scene_base: SceneBase,
	asset_loader: AssetLoader,
	loading_progress: LoadingProgress,
	icon_asset: Option<ImageAsset>,
	atlas_json_asset: Option<JsonAsset>,
}

impl Scene for LoadingScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.asset_loader.set_concurrency(4);
		self.icon_asset = Some(self.asset_loader.add_image("examples/assets/atlas/icons.png"));
		self.atlas_json_asset = Some(self.asset_loader.add_json("examples/assets/atlas/icons.json"));
		let completion_handle = self.asset_loader.get_completion_handle();
		self.scene_base.track_load_handle(&completion_handle);
		self.asset_loader.load(engine);
	}

	fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.asset_loader.update(engine);
		self.loading_progress.set_progress(self.asset_loader.get_progress());
		let time_delta = engine.get_time_manager().get_unscale_delta_time();
		self.loading_progress.update(time_delta);
		let shown_progress = self.loading_progress.get_shown_progress();

		self.scene_base.draw_scene_background(engine, graphic);
		let view_size = engine.get_view_manager().get_view_size();
		let bar_width = view_size.x * 0.6;
		let bar_left = (view_size.x - bar_width) * 0.5;
		let bar_top = view_size.y * 0.8;
		graphic.set_fill_color("#404040");
		graphic.draw_rect(Rect::create(bar_left, bar_top, bar_width, 12.0));
		graphic.set_fill_color(Colors::WHITE);
		graphic.draw_rect(Rect::create(bar_left, bar_top, bar_width * shown_progress, 12.0));
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		for failure in self.asset_loader.get_failed_list() {
			eprintln!("{} {}", failure.path, failure.error_message);
		}
		if let Some(icon_asset) = self.icon_asset.as_mut() {
			println!("{}", icon_asset.get_image_size().x);
		}
		if let Some(atlas_json_asset) = &self.atlas_json_asset {
			println!("{}", atlas_json_asset.get_data().is_some());
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let loading_scene = LoadingScene {
		scene_base: SceneBase::new(),
		asset_loader: AssetLoader::new(),
		loading_progress: LoadingProgress::new(),
		icon_asset: None,
		atlas_json_asset: None,
	};
	if let Err(error) = engine.run(Box::new(loading_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

`GameSceneBase` 를 쓰는 씬은 `track_asset_loader(&asset_loader)` 한 줄로 같은 일을 합니다. 기본 로딩 화면의 `draw_on_load` 가 프레임마다 `update` 를 부르고 진행률을 막대에 넘깁니다.

## 이미 가진 바이트로 읽기

압축 파일에서 꺼낸 것처럼 바이트를 이미 들고 있으면 받지 않고 바로 읽힙니다. 이미 요청한 애셋이어도 새 바이트로 바꿉니다. (`FontAsset` 과 `AudioAsset` 은 이미 읽기를 걸었으면 아무것도 하지 않습니다)

| 구조체 | 메서드 | 돌려줌 |
| --- | --- | --- |
| `ImageAsset` | `load_from_bytes(asset_path, bytes)` | 읽기 손잡이 (풀지 못하면 실패로 끝남) |
| `TextAsset`, `JsonAsset`, `BlobAsset`, `BunchAsset` | `load_from_bytes(asset_path, bytes)` | 없음 |
| `FontAsset` | `load_font_from_bytes(engine, family, bytes)`, `load_font_from_bytes_with_descriptors(engine, family, bytes, descriptors)` | `FontLoadHandle` |
| `AudioAsset` | `load_from_bytes(bytes)` (경로 인자 없음, `Rc<AudioAsset>` 에 부름) | `AudioLoadHandle` |

`asset_path` 는 알림 글과 `get_asset_path()` 에 쓰는 이름입니다. 바이트는 `Vec<u8>` 로 넘겨 애셋이 가집니다. 직접 만든 애셋 구조체는 `AssetBase` 를 필드로 들고 `Asset` 트레이트의 `get_asset_base`, `get_asset_base_mut` 만 채우면 `track_asset` 에 맡길 수 있습니다.

```rust
use vanilla::{Asset, AssetBase, AssetType, ImageAsset};

struct NoteAsset {
	asset_base: AssetBase,
}

impl Asset for NoteAsset {
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}

let mut note_asset = NoteAsset {
	asset_base: AssetBase::new(AssetType::Text),
};
note_asset.get_asset_base_mut().load_from_bytes("note.txt", b"hello".to_vec());
assert!(note_asset.is_loaded());
assert_eq!(note_asset.get_asset_path(), "note.txt");

let mut image_asset = ImageAsset::new();
let load_handle = image_asset.load_from_bytes("broken.png", vec![0, 1, 2, 3]);
assert!(load_handle.is_loaded());
assert!(image_asset.get_image().is_none());
assert_eq!(image_asset.get_image_size().x, 0.0);
```

## SpriteAtlas

여러 그림을 쪽 그림 몇 장에 모아 두고 이름으로 꺼냅니다. 같은 쪽 그림을 쓰는 스프라이트는 엔진이 묶어 한 번에 그리므로 그리기 호출이 줄어듭니다.

읽는 JSON 은 원본과 같은 두 꼴입니다. 하나는 원본 vanilla.js 저장소(https://github.com/ddukbaek2/vanilla.js)의 `tools/atlas.cjs` 가 만드는 꼴이고, 이 저장소에는 그 도구가 없습니다. 저장소의 `examples/assets/atlas/icons.json` 이 그 꼴입니다.

```json
{
	"images": {
		"coin.png": { "x": 84, "y": 55, "width": 30, "height": 30, "page": 0 }
	},
	"meta": {
		"imageFileName": "icons.png",
		"pages": [{ "imageFileName": "icons.png", "width": 128, "height": 128 }]
	}
}
```

- `page` 가 없으면 0 쪽이고, `meta.pages` 가 없으면 `meta.imageFileName` 한 장입니다.
- 다른 하나는 쪽 경로 목록과 `[쪽, x, y, 가로, 세로]` 표입니다. (`{ "pagePaths": [...], "frames": { "이름": [...] } }`)
- 쪽 그림 경로에 빗금(/)이 없으면 JSON 이 있는 폴더(`load_data` 는 `base_directory`)를 앞에 붙이고, 있으면 적힌 그대로 읽습니다. `"./"` 로 시작하면 그것을 떼고 애셋 경로로 읽습니다.
- 그림 이름은 파일 이름으로도, 확장자를 뗀 이름으로도 찾습니다. (`"coin.png"` 는 `"coin"` 으로도 찾음)

JSON 파일은 `load(engine, json_path)` 로 읽습니다. 데스크탑은 이 자리에서 JSON 과 쪽 그림이 다 읽힙니다. 웹은 JSON 이 온 뒤 쪽 그림을 청해야 하므로, `load` 가 돌려준 손잡이를 엔진 작업 안에서 `await` 합니다. (기다리는 동안 프레임마다 `update` 를 불러 줍니다)

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, Sprite, SpriteAtlas, Vector2};

struct AtlasScene {
	scene_base: SceneBase,
	sprite_atlas: SpriteAtlas,
}

impl Scene for AtlasScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		let load_task = engine.spawn_scene_task(self, |mut scene_context| async move {
			scene_context.with(|scene, engine| scene.sprite_atlas.load(engine, "examples/assets/atlas/icons.json")).await;
		});
		self.scene_base.track_task(&load_task);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let frame_names = ["star", "heart", "leaf", "gem", "coin"];
		for (frame_index, frame_name) in frame_names.iter().enumerate() {
			let icon_id = node_tree.create_child(root_id, frame_name);
			let icon_node = &mut node_tree[icon_id];
			icon_node.set_local_position(Vector2::create(240.0 + frame_index as f32 * 200.0, 360.0));
			icon_node.set_content_size(Vector2::create(96.0, 96.0));
			let sprite = icon_node.add_component(Sprite::new());
			self.sprite_atlas.apply_frame(sprite, frame_name);
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let atlas_scene = AtlasScene {
		scene_base: SceneBase::new(),
		sprite_atlas: SpriteAtlas::new(),
	};
	if let Err(error) = engine.run(Box::new(atlas_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

코드에 둔 표는 `load_data(engine, &atlas_data, base_directory)` 로 읽습니다. 아래 예제는 쪽 그림 자리에 64 x 32 SVG 를 `"data:"` 주소로 두고 32 x 32 조각 둘로 나눕니다.

```rust
use std::cell::RefCell;
use std::rc::Rc;
use vanilla::{Engine, EngineConfiguration, Rect, Sprite, SpriteAtlas};

let engine = Engine::new(EngineConfiguration::new());
let atlas_data = serde_json::json!({
	"pagePaths": ["data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='64' height='32'/>"],
	"frames": {
		"left": [0, 0, 0, 32, 32],
		"right": [0, 32, 0, 32, 32]
	}
});
let mut sprite_atlas = SpriteAtlas::new();
sprite_atlas.load_data(&engine, &atlas_data, "");
assert!(sprite_atlas.is_loaded());
assert_eq!(sprite_atlas.get_page_count(), 1);
let right_frame = sprite_atlas.read_frame("right").expect("right frame");
assert_eq!(right_frame.get_image_rect(), Rect::create(32.0, 0.0, 32.0, 32.0));
assert_eq!(right_frame.get_image().get_width(), 64);

let mut sprite = Sprite::new();
assert!(sprite_atlas.apply_frame(&mut sprite, "left"));
assert!(!sprite_atlas.apply_frame(&mut sprite, "missing"));
assert!(sprite.get_image().is_none());

let shared_atlas = Rc::new(RefCell::new(sprite_atlas));
SpriteAtlas::add_search_atlas(&shared_atlas);
assert!(SpriteAtlas::apply_search_frame(&mut sprite, "right"));
assert_eq!(sprite.get_image_rect(), Rect::create(32.0, 0.0, 32.0, 32.0));
SpriteAtlas::clear_search_atlases();
assert!(SpriteAtlas::find_frame("right").is_none());
```

- `read_frame(name)` 은 `Option<&Frame>`(그림과 그 안의 영역)을 돌려줍니다. `has_frame(name)`, `get_frame_names()`, `get_page_images()`, `get_page_count()`, `is_loaded()` 도 있습니다.
- `apply_frame(&mut sprite, name)` 은 스프라이트에 조각을 겁니다. 없는 이름이면 스프라이트 그림을 비우고 `false` 를 돌려줍니다.
- 찾을 아틀라스 목록은 `Rc<RefCell<SpriteAtlas>>` 로 넣습니다. `SpriteAtlas::add_search_atlas` 로 넣어 두면 `SpriteAtlas::find_frame(name)`(조각의 복사, `Option<Frame>`), `SpriteAtlas::apply_search_frame(&mut sprite, name)` 이 넣은 차례로 찾습니다(앞의 것이 이깁니다). `remove_search_atlas`, `clear_search_atlases` 로 뺍니다. 목록은 스레드 하나에 하나입니다. 전역 목록을 쓰지 않으려면 `SpriteAtlas::find_frame_in_order(&[&atlas], name)`, `SpriteAtlas::apply_frame_in_order(&[&atlas], &mut sprite, name)` 을 씁니다.
- `await` 없이 쓰려면 `create` 에서 `load` 한 뒤 `track_json_asset(&mut scene_base)` 로 JSON 을 씬에 맡기고, `is_loaded()` 가 참이 될 때까지 `draw_on_load` 나 `tick` 에서 `update(engine)` 를 부릅니다. 쪽 그림은 JSON 이 온 뒤 `update` 가 청합니다.
- 다시 `load` 하는 동안은 원본처럼 앞서 다 읽힌 조각을 그대로 씁니다.

## 주의할 점

- 원본의 `load` 는 실패하면 예외를 냅니다. 러스트 판은 예외가 없습니다. 읽기 손잡이가 실패(`is_failed()`, `get_error_message()`)로 끝나고, `await` 하면 `Err(까닭)` 입니다. 씬은 실패한 애셋도 끝난 것으로 칩니다.
- 그림 손잡이는 바이트가 다 오면 `is_loaded()` 가 참입니다. 깨진 그림인지는 `get_image()` 가 `None` 인지로 압니다. (손잡이를 `await` 했거나 `AssetLoader` 로 받았으면 그때 실패로 끝납니다)
- 같은 애셋에 `load` 를 두 번 부르면 두 번째는 첫 요청의 손잡이를 돌려줍니다. 다른 경로로 바꿔 받으려면 `unload` 뒤에 부르거나 `load_from_bytes` 로 바꿉니다.
- `AudioAsset` 은 `Asset` 이 아니어서 `track_asset` 대신 `track_audio_asset` 으로 맡깁니다. `SpriteAtlas` 도 애셋이 아니어서 `track_json_asset` 이나 작업 맡기기(`track_task`)로 기다립니다.
- `SpriteAtlas` 는 JSON 을 읽지 못하거나 꼴을 모르면 표준 에러에 알리고 조각 없이 끝납니다. 손잡이의 `await` 도 끝나므로 `is_loaded()` 로 확인합니다.
- `AssetLoader` 의 `add_font` 는 원본과 달리 `FontAsset` 이 아니라 읽기 손잡이를 돌려줍니다.
- `draw_on_load` 는 맡긴 것이 끝나기 전까지만 불립니다. 이때 그리는 좌표계는 직접 맞춥니다. (`draw_scene_background` 나 `engine.get_view_manager().apply_view_rect(graphic)`)

## 관련 타입

Asset, AssetBase, AssetType, AssetLoadHandle, AssetLoadFuture, ImageAsset, TextAsset, JsonAsset, BlobAsset, BunchAsset, FontAsset, FontLoadHandle, FontFaceDescriptors, AudioAsset, AudioLoadHandle, VisualAsset, AssetLoader, AssetLoaderLoadHandle, AssetLoadFailure, LoadingProgress, SpriteAtlas, SpriteAtlasLoadHandle, Frame, Image, Sprite, SceneBase, EngineConfiguration
