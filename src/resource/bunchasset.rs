//==============================================================================
//! 묶음 애셋. (원본 resource/bunchasset.js 의 BunchAsset)
//! - 원본은 BlobAsset 을 상속하고 애셋 타입만 bunch 로 바꾼 뼈대다. 읽기는 BlobAsset 과 같다. (묶음을 푸는 일은 아직 없다)
//! - 러스트 판은 상속 대신 같은 함수를 가진 구조체이고, 애셋 타입이 AssetType::Bunch 다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use std::rc::Rc;

//==============================================================================
/// 묶음 애셋.
//==============================================================================
pub struct BunchAsset {
	asset_base: AssetBase,
}

impl BunchAsset {
	//==============================================================================
	/// 생성. (애셋 타입 bunch)
	//==============================================================================
	pub fn new() -> BunchAsset {
		BunchAsset {
			asset_base: AssetBase::new(AssetType::Bunch),
		}
	}

	//==============================================================================
	/// 읽기 요청. (원본 load = BlobAsset.load, await 할 수 있는 읽기 손잡이를 돌려준다)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		self.asset_base.request_load(engine, asset_path)
	}

	//==============================================================================
	/// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	/// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
	}

	//==============================================================================
	/// 바이트. (원본 get_blob. 다 읽히기 전이면 None)
	//==============================================================================
	pub fn get_blob(&self) -> Option<Rc<Vec<u8>>> {
		self.asset_base.get_bytes()
	}
}

impl Default for BunchAsset {
	fn default() -> BunchAsset {
		BunchAsset::new()
	}
}

impl Asset for BunchAsset {
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}
