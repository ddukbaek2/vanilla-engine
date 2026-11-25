//==============================================================================
// 레이아웃 솔버 미지수. (원본 ui/autolayout/layoutvariable.js 의 LayoutVariable)
// - 사용자 코드가 식에 쓰는 변수다. 솔버가 풀고 나서 update_variables 를 부르면 set_value 로 결과가 들어온다.
// - 원본과 다른 것:
//   - 원본은 객체 하나를 여러 곳(노드, 식, 솔버)이 같이 가리킨다. 러스트 판은 Rc 를 품은 값싼 Clone 핸들이라
//     복제한 핸들끼리 같은 이름과 값을 본다. 같음과 해시는 원본의 객체 동일성처럼 핸들이 가리키는 변수로 가린다.
//   - 원본에 없는 get_id 로 변수마다 붙는 고유 번호를 읽는다.
//   - 이름과 값은 RefCell, Cell 에 두어 &self 로 바꾼다.
//==============================================================================
use std::cell::{Cell, RefCell};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

// 변수마다 붙이는 고유 번호의 다음 값.
static NEXT_VARIABLE_ID: AtomicU64 = AtomicU64::new(1);

//==============================================================================
// 핸들이 같이 가리키는 변수 본체.
//==============================================================================
struct LayoutVariableData {
	id: u64,
	name: RefCell<String>,
	value: Cell<f64>,
}

//==============================================================================
// 레이아웃 솔버 미지수. (복제해도 같은 변수를 가리킨다)
//==============================================================================
#[derive(Clone)]
pub struct LayoutVariable {
	data: Rc<LayoutVariableData>,
}

impl LayoutVariable {
	//==============================================================================
	// 생성. (값은 0)
	//==============================================================================
	pub fn new(name: &str) -> LayoutVariable {
		let id = NEXT_VARIABLE_ID.fetch_add(1, Ordering::Relaxed);
		LayoutVariable {
			data: Rc::new(LayoutVariableData {
				id,
				name: RefCell::new(name.to_string()),
				value: Cell::new(0.0),
			}),
		}
	}

	//==============================================================================
	// 고유 번호 반환.
	//==============================================================================
	pub fn get_id(&self) -> u64 {
		self.data.id
	}

	//==============================================================================
	// 이름 설정.
	//==============================================================================
	pub fn set_name(&self, name: &str) {
		*self.data.name.borrow_mut() = name.to_string();
	}

	//==============================================================================
	// 이름 반환.
	//==============================================================================
	pub fn get_name(&self) -> String {
		self.data.name.borrow().clone()
	}

	//==============================================================================
	// 값 설정. (솔버가 부르며 바깥에서 직접 부르는 것은 권하지 않는다)
	//==============================================================================
	pub fn set_value(&self, value: f64) {
		self.data.value.set(value);
	}

	//==============================================================================
	// 현재 값 반환. (update_variables 뒤에 맞다)
	//==============================================================================
	pub fn get_value(&self) -> f64 {
		self.data.value.get()
	}
}

impl PartialEq for LayoutVariable {
	//==============================================================================
	// 같은 변수를 가리키는지.
	//==============================================================================
	fn eq(&self, other: &LayoutVariable) -> bool {
		Rc::ptr_eq(&self.data, &other.data)
	}
}

impl Eq for LayoutVariable {
}

impl Hash for LayoutVariable {
	//==============================================================================
	// 고유 번호로 해시한다.
	//==============================================================================
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.data.id.hash(state);
	}
}

impl fmt::Debug for LayoutVariable {
	//==============================================================================
	// 이름, 번호, 값을 보인다.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.debug_struct("LayoutVariable").field("id", &self.data.id).field("name", &*self.data.name.borrow()).field("value", &self.data.value.get()).finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 복제한 핸들은 같은 변수다. 이름이 같아도 따로 만든 변수는 다르다.
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
