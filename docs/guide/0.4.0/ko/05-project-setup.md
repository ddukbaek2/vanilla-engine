# 프로젝트 시작

게임 크레이트에 엔진을 의존성으로 넣는 법, 기능 플래그와 빌드 프로필, 프로젝트 폴더 꼴과 애셋 폴더, 가장 작은 `main.rs`, 데스크탑 실행과 웹 빌드를 차례로 봅니다.

## 엔진 의존성

엔진은 크레이트 하나입니다. 게임의 `Cargo.toml` 에 넣습니다.

```toml
[package]
name = "mygame"
version = "0.1.0"
edition = "2021"

[dependencies]
vanilla-engine = { git = "https://github.com/ddukbaek2/vanilla-engine" }
```

- 크레이트 이름은 `vanilla-engine`, 코드에서 쓰는 이름은 `vanilla` 입니다. (`use vanilla::{Engine, Scene};`)
- 반사 매크로는 따로 된 크레이트(`vanilla-engine-macros`)에 있지만 엔진이 `vanilla::reflect` 로 다시 내보냅니다. 게임은 엔진 하나만 의존합니다.
- git 의존성은 저장소의 기본 브랜치를 받습니다. 판을 묶으려면 Cargo 의 `branch`, `tag`, `rev` 키를 씁니다. 엔진 저장소를 옆에 받아 두고 고치면서 쓸 때는 `path` 로 가리킵니다.

```toml
[dependencies]
vanilla-engine = { path = "../vanilla-engine" }
```

## 기능 플래그

| 기능 | 처음 | 하는 일 |
| --- | --- | --- |
| `tls` | 켬 | 데스크탑 `WebSocketClient` 가 `wss://` 를 엽니다. (ewebsock 의 tls, rustls 와 암호 공급자 ring) 웹은 브라우저 WebSocket 이라 상관없습니다 |

`default-features = false` 로 끄면 데스크탑은 `ws://` 만 열립니다. ring 이 C 를 빌드하므로, 윈도우 C 헤더가 없는 맥에서 윈도우 대상을 검사할 때도 끕니다.

```toml
[dependencies]
vanilla-engine = { git = "https://github.com/ddukbaek2/vanilla-engine", default-features = false }
```

```bash
rustup target add x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc --no-default-features
```

## 빌드 프로필

엔진 `Cargo.toml` 에 적힌 `[profile.release]` 와 `[profile.wasm-release]` 는 엔진 저장소를 맨 위로 빌드할 때만 쓰입니다. Cargo 는 의존성 크레이트의 프로필을 읽지 않으므로, 같은 설정을 쓰려면 게임의 `Cargo.toml` 에 옮겨 둡니다.

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

- `release` 는 한 덩어리로 묶어 최적화하고 디버그 기호를 뺍니다.
- `wasm-release` 는 `release` 와 같되 기호를 남겨 wasm-bindgen 이 wasm 기능 정보를 읽게 합니다. 크기를 줄이는 것은 wasm-bindgen 이 합니다.
- 데스크탑 AVIF 그림은 rav1d 로 풉니다. rav1d 는 opt-level 0 이면 느려서(1600 x 1200 한 장이 dev 빌드에서 2.9 초, rav1d 만 opt-level 3 이면 0.9 초) dev 빌드에서도 rav1d 만 최적화하기를 권합니다.

```toml
[profile.dev.package.rav1d]
opt-level = 3
```

## 프로젝트 폴더 꼴

엔진이 정한 꼴은 없습니다. 아래는 한 예입니다.

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

| 자리 | 쓰임 |
| --- | --- |
| `src/main.rs` | 진입 파일. 엔진 설정을 만들고 첫 씬으로 `Engine::run` 을 부릅니다 |
| `assets/` | 애셋 폴더. 데스크탑은 `EngineConfiguration::asset_directory_paths` 로, 웹은 `asset_base_url` 로 가리킵니다 |
| `web/` | 웹 빌드의 출력 자리. wasm-bindgen 이 낸 `.js` 와 `.wasm` 옆에 `index.html` 을 둡니다 |

## 애셋 폴더

애셋은 애셋 폴더 기준 상대 경로(`"images/logo.png"` 같은)로 요청합니다. 같은 게임 코드가 두 플랫폼에서 이렇게 찾습니다.

| 플랫폼 | 찾는 법 |
| --- | --- |
| 데스크탑 | `asset_directory_paths` 의 후보를 앞에서부터 보고 파일이 있는 첫 후보에서 읽습니다. 어디에도 없으면 첫 후보 기준 경로이고, 후보가 없으면 상대 경로 그대로입니다 |
| 웹 | `asset_base_url` 과 상대 경로를 이은 주소를 fetch 합니다. 비우면(처음 값) 페이지 기준 상대 주소입니다 |

데스크탑의 상대 경로 후보는 실행할 때의 작업 폴더 기준입니다. `cargo run` 은 부른 자리를 작업 폴더로 두므로, 실행 파일을 다른 자리에서 열 때를 위해 실행 파일 옆 폴더도 후보에 넣을 수 있습니다.

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

`Engine::resolve_asset_path` 는 데스크탑에서 실제로 읽을 경로를 돌려줍니다. 이 예제는 어느 후보에도 파일이 없어 첫 후보 기준 경로가 나옵니다.

## 가장 작은 main.rs

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

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		engine.get_view_manager().apply_view_rect(graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
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

- `Graphic` 의 변환은 매 프레임 처음에 단위 행렬로 돌아갑니다. 그대로 그리면 좌표가 그리기 버퍼의 실제 픽셀입니다.
- `pre_draw` 에서 `ViewManager::apply_view_rect` 를 부르면 그다음 그리기가 뷰 좌표로 바뀝니다. (0, 0) 이 뷰의 왼쪽 위이고, 오른쪽 아래가 `ViewManager::get_view_size` 입니다.
- 이 예제는 바탕을 칠하지 않아 화면이 프레임 처음에 비운 검정으로 남습니다. 창 끝까지 바탕색을 칠하려면 `SceneBase::draw_scene_background` 를 부릅니다. (칠한 뒤 뷰 좌표를 겁니다)
- 덮어쓴 `draw` 에서 `self.scene_base.draw(graphic)` 을 부르면 씬의 노드 트리도 그립니다.
- 기준 해상도, 뷰 스케일 모드, 씬의 훅 차례는 "엔진과 씬 생명 주기" 장에서 봅니다.

## 데스크탑 실행과 배포

```bash
cargo run
cargo run --release
cargo build --release
```

- 출시 빌드의 실행 파일은 `target/release/` 에 생깁니다. 애셋 폴더를 실행 파일과 함께 옮기고, `asset_directory_paths` 가 그 자리를 가리키게 합니다.
- 창 설정은 `EngineConfiguration` 의 칸으로 정합니다.

| 칸 | 처음 값 | 뜻 |
| --- | --- | --- |
| `title` | 엔진의 한글 이름 | 창 제목 |
| `window_size` | `Vector2::zero()` | 처음 창 크기 (논리 크기, 0 이면 기준 해상도) |
| `is_resizable` | `true` | 창 크기를 바꿀 수 있는지 |
| `use_vertical_sync` | `true` | 수직 동기 |
| `window_icon_path` | `""` | 창 아이콘 PNG (애셋 폴더 기준 상대 경로, 비우면 기본 아이콘) |
| `default_font_paths` | 빈 목록 | 기본 글꼴 파일. 비우면 운영체제 글꼴에서 원본의 시스템 글꼴 이름 차례로 찾습니다 |

## 웹 빌드

`wasm32-unknown-unknown` 타깃과 wasm-bindgen 명령줄 도구가 있어야 합니다. wasm-bindgen 명령줄 도구의 판은 `Cargo.lock` 에 묶인 `wasm-bindgen` 크레이트 판과 같아야 합니다.

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

- 애셋은 페이지에서 `asset_base_url` 과 이은 주소로 닿는 자리에 둡니다. 게임 코드의 경로(`"images/logo.png"`)를 두 플랫폼에서 같게 쓰려면 데스크탑은 `asset_directory_paths` 를 `assets` 로, 웹은 `asset_base_url` 을 `"assets/"` 로 두고 애셋을 `web/assets/` 에 옮깁니다.
- 캔버스는 `canvas_id`(처음 `mainCanvas`)로 찾고, 없으면 엔진이 그 아이디로 만들어 `body` 끝에 붙입니다. 페이지에 미리 둔 캔버스를 쓰려면 같은 아이디를 줍니다.
- 모듈 스크립트와 애셋 받기는 `file://` 로 연 페이지에서 브라우저가 막으므로 로컬 웹 서버로 엽니다.

웹에서만 쓰는 칸입니다.

| 칸 | 처음 값 | 뜻 |
| --- | --- | --- |
| `canvas_id` | `"mainCanvas"` | 그릴 캔버스의 아이디 |
| `asset_base_url` | `""` | 애셋 주소 앞에 붙일 것 |
| `default_font_url` | `""` | 기본 글꼴 주소. `default_font_bytes` 가 없으면 여기서 받습니다. 비우면 받지 않고 브라우저 시스템 글꼴로 그립니다 |
| `resume_audio_on_touch_end` | `false` | `touchend` 와 `pointerup` 도 사용자 동작으로 보고 오디오를 깨울지 |

## 주의할 점

- 엔진 `Cargo.toml` 의 프로필은 게임 빌드에 이어지지 않습니다. 출시 설정과 `wasm-release` 는 게임 `Cargo.toml` 에 둡니다.
- 데스크탑 애셋 폴더의 상대 경로는 실행할 때의 작업 폴더 기준입니다. 다른 자리에서 실행 파일을 열면 애셋을 못 찾을 수 있습니다.
- 엔진은 글꼴을 따로 넣거나 받지 않습니다. `default_font_url` 의 처음 값은 빈 글입니다(원본 `defaultFontUrl` 과 같다). 게임 글꼴은 `default_font_url`, `default_font_bytes`, `FontAsset` 가운데 하나로 줍니다.
- `title` 은 데스크탑 창 제목입니다. 웹은 시작할 때 이 칸을 문서 제목에 쓰지 않으므로 `index.html` 의 `<title>` 로 정하거나, 실행 중에 `Engine::set_title` 을 부릅니다.
- 리눅스 대상 검사는 cpal 의 alsa-sys 와 gilrs 의 libudev-sys 가 pkg-config 를 부릅니다. 맥에서 검사할 때는 빈 플래그를 내는 pkg-config 를 `PKG_CONFIG` 로 주고 `PKG_CONFIG_ALLOW_CROSS=1` 로 검사합니다.

## 관련 타입

Engine, EngineConfiguration, Scene, SceneBase, ViewManager, Graphic, Colors
