//==============================================================================
//! 맥 CoreText 글리프 그리기. (macOS 만)
//! - 원본에 없는 파일이다. 원본 크롬 맥은 글리프를 CoreText(CoreGraphics)로 그린다. 러스트 판도 맥에서는 글리프를 여기서 CoreText 로 그린다. (textshaping.rs 가 부른다)
//!   색 이모지와 열린 해석기가 읽지 못하는 글꼴은 이 파일만 그릴 수 있다.
//!   - 색 이모지(sbix, Apple Color Emoji): CoreText 는 작은 글자 크기에서 이모지를 키워 그리고 아래로 내린다. (16px 에서 20px 판, 24px 부터 글자 크기 그대로)
//!     맥 크롬 Canvas2D 의 measureText 로 잰 이모지 전진폭은 CoreText 가 그린 이모지 상자의 높이와 같다. (8px 10, 12px 15, 16px 20, 20px 22, 24px 24, 32px 32, 64px 64)
//!     크롬의 전진폭은 그 상자 높이를 정수로 반올림한 값이다. (13px 의 16.25 는 16, 10px 의 12.5 는 13. 8 ~ 128px 의 23 가지 크기에서 크롬과 같다, 반올림은 textshaping.rs 가 한다)
//!     CoreText 의 전진폭(CTFontGetAdvancesForGlyphs)은 글꼴의 자간 표(trak)가 더해져 크롬과 다르므로 쓰지 않는다.
//!   - 글리프 윤곽이 glyf, CFF, CFF2 어디에도 없는 글꼴: 맥 26 의 PingFangUI.ttc(중국어 대체 글꼴)는 애플 전용 hvgl 표에 윤곽이 있다.
//! - 글꼴은 그 얼굴의 PostScript 이름으로 짓는다. (시스템에 올린 글꼴만 CoreText 가 이모지를 키워 그린다) 이름이 점으로 시작하는 숨은 글꼴이거나 축 값을 걸면
//!   파일 경로의 CTFontManagerCreateFontDescriptorsFromURL 서술자를 PostScript 이름(없으면 모음 안의 차례)으로 골라 짓는다. 글자 크기마다 지은 CTFont 는 담아 둔다. (스레드 하나, 넘치면 비운다)
//! - 색 글리프 그림은 프리멀티플라이드 RGBA(sRGB) 비트맵 컨텍스트에 흰색으로 그린다. 크롬 캔버스처럼 회색 안티앨리어싱이고 LCD 글자 다듬기는 끈다.
//! - 색이 아닌 글리프의 덮인 정도(rasterize_glyph_mask)는 크롬 맥 Canvas2D 와 같은 길로 그린다. 블링크 FontPlatformData::CreateSkFont 는 캔버스 글자에
//!   kSubpixelAntiAlias 와 서브픽셀 자리를 주고, 투명 캔버스는 화소 배열을 모르므로(kUnknown_SkPixelGeometry) 스키아가 LCD 대신 A8 을 LCD 그림에서 뽑는다.
//!   스키아 SkScalerContext_Mac 은 그때 흰 바탕 RGB 비트맵(DeviceRGB, 알파 없음)에 검은 글자를 글꼴 부드럽게(CGContextSetShouldSmoothFonts)를 켜고 칠한 뒤
//!   CoreGraphics 의 감마 2 를 제곱 표로 풀고(gLinearCoverageFromCGLCDValue), 뒤집은 RGB 의 밝기(SkComputeLuminance)를 감마 전보정 표로 고친다.
//!   글리프 상자는 CTFontGetBoundingRectsForGlyphs 에 서브픽셀 몫을 더해 밖으로 반올림하고 사방 1 픽셀을 더한 것이다. (SkScalerContext_Mac::generateMetrics)
//!   가짜 기울임은 스키아처럼 글자 행렬의 기울기(-1/4)로 CoreText 가 그린다.
//!
//! # 예제
//! 색 이모지 글꼴의 키움 비 재기
//! ```rust
//! use vanilla::platform::desktop::coretextglyph::read_emoji_scale;
//! use vanilla::platform::find_fallback_font_file;
//!
//! if let Some(source) = find_fallback_font_file("\u{1F600}", 400, false, true) {
//! 	let emoji_scale = read_emoji_scale(&source.path, source.face_index.unwrap_or(0), 16.0);
//! 	assert!(emoji_scale > 0.0);
//! }
//! ```
//==============================================================================
// 옮긴 원문: 스키아 src/ports/SkScalerContext_mac_ct.cpp(SkScalerContext_Mac), src/utils/mac/SkCTFont.cpp(SkCTFontGetSmoothBehavior) 의 셈을 옮김, BSD-3-Clause, THIRD_PARTY_LICENSES.md
use objc2_core_foundation::{CFArray, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURL, CGAffineTransform, CGPoint, CGRect};
use objc2_core_graphics::{kCGColorSpaceSRGB, CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo, CGTextDrawingMode};
use objc2_core_text::{kCTFontNameAttribute, kCTFontURLAttribute, kCTFontVariationAttribute, CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromData, CTFontManagerCreateFontDescriptorsFromURL, CTFontOrientation};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

/// 담아 두는 CTFont 의 수 상한. (넘치면 모두 비운다)
const MAXIMUM_FONT_COUNT: usize = 64;
/// 그릴 수 있는 그림의 가로세로 상한. (픽셀)
const MAXIMUM_RASTER_SIZE: i32 = 4096;
/// 이모지 크기를 재는 글자. (웃는 얼굴, 애플 컬러 이모지의 그림은 모두 정사각형이다)
const REFERENCE_EMOJI: char = '\u{1F600}';

thread_local! {
	/// (경로, 얼굴 번호) → 글꼴 서술자. (None 은 CoreText 가 읽지 못한 파일)
	static FONT_DESCRIPTOR_TABLE: RefCell<HashMap<(PathBuf, u32), Option<CFRetained<CTFontDescriptor>>>> = RefCell::new(HashMap::new());
	/// (경로, 얼굴 번호, 크기 비트, 축 값 글) → 지은 CTFont.
	static FONT_TABLE: RefCell<HashMap<(PathBuf, u32, u32, String), CFRetained<CTFont>>> = RefCell::new(HashMap::new());
	/// (바이트 자리, 얼굴 번호) → (바이트, 글꼴 서술자). (바이트를 쥐어 그 자리가 다른 글꼴에 다시 쓰이지 않게 한다, None 은 CoreText 가 읽지 못한 바이트)
	static BYTES_FONT_DESCRIPTOR_TABLE: RefCell<HashMap<(usize, u32), (Rc<[u8]>, Option<CFRetained<CTFontDescriptor>>)>> = RefCell::new(HashMap::new());
	/// (바이트 자리, 얼굴 번호, 크기 비트, 축 값 글) → (바이트, 지은 CTFont).
	static BYTES_FONT_TABLE: RefCell<HashMap<(usize, u32, u32, String), (Rc<[u8]>, CFRetained<CTFont>)>> = RefCell::new(HashMap::new());
}

/// CoreGraphics 의 글꼴 부드럽게 판정. (처음 물을 때 한 번 잰다)
static SMOOTH_BEHAVIOR: OnceLock<SmoothBehavior> = OnceLock::new();

//==============================================================================
/// 글꼴 부드럽게(CGContextSetShouldSmoothFonts)가 CoreGraphics 그림에 하는 일. (스키아 SkCTFontSmoothBehavior)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SmoothBehavior {
	/// 켜도 그림이 같다. (시스템 설정에서 끈 경우)
	None,
	/// 획을 두껍게 하되 회색이다. (맥 10.14 부터의 기본)
	Some,
	/// 채널마다 다른 LCD 그림이다.
	Subpixel,
}

//==============================================================================
/// 색이 아닌 글리프의 덮인 정도. (크롬 맥 Canvas2D 의 A8 마스크, 왼쪽 위 자리는 펜 정수 자리에서 오른쪽 left_x, 기준선에서 위로 top_y)
//==============================================================================
pub(crate) struct CoreTextGlyphMask {
	/// 덮인 정도. (0 ~ 255, width * height 개, 위 줄부터)
	pub(crate) coverage: Vec<u8>,
	/// 가로. (픽셀)
	pub(crate) width: i32,
	/// 세로. (픽셀)
	pub(crate) height: i32,
	/// 펜 정수 자리에서 그림 왼쪽까지. (픽셀, 오른쪽이 양수)
	pub(crate) left_x: i32,
	/// 기준선에서 그림 위쪽까지. (픽셀, 위가 양수)
	pub(crate) top_y: i32,
}

//==============================================================================
/// CoreText 로 지을 글꼴의 출처. (파일 경로나 풀린 글꼴 바이트)
//==============================================================================
pub(crate) enum CoreTextFontSource<'source> {
	/// 글꼴 파일 경로.
	Path(&'source Path),
	/// 풀린 글꼴 바이트. (ttf, otf, ttc)
	Bytes(&'source Rc<[u8]>),
}

//==============================================================================
/// CoreText 로 그린 글리프. (프리멀티플라이드 RGBA, 왼쪽 위 자리는 펜 원점에서 오른쪽 left_x, 기준선에서 위로 top_y)
///
/// # 예제
/// 시스템 대체 글꼴의 A 글리프 그리기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::textshaping::LoadedFont;
/// use vanilla::platform::desktop::coretextglyph::{rasterize_glyph, CoreTextGlyphRaster};
/// use vanilla::platform::find_fallback_font_file;
///
/// fn rasterize_character(character: char, pixel_size: f32) -> Option<CoreTextGlyphRaster> {
/// 	let source = find_fallback_font_file(&character.to_string(), 400, false, false)?;
/// 	let face_index = source.face_index.unwrap_or(0);
/// 	let font_bytes: Rc<[u8]> = std::fs::read(&source.path).ok()?.into();
/// 	let loaded_font = LoadedFont::create_from_shared_bytes(font_bytes, face_index, &[]).ok()?;
/// 	let glyph_id = loaded_font.get_shaping_face().glyph_index(character)?;
/// 	rasterize_glyph(&source.path, face_index, glyph_id.0, pixel_size, &[])
/// }
///
/// if let Some(raster) = rasterize_character('A', 32.0) {
/// 	assert_eq!(raster.pixels.len(), (raster.width * raster.height * 4) as usize);
/// 	assert!(raster.top_y > 0);
/// }
/// ```
//==============================================================================
pub struct CoreTextGlyphRaster {
	/// 픽셀. (프리멀티플라이드 RGBA, 한 줄 width * 4 바이트, 위 줄부터)
	pub pixels: Vec<u8>,
	/// 가로. (픽셀, 글리프 상자에 사방 1 픽셀을 더한 것)
	pub width: i32,
	/// 세로. (픽셀, 글리프 상자에 사방 1 픽셀을 더한 것)
	pub height: i32,
	/// 펜 원점에서 그림 왼쪽까지. (픽셀, 오른쪽이 양수)
	pub left_x: i32,
	/// 기준선에서 그림 위쪽까지. (픽셀, 위가 양수)
	pub top_y: i32,
}

//==============================================================================
/// 파일의 얼굴 서술자. (처음 쓸 때 짓고 담아 둔다)
//==============================================================================
fn read_font_descriptor(path: &Path, face_index: u32) -> Option<CFRetained<CTFontDescriptor>> {
	FONT_DESCRIPTOR_TABLE.with(|font_descriptor_table_cell| {
		let mut font_descriptor_table = font_descriptor_table_cell.borrow_mut();
		let key = (path.to_path_buf(), face_index);
		if let Some(font_descriptor) = font_descriptor_table.get(&key) {
			return font_descriptor.clone();
		}
		// 서술자 목록의 차례는 모음 안의 차례와 다를 수 있어 PostScript 이름으로 맞춘다. (없으면 차례로)
		let post_script_name = read_face_post_script_name(path, face_index);
		let font_descriptor = CFURL::from_file_path(path).and_then(|url| {
			let descriptor_array: CFRetained<CFArray> = unsafe { CTFontManagerCreateFontDescriptorsFromURL(&url) }?;
			let typed_descriptor_array = unsafe { descriptor_array.cast_unchecked::<CTFontDescriptor>() };
			let named_descriptor = post_script_name.as_ref().and_then(|post_script_name| {
				typed_descriptor_array.iter().find(|font_descriptor| {
					let name_value = unsafe { font_descriptor.attribute(kCTFontNameAttribute) };
					name_value.and_then(|name_value| name_value.downcast::<CFString>().ok()).map(|name_text| name_text.to_string() == *post_script_name).unwrap_or(false)
				})
			});
			named_descriptor.or_else(|| typed_descriptor_array.get(face_index as usize))
		});
		font_descriptor_table.insert(key, font_descriptor.clone());
		font_descriptor
	})
}

//==============================================================================
/// 모음 안 얼굴의 PostScript 이름. (파일 앞머리의 표 목록과 name 표만 읽는다, 못 읽으면 None)
//==============================================================================
fn read_face_post_script_name(path: &Path, face_index: u32) -> Option<String> {
	let font_bytes = std::fs::read(path).ok()?;
	read_bytes_post_script_name(&font_bytes, face_index)
}

//==============================================================================
/// 글꼴 바이트 안 얼굴의 PostScript 이름. (못 읽으면 None)
//==============================================================================
fn read_bytes_post_script_name(font_bytes: &[u8], face_index: u32) -> Option<String> {
	let face = rustybuzz::ttf_parser::Face::parse(font_bytes, face_index).ok()?;
	face.names().into_iter().filter(|name| name.name_id == rustybuzz::ttf_parser::name_id::POST_SCRIPT_NAME).find_map(|name| {
		match name.to_string() {
			Some(name_text) => {
				Some(name_text)
			}
			None => {
				// 맥 로만 인코딩. (PostScript 이름은 ASCII 다)
				if name.platform_id == rustybuzz::ttf_parser::PlatformId::Macintosh {
					Some(String::from_utf8_lossy(name.name).into_owned())
				}
				else {
					None
				}
			}
		}
	})
}

//==============================================================================
/// 서술자에 가변 글꼴 축 값 걸기. (축 값이 없으면 그대로)
//==============================================================================
fn apply_variation_list(font_descriptor: CFRetained<CTFontDescriptor>, variation_list: &[(u32, f32)]) -> CFRetained<CTFontDescriptor> {
	if variation_list.is_empty() {
		return font_descriptor;
	}
	let axis_key_list: Vec<CFRetained<CFNumber>> = variation_list.iter().map(|(tag, _)| CFNumber::new_i64(*tag as i64)).collect();
	let axis_value_list: Vec<CFRetained<CFNumber>> = variation_list.iter().map(|(_, value)| CFNumber::new_f64(*value as f64)).collect();
	let axis_key_reference_list: Vec<&CFNumber> = axis_key_list.iter().map(|axis_key| &**axis_key).collect();
	let axis_value_reference_list: Vec<&CFNumber> = axis_value_list.iter().map(|axis_value| &**axis_value).collect();
	let variation_dictionary = CFDictionary::<CFNumber, CFNumber>::from_slices(&axis_key_reference_list, &axis_value_reference_list);
	let attribute_key_list: [&CFString; 1] = unsafe { [kCTFontVariationAttribute] };
	let attribute_value_list: [&CFType; 1] = [&variation_dictionary];
	let attribute_dictionary = CFDictionary::<CFString, CFType>::from_slices(&attribute_key_list, &attribute_value_list);
	unsafe { font_descriptor.copy_with_attributes(attribute_dictionary.as_ref()) }
}

//==============================================================================
/// 글꼴 바이트의 얼굴 서술자. (CTFontManagerCreateFontDescriptorsFromData, PostScript 이름으로 고르고 없으면 차례로, 처음 쓸 때 짓고 담아 둔다)
//==============================================================================
fn read_bytes_font_descriptor(font_bytes: &Rc<[u8]>, face_index: u32) -> Option<CFRetained<CTFontDescriptor>> {
	BYTES_FONT_DESCRIPTOR_TABLE.with(|font_descriptor_table_cell| {
		let mut font_descriptor_table = font_descriptor_table_cell.borrow_mut();
		let key = (font_bytes.as_ptr() as usize, face_index);
		if let Some((_, font_descriptor)) = font_descriptor_table.get(&key) {
			return font_descriptor.clone();
		}
		let post_script_name = read_bytes_post_script_name(font_bytes, face_index);
		let data = CFData::from_bytes(font_bytes);
		let font_descriptor = {
			let descriptor_array: CFRetained<CFArray> = unsafe { CTFontManagerCreateFontDescriptorsFromData(&data) };
			let typed_descriptor_array = unsafe { descriptor_array.cast_unchecked::<CTFontDescriptor>() };
			let named_descriptor = post_script_name.as_ref().and_then(|post_script_name| {
				typed_descriptor_array.iter().find(|font_descriptor| {
					let name_value = unsafe { font_descriptor.attribute(kCTFontNameAttribute) };
					name_value.and_then(|name_value| name_value.downcast::<CFString>().ok()).map(|name_text| name_text.to_string() == *post_script_name).unwrap_or(false)
				})
			});
			named_descriptor.or_else(|| typed_descriptor_array.get(face_index as usize))
		};
		if font_descriptor_table.len() >= MAXIMUM_FONT_COUNT {
			font_descriptor_table.clear();
		}
		font_descriptor_table.insert(key, (font_bytes.clone(), font_descriptor.clone()));
		font_descriptor
	})
}

//==============================================================================
/// 글꼴 바이트의 글자 크기와 축 값 CTFont. (처음 쓸 때 짓고 담아 둔다)
//==============================================================================
fn read_bytes_font(font_bytes: &Rc<[u8]>, face_index: u32, pixel_size: f32, variation_list: &[(u32, f32)]) -> Option<CFRetained<CTFont>> {
	let variation_key: String = variation_list.iter().map(|(tag, value)| format!("{}={};", tag, value)).collect();
	let key = (font_bytes.as_ptr() as usize, face_index, pixel_size.to_bits(), variation_key);
	if let Some(font) = BYTES_FONT_TABLE.with(|font_table_cell| font_table_cell.borrow().get(&key).map(|(_, font)| font.clone())) {
		return Some(font);
	}
	let font_descriptor = apply_variation_list(read_bytes_font_descriptor(font_bytes, face_index)?, variation_list);
	let font = unsafe { CTFont::with_font_descriptor(&font_descriptor, pixel_size as f64, std::ptr::null()) };
	BYTES_FONT_TABLE.with(|font_table_cell| {
		let mut font_table = font_table_cell.borrow_mut();
		if font_table.len() >= MAXIMUM_FONT_COUNT {
			font_table.clear();
		}
		font_table.insert(key, (font_bytes.clone(), font.clone()));
	});
	Some(font)
}

//==============================================================================
/// 글꼴 부드럽게 판정. (스키아 SkCTFontGetSmoothBehavior 와 같은 셈, 처음 물을 때 한 번 잰다)
/// - 16x16 검은 RGB 비트맵 둘에 흰 글리프를 글꼴 부드럽게를 끄고 켜고 그려 견준다. 켠 그림의 채널이 다르면 Subpixel, 끈 그림과 다르면 Some, 같으면 None.
///   스키아는 저장소에 넣은 거미 글꼴(SpiderSymbol)의 글리프를 그리고, 여기서는 시스템 글꼴 Helvetica 의 M 을 그린다. (설정이 그림을 바꾸는지만 본다)
//==============================================================================
pub(crate) fn read_smooth_behavior() -> SmoothBehavior {
	*SMOOTH_BEHAVIOR.get_or_init(|| {
		let font = unsafe { CTFont::with_name(&CFString::from_str("Helvetica"), 16.0, std::ptr::null()) };
		let mut character_unit = [b'M' as u16];
		let mut glyph = [0u16];
		let is_found = unsafe { font.glyphs_for_characters(std::ptr::NonNull::from(&mut character_unit[0]), std::ptr::NonNull::from(&mut glyph[0]), 1) };
		if !is_found {
			return SmoothBehavior::Some;
		}
		let mut no_smooth_pixels = vec![0u8; 16 * 16 * 4];
		let mut smooth_pixels = vec![0u8; 16 * 16 * 4];
		let Some(color_space) = CGColorSpace::new_device_rgb() else {
			return SmoothBehavior::Some;
		};
		let bitmap_info = CGImageAlphaInfo::NoneSkipFirst.0 | CGImageByteOrderInfo::Order32Little.0;
		for (pixels, is_smooth) in [(&mut no_smooth_pixels, false), (&mut smooth_pixels, true)] {
			let Some(context) = (unsafe { CGBitmapContextCreate(pixels.as_mut_ptr().cast(), 16, 16, 8, 16 * 4, Some(&color_space), bitmap_info) }) else {
				return SmoothBehavior::Some;
			};
			CGContext::set_should_smooth_fonts(Some(&context), is_smooth);
			CGContext::set_should_antialias(Some(&context), true);
			CGContext::set_text_drawing_mode(Some(&context), CGTextDrawingMode::Fill);
			CGContext::set_gray_fill_color(Some(&context), 1.0, 1.0);
			let mut position = CGPoint { x: 0.0, y: 3.0 };
			unsafe { font.draw_glyphs(std::ptr::NonNull::from(&mut glyph[0]), std::ptr::NonNull::from(&mut position), 1, &context) };
		}
		let mut smooth_behavior = SmoothBehavior::None;
		for pixel_index in 0..16 * 16 {
			let smooth_pixel = &smooth_pixels[pixel_index * 4..pixel_index * 4 + 3];
			if smooth_pixel[0] != smooth_pixel[1] || smooth_pixel[0] != smooth_pixel[2] {
				return SmoothBehavior::Subpixel;
			}
			if no_smooth_pixels[pixel_index * 4..pixel_index * 4 + 3] != *smooth_pixel {
				smooth_behavior = SmoothBehavior::Some;
			}
		}
		smooth_behavior
	})
}

//==============================================================================
/// 색이 아닌 글리프의 덮인 정도 그리기. (스키아 SkScalerContext_Mac 이 크롬 캔버스 글자에 그리는 A8 마스크와 같은 셈)
/// - 흰 바탕의 DeviceRGB 비트맵(알파 없음, 호스트 바이트 차례)에 검은 글자를 안티앨리어싱과 글꼴 부드럽게를 켜고, 서브픽셀 양자화는 끄고 서브픽셀 자리는 켜고 칠한다. (Offscreen::getCG)
/// - 글꼴 부드럽게가 그림을 바꾸면 채널마다 CoreGraphics 의 감마 2 를 풀고((v * v + 128) / 255), 뒤집은 RGB 의 밝기를 전보정 표로 고친다. (generateImage, rgb_to_a8)
///
/// # 인자
/// - `font_source`: 글꼴 출처. (파일 경로나 풀린 바이트)
/// - `face_index`: 모음(TTC) 안의 얼굴 번호.
/// - `glyph_index`: 글리프 번호.
/// - `pixel_size`: 글자 크기. (픽셀)
/// - `variation_list`: 가변 글꼴 축 값. ((4 글자 태그의 수, 값) 목록)
/// - `subpixel_x`: 서브픽셀 가로 자리. (0, 0.25, 0.5, 0.75)
/// - `skew_x`: 글자 행렬의 기울기. (가짜 기울임이면 스키아 setSkewX(-1/4) 의 CoreGraphics 값 0.25, 아니면 0)
/// - `correcting_table`: 감마 전보정 표. (None 이면 고치지 않는다)
///
/// # 돌려줌
/// 덮인 정도. 그릴 것이 없거나, CoreText 가 글꼴을 짓지 못했거나, 그림이 4096 픽셀을 넘으면 None.
//==============================================================================
#[allow(clippy::too_many_arguments)]
pub(crate) fn rasterize_glyph_mask(font_source: CoreTextFontSource, face_index: u32, glyph_index: u16, pixel_size: f32, variation_list: &[(u32, f32)], subpixel_x: f32, skew_x: f32, correcting_table: Option<&[u8; 256]>) -> Option<CoreTextGlyphMask> {
	let font = match font_source {
		CoreTextFontSource::Path(path) => {
			read_font(path, face_index, pixel_size, variation_list)?
		}
		CoreTextFontSource::Bytes(font_bytes) => {
			read_bytes_font(font_bytes, face_index, pixel_size, variation_list)?
		}
	};
	// 글자 행렬. (스키아 MatrixToCGAffineTransform: c 는 -kMSkewX)
	let text_transform = CGAffineTransform {
		a: 1.0,
		b: 0.0,
		c: skew_x as f64,
		d: 1.0,
		tx: 0.0,
		ty: 0.0,
	};
	// 글리프 상자. (CG 단위 픽셀 y 위로, 글자 행렬을 건 상자를 감싸는 네모, 스키아는 SkScalar 로 바꿔 셈한다)
	let mut glyph = glyph_index;
	let mut glyph_rect = CGRect::default();
	unsafe { font.bounding_rects_for_glyphs(CTFontOrientation::Horizontal, std::ptr::NonNull::from(&mut glyph), &mut glyph_rect, 1) };
	if glyph_rect.size.width <= 0.0 || glyph_rect.size.height <= 0.0 {
		return None;
	}
	let corner_list = [(glyph_rect.origin.x, glyph_rect.origin.y), (glyph_rect.origin.x + glyph_rect.size.width, glyph_rect.origin.y), (glyph_rect.origin.x, glyph_rect.origin.y + glyph_rect.size.height), (glyph_rect.origin.x + glyph_rect.size.width, glyph_rect.origin.y + glyph_rect.size.height)];
	let transformed_x_list: Vec<f64> = corner_list.iter().map(|(x, y)| x + text_transform.c * y).collect();
	let minimum_x = transformed_x_list.iter().cloned().fold(f64::INFINITY, f64::min) as f32;
	let maximum_x = transformed_x_list.iter().cloned().fold(f64::NEG_INFINITY, f64::max) as f32;
	let minimum_y = glyph_rect.origin.y as f32;
	let maximum_y = (glyph_rect.origin.y + glyph_rect.size.height) as f32;
	// 스키아 글리프 단위(y 아래로)로 옮기고 서브픽셀 몫을 오른쪽에 더해 밖으로 반올림한 뒤 사방 1 픽셀. (generateMetrics)
	let left = minimum_x.floor() as i32 - 1;
	let top = (-maximum_y).floor() as i32 - 1;
	let right = (maximum_x + subpixel_x).ceil() as i32 + 1;
	let bottom = (-minimum_y).ceil() as i32 + 1;
	let width = right - left;
	let height = bottom - top;
	if width <= 0 || height <= 0 || width > MAXIMUM_RASTER_SIZE || height > MAXIMUM_RASTER_SIZE {
		return None;
	}
	// 흰 바탕. (알파 없는 RGB, 호스트 바이트 차례면 메모리는 B, G, R, X)
	let mut pixels = vec![0xFFu8; (width * height * 4) as usize];
	{
		let color_space = CGColorSpace::new_device_rgb()?;
		let bitmap_info = CGImageAlphaInfo::NoneSkipFirst.0 | CGImageByteOrderInfo::Order32Little.0;
		let context: CFRetained<CGContext> = unsafe { CGBitmapContextCreate(pixels.as_mut_ptr().cast(), width as usize, height as usize, 8, (width * 4) as usize, Some(&color_space), bitmap_info) }?;
		CGContext::set_allows_font_subpixel_quantization(Some(&context), false);
		CGContext::set_should_subpixel_quantize_fonts(Some(&context), false);
		CGContext::set_allows_font_subpixel_positioning(Some(&context), true);
		CGContext::set_should_subpixel_position_fonts(Some(&context), true);
		CGContext::set_text_drawing_mode(Some(&context), CGTextDrawingMode::Fill);
		CGContext::set_gray_fill_color(Some(&context), 0.0, 1.0);
		CGContext::set_text_matrix(Some(&context), text_transform);
		CGContext::set_should_antialias(Some(&context), true);
		CGContext::set_should_smooth_fonts(Some(&context), true);
		// 펜 자리. (그림 왼쪽 아래가 원점, 글자 행렬을 거꾸로 걸어 글자 공간으로)
		let point_x = -left as f64 + subpixel_x as f64;
		let point_y = (top + height) as f64;
		let mut position = CGPoint {
			x: point_x - text_transform.c * point_y,
			y: point_y,
		};
		unsafe { font.draw_glyphs(std::ptr::NonNull::from(&mut glyph), std::ptr::NonNull::from(&mut position), 1, &context) };
	}
	let is_smooth_applied = read_smooth_behavior() != SmoothBehavior::None;
	let linearize = |value: u8| -> u32 {
		if is_smooth_applied {
			(value as u32 * value as u32 + 128) / 255
		}
		else {
			value as u32
		}
	};
	let coverage: Vec<u8> = pixels
		.chunks_exact(4)
		.map(|pixel| {
			let blue = 255 - linearize(pixel[0]);
			let green = 255 - linearize(pixel[1]);
			let red = 255 - linearize(pixel[2]);
			let luminance = crate::core::maskgamma::compute_luminance(red as u8, green as u8, blue as u8);
			match correcting_table {
				Some(correcting_table) => {
					correcting_table[luminance as usize]
				}
				None => {
					luminance
				}
			}
		})
		.collect();
	Some(CoreTextGlyphMask {
		coverage,
		width,
		height,
		left_x: left,
		top_y: -top,
	})
}

//==============================================================================
/// 글자 크기와 축 값의 CTFont. (축 값은 (4 글자 태그의 수, 값) 목록, 처음 쓸 때 짓고 담아 둔다)
//==============================================================================
fn read_font(path: &Path, face_index: u32, pixel_size: f32, variation_list: &[(u32, f32)]) -> Option<CFRetained<CTFont>> {
	let variation_key: String = variation_list.iter().map(|(tag, value)| format!("{}={};", tag, value)).collect();
	let key = (path.to_path_buf(), face_index, pixel_size.to_bits(), variation_key);
	if let Some(font) = FONT_TABLE.with(|font_table_cell| font_table_cell.borrow().get(&key).cloned()) {
		return Some(font);
	}
	// 시스템에 올린 글꼴은 이름으로 지어야 CoreText 가 그 글꼴에 거는 처리(색 이모지를 작은 크기에서 키우기)가 들어간다.
	// 파일로 지은 글꼴에는 들어가지 않는다. 점으로 시작하는 숨은 이름은 이름으로 지을 수 없어 파일로 짓는다.
	if variation_list.is_empty() {
		if let Some(post_script_name) = read_face_post_script_name(path, face_index) {
			if !post_script_name.starts_with('.') {
				let named_font = unsafe { CTFont::with_name(&CFString::from_str(&post_script_name), pixel_size as f64, std::ptr::null()) };
				let is_same_font = unsafe { named_font.post_script_name() }.to_string() == post_script_name;
				let is_same_file = unsafe { named_font.attribute(kCTFontURLAttribute) }.and_then(|url_value| url_value.downcast::<CFURL>().ok()).and_then(|url| url.to_file_path()).map(|named_path| named_path == path).unwrap_or(false);
				if is_same_font && is_same_file {
					FONT_TABLE.with(|font_table_cell| {
						let mut font_table = font_table_cell.borrow_mut();
						if font_table.len() >= MAXIMUM_FONT_COUNT {
							font_table.clear();
						}
						font_table.insert(key.clone(), named_font.clone());
					});
					return Some(named_font);
				}
			}
		}
	}
	let font_descriptor = apply_variation_list(read_font_descriptor(path, face_index)?, variation_list);
	let font = unsafe { CTFont::with_font_descriptor(&font_descriptor, pixel_size as f64, std::ptr::null()) };
	FONT_TABLE.with(|font_table_cell| {
		let mut font_table = font_table_cell.borrow_mut();
		if font_table.len() >= MAXIMUM_FONT_COUNT {
			font_table.clear();
		}
		font_table.insert(key, font.clone());
	});
	Some(font)
}

//==============================================================================
/// 글리프가 그려지는 상자. (CTFontGetBoundingRectsForGlyphs, 펜 원점 기준 픽셀, y 는 위가 양수. 그릴 것이 없으면 None)
///
/// # 인자
/// - `path`: 글꼴 파일 경로.
/// - `face_index`: 모음(TTC) 안의 얼굴 번호.
/// - `glyph_index`: 글리프 번호.
/// - `pixel_size`: 글자 크기. (픽셀)
/// - `variation_list`: 가변 글꼴 축 값. ((4 글자 태그의 수, 값) 목록, 비면 축을 걸지 않는다)
///
/// # 돌려줌
/// 글리프 상자. CoreText 가 글꼴을 짓지 못했거나 상자의 가로나 세로가 0 이하면 None.
//==============================================================================
pub fn read_glyph_bounds(path: &Path, face_index: u32, glyph_index: u16, pixel_size: f32, variation_list: &[(u32, f32)]) -> Option<CGRect> {
	let font = read_font(path, face_index, pixel_size, variation_list)?;
	let mut glyph = glyph_index;
	let mut glyph_rect = CGRect::default();
	unsafe { font.bounding_rects_for_glyphs(CTFontOrientation::Horizontal, std::ptr::NonNull::from(&mut glyph), &mut glyph_rect, 1) };
	if glyph_rect.size.width <= 0.0 || glyph_rect.size.height <= 0.0 {
		return None;
	}
	Some(glyph_rect)
}

//==============================================================================
/// 스키아가 짓는 CTFont 의 글리프 상자. (크롬 맥 measureText 의 경로 없는 글리프 상자 셈, 반올림하지 않는다)
/// - 스키아 SkScalerContext_Mac 은 글꼴을 SkCTFontCreateExactCopy 로 다시 짓는다. 광학 크기 속성(NSCTFontOpticalSizeAttribute)은 바탕 글꼴의 값이나
///   글자 크기이고, 자간 속성(NSCTFontUnscaledTrackingAttribute)을 0 으로 두어 trak 과 그 밖의 자간을 끈다. 그 글꼴의 CTFontGetBoundingRectsForGlyphs 다.
/// - 색 이모지는 이 글꼴에서 키우기에 따른 자리 밀림이 빠진다. (16px 에서 바탕 글꼴 0.46 ~ 20.46, 스키아 글꼴 0 ~ 20)
///
/// # 인자
/// - `path`: 글꼴 파일 경로.
/// - `face_index`: 모음(TTC) 안의 얼굴 번호.
/// - `glyph_index`: 글리프 번호.
/// - `pixel_size`: 글자 크기. (픽셀)
/// - `variation_list`: 축 값 목록. ((4 글자 태그의 수, 값))
///
/// # 돌려줌
/// 글리프 상자. (펜 원점에서 픽셀, 위가 양수) CoreText 가 글꼴을 짓지 못했거나 상자의 가로나 세로가 0 이하면 None.
//==============================================================================
pub fn read_glyph_skia_bounds(path: &Path, face_index: u32, glyph_index: u16, pixel_size: f32, variation_list: &[(u32, f32)]) -> Option<CGRect> {
	let base_font = read_font(path, face_index, pixel_size, variation_list)?;
	let optical_size_key = CFString::from_static_str("NSCTFontOpticalSizeAttribute");
	let tracking_key = CFString::from_static_str("NSCTFontUnscaledTrackingAttribute");
	let base_optical_size = unsafe { base_font.attribute(&optical_size_key) }.and_then(|value| value.downcast::<CFNumber>().ok()).and_then(|number| number.as_f64()).filter(|value| *value > 0.0);
	let optical_size = base_optical_size.unwrap_or_else(|| unsafe { base_font.size() });
	let optical_size_value = CFNumber::new_f64(optical_size);
	let tracking_value = CFNumber::new_i32(0);
	let attribute_key_list: [&CFString; 2] = [&optical_size_key, &tracking_key];
	let attribute_value_list: [&CFType; 2] = [&optical_size_value, &tracking_value];
	let attribute_dictionary = CFDictionary::<CFString, CFType>::from_slices(&attribute_key_list, &attribute_value_list);
	let font_descriptor = unsafe { CTFontDescriptor::with_attributes(attribute_dictionary.as_ref()) };
	let font = unsafe { base_font.copy_with_attributes(pixel_size as f64, std::ptr::null(), Some(&font_descriptor)) };
	let mut glyph = glyph_index;
	let mut glyph_rect = CGRect::default();
	unsafe { font.bounding_rects_for_glyphs(CTFontOrientation::Horizontal, std::ptr::NonNull::from(&mut glyph), &mut glyph_rect, 1) };
	if glyph_rect.size.width <= 0.0 || glyph_rect.size.height <= 0.0 {
		return None;
	}
	Some(glyph_rect)
}

//==============================================================================
/// 색 이모지 글꼴을 CoreText 가 키워 그리는 비. (웃는 얼굴 이모지 상자의 높이 / 글자 크기, 재지 못하면 1)
///
/// # 인자
/// - `path`: 글꼴 파일 경로.
/// - `face_index`: 모음(TTC) 안의 얼굴 번호.
/// - `pixel_size`: 글자 크기. (픽셀)
///
/// # 돌려줌
/// 이모지 상자 높이를 글자 크기로 나눈 비. (16px 에서 1.25, 24px 부터 1) 글자 크기가 0 이하이거나, 글꼴에 웃는 얼굴이 없거나, 재지 못하면 1.
//==============================================================================
pub fn read_emoji_scale(path: &Path, face_index: u32, pixel_size: f32) -> f32 {
	if pixel_size <= 0.0 {
		return 1.0;
	}
	let measured_scale = read_font(path, face_index, pixel_size, &[]).and_then(|font| {
		let mut character_units = [0u16; 2];
		let character_unit_count = REFERENCE_EMOJI.encode_utf16(&mut character_units).len();
		let mut glyph_list = [0u16; 2];
		let is_found = unsafe { font.glyphs_for_characters(std::ptr::NonNull::from(&mut character_units[0]), std::ptr::NonNull::from(&mut glyph_list[0]), character_unit_count as isize) };
		if !is_found || glyph_list[0] == 0 {
			return None;
		}
		let glyph_rect = read_glyph_bounds(path, face_index, glyph_list[0], pixel_size, &[])?;
		Some(glyph_rect.size.height as f32 / pixel_size)
	});
	measured_scale.unwrap_or(1.0)
}

//==============================================================================
/// 글리프 하나 그리기. (흰색, 색 글리프는 제 색. 그릴 것이 없거나 CoreText 가 읽지 못하면 None)
///
/// # 인자
/// - `path`: 글꼴 파일 경로.
/// - `face_index`: 모음(TTC) 안의 얼굴 번호.
/// - `glyph_index`: 글리프 번호.
/// - `pixel_size`: 글자 크기. (픽셀)
/// - `variation_list`: 가변 글꼴 축 값. ((4 글자 태그의 수, 값) 목록)
///
/// # 돌려줌
/// 그린 글리프. 그릴 것이 없거나, CoreText 가 글꼴을 짓지 못했거나, 그림이 4096 픽셀을 넘으면 None.
//==============================================================================
pub fn rasterize_glyph(path: &Path, face_index: u32, glyph_index: u16, pixel_size: f32, variation_list: &[(u32, f32)]) -> Option<CoreTextGlyphRaster> {
	let font = read_font(path, face_index, pixel_size, variation_list)?;
	let glyph_rect = read_glyph_bounds(path, face_index, glyph_index, pixel_size, variation_list)?;
	// 안티앨리어싱이 번지는 몫으로 사방에 한 픽셀씩 둔다.
	let left_x = glyph_rect.origin.x.floor() as i32 - 1;
	let right_x = (glyph_rect.origin.x + glyph_rect.size.width).ceil() as i32 + 1;
	let bottom_y = glyph_rect.origin.y.floor() as i32 - 1;
	let top_y = (glyph_rect.origin.y + glyph_rect.size.height).ceil() as i32 + 1;
	let width = right_x - left_x;
	let height = top_y - bottom_y;
	if width <= 0 || height <= 0 || width > MAXIMUM_RASTER_SIZE || height > MAXIMUM_RASTER_SIZE {
		return None;
	}
	let mut pixels = vec![0u8; (width * height * 4) as usize];
	{
		let color_space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
		let bitmap_info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
		let context: CFRetained<CGContext> = unsafe { CGBitmapContextCreate(pixels.as_mut_ptr().cast(), width as usize, height as usize, 8, (width * 4) as usize, Some(&color_space), bitmap_info) }?;
		CGContext::set_should_antialias(Some(&context), true);
		CGContext::set_allows_font_smoothing(Some(&context), false);
		CGContext::set_should_smooth_fonts(Some(&context), false);
		CGContext::set_rgb_fill_color(Some(&context), 1.0, 1.0, 1.0, 1.0);
		let mut glyph = glyph_index;
		let mut position = CGPoint {
			x: -left_x as f64,
			y: -bottom_y as f64,
		};
		unsafe { font.draw_glyphs(std::ptr::NonNull::from(&mut glyph), std::ptr::NonNull::from(&mut position), 1, &context) };
	}
	Some(CoreTextGlyphRaster {
		pixels,
		width,
		height,
		left_x,
		top_y,
	})
}
