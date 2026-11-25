//==============================================================================
// 비주얼 애셋. (원본 experimental/visual/visualasset.js 의 VisualAsset)
// - 원본 load 가 하는 일을 다 옮겼다. 원본 load 는 이미 읽혔으면 바로 끝나고, 아니면 경로를 적고(기본 Asset.load) 곧바로 풀리는 Promise 를 기다린다.
//   그림 읽기 줄은 원본에서 주석으로 막혀 있어 아무것도 받지 않고, 읽힌 것으로 바꾸지도 않으며(isLoaded 는 늘 false), Visual 도 만들지 않는다.
//   러스트 판도 같다: load 는 경로를 적고 바로 끝난 읽기 손잡이를 돌려주어 await 가 바로 끝난다. (wait_for_load 도) is_loaded 는 늘 false 이고,
//   애셋 공통 상태에는 손잡이를 두지 않아 track_asset 에 맡겨도 씬을 붙잡지 않는다.
// - set_visual / get_visual 은 원본에도 있다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetLoadFuture, AssetLoadHandle, AssetType};
use crate::misc::visual::Visual;

//==============================================================================
// 비주얼 애셋.
//==============================================================================
pub struct VisualAsset {
	asset_base: AssetBase,
	asset_path: String,
	visual: Option<Visual>,
	// load 가 돌려준 바로 끝난 손잡이. (wait_for_load 가 기다린다, 원본처럼 읽힌 것으로는 치지 않아 공통 상태에는 두지 않는다)
	finished_load_handle: Option<AssetLoadHandle>,
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
			finished_load_handle: None,
		}
	}

	//==============================================================================
	// 읽기. (이미 읽혔으면 아무것도 하지 않고, 아니면 경로만 적는다. 원본과 같다. await 하면 바로 끝나는 손잡이를 돌려준다)
	//==============================================================================
	pub fn load(&mut self, asset_path: &str) -> AssetLoadHandle {
		if !self.is_loaded() {
			self.asset_path = asset_path.to_string();
		}
		let finished_load_handle = AssetLoadHandle::create_loaded(&self.asset_path, Vec::new());
		self.finished_load_handle = Some(finished_load_handle.clone());
		finished_load_handle
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

	//==============================================================================
	// 읽기를 마치기를 기다리는 퓨처. (load 뒤면 바로 끝난다, load 전이면 실패)
	//==============================================================================
	fn wait_for_load(&self) -> AssetLoadFuture {
		AssetLoadFuture::new(self.finished_load_handle.clone())
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
