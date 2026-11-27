//==============================================================================
//! 컴포넌트.
//! - 노드에 붙는 기능 단위. 원본은 Component 클래스를 상속하고, 여기서는 Component 트레이트를 구현한다.
//! - 원본의 컴포넌트는 get_node() 로 자기 노드를 찾는다. 러스트 판은 tick / 터치 때 ComponentContext 를 넘겨 준다.
//!   문맥에서 자기 노드(트랜스폼), 트리 전체(다른 노드 찾기, 만들기, 지우기), 엔진(입력, 시간, 뷰)을 꺼낸다.
//! - draw 는 노드 트랜스폼이 걸린 상태에서 자기 트랜스폼만 받는다. (그리는 동안에는 트리를 바꾸지 않는다)
//! - push_transform, pop_transform 은 노드를 그리는 앞뒤(자기 컴포넌트와 자식, 기즈모를 감싸서)에 불린다. 원본 노드가 pushTransform, popTransform 을 덮어써
//!   자손까지 그리는 상태(블렌드 같은 것)를 바꾸던 자리다. (TouchEffect 의 더하기 블렌드)
//! - enable / gizmo 같은 공통 상태는 ComponentBase 에 두고, 구현체는 그것을 돌려주기만 한다.
//! - 의존 컴포넌트(원본 require): 원본은 클래스 목록을 돌려준다. 러스트 판은 RequiredComponent(타입과 기본값으로 만드는 함수) 목록을 돌려주고,
//!   WorldNode::add_component 가 붙이기 전에 그 타입이 없으면 만들어 붙인다. 떼어질 때는 원본처럼 detach 를 부른다.
//! - 원본 setNode, getNode, setComponentType 은 옮기지 않았다. 러스트 판은 노드를 문맥으로 넘기고(전역 객체 참조가 없다) 이름은 타입이 정한다.
//! - 반사(원본이 실행 중에 프로토타입을 훑어 get, is, set 함수를 찾는 것): impl 블록과 impl Component 블록에 `#[vanilla::reflect]` 를 붙이면
//!   매크로가 컴파일 때 그 목록과 부르는 코드를 짓는다. (core/reflection.rs, 언리얼 UHT 와 같은 꼴)
//!   Reflect 가 Component 의 상위 트레이트라 매크로를 빠뜨린 컴포넌트는 컴파일되지 않는다. 그래서 원본처럼 모든 컴포넌트가 반사된다.
//!   반사를 쓰는 쪽은 `&dyn Component` 를 `&dyn Reflect` 로 올려 부른다. (트레이트 업캐스팅)
//!   구체 타입으로 꺼내기(C++ RTTI 의 dynamic_cast 자리)는 AsAny 다.
//==============================================================================
pub mod mask;
pub mod outline;
pub mod paint;
pub mod richtext;
pub mod shadowimage;
pub mod shadowtext;
pub mod softshade;
pub mod sprite;
pub mod text;

use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::{push_draw_bounds_dirty, DrawBoundsKey, NodeTransform};
use crate::core::reflection::Reflect;
use std::any::{Any, TypeId};
use std::sync::atomic::{AtomicU64, Ordering};

//==============================================================================
/// 의존 컴포넌트 하나. (그 타입과 없을 때 만드는 함수, 원본 require 가 돌려주는 클래스 자리)
/// - Component::require 가 돌려주는 목록의 한 칸이다. 보통 RequiredComponent::of 로 만든다.
///
/// # 예제
/// 걷기 컴포넌트를 붙이면 몸 컴포넌트도 함께 붙는다
/// ```rust
/// use vanilla::core::component::RequiredComponent;
/// use vanilla::{Component, ComponentBase, WorldNode};
///
/// #[derive(Default)]
/// struct Body {
/// 	component_base: ComponentBase,
/// }
///
/// #[vanilla::reflect]
/// impl Body {}
///
/// #[vanilla::reflect]
/// impl Component for Body {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"Body"
/// 	}
/// }
///
/// #[derive(Default)]
/// struct Walker {
/// 	component_base: ComponentBase,
/// }
///
/// #[vanilla::reflect]
/// impl Walker {}
///
/// #[vanilla::reflect]
/// impl Component for Walker {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"Walker"
/// 	}
///
/// 	fn require(&self) -> Vec<RequiredComponent> {
/// 		vec![RequiredComponent::of::<Body>()]
/// 	}
/// }
///
/// let mut node = WorldNode::new("walker");
/// node.add_component(Walker::default());
/// assert!(node.has_component::<Body>());
/// node.add_component(Walker::default());
/// assert_eq!(node.get_components::<Body>().len(), 1);
/// ```
//==============================================================================
pub struct RequiredComponent {
	/// 의존 컴포넌트의 타입. (같은 노드에 이 타입이 이미 있으면 만들지 않는다)
	pub type_id: TypeId,
	/// 그 타입이 없을 때 새로 만드는 함수.
	pub create: fn() -> Box<dyn Component>,
}

impl RequiredComponent {
	//==============================================================================
	/// 그 타입의 기본값으로 만드는 의존 컴포넌트.
	///
	/// # 돌려줌
	/// T 의 타입과 `T::default()` 로 만드는 함수를 든 의존 컴포넌트.
	//==============================================================================
	pub fn of<T: Component + Default + 'static>() -> RequiredComponent {
		RequiredComponent {
			type_id: TypeId::of::<T>(),
			create: create_default_component::<T>,
		}
	}
}

//==============================================================================
/// 기본값 컴포넌트 만들기.
/// - 의존 컴포넌트의 create 칸에 든다.
//==============================================================================
fn create_default_component<T: Component + Default + 'static>() -> Box<dyn Component> {
	Box::new(T::default())
}

//==============================================================================
/// 구체 타입으로 꺼내기 위한 도움 트레이트. (`get_component::<Sprite>()` 같은 곳)
/// - Component 를 구현한 'static 타입이면 저절로 구현된다. (직접 구현하지 않는다)
/// - `dyn Component` 에서 부를 때는 이 트레이트를 use 로 들여온다.
///
/// # 예제
/// `Box<dyn Component>` 에서 구체 타입 꺼내기
/// ```rust
/// use vanilla::core::component::AsAny;
/// use vanilla::{Component, Paint, Text};
///
/// let component: Box<dyn Component> = Box::new(Paint::new());
/// let component_reference: &dyn Component = component.as_ref();
/// assert!(component_reference.as_any().is::<Paint>());
/// assert!(component_reference.as_any().downcast_ref::<Text>().is_none());
/// ```
//==============================================================================
pub trait AsAny {
	//==============================================================================
	/// `&dyn Any` 로 보기. (downcast_ref 로 구체 타입을 꺼낸다)
	///
	/// # 돌려줌
	/// 자기 자신을 가리키는 `&dyn Any`.
	//==============================================================================
	fn as_any(&self) -> &dyn Any;

	//==============================================================================
	/// `&mut dyn Any` 로 보기. (downcast_mut 로 구체 타입을 꺼낸다)
	///
	/// # 돌려줌
	/// 자기 자신을 가리키는 `&mut dyn Any`.
	//==============================================================================
	fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Component + 'static> AsAny for T {
	//==============================================================================
	/// 자기 자신을 `&dyn Any` 로 돌려준다.
	//==============================================================================
	fn as_any(&self) -> &dyn Any {
		self
	}

	//==============================================================================
	/// 자기 자신을 `&mut dyn Any` 로 돌려준다.
	//==============================================================================
	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}
}

impl<'a> dyn Component + 'a {
	//==============================================================================
	/// 그 타입이거나 그 타입을 기반으로 품었는지. (원본 `component instanceof T`)
	/// - 자기 타입부터 get_base_component 사슬을 따라 본다.
	///
	/// # 돌려줌
	/// 자기나 품은 기반 가운데 T 가 있으면 true.
	///
	/// # 예제
	/// ShaderSprite 는 Sprite 이고 Paint 다
	/// ```rust
	/// use vanilla::{Component, Paint, ShaderSprite, Sprite, Text};
	///
	/// let component: Box<dyn Component> = Box::new(ShaderSprite::new());
	/// assert!(component.is_instance_of::<ShaderSprite>());
	/// assert!(component.is_instance_of::<Sprite>());
	/// assert!(component.is_instance_of::<Paint>());
	/// assert!(!component.is_instance_of::<Text>());
	/// ```
	//==============================================================================
	pub fn is_instance_of<T: Component + 'static>(&self) -> bool {
		self.as_instance_of::<T>().is_some()
	}

	//==============================================================================
	/// 그 타입으로 보기. (자기 타입이 아니면 품은 기반 사슬에서 찾는다, 원본 instanceof 로 맞은 객체)
	///
	/// # 돌려줌
	/// 자기나 품은 기반 가운데 처음 맞는 T. 없으면 None.
	///
	/// # 예제
	/// ShaderSprite 를 Sprite 로 보기
	/// ```rust
	/// use vanilla::{Color, Component, ShaderSprite, Sprite};
	///
	/// let mut shader_sprite = ShaderSprite::new();
	/// shader_sprite.set_color(Color::create(1.0, 0.0, 0.0, 1.0));
	/// let component: Box<dyn Component> = Box::new(shader_sprite);
	/// assert_eq!(component.as_instance_of::<Sprite>().expect("sprite").get_color(), Color::create(1.0, 0.0, 0.0, 1.0));
	/// ```
	//==============================================================================
	pub fn as_instance_of<T: Component + 'static>(&self) -> Option<&T> {
		if let Some(found) = self.as_any().downcast_ref::<T>() {
			return Some(found);
		}
		match self.get_base_component() {
			Some(base_component) => base_component.as_instance_of::<T>(),
			None => None,
		}
	}

	//==============================================================================
	/// 그 타입으로 보기. (변경용)
	///
	/// # 돌려줌
	/// 자기나 품은 기반 가운데 처음 맞는 T. (변경용) 없으면 None.
	///
	/// # 예제
	/// ShaderSprite 의 Sprite 값 바꾸기
	/// ```rust
	/// use vanilla::{Color, Component, ShaderSprite, Sprite};
	///
	/// let mut component: Box<dyn Component> = Box::new(ShaderSprite::new());
	/// component.as_instance_of_mut::<Sprite>().expect("sprite").set_color(Color::create(0.0, 0.0, 1.0, 1.0));
	/// assert_eq!(component.as_instance_of::<ShaderSprite>().expect("shader sprite").get_color(), Color::create(0.0, 0.0, 1.0, 1.0));
	/// ```
	//==============================================================================
	pub fn as_instance_of_mut<T: Component + 'static>(&mut self) -> Option<&mut T> {
		if self.as_any().is::<T>() {
			return self.as_any_mut().downcast_mut::<T>();
		}
		match self.get_base_component_mut() {
			Some(base_component) => base_component.as_instance_of_mut::<T>(),
			None => None,
		}
	}
}

//==============================================================================
/// 컴포넌트 공통 상태.
/// - 켜짐(기본 true)과 기즈모 표시(기본 false)를 든다. 구현체가 필드로 들고 get_component_base 로 돌려준다.
///
/// # 예제
/// 새 공통 상태는 켜져 있고 기즈모는 꺼져 있다
/// ```rust
/// use vanilla::ComponentBase;
///
/// let mut component_base = ComponentBase::new();
/// assert!(component_base.is_enable());
/// assert!(!component_base.is_gizmo_visible());
/// component_base.set_enable(false);
/// component_base.set_gizmo_visible(true);
/// assert!(!component_base.is_enable());
/// assert!(component_base.is_gizmo_visible());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ComponentBase {
	is_enable: bool,
	is_gizmo_visible: bool,
	draw_bounds_key: Option<DrawBoundsKey>,
}

/// 컴포넌트 그리는 사각형 세대. (모든 컴포넌트의 그리는 사각형이 한꺼번에 바뀔 수 있을 때 하나 오른다, 글꼴 적재 같은)
static DRAWN_RECT_GENERATION: AtomicU64 = AtomicU64::new(1);

impl ComponentBase {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 켜져 있고 기즈모는 꺼진 공통 상태.
	//==============================================================================
	pub fn new() -> ComponentBase {
		ComponentBase {
			is_enable: true,
			is_gizmo_visible: false,
			draw_bounds_key: None,
		}
	}

	//==============================================================================
	/// 활성화 설정.
	///
	/// # 인자
	/// - `is_enable`: 켤지. (false 면 tick, 그리기, 터치, 휠에서 빠진다)
	//==============================================================================
	pub fn set_enable(&mut self, is_enable: bool) {
		if self.is_enable != is_enable {
			self.is_enable = is_enable;
			self.mark_drawn_rect_dirty();
		}
	}

	//==============================================================================
	/// 활성화 여부.
	///
	/// # 돌려줌
	/// 켜져 있으면 true.
	//==============================================================================
	pub fn is_enable(&self) -> bool {
		self.is_enable
	}

	//==============================================================================
	/// 기즈모 표시 설정.
	///
	/// # 인자
	/// - `is_gizmo_visible`: 노드의 기즈모를 그릴 때 이 컴포넌트의 draw_gizmos 도 부를지.
	//==============================================================================
	pub fn set_gizmo_visible(&mut self, is_gizmo_visible: bool) {
		if self.is_gizmo_visible != is_gizmo_visible {
			self.is_gizmo_visible = is_gizmo_visible;
			self.mark_drawn_rect_dirty();
		}
	}

	//==============================================================================
	/// 기즈모 표시 여부.
	///
	/// # 돌려줌
	/// 기즈모를 그리면 true.
	//==============================================================================
	pub fn is_gizmo_visible(&self) -> bool {
		self.is_gizmo_visible
	}

	//==============================================================================
	/// 그리는 사각형(Component::calculate_drawn_rect)이 바뀌었음을 노드에 알린다. (원본 Component.markDrawnRectDirty)
	/// - 붙은 노드와 조상이 다음 그리기 전에 하위 트리 그리기 경계를 다시 셈한다. 노드에 붙지 않았으면 할 일이 없다.
	/// - 그리는 사각형을 바꾸는 세터마다 부른다. (부르지 않으면 넓어진 그림이 화면에 들어와도 하위 트리째 건너뛰어 안 그려질 수 있다)
	///
	/// # 예제
	/// 세터에서 알리기
	/// ```rust
	/// use vanilla::ComponentBase;
	///
	/// let component_base = ComponentBase::new();
	/// component_base.mark_drawn_rect_dirty();
	/// ```
	//==============================================================================
	pub fn mark_drawn_rect_dirty(&self) {
		push_draw_bounds_dirty(self.draw_bounds_key, false);
	}

	//==============================================================================
	/// 경계 더러움 열쇠 달기. (노드에 붙을 때 WorldNode 가 단다)
	//==============================================================================
	pub(crate) fn set_draw_bounds_key(&mut self, draw_bounds_key: Option<DrawBoundsKey>) {
		self.draw_bounds_key = draw_bounds_key;
	}

	//==============================================================================
	/// 모든 컴포넌트의 그리는 사각형이 바뀌었음을 알린다. (원본 Component.markAllDrawnRectsDirty, 글꼴을 새로 읽어 글 너비가 바뀔 때 같은)
	/// - 세대를 하나 올려 모든 노드가 다음에 경계를 읽을 때 다시 셈하게 한다.
	///
	/// # 예제
	/// 세대가 오른다
	/// ```rust
	/// use vanilla::ComponentBase;
	///
	/// let generation = ComponentBase::get_drawn_rect_generation();
	/// ComponentBase::mark_all_drawn_rects_dirty();
	/// assert!(ComponentBase::get_drawn_rect_generation() > generation);
	/// ```
	//==============================================================================
	pub fn mark_all_drawn_rects_dirty() {
		DRAWN_RECT_GENERATION.fetch_add(1, Ordering::Relaxed);
	}

	//==============================================================================
	/// 컴포넌트 그리는 사각형 세대. (원본 Component.getDrawnRectGeneration, mark_all_drawn_rects_dirty 마다 하나 오른다)
	///
	/// # 돌려줌
	/// 지금 세대. (1 부터)
	//==============================================================================
	pub fn get_drawn_rect_generation() -> u64 {
		DRAWN_RECT_GENERATION.load(Ordering::Relaxed)
	}
}

impl Default for ComponentBase {
	//==============================================================================
	/// 기본값. (ComponentBase::new 와 같다)
	//==============================================================================
	fn default() -> ComponentBase {
		ComponentBase::new()
	}
}

//==============================================================================
/// 컴포넌트 트레이트.
/// - 원본은 모든 객체가 반사된다. 러스트 판은 반사(Reflect)를 상위 트레이트로 두어 반사 없는 컴포넌트가 생길 수 없다.
///   컴포넌트를 만드는 길은 `#[vanilla::reflect]` 하나다. 타입의 impl 블록과 이 트레이트의 impl 블록 둘 다에 붙인다.
///   게터가 없는 타입은 빈 블록 `#[vanilla::reflect] impl 타입 {}` 에 붙인다. 빠뜨리면 컴파일 에러가 붙일 자리를 알려 준다.
/// - UI 문서 저장과 DevTools 인스펙터는 `&dyn Component` 를 `&dyn Reflect` 로 올려(트레이트 업캐스팅) 원본처럼 모든 컴포넌트의 속성을 다룬다.
///
/// # 예제
/// 노드를 돌리는 컴포넌트를 만들어 붙이고 tick 하기
/// ```rust
/// use vanilla::{reflect, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, NodeTree};
///
/// struct Spin {
/// 	component_base: ComponentBase,
/// 	degrees_per_second: f64,
/// }
///
/// #[reflect]
/// impl Spin {
/// 	pub fn get_degrees_per_second(&self) -> f64 {
/// 		self.degrees_per_second
/// 	}
///
/// 	pub fn set_degrees_per_second(&mut self, degrees_per_second: f64) {
/// 		self.degrees_per_second = degrees_per_second;
/// 	}
/// }
///
/// #[reflect]
/// impl Component for Spin {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"Spin"
/// 	}
///
/// 	fn tick(&mut self, context: &mut ComponentContext, time_delta: f64) {
/// 		let transform = context.get_transform_mut();
/// 		let local_rotation = transform.get_local_rotation();
/// 		transform.set_local_rotation(local_rotation + self.degrees_per_second * time_delta);
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let node_id = node_tree.create_child(root_id, "spinner");
/// node_tree[node_id].add_component(Spin {
/// 	component_base: ComponentBase::new(),
/// 	degrees_per_second: 90.0,
/// });
/// node_tree.tick(&engine, 0.5);
/// assert_eq!(node_tree[node_id].get_local_rotation(), 45.0);
/// ```
//==============================================================================
pub trait Component: AsAny + Reflect {
	//==============================================================================
	/// 공통 상태 반환.
	///
	/// # 돌려줌
	/// 구현체가 든 ComponentBase.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase;

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	///
	/// # 돌려줌
	/// 구현체가 든 ComponentBase. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase;

	//==============================================================================
	/// 컴포넌트 이름. (원본 getComponentType)
	///
	/// # 돌려줌
	/// 종류 이름. (보통 구조체 이름 그대로, 반사의 ComponentType 값)
	//==============================================================================
	fn get_component_type(&self) -> &'static str;

	//==============================================================================
	/// 노드에 붙을 때. (WorldNode::add_component 와 NodeTree::add_component 둘 다 부른다)
	///
	/// # 인자
	/// - `_transform`: 붙는 노드의 트랜스폼. (여기서 바꿔도 된다)
	//==============================================================================
	fn attach(&mut self, _transform: &mut NodeTransform) {
	}

	//==============================================================================
	/// 노드에서 떨어질 때. (WorldNode::remove_component 가 부른다)
	///
	/// # 인자
	/// - `_transform`: 떨어지는 노드의 트랜스폼.
	//==============================================================================
	fn detach(&mut self, _transform: &mut NodeTransform) {
	}

	//==============================================================================
	/// 의존 컴포넌트 목록. (붙기 전에 같은 노드에 없으면 만들어 붙인다, 원본 Unity RequireComponent 사상)
	///
	/// # 돌려줌
	/// 의존 컴포넌트 목록. (기본은 빈 목록)
	//==============================================================================
	fn require(&self) -> Vec<RequiredComponent> {
		Vec::new()
	}

	//==============================================================================
	/// 트리 안의 노드에 붙을 때. (NodeTree::add_component 만 부른다. 자식 노드를 만드는 컴포넌트는 여기서 만든다)
	/// - 원본 UIView.attach 가 content 노드를 만들어 붙이는 자리. 이런 컴포넌트는 node_tree.add_component 로 붙인다.
	///
	/// # 인자
	/// - `_node_tree`: 노드가 든 트리. (이 컴포넌트는 그동안 노드에서 빠져 있고 자리표가 대신 든다)
	/// - `_node_id`: 이 컴포넌트가 붙는 노드.
	///
	/// # 예제
	/// 붙을 때 라벨 자식 노드를 만드는 컴포넌트
	/// ```rust
	/// use vanilla::{Component, ComponentBase, NodeId, NodeTree};
	///
	/// struct Badge {
	/// 	component_base: ComponentBase,
	/// 	label_id: Option<NodeId>,
	/// }
	///
	/// #[vanilla::reflect]
	/// impl Badge {}
	///
	/// #[vanilla::reflect]
	/// impl Component for Badge {
	/// 	fn get_component_base(&self) -> &ComponentBase {
	/// 		&self.component_base
	/// 	}
	///
	/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
	/// 		&mut self.component_base
	/// 	}
	///
	/// 	fn get_component_type(&self) -> &'static str {
	/// 		"Badge"
	/// 	}
	///
	/// 	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
	/// 		self.label_id = Some(node_tree.create_child(node_id, "label"));
	/// 	}
	/// }
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_id = node_tree.create_child(root_id, "badge");
	/// let badge = node_tree.add_component(node_id, Badge {
	/// 	component_base: ComponentBase::new(),
	/// 	label_id: None,
	/// }).expect("badge");
	/// let label_id = badge.label_id.expect("label");
	/// assert_eq!(node_tree.get_parent_id(label_id), Some(node_id));
	/// ```
	//==============================================================================
	fn attach_to_tree(&mut self, _node_tree: &mut NodeTree, _node_id: NodeId) {
	}

	//==============================================================================
	/// 주기적 갱신.
	/// - 노드와 조상이 모두 활성이고 이 컴포넌트가 켜져 있고 is_tick_needed 가 true 일 때 NodeTree::tick 이 부른다.
	///
	/// # 인자
	/// - `_context`: 자기 노드, 트리 전체, 엔진을 꺼내는 문맥.
	/// - `_time_delta`: 지난 tick 뒤로 흐른 시간. (초)
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, _time_delta: f64) {
	}

	//==============================================================================
	/// tick 이 할 일이 있는지. (false 면 노드를 돌 때 이 컴포넌트를 부르지 않는다. 기본 true)
	/// - 그리기만 하는 Paint, Sprite, Text, Mask 는 false. 컴포넌트가 이것만 붙은 노드는 tick 때 꺼내지 않는다.
	///
	/// # 돌려줌
	/// tick 을 불러야 하면 true.
	//==============================================================================
	fn is_tick_needed(&self) -> bool {
		true
	}

	//==============================================================================
	/// 노드 출력 상태 시작. (노드 트랜스폼을 건 바로 뒤, 이 노드의 컴포넌트와 자식을 그리기 전. 원본 노드의 pushTransform 을 덮어쓰던 자리)
	/// - 여기서 바꾼 Graphic 상태는 이 노드와 자손 모두에 걸린다. pop_transform 에서 되돌린다. 켜진 컴포넌트만, 붙인 차례로 부른다.
	///
	/// # 인자
	/// - `_transform`: 이 노드의 트랜스폼.
	/// - `_graphic`: 그리는 Graphic. (노드 트랜스폼과 불투명도가 걸려 있다)
	///
	/// # 예제
	/// 노드와 자손을 모두 반투명하게 그리는 컴포넌트
	/// ```rust
	/// use vanilla::{Component, ComponentBase, Graphic, NodeTransform};
	///
	/// struct HalfFade {
	/// 	component_base: ComponentBase,
	/// 	saved_alpha: f64,
	/// }
	///
	/// #[vanilla::reflect]
	/// impl HalfFade {}
	///
	/// #[vanilla::reflect]
	/// impl Component for HalfFade {
	/// 	fn get_component_base(&self) -> &ComponentBase {
	/// 		&self.component_base
	/// 	}
	///
	/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
	/// 		&mut self.component_base
	/// 	}
	///
	/// 	fn get_component_type(&self) -> &'static str {
	/// 		"HalfFade"
	/// 	}
	///
	/// 	fn push_transform(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
	/// 		self.saved_alpha = graphic.get_global_alpha();
	/// 		graphic.multiply_global_alpha(0.5);
	/// 	}
	///
	/// 	fn pop_transform(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
	/// 		graphic.set_global_alpha(self.saved_alpha);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	fn push_transform(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 노드 출력 상태 끝. (자식과 기즈모를 다 그린 뒤, 노드 트랜스폼을 풀기 전. 원본 노드의 popTransform 을 덮어쓰던 자리, 붙인 차례의 거꾸로 부른다)
	///
	/// # 인자
	/// - `_transform`: 이 노드의 트랜스폼.
	/// - `_graphic`: 그리는 Graphic. (push_transform 에서 바꾼 상태를 여기서 되돌린다)
	//==============================================================================
	fn pop_transform(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 출력. (노드 트랜스폼이 걸린 상태에서 불린다. 원점은 노드 content_size 영역의 왼쪽 위)
	///
	/// # 인자
	/// - `_transform`: 이 노드의 트랜스폼. (content_size 로 그릴 크기를 정한다)
	/// - `_graphic`: 그릴 Graphic.
	//==============================================================================
	fn draw(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 기즈모 출력.
	/// - 노드의 기즈모를 그리는 때(노드의 기즈모 표시나 Graphic 의 기즈모 강제 표시)에 이 컴포넌트의 기즈모 표시가 켜져 있으면 불린다.
	///
	/// # 인자
	/// - `_transform`: 이 노드의 트랜스폼.
	/// - `_graphic`: 그릴 Graphic. (노드 트랜스폼이 걸려 있다)
	//==============================================================================
	fn draw_gizmos(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 자식 출력을 이 영역으로 자를지. (Mask 만 Some(둥근 크기) 를 돌려준다)
	///
	/// # 돌려줌
	/// 자를 영역의 귀퉁이 둥근 크기. (노드 좌표, 0 이면 곧은 사각형) 자르지 않으면 None.
	//==============================================================================
	fn get_clip_round_size(&self) -> Option<f64> {
		None
	}

	//==============================================================================
	/// 노드 content_size 영역 안에만 그리는지. (true 면 영역이 화면 밖일 때 엔진이 draw 를 건너뛴다)
	/// - Paint, Sprite, Mask 는 true. 글자, 파티클처럼 영역 밖으로 나가는 것은 false. (기본 false)
	///
	/// # 돌려줌
	/// content_size 영역 안에만 그리면 true.
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		false
	}

	//==============================================================================
	/// draw 를 덮어썼는지. (원본은 component.draw 가 Component.prototype.draw 와 다른지로 본다)
	/// - false 면 아무것도 그리지 않는 컴포넌트라 화면 밖 거르기와 하위 트리 그리기 경계에서 빠진다. (ButtonMotion, UINode 같은)
	/// - #[reflect] 를 붙인 impl Component 블록에 fn draw 가 없으면 매크로가 false 를 돌려주게 짓는다. 매크로 없이 쓰면 기본 true. (어디에 그리는지 모르는 것으로 본다)
	///
	/// # 돌려줌
	/// draw 를 덮어썼으면 true.
	//==============================================================================
	fn is_draw_overridden(&self) -> bool {
		true
	}

	//==============================================================================
	/// 그리는 사각형(calculate_drawn_rect)이 바뀌었음을 노드에 알린다. (원본 Component.markDrawnRectDirty, ComponentBase::mark_drawn_rect_dirty)
	//==============================================================================
	fn mark_drawn_rect_dirty(&self) {
		self.get_component_base().mark_drawn_rect_dirty();
	}

	//==============================================================================
	/// 화면 밖 거르기에 쓰는, 실제로 그리는 사각형. (노드의 content 좌표, draw 와 같은 Graphic 변환이 걸린 때 부른다)
	/// - None 이면 모른다는 뜻이라 노드를 거르지 않는다. 노드의 켜진 컴포넌트가 모두 사각형을 주면 그 합이 화면 밖일 때 거른다.
	/// - 기본은 is_drawn_inside_content 가 true 이고 content_size 가 있으면 content 영역, 아니면 None.
	/// - 글자처럼 정렬, 넘침, 줄바꿈으로 content 밖에도 그리는 컴포넌트는 그리는 사각형을 넉넉히 셈해 돌려준다.
	///
	/// # 인자
	/// - `transform`: 이 노드의 트랜스폼.
	/// - `_graphic`: 그리는 Graphic. (draw 와 같은 변환이 걸려 있다, Text 는 글 크기를 잴 때 쓴다)
	///
	/// # 돌려줌
	/// 그리는 사각형. (노드의 content 좌표, 왼쪽 위가 원점) 모르면 None.
	//==============================================================================
	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, _graphic: &mut Graphic) -> Option<Rect> {
		if !self.is_drawn_inside_content() {
			return None;
		}
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return None;
		}
		Some(Rect::create(0.0, 0.0, content_size.x, content_size.y))
	}

	//==============================================================================
	/// 터치 눌림. (노드가 interactable 이고 맨 위에 있을 때)
	///
	/// # 인자
	/// - `_context`: 자기 노드, 트리 전체, 엔진을 꺼내는 문맥.
	/// - `_view_input_position`: 터치 자리. (뷰 좌표)
	//==============================================================================
	fn touch_press(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 이동. (눌린 노드에)
	///
	/// # 인자
	/// - `_context`: 자기 노드, 트리 전체, 엔진을 꺼내는 문맥.
	/// - `_view_input_position`: 터치 자리. (뷰 좌표)
	//==============================================================================
	fn touch_move(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 뗌. (눌린 노드에)
	///
	/// # 인자
	/// - `_context`: 자기 노드, 트리 전체, 엔진을 꺼내는 문맥.
	/// - `_view_input_position`: 터치 자리. (뷰 좌표)
	//==============================================================================
	fn touch_release(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 취소. (눌린 노드에)
	///
	/// # 인자
	/// - `_context`: 자기 노드, 트리 전체, 엔진을 꺼내는 문맥.
	/// - `_view_input_position`: 터치 자리. (뷰 좌표)
	//==============================================================================
	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 마우스 휠. (터치 아래 노드부터 부모 쪽으로 올라가며 묻는다. 받아 썼으면 true, 원본 UIScrollView.wheel)
	///
	/// # 인자
	/// - `_context`: 자기 노드, 트리 전체, 엔진을 꺼내는 문맥.
	/// - `_wheel_delta`: 이번 프레임의 휠 이동량. (픽셀, 내용을 아래로 넘기면 y 가 양수)
	///
	/// # 돌려줌
	/// 받아 썼으면 true. (부모 쪽으로 더 묻지 않는다)
	//==============================================================================
	fn wheel(&mut self, _context: &mut ComponentContext, _wheel_delta: Vector2) -> bool {
		false
	}

	//==============================================================================
	/// 휠을 받는 칸인지. (스크롤 칸이 true. 그 자리를 덮는 것 가운데 가장 앞이 이 컴포넌트의 노드면 누를 수 없는 것 위의 휠도 받는다, NodeTree::dispatch_wheel_at)
	///
	/// # 돌려줌
	/// 휠을 받는 칸이면 true.
	//==============================================================================
	fn is_wheel_receiver(&self) -> bool {
		false
	}

	//==============================================================================
	/// 자연 크기. (UIKit 의 intrinsicContentSize, UINode 가 모은다. 축마다 -1 이면 선호 없음, 기본은 둘 다 -1)
	/// - 원본은 이 함수가 있는 컴포넌트(Text, RichText)만 UINode 가 읽는다. 러스트 판은 기본값 -1 이 "없음" 자리다.
	///
	/// # 돌려줌
	/// 자연 크기. (축마다 -1 이면 그 축은 선호 없음)
	//==============================================================================
	fn get_intrinsic_content_size(&self) -> Vector2 {
		Vector2::create(-1.0, -1.0)
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (원본 상위 클래스 자리, ShaderSprite 의 Sprite 나 UIButton 의 UIControl 처럼)
	/// - get_component, get_components, has_component, remove_component 가 이 사슬을 따라 원본 instanceof 처럼 기반 타입으로도 찾는다.
	/// - 다른 컴포넌트를 품어 원본 상속을 옮긴 컴포넌트만 덮어쓴다.
	///
	/// # 돌려줌
	/// 품은 기반 컴포넌트. 기반이 없으면 None. (기본)
	//==============================================================================
	fn get_base_component(&self) -> Option<&dyn Component> {
		None
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (변경용)
	///
	/// # 돌려줌
	/// 품은 기반 컴포넌트. (변경용) 기반이 없으면 None. (기본)
	//==============================================================================
	fn get_base_component_mut(&mut self) -> Option<&mut dyn Component> {
		None
	}

	//==============================================================================
	/// 활성화 여부.
	///
	/// # 돌려줌
	/// 켜져 있으면 true. (ComponentBase 의 값)
	//==============================================================================
	fn is_enable(&self) -> bool {
		self.get_component_base().is_enable()
	}

	//==============================================================================
	/// 활성화 설정.
	///
	/// # 인자
	/// - `is_enable`: 켤지. (false 면 tick, 그리기, 터치, 휠에서 빠진다)
	//==============================================================================
	fn set_enable(&mut self, is_enable: bool) {
		self.get_component_base_mut().set_enable(is_enable);
	}

	//==============================================================================
	/// 기즈모 표시 여부.
	///
	/// # 돌려줌
	/// 기즈모를 그리면 true. (ComponentBase 의 값)
	//==============================================================================
	fn is_gizmo_visible(&self) -> bool {
		self.get_component_base().is_gizmo_visible()
	}

	//==============================================================================
	/// 기즈모 표시 설정.
	///
	/// # 인자
	/// - `is_gizmo_visible`: 노드의 기즈모를 그릴 때 이 컴포넌트의 draw_gizmos 도 부를지.
	//==============================================================================
	fn set_gizmo_visible(&mut self, is_gizmo_visible: bool) {
		self.get_component_base_mut().set_gizmo_visible(is_gizmo_visible);
	}
}
