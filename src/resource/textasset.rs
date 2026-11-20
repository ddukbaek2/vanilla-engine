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
	asset_base: AssetBase,
	text: Option<String>,
}

impl TextAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TextAsset {
		TextAsset {
			asset_base: AssetBase::new(AssetType::Text),
			text: None,
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) {
		self.asset_base.request_load(engine, asset_path);
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.text = None;
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.text = None;
	}

	//==============================================================================
	// 글. (다 읽히기 전이면 None. UTF-8 이 아닌 바이트는 대체 문자로 바꾼다)
	//==============================================================================
	pub fn get_text(&mut self) -> Option<&str> {
		if self.text.is_none() {
			let bytes = self.asset_base.get_bytes()?;
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
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}

//==============================================================================
// JSON 애셋.
//==============================================================================
pub struct JsonAsset {
	asset_base: AssetBase,
	data: Option<serde_json::Value>,
	is_parse_failed: bool,
}

impl JsonAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> JsonAsset {
		JsonAsset {
			asset_base: AssetBase::new(AssetType::Json),
			data: None,
			is_parse_failed: false,
		}
	}

	//==============================================================================
	// 읽기 요청. (원본 load)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) {
		self.asset_base.request_load(engine, asset_path);
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.data = None;
		self.is_parse_failed = false;
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.data = None;
		self.is_parse_failed = false;
	}

	//==============================================================================
	// 자료. (원본 data. 다 읽히기 전이거나 해석에 실패하면 None)
	//==============================================================================
	pub fn get_data(&mut self) -> Option<&serde_json::Value> {
		if self.data.is_none() && !self.is_parse_failed {
			let bytes = self.asset_base.get_bytes()?;
			match serde_json::from_slice::<serde_json::Value>(&bytes) {
				Ok(data) => {
					self.data = Some(data);
				}
				Err(error) => {
					eprintln!("JsonAsset: Error loading json: {} ({})", self.asset_base.get_asset_path(), error);
					self.is_parse_failed = true;
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
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}
