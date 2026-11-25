//==============================================================================
// 글꼴 애셋. (원본 resource/fontasset.js)
// - 원본은 FontFace 를 만들어 document.fonts 에 더한다. 그 뒤로 폰트 문자열("16px 이름")에서 그 이름을 쓴다.
// - 러스트 판은 바이트를 받은 뒤 엔진이 다음 프레임 시작에 Graphic 의 글자 캐시에 이름으로 등록한다.
//   WOFF, WOFF2 도 된다. (글자 캐시가 TTF 로 푼다)
// - 원본 descriptors(굵기, 기울기, 글자 범위)는 FontFaceDescriptors 로 받는다. (load_font_with_descriptors, 굵기만이면 load_font_with_bold)
//   같은 이름에 굵기나 기울기가 다른 파일을 여러 개 올리면 글자 캐시가 CSS 처럼 골라 그린다. 진짜 기울인 얼굴이 있으면 가짜 기울임 대신 그것을 쓴다.
// - unload 는 원본처럼 그 얼굴을 글꼴 목록에서 뺀다. (document.fonts.delete)
// - load_font_from_bytes: 이미 가진 글꼴 바이트로 읽는다. (원본 loadFontFromBytes, 서술자의 source_key 가 비면 이름을 쓴다)
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;
use crate::core::textshaping::FontFaceDescriptors;

//==============================================================================
// 글꼴 애셋.
//==============================================================================
pub struct FontAsset {
	asset_base: AssetBase,
	family: String,
	descriptors: FontFaceDescriptors,
}

impl FontAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> FontAsset {
		FontAsset {
			asset_base: AssetBase::new(AssetType::Font),
			family: String::new(),
			descriptors: FontFaceDescriptors::new(),
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
		self.load_font_with_descriptors(engine, family, asset_path, FontFaceDescriptors::create_with_bold(is_bold));
	}

	//==============================================================================
	// 서술자를 주어 글꼴 읽기 요청. (원본 loadFont(family, assetPath, descriptors), 서술자의 source_key 가 비면 경로를 쓴다)
	//==============================================================================
	pub fn load_font_with_descriptors(&mut self, engine: &mut Engine, family: &str, asset_path: &str, mut descriptors: FontFaceDescriptors) {
		if self.asset_base.get_load_handle().is_some() {
			return;
		}
		if descriptors.source_key.is_empty() {
			descriptors.source_key = asset_path.to_string();
		}
		self.family = family.to_string();
		self.descriptors = descriptors.clone();
		self.asset_base.request_load(engine, asset_path);
		if let Some(load_handle) = self.asset_base.get_load_handle() {
			let load_handle = load_handle.clone();
			engine.register_font_face_when_loaded(family, descriptors, load_handle);
		}
	}

	//==============================================================================
	// 이미 가진 글꼴 바이트로 읽기. (ttf, otf, woff, woff2, 원본 loadFontFromBytes, 엔진이 다음 프레임 시작에 family 이름으로 올린다)
	//==============================================================================
	pub fn load_font_from_bytes(&mut self, engine: &mut Engine, family: &str, bytes: Vec<u8>) {
		self.load_font_from_bytes_with_descriptors(engine, family, bytes, FontFaceDescriptors::new());
	}

	//==============================================================================
	// 서술자를 주어 이미 가진 글꼴 바이트로 읽기. (원본 loadFontFromBytes(family, bytes, descriptors))
	//==============================================================================
	pub fn load_font_from_bytes_with_descriptors(&mut self, engine: &mut Engine, family: &str, bytes: Vec<u8>, mut descriptors: FontFaceDescriptors) {
		if self.asset_base.get_load_handle().is_some() {
			return;
		}
		if descriptors.source_key.is_empty() {
			descriptors.source_key = family.to_string();
		}
		self.family = family.to_string();
		self.descriptors = descriptors.clone();
		self.asset_base.load_from_bytes(&descriptors.source_key, bytes);
		if let Some(load_handle) = self.asset_base.get_load_handle() {
			let load_handle = load_handle.clone();
			engine.register_font_face_when_loaded(family, descriptors, load_handle);
		}
	}

	//==============================================================================
	// 애셋 내리기. (원본 unload: 글꼴 목록에서 이 얼굴을 뺀다)
	//==============================================================================
	pub fn unload(&mut self, engine: &mut Engine) {
		if self.asset_base.get_load_handle().is_none() {
			return;
		}
		engine.unregister_font_face(&self.family, &self.descriptors.source_key);
		self.asset_base.unload();
	}

	//==============================================================================
	// 얼굴 서술자.
	//==============================================================================
	pub fn get_descriptors(&self) -> &FontFaceDescriptors {
		&self.descriptors
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
		self.descriptors.minimum_weight >= crate::core::textshaping::BOLD_WEIGHT_THRESHOLD
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
		font_asset.load_font_from_bytes_with_descriptors(&mut engine, "Pixel", vec![0u8, 1, 0, 0], FontFaceDescriptors::create_with_bold(true));
		assert!(font_asset.is_loaded());
		assert_eq!(font_asset.get_family(), "Pixel");
		assert!(font_asset.is_bold());
		assert_eq!(font_asset.get_descriptors().source_key, "Pixel");
		font_asset.load_font_from_bytes(&mut engine, "Other", vec![0u8]);
		assert_eq!(font_asset.get_family(), "Pixel");
		assert_eq!(engine.get_pending_font_face_count(), 1);
		font_asset.unload(&mut engine);
		assert!(!font_asset.is_loaded());
		assert_eq!(engine.get_pending_font_face_count(), 0);
	}
}
