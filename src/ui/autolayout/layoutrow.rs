//==============================================================================
//! 솔버 안의 행. (원본 ui/autolayout/layoutrow.js 의 LayoutRow)
//! - tableau 의 한 행이다. 셀(심볼 -> 계수)과 상수항.
//! - 바꾸는 함수는 모두 그 자리에서 바꾼다.
//! - 원본과 다른 것:
//!   - 셀은 원본 Map 처럼 넣은 차례를 지키는 LayoutSymbolMap 이다.
//!   - 원본 copy() 는 Clone 이다. (차례까지 같은 사본)
//!   - 생략할 수 있던 coefficient(기본 1)는 꼭 받는다.
//!   - 원본이 행과 솔버에 따로 둔 0 근접 판정은 near_zero 하나를 같이 쓴다.
//==============================================================================
use crate::ui::autolayout::layoutsymbol::{LayoutSymbol, LayoutSymbolMap};

/// 0 근접 판정의 문턱. (Kiwi 의 EPS 와 같다)
const NEAR_ZERO_EPSILON: f64 = 1e-8;

//==============================================================================
/// 부동소수가 0 에 가까운지. (Kiwi 의 nearZero 와 같다)
///
/// # 인자
/// - `value`: 볼 값.
///
/// # 돌려줌
/// 절댓값이 1e-8 보다 작으면 true.
//==============================================================================
pub fn near_zero(value: f64) -> bool {
	if value < 0.0 {
		-value < NEAR_ZERO_EPSILON
	}
	else {
		value < NEAR_ZERO_EPSILON
	}
}

//==============================================================================
/// 솔버 안의 행.
/// - 솔버 안에서 쓰는 값이다. 사용자 코드는 보통 LayoutVariable 과 LayoutSolver 만 쓴다.
///
/// # 예제
/// 10 + 2x + 4s = 0 을 x 에 대해 풀기
/// ```rust
/// use vanilla::ui::autolayout::layoutrow::LayoutRow;
/// use vanilla::ui::autolayout::layoutsymbol::LayoutSymbol;
/// use vanilla::ui::autolayout::layoutsymboltype::LayoutSymbolType;
///
/// let x = LayoutSymbol::new(LayoutSymbolType::External);
/// let slack = LayoutSymbol::new(LayoutSymbolType::Slack);
/// let mut row = LayoutRow::new(10.0);
/// row.insert_symbol(x, 2.0);
/// row.insert_symbol(slack, 4.0);
/// row.solve_for(x);
/// assert_eq!(row.get_constant(), -5.0);
/// assert_eq!(row.coefficient_for(slack), -2.0);
/// assert_eq!(row.coefficient_for(x), 0.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct LayoutRow {
	cells: LayoutSymbolMap<f64>,
	constant: f64,
}

impl LayoutRow {
	//==============================================================================
	/// 생성. (constant 는 처음 상수항)
	///
	/// # 인자
	/// - `constant`: 처음 상수항.
	///
	/// # 돌려줌
	/// 셀이 없는 새 행.
	//==============================================================================
	pub fn new(constant: f64) -> LayoutRow {
		LayoutRow {
			cells: LayoutSymbolMap::new(),
			constant,
		}
	}

	//==============================================================================
	/// 셀 맵 반환.
	///
	/// # 돌려줌
	/// 셀 맵(심볼 -> 계수, 넣은 차례)의 참조.
	//==============================================================================
	pub fn get_cells(&self) -> &LayoutSymbolMap<f64> {
		&self.cells
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
	/// 상수항 설정.
	///
	/// # 인자
	/// - `constant`: 새 상수항.
	//==============================================================================
	pub fn set_constant(&mut self, constant: f64) {
		self.constant = constant;
	}

	//==============================================================================
	/// 상수항에 값을 더한다. (새 상수항을 돌려준다)
	///
	/// # 인자
	/// - `value`: 상수항에 더할 값.
	///
	/// # 돌려줌
	/// 더한 뒤의 상수항.
	//==============================================================================
	pub fn add_constant(&mut self, value: f64) -> f64 {
		let new_constant = self.get_constant() + value;
		self.set_constant(new_constant);
		new_constant
	}

	//==============================================================================
	/// 심볼의 계수에 coefficient 를 더한다. (결과가 0 에 가까우면 셀에서 지운다)
	///
	/// # 인자
	/// - `symbol`: 계수를 더할 심볼. (셀에 없으면 계수 0 에서 시작한다)
	/// - `coefficient`: 더할 계수.
	//==============================================================================
	pub fn insert_symbol(&mut self, symbol: LayoutSymbol, coefficient: f64) {
		let previous = self.coefficient_for(symbol);
		let next = previous + coefficient;
		if near_zero(next) {
			self.cells.remove(&symbol);
		}
		else {
			self.cells.insert(symbol, next);
		}
	}

	//==============================================================================
	/// 다른 행에 coefficient 를 곱해 이 행에 더한다.
	///
	/// # 인자
	/// - `other_row`: 더할 행. (상수항과 셀 모두)
	/// - `coefficient`: other_row 에 곱할 수.
	//==============================================================================
	pub fn insert_row(&mut self, other_row: &LayoutRow, coefficient: f64) {
		let other_constant = other_row.get_constant();
		self.add_constant(other_constant * coefficient);
		for (symbol, other_value) in other_row.cells.iter() {
			self.insert_symbol(symbol, *other_value * coefficient);
		}
	}

	//==============================================================================
	/// 심볼 지우기.
	///
	/// # 인자
	/// - `symbol`: 지울 심볼. (셀에 없으면 그대로)
	//==============================================================================
	pub fn remove_symbol(&mut self, symbol: LayoutSymbol) {
		self.cells.remove(&symbol);
	}

	//==============================================================================
	/// 부호 뒤집기. (상수항과 모든 계수에 -1 을 곱한다)
	//==============================================================================
	pub fn reverse_sign(&mut self) {
		self.constant = -self.constant;
		for (_, value) in self.cells.iter_mut() {
			*value = -*value;
		}
	}

	//==============================================================================
	/// 행을 subject 에 대해 푼다.
	/// - subject 의 셀을 지우고, 남은 셀과 상수항에 -1 / subject 계수를 곱한다. (Kiwi 관례)
	/// - subject 가 셀에 없으면 부르는 쪽의 실수다.
	///
	/// # 인자
	/// - `subject`: 풀 심볼. (셀에 있어야 한다. 없으면 0 으로 나눠 상수항과 계수가 무한대나 NaN 이 된다)
	//==============================================================================
	pub fn solve_for(&mut self, subject: LayoutSymbol) {
		let subject_coefficient = self.coefficient_for(subject);
		let inverse = -1.0 / subject_coefficient;
		self.cells.remove(&subject);
		self.constant *= inverse;
		for (_, value) in self.cells.iter_mut() {
			*value *= inverse;
		}
	}

	//==============================================================================
	/// lhs = rhs 꼴로 둔 행을 rhs 에 대해 다시 푼다. (lhs 를 -1 로 넣고 rhs 로 푼다)
	///
	/// # 인자
	/// - `lhs`: 왼쪽 심볼. (계수 -1 로 더한다)
	/// - `rhs`: 다시 풀 심볼.
	//==============================================================================
	pub fn solve_for_pair(&mut self, lhs: LayoutSymbol, rhs: LayoutSymbol) {
		self.insert_symbol(lhs, -1.0);
		self.solve_for(rhs);
	}

	//==============================================================================
	/// 심볼의 계수 반환. (없으면 0)
	///
	/// # 인자
	/// - `symbol`: 찾을 심볼.
	///
	/// # 돌려줌
	/// 계수. 셀에 없으면 0.
	//==============================================================================
	pub fn coefficient_for(&self, symbol: LayoutSymbol) -> f64 {
		match self.cells.get(&symbol) {
			Some(coefficient) => {
				*coefficient
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// symbol 을 substitution_row 로 바꿔 넣는다. (symbol 이 없으면 그대로)
	///
	/// # 인자
	/// - `symbol`: 바꿀 심볼.
	/// - `substitution_row`: symbol 자리에 넣을 행. (symbol 의 계수를 곱해 더한다)
	//==============================================================================
	pub fn substitute(&mut self, symbol: LayoutSymbol, substitution_row: &LayoutRow) {
		let coefficient = match self.cells.remove(&symbol) {
			Some(coefficient) => {
				coefficient
			}
			None => {
				return;
			}
		};
		self.insert_row(substitution_row, coefficient);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ui::autolayout::layoutsymboltype::LayoutSymbolType;

	//==============================================================================
	/// 0 에 가까워진 셀은 지워지고, solve_for 는 subject 계수가 -1 이 되게 나눈다.
	//==============================================================================
	#[test]
	fn row_insert_and_solve() {
		let external_symbol = LayoutSymbol::new(LayoutSymbolType::External);
		let slack_symbol = LayoutSymbol::new(LayoutSymbolType::Slack);
		let mut row = LayoutRow::new(10.0);
		row.insert_symbol(external_symbol, 2.0);
		row.insert_symbol(slack_symbol, 1.0);
		row.insert_symbol(slack_symbol, -1.0);
		assert!(!row.get_cells().contains_key(&slack_symbol));
		row.insert_symbol(slack_symbol, 4.0);
		row.solve_for(external_symbol);
		assert_eq!(row.get_constant(), -5.0);
		assert_eq!(row.coefficient_for(slack_symbol), -2.0);
		assert_eq!(row.coefficient_for(external_symbol), 0.0);
		let mut other = LayoutRow::new(1.0);
		other.insert_symbol(external_symbol, 3.0);
		other.substitute(external_symbol, &row);
		assert_eq!(other.get_constant(), 1.0 - 15.0);
		assert_eq!(other.coefficient_for(slack_symbol), -6.0);
		other.reverse_sign();
		assert_eq!(other.get_constant(), 14.0);
		assert_eq!(other.coefficient_for(slack_symbol), 6.0);
		assert!(near_zero(1e-9));
		assert!(near_zero(-1e-9));
		assert!(!near_zero(1e-7));
	}
}
