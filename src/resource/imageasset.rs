//==============================================================================
// 이미지 애셋. (원본 resource/imageasset.js)
// - 원본은 HTMLImageElement 에 src 를 넣고 onload 를 기다린다.
//   러스트 판은 엔진에게 바이트를 부탁하고, 다 오면 처음 get_image 할 때 풀어서 Image 로 만든다. (Image::load_from_memory 의 형식, 웹은 받을 때 브라우저가 푼 그림을 손잡이에서 쓴다)
//   AssetLoader 로 받은 것은 다 받았을 때 이미 풀어 둔 그림(손잡이의 get_decoded_image)을 그대로 쓴다. (풀지 못했으면 손잡이가 실패다)
// - load, load_from_bytes 는 원본처럼 await 할 수 있는 읽기 손잡이를 돌려준다. await 는 원본 img.onload 처럼 풀기까지 끝나야 끝나고,
//   풀지 못하면 실패(원본 onerror 의 reject)로 끝난다. 푼 그림은 손잡이에 담겨 get_image 가 다시 풀지 않는다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use crate::core::image::Image;
use std::rc::Rc;

//==============================================================================
// 이미지 애셋.
//==============================================================================
pub struct ImageAsset {
	asset_base: AssetBase,
	image: Option<Rc<Image>>,
	is_decode_failed: bool,
}

impl ImageAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ImageAsset {
		ImageAsset {
			asset_base: AssetBase::new(AssetType::Image),
			image: None,
			is_decode_failed: false,
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load, await 할 수 있는 읽기 손잡이를 돌려준다. 풀기까지 끝나야 끝난다)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		let load_handle = self.asset_base.request_load(engine, asset_path);
		load_handle.request_image_decode();
		load_handle
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기. (원본 loadFromBytes, await 할 수 있는 읽기 손잡이를 돌려준다. 풀지 못하면 실패로 끝난다)
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) -> AssetLoadHandle {
		self.image = None;
		self.is_decode_failed = false;
		let load_handle = self.asset_base.load_from_bytes(asset_path, bytes);
		load_handle.request_image_decode();
		load_handle
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.image = None;
		self.is_decode_failed = false;
	}

	//==============================================================================
	// 이미지. (다 읽히기 전이거나 풀기에 실패하면 None)
	//==============================================================================
	pub fn get_image(&mut self) -> Option<Rc<Image>> {
		if let Some(image) = &self.image {
			return Some(image.clone());
		}
		if self.is_decode_failed {
			return None;
		}
		if let Some(decoded_image) = self.asset_base.get_load_handle().and_then(|load_handle| load_handle.get_decoded_image()) {
			self.image = Some(decoded_image.clone());
			return Some(decoded_image);
		}
		let bytes = self.asset_base.get_bytes()?;
		match Image::load_from_memory(&bytes) {
			Ok(image) => {
				self.image = Some(image.clone());
				Some(image)
			}
			Err(error) => {
				eprintln!("ImageAsset: {} ({})", error, self.asset_base.get_asset_path());
				self.is_decode_failed = true;
				None
			}
		}
	}

	//==============================================================================
	// 이미지 크기. (픽셀, 아직 없으면 0)
	//==============================================================================
	pub fn get_image_size(&mut self) -> Vector2 {
		match self.get_image() {
			Some(image) => {
				Vector2::create(image.get_width() as f32, image.get_height() as f32)
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
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}
