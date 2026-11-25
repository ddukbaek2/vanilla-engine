//==============================================================================
//! 레이아웃 제약 강도. (원본 ui/autolayout/layoutstrength.js 의 LayoutStrength)
//! - Cassowary, Kiwi 의 표준 강도 체계. 제약이 부딪힐 때 어느 쪽을 먼저 지킬지 정한다.
//! - REQUIRED 는 반드시 지켜야 하고, 솔버가 못 풀면 오류를 돌려준다.
//! - STRONG, MEDIUM, WEAK 는 되도록 지키되 부딪히면 강도 차례로 양보한다.
//! - create_strength(a, b, c, weight) 로 강도를 합성한다.
//! - 원본의 정적 필드는 연관 상수, 정적 함수는 연관 함수다. 생략할 수 있던 weight 는 Option 으로 받는다.
//==============================================================================

//==============================================================================
/// 레이아웃 제약 강도.
/// - 값을 품지 않는 이름 자리다. 연관 상수와 연관 함수만 쓴다. 강도는 f64 다.
///
/// # 예제
/// 강한 제약이 약한 제약을 이긴다
/// ```rust
/// use vanilla::{LayoutExpression, LayoutSolver, LayoutStrength, LayoutVariable};
///
/// let solver = LayoutSolver::new();
/// let x = LayoutVariable::new("x");
/// solver.add_constraint(&LayoutExpression::from_variable(&x).equal_to(10).with_strength(LayoutStrength::WEAK)).unwrap();
/// solver.add_constraint(&LayoutExpression::from_variable(&x).equal_to(50).with_strength(LayoutStrength::STRONG)).unwrap();
/// solver.update_variables();
/// assert!((x.get_value() - 50.0).abs() < 1e-6);
/// ```
//==============================================================================
pub struct LayoutStrength;

impl LayoutStrength {
	/// 반드시 지켜야 하는 강도. (create_strength(1000, 1000, 1000) 과 같은 1001001000)
	pub const REQUIRED: f64 = 1001001000.0;
	/// 강함. (create_strength(1, 0, 0) 과 같은 1000000)
	pub const STRONG: f64 = 1000000.0;
	/// 보통. (create_strength(0, 1, 0) 과 같은 1000)
	pub const MEDIUM: f64 = 1000.0;
	/// 약함. (create_strength(0, 0, 1) 과 같은 1)
	pub const WEAK: f64 = 1.0;

	//==============================================================================
	/// 강도 합성.
	/// - 세 자리 성분 (a, b, c) 에 weight(없으면 1)를 곱한 뒤 각각 0 ~ 1000 으로 자른다.
	/// - 결과는 0 ~ REQUIRED 범위다.
	///
	/// # 인자
	/// - `a`: strong 자리 성분. (weight 를 곱해 0 ~ 1000 으로 자른 뒤 1000000 을 곱한다)
	/// - `b`: medium 자리 성분. (weight 를 곱해 0 ~ 1000 으로 자른 뒤 1000 을 곱한다)
	/// - `c`: weak 자리 성분. (weight 를 곱해 0 ~ 1000 으로 자른다)
	/// - `weight`: 세 성분에 곱할 가중치. None 이면 1.
	///
	/// # 돌려줌
	/// 세 자리를 더한 강도. (0 ~ REQUIRED)
	///
	/// # 예제
	/// 자리마다 자르고 더하기
	/// ```rust
	/// use vanilla::LayoutStrength;
	///
	/// assert_eq!(LayoutStrength::create_strength(1.0, 0.0, 0.0, None), LayoutStrength::STRONG);
	/// assert_eq!(LayoutStrength::create_strength(0.0, 2.0, 0.0, Some(3.0)), 6000.0);
	/// assert_eq!(LayoutStrength::create_strength(2000.0, 0.0, 5.0, None), 1000000000.0 + 5.0);
	/// ```
	//==============================================================================
	pub fn create_strength(a: f64, b: f64, c: f64, weight: Option<f64>) -> f64 {
		let final_weight = weight.unwrap_or(1.0);
		let component_a = (a * final_weight).clamp(0.0, 1000.0) * 1000000.0;
		let component_b = (b * final_weight).clamp(0.0, 1000.0) * 1000.0;
		let component_c = (c * final_weight).clamp(0.0, 1000.0);
		component_a + component_b + component_c
	}

	//==============================================================================
	/// 강도 자르기. (0 ~ REQUIRED)
	///
	/// # 인자
	/// - `strength`: 자를 강도.
	///
	/// # 돌려줌
	/// 0 ~ REQUIRED 로 자른 강도.
	//==============================================================================
	pub fn clip_strength(strength: f64) -> f64 {
		strength.clamp(0.0, LayoutStrength::REQUIRED)
	}

	//==============================================================================
	/// UIKit 의 우선순위(1 ~ 1000)를 강도로 바꾼다.
	/// - 1000 이상은 REQUIRED, 1 ~ 999 는 medium 단의 가중치(750 이면 750000), 0 이하는 0 이다.
	///
	/// # 인자
	/// - `priority`: UIKit 꼴 우선순위. (LayoutPriority 의 값, 1 ~ 1000)
	///
	/// # 돌려줌
	/// 강도. (1000 이상이면 REQUIRED, 0 이하면 0, 그 사이는 priority * 1000)
	///
	/// # 예제
	/// 우선순위를 강도로 바꾸기
	/// ```rust
	/// use vanilla::{LayoutPriority, LayoutStrength};
	///
	/// assert_eq!(LayoutStrength::from_priority(LayoutPriority::REQUIRED), LayoutStrength::REQUIRED);
	/// assert_eq!(LayoutStrength::from_priority(LayoutPriority::DEFAULT_HIGH), 750000.0);
	/// assert_eq!(LayoutStrength::from_priority(-10.0), 0.0);
	/// ```
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
	/// 합성, 자르기, 우선순위 바꾸기가 원본 값과 같다.
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
