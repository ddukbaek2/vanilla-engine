//==============================================================================
// 이미지. (원본 HTMLImageElement / ImageAsset 자리)
// - 픽셀은 프리멀티플라이드 알파 RGBA 로 들고 있다. (원본은 텍스처에 올릴 때 UNPACK_PREMULTIPLY_ALPHA 로 같은 일을 한다)
// - Rc 로 나눠 쓰고, GPU 텍스처는 Graphic 의 ImageTextureCache 가 처음 그릴 때 만든다.
// - GPU 에 올린 뒤에는 CPU 픽셀을 놓아 메모리를 돌려준다. (원본은 브라우저가 HTMLImageElement 를 들고 있다)
//   픽셀을 다시 읽어야 하는 그림은 set_pixels_retained(true) 로 남긴다. (ShadowImage 는 그림을 받을 때 스스로 남기고, 이미 놓였으면 텍스처에서 되읽는다)
//   이 엔진은 GL 컨텍스트를 잃었다 되살리는 길이 없어서 놓은 픽셀로 텍스처를 다시 올릴 일은 없다.
//==============================================================================
use crate::base::identifier;
use std::cell::{Cell, Ref, RefCell};
use std::path::Path;
use std::rc::Rc;

pub struct Image {
	instance_id: u64,
	width: u32,
	height: u32,
	// 프리멀티플라이드 RGBA. (GPU 에 올린 뒤 놓았으면 비어 있다)
	pixels: RefCell<Vec<u8>>,
	// GPU 에 올린 뒤에도 CPU 픽셀을 남길지.
	is_pixels_retained: Cell<bool>,
}

impl Image {
	//==============================================================================
	// 파일에서 읽기. (PNG)
	//==============================================================================
	pub fn load(path: &Path) -> Result<Rc<Image>, String> {
		let bytes = std::fs::read(path).map_err(|error| format!("이미지 읽기 실패 ({}): {}", path.display(), error))?;
		Image::load_from_memory(&bytes)
	}

	//==============================================================================
	// 메모리에서 읽기. (PNG)
	//==============================================================================
	pub fn load_from_memory(bytes: &[u8]) -> Result<Rc<Image>, String> {
		let dynamic_image = image::load_from_memory(bytes).map_err(|error| format!("이미지 해석 실패: {}", error))?;
		let rgba_image = dynamic_image.to_rgba8();
		let width = rgba_image.width();
		let height = rgba_image.height();
		let straight_pixels = rgba_image.into_raw();
		Ok(Image::create_from_pixels(width, height, straight_pixels))
	}

	//==============================================================================
	// 스트레이트 알파 RGBA 픽셀로 생성. (코드로 만든 그림)
	//==============================================================================
	pub fn create_from_pixels(width: u32, height: u32, straight_pixels: Vec<u8>) -> Rc<Image> {
		let mut pixels = straight_pixels;
		for pixel in pixels.as_chunks_mut::<4>().0 {
			let alpha = pixel[3] as u32;
			pixel[0] = ((pixel[0] as u32 * alpha + 127) / 255) as u8;
			pixel[1] = ((pixel[1] as u32 * alpha + 127) / 255) as u8;
			pixel[2] = ((pixel[2] as u32 * alpha + 127) / 255) as u8;
		}
		Rc::new(Image {
			instance_id: identifier::create_instance_id(),
			width,
			height,
			pixels: RefCell::new(pixels),
			is_pixels_retained: Cell::new(false),
		})
	}

	//==============================================================================
	// 인스턴스 식별자.
	//==============================================================================
	pub fn get_instance_id(&self) -> u64 {
		self.instance_id
	}

	//==============================================================================
	// 너비. (픽셀)
	//==============================================================================
	pub fn get_width(&self) -> u32 {
		self.width
	}

	//==============================================================================
	// 높이. (픽셀)
	//==============================================================================
	pub fn get_height(&self) -> u32 {
		self.height
	}

	//==============================================================================
	// 프리멀티플라이드 RGBA 픽셀. (GPU 에 올린 뒤 놓았으면 빈 배열, has_pixels 로 가린다)
	//==============================================================================
	pub fn get_pixels(&self) -> Ref<'_, [u8]> {
		Ref::map(self.pixels.borrow(), |pixels| pixels.as_slice())
	}

	//==============================================================================
	// CPU 픽셀을 들고 있는지. (GPU 에 올린 뒤 놓았으면 false)
	//==============================================================================
	pub fn has_pixels(&self) -> bool {
		!self.pixels.borrow().is_empty()
	}

	//==============================================================================
	// GPU 에 올린 뒤에도 CPU 픽셀을 남길지 설정. (기본 false. 픽셀을 다시 읽는 그림이 켠다)
	//==============================================================================
	pub fn set_pixels_retained(&self, is_pixels_retained: bool) {
		self.is_pixels_retained.set(is_pixels_retained);
	}

	//==============================================================================
	// GPU 에 올린 뒤에도 CPU 픽셀을 남기는지.
	//==============================================================================
	pub fn is_pixels_retained(&self) -> bool {
		self.is_pixels_retained.get()
	}

	//==============================================================================
	// CPU 픽셀 놓기. (ImageTextureCache 가 GPU 에 올린 뒤 부른다. 남기게 했으면 그대로 둔다)
	//==============================================================================
	pub(crate) fn release_pixels(&self) {
		if self.is_pixels_retained.get() {
			return;
		}
		let released_pixels = std::mem::take(&mut *self.pixels.borrow_mut());
		drop(released_pixels);
	}

	//==============================================================================
	// 놓았던 CPU 픽셀 되돌리기. (Graphic 이 텍스처에서 되읽은 것)
	//==============================================================================
	pub(crate) fn restore_pixels(&self, pixels: Vec<u8>) {
		*self.pixels.borrow_mut() = pixels;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 놓으면 비고, 남기게 하면 그대로이고, 되돌리면 다시 든다.
	//==============================================================================
	#[test]
	fn release_and_retain_pixels() {
		let image = Image::create_from_pixels(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]);
		assert!(image.has_pixels());
		assert_eq!(image.get_pixels()[7], 128);
		image.set_pixels_retained(true);
		image.release_pixels();
		assert!(image.has_pixels());
		image.set_pixels_retained(false);
		image.release_pixels();
		assert!(!image.has_pixels());
		assert!(image.get_pixels().is_empty());
		assert_eq!(image.get_width(), 2);
		image.restore_pixels(vec![1, 2, 3, 4, 5, 6, 7, 8]);
		assert!(image.has_pixels());
		assert_eq!(image.get_pixels()[4], 5);
	}
}
