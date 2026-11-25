//==============================================================================
//! 선형 식. (원본 ui/autolayout/layoutexpression.js 의 LayoutExpression, 항 목록 + 상수항)
//! - 산술 함수는 모두 새 식을 돌려준다. (바뀌지 않는 값)
//! - equal_to, less_than_or_equal_to, greater_than_or_equal_to 는 LayoutConstraint 를 돌려준다. (required 강도)
//! - 인자는 숫자(f64, f32, i32), LayoutVariable, LayoutTerm, LayoutExpression 을 받는다. (impl `Into<LayoutExpression>`)
//! - 원본과 다른 것:
//!   - 원본 toExpression 의 실행 중 타입 검사는 From 구현으로 옮겼다. 받지 않는 타입은 컴파일이 막는다.
//!   - multiply, divide 는 원본도 숫자만 받으므로 f64 만 받는다. (원본의 '스칼라만 지원' 예외는 생기지 않는다)
//!   - 0 으로 나누기는 프로그램 실수라 원본과 같은 문구로 panic 한다.
//!   - 원본은 LayoutConstraint 와 서로 import 하는 것을 피하려고 늦게 읽는 꾀를 쓴다. 러스트 판은 그냥 use 한다.
//==============================================================================
use crate::ui::autolayout::layoutconstraint::LayoutConstraint;
use crate::ui::autolayout::layoutrelation::LayoutRelation;
use crate::ui::autolayout::layoutstrength::LayoutStrength;
use crate::ui::autolayout::layoutterm::LayoutTerm;
use crate::ui::autolayout::layoutvariable::LayoutVariable;

//==============================================================================
/// 선형 식.
/// - 항(계수 * 변수) 목록과 상수항의 합이다. 같은 변수의 항도 합치지 않고 넣은 차례대로 둔다.
///
/// # 예제
/// 왼쪽과 너비로 가운데 식 만들기
/// ```rust
/// use vanilla::{LayoutExpression, LayoutVariable};
///
/// let left = LayoutVariable::new("left");
/// let width = LayoutVariable::new("width");
/// left.set_value(20.0);
/// width.set_value(100.0);
/// let center_x = LayoutExpression::from_variable(&left).add(LayoutExpression::from_variable(&width).divide(2.0));
/// assert_eq!(center_x.get_terms().len(), 2);
/// assert_eq!(center_x.get_value(), 70.0);
///
/// let shifted = center_x.subtract(10).multiply(2.0);
/// assert_eq!(shifted.get_constant(), -20.0);
/// assert_eq!(shifted.get_value(), 120.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct LayoutExpression {
	terms: Vec<LayoutTerm>,
	constant: f64,
}

impl LayoutExpression {
	//==============================================================================
	/// 생성. (항 목록과 상수항)
	///
	/// # 인자
	/// - `terms`: 항 목록.
	/// - `constant`: 상수항.
	///
	/// # 돌려줌
	/// 새 식.
	//==============================================================================
	pub fn new(terms: Vec<LayoutTerm>, constant: f64) -> LayoutExpression {
		LayoutExpression {
			terms,
			constant,
		}
	}

	//==============================================================================
	/// 변수 하나로 식 만들기. (1 * variable + 0)
	///
	/// # 인자
	/// - `variable`: 식에 넣을 변수. (핸들을 복제해 쥔다)
	///
	/// # 돌려줌
	/// 항 하나(1 * variable)와 상수항 0 의 식.
	//==============================================================================
	pub fn from_variable(variable: &LayoutVariable) -> LayoutExpression {
		let term = LayoutTerm::new(variable.clone(), 1.0);
		LayoutExpression::new(vec![term], 0.0)
	}

	//==============================================================================
	/// 상수만 가진 식 만들기.
	///
	/// # 인자
	/// - `value`: 상수항.
	///
	/// # 돌려줌
	/// 항이 없고 상수항이 value 인 식.
	//==============================================================================
	pub fn from_constant(value: f64) -> LayoutExpression {
		LayoutExpression::new(Vec::new(), value)
	}

	//==============================================================================
	/// 받을 수 있는 입력을 식으로 바꾼다.
	///
	/// # 인자
	/// - `input`: 바꿀 것. (숫자 f64, f32, i32, LayoutVariable, LayoutTerm, LayoutExpression 과 그 참조)
	///
	/// # 돌려줌
	/// 바꾼 식. (숫자는 상수 식, 변수는 1 * variable, 항은 그 항 하나, 식은 사본)
	///
	/// # 예제
	/// 여러 꼴의 입력을 식으로
	/// ```rust
	/// use vanilla::{LayoutExpression, LayoutTerm, LayoutVariable};
	///
	/// let x = LayoutVariable::new("x");
	/// assert!(LayoutExpression::to_expression(12).is_constant());
	/// assert_eq!(LayoutExpression::to_expression(2.5f32).get_constant(), 2.5);
	/// assert_eq!(LayoutExpression::to_expression(&x).get_terms()[0].get_coefficient(), 1.0);
	/// assert_eq!(LayoutExpression::to_expression(LayoutTerm::new(x.clone(), 3.0)).get_terms()[0].get_coefficient(), 3.0);
	/// ```
	//==============================================================================
	pub fn to_expression(input: impl Into<LayoutExpression>) -> LayoutExpression {
		input.into()
	}

	//==============================================================================
	/// 항 목록 반환.
	///
	/// # 돌려줌
	/// 항 목록 조각. (넣은 차례)
	//==============================================================================
	pub fn get_terms(&self) -> &[LayoutTerm] {
		&self.terms
	}

	//==============================================================================
	/// 상수항 반환.
	///
	/// # 돌려줌
	/// 상수항.
	//==============================================================================
	pub fn get_constant(&self) -> f64 {
		self.constant
	}

	//==============================================================================
	/// 항이 없는지. (상수항만 있는지)
	///
	/// # 돌려줌
	/// 항이 없으면 true.
	//==============================================================================
	pub fn is_constant(&self) -> bool {
		self.terms.is_empty()
	}

	//==============================================================================
	/// 평가 값 반환. (항 값의 합 + 상수항)
	///
	/// # 돌려줌
	/// 항마다 계수 * 변수의 지금 값을 더하고 상수항을 더한 값.
	//==============================================================================
	pub fn get_value(&self) -> f64 {
		let mut result = self.get_constant();
		for term in &self.terms {
			result += term.get_value();
		}
		result
	}

	//==============================================================================
	/// 덧셈.
	///
	/// # 인자
	/// - `input`: 더할 것. (숫자, 변수, 항, 식)
	///
	/// # 돌려줌
	/// self + input 인 새 식. (self 의 항 뒤에 input 의 항을 붙이고 상수항을 더한다)
	//==============================================================================
	pub fn add(&self, input: impl Into<LayoutExpression>) -> LayoutExpression {
		let other_expression: LayoutExpression = input.into();
		let mut new_terms = self.terms.clone();
		for other_term in other_expression.terms {
			new_terms.push(other_term);
		}
		let new_constant = self.get_constant() + other_expression.constant;
		LayoutExpression::new(new_terms, new_constant)
	}

	//==============================================================================
	/// 뺄셈.
	///
	/// # 인자
	/// - `input`: 뺄 것. (숫자, 변수, 항, 식)
	///
	/// # 돌려줌
	/// self - input 인 새 식. (input 에 -1 을 곱해 더한다)
	//==============================================================================
	pub fn subtract(&self, input: impl Into<LayoutExpression>) -> LayoutExpression {
		let other_expression: LayoutExpression = input.into();
		let negated = other_expression.multiply(-1.0);
		self.add(negated)
	}

	//==============================================================================
	/// 스칼라 곱셈. (변수끼리의 곱 같은 비선형 곱은 받지 않는다)
	///
	/// # 인자
	/// - `coefficient`: 곱할 수.
	///
	/// # 돌려줌
	/// 모든 계수와 상수항에 coefficient 를 곱한 새 식.
	//==============================================================================
	pub fn multiply(&self, coefficient: f64) -> LayoutExpression {
		let mut new_terms = Vec::with_capacity(self.terms.len());
		for term in &self.terms {
			let variable = term.get_variable().clone();
			let old_coefficient = term.get_coefficient();
			new_terms.push(LayoutTerm::new(variable, old_coefficient * coefficient));
		}
		let new_constant = self.get_constant() * coefficient;
		LayoutExpression::new(new_terms, new_constant)
	}

	//==============================================================================
	/// 스칼라 나눗셈. (0 으로 나누면 panic 한다)
	///
	/// # 인자
	/// - `denominator`: 나눌 수. (0 이면 panic)
	///
	/// # 돌려줌
	/// self * (1 / denominator) 인 새 식.
	//==============================================================================
	pub fn divide(&self, denominator: f64) -> LayoutExpression {
		if denominator == 0.0 {
			panic!("[LayoutExpression] divide: 0 으로 나눌 수 없음.");
		}
		self.multiply(1.0 / denominator)
	}

	//==============================================================================
	/// 같음 제약 만들기. (self == input)
	///
	/// # 인자
	/// - `input`: 맞출 것. (숫자, 변수, 항, 식)
	///
	/// # 돌려줌
	/// 식이 self - input 이고 관계가 Equal 인 required 제약. (솔버에는 걸지 않는다)
	///
	/// # 예제
	/// 오른쪽 == 왼쪽 + 너비를 걸어 풀기
	/// ```rust
	/// use vanilla::{LayoutExpression, LayoutSolver, LayoutVariable};
	///
	/// let solver = LayoutSolver::new();
	/// let left = LayoutVariable::new("left");
	/// let right = LayoutVariable::new("right");
	/// solver.add_constraint(&LayoutExpression::from_variable(&left).equal_to(30)).unwrap();
	/// let right_constraint = LayoutExpression::from_variable(&right).equal_to(LayoutExpression::from_variable(&left).add(200));
	/// solver.add_constraint(&right_constraint).unwrap();
	/// solver.update_variables();
	/// assert!((right.get_value() - 230.0).abs() < 1e-9);
	/// ```
	//==============================================================================
	pub fn equal_to(&self, input: impl Into<LayoutExpression>) -> LayoutConstraint {
		let difference = self.subtract(input);
		LayoutConstraint::new(difference, LayoutRelation::Equal, LayoutStrength::REQUIRED)
	}

	//==============================================================================
	/// 작거나 같음 제약 만들기. (self <= input)
	///
	/// # 인자
	/// - `input`: 넘지 않을 것. (숫자, 변수, 항, 식)
	///
	/// # 돌려줌
	/// 식이 self - input 이고 관계가 LessThanOrEqual 인 required 제약.
	//==============================================================================
	pub fn less_than_or_equal_to(&self, input: impl Into<LayoutExpression>) -> LayoutConstraint {
		let difference = self.subtract(input);
		LayoutConstraint::new(difference, LayoutRelation::LessThanOrEqual, LayoutStrength::REQUIRED)
	}

	//==============================================================================
	/// 크거나 같음 제약 만들기. (self >= input)
	///
	/// # 인자
	/// - `input`: 밑돌지 않을 것. (숫자, 변수, 항, 식)
	///
	/// # 돌려줌
	/// 식이 self - input 이고 관계가 GreaterThanOrEqual 인 required 제약.
	//==============================================================================
	pub fn greater_than_or_equal_to(&self, input: impl Into<LayoutExpression>) -> LayoutConstraint {
		let difference = self.subtract(input);
		LayoutConstraint::new(difference, LayoutRelation::GreaterThanOrEqual, LayoutStrength::REQUIRED)
	}
}

impl From<f64> for LayoutExpression {
	//==============================================================================
	/// 숫자는 상수 식이 된다.
	//==============================================================================
	fn from(value: f64) -> LayoutExpression {
		LayoutExpression::from_constant(value)
	}
}

impl From<f32> for LayoutExpression {
	//==============================================================================
	/// 숫자는 상수 식이 된다.
	//==============================================================================
	fn from(value: f32) -> LayoutExpression {
		LayoutExpression::from_constant(value as f64)
	}
}

impl From<i32> for LayoutExpression {
	//==============================================================================
	/// 숫자는 상수 식이 된다.
	//==============================================================================
	fn from(value: i32) -> LayoutExpression {
		LayoutExpression::from_constant(value as f64)
	}
}

impl From<&LayoutVariable> for LayoutExpression {
	//==============================================================================
	/// 변수는 1 * variable 식이 된다.
	//==============================================================================
	fn from(variable: &LayoutVariable) -> LayoutExpression {
		LayoutExpression::from_variable(variable)
	}
}

impl From<LayoutVariable> for LayoutExpression {
	//==============================================================================
	/// 변수는 1 * variable 식이 된다.
	//==============================================================================
	fn from(variable: LayoutVariable) -> LayoutExpression {
		LayoutExpression::new(vec![LayoutTerm::new(variable, 1.0)], 0.0)
	}
}

impl From<LayoutTerm> for LayoutExpression {
	//==============================================================================
	/// 항은 그 항 하나뿐인 식이 된다.
	//==============================================================================
	fn from(term: LayoutTerm) -> LayoutExpression {
		LayoutExpression::new(vec![term], 0.0)
	}
}

impl From<&LayoutTerm> for LayoutExpression {
	//==============================================================================
	/// 항은 그 항 하나뿐인 식이 된다.
	//==============================================================================
	fn from(term: &LayoutTerm) -> LayoutExpression {
		LayoutExpression::new(vec![term.clone()], 0.0)
	}
}

impl From<&LayoutExpression> for LayoutExpression {
	//==============================================================================
	/// 식은 사본이 된다.
	//==============================================================================
	fn from(expression: &LayoutExpression) -> LayoutExpression {
		expression.clone()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 덧셈, 뺄셈, 곱셈, 나눗셈이 항과 상수항을 원본처럼 셈한다.
	//==============================================================================
	#[test]
	fn arithmetic_builds_terms_and_constant() {
		let x = LayoutVariable::new("x");
		let y = LayoutVariable::new("y");
		x.set_value(3.0);
		y.set_value(5.0);
		let expression = LayoutExpression::from_variable(&x).add(&y).add(10).subtract(2.5).multiply(2.0).divide(4.0);
		assert_eq!(expression.get_terms().len(), 2);
		assert_eq!(expression.get_terms()[0].get_coefficient(), 0.5);
		assert_eq!(expression.get_terms()[1].get_variable(), &y);
		assert_eq!(expression.get_constant(), 3.75);
		assert_eq!(expression.get_value(), 0.5 * 3.0 + 0.5 * 5.0 + 3.75);
		assert!(!expression.is_constant());
		assert!(LayoutExpression::to_expression(7.0f32).is_constant());
		let difference = LayoutExpression::from_variable(&x).subtract(&LayoutExpression::from_variable(&y).multiply(3.0));
		assert_eq!(difference.get_terms()[1].get_coefficient(), -3.0);
		assert_eq!(difference.get_value(), 3.0 - 15.0);
	}

	//==============================================================================
	/// 비교 함수는 (self - input) 식과 관계를 담은 required 제약을 만든다.
	//==============================================================================
	#[test]
	fn comparison_builds_required_constraint() {
		let x = LayoutVariable::new("x");
		let constraint = LayoutExpression::from_variable(&x).greater_than_or_equal_to(20);
		assert_eq!(constraint.get_relation(), LayoutRelation::GreaterThanOrEqual);
		assert_eq!(constraint.get_strength(), LayoutStrength::REQUIRED);
		assert_eq!(constraint.get_expression().get_constant(), -20.0);
		assert_eq!(LayoutExpression::from_variable(&x).equal_to(&x).get_relation(), LayoutRelation::Equal);
		assert_eq!(LayoutExpression::from_variable(&x).less_than_or_equal_to(1.0).get_relation(), LayoutRelation::LessThanOrEqual);
	}

	//==============================================================================
	/// 0 으로 나누면 원본 문구로 panic 한다.
	//==============================================================================
	#[test]
	#[should_panic(expected = "0 으로 나눌 수 없음")]
	fn divide_by_zero_panics() {
		let x = LayoutVariable::new("x");
		let _ = LayoutExpression::from_variable(&x).divide(0.0);
	}
}
