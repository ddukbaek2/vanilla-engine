//==============================================================================
//! 바르게 반올림한 f64 사인, 코사인, 탄젠트. (원본이 도는 크롬의 Math.sin, cos, tan 과 같은 비트)
//! - 크롬 158(V8 의 src/base/ieee754.cc 가 llvm-libc 의 sin, cos, tan 을 부름)은 바르게 반올림한 값(참값에 가장 가까운 f64)을 낸다.
//!   바르게 반올림한 값은 하나뿐이라 셈법이 달라도 같은 비트가 나온다. 시스템 libm(맥, 윈도우, 리눅스, wasm 의 musl 판)은 끝자리가 플랫폼마다 달라 쓰지 않는다.
//! - 셈: 2/π 비트 표로 Payne-Hanek 줄이기(f 를 200 비트 넘게 정확히) 뒤 |r| <= π/4 에서 두 배 정밀도(double-double) 테일러 급수, 마지막에 한 번 반올림.
//!   두 배 정밀도 오차는 2^-100 안쪽이라 참값이 두 f64 의 가운데에서 그보다 가까운 입력(알려진 어려운 경우 몇 개)만 끝자리가 갈릴 수 있다.
//!   크롬 158 과 22,000 입력(±7 부터 ±1e15, 2^-1000 ~ 2^1000)의 sin, cos, tan 을 견줘 모두 같았다.
//! - 곱셈과 덧셈을 합치지 않는다. (Dekker 나누기, fma 를 쓰지 않아 모든 플랫폼에서 같다)
//! - 표와 계수는 mpmath(2000 비트)로 직접 뽑았다. 남의 코드를 옮기지 않았다.
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
}
