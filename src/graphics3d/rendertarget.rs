//==============================================================================
// 렌더 타겟. (원본 experimental/graphics/rendertarget.js)
// - 오프스크린 프레임버퍼 + 컬러 텍스처 [+ 깊이 렌더버퍼 / 깊이 텍스처]. 포스트 프로세싱 체인의 기본 단위다.
// - 옵션으로 다중 컬러 어태치먼트(MRT) / 반정밀 실수 컬러(HDR) / 깊이 텍스처를 켤 수 있다.
//   원본 옵션 객체 { color_attachment_count, use_float_color, use_depth_texture } 는 RenderTargetOptions 로 옮겼다.
// - 원본은 RGBA16F 를 그리려고 EXT_color_buffer_float 를 켠다. 데스크탑 GL 3.3 은 기본으로 되고, 웹은 glow 가 컨텍스트를
//   만들 때 켜 둔다.
// - GL 객체를 못 만들면 원본은 그대로 진행하지만, 러스트 판은 오류(Result)로 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::{GlFramebuffer, GlRenderbuffer};
use glow::HasContext;

//==============================================================================
// 렌더 타겟 옵션. (원본 options 객체, 기본값은 컬러 1장 / RGBA8 / 깊이 렌더버퍼)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderTargetOptions {
	pub color_attachment_count: usize,
	pub use_float_color: bool,
	pub use_depth_texture: bool,
}

impl Default for RenderTargetOptions {
	//==============================================================================
	// 기본 옵션. (원본 options = null)
	//==============================================================================
	fn default() -> RenderTargetOptions {
		RenderTargetOptions {
			color_attachment_count: 1,
			use_float_color: false,
			use_depth_texture: false,
		}
	}
}

//==============================================================================
// 렌더 타겟.
//==============================================================================
pub struct RenderTarget {
	framebuffer: Option<GlFramebuffer>,
	color_texture_list: Vec<GlTexture>,
	depth_renderbuffer: Option<GlRenderbuffer>,
	depth_texture: Option<GlTexture>,
	width: i32,
	height: i32,
	use_depth: bool,
	color_attachment_count: usize,
	use_float_color: bool,
	use_depth_texture: bool,
}

impl RenderTarget {
	//==============================================================================
	// 생성. (options 가 None 이면 기본 옵션)
	//==============================================================================
	pub fn new(gl: &glow::Context, width: i32, height: i32, use_depth: bool, options: Option<RenderTargetOptions>) -> Result<RenderTarget, String> {
		let resolved_options = options.unwrap_or_default();
		let color_attachment_count = if resolved_options.color_attachment_count > 0 {
			resolved_options.color_attachment_count
		}
		else {
			1
		};
		let mut render_target = RenderTarget {
			framebuffer: None,
			color_texture_list: Vec::new(),
			depth_renderbuffer: None,
			depth_texture: None,
			width: 0,
			height: 0,
			use_depth,
			color_attachment_count,
			use_float_color: resolved_options.use_float_color,
			use_depth_texture: resolved_options.use_depth_texture,
		};
		render_target.resize(gl, width, height)?;
		Ok(render_target)
	}

	//==============================================================================
	// 크기 변경. (같은 크기면 무시, 다르면 리소스 재생성)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		if width == self.width && height == self.height {
			return Ok(());
		}

		self.destroy(gl);

		unsafe {
			let framebuffer = gl.create_framebuffer()?;
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));

			// 컬러 어태치먼트. (개수만큼 텍스처 생성, 실수 컬러는 RGBA16F)
			let internal_format = if self.use_float_color {
				glow::RGBA16F
			}
			else {
				glow::RGBA8
			};
			let component_type = if self.use_float_color {
				glow::HALF_FLOAT
			}
			else {
				glow::UNSIGNED_BYTE
			};
			let mut color_texture_list = Vec::new();
			let mut draw_buffer_list = Vec::new();
			for attachment_index in 0..self.color_attachment_count {
				let color_texture = gl.create_texture()?;
				gl.bind_texture(glow::TEXTURE_2D, Some(color_texture));
				gl.tex_image_2d(glow::TEXTURE_2D, 0, internal_format as i32, width, height, 0, glow::RGBA, component_type, glow::PixelUnpackData::Slice(None));
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
				let attachment_point = glow::COLOR_ATTACHMENT0 + attachment_index as u32;
				gl.framebuffer_texture_2d(glow::FRAMEBUFFER, attachment_point, glow::TEXTURE_2D, Some(color_texture), 0);
				color_texture_list.push(color_texture);
				draw_buffer_list.push(attachment_point);
			}
			if self.color_attachment_count > 1 {
				gl.draw_buffers(&draw_buffer_list);
			}

			// 깊이 어태치먼트. (텍스처, 이후 패스에서 샘플링 가능 / 렌더버퍼, 깊이 테스트 전용)
			let mut depth_renderbuffer = None;
			let mut depth_texture = None;
			if self.use_depth {
				if self.use_depth_texture {
					let created_depth_texture = gl.create_texture()?;
					gl.bind_texture(glow::TEXTURE_2D, Some(created_depth_texture));
					gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, width, height, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
					gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(created_depth_texture), 0);
					depth_texture = Some(created_depth_texture);
				}
				else {
					let created_depth_renderbuffer = gl.create_renderbuffer()?;
					gl.bind_renderbuffer(glow::RENDERBUFFER, Some(created_depth_renderbuffer));
					gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT16, width, height);
					gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(created_depth_renderbuffer));
					depth_renderbuffer = Some(created_depth_renderbuffer);
				}
			}

			gl.bind_framebuffer(glow::FRAMEBUFFER, None);

			self.framebuffer = Some(framebuffer);
			self.color_texture_list = color_texture_list;
			self.depth_renderbuffer = depth_renderbuffer;
			self.depth_texture = depth_texture;
		}
		self.width = width;
		self.height = height;
		Ok(())
	}

	//==============================================================================
	// 렌더링 대상으로 바인드. (프레임버퍼 + 뷰포트)
	//==============================================================================
	pub fn bind(&self, gl: &glow::Context) {
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, self.framebuffer);
			gl.viewport(0, 0, self.width, self.height);
		}
	}

	//==============================================================================
	// 파괴. (GL 리소스 해제)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		unsafe {
			if let Some(framebuffer) = self.framebuffer.take() {
				gl.delete_framebuffer(framebuffer);
			}
			for color_texture in self.color_texture_list.drain(..) {
				gl.delete_texture(color_texture);
			}
			if let Some(depth_renderbuffer) = self.depth_renderbuffer.take() {
				gl.delete_renderbuffer(depth_renderbuffer);
			}
			if let Some(depth_texture) = self.depth_texture.take() {
				gl.delete_texture(depth_texture);
			}
		}
		self.width = 0;
		self.height = 0;
	}

	//==============================================================================
	// 프레임버퍼 반환.
	//==============================================================================
	pub fn get_framebuffer(&self) -> Option<GlFramebuffer> {
		self.framebuffer
	}

	//==============================================================================
	// 컬러 텍스처 반환. (어태치먼트 인덱스, 원본 기본값 0)
	//==============================================================================
	pub fn get_color_texture(&self, attachment_index: usize) -> Option<GlTexture> {
		self.color_texture_list.get(attachment_index).copied()
	}

	//==============================================================================
	// 깊이 텍스처 반환. (use_depth_texture 옵션일 때만 존재)
	//==============================================================================
	pub fn get_depth_texture(&self) -> Option<GlTexture> {
		self.depth_texture
	}

	//==============================================================================
	// 컬러 어태치먼트 개수 반환.
	//==============================================================================
	pub fn get_color_attachment_count(&self) -> usize {
		self.color_attachment_count
	}

	//==============================================================================
	// 실수 컬러 사용 여부 반환.
	//==============================================================================
	pub fn get_use_float_color(&self) -> bool {
		self.use_float_color
	}

	//==============================================================================
	// 깊이 텍스처 사용 여부 반환.
	//==============================================================================
	pub fn get_use_depth_texture(&self) -> bool {
		self.use_depth_texture
	}

	//==============================================================================
	// 가로 크기 반환.
	//==============================================================================
	pub fn get_width(&self) -> i32 {
		self.width
	}

	//==============================================================================
	// 세로 크기 반환.
	//==============================================================================
	pub fn get_height(&self) -> i32 {
		self.height
	}
}
