//==============================================================================
//! 제약 관계 연산자. (원본 ui/autolayout/layoutrelation.js 의 LayoutRelation)
//! - LayoutConstraint 가 식 == 0, <= 0, >= 0 중 어느 쪽인지 가린다.
//! - 원본은 문자열 값("==", "<=", ">=")을 담은 객체다. 러스트 판은 열거형이다.
//!
//! # 예제
//! 비교 함수가 고른 관계와 직접 만든 제약
//! ```rust
//! use vanilla::{LayoutConstraint, LayoutExpression, LayoutRelation, LayoutStrength, LayoutVariable};
//!
//! let x = LayoutVariable::new("x");
//! let at_most = LayoutExpression::from_variable(&x).less_than_or_equal_to(100);
//! assert_eq!(at_most.get_relation(), LayoutRelation::LessThanOrEqual);
//! assert_eq!(at_most.get_expression().get_constant(), -100.0);
//!
//! let expression = LayoutExpression::from_variable(&x).subtract(20);
//! let at_least = LayoutConstraint::new(expression, LayoutRelation::GreaterThanOrEqual, LayoutStrength::STRONG);
//! assert_eq!(at_least.get_relation(), LayoutRelation::GreaterThanOrEqual);
//! ```
//==============================================================================

//==============================================================================
/// 제약 관계 연산자.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutRelation {
	/// 식 == 0
	Equal,
	/// 식 <= 0
	LessThanOrEqual,
	/// 식 >= 0
	GreaterThanOrEqual,
}
