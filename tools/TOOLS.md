# 도구

## apidoc
엔진 API 문서 자료(JSON)를 뽑는 도구. (워크스페이스 크레이트 vanilla-engine-apidoc, syn 과 serde_json 만 씀, vanilla.js 의 tools/apidoc.cjs 와 같은 자료 꼴)
```bash
cargo run -p vanilla-engine-apidoc -- <판> [--date YYYY-MM-DD] [--counterpart <vanilla.js 의 docs/api/<판>.json>]
cargo run -p vanilla-engine-apidoc -- <판> --check
cargo run -p vanilla-engine-apidoc -- <판> --missing [--only <경로 앞>] [--exclude <경로 앞>]
cargo run -p vanilla-engine-apidoc -- <판> --gaps [--only <경로 앞>] [--exclude <경로 앞>]
```
- 첫 줄: src/lib.rs 에서 pub mod 와 pub use 로 밖에서 닿는 공개 API(구조체, 트레이트, 꼴 별칭, 열거, 함수, 상수, 내보낸 매크로, 다시 내보낸 반사 매크로)를 읽어 docs/api/<판>.json 을 쓴다. pub(crate) 와 닿지 않는 모듈의 것은 뺀다. (닿지 않는 모듈의 것도 pub use 로 내보냈으면 넣는다)
- 자료 꼴은 vanilla.js 와 같다. { engine, version, date, modules: [ { name, summary, examples, classes, functions, enums } ] } 이고 설명 칸은 모두 { ko, en } 이다. 러스트 판에서 더한 것:
  - 클래스 자리에 struct, trait, 꼴 별칭(type)을 두고 kind 칸으로 가른다. 꼴 별칭은 signature 칸에 정의를 둔다. traits 는 구현한 트레이트(derive 포함, 트레이트는 상위 트레이트)다. extends 는 없다.
  - 멤버 kind: self 를 받는 함수는 method, 받지 않는 연관 함수는 static, 그 가운데 new 는 constructor, 연관 상수는 static, 공개 필드는 field. 모듈 함수는 원본처럼 method, 모듈 상수와 static 은 field.
  - 열거에 impl 블록의 공개 함수가 있으면 members 칸에 둔다. 값의 value 는 적은 판별 값이나 품은 꼴(Number(f32) 같은)이고 없으면 칸이 없다.
  - counterpart 는 클래스, 열거, 멤버, 함수에 붙는다. { engine: "vanilla.js", module, class, name } (없는 칸은 뺀다)
  - source 는 클래스와 열거에도 붙는다.
- 설명은 rustdoc 문서 주석에서 읽는다. 첫 절이 설명, `# 인자` 의 `` - `이름`: 설명 `` 이 인자 설명, `# 돌려줌` 이 돌려줌 설명, `# 예제` 의 코드 칸이 예제(바로 앞 한 줄 글이 제목, rustdoc 이 숨기는 `# ` 줄은 뺀다)다. 문서 주석이 없으면 `//====` 머리 주석 글을 설명으로 쓴다. 꼴은 CLAUDE.md 의 '문서 주석'.
- ko 는 문서 주석 원문(긴 줄표와 가운뎃점만 쉼표로 바꿈)이고 마크다운 그대로다. en 은 docs/api/translation.en.json(ko 원문 → en) 표에서 가져온다. 표에 없는 ko 는 en 을 빈 글로 두고 몇 개인지 알린다. 한글이 없는 글은 그대로 en 이 된다.
- since 는 v0.3.2 태그(러스트 판 첫 release)의 소스에서 같은 타입에 같은 이름의 멤버(모듈 함수는 같은 이름)가 있으면 "0.3.2", 없으면 그 판이다. 이름은 밑줄과 대소문자를 빼고 비교한다. (0.3.2 의 이름은 camelCase 였다)
- date 는 --date, UPDATE.md 의 "# <판> (YYYY-MM-DD)", 이미 있는 자료의 날짜, 오늘(한국 시각) 차례로 정한다.
- counterpart 는 docs/api/counterpart.<판>.json 의 짝이다. --counterpart 에 vanilla.js 의 자료를 주면 이름으로 짝을 찾아 그 표를 다시 쓴다. (클래스와 열거는 같은 이름, 이름 끝 Base 를 뗀 이름의 멤버도 본다. 멤버는 snake_case 를 camelCase 로, 밑줄과 대소문자를 뺀 이름, get_ / set_ / is_ / has_ 를 뗀 속성과 필드, new 와 constructor. 모듈 함수는 같은 마지막 경로 조각의 모듈을 먼저 고른다) 표는 { engine, counterpartEngine, version, items: [ { "vanilla-engine": { module, class, name }, "vanilla.js": { module, class, name } } ] } 이고, vanilla.js 쪽 자료가 러스트 짝을 채울 때도 쓴다.
- --missing: 표에 없는 ko 글 목록을 JSON 배열로 낸다. 옮긴 것을 translation.en.json 에 더하고 첫 줄을 다시 돌려 en 빈 칸을 0 으로 만든다.
- --check: docs/api/<판>.json 의 꼴(설명 칸이 { ko, en } 인지, kind, since, 예제 칸, 예제 코드에 한글)을 검사하고 모듈, 클래스(종류별), 멤버(종류별), 인자, 함수, 열거 수와 짝 수, ko 빈 칸(갈래별), en 빈 칸, 예제 수, since 분포를 찍는다. 필드와 상수의 returns 는 꼴만 담는 칸이라 설명이 비어도 세지 않는다. 가이드는 ko 와 en 의 파일 이름과 코드 칸이 같은지, rust 코드 칸과 en 본문에 한글이 없는지, 업데이트 문서가 있는지, 자료와 문서에 긴 줄표나 가운뎃점이 없는지 본다. 오류가 있으면 끝 코드가 1 이다.
- --gaps: 소스에서 문서가 빈 곳을 파일마다 찍는다. 공개 API 의 빈 설명(모듈 요약, 클래스 요약, 멤버, 인자, 돌려줌, 열거와 그 값), 없는 인자를 적은 곳, 예제 없는 구조체와 트레이트, 제 예제도 클래스 예제도 없는 모듈, src 의 모든 함수 가운데 문서 주석도 머리 주석도 없는 것. 줄 번호는 이름:줄 로 찍는다.
- --only, --exclude: --missing, --gaps 가 볼 소스를 저장소 기준 경로의 앞부분으로 거른다. (src/ui/, src/base/rect.rs 처럼, 여러 번 줄 수 있다)

### 예제 검사
- API 예제는 문서 주석의 코드 칸이라 `cargo test --doc` 이 엔진에 대고 컴파일하고 돌린다. 창이나 GL 이 있어야 하면 `rust,no_run` 으로 컴파일만 한다.
- 가이드(docs/guide/<가장 높은 판>/ko/*.md)의 rust 코드 칸은 이 크레이트의 라이브러리 자리(tools/apidoc/src/lib.rs, 목록은 build.rs 가 장 파일마다 짓는다)가 doctest 로 검사한다. `cargo test --doc -p vanilla-engine-apidoc` (장 하나는 `-- Guide08Drawing` 처럼 파일 이름을 이은 이름으로 거른다) en 가이드의 코드 칸은 ko 와 같아야 한다. (--check)
- 가이드(docs/guide/<판>/ko, en)와 업데이트(docs/update/<판>.ko.md, .en.md)는 손으로 옮긴다. 업데이트 ko 는 UPDATE.md 의 그 판 절이다.
