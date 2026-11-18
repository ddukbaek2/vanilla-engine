//==============================================================================
// 비주얼 애셋. (원본 experimental/visual/visualasset.js 의 VisualAsset)
// - 원본은 뼈대다. load 는 경로만 적고(기본 Asset.load) 아무것도 읽지 않으며, 읽힌 것으로 바꾸지도 않는다. Visual 도 만들지 않는다.
//   러스트 판도 그대로다: load 는 경로만 적고 isLoaded 는 늘 false 다. 그래서 trackAsset 에 맡겨도 씬을 붙잡지 않는다. (읽기 손잡이가 없다)
// - 원본 private 필드 visual 은 읽는 곳이 없어서, 러스트 판은 setVisual / getVisual 을 더했다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::misc::visual::Visual;

//==============================================================================
// 비주얼 애셋.
//==============================================================================
pub struct VisualAsset {
	assetBase: AssetBase,
	assetPath: String,
	visual: Option<Visual>,
}

impl VisualAsset {
	//==============================================================================
	// 생성. (애셋 타입 visual)
	//==============================================================================
	pub fn new() -> VisualAsset {
		VisualAsset {
			assetBase: AssetBase::new(AssetType::visual),
			assetPath: String::new(),
			visual: None,
		}
	}

	//==============================================================================
	// 읽기. (이미 읽혔으면 아무것도 하지 않고, 아니면 경로만 적는다. 원본과 같다)
	//==============================================================================
	pub fn load(&mut self, assetPath: &str) {
		if self.isLoaded() {
			return;
		}
		self.assetPath = assetPath.to_string();
	}

	//==============================================================================
	// 비주얼 설정.
	//==============================================================================
	pub fn setVisual(&mut self, visual: Option<Visual>) {
		self.visual = visual;
	}

	//==============================================================================
	// 비주얼 반환.
	//==============================================================================
	pub fn getVisual(&self) -> Option<&Visual> {
		self.visual.as_ref()
	}
}

impl Default for VisualAsset {
	fn default() -> VisualAsset {
		VisualAsset::new()
	}
}

impl Asset for VisualAsset {
	fn getAssetBase(&self) -> &AssetBase {
		&self.assetBase
	}

	fn getAssetBaseMut(&mut self) -> &mut AssetBase {
		&mut self.assetBase
	}

	//==============================================================================
	// 적어 둔 경로.
	//==============================================================================
	fn getAssetPath(&self) -> &str {
		&self.assetPath
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::scene::SceneBase;
	use crate::resource::bunchasset::BunchAsset;

	//==============================================================================
	// 비주얼 애셋은 경로만 적고 읽히지 않으며 씬을 붙잡지 않는다. 묶음 애셋은 바이트를 들고 타입이 bunch 다.
	//==============================================================================
	#[test]
	fn visualAssetRecordsPathAndBunchAssetHoldsBytes() {
		let mut visualAsset = VisualAsset::new();
		visualAsset.load("hero.visual");
		assert_eq!(visualAsset.getAssetPath(), "hero.visual");
		assert!(!visualAsset.isLoaded());
		assert_eq!(visualAsset.getAssetBase().getAssetType(), AssetType::visual);
		let mut sceneBase = SceneBase::new();
		sceneBase.trackAsset(&visualAsset);
		assert!(sceneBase.areTrackedAssetsFinished());
		visualAsset.setVisual(Some(Visual::new()));
		assert!(visualAsset.getVisual().is_some());

		let mut bunchAsset = BunchAsset::new();
		bunchAsset.loadFromBytes("pack.bunch", vec![1, 2, 3]);
		assert!(bunchAsset.isLoaded());
		assert_eq!(bunchAsset.getBlob().map(|bytes| bytes.to_vec()), Some(vec![1, 2, 3]));
		assert_eq!(bunchAsset.getAssetBase().getAssetType(), AssetType::bunch);
	}
}
