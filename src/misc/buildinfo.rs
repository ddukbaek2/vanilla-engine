//==============================================================================
// 빌드 정보. (원본 misc/buildinfo.js)
// - 배포 빌드가 적어 둔 판을 앱이 켜질 때 한 번 읽는다. 읽는 차례:
//   1. 웹: 페이지 머리의 <meta name="build-version" content="..."> 의 content. (있으면 그 글 그대로)
//   2. 없으면 version.json. { version, date, time, hash, ... } 꼴이고, 글은 "날짜 시각 (해시)" 이다. 해시가 없으면 version.
//      웹은 페이지 기준 주소를 캐시 없이(fetch 의 cache: "no-cache") 받는다. 새 판을 올려도 옛 판 표시가 남지 않게 한다.
//      데스크탑은 애셋 폴더에서 읽는다. (그 자리에서 끝난다)
//   3. 둘 다 없으면 set_package_version 으로 넣은 판. (게임 크레이트가 env!("CARGO_PKG_VERSION") 을 넘긴다, 넣지 않았으면 원본처럼 빈 글)
// - 원본과 다른 점:
//   원본 생성자의 version.json 주소 기본 인자는 new()(기본 주소)와 set_version_json_path 로 나눴다.
//   원본 await load() 는 받을 때까지 기다린다. 러스트 판도 load(engine) 가 await 할 수 있는 BuildInfoLoadHandle 을 돌려준다. (wait_for_load().await 도 같다)
//   await 없이는 마쳤는지를 is_loaded() 로 보고, 씬은 get_load_handle() 을 track_load_handle 로 맡긴다.
//   meta 는 브라우저에만 있어서 데스크탑은 읽지 않는다. 캐시 끄기도 브라우저 fetch 만의 일이다.
//==============================================================================
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use serde_json::Value;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

//==============================================================================
// 빌드 정보 읽기 손잡이. (BuildInfo::load 가 돌려준다. 원본 load 의 Promise 자리, meta 로 읽었으면 바로 끝난다)
//==============================================================================
#[derive(Clone, Debug)]
pub struct BuildInfoLoadHandle {
	load_handle: Option<AssetLoadHandle>,
}

impl IntoFuture for BuildInfoLoadHandle {
	type Output = ();
	type IntoFuture = BuildInfoLoadFuture;

	//==============================================================================
	// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> BuildInfoLoadFuture {
		BuildInfoLoadFuture {
			load_handle: self.load_handle,
		}
	}
}

//==============================================================================
// 빌드 정보 읽기를 기다리는 퓨처. (실패해도 마친 것이다)
//==============================================================================
#[derive(Clone, Debug)]
pub struct BuildInfoLoadFuture {
	load_handle: Option<AssetLoadHandle>,
}

impl Future for BuildInfoLoadFuture {
	type Output = ();

	//==============================================================================
	// 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut BuildInfoLoadFuture>, context: &mut Context<'_>) -> Poll<()> {
		let is_finished = self.load_handle.as_ref().map(|load_handle| load_handle.is_finished()).unwrap_or(true);
		if is_finished {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

// 빌드 판을 적는 meta 의 이름.
const DEFAULT_META_NAME: &str = "build-version";

// 빌드 판을 적는 파일. (웹은 페이지 기준, 데스크탑은 애셋 폴더 기준)
const DEFAULT_VERSION_JSON_PATH: &str = "version.json";

//==============================================================================
// 빌드 정보.
//==============================================================================
pub struct BuildInfo {
	package_version: String,
	is_load_requested: bool,
	meta_name: String,
	version_json_path: String,
	meta_text: String,
	load_handle: Option<AssetLoadHandle>,
	version_data: Option<Value>,
	is_version_data_read: bool,
}

impl BuildInfo {
	//==============================================================================
	// 생성. (version.json 주소는 기본 "version.json")
	//==============================================================================
	pub fn new() -> BuildInfo {
		BuildInfo {
			package_version: String::new(),
			is_load_requested: false,
			meta_name: String::from(DEFAULT_META_NAME),
			version_json_path: String::from(DEFAULT_VERSION_JSON_PATH),
			meta_text: String::new(),
			load_handle: None,
			version_data: None,
			is_version_data_read: false,
		}
	}

	//==============================================================================
	// meta 이름 설정. (기본 "build-version")
	//==============================================================================
	pub fn set_meta_name(&mut self, meta_name: &str) {
		self.meta_name = meta_name.to_string();
	}

	//==============================================================================
	// 빌드 판 파일 경로 설정. (기본 "version.json", 빈 글이면 파일을 받지 않는다)
	//==============================================================================
	pub fn set_version_json_path(&mut self, version_json_path: &str) {
		self.version_json_path = version_json_path.to_string();
	}

	//==============================================================================
	// 읽기 요청. (웹은 meta 를 바로 읽고, 없으면 version.json 을 캐시 없이 요청한다. 데스크탑은 애셋 폴더의 version.json 을 읽는다. await 할 수 있는 손잡이를 돌려준다)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine) -> BuildInfoLoadHandle {
		self.is_load_requested = true;
		self.meta_text = self.read_meta_text();
		self.load_handle = None;
		self.version_data = None;
		self.is_version_data_read = false;
		if self.meta_text.is_empty() && !self.version_json_path.is_empty() {
			let load_handle = platform_build_info::request_version_json(engine, &self.version_json_path);
			self.load_handle = Some(load_handle);
		}
		BuildInfoLoadHandle {
			load_handle: self.load_handle.clone(),
		}
	}

	//==============================================================================
	// 읽기를 마쳤는지. (원본 isLoaded, load 전이면 거짓, 실패해도 마친 것)
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		if !self.is_load_requested {
			return false;
		}
		match &self.load_handle {
			Some(load_handle) => {
				load_handle.is_finished()
			}
			None => {
				true
			}
		}
	}

	//==============================================================================
	// 읽기를 마치기를 기다리는 퓨처. (원본 await load() 의 await 자리, load 를 부른 뒤 엔진 작업 안에서 await 한다. 실패해도 마친 것)
	// - meta 로 읽었으면 바로 끝난다. 손잡이를 나눠 들어 빌리지 않는다.
	//==============================================================================
	pub fn wait_for_load(&self) -> BuildInfoLoadFuture {
		BuildInfoLoadFuture {
			load_handle: self.load_handle.clone(),
		}
	}

	//==============================================================================
	// version.json 읽기 손잡이. (씬이 track_load_handle 로 맡긴다. meta 로 읽었으면 None)
	//==============================================================================
	pub fn get_load_handle(&self) -> Option<&AssetLoadHandle> {
		self.load_handle.as_ref()
	}

	//==============================================================================
	// version.json 에서 읽은 값. (받기 전이거나 meta 로 읽었거나 풀 수 없으면 None)
	//==============================================================================
	pub fn read_version_data(&mut self) -> Option<&Value> {
		if !self.is_version_data_read {
			let bytes = self.load_handle.as_ref().and_then(|load_handle| load_handle.get_bytes());
			if let Some(bytes) = bytes {
				self.is_version_data_read = true;
				self.version_data = match serde_json::from_slice::<Value>(&bytes) {
					Ok(Value::Object(object_map)) => {
						Some(Value::Object(object_map))
					}
					_ => {
						None
					}
				};
			}
		}
		self.version_data.as_ref()
	}

	//==============================================================================
	// 판의 글. (meta > version.json > 빌드 때 넣은 판)
	//==============================================================================
	pub fn read_text(&mut self) -> String {
		if !self.meta_text.is_empty() {
			return self.meta_text.clone();
		}
		if let Some(version_data) = self.read_version_data() {
			let composed_text = BuildInfo::compose_text(version_data);
			if !composed_text.is_empty() {
				return composed_text;
			}
		}
		self.package_version.clone()
	}

	//==============================================================================
	// 페이지의 meta 에 적힌 판. (원본 readMetaText, 데스크탑과 없으면 빈 글)
	//==============================================================================
	pub fn read_meta_text(&self) -> String {
		platform_build_info::read_meta_text(&self.meta_name)
	}

	//==============================================================================
	// 빌드 때 넣은 판 설정. (meta 도 version.json 도 없을 때의 글, 보통 env!("CARGO_PKG_VERSION"))
	//==============================================================================
	pub fn set_package_version(&mut self, package_version: &str) {
		self.package_version = package_version.to_string();
	}

	//==============================================================================
	// 빌드 때 넣은 판.
	//==============================================================================
	pub fn get_package_version(&self) -> &str {
		&self.package_version
	}

	//==============================================================================
	// version.json 값으로 판의 글 만들기. (정적, "날짜 시각 (해시)", 해시가 없으면 version, 그것도 없으면 빈 글)
	//==============================================================================
	pub fn compose_text(version_data: &Value) -> String {
		let read_string = |name: &str| -> String {
			version_data.get(name).and_then(|value| value.as_str()).map(|text| text.trim().to_string()).unwrap_or_default()
		};
		let hash = read_string("hash");
		if hash.is_empty() {
			return read_string("version");
		}
		let mut part_list: Vec<String> = Vec::new();
		let date = read_string("date");
		if !date.is_empty() {
			part_list.push(date);
		}
		let time = read_string("time");
		if !time.is_empty() {
			part_list.push(time);
		}
		part_list.push(format!("({})", hash));
		part_list.join(" ")
	}
}

impl Default for BuildInfo {
	fn default() -> BuildInfo {
		BuildInfo::new()
	}
}

//==============================================================================
// 데스크탑. (meta 가 없고, version.json 은 애셋 폴더에서 읽는다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod platform_build_info {
	use crate::core::asset::AssetLoadHandle;
	use crate::core::engine::Engine;

	//==============================================================================
	// meta 의 판. (데스크탑은 없다)
	//==============================================================================
	pub fn read_meta_text(_meta_name: &str) -> String {
		String::new()
	}

	//==============================================================================
	// version.json 요청. (애셋 폴더에서 그 자리에서 읽는다)
	//==============================================================================
	pub fn request_version_json(engine: &Engine, version_json_path: &str) -> AssetLoadHandle {
		engine.request_asset_bytes(version_json_path)
	}
}

//==============================================================================
// 웹. (페이지의 meta 와, 페이지 기준 version.json 을 캐시 없이)
//==============================================================================
#[cfg(target_arch = "wasm32")]
mod platform_build_info {
	use crate::core::asset::AssetLoadHandle;
	use crate::core::engine::Engine;
	use wasm_bindgen::{JsCast, JsValue};

	//==============================================================================
	// meta 의 판. (없으면 빈 글)
	//==============================================================================
	pub fn read_meta_text(meta_name: &str) -> String {
		let document = match web_sys::window().and_then(|window| window.document()) {
			Some(document) => {
				document
			}
			None => {
				return String::new();
			}
		};
		let selector = format!("meta[name=\"{}\"]", meta_name);
		match document.query_selector(&selector) {
			Ok(Some(meta_element)) => {
				meta_element.get_attribute("content").map(|content| content.trim().to_string()).unwrap_or_default()
			}
			_ => {
				String::new()
			}
		}
	}

	//==============================================================================
	// version.json 요청. (fetch 를 cache: "no-cache" 로 부른다. 받으면 손잡이를 끝낸다)
	//==============================================================================
	pub fn request_version_json(_engine: &Engine, version_json_path: &str) -> AssetLoadHandle {
		let load_handle = AssetLoadHandle::create_pending(version_json_path);
		let load_handle_for_fetch = load_handle.clone();
		let version_json_path = version_json_path.to_string();
		wasm_bindgen_futures::spawn_local(async move {
			let result = fetch_without_cache(&version_json_path).await;
			load_handle_for_fetch.finish(result);
		});
		load_handle
	}

	//==============================================================================
	// 캐시 없이 받기. (web-sys 의 RequestInit 대신 fetch 함수와 init 객체를 직접 만든다)
	//==============================================================================
	async fn fetch_without_cache(path: &str) -> Result<Vec<u8>, String> {
		let window = web_sys::window().ok_or_else(|| String::from("window 없음"))?;
		let init_object = js_sys::Object::new();
		js_sys::Reflect::set(&init_object, &JsValue::from_str("cache"), &JsValue::from_str("no-cache")).map_err(|_| String::from("fetch 설정 실패"))?;
		let fetch_value = js_sys::Reflect::get(&window, &JsValue::from_str("fetch")).map_err(|_| String::from("fetch 없음"))?;
		let fetch_function: js_sys::Function = fetch_value.dyn_into().map_err(|_| String::from("fetch 가 함수가 아님"))?;
		let promise_value = fetch_function.call2(&window, &JsValue::from_str(path), &init_object).map_err(|_| format!("빌드 판 받기 실패 ({})", path))?;
		let promise: js_sys::Promise = promise_value.dyn_into().map_err(|_| format!("빌드 판 응답이 아님 ({})", path))?;
		let response_value = wasm_bindgen_futures::JsFuture::from(promise).await.map_err(|_| format!("빌드 판 받기 실패 ({})", path))?;
		let response: web_sys::Response = response_value.dyn_into().map_err(|_| format!("빌드 판 응답이 아님 ({})", path))?;
		if !response.ok() {
			return Err(format!("빌드 판 받기 실패 ({}): HTTP {}", path, response.status()));
		}
		let array_buffer_promise = response.array_buffer().map_err(|_| format!("빌드 판 본문 읽기 실패 ({})", path))?;
		let array_buffer = wasm_bindgen_futures::JsFuture::from(array_buffer_promise).await.map_err(|_| format!("빌드 판 본문 읽기 실패 ({})", path))?;
		Ok(js_sys::Uint8Array::new(&array_buffer).to_vec())
	}
}

//==============================================================================
// 시험. (데스크탑, 임시 애셋 폴더)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;

	//==============================================================================
	// version.json 이 있으면 "날짜 시각 (해시)", 없으면 빌드 때 넣은 판.
	//==============================================================================
	#[test]
	fn read_text_from_version_json_or_package_version() {
		let asset_directory_path = std::env::temp_dir().join(format!("vanillaBuildInfoTest-{}", std::process::id()));
		std::fs::create_dir_all(&asset_directory_path).expect("asset directory");
		std::fs::write(asset_directory_path.join("version.json"), "{\"version\":\"1.2.3\",\"date\":\"2026-11-25\",\"time\":\"04:00\",\"hash\":\"abc1234\"}").expect("version file");
		let mut engine_configuration = EngineConfiguration::new();
		engine_configuration.asset_directory_paths = vec![asset_directory_path.clone()];
		let engine = Engine::new(engine_configuration);

		let mut build_info = BuildInfo::new();
		build_info.set_package_version("0.9.0");
		assert_eq!(build_info.read_text(), "0.9.0");
		assert!(!build_info.is_loaded());
		build_info.load(&engine);
		assert!(build_info.is_loaded());
		assert_eq!(build_info.read_text(), "2026-11-25 04:00 (abc1234)");
		assert_eq!(build_info.read_version_data().expect("version data")["version"], serde_json::json!("1.2.3"));

		let mut missing_build_info = BuildInfo::new();
		missing_build_info.set_package_version("0.9.0");
		missing_build_info.set_version_json_path("missing-version.json");
		missing_build_info.load(&engine);
		assert!(missing_build_info.is_loaded());
		assert!(missing_build_info.read_version_data().is_none());
		assert_eq!(missing_build_info.read_text(), "0.9.0");
		assert_eq!(missing_build_info.get_package_version(), "0.9.0");

		assert_eq!(BuildInfo::compose_text(&serde_json::json!({ "version": " 2.0.0 " })), "2.0.0");
		assert_eq!(BuildInfo::compose_text(&serde_json::json!({ "hash": "def5678" })), "(def5678)");
		assert_eq!(BuildInfo::default().read_text(), "");
		let _ = std::fs::remove_dir_all(&asset_directory_path);
	}
}
