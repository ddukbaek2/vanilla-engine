//==============================================================================
// 비주얼 애셋. (원본 experimental/visual/visualasset.js 의 VisualAsset)
// - 원본은 뼈대다. load 는 경로만 적고(기본 Asset.load) 아무것도 읽지 않으며, 읽힌 것으로 바꾸지도 않는다. Visual 도 만들지 않는다.
//   러스트 판도 그대로다: load 는 경로만 적고 is_loaded 는 늘 false 다. 그래서 track_asset 에 맡겨도 씬을 붙잡지 않는다. (읽기 손잡이가 없다)
// - 원본 private 필드 visual 은 읽는 곳이 없어서, 러스트 판은 set_visual / get_visual 을 더했다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::misc::visual::Visual;

//==============================================================================
// 비주얼 애셋.
//==============================================================================
pub struct VisualAsset {
	asset_base: AssetBase,
	asset_path: String,
	visual: Option<Visual>,
}

impl VisualAsset {
	//==============================================================================
	// 생성. (애셋 타입 visual)
	//==============================================================================
	pub fn new() -> VisualAsset {
		VisualAsset {
			asset_base: AssetBase::new(AssetType::Visual),
			asset_path: String::new(),
			visual: None,
		}
	}

	//==============================================================================
	// 읽기. (이미 읽혔으면 아무것도 하지 않고, 아니면 경로만 적는다. 원본과 같다)
	//==============================================================================
	pub fn load(&mut self, asset_path: &str) {
		if self.is_loaded() {
			return;
		}
		self.asset_path = asset_path.to_string();
	}

	//==============================================================================
	// 비주얼 설정.
	//==============================================================================
	pub fn set_visual(&mut self, visual: Option<Visual>) {
		self.visual = visual;
	}

	//==============================================================================
	// 비주얼 반환.
	//==============================================================================
	pub fn get_visual(&self) -> Option<&Visual> {
		self.visual.as_ref()
	}
}

impl Default for VisualAsset {
	fn default() -> VisualAsset {
		VisualAsset::new()
	}
}

impl Asset for VisualAsset {
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}

	//==============================================================================
	// 적어 둔 경로.
	//==============================================================================
	fn get_asset_path(&self) -> &str {
		&self.asset_path
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
	fn visual_asset_records_path_and_bunch_asset_holds_bytes() {
		let mut visual_asset = VisualAsset::new();
		visual_asset.load("hero.visual");
		assert_eq!(visual_asset.get_asset_path(), "hero.visual");
		assert!(!visual_asset.is_loaded());
		assert_eq!(visual_asset.get_asset_base().get_asset_type(), AssetType::Visual);
		let mut scene_base = SceneBase::new();
		scene_base.track_asset(&visual_asset);
		assert!(scene_base.are_tracked_assets_finished());
		visual_asset.set_visual(Some(Visual::new()));
		assert!(visual_asset.get_visual().is_some());

		let mut bunch_asset = BunchAsset::new();
		bunch_asset.load_from_bytes("pack.bunch", vec![1, 2, 3]);
		assert!(bunch_asset.is_loaded());
		assert_eq!(bunch_asset.get_blob().map(|bytes| bytes.to_vec()), Some(vec![1, 2, 3]));
		assert_eq!(bunch_asset.get_asset_base().get_asset_type(), AssetType::Bunch);
	}
}
