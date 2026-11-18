//==============================================================================
// 이미지 애셋. (원본 resource/imageasset.js)
// - 원본은 HTMLImageElement 에 src 를 넣고 onload 를 기다린다.
//   러스트 판은 엔진에게 바이트를 부탁하고, 다 오면 처음 getImage 할 때 풀어서 Image 로 만든다. (PNG, JPEG)
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;
use crate::core::image::Image;
use std::rc::Rc;

//==============================================================================
// 이미지 애셋.
//==============================================================================
pub struct ImageAsset {
	assetBase: AssetBase,
	image: Option<Rc<Image>>,
	isDecodeFailed: bool,
}

impl ImageAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ImageAsset {
		ImageAsset {
			assetBase: AssetBase::new(AssetType::image),
			image: None,
			isDecodeFailed: false,
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, assetPath: &str) {
		self.assetBase.requestLoad(engine, assetPath);
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn loadFromBytes(&mut self, assetPath: &str, bytes: Vec<u8>) {
		self.image = None;
		self.isDecodeFailed = false;
		self.assetBase.loadFromBytes(assetPath, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.assetBase.unload();
		self.image = None;
		self.isDecodeFailed = false;
	}

	//==============================================================================
	// 이미지. (다 읽히기 전이거나 풀기에 실패하면 None)
	//==============================================================================
	pub fn getImage(&mut self) -> Option<Rc<Image>> {
		if let Some(image) = &self.image {
			return Some(image.clone());
		}
		if self.isDecodeFailed {
			return None;
		}
		let bytes = self.assetBase.getBytes()?;
		match Image::loadFromMemory(&bytes) {
			Ok(image) => {
				self.image = Some(image.clone());
				Some(image)
			}
			Err(error) => {
				eprintln!("ImageAsset: {} ({})", error, self.assetBase.getAssetPath());
				self.isDecodeFailed = true;
				None
			}
		}
	}

	//==============================================================================
	// 이미지 크기. (픽셀, 아직 없으면 0)
	//==============================================================================
	pub fn getImageSize(&mut self) -> Vector2 {
		match self.getImage() {
			Some(image) => {
				Vector2::create(image.getWidth() as f32, image.getHeight() as f32)
			}
			None => {
				Vector2::zero()
			}
		}
	}
}

impl Default for ImageAsset {
	fn default() -> ImageAsset {
		ImageAsset::new()
	}
}

impl Asset for ImageAsset {
	fn getAssetBase(&self) -> &AssetBase {
		&self.assetBase
	}

	fn getAssetBaseMut(&mut self) -> &mut AssetBase {
		&mut self.assetBase
	}
}
