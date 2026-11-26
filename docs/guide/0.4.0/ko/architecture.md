# 구조

엔진이 무엇으로 이루어졌고, 무엇이 무엇을 들고 있고, 한 프레임이 어떤 차례로 도는지 봅니다. 저장소의 `ARCHITECTURE.md` 를 엔진을 쓰는 사람의 눈높이로 줄인 것이고, 원본 vanilla.js 와 달라진 것을 중심으로 적습니다.

## 모듈 배치

| 모듈 | 들어 있는 것 |
| --- | --- |
| `base` | 엔진을 모르는 기반 타입: `Vector2`, `Vector3`, `Matrix4`, `Quaternion`, `Rect`, `Color`, `Colors`, `Pivot`, `SeededRandom`, `FiniteStateMachine`, `ObjectPool`, `Cooldown`, `RepeatTimer`, `Version`, `format`, `math`, `wait` |
| `core` | 엔진 본체: `Engine`, `Scene`, `SceneBase`, `Graphic`, `Component`, `TimeManager`, `ViewManager`, `InputManager`, `AudioManager`, 애셋 트레이트(`Asset`), `Tween`, `Animation`, async 실행기(`EngineContext`, `SceneContext`, `TaskHandle`, `Promise`), `Canvas2D`, 반사 값과 트레이트 |
| `core/node` | `NodeTree`(노드 저장소), `WorldNode` |
| `core/component` | `Paint`, `Sprite`, `Text`, `RichText`, `Mask`, `Outline`, `SoftShade`, `ShadowText`, `ShadowImage` |
| `resource` | `ImageAsset`, `TextAsset`, `JsonAsset`, `BlobAsset`, `BunchAsset`, `FontAsset`, `AudioAsset`, `VisualAsset`, `AssetLoader`, `SpriteAtlas` |
| `effect` | `ParticleSystem`, `ShaderSprite`, `ScreenEffect`, `TrailRenderer`, `HeatHazeEffect` |
| `misc` | `Camera2D`, `PathFinder`, `Collision2D`, `LocalStorage`, `Localization`, `DialogueRunner`, `DevTools` 같은 게임 보조 기능 |
| `animation` | `Timeline`, `Animator`, `Action` |
| `graphics3d` | `SkinnedModel`, `SkinnedModelRenderer`, `HumanSkinRenderer`, `Material`, `ShadowMap`, `RenderTarget`, 블룸, AO, SSS, `Camera3D`, 툰 3D(`graphics3d/toon`) |
| `ui` | UI 위젯, 창 쌓기, `UIDocument`, `UINode`, `UIScene`, 오토레이아웃(`LayoutSolver` 등) |
| `game` | `GameScene`, `GameSceneBase` (로딩 화면, 터치 라우팅, 세이프 에어리어, 페이드, 개발자 도구를 갖춘 게임용 씬) |
| `web` | `WebSocketClient`, `Pane` |
| `platform` | 창, GL 컨텍스트, 입력 이벤트, 프레임 루프, 애셋 바이트 읽기, 소리. 데스크탑(winit, glutin)과 웹(캔버스, WebGL2) |

- 공개 이름은 `src/lib.rs` 가 크레이트 맨 위로 다시 내보냅니다. 그래서 `use vanilla::{Engine, Scene, SceneBase};` 처럼 씁니다. 모듈 경로(`vanilla::core::scene::SceneAny` 같은)로도 닿습니다.
- 레이어는 원본처럼 아래에서 위로만 의존합니다. (`base` ← `core` ← `resource`, `effect`, `misc` 같은 위층) `platform` 은 `core` 밑에서 창, GL, 입력을 대 주고, 엔진 본체는 플랫폼을 모릅니다.
- 반사 매크로는 `macros/` 의 따로 된 크레이트(`vanilla-engine-macros`)이고, 엔진이 `vanilla::reflect` 로 다시 내보냅니다. 게임은 엔진 하나만 의존합니다.

## 설계 원칙 (원본과 다른 것)

1. **노드는 핸들로 가리킵니다.** 원본은 부모가 자식 객체를 배열로 들고 자식이 부모를 거꾸로 가리킵니다. 러스트 판은 씬의 `NodeTree` 가 모든 노드를 저장소 하나(slotmap)에 담고, 부모와 자식을 `NodeId` 로 잇습니다. `NodeId` 는 칸 번호와 세대 번호라서, 지운 노드의 칸을 새 노드가 다시 써도 예전 핸들은 `None` 을 돌려줍니다. `Rc<RefCell>` 은 쓰지 않습니다.
2. **전역 엔진이 없습니다.** 원본의 `Engine.getEngine()` 대신 씬 훅과 컴포넌트 문맥(`ComponentContext`)이 엔진을 인자로 받습니다. async 작업은 컨텍스트의 `with` 안에서만 엔진을 빌립니다.
3. **상속 대신 트레이트와 공통 상태 구조체.** `Scene` + `SceneBase`, `Component` + `ComponentBase`, `Asset` + `AssetBase` 꼴입니다. 원본의 노드 상속 넷(Node, ComponentNode, TransformNode, WorldNode)은 `WorldNode` 하나로 합쳤고, 원본에서 노드였던 위젯(입력 필드, 스크롤 바, 창 같은 것)과 `UINode` 는 러스트 판에서 컴포넌트입니다.
4. **플랫폼 층을 뗐습니다.** 창, GL, 입력, 루프, 애셋 바이트, 소리는 `platform` 아래에만 있습니다.
5. **성능 기능은 엔진 기본입니다.** 묶어 그리기와 화면 밖 거르기를 처음부터 켭니다.
6. **ECS 로 가지 않습니다.** 원본의 씬, 노드, 컴포넌트 꼴을 그대로 두어 vanilla.js 게임을 옮기기 쉽게 합니다.
7. **2D 와 3D 는 GL 컨텍스트 하나를 같이 씁니다.** 씬의 `pre_draw` 에서 그림자, 3D 장면, 블룸 패스를 그리고 `Graphic::restore_render_state` 로 2D 상태를 되살린 뒤 UI 를 그립니다. 2D 그리기 사이에 끼우는 3D 는 `Graphic::run_external_pass` 로 감쌉니다.

핸들이 어떻게 도는지는 창 없이도 볼 수 있습니다.

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

## 소유 구조

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

- 플랫폼이 엔진 실행자(`EngineRunner`)를 들고 프레임마다 부릅니다. `Engine::run` 이 이 실행자를 만들어 플랫폼에 넘깁니다. 게임 코드가 실행자를 직접 다룰 일은 없습니다.
- `Graphic` 은 `Engine` 이 듭니다. 플랫폼이 GL 컨텍스트를 만든 뒤 넘겨주고, 원본 `getGraphic()` 처럼 `engine.get_graphic()`, `engine.get_graphic_mut()` 로 씬의 `create`, `initialize`, `tick` 이나 컴포넌트의 `tick`(`context.get_engine()`)에서 꺼냅니다. 씬의 그리기 훅(`draw_on_load`, `pre_draw`, `draw`, `post_draw`, `draw_gizmos`)은 `&mut Graphic` 을 인자로 받습니다. 엔진이 그래픽을 잠깐 꺼내 넘기는 것이라 그 안에서는 `engine.get_graphic()` 이 `None` 이고 인자의 `graphic` 을 씁니다. (`&mut Engine` 과 `&mut Graphic` 을 함께 넘기는 러스트 빌림 꼴, 그리기 밖에서 둘을 함께 쓰려면 `engine.with_graphic(|engine, graphic| ...)`)
- 씬은 엔진이 `Box<dyn Scene>` 으로 소유합니다. 다른 씬을 가리킬 때는 씬 값 대신 `Engine::load_scene` 이 돌려준 `SceneId` 를 씁니다. 원본의 `SceneManager` 는 엔진에 합쳤습니다.
- 내리는 중인 씬(unload 에서 맡긴 작업을 기다리는 씬)은 원본처럼 목록에 남아 `tick` 과 `draw` 를 받습니다. 목록에서 뺀 뒤에도 그 씬의 async 작업이 남았으면 작업이 끝날 때까지 씬 값을 살려 두고, 다 끝나면 `destroy` 를 부르고 버립니다.
- 애셋은 원본처럼 중앙 매니저 없이 씬이 멤버로 듭니다. 같은 그림을 여러 번 그려도 GPU 텍스처는 `Graphic` 의 캐시가 하나만 만듭니다. 여럿을 함께 받을 때는 `AssetLoader` 가 동시 수만큼 요청하고 진행률을 셉니다.
- 소리 장치는 `AudioManager` 가 처음 쓸 때 엽니다.

## 노드와 컴포넌트

- 노드 만들기: `node_tree.create_child(parent_id, "name")` 이 `NodeId` 를 돌려주고, `node_tree[node_id]` 로 꺼내 설정합니다. 붙이지 않은 노드는 `create_node` 로 만들고 `add_child` 로 붙입니다.
- 지우기: `destroy_node(node_id)` 는 자손까지 지웁니다. `tick` 이나 터치를 도는 도중에 부르면 비활성으로 두었다가 다 돈 뒤에 지웁니다.
- 컴포넌트 붙이기: 보통은 `node_tree[node_id].add_component(...)` 입니다. 붙을 때 자식 노드를 만드는 컴포넌트(UI 위젯 같은 것)는 `node_tree.add_component(node_id, ...)` 로 붙입니다.
- 컴포넌트는 `tick`, 터치, 휠에서 `ComponentContext` 를 받아 자기 노드와 트랜스폼, 트리 전체, 엔진(입력, 시간, 뷰, 소리)에 닿습니다. 부르는 컴포넌트 하나만 노드에서 잠깐 꺼내 두므로, `&mut self` 를 쥔 채로 같은 노드의 다른 컴포넌트와 트리를 빌릴 수 있습니다.
- 씬 코드에서 컴포넌트와 트리를 함께 바꿀 때는 `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)` 를 씁니다. 자주 쓰는 일은 트리를 받는 `*_in_tree` 함수로 따로 있습니다.
- 그리기만 하는 컴포넌트(`Paint`, `Sprite`, `Text`, `Mask`)만 붙은 노드는 `tick` 에서 건너뜁니다. 그리는 사각형을 알려 주는 컴포넌트만 붙은 노드는 화면 밖이면 그리지 않습니다.

## 프레임 루프

플랫폼이 창이나 캔버스와 GL 을 만들고 첫 씬을 올린 뒤, 매 프레임 이 차례로 돕니다.

1. 바이트가 온 글꼴을 글자 캐시에 올립니다.
2. `Graphic` 의 상태를 되돌리고 화면을 비웁니다. (뷰포트, 투영, 변환은 단위 행렬)
3. 시간(`TimeManager`)과 입력(`InputManager`, 게임패드와 입력 동작 포함)을 갱신합니다.
4. 깨어난 async 작업을 폴링하고, 내리기가 끝난 씬을 정리합니다.
5. 씬마다: 불러오는 중이면 `draw_on_load` 만 부르고, 아니면 `tick`, 씬의 터치 훅, `pre_draw`, `draw`, `post_draw`, (기즈모를 켰으면) `draw_gizmos` 차례로 부릅니다.
6. 정보창(`use_statistics` 를 켰을 때)과 마우스 커서를 처리하고, 한 프레임짜리 입력 표시를 지웁니다.
7. 남은 묶음을 그리고, 검증용 화면 저장을 하고, 프레임 번호를 하나 올립니다. (첫 프레임이 1)
8. 씬 목록을 바꿉니다. 내릴 씬은 `finalize`, `unload` 를 부르고, 새로 올린 씬은 `create` 를 부릅니다.

그 뒤에 플랫폼이 화면을 넘깁니다. (데스크탑은 버퍼 바꾸기, 웹은 콜백이 끝나면 브라우저가) 데스크탑에서 엔진의 질문 창(`Engine::alert` 같은 것)이 떠 있는 동안은 원본에서 자바스크립트가 멈추듯 씬의 `tick` 과 async 작업을 건너뜁니다.

씬 불러오기는 원본의 create, 비동기 load, initialize 를 `create`, (맡긴 애셋과 작업을 기다림), `initialize` 로 옮겼습니다. 원본 `async load` 의 몸은 `create` 에서 `Engine::spawn_scene_task` 로 맡긴 작업이고, `SceneBase::track_task` 로 씬에 맡기면 엔진이 그 작업이 끝나기를 기다립니다. 맡긴 것이 다 끝난 뒤에도 원본처럼 0.3 초 더 로딩 화면을 보이고 `initialize` 를 부릅니다. 자세한 차례는 "엔진과 씬 생명 주기" 장에서 봅니다.

## 그리기

- 모든 2D 그리기는 `Graphic::draw_vertices` 한 곳으로 모입니다. 묶을 수 있으면 정점을 CPU 에서 변환해 모았다가 같은 텍스처가 이어지는 동안 드로우 콜 한 번으로 그립니다. 정점마다 색과 틴트를 실어, 색이 다른 그리기도 같은 묶음에 듭니다.
- 블렌드가 바뀔 때, 클립을 걸거나 풀 때, 파티클을 그릴 때, 프레임 끝에 모은 것을 먼저 내보냅니다. GL 을 직접 쓰는 코드는 그 전에 `Graphic::flush_batch` 를 부르거나 `Graphic::run_external_pass` 로 감쌉니다.
- 블렌드 모드는 원본처럼 26 가지를 모두 그립니다. 13 가지는 고정 블렌딩이고, 나머지 13 가지(Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity)는 그릴 영역 아래를 떠 와 W3C Compositing and Blending 공식대로 셰이더로 섞습니다. (이 모드에서는 묶지 않습니다)
- 클립은 원본처럼 스텐실 기반입니다.
- 화면 효과(`ScreenEffect`)는 씬이 `pre_draw` 첫머리에서 시작하고 `post_draw` 끝에서 마칩니다. 마칠 때 화면을 텍스처로 떠 와 풀스크린 패스 사슬을 겁니다.
- 묶어 그리기와 거르기는 `Graphic::set_batching_enabled`, `Graphic::set_culling_enabled` 로 끕니다.

## 애셋

- `ImageAsset`, `TextAsset`, `JsonAsset`, `BlobAsset`, `FontAsset` 은 `load(engine, 경로)` 로 요청하고 `SceneBase::track_asset` 으로 씬에 맡깁니다. `load` 는 await 할 수 있는 손잡이를 돌려줍니다.
- 데스크탑은 `EngineConfiguration::asset_directory_paths` 의 폴더 후보에서 그 자리에서 읽고, 웹은 `asset_base_url` 과 경로를 이어 fetch 합니다. 게임 코드는 같습니다.
- 실패한 애셋도 끝난 것으로 쳐서 씬이 멈추지 않습니다.
- 그림, 소리, 글꼴을 웹은 원본처럼 브라우저가 풀고, 데스크탑은 엔진이 같은 결과가 나오게 풉니다.
- 기본 글꼴은 엔진에 들어 있지 않습니다. 두 플랫폼 모두 시스템 글꼴이 기본이고, 게임 글꼴은 `default_font_url`, `default_font_bytes`, `FontAsset` 가운데 하나로 줍니다.

## async/await

- 엔진은 외부 런타임 없이 스레드 하나짜리 실행기를 품습니다. 퓨처는 `Send` 가 아니어도 되고, 데스크탑과 웹이 같은 코드로 돕니다.
- `Engine::spawn_task`, `Engine::spawn_scene_task` 는 원본 async 함수를 부를 때처럼 맡기는 자리에서 첫 await 까지 돌고, 남은 것은 프레임마다 씬의 `tick` 앞에서 깨어난 차례대로 돕니다.
- 퓨처는 `&mut` 를 await 너머로 쥘 수 없으므로, 작업은 컨텍스트의 `with(|scene, engine| ...)` 클로저 안에서만 씬과 엔진을 빌립니다.
- 원본에서 Promise 를 돌려주던 load 들은 애셋이나 씬을 빌리지 않는 손잡이를 돌려줍니다. 그래서 원본 `await this.atlas.load(path)` 가 `scene_context.with(|scene, engine| scene.atlas.load(engine, path)).await` 한 줄입니다.
- 자세한 것은 "async/await" 장에서 봅니다.

## 원본과의 대응

원본 `src/` 의 .js 200 파일 가운데 182 파일을 옮겼습니다. (같은 이름 176, 다른 파일에 합친 것 6) 나머지 18 파일은 빈 파일, 죽은 코드, 러스트 언어와 표준이 맡는 것(객체 모델 도우미, 컬렉션), 다른 기능이 덮는 것(`Camera2D` 가 덮는 deprecated/camera.js)이라 일부러 옮기지 않았습니다.

러스트 판에만 있는 것은 원본에서 브라우저가 하던 일입니다. 플랫폼 층, 글자 모양 짓기와 색 이모지, 그림 풀기, 글 입력 줄, 노드 저장소, async 실행기, 브라우저 자리(`Canvas2D`, 질문 창, 브라우저가 그리는 글의 번역)가 그렇습니다.

## 두 엔진의 차이

두 엔진은 기능 차이를 두지 않습니다. 남은 차이는 언어나 플랫폼 때문에 정말 맞출 수 없는 것과, 러스트에 맞게 일부러 정한 꼴뿐입니다.

**정말 맞출 수 없는 것**

- 반사(UI 문서의 반사 저장과 읽기, 개발자 도구 인스펙터의 게터 목록)는 `#[vanilla::reflect]` 를 붙인 타입만 됩니다. 엔진 컴포넌트는 모두 붙였고, 게임 컴포넌트는 `impl` 블록과 `impl Component` 블록에 붙입니다. 러스트에는 실행 중에 메서드를 훑는 길이 없어서입니다.
- 데스크탑 개발자 도구의 메모리 칸은 자바스크립트 힙 대신 시스템 할당자의 힙입니다.
- 데스크탑의 글자 래스터 픽셀과 도형 가장자리 안티에일리어싱이 크롬과 다릅니다. (글자 너비와 도형 안쪽 색은 같습니다)
- 데스크탑 Opus 와 AAC 소리의 표본 끝자리가 크롬과 아주 조금 다릅니다. (길이와 다듬기는 같습니다)

**일부러 정한 꼴**

| 차이 | 까닭 |
| --- | --- |
| 전역 엔진이 없습니다. 씬 함수와 `ComponentContext` 로 넘깁니다 | 엔진을 쓰는 곳이 인자로 드러나게 했습니다 |
| 노드는 `NodeTree` 안에 있고 `NodeId` 로 가리킵니다 | 부모와 자식을 `Rc<RefCell>` 로 서로 가리키면 순환 참조와 실행 중 빌림 검사가 생깁니다 |
| 노드 종류는 `WorldNode` 하나입니다 | 러스트에 클래스 상속이 없어 노드 꼴을 하나로 두고 기능을 컴포넌트로 붙였습니다 |
| 상속 대신 트레이트와 품기입니다 (`Scene` + `SceneBase`, `Component` + `ComponentBase`) | 공통 상태는 구조체로, 덮어쓰기는 트레이트 함수로 둡니다 |
| 수는 `f32` 입니다 (JS 수의 글을 그대로 내야 하는 곳은 `f64`) | GL 에 올리는 값과 같은 정밀도로 듭니다 |
| GL 을 쓰는 객체는 컨텍스트를 들지 않고 함수마다 gl 을 받습니다 | 컨텍스트는 `Graphic` 이 들고, 나눠 쥐면 수명과 빌림이 엮입니다 |
| 트리를 받는 `*_in_tree` 함수를 따로 둔 곳이 있습니다 | 씬 코드가 컴포넌트를 쥔 동안 같은 트리의 다른 노드를 함께 빌릴 수 없습니다 |
| 선택 인자 객체 대신 기본값을 가진 옵션 구조체입니다 (`RenderTargetOptions`, `UIConfirmWindowOptions` 같은 것) | 러스트에는 이름 붙은 선택 인자가 없습니다 |
| 콜백은 `Box<dyn FnMut>` 로 받고, 콜백 게터는 `Rc<RefCell>` 로 돌려줍니다 | 엔진과 부르는 쪽이 같은 함수를 나눠 쥡니다 |
| 예외 대신 `Result` 와 `Option` 입니다 | 러스트는 예외를 던지지 않습니다 |
| 이름은 러스트 규칙(snake_case, UpperCamelCase, SCREAMING_SNAKE_CASE)입니다 | Rust API Guidelines C-CASE 를 따릅니다 |
| async 작업은 컨텍스트의 `with` 안에서만 씬과 엔진을 만집니다 | 러스트 퓨처는 `&mut` 를 await 너머로 쥘 수 없습니다 |

옮겨 오는 쪽에서 본 이름 바꾸기와 소유 규칙은 "원본 vanilla.js 에서 옮겨 오기" 장에서 봅니다.

## 관련 타입

Engine, EngineConfiguration, Scene, SceneBase, SceneId, NodeTree, NodeId, WorldNode, Component, ComponentBase, ComponentContext, Graphic, EngineContext, SceneContext, TaskHandle, Promise
