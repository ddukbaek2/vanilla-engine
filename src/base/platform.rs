//==============================================================================
// 플랫폼 정보. (원본 base/platform.js)
// - 원본은 navigator.user_agent 를 보고 플랫폼과 브라우저를 고른다.
// - 러스트 판은 웹(wasm32)에서는 원본과 같은 규칙으로 user_agent 를 보고, 데스크탑에서는 빌드 대상 OS 를 쓴다.
//   데스크탑의 브라우저는 unknown 이고, 스팀덱 구분은 아직 하지 않는다(리눅스로 나온다).
// - 원본의 getOrAddCanvas 는 platform/web.rs 가, getResouceUsage(웹 리소스 전송량)는 옮기지 않았다.
//==============================================================================

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
}

impl Default for Platform {
	fn default() -> Platform {
		Platform::new()
	}
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
}
