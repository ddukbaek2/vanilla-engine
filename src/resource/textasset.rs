//==============================================================================
// 글 애셋. (원본 resource/textasset.js, resource/jsonasset.js)
// - 원본 TextAsset 은 fetch 로 받은 글을 text 에, JsonAsset 은 JSON.parse 한 것을 data 에 둔다.
// - 러스트 판은 바이트가 다 오면 처음 꺼낼 때 UTF-8 글로 풀고, JsonAsset 은 serde_json::Value 로 푼다.
// - 원본의 setNoCache(웹 fetch 캐시 끄기)는 옮기지 않았다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;

//==============================================================================
// 글 애셋.
//==============================================================================
pub struct TextAsset {
	assetBase: AssetBase,
	text: Option<String>,
}

impl TextAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TextAsset {
		TextAsset {
			assetBase: AssetBase::new(AssetType::text),
			text: None,
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, assetPath: &str) {
		self.assetBase.requestLoad(engine, assetPath);
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn loadFromBytes(&mut self, assetPath: &str, bytes: Vec<u8>) {
		self.text = None;
		self.assetBase.loadFromBytes(assetPath, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.assetBase.unload();
		self.text = None;
	}

	//==============================================================================
	// 글. (다 읽히기 전이면 None. UTF-8 이 아닌 바이트는 대체 문자로 바꾼다)
	//==============================================================================
	pub fn getText(&mut self) -> Option<&str> {
		if self.text.is_none() {
			let bytes = self.assetBase.getBytes()?;
			let text = String::from_utf8_lossy(&bytes).into_owned();
			self.text = Some(text);
		}
		self.text.as_deref()
	}
}

impl Default for TextAsset {
	fn default() -> TextAsset {
		TextAsset::new()
	}
}

impl Asset for TextAsset {
	fn getAssetBase(&self) -> &AssetBase {
		&self.assetBase
	}

	fn getAssetBaseMut(&mut self) -> &mut AssetBase {
		&mut self.assetBase
	}
}

//==============================================================================
// JSON 애셋.
//==============================================================================
pub struct JsonAsset {
	assetBase: AssetBase,
	data: Option<serde_json::Value>,
	isParseFailed: bool,
}

impl JsonAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> JsonAsset {
		JsonAsset {
			assetBase: AssetBase::new(AssetType::json),
			data: None,
			isParseFailed: false,
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, assetPath: &str) {
		self.assetBase.requestLoad(engine, assetPath);
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn loadFromBytes(&mut self, assetPath: &str, bytes: Vec<u8>) {
		self.data = None;
		self.isParseFailed = false;
		self.assetBase.loadFromBytes(assetPath, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.assetBase.unload();
		self.data = None;
		self.isParseFailed = false;
	}

	//==============================================================================
	// 자료. (원본 data. 다 읽히기 전이거나 해석에 실패하면 None)
	//==============================================================================
	pub fn getData(&mut self) -> Option<&serde_json::Value> {
		if self.data.is_none() && !self.isParseFailed {
			let bytes = self.assetBase.getBytes()?;
			match serde_json::from_slice::<serde_json::Value>(&bytes) {
				Ok(data) => {
					self.data = Some(data);
				}
				Err(error) => {
					eprintln!("JsonAsset: Error loading json: {} ({})", self.assetBase.getAssetPath(), error);
					self.isParseFailed = true;
				}
			}
		}
		self.data.as_ref()
	}
}

impl Default for JsonAsset {
	fn default() -> JsonAsset {
		JsonAsset::new()
	}
}

impl Asset for JsonAsset {
	fn getAssetBase(&self) -> &AssetBase {
		&self.assetBase
	}

	fn getAssetBaseMut(&mut self) -> &mut AssetBase {
		&mut self.assetBase
	}
}
