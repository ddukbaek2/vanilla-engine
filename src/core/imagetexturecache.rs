//==============================================================================
// 이미지 텍스처 캐시.
// - 원본은 WeakMap<image, texture> 로 원본 이미지가 수거되면 텍스처도 같이 사라진다.
// - 여기서는 Weak<Image> 를 들고, sweep() 이 원본이 사라진 텍스처를 지운다.
//==============================================================================
use crate::core::image::Image;
use glow::HasContext;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

struct ImageTextureEntry {
	image: Weak<Image>,
	texture: crate::core::graphic::GlTexture,
	is_smoothing: bool,
	is_mipmap: bool,
}

pub struct ImageTextureCache {
	entry_table: HashMap<u64, ImageTextureEntry>,
}

impl ImageTextureCache {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ImageTextureCache {
		ImageTextureCache {
			entry_table: HashMap::new(),
		}
	}

	//==============================================================================
	// 이미 올라가 있고 필터 설정도 같은 텍스처 찾기. (GL 을 건드리지 않는다, 없으면 None)
	//==============================================================================
	pub(crate) fn find_texture(&self, image: &Image, is_smoothing: bool, is_mipmap: bool) -> Option<crate::core::graphic::GlTexture> {
		let entry = self.entry_table.get(&image.get_instance_id())?;
		if entry.is_smoothing != is_smoothing || entry.is_mipmap != is_mipmap {
			return None;
		}
		Some(entry.texture)
	}

	//==============================================================================
	// 이미지의 텍스처 반환. (없으면 올리고, 필터 설정이 바뀌었으면 다시 건다)
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
				glow::PixelUnpackData::Slice(Some(pixels)),
			);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.generate_mipmap(glow::TEXTURE_2D);
			apply_filter(gl, is_smoothing, is_mipmap);
			texture
		};
		self.entry_table.insert(instance_id, ImageTextureEntry {
			image: Rc::downgrade(image),
			texture,
			is_smoothing,
			is_mipmap,
		});
		Some(texture)
	}

	//==============================================================================
	// 원본 이미지가 사라진 텍스처 지우기.
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
	// 올라가 있는 텍스처 수.
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
// 텍스처 필터 적용. (스무딩 끄면 도트가 또렷한 NEAREST)
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
