# 바닐라엔진 구조 설계 문서

vanilla.js 를 러스트로 옮긴 엔진의 구성과 소유 구조를 정리한 문서다. "무엇이 무엇을 만들고, 들고 있고, 어떤 순서로 굴러가는가"를 본다. 원본 구조는 vanilla.js 의 ARCHITECTURE.md 에 있고, 여기서는 러스트 판에서 달라진 것을 중심으로 적는다.

- 기준: 2026-11-18
- 이름: 바닐라엔진(Vanilla Engine). 저장소와 크레이트 이름은 `vanilla-engine`, 코드에서는 `vanilla` (`use vanilla::...`)

## 1. 전체 배치

```
src/lib.rs           공개 API 를 다시 내보낸다 (원본 import.js 자리)
src/animation/       Timeline, Animator, Action (원본 experimental/animation, experimental/action)
src/base/            엔진을 모르는 기반 타입 (수학, 색, 식별자, 플랫폼 정보)
src/core/            엔진 본체 (Engine, 매니저, Scene, 노드 트리, Graphic, Component, 애셋 꼴, 트윈, 오디오, 게임패드)
src/core/node/       NodeTree(노드 저장소), WorldNode
src/core/component/  Paint, Sprite, Text, RichText, Mask
src/resource/        애셋 (이미지, 글, JSON, 바이너리, 글꼴, 오디오)
src/effect/          ParticleSystem, ShaderSprite, ScreenEffect, TrailRenderer
src/misc/            BeepPlayer, Camera2D, Shaker, Grid, PathFinder, Collision2D, Steering2D, NodeLayout, LocalStorage, PersistedStore,
                     FloatingText, VirtualPad, PlatformerBody, SpriteAnimator, Localization, Typewriter, Dialogue, FocusNavigator,
                     PointerGesture, ImageScroller, TouchEffect, DevTools(F2 하이어라키/인스펙터)
src/graphics3d/      Material, SkinnedModel(glTF/GLB), FbxLoader, SkinnedModelRenderer(스키닝 PBR), ShadowMap, RenderTarget, BloomEffect,
                     AmbientOcclusionEffect, SubsurfaceScatteringEffect (FullscreenPass 는 core)
src/ui/              UIView, UIControl, UILabel, UIButton, UIToggleButton, UISlider, UIProgressView, UIImageView, UIScrollView, UIScrollBar, TouchRecognizer, UIInputField(데스크탑 winit IME, 웹 숨긴 DOM input, 클립보드 arboard),
                     UIListView, UISnapScrollView, UIDropdown, UIDialog, UIToast, PopupMotion, UIDraggable, UISpinner, UIContextMenu, UIChart,
                     UIWindow, UIWindowLayer, UIConfirmWindow, UIDocument, UINode, UIScene(UISceneBase), autolayout(LayoutSolver 등)
src/game/            GameSceneBase, GameScene 트레이트와 game_scene! (원본 GameScene: 로딩 화면, 터치 라우팅, 세이프 에어리어, 크기 바뀌면 다시 놓기, 페이드, 놀이 입력, 개발자 도구)
src/web/             WebSocketClient (ewebsock, 데스크탑과 웹), Pane (DOM 화면 분할 패널, 웹 전용)
src/platform/        창, GL 컨텍스트, 입력 이벤트, 프레임 루프, 화면 넘기기, 애셋 바이트 읽기, 오디오 백엔드
  desktop.rs         winit 0.30 + glutin 0.32 (OpenGL 3.3 코어)
  web.rs             캔버스 + WebGL2 + request_animation_frame + fetch
  audio/             데스크탑 kira 0.12, 웹 WebAudio
```

레이어 방향은 원본처럼 아래에서 위로만 의존한다: base ← core ← (resource, effect, misc). platform 은 core 밑에서 창과 GL 과 입력을 대 준다. 엔진 본체(core::engine::EngineRunner)는 플랫폼을 모른다.

## 2. 설계 원칙 (원본과 다른 것)

1. **노드는 핸들로 가리킨다.** 원본은 부모가 children[] 으로 자식 객체를 들고 자식이 parent 로 거꾸로 가리킨다. 러스트 판은 씬의 NodeTree 가 노드를 slotmap 저장소 하나에 담고, 부모와 자식을 NodeId 로 잇는다. NodeId 는 칸 번호와 세대 번호라서 지운 노드의 핸들은 get_node 가 None 을 돌려준다(원본 isDestroyed 의 공백을 메움). Rc<RefCell> 은 쓰지 않는다.
2. **전역 엔진이 없다.** 원본의 Engine.get_engine() 대신 씬 함수와 컴포넌트 문맥으로 Engine 을 넘긴다.
3. **상속 대신 트레이트와 공통 상태 구조체.** Scene 트레이트 + SceneBase, Component 트레이트 + ComponentBase, Asset 트레이트 + AssetBase. 원본의 Node, ComponentNode, TransformNode, WorldNode 상속 넷은 WorldNode 하나로 합쳤다.
4. **플랫폼 층을 뗐다.** 창, GL, 입력, 루프, 애셋 바이트, 오디오는 platform 아래에만 있다. 안드로이드, iOS 는 여기에 모듈을 더하면 된다.
5. **성능 기능은 엔진 기본.** 묶어 그리기와 화면 밖 거르기를 기본으로 켠다(원본은 묶어 그리기가 기본 꺼짐, 거르기 없음).
6. **ECS 로 가지 않는다.** 원본의 씬, 노드, 컴포넌트 꼴을 그대로 둬서 vanilla.js 게임을 옮기기 쉽게 한다.
7. **2D 와 3D 는 GL 컨텍스트 하나를 같이 쓴다.** (원본 원칙 그대로) 원본 3D 씬은 자체 rAF 를 돌지만 러스트 판은 씬의 pre_draw 에서 그림자, 씬, 블룸 패스를 그리고 `graphic.restore_render_state()` 뒤에 2D 를 그린다. 데스크탑 GL 설정은 깊이 24, 스텐실 8 이다. (examples/modelviewer.rs)

## 3. 소유 구조

```
EngineRunner (플랫폼이 들고 프레임마다 부른다)
├── Engine
│   ├── EngineConfiguration
│   ├── TimeManager, ViewManager
│   ├── InputManager
│   │   └── GamepadManager (gilrs, 첫 tick 에 장치를 연다)
│   ├── AudioManager (소리 장치는 처음 쓸 때 연다)
│   ├── Platform (플랫폼, 브라우저 정보)
│   ├── platform_window (창 손잡이: 데스크탑 winit Window, 웹 캔버스)
│   ├── pending_scene_list, pending_font_registration_list
├── Graphic (플랫폼이 GL 컨텍스트를 만든 뒤 attach_graphic 으로 넘김)
│   ├── ShaderProgram (기본), 파티클/묶음 셰이더
│   ├── ImageTextureCache, TextStringTextureCache (fontdue, WOFF/WOFF2 는 wuff 로 풀기, 색 이모지는 colorglyph)
│   └── 묶음 버퍼, 클립 스택
└── Scene[] (Box<dyn Scene>)
    └── SceneBase
        ├── NodeTree ── slotmap<NodeId, WorldNode>, 루트 노드
        │   └── WorldNode ── NodeTransform, Vec<Box<dyn Component>>, parent_id, child_ids
        ├── Tween[] (TweenId 로 가리킴)
        ├── 맡긴 애셋 목록 (다 읽히면 initialize)
        └── 눌린 노드, 첫 터치 콜백
```

애셋은 원본처럼 중앙 매니저 없이 씬이 멤버로 든다. 같은 그림을 여러 번 그려도 GPU 텍스처는 Graphic 의 캐시가 하나만 만든다.

## 4. 노드와 컴포넌트

- 노드 만들기: `let node_id = node_tree.create_child(parent_id, "name");` 그 뒤 `node_tree[node_id]` 로 꺼내 설정한다. 붙이지 않은 노드는 `create_node` 로 만들고 `add_child` 로 붙인다.
- 지우기: `destroy_node(node_id)` 는 자손까지 지운다. tick, 터치 도중에 부르면 비활성으로 두고 다 돈 뒤에 지운다.
- 컴포넌트 붙이기: 보통은 `node_tree[node_id].add_component(Sprite::...)`. 붙을 때 자식 노드를 만드는 컴포넌트(UIView 같은)는 `node_tree.add_component(node_id, ...)` 로 붙여야 `attach_to_tree` 가 불린다.
- 컴포넌트 문맥(ComponentContext): tick, 터치, 휠에서 받는다. 자기 노드와 트랜스폼, 트리 전체(다른 노드 찾기, 만들기, 지우기), 엔진(입력, 시간, 뷰, 오디오)을 꺼낸다. 부르는 컴포넌트 하나만 노드에서 잠깐 빠져 있고, 같은 노드의 다른 컴포넌트는 `context.get_node_mut().get_component_mut::<T>()` 로 찾는다.
- 씬 코드에서 다른 노드도 만지는 컴포넌트(위젯 같은)를 바로 바꿀 때는 `node_tree.with_component_mut::<T, _>(node_id, |component, tree| ...)` 로 컴포넌트와 트리를 같이 쥔다.
- draw 는 노드 트랜스폼이 걸린 상태에서 자기 트랜스폼만 받는다. 그리는 동안 트리는 바뀌지 않는다.
- 노드의 켜진 컴포넌트가 모두 그리는 사각형(`calculate_drawn_rect`)을 알려 주면, 그 합을 그릴 때의 Graphic 변환(노드 배율, 회전, 부모가 밀어 넣은 변환 포함)으로 화면에 대 보고 밖이면 그 노드의 컴포넌트를 그리지 않는다. 자식은 따로 판정한다. 기본은 `is_drawn_inside_content` 가 true 인 컴포넌트(Paint, Sprite, Mask 등)만 content 영역을 알려 주고 나머지는 모른다(거르지 않음). Text 와 UILabel 은 정렬, 넘침, 줄바꿈 결과로 셈한 글 사각형을, ShadowText 와 ShadowImage 는 그림자 여백까지 넣은 사각형을 알려 준다. `is_tick_needed` 가 false 인 컴포넌트(Paint, Sprite, Text, Mask)만 붙은 노드는 tick 때 꺼내지 않는다.

## 5. 프레임 루프

```
플랫폼: 창/캔버스와 GL 을 만들고 EngineRunner.attach_graphic → apply_pending_scenes
매 프레임 EngineRunner.update_engine:
1. 바이트가 온 글꼴을 글자 캐시에 등록
2. graphic.apply_settings()          뷰포트, 프로젝션, 상태 리셋, 화면 비우기
3. time_manager.update(), input_manager.tick() (게임패드 폴링 포함)
4. 씬마다:
   - 맡긴 애셋을 기다리는 씬: 다 읽혔으면 initialize, 아니면 draw_on_load 만
   - scene.tick → SceneBase.tick: 노드 tick(컴포넌트 → 자식) → 휠, 터치 전달 → 트윈
   - 씬의 터치 훅(touch_press, 뗌 또는 누르고 있음 또는 취소, touch_wheel, 원본 Scene.tickTouch 차례)
   - pre_draw → draw(NodeTree.draw 재귀) → post_draw → (기즈모)
5. 통계 출력(옵션: FPS, DrawCall, Vertex, Culled) → 1회성 입력 플래그 지우기
6. graphic.end_frame() (묶음 내보내기) → 검증용 화면 저장(request_frame_capture, 데스크탑 VANILLA_CAPTURE_*, 웹 ?capture=) → 프레임 번호 증가 (첫 프레임 1)
플랫폼: 데스크탑은 렌더 배율 상한이나 캔버스 비 상한으로 줄여 그렸으면 창의 캔버스 자리로 늘이고(desktop/renderscaler.rs), 화면 넘기기 (데스크탑 swap_buffers, 웹은 콜백이 끝나면 브라우저가)
```

씬 불러오기는 원본의 create → load(비동기) → set_loaded → initialize 를 create → (맡긴 애셋을 기다림) → initialize 로 옮겼다. 데스크탑은 애셋이 그 자리에서 읽혀서 create 바로 뒤에 initialize 가 불린다.

## 6. 그리기

- 모든 2D 그리기는 `Graphic.draw_vertices` 한 곳으로 모인다. 묶을 수 있으면(클립 영역 기록 중 아님, 바꿔 낀 셰이더 없음) 정점을 CPU 에서 변환해 모았다가 같은 텍스처가 이어지는 동안 한 번에 그린다. 원본과 같은 묶음 셰이더로 정점마다 색과 틴트를 실어 틴트가 걸린 그리기도 묶는다.
- 블렌드가 바뀔 때, 클립을 걸거나 풀 때, 파티클을 그릴 때, 프레임 끝에 모은 것을 먼저 내보낸다(`flush_batch`). GL 을 직접 쓰는 코드는 그 전에 flush_batch 를 부른다.
- 셰이더 바꿔 끼우기: `set_shader_program_override` 가 걸린 동안은 묶지 않고 바꿔 낀 프로그램으로 바로 그린다(ShaderSprite). 바꿔 낄 프로그램은 `resolve_shader_program` 이 이름으로 담아 두고, 어트리뷰트 위치를 기본 프로그램과 같게 고정해 기본 버텍스 어레이를 같이 쓴다.
- 화면 효과(ScreenEffect)는 엔진을 거치지 않는다. 씬이 pre_draw 첫머리에서 begin, post_draw 끝에서 end 를 부르면, end 가 화면(또는 영역)을 텍스처로 떠 와 FullscreenPass 사슬을 걸고 되돌려 놓은 뒤 `restore_render_state` 로 Graphic 상태를 되살린다.
- 클립은 원본처럼 스텐실 기반이다.
- 블렌드는 원본처럼 26종을 모두 그린다. 13종(포터 더프 연산자, Lighter, Multiply, Screen)은 고정 블렌딩(`BlendMode::get_blend_function`), 나머지 13종(Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity)은 그리기마다 그릴 영역(정점의 경계 상자)만 지금 프레임버퍼에서 `blit_framebuffer` 로 같은 형식, 같은 자리의 칸에 떠 와 W3C Compositing and Blending Level 1 공식대로 셰이더로 섞는다. 셰이더 글은 원본과 같다. 이 모드에서는 묶지 않고 그리기 하나에 복사 한 번과 드로우 콜 한 번이 든다. 파티클은 서로 겹치지 않는 쿼드끼리 모아 한 번 떠 오고, 바꿔 낀 셰이더는 덧그림 칸(RGBA16F)에 먼저 그린 뒤 섞어 옮긴다. Canvas2D 와 달리 SourceIn, SourceOut, DestinationIn, DestinationAtop, Copy 는 그린 도형 안만 바꾼다(원본과 같음).
- 기본 프레임버퍼에 그릴 때는 알파를 쓰지 않는다(`apply_color_mask`, 원본에 없음). 원본과 웹은 캔버스가 `alpha: false` 라 화면 알파가 늘 1 이고, 데스크탑 기본 프레임버퍼도 프레임 첫머리에 알파를 1 로 비운 뒤 그대로 두어 DST_ALPHA 를 쓰는 모드와 셰이더 섞기가 웹과 같게 나온다. 렌더 타겟에는 알파도 쓰고, GL 을 밖에 내줄 때는 모두 쓰는 마스크로 되돌린다.
- Multiply 는 고정 블렌딩이라 바탕 알파가 1 보다 작은 렌더 타겟에서는 W3C 식과 다르다(원본과 같음). 기본 화면에서는 같다.
- 묶어 그리기와 거르기는 `set_batching_enabled`, `set_culling_enabled` 로 끈다.

## 7. 애셋

- `ImageAsset`, `TextAsset`, `JsonAsset`, `BlobAsset`, `FontAsset`: `load(engine, 경로)` 로 요청하고 `scene_base.track_asset(&asset)` 으로 맡긴다. 데스크탑은 애셋 폴더 후보에서 파일을 읽고, 웹은 `asset_base_url + 경로` 를 fetch 한다.
- 실패한 애셋도 "끝난 것"으로 쳐서 씬이 멈추지 않는다.
- AssetLoader 는 다 받은 그림을 원본 img.onload 처럼 풀어 보고, 깨진 것은 실패로 친다. 푼 그림은 읽기 손잡이에 담겨 ImageAsset 이 다시 풀지 않는다.
- `AudioAsset` 은 오디오 백엔드와 붙어 있어 `engine.load_audio_asset` 으로 읽고 `track_audio_asset` 으로 맡긴다.
- 이미지는 png, jpeg, webp 를 푼다. 글꼴은 ttf, otf, woff, woff2 를 읽는다.
- 이미지는 GPU 에 올린 뒤 CPU 픽셀을 놓는다. 픽셀을 다시 읽을 그림은 `Image.set_pixels_retained(true)` 로 남긴다. (ShadowImage 는 받은 그림을 스스로 남기고, 이미 놓였으면 텍스처를 프레임버퍼에 붙여 되읽는다) GL 컨텍스트를 잃었다 되살리는 길은 아직 없다.
- 기본 글꼴은 엔진에 넣지 않는다. 둘 다 시스템 글꼴이 기본이다. 데스크탑은 default_font_paths 의 글꼴 파일(윈도우는 맑은 고딕)을 fontdue 로 굽고, 웹은 글꼴 바이트가 하나도 없으면 원본처럼 브라우저 Canvas2D 로 구워 브라우저가 고른 시스템 글꼴이 나온다(platform/web/systemfont.rs). 게임 글꼴은 `default_font_url`, `default_font_bytes`, FontAsset 가운데 하나로 주면 fontdue 길로 굽는다. (원본 기본값 DOSGothic 은 뺐다)

## 8. 코드 꼴

이름은 러스트 규칙(Rust API Guidelines C-CASE)을 따른다: 함수, 메서드, 변수, 필드, 모듈은 snake_case, 타입과 트레이트와 열거 값은 UpperCamelCase, 상수는 SCREAMING_SNAKE_CASE. vanilla.js 의 이름은 이 규칙으로 바꿔 옮긴다. (setFillColor → set_fill_color, Colors.white → Colors::WHITE) 나머지는 vanilla.js 지침을 따른다: 이름 줄이지 않기, 중괄호 늘 쓰기, else 는 다음 줄, `//====` 머리 주석에 독백체 설명, 파일 머리에 원본과의 대응과 다른 점. rustfmt 는 else 줄바꿈을 망가뜨려서 돌리지 않는다.

## 9. 아직 안 옮긴 것

- UINode, 오토레이아웃(ui/autolayout), UIScene 은 옮겼다. 노드 종류가 WorldNode 하나뿐이라 UINode 는 노드에 붙는 컴포넌트이고, 솔버 값을 노드의 자리와 content_size 로 옮긴다. (ui/uinode.rs, ui/uiscene.rs)
- GameScene, UIScene 의 덮어쓰기는 GameScene, UIScene 트레이트와 game_scene!, ui_scene! 매크로다. 바탕이 resize, layout, post_draw 를 스스로 부른다. (game/gamescene.rs)
- DevTools 는 옮겼다. 리플렉션이 없어 인스펙터의 컴포넌트 속성은 UIDocument 가 저장하는 키와 값으로 보인다. (misc/devtools.rs)
- web/Pane(DOM 화면 분할 패널)은 웹(wasm32) 빌드에만 옮겼다. 데스크탑에는 DOM 이 없다. (web/pane.rs)
- 글자, 글꼴 갈래는 원본과 맞췄다. (2026-11-25) 오른쪽에서 왼쪽 글(양방향 정렬과 글자 모양 짓기, core/textshaping.rs), CSS 같은 글꼴 얼굴 고르기와 가짜 기울임, 가짜 굵게(Chromium 과 Skia 의 값),
  ICU 낱말 나누기와 글자 묶음 줄바꿈, DynamicFont, UIFont, LanguagePack 을 옮겼다. 색 이모지는 core/colorglyph.rs 가 sbix, CBDT, COLR v0/v1 을 제 색으로 그리고,
  이모지 표시 묶음(FE0F, Emoji_Presentation)은 크롬처럼 색 글리프를 가진 글꼴을 먼저 고른다. 원본과 같아질 수 없어 남은 것: 래스터 픽셀(fontdue 와 브라우저 Skia, CoreGraphics),
  일본어와 태국어 낱말 사전(ICU4X 와 크롬의 ICU4C 데이터), 데스크탑의 시스템 글꼴 대체 목록(브라우저와 운영체제가 고르는 목록 대신 EngineConfiguration.fallback_font_paths),
  맥 크롬이 CoreText 로 Apple Color Emoji 를 그릴 때의 크기와 자리 보정(23px 아래에서 전진폭과 그림을 키우고 기준선 아래로 약 1/8em 내린다. 글꼴 자료에 없는 CoreText 안의 동작이라 러스트 판은 sbix 자료대로 놓는다).
- 코어, 플랫폼, 기반에서 원본과 다르게 남은 것 (언어와 플랫폼 때문):
  - 원본의 객체 모델 도우미(base/object.js, reflection.js, singleton.js), 비동기 기다리기(base/wait.js), 빈 함수 calculator.js, Enum 흉내(begin, auto, readonly)는 옮기지 않는다. 러스트의 트레이트, Any, enum 과 엔진 루프가 맡는다.
  - 전역 엔진(Engine.getEngine, Scene.getEngine, SceneManager, PointerCursor.readEngineCanvas)이 없어 엔진을 함수로 넘긴다. Component.getNode 자리는 ComponentContext 다.
  - SoundSystem 의 음악과 효과음 버스는 AudioBus 다. (원본에도 같은 꼴로 넣었다) 데스크탑은 kira 하위 트랙, 웹은 GainNode 이고, 그 노드를 꺼내는 get_gain_node 는 웹에만 있다.
  - Timer.use 는 러스트 예약어라 activate 다. (원본에도 activate 를 넣었다)
  - 노드 객체를 돌려주는 getParent, getChildren, getSiblings 는 핸들을 돌려주는 get_parent_id, get_child_ids, get_sibling_ids 다.
  - 갈아 끼운 저장소(LocalStorage.get_backend, PersistedStore.get_storage_backend)와 콜백 게터(UIButton.get_click_event 같은 것)는 Rc<RefCell> 에 담아 돌려주고, 부르는 쪽이 빌려 쓴다.
    SpriteAnimator.findSprite(형제를 쥐기)와 SharedBakeCache 의 놓기(release)는 빌림 규칙과 Rc 참조 수로 갈음한다.
  - 데스크탑에는 문서와 화면 키보드와 브라우저 캐시가 없어 auto_resize_on_window_resize, ignore_virtual_keyboard_resize, preserve_drawing_buffer, set_no_cache, 오른쪽 클릭 메뉴 막기는 웹에서만 뜻이 있다. 웨이랜드는 창 가림과 최소화를 알리지 않아 Hidden 이 오지 않는다.
- UIDocument 는 옮겼다. 원본의 리플렉션 직렬화 대신 종류 이름마다 짓기, 쓰기, 읽기 함수를 표에 등록하고, 원본이 반사로 저장하는 종류는 원본 반사 결과와 같은 키로 쓴다. (ui/uidocument.rs)
- 원본 160파일 가운데 122파일을 옮겼고, 20파일은 죽은 코드이거나 러스트 표준으로 대신해서 일부러 옮기지 않았다. (2026-11-25, HumanSkinRenderer 를 옮김)
- experimental/camera.js 는 옮기지 않는다. 회전과 begin/end 를 Camera2D(misc/camera2d.rs)에 넣어 그 기능을 모두 덮는다. experimental/transform2d.js, aabb.js 는 파일 전체가 주석이다.
