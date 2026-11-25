//==============================================================================
// 바닐라엔진 절차 매크로. (원본 vanilla.js 의 실행 중 반사 자리)
// - 원본은 자바스크립트라 실행 중에 객체의 프로토타입을 훑어 get, is, set 함수를 찾는다.
//   (ui/uidocument.js 의 collectReflectedProperties, misc/devtools.js 의 getComponentProperties)
//   러스트에는 실행 중 반사가 없어, 언리얼의 UHT(UPROPERTY), Qt 의 moc 처럼 컴파일 때 코드를 지어 같은 일을 한다.
// - #[reflect] 를 컴포넌트의 impl 블록에 붙이면 블록 안의 pub fn get_*(&self) -> T, pub fn is_*(&self) -> T 와
//   그 짝 pub fn set_*(&mut self, value) 를 모아 vanilla::core::reflection::Reflect 구현을 짓는다.
// - #[reflect] 를 impl Component 블록에 붙이면 get_reflection, get_reflection_mut 을 넣고,
//   그 블록에서 다시 정의한 게터(원본 하위 클래스가 덮어쓴 isTickNeeded 같은 것)를 인스펙터용으로 모은다. 두 블록 모두 붙여야 한다.
// - 원본 규칙을 그대로 따른다:
//   - 게터는 인자 없는 get*, is* 함수다. (원본 getterFunction.length === 0) 저장 속성은 같은 이름의 set* 이 있는 게터만이다.
//   - 키는 접두어(get, is)를 뗀 원본 이름이다. 러스트 snake_case 를 원본 camelCase 로 되돌린다. (get_line_width → getLineWidth → LineWidth)
//   - 차례는 정의 차례, 같은 키는 먼저 나온 것이다.
// - 값으로 바꿀 수 없는 꼴의 게터도 컴파일 에러를 내지 않는다. 자동 참조 특수화(autoref specialization)로 바꿀 수 있는 꼴만 값으로 읽고,
//   나머지는 원본처럼 저장에서 건너뛰고 인스펙터에는 [종류 이름] 으로 보인다.
// - 함수에 붙이는 것: #[reflect(skip)] 반사에서 뺀다. #[reflect(transient)] 인스펙터에는 보이되 저장 속성에서 뺀다.
//   #[reflect(name = "UIScale")] 원본 키 이름을 정한다. (원본 이름의 머리글자 낱말처럼 snake_case 에서 되돌릴 수 없는 이름)
// - 블록에 붙이는 것: #[reflect(base = 필드)] 품은 값의 반사를 원본 상위 클래스처럼 뒤에 잇는다.
//   #[reflect(crate = 경로)] 엔진 경로. (기본 ::vanilla, 엔진 크레이트 안에서는 lib.rs 의 extern crate self as vanilla 로 같은 경로가 통한다)
//==============================================================================
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::parse::Parser;
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;
use syn::{Attribute, FnArg, GenericParam, Ident, ImplItem, ImplItemFn, ItemImpl, Lifetime, LitStr, Path, ReturnType, Type};

// 인스펙터에도 보이지 않는 Component 트레이트의 게터. (공통 상태와 종류 이름은 원본 Component 기반 클래스 몫이다)
const COMPONENT_BASE_GETTER_NAMES: [&str; 2] = ["get_component_base", "get_component_type"];

//==============================================================================
// 블록 옵션. (#[reflect(base = 필드, crate = 경로)])
//==============================================================================
struct BlockOptions {
	base_field: Option<Ident>,
	crate_path: Path,
}

//==============================================================================
// 함수 옵션. (#[reflect(skip)], #[reflect(transient)], #[reflect(name = "...")])
//==============================================================================
#[derive(Default)]
struct MethodOptions {
	is_skipped: bool,
	is_transient: bool,
	reflected_name: Option<String>,
}

//==============================================================================
// 모은 게터 하나.
//==============================================================================
struct GetterMethod {
	method_name: Ident,
	suffix: String,
	reflected_name: String,
	original_name: String,
	return_type: Type,
	cfg_attributes: Vec<Attribute>,
	is_transient: bool,
}

//==============================================================================
// 모은 세터 하나. (value_type 이 None 이면 값 하나로 부를 수 없는 세터다. 인자가 둘 이상이거나 제네릭)
//==============================================================================
struct SetterMethod {
	method_name: Ident,
	suffix: String,
	value_type: Option<Type>,
	is_impl_trait: bool,
	cfg_attributes: Vec<Attribute>,
}

//==============================================================================
// 반사 매크로. (impl 블록에 붙인다. 트레이트 impl 이면 impl Component 블록으로 본다)
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
// 블록 옵션 읽기.
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
// 함수의 #[reflect(...)] 를 읽고 지운다. (지우지 않으면 컴파일러가 함수에 붙은 매크로로 다시 부른다)
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
			else if meta.path.is_ident("name") {
				let reflected_name: LitStr = meta.value()?.parse()?;
				method_options.reflected_name = Some(reflected_name.value());
				Ok(())
			}
			else {
				Err(meta.error("함수의 #[reflect] 는 skip, transient, name = \"...\" 만 받는다"))
			}
		})?;
	}
	*attributes = kept_attributes;
	Ok(method_options)
}

//==============================================================================
// cfg 속성 모으기. (지은 코드도 함수와 같은 조건에서만 들어가야 한다)
//==============================================================================
fn collect_cfg_attributes(attributes: &[Attribute]) -> Vec<Attribute> {
	attributes.iter().filter(|attribute| attribute.path().is_ident("cfg")).cloned().collect()
}

//==============================================================================
// cfg 속성의 비교용 글. (같은 이름이 다른 조건으로 둘 있으면 둘 다 짓는다)
//==============================================================================
fn read_cfg_text(cfg_attributes: &[Attribute]) -> String {
	cfg_attributes.iter().map(|attribute| attribute.to_token_stream().to_string()).collect::<Vec<String>>().join(" ")
}

//==============================================================================
// snake_case 접미어 → 원본 이름. (낱말마다 머리를 대문자로 이어 붙인다. line_width → LineWidth)
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
// 함수 이름을 접두어와 접미어로 나누기. (get_, is_, set_ 가운데 하나, 접미어가 비면 None)
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
// 부를 수 있는 꼴의 함수인지. (unsafe, async, extern, 타입이나 상수 제네릭이 없다)
//==============================================================================
fn is_plain_signature(signature: &syn::Signature) -> bool {
	if signature.unsafety.is_some() || signature.asyncness.is_some() || signature.abi.is_some() || signature.variadic.is_some() {
		return false;
	}
	signature.generics.params.iter().all(|generic_param| matches!(generic_param, GenericParam::Lifetime(_)))
}

//==============================================================================
// 받는 꼴. (&self 면 Some(false), &mut self 면 Some(true), 그 밖은 None)
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
// 돌려주는 꼴. (없거나 () 나 ! 면 None)
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
// 꼴 안의 수명을 '_ 로. (함수의 수명 매개변수는 지은 코드에서 보이지 않는다. 'static 은 둔다)
//==============================================================================
struct LifetimeEraser;

impl VisitMut for LifetimeEraser {
	fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
		if lifetime.ident != "static" {
			*lifetime = Lifetime::new("'_", lifetime.span());
		}
	}
}

//==============================================================================
// 수명을 지운 꼴.
//==============================================================================
fn erase_lifetimes(type_value: &Type) -> Type {
	let mut erased_type = type_value.clone();
	LifetimeEraser.visit_type_mut(&mut erased_type);
	erased_type
}

//==============================================================================
// impl Trait 이 든 꼴인지. (이름을 쓸 수 없어 지은 코드에 옮길 수 없다)
//==============================================================================
struct ImplTraitFinder {
	is_found: bool,
}

impl<'a> syn::visit::Visit<'a> for ImplTraitFinder {
	fn visit_type_impl_trait(&mut self, _type_impl_trait: &'a syn::TypeImplTrait) {
		self.is_found = true;
	}
}

//==============================================================================
// impl Trait 이 든 꼴인지 보기.
//==============================================================================
fn contains_impl_trait(type_value: &Type) -> bool {
	let mut impl_trait_finder = ImplTraitFinder {
		is_found: false,
	};
	syn::visit::Visit::visit_type(&mut impl_trait_finder, type_value);
	impl_trait_finder.is_found
}

//==============================================================================
// 게터 읽기. (인자 없는 get_*, is_* 이고 &self 를 받고 무언가 돌려주는 함수)
//==============================================================================
fn read_getter(method: &ImplItemFn, method_options: &MethodOptions) -> Option<GetterMethod> {
	let signature = &method.sig;
	let method_text = signature.ident.to_string();
	let (prefix, suffix) = split_method_name(&method_text, &["get_", "is_"])?;
	if !is_plain_signature(signature) || read_receiver_mutability(signature) != Some(false) || signature.inputs.len() != 1 {
		return None;
	}
	let return_type = read_return_type(signature)?;
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
		suffix: String::from(suffix),
		original_name: format!("{}{}", original_prefix, reflected_name),
		reflected_name,
		return_type,
		cfg_attributes: collect_cfg_attributes(&method.attrs),
		is_transient: method_options.is_transient,
	})
}

//==============================================================================
// 세터 읽기. (set_* 이고 &mut self 를 받는 함수. 값 하나를 받으면 그 꼴을 든다)
//==============================================================================
fn read_setter(method: &ImplItemFn) -> Option<SetterMethod> {
	let signature = &method.sig;
	let method_text = signature.ident.to_string();
	let (_, suffix) = split_method_name(&method_text, &["set_"])?;
	if read_receiver_mutability(signature) != Some(true) {
		return None;
	}
	let mut value_type: Option<Type> = None;
	let mut is_impl_trait = false;
	if is_plain_signature(signature) && signature.inputs.len() == 2 {
		if let Some(FnArg::Typed(pattern_type)) = signature.inputs.iter().nth(1) {
			if contains_impl_trait(&pattern_type.ty) {
				is_impl_trait = true;
			}
			else {
				value_type = Some(erase_lifetimes(&pattern_type.ty));
			}
		}
	}
	Some(SetterMethod {
		method_name: signature.ident.clone(),
		suffix: String::from(suffix),
		value_type,
		is_impl_trait,
		cfg_attributes: collect_cfg_attributes(&method.attrs),
	})
}

//==============================================================================
// impl 대상 타입의 이름. (원본 constructor.name 자리, 경로의 끝 이름)
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
// 게터 값을 반사 값으로 읽는 식. (바꿀 수 있는 꼴 → 품은 꼴(Option, 배열) → 종류 이름 차례로 고른다)
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
// impl 블록 펼치기. (블록은 그대로 두고 Reflect 구현을 덧붙인다)
//==============================================================================
fn expand_inherent_impl(block_options: &BlockOptions, mut item_impl: ItemImpl) -> syn::Result<TokenStream2> {
	let crate_path = &block_options.crate_path;
	let reflect_path = quote!(#crate_path::core::reflection);
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
		if method_options.is_skipped || !matches!(method.vis, syn::Visibility::Public(_)) {
			continue;
		}
		if let Some(getter) = read_getter(method, &method_options) {
			getter_list.push(getter);
		}
		else if let Some(setter) = read_setter(method) {
			setter_list.push(setter);
		}
	}

	// 저장 속성. (짝 세터가 있는 게터, 정의 차례, 같은 키와 같은 조건이면 먼저 것)
	let mut property_key_list: Vec<(String, String)> = Vec::new();
	let mut property_push_list: Vec<TokenStream2> = Vec::new();
	let mut read_arm_list: Vec<TokenStream2> = Vec::new();
	let mut write_arm_list: Vec<TokenStream2> = Vec::new();
	for getter in getter_list.iter() {
		if getter.is_transient {
			continue;
		}
		for setter in setter_list.iter().filter(|setter| setter.suffix == getter.suffix) {
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
			property_push_list.push(quote! {
				#(#cfg_attributes)*
				property_list.push(#reflect_path::ReflectedProperty {
					name: #reflected_name,
					getter_name: #getter_text,
					setter_name: ::core::option::Option::Some(#setter_text),
				});
			});
			let getter_name = &getter.method_name;
			let read_expression = create_read_expression(&reflect_path, quote!(self.#getter_name()));
			read_arm_list.push(quote! {
				#(#cfg_attributes)*
				#reflected_name => {
					::core::option::Option::Some(#read_expression)
				}
			});
			// 세터가 impl Trait 을 받으면 게터 꼴로 바꿔 넘긴다. (set_color(impl Into<Color>) 에 get_color 의 Color)
			let value_type = if setter.is_impl_trait && !contains_impl_trait(&getter.return_type) {
				Some(erase_lifetimes(&getter.return_type))
			}
			else {
				setter.value_type.clone()
			};
			let setter_name = &setter.method_name;
			match value_type {
				Some(value_type) => {
					write_arm_list.push(quote! {
						#(#cfg_attributes)*
						#reflected_name => {
							#[allow(unused_imports)]
							use #reflect_path::{ReflectSetViaNone as _, ReflectSetViaValue as _};
							match (&#reflect_path::ReflectSetProbe::<#value_type>::new(value)).convert_reflect_value() {
								::core::option::Option::Some(converted_value) => {
									let _ = self.#setter_name(converted_value);
									true
								}
								::core::option::Option::None => {
									false
								}
							}
						}
					});
				}
				None => {
					write_arm_list.push(quote! {
						#(#cfg_attributes)*
						#reflected_name => {
							false
						}
					});
				}
			}
		}
	}

	// 인스펙터 게터. (모든 게터, 정의 차례)
	let mut getter_key_list: Vec<(String, String)> = Vec::new();
	let mut getter_push_list: Vec<TokenStream2> = Vec::new();
	for getter in getter_list.iter() {
		let getter_key = (getter.original_name.clone(), read_cfg_text(&getter.cfg_attributes));
		if getter_key_list.contains(&getter_key) {
			continue;
		}
		getter_key_list.push(getter_key);
		let cfg_attributes = &getter.cfg_attributes;
		let original_name = &getter.original_name;
		let getter_name = &getter.method_name;
		let read_expression = create_read_expression(&reflect_path, quote!(self.#getter_name()));
		getter_push_list.push(quote! {
			#(#cfg_attributes)*
			#reflect_path::push_reflected_getter(getter_list, #original_name, #read_expression);
		});
	}

	// 품은 값. (원본 상위 클래스 자리, 이 타입에 없는 키만 뒤에 잇는다)
	let (base_property_extension, base_read_fallback, base_write_fallback, base_getter_extension) = match &block_options.base_field {
		Some(base_field) => {
			(
				quote! {
					for base_property in #reflect_path::Reflect::get_reflected_properties(&self.#base_field) {
						if !property_list.iter().any(|property: &#reflect_path::ReflectedProperty| property.name == base_property.name) {
							property_list.push(base_property);
						}
					}
				},
				quote! {
					#reflect_path::Reflect::get_reflected_value(&self.#base_field, property_name)
				},
				quote! {
					#reflect_path::Reflect::set_reflected_value(&mut self.#base_field, property_name, value)
				},
				quote! {
					let mut base_getter_list: ::std::vec::Vec<#reflect_path::ReflectedGetter> = ::std::vec::Vec::new();
					#reflect_path::Reflect::collect_reflected_getters(&self.#base_field, &mut base_getter_list);
					for base_getter in base_getter_list {
						#reflect_path::push_reflected_getter(getter_list, base_getter.name, base_getter.value);
					}
				},
			)
		}
		None => {
			(quote!(), quote!(::core::option::Option::None), quote!(false), quote!())
		}
	};

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
			fn set_reflected_value(&mut self, property_name: &str, value: &#reflect_path::ReflectValue) -> bool {
				match property_name {
					#(#write_arm_list)*
					_ => {
						#base_write_fallback
					}
				}
			}

			fn collect_reflected_getters(&self, getter_list: &mut ::std::vec::Vec<#reflect_path::ReflectedGetter>) {
				#reflect_path::ReflectComponentGetters::collect_component_getters(self, getter_list);
				#(#getter_push_list)*
				#base_getter_extension
			}
		}
	})
}

//==============================================================================
// impl Component 블록 펼치기. (반사를 꺼내는 함수를 넣고, 이 블록에서 다시 정의한 게터를 모으는 구현을 덧붙인다)
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
		if method_text == "get_reflection" || method_text == "get_reflection_mut" {
			return Err(syn::Error::new(method.sig.ident.span(), "#[reflect] 를 붙인 impl Component 블록에는 get_reflection 을 쓰지 않는다. (매크로가 넣는다)"));
		}
		let method_options = take_method_options(&mut method.attrs)?;
		if method_options.is_skipped || COMPONENT_BASE_GETTER_NAMES.contains(&method_text.as_str()) {
			continue;
		}
		let getter = match read_getter(method, &method_options) {
			Some(getter) => {
				getter
			}
			None => {
				continue;
			}
		};
		let cfg_attributes = &getter.cfg_attributes;
		let original_name = &getter.original_name;
		let getter_name = &getter.method_name;
		let read_expression = create_read_expression(&reflect_path, quote!(<Self as #trait_path>::#getter_name(self)));
		getter_push_list.push(quote! {
			#(#cfg_attributes)*
			#reflect_path::push_reflected_getter(getter_list, #original_name, #read_expression);
		});
	}

	item_impl.items.push(syn::parse_quote! {
		fn get_reflection(&self) -> ::core::option::Option<&dyn #reflect_path::Reflect> {
			::core::option::Option::Some(self)
		}
	});
	item_impl.items.push(syn::parse_quote! {
		fn get_reflection_mut(&mut self) -> ::core::option::Option<&mut dyn #reflect_path::Reflect> {
			::core::option::Option::Some(self)
		}
	});

	let self_type = &item_impl.self_ty;
	let (impl_generics, _, where_clause) = item_impl.generics.split_for_impl();
	Ok(quote! {
		#item_impl

		#[allow(deprecated)]
		impl #impl_generics #reflect_path::ReflectComponentGetters for #self_type #where_clause {
			#[allow(unused_variables)]
			fn collect_component_getters(&self, getter_list: &mut ::std::vec::Vec<#reflect_path::ReflectedGetter>) {
				#(#getter_push_list)*
			}
		}
	})
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// snake_case 접미어는 원본 camelCase 의 머리 대문자 이름으로 돌아간다.
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
	// 블록 옵션은 base = 필드, crate = 경로 이고, 모르는 옵션은 에러다.
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
	// 접두어는 get_, is_, set_ 이고 접미어가 비면 게터가 아니다.
	//==============================================================================
	#[test]
	fn method_name_splits_prefix() {
		assert_eq!(split_method_name("get_color", &["get_", "is_"]), Some(("get_", "color")));
		assert_eq!(split_method_name("is_circle", &["get_", "is_"]), Some(("is_", "circle")));
		assert_eq!(split_method_name("get_", &["get_", "is_"]), None);
		assert_eq!(split_method_name("island", &["get_", "is_"]), None);
	}

	//==============================================================================
	// 게터는 &self 하나만 받고 무언가 돌려주는 pub get_, is_ 함수다.
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
		assert!(read_getter(&mutable_method, &method_options).is_none());
		let unit_method: ImplItemFn = syn::parse_quote! {
			pub fn is_done(&self) {
			}
		};
		assert!(read_getter(&unit_method, &method_options).is_none());
		let renamed_options = MethodOptions {
			is_skipped: false,
			is_transient: false,
			reflected_name: Some(String::from("UIScale")),
		};
		let renamed = read_getter(&getter_method, &renamed_options).expect("renamed");
		assert_eq!(renamed.original_name, "getUIScale");
	}

	//==============================================================================
	// 세터는 값 하나를 받으면 그 꼴을, impl Trait 이면 표시만, 둘 이상이면 꼴 없이 든다.
	//==============================================================================
	#[test]
	fn setter_rules_keep_value_type() {
		let plain_method: ImplItemFn = syn::parse_quote! {
			pub fn set_text<'a>(&mut self, text: &'a str) {
			}
		};
		let plain = read_setter(&plain_method).expect("setter");
		assert_eq!(plain.value_type.map(|value_type| value_type.to_token_stream().to_string()), Some(String::from("& '_ str")));
		let impl_method: ImplItemFn = syn::parse_quote! {
			pub fn set_color(&mut self, color: impl Into<Color>) {
			}
		};
		let impl_setter = read_setter(&impl_method).expect("impl setter");
		assert!(impl_setter.is_impl_trait);
		assert!(impl_setter.value_type.is_none());
		let pair_method: ImplItemFn = syn::parse_quote! {
			pub fn set_image(&mut self, image: Option<Rc<Image>>, image_rect: Option<Rect>) {
			}
		};
		let pair_setter = read_setter(&pair_method).expect("pair setter");
		assert!(!pair_setter.is_impl_trait);
		assert!(pair_setter.value_type.is_none());
	}
}
