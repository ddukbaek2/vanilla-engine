//==============================================================================
// 글 애셋. (원본 resource/textasset.js, resource/jsonasset.js)
// - 원본 TextAsset 은 fetch 로 받은 글을 text 에, JsonAsset 은 JSON.parse 한 것을 data 에 둔다.
// - 러스트 판은 바이트가 다 오면 처음 꺼낼 때 UTF-8 글로 풀고, JsonAsset 은 serde_json::Value 로 푼다.
// - JsonAsset 은 원본처럼 TextAsset 의 글(get_text)도 들고, 풀지 못한 JSON 은 읽히지 않은 것(is_loaded 거짓, get_data None)이다.
//   AssetLoader 로 받으면 원본처럼 실패 목록에 든다. (푼 자료는 AssetLoader 의 목록 칸과 나눠 들어 한 번만 푼다)
// - set_no_cache: 참이면 웹 fetch 에 cache: "no-cache" 를 붙여 배포 뒤 옛 자료를 쓰지 않게 한다. (원본과 같다, 데스크탑 파일 읽기에는 캐시가 없다)
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetType};
use crate::core::engine::Engine;
use std::cell::OnceCell;
use std::rc::Rc;

// 푼 JSON. (풀지 못했으면 까닭, 바이트가 오면 처음 볼 때 한 번 채운다)
pub(crate) type ParsedJsonCell = Rc<OnceCell<Result<serde_json::Value, String>>>;

//==============================================================================
// JSON 바이트 풀기. (풀지 못하면 원본처럼 "Error loading json: 경로" 를 알리고 까닭을 돌려준다)
//==============================================================================
pub(crate) fn parse_json_bytes(bytes: &[u8], asset_path: &str) -> Result<serde_json::Value, String> {
	match serde_json::from_slice::<serde_json::Value>(bytes) {
		Ok(data) => {
			Ok(data)
		}
		Err(error) => {
			let error_message = format!("Error loading json: {} ({})", asset_path, error);
			eprintln!("JsonAsset: {}", error_message);
			Err(error_message)
		}
	}
}

//==============================================================================
// 글 애셋.
//==============================================================================
pub struct TextAsset {
	asset_base: AssetBase,
	text: Option<String>,
	is_no_cache: bool,
}

impl TextAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TextAsset {
		TextAsset {
			asset_base: AssetBase::new(AssetType::Text),
			text: None,
			is_no_cache: false,
		}
	}

	//==============================================================================
	// 캐시 무효화 로드 여부 설정. (참이면 웹 fetch 에 cache: "no-cache" 를 붙인다, 데이터 표 JSON 등)
	//==============================================================================
	pub fn set_no_cache(&mut self, is_no_cache: bool) {
		self.is_no_cache = is_no_cache;
	}

	//==============================================================================
	// 캐시 무효화 로드 여부 반환.
	//==============================================================================
	pub fn is_no_cache(&self) -> bool {
		self.is_no_cache
	}

	//==============================================================================
	// 읽기 요청. (원본 load)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) {
		self.asset_base.request_load_with_cache(engine, asset_path, self.is_no_cache);
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
	// 푼 자료. (AssetLoader 의 목록 칸과 나눠 든다)
	parsed_data: ParsedJsonCell,
	text: Option<String>,
	is_no_cache: bool,
}

impl JsonAsset {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> JsonAsset {
		JsonAsset {
			asset_base: AssetBase::new(AssetType::Json),
			parsed_data: Rc::new(OnceCell::new()),
			text: None,
			is_no_cache: false,
		}
	}

	//==============================================================================
	// 캐시 무효화 로드 여부 설정. (참이면 웹 fetch 에 cache: "no-cache" 를 붙인다, 데이터 표 JSON 등)
	//==============================================================================
	pub fn set_no_cache(&mut self, is_no_cache: bool) {
		self.is_no_cache = is_no_cache;
	}

	//==============================================================================
	// 캐시 무효화 로드 여부 반환.
	//==============================================================================
	pub fn is_no_cache(&self) -> bool {
		self.is_no_cache
	}

	//==============================================================================
	// 읽기 요청. (원본 load)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) {
		self.asset_base.request_load_with_cache(engine, asset_path, self.is_no_cache);
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.parsed_data = Rc::new(OnceCell::new());
		self.text = None;
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.parsed_data = Rc::new(OnceCell::new());
		self.text = None;
	}

	//==============================================================================
	// 푼 결과. (바이트가 오기 전이면 None, 처음 볼 때 한 번 푼다)
	//==============================================================================
	fn read_parsed_data(&self) -> Option<&Result<serde_json::Value, String>> {
		let bytes = self.asset_base.get_bytes()?;
		Some(self.parsed_data.get_or_init(|| parse_json_bytes(&bytes, self.asset_base.get_asset_path())))
	}

	//==============================================================================
	// 자료. (원본 data. 다 읽히기 전이거나 해석에 실패하면 None)
	//==============================================================================
	pub fn get_data(&self) -> Option<&serde_json::Value> {
		self.read_parsed_data()?.as_ref().ok()
	}

	//==============================================================================
	// 글. (원본 TextAsset 의 text. 다 읽히기 전이면 None, 풀지 못한 JSON 도 글은 있다. UTF-8 이 아닌 바이트는 대체 문자로 바꾼다)
	//==============================================================================
	pub fn get_text(&mut self) -> Option<&str> {
		if self.text.is_none() {
			let bytes = self.asset_base.get_bytes()?;
			let text = String::from_utf8_lossy(&bytes).into_owned();
			self.text = Some(text);
		}
		self.text.as_deref()
	}

	//==============================================================================
	// 나눠 드는 푼 자료. (AssetLoader 가 받은 뒤 실패인지 볼 때)
	//==============================================================================
	pub(crate) fn get_parsed_data_cell(&self) -> ParsedJsonCell {
		self.parsed_data.clone()
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

	//==============================================================================
	// 다 읽혔는지. (바이트가 오고 JSON 으로 풀렸을 때, 원본처럼 풀지 못하면 거짓)
	//==============================================================================
	fn is_loaded(&self) -> bool {
		self.get_data().is_some()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 깨진 JSON 은 원본처럼 읽히지 않은 것이고 글은 남는다. 다시 읽으면 새로 푼다.
	//==============================================================================
	#[test]
	fn broken_json_is_not_loaded_but_keeps_text() {
		let mut json_asset = JsonAsset::new();
		assert!(!json_asset.is_loaded());
		assert!(json_asset.get_text().is_none());
		json_asset.load_from_bytes("broken.json", b"{\"count\": ".to_vec());
		assert!(!json_asset.is_loaded());
		assert!(json_asset.get_data().is_none());
		assert_eq!(json_asset.get_text(), Some("{\"count\": "));
		json_asset.load_from_bytes("good.json", b"{\"count\": 3}".to_vec());
		assert!(json_asset.is_loaded());
		assert_eq!(json_asset.get_data().expect("data")["count"], serde_json::json!(3));
		assert_eq!(json_asset.get_text(), Some("{\"count\": 3}"));
		json_asset.unload();
		assert!(!json_asset.is_loaded());
	}
}
