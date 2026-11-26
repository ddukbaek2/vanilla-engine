//==============================================================================
//! 바르게 반올림한 f64 사인, 코사인, 탄젠트, 아크 코사인과 크롬과 같은 셈의 아크 탄젠트 둘, 빗변. (원본이 도는 크롬의 Math.sin, cos, tan, acos, atan2, hypot 과 같은 비트)
//! - 크롬 158(V8 의 src/base/ieee754.cc 가 llvm-libc 의 sin, cos, tan, acos, atan2 를 부름)은 sin, cos, tan, acos 를 바르게 반올림한 값(참값에 가장 가까운 f64)으로 낸다.
//!   바르게 반올림한 값은 하나뿐이라 셈법이 달라도 같은 비트가 나온다. 시스템 libm(맥, 윈도우, 리눅스, wasm 의 musl 판)은 끝자리가 플랫폼마다 달라 쓰지 않는다.
//! - 셈: 2/π 비트 표로 Payne-Hanek 줄이기(f 를 200 비트 넘게 정확히) 뒤 |r| <= π/4 에서 두 배 정밀도(double-double) 테일러 급수, 마지막에 한 번 반올림.
//!   두 배 정밀도 오차는 2^-100 안쪽이라 참값이 두 f64 의 가운데에서 그보다 가까운 입력(알려진 어려운 경우 몇 개)만 끝자리가 갈릴 수 있다.
//!   크롬 158 과 22,000 입력(±7 부터 ±1e15, 2^-1000 ~ 2^1000)의 sin, cos, tan 을 견줘 모두 같았다.
//! - 아크 코사인은 직접 짠 두 배 정밀도 asin 급수. 아크 탄젠트 둘은 바르게 반올림이 아니라(llvm-libc 문서에 1 ULP) LLVM libc 의 셈 차례를 옮겼다.
//!   빗변은 V8 이 직접 하는 셈(src/builtins/math.tq)을 그대로 옮겼다. 크롬 158 과 acos 34,022, atan2 70,484, hypot 둘 30,484, 셋 10,264, 넷 3,000 입력을 견줘 모두 같았다.
//! - sin, cos, tan, acos 는 곱셈과 덧셈을 합치지 않는다(Dekker 나누기). atan2 는 크롬 arm64 와 같게 합친다(mul_add, 모든 플랫폼에서 한 번 반올림이라 같다).
//! - sin, cos, tan, acos 의 표와 계수는 mpmath(2000 비트)로 직접 뽑았다. atan2 의 atan(i/64) 표도 mpmath 로 뽑았고 llvm-libc 표와 65 값 모두 같다.
//! - atan2 의 셈 차례는 LLVM libc(src/__support/math/atan2.h, atan_utils.h, FPUtil/double_double.h)에서 옮겼다. Apache-2.0 WITH LLVM-exception, THIRD_PARTY_LICENSES.md 의 LLVM libc 절.
//==============================================================================

// 2/π 의 소수 비트. (첫 원소의 맨 위 비트가 2^-1 자리, 1536 비트)
const TWO_OVER_PI: [u64; 24] = [0xA2F9836E4E441529, 0xFC2757D1F534DDC0, 0xDB6295993C439041, 0xFE5163ABDEBBC561, 0xB7246E3A424DD2E0, 0x06492EEA09D1921C, 0xFE1DEB1CB129A73E, 0xE88235F52EBB4484, 0xE99C7026B45F7E41, 0x3991D639835339F4, 0x9C845F8BBDF9283B, 0x1FF897FFDE05980F, 0xEF2F118B5A0A6D1F, 0x6D367ECF27CB09B7, 0x4F463F669E5FEA2D, 0x7527BAC7EBE5F17B, 0x3D0739F78A5292EA, 0x6BFB5FB11F8D5D08, 0x56033046FC7B6BAB, 0xF0CFBC209AF4361D, 0xA9E391615EE61B08, 0x6599855F14A06840, 0x8DFFD8804D732731, 0x06061556CA73A8C9];

// π/2 의 두 배 정밀도 값. (위, 아래)
const HALF_PI: (f64, f64) = (1.5707963267948966, 6.123233995736766e-17);

// 1/(2n+1)! 의 두 배 정밀도 값. (사인 급수 계수, n = 0..15, (π/4)^31/31! < 2^-120)
const SIN_COEFFICIENTS: [(f64, f64); 16] = [(1.0, 0.0), (0.16666666666666666, 9.25185853854297e-18), (0.008333333333333333, 1.1564823173178714e-19), (0.0001984126984126984, 1.7209558293420705e-22), (2.7557319223985893e-06, -1.858393274046472e-22), (2.505210838544172e-08, -1.448814070935912e-24), (1.6059043836821613e-10, 1.2585294588752098e-26), (7.647163731819816e-13, 7.03872877733453e-30), (2.8114572543455206e-15, 1.6508842730861433e-31), (8.22063524662433e-18, 2.2141894119604265e-34), (1.9572941063391263e-20, -1.3643503830087908e-36), (3.868170170630684e-23, -8.843177655482344e-40), (6.446950284384474e-26, -1.9330404233703465e-42), (9.183689863795546e-29, 1.4303150396787322e-45), (1.1309962886447716e-31, 1.0498015412959506e-47), (1.216125041553518e-34, 5.586290567888806e-51)];

// 1/(2n)! 의 두 배 정밀도 값. (코사인 급수 계수, n = 0..16)
const COS_COEFFICIENTS: [(f64, f64); 17] = [(1.0, 0.0), (0.5, 0.0), (0.041666666666666664, 2.3129646346357427e-18), (0.001388888888888889, -5.300543954373577e-20), (2.48015873015873e-05, 2.1511947866775882e-23), (2.755731922398589e-07, 2.3767714622250297e-23), (2.08767569878681e-09, -1.20734505911326e-25), (1.1470745597729725e-11, 2.0655512752830745e-28), (4.779477332387385e-14, 4.399205485834081e-31), (1.5619206968586225e-16, 1.1910679660273754e-32), (4.110317623312165e-19, 1.4412973378659527e-36), (8.896791392450574e-22, -7.911402614872376e-38), (1.6117375710961184e-24, -3.6846573564509766e-41), (2.4795962632247976e-27, -1.2953730964765229e-43), (3.279889237069838e-30, 1.5117542744029879e-46), (3.7699876288159054e-33, 2.5870347832750324e-49), (3.8003907548547434e-36, 1.7457158024652518e-52)];

// 이보다 작은 |x| 는 sin x = x, tan x = x, cos x = 1 로 반올림된다. (2^-27)
const TINY: f64 = 7.450580596923828e-9;

// π/4. (이보다 작으면 줄이지 않는다)
const QUARTER_PI: f64 = 0.7853981633974483;

//==============================================================================
/// 두 수의 합과 반올림 오차. (Knuth TwoSum)
///
/// # 인자
/// - `left`: 왼쪽 값.
/// - `right`: 오른쪽 값.
///
/// # 돌려줌
/// (합, 오차).
//==============================================================================
fn two_sum(left: f64, right: f64) -> (f64, f64) {
	let sum = left + right;
	let right_part = sum - left;
	let error = (left - (sum - right_part)) + (right - right_part);
	(sum, error)
}

//==============================================================================
/// |위| >= |아래| 인 두 수의 합과 오차. (Dekker FastTwoSum)
///
/// # 인자
/// - `high`: 큰 값.
/// - `low`: 작은 값.
///
/// # 돌려줌
/// (합, 오차).
//==============================================================================
fn fast_two_sum(high: f64, low: f64) -> (f64, f64) {
	let sum = high + low;
	let error = low - (sum - high);
	(sum, error)
}

//==============================================================================
/// 26 비트씩 둘로 나누기. (Dekker split)
///
/// # 인자
/// - `value`: 값.
///
/// # 돌려줌
/// (위, 아래).
//==============================================================================
fn split(value: f64) -> (f64, f64) {
	let scaled = 134217729.0 * value;
	let high = scaled - (scaled - value);
	(high, value - high)
}

//==============================================================================
/// 두 수의 곱과 반올림 오차. (Dekker TwoProduct, fma 없이)
///
/// # 인자
/// - `left`: 왼쪽 값.
/// - `right`: 오른쪽 값.
///
/// # 돌려줌
/// (곱, 오차).
//==============================================================================
fn two_product(left: f64, right: f64) -> (f64, f64) {
	let product = left * right;
	let (left_high, left_low) = split(left);
	let (right_high, right_low) = split(right);
	let error = ((left_high * right_high - product) + left_high * right_low + left_low * right_high) + left_low * right_low;
	(product, error)
}

//==============================================================================
/// 두 배 정밀도 곱.
///
/// # 인자
/// - `left`: 왼쪽 값. (위, 아래)
/// - `right`: 오른쪽 값. (위, 아래)
///
/// # 돌려줌
/// 곱. (위, 아래)
//==============================================================================
fn multiply(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
	let (product, error) = two_product(left.0, right.0);
	let error = error + (left.0 * right.1 + left.1 * right.0);
	fast_two_sum(product, error)
}

//==============================================================================
/// 두 배 정밀도 합.
///
/// # 인자
/// - `left`: 왼쪽 값. (위, 아래)
/// - `right`: 오른쪽 값. (위, 아래)
///
/// # 돌려줌
/// 합. (위, 아래)
//==============================================================================
fn add(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
	let (sum, error) = two_sum(left.0, right.0);
	let (low_sum, low_error) = two_sum(left.1, right.1);
	let (sum, error) = fast_two_sum(sum, error + low_sum);
	fast_two_sum(sum, error + low_error)
}

//==============================================================================
/// 두 배 정밀도 나눗셈.
///
/// # 인자
/// - `left`: 나뉠 값. (위, 아래)
/// - `right`: 나눌 값. (위, 아래)
///
/// # 돌려줌
/// 몫. (위, 아래)
//==============================================================================
fn divide(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
	let first = left.0 / right.0;
	let product = multiply(right, (first, 0.0));
	let remainder = add(left, (-product.0, -product.1));
	let second = remainder.0 / right.0;
	let product = multiply(right, (second, 0.0));
	let remainder = add(remainder, (-product.0, -product.1));
	let third = remainder.0 / right.0;
	let (sum, error) = fast_two_sum(first, second);
	fast_two_sum(sum, error + third)
}

//==============================================================================
/// 2^exponent. (정규 범위)
///
/// # 인자
/// - `exponent`: 지수. (-1022 ~ 1023)
///
/// # 돌려줌
/// 2 의 거듭제곱.
//==============================================================================
fn power_of_two(exponent: i32) -> f64 {
	f64::from_bits(((exponent + 1023) as u64) << 52)
}

//==============================================================================
/// 작은 쪽부터 놓인 비트 배열에서 position 부터 64 비트 꺼내기. (밖은 0)
///
/// # 인자
/// - `limbs`: 64 비트 낱말들. (0 번이 가장 아래)
/// - `position`: 꺼낼 맨 아래 비트 자리. (음수면 그 아래는 0)
///
/// # 돌려줌
/// 64 비트.
//==============================================================================
fn bits_at(limbs: &[u64], position: i32) -> u64 {
	let read = |index: i32| -> u64 {
		if index < 0 || index as usize >= limbs.len() {
			0
		}
		else {
			limbs[index as usize]
		}
	};
	let index = position.div_euclid(64);
	let shift = position.rem_euclid(64) as u32;
	if shift == 0 {
		read(index)
	}
	else {
		(read(index) >> shift) | (read(index + 1) << (64 - shift))
	}
}

//==============================================================================
/// x 를 π/2 의 배수와 나머지로 줄이기. (Payne-Hanek, |x| >= π/4, 유한)
///
/// # 인자
/// - `value`: 줄일 값. (0 이상)
///
/// # 돌려줌
/// (사분면 0 ~ 3, 나머지 r 의 두 배 정밀도 값, |r| <= π/4).
//==============================================================================
fn reduce(value: f64) -> (u32, (f64, f64)) {
	let bits = value.to_bits();
	let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
	let exponent = ((bits >> 52) & 0x7FF) as i32 - 1075;
	// 2/π 의 start 번째 소수 비트부터 256 비트. (그 위 비트는 x 에 곱하면 4 의 배수라 버린다)
	let start = (exponent - 2).max(0);
	let mut window = [0u64; 4];
	for (index, limb) in window.iter_mut().enumerate() {
		// 표에서 비트 i 는 낱말 i / 64 의 위에서 i % 64 번째. 창의 낱말 3 이 가장 위다.
		let first_bit = start + 64 * (3 - index as i32);
		let word = (first_bit / 64) as usize;
		let shift = (first_bit % 64) as u32;
		let high = TWO_OVER_PI.get(word).copied().unwrap_or(0);
		let low = TWO_OVER_PI.get(word + 1).copied().unwrap_or(0);
		*limb = if shift == 0 { high } else { (high << shift) | (low >> (64 - shift)) };
	}
	// 곱 = 가수 x 창. (309 비트까지, 작은 쪽부터)
	let mut product = [0u64; 5];
	let mut carry = 0u128;
	for index in 0..4 {
		let part = mantissa as u128 * window[index] as u128 + carry;
		product[index] = part as u64;
		carry = part >> 64;
	}
	product[4] = carry as u64;
	// 소수 비트 수. (곱 x 2^-fraction_bits 가 x x 2/π mod 4 의 근사, 버린 꼬리는 2^-201 아래)
	let fraction_bits = start + 256 - exponent;
	let mut quadrant = (bits_at(&product, fraction_bits) & 3) as u32;
	// 소수가 1/2 이상이면 다음 사분면에서 음수 나머지로.
	let negative = bits_at(&product, fraction_bits - 64) >> 63 == 1;
	let mut fraction = [0u64; 5];
	for index in 0..5 {
		let position = index as i32 * 64;
		let mut word = bits_at(&product, position);
		if position + 64 > fraction_bits {
			let keep = fraction_bits - position;
			word = if keep <= 0 { 0 } else { word & (u64::MAX >> (64 - keep)) };
		}
		fraction[index] = word;
	}
	if negative {
		quadrant = (quadrant + 1) & 3;
		// 2^fraction_bits - fraction. (fraction_bits 비트 안의 2 의 보수)
		let mut borrow = 0u64;
		for limb in fraction.iter_mut() {
			let (difference, overflow_first) = 0u64.overflowing_sub(*limb);
			let (difference, overflow_second) = difference.overflowing_sub(borrow);
			*limb = difference;
			borrow = (overflow_first || overflow_second) as u64;
		}
		for index in 0..5 {
			let position = index as i32 * 64;
			if position + 64 > fraction_bits {
				let keep = fraction_bits - position;
				fraction[index] = if keep <= 0 { 0 } else { fraction[index] & (u64::MAX >> (64 - keep)) };
			}
		}
	}
	// 맨 위 1 비트부터 128 비트를 두 배 정밀도로.
	let mut leading = -1;
	for index in (0..5).rev() {
		if fraction[index] != 0 {
			leading = index as i32 * 64 + 63 - fraction[index].leading_zeros() as i32;
			break;
		}
	}
	if leading < 0 {
		return (quadrant, (0.0, 0.0));
	}
	let top = bits_at(&fraction, leading - 63);
	let next = bits_at(&fraction, leading - 127);
	let high_part = (top >> 11) as f64 * power_of_two(leading - 52 - fraction_bits);
	let rest = (((top & 0x7FF) as u128) << 64) | next as u128;
	let low_part = rest as f64 * power_of_two(leading - 127 - fraction_bits);
	let (high_part, low_part) = fast_two_sum(high_part, low_part);
	let mut remainder = multiply((high_part, low_part), HALF_PI);
	if negative {
		remainder = (-remainder.0, -remainder.1);
	}
	(quadrant, remainder)
}

//==============================================================================
/// |r| <= π/4 의 사인과 코사인. (두 배 정밀도 테일러 급수)
///
/// # 인자
/// - `remainder`: r. (위, 아래)
///
/// # 돌려줌
/// (sin r, cos r). 각각 (위, 아래)
//==============================================================================
fn sine_cosine(remainder: (f64, f64)) -> ((f64, f64), (f64, f64)) {
	let square = multiply(remainder, remainder);
	let signed = |coefficient: (f64, f64), index: usize| -> (f64, f64) {
		if index % 2 == 1 {
			(-coefficient.0, -coefficient.1)
		}
		else {
			coefficient
		}
	};
	let last = SIN_COEFFICIENTS.len() - 1;
	let mut sine = signed(SIN_COEFFICIENTS[last], last);
	for index in (0..last).rev() {
		sine = add(signed(SIN_COEFFICIENTS[index], index), multiply(sine, square));
	}
	let last = COS_COEFFICIENTS.len() - 1;
	let mut cosine = signed(COS_COEFFICIENTS[last], last);
	for index in (0..last).rev() {
		cosine = add(signed(COS_COEFFICIENTS[index], index), multiply(cosine, square));
	}
	(multiply(sine, remainder), cosine)
}

//==============================================================================
/// 사분면과 나머지. (부호를 뗀 |x| 에 대해)
///
/// # 인자
/// - `value`: |x|. (유한, TINY 이상)
///
/// # 돌려줌
/// (사분면, sin r, cos r).
//==============================================================================
fn evaluate(value: f64) -> (u32, (f64, f64), (f64, f64)) {
	let (quadrant, remainder) = if value < QUARTER_PI { (0, (value, 0.0)) } else { reduce(value) };
	let (sine, cosine) = sine_cosine(remainder);
	(quadrant, sine, cosine)
}

//==============================================================================
/// 사인. (바르게 반올림, 크롬 Math.sin 과 같은 비트)
///
/// # 인자
/// - `value`: 각. (라디안)
///
/// # 돌려줌
/// 사인 값. (무한대와 NaN 은 NaN)
//==============================================================================
pub fn sin(value: f64) -> f64 {
	if !value.is_finite() {
		return f64::NAN;
	}
	let magnitude = value.abs();
	if magnitude < TINY {
		return value;
	}
	let (quadrant, sine, cosine) = evaluate(magnitude);
	let result = match quadrant {
		0 => sine.0,
		1 => cosine.0,
		2 => -sine.0,
		_ => -cosine.0,
	};
	if value < 0.0 { -result } else { result }
}

//==============================================================================
/// 코사인. (바르게 반올림, 크롬 Math.cos 와 같은 비트)
///
/// # 인자
/// - `value`: 각. (라디안)
///
/// # 돌려줌
/// 코사인 값. (무한대와 NaN 은 NaN)
//==============================================================================
pub fn cos(value: f64) -> f64 {
	if !value.is_finite() {
		return f64::NAN;
	}
	let magnitude = value.abs();
	if magnitude < TINY {
		return 1.0;
	}
	let (quadrant, sine, cosine) = evaluate(magnitude);
	match quadrant {
		0 => cosine.0,
		1 => -sine.0,
		2 => -cosine.0,
		_ => sine.0,
	}
}

//==============================================================================
/// 탄젠트. (바르게 반올림, 크롬 Math.tan 과 같은 비트)
///
/// # 인자
/// - `value`: 각. (라디안)
///
/// # 돌려줌
/// 탄젠트 값. (무한대와 NaN 은 NaN)
//==============================================================================
pub fn tan(value: f64) -> f64 {
	if !value.is_finite() {
		return f64::NAN;
	}
	let magnitude = value.abs();
	if magnitude < TINY {
		return value;
	}
	let (quadrant, sine, cosine) = evaluate(magnitude);
	let result = if quadrant % 2 == 0 { divide(sine, cosine).0 } else { -divide(cosine, sine).0 };
	if value < 0.0 { -result } else { result }
}

//==============================================================================
// 아래 아크 코사인, 아크 탄젠트 둘, 빗변. (원본이 도는 크롬의 Math.acos, atan2, hypot 과 같은 비트)
//==============================================================================

// atan(i/64) 의 두 배 정밀도 값. (위, 아래, i = 0..64, mpmath 로 뽑음)
const ATAN_TABLE: [(f64, f64); 65] = [(0.0, 0.0), (0.015623728620476831, -4.913600136566304e-19), (0.031239833430268277, -1.188442711587748e-18), (0.046840712915969654, -1.655677442254952e-19), (0.06241880999595735, -1.5490756308295046e-18), (0.0779666338315423, 5.804551873143357e-18), (0.09347678115858947, -6.2844725995420954e-18), (0.10894195698986579, 6.8267122072409585e-18), (0.12435499454676144, -3.1253241424539383e-18), (0.13970887428916365, -2.9579864247315813e-18), (0.15499674192394097, 9.585415594114324e-18), (0.1702119252854744, -3.541164079802125e-18), (0.18534794999569476, 4.180692268843079e-18), (0.2003985538258785, 3.1399542871844493e-18), (0.21535769969773805, 4.738160130078733e-19), (0.23021958727684372, 1.2313404529142703e-17), (0.24497866312686414, 1.0698755618734451e-17), (0.2596296294082575, 1.9238754924615304e-17), (0.2741674511196588, 8.261353575163773e-18), (0.2885873618940774, -1.428369957377257e-17), (0.3028848683749714, -1.1010827903001369e-17), (0.31705575320914703, -1.893928924292642e-17), (0.3310960767041321, -7.952610375793799e-18), (0.34500217720710513, -2.2938804755578304e-17), (0.35877067027057225, -2.4623815582638635e-17), (0.3723984466767542, 1.9612311504845653e-17), (0.38588266939807375, 2.378822732491941e-17), (0.39922076957525254, 2.246598105617042e-17), (0.4124104415973873, -1.587652227770689e-17), (0.42544963737004227, 2.3315530741892885e-17), (0.43833655985795783, -2.494277030626541e-17), (0.4510696559885235, -2.2703795229420475e-17), (0.4636476090008061, 2.2698777452961687e-17), (0.4760693303227612, 1.4654487332256713e-17), (0.48833395105640554, -1.1373236189329585e-17), (0.5004408131472942, -4.7181675085518756e-17), (0.5123894603107377, -2.5462781472855804e-17), (0.5241796287829132, 5.520094119641666e-18), (0.5358112379604637, -4.0637956834825575e-18), (0.5472843809874369, 4.923709671396255e-17), (0.5585993153435624, -5.4556305485916264e-18), (0.5697564534829784, 1.2255062085054184e-17), (0.5807563535676704, -1.441464378193067e-17), (0.5915997103351114, 4.920495453686772e-17), (0.6022873461349642, 2.950430737228402e-17), (0.6128202021652414, -3.1552061848586226e-17), (0.6231993299340659, 2.672403885140095e-17), (0.6334258829691446, -2.7290767436015276e-17), (0.6435011087932844, 1.5834785051444286e-17), (0.6534263411807619, 3.5800634857340095e-17), (0.6632029927060933, -3.076054864429649e-17), (0.6728325475937632, -1.899315009714705e-17), (0.6823165548747481, 6.943223671560008e-18), (0.6916566218531999, -8.117151192285796e-18), (0.7008544078844502, -1.987626234335816e-17), (0.7099116184635249, -4.597166450584887e-17), (0.7188299996216245, -2.1478388444456983e-17), (0.7276113326265107, 2.569325697391839e-18), (0.7362574289814281, 3.473937648299457e-17), (0.7447701257160751, 3.708315849135547e-17), (0.7531512809621944, -2.4256934659182068e-17), (0.7614027698055784, 9.850030332752822e-18), (0.7695264804056583, -3.704991905602721e-17), (0.7775243103733478, -2.6676490951944502e-17), (0.7853981633974483, 3.061616997868383e-17)];

// asin 급수 계수 (2n)!/(4^n (n!)^2 (2n+1)) 의 두 배 정밀도 값. (n = 0..51, 0.25^n 을 곱하면 2^-112 아래까지)
const ASIN_COEFFICIENTS: [(f64, f64); 52] = [(1.0, 0.0), (0.16666666666666666, 9.25185853854297e-18), (0.075, 2.7755575615628915e-18), (0.044642857142857144, -9.912705577010326e-19), (0.030381944444444444, 3.854941057726238e-19), (0.022372159090909092, -9.462128050782583e-19), (0.017352764423076924, -8.006416042969879e-19), (0.01396484375, -6.938893903907229e-19), (0.011551800896139705, 8.163404592832033e-19), (0.009761609529194078, 5.478074134663601e-19), (0.008390335809616815, 4.130293990420969e-19), (0.0073125258735988454, -3.394024192128536e-19), (0.006447210311889649, -3.1225022567582527e-19), (0.005740037670841924, -1.2849803525754126e-19), (0.005153309682319905, -3.888173308223878e-19), (0.004660143486915096, -2.7979410902851727e-20), (0.004240907093679363, -1.5770213417970974e-19), (0.003880964558837669, 1.858632295689436e-19), (0.0035692053938259347, -1.992587776459846e-19), (0.003297059503473485, -1.3344026738283132e-19), (0.0030578216492580306, 2.1155164341180575e-20), (0.002846178401108942, 1.1094161764967953e-19), (0.00265787063820729, -1.4456028966473392e-19), (0.0024894486782468836, -8.304527278612374e-20), (0.002338091892111975, 1.4161007967157608e-19), (0.0022014739737101384, -1.5306383611560064e-19), (0.0020776610325181676, -1.4728784229991757e-19), (0.0019650336162772837, -7.096596038086938e-20), (0.0018622264064031275, -5.325905408700723e-20), (0.0017680811205154183, -5.880418562633244e-20), (0.0016816093935831068, 1.4219044885055797e-20), (0.001601963275351444, 4.818676322157797e-20), (0.0015284115961225677, -3.8364076872564005e-20), (0.0014603208940791154, -1.375480368078625e-20), (0.0013971399176302534, 1.11955659115351e-20), (0.0013383869512751784, -5.506907161304015e-20), (0.0012836393876290285, 3.621355929288591e-20), (0.001232525098500017, -9.357455312634007e-20), (0.0011847152561624392, 8.758128167121453e-20), (0.0011399183307022236, 8.194708704184068e-20), (0.0010978750465914472, 4.854022464579631e-20), (0.0010583541258722428, 9.018646571938304e-20), (0.0010211486797106276, 8.391832465295875e-20), (0.0009860731369833312, 7.622485745671054e-20), (0.0009529606197429564, 3.077887041034637e-20), (0.0009216606921836334, 5.241836681717014e-20), (0.0008920374230917097, 5.279707379901296e-21), (0.0008639677124658675, 1.7643208659492762e-20), (0.0008373398416027121, -3.173157570752775e-20), (0.0008120522129086701, 8.562849812695216e-21), (0.0007880122513582056, -1.684626855858876e-21), (0.000765135444137165, -4.6754051814894244e-20)];

// π 의 두 배 정밀도 값. (위, 아래)
const PI: (f64, f64) = (3.141592653589793, 1.2246467991473532e-16);

// π/4, 3π/4 의 두 배 정밀도 값. (위, 아래)
const QUARTER_PI_PAIR: (f64, f64) = (0.7853981633974483, 3.061616997868383e-17);
const THREE_QUARTER_PI: (f64, f64) = (2.356194490192345, 9.184850993605148e-17);

//==============================================================================
/// |위| >= |아래| 일 때 두 수의 합과 오차. (LLVM libc exact_add 의 차례, 위 + 아래 - 위)
///
/// # 인자
/// - `high`: 앞 값.
/// - `low`: 뒤 값.
///
/// # 돌려줌
/// (합, 오차).
//==============================================================================
fn ordered_two_sum(high: f64, low: f64) -> (f64, f64) {
	let sum = high + low;
	let part = sum - high;
	(sum, low - part)
}

//==============================================================================
/// 두 배 정밀도 합. (LLVM libc fputil::add 의 차례)
///
/// # 인자
/// - `left`: 앞 값. (위, 아래)
/// - `right`: 뒤 값. (위, 아래)
///
/// # 돌려줌
/// 합. (위, 아래)
//==============================================================================
fn ordered_add(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
	let (sum, error) = ordered_two_sum(left.0, right.0);
	let low = left.1 + right.1;
	ordered_two_sum(sum, error + low)
}

//==============================================================================
/// 두 수의 곱과 그 오차. (곱셈 덧셈 합치기로 오차를 정확히)
///
/// # 인자
/// - `left`: 왼쪽 값.
/// - `right`: 오른쪽 값.
///
/// # 돌려줌
/// (곱, 오차).
//==============================================================================
fn fused_two_product(left: f64, right: f64) -> (f64, f64) {
	let product = left * right;
	(product, left.mul_add(right, -product))
}

//==============================================================================
/// 두 배 정밀도 나누기. (LLVM libc fputil::div 의 곱셈 덧셈 합치기 판)
///
/// # 인자
/// - `left`: 나뉠 값. (위, 아래)
/// - `right`: 나눌 값. (위, 아래)
///
/// # 돌려줌
/// 몫. (위, 아래)
//==============================================================================
fn fused_divide(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
	let inverse = 1.0 / right.0;
	let high = left.0 * inverse;
	let error_high = right.0.mul_add(-high, left.0);
	let error_low = right.1.mul_add(-high, left.1);
	(high, inverse * (error_high + error_low))
}

//==============================================================================
/// |값| <= 2^-7 의 아크 탄젠트. (9 차 테일러 다항식, LLVM libc atan_eval 의 차례)
///
/// # 인자
/// - `value`: 값. (위, 아래)
///
/// # 돌려줌
/// 아크 탄젠트. (위, 아래)
//==============================================================================
fn atan_small(value: (f64, f64)) -> (f64, f64) {
	let square = value.0 * value.0;
	let first = square.mul_add(0.2, -0.3333333333333333);
	let second = square.mul_add(0.1111111111111111, -0.14285714285714285);
	let cube = square * value.0;
	let fourth = square * square;
	let middle = fourth.mul_add(second, first);
	let low_part = (fourth - square).mul_add(value.1, value.1);
	(value.0, cube.mul_add(middle, low_part))
}

//==============================================================================
/// 아크 탄젠트 둘. (크롬 Math.atan2 와 같은 비트)
/// - 크롬 158 의 V8 은 llvm-libc 의 atan2 를 부르고, 그 셈은 바르게 반올림이 아니다(문서에 1 ULP). 같은 비트를 내려고
///   LLVM libc(src/__support/math/atan2.h, Apache-2.0 WITH LLVM-exception)의 셈 차례를 그대로 옮겼다.
/// - 곱셈 덧셈은 크롬 arm64(맥 애플 실리콘)처럼 합친다(mul_add). 합치지 않는 판(x64 크롬의 셈)도 같은 70,484 입력에서 모두 같았다.
///   두 판은 셈 중간의 아래 자리가 달라 이론상 드문 입력에서 끝자리가 갈릴 수 있다. (못 찾음, 짐작)
///
/// # 인자
/// - `y`: 세로 값.
/// - `x`: 가로 값.
///
/// # 돌려줌
/// (x, y) 의 각. (라디안, -π ~ π)
//==============================================================================
pub fn atan2(y: f64, x: f64) -> f64 {
	const SIGN_BIT: u64 = 1 << 63;
	const ZERO: (f64, f64) = (0.0, 0.0);
	const NEGATIVE_ZERO: (f64, f64) = (-0.0, -0.0);
	let negative_pi = (-PI.0, -PI.1);
	let negative_half_pi = (-HALF_PI.0, -HALF_PI.1);
	let x_negative = x.is_sign_negative();
	let y_negative = y.is_sign_negative();
	let x_bits = x.to_bits() & !SIGN_BIT;
	let y_bits = y.to_bits() & !SIGN_BIT;
	let reciprocal = x_bits < y_bits;
	let (small_bits, large_bits) = if reciprocal { (x_bits, y_bits) } else { (y_bits, x_bits) };
	let mut small_exponent = (small_bits >> 52) as u32;
	let mut large_exponent = (large_bits >> 52) as u32;
	let mut numerator = f64::from_bits(small_bits);
	let mut denominator = f64::from_bits(large_bits);
	if large_exponent > 0x7FF - 128 || small_exponent < 128 {
		if x.is_nan() || y.is_nan() {
			return f64::NAN;
		}
		let kind = |value: f64| -> usize {
			if value == 0.0 {
				0
			}
			else if value.is_infinite() {
				2
			}
			else {
				1
			}
		};
		let (x_kind, y_kind) = (kind(x), kind(y));
		if x_kind != 1 || y_kind != 1 {
			let table: [[[(f64, f64); 2]; 3]; 3] = [[[ZERO, PI], [ZERO, PI], [ZERO, PI]], [[HALF_PI, HALF_PI], [ZERO, ZERO], [ZERO, PI]], [[HALF_PI, HALF_PI], [HALF_PI, HALF_PI], [QUARTER_PI_PAIR, THREE_QUARTER_PI]]];
			let result = table[y_kind][x_kind][x_negative as usize];
			let sign: f64 = if y_negative { -1.0 } else { 1.0 };
			return sign.mul_add(result.0, sign * result.1);
		}
		let scale_up = small_exponent < 128;
		let scale_down = large_exponent > 0x7FF - 128;
		if scale_up {
			numerator *= 18446744073709551616.0;
			if !scale_down {
				denominator *= 18446744073709551616.0;
			}
		}
		else if scale_down {
			denominator *= 5.421010862427522e-20;
			numerator *= 5.421010862427522e-20;
		}
		small_exponent = (numerator.to_bits() >> 52) as u32;
		large_exponent = (denominator.to_bits() >> 52) as u32;
	}
	let final_sign: f64 = if (x_negative != y_negative) != reciprocal { -1.0 } else { 1.0 };
	let adjust: [[[(f64, f64); 2]; 2]; 2] = [[[ZERO, negative_half_pi], [NEGATIVE_ZERO, negative_half_pi]], [[negative_pi, HALF_PI], [negative_pi, HALF_PI]]];
	let constant = adjust[x_negative as usize][y_negative as usize][reciprocal as usize];
	let exponent_difference = large_exponent.wrapping_sub(small_exponent);
	if exponent_difference > 54 {
		return final_sign.mul_add(constant.0, final_sign * (constant.1 + numerator / denominator));
	}
	let nearest = (64.0 * numerator / denominator).round_ties_even();
	let index = nearest as usize;
	let step = nearest * 0.015625;
	let numerator_step = fused_two_product(numerator, step);
	let denominator_step = fused_two_product(denominator, step);
	let reduced_numerator = ordered_two_sum(numerator - denominator_step.0, -denominator_step.1);
	let mut reduced_denominator = ordered_two_sum(denominator, numerator_step.0);
	reduced_denominator.1 += numerator_step.1;
	let quotient = fused_divide(reduced_numerator, reduced_denominator);
	let series = atan_small(quotient);
	let result = ordered_add(constant, ordered_add(ATAN_TABLE[index], series));
	result.0 * final_sign + result.1 * final_sign
}

//==============================================================================
/// 0 <= u <= 1/4 에서 asin(√u)/√u 급수. (두 배 정밀도 호너)
///
/// # 인자
/// - `square`: u. (위, 아래)
///
/// # 돌려줌
/// 급수 값. (위, 아래)
//==============================================================================
fn asin_series(square: (f64, f64)) -> (f64, f64) {
	let mut sum = ASIN_COEFFICIENTS[ASIN_COEFFICIENTS.len() - 1];
	for index in (0..ASIN_COEFFICIENTS.len() - 1).rev() {
		sum = add(multiply(sum, square), ASIN_COEFFICIENTS[index]);
	}
	sum
}

//==============================================================================
/// 아크 코사인. (바르게 반올림, 크롬 Math.acos 와 같은 비트)
/// - 크롬 158 의 V8 은 llvm-libc 의 acos 를 부르고, 그 셈은 바르게 반올림이다(문서에 네 반올림 모두). 바르게 반올림한 값은 하나뿐이라
///   셈법이 달라도 같은 비트가 나온다. 셈은 직접 짰다: |x| <= 1/2 는 π/2 - asin(x), 그 밖은 2 asin(√((1 - |x|)/2)) (음수는 π 에서 뺌).
///
/// # 인자
/// - `value`: 값. (-1 ~ 1)
///
/// # 돌려줌
/// 각. (라디안, 0 ~ π, 범위 밖과 NaN 은 NaN)
//==============================================================================
pub fn acos(value: f64) -> f64 {
	let magnitude = value.abs();
	if !(magnitude <= 1.0) {
		return f64::NAN;
	}
	if magnitude == 1.0 {
		return if value > 0.0 { 0.0 } else { PI.0 };
	}
	if magnitude <= 0.5 {
		let square = two_product(value, value);
		let arcsine = multiply((value, 0.0), asin_series(square));
		let result = add(HALF_PI, (-arcsine.0, -arcsine.1));
		return result.0 + result.1;
	}
	// 1 - |x| 는 정확하다. (Sterbenz)
	let half = (1.0 - magnitude) * 0.5;
	let root = half.sqrt();
	let (product, error) = two_product(root, root);
	let root_low = ((half - product) - error) / (root + root);
	let arcsine = multiply((root, root_low), asin_series((half, 0.0)));
	let doubled = (arcsine.0 * 2.0, arcsine.1 * 2.0);
	let result = if value > 0.0 { doubled } else { add(PI, (-doubled.0, -doubled.1)) };
	result.0 + result.1
}

//==============================================================================
/// 빗변 길이 √(a² + b²). (크롬 Math.hypot 두 인자와 같은 셈, V8 src/builtins/math.tq FastMathHypot)
///
/// # 인자
/// - `left`: 첫 값.
/// - `right`: 둘째 값.
///
/// # 돌려줌
/// 길이. (무한대가 있으면 무한대, 그다음 NaN 이 있으면 NaN)
//==============================================================================
pub fn hypot(left: f64, right: f64) -> f64 {
	hypot_list(&[left, right])
}

//==============================================================================
/// 여러 값의 빗변 길이. (크롬 Math.hypot 과 같은 셈, V8 src/builtins/math.tq)
/// - 셋까지는 V8 FastMathHypot: 가장 큰 값으로 나눈 제곱의 합(셋은 앞 둘 합의 오차를 셋째에서 뺌)의 제곱근에 가장 큰 값을 곱한다.
/// - 넷부터는 V8 의 느린 길: 가장 큰 값으로 나눈 제곱을 Kahan 합으로 더한다.
///
/// # 인자
/// - `value_list`: 값들.
///
/// # 돌려줌
/// 길이. (빈 목록은 0, 무한대가 있으면 무한대, 그다음 NaN 이 있으면 NaN)
//==============================================================================
pub fn hypot_list(value_list: &[f64]) -> f64 {
	match value_list.len() {
		0 => 0.0,
		1 => value_list[0].abs(),
		2 | 3 => {
			if value_list.iter().any(|value| value.is_infinite()) {
				return f64::INFINITY;
			}
			if value_list.iter().any(|value| value.is_nan()) {
				return f64::NAN;
			}
			let largest = value_list.iter().fold(0.0f64, |largest, value| if value.abs() > largest { value.abs() } else { largest });
			if largest == 0.0 {
				return 0.0;
			}
			let power = |value: f64| (value.abs() / largest) * (value.abs() / largest);
			let first = power(value_list[0]);
			let second = power(value_list[1]);
			if value_list.len() == 2 {
				return (first + second).sqrt() * largest;
			}
			let compensation = (first + second) - first - second;
			let third = power(value_list[2]) - compensation;
			(first + second + third).sqrt() * largest
		}
		_ => {
			let mut largest = 0.0f64;
			let mut has_nan = false;
			for value in value_list {
				if value.is_nan() {
					has_nan = true;
				}
				else if value.abs() > largest {
					largest = value.abs();
				}
			}
			if largest == f64::INFINITY {
				return f64::INFINITY;
			}
			if has_nan {
				return f64::NAN;
			}
			if largest == 0.0 {
				return 0.0;
			}
			let mut sum = 0.0;
			let mut compensation = 0.0;
			for value in value_list {
				let scaled = value.abs() / largest;
				let summand = scaled * scaled - compensation;
				let preliminary = sum + summand;
				compensation = (preliminary - sum) - summand;
				sum = preliminary;
			}
			sum.sqrt() * largest
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 크롬 158 의 Math.sin, cos, tan 과 같은 비트. (±7 부터 ±1e15, 아주 작고 큰 값)
	//==============================================================================
	#[test]
	fn same_bits_as_chrome() {
		let cases: [(u64, u64, u64, u64); 8] = [
			(0xc015dd14c34541fa, 0x3fe7565b04bec33c, 0x3fe5e4fe0e4ca75d, 0x3ff10dec0fb50299),
			(0xc042605e53f8e7fc, 0x3fe9f580cd3c709d, 0x3fe2b636d61efb6c, 0x3ff6326a03aa60d1),
			(0xc09ea28b01beb5fc, 0xbfd1ce3d4a7cb5d4, 0x3feebc934747ac82, 0xbfd2899927cd54ed),
			(0xc14c8d6dbd430726, 0x3febcecd0467b7cb, 0x3fdfab2ab501d153, 0x3ffc194ab2afa8c2),
			(0x42f73cfba4dde84c, 0x3fe61e4740f43fff, 0xbfe7201717cd858a, 0xbfee9b3fe263e045),
			(0x025815902e6b94f4, 0x025815902e6b94f4, 0x3ff0000000000000, 0x025815902e6b94f4),
			(0x4c56f94fe4408580, 0x3fef0b80d6d838b0, 0x3fcf09df5e283ad8, 0x401000d7338fd495),
			(0xdc108121dacf2af8, 0xbfed7edb63d89f8f, 0x3fd8d2078c7eb388, 0xc0030382e39f06fa),
		];
		for (value, sine, cosine, tangent) in cases {
			let value = f64::from_bits(value);
			assert_eq!(sin(value).to_bits(), sine, "sin {:e}", value);
			assert_eq!(cos(value).to_bits(), cosine, "cos {:e}", value);
			assert_eq!(tan(value).to_bits(), tangent, "tan {:e}", value);
		}
		// 줄이기가 어려운 큰 값. (참값 -0.8522008497671888)
		assert_eq!(sin(1e22), -0.8522008497671888);
		assert_eq!(sin(-0.0).to_bits(), (-0.0f64).to_bits());
		assert_eq!(cos(0.0), 1.0);
		assert!(sin(f64::INFINITY).is_nan());
		assert!(tan(f64::NAN).is_nan());
	}

	//==============================================================================
	/// 크롬 158 의 Math.acos, atan2, hypot 과 같은 비트. (끝 ±1 둘레, 아주 작은 결과, 큰 값, 셋과 넷 인자)
	//==============================================================================
	#[test]
	fn inverse_and_hypot_same_bits_as_chrome() {
		let arc_cosine: [(u64, u64); 6] = [(0x3fe0000000000000, 0x3ff0c152382d7366), (0xbfe832948c881f94, 0x40036d0ac76a72b0), (0x0000000000176293, 0x3ff921fb54442d18), (0xbfefffffffffffff, 0x400921fb52442d18), (0x3fefffffffffffff, 0x3e50000000000000), (0xbff0000000000000, 0x400921fb54442d18)];
		for (value, expected) in arc_cosine {
			assert_eq!(acos(f64::from_bits(value)).to_bits(), expected, "acos {:016x}", value);
		}
		let arc_tangent: [(u64, u64, u64); 7] = [(0x3ff41c539fa2def6, 0xc006614e58a53868, 0x4005c12c6cde1eb2), (0xc021e7fe7f50b6a9, 0xc0202208a4fa189a, 0xc0026ee4325000d7), (0x406877de99502692, 0xc0550833ddaf739b, 0x3fffa09b7c7f7002), (0x4044704223385c7a, 0x40447042233a264f, 0x3fe921fb5442c6b0), (0x3e4bed6bfcdc60ea, 0x4027325194951185, 0x3e13435e158190fc), (0x16c083ee925ad7c0, 0x5ed019320c650801, 0x0000000000000000), (0x9d541fb17662769e, 0x5d3bf8d9ac3b3430, 0x800b82c391960771)];
		for (y, x, expected) in arc_tangent {
			assert_eq!(atan2(f64::from_bits(y), f64::from_bits(x)).to_bits(), expected, "atan2 {:016x} {:016x}", y, x);
		}
		assert_eq!(atan2(-0.0, -1.0), -std::f64::consts::PI);
		assert_eq!(atan2(f64::INFINITY, f64::NEG_INFINITY).to_bits(), 0x4002d97c7f3321d2);
		assert!(atan2(f64::NAN, 1.0).is_nan());
		let hypotenuse: [(&[u64], u64); 5] = [(&[0x4027d64a7eb75c96, 0xc044069aab2a73fc], 0x4044e4c78ff67372), (&[0xc06cd8d4a8014906, 0x40769f80ba4b9d6d], 0x407ad4700c361293), (&[0x40712569296115fc, 0x407894c023f5a03e, 0x4069c9b6eec2a8d7], 0x408050294ffe5407), (&[0x405c19bac3dd959f, 0xf6204cb16f19602b, 0xc00df40508a0b420, 0xc0662ee048ad58ba], 0x76204cb16f19602b), (&[0xc078a151cc3951bd, 0x4cff1c46be8016b3, 0xbffdc9e93ca8f856, 0x4079623d8d1c47ae], 0x4cff1c46be8016b3)];
		for (value_list, expected) in hypotenuse {
			let value_list: Vec<f64> = value_list.iter().map(|value| f64::from_bits(*value)).collect();
			assert_eq!(hypot_list(&value_list).to_bits(), expected, "hypot {:?}", value_list);
		}
		assert_eq!(hypot(3.0, 4.0), 5.0);
		assert_eq!(hypot(f64::NAN, f64::INFINITY), f64::INFINITY);
		assert!(hypot(f64::NAN, 1.0).is_nan());
		assert_eq!(hypot_list(&[]), 0.0);
	}
}
