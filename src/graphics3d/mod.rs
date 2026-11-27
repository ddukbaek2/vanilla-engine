//==============================================================================
//! 3D 그래픽. (원본 src/graphics3d, WebGL2 3D 스택)
//! - 원본과 같이 2D Graphic 과 GL 컨텍스트 하나를 같이 쓴다. GL 객체를 만들거나 그리는 함수는 원본의 webGL2RenderingContext
//!   멤버 대신 gl: &GlContext 를 받는다. (Graphic.get_gl() 로 꺼낸다. ShaderProgram 과 같은 꼴)
//! - 3D 를 그리기 전에 graphic.flush_batch() 로 모아 둔 2D 를 내보내고, 다 그린 뒤 graphic.restore_render_state() 로
//!   2D 프로그램, 버텍스 어레이, 뷰포트, 블렌드, 깊이 상태를 되돌린다.
//! - 파일 하나가 원본 파일 하나다. fullscreenpass.js 는 core::fullscreenpass 에 있고 여기서 다시 내보낸다.
//!   원본에서 옮기지 않은 것: webgl2.js 와 gl2d.js(죽은 코드, 원본은 src/deprecated 로 옮겼다).
//!
//! # 예제
//! 2D 를 내보내고 3D 모델을 그린 뒤 2D 상태 되돌리기
//! ```rust,no_run
//! use vanilla::{Graphic, Matrix4, SkinnedModel, SkinnedModelRenderer, Vector3};
//!
//! fn draw_3d(graphic: &mut Graphic, skinned_model_renderer: &SkinnedModelRenderer, skinned_model: &mut SkinnedModel, view_projection_matrix: &Matrix4, camera_position: Vector3) {
//! 	graphic.flush_batch();
//! 	let model_matrix = Matrix4::create_identity();
//! 	skinned_model_renderer.draw(graphic.get_gl(), skinned_model, view_projection_matrix.get_elements(), model_matrix.get_elements(), camera_position.x, camera_position.y, camera_position.z);
//! 	graphic.restore_render_state();
//! }
//! ```
//==============================================================================
pub mod ambientocclusioneffect;
pub mod bloomeffect;
pub mod fbxloader;
pub mod material;
pub mod rendertarget;
pub mod shadowmap;
pub mod skinnedmodel;
pub mod skinnedmodelasset;
pub mod skinnedmodelrenderer;
pub mod subsurfacescatteringeffect;
pub mod toon;

// 풀스크린 패스는 core 의 것을 쓴다. (원본 fullscreenpass.js, ScreenEffect 와 같이 쓰는 부품)
pub use crate::core::fullscreenpass;

/// GL 객체 타입. (core::graphic 에 없는 프레임버퍼와 렌더버퍼)
pub type GlFramebuffer = <glow::Context as glow::HasContext>::Framebuffer;
/// GL 렌더버퍼 핸들. (glow::Context 의 Renderbuffer, 렌더 타겟의 깊이 버퍼 같은 것)
pub type GlRenderbuffer = <glow::Context as glow::HasContext>::Renderbuffer;
pub mod camera3d;
pub mod camerashake;
pub mod humanskinrenderer;

// 지금 묶인 GL 객체 읽기. (앞의 것을 돌려 놓을 때, 원본 getParameter(..._BINDING). 웹에서 null 을 먼저 가리는 까닭은 core::graphic::is_gl_binding_present 머리말)
pub(crate) use crate::core::graphic::{read_framebuffer_binding, read_program_binding, read_texture_binding, read_vertex_array_binding};
pub mod multilevelbloomeffect;
