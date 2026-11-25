//==============================================================================
// 3D 그래픽. (원본 src/experimental/graphics, WebGL2 3D 스택)
// - 원본과 같이 2D Graphic 과 GL 컨텍스트 하나를 같이 쓴다. GL 객체를 만들거나 그리는 함수는 원본의 webGL2RenderingContext
//   멤버 대신 gl: &glow::Context 를 받는다. (Graphic.get_gl() 로 꺼낸다. ShaderProgram 과 같은 꼴)
// - 3D 를 그리기 전에 graphic.flush_batch() 로 모아 둔 2D 를 내보내고, 다 그린 뒤 graphic.restore_render_state() 로
//   2D 프로그램, 버텍스 어레이, 뷰포트, 블렌드, 깊이 상태를 되돌린다.
// - 파일 하나가 원본 파일 하나다. fullscreenpass.js 는 core::fullscreenpass 에 있고 여기서 다시 내보낸다.
//   원본에서 옮기지 않은 것: webgl2.js 와 gl2d.js(죽은 코드).
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
pub mod humanskinrenderer;

//==============================================================================
// 지금 묶인 프레임버퍼 읽기. (앞의 것을 돌려 놓을 때)
// - 웹의 glow 는 묶인 것이 없을 때(기본 프레임버퍼) 읽으면 "A resource was created externally" 로 패닉한다.
//   glow 웹 컨텍스트에서는 원래 WebGL2 컨텍스트를 꺼낼 수 없어 null 을 가릴 수 없으므로, 웹에서는 기본 프레임버퍼(None)로 본다.
//==============================================================================
pub(crate) fn read_framebuffer_binding(gl: &glow::Context) -> Option<GlFramebuffer> {
	#[cfg(not(target_arch = "wasm32"))]
	{
		use glow::HasContext;
		unsafe {
			gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING)
		}
	}
	#[cfg(target_arch = "wasm32")]
	{
		let _ = gl;
		None
	}
}

//==============================================================================
// 지금 유닛에 묶인 2D 텍스처 읽기. (앞의 것을 돌려 놓을 때, 웹은 read_framebuffer_binding 과 같은 까닭으로 None)
//==============================================================================
pub(crate) fn read_texture_binding(gl: &glow::Context) -> Option<crate::core::graphic::GlTexture> {
	#[cfg(not(target_arch = "wasm32"))]
	{
		use glow::HasContext;
		unsafe {
			gl.get_parameter_texture(glow::TEXTURE_BINDING_2D)
		}
	}
	#[cfg(target_arch = "wasm32")]
	{
		let _ = gl;
		None
	}
}
