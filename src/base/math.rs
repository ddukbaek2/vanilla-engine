//==============================================================================
//! 수학 보조 함수. (원본 base/math.js)
//! - 원본 수는 f64 이고 러스트 판은 엔진의 다른 값처럼 f32 다. random_int 만 i32 다.
//! - 원본의 floor, ceil, sqrt, abs, sin, cos, tan, asin, acos, pow 를 같은 이름의 함수로 둔다. (쓰는 코드가 원본과 같게)
//!   원본 sin, cos 는 [-π, π] 로 접은 테일러 급수 10 항이라 f32 정밀도에서는 표준 함수와 같아 f32 메서드로 셈한다.
//!   round 는 반을 +무한대 쪽으로 올리는 점(-2.5 → -2)이 f32::round(-2.5 → -3)와 달라 원본 식 그대로 옮겼다.
//! - 무작위는 엔진의 random(xorshift64*)을 쓴다. (원본 Math.random 자리)
//! - 원본 shuffle 의 randomFunction 기본 인자는 shuffle(엔진 random), shuffle_with_random_function(넘긴 함수)으로 나눴다.
//==============================================================================
use std::cell::Cell;
use web_time::{SystemTime, UNIX_EPOCH};

pub const PI: f32 = std::f32::consts::PI;

/// 양의 무한대. (원본 PositiveInfinity)
pub const POSITIVE_INFINITY: f32 = f32::INFINITY;

/// 음의 무한대. (원본 NegativeInfinity)
pub const NEGATIVE_INFINITY: f32 = f32::NEG_INFINITY;

thread_local! {
	/// 난수 상태. (xorshift64*, 처음 쓸 때 시각으로 씨앗을 정한다)
	static RANDOM_STATE: Cell<u64> = const { Cell::new(0) };
}

//==============================================================================
/// 더 낮은 수. (원본처럼 left < right 가 아니면 right, NaN 이 끼면 f32::min 과 다르다)
//==============================================================================
pub fn min(left: f32, right: f32) -> f32 {
	if left < right {
		left
	}
	else {
		right
	}
}

//==============================================================================
/// 더 높은 수. (원본처럼 left > right 가 아니면 right)
//==============================================================================
pub fn max(left: f32, right: f32) -> f32 {
	if left > right {
		left
	}
	else {
		right
	}
}

//==============================================================================
/// 내림. (무한대는 그대로)
//==============================================================================
pub fn floor(value: f32) -> f32 {
	value.floor()
}

//==============================================================================
/// 올림. (무한대는 그대로)
//==============================================================================
pub fn ceil(value: f32) -> f32 {
	value.ceil()
}

//==============================================================================
/// 반올림. (원본 round: 반은 +무한대 쪽으로, -2.5 → -2, -0.5 → 0, 무한대는 그대로)
//==============================================================================
pub fn round(value: f32) -> f32 {
	// 무한대는 그대로. (value % 1 이 NaN 이 된다)
	if !value.is_finite() {
		return value;
	}
	let remainder = value % 1.0;
	if value >= 0.0 {
		if remainder >= 0.5 {
			value - remainder + 1.0
		}
		else {
			value - remainder
		}
	}
	else if remainder < -0.5 {
		value - remainder - 1.0
	}
	else {
		value - remainder
	}
}

//==============================================================================
/// 제곱근. (음수는 NaN)
//==============================================================================
pub fn sqrt(value: f32) -> f32 {
	value.sqrt()
}

//==============================================================================
/// 절대값. (원본처럼 0 보다 작으면 부호를 뒤집는다)
//==============================================================================
pub fn abs(value: f32) -> f32 {
	if value < 0.0 {
		return -value;
	}
	value
}

//==============================================================================
/// 사인.
//==============================================================================
pub fn sin(value: f32) -> f32 {
	value.sin()
}

//==============================================================================
/// 코사인.
//==============================================================================
pub fn cos(value: f32) -> f32 {
	value.cos()
}

//==============================================================================
/// 탄젠트. (원본처럼 사인을 코사인으로 나눈다)
//==============================================================================
pub fn tan(value: f32) -> f32 {
	sin(value) / cos(value)
}

//==============================================================================
/// 아크사인. (범위 밖은 NaN)
//==============================================================================
pub fn asin(value: f32) -> f32 {
	value.asin()
}

//==============================================================================
/// 아크코사인. (범위 밖은 NaN)
//==============================================================================
pub fn acos(value: f32) -> f32 {
	value.acos()
}

//==============================================================================
/// 거듭제곱. (지수가 분수나 음수여도 맞다)
//==============================================================================
pub fn pow(base: f32, exponent: f32) -> f32 {
	base.powf(exponent)
}

//==============================================================================
/// 범위 제한.
//==============================================================================
pub fn clamp(value: f32, minimum: f32, maximum: f32) -> f32 {
	if value < minimum {
		return minimum;
	}
	if value > maximum {
		return maximum;
	}
	value
}

//==============================================================================
/// 0 ~ 1 범위 제한.
//==============================================================================
pub fn clamp01(value: f32) -> f32 {
	clamp(value, 0.0, 1.0)
}

//==============================================================================
/// 도 → 라디안.
//==============================================================================
pub fn degree_to_radian(degree: f32) -> f32 {
	degree * (PI / 180.0)
}

//==============================================================================
/// 라디안 → 도.
//==============================================================================
pub fn radian_to_degree(radian: f32) -> f32 {
	radian / (PI / 180.0)
}

//==============================================================================
/// 선형 보간.
//==============================================================================
pub fn lerp(from: f32, to: f32, time: f32) -> f32 {
	from + (to - from) * time
}

//==============================================================================
/// 0 이상 1 미만 난수.
//==============================================================================
pub fn random() -> f32 {
	RANDOM_STATE.with(|random_state| {
		let mut state = random_state.get();
		if state == 0 {
			let nanoseconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos() as u64).unwrap_or(0x9E3779B97F4A7C15);
			state = nanoseconds | 1;
		}
		state ^= state >> 12;
		state ^= state << 25;
		state ^= state >> 27;
		random_state.set(state);
		let mixed = state.wrapping_mul(0x2545F4914F6CDD1D);
		((mixed >> 40) as f32) / ((1u64 << 24) as f32)
	})
}

//==============================================================================
/// 범위 난수. (minimum 이상 maximum 미만)
//==============================================================================
pub fn random_range(minimum: f32, maximum: f32) -> f32 {
	let random_value = random();
	minimum + (maximum - minimum) * random_value
}

//==============================================================================
/// 정수 범위 난수. (minimum 이상 maximum 이하)
//==============================================================================
pub fn random_int(minimum: i32, maximum: i32) -> i32 {
	let random_value = random() as f64;
	let offset = (random_value * (maximum as f64 - minimum as f64 + 1.0)).floor();
	(minimum as f64 + offset) as i32
}

//==============================================================================
/// 배열에서 무작위로 하나. (비었으면 None, 원본 undefined)
//==============================================================================
pub fn pick_random<T>(array: &[T]) -> Option<&T> {
	if array.is_empty() {
		return None;
	}
	let index = random_int(0, array.len() as i32 - 1) as usize;
	array.get(index)
}

//==============================================================================
/// 배열 제자리 섞기. (Fisher-Yates, 엔진 random)
//==============================================================================
pub fn shuffle<T>(array: &mut [T]) {
	shuffle_with_random_function(array, || random() as f64);
}

//==============================================================================
/// 넘긴 난수 함수(0 이상 1 미만)로 배열 제자리 섞기. (SeededRandom 의 next_value 등)
//==============================================================================
pub fn shuffle_with_random_function<T>(array: &mut [T], mut random_function: impl FnMut() -> f64) {
	let mut index = array.len();
	while index > 1 {
		index -= 1;
		let swap_index = ((random_function() * (index + 1) as f64).floor() as usize).min(index);
		array.swap(index, swap_index);
	}
}

//==============================================================================
/// 프레임률 독립 지수 감쇠 추종. (목표가 도중에 바뀌어도 튀지 않는다, rate 가 클수록 빨리 붙는다)
/// - current + (target - current) * (1 - e^(-rate * time_delta))
//==============================================================================
pub fn approach(current_value: f32, target_value: f32, rate: f32, time_delta: f32) -> f32 {
	let blend_ratio = 1.0 - (-rate * time_delta).exp();
	current_value + (target_value - current_value) * blend_ratio
}

//==============================================================================
/// 값을 일정 속도로 목표까지 옮기기. (지나치지 않는다)
//==============================================================================
pub fn move_towards(current_value: f32, target_value: f32, maximum_delta: f32) -> f32 {
	let difference = target_value - current_value;
	if difference.abs() <= maximum_delta {
		return target_value;
	}
	if difference > 0.0 {
		current_value + maximum_delta
	}
	else {
		current_value - maximum_delta
	}
}

//==============================================================================
/// 이징: 바깥으로 감속하는 2차 곡선.
//==============================================================================
pub fn ease_out_quad(time: f32) -> f32 {
	1.0 - (1.0 - time) * (1.0 - time)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// min, max, round 가 원본과 같은지. (NaN 이 끼면 둘째 값, 반은 +무한대 쪽, 무한대는 그대로)
	//==============================================================================
	#[test]
	fn minimum_maximum_and_round() {
		assert_eq!(min(2.0, 1.0), 1.0);
		assert_eq!(max(2.0, 1.0), 2.0);
		assert!(min(1.0, f32::NAN).is_nan());
		assert_eq!(min(f32::NAN, 1.0), 1.0);
		assert_eq!(max(f32::NAN, 1.0), 1.0);
		let case_list: [(f32, f32); 9] = [(2.5, 3.0), (-2.5, -2.0), (-2.6, -3.0), (-2.4, -2.0), (0.5, 1.0), (-0.5, 0.0), (1.5, 2.0), (-1.5, -1.0), (7.0, 7.0)];
		for (value, expected_value) in case_list {
			assert_eq!(round(value), expected_value, "{}", value);
		}
		assert_eq!(round(f32::INFINITY), f32::INFINITY);
		assert_eq!(round(f32::NEG_INFINITY), f32::NEG_INFINITY);
		assert!(round(f32::NAN).is_nan());
		assert_eq!(POSITIVE_INFINITY, f32::INFINITY);
		assert_eq!(NEGATIVE_INFINITY, f32::NEG_INFINITY);
	}

	//==============================================================================
	/// 정수 난수는 양 끝을 포함하고, 뽑기는 빈 배열이면 None, 섞기는 원소를 잃지 않는다.
	//==============================================================================
	#[test]
	fn random_int_pick_and_shuffle() {
		let mut seen = [false; 4];
		for _ in 0..2000 {
			let value = random_int(-1, 2);
			assert!((-1..=2).contains(&value));
			seen[(value + 1) as usize] = true;
		}
		assert!(seen.iter().all(|is_seen| *is_seen));
		assert_eq!(random_int(5, 5), 5);

		let empty_list: [i32; 0] = [];
		assert!(pick_random(&empty_list).is_none());
		let one_list = [42];
		assert_eq!(pick_random(&one_list), Some(&42));

		let mut number_list: Vec<i32> = (0..50).collect();
		shuffle(&mut number_list);
		let mut sorted_list = number_list.clone();
		sorted_list.sort();
		assert_eq!(sorted_list, (0..50).collect::<Vec<i32>>());
	}

	//==============================================================================
	/// 넘긴 난수 함수로 섞기. (원본 shuffle 에 randomFunction 을 넘긴 것과 같은 차례)
	/// - 원본: [1, 2, 3, 4] 를 늘 0 을 돌려주는 함수로 섞으면 [2, 3, 4, 1], 늘 0.999 면 그대로.
	//==============================================================================
	#[test]
	fn shuffle_with_given_random_function() {
		let mut number_list = [1, 2, 3, 4];
		shuffle_with_random_function(&mut number_list, || 0.0);
		assert_eq!(number_list, [2, 3, 4, 1]);
		let mut number_list = [1, 2, 3, 4];
		shuffle_with_random_function(&mut number_list, || 0.999);
		assert_eq!(number_list, [1, 2, 3, 4]);
		let mut empty_list: [i32; 0] = [];
		shuffle_with_random_function(&mut empty_list, || 0.5);
	}

	//==============================================================================
	/// 지수 감쇠 추종과 일정 속도 옮기기.
	//==============================================================================
	#[test]
	fn approach_and_move_towards() {
		assert_eq!(approach(0.0, 10.0, 5.0, 0.0), 0.0);
		let approached_value = approach(0.0, 10.0, 1.0, 1.0);
		assert!((approached_value - 10.0 * (1.0 - (-1.0f32).exp())).abs() < 1e-5);
		assert_eq!(move_towards(0.0, 10.0, 3.0), 3.0);
		assert_eq!(move_towards(0.0, -10.0, 3.0), -3.0);
		assert_eq!(move_towards(9.0, 10.0, 3.0), 10.0);
		assert_eq!(move_towards(10.0, 10.0, 0.0), 10.0);
	}

	//==============================================================================
	/// 원본 이름의 표준 함수들. (무한대 보존, abs 의 -0, tan = sin / cos)
	//==============================================================================
	#[test]
	fn standard_functions_follow_original() {
		assert_eq!(floor(-1.5), -2.0);
		assert_eq!(ceil(-1.5), -1.0);
		assert_eq!(floor(f32::INFINITY), f32::INFINITY);
		assert_eq!(ceil(f32::NEG_INFINITY), f32::NEG_INFINITY);
		assert_eq!(sqrt(1e6), 1000.0);
		assert!(sqrt(-1.0).is_nan());
		assert_eq!(abs(-3.0), 3.0);
		assert_eq!(abs(2.0), 2.0);
		assert!((sin(PI / 2.0) - 1.0).abs() < 1e-6);
		assert!((cos(PI) + 1.0).abs() < 1e-6);
		assert!((tan(PI / 4.0) - 1.0).abs() < 1e-6);
		assert!((asin(1.0) - PI / 2.0).abs() < 1e-6);
		assert!((acos(-1.0) - PI).abs() < 1e-6);
		assert!(asin(2.0).is_nan());
		assert_eq!(pow(2.0, 10.0), 1024.0);
		assert_eq!(pow(4.0, 0.5), 2.0);
		assert_eq!(pow(2.0, -1.0), 0.5);
		assert!((radian_to_degree(PI) - 180.0).abs() < 1e-4);
	}
}
