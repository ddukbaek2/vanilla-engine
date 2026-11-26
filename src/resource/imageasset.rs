//==============================================================================
//! 이미지 애셋. (원본 resource/imageasset.js)
//! - 원본은 HTMLImageElement 에 src 를 넣고 onload 를 기다린다.
//!   러스트 판은 엔진에게 바이트를 부탁하고, 다 오면 처음 get_image 할 때 풀어서 Image 로 만든다. (Image::load_from_memory 의 형식, 웹은 받을 때 브라우저가 푼 그림을 손잡이에서 쓴다)
//!   AssetLoader 로 받은 것은 다 받았을 때 이미 풀어 둔 그림(손잡이의 get_decoded_image)을 그대로 쓴다. (풀지 못했으면 손잡이가 실패다)
//! - load, load_from_bytes 는 원본처럼 await 할 수 있는 읽기 손잡이를 돌려준다. await 는 원본 img.onload 처럼 풀기까지 끝나야 끝나고,
//!   풀지 못하면 실패(원본 onerror 의 reject)로 끝난다. 푼 그림은 손잡이에 담겨 get_image 가 다시 풀지 않는다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use crate::core::image::Image;
use std::rc::Rc;

//==============================================================================
/// 이미지 애셋.
/// - 바이트가 다 오면 처음 get_image 할 때 풀어 Image 로 든다. (PNG, JPEG 같은 Image::load_from_memory 의 형식)
///
/// # 예제
/// 저장소의 PNG 를 읽어 크기 보기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, ImageAsset, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut image_asset = ImageAsset::new();
/// let load_handle = image_asset.load(&engine, "examples/assets/atlas/icons.png");
/// assert!(load_handle.is_loaded());
/// let image = image_asset.get_image().expect("decoded image");
/// assert_eq!(image.get_width(), 128);
/// assert_eq!(image_asset.get_image_size(), Vector2::create(128.0, 128.0));
/// ```
//==============================================================================
pub struct ImageAsset {
	asset_base: AssetBase,
	image: Option<Rc<Image>>,
	is_decode_failed: bool,
}

impl ImageAsset {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 요청 전의 빈 이미지 애셋.
	//==============================================================================
	pub fn new() -> ImageAsset {
		ImageAsset {
			asset_base: AssetBase::new(AssetType::Image),
			image: None,
			is_decode_failed: false,
		}
	}

	//==============================================================================
	/// 읽기 요청. (원본 load, await 할 수 있는 읽기 손잡이를 돌려준다. 풀기까지 끝나야 끝난다)
	/// - 이미 요청했으면 다시 하지 않고 처음 손잡이를 돌려준다.
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `asset_path`: 애셋 경로. (데스크탑은 애셋 폴더 기준, 웹은 asset_base_url 기준, data: 주소도 된다)
	///
	/// # 돌려줌
	/// 읽기 손잡이. (데스크탑은 그 자리에서 바이트가 읽혀 있다. 못 읽으면 실패로 끝난다)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		let load_handle = self.asset_base.request_load(engine, asset_path);
		load_handle.request_image_decode();
		load_handle
	}

	//==============================================================================
	/// 이미 가진 바이트로 읽기. (원본 loadFromBytes, await 할 수 있는 읽기 손잡이를 돌려준다. 풀지 못하면 실패로 끝난다)
	/// - 앞서 푼 그림은 버리고 이 바이트로 다시 푼다.
	///
	/// # 인자
	/// - `asset_path`: 적어 둘 경로. (알림 글에 쓴다)
	/// - `bytes`: 그림 파일 바이트.
	///
	/// # 돌려줌
	/// 바이트가 다 읽힌 읽기 손잡이.
	///
	/// # 예제
	/// 파일 바이트로 읽기와 깨진 바이트
	/// ```rust
	/// use vanilla::ImageAsset;
	///
	/// let png_bytes = std::fs::read("examples/assets/atlas/icons.png").expect("png file");
	/// let mut image_asset = ImageAsset::new();
	/// image_asset.load_from_bytes("icons.png", png_bytes);
	/// assert_eq!(image_asset.get_image_size().x, 128.0);
	/// image_asset.load_from_bytes("broken.png", vec![0, 1, 2, 3]);
	/// assert!(image_asset.get_image().is_none());
	/// assert_eq!(image_asset.get_image_size().x, 0.0);
	/// ```
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) -> AssetLoadHandle {
		self.image = None;
		self.is_decode_failed = false;
		let load_handle = self.asset_base.load_from_bytes(asset_path, bytes);
		load_handle.request_image_decode();
		load_handle
	}

	//==============================================================================
	/// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.image = None;
		self.is_decode_failed = false;
	}

	//==============================================================================
	/// 이미지. (다 읽히기 전이거나 풀기에 실패하면 None)
	/// - 처음 부를 때 한 번 풀고 그 뒤로는 푼 것을 돌려준다. 풀지 못하면 표준 에러에 까닭을 찍고 다시 풀지 않는다.
	///
	/// # 돌려줌
	/// 푼 이미지. 요청 전이거나 읽는 중이거나 읽기나 풀기에 실패했으면 None.
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
	/// 이미지 크기. (픽셀, 아직 없으면 0)
	///
	/// # 돌려줌
	/// 이미지의 가로와 세로. (픽셀) 이미지가 없으면 (0, 0).
	//==============================================================================
	pub fn get_image_size(&mut self) -> Vector2 {
		match self.get_image() {
			Some(image) => {
				Vector2::create(image.get_width() as f64, image.get_height() as f64)
			}
			None => {
				Vector2::zero()
			}
		}
	}
}

impl Default for ImageAsset {
	//==============================================================================
	/// 요청 전의 빈 이미지 애셋. (new 와 같다)
	//==============================================================================
	fn default() -> ImageAsset {
		ImageAsset::new()
	}
}

impl Asset for ImageAsset {
	//==============================================================================
	/// 공통 상태.
	//==============================================================================
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	//==============================================================================
	/// 공통 상태. (변경용)
	//==============================================================================
	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}
