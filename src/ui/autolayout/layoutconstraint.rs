//==============================================================================
//! 레이아웃 제약. (원본 ui/autolayout/layoutconstraint.js 의 LayoutConstraint)
//! - 꼴: 식 { ==, <=, >= } 0
//! - 보통 LayoutExpression 의 equal_to, less_than_or_equal_to, greater_than_or_equal_to 가 만든다.
//! - 강도는 부딪힐 때의 우선순위다. 만들 때 0 ~ REQUIRED 로 자른다.
//! - 원본과 다른 것:
//!   - 원본은 객체 동일성으로 솔버에 등록된 제약을 가린다. 러스트 판은 Rc 를 품은 값싼 Clone 핸들이라
//!     복제한 핸들은 같은 제약이고, 같음과 해시는 핸들이 가리키는 제약으로 가린다. (식이 같아도 따로 만들면 다른 제약)
//!   - 생략할 수 있던 strength(기본 REQUIRED)는 꼭 받는다.
//==============================================================================
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::ui::autolayout::layoutexpression::LayoutExpression;
use crate::ui::autolayout::layoutrelation::LayoutRelation;
use crate::ui::autolayout::layoutstrength::LayoutStrength;

//==============================================================================
/// 핸들이 같이 가리키는 제약 본체.
//==============================================================================
struct LayoutConstraintData {
	expression: LayoutExpression,
	relation: LayoutRelation,
	strength: f64,
}

//==============================================================================
/// 레이아웃 제약. (복제해도 같은 제약을 가리킨다)
///
/// # 예제
/// 식으로 제약을 만들어 솔버에 걸기
/// ```rust
/// use vanilla::{LayoutExpression, LayoutRelation, LayoutSolver, LayoutStrength, LayoutVariable};
///
/// let solver = LayoutSolver::new();
/// let width = LayoutVariable::new("width");
/// let minimum_width = LayoutExpression::from_variable(&width).greater_than_or_equal_to(120);
/// assert_eq!(minimum_width.get_relation(), LayoutRelation::GreaterThanOrEqual);
/// assert_eq!(minimum_width.get_strength(), LayoutStrength::REQUIRED);
/// solver.add_constraint(&minimum_width).unwrap();
/// solver.add_constraint(&LayoutExpression::from_variable(&width).equal_to(80).with_strength(LayoutStrength::STRONG)).unwrap();
/// solver.update_variables();
/// assert!((width.get_value() - 120.0).abs() < 1e-6);
/// assert!(solver.has_constraint(&minimum_width.clone()));
/// ```
//==============================================================================
#[derive(Clone)]
pub struct LayoutConstraint {
	data: Rc<LayoutConstraintData>,
}

impl LayoutConstraint {
	//==============================================================================
	/// 생성. (강도는 0 ~ REQUIRED 로 자른다)
	///
	/// # 인자
	/// - `expression`: 제약 식. (식 관계 0 꼴의 왼쪽)
	/// - `relation`: 식과 0 의 관계.
	/// - `strength`: 강도. (0 ~ LayoutStrength::REQUIRED 로 자른다)
	///
	/// # 돌려줌
	/// 새 제약 핸들.
	///
	/// # 예제
	/// width - 100 >= 0 을 strong 강도로
	/// ```rust
	/// use vanilla::{LayoutConstraint, LayoutExpression, LayoutRelation, LayoutStrength, LayoutVariable};
	///
	/// let width = LayoutVariable::new("width");
	/// let expression = LayoutExpression::from_variable(&width).subtract(100);
	/// let constraint = LayoutConstraint::new(expression, LayoutRelation::GreaterThanOrEqual, LayoutStrength::STRONG);
	/// assert_eq!(constraint.get_strength(), LayoutStrength::STRONG);
	/// assert_eq!(constraint.get_expression().get_constant(), -100.0);
	///
	/// let clipped = LayoutConstraint::new(LayoutExpression::from_variable(&width), LayoutRelation::Equal, 5e9);
	/// assert_eq!(clipped.get_strength(), LayoutStrength::REQUIRED);
	/// ```
	//==============================================================================
	pub fn new(expression: LayoutExpression, relation: LayoutRelation, strength: f64) -> LayoutConstraint {
		LayoutConstraint {
			data: Rc::new(LayoutConstraintData {
				expression,
				relation,
				strength: LayoutStrength::clip_strength(strength),
			}),
		}
	}

	//==============================================================================
	/// 식 반환.
	///
	/// # 돌려줌
	/// 제약 식의 참조. (비교 함수로 만들었으면 self - 상대 식)
	//==============================================================================
	pub fn get_expression(&self) -> &LayoutExpression {
		&self.data.expression
	}

	//==============================================================================
	/// 관계 반환.
	///
	/// # 돌려줌
	/// 식과 0 의 관계.
	//==============================================================================
	pub fn get_relation(&self) -> LayoutRelation {
		self.data.relation
	}

	//==============================================================================
	/// 강도 반환.
	///
	/// # 돌려줌
	/// 강도. (만들 때 0 ~ REQUIRED 로 자른 값)
	//==============================================================================
	pub fn get_strength(&self) -> f64 {
		self.data.strength
	}

	//==============================================================================
	/// 강도만 다른 새 제약 반환. (우선순위만 바꿔 다시 등록할 때)
	///
	/// # 인자
	/// - `new_strength`: 새 강도. (0 ~ LayoutStrength::REQUIRED 로 자른다)
	///
	/// # 돌려줌
	/// 식과 관계가 같고 강도만 다른 새 제약. (따로 만든 제약이라 self 와 같지 않다)
	///
	/// # 예제
	/// 약하게 건 제약을 강하게 바꿔 다시 걸기
	/// ```rust
	/// use vanilla::{LayoutExpression, LayoutSolver, LayoutStrength, LayoutVariable};
	///
	/// let solver = LayoutSolver::new();
	/// let x = LayoutVariable::new("x");
	/// let preferred = LayoutExpression::from_variable(&x).equal_to(100).with_strength(LayoutStrength::WEAK);
	/// solver.add_constraint(&preferred).unwrap();
	/// let stronger = preferred.with_strength(LayoutStrength::STRONG);
	/// assert_ne!(stronger, preferred);
	/// solver.remove_constraint(&preferred).unwrap();
	/// solver.add_constraint(&stronger).unwrap();
	/// assert!(solver.has_constraint(&stronger));
	/// assert!(!solver.has_constraint(&preferred));
	/// ```
	//==============================================================================
	pub fn with_strength(&self, new_strength: f64) -> LayoutConstraint {
		let expression = self.get_expression().clone();
		let relation = self.get_relation();
		LayoutConstraint::new(expression, relation, new_strength)
	}
}

impl PartialEq for LayoutConstraint {
	//==============================================================================
	/// 같은 제약을 가리키는지.
	//==============================================================================
	fn eq(&self, other: &LayoutConstraint) -> bool {
		Rc::ptr_eq(&self.data, &other.data)
	}
}

impl Eq for LayoutConstraint {
}

impl Hash for LayoutConstraint {
	//==============================================================================
	/// 가리키는 제약의 주소로 해시한다.
	//==============================================================================
	fn hash<H: Hasher>(&self, state: &mut H) {
		Rc::as_ptr(&self.data).hash(state);
	}
}

impl fmt::Debug for LayoutConstraint {
	//==============================================================================
	/// 식, 관계, 강도를 보인다.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.debug_struct("LayoutConstraint").field("expression", &self.data.expression).field("relation", &self.data.relation).field("strength", &self.data.strength).finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ui::autolayout::layoutvariable::LayoutVariable;

	//==============================================================================
	/// with_strength 는 식과 관계가 같은 다른 제약을 만들고 강도를 자른다. 복제한 핸들은 같은 제약이다.
	//==============================================================================
	#[test]
	fn with_strength_makes_distinct_constraint() {
		let x = LayoutVariable::new("x");
		let constraint = LayoutExpression::from_variable(&x).less_than_or_equal_to(100);
		let weaker = constraint.with_strength(LayoutStrength::WEAK);
		assert_ne!(constraint, weaker);
		assert_eq!(constraint, constraint.clone());
		assert_eq!(weaker.get_relation(), LayoutRelation::LessThanOrEqual);
		assert_eq!(weaker.get_strength(), LayoutStrength::WEAK);
		assert_eq!(weaker.get_expression().get_constant(), -100.0);
		assert_eq!(constraint.with_strength(-3.0).get_strength(), 0.0);
		assert_eq!(constraint.with_strength(2e9).get_strength(), LayoutStrength::REQUIRED);
	}
}
