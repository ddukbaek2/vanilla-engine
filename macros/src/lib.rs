//==============================================================================
//! 바닐라엔진 절차 매크로. (원본 vanilla.js 의 실행 중 반사 자리)
//! - 원본은 자바스크립트라 실행 중에 객체의 프로토타입을 훑어 get, is, set 함수를 찾는다.
//!   (ui/uidocument.js 의 collectReflectedProperties, misc/devtools.js 의 getComponentProperties)
//!   러스트에는 실행 중 반사가 없어, 언리얼의 UHT(UPROPERTY), Qt 의 moc 처럼 컴파일 때 코드를 지어 같은 일을 한다.
//! - `#[reflect]` 를 컴포넌트의 impl 블록에 붙이면 블록 안의 `pub fn get_*(&self) -> T`, `pub fn is_*(&self) -> T` 와
//!   그 짝 `pub fn set_*(&mut self, value)` 를 모아 vanilla::core::reflection::Reflect 구현을 짓는다.
//! - `#[reflect]` 를 impl Component 블록에 붙이면 그 블록에서 다시 정의한 게터(원본 하위 클래스가 덮어쓴 isTickNeeded 같은 것)를 인스펙터용으로 모은다.
//!   그 블록에 fn draw 가 없으면 Component::is_draw_overridden 을 false 로 짓는다. (그리지 않는 컴포넌트, 원본 draw === Component.prototype.draw)
//!   두 블록 모두 붙여야 한다. Reflect 가 Component 의 상위 트레이트라 어느 쪽을 빠뜨려도 컴파일 에러다. (원본처럼 모든 컴포넌트가 반사된다)
//!   게터가 없는 타입은 빈 블록 `#[reflect] impl 타입 {}` 에 붙인다.
//! - 원본 규칙을 그대로 따른다:
//!   - 게터는 인자 없는 `get*`, `is*` 함수다. (원본 `getterFunction.length === 0`) 저장 속성은 같은 키의 `set*` 이 있는 게터만이다. (원본 `"set" + baseName`)
//!     &mut self 게터도 게터다. (원본 함수에는 &self, &mut self 구별이 없다) 반사가 &mut 를 받는 갈래(get_reflected_value_mut, collect_reflected_getters_mut)로 부른다.
//!     러스트의 _mut 접근자(get_uiview_mut 같은 것)와 &mut 를 돌려주는 함수는 원본 게터가 아니라 뺀다.
//!   - 세터는 값 하나를 받는 `set*` 이다. 트리를 받고 값을 받는 세터(set_snap_current_index(트리, 칸) 같은 것)도 알아보고,
//!     트리를 쥔 쪽(UIDocument 읽기)이 set_reflected_value_in_tree 로 원본처럼 부른다.
//!   - 트리만 받는 게터(get_target_button(&self, 트리) 같은 것)도 원본 게터다. 원본은 이웃 컴포넌트를 바로 쥐지만 러스트 판은 트리에서 꺼낸다.
//!     트리를 쥔 쪽(DevTools 인스펙터)이 collect_reflected_getters_in_tree 로 부른다. 저장 속성은 아니다.
//!   - 키는 접두어(get, is)를 뗀 원본 이름이다. 러스트 snake_case 를 원본 camelCase 로 되돌린다. (get_line_width → getLineWidth → LineWidth)
//!   - 차례는 정의 차례, 같은 키는 먼저 나온 것이다.
//! - 값으로 바꿀 수 없는 꼴의 게터도 컴파일 에러를 내지 않는다. 자동 참조 특수화(autoref specialization)로 바꿀 수 있는 꼴만 값으로 읽고,
//!   나머지는 원본처럼 저장에서 건너뛰고 인스펙터에는 `[종류 이름]` 으로 보인다.
//! - 함수에 붙이는 것: `#[reflect(skip)]` 반사에서 뺀다. `#[reflect(transient)]` 인스펙터에는 보이되 속성 목록에서 뺀다.
//!   `#[reflect(unsaved)]` 속성 목록에는 두되 반사 저장과 읽기에서 뺀다. (원본에서 값이 객체라 saveByReflection 이 적지 않는 속성, ShadowText 의 FontFace)
//!   `#[reflect(name = "UIScale")]` 원본 키 이름을 정한다. (원본 이름의 머리글자 낱말처럼 snake_case 에서 되돌릴 수 없는 이름, 원본과 이름이 다른 게터)
//!   `#[reflect(after = "getFittedFontSize")]` impl Component 블록의 게터를 인스펙터 목록에서 그 원본 게터 뒤에 둔다. (원본 클래스 몸통의 차례, 없으면 맨 앞)
//! - 블록에 붙이는 것: `#[reflect(base = 필드)]` 품은 값의 반사를 원본 상위 클래스처럼 뒤에 잇는다.
//!   `#[reflect(crate = 경로)]` 엔진 경로. (기본 ::vanilla, 엔진 크레이트 안에서는 lib.rs 의 extern crate self as vanilla 로 같은 경로가 통한다)
//!
//! # 예제
//! impl 블록과 impl Component 블록에 붙이기 (ignore: 절차 매크로 크레이트의 doctest 는 엔진을 의존하지 못한다. 같은 예제를 엔진의 core::reflection 모듈 예제가 돌린다)
//! ```rust,ignore
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
//==============================================================================
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::parse::Parser;
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;
use syn::{Attribute, FnArg, GenericParam, Ident, ImplItem, ImplItemFn, ItemImpl, Lifetime, LitStr, Path, ReturnType, Type};

/// 인스펙터에도 보이지 않는 Component 트레이트의 게터. (공통 상태와 종류 이름은 원본 Component 기반 클래스 몫이고, 품은 기반 컴포넌트는 원본 상위 클래스 자리다)
const COMPONENT_BASE_GETTER_NAMES: [&str; 3] = ["get_component_base", "get_component_type", "get_base_component"];

//==============================================================================
/// 블록 옵션. (#[reflect(base = 필드, crate = 경로)])
//==============================================================================
struct BlockOptions {
	base_field: Option<Ident>,
	crate_path: Path,
}

//==============================================================================
/// 함수 옵션. (#[reflect(skip)], #[reflect(transient)], #[reflect(name = "...")])
//==============================================================================
#[derive(Default)]
struct MethodOptions {
	is_skipped: bool,
	is_transient: bool,
	is_unsaved: bool,
	reflected_name: Option<String>,
	after_name: Option<String>,
}

//==============================================================================
/// 모은 게터 하나.
//==============================================================================
struct GetterMethod {
	method_name: Ident,
	reflected_name: String,
	original_name: String,
	return_type: Type,
	cfg_attributes: Vec<Attribute>,
	is_transient: bool,
	is_unsaved: bool,
	is_mutable: bool,
	takes_tree: bool,
	after_name: Option<String>,
}

//==============================================================================
/// 모은 세터 하나. (value_type 이 None 이면 값 하나로 부를 수 없는 세터다. 트리 말고도 인자가 더 있거나 제네릭)
/// - takes_tree 면 트리를 먼저 받고 값을 받는 세터다. (set_x(&mut self, node_tree: &mut NodeTree, value))
//==============================================================================
struct SetterMethod {
	method_name: Ident,
	reflected_name: String,
	value_type: Option<Type>,
	is_impl_trait: bool,
	takes_tree: bool,
	cfg_attributes: Vec<Attribute>,
}

//==============================================================================
/// 반사 매크로. (impl 블록에 붙인다. 트레이트 impl 이면 impl Component 블록으로 본다)
/// - 게임은 엔진이 다시 내보낸 vanilla::reflect 로 쓴다. (`use vanilla::reflect;` 뒤 `#[reflect]`, 또는 `#[vanilla::reflect]`)
/// - 컴포넌트의 impl 블록과 impl Component 블록 둘 다에 붙인다. 한쪽만 붙이면 컴파일 에러가 붙일 곳을 알려 준다.
/// - 블록 옵션은 `base = 필드`, `crate = 경로` 이고, 함수 옵션은 `skip`, `transient`, `unsaved`, `name = "..."`, `after = "..."` 이다. (impl Component 블록에는 base 를 쓰지 않는다)
///
/// # 예제
/// 함수 옵션과 품은 값 (ignore: 절차 매크로 크레이트의 doctest 는 엔진을 의존하지 못한다. 같은 예제를 엔진의 core::reflection 모듈 예제가 돌린다)
/// ```rust,ignore
/// use vanilla::core::reflection::{collect_reflected_getters, collect_reflected_properties};
/// use vanilla::{reflect, Component, ComponentBase};
///
/// struct Meter {
/// 	component_base: ComponentBase,
/// 	value: f32,
/// }
///
/// #[reflect]
/// impl Meter {
/// 	pub fn get_value(&self) -> f32 {
/// 		self.value
/// 	}
///
/// 	pub fn set_value(&mut self, value: f32) {
/// 		self.value = value;
/// 	}
/// }
///
/// #[reflect]
/// impl Component for Meter {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"Meter"
/// 	}
/// }
///
/// struct LabeledMeter {
/// 	component_base: ComponentBase,
/// 	meter: Meter,
/// 	label: String,
/// }
///
/// #[reflect(base = meter)]
/// impl LabeledMeter {
/// 	#[reflect(name = "Caption")]
/// 	pub fn get_label(&self) -> &str {
/// 		&self.label
/// 	}
///
/// 	#[reflect(name = "Caption")]
/// 	pub fn set_label(&mut self, label: &str) {
/// 		self.label = String::from(label);
/// 	}
///
/// 	#[reflect(transient)]
/// 	pub fn get_label_length(&self) -> usize {
/// 		self.label.len()
/// 	}
///
/// 	#[reflect(skip)]
/// 	pub fn get_meter(&self) -> &Meter {
/// 		&self.meter
/// 	}
/// }
///
/// #[reflect]
/// impl Component for LabeledMeter {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"LabeledMeter"
/// 	}
/// }
///
/// let labeled_meter = LabeledMeter {
/// 	component_base: ComponentBase::new(),
/// 	meter: Meter {
/// 		component_base: ComponentBase::new(),
/// 		value: 0.5,
/// 	},
/// 	label: String::from("hp"),
/// };
/// let property_names: Vec<&str> = collect_reflected_properties(&labeled_meter).iter().map(|property| property.name).collect();
/// assert_eq!(property_names, ["Caption", "Value", "GizmoVisible", "Enable", "ComponentType"]);
/// let getter_names: Vec<&str> = collect_reflected_getters(&labeled_meter).expect("reflection").iter().map(|getter| getter.name).collect();
/// assert_eq!(getter_names, ["getCaption", "getLabelLength", "getValue"]);
/// ```
//==============================================================================
#[proc_macro_attribute]
pub fn reflect(attribute: TokenStream, item: TokenStream) -> TokenStream {
	let block_options = match parse_block_options(attribute.into()) {
		Ok(block_options) => {
			block_options
		}
		Err(error) => {
			return error.to_compile_error().into();
		}
	};
	let item_impl = match syn::parse::<ItemImpl>(item) {
		Ok(item_impl) => {
			item_impl
		}
		Err(error) => {
			return syn::Error::new(error.span(), "#[reflect] 는 impl 블록에 붙인다").to_compile_error().into();
		}
	};
	let result = if item_impl.trait_.is_some() {
		expand_component_impl(&block_options, item_impl)
	}
	else {
		expand_inherent_impl(&block_options, item_impl)
	};
	match result {
		Ok(token_stream) => {
			token_stream.into()
		}
		Err(error) => {
			error.to_compile_error().into()
		}
	}
}

//==============================================================================
/// 블록 옵션 읽기.
//==============================================================================
fn parse_block_options(attribute: TokenStream2) -> syn::Result<BlockOptions> {
	let mut base_field: Option<Ident> = None;
	let mut crate_path: Path = syn::parse_quote!(::vanilla);
	let option_parser = syn::meta::parser(|meta| {
		if meta.path.is_ident("base") {
			base_field = Some(meta.value()?.parse()?);
			Ok(())
		}
		else if meta.path.is_ident("crate") {
			crate_path = meta.value()?.parse()?;
			Ok(())
		}
		else {
			Err(meta.error("impl 블록의 #[reflect] 는 base = 필드, crate = 경로 만 받는다"))
		}
	});
	option_parser.parse2(attribute)?;
	Ok(BlockOptions {
		base_field,
		crate_path,
	})
}

//==============================================================================
/// 함수의 #[reflect(...)] 를 읽고 지운다. (지우지 않으면 컴파일러가 함수에 붙은 매크로로 다시 부른다)
//==============================================================================
fn take_method_options(attributes: &mut Vec<Attribute>) -> syn::Result<MethodOptions> {
	let mut method_options = MethodOptions::default();
	let mut kept_attributes: Vec<Attribute> = Vec::new();
	for attribute in attributes.drain(..) {
		let is_reflect_attribute = attribute.path().segments.last().map(|segment| segment.ident == "reflect").unwrap_or(false);
		if !is_reflect_attribute {
			kept_attributes.push(attribute);
			continue;
		}
		attribute.parse_nested_meta(|meta| {
			if meta.path.is_ident("skip") {
				method_options.is_skipped = true;
				Ok(())
			}
			else if meta.path.is_ident("transient") {
				method_options.is_transient = true;
				Ok(())
			}
			else if meta.path.is_ident("unsaved") {
				method_options.is_unsaved = true;
				Ok(())
			}
			else if meta.path.is_ident("name") {
				let reflected_name: LitStr = meta.value()?.parse()?;
				method_options.reflected_name = Some(reflected_name.value());
				Ok(())
			}
			else if meta.path.is_ident("after") {
				let after_name: LitStr = meta.value()?.parse()?;
				method_options.after_name = Some(after_name.value());
				Ok(())
			}
			else {
				Err(meta.error("함수의 #[reflect] 는 skip, transient, unsaved, name = \"...\", after = \"...\" 만 받는다"))
			}
		})?;
	}
	*attributes = kept_attributes;
	Ok(method_options)
}

//==============================================================================
/// cfg 속성 모으기. (지은 코드도 함수와 같은 조건에서만 들어가야 한다)
//==============================================================================
fn collect_cfg_attributes(attributes: &[Attribute]) -> Vec<Attribute> {
	attributes.iter().filter(|attribute| attribute.path().is_ident("cfg")).cloned().collect()
}

//==============================================================================
/// cfg 속성의 비교용 글. (같은 이름이 다른 조건으로 둘 있으면 둘 다 짓는다)
//==============================================================================
fn read_cfg_text(cfg_attributes: &[Attribute]) -> String {
	cfg_attributes.iter().map(|attribute| attribute.to_token_stream().to_string()).collect::<Vec<String>>().join(" ")
}

//==============================================================================
/// snake_case 접미어 → 원본 이름. (낱말마다 머리를 대문자로 이어 붙인다. line_width → LineWidth)
//==============================================================================
fn convert_suffix_to_reflected_name(suffix: &str) -> String {
	let mut reflected_name = String::new();
	for word in suffix.split('_') {
		let mut characters = word.chars();
		if let Some(first_character) = characters.next() {
			reflected_name.extend(first_character.to_uppercase());
			reflected_name.push_str(characters.as_str());
		}
	}
	reflected_name
}

//==============================================================================
/// 함수 이름을 접두어와 접미어로 나누기. (get_, is_, set_ 가운데 하나, 접미어가 비면 None)
//==============================================================================
fn split_method_name<'a>(method_name: &'a str, prefixes: &[&'static str]) -> Option<(&'static str, &'a str)> {
	for prefix in prefixes.iter() {
		if let Some(suffix) = method_name.strip_prefix(prefix) {
			if !suffix.is_empty() {
				return Some((prefix, suffix));
			}
		}
	}
	None
}

//==============================================================================
/// 부를 수 있는 꼴의 함수인지. (unsafe, async, extern, 타입이나 상수 제네릭이 없다)
//==============================================================================
fn is_plain_signature(signature: &syn::Signature) -> bool {
	if signature.unsafety.is_some() || signature.asyncness.is_some() || signature.abi.is_some() || signature.variadic.is_some() {
		return false;
	}
	signature.generics.params.iter().all(|generic_param| matches!(generic_param, GenericParam::Lifetime(_)))
}

//==============================================================================
/// 받는 꼴. (&self 면 Some(false), &mut self 면 Some(true), 그 밖은 None)
//==============================================================================
fn read_receiver_mutability(signature: &syn::Signature) -> Option<bool> {
	match signature.inputs.first() {
		Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() && receiver.colon_token.is_none() => {
			Some(receiver.mutability.is_some())
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 돌려주는 꼴. (없거나 () 나 ! 면 None)
//==============================================================================
fn read_return_type(signature: &syn::Signature) -> Option<Type> {
	match &signature.output {
		ReturnType::Default => {
			None
		}
		ReturnType::Type(_, return_type) => {
			match return_type.as_ref() {
				Type::Tuple(tuple_type) if tuple_type.elems.is_empty() => {
					None
				}
				Type::Never(_) => {
					None
				}
				_ => {
					Some(return_type.as_ref().clone())
				}
			}
		}
	}
}

//==============================================================================
/// 꼴 안의 수명을 '_ 로. (함수의 수명 매개변수는 지은 코드에서 보이지 않는다. 'static 은 둔다)
//==============================================================================
struct LifetimeEraser;

impl VisitMut for LifetimeEraser {
	//==============================================================================
	/// 수명 하나 바꾸기. ('static 이 아니면 '_ 로)
	//==============================================================================
	fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
		if lifetime.ident != "static" {
			*lifetime = Lifetime::new("'_", lifetime.span());
		}
	}
}

//==============================================================================
/// 수명을 지운 꼴.
//==============================================================================
fn erase_lifetimes(type_value: &Type) -> Type {
	let mut erased_type = type_value.clone();
	LifetimeEraser.visit_type_mut(&mut erased_type);
	erased_type
}

//==============================================================================
/// impl Trait 이 든 꼴인지. (이름을 쓸 수 없어 지은 코드에 옮길 수 없다)
//==============================================================================
struct ImplTraitFinder {
	is_found: bool,
}

impl<'a> syn::visit::Visit<'a> for ImplTraitFinder {
	//==============================================================================
	/// impl Trait 꼴을 만나면 찾았다고 적는다.
	//==============================================================================
	fn visit_type_impl_trait(&mut self, _type_impl_trait: &'a syn::TypeImplTrait) {
		self.is_found = true;
	}
}

//==============================================================================
/// impl Trait 이 든 꼴인지 보기.
//==============================================================================
fn contains_impl_trait(type_value: &Type) -> bool {
	let mut impl_trait_finder = ImplTraitFinder {
		is_found: false,
	};
	syn::visit::Visit::visit_type(&mut impl_trait_finder, type_value);
	impl_trait_finder.is_found
}

//==============================================================================
/// 게터 읽기. (인자 없는 get_*, is_* 이고 &self 를 받고 무언가 돌려주는 함수)
//==============================================================================
fn read_getter(method: &ImplItemFn, method_options: &MethodOptions) -> Option<GetterMethod> {
	let signature = &method.sig;
	let method_text = signature.ident.to_string();
	let (prefix, suffix) = split_method_name(&method_text, &["get_", "is_"])?;
	if !is_plain_signature(signature) {
		return None;
	}
	let takes_tree = match signature.inputs.iter().nth(1) {
		Some(FnArg::Typed(pattern_type)) if signature.inputs.len() == 2 && is_node_tree_shared_reference(&pattern_type.ty) => {
			true
		}
		_ => {
			false
		}
	};
	if signature.inputs.len() != 1 && !takes_tree {
		return None;
	}
	let is_mutable = read_receiver_mutability(signature)?;
	if is_mutable && takes_tree {
		return None;
	}
	let return_type = read_return_type(signature)?;
	// 러스트의 _mut 접근자와 &mut 를 돌려주는 함수는 원본 게터가 아니다.
	let is_mutable_reference_return = matches!(&return_type, Type::Reference(reference_type) if reference_type.mutability.is_some());
	if is_mutable && (method_text.ends_with("_mut") || is_mutable_reference_return) {
		return None;
	}
	let reflected_name = match &method_options.reflected_name {
		Some(reflected_name) => {
			reflected_name.clone()
		}
		None => {
			convert_suffix_to_reflected_name(suffix)
		}
	};
	let original_prefix = if prefix == "get_" {
		"get"
	}
	else {
		"is"
	};
	Some(GetterMethod {
		method_name: signature.ident.clone(),
		original_name: format!("{}{}", original_prefix, reflected_name),
		reflected_name,
		return_type,
		cfg_attributes: collect_cfg_attributes(&method.attrs),
		is_transient: method_options.is_transient,
		is_unsaved: method_options.is_unsaved,
		is_mutable,
		takes_tree,
		after_name: method_options.after_name.clone(),
	})
}

//==============================================================================
/// &NodeTree 꼴인지. (트리를 받는 게터의 인자)
//==============================================================================
fn is_node_tree_shared_reference(type_value: &Type) -> bool {
	match type_value {
		Type::Reference(reference_type) if reference_type.mutability.is_none() => {
			match reference_type.elem.as_ref() {
				Type::Path(type_path) => {
					type_path.path.segments.last().map(|segment| segment.ident == "NodeTree").unwrap_or(false)
				}
				_ => {
					false
				}
			}
		}
		_ => {
			false
		}
	}
}

//==============================================================================
/// &mut NodeTree 꼴인지. (트리를 받는 세터의 첫 인자)
//==============================================================================
fn is_node_tree_reference(type_value: &Type) -> bool {
	match type_value {
		Type::Reference(reference_type) if reference_type.mutability.is_some() => {
			match reference_type.elem.as_ref() {
				Type::Path(type_path) => {
					type_path.path.segments.last().map(|segment| segment.ident == "NodeTree").unwrap_or(false)
				}
				_ => {
					false
				}
			}
		}
		_ => {
			false
		}
	}
}

//==============================================================================
/// 세터 읽기. (set_* 이고 &mut self 를 받는 함수. 값 하나를 받으면 그 꼴을 든다)
//==============================================================================
fn read_setter(method: &ImplItemFn, method_options: &MethodOptions) -> Option<SetterMethod> {
	let signature = &method.sig;
	let method_text = signature.ident.to_string();
	let (_, suffix) = split_method_name(&method_text, &["set_"])?;
	if read_receiver_mutability(signature) != Some(true) {
		return None;
	}
	let mut value_type: Option<Type> = None;
	let mut is_impl_trait = false;
	let mut takes_tree = false;
	let parameter_types: Vec<&Type> = signature.inputs.iter().skip(1).filter_map(|input| {
		match input {
			FnArg::Typed(pattern_type) => {
				Some(pattern_type.ty.as_ref())
			}
			FnArg::Receiver(_) => {
				None
			}
		}
	}).collect();
	let value_parameter_type = match parameter_types.as_slice() {
		[value_parameter_type] => {
			Some(*value_parameter_type)
		}
		[tree_parameter_type, value_parameter_type] if is_node_tree_reference(tree_parameter_type) => {
			takes_tree = true;
			Some(*value_parameter_type)
		}
		_ => {
			None
		}
	};
	if let Some(value_parameter_type) = value_parameter_type {
		if is_plain_signature(signature) {
			if contains_impl_trait(value_parameter_type) {
				is_impl_trait = true;
			}
			else {
				value_type = Some(erase_lifetimes(value_parameter_type));
			}
		}
	}
	let takes_tree = takes_tree && (value_type.is_some() || is_impl_trait);
	let reflected_name = match &method_options.reflected_name {
		Some(reflected_name) => {
			reflected_name.clone()
		}
		None => {
			convert_suffix_to_reflected_name(suffix)
		}
	};
	Some(SetterMethod {
		method_name: signature.ident.clone(),
		reflected_name,
		value_type,
		is_impl_trait,
		takes_tree,
		cfg_attributes: collect_cfg_attributes(&method.attrs),
	})
}

//==============================================================================
/// impl 대상 타입의 이름. (원본 constructor.name 자리, 경로의 끝 이름)
//==============================================================================
fn read_type_name(self_type: &Type) -> syn::Result<String> {
	match self_type {
		Type::Path(type_path) => {
			match type_path.path.segments.last() {
				Some(segment) => {
					Ok(segment.ident.to_string())
				}
				None => {
					Err(syn::Error::new(self_type.span(), "#[reflect] 는 이름 있는 타입의 impl 블록에 붙인다"))
				}
			}
		}
		_ => {
			Err(syn::Error::new(self_type.span(), "#[reflect] 는 이름 있는 타입의 impl 블록에 붙인다"))
		}
	}
}

//==============================================================================
/// 게터 값을 반사 값으로 읽는 식. (바꿀 수 있는 꼴 → 품은 꼴(Option, 배열) → 종류 이름 차례로 고른다)
//==============================================================================
fn create_read_expression(reflect_path: &TokenStream2, call_expression: TokenStream2) -> TokenStream2 {
	quote! {
		{
			#[allow(unused_imports)]
			use #reflect_path::{ReflectGetViaName as _, ReflectGetViaOpaque as _, ReflectGetViaValue as _};
			let reflected_value = #call_expression;
			(&&&#reflect_path::ReflectGetProbe(&reflected_value)).read_reflect_value()
		}
	}
}

//==============================================================================
/// impl 블록 펼치기. (블록은 그대로 두고 Reflect 구현을 덧붙인다)
//==============================================================================
fn expand_inherent_impl(block_options: &BlockOptions, mut item_impl: ItemImpl) -> syn::Result<TokenStream2> {
	let crate_path = &block_options.crate_path;
	let reflect_path = quote!(#crate_path::core::reflection);
	let node_tree_path = quote!(#crate_path::core::node::nodetree::NodeTree);
	let type_name = read_type_name(&item_impl.self_ty)?;

	// 게터와 세터 모으기. (pub 함수만, 원본 # 비공개 함수는 프로토타입에 보이지 않는다)
	let mut getter_list: Vec<GetterMethod> = Vec::new();
	let mut setter_list: Vec<SetterMethod> = Vec::new();
	for impl_item in item_impl.items.iter_mut() {
		let method = match impl_item {
			ImplItem::Fn(method) => {
				method
			}
			_ => {
				continue;
			}
		};
		let method_options = take_method_options(&mut method.attrs)?;
		if method_options.after_name.is_some() {
			return Err(syn::Error::new(method.sig.ident.span(), "after = \"...\" 는 impl Component 블록의 게터에 쓴다"));
		}
		if method_options.is_skipped || !matches!(method.vis, syn::Visibility::Public(_)) {
			continue;
		}
		if let Some(getter) = read_getter(method, &method_options) {
			getter_list.push(getter);
		}
		else if let Some(setter) = read_setter(method, &method_options) {
			setter_list.push(setter);
		}
	}

	// 저장 속성. (짝 세터가 있는 게터, 정의 차례, 같은 키와 같은 조건이면 먼저 것)
	let mut property_key_list: Vec<(String, String)> = Vec::new();
	let mut property_push_list: Vec<TokenStream2> = Vec::new();
	for getter in getter_list.iter() {
		if getter.is_transient || getter.takes_tree {
			continue;
		}
		for setter in setter_list.iter().filter(|setter| setter.reflected_name == getter.reflected_name) {
			let mut cfg_attributes = getter.cfg_attributes.clone();
			cfg_attributes.extend(setter.cfg_attributes.iter().cloned());
			let property_key = (getter.reflected_name.clone(), read_cfg_text(&cfg_attributes));
			if property_key_list.contains(&property_key) {
				continue;
			}
			property_key_list.push(property_key);
			let reflected_name = &getter.reflected_name;
			let getter_text = getter.method_name.to_string();
			let setter_text = setter.method_name.to_string();
			let is_saved = !getter.is_unsaved;
			property_push_list.push(quote! {
				#(#cfg_attributes)*
				property_list.push(#reflect_path::ReflectedProperty {
					name: #reflected_name,
					getter_name: #getter_text,
					setter_name: ::core::option::Option::Some(#setter_text),
					is_saved: #is_saved,
				});
			});
		}
	}

	// 읽기. (이 블록의 게터 모두, 같은 키면 먼저 것. 원본 component[getterName] 처럼 가장 아래 타입의 것이 먼저다. &mut self 게터는 &mut 를 받는 갈래에서만 부른다)
	let mut read_key_list: Vec<(String, String)> = Vec::new();
	let mut read_arm_list: Vec<TokenStream2> = Vec::new();
	let mut read_mutable_arm_list: Vec<TokenStream2> = Vec::new();
	for getter in getter_list.iter().filter(|getter| !getter.takes_tree) {
		let read_key = (getter.reflected_name.clone(), read_cfg_text(&getter.cfg_attributes));
		if read_key_list.contains(&read_key) {
			continue;
		}
		read_key_list.push(read_key);
		let cfg_attributes = &getter.cfg_attributes;
		let reflected_name = &getter.reflected_name;
		let getter_name = &getter.method_name;
		let read_expression = create_read_expression(&reflect_path, quote!(self.#getter_name()));
		if getter.is_mutable {
			read_arm_list.push(quote! {
				#(#cfg_attributes)*
				#reflected_name => {
					::core::option::Option::None
				}
			});
		}
		else {
			read_arm_list.push(quote! {
				#(#cfg_attributes)*
				#reflected_name => {
					::core::option::Option::Some(#read_expression)
				}
			});
		}
		read_mutable_arm_list.push(quote! {
			#(#cfg_attributes)*
			#reflected_name => {
				::core::option::Option::Some(#read_expression)
			}
		});
	}

	// 넣기. (이 블록의 세터 모두, 같은 키면 먼저 것. 원본 component[setterName] 처럼 게터가 상위 클래스에 있어도 가장 아래 타입의 세터가 먼저다)
	// - 키는 세터 이름에서 되돌린 것(이름을 정했으면 그 이름)이다. 같은 키의 게터가 짝이다. (원본 "set" + baseName)
	// - 세터가 impl Trait 을 받으면 짝 게터 꼴로 바꿔 넘긴다. (set_color(impl Into<Color>) 에 get_color 의 Color)
	let mut write_key_list: Vec<(String, String)> = Vec::new();
	let mut write_arm_list: Vec<TokenStream2> = Vec::new();
	let mut write_in_tree_arm_list: Vec<TokenStream2> = Vec::new();
	for setter in setter_list.iter() {
		let paired_getter = getter_list.iter().find(|getter| getter.reflected_name == setter.reflected_name && !getter.takes_tree);
		let reflected_name = setter.reflected_name.clone();
		let write_key = (reflected_name.clone(), read_cfg_text(&setter.cfg_attributes));
		if write_key_list.contains(&write_key) {
			continue;
		}
		write_key_list.push(write_key);
		let cfg_attributes = &setter.cfg_attributes;
		let value_type = match (setter.is_impl_trait, paired_getter) {
			(true, Some(paired_getter)) if !contains_impl_trait(&paired_getter.return_type) => {
				Some(erase_lifetimes(&paired_getter.return_type))
			}
			_ => {
				setter.value_type.clone()
			}
		};
		let setter_name = &setter.method_name;
		let value_type = match value_type {
			Some(value_type) => {
				value_type
			}
			None => {
				let false_arm = quote! {
					#(#cfg_attributes)*
					#reflected_name => {
						false
					}
				};
				write_arm_list.push(false_arm.clone());
				write_in_tree_arm_list.push(false_arm);
				continue;
			}
		};
		let create_write_arm = |call_expression: TokenStream2| -> TokenStream2 {
			quote! {
				#(#cfg_attributes)*
				#reflected_name => {
					#[allow(unused_imports)]
					use #reflect_path::{ReflectSetViaNone as _, ReflectSetViaValue as _};
					match (&#reflect_path::ReflectSetProbe::<#value_type>::new(value)).convert_reflect_value() {
						::core::option::Option::Some(converted_value) => {
							let _ = #call_expression;
							true
						}
						::core::option::Option::None => {
							false
						}
					}
				}
			}
		};
		if setter.takes_tree {
			write_arm_list.push(quote! {
				#(#cfg_attributes)*
				#reflected_name => {
					false
				}
			});
			write_in_tree_arm_list.push(create_write_arm(quote!(self.#setter_name(node_tree, converted_value))));
		}
		else {
			write_arm_list.push(create_write_arm(quote!(self.#setter_name(converted_value))));
			write_in_tree_arm_list.push(create_write_arm(quote!(self.#setter_name(converted_value))));
		}
	}

	// 인스펙터 게터. (모든 게터, 정의 차례. &self 갈래는 &mut self 게터와 트리를 받는 게터를 빼고, 트리 갈래는 &mut self 게터를 뺀다)
	let mut getter_key_list: Vec<(String, String)> = Vec::new();
	let mut getter_push_list: Vec<TokenStream2> = Vec::new();
	let mut getter_in_tree_push_list: Vec<TokenStream2> = Vec::new();
	let mut getter_mutable_push_list: Vec<TokenStream2> = Vec::new();
	let mut has_mutable_getter = false;
	for getter in getter_list.iter() {
		let getter_key = (getter.original_name.clone(), read_cfg_text(&getter.cfg_attributes));
		if getter_key_list.contains(&getter_key) {
			continue;
		}
		getter_key_list.push(getter_key);
		let cfg_attributes = &getter.cfg_attributes;
		let original_name = &getter.original_name;
		let getter_name = &getter.method_name;
		let call_expression = if getter.takes_tree {
			quote!(self.#getter_name(node_tree))
		}
		else {
			quote!(self.#getter_name())
		};
		let read_expression = create_read_expression(&reflect_path, call_expression);
		let push_statement = quote! {
			#(#cfg_attributes)*
			#reflect_path::push_reflected_getter(getter_list, #original_name, #read_expression);
		};
		if getter.is_mutable {
			has_mutable_getter = true;
		}
		else {
			if !getter.takes_tree {
				getter_push_list.push(push_statement.clone());
			}
			getter_in_tree_push_list.push(push_statement.clone());
		}
		getter_mutable_push_list.push(push_statement);
	}

	// 품은 값. (원본 상위 클래스 자리, 이 타입에 없는 키만 뒤에 잇는다)
	let base_field = &block_options.base_field;
	let base_property_extension = match base_field {
		Some(base_field) => {
			quote! {
				for base_property in #reflect_path::Reflect::get_reflected_properties(&self.#base_field) {
					if !property_list.iter().any(|property: &#reflect_path::ReflectedProperty| property.name == base_property.name) {
						property_list.push(base_property);
					}
				}
			}
		}
		None => {
			quote!()
		}
	};
	let create_base_call = |base_call: TokenStream2, fallback: TokenStream2| -> TokenStream2 {
		match base_field {
			Some(_) => {
				base_call
			}
			None => {
				fallback
			}
		}
	};
	let base_field_tokens = quote!(#base_field);
	let base_read_fallback = create_base_call(quote!(#reflect_path::Reflect::get_reflected_value(&self.#base_field_tokens, property_name)), quote!(::core::option::Option::None));
	let base_read_mutable_fallback = create_base_call(quote!(#reflect_path::Reflect::get_reflected_value_mut(&mut self.#base_field_tokens, property_name)), quote!(::core::option::Option::None));
	let base_write_fallback = create_base_call(quote!(#reflect_path::Reflect::set_reflected_value(&mut self.#base_field_tokens, property_name, value)), quote!(false));
	let base_write_in_tree_fallback = create_base_call(quote!(#reflect_path::Reflect::set_reflected_value_in_tree(&mut self.#base_field_tokens, node_tree, property_name, value)), quote!(false));
	let base_has_mutable_getters = create_base_call(quote!(|| #reflect_path::Reflect::has_mutable_getters(&self.#base_field_tokens)), quote!());
	let create_base_getter_extension = |collect_call: TokenStream2| -> TokenStream2 {
		match base_field {
			Some(_) => {
				quote! {
					let mut base_getter_list: ::std::vec::Vec<#reflect_path::ReflectedGetter> = ::std::vec::Vec::new();
					#collect_call;
					for base_getter in base_getter_list {
						#reflect_path::push_reflected_getter(getter_list, base_getter.name, base_getter.value);
					}
				}
			}
			None => {
				quote!()
			}
		}
	};
	let base_getter_extension = create_base_getter_extension(quote!(#reflect_path::Reflect::collect_reflected_getters(&self.#base_field_tokens, &mut base_getter_list)));
	let base_getter_in_tree_extension = create_base_getter_extension(quote!(#reflect_path::Reflect::collect_reflected_getters_in_tree(&self.#base_field_tokens, node_tree, &mut base_getter_list)));
	let base_getter_mutable_extension = create_base_getter_extension(quote!(#reflect_path::Reflect::collect_reflected_getters_mut(&mut self.#base_field_tokens, node_tree, &mut base_getter_list)));

	let self_type = &item_impl.self_ty;
	let (impl_generics, _, where_clause) = item_impl.generics.split_for_impl();
	Ok(quote! {
		#item_impl

		#[allow(deprecated)]
		impl #impl_generics #reflect_path::Reflect for #self_type #where_clause {
			fn get_reflected_type_name(&self) -> &'static str {
				#type_name
			}

			fn get_reflected_properties(&self) -> ::std::vec::Vec<#reflect_path::ReflectedProperty> {
				#[allow(unused_mut)]
				let mut property_list: ::std::vec::Vec<#reflect_path::ReflectedProperty> = ::std::vec::Vec::new();
				#(#property_push_list)*
				#base_property_extension
				property_list
			}

			#[allow(unused_variables)]
			fn get_reflected_value(&self, property_name: &str) -> ::core::option::Option<#reflect_path::ReflectValue> {
				match property_name {
					#(#read_arm_list)*
					_ => {
						#base_read_fallback
					}
				}
			}

			#[allow(unused_variables)]
			fn get_reflected_value_mut(&mut self, property_name: &str) -> ::core::option::Option<#reflect_path::ReflectValue> {
				match property_name {
					#(#read_mutable_arm_list)*
					_ => {
						#base_read_mutable_fallback
					}
				}
			}

			#[allow(unused_variables)]
			fn set_reflected_value(&mut self, property_name: &str, value: &#reflect_path::ReflectValue) -> bool {
				match property_name {
					#(#write_arm_list)*
					_ => {
						#base_write_fallback
					}
				}
			}

			#[allow(unused_variables)]
			fn set_reflected_value_in_tree(&mut self, node_tree: &mut #node_tree_path, property_name: &str, value: &#reflect_path::ReflectValue) -> bool {
				match property_name {
					#(#write_in_tree_arm_list)*
					_ => {
						#base_write_in_tree_fallback
					}
				}
			}

			fn has_mutable_getters(&self) -> bool {
				#has_mutable_getter #base_has_mutable_getters
			}

			fn collect_reflected_getters(&self, getter_list: &mut ::std::vec::Vec<#reflect_path::ReflectedGetter>) {
				let mut anchored_getter_list: ::std::vec::Vec<(&'static str, #reflect_path::ReflectedGetter)> = ::std::vec::Vec::new();
				#reflect_path::ReflectComponentGetters::collect_component_getters(self, getter_list, &mut anchored_getter_list);
				#(#getter_push_list)*
				#reflect_path::insert_anchored_getters(getter_list, anchored_getter_list);
				#base_getter_extension
			}

			#[allow(unused_variables)]
			fn collect_reflected_getters_in_tree(&self, node_tree: &#node_tree_path, getter_list: &mut ::std::vec::Vec<#reflect_path::ReflectedGetter>) {
				let mut anchored_getter_list: ::std::vec::Vec<(&'static str, #reflect_path::ReflectedGetter)> = ::std::vec::Vec::new();
				#reflect_path::ReflectComponentGetters::collect_component_getters(self, getter_list, &mut anchored_getter_list);
				#(#getter_in_tree_push_list)*
				#reflect_path::insert_anchored_getters(getter_list, anchored_getter_list);
				#base_getter_in_tree_extension
			}

			#[allow(unused_variables)]
			fn collect_reflected_getters_mut(&mut self, node_tree: &#node_tree_path, getter_list: &mut ::std::vec::Vec<#reflect_path::ReflectedGetter>) {
				let mut anchored_getter_list: ::std::vec::Vec<(&'static str, #reflect_path::ReflectedGetter)> = ::std::vec::Vec::new();
				#reflect_path::ReflectComponentGetters::collect_component_getters(&*self, getter_list, &mut anchored_getter_list);
				#(#getter_mutable_push_list)*
				#reflect_path::insert_anchored_getters(getter_list, anchored_getter_list);
				#base_getter_mutable_extension
			}
		}
	})
}

//==============================================================================
/// impl Component 블록 펼치기. (이 블록에서 다시 정의한 게터를 모으는 구현을 덧붙인다)
/// - 원본 클래스에 없는 덮어쓰기(러스트 판의 is_tick_needed 같은 것)는 #[reflect(skip)] 으로 빼고,
///   원본 클래스 몸통에서 다른 게터 뒤에 있던 것은 #[reflect(after = "원본 게터 이름")] 로 그 자리에 둔다.
//==============================================================================
fn expand_component_impl(block_options: &BlockOptions, mut item_impl: ItemImpl) -> syn::Result<TokenStream2> {
	let crate_path = &block_options.crate_path;
	let reflect_path = quote!(#crate_path::core::reflection);
	if block_options.base_field.is_some() {
		return Err(syn::Error::new(item_impl.span(), "base = 필드 는 impl 블록(트레이트 impl 이 아닌 것)의 #[reflect] 에 쓴다"));
	}
	let trait_path = match &item_impl.trait_ {
		Some((_, trait_path, _)) => {
			trait_path.clone()
		}
		None => {
			return Err(syn::Error::new(item_impl.span(), "트레이트 impl 이 아니다"));
		}
	};
	let is_component_trait = trait_path.segments.last().map(|segment| segment.ident == "Component").unwrap_or(false);
	if !is_component_trait {
		return Err(syn::Error::new(trait_path.span(), "트레이트 impl 에는 impl Component 블록에만 #[reflect] 를 붙인다"));
	}

	// 이 블록에서 정의한 게터 모으기. (트레이트 impl 의 함수는 pub 이 없다)
	let mut getter_push_list: Vec<TokenStream2> = Vec::new();
	for impl_item in item_impl.items.iter_mut() {
		let method = match impl_item {
			ImplItem::Fn(method) => {
				method
			}
			_ => {
				continue;
			}
		};
		let method_text = method.sig.ident.to_string();
		let method_options = take_method_options(&mut method.attrs)?;
		if method_options.is_skipped || COMPONENT_BASE_GETTER_NAMES.contains(&method_text.as_str()) {
			continue;
		}
		let getter = match read_getter(method, &method_options) {
			Some(getter) if !getter.is_mutable => {
				getter
			}
			_ => {
				continue;
			}
		};
		let cfg_attributes = &getter.cfg_attributes;
		let original_name = &getter.original_name;
		let getter_name = &getter.method_name;
		let read_expression = create_read_expression(&reflect_path, quote!(<Self as #trait_path>::#getter_name(self)));
		match &getter.after_name {
			Some(after_name) => {
				getter_push_list.push(quote! {
					#(#cfg_attributes)*
					anchored_getter_list.push((#after_name, #reflect_path::ReflectedGetter {
						name: #original_name,
						value: #read_expression,
					}));
				});
			}
			None => {
				getter_push_list.push(quote! {
					#(#cfg_attributes)*
					#reflect_path::push_reflected_getter(getter_list, #original_name, #read_expression);
				});
			}
		}
	}

	// draw 를 덮어쓰지 않은 컴포넌트는 아무것도 그리지 않는다고 알린다. (원본은 component.draw 가 Component.prototype.draw 와 같은지로 본다, 화면 밖 거르기와 하위 트리 그리기 경계가 뺀다)
	let is_method_defined = |method_name: &str| item_impl.items.iter().any(|impl_item| matches!(impl_item, ImplItem::Fn(method) if method.sig.ident == method_name));
	if !is_method_defined("draw") && !is_method_defined("is_draw_overridden") {
		item_impl.items.push(syn::parse_quote! {
			fn is_draw_overridden(&self) -> bool {
				false
			}
		});
	}

	let self_type = &item_impl.self_ty;
	let (impl_generics, _, where_clause) = item_impl.generics.split_for_impl();
	Ok(quote! {
		#item_impl

		#[allow(deprecated)]
		impl #impl_generics #reflect_path::ReflectComponentGetters for #self_type #where_clause {
			#[allow(unused_variables)]
			fn collect_component_getters(&self, getter_list: &mut ::std::vec::Vec<#reflect_path::ReflectedGetter>, anchored_getter_list: &mut ::std::vec::Vec<(&'static str, #reflect_path::ReflectedGetter)>) {
				#(#getter_push_list)*
			}
		}
	})
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// snake_case 접미어는 원본 camelCase 의 머리 대문자 이름으로 돌아간다.
	//==============================================================================
	#[test]
	fn suffix_becomes_original_name() {
		assert_eq!(convert_suffix_to_reflected_name("line_width"), "LineWidth");
		assert_eq!(convert_suffix_to_reflected_name("gizmo_visible"), "GizmoVisible");
		assert_eq!(convert_suffix_to_reflected_name("circle"), "Circle");
		assert_eq!(convert_suffix_to_reflected_name("fit_minimum_ratio"), "FitMinimumRatio");
		assert_eq!(convert_suffix_to_reflected_name("position_3d"), "Position3d");
	}

	//==============================================================================
	/// 블록 옵션은 base = 필드, crate = 경로 이고, 모르는 옵션은 에러다.
	//==============================================================================
	#[test]
	fn block_options_read_base_and_crate() {
		let empty_options = parse_block_options(TokenStream2::new()).expect("empty");
		assert!(empty_options.base_field.is_none());
		assert_eq!(empty_options.crate_path.to_token_stream().to_string(), ":: vanilla");
		let block_options = parse_block_options(quote!(base = scroll_view, crate = ::game_engine)).expect("options");
		assert_eq!(block_options.base_field.map(|base_field| base_field.to_string()), Some(String::from("scroll_view")));
		assert_eq!(block_options.crate_path.to_token_stream().to_string(), ":: game_engine");
		assert!(parse_block_options(quote!(parent = ui_view)).is_err());
	}

	//==============================================================================
	/// 접두어는 get_, is_, set_ 이고 접미어가 비면 게터가 아니다.
	//==============================================================================
	#[test]
	fn method_name_splits_prefix() {
		assert_eq!(split_method_name("get_color", &["get_", "is_"]), Some(("get_", "color")));
		assert_eq!(split_method_name("is_circle", &["get_", "is_"]), Some(("is_", "circle")));
		assert_eq!(split_method_name("get_", &["get_", "is_"]), None);
		assert_eq!(split_method_name("island", &["get_", "is_"]), None);
	}

	//==============================================================================
	/// 게터는 &self 하나만 받고 무언가 돌려주는 pub get_, is_ 함수다.
	//==============================================================================
	#[test]
	fn getter_rules_follow_original() {
		let method_options = MethodOptions::default();
		let getter_method: ImplItemFn = syn::parse_quote! {
			pub fn get_line_width(&self) -> f32 {
				1.0
			}
		};
		let getter = read_getter(&getter_method, &method_options).expect("getter");
		assert_eq!(getter.reflected_name, "LineWidth");
		assert_eq!(getter.original_name, "getLineWidth");
		let argument_method: ImplItemFn = syn::parse_quote! {
			pub fn get_point(&self, index: usize) -> f32 {
				1.0
			}
		};
		assert!(read_getter(&argument_method, &method_options).is_none());
		let mutable_method: ImplItemFn = syn::parse_quote! {
			pub fn get_cache(&mut self) -> f32 {
				1.0
			}
		};
		assert!(read_getter(&mutable_method, &method_options).expect("mutable getter").is_mutable);
		let accessor_method: ImplItemFn = syn::parse_quote! {
			pub fn get_uiview_mut(&mut self) -> &mut UIView {
				&mut self.ui_view
			}
		};
		assert!(read_getter(&accessor_method, &method_options).is_none());
		let unit_method: ImplItemFn = syn::parse_quote! {
			pub fn is_done(&self) {
			}
		};
		assert!(read_getter(&unit_method, &method_options).is_none());
		let renamed_options = MethodOptions {
			is_skipped: false,
			is_transient: false,
			is_unsaved: false,
			reflected_name: Some(String::from("UIScale")),
			after_name: None,
		};
		let renamed = read_getter(&getter_method, &renamed_options).expect("renamed");
		assert_eq!(renamed.original_name, "getUIScale");
	}

	//==============================================================================
	/// 세터는 값 하나를 받으면 그 꼴을, impl Trait 이면 표시만, 트리와 값을 받으면 트리 세터로, 그 밖은 꼴 없이 든다.
	//==============================================================================
	#[test]
	fn setter_rules_keep_value_type() {
		let plain_method: ImplItemFn = syn::parse_quote! {
			pub fn set_text<'a>(&mut self, text: &'a str) {
			}
		};
		let plain = read_setter(&plain_method, &MethodOptions::default()).expect("setter");
		assert_eq!(plain.value_type.map(|value_type| value_type.to_token_stream().to_string()), Some(String::from("& '_ str")));
		let impl_method: ImplItemFn = syn::parse_quote! {
			pub fn set_color(&mut self, color: impl Into<Color>) {
			}
		};
		let impl_setter = read_setter(&impl_method, &MethodOptions::default()).expect("impl setter");
		assert!(impl_setter.is_impl_trait);
		assert!(impl_setter.value_type.is_none());
		let pair_method: ImplItemFn = syn::parse_quote! {
			pub fn set_image(&mut self, image: Option<Rc<Image>>, image_rect: Option<Rect>) {
			}
		};
		let pair_setter = read_setter(&pair_method, &MethodOptions::default()).expect("pair setter");
		assert!(!pair_setter.is_impl_trait);
		assert!(pair_setter.value_type.is_none());
		let tree_method: ImplItemFn = syn::parse_quote! {
			pub fn set_snap_current_index(&mut self, node_tree: &mut NodeTree, index: usize) {
			}
		};
		let tree_setter = read_setter(&tree_method, &MethodOptions::default()).expect("tree setter");
		assert!(tree_setter.takes_tree);
		assert_eq!(tree_setter.reflected_name, "SnapCurrentIndex");
		assert_eq!(tree_setter.value_type.map(|value_type| value_type.to_token_stream().to_string()), Some(String::from("usize")));
	}
}
