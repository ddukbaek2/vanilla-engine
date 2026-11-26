# 엔진과 씬 생명 주기

`EngineConfiguration` 으로 엔진을 정하고 `Engine::run` 으로 첫 씬을 돌리는 데서 시작해, 씬의 훅이 불리는 차례, 씬 바꾸기, 시간, 화면 크기와 기준 해상도, 페이지 이벤트를 봅니다.

## 엔진 설정 (EngineConfiguration)

`EngineConfiguration::new()` 로 만든 뒤 칸을 고쳐 `Engine::new(engine_configuration)` 에 넘깁니다. (`Default` 도 같은 값입니다) 처음 값은 `new` 에 적힌 값입니다.

| 칸 | 처음 값 | 뜻 |
| --- | --- | --- |
| `reference_resolution_size` | `Vector2::create(1280.0, 720.0)` | 기준 해상도. 뷰 스케일 모드가 `None` 이 아니면 씬은 이 좌표계로 그립니다 |
| `view_scale_mode` | `ViewScaleMode::StretchShort` | 엔진을 만들 때 뷰 매니저에 거는 뷰 스케일 모드 |
| `title` | 엔진의 한글 이름 | 데스크탑 창 제목 |
| `window_size` | `Vector2::zero()` | 데스크탑 처음 창 크기 (논리 크기, 0 이면 기준 해상도) |
| `is_resizable` | `true` | 데스크탑 창 크기를 바꿀 수 있는지 |
| `use_statistics` | `false` | 화면 왼쪽 위에 FPS, 드로우 콜, 정점 수, 거른 노드 수를 그릴지 |
| `use_dev_tools` | `true` | `GameSceneBase`(와 그것을 품은 `UISceneBase`)가 개발자 도구(F2)를 만들지. 출시 빌드에서 끕니다 |
| `maximum_frame_per_second` | `0.0` | 최대 FPS. 0 이면 제한하지 않습니다. (수직 동기를 켜면 화면 주사율을 따릅니다) 실행 중에는 `Engine::set_maximum_frame_per_second` 로 바꿉니다 |
| `use_vertical_sync` | `true` | 데스크탑 수직 동기 |
| `default_font_paths`, `default_bold_font_paths` | 빈 목록 | 데스크탑 기본 글꼴 사슬의 파일 (보통, 굵게). 비우면 원본의 시스템 글꼴 이름 차례로 운영체제 글꼴을 찾아 잇습니다 |
| `fallback_font_paths` | 빈 목록 | 데스크탑 대체 글꼴 후보. 비우면 운영체제가 고른 대체 글꼴로 그립니다 |
| `asset_directory_paths` | 빈 목록 | 데스크탑 애셋 폴더 후보. `Engine::resolve_asset_path` 가 앞에서부터 찾습니다 |
| `window_icon_path` | `""` | 데스크탑 창 아이콘 PNG (애셋 폴더 기준 상대 경로, 비우면 기본 아이콘) |
| `default_font_bytes`, `default_bold_font_bytes` | `None` | 기본 글꼴 파일 내용. 있으면 경로 후보보다 먼저 씁니다 |
| `canvas_id` | `"mainCanvas"` | 웹에서 그릴 캔버스 아이디. 그런 요소가 없으면 그 아이디로 만들어 `body` 에 붙입니다 |
| `default_font_url` | `""` | 웹 기본 글꼴 주소. `default_font_bytes` 가 없으면 여기서 받습니다. 비우면 받지 않습니다 |
| `resume_audio_on_touch_end` | `false` | 웹에서 `touchend`, `pointerup` 도 사용자 동작으로 보고 오디오를 깨울지 |
| `asset_base_url` | `""` | 웹 애셋 주소 앞에 붙일 것 (비우면 페이지 기준 상대 주소) |
| `auto_resize_on_window_resize` | `false` | 창 크기가 바뀌면 캔버스를 창에 맞출지. 켜면 화면 키보드가 가린 아래를 뺀 크기에 맞추고 `max_canvas_aspect_ratio` 를 겁니다 |
| `preserve_drawing_buffer` | `false` | 프레임이 끝난 뒤에도 그린 것을 남길지. 켜면 프레임 첫머리에 색을 비우지 않아 지난 프레임 그림 위에 그립니다 |
| `ignore_virtual_keyboard_resize` | `false` | 글자 입력칸에 초점이 있는 동안 가로는 같고 세로만 줄어든 크기 바뀜(화면 키보드)을 무시할지 (`auto_resize_on_window_resize` 일 때) |
| `max_canvas_aspect_ratio` | `0.0` | 캔버스의 가로세로 비 상한. 창이 그보다 길쭉하면 캔버스를 줄여 가운데에 둡니다. 0 이면 제한하지 않습니다 |
| `rebake_text_on_font_load` | `false` | 글꼴이 늦게 받아지면 그 글꼴의 글자 범위에 든 구운 글자를 버려 다시 구울지 |
| `document_language` | `""` | 문서 언어 (웹 판 `index.html` 의 html lang 자리, `Document::get_language`) |
| `document_meta_list` | 빈 목록 | 문서의 meta 목록 (이름, content) (`Document::get_meta_content`, `BuildInfo` 가 build-version 을 읽습니다) |

## 엔진 만들고 돌리기

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

`Engine::new(engine_configuration)` 은 이런 일을 합니다.

- 설정을 넘겨받아 엔진이 소유합니다. 읽을 때는 `get_engine_configuration` 입니다.
- 시간, 뷰, 입력, 마우스 커서, 오디오 매니저를 만들고 문서 언어와 meta 를 설정대로 둡니다. 소리 장치는 처음 쓸 때 엽니다.
- 창과 GL 은 아직 만들지 않습니다. 그래서 창 없이 엔진을 만들어 매니저와 설정을 다룰 수 있습니다.

```rust
use std::path::PathBuf;
use vanilla::{Engine, EngineConfiguration, Vector2, ViewScaleMode};

let mut engine_configuration = EngineConfiguration::new();
engine_configuration.reference_resolution_size = Vector2::create(1920.0, 1080.0);
engine_configuration.view_scale_mode = ViewScaleMode::StretchShortExpandLong;
engine_configuration.asset_directory_paths = vec![PathBuf::from("assets")];
let mut engine = Engine::new(engine_configuration);
assert_eq!(engine.get_frame_number(), 1);
assert_eq!(engine.get_canvas().is_some(), engine.get_graphic().is_some());
let view_manager = engine.get_view_manager();
assert_eq!(view_manager.get_reference_resolution_size(), Vector2::create(1920.0, 1080.0));
assert_eq!(view_manager.get_view_scale_mode(), ViewScaleMode::StretchShortExpandLong);
engine.set_maximum_frame_per_second(30.0);
assert_eq!(engine.get_engine_configuration().maximum_frame_per_second, 30.0);
```

`engine.run(scene)` 은 엔진을 넘겨받아(`self`) 첫 씬을 불러오기 목록에 넣고 플랫폼에 넘깁니다. 플랫폼이 창이나 캔버스와 GL 컨텍스트를 만들고 기본 글꼴을 읽은 뒤 첫 씬을 올리고 프레임 루프를 돕니다.

- 데스크탑은 창을 닫거나 `Engine::exit_application` 을 부를 때까지 돌아오지 않습니다. 웹은 캔버스와 WebGL2 를 잡고 `requestAnimationFrame` 을 걸어 둔 뒤 바로 돌아옵니다.
- 창이나 GL 컨텍스트를 만들지 못하는 등 플랫폼이 실패하면 `Err(까닭)` 을 돌려줍니다.
- `run` 은 엔진을 가져가므로 한 번만 부릅니다. 다음 씬부터는 씬 훅이 받은 엔진의 `load_scene` 으로 올립니다.

엔진이 가진 것은 `get_time_manager`, `get_view_manager`, `get_input_manager`, `get_audio_manager`, `get_pointer_cursor` 와 각각의 `_mut` 판, `get_platform`, `get_canvas`(데스크탑은 창, 웹은 캔버스), `get_graphic`(원본 생성자처럼 데스크탑은 `Engine::new` 직후부터 있습니다. 화면 없는 기계, 주 스레드가 아닌 곳, 웹은 `run` 이 창이나 캔버스를 잡기 전에는 `None`), `get_engine_configuration` 으로 얻습니다. 원본과 다르게 전역 엔진(`Engine.getEngine()`)이 없고, 씬 훅이 `&mut Engine` 을 인자로 받습니다. 그리기 훅은 `Graphic` 도 인자로 받습니다. 엔진이 든 그래픽을 잠깐 꺼내 넘긴 것이라 그 안에서는 `engine.get_graphic()` 이 `None` 이고 인자의 `graphic` 을 씁니다.

## 씬의 생명 주기

씬은 `Scene` 트레이트를 구현하고, 씬 공통 상태(노드 트리, 트윈, 맡긴 애셋, 배경색)를 든 `SceneBase` 를 필드로 품습니다. 꼭 구현할 것은 `get_scene_base`, `get_scene_base_mut` 둘이고, 나머지 훅은 필요한 것만 덮어씁니다. 불리는 차례는 이렇습니다.

1. 씬 값을 만듭니다. `SceneBase::new()` 가 이름이 "root" 인 루트 노드를 함께 만듭니다. (원본은 루트를 `create` 에서 만듭니다)
2. `Engine::load_scene` 이나 `Engine::run` 으로 넘기면 씬 핸들(`SceneId`)이 매겨지고, 그 프레임이 끝날 때 `create(engine)` 를 부릅니다. 바로 뒤에 엔진이 루트를 뷰 크기로 맞춥니다. (자리 0, 왼쪽 위 피벗, 콘텐츠 크기는 뷰 크기)
3. `create` 에서 맡긴 애셋과 작업(`track_asset`, `track_load_handle`, `track_audio_asset`, `track_task`)이 모두 끝나고 0.3 초가 더 지나면 `initialize(engine)` 를 부르고, 이어서 `resize(engine, canvas_native_size)` 를 한 번 부릅니다. 맡긴 것이 없으면 `create` 뒤 0.3 초입니다.
4. `initialize` 전까지 엔진은 매 프레임 `draw_on_load(engine, graphic)` 만 부릅니다. (로딩 화면)
5. 그 뒤로 매 프레임 `tick(engine, time_delta)`, 씬의 터치 훅(`touch_press`, `touch_move`, `touch_release`, `touch_cancel`, `touch_wheel`), `pre_draw`, `draw`, `post_draw` 차례로 부릅니다. `SceneBase::set_gizmo_visible(true)` 면 그 뒤에 `draw_gizmos` 도 부릅니다.
6. 창 크기가 바뀌면 `resize(engine, canvas_native_size)` 를 부릅니다.
7. 내릴 때는 그 프레임이 끝날 때 `finalize(engine)`, 그다음 `unload(engine)` 를 부릅니다. `unload` 에서 `track_task` 로 맡긴 작업이 끝나면 목록에서 빠지고, 그 씬의 async 작업이 모두 끝난 뒤 `destroy(engine)` 를 부르고 씬 값을 버립니다.

기본 `tick` 은 `SceneBase::tick` 으로 노드 트리 갱신, 터치 넘기기(누른 자리 맨 위의 누를 수 있는 노드), 트윈 갱신을 차례로 합니다. 기본 `draw` 는 `SceneBase::draw` 로 노드 트리를 그립니다. 원본의 `super.tick()`, `super.draw()` 자리는 이 둘을 직접 부르는 것입니다.

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

	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
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

`SceneBase` 는 엔진 없이도 만들어 볼 수 있습니다. 불러오기 전에는 핸들이 없고, 맡긴 것이 없으면 다 끝난 것으로 칩니다.

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

### 애셋과 작업 기다리기 (원본 async load 자리)

원본은 `async load(engine)` 안에서 애셋을 await 합니다. 러스트 판은 `create` 에서 불러오기 작업을 `Engine::spawn_scene_task` 로 맡기고 `SceneBase::track_task` 로 씬에 맡깁니다. 작업은 맡기는 자리에서 첫 await 까지 돌고, 작업 안에서는 `SceneContext::with` 안에서만 씬과 엔진을 만집니다. 엔진은 맡긴 작업이 끝나야 `initialize` 를 부릅니다.

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

await 없이 애셋의 읽기 손잡이를 `track_asset` 으로 맡기는 꼴도 그대로 되고, 둘을 섞어도 됩니다. 데스크탑은 파일을 그 자리에서 읽고, 웹은 받기가 끝나는 프레임까지 `draw_on_load` 로 로딩 화면을 그립니다. 애셋 갈래는 "애셋" 장, async 작업은 "async/await" 장에서 봅니다.

## 한 프레임

엔진 쪽에서 본 한 프레임은 이렇습니다. 글꼴 올리기, `Graphic` 상태 되돌리기와 화면 비우기, `TimeManager` 갱신, `InputManager` 갱신(게임패드, 입력 동작), 깨어난 async 작업 폴링, 불러온 씬마다 위의 4 번(불러오는 중)이나 5 번, 정보창(켰을 때), 마우스 커서, 한 프레임짜리 입력 표시 지우기, 남은 묶음 그리기, 검증용 화면 저장(요청했을 때), 프레임 번호 올리기, 그리고 씬 목록 바꾸기(내릴 씬의 `finalize` 와 `unload`, 새 씬의 `create`).

씬에서 난 패닉은 엔진이 잡지 않습니다. (원본은 씬마다 예외를 잡아 찍고 루프를 이어 갑니다)

## 씬 바꾸기

씬 목록은 엔진이 듭니다. (원본 `SceneManager` 의 일을 엔진이 합니다)

| 함수 | 하는 일 |
| --- | --- |
| `load_scene(scene)` | 씬을 이번 프레임이 끝난 뒤 목록에 더하고 그 씬의 `SceneId` 를 돌려줍니다 (1 부터 매깁니다) |
| `unload_scene(scene)` | 씬 하나를 이번 프레임이 끝난 뒤 내립니다. `SceneId` 나 씬 공통 상태(`&SceneBase`)를 받습니다. 없는 핸들이면 아무것도 하지 않습니다 |
| `unload_all_scenes()` | 모든 씬을 이번 프레임이 끝난 뒤 내립니다. `load_scene` 과 같이 부르면 내린 다음 불러옵니다 |
| `load_scene_async(scene)` | `load_scene` 과 같고, `initialize` 가 끝나면 그 씬의 핸들로 풀리는 `Promise` 를 돌려줍니다 |
| `unload_scene_async(scene)`, `unload_all_scenes_async()` | 내리기가 끝나면 풀리는 `Promise` 를 돌려줍니다 |
| `get_all_loaded_scenes()` | 불러온(`create` 를 부른) 씬의 핸들 목록 (불러온 차례) |

씬은 여럿을 함께 올릴 수 있습니다. 올린 씬은 모두 매 프레임 갱신되고, 올린 차례로 그려집니다. (나중에 올린 씬이 위) 씬 스스로는 `self.scene_base.get_scene_id()` 로 자기 핸들을 알고, `engine.unload_scene(&self.scene_base)` 로 스스로 내립니다.

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

(창 없이 만든 엔진은 프레임을 돌지 않으므로 `create` 가 불리지 않고, 불러온 씬 목록도 비어 있습니다)

바꿀 때는 다음 씬을 먼저 올려 두고, 다 올라오면 지금 씬을 내리면 화면이 비지 않습니다. `load_scene_async` 가 돌려주는 `Promise` 를 엔진 작업에서 기다리면 원본의 `await engine.loadScene(...)` 과 같은 흐름입니다.

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
	elapsed_seconds: f64,
	is_leaving: bool,
}

impl Scene for SplashScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
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

- 같은 프레임에 불러온 씬(아직 `create` 전)을 내리면 `create` 도 `destroy` 도 부르지 않고 뺍니다. 그 씬의 `load_scene_async` 의 `Promise` 는 풀리지 않습니다.
- `create` 뒤 불러오는 도중에 내린 씬은 원본처럼 불러오기가 끝나면 `initialize` 까지 받은 뒤 버려집니다. (`load_scene_async` 의 `Promise` 도 그때 풀립니다)
- `unload_scene_async` 의 `Promise` 는 없는 핸들이어도 풀립니다.

## 시간 (TimeManager)

`tick(engine, time_delta)` 의 `time_delta` 는 지난 프레임부터 흐른 초에 시간 배율을 곱한 값입니다. 한 프레임 간격은 0 에서 상한(처음 0.25 초) 사이로 자릅니다. 창이 가려졌다 돌아오거나 오래 멈춘 뒤의 첫 프레임이 크게 뛰지 않게 하려는 것입니다. 씬에서는 `engine.get_time_manager()` 로 읽고 `engine.get_time_manager_mut()` 로 배율과 상한을 바꿉니다.

| 함수 | 내용 |
| --- | --- |
| `get_time_delta()` | 배율을 곱한 프레임 간격 (초, `f64`). 씬과 컴포넌트의 `tick` 이 받는 값 |
| `get_unscale_delta_time()` | 배율을 곱하기 전 프레임 간격 (초, 0 ~ 상한) |
| `get_time()` | 상한으로 자른 간격을 더한 누적 시간 (초, `f64`, 배율을 곱하지 않음) |
| `get_realtime_since_startup()` | 엔진 시작 뒤 경과 초 (마지막 갱신의 값, `f64`) |
| `get_frame_per_second()` | 지난 1 초 동안 센 프레임 수 (첫 1 초 전에는 0) |
| `set_time_scale(time_scale)`, `get_time_scale()` | 시간 배율 (처음 1). 0 이면 `tick` 은 계속 불리지만 `time_delta` 가 0 입니다. 음수도 받은 그대로 둡니다 |
| `set_maximum_time_delta(seconds)`, `get_maximum_time_delta()` | 한 프레임 간격의 상한 (처음 0.25 초, 0 보다 작거나 유한하지 않으면 0.25) |

`TimeManager::update` 는 엔진이 프레임마다 엔진 시작 뒤 경과 초를 넣어 부릅니다. 시간 관리자를 따로 만들어 직접 넣어 보면 이렇게 셉니다.

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

씬에서 쓰면 이렇습니다.

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

	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.scene_base.tick(engine, time_delta);
		let time_manager = engine.get_time_manager();
		let frame_per_second = time_manager.get_frame_per_second();
		let time = time_manager.get_time();
		engine.set_title(&format!("{} FPS, {:.1} s", frame_per_second, time));
	}
}
```

## 화면과 기준 해상도 (ViewManager, ViewScaleMode)

`ViewManager` 는 엔진이 하나 듭니다. 다루는 크기는 이렇습니다.

| 이름 | 뜻 |
| --- | --- |
| `get_canvas_native_size()` | 캔버스(데스크탑은 창 안쪽)의 논리 크기 (물리 픽셀을 배율로 나눠 반올림) |
| `get_device_pixel_ratio()` | 배율 (원본 `devicePixelRatio`, 데스크탑은 창의 배율) |
| `get_canvas_pixel_size()` | 그리기 버퍼의 물리 픽셀 크기 |
| `get_reference_resolution_size()` | 기준 해상도 |
| `get_view_native_rect()` | 캔버스 안에서 뷰가 차지하는 사각형 (논리 픽셀) |
| `get_view_size()` | 게임 코드가 쓰는 뷰 크기. 노드 배치와 입력 좌표가 이 단위입니다 |
| `get_target_resolution_scale()` | 뷰 한 단위가 논리 픽셀 몇 개인지 |

뷰를 어떻게 정할지는 `ViewScaleMode` 로 고릅니다. 엔진을 만들 때는 `EngineConfiguration::view_scale_mode`(처음 `StretchShort`)를 겁니다.

| 모드 | 뷰 크기 | 남는 곳 |
| --- | --- | --- |
| `None` | 캔버스 논리 크기 그대로 (늘이지 않음) | 없음 |
| `ReferenceResolution` | 기준 해상도, 늘이지 않고 가운데 | 양쪽 축이 잘리거나 남음 |
| `StretchWidth` | 기준 해상도, 가로를 캔버스에 맞춰 늘임 | 세로가 잘리거나 남음 |
| `StretchHeight` | 기준 해상도, 세로를 캔버스에 맞춰 늘임 | 가로가 잘리거나 남음 |
| `StretchShort` | 기준 해상도, 짧은 축을 맞춰 늘임 | 긴 축에 여백 |
| `StretchWidthExpandHeight` | 가로는 기준 해상도, 세로는 화면 비율대로 | 없음 |
| `StretchHeightExpandWidth` | 세로는 기준 해상도, 가로는 화면 비율대로 | 없음 |
| `StretchShortExpandLong` | 짧은 축은 기준 해상도, 긴 축은 화면을 채움 | 없음 |

`apply_aspect_view_scale_mode()` 는 화면이 기준 해상도보다 세로로 길면 `StretchWidthExpandHeight`, 아니면 `StretchHeightExpandWidth` 를 고르고 바로 뷰를 다시 셉니다.

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

`calculate_view_rect` 는 창 크기나 배율이 바뀔 때 플랫폼이 부릅니다. 위 예제처럼 직접 부르는 것은 시험할 때입니다.

엔진은 씬마다 `pre_draw` 앞(로딩 중이면 `draw_on_load` 앞)에 뷰 좌표(`apply_view_rect`)를 걸어 둡니다. 씬이 받는 터치 좌표와 같은 좌표라, 기기 픽셀 배율이 2 인 화면(레티나 같은)에서도 같은 크기와 자리로 그려집니다. (원본과 같다) 씬은 `pre_draw` 에서 좌표계를 바꿀 수 있고, 그리기 버퍼의 물리 픽셀로 그리려면 `graphic.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)` 을 부릅니다.

- `view_manager.apply_canvas_native_rect(graphic)`: 캔버스 전체를 논리 픽셀로. 바탕을 캔버스 끝까지 칠할 때 씁니다.
- `view_manager.apply_view_rect(graphic)`: 뷰 좌표로. (0, 0) 이 뷰의 왼쪽 위이고 오른쪽 아래가 `get_view_size()` 입니다. 씬이 받는 터치 좌표도 이 좌표입니다.
- `scene_base.draw_scene_background(engine, graphic)`: 앞의 둘을 이어 부릅니다. 캔버스 전체를 씬 배경색으로 칠하고 뷰 좌표를 겁니다. (원본 `GameScene.preDraw` 자리)

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

그 밖의 쓰임입니다.

- 실행 중에 모드나 기준 해상도를 바꾸려면 `set_view_scale_mode`, `apply_reference_resolution_size` 로 바꾼 뒤 `engine.resize()` 를 부릅니다. 두 함수는 다음 뷰 계산부터 반영되고, `Engine::resize` 는 플랫폼이 다음 프레임 첫머리에 캔버스와 뷰를 다시 세고 씬마다 `resize` 를 부르게 합니다.
- `canvas_position_to_view_position(canvas_position)`, `convert_client_to_view(client_x, client_y)`: 캔버스 좌표나 화면(client) 좌표를 뷰 좌표로 바꿉니다.
- `get_safe_area_insets()`: 노치와 홈 표시줄을 비킬 여백을 뷰 좌표의 `SafeAreaInsets`(`top`, `right`, `bottom`, `left`)로 줍니다. 데스크탑은 크롬 데스크탑처럼 0 이고, `set_safe_area_override` 나 환경 변수 `VANILLA_SAFE_AREA=위,오른쪽,아래,왼쪽` 으로 흉내 냅니다. (웹은 주소의 `?safeArea=`) 무시할 쪽은 `set_ignored_safe_area_sides` 로 정합니다.
- `set_max_render_pixel_ratio(ratio)`: 배율이 높은 기기에서 그리기 버퍼가 너무 커지지 않게 상한을 겁니다. 0 이면 제한하지 않습니다. 데스크탑은 줄인 크기로 그린 뒤 창 크기로 늘입니다.
- `get_css_pixel_scale()`: 뷰 한 단위가 화면에서 몇 CSS 픽셀인지 줍니다.

## 페이지 이벤트

엔진은 페이지가 가려지고 다시 보일 때, 창이 초점을 잃고 얻을 때, 창을 누를 때 등록한 리스너에 `PageEvent` 값을 그 자리에서 넘깁니다.

| 값 | 때 |
| --- | --- |
| `PageEvent::Hidden` | 페이지가 가려짐 (웹은 다른 탭과 앱 전환, 데스크탑은 창이 가려지거나 최소화됨) |
| `PageEvent::Visible` | 페이지가 다시 보임 |
| `PageEvent::Blurred` | 창이 초점을 잃음 |
| `PageEvent::Focused` | 창이 초점을 얻음 |
| `PageEvent::PointerDown` | 창을 누름 |

리스너는 `PageEventListener`(`Rc<RefCell<dyn FnMut(PageEvent)>>`)이고, 엔진은 약한 참조로 듭니다. 부른 쪽이 리스너를 살려 두어야 하고, 버리면 더 부르지 않습니다. 리스너는 엔진을 받지 않으므로 씬 상태는 `Rc` 로 나눠 쥡니다. `engine.is_page_hidden()` 은 마지막 이벤트가 `Hidden` 인지 알려 줍니다.

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

`dispatch_page_event` 는 보통 플랫폼이 부르고, 게임이 불러도 됩니다. 씬에는 덮어쓸 훅이 둘 있습니다. `on_page_visibility_restored` 는 페이지가 다시 보일 때, `on_page_process_restored` 는 멈췄던 페이지가 되살아날 때(웹의 `resume` 사건, 뒤로 가기 캐시에서 돌아온 `pageshow`) 불립니다. 이 둘을 부르기 전에 엔진은 오디오 컨텍스트를 깨우고 눌린 키와 터치 상태를 지웁니다.

```rust,no_run
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use vanilla::{Engine, PageEvent, PageEventListener, Scene, SceneBase};

struct PausingScene {
	scene_base: SceneBase,
	is_paused: Rc<Cell<bool>>,
	page_event_listener: Option<PageEventListener>,
	play_seconds: f64,
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

	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
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

플랫폼마다 다른 것이 있습니다.

- 웹: `visibilitychange` 가 `Hidden` 과 `Visible`, `blur` 가 `Blurred`, `focus` 가 `Focused`, `pointerdown` 이 `PointerDown` 입니다.
- 데스크탑: 창이 가려짐(맥과 X11 은 winit 이 알립니다), 최소화(윈도우는 이것으로 알립니다), 웨이랜드 프레임 콜백이 3 초 넘게 오지 않음 가운데 하나라도 맞으면 `Hidden`, 모두 풀리면 `Visible` 과 `on_page_visibility_restored` 입니다. 윈도우, 맥, 리눅스의 winit 은 앱 멈춤(suspended)을 보내지 않아 데스크탑에서 `on_page_process_restored` 는 불리지 않습니다.

## GameScene 과 UIScene

`Scene` 을 바로 구현하는 대신, 자주 쓰는 일을 미리 해 둔 바탕을 품을 수 있습니다. 러스트에는 상속이 없어서 바탕 구조체를 필드로 품고 트레이트를 구현한 뒤, 매크로 한 줄로 `Scene` 을 얻습니다.

| 바탕 | 트레이트와 매크로 | 하는 일 |
| --- | --- | --- |
| `GameSceneBase` | `GameScene`, `vanilla::game_scene!` | 로딩 화면(가운데 그림, 게이지, 누르면 건너뛰기, 최소 3 초), 터치를 `TouchRecognizer` 로 노드에 넘기기, 세이프 에어리어 노드, 크기가 바뀌면 `layout` 다시 부르기, `pre_draw` 에서 캔버스 배경 칠하고 뷰 좌표 걸기, 화면 페이드, UI 가 받지 않은 놀이 입력, 개발자 도구 |
| `UISceneBase` | `UIScene`, `vanilla::ui_scene!` | `GameSceneBase` 를 품고 제약 솔버(`LayoutSolver`)와 화면 가이드, 세이프 에어리어 가이드(`UINode`)를 더합니다 |

매크로가 만든 `Scene` 함수는 바탕을 먼저 부르고 트레이트의 같은 이름 함수를 부릅니다. (원본에서 `super` 를 먼저 부르는 덮어쓰기) `post_draw` 만 씬의 것을 먼저 부르고, 바탕(페이드 막, 개발자 도구)을 위에 그립니다.

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

`UIScene` 과 제약 기반 레이아웃은 "UI" 장에서 봅니다.

## 주의할 점

- 루트 노드는 `SceneBase::new()` 가 만듭니다. (원본은 `create()` 에서 만듭니다) 루트를 뷰 크기로 맞추는 것은 엔진이 `create` 바로 뒤에 합니다.
- 맡긴 애셋과 작업이 끝나도 0.3 초 뒤에야 `initialize` 와 `tick` 이 시작됩니다. 데스크탑은 애셋을 그 자리에서 읽으므로 맡긴 작업이 없으면 `create` 뒤 0.3 초입니다.
- 웹은 애셋이 나중에 오므로, 애셋을 쓰는 노드는 `initialize` 에서 만듭니다.
- `tick` 이나 `draw` 를 덮어쓰면 기본 구현이 하던 일(노드 트리 갱신과 그리기, 터치 넘기기, 트윈)이 빠집니다. 필요하면 `self.scene_base.tick(engine, time_delta)`, `self.scene_base.draw(graphic)` 를 직접 부릅니다.
- `GameSceneBase` 와 `UISceneBase` 의 뷰 스케일 모드는 처음 `ViewScaleMode::None` 이고, `initialize` 에서 뷰 매니저에 겁니다. `EngineConfiguration::view_scale_mode` 를 정해 두어도 이 씬들은 자기 모드로 바꾸므로, 기준 해상도로 그리려면 `set_view_scale_mode` 로 정합니다.
- `ViewScaleMode::None` 이 아닌 모드는 기준 해상도로 나눕니다. 기준 해상도를 0 으로 두면 뷰를 셀 수 없습니다.
- 씬을 내리지 않고 다음 씬을 올리면 두 씬이 함께 돌고 함께 그려집니다.
- 리스너를 `add_page_event_listener` 에 넘긴 뒤 버리면 더 불리지 않습니다. 씬의 필드처럼 살아 있는 곳에 둡니다.
- 씬에서 난 패닉은 엔진이 잡지 않습니다.

## 관련 타입

Engine, EngineConfiguration, Scene, SceneBase, SceneId, SceneContext, EngineContext, Promise, TimeManager, ViewManager, ViewScaleMode, SafeAreaInsets, SafeAreaSide, PageEvent, PageEventListener, GameScene, GameSceneBase, UIScene, UISceneBase, Graphic
