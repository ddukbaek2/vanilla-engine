# 바닐라엔진 구조 설계 문서

vanilla.js 를 러스트로 옮긴 엔진의 구성과 소유 구조를 정리한 문서다. "무엇이 무엇을 만들고, 들고 있고, 어떤 순서로 굴러가는가"를 본다. 원본 구조는 vanilla.js 의 ARCHITECTURE.md 에 있고, 여기서는 러스트 판에서 달라진 것을 중심으로 적는다.

- 기준: 2026-11-26 (dev)
- 이름: 바닐라엔진(Vanilla Engine). 저장소와 크레이트 이름은 `vanilla-engine`, 코드에서는 `vanilla` (`use vanilla::...`)

## 1. 전체 배치

```
Cargo.toml           워크스페이스 (엔진 크레이트와 macros/)
macros/              반사 절차 매크로 #[reflect] (vanilla-engine-macros, 엔진이 vanilla::reflect 로 다시 내보낸다)
src/lib.rs           공개 API 를 다시 내보낸다 (원본 import.js 자리)
src/animation/       Timeline, Animator, Action
src/base/            엔진을 모르는 기반 타입 (수학, 색, 식별자, 플랫폼 정보, Format, FiniteStateMachine, ObjectPool, SeededRandom, Timer, Version, wait(Wait.seconds, nextFrame))
src/core/            엔진 본체 (Engine, 매니저, Scene, 노드 트리, Graphic, Component, reflection(반사 값과 트레이트), 애셋 꼴, 트윈, Animation, Frame)
                     async: task(스레드 하나짜리 실행기, EngineContext, SceneContext, TaskHandle, Promise)
                     브라우저 자리: canvas2d(Canvas2D, Path2D, 그라데이션과 패턴, 데스크탑은 tiny-skia), modaldialog(alert, confirm, prompt 질문 창),
                     browseruitext(브라우저가 그리는 글의 크로미엄 번역 82 로케일)
                     입력: InputManager, InputActions, GamepadManager, PointerCursor, textinput(글 입력 줄)
                     소리: AudioManager, AudioPlayer, AudioBus, SoundSystem, SoundEffectPool
                     그리기: Graphic, ShaderProgram, ShaderWarmer, FullscreenPass, ImageTextureCache, TextStringTextureCache, sharedbakecache
                     글자와 그림: textshaping(양방향 정렬, 모양 짓기, 글꼴 얼굴), colorglyph(색 이모지), image(그림 풀기), avifimage(데스크탑 AVIF)
src/core/node/       NodeTree(노드 저장소), WorldNode
src/core/component/  Paint, Sprite, Text, RichText, Mask, Outline, SoftShade, ShadowText, ShadowImage
src/resource/        애셋 (ImageAsset, TextAsset과 JsonAsset, BlobAsset, BunchAsset, FontAsset, AudioAsset, VisualAsset, AnimationClip), AssetLoader, SpriteAtlas
src/effect/          ParticleSystem, ShaderSprite, ScreenEffect, TrailRenderer, HeatHazeEffect
src/misc/            BeepPlayer, Camera2D, CameraGestures, Shaker, Grid, PathFinder, Collision2D, Steering2D, NodeLayout, LocalStorage, PersistedStore,
                     FloatingText, VirtualPad, PlatformerBody, SpriteAnimator, Localization, LanguagePack, UIFont, DynamicFont, Typewriter, Dialogue,
                     FocusNavigator, PointerGesture, ImageScroller, TouchEffect, Indicator, Visual, FrameProbe, FixedStepper, LoadingProgress,
                     GraphicsQualityGovernor, BuildInfo, DevTools(F2 하이어라키/인스펙터)
src/graphics3d/      Material, SkinnedModel(glTF/GLB), FbxLoader, SkinnedModelRenderer(스키닝 PBR), HumanSkinRenderer, ShadowMap, RenderTarget(멀티샘플),
                     BloomEffect, MultiLevelBloomEffect, AmbientOcclusionEffect, SubsurfaceScatteringEffect, Camera3D (FullscreenPass 는 core)
src/graphics3d/toon/ 툰 3D: MeshBuilder, glbmeshreader, ToonRenderer, ToonLightSet, ToonMaterial, ToonDrawQueue, ToonScreenOutline, ToonVfx, ToonPostProcess, ToonView3D
src/ui/              UIView, UIControl, UILabel, UIButton, UIToggleButton, UISlider, UIProgressView, UIProgressRing, UIImageView, UIScrollView, UIScrollBar,
                     TouchRecognizer, UIInputField(데스크탑 winit IME, 웹 숨긴 DOM input, 클립보드 arboard, 데스크탑 크롬 편집 칸 메뉴 uiinputfieldmenu), UIListView, UISnapScrollView, UIDropdown,
                     UIDialog, UIToast, UISpinner, UIContextMenu, UIChart, UIDraggable, UIHitArea, PopupMotion, ButtonMotion, ToastMotion, UIScale,
                     UIWindow, UIWindowLayer, UIConfirmWindow, UIDocument, UINode, UIScene(UISceneBase), autolayout(LayoutSolver 등)
src/game/            GameSceneBase, GameScene 트레이트와 game_scene! (원본 GameScene: 로딩 화면, 터치 라우팅, 세이프 에어리어, 크기 바뀌면 다시 놓기, 페이드, 놀이 입력, 개발자 도구)
src/experimental/    0.4.0 뒤 기능의 초안 (원본 import.js 의 Experimental 이름 공간 자리, API 가 바뀔 수 있다)
src/web/             WebSocketClient (ewebsock, 데스크탑과 웹), Pane (화면 분할 패널, 웹은 DOM 요소 pane/dompane, 데스크탑은 엔진 노드 pane/nodepane)
src/platform/        창, GL 컨텍스트, 입력 이벤트, 프레임 루프, 화면 넘기기, 애셋 바이트 읽기, 오디오 백엔드
  desktop.rs         winit 0.30 + glutin 0.32 (OpenGL 3.3 코어)
  document.rs        문서(html lang, head 의 meta): 웹은 브라우저 문서, 데스크탑은 엔진이 든 값 (Document, EngineConfiguration.document_language, document_meta_list)
  desktop/           renderscaler(렌더 배율 상한), preserveddrawingbuffer(드로잉 버퍼 남기기), textinput(IME, 클립보드), systemfont(운영체제 글꼴과 대체 글꼴), emojipanel(운영체제 이모티콘 창),
                     coretextglyph(맥 CoreText 글리프), gamepadhaptics(맥 패드 진동), windowsplatform(윈도우 잠자기 깸, 손가락 판정),
                     windowsinputpane(윈도우 터치 키보드), linuxplatform(리눅스 손가락 판정), waylandframecallback(웨이랜드 가려짐과 수직 동기)
  web.rs             캔버스 + WebGL2 + request_animation_frame + fetch
  web/               systemfont(브라우저 Canvas2D 로 글자 굽기와 재기), textinput(숨긴 DOM input, 클립보드)
  framelimiter.rs    웹 최대 FPS 의 프레임 세기
  audio/             데스크탑 cpal 위의 엔진 믹서(mixer.rs, 크롬 WebAudio 와 같은 노드 그래프와 셈), 웹 WebAudio
    decode/          데스크탑 소리 해석 (크롬 decodeAudioData 와 같게: symphonia 0.6, Opus 는 opus-pure, 앞뒤 다듬기, SincResampler, 내리기)
third_party/         고쳐 쓰는 남의 크레이트 (워크스페이스 밖, 엔진이 경로로 의존한다)
  rustybuzz/         rustybuzz 0.20.1 에 크롬 HarfBuzz 처럼 자리 배율(Face::set_scale, 16.16)과 축 값 정규화를 더한 판
  ttf-parser/        ttf-parser 0.25.1 에서 짝 커닝(PairPos 꼴 1)의 장치 표 기준을 HarfBuzz 처럼 바로잡은 판 (fontdue 는 crates.io 판을 쓴다)
tests/               데스크탑 플랫폼 시험 desktopplatform.rs (#[ignore], 시스템 글꼴과 대체 글꼴, 색 이모지, 클립보드, 같은 기계의 크롬 measureText 견주기)
  assets/            크롬 기대값 자료와 그것을 찍는 스크립트 (소리 해석, 믹서, 비프, PannerNode, 글 재기)
.github/workflows/   CI platforms.yml: 윈도우, 리눅스(X11 과 웨이랜드), 인텔 맥에서 시험, 예제 찍기(메사 llvmpipe 소프트웨어 GL), 같은 기계의 크롬과 견주기, wasm32 빌드
.github/scripts/     CI 도구 (captureexamples.cjs 예제 찍기, chromecompare.cjs 원본 예제를 헤드리스 크롬으로 찍어 화소 견주기)
```

레이어 방향은 원본처럼 아래에서 위로만 의존한다: base ← core ← (resource, effect, misc). platform 은 core 밑에서 창과 GL 과 입력을 대 준다. 엔진 본체(core::engine::EngineRunner)는 플랫폼을 모른다.

## 2. 설계 원칙 (원본과 다른 것)

1. **노드는 핸들로 가리킨다.** 원본은 부모가 children[] 으로 자식 객체를 들고 자식이 parent 로 거꾸로 가리킨다. 러스트 판은 씬의 NodeTree 가 노드를 slotmap 저장소 하나에 담고, 부모와 자식을 NodeId 로 잇는다. NodeId 는 칸 번호와 세대 번호라서 지운 노드의 핸들은 get_node 가 None 을 돌려준다(원본 isDestroyed 의 공백을 메움). Rc<RefCell> 은 쓰지 않는다.
2. **전역 엔진이 없다.** 원본의 Engine.getEngine() 대신 씬 함수와 컴포넌트 문맥으로 Engine 을 넘긴다. async 작업도 폴링하는 동안 컨텍스트의 with 로 받는다.
3. **상속 대신 트레이트와 공통 상태 구조체.** Scene 트레이트 + SceneBase, Component 트레이트 + ComponentBase, Asset 트레이트 + AssetBase. 원본의 Node, ComponentNode, TransformNode, WorldNode 상속 넷은 WorldNode 하나로 합쳤다.
4. **플랫폼 층을 뗐다.** 창, GL, 입력, 루프, 애셋 바이트, 오디오는 platform 아래에만 있다. 안드로이드, iOS 는 여기에 모듈을 더하면 된다.
5. **성능 기능은 엔진 기본.** 묶어 그리기와 화면 밖 거르기를 기본으로 켠다. (원본도 같이 기본 켬으로 바꿨다)
6. **ECS 로 가지 않는다.** 원본의 씬, 노드, 컴포넌트 꼴을 그대로 둬서 vanilla.js 게임을 옮기기 쉽게 한다.
7. **2D 와 3D 는 GL 컨텍스트 하나를 같이 쓴다.** (원본 원칙 그대로) 원본 3D 씬은 자체 rAF 를 돌지만 러스트 판은 씬의 pre_draw 에서 그림자, 씬, 블룸 패스를 그리고 `graphic.restore_render_state()` 뒤에 2D 를 그린다. 2D 그리기 사이에 끼우는 3D 는 `Graphic.run_external_pass` 로 감싼다. (노드 사각형 안의 툰 3D 는 ToonView3D 가 이 길로 그린다, 원본과 같다) 데스크탑 GL 설정은 깊이 24, 스텐실 8 이다. (examples/modelviewer.rs)

## 3. 소유 구조

```
EngineRunner (플랫폼이 들고 프레임마다 부른다)
├── Engine
│   ├── EngineConfiguration
│   ├── TimeManager, ViewManager
│   ├── InputManager
│   │   ├── GamepadManager (데스크탑 gilrs, 웹 navigator.getGamepads(), 첫 tick 에 연다)
│   │   ├── InputActions (입력 동작, tick 에서 갱신)
│   │   └── TextInput (글 입력 줄, 입력 필드가 꺼내 쓴다)
│   ├── PointerCursor (켜져 있으면 프레임 끝에 커서를 플랫폼에 넘긴다)
│   ├── AudioManager (소리 장치는 처음 쓸 때 연다)
│   ├── Platform (플랫폼, 브라우저 정보)
│   ├── TaskExecutor (async 작업, 씬 작업은 씬 핸들로 그 씬을 가리킨다)
│   ├── platform_window (창 손잡이: 데스크탑 winit Window, 웹 캔버스)
│   ├── Graphic (플랫폼이 GL 컨텍스트를 만든 뒤 attach_graphic 으로 넘김. 원본 getGraphic 처럼 get_graphic, get_graphic_mut 로 어느 훅에서나 꺼낸다)
│   │   ├── ShaderProgram (기본), 묶음, 파티클, 블렌드 섞기 셰이더, 바꿔 낄 프로그램 표
│   │   ├── ImageTextureCache
│   │   ├── TextStringTextureCache (데스크탑: rustybuzz 모양 짓기, fontdue 와 skrifa 래스터(맥은 크롬처럼 CoreText), 색 글리프 colorglyph, 맥 색 이모지는 CoreText,
│   │   │                           시스템 글꼴 사슬과 운영체제 대체 글꼴 / 웹: 브라우저 Canvas2D)
│   │   └── 묶음 버퍼, 클립 스택, 블렌드 섞기용 바탕 칸
│   ├── pending_scene_list, pending_font_registration_list, 페이지 이벤트 리스너
├── Scene[] (Box<dyn Scene>, 불러온 씬. unload 가 맡긴 작업을 기다리는 내리는 중인 씬도 원본처럼 여기 남는다)
│   └── SceneBase
│       ├── NodeTree ── slotmap<NodeId, WorldNode>, 루트 노드
│       │   └── WorldNode ── NodeTransform, Vec<Box<dyn Component>>, parent_id, child_ids
│       ├── Tween[] (TweenId 로 가리킴)
│       ├── 맡긴 애셋과 작업 목록 (다 끝나고 0.3초 뒤 initialize), unload 에서 맡긴 목록 (끝나야 내리기가 끝난다)
│       └── 눌린 노드, 첫 터치 콜백
└── 내린 뒤 살려 둔 씬[] (그 씬의 작업이 남았거나 불러오기가 덜 끝난 씬. tick, draw 는 받지 않고 작업의 with 가 받는다. 다 끝나면 destroy)
```

애셋은 원본처럼 중앙 매니저 없이 씬이 멤버로 든다. 같은 그림을 여러 번 그려도 GPU 텍스처는 Graphic 의 캐시가 하나만 만든다. 여럿을 함께 받을 때는 AssetLoader 가 동시 수만큼 요청하고 진행률을 센다.

그래픽 빌림: 씬의 그리기 함수(pre_draw, draw, post_draw, draw_on_load, draw_gizmos)는 &mut Engine 과 &mut Graphic 을 함께 받는다. 그래픽이 엔진 안에 있으면 둘을 함께 &mut 로 넘길 수 없어서, EngineRunner 는 Engine::with_graphic 으로 그래픽을 엔진에서 잠깐 꺼내(Option::take) 넘기고 그리기가 끝나면 돌려 넣는다. (씬 목록을 std::mem::take 로 꺼내 씬마다 &mut Engine 을 넘기는 것과 같은 꼴) 그동안 engine.get_graphic() 은 None 이고 인자의 graphic 이 곧 그 그래픽이다. RefCell 도 unsafe 도 쓰지 않아 이중 빌림으로 터질 길이 없다. 그리기 밖(create, initialize, tick, 컴포넌트 tick 의 context.get_engine(), 작업의 첫 구간과 await 다음 줄)에서는 그래픽이 엔진에 들어 있다.

## 4. 노드와 컴포넌트

- 노드 만들기: `let node_id = node_tree.create_child(parent_id, "name");` 그 뒤 `node_tree[node_id]` 로 꺼내 설정한다. 붙이지 않은 노드는 `create_node` 로 만들고 `add_child` 로 붙인다.
- 지우기: `destroy_node(node_id)` 는 자손까지 지운다. tick, 터치 도중에 부르면 비활성으로 두고 다 돈 뒤에 지운다.
- 컴포넌트 붙이기: 보통은 `node_tree[node_id].add_component(Sprite::...)`. 붙을 때 자식 노드를 만드는 컴포넌트(UIView 같은)는 `node_tree.add_component(node_id, ...)` 로 붙여야 `attach_to_tree` 가 불린다.
- 컴포넌트 문맥(ComponentContext): tick, 터치, 휠에서 받는다. 자기 노드와 트랜스폼, 트리 전체(다른 노드 찾기, 만들기, 지우기), 엔진(입력, 시간, 뷰, 오디오)을 꺼낸다. 부르는 컴포넌트 하나만 노드에서 잠깐 빠져 있고, 같은 노드의 다른 컴포넌트는 `context.get_node_mut().get_component_mut::<T>()` 로 찾는다.
- 씬 코드에서 다른 노드도 만지는 컴포넌트(위젯 같은)를 바로 바꿀 때는 `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)` 로 컴포넌트와 트리를 같이 쥔다. 자주 쓰는 것은 트리를 받는 `*_in_tree` 함수로 따로 두었다. (UIWindow::close_in_tree, UIScrollView::set_shows_vertical_scroll_bar_in_tree 같은 것)
- draw 는 노드 트랜스폼이 걸린 상태에서 자기 트랜스폼만 받는다. 그리는 동안 트리는 바뀌지 않는다. 노드의 출력 상태 앞뒤 훅 `push_transform`, `pop_transform` 은 자기 컴포넌트와 자식, 기즈모를 감싸서 불린다. (원본 노드가 pushTransform, popTransform 을 덮어쓰던 자리, TouchEffect 가 쓴다)
- 노드의 켜진 컴포넌트가 모두 그리는 사각형(`calculate_drawn_rect`)을 알려 주면, 그 합을 그릴 때의 Graphic 변환(노드 배율, 회전, 부모가 밀어 넣은 변환 포함)으로 화면에 대 보고 밖이면 그 노드의 컴포넌트를 그리지 않는다. 자식은 따로 판정한다. 기본은 `is_drawn_inside_content` 가 true 인 컴포넌트(Paint, Sprite, Mask 등)만 content 영역을 알려 주고 나머지는 모른다(거르지 않음). Text 와 UILabel 은 정렬, 넘침, 줄바꿈 결과로 셈한 글 사각형을, ShadowText 와 ShadowImage 는 그림자 여백까지 넣은 사각형을 알려 준다. `is_tick_needed` 가 false 인 컴포넌트(Paint, Sprite, Text, Mask)만 붙은 노드는 tick 때 꺼내지 않는다.

## 5. 프레임 루프

```
플랫폼: 창/캔버스와 GL 을 만들고 EngineRunner.attach_graphic → apply_pending_scenes
플랫폼: 최대 FPS 차례가 아닌 프레임은 건너뛴다 (웹은 원본처럼 처리한 프레임 수로 센다, platform/framelimiter.rs)
매 프레임 EngineRunner.update_engine:
1. 바이트가 온 글꼴을 글자 캐시에 등록
2. graphic.apply_settings()          뷰포트, 프로젝션, 상태 리셋, 화면 비우기 (preserve_drawing_buffer 면 원본처럼 스텐실만 비운다)
3. time_manager.update(), input_manager.tick() (게임패드 폴링, 입력 동작 갱신 포함)
4. 깨어난 작업 폴링 (task::poll_tasks, 작업마다 많아야 한 번. 씬 작업은 그 씬과 엔진을, 엔진 작업은 엔진을 with 안에서 빌린다)
   → 내리기 마무리 (unload 가 맡긴 작업이 끝난 씬을 목록에서 빼고 unload_scene_async 의 Promise 를 푼다, 작업이 다 끝난 살려 둔 씬은 destroy)
5. 씬마다:
   - 맡긴 애셋과 작업을 기다리는 씬: 다 끝나고 0.3초가 지났으면 initialize 와 resize(그리고 load_scene_async 의 Promise 를 푼다), 아니면 draw_on_load 만
   - scene.tick → SceneBase.tick: 노드 tick(컴포넌트 → 자식) → 휠, 터치 전달 → 트윈
   - 씬의 터치 훅(touch_press, 뗌 또는 누르고 있음 또는 취소, touch_wheel, 원본 Scene.tickTouch 차례)
   - pre_draw → draw(NodeTree.draw 재귀) → post_draw → (기즈모)   그래픽은 Engine::with_graphic 으로 엔진에서 잠깐 꺼내 인자로 넘긴다 (3 장 그래픽 빌림)
6. 통계 출력(옵션: FPS, DrawCall, Vertex, Culled) → 마우스 커서(PointerCursor) → 1회성 입력 플래그 지우기
7. graphic.end_frame() (묶음 내보내기) → 검증용 화면 저장(request_frame_capture, 데스크탑 VANILLA_CAPTURE_*, 웹 ?capture=) → 프레임 번호 증가 (첫 프레임 1)
8. 씬 목록 바꾸기 (apply_pending_scenes: 내릴 씬 finalize, unload 와 내리기 마무리, 불러온 씬 create. create 와 unload 에서 맡긴 작업은 그 자리에서 첫 await 까지 돈다)
플랫폼: 데스크탑은 preserve_drawing_buffer 면 update_engine 앞에 지난 프레임 그림을 되그리고 뒤에 떠 둔다(desktop/preserveddrawingbuffer.rs)
플랫폼: 데스크탑은 렌더 배율 상한이나 캔버스 비 상한으로 줄여 그렸으면 창의 캔버스 자리로 늘이고(desktop/renderscaler.rs), 화면 넘기기 (데스크탑 swap_buffers, 웹은 콜백이 끝나면 브라우저가)
플랫폼: 웨이랜드는 EGL 스왑 간격을 0 으로 두고, 넘기기 전에 지난 프레임 콜백을 1/20 초까지 기다린 뒤 새 콜백을 보낸다 (desktop/waylandframecallback.rs)
```

씬 불러오기는 원본의 create → load(비동기) → set_loaded → initialize 를 create → (맡긴 애셋과 작업을 기다림) → initialize 로 옮겼다. 원본 async load 의 몸은 create 에서 spawn_scene_task 로 맡긴 불러오기 작업이고(맡기는 자리에서 첫 await 까지 돈다), track_task 로 씬에 맡기면 엔진이 그 작업이 끝나기를 기다린다. (애셋 손잡이를 track_asset 으로 맡기는 동기 꼴도 그대로 된다) create 바로 뒤에 루트를 원본 Scene.load 처럼 뷰 크기로 맞추고, 맡긴 것이 다 끝난 뒤에도 원본 SceneManager.loadScene 처럼 0.3초 더 로딩 화면을 보인다. 데스크탑은 애셋이 그 자리에서 읽혀서 작업이 없으면 create 뒤 0.3초에 initialize 가 불린다.

씬 내리기는 원본 SceneManager.unloadScene 처럼 finalize, unload 를 부르고 unload 가 track_task 로 맡긴 작업(원본 await scene.unload)이 끝날 때까지 씬을 목록에 둔다. (원본처럼 그동안 tick, draw 를 받는다) 끝나면 목록에서 빼고 unload_scene_async, unload_all_scenes_async 의 Promise 를 푼다. 그 씬의 작업이 남았거나 불러오기가 덜 끝났으면 원본의 Promise 사슬처럼 씬 값을 살려 두고(tick, draw 없이), 불러오기가 끝나면 원본 loadScene 처럼 initialize 까지 부른다. 다 끝나면 destroy 를 부르고 버린다.

async/await: 엔진은 외부 런타임 없이 스레드 하나짜리 실행기(core/task.rs)를 품는다. 퓨처는 Send 가 아니어도 되고 데스크탑과 웹이 같은 코드로 돈다.
- 맡기기: spawn_task(&mut Engine), spawn_scene_task(&mut Engine, &mut 씬)는 원본 async 함수를 부를 때처럼 그 자리에서 첫 await 까지 한 번 폴링한다. 받은 &mut 로 칸을 세우고 폴링이 끝나면 앞의 값으로 되돌린다. (그래픽은 엔진이 들어 첫 구간의 with_graphic 도 그래픽을 받는다. 그리기 함수 안에서 맡기면 엔진이 그래픽을 그 함수에 내어 준 동안이라 None) &Engine 만 있는 자리는 spawn_deferred_task, spawn_deferred_scene_task 로 맡기고 다음 폴링 차례에 처음 돈다.
- 깨우기: 원본 마이크로태스크 줄처럼 깨운 차례대로 줄에 넣고(Waker 는 std::task::Wake, 줄은 Arc 와 Mutex), 폴링 차례에 줄이 빌 때까지 앞에서부터 폴링한다. 한 작업이 Promise 를 풀거나 끝나면 기다리던 작업은 같은 차례에 풀린 차례대로 이어 간다. 폴링되는 도중에 스스로를 깨운 작업(엔진 상태를 보는 퓨처: 애셋 손잡이, wait::seconds, wait::next_frame)은 다음 프레임 줄에 들어간다. Promise 는 resolve 가, TaskHandle 은 그 작업이 끝날 때, 웹 Promise 는 JsFuture 가 깨운다. 내리기 마무리가 푼 Promise 를 기다리던 작업도 같은 차례에 이어 간다.
- 엔진과 씬에 닿기: 퓨처는 &mut 를 await 너머로 쥘 수 없어서, 작업은 컨텍스트의 with(|씬, 엔진| ...) 클로저 안에서만 빌린다. 폴링하는 동안만 스레드 지역 칸에 엔진과 그 작업의 씬을 가리키는 포인터를 두고, with 는 그것을 꺼내 쓰고 되돌린다. (with_graphic 은 그 엔진의 Engine::with_graphic 으로 그래픽을 잠깐 꺼내 넘긴다) 포인터는 실행기가 &mut 로 쥔 서로 다른 값(EngineRunner 의 엔진, 씬 목록의 한 칸)에서 만들고 폴링이 끝날 때까지 실행기는 그 &mut 를 쓰지 않는다. 폴링하는 작업 목록은 엔진에서 꺼내 둔 지역 값이다. with 가 쓰는 동안 칸이 비어 있어 두 번째 &mut 가 생길 길이 없고, 컨텍스트는 복제할 수 없고 with 가 &mut self 를 받아 겹친 with 는 컴파일되지 않는다. 클로저 인자의 수명은 클로저마다 새로 매겨져 참조가 밖으로 새지 않는다. 씬은 Any 로 컨텍스트의 타입인지 보고, 씬 핸들로 그 작업의 씬인지 본다. (빼돌린 컨텍스트를 작업 바깥이나 다른 씬에서 쓰면 이중 빌림 대신 까닭을 적은 패닉)
- 원본에서 Promise 를 돌려주는 load 들은 애셋이나 씬을 빌리지 않는 나눠 든 상태의 손잡이(IntoFuture)를 돌려준다. 그래서 원본 await this.atlas.load(경로) 가 scene_context.with(|씬, 엔진| 씬.atlas.load(엔진, 경로)).await 한 줄이다.
- 엔진을 빌려 이어 가는 퓨처(AssetLoader, SpriteAtlas, LanguagePack, UIFont 의 손잡이, 모델 짓기)는 같은 칸을 crate 안에서 잠깐 꺼내 update 를 부르거나 GL 을 쓴다.
- 화면 페이드(fade_screen_async)는 다 어두워진 tick 에서 game_scene! 이 on_dark 를 씬 작업으로 맡겨 그 자리에서 첫 await 까지 돌린다. (원본 onDark 를 부르는 자리)

데스크탑 찍기 모드(VANILLA_CAPTURE_PATH)는 창을 숨기고 돈다. 숨긴 창은 다시 그리기 요청과 수직 동기가 없어 about_to_wait 에서 프레임을 바로 돌고, 최대 FPS 가 없으면 모니터 주사율(모르면 60)로 빠르기를 맞춘다. 창을 보려면 VANILLA_CAPTURE_SHOW_WINDOW=1 이다.

## 6. 그리기

- 모든 2D 그리기는 `Graphic.draw_vertices` 한 곳으로 모인다. 묶을 수 있으면(클립 영역 기록 중 아님, 바꿔 낀 셰이더 없음, 셰이더로 섞는 블렌드 아님) 정점을 CPU 에서 변환해 모았다가 같은 텍스처가 이어지는 동안 한 번에 그린다. 원본과 같은 묶음 셰이더로 정점마다 색과 틴트를 실어 틴트가 걸린 그리기도 묶는다.
- 블렌드가 바뀔 때, 클립을 걸거나 풀 때, 파티클을 그릴 때, 프레임 끝에 모은 것을 먼저 내보낸다(`flush_batch`). GL 을 직접 쓰는 코드는 그 전에 flush_batch 를 부르거나 `run_external_pass` 로 감싼다.
- 마지막으로 건 셰이더 프로그램과 텍스처 유닛 0 의 텍스처를 기억해 같은 것을 다시 거는 GL 호출을 건너뛴다. (원본과 같다) `get_gl` 로 GL 을 내줄 때, `restore_render_state`, 프레임 시작과 끝, 웹 GL 컨텍스트를 잃거나 되살릴 때는 잊는다.
- 셰이더 바꿔 끼우기: `set_shader_program_override` 가 걸린 동안은 묶지 않고 바꿔 낀 프로그램으로 바로 그린다(ShaderSprite). 바꿔 낄 프로그램은 `resolve_shader_program` 이 이름으로 담아 두고, 어트리뷰트 위치를 기본 프로그램과 같게 고정해 기본 버텍스 어레이를 같이 쓴다. 미리 링크해 둘 셰이더는 ShaderWarmer 로 넘긴다. (KHR_parallel_shader_compile)
- 화면 효과(ScreenEffect)는 엔진을 거치지 않는다. 씬이 pre_draw 첫머리에서 begin, post_draw 끝에서 end 를 부르면, end 가 화면(또는 영역)을 텍스처로 떠 와 FullscreenPass 사슬을 걸고 되돌려 놓은 뒤 `restore_render_state` 로 Graphic 상태를 되살린다.
- 클립은 원본처럼 스텐실 기반이다.
- 블렌드는 원본처럼 26종을 모두 그린다. 13종(포터 더프 연산자, Lighter, Multiply, Screen)은 고정 블렌딩(`BlendMode::get_blend_function`), 나머지 13종(Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity)은 그리기마다 그릴 영역(정점의 경계 상자)만 지금 프레임버퍼에서 `blit_framebuffer` 로 같은 형식, 같은 자리의 칸에 떠 와 W3C Compositing and Blending Level 1 공식대로 셰이더로 섞는다. 셰이더 글은 원본과 같다. 이 모드에서는 묶지 않고 그리기 하나에 복사 한 번과 드로우 콜 한 번이 든다. 파티클은 서로 겹치지 않는 쿼드끼리 모아 한 번 떠 오고, 바꿔 낀 셰이더는 덧그림 칸(데스크탑 RGBA16F, 웹은 EXT_color_buffer_float 가 있으면 RGBA16F, 없으면 RGBA8)에 먼저 그린 뒤 섞어 옮긴다. Canvas2D 와 달리 SourceIn, SourceOut, DestinationIn, DestinationAtop, Copy 는 그린 도형 안만 바꾼다(원본과 같음).
- Multiply 는 기본 프레임버퍼에서는 고정 블렌딩과 묶어 그리기 그대로이고, 렌더 타겟에 그릴 때만 위의 13종처럼 그릴 영역 아래를 떠 와 W3C 식(Cb x Cs)으로 섞는다. 고정 블렌딩은 바탕이 반투명하면 W3C 식과 달라서다. (원본과 같이 고쳤다)
- 기본 프레임버퍼에 그릴 때는 알파를 쓰지 않는다(`apply_color_mask`, 원본에 없음). 원본과 웹은 캔버스가 `alpha: false` 라 화면 알파가 늘 1 이고, 데스크탑 기본 프레임버퍼도 프레임 첫머리에 알파를 1 로 비운 뒤 그대로 두어 DST_ALPHA 를 쓰는 모드와 셰이더 섞기가 웹과 같게 나온다. 렌더 타겟에는 알파도 쓰고, GL 을 밖에 내줄 때는 모두 쓰는 마스크로 되돌린다.
- 묶어 그리기와 거르기는 `set_batching_enabled`, `set_culling_enabled` 로 끈다.
- 3D 렌더 타겟은 멀티샘플(RenderTargetOptions.sample_count, 기기가 받는 수로 줄임)과 깊이 비트(16, 24)를 받는다. 툰 3D 의 HDR 마무리(ToonPostProcess)는 멀티샘플, 실수 판(못 쓰는 기기는 RGBA8 LDR 길), 다단 블룸(MultiLevelBloomEffect), 색 보정을 건다.
- 툰 셰이딩은 그늘 램프다. (원본과 같은 셰이더) 빛을 받는 몫 x(반 램버트 기본, shade_offset 을 더하고 해는 그림자를 곱함)를 칸마다 자리, 색, 부드러움(fwidth 로 막음)인 램프나 램프 텍스처에 대어, 해는 램프 색을 통째로, 채움 빛과 점광과 스포트라이트는 램프의 밝기 단계를 받는다. 하이라이트와 림은 0.5 를 가른 단단한 꼴에서 부드러운 꼴까지, 림은 더한다. 색 = 바탕색 x irradiance + specular x specularMask + rim x rimMask + 빛남, 그 뒤 매트캡. 노멀은 역행렬의 전치로 옮긴다. 점광과 스포트라이트 그림자는 ShadowAtlas(점광은 큐브 여섯 칸, 스포트라이트는 원근 한 칸)를 set_local_shadows 로 받아 그 빛의 x 에 곱한다. 색마다 빛 색 쓰기, 빛 세기 막기, 그늘 쪽 림, 천사 고리(보기 노멀 x 와 2 번 UV 의 y, 카메라 기울임을 되돌림)는 유니티 툰 셰이더 소스의 식이다. 아틀라스 칸은 URP 기본처럼 1024 에서 다 들어갈 때까지 반씩 줄인다.
- 툰 바깥선은 두 가지이고 함께 써도 된다. (원본과 같은 셰이더)
  - 껍데기(inverted hull): MeshBuilder 가 같은 자리의 노멀을 모아 평균한 쪽으로 민 굵기를 짓고, ToonRenderer 가 바탕 자리에서 그리기마다의 굵기 배와 거리 배(set_outline_distance_scale)만큼 밀어 뒷면만 그린다. 색은 그리기마다(outline_color).
  - 화면 공간(ToonScreenOutline): 불투명한 모형을 같은 행렬로 한 번 더 그려 선의 판(노멀 팔면체 접기, 선 번호 14 비트, 꼴 2 비트, 깊이 24)을 짓고, 화소와 바로 이웃 넷 사이 경계마다 한 번 보아 선이면 경계 가운데를 씨앗(RGBA16UI, 반 화소 단위)으로 쓴다. 바깥 선은 빈 곳, 다른 번호, 서로의 접평면에서 깊이 문턱보다 떨어진 같은 번호. 꺾임 선은 같은 번호에서 두 노멀이 꺾임 각보다 벌어진 곳. 씨앗을 점프 플러딩(걸음 k, k/2, ..., 1)으로 퍼뜨리고, 그리는 영역 화소마다 선의 판 2x2 칸의 씨앗까지 거리로 clamp(굵기 / 2 - 거리 + 0.5, 0, 1) 만큼 섞는다. 선이 화소 사이에 서므로 띠와 평행한 둘째 선이 없다. 선의 판과 씨앗은 그리는 영역 해상도이고 set_resolution_scale 은 점프 플러딩 판만 줄여(모으기 패스) 줄여도 계단과 점선이 없다. 그리기 줄(ToonDrawQueue::set_screen_outline)에 맡기면 선의 판은 몸 앞에, 섞기는 바깥선 껍데기 뒤에 한다.

## 7. 애셋

- `ImageAsset`, `TextAsset`, `JsonAsset`, `BlobAsset`, `FontAsset`: `load(engine, 경로)` 로 요청하고 `scene_base.track_asset(&asset)` 으로 맡긴다. load 는 원본처럼 await 할 수 있는 손잡이를 돌려준다. (원본 await asset.load(경로) 는 엔진 작업 안에서 `scene_context.with(|씬, 엔진| 씬.애셋.load(엔진, 경로)).await`, 그림은 풀기까지, 글꼴은 글꼴 목록에 올리기까지 기다린다) 데스크탑은 애셋 폴더 후보에서 파일을 읽고, 웹은 `asset_base_url + 경로` 를 fetch 한다.
- 실패한 애셋도 "끝난 것"으로 쳐서 씬이 멈추지 않는다. 풀지 못한 JSON 은 원본처럼 읽히지 않은 것이다.
- AssetLoader 는 여러 애셋을 동시 수(기본 6)만큼 받고 진행률과 실패 목록을 센다. 다 받은 그림은 원본 img.onload 처럼 풀어 보고, 깨진 것은 실패로 친다. 푼 그림은 읽기 손잡이에 담겨 ImageAsset 이 다시 풀지 않는다.
- `AudioAsset` 은 오디오 백엔드와 붙어 있어 `engine.load_audio_asset` 으로 읽고 `track_audio_asset` 으로 맡긴다.
- 그림: 데스크탑은 png, jpeg, webp, gif(첫 장), bmp, ico 를 image 크레이트로, SVG 를 resvg 로, AVIF 를 엔진의 HEIF 상자와 트랙 읽기와 rav1d 로 푼다. 박힌 색 프로필(PNG 의 cICP, iCCP, gAMA 와 cHRM, JPEG 의 APP2 ICC, WebP 의 ICCP, AVIF 의 colr)은 크롬처럼 sRGB 로 바꾼다. (core/colorprofile.rs, 스키아와 skcms 의 셈) 웹은 원본처럼 브라우저(HTMLImageElement 의 decode)가 푼다. 그림의 형은 원본처럼 응답의 content-type 이다. (SVG 는 형으로, 래스터는 내용으로 가린다. 응답이 없는 데스크탑은 내용으로 고른다)
- 소리: 웹은 원본처럼 브라우저 decodeAudioData 가 풀고, 데스크탑은 같은 결과가 나오게 platform/audio/decode 가 푼다. 꼴은 내용으로 가리고, symphonia 0.6 으로 디먹스해 FLAC, MP3, PCM, Vorbis 는 크롬과 같은 symphonia 해석기, Opus 는 opus-pure(libopus 를 옮긴 것), AAC 는 AAC-LC 2 채널까지 symphonia 의 AAC, HE-AAC(SBR, PS, ASC 에 적혔든 몸통에만 있든)와 3 채널 이상은 oxideav-aac(명세로 짠 해석기)로 푼다. 크롬이 FFmpeg 디먹스 값으로 버리는 앞뒤(Opus pre-skip 과 webm CodecDelay, DiscardPadding, MP3 LAME 지연, Vorbis granule, m4a 편집 목록과 iTunSMPB)를 같게 버리고, 정수 표본을 크롬 식으로 실수로 바꾸고, 원본 DECODE_SAMPLE_RATE(44100)로 크로미엄 SincResampler 를 옮겨 바꾸고, WebAudio 스피커 규칙으로 1 채널이나 2 채널로 내린다. `tests/assets/audio` 의 27 파일이 크롬 158 과 길이, 채널, 표본까지 같다. (Opus, AAC 는 1.5e-7 안팎) 표본율 바꾸기의 합 차례는 크롬처럼 aarch64 는 NEON, x86_64 는 AVX2 와 FMA 또는 SSE 를 따른다. 기대값은 애플 실리콘 맥의 크롬에서 뽑았고, CI 의 윈도우와 리눅스 x64 에서도 그 기대값과, 그 기계의 크롬 157 로 다시 찍은 기대값 모두와 같은 범위 안이다.
- 소리 내기: 데스크탑은 cpal 로 기본 출력 장치를 열고 그 표본율(원본 AudioContext 의 하드웨어 표본율)로 엔진 믹서를 돌린다. 원본이 짓는 그래프(AudioBufferSourceNode → 플레이어 GainNode → [StereoPannerNode] → 버스 GainNode 또는 출력, 비프 OscillatorNode → GainNode → 출력)를 128 프레임 퀀텀으로 그리고, 소스(시작 위치, 선형 보간과 끝의 외삽, 루프, playbackRate 는 f32), StereoPanner(모노와 스테레오 식), GainNode, 모노를 양쪽에 올리는 섞기, 출력의 -1 ~ 1 자르기를 크로미엄 audio_buffer_source_handler.cc, stereo_panner.cc 와 같게 셈한다. GainNode 의 gain 은 AudioParam 이고 자동화를 크로미엄과 같은 셈으로 표본마다 흘린다. `tests/assets/mixer` 의 그래프 아홉(자동화, 예약 정지, 그리는 도중 지금 값에서 줄여 끄기와 겹쳐 바꾸기 포함)이 크롬 158 OfflineAudioContext 와 1e-6 안에서 같다. (자동화의 exp, pow 는 크롬도 엔진도 운영체제의 수학 라이브러리라, 애플 실리콘 맥 크롬의 기대값과 윈도우 x64 는 줄여 끄는 그래프의 제곱합이 1.1e-6 다르고 그 기계의 크롬과는 1e-6 안이다. 시험은 기대값을 찍은 기계가 다르면 제곱합을 1e-5 안으로 본다) BeepPlayer 는 원본 playTone 의 그래프를 그대로 짓고, OscillatorNode 는 크로미엄 oscillator_handler.cc 와 CPU 별 커널(arm64 NEON, x86 SSE2)을 옮겨 대역 제한 표, frequency 자동화, 읽기 자리와 보간을 같은 차례로 셈한다. 비프 아홉 경우가 크롬과 표본마다 1.2e-7 안에서 같다. (x86 판은 CI 의 윈도우와 리눅스 x64 크롬 157 과 표본마다 1.5e-7 안이다) 초안 공간 소리(experimental::spatialaudio)의 PannerNode 는 크로미엄 panner_handler.cc 를 옮긴 panner.rs 가 셈하고, panningModel "HRTF" 는 hrtf/ 가 크로미엄 HRTFPanner(귀마다 지연과 FFT 합성곱, 방위 192 칸과 높이 10 칸, 바뀔 때 크로스페이드)와 크롬이 싣는 IRCAM 자료 Composite.flac 으로 셈한다. FFT 는 크롬 158 과 같은 rustfft 6.4 를 같은 꼴로 부르고, 커널 보간의 log10 과 pow 는 크롬처럼 fdlibm 이다. 크롬 맥의 vDSP 는 복소 곱과 곱해 더하기에서 곱셈과 덧셈을 합쳐(애플 실리콘에서 잼) 엔진도 그 맥에서만 합친다. `tests/assets/spatialaudio` 의 HRTF 그래프 여덟(22050 ~ 96000 Hz)이 애플 실리콘 맥의 크롬 158 과 모든 표본에서 f32 까지 같다. (CI 의 윈도우와 리눅스 x64 크롬 157 과는 HRTF 와 등력 PannerNode 모두 모든 표본에서 1.4e-17 안이다)
- "data:" 주소: 데스크탑 애셋 읽기(request_asset_bytes, load_image, load_audio_asset)와 타임라인이 base/dataurl.rs 로 푼다. 웹은 기준 주소를 붙이지 않고 브라우저 fetch 에 맡긴다.
- 글꼴: ttf, otf, woff, woff2 를 읽는다. 데스크탑은 woff, woff2 를 wuff 로 풀고, 운영체제 글꼴은 글꼴 모음(TTC)에서 이름이나 굵기로 얼굴을 고른다. 웹은 원본처럼 글꼴 바이트를 document.fonts 의 FontFace 로 올린다.
- 이미지는 GPU 에 올린 뒤 CPU 픽셀을 놓는다. 픽셀을 다시 읽을 그림은 `Image.set_pixels_retained(true)` 로 남긴다. (ShadowImage 는 받은 그림을 스스로 남기고, 이미 놓였으면 텍스처를 프레임버퍼에 붙여 되읽는다. Canvas2D 의 drawImage 와 createPattern 도 놓인 그림은 텍스처에서 되읽는다) GL 컨텍스트를 잃었다 되살리는 길은 아직 없다.
- 기본 글꼴은 엔진에 넣지 않는다. 둘 다 시스템 글꼴이 기본이다.
  - 데스크탑: `default_font_paths` 가 비었으면(기본) 원본 SYSTEM_FONT_STRING 의 이름 차례로 운영체제 글꼴을 찾아 사슬로 잇는다. 맥은 크롬처럼 -apple-system 을 모르는 이름으로 넘겨 Apple SD Gothic Neo, 윈도우는 Segoe UI 와 맑은 고딕이다. 폰트 문자열의 올리지 않은 이름도 운영체제 글꼴에서 찾고, 일반 이름(serif, sans-serif, monospace 등)은 크롬의 기본 글꼴 표대로 고른다. 글꼴 목록에 없는 글자는 운영체제가 고른 대체 글꼴(맥 CoreText, 윈도우 DirectWrite, 리눅스 fontconfig)로 그린다. (platform/desktop/systemfont.rs)
  - 웹: 원본처럼 늘 브라우저 Canvas2D 로 굽고 잰다. (platform/web/systemfont.rs)
  - 게임 글꼴은 `default_font_url`, `default_font_bytes`, FontAsset 가운데 하나로 준다. (원본 기본값 DOSGothic 은 뺐다)

## 8. 코드 꼴

이름은 러스트 규칙(Rust API Guidelines C-CASE)을 따른다: 함수, 메서드, 변수, 필드, 모듈은 snake_case, 타입과 트레이트와 열거 값은 UpperCamelCase, 상수는 SCREAMING_SNAKE_CASE. vanilla.js 의 이름은 이 규칙으로 바꿔 옮긴다. (setFillColor → set_fill_color, Colors.white → Colors::WHITE) 나머지는 vanilla.js 지침을 따른다: 이름 줄이지 않기, 중괄호 늘 쓰기, else 는 다음 줄, `//====` 머리 주석에 독백체 설명, 파일 머리에 원본과의 대응과 다른 점. rustfmt 는 else 줄바꿈을 망가뜨려서 돌리지 않는다.

문서 주석: 머리 주석(`//====` 두 줄 사이)의 설명 줄은 rustdoc 의 `///`, 파일 머리는 `//!` 다. 인자는 `# 인자` 절의 `` - `이름`: 설명 ``, 돌려줌은 `# 돌려줌` 절, 예제는 `# 예제` 절의 ```rust 칸(바로 앞 한 줄이 제목)으로 쓰고, 칸 바로 뒤 줄의 `<!-- image: <파일 이름> -->` 이 예제 그림(docs/guide/<판>/images/, 가이드와 같이 씀)이다. 예제는 `cargo test --doc` 이 컴파일하고 돌린다. (창이나 GL 이 있어야 하면 no_run) 이 주석에서 API 문서 자료(docs/api/<판>.json)를 뽑는다. (tools/apidoc) 자세한 꼴은 CLAUDE.md 의 '문서 주석'.

## 9. 원본과의 대응

2026-11-26 dev 기준으로 원본 `src/` 의 .js 200 파일 가운데 182 파일을 옮겼고, 18 파일은 일부러 옮기지 않았다. (원본 파일마다 같은 이름의 .rs 가 있는지 보고, 없는 것은 러스트 파일 머리 주석의 원본 경로로 찾아 셌다. 0.4.0 뒤 기능의 초안 자리 src/experimental 은 두 엔진에 같이 두고 셈에서 뺀다)

- 같은 이름으로 옮긴 것 176 파일. 폴더는 원본과 같다. (graphics3d/, graphics3d/toon/, animation/, misc/, resource/, core/fullscreenpass) 원본 core/graphic/* 만 러스트 판에서 core/ 바로 아래에 있다.
- 다른 파일에 합친 것 6 파일:
  - base/node.js, core/node/componentnode.js, core/node/transformnode.js: 노드 종류가 하나라 core/node/worldnode.rs 와 nodetree.rs 에 합쳤다.
  - core/scenemanager.js: 엔진이 씬을 들고 있어 core/engine.rs 와 core/scene.rs 에 합쳤다. (load_scene, unload_scene, get_all_loaded_scenes)
  - core/touchraycaster.js: core/scene.rs 의 SceneBase::tick_touch 와 core/node/nodetree.rs 의 누름 판정, ui/touchrecognizer.rs 에 합쳤다.
  - resource/jsonasset.js: resource/textasset.rs 에 TextAsset 과 함께 둔다.
- 일부러 옮기지 않은 것 18 파일:
  - 빈 파일이거나 아무 일도 하지 않는 것 4: base/calculator.js(0 을 돌려주는 빈 함수), core/layer.js(빈 스텁), core/resources.js(0 바이트), misc/uibuilder.js(NodeLayout 으로 대체됐다는 주석 한 줄)
  - 죽은 코드 5: deprecated/aabb.js, deprecated/transform2d.js(파일 전체가 주석), deprecated/gl2d.js(내보내는 것 없음), deprecated/webgl2.js(내보내지 않아 쓰이지 않음), deprecated/anchoredworldmnode.js(폐기, import.js 에서도 막아 둠)
  - 러스트 언어와 표준이 맡는 것 8: base/object.js, base/reflection.js(createGUID 만 base/identifier.rs 의 create_guid 로 옮겼다), base/singleton.js, base/collection 의 dictionary.js, list.js, queue.js, set.js, stack.js. 트레이트, Any, enum, Vec, HashMap 이 같은 일을 한다.
  - 다른 기능이 덮는 것 1: deprecated/camera.js. (원본도 폐기 예정) 회전과 begin/end 를 Camera2D(misc/camera2d.rs)에 넣어 그 기능을 모두 덮는다.
- 러스트에만 있는 파일은 플랫폼 층(platform/ 아래 23 파일, mod.rs 빼고), 글자 모양 짓기와 색 글리프(core/textshaping.rs, core/colorglyph.rs), 그림 풀기(core/image.rs, core/avifimage.rs), 글 입력 줄(core/textinput.rs), 노드 저장소(core/node/nodetree.rs), async 실행기(core/task.rs), 브라우저 자리(core/canvas2d.rs 와 그 아래 셋, core/modaldialog.rs, core/browseruitext.rs, misc/storagetext.rs, ui/uiinputfieldmenu.rs)다. 원본에서는 브라우저가 하는 일이다. (web/pane.js 는 web/pane.rs 와 그 아래 dompane.rs, nodepane.rs 로 나눴다)
- 옮기며 바꾼 쓰임새:
  - GameScene, UIScene 의 덮어쓰기는 GameScene, UIScene 트레이트와 game_scene!, ui_scene! 매크로다. 바탕이 resize, layout, post_draw 를 스스로 부른다. (game/gamescene.rs)
  - UINode 는 노드에 붙는 컴포넌트이고, 솔버 값을 노드의 자리와 content_size 로 옮긴다. (ui/uinode.rs, ui/uiscene.rs)
  - 원본의 실행 중 반사(uidocument.js 의 collectReflectedProperties, saveByReflection, loadByReflection, devtools.js 의 getComponentProperties)는 절차 매크로 #[reflect](macros/ 크레이트, 엔진이 vanilla::reflect 로 다시 내보낸다)가 컴파일 때 지은 코드로 한다. 저장소는 엔진과 매크로 두 크레이트의 워크스페이스다. 엔진 컴포넌트는 모두 붙였고, 원본 상위 클래스는 품은 필드를 #[reflect(base = 필드)] 로 잇는다. &self 게터, 트리를 받는 게터, &mut self 게터, 값 하나를 받는 세터, 트리와 값을 받는 세터를 모두 반사로 부른다. (core/reflection.rs)
  - UIDocument 는 원본처럼 쓰기, 읽기 함수가 없는 종류를 반사로 쓰고 읽는다. 원본 노드 종류(WorldNode 하위 클래스, UIInputField, UIScrollBar 같은 것)는 러스트 판의 컴포넌트를 register_node_type 으로 등록해 원본 문서의 노드 type 과 생성자가 만든 컴포넌트, 자식 꼴 그대로 쓰고 읽는다. 원본 에디터(tools/uieditor)가 등록하는 종류(컴포넌트 35, 노드 종류 12)는 register_editor_types 한 줄이다. DevTools 인스펙터는 원본처럼 게터를 모두 불러 보인다. 컴포넌트마다의 문서, 노드 종류마다의 문서, 에디터 팔레트 15 종으로 지은 문서(tick 전후), 인스펙터 게터 목록, 반사 속성 목록과 반사 저장이 원본을 node 로 돌려 뽑은 기대값(src/ui/testdata/uidocument)과 같은지 시험한다. (ui/uidocument.rs, misc/devtools.rs)
  - web/Pane(화면 분할 패널)은 웹에서 원본처럼 DOM 요소로, 데스크탑에서 엔진 노드로 짓는다. 크기 셈과 끌기 셈은 두 플랫폼이 같은 함수다. 데스크탑은 컨테이너가 노드이고 노드를 고치는 함수가 NodeTree 를 받는다. (web/pane.rs)
  - 브라우저가 해 주던 것은 엔진이 같은 API 로 한다. 원본과 게임의 캔버스와 2D 컨텍스트는 Canvas2D(웹은 브라우저 캔버스, 데스크탑은 tiny-skia), window.alert, confirm, prompt 는 Engine::alert, confirm, prompt(웹은 브라우저 창, 데스크탑은 엔진이 그린 크롬 탭 모달 같은 창, await 꼴도 있다, 원본 Engine 에도 같은 이름과 동작으로 있다), html lang 과 meta 는 Document, 직접 누르는 입력 필드의 크롬 편집 칸 메뉴와 DOM 입력 칸의 되돌리기는 데스크탑에서 엔진이 그린다. LocalStorage 와 PersistedStore 는 원본 JS 글의 짝 없는 서로게이트를 원래 단위 그대로 지키고, 짝 없는 서로게이트로만 다른 두 키도 따로 든다. 원래 단위가 필요하면 LocalStorage 의 코드 단위 함수(get_item_code_units, get_keys_code_units, get_item_by_key_code_units, set_item_by_key_code_units 같은 것)로 원본 localStorage 처럼 JS 글 그대로 읽고 쓴다. (misc/storagetext.rs)
  - SoundSystem 의 음악과 효과음 버스는 AudioBus 다. (원본에도 같은 꼴로 넣었다) 데스크탑은 엔진 믹서의 버스 노드, 웹은 GainNode 이고, get_gain_node 는 두 플랫폼 모두 AudioGainNode(웹 GainNode, 데스크탑은 버스 노드를 나눠 드는 Rc<MixerGainNode>)를 돌려준다.
  - SoundSystem 의 크기 바꾸기와 줄이기는 원본처럼 GainNode 의 AudioParam 자동화(value 대입, setValueAtTime, linearRampToValueAtTime, setTargetAtTime, cancelScheduledValues)와 source.stop(when)이다. 웹은 WebAudio 에 그대로 걸고, 데스크탑 믹서는 크로미엄 audio_param_handler.cc 를 옮긴 platform/audio/audioparam.rs 로 표본마다 같은 셈을 한다. (AudioPlayer, AudioBus 의 pub(crate) apply_gain_automation, get_gain_value, stop_at_time)
  - Cooldown.use(원본 base/timer.js), FullscreenPass.use, ShaderProgram.use 는 use 가 러스트 예약어라 r#use 로 부른다. activate, use_program 은 같은 일을 하는 이름으로 남긴다.
  - 노드 객체를 돌려주는 getParent, getChildren, getSiblings 는 핸들을 돌려주는 get_parent_id, get_child_ids, get_sibling_ids 다.
  - 갈아 끼운 저장소(LocalStorage.get_backend, PersistedStore.get_storage_backend)와 콜백 게터(UIButton.get_click_event 같은 것)는 Rc<RefCell> 에 담아 돌려주고, 부르는 쪽이 빌려 쓴다. SpriteAnimator.findSprite(형제를 쥐기)와 SharedBakeCache 의 놓기(release)는 빌림 규칙과 Rc 참조 수로 갈음한다.
- 글자는 원본(브라우저 Canvas2D)과 맞췄다. 오른쪽에서 왼쪽 글(양방향 정렬과 글자 모양 짓기, core/textshaping.rs), CSS 같은 글꼴 얼굴 고르기, 가짜 기울임과 굵게(Chromium 과 Skia 의 값), 크롬 캔버스처럼 낱말마다 모양 짓기, 폰트 문자열의 CSS 단위와 calc(), 낱말 나누기(데스크탑은 ICU4X 사전에 ICU4C CjkBreakEngine 의 셈을 옮겨 크롬 Intl.Segmenter 와 같게), 색 이모지(sbix, CBDT, COLR v0/v1, 맥은 CoreText 로 크롬처럼 작은 크기에서 키우고 내림), 맥 시스템 UI 글꼴의 opsz 축과 trak 자간(HarfBuzz 15 처럼).
- 작게 남은 차이:
  - 글자 자리는 크롬 HarfBuzz 처럼 16.16 배율에서 셈한다(third_party/rustybuzz). 맥 SFNS(system-ui) 8 ~ 128px, 굵기 300, 400, 700 과 기울임의 세 글 228 개 가운데 222 개가 크롬 158 measureText 와 끝자리까지 같고, 나머지 6 개는 커닝 짝 하나에서 HarfBuzz 위치 단위 1(1/65536px)이 드물게 달라 6.1e-5px 안이다. (그 한 단위의 까닭은 찾지 못했다)
  - 윈도우, 리눅스, 맥의 글꼴 갈래(DirectWrite, fontconfig, CoreText)는 CI 에서 같은 기계의 크롬 measureText 와 전진폭, 글꼴 위아래 길이를 217 글로 견준다. (tests/desktopplatform.rs) 맥 26(한국어 화면) 크롬 158 은 arm64 와 로제타 x86_64 모두, 리눅스(영어 화면) 크롬 158 은 217 이 모두 같다. 윈도우 system-ui 의 메뉴 글꼴(NONCLIENTMETRICS)은 크롬과 같은 글꼴이다. 대체 글꼴은 크롬 FontCache::PlatformFallbackFontForCharacter 의 운영체제별 차례를 따른다:
    - 기본 글꼴 사슬(원본 SYSTEM_FONT_STRING)은 기본 글꼴 이름 default 를 청했을 때만 잇는다. 원본도 기본 글꼴일 때만 폰트 문자열이 그 사슬이고, 다른 이름이면 크롬은 사슬 없이 시스템 대체로 간다.
    - 대체는 첫 글꼴(빈칸을 가진 첫 얼굴, FontFallbackList::PrimarySimpleFontDataWithSpace)과 폰트 문자열마다 따로 묻고, 대체 얼굴은 운영체제가 그 얼굴을 고른 글자만 그린다. (다른 글자를 위해 읽은 Geeza Pro 가 é 를 가로채지 않는다)
    - 이름 목록의 얼굴이 합친 글자(ï)를 갖지 않아도 정준 분해한 글자(i, U+0308)를 가지면 그 얼굴로 그린다. (HarfBuzz 정규화의 decompose, 크롬은 .notdef 가 남은 글자만 대체로 넘긴다)
    - 맥: 첫 글꼴의 CTFont(이름으로 찾은 글꼴은 청한 굵기와 기울기, 올린 글꼴은 바이트로 지은 것)에서 CTFontCreateForString, 고른 글꼴의 특성이나 굵기가 다르면 같은 이름에서 청한 특성과 굵기로 다시 고른다. (font_cache_mac.mm 의 GetSubstituteFont, GetAlternateFontPlatformData)
    - 윈도우: 굵게나 기울임이면 첫 이름의 보통 얼굴, 블링크 font_fallback_win.cc 의 블록과 문자별 표(태국어 Tahoma, monospace 의 아랍어와 히브리어 Courier New, 한자는 화면 언어), 넓은 글꼴 목록, 폰트 문자열의 첫 이름을 바탕으로 한 DirectWrite 대체 차례. (font_cache_skia_win.cc)
    - 리눅스: 첫 글꼴과 상관없이 fontconfig 에 글자로 묻는다. (font_cache_linux.cc)
  - 모든 글자가 U+00FF 이하인 글은 크롬 HarfBuzzShaper 처럼(8 비트 글) 라틴 문자로 모양 짓는다. 숫자처럼 Common 뿐인 낱말도 latn 기능을 쓴다. (리눅스 Norasi 의 DFLT 에만 있는 숫자 커닝을 걸지 않는다)
  - 실제로 닿는 상자(actualBoundingBox)는 찍기만 하고 견주지 않는다. (리눅스 "Hello World Wifi" 의 아래가 엔진 1, 크롬 0 처럼 1px 안팎으로 다르다) 부모 글꼴 치수(ex, ch 단위의 글꼴)는 윈도우, 리눅스 크롬과 견주지 못했다.
  - 데스크탑 그림의 색 프로필은 크롬 WebGL texImage2D 길과 같게 sRGB 로 바꾼다(core/colorprofile.rs, PNG, WebP 는 비트까지 같다). AVIF 는 크롬 158 의 CrabbyAvif 길(libyuv 가 받는 꼴은 libyuv 의 정수 셈, 그 밖은 libavif 실수 셈, 높은 비트 깊이는 libyuv HalfFloatPlane 의 반정밀)을 따라 WebGL 길과 시험 그림 19 장이 비트까지 같다(core/avifimage.rs). 크롬 CPU 캔버스는 알파를 libyuv ARGBAttenuate 로 곱하고 높은 비트 깊이를 8 비트 libyuv 길로 풀어 그 둘에서 한 단계 넘게 다르다. JPEG 은 해독기(zune-jpeg 와 libjpeg-turbo) 차이로 한두 단계 다르다. 크롬 Canvas2D 는 알파 그림을 알파를 곱한 뒤 바꾸고 PQ, HLG 를 더 어둡게 그려, 그 둘은 Canvas2D 에서 다르다. (엔진은 한 픽셀 묶음을 두 길에 같이 써 WebGL 길에 맞췄다) CMYK JPEG, BMP 와 ICO 안의 프로필, prem AVIF 의 프로필, mini 상자 AVIF 는 다루지 않는다.
  - 움직이는 AVIF 는 첫 장만 푼다. 원본 그림은 HTMLImageElement 이고, 크롬은 그것을 drawImage 와 texImage2D 로 그릴 때 늘 첫 장을 쓴다(bitmap_image.cc ImageForDefaultFrame, image_extractor.cc 의 DecodeFrameBufferAtIndex(0), 문서에 붙여 화면에서 움직이는 동안에도 같다). 어느 첫 장인지는 크롬 CrabbyAvif 처럼 주 상표 avis 면 트랙, avif 면 주 그림 항목, 그 밖은 트랙이 있으면 트랙이다.
  - 데스크탑 Canvas2D 의 그림자 흐림은 상자 흐림 세 번이다. 경로의 arc, ellipse, arcTo, roundRect 는 크롬과 같은 스키아 원뿔 곡선 점과 무게이고(블링크 canvas_path.cc, 스키아 SkPathBuilder 의 셈), 칠할 때는 스키아 CPU 래스터처럼 기기 픽셀 0.25 안의 2 차 곡선으로 나눈다. 크롬이 GPU 로 칠하는 캔버스의 곡선 나누기와는 가장자리 픽셀이 다를 수 있다. font 는 블링크 Font::ParseShorthand 와 font 게터대로 읽고 직렬화하는데(desktop_font.rs), 윈도우 크롬의 small-caption, menu, status-bar 는 운영체제 글꼴이라 다를 수 있다. getImageData 의 프리멀티플라이 풀기는 크롬 CPU 캔버스의 스키아 셈이라 크롬 GPU 캔버스의 되읽기와는 반이 딱 떨어지는 값에서 1 다를 수 있다.
  - 데스크탑 PaneStyle 은 프리셋과 원본 도구가 쓰는 CSS(블록과 flex 배치, flex 비율과 세로 flex, min, max, 쪽마다 여백과 테두리, overflow 자르기와 휠, 썸 끌기 스크롤)를 읽는다. flex-wrap 줄넘김과 말줄임(white-space, text-overflow)은 읽지 않고, 스크롤 막대는 자리를 차지하지 않는 얹는 썸이다. (여러 줄 입력 필드도 같다) textarea 는 엔진 여러 줄 입력 필드가 받는다.
  - 데스크탑 질문 창과 브라우저 문맥 메뉴의 치수와 색은 크롬 main 의 값(MessageBoxView 폭 400, 여백 20, 제목 16px, 기본 밝은 팔레트 같은 것)이고, 흐린 그림자는 겹친 둥근 사각형으로 갈음한다. 단추와 입력 칸 높이는 크롬 셈을 기본 글꼴로 갈음한 값이다.
  - 직접 누르는 입력 필드는 데스크탑에서 크롬 데스크탑 터치처럼 손가락으로 650ms 길게 누르면(15 DIP 안) 낱말을 고르고 손잡이와 빠른 메뉴(잘라내기, 복사, 붙여넣기, ...)를 띄운다. 고른 것도 붙여 넣을 것도 없으면 크롬 편집 칸 메뉴다. (윈도우는 뗄 때 정한다) 손잡이 그림의 자리(크롬 그림 창의 6 여백 다루기)와 빠른 메뉴의 흐린 그림자는 갈음한 값이다. 크롬 안드로이드의 떠 있는 빠른 메뉴(안드로이드 ActionMode)는 데스크탑 대상이 없어 넣지 않았다.

## 10. 두 엔진의 차이

원본(vanilla.js)과 러스트 판은 기능 차이를 두지 않는다. 남은 차이는 언어나 플랫폼 때문에 정말 맞출 수 없는 것과, 러스트에 맞게 일부러 정한 꼴뿐이다.

**정말 맞출 수 없는 것**

| 차이 | 까닭 |
| --- | --- |
| 반사(UIDocument 의 반사 저장과 읽기, DevTools 인스펙터의 게터 목록)는 #[vanilla::reflect] 를 붙인 타입만 된다. 엔진 컴포넌트는 모두 붙였고, 게임 컴포넌트는 impl 블록과 impl Component 블록에 붙인다 (원본은 모든 객체). 빠뜨린 것은 알 수 있다: 한쪽만 붙이면 컴파일 에러가 붙일 곳을 알려 주고, 둘 다 빠뜨리면 UIDocument 가 쓸 때 경고를 내고 인스펙터가 "반사 없음" 줄을 보인다 | 러스트에는 실행 중에 필드와 메서드를 훑는 길이 없다. C++ 엔진(언리얼 UCLASS 와 UHT, Qt moc)처럼 컴파일 때 절차 매크로가 함수 목록과 부르는 코드를 짓는다 |
| 직접 누르는 입력 필드의 크롬 편집 칸 메뉴에 크롬 자체 기능 항목(읽기 모드로 열기, 맞춤법 검사와 언어 설정, 선택이 있을 때의 검색, 인쇄, 번역, 맥의 찾기와 음성, 비밀번호와 Gemini 항목)이 없다. 그 밖의 항목(그림 이모티콘, 실행 취소, 다시 실행, 잘라내기, 복사, 붙여넣기, 일반 텍스트로 붙여넣기, 전체 선택, 쓰기 방향, 검사)은 같은 차례와 단축키 글로 같은 일을 한다 | 그 항목들은 페이지가 아니라 크롬 브라우저(옆 패널, 맞춤법 사전 설정, 검색 엔진, 인쇄 대화 상자, 번역, 계정)와 맥 서비스를 부른다. 데스크탑 앱에는 그 브라우저가 없다. (입력 필드는 spellcheck=false 라 맞춤법 설정이 페이지에 보이는 것도 없다) |
| 데스크탑에서 뜻이 없는 set_no_cache | 데스크탑에는 브라우저 캐시가 없다 (파일을 늘 그 자리에서 읽는다) |
| 웨이랜드에서 xdg_toplevel 의 suspended 상태는 받지 못한다. 가려짐(페이지 이벤트 Hidden)은 크롬의 다른 갈래인 프레임 콜백 3 초 규칙으로만 본다 | xdg_toplevel 은 winit(sctk)이 리스너를 달아 쥐고 있고 libwayland 는 프록시 하나에 리스너 하나만 단다. winit 0.30.13 은 configure 의 suspended 를 받아 두기만 하고 알리지 않는다 (0.31 베타 4 와 본줄기도 같다). suspended 를 보내면서 프레임 콜백도 계속 보내는 컴포지터에서만 크롬과 달라진다 |
| DevTools 메모리 칸은 자바스크립트 힙 대신 시스템 할당자의 힙이고, jsHeapSizeLimit 는 (N/A) 다 | 데스크탑에는 V8 힙이 없다. 시스템 할당자에는 V8 처럼 정한 힙 상한이 없다 |
| 윈도우와 리눅스 데스크탑의 글자 래스터 픽셀이 크롬과 다르다 (글자 너비는 크롬 measureText 와 맞췄다. 맥은 같은 장면의 글자 픽셀이 크롬과 같다) | 맥은 크롬처럼 CoreText 로 스키아 SkScalerContext_Mac 과 같은 설정으로 칠한다. 윈도우(크롬은 DirectWrite)와 리눅스(크롬은 FreeType)는 아직 fontdue 와 skrifa 가 칠하고 서브픽셀 자리와 감마 전보정이 없다. 같은 길(SkScalerContext_DW, SkScalerContext_FreeType 의 설정)로 옮길 일이 남았다 |
| 맥 데스크탑의 도형 가장자리 일부 화소가 크롬과 표본 하나(덮인 정도 1/4)만큼 다르다 (둘 다 4 표본 MSAA 이고 안쪽 색과 가장자리 대부분은 같다) | 같은 회전 격자 표본이 맥 OpenGL(아래가 원점)에서는 크롬 ANGLE Metal 과 위아래로 뒤집혀 놓인다. 엔진이 화면을 위아래로 뒤집어 그리고 내보낼 때 되뒤집으면 같아지지만, 기본 프레임버퍼 좌표를 쓰는 곳(화면 복사, 자르기, 3D 패스, 찍기)을 모두 바꿔야 해 아직 하지 않았다 |
| 엔진을 만든 뒤 run 이 창을 열기 전에는 그래픽이 없다 (engine.get_graphic() 이 None, 원본은 Engine 생성자에서 Graphic 을 만든다). run 뒤에는 씬의 create 부터 늘 있다 | GL 컨텍스트는 창(표면)이 있어야 만든다. winit 은 이벤트 루프가 돈 뒤(resumed)에야 창을 만들 수 있다 |
| 데스크탑 Opus 와 AAC 의 표본 끝자리가 크롬과 1.5e-7 안팎으로 다르다 (길이와 다듬기는 같다) | 크롬은 Opus 를 libopus(C), AAC 를 FFmpeg(LGPL)로 푼다. 데스크탑은 C 빌드 없이 맥에서 윈도우 대상 검사가 되도록 libopus 를 러스트로 옮긴 opus-pure 로, AAC 는 LGPL 코드를 옮기지 않고 symphonia 와 oxideav-aac 로 푼다 |
| 데스크탑 3 채널 이상 AAC 의 PNS(잡음 대치) 띠는 잡음 표본이 크롬과 다르다 (띠의 에너지는 같다) | 명세는 PNS 난수 생성기를 정하지 않는다. 크롬(FFmpeg)과 같은 잡음 열을 내려면 FFmpeg 의 생성기와 정규화를 옮겨야 한다. (2 채널 이하는 symphonia 로 풀어 크롬과 같다) |

**크롬과 결과가 같아 차이가 아닌 것** (크로미엄 소스로 확인한 것)

- html lang: 원본은 LanguagePack 이 documentElement.lang 을 바꾸지만, 글을 굽고 재는 캔버스는 모두 문서에 붙이지 않은 canvas 요소라 그 글꼴 locale 은 html lang 이 아니라 크롬 화면 언어다. (Canvas2D 의 lang 기본값 "inherit" 는 캔버스 요소의 ComputeInheritedLanguage 를 쓰는데 떨어진 요소는 조상이 없어 LayoutLocale::GetDefault, 곧 크롬 화면 언어다. canvas_rendering_context_2d.cc, html_canvas_element.cc) 데스크탑도 화면 언어로 글꼴을 고르고, html lang 자리는 Document::get_language, set_language 로 읽고 쓴다. 화면 읽기 프로그램과 번역 제안은 브라우저의 일이다.
- 세이프 에어리어 인셋 0: 크롬 데스크탑도 env(safe-area-inset-*) 가 0 이다. 0 이 아닌 값은 WebContentsImpl::SetDisplayCutoutSafeArea 로만 들어오는데 이 함수는 `#if BUILDFLAG(IS_ANDROID)` 안에만 있고 부르는 곳도 안드로이드(web_contents_android.cc)뿐이다. 그 밖에는 블링크 기본값 "0px" 이다 (style_environment_variables.cc kSafeAreaInsetDefault). 노치가 있는 맥북의 전체 화면도 같다.
- 화면 키보드: 크롬 윈도우는 손가락이나 펜으로 글자 입력칸에 포커스하면 InputPane 으로 터치 키보드를 띄우고, 가린 영역의 세로만큼 visualViewport 를 줄인다 (virtual_keyboard_controller_win.cc, on_screen_keyboard_display_manager_input_pane.cc). 데스크탑 윈도우도 같은 InputPane 으로 띄우고 같은 인셋을 쓴다 (desktop/windowsinputpane.rs). auto_resize_on_window_resize 면 캔버스가 줄고 ignore_virtual_keyboard_resize 가 원본처럼 그것을 막는다. 크롬 맥은 화면 키보드를 띄우지 않고 보이는 뷰포트가 창 크기 그대로다 (render_widget_host_view_base.cc). 크롬 리눅스 웨이랜드는 text-input 으로 입력판을 띄우기만 하고 영역을 받지 않아 뷰포트를 줄이지 않는다 (wayland_input_method_context.cc OnInputPanelState). 데스크탑도 winit 의 IME 켜기(text-input-v3 enable)로 같은 신호를 낸다.
- 웨이랜드 가려짐: 크롬은 suspended 이거나 보낸 wl_surface.frame 콜백이 3 초 동안 오지 않으면 창을 가려짐으로 보고 페이지를 hidden 으로 만든다 (wayland_toplevel_window.cc MaybeUpdateOcclusionState, wayland_frame_manager.cc kFrameCallbackOcclusionTimeout, kWaylandWindowOcclusion 기본 켬). 데스크탑도 winit 창의 표면에 손님 연결로 프레임 콜백을 보내 같은 3 초 규칙으로 Hidden 과 Visible 을 낸다 (위 표의 suspended 만 빼고).
- preserve_drawing_buffer: 원본은 엔진이 프레임마다 색을 비우지 않고(applySettings 는 스텐실만), 꺼져 있으면 브라우저가 드로잉 버퍼를 비우고, 켜져 있으면 지난 프레임 그림이 남아 화면을 다 덮지 않는 씬은 쌓아 그린다. 데스크탑은 꺼져 있으면 프레임 첫머리에 비우고, 켜져 있으면 비우지 않고 지난 프레임 그림을 되살린다 (뒷 버퍼는 넘긴 뒤 내용이 정해져 있지 않아서). 검증용 화면 저장은 원본처럼 프레임 끝, 넘기기 전에 읽는다. 다른 캔버스가 이 캔버스를 읽는 쓰임은 페이지의 일이라 데스크탑에는 없다.
- auto_resize_on_window_resize: 데스크탑 창은 캔버스가 창을 가득 채운 페이지(캔버스 CSS 100%)와 같다고 본다. 켜면 원본처럼 visualViewport(창에서 화면 키보드가 가린 아래를 뺀 것)에 맞추고 캔버스 비 상한을 건다. 끄면 원본처럼 페이지가 정한 캔버스 크기(창 크기)를 쓰고 화면 키보드에 줄지 않으며 캔버스 비 상한도 걸지 않는다.
- set_prevented_keys 와 오른쪽 클릭 메뉴 막기: 크롬에서 막는 것은 브라우저 기본 동작(Tab 의 포커스 옮기기, Space 와 화살표의 스크롤, F5 새로 고침, 브라우저 문맥 메뉴)이다. 데스크탑 창에는 그런 기본 동작이 없어 막은 것과 결과가 같다. (winit 은 윈도우에서 메뉴가 없는 창의 WM_SYSKEYUP 을 DefWindowProc 에 넘기지 않아 Alt, F10 이 시스템 메뉴로 들어가지도 않는다)
- 브라우저 이름: 원본은 Platform 의 브라우저 이름을 개발자 도구와 통계 글에 보이기만 한다. 데스크탑 판은 크롬 위의 원본과 같은 결과를 내도록 맞췄으므로 브라우저는 Chrome 이고, 게임 코드의 브라우저 분기도 크롬 갈래로 간다.

**일부러 정한 꼴**

| 차이 | 까닭 |
| --- | --- |
| 전역 엔진이 없다 (Engine.getEngine, Scene.getEngine 대신 씬 함수와 ComponentContext 로 넘긴다. 그래서 인스펙터 게터 목록에 원본 Component.getEngine 이, 반사 속성 목록에 Engine 이 없다. 엔진을 받지 않는 컴포넌트 훅(attach, detach, attach_to_tree)은 그래픽에도 닿지 않아, GL 자원은 컴포넌트를 짓는 자리나 tick 의 context.get_engine().get_graphic() 으로 짓는다. 원본 엔진 컴포넌트에도 attach 에서 그래픽을 쓰는 것은 없다) | 전역 가변 상태는 빌림 검사를 비켜 가서, 엔진을 쓰는 곳이 인자로 드러나게 했다 |
| 노드는 NodeTree 안에 있고 NodeId 핸들로 가리킨다 | 부모와 자식을 Rc<RefCell> 로 서로 가리키면 순환 참조와 실행 중 빌림 검사가 생긴다. 지운 노드의 핸들은 None 이 된다 |
| 노드 종류는 WorldNode 하나다 (원본 Node, ComponentNode, TransformNode, WorldNode 를 합침, 원본에서 노드인 위젯과 UINode 는 컴포넌트) | 러스트에 클래스 상속이 없어 노드 꼴을 하나로 두고 기능을 컴포넌트로 붙였다 |
| 상속 대신 트레이트와 품기 (Scene + SceneBase, Component + ComponentBase, ShaderSprite 는 Sprite 를 품고 Deref) | 러스트에 클래스 상속이 없어 공통 상태는 구조체로, 덮어쓰기는 트레이트 함수로 둔다 |
| 반사는 원본 상위 클래스 몫을 품은 필드로 잇는다 (#[reflect(base = 필드)], 결과 목록과 차례는 원본과 같다). &mut self 게터는 &mut 로 트리를 받는 갈래(UIDocument::to_json_text_mut, serialize_node_mut, 인스펙터는 tick 에서 읽어 둔다)로 읽는다. &NodeTree 만 받는 to_json_text 는 &mut self 게터를 읽지 못한다 | 상속이 없다. &self 로 &mut self 함수를 부를 수 없다 (엔진 컴포넌트의 저장 속성에는 &mut self 게터가 없어 두 갈래 결과가 같다) |
| 그리기 함수와 with_graphic 의 클로저 안에서는 engine.get_graphic() 이 None 이고 인자로 받은 graphic 을 쓴다 (원본 getGraphic 은 어디서나 같은 그래픽) | &mut Engine 과 &mut Graphic 을 함께 넘기려면 그동안 그래픽이 엔진 밖에 있어야 한다. 엔진이 그래픽을 Option::take 로 잠깐 꺼내 넘기고 돌려 넣어 RefCell 이나 unsafe 없이 이중 빌림을 막는다 (3 장 그래픽 빌림) |
| 수는 f32 다 (Format, LocalStorage 처럼 JS 수의 글을 그대로 내야 하는 곳은 f64) | GL 에 올리는 값과 같은 정밀도로 들고 메모리를 아낀다. 원본 Matrix4 는 f32 로 담고 f64 로 셈해 끝자리가 조금 다를 수 있다 |
| GL 을 쓰는 객체는 컨텍스트를 들지 않고 함수마다 gl 을 받는다 (FullscreenPass, RenderTarget, 툰 렌더러 같은 것) | 컨텍스트는 Graphic 이 들고, 객체마다 나눠 쥐면 수명과 빌림이 엮인다 |
| 트리를 받는 `*_in_tree` 함수를 따로 둔 곳 15 개 (UIScrollView::set_shows_vertical_scroll_bar_in_tree, UIWindow::close_in_tree, ParticleSystem::emit_in_tree 같은 것) | 씬 코드가 컴포넌트를 쥔 동안 같은 트리의 다른 노드를 함께 빌릴 수 없다 |
| 선택 인자 객체 대신 기본값을 가진 옵션 구조체 (ToonDrawOptions, ToonParticleOptions, RenderTargetOptions, ToonPostProcessOptions, UIConfirmWindowOptions 같은 것) | 러스트에는 이름 붙은 선택 인자가 없다. 필드를 더해도 `..Default::default()` 로 지은 코드는 그대로 돈다 |
| 콜백은 Box<dyn FnMut> 로 받고, 콜백 게터와 갈아 끼운 저장소는 Rc<RefCell> 로 돌려준다 | 엔진과 부르는 쪽이 같은 함수를 나눠 쥐고 부르는 동안 다시 부르는 것을 막는다 |
| 예외 대신 Result 와 Option, 셰이더를 짓지 못하면 한 번 알리고 건너뛴다 | 러스트는 예외를 던지지 않는다. 한 효과가 실패해도 프레임이 멈추지 않게 했다 |
| 이름은 러스트 규칙(snake_case, UpperCamelCase, SCREAMING_SNAKE_CASE)이다 | Rust API Guidelines C-CASE 를 따른다 |
| async 작업은 컨텍스트의 with(\|씬, 엔진\| ...) 안에서만 씬과 엔진을 만진다 (원본은 await 다음 줄에서 this 와 엔진을 바로 쓴다) | 전역 엔진이 없고 노드를 NodeId 로 가리키는 것과 같은 러스트 빌림 꼴이다. 러스트 퓨처는 &mut 를 await 너머로 쥘 수 없어, 폴링하는 동안만 빌려 주어 이중 빌림이 생기지 않게 했다 |
| 원본 reject 는 Promise<Result<T, E>> 의 reject(까닭)이고, 실패할 수 있는 손잡이의 await 는 Result 로 끝난다 | 예외 대신 Result 를 쓰는 것과 같다 |
