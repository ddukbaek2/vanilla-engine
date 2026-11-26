//==============================================================================
//! 글자 마스크 감마 전보정. (데스크탑 글리프 래스터가 쓴다)
//! - 원본에 없는 파일이다. 원본 크롬은 글리프의 덮인 정도(A8 마스크)를 스키아가 감마 전보정 표로 고친 뒤 글자 색으로 칠한다.
//!   러스트 판도 같은 표로 고친다. 스키아 SkMaskGamma.cpp 의 SkTMaskGamma_build_correcting_lut, SkMaskGamma.h 의 CanonicalColor, preBlend,
//!   SkColorData.h 의 SkComputeLuminance, SkScalerContext.cpp 의 PreprocessRec, MakeRecAndEffects 를 옮겼다. (크롬 158 의 스키아 3279352f)
//! - 밝기는 채널마다 3 비트(8 단계)로 줄인다. (SkMaskGamma = SkTMaskGamma<3, 3, 3>) 표는 단계마다 하나다.
//! - 감마 값 0 은 sRGB 곡선, 1 은 선형(표가 그대로라 고치지 않는다), 그 밖은 거듭제곱이다. (SkColorSpaceLuminance::Fetch)
//!   크롬의 플랫폼별 값(크로미엄 skia/BUILD.gn 의 SK_GAMMA_EXPONENT, SK_GAMMA_CONTRAST): 맥 sRGB 와 대비 0, 리눅스 1.2 와 0.2, 윈도우 sRGB 와 대비 1(시스템 값이 있으면 그것).
//==============================================================================
// 옮긴 원문: 스키아 src/core/SkMaskGamma.cpp, SkMaskGamma.h, SkColorData.h, SkScalerContext.cpp 에서 옮김, BSD-3-Clause, THIRD_PARTY_LICENSES.md
use std::cell::RefCell;
use std::collections::HashMap;

/// 밝기 단계의 비트 수. (SkMaskGamma 의 채널마다 3 비트)
const LUMINANCE_BITS: u32 = 3;

thread_local! {
	/// (밝기 단계, 대비 비트, 감마 비트) → 전보정 표.
	static CORRECTING_TABLE_CACHE: RefCell<HashMap<(u8, u32, u32), [u8; 256]>> = RefCell::new(HashMap::new());
}

//==============================================================================
/// 밝기 단계를 0 ~ 255 로 넓히기. (스키아 sk_t_scale255<3>)
//==============================================================================
fn scale_luminance_level(level: u32) -> u8 {
	let base = level << (8 - LUMINANCE_BITS);
	let mut luminance = base;
	let mut shift = LUMINANCE_BITS;
	while shift < 8 {
		luminance |= base >> shift;
		shift += LUMINANCE_BITS;
	}
	luminance as u8
}

//==============================================================================
/// 채널 값을 가장 가까운 밝기 단계 값으로. (스키아 SkTMaskGamma::CanonicalColor 의 한 채널)
//==============================================================================
pub(crate) fn canonicalize_channel(value: u8) -> u8 {
	scale_luminance_level((value >> (8 - LUMINANCE_BITS)) as u32)
}

//==============================================================================
/// 색의 밝기. (스키아 SkComputeLuminance, 1.8 고정 소수점 54, 183, 19)
//==============================================================================
pub(crate) fn compute_luminance(red: u8, green: u8, blue: u8) -> u8 {
	((red as u32 * 54 + green as u32 * 183 + blue as u32 * 19) >> 8) as u8
}

//==============================================================================
/// A8 마스크의 밝기 색. (스키아 setLuminanceColor 의 CanonicalColor 뒤 PreprocessRec 이 A8 이면 밝기 하나로 모으고 다시 CanonicalColor)
//==============================================================================
pub(crate) fn resolve_a8_luminance(red: u8, green: u8, blue: u8) -> u8 {
	let luminance = compute_luminance(canonicalize_channel(red), canonicalize_channel(green), canonicalize_channel(blue));
	canonicalize_channel(luminance)
}

//==============================================================================
/// 밝기에서 선형 값으로. (스키아 SkColorSpaceLuminance::toLuma)
//==============================================================================
fn convert_to_linear(gamma: f32, value: f32) -> f32 {
	if gamma == 0.0 {
		if value <= 0.04045 {
			return value / 12.92;
		}
		return ((value + 0.055) / 1.055).powf(2.4);
	}
	if gamma == 1.0 {
		return value;
	}
	value.powf(gamma)
}

//==============================================================================
/// 선형 값에서 밝기로. (스키아 SkColorSpaceLuminance::fromLuma)
//==============================================================================
fn convert_from_linear(gamma: f32, value: f32) -> f32 {
	if gamma == 0.0 {
		if value <= 0.0031308 {
			return value * 12.92;
		}
		return 1.055 * value.powf(1.0 / 2.4) - 0.055;
	}
	if gamma == 1.0 {
		return value;
	}
	value.powf(1.0 / gamma)
}

//==============================================================================
/// 반올림. (스키아 sk_float_round2int, floor(x + 0.5))
//==============================================================================
fn round_to_byte(value: f32) -> u8 {
	(value + 0.5).floor().clamp(0.0, 255.0) as u8
}

//==============================================================================
/// 전보정 표 짓기. (스키아 SkTMaskGamma_build_correcting_lut, 글 색 밝기 source 위에 그 반대 밝기의 바탕을 가정한다)
//==============================================================================
fn build_correcting_table(source_luminance: u8, contrast: f32, gamma: f32) -> [u8; 256] {
	let mut table = [0u8; 256];
	let source = source_luminance as f32 / 255.0;
	let linear_source = convert_to_linear(gamma, source);
	let destination = 1.0 - source;
	let linear_destination = convert_to_linear(gamma, destination);
	let adjusted_contrast = contrast * linear_destination;
	let apply_contrast = |source_alpha: f32| source_alpha + (1.0 - source_alpha) * adjusted_contrast * source_alpha;
	if (source - destination).abs() < 1.0 / 256.0 {
		for (index, entry) in table.iter_mut().enumerate() {
			let source_alpha = apply_contrast(index as f32 / 255.0);
			*entry = round_to_byte(255.0 * source_alpha);
		}
	}
	else {
		for (index, entry) in table.iter_mut().enumerate() {
			let source_alpha = apply_contrast(index as f32 / 255.0);
			let destination_alpha = 1.0 - source_alpha;
			let linear_output = linear_source * source_alpha + destination_alpha * linear_destination;
			let output = convert_from_linear(gamma, linear_output);
			let result = (output - destination) / (source - destination);
			*entry = round_to_byte(255.0 * result);
		}
	}
	table
}

//==============================================================================
/// A8 마스크 전보정 표. (밝기 색의 단계 표, 스키아 SkTMaskGamma::preBlend 의 fG. 대비 0 에 감마 1 이면 None: 고치지 않는다)
/// - `luminance`: resolve_a8_luminance 로 얻은 밝기.
/// - `contrast`: 대비. (0 ~ 1, 스키아 내부 값처럼 1/255 단위로 반올림한다)
/// - `gamma`: 감마. (0 은 sRGB, 스키아 내부 값처럼 1/64 단위로 내린다)
//==============================================================================
pub(crate) fn find_a8_correcting_table(luminance: u8, contrast: f32, gamma: f32) -> Option<[u8; 256]> {
	// 스키아는 레크에 대비를 8 비트(1/255), 감마를 8 비트(1/64)로 담는다. (InternalContrastFromExternal, InternalGammaFromExternal)
	let contrast_bits = (contrast * 255.0 + 0.5) as u32;
	let gamma_bits = (gamma * 64.0) as u32;
	if contrast_bits == 0 && gamma_bits == 64 {
		return None;
	}
	let level = luminance >> (8 - LUMINANCE_BITS);
	CORRECTING_TABLE_CACHE.with(|cache_cell| {
		let mut cache = cache_cell.borrow_mut();
		let table = cache.entry((level, contrast_bits, gamma_bits)).or_insert_with(|| build_correcting_table(scale_luminance_level(level as u32), contrast_bits as f32 / 255.0, gamma_bits as f32 / 64.0));
		Some(*table)
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 밝기 단계와 밝기 셈이 스키아와 같은지. (sk_t_scale255<3>, SkComputeLuminance)
	//==============================================================================
	#[test]
	fn luminance_levels_match_skia() {
		// sk_t_scale255<3>: 3 → 96 | 12 | 1 = 109, 7 → 255.
		assert_eq!(canonicalize_channel(127), 109);
		assert_eq!(canonicalize_channel(255), 255);
		assert_eq!(canonicalize_channel(0), 0);
		assert_eq!(compute_luminance(255, 255, 255), 255);
		assert_eq!(resolve_a8_luminance(127, 127, 127), 109);
	}

	//==============================================================================
	/// 전보정 표의 끝값과 선형 감마, 어두운 글의 보정 방향.
	//==============================================================================
	#[test]
	fn correcting_table_keeps_ends() {
		let table = find_a8_correcting_table(109, 0.0, 0.0).expect("srgb table");
		assert_eq!(table[0], 0);
		assert_eq!(table[255], 255);
		assert!(find_a8_correcting_table(109, 0.0, 1.0).is_none());
		// 어두운 글(밝기 0)은 sRGB 에서 덮인 정도를 줄인다. (바탕이 밝아 선형으로 섞으면 진해지므로)
		let dark_table = find_a8_correcting_table(0, 0.0, 0.0).expect("dark table");
		assert!(dark_table[128] < 128);
	}
}
