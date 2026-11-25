//==============================================================================
// 데스크탑 시스템 글꼴 찾기. (글꼴 이름의 파일, 글꼴에 없는 글자를 그릴 대체 글꼴)
// - 원본에 없는 파일이다. 원본은 브라우저가 CSS 글꼴 이름을 운영체제 글꼴로 찾고, 글꼴 목록에 없는 글자는 운영체제의 대체 글꼴로 그린다.
//   크롬은 맥에서 CoreText, 윈도우에서 DirectWrite, 리눅스에서 fontconfig 로 찾는다. 러스트 판 데스크탑도 같은 API 로 파일을 찾아 글자 캐시에 올린다.
// - find_family_font_file: 글꼴 이름, 굵기, 기울기로 파일과 얼굴을 찾는다. 그 이름의 글꼴이 없으면 None 이다. (CSS 처럼 다른 글꼴로 바꿔 주지 않는다)
//   - 맥: -apple-system, BlinkMacSystemFont, system-ui 는 시스템 UI 글꼴(CTFontCreateUIFontForLanguage), 그 밖은 이름과 굵기 특성으로 지은 글꼴 서술자.
//   - 윈도우: system-ui 는 Segoe UI, 그 밖은 시스템 글꼴 모음의 이름(FindFamilyName)과 GetFirstMatchingFont.
//   - 리눅스: fontconfig 의 FcFontMatch. 일반 이름(sans-serif 등)은 fontconfig 설정이 고른다.
//   - 일반 이름은 크롬의 기본 글꼴과 같게 맥 sans-serif Helvetica, serif Times, monospace Courier, 윈도우 sans-serif Arial, serif Times New Roman, monospace Consolas 다.
// - find_fallback_font_file: 글(글자 묶음 하나)을 그릴 대체 글꼴을 운영체제가 고르게 한다.
//   - 맥: 시스템 UI 글꼴에서 CTFontCreateForString. (크롬 맥의 글꼴 대체와 같은 API) 색 이모지를 바라면 Apple Color Emoji 를 먼저 본다.
//   - 윈도우: IDWriteFontFallback.MapCharacters (기본 이름 Segoe UI, 사용자 언어)
//   - 리눅스: 글자를 담은 charset 으로 FcFontSort 해 그 글자를 가진 첫 글꼴. 색 이모지를 바라면 color 속성을 건다.
// - 돌려주는 FontFileSource 의 얼굴 번호는 운영체제가 알려 주면(윈도우, 리눅스) 넣고, 맥은 PostScript 이름을 넣어 글꼴을 읽을 때 맞는 얼굴을 찾는다.
//   가변 글꼴(맥 시스템 UI 글꼴 SFNS 같은)의 굵기는 글자 캐시가 축 값으로 만든다.
//==============================================================================
use crate::core::textstringtexturecache::FontFileSource;

//==============================================================================
// 글꼴 이름으로 파일 찾기. (CSS 글꼴 이름 하나, 일반 이름 포함. 없으면 None)
//==============================================================================
pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
	let family_name = family_name.trim();
	if family_name.is_empty() {
		return None;
	}
	platform_font::find_family_font_file(family_name, weight, is_italic)
}

//==============================================================================
// 글을 그릴 대체 글꼴 파일 찾기. (운영체제가 고른다, is_color_wanted 면 색 이모지 글꼴을 먼저)
//==============================================================================
pub fn find_fallback_font_file(text: &str, weight: u16, is_italic: bool, is_color_wanted: bool) -> Option<FontFileSource> {
	if text.is_empty() {
		return None;
	}
	platform_font::find_fallback_font_file(text, weight, is_italic, is_color_wanted)
}

//==============================================================================
// 맥: CoreText.
//==============================================================================
#[cfg(target_os = "macos")]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;
	use objc2_core_foundation::{CFDictionary, CFNumber, CFRange, CFRetained, CFString, CFType, CFURL};
	use objc2_core_text::{kCTFontFamilyNameAttribute, kCTFontSymbolicTrait, kCTFontTraitsAttribute, kCTFontURLAttribute, kCTFontWeightTrait, CTFont, CTFontDescriptor, CTFontSymbolicTraits, CTFontUIFontType};

	// 글꼴을 지을 때의 크기. (파일만 읽으므로 아무 크기나 된다)
	const LOOKUP_FONT_SIZE: f64 = 16.0;

	//==============================================================================
	// CSS 굵기 → CoreText 굵기 특성. (AppKit NSFontWeight 값, 사이는 곧게 잇는다)
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
	// 글꼴의 파일과 PostScript 이름.
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
	// 시스템 UI 글꼴. (굵기가 600 이상이면 굵은 시스템 글꼴)
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
	// 이름, 굵기, 기울기로 글꼴 짓기. (그 이름의 글꼴이 없으면 None)
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
		let trait_key_list: [&CFString; 2] = unsafe { [kCTFontWeightTrait, kCTFontSymbolicTrait] };
		let trait_value_list: [&CFType; 2] = [&weight_number, &symbolic_number];
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
	// 글꼴 이름으로 파일 찾기.
	//==============================================================================
	pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
		let lowercase_family_name = family_name.to_lowercase();
		let font = match lowercase_family_name.as_str() {
			"-apple-system" | "blinkmacsystemfont" | "system-ui" | "ui-sans-serif" => {
				create_system_ui_font(weight)?
			}
			"sans-serif" => {
				create_family_font("Helvetica", weight, is_italic)?
			}
			"serif" | "ui-serif" => {
				create_family_font("Times", weight, is_italic)?
			}
			"monospace" | "ui-monospace" => {
				create_family_font("Courier", weight, is_italic)?
			}
			"cursive" => {
				create_family_font("Apple Chancery", weight, is_italic)?
			}
			"fantasy" => {
				create_family_font("Papyrus", weight, is_italic)?
			}
			_ => {
				create_family_font(family_name, weight, is_italic)?
			}
		};
		read_font_file(&font)
	}

	//==============================================================================
	// 대체 글꼴 파일 찾기. (시스템 UI 글꼴에서 CTFontCreateForString)
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
// 윈도우: DirectWrite.
//==============================================================================
#[cfg(target_os = "windows")]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;
	use dwrote::{FontCollection, FontFallback, FontStretch, FontStyle, FontWeight, TextAnalysisSource, TextAnalysisSourceMethods};
	use std::borrow::Cow;
	use winapi::um::dwrite::{DWRITE_READING_DIRECTION, DWRITE_READING_DIRECTION_LEFT_TO_RIGHT};

	// 대체 글꼴을 고를 때의 기본 이름. (크롬 윈도우의 시스템 글꼴)
	const FALLBACK_BASE_FAMILY: &str = "Segoe UI";

	//==============================================================================
	// 대체 글꼴 고르기에 줄 글의 언어와 방향.
	//==============================================================================
	struct FallbackTextSource {
		locale_name: String,
	}

	impl TextAnalysisSourceMethods for FallbackTextSource {
		//==============================================================================
		// 글 전체의 언어. (사용자 기본 언어)
		//==============================================================================
		fn get_locale_name(&self, _text_position: u32) -> (Cow<'_, str>, u32) {
			(Cow::Borrowed(self.locale_name.as_str()), u32::MAX)
		}

		//==============================================================================
		// 문단 방향. (왼쪽에서 오른쪽, 글자 고르기에는 쓰이지 않는다)
		//==============================================================================
		fn get_paragraph_reading_direction(&self) -> DWRITE_READING_DIRECTION {
			DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
		}
	}

	//==============================================================================
	// 사용자 기본 언어 이름. ("ko-KR" 같은, 읽지 못하면 "en-US")
	//==============================================================================
	fn read_user_locale_name() -> String {
		let mut locale_buffer = [0u16; 85];
		let length = unsafe { winapi::um::winnls::GetUserDefaultLocaleName(locale_buffer.as_mut_ptr(), locale_buffer.len() as i32) };
		if length <= 1 {
			return String::from("en-US");
		}
		String::from_utf16_lossy(&locale_buffer[..(length - 1) as usize])
	}

	//==============================================================================
	// 글꼴의 파일과 얼굴 번호.
	//==============================================================================
	fn read_font_file(font: &dwrote::Font) -> Option<FontFileSource> {
		let font_face = font.create_font_face();
		let font_file = font_face.files().ok()?.into_iter().next()?;
		let path = font_file.font_file_path().ok()?;
		Some(FontFileSource {
			path,
			face_index: Some(font_face.get_index()),
			post_script_name: None,
		})
	}

	//==============================================================================
	// 굵기와 기울기를 DirectWrite 값으로.
	//==============================================================================
	fn convert_font_style(weight: u16, is_italic: bool) -> (FontWeight, FontStyle) {
		let font_style = if is_italic {
			FontStyle::Italic
		}
		else {
			FontStyle::Normal
		};
		(FontWeight::from_u32(weight as u32), font_style)
	}

	//==============================================================================
	// 글꼴 이름으로 파일 찾기.
	//==============================================================================
	pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
		let lowercase_family_name = family_name.to_lowercase();
		let system_family_name = match lowercase_family_name.as_str() {
			"system-ui" | "ui-sans-serif" => {
				"Segoe UI"
			}
			"sans-serif" => {
				"Arial"
			}
			"serif" | "ui-serif" => {
				"Times New Roman"
			}
			"monospace" | "ui-monospace" => {
				"Consolas"
			}
			"cursive" => {
				"Comic Sans MS"
			}
			"fantasy" => {
				"Impact"
			}
			"-apple-system" | "blinkmacsystemfont" => {
				return None;
			}
			_ => {
				family_name
			}
		};
		let font_collection = FontCollection::system();
		let font_family = font_collection.font_family_by_name(system_family_name).ok()??;
		let (font_weight, font_style) = convert_font_style(weight, is_italic);
		let font = font_family.first_matching_font(font_weight, FontStretch::Normal, font_style).ok()?;
		read_font_file(&font)
	}

	//==============================================================================
	// 대체 글꼴 파일 찾기. (IDWriteFontFallback.MapCharacters)
	//==============================================================================
	pub fn find_fallback_font_file(text: &str, weight: u16, is_italic: bool, is_color_wanted: bool) -> Option<FontFileSource> {
		if is_color_wanted {
			if let Some(font_file_source) = find_family_font_file("Segoe UI Emoji", 400, false) {
				return Some(font_file_source);
			}
		}
		let font_fallback = FontFallback::get_system_fallback()?;
		let text_units: Vec<u16> = text.encode_utf16().collect();
		let text_length = text_units.len() as u32;
		let fallback_text_source = FallbackTextSource {
			locale_name: read_user_locale_name(),
		};
		let text_analysis_source = TextAnalysisSource::from_text(Box::new(fallback_text_source), Cow::Owned(text_units));
		let font_collection = FontCollection::system();
		let (font_weight, font_style) = convert_font_style(weight, is_italic);
		let fallback_result = font_fallback.map_characters(&text_analysis_source, 0, text_length, &font_collection, Some(FALLBACK_BASE_FAMILY), font_weight, font_style, FontStretch::Normal);
		read_font_file(fallback_result.mapped_font.as_ref()?)
	}
}

//==============================================================================
// 리눅스와 그 밖의 유닉스: fontconfig. (라이브러리를 실행할 때 열고, 없으면 아무것도 찾지 않는다)
//==============================================================================
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;
	use fontconfig_sys::constants::{FC_CHARSET, FC_COLOR, FC_FAMILY, FC_FILE, FC_INDEX, FC_SLANT, FC_SLANT_ITALIC, FC_SLANT_ROMAN, FC_WEIGHT};
	use fontconfig_sys::{Fc, FcMatchPattern, FcPattern, FcResultMatch};
	use std::ffi::{CStr, CString};

	//==============================================================================
	// fontconfig 라이브러리. (처음 쓸 때 연다, 없으면 None)
	//==============================================================================
	fn read_library() -> Option<&'static Fc> {
		fontconfig_sys::statics::LIB_RESULT.as_ref().ok()
	}

	//==============================================================================
	// CSS 굵기 → fontconfig 굵기. (OpenType 굵기 표와 같은 점들, 사이는 곧게 잇는다)
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
	// 패턴의 파일과 얼굴 번호.
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
	// 굵기와 기울기를 담은 패턴. (찾고 나면 FcPatternDestroy)
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
			(library.FcPatternAddString)(pattern, FC_FAMILY.as_ptr(), family_text.as_ptr() as *const u8);
			(library.FcPatternAddInteger)(pattern, FC_WEIGHT.as_ptr(), convert_weight(weight));
			(library.FcPatternAddInteger)(pattern, FC_SLANT.as_ptr(), slant);
		}
		Some(pattern)
	}

	//==============================================================================
	// 패턴에 설정과 기본값 채우기.
	//==============================================================================
	fn substitute_pattern(library: &Fc, pattern: *mut FcPattern) {
		unsafe {
			(library.FcConfigSubstitute)(std::ptr::null_mut(), pattern, FcMatchPattern);
			(library.FcDefaultSubstitute)(pattern);
		}
	}

	//==============================================================================
	// 글꼴 이름으로 파일 찾기. (일반 이름이 아니면 찾은 글꼴의 이름이 맞을 때만)
	//==============================================================================
	pub fn find_family_font_file(family_name: &str, weight: u16, is_italic: bool) -> Option<FontFileSource> {
		let library = read_library()?;
		let lowercase_family_name = family_name.to_lowercase();
		let lookup_family_name = match lowercase_family_name.as_str() {
			"system-ui" | "ui-sans-serif" => {
				"sans-serif"
			}
			"ui-serif" => {
				"serif"
			}
			"ui-monospace" => {
				"monospace"
			}
			"-apple-system" | "blinkmacsystemfont" => {
				return None;
			}
			_ => {
				family_name
			}
		};
		let is_generic = matches!(lookup_family_name.to_lowercase().as_str(), "sans-serif" | "serif" | "monospace" | "cursive" | "fantasy" | "emoji" | "math");
		let pattern = create_pattern(library, lookup_family_name, weight, is_italic)?;
		substitute_pattern(library, pattern);
		let mut match_result = 0;
		let matched_pattern = unsafe { (library.FcFontMatch)(std::ptr::null_mut(), pattern, &mut match_result) };
		unsafe { (library.FcPatternDestroy)(pattern) };
		if matched_pattern.is_null() {
			return None;
		}
		let mut is_family_matched = is_generic;
		let mut family_index = 0;
		while !is_family_matched {
			let mut found_family_text: *mut u8 = std::ptr::null_mut();
			let family_result = unsafe { (library.FcPatternGetString)(matched_pattern, FC_FAMILY.as_ptr(), family_index, &mut found_family_text) };
			if family_result != FcResultMatch || found_family_text.is_null() {
				break;
			}
			let found_family_name = unsafe { CStr::from_ptr(found_family_text as *const std::ffi::c_char) }.to_string_lossy().into_owned();
			is_family_matched = found_family_name.eq_ignore_ascii_case(lookup_family_name);
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
	// 대체 글꼴 파일 찾기. (글의 첫 글자를 담은 charset 으로 FcFontSort, 그 글자를 가진 첫 글꼴)
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
// 그 밖의 운영체제: 찾지 않는다.
//==============================================================================
#[cfg(not(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android")))))]
mod platform_font {
	use crate::core::textstringtexturecache::FontFileSource;

	//==============================================================================
	// 글꼴 이름으로 파일 찾기. (찾지 않는다)
	//==============================================================================
	pub fn find_family_font_file(_family_name: &str, _weight: u16, _is_italic: bool) -> Option<FontFileSource> {
		None
	}

	//==============================================================================
	// 대체 글꼴 파일 찾기. (찾지 않는다)
	//==============================================================================
	pub fn find_fallback_font_file(_text: &str, _weight: u16, _is_italic: bool, _is_color_wanted: bool) -> Option<FontFileSource> {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 한글 대체 글꼴과 일반 이름 sans-serif 는 찾고, 없는 이름은 None. (맥, 윈도우, fontconfig 가 있는 리눅스)
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
