//==============================================================================
// 바이너리 애셋. (원본 resource/blobasset.js)
// - 원본은 fetch 로 받은 Blob 을 든다. 러스트 판은 바이트를 그대로 든다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use std::rc::Rc;

//==============================================================================
// 바이너리 애셋.
//==============================================================================
pub struct BlobAsset {
	asset_base: AssetBase,
}

impl BlobAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> BlobAsset {
		BlobAsset {
			asset_base: AssetBase::new(AssetType::Blob),
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load, await 할 수 있는 읽기 손잡이를 돌려준다)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		self.asset_base.request_load(engine, asset_path)
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
	}

	//==============================================================================
	// 바이트. (원본 get_blob. 다 읽히기 전이면 None)
	//==============================================================================
	pub fn get_blob(&self) -> Option<Rc<Vec<u8>>> {
		self.asset_base.get_bytes()
	}
}

impl Default for BlobAsset {
	fn default() -> BlobAsset {
		BlobAsset::new()
	}
}

impl Asset for BlobAsset {
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}
