//==============================================================================
//! 데스크탑 시스템 글꼴 찾기. (글꼴 이름의 파일, 글꼴에 없는 글자를 그릴 대체 글꼴)
//! - 원본에 없는 파일이다. 원본은 브라우저가 CSS 글꼴 이름을 운영체제 글꼴로 찾고, 글꼴 목록에 없는 글자는 운영체제의 대체 글꼴로 그린다.
//!   크롬은 맥에서 CoreText, 윈도우에서 DirectWrite, 리눅스에서 fontconfig 로 찾는다. 러스트 판 데스크탑도 같은 API 로 파일을 찾아 글자 캐시에 올린다.
//! - find_family_font_file: 글꼴 이름, 굵기, 기울기로 파일과 얼굴을 찾는다. 그 이름의 글꼴이 없으면 None 이다. (CSS 처럼 다른 글꼴로 바꿔 주지 않는다)
//!   - 맥: system-ui, BlinkMacSystemFont 는 시스템 UI 글꼴(CTFontCreateUIFontForLanguage, 크롬 font_matcher_mac.mm 의 MatchSystemUIFont),
//!     그 밖은 이름과 굵기 특성으로 지은 글꼴 서술자. -apple-system 과 ui-serif, ui-sans-serif, ui-monospace, ui-rounded 는 크롬이 모르는 이름이라 None 이다.
//!     (사파리만 안다. 맥 크롬 158 의 Canvas2D 에서 "16px -apple-system" 은 기본 글꼴로 그려진다)
//!   - 윈도우: system-ui 는 크롬처럼 메뉴 글꼴(NONCLIENTMETRICS 의 lfMenuFont, 읽지 못하면 Segoe UI), 그 밖은 시스템 글꼴 모음의 이름(FindFamilyName)과 GetFirstMatchingFont.
//!   - 리눅스: fontconfig 의 FcFontMatch. 크롬(Skia SkFontConfigInterface_direct.cpp 의 isAcceptableMatch)처럼 찾은 글꼴의 이름이 청한 이름이나 설정을 거친 이름과 같거나,
//!     치수가 같은 대체 글꼴(Arial 과 Liberation Sans 같은 표)이거나, 청한 이름이 sans, serif, monospace 일 때만 받는다.
//! - 일반 이름(serif, sans-serif, monospace, cursive, fantasy, math, -webkit-body 는 표준 글꼴)은 크롬의 기본 글꼴 표대로 글꼴 이름으로 바꾼 뒤 찾는다. (resolve_generic_family_name)
//!   큰따옴표로 감싼 이름("\"monospace\"")은 크롬처럼 바꾸지 않고 그 글자의 글꼴을 찾는다. (맥과 윈도우에는 그 이름의 글꼴이 없고, 리눅스 fontconfig 는 monospace 를 받는다)
//!   표는 크로미엄 chrome/app/resources/`locale_settings_<운영체제>`.grd 의 IDS_*_FONT_FAMILY 와 그 언어 번역(platform_locale_settings/*.xtb)이다.
//!   크롬은 브라우저 언어(화면 언어)로 Common(Zyyy) 스크립트 값을 정하고, 문서 lang 이 없는 캔버스는 기본 로캘(브라우저 언어)의 스크립트로 글꼴을 고른다.
//!   그 스크립트의 따로 정한 값은 브라우저 언어의 스크립트라 등록하지 않으므로(prefs_tab_helper.cc 의 RegisterProfilePrefs) 결국 Common 값을 쓴다.
//!   원본이 글을 굽는 캔버스는 문서에 붙이지 않아 문서의 lang 을 바꿔도 그대로다. (맥 크롬 158 로 확인)
//!   러스트 판은 시스템 선호 언어에서 크롬이 고르는 화면 언어를 찾아(read_browser_locale) 같은 표를 쓴다.
//!   예: 맥 한국어는 serif AppleMyungjo, sans-serif Apple SD Gothic Neo, monospace Menlo, 영어는 Times, Helvetica, Menlo. 윈도우 한국어는 serif Batang, sans-serif Malgun Gothic.
//! - find_fallback_font_file: 글(글자 묶음 하나)을 그릴 대체 글꼴을 운영체제가 고르게 한다.
//!   - 맥: 시스템 UI 글꼴에서 CTFontCreateForString. (크롬 맥의 글꼴 대체와 같은 API) 색 이모지를 바라면 Apple Color Emoji 를 먼저 본다.
//!   - 윈도우: IDWriteFontFallback.MapCharacters (기본 이름 Segoe UI, 사용자 언어)
//!   - 리눅스: 글자를 담은 charset 으로 FcFontSort 해 그 글자를 가진 첫 글꼴. 색 이모지를 바라면 color 속성을 건다.
//! - 돌려주는 FontFileSource 의 얼굴 번호는 운영체제가 알려 주면(윈도우, 리눅스) 넣고, 맥은 PostScript 이름을 넣어 글꼴을 읽을 때 맞는 얼굴을 찾는다.
//!   가변 글꼴(맥 시스템 UI 글꼴 SFNS 같은)의 굵기는 글자 캐시가 축 값으로 만든다.
//!
//! # 예제
//! 일반 이름 sans-serif 의 굵은 글꼴 파일 찾기
//! ```rust
//! use vanilla::platform::desktop::systemfont::{find_family_font_file, resolve_generic_family_name, GenericFamily};
//!
//! assert_eq!(GenericFamily::from_family_name("Sans-Serif"), Some(GenericFamily::SansSerif));
//! assert_eq!(GenericFamily::from_family_name("Helvetica"), None);
//! if let Some(family_name) = resolve_generic_family_name(GenericFamily::SansSerif) {
//! 	if let Some(source) = find_family_font_file(&family_name, 700, false) {
//! 		assert!(source.path.exists());
//! 	}
//! }
//! ```
//==============================================================================
use crate::core::textstringtexturecache::FontFileSource;

//==============================================================================
/// 이어 누르기(두 번 누르기)로 볼 시간. (초, 입력 필드의 두 번, 세 번 누르기)
/// - 맥: 크롬은 NSEvent clickCount 를 쓰고, 그 시간은 시스템 설정의 두 번 클릭 속도인 NSEvent.doubleClickInterval 이다.
/// - 윈도우, 리눅스: 크롬 aura 는 운영체제 값을 읽지 않고 ui/events/event_constants.h 의 kDoubleClickTimeMs 500 을 쓴다.
///
/// # 돌려줌
/// 이어 누르기로 볼 시간. (초, 맥은 시스템 설정 값, 읽지 못하거나 다른 운영체제면 0.5)
//==============================================================================
pub fn read_double_click_seconds() -> f32 {
	#[cfg(target_os = "macos")]
	{
		use objc2::msg_send;
		use objc2::runtime::AnyClass;
		if let Some(event_class) = AnyClass::get(c"NSEvent") {
			let double_click_interval: f64 = unsafe { msg_send![event_class, doubleClickInterval] };
			if double_click_interval > 0.0 {
				return double_click_interval as f32;
			}
		}
	}
	0.5
}

//==============================================================================
/// 시스템이 얹는 스크롤 막대를 바라는지. (맥 시스템 설정의 스크롤 막대 보기, 크롬 os_settings_provider_mac.mm 의 PrefersOverlayScrollbars)
/// - 맥: NSScroller.preferredScrollerStyle 이 NSScrollerStyleOverlay(1) 이면 참. (자동이면 마우스와 트랙패드에 따라 시스템이 정한다)
/// - 윈도우, 리눅스: 크롬은 얹는 막대 기능(kOverlayScrollbar)이 꺼져 있어 늘 거짓.
///
/// # 돌려줌
/// 얹는 스크롤 막대를 바라면 true. (윈도우, 리눅스는 늘 false)
//==============================================================================
pub fn is_overlay_scrollbar_preferred() -> bool {
	#[cfg(target_os = "macos")]
	{
		use objc2::msg_send;
		use objc2::runtime::AnyClass;
		if let Some(scroller_class) = AnyClass::get(c"NSScroller") {
			let scroller_style: isize = unsafe { msg_send![scroller_class, preferredScrollerStyle] };
			return scroller_style == 1;
		}
		false
	}
	#[cfg(not(target_os = "macos"))]
	{
		false
	}
}

//==============================================================================
/// 글꼴 이름으로 파일 찾기. (CSS 글꼴 이름 하나, 일반 이름은 크롬 기본 글꼴 표의 이름으로 바꿔 찾는다. 없으면 None)
/// - 큰따옴표로 감싼 이름은 일반 이름과 같은 글자라도 바꾸지 않고 그 이름의 글꼴을 찾는다. (크롬 FontSelector::FamilyNameFromSettings 는 따옴표 친 이름을 기본 글꼴 표로 바꾸지 않고, FontCache 가 그 글자로 찾는다)
///
/// # 인자
/// - `family_name`: 글꼴 이름 하나. (앞뒤 빈칸은 뗀다, "\"monospace\"" 처럼 큰따옴표로 감싸면 글꼴 이름)
/// - `weight`: 굵기. (CSS font-weight, 100 ~ 900, 400 이 보통)
/// - `is_italic`: 기울인 얼굴을 찾으려면 true.
///
/// # 돌려줌
/// 찾은 파일과 얼굴. 이름이 비었거나, 그 이름의 글꼴이 없거나, 일반 이름에 맞는 글꼴이 없으면 None.
///
/// # 예제
/// 이름으로 찾고 없으면 대체 글꼴로
/// ```rust
/// use vanilla::platform::{find_fallback_font_file, find_family_font_file};
///
/// let source = find_family_font_file("NoSuchFamily", 400, false).or_else(|| find_fallback_font_file("A", 400, false, false));
/// if let Some(source) = source {
/// 	assert!(source.path.exists());
/// }
/// assert!(find_family_font_file("  ", 400, false).is_none());
/// assert!(find_family_font_file("\"monospace\"", 400, false).is_none() || find_family_font_file("monospace", 400, false).is_some());
/// ```
//==============================================================================
pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
	let family_name = family_name.trim();
	if family_name.is_empty() {
		return None;
	}
	if let Some(quoted_family_name) = family_name.strip_prefix('"').and_then(|family_name| family_name.strip_suffix('"')) {
		if quoted_family_name.is_empty() {
			return None;
		}
		return platform_font::find_family_font_file(quoted_family_name, weight, is_italic);
	}
	if let Some(generic_family) = GenericFamily::from_family_name(family_name) {
		let resolved_family_name = resolve_generic_family_name(generic_family)?;
		return platform_font::find_family_font_file(&resolved_family_name, weight, is_italic);
	}
	platform_font::find_family_font_file(family_name, weight, is_italic)
}

//==============================================================================
/// 글을 그릴 대체 글꼴 파일 찾기. (운영체제가 고른다, is_color_wanted 면 색 이모지 글꼴을 먼저)
///
/// # 인자
/// - `text`: 그릴 글. (글자 묶음 하나)
/// - `weight`: 굵기. (CSS font-weight)
/// - `is_italic`: 기울인 얼굴을 바라면 true.
/// - `is_color_wanted`: 색 이모지 글꼴을 먼저 보려면 true.
///
/// # 돌려줌
/// 운영체제가 고른 파일과 얼굴. 글이 비었거나 고른 것이 없으면 None.
//==============================================================================
pub fn find_fallback_font_file(text: &str, weight: u16, is_italic: bool, is_color_wanted: bool) -> Option<FontFileSource> {
	if text.is_empty() {
		return None;
	}
	platform_font::find_fallback_font_file(text, weight, is_italic, is_color_wanted)
}

//==============================================================================
/// 마지막 대체 글꼴 파일. (크롬 FontCache::GetLastResortFallbackFont, 글꼴 이름도 돌려준다. 없으면 None)
/// - 문서에 붙이지 않은 캔버스의 폰트 문자열에서 글꼴에 대는 단위(em 밖의 ex, ch, cap, ic, lh)를 셀 때 부모 글꼴이 된다. (맥 크롬 158 에서 Times 의 치수로 확인)
/// - 맥: Times, 없으면 Lucida Grande. (font_cache_mac.mm)
/// - 윈도우, 리눅스: 이름 없음(운영체제 기본), Sans, Arial, 윈도우는 그 뒤 MS UI Gothic, Microsoft Sans Serif, Segoe UI, Calibri, Times New Roman, Courier New. (font_cache_skia.cc)
///
/// # 돌려줌
/// (찾은 파일과 얼굴, 목록의 글꼴 이름). 목록의 어느 이름도 찾지 못하면 None.
//==============================================================================
pub fn find_last_resort_font_file() -> Option<(FontFileSource, String)> {
	for family_name in platform_font::LAST_RESORT_FAMILY_LIST {
		if let Some(font_file_source) = platform_font::find_family_font_file(family_name, 400, false) {
			return Some((font_file_source, family_name.to_string()));
		}
	}
	None
}

//==============================================================================
/// 마지막 대체 글꼴의 이름. (깔린 첫 이름, 이름 없음(운영체제 기본)은 넘긴다. 처음 한 번 찾는다, 없으면 None)
/// - 글자 캐시가 시스템 대체 글꼴 뒤에 이 글꼴을 둔다. (크롬 FontFallbackIterator 의 마지막 단계)
///
/// # 돌려줌
/// 깔린 첫 이름. 목록의 이름이 하나도 깔려 있지 않으면 None. (스레드마다 처음 찾은 값을 그대로 쓴다)
//==============================================================================
pub fn read_last_resort_family_name() -> Option<String> {
	thread_local! {
		static LAST_RESORT_FAMILY_NAME: std::cell::OnceCell<Option<String>> = const { std::cell::OnceCell::new() };
	}
	LAST_RESORT_FAMILY_NAME.with(|family_name_cell| {
		family_name_cell.get_or_init(|| platform_font::LAST_RESORT_FAMILY_LIST.iter().find(|family_name| !family_name.is_empty() && platform_font::find_family_font_file(family_name, 400, false).is_some()).map(|family_name| family_name.to_string())).clone()
	})
}

//==============================================================================
/// CSS 일반 글꼴 이름의 종류. (크롬 GenericFontFamilySettings 의 종류, 표의 칸 차례와 같다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenericFamily {
	/// 기본 글꼴. (표의 첫 칸, 일반 이름 -webkit-body 가 이것이다. 크롬 FontBuilder::GenericFontFamilyName 이 표준 글꼴 이름을 준다)
	Standard,
	/// serif.
	Serif,
	/// sans-serif.
	SansSerif,
	/// monospace.
	Monospace,
	/// cursive.
	Cursive,
	/// fantasy.
	Fantasy,
	/// math.
	Math,
}

impl GenericFamily {
	//==============================================================================
	/// 일반 이름 → 종류. (serif, sans-serif, monospace, cursive, fantasy, math, -webkit-body 는 Standard, 대소문자를 가리지 않는다. 크롬 font_selector.cc 가 아는 이름만, 아니면 None)
	///
	/// # 인자
	/// - `family_name`: CSS 글꼴 이름 하나. (앞뒤 빈칸은 뗀다)
	///
	/// # 돌려줌
	/// 그 일반 이름의 종류. 일반 이름이 아니면 None. (system-ui 같은 이름도 None)
	//==============================================================================
	pub fn from_family_name(family_name: &str) -> Option<GenericFamily> {
		match family_name.trim().to_ascii_lowercase().as_str() {
			"serif" => {
				Some(GenericFamily::Serif)
			}
			"sans-serif" => {
				Some(GenericFamily::SansSerif)
			}
			"monospace" => {
				Some(GenericFamily::Monospace)
			}
			"cursive" => {
				Some(GenericFamily::Cursive)
			}
			"fantasy" => {
				Some(GenericFamily::Fantasy)
			}
			"math" => {
				Some(GenericFamily::Math)
			}
			"-webkit-body" => {
				Some(GenericFamily::Standard)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 표의 칸 번호.
	//==============================================================================
	fn read_table_index(self) -> usize {
		match self {
			GenericFamily::Standard => {
				0
			}
			GenericFamily::Serif => {
				1
			}
			GenericFamily::SansSerif => {
				2
			}
			GenericFamily::Monospace => {
				3
			}
			GenericFamily::Cursive => {
				4
			}
			GenericFamily::Fantasy => {
				5
			}
			GenericFamily::Math => {
				6
			}
		}
	}
}

/// 크롬 화면 언어. (locale_settings_mac.grd 의 번역 목록, 의사 언어 ar-XB, en-XA 를 뺀다. 언어 부분만, zh 는 따로 zh-CN, zh-TW)
const CHROME_LOCALE_LANGUAGE_LIST: [&str; 55] = ["af", "am", "ar", "bg", "bn", "ca", "cs", "da", "de", "el", "en", "es", "et", "fa", "fi", "fil", "fr", "gu", "he", "hi", "hr", "hu", "id", "it", "ja", "kn", "ko", "lt", "lv", "ml", "mr", "ms", "nl", "no", "pl", "pt", "ro", "ru", "sk", "sl", "sr", "sv", "sw", "ta", "te", "th", "tr", "uk", "ur", "vi", "zh", "nb", "iw", "tl", "in"];

//==============================================================================
/// 크롬이 고르는 화면 언어. (시스템 선호 언어 차례에서 크롬에 번역이 있는 첫 언어, 없으면 "en")
/// - 언어 부분으로 맞춘다. 중국어는 번체(Hant)나 대만, 홍콩, 마카오면 zh-TW, 아니면 zh-CN. 옛 이름 iw 는 he, tl 은 fil, in 은 id, nb 는 no 다. (l10n_util 의 별칭)
/// - 글꼴 표가 언어마다 다른 것은 맥 ar, fa, he, ja, ko, zh-CN, zh-TW, 윈도우 bn, ja, ko, th, zh-CN, zh-TW, 리눅스 bn, ja, ko, th 뿐이다.
///
/// # 돌려줌
/// 크롬 화면 언어 이름. ("ko", "en", "zh-TW" 같은, 맞는 것이 없으면 "en")
//==============================================================================
pub fn read_browser_locale() -> String {
	for language_tag in crate::misc::localization::read_preferred_language_tags() {
		if let Some(browser_locale) = convert_to_browser_locale(&language_tag) {
			return browser_locale;
		}
	}
	String::from("en")
}

//==============================================================================
/// 언어 태그 → 크롬 화면 언어. (번역이 없는 언어면 None)
//==============================================================================
fn convert_to_browser_locale(language_tag: &str) -> Option<String> {
	let lowercase_tag = language_tag.trim().replace('_', "-").to_ascii_lowercase();
	let mut subtag_iterator = lowercase_tag.split('-');
	let language = subtag_iterator.next().unwrap_or("");
	let subtag_list: Vec<&str> = subtag_iterator.collect();
	if !CHROME_LOCALE_LANGUAGE_LIST.contains(&language) {
		return None;
	}
	let browser_locale = match language {
		"zh" => {
			let is_traditional = subtag_list.iter().any(|subtag| matches!(*subtag, "hant" | "tw" | "hk" | "mo"));
			if is_traditional {
				"zh-TW"
			}
			else {
				"zh-CN"
			}
		}
		"iw" => {
			"he"
		}
		"tl" => {
			"fil"
		}
		"in" => {
			"id"
		}
		"nb" => {
			"no"
		}
		_ => {
			language
		}
	};
	Some(browser_locale.to_string())
}

//==============================================================================
/// 크롬 기본 글꼴 표의 값. (화면 언어의 번역이 있으면 그것, 없으면 기본 값. 쉼표로 시작하면 이름 목록)
//==============================================================================
fn read_default_family_value(browser_locale: &str, generic_family: GenericFamily) -> &'static str {
	let table_index = generic_family.read_table_index();
	for (locale, value_list) in platform_font::LOCALE_FAMILY_TABLE {
		if *locale == *browser_locale {
			if let Some(value) = value_list[table_index] {
				return value;
			}
		}
	}
	platform_font::BASE_FAMILY_TABLE[table_index]
}

thread_local! {
	/// 일반 이름 → 고른 글꼴 이름. (화면 언어와 깔린 글꼴은 도는 동안 바뀌지 않는다고 보고 한 번 고른다)
	static GENERIC_FAMILY_NAME_TABLE: std::cell::RefCell<Vec<(GenericFamily, Option<String>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

//==============================================================================
/// 일반 이름 → 글꼴 이름. (크롬 GenericFontFamilySettings::Get 과 같은 값, 표가 없는 운영체제는 None)
/// - 쉼표로 시작하는 목록은 깔린 첫 글꼴, 하나도 없으면 첫 이름이다. (gfx::FontList::FirstAvailableOrFirst)
/// - 맥: monospace 가 Osaka 면 Osaka-Mono. (generic_font_family_settings.cc)
/// - 윈도우: monospace 가 Courier 로 시작하고 ClearType 이 켜져 있으면 Consolas. (prefs_tab_helper.cc 의 ShouldUseAlternateDefaultFixedFont)
///
/// # 인자
/// - `generic_family`: 일반 이름의 종류.
///
/// # 돌려줌
/// 고른 글꼴 이름. 표가 없는 운영체제이거나 그 칸의 값이 비었으면 None. (스레드마다 처음 고른 값을 그대로 쓴다)
//==============================================================================
pub fn resolve_generic_family_name(generic_family: GenericFamily) -> Option<String> {
	let cached_name = GENERIC_FAMILY_NAME_TABLE.with(|table_cell| table_cell.borrow().iter().find(|(cached_family, _)| *cached_family == generic_family).map(|(_, family_name)| family_name.clone()));
	if let Some(family_name) = cached_name {
		return family_name;
	}
	let browser_locale = read_browser_locale();
	let value = read_default_family_value(&browser_locale, generic_family);
	let family_name = if value.is_empty() {
		None
	}
	else if let Some(list_text) = value.strip_prefix(',') {
		let name_list: Vec<&str> = list_text.split(',').map(|name| name.trim()).filter(|name| !name.is_empty()).collect();
		let available_name = name_list.iter().find(|name| platform_font::is_family_available(name)).or(name_list.first());
		available_name.map(|name| name.to_string())
	}
	else {
		Some(value.to_string())
	};
	let family_name = family_name.map(|family_name| platform_font::adjust_generic_family_name(generic_family, family_name));
	GENERIC_FAMILY_NAME_TABLE.with(|table_cell| table_cell.borrow_mut().push((generic_family, family_name.clone())));
	family_name
}

//==============================================================================
/// 맥: CoreText.
//==============================================================================
#[cfg(target_os = "macos")]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;
	use objc2_core_foundation::{CFDictionary, CFNumber, CFRange, CFRetained, CFString, CFType, CFURL};
	use objc2_core_text::{kCTFontFamilyNameAttribute, kCTFontSymbolicTrait, kCTFontTraitsAttribute, kCTFontURLAttribute, kCTFontWeightTrait, kCTFontWidthTrait, CTFont, CTFontDescriptor, CTFontSymbolicTraits, CTFontUIFontType};

	/// 글꼴을 지을 때의 크기. (파일만 읽으므로 아무 크기나 된다)
	const LOOKUP_FONT_SIZE: f64 = 16.0;

	/// 크롬 맥 기본 글꼴. (locale_settings_mac.grd, 칸 차례: 표준, serif, sans-serif, monospace, cursive, fantasy, math)
	pub const BASE_FAMILY_TABLE: [&str; 7] = ["Times", "Times", "Helvetica", "Menlo", "Apple Chancery", "Papyrus", "STIX Two Math"];
	/// 화면 언어의 번역. (platform_locale_settings/`locale_settings_mac_<언어>`.xtb, None 은 기본 값)
	pub const LOCALE_FAMILY_TABLE: [(&str, [Option<&str>; 7]); 7] = [
		("ar", [None, Some("Times New Roman"), Some("Arial"), Some("Courier New"), None, None, None]),
		("fa", [None, Some("Times New Roman"), Some("Arial"), Some("Courier New"), None, None, None]),
		("he", [None, Some("Times New Roman"), Some("Arial"), Some("Courier New"), None, None, None]),
		("ja", [Some("Hiragino Kaku Gothic ProN"), Some("Hiragino Mincho ProN"), Some("Hiragino Kaku Gothic ProN"), Some(",Osaka,BIZ UDGothic,Menlo"), None, None, None]),
		("ko", [Some("Apple SD Gothic Neo"), Some("AppleMyungjo"), Some("Apple SD Gothic Neo"), None, None, None, None]),
		("zh-CN", [Some(",PingFang SC,STHeiti"), Some("Songti SC"), Some(",PingFang SC,STHeiti"), None, Some("Kaiti SC"), None, None]),
		("zh-TW", [Some(",PingFang TC,Heiti TC"), Some("Songti TC"), Some(",PingFang TC,Heiti TC"), None, Some("Kaiti TC"), None, None]),
	];
	/// 마지막 대체 글꼴 차례. (font_cache_mac.mm 의 GetLastResortFallbackFont)
	pub const LAST_RESORT_FAMILY_LIST: [&str; 2] = ["Times", "Lucida Grande"];

	//==============================================================================
	/// CSS 굵기 → CoreText 굵기 특성. (AppKit NSFontWeight 값, 사이는 곧게 잇는다)
	//==============================================================================
	fn convert_weight_trait(weight: u16) -> f64 {
		const WEIGHT_TABLE: [(f64, f64); 9] = [(100.0, -0.8), (200.0, -0.6), (300.0, -0.4), (400.0, 0.0), (500.0, 0.23), (600.0, 0.3), (700.0, 0.4), (800.0, 0.56), (900.0, 0.62)];
		let weight = (weight as f64).clamp(100.0, 900.0);
		for table_index in 1..WEIGHT_TABLE.len() {
			let (upper_weight, upper_trait) = WEIGHT_TABLE[table_index];
			if weight <= upper_weight {
				let (lower_weight, lower_trait) = WEIGHT_TABLE[table_index - 1];
				return lower_trait + (upper_trait - lower_trait) * (weight - lower_weight) / (upper_weight - lower_weight);
			}
		}
		WEIGHT_TABLE[WEIGHT_TABLE.len() - 1].1
	}

	//==============================================================================
	/// 글꼴의 파일과 PostScript 이름.
	//==============================================================================
	fn read_font_file(font: &CTFont) -> Option<FontFileSource> {
		let url_value = unsafe { font.attribute(kCTFontURLAttribute) }?;
		let url = url_value.downcast::<CFURL>().ok()?;
		let path = url.to_file_path()?;
		let post_script_name = unsafe { font.post_script_name() }.to_string();
		Some(FontFileSource {
			path,
			face_index: None,
			post_script_name: Some(post_script_name),
		})
	}

	//==============================================================================
	/// 시스템 UI 글꼴. (굵기가 600 이상이면 굵은 시스템 글꼴)
	//==============================================================================
	fn create_system_ui_font(weight: u16) -> Option<CFRetained<CTFont>> {
		let ui_font_type = if weight >= 600 {
			CTFontUIFontType::EmphasizedSystem
		}
		else {
			CTFontUIFontType::System
		};
		unsafe { CTFont::new_ui_font_for_language(ui_font_type, LOOKUP_FONT_SIZE, None) }
	}

	//==============================================================================
	/// 이름, 굵기, 기울기로 글꼴 짓기. (그 이름의 글꼴이 없으면 None)
	/// - 너비는 보통(0)을 청한다. (크롬 MatchFontFamily 도 font-stretch normal 로 고른다. 주지 않으면 Papyrus 가 Condensed 로 나온다)
	//==============================================================================
	fn create_family_font(family_name: &str, weight: u16, is_italic: bool) -> Option<CFRetained<CTFont>> {
		let family_text = CFString::from_str(family_name);
		let weight_number = CFNumber::new_f64(convert_weight_trait(weight));
		let symbolic_number = CFNumber::new_i64(if is_italic {
			CTFontSymbolicTraits::ItalicTrait.0 as i64
		}
		else {
			0
		});
		let width_number = CFNumber::new_f64(0.0);
		let trait_key_list: [&CFString; 3] = unsafe { [kCTFontWeightTrait, kCTFontWidthTrait, kCTFontSymbolicTrait] };
		let trait_value_list: [&CFType; 3] = [&weight_number, &width_number, &symbolic_number];
		let trait_dictionary = CFDictionary::<CFString, CFType>::from_slices(&trait_key_list, &trait_value_list);
		let attribute_key_list: [&CFString; 2] = unsafe { [kCTFontFamilyNameAttribute, kCTFontTraitsAttribute] };
		let attribute_value_list: [&CFType; 2] = [&family_text, &trait_dictionary];
		let attribute_dictionary = CFDictionary::<CFString, CFType>::from_slices(&attribute_key_list, &attribute_value_list);
		let font_descriptor = unsafe { CTFontDescriptor::with_attributes(attribute_dictionary.as_ref()) };
		let font = unsafe { CTFont::with_font_descriptor(&font_descriptor, LOOKUP_FONT_SIZE, std::ptr::null()) };
		// 찾지 못하면 CoreText 는 다른 글꼴을 준다. CSS 처럼 이름이 맞을 때만 쓴다.
		let found_family_name = unsafe { font.family_name() }.to_string();
		if !found_family_name.eq_ignore_ascii_case(family_name) {
			return None;
		}
		Some(font)
	}

	//==============================================================================
	/// 글꼴 이름으로 파일 찾기. (system-ui 와 BlinkMacSystemFont 는 시스템 UI 글꼴, 그 밖은 그 이름의 글꼴. 일반 이름은 위에서 글꼴 이름으로 바꿔 온다)
	//==============================================================================
	pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
		let lowercase_family_name = family_name.to_lowercase();
		let font = match lowercase_family_name.as_str() {
			"system-ui" | "blinkmacsystemfont" => {
				create_system_ui_font(weight)?
			}
			_ => {
				create_family_font(family_name, weight, is_italic)?
			}
		};
		read_font_file(&font)
	}

	//==============================================================================
	/// 그 이름의 글꼴이 깔렸는지. (기본 글꼴 표의 이름 목록에서 깔린 첫 글꼴을 고를 때)
	//==============================================================================
	pub fn is_family_available(family_name: &str) -> bool {
		create_family_font(family_name, 400, false).is_some()
	}

	//==============================================================================
	/// 고른 일반 글꼴 이름 고치기. (monospace 가 Osaka 면 Osaka-Mono, generic_font_family_settings.cc)
	//==============================================================================
	pub fn adjust_generic_family_name(generic_family: super::GenericFamily, family_name: String) -> String {
		if generic_family == super::GenericFamily::Monospace && family_name == "Osaka" {
			return String::from("Osaka-Mono");
		}
		family_name
	}

	//==============================================================================
	/// 대체 글꼴 파일 찾기. (시스템 UI 글꼴에서 CTFontCreateForString)
	//==============================================================================
	pub fn find_fallback_font_file(text: &str, weight: u16, is_italic: bool, is_color_wanted: bool) -> Option<FontFileSource> {
		if is_color_wanted {
			if let Some(font) = create_family_font("Apple Color Emoji", 400, false) {
				return read_font_file(&font);
			}
		}
		let base_font = create_system_ui_font(weight)?;
		let text_string = CFString::from_str(text);
		let range = CFRange {
			location: 0,
			length: text.encode_utf16().count() as isize,
		};
		let fallback_font = unsafe { base_font.for_string(&text_string, range) };
		let fallback_font = if is_italic {
			unsafe { fallback_font.copy_with_symbolic_traits(0.0, std::ptr::null(), CTFontSymbolicTraits::ItalicTrait, CTFontSymbolicTraits::ItalicTrait) }.unwrap_or(fallback_font)
		}
		else {
			fallback_font
		};
		read_font_file(&fallback_font)
	}
}

//==============================================================================
/// 윈도우: DirectWrite.
//==============================================================================
#[cfg(target_os = "windows")]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;
	use std::ffi::{c_void, OsString};
	use std::os::windows::ffi::OsStringExt;
	use std::path::PathBuf;
	use windows::core::{implement, Interface, OutRef, BOOL, PCWSTR};
	use windows::Win32::Foundation::E_INVALIDARG;
	use windows::Win32::Graphics::DirectWrite::{
		DWriteCreateFactory, IDWriteFactory, IDWriteFactory2, IDWriteFont, IDWriteFontCollection, IDWriteFontFamily, IDWriteFontFile, IDWriteLocalFontFileLoader, IDWriteNumberSubstitution, IDWriteTextAnalysisSource, IDWriteTextAnalysisSource_Impl, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE, DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_READING_DIRECTION, DWRITE_READING_DIRECTION_LEFT_TO_RIGHT,
	};

	/// 대체 글꼴을 고를 때의 기본 이름. (크롬 윈도우의 시스템 글꼴)
	const FALLBACK_BASE_FAMILY: &str = "Segoe UI";

	/// 크롬 윈도우 기본 글꼴. (locale_settings_win.grd, 칸 차례: 표준, serif, sans-serif, monospace, cursive, fantasy, math)
	pub const BASE_FAMILY_TABLE: [&str; 7] = ["Times New Roman", "Times New Roman", "Arial", "Courier New", "Comic Sans MS", "Impact", "Cambria Math"];
	/// 화면 언어의 번역. (platform_locale_settings/`locale_settings_win_<언어>`.xtb, None 은 기본 값)
	pub const LOCALE_FAMILY_TABLE: [(&str, [Option<&str>; 7]); 6] = [
		("bn", [None, None, Some("Vrinda"), None, None, None, None]),
		("ja", [Some(",Noto Sans JP,Noto Sans CJK JP,Meiryo,Yu Gothic"), Some(",Noto Serif JP,Noto Serif CJK JP,Yu Mincho,MS PMincho"), Some(",Noto Sans JP,Noto Sans CJK JP,Meiryo,Yu Gothic"), Some(",BIZ UDGothic,MS Gothic"), None, None, None]),
		("ko", [Some(",Noto Sans KR,Noto Sans CJK KR,Malgun Gothic"), Some(",Noto Serif KR,Noto Serif CJK KR,Batang"), Some(",Noto Sans KR,Noto Sans CJK KR,Malgun Gothic"), Some("Gulimche"), Some("Gungsuh"), None, None]),
		("th", [None, Some("Tahoma"), Some("Tahoma"), Some("Tahoma"), None, None, None]),
		("zh-CN", [Some(",Noto Sans SC,Noto Sans CJK SC,Microsoft YaHei"), Some(",Noto Serif SC,Noto Serif CJK SC,Simsun"), Some(",Noto Sans SC,Noto Sans CJK SC,Microsoft YaHei"), Some("NSimsun"), Some("KaiTi"), None, None]),
		("zh-TW", [None, None, None, None, Some("DFKai-SB"), None, None]),
	];
	/// 마지막 대체 글꼴 차례. (font_cache_skia.cc 의 GetLastResortFallbackFont, 이름 없는 것은 DirectWrite 가 찾지 못해 넘긴다)
	pub const LAST_RESORT_FAMILY_LIST: [&str; 8] = ["Sans", "Arial", "MS UI Gothic", "Microsoft Sans Serif", "Segoe UI", "Calibri", "Times New Roman", "Courier New"];

	thread_local! {
		/// DirectWrite 공유 공장. (스레드마다 처음 쓸 때 한 번 짓는다, 짓지 못하면 None 이라 아무 글꼴도 찾지 않는다)
		static DIRECT_WRITE_FACTORY: Option<IDWriteFactory> = unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) }.ok();
	}

	//==============================================================================
	/// DirectWrite 공장 반환.
	//==============================================================================
	fn get_direct_write_factory() -> Option<IDWriteFactory> {
		DIRECT_WRITE_FACTORY.with(|direct_write_factory| direct_write_factory.clone())
	}

	//==============================================================================
	/// 시스템 글꼴 모음 반환. (새 글꼴이 깔렸는지는 다시 살피지 않는다)
	//==============================================================================
	fn get_system_font_collection() -> Option<IDWriteFontCollection> {
		let direct_write_factory = get_direct_write_factory()?;
		let mut font_collection: Option<IDWriteFontCollection> = None;
		unsafe { direct_write_factory.GetSystemFontCollection(&mut font_collection, false) }.ok()?;
		font_collection
	}

	//==============================================================================
	/// 글을 끝에 0 을 붙인 UTF-16 으로. (DirectWrite 에 넘기는 이름)
	//==============================================================================
	fn encode_wide_null(text: &str) -> Vec<u16> {
		text.encode_utf16().chain(std::iter::once(0)).collect()
	}

	//==============================================================================
	/// 글꼴 모음에서 이름으로 글꼴 무리 찾기.
	//==============================================================================
	fn find_font_family(font_collection: &IDWriteFontCollection, family_name: &str) -> Option<IDWriteFontFamily> {
		let wide_family_name = encode_wide_null(family_name);
		let mut family_index: u32 = 0;
		let mut is_found = BOOL::default();
		unsafe { font_collection.FindFamilyName(PCWSTR(wide_family_name.as_ptr()), &mut family_index, &mut is_found) }.ok()?;
		if !is_found.as_bool() {
			return None;
		}
		unsafe { font_collection.GetFontFamily(family_index) }.ok()
	}

	//==============================================================================
	/// 그 이름의 글꼴이 깔렸는지. (시스템 글꼴 모음의 이름)
	//==============================================================================
	pub fn is_family_available(family_name: &str) -> bool {
		match get_system_font_collection() {
			Some(font_collection) => {
				find_font_family(&font_collection, family_name).is_some()
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 고른 일반 글꼴 이름 고치기. (monospace 가 Courier 로 시작하고 ClearType 이 켜져 있으면 Consolas, prefs_tab_helper.cc 의 ShouldUseAlternateDefaultFixedFont)
	//==============================================================================
	pub fn adjust_generic_family_name(generic_family: super::GenericFamily, family_name: String) -> String {
		if generic_family == super::GenericFamily::Monospace && family_name.to_ascii_lowercase().starts_with("courier") {
			let mut smoothing_type: u32 = 0;
			let is_read = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(windows_sys::Win32::UI::WindowsAndMessaging::SPI_GETFONTSMOOTHINGTYPE, 0, &mut smoothing_type as *mut u32 as *mut std::ffi::c_void, 0) } != 0;
			if is_read && smoothing_type == windows_sys::Win32::UI::WindowsAndMessaging::FE_FONTSMOOTHINGCLEARTYPE {
				return String::from("Consolas");
			}
		}
		family_name
	}

	//==============================================================================
	/// 메뉴 글꼴 이름. (크롬 윈도우의 system-ui, font_cache_skia_win.cc 의 SystemFontFamily 는 MenuFontFamily 다. 읽지 못하면 Segoe UI)
	//==============================================================================
	fn read_menu_font_family_name() -> String {
		let mut non_client_metrics: windows_sys::Win32::UI::WindowsAndMessaging::NONCLIENTMETRICSW = unsafe { std::mem::zeroed() };
		non_client_metrics.cbSize = std::mem::size_of::<windows_sys::Win32::UI::WindowsAndMessaging::NONCLIENTMETRICSW>() as u32;
		let is_read = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(windows_sys::Win32::UI::WindowsAndMessaging::SPI_GETNONCLIENTMETRICS, non_client_metrics.cbSize, &mut non_client_metrics as *mut windows_sys::Win32::UI::WindowsAndMessaging::NONCLIENTMETRICSW as *mut std::ffi::c_void, 0) } != 0;
		if !is_read {
			return String::from("Segoe UI");
		}
		let face_name = &non_client_metrics.lfMenuFont.lfFaceName;
		let name_length = face_name.iter().position(|character| *character == 0).unwrap_or(face_name.len());
		let family_name = String::from_utf16_lossy(&face_name[..name_length]);
		if family_name.is_empty() {
			String::from("Segoe UI")
		}
		else {
			family_name
		}
	}

	//==============================================================================
	/// 대체 글꼴 고르기에 줄 글과 언어와 방향. (IDWriteTextAnalysisSource 를 구현한 COM 객체)
	/// - 돌려주는 글과 언어 이름의 포인터는 이 객체가 든 Vec 을 가리킨다. DirectWrite 는 객체를 쥔 동안에만 읽는다.
	//==============================================================================
	#[implement(IDWriteTextAnalysisSource)]
	struct FallbackTextSource {
		text_unit_list: Vec<u16>,
		wide_locale_name: Vec<u16>,
	}

	impl IDWriteTextAnalysisSource_Impl for FallbackTextSource_Impl {
		//==============================================================================
		/// 그 자리부터 끝까지의 글. (끝이거나 넘으면 빈 글)
		//==============================================================================
		fn GetTextAtPosition(&self, text_position: u32, text_string: *mut *mut u16, text_length: *mut u32) -> windows::core::Result<()> {
			let total_length = self.text_unit_list.len() as u32;
			unsafe {
				if text_position >= total_length {
					*text_string = std::ptr::null_mut();
					*text_length = 0;
				}
				else {
					*text_string = self.text_unit_list.as_ptr().add(text_position as usize) as *mut u16;
					*text_length = total_length - text_position;
				}
			}
			Ok(())
		}

		//==============================================================================
		/// 그 자리 앞의 글. (처음이거나 넘으면 빈 글)
		//==============================================================================
		fn GetTextBeforePosition(&self, text_position: u32, text_string: *mut *mut u16, text_length: *mut u32) -> windows::core::Result<()> {
			let total_length = self.text_unit_list.len() as u32;
			unsafe {
				if text_position == 0 || text_position > total_length {
					*text_string = std::ptr::null_mut();
					*text_length = 0;
				}
				else {
					*text_string = self.text_unit_list.as_ptr() as *mut u16;
					*text_length = text_position;
				}
			}
			Ok(())
		}

		//==============================================================================
		/// 문단 방향. (왼쪽에서 오른쪽, 글자 고르기에는 쓰이지 않는다)
		//==============================================================================
		fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
			DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
		}

		//==============================================================================
		/// 그 자리부터 끝까지의 언어. (글 전체가 사용자 기본 언어)
		//==============================================================================
		fn GetLocaleName(&self, text_position: u32, text_length: *mut u32, locale_name: *mut *mut u16) -> windows::core::Result<()> {
			let total_length = self.text_unit_list.len() as u32;
			unsafe {
				*text_length = total_length.saturating_sub(text_position);
				*locale_name = self.wide_locale_name.as_ptr() as *mut u16;
			}
			Ok(())
		}

		//==============================================================================
		/// 숫자 바꾸기. (쓰지 않는다)
		//==============================================================================
		fn GetNumberSubstitution(&self, text_position: u32, text_length: *mut u32, number_substitution: OutRef<IDWriteNumberSubstitution>) -> windows::core::Result<()> {
			let total_length = self.text_unit_list.len() as u32;
			if text_position >= total_length {
				return Err(E_INVALIDARG.into());
			}
			unsafe {
				*text_length = total_length - text_position;
			}
			number_substitution.write(None)
		}
	}

	//==============================================================================
	/// 사용자 기본 언어 이름. ("ko-KR" 같은, 읽지 못하면 "en-US")
	//==============================================================================
	fn read_user_locale_name() -> String {
		let mut locale_buffer = [0u16; 85];
		let length = unsafe { windows_sys::Win32::Globalization::GetUserDefaultLocaleName(locale_buffer.as_mut_ptr(), locale_buffer.len() as i32) };
		if length <= 1 {
			return String::from("en-US");
		}
		String::from_utf16_lossy(&locale_buffer[..(length - 1) as usize])
	}

	//==============================================================================
	/// 글꼴 파일의 경로. (디스크에 있는 파일만, 메모리에서 읽은 글꼴이면 None)
	//==============================================================================
	fn read_font_file_path(font_file: &IDWriteFontFile) -> Option<PathBuf> {
		let mut reference_key: *mut c_void = std::ptr::null_mut();
		let mut reference_key_size: u32 = 0;
		unsafe { font_file.GetReferenceKey(&mut reference_key, &mut reference_key_size) }.ok()?;
		let font_file_loader = unsafe { font_file.GetLoader() }.ok()?;
		let local_font_file_loader = font_file_loader.cast::<IDWriteLocalFontFileLoader>().ok()?;
		let path_length = unsafe { local_font_file_loader.GetFilePathLengthFromKey(reference_key, reference_key_size) }.ok()?;
		let mut path_buffer = vec![0u16; path_length as usize + 1];
		unsafe { local_font_file_loader.GetFilePathFromKey(reference_key, reference_key_size, &mut path_buffer) }.ok()?;
		Some(PathBuf::from(OsString::from_wide(&path_buffer[..path_length as usize])))
	}

	//==============================================================================
	/// 글꼴의 파일과 얼굴 번호.
	//==============================================================================
	fn read_font_file(font: &IDWriteFont) -> Option<FontFileSource> {
		let font_face = unsafe { font.CreateFontFace() }.ok()?;
		let mut file_count: u32 = 0;
		unsafe { font_face.GetFiles(&mut file_count, None) }.ok()?;
		if file_count == 0 {
			return None;
		}
		let mut font_file_list: Vec<Option<IDWriteFontFile>> = vec![None; file_count as usize];
		unsafe { font_face.GetFiles(&mut file_count, Some(font_file_list.as_mut_ptr())) }.ok()?;
		let font_file = font_file_list.into_iter().next()??;
		Some(FontFileSource {
			path: read_font_file_path(&font_file)?,
			face_index: Some(unsafe { font_face.GetIndex() }),
			post_script_name: None,
		})
	}

	//==============================================================================
	/// 굵기와 기울기를 DirectWrite 값으로.
	//==============================================================================
	fn convert_font_style(weight: u16, is_italic: bool) -> (DWRITE_FONT_WEIGHT, DWRITE_FONT_STYLE) {
		let font_style = if is_italic {
			DWRITE_FONT_STYLE_ITALIC
		}
		else {
			DWRITE_FONT_STYLE_NORMAL
		};
		(DWRITE_FONT_WEIGHT(weight as i32), font_style)
	}

	//==============================================================================
	/// 글꼴 이름으로 파일 찾기.
	//==============================================================================
	pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
		let lowercase_family_name = family_name.to_lowercase();
		let system_family_name = match lowercase_family_name.as_str() {
			"system-ui" => {
				read_menu_font_family_name()
			}
			_ => {
				family_name.to_string()
			}
		};
		let font_collection = get_system_font_collection()?;
		let font_family = find_font_family(&font_collection, &system_family_name)?;
		let (font_weight, font_style) = convert_font_style(weight, is_italic);
		let font = unsafe { font_family.GetFirstMatchingFont(font_weight, DWRITE_FONT_STRETCH_NORMAL, font_style) }.ok()?;
		read_font_file(&font)
	}

	//==============================================================================
	/// 대체 글꼴 파일 찾기. (IDWriteFontFallback.MapCharacters, 윈도우 8.1 보다 옛 판이라 대체 글꼴 기능이 없으면 None)
	//==============================================================================
	pub fn find_fallback_font_file(text: &str, weight: u16, is_italic: bool, is_color_wanted: bool) -> Option<FontFileSource> {
		if is_color_wanted {
			if let Some(font_file_source) = find_family_font_file("Segoe UI Emoji", 400, false) {
				return Some(font_file_source);
			}
		}
		let direct_write_factory = get_direct_write_factory()?;
		let direct_write_factory_2 = direct_write_factory.cast::<IDWriteFactory2>().ok()?;
		let font_fallback = unsafe { direct_write_factory_2.GetSystemFontFallback() }.ok()?;
		let text_unit_list: Vec<u16> = text.encode_utf16().collect();
		let text_length = text_unit_list.len() as u32;
		let text_analysis_source: IDWriteTextAnalysisSource = FallbackTextSource {
			text_unit_list,
			wide_locale_name: encode_wide_null(&read_user_locale_name()),
		}
		.into();
		let font_collection = get_system_font_collection()?;
		let (font_weight, font_style) = convert_font_style(weight, is_italic);
		let wide_base_family_name = encode_wide_null(FALLBACK_BASE_FAMILY);
		let mut mapped_length: u32 = 0;
		let mut mapped_font: Option<IDWriteFont> = None;
		let mut scale: f32 = 1.0;
		unsafe { font_fallback.MapCharacters(&text_analysis_source, 0, text_length, &font_collection, PCWSTR(wide_base_family_name.as_ptr()), font_weight, font_style, DWRITE_FONT_STRETCH_NORMAL, &mut mapped_length, &mut mapped_font, &mut scale) }.ok()?;
		read_font_file(mapped_font.as_ref()?)
	}
}

//==============================================================================
/// 리눅스와 그 밖의 유닉스: fontconfig. (라이브러리를 실행할 때 열고, 없으면 아무것도 찾지 않는다)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;
	use fontconfig_sys::constants::{FC_CHARSET, FC_COLOR, FC_FAMILY, FC_FILE, FC_INDEX, FC_SLANT, FC_SLANT_ITALIC, FC_SLANT_ROMAN, FC_WEIGHT};
	use fontconfig_sys::{Fc, FcMatchPattern, FcPattern, FcResultMatch};
	use std::ffi::{CStr, CString};

	/// 크롬 리눅스 기본 글꼴. (locale_settings_linux.grd, 칸 차례: 표준, serif, sans-serif, monospace, cursive, fantasy, math)
	pub const BASE_FAMILY_TABLE: [&str; 7] = ["Times New Roman", "Times New Roman", "Arial", "Monospace", "Comic Sans MS", "Impact", "Latin Modern Math"];
	/// 화면 언어의 번역. (platform_locale_settings/`locale_settings_linux_<언어>`.xtb, None 은 기본 값)
	pub const LOCALE_FAMILY_TABLE: [(&str, [Option<&str>; 7]); 4] = [
		("bn", [None, None, Some("Vrinda"), None, None, None, None]),
		("ja", [Some(",Noto Sans JP,Noto Sans CJK JP,VL PGothic"), Some(",Noto Serif JP,Noto Serif CJK JP,Sazanami Mincho"), Some(",Noto Sans JP,Noto Sans CJK JP,VL PGothic"), Some("VL Gothic"), Some("Sazanami Mincho"), Some("Sazanami Mincho"), None]),
		("ko", [Some(",Noto Sans KR,Noto Sans CJK KR"), Some(",Noto Serif KR,Noto Serif CJK KR,NanumMyeongjo"), Some(",Noto Sans KR,Noto Sans CJK KR"), Some("UnDotum"), Some("UnGungsuh"), Some("UnYetGul"), None]),
		("th", [None, Some("Norasi"), Some("Norasi"), Some("TlwgMono"), None, None, None]),
	];
	/// 마지막 대체 글꼴 차례. (font_cache_skia.cc 의 GetLastResortFallbackFont, 이름 없음은 fontconfig 기본 글꼴)
	pub const LAST_RESORT_FAMILY_LIST: [&str; 3] = ["", "Sans", "Arial"];

	/// 치수가 같은 대체 글꼴 무리. (Skia SkFontConfigInterface_direct.cpp 의 kFontEquivMap, 무리 번호와 이름)
	const METRIC_COMPATIBLE_FAMILY_TABLE: [(u8, &str); 66] = [
		(1, "Arial"), (1, "Arimo"), (1, "Liberation Sans"),
		(2, "Times New Roman"), (2, "Tinos"), (2, "Liberation Serif"),
		(3, "Courier New"), (3, "Cousine"), (3, "Liberation Mono"),
		(4, "Symbol"), (4, "Symbol Neu"),
		(5, "MS PGothic"), (5, "\u{FF2D}\u{FF33} \u{FF30}\u{30B4}\u{30B7}\u{30C3}\u{30AF}"), (5, "Noto Sans CJK JP"), (5, "IPAPGothic"), (5, "MotoyaG04Gothic"),
		(6, "MS Gothic"), (6, "\u{FF2D}\u{FF33} \u{30B4}\u{30B7}\u{30C3}\u{30AF}"), (6, "Noto Sans Mono CJK JP"), (6, "IPAGothic"), (6, "MotoyaG04GothicMono"),
		(7, "MS PMincho"), (7, "\u{FF2D}\u{FF33} \u{FF30}\u{660E}\u{671D}"), (7, "Noto Serif CJK JP"), (7, "IPAPMincho"), (7, "MotoyaG04Mincho"),
		(8, "MS Mincho"), (8, "\u{FF2D}\u{FF33} \u{660E}\u{671D}"), (8, "Noto Serif CJK JP"), (8, "IPAMincho"), (8, "MotoyaG04MinchoMono"),
		(9, "Simsun"), (9, "\u{5B8B}\u{4F53}"), (9, "Noto Serif CJK SC"), (9, "MSung GB18030"), (9, "Song ASC"),
		(10, "NSimsun"), (10, "\u{65B0}\u{5B8B}\u{4F53}"), (10, "Noto Serif CJK SC"), (10, "MSung GB18030"), (10, "N Song ASC"),
		(11, "Simhei"), (11, "\u{9ED1}\u{4F53}"), (11, "Noto Sans CJK SC"), (11, "MYingHeiGB18030"), (11, "MYingHeiB5HK"),
		(12, "PMingLiU"), (12, "\u{65B0}\u{7D30}\u{660E}\u{9AD4}"), (12, "Noto Serif CJK TC"), (12, "MSung B5HK"),
		(13, "MingLiU"), (13, "\u{7D30}\u{660E}\u{9AD4}"), (13, "Noto Serif CJK TC"), (13, "MSung B5HK"),
		(14, "PMingLiU_HKSCS"), (14, "\u{65B0}\u{7D30}\u{660E}\u{9AD4}_HKSCS"), (14, "Noto Serif CJK TC"), (14, "MSung B5HK"),
		(15, "MingLiU_HKSCS"), (15, "\u{7D30}\u{660E}\u{9AD4}_HKSCS"), (15, "Noto Serif CJK TC"), (15, "MSung B5HK"),
		(16, "Cambria"), (16, "Caladea"),
		(17, "Calibri"), (17, "Carlito"),
	];

	//==============================================================================
	/// fontconfig 라이브러리. (처음 쓸 때 연다, 없으면 None)
	//==============================================================================
	fn read_library() -> Option<&'static Fc> {
		fontconfig_sys::statics::LIB_RESULT.as_ref().ok()
	}

	//==============================================================================
	/// 치수가 같은 대체 글꼴인지. (같은 무리에 든 두 이름, 대소문자를 가리지 않는다. Skia IsMetricCompatibleReplacement)
	//==============================================================================
	fn is_metric_compatible_replacement(first_family_name: &str, second_family_name: &str) -> bool {
		let find_group = |family_name: &str| -> Option<u8> {
			METRIC_COMPATIBLE_FAMILY_TABLE.iter().find(|(_, table_family_name)| table_family_name.eq_ignore_ascii_case(family_name)).map(|(group_number, _)| *group_number)
		};
		match (find_group(first_family_name), find_group(second_family_name)) {
			(Some(first_group), Some(second_group)) => {
				first_group == second_group
			}
			_ => {
				false
			}
		}
	}

	//==============================================================================
	/// 그 이름의 글꼴이 깔렸는지. (fontconfig 가 찾은 글꼴을 크롬처럼 받을 때)
	//==============================================================================
	pub fn is_family_available(family_name: &str) -> bool {
		find_family_font_file(family_name, 400, false).is_some()
	}

	//==============================================================================
	/// 고른 일반 글꼴 이름 고치기. (리눅스는 고치지 않는다)
	//==============================================================================
	pub fn adjust_generic_family_name(_generic_family: super::GenericFamily, family_name: String) -> String {
		family_name
	}

	//==============================================================================
	/// CSS 굵기 → fontconfig 굵기. (OpenType 굵기 표와 같은 점들, 사이는 곧게 잇는다)
	//==============================================================================
	fn convert_weight(weight: u16) -> i32 {
		const WEIGHT_TABLE: [(f32, f32); 10] = [(100.0, 0.0), (200.0, 40.0), (300.0, 50.0), (350.0, 75.0), (400.0, 80.0), (500.0, 100.0), (600.0, 180.0), (700.0, 200.0), (800.0, 205.0), (900.0, 210.0)];
		let weight = (weight as f32).clamp(100.0, 900.0);
		for table_index in 1..WEIGHT_TABLE.len() {
			let (upper_weight, upper_value) = WEIGHT_TABLE[table_index];
			if weight <= upper_weight {
				let (lower_weight, lower_value) = WEIGHT_TABLE[table_index - 1];
				return (lower_value + (upper_value - lower_value) * (weight - lower_weight) / (upper_weight - lower_weight)).round() as i32;
			}
		}
		210
	}

	//==============================================================================
	/// 패턴의 파일과 얼굴 번호.
	//==============================================================================
	fn read_pattern_file(library: &Fc, pattern: *mut FcPattern) -> Option<FontFileSource> {
		let mut file_text: *mut u8 = std::ptr::null_mut();
		let file_result = unsafe { (library.FcPatternGetString)(pattern, FC_FILE.as_ptr(), 0, &mut file_text) };
		if file_result != FcResultMatch || file_text.is_null() {
			return None;
		}
		let path = std::path::PathBuf::from(unsafe { CStr::from_ptr(file_text as *const std::ffi::c_char) }.to_string_lossy().into_owned());
		let mut face_index: i32 = 0;
		let index_result = unsafe { (library.FcPatternGetInteger)(pattern, FC_INDEX.as_ptr(), 0, &mut face_index) };
		Some(FontFileSource {
			path,
			face_index: if index_result == FcResultMatch {
				Some((face_index & 0xFFFF) as u32)
			}
			else {
				Some(0)
			},
			post_script_name: None,
		})
	}

	//==============================================================================
	/// 굵기와 기울기를 담은 패턴. (찾고 나면 FcPatternDestroy)
	//==============================================================================
	fn create_pattern(library: &Fc, family_name: &str, weight: u16, is_italic: bool) -> Option<*mut FcPattern> {
		let family_text = CString::new(family_name).ok()?;
		let pattern = unsafe { (library.FcPatternCreate)() };
		if pattern.is_null() {
			return None;
		}
		let slant = if is_italic {
			FC_SLANT_ITALIC
		}
		else {
			FC_SLANT_ROMAN
		};
		unsafe {
			if !family_name.is_empty() {
				(library.FcPatternAddString)(pattern, FC_FAMILY.as_ptr(), family_text.as_ptr() as *const u8);
			}
			(library.FcPatternAddInteger)(pattern, FC_WEIGHT.as_ptr(), convert_weight(weight));
			(library.FcPatternAddInteger)(pattern, FC_SLANT.as_ptr(), slant);
		}
		Some(pattern)
	}

	//==============================================================================
	/// 패턴에 설정과 기본값 채우기.
	//==============================================================================
	fn substitute_pattern(library: &Fc, pattern: *mut FcPattern) {
		unsafe {
			(library.FcConfigSubstitute)(std::ptr::null_mut(), pattern, FcMatchPattern);
			(library.FcDefaultSubstitute)(pattern);
		}
	}

	//==============================================================================
	/// 패턴의 몇째 이름. (없으면 None)
	//==============================================================================
	fn read_pattern_family_name(library: &Fc, pattern: *mut FcPattern, family_index: i32) -> Option<String> {
		let mut family_text: *mut u8 = std::ptr::null_mut();
		let family_result = unsafe { (library.FcPatternGetString)(pattern, FC_FAMILY.as_ptr(), family_index, &mut family_text) };
		if family_result != FcResultMatch || family_text.is_null() {
			return None;
		}
		Some(unsafe { CStr::from_ptr(family_text as *const std::ffi::c_char) }.to_string_lossy().into_owned())
	}

	//==============================================================================
	/// 글꼴 이름으로 파일 찾기. (크롬처럼 받을 만한 글꼴만, Skia SkFontConfigInterfaceDirect::matchFamilyName 과 isAcceptableMatch)
	/// - 청한 이름이 비었거나 sans, serif, monospace 면 fontconfig 가 고른 대로 받는다.
	/// - 그 밖은 찾은 글꼴의 이름 가운데 설정을 거친 첫 이름이나 청한 이름과 같은 것, 또는 치수가 같은 대체 글꼴이 있어야 받는다.
	/// - system-ui 는 fontconfig 의 sans 로 찾는다. (크롬 리눅스는 데스크탑 설정의 UI 글꼴 이름을 쓴다, 그 이름을 읽지 않으므로 기본 sans 로 갈음한다)
	//==============================================================================
	pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
		let library = read_library()?;
		let lookup_family_name = if family_name.eq_ignore_ascii_case("system-ui") {
			"sans"
		}
		else {
			family_name
		};
		let is_fallback_allowed = lookup_family_name.is_empty() || lookup_family_name.eq_ignore_ascii_case("sans") || lookup_family_name.eq_ignore_ascii_case("serif") || lookup_family_name.eq_ignore_ascii_case("monospace");
		let pattern = create_pattern(library, lookup_family_name, weight, is_italic)?;
		substitute_pattern(library, pattern);
		let post_config_family_name = read_pattern_family_name(library, pattern, 0).unwrap_or_default();
		let mut match_result = 0;
		let matched_pattern = unsafe { (library.FcFontMatch)(std::ptr::null_mut(), pattern, &mut match_result) };
		unsafe { (library.FcPatternDestroy)(pattern) };
		if matched_pattern.is_null() {
			return None;
		}
		let mut is_family_matched = is_fallback_allowed;
		let mut family_index = 0;
		while !is_family_matched && family_index < 255 {
			let found_family_name = match read_pattern_family_name(library, matched_pattern, family_index) {
				Some(found_family_name) => {
					found_family_name
				}
				None => {
					break;
				}
			};
			is_family_matched = found_family_name.eq_ignore_ascii_case(&post_config_family_name) || found_family_name.eq_ignore_ascii_case(lookup_family_name) || is_metric_compatible_replacement(lookup_family_name, &found_family_name);
			family_index += 1;
		}
		let font_file_source = if is_family_matched {
			read_pattern_file(library, matched_pattern)
		}
		else {
			None
		};
		unsafe { (library.FcPatternDestroy)(matched_pattern) };
		font_file_source
	}

	//==============================================================================
	/// 대체 글꼴 파일 찾기. (글의 첫 글자를 담은 charset 으로 FcFontSort, 그 글자를 가진 첫 글꼴)
	//==============================================================================
	pub fn find_fallback_font_file(text: &str, weight: u16, is_italic: bool, is_color_wanted: bool) -> Option<FontFileSource> {
		let library = read_library()?;
		let character = text.chars().next()? as u32;
		let pattern = create_pattern(library, "sans-serif", weight, is_italic)?;
		let character_set = unsafe { (library.FcCharSetCreate)() };
		unsafe {
			(library.FcCharSetAddChar)(character_set, character);
			(library.FcPatternAddCharSet)(pattern, FC_CHARSET.as_ptr(), character_set);
			if is_color_wanted {
				(library.FcPatternAddBool)(pattern, FC_COLOR.as_ptr(), 1);
			}
		}
		substitute_pattern(library, pattern);
		let mut sort_result = 0;
		let font_set = unsafe { (library.FcFontSort)(std::ptr::null_mut(), pattern, 1, std::ptr::null_mut(), &mut sort_result) };
		let mut font_file_source: Option<FontFileSource> = None;
		if !font_set.is_null() {
			let font_count = unsafe { (*font_set).nfont }.max(0) as usize;
			for font_index in 0..font_count {
				let font_pattern = unsafe { *(*font_set).fonts.add(font_index) };
				let mut font_character_set = std::ptr::null_mut();
				let charset_result = unsafe { (library.FcPatternGetCharSet)(font_pattern, FC_CHARSET.as_ptr(), 0, &mut font_character_set) };
				if charset_result != FcResultMatch || font_character_set.is_null() {
					continue;
				}
				let has_character = unsafe { (library.FcCharSetHasChar)(font_character_set, character) } != 0;
				if has_character {
					font_file_source = read_pattern_file(library, font_pattern);
					break;
				}
			}
			unsafe { (library.FcFontSetDestroy)(font_set) };
		}
		unsafe {
			(library.FcPatternDestroy)(pattern);
			(library.FcCharSetDestroy)(character_set);
		}
		font_file_source
	}
}

//==============================================================================
/// 그 밖의 운영체제: 찾지 않는다.
//==============================================================================
#[cfg(not(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android")))))]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;

	/// 기본 글꼴 표가 없다.
	pub const BASE_FAMILY_TABLE: [&str; 7] = [""; 7];
	pub const LOCALE_FAMILY_TABLE: [(&str, [Option<&str>; 7]); 0] = [];
	pub const LAST_RESORT_FAMILY_LIST: [&str; 0] = [];

	//==============================================================================
	/// 그 이름의 글꼴이 깔렸는지. (찾지 않는다)
	//==============================================================================
	pub fn is_family_available(_family_name: &str) -> bool {
		false
	}

	//==============================================================================
	/// 고른 일반 글꼴 이름 고치기. (고치지 않는다)
	//==============================================================================
	pub fn adjust_generic_family_name(_generic_family: super::GenericFamily, family_name: String) -> String {
		family_name
	}

	//==============================================================================
	/// 글꼴 이름으로 파일 찾기. (찾지 않는다)
	//==============================================================================
	pub fn find_family_font_file(_family_name: &str, _weight: u16, _is_italic: bool) -> Option<FontFileSource> {
		None
	}

	//==============================================================================
	/// 대체 글꼴 파일 찾기. (찾지 않는다)
	//==============================================================================
	pub fn find_fallback_font_file(_text: &str, _weight: u16, _is_italic: bool, _is_color_wanted: bool) -> Option<FontFileSource> {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 한글 대체 글꼴과 일반 이름 sans-serif 는 찾고, 없는 이름은 None. (맥, 윈도우, fontconfig 가 있는 리눅스)
	//==============================================================================
	#[test]
	fn finds_system_fonts_and_rejects_unknown_family() {
		assert!(find_family_font_file("NoSuchFamily VanillaEngine", 400, false).is_none());
		if let Some(korean_font_file) = find_fallback_font_file("가", 400, false, false) {
			assert!(korean_font_file.path.exists());
		}
		if let Some(sans_serif_font_file) = find_family_font_file("sans-serif", 700, false) {
			assert!(sans_serif_font_file.path.exists());
		}
	}
}
