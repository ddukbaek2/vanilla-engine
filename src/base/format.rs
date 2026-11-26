//==============================================================================
//! 표기 유틸. (원본 base/format.js, 원본 import.js 의 Format)
//! - 재화, 점수 표기처럼 어느 게임에서나 되풀이되는 숫자 글 만들기를 모았다.
//! - 원본은 JS 의 수(f64)를 받는다. 러스트 판도 f64 를 받고, 반올림과 음수, 0, 무한대, NaN 의 글을 원본과 같게 낸다.
//!   toFixed 는 실제 값으로 반올림하고 딱 반이면 올린다. toLocaleString 은 가장 짧은 십진 표기에서 소수 셋째 자리로 반을 올린다.
//!   String(수)는 Number.prototype.toString 꼴이라 1e21 이상과 1e-7 이하는 지수로 쓴다. (1e+21, 1e-7)
//! - 원본의 기본 인자는 러스트에 없어서 늘 넘긴다. (원본 기본값: format_number 의 locale 은 "en-US", format_number_with_unit 은 "ko-KR",
//!   format_compact_number 의 fraction_digits 는 1, format_padded_number 의 범위는 f64::NEG_INFINITY 와 f64::INFINITY)
//! - 로캘: 원본은 브라우저의 Intl(ICU)에 맡긴다. 러스트 std 에는 그런 것이 없어서 소수점, 천 단위 기호, 음수 앞붙이, 숫자 모양,
//!   위 묶음 길이(인도식은 3 다음 2), 최소 묶음 자리(1234 는 묶지 않는 es, pl 등), NaN 글을 node 25(ICU 78.3, CLDR 48)의 Intl 에서 뽑아 표로 넣었다.
//!   무한대는 모든 로캘이 ∞ 다. 언어 태그는 대소문자를 가리지 않고 뒤 조각부터 하나씩 떼며 표에서 찾는다. (zh-Hant-TW → zh-hant)
//!   유니코드 확장(-u-...)은 보지 않고, 밑줄은 하이픈으로 본다. 표에 없는 태그는 원본이 브라우저의 기본 로캘로 쓰듯 시스템 언어로 쓴다.
//!   시스템 언어는 Localization.detect_language 와 같은 공용 함수(localization::read_preferred_language_tags)의 첫째다.
//!   (웹은 navigator.languages, 데스크탑은 sys-locale 이 읽은 시스템 선호 언어 차례: 윈도우 GetUserPreferredUILanguages, 맥 CFLocaleCopyPreferredLanguages,
//!   리눅스 LANGUAGE / LC_ALL / LC_MESSAGES / LANG. 그것도 표에 없으면 en-US)
//!   (원본은 틀린 태그면 RangeError 를 던진다. 러스트 판은 던지지 않고 표에 없는 태그처럼 쓴다)
//! - fraction_digits 는 원본 toFixed 처럼 0 ~ 100 이 맞다. 넘겨도 던지지 않고 그 자릿수로 낸다.
//!
//! # 예제
//! 점수, 재화, 시간 표기
//! ```rust
//! use vanilla::format;
//!
//! assert_eq!(format::format_number(12480.0, "en-US"), "12,480");
//! assert_eq!(format::format_number_with_unit(1200000.0, " G", "ko-KR"), "1,200,000 G");
//! assert_eq!(format::format_compact_number(5600000.0, 1), "5.6M");
//! assert_eq!(format::format_padded_number(7.0, 3, 0.0, 999.0), "007");
//! assert_eq!(format::format_minutes_seconds(75.0), "1:15");
//! ```
//==============================================================================
// 옮긴 원문: ICU4C, CLDR 의 숫자 꼴 자료에서 옮김, Unicode-3.0, THIRD_PARTY_LICENSES.md

/// 라틴 숫자 모양.
const LATIN_DIGITS: &str = "0123456789";
/// 대부분의 로캘이 쓰는 NaN 글.
const NOT_A_NUMBER_TEXT: &str = "NaN";
/// 무한대 글. (모든 로캘이 같다)
const INFINITY_TEXT: &str = "\u{221e}";
/// toLocaleString 기본 옵션의 소수 최대 자릿수.
const LOCALE_MAXIMUM_FRACTION_DIGITS: usize = 3;
/// 천 단위 묶음의 맨 아래 묶음 길이. (모든 로캘이 3 이다)
const PRIMARY_GROUP_SIZE: usize = 3;
/// 표에 없는 로캘이 쓰는 태그. (en-US 는 en 과 같다)
const FALLBACK_LOCALE_TAG: &str = "en";
/// 가장 짧은 표기가 딱 가운데일 수 있는 이진 소수 자릿수의 끝. (5^25 가 10^18 보다 작다)
const MAXIMUM_TIE_BINARY_FRACTION_DIGITS: u32 = 25;
/// 큰 수를 줄이는 단위의 나눌 수와 붙일 글. (작은 것부터, 원본 COMPACT_UNIT_LIST)
const COMPACT_UNIT_LIST: [(f64, &str); 3] = [(1000.0, "K"), (1000000.0, "M"), (1000000000.0, "B")];

//==============================================================================
/// 로캘 하나의 숫자 표기. (Intl.NumberFormat 기본 옵션의 결과)
//==============================================================================
struct NumberLocale {
	tag: &'static str,
	decimal_separator: &'static str,
	group_separator: &'static str,
	minus_prefix: &'static str,
	digits: &'static str,
	secondary_group_size: usize,
	minimum_grouping_digits: usize,
	not_a_number_text: &'static str,
}

impl NumberLocale {
	//==============================================================================
	/// 생성.
	//==============================================================================
	const fn create(tag: &'static str, decimal_separator: &'static str, group_separator: &'static str, minus_prefix: &'static str, digits: &'static str, secondary_group_size: usize, minimum_grouping_digits: usize, not_a_number_text: &'static str) -> NumberLocale {
		NumberLocale {
			tag,
			decimal_separator,
			group_separator,
			minus_prefix,
			digits,
			secondary_group_size,
			minimum_grouping_digits,
			not_a_number_text,
		}
	}
}

/// 로캘 표. (언어 기본 줄과, 언어 기본과 다른 지역 줄만. 태그는 소문자)
const NUMBER_LOCALES: &[NumberLocale] = &[
	NumberLocale::create("af", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("am", ".", ",", "-", LATIN_DIGITS, 3, 1, "\u{1260}\u{1241}\u{1325}\u{122d}\u{a0}\u{120a}\u{1308}\u{1208}\u{133d}\u{a0}\u{12e8}\u{121b}\u{12ed}\u{127d}\u{120d}"),
	NumberLocale::create("ar", ".", ",", "\u{200e}-", LATIN_DIGITS, 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("az", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("be", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("bg", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("bn", ".", ",", "-", "\u{9e6}\u{9e7}\u{9e8}\u{9e9}\u{9ea}\u{9eb}\u{9ec}\u{9ed}\u{9ee}\u{9ef}", 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("bs", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ca", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("cs", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("cy", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("da", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("de", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("el", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es", ",", ".", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("et", ",", "\u{a0}", "\u{2212}", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("eu", ",", ".", "\u{2212}", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("fa", "\u{66b}", "\u{66c}", "\u{200e}\u{2212}", "\u{6f0}\u{6f1}\u{6f2}\u{6f3}\u{6f4}\u{6f5}\u{6f6}\u{6f7}\u{6f8}\u{6f9}", 3, 1, "\u{646}\u{627}\u{639}\u{62f}\u{62f}"),
	NumberLocale::create("fi", ",", "\u{a0}", "\u{2212}", LATIN_DIGITS, 3, 1, "ep\u{e4}luku"),
	NumberLocale::create("fil", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("fr", ",", "\u{202f}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ga", ".", ",", "-", LATIN_DIGITS, 3, 1, "Nuimh"),
	NumberLocale::create("gl", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("gu", ".", ",", "-", LATIN_DIGITS, 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("he", ".", ",", "\u{200e}-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("hi", ".", ",", "-", LATIN_DIGITS, 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("hr", ",", ".", "\u{2212}", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("hu", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("hy", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, "\u{548}\u{579}\u{539}"),
	NumberLocale::create("id", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("is", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("it", ",", ".", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ja", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ka", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, "\u{10d0}\u{10e0}\u{a0}\u{10d0}\u{10e0}\u{10d8}\u{10e1}\u{a0}\u{10e0}\u{10d8}\u{10ea}\u{10ee}\u{10d5}\u{10d8}"),
	NumberLocale::create("kk", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, "\u{441}\u{430}\u{43d}\u{a0}\u{435}\u{43c}\u{435}\u{441}"),
	NumberLocale::create("km", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("kn", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ko", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ky", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, "\u{441}\u{430}\u{43d}\u{a0}\u{44d}\u{43c}\u{435}\u{441}"),
	NumberLocale::create("lo", ",", ".", "-", LATIN_DIGITS, 3, 1, "\u{e9a}\u{ecd}\u{ec8}\u{200b}\u{ec1}\u{ea1}\u{ec8}\u{e99}\u{200b}\u{ec2}\u{e95}\u{200b}\u{ec0}\u{ea5}\u{e81}"),
	NumberLocale::create("lt", ",", "\u{a0}", "\u{2212}", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("lv", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, "NS"),
	NumberLocale::create("mk", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ml", ".", ",", "-", LATIN_DIGITS, 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("mn", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("mr", ".", ",", "-", "\u{966}\u{967}\u{968}\u{969}\u{96a}\u{96b}\u{96c}\u{96d}\u{96e}\u{96f}", 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ms", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("my", ".", ",", "-", "\u{1040}\u{1041}\u{1042}\u{1043}\u{1044}\u{1045}\u{1046}\u{1047}\u{1048}\u{1049}", 3, 1, "\u{1002}\u{100f}\u{1014}\u{103a}\u{1038}\u{1019}\u{101f}\u{102f}\u{1010}\u{103a}\u{101e}\u{1031}\u{102c}"),
	NumberLocale::create("nb", ",", "\u{a0}", "\u{2212}", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ne", ".", ",", "-", "\u{966}\u{967}\u{968}\u{969}\u{96a}\u{96b}\u{96c}\u{96d}\u{96e}\u{96f}", 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("nl", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("no", ",", "\u{a0}", "\u{2212}", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("pa", ".", ",", "-", LATIN_DIGITS, 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("pl", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ps", "\u{66b}", "\u{66c}", "\u{200e}-\u{200e}", "\u{6f0}\u{6f1}\u{6f2}\u{6f3}\u{6f4}\u{6f5}\u{6f6}\u{6f7}\u{6f8}\u{6f9}", 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("pt", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ro", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ru", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, "\u{43d}\u{435}\u{a0}\u{447}\u{438}\u{441}\u{43b}\u{43e}"),
	NumberLocale::create("si", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("sk", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("sl", ",", ".", "\u{2212}", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("sq", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("sr", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("sv", ",", "\u{a0}", "\u{2212}", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("sw", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ta", ".", ",", "-", LATIN_DIGITS, 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("te", ".", ",", "-", LATIN_DIGITS, 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("th", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("tr", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("uk", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ur", ".", ",", "\u{200e}-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("uz", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, "son\u{a0}emas"),
	NumberLocale::create("vi", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("zh", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("zu", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-in", ".", ",", "-", LATIN_DIGITS, 2, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-za", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-ch", ".", "'", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-de", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-nl", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-se", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-dk", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-fi", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-at", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("en-be", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("de-at", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("de-ch", ".", "'", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("de-li", ".", "'", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("fr-ca", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("fr-ch", ",", "'", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("fr-lu", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-mx", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-ar", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-co", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-cl", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-us", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-419", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-pe", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-ve", ",", ".", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-cr", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("es-pr", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("pt-pt", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("it-ch", ".", "'", "-", LATIN_DIGITS, 3, 2, NOT_A_NUMBER_TEXT),
	NumberLocale::create("zh-tw", ".", ",", "-", LATIN_DIGITS, 3, 1, "\u{975e}\u{6578}\u{503c}"),
	NumberLocale::create("zh-hk", ".", ",", "-", LATIN_DIGITS, 3, 1, "\u{975e}\u{6578}\u{503c}"),
	NumberLocale::create("zh-hant", ".", ",", "-", LATIN_DIGITS, 3, 1, "\u{975e}\u{6578}\u{503c}"),
	NumberLocale::create("zh-mo", ".", ",", "-", LATIN_DIGITS, 3, 1, "\u{975e}\u{6578}\u{503c}"),
	NumberLocale::create("ar-sa", "\u{66b}", "\u{66c}", "\u{61c}-", "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}", 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-eg", "\u{66b}", "\u{66c}", "\u{61c}-", "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}", 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-ma", ",", ".", "\u{200e}-", LATIN_DIGITS, 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-dz", ",", ".", "\u{200e}-", LATIN_DIGITS, 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-tn", ",", ".", "\u{200e}-", LATIN_DIGITS, 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-iq", "\u{66b}", "\u{66c}", "\u{61c}-", "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}", 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-jo", "\u{66b}", "\u{66c}", "\u{61c}-", "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}", 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-kw", "\u{66b}", "\u{66c}", "\u{61c}-", "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}", 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-lb", "\u{66b}", "\u{66c}", "\u{61c}-", "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}", 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ar-qa", "\u{66b}", "\u{66c}", "\u{61c}-", "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}", 3, 1, "\u{644}\u{64a}\u{633}\u{a0}\u{631}\u{642}\u{645}\u{64b}\u{627}"),
	NumberLocale::create("ru-ua", ",", "\u{a0}", "-", LATIN_DIGITS, 3, 2, "\u{43d}\u{435}\u{a0}\u{447}\u{438}\u{441}\u{43b}\u{43e}"),
	NumberLocale::create("ur-in", "\u{66b}", "\u{66c}", "\u{200e}-\u{200e}", "\u{6f0}\u{6f1}\u{6f2}\u{6f3}\u{6f4}\u{6f5}\u{6f6}\u{6f7}\u{6f8}\u{6f9}", 3, 1, NOT_A_NUMBER_TEXT),
	NumberLocale::create("ta-my", ".", ",", "-", LATIN_DIGITS, 3, 1, NOT_A_NUMBER_TEXT),
];

//==============================================================================
/// 천 단위 구분 기호를 붙여 반환. (12480 -> "12,480", 원본 기본 locale 은 "en-US")
///
/// # 인자
/// - `value`: 표기할 수.
/// - `locale`: 언어 태그. ("en-US", "ko-KR" 처럼, 대소문자를 가리지 않는다. 표에 없으면 시스템 언어, 그것도 표에 없으면 en-US 로 쓴다)
///
/// # 돌려줌
/// 로캘 표기 글. (소수는 0 ~ 3 자리로 반 올림하고 끝의 0 은 뺀다. NaN 은 로캘의 NaN 글, 무한대는 ∞)
///
/// # 예제
/// 로캘마다 다른 소수점과 묶음 기호
/// ```rust
/// use vanilla::format;
///
/// assert_eq!(format::format_number(1234.5678, "en-US"), "1,234.568");
/// assert_eq!(format::format_number(1234567.891, "de-DE"), "1.234.567,891");
/// assert_eq!(format::format_number(123456789.123, "hi-IN"), "12,34,56,789.123");
/// assert_eq!(format::format_number(-0.0001, "en-US"), "-0");
/// ```
//==============================================================================
pub fn format_number(value: f64, locale: &str) -> String {
	let number_locale = find_number_locale(locale);
	format_locale_number(value, number_locale)
}

//==============================================================================
/// 천 단위 구분 + 단위 접미사. (1200000, "원" -> "1,200,000원", 원본 기본 locale 은 "ko-KR")
///
/// # 인자
/// - `value`: 표기할 수.
/// - `unit_suffix`: 뒤에 붙일 단위 글. (사이에 공백을 넣지 않는다)
/// - `locale`: 언어 태그. (format_number 와 같다)
///
/// # 돌려줌
/// format_number 의 글 뒤에 unit_suffix 를 붙인 글.
//==============================================================================
pub fn format_number_with_unit(value: f64, unit_suffix: &str, locale: &str) -> String {
	let number_text = format_number(value, locale);
	format!("{}{}", number_text, unit_suffix)
}

//==============================================================================
/// 큰 수를 짧게 줄여 반환. (1234 -> "1.2K", 5600000 -> "5.6M", 원본 기본 fractionDigits 는 1)
/// - 반올림한 글이 1000 이 되면 Intl 의 compact 표기처럼 다음 단위로 올린다. (999999 -> "1M")
///
/// # 인자
/// - `value`: 표기할 수. (음수는 절대값을 줄여 앞에 - 를 붙인다)
/// - `fraction_digits`: 줄인 수의 소수 자릿수. (끝의 0 은 뺀다, 절대값이 1000 미만이면 쓰지 않는다)
///
/// # 돌려줌
/// 절대값이 1000 이상이면 K, 1000000 이상이면 M, 1000000000 이상이면 B 를 붙여 줄인 글. 1000 미만은 String(수) 꼴 그대로다.
/// (반올림은 toFixed 꼴이고, 반올림해 1000 이 되면 다음 단위로 올려 999999 는 "1M", NaN 은 "NaN")
///
/// # 예제
/// 단위 경계와 반올림
/// ```rust
/// use vanilla::format;
///
/// assert_eq!(format::format_compact_number(1234.0, 1), "1.2K");
/// assert_eq!(format::format_compact_number(1234567890.0, 2), "1.23B");
/// assert_eq!(format::format_compact_number(-1234.0, 1), "-1.2K");
/// assert_eq!(format::format_compact_number(999.5, 1), "999.5");
/// assert_eq!(format::format_compact_number(999999.0, 1), "1M");
/// ```
//==============================================================================
pub fn format_compact_number(value: f64, fraction_digits: usize) -> String {
	let absolute_value = value.abs();
	let sign_text = if value < 0.0 {
		"-"
	}
	else {
		""
	};
	let mut unit_index = match COMPACT_UNIT_LIST.iter().rposition(|(divisor, _)| absolute_value >= *divisor) {
		Some(unit_index) => {
			unit_index
		}
		None => {
			return format!("{}{}", sign_text, number_to_string(absolute_value));
		}
	};
	let mut compact_text = format_fixed(absolute_value / COMPACT_UNIT_LIST[unit_index].0, fraction_digits);
	let rounded_value = compact_text.parse::<f64>().unwrap_or(0.0);
	if rounded_value >= 1000.0 && unit_index < COMPACT_UNIT_LIST.len() - 1 {
		unit_index += 1;
		compact_text = format_fixed(absolute_value / COMPACT_UNIT_LIST[unit_index].0, fraction_digits);
	}
	format!("{}{}{}", sign_text, trim_trailing_zero(&compact_text), COMPACT_UNIT_LIST[unit_index].1)
}

//==============================================================================
/// 값을 범위로 자르고 자릿수를 0 으로 채워 반환. (7세그먼트 카운터 표기 등)
/// - format_padded_number(7.0, 3, ..) -> "007", format_padded_number(-5.0, 3, ..) -> "-05"
/// - 원본 기본 범위는 minimum_value = f64::NEG_INFINITY, maximum_value = f64::INFINITY
///
/// # 인자
/// - `value`: 표기할 수.
/// - `digit_count`: 전체 자릿수. (음수면 - 기호도 한 자리로 센다)
/// - `minimum_value`: 아래 끝. (끝 없이 쓰려면 f64::NEG_INFINITY)
/// - `maximum_value`: 위 끝. (끝 없이 쓰려면 f64::INFINITY)
///
/// # 돌려줌
/// 범위로 자르고 앞을 0 으로 채운 글. (자릿수보다 길면 자르지 않는다. 소수, NaN, 지수 꼴도 String(수) 글을 그대로 채운다, 값이나 끝이 NaN 이면 NaN)
///
/// # 예제
/// 세 자리 카운터
/// ```rust
/// use vanilla::format;
///
/// assert_eq!(format::format_padded_number(7.0, 3, f64::NEG_INFINITY, f64::INFINITY), "007");
/// assert_eq!(format::format_padded_number(-5.0, 3, f64::NEG_INFINITY, f64::INFINITY), "-05");
/// assert_eq!(format::format_padded_number(1234.0, 3, -99.0, 999.0), "999");
/// assert_eq!(format::format_padded_number(-1234.0, 3, -99.0, 999.0), "-99");
/// ```
//==============================================================================
pub fn format_padded_number(value: f64, digit_count: usize, minimum_value: f64, maximum_value: f64) -> String {
	let clamped_value = minimum_with_nan(maximum_value, maximum_with_nan(minimum_value, value));
	if clamped_value < 0.0 {
		let digit_text = pad_start_with_zero(&number_to_string(-clamped_value), digit_count.saturating_sub(1));
		return format!("-{}", digit_text);
	}
	pad_start_with_zero(&number_to_string(clamped_value), digit_count)
}

//==============================================================================
/// 초를 "분:초" 표기로 반환. (75 -> "1:15")
///
/// # 인자
/// - `total_seconds`: 전체 시간. (초, 소수는 버리고 음수는 0 으로 친다)
///
/// # 돌려줌
/// "분:초" 글. (초는 두 자리, 분은 60 을 넘어도 그대로. NaN 은 "NaN:NaN")
//==============================================================================
pub fn format_minutes_seconds(total_seconds: f64) -> String {
	let floored_seconds = maximum_with_nan(0.0, total_seconds.floor());
	let minutes = (floored_seconds / 60.0).floor();
	let seconds = floored_seconds % 60.0;
	format!("{}:{}", number_to_string(minutes), pad_start_with_zero(&number_to_string(seconds), 2))
}

//==============================================================================
/// 소수 표기의 꼬리 0 제거. ("1.0" -> "1")
//==============================================================================
fn trim_trailing_zero(number_text: &str) -> String {
	if !number_text.contains('.') {
		return String::from(number_text);
	}
	let trimmed_text = number_text.trim_end_matches('0');
	let trimmed_text = trimmed_text.strip_suffix('.').unwrap_or(trimmed_text);
	String::from(trimmed_text)
}

//==============================================================================
/// 언어 태그로 로캘 찾기. (표에 없으면 원본의 브라우저 기본 로캘 자리인 시스템 언어, 그것도 없으면 en-US)
//==============================================================================
fn find_number_locale(locale: &str) -> &'static NumberLocale {
	if let Some(number_locale) = find_table_number_locale(locale) {
		return number_locale;
	}
	let system_language_tag_list = crate::misc::localization::read_preferred_language_tags();
	if let Some(number_locale) = system_language_tag_list.first().and_then(|system_language_tag| find_table_number_locale(system_language_tag)) {
		return number_locale;
	}
	match NUMBER_LOCALES.iter().find(|number_locale| number_locale.tag == FALLBACK_LOCALE_TAG) {
		Some(number_locale) => {
			number_locale
		}
		None => {
			&NUMBER_LOCALES[0]
		}
	}
}

//==============================================================================
/// 언어 태그로 표에서 로캘 찾기. (뒤 조각부터 하나씩 떼며 찾고, 없으면 None)
//==============================================================================
fn find_table_number_locale(locale: &str) -> Option<&'static NumberLocale> {
	let normalized_locale = locale.replace('_', "-").to_lowercase();
	let mut subtag_list: Vec<&str> = Vec::new();
	for subtag in normalized_locale.split('-') {
		// 한 글자 조각(-u-, -x-)부터는 확장이라 보지 않는다.
		if subtag.len() <= 1 {
			break;
		}
		subtag_list.push(subtag);
	}
	while !subtag_list.is_empty() {
		let candidate_tag = subtag_list.join("-");
		if let Some(number_locale) = NUMBER_LOCALES.iter().find(|number_locale| number_locale.tag == candidate_tag) {
			return Some(number_locale);
		}
		subtag_list.pop();
	}
	None
}

//==============================================================================
/// 로캘 표기로 수 쓰기. (toLocaleString 기본 옵션: 소수 0 ~ 3 자리, 반 올림, 천 단위 묶음)
/// - 0 의 부호도 원본처럼 쓴다. (-0 과 반올림해 0 이 된 음수는 "-0")
//==============================================================================
fn format_locale_number(value: f64, number_locale: &NumberLocale) -> String {
	if value.is_nan() {
		return String::from(number_locale.not_a_number_text);
	}
	let sign_text = if value.is_sign_negative() {
		number_locale.minus_prefix
	}
	else {
		""
	};
	if value.is_infinite() {
		return format!("{}{}", sign_text, INFINITY_TEXT);
	}
	let (digit_text, point_position) = read_shortest_digits(value.abs());
	let (integer_text, fraction_text) = round_decimal_digits(&digit_text, point_position, LOCALE_MAXIMUM_FRACTION_DIGITS);
	let fraction_text = fraction_text.trim_end_matches('0');
	let digit_list: Vec<char> = number_locale.digits.chars().collect();
	let mut result_text = String::from(sign_text);
	result_text.push_str(&localize_digits(&group_integer_digits(&integer_text, number_locale), &digit_list));
	if !fraction_text.is_empty() {
		result_text.push_str(number_locale.decimal_separator);
		result_text.push_str(&localize_digits(fraction_text, &digit_list));
	}
	result_text
}

//==============================================================================
/// 정수 자리에 천 단위 기호 넣기. (맨 아래 3 자리, 그 위는 로캘의 묶음 길이, 자리가 모자라면 묶지 않는다)
//==============================================================================
fn group_integer_digits(integer_text: &str, number_locale: &NumberLocale) -> String {
	let digit_count = integer_text.len();
	if digit_count < PRIMARY_GROUP_SIZE + number_locale.minimum_grouping_digits {
		return String::from(integer_text);
	}
	let mut group_list: Vec<&str> = Vec::new();
	let mut end_index = digit_count;
	let mut group_size = PRIMARY_GROUP_SIZE;
	while end_index > 0 {
		let start_index = end_index.saturating_sub(group_size);
		group_list.push(&integer_text[start_index..end_index]);
		end_index = start_index;
		group_size = number_locale.secondary_group_size;
	}
	group_list.reverse();
	group_list.join(number_locale.group_separator)
}

//==============================================================================
/// 라틴 숫자를 로캘의 숫자 모양으로 바꾸기. (숫자가 아닌 글자는 그대로)
//==============================================================================
fn localize_digits(text: &str, digit_list: &[char]) -> String {
	text.chars().map(|character| {
		match character.to_digit(10) {
			Some(digit) => {
				digit_list[digit as usize]
			}
			None => {
				character
			}
		}
	}).collect()
}

//==============================================================================
/// 가장 짧은 십진 표기의 숫자들과 소수점 자리. (value = 0.숫자들 × 10^소수점 자리, 0 이상의 유한한 값)
/// - 같은 길이의 후보 둘이 값에서 딱 같은 거리면 원본(ECMAScript Number::toString, ICU)처럼 끝자리가 짝수인 쪽을 쓴다.
///   러스트의 {:e} 는 이때 짝수를 고르지 않아 고쳐 쓴다. (224567413330.078125 → 원본 ...07812, 러스트 ...07813)
//==============================================================================
fn read_shortest_digits(value: f64) -> (String, i32) {
	if value == 0.0 {
		return (String::from("0"), 1);
	}
	let exponent_text = format!("{:e}", value);
	let (mantissa_text, exponent_part) = exponent_text.split_once('e').unwrap_or((exponent_text.as_str(), "0"));
	let digit_text = mantissa_text.replace('.', "");
	let exponent = exponent_part.parse::<i32>().unwrap_or(0);
	let digit_text = choose_even_tie_digits(value, digit_text, exponent);
	(digit_text, exponent + 1)
}

//==============================================================================
/// 가장 짧은 표기의 끝자리가 홀수이고 값이 딱 그 옆 짝수 후보와의 가운데면 짝수 후보로 바꾼다.
/// - 딱 가운데인 값은 정확한 십진 전개가 숫자들보다 한 자리 길고 5 로 끝난다. 이진 소수 자릿수가 그 십진 소수 자릿수라,
///   유효 숫자 18 자리 안에 들 수 있는 이진 소수 25 자리 이하만 정확한 전개를 만들어 본다.
//==============================================================================
fn choose_even_tie_digits(value: f64, digit_text: String, exponent: i32) -> String {
	let binary_fraction_digit_count = count_binary_fraction_digits(value);
	if binary_fraction_digit_count == 0 || binary_fraction_digit_count > MAXIMUM_TIE_BINARY_FRACTION_DIGITS {
		return digit_text;
	}
	let exact_text = format!("{:.*}", binary_fraction_digit_count as usize, value);
	let exact_digit_text = exact_text.replace('.', "");
	let exact_digit_text = exact_digit_text.trim_start_matches('0');
	let digit_bytes = digit_text.as_bytes();
	let last_digit = digit_bytes[digit_bytes.len() - 1];
	if (last_digit - b'0') % 2 == 0 || exact_digit_text.len() != digit_bytes.len() + 1 || !exact_digit_text.ends_with('5') {
		return digit_text;
	}
	let prefix_text = &digit_text[..digit_text.len() - 1];
	let lower_text = format!("{}{}", prefix_text, (last_digit - 1) as char);
	let upper_text = format!("{}{}", prefix_text, (last_digit + 1) as char);
	let candidate_text = if exact_digit_text == format!("{}5", lower_text) {
		lower_text
	}
	else if last_digit < b'9' && exact_digit_text == format!("{}5", digit_text) {
		upper_text
	}
	else {
		return digit_text;
	};
	// 고른 후보도 같은 값으로 읽혀야 쓴다.
	let candidate_value = format!("0.{}e{}", candidate_text, exponent + 1).parse::<f64>().unwrap_or(f64::NAN);
	if candidate_value != value {
		return digit_text;
	}
	candidate_text
}

//==============================================================================
/// 십진 숫자들을 소수 fraction_digit_count 자리로 반 올림. (정수 숫자들, 소수 숫자들을 돌려준다, 소수는 늘 그 자릿수)
/// - point_position 은 소수점 앞 숫자 수다. (0 이하면 소수점 뒤에 그만큼 0 이 먼저 온다)
//==============================================================================
fn round_decimal_digits(digit_text: &str, point_position: i32, fraction_digit_count: usize) -> (String, String) {
	let digit_bytes = digit_text.as_bytes();
	let keep_count = point_position + fraction_digit_count as i32;
	let mut point_position = point_position;
	let mut kept_digits: Vec<u8> = Vec::new();
	if keep_count < 0 {
		return (String::from("0"), "0".repeat(fraction_digit_count));
	}
	let keep_count = keep_count as usize;
	if keep_count >= digit_bytes.len() {
		kept_digits.extend_from_slice(digit_bytes);
		kept_digits.resize(keep_count, b'0');
	}
	else {
		kept_digits.extend_from_slice(&digit_bytes[..keep_count]);
		if digit_bytes[keep_count] >= b'5' {
			let mut digit_index = kept_digits.len();
			let mut is_carried = true;
			while is_carried && digit_index > 0 {
				digit_index -= 1;
				if kept_digits[digit_index] == b'9' {
					kept_digits[digit_index] = b'0';
				}
				else {
					kept_digits[digit_index] += 1;
					is_carried = false;
				}
			}
			if is_carried {
				kept_digits.insert(0, b'1');
				point_position += 1;
			}
		}
	}
	let kept_text = String::from_utf8_lossy(&kept_digits).into_owned();
	if point_position <= 0 {
		let fraction_text = format!("{}{}", "0".repeat((-point_position) as usize), kept_text);
		return (String::from("0"), fraction_text);
	}
	let point_index = point_position as usize;
	let integer_text = kept_text[..point_index].trim_start_matches('0');
	let integer_text = if integer_text.is_empty() {
		"0"
	}
	else {
		integer_text
	};
	(String::from(integer_text), String::from(&kept_text[point_index..]))
}

//==============================================================================
/// 원본 Number.prototype.toFixed. (실제 값으로 반올림하고 딱 반이면 크기를 올린다, 1e21 이상과 무한대는 String(수))
//==============================================================================
pub(crate) fn format_fixed(value: f64, fraction_digit_count: usize) -> String {
	if value.is_nan() {
		return String::from(NOT_A_NUMBER_TEXT);
	}
	if value.abs() >= 1e21 {
		return number_to_string(value);
	}
	let sign_text = if value < 0.0 {
		"-"
	}
	else {
		""
	};
	let absolute_value = value.abs();
	// 러스트의 {:.N} 은 딱 반일 때 짝수로 가서, 딱 반일 때만 한 자리 더 쓴 정확한 값을 올린다.
	if count_binary_fraction_digits(absolute_value) != fraction_digit_count as u32 + 1 {
		return format!("{}{:.*}", sign_text, fraction_digit_count, absolute_value);
	}
	let exact_text = format!("{:.*}", fraction_digit_count + 1, absolute_value);
	let (integer_part, fraction_part) = exact_text.split_once('.').unwrap_or((exact_text.as_str(), ""));
	let digit_text = format!("{}{}", integer_part, fraction_part);
	let (integer_text, fraction_text) = round_decimal_digits(&digit_text, integer_part.len() as i32, fraction_digit_count);
	if fraction_text.is_empty() {
		return format!("{}{}", sign_text, integer_text);
	}
	format!("{}{}.{}", sign_text, integer_text, fraction_text)
}

//==============================================================================
/// 이진 소수 자릿수. (값 = 홀수 / 2^자릿수, 정수면 0. 정확한 십진 전개의 소수 자릿수와 같고 끝자리는 5 다)
//==============================================================================
fn count_binary_fraction_digits(value: f64) -> u32 {
	if value == 0.0 || !value.is_finite() {
		return 0;
	}
	let bits = value.to_bits();
	let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
	let mantissa_bits = bits & 0x000f_ffff_ffff_ffff;
	let (mantissa, exponent) = if exponent_bits == 0 {
		(mantissa_bits, -1074)
	}
	else {
		(mantissa_bits | (1u64 << 52), exponent_bits - 1075)
	};
	let reduced_exponent = exponent + mantissa.trailing_zeros() as i32;
	if reduced_exponent < 0 {
		(-reduced_exponent) as u32
	}
	else {
		0
	}
}

//==============================================================================
/// 원본 String(수). (Number.prototype.toString: 가장 짧은 표기, 1e21 이상과 1e-7 이하는 지수, -0 은 "0")
//==============================================================================
pub(crate) fn number_to_string(value: f64) -> String {
	if value.is_nan() {
		return String::from(NOT_A_NUMBER_TEXT);
	}
	if value == 0.0 {
		return String::from("0");
	}
	if value < 0.0 {
		return format!("-{}", number_to_string(-value));
	}
	if value.is_infinite() {
		return String::from("Infinity");
	}
	let (digit_text, point_position) = read_shortest_digits(value);
	let digit_count = digit_text.len() as i32;
	if digit_count <= point_position && point_position <= 21 {
		return format!("{}{}", digit_text, "0".repeat((point_position - digit_count) as usize));
	}
	if 0 < point_position && point_position <= 21 {
		let point_index = point_position as usize;
		return format!("{}.{}", &digit_text[..point_index], &digit_text[point_index..]);
	}
	if -6 < point_position && point_position <= 0 {
		return format!("0.{}{}", "0".repeat((-point_position) as usize), digit_text);
	}
	let exponent = point_position - 1;
	let exponent_sign = if exponent < 0 {
		"-"
	}
	else {
		"+"
	};
	if digit_count == 1 {
		return format!("{}e{}{}", digit_text, exponent_sign, exponent.abs());
	}
	format!("{}.{}e{}{}", &digit_text[..1], &digit_text[1..], exponent_sign, exponent.abs())
}

//==============================================================================
/// 앞을 0 으로 채우기. (원본 padStart(길이, "0"))
//==============================================================================
fn pad_start_with_zero(text: &str, target_length: usize) -> String {
	let character_count = text.chars().count();
	if character_count >= target_length {
		return String::from(text);
	}
	format!("{}{}", "0".repeat(target_length - character_count), text)
}

//==============================================================================
/// 원본 Math.max. (하나라도 NaN 이면 NaN)
//==============================================================================
fn maximum_with_nan(first_value: f64, second_value: f64) -> f64 {
	if first_value.is_nan() || second_value.is_nan() {
		return f64::NAN;
	}
	first_value.max(second_value)
}

//==============================================================================
/// 원본 Math.min. (하나라도 NaN 이면 NaN)
//==============================================================================
fn minimum_with_nan(first_value: f64, second_value: f64) -> f64 {
	if first_value.is_nan() || second_value.is_nan() {
		return f64::NAN;
	}
	first_value.min(second_value)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// format_number: 반올림, 음수, -0, 큰 수, NaN, 무한대, 로캘마다의 기호와 묶음과 숫자 모양. (기대값은 원본 format.js 를 node 25 로 돌린 것)
	//==============================================================================
	#[test]
	fn format_number_matches_original() {
		let case_list: [(&str, f64, &str); 43] = [
			("en-US", 12480.0, "12,480"),
			("en-US", 1234.5678, "1,234.568"),
			("en-US", -1234.5678, "-1,234.568"),
			("en-US", 0.0005, "0.001"),
			("en-US", 0.0004999, "0"),
			("en-US", -0.0001, "-0"),
			("en-US", -0.0, "-0"),
			("en-US", 999.9995, "1,000"),
			("en-US", 1e21, "1,000,000,000,000,000,000,000"),
			("en-US", 1.2345678901234566e25, "12,345,678,901,234,566,000,000,000"),
			("en-US", f64::NAN, "NaN"),
			("en-US", f64::INFINITY, "\u{221e}"),
			("en-US", f64::NEG_INFINITY, "-\u{221e}"),
			("en-US", 0.30000000000000004, "0.3"),
			("en-US", 1.0005, "1.001"),
			("ko-KR", 1234567.891, "1,234,567.891"),
			("de-DE", 1234567.891, "1.234.567,891"),
			("es-ES", 1234.5, "1234,5"),
			("es-ES", 12345.5, "12.345,5"),
			("es-ES", 1234567.0, "1.234.567"),
			("pl-PL", 1234.0, "1234"),
			("pl-PL", 12345.0, "12\u{a0}345"),
			("fr-FR", -1234567.891, "-1\u{202f}234\u{202f}567,891"),
			("de-CH", 1234567.891, "1'234'567.891"),
			("hi-IN", 123456789.123, "12,34,56,789.123"),
			("en-IN", 123456789.123, "12,34,56,789.123"),
			("ar-EG", -1234567.891, "\u{61c}-\u{661}\u{66c}\u{662}\u{663}\u{664}\u{66c}\u{665}\u{666}\u{667}\u{66b}\u{668}\u{669}\u{661}"),
			("ar-SA", -1234567.891, "\u{61c}-\u{661}\u{66c}\u{662}\u{663}\u{664}\u{66c}\u{665}\u{666}\u{667}\u{66b}\u{668}\u{669}\u{661}"),
			("ar", -1234567.891, "\u{200e}-1,234,567.891"),
			("fa-IR", -1234567.891, "\u{200e}\u{2212}\u{6f1}\u{66c}\u{6f2}\u{6f3}\u{6f4}\u{66c}\u{6f5}\u{6f6}\u{6f7}\u{66b}\u{6f8}\u{6f9}\u{6f1}"),
			("sv-SE", -1234567.891, "\u{2212}1\u{a0}234\u{a0}567,891"),
			("bn-BD", 1234567.891, "\u{9e7}\u{9e8},\u{9e9}\u{9ea},\u{9eb}\u{9ec}\u{9ed}.\u{9ee}\u{9ef}\u{9e7}"),
			("ru-RU", f64::NAN, "\u{43d}\u{435}\u{a0}\u{447}\u{438}\u{441}\u{43b}\u{43e}"),
			("zh-TW", f64::NAN, "\u{975e}\u{6578}\u{503c}"),
			("pt-PT", 1234.5, "1234,5"),
			("pt-BR", 1234.5, "1.234,5"),
			("zh-Hant-TW", 1234567.891, "1,234,567.891"),
			("ja-JP", 1234567.891, "1,234,567.891"),
			("ps", -1234567.891, "\u{200e}-\u{200e}\u{6f1}\u{66c}\u{6f2}\u{6f3}\u{6f4}\u{66c}\u{6f5}\u{6f6}\u{6f7}\u{66b}\u{6f8}\u{6f9}\u{6f1}"),
			("it-CH", 1234567.891, "1'234'567.891"),
			("et-EE", -1234.5, "\u{2212}1234,5"),
			("en-u-nu-latn", 1234567.891, "1,234,567.891"),
			("EN_gb", 1234567.891, "1,234,567.891"),
		];
		for (locale, value, expected_text) in case_list {
			assert_eq!(format_number(value, locale), expected_text, "{} {}", locale, value);
		}
	}

	//==============================================================================
	/// 표에 없는 태그는 원본의 브라우저 기본 로캘처럼 시스템 언어(첫째)로, 그것도 표에 없으면 en-US 로 쓴다.
	//==============================================================================
	#[test]
	fn unknown_locale_uses_system_language() {
		assert!(find_table_number_locale("xx-YY").is_none());
		let system_language_tag_list = crate::misc::localization::read_preferred_language_tags();
		let expected_locale = system_language_tag_list.first().and_then(|system_language_tag| find_table_number_locale(system_language_tag)).map(|number_locale| number_locale.tag).unwrap_or(FALLBACK_LOCALE_TAG);
		assert_eq!(find_number_locale("xx-YY").tag, expected_locale);
		assert_eq!(format_number(1234567.891, "xx-YY"), format_number(1234567.891, expected_locale));
	}

	//==============================================================================
	/// format_number_with_unit: 로캘 표기 뒤에 단위.
	//==============================================================================
	#[test]
	fn format_number_with_unit_matches_original() {
		assert_eq!(format_number_with_unit(1200000.0, "\u{c6d0}", "ko-KR"), "1,200,000\u{c6d0}");
		assert_eq!(format_number_with_unit(-5.0, "\u{c810}", "ko-KR"), "-5\u{c810}");
		assert_eq!(format_number_with_unit(1234.5678, "G", "en-US"), "1,234.568G");
		assert_eq!(format_number_with_unit(1234.5, "\u{20ac}", "de-DE"), "1.234,5\u{20ac}");
	}

	//==============================================================================
	/// format_compact_number: 단위 경계, toFixed 의 딱 반 올림(1250 → 1.3K)과 실제 값 반올림(1005 → 1K), 반올림해 1000 이 되면 다음 단위(999999 → 1M), 음수, 지수 꼴.
	//==============================================================================
	#[test]
	fn format_compact_number_matches_original() {
		let case_list: [(f64, usize, &str); 35] = [
			(1234.0, 1, "1.2K"),
			(5600000.0, 1, "5.6M"),
			(999.0, 1, "999"),
			(999.5, 1, "999.5"),
			(1000.0, 1, "1K"),
			(1250.0, 1, "1.3K"),
			(1350.0, 1, "1.4K"),
			(1050.0, 1, "1.1K"),
			(1000000.0, 1, "1M"),
			(999999.0, 1, "1M"),
			(999950.0, 1, "1M"),
			(999949.0, 1, "999.9K"),
			(999999999.0, 1, "1B"),
			(-999999.0, 1, "-1M"),
			(999500.0, 0, "1M"),
			(999499.0, 0, "999K"),
			(-1234.0, 1, "-1.2K"),
			(-0.0, 1, "0"),
			(0.0, 1, "0"),
			(1234567890.0, 2, "1.23B"),
			(1e30, 1, "1e+21B"),
			(f64::INFINITY, 1, "InfinityB"),
			(f64::NAN, 1, "NaN"),
			(f64::NEG_INFINITY, 1, "-InfinityB"),
			(0.1, 1, "0.1"),
			(1e-7, 1, "1e-7"),
			(2500.0, 0, "3K"),
			(3500.0, 0, "4K"),
			(1234.0, 3, "1.234K"),
			(1e21, 1, "1000000000000B"),
			(123.456, 1, "123.456"),
			(1999.95, 1, "2K"),
			(1500000000000000.0, 1, "1500000B"),
			(1005.0, 2, "1K"),
			(1015.0, 2, "1.01K"),
		];
		for (value, fraction_digits, expected_text) in case_list {
			assert_eq!(format_compact_number(value, fraction_digits), expected_text, "{} {}", value, fraction_digits);
		}
	}

	//==============================================================================
	/// format_padded_number: 범위 자르기, 음수는 부호 몫 한 자리를 뺀다, 소수와 NaN 과 지수 꼴은 String(수) 그대로.
	//==============================================================================
	#[test]
	fn format_padded_number_matches_original() {
		let case_list: [(f64, usize, f64, f64, &str); 15] = [
			(7.0, 3, f64::NEG_INFINITY, f64::INFINITY, "007"),
			(-5.0, 3, f64::NEG_INFINITY, f64::INFINITY, "-05"),
			(1234.0, 3, f64::NEG_INFINITY, f64::INFINITY, "1234"),
			(1234.0, 3, -99.0, 999.0, "999"),
			(-1234.0, 3, -99.0, 999.0, "-99"),
			(7.5, 4, f64::NEG_INFINITY, f64::INFINITY, "07.5"),
			(-0.0, 3, f64::NEG_INFINITY, f64::INFINITY, "000"),
			(f64::NAN, 3, f64::NEG_INFINITY, f64::INFINITY, "NaN"),
			(5.0, 0, f64::NEG_INFINITY, f64::INFINITY, "5"),
			(-5.0, 1, f64::NEG_INFINITY, f64::INFINITY, "-5"),
			(-5.0, 0, f64::NEG_INFINITY, f64::INFINITY, "-5"),
			(1e21, 3, f64::NEG_INFINITY, f64::INFINITY, "1e+21"),
			(42.0, 5, 0.0, 100.0, "00042"),
			(0.000001, 2, f64::NEG_INFINITY, f64::INFINITY, "0.000001"),
			(1e-7, 2, f64::NEG_INFINITY, f64::INFINITY, "1e-7"),
		];
		for (value, digit_count, minimum_value, maximum_value, expected_text) in case_list {
			assert_eq!(format_padded_number(value, digit_count, minimum_value, maximum_value), expected_text, "{} {}", value, digit_count);
		}
	}

	//==============================================================================
	/// format_minutes_seconds: 내림, 음수는 0, NaN 과 무한대, 큰 분.
	//==============================================================================
	#[test]
	fn format_minutes_seconds_matches_original() {
		let case_list: [(f64, &str); 12] = [
			(75.0, "1:15"),
			(0.0, "0:00"),
			(59.9, "0:59"),
			(60.0, "1:00"),
			(3599.0, "59:59"),
			(3600.0, "60:00"),
			(-5.0, "0:00"),
			(f64::NAN, "NaN:NaN"),
			(f64::INFINITY, "Infinity:NaN"),
			(125.7, "2:05"),
			(1e21, "16666666666666666000:40"),
			(600.0, "10:00"),
		];
		for (total_seconds, expected_text) in case_list {
			assert_eq!(format_minutes_seconds(total_seconds), expected_text, "{}", total_seconds);
		}
	}

	//==============================================================================
	/// 가장 짧은 표기가 딱 가운데면 원본처럼 끝자리가 짝수인 쪽. (224567413330.078125 → ...07812)
	//==============================================================================
	#[test]
	fn shortest_digits_choose_even_on_tie() {
		assert_eq!(format_padded_number(-224567413330.078125, 6, f64::NEG_INFINITY, f64::INFINITY), "-224567413330.07812");
		assert_eq!(format_padded_number(31122207641.6015625, 6, f64::NEG_INFINITY, f64::INFINITY), "31122207641.601562");
		assert_eq!(format_number(-1083787679672241.25, "it"), "-1.083.787.679.672.241,2");
		assert_eq!(format_number(-281161069869995.125, "hr"), "\u{2212}281.161.069.869.995,12");
	}
}
