//==============================================================================
// 글꼴 애셋. (원본 resource/fontasset.js)
// - 원본은 FontFace 를 만들어 document.fonts 에 더한다. 그 뒤로 폰트 문자열("16px 이름")에서 그 이름을 쓴다.
// - 러스트 판은 바이트를 받은 뒤 엔진이 다음 프레임 시작에 Graphic 의 글자 캐시에 이름으로 등록한다.
//   WOFF, WOFF2 도 된다. (글자 캐시가 TTF 로 푼다)
// - 원본 descriptors 가운데 굵기만 is_bold 로 받는다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;

//==============================================================================
// 글꼴 애셋.
//==============================================================================
pub struct FontAsset {
	asset_base: AssetBase,
	family: String,
	is_bold: bool,
}

impl FontAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> FontAsset {
		FontAsset {
			asset_base: AssetBase::new(AssetType::Font),
			family: String::new(),
			is_bold: false,
		}
	}

	//==============================================================================
	// 글꼴 읽기 요청. (원본 load_font(family, asset_path). 다 오면 엔진이 family 이름으로 등록한다)
	//==============================================================================
	pub fn load_font(&mut self, engine: &mut Engine, family: &str, asset_path: &str) {
		self.load_font_with_bold(engine, family, asset_path, false);
	}

	//==============================================================================
	// 굵은 글꼴 읽기 요청. (원본 descriptors 의 weight 자리)
	//==============================================================================
	pub fn load_font_with_bold(&mut self, engine: &mut Engine, family: &str, asset_path: &str, is_bold: bool) {
		if self.asset_base.get_load_handle().is_some() {
			return;
		}
		self.family = family.to_string();
		self.is_bold = is_bold;
		self.asset_base.request_load(engine, asset_path);
		if let Some(load_handle) = self.asset_base.get_load_handle() {
			let load_handle = load_handle.clone();
			engine.register_font_when_loaded(family, is_bold, load_handle);
		}
	}

	//==============================================================================
	// 글꼴 이름.
	//==============================================================================
	pub fn get_family(&self) -> &str {
		&self.family
	}

	//==============================================================================
	// 굵은 글꼴인지.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.is_bold
	}
}

impl Default for FontAsset {
	fn default() -> FontAsset {
		FontAsset::new()
	}
}

impl Asset for FontAsset {
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}
