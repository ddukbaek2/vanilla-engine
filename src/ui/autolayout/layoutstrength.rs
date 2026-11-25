//==============================================================================
// 레이아웃 제약 강도. (원본 ui/autolayout/layoutstrength.js 의 LayoutStrength)
// - Cassowary, Kiwi 의 표준 강도 체계. 제약이 부딪힐 때 어느 쪽을 먼저 지킬지 정한다.
// - REQUIRED 는 반드시 지켜야 하고, 솔버가 못 풀면 오류를 돌려준다.
// - STRONG, MEDIUM, WEAK 는 되도록 지키되 부딪히면 강도 차례로 양보한다.
// - create_strength(a, b, c, weight) 로 강도를 합성한다.
// - 원본의 정적 필드는 연관 상수, 정적 함수는 연관 함수다. 생략할 수 있던 weight 는 Option 으로 받는다.
//==============================================================================

//==============================================================================
// 레이아웃 제약 강도.
//==============================================================================
pub struct LayoutStrength;

impl LayoutStrength {
	pub const REQUIRED: f64 = 1001001000.0;
	pub const STRONG: f64 = 1000000.0;
	pub const MEDIUM: f64 = 1000.0;
	pub const WEAK: f64 = 1.0;

	//==============================================================================
	// 강도 합성.
	// - 세 자리 성분 (a, b, c) 에 weight(없으면 1)를 곱한 뒤 각각 0 ~ 1000 으로 자른다.
	// - 결과는 0 ~ REQUIRED 범위다.
	//==============================================================================
	pub fn create_strength(a: f64, b: f64, c: f64, weight: Option<f64>) -> f64 {
		let final_weight = weight.unwrap_or(1.0);
		let component_a = (a * final_weight).clamp(0.0, 1000.0) * 1000000.0;
		let component_b = (b * final_weight).clamp(0.0, 1000.0) * 1000.0;
		let component_c = (c * final_weight).clamp(0.0, 1000.0);
		component_a + component_b + component_c
	}

	//==============================================================================
	// 강도 자르기. (0 ~ REQUIRED)
	//==============================================================================
	pub fn clip_strength(strength: f64) -> f64 {
		strength.clamp(0.0, LayoutStrength::REQUIRED)
	}

	//==============================================================================
	// UIKit 의 우선순위(1 ~ 1000)를 강도로 바꾼다.
	// - 1000 이상은 REQUIRED, 1 ~ 999 는 medium 단의 가중치(750 이면 750000), 0 이하는 0 이다.
	//==============================================================================
	pub fn from_priority(priority: f64) -> f64 {
		if priority >= 1000.0 {
			return LayoutStrength::REQUIRED;
		}
		if priority <= 0.0 {
			return 0.0;
		}
		LayoutStrength::create_strength(0.0, priority, 0.0, None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ui::autolayout::layoutpriority::LayoutPriority;

	//==============================================================================
	// 합성, 자르기, 우선순위 바꾸기가 원본 값과 같다.
	//==============================================================================
	#[test]
	fn strength_matches_original_values() {
		assert_eq!(LayoutStrength::create_strength(1.0, 0.0, 0.0, None), LayoutStrength::STRONG);
		assert_eq!(LayoutStrength::create_strength(0.0, 1.0, 0.0, None), LayoutStrength::MEDIUM);
		assert_eq!(LayoutStrength::create_strength(0.0, 0.0, 1.0, None), LayoutStrength::WEAK);
		assert_eq!(LayoutStrength::create_strength(1000.0, 1000.0, 1000.0, None), LayoutStrength::REQUIRED);
		assert_eq!(LayoutStrength::create_strength(2000.0, -5.0, 3.0, Some(2.0)), 1000000000.0 + 6.0);
		assert_eq!(LayoutStrength::clip_strength(-1.0), 0.0);
		assert_eq!(LayoutStrength::clip_strength(1e12), LayoutStrength::REQUIRED);
		assert_eq!(LayoutStrength::from_priority(LayoutPriority::REQUIRED), LayoutStrength::REQUIRED);
		assert_eq!(LayoutStrength::from_priority(LayoutPriority::DEFAULT_HIGH), 750000.0);
		assert_eq!(LayoutStrength::from_priority(LayoutPriority::DEFAULT_LOW), 250000.0);
		assert_eq!(LayoutStrength::from_priority(0.0), 0.0);
	}
}
