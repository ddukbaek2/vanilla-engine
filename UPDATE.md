# 업데이트 기록

# 아직 release 에 합치지 않은 것 (release 에 합칠 때 패치 판을 하나 올려 매긴다)
- README: 장점 위주 소개, 예제 화면 사진(docs/images), 사용법(씬 생명주기, 노드와 컴포넌트, 애셋 로딩, 웹 빌드), 기술 노트(제너레이셔널 핸들 노드 트리, 컴포넌트 접근, 배칭, 컬링, async 없는 애셋 로딩, 텍스트 렌더링, 2D 와 3D, 웹 한글 입력, 플랫폼 레이어), 성능 표를 새로 썼다
- 예제: nodebasics. 노드를 만들고 Paint, Text, 직접 만든 컴포넌트(Spin)를 붙인다. (README '노드와 컴포넌트'의 코드)

# 0.3.2 (2026-11-18)
- 첫 공개. vanilla.js 를 러스트로 옮겼다. 옮긴 원본은 vanilla.js dev 의 5efe62a(0.3.2 와 그 뒤 dev 커밋 둘)다. 판 번호는 vanilla.js 0.3.2 에서 이어 받고, 그 뒤로는 이 저장소대로 올린다.
- 데스크탑(OpenGL 3.3 코어, winit, glutin)과 웹(wasm32, WebGL2, requestAnimationFrame)에서 돈다.
- 원본 160 파일 가운데 121 파일을 옮겼다. 20 파일은 러스트에 필요 없어서(컬렉션, 죽은 코드, 빈 파일 같은 것) 옮기지 않았다. 아직 안 옮긴 것은 UINode, UIScene, UIDocument, 오토레이아웃, DevTools, DynamicFont, web/Pane, HumanSkinRenderer 다.
- 원본과 다른 것
  - 노드는 씬의 NodeTree 가 slotmap 저장소에 담고 NodeId(칸 번호와 세대 번호)로 가리킨다. 지운 노드의 핸들은 None 을 돌려준다.
  - 애셋은 await 대신 "읽기 요청 → 나중에 다 읽힘" 꼴이다. 씬이 create 에서 요청하고 trackAsset 으로 맡기면, 다 읽힌 뒤 initialize 를 부르고 그동안은 drawOnLoad 를 그린다.
  - 2D 그리기를 묶어 그린다. (배칭, 기본으로 켬) 화면 밖의 노드는 그리지 않는다.
  - 기본 글꼴(DOSGothic)을 넣지 않는다. 글꼴을 주지 않으면 데스크탑은 시스템 글꼴 파일을, 웹은 원본처럼 브라우저 Canvas2D 로 시스템 글꼴을 쓴다.
- 옮기다 찾아 고친 원본 버그
  - Easing: 거듭제곱이 정수 지수에서만 맞던 것
  - OBB.overlaps: 늘 false 를 돌려주던 것
  - arrangeFan: 빠진 import, 라디안 값을 도 단위 회전에 넘기던 것
  - NodeLayout: anchoredPosition 을 잘못된 차례로 셈하던 것
  - Text: 줄바꿈한 글의 보이는 글자 수가 다음 줄을 덧그리던 것
  - UIDropdown: 고른 항목 강조를 틱마다 덮어쓰던 것
