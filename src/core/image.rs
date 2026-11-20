//==============================================================================
// 이미지. (원본 HTMLImageElement / ImageAsset 자리)
// - 픽셀은 프리멀티플라이드 알파 RGBA 로 들고 있다. (원본은 텍스처에 올릴 때 UNPACK_PREMULTIPLY_ALPHA 로 같은 일을 한다)
// - Rc 로 나눠 쓰고, GPU 텍스처는 Graphic 의 ImageTextureCache 가 처음 그릴 때 만든다.
//==============================================================================
use crate::base::identifier;
use std::path::Path;
use std::rc::Rc;

pub struct Image {
	instance_id: u64,
	width: u32,
	height: u32,
	pixels: Vec<u8>,
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
			pixels,
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
	// 프리멀티플라이드 RGBA 픽셀.
	//==============================================================================
	pub fn get_pixels(&self) -> &[u8] {
		&self.pixels
	}
}
