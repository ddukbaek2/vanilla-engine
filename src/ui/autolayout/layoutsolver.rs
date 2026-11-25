//==============================================================================
//! 제약 솔버. (원본 ui/autolayout/layoutsolver.js 의 LayoutSolver, Kiwi dual simplex 기반)
//! - 사용:
//!   let solver = LayoutSolver::new();
//!   let x = LayoutVariable::new("x");
//!   let y = LayoutVariable::new("y");
//!   solver.add_constraint(&LayoutExpression::from_variable(&x).equal_to(10))?;
//!   solver.add_constraint(&LayoutExpression::from_variable(&y).equal_to(LayoutExpression::from_variable(&x).add(20)))?;
//!   solver.update_variables();
//!   x.get_value(); // 10
//!   y.get_value(); // 30
//! - 움직이는 입력(끌기 등):
//!   solver.add_edit_variable(&x, LayoutStrength::STRONG)?;
//!   solver.suggest_value(&x, 50.0)?;
//!   solver.update_variables();
//! - 원본과 다른 것:
//!   - 원본은 객체 하나를 여러 UINode 와 UIScene 이 같이 쥔다. 러스트 판은 `Rc<RefCell>` 을 품은 값싼 Clone 핸들이고 함수는 모두 &self 다.
//!   - 원본이 예외를 던지는 곳(겹친 제약, 만족할 수 없는 required 제약, 모르는 제약과 편집 변수, required 편집 강도,
//!   ```text
//!     솔버 안의 실패)은 Result<(), LayoutError> 를 돌려준다. 문구는 원본과 같되 긴 줄표는 쌍점으로 바꿨다.
//!   ```
//!   - 원본의 Map 가운데 차례가 고르는 심볼을 바꾸는 행 맵은 넣은 차례를 지키는 LayoutSymbolMap 이다.
//!   ```text
//!     차례를 보지 않는 제약, 변수, 편집 변수 맵은 HashMap 이다.
//!   ```
//!   - 원본 LayoutTag 의 null 심볼은 무효 심볼(LayoutSymbol::invalid)이다. (Kiwi 와 같다)
//==============================================================================
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::ui::autolayout::layoutconstraint::LayoutConstraint;
use crate::ui::autolayout::layoutexpression::LayoutExpression;
use crate::ui::autolayout::layoutrelation::LayoutRelation;
use crate::ui::autolayout::layoutrow::{near_zero, LayoutRow};
use crate::ui::autolayout::layoutstrength::LayoutStrength;
use crate::ui::autolayout::layoutsymbol::{LayoutSymbol, LayoutSymbolMap};
use crate::ui::autolayout::layoutsymboltype::LayoutSymbolType;
use crate::ui::autolayout::layoutvariable::LayoutVariable;

/// 최적화 반복의 상한. (원본 safeguard)
const OPTIMIZE_SAFEGUARD: usize = 100000;

//==============================================================================
/// 솔버 오류. (원본이 던지는 예외 자리)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
	/// 이미 추가한 제약을 또 추가했다.
	DuplicateConstraint,
	/// required 제약을 만족할 수 없다.
	UnsatisfiableConstraint,
	/// 등록하지 않은 제약을 지우려 했다.
	UnknownConstraint,
	/// 이미 편집 변수로 등록한 변수를 또 등록했다.
	DuplicateEditVariable,
	/// 편집 변수에 required 강도를 주었다.
	BadRequiredStrength,
	/// 등록하지 않은 편집 변수를 쓰거나 지우려 했다.
	UnknownEditVariable,
	/// 제약을 지울 때 marker 가 떠날 행을 못 찾았다. (솔버 안의 실패)
	RemoveConstraintFailed,
	/// 목적함수가 끝없이 줄어든다. (솔버 안의 실패)
	UnboundedObjective,
	/// optimize 반복 상한을 넘었다. (솔버 안의 실패)
	OptimizeLimitExceeded,
	/// dual_optimize 에서 들어올 심볼을 못 찾았다. (솔버 안의 실패)
	DualOptimizeFailed,
	/// dual_optimize 반복 상한을 넘었다. (솔버 안의 실패)
	DualOptimizeLimitExceeded,
}

impl fmt::Display for LayoutError {
	//==============================================================================
	/// 원본 예외 문구를 보인다.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		let message = match self {
			LayoutError::DuplicateConstraint => {
				"[LayoutSolver] 이미 추가된 제약."
			}
			LayoutError::UnsatisfiableConstraint => {
				"[LayoutSolver] 모순 제약: 만족 불가능."
			}
			LayoutError::UnknownConstraint => {
				"[LayoutSolver] 등록되지 않은 제약."
			}
			LayoutError::DuplicateEditVariable => {
				"[LayoutSolver] 이미 편집 변수로 등록됨."
			}
			LayoutError::BadRequiredStrength => {
				"[LayoutSolver] 편집 변수에는 required 강도를 쓸 수 없음."
			}
			LayoutError::UnknownEditVariable => {
				"[LayoutSolver] 등록되지 않은 편집 변수."
			}
			LayoutError::RemoveConstraintFailed => {
				"[LayoutSolver] 제약 제거 실패: leaving row 없음."
			}
			LayoutError::UnboundedObjective => {
				"[LayoutSolver] 목적함수가 무한대: bound 가 부족함."
			}
			LayoutError::OptimizeLimitExceeded => {
				"[LayoutSolver] optimize 반복 한계 초과."
			}
			LayoutError::DualOptimizeFailed => {
				"[LayoutSolver] dual optimize 실패: entering 없음."
			}
			LayoutError::DualOptimizeLimitExceeded => {
				"[LayoutSolver] dual_optimize 반복 한계 초과."
			}
		};
		formatter.write_str(message)
	}
}

impl std::error::Error for LayoutError {
}

//==============================================================================
/// 제약을 등록할 때 붙는 marker, other 심볼 쌍. (없으면 무효 심볼)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct LayoutTag {
	marker: LayoutSymbol,
	other: LayoutSymbol,
}

impl LayoutTag {
	//==============================================================================
	/// 생성. (둘 다 무효 심볼)
	//==============================================================================
	fn new() -> LayoutTag {
		LayoutTag {
			marker: LayoutSymbol::invalid(),
			other: LayoutSymbol::invalid(),
		}
	}
}

//==============================================================================
/// 편집 변수 정보.
//==============================================================================
struct LayoutEditInfo {
	constraint: LayoutConstraint,
	tag: LayoutTag,
	constant: f64,
}

//==============================================================================
/// optimize 가 줄일 목적함수. (원본은 행 객체를 넘긴다)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq)]
enum LayoutObjective {
	/// 솔버의 목적함수.
	Main,
	/// 인공 변수로 행을 넣을 때의 인공 목적함수.
	Artificial,
}

//==============================================================================
/// 핸들이 같이 가리키는 솔버 본체.
//==============================================================================
struct LayoutSolverData {
	constraint_tags: HashMap<LayoutConstraint, LayoutTag>,
	rows: LayoutSymbolMap<LayoutRow>,
	variable_symbols: HashMap<LayoutVariable, LayoutSymbol>,
	edit_infos: HashMap<LayoutVariable, LayoutEditInfo>,
	infeasible_rows: Vec<LayoutSymbol>,
	objective: LayoutRow,
	artificial: Option<LayoutRow>,
}

//==============================================================================
/// 제약 솔버. (복제해도 같은 솔버를 가리킨다)
///
/// # 예제
/// x == 10, y == x + 20 풀기
/// ```rust
/// use vanilla::{LayoutExpression, LayoutSolver, LayoutVariable};
///
/// # fn main() -> Result<(), vanilla::LayoutError> {
/// let solver = LayoutSolver::new();
/// let x = LayoutVariable::new("x");
/// let y = LayoutVariable::new("y");
/// solver.add_constraint(&LayoutExpression::from_variable(&x).equal_to(10))?;
/// solver.add_constraint(&LayoutExpression::from_variable(&y).equal_to(LayoutExpression::from_variable(&x).add(20)))?;
/// solver.update_variables();
/// assert!((x.get_value() - 10.0).abs() < 1e-9);
/// assert!((y.get_value() - 30.0).abs() < 1e-9);
/// # Ok(())
/// # }
/// ```
///
/// 움직이는 입력(끌기 등)은 편집 변수로
/// ```rust
/// use vanilla::{LayoutExpression, LayoutSolver, LayoutStrength, LayoutVariable};
///
/// # fn main() -> Result<(), vanilla::LayoutError> {
/// let solver = LayoutSolver::new();
/// let x = LayoutVariable::new("x");
/// solver.add_constraint(&LayoutExpression::from_variable(&x).less_than_or_equal_to(100))?;
/// solver.add_edit_variable(&x, LayoutStrength::STRONG)?;
/// solver.suggest_value(&x, 50.0)?;
/// solver.update_variables();
/// assert!((x.get_value() - 50.0).abs() < 1e-9);
/// # Ok(())
/// # }
/// ```
//==============================================================================
#[derive(Clone)]
pub struct LayoutSolver {
	data: Rc<RefCell<LayoutSolverData>>,
}

impl LayoutSolver {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 제약이 없는 새 솔버 핸들. (복제하면 같은 솔버를 가리킨다)
	//==============================================================================
	pub fn new() -> LayoutSolver {
		LayoutSolver {
			data: Rc::new(RefCell::new(LayoutSolverData {
				constraint_tags: HashMap::new(),
				rows: LayoutSymbolMap::new(),
				variable_symbols: HashMap::new(),
				edit_infos: HashMap::new(),
				infeasible_rows: Vec::new(),
				objective: LayoutRow::new(0.0),
				artificial: None,
			})),
		}
	}

	//==============================================================================
	/// 제약 추가.
	///
	/// # 인자
	/// - `constraint`: 걸 제약. (솔버가 핸들을 복제해 쥔다)
	///
	/// # 돌려줌
	/// 걸었으면 Ok(()).
	/// - 같은 제약(같은 핸들이나 그 복제)을 이미 걸었으면 Err(DuplicateConstraint).
	/// - required 제약을 만족할 수 없으면 Err(UnsatisfiableConstraint). 부등식이 그렇게 실패하면 솔버 안이 되돌려지지 않는다. (원본, Kiwi 와 같다)
	/// - 최적화가 실패하면 Err(UnboundedObjective) 나 Err(OptimizeLimitExceeded).
	///
	/// # 예제
	/// 겹친 제약과 모순 제약의 오류
	/// ```rust
	/// use vanilla::{LayoutError, LayoutExpression, LayoutSolver, LayoutVariable};
	///
	/// let solver = LayoutSolver::new();
	/// let x = LayoutVariable::new("x");
	/// let constraint = LayoutExpression::from_variable(&x).equal_to(10);
	/// assert_eq!(solver.add_constraint(&constraint), Ok(()));
	/// assert_eq!(solver.add_constraint(&constraint.clone()), Err(LayoutError::DuplicateConstraint));
	/// let conflict = LayoutExpression::from_variable(&x).equal_to(20);
	/// assert_eq!(solver.add_constraint(&conflict), Err(LayoutError::UnsatisfiableConstraint));
	/// assert!(!solver.has_constraint(&conflict));
	/// ```
	//==============================================================================
	pub fn add_constraint(&self, constraint: &LayoutConstraint) -> Result<(), LayoutError> {
		self.data.borrow_mut().add_constraint(constraint)?;
		Ok(())
	}

	//==============================================================================
	/// 제약 제거.
	///
	/// # 인자
	/// - `constraint`: 뺄 제약. (건 핸들이나 그 복제)
	///
	/// # 돌려줌
	/// 뺐으면 Ok(()).
	/// - 걸지 않은 제약이면 Err(UnknownConstraint). (식이 같아도 따로 만든 제약은 모르는 제약이다)
	/// - marker 가 떠날 행을 못 찾으면 Err(RemoveConstraintFailed), 최적화가 실패하면 Err(UnboundedObjective) 나 Err(OptimizeLimitExceeded).
	//==============================================================================
	pub fn remove_constraint(&self, constraint: &LayoutConstraint) -> Result<(), LayoutError> {
		self.data.borrow_mut().remove_constraint(constraint)
	}

	//==============================================================================
	/// 제약이 등록되어 있는지. (핸들이 가리키는 제약으로 가린다)
	///
	/// # 인자
	/// - `constraint`: 볼 제약.
	///
	/// # 돌려줌
	/// 걸려 있으면 true.
	//==============================================================================
	pub fn has_constraint(&self, constraint: &LayoutConstraint) -> bool {
		self.data.borrow().constraint_tags.contains_key(constraint)
	}

	//==============================================================================
	/// 편집 변수 추가. (suggest_value 를 쓰기 전에 꼭 등록한다. strength 는 REQUIRED 보다 작아야 한다)
	///
	/// # 인자
	/// - `variable`: 편집 변수로 쓸 변수.
	/// - `strength`: 바라는 값을 지킬 강도. (0 ~ REQUIRED 로 자른 값이 REQUIRED 보다 작아야 한다)
	///
	/// # 돌려줌
	/// 등록했으면 Ok(()).
	/// - 이미 편집 변수면 Err(DuplicateEditVariable).
	/// - 자른 강도가 REQUIRED 면 Err(BadRequiredStrength).
	/// - 안에서 거는 편집 제약(variable == 바라는 값, 처음 바라는 값은 0)이 실패하면 add_constraint 의 오류.
	//==============================================================================
	pub fn add_edit_variable(&self, variable: &LayoutVariable, strength: f64) -> Result<(), LayoutError> {
		self.data.borrow_mut().add_edit_variable(variable, strength)
	}

	//==============================================================================
	/// 편집 변수 제거.
	///
	/// # 인자
	/// - `variable`: 뺄 편집 변수.
	///
	/// # 돌려줌
	/// 뺐으면 Ok(()). 편집 변수가 아니면 Err(UnknownEditVariable). 편집 제약을 빼다 실패하면 remove_constraint 의 오류.
	//==============================================================================
	pub fn remove_edit_variable(&self, variable: &LayoutVariable) -> Result<(), LayoutError> {
		self.data.borrow_mut().remove_edit_variable(variable)
	}

	//==============================================================================
	/// 편집 변수로 등록되어 있는지.
	///
	/// # 인자
	/// - `variable`: 볼 변수.
	///
	/// # 돌려줌
	/// 편집 변수로 등록되어 있으면 true.
	//==============================================================================
	pub fn has_edit_variable(&self, variable: &LayoutVariable) -> bool {
		self.data.borrow().edit_infos.contains_key(variable)
	}

	//==============================================================================
	/// 편집 변수에 바라는 값을 준다. (끌기 갱신 등)
	/// - 솔버는 되도록 그 값을 지키되 다른 제약과 부딪히면 강도에 따라 양보한다.
	///
	/// # 인자
	/// - `variable`: 편집 변수. (add_edit_variable 로 등록한 것)
	/// - `value`: 바라는 값.
	///
	/// # 돌려줌
	/// 반영했으면 Ok(()). (변수 값은 update_variables 뒤에 바뀐다)
	/// - 편집 변수가 아니면 Err(UnknownEditVariable).
	/// - dual optimize 가 실패하면 Err(DualOptimizeFailed) 나 Err(DualOptimizeLimitExceeded).
	///
	/// # 예제
	/// 끌기로 값을 옮기되 required 제약에서 멈춘다
	/// ```rust
	/// use vanilla::{LayoutExpression, LayoutSolver, LayoutStrength, LayoutVariable};
	///
	/// # fn main() -> Result<(), vanilla::LayoutError> {
	/// let solver = LayoutSolver::new();
	/// let x = LayoutVariable::new("x");
	/// solver.add_constraint(&LayoutExpression::from_variable(&x).less_than_or_equal_to(100))?;
	/// solver.add_edit_variable(&x, LayoutStrength::STRONG)?;
	/// for drag_value in [30.0, 60.0, 150.0] {
	/// 	solver.suggest_value(&x, drag_value)?;
	/// 	solver.update_variables();
	/// }
	/// assert!((x.get_value() - 100.0).abs() < 1e-9);
	/// solver.remove_edit_variable(&x)?;
	/// assert!(!solver.has_edit_variable(&x));
	/// # Ok(())
	/// # }
	/// ```
	//==============================================================================
	pub fn suggest_value(&self, variable: &LayoutVariable, value: f64) -> Result<(), LayoutError> {
		self.data.borrow_mut().suggest_value(variable, value)
	}

	//==============================================================================
	/// 변수 값 갱신. (tableau 의 external 행 상수항을 각 LayoutVariable 에 넣는다)
	//==============================================================================
	pub fn update_variables(&self) {
		self.data.borrow().update_variables();
	}
}

impl Default for LayoutSolver {
	//==============================================================================
	/// 생성. (LayoutSolver::new 와 같다)
	//==============================================================================
	fn default() -> LayoutSolver {
		LayoutSolver::new()
	}
}

impl LayoutSolverData {
	//==============================================================================
	/// 제약 추가. (붙인 tag 를 돌려준다)
	//==============================================================================
	fn add_constraint(&mut self, constraint: &LayoutConstraint) -> Result<LayoutTag, LayoutError> {
		if self.constraint_tags.contains_key(constraint) {
			return Err(LayoutError::DuplicateConstraint);
		}
		let mut tag = LayoutTag::new();
		let mut row = self.create_row(constraint, &mut tag);
		let mut subject = self.choose_subject(&row, &tag);

		// subject 가 무효이고 모든 항이 dummy 면 따로 다룬다.
		if subject.is_invalid() && all_dummies(&row) {
			if !near_zero(row.get_constant()) {
				return Err(LayoutError::UnsatisfiableConstraint);
			}
			subject = tag.marker;
		}

		if subject.is_invalid() {
			// 인공 변수로 행을 넣어 본다.
			let success = self.add_with_artificial_variable(&row)?;
			if !success {
				return Err(LayoutError::UnsatisfiableConstraint);
			}
		}
		else {
			row.solve_for(subject);
			self.substitute(subject, &row);
			self.rows.insert(subject, row);
		}

		self.constraint_tags.insert(constraint.clone(), tag);
		self.optimize(LayoutObjective::Main)?;
		Ok(tag)
	}

	//==============================================================================
	/// 제약 제거.
	//==============================================================================
	fn remove_constraint(&mut self, constraint: &LayoutConstraint) -> Result<(), LayoutError> {
		let tag = match self.constraint_tags.remove(constraint) {
			Some(tag) => {
				tag
			}
			None => {
				return Err(LayoutError::UnknownConstraint);
			}
		};

		// 오차 변수가 목적함수에 끼친 것을 걷어 낸다.
		self.remove_constraint_effects(constraint, &tag);

		// marker 행을 지우고, marker 가 basic 이 아니면 떠날 행을 찾아 지운다.
		let marker = tag.marker;
		if self.rows.contains_key(&marker) {
			self.rows.remove(&marker);
		}
		else {
			let leaving_symbol = self.get_marker_leaving_symbol(marker);
			if leaving_symbol.is_invalid() {
				return Err(LayoutError::RemoveConstraintFailed);
			}
			let mut leaving_row = self.take_row(leaving_symbol);
			leaving_row.solve_for_pair(leaving_symbol, marker);
			self.substitute(marker, &leaving_row);
		}

		self.optimize(LayoutObjective::Main)
	}

	//==============================================================================
	/// 편집 변수 추가.
	//==============================================================================
	fn add_edit_variable(&mut self, variable: &LayoutVariable, strength: f64) -> Result<(), LayoutError> {
		if self.edit_infos.contains_key(variable) {
			return Err(LayoutError::DuplicateEditVariable);
		}
		let clipped = LayoutStrength::clip_strength(strength);
		if clipped >= LayoutStrength::REQUIRED {
			return Err(LayoutError::BadRequiredStrength);
		}
		let expression = LayoutExpression::from_variable(variable);
		let constraint = LayoutConstraint::new(expression, LayoutRelation::Equal, clipped);
		let tag = self.add_constraint(&constraint)?;
		let edit_info = LayoutEditInfo {
			constraint,
			tag,
			constant: 0.0,
		};
		self.edit_infos.insert(variable.clone(), edit_info);
		Ok(())
	}

	//==============================================================================
	/// 편집 변수 제거.
	//==============================================================================
	fn remove_edit_variable(&mut self, variable: &LayoutVariable) -> Result<(), LayoutError> {
		let constraint = match self.edit_infos.get(variable) {
			Some(edit_info) => {
				edit_info.constraint.clone()
			}
			None => {
				return Err(LayoutError::UnknownEditVariable);
			}
		};
		self.remove_constraint(&constraint)?;
		self.edit_infos.remove(variable);
		Ok(())
	}

	//==============================================================================
	/// 편집 변수에 바라는 값을 준다.
	//==============================================================================
	fn suggest_value(&mut self, variable: &LayoutVariable, value: f64) -> Result<(), LayoutError> {
		let (delta, tag) = match self.edit_infos.get_mut(variable) {
			Some(edit_info) => {
				let delta = value - edit_info.constant;
				edit_info.constant = value;
				(delta, edit_info.tag)
			}
			None => {
				return Err(LayoutError::UnknownEditVariable);
			}
		};
		let marker_symbol = tag.marker;
		let other_symbol = tag.other;

		// marker 나 other 가 basic 이면 그 행의 상수항만 고친다.
		if let Some(row) = self.rows.get_mut(&marker_symbol) {
			if row.add_constant(-delta) < 0.0 {
				self.infeasible_rows.push(marker_symbol);
			}
			return self.dual_optimize();
		}
		if let Some(row) = self.rows.get_mut(&other_symbol) {
			if row.add_constant(delta) < 0.0 {
				self.infeasible_rows.push(other_symbol);
			}
			return self.dual_optimize();
		}

		// 그 밖에는 marker 가 나오는 모든 행의 상수항을 고친다.
		for (symbol, row) in self.rows.iter_mut() {
			let coefficient = row.coefficient_for(marker_symbol);
			if coefficient == 0.0 {
				continue;
			}
			if row.add_constant(delta * coefficient) < 0.0 && symbol.get_type() != LayoutSymbolType::External {
				self.infeasible_rows.push(symbol);
			}
		}
		self.dual_optimize()
	}

	//==============================================================================
	/// 변수 값 갱신.
	//==============================================================================
	fn update_variables(&self) {
		for (variable, symbol) in &self.variable_symbols {
			match self.rows.get(symbol) {
				Some(row) => {
					variable.set_value(row.get_constant());
				}
				None => {
					variable.set_value(0.0);
				}
			}
		}
	}

	//==============================================================================
	/// 변수의 심볼 찾기. (없으면 만든다)
	//==============================================================================
	fn get_or_create_symbol(&mut self, variable: &LayoutVariable) -> LayoutSymbol {
		if let Some(symbol) = self.variable_symbols.get(variable) {
			return *symbol;
		}
		let symbol = LayoutSymbol::new(LayoutSymbolType::External);
		self.variable_symbols.insert(variable.clone(), symbol);
		symbol
	}

	//==============================================================================
	/// 행에서 symbol 의 행을 꺼낸다. (부르는 쪽이 행 맵에서 고른 심볼이라 늘 있다)
	//==============================================================================
	fn take_row(&mut self, symbol: LayoutSymbol) -> LayoutRow {
		match self.rows.remove(&symbol) {
			Some(row) => {
				row
			}
			None => {
				panic!("[LayoutSolver] 행 맵에 없는 심볼.");
			}
		}
	}

	//==============================================================================
	/// 제약으로 행 만들기. (tag 에 marker, other 심볼을 채운다)
	//==============================================================================
	fn create_row(&mut self, constraint: &LayoutConstraint, tag: &mut LayoutTag) -> LayoutRow {
		let expression = constraint.get_expression();
		let mut row = LayoutRow::new(expression.get_constant());

		// 외부 변수 항을 넣는다. basic 인 항은 그 행으로 바꿔 넣는다.
		for term in expression.get_terms() {
			let coefficient = term.get_coefficient();
			if near_zero(coefficient) {
				continue;
			}
			let symbol = self.get_or_create_symbol(term.get_variable());
			match self.rows.get(&symbol) {
				Some(basic_row) => {
					row.insert_row(basic_row, coefficient);
				}
				None => {
					row.insert_symbol(symbol, coefficient);
				}
			}
		}

		let relation = constraint.get_relation();
		let strength = constraint.get_strength();

		if relation == LayoutRelation::LessThanOrEqual || relation == LayoutRelation::GreaterThanOrEqual {
			let sign = if relation == LayoutRelation::LessThanOrEqual {
				1.0
			}
			else {
				-1.0
			};
			let slack_symbol = LayoutSymbol::new(LayoutSymbolType::Slack);
			tag.marker = slack_symbol;
			row.insert_symbol(slack_symbol, sign);
			if strength < LayoutStrength::REQUIRED {
				let error_symbol = LayoutSymbol::new(LayoutSymbolType::Error);
				tag.other = error_symbol;
				row.insert_symbol(error_symbol, -sign);
				self.objective.insert_symbol(error_symbol, strength);
			}
		}
		else {
			// 같음
			if strength < LayoutStrength::REQUIRED {
				let error_plus = LayoutSymbol::new(LayoutSymbolType::Error);
				let error_minus = LayoutSymbol::new(LayoutSymbolType::Error);
				tag.marker = error_plus;
				tag.other = error_minus;
				row.insert_symbol(error_plus, -1.0);
				row.insert_symbol(error_minus, 1.0);
				self.objective.insert_symbol(error_plus, strength);
				self.objective.insert_symbol(error_minus, strength);
			}
			else {
				let dummy_symbol = LayoutSymbol::new(LayoutSymbolType::Dummy);
				tag.marker = dummy_symbol;
				row.insert_symbol(dummy_symbol, 1.0);
			}
		}

		// 상수항이 음수면 양변에 -1 을 곱해 양수로 만든다.
		if row.get_constant() < 0.0 {
			row.reverse_sign();
		}

		row
	}

	//==============================================================================
	/// 행에서 subject 심볼 고르기.
	/// - 1) 외부 변수가 먼저. 2) 계수가 음수인 slack, error 의 marker, other. 3) 둘 다 없으면 무효.
	//==============================================================================
	fn choose_subject(&self, row: &LayoutRow, tag: &LayoutTag) -> LayoutSymbol {
		for (symbol, _) in row.get_cells().iter() {
			if symbol.get_type() == LayoutSymbolType::External {
				return symbol;
			}
		}
		let marker_symbol = tag.marker;
		if marker_symbol.get_type() == LayoutSymbolType::Slack || marker_symbol.get_type() == LayoutSymbolType::Error {
			if row.coefficient_for(marker_symbol) < 0.0 {
				return marker_symbol;
			}
		}
		let other_symbol = tag.other;
		if other_symbol.get_type() == LayoutSymbolType::Slack || other_symbol.get_type() == LayoutSymbolType::Error {
			if row.coefficient_for(other_symbol) < 0.0 {
				return other_symbol;
			}
		}
		LayoutSymbol::invalid()
	}

	//==============================================================================
	/// 인공 변수로 행을 억지로 넣는다. (성공했는지 돌려준다)
	//==============================================================================
	fn add_with_artificial_variable(&mut self, row: &LayoutRow) -> Result<bool, LayoutError> {
		let artificial_symbol = LayoutSymbol::new(LayoutSymbolType::Slack);
		self.rows.insert(artificial_symbol, row.clone());
		self.artificial = Some(row.clone());

		let optimize_result = self.optimize(LayoutObjective::Artificial);
		let success = match &self.artificial {
			Some(artificial) => {
				near_zero(artificial.get_constant())
			}
			None => {
				false
			}
		};
		self.artificial = None;
		optimize_result?;

		// 인공 행이 아직 basic 이면 지운다.
		if let Some(mut basic_row) = self.rows.remove(&artificial_symbol) {
			if basic_row.get_cells().is_empty() {
				return Ok(success);
			}
			let entering = any_pivotable_symbol(&basic_row);
			if entering.is_invalid() {
				return Ok(false);
			}
			basic_row.solve_for_pair(artificial_symbol, entering);
			self.substitute(entering, &basic_row);
			self.rows.insert(entering, basic_row);
		}

		// 모든 행에서 인공 심볼을 지운다.
		for (_, current_row) in self.rows.iter_mut() {
			current_row.remove_symbol(artificial_symbol);
		}
		self.objective.remove_symbol(artificial_symbol);
		Ok(success)
	}

	//==============================================================================
	/// 모든 행과 목적함수에서 symbol 을 row 로 바꿔 넣는다.
	//==============================================================================
	fn substitute(&mut self, symbol: LayoutSymbol, row: &LayoutRow) {
		for (basic_symbol, basic_row) in self.rows.iter_mut() {
			basic_row.substitute(symbol, row);
			if basic_symbol.get_type() != LayoutSymbolType::External && basic_row.get_constant() < 0.0 {
				self.infeasible_rows.push(basic_symbol);
			}
		}
		self.objective.substitute(symbol, row);
		if let Some(artificial) = &mut self.artificial {
			artificial.substitute(symbol, row);
		}
	}

	//==============================================================================
	/// 1차(primal) 최적화. 목적함수에 음수 계수가 없어질 때까지 되풀이한다.
	//==============================================================================
	fn optimize(&mut self, objective: LayoutObjective) -> Result<(), LayoutError> {
		for _ in 0..OPTIMIZE_SAFEGUARD {
			let entering_symbol = match objective {
				LayoutObjective::Main => {
					get_entering_symbol(&self.objective)
				}
				LayoutObjective::Artificial => {
					match &self.artificial {
						Some(artificial) => {
							get_entering_symbol(artificial)
						}
						None => {
							LayoutSymbol::invalid()
						}
					}
				}
			};
			if entering_symbol.is_invalid() {
				return Ok(());
			}
			let leaving_symbol = self.get_leaving_symbol(entering_symbol);
			if leaving_symbol.is_invalid() {
				return Err(LayoutError::UnboundedObjective);
			}
			let mut leaving_row = self.take_row(leaving_symbol);
			leaving_row.solve_for_pair(leaving_symbol, entering_symbol);
			self.substitute(entering_symbol, &leaving_row);
			self.rows.insert(entering_symbol, leaving_row);
		}
		Err(LayoutError::OptimizeLimitExceeded)
	}

	//==============================================================================
	/// dual simplex 최적화. 상수항이 음수가 된(infeasible) 행을 고친다.
	//==============================================================================
	fn dual_optimize(&mut self) -> Result<(), LayoutError> {
		for _ in 0..OPTIMIZE_SAFEGUARD {
			let leaving_symbol = match self.infeasible_rows.pop() {
				Some(symbol) => {
					symbol
				}
				None => {
					return Ok(());
				}
			};
			let entering_symbol = match self.rows.get(&leaving_symbol) {
				Some(row) => {
					if row.get_constant() >= 0.0 {
						continue;
					}
					self.get_dual_entering_symbol(row)
				}
				None => {
					continue;
				}
			};
			if entering_symbol.is_invalid() {
				return Err(LayoutError::DualOptimizeFailed);
			}
			let mut row = self.take_row(leaving_symbol);
			row.solve_for_pair(leaving_symbol, entering_symbol);
			self.substitute(entering_symbol, &row);
			self.rows.insert(entering_symbol, row);
		}
		Err(LayoutError::DualOptimizeLimitExceeded)
	}

	//==============================================================================
	/// 떠날 심볼 고르기. (비율 시험)
	//==============================================================================
	fn get_leaving_symbol(&self, entering_symbol: LayoutSymbol) -> LayoutSymbol {
		let mut ratio = f64::INFINITY;
		let mut result = LayoutSymbol::invalid();
		for (symbol, row) in self.rows.iter() {
			if symbol.get_type() == LayoutSymbolType::External {
				continue;
			}
			let coefficient = row.coefficient_for(entering_symbol);
			if coefficient < 0.0 {
				let tentative_ratio = -row.get_constant() / coefficient;
				if tentative_ratio < ratio {
					ratio = tentative_ratio;
					result = symbol;
				}
			}
		}
		result
	}

	//==============================================================================
	/// dual 에서 들어올 심볼 고르기.
	//==============================================================================
	fn get_dual_entering_symbol(&self, row: &LayoutRow) -> LayoutSymbol {
		let mut ratio = f64::INFINITY;
		let mut result = LayoutSymbol::invalid();
		for (symbol, value) in row.get_cells().iter() {
			if *value > 0.0 && symbol.get_type() != LayoutSymbolType::Dummy {
				let objective_coefficient = self.objective.coefficient_for(symbol);
				let tentative_ratio = objective_coefficient / *value;
				if tentative_ratio < ratio {
					ratio = tentative_ratio;
					result = symbol;
				}
			}
		}
		result
	}

	//==============================================================================
	/// marker 를 지울 때 떠날 행의 심볼 고르기.
	//==============================================================================
	fn get_marker_leaving_symbol(&self, marker: LayoutSymbol) -> LayoutSymbol {
		let mut first_ratio = f64::INFINITY;
		let mut second_ratio = f64::INFINITY;
		let mut first_result = LayoutSymbol::invalid();
		let mut second_result = LayoutSymbol::invalid();
		let mut third_result = LayoutSymbol::invalid();

		for (symbol, row) in self.rows.iter() {
			let coefficient = row.coefficient_for(marker);
			if coefficient == 0.0 {
				continue;
			}
			if symbol.get_type() == LayoutSymbolType::External {
				third_result = symbol;
			}
			else if coefficient < 0.0 {
				let tentative_ratio = -row.get_constant() / coefficient;
				if tentative_ratio < first_ratio {
					first_ratio = tentative_ratio;
					first_result = symbol;
				}
			}
			else {
				let tentative_ratio = row.get_constant() / coefficient;
				if tentative_ratio < second_ratio {
					second_ratio = tentative_ratio;
					second_result = symbol;
				}
			}
		}
		if !first_result.is_invalid() {
			return first_result;
		}
		if !second_result.is_invalid() {
			return second_result;
		}
		third_result
	}

	//==============================================================================
	/// 제약의 오차 변수가 목적함수에 끼친 것을 걷어 낸다.
	//==============================================================================
	fn remove_constraint_effects(&mut self, constraint: &LayoutConstraint, tag: &LayoutTag) {
		let marker_symbol = tag.marker;
		let other_symbol = tag.other;
		if marker_symbol.get_type() == LayoutSymbolType::Error {
			self.remove_marker_effect(marker_symbol, constraint.get_strength());
		}
		if other_symbol.get_type() == LayoutSymbolType::Error {
			self.remove_marker_effect(other_symbol, constraint.get_strength());
		}
	}

	//==============================================================================
	/// 오차 변수 하나가 끼친 것을 걷어 낸다.
	//==============================================================================
	fn remove_marker_effect(&mut self, marker_symbol: LayoutSymbol, strength: f64) {
		match self.rows.get(&marker_symbol) {
			Some(row) => {
				self.objective.insert_row(row, -strength);
			}
			None => {
				self.objective.insert_symbol(marker_symbol, -strength);
			}
		}
	}
}

//==============================================================================
/// 목적함수에서 계수가 음수인 첫 심볼을 들어올 심볼로 고른다. (dummy 는 빼고)
//==============================================================================
fn get_entering_symbol(objective: &LayoutRow) -> LayoutSymbol {
	for (symbol, value) in objective.get_cells().iter() {
		if symbol.get_type() != LayoutSymbolType::Dummy && *value < 0.0 {
			return symbol;
		}
	}
	LayoutSymbol::invalid()
}

//==============================================================================
/// 행이 dummy 심볼만 담았는지.
//==============================================================================
fn all_dummies(row: &LayoutRow) -> bool {
	for (symbol, _) in row.get_cells().iter() {
		if symbol.get_type() != LayoutSymbolType::Dummy {
			return false;
		}
	}
	true
}

//==============================================================================
/// 행에서 축으로 삼을 수 있는 첫 심볼. (slack, error 만)
//==============================================================================
fn any_pivotable_symbol(row: &LayoutRow) -> LayoutSymbol {
	for (symbol, _) in row.get_cells().iter() {
		let symbol_type = symbol.get_type();
		if symbol_type == LayoutSymbolType::Slack || symbol_type == LayoutSymbolType::Error {
			return symbol;
		}
	}
	LayoutSymbol::invalid()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ui::autolayout::layoutpriority::LayoutPriority;

	//==============================================================================
	/// 두 값이 거의 같은지 본다.
	//==============================================================================
	fn assert_near(actual: f64, expected: f64) {
		assert!((actual - expected).abs() < 1e-6, "값 {} 이 {} 가 아니다", actual, expected);
	}

	//==============================================================================
	/// 변수 하나를 식으로.
	//==============================================================================
	fn expression(variable: &LayoutVariable) -> LayoutExpression {
		LayoutExpression::from_variable(variable)
	}

	//==============================================================================
	/// 가로 축만 본 UINode 꼴. (left, right, width, center_x 와 본질 제약 둘)
	//==============================================================================
	struct HorizontalNode {
		left: LayoutVariable,
		right: LayoutVariable,
		width: LayoutVariable,
		center_x: LayoutVariable,
	}

	impl HorizontalNode {
		//==============================================================================
		/// 생성하고 원본 buildIntrinsicConstraints 의 가로 둘(right == left + width, center_x == left + width / 2)을 등록한다.
		//==============================================================================
		fn new(solver: &LayoutSolver) -> HorizontalNode {
			let node = HorizontalNode {
				left: LayoutVariable::new("left"),
				right: LayoutVariable::new("right"),
				width: LayoutVariable::new("width"),
				center_x: LayoutVariable::new("centerX"),
			};
			solver.add_constraint(&expression(&node.right).equal_to(expression(&node.left).add(&node.width))).unwrap();
			solver.add_constraint(&expression(&node.center_x).equal_to(expression(&node.left).add(expression(&node.width).divide(2.0)))).unwrap();
			node
		}

		//==============================================================================
		/// 원본 buildIntrinsicSizeConstraints 의 가로 둘(hug, 압축 저항)을 등록한다.
		//==============================================================================
		fn add_intrinsic_width(&self, solver: &LayoutSolver, intrinsic_width: f64, hugging_priority: f64, compression_priority: f64) {
			let hug = expression(&self.width).less_than_or_equal_to(intrinsic_width).with_strength(LayoutStrength::from_priority(hugging_priority));
			let compression = expression(&self.width).greater_than_or_equal_to(intrinsic_width).with_strength(LayoutStrength::from_priority(compression_priority));
			solver.add_constraint(&hug).unwrap();
			solver.add_constraint(&compression).unwrap();
		}
	}

	//==============================================================================
	/// 원본 머리 주석의 사용 예: x == 10, y == x + 20.
	//==============================================================================
	#[test]
	fn simple_equality() {
		let solver = LayoutSolver::new();
		let x = LayoutVariable::new("x");
		let y = LayoutVariable::new("y");
		solver.add_constraint(&expression(&x).equal_to(10)).unwrap();
		solver.add_constraint(&expression(&y).equal_to(expression(&x).add(20))).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 10.0);
		assert_near(y.get_value(), 30.0);
	}

	//==============================================================================
	/// 부딪히는 제약은 강도가 센 쪽이 이기고, required 부등식은 늘 지킨다.
	//==============================================================================
	#[test]
	fn inequality_with_strengths() {
		let solver = LayoutSolver::new();
		let x = LayoutVariable::new("x");
		solver.add_constraint(&expression(&x).equal_to(40).with_strength(LayoutStrength::WEAK)).unwrap();
		solver.add_constraint(&expression(&x).equal_to(60).with_strength(LayoutStrength::STRONG)).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 60.0);
		solver.add_constraint(&expression(&x).less_than_or_equal_to(30)).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 30.0);
		let y = LayoutVariable::new("y");
		solver.add_constraint(&expression(&y).greater_than_or_equal_to(expression(&x).multiply(2.0))).unwrap();
		solver.add_constraint(&expression(&y).equal_to(0).with_strength(LayoutStrength::MEDIUM)).unwrap();
		solver.update_variables();
		assert_near(y.get_value(), 60.0);
	}

	//==============================================================================
	/// 편집 변수에 바라는 값을 주면 값이 따라가고, required 제약에 막히면 거기서 멈춘다. 지우면 다른 제약대로 돌아간다.
	//==============================================================================
	#[test]
	fn edit_variable_suggest_value() {
		let solver = LayoutSolver::new();
		let x = LayoutVariable::new("x");
		let y = LayoutVariable::new("y");
		solver.add_constraint(&expression(&y).equal_to(expression(&x).multiply(2.0))).unwrap();
		solver.add_constraint(&expression(&x).less_than_or_equal_to(100)).unwrap();
		solver.add_constraint(&expression(&x).equal_to(5).with_strength(LayoutStrength::WEAK)).unwrap();
		solver.add_edit_variable(&x, LayoutStrength::STRONG).unwrap();
		assert!(solver.has_edit_variable(&x));
		solver.suggest_value(&x, 50.0).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 50.0);
		assert_near(y.get_value(), 100.0);
		solver.suggest_value(&x, 10.0).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 10.0);
		assert_near(y.get_value(), 20.0);
		solver.suggest_value(&x, 150.0).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 100.0);
		assert_near(y.get_value(), 200.0);
		solver.remove_edit_variable(&x).unwrap();
		assert!(!solver.has_edit_variable(&x));
		solver.update_variables();
		assert_near(x.get_value(), 5.0);
	}

	//==============================================================================
	/// 이긴 제약을 지우면 앞의 해로 돌아간다.
	//==============================================================================
	#[test]
	fn remove_constraint_restores_previous_solution() {
		let solver = LayoutSolver::new();
		let x = LayoutVariable::new("x");
		let y = LayoutVariable::new("y");
		solver.add_constraint(&expression(&x).equal_to(10).with_strength(LayoutStrength::WEAK)).unwrap();
		solver.add_constraint(&expression(&y).equal_to(expression(&x).add(5))).unwrap();
		let strong = expression(&x).equal_to(50).with_strength(LayoutStrength::STRONG);
		solver.add_constraint(&strong).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 50.0);
		assert_near(y.get_value(), 55.0);
		solver.remove_constraint(&strong).unwrap();
		assert!(!solver.has_constraint(&strong));
		solver.update_variables();
		assert_near(x.get_value(), 10.0);
		assert_near(y.get_value(), 15.0);
		let required = expression(&x).greater_than_or_equal_to(30);
		solver.add_constraint(&required).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 30.0);
		solver.remove_constraint(&required).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 10.0);
	}

	//==============================================================================
	/// 같은 제약을 또 넣으면 오류다. 식이 같아도 따로 만든 제약은 다른 제약이다. (원본 객체 동일성)
	//==============================================================================
	#[test]
	fn duplicate_constraint_error() {
		let solver = LayoutSolver::new();
		let x = LayoutVariable::new("x");
		let constraint = expression(&x).equal_to(10);
		solver.add_constraint(&constraint).unwrap();
		assert!(solver.has_constraint(&constraint));
		assert!(solver.has_constraint(&constraint.clone()));
		assert_eq!(solver.add_constraint(&constraint.clone()), Err(LayoutError::DuplicateConstraint));
		let same_shape = expression(&x).equal_to(10);
		assert!(!solver.has_constraint(&same_shape));
		solver.add_constraint(&same_shape).unwrap();
		solver.update_variables();
		assert_near(x.get_value(), 10.0);
		assert_eq!(LayoutError::DuplicateConstraint.to_string(), "[LayoutSolver] 이미 추가된 제약.");
	}

	//==============================================================================
	/// 만족할 수 없는 required 제약은 오류다. (dummy 만 남는 등식, 인공 변수로도 못 넣는 부등식)
	/// - 인공 변수 길에서 실패하면 원본, Kiwi 처럼 tableau 가 돌려지지 않으므로 그 뒤의 y 값은 보지 않는다.
	//==============================================================================
	#[test]
	fn unsatisfiable_required_constraint_error() {
		let solver = LayoutSolver::new();
		let x = LayoutVariable::new("x");
		solver.add_constraint(&expression(&x).equal_to(10)).unwrap();
		let conflict = expression(&x).equal_to(20);
		assert_eq!(solver.add_constraint(&conflict), Err(LayoutError::UnsatisfiableConstraint));
		assert!(!solver.has_constraint(&conflict));
		let y = LayoutVariable::new("y");
		solver.add_constraint(&expression(&y).greater_than_or_equal_to(10)).unwrap();
		assert_eq!(solver.add_constraint(&expression(&y).less_than_or_equal_to(5)), Err(LayoutError::UnsatisfiableConstraint));
		solver.update_variables();
		assert_near(x.get_value(), 10.0);
		assert_eq!(LayoutError::UnsatisfiableConstraint.to_string(), "[LayoutSolver] 모순 제약: 만족 불가능.");
	}

	//==============================================================================
	/// 모르는 제약, 모르는 편집 변수, 겹친 편집 변수, required 편집 강도는 오류다.
	//==============================================================================
	#[test]
	fn unknown_and_edit_variable_errors() {
		let solver = LayoutSolver::new();
		let x = LayoutVariable::new("x");
		assert_eq!(solver.remove_constraint(&expression(&x).equal_to(1)), Err(LayoutError::UnknownConstraint));
		assert_eq!(solver.suggest_value(&x, 1.0), Err(LayoutError::UnknownEditVariable));
		assert_eq!(solver.remove_edit_variable(&x), Err(LayoutError::UnknownEditVariable));
		assert_eq!(solver.add_edit_variable(&x, LayoutStrength::REQUIRED), Err(LayoutError::BadRequiredStrength));
		assert_eq!(solver.add_edit_variable(&x, 2e9), Err(LayoutError::BadRequiredStrength));
		solver.add_edit_variable(&x, LayoutStrength::MEDIUM).unwrap();
		assert_eq!(solver.add_edit_variable(&x, LayoutStrength::WEAK), Err(LayoutError::DuplicateEditVariable));
	}

	//==============================================================================
	/// 복제한 솔버 핸들은 같은 솔버다. (여러 노드가 나눠 쥔다)
	//==============================================================================
	#[test]
	fn cloned_solver_shares_state() {
		let solver = LayoutSolver::new();
		let shared = solver.clone();
		let x = LayoutVariable::new("x");
		let constraint = expression(&x).equal_to(7);
		shared.add_constraint(&constraint).unwrap();
		assert!(solver.has_constraint(&constraint));
		solver.update_variables();
		assert_near(x.get_value(), 7.0);
	}

	//==============================================================================
	/// UIKit 꼴 배치: 화면(UIScene 처럼 left == 0, 너비는 편집 변수), 여백 10 의 자식, 가운데 맞춘 라벨의 hug, 압축 저항.
	//==============================================================================
	#[test]
	fn uikit_like_layout() {
		let solver = LayoutSolver::new();
		let screen = HorizontalNode::new(&solver);
		solver.add_constraint(&expression(&screen.left).equal_to(0)).unwrap();
		solver.add_edit_variable(&screen.width, LayoutStrength::STRONG).unwrap();
		solver.suggest_value(&screen.width, 400.0).unwrap();

		let child = HorizontalNode::new(&solver);
		solver.add_constraint(&expression(&child.left).equal_to(expression(&screen.left).add(10))).unwrap();
		solver.add_constraint(&expression(&child.width).equal_to(expression(&screen.width).subtract(20))).unwrap();
		solver.add_constraint(&expression(&child.center_x).equal_to(&screen.center_x)).unwrap();

		let label = HorizontalNode::new(&solver);
		solver.add_constraint(&expression(&label.center_x).equal_to(&screen.center_x)).unwrap();
		label.add_intrinsic_width(&solver, 120.0, LayoutPriority::DEFAULT_LOW, LayoutPriority::DEFAULT_HIGH);
		let narrow = expression(&label.width).equal_to(80).with_strength(LayoutStrength::from_priority(500.0));
		solver.add_constraint(&narrow).unwrap();
		solver.update_variables();

		assert_near(screen.right.get_value(), 400.0);
		assert_near(child.left.get_value(), 10.0);
		assert_near(child.width.get_value(), 380.0);
		assert_near(child.right.get_value(), 390.0);
		assert_near(child.center_x.get_value(), 200.0);
		// 압축 저항(750)이 좁히려는 제약(500)을 이긴다.
		assert_near(label.width.get_value(), 120.0);
		assert_near(label.left.get_value(), 140.0);

		// 넓히려는 제약(500)은 hug(250)를 이긴다.
		solver.remove_constraint(&narrow).unwrap();
		solver.add_constraint(&expression(&label.width).equal_to(200).with_strength(LayoutStrength::from_priority(500.0))).unwrap();
		solver.update_variables();
		assert_near(label.width.get_value(), 200.0);
		assert_near(label.left.get_value(), 100.0);

		// 화면 크기가 바뀌면 다시 놓인다. (UIScene 의 suggestValue)
		solver.suggest_value(&screen.width, 600.0).unwrap();
		solver.update_variables();
		assert_near(child.width.get_value(), 580.0);
		assert_near(child.right.get_value(), 590.0);
		assert_near(label.left.get_value(), 200.0);
		assert_near(label.right.get_value(), 400.0);
	}

	//==============================================================================
	/// 나란한 두 라벨이 자리보다 크면 압축 저항이 낮은 쪽이 줄고, 자리가 남으면 hug 가 낮은 쪽이 는다.
	//==============================================================================
	#[test]
	fn compression_and_hugging_choose_lower_priority() {
		let solver = LayoutSolver::new();
		let first = HorizontalNode::new(&solver);
		let second = HorizontalNode::new(&solver);
		solver.add_constraint(&expression(&first.left).equal_to(0)).unwrap();
		solver.add_constraint(&expression(&second.left).equal_to(&first.right)).unwrap();
		solver.add_constraint(&expression(&second.right).equal_to(200)).unwrap();
		first.add_intrinsic_width(&solver, 120.0, LayoutPriority::DEFAULT_LOW, LayoutPriority::DEFAULT_HIGH);
		second.add_intrinsic_width(&solver, 120.0, LayoutPriority::DEFAULT_LOW, LayoutPriority::DEFAULT_HIGH + 10.0);
		solver.update_variables();
		assert_near(first.width.get_value(), 80.0);
		assert_near(second.width.get_value(), 120.0);
		assert_near(second.left.get_value(), 80.0);

		let roomy_solver = LayoutSolver::new();
		let first = HorizontalNode::new(&roomy_solver);
		let second = HorizontalNode::new(&roomy_solver);
		roomy_solver.add_constraint(&expression(&first.left).equal_to(0)).unwrap();
		roomy_solver.add_constraint(&expression(&second.left).equal_to(&first.right)).unwrap();
		roomy_solver.add_constraint(&expression(&second.right).equal_to(200)).unwrap();
		first.add_intrinsic_width(&roomy_solver, 50.0, LayoutPriority::DEFAULT_LOW, LayoutPriority::DEFAULT_HIGH);
		second.add_intrinsic_width(&roomy_solver, 50.0, LayoutPriority::DEFAULT_LOW + 1.0, LayoutPriority::DEFAULT_HIGH);
		roomy_solver.update_variables();
		assert_near(first.width.get_value(), 150.0);
		assert_near(second.width.get_value(), 50.0);
	}

	//==============================================================================
	/// 원본 UIScene 의 safeAreaLayoutGuide 다시 짓기: 제약 넷을 지우고 새 여백으로 다시 넣는다.
	//==============================================================================
	#[test]
	fn rebuild_guide_constraints() {
		let solver = LayoutSolver::new();
		let screen = HorizontalNode::new(&solver);
		solver.add_constraint(&expression(&screen.left).equal_to(0)).unwrap();
		solver.add_edit_variable(&screen.width, LayoutStrength::STRONG).unwrap();
		solver.suggest_value(&screen.width, 300.0).unwrap();
		let guide = HorizontalNode::new(&solver);
		let mut guide_constraints: Vec<LayoutConstraint> = Vec::new();
		for (left_inset, right_inset) in [(0.0, 0.0), (20.0, 30.0), (5.0, 0.0)] {
			for constraint in &guide_constraints {
				if solver.has_constraint(constraint) {
					solver.remove_constraint(constraint).unwrap();
				}
			}
			guide_constraints = vec![
				expression(&guide.left).equal_to(expression(&screen.left).add(left_inset)),
				expression(&guide.right).equal_to(expression(&screen.right).subtract(right_inset)),
			];
			for constraint in &guide_constraints {
				solver.add_constraint(constraint).unwrap();
			}
			solver.update_variables();
			assert_near(guide.left.get_value(), left_inset);
			assert_near(guide.width.get_value(), 300.0 - left_inset - right_inset);
		}
	}
}
