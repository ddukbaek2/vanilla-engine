//==============================================================================
//! 이미지 텍스처 캐시.
//! - 원본은 `WeakMap<image, texture>` 로 원본 이미지가 수거되면 텍스처도 같이 사라진다.
//! - 여기서는 `Weak<Image>` 를 들고, sweep() 이 원본이 사라진 텍스처를 지운다.
//! - 올린 뒤에는 이미지의 CPU 픽셀을 놓는다. (Image.set_pixels_retained 로 남기게 한 그림은 둔다)
//! - 놓은 픽셀이 다시 필요하면(Canvas2D 의 drawImage, createPattern) 그림이 기억한 텍스처를 GPU 에서 되읽는다. (restore_image_pixels_from_texture)
//!   크롬이 GPU 에 있는 그림을 CPU 캔버스에 그릴 때 텍스처를 되읽는 것과 같은 길이다. 되읽을 GL 컨텍스트는 Graphic 이 만들 때 건다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::core::image::Image;
use glow::HasContext;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

thread_local! {
	// 그림 텍스처를 되읽을 GL 컨텍스트. (Graphic 이 만들 때 건다, Graphic 이 사라지면 비어 있다)
	static PIXEL_READ_CONTEXT: RefCell<Weak<glow::Context>> = RefCell::new(Weak::new());
}

//==============================================================================
// 그림 텍스처를 되읽을 GL 컨텍스트 걸기. (Graphic::new)
//==============================================================================
pub(crate) fn register_pixel_read_context(gl: &Rc<glow::Context>) {
	PIXEL_READ_CONTEXT.with(|context_cell| {
		*context_cell.borrow_mut() = Rc::downgrade(gl);
	});
}

//==============================================================================
// 텍스처 픽셀 읽기. (올린 그대로의 프리멀티플라이드 RGBA, 위에서 아래 차례. 못 읽으면 None)
// - 텍스처를 잠깐 만든 프레임버퍼에 붙여 읽고, 그 전에 묶여 있던 프레임버퍼로 되돌린다. 묶인 텍스처는 바꾸지 않는다.
//==============================================================================
pub(crate) fn read_texture_pixels(gl: &glow::Context, texture: GlTexture, width: u32, height: u32) -> Option<Vec<u8>> {
	let mut pixels = vec![0u8; width as usize * height as usize * 4];
	unsafe {
		let previous_framebuffer = crate::core::graphic::read_framebuffer_binding(gl);
		let framebuffer = gl.create_framebuffer().ok()?;
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
		let is_complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
		if is_complete {
			gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
			gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
		}
		gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
		gl.delete_framebuffer(framebuffer);
		if !is_complete {
			return None;
		}
	}
	Some(pixels)
}

//==============================================================================
// GPU 에 올리며 놓은 그림의 CPU 픽셀을 그림이 기억한 텍스처에서 되읽어 되돌린다. (픽셀이 있으면 그대로 true)
// - 올린 적이 없거나, Graphic 이 없거나, 읽지 못하면 false. 되돌린 픽셀은 그림이 계속 든다. (다음에 또 되읽지 않는다)
//==============================================================================
pub(crate) fn restore_image_pixels_from_texture(image: &Image) -> bool {
	if image.has_pixels() {
		return true;
	}
	let texture = match image.get_uploaded_texture() {
		Some(texture) => {
			texture
		}
		None => {
			return false;
		}
	};
	let gl = match PIXEL_READ_CONTEXT.with(|context_cell| context_cell.borrow().upgrade()) {
		Some(gl) => {
			gl
		}
		None => {
			return false;
		}
	};
	match read_texture_pixels(&gl, texture, image.get_width(), image.get_height()) {
		Some(pixels) => {
			image.restore_pixels(pixels);
			true
		}
		None => {
			false
		}
	}
}

struct ImageTextureEntry {
	image: Weak<Image>,
	texture: GlTexture,
	is_smoothing: bool,
	is_mipmap: bool,
}

pub struct ImageTextureCache {
	entry_table: HashMap<u64, ImageTextureEntry>,
}

impl ImageTextureCache {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> ImageTextureCache {
		ImageTextureCache {
			entry_table: HashMap::new(),
		}
	}

	//==============================================================================
	/// 이미 올라가 있고 필터 설정도 같은 텍스처 찾기. (GL 을 건드리지 않는다, 없으면 None)
	//==============================================================================
	pub(crate) fn find_texture(&self, image: &Image, is_smoothing: bool, is_mipmap: bool) -> Option<crate::core::graphic::GlTexture> {
		let entry = self.entry_table.get(&image.get_instance_id())?;
		if entry.is_smoothing != is_smoothing || entry.is_mipmap != is_mipmap {
			return None;
		}
		Some(entry.texture)
	}

	//==============================================================================
	/// 이미 올라가 있는 텍스처 찾기. (필터 설정은 보지 않는다, GL 을 건드리지 않는다)
	//==============================================================================
	pub(crate) fn find_uploaded_texture(&self, image: &Image) -> Option<crate::core::graphic::GlTexture> {
		self.entry_table.get(&image.get_instance_id()).map(|entry| entry.texture)
	}

	//==============================================================================
	/// 이미지의 텍스처 반환. (없으면 올리고 CPU 픽셀을 놓는다, 필터 설정이 바뀌었으면 다시 건다)
	//==============================================================================
	pub fn get_texture(&mut self, gl: &glow::Context, image: &Rc<Image>, is_smoothing: bool, is_mipmap: bool) -> Option<crate::core::graphic::GlTexture> {
		let instance_id = image.get_instance_id();
		if let Some(entry) = self.entry_table.get_mut(&instance_id) {
			if entry.is_smoothing != is_smoothing || entry.is_mipmap != is_mipmap {
				unsafe {
					gl.bind_texture(glow::TEXTURE_2D, Some(entry.texture));
					apply_filter(gl, is_smoothing, is_mipmap);
				}
				entry.is_smoothing = is_smoothing;
				entry.is_mipmap = is_mipmap;
			}
			return Some(entry.texture);
		}

		let texture = unsafe {
			let texture = gl.create_texture().ok()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			let pixels = image.get_pixels();
			gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
			gl.tex_image_2d(
				glow::TEXTURE_2D,
				0,
				glow::RGBA8 as i32,
				image.get_width() as i32,
				image.get_height() as i32,
				0,
				glow::RGBA,
				glow::UNSIGNED_BYTE,
				glow::PixelUnpackData::Slice(Some(&pixels)),
			);
			drop(pixels);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.generate_mipmap(glow::TEXTURE_2D);
			apply_filter(gl, is_smoothing, is_mipmap);
			texture
		};
		image.release_pixels(texture);
		self.entry_table.insert(instance_id, ImageTextureEntry {
			image: Rc::downgrade(image),
			texture,
			is_smoothing,
			is_mipmap,
		});
		Some(texture)
	}

	//==============================================================================
	/// 원본 이미지가 사라진 텍스처 지우기.
	//==============================================================================
	pub fn sweep(&mut self, gl: &glow::Context) {
		self.entry_table.retain(|_, entry| {
			let is_alive = entry.image.strong_count() > 0;
			if !is_alive {
				unsafe {
					gl.delete_texture(entry.texture);
				}
			}
			is_alive
		});
	}

	//==============================================================================
	/// 올라가 있는 텍스처 수.
	//==============================================================================
	pub fn get_texture_count(&self) -> usize {
		self.entry_table.len()
	}
}

impl Default for ImageTextureCache {
	fn default() -> ImageTextureCache {
		ImageTextureCache::new()
	}
}

//==============================================================================
/// 텍스처 필터 적용. (스무딩 끄면 도트가 또렷한 NEAREST)
//==============================================================================
unsafe fn apply_filter(gl: &glow::Context, is_smoothing: bool, is_mipmap: bool) {
	let magnify_filter = if is_smoothing {
		glow::LINEAR
	}
	else {
		glow::NEAREST
	};
	let minify_filter = if is_mipmap {
		if is_smoothing {
			glow::LINEAR_MIPMAP_LINEAR
		}
		else {
			glow::NEAREST_MIPMAP_NEAREST
		}
	}
	else {
		magnify_filter
	};
	gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, minify_filter as i32);
	gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, magnify_filter as i32);
}
