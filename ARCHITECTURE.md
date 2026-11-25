# 바닐라엔진 구조 설계 문서

vanilla.js 를 러스트로 옮긴 엔진의 구성과 소유 구조를 정리한 문서다. "무엇이 무엇을 만들고, 들고 있고, 어떤 순서로 굴러가는가"를 본다. 원본 구조는 vanilla.js 의 ARCHITECTURE.md 에 있고, 여기서는 러스트 판에서 달라진 것을 중심으로 적는다.

- 기준: 2026-11-26 (dev)
- 이름: 바닐라엔진(Vanilla Engine). 저장소와 크레이트 이름은 `vanilla-engine`, 코드에서는 `vanilla` (`use vanilla::...`)

## 1. 전체 배치

```
src/lib.rs           공개 API 를 다시 내보낸다 (원본 import.js 자리)
src/animation/       Timeline, Animator, Action (원본 experimental/animation, experimental/action)
src/base/            엔진을 모르는 기반 타입 (수학, 색, 식별자, 플랫폼 정보, Format, FiniteStateMachine, ObjectPool, SeededRandom, Timer, Version)
src/core/            엔진 본체 (Engine, 매니저, Scene, 노드 트리, Graphic, Component, 애셋 꼴, 트윈, Animation, Frame)
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
                     TouchRecognizer, UIInputField(데스크탑 winit IME, 웹 숨긴 DOM input, 클립보드 arboard), UIListView, UISnapScrollView, UIDropdown,
                     UIDialog, UIToast, UISpinner, UIContextMenu, UIChart, UIDraggable, UIHitArea, PopupMotion, ButtonMotion, ToastMotion, UIScale,
                     UIWindow, UIWindowLayer, UIConfirmWindow, UIDocument, UINode, UIScene(UISceneBase), autolayout(LayoutSolver 등)
src/game/            GameSceneBase, GameScene 트레이트와 game_scene! (원본 GameScene: 로딩 화면, 터치 라우팅, 세이프 에어리어, 크기 바뀌면 다시 놓기, 페이드, 놀이 입력, 개발자 도구)
src/web/             WebSocketClient (ewebsock, 데스크탑과 웹), Pane (DOM 화면 분할 패널, 웹 전용)
src/platform/        창, GL 컨텍스트, 입력 이벤트, 프레임 루프, 화면 넘기기, 애셋 바이트 읽기, 오디오 백엔드
  desktop.rs         winit 0.30 + glutin 0.32 (OpenGL 3.3 코어)
  desktop/           renderscaler(렌더 배율 상한), textinput(IME, 클립보드), systemfont(운영체제 글꼴과 대체 글꼴), coretextglyph(맥 CoreText 글리프),
                     gamepadhaptics(맥 패드 진동), windowsplatform(윈도우 잠자기 깸, 손가락 판정), linuxplatform(리눅스 손가락 판정)
  web.rs             캔버스 + WebGL2 + request_animation_frame + fetch
  web/               systemfont(브라우저 Canvas2D 로 글자 굽기와 재기), textinput(숨긴 DOM input, 클립보드)
  framelimiter.rs    웹 최대 FPS 의 프레임 세기
  audio/             데스크탑 kira 0.12, 웹 WebAudio
    decode/          데스크탑 소리 해석 (크롬 decodeAudioData 와 같게: symphonia 0.6, Opus 는 opus-pure, 앞뒤 다듬기, SincResampler, 내리기)
```

레이어 방향은 원본처럼 아래에서 위로만 의존한다: base ← core ← (resource, effect, misc). platform 은 core 밑에서 창과 GL 과 입력을 대 준다. 엔진 본체(core::engine::EngineRunner)는 플랫폼을 모른다.

## 2. 설계 원칙 (원본과 다른 것)

1. **노드는 핸들로 가리킨다.** 원본은 부모가 children[] 으로 자식 객체를 들고 자식이 parent 로 거꾸로 가리킨다. 러스트 판은 씬의 NodeTree 가 노드를 slotmap 저장소 하나에 담고, 부모와 자식을 NodeId 로 잇는다. NodeId 는 칸 번호와 세대 번호라서 지운 노드의 핸들은 get_node 가 None 을 돌려준다(원본 isDestroyed 의 공백을 메움). Rc<RefCell> 은 쓰지 않는다.
2. **전역 엔진이 없다.** 원본의 Engine.getEngine() 대신 씬 함수와 컴포넌트 문맥으로 Engine 을 넘긴다.
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
│   ├── platform_window (창 손잡이: 데스크탑 winit Window, 웹 캔버스)
│   ├── pending_scene_list, pending_font_registration_list, 페이지 이벤트 리스너
├── Graphic (플랫폼이 GL 컨텍스트를 만든 뒤 attach_graphic 으로 넘김)
│   ├── ShaderProgram (기본), 묶음, 파티클, 블렌드 섞기 셰이더, 바꿔 낄 프로그램 표
│   ├── ImageTextureCache
│   ├── TextStringTextureCache (데스크탑: rustybuzz 모양 짓기, fontdue 와 skrifa 래스터, 색 글리프 colorglyph, 맥 색 이모지는 CoreText,
│   │                           시스템 글꼴 사슬과 운영체제 대체 글꼴 / 웹: 브라우저 Canvas2D)
│   └── 묶음 버퍼, 클립 스택, 블렌드 섞기용 바탕 칸
└── Scene[] (Box<dyn Scene>)
    └── SceneBase
        ├── NodeTree ── slotmap<NodeId, WorldNode>, 루트 노드
        │   └── WorldNode ── NodeTransform, Vec<Box<dyn Component>>, parent_id, child_ids
        ├── Tween[] (TweenId 로 가리킴)
        ├── 맡긴 애셋 목록 (다 읽히고 0.3초 뒤 initialize)
        └── 눌린 노드, 첫 터치 콜백
```

애셋은 원본처럼 중앙 매니저 없이 씬이 멤버로 든다. 같은 그림을 여러 번 그려도 GPU 텍스처는 Graphic 의 캐시가 하나만 만든다. 여럿을 함께 받을 때는 AssetLoader 가 동시 수만큼 요청하고 진행률을 센다.

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
2. graphic.apply_settings()          뷰포트, 프로젝션, 상태 리셋, 화면 비우기
3. time_manager.update(), input_manager.tick() (게임패드 폴링, 입력 동작 갱신 포함)
4. 씬마다:
   - 맡긴 애셋을 기다리는 씬: 다 읽히고 0.3초가 지났으면 initialize 와 resize, 아니면 draw_on_load 만
   - scene.tick → SceneBase.tick: 노드 tick(컴포넌트 → 자식) → 휠, 터치 전달 → 트윈
   - 씬의 터치 훅(touch_press, 뗌 또는 누르고 있음 또는 취소, touch_wheel, 원본 Scene.tickTouch 차례)
   - pre_draw → draw(NodeTree.draw 재귀) → post_draw → (기즈모)
5. 통계 출력(옵션: FPS, DrawCall, Vertex, Culled) → 마우스 커서(PointerCursor) → 1회성 입력 플래그 지우기
6. graphic.end_frame() (묶음 내보내기) → 검증용 화면 저장(request_frame_capture, 데스크탑 VANILLA_CAPTURE_*, 웹 ?capture=) → 프레임 번호 증가 (첫 프레임 1)
7. 씬 목록 바꾸기 (apply_pending_scenes: 불러온 씬 create, 내린 씬 finalize, unload, destroy)
플랫폼: 데스크탑은 렌더 배율 상한이나 캔버스 비 상한으로 줄여 그렸으면 창의 캔버스 자리로 늘이고(desktop/renderscaler.rs), 화면 넘기기 (데스크탑 swap_buffers, 웹은 콜백이 끝나면 브라우저가)
```

씬 불러오기는 원본의 create → load(비동기) → set_loaded → initialize 를 create → (맡긴 애셋을 기다림) → initialize 로 옮겼다. create 바로 뒤에 루트를 원본 Scene.load 처럼 뷰 크기로 맞추고, 맡긴 애셋이 다 끝난 뒤에도 원본 SceneManager.loadScene 처럼 0.3초 더 로딩 화면을 보인다. 데스크탑은 애셋이 그 자리에서 읽혀서 create 뒤 0.3초에 initialize 가 불린다.

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

## 7. 애셋

- `ImageAsset`, `TextAsset`, `JsonAsset`, `BlobAsset`, `FontAsset`: `load(engine, 경로)` 로 요청하고 `scene_base.track_asset(&asset)` 으로 맡긴다. 데스크탑은 애셋 폴더 후보에서 파일을 읽고, 웹은 `asset_base_url + 경로` 를 fetch 한다.
- 실패한 애셋도 "끝난 것"으로 쳐서 씬이 멈추지 않는다. 풀지 못한 JSON 은 원본처럼 읽히지 않은 것이다.
- AssetLoader 는 여러 애셋을 동시 수(기본 6)만큼 받고 진행률과 실패 목록을 센다. 다 받은 그림은 원본 img.onload 처럼 풀어 보고, 깨진 것은 실패로 친다. 푼 그림은 읽기 손잡이에 담겨 ImageAsset 이 다시 풀지 않는다.
- `AudioAsset` 은 오디오 백엔드와 붙어 있어 `engine.load_audio_asset` 으로 읽고 `track_audio_asset` 으로 맡긴다.
- 그림: 데스크탑은 png, jpeg, webp, gif(첫 장), bmp, ico 를 image 크레이트로, SVG 를 resvg 로, AVIF 를 엔진의 HEIF 상자 읽기와 rav1d 로 푼다. 웹은 원본처럼 브라우저(HTMLImageElement 의 decode)가 푼다. 그림의 형은 원본처럼 응답의 content-type 이다. (SVG 는 형으로, 래스터는 내용으로 가린다. 응답이 없는 데스크탑은 내용으로 고른다)
- 소리: 웹은 원본처럼 브라우저 decodeAudioData 가 풀고, 데스크탑은 같은 결과가 나오게 platform/audio/decode 가 푼다. 꼴은 내용으로 가리고, symphonia 0.6 으로 디먹스해 FLAC, MP3, PCM, Vorbis 는 크롬과 같은 symphonia 해석기, Opus 는 opus-pure(libopus 를 옮긴 것), AAC 는 symphonia 의 AAC 로 푼다. 크롬이 FFmpeg 디먹스 값으로 버리는 앞뒤(Opus pre-skip 과 webm CodecDelay, DiscardPadding, MP3 LAME 지연, Vorbis granule, m4a 편집 목록과 iTunSMPB)를 같게 버리고, 정수 표본을 크롬 식으로 실수로 바꾸고, 원본 DECODE_SAMPLE_RATE(44100)로 크로미엄 SincResampler 를 옮겨 바꾸고, WebAudio 스피커 규칙으로 1 채널이나 2 채널로 내린다. `tests/assets/audio` 의 21 파일이 크롬 158 과 길이, 채널, 표본까지 같다. (Opus, AAC 는 1e-7 안팎)
- "data:" 주소: 데스크탑 애셋 읽기(request_asset_bytes, load_image, load_audio_asset)와 타임라인이 base/dataurl.rs 로 푼다. 웹은 기준 주소를 붙이지 않고 브라우저 fetch 에 맡긴다.
- 글꼴: ttf, otf, woff, woff2 를 읽는다. 데스크탑은 woff, woff2 를 wuff 로 풀고, 운영체제 글꼴은 글꼴 모음(TTC)에서 이름이나 굵기로 얼굴을 고른다. 웹은 원본처럼 글꼴 바이트를 document.fonts 의 FontFace 로 올린다.
- 이미지는 GPU 에 올린 뒤 CPU 픽셀을 놓는다. 픽셀을 다시 읽을 그림은 `Image.set_pixels_retained(true)` 로 남긴다. (ShadowImage 는 받은 그림을 스스로 남기고, 이미 놓였으면 텍스처를 프레임버퍼에 붙여 되읽는다) GL 컨텍스트를 잃었다 되살리는 길은 아직 없다.
- 기본 글꼴은 엔진에 넣지 않는다. 둘 다 시스템 글꼴이 기본이다.
  - 데스크탑: `default_font_paths` 가 비었으면(기본) 원본 SYSTEM_FONT_STRING 의 이름 차례로 운영체제 글꼴을 찾아 사슬로 잇는다. 맥은 크롬처럼 -apple-system 을 모르는 이름으로 넘겨 Apple SD Gothic Neo, 윈도우는 Segoe UI 와 맑은 고딕이다. 폰트 문자열의 올리지 않은 이름도 운영체제 글꼴에서 찾고, 일반 이름(serif, sans-serif, monospace 등)은 크롬의 기본 글꼴 표대로 고른다. 글꼴 목록에 없는 글자는 운영체제가 고른 대체 글꼴(맥 CoreText, 윈도우 DirectWrite, 리눅스 fontconfig)로 그린다. (platform/desktop/systemfont.rs)
  - 웹: 원본처럼 늘 브라우저 Canvas2D 로 굽고 잰다. (platform/web/systemfont.rs)
  - 게임 글꼴은 `default_font_url`, `default_font_bytes`, FontAsset 가운데 하나로 준다. (원본 기본값 DOSGothic 은 뺐다)

## 8. 코드 꼴

이름은 러스트 규칙(Rust API Guidelines C-CASE)을 따른다: 함수, 메서드, 변수, 필드, 모듈은 snake_case, 타입과 트레이트와 열거 값은 UpperCamelCase, 상수는 SCREAMING_SNAKE_CASE. vanilla.js 의 이름은 이 규칙으로 바꿔 옮긴다. (setFillColor → set_fill_color, Colors.white → Colors::WHITE) 나머지는 vanilla.js 지침을 따른다: 이름 줄이지 않기, 중괄호 늘 쓰기, else 는 다음 줄, `//====` 머리 주석에 독백체 설명, 파일 머리에 원본과의 대응과 다른 점. rustfmt 는 else 줄바꿈을 망가뜨려서 돌리지 않는다.

## 9. 원본과의 대응

2026-11-26 dev 기준으로 원본 `src/` 의 .js 200 파일 가운데 181 파일을 옮겼고, 19 파일은 일부러 옮기지 않았다. (원본 파일마다 같은 이름의 .rs 가 있는지 보고, 없는 것은 러스트 파일 머리 주석의 원본 경로로 찾아 셌다)

- 같은 이름으로 옮긴 것 175 파일. 폴더는 러스트 판에 맞게 바꾼 곳이 있다. (core/graphic/* 는 core/, experimental/graphics 는 graphics3d/, experimental/animation 과 action.js 는 animation/, experimental/indicator.js 와 visual/visual.js 는 misc/, experimental/bunchasset.js 와 visual/visualasset.js 는 resource/, experimental/graphics/fullscreenpass.js 는 core/)
- 다른 파일에 합친 것 6 파일:
  - base/node.js, core/node/componentnode.js, core/node/transformnode.js: 노드 종류가 하나라 core/node/worldnode.rs 와 nodetree.rs 에 합쳤다.
  - core/scenemanager.js: 엔진이 씬을 들고 있어 core/engine.rs 와 core/scene.rs 에 합쳤다. (load_scene, unload_scene, get_all_loaded_scenes)
  - core/touchraycaster.js: core/scene.rs 의 SceneBase::tick_touch 와 core/node/nodetree.rs 의 누름 판정, ui/touchrecognizer.rs 에 합쳤다.
  - resource/jsonasset.js: resource/textasset.rs 에 TextAsset 과 함께 둔다.
- 일부러 옮기지 않은 것 19 파일:
  - 빈 파일이거나 아무 일도 하지 않는 것 4: base/calculator.js(0 을 돌려주는 빈 함수), core/layer.js(빈 스텁), core/resources.js(0 바이트), misc/uibuilder.js(NodeLayout 으로 대체됐다는 주석 한 줄)
  - 죽은 코드 5: experimental/aabb.js, experimental/transform2d.js(파일 전체가 주석), experimental/graphics/gl2d.js(내보내는 것 없음), experimental/graphics/webgl2.js(import 경로가 깨져 쓰이지 않음), deprecated/anchoredworldmnode.js(폐기, import.js 에서도 막아 둠)
  - 러스트 언어와 표준이 맡는 것 9: base/object.js, base/reflection.js(createGUID 만 base/identifier.rs 의 create_guid 로 옮겼다), base/singleton.js, base/wait.js, experimental/collection 의 dictionary.js, list.js, queue.js, set.js, stack.js. 트레이트, Any, enum, Vec, HashMap 과 엔진 루프가 같은 일을 한다.
  - 다른 기능이 덮는 것 1: experimental/camera.js. 회전과 begin/end 를 Camera2D(misc/camera2d.rs)에 넣어 그 기능을 모두 덮는다.
- 러스트에만 있는 파일은 플랫폼 층(platform/ 아래 14 파일, mod.rs 빼고), 글자 모양 짓기와 색 글리프(core/textshaping.rs, core/colorglyph.rs), 그림 풀기(core/image.rs, core/avifimage.rs), 글 입력 줄(core/textinput.rs), 노드 저장소(core/node/nodetree.rs)다. 원본에서는 브라우저가 하는 일이다.
- 옮기며 바꾼 쓰임새:
  - GameScene, UIScene 의 덮어쓰기는 GameScene, UIScene 트레이트와 game_scene!, ui_scene! 매크로다. 바탕이 resize, layout, post_draw 를 스스로 부른다. (game/gamescene.rs)
  - UINode 는 노드에 붙는 컴포넌트이고, 솔버 값을 노드의 자리와 content_size 로 옮긴다. (ui/uinode.rs, ui/uiscene.rs)
  - UIDocument 는 원본의 리플렉션 직렬화 대신 종류 이름마다 짓기, 쓰기, 읽기 함수를 표에 등록하고, 원본이 반사로 저장하는 종류는 원본 반사 결과와 같은 키로 쓴다. DevTools 인스펙터의 컴포넌트 속성도 UIDocument 가 저장하는 키와 값으로 보인다. (ui/uidocument.rs, misc/devtools.rs)
  - web/Pane(DOM 화면 분할 패널)은 웹(wasm32) 빌드에만 있다. (web/pane.rs)
  - SoundSystem 의 음악과 효과음 버스는 AudioBus 다. (원본에도 같은 꼴로 넣었다) 데스크탑은 kira 하위 트랙, 웹은 GainNode 이고, get_gain_node 는 두 플랫폼 모두 AudioGainNode(웹 GainNode, 데스크탑은 버스 트랙을 나눠 드는 Rc<RefCell<TrackHandle>>)를 돌려준다.
  - Cooldown.use(원본 base/timer.js), FullscreenPass.use, ShaderProgram.use 는 use 가 러스트 예약어라 r#use 로 부른다. activate, use_program 은 같은 일을 하는 이름으로 남긴다.
  - 노드 객체를 돌려주는 getParent, getChildren, getSiblings 는 핸들을 돌려주는 get_parent_id, get_child_ids, get_sibling_ids 다.
  - 갈아 끼운 저장소(LocalStorage.get_backend, PersistedStore.get_storage_backend)와 콜백 게터(UIButton.get_click_event 같은 것)는 Rc<RefCell> 에 담아 돌려주고, 부르는 쪽이 빌려 쓴다. SpriteAnimator.findSprite(형제를 쥐기)와 SharedBakeCache 의 놓기(release)는 빌림 규칙과 Rc 참조 수로 갈음한다.
- 글자는 원본(브라우저 Canvas2D)과 맞췄다. 오른쪽에서 왼쪽 글(양방향 정렬과 글자 모양 짓기, core/textshaping.rs), CSS 같은 글꼴 얼굴 고르기, 가짜 기울임과 굵게(Chromium 과 Skia 의 값), 크롬 캔버스처럼 낱말마다 모양 짓기, 폰트 문자열의 CSS 단위와 calc(), 낱말 나누기(데스크탑은 ICU4X 사전에 ICU4C CjkBreakEngine 의 셈을 옮겨 크롬 Intl.Segmenter 와 같게), 색 이모지(sbix, CBDT, COLR v0/v1, 맥은 CoreText 로 크롬처럼 작은 크기에서 키우고 내림), 맥 시스템 UI 글꼴의 opsz 축과 trak 자간(HarfBuzz 15 처럼).
- 작게 남은 차이:
  - rustybuzz 는 글리프 위치를 글꼴 단위 정수로 들어 GPOS 가변 델타를 반올림한다. HarfBuzz 15 는 소수로 든다. 맥 SFNS 18, 20, 21, 24px 의 "Hello World Wifi" 너비가 크롬과 0.002 ~ 0.004px 다르다.
  - 윈도우와 리눅스의 글꼴 갈래(DirectWrite, 메뉴 글꼴, fontconfig, 부모 글꼴 치수)는 그 대상으로 컴파일해 보았고 실행해 크롬과 견주지는 못했다.
  - 데스크탑 AVIF 는 색 프로필 변환과 움직이는 AVIF 를 하지 않는다.

## 10. 두 엔진의 차이

원본(vanilla.js)과 러스트 판은 기능 차이를 두지 않는다. 남은 차이는 언어나 플랫폼 때문에 정말 맞출 수 없는 것과, 러스트에 맞게 일부러 정한 꼴뿐이다.

**정말 맞출 수 없는 것**

| 차이 | 까닭 |
| --- | --- |
| async/Promise 대신 받기 손잡이와 콜백 (AssetLoadHandle, ScreenFadeHold, read_clipboard_text_with_callback 같은 것) | 엔진이 프레임마다 씬과 컴포넌트를 &mut 로 빌려 부르는 동기 루프라, 그 함수 안에서 await 로 멈춰 기다릴 수 없다 |
| 런타임 리플렉션이 없다 (UIDocument 와 DevTools 는 종류마다 등록한 표) | 러스트는 실행 중에 필드와 메서드를 훑는 길이 없다 |
| 데스크탑에 DOM 이 없다 (Pane 은 웹만, 데스크탑 입력 필드는 winit IME, 저장소 입력은 패널 안 입력 창) | 데스크탑 창에는 HTML 문서가 없다 |
| 데스크탑에서 뜻이 없는 브라우저 설정 (auto_resize_on_window_resize, preserve_drawing_buffer, set_no_cache, 오른쪽 클릭 메뉴 막기, set_prevented_keys) | 데스크탑에는 페이지, 브라우저 캐시, 브라우저 기본 동작이 없고 창은 늘 창 크기에 맞춘다 |
| 데스크탑에 화면 키보드가 없다 (ignore_virtual_keyboard_resize 는 웹만) | 브라우저의 화면 키보드가 줄이는 visualViewport 크기 변화가 데스크탑 창에는 없다 |
| 웨이랜드에서는 페이지 이벤트 Hidden 이 오지 않는다 | winit 0.30 이 웨이랜드에서 창 가림도 최소화도 알리지 않는다 (맥과 X11 은 가림, 윈도우는 최소화로 낸다) |
| 데스크탑 세이프 에어리어 인셋은 0 이다 (강제값 set_safe_area_override 와 VANILLA_SAFE_AREA 는 된다) | CSS env(safe-area-inset-*) 가 없고, winit 0.30 은 세이프 에어리어를 iOS 에서만 읽는다 |
| 데스크탑에는 브라우저 정보가 없다 (Platform 의 브라우저는 unknown, DevTools 메모리는 (N/A)) | 브라우저 위에서 돌지 않는다 |
| 데스크탑의 글자 래스터 픽셀과 도형 가장자리 안티에일리어싱이 크롬과 다르다 (글자 너비는 크롬 measureText 와 맞췄고, 도형 안쪽 색은 같다) | 크롬의 Skia, CoreGraphics 래스터라이저 대신 fontdue, skrifa(맥의 색 이모지와 일부 글꼴은 CoreText)와 GL 이 칠한다 |
| PersistedStore 와 LocalStorage 의 글은 짝 없는 서로게이트를 담지 못한다 | 러스트 String 은 UTF-8 이라 짝 없는 서로게이트를 담을 수 없다 (JS 글은 UTF-16 이라 담는다) |
| 데스크탑 Opus 와 AAC 의 표본 끝자리가 크롬과 1e-7 안팎으로 다르다 (길이와 다듬기는 같다) | 크롬은 Opus 를 libopus(C), AAC 를 FFmpeg(LGPL)로 푼다. 데스크탑은 C 빌드 없이 맥에서 윈도우 대상 검사가 되도록 libopus 를 러스트로 옮긴 opus-pure 로, AAC 는 LGPL 코드를 옮기지 않고 symphonia 로 푼다 |
| 데스크탑은 HE-AAC(SBR, PS)와 3 채널 이상 AAC 를 풀지 못한다 | symphonia 0.6.1 의 AAC 는 AAC-LC 2 채널까지만 받는다 ("aac too complex"). 크롬은 FFmpeg 로 푼다 |

**일부러 정한 꼴**

| 차이 | 까닭 |
| --- | --- |
| 전역 엔진이 없다 (Engine.getEngine, Scene.getEngine 대신 씬 함수와 ComponentContext 로 넘긴다) | 전역 가변 상태는 빌림 검사를 비켜 가서, 엔진을 쓰는 곳이 인자로 드러나게 했다 |
| 노드는 NodeTree 안에 있고 NodeId 핸들로 가리킨다 | 부모와 자식을 Rc<RefCell> 로 서로 가리키면 순환 참조와 실행 중 빌림 검사가 생긴다. 지운 노드의 핸들은 None 이 된다 |
| 노드 종류는 WorldNode 하나다 (원본 Node, ComponentNode, TransformNode, WorldNode 를 합침, 원본에서 노드인 위젯과 UINode 는 컴포넌트) | 러스트에 클래스 상속이 없어 노드 꼴을 하나로 두고 기능을 컴포넌트로 붙였다 |
| 상속 대신 트레이트와 품기 (Scene + SceneBase, Component + ComponentBase, ShaderSprite 는 Sprite 를 품고 Deref) | 러스트에 클래스 상속이 없어 공통 상태는 구조체로, 덮어쓰기는 트레이트 함수로 둔다 |
| 수는 f32 다 (Format, LocalStorage 처럼 JS 수의 글을 그대로 내야 하는 곳은 f64) | GL 에 올리는 값과 같은 정밀도로 들고 메모리를 아낀다. 원본 Matrix4 는 f32 로 담고 f64 로 셈해 끝자리가 조금 다를 수 있다 |
| GL 을 쓰는 객체는 컨텍스트를 들지 않고 함수마다 gl 을 받는다 (FullscreenPass, RenderTarget, 툰 렌더러 같은 것) | 컨텍스트는 Graphic 이 들고, 객체마다 나눠 쥐면 수명과 빌림이 엮인다 |
| 트리를 받는 `*_in_tree` 함수를 따로 둔 곳 15 개 (UIScrollView::set_shows_vertical_scroll_bar_in_tree, UIWindow::close_in_tree, ParticleSystem::emit_in_tree 같은 것) | 씬 코드가 컴포넌트를 쥔 동안 같은 트리의 다른 노드를 함께 빌릴 수 없다 |
| 선택 인자 객체 대신 기본값을 가진 옵션 구조체 (ToonDrawOptions, ToonParticleOptions, RenderTargetOptions, ToonPostProcessOptions, UIConfirmWindowOptions 같은 것) | 러스트에는 이름 붙은 선택 인자가 없다. 필드를 더해도 `..Default::default()` 로 지은 코드는 그대로 돈다 |
| 콜백은 Box<dyn FnMut> 로 받고, 콜백 게터와 갈아 끼운 저장소는 Rc<RefCell> 로 돌려준다 | 엔진과 부르는 쪽이 같은 함수를 나눠 쥐고 부르는 동안 다시 부르는 것을 막는다 |
| 예외 대신 Result 와 Option, 셰이더를 짓지 못하면 한 번 알리고 건너뛴다 | 러스트는 예외를 던지지 않는다. 한 효과가 실패해도 프레임이 멈추지 않게 했다 |
| 이름은 러스트 규칙(snake_case, UpperCamelCase, SCREAMING_SNAKE_CASE)이다 | Rust API Guidelines C-CASE 를 따른다 |
