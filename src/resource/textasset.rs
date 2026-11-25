//==============================================================================
//! 글 애셋. (원본 resource/textasset.js, resource/jsonasset.js)
//! - 원본 TextAsset 은 fetch 로 받은 글을 text 에, JsonAsset 은 JSON.parse 한 것을 data 에 둔다.
//! - 러스트 판은 바이트가 다 오면 처음 꺼낼 때 UTF-8 글로 풀고, JsonAsset 은 serde_json::Value 로 푼다.
//! - JsonAsset 은 원본처럼 TextAsset 의 글(get_text)도 들고, 풀지 못한 JSON 은 읽히지 않은 것(is_loaded 거짓, get_data None)이다.
//!   AssetLoader 로 받으면 원본처럼 실패 목록에 든다. (푼 자료는 AssetLoader 의 목록 칸과 나눠 들어 한 번만 푼다)
//! - set_no_cache: 참이면 웹 fetch 에 cache: "no-cache" 를 붙여 배포 뒤 옛 자료를 쓰지 않게 한다. (원본과 같다, 데스크탑 파일 읽기에는 캐시가 없다)
//! - load 는 원본처럼 await 할 수 있는 읽기 손잡이를 돌려준다. (원본 JsonAsset.load 처럼 풀지 못한 JSON 도 받기만 끝나면 끝나고 get_data 가 None 이다)
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use std::cell::OnceCell;
use std::rc::Rc;

/// 푼 JSON. (풀지 못했으면 까닭, 바이트가 오면 처음 볼 때 한 번 채운다)
pub(crate) type ParsedJsonCell = Rc<OnceCell<Result<serde_json::Value, String>>>;

//==============================================================================
/// JSON 바이트 풀기. (풀지 못하면 원본처럼 "Error loading json: 경로" 를 알리고 까닭을 돌려준다)
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
/// 글 애셋.
/// - 바이트가 다 오면 처음 get_text 할 때 UTF-8 글로 푼다.
///
/// # 예제
/// data: 주소와 가진 바이트로 읽기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, TextAsset};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut text_asset = TextAsset::new();
/// text_asset.load(&engine, "data:,hello");
/// assert_eq!(text_asset.get_text(), Some("hello"));
///
/// let mut note_asset = TextAsset::new();
/// assert_eq!(note_asset.get_text(), None);
/// note_asset.load_from_bytes("note.txt", b"line one".to_vec());
/// assert_eq!(note_asset.get_text(), Some("line one"));
/// ```
//==============================================================================
pub struct TextAsset {
	asset_base: AssetBase,
	text: Option<String>,
	is_no_cache: bool,
}

impl TextAsset {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 요청 전의 빈 글 애셋. (캐시 무효화 꺼짐)
	//==============================================================================
	pub fn new() -> TextAsset {
		TextAsset {
			asset_base: AssetBase::new(AssetType::Text),
			text: None,
			is_no_cache: false,
		}
	}

	//==============================================================================
	/// 캐시 무효화 로드 여부 설정. (참이면 웹 fetch 에 cache: "no-cache" 를 붙인다, 데이터 표 JSON 등)
	///
	/// # 인자
	/// - `is_no_cache`: true 면 웹에서 브라우저 캐시를 쓰지 않는다. (load 전에 정한다, 데스크탑은 보지 않는다)
	//==============================================================================
	pub fn set_no_cache(&mut self, is_no_cache: bool) {
		self.is_no_cache = is_no_cache;
	}

	//==============================================================================
	/// 캐시 무효화 로드 여부 반환.
	///
	/// # 돌려줌
	/// 웹에서 브라우저 캐시를 쓰지 않으면 true. (기본 false)
	//==============================================================================
	pub fn is_no_cache(&self) -> bool {
		self.is_no_cache
	}

	//==============================================================================
	/// 읽기 요청. (원본 load, await 할 수 있는 읽기 손잡이를 돌려준다)
	/// - 이미 요청했으면 다시 하지 않고 처음 손잡이를 돌려준다.
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `asset_path`: 애셋 경로. (데스크탑은 애셋 폴더 기준, 웹은 asset_base_url 기준, data: 주소도 된다)
	///
	/// # 돌려줌
	/// 읽기 손잡이. (데스크탑은 그 자리에서 읽혀 끝나 있다. 못 읽으면 실패로 끝난다)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		self.asset_base.request_load_with_cache(engine, asset_path, self.is_no_cache)
	}

	//==============================================================================
	/// 이미 가진 바이트로 읽기.
	/// - 앞서 푼 글은 버린다. 이미 요청한 손잡이가 있어도 새 손잡이로 바꾼다.
	///
	/// # 인자
	/// - `asset_path`: 적어 둘 경로.
	/// - `bytes`: 글 바이트. (UTF-8)
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.text = None;
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	/// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.text = None;
	}

	//==============================================================================
	/// 글. (다 읽히기 전이면 None. UTF-8 이 아닌 바이트는 대체 문자로 바꾼다)
	///
	/// # 돌려줌
	/// 읽은 글. 요청 전이거나 읽는 중이거나 실패했으면 None.
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
	//==============================================================================
	/// 요청 전의 빈 글 애셋. (new 와 같다)
	//==============================================================================
	fn default() -> TextAsset {
		TextAsset::new()
	}
}

impl Asset for TextAsset {
	//==============================================================================
	/// 공통 상태.
	//==============================================================================
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	//==============================================================================
	/// 공통 상태. (변경용)
	//==============================================================================
	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}

//==============================================================================
/// JSON 애셋.
/// - 바이트가 다 오면 처음 볼 때 serde_json::Value 로 한 번 푼다. 풀지 못한 JSON 은 읽히지 않은 것(is_loaded 거짓, get_data None)이고 글은 남는다.
///
/// # 예제
/// 저장소의 아틀라스 JSON 을 읽어 값 보기
/// ```rust
/// use vanilla::{Asset, Engine, EngineConfiguration, JsonAsset};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut json_asset = JsonAsset::new();
/// json_asset.set_no_cache(true);
/// json_asset.load(&engine, "examples/assets/atlas/icons.json");
/// assert!(json_asset.is_loaded());
/// let data = json_asset.get_data().expect("json data");
/// assert_eq!(data["meta"]["imageFileName"], "icons.png");
/// assert_eq!(data["images"]["coin.png"]["width"], 30);
/// ```
///
/// 풀지 못한 JSON 은 읽히지 않은 것이고 글은 남는다
/// ```rust
/// use vanilla::{Asset, JsonAsset};
///
/// let mut json_asset = JsonAsset::new();
/// json_asset.load_from_bytes("broken.json", b"{\"count\": ".to_vec());
/// assert!(!json_asset.is_loaded());
/// assert!(json_asset.get_data().is_none());
/// assert_eq!(json_asset.get_text(), Some("{\"count\": "));
/// ```
//==============================================================================
pub struct JsonAsset {
	asset_base: AssetBase,
	/// 푼 자료. (AssetLoader 의 목록 칸과 나눠 든다)
	parsed_data: ParsedJsonCell,
	text: Option<String>,
	is_no_cache: bool,
}

impl JsonAsset {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 요청 전의 빈 JSON 애셋. (캐시 무효화 꺼짐)
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
	/// 캐시 무효화 로드 여부 설정. (참이면 웹 fetch 에 cache: "no-cache" 를 붙인다, 데이터 표 JSON 등)
	///
	/// # 인자
	/// - `is_no_cache`: true 면 웹에서 브라우저 캐시를 쓰지 않는다. (load 전에 정한다, 데스크탑은 보지 않는다)
	//==============================================================================
	pub fn set_no_cache(&mut self, is_no_cache: bool) {
		self.is_no_cache = is_no_cache;
	}

	//==============================================================================
	/// 캐시 무효화 로드 여부 반환.
	///
	/// # 돌려줌
	/// 웹에서 브라우저 캐시를 쓰지 않으면 true. (기본 false)
	//==============================================================================
	pub fn is_no_cache(&self) -> bool {
		self.is_no_cache
	}

	//==============================================================================
	/// 읽기 요청. (원본 load, await 할 수 있는 읽기 손잡이를 돌려준다. 원본처럼 풀지 못한 JSON 도 받기만 끝나면 끝나고 get_data 가 None 이다)
	/// - 이미 요청했으면 다시 하지 않고 처음 손잡이를 돌려준다.
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `asset_path`: 애셋 경로. (데스크탑은 애셋 폴더 기준, 웹은 asset_base_url 기준, data: 주소도 된다)
	///
	/// # 돌려줌
	/// 읽기 손잡이. (데스크탑은 그 자리에서 읽혀 끝나 있다. 못 읽으면 실패로 끝난다)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		self.asset_base.request_load_with_cache(engine, asset_path, self.is_no_cache)
	}

	//==============================================================================
	/// 이미 가진 바이트로 읽기.
	/// - 앞서 푼 자료와 글은 버리고 새로 푼다. 이미 요청한 손잡이가 있어도 새 손잡이로 바꾼다.
	///
	/// # 인자
	/// - `asset_path`: 적어 둘 경로. (풀지 못했을 때 알림 글에 쓴다)
	/// - `bytes`: JSON 글 바이트. (UTF-8)
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.parsed_data = Rc::new(OnceCell::new());
		self.text = None;
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	/// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.parsed_data = Rc::new(OnceCell::new());
		self.text = None;
	}

	//==============================================================================
	/// 푼 결과. (바이트가 오기 전이면 None, 처음 볼 때 한 번 푼다)
	//==============================================================================
	fn read_parsed_data(&self) -> Option<&Result<serde_json::Value, String>> {
		let bytes = self.asset_base.get_bytes()?;
		Some(self.parsed_data.get_or_init(|| parse_json_bytes(&bytes, self.asset_base.get_asset_path())))
	}

	//==============================================================================
	/// 자료. (원본 data. 다 읽히기 전이거나 해석에 실패하면 None)
	///
	/// # 돌려줌
	/// 푼 JSON 값. 요청 전이거나 읽는 중이거나 읽기나 풀기에 실패했으면 None.
	//==============================================================================
	pub fn get_data(&self) -> Option<&serde_json::Value> {
		self.read_parsed_data()?.as_ref().ok()
	}

	//==============================================================================
	/// 글. (원본 TextAsset 의 text. 다 읽히기 전이면 None, 풀지 못한 JSON 도 글은 있다. UTF-8 이 아닌 바이트는 대체 문자로 바꾼다)
	///
	/// # 돌려줌
	/// 읽은 JSON 글. 요청 전이거나 읽는 중이거나 읽기에 실패했으면 None.
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
	/// 나눠 드는 푼 자료. (AssetLoader 가 받은 뒤 실패인지 볼 때)
	//==============================================================================
	pub(crate) fn get_parsed_data_cell(&self) -> ParsedJsonCell {
		self.parsed_data.clone()
	}
}

impl Default for JsonAsset {
	//==============================================================================
	/// 요청 전의 빈 JSON 애셋. (new 와 같다)
	//==============================================================================
	fn default() -> JsonAsset {
		JsonAsset::new()
	}
}

impl Asset for JsonAsset {
	//==============================================================================
	/// 공통 상태.
	//==============================================================================
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	//==============================================================================
	/// 공통 상태. (변경용)
	//==============================================================================
	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}

	//==============================================================================
	/// 다 읽혔는지. (바이트가 오고 JSON 으로 풀렸을 때, 원본처럼 풀지 못하면 거짓)
	//==============================================================================
	fn is_loaded(&self) -> bool {
		self.get_data().is_some()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 깨진 JSON 은 원본처럼 읽히지 않은 것이고 글은 남는다. 다시 읽으면 새로 푼다.
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
