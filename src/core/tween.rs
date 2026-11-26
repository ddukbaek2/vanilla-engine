//==============================================================================
//! 트윈.
//! - 원본 core/tween.js 를 옮긴 것이다. 시작값 묶음(이름 → 숫자 또는 Vector2)과 목표값 묶음 사이를
//!   대기(delay) 뒤 duration 초 동안 이징 함수로 보간하고, tick 마다 보간한 값 묶음을 갱신 콜백에 넘긴다.
//!   다 가면 멈추고 완료 콜백을 부른다. 반복이나 요요는 원본에 없어서 옮기지 않았다.
//! - 원본의 Tween.easing_function.back.out 은 easing_function::back::out 이다.
//!   in 은 러스트 예약어라 easing_function::quadratic::r#in 처럼 r# 을 붙여 부른다. (이름은 원본 그대로)
//! - 원본의 콜백은 노드 객체를 붙잡고 node.set_position(...) 을 부른다. 러스트 판은 노드가 씬의 NodeTree 에 있어서
//!   갱신 콜백과 완료 콜백이 &mut NodeTree 를 함께 받는다. (붙잡아 둔 NodeId 로 노드를 꺼내 쓴다)
//! - set_target(node_id): 값 이름이 노드 속성 이름(TweenProperty, 예: "localPosition")이면 tick 마다 그 노드에 바로 쓴다.
//!   대상 노드가 지워지면 콜백 없이 조용히 끝난다. create_node_tween 은 그것의 줄임이다. (원본 setTarget(node), createNodeTween)
//!   원본처럼 처음 돈 때의 노드 트리에서 떨어지면(맨 위 노드가 달라지면) 끝난다. 러스트 판은 destroy_node 로 지워져도 끝난다.
//! - 이징은 원본과 같은 공식이다. (원본 base/math.js 의 pow 도 내장 거듭제곱이라 exponential, elastic 값이 같다)
//! - duration 이 0 이하이면 바로 끝(진행률 1)으로 친다. (원본도 같다)
//! - 씬은 SceneBase::start_tween 으로 트윈을 넘겨받아 들고, 노드 tick → 터치 다음에 돌리며 끝난 트윈을 뺀다.
//!   원본은 트윈 객체를 들고 stop_tween(tween) 으로 가리키지만, 러스트 판은 씬이 트윈을 가져가므로(소유권이 넘어간다)
//!   start_tween 이 돌려준 TweenId 로 가리킨다. get_tween / get_tween_mut 은 그 때문에 있다.
//==============================================================================
// 옮긴 원문: tween.js 와 Robert Penner 의 이징 식에서 옮김, MIT 와 BSD-3-Clause, THIRD_PARTY_LICENSES.md
use crate::base::identifier;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::WorldNode;
use std::borrow::Cow;

//==============================================================================
/// 트윈 값. (원본은 숫자와 Vector2 만 보간한다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TweenValue {
	/// 숫자 하나. (불투명도, 회전 같은 값)
	Number(f32),
	/// 2차원 벡터. (위치, 배율, 크기 같은 값, 두 축을 따로 보간한다)
	Vector2(Vector2),
}

impl From<f32> for TweenValue {
	//==============================================================================
	/// 숫자 → 트윈 값.
	//==============================================================================
	fn from(number: f32) -> TweenValue {
		TweenValue::Number(number)
	}
}

impl From<Vector2> for TweenValue {
	//==============================================================================
	/// Vector2 → 트윈 값.
	//==============================================================================
	fn from(vector: Vector2) -> TweenValue {
		TweenValue::Vector2(vector)
	}
}

//==============================================================================
/// 이름 붙은 트윈 값 하나.
//==============================================================================
#[derive(Clone, Debug)]
struct TweenValueEntry {
	name: Cow<'static, str>,
	value: TweenValue,
}

//==============================================================================
/// 트윈 값 묶음. (원본의 { x: 0, y: 0 } 같은 객체 자리, 넣은 순서를 지킨다)
/// - 이름은 보통 문자열 상수("x")를 쓴다. 상수 이름은 복사할 때 메모리를 새로 잡지 않는다.
///
/// # 예제
/// 숫자와 Vector2 를 담고 꺼내기
/// ```rust
/// use vanilla::{TweenValue, TweenValues, Vector2};
///
/// let mut values = TweenValues::new()
/// 	.with("alpha", 0.0)
/// 	.with("position", Vector2::create(10.0, 20.0));
/// values.set("alpha", 1.0);
/// assert_eq!(values.get_count(), 2);
/// assert_eq!(values.get_number("alpha"), Some(1.0));
/// assert_eq!(values.get("position"), Some(TweenValue::Vector2(Vector2::create(10.0, 20.0))));
/// assert_eq!(values.get_number("position"), None);
/// assert_eq!(values.get_vector2("missing"), None);
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct TweenValues {
	entries: Vec<TweenValueEntry>,
}

impl TweenValues {
	//==============================================================================
	/// 생성. (비어 있다)
	///
	/// # 돌려줌
	/// 빈 묶음.
	//==============================================================================
	pub fn new() -> TweenValues {
		TweenValues {
			entries: Vec::new(),
		}
	}

	//==============================================================================
	/// 값을 넣고 자신을 돌려준다. (이어 쓰기용, 같은 이름이 있으면 바꾼다)
	///
	/// # 인자
	/// - `name`: 값 이름. (보통 문자열 상수, TweenProperty 이름이면 대상 노드의 그 속성에 쓴다)
	/// - `value`: 값. (f32 또는 Vector2)
	///
	/// # 돌려줌
	/// 값을 넣은 자신.
	//==============================================================================
	pub fn with(mut self, name: impl Into<Cow<'static, str>>, value: impl Into<TweenValue>) -> TweenValues {
		self.set(name, value);
		self
	}

	//==============================================================================
	/// 값 설정. (같은 이름이 있으면 바꾸고, 없으면 뒤에 붙인다)
	///
	/// # 인자
	/// - `name`: 값 이름.
	/// - `value`: 값. (f32 또는 Vector2, 같은 이름의 값을 다른 종류로 바꿀 수도 있다)
	//==============================================================================
	pub fn set(&mut self, name: impl Into<Cow<'static, str>>, value: impl Into<TweenValue>) {
		let name = name.into();
		let value = value.into();
		for entry in self.entries.iter_mut() {
			if entry.name == name {
				entry.value = value;
				return;
			}
		}
		self.entries.push(TweenValueEntry {
			name,
			value,
		});
	}

	//==============================================================================
	/// 값 반환. (없으면 None)
	///
	/// # 인자
	/// - `name`: 찾을 값 이름.
	///
	/// # 돌려줌
	/// 그 이름의 값. 없으면 None.
	//==============================================================================
	pub fn get(&self, name: &str) -> Option<TweenValue> {
		self.entries.iter().find(|entry| entry.name == name).map(|entry| entry.value)
	}

	//==============================================================================
	/// 숫자 값 반환. (없거나 숫자가 아니면 None)
	///
	/// # 인자
	/// - `name`: 찾을 값 이름.
	///
	/// # 돌려줌
	/// 그 이름의 숫자. 없거나 Vector2 면 None.
	//==============================================================================
	pub fn get_number(&self, name: &str) -> Option<f32> {
		match self.get(name) {
			Some(TweenValue::Number(number)) => {
				Some(number)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// Vector2 값 반환. (없거나 Vector2 가 아니면 None)
	///
	/// # 인자
	/// - `name`: 찾을 값 이름.
	///
	/// # 돌려줌
	/// 그 이름의 Vector2. 없거나 숫자면 None.
	//==============================================================================
	pub fn get_vector2(&self, name: &str) -> Option<Vector2> {
		match self.get(name) {
			Some(TweenValue::Vector2(vector)) => {
				Some(vector)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 값 수.
	///
	/// # 돌려줌
	/// 든 값의 수.
	//==============================================================================
	pub fn get_count(&self) -> usize {
		self.entries.len()
	}

	//==============================================================================
	/// 모두 비운다. (잡아 둔 메모리는 남긴다)
	//==============================================================================
	fn clear(&mut self) {
		self.entries.clear();
	}

	//==============================================================================
	/// 같은 이름을 찾지 않고 뒤에 붙인다. (tick 이 보간 결과를 채울 때)
	//==============================================================================
	fn push(&mut self, name: Cow<'static, str>, value: TweenValue) {
		self.entries.push(TweenValueEntry {
			name,
			value,
		});
	}
}

//==============================================================================
/// 트윈이 바로 쓰는 노드 속성. (set_target 으로 대상 노드를 정했을 때, 값 이름이 이 이름이면 그 노드에 쓴다)
/// - Vector2 속성에 숫자를 주면 두 축에 같이 쓴다. (원본 예제의 set_scale(Vector2.one().multiply(v.scale)) 자리)
/// - 숫자 속성에 Vector2 를 주면 쓰지 않는다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TweenProperty {
	/// 로컬 위치. (이름 "localPosition", Vector2 또는 두 축에 같이 쓸 숫자)
	LocalPosition,
	/// 로컬 배율. (이름 "localScale", Vector2 또는 두 축에 같이 쓸 숫자)
	LocalScale,
	/// 로컬 회전. (이름 "localRotation", 도, 숫자만)
	LocalRotation,
	/// 로컬 불투명도. (이름 "localOpacity", 숫자만, 노드가 0 ~ 1 로 맞춘다)
	LocalOpacity,
	/// 내용 크기. (이름 "contentSize", Vector2 또는 두 축에 같이 쓸 숫자)
	ContentSize,
}

impl TweenProperty {
	//==============================================================================
	/// 값 묶음에서 쓰는 이름. (enum 이름의 첫 글자를 소문자로 쓴 원본 속성 이름, localPosition 같은)
	///
	/// # 돌려줌
	/// 값 묶음에서 쓰는 속성 이름. ("localPosition" 처럼 첫 글자가 소문자)
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		match self {
			TweenProperty::LocalPosition => {
				"localPosition"
			}
			TweenProperty::LocalScale => {
				"localScale"
			}
			TweenProperty::LocalRotation => {
				"localRotation"
			}
			TweenProperty::LocalOpacity => {
				"localOpacity"
			}
			TweenProperty::ContentSize => {
				"contentSize"
			}
		}
	}

	//==============================================================================
	/// 이름으로 찾기. (노드 속성 이름이 아니면 None)
	///
	/// # 인자
	/// - `name`: 값 이름. (get_name 이 돌려주는 이름과 견준다, 대소문자를 가린다)
	///
	/// # 돌려줌
	/// 그 이름의 속성. 노드 속성 이름이 아니면 None.
	///
	/// # 예제
	/// 속성 이름과 아닌 이름
	/// ```rust
	/// use vanilla::TweenProperty;
	///
	/// assert_eq!(TweenProperty::find_by_name("localScale"), Some(TweenProperty::LocalScale));
	/// assert_eq!(TweenProperty::find_by_name("scale"), None);
	/// assert_eq!(TweenProperty::LocalScale.get_name(), "localScale");
	/// ```
	//==============================================================================
	pub fn find_by_name(name: &str) -> Option<TweenProperty> {
		match name {
			"localPosition" => {
				Some(TweenProperty::LocalPosition)
			}
			"localScale" => {
				Some(TweenProperty::LocalScale)
			}
			"localRotation" => {
				Some(TweenProperty::LocalRotation)
			}
			"localOpacity" => {
				Some(TweenProperty::LocalOpacity)
			}
			"contentSize" => {
				Some(TweenProperty::ContentSize)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 노드에 값 쓰기.
	/// - Vector2 속성(위치, 배율, 내용 크기)에 숫자를 주면 두 축에 같이 쓴다. 숫자 속성(회전, 불투명도)에 Vector2 를 주면 쓰지 않는다.
	///
	/// # 인자
	/// - `node`: 값을 쓸 노드.
	/// - `value`: 쓸 값.
	///
	/// # 예제
	/// 숫자 하나로 두 축 배율 쓰기
	/// ```rust
	/// use vanilla::{TweenProperty, TweenValue, Vector2, WorldNode};
	///
	/// let mut node = WorldNode::new("Node");
	/// TweenProperty::LocalScale.apply_to_node(&mut node, TweenValue::Number(2.0));
	/// assert_eq!(node.get_local_scale(), Vector2::create(2.0, 2.0));
	///
	/// TweenProperty::LocalRotation.apply_to_node(&mut node, TweenValue::Vector2(Vector2::create(1.0, 1.0)));
	/// assert_eq!(node.get_local_rotation(), 0.0);
	/// ```
	//==============================================================================
	pub fn apply_to_node(&self, node: &mut WorldNode, value: TweenValue) {
		let vector = match value {
			TweenValue::Number(number) => {
				Vector2::create(number, number)
			}
			TweenValue::Vector2(vector) => {
				vector
			}
		};
		match self {
			TweenProperty::LocalPosition => {
				node.set_local_position(vector);
			}
			TweenProperty::LocalScale => {
				node.set_local_scale(vector);
			}
			TweenProperty::ContentSize => {
				node.set_content_size(vector);
			}
			TweenProperty::LocalRotation => {
				if let TweenValue::Number(number) = value {
					node.set_local_rotation(number);
				}
			}
			TweenProperty::LocalOpacity => {
				if let TweenValue::Number(number) = value {
					node.set_local_opacity(number);
				}
			}
		}
	}
}

//==============================================================================
/// 트윈 핸들. (SceneBase::start_tween 이 돌려주고, stop_tween / get_tween 에 넘긴다)
/// - 트윈을 만들 때 정해지고 트윈마다 다르다. 씬이 트윈을 가져간 뒤에는 이것으로 가리킨다.
///
/// # 예제
/// 씬에 맡긴 트윈을 핸들로 찾고 멈추기
/// ```rust
/// use vanilla::{SceneBase, Tween, TweenValues};
///
/// let mut scene_base = SceneBase::new();
/// let tween = Tween::new(TweenValues::new().with("value", 0.0)).to(TweenValues::new().with("value", 1.0), 2.0);
/// let tween_id = scene_base.start_tween(tween);
/// assert_eq!(scene_base.get_tween(tween_id).unwrap().get_tween_id(), tween_id);
///
/// scene_base.stop_tween(tween_id);
/// assert!(scene_base.get_tween(tween_id).is_none());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TweenId(u64);

/// 갱신 콜백. (보간한 값 묶음과 노드 트리를 받는다)
type TweenUpdateCallback = Box<dyn FnMut(&TweenValues, &mut NodeTree)>;

/// 완료 콜백. (노드 트리를 받는다)
type TweenCompleteCallback = Box<dyn FnMut(&mut NodeTree)>;

//==============================================================================
/// 트윈.
/// - 시작값 묶음과 목표값 묶음 사이를 대기 뒤 길이(초) 동안 이징 함수로 보간한다. tick 마다 보간한 값을 갱신 콜백에 넘기고, 다 가면 완료 콜백을 부른다.
/// - 만든 뒤 to, delay, easing, set_update, set_complete, set_target 을 이어 붙여 정하고, start 한 뒤 tick 을 부른다. (씬은 SceneBase::start_tween 이 start 와 tick 을 맡는다)
///
/// # 예제
/// 대기 뒤 0 에서 10 으로
/// ```rust
/// use vanilla::{easing_function, NodeTree, Tween, TweenValues};
/// use std::cell::Cell;
/// use std::rc::Rc;
///
/// let mut node_tree = NodeTree::new();
/// let last_value = Rc::new(Cell::new(-1.0f32));
/// let last_value_in_callback = last_value.clone();
/// let mut tween = Tween::new(TweenValues::new().with("value", 0.0))
/// 	.to(TweenValues::new().with("value", 10.0), 1.0)
/// 	.delay(0.5)
/// 	.easing(easing_function::linear)
/// 	.set_update(move |values, _node_tree| {
/// 		last_value_in_callback.set(values.get_number("value").unwrap());
/// 	});
/// tween.start();
/// tween.tick(&mut node_tree, 0.25);
/// assert_eq!(last_value.get(), -1.0);
/// tween.tick(&mut node_tree, 0.75);
/// assert_eq!(last_value.get(), 5.0);
/// tween.tick(&mut node_tree, 0.5);
/// assert_eq!(last_value.get(), 10.0);
/// assert!(tween.is_finished());
/// ```
//==============================================================================
pub struct Tween {
	tween_id: TweenId,
	values_start: TweenValues,
	values_end: TweenValues,
	current_values: TweenValues,
	duration: f32,
	delay: f32,
	elapsed_time: f32,
	easing_function: Box<dyn Fn(f32) -> f32>,
	tick_callback: Option<TweenUpdateCallback>,
	complete_callback: Option<TweenCompleteCallback>,
	target_node_id: Option<NodeId>,
	/// 처음 돈 때 대상 노드의 맨 위 노드. (달라지면 트리에서 떨어진 것으로 보고 끝낸다)
	target_root_node_id: Option<NodeId>,
	is_playing: bool,
	is_finished: bool,
}

impl Tween {
	//==============================================================================
	/// 생성. (시작값 묶음을 받는다. 길이 1초, 대기 0, linear)
	///
	/// # 인자
	/// - `initial_values`: 시작값 묶음. (목표값에 같은 이름이 있는 값만 보간한다)
	///
	/// # 돌려줌
	/// 목표값이 비고 멈춘 새 트윈. (start 해야 돈다)
	//==============================================================================
	pub fn new(initial_values: TweenValues) -> Tween {
		let instance_id = identifier::create_instance_id();
		Tween {
			tween_id: TweenId(instance_id),
			values_start: initial_values,
			values_end: TweenValues::new(),
			current_values: TweenValues::new(),
			duration: 1.0,
			delay: 0.0,
			elapsed_time: 0.0,
			easing_function: Box::new(easing_function::linear),
			tick_callback: None,
			complete_callback: None,
			target_node_id: None,
			target_root_node_id: None,
			is_playing: false,
			is_finished: false,
		}
	}

	//==============================================================================
	/// 노드 속성 하나를 움직이는 트윈 생성.
	/// - Tween::new(시작값).to(목표값, duration).set_target(node_id) 와 같다. 값 이름은 속성 이름이다.
	///
	/// # 인자
	/// - `node_id`: 움직일 노드.
	/// - `tween_property`: 움직일 노드 속성.
	/// - `start_value`: 시작값. (f32 또는 Vector2, Vector2 속성에 숫자를 주면 두 축에 같이 쓴다)
	/// - `end_value`: 목표값. (start_value 와 같은 종류)
	/// - `duration`: 길이. (초, 0 이하면 첫 tick 에 끝난다)
	///
	/// # 돌려줌
	/// 대상 노드가 정해진 멈춘 새 트윈. (start 하거나 SceneBase::start_tween 에 넘긴다)
	///
	/// # 예제
	/// 노드를 1 초 동안 오른쪽 아래로
	/// ```rust
	/// use vanilla::{NodeTree, Tween, TweenProperty, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_id = node_tree.create_child(root_id, "Mover");
	/// let mut tween = Tween::create_node_tween(node_id, TweenProperty::LocalPosition, Vector2::zero(), Vector2::create(100.0, 50.0), 1.0);
	/// tween.start();
	/// tween.tick(&mut node_tree, 0.5);
	/// assert_eq!(node_tree[node_id].get_local_position(), Vector2::create(50.0, 25.0));
	/// ```
	//==============================================================================
	pub fn create_node_tween(node_id: NodeId, tween_property: TweenProperty, start_value: impl Into<TweenValue>, end_value: impl Into<TweenValue>, duration: f32) -> Tween {
		let property_name = tween_property.get_name();
		let initial_values = TweenValues::new().with(property_name, start_value);
		let target_values = TweenValues::new().with(property_name, end_value);
		Tween::new(initial_values).to(target_values, duration).set_target(node_id)
	}

	//==============================================================================
	/// 노드의 맨 위 노드 반환. (부모를 따라 올라간다, 원본 Tween.findRootNode)
	///
	/// # 인자
	/// - `node_tree`: 노드가 든 트리.
	/// - `node_id`: 올라가기 시작할 노드.
	///
	/// # 돌려줌
	/// 부모가 없는 맨 위 노드. (node_id 에 부모가 없으면 node_id 그대로)
	///
	/// # 예제
	/// 루트에 붙은 노드와 떨어진 노드
	/// ```rust
	/// use vanilla::{NodeTree, Tween};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let parent_id = node_tree.create_child(root_id, "Parent");
	/// let child_id = node_tree.create_child(parent_id, "Child");
	/// assert_eq!(Tween::find_root_node(&node_tree, child_id), root_id);
	///
	/// let loose_id = node_tree.create_node("Loose");
	/// assert_eq!(Tween::find_root_node(&node_tree, loose_id), loose_id);
	/// ```
	//==============================================================================
	pub fn find_root_node(node_tree: &NodeTree, node_id: NodeId) -> NodeId {
		let mut current_node_id = node_id;
		while let Some(parent_node_id) = node_tree.get_parent_id(current_node_id) {
			current_node_id = parent_node_id;
		}
		current_node_id
	}

	//==============================================================================
	/// 갱신.
	/// - 대기 시간이 지나면 진행률(0 ~ 1)에 이징을 걸어 목표값 이름 순서로 보간한다.
	///   시작값에 없는 이름, 시작값과 종류(숫자 / Vector2)가 다른 이름은 건너뛴다. (원본과 같음)
	/// - 대상 노드가 있으면 노드 속성 이름의 값을 그 노드에 쓰고, 갱신 콜백을 부른다.
	/// - 진행률이 1 이 되면 멈추고 완료 콜백을 부른다.
	/// - start 전이나 끝난 뒤에는 아무것도 하지 않는다. 대상 노드가 지워졌거나 처음 돈 때의 맨 위 노드에서 떨어졌으면 콜백 없이 끝낸다.
	///
	/// # 인자
	/// - `node_tree`: 대상 노드에 쓰고 콜백에 넘길 노드 트리.
	/// - `time_delta`: 지난 시간. (초)
	///
	/// # 예제
	/// 노드를 흐리게 하다가 노드가 지워지면 조용히 끝나기
	/// ```rust
	/// use vanilla::{NodeTree, Tween, TweenProperty};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_id = node_tree.create_child(root_id, "Fader");
	/// let mut tween = Tween::create_node_tween(node_id, TweenProperty::LocalOpacity, 1.0, 0.0, 1.0);
	/// tween.tick(&mut node_tree, 0.5);
	/// assert_eq!(node_tree[node_id].get_local_opacity(), 1.0);
	///
	/// tween.start();
	/// tween.tick(&mut node_tree, 0.5);
	/// assert_eq!(node_tree[node_id].get_local_opacity(), 0.5);
	///
	/// node_tree.destroy_node(node_id);
	/// tween.tick(&mut node_tree, 0.5);
	/// assert!(tween.is_finished());
	/// ```
	//==============================================================================
	pub fn tick(&mut self, node_tree: &mut NodeTree, time_delta: f32) {
		if !self.is_playing || self.is_finished {
			return;
		}

		// 대상 노드가 지워졌거나 처음 돈 때의 노드 트리에서 떨어졌으면 콜백 없이 끝낸다.
		if let Some(target_node_id) = self.target_node_id {
			if !node_tree.contains(target_node_id) {
				self.stop();
				return;
			}
			let target_root_node_id = Tween::find_root_node(node_tree, target_node_id);
			match self.target_root_node_id {
				None => {
					self.target_root_node_id = Some(target_root_node_id);
				}
				Some(first_root_node_id) => {
					if first_root_node_id != target_root_node_id {
						self.stop();
						return;
					}
				}
			}
		}

		self.elapsed_time += time_delta;
		if self.elapsed_time < self.delay {
			return;
		}

		let progress = if self.duration > 0.0 {
			let raw_progress = (self.elapsed_time - self.delay) / self.duration;
			math::clamp(raw_progress, 0.0, 1.0)
		}
		else {
			1.0
		};
		let eased_progress = (self.easing_function)(progress);

		// 보간.
		self.current_values.clear();
		for end_entry in self.values_end.entries.iter() {
			let start_value = match self.values_start.get(&end_entry.name) {
				Some(start_value) => {
					start_value
				}
				None => {
					continue;
				}
			};
			match (start_value, end_entry.value) {
				// 일반 숫자.
				(TweenValue::Number(start_number), TweenValue::Number(end_number)) => {
					let number = math::lerp(start_number, end_number, eased_progress);
					self.current_values.push(end_entry.name.clone(), TweenValue::Number(number));
				}
				// 2차원 벡터.
				(TweenValue::Vector2(start_vector), TweenValue::Vector2(end_vector)) => {
					let interpolated_x = math::lerp(start_vector.x, end_vector.x, eased_progress);
					let interpolated_y = math::lerp(start_vector.y, end_vector.y, eased_progress);
					let interpolated_vector = Vector2::create(interpolated_x, interpolated_y);
					self.current_values.push(end_entry.name.clone(), TweenValue::Vector2(interpolated_vector));
				}
				_ => {
				}
			}
		}

		// 대상 노드에 쓰기.
		if let Some(target_node_id) = self.target_node_id {
			if let Some(target_node) = node_tree.get_node_mut(target_node_id) {
				for entry in self.current_values.entries.iter() {
					if let Some(tween_property) = TweenProperty::find_by_name(&entry.name) {
						tween_property.apply_to_node(target_node, entry.value);
					}
				}
			}
		}

		if let Some(tick_callback) = self.tick_callback.as_mut() {
			tick_callback(&self.current_values, node_tree);
		}

		if progress >= 1.0 {
			self.stop();
			if let Some(complete_callback) = self.complete_callback.as_mut() {
				complete_callback(node_tree);
			}
		}
	}

	//==============================================================================
	/// 목표값 설정. (길이도 함께 정한다)
	///
	/// # 인자
	/// - `properties`: 목표값 묶음. (이 묶음의 순서로 보간해 갱신 콜백에 넘긴다)
	/// - `duration`: 길이. (초, 대기 시간은 빼고, 0 이하면 첫 tick 에 끝난다)
	///
	/// # 돌려줌
	/// 목표값을 정한 자신.
	//==============================================================================
	pub fn to(mut self, properties: TweenValues, duration: f32) -> Tween {
		self.values_end = properties;
		self.duration = duration;
		self
	}

	//==============================================================================
	/// 시작 전 대기 시간 설정. (초)
	///
	/// # 인자
	/// - `amount`: start 한 뒤 보간을 시작하기까지 기다릴 시간. (초, 처음 0)
	///
	/// # 돌려줌
	/// 대기 시간을 정한 자신.
	//==============================================================================
	pub fn delay(mut self, amount: f32) -> Tween {
		self.delay = amount;
		self
	}

	//==============================================================================
	/// 이징 함수 설정. (easing_function 모듈의 함수, 또는 진행률을 받아 비율을 돌려주는 아무 함수)
	///
	/// # 인자
	/// - `easing_function`: 진행률(0 ~ 1)을 받아 보간 비율을 돌려주는 함수. (처음 easing_function::linear)
	///
	/// # 돌려줌
	/// 이징 함수를 정한 자신.
	///
	/// # 예제
	/// 네 계단으로 끊어 움직이기
	/// ```rust
	/// use vanilla::{NodeTree, Tween, TweenValues};
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	///
	/// let mut node_tree = NodeTree::new();
	/// let last_value = Rc::new(Cell::new(0.0f32));
	/// let last_value_in_callback = last_value.clone();
	/// let mut tween = Tween::new(TweenValues::new().with("value", 0.0))
	/// 	.to(TweenValues::new().with("value", 8.0), 1.0)
	/// 	.easing(|progress| (progress * 4.0).floor() / 4.0)
	/// 	.set_update(move |values, _node_tree| {
	/// 		last_value_in_callback.set(values.get_number("value").unwrap());
	/// 	});
	/// tween.start();
	/// tween.tick(&mut node_tree, 0.3);
	/// assert_eq!(last_value.get(), 2.0);
	/// ```
	//==============================================================================
	pub fn easing(mut self, easing_function: impl Fn(f32) -> f32 + 'static) -> Tween {
		self.easing_function = Box::new(easing_function);
		self
	}

	//==============================================================================
	/// 갱신 콜백 설정. (보간한 값 묶음과 노드 트리를 받는다)
	///
	/// # 인자
	/// - `callback`: 보간한 값 묶음과 노드 트리를 받는 함수. (대기가 끝난 뒤 tick 마다, 끝나는 tick 에도 불린다, 대상 노드에 쓴 다음에 불린다)
	///
	/// # 돌려줌
	/// 갱신 콜백을 정한 자신.
	///
	/// # 예제
	/// 보간한 값으로 다른 노드 옮기기
	/// ```rust
	/// use vanilla::{NodeTree, Tween, TweenValues, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let label_id = node_tree.create_child(root_id, "Label");
	/// let mut tween = Tween::new(TweenValues::new().with("x", 0.0))
	/// 	.to(TweenValues::new().with("x", 200.0), 2.0)
	/// 	.set_update(move |values, node_tree| {
	/// 		let position_x = values.get_number("x").unwrap();
	/// 		node_tree[label_id].set_local_position(Vector2::create(position_x, 0.0));
	/// 	});
	/// tween.start();
	/// tween.tick(&mut node_tree, 1.0);
	/// assert_eq!(node_tree[label_id].get_local_position(), Vector2::create(100.0, 0.0));
	/// ```
	//==============================================================================
	pub fn set_update(mut self, callback: impl FnMut(&TweenValues, &mut NodeTree) + 'static) -> Tween {
		self.tick_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 완료 콜백 설정. (노드 트리를 받는다)
	///
	/// # 인자
	/// - `callback`: 노드 트리를 받는 함수. (진행률이 1 이 된 tick 에 갱신 콜백 다음 한 번 불린다, stop 이나 대상 노드가 사라져 끝나면 불리지 않는다)
	///
	/// # 돌려줌
	/// 완료 콜백을 정한 자신.
	///
	/// # 예제
	/// 다 흐려지면 노드 지우기
	/// ```rust
	/// use vanilla::{NodeTree, Tween, TweenProperty};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let toast_id = node_tree.create_child(root_id, "Toast");
	/// let mut tween = Tween::create_node_tween(toast_id, TweenProperty::LocalOpacity, 1.0, 0.0, 0.5)
	/// 	.set_complete(move |node_tree| {
	/// 		node_tree.destroy_node(toast_id);
	/// 	});
	/// tween.start();
	/// tween.tick(&mut node_tree, 0.5);
	/// assert!(tween.is_finished());
	/// assert!(!node_tree.contains(toast_id));
	/// ```
	//==============================================================================
	pub fn set_complete(mut self, callback: impl FnMut(&mut NodeTree) + 'static) -> Tween {
		self.complete_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 대상 노드 설정. (값 이름이 TweenProperty 이름이면 이 노드에 쓴다)
	///
	/// # 인자
	/// - `node_id`: 대상 노드. (지워지거나 처음 돈 때의 맨 위 노드에서 떨어지면 트윈이 조용히 끝난다)
	///
	/// # 돌려줌
	/// 대상 노드를 정한 자신.
	///
	/// # 예제
	/// 배율과 회전을 함께 움직이기
	/// ```rust
	/// use vanilla::{NodeTree, Tween, TweenValues, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let card_id = node_tree.create_child(root_id, "Card");
	/// let mut tween = Tween::new(TweenValues::new().with("localScale", 0.5).with("localRotation", 0.0))
	/// 	.to(TweenValues::new().with("localScale", 1.0).with("localRotation", 90.0), 1.0)
	/// 	.set_target(card_id);
	/// tween.start();
	/// tween.tick(&mut node_tree, 1.0);
	/// assert_eq!(node_tree[card_id].get_local_scale(), Vector2::create(1.0, 1.0));
	/// assert_eq!(node_tree[card_id].get_local_rotation(), 90.0);
	/// ```
	//==============================================================================
	pub fn set_target(mut self, node_id: NodeId) -> Tween {
		self.target_node_id = Some(node_id);
		self
	}

	//==============================================================================
	/// 시작. (이미 재생 중이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn start(&mut self) {
		if self.is_playing {
			return;
		}

		self.is_playing = true;
		self.is_finished = false;
		self.elapsed_time = 0.0;
	}

	//==============================================================================
	/// 정지. (완료 콜백은 부르지 않는다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.is_playing = false;
		self.is_finished = true;
	}

	//==============================================================================
	/// 완료 여부 반환.
	///
	/// # 돌려줌
	/// 끝났으면 true. (다 갔거나, stop 했거나, 대상 노드가 사라졌을 때, start 하면 다시 false)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.is_finished
	}

	//==============================================================================
	/// 핸들 반환.
	///
	/// # 돌려줌
	/// 이 트윈의 핸들. (만들 때 정해진다)
	//==============================================================================
	pub fn get_tween_id(&self) -> TweenId {
		self.tween_id
	}

	//==============================================================================
	/// 대상 노드 핸들 반환. (set_target 을 하지 않았으면 None)
	///
	/// # 돌려줌
	/// 대상 노드. set_target 을 하지 않았으면 None.
	//==============================================================================
	pub fn get_target_node_id(&self) -> Option<NodeId> {
		self.target_node_id
	}
}

//==============================================================================
/// 이징 함수. (원본 Tween.easingFunction)
/// - 진행률(0 ~ 1)을 받아 보간 비율을 돌려준다. back, elastic 은 0 ~ 1 밖으로 넘친다.
/// - 원본은 인자 k 를 그 자리에서 바꿔 가며(k *= 2, --k) 쓴다. 여기서는 바뀐 값마다 이름을 붙였다.
///
/// # 예제
/// 이징 값 보기와 트윈에 걸기
/// ```rust
/// use vanilla::{easing_function, Tween, TweenValues};
///
/// assert_eq!(easing_function::linear(0.25), 0.25);
/// assert_eq!(easing_function::quadratic::r#in(0.5), 0.25);
///
/// let tween = Tween::new(TweenValues::new().with("x", 0.0))
/// 	.to(TweenValues::new().with("x", 100.0), 0.5)
/// 	.easing(easing_function::back::out);
/// assert!(!tween.is_finished());
/// ```
//==============================================================================
pub mod easing_function {
	use crate::base::math::PI;

	//==============================================================================
	/// 선형.
	///
	/// # 인자
	/// - `progress`: 진행률. (0 ~ 1)
	///
	/// # 돌려줌
	/// 보간 비율. (progress 그대로)
	//==============================================================================
	pub fn linear(progress: f32) -> f32 {
		progress
	}

	//==============================================================================
	/// 2차 곡선.
	///
	/// # 예제
	/// 가운데와 사분 자리 값
	/// ```rust
	/// use vanilla::easing_function::quadratic;
	///
	/// assert_eq!(quadratic::r#in(0.5), 0.25);
	/// assert_eq!(quadratic::out(0.5), 0.75);
	/// assert_eq!(quadratic::in_out(0.25), 0.125);
	/// ```
	//==============================================================================
	pub mod quadratic {
		//==============================================================================
		/// 천천히 시작한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress
		}

		//==============================================================================
		/// 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			progress * (2.0 - progress)
		}

		//==============================================================================
		/// 천천히 시작하고 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			let doubled_progress = progress * 2.0;
			if doubled_progress < 1.0 {
				return 0.5 * doubled_progress * doubled_progress;
			}
			let shifted_progress = doubled_progress - 1.0;
			-0.5 * (shifted_progress * (shifted_progress - 2.0) - 1.0)
		}
	}

	//==============================================================================
	/// 3차 곡선.
	///
	/// # 예제
	/// 가운데와 사분 자리 값
	/// ```rust
	/// use vanilla::easing_function::cubic;
	///
	/// assert_eq!(cubic::r#in(0.5), 0.125);
	/// assert_eq!(cubic::out(0.5), 0.875);
	/// assert_eq!(cubic::in_out(0.75), 0.9375);
	/// ```
	//==============================================================================
	pub mod cubic {
		//==============================================================================
		/// 천천히 시작한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * progress
		}

		//==============================================================================
		/// 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shifted_progress = progress - 1.0;
			shifted_progress * shifted_progress * shifted_progress + 1.0
		}

		//==============================================================================
		/// 천천히 시작하고 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			let doubled_progress = progress * 2.0;
			if doubled_progress < 1.0 {
				return 0.5 * doubled_progress * doubled_progress * doubled_progress;
			}
			let shifted_progress = doubled_progress - 2.0;
			0.5 * (shifted_progress * shifted_progress * shifted_progress + 2.0)
		}
	}

	//==============================================================================
	/// 4차 곡선.
	///
	/// # 예제
	/// 가운데와 사분 자리 값
	/// ```rust
	/// use vanilla::easing_function::quartic;
	///
	/// assert_eq!(quartic::r#in(0.5), 0.0625);
	/// assert_eq!(quartic::out(0.5), 0.9375);
	/// assert_eq!(quartic::in_out(0.25), 0.03125);
	/// ```
	//==============================================================================
	pub mod quartic {
		//==============================================================================
		/// 천천히 시작한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * progress * progress
		}

		//==============================================================================
		/// 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shifted_progress = progress - 1.0;
			1.0 - (shifted_progress * shifted_progress * shifted_progress * shifted_progress)
		}

		//==============================================================================
		/// 천천히 시작하고 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			let doubled_progress = progress * 2.0;
			if doubled_progress < 1.0 {
				return 0.5 * doubled_progress * doubled_progress * doubled_progress * doubled_progress;
			}
			let shifted_progress = doubled_progress - 2.0;
			-0.5 * (shifted_progress * shifted_progress * shifted_progress * shifted_progress - 2.0)
		}
	}

	//==============================================================================
	/// 5차 곡선.
	///
	/// # 예제
	/// 가운데와 사분 자리 값
	/// ```rust
	/// use vanilla::easing_function::quintic;
	///
	/// assert_eq!(quintic::r#in(0.5), 0.03125);
	/// assert_eq!(quintic::out(0.5), 0.96875);
	/// assert_eq!(quintic::in_out(0.75), 0.984375);
	/// ```
	//==============================================================================
	pub mod quintic {
		//==============================================================================
		/// 천천히 시작한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * progress * progress * progress
		}

		//==============================================================================
		/// 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shifted_progress = progress - 1.0;
			shifted_progress * shifted_progress * shifted_progress * shifted_progress * shifted_progress + 1.0
		}

		//==============================================================================
		/// 천천히 시작하고 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			let doubled_progress = progress * 2.0;
			if doubled_progress < 1.0 {
				return 0.5 * doubled_progress * doubled_progress * doubled_progress * doubled_progress * doubled_progress;
			}
			let shifted_progress = doubled_progress - 2.0;
			0.5 * (shifted_progress * shifted_progress * shifted_progress * shifted_progress * shifted_progress + 2.0)
		}
	}

	//==============================================================================
	/// 사인 곡선.
	///
	/// # 예제
	/// 끝과 가운데 값
	/// ```rust
	/// use vanilla::easing_function::sinusoidal;
	///
	/// assert_eq!(sinusoidal::r#in(0.0), 0.0);
	/// assert!((sinusoidal::out(1.0) - 1.0).abs() < 1e-6);
	/// assert!((sinusoidal::in_out(0.5) - 0.5).abs() < 1e-6);
	/// ```
	//==============================================================================
	pub mod sinusoidal {
		use super::PI;

		//==============================================================================
		/// 천천히 시작한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			1.0 - (progress * PI / 2.0).cos()
		}

		//==============================================================================
		/// 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			(progress * PI / 2.0).sin()
		}

		//==============================================================================
		/// 천천히 시작하고 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			0.5 * (1.0 - (PI * progress).cos())
		}
	}

	//==============================================================================
	/// 지수 곡선.
	///
	/// # 예제
	/// 끝과 가운데 값
	/// ```rust
	/// use vanilla::easing_function::exponential;
	///
	/// assert_eq!(exponential::r#in(0.0), 0.0);
	/// assert_eq!(exponential::out(1.0), 1.0);
	/// assert!((exponential::r#in(0.5) - 0.03125).abs() < 1e-6);
	/// assert!((exponential::out(0.5) - 0.96875).abs() < 1e-6);
	/// ```
	//==============================================================================
	pub mod exponential {
		//==============================================================================
		/// 천천히 시작한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			1024.0f32.powf(progress - 1.0)
		}

		//==============================================================================
		/// 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			if progress == 1.0 {
				return 1.0;
			}
			1.0 - 2.0f32.powf(-10.0 * progress)
		}

		//==============================================================================
		/// 천천히 시작하고 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let doubled_progress = progress * 2.0;
			if doubled_progress < 1.0 {
				return 0.5 * 1024.0f32.powf(doubled_progress - 1.0);
			}
			0.5 * (-2.0f32.powf(-10.0 * (doubled_progress - 1.0)) + 2.0)
		}
	}

	//==============================================================================
	/// 원 곡선.
	///
	/// # 예제
	/// 원 위의 값
	/// ```rust
	/// use vanilla::easing_function::circular;
	///
	/// assert!((circular::r#in(0.6) - 0.2).abs() < 1e-6);
	/// assert!((circular::out(0.4) - 0.8).abs() < 1e-6);
	/// assert_eq!(circular::in_out(0.5), 0.5);
	/// ```
	//==============================================================================
	pub mod circular {
		//==============================================================================
		/// 천천히 시작한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			1.0 - (1.0 - progress * progress).sqrt()
		}

		//==============================================================================
		/// 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shifted_progress = progress - 1.0;
			(1.0 - (shifted_progress * shifted_progress)).sqrt()
		}

		//==============================================================================
		/// 천천히 시작하고 천천히 끝난다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			let doubled_progress = progress * 2.0;
			if doubled_progress < 1.0 {
				return -0.5 * ((1.0 - doubled_progress * doubled_progress).sqrt() - 1.0);
			}
			let shifted_progress = doubled_progress - 2.0;
			0.5 * ((1.0 - shifted_progress * shifted_progress).sqrt() + 1.0)
		}
	}

	//==============================================================================
	/// 튕기는 고무줄.
	/// - 원본은 a = 0.1, p = 0.4 로 시작하지만 a < 1 이면 a = 1, s = p / 4 로 바꾸므로 늘 아래 값이다.
	///
	/// # 예제
	/// 끝을 넘고 처음 아래로 내려가는 값
	/// ```rust
	/// use vanilla::easing_function::elastic;
	///
	/// assert_eq!(elastic::out(0.0), 0.0);
	/// assert_eq!(elastic::out(1.0), 1.0);
	/// assert!((elastic::out(0.2) - 1.25).abs() < 1e-5);
	/// assert!((elastic::r#in(0.8) + 0.25).abs() < 1e-5);
	/// ```
	//==============================================================================
	pub mod elastic {
		use super::PI;

		const ELASTIC_AMPLITUDE: f32 = 1.0;
		const ELASTIC_PERIOD: f32 = 0.4;
		const ELASTIC_SHIFT: f32 = ELASTIC_PERIOD / 4.0;

		//==============================================================================
		/// 시작에서 떨다가 나아간다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 에서 0, 1 에서 1, 사이에서는 0 ~ 1 밖으로 넘치며 떤다)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let shifted_progress = progress - 1.0;
			let wave = ((shifted_progress - ELASTIC_SHIFT) * (2.0 * PI) / ELASTIC_PERIOD).sin();
			-(ELASTIC_AMPLITUDE * 2.0f32.powf(10.0 * shifted_progress) * wave)
		}

		//==============================================================================
		/// 끝을 넘었다가 떨며 멈춘다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 에서 0, 1 에서 1, 사이에서는 0 ~ 1 밖으로 넘치며 떤다)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let wave = ((progress - ELASTIC_SHIFT) * (2.0 * PI) / ELASTIC_PERIOD).sin();
			ELASTIC_AMPLITUDE * 2.0f32.powf(-10.0 * progress) * wave + 1.0
		}

		//==============================================================================
		/// 시작과 끝에서 떤다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 에서 0, 1 에서 1, 사이에서는 0 ~ 1 밖으로 넘치며 떤다)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let doubled_progress = progress * 2.0;
			let shifted_progress = doubled_progress - 1.0;
			let wave = ((shifted_progress - ELASTIC_SHIFT) * (2.0 * PI) / ELASTIC_PERIOD).sin();
			if doubled_progress < 1.0 {
				return -0.5 * (ELASTIC_AMPLITUDE * 2.0f32.powf(10.0 * shifted_progress) * wave);
			}
			ELASTIC_AMPLITUDE * 2.0f32.powf(-10.0 * shifted_progress) * wave * 0.5 + 1.0
		}
	}

	//==============================================================================
	/// 뒤로 살짝 물러났다가 간다.
	///
	/// # 예제
	/// 0 아래와 1 위로 넘치는 값
	/// ```rust
	/// use vanilla::easing_function::back;
	///
	/// assert!(back::r#in(0.5) < 0.0);
	/// assert!(back::out(0.5) > 1.0);
	/// assert!((back::r#in(1.0) - 1.0).abs() < 1e-6);
	/// assert!(back::out(0.0).abs() < 1e-6);
	/// ```
	//==============================================================================
	pub mod back {
		const BACK_OVERSHOOT: f32 = 1.70158;
		const BACK_IN_OUT_OVERSHOOT: f32 = 1.70158 * 1.525;

		//==============================================================================
		/// 뒤로 물러났다가 출발한다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 에서 0, 1 에서 1, 시작 쪽에서 0 아래로 내려간다)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * ((BACK_OVERSHOOT + 1.0) * progress - BACK_OVERSHOOT)
		}

		//==============================================================================
		/// 끝을 살짝 넘었다가 돌아온다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 에서 0, 1 에서 1, 끝 쪽에서 1 을 넘는다)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shifted_progress = progress - 1.0;
			shifted_progress * shifted_progress * ((BACK_OVERSHOOT + 1.0) * shifted_progress + BACK_OVERSHOOT) + 1.0
		}

		//==============================================================================
		/// 시작에서 물러나고 끝에서 넘친다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 에서 0, 1 에서 1, 시작 쪽에서 0 아래로, 끝 쪽에서 1 위로 넘친다)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			let doubled_progress = progress * 2.0;
			if doubled_progress < 1.0 {
				return 0.5 * (doubled_progress * doubled_progress * ((BACK_IN_OUT_OVERSHOOT + 1.0) * doubled_progress - BACK_IN_OUT_OVERSHOOT));
			}
			let shifted_progress = doubled_progress - 2.0;
			0.5 * (shifted_progress * shifted_progress * ((BACK_IN_OUT_OVERSHOOT + 1.0) * shifted_progress + BACK_IN_OUT_OVERSHOOT) + 2.0)
		}
	}

	//==============================================================================
	/// 바닥에 튕긴다.
	///
	/// # 예제
	/// 튕기는 자리의 값
	/// ```rust
	/// use vanilla::easing_function::bounce;
	///
	/// assert_eq!(bounce::out(0.0), 0.0);
	/// assert!((bounce::out(0.5) - 0.765625).abs() < 1e-6);
	/// assert!((bounce::out(1.0) - 1.0).abs() < 1e-6);
	/// assert_eq!(bounce::in_out(0.5), 0.5);
	/// ```
	//==============================================================================
	pub mod bounce {
		//==============================================================================
		/// 시작에서 튕기다 나아간다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			1.0 - out(1.0 - progress)
		}

		//==============================================================================
		/// 끝에 닿아 튕기다 멈춘다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			if progress < 1.0 / 2.75 {
				7.5625 * progress * progress
			}
			else if progress < 2.0 / 2.75 {
				let shifted_progress = progress - 1.5 / 2.75;
				7.5625 * shifted_progress * shifted_progress + 0.75
			}
			else if progress < 2.5 / 2.75 {
				let shifted_progress = progress - 2.25 / 2.75;
				7.5625 * shifted_progress * shifted_progress + 0.9375
			}
			else {
				let shifted_progress = progress - 2.625 / 2.75;
				7.5625 * shifted_progress * shifted_progress + 0.984375
			}
		}

		//==============================================================================
		/// 시작과 끝에서 튕긴다.
		///
		/// # 인자
		/// - `progress`: 진행률. (0 ~ 1)
		///
		/// # 돌려줌
		/// 보간 비율. (0 ~ 1, 0 에서 0, 1 에서 1)
		//==============================================================================
		pub fn in_out(progress: f32) -> f32 {
			if progress < 0.5 {
				return r#in(progress * 2.0) * 0.5;
			}
			out(progress * 2.0 - 1.0) * 0.5 + 0.5
		}
	}
}

//==============================================================================
/// 시험. (GL 없이 돈다. NodeTree::new 와 Engine::new 는 창이나 GL 이 없어도 된다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::component::{Component, ComponentBase};
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::ComponentContext;
	use crate::core::scene::SceneBase;
	use std::cell::{Cell, RefCell};
	use std::rc::Rc;

	const EASING_SAMPLE_PROGRESSES: [f32; 7] = [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];

	/// 이징 기준표 한 줄. (이름, 함수, 진행률 일곱 곳의 값)
	type EasingReferenceRow = (&'static str, fn(f32) -> f32, [f64; 7]);

	//==============================================================================
	/// 이징 기준표.
	/// - 원본 tween.js 의 Tween.easing_function 을 node 에서 불러 뽑은 값이다. (원본을 그대로 돌려도 1e-8 안에서 같다)
	/// - 뽑은 숫자를 그대로 적어서 1/√2 같은 상수와 닮은 값이 있다. (approx_constant 를 끈다)
	//==============================================================================
	#[allow(clippy::approx_constant)]
	fn get_easing_reference_table() -> Vec<EasingReferenceRow> {
		vec![
			("linear", easing_function::linear as fn(f32) -> f32, [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0]),
			("quadratic.in", easing_function::quadratic::r#in as fn(f32) -> f32, [0.000000000, 0.010000000, 0.062500000, 0.250000000, 0.562500000, 0.810000000, 1.000000000]),
			("quadratic.out", easing_function::quadratic::out as fn(f32) -> f32, [0.000000000, 0.190000000, 0.437500000, 0.750000000, 0.937500000, 0.990000000, 1.000000000]),
			("quadratic.inOut", easing_function::quadratic::in_out as fn(f32) -> f32, [0.000000000, 0.020000000, 0.125000000, 0.500000000, 0.875000000, 0.980000000, 1.000000000]),
			("cubic.in", easing_function::cubic::r#in as fn(f32) -> f32, [0.000000000, 0.001000000, 0.015625000, 0.125000000, 0.421875000, 0.729000000, 1.000000000]),
			("cubic.out", easing_function::cubic::out as fn(f32) -> f32, [0.000000000, 0.271000000, 0.578125000, 0.875000000, 0.984375000, 0.999000000, 1.000000000]),
			("cubic.inOut", easing_function::cubic::in_out as fn(f32) -> f32, [0.000000000, 0.004000000, 0.062500000, 0.500000000, 0.937500000, 0.996000000, 1.000000000]),
			("quartic.in", easing_function::quartic::r#in as fn(f32) -> f32, [0.000000000, 0.000100000, 0.003906250, 0.062500000, 0.316406250, 0.656100000, 1.000000000]),
			("quartic.out", easing_function::quartic::out as fn(f32) -> f32, [0.000000000, 0.343900000, 0.683593750, 0.937500000, 0.996093750, 0.999900000, 1.000000000]),
			("quartic.inOut", easing_function::quartic::in_out as fn(f32) -> f32, [0.000000000, 0.000800000, 0.031250000, 0.500000000, 0.968750000, 0.999200000, 1.000000000]),
			("quintic.in", easing_function::quintic::r#in as fn(f32) -> f32, [0.000000000, 0.000010000, 0.000976563, 0.031250000, 0.237304688, 0.590490000, 1.000000000]),
			("quintic.out", easing_function::quintic::out as fn(f32) -> f32, [0.000000000, 0.409510000, 0.762695313, 0.968750000, 0.999023438, 0.999990000, 1.000000000]),
			("quintic.inOut", easing_function::quintic::in_out as fn(f32) -> f32, [0.000000000, 0.000160000, 0.015625000, 0.500000000, 0.984375000, 0.999840000, 1.000000000]),
			("sinusoidal.in", easing_function::sinusoidal::r#in as fn(f32) -> f32, [0.000000000, 0.012311659, 0.076120467, 0.292893219, 0.617316568, 0.843565535, 1.000000000]),
			("sinusoidal.out", easing_function::sinusoidal::out as fn(f32) -> f32, [0.000000000, 0.156434465, 0.382683432, 0.707106781, 0.923879533, 0.987688341, 1.000000000]),
			("sinusoidal.inOut", easing_function::sinusoidal::in_out as fn(f32) -> f32, [0.000000000, 0.024471742, 0.146446609, 0.500000000, 0.853553391, 0.975528258, 1.000000000]),
			("exponential.in", easing_function::exponential::r#in as fn(f32) -> f32, [0.000000000, 0.001953125, 0.005524272, 0.031250000, 0.176776695, 0.500000000, 1.000000000]),
			("exponential.out", easing_function::exponential::out as fn(f32) -> f32, [0.000000000, 0.500000000, 0.823223305, 0.968750000, 0.994475728, 0.998046875, 1.000000000]),
			("exponential.inOut", easing_function::exponential::in_out as fn(f32) -> f32, [0.000000000, 0.001953125, 0.015625000, 0.500000000, 0.984375000, 0.998046875, 1.000000000]),
			("circular.in", easing_function::circular::r#in as fn(f32) -> f32, [0.000000000, 0.005012563, 0.031754163, 0.133974596, 0.338562172, 0.564110106, 1.000000000]),
			("circular.out", easing_function::circular::out as fn(f32) -> f32, [0.000000000, 0.435889894, 0.661437828, 0.866025404, 0.968245837, 0.994987437, 1.000000000]),
			("circular.inOut", easing_function::circular::in_out as fn(f32) -> f32, [0.000000000, 0.010102051, 0.066987298, 0.500000000, 0.933012702, 0.989897949, 1.000000000]),
			("elastic.in", easing_function::elastic::r#in as fn(f32) -> f32, [0.000000000, 0.000000000, 0.003906250, 0.000000000, -0.125000000, 0.000000000, 1.000000000]),
			("elastic.out", easing_function::elastic::out as fn(f32) -> f32, [0.000000000, 1.000000000, 1.125000000, 1.000000000, 0.996093750, 1.000000000, 1.000000000]),
			("elastic.inOut", easing_function::elastic::in_out as fn(f32) -> f32, [0.000000000, 0.001953125, 0.000000000, 0.500000000, 1.000000000, 0.998046875, 1.000000000]),
			("back.in", easing_function::back::r#in as fn(f32) -> f32, [0.000000000, -0.014314220, -0.064136563, -0.087697500, 0.182590312, 0.591172020, 1.000000000]),
			("back.out", easing_function::back::out as fn(f32) -> f32, [0.000000000, 0.408827980, 0.817409688, 1.087697500, 1.064136563, 1.014314220, 1.000000000]),
			("back.inOut", easing_function::back::in_out as fn(f32) -> f32, [0.000000000, -0.037518552, -0.099681844, 0.500000000, 1.099681844, 1.037518552, 1.000000000]),
			("bounce.in", easing_function::bounce::r#in as fn(f32) -> f32, [0.000000000, 0.011875000, 0.027343750, 0.234375000, 0.527343750, 0.924375000, 1.000000000]),
			("bounce.out", easing_function::bounce::out as fn(f32) -> f32, [0.000000000, 0.075625000, 0.472656250, 0.765625000, 0.972656250, 0.988125000, 1.000000000]),
			("bounce.inOut", easing_function::bounce::in_out as fn(f32) -> f32, [0.000000000, 0.030000000, 0.117187500, 0.500000000, 0.882812500, 0.970000000, 1.000000000]),
		]
	}

	//==============================================================================
	/// 두 값이 거의 같은지 본다.
	//==============================================================================
	fn assert_near(actual_value: f32, expected_value: f32) {
		assert!((actual_value - expected_value).abs() < 1e-4, "{} != {}", actual_value, expected_value);
	}

	//==============================================================================
	/// 두 벡터가 거의 같은지 본다.
	//==============================================================================
	fn assert_near_vector(actual_vector: Vector2, expected_vector: Vector2) {
		assert_near(actual_vector.x, expected_vector.x);
		assert_near(actual_vector.y, expected_vector.y);
	}

	//==============================================================================
	/// 불린 횟수를 세는 칸.
	//==============================================================================
	fn create_counter() -> Rc<Cell<u32>> {
		Rc::new(Cell::new(0))
	}

	//==============================================================================
	/// 칸의 숫자를 하나 올린다.
	//==============================================================================
	fn increase_counter(counter: &Rc<Cell<u32>>) {
		let previous_count = counter.get();
		counter.set(previous_count + 1);
	}

	//==============================================================================
	/// 이징 값이 원본 공식과 같다. (표의 진행률 일곱 곳)
	//==============================================================================
	#[test]
	fn easing_matches_original_formulas() {
		for (easing_name, easing, expected_values) in get_easing_reference_table() {
			for (sample_index, progress) in EASING_SAMPLE_PROGRESSES.iter().enumerate() {
				let actual_value = easing(*progress) as f64;
				let expected_value = expected_values[sample_index];
				assert!((actual_value - expected_value).abs() < 2e-5, "{} ({}): {} != {}", easing_name, progress, actual_value, expected_value);
			}
		}
	}

	//==============================================================================
	/// 모든 이징은 0 에서 0, 1 에서 1 이다.
	//==============================================================================
	#[test]
	fn easing_endpoints_are_zero_and_one() {
		for (easing_name, easing, _) in get_easing_reference_table() {
			let start_value = easing(0.0);
			let end_value = easing(1.0);
			assert!(start_value.abs() < 1e-6, "{} (0): {}", easing_name, start_value);
			assert!((end_value - 1.0).abs() < 1e-6, "{} (1): {}", easing_name, end_value);
		}
	}

	//==============================================================================
	/// start 전에는 돌지 않고, 대기 시간이 지나면 진행률대로 보간하고, 끝나면 완료 콜백을 한 번 부른다.
	//==============================================================================
	#[test]
	fn tween_waits_delay_then_interpolates_and_completes_once() {
		let mut node_tree = NodeTree::new();
		let update_log: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
		let update_log_in_callback = update_log.clone();
		let complete_counter = create_counter();
		let complete_counter_in_callback = complete_counter.clone();
		let mut tween = Tween::new(TweenValues::new().with("value", 10.0))
			.to(TweenValues::new().with("value", 20.0), 1.0)
			.delay(0.5)
			.set_update(move |values, _node_tree| {
				let value = values.get_number("value").expect("value");
				update_log_in_callback.borrow_mut().push(value);
			})
			.set_complete(move |_node_tree| {
				increase_counter(&complete_counter_in_callback);
			});

		// start 전.
		tween.tick(&mut node_tree, 0.25);
		assert!(update_log.borrow().is_empty());
		assert!(!tween.is_finished());

		// 대기 중.
		tween.start();
		tween.tick(&mut node_tree, 0.25);
		assert!(update_log.borrow().is_empty());

		// 대기가 끝난 순간은 진행률 0.
		tween.tick(&mut node_tree, 0.25);
		tween.tick(&mut node_tree, 0.5);
		assert_eq!(complete_counter.get(), 0);
		tween.tick(&mut node_tree, 0.75);
		assert_eq!(*update_log.borrow(), vec![10.0, 15.0, 20.0]);
		assert!(tween.is_finished());
		assert_eq!(complete_counter.get(), 1);

		// 끝난 뒤에는 아무것도 하지 않는다.
		tween.tick(&mut node_tree, 1.0);
		assert_eq!(update_log.borrow().len(), 3);
		assert_eq!(complete_counter.get(), 1);
	}

	//==============================================================================
	/// 이징을 걸면 진행률 대신 이징 값으로 보간한다.
	//==============================================================================
	#[test]
	fn tween_applies_easing() {
		let mut node_tree = NodeTree::new();
		let last_value = Rc::new(Cell::new(0.0f32));
		let last_value_in_callback = last_value.clone();
		let mut tween = Tween::new(TweenValues::new().with("value", 0.0))
			.to(TweenValues::new().with("value", 100.0), 2.0)
			.easing(easing_function::quadratic::r#in)
			.set_update(move |values, _node_tree| {
				let value = values.get_number("value").expect("value");
				last_value_in_callback.set(value);
			});
		tween.start();
		tween.tick(&mut node_tree, 1.0);
		let eased_value = last_value.get();
		assert_near(eased_value, 25.0);

		// 아무 함수나 이징으로 쓸 수 있다.
		let mut step_tween = Tween::new(TweenValues::new().with("value", 0.0))
			.to(TweenValues::new().with("value", 100.0), 2.0)
			.easing(|_progress| 0.0);
		step_tween.start();
		step_tween.tick(&mut node_tree, 1.0);
		assert_eq!(step_tween.current_values.get_number("value"), Some(0.0));
	}

	//==============================================================================
	/// 시작값에 없는 이름과 종류가 다른 이름은 건너뛰고, 목표값 순서로 담는다. Vector2 도 보간한다.
	//==============================================================================
	#[test]
	fn tween_skips_missing_and_mismatched_values() {
		let mut node_tree = NodeTree::new();
		let initial_values = TweenValues::new().with("first", 1.0).with("vector", Vector2::zero()).with("mismatched", 5.0);
		let target_values = TweenValues::new().with("vector", Vector2::create(2.0, 4.0)).with("missing", 1.0).with("mismatched", Vector2::one()).with("first", 3.0);
		let mut tween = Tween::new(initial_values).to(target_values, 1.0);
		tween.start();
		tween.tick(&mut node_tree, 0.5);

		let names: Vec<&str> = tween.current_values.entries.iter().map(|entry| entry.name.as_ref()).collect();
		assert_eq!(names, vec!["vector", "first"]);
		let interpolated_vector = tween.current_values.get_vector2("vector").expect("vector");
		assert_near_vector(interpolated_vector, Vector2::create(1.0, 2.0));
		let interpolated_number = tween.current_values.get_number("first").expect("first");
		assert_near(interpolated_number, 2.0);
		assert_eq!(tween.current_values.get_number("vector"), None);
	}

	//==============================================================================
	/// 재생 중에 start 해도 처음부터 다시 하지 않는다. 끝난 뒤 start 하면 처음부터 다시 간다.
	//==============================================================================
	#[test]
	fn tween_start_while_playing_does_not_restart() {
		let mut node_tree = NodeTree::new();
		let mut tween = Tween::new(TweenValues::new().with("value", 0.0)).to(TweenValues::new().with("value", 1.0), 1.0);
		tween.start();
		tween.tick(&mut node_tree, 0.5);
		tween.start();
		tween.tick(&mut node_tree, 0.5);
		assert!(tween.is_finished());

		tween.start();
		assert!(!tween.is_finished());
		tween.tick(&mut node_tree, 0.25);
		let restarted_value = tween.current_values.get_number("value").expect("value");
		assert_near(restarted_value, 0.25);

		// stop 은 완료 처리만 하고 완료 콜백은 부르지 않는다.
		tween.stop();
		assert!(tween.is_finished());
	}

	//==============================================================================
	/// 길이가 0 이면 첫 tick 에 끝난다. (원본도 같다)
	//==============================================================================
	#[test]
	fn zero_duration_finishes_on_first_tick() {
		let mut node_tree = NodeTree::new();
		let mut tween = Tween::new(TweenValues::new().with("value", 0.0)).to(TweenValues::new().with("value", 8.0), 0.0);
		tween.start();
		tween.tick(&mut node_tree, 0.0);
		assert!(tween.is_finished());
		assert_eq!(tween.current_values.get_number("value"), Some(8.0));
	}

	//==============================================================================
	/// 노드 트윈은 tick 마다 NodeTree 를 거쳐 노드 속성에 쓴다.
	//==============================================================================
	#[test]
	fn node_tween_writes_through_node_tree() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "mover");
		let mut tween = Tween::create_node_tween(node_id, TweenProperty::LocalPosition, Vector2::zero(), Vector2::create(100.0, 50.0), 1.0);
		assert_eq!(tween.get_target_node_id(), Some(node_id));
		tween.start();
		tween.tick(&mut node_tree, 0.5);
		let halfway_position = node_tree[node_id].get_local_position();
		assert_near_vector(halfway_position, Vector2::create(50.0, 25.0));
		tween.tick(&mut node_tree, 0.5);
		let final_position = node_tree[node_id].get_local_position();
		assert_near_vector(final_position, Vector2::create(100.0, 50.0));
		assert!(tween.is_finished());
	}

	//==============================================================================
	/// Vector2 속성에 숫자를 주면 두 축에 같이 쓰고, 숫자 속성에 Vector2 를 주면 쓰지 않는다.
	//==============================================================================
	#[test]
	fn node_tween_applies_each_property() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "target");
		let initial_values = TweenValues::new()
			.with(TweenProperty::LocalScale.get_name(), 0.0)
			.with(TweenProperty::LocalRotation.get_name(), 0.0)
			.with(TweenProperty::LocalOpacity.get_name(), 1.0)
			.with(TweenProperty::ContentSize.get_name(), Vector2::create(10.0, 20.0))
			.with("custom", 0.0);
		let target_values = TweenValues::new()
			.with("localScale", 2.0)
			.with("localRotation", 90.0)
			.with("localOpacity", 0.0)
			.with("contentSize", Vector2::create(30.0, 40.0))
			.with("custom", 4.0);
		let custom_value = Rc::new(Cell::new(0.0f32));
		let custom_value_in_callback = custom_value.clone();
		let mut tween = Tween::new(initial_values)
			.to(target_values, 1.0)
			.set_target(node_id)
			.set_update(move |values, _node_tree| {
				let custom = values.get_number("custom").expect("custom");
				custom_value_in_callback.set(custom);
			});
		tween.start();
		tween.tick(&mut node_tree, 0.5);
		let node = &node_tree[node_id];
		let local_scale = node.get_local_scale();
		let local_rotation = node.get_local_rotation();
		let local_opacity = node.get_local_opacity();
		let content_size = node.get_content_size();
		let custom = custom_value.get();
		assert_near_vector(local_scale, Vector2::create(1.0, 1.0));
		assert_near(local_rotation, 45.0);
		assert_near(local_opacity, 0.5);
		assert_near_vector(content_size, Vector2::create(20.0, 30.0));
		assert_near(custom, 2.0);

		let mut rotation_node = WorldNode::new("rotation");
		TweenProperty::LocalRotation.apply_to_node(&mut rotation_node, TweenValue::Vector2(Vector2::one()));
		assert_eq!(rotation_node.get_local_rotation(), 0.0);
		assert_eq!(TweenProperty::find_by_name("localOpacity"), Some(TweenProperty::LocalOpacity));
		assert_eq!(TweenProperty::find_by_name("custom"), None);
	}

	//==============================================================================
	/// 대상 노드가 중간에 지워지면 콜백 없이 조용히 끝난다.
	//==============================================================================
	#[test]
	fn node_tween_finishes_quietly_when_node_destroyed() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "doomed");
		let update_counter = create_counter();
		let update_counter_in_callback = update_counter.clone();
		let complete_counter = create_counter();
		let complete_counter_in_callback = complete_counter.clone();
		let mut tween = Tween::create_node_tween(node_id, TweenProperty::LocalPosition, Vector2::zero(), Vector2::create(10.0, 0.0), 1.0)
			.set_update(move |_values, _node_tree| {
				increase_counter(&update_counter_in_callback);
			})
			.set_complete(move |_node_tree| {
				increase_counter(&complete_counter_in_callback);
			});
		tween.start();
		tween.tick(&mut node_tree, 0.25);
		assert_eq!(update_counter.get(), 1);

		node_tree.destroy_node(node_id);
		tween.tick(&mut node_tree, 0.25);
		assert!(tween.is_finished());
		tween.tick(&mut node_tree, 1.0);
		assert_eq!(update_counter.get(), 1);
		assert_eq!(complete_counter.get(), 0);
	}

	//==============================================================================
	/// 대상 노드가 처음 돈 때의 트리에서 떨어지면(부모에서 떼면) 원본처럼 콜백 없이 끝난다. 맨 위 노드는 부모를 따라 올라간 끝이다.
	//==============================================================================
	#[test]
	fn node_tween_finishes_quietly_when_node_detached() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let parent_id = node_tree.create_child(root_id, "parent");
		let node_id = node_tree.create_child(parent_id, "detached");
		assert_eq!(Tween::find_root_node(&node_tree, node_id), root_id);
		let complete_counter = create_counter();
		let complete_counter_in_callback = complete_counter.clone();
		let mut tween = Tween::create_node_tween(node_id, TweenProperty::LocalPosition, Vector2::zero(), Vector2::create(10.0, 0.0), 1.0)
			.set_complete(move |_node_tree| {
				increase_counter(&complete_counter_in_callback);
			});
		tween.start();
		tween.tick(&mut node_tree, 0.25);
		assert!(!tween.is_finished());

		node_tree.remove_child(root_id, parent_id);
		assert_eq!(Tween::find_root_node(&node_tree, node_id), parent_id);
		tween.tick(&mut node_tree, 0.25);
		assert!(tween.is_finished());
		assert_eq!(complete_counter.get(), 0);
		assert_near(node_tree[node_id].get_local_position().x, 2.5);
	}

	//==============================================================================
	/// 콜백은 노드 트리를 받아 다른 노드를 바꾸고 지울 수 있다.
	//==============================================================================
	#[test]
	fn tween_callbacks_can_edit_node_tree() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let fading_id = node_tree.create_child(root_id, "fading");
		let removed_id = node_tree.create_child(root_id, "removed");
		let mut tween = Tween::new(TweenValues::new().with("alpha", 1.0))
			.to(TweenValues::new().with("alpha", 0.0), 1.0)
			.set_update(move |values, node_tree| {
				let alpha = values.get_number("alpha").expect("alpha");
				if let Some(fading_node) = node_tree.get_node_mut(fading_id) {
					fading_node.set_local_opacity(alpha);
				}
			})
			.set_complete(move |node_tree| {
				node_tree.destroy_node(removed_id);
			});
		tween.start();
		tween.tick(&mut node_tree, 0.5);
		let halfway_opacity = node_tree[fading_id].get_local_opacity();
		assert_near(halfway_opacity, 0.5);
		assert!(node_tree.contains(removed_id));
		tween.tick(&mut node_tree, 0.5);
		let final_opacity = node_tree[fading_id].get_local_opacity();
		assert_near(final_opacity, 0.0);
		assert!(!node_tree.contains(removed_id));
	}

	//==============================================================================
	/// 시험용 컴포넌트. (tick 마다 자기 노드 위치를 한 자리에 박는다)
	//==============================================================================
	struct PositionPinner {
		component_base: ComponentBase,
		pinned_position: Vector2,
	}

	impl Component for PositionPinner {
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
			"PositionPinner"
		}

		//==============================================================================
		/// 주기적 갱신. (위치를 박는다)
		//==============================================================================
		fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
			let pinned_position = self.pinned_position;
			context.get_transform_mut().set_local_position(pinned_position);
		}
	}

	//==============================================================================
	/// 씬은 노드 tick 다음에 트윈을 돌리고(트윈이 이긴다), 끝난 트윈을 목록에서 뺀다.
	//==============================================================================
	#[test]
	fn scene_base_ticks_tweens_after_nodes_and_removes_finished() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut scene_base = SceneBase::new();
		let root_id = scene_base.get_root_id();
		let node_id = scene_base.get_node_tree_mut().create_child(root_id, "pinned");
		scene_base.get_node_tree_mut()[node_id].add_component(PositionPinner {
			component_base: ComponentBase::new(),
			pinned_position: Vector2::create(999.0, 999.0),
		});
		let tween = Tween::create_node_tween(node_id, TweenProperty::LocalPosition, Vector2::zero(), Vector2::create(10.0, 0.0), 1.0);
		let tween_id = scene_base.start_tween(tween);
		assert_eq!(scene_base.get_tween_count(), 1);
		assert!(scene_base.get_tween(tween_id).is_some());

		scene_base.tick(&engine, 0.5);
		let halfway_position = scene_base.get_node_tree()[node_id].get_local_position();
		assert_near_vector(halfway_position, Vector2::create(5.0, 0.0));
		assert_eq!(scene_base.get_tween_count(), 1);

		scene_base.tick(&engine, 0.5);
		let final_position = scene_base.get_node_tree()[node_id].get_local_position();
		assert_near_vector(final_position, Vector2::create(10.0, 0.0));
		assert_eq!(scene_base.get_tween_count(), 0);
		assert!(scene_base.get_tween(tween_id).is_none());

		// 트윈이 빠진 뒤에는 컴포넌트 값이 남는다.
		scene_base.tick(&engine, 0.5);
		let pinned_position = scene_base.get_node_tree()[node_id].get_local_position();
		assert_near_vector(pinned_position, Vector2::create(999.0, 999.0));
	}

	//==============================================================================
	/// stop_tween 은 완료 콜백 없이 멈추고 빼며, 없는 핸들은 무시한다. stop_all_tweens 는 모두 뺀다.
	//==============================================================================
	#[test]
	fn scene_base_stop_tween_skips_complete() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut scene_base = SceneBase::new();
		let complete_counter = create_counter();
		let complete_counter_in_callback = complete_counter.clone();
		let tween = Tween::new(TweenValues::new().with("value", 0.0))
			.to(TweenValues::new().with("value", 1.0), 1.0)
			.set_complete(move |_node_tree| {
				increase_counter(&complete_counter_in_callback);
			});
		let tween_id = scene_base.start_tween(tween);
		scene_base.tick(&engine, 0.25);
		scene_base.stop_tween(tween_id);
		assert_eq!(scene_base.get_tween_count(), 0);
		scene_base.stop_tween(tween_id);
		scene_base.tick(&engine, 2.0);
		assert_eq!(complete_counter.get(), 0);

		for _ in 0..3 {
			let tween = Tween::new(TweenValues::new().with("value", 0.0)).to(TweenValues::new().with("value", 1.0), 1.0);
			scene_base.start_tween(tween);
		}
		assert_eq!(scene_base.get_tween_count(), 3);
		scene_base.stop_all_tweens();
		assert_eq!(scene_base.get_tween_count(), 0);
	}

	//==============================================================================
	/// 같은 속성을 둘이 움직이면 먼저 시작한 트윈이 이긴다. (원본처럼 뒤에서부터 돈다)
	//==============================================================================
	#[test]
	fn scene_base_earlier_tween_wins_on_same_property() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut scene_base = SceneBase::new();
		let root_id = scene_base.get_root_id();
		let node_id = scene_base.get_node_tree_mut().create_child(root_id, "contested");
		let first_tween = Tween::create_node_tween(node_id, TweenProperty::LocalRotation, 100.0, 100.0, 1.0);
		let second_tween = Tween::create_node_tween(node_id, TweenProperty::LocalRotation, 200.0, 200.0, 1.0);
		scene_base.start_tween(first_tween);
		scene_base.start_tween(second_tween);
		scene_base.tick(&engine, 0.5);
		let contested_rotation = scene_base.get_node_tree()[node_id].get_local_rotation();
		assert_near(contested_rotation, 100.0);
	}

	//==============================================================================
	/// 씬의 트윈은 노드가 지워지면 다음 tick 에 조용히 빠진다.
	//==============================================================================
	#[test]
	fn scene_base_drops_tween_of_destroyed_node() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut scene_base = SceneBase::new();
		let root_id = scene_base.get_root_id();
		let node_id = scene_base.get_node_tree_mut().create_child(root_id, "doomed");
		let tween = Tween::create_node_tween(node_id, TweenProperty::LocalOpacity, 1.0, 0.0, 1.0);
		let tween_id = scene_base.start_tween(tween);
		scene_base.tick(&engine, 0.25);
		scene_base.get_node_tree_mut().destroy_node(node_id);
		scene_base.tick(&engine, 0.25);
		assert_eq!(scene_base.get_tween_count(), 0);
		assert!(scene_base.get_tween(tween_id).is_none());
	}

	//==============================================================================
	/// 값 묶음: 같은 이름은 바꾸고, 없는 이름이나 다른 종류는 None.
	//==============================================================================
	#[test]
	fn tween_values_set_replaces_same_name() {
		let mut values = TweenValues::new().with("x", 1.0);
		values.set("x", 2.0);
		values.set(String::from("y"), Vector2::create(3.0, 4.0));
		assert_eq!(values.get_count(), 2);
		assert_eq!(values.get_number("x"), Some(2.0));
		assert_eq!(values.get_vector2("y"), Some(Vector2::create(3.0, 4.0)));
		assert_eq!(values.get_number("y"), None);
		assert_eq!(values.get("z"), None);
	}
}
