//==============================================================================
// 플랫폼 정보. (원본 base/platform.js)
// - 원본은 navigator.user_agent 를 보고 플랫폼과 브라우저를 고른다.
// - 러스트 판은 웹(wasm32)에서는 원본과 같은 규칙으로 user_agent 를 보고, 데스크탑에서는 빌드 대상 OS 를 쓴다.
//   데스크탑의 브라우저는 unknown 이고, 스팀덱 구분은 아직 하지 않는다(리눅스로 나온다).
// - 원본의 getOrAddCanvas 는 platform/web.rs 가 엔진 설정의 캔버스 id 로 한다.
// - get_resouce_usage(원본 이름의 오타를 그대로 둔다): 웹은 원본처럼 performance 의 resource 항목을 더하고, 데스크탑은 네트워크로 받은 것이 없어 비어 있다.
// - get_safe_area_rect: 원본은 캔버스(없으면 창)의 크기를 읽는다. 러스트 판은 문서 객체가 없어 그 크기를 받는다. 데스크탑의 인셋은 0 이다.
// - is_coarse_pointer: 원본처럼 CSS pointer: coarse, 데스크탑은 마우스가 주된 손이라 거짓이다.
// - is_desktop_web: 원본은 부를 때마다 플랫폼 정보를 새로 읽는다. 러스트 판은 만들 때 읽은 것을 쓴다. (user_agent 는 바뀌지 않는다)
//   데스크탑 빌드는 모바일이 아니고 손가락 입력 수(navigator.maxTouchPoints)가 없어 늘 true 다.
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;

// 공통 시스템 폰트 목록. (원본 SYSTEM_FONT_STRING, 웹 CSS 글꼴 목록)
pub const SYSTEM_FONT_STRING: &str = "-apple-system, \"Segoe UI\", Roboto, \"Apple SD Gothic Neo\", \"Malgun Gothic\", sans-serif";

//==============================================================================
// 플랫폼 타입.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformType {
	Windows,
	MacOs,
	Android,
	SteamDeck,
	IPadOs,
	IOs,
	Linux,
	Unknown,
}

impl PlatformType {
	//==============================================================================
	// 원본과 같은 이름 글. ("Windows", "macOS" ...)
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		match self {
			PlatformType::Windows => {
				"Windows"
			}
			PlatformType::MacOs => {
				"macOS"
			}
			PlatformType::Android => {
				"Android"
			}
			PlatformType::SteamDeck => {
				"SteamDeck"
			}
			PlatformType::IPadOs => {
				"iPadOS"
			}
			PlatformType::IOs => {
				"iOS"
			}
			PlatformType::Linux => {
				"Linux"
			}
			PlatformType::Unknown => {
				"Unknown"
			}
		}
	}
}

//==============================================================================
// 브라우저 타입.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserType {
	Chrome,
	Edge,
	Firefox,
	InternetExplorer,
	Safari,
	Unknown,
}

impl BrowserType {
	//==============================================================================
	// 원본과 같은 이름 글. ("Chrome", "Edge" ...)
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		match self {
			BrowserType::Chrome => {
				"Chrome"
			}
			BrowserType::Edge => {
				"Edge"
			}
			BrowserType::Firefox => {
				"Firefox"
			}
			BrowserType::InternetExplorer => {
				"Internet Explorer"
			}
			BrowserType::Safari => {
				"Safari"
			}
			BrowserType::Unknown => {
				"Unknown"
			}
		}
	}
}

//==============================================================================
// 플랫폼 정보.
//==============================================================================
#[derive(Clone, Debug)]
pub struct Platform {
	platform_name: PlatformType,
	browser_name: BrowserType,
	is_mobile: bool,
}

impl Platform {
	//==============================================================================
	// 생성. (탐지까지 한다. 원본은 생성 뒤 get_platform_info 를 부른다)
	//==============================================================================
	pub fn new() -> Platform {
		let mut platform = Platform {
			platform_name: PlatformType::Unknown,
			browser_name: BrowserType::Unknown,
			is_mobile: false,
		};
		platform.get_platform_info();
		platform
	}

	//==============================================================================
	// 플랫폼 정보 탐지 및 적용. (플랫폼, 브라우저)
	//==============================================================================
	pub fn get_platform_info(&mut self) -> (PlatformType, BrowserType) {
		#[cfg(target_arch = "wasm32")]
		{
			let user_agent = web_sys::window().and_then(|window| window.navigator().user_agent().ok()).unwrap_or_default();
			self.apply_user_agent(&user_agent);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.platform_name = detect_native_platform_type();
			self.browser_name = BrowserType::Unknown;
			self.is_mobile = matches!(self.platform_name, PlatformType::Android | PlatformType::IOs | PlatformType::IPadOs);
		}
		(self.platform_name, self.browser_name)
	}

	//==============================================================================
	// user_agent 로 플랫폼과 브라우저 고르기. (원본 get_platform_info 의 정규식과 같은 순서, 대소문자 무시)
	//==============================================================================
	pub fn apply_user_agent(&mut self, user_agent: &str) {
		let lower_user_agent = user_agent.to_lowercase();
		let contains_any = |word_list: &[&str]| word_list.iter().any(|word| lower_user_agent.contains(word));
		self.is_mobile = contains_any(&["mobi", "android", "iphone", "ipad"]);

		// 플랫폼 감지.
		self.platform_name = if contains_any(&["windows"]) {
			PlatformType::Windows
		}
		else if contains_any(&["valve steam gameoverlay"]) {
			PlatformType::SteamDeck
		}
		else if contains_any(&["ipad"]) {
			PlatformType::IPadOs
		}
		else if contains_any(&["iphone", "ipod"]) {
			PlatformType::IOs
		}
		else if contains_any(&["macintosh", "mac os x"]) {
			PlatformType::MacOs
		}
		else if contains_any(&["android"]) {
			PlatformType::Android
		}
		else if contains_any(&["linux"]) {
			PlatformType::Linux
		}
		else {
			PlatformType::Unknown
		};

		// 브라우저 감지.
		self.browser_name = if contains_any(&["edg"]) {
			BrowserType::Edge
		}
		else if contains_any(&["chrome"]) {
			BrowserType::Chrome
		}
		else if contains_any(&["safari"]) {
			BrowserType::Safari
		}
		else if contains_any(&["firefox"]) {
			BrowserType::Firefox
		}
		else if contains_any(&["msie", "trident"]) {
			BrowserType::InternetExplorer
		}
		else {
			BrowserType::Unknown
		};
	}

	//==============================================================================
	// 플랫폼.
	//==============================================================================
	pub fn get_platform_name(&self) -> PlatformType {
		self.platform_name
	}

	//==============================================================================
	// 브라우저.
	//==============================================================================
	pub fn get_browser_name(&self) -> BrowserType {
		self.browser_name
	}

	//==============================================================================
	// 모바일인지.
	//==============================================================================
	pub fn is_mobile(&self) -> bool {
		self.is_mobile
	}

	//==============================================================================
	// 데스크탑 웹인지. (마우스 휠, 키보드 안내처럼 데스크탑에서만 보일 것을 가릴 때 쓴다)
	// - 모바일이면 아니다. 아이패드의 사파리는 데스크탑처럼 맥이라고 알리므로, 맥인데 손가락 입력이 여럿 되면 아이패드로 본다.
	//==============================================================================
	pub fn is_desktop_web(&self) -> bool {
		self.is_desktop_web_with_touch_points(read_max_touch_points())
	}

	//==============================================================================
	// 주된 손이 손가락인지. (CSS 의 pointer: coarse, 단축키 안내처럼 마우스로 하는 화면에만 보일 것을 가릴 때 쓴다)
	//==============================================================================
	pub fn is_coarse_pointer(&self) -> bool {
		crate::platform::is_coarse_pointer()
	}

	//==============================================================================
	// 리소스 사용량. (웹이 받은 파일마다 전송량과 푼 크기, 원본 이름 그대로)
	//==============================================================================
	pub fn get_resouce_usage(&self) -> ResourceUsage {
		read_resource_usage()
	}

	//==============================================================================
	// 세이프 에어리어. (area_size 크기에서 env(safe-area-inset-*) 를 뺀 사각형, 인셋은 원본 parseInt 처럼 정수로 자른다)
	//==============================================================================
	pub fn get_safe_area_rect(&self, area_size: Vector2) -> Rect {
		let insets = crate::platform::read_safe_area_insets();
		let top = insets[0].trunc();
		let right = insets[1].trunc();
		let bottom = insets[2].trunc();
		let left = insets[3].trunc();
		Rect::create(left, top, area_size.x - left - right, area_size.y - top - bottom)
	}

	//==============================================================================
	// 손가락 입력 수를 받아 데스크탑 웹인지. (is_desktop_web 의 몸통)
	//==============================================================================
	fn is_desktop_web_with_touch_points(&self, max_touch_points: i32) -> bool {
		if self.is_mobile {
			return false;
		}
		if self.platform_name == PlatformType::MacOs && max_touch_points > 1 {
			return false;
		}
		true
	}
}

impl Default for Platform {
	fn default() -> Platform {
		Platform::new()
	}
}

//==============================================================================
// 받은 파일 하나. (원본 loadedFiles 의 { path, name, type, transferSize, decodedSize })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedFile {
	pub path: String,
	pub name: String,
	pub resource_type: String,
	pub transfer_size: f64,
	pub decoded_size: f64,
}

//==============================================================================
// 리소스 사용량. (원본 { totalTransferSize, totalDecodedSize, loadedFiles })
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceUsage {
	pub total_transfer_size: f64,
	pub total_decoded_size: f64,
	pub loaded_files: Vec<LoadedFile>,
}

//==============================================================================
// 리소스 사용량 읽기. (웹: performance.getEntriesByType("resource"))
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_resource_usage() -> ResourceUsage {
	use wasm_bindgen::{JsCast, JsValue};
	let mut resource_usage = ResourceUsage::default();
	let performance = match web_sys::window().and_then(|window| window.performance()) {
		Some(performance) => {
			performance
		}
		None => {
			return resource_usage;
		}
	};
	let get_entries_function: js_sys::Function = match js_sys::Reflect::get(&performance, &JsValue::from_str("getEntriesByType")).ok().and_then(|value| value.dyn_into().ok()) {
		Some(get_entries_function) => {
			get_entries_function
		}
		None => {
			return resource_usage;
		}
	};
	let entry_list: js_sys::Array = match get_entries_function.call1(&performance, &JsValue::from_str("resource")).ok().and_then(|value| value.dyn_into().ok()) {
		Some(entry_list) => {
			entry_list
		}
		None => {
			return resource_usage;
		}
	};
	let read_number = |entry: &JsValue, name: &str| -> f64 {
		js_sys::Reflect::get(entry, &JsValue::from_str(name)).ok().and_then(|value| value.as_f64()).unwrap_or(0.0)
	};
	let read_text = |entry: &JsValue, name: &str| -> String {
		js_sys::Reflect::get(entry, &JsValue::from_str(name)).ok().and_then(|value| value.as_string()).unwrap_or_default()
	};
	for entry in entry_list.iter() {
		let path = read_text(&entry, "name");
		let transfer_size = read_number(&entry, "transferSize");
		let decoded_size = read_number(&entry, "decodedBodySize");
		resource_usage.total_transfer_size += transfer_size;
		resource_usage.total_decoded_size += decoded_size;
		resource_usage.loaded_files.push(LoadedFile {
			name: path.rsplit('/').next().unwrap_or_default().to_string(),
			path,
			resource_type: read_text(&entry, "initiatorType"),
			transfer_size,
			decoded_size,
		});
	}
	resource_usage
}

//==============================================================================
// 리소스 사용량 읽기. (데스크탑: 네트워크로 받은 것이 없어 비어 있다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_resource_usage() -> ResourceUsage {
	ResourceUsage::default()
}

//==============================================================================
// 동시에 닿을 수 있는 손가락 수. (웹은 navigator.maxTouchPoints, 없으면 0)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_max_touch_points() -> i32 {
	web_sys::window().map(|window| window.navigator().max_touch_points()).unwrap_or(0)
}

//==============================================================================
// 동시에 닿을 수 있는 손가락 수. (데스크탑은 navigator 가 없어 0)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_max_touch_points() -> i32 {
	0
}

//==============================================================================
// 빌드 대상 OS 로 플랫폼 고르기. (데스크탑, 나중의 모바일 빌드)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn detect_native_platform_type() -> PlatformType {
	if cfg!(target_os = "windows") {
		PlatformType::Windows
	}
	else if cfg!(target_os = "macos") {
		PlatformType::MacOs
	}
	else if cfg!(target_os = "ios") {
		PlatformType::IOs
	}
	else if cfg!(target_os = "android") {
		PlatformType::Android
	}
	else if cfg!(target_os = "linux") {
		PlatformType::Linux
	}
	else {
		PlatformType::Unknown
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 원본 정규식과 같은 결과인지. (대표 user_agent 몇 개)
	//==============================================================================
	#[test]
	fn detect_from_user_agent() {
		let mut platform = Platform::new();
		platform.apply_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0");
		assert_eq!(platform.get_platform_name(), PlatformType::Windows);
		assert_eq!(platform.get_browser_name(), BrowserType::Edge);
		assert!(!platform.is_mobile());

		platform.apply_user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1");
		assert_eq!(platform.get_platform_name(), PlatformType::IOs);
		assert_eq!(platform.get_browser_name(), BrowserType::Safari);
		assert!(platform.is_mobile());

		platform.apply_user_agent("Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Mobile Safari/537.36");
		assert_eq!(platform.get_platform_name(), PlatformType::Android);
		assert_eq!(platform.get_browser_name(), BrowserType::Chrome);

		platform.apply_user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0");
		assert_eq!(platform.get_platform_name(), PlatformType::Linux);
		assert_eq!(platform.get_browser_name(), BrowserType::Firefox);
	}

	//==============================================================================
	// 데스크탑 웹 판정. (모바일은 아니다, 맥이라고 알리는데 손가락 입력이 여럿이면 아이패드다)
	//==============================================================================
	#[test]
	fn desktop_web_from_user_agent_and_touch_points() {
		let mut platform = Platform::new();
		platform.apply_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36");
		assert!(platform.is_desktop_web_with_touch_points(0));
		assert!(platform.is_desktop_web_with_touch_points(10));

		platform.apply_user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15");
		assert!(platform.is_desktop_web_with_touch_points(0));
		assert!(platform.is_desktop_web_with_touch_points(1));
		assert!(!platform.is_desktop_web_with_touch_points(5));

		platform.apply_user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1");
		assert!(!platform.is_desktop_web_with_touch_points(0));

		platform.apply_user_agent("Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Mobile Safari/537.36");
		assert!(!platform.is_desktop_web_with_touch_points(5));
	}

	//==============================================================================
	// 세이프 에어리어 사각형은 받은 크기에서 인셋을 뺀 것이다. (데스크탑 인셋 0) 데스크탑 리소스 사용량은 비어 있다.
	//==============================================================================
	#[test]
	fn safe_area_rect_and_resource_usage() {
		let platform = Platform::new();
		assert_eq!(platform.get_safe_area_rect(Vector2::create(800.0, 600.0)), Rect::create(0.0, 0.0, 800.0, 600.0));
		assert!(!platform.is_coarse_pointer());
		assert_eq!(platform.get_resouce_usage(), ResourceUsage::default());
	}
}
