//==============================================================================
// 묶음 애셋. (원본 experimental/bunchasset.js 의 BunchAsset)
// - 원본은 BlobAsset 을 상속하고 애셋 타입만 bunch 로 바꾼 뼈대다. 읽기는 BlobAsset 과 같다. (묶음을 푸는 일은 아직 없다)
// - 러스트 판은 상속 대신 같은 함수를 가진 구조체이고, 애셋 타입이 AssetType::bunch 다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;
use std::rc::Rc;

//==============================================================================
// 묶음 애셋.
//==============================================================================
pub struct BunchAsset {
	assetBase: AssetBase,
}

impl BunchAsset {
	//==============================================================================
	// 생성. (애셋 타입 bunch)
	//==============================================================================
	pub fn new() -> BunchAsset {
		BunchAsset {
			assetBase: AssetBase::new(AssetType::bunch),
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load = BlobAsset.load)
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

impl Default for BunchAsset {
	fn default() -> BunchAsset {
		BunchAsset::new()
	}
}

impl Asset for BunchAsset {
	fn getAssetBase(&self) -> &AssetBase {
		&self.assetBase
	}

	fn getAssetBaseMut(&mut self) -> &mut AssetBase {
		&mut self.assetBase
	}
}
