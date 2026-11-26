//==============================================================================
//! 플랫폼 정보. (원본 base/platform.js)
//! - 원본은 navigator.user_agent 를 보고 플랫폼과 브라우저를 고른다.
//! - 러스트 판은 웹(wasm32)에서는 원본과 같은 규칙으로 user_agent 를 보고, 데스크탑에서는 빌드 대상 OS 를 쓴다.
//!   데스크탑의 브라우저는 Chrome 이다. 데스크탑 판은 크롬 위에서 도는 원본과 같은 결과를 내도록 맞췄고(글꼴과 글자 너비, 주된 손 판정, 페이지 이벤트,
//!   세이프 에어리어, 화면 키보드), 원본은 이 이름을 개발자 도구와 통계 글에 보이기만 하므로, 게임 코드의 브라우저 분기도 크롬 갈래로 가야 같은 결과다.
//!   리눅스는 스팀덱이면 SteamDeck 이다. 스팀이 스팀덱에서 띄울 때 넣는 환경 변수 SteamDeck=1
//!   (밸브 Proton 실행 스크립트가 이것으로 스팀덱을 가린다), 또는 밸브 SteamOS 의 jupiter-hw-support 처럼
//!   /sys/class/dmi/id/board_vendor 가 Valve 이고 board_name 이 Jupiter(LCD) 나 Galileo(OLED) 이면 스팀덱으로 본다.
//! - 원본의 getOrAddCanvas 는 platform/web.rs 가 엔진 설정의 캔버스 id 로 한다.
//! - get_resource_usage: 웹은 원본처럼 performance 의 resource 항목을 더한다. 데스크탑은 네트워크로 받은 것이 없어,
//!   엔진이 애셋으로 읽은 파일(애셋 바이트 읽기, Engine::load_image, 오디오 파일)을 읽은 차례로 든다. 종류는 "file", 전송량과 푼 크기는 둘 다 파일 바이트 수다.
//!   (기본 글꼴과 대체 글꼴처럼 시스템에서 읽는 글꼴은 원본의 시스템 글꼴처럼 들지 않는다)
//!   옛 이름의 오타 get_resouce_usage 는 같은 함수를 부르는 별칭으로 남긴다. (원본도 같다)
//! - get_safe_area_rect: 원본은 캔버스(없으면 창)의 크기를 읽는다. 러스트 판은 문서 객체가 없어 그 크기를 받는다. 데스크탑의 인셋은 크롬 데스크탑처럼 0 이다.
//! - is_coarse_pointer: 원본처럼 CSS pointer: coarse. 데스크탑은 크로미엄의 판정을 옮겼다. (윈도우는 태블릿 모드나 마우스 없는 터치 기기, 리눅스는 마우스와 터치패드 없이
//!   터치스크린만 있을 때 참, 맥은 늘 거짓. platform/desktop.rs)
//! - is_desktop_web: 원본은 부를 때마다 플랫폼 정보를 새로 읽는다. 러스트 판은 만들 때 읽은 것을 쓴다. (user_agent 는 바뀌지 않는다)
//!   데스크탑 빌드는 모바일이 아니고 손가락 입력 수(navigator.maxTouchPoints)가 없어 늘 true 다.
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;

/// 공통 시스템 폰트 목록. (원본 SYSTEM_FONT_STRING, 웹 CSS 글꼴 목록)
pub const SYSTEM_FONT_STRING: &str = "-apple-system, \"Segoe UI\", Roboto, \"Apple SD Gothic Neo\", \"Malgun Gothic\", sans-serif";

//==============================================================================
/// 플랫폼 타입.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformType {
	/// 윈도우.
	Windows,
	/// 맥OS. (웹은 user_agent 의 macintosh 나 mac os x)
	MacOs,
	/// 안드로이드.
	Android,
	/// 스팀덱. (웹은 user_agent 의 valve steam gameoverlay, 데스크탑은 리눅스 가운데 스팀덱으로 가린 것)
	SteamDeck,
	/// 아이패드OS. (웹은 user_agent 의 ipad)
	IPadOs,
	/// iOS. (웹은 user_agent 의 iphone 이나 ipod)
	IOs,
	/// 리눅스. (스팀덱이 아닌 것)
	Linux,
	/// 위 어느 것도 아닌 것.
	Unknown,
}

impl PlatformType {
	//==============================================================================
	/// 원본과 같은 이름 글. ("Windows", "macOS" ...)
	///
	/// # 돌려줌
	/// 이름 글. ("Windows", "macOS", "Android", "SteamDeck", "iPadOS", "iOS", "Linux", "Unknown")
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
/// 브라우저 타입.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserType {
	/// 크롬. (데스크탑 빌드는 늘 이것)
	Chrome,
	/// 엣지. (웹은 user_agent 의 edg, 크롬보다 먼저 본다)
	Edge,
	/// 파이어폭스.
	Firefox,
	/// 인터넷 익스플로러. (웹은 user_agent 의 msie 나 trident)
	InternetExplorer,
	/// 사파리. (웹은 user_agent 에 edg 와 chrome 이 없고 safari 가 든 것)
	Safari,
	/// 위 어느 것도 아닌 것.
	Unknown,
}

impl BrowserType {
	//==============================================================================
	/// 원본과 같은 이름 글. ("Chrome", "Edge" ...)
	///
	/// # 돌려줌
	/// 이름 글. ("Chrome", "Edge", "Firefox", "Internet Explorer", "Safari", "Unknown")
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
/// 플랫폼 정보.
///
/// # 예제
/// user_agent 로 플랫폼과 브라우저 고르기
/// ```rust
/// use vanilla::{BrowserType, Platform, PlatformType};
///
/// let mut platform = Platform::new();
/// platform.apply_user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1");
/// assert_eq!(platform.get_platform_name(), PlatformType::IOs);
/// assert_eq!(platform.get_platform_name().get_name(), "iOS");
/// assert_eq!(platform.get_browser_name(), BrowserType::Safari);
/// assert!(platform.is_mobile());
/// assert!(!platform.is_desktop_web());
/// ```
///
/// 엔진의 플랫폼 정보로 단축키 안내를 보일지 가리기
/// ```rust
/// use vanilla::Engine;
///
/// fn is_shortcut_hint_visible(engine: &Engine) -> bool {
/// 	let platform = engine.get_platform();
/// 	platform.is_desktop_web() && !platform.is_coarse_pointer()
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Platform {
	platform_name: PlatformType,
	browser_name: BrowserType,
	is_mobile: bool,
}

impl Platform {
	//==============================================================================
	/// 생성. (탐지까지 한다. 원본은 생성 뒤 get_platform_info 를 부른다)
	///
	/// # 돌려줌
	/// 탐지를 마친 플랫폼 정보. (웹은 navigator 의 user_agent, 데스크탑은 빌드 대상 OS 로 고른 것)
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
	/// 플랫폼 정보 탐지 및 적용. (플랫폼, 브라우저)
	///
	/// # 돌려줌
	/// 다시 탐지해 넣은 (플랫폼, 브라우저). (모바일 여부도 다시 넣는다)
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
			self.browser_name = BrowserType::Chrome;
			self.is_mobile = matches!(self.platform_name, PlatformType::Android | PlatformType::IOs | PlatformType::IPadOs);
		}
		(self.platform_name, self.browser_name)
	}

	//==============================================================================
	/// user_agent 로 플랫폼과 브라우저 고르기. (원본 get_platform_info 의 정규식과 같은 순서, 대소문자 무시)
	/// - 모바일 여부도 같이 넣는다. (mobi, android, iphone, ipad 가 들면 모바일)
	///
	/// # 인자
	/// - `user_agent`: 브라우저의 user_agent 글.
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
	/// 플랫폼.
	///
	/// # 돌려줌
	/// 탐지한 플랫폼.
	//==============================================================================
	pub fn get_platform_name(&self) -> PlatformType {
		self.platform_name
	}

	//==============================================================================
	/// 브라우저.
	///
	/// # 돌려줌
	/// 탐지한 브라우저. (데스크탑 빌드는 Chrome)
	//==============================================================================
	pub fn get_browser_name(&self) -> BrowserType {
		self.browser_name
	}

	//==============================================================================
	/// 모바일인지.
	///
	/// # 돌려줌
	/// 모바일이면 true. (웹은 user_agent 로, 데스크탑 빌드는 안드로이드, iOS, 아이패드OS 대상일 때)
	//==============================================================================
	pub fn is_mobile(&self) -> bool {
		self.is_mobile
	}

	//==============================================================================
	/// 데스크탑 웹인지. (마우스 휠, 키보드 안내처럼 데스크탑에서만 보일 것을 가릴 때 쓴다)
	/// - 모바일이면 아니다. 아이패드의 사파리는 데스크탑처럼 맥이라고 알리므로, 맥인데 손가락 입력이 여럿 되면 아이패드로 본다.
	///
	/// # 돌려줌
	/// 데스크탑 웹이면 true. (모바일이거나, 맥인데 navigator.maxTouchPoints 가 2 이상이면 false, 데스크탑 빌드는 늘 true)
	//==============================================================================
	pub fn is_desktop_web(&self) -> bool {
		self.is_desktop_web_with_touch_points(read_max_touch_points())
	}

	//==============================================================================
	/// 주된 손이 손가락인지. (CSS 의 pointer: coarse, 단축키 안내처럼 마우스로 하는 화면에만 보일 것을 가릴 때 쓴다)
	///
	/// # 돌려줌
	/// 주된 손이 손가락이면 true. (부를 때마다 새로 읽는다, 맥 데스크탑은 늘 false)
	//==============================================================================
	pub fn is_coarse_pointer(&self) -> bool {
		crate::platform::is_coarse_pointer()
	}

	//==============================================================================
	/// 리소스 사용량. (웹이 받은 파일마다 전송량과 푼 크기)
	///
	/// # 돌려줌
	/// 지금까지 받은 파일 목록과 그 합. (부를 때마다 새로 모은다, 데스크탑은 엔진이 애셋으로 읽은 파일)
	//==============================================================================
	pub fn get_resource_usage(&self) -> ResourceUsage {
		read_resource_usage()
	}

	//==============================================================================
	/// 리소스 사용량. (옛 이름의 오타를 그대로 둔 별칭, get_resource_usage 와 같다)
	///
	/// # 돌려줌
	/// get_resource_usage 와 같은 값.
	//==============================================================================
	pub fn get_resouce_usage(&self) -> ResourceUsage {
		self.get_resource_usage()
	}

	//==============================================================================
	/// 세이프 에어리어. (area_size 크기에서 env(safe-area-inset-*) 를 뺀 사각형, 인셋은 원본 parseInt 처럼 정수로 자른다)
	/// - 인셋이 CSS 픽셀이므로 area_size 도 CSS 픽셀 크기로 준다. (원본처럼 캔버스의 CSS 크기 clientWidth, clientHeight 이고 그리기 버퍼 크기가 아니다) 너비와 높이는 0 이상이다.
	///
	/// # 인자
	/// - `area_size`: 인셋을 뺄 바탕 크기. (픽셀, 원본의 캔버스 크기 자리)
	///
	/// # 돌려줌
	/// (왼쪽 인셋, 위 인셋) 에서 시작하고, 너비는 좌우 인셋을, 높이는 위아래 인셋을 뺀 사각형.
	///
	/// # 예제
	/// 데스크탑은 인셋이 0 이라 받은 크기 그대로다
	/// ```rust
	/// use vanilla::{Platform, Rect, Vector2};
	///
	/// let platform = Platform::new();
	/// let safe_area_rect = platform.get_safe_area_rect(Vector2::create(800.0, 600.0));
	/// assert_eq!(safe_area_rect, Rect::create(0.0, 0.0, 800.0, 600.0));
	/// ```
	//==============================================================================
	pub fn get_safe_area_rect(&self, area_size: Vector2) -> Rect {
		let insets = crate::platform::read_safe_area_insets();
		let top = insets[0].trunc();
		let right = insets[1].trunc();
		let bottom = insets[2].trunc();
		let left = insets[3].trunc();
		Rect::create(left, top, (area_size.x - left - right).max(0.0), (area_size.y - top - bottom).max(0.0))
	}

	//==============================================================================
	/// 손가락 입력 수를 받아 데스크탑 웹인지. (is_desktop_web 의 몸통)
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
	//==============================================================================
	/// 탐지까지 마친 플랫폼 정보. (Platform::new 와 같다)
	//==============================================================================
	fn default() -> Platform {
		Platform::new()
	}
}

//==============================================================================
/// 받은 파일 하나. (원본 loadedFiles 의 { path, name, type, transferSize, decodedSize })
///
/// # 예제
/// 가장 큰 파일 찾기
/// ```rust
/// use vanilla::base::platform::LoadedFile;
/// use vanilla::Platform;
///
/// let platform = Platform::new();
/// let resource_usage = platform.get_resource_usage();
/// let largest_file: Option<&LoadedFile> = resource_usage.loaded_files.iter().max_by(|first, second| first.decoded_size.total_cmp(&second.decoded_size));
/// if let Some(loaded_file) = largest_file {
/// 	println!("{} {}", loaded_file.path, loaded_file.decoded_size);
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedFile {
	/// 경로. (웹은 performance 항목의 name 인 주소, 데스크탑은 엔진이 읽은 애셋 경로)
	pub path: String,
	/// 경로의 마지막 조각인 파일 이름. (웹은 / 로, 데스크탑은 / 와 \ 로 가른다)
	pub name: String,
	/// 종류. (웹은 initiatorType, 데스크탑은 "file")
	pub resource_type: String,
	/// 전송량. (바이트, 웹은 transferSize, 데스크탑은 파일 바이트 수)
	pub transfer_size: f64,
	/// 푼 크기. (바이트, 웹은 decodedBodySize, 데스크탑은 파일 바이트 수)
	pub decoded_size: f64,
}

//==============================================================================
/// 리소스 사용량. (원본 { totalTransferSize, totalDecodedSize, loadedFiles })
///
/// # 예제
/// 합은 파일마다의 값을 더한 것이다
/// ```rust
/// use vanilla::Platform;
///
/// let platform = Platform::new();
/// let resource_usage = platform.get_resource_usage();
/// let total_transfer_size: f64 = resource_usage.loaded_files.iter().map(|loaded_file| loaded_file.transfer_size).sum();
/// assert_eq!(total_transfer_size, resource_usage.total_transfer_size);
/// println!("{} {}", resource_usage.loaded_files.len(), resource_usage.total_decoded_size);
/// ```
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceUsage {
	/// 전송량 합. (바이트)
	pub total_transfer_size: f64,
	/// 푼 크기 합. (바이트)
	pub total_decoded_size: f64,
	/// 받은 파일 목록. (데스크탑은 읽은 차례)
	pub loaded_files: Vec<LoadedFile>,
}

//==============================================================================
/// 리소스 사용량 읽기. (웹: performance.getEntriesByType("resource"))
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

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
	/// 엔진이 애셋으로 읽은 파일. (데스크탑, 읽은 차례)
	static LOADED_FILE_LIST: std::cell::RefCell<Vec<LoadedFile>> = const { std::cell::RefCell::new(Vec::new()) };
}

//==============================================================================
/// 애셋으로 읽은 파일 적기. (데스크탑, 원본 performance 의 resource 항목 자리. 엔진이 파일을 읽은 뒤 부른다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn record_loaded_file(path: &str, byte_count: u64) {
	let name = path.rsplit(['/', '\\']).next().unwrap_or_default().to_string();
	LOADED_FILE_LIST.with(|loaded_file_list| {
		loaded_file_list.borrow_mut().push(LoadedFile {
			path: String::from(path),
			name,
			resource_type: String::from("file"),
			transfer_size: byte_count as f64,
			decoded_size: byte_count as f64,
		});
	});
}

//==============================================================================
/// 리소스 사용량 읽기. (데스크탑: 엔진이 애셋으로 읽은 파일)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_resource_usage() -> ResourceUsage {
	let mut resource_usage = ResourceUsage::default();
	LOADED_FILE_LIST.with(|loaded_file_list| {
		for loaded_file in loaded_file_list.borrow().iter() {
			resource_usage.total_transfer_size += loaded_file.transfer_size;
			resource_usage.total_decoded_size += loaded_file.decoded_size;
			resource_usage.loaded_files.push(loaded_file.clone());
		}
	});
	resource_usage
}

//==============================================================================
/// 동시에 닿을 수 있는 손가락 수. (웹은 navigator.maxTouchPoints, 없으면 0)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_max_touch_points() -> i32 {
	web_sys::window().map(|window| window.navigator().max_touch_points()).unwrap_or(0)
}

//==============================================================================
/// 동시에 닿을 수 있는 손가락 수. (데스크탑은 navigator 가 없어 0)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_max_touch_points() -> i32 {
	0
}

//==============================================================================
/// 빌드 대상 OS 로 플랫폼 고르기. (데스크탑, 나중의 모바일 빌드)
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
		if is_steam_deck() {
			PlatformType::SteamDeck
		}
		else {
			PlatformType::Linux
		}
	}
	else {
		PlatformType::Unknown
	}
}

//==============================================================================
/// 스팀덱인지. (환경 변수 SteamDeck=1, 또는 DMI 의 보드 제조사 Valve 와 보드 이름 Jupiter, Galileo)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn is_steam_deck() -> bool {
	let steam_deck_variable = std::env::var("SteamDeck").unwrap_or_default();
	let read_dmi_text = |dmi_path: &str| -> String {
		std::fs::read_to_string(dmi_path).map(|dmi_text| dmi_text.trim().to_string()).unwrap_or_default()
	};
	let board_vendor = read_dmi_text("/sys/class/dmi/id/board_vendor");
	let board_name = read_dmi_text("/sys/class/dmi/id/board_name");
	is_steam_deck_from_values(&steam_deck_variable, &board_vendor, &board_name)
}

//==============================================================================
/// 환경 변수 SteamDeck 값과 DMI 의 보드 제조사, 보드 이름으로 스팀덱인지. (is_steam_deck 의 몸통)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn is_steam_deck_from_values(steam_deck_variable: &str, board_vendor: &str, board_name: &str) -> bool {
	if steam_deck_variable == "1" {
		return true;
	}
	board_vendor == "Valve" && (board_name == "Jupiter" || board_name == "Galileo")
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 원본 정규식과 같은 결과인지. (대표 user_agent 몇 개)
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
	/// 데스크탑 웹 판정. (모바일은 아니다, 맥이라고 알리는데 손가락 입력이 여럿이면 아이패드다)
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
	/// 세이프 에어리어 사각형은 받은 크기에서 인셋을 뺀 것이다. (데스크탑 인셋 0) 데스크탑 리소스 사용량은 엔진이 읽은 파일이다.
	/// 데스크탑의 브라우저는 크롬 위의 원본과 같은 갈래로 가게 Chrome 이다.
	//==============================================================================
	#[test]
	fn safe_area_rect_and_resource_usage() {
		let platform = Platform::new();
		assert_eq!(platform.get_browser_name(), BrowserType::Chrome);
		assert_eq!(platform.get_safe_area_rect(Vector2::create(800.0, 600.0)), Rect::create(0.0, 0.0, 800.0, 600.0));
		assert_eq!(platform.get_safe_area_rect(Vector2::create(-4.0, -2.0)), Rect::create(0.0, 0.0, 0.0, 0.0));
		assert!(!platform.is_coarse_pointer());
		assert_eq!(platform.get_resource_usage(), ResourceUsage::default());
		record_loaded_file("assets/images/logo.png", 1200);
		record_loaded_file("assets\\sounds\\hit.ogg", 300);
		let resource_usage = platform.get_resource_usage();
		assert_eq!(resource_usage.total_transfer_size, 1500.0);
		assert_eq!(resource_usage.total_decoded_size, 1500.0);
		assert_eq!(resource_usage.loaded_files[0].name, "logo.png");
		assert_eq!(resource_usage.loaded_files[1].name, "hit.ogg");
		assert_eq!(resource_usage.loaded_files[1].resource_type, "file");
		assert_eq!(platform.get_resouce_usage(), resource_usage);
	}

	//==============================================================================
	/// 스팀덱 판별: SteamDeck=1, 또는 보드 제조사 Valve 와 보드 이름 Jupiter, Galileo.
	//==============================================================================
	#[test]
	fn steam_deck_from_environment_and_board() {
		assert!(is_steam_deck_from_values("1", "", ""));
		assert!(is_steam_deck_from_values("", "Valve", "Jupiter"));
		assert!(is_steam_deck_from_values("", "Valve", "Galileo"));
		assert!(!is_steam_deck_from_values("0", "Valve", "Other"));
		assert!(!is_steam_deck_from_values("", "ASUSTeK COMPUTER INC.", "Jupiter"));
	}
}
