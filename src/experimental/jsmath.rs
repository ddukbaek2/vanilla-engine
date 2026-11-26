//==============================================================================
//! 원본 JS Math.max, Math.min 과 같은 뜻의 실수 고르기. (초안 물리, 공간 나누기가 원본과 비트까지 같게 쓴다)
//! - f64::max, min 은 NaN 을 버리고 (-0, 0) 의 부호를 약속하지 않는다. 원본 Math.max, min 은 NaN 을 퍼뜨리고
//!   max(-0, 0) 은 +0, min(-0, 0) 은 -0 이다. (ECMAScript Math.max, Math.min)
//==============================================================================

//==============================================================================
/// 큰 쪽. (원본 Math.max(left, right): 한쪽이라도 NaN 이면 NaN, -0 과 +0 이면 +0)
///
/// # 인자
/// - `left`: 첫 값.
/// - `right`: 둘째 값.
///
/// # 돌려줌
/// 큰 값.
//==============================================================================
#[inline]
pub(crate) fn js_max(left: f64, right: f64) -> f64 {
	if left.is_nan() || right.is_nan() {
		return f64::NAN;
	}
	if left == right {
		// 같으면 둘 다 0 일 때만 부호가 갈린다. (+0 이 있으면 +0)
		if left.is_sign_positive() {
			return left;
		}
		return right;
	}
	if left > right {
		left
	}
	else {
		right
	}
}

//==============================================================================
/// 작은 쪽. (원본 Math.min(left, right): 한쪽이라도 NaN 이면 NaN, -0 과 +0 이면 -0)
///
/// # 인자
/// - `left`: 첫 값.
/// - `right`: 둘째 값.
///
/// # 돌려줌
/// 작은 값.
//==============================================================================
#[inline]
pub(crate) fn js_min(left: f64, right: f64) -> f64 {
	if left.is_nan() || right.is_nan() {
		return f64::NAN;
	}
	if left == right {
		// 같으면 둘 다 0 일 때만 부호가 갈린다. (-0 이 있으면 -0)
		if left.is_sign_negative() {
			return left;
		}
		return right;
	}
	if left < right {
		left
	}
	else {
		right
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// NaN 을 퍼뜨리고 0 의 부호가 원본 Math.max, min 과 같다.
	//==============================================================================
	#[test]
	fn follows_javascript_math() {
		assert!(js_max(f64::NAN, 1.0).is_nan() && js_max(1.0, f64::NAN).is_nan());
		assert!(js_min(f64::NAN, 1.0).is_nan() && js_min(1.0, f64::NAN).is_nan());
		assert!(js_max(-0.0, 0.0).is_sign_positive() && js_max(0.0, -0.0).is_sign_positive());
		assert!(js_min(-0.0, 0.0).is_sign_negative() && js_min(0.0, -0.0).is_sign_negative());
		assert!(js_max(-0.0, -0.0).is_sign_negative() && js_min(0.0, 0.0).is_sign_positive());
		assert_eq!(js_max(2.0, -3.0), 2.0);
		assert_eq!(js_min(2.0, -3.0), -3.0);
		assert_eq!(js_max(f64::NEG_INFINITY, 1.0), 1.0);
	}
}
