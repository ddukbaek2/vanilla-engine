//==============================================================================
//! 2D 물리의 사용자 값. (몸, 모양, 관절, 그 정의에 붙이는 아무 값)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본의 userData(아무 값, 기본 null)와 같은 뜻이다.
//! - 값은 `Rc<dyn Any>` 로 나눠 든다. 복제하면 같은 값을 가리키고, 견줌(==)은 같은 값을 가리키는지(Rc::ptr_eq)다.
//!   원본 객체 참조의 === 와 같은 뜻이다. 비어 있는 둘은 같다(원본 null === null).
//! - 원본과 다른 점: 원본은 수, 글 같은 원시 값도 === 로 값을 견주지만, 러스트는 감싼 값마다 따로 견준다. 같은 수를 두 번 감싸면 다르다.
//==============================================================================
use std::any::Any;
use std::fmt;
use std::rc::Rc;

//==============================================================================
/// 2D 물리의 사용자 값.
///
/// # 예제
/// 몸에 사용자 값을 붙이고 다시 꺼내기
/// ```
/// use vanilla::experimental::physics2d::{BodyDefinition2D, PhysicsUserData2D, PhysicsWorld2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(50.0));
/// let mut body_definition = BodyDefinition2D::new();
/// body_definition.user_data = PhysicsUserData2D::new(String::from("player"));
/// let body_id = world.create_body(&body_definition);
/// let user_data = world.get_body(body_id).expect("body").get_user_data();
/// assert_eq!(user_data.get::<String>().map(String::as_str), Some("player"));
/// assert!(*user_data == body_definition.user_data);
/// ```
//==============================================================================
#[derive(Clone, Default)]
pub struct PhysicsUserData2D {
	value: Option<Rc<dyn Any>>,
}

impl PhysicsUserData2D {
	//==============================================================================
	/// 값을 감싸 만들기.
	///
	/// # 인자
	/// - `value`: 붙일 값.
	///
	/// # 돌려줌
	/// 그 값을 가리키는 사용자 값.
	//==============================================================================
	pub fn new<T: Any>(value: T) -> PhysicsUserData2D {
		PhysicsUserData2D {
			value: Some(Rc::new(value)),
		}
	}

	//==============================================================================
	/// 이미 나눠 든 값으로 만들기.
	///
	/// # 인자
	/// - `value`: 나눠 든 값. (같은 Rc 로 만든 사용자 값끼리는 같다)
	///
	/// # 돌려줌
	/// 그 값을 가리키는 사용자 값.
	//==============================================================================
	pub fn from_rc(value: Rc<dyn Any>) -> PhysicsUserData2D {
		PhysicsUserData2D {
			value: Some(value),
		}
	}

	//==============================================================================
	/// 비었는지. (원본 null)
	///
	/// # 돌려줌
	/// 값이 없으면 true.
	//==============================================================================
	pub fn is_none(&self) -> bool {
		self.value.is_none()
	}

	//==============================================================================
	/// 값을 그 꼴로 읽기.
	///
	/// # 돌려줌
	/// 값이 있고 꼴이 T 면 그 값, 아니면 None.
	//==============================================================================
	pub fn get<T: Any>(&self) -> Option<&T> {
		self.value.as_ref().and_then(|value| value.downcast_ref::<T>())
	}

	//==============================================================================
	/// 나눠 든 값 반환.
	///
	/// # 돌려줌
	/// 값이 있으면 그 Rc 의 복제, 없으면 None.
	//==============================================================================
	pub fn get_rc(&self) -> Option<Rc<dyn Any>> {
		self.value.clone()
	}
}

impl PartialEq for PhysicsUserData2D {
	//==============================================================================
	/// 둘 다 비었거나 같은 값(Rc 가 같은 자리)을 가리키면 같다. (원본의 === 와 같다)
	//==============================================================================
	fn eq(&self, other: &PhysicsUserData2D) -> bool {
		match (&self.value, &other.value) {
			(None, None) => true,
			(Some(left), Some(right)) => Rc::ptr_eq(left, right),
			_ => false,
		}
	}
}

impl fmt::Debug for PhysicsUserData2D {
	//==============================================================================
	/// 비었으면 PhysicsUserData2D(null), 값이 있으면 그 자리 주소를 적는다.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match &self.value {
			None => formatter.write_str("PhysicsUserData2D(null)"),
			Some(value) => write!(formatter, "PhysicsUserData2D({:p})", Rc::as_ptr(value)),
		}
	}
}
