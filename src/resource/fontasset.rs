//==============================================================================
// 글꼴 애셋. (원본 resource/fontasset.js)
// - 원본은 FontFace 를 만들어 document.fonts 에 더한다. 그 뒤로 폰트 문자열("16px 이름")에서 그 이름을 쓴다.
// - 러스트 판은 바이트를 받은 뒤 엔진이 다음 프레임 시작에 Graphic 의 글자 캐시에 이름으로 등록한다.
//   WOFF, WOFF2 도 된다. (글자 캐시가 TTF 로 푼다)
// - 원본 descriptors 가운데 굵기만 isBold 로 받는다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;

//==============================================================================
// 글꼴 애셋.
//==============================================================================
pub struct FontAsset {
	assetBase: AssetBase,
	family: String,
	isBold: bool,
}

impl FontAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> FontAsset {
		FontAsset {
			assetBase: AssetBase::new(AssetType::font),
			family: String::new(),
			isBold: false,
		}
	}

	//==============================================================================
	// 글꼴 읽기 요청. (원본 loadFont(family, assetPath). 다 오면 엔진이 family 이름으로 등록한다)
	//==============================================================================
	pub fn loadFont(&mut self, engine: &mut Engine, family: &str, assetPath: &str) {
		self.loadFontWithBold(engine, family, assetPath, false);
	}

	//==============================================================================
	// 굵은 글꼴 읽기 요청. (원본 descriptors 의 weight 자리)
	//==============================================================================
	pub fn loadFontWithBold(&mut self, engine: &mut Engine, family: &str, assetPath: &str, isBold: bool) {
		if self.assetBase.getLoadHandle().is_some() {
			return;
		}
		self.family = family.to_string();
		self.isBold = isBold;
		self.assetBase.requestLoad(engine, assetPath);
		if let Some(loadHandle) = self.assetBase.getLoadHandle() {
			let loadHandle = loadHandle.clone();
			engine.registerFontWhenLoaded(family, isBold, loadHandle);
		}
	}

	//==============================================================================
	// 글꼴 이름.
	//==============================================================================
	pub fn getFamily(&self) -> &str {
		&self.family
	}

	//==============================================================================
	// 굵은 글꼴인지.
	//==============================================================================
	pub fn isBold(&self) -> bool {
		self.isBold
	}
}

impl Default for FontAsset {
	fn default() -> FontAsset {
		FontAsset::new()
	}
}

impl Asset for FontAsset {
	fn getAssetBase(&self) -> &AssetBase {
		&self.assetBase
	}

	fn getAssetBaseMut(&mut self) -> &mut AssetBase {
		&mut self.assetBase
	}
}
