//==============================================================================
//! 레이아웃 우선순위. (원본 ui/autolayout/layoutpriority.js 의 LayoutPriority, UIKit 의 UILayoutPriority 와 같은 생각)
//! - REQUIRED (1000): 반드시 지켜야 하는 우선순위. LayoutStrength::REQUIRED 로 바뀐다.
//! - DEFAULT_HIGH (750): 기본 압축 저항(compression resistance) 우선순위.
//! - DEFAULT_LOW (250): 기본 hug 우선순위.
//! - FITTING_SIZE_LEVEL (50): 맞춤 크기를 셀 때 권장 크기 우선순위.
//! - 1 ~ 999 의 값은 LayoutStrength::from_priority 로 medium 단의 강도가 된다.
//! - 원본의 정적 필드는 연관 상수다. 값은 f64 다.
//==============================================================================

//==============================================================================
/// 레이아웃 우선순위.
/// - 값을 품지 않는 이름 자리다. 연관 상수만 쓰고, LayoutStrength::from_priority 로 강도로 바꿔 제약에 준다.
///
/// # 예제
/// 우선순위가 높은 제약이 이긴다
/// ```rust
/// use vanilla::{LayoutExpression, LayoutPriority, LayoutSolver, LayoutStrength, LayoutVariable};
///
/// let solver = LayoutSolver::new();
/// let width = LayoutVariable::new("width");
/// let low_strength = LayoutStrength::from_priority(LayoutPriority::DEFAULT_LOW);
/// let high_strength = LayoutStrength::from_priority(LayoutPriority::DEFAULT_HIGH);
/// solver.add_constraint(&LayoutExpression::from_variable(&width).equal_to(100).with_strength(low_strength)).unwrap();
/// solver.add_constraint(&LayoutExpression::from_variable(&width).equal_to(200).with_strength(high_strength)).unwrap();
/// solver.update_variables();
/// assert!((width.get_value() - 200.0).abs() < 1e-6);
/// ```
//==============================================================================
pub struct LayoutPriority;

impl LayoutPriority {
	/// 반드시 지켜야 하는 우선순위. (1000, LayoutStrength::REQUIRED 로 바뀐다)
	pub const REQUIRED: f64 = 1000.0;
	/// 기본 압축 저항(compression resistance) 우선순위. (750)
	pub const DEFAULT_HIGH: f64 = 750.0;
	/// 기본 hug 우선순위. (250)
	pub const DEFAULT_LOW: f64 = 250.0;
	/// 맞춤 크기를 셀 때 권장 크기 우선순위. (50)
	pub const FITTING_SIZE_LEVEL: f64 = 50.0;
}
