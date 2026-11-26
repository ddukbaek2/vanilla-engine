//==============================================================================
//! fdlibm 의 log, log10, pow. (크로미엄 third_party/fdlibm/ieee754.cc 를 옮긴 것, V8 이 NetBSD fdlibm 을 고친 판)
//! - 크롬 HRTF 의 커널 보간(FFTFrame::InterpolateFrequencyComponents)이 크기를 데시벨로 섞을 때 fdlibm::log10, fdlibm::pow 를 부른다.
//!   시스템 libm 은 플랫폼마다 끝자리가 달라 크롬과 같은 값을 내려고 같은 셈을 옮겼다.
//! - 낱말 꺼내기(EXTRACT_WORDS, GET_HIGH_WORD 같은)는 f64 의 비트 위 32, 아래 32 다. 곱셈과 덧셈은 합치지 않는다. (크롬 -ffp-contract=off)
//! - 원본 고지 (fdlibm LICENSE, 고지를 지키는 조건으로 쓰게 한다):
//!   Copyright (C) 1993-2004 by Sun Microsystems, Inc. All rights reserved.
//!   Developed at SunSoft, a Sun Microsystems, Inc. business.
//!   Permission to use, copy, modify, and distribute this
//!   software is freely granted, provided that this notice
//!   is preserved.
//==============================================================================

//==============================================================================
/// f64 의 위 32 비트. (GET_HIGH_WORD, 부호 있는 정수)
///
/// # 인자
/// - `value`: 값.
///
/// # 돌려줌
/// 위 낱말.
//==============================================================================
fn get_high_word(value: f64) -> i32 {
	(value.to_bits() >> 32) as u32 as i32
}

//==============================================================================
/// f64 의 아래 32 비트. (GET_LOW_WORD)
///
/// # 인자
/// - `value`: 값.
///
/// # 돌려줌
/// 아래 낱말.
//==============================================================================
fn get_low_word(value: f64) -> u32 {
	value.to_bits() as u32
}

//==============================================================================
/// 위 32 비트 바꾸기. (SET_HIGH_WORD)
///
/// # 인자
/// - `value`: 값.
/// - `high_word`: 새 위 낱말.
///
/// # 돌려줌
/// 바꾼 값.
//==============================================================================
fn set_high_word(value: f64, high_word: i32) -> f64 {
	f64::from_bits((value.to_bits() & 0x0000_0000_FFFF_FFFF) | ((high_word as u32 as u64) << 32))
}

//==============================================================================
/// 아래 32 비트 바꾸기. (SET_LOW_WORD)
///
/// # 인자
/// - `value`: 값.
/// - `low_word`: 새 아래 낱말.
///
/// # 돌려줌
/// 바꾼 값.
//==============================================================================
fn set_low_word(value: f64, low_word: u32) -> f64 {
	f64::from_bits((value.to_bits() & 0xFFFF_FFFF_0000_0000) | low_word as u64)
}

//==============================================================================
/// 2 의 거듭제곱 곱하기. (std::scalbn, 끝에 한 번만 반올림한다. pow 의 아주 작은 결과에만 쓴다)
///
/// # 인자
/// - `value`: 값.
/// - `exponent`: 2 의 지수.
///
/// # 돌려줌
/// value x 2^exponent.
//==============================================================================
fn scalbn(value: f64, exponent: i32) -> f64 {
	let two_1023 = f64::from_bits(0x7FE0_0000_0000_0000);
	let two_minus_969 = f64::from_bits(0x0360_0000_0000_0000);
	let mut result = value;
	let mut remaining = exponent;
	if remaining > 1023 {
		result *= two_1023;
		remaining -= 1023;
		if remaining > 1023 {
			result *= two_1023;
			remaining -= 1023;
			if remaining > 1023 {
				remaining = 1023;
			}
		}
	}
	else if remaining < -1022 {
		// 2^-1022 x 2^53 으로 두 번 반올림을 피한다.
		result *= two_minus_969;
		remaining += 1022 - 53;
		if remaining < -1022 {
			result *= two_minus_969;
			remaining += 1022 - 53;
			if remaining < -1022 {
				remaining = -1022;
			}
		}
	}
	result * f64::from_bits(((0x3FF + remaining) as u64) << 52)
}

//==============================================================================
/// 자연 로그. (fdlibm log)
///
/// # 인자
/// - `x`: 값.
///
/// # 돌려줌
/// ln(x). (0 이면 -무한, 음수면 NaN)
//==============================================================================
pub(super) fn log(x: f64) -> f64 {
	const LN2_HI: f64 = 6.93147180369123816490e-01;
	const LN2_LO: f64 = 1.90821492927058770002e-10;
	const TWO54: f64 = 1.80143985094819840000e+16;
	const LG1: f64 = 6.666666666666735130e-01;
	const LG2: f64 = 3.999999999940941908e-01;
	const LG3: f64 = 2.857142874366239149e-01;
	const LG4: f64 = 2.222219843214978396e-01;
	const LG5: f64 = 1.818357216161805012e-01;
	const LG6: f64 = 1.531383769920937332e-01;
	const LG7: f64 = 1.479819860511658591e-01;
	let mut x = x;
	let mut high_word = get_high_word(x);
	let low_word = get_low_word(x);
	let mut k: i32 = 0;
	if high_word < 0x00100000 {
		if ((high_word & 0x7FFFFFFF) as u32 | low_word) == 0 {
			return f64::NEG_INFINITY;
		}
		if high_word < 0 {
			return f64::NAN;
		}
		k -= 54;
		x *= TWO54;
		high_word = get_high_word(x);
	}
	if high_word >= 0x7FF00000 {
		return x + x;
	}
	k += (high_word >> 20) - 1023;
	high_word &= 0x000FFFFF;
	let mut i = (high_word + 0x95F64) & 0x100000;
	x = set_high_word(x, high_word | (i ^ 0x3FF00000));
	k += i >> 20;
	let f = x - 1.0;
	if (0x000FFFFF & (2 + high_word)) < 3 {
		if f == 0.0 {
			if k == 0 {
				return 0.0;
			}
			let dk = k as f64;
			return dk * LN2_HI + dk * LN2_LO;
		}
		let r = f * f * (0.5 - 0.33333333333333333 * f);
		if k == 0 {
			return f - r;
		}
		let dk = k as f64;
		return dk * LN2_HI - ((r - dk * LN2_LO) - f);
	}
	let s = f / (2.0 + f);
	let dk = k as f64;
	let z = s * s;
	i = high_word - 0x6147A;
	let w = z * z;
	let j = 0x6B851 - high_word;
	let t1 = w * (LG2 + w * (LG4 + w * LG6));
	let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
	i |= j;
	let r = t2 + t1;
	if i > 0 {
		let hfsq = 0.5 * f * f;
		if k == 0 {
			f - (hfsq - s * (hfsq + r))
		}
		else {
			dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
		}
	}
	else if k == 0 {
		f - s * (f - r)
	}
	else {
		dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
	}
}

//==============================================================================
/// 상용 로그. (fdlibm log10, n log10(2) 두 조각 + log(x) / ln(10))
///
/// # 인자
/// - `x`: 값.
///
/// # 돌려줌
/// log10(x). (0 이면 -무한, 음수면 NaN)
//==============================================================================
pub(super) fn log10(x: f64) -> f64 {
	const TWO54: f64 = 1.80143985094819840000e+16;
	const IVLN10: f64 = 4.34294481903251816668e-01;
	const LOG10_2HI: f64 = 3.01029995663611771306e-01;
	const LOG10_2LO: f64 = 3.69423907715893078616e-13;
	let mut x = x;
	let mut high_word = get_high_word(x);
	let mut low_word = get_low_word(x);
	let mut k: i32 = 0;
	if high_word < 0x00100000 {
		if ((high_word & 0x7FFFFFFF) as u32 | low_word) == 0 {
			return f64::NEG_INFINITY;
		}
		if high_word < 0 {
			return f64::NAN;
		}
		k -= 54;
		x *= TWO54;
		high_word = get_high_word(x);
		low_word = get_low_word(x);
	}
	if high_word >= 0x7FF00000 {
		return x + x;
	}
	if high_word == 0x3FF00000 && low_word == 0 {
		return 0.0;
	}
	k += (high_word >> 20) - 1023;
	let i = ((k as u32 & 0x80000000) >> 31) as i32;
	high_word = (high_word & 0x000FFFFF) | ((0x3FF - i) << 20);
	let y = (k + i) as f64;
	x = set_high_word(x, high_word);
	x = set_low_word(x, low_word);
	let z = y * LOG10_2LO + IVLN10 * log(x);
	z + y * LOG10_2HI
}

//==============================================================================
/// 거듭제곱. (fdlibm pow, log2(x) 를 두 조각으로 셈해 y 를 곱하고 2 의 거듭제곱으로 되돌린다)
///
/// # 인자
/// - `x`: 밑.
/// - `y`: 지수.
///
/// # 돌려줌
/// x^y.
//==============================================================================
pub(super) fn pow(x: f64, y: f64) -> f64 {
	const BP: [f64; 2] = [1.0, 1.5];
	const DP_H: [f64; 2] = [0.0, 5.84962487220764160156e-01];
	const DP_L: [f64; 2] = [0.0, 1.35003920212974897128e-08];
	const TWO53: f64 = 9007199254740992.0;
	const HUGE: f64 = 1.0e300;
	const TINY: f64 = 1.0e-300;
	const L1: f64 = 5.99999999999994648725e-01;
	const L2: f64 = 4.28571428578550184252e-01;
	const L3: f64 = 3.33333329818377432918e-01;
	const L4: f64 = 2.72728123808534006489e-01;
	const L5: f64 = 2.30660745775561754067e-01;
	const L6: f64 = 2.06975017800338417784e-01;
	const P1: f64 = 1.66666666666666019037e-01;
	const P2: f64 = -2.77777777770155933842e-03;
	const P3: f64 = 6.61375632143793436117e-05;
	const P4: f64 = -1.65339022054652515390e-06;
	const P5: f64 = 4.13813679705723846039e-08;
	const LG2: f64 = 6.93147180559945286227e-01;
	const LG2_H: f64 = 6.93147182464599609375e-01;
	const LG2_L: f64 = -1.90465429995776804525e-09;
	const OVT: f64 = 8.0085662595372944372e-0017;
	const CP: f64 = 9.61796693925975554329e-01;
	const CP_H: f64 = 9.61796700954437255859e-01;
	const CP_L: f64 = -7.02846165095275826516e-09;
	const IVLN2: f64 = 1.44269504088896338700e+00;
	const IVLN2_H: f64 = 1.44269502162933349609e+00;
	const IVLN2_L: f64 = 1.92596299112661746887e-08;
	let x_high_word = get_high_word(x);
	let x_low_word = get_low_word(x);
	let y_high_word = get_high_word(y);
	let y_low_word = get_low_word(y);
	let mut ix = x_high_word & 0x7fffffff;
	let iy = y_high_word & 0x7fffffff;
	// y 가 0 이면 1.
	if (iy as u32 | y_low_word) == 0 {
		return 1.0;
	}
	// NaN 이 끼면 x + y.
	if ix > 0x7ff00000 || (ix == 0x7ff00000 && x_low_word != 0) || iy > 0x7ff00000 || (iy == 0x7ff00000 && y_low_word != 0) {
		return x + y;
	}
	// x 가 음수일 때 y 가 정수인지. (0 아님, 1 홀수, 2 짝수)
	let mut y_is_integer = 0;
	if x_high_word < 0 {
		if iy >= 0x43400000 {
			y_is_integer = 2;
		}
		else if iy >= 0x3ff00000 {
			let k = (iy >> 20) - 0x3ff;
			if k > 20 {
				let j = (y_low_word >> (52 - k)) as i32;
				if j.wrapping_shl((52 - k) as u32) == y_low_word as i32 {
					y_is_integer = 2 - (j & 1);
				}
			}
			else if y_low_word == 0 {
				let j = iy >> (20 - k);
				if j.wrapping_shl((20 - k) as u32) == iy {
					y_is_integer = 2 - (j & 1);
				}
			}
		}
	}
	// y 의 특별한 값.
	if y_low_word == 0 {
		if iy == 0x7ff00000 {
			if ((ix - 0x3ff00000) as u32 | x_low_word) == 0 {
				return y - y;
			}
			else if ix >= 0x3ff00000 {
				return if y_high_word >= 0 { y } else { 0.0 };
			}
			else {
				return if y_high_word < 0 { -y } else { 0.0 };
			}
		}
		if iy == 0x3ff00000 {
			if y_high_word < 0 {
				return 1.0 / x;
			}
			else {
				return x;
			}
		}
		if y_high_word == 0x40000000 {
			return x * x;
		}
		if y_high_word == 0x3fe00000 && x_high_word >= 0 {
			return x.sqrt();
		}
	}
	let mut ax = x.abs();
	// x 의 특별한 값. (+-0, +-무한, +-1)
	if x_low_word == 0 && (ix == 0x7ff00000 || ix == 0 || ix == 0x3ff00000) {
		let mut z = ax;
		if y_high_word < 0 {
			z = 1.0 / z;
		}
		if x_high_word < 0 {
			if ((ix - 0x3ff00000) | y_is_integer) == 0 {
				z = f64::NAN;
			}
			else if y_is_integer == 1 {
				z = -z;
			}
		}
		return z;
	}
	let mut n = (x_high_word >> 31) + 1;
	// 음수의 정수 아닌 거듭제곱은 NaN.
	if (n | y_is_integer) == 0 {
		return f64::NAN;
	}
	let mut sign = 1.0;
	if (n | (y_is_integer - 1)) == 0 {
		sign = -1.0;
	}
	let t1;
	let t2;
	if iy > 0x41e00000 {
		// |y| 가 2^31 보다 크다.
		if iy > 0x43f00000 {
			if ix <= 0x3fefffff {
				return if y_high_word < 0 { HUGE * HUGE } else { TINY * TINY };
			}
			if ix >= 0x3ff00000 {
				return if y_high_word > 0 { HUGE * HUGE } else { TINY * TINY };
			}
		}
		if ix < 0x3fefffff {
			return if y_high_word < 0 { sign * HUGE * HUGE } else { sign * TINY * TINY };
		}
		if ix > 0x3ff00000 {
			return if y_high_word > 0 { sign * HUGE * HUGE } else { sign * TINY * TINY };
		}
		// |1 - x| 가 2^-20 이하라 log(x) 를 x - x^2/2 + x^3/3 - x^4/4 로 셈한다.
		let t = ax - 1.0;
		let w = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
		let u = IVLN2_H * t;
		let v = t * IVLN2_L - w * IVLN2;
		t1 = set_low_word(u + v, 0);
		t2 = v - (t1 - u);
	}
	else {
		n = 0;
		// 비정규 수.
		if ix < 0x00100000 {
			ax *= TWO53;
			n -= 53;
			ix = get_high_word(ax);
		}
		n += (ix >> 20) - 0x3ff;
		let j = ix & 0x000fffff;
		// 구간 고르기.
		ix = j | 0x3ff00000;
		let k: usize;
		if j <= 0x3988E {
			k = 0;
		}
		else if j < 0xBB67A {
			k = 1;
		}
		else {
			k = 0;
			n += 1;
			ix -= 0x00100000;
		}
		ax = set_high_word(ax, ix);
		// ss = s_h + s_l = (x - 1) / (x + 1) 이나 (x - 1.5) / (x + 1.5).
		let mut u = ax - BP[k];
		let mut v = 1.0 / (ax + BP[k]);
		let ss = u * v;
		let s_h = set_low_word(ss, 0);
		let mut t_h = set_high_word(0.0, ((ix >> 1) | 0x20000000) + 0x00080000 + ((k as i32) << 18));
		let mut t_l = ax - (t_h - BP[k]);
		let s_l = v * ((u - s_h * t_h) - s_h * t_l);
		// log(ax).
		let mut s2 = ss * ss;
		let mut r = s2 * s2 * (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
		r += s_l * (s_h + ss);
		s2 = s_h * s_h;
		t_h = set_low_word(3.0 + s2 + r, 0);
		t_l = r - ((t_h - 3.0) - s2);
		u = s_h * t_h;
		v = s_l * t_h + t_l * ss;
		let p_h = set_low_word(u + v, 0);
		let p_l = v - (p_h - u);
		let z_h = CP_H * p_h;
		let z_l = CP_L * p_h + p_l * CP + DP_L[k];
		// log2(ax) = n + dp_h + z_h + z_l.
		let t = n as f64;
		t1 = set_low_word(((z_h + z_l) + DP_H[k]) + t, 0);
		t2 = z_l - (((t1 - t) - DP_H[k]) - z_h);
	}
	// y = y1 + y2 로 나눠 (y1 + y2) x (t1 + t2).
	let y1 = set_low_word(y, 0);
	let p_l = (y - y1) * t1 + y * t2;
	let mut p_h = y1 * t1;
	let mut z = p_l + p_h;
	let mut j = get_high_word(z);
	let i = get_low_word(z) as i32;
	if j >= 0x40900000 {
		if ((j - 0x40900000) | i) != 0 {
			return sign * HUGE * HUGE;
		}
		else if p_l + OVT > z - p_h {
			return sign * HUGE * HUGE;
		}
	}
	else if (j & 0x7fffffff) >= 0x4090cc00 {
		if ((j as u32).wrapping_sub(0xc090cc00) | i as u32) != 0 {
			return sign * TINY * TINY;
		}
		else if p_l <= z - p_h {
			return sign * TINY * TINY;
		}
	}
	// 2^(p_h + p_l).
	let i = j & 0x7fffffff;
	let mut k = (i >> 20) - 0x3ff;
	n = 0;
	if i > 0x3fe00000 {
		n = j + (0x00100000 >> (k + 1));
		k = ((n & 0x7fffffff) >> 20) - 0x3ff;
		let t = set_high_word(0.0, n & !(0x000fffff >> k));
		n = ((n & 0x000fffff) | 0x00100000) >> (20 - k);
		if j < 0 {
			n = -n;
		}
		p_h -= t;
	}
	let t = set_low_word(p_l + p_h, 0);
	let u = t * LG2_H;
	let v = (p_l - (t - p_h)) * LG2 + t * LG2_L;
	z = u + v;
	let w = v - (z - u);
	let t = z * z;
	let t1 = z - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
	let r = (z * t1) / ((t1 - 2.0) - (w + z * w));
	z = 1.0 - (r - z);
	j = get_high_word(z);
	j = j.wrapping_add(((n as u32) << 20) as i32);
	if (j >> 20) <= 0 {
		z = scalbn(z, n);
	}
	else {
		let high_word = get_high_word(z);
		z = set_high_word(z, high_word.wrapping_add(((n as u32) << 20) as i32));
	}
	sign * z
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 정확한 값과 특별한 값. (log10(10^n) = n, 정수 거듭제곱, 0 과 1)
	//==============================================================================
	#[test]
	fn fdlibm_exact_values() {
		assert_eq!(log10(1000.0), 3.0);
		assert_eq!(log10(1.0), 0.0);
		assert_eq!(log10(0.0), f64::NEG_INFINITY);
		assert!(log10(-1.0).is_nan());
		assert_eq!(log(1.0), 0.0);
		assert_eq!(pow(10.0, 2.0), 100.0);
		assert_eq!(pow(2.0, 10.0), 1024.0);
		assert_eq!(pow(10.0, -3.0), 0.001);
		assert_eq!(pow(0.5, 0.0), 1.0);
		assert_eq!(pow(10.0, f64::NEG_INFINITY), 0.0);
		assert_eq!(pow(4.0, 0.5), 2.0);
		assert_eq!(pow(-2.0, 3.0), -8.0);
		assert!(pow(-2.0, 0.5).is_nan());
	}

	//==============================================================================
	/// 시스템 libm 과 끝자리 하나 안에서 같다. (데시벨 범위의 log10 과 10^(0.05 dB), s^0.75)
	//==============================================================================
	#[test]
	fn fdlibm_close_to_system_libm() {
		let ulp_distance = |left: f64, right: f64| (left.to_bits() as i64 - right.to_bits() as i64).abs();
		for index in 1..20000 {
			let value = index as f64 * 0.37 + 1.0e-6 * index as f64;
			assert!(ulp_distance(log10(value), value.log10()) <= 1, "log10 {}", value);
			let decibel = -300.0 + index as f64 * 0.0173;
			assert!(ulp_distance(pow(10.0, 0.05 * decibel), 10.0f64.powf(0.05 * decibel)) <= 1, "pow {}", decibel);
			let ratio = index as f64 / 20000.0;
			assert!(ulp_distance(pow(ratio, 0.75), ratio.powf(0.75)) <= 1, "pow {}", ratio);
		}
	}
}
