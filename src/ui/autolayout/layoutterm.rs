//==============================================================================
//! 식의 한 항. (원본 ui/autolayout/layoutterm.js 의 LayoutTerm, coefficient * variable)
//! - LayoutExpression 을 이루는 값이다. 바뀌지 않는다.
//! - 원본과 다른 것:
//!   - 원본은 변수 없이(null) 만들 수 있지만 솔버가 그런 항을 받으면 망가진다. 러스트 판은 늘 변수를 받는다.
//!   - 생략할 수 있던 coefficient(기본 1)는 꼭 받는다.
//==============================================================================
use crate::ui::autolayout::layoutvariable::LayoutVariable;

//==============================================================================
/// 식의 한 항.
///
/// # 예제
/// 너비의 절반 항으로 식 만들기
/// ```rust
/// use vanilla::{LayoutExpression, LayoutTerm, LayoutVariable};
///
/// let width = LayoutVariable::new("width");
/// width.set_value(80.0);
/// let half_width = LayoutTerm::new(width.clone(), 0.5);
/// assert_eq!(half_width.get_variable(), &width);
/// assert_eq!(half_width.get_value(), 40.0);
///
/// let expression = LayoutExpression::from(half_width).add(10);
/// assert_eq!(expression.get_value(), 50.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct LayoutTerm {
	variable: LayoutVariable,
	coefficient: f64,
}

impl LayoutTerm {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `variable`: 항의 변수. (핸들을 그대로 쥔다)
	/// - `coefficient`: 변수에 곱할 계수.
	///
	/// # 돌려줌
	/// 새 항.
	//==============================================================================
	pub fn new(variable: LayoutVariable, coefficient: f64) -> LayoutTerm {
		LayoutTerm {
			variable,
			coefficient,
		}
	}

	//==============================================================================
	/// 변수 반환.
	///
	/// # 돌려줌
	/// 항이 쥔 변수 핸들의 참조.
	//==============================================================================
	pub fn get_variable(&self) -> &LayoutVariable {
		&self.variable
	}

	//==============================================================================
	/// 계수 반환.
	///
	/// # 돌려줌
	/// 계수.
	//==============================================================================
	pub fn get_coefficient(&self) -> f64 {
		self.coefficient
	}

	//==============================================================================
	/// 평가 값 반환. (coefficient * 변수 값)
	///
	/// # 돌려줌
	/// 계수 * 변수의 지금 값.
	//==============================================================================
	pub fn get_value(&self) -> f64 {
		let coefficient = self.get_coefficient();
		let variable_value = self.variable.get_value();
		coefficient * variable_value
	}
}
