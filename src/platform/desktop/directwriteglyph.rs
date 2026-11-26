//==============================================================================
//! 윈도우 DirectWrite 글리프 마스크. (데스크탑 글리프 래스터가 쓴다)
//! - 원본에 없는 파일이다. 원본 크롬 윈도우는 Canvas2D 글자를 스키아 SkScalerContext_DW 가 DirectWrite 로 칠한다.
//!   러스트 판도 같은 DirectWrite 호출로 칠한다. (크롬 158 의 스키아 3279352f, src/ports/SkScalerContext_win_dw.cpp, SkTypeface_win_dw.cpp)
//! - A8 마스크: IDWriteFactory2::CreateGlyphRunAnalysis 에 렌더링 모드(gasp 표, 크기, 바이트코드 힌트, 비트맵 글꼴로 고름), 측정 모드,
//!   격자 맞춤 켬(힌팅 kNormal), 안티에일리어스 그레이스케일을 주고 DWRITE_TEXTURE_ALIASED_1x1 의 알파 텍스처를 받는다.
//!   ClearType 이 켜진 기계는 크롬이 글자를 LCD 로 두어 캔버스에서 kGenA8FromLCD 가 걸리므로, IDWriteFactory 로 ClearType 3x1 텍스처를 받아 세 채널 평균을 낸다. (RGBToA8)
//!   서브픽셀 자리는 글자 행렬의 dx, 가짜 기울임은 글자 행렬의 m21(-1/4)이다. 그 뒤 스키아처럼 감마 전보정 표로 고친다. (GrayscaleToA8)
//! - 글자 대비와 감마: 크롬 158 은 HKCU\SOFTWARE\Microsoft\Avalon.Graphics\<주 화면 장치> 키가 있을 때만 IDWriteRenderingParams 의 값을,
//!   없으면 스키아 기본(대비 1, sRGB)을 쓴다. (ui/gfx/font_util_win.cc GetTextParameters, font_render_params_win.cc, skia/ext/legacy_display_globals.cc 의 SetCachedParams)
//==============================================================================
// 옮긴 원문: 스키아 src/ports/SkScalerContext_win_dw.cpp 에서 옮김, BSD-3-Clause, THIRD_PARTY_LICENSES.md
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use windows::core::{Interface, BOOL, PCWSTR};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::DirectWrite::{
	DWriteCreateFactory, IDWriteFactory, IDWriteFactory2, IDWriteFactory5, IDWriteFactory6, IDWriteFontFace, IDWriteFontFile, IDWriteInMemoryFontFileLoader, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_AXIS_TAG, DWRITE_FONT_AXIS_VALUE, DWRITE_FONT_FACE_TYPE, DWRITE_FONT_FILE_TYPE, DWRITE_FONT_SIMULATIONS_NONE, DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN,
	DWRITE_GRID_FIT_MODE_ENABLED, DWRITE_MATRIX, DWRITE_MEASURING_MODE, DWRITE_RENDERING_MODE, DWRITE_TEXTURE_ALIASED_1x1, DWRITE_TEXTURE_CLEARTYPE_3x1, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
};

/// 글꼴 얼굴 캐시 상한. (넘으면 비운다)
const MAXIMUM_FONT_FACE_COUNT: usize = 32;

thread_local! {
	/// DirectWrite 공유 공장. (스레드마다 처음 쓸 때 한 번 짓는다)
	static DIRECT_WRITE_FACTORY: Option<IDWriteFactory> = unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) }.ok();
	/// 메모리 글꼴 파일 읽개. (공장에 한 번 올린다)
	static IN_MEMORY_LOADER: Option<IDWriteInMemoryFontFileLoader> = create_in_memory_loader();
	/// (글꼴 바이트 자리, 얼굴 번호, 축 값) → (바이트를 붙잡은 Rc, 얼굴).
	static FONT_FACE_CACHE: RefCell<HashMap<(usize, u32, Vec<(u32, u32)>), (Rc<[u8]>, IDWriteFontFace)>> = RefCell::new(HashMap::new());
	/// 글자 대비와 감마. (한 번 읽는다)
	static TEXT_CONTRAST_AND_GAMMA: (f32, f32) = read_registry_text_contrast_and_gamma();
	/// 크롬이 글자를 LCD 로 다루는지. (한 번 읽는다)
	static IS_LCD_TEXT_ENABLED: bool = read_lcd_text_enabled();
}

/// 글리프 마스크. (덮인 정도와 상자, 위가 양수인 top_y)
pub struct DirectWriteGlyphMask {
	/// 덮인 정도. (한 줄에 width 바이트, 위 줄부터)
	pub coverage: Vec<u8>,
	/// 너비. (픽셀)
	pub width: i32,
	/// 높이. (픽셀)
	pub height: i32,
	/// 펜 원점에서 왼쪽 끝. (픽셀)
	pub left_x: i32,
	/// 펜 원점에서 위 끝. (픽셀, 위가 양수)
	pub top_y: i32,
}

//==============================================================================
/// 메모리 글꼴 파일 읽개 짓기. (IDWriteFactory5, 윈도우 10 1703 부터)
//==============================================================================
fn create_in_memory_loader() -> Option<IDWriteInMemoryFontFileLoader> {
	let factory = DIRECT_WRITE_FACTORY.with(|factory| factory.clone())?;
	let factory_5 = factory.cast::<IDWriteFactory5>().ok()?;
	unsafe {
		let loader = factory_5.CreateInMemoryFontFileLoader().ok()?;
		factory.RegisterFontFileLoader(&loader).ok()?;
		Some(loader)
	}
}

//==============================================================================
/// 크롬 윈도우의 글자 대비와 감마. ((대비, 감마), 감마 0 은 sRGB)
/// - 크롬처럼 주 화면 장치의 Avalon.Graphics 키가 있을 때만 DirectWrite 렌더링 값을 쓰고, 없으면 스키아 기본 대비 1, sRGB 다.
//==============================================================================
pub fn read_text_contrast_and_gamma() -> (f32, f32) {
	TEXT_CONTRAST_AND_GAMMA.with(|value| *value)
}

//==============================================================================
/// 레지스트리와 DirectWrite 로 글자 대비와 감마 읽기. (font_util_win.cc GetTextParameters, ClampContrast, ClampGamma)
//==============================================================================
fn read_registry_text_contrast_and_gamma() -> (f32, f32) {
	// 스키아 기본. (크로미엄 skia/BUILD.gn 윈도우: SK_GAMMA_SRGB, SK_GAMMA_CONTRAST=1.0)
	let default_value = (1.0, 0.0);
	if !has_text_settings_registry_key() {
		return default_value;
	}
	let factory = match DIRECT_WRITE_FACTORY.with(|factory| factory.clone()) {
		Some(factory) => {
			factory
		}
		None => {
			return default_value;
		}
	};
	match unsafe { factory.CreateRenderingParams() } {
		Ok(rendering_params) => {
			let contrast = unsafe { rendering_params.GetEnhancedContrast() };
			let gamma = unsafe { rendering_params.GetGamma() };
			// SkSurfaceProps 의 범위. (대비 [0, 1], 감마 [0, 4), 4 는 float epsilon 을 뺀다)
			(contrast.clamp(0.0, 1.0), gamma.clamp(0.0, 4.0 - f32::EPSILON))
		}
		Err(_) => {
			default_value
		}
	}
}

//==============================================================================
/// 주 화면 장치의 글자 설정 레지스트리 키가 있는지. (FontUtilWin::GetTextSettingsRegistryKey: HKCU\SOFTWARE\Microsoft\Avalon.Graphics\<장치 이름의 끝 이름>)
//==============================================================================
fn has_text_settings_registry_key() -> bool {
	read_text_settings_registry().0
}

//==============================================================================
/// 글자 설정 레지스트리 키 읽기. ((키가 있는지, PixelStructure 값))
//==============================================================================
fn read_text_settings_registry() -> (bool, Option<u32>) {
	use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_PRIMARY_DEVICE};
	use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD};
	let mut device_index = 0u32;
	loop {
		let mut display_device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
		display_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
		if unsafe { EnumDisplayDevicesW(std::ptr::null(), device_index, &mut display_device, 0) } == 0 {
			return (false, None);
		}
		if display_device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0 {
			let name_length = display_device.DeviceName.iter().position(|value| *value == 0).unwrap_or(display_device.DeviceName.len());
			let device_name = String::from_utf16_lossy(&display_device.DeviceName[..name_length]);
			// base::FilePath::BaseName: 마지막 '\' 뒤. (\\.\DISPLAY1 → DISPLAY1)
			let base_name = device_name.rsplit('\\').next().unwrap_or("").to_string();
			let key_path: Vec<u16> = format!("SOFTWARE\\Microsoft\\Avalon.Graphics\\{}", base_name).encode_utf16().chain(std::iter::once(0)).collect();
			let mut key: HKEY = 0;
			if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key_path.as_ptr(), 0, KEY_READ, &mut key) } != 0 {
				return (false, None);
			}
			let value_name: Vec<u16> = "PixelStructure".encode_utf16().chain(std::iter::once(0)).collect();
			let mut value_type = 0u32;
			let mut value = 0u32;
			let mut value_size = std::mem::size_of::<u32>() as u32;
			let status = unsafe { RegQueryValueExW(key, value_name.as_ptr(), std::ptr::null(), &mut value_type, &mut value as *mut u32 as *mut u8, &mut value_size) };
			unsafe {
				RegCloseKey(key);
			}
			let pixel_structure = if status == 0 && value_type == REG_DWORD {
				Some(value)
			}
			else {
				None
			};
			return (true, pixel_structure);
		}
		device_index += 1;
	}
}

//==============================================================================
/// 크롬 윈도우가 글자를 LCD 로 다루는지. (ClearType 이면 블링크가 kSubpixelAntiAlias 로 두고, 캔버스는 화소 배열을 모르니 스키아가 A8 에 kGenA8FromLCD 를 건다)
/// - 크롬 font_render_params_win.cc: SPI_GETFONTSMOOTHING 이 켜졌고 SPI_GETFONTSMOOTHINGTYPE 이 ClearType 이면 GetSubpixelRenderingGeometry 가
///   Avalon.Graphics 의 PixelStructure(0 없음, 1 RGB, 2 BGR, 그 밖은 없음), 그 값이 없으면 SPI_GETFONTSMOOTHINGORIENTATION(RGB, BGR, 그 밖은 없음)이다.
///   렌더러는 이 값이 없음이 아니면 LCD 글자를 켠다. (web_view_impl.cc 의 SetLCDTextEnabled)
//==============================================================================
pub fn is_lcd_text_enabled() -> bool {
	IS_LCD_TEXT_ENABLED.with(|value| *value)
}

//==============================================================================
/// 시스템 설정으로 LCD 글자 여부 셈. (is_lcd_text_enabled)
//==============================================================================
fn read_lcd_text_enabled() -> bool {
	use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, FE_FONTSMOOTHINGCLEARTYPE, FE_FONTSMOOTHINGORIENTATIONBGR, FE_FONTSMOOTHINGORIENTATIONRGB, SPI_GETFONTSMOOTHING, SPI_GETFONTSMOOTHINGORIENTATION, SPI_GETFONTSMOOTHINGTYPE};
	let mut is_enabled = 0i32;
	if unsafe { SystemParametersInfoW(SPI_GETFONTSMOOTHING, 0, &mut is_enabled as *mut i32 as *mut std::ffi::c_void, 0) } == 0 || is_enabled == 0 {
		return false;
	}
	let mut smoothing_type = 0u32;
	if unsafe { SystemParametersInfoW(SPI_GETFONTSMOOTHINGTYPE, 0, &mut smoothing_type as *mut u32 as *mut std::ffi::c_void, 0) } == 0 || smoothing_type != FE_FONTSMOOTHINGCLEARTYPE {
		return false;
	}
	if let (true, Some(pixel_structure)) = read_text_settings_registry() {
		return pixel_structure == 1 || pixel_structure == 2;
	}
	let mut orientation = 0u32;
	if unsafe { SystemParametersInfoW(SPI_GETFONTSMOOTHINGORIENTATION, 0, &mut orientation as *mut u32 as *mut std::ffi::c_void, 0) } != 0 {
		return orientation == FE_FONTSMOOTHINGORIENTATIONRGB || orientation == FE_FONTSMOOTHINGORIENTATIONBGR;
	}
	false
}

//==============================================================================
/// 글꼴 얼굴 찾기. (캐시에 없으면 파일이나 메모리에서 짓는다, 축 값이 있으면 IDWriteFactory6 의 글꼴 자원으로)
//==============================================================================
fn find_font_face(font_bytes: &Rc<[u8]>, source_path: Option<&Path>, face_index: u32, variation_list: &[(u32, f32)]) -> Option<IDWriteFontFace> {
	let key = (font_bytes.as_ptr() as usize, face_index, variation_list.iter().map(|(tag, value)| (*tag, value.to_bits())).collect::<Vec<_>>());
	if let Some(font_face) = FONT_FACE_CACHE.with(|cache| cache.borrow().get(&key).map(|(_, font_face)| font_face.clone())) {
		return Some(font_face);
	}
	let factory = DIRECT_WRITE_FACTORY.with(|factory| factory.clone())?;
	let font_file: IDWriteFontFile = unsafe {
		match source_path {
			Some(source_path) => {
				let path: Vec<u16> = source_path.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
				factory.CreateFontFileReference(PCWSTR(path.as_ptr()), None).ok()?
			}
			None => {
				// 소유 개체를 주지 않으면 읽개가 바이트를 베껴 둔다.
				let loader = IN_MEMORY_LOADER.with(|loader| loader.clone())?;
				loader.CreateInMemoryFontFileReference(&factory, font_bytes.as_ptr() as *const _, font_bytes.len() as u32, None).ok()?
			}
		}
	};
	let font_face = if variation_list.is_empty() {
		let mut is_supported = BOOL(0);
		let mut file_type = DWRITE_FONT_FILE_TYPE(0);
		let mut face_type = DWRITE_FONT_FACE_TYPE(0);
		let mut face_count = 0u32;
		unsafe {
			font_file.Analyze(&mut is_supported, &mut file_type, Some(&mut face_type), &mut face_count).ok()?;
		}
		if !is_supported.as_bool() {
			return None;
		}
		unsafe { factory.CreateFontFace(face_type, &[Some(font_file.clone())], face_index, DWRITE_FONT_SIMULATIONS_NONE).ok()? }
	}
	else {
		let factory_6 = factory.cast::<IDWriteFactory6>().ok()?;
		let font_resource = unsafe { factory_6.CreateFontResource(&font_file, face_index).ok()? };
		// DWRITE_MAKE_FONT_AXIS_TAG(a, b, c, d) = (d << 24) | (c << 16) | (b << 8) | a. (러스트 태그 수는 a 가 가장 높은 바이트)
		let axis_value_list: Vec<DWRITE_FONT_AXIS_VALUE> = variation_list
			.iter()
			.map(|(tag, value)| DWRITE_FONT_AXIS_VALUE {
				axisTag: DWRITE_FONT_AXIS_TAG(tag.swap_bytes()),
				value: *value,
			})
			.collect();
		let font_face_5 = unsafe { font_resource.CreateFontFace(DWRITE_FONT_SIMULATIONS_NONE, &axis_value_list).ok()? };
		font_face_5.cast::<IDWriteFontFace>().ok()?
	};
	FONT_FACE_CACHE.with(|cache| {
		let mut cache = cache.borrow_mut();
		if cache.len() >= MAXIMUM_FONT_FACE_COUNT {
			cache.clear();
		}
		cache.insert(key, (font_bytes.clone(), font_face.clone()));
	});
	Some(font_face)
}

//==============================================================================
/// DirectWrite 로 글리프 A8 마스크 그리기. (스키아 SkScalerContext_DW::generateDWMetrics, getDWMaskBits, GrayscaleToA8)
///
/// # 인자
/// - `font_bytes`: 글꼴 바이트. (캐시 열쇠이고, 파일 경로가 없으면 이것으로 짓는다)
/// - `source_path`: 글꼴 파일 경로.
/// - `face_index`: 얼굴 번호.
/// - `variation_list`: 축 값. ((4 글자 태그의 수, 값), 비었으면 기본 인스턴스)
/// - `glyph_index`: 글리프 번호.
/// - `rendering_mode`: DWRITE_RENDERING_MODE 값. (스키아처럼 gasp 표와 크기로 고른 것)
/// - `measuring_mode`: DWRITE_MEASURING_MODE 값.
/// - `render_size`: 그릴 글자 크기. (em 크기, 픽셀)
/// - `subpixel_x`: 서브픽셀 가로 자리. (0 ~ 1)
/// - `skew_x`: 가로 기울기. (가짜 기울임이면 -1/4, 스키아 fPreSkewX 를 DirectWrite 글자 행렬 m21 로)
/// - `correcting_table`: 감마 전보정 표. (None 이면 고치지 않는다)
///
/// # 돌려줌
/// 마스크. 윤곽이 없거나 DirectWrite 가 그리지 못하면 None.
//==============================================================================
#[allow(clippy::too_many_arguments)]
pub fn rasterize_glyph_mask(font_bytes: &Rc<[u8]>, source_path: Option<&Path>, face_index: u32, variation_list: &[(u32, f32)], glyph_index: u16, rendering_mode: i32, measuring_mode: i32, render_size: f32, subpixel_x: f32, skew_x: f32, correcting_table: Option<&[u8; 256]>) -> Option<DirectWriteGlyphMask> {
	let font_face = find_font_face(font_bytes, source_path, face_index, variation_list)?;
	let factory = DIRECT_WRITE_FACTORY.with(|factory| factory.clone())?;
	let factory_2 = factory.cast::<IDWriteFactory2>().ok()?;
	let advance = 0.0f32;
	let offset = DWRITE_GLYPH_OFFSET {
		advanceOffset: 0.0,
		ascenderOffset: 0.0,
	};
	let glyph_run = DWRITE_GLYPH_RUN {
		fontFace: std::mem::ManuallyDrop::new(Some(font_face.clone())),
		fontEmSize: render_size,
		glyphCount: 1,
		glyphIndices: &glyph_index,
		glyphAdvances: &advance,
		glyphOffsets: &offset,
		isSideways: BOOL(0),
		bidiLevel: 0,
	};
	// 스키아 computeMatrices(kVertical) 의 남은 행렬. (세로 배율로 나눈 기울임, 서브픽셀 자리는 dx)
	let transform = DWRITE_MATRIX {
		m11: 1.0,
		m12: 0.0,
		m21: skew_x,
		m22: 1.0,
		dx: subpixel_x,
		dy: 0.0,
	};
	// LCD 글자면 스키아는 그레이스케일로 바꾸지 않고(kGenA8FromLCD) IDWriteFactory 의 ClearType 3x1 텍스처를 받아 세 채널의 평균을 낸다. (RGBToA8)
	let is_lcd = is_lcd_text_enabled();
	let texture_type = if is_lcd {
		DWRITE_TEXTURE_CLEARTYPE_3x1
	}
	else {
		DWRITE_TEXTURE_ALIASED_1x1
	};
	let analysis_result = if is_lcd {
		unsafe { factory.CreateGlyphRunAnalysis(&glyph_run, 1.0, Some(&transform), DWRITE_RENDERING_MODE(rendering_mode), DWRITE_MEASURING_MODE(measuring_mode), 0.0, 0.0) }
	}
	else {
		unsafe { factory_2.CreateGlyphRunAnalysis(&glyph_run, Some(&transform), DWRITE_RENDERING_MODE(rendering_mode), DWRITE_MEASURING_MODE(measuring_mode), DWRITE_GRID_FIT_MODE_ENABLED, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE, 0.0, 0.0) }
	};
	// 글리프 런이 든 얼굴을 놓는다. (ManuallyDrop 이라 직접)
	drop(std::mem::ManuallyDrop::into_inner(glyph_run.fontFace));
	let glyph_run_analysis = analysis_result.ok()?;
	let bounds: RECT = unsafe { glyph_run_analysis.GetAlphaTextureBounds(texture_type).ok()? };
	// 비었거나 크게 깨진 상자는 그리지 않는다. (스키아도 left >= right 면 빈 것으로 본다)
	if bounds.left >= bounds.right || bounds.top >= bounds.bottom {
		return Some(DirectWriteGlyphMask {
			coverage: Vec::new(),
			width: 0,
			height: 0,
			left_x: 0,
			top_y: 0,
		});
	}
	let width = bounds.right - bounds.left;
	let height = bounds.bottom - bounds.top;
	let mut coverage = vec![0u8; (width * height) as usize];
	if is_lcd {
		let mut rgb_coverage = vec![0u8; (width * height * 3) as usize];
		unsafe {
			glyph_run_analysis.CreateAlphaTexture(DWRITE_TEXTURE_CLEARTYPE_3x1, &bounds, &mut rgb_coverage).ok()?;
		}
		for (value, rgb) in coverage.iter_mut().zip(rgb_coverage.chunks_exact(3)) {
			*value = ((rgb[0] as u32 + rgb[1] as u32 + rgb[2] as u32) / 3) as u8;
		}
	}
	else {
		unsafe {
			glyph_run_analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &bounds, &mut coverage).ok()?;
		}
	}
	if let Some(correcting_table) = correcting_table {
		for value in coverage.iter_mut() {
			*value = correcting_table[*value as usize];
		}
	}
	Some(DirectWriteGlyphMask {
		coverage,
		width,
		height,
		left_x: bounds.left,
		top_y: -bounds.top,
	})
}
