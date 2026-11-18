//==============================================================================
// 렌더 타겟. (원본 experimental/graphics/rendertarget.js)
// - 오프스크린 프레임버퍼 + 컬러 텍스처 [+ 깊이 렌더버퍼 / 깊이 텍스처]. 포스트 프로세싱 체인의 기본 단위다.
// - 옵션으로 다중 컬러 어태치먼트(MRT) / 반정밀 실수 컬러(HDR) / 깊이 텍스처를 켤 수 있다.
//   원본 옵션 객체 { colorAttachmentCount, useFloatColor, useDepthTexture } 는 RenderTargetOptions 로 옮겼다.
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
	pub colorAttachmentCount: usize,
	pub useFloatColor: bool,
	pub useDepthTexture: bool,
}

impl Default for RenderTargetOptions {
	//==============================================================================
	// 기본 옵션. (원본 options = null)
	//==============================================================================
	fn default() -> RenderTargetOptions {
		RenderTargetOptions {
			colorAttachmentCount: 1,
			useFloatColor: false,
			useDepthTexture: false,
		}
	}
}

//==============================================================================
// 렌더 타겟.
//==============================================================================
pub struct RenderTarget {
	framebuffer: Option<GlFramebuffer>,
	colorTextureList: Vec<GlTexture>,
	depthRenderbuffer: Option<GlRenderbuffer>,
	depthTexture: Option<GlTexture>,
	width: i32,
	height: i32,
	useDepth: bool,
	colorAttachmentCount: usize,
	useFloatColor: bool,
	useDepthTexture: bool,
}

impl RenderTarget {
	//==============================================================================
	// 생성. (options 가 None 이면 기본 옵션)
	//==============================================================================
	pub fn new(gl: &glow::Context, width: i32, height: i32, useDepth: bool, options: Option<RenderTargetOptions>) -> Result<RenderTarget, String> {
		let resolvedOptions = options.unwrap_or_default();
		let colorAttachmentCount = if resolvedOptions.colorAttachmentCount > 0 {
			resolvedOptions.colorAttachmentCount
		}
		else {
			1
		};
		let mut renderTarget = RenderTarget {
			framebuffer: None,
			colorTextureList: Vec::new(),
			depthRenderbuffer: None,
			depthTexture: None,
			width: 0,
			height: 0,
			useDepth,
			colorAttachmentCount,
			useFloatColor: resolvedOptions.useFloatColor,
			useDepthTexture: resolvedOptions.useDepthTexture,
		};
		renderTarget.resize(gl, width, height)?;
		Ok(renderTarget)
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
			let internalFormat = if self.useFloatColor {
				glow::RGBA16F
			}
			else {
				glow::RGBA8
			};
			let componentType = if self.useFloatColor {
				glow::HALF_FLOAT
			}
			else {
				glow::UNSIGNED_BYTE
			};
			let mut colorTextureList = Vec::new();
			let mut drawBufferList = Vec::new();
			for attachmentIndex in 0..self.colorAttachmentCount {
				let colorTexture = gl.create_texture()?;
				gl.bind_texture(glow::TEXTURE_2D, Some(colorTexture));
				gl.tex_image_2d(glow::TEXTURE_2D, 0, internalFormat as i32, width, height, 0, glow::RGBA, componentType, glow::PixelUnpackData::Slice(None));
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
				let attachmentPoint = glow::COLOR_ATTACHMENT0 + attachmentIndex as u32;
				gl.framebuffer_texture_2d(glow::FRAMEBUFFER, attachmentPoint, glow::TEXTURE_2D, Some(colorTexture), 0);
				colorTextureList.push(colorTexture);
				drawBufferList.push(attachmentPoint);
			}
			if self.colorAttachmentCount > 1 {
				gl.draw_buffers(&drawBufferList);
			}

			// 깊이 어태치먼트. (텍스처, 이후 패스에서 샘플링 가능 / 렌더버퍼, 깊이 테스트 전용)
			let mut depthRenderbuffer = None;
			let mut depthTexture = None;
			if self.useDepth {
				if self.useDepthTexture {
					let createdDepthTexture = gl.create_texture()?;
					gl.bind_texture(glow::TEXTURE_2D, Some(createdDepthTexture));
					gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, width, height, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
					gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(createdDepthTexture), 0);
					depthTexture = Some(createdDepthTexture);
				}
				else {
					let createdDepthRenderbuffer = gl.create_renderbuffer()?;
					gl.bind_renderbuffer(glow::RENDERBUFFER, Some(createdDepthRenderbuffer));
					gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT16, width, height);
					gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(createdDepthRenderbuffer));
					depthRenderbuffer = Some(createdDepthRenderbuffer);
				}
			}

			gl.bind_framebuffer(glow::FRAMEBUFFER, None);

			self.framebuffer = Some(framebuffer);
			self.colorTextureList = colorTextureList;
			self.depthRenderbuffer = depthRenderbuffer;
			self.depthTexture = depthTexture;
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
			for colorTexture in self.colorTextureList.drain(..) {
				gl.delete_texture(colorTexture);
			}
			if let Some(depthRenderbuffer) = self.depthRenderbuffer.take() {
				gl.delete_renderbuffer(depthRenderbuffer);
			}
			if let Some(depthTexture) = self.depthTexture.take() {
				gl.delete_texture(depthTexture);
			}
		}
		self.width = 0;
		self.height = 0;
	}

	//==============================================================================
	// 프레임버퍼 반환.
	//==============================================================================
	pub fn getFramebuffer(&self) -> Option<GlFramebuffer> {
		self.framebuffer
	}

	//==============================================================================
	// 컬러 텍스처 반환. (어태치먼트 인덱스, 원본 기본값 0)
	//==============================================================================
	pub fn getColorTexture(&self, attachmentIndex: usize) -> Option<GlTexture> {
		self.colorTextureList.get(attachmentIndex).copied()
	}

	//==============================================================================
	// 깊이 텍스처 반환. (useDepthTexture 옵션일 때만 존재)
	//==============================================================================
	pub fn getDepthTexture(&self) -> Option<GlTexture> {
		self.depthTexture
	}

	//==============================================================================
	// 컬러 어태치먼트 개수 반환.
	//==============================================================================
	pub fn getColorAttachmentCount(&self) -> usize {
		self.colorAttachmentCount
	}

	//==============================================================================
	// 실수 컬러 사용 여부 반환.
	//==============================================================================
	pub fn getUseFloatColor(&self) -> bool {
		self.useFloatColor
	}

	//==============================================================================
	// 깊이 텍스처 사용 여부 반환.
	//==============================================================================
	pub fn getUseDepthTexture(&self) -> bool {
		self.useDepthTexture
	}

	//==============================================================================
	// 가로 크기 반환.
	//==============================================================================
	pub fn getWidth(&self) -> i32 {
		self.width
	}

	//==============================================================================
	// 세로 크기 반환.
	//==============================================================================
	pub fn getHeight(&self) -> i32 {
		self.height
	}
}
