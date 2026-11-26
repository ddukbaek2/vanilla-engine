//==============================================================================
//! 반사. (원본 vanilla.js 의 실행 중 반사 자리: ui/uidocument.js 의 collectReflectedProperties 와 getReflectedProperties,
//!   misc/devtools.js 의 getComponentProperties)
//! - 원본은 자바스크립트라 실행 중에 객체의 프로토타입을 훑어 get, is, set 함수를 찾는다. 러스트에는 실행 중 반사가 없어,
//!   언리얼의 UHT(UPROPERTY), Qt 의 moc 처럼 컴파일 때 코드를 지어 같은 일을 한다. 러스트에서는 절차 매크로 `#[reflect]` 가 그 일을 한다.
//!   (macros/ 크레이트, 엔진이 vanilla::reflect 로 다시 내보낸다) C++ RTTI 의 typeid, dynamic_cast 자리는 이미 AsAny(std::any::Any)다.
//! - 이 모듈은 매크로가 지은 코드가 쓰는 값 꼴(ReflectValue)과 트레이트(Reflect), 원본 Component 기반 클래스의 공통 속성을 둔다.
//! - 쓰는 법: 컴포넌트의 impl 블록과 impl Component 블록 둘 다에 `#[vanilla::reflect]` 를 붙인다.
//!   ```text
//!     #[vanilla::reflect]
//!     impl Spin {
//!         pub fn get_speed(&self) -> f32 { self.speed }
//!         pub fn set_speed(&mut self, speed: f32) { self.speed = speed; }
//!     }
//!     #[vanilla::reflect]
//!     impl Component for Spin { ... }
//!   ```
//!   그러면 UIDocument 가 등록된 쓰기, 읽기 함수 없이 Speed 키로 쓰고 읽고(원본 saveByReflection, loadByReflection),
//!   DevTools 인스펙터가 speed 줄로 보인다. (원본 getComponentProperties)
//! - 원본은 모든 객체가 반사된다. 러스트 판은 Reflect 를 Component 의 상위 트레이트로 두어 반사 없는 컴포넌트가 생길 수 없다.
//!   어느 쪽을 빠뜨려도 컴파일 에러가 붙일 곳을 알려 준다. 게터가 없는 타입은 빈 블록 `#[vanilla::reflect] impl Spin {}` 에 붙인다.
//!   반사를 쓰는 쪽은 `&dyn Component` 를 `&dyn Reflect` 로 올려(트레이트 업캐스팅) 부른다.
//! - 원본 규칙을 따른다:
//!   - 게터는 인자 없는 pub get_*, is_* 함수, 저장 속성은 같은 이름의 set_* 이 있는 게터다. 키는 접두어를 뗀 원본 이름이다. (get_line_width → LineWidth)
//!   - 차례는 정의 차례이고 같은 키는 먼저 나온 것이다. 원본 Component 기반 클래스의 GizmoVisible, Enable, ComponentType 이 끝에 붙는다.
//!   - 저장하는 값은 원본 saveByReflection 이 가리는 꼴(수, 불, 글, Color, Vector2, 노드)이다. 원본에서 글인 열거(TextAlign 같은 것)는 같은 글로 바꾼다.
//! - 원본처럼 하는 것:
//!   - 원본 상위 클래스의 게터는 프로토타입 사슬로 따라온다. 러스트 판은 상속이 없어 품은 값을 #[reflect(base = 필드)] 로 잇는다.
//!     원본에서 상위 클래스 몫인 함수를 러스트 판이 편의로 다시 둔 것은 `#[reflect(skip)]` 로 빼서 차례를 원본처럼 둔다.
//!     읽기와 넣기는 원본 `component[getterName]`, `component[setterName]` 처럼 따로 가장 아래 타입의 것부터 찾는다. (RichText 는 게터를 Text 에서 받고 세터는 제 것)
//!   - 짝은 원본처럼 키(baseName)로 짓는다. (get_target 의 키를 TargetNode 로 정하면 set_target(키 Target)과 짝이 아니다)
//!   - impl Component 블록에서 다시 정의한 게터는 원본 클래스에도 있는 것만 보이고(`#[reflect(skip)]` 으로 빼고), 원본 클래스 몸통의 자리는 #[reflect(after = "...")] 로 맞춘다.
//!   - 원본에서 값이 객체라 저장에서 빠지는 속성은 `#[reflect(unsaved)]` 로 목록에는 두고 저장에서 뺀다. (ShadowText 의 FontFace)
//!   - 읽을 때 꼴은 세터의 꼴로 가린다. 원본은 지금 값의 꼴로 가리고 지금 값이 없으면(null) 저장한 꼴로 받는다. (원본 saveByReflection 은 Color 만 수 넷,
//!     Vector2 만 수 둘로 적는다) 그래서 두 엔진이 같은 문서를 같게 읽는다.
//! - 원본과 다른 것:
//!   - 원본은 모든 객체가 반사된다. 러스트 판은 `#[reflect]` 를 붙인 타입만이다. 러스트에는 실행 중에 메서드 목록을 읽는 길이 없어 정말 방법이 없다.
//!     (언리얼 UCLASS, UPROPERTY 와 같은 꼴) 빠뜨린 것은 이렇게 알 수 있다: 한쪽 블록에만 붙이면 컴파일 에러가 붙일 곳을 알려 주고,
//!     둘 다 빠뜨리면 UIDocument 가 쓸 때 경고를 내고 DevTools 인스펙터가 "반사 없음" 줄을 보인다.
//!   - &mut self 게터는 &mut 를 받는 갈래(get_reflected_value_mut, collect_reflected_getters_mut)로 부른다. &self 갈래에서는 값이 None 이다.
//!     트리를 받는 게터(get_target_button(트리) 같은 것, 원본은 이웃 컴포넌트를 바로 쥔다)는 트리를 쥔 쪽이 collect_reflected_getters_in_tree 로 부른다.
//!     트리를 받는 세터(set_snap_current_index(트리, 칸) 같은 것)는 트리를 쥔 쪽이 set_reflected_value_in_tree 로 부른다.
//!     값 말고도 인자를 더 받는 세터(set_image(그림, 조각) 같은 것)는 속성 목록에는 들지만(원본처럼 set 함수가 있으면 짝이다) 넣지 않는다.
//!     원본 setComponentType 자리는 러스트에 세터가 없어(종류는 타입이 정한다) 쓰기만 한다. 원본도 이 키는 읽어도 종류가 바뀌지 않는다.
//!
//! # 예제
//! impl 블록과 impl Component 블록에 반사 매크로를 붙이고 속성을 읽고 넣기
//! ```rust
//! use vanilla::core::reflection::{collect_reflected_properties, read_reflected_property, write_reflected_property};
//! use vanilla::{reflect, Component, ComponentBase, ReflectValue};
//!
//! struct Spin {
//! 	component_base: ComponentBase,
//! 	speed: f32,
//! }
//!
//! #[reflect]
//! impl Spin {
//! 	pub fn get_speed(&self) -> f32 {
//! 		self.speed
//! 	}
//!
//! 	pub fn set_speed(&mut self, speed: f32) {
//! 		self.speed = speed;
//! 	}
//! }
//!
//! #[reflect]
//! impl Component for Spin {
//! 	fn get_component_base(&self) -> &ComponentBase {
//! 		&self.component_base
//! 	}
//!
//! 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
//! 		&mut self.component_base
//! 	}
//!
//! 	fn get_component_type(&self) -> &'static str {
//! 		"Spin"
//! 	}
//! }
//!
//! let mut spin = Spin {
//! 	component_base: ComponentBase::new(),
//! 	speed: 1.5,
//! };
//! let property_names: Vec<&str> = collect_reflected_properties(&spin).iter().map(|property| property.name).collect();
//! assert_eq!(property_names, ["Speed", "GizmoVisible", "Enable", "ComponentType"]);
//! assert_eq!(read_reflected_property(&spin, "Speed"), Some(ReflectValue::Number(1.5)));
//! assert!(write_reflected_property(&mut spin, "Speed", &ReflectValue::Number(3.0)));
//! assert_eq!(spin.get_speed(), 3.0);
//! ```
//!
//! 함수 옵션(name, transient, skip)과 품은 값(base)
//! ```rust
//! use vanilla::core::reflection::{collect_reflected_getters, collect_reflected_properties};
//! use vanilla::{reflect, Component, ComponentBase};
//!
//! struct Meter {
//! 	component_base: ComponentBase,
//! 	value: f32,
//! }
//!
//! #[reflect]
//! impl Meter {
//! 	pub fn get_value(&self) -> f32 {
//! 		self.value
//! 	}
//!
//! 	pub fn set_value(&mut self, value: f32) {
//! 		self.value = value;
//! 	}
//! }
//!
//! #[reflect]
//! impl Component for Meter {
//! 	fn get_component_base(&self) -> &ComponentBase {
//! 		&self.component_base
//! 	}
//!
//! 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
//! 		&mut self.component_base
//! 	}
//!
//! 	fn get_component_type(&self) -> &'static str {
//! 		"Meter"
//! 	}
//! }
//!
//! struct LabeledMeter {
//! 	component_base: ComponentBase,
//! 	meter: Meter,
//! 	label: String,
//! }
//!
//! #[reflect(base = meter)]
//! impl LabeledMeter {
//! 	#[reflect(name = "Caption")]
//! 	pub fn get_label(&self) -> &str {
//! 		&self.label
//! 	}
//!
//! 	#[reflect(name = "Caption")]
//! 	pub fn set_label(&mut self, label: &str) {
//! 		self.label = String::from(label);
//! 	}
//!
//! 	#[reflect(transient)]
//! 	pub fn get_label_length(&self) -> usize {
//! 		self.label.len()
//! 	}
//!
//! 	#[reflect(skip)]
//! 	pub fn get_meter(&self) -> &Meter {
//! 		&self.meter
//! 	}
//! }
//!
//! #[reflect]
//! impl Component for LabeledMeter {
//! 	fn get_component_base(&self) -> &ComponentBase {
//! 		&self.component_base
//! 	}
//!
//! 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
//! 		&mut self.component_base
//! 	}
//!
//! 	fn get_component_type(&self) -> &'static str {
//! 		"LabeledMeter"
//! 	}
//! }
//!
//! let labeled_meter = LabeledMeter {
//! 	component_base: ComponentBase::new(),
//! 	meter: Meter {
//! 		component_base: ComponentBase::new(),
//! 		value: 0.5,
//! 	},
//! 	label: String::from("hp"),
//! };
//! let property_names: Vec<&str> = collect_reflected_properties(&labeled_meter).iter().map(|property| property.name).collect();
//! assert_eq!(property_names, ["Caption", "Value", "GizmoVisible", "Enable", "ComponentType"]);
//! let getter_names: Vec<&str> = collect_reflected_getters(&labeled_meter).iter().map(|getter| getter.name).collect();
//! assert_eq!(getter_names, ["getCaption", "getLabelLength", "getValue"]);
//! ```
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::Component;
use crate::core::node::nodetree::{NodeId, NodeTree};
use std::marker::PhantomData;

/// 감싼 꼴의 앞머리. (인스펙터의 종류 이름은 감싼 것을 벗긴 안의 이름이다. `Rc<RefCell<Image>>` → Image)
const WRAPPER_TYPE_PREFIXES: [&str; 5] = ["Rc<", "Arc<", "Box<", "RefCell<", "Cell<"];

//==============================================================================
/// 반사 값. (원본 게터가 돌려주는 자바스크립트 값 자리)
/// - Null 은 원본 null(러스트 None), List 는 문서에서 읽은 배열, Object 는 옮길 수 없는 값의 종류 이름이다. (인스펙터의 `[이름]`)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum ReflectValue {
	/// 값 없음. (원본 null, 러스트 None)
	Null,
	/// 불.
	Boolean(bool),
	/// 수. (f32 와 정수 꼴도 이것으로 바꾼다)
	Number(f64),
	/// 글. (원본에서 글인 열거도 이것)
	String(String),
	/// 2차원 벡터.
	Vector2(Vector2),
	/// 색.
	Color(Color),
	/// 사각형.
	Rect(Rect),
	/// 노드 핸들.
	Node(NodeId),
	/// 문서에서 읽은 배열. (Vector2 는 수 둘, Color 는 수 넷으로 받는다)
	List(Vec<ReflectValue>),
	/// 옮길 수 없는 값의 종류 이름. (인스펙터의 `[이름]`)
	Object(String),
}

//==============================================================================
/// 반사 값으로 바꿀 수 있는 꼴.
///
/// # 예제
/// 기본 꼴을 반사 값으로
/// ```rust
/// use vanilla::{Color, ReflectValue, ToReflectValue, Vector2};
///
/// assert_eq!(0.25f32.to_reflect_value(), ReflectValue::Number(0.25));
/// assert_eq!("left".to_reflect_value(), ReflectValue::String(String::from("left")));
/// assert_eq!(Some(Vector2::create(1.0, 2.0)).to_reflect_value(), ReflectValue::Vector2(Vector2::create(1.0, 2.0)));
/// let empty_color: Option<Color> = None;
/// assert_eq!(empty_color.to_reflect_value(), ReflectValue::Null);
/// ```
//==============================================================================
pub trait ToReflectValue {
	//==============================================================================
	/// 반사 값으로 바꾸기.
	///
	/// # 돌려줌
	/// 이 값의 반사 값.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue;
}

//==============================================================================
/// 반사 값에서 읽을 수 있는 꼴. (꼴이 맞지 않으면 None, 원본 loadByReflection 의 typeof 가 같은지 자리)
///
/// # 예제
/// 문서에서 읽은 배열을 꼴에 맞춰 읽기
/// ```rust
/// use vanilla::{Color, FromReflectValue, ReflectValue, Vector2};
///
/// let list_value = ReflectValue::List(vec![ReflectValue::Number(1.0), ReflectValue::Number(2.0)]);
/// assert_eq!(Vector2::from_reflect_value(&list_value), Some(Vector2::create(1.0, 2.0)));
/// assert_eq!(Color::from_reflect_value(&list_value), None);
/// assert_eq!(Option::<f32>::from_reflect_value(&ReflectValue::Null), Some(None));
/// assert_eq!(f32::from_reflect_value(&ReflectValue::Boolean(true)), None);
/// ```
//==============================================================================
pub trait FromReflectValue<'a>: Sized {
	//==============================================================================
	/// 반사 값에서 읽기.
	///
	/// # 인자
	/// - `value`: 읽을 반사 값.
	///
	/// # 돌려줌
	/// 읽은 값. 꼴이 맞지 않으면 None.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<Self>;
}

//==============================================================================
/// 저장 속성 하나. (원본 getReflectedProperties 가 돌려주는 { name, getterName, setterName })
/// - name 은 원본 키(접두어를 뗀 원본 이름), getter_name 과 setter_name 은 러스트 함수 이름이다. setter_name 이 None 이면 러스트에 세터가 없다.
/// - is_saved 가 false 면 원본에서 값이 객체라 반사 저장이 적지 않는 속성이다. (`#[reflect(unsaved)]`, ShadowText 의 FontFace)
///
/// # 예제
/// Component 공통 속성 하나 보기
/// ```rust
/// use vanilla::core::reflection::COMPONENT_REFLECTED_PROPERTIES;
///
/// let enable_property = COMPONENT_REFLECTED_PROPERTIES[1];
/// assert_eq!(enable_property.name, "Enable");
/// assert_eq!(enable_property.getter_name, "is_enable");
/// assert_eq!(enable_property.setter_name, Some("set_enable"));
/// assert!(enable_property.is_saved);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReflectedProperty {
	/// 원본 키. (접두어를 뗀 원본 이름, LineWidth)
	pub name: &'static str,
	/// 러스트 게터 이름. (get_line_width)
	pub getter_name: &'static str,
	/// 러스트 세터 이름. 러스트에 세터가 없으면 None.
	pub setter_name: Option<&'static str>,
	/// 반사 저장이 적는 속성이면 true. (`#[reflect(unsaved)]` 면 false)
	pub is_saved: bool,
}

//==============================================================================
/// 게터 하나와 그 값. (원본 getComponentProperties 가 돌려주는 { name, value }, name 은 원본 함수 이름 getLineWidth, isCircle)
///
/// # 예제
/// 게터 목록에 더하기 (같은 이름은 먼저 것만 남는다)
/// ```rust
/// use vanilla::core::reflection::push_reflected_getter;
/// use vanilla::{ReflectedGetter, ReflectValue};
///
/// let mut getter_list: Vec<ReflectedGetter> = Vec::new();
/// push_reflected_getter(&mut getter_list, "getSpeed", ReflectValue::Number(2.0));
/// push_reflected_getter(&mut getter_list, "getSpeed", ReflectValue::Number(9.0));
/// assert_eq!(getter_list.len(), 1);
/// assert_eq!(getter_list[0].name, "getSpeed");
/// assert_eq!(getter_list[0].value, ReflectValue::Number(2.0));
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct ReflectedGetter {
	/// 원본 함수 이름. (getLineWidth, isCircle)
	pub name: &'static str,
	/// 게터가 돌려준 값.
	pub value: ReflectValue,
}

//==============================================================================
/// 반사 구현. (`#[reflect]` 를 붙인 impl 블록에서 매크로가 짓는다)
/// - 직접 구현하지 않는다. 타입의 impl 블록과 impl Component 블록에 반사 매크로를 붙이면 매크로가 짓는다.
///
/// # 예제
/// 매크로가 지은 반사 구현을 바로 부르기
/// ```rust
/// use vanilla::{reflect, Component, ComponentBase, Reflect, ReflectValue};
///
/// struct Gauge {
/// 	component_base: ComponentBase,
/// 	ratio: f32,
/// }
///
/// #[reflect]
/// impl Gauge {
/// 	pub fn get_ratio(&self) -> f32 {
/// 		self.ratio
/// 	}
///
/// 	pub fn set_ratio(&mut self, ratio: f32) {
/// 		self.ratio = ratio;
/// 	}
/// }
///
/// #[reflect]
/// impl Component for Gauge {
/// # 	fn get_component_base(&self) -> &ComponentBase {
/// # 		&self.component_base
/// # 	}
/// # 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// # 		&mut self.component_base
/// # 	}
/// # 	fn get_component_type(&self) -> &'static str {
/// # 		"Gauge"
/// # 	}
/// }
///
/// let mut gauge = Gauge {
/// 	component_base: ComponentBase::new(),
/// 	ratio: 0.5,
/// };
/// assert_eq!(gauge.get_reflected_type_name(), "Gauge");
/// assert_eq!(gauge.get_reflected_properties()[0].name, "Ratio");
/// assert!(gauge.set_reflected_value("Ratio", &ReflectValue::Number(0.75)));
/// assert_eq!(gauge.get_reflected_value("Ratio"), Some(ReflectValue::Number(0.75)));
/// assert!(!gauge.set_reflected_value("Ratio", &ReflectValue::Boolean(true)));
/// assert_eq!(gauge.get_reflected_value("Missing"), None);
/// ```
//==============================================================================
#[diagnostic::on_unimplemented(message = "`{Self}` 에 반사 구현이 없다 (컴포넌트는 반사가 있어야 한다)", label = "반사 구현이 없는 타입", note = "`impl {Self}` 블록에 #[vanilla::reflect] 를 붙인다. 게터가 없으면 빈 블록 `#[vanilla::reflect] impl {Self} {{}}` 에 붙인다")]
pub trait Reflect {
	//==============================================================================
	/// 타입 이름. (원본 constructor.name 자리, 등록하지 않은 컴포넌트를 문서에 쓸 때의 type)
	///
	/// # 돌려줌
	/// impl 대상 타입의 이름. (경로의 끝 이름)
	//==============================================================================
	fn get_reflected_type_name(&self) -> &'static str;

	//==============================================================================
	/// 저장 속성 목록. (짝이 맞는 게터와 세터, 정의 차례. Component 공통 속성은 들지 않는다)
	///
	/// # 돌려줌
	/// 저장 속성 목록. (품은 값(base)의 것 가운데 없는 키가 뒤에 붙는다)
	//==============================================================================
	fn get_reflected_properties(&self) -> Vec<ReflectedProperty>;

	//==============================================================================
	/// 속성 값 읽기. (원본 키, 없는 키나 &mut self 게터면 None)
	///
	/// # 인자
	/// - `property_name`: 원본 키. (Speed 처럼 접두어를 뗀 이름)
	///
	/// # 돌려줌
	/// 속성 값. 없는 키, &mut self 게터, 트리를 받는 게터면 None.
	//==============================================================================
	fn get_reflected_value(&self, property_name: &str) -> Option<ReflectValue>;

	//==============================================================================
	/// 속성 값 읽기. (&mut self 게터까지)
	///
	/// # 인자
	/// - `property_name`: 원본 키.
	///
	/// # 돌려줌
	/// 속성 값. 없는 키나 트리를 받는 게터면 None.
	//==============================================================================
	fn get_reflected_value_mut(&mut self, property_name: &str) -> Option<ReflectValue>;

	//==============================================================================
	/// 속성 값 넣기. (꼴이 맞지 않거나, 없는 키거나, 트리를 받거나 값 하나로 부를 수 없는 세터면 false)
	///
	/// # 인자
	/// - `property_name`: 원본 키.
	/// - `value`: 넣을 값. (세터의 꼴로 바꿔 넣는다)
	///
	/// # 돌려줌
	/// 넣었으면 true.
	//==============================================================================
	fn set_reflected_value(&mut self, property_name: &str, value: &ReflectValue) -> bool;

	//==============================================================================
	/// 속성 값 넣기. (트리를 받는 세터까지. 이 컴포넌트는 트리에서 꺼낸 채로 부른다)
	///
	/// # 인자
	/// - `node_tree`: 트리를 받는 세터에 넘길 트리.
	/// - `property_name`: 원본 키.
	/// - `value`: 넣을 값. (세터의 꼴로 바꿔 넣는다)
	///
	/// # 돌려줌
	/// 넣었으면 true. 꼴이 맞지 않거나, 없는 키거나, 값 하나로 부를 수 없는 세터면 false.
	//==============================================================================
	fn set_reflected_value_in_tree(&mut self, node_tree: &mut NodeTree, property_name: &str, value: &ReflectValue) -> bool;

	//==============================================================================
	/// &mut self 게터가 있는지. (있으면 인스펙터와 저장이 &mut 갈래로 읽는다)
	///
	/// # 돌려줌
	/// &mut self 게터가 하나라도 있으면 true. (품은 값까지 본다)
	//==============================================================================
	fn has_mutable_getters(&self) -> bool;

	//==============================================================================
	/// 게터 값 모으기. (인스펙터용, 세터가 없는 것까지 모든 게터. impl Component 블록에서 다시 정의한 것 → impl 블록 차례. &mut self 게터는 뺀다)
	///
	/// # 인자
	/// - `getter_list`: 게터를 더할 목록. (같은 이름이 이미 있으면 더하지 않는다)
	//==============================================================================
	fn collect_reflected_getters(&self, getter_list: &mut Vec<ReflectedGetter>);

	//==============================================================================
	/// 게터 값 모으기. (트리를 받는 게터까지, &mut self 게터는 뺀다. 원본은 이웃 컴포넌트를 바로 쥐는 게터가 러스트 판에서는 트리를 받는다)
	///
	/// # 인자
	/// - `node_tree`: 트리를 받는 게터에 넘길 트리.
	/// - `getter_list`: 게터를 더할 목록. (같은 이름이 이미 있으면 더하지 않는다)
	//==============================================================================
	fn collect_reflected_getters_in_tree(&self, node_tree: &NodeTree, getter_list: &mut Vec<ReflectedGetter>);

	//==============================================================================
	/// 게터 값 모으기. (&mut self 게터와 트리를 받는 게터까지, 정의 차례. 이 컴포넌트는 트리에서 꺼낸 채로 부른다)
	///
	/// # 인자
	/// - `node_tree`: 트리를 받는 게터에 넘길 트리.
	/// - `getter_list`: 게터를 더할 목록. (같은 이름이 이미 있으면 더하지 않는다)
	//==============================================================================
	fn collect_reflected_getters_mut(&mut self, node_tree: &NodeTree, getter_list: &mut Vec<ReflectedGetter>);
}

//==============================================================================
/// impl Component 블록에서 다시 정의한 게터 모으기. (`#[reflect]` 를 붙인 impl Component 블록에서 매크로가 짓는다)
/// - 원본 하위 클래스가 덮어쓴 isTickNeeded, isDrawnInsideContent 같은 것이다. 원본 인스펙터는 그 클래스의 함수로 보인다.
/// - 직접 구현하지 않는다. impl Component 블록에 반사 매크로를 붙이면 매크로가 짓는다.
///
/// # 예제
/// impl Component 블록에서 다시 정의한 게터가 impl 블록의 게터보다 앞에 온다
/// ```rust
/// use vanilla::core::reflection::{collect_reflected_getters, ReflectComponentGetters};
/// use vanilla::{reflect, Component, ComponentBase, ReflectedGetter, ReflectValue};
///
/// struct Label {
/// 	component_base: ComponentBase,
/// }
///
/// #[reflect]
/// impl Label {
/// 	pub fn get_text(&self) -> &str {
/// 		"hello"
/// 	}
/// }
///
/// #[reflect]
/// impl Component for Label {
/// # 	fn get_component_base(&self) -> &ComponentBase {
/// # 		&self.component_base
/// # 	}
/// # 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// # 		&mut self.component_base
/// # 	}
/// # 	fn get_component_type(&self) -> &'static str {
/// # 		"Label"
/// # 	}
/// 	fn is_tick_needed(&self) -> bool {
/// 		false
/// 	}
/// }
///
/// let label = Label {
/// 	component_base: ComponentBase::new(),
/// };
/// let mut getter_list: Vec<ReflectedGetter> = Vec::new();
/// let mut anchored_getter_list = Vec::new();
/// label.collect_component_getters(&mut getter_list, &mut anchored_getter_list);
/// assert_eq!(getter_list[0].name, "isTickNeeded");
/// assert_eq!(getter_list[0].value, ReflectValue::Boolean(false));
/// let all_getter_list = collect_reflected_getters(&label);
/// let getter_names: Vec<&str> = all_getter_list.iter().map(|getter| getter.name).collect();
/// assert_eq!(getter_names, ["isTickNeeded", "getText"]);
/// ```
//==============================================================================
#[diagnostic::on_unimplemented(message = "`{Self}` 의 impl Component 블록에 #[reflect] 가 없다", label = "impl Component 블록에 #[reflect] 가 없는 타입", note = "`impl Component for {Self}` 블록에도 #[vanilla::reflect] 를 붙인다")]
pub trait ReflectComponentGetters {
	//==============================================================================
	/// 게터 모으기. (자리를 정한 것은 anchored_getter_list 에 (앞 원본 게터 이름, 게터) 로 담고, 나머지는 getter_list 앞에 담는다)
	///
	/// # 인자
	/// - `getter_list`: 자리를 정하지 않은 게터를 더할 목록.
	/// - `anchored_getter_list`: `#[reflect(after = "...")]` 로 자리를 정한 게터를 (앞 원본 게터 이름, 게터) 로 더할 목록.
	//==============================================================================
	fn collect_component_getters(&self, getter_list: &mut Vec<ReflectedGetter>, anchored_getter_list: &mut Vec<(&'static str, ReflectedGetter)>);
}

//==============================================================================
/// 원본 Component 기반 클래스의 저장 속성. (isGizmoVisible / setGizmoVisible, isEnable / setEnable, getComponentType / setComponentType, 정의 차례)
//==============================================================================
pub const COMPONENT_REFLECTED_PROPERTIES: [ReflectedProperty; 3] = [
	ReflectedProperty {
		name: "GizmoVisible",
		getter_name: "is_gizmo_visible",
		setter_name: Some("set_gizmo_visible"),
		is_saved: true,
	},
	ReflectedProperty {
		name: "Enable",
		getter_name: "is_enable",
		setter_name: Some("set_enable"),
		is_saved: true,
	},
	ReflectedProperty {
		name: "ComponentType",
		getter_name: "get_component_type",
		setter_name: None,
		is_saved: true,
	},
];

//==============================================================================
/// 컴포넌트의 저장 속성 목록. (원본 collectReflectedProperties: 이 타입의 것 → Component 공통 속성, 같은 키는 앞의 것)
///
/// # 인자
/// - `component`: 볼 컴포넌트.
///
/// # 돌려줌
/// 저장 속성 목록. (이 타입의 것 뒤에 GizmoVisible, Enable, ComponentType 가운데 없는 키)
//==============================================================================
pub fn collect_reflected_properties(component: &dyn Component) -> Vec<ReflectedProperty> {
	let reflection: &dyn Reflect = component;
	let mut property_list = reflection.get_reflected_properties();
	for base_property in COMPONENT_REFLECTED_PROPERTIES.iter() {
		if !property_list.iter().any(|property| property.name == base_property.name) {
			property_list.push(*base_property);
		}
	}
	property_list
}

//==============================================================================
/// 컴포넌트의 속성 값 읽기. (원본 `component[getterName]`(), 이 타입의 것이 먼저, 없으면 Component 공통 속성)
///
/// # 인자
/// - `component`: 읽을 컴포넌트.
/// - `property_name`: 원본 키.
///
/// # 돌려줌
/// 속성 값. 이 타입에도 공통 속성에도 없는 키면 None.
///
/// # 예제
/// 게터가 없는 컴포넌트도 공통 속성은 읽고 넣는다
/// ```rust
/// use vanilla::core::reflection::{read_reflected_property, write_reflected_property};
/// use vanilla::{reflect, Component, ComponentBase, ReflectValue};
///
/// struct Plain {
/// 	component_base: ComponentBase,
/// }
///
/// #[reflect]
/// impl Plain {}
///
/// #[reflect]
/// impl Component for Plain {
/// # 	fn get_component_base(&self) -> &ComponentBase {
/// # 		&self.component_base
/// # 	}
/// # 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// # 		&mut self.component_base
/// # 	}
/// # 	fn get_component_type(&self) -> &'static str {
/// # 		"Plain"
/// # 	}
/// }
///
/// let mut plain = Plain {
/// 	component_base: ComponentBase::new(),
/// };
/// assert_eq!(read_reflected_property(&plain, "Enable"), Some(ReflectValue::Boolean(true)));
/// assert_eq!(read_reflected_property(&plain, "ComponentType"), Some(ReflectValue::String(String::from("Plain"))));
/// assert_eq!(read_reflected_property(&plain, "Speed"), None);
/// assert!(write_reflected_property(&mut plain, "Enable", &ReflectValue::Boolean(false)));
/// assert!(!plain.is_enable());
/// assert!(!write_reflected_property(&mut plain, "ComponentType", &ReflectValue::String(String::from("Other"))));
/// ```
//==============================================================================
pub fn read_reflected_property(component: &dyn Component, property_name: &str) -> Option<ReflectValue> {
	let reflection: &dyn Reflect = component;
	if let Some(value) = reflection.get_reflected_value(property_name) {
		return Some(value);
	}
	match property_name {
		"GizmoVisible" => {
			Some(ReflectValue::Boolean(component.is_gizmo_visible()))
		}
		"Enable" => {
			Some(ReflectValue::Boolean(component.is_enable()))
		}
		"ComponentType" => {
			Some(ReflectValue::String(String::from(component.get_component_type())))
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 컴포넌트의 속성 값 읽기. (&mut self 게터까지)
///
/// # 인자
/// - `component`: 읽을 컴포넌트.
/// - `property_name`: 원본 키.
///
/// # 돌려줌
/// 속성 값. 이 타입에도 공통 속성에도 없는 키면 None.
//==============================================================================
pub fn read_reflected_property_mut(component: &mut dyn Component, property_name: &str) -> Option<ReflectValue> {
	let reflection: &mut dyn Reflect = component;
	if let Some(value) = reflection.get_reflected_value_mut(property_name) {
		return Some(value);
	}
	read_reflected_property(component, property_name)
}

//==============================================================================
/// 컴포넌트의 속성 값 넣기. (원본 `component[setterName]`(value), 넣었으면 true. ComponentType 은 러스트에 세터가 없어 false)
///
/// # 인자
/// - `component`: 넣을 컴포넌트.
/// - `property_name`: 원본 키.
/// - `value`: 넣을 값.
///
/// # 돌려줌
/// 넣었으면 true. 꼴이 맞지 않거나, 없는 키거나, 세터가 없으면 false.
//==============================================================================
pub fn write_reflected_property(component: &mut dyn Component, property_name: &str, value: &ReflectValue) -> bool {
	let reflection: &mut dyn Reflect = component;
	if reflection.set_reflected_value(property_name, value) {
		return true;
	}
	write_component_base_property(component, property_name, value)
}

//==============================================================================
/// 컴포넌트의 속성 값 넣기. (트리를 받는 세터까지. 이 컴포넌트는 트리에서 꺼낸 채로 부른다, NodeTree::with_component_at_mut)
///
/// # 인자
/// - `component`: 넣을 컴포넌트. (트리에서 꺼낸 것)
/// - `node_tree`: 트리를 받는 세터에 넘길 트리.
/// - `property_name`: 원본 키.
/// - `value`: 넣을 값.
///
/// # 돌려줌
/// 넣었으면 true. 꼴이 맞지 않거나, 없는 키거나, 세터가 없으면 false.
///
/// # 예제
/// 트리에 붙은 컴포넌트를 차례 번호로 꺼내 속성 넣기
/// ```rust
/// use vanilla::core::reflection::write_reflected_property_in_tree;
/// use vanilla::{Component, NodeTree, Paint, ReflectValue};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let node_id = node_tree.create_child(root_id, "panel");
/// node_tree[node_id].add_component(Paint::new());
/// let is_written = node_tree.with_component_at_mut(node_id, 0, |component, tree| {
/// 	write_reflected_property_in_tree(component, tree, "Enable", &ReflectValue::Boolean(false))
/// });
/// assert_eq!(is_written, Some(true));
/// assert!(!node_tree[node_id].get_component::<Paint>().expect("paint").is_enable());
/// ```
//==============================================================================
pub fn write_reflected_property_in_tree(component: &mut dyn Component, node_tree: &mut NodeTree, property_name: &str, value: &ReflectValue) -> bool {
	let reflection: &mut dyn Reflect = component;
	if reflection.set_reflected_value_in_tree(node_tree, property_name, value) {
		return true;
	}
	write_component_base_property(component, property_name, value)
}

//==============================================================================
/// Component 공통 속성 넣기. (GizmoVisible, Enable)
//==============================================================================
fn write_component_base_property(component: &mut dyn Component, property_name: &str, value: &ReflectValue) -> bool {
	match (property_name, value) {
		("GizmoVisible", ReflectValue::Boolean(is_gizmo_visible)) => {
			component.set_gizmo_visible(*is_gizmo_visible);
			true
		}
		("Enable", ReflectValue::Boolean(is_enable)) => {
			component.set_enable(*is_enable);
			true
		}
		_ => {
			false
		}
	}
}

//==============================================================================
/// 컴포넌트의 게터 값 모두. (원본 getComponentProperties, Component 기반 클래스의 것은 빼고 그 타입의 것만)
///
/// # 인자
/// - `component`: 볼 컴포넌트.
///
/// # 돌려줌
/// 게터 이름과 값 목록. (게터가 없는 컴포넌트는 빈 목록)
//==============================================================================
pub fn collect_reflected_getters(component: &dyn Component) -> Vec<ReflectedGetter> {
	let reflection: &dyn Reflect = component;
	let mut getter_list: Vec<ReflectedGetter> = Vec::new();
	reflection.collect_reflected_getters(&mut getter_list);
	getter_list
}

//==============================================================================
/// 컴포넌트의 게터 값 모두. (트리를 받는 게터까지)
///
/// # 인자
/// - `component`: 볼 컴포넌트.
/// - `node_tree`: 트리를 받는 게터에 넘길 트리.
///
/// # 돌려줌
/// 게터 이름과 값 목록. (게터가 없는 컴포넌트는 빈 목록)
//==============================================================================
pub fn collect_reflected_getters_in_tree(component: &dyn Component, node_tree: &NodeTree) -> Vec<ReflectedGetter> {
	let reflection: &dyn Reflect = component;
	let mut getter_list: Vec<ReflectedGetter> = Vec::new();
	reflection.collect_reflected_getters_in_tree(node_tree, &mut getter_list);
	getter_list
}

//==============================================================================
/// 컴포넌트의 게터 값 모두. (&mut self 게터와 트리를 받는 게터까지. 이 컴포넌트는 트리에서 꺼낸 채로 부른다, NodeTree::with_component_at_mut)
///
/// # 인자
/// - `component`: 볼 컴포넌트. (트리에서 꺼낸 것)
/// - `node_tree`: 트리를 받는 게터에 넘길 트리.
///
/// # 돌려줌
/// 게터 이름과 값 목록. (게터가 없는 컴포넌트는 빈 목록)
//==============================================================================
pub fn collect_reflected_getters_mut(component: &mut dyn Component, node_tree: &NodeTree) -> Vec<ReflectedGetter> {
	let reflection: &mut dyn Reflect = component;
	let mut getter_list: Vec<ReflectedGetter> = Vec::new();
	reflection.collect_reflected_getters_mut(node_tree, &mut getter_list);
	getter_list
}

//==============================================================================
/// 컴포넌트에 &mut self 게터가 있는지.
///
/// # 인자
/// - `component`: 볼 컴포넌트.
///
/// # 돌려줌
/// &mut self 게터가 있으면 true.
//==============================================================================
pub fn has_mutable_getters(component: &dyn Component) -> bool {
	let reflection: &dyn Reflect = component;
	reflection.has_mutable_getters()
}

//==============================================================================
/// 자리를 정한 게터 넣기. (앞 게터 바로 뒤, 같은 앞 게터면 담긴 차례로. 앞 게터가 없으면 끝에, 같은 이름이 이미 있으면 넣지 않는다)
///
/// # 인자
/// - `getter_list`: 넣을 목록.
/// - `anchored_getter_list`: (앞 원본 게터 이름, 게터) 목록. (담긴 차례로 넣는다)
///
/// # 예제
/// 앞 게터 바로 뒤에 넣기
/// ```rust
/// use vanilla::core::reflection::{insert_anchored_getters, push_reflected_getter};
/// use vanilla::{ReflectedGetter, ReflectValue};
///
/// let mut getter_list: Vec<ReflectedGetter> = Vec::new();
/// push_reflected_getter(&mut getter_list, "getText", ReflectValue::String(String::from("hi")));
/// push_reflected_getter(&mut getter_list, "getFontSize", ReflectValue::Number(16.0));
/// let anchored_getter_list = vec![("getText", ReflectedGetter {
/// 	name: "getFittedFontSize",
/// 	value: ReflectValue::Number(12.0),
/// })];
/// insert_anchored_getters(&mut getter_list, anchored_getter_list);
/// let getter_names: Vec<&str> = getter_list.iter().map(|getter| getter.name).collect();
/// assert_eq!(getter_names, ["getText", "getFittedFontSize", "getFontSize"]);
/// ```
//==============================================================================
pub fn insert_anchored_getters(getter_list: &mut Vec<ReflectedGetter>, anchored_getter_list: Vec<(&'static str, ReflectedGetter)>) {
	let mut last_anchor: Option<(&'static str, usize)> = None;
	for (anchor_name, getter) in anchored_getter_list {
		if getter_list.iter().any(|existing_getter| existing_getter.name == getter.name) {
			continue;
		}
		let insert_index = match last_anchor {
			Some((last_anchor_name, last_insert_index)) if last_anchor_name == anchor_name => {
				last_insert_index + 1
			}
			_ => {
				match getter_list.iter().position(|existing_getter| existing_getter.name == anchor_name) {
					Some(anchor_index) => {
						anchor_index + 1
					}
					None => {
						getter_list.len()
					}
				}
			}
		};
		getter_list.insert(insert_index, getter);
		last_anchor = Some((anchor_name, insert_index));
	}
}

//==============================================================================
/// 게터 하나 더하기. (같은 이름이 이미 있으면 더하지 않는다. 원본 getComponentProperties 의 seenNames, 하위 클래스의 것이 먼저다)
///
/// # 인자
/// - `getter_list`: 더할 목록.
/// - `name`: 원본 함수 이름. (getLineWidth)
/// - `value`: 게터 값.
//==============================================================================
pub fn push_reflected_getter(getter_list: &mut Vec<ReflectedGetter>, name: &'static str, value: ReflectValue) {
	if getter_list.iter().any(|getter| getter.name == name) {
		return;
	}
	getter_list.push(ReflectedGetter {
		name,
		value,
	});
}

//==============================================================================
/// f32 를 가장 짧은 십진으로 읽은 f64. (0.22 는 0.22, 원본에서 같은 값을 자바스크립트 수로 적은 것과 같다)
///
/// # 인자
/// - `number`: 바꿀 수.
///
/// # 돌려줌
/// 가장 짧은 십진 글로 읽은 f64. (무한과 NaN 은 그대로 넓힌다)
///
/// # 예제
/// f32 0.22 를 넓히면 0.22 가 아니지만, 가장 짧은 십진으로 읽으면 0.22 다
/// ```rust
/// use vanilla::core::reflection::read_shortest_number;
///
/// assert_eq!(read_shortest_number(0.22), 0.22);
/// assert_ne!(0.22f32 as f64, 0.22);
/// ```
//==============================================================================
pub fn read_shortest_number(number: f32) -> f64 {
	if !number.is_finite() {
		return number as f64;
	}
	format!("{}", number).parse::<f64>().unwrap_or(number as f64)
}

//==============================================================================
/// 꼴의 짧은 이름. (모듈 경로와 감싼 꼴을 벗긴다. `&alloc::rc::Rc<vanilla::core::image::Image>` → Image)
///
/// # 돌려줌
/// 모듈 경로와 앞의 &, 감싼 꼴(Rc, Arc, Box, RefCell, Cell)을 벗긴 이름. (안쪽 제네릭 인자의 경로도 벗긴다)
///
/// # 예제
/// 감싼 꼴과 경로 벗기기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::core::reflection::read_short_type_name;
/// use vanilla::{Color, Vector2};
///
/// assert_eq!(read_short_type_name::<Rc<RefCell<Color>>>(), "Color");
/// assert_eq!(read_short_type_name::<&Vector2>(), "Vector2");
/// assert_eq!(read_short_type_name::<Option<Color>>(), "Option<Color>");
/// ```
//==============================================================================
pub fn read_short_type_name<T: ?Sized>() -> String {
	let full_name = std::any::type_name::<T>();
	let mut short_name = String::with_capacity(full_name.len());
	let mut word = String::new();
	for character in full_name.chars() {
		if character.is_alphanumeric() || character == '_' || character == ':' {
			word.push(character);
			continue;
		}
		short_name.push_str(word.rsplit("::").next().unwrap_or(""));
		word.clear();
		short_name.push(character);
	}
	short_name.push_str(word.rsplit("::").next().unwrap_or(""));
	loop {
		if let Some(inner_name) = short_name.strip_prefix("&mut ") {
			short_name = String::from(inner_name);
			continue;
		}
		if let Some(inner_name) = short_name.strip_prefix('&') {
			short_name = String::from(inner_name);
			continue;
		}
		let wrapper_prefix = WRAPPER_TYPE_PREFIXES.iter().find(|wrapper_prefix| short_name.starts_with(**wrapper_prefix) && short_name.ends_with('>'));
		match wrapper_prefix {
			Some(wrapper_prefix) => {
				short_name = String::from(&short_name[wrapper_prefix.len()..short_name.len() - 1]);
			}
			None => {
				break;
			}
		}
	}
	short_name
}

//==============================================================================
/// 게터 값 읽기의 자동 참조 특수화. (매크로가 (&&&ReflectGetProbe(&값)).read_reflect_value() 로 부른다)
/// - 메서드를 찾을 때 참조를 하나씩 벗기며 맞는 구현을 고르는 것을 써서, 바꿀 수 있는 꼴(ToReflectValue) → 품은 꼴(Option, 배열) → 종류 이름 차례로 고른다.
///   바꿀 수 없는 꼴의 게터도 컴파일 에러 없이 원본처럼 저장에서 빠지고 인스펙터에는 [종류 이름] 으로 보인다.
///
/// # 예제
/// 매크로가 부르는 꼴 그대로 읽기
/// ```rust
/// use vanilla::core::reflection::{ReflectGetProbe, ReflectGetViaName as _, ReflectGetViaOpaque as _, ReflectGetViaValue as _};
/// use vanilla::{ReflectValue, Vector2};
///
/// let speed = 0.5f32;
/// assert_eq!((&&&ReflectGetProbe(&speed)).read_reflect_value(), ReflectValue::Number(0.5));
/// let points = vec![Vector2::zero()];
/// assert_eq!((&&&ReflectGetProbe(&points)).read_reflect_value(), ReflectValue::Object(String::from("Array")));
/// let pair = (1u8, 2u8);
/// assert_eq!((&&&ReflectGetProbe(&pair)).read_reflect_value(), ReflectValue::Object(String::from("(u8, u8)")));
/// ```
//==============================================================================
pub struct ReflectGetProbe<'a, T: ?Sized>(
	/// 읽을 게터 값의 참조.
	pub &'a T
);

//==============================================================================
/// 바꿀 수 있는 꼴. (가장 먼저 고른다)
///
/// # 예제
/// 바꿀 수 있는 꼴의 게터 값
/// ```rust
/// use vanilla::core::reflection::{ReflectGetProbe, ReflectGetViaValue};
/// use vanilla::ReflectValue;
///
/// let is_open = true;
/// assert_eq!((&&ReflectGetProbe(&is_open)).read_reflect_value(), ReflectValue::Boolean(true));
/// ```
//==============================================================================
pub trait ReflectGetViaValue {
	//==============================================================================
	/// 게터 값을 반사 값으로 읽기.
	///
	/// # 돌려줌
	/// ToReflectValue 로 바꾼 값.
	//==============================================================================
	fn read_reflect_value(&self) -> ReflectValue;
}

impl<'a, T: ToReflectValue + ?Sized> ReflectGetViaValue for &&ReflectGetProbe<'a, T> {
	//==============================================================================
	/// ToReflectValue 로 바꾼다.
	//==============================================================================
	fn read_reflect_value(&self) -> ReflectValue {
		self.0.to_reflect_value()
	}
}

//==============================================================================
/// 품은 꼴. (Option 은 None 이면 Null 이고 Some 이면 안의 종류 이름, 배열은 Array. 원본 null 과 배열 자리)
///
/// # 예제
/// Option 과 배열의 반사 값
/// ```rust
/// use vanilla::core::reflection::ReflectOpaque;
/// use vanilla::{Color, ReflectValue};
///
/// let color: Option<Color> = Some(Color::white());
/// assert_eq!(color.to_opaque_reflect_value(), ReflectValue::Object(String::from("Color")));
/// let empty_color: Option<Color> = None;
/// assert_eq!(empty_color.to_opaque_reflect_value(), ReflectValue::Null);
/// assert_eq!(vec![1, 2].to_opaque_reflect_value(), ReflectValue::Object(String::from("Array")));
/// ```
//==============================================================================
pub trait ReflectOpaque {
	//==============================================================================
	/// 품은 꼴의 반사 값.
	///
	/// # 돌려줌
	/// Option 은 None 이면 Null, Some 이면 안의 종류 이름. 배열은 `Object("Array")`.
	//==============================================================================
	fn to_opaque_reflect_value(&self) -> ReflectValue;
}

impl<T> ReflectOpaque for Option<T> {
	//==============================================================================
	/// None 이면 Null, Some 이면 안의 종류 이름.
	//==============================================================================
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		match self {
			Some(_) => {
				ReflectValue::Object(read_short_type_name::<T>())
			}
			None => {
				ReflectValue::Null
			}
		}
	}
}

impl<T> ReflectOpaque for Vec<T> {
	//==============================================================================
	/// 배열. (`Object("Array")`)
	//==============================================================================
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(String::from("Array"))
	}
}

impl<T> ReflectOpaque for [T] {
	//==============================================================================
	/// 배열. (`Object("Array")`)
	//==============================================================================
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(String::from("Array"))
	}
}

impl<T, const N: usize> ReflectOpaque for [T; N] {
	//==============================================================================
	/// 배열. (`Object("Array")`)
	//==============================================================================
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(String::from("Array"))
	}
}

impl<T: ReflectOpaque + ?Sized> ReflectOpaque for &T {
	//==============================================================================
	/// 가리키는 값의 것.
	//==============================================================================
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		(**self).to_opaque_reflect_value()
	}
}

//==============================================================================
/// 품은 꼴 고르기. (둘째로 고른다)
///
/// # 예제
/// 값으로 바꿀 수 없는 꼴을 품은 Option
/// ```rust
/// use vanilla::core::reflection::{ReflectGetProbe, ReflectGetViaOpaque};
/// use vanilla::ReflectValue;
///
/// let empty_list: Option<Vec<u8>> = None;
/// assert_eq!((&ReflectGetProbe(&empty_list)).read_reflect_value(), ReflectValue::Null);
/// ```
//==============================================================================
pub trait ReflectGetViaOpaque {
	//==============================================================================
	/// 품은 꼴의 게터 값 읽기.
	///
	/// # 돌려줌
	/// ReflectOpaque 로 바꾼 값.
	//==============================================================================
	fn read_reflect_value(&self) -> ReflectValue;
}

impl<'a, T: ReflectOpaque + ?Sized> ReflectGetViaOpaque for &ReflectGetProbe<'a, T> {
	//==============================================================================
	/// ReflectOpaque 로 바꾼다.
	//==============================================================================
	fn read_reflect_value(&self) -> ReflectValue {
		self.0.to_opaque_reflect_value()
	}
}

//==============================================================================
/// 종류 이름. (마지막으로 고른다. 원본 formatPropertyValue 의 `[constructor.name]` 자리)
///
/// # 예제
/// 바꿀 수 없는 꼴은 종류 이름으로
/// ```rust
/// use vanilla::core::reflection::{ReflectGetProbe, ReflectGetViaName};
/// use vanilla::ReflectValue;
///
/// let pair = (1u8, 2u8);
/// assert_eq!(ReflectGetProbe(&pair).read_reflect_value(), ReflectValue::Object(String::from("(u8, u8)")));
/// ```
//==============================================================================
pub trait ReflectGetViaName {
	//==============================================================================
	/// 종류 이름으로 읽기.
	///
	/// # 돌려줌
	/// `Object(짧은 종류 이름)`.
	//==============================================================================
	fn read_reflect_value(&self) -> ReflectValue;
}

impl<'a, T: ?Sized> ReflectGetViaName for ReflectGetProbe<'a, T> {
	//==============================================================================
	/// 짧은 종류 이름의 Object.
	//==============================================================================
	fn read_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(read_short_type_name::<T>())
	}
}

//==============================================================================
/// 세터 값 읽기의 자동 참조 특수화. (매크로가 (`&ReflectSetProbe::<세터 꼴>`::new(값)).convert_reflect_value() 로 부른다)
/// - 세터 꼴이 FromReflectValue 면 바꾸고, 아니면 늘 None 이라 그 속성은 읽기를 건너뛴다. (컴파일 에러가 없다)
///
/// # 예제
/// 세터 꼴로 바꿔 보기 (바꿀 수 없는 꼴은 None)
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::reflection::{ReflectSetProbe, ReflectSetViaNone as _, ReflectSetViaValue as _};
/// use vanilla::{Color, ReflectValue};
///
/// let color_list = ReflectValue::List(vec![ReflectValue::Number(1.0), ReflectValue::Number(0.0), ReflectValue::Number(0.0), ReflectValue::Number(1.0)]);
/// assert_eq!((&ReflectSetProbe::<Color>::new(&color_list)).convert_reflect_value(), Some(Color::create(1.0, 0.0, 0.0, 1.0)));
/// assert!((&ReflectSetProbe::<Rc<Vec<u8>>>::new(&color_list)).convert_reflect_value().is_none());
/// ```
//==============================================================================
pub struct ReflectSetProbe<'a, T> {
	value: &'a ReflectValue,
	target_type: PhantomData<fn() -> T>,
}

impl<'a, T> ReflectSetProbe<'a, T> {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `value`: 바꿀 반사 값.
	///
	/// # 돌려줌
	/// 세터 꼴 T 로 바꿀 탐침.
	//==============================================================================
	pub fn new(value: &'a ReflectValue) -> ReflectSetProbe<'a, T> {
		ReflectSetProbe {
			value,
			target_type: PhantomData,
		}
	}
}

//==============================================================================
/// 읽을 수 있는 꼴. (먼저 고른다)
///
/// # 예제
/// 수를 정수 세터 꼴로
/// ```rust
/// use vanilla::core::reflection::{ReflectSetProbe, ReflectSetViaValue};
/// use vanilla::ReflectValue;
///
/// let number = ReflectValue::Number(3.0);
/// assert_eq!(ReflectSetProbe::<usize>::new(&number).convert_reflect_value(), Some(3));
/// ```
//==============================================================================
pub trait ReflectSetViaValue<T> {
	//==============================================================================
	/// 반사 값을 세터 꼴로 바꾸기.
	///
	/// # 돌려줌
	/// 바꾼 값. 꼴이 맞지 않으면 None.
	//==============================================================================
	fn convert_reflect_value(&self) -> Option<T>;
}

impl<'a, T: FromReflectValue<'a>> ReflectSetViaValue<T> for ReflectSetProbe<'a, T> {
	//==============================================================================
	/// FromReflectValue 로 바꾼다.
	//==============================================================================
	fn convert_reflect_value(&self) -> Option<T> {
		T::from_reflect_value(self.value)
	}
}

//==============================================================================
/// 읽을 수 없는 꼴. (늘 None)
///
/// # 예제
/// FromReflectValue 가 없는 꼴
/// ```rust
/// use std::rc::Rc;
/// use vanilla::core::reflection::{ReflectSetProbe, ReflectSetViaNone};
/// use vanilla::ReflectValue;
///
/// let number = ReflectValue::Number(3.0);
/// assert!((&ReflectSetProbe::<Rc<Vec<u8>>>::new(&number)).convert_reflect_value().is_none());
/// ```
//==============================================================================
pub trait ReflectSetViaNone<T> {
	//==============================================================================
	/// 읽을 수 없는 꼴의 값.
	///
	/// # 돌려줌
	/// 늘 None.
	//==============================================================================
	fn convert_reflect_value(&self) -> Option<T>;
}

impl<'a, T> ReflectSetViaNone<T> for &ReflectSetProbe<'a, T> {
	//==============================================================================
	/// 늘 None.
	//==============================================================================
	fn convert_reflect_value(&self) -> Option<T> {
		None
	}
}

//==============================================================================
/// 기본 꼴의 반사 값.
//==============================================================================
impl ToReflectValue for bool {
	//==============================================================================
	/// Boolean 으로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Boolean(*self)
	}
}

impl ToReflectValue for f32 {
	//==============================================================================
	/// 가장 짧은 십진으로 읽은 Number 로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Number(read_shortest_number(*self))
	}
}

impl ToReflectValue for f64 {
	//==============================================================================
	/// Number 로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Number(*self)
	}
}

impl ToReflectValue for str {
	//==============================================================================
	/// String 으로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::String(String::from(self))
	}
}

impl ToReflectValue for String {
	//==============================================================================
	/// String 으로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::String(self.clone())
	}
}

impl ToReflectValue for Vector2 {
	//==============================================================================
	/// Vector2 로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Vector2(*self)
	}
}

impl ToReflectValue for Color {
	//==============================================================================
	/// Color 로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Color(*self)
	}
}

impl ToReflectValue for Rect {
	//==============================================================================
	/// Rect 로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Rect(*self)
	}
}

impl ToReflectValue for NodeId {
	//==============================================================================
	/// Node 로.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Node(*self)
	}
}

impl<T: ToReflectValue> ToReflectValue for Option<T> {
	//==============================================================================
	/// Some 이면 안의 값, None 이면 Null.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		match self {
			Some(value) => {
				value.to_reflect_value()
			}
			None => {
				ReflectValue::Null
			}
		}
	}
}

impl<T: ToReflectValue + ?Sized> ToReflectValue for &T {
	//==============================================================================
	/// 가리키는 값의 것.
	//==============================================================================
	fn to_reflect_value(&self) -> ReflectValue {
		(**self).to_reflect_value()
	}
}

impl<'a> FromReflectValue<'a> for bool {
	//==============================================================================
	/// Boolean 이면 그 값.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<bool> {
		match value {
			ReflectValue::Boolean(boolean_value) => {
				Some(*boolean_value)
			}
			_ => {
				None
			}
		}
	}
}

impl<'a> FromReflectValue<'a> for f32 {
	//==============================================================================
	/// Number 면 f32 로.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<f32> {
		match value {
			ReflectValue::Number(number) => {
				Some(*number as f32)
			}
			_ => {
				None
			}
		}
	}
}

impl<'a> FromReflectValue<'a> for f64 {
	//==============================================================================
	/// Number 면 그 값.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<f64> {
		match value {
			ReflectValue::Number(number) => {
				Some(*number)
			}
			_ => {
				None
			}
		}
	}
}

impl<'a> FromReflectValue<'a> for String {
	//==============================================================================
	/// String 이면 복사한 글.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<String> {
		match value {
			ReflectValue::String(text) => {
				Some(text.clone())
			}
			_ => {
				None
			}
		}
	}
}

impl<'a> FromReflectValue<'a> for &'a str {
	//==============================================================================
	/// String 이면 빌린 글.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<&'a str> {
		match value {
			ReflectValue::String(text) => {
				Some(text.as_str())
			}
			_ => {
				None
			}
		}
	}
}

impl<'a> FromReflectValue<'a> for Vector2 {
	//==============================================================================
	/// Vector2 거나 수 둘의 List 면 Vector2.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<Vector2> {
		match value {
			ReflectValue::Vector2(vector) => {
				Some(*vector)
			}
			ReflectValue::List(item_values) if item_values.len() == 2 => {
				Some(Vector2::create(read_list_number(item_values, 0)?, read_list_number(item_values, 1)?))
			}
			_ => {
				None
			}
		}
	}
}

impl<'a> FromReflectValue<'a> for Color {
	//==============================================================================
	/// Color 거나 수 넷의 List 면 Color.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<Color> {
		match value {
			ReflectValue::Color(color) => {
				Some(*color)
			}
			ReflectValue::List(item_values) if item_values.len() == 4 => {
				Some(Color::create(read_list_number(item_values, 0)?, read_list_number(item_values, 1)?, read_list_number(item_values, 2)?, read_list_number(item_values, 3)?))
			}
			_ => {
				None
			}
		}
	}
}

impl<'a> FromReflectValue<'a> for NodeId {
	//==============================================================================
	/// Node 면 그 핸들.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<NodeId> {
		match value {
			ReflectValue::Node(node_id) => {
				Some(*node_id)
			}
			_ => {
				None
			}
		}
	}
}

impl<'a, T: FromReflectValue<'a>> FromReflectValue<'a> for Option<T> {
	//==============================================================================
	/// Null 이면 Some(None), 아니면 안의 꼴로 읽은 것.
	//==============================================================================
	fn from_reflect_value(value: &'a ReflectValue) -> Option<Option<T>> {
		match value {
			ReflectValue::Null => {
				Some(None)
			}
			_ => {
				T::from_reflect_value(value).map(Some)
			}
		}
	}
}

//==============================================================================
/// 배열의 수 하나. (f32)
//==============================================================================
fn read_list_number(item_values: &[ReflectValue], item_index: usize) -> Option<f32> {
	match item_values.get(item_index) {
		Some(ReflectValue::Number(number)) => {
			Some(*number as f32)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 정수 꼴의 반사 값. (원본 수, 읽을 때는 원본처럼 수를 그 꼴로 넣는다)
//==============================================================================
macro_rules! impl_reflect_integer {
	($($integer_type:ty),*) => {
		$(
			impl ToReflectValue for $integer_type {
				fn to_reflect_value(&self) -> ReflectValue {
					ReflectValue::Number(*self as f64)
				}
			}

			impl<'a> FromReflectValue<'a> for $integer_type {
				fn from_reflect_value(value: &'a ReflectValue) -> Option<$integer_type> {
					match value {
						ReflectValue::Number(number) => {
							Some(*number as $integer_type)
						}
						_ => {
							None
						}
					}
				}
			}
		)*
	};
}

impl_reflect_integer!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

//==============================================================================
/// 원본에서 글인 열거의 반사 값. (열거 정의 옆에서 부른다. 값과 원본 글의 짝을 모두 적거나, 이미 있는 짝 표 상수를 준다)
//==============================================================================
macro_rules! impl_reflect_names {
	($enum_type:ty, [$(($variant:path, $name:literal)),* $(,)?]) => {
		impl $crate::core::reflection::ToReflectValue for $enum_type {
			fn to_reflect_value(&self) -> $crate::core::reflection::ReflectValue {
				match self {
					$(
						$variant => {
							$crate::core::reflection::ReflectValue::String(::std::string::String::from($name))
						}
					)*
				}
			}
		}

		impl<'a> $crate::core::reflection::FromReflectValue<'a> for $enum_type {
			fn from_reflect_value(value: &'a $crate::core::reflection::ReflectValue) -> ::core::option::Option<$enum_type> {
				match value {
					$crate::core::reflection::ReflectValue::String(name) => {
						match name.as_str() {
							$(
								$name => {
									::core::option::Option::Some($variant)
								}
							)*
							_ => {
								::core::option::Option::None
							}
						}
					}
					_ => {
						::core::option::Option::None
					}
				}
			}
		}
	};
	($enum_type:ty, $name_table:expr) => {
		impl $crate::core::reflection::ToReflectValue for $enum_type {
			fn to_reflect_value(&self) -> $crate::core::reflection::ReflectValue {
				match $name_table.iter().find(|(table_value, _)| table_value == self) {
					::core::option::Option::Some((_, name)) => {
						$crate::core::reflection::ReflectValue::String(::std::string::String::from(*name))
					}
					::core::option::Option::None => {
						$crate::core::reflection::ReflectValue::Null
					}
				}
			}
		}

		impl<'a> $crate::core::reflection::FromReflectValue<'a> for $enum_type {
			fn from_reflect_value(value: &'a $crate::core::reflection::ReflectValue) -> ::core::option::Option<$enum_type> {
				match value {
					$crate::core::reflection::ReflectValue::String(name) => {
						$name_table.iter().find(|(_, table_name)| *table_name == name.as_str()).map(|(table_value, _)| *table_value)
					}
					_ => {
						::core::option::Option::None
					}
				}
			}
		}
	};
}

pub(crate) use impl_reflect_names;

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	//==============================================================================
	/// 시험용 열거.
	//==============================================================================
	#[derive(Clone, Copy, Debug, PartialEq)]
	enum Side {
		Left,
		Right,
	}

	impl_reflect_names!(Side, [(Side::Left, "left"), (Side::Right, "right")]);

	//==============================================================================
	/// 게터 값 읽기를 매크로가 짓는 꼴 그대로 부른다.
	//==============================================================================
	fn read_probe<T>(value: T) -> ReflectValue {
		#[allow(unused_imports)]
		use super::{ReflectGetViaName as _, ReflectGetViaOpaque as _, ReflectGetViaValue as _};
		(&&&ReflectGetProbe(&value)).read_reflect_value()
	}

	//==============================================================================
	/// 바꿀 수 있는 꼴은 값으로, Option 과 배열은 null 과 Array 로, 그 밖은 종류 이름으로 읽는다. (구체 꼴에서 특수화가 고른다)
	//==============================================================================
	#[test]
	fn get_probe_picks_value_then_opaque_then_name() {
		#[allow(unused_imports)]
		use super::{ReflectGetViaName as _, ReflectGetViaOpaque as _, ReflectGetViaValue as _};
		let number = 0.22f32;
		assert_eq!((&&&ReflectGetProbe(&number)).read_reflect_value(), ReflectValue::Number(0.22));
		let text = "hello";
		assert_eq!((&&&ReflectGetProbe(&text)).read_reflect_value(), ReflectValue::String(String::from("hello")));
		let color = Some(Color::create(1.0, 0.5, 0.0, 1.0));
		assert_eq!((&&&ReflectGetProbe(&color)).read_reflect_value(), ReflectValue::Color(Color::create(1.0, 0.5, 0.0, 1.0)));
		let side = Side::Right;
		assert_eq!((&&&ReflectGetProbe(&side)).read_reflect_value(), ReflectValue::String(String::from("right")));
		let image: Option<Rc<RefCell<Vec<u8>>>> = Some(Rc::new(RefCell::new(Vec::new())));
		assert_eq!((&&&ReflectGetProbe(&image)).read_reflect_value(), ReflectValue::Object(String::from("Vec<u8>")));
		let empty_image: Option<Rc<RefCell<Vec<u8>>>> = None;
		assert_eq!((&&&ReflectGetProbe(&empty_image)).read_reflect_value(), ReflectValue::Null);
		let points: &[Vector2] = &[Vector2::create(0.0, 0.0)];
		assert_eq!((&&&ReflectGetProbe(&points)).read_reflect_value(), ReflectValue::Object(String::from("Array")));
		let pair = (1usize, 2usize);
		assert_eq!((&&&ReflectGetProbe(&pair)).read_reflect_value(), ReflectValue::Object(String::from("(usize, usize)")));
		// 제네릭 안에서는 꼴을 모르니 늘 종류 이름이다. (매크로는 구체 꼴의 impl 블록에서 부른다)
		assert_eq!(read_probe(1.0f32), ReflectValue::Object(String::from("f32")));
	}

	//==============================================================================
	/// 세터 꼴로 읽고, 읽을 수 없는 꼴은 None 이다.
	//==============================================================================
	#[test]
	fn set_probe_converts_by_setter_type() {
		#[allow(unused_imports)]
		use super::{ReflectSetViaNone as _, ReflectSetViaValue as _};
		let color_list = ReflectValue::List(vec![ReflectValue::Number(1.0), ReflectValue::Number(0.0), ReflectValue::Number(0.0), ReflectValue::Number(1.0)]);
		assert_eq!((&ReflectSetProbe::<Color>::new(&color_list)).convert_reflect_value(), Some(Color::create(1.0, 0.0, 0.0, 1.0)));
		assert_eq!((&ReflectSetProbe::<Option<Color>>::new(&color_list)).convert_reflect_value(), Some(Some(Color::create(1.0, 0.0, 0.0, 1.0))));
		assert_eq!((&ReflectSetProbe::<Vector2>::new(&color_list)).convert_reflect_value(), None);
		let text = ReflectValue::String(String::from("left"));
		assert_eq!((&ReflectSetProbe::<&str>::new(&text)).convert_reflect_value(), Some("left"));
		assert_eq!((&ReflectSetProbe::<Side>::new(&text)).convert_reflect_value(), Some(Side::Left));
		assert_eq!((&ReflectSetProbe::<f32>::new(&text)).convert_reflect_value(), None);
		let number = ReflectValue::Number(3.0);
		assert_eq!((&ReflectSetProbe::<usize>::new(&number)).convert_reflect_value(), Some(3));
		assert!((&ReflectSetProbe::<Rc<Vec<u8>>>::new(&number)).convert_reflect_value().is_none());
	}

	//==============================================================================
	/// 종류 이름은 경로와 감싼 꼴을 벗긴다.
	//==============================================================================
	#[test]
	fn short_type_name_strips_paths_and_wrappers() {
		assert_eq!(read_short_type_name::<Rc<RefCell<Color>>>(), "Color");
		assert_eq!(read_short_type_name::<&Vector2>(), "Vector2");
		assert_eq!(read_short_type_name::<Option<Color>>(), "Option<Color>");
		assert_eq!(read_short_type_name::<std::collections::HashMap<String, Color>>(), "HashMap<String, Color>");
	}
}
