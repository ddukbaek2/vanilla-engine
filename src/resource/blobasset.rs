//==============================================================================
// 바이너리 애셋. (원본 resource/blobasset.js)
// - 원본은 fetch 로 받은 Blob 을 든다. 러스트 판은 바이트를 그대로 든다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;
use std::rc::Rc;

//==============================================================================
// 바이너리 애셋.
//==============================================================================
pub struct BlobAsset {
	assetBase: AssetBase,
}

impl BlobAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> BlobAsset {
		BlobAsset {
			assetBase: AssetBase::new(AssetType::blob),
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
		self.assetBase.loadFromBytes(assetPath, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.assetBase.unload();
	}

	//==============================================================================
	// 바이트. (원본 getBlob. 다 읽히기 전이면 None)
	//==============================================================================
	pub fn getBlob(&self) -> Option<Rc<Vec<u8>>> {
		self.assetBase.getBytes()
	}
}

impl Default for BlobAsset {
	fn default() -> BlobAsset {
		BlobAsset::new()
	}
}

impl Asset for BlobAsset {
	fn getAssetBase(&self) -> &AssetBase {
		&self.assetBase
	}

	fn getAssetBaseMut(&mut self) -> &mut AssetBase {
		&mut self.assetBase
	}
}
