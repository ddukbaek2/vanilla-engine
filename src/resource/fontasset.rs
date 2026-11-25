//==============================================================================
// 글꼴 애셋. (원본 resource/fontasset.js)
// - 원본은 FontFace 를 만들어 document.fonts 에 더한다. 그 뒤로 폰트 문자열("16px 이름")에서 그 이름을 쓴다.
// - 러스트 판은 바이트를 받은 뒤 엔진이 다음 프레임 시작에 Graphic 의 글자 캐시에 이름으로 등록한다.
//   WOFF, WOFF2 도 된다. (글자 캐시가 TTF 로 푼다)
// - 원본 descriptors 가운데 굵기만 is_bold 로 받는다.
// - load_font_from_bytes: 이미 가진 글꼴 바이트로 읽는다. (원본 loadFontFromBytes) unload 는 등록한 이름을 뺀다. (원본 document.fonts.delete)
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
	// 이미 가진 글꼴 바이트로 읽기. (ttf, otf, woff, woff2, 원본 loadFontFromBytes, 엔진이 다음 프레임 시작에 family 이름으로 등록한다)
	//==============================================================================
	pub fn load_font_from_bytes(&mut self, engine: &mut Engine, family: &str, bytes: Vec<u8>) {
		self.load_font_from_bytes_with_bold(engine, family, bytes, false);
	}

	//==============================================================================
	// 이미 가진 굵은 글꼴 바이트로 읽기. (원본 descriptors 의 weight 자리)
	//==============================================================================
	pub fn load_font_from_bytes_with_bold(&mut self, engine: &mut Engine, family: &str, bytes: Vec<u8>, is_bold: bool) {
		if self.asset_base.get_load_handle().is_some() {
			return;
		}
		self.family = family.to_string();
		self.is_bold = is_bold;
		self.asset_base.load_from_bytes(family, bytes);
		if let Some(load_handle) = self.asset_base.get_load_handle() {
			let load_handle = load_handle.clone();
			engine.register_font_when_loaded(family, is_bold, load_handle);
		}
	}

	//==============================================================================
	// 내리기. (등록한 이름을 빼서 그 뒤로는 기본 글꼴로 그린다, 다시 읽을 수 있다)
	//==============================================================================
	pub fn unload(&mut self, engine: &mut Engine) {
		if self.asset_base.get_load_handle().is_none() {
			return;
		}
		engine.unregister_font(&self.family, self.is_bold);
		self.asset_base.unload();
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;

	//==============================================================================
	// 바이트로 읽으면 바로 읽힌 것이고, 내리면 읽히지 않은 것이 된다. 두 번째 읽기는 무시한다.
	//==============================================================================
	#[test]
	fn load_from_bytes_and_unload() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut font_asset = FontAsset::new();
		font_asset.load_font_from_bytes_with_bold(&mut engine, "Pixel", vec![0u8, 1, 0, 0], true);
		assert!(font_asset.is_loaded());
		assert_eq!(font_asset.get_family(), "Pixel");
		assert!(font_asset.is_bold());
		font_asset.load_font_from_bytes(&mut engine, "Other", vec![0u8]);
		assert_eq!(font_asset.get_family(), "Pixel");
		font_asset.unload(&mut engine);
		assert!(!font_asset.is_loaded());
	}
}
