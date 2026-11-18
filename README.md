<div align="center">

# 바닐라엔진

**Vanilla Engine. [vanilla.js](https://github.com/ddukbaek2/vanilla.js) 를 러스트로 옮긴 게임 엔진**

[![version](https://img.shields.io/badge/version-0.3.2-d4b06a?style=flat-square)](UPDATE.md)
[![license](https://img.shields.io/badge/license-MIT-3f3f46?style=flat-square)](LICENSE)
[![rust](https://img.shields.io/badge/rust-2021-3f3f46?style=flat-square)](Cargo.toml)
[![renderer](https://img.shields.io/badge/renderer-OpenGL%203.3%20%2F%20WebGL2-3f3f46?style=flat-square)](src/core/graphic.rs)

[빠른 시작](#빠른-시작) / [예제](#예제) / [웹으로 빌드](#웹으로-빌드) / [진행 상황](#진행-상황) / [구조 문서](ARCHITECTURE.md) / [업데이트 기록](UPDATE.md)

</div>

---

## 소개

바닐라엔진은 자바스크립트 웹게임 엔진 vanilla.js 를 러스트로 옮긴 엔진이다. 크레이트 이름은 `vanilla-engine` 이고, 코드에서는 짧게 `vanilla` 로 쓴다. (`use vanilla::...`) 공개 API 는 원본의 이름과 쓰임새를 따른다. 같은 게임 코드가 데스크탑 창과 웹(wasm) 양쪽에서 돈다.

- 씬, 노드와 컴포넌트, 트윈, 입력, 해상도 대응 뷰
- 스프라이트, 텍스트, 도형, 마스크를 같은 출력 경로로 그리는 2D 렌더러 (배칭, 화면 밖 걸러내기)
- UI 위젯 (버튼, 슬라이더, 스크롤 뷰, 입력 필드, 드롭다운 같은 것들), 리치텍스트
- 파티클, 셰이더 스프라이트, 화면 효과, 트레일
- 타임라인, 애니메이터, 스키닝 3D (glTF, FBX), 그림자, 블룸, 앰비언트 오클루전
- 오디오, 게임패드, 웹소켓, 로컬 저장소

데스크탑은 OpenGL 3.3 코어(glow, winit, glutin)로, 웹은 원본처럼 캔버스와 WebGL2, requestAnimationFrame 으로 돈다.

## 빠른 시작

`Cargo.toml`:

```toml
[dependencies]
vanilla-engine = { git = "https://github.com/ddukbaek2/vanilla-engine" }
```

`src/main.rs`:

```rust
#![allow(non_snake_case)]

use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};

struct MainScene {
	sceneBase: SceneBase,
}

impl Scene for MainScene {
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.sceneBase.drawSceneBackground(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.setFillColor(Colors::white);
		graphic.drawRect(Rect::create(100.0, 100.0, 200.0, 120.0));
	}
}

fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("바닐라엔진");
	engineConfiguration.referenceResolutionSize = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engineConfiguration);
	let mainScene = MainScene {
		sceneBase: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(mainScene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

같은 코드가 `examples/quickstart.rs` 에 있다.

```bash
cargo run --example quickstart
```

## 예제

| 예제 | 내용 |
| --- | --- |
| quickstart | 빠른 시작과 같은 코드 |
| uigallery, uigallery2 | UI 위젯 |
| inputgallery | 입력 필드, IME 조합, 리치텍스트 |
| effectgallery | 셰이더 스프라이트, 화면 효과, 트레일 |
| misctour | 카메라, 길찾기, 대사, 흔들기 같은 misc 도구 |
| timelineshowcase | 타임라인 |
| modelviewer | 스키닝 3D, 그림자, 블룸 |
| audiotone | 소리 (창 없이 소리만 내고 끝난다) |
| gamepadlist | 연결된 게임패드 목록 |

`modelviewer` 와 `timelineshowcase` 는 원본 예제 애셋을 쓴다. vanilla.js 저장소를 이 저장소 옆에 받아 두면 된다.

```
Github/
  vanilla.js/
  vanilla-engine/
```

## 웹으로 빌드

wasm32 타깃과 [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) CLI 가 있어야 한다.

```bash
rustup target add wasm32-unknown-unknown
cargo build --profile wasm-release --example uigallery --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web target/wasm32-unknown-unknown/wasm-release/examples/uigallery.wasm
```

`web/index.html`:

```html
<!doctype html>
<html>
<head><meta charset="utf-8"><style>html, body { margin: 0; overflow: hidden; background: #000; }</style></head>
<body>
<script type="module">
import init from "./uigallery.js";
init();
</script>
</body>
</html>
```

캔버스는 `EngineConfiguration.canvasId`(기본 `mainCanvas`) 로 찾고, 없으면 만든다. 글꼴을 주지 않으면 데스크탑은 시스템 글꼴 파일을, 웹은 브라우저 시스템 글꼴을 쓴다.

## 진행 상황

- 판 0.3.2. 판 번호는 vanilla.js 0.3.2 에서 이어 받았다. 아직 옮기는 중이다.
- 원본 160 파일 가운데 121 파일을 옮겼다. 20 파일은 러스트에 필요 없어서(컬렉션, 죽은 코드 같은 것) 일부러 옮기지 않았다.
- 아직 안 옮긴 것: UINode, UIScene, UIDocument, 오토레이아웃, DevTools, DynamicFont, web/Pane, HumanSkinRenderer
- 확인한 환경: 윈도우 11 데스크탑, 크롬(웹). macOS, 리눅스는 아직 돌려 보지 않았다.
- 실제 기기에서 아직 확인하지 않은 것: 웹 오디오, 웹 터치 입력, 실물 게임패드

## 코드 꼴

원본 vanilla.js 와 같다.

- 이름은 모두 camelCase 다. 줄임말을 쓰지 않는다. (러스트 기본 꼴 경고는 끈다)
- 중괄호는 늘 쓰고, `else` 는 새 줄에서 시작한다. 들여쓰기는 탭이다.
- 함수마다 `//====` 머리 주석을 단다. 파일 머리에는 원본의 어느 파일을 옮겼는지 적는다.
- rustfmt 는 쓰지 않는다.

## 라이선스

[MIT](LICENSE)
