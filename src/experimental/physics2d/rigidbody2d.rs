//==============================================================================
//! 강체 컴포넌트. (노드를 2D 물리 몸에 잇는다)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/rigidbody2d.js 와 같은 API 다.
//! - set_body(world, body_id) 로 월드의 몸 하나를 맡으면, tick 마다 몸의 자리와 돌림을 노드의 로컬 자리와 돌림(도)에 넣는다. (유니티 Rigidbody2D 가 Transform 을 움직이는 것과 같다)
//!   몸이 노드를 움직이고 노드가 몸을 움직이지는 않는다. 노드를 옮기려면 몸의 set_transform 을 부른다.
//! - 월드 좌표가 노드 부모의 좌표와 같다고 본다. 물리 노드들은 월드 공간 하나를 맡은 부모 아래에 둔다.
//! - 보간을 켜면(기본) 월드의 지난 advance 몫으로 앞 걸음과 지금 걸음 사이를 섞는다.
//! - 원본과 다른 점: 월드는 씬과 컴포넌트가 함께 쓰도록 `Rc<RefCell<PhysicsWorld2D>>` 로 받는다.
//!   원본 setBody 처럼 붙은 노드를 바로 몸 자리에 두려면 RigidBody2D::set_body_on_node(node, ..) 를 부른다. (컴포넌트의 set_body 는 노드를 몰라 붙을 때와 tick 에서 둔다)
//!   원본 getBody 자리는 with_body, with_body_mut 이다. (월드를 빌린 동안 몸을 넘긴다)
//!   노드 자리는 엔진 Vector2(f32)라 몸의 f64 자리를 f32 로 반올림해 넣는다. (몸 셈은 f64 그대로)
//==============================================================================
// 옮긴 원문: Box2D 3 의 셈에서 옮김, MIT, THIRD_PARTY_LICENSES.md
use std::cell::RefCell;
use std::rc::Rc;

use super::body2d::Body2D;
use super::physicsid2d::BodyId2D;
use super::physicsworld2d::PhysicsWorld2D;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::{NodeTransform, WorldNode};

//==============================================================================
/// 강체 컴포넌트.
///
/// # 예제
/// 떨어지는 상자 노드
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{Engine, EngineConfiguration, NodeTree};
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, PhysicsVector2, PhysicsWorld2D, Polygon2D, RigidBody2D, ShapeDefinition2D, WorldDefinition2D};
///
/// let world = Rc::new(RefCell::new(PhysicsWorld2D::new(&WorldDefinition2D::new(50.0))));
/// let mut body_definition = BodyDefinition2D::new();
/// body_definition.body_type = BodyType2D::Dynamic;
/// body_definition.position = PhysicsVector2::create(640.0, 100.0);
/// let body_id = world.borrow_mut().create_body(&body_definition);
/// world.borrow_mut().create_shape(body_id, &ShapeDefinition2D::new(), Polygon2D::make_box(40.0, 40.0));
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let box_id = node_tree.create_child(root_id, "box");
/// let mut rigid_body = RigidBody2D::new();
/// rigid_body.set_body(world.clone(), body_id);
/// node_tree[box_id].add_component(rigid_body);
/// for _ in 0..30 {
/// 	world.borrow_mut().advance(1.0 / 60.0);
/// 	node_tree.tick(&engine, 1.0 / 60.0);
/// }
/// assert!(node_tree[box_id].get_local_position().y > 100.0);
/// ```
//==============================================================================
pub struct RigidBody2D {
	component_base: ComponentBase,
	world: Option<Rc<RefCell<PhysicsWorld2D>>>,
	body_id: Option<BodyId2D>,
	is_interpolation_enabled: bool,
}

impl Default for RigidBody2D {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> RigidBody2D {
		RigidBody2D::new()
	}
}

impl RigidBody2D {
	//==============================================================================
	/// 생성. (맡은 몸 없음, 보간 켬)
	///
	/// # 돌려줌
	/// 새 컴포넌트.
	//==============================================================================
	pub fn new() -> RigidBody2D {
		RigidBody2D {
			component_base: ComponentBase::new(),
			world: None,
			body_id: None,
			is_interpolation_enabled: true,
		}
	}

	//==============================================================================
	/// 맡을 몸 설정.
	///
	/// # 인자
	/// - `world`: 몸이 든 월드.
	/// - `body_id`: 맡을 몸.
	//==============================================================================
	pub fn set_body(&mut self, world: Rc<RefCell<PhysicsWorld2D>>, body_id: BodyId2D) {
		self.world = Some(world);
		self.body_id = Some(body_id);
	}

	//==============================================================================
	/// 노드에 붙은 강체의 몸을 정하고 바로 노드를 몸 자리에 둔다. (원본 setBody 와 같다)
	///
	/// # 인자
	/// - `node`: RigidBody2D 가 붙은 노드.
	/// - `world`: 몸이 든 월드.
	/// - `body_id`: 맡을 몸.
	///
	/// # 돌려줌
	/// 노드에 RigidBody2D 가 있어 정했으면 true.
	///
	/// # 예제
	/// ```
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{NodeTree, Vector2};
	/// use vanilla::experimental::physics2d::{BodyDefinition2D, PhysicsVector2, PhysicsWorld2D, RigidBody2D, WorldDefinition2D};
	///
	/// let world = Rc::new(RefCell::new(PhysicsWorld2D::new(&WorldDefinition2D::new(50.0))));
	/// let mut body_definition = BodyDefinition2D::new();
	/// body_definition.position = PhysicsVector2::create(30.0, 40.0);
	/// let body_id = world.borrow_mut().create_body(&body_definition);
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_id = node_tree.create_child(root_id, "body");
	/// node_tree[node_id].add_component(RigidBody2D::new());
	/// assert!(RigidBody2D::set_body_on_node(&mut node_tree[node_id], world.clone(), body_id));
	/// assert_eq!(node_tree[node_id].get_local_position(), Vector2::create(30.0, 40.0));
	/// let rigid_body = node_tree[node_id].get_component::<RigidBody2D>().expect("rigid body");
	/// assert_eq!(rigid_body.with_body(|body| body.get_position().x), Some(30.0));
	/// ```
	//==============================================================================
	pub fn set_body_on_node(node: &mut WorldNode, world: Rc<RefCell<PhysicsWorld2D>>, body_id: BodyId2D) -> bool {
		let Some(rigid_body) = node.get_component_mut::<RigidBody2D>() else {
			return false;
		};
		rigid_body.set_body(world.clone(), body_id);
		synchronize_transform(&world, body_id, node.get_transform_mut(), 1.0);
		true
	}

	//==============================================================================
	/// 맡은 몸 읽기. (원본 getBody 자리, 월드를 빌린 동안 몸을 넘긴다)
	///
	/// # 인자
	/// - `callback`: 몸을 받는 함수.
	///
	/// # 돌려줌
	/// 함수의 값. (몸이 없거나 없어졌으면 None)
	//==============================================================================
	pub fn with_body<R>(&self, callback: impl FnOnce(&Body2D) -> R) -> Option<R> {
		let (Some(world), Some(body_id)) = (self.world.as_ref(), self.body_id) else {
			return None;
		};
		let world = world.borrow();
		world.get_body(body_id).map(callback)
	}

	//==============================================================================
	/// 맡은 몸 고치기. (원본 getBody 로 받은 몸을 고치는 자리)
	///
	/// # 인자
	/// - `callback`: 몸을 받는 함수.
	///
	/// # 돌려줌
	/// 함수의 값. (몸이 없거나 없어졌으면 None)
	//==============================================================================
	pub fn with_body_mut<R>(&self, callback: impl FnOnce(&mut Body2D) -> R) -> Option<R> {
		let (Some(world), Some(body_id)) = (self.world.as_ref(), self.body_id) else {
			return None;
		};
		let mut world = world.borrow_mut();
		world.get_body_mut(body_id).map(callback)
	}

	//==============================================================================
	/// 월드 반환.
	///
	/// # 돌려줌
	/// 몸이 든 월드. (없으면 None)
	//==============================================================================
	pub fn get_world(&self) -> Option<Rc<RefCell<PhysicsWorld2D>>> {
		self.world.clone()
	}

	//==============================================================================
	/// 몸 아이디 반환.
	///
	/// # 돌려줌
	/// 맡은 몸. (없으면 None)
	//==============================================================================
	pub fn get_body_id(&self) -> Option<BodyId2D> {
		self.body_id
	}

	//==============================================================================
	/// 보간을 쓸지 설정.
	///
	/// # 인자
	/// - `is_interpolation_enabled`: 보간을 쓸지.
	//==============================================================================
	pub fn set_interpolation_enabled(&mut self, is_interpolation_enabled: bool) {
		self.is_interpolation_enabled = is_interpolation_enabled;
	}

	//==============================================================================
	/// 보간을 쓰는지.
	///
	/// # 돌려줌
	/// 보간을 쓰면 true.
	//==============================================================================
	pub fn is_interpolation_enabled(&self) -> bool {
		self.is_interpolation_enabled
	}

	//==============================================================================
	/// 노드를 몸 자리에 두기. (alpha 는 보간 몫, 몸이 없으면 그대로 둔다)
	///
	/// # 인자
	/// - `transform`: 노드 트랜스폼.
	/// - `alpha`: 앞 걸음과 지금 걸음 사이 몫. (1 이면 지금 걸음)
	//==============================================================================
	pub fn synchronize_node(&self, transform: &mut NodeTransform, alpha: f64) {
		if let (Some(world), Some(body_id)) = (self.world.as_ref(), self.body_id) {
			synchronize_transform(world, body_id, transform, alpha);
		}
	}
}

//==============================================================================
// 노드 트랜스폼을 몸 자리에 두기.
///
/// # 인자
/// - `world`: 몸이 든 월드.
/// - `body_id`: 몸.
/// - `transform`: 노드 트랜스폼.
/// - `alpha`: 앞 걸음과 지금 걸음 사이 몫. (1 이면 지금 걸음)
//==============================================================================
fn synchronize_transform(world: &Rc<RefCell<PhysicsWorld2D>>, body_id: BodyId2D, transform: &mut NodeTransform, alpha: f64) {
	let world = world.borrow();
	let Some(body) = world.get_body(body_id) else {
		return;
	};
	let position = body.get_interpolated_position(alpha);
	let angle = body.get_interpolated_angle(alpha);
	transform.set_local_position(position.to_vector2());
	transform.set_local_rotation((angle * 180.0 / std::f64::consts::PI) as f32);
}

impl Component for RigidBody2D {
	//==============================================================================
	/// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"RigidBody2D"
	}

	//==============================================================================
	/// 붙음. (노드를 바로 몸 자리에 둔다)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		self.synchronize_node(transform, 1.0);
	}

	//==============================================================================
	/// 갱신. (몸의 자리와 돌림을 노드에 넣는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let Some(world) = self.world.as_ref() else {
			return;
		};
		let alpha = if self.is_interpolation_enabled {
			world.borrow().get_interpolation_alpha()
		}
		else {
			1.0
		};
		self.synchronize_node(context.get_transform_mut(), alpha);
	}
}
