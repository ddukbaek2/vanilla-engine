//==============================================================================
// 렌더 배율 상한의 데스크탑 출력. (ViewManager.set_max_render_pixel_ratio)
// - GL 기본 프레임버퍼는 창의 물리 픽셀 크기로 정해져 줄일 수 없다. 그래서 엔진은 기본 프레임버퍼의 왼쪽 아래에 줄인 렌더 크기로 그리고,
//   프레임 끝에 그 영역을 텍스처로 떠서 창 전체에 늘여 그린다. (웹은 캔버스 드로잉 버퍼를 줄이고 브라우저가 CSS 크기로 늘인다)
// - 기본 프레임버퍼에 그대로 그리므로 화면 효과와 3D 패스가 기본 프레임버퍼로 되돌아가도 그대로 맞다.
// - 늘이기는 선형 거르기다. (브라우저가 캔버스를 늘이는 기본과 같다)
//==============================================================================
use crate::core::fullscreenpass::FullscreenPass;
use crate::core::graphic::{GlTexture, GlUniformLocation};
use glow::HasContext;

// 렌더 영역을 늘여 그리는 셰이더.
const RENDER_SCALE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision mediump float;
in vec2 fragmentCoordinate;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
out vec4 outputColor;
void main() {
	outputColor = texture(sourceTexture, fragmentTextureCoordinate);
}
";

//==============================================================================
// 렌더 영역 늘이개.
//==============================================================================
pub(crate) struct RenderScaler {
	fullscreen_pass: FullscreenPass,
	source_texture: GlTexture,
	source_texture_location: Option<GlUniformLocation>,
}

impl RenderScaler {
	//==============================================================================
	// 생성. (셰이더와 텍스처)
	//==============================================================================
	pub(crate) fn new(gl: &glow::Context) -> Result<RenderScaler, String> {
		let fullscreen_pass = FullscreenPass::new(gl, RENDER_SCALE_FRAGMENTSHADER_SOURCE)?;
		let source_texture_location = fullscreen_pass.get_uniform_location(gl, "sourceTexture");
		let source_texture = unsafe {
			let source_texture = gl.create_texture()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(source_texture));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.bind_texture(glow::TEXTURE_2D, None);
			source_texture
		};
		Ok(RenderScaler {
			fullscreen_pass,
			source_texture,
			source_texture_location,
		})
	}

	//==============================================================================
	// 왼쪽 아래 렌더 영역(render_width x render_height)을 창 전체(window_width x window_height)로 늘여 그리기.
	// - 건드린 GL 상태(뷰포트, 블렌드, 스텐실, 시저, 깊이)는 되돌린다. 프로그램과 텍스처는 Graphic 이 다음 프레임 첫머리에 다시 건다.
	//==============================================================================
	pub(crate) fn present(&self, gl: &glow::Context, render_width: i32, render_height: i32, window_width: i32, window_height: i32) {
		unsafe {
			let is_blend_enabled = gl.is_enabled(glow::BLEND);
			let is_stencil_test_enabled = gl.is_enabled(glow::STENCIL_TEST);
			let is_scissor_test_enabled = gl.is_enabled(glow::SCISSOR_TEST);
			let is_depth_test_enabled = gl.is_enabled(glow::DEPTH_TEST);
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, Some(self.source_texture));
			gl.copy_tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA, 0, 0, render_width, render_height, 0);
			gl.disable(glow::BLEND);
			gl.disable(glow::STENCIL_TEST);
			gl.disable(glow::SCISSOR_TEST);
			gl.disable(glow::DEPTH_TEST);
			gl.viewport(0, 0, window_width, window_height);
			self.fullscreen_pass.use_program(gl);
			gl.uniform_1_i32(self.source_texture_location.as_ref(), 0);
			self.fullscreen_pass.draw(gl);
			gl.viewport(0, 0, render_width, render_height);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.bind_vertex_array(None);
			if is_blend_enabled {
				gl.enable(glow::BLEND);
			}
			if is_stencil_test_enabled {
				gl.enable(glow::STENCIL_TEST);
			}
			if is_scissor_test_enabled {
				gl.enable(glow::SCISSOR_TEST);
			}
			if is_depth_test_enabled {
				gl.enable(glow::DEPTH_TEST);
			}
		}
	}
}
