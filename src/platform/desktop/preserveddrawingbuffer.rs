//==============================================================================
//! 데스크탑의 드로잉 버퍼 남기기. (EngineConfiguration.preserve_drawing_buffer, 원본 WebGL 컨텍스트 옵션 preserveDrawingBuffer)
//! - 원본은 이 옵션을 켜면 브라우저가 프레임 뒤에 드로잉 버퍼를 비우지 않고, 엔진도 프레임 첫머리에 색을 비우지 않는다. (graphic.js applySettings 는 스텐실만 비운다)
//!   그래서 화면을 다 덮지 않는 씬(배경색이 반투명한 GameScene, 배경을 칠하지 않는 Scene)은 지난 프레임 그림 위에 쌓아 그린다.
//! - 창의 뒷 버퍼는 화면을 넘긴 뒤 내용이 정해져 있지 않다. (EGL 기본 EGL_BUFFER_DESTROYED, 겹 버퍼면 두 프레임 전 그림이 남기도 한다)
//!   그래서 프레임을 다 그린 뒤(늘이개가 창 크기로 늘이기 전) 렌더 영역을 텍스처로 떠 두고, 다음 프레임 첫머리에 같은 크기로 되그려 놓는다.
//!   크롬도 이 옵션을 켜면 드로잉 버퍼를 한 번 더 복사해 화면에 낸다. (원본 엔진 설정 주석의 "한 번 더 복사하는 만큼 비용")
//! - 기본 프레임버퍼에 그대로 그리므로 화면 효과와 3D 패스가 기본 프레임버퍼로 되돌아가도 그대로 맞다. (늘이개와 같은 방법)
//! - 렌더 크기가 바뀌면 원본처럼 비운다. (브라우저도 캔버스 width, height 를 바꾸면 드로잉 버퍼를 새로 만들어 비운다) 처음 프레임도 비운다.
//==============================================================================
use crate::core::glcontext::GlContext;
use crate::core::fullscreenpass::FullscreenPass;
use crate::core::graphic::{GlTexture, GlUniformLocation};

/// 떠 둔 그림을 그대로 되그리는 셰이더.
const RESTORE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
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
/// 드로잉 버퍼 남기개.
//==============================================================================
pub(crate) struct PreservedDrawingBuffer {
	fullscreen_pass: FullscreenPass,
	saved_texture: GlTexture,
	source_texture_location: Option<GlUniformLocation>,
	/// 떠 둔 그림의 크기. (렌더 픽셀, 아직 없으면 0)
	saved_width: i32,
	saved_height: i32,
}

impl PreservedDrawingBuffer {
	//==============================================================================
	/// 생성. (셰이더와 텍스처, 같은 크기로 되그리므로 가장 가까운 픽셀로 거른다)
	//==============================================================================
	pub(crate) fn new(gl: &GlContext) -> Result<PreservedDrawingBuffer, String> {
		let fullscreen_pass = FullscreenPass::new(gl, RESTORE_FRAGMENTSHADER_SOURCE)?;
		let source_texture_location = fullscreen_pass.get_uniform_location(gl, "sourceTexture");
		let saved_texture = unsafe {
			let saved_texture = crate::core::glstatecache::create_texture(gl)?;
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(saved_texture));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			saved_texture
		};
		Ok(PreservedDrawingBuffer {
			fullscreen_pass,
			saved_texture,
			source_texture_location,
			saved_width: 0,
			saved_height: 0,
		})
	}

	//==============================================================================
	/// 다 그린 프레임 떠 두기. (기본 프레임버퍼의 왼쪽 아래 렌더 영역, 늘이개가 창 크기로 늘이기 전에 부른다)
	//==============================================================================
	pub(crate) fn save(&mut self, gl: &GlContext, render_width: i32, render_height: i32) {
		if render_width <= 0 || render_height <= 0 {
			return;
		}
		unsafe {
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, None);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(self.saved_texture));
			gl.copy_tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA, 0, 0, render_width, render_height, 0);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
		}
		self.saved_width = render_width;
		self.saved_height = render_height;
	}

	//==============================================================================
	/// 지난 프레임 그림 되살리기. (엔진 프레임 첫머리, 크기가 다르거나 아직 떠 둔 것이 없으면 원본처럼 비운다)
	/// - 뷰포트는 렌더 영역으로 둔다. (Graphic 이 프레임 첫머리에 거는 것과 같다) 건드린 블렌드, 스텐실, 시저, 깊이는 되돌린다.
	///   프로그램과 텍스처, 버텍스 어레이는 Graphic 이 프레임 첫머리에 다시 건다.
	//==============================================================================
	pub(crate) fn restore(&self, gl: &GlContext, render_width: i32, render_height: i32) {
		if render_width <= 0 || render_height <= 0 {
			return;
		}
		unsafe {
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, None);
			crate::core::glstatecache::color_mask(gl, true, true, true, true);
			if self.saved_width != render_width || self.saved_height != render_height {
				let is_scissor_test_enabled = crate::core::glstatecache::is_enabled(gl, glow::SCISSOR_TEST);
				crate::core::glstatecache::disable(gl, glow::SCISSOR_TEST);
				crate::core::glstatecache::clear_color(gl, 0.0, 0.0, 0.0, 1.0);
				gl.clear(glow::COLOR_BUFFER_BIT);
				if is_scissor_test_enabled {
					crate::core::glstatecache::enable(gl, glow::SCISSOR_TEST);
				}
				return;
			}
			let is_blend_enabled = crate::core::glstatecache::is_enabled(gl, glow::BLEND);
			let is_stencil_test_enabled = crate::core::glstatecache::is_enabled(gl, glow::STENCIL_TEST);
			let is_scissor_test_enabled = crate::core::glstatecache::is_enabled(gl, glow::SCISSOR_TEST);
			let is_depth_test_enabled = crate::core::glstatecache::is_enabled(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::disable(gl, glow::BLEND);
			crate::core::glstatecache::disable(gl, glow::STENCIL_TEST);
			crate::core::glstatecache::disable(gl, glow::SCISSOR_TEST);
			crate::core::glstatecache::disable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::viewport(gl, 0, 0, render_width, render_height);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(self.saved_texture));
			self.fullscreen_pass.use_program(gl);
			gl.uniform_1_i32(self.source_texture_location.as_ref(), 0);
			self.fullscreen_pass.draw(gl);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			crate::core::glstatecache::bind_vertex_array(gl, None);
			if is_blend_enabled {
				crate::core::glstatecache::enable(gl, glow::BLEND);
			}
			if is_stencil_test_enabled {
				crate::core::glstatecache::enable(gl, glow::STENCIL_TEST);
			}
			if is_scissor_test_enabled {
				crate::core::glstatecache::enable(gl, glow::SCISSOR_TEST);
			}
			if is_depth_test_enabled {
				crate::core::glstatecache::enable(gl, glow::DEPTH_TEST);
			}
		}
	}
}
