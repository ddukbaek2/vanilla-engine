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
	isSmoothing: bool,
	isMipmap: bool,
}

pub struct ImageTextureCache {
	entryTable: HashMap<u64, ImageTextureEntry>,
}

impl ImageTextureCache {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ImageTextureCache {
		ImageTextureCache {
			entryTable: HashMap::new(),
		}
	}

	//==============================================================================
	// 이미지의 텍스처 반환. (없으면 올리고, 필터 설정이 바뀌었으면 다시 건다)
	//==============================================================================
	pub fn getTexture(&mut self, gl: &glow::Context, image: &Rc<Image>, isSmoothing: bool, isMipmap: bool) -> Option<crate::core::graphic::GlTexture> {
		let instanceId = image.getInstanceId();
		if let Some(entry) = self.entryTable.get_mut(&instanceId) {
			if entry.isSmoothing != isSmoothing || entry.isMipmap != isMipmap {
				unsafe {
					gl.bind_texture(glow::TEXTURE_2D, Some(entry.texture));
					applyFilter(gl, isSmoothing, isMipmap);
				}
				entry.isSmoothing = isSmoothing;
				entry.isMipmap = isMipmap;
			}
			return Some(entry.texture);
		}

		let texture = unsafe {
			let texture = gl.create_texture().ok()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			let pixels = image.getPixels();
			gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
			gl.tex_image_2d(
				glow::TEXTURE_2D,
				0,
				glow::RGBA8 as i32,
				image.getWidth() as i32,
				image.getHeight() as i32,
				0,
				glow::RGBA,
				glow::UNSIGNED_BYTE,
				glow::PixelUnpackData::Slice(Some(pixels)),
			);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.generate_mipmap(glow::TEXTURE_2D);
			applyFilter(gl, isSmoothing, isMipmap);
			texture
		};
		self.entryTable.insert(instanceId, ImageTextureEntry {
			image: Rc::downgrade(image),
			texture,
			isSmoothing,
			isMipmap,
		});
		Some(texture)
	}

	//==============================================================================
	// 원본 이미지가 사라진 텍스처 지우기.
	//==============================================================================
	pub fn sweep(&mut self, gl: &glow::Context) {
		self.entryTable.retain(|_, entry| {
			let isAlive = entry.image.strong_count() > 0;
			if !isAlive {
				unsafe {
					gl.delete_texture(entry.texture);
				}
			}
			isAlive
		});
	}

	//==============================================================================
	// 올라가 있는 텍스처 수.
	//==============================================================================
	pub fn getTextureCount(&self) -> usize {
		self.entryTable.len()
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
unsafe fn applyFilter(gl: &glow::Context, isSmoothing: bool, isMipmap: bool) {
	let magnifyFilter = if isSmoothing {
		glow::LINEAR
	}
	else {
		glow::NEAREST
	};
	let minifyFilter = if isMipmap {
		if isSmoothing {
			glow::LINEAR_MIPMAP_LINEAR
		}
		else {
			glow::NEAREST_MIPMAP_NEAREST
		}
	}
	else {
		magnifyFilter
	};
	gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, minifyFilter as i32);
	gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, magnifyFilter as i32);
}
