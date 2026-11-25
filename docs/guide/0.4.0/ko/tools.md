# 도구

엔진 저장소에 든 도구는 API 문서 자료를 뽑고 문서를 검사하는 `apidoc` 하나입니다. UI, 파티클, 타임라인, 씬을 짜는 저작 도구는 원본 vanilla.js 저장소에 있고, 러스트 판은 그 도구가 저장한 파일을 그대로 읽습니다.

## apidoc

엔진 API 문서 자료(JSON)를 뽑는 도구입니다. 워크스페이스 크레이트 `vanilla-engine-apidoc` 이고, 원본 vanilla.js 의 `tools/apidoc.cjs` 와 같은 자료 꼴을 씁니다. 엔진 저장소 루트에서 실행합니다.

```bash
cargo run -p vanilla-engine-apidoc -- 0.4.0
cargo run -p vanilla-engine-apidoc -- 0.4.0 --check
cargo run -p vanilla-engine-apidoc -- 0.4.0 --missing
cargo run -p vanilla-engine-apidoc -- 0.4.0 --gaps --only src/ui/
```

| 갈래 | 하는 일 |
| --- | --- |
| (판만 줌) | `src/lib.rs` 에서 `pub mod` 와 `pub use` 로 밖에서 닿는 공개 API(구조체, 트레이트, 꼴 별칭, 열거, 함수, 상수, 내보낸 매크로, 다시 내보낸 반사 매크로)를 읽어 `docs/api/<판>.json` 을 씁니다. `--date YYYY-MM-DD` 로 날짜를, `--counterpart <vanilla.js 의 자료>` 로 원본과의 짝 표를 다시 짓게 할 수 있습니다 |
| `--check` | 자료의 꼴을 검사하고 모듈, 클래스, 멤버, 예제 수와 빈 설명 수를 찍습니다. 가이드는 ko 와 en 의 파일 이름과 코드 칸이 같은지, rust 코드 칸과 en 본문에 한글이 없는지 봅니다. 오류가 있으면 끝 코드가 1 입니다 |
| `--missing` | 영어 번역 표(`docs/api/translation.en.json`)에 없는 설명 글을 JSON 배열로 냅니다 |
| `--gaps` | 소스에서 문서가 빈 곳(공개 API 의 빈 설명, 예제 없는 구조체와 트레이트, 머리 주석이 없는 함수)을 파일마다 찍습니다 |
| `--only`, `--exclude` | `--missing`, `--gaps` 가 볼 소스를 저장소 기준 경로의 앞부분으로 거릅니다. (여러 번 줄 수 있습니다) |

- 설명은 rustdoc 문서 주석에서 읽습니다. 첫 절이 설명이고, 인자, 돌려줌, 예제 절을 나눠 읽습니다. 예제는 예제 절의 코드 칸이고, 바로 앞 한 줄 글이 제목입니다.
- 자료의 설명 칸은 모두 `{ ko, en }` 입니다. ko 는 문서 주석 원문이고, en 은 번역 표에서 가져옵니다. 표에 없는 글은 en 을 비워 두고 몇 개인지 알립니다.
- 원본 자료와 다르게 클래스 자리에 구조체, 트레이트, 꼴 별칭을 두고 `kind` 칸으로 가릅니다. 원본의 같은 항목은 `counterpart` 칸에 적습니다.

## 예제와 가이드 검사

- API 문서의 예제는 문서 주석의 코드 칸이라 `cargo test --doc` 이 엔진에 대고 컴파일하고 돌립니다. 창이나 GL 이 있어야 하는 예제는 `no_run` 으로 컴파일만 합니다.
- 이 가이드의 rust 코드 칸도 같습니다. `vanilla-engine-apidoc` 크레이트가 가장 높은 판의 ko 장마다 doctest 를 짓습니다. 장 하나만 돌릴 때는 파일 이름을 이은 이름으로 거릅니다.

```bash
cargo test --doc
cargo test --doc -p vanilla-engine-apidoc
cargo test --doc -p vanilla-engine-apidoc -- Guide08Drawing
```

## 원본 저작 도구와의 관계

원본 vanilla.js 저장소에는 브라우저에서 여는 저작 도구(UI Editor, Particle Editor, Timeline Editor, Visual Editor)와 Node.js 로 돌리는 명령줄 도구(`tools/atlas.cjs` 같은 것)가 있습니다. 러스트 판에는 따로 된 저작 도구가 없고, 그 도구들이 저장한 파일을 읽습니다.

| 원본 도구가 쓰는 것 | 러스트 판에서 읽는 곳 |
| --- | --- |
| Visual Editor 의 씬 파일(`.scene.json`), UI Editor 의 문서 | `UIDocument` (`register_editor_types` 한 줄이 원본 에디터가 등록하는 종류를 모두 등록합니다) |
| Timeline Editor 의 타임라인 문서 | `Timeline` |
| Particle Editor 의 파티클 서술 | `ParticleSystem::apply_description` |
| `tools/atlas.cjs` 가 구운 아틀라스 JSON 과 쪽 그림 | `SpriteAtlas` |

도구마다의 파일 꼴과 두 엔진이 같이 쓰는 파일은 "저작 도구와 두 엔진이 같이 쓰는 파일" 장에서 봅니다. 원본 도구 목록과 데모는 원본 저장소(<https://github.com/ddukbaek2/vanilla.js>)에 있습니다.

## 관련 타입

UIDocument, Timeline, ParticleSystem, SpriteAtlas
