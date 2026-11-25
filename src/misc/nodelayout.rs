//==============================================================================
//! 노드 레이아웃.
//! - 원본 misc/nodelayout.js 를 옮긴 것이다. 지역 변수 없이 노드 계층을 선언하듯 짜는 플루언트 빌더다.
//! - Vector2 없이 숫자쌍(x, y)으로 위치와 크기를 정한다.
//! - 사용:
//!   ```text
//!     let panel_id = NodeLayout::create()
//!         .pivot(Pivot::MIDDLE_CENTER)
//!         .content_size(680.0, 900.0)
//!         .component_with(Paint::new(), |paint| { paint.set_color(Color::create(0.1, 0.1, 0.1, 1.0)); })
//!         .children([
//!             NodeLayout::create()
//!                 .content_size(120.0, 70.0)
//!                 .component_with(Text::new(), |text| { text.set_text("확인"); }),
//!         ])
//!         .build(node_tree, Some(parent_id));
//!   ```
//! - 원본과 다른 점:
//!   러스트 판은 노드가 트리(NodeTree) 안에만 있어서, 빌더는 설정을 차례대로 모아 두었다가 build(node_tree, parent_id) 때 노드를 만들고 그 순서대로 건다.
//!   그래서 컴포넌트도 build 때 node_tree.add_component 로 붙는다. (attach_to_tree 가 불려 자식 노드를 만드는 컴포넌트도 된다)
//!   노드 클래스가 없으므로 create(nodeClass) 대신 create() 다.
//!   component(type, callback) 은 component(값) 과 component_with(값, 콜백) 둘로 나눴다. (러스트는 생략 인자가 없다)
//!   children(...layouts) 의 가변 인자는 배열이나 Vec 하나로 받는다.
//!   anchored_position 은 build 때 부모 content_size 기준으로 건다. 원본은 부모가 붙기 전에 불려서 사실상 local_position 과 같았다.
//!   mask_enabled(false) 는 원본처럼 첫 마스크만이 아니라 그 노드의 Mask 를 모두 뗀다.
//==============================================================================
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::mask::Mask;
use crate::core::component::Component;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::WorldNode;

/// build 때 노드에 차례대로 걸 설정 하나.
type NodeLayoutAction = Box<dyn FnOnce(&mut NodeTree, NodeId)>;

//==============================================================================
/// 부채꼴 배치 설정. (원본 options = { center_x, center_y, radius, angle_step_radian = 0.12, base_angle_radian = -PI/2, rotate_nodes = true })
///
/// # 예제
/// 중심과 반지름만 정하고 나머지는 기본값
/// ```rust
/// use vanilla::NodeLayoutFanOptions;
///
/// let fan_options = NodeLayoutFanOptions {
/// 	center_x: 640.0,
/// 	center_y: 900.0,
/// 	radius: 400.0,
/// 	..NodeLayoutFanOptions::default()
/// };
/// assert_eq!(fan_options.angle_step_radian, 0.12);
/// assert!(fan_options.rotate_nodes);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeLayoutFanOptions {
	/// 호의 중심.
	pub center_x: f32,
	/// 호의 중심 y. (center_x 와 짝, 부모 영역 기준 로컬 좌표)
	pub center_y: f32,
	/// 호의 반지름.
	pub radius: f32,
	/// 이웃한 항목 사이 각도. (라디안)
	pub angle_step_radian: f32,
	/// 가운데 항목이 바라보는 방향. (라디안, 기본 위쪽)
	pub base_angle_radian: f32,
	/// 항목 회전도 호의 접선에 맞출지.
	pub rotate_nodes: bool,
}

impl Default for NodeLayoutFanOptions {
	//==============================================================================
	/// 기본값. (중심 0, 반지름 0, 간격 0.12 라디안, 위쪽, 회전함)
	//==============================================================================
	fn default() -> NodeLayoutFanOptions {
		NodeLayoutFanOptions {
			center_x: 0.0,
			center_y: 0.0,
			radius: 0.0,
			angle_step_radian: 0.12,
			base_angle_radian: -math::PI / 2.0,
			rotate_nodes: true,
		}
	}
}

//==============================================================================
/// 노드 레이아웃.
/// - 설정 메서드는 self 를 받아 돌려주므로 사슬로 잇고, 끝에 build 로 노드를 만든다.
///
/// # 예제
/// 패널과 그 안의 글 노드를 한 번에 짓기
/// ```rust
/// use vanilla::{Color, NodeLayout, NodeTree, Paint, Pivot, Text, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let parent_id = node_tree.get_root_id();
/// let panel_id = NodeLayout::create()
/// 	.pivot(Pivot::MIDDLE_CENTER)
/// 	.content_size(680.0, 900.0)
/// 	.component_with(Paint::new(), |paint| {
/// 		paint.set_color(Color::create(0.1, 0.1, 0.1, 1.0));
/// 	})
/// 	.children([
/// 		NodeLayout::create()
/// 			.name("label")
/// 			.content_size(120.0, 70.0)
/// 			.component_with(Text::new(), |text| {
/// 				text.set_text("OK");
/// 			}),
/// 	])
/// 	.build(&mut node_tree, Some(parent_id));
///
/// assert_eq!(node_tree[panel_id].get_content_size(), Vector2::create(680.0, 900.0));
/// let label_id = node_tree.find_child_by_name(panel_id, "label").unwrap();
/// assert_eq!(node_tree[label_id].get_component::<Text>().unwrap().get_text(), "OK");
/// ```
//==============================================================================
pub struct NodeLayout {
	actions: Vec<NodeLayoutAction>,
	child_layouts: Vec<NodeLayout>,
	parent_id: Option<NodeId>,
}

impl NodeLayout {
	//==============================================================================
	/// 생성. (빈 설정)
	///
	/// # 돌려줌
	/// 설정도 자식도 없는 새 레이아웃.
	//==============================================================================
	pub fn new() -> NodeLayout {
		NodeLayout {
			actions: Vec::new(),
			child_layouts: Vec::new(),
			parent_id: None,
		}
	}

	//==============================================================================
	/// 레이아웃 만들기. (원본 NodeLayout.create(WorldNode))
	///
	/// # 돌려줌
	/// 설정도 자식도 없는 새 레이아웃. (new 와 같다)
	//==============================================================================
	pub fn create() -> NodeLayout {
		NodeLayout::new()
	}

	//==============================================================================
	/// 부채꼴 배치. (손패 카드처럼 노드들을 호 위에 펼친다)
	/// - 가운데 항목이 base_angle_radian 방향을 보도록 좌우 대칭으로 벌린다.
	/// - rotate_nodes 면 각 노드의 회전(도)도 호의 접선에 맞춘다.
	/// - 노드의 로컬 위치를 바로 바꾼다. (빌더와 따로 쓰는 정적 함수)
	///
	/// # 인자
	/// - `node_tree`: 노드가 든 트리.
	/// - `node_ids`: 펼칠 노드 목록. (이 차례로 왼쪽에서 오른쪽, 트리에 없는 노드는 건너뛴다)
	/// - `options`: 호의 중심, 반지름, 간격, 방향, 회전 여부.
	///
	/// # 예제
	/// 카드 세 장을 위쪽 호에 펼치기
	/// ```rust
	/// use vanilla::{NodeId, NodeLayout, NodeLayoutFanOptions, NodeTree};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let card_ids: Vec<NodeId> = (0..3).map(|_| node_tree.create_child(root_id, "card")).collect();
	/// NodeLayout::arrange_fan(&mut node_tree, &card_ids, NodeLayoutFanOptions {
	/// 	center_x: 100.0,
	/// 	center_y: 500.0,
	/// 	radius: 200.0,
	/// 	..NodeLayoutFanOptions::default()
	/// });
	/// let middle_position = node_tree[card_ids[1]].get_local_position();
	/// assert!((middle_position.x - 100.0).abs() < 0.001);
	/// assert!((middle_position.y - 300.0).abs() < 0.001);
	/// assert_eq!(node_tree[card_ids[1]].get_local_rotation(), 0.0);
	/// assert!(node_tree[card_ids[2]].get_local_rotation() > 0.0);
	/// ```
	//==============================================================================
	pub fn arrange_fan(node_tree: &mut NodeTree, node_ids: &[NodeId], options: NodeLayoutFanOptions) {
		let count = node_ids.len();
		if count == 0 {
			return;
		}
		let middle_index = (count as f32 - 1.0) / 2.0;
		for (index, node_id) in node_ids.iter().enumerate() {
			let node = match node_tree.get_node_mut(*node_id) {
				Some(node) => {
					node
				}
				None => {
					continue;
				}
			};
			let angle_radian = options.base_angle_radian + (index as f32 - middle_index) * options.angle_step_radian;
			let position_x = options.center_x + angle_radian.cos() * options.radius;
			let position_y = options.center_y + angle_radian.sin() * options.radius;
			node.set_local_position(Vector2::create(position_x, position_y));
			if options.rotate_nodes {
				let rotation_degree = math::radian_to_degree(angle_radian - options.base_angle_radian);
				node.set_local_rotation(rotation_degree);
			}
		}
	}

	//==============================================================================
	/// 설정 하나를 뒤에 붙인다. (build 때 붙인 순서대로 걸린다)
	//==============================================================================
	fn push_action(mut self, action: NodeLayoutAction) -> NodeLayout {
		self.actions.push(action);
		self
	}

	//==============================================================================
	/// 노드 직접 접근. (빌더에 없는 속성을 정할 때)
	///
	/// # 인자
	/// - `callback`: build 때 만든 노드를 받아 고칠 함수. (앞뒤 설정과 붙인 순서대로 불린다)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	///
	/// # 예제
	/// 누를 수 있는 노드로 만들기
	/// ```rust
	/// use vanilla::{NodeLayout, NodeTree};
	///
	/// let mut node_tree = NodeTree::new();
	/// let button_id = NodeLayout::create()
	/// 	.apply(|node| {
	/// 		node.set_interactable(true);
	/// 	})
	/// 	.build(&mut node_tree, None);
	/// assert!(node_tree[button_id].is_interactable());
	/// ```
	//==============================================================================
	pub fn apply(self, callback: impl FnOnce(&mut WorldNode) + 'static) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			callback(&mut node_tree[node_id]);
		}))
	}

	//==============================================================================
	/// 활성화 설정.
	///
	/// # 인자
	/// - `is_active`: 노드를 켤지.
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn active(self, is_active: bool) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree[node_id].set_active(is_active);
		}))
	}

	//==============================================================================
	/// 이름 설정.
	///
	/// # 인자
	/// - `name`: 노드 이름. (빌드한 노드의 처음 이름은 빈 글)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn name(self, name: &str) -> NodeLayout {
		let name = name.to_string();
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree[node_id].set_name(&name);
		}))
	}

	//==============================================================================
	/// 컴포넌트 추가. (설정은 넘기기 전에 값에 해 둔다)
	///
	/// # 인자
	/// - `component`: build 때 node_tree.add_component 로 붙일 컴포넌트.
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn component<T: Component + 'static>(self, component: T) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree.add_component(node_id, component);
		}))
	}

	//==============================================================================
	/// 컴포넌트 추가 후 콜백으로 설정. (원본 component(type, callback))
	///
	/// # 인자
	/// - `component`: build 때 붙일 컴포넌트.
	/// - `callback`: 붙인 뒤 그 컴포넌트를 받아 설정할 함수. (붙이지 못하면 부르지 않는다)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn component_with<T: Component + 'static>(self, component: T, callback: impl FnOnce(&mut T) + 'static) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			if let Some(added_component) = node_tree.add_component(node_id, component) {
				callback(added_component);
			}
		}))
	}

	//==============================================================================
	/// 자식 레이아웃 추가. (여러 개를 한 번에, 붙인 순서대로 그려진다)
	///
	/// # 인자
	/// - `layouts`: 자식 레이아웃 배열이나 Vec. (build 때 이 노드를 부모로 재귀 빌드한다)
	///
	/// # 돌려줌
	/// 자식을 더한 레이아웃.
	//==============================================================================
	pub fn children(mut self, layouts: impl IntoIterator<Item = NodeLayout>) -> NodeLayout {
		for layout in layouts {
			self.child_layouts.push(layout);
		}
		self
	}

	//==============================================================================
	/// 로컬 위치 설정.
	///
	/// # 인자
	/// - `x`: 로컬 위치 x. (부모 영역 기준, 픽셀)
	/// - `y`: 로컬 위치 y. (픽셀)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn local_position(self, x: f32, y: f32) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree[node_id].set_local_position(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	/// 피봇 설정.
	///
	/// # 인자
	/// - `pivot`: 피봇. (0 ~ 1, Pivot::MIDDLE_CENTER 같은 상수)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn pivot(self, pivot: Vector2) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree[node_id].set_pivot(pivot);
		}))
	}

	//==============================================================================
	/// 컨텐츠 크기 설정.
	///
	/// # 인자
	/// - `x`: 너비. (픽셀)
	/// - `y`: 높이. (픽셀)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn content_size(self, x: f32, y: f32) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree[node_id].set_content_size(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	/// 로컬 크기 설정.
	///
	/// # 인자
	/// - `x`: 가로 배율. (1 이 원래 크기)
	/// - `y`: 세로 배율.
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn local_scale(self, x: f32, y: f32) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree[node_id].set_local_scale(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	/// 부모 노드 설정. (build 에 부모를 넘기지 않으면 이 부모의 자식으로 붙는다)
	///
	/// # 인자
	/// - `parent_id`: 붙일 부모 노드.
	///
	/// # 돌려줌
	/// 부모를 정한 레이아웃.
	//==============================================================================
	pub fn parent(mut self, parent_id: NodeId) -> NodeLayout {
		self.parent_id = Some(parent_id);
		self
	}

	//==============================================================================
	/// 앵커 설정.
	///
	/// # 인자
	/// - `x`: 가로 앵커. (0 ~ 1, 부모 content_size 의 비율)
	/// - `y`: 세로 앵커. (0 ~ 1)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn anchor(self, x: f32, y: f32) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			node_tree[node_id].set_anchor(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	/// 앵커 기준 위치 설정. (부모가 있으면 부모 content_size 의 앵커 점 기준, 없으면 로컬 위치 그대로)
	/// - build 때 거는 순간의 앵커와 부모 크기로 셈한다. 그러니 anchor 를 이것보다 먼저 붙인다.
	///
	/// # 인자
	/// - `x`: 앵커 점에서 가로 거리. (픽셀)
	/// - `y`: 앵커 점에서 세로 거리. (픽셀)
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	///
	/// # 예제
	/// 부모 오른쪽 아래 모서리에서 안쪽으로
	/// ```rust
	/// use vanilla::{NodeLayout, NodeTree, Pivot, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let panel_id = NodeLayout::create()
	/// 	.pivot(Pivot::TOP_LEFT)
	/// 	.content_size(680.0, 900.0)
	/// 	.children([
	/// 		NodeLayout::create().name("corner").anchor(1.0, 1.0).anchored_position(-10.0, -20.0),
	/// 	])
	/// 	.build(&mut node_tree, None);
	/// let corner_id = node_tree.find_child_by_name(panel_id, "corner").unwrap();
	/// assert_eq!(node_tree[corner_id].get_local_position(), Vector2::create(670.0, 880.0));
	/// ```
	//==============================================================================
	pub fn anchored_position(self, x: f32, y: f32) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			let anchored_position = Vector2::create(x, y);
			let parent_content_size = match node_tree.get_parent_id(node_id) {
				Some(parent_id) => {
					node_tree[parent_id].get_content_size()
				}
				None => {
					Vector2::zero()
				}
			};
			node_tree[node_id].get_transform_mut().set_anchored_position(anchored_position, parent_content_size);
		}))
	}

	//==============================================================================
	/// 마스크 켜기 / 끄기. (켜면 Mask 가 없을 때만 붙이고, 끄면 뗀다)
	///
	/// # 인자
	/// - `is_enabled`: true 면 Mask 를 붙이고, false 면 그 노드의 Mask 를 모두 뗀다.
	///
	/// # 돌려줌
	/// 설정을 하나 더 붙인 레이아웃.
	//==============================================================================
	pub fn mask_enabled(self, is_enabled: bool) -> NodeLayout {
		self.push_action(Box::new(move |node_tree, node_id| {
			if is_enabled {
				let has_mask = node_tree[node_id].has_component::<Mask>();
				if !has_mask {
					node_tree.add_component(node_id, Mask::new());
				}
			}
			else {
				node_tree[node_id].remove_component::<Mask>();
			}
		}))
	}

	//==============================================================================
	/// 빌드. (노드를 만들어 부모에 붙이고, 모은 설정을 차례대로 건 뒤, 자식 레이아웃을 재귀로 빌드한다)
	/// - parent_id 를 넘기면 그 부모에, 없으면 parent() 로 정한 부모에 붙인다. 둘 다 없으면 어디에도 붙지 않는다.
	/// - 만든 노드의 핸들을 돌려준다.
	///
	/// # 인자
	/// - `node_tree`: 노드를 만들 트리.
	/// - `parent_id`: 붙일 부모. None 이면 parent() 로 정한 부모.
	///
	/// # 돌려줌
	/// 만든 노드. (자식 레이아웃의 노드는 그 아래에 있다)
	///
	/// # 예제
	/// parent() 로 부모를 미리 정하고 None 으로 빌드
	/// ```rust
	/// use vanilla::{NodeLayout, NodeTree};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_id = NodeLayout::create().name("hud").parent(root_id).build(&mut node_tree, None);
	/// assert_eq!(node_tree.get_parent_id(node_id), Some(root_id));
	///
	/// let detached_id = NodeLayout::create().build(&mut node_tree, None);
	/// assert_eq!(node_tree.get_parent_id(detached_id), None);
	/// ```
	//==============================================================================
	pub fn build(self, node_tree: &mut NodeTree, parent_id: Option<NodeId>) -> NodeId {
		let node_id = node_tree.create_node("");
		let target_parent_id = parent_id.or(self.parent_id);
		if let Some(target_parent_id) = target_parent_id {
			node_tree.add_child(target_parent_id, node_id);
		}
		for action in self.actions {
			action(node_tree, node_id);
		}
		for child_layout in self.child_layouts {
			child_layout.build(node_tree, Some(node_id));
		}
		node_id
	}
}

impl Default for NodeLayout {
	//==============================================================================
	/// 기본값. (빈 설정)
	//==============================================================================
	fn default() -> NodeLayout {
		NodeLayout::new()
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::color::Color;
	use crate::base::pivot::Pivot;
	use crate::core::component::paint::Paint;
	use crate::core::component::text::Text;

	//==============================================================================
	/// 중첩 레이아웃이 이름, 크기, 위치, 컴포넌트, 부모 관계를 그대로 만든다.
	//==============================================================================
	#[test]
	fn build_creates_nested_nodes() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let panel_id = NodeLayout::create()
			.name("panel")
			.pivot(Pivot::TOP_LEFT)
			.content_size(680.0, 900.0)
			.local_position(10.0, 20.0)
			.component_with(Paint::new(), |paint| {
				paint.set_color(Color::create(0.1, 0.2, 0.3, 1.0));
			})
			.apply(|node| {
				node.set_interactable(true);
			})
			.children([
				NodeLayout::create().name("title").content_size(120.0, 70.0).component_with(Text::new(), |text| {
					text.set_text("확인");
				}),
				NodeLayout::create().name("corner").anchor(1.0, 1.0).anchored_position(-10.0, -20.0).active(false),
			])
			.build(&mut node_tree, Some(root_id));

		let panel = &node_tree[panel_id];
		assert_eq!(panel.get_name(), "panel");
		assert_eq!(panel.get_content_size(), Vector2::create(680.0, 900.0));
		assert_eq!(panel.get_local_position(), Vector2::create(10.0, 20.0));
		assert_eq!(panel.get_pivot(), Pivot::TOP_LEFT);
		assert!(panel.is_interactable());
		assert_eq!(panel.get_component::<Paint>().expect("paint").get_color().green, 0.2);
		assert_eq!(node_tree.get_parent_id(panel_id), Some(root_id));

		let title_id = node_tree.find_child_by_name(panel_id, "title").expect("title");
		assert_eq!(node_tree[title_id].get_component::<Text>().expect("text").get_text(), "확인");
		let corner_id = node_tree.find_child_by_name(panel_id, "corner").expect("corner");
		assert_eq!(node_tree[corner_id].get_local_position(), Vector2::create(670.0, 880.0));
		assert!(!node_tree[corner_id].is_active());
		assert_eq!(node_tree.get_child_ids(panel_id), &[title_id, corner_id]);
	}

	//==============================================================================
	/// parent() 로 정한 부모, 마스크 켜고 끄기.
	//==============================================================================
	#[test]
	fn parent_and_mask_toggle() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let masked_id = NodeLayout::create().parent(root_id).mask_enabled(true).mask_enabled(true).build(&mut node_tree, None);
		assert_eq!(node_tree.get_parent_id(masked_id), Some(root_id));
		let mask_count = node_tree[masked_id].get_all_components().len();
		assert_eq!(mask_count, 1);
		let unmasked_id = NodeLayout::create().mask_enabled(true).mask_enabled(false).build(&mut node_tree, None);
		assert!(!node_tree[unmasked_id].has_component::<Mask>());
		assert_eq!(node_tree.get_parent_id(unmasked_id), None);
	}

	//==============================================================================
	/// 부채꼴 배치. (가운데는 위쪽, 좌우 대칭, 회전은 도)
	//==============================================================================
	#[test]
	fn arrange_fan_spreads_symmetrically() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_ids: Vec<NodeId> = (0..3).map(|_| node_tree.create_child(root_id, "card")).collect();
		NodeLayout::arrange_fan(&mut node_tree, &node_ids, NodeLayoutFanOptions {
			center_x: 100.0,
			center_y: 500.0,
			radius: 200.0,
			..NodeLayoutFanOptions::default()
		});
		let middle_position = node_tree[node_ids[1]].get_local_position();
		assert!((middle_position.x - 100.0).abs() < 0.001);
		assert!((middle_position.y - 300.0).abs() < 0.001);
		assert_eq!(node_tree[node_ids[1]].get_local_rotation(), 0.0);
		let left_position = node_tree[node_ids[0]].get_local_position();
		let right_position = node_tree[node_ids[2]].get_local_position();
		assert!((left_position.x - 100.0 + (right_position.x - 100.0)).abs() < 0.001);
		assert!((left_position.y - right_position.y).abs() < 0.001);
		let right_rotation = node_tree[node_ids[2]].get_local_rotation();
		assert!((right_rotation - math::radian_to_degree(0.12)).abs() < 0.001);
	}
}
