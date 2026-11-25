//==============================================================================
// 반사. (원본 vanilla.js 의 실행 중 반사 자리: ui/uidocument.js 의 collectReflectedProperties 와 getReflectedProperties,
//   misc/devtools.js 의 getComponentProperties)
// - 원본은 자바스크립트라 실행 중에 객체의 프로토타입을 훑어 get, is, set 함수를 찾는다. 러스트에는 실행 중 반사가 없어,
//   언리얼의 UHT(UPROPERTY), Qt 의 moc 처럼 컴파일 때 코드를 지어 같은 일을 한다. 러스트에서는 절차 매크로 #[reflect] 가 그 일을 한다.
//   (macros/ 크레이트, 엔진이 vanilla::reflect 로 다시 내보낸다) C++ RTTI 의 typeid, dynamic_cast 자리는 이미 AsAny(std::any::Any)다.
// - 이 모듈은 매크로가 지은 코드가 쓰는 값 꼴(ReflectValue)과 트레이트(Reflect), 원본 Component 기반 클래스의 공통 속성을 둔다.
// - 쓰는 법: 컴포넌트의 impl 블록과 impl Component 블록 둘 다에 #[vanilla::reflect] 를 붙인다.
//     #[vanilla::reflect]
//     impl Spin {
//         pub fn get_speed(&self) -> f32 { self.speed }
//         pub fn set_speed(&mut self, speed: f32) { self.speed = speed; }
//     }
//     #[vanilla::reflect]
//     impl Component for Spin { ... }
//   그러면 UIDocument 가 등록된 쓰기, 읽기 함수 없이 Speed 키로 쓰고 읽고(원본 saveByReflection, loadByReflection),
//   DevTools 인스펙터가 speed 줄로 보인다. (원본 getComponentProperties) 한쪽만 붙이면 컴파일 에러가 붙일 곳을 알려 준다.
// - 원본 규칙을 따른다:
//   - 게터는 인자 없는 pub get_*, is_* 함수, 저장 속성은 같은 이름의 set_* 이 있는 게터다. 키는 접두어를 뗀 원본 이름이다. (get_line_width → LineWidth)
//   - 차례는 정의 차례이고 같은 키는 먼저 나온 것이다. 원본 Component 기반 클래스의 GizmoVisible, Enable, ComponentType 이 끝에 붙는다.
//   - 저장하는 값은 원본 saveByReflection 이 가리는 꼴(수, 불, 글, Color, Vector2, 노드)이다. 원본에서 글인 열거(TextAlign 같은 것)는 같은 글로 바꾼다.
// - 원본과 다른 것:
//   - 원본은 모든 객체가 반사된다. 러스트 판은 #[reflect] 를 붙인 타입만이다. 붙이지 않은 컴포넌트는 get_reflection 이 None 이다.
//   - 원본 상위 클래스의 게터는 프로토타입 사슬로 따라온다. 러스트 판은 상속이 없어 품은 값을 #[reflect(base = 필드)] 로 잇는다.
//     원본에서 상위 클래스 몫인 함수를 러스트 판이 편의로 다시 둔 것은 #[reflect(skip)] 로 빼서 차례를 원본처럼 둔다.
//   - &mut self 게터, 인자를 더 받는 세터(set_image(그림, 조각) 같은 것)는 값 하나로 부를 수 없다. 세터가 둘 이상을 받으면 속성 목록에는 들지만
//     (원본처럼 set 함수가 있으면 짝이다) 읽기는 건너뛴다. 원본 setComponentType 자리는 러스트에 세터가 없어(종류는 타입이 정한다) 쓰기만 한다.
//   - 원본은 읽을 때 지금 값의 꼴로 받을지 가린다. (배열이면 지금 값이 Color 인지 Vector2 인지, 그 밖은 typeof 가 같은지)
//     러스트 판은 세터의 꼴로 가린다. 지금 값이 있으면 결과가 같고, 지금 값이 없는(None) 속성도 읽는다.
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::Component;
use crate::core::node::nodetree::NodeId;
use std::marker::PhantomData;

// 감싼 꼴의 앞머리. (인스펙터의 종류 이름은 감싼 것을 벗긴 안의 이름이다. Rc<RefCell<Image>> → Image)
const WRAPPER_TYPE_PREFIXES: [&str; 5] = ["Rc<", "Arc<", "Box<", "RefCell<", "Cell<"];

//==============================================================================
// 반사 값. (원본 게터가 돌려주는 자바스크립트 값 자리)
// - Null 은 원본 null(러스트 None), List 는 문서에서 읽은 배열, Object 는 옮길 수 없는 값의 종류 이름이다. (인스펙터의 [이름])
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum ReflectValue {
	Null,
	Boolean(bool),
	Number(f64),
	String(String),
	Vector2(Vector2),
	Color(Color),
	Rect(Rect),
	Node(NodeId),
	List(Vec<ReflectValue>),
	Object(String),
}

//==============================================================================
// 반사 값으로 바꿀 수 있는 꼴.
//==============================================================================
pub trait ToReflectValue {
	fn to_reflect_value(&self) -> ReflectValue;
}

//==============================================================================
// 반사 값에서 읽을 수 있는 꼴. (꼴이 맞지 않으면 None, 원본 loadByReflection 의 typeof 가 같은지 자리)
//==============================================================================
pub trait FromReflectValue<'a>: Sized {
	fn from_reflect_value(value: &'a ReflectValue) -> Option<Self>;
}

//==============================================================================
// 저장 속성 하나. (원본 getReflectedProperties 가 돌려주는 { name, getterName, setterName })
// - name 은 원본 키(접두어를 뗀 원본 이름), getter_name 과 setter_name 은 러스트 함수 이름이다. setter_name 이 None 이면 러스트에 세터가 없다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReflectedProperty {
	pub name: &'static str,
	pub getter_name: &'static str,
	pub setter_name: Option<&'static str>,
}

//==============================================================================
// 게터 하나와 그 값. (원본 getComponentProperties 가 돌려주는 { name, value }, name 은 원본 함수 이름 getLineWidth, isCircle)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct ReflectedGetter {
	pub name: &'static str,
	pub value: ReflectValue,
}

//==============================================================================
// 반사 구현. (#[reflect] 를 붙인 impl 블록에서 매크로가 짓는다)
//==============================================================================
#[diagnostic::on_unimplemented(message = "`{Self}` 에 반사 구현이 없다", label = "반사 구현이 없는 타입", note = "`impl {Self}` 블록에 #[vanilla::reflect] 를 붙인다")]
pub trait Reflect {
	//==============================================================================
	// 타입 이름. (원본 constructor.name 자리, 등록하지 않은 컴포넌트를 문서에 쓸 때의 type)
	//==============================================================================
	fn get_reflected_type_name(&self) -> &'static str;

	//==============================================================================
	// 저장 속성 목록. (짝이 맞는 게터와 세터, 정의 차례. Component 공통 속성은 들지 않는다)
	//==============================================================================
	fn get_reflected_properties(&self) -> Vec<ReflectedProperty>;

	//==============================================================================
	// 속성 값 읽기. (원본 키, 없는 키면 None)
	//==============================================================================
	fn get_reflected_value(&self, property_name: &str) -> Option<ReflectValue>;

	//==============================================================================
	// 속성 값 넣기. (꼴이 맞지 않거나, 없는 키거나, 값 하나로 부를 수 없는 세터면 false)
	//==============================================================================
	fn set_reflected_value(&mut self, property_name: &str, value: &ReflectValue) -> bool;

	//==============================================================================
	// 게터 값 모으기. (인스펙터용, 세터가 없는 것까지 모든 게터. impl Component 블록에서 다시 정의한 것 → impl 블록 차례)
	//==============================================================================
	fn collect_reflected_getters(&self, getter_list: &mut Vec<ReflectedGetter>);
}

//==============================================================================
// impl Component 블록에서 다시 정의한 게터 모으기. (#[reflect] 를 붙인 impl Component 블록에서 매크로가 짓는다)
// - 원본 하위 클래스가 덮어쓴 isTickNeeded, isDrawnInsideContent 같은 것이다. 원본 인스펙터는 그 클래스의 함수로 보인다.
//==============================================================================
#[diagnostic::on_unimplemented(message = "`{Self}` 의 impl Component 블록에 #[reflect] 가 없다", label = "impl Component 블록에 #[reflect] 가 없는 타입", note = "`impl Component for {Self}` 블록에도 #[vanilla::reflect] 를 붙인다")]
pub trait ReflectComponentGetters {
	fn collect_component_getters(&self, getter_list: &mut Vec<ReflectedGetter>);
}

//==============================================================================
// 원본 Component 기반 클래스의 저장 속성. (isGizmoVisible / setGizmoVisible, isEnable / setEnable, getComponentType / setComponentType, 정의 차례)
//==============================================================================
pub const COMPONENT_REFLECTED_PROPERTIES: [ReflectedProperty; 3] = [
	ReflectedProperty {
		name: "GizmoVisible",
		getter_name: "is_gizmo_visible",
		setter_name: Some("set_gizmo_visible"),
	},
	ReflectedProperty {
		name: "Enable",
		getter_name: "is_enable",
		setter_name: Some("set_enable"),
	},
	ReflectedProperty {
		name: "ComponentType",
		getter_name: "get_component_type",
		setter_name: None,
	},
];

//==============================================================================
// 컴포넌트의 저장 속성 목록. (원본 collectReflectedProperties: 이 타입의 것 → Component 공통 속성, 같은 키는 앞의 것)
// - 반사가 없는 컴포넌트는 공통 속성만이다.
//==============================================================================
pub fn collect_reflected_properties(component: &dyn Component) -> Vec<ReflectedProperty> {
	let mut property_list = match component.get_reflection() {
		Some(reflection) => {
			reflection.get_reflected_properties()
		}
		None => {
			Vec::new()
		}
	};
	for base_property in COMPONENT_REFLECTED_PROPERTIES.iter() {
		if !property_list.iter().any(|property| property.name == base_property.name) {
			property_list.push(*base_property);
		}
	}
	property_list
}

//==============================================================================
// 컴포넌트의 속성 값 읽기. (원본 component[getterName](), 이 타입의 것이 먼저, 없으면 Component 공통 속성)
//==============================================================================
pub fn read_reflected_property(component: &dyn Component, property_name: &str) -> Option<ReflectValue> {
	if let Some(reflection) = component.get_reflection() {
		if let Some(value) = reflection.get_reflected_value(property_name) {
			return Some(value);
		}
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
// 컴포넌트의 속성 값 넣기. (원본 component[setterName](value), 넣었으면 true. ComponentType 은 러스트에 세터가 없어 false)
//==============================================================================
pub fn write_reflected_property(component: &mut dyn Component, property_name: &str, value: &ReflectValue) -> bool {
	if let Some(reflection) = component.get_reflection_mut() {
		if reflection.set_reflected_value(property_name, value) {
			return true;
		}
	}
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
// 컴포넌트의 게터 값 모두. (원본 getComponentProperties, Component 기반 클래스의 것은 빼고 그 타입의 것만. 반사가 없으면 None)
//==============================================================================
pub fn collect_reflected_getters(component: &dyn Component) -> Option<Vec<ReflectedGetter>> {
	let reflection = component.get_reflection()?;
	let mut getter_list: Vec<ReflectedGetter> = Vec::new();
	reflection.collect_reflected_getters(&mut getter_list);
	Some(getter_list)
}

//==============================================================================
// 게터 하나 더하기. (같은 이름이 이미 있으면 더하지 않는다. 원본 getComponentProperties 의 seenNames, 하위 클래스의 것이 먼저다)
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
// f32 를 가장 짧은 십진으로 읽은 f64. (0.22 는 0.22, 원본에서 같은 값을 자바스크립트 수로 적은 것과 같다)
//==============================================================================
pub fn read_shortest_number(number: f32) -> f64 {
	if !number.is_finite() {
		return number as f64;
	}
	format!("{}", number).parse::<f64>().unwrap_or(number as f64)
}

//==============================================================================
// 꼴의 짧은 이름. (모듈 경로와 감싼 꼴을 벗긴다. &alloc::rc::Rc<vanilla::core::image::Image> → Image)
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
// 게터 값 읽기의 자동 참조 특수화. (매크로가 (&&&ReflectGetProbe(&값)).read_reflect_value() 로 부른다)
// - 메서드를 찾을 때 참조를 하나씩 벗기며 맞는 구현을 고르는 것을 써서, 바꿀 수 있는 꼴(ToReflectValue) → 품은 꼴(Option, 배열) → 종류 이름 차례로 고른다.
//   바꿀 수 없는 꼴의 게터도 컴파일 에러 없이 원본처럼 저장에서 빠지고 인스펙터에는 [종류 이름] 으로 보인다.
//==============================================================================
pub struct ReflectGetProbe<'a, T: ?Sized>(pub &'a T);

//==============================================================================
// 바꿀 수 있는 꼴. (가장 먼저 고른다)
//==============================================================================
pub trait ReflectGetViaValue {
	fn read_reflect_value(&self) -> ReflectValue;
}

impl<'a, T: ToReflectValue + ?Sized> ReflectGetViaValue for &&ReflectGetProbe<'a, T> {
	fn read_reflect_value(&self) -> ReflectValue {
		self.0.to_reflect_value()
	}
}

//==============================================================================
// 품은 꼴. (Option 은 None 이면 Null 이고 Some 이면 안의 종류 이름, 배열은 Array. 원본 null 과 배열 자리)
//==============================================================================
pub trait ReflectOpaque {
	fn to_opaque_reflect_value(&self) -> ReflectValue;
}

impl<T> ReflectOpaque for Option<T> {
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
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(String::from("Array"))
	}
}

impl<T> ReflectOpaque for [T] {
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(String::from("Array"))
	}
}

impl<T, const N: usize> ReflectOpaque for [T; N] {
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(String::from("Array"))
	}
}

impl<T: ReflectOpaque + ?Sized> ReflectOpaque for &T {
	fn to_opaque_reflect_value(&self) -> ReflectValue {
		(**self).to_opaque_reflect_value()
	}
}

//==============================================================================
// 품은 꼴 고르기. (둘째로 고른다)
//==============================================================================
pub trait ReflectGetViaOpaque {
	fn read_reflect_value(&self) -> ReflectValue;
}

impl<'a, T: ReflectOpaque + ?Sized> ReflectGetViaOpaque for &ReflectGetProbe<'a, T> {
	fn read_reflect_value(&self) -> ReflectValue {
		self.0.to_opaque_reflect_value()
	}
}

//==============================================================================
// 종류 이름. (마지막으로 고른다. 원본 formatPropertyValue 의 [constructor.name] 자리)
//==============================================================================
pub trait ReflectGetViaName {
	fn read_reflect_value(&self) -> ReflectValue;
}

impl<'a, T: ?Sized> ReflectGetViaName for ReflectGetProbe<'a, T> {
	fn read_reflect_value(&self) -> ReflectValue {
		ReflectValue::Object(read_short_type_name::<T>())
	}
}

//==============================================================================
// 세터 값 읽기의 자동 참조 특수화. (매크로가 (&ReflectSetProbe::<세터 꼴>::new(값)).convert_reflect_value() 로 부른다)
// - 세터 꼴이 FromReflectValue 면 바꾸고, 아니면 늘 None 이라 그 속성은 읽기를 건너뛴다. (컴파일 에러가 없다)
//==============================================================================
pub struct ReflectSetProbe<'a, T> {
	value: &'a ReflectValue,
	target_type: PhantomData<fn() -> T>,
}

impl<'a, T> ReflectSetProbe<'a, T> {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(value: &'a ReflectValue) -> ReflectSetProbe<'a, T> {
		ReflectSetProbe {
			value,
			target_type: PhantomData,
		}
	}
}

//==============================================================================
// 읽을 수 있는 꼴. (먼저 고른다)
//==============================================================================
pub trait ReflectSetViaValue<T> {
	fn convert_reflect_value(&self) -> Option<T>;
}

impl<'a, T: FromReflectValue<'a>> ReflectSetViaValue<T> for ReflectSetProbe<'a, T> {
	fn convert_reflect_value(&self) -> Option<T> {
		T::from_reflect_value(self.value)
	}
}

//==============================================================================
// 읽을 수 없는 꼴. (늘 None)
//==============================================================================
pub trait ReflectSetViaNone<T> {
	fn convert_reflect_value(&self) -> Option<T>;
}

impl<'a, T> ReflectSetViaNone<T> for &ReflectSetProbe<'a, T> {
	fn convert_reflect_value(&self) -> Option<T> {
		None
	}
}

//==============================================================================
// 기본 꼴의 반사 값.
//==============================================================================
impl ToReflectValue for bool {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Boolean(*self)
	}
}

impl ToReflectValue for f32 {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Number(read_shortest_number(*self))
	}
}

impl ToReflectValue for f64 {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Number(*self)
	}
}

impl ToReflectValue for str {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::String(String::from(self))
	}
}

impl ToReflectValue for String {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::String(self.clone())
	}
}

impl ToReflectValue for Vector2 {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Vector2(*self)
	}
}

impl ToReflectValue for Color {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Color(*self)
	}
}

impl ToReflectValue for Rect {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Rect(*self)
	}
}

impl ToReflectValue for NodeId {
	fn to_reflect_value(&self) -> ReflectValue {
		ReflectValue::Node(*self)
	}
}

impl<T: ToReflectValue> ToReflectValue for Option<T> {
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
	fn to_reflect_value(&self) -> ReflectValue {
		(**self).to_reflect_value()
	}
}

impl<'a> FromReflectValue<'a> for bool {
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
// 배열의 수 하나. (f32)
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
// 정수 꼴의 반사 값. (원본 수, 읽을 때는 원본처럼 수를 그 꼴로 넣는다)
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
// 원본에서 글인 열거의 반사 값. (열거 정의 옆에서 부른다. 값과 원본 글의 짝을 모두 적거나, 이미 있는 짝 표 상수를 준다)
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
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	//==============================================================================
	// 시험용 열거.
	//==============================================================================
	#[derive(Clone, Copy, Debug, PartialEq)]
	enum Side {
		Left,
		Right,
	}

	impl_reflect_names!(Side, [(Side::Left, "left"), (Side::Right, "right")]);

	//==============================================================================
	// 게터 값 읽기를 매크로가 짓는 꼴 그대로 부른다.
	//==============================================================================
	fn read_probe<T>(value: T) -> ReflectValue {
		#[allow(unused_imports)]
		use super::{ReflectGetViaName as _, ReflectGetViaOpaque as _, ReflectGetViaValue as _};
		(&&&ReflectGetProbe(&value)).read_reflect_value()
	}

	//==============================================================================
	// 바꿀 수 있는 꼴은 값으로, Option 과 배열은 null 과 Array 로, 그 밖은 종류 이름으로 읽는다. (구체 꼴에서 특수화가 고른다)
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
	// 세터 꼴로 읽고, 읽을 수 없는 꼴은 None 이다.
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
	// 종류 이름은 경로와 감싼 꼴을 벗긴다.
	//==============================================================================
	#[test]
	fn short_type_name_strips_paths_and_wrappers() {
		assert_eq!(read_short_type_name::<Rc<RefCell<Color>>>(), "Color");
		assert_eq!(read_short_type_name::<&Vector2>(), "Vector2");
		assert_eq!(read_short_type_name::<Option<Color>>(), "Option<Color>");
		assert_eq!(read_short_type_name::<std::collections::HashMap<String, Color>>(), "HashMap<String, Color>");
	}
}
