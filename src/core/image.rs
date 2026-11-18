//==============================================================================
// 이미지. (원본 HTMLImageElement / ImageAsset 자리)
// - 픽셀은 프리멀티플라이드 알파 RGBA 로 들고 있다. (원본은 텍스처에 올릴 때 UNPACK_PREMULTIPLY_ALPHA 로 같은 일을 한다)
// - Rc 로 나눠 쓰고, GPU 텍스처는 Graphic 의 ImageTextureCache 가 처음 그릴 때 만든다.
//==============================================================================
use crate::base::identifier;
use std::path::Path;
use std::rc::Rc;

pub struct Image {
	instanceId: u64,
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
		Image::loadFromMemory(&bytes)
	}

	//==============================================================================
	// 메모리에서 읽기. (PNG)
	//==============================================================================
	pub fn loadFromMemory(bytes: &[u8]) -> Result<Rc<Image>, String> {
		let dynamicImage = image::load_from_memory(bytes).map_err(|error| format!("이미지 해석 실패: {}", error))?;
		let rgbaImage = dynamicImage.to_rgba8();
		let width = rgbaImage.width();
		let height = rgbaImage.height();
		let straightPixels = rgbaImage.into_raw();
		Ok(Image::createFromPixels(width, height, straightPixels))
	}

	//==============================================================================
	// 스트레이트 알파 RGBA 픽셀로 생성. (코드로 만든 그림)
	//==============================================================================
	pub fn createFromPixels(width: u32, height: u32, straightPixels: Vec<u8>) -> Rc<Image> {
		let mut pixels = straightPixels;
		for pixel in pixels.as_chunks_mut::<4>().0 {
			let alpha = pixel[3] as u32;
			pixel[0] = ((pixel[0] as u32 * alpha + 127) / 255) as u8;
			pixel[1] = ((pixel[1] as u32 * alpha + 127) / 255) as u8;
			pixel[2] = ((pixel[2] as u32 * alpha + 127) / 255) as u8;
		}
		Rc::new(Image {
			instanceId: identifier::createInstanceId(),
			width,
			height,
			pixels,
		})
	}

	//==============================================================================
	// 인스턴스 식별자.
	//==============================================================================
	pub fn getInstanceId(&self) -> u64 {
		self.instanceId
	}

	//==============================================================================
	// 너비. (픽셀)
	//==============================================================================
	pub fn getWidth(&self) -> u32 {
		self.width
	}

	//==============================================================================
	// 높이. (픽셀)
	//==============================================================================
	pub fn getHeight(&self) -> u32 {
		self.height
	}

	//==============================================================================
	// 프리멀티플라이드 RGBA 픽셀.
	//==============================================================================
	pub fn getPixels(&self) -> &[u8] {
		&self.pixels
	}
}
