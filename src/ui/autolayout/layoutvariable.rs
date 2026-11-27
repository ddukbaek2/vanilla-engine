//==============================================================================
//! 레이아웃 솔버 미지수. (원본 ui/autolayout/layoutvariable.js 의 LayoutVariable)
//! - 사용자 코드가 식에 쓰는 변수다. 솔버가 풀고 나서 update_variables 를 부르면 set_value 로 결과가 들어온다.
//! - 원본과 다른 것:
//!   - 원본은 객체 하나를 여러 곳(노드, 식, 솔버)이 같이 가리킨다. 러스트 판은 Rc 를 품은 값싼 Clone 핸들이라
//!     복제한 핸들끼리 같은 이름과 값을 본다. 같음과 해시는 원본의 객체 동일성처럼 핸들이 가리키는 변수로 가린다.
//!   - 원본에 없는 get_id 로 변수마다 붙는 고유 번호를 읽는다.
//!   - 이름과 값은 RefCell, Cell 에 두어 &self 로 바꾼다.
//==============================================================================
use std::cell::{Cell, RefCell};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 변수마다 붙이는 고유 번호의 다음 값.
static NEXT_VARIABLE_ID: AtomicU64 = AtomicU64::new(1);

//==============================================================================
/// 핸들이 같이 가리키는 변수 본체.
//==============================================================================
struct LayoutVariableData {
	id: u64,
	name: RefCell<String>,
	value: Cell<f64>,
	value_changed_event: RefCell<Option<LayoutVariableEvent>>,
}

/// 값이 바뀐 변수를 받는 함수. (원본 setValueChangedEvent 의 함수)
pub type LayoutVariableEvent = Rc<dyn Fn(&LayoutVariable)>;

//==============================================================================
/// 레이아웃 솔버 미지수. (복제해도 같은 변수를 가리킨다)
///
/// # 예제
/// 솔버가 푼 값을 읽기, 복제한 핸들도 같은 값을 본다
/// ```rust
/// use vanilla::{LayoutExpression, LayoutSolver, LayoutVariable};
///
/// let solver = LayoutSolver::new();
/// let left = LayoutVariable::new("left");
/// let width = LayoutVariable::new("width");
/// let right = LayoutVariable::new("right");
/// solver.add_constraint(&LayoutExpression::from_variable(&left).equal_to(16)).unwrap();
/// solver.add_constraint(&LayoutExpression::from_variable(&width).equal_to(200)).unwrap();
/// solver.add_constraint(&LayoutExpression::from_variable(&right).equal_to(LayoutExpression::from_variable(&left).add(&width))).unwrap();
/// solver.update_variables();
/// assert!((right.get_value() - 216.0).abs() < 1e-9);
///
/// let shared = right.clone();
/// assert_eq!(shared, right);
/// assert_eq!(shared.get_id(), right.get_id());
/// assert_eq!(shared.get_value(), right.get_value());
/// ```
//==============================================================================
#[derive(Clone)]
pub struct LayoutVariable {
	data: Rc<LayoutVariableData>,
}

impl LayoutVariable {
	//==============================================================================
	/// 생성. (값은 0)
	///
	/// # 인자
	/// - `name`: 이름. (보이기용, 이름이 같아도 따로 만든 변수는 다르다)
	///
	/// # 돌려줌
	/// 새 고유 번호를 붙인 새 변수 핸들.
	//==============================================================================
	pub fn new(name: &str) -> LayoutVariable {
		let id = NEXT_VARIABLE_ID.fetch_add(1, Ordering::Relaxed);
		LayoutVariable {
			data: Rc::new(LayoutVariableData {
				id,
				name: RefCell::new(name.to_string()),
				value: Cell::new(0.0),
				value_changed_event: RefCell::new(None),
			}),
		}
	}

	//==============================================================================
	/// 고유 번호 반환.
	///
	/// # 돌려줌
	/// 고유 번호. (1 부터 만든 차례로 하나씩 늘어난다, 복제한 핸들은 같은 번호)
	//==============================================================================
	pub fn get_id(&self) -> u64 {
		self.data.id
	}

	//==============================================================================
	/// 이름 설정.
	///
	/// # 인자
	/// - `name`: 새 이름. (복제한 핸들 모두에 보인다)
	//==============================================================================
	pub fn set_name(&self, name: &str) {
		*self.data.name.borrow_mut() = name.to_string();
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 이름의 사본.
	//==============================================================================
	pub fn get_name(&self) -> String {
		self.data.name.borrow().clone()
	}

	//==============================================================================
	/// 값 설정. (솔버가 부르며 바깥에서 직접 부르는 것은 권하지 않는다)
	///
	/// # 인자
	/// - `value`: 넣을 값.
	//==============================================================================
	pub fn set_value(&self, value: f64) {
		if self.data.value.get() == value {
			return;
		}
		self.data.value.set(value);
		let value_changed_event = self.data.value_changed_event.borrow().clone();
		if let Some(value_changed_event) = value_changed_event {
			value_changed_event(self);
		}
	}

	//==============================================================================
	/// 값이 바뀔 때 부를 함수 설정. (원본 setValueChangedEvent, 솔버가 푼 값이 달라질 때만 부른다, None 이면 부르지 않는다)
	/// - 변수를 가진 쪽이 바뀐 것을 알아챈다. (원본 UINode 는 자리와 크기 변수에 걸어 하위 트리 그리기 경계를 다시 셈하게 한다. 러스트 판 UINode 는 노드 트랜스폼에 풀어 넣으므로 트랜스폼 세터가 알린다)
	///
	/// # 인자
	/// - `value_changed_event`: 바뀐 변수를 받는 함수. None 이면 지운다.
	///
	/// # 예제
	/// 값이 바뀔 때 알기
	/// ```rust
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	/// use vanilla::LayoutVariable;
	///
	/// let width = LayoutVariable::new("width");
	/// let changed_count = Rc::new(Cell::new(0));
	/// let counter = changed_count.clone();
	/// width.set_value_changed_event(Some(Rc::new(move |variable: &LayoutVariable| {
	/// 	assert_eq!(variable.get_value(), 120.0);
	/// 	counter.set(counter.get() + 1);
	/// })));
	/// width.set_value(120.0);
	/// width.set_value(120.0);
	/// assert_eq!(changed_count.get(), 1);
	/// assert!(width.get_value_changed_event().is_some());
	/// ```
	//==============================================================================
	pub fn set_value_changed_event(&self, value_changed_event: Option<LayoutVariableEvent>) {
		*self.data.value_changed_event.borrow_mut() = value_changed_event;
	}

	//==============================================================================
	/// 값이 바뀔 때 부를 함수 반환. (원본 getValueChangedEvent)
	///
	/// # 돌려줌
	/// 건 함수. 없으면 None.
	//==============================================================================
	pub fn get_value_changed_event(&self) -> Option<LayoutVariableEvent> {
		self.data.value_changed_event.borrow().clone()
	}

	//==============================================================================
	/// 현재 값 반환. (update_variables 뒤에 맞다)
	///
	/// # 돌려줌
	/// 지금 값. (처음은 0)
	//==============================================================================
	pub fn get_value(&self) -> f64 {
		self.data.value.get()
	}
}

impl PartialEq for LayoutVariable {
	//==============================================================================
	/// 같은 변수를 가리키는지.
	//==============================================================================
	fn eq(&self, other: &LayoutVariable) -> bool {
		Rc::ptr_eq(&self.data, &other.data)
	}
}

impl Eq for LayoutVariable {
}

impl Hash for LayoutVariable {
	//==============================================================================
	/// 고유 번호로 해시한다.
	//==============================================================================
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.data.id.hash(state);
	}
}

impl fmt::Debug for LayoutVariable {
	//==============================================================================
	/// 이름, 번호, 값을 보인다.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.debug_struct("LayoutVariable").field("id", &self.data.id).field("name", &*self.data.name.borrow()).field("value", &self.data.value.get()).finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 복제한 핸들은 같은 변수다. 이름이 같아도 따로 만든 변수는 다르다.
	//==============================================================================
	#[test]
	fn clone_shares_identity_and_value() {
		let variable = LayoutVariable::new("width");
		let copied = variable.clone();
		copied.set_value(12.5);
		copied.set_name("height");
		assert_eq!(variable.get_value(), 12.5);
		assert_eq!(variable.get_name(), "height");
		assert_eq!(variable, copied);
		assert_eq!(variable.get_id(), copied.get_id());
		let other = LayoutVariable::new("height");
		assert_ne!(variable, other);
		assert_eq!(other.get_value(), 0.0);
	}
}
