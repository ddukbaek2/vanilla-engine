//==============================================================================
//! 렌더 타겟. (원본 experimental/graphics/rendertarget.js)
//! - 오프스크린 프레임버퍼 + 컬러 텍스처 [+ 깊이 렌더버퍼 / 깊이 텍스처]. 포스트 프로세싱 체인의 기본 단위다.
//! - 옵션으로 다중 컬러 어태치먼트(MRT) / 반정밀 실수 컬러(HDR) / 깊이 텍스처 / 멀티샘플 / 깊이 비트를 켤 수 있다.
//!   원본 옵션 객체 { colorAttachmentCount, useFloatColor, useDepthTexture, sampleCount, depthBits } 는 RenderTargetOptions 로 옮겼다.
//! - sample_count 가 2 이상이면 멀티샘플(MSAA) 렌더버퍼에 그리고 resolve() 가 컬러 텍스처(와 깊이 텍스처)로 푼다.
//!   기기가 받는 수(MAX_SAMPLES, 웹은 포맷마다 받는 표본 수도)를 넘으면 그 밑의 가장 큰 수로 줄이고, 못 받거나 프레임버퍼가 완성되지 않으면 멀티샘플 없이 짓는다. (get_sample_count)
//!   데스크탑 GL 3.3 에는 포맷마다 받는 표본 수를 묻는 함수(glGetInternalformativ, GL 4.2)가 없어 MAX_SAMPLES 만 본다. (3.3 은 실수, 정수가 아닌 색 포맷이면 MAX_SAMPLES 까지 받는다)
//!   멀티샘플이면 get_framebuffer 는 그리는 멀티샘플 프레임버퍼, get_resolve_framebuffer 는 텍스처가 붙은 프레임버퍼다. 텍스처를 읽기 전에 resolve() 를 부른다.
//! - depth_bits 는 깊이 렌더버퍼의 비트 수(16 이나 24)다. 깊이 텍스처는 늘 24 비트다. 멀티샘플이면서 깊이 텍스처를 쓰면 멀티샘플 깊이도 24 비트로 지어 그대로 푼다.
//! - 원본은 RGBA16F 를 그리려고 EXT_color_buffer_float 를 켠다. 데스크탑 GL 3.3 은 기본으로 되고, 웹은 glow 가 컨텍스트를
//!   만들 때 켜 둔다.
//! - GL 객체를 못 만들면 원본은 그대로 진행하지만, 러스트 판은 오류(Result)로 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::{read_framebuffer_binding, GlFramebuffer, GlRenderbuffer};
use glow::HasContext;

//==============================================================================
/// 렌더 타겟 옵션. (원본 options 객체, 기본값은 컬러 1장 / RGBA8 / 깊이 렌더버퍼 16 비트 / 멀티샘플 없음)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderTargetOptions {
	pub color_attachment_count: usize,
	pub use_float_color: bool,
	pub use_depth_texture: bool,
	/// 멀티샘플 표본 수. (2 밑이면 멀티샘플 없음)
	pub sample_count: i32,
	/// 깊이 렌더버퍼 비트 수. (24 면 24, 그 밖은 16)
	pub depth_bits: i32,
}

impl Default for RenderTargetOptions {
	//==============================================================================
	/// 기본 옵션. (원본 options = null)
	//==============================================================================
	fn default() -> RenderTargetOptions {
		RenderTargetOptions {
			color_attachment_count: 1,
			use_float_color: false,
			use_depth_texture: false,
			sample_count: 0,
			depth_bits: 16,
		}
	}
}

//==============================================================================
/// 렌더 타겟.
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
	/// 옵션으로 받은 표본 수.
	requested_sample_count: i32,
	/// 실제로 지은 표본 수. (0 이면 멀티샘플 없음)
	sample_count: i32,
	depth_bits: i32,
	multisample_framebuffer: Option<GlFramebuffer>,
	multisample_color_renderbuffer_list: Vec<GlRenderbuffer>,
	multisample_depth_renderbuffer: Option<GlRenderbuffer>,
}

//==============================================================================
/// 포맷마다 받는 표본 수로 줄이기. (웹, 포맷마다 받는 표본 수 가운데 sample_count 밑의 가장 큰 수, 플랫폼 층이 SAMPLES 목록을 읽는다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn clamp_sample_count_to_formats(_gl: &glow::Context, sample_count: i32, internal_format_list: &[u32]) -> i32 {
	let mut clamped_sample_count = sample_count;
	for internal_format in internal_format_list {
		let supported_sample_count_list = crate::platform::read_renderbuffer_sample_counts(*internal_format);
		clamped_sample_count = supported_sample_count_list.iter().copied().filter(|supported_sample_count| *supported_sample_count <= clamped_sample_count).max().unwrap_or(0);
	}
	clamped_sample_count
}

//==============================================================================
/// 포맷마다 받는 표본 수로 줄이기. (데스크탑 GL 3.3 은 물을 함수가 없어 그대로, 머리말)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn clamp_sample_count_to_formats(_gl: &glow::Context, sample_count: i32, _internal_format_list: &[u32]) -> i32 {
	sample_count
}

impl RenderTarget {
	//==============================================================================
	/// 기기가 받는 표본 수. (요청 수를 MAX_SAMPLES 와 포맷마다 받는 표본 수 밑의 가장 큰 수로, 2 밑이면 0)
	//==============================================================================
	pub fn read_supported_sample_count(gl: &glow::Context, requested_sample_count: i32, internal_format_list: &[u32]) -> i32 {
		if requested_sample_count < 2 {
			return 0;
		}
		let maximum_sample_count = unsafe { gl.get_parameter_i32(glow::MAX_SAMPLES) };
		let sample_count = clamp_sample_count_to_formats(gl, requested_sample_count.min(maximum_sample_count), internal_format_list);
		if sample_count >= 2 {
			sample_count
		}
		else {
			0
		}
	}

	//==============================================================================
	/// 생성. (options 가 None 이면 기본 옵션)
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
			requested_sample_count: resolved_options.sample_count.max(0),
			sample_count: 0,
			depth_bits: if resolved_options.depth_bits == 24 {
				24
			}
			else {
				16
			},
			multisample_framebuffer: None,
			multisample_color_renderbuffer_list: Vec::new(),
			multisample_depth_renderbuffer: None,
		};
		render_target.resize(gl, width, height)?;
		Ok(render_target)
	}

	//==============================================================================
	/// 크기 변경. (같은 크기면 무시, 다르면 리소스 재생성)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		if width == self.width && height == self.height {
			return Ok(());
		}

		self.destroy(gl);

		let internal_format = if self.use_float_color {
			glow::RGBA16F
		}
		else {
			glow::RGBA8
		};
		let depth_renderbuffer_format = if self.depth_bits == 24 {
			glow::DEPTH_COMPONENT24
		}
		else {
			glow::DEPTH_COMPONENT16
		};

		// 멀티샘플 프레임버퍼. (컬러 렌더버퍼 개수만큼과 깊이 렌더버퍼, 완성되지 않으면 지우고 멀티샘플 없이)
		let multisample_depth_format = if self.use_depth_texture {
			glow::DEPTH_COMPONENT24
		}
		else {
			depth_renderbuffer_format
		};
		let format_list = if self.use_depth {
			vec![internal_format, multisample_depth_format]
		}
		else {
			vec![internal_format]
		};
		let mut sample_count = RenderTarget::read_supported_sample_count(gl, self.requested_sample_count, &format_list);
		if sample_count > 0 {
			unsafe {
				let multisample_framebuffer = gl.create_framebuffer()?;
				gl.bind_framebuffer(glow::FRAMEBUFFER, Some(multisample_framebuffer));
				self.multisample_framebuffer = Some(multisample_framebuffer);
				let mut multisample_draw_buffer_list = Vec::new();
				for attachment_index in 0..self.color_attachment_count {
					let color_renderbuffer = gl.create_renderbuffer()?;
					gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color_renderbuffer));
					gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, sample_count, internal_format, width, height);
					let attachment_point = glow::COLOR_ATTACHMENT0 + attachment_index as u32;
					gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, attachment_point, glow::RENDERBUFFER, Some(color_renderbuffer));
					self.multisample_color_renderbuffer_list.push(color_renderbuffer);
					multisample_draw_buffer_list.push(attachment_point);
				}
				if self.color_attachment_count > 1 {
					gl.draw_buffers(&multisample_draw_buffer_list);
				}
				if self.use_depth {
					let multisample_depth_renderbuffer = gl.create_renderbuffer()?;
					gl.bind_renderbuffer(glow::RENDERBUFFER, Some(multisample_depth_renderbuffer));
					gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, sample_count, multisample_depth_format, width, height);
					gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(multisample_depth_renderbuffer));
					self.multisample_depth_renderbuffer = Some(multisample_depth_renderbuffer);
				}
				gl.bind_renderbuffer(glow::RENDERBUFFER, None);
				if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
					self.destroy_multisample(gl);
					sample_count = 0;
				}
			}
		}
		self.sample_count = sample_count;

		unsafe {
			let framebuffer = gl.create_framebuffer()?;
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));

			// 컬러 어태치먼트. (개수만큼 텍스처 생성, 실수 컬러는 RGBA16F)
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

			// 깊이 어태치먼트. (텍스처, 이후 패스에서 샘플링 가능 / 렌더버퍼, 깊이 테스트 전용, 멀티샘플이면 깊이 시험은 멀티샘플 쪽이 맡아 렌더버퍼는 짓지 않는다)
			let mut depth_renderbuffer = None;
			let mut depth_texture = None;
			if self.use_depth && (self.use_depth_texture || sample_count == 0) {
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
					gl.renderbuffer_storage(glow::RENDERBUFFER, depth_renderbuffer_format, width, height);
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
	/// 멀티샘플을 텍스처로 풀기. (멀티샘플이 아니면 아무것도 하지 않음, 컬러 어태치먼트마다와 깊이 텍스처, 묶여 있던 프레임버퍼로 돌림)
	//==============================================================================
	pub fn resolve(&self, gl: &glow::Context) {
		let Some(multisample_framebuffer) = self.multisample_framebuffer else {
			return;
		};
		let previous_framebuffer = read_framebuffer_binding(gl);
		let width = self.width;
		let height = self.height;
		unsafe {
			gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(multisample_framebuffer));
			gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.framebuffer);
			if self.color_attachment_count > 1 {
				let mut draw_buffer_list = vec![glow::NONE; self.color_attachment_count];
				for attachment_index in 0..self.color_attachment_count {
					for (buffer_index, draw_buffer) in draw_buffer_list.iter_mut().enumerate() {
						*draw_buffer = if buffer_index == attachment_index {
							glow::COLOR_ATTACHMENT0 + buffer_index as u32
						}
						else {
							glow::NONE
						};
					}
					gl.read_buffer(glow::COLOR_ATTACHMENT0 + attachment_index as u32);
					gl.draw_buffers(&draw_buffer_list);
					gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
				}
				for (buffer_index, draw_buffer) in draw_buffer_list.iter_mut().enumerate() {
					*draw_buffer = glow::COLOR_ATTACHMENT0 + buffer_index as u32;
				}
				gl.read_buffer(glow::COLOR_ATTACHMENT0);
				gl.draw_buffers(&draw_buffer_list);
			}
			else {
				gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
			}
			if self.depth_texture.is_some() {
				gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::DEPTH_BUFFER_BIT, glow::NEAREST);
			}
			gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
		}
	}

	//==============================================================================
	/// 렌더링 대상으로 바인드. (프레임버퍼 + 뷰포트, 멀티샘플이면 멀티샘플 프레임버퍼)
	//==============================================================================
	pub fn bind(&self, gl: &glow::Context) {
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, self.get_framebuffer());
			gl.viewport(0, 0, self.width, self.height);
		}
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제)
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
		self.destroy_multisample(gl);
		self.width = 0;
		self.height = 0;
	}

	//==============================================================================
	/// 멀티샘플 프레임버퍼와 렌더버퍼 지우기.
	//==============================================================================
	fn destroy_multisample(&mut self, gl: &glow::Context) {
		unsafe {
			if let Some(multisample_framebuffer) = self.multisample_framebuffer.take() {
				gl.delete_framebuffer(multisample_framebuffer);
			}
			for color_renderbuffer in self.multisample_color_renderbuffer_list.drain(..) {
				gl.delete_renderbuffer(color_renderbuffer);
			}
			if let Some(multisample_depth_renderbuffer) = self.multisample_depth_renderbuffer.take() {
				gl.delete_renderbuffer(multisample_depth_renderbuffer);
			}
		}
		self.sample_count = 0;
	}

	//==============================================================================
	/// 프레임버퍼 반환. (그리는 프레임버퍼, 멀티샘플이면 멀티샘플 프레임버퍼)
	//==============================================================================
	pub fn get_framebuffer(&self) -> Option<GlFramebuffer> {
		if self.multisample_framebuffer.is_some() {
			self.multisample_framebuffer
		}
		else {
			self.framebuffer
		}
	}

	//==============================================================================
	/// 텍스처가 붙은 프레임버퍼 반환. (멀티샘플이 아니면 get_framebuffer 와 같음)
	//==============================================================================
	pub fn get_resolve_framebuffer(&self) -> Option<GlFramebuffer> {
		self.framebuffer
	}

	//==============================================================================
	/// 실제로 지은 표본 수 반환. (0 이면 멀티샘플 없음)
	//==============================================================================
	pub fn get_sample_count(&self) -> i32 {
		self.sample_count
	}

	//==============================================================================
	/// 깊이 렌더버퍼 비트 수 반환. (16 이나 24)
	//==============================================================================
	pub fn get_depth_bits(&self) -> i32 {
		self.depth_bits
	}

	//==============================================================================
	/// 컬러 텍스처 반환. (어태치먼트 인덱스, 원본 기본값 0)
	//==============================================================================
	pub fn get_color_texture(&self, attachment_index: usize) -> Option<GlTexture> {
		self.color_texture_list.get(attachment_index).copied()
	}

	//==============================================================================
	/// 깊이 텍스처 반환. (use_depth_texture 옵션일 때만 존재)
	//==============================================================================
	pub fn get_depth_texture(&self) -> Option<GlTexture> {
		self.depth_texture
	}

	//==============================================================================
	/// 컬러 어태치먼트 개수 반환.
	//==============================================================================
	pub fn get_color_attachment_count(&self) -> usize {
		self.color_attachment_count
	}

	//==============================================================================
	/// 실수 컬러 사용 여부 반환.
	//==============================================================================
	pub fn get_use_float_color(&self) -> bool {
		self.use_float_color
	}

	//==============================================================================
	/// 깊이 텍스처 사용 여부 반환.
	//==============================================================================
	pub fn get_use_depth_texture(&self) -> bool {
		self.use_depth_texture
	}

	//==============================================================================
	/// 가로 크기 반환.
	//==============================================================================
	pub fn get_width(&self) -> i32 {
		self.width
	}

	//==============================================================================
	/// 세로 크기 반환.
	//==============================================================================
	pub fn get_height(&self) -> i32 {
		self.height
	}
}
