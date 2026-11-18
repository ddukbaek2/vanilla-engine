//==============================================================================
// 플랫폼 정보. (원본 base/platform.js)
// - 원본은 navigator.userAgent 를 보고 플랫폼과 브라우저를 고른다.
// - 러스트 판은 웹(wasm32)에서는 원본과 같은 규칙으로 userAgent 를 보고, 데스크탑에서는 빌드 대상 OS 를 쓴다.
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
	windows,
	macOS,
	android,
	steamDeck,
	iPadOS,
	iOS,
	linux,
	unknown,
}

impl PlatformType {
	//==============================================================================
	// 원본과 같은 이름 글. ("Windows", "macOS" ...)
	//==============================================================================
	pub fn getName(&self) -> &'static str {
		match self {
			PlatformType::windows => {
				"Windows"
			}
			PlatformType::macOS => {
				"macOS"
			}
			PlatformType::android => {
				"Android"
			}
			PlatformType::steamDeck => {
				"SteamDeck"
			}
			PlatformType::iPadOS => {
				"iPadOS"
			}
			PlatformType::iOS => {
				"iOS"
			}
			PlatformType::linux => {
				"Linux"
			}
			PlatformType::unknown => {
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
	chrome,
	edge,
	firefox,
	internetExplorer,
	safari,
	unknown,
}

impl BrowserType {
	//==============================================================================
	// 원본과 같은 이름 글. ("Chrome", "Edge" ...)
	//==============================================================================
	pub fn getName(&self) -> &'static str {
		match self {
			BrowserType::chrome => {
				"Chrome"
			}
			BrowserType::edge => {
				"Edge"
			}
			BrowserType::firefox => {
				"Firefox"
			}
			BrowserType::internetExplorer => {
				"Internet Explorer"
			}
			BrowserType::safari => {
				"Safari"
			}
			BrowserType::unknown => {
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
	platformName: PlatformType,
	browserName: BrowserType,
	isMobile: bool,
}

impl Platform {
	//==============================================================================
	// 생성. (탐지까지 한다. 원본은 생성 뒤 getPlatformInfo 를 부른다)
	//==============================================================================
	pub fn new() -> Platform {
		let mut platform = Platform {
			platformName: PlatformType::unknown,
			browserName: BrowserType::unknown,
			isMobile: false,
		};
		platform.getPlatformInfo();
		platform
	}

	//==============================================================================
	// 플랫폼 정보 탐지 및 적용. (플랫폼, 브라우저)
	//==============================================================================
	pub fn getPlatformInfo(&mut self) -> (PlatformType, BrowserType) {
		#[cfg(target_arch = "wasm32")]
		{
			let userAgent = web_sys::window().and_then(|window| window.navigator().user_agent().ok()).unwrap_or_default();
			self.applyUserAgent(&userAgent);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.platformName = detectNativePlatformType();
			self.browserName = BrowserType::unknown;
			self.isMobile = matches!(self.platformName, PlatformType::android | PlatformType::iOS | PlatformType::iPadOS);
		}
		(self.platformName, self.browserName)
	}

	//==============================================================================
	// userAgent 로 플랫폼과 브라우저 고르기. (원본 getPlatformInfo 의 정규식과 같은 순서, 대소문자 무시)
	//==============================================================================
	pub fn applyUserAgent(&mut self, userAgent: &str) {
		let lowerUserAgent = userAgent.to_lowercase();
		let containsAny = |wordList: &[&str]| wordList.iter().any(|word| lowerUserAgent.contains(word));
		self.isMobile = containsAny(&["mobi", "android", "iphone", "ipad"]);

		// 플랫폼 감지.
		self.platformName = if containsAny(&["windows"]) {
			PlatformType::windows
		}
		else if containsAny(&["valve steam gameoverlay"]) {
			PlatformType::steamDeck
		}
		else if containsAny(&["ipad"]) {
			PlatformType::iPadOS
		}
		else if containsAny(&["iphone", "ipod"]) {
			PlatformType::iOS
		}
		else if containsAny(&["macintosh", "mac os x"]) {
			PlatformType::macOS
		}
		else if containsAny(&["android"]) {
			PlatformType::android
		}
		else if containsAny(&["linux"]) {
			PlatformType::linux
		}
		else {
			PlatformType::unknown
		};

		// 브라우저 감지.
		self.browserName = if containsAny(&["edg"]) {
			BrowserType::edge
		}
		else if containsAny(&["chrome"]) {
			BrowserType::chrome
		}
		else if containsAny(&["safari"]) {
			BrowserType::safari
		}
		else if containsAny(&["firefox"]) {
			BrowserType::firefox
		}
		else if containsAny(&["msie", "trident"]) {
			BrowserType::internetExplorer
		}
		else {
			BrowserType::unknown
		};
	}

	//==============================================================================
	// 플랫폼.
	//==============================================================================
	pub fn getPlatformName(&self) -> PlatformType {
		self.platformName
	}

	//==============================================================================
	// 브라우저.
	//==============================================================================
	pub fn getBrowserName(&self) -> BrowserType {
		self.browserName
	}

	//==============================================================================
	// 모바일인지.
	//==============================================================================
	pub fn isMobile(&self) -> bool {
		self.isMobile
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
fn detectNativePlatformType() -> PlatformType {
	if cfg!(target_os = "windows") {
		PlatformType::windows
	}
	else if cfg!(target_os = "macos") {
		PlatformType::macOS
	}
	else if cfg!(target_os = "ios") {
		PlatformType::iOS
	}
	else if cfg!(target_os = "android") {
		PlatformType::android
	}
	else if cfg!(target_os = "linux") {
		PlatformType::linux
	}
	else {
		PlatformType::unknown
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 원본 정규식과 같은 결과인지. (대표 userAgent 몇 개)
	//==============================================================================
	#[test]
	fn detectFromUserAgent() {
		let mut platform = Platform::new();
		platform.applyUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0");
		assert_eq!(platform.getPlatformName(), PlatformType::windows);
		assert_eq!(platform.getBrowserName(), BrowserType::edge);
		assert!(!platform.isMobile());

		platform.applyUserAgent("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1");
		assert_eq!(platform.getPlatformName(), PlatformType::iOS);
		assert_eq!(platform.getBrowserName(), BrowserType::safari);
		assert!(platform.isMobile());

		platform.applyUserAgent("Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Mobile Safari/537.36");
		assert_eq!(platform.getPlatformName(), PlatformType::android);
		assert_eq!(platform.getBrowserName(), BrowserType::chrome);

		platform.applyUserAgent("Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0");
		assert_eq!(platform.getPlatformName(), PlatformType::linux);
		assert_eq!(platform.getBrowserName(), BrowserType::firefox);
	}
}
