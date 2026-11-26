//==============================================================================
//! 그림의 색 프로필을 sRGB 로 바꾸기. (데스크탑 그림 해석, core/image.rs 와 core/avifimage.rs 가 부른다)
//! - 원본은 브라우저가 한다. 크롬은 HTMLImageElement 의 그림을 ColorBehavior::kTag 로 풀어 박힌 색 공간을 그림에 달고,
//!   Canvas2D drawImage 와 WebGL texImage2D 때 캔버스 색 공간(sRGB)으로 바꾼다. 원본 엔진은 WebGL texImage2D(UNPACK_PREMULTIPLY_ALPHA_WEBGL true)로 그림을 올리고,
//!   크롬의 그 길(blink ImageExtractor)은 그림을 첫 장부터 다시 풀어(알파를 곱하지 않고, kTag) SkImage::readPixels 로 sRGB 와 프리멀티플라이드로 바꾼다.
//!   이 파일은 그 길과 같은 셈을 한다. 웹 판은 브라우저가 한다.
//! - 크롬 158 원문:
//!   - blink image_decoder.cc 의 SetEmbeddedColorProfile(kTag 면 박힌 SkColorSpace 를 달고, 그 색 공간이 프로필과 꼭 같지 않으면 해독 때 skcms_Transform 으로 바꾼다)
//!   - skia/ext/color_profile.cc 의 ComputeSkColorSpace(ICC 의 cicp 태그, SkColorSpace::Make, 원색만 살리고 sRGB 전달 함수, 그 밖은 sRGB 차례)
//!   - 스키아 rust/icc/icc_parse.rs(크롬은 SK_CODEC_COLOR_PROFILE_PARSE_WITH_RUST 로 이 해석기를 쓴다, skcms_Parse 와 같은 규칙)
//!   - 스키아 SkColorSpace.cpp(Make, MakeRGB 의 거의 같음 맞추기, MakeCICP), SkColorSpaceXformSteps.cpp(그릴 때 바꾸는 단계), SkRasterPipeline_opts.h(highp 단계의 셈)
//!   - skcms.cc 와 Transform_inl.h(전달 함수, 역함수, 프로필 비교, skcms_Transform 의 단계와 근사 셈)
//!   - 그림 꼴마다 프로필을 고르는 곳: 스키아 SkPngRustCodec.cpp(cICP, iCCP, sRGB, gAMA 와 cHRM 차례), blink jpeg_image_decoder.cc 와 SkJpegMetadataDecoderImpl.cpp(APP2 ICC_PROFILE 조각),
//!     blink webp_image_decoder.cc(VP8X 의 ICC 표시, 움직이는 WebP 는 무시), blink avif_image_decoder.cc 와 ui/gfx/color_space.cc(AVIF 의 colr ICC 와 nclx)
//! - 셈의 꼴: 크롬 맥(arm64)처럼 래스터 파이프라인의 mad 는 융합 곱셈 덧셈(vfmaq_f32), 반올림은 짝수 쪽(vcvtnq), skcms 는 융합 없이(-ffp-contract=off) 반올림을 +0.5 버림으로 한다.
//! - 그림 하나의 픽셀을 WebGL 과 Canvas2D 가 함께 쓰므로 WebGL 길에 맞춘다. 크롬 Canvas2D drawImage 는 알파가 있는 그림을 알파를 곱한 8 비트로 푼 뒤 바꾸고,
//!   PQ, HLG 그림은 캔버스에 더 어둡게 그려(크롬 158 에서 PQ 의 회색 128 이 WebGL 181, Canvas2D 132) 그 둘은 Canvas2D 에서 다르다.
//! - CMYK JPEG 의 CMYK 프로필: 블링크처럼 화소를 R = C x K / 255 로 먼저 바꾼 RGBA 를 skcms_Transform 처럼 CMYK 로 읽어(Op::invert 로 네 값을 1 에서 뺀다, 알파 자리가 K) A2B 로 바꾼다. (core/jpegimage.rs)
//! - 원본과 다른 것: BMP 와 ICO 안의 색 프로필은 읽지 않는다.
//==============================================================================

/// 전달 함수. (skcms_TransferFunction, 늘어선 차례 g a b c d e f)
/// - sRGB 꼴은 x < d 이면 c x + f, 아니면 (a x + b)^g + e 다. g 가 음수면 PQ, HLG 같은 꼴을 가리키는 표시다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TransferFunction {
	pub(crate) g: f32,
	pub(crate) a: f32,
	pub(crate) b: f32,
	pub(crate) c: f32,
	pub(crate) d: f32,
	pub(crate) e: f32,
	pub(crate) f: f32,
}

impl TransferFunction {
	//==============================================================================
	/// 일곱 값으로 생성. (g a b c d e f 차례)
	//==============================================================================
	const fn new(g: f32, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> TransferFunction {
		TransferFunction {
			g,
			a,
			b,
			c,
			d,
			e,
			f,
		}
	}

	//==============================================================================
	/// 일곱 값의 비트. (스키아는 해시로 같음을 본다, -0 과 0 도 다르다)
	//==============================================================================
	fn to_bits(&self) -> [u32; 7] {
		[self.g.to_bits(), self.a.to_bits(), self.b.to_bits(), self.c.to_bits(), self.d.to_bits(), self.e.to_bits(), self.f.to_bits()]
	}

	//==============================================================================
	/// 비트까지 같은지.
	//==============================================================================
	fn is_bitwise_equal(&self, other: &TransferFunction) -> bool {
		self.to_bits() == other.to_bits()
	}
}

/// sRGB 전달 함수. (SkNamedTransferFn::kSRGB, skcms 의 sRGB 와 비트까지 같다)
const SRGB_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(2.4, (1.0f64 / 1.055) as f32, (0.055f64 / 1.055) as f32, (1.0f64 / 12.92) as f32, 0.04045, 0.0, 0.0);
/// 선형. (SkNamedTransferFn::kLinear)
const LINEAR_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
/// 감마 2.2. (SkNamedTransferFn::k2Dot2)
const GAMMA_2_2_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(2.2, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
/// BT.709, BT.601 의 BT.1886 감마 2.4. (SkNamedTransferFn::kRec709)
const REC709_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(2.4, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
/// BT.470 M 감마 2.2. (SkNamedTransferFn::kRec470SystemM)
const REC470_SYSTEM_M_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(2.2, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
/// BT.470 B, G 감마 2.8. (SkNamedTransferFn::kRec470SystemBG)
const REC470_SYSTEM_BG_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(2.8, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
/// SMPTE ST 240. (SkNamedTransferFn::kSMPTE_ST_240)
const SMPTE_ST_240_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(2.222222222222, 0.899626676224, 0.100373323776, 0.25, 0.091286342118, 0.0, 0.0);
/// SMPTE ST 428-1. (SkNamedTransferFn::kSMPTE_ST_428_1)
const SMPTE_ST_428_1_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(2.6, 1.034080527699, 0.0, 0.0, 0.0, 0.0, 0.0);
/// PQ, 기준 흰색 203 니트. (SkNamedTransferFn::kPQ)
const PQ_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(-5.0, 203.0, 0.0, 0.0, 0.0, 0.0, 0.0);
/// HLG, 기준 흰색 203 니트, 정점 1000 니트, 시스템 감마 1.2. (SkNamedTransferFn::kHLG)
const HLG_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(-6.0, 203.0, 1000.0, 1.2, 0.0, 0.0, 0.0);
/// sRGB 역함수. (skcms_sRGB_Inverse_TransferFunction, 역함수를 구하지 못할 때 쓴다)
const SRGB_INVERSE_TRANSFER_FUNCTION: TransferFunction = TransferFunction::new(0.416666657, 1.137283325, -0.0, 12.920000076, 0.003130805, -0.054969788, -0.0);
/// 그릴 때 PQ 를 펴는 함수. (SkColorSpaceXformSteps 의 kPQish)
const PQ_LIKE_STEP_FUNCTION: TransferFunction = TransferFunction::new(-2.0, -107.0 / 128.0, 1.0, 32.0 / 2523.0, 2413.0 / 128.0, -2392.0 / 128.0, 8192.0 / 1305.0);
/// 그릴 때 HLG 를 펴는 함수. (SkColorSpaceXformSteps 의 kHLGish, f 는 1/12 - 1 로 바꿔 쓴다)
const HLG_LIKE_STEP_FUNCTION: TransferFunction = TransferFunction::new(-3.0, 2.0, 2.0, 1.0 / 0.17883277, 0.28466892, 0.55991073, 0.0);

/// 3 x 3 행렬. (줄 먼저, toXYZD50 은 RGB 를 D50 XYZ 로)
pub(crate) type Matrix3x3 = [[f32; 3]; 3];

//==============================================================================
/// 16.16 고정 소수 값으로 행렬 만들기. (SkFixedToFloat)
//==============================================================================
const fn fixed_matrix(values: [[i32; 3]; 3]) -> Matrix3x3 {
	[
		[values[0][0] as f32 / 65536.0, values[0][1] as f32 / 65536.0, values[0][2] as f32 / 65536.0],
		[values[1][0] as f32 / 65536.0, values[1][1] as f32 / 65536.0, values[1][2] as f32 / 65536.0],
		[values[2][0] as f32 / 65536.0, values[2][1] as f32 / 65536.0, values[2][2] as f32 / 65536.0],
	]
}

/// sRGB 원색. (SkNamedGamut::kSRGB, skcms 의 sRGB 와 같다)
const SRGB_GAMUT: Matrix3x3 = fixed_matrix([[0x6FA2, 0x6299, 0x24A0], [0x38F5, 0xB785, 0x0F84], [0x0390, 0x18DA, 0xB6CF]]);
/// Display P3 원색. (SkNamedGamut::kDisplayP3)
const DISPLAY_P3_GAMUT: Matrix3x3 = [[0.515102, 0.291965, 0.157153], [0.241182, 0.692236, 0.0665819], [-0.00104941, 0.0418818, 0.784378]];
/// BT.2020 원색. (SkNamedGamut::kRec2020)
const REC2020_GAMUT: Matrix3x3 = [[0.673459, 0.165661, 0.125100], [0.279033, 0.675338, 0.0456288], [-0.00193139, 0.0299794, 0.797162]];
/// 단위 행렬. (SkNamedGamut::kXYZ)
const IDENTITY_MATRIX: Matrix3x3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// 원색의 xy 좌표. (SkColorSpacePrimaries, 빨강 x y, 초록 x y, 파랑 x y, 흰색 x y)
type ColorPrimaries = [f32; 8];
/// BT.709. (SkNamedPrimaries::kRec709, CICP 1)
const REC709_PRIMARIES: ColorPrimaries = [0.64, 0.33, 0.3, 0.6, 0.15, 0.06, 0.3127, 0.329];
/// BT.470 M. (CICP 4)
const REC470_SYSTEM_M_PRIMARIES: ColorPrimaries = [0.67, 0.33, 0.21, 0.71, 0.14, 0.08, 0.31, 0.316];
/// BT.470 B, G. (CICP 5)
const REC470_SYSTEM_BG_PRIMARIES: ColorPrimaries = [0.64, 0.33, 0.29, 0.60, 0.15, 0.06, 0.3127, 0.3290];
/// BT.601. (CICP 6, SMPTE ST 240 인 7 도 같다)
const REC601_PRIMARIES: ColorPrimaries = [0.630, 0.340, 0.310, 0.595, 0.155, 0.070, 0.3127, 0.3290];
/// 필름. (CICP 8)
const GENERIC_FILM_PRIMARIES: ColorPrimaries = [0.681, 0.319, 0.243, 0.692, 0.145, 0.049, 0.310, 0.316];
/// BT.2020. (CICP 9)
const REC2020_PRIMARIES: ColorPrimaries = [0.708, 0.292, 0.170, 0.797, 0.131, 0.046, 0.3127, 0.3290];
/// SMPTE ST 428-1. (CICP 10)
const SMPTE_ST_428_1_PRIMARIES: ColorPrimaries = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0 / 3.0, 1.0 / 3.0];
/// SMPTE RP 431-2. (CICP 11)
const SMPTE_RP_431_2_PRIMARIES: ColorPrimaries = [0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.314, 0.351];
/// SMPTE EG 432-1. (CICP 12, Display P3)
const SMPTE_EG_432_1_PRIMARIES: ColorPrimaries = [0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.3127, 0.3290];
/// H.273 의 22. (CICP 22)
const ITU_T_H273_VALUE22_PRIMARIES: ColorPrimaries = [0.630, 0.340, 0.295, 0.605, 0.155, 0.077, 0.3127, 0.3290];

/// CICP 원색 표. (SkNamedPrimaries cicp_table, 번호, xy, 이름 붙은 행렬이 있으면 그것)
const CICP_PRIMARIES_TABLE: [(u8, ColorPrimaries, Option<Matrix3x3>); 11] = [
	(1, REC709_PRIMARIES, Some(SRGB_GAMUT)),
	(4, REC470_SYSTEM_M_PRIMARIES, None),
	(5, REC470_SYSTEM_BG_PRIMARIES, None),
	(6, REC601_PRIMARIES, None),
	(7, REC601_PRIMARIES, None),
	(8, GENERIC_FILM_PRIMARIES, None),
	(9, REC2020_PRIMARIES, Some(REC2020_GAMUT)),
	(10, SMPTE_ST_428_1_PRIMARIES, None),
	(11, SMPTE_RP_431_2_PRIMARIES, None),
	(12, SMPTE_EG_432_1_PRIMARIES, Some(DISPLAY_P3_GAMUT)),
	(22, ITU_T_H273_VALUE22_PRIMARIES, None),
];

/// CICP 전달 특성 표. (SkNamedTransferFn cicp_table)
const CICP_TRANSFER_TABLE: [(u8, TransferFunction); 13] = [
	(1, REC709_TRANSFER_FUNCTION),
	(4, REC470_SYSTEM_M_TRANSFER_FUNCTION),
	(5, REC470_SYSTEM_BG_TRANSFER_FUNCTION),
	(6, REC709_TRANSFER_FUNCTION),
	(7, SMPTE_ST_240_TRANSFER_FUNCTION),
	(8, LINEAR_TRANSFER_FUNCTION),
	(11, REC709_TRANSFER_FUNCTION),
	(13, SRGB_TRANSFER_FUNCTION),
	(14, REC709_TRANSFER_FUNCTION),
	(15, REC709_TRANSFER_FUNCTION),
	(16, PQ_TRANSFER_FUNCTION),
	(17, SMPTE_ST_428_1_TRANSFER_FUNCTION),
	(18, HLG_TRANSFER_FUNCTION),
];

/// ICC 태그와 형의 이름. (네 글자를 큰 쪽부터 읽은 값)
const SIGNATURE_ACSP: u32 = u32::from_be_bytes(*b"acsp");
const SIGNATURE_RED_TRC: u32 = u32::from_be_bytes(*b"rTRC");
const SIGNATURE_GREEN_TRC: u32 = u32::from_be_bytes(*b"gTRC");
const SIGNATURE_BLUE_TRC: u32 = u32::from_be_bytes(*b"bTRC");
const SIGNATURE_GRAY_TRC: u32 = u32::from_be_bytes(*b"kTRC");
const SIGNATURE_RED_XYZ: u32 = u32::from_be_bytes(*b"rXYZ");
const SIGNATURE_GREEN_XYZ: u32 = u32::from_be_bytes(*b"gXYZ");
const SIGNATURE_BLUE_XYZ: u32 = u32::from_be_bytes(*b"bXYZ");
const SIGNATURE_A2B0: u32 = u32::from_be_bytes(*b"A2B0");
const SIGNATURE_B2A0: u32 = u32::from_be_bytes(*b"B2A0");
const SIGNATURE_CICP: u32 = u32::from_be_bytes(*b"cicp");
const SIGNATURE_HAGC: u32 = u32::from_be_bytes(*b"HAGC");
const SIGNATURE_HAGC_TYPE: u32 = u32::from_be_bytes(*b"hagc");
const SIGNATURE_CURV: u32 = u32::from_be_bytes(*b"curv");
const SIGNATURE_PARA: u32 = u32::from_be_bytes(*b"para");
const SIGNATURE_MFT1: u32 = u32::from_be_bytes(*b"mft1");
const SIGNATURE_MFT2: u32 = u32::from_be_bytes(*b"mft2");
const SIGNATURE_MAB: u32 = u32::from_be_bytes(*b"mAB ");
const SIGNATURE_MBA: u32 = u32::from_be_bytes(*b"mBA ");
const SIGNATURE_XYZ: u32 = u32::from_be_bytes(*b"XYZ ");
const SIGNATURE_LAB: u32 = u32::from_be_bytes(*b"Lab ");
const SIGNATURE_GRAY: u32 = u32::from_be_bytes(*b"GRAY");
const SIGNATURE_RGB: u32 = u32::from_be_bytes(*b"RGB ");
const SIGNATURE_CMYK: u32 = u32::from_be_bytes(*b"CMYK");

/// 프로필 비교에 쓰는 바이트 252 개. (skcms_252_random_bytes)
const RANDOM_BYTES: [u8; 252] = [
	8, 179, 128, 204, 253, 38, 134, 184, 68, 102, 32, 138, 99, 39, 169, 215,
	119, 26, 3, 223, 95, 239, 52, 132, 114, 74, 81, 234, 97, 116, 244, 205, 30,
	154, 173, 12, 51, 159, 122, 153, 61, 226, 236, 178, 229, 55, 181, 220, 191,
	194, 160, 126, 168, 82, 131, 18, 180, 245, 163, 22, 246, 69, 235, 252, 57,
	108, 14, 6, 152, 240, 255, 171, 242, 20, 227, 177, 238, 96, 85, 16, 211,
	70, 200, 149, 155, 146, 127, 145, 100, 151, 109, 19, 165, 208, 195, 164,
	137, 254, 182, 248, 64, 201, 45, 209, 5, 147, 207, 210, 113, 162, 83, 225,
	9, 31, 15, 231, 115, 37, 58, 53, 24, 49, 197, 56, 120, 172, 48, 21, 214,
	129, 111, 11, 50, 187, 196, 34, 60, 103, 71, 144, 47, 203, 77, 80, 232,
	140, 222, 250, 206, 166, 247, 139, 249, 221, 72, 106, 27, 199, 117, 54,
	219, 135, 118, 40, 79, 41, 251, 46, 93, 212, 92, 233, 148, 28, 121, 63,
	123, 158, 105, 59, 29, 42, 143, 23, 0, 107, 176, 87, 104, 183, 156, 193,
	189, 90, 188, 65, 190, 17, 198, 7, 186, 161, 1, 124, 78, 125, 170, 133,
	174, 218, 67, 157, 75, 101, 89, 217, 62, 33, 141, 228, 25, 35, 91, 230, 4,
	2, 13, 73, 86, 167, 237, 84, 243, 44, 185, 66, 130, 110, 150, 142, 216, 88,
	112, 36, 224, 136, 202, 76, 94, 98, 175, 213,
];

//==============================================================================
/// 전달 함수의 꼴. (skcms_TFType, 값은 음수 g 표시와 같다)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TransferFunctionType {
	Invalid = 0,
	SrgbLike = 1,
	PqLike = 2,
	HlgLike = 3,
	HlgInverseLike = 4,
	Pq = 5,
	Hlg = 6,
}

//==============================================================================
/// 전달 함수의 꼴 가리기. (skcms classify)
//==============================================================================
fn classify_transfer_function(function: &TransferFunction) -> TransferFunctionType {
	if function.g < 0.0 {
		if function.g < -128.0 {
			return TransferFunctionType::Invalid;
		}
		let enum_value = -(function.g as i32);
		if (-enum_value) as f32 != function.g {
			return TransferFunctionType::Invalid;
		}
		return match enum_value {
			2 => {
				TransferFunctionType::PqLike
			}
			3 => {
				TransferFunctionType::HlgLike
			}
			4 => {
				TransferFunctionType::HlgInverseLike
			}
			5 => {
				if function.b != 0.0 || function.c != 0.0 || function.d != 0.0 || function.e != 0.0 || function.f != 0.0 {
					TransferFunctionType::Invalid
				}
				else {
					TransferFunctionType::Pq
				}
			}
			6 => {
				if function.d != 0.0 || function.e != 0.0 || function.f != 0.0 {
					TransferFunctionType::Invalid
				}
				else {
					TransferFunctionType::Hlg
				}
			}
			_ => {
				TransferFunctionType::Invalid
			}
		};
	}
	let sum = function.a + function.b + function.c + function.d + function.e + function.f + function.g;
	if sum * 0.0 == 0.0 && function.a >= 0.0 && function.c >= 0.0 && function.d >= 0.0 && function.g >= 0.0 && function.a * function.d + function.b >= 0.0 {
		return TransferFunctionType::SrgbLike;
	}
	TransferFunctionType::Invalid
}

//==============================================================================
/// skcms 의 근사 log2. (skcms.cc log2f_, 융합하지 않는다)
//==============================================================================
fn skcms_log2(x: f32) -> f32 {
	let bits = x.to_bits() as i32;
	let exponent = bits as f32 * (1.0 / (1u32 << 23) as f32);
	let mantissa = f32::from_bits(((bits & 0x007fffff) | 0x3f000000) as u32);
	exponent - 124.225514990 - 1.498030302 * mantissa - 1.725879990 / (0.3520887068 + mantissa)
}

//==============================================================================
/// skcms 의 바닥. (skcms_internals.h floorf_)
//==============================================================================
fn skcms_floor(x: f32) -> f32 {
	let roundtrip = (x as i32) as f32;
	if roundtrip > x {
		roundtrip - 1.0
	}
	else {
		roundtrip
	}
}

//==============================================================================
/// skcms 의 근사 2^x. (skcms.cc exp2f_, 범위 밖은 무한이나 0)
//==============================================================================
fn skcms_exp2(x: f32) -> f32 {
	if x > 128.0 {
		return f32::INFINITY;
	}
	else if x < -127.0 {
		return 0.0;
	}
	let fraction = x - skcms_floor(x);
	let float_bits = (1u32 << 23) as f32 * (x + 121.274057500 - 1.490129070 * fraction + 27.728023300 / (4.84252568 - fraction));
	if float_bits >= i32::MAX as f32 {
		return f32::INFINITY;
	}
	else if float_bits < 0.0 {
		return 0.0;
	}
	f32::from_bits((float_bits as i32) as u32)
}

//==============================================================================
/// skcms 의 근사 거듭제곱. (skcms.cc powf_, 0 이하는 0, 1 은 1)
//==============================================================================
fn skcms_pow(x: f32, y: f32) -> f32 {
	if x <= 0.0 {
		return 0.0;
	}
	if x == 1.0 {
		return 1.0;
	}
	skcms_exp2(skcms_log2(x) * y)
}

//==============================================================================
/// 전달 함수 셈. (skcms_TransferFunction_eval, 음수는 부호를 떼고 셈해 붙인다)
//==============================================================================
fn evaluate_transfer_function(function: &TransferFunction, value: f32) -> f32 {
	let sign = if value < 0.0 { -1.0 } else { 1.0 };
	let x = value * sign;
	match classify_transfer_function(function) {
		TransferFunctionType::Invalid => {
			0.0
		}
		TransferFunctionType::Hlg => {
			let a = 0.17883277f32;
			let b = 0.28466892f32;
			let c = 0.55991073f32;
			if x <= 0.5 {
				sign * (x * x / 3.0)
			}
			else {
				sign * ((skcms_exp2(1.4426950408889634074 * ((x - c) / a)) + b) / 12.0)
			}
		}
		TransferFunctionType::HlgLike => {
			let scale = function.f + 1.0;
			let curve = if x * function.a <= 1.0 { skcms_pow(x * function.a, function.b) } else { skcms_exp2(1.4426950408889634074 * ((x - function.e) * function.c)) + function.d };
			scale * sign * curve
		}
		TransferFunctionType::HlgInverseLike => {
			let scale = function.f + 1.0;
			let scaled = x / scale;
			if scaled <= 1.0 {
				sign * (function.a * skcms_pow(scaled, function.b))
			}
			else {
				sign * (function.c * (0.69314718 * skcms_log2(scaled - function.d)) + function.e)
			}
		}
		TransferFunctionType::SrgbLike => {
			if x < function.d {
				sign * (function.c * x + function.f)
			}
			else {
				sign * (skcms_pow(function.a * x + function.b, function.g) + function.e)
			}
		}
		TransferFunctionType::Pq => {
			let c1 = 107.0f32 / 128.0;
			let c2 = 2413.0f32 / 128.0;
			let c3 = 2392.0f32 / 128.0;
			let m1 = 1305.0f32 / 8192.0;
			let m2 = 2523.0f32 / 32.0;
			let p = skcms_pow(x, 1.0 / m2);
			skcms_pow((p - c1) / (c2 - c3 * p), 1.0 / m1)
		}
		TransferFunctionType::PqLike => {
			sign * skcms_pow((function.a + function.b * skcms_pow(x, function.c)) / (function.d + function.e * skcms_pow(x, function.c)), function.f)
		}
	}
}

//==============================================================================
/// 전달 함수의 역함수. (skcms_TransferFunction_invert, 구하지 못하면 None)
//==============================================================================
fn invert_transfer_function(source: &TransferFunction) -> Option<TransferFunction> {
	match classify_transfer_function(source) {
		TransferFunctionType::Invalid | TransferFunctionType::Pq | TransferFunctionType::Hlg => {
			return None;
		}
		TransferFunctionType::PqLike => {
			return Some(TransferFunction::new(-(TransferFunctionType::PqLike as i32 as f32), -source.a, source.d, 1.0 / source.f, source.b, -source.e, 1.0 / source.c));
		}
		TransferFunctionType::HlgLike => {
			return Some(TransferFunction::new(-(TransferFunctionType::HlgInverseLike as i32 as f32), 1.0 / source.a, 1.0 / source.b, 1.0 / source.c, source.d, source.e, source.f));
		}
		TransferFunctionType::HlgInverseLike => {
			return Some(TransferFunction::new(-(TransferFunctionType::HlgLike as i32 as f32), 1.0 / source.a, 1.0 / source.b, 1.0 / source.c, source.d, source.e, source.f));
		}
		TransferFunctionType::SrgbLike => {
		}
	}
	let mut inverse = TransferFunction::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
	let left_value = source.c * source.d + source.f;
	let right_value = skcms_pow(source.a * source.d + source.b, source.g) + source.e;
	let difference = left_value - right_value;
	let absolute_difference = if difference < 0.0 { -difference } else { difference };
	if absolute_difference > 1.0 / 512.0 {
		return None;
	}
	inverse.d = left_value;
	if inverse.d > 0.0 {
		inverse.c = 1.0 / source.c;
		inverse.f = -source.f / source.c;
	}
	let scale = skcms_pow(source.a, -source.g);
	inverse.g = 1.0 / source.g;
	inverse.a = scale;
	inverse.b = -scale * source.e;
	inverse.e = -source.b / source.a;
	if inverse.a < 0.0 {
		return None;
	}
	if inverse.a * inverse.d + inverse.b < 0.0 {
		inverse.b = -inverse.a * inverse.d;
	}
	if classify_transfer_function(&inverse) != TransferFunctionType::SrgbLike {
		return None;
	}
	let mut source_at_one = evaluate_transfer_function(source, 1.0);
	if source_at_one * 0.0 != 0.0 {
		return None;
	}
	let sign = if source_at_one < 0.0 { -1.0 } else { 1.0 };
	source_at_one *= sign;
	if source_at_one < inverse.d {
		inverse.f = 1.0 - sign * inverse.c * source_at_one;
	}
	else {
		inverse.e = 1.0 - sign * skcms_pow(inverse.a * source_at_one + inverse.b, inverse.g);
	}
	if classify_transfer_function(&inverse) != TransferFunctionType::SrgbLike {
		return None;
	}
	Some(inverse)
}

//==============================================================================
/// 3 x 3 행렬 역행렬. (skcms_Matrix3x3_invert, f64 로 셈한다, 구하지 못하면 None)
//==============================================================================
fn invert_matrix(source: &Matrix3x3) -> Option<Matrix3x3> {
	let a00 = source[0][0] as f64;
	let a01 = source[1][0] as f64;
	let a02 = source[2][0] as f64;
	let a10 = source[0][1] as f64;
	let a11 = source[1][1] as f64;
	let a12 = source[2][1] as f64;
	let a20 = source[0][2] as f64;
	let a21 = source[1][2] as f64;
	let a22 = source[2][2] as f64;
	let mut b0 = a00 * a11 - a01 * a10;
	let mut b1 = a00 * a12 - a02 * a10;
	let mut b2 = a01 * a12 - a02 * a11;
	let mut b3 = a20;
	let mut b4 = a21;
	let mut b5 = a22;
	let determinant = b0 * b5 - b1 * b4 + b2 * b3;
	if determinant == 0.0 {
		return None;
	}
	let inverse_determinant = 1.0 / determinant;
	if inverse_determinant > f32::MAX as f64 || inverse_determinant < -(f32::MAX as f64) || !(inverse_determinant as f32).is_finite() {
		return None;
	}
	b0 *= inverse_determinant;
	b1 *= inverse_determinant;
	b2 *= inverse_determinant;
	b3 *= inverse_determinant;
	b4 *= inverse_determinant;
	b5 *= inverse_determinant;
	let mut inverse = [[0.0f32; 3]; 3];
	inverse[0][0] = (a11 * b5 - a12 * b4) as f32;
	inverse[1][0] = (a02 * b4 - a01 * b5) as f32;
	inverse[2][0] = b2 as f32;
	inverse[0][1] = (a12 * b3 - a10 * b5) as f32;
	inverse[1][1] = (a00 * b5 - a02 * b3) as f32;
	inverse[2][1] = (-b1) as f32;
	inverse[0][2] = (a10 * b4 - a11 * b3) as f32;
	inverse[1][2] = (a01 * b3 - a00 * b4) as f32;
	inverse[2][2] = b0 as f32;
	if !is_finite_matrix(&inverse) {
		return None;
	}
	Some(inverse)
}

//==============================================================================
/// 행렬의 값이 모두 유한한지. (skcms_Matrix3x3_isfinite)
//==============================================================================
fn is_finite_matrix(matrix: &Matrix3x3) -> bool {
	matrix.iter().all(|row| row.iter().all(|value| *value * 0.0 == 0.0))
}

//==============================================================================
/// 행렬 곱 A x B. (skcms_Matrix3x3_concat)
//==============================================================================
fn concatenate_matrix(left: &Matrix3x3, right: &Matrix3x3) -> Matrix3x3 {
	let mut product = [[0.0f32; 3]; 3];
	for row in 0..3 {
		for column in 0..3 {
			product[row][column] = left[row][0] * right[0][column] + left[row][1] * right[1][column] + left[row][2] * right[2][column];
		}
	}
	product
}

//==============================================================================
/// 행렬 x 벡터. (skcms mv_mul)
//==============================================================================
fn multiply_matrix_vector(matrix: &Matrix3x3, vector: &[f32; 3]) -> [f32; 3] {
	let mut result = [0.0f32; 3];
	for row in 0..3 {
		result[row] = matrix[row][0] * vector[0] + matrix[row][1] * vector[1] + matrix[row][2] * vector[2];
	}
	result
}

//==============================================================================
/// 0 과 1 사이인지. (skcms is_zero_to_one)
//==============================================================================
fn is_zero_to_one(value: f32) -> bool {
	0.0 <= value && value <= 1.0
}

//==============================================================================
/// 흰색 xy 를 D50 으로 옮기는 브래드퍼드 행렬. (skcms_AdaptToXYZD50, 못 구하면 None)
//==============================================================================
fn adapt_to_xyzd50(white_x: f32, white_y: f32) -> Option<Matrix3x3> {
	if !is_zero_to_one(white_x) || !is_zero_to_one(white_y) || white_y == 0.0 {
		return None;
	}
	let white_xyz = [white_x / white_y, 1.0, (1.0 - white_x - white_y) / white_y];
	let white_xyzd50 = [0.96422f32, 1.0, 0.82521];
	let xyz_to_lms: Matrix3x3 = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
	let lms_to_xyz: Matrix3x3 = [[0.9869929, -0.1470543, 0.1599627], [0.4323053, 0.5183603, 0.0492912], [-0.0085287, 0.0400428, 0.9684867]];
	let source_cone = multiply_matrix_vector(&xyz_to_lms, &white_xyz);
	let destination_cone = multiply_matrix_vector(&xyz_to_lms, &white_xyzd50);
	if source_cone[0] == 0.0 || source_cone[1] == 0.0 || source_cone[2] == 0.0 {
		return None;
	}
	let mut matrix: Matrix3x3 = [[destination_cone[0] / source_cone[0], 0.0, 0.0], [0.0, destination_cone[1] / source_cone[1], 0.0], [0.0, 0.0, destination_cone[2] / source_cone[2]]];
	matrix = concatenate_matrix(&matrix, &xyz_to_lms);
	matrix = concatenate_matrix(&lms_to_xyz, &matrix);
	if !is_finite_matrix(&matrix) {
		return None;
	}
	Some(matrix)
}

//==============================================================================
/// 원색 xy 로 toXYZD50 행렬 만들기. (skcms_PrimariesToXYZD50, 못 구하면 None)
//==============================================================================
fn primaries_to_xyzd50(primaries: &ColorPrimaries) -> Option<Matrix3x3> {
	let [red_x, red_y, green_x, green_y, blue_x, blue_y, white_x, white_y] = *primaries;
	if !is_zero_to_one(red_x) || !is_zero_to_one(red_y) || !is_zero_to_one(green_x) || !is_zero_to_one(green_y) || !is_zero_to_one(blue_x) || !is_zero_to_one(blue_y) || !is_zero_to_one(white_x) || !is_zero_to_one(white_y) || white_y == 0.0 {
		return None;
	}
	let primary_matrix: Matrix3x3 = [[red_x, green_x, blue_x], [red_y, green_y, blue_y], [1.0 - red_x - red_y, 1.0 - green_x - green_y, 1.0 - blue_x - blue_y]];
	let primary_inverse = invert_matrix(&primary_matrix)?;
	let white_xyz = [white_x / white_y, 1.0, (1.0 - white_x - white_y) / white_y];
	let xyz = multiply_matrix_vector(&primary_inverse, &white_xyz);
	let mut to_xyz: Matrix3x3 = [[xyz[0], 0.0, 0.0], [0.0, xyz[1], 0.0], [0.0, 0.0, xyz[2]]];
	to_xyz = concatenate_matrix(&primary_matrix, &to_xyz);
	let adaptation = adapt_to_xyzd50(white_x, white_y)?;
	let matrix = concatenate_matrix(&adaptation, &to_xyz);
	if !is_finite_matrix(&matrix) {
		return None;
	}
	Some(matrix)
}

//==============================================================================
/// 행렬의 비트. (스키아는 해시로 같음을 본다)
//==============================================================================
fn matrix_bits(matrix: &Matrix3x3) -> [u32; 9] {
	let mut bits = [0u32; 9];
	for row in 0..3 {
		for column in 0..3 {
			bits[row * 3 + column] = matrix[row][column].to_bits();
		}
	}
	bits
}

//==============================================================================
/// 두 행렬이 거의 같은지. (SkColorSpacePriv xyz_almost_equal, 0.01 안)
//==============================================================================
fn is_almost_equal_matrix(left: &Matrix3x3, right: &Matrix3x3) -> bool {
	(0..3).all(|row| (0..3).all(|column| (left[row][column] - right[row][column]).abs() < 0.01))
}

//==============================================================================
/// 두 전달 함수 값이 거의 같은지. (SkColorSpacePriv transfer_fn_almost_equal, 0.001 안)
//==============================================================================
fn is_almost_equal_value(left: f32, right: f32) -> bool {
	(left - right).abs() < 0.001
}

//==============================================================================
/// 곡선. (skcms_Curve, 매개 변수 곡선이나 8 비트, 16 비트 표)
//==============================================================================
#[derive(Clone, Debug)]
enum Curve {
	Parametric(TransferFunction),
	Table8(Vec<u8>),
	Table16(Vec<u16>),
}

impl Curve {
	//==============================================================================
	/// 표의 칸 수. (매개 변수 곡선은 0)
	//==============================================================================
	fn get_table_entry_count(&self) -> usize {
		match self {
			Curve::Parametric(_) => {
				0
			}
			Curve::Table8(table) => {
				table.len()
			}
			Curve::Table16(table) => {
				table.len()
			}
		}
	}

	//==============================================================================
	/// 표의 한 칸. (0 과 1 사이)
	//==============================================================================
	fn read_table_entry(&self, index: usize) -> f32 {
		match self {
			Curve::Parametric(_) => {
				0.0
			}
			Curve::Table8(table) => {
				table[index] as f32 * (1.0 / 255.0)
			}
			Curve::Table16(table) => {
				table[index] as f32 * (1.0 / 65535.0)
			}
		}
	}

	//==============================================================================
	/// 곡선 셈. (skcms.cc eval_curve, 표는 이웃 칸 사이를 선형으로 잇는다)
	//==============================================================================
	fn evaluate(&self, value: f32) -> f32 {
		if let Curve::Parametric(function) = self {
			return evaluate_transfer_function(function, value);
		}
		let entry_count = self.get_table_entry_count();
		let clamped = if value < 1.0 { value } else { 1.0 };
		let clamped = if 0.0 > clamped { 0.0 } else { clamped };
		let index = clamped * (entry_count - 1) as f32;
		let low = index as i32 as usize;
		let high = (f32::from_bits((index + 1.0).to_bits() - 1)) as i32 as usize;
		let fraction = index - low as f32;
		let low_value = self.read_table_entry(low);
		let high_value = self.read_table_entry(high);
		low_value + (high_value - low_value) * fraction
	}

	//==============================================================================
	/// 역함수와 되돌린 오차의 최댓값. (skcms_MaxRoundtripError)
	//==============================================================================
	fn get_maximum_roundtrip_error(&self, inverse_function: &TransferFunction) -> f32 {
		let entry_count = self.get_table_entry_count().max(256);
		let step = 1.0 / (entry_count - 1) as f32;
		let mut error = 0.0f32;
		for index in 0..entry_count {
			let x = index as f32 * step;
			let y = self.evaluate(x);
			let difference = x - evaluate_transfer_function(inverse_function, y);
			let absolute_difference = if difference < 0.0 { -difference } else { difference };
			error = if error > absolute_difference { error } else { absolute_difference };
		}
		error
	}
}

//==============================================================================
/// 색 찾기 표 변환. (skcms_A2B 와 skcms_B2A, A 곡선, CLUT, M 곡선, 행렬, B 곡선)
//==============================================================================
#[derive(Clone, Debug)]
struct LookupTransform {
	input_curves: Vec<Curve>,
	/// CLUT 의 입력 수. (0 이면 CLUT 를 건너뛴다)
	input_channel_count: usize,
	grid_points: [u8; 4],
	/// CLUT 값. (8 비트는 그대로, 16 비트는 큰 쪽부터 읽은 값)
	grid_8: Vec<u8>,
	grid_16: Vec<u16>,
	matrix_curves: Vec<Curve>,
	/// 3 x 4 행렬. (줄마다 세 값과 치우침)
	matrix: [[f32; 4]; 3],
	matrix_channel_count: usize,
	output_curves: Vec<Curve>,
	output_channel_count: usize,
}

//==============================================================================
/// 읽은 ICC 프로필. (skcms_ICCProfile 가운데 원본에서 그림을 바꾸는 데 쓰는 것)
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) struct IccProfile {
	data_color_space: u32,
	connection_space: u32,
	to_xyzd50: Option<Matrix3x3>,
	transfer_curves: Option<[Curve; 3]>,
	lookup_transform: Option<LookupTransform>,
	/// cicp 태그. (원색, 전달 특성, 행렬 계수, 전체 범위)
	coding_independent_code_points: Option<[u8; 4]>,
}

impl IccProfile {
	//==============================================================================
	/// 빈 RGB 프로필. (skcms_Init)
	//==============================================================================
	fn new() -> IccProfile {
		IccProfile {
			data_color_space: SIGNATURE_RGB,
			connection_space: SIGNATURE_XYZ,
			to_xyzd50: None,
			transfer_curves: None,
			lookup_transform: None,
			coding_independent_code_points: None,
		}
	}

	//==============================================================================
	/// 세 곡선을 같은 전달 함수로. (skcms_SetTransferFunction)
	//==============================================================================
	fn set_transfer_function(&mut self, function: &TransferFunction) {
		self.transfer_curves = Some([Curve::Parametric(*function), Curve::Parametric(*function), Curve::Parametric(*function)]);
	}
}

//==============================================================================
/// ICC 를 앞에서부터 읽는 것. (스키아 IccReader, 큰 쪽부터)
//==============================================================================
#[derive(Clone, Copy)]
struct IccReader<'a> {
	bytes: &'a [u8],
	cursor: usize,
}

impl<'a> IccReader<'a> {
	//==============================================================================
	/// 생성.
	//==============================================================================
	fn new(bytes: &'a [u8]) -> IccReader<'a> {
		IccReader {
			bytes,
			cursor: 0,
		}
	}

	//==============================================================================
	/// 자리부터 길이만큼의 새 읽개. (넘치면 None)
	//==============================================================================
	fn create_sub_reader(&self, offset: usize, length: usize) -> Option<IccReader<'a>> {
		let end = offset.checked_add(length)?;
		Some(IccReader::new(self.bytes.get(offset..end)?))
	}

	//==============================================================================
	/// 자리부터 끝까지의 새 읽개.
	//==============================================================================
	fn create_sub_reader_from(&self, offset: usize) -> Option<IccReader<'a>> {
		Some(IccReader::new(self.bytes.get(offset..)?))
	}

	//==============================================================================
	/// 바이트 읽기.
	//==============================================================================
	fn read_bytes(&mut self, length: usize) -> Option<&'a [u8]> {
		let end = self.cursor.checked_add(length)?;
		let slice = self.bytes.get(self.cursor..end)?;
		self.cursor = end;
		Some(slice)
	}

	//==============================================================================
	/// 8 비트 정수 읽기.
	//==============================================================================
	fn read_u8(&mut self) -> Option<u8> {
		Some(self.read_bytes(1)?[0])
	}

	//==============================================================================
	/// 16 비트 정수 읽기.
	//==============================================================================
	fn read_u16(&mut self) -> Option<u16> {
		let slice = self.read_bytes(2)?;
		Some(u16::from_be_bytes([slice[0], slice[1]]))
	}

	//==============================================================================
	/// 32 비트 정수 읽기.
	//==============================================================================
	fn read_u32(&mut self) -> Option<u32> {
		let slice = self.read_bytes(4)?;
		Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
	}

	//==============================================================================
	/// s15.16 고정 소수 읽기.
	//==============================================================================
	fn read_fixed(&mut self) -> Option<f32> {
		Some(self.read_u32()? as i32 as f32 * (1.0 / 65536.0))
	}

	//==============================================================================
	/// 건너뛰기.
	//==============================================================================
	fn skip(&mut self, length: usize) -> Option<()> {
		self.read_bytes(length).map(|_| ())
	}

	//==============================================================================
	/// 4 바이트 경계로 건너뛰기.
	//==============================================================================
	fn skip_to_alignment(&mut self, alignment: usize) -> Option<()> {
		let remainder = self.cursor % alignment;
		if remainder != 0 {
			self.skip(alignment - remainder)?;
		}
		Some(())
	}

	//==============================================================================
	/// 8 비트 표 읽기.
	//==============================================================================
	fn read_table_8(&mut self, entry_count: usize) -> Option<Vec<u8>> {
		Some(self.read_bytes(entry_count)?.to_vec())
	}

	//==============================================================================
	/// 16 비트 표 읽기. (큰 쪽부터)
	//==============================================================================
	fn read_table_16(&mut self, entry_count: usize) -> Option<Vec<u16>> {
		let slice = self.read_bytes(entry_count.checked_mul(2)?)?;
		Some(slice.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect())
	}
}

//==============================================================================
/// ICC 태그 하나. (형 이름을 읽은 뒤의 읽개)
//==============================================================================
struct IccTag<'a> {
	tag_type: u32,
	reader: IccReader<'a>,
}

//==============================================================================
/// 이름으로 태그 찾기. (skcms_GetTagBySignature, 처음 맞는 것)
//==============================================================================
fn get_tag_by_signature<'a>(bytes: &'a [u8], profile_size: usize, tag_count: usize, signature: u32) -> Option<IccTag<'a>> {
	let profile_reader = IccReader::new(bytes.get(..profile_size)?);
	let mut tag_table_reader = profile_reader.create_sub_reader(132, tag_count.checked_mul(12)?)?;
	for _ in 0..tag_count {
		let tag_signature = tag_table_reader.read_u32()?;
		let offset = tag_table_reader.read_u32()? as usize;
		let size = tag_table_reader.read_u32()? as usize;
		if tag_signature == signature {
			let mut tag_reader = profile_reader.create_sub_reader(offset, size)?;
			let tag_type = tag_reader.read_u32()?;
			return Some(IccTag {
				tag_type,
				reader: tag_reader,
			});
		}
	}
	None
}

//==============================================================================
/// XYZ 태그 읽기. (read_tag_xyz)
//==============================================================================
fn read_xyz_tag(tag: &IccTag) -> Option<[f32; 3]> {
	if tag.tag_type != SIGNATURE_XYZ {
		return None;
	}
	let mut reader = tag.reader;
	reader.skip(4)?;
	Some([reader.read_fixed()?, reader.read_fixed()?, reader.read_fixed()?])
}

//==============================================================================
/// para 곡선 읽기. (read_curve_para, sRGB 꼴이 아니면 None)
//==============================================================================
fn read_parametric_curve(reader: &mut IccReader) -> Option<Curve> {
	reader.skip(4)?;
	let function_type = reader.read_u16()?;
	reader.skip(2)?;
	if function_type > 4 {
		return None;
	}
	let mut function = TransferFunction::new(reader.read_fixed()?, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
	match function_type {
		1 => {
			function.a = reader.read_fixed()?;
			function.b = reader.read_fixed()?;
			if function.a == 0.0 {
				return None;
			}
			function.d = -function.b / function.a;
		}
		2 => {
			function.a = reader.read_fixed()?;
			function.b = reader.read_fixed()?;
			function.e = reader.read_fixed()?;
			if function.a == 0.0 {
				return None;
			}
			function.d = -function.b / function.a;
			function.f = function.e;
		}
		3 => {
			function.a = reader.read_fixed()?;
			function.b = reader.read_fixed()?;
			function.c = reader.read_fixed()?;
			function.d = reader.read_fixed()?;
		}
		4 => {
			function.a = reader.read_fixed()?;
			function.b = reader.read_fixed()?;
			function.c = reader.read_fixed()?;
			function.d = reader.read_fixed()?;
			function.e = reader.read_fixed()?;
			function.f = reader.read_fixed()?;
		}
		_ => {
		}
	}
	let sum = function.a + function.b + function.c + function.d + function.e + function.f + function.g;
	if !(sum.is_finite() && function.a >= 0.0 && function.c >= 0.0 && function.d >= 0.0 && function.g >= 0.0 && function.a * function.d + function.b >= 0.0) {
		return None;
	}
	Some(Curve::Parametric(function))
}

//==============================================================================
/// curv 곡선 읽기. (read_curve_curv, 0 칸은 항등, 1 칸은 8.8 감마)
//==============================================================================
fn read_table_curve(reader: &mut IccReader) -> Option<Curve> {
	reader.skip(4)?;
	let value_count = reader.read_u32()? as usize;
	if value_count < 2 {
		let gamma = if value_count == 0 { 1.0 } else { reader.read_u16()? as f32 * (1.0 / 256.0) };
		return Some(Curve::Parametric(TransferFunction::new(gamma, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0)));
	}
	Some(Curve::Table16(reader.read_table_16(value_count)?))
}

//==============================================================================
/// 형 이름부터 곡선 읽기. (read_curve)
//==============================================================================
fn read_curve(reader: &mut IccReader) -> Option<Curve> {
	let tag_type = reader.read_u32()?;
	if tag_type == SIGNATURE_PARA {
		read_parametric_curve(reader)
	}
	else if tag_type == SIGNATURE_CURV {
		read_table_curve(reader)
	}
	else {
		None
	}
}

//==============================================================================
/// 태그로 곡선 읽기.
//==============================================================================
fn read_curve_from_tag(tag: &IccTag) -> Option<Curve> {
	let mut reader = tag.reader;
	if tag.tag_type == SIGNATURE_PARA {
		read_parametric_curve(&mut reader)
	}
	else if tag.tag_type == SIGNATURE_CURV {
		read_table_curve(&mut reader)
	}
	else {
		None
	}
}

//==============================================================================
/// 곡선 여럿 읽기. (read_curves, 곡선마다 4 바이트 경계)
//==============================================================================
fn read_curves(mut reader: IccReader, curve_count: usize) -> Option<Vec<Curve>> {
	let mut curve_list: Vec<Curve> = Vec::with_capacity(curve_count);
	for _ in 0..curve_count {
		curve_list.push(read_curve(&mut reader)?);
		reader.skip_to_alignment(4)?;
	}
	Some(curve_list)
}

//==============================================================================
/// 빈 찾기 표 변환.
//==============================================================================
fn create_empty_lookup_transform() -> LookupTransform {
	LookupTransform {
		input_curves: Vec::new(),
		input_channel_count: 0,
		grid_points: [0; 4],
		grid_8: Vec::new(),
		grid_16: Vec::new(),
		matrix_curves: Vec::new(),
		matrix: [[0.0; 4]; 3],
		matrix_channel_count: 0,
		output_curves: Vec::new(),
		output_channel_count: 0,
	}
}

//==============================================================================
/// mft1, mft2 읽기. (read_mft_common 과 init_tables, is_device_to_connection 이면 A2B 쪽 제한)
/// - mft 의 행렬은 PCSXYZ 입력에서만 쓰여 무시한다. (skcms 와 같다)
//==============================================================================
fn read_multi_function_table(tag: &IccTag, is_device_to_connection: bool, is_16_bit: bool) -> Option<LookupTransform> {
	let mut lookup_transform = create_empty_lookup_transform();
	let mut reader = tag.reader;
	reader.skip(4)?;
	let input_channel_count = reader.read_u8()? as usize;
	let output_channel_count = reader.read_u8()? as usize;
	let grid_point = reader.read_u8()?;
	if is_device_to_connection {
		if output_channel_count != 3 || input_channel_count < 1 || input_channel_count > 4 || grid_point < 2 {
			return None;
		}
	}
	else if input_channel_count != 3 || output_channel_count < 3 || output_channel_count > 4 || grid_point < 2 {
		return None;
	}
	for index in 0..input_channel_count {
		lookup_transform.grid_points[index] = grid_point;
	}
	lookup_transform.input_channel_count = input_channel_count;
	lookup_transform.output_channel_count = output_channel_count;
	let (input_entry_count, output_entry_count, mut table_reader) = if is_16_bit {
		let mut entry_reader = tag.reader.create_sub_reader_from(48)?;
		let input_entry_count = entry_reader.read_u16()? as usize;
		let output_entry_count = entry_reader.read_u16()? as usize;
		if !(2..=4096).contains(&input_entry_count) || !(2..=4096).contains(&output_entry_count) {
			return None;
		}
		(input_entry_count, output_entry_count, tag.reader.create_sub_reader_from(52)?)
	}
	else {
		(256, 256, tag.reader.create_sub_reader_from(48)?)
	};
	for _ in 0..input_channel_count {
		lookup_transform.input_curves.push(if is_16_bit { Curve::Table16(table_reader.read_table_16(input_entry_count)?) } else { Curve::Table8(table_reader.read_table_8(input_entry_count)?) });
	}
	let mut grid_point_count = 1usize;
	for index in 0..input_channel_count {
		grid_point_count = grid_point_count.checked_mul(lookup_transform.grid_points[index] as usize)?;
	}
	let sample_count = grid_point_count.checked_mul(output_channel_count)?;
	if is_16_bit {
		lookup_transform.grid_16 = table_reader.read_table_16(sample_count)?;
	}
	else {
		lookup_transform.grid_8 = table_reader.read_table_8(sample_count)?;
	}
	for _ in 0..output_channel_count {
		lookup_transform.output_curves.push(if is_16_bit { Curve::Table16(table_reader.read_table_16(output_entry_count)?) } else { Curve::Table8(table_reader.read_table_8(output_entry_count)?) });
	}
	Some(lookup_transform)
}

//==============================================================================
/// mAB, mBA 읽기. (read_tag_mab_or_mba, B 곡선은 꼭 있다, A 곡선과 CLUT 는 함께, M 곡선과 행렬은 함께)
//==============================================================================
fn read_multi_process_table(tag: &IccTag, is_connection_xyz: bool, is_device_to_connection: bool) -> Option<LookupTransform> {
	let mut reader = tag.reader;
	reader.skip(4)?;
	let input_channel_count = reader.read_u8()? as usize;
	let output_channel_count = reader.read_u8()? as usize;
	reader.skip(2)?;
	if is_device_to_connection {
		if output_channel_count != 3 || input_channel_count < 1 || input_channel_count > 4 {
			return None;
		}
	}
	else if input_channel_count != 3 || output_channel_count < 3 || output_channel_count > 4 {
		return None;
	}
	let b_curve_offset = reader.read_u32()? as usize;
	let matrix_offset = reader.read_u32()? as usize;
	let m_curve_offset = reader.read_u32()? as usize;
	let lookup_table_offset = reader.read_u32()? as usize;
	let a_curve_offset = reader.read_u32()? as usize;
	if b_curve_offset == 0 {
		return None;
	}
	let b_curve_count = if is_device_to_connection { output_channel_count } else { input_channel_count };
	let b_curve_list = read_curves(tag.reader.create_sub_reader_from(b_curve_offset)?, b_curve_count)?;
	let mut lookup_transform = create_empty_lookup_transform();
	if m_curve_offset != 0 {
		if matrix_offset == 0 {
			return None;
		}
		let mut matrix_reader = tag.reader.create_sub_reader_from(matrix_offset)?;
		let matrix_channel_count = if is_device_to_connection { output_channel_count } else { input_channel_count };
		lookup_transform.matrix_curves = read_curves(tag.reader.create_sub_reader_from(m_curve_offset)?, matrix_channel_count)?;
		let encoding_factor = if is_connection_xyz { if is_device_to_connection { 65535.0f32 / 32768.0 } else { 32768.0f32 / 65535.0 } } else { 1.0 };
		let mut values = [0.0f32; 12];
		for value in values.iter_mut() {
			*value = encoding_factor * matrix_reader.read_fixed()?;
		}
		for row in 0..3 {
			lookup_transform.matrix[row] = [values[row * 3], values[row * 3 + 1], values[row * 3 + 2], values[9 + row]];
		}
		lookup_transform.matrix_channel_count = matrix_channel_count;
	}
	else if matrix_offset != 0 {
		return None;
	}
	let mut final_channel_count = 0;
	let mut a_curve_list: Vec<Curve> = Vec::new();
	if a_curve_offset != 0 {
		if lookup_table_offset == 0 {
			return None;
		}
		let mut lookup_table_reader = tag.reader.create_sub_reader_from(lookup_table_offset)?;
		let a_curve_count = if is_device_to_connection { input_channel_count } else { output_channel_count };
		a_curve_list = read_curves(tag.reader.create_sub_reader_from(a_curve_offset)?, a_curve_count)?;
		let mut grid_point_count = 1usize;
		for index in 0..16 {
			let grid_point = lookup_table_reader.read_u8()?;
			if index < input_channel_count {
				lookup_transform.grid_points[index] = grid_point;
				if grid_point < 2 {
					return None;
				}
				grid_point_count = grid_point_count.checked_mul(grid_point as usize)?;
			}
		}
		let precision = lookup_table_reader.read_u8()?;
		lookup_table_reader.skip(3)?;
		let sample_count = grid_point_count.checked_mul(output_channel_count)?;
		match precision {
			1 => {
				lookup_transform.grid_8 = lookup_table_reader.read_table_8(sample_count)?;
			}
			2 => {
				lookup_transform.grid_16 = lookup_table_reader.read_table_16(sample_count)?;
			}
			_ => {
				return None;
			}
		}
		final_channel_count = if is_device_to_connection { input_channel_count } else { output_channel_count };
	}
	else {
		if lookup_table_offset != 0 {
			return None;
		}
		if input_channel_count != output_channel_count {
			return None;
		}
	}
	if is_device_to_connection {
		lookup_transform.input_curves = a_curve_list;
		lookup_transform.input_channel_count = final_channel_count;
		lookup_transform.output_curves = b_curve_list;
		lookup_transform.output_channel_count = output_channel_count;
	}
	else {
		lookup_transform.input_curves = b_curve_list;
		lookup_transform.input_channel_count = input_channel_count;
		lookup_transform.output_curves = a_curve_list;
		lookup_transform.output_channel_count = final_channel_count;
	}
	Some(lookup_transform)
}

//==============================================================================
/// 찾기 표 태그 읽기. (read_lut, 형으로 고른다)
//==============================================================================
fn read_lookup_tag(tag: &IccTag, is_connection_xyz: bool, is_device_to_connection: bool) -> Option<LookupTransform> {
	match tag.tag_type {
		SIGNATURE_MFT1 => {
			read_multi_function_table(tag, is_device_to_connection, false)
		}
		SIGNATURE_MFT2 => {
			read_multi_function_table(tag, is_device_to_connection, true)
		}
		SIGNATURE_MAB if is_device_to_connection => {
			read_multi_process_table(tag, is_connection_xyz, true)
		}
		SIGNATURE_MBA if !is_device_to_connection => {
			read_multi_process_table(tag, is_connection_xyz, false)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// ICC 프로필 읽기. (스키아 rust/icc parse_icc_profile, 우선순위는 지각(0) 다음 상대 색도(1), 쓸 수 없으면 None)
/// - 머리: 크기, acsp, 판 4 이하, 광원 D50(0.01 안), 태그 자리와 크기, PCS 는 XYZ 나 Lab.
/// - 회색은 kTRC 를 세 곡선으로 쓰고 PCS 가 XYZ 면 광원을 대각 행렬로 둔다. 그 밖은 rTRC, gTRC, bTRC 와 rXYZ, gXYZ, bXYZ.
//==============================================================================
pub(crate) fn parse_icc_profile(bytes: &[u8]) -> Option<IccProfile> {
	if bytes.len() < 132 {
		return None;
	}
	let mut reader = IccReader::new(bytes);
	let size = reader.read_u32()? as usize;
	reader.skip(4)?;
	let version = reader.read_u32()?;
	reader.skip(4)?;
	let data_color_space = reader.read_u32()?;
	let connection_space = reader.read_u32()?;
	reader.skip(12)?;
	let signature = reader.read_u32()?;
	reader.skip(24)?;
	reader.skip(4)?;
	let illuminant_x = reader.read_fixed()?;
	let illuminant_y = reader.read_fixed()?;
	let illuminant_z = reader.read_fixed()?;
	reader.skip(48)?;
	let tag_count = reader.read_u32()? as usize;
	let tag_table_size = tag_count.checked_mul(12)?;
	let minimum_size = 132usize.checked_add(tag_table_size)?;
	if signature != SIGNATURE_ACSP || size > bytes.len() || size < minimum_size || (version >> 24) > 4 {
		return None;
	}
	if (illuminant_x - 0.9642).abs() > 0.0100 || (illuminant_y - 1.0000).abs() > 0.0100 || (illuminant_z - 0.8249).abs() > 0.0100 {
		return None;
	}
	let mut tag_table = reader.create_sub_reader(132, tag_table_size)?;
	for _ in 0..tag_count {
		tag_table.skip(4)?;
		let tag_offset = tag_table.read_u32()? as usize;
		let tag_size = tag_table.read_u32()? as usize;
		let tag_end = tag_offset.checked_add(tag_size)?;
		if tag_size < 4 || tag_end > size {
			return None;
		}
	}
	if connection_space != SIGNATURE_XYZ && connection_space != SIGNATURE_LAB {
		return None;
	}
	let is_connection_xyz = connection_space == SIGNATURE_XYZ;
	let mut profile = IccProfile {
		data_color_space,
		connection_space,
		to_xyzd50: None,
		transfer_curves: None,
		lookup_transform: None,
		coding_independent_code_points: None,
	};
	if data_color_space == SIGNATURE_GRAY {
		if let Some(gray_tag) = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_GRAY_TRC) {
			let curve = read_curve_from_tag(&gray_tag)?;
			profile.transfer_curves = Some([curve.clone(), curve.clone(), curve]);
			if is_connection_xyz {
				profile.to_xyzd50 = Some([[illuminant_x, 0.0, 0.0], [0.0, illuminant_y, 0.0], [0.0, 0.0, illuminant_z]]);
			}
		}
	}
	else {
		let red_trc = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_RED_TRC);
		let green_trc = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_GREEN_TRC);
		let blue_trc = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_BLUE_TRC);
		if let (Some(red_tag), Some(green_tag), Some(blue_tag)) = (red_trc, green_trc, blue_trc) {
			profile.transfer_curves = Some([read_curve_from_tag(&red_tag)?, read_curve_from_tag(&green_tag)?, read_curve_from_tag(&blue_tag)?]);
		}
		let red_xyz = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_RED_XYZ);
		let green_xyz = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_GREEN_XYZ);
		let blue_xyz = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_BLUE_XYZ);
		if let (Some(red_tag), Some(green_tag), Some(blue_tag)) = (red_xyz, green_xyz, blue_xyz) {
			let red = read_xyz_tag(&red_tag)?;
			let green = read_xyz_tag(&green_tag)?;
			let blue = read_xyz_tag(&blue_tag)?;
			profile.to_xyzd50 = Some([[red[0], green[0], blue[0]], [red[1], green[1], blue[1]], [red[2], green[2], blue[2]]]);
		}
	}
	for priority in [0u32, 1] {
		if let Some(tag) = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_A2B0 + priority) {
			profile.lookup_transform = Some(read_lookup_tag(&tag, is_connection_xyz, true)?);
			break;
		}
	}
	for priority in [0u32, 1] {
		if let Some(tag) = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_B2A0 + priority) {
			read_lookup_tag(&tag, is_connection_xyz, false)?;
			break;
		}
	}
	if let Some(tag) = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_CICP) {
		if tag.tag_type != SIGNATURE_CICP {
			return None;
		}
		let mut cicp_reader = tag.reader;
		cicp_reader.skip(4)?;
		profile.coding_independent_code_points = Some([cicp_reader.read_u8()?, cicp_reader.read_u8()?, cicp_reader.read_u8()?, cicp_reader.read_u8()?]);
	}
	if let Some(tag) = get_tag_by_signature(bytes, size, tag_count, SIGNATURE_HAGC) {
		if tag.tag_type != SIGNATURE_HAGC_TYPE {
			return None;
		}
		let mut hagc_reader = tag.reader;
		hagc_reader.skip(4)?;
		let hagc_size = hagc_reader.read_u32()? as usize;
		hagc_reader.read_bytes(hagc_size)?;
	}
	if profile.lookup_transform.is_none() && (profile.transfer_curves.is_none() || profile.to_xyzd50.is_none()) {
		return None;
	}
	Some(profile)
}

//==============================================================================
/// 색 공간. (SkColorSpace, 전달 함수와 toXYZD50)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub(crate) struct ColorSpace {
	transfer_function: TransferFunction,
	to_xyzd50: Matrix3x3,
}

impl ColorSpace {
	//==============================================================================
	/// sRGB. (SkColorSpace::MakeSRGB)
	//==============================================================================
	const fn srgb() -> ColorSpace {
		ColorSpace {
			transfer_function: SRGB_TRANSFER_FUNCTION,
			to_xyzd50: SRGB_GAMUT,
		}
	}

	//==============================================================================
	/// sRGB 와 비트까지 같은지. (스키아 sRGB 싱글톤과 해시가 같은지)
	//==============================================================================
	fn is_srgb(&self) -> bool {
		self.transfer_function.is_bitwise_equal(&SRGB_TRANSFER_FUNCTION) && matrix_bits(&self.to_xyzd50) == matrix_bits(&SRGB_GAMUT)
	}

	//==============================================================================
	/// 전달 함수와 원색으로 생성. (SkColorSpace::MakeRGB, 거의 sRGB, 2.2, 선형이면 그 값으로 맞춘다, 꼴이 틀리면 None)
	//==============================================================================
	fn make_rgb(transfer_function: &TransferFunction, to_xyzd50: &Matrix3x3) -> Option<ColorSpace> {
		if classify_transfer_function(transfer_function) == TransferFunctionType::Invalid {
			return None;
		}
		let function = transfer_function;
		let is_almost_srgb = is_almost_equal_value(SRGB_TRANSFER_FUNCTION.a, function.a) && is_almost_equal_value(SRGB_TRANSFER_FUNCTION.b, function.b) && is_almost_equal_value(SRGB_TRANSFER_FUNCTION.c, function.c) && is_almost_equal_value(SRGB_TRANSFER_FUNCTION.d, function.d) && is_almost_equal_value(SRGB_TRANSFER_FUNCTION.e, function.e) && is_almost_equal_value(SRGB_TRANSFER_FUNCTION.f, function.f) && is_almost_equal_value(SRGB_TRANSFER_FUNCTION.g, function.g);
		let is_almost_gamma_2_2 = is_almost_equal_value(1.0, function.a) && is_almost_equal_value(0.0, function.b) && is_almost_equal_value(0.0, function.e) && is_almost_equal_value(2.2, function.g) && function.d <= 0.0;
		let is_linear_exponent = is_almost_equal_value(1.0, function.a) && is_almost_equal_value(0.0, function.b) && is_almost_equal_value(0.0, function.e) && is_almost_equal_value(1.0, function.g) && function.d <= 0.0;
		let is_linear_function = is_almost_equal_value(1.0, function.c) && is_almost_equal_value(0.0, function.f) && function.d >= 1.0;
		let snapped_function = if is_almost_srgb {
			if is_almost_equal_matrix(to_xyzd50, &SRGB_GAMUT) {
				return Some(ColorSpace::srgb());
			}
			SRGB_TRANSFER_FUNCTION
		}
		else if is_almost_gamma_2_2 {
			GAMMA_2_2_TRANSFER_FUNCTION
		}
		else if is_linear_exponent || is_linear_function {
			if is_almost_equal_matrix(to_xyzd50, &SRGB_GAMUT) {
				return Some(ColorSpace {
					transfer_function: LINEAR_TRANSFER_FUNCTION,
					to_xyzd50: SRGB_GAMUT,
				});
			}
			LINEAR_TRANSFER_FUNCTION
		}
		else {
			*function
		};
		Some(ColorSpace {
			transfer_function: snapped_function,
			to_xyzd50: *to_xyzd50,
		})
	}

	//==============================================================================
	/// CICP 원색과 전달 특성으로 생성. (SkColorSpace::MakeCICP, 표에 없으면 None)
	//==============================================================================
	fn make_cicp(color_primaries: u8, transfer_characteristics: u8) -> Option<ColorSpace> {
		let transfer_function = get_cicp_transfer_function(transfer_characteristics)?;
		let to_xyzd50 = get_cicp_primaries_matrix(color_primaries)?;
		ColorSpace::make_rgb(&transfer_function, &to_xyzd50)
	}

	//==============================================================================
	/// ICC 프로필로 생성. (SkColorSpace::Make, cicp 태그를 먼저, 거의 sRGB 면 sRGB, 표 곡선은 sRGB 에 가까울 때만, 못 하면 None)
	//==============================================================================
	fn make_from_profile(profile: &IccProfile) -> Option<ColorSpace> {
		let cicp = profile.coding_independent_code_points.unwrap_or([0; 4]);
		let use_cicp = profile.coding_independent_code_points.is_some() && cicp[2] == 0 && cicp[3] == 1;
		if use_cicp && cicp[0] == 1 && cicp[1] == 11 {
			return Some(ColorSpace::srgb());
		}
		else if is_approximately_equal_profile(profile, &ColorSpace::srgb().to_profile()) {
			return Some(ColorSpace::srgb());
		}
		let mut to_xyzd50: Option<Matrix3x3> = None;
		if use_cicp {
			match get_cicp_primaries_matrix(cicp[0]) {
				Some(matrix) => {
					to_xyzd50 = Some(matrix);
				}
				None => {
					if cicp[0] != 2 {
						return None;
					}
				}
			}
		}
		if to_xyzd50.is_none() {
			if let Some(matrix) = profile.to_xyzd50 {
				if invert_matrix(&matrix).is_some() {
					to_xyzd50 = Some(matrix);
				}
			}
		}
		let to_xyzd50 = to_xyzd50?;
		let mut transfer_function: Option<TransferFunction> = None;
		if use_cicp {
			match get_cicp_transfer_function(cicp[1]) {
				Some(function) => {
					transfer_function = Some(function);
				}
				None => {
					if cicp[1] != 2 {
						return None;
					}
				}
			}
		}
		if transfer_function.is_none() {
			if let Some(curve_list) = &profile.transfer_curves {
				if let (Curve::Parametric(red), Curve::Parametric(green), Curve::Parametric(blue)) = (&curve_list[0], &curve_list[1], &curve_list[2]) {
					if red.is_bitwise_equal(green) && red.is_bitwise_equal(blue) {
						transfer_function = Some(*red);
					}
				}
				if transfer_function.is_none() && curve_list.iter().all(|curve| curve.get_maximum_roundtrip_error(&SRGB_INVERSE_TRANSFER_FUNCTION) < 1.0 / 512.0) {
					transfer_function = Some(SRGB_TRANSFER_FUNCTION);
				}
			}
		}
		ColorSpace::make_rgb(&transfer_function?, &to_xyzd50)
	}

	//==============================================================================
	/// 이 색 공간의 ICC 프로필. (SkColorSpace::toProfile, PQ 와 HLG 는 cicp 를 단다)
	//==============================================================================
	fn to_profile(&self) -> IccProfile {
		let mut profile = IccProfile::new();
		profile.set_transfer_function(&self.transfer_function);
		profile.to_xyzd50 = Some(self.to_xyzd50);
		let transfer_type = classify_transfer_function(&self.transfer_function);
		let transfer_characteristics = match transfer_type {
			TransferFunctionType::Pq | TransferFunctionType::PqLike => {
				Some(16)
			}
			TransferFunctionType::Hlg | TransferFunctionType::HlgLike => {
				Some(18)
			}
			_ => {
				None
			}
		};
		if let Some(transfer_characteristics) = transfer_characteristics {
			let mut color_primaries = 2u8;
			for (cicp_id, primaries, _) in CICP_PRIMARIES_TABLE.iter() {
				if let Some(matrix) = primaries_to_xyzd50(primaries) {
					if is_almost_equal_matrix(&self.to_xyzd50, &matrix) {
						color_primaries = *cicp_id;
						break;
					}
				}
			}
			profile.coding_independent_code_points = Some([color_primaries, transfer_characteristics, 0, 1]);
		}
		profile
	}
}

//==============================================================================
/// CICP 원색 번호의 toXYZD50. (SkNamedPrimaries::GetCicp, 표에 없으면 None)
//==============================================================================
fn get_cicp_primaries_matrix(color_primaries: u8) -> Option<Matrix3x3> {
	for (cicp_id, primaries, named_matrix) in CICP_PRIMARIES_TABLE.iter() {
		if *cicp_id != color_primaries {
			continue;
		}
		if let Some(matrix) = named_matrix {
			return Some(*matrix);
		}
		if let Some(matrix) = primaries_to_xyzd50(primaries) {
			return Some(matrix);
		}
	}
	None
}

//==============================================================================
/// CICP 전달 특성 번호의 전달 함수. (SkNamedTransferFn::GetCicp, 표에 없으면 None)
//==============================================================================
fn get_cicp_transfer_function(transfer_characteristics: u8) -> Option<TransferFunction> {
	CICP_TRANSFER_TABLE.iter().find(|(cicp_id, _)| *cicp_id == transfer_characteristics).map(|(_, function)| *function)
}

//==============================================================================
/// CICP 로 색 공간. (skia::CICPGetSkColorSpace, 행렬 계수 0 과 전체 범위만, sRGB 전달 함수를 먼저면 1, 6, 11 을 sRGB 로)
//==============================================================================
fn get_cicp_color_space(cicp: &[u8; 4], is_srgb_transfer_preferred: bool) -> Option<ColorSpace> {
	if cicp[2] != 0 || cicp[3] != 1 {
		return None;
	}
	let mut transfer_characteristics = cicp[1];
	if is_srgb_transfer_preferred && (transfer_characteristics == 1 || transfer_characteristics == 6 || transfer_characteristics == 11) {
		transfer_characteristics = 13;
	}
	ColorSpace::make_cicp(cicp[0], transfer_characteristics)
}

//==============================================================================
/// AVIF 의 nclx 로 색 공간. (blink avif_image_decoder.cc GetColorSpace 와 ui/gfx ColorSpace::ToSkColorSpace)
/// - 미지정(2)은 BT.709 원색과 sRGB 전달 특성으로 읽는다. 크롬이 모르는 번호면 sRGB 다.
/// - BT.709, SMPTE 170M, BT.2020 의 전달 특성은 sRGB 로, PQ 와 HLG 는 기준 흰색 203 니트로 둔다.
///
/// # 돌려줌
/// 색 공간. 크롬이 SkColorSpace 로 바꾸지 못하는 전달 특성(9, 10, 11, 12)이면 None. (크롬은 그림 풀기에 실패한다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn get_avif_color_space(color_primaries: u16, transfer_characteristics: u16, matrix_coefficients: u16, is_monochrome: bool) -> Option<ColorSpace> {
	let primaries = if color_primaries == 2 { 1 } else { color_primaries };
	let transfer = if transfer_characteristics == 2 { 13 } else { transfer_characteristics };
	let matrix = if is_monochrome || matrix_coefficients == 2 { 6 } else { matrix_coefficients };
	let is_primaries_valid = (1..=22).contains(&primaries) && primaries != 3 && !(13..22).contains(&primaries);
	let is_transfer_valid = (1..=18).contains(&transfer) && transfer != 3;
	let is_matrix_valid = matrix <= 11 && matrix != 3 && matrix != 10 && matrix != 2;
	if !is_primaries_valid || !is_transfer_valid || !is_matrix_valid {
		return Some(ColorSpace::srgb());
	}
	if primaries == 1 {
		if transfer == 13 {
			return Some(ColorSpace::srgb());
		}
		if transfer == 8 {
			return Some(ColorSpace {
				transfer_function: LINEAR_TRANSFER_FUNCTION,
				to_xyzd50: SRGB_GAMUT,
			});
		}
	}
	let transfer_function = match transfer {
		13 => {
			TransferFunction::new(2.4, 0.947867345704, 0.052132654296, 0.077399380805, 0.040449937172, 0.0, 0.0)
		}
		8 => {
			LINEAR_TRANSFER_FUNCTION
		}
		18 => {
			HLG_TRANSFER_FUNCTION
		}
		16 => {
			PQ_TRANSFER_FUNCTION
		}
		4 => {
			REC470_SYSTEM_M_TRANSFER_FUNCTION
		}
		5 => {
			REC470_SYSTEM_BG_TRANSFER_FUNCTION
		}
		7 => {
			SMPTE_ST_240_TRANSFER_FUNCTION
		}
		1 | 6 | 14 | 15 => {
			SRGB_TRANSFER_FUNCTION
		}
		17 => {
			SMPTE_ST_428_1_TRANSFER_FUNCTION
		}
		_ => {
			return None;
		}
	};
	let gamut = match primaries {
		1 => {
			SRGB_GAMUT
		}
		12 => {
			DISPLAY_P3_GAMUT
		}
		9 => {
			REC2020_GAMUT
		}
		_ => {
			let xy_primaries = match primaries {
				4 => {
					REC470_SYSTEM_M_PRIMARIES
				}
				5 => {
					REC470_SYSTEM_BG_PRIMARIES
				}
				6 | 7 => {
					REC601_PRIMARIES
				}
				8 => {
					GENERIC_FILM_PRIMARIES
				}
				10 => {
					SMPTE_ST_428_1_PRIMARIES
				}
				11 => {
					SMPTE_RP_431_2_PRIMARIES
				}
				_ => {
					ITU_T_H273_VALUE22_PRIMARIES
				}
			};
			primaries_to_xyzd50(&xy_primaries).unwrap_or(IDENTITY_MATRIX)
		}
	};
	ColorSpace::make_rgb(&transfer_function, &gamut)
}

//==============================================================================
/// skcms 단계 셈의 근사 log2. (Transform_inl.h approx_log2)
//==============================================================================
fn transform_log2(x: f32) -> f32 {
	skcms_log2(x)
}

//==============================================================================
/// skcms 단계 셈의 근사 2^x. (Transform_inl.h approx_exp2, 범위를 0 과 무한 비트로 자르고 버린다)
//==============================================================================
fn transform_exp2(x: f32) -> f32 {
	let fraction = x - x.floor();
	let float_bits = (1u32 << 23) as f32 * (x + 121.274057500 - 1.490129070 * fraction + 27.728023300 / (4.84252568 - fraction));
	let clamped = float_bits.max(0.0).min(2139095040.0);
	f32::from_bits((clamped as i32) as u32)
}

//==============================================================================
/// skcms 단계 셈의 근사 거듭제곱. (Transform_inl.h approx_pow, 0 과 1 은 그대로)
//==============================================================================
fn transform_pow(x: f32, y: f32) -> f32 {
	if x == 0.0 || x == 1.0 {
		x
	}
	else {
		transform_exp2(transform_log2(x) * y)
	}
}

//==============================================================================
/// skcms 단계의 곡선 하나. (select_curve_op)
//==============================================================================
#[derive(Clone, Copy)]
enum TransformCurve<'a> {
	Gamma(TransferFunction),
	Parametric(TransferFunction),
	PqLike(TransferFunction),
	HlgLike(TransferFunction),
	HlgInverseLike(TransferFunction),
	Table(&'a Curve),
}

//==============================================================================
/// skcms 단계 하나. (skcms_Transform 이 짓는 Op)
//==============================================================================
#[derive(Clone, Copy)]
enum TransformOperation<'a> {
	Curve(usize, TransformCurve<'a>),
	/// 네 값을 1 에서 뺀다. (CMYK 원천, skcms Op::invert)
	Invert,
	Clamp,
	LookupTable(&'a LookupTransform),
	Matrix3x3(Matrix3x3),
	Matrix3x4([[f32; 4]; 3]),
	LabToXyz,
	HlgOotfScale,
	HlgInverseOotfScale,
}

//==============================================================================
/// 매개 변수 곡선 하나의 단계 고르기. (select_curve_op, 감마 1 과 셈할 수 없는 꼴은 None)
//==============================================================================
fn select_parametric_transform_curve(function: &TransferFunction) -> Option<TransformCurve<'static>> {
	let is_gamma = function.g > 0.0 && function.a == 1.0 && function.b == 0.0 && function.c == 0.0 && function.d == 0.0 && function.e == 0.0 && function.f == 0.0;
	if is_gamma {
		return if function.g != 1.0 { Some(TransformCurve::Gamma(*function)) } else { None };
	}
	match classify_transfer_function(function) {
		TransferFunctionType::SrgbLike => {
			Some(TransformCurve::Parametric(*function))
		}
		TransferFunctionType::PqLike => {
			Some(TransformCurve::PqLike(*function))
		}
		TransferFunctionType::HlgLike => {
			Some(TransformCurve::HlgLike(*function))
		}
		TransferFunctionType::HlgInverseLike => {
			Some(TransformCurve::HlgInverseLike(*function))
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 곡선 하나의 단계 고르기. (매개 변수 곡선이나 표)
//==============================================================================
fn select_transform_curve(curve: &Curve) -> Option<TransformCurve<'_>> {
	match curve {
		Curve::Parametric(function) => {
			select_parametric_transform_curve(function)
		}
		_ => {
			Some(TransformCurve::Table(curve))
		}
	}
}

//==============================================================================
/// 곡선 여럿을 단계로 넣기. (add_curve_ops, 표 칸이 1 << 24 를 넘으면 실패)
//==============================================================================
fn push_curve_operations<'a>(operation_list: &mut Vec<TransformOperation<'a>>, curve_list: &'a [Curve], channel_count: usize) -> bool {
	for channel in 0..channel_count {
		let Some(curve) = curve_list.get(channel) else {
			return false;
		};
		if curve.get_table_entry_count() > (1 << 24) {
			return false;
		}
		if let Some(transform_curve) = select_transform_curve(curve) {
			operation_list.push(TransformOperation::Curve(channel, transform_curve));
		}
	}
	true
}

//==============================================================================
/// PQ 꼴 셈. (Transform_inl.h apply_pq)
//==============================================================================
fn apply_transform_pq(function: &TransferFunction, value: f32) -> f32 {
	let sign = value.to_bits() & 0x80000000;
	let x = f32::from_bits(value.to_bits() ^ sign);
	let curve = transform_pow((function.a + function.b * transform_pow(x, function.c)).max(0.0) / (function.d + function.e * transform_pow(x, function.c)), function.f);
	f32::from_bits(sign | curve.to_bits())
}

//==============================================================================
/// HLG 꼴 셈. (Transform_inl.h apply_hlg)
//==============================================================================
fn apply_transform_hlg(function: &TransferFunction, value: f32) -> f32 {
	let sign = value.to_bits() & 0x80000000;
	let x = f32::from_bits(value.to_bits() ^ sign);
	let curve = if x * function.a <= 1.0 { transform_pow(x * function.a, function.b) } else { transform_exp2(1.4426950408889634074 * ((x - function.e) * function.c)) + function.d };
	(function.f + 1.0) * f32::from_bits(sign | curve.to_bits())
}

//==============================================================================
/// HLG 역꼴 셈. (Transform_inl.h apply_hlginv)
//==============================================================================
fn apply_transform_hlg_inverse(function: &TransferFunction, value: f32) -> f32 {
	let sign = value.to_bits() & 0x80000000;
	let x = f32::from_bits(value.to_bits() ^ sign) / (function.f + 1.0);
	let curve = if x <= 1.0 { function.a * transform_pow(x, function.b) } else { function.c * (0.69314718 * transform_log2(x - function.d)) + function.e };
	f32::from_bits(sign | curve.to_bits())
}

//==============================================================================
/// skcms 단계 곡선 하나 셈.
//==============================================================================
fn apply_transform_curve(curve: &TransformCurve, value: f32) -> f32 {
	match curve {
		TransformCurve::Gamma(function) => {
			let sign = value.to_bits() & 0x80000000;
			let x = f32::from_bits(value.to_bits() ^ sign);
			f32::from_bits(sign | transform_pow(x, function.g).to_bits())
		}
		TransformCurve::Parametric(function) => {
			let sign = value.to_bits() & 0x80000000;
			let x = f32::from_bits(value.to_bits() ^ sign);
			let curve_value = if x < function.d { function.c * x + function.f } else { transform_pow(function.a * x + function.b, function.g) + function.e };
			f32::from_bits(sign | curve_value.to_bits())
		}
		TransformCurve::PqLike(function) => {
			apply_transform_pq(function, value)
		}
		TransformCurve::HlgLike(function) => {
			apply_transform_hlg(function, value)
		}
		TransformCurve::HlgInverseLike(function) => {
			apply_transform_hlg_inverse(function, value)
		}
		TransformCurve::Table(table_curve) => {
			let entry_count = table_curve.get_table_entry_count();
			let index = value.min(1.0).max(0.0) * (entry_count - 1) as f32;
			let low = index as i32 as usize;
			let high = f32::from_bits((index + 1.0).to_bits() - 1) as i32 as usize;
			let fraction = index - low as f32;
			let low_value = table_curve.read_table_entry(low);
			let high_value = table_curve.read_table_entry(high);
			low_value + (high_value - low_value) * fraction
		}
	}
}

//==============================================================================
/// CLUT 셈. (Transform_inl.h clut, 이웃 격자점 2^차원 개를 가중 합한다)
//==============================================================================
fn apply_lookup_table(lookup_transform: &LookupTransform, rgba: &mut [f32; 4]) {
	let dimension = lookup_transform.input_channel_count;
	if dimension == 0 || dimension > 4 {
		return;
	}
	let mut index = [0usize; 8];
	let mut weight = [0.0f32; 8];
	let inputs = *rgba;
	let mut stride = 1usize;
	for channel in (0..dimension).rev() {
		let grid_point = lookup_transform.grid_points[channel] as usize;
		let x = inputs[channel].min(1.0).max(0.0) * (grid_point - 1) as f32;
		let low = x as i32 as usize;
		let high = f32::from_bits((x + 1.0).to_bits() - 1) as i32 as usize;
		index[channel] = low * stride;
		index[channel + 4] = high * stride;
		stride *= grid_point;
		let fraction = x - low as f32;
		weight[channel] = 1.0 - fraction;
		weight[channel + 4] = fraction;
	}
	let output_channel_count = lookup_transform.output_channel_count;
	let mut sum = [0.0f32; 4];
	for combination in 0..(1usize << dimension) {
		let mut grid_index = index[(combination & 1) * 4];
		let mut grid_weight = weight[(combination & 1) * 4];
		if dimension >= 4 {
			grid_index += index[3 + (combination & 8) / 2];
			grid_weight *= weight[3 + (combination & 8) / 2];
		}
		if dimension >= 3 {
			grid_index += index[2 + (combination & 4)];
			grid_weight *= weight[2 + (combination & 4)];
		}
		if dimension >= 2 {
			grid_index += index[1 + (combination & 2) * 2];
			grid_weight *= weight[1 + (combination & 2) * 2];
		}
		for channel in 0..output_channel_count {
			let sample_index = output_channel_count * grid_index + channel;
			let sample = if lookup_transform.grid_8.is_empty() { lookup_transform.grid_16[sample_index] as f32 * (1.0 / 65535.0) } else { lookup_transform.grid_8[sample_index] as f32 * (1.0 / 255.0) };
			sum[channel] += grid_weight * sample;
		}
	}
	rgba[0] = sum[0];
	rgba[1] = sum[1];
	rgba[2] = sum[2];
	if output_channel_count == 4 {
		rgba[3] = sum[3];
	}
	if lookup_transform.input_channel_count == 4 {
		rgba[3] = 1.0;
	}
}

//==============================================================================
/// skcms 단계 하나 셈.
//==============================================================================
fn apply_transform_operation(operation: &TransformOperation, rgba: &mut [f32; 4]) {
	match operation {
		TransformOperation::Curve(channel, curve) => {
			rgba[*channel] = apply_transform_curve(curve, rgba[*channel]);
		}
		TransformOperation::Invert => {
			for value in rgba.iter_mut() {
				*value = 1.0 - *value;
			}
		}
		TransformOperation::Clamp => {
			for value in rgba.iter_mut() {
				*value = value.min(1.0).max(0.0);
			}
		}
		TransformOperation::LookupTable(lookup_transform) => {
			apply_lookup_table(lookup_transform, rgba);
		}
		TransformOperation::Matrix3x3(matrix) => {
			let red = matrix[0][0] * rgba[0] + matrix[0][1] * rgba[1] + matrix[0][2] * rgba[2];
			let green = matrix[1][0] * rgba[0] + matrix[1][1] * rgba[1] + matrix[1][2] * rgba[2];
			let blue = matrix[2][0] * rgba[0] + matrix[2][1] * rgba[1] + matrix[2][2] * rgba[2];
			rgba[0] = red;
			rgba[1] = green;
			rgba[2] = blue;
		}
		TransformOperation::Matrix3x4(matrix) => {
			let red = matrix[0][0] * rgba[0] + matrix[0][1] * rgba[1] + matrix[0][2] * rgba[2] + matrix[0][3];
			let green = matrix[1][0] * rgba[0] + matrix[1][1] * rgba[1] + matrix[1][2] * rgba[2] + matrix[1][3];
			let blue = matrix[2][0] * rgba[0] + matrix[2][1] * rgba[1] + matrix[2][2] * rgba[2] + matrix[2][3];
			rgba[0] = red;
			rgba[1] = green;
			rgba[2] = blue;
		}
		TransformOperation::LabToXyz => {
			let lightness = rgba[0] * 100.0;
			let a_value = rgba[1] * 255.0 - 128.0;
			let b_value = rgba[2] * 255.0 - 128.0;
			let mut y = (lightness + 16.0) * (1.0 / 116.0);
			let mut x = y + a_value * (1.0 / 500.0);
			let mut z = y - b_value * (1.0 / 200.0);
			x = if x * x * x > 0.008856 { x * x * x } else { (x - (16.0 / 116.0)) * (1.0 / 7.787) };
			y = if y * y * y > 0.008856 { y * y * y } else { (y - (16.0 / 116.0)) * (1.0 / 7.787) };
			z = if z * z * z > 0.008856 { z * z * z } else { (z - (16.0 / 116.0)) * (1.0 / 7.787) };
			rgba[0] = x * 0.9642;
			rgba[1] = y;
			rgba[2] = z * 0.8249;
		}
		TransformOperation::HlgOotfScale => {
			let luminance = -0.02831655 * rgba[0] + 1.00995452 * rgba[1] + 0.02102382 * rgba[2];
			let sign = luminance.to_bits() & 0x80000000;
			let magnitude = f32::from_bits(luminance.to_bits() ^ sign);
			let scale = f32::from_bits(sign | transform_pow(magnitude, 0.2).to_bits());
			for channel in 0..3 {
				rgba[channel] = rgba[channel] * scale;
				rgba[channel] *= 1000.0 / 203.0;
			}
		}
		TransformOperation::HlgInverseOotfScale => {
			for channel in 0..3 {
				rgba[channel] *= 203.0 / 1000.0;
			}
			let luminance = -0.02831655 * rgba[0] + 1.00995452 * rgba[1] + 0.02102382 * rgba[2];
			let sign = luminance.to_bits() & 0x80000000;
			let magnitude = f32::from_bits(luminance.to_bits() ^ sign);
			let scale = f32::from_bits(sign | transform_pow(magnitude, 1.0 / 1.2 - 1.0).to_bits());
			for channel in 0..3 {
				rgba[channel] = rgba[channel] * scale;
			}
		}
	}
}

//==============================================================================
/// 기준 PQ 꼴 함수. (skcms set_reference_pq_ish_trc, 1.0 이 10000 / 203 으로 간다)
//==============================================================================
fn get_reference_pq_like_function() -> TransferFunction {
	let mut function = TransferFunction::new(-(TransferFunctionType::PqLike as i32 as f32), -107.0 / 128.0, 1.0, 32.0 / 2523.0, 2413.0 / 128.0, -2392.0 / 128.0, 8192.0 / 1305.0);
	let white_scale = skcms_pow(10000.0 / 203.0, 1.0 / function.f);
	function.a *= white_scale;
	function.b *= white_scale;
	function
}

//==============================================================================
/// SDR HLG 꼴 함수. (skcms set_sdr_hlg_ish_trc)
//==============================================================================
fn get_sdr_hlg_like_function() -> TransferFunction {
	TransferFunction::new(-(TransferFunctionType::HlgLike as i32 as f32), 2.0, 2.0, 1.0 / 0.17883277, 0.28466892, 0.55991073, 1.0 / 12.0 - 1.0)
}

//==============================================================================
/// skcms_Transform 의 단계 짓기. (원본 프로필에서 대상 프로필로, 대상은 행렬과 매개 변수 곡선이어야 한다, 못 지으면 None)
/// - is_float_destination 이면 끝에 자르지 않는다. (RGBA_hhhh 같은 반정밀 출력)
//==============================================================================
fn build_transform_program<'a>(source: &'a IccProfile, destination: &IccProfile, is_float_destination: bool) -> Option<Vec<TransformOperation<'a>>> {
	let mut operation_list: Vec<TransformOperation<'a>> = Vec::new();
	let destination_from_xyz = destination.to_xyzd50.and_then(|matrix| invert_matrix(&matrix));
	let destination_cicp_transfer = destination.coding_independent_code_points.map(|cicp| cicp[1]);
	let mut destination_inverse_list: [TransferFunction; 3] = [LINEAR_TRANSFER_FUNCTION; 3];
	let mut is_destination_hlg_ootf = false;
	if destination_cicp_transfer == Some(16) && destination_from_xyz.is_some() {
		let inverse = invert_transfer_function(&get_reference_pq_like_function())?;
		destination_inverse_list = [inverse; 3];
	}
	else if destination_cicp_transfer == Some(18) && destination_from_xyz.is_some() {
		let inverse = invert_transfer_function(&get_sdr_hlg_like_function())?;
		destination_inverse_list = [inverse; 3];
		is_destination_hlg_ootf = true;
	}
	else {
		destination_from_xyz?;
		let curve_list = destination.transfer_curves.as_ref()?;
		for channel in 0..3 {
			let Curve::Parametric(function) = &curve_list[channel] else {
				return None;
			};
			destination_inverse_list[channel] = invert_transfer_function(function)?;
		}
	}
	let destination_from_xyz = destination_from_xyz?;
	if source.data_color_space == SIGNATURE_CMYK {
		// 포토샵이 만드는 뒤집힌 CMYK 로 본다. (skcms_Transform 의 Op::invert, 알파는 곱하지 않는다)
		operation_list.push(TransformOperation::Invert);
	}
	let source_cicp_transfer = source.coding_independent_code_points.map(|cicp| cicp[1]);
	let mut is_source_lookup = false;
	let mut is_source_hlg_ootf = false;
	if source_cicp_transfer == Some(16) && source.to_xyzd50.is_some() {
		let function = get_reference_pq_like_function();
		for channel in 0..3 {
			operation_list.push(TransformOperation::Curve(channel, TransformCurve::PqLike(function)));
		}
	}
	else if source_cicp_transfer == Some(18) && source.to_xyzd50.is_some() {
		is_source_hlg_ootf = true;
		let function = get_sdr_hlg_like_function();
		for channel in 0..3 {
			operation_list.push(TransformOperation::Curve(channel, TransformCurve::HlgLike(function)));
		}
	}
	else if let Some(lookup_transform) = &source.lookup_transform {
		is_source_lookup = true;
		if lookup_transform.input_channel_count > 0 {
			if !push_curve_operations(&mut operation_list, &lookup_transform.input_curves, lookup_transform.input_channel_count) {
				return None;
			}
			operation_list.push(TransformOperation::Clamp);
			operation_list.push(TransformOperation::LookupTable(lookup_transform));
		}
		if lookup_transform.matrix_channel_count == 3 {
			if !push_curve_operations(&mut operation_list, &lookup_transform.matrix_curves, 3) {
				return None;
			}
			let identity = [[1.0f32, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
			let is_identity = (0..3).all(|row| (0..4).all(|column| lookup_transform.matrix[row][column].to_bits() == identity[row][column].to_bits()));
			if !is_identity {
				operation_list.push(TransformOperation::Matrix3x4(lookup_transform.matrix));
			}
		}
		if lookup_transform.output_channel_count == 3 && !push_curve_operations(&mut operation_list, &lookup_transform.output_curves, 3) {
			return None;
		}
		if source.connection_space == SIGNATURE_LAB {
			operation_list.push(TransformOperation::LabToXyz);
		}
	}
	else if let (Some(curve_list), Some(_)) = (&source.transfer_curves, &source.to_xyzd50) {
		if !push_curve_operations(&mut operation_list, curve_list, 3) {
			return None;
		}
	}
	else {
		return None;
	}
	if is_source_hlg_ootf != is_destination_hlg_ootf {
		if !is_source_lookup {
			operation_list.push(TransformOperation::Matrix3x3(source.to_xyzd50?));
		}
		if is_source_hlg_ootf {
			operation_list.push(TransformOperation::HlgOotfScale);
		}
		if is_destination_hlg_ootf {
			operation_list.push(TransformOperation::HlgInverseOotfScale);
		}
		operation_list.push(TransformOperation::Matrix3x3(destination_from_xyz));
	}
	else if is_source_lookup {
		operation_list.push(TransformOperation::Matrix3x3(destination_from_xyz));
	}
	else {
		let source_to_xyz = source.to_xyzd50?;
		let destination_to_xyz = destination.to_xyzd50?;
		if matrix_bits(&destination_to_xyz) != matrix_bits(&source_to_xyz) {
			operation_list.push(TransformOperation::Matrix3x3(concatenate_matrix(&destination_from_xyz, &source_to_xyz)));
		}
	}
	for channel in 0..3 {
		if let Some(transform_curve) = select_parametric_transform_curve(&destination_inverse_list[channel]) {
			operation_list.push(TransformOperation::Curve(channel, transform_curve));
		}
	}
	if !is_float_destination {
		operation_list.push(TransformOperation::Clamp);
	}
	Some(operation_list)
}

//==============================================================================
/// skcms 의 8 비트 저장 값. (to_fixed, v x 255 + 0.5 를 버린다)
//==============================================================================
fn store_transform_byte(value: f32) -> u8 {
	((value * 255.0 + 0.5) as i32) as u8
}

//==============================================================================
/// 두 프로필이 거의 같은지. (skcms_ApproximatelyEqualProfiles, 바이트 252 개를 XYZD50 으로 바꿔 모두 한 단계 안이면 같다)
//==============================================================================
fn is_approximately_equal_profile(profile: &IccProfile, other_profile: &IccProfile) -> bool {
	if (profile.data_color_space == SIGNATURE_CMYK) != (other_profile.data_color_space == SIGNATURE_CMYK) {
		return false;
	}
	let mut xyz_profile = IccProfile::new();
	xyz_profile.set_transfer_function(&LINEAR_TRANSFER_FUNCTION);
	xyz_profile.to_xyzd50 = Some(IDENTITY_MATRIX);
	let transform_bytes = |source: &IccProfile| -> Option<[u8; 252]> {
		let program = build_transform_program(source, &xyz_profile, false)?;
		let mut output = [0u8; 252];
		for pixel_index in 0..84 {
			let mut rgba = [RANDOM_BYTES[pixel_index * 3] as f32 * (1.0 / 255.0), RANDOM_BYTES[pixel_index * 3 + 1] as f32 * (1.0 / 255.0), RANDOM_BYTES[pixel_index * 3 + 2] as f32 * (1.0 / 255.0), 1.0];
			for operation in program.iter() {
				apply_transform_operation(operation, &mut rgba);
			}
			for channel in 0..3 {
				output[pixel_index * 3 + channel] = store_transform_byte(rgba[channel]);
			}
		}
		Some(output)
	};
	let (Some(profile_bytes), Some(other_bytes)) = (transform_bytes(profile), transform_bytes(other_profile)) else {
		return false;
	};
	profile_bytes.iter().zip(other_bytes.iter()).all(|(left, right)| (*left as i32 - *right as i32).abs() <= 1)
}

//==============================================================================
/// 래스터 파이프라인의 근사 log2. (SkRasterPipeline_opts.h approx_log2, nmad 는 융합)
//==============================================================================
fn pipeline_log2(x: f32) -> f32 {
	let exponent = x.to_bits() as f32 * (1.0 / (1u32 << 23) as f32);
	let mantissa = f32::from_bits((x.to_bits() & 0x007fffff) | 0x3f000000);
	(-mantissa).mul_add(1.498030302, exponent - 124.225514990) - 1.725879990 / (0.3520887068 + mantissa)
}

//==============================================================================
/// 래스터 파이프라인의 근사 2^x. (approx_pow2, 짝수 쪽 반올림)
//==============================================================================
fn pipeline_pow2(x: f32) -> f32 {
	let fraction = x - x.floor();
	let mut approximation = (-fraction).mul_add(1.490129070, x + 121.274057500);
	approximation += 27.728023300 / (4.84252568 - fraction);
	approximation *= (1u32 << 23) as f32;
	approximation = approximation.max(0.0).min(2139095040.0);
	f32::from_bits(approximation.round_ties_even() as u32)
}

//==============================================================================
/// 래스터 파이프라인의 근사 거듭제곱. (approx_powf, 0 과 1 은 그대로)
//==============================================================================
fn pipeline_pow(x: f32, y: f32) -> f32 {
	if x == 0.0 || x == 1.0 {
		x
	}
	else {
		pipeline_pow2(pipeline_log2(x) * y)
	}
}

//==============================================================================
/// 래스터 파이프라인 전달 함수 단계 하나. (appendTransferFunction 이 고르는 parametric, gamma_, PQish, HLGish, HLGinvish)
//==============================================================================
fn apply_pipeline_transfer_function(function: &TransferFunction, value: f32) -> f32 {
	let sign = value.to_bits() & 0x80000000;
	let x = f32::from_bits(value.to_bits() ^ sign);
	match classify_transfer_function(function) {
		TransferFunctionType::SrgbLike => {
			let is_gamma = function.a == 1.0 && function.b == 0.0 && function.c == 0.0 && function.d == 0.0 && function.e == 0.0 && function.f == 0.0;
			let curve = if is_gamma {
				pipeline_pow(x, function.g)
			}
			else if x <= function.d {
				function.c.mul_add(x, function.f)
			}
			else {
				pipeline_pow(function.a.mul_add(x, function.b), function.g) + function.e
			};
			f32::from_bits(sign | curve.to_bits())
		}
		TransferFunctionType::PqLike => {
			let power = pipeline_pow(x, function.c);
			let curve = pipeline_pow(function.b.mul_add(power, function.a).max(0.0) / function.e.mul_add(power, function.d), function.f);
			f32::from_bits(sign | curve.to_bits())
		}
		TransferFunctionType::HlgLike => {
			let curve = if x * function.a <= 1.0 { pipeline_pow(x * function.a, function.b) } else { pipeline_pow2(1.4426950408889634074 * ((x - function.e) * function.c)) + function.d };
			(function.f + 1.0) * f32::from_bits(sign | curve.to_bits())
		}
		TransferFunctionType::HlgInverseLike => {
			let scaled = x / (function.f + 1.0);
			let curve = if scaled <= 1.0 { function.a * pipeline_pow(scaled, function.b) } else { function.c * (0.69314718 * pipeline_log2(scaled - function.d)) + function.e };
			f32::from_bits(sign | curve.to_bits())
		}
		_ => {
			value
		}
	}
}

//==============================================================================
/// 그릴 때 색 공간을 바꾸는 단계. (SkColorSpaceXformSteps, 원본은 알파를 곱하지 않은 것, 대상은 sRGB 프리멀티플라이드)
//==============================================================================
struct ColorSpaceTransformSteps {
	is_linearizing: bool,
	is_source_ootf: bool,
	is_gamut_transform: bool,
	is_encoding: bool,
	is_premultiplying: bool,
	source_function: TransferFunction,
	source_ootf: [f32; 4],
	/// 열 먼저 늘어선 원색 바꾸기 행렬. (밝기 배율을 곱했다)
	source_to_destination_matrix: [f32; 9],
	destination_inverse_function: TransferFunction,
}

impl ColorSpaceTransformSteps {
	//==============================================================================
	/// 색 공간에서 sRGB 로 가는 단계 짓기. (has_alpha 면 끝에 알파를 곱한다)
	//==============================================================================
	fn new(source: &ColorSpace, has_alpha: bool) -> ColorSpaceTransformSteps {
		let destination = ColorSpace::srgb();
		let mut steps = ColorSpaceTransformSteps {
			is_linearizing: false,
			is_source_ootf: false,
			is_gamut_transform: false,
			is_encoding: false,
			is_premultiplying: has_alpha,
			source_function: LINEAR_TRANSFER_FUNCTION,
			source_ootf: [0.0; 4],
			source_to_destination_matrix: [0.0; 9],
			destination_inverse_function: LINEAR_TRANSFER_FUNCTION,
		};
		let source_transfer = source.transfer_function;
		let mut scale_factor = 1.0f32;
		match classify_transfer_function(&source_transfer) {
			TransferFunctionType::Pq => {
				scale_factor *= 10000.0 / source_transfer.a;
				steps.source_function = PQ_LIKE_STEP_FUNCTION;
				steps.is_linearizing = true;
			}
			TransferFunctionType::Hlg => {
				scale_factor *= source_transfer.b / source_transfer.a;
				steps.is_linearizing = true;
				steps.source_function = HLG_LIKE_STEP_FUNCTION;
				steps.source_function.f = 1.0 / 12.0 - 1.0;
				if source_transfer.c != 1.0 {
					steps.is_source_ootf = true;
					steps.source_ootf[3] = source_transfer.c - 1.0;
					let rec2020_linear_from_xyz = invert_matrix(&REC2020_GAMUT).unwrap_or(IDENTITY_MATRIX);
					let to_rec2020 = concatenate_matrix(&rec2020_linear_from_xyz, &source.to_xyzd50);
					let luminance_rec2020 = [0.262700f32, 0.678000, 0.059300];
					for index in 0..3 {
						steps.source_ootf[index] = 0.0;
						for row in 0..3 {
							steps.source_ootf[index] += to_rec2020[row][index] * luminance_rec2020[row];
						}
					}
				}
			}
			_ => {
				steps.is_linearizing = !source_transfer.is_bitwise_equal(&LINEAR_TRANSFER_FUNCTION);
				if steps.is_linearizing {
					steps.source_function = source_transfer;
				}
			}
		}
		steps.is_encoding = true;
		steps.destination_inverse_function = invert_transfer_function(&destination.transfer_function).unwrap_or(SRGB_INVERSE_TRANSFER_FUNCTION);
		steps.is_gamut_transform = matrix_bits(&source.to_xyzd50) != matrix_bits(&destination.to_xyzd50) || scale_factor != 1.0;
		if steps.is_gamut_transform {
			let destination_from_xyz = invert_matrix(&destination.to_xyzd50).unwrap_or(IDENTITY_MATRIX);
			let source_to_destination = concatenate_matrix(&destination_from_xyz, &source.to_xyzd50);
			for column in 0..3 {
				for row in 0..3 {
					steps.source_to_destination_matrix[column * 3 + row] = source_to_destination[row][column] * scale_factor;
				}
			}
		}
		if steps.is_linearizing && !steps.is_source_ootf && !steps.is_gamut_transform && steps.is_encoding && source_transfer.is_bitwise_equal(&destination.transfer_function) {
			steps.is_linearizing = false;
			steps.is_encoding = false;
		}
		steps
	}

	//==============================================================================
	/// 8 비트 값 256 개를 편 표. (원본이 8 비트면 펴기의 입력이 256 가지라 표로 셈해도 같다)
	//==============================================================================
	fn create_byte_linearize_table(&self) -> Option<Vec<f32>> {
		if !self.is_linearizing {
			return None;
		}
		Some((0..256).map(|value| apply_pipeline_transfer_function(&self.source_function, value as f32 * (1.0 / 255.0))).collect())
	}

	//==============================================================================
	/// 픽셀 하나 바꾸기. (unpremul 은 원본이 곧은 알파라 없다, 그 뒤 차례는 펴기, OOTF, 원색, 되감기, 알파 곱하기)
	/// - byte_linearize_table 이 있으면 펴기를 표로 한다. (값이 바이트 / 255 일 때)
	//==============================================================================
	fn apply(&self, rgba: &mut [f32; 4], byte_linearize_table: Option<&[f32]>) {
		if self.is_linearizing {
			for channel in 0..3 {
				rgba[channel] = match byte_linearize_table {
					Some(table) => {
						table[(rgba[channel] * 255.0).round() as usize]
					}
					None => {
						apply_pipeline_transfer_function(&self.source_function, rgba[channel])
					}
				};
			}
		}
		if self.is_source_ootf {
			let luminance = self.source_ootf[0] * rgba[0] + self.source_ootf[1] * rgba[1] + self.source_ootf[2] * rgba[2];
			let sign = luminance.to_bits() & 0x80000000;
			let magnitude = f32::from_bits(luminance.to_bits() ^ sign);
			let scale = f32::from_bits(sign | pipeline_pow(magnitude, self.source_ootf[3]).to_bits());
			for channel in 0..3 {
				rgba[channel] = rgba[channel] * scale;
			}
		}
		if self.is_gamut_transform {
			let matrix = &self.source_to_destination_matrix;
			let red = rgba[0].mul_add(matrix[0], rgba[1].mul_add(matrix[3], rgba[2] * matrix[6]));
			let green = rgba[0].mul_add(matrix[1], rgba[1].mul_add(matrix[4], rgba[2] * matrix[7]));
			let blue = rgba[0].mul_add(matrix[2], rgba[1].mul_add(matrix[5], rgba[2] * matrix[8]));
			rgba[0] = red;
			rgba[1] = green;
			rgba[2] = blue;
		}
		if self.is_encoding {
			for channel in 0..3 {
				rgba[channel] = apply_pipeline_transfer_function(&self.destination_inverse_function, rgba[channel]);
			}
		}
		if self.is_premultiplying {
			for channel in 0..3 {
				rgba[channel] = rgba[channel] * rgba[3];
			}
		}
	}
}

//==============================================================================
/// 래스터 파이프라인의 8 비트 저장 값. (store_8888 의 to_unorm, 0 과 255 사이로 자르고 짝수 쪽 반올림)
//==============================================================================
fn store_pipeline_byte(value: f32) -> u8 {
	value.mul_add(255.0, 0.0).max(0.0).min(255.0).round_ties_even() as u8
}

//==============================================================================
/// 32 비트 실수를 반정밀로 반올림한 값. (F16 으로 풀린 그림, 짝수 쪽 반올림, 반정밀 아래로 작은 값은 반정밀 비정규 값)
//==============================================================================
pub(crate) fn round_to_half(value: f32) -> f32 {
	let bits = value.to_bits();
	let sign = bits & 0x80000000;
	let magnitude = f32::from_bits(bits ^ sign);
	if !magnitude.is_finite() {
		return value;
	}
	let rounded = if magnitude < 6.103515625e-05 {
		let quantum = 5.960464477539063e-08f32;
		(magnitude / quantum).round_ties_even() * quantum
	}
	else {
		let exponent = ((magnitude.to_bits() >> 23) & 0xff) as i32 - 127;
		let quantum = f32::from_bits(((exponent - 10 + 127) as u32) << 23);
		(magnitude / quantum).round_ties_even() * quantum
	};
	let clamped = if rounded > 65504.0 { f32::INFINITY } else { rounded };
	f32::from_bits(sign | clamped.to_bits())
}

//==============================================================================
/// 그림에 박힌 색 프로필. (skia::ColorProfile 과 blink ImageDecoder 의 kTag 다루기)
//==============================================================================
pub(crate) struct EmbeddedColorProfile {
	profile: IccProfile,
	/// 그림에 다는 색 공간. (GetSkColorSpace)
	color_space: ColorSpace,
	/// 그 색 공간이 프로필과 꼭 같은지. (아니면 해독 때 skcms_Transform 으로 그 색 공간으로 바꾼다)
	is_color_space_exact: bool,
	/// 해독 때 바꾸기를 SkCodec 이 하는지. (PNG, 두 프로필이 거의 같으면 바꾸지 않는다)
	is_codec_transform: bool,
}

impl EmbeddedColorProfile {
	//==============================================================================
	/// 읽은 프로필로 생성. (skia::ColorProfile::Make 의 ComputeSkColorSpace)
	/// - cicp 태그가 색 공간이 되면 그것, SkColorSpace::Make 가 되면 그것(A2B 가 있으면 꼭 같지 않다), 원색이 있으면 sRGB 전달 함수를 단 그 원색, 그 밖은 sRGB.
	//==============================================================================
	fn from_profile(profile: IccProfile) -> EmbeddedColorProfile {
		if let Some(cicp) = profile.coding_independent_code_points {
			if let Some(color_space) = get_cicp_color_space(&cicp, true) {
				return EmbeddedColorProfile {
					profile,
					color_space,
					is_color_space_exact: true,
					is_codec_transform: false,
				};
			}
		}
		if let Some(color_space) = ColorSpace::make_from_profile(&profile) {
			let is_color_space_exact = profile.lookup_transform.is_none();
			return EmbeddedColorProfile {
				profile,
				color_space,
				is_color_space_exact,
				is_codec_transform: false,
			};
		}
		if profile.to_xyzd50.is_some() {
			let mut srgb_transfer_profile = profile.clone();
			srgb_transfer_profile.set_transfer_function(&SRGB_TRANSFER_FUNCTION);
			if let Some(color_space) = ColorSpace::make_from_profile(&srgb_transfer_profile) {
				return EmbeddedColorProfile {
					profile,
					color_space,
					is_color_space_exact: false,
					is_codec_transform: false,
				};
			}
		}
		EmbeddedColorProfile {
			profile,
			color_space: ColorSpace::srgb(),
			is_color_space_exact: false,
			is_codec_transform: false,
		}
	}

	//==============================================================================
	/// ICC 바이트로 생성. (skia::ColorProfile::Make(span), 읽지 못하면 None)
	//==============================================================================
	pub(crate) fn from_icc_bytes(bytes: &[u8]) -> Option<EmbeddedColorProfile> {
		Some(EmbeddedColorProfile::from_profile(parse_icc_profile(bytes)?))
	}

	//==============================================================================
	/// 색 공간으로 생성. (skia::ColorProfile::Make(SkColorSpace), 늘 꼭 같다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn from_color_space(color_space: ColorSpace) -> EmbeddedColorProfile {
		EmbeddedColorProfile {
			profile: color_space.to_profile(),
			color_space,
			is_color_space_exact: true,
			is_codec_transform: false,
		}
	}

	//==============================================================================
	/// RGB 프로필인지.
	//==============================================================================
	pub(crate) fn is_rgb(&self) -> bool {
		self.profile.data_color_space == SIGNATURE_RGB
	}

	//==============================================================================
	/// 회색 프로필인지.
	//==============================================================================
	pub(crate) fn is_gray(&self) -> bool {
		self.profile.data_color_space == SIGNATURE_GRAY
	}

	//==============================================================================
	/// CMYK 프로필인지. (블링크는 성분 4 개 JPEG 에 CMYK 프로필만 쓴다)
	//==============================================================================
	pub(crate) fn is_cmyk(&self) -> bool {
		self.profile.data_color_space == SIGNATURE_CMYK
	}

	//==============================================================================
	/// 픽셀을 바꿔야 하는지. (sRGB 로 꼭 같으면 바꿀 것이 없다)
	///
	/// # 돌려줌
	/// 그림에 다는 색 공간이 sRGB 가 아니거나 해독 때 바꾸기가 있으면 true.
	//==============================================================================
	pub(crate) fn needs_conversion(&self) -> bool {
		!(self.is_color_space_exact && self.color_space.is_srgb())
	}

	//==============================================================================
	/// 곧은 알파 RGBA(0 과 1 사이 실수, 또는 반정밀)를 sRGB 프리멀티플라이드 8 비트로. (load_pixel 이 픽셀 번호의 값을 준다)
	/// - 해독 때 바꾸기(꼭 같지 않을 때 skcms_Transform), 그다음 그릴 때 바꾸기(SkColorSpaceXformSteps).
	/// - 색 공간이 sRGB 로 바뀌면 8 비트 원본은 SkConvertPixels 처럼 정수로 알파를 곱한다. (swizzle_or_premul)
	//==============================================================================
	fn convert_pixels(&self, pixel_count: usize, load_pixel: impl Fn(usize) -> [f32; 4], is_half_float: bool) -> Vec<u8> {
		let destination_profile = self.color_space.to_profile();
		let is_transform_skipped = self.is_color_space_exact || (self.is_codec_transform && is_approximately_equal_profile(&self.profile, &destination_profile));
		let program = if is_transform_skipped { None } else { build_transform_program(&self.profile, &destination_profile, is_half_float) };
		let is_integer_premultiply = self.color_space.is_srgb() && !is_half_float;
		let has_alpha = (0..pixel_count).any(|pixel_index| load_pixel(pixel_index)[3] < 1.0);
		let steps = ColorSpaceTransformSteps::new(&self.color_space, has_alpha);
		let byte_linearize_table = if is_half_float { None } else { steps.create_byte_linearize_table() };
		let mut output_pixels: Vec<u8> = Vec::with_capacity(pixel_count * 4);
		for pixel_index in 0..pixel_count {
			let mut rgba = load_pixel(pixel_index);
			if let Some(program) = program.as_ref() {
				for operation in program.iter() {
					apply_transform_operation(operation, &mut rgba);
				}
				for value in rgba.iter_mut() {
					*value = if is_half_float { round_to_half(*value) } else { store_transform_byte(*value) as f32 * (1.0 / 255.0) };
				}
			}
			if is_integer_premultiply {
				let alpha = (rgba[3] * 255.0).round() as u32;
				for channel in 0..3 {
					let value = (rgba[channel] * 255.0).round() as u32;
					output_pixels.push(((value * alpha + 127) / 255) as u8);
				}
				output_pixels.push(alpha as u8);
				continue;
			}
			steps.apply(&mut rgba, byte_linearize_table.as_deref());
			for value in rgba.iter() {
				output_pixels.push(store_pipeline_byte(*value));
			}
		}
		output_pixels
	}

	//==============================================================================
	/// 곧은 알파 RGBA 8 비트를 sRGB 프리멀티플라이드로. (needs_conversion 이 false 면 부르는 쪽이 그대로 알파를 곱한다)
	///
	/// # 인자
	/// - `straight_pixels`: 알파를 곱하지 않은 RGBA 바이트. (그림의 색 공간)
	///
	/// # 돌려줌
	/// sRGB 로 바꾸고 알파를 곱한 RGBA 바이트.
	//==============================================================================
	pub(crate) fn convert_to_srgb_premultiplied(&self, straight_pixels: &[u8]) -> Vec<u8> {
		let load_pixel = |pixel_index: usize| -> [f32; 4] {
			let pixel = &straight_pixels[pixel_index * 4..pixel_index * 4 + 4];
			[pixel[0] as f32 * (1.0 / 255.0), pixel[1] as f32 * (1.0 / 255.0), pixel[2] as f32 * (1.0 / 255.0), pixel[3] as f32 * (1.0 / 255.0)]
		};
		self.convert_pixels(straight_pixels.len() / 4, load_pixel, false)
	}

	//==============================================================================
	/// 곧은 알파 RGBA 반정밀 값을 sRGB 프리멀티플라이드로. (높은 비트 깊이 그림, 크롬은 F16 으로 푼다)
	///
	/// # 인자
	/// - `half_float_pixels`: 알파를 곱하지 않은 RGBA 값. (반정밀로 반올림한 실수)
	///
	/// # 돌려줌
	/// sRGB 로 바꾸고 알파를 곱한 RGBA 바이트.
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn convert_half_float_to_srgb_premultiplied(&self, half_float_pixels: &[f32]) -> Vec<u8> {
		let load_pixel = |pixel_index: usize| -> [f32; 4] {
			let pixel = &half_float_pixels[pixel_index * 4..pixel_index * 4 + 4];
			[pixel[0], pixel[1], pixel[2], pixel[3]]
		};
		self.convert_pixels(half_float_pixels.len() / 4, load_pixel, true)
	}
}

//==============================================================================
/// PNG 의 색 프로필 읽기. (스키아 SkPngRustCodec CreateColorProfile 의 차례, png 크레이트로 청크를 읽는다, 없거나 무시하면 None)
/// - cICP(행렬 계수 0, 전체 범위)가 가장 먼저, 그다음 iCCP, sRGB 청크가 있으면 sRGB, gAMA 가 없거나 0 이하면 sRGB.
/// - cHRM 없이 gAMA 만 있고 1/2.2 의 5% 안이면 무시한다. 아니면 gAMA 의 1/감마를 전달 함수로, cHRM 이나 sRGB 원색을 쓴다.
/// - 회색 프로필은 회색 PNG 에만, CMYK 프로필은 쓰지 않는다. (SkPngCodecBase::isCompatibleColorProfileAndType)
//==============================================================================
pub(crate) fn read_png_color_profile(bytes: &[u8]) -> Option<EmbeddedColorProfile> {
	let mut options = png::DecodeOptions::default();
	options.set_ignore_text_chunk(true);
	let decoder = png::Decoder::new_with_options(std::io::Cursor::new(bytes), options);
	let reader = decoder.read_info().ok()?;
	let info = reader.info();
	let is_gray_image = matches!(info.color_type, png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha);
	let profile = read_png_chunk_profile(info)?;
	if profile.data_color_space == SIGNATURE_CMYK || (profile.data_color_space == SIGNATURE_GRAY && !is_gray_image) {
		return None;
	}
	let mut embedded_profile = EmbeddedColorProfile::from_profile(profile);
	embedded_profile.is_codec_transform = true;
	Some(embedded_profile)
}

//==============================================================================
/// PNG 청크로 프로필 짓기. (CreateColorProfile)
//==============================================================================
fn read_png_chunk_profile(info: &png::Info) -> Option<IccProfile> {
	if let Some(cicp) = &info.coding_independent_code_points {
		if cicp.matrix_coefficients == 0 && cicp.is_video_full_range_image {
			if let Some(color_space) = ColorSpace::make_cicp(cicp.color_primaries, cicp.transfer_function) {
				return Some(color_space.to_profile());
			}
		}
	}
	if let Some(icc_bytes) = &info.icc_profile {
		if let Some(profile) = parse_icc_profile(icc_bytes) {
			return Some(profile);
		}
	}
	if info.srgb.is_some() {
		return None;
	}
	let gamma = 0.00001f32 * (info.gama_chunk?.into_scaled() as f32);
	if gamma <= 0.0 {
		return None;
	}
	let to_xyzd50 = match &info.chrm_chunk {
		Some(chromaticities) => {
			let read = |value: png::ScaledFloat| -> f32 {
				0.00001f32 * (value.into_scaled() as f32)
			};
			let primaries = [read(chromaticities.red.0), read(chromaticities.red.1), read(chromaticities.green.0), read(chromaticities.green.1), read(chromaticities.blue.0), read(chromaticities.blue.1), read(chromaticities.white.0), read(chromaticities.white.1)];
			primaries_to_xyzd50(&primaries)?
		}
		None => {
			let neutral_value = gamma * 2.2;
			if 0.95 < neutral_value && neutral_value < 1.05 {
				return None;
			}
			SRGB_GAMUT
		}
	};
	let mut profile = IccProfile::new();
	profile.set_transfer_function(&TransferFunction::new(1.0 / gamma, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0));
	profile.to_xyzd50 = Some(to_xyzd50);
	Some(profile)
}

//==============================================================================
/// JPEG 의 색 프로필 읽기. (SkJpegMetadataDecoderImpl 의 APP2 "ICC_PROFILE\0" 조각을 번호 차례로 잇는다, blink 는 회색 JPEG 에 회색이나 RGB, 그 밖은 RGB 프로필만 쓴다)
/// - 조각 수가 0, 번호가 범위 밖, 수가 서로 다름, 같은 번호 둘, 모자람은 프로필 없음이다. 첫 SOS 앞의 표시만 본다.
//==============================================================================
pub(crate) fn read_jpeg_color_profile(bytes: &[u8]) -> Option<EmbeddedColorProfile> {
	if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
		return None;
	}
	let signature = b"ICC_PROFILE\0";
	let mut position = 2usize;
	let mut part_list: Vec<Option<&[u8]>> = Vec::new();
	let mut expected_part_count = 0usize;
	let mut found_part_count = 0usize;
	let mut component_count = 0usize;
	while position + 4 <= bytes.len() {
		if bytes[position] != 0xff {
			return None;
		}
		let marker = bytes[position + 1];
		if marker == 0xff {
			position += 1;
			continue;
		}
		if marker == 0xd8 || (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
			position += 2;
			continue;
		}
		if marker == 0xda || marker == 0xd9 {
			break;
		}
		let segment_length = ((bytes[position + 2] as usize) << 8) | bytes[position + 3] as usize;
		if segment_length < 2 || position + 2 + segment_length > bytes.len() {
			break;
		}
		let segment = &bytes[position + 4..position + 2 + segment_length];
		if (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc && segment.len() >= 6 {
			component_count = segment[5] as usize;
		}
		if marker == 0xe2 && &segment[..signature.len().min(segment.len())] == signature && segment.len() > signature.len() + 2 && found_part_count < expected_part_count.max(1) {
			let part_index = segment[signature.len()] as usize;
			let part_count = segment[signature.len() + 1] as usize;
			if part_count == 0 || part_index == 0 || part_index > part_count {
				return None;
			}
			if expected_part_count == 0 {
				expected_part_count = part_count;
				part_list = vec![None; part_count];
			}
			if part_count != expected_part_count {
				return None;
			}
			if part_list[part_index - 1].is_some() {
				return None;
			}
			part_list[part_index - 1] = Some(&segment[signature.len() + 2..]);
			found_part_count += 1;
		}
		position += 2 + segment_length;
	}
	if expected_part_count == 0 || found_part_count != expected_part_count {
		return None;
	}
	let icc_bytes: Vec<u8> = part_list.iter().flat_map(|part| part.unwrap_or(&[]).iter().copied()).collect();
	let profile = EmbeddedColorProfile::from_icc_bytes(&icc_bytes)?;
	let is_accepted = match component_count {
		1 => {
			profile.is_gray() || profile.is_rgb()
		}
		4 => {
			profile.is_cmyk()
		}
		_ => {
			profile.is_rgb()
		}
	};
	if is_accepted {
		Some(profile)
	}
	else {
		None
	}
}

//==============================================================================
/// WebP 의 색 프로필 읽기. (blink WEBPImageDecoder::ReadColorProfile, VP8X 에 ICC 표시가 있고 움직이지 않을 때 첫 ICCP 청크, RGB 프로필만)
//==============================================================================
pub(crate) fn read_webp_color_profile(bytes: &[u8]) -> Option<EmbeddedColorProfile> {
	if bytes.len() < 30 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" || &bytes[12..16] != b"VP8X" {
		return None;
	}
	let flags = bytes[20];
	if (flags & 0x20) == 0 || (flags & 0x02) != 0 {
		return None;
	}
	let riff_end = (8 + u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize).min(bytes.len());
	let mut position = 12usize;
	while position + 8 <= riff_end {
		let chunk_size = u32::from_le_bytes([bytes[position + 4], bytes[position + 5], bytes[position + 6], bytes[position + 7]]) as usize;
		let chunk_end = position.checked_add(8)?.checked_add(chunk_size)?;
		if chunk_end > riff_end {
			return None;
		}
		if &bytes[position..position + 4] == b"ICCP" {
			let profile = EmbeddedColorProfile::from_icc_bytes(&bytes[position + 8..chunk_end])?;
			return if profile.is_rgb() { Some(profile) } else { None };
		}
		position = chunk_end + (chunk_size & 1);
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::image::Image;

	/// 4 x 2 PNG, Display P3 원색과 sRGB 곡선(para 3)의 ICC. (빨강, 초록, 파랑, 회색 128, (200, 100, 50), (10, 20, 30), 흰색, 검정)
	const DISPLAY_P3_PNG_HEX: &str = concat!(
		"89504e470d0a1a0a0000000d4948445200000004000000020802000000f0caea34000000a5694343507033000078da636060ccc8cbcf4b65316060c8cd2b290a72775288888c5260400289c9c5050c78",
		"c1b7ab0c8c20fab20e03e980ad086821903e03c422e910f603103b09c2fe0262178504393330307200d90ae910b606889d04617b80d8087737df6360b0ddf7ffffffdd0831af7d0c0c1bcd1818b87622",
		"c434ac1918047918184e9c2d482c4a040b310331535a1a03c3a7e50c0cbc910c0cc217807aa22995070080ed36466bed4ace0000001d4944415478da63f8cfc0c000c60d0d0d0c27528cb844e4feff07",
		"09000075b809158508713d0000000049454e44ae426082",
	);
	/// 같은 프로필의 RGBA PNG. (알파 255, 128, 64, 200, 10, 0, 128, 77)
	const DISPLAY_P3_ALPHA_PNG_HEX: &str = concat!(
		"89504e470d0a1a0a0000000d49484452000000040000000208060000007fa87d63000000a5694343507033000078da636060ccc8cbcf4b65316060c8cd2b290a72775288888c5260400289c9c5050c78",
		"c1b7ab0c8c20fab20e03e980ad086821903e03c422e910f603103b09c2fe0262178504393330307200d90ae910b606889d04617b80d8087737df6360b0ddf7ffffffdd0831af7d0c0c1bcd1818b87622",
		"c434ac1918047918184e9c2d482c4a040b310331535a1a03c3a7e50c0cbc910c0cc217807aa22995070080ed36466bed4ace000000284944415478da63f8cfc0f01f081b8094434343c309861329465c",
		"5c22720cffffff6f88da22e70b00e3d20d9f4d7c2d080000000049454e44ae426082",
	);
	/// gAMA 0.45455 와 sRGB 값의 cHRM PNG. (감마 2.2 전달 함수와 cHRM 원색)
	const GAMMA_CHROMATICITY_PNG_HEX: &str = concat!(
		"89504e470d0a1a0a0000000d4948445200000004000000020802000000f0caea340000000467414d410000b18f0bfc6105000000206348524d00007a26000080840000fa00000080e8000075300000ea",
		"6000003a98000017709cba513c0000001d4944415478da63f8cfc0c000c60d0d0d0c27528cb844e4feff0709000075b809158508713d0000000049454e44ae426082",
	);
	/// P3 행렬, para 감마 2.2 와 A2B0(mft2, 5 격자)가 함께 있는 ICC 의 PNG. (A2B 를 먼저 쓰고 해독 때 바꾼다)
	const LOOKUP_TABLE_PNG_HEX: &str = concat!(
		"89504e470d0a1a0a0000000d4948445200000004000000020802000000f0caea34000003c4694343506c7574000078da6360609997979f97ca24c0c0909b575214e4eea4101119a5c0800412938b0b18",
		"f0826f57191841f4651d06d2014711d04220fd048845d221ec1f20761298cdc803629797149400d92077891485043903d92640b6403a84ed02622741d82120b6a3919301909dc2c0c06c85f04ff33d06",
		"06db7dffffffdf8d10f3dac7c0b0d18c81816b27424cc39a81411068ef89b308b16fd7a07ed42d482c4a84bb9ec9d898103f37adc408c46466666580988104b0f1998090e1ff7f040683f70ca92ccb99",
		"df30ae149ec83989854fff885000fba1e82c266ee6b30c2d4c3f598c58b558bfb196898872cf65bf6de02112cd3d21fa1d4726ff4a2679ce08013eb6633ca18241a21b047f0b6f337c2cb54b3c3bb653",
		"244ef535abb2a88f5a00e749094ff529125b643f68fe3079aeb64e776d42afda790f47ce93eafb3c96f11568edf67c251b6590e8e36fd967a910c09c6ac2f49e7101832cf33d265696236cb799bd84d7",
		"f134b2aed37f28eac0991cddce1ac926c7b093cd9dad993581d385fdb88817df6b2e1d835af195bc0f6294793c8539982a79cd8573d915044c4436897189de161733b2959b29753af6b1c41bad3ed695",
		"92b7b4be70f9cb5cd7b190b452aad59b639aab6d63149828aca3e417ccd5a82be8b7835fc380cfff8f9c90e996a0442b57bbe230b1d4d73c2a6ca50c79bcc26cb75845050438e484ff8b6ee36a363092",
		"2be5338fbec1f78b7b0fa32aff4b1e45d63342cf78d344b649f4f09f3278a9e82d5c1fd32f562fb997995b3c4f4a897da354b6749ad81c0541d95346e7352e28d6c7952a5f36ea614b573968f4919b4f",
		"7d9f89a9e40fdd54b35966da66ca567e8917cd2f841fe62eb2d81fa1212061bd2732579ed93129faa2b59997627c5bda1dd95afe1a4666b91cfe87acab1433055545a6a8f30b77199cd03f276e1793ab",
		"705c7416e30dc5ada2ffd9ea5436893b8b666a4548ae309c632c2e1b13eba4f144651bf313cdcbaa321c5d3a17d412c44b8dca348e18afb036d5a98cf7337b6377853dd7fc96bd258f88d575875aa97f",
		"0eb54e8fcc0d3d6ddca6255df7de9cdaccb3ca6761ea0bc140ff79e93a0ad6a12e99136cf2e398735d33442d0264d3185f58dac99e61ebb7b1961710ad747ca45862b8c66b81aa666c908da04a2ad36d",
		"5b069553ec4df67fd5f8c4725c176b141bcdf74bd2518f7375396db29895cf75972927e7568fed66be12f3fd622d369a5c0a97b1494ba808ee0c10e13816521e50c99b135e12b857263c562a44c9a22b",
		"f566f8f514834ce90a3b3eb32cae8ac5427773d92b9f2b1e2f5a5be363fbbb3aa78121730172ca070095cb34c6129b422b0000001d4944415478da63f8cfc0c000c60d0d0d0c27528cb844e4feff0709",
		"000075b809158508713d0000000049454e44ae426082",
	);
	/// cICP 9, 16, 0, 1(BT.2020 원색, PQ) PNG.
	const PQ_PNG_HEX: &str = concat!(
		"89504e470d0a1a0a0000000d4948445200000004000000020802000000f0caea340000000463494350091000014d2323fe0000001d4944415478da63f8cfc0c000c60d0d0d0c27528cb844e4feff0709",
		"000075b809158508713d0000000049454e44ae426082",
	);

	//==============================================================================
	/// 16진 글을 바이트로.
	//==============================================================================
	fn decode_hex(hex_text: &str) -> Vec<u8> {
		(0..hex_text.len()).step_by(2).map(|character_index| u8::from_str_radix(&hex_text[character_index..character_index + 2], 16).expect("16진")).collect()
	}

	//==============================================================================
	/// 그림의 프리멀티플라이드 픽셀 목록.
	//==============================================================================
	fn read_pixel_list(bytes: &[u8]) -> Vec<[u8; 4]> {
		let image = Image::load_from_memory(bytes).expect("그림");
		let pixel_list: Vec<[u8; 4]> = image.get_pixels().chunks_exact(4).map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]]).collect();
		pixel_list
	}

	//==============================================================================
	/// sRGB 전달 함수의 역함수는 skcms 가 셈한 값과 같고, 되돌리면 1 은 1 이다.
	//==============================================================================
	#[test]
	fn invert_srgb_transfer_function() {
		let inverse = invert_transfer_function(&SRGB_TRANSFER_FUNCTION).expect("inverse");
		assert!((inverse.g - 1.0 / 2.4).abs() < 1e-6);
		assert!((inverse.c - 12.92).abs() < 1e-4);
		assert_eq!(evaluate_transfer_function(&inverse, evaluate_transfer_function(&SRGB_TRANSFER_FUNCTION, 1.0)), 1.0);
		assert!(invert_transfer_function(&PQ_TRANSFER_FUNCTION).is_none());
	}

	//==============================================================================
	/// CICP 색 공간: 1, 13 은 sRGB, 12, 13 은 Display P3 원색에 sRGB 전달 함수, 9, 16 은 PQ 다.
	//==============================================================================
	#[test]
	fn make_cicp_color_space() {
		assert!(ColorSpace::make_cicp(1, 13).expect("srgb").is_srgb());
		let display_p3 = ColorSpace::make_cicp(12, 13).expect("p3");
		assert!(!display_p3.is_srgb());
		assert_eq!(matrix_bits(&display_p3.to_xyzd50), matrix_bits(&DISPLAY_P3_GAMUT));
		let pq = ColorSpace::make_cicp(9, 16).expect("pq");
		assert_eq!(classify_transfer_function(&pq.transfer_function), TransferFunctionType::Pq);
		assert!(ColorSpace::make_cicp(3, 13).is_none());
		// AVIF 는 BT.709 전달 특성을 sRGB 로 읽고, 모르는 원색은 sRGB, 전달 특성 11 은 풀지 못한다.
		assert!(get_avif_color_space(1, 1, 6, false).expect("avif").is_srgb());
		assert!(get_avif_color_space(3, 13, 6, false).expect("avif").is_srgb());
		assert!(get_avif_color_space(1, 11, 6, false).is_none());
	}

	//==============================================================================
	/// 반정밀 반올림: 1 과 그 둘레, 짝수 쪽, 비정규 값.
	//==============================================================================
	#[test]
	fn round_half_float() {
		assert_eq!(round_to_half(1.0), 1.0);
		assert_eq!(round_to_half(1.0 + 1.0 / 4096.0), 1.0);
		assert_eq!(round_to_half(1.0 + 3.0 / 4096.0), 1.0 + 1.0 / 1024.0);
		assert_eq!(round_to_half(0.0), 0.0);
		assert_eq!(round_to_half(-0.5), -0.5);
		assert_eq!(round_to_half(5.960464477539063e-08 * 0.4), 0.0);
	}

	//==============================================================================
	/// PNG 의 iCCP, gAMA 와 cHRM, A2B 찾기 표, cICP PQ 를 sRGB 로 바꾼 값이 크롬 158 WebGL texImage2D(UNPACK_PREMULTIPLY_ALPHA_WEBGL) 로 잰 값과 같다.
	//==============================================================================
	#[test]
	fn convert_png_color_profile_like_chrome() {
		let expected_list: [(&str, [[u8; 4]; 8]); 5] = [
			(DISPLAY_P3_PNG_HEX, [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [128, 128, 128, 255], [215, 93, 31, 255], [7, 20, 31, 255], [255, 255, 255, 255], [0, 0, 0, 255]]),
			(DISPLAY_P3_ALPHA_PNG_HEX, [[255, 0, 0, 255], [0, 130, 0, 128], [0, 0, 67, 64], [100, 100, 100, 200], [8, 4, 1, 10], [0, 0, 0, 0], [128, 128, 128, 128], [12, 55, 0, 77]]),
			(GAMMA_CHROMATICITY_PNG_HEX, [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [129, 129, 129, 255], [201, 100, 46, 255], [3, 12, 24, 255], [255, 255, 255, 255], [0, 0, 0, 255]]),
			(LOOKUP_TABLE_PNG_HEX, [[187, 15, 1, 255], [0, 168, 2, 255], [0, 0, 187, 255], [93, 93, 93, 255], [147, 82, 38, 255], [13, 19, 28, 255], [187, 187, 187, 255], [0, 0, 0, 255]]),
			(PQ_PNG_HEX, [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [181, 181, 181, 255], [255, 0, 0, 255], [0, 3, 9, 255], [255, 255, 255, 255], [0, 0, 0, 255]]),
		];
		for (hex_text, expected_pixel_list) in expected_list.iter() {
			assert_eq!(read_pixel_list(&decode_hex(hex_text)), expected_pixel_list.to_vec());
		}
	}

	//==============================================================================
	/// JPEG 의 APP2 ICC 조각은 번호 차례로 잇고, 모자라거나 같은 번호가 둘이면 버린다. WebP 는 VP8X 의 ICC 표시가 있고 움직이지 않을 때만 ICCP 를 쓴다.
	//==============================================================================
	#[test]
	fn read_jpeg_and_webp_icc_profile() {
		let png_bytes = decode_hex(DISPLAY_P3_PNG_HEX);
		let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes.as_slice()));
		let reader = decoder.read_info().expect("png");
		let icc_bytes = reader.info().icc_profile.as_ref().expect("iCCP").to_vec();
		let (first_half, second_half) = icc_bytes.split_at(icc_bytes.len() / 2);
		let create_segment = |marker: u8, payload: &[u8]| -> Vec<u8> {
			let mut segment = vec![0xff, marker, ((payload.len() + 2) >> 8) as u8, (payload.len() + 2) as u8];
			segment.extend_from_slice(payload);
			segment
		};
		let create_icc_segment = |part_index: u8, part: &[u8]| -> Vec<u8> {
			let mut payload = b"ICC_PROFILE\0".to_vec();
			payload.push(part_index);
			payload.push(2);
			payload.extend_from_slice(part);
			create_segment(0xe2, &payload)
		};
		let start_of_frame = create_segment(0xc0, &[8, 0, 2, 0, 4, 3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
		let mut jpeg_bytes = vec![0xff, 0xd8];
		jpeg_bytes.extend(create_icc_segment(2, second_half));
		jpeg_bytes.extend(create_icc_segment(1, first_half));
		jpeg_bytes.extend(start_of_frame.clone());
		jpeg_bytes.extend([0xff, 0xda]);
		let jpeg_profile = read_jpeg_color_profile(&jpeg_bytes).expect("jpeg icc");
		assert!(jpeg_profile.is_rgb() && jpeg_profile.needs_conversion());
		let mut duplicate_bytes = vec![0xff, 0xd8];
		duplicate_bytes.extend(create_icc_segment(1, first_half));
		duplicate_bytes.extend(create_icc_segment(1, first_half));
		duplicate_bytes.extend(start_of_frame);
		duplicate_bytes.extend([0xff, 0xda]);
		assert!(read_jpeg_color_profile(&duplicate_bytes).is_none());
		let create_webp = |flags: u8| -> Vec<u8> {
			let mut body = b"WEBPVP8X".to_vec();
			body.extend([10, 0, 0, 0, flags, 0, 0, 0, 3, 0, 0, 1, 0, 0]);
			body.extend(b"ICCP");
			body.extend((icc_bytes.len() as u32).to_le_bytes());
			body.extend(icc_bytes.iter());
			if icc_bytes.len() % 2 == 1 {
				body.push(0);
			}
			let mut riff = b"RIFF".to_vec();
			riff.extend((body.len() as u32).to_le_bytes());
			riff.extend(body);
			riff
		};
		assert!(read_webp_color_profile(&create_webp(0x20)).expect("webp icc").needs_conversion());
		assert!(read_webp_color_profile(&create_webp(0x22)).is_none());
		assert!(read_webp_color_profile(&create_webp(0x00)).is_none());
	}
}
