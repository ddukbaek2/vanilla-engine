//==============================================================================
// 3D 그래픽. (원본 src/experimental/graphics, WebGL2 3D 스택)
// - 원본과 같이 2D Graphic 과 GL 컨텍스트 하나를 같이 쓴다. GL 객체를 만들거나 그리는 함수는 원본의 webGL2RenderingContext
//   멤버 대신 gl: &glow::Context 를 받는다. (Graphic.get_gl() 로 꺼낸다. ShaderProgram 과 같은 꼴)
// - 3D 를 그리기 전에 graphic.flush_batch() 로 모아 둔 2D 를 내보내고, 다 그린 뒤 graphic.restore_render_state() 로
//   2D 프로그램, 버텍스 어레이, 뷰포트, 블렌드, 깊이 상태를 되돌린다.
// - 파일 하나가 원본 파일 하나다. fullscreenpass.js 는 core::fullscreenpass 에 있고 여기서 다시 내보낸다.
//   원본에서 옮기지 않은 것: humanskinrenderer.js(게임이 안 씀), webgl2.js 와 gl2d.js(죽은 코드).
//==============================================================================
pub mod ambientocclusioneffect;
pub mod bloomeffect;
pub mod fbxloader;
pub mod material;
pub mod rendertarget;
pub mod shadowmap;
pub mod skinnedmodel;
pub mod skinnedmodelrenderer;
pub mod subsurfacescatteringeffect;
pub mod toon;

// 풀스크린 패스는 core 의 것을 쓴다. (원본 fullscreenpass.js, ScreenEffect 와 같이 쓰는 부품)
pub use crate::core::fullscreenpass;

// GL 객체 타입. (core::graphic 에 없는 프레임버퍼와 렌더버퍼)
pub type GlFramebuffer = <glow::Context as glow::HasContext>::Framebuffer;
pub type GlRenderbuffer = <glow::Context as glow::HasContext>::Renderbuffer;
pub mod camera3d;
