//==============================================================================
// 노드 트리.
// - 씬의 노드를 저장소 하나(slotmap)에 담고, 부모와 자식을 핸들(NodeId)로 잇는다.
// - 게임 코드는 NodeId 를 들고 다니다가 get_node / get_node_mut (또는 tree[node_id]) 로 노드를 꺼낸다.
//   지운 노드의 NodeId 는 get_node 가 None 을 돌려준다. (원본 isDestroyed 자리)
// - 원본은 부모가 children[] 으로 자식 객체를 들고 자식이 parent 로 거꾸로 가리킨다.
//   러스트에서 그 꼴(Rc<RefCell>)은 실행 중 빌림 검사 비용과 순환 참조가 생겨서 핸들로 바꿨다.
// - tick / 터치를 돌 때는 부르는 컴포넌트 하나만 노드에서 잠깐 꺼내(그 자리에는 빈 자리표를 둔다) ComponentContext(자기 노드 + 트리 전체 + 엔진)를 넘긴다.
//   그래서 컴포넌트가 다른 노드를 찾고 바꾸고 만들 수 있고, 같은 노드의 다른 컴포넌트(형제)도 get_node_mut().get_component_mut::<T>() 로 찾는다.
// - 도는 중에 지운 노드는 그 자리에서 비활성으로 두고, 다 돈 뒤에 실제로 지운다.
// - 형제 순서(원본 Node.setSiblingIndex, getSiblingIndex, getSiblings)와 계층 판정(isActiveInHierarchy, TransformNode.isVisibleInHierarchy)은
//   부모를 봐야 해서 NodeTree 가 노드 핸들을 받아 한다. 원본의 -1 과 null 은 None 이다.
//   원본 isVisible 은 활성이고 불투명도가 0 보다 큰 것이다. 러스트 판은 그리기를 끄는 보이기 플래그(set_visible)가 따로 있어 그것까지 본다.
// - 원본 Node 의 계층 함수는 같은 이름으로 둔다. 노드 객체를 주고받는 자리는 NodeId 이고, 이름 끝에 _id(_ids)가 붙은 것은 핸들을 돌려주는 짝이다.
//   (getParent → get_parent_id, getChildren → get_child_ids, getSiblings → get_sibling_ids) 부모에서 떼는 것(remove_child)은 노드를 지우지 않는다.
// - 원본 WorldNode.getPosition, setPosition, getWorldCorners, getWorldBounds 도 노드 핸들을 받아 한다. 전역 위치는 원본처럼 부모 피벗 기준으로 셈한
//   피벗 점의 자리이고, 네 모서리는 그 전역 위치와 전역 크기, 전역 회전으로 셈한다.
//==============================================================================
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::component::mask::Mask;
use crate::core::component::{Component, ComponentBase};
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::{NodeTransform, WorldNode};
use slotmap::{new_key_type, SlotMap};
use std::ops::{Index, IndexMut};

new_key_type! {
	// 노드 핸들. (칸 번호 + 세대 번호. 지운 칸을 다시 써도 옛 핸들은 세대가 달라 걸러진다)
	pub struct NodeId;
}

//==============================================================================
// 터치 종류. (NodeTree::dispatch_touch 가 컴포넌트의 어느 함수를 부를지)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchKind {
	Press,
	Moved,
	Release,
	Cancel,
}

//==============================================================================
// 노드 트리.
//==============================================================================
pub struct NodeTree {
	nodes: SlotMap<NodeId, WorldNode>,
	root_id: NodeId,
	traversal_depth: u32,
	pending_destroy_ids: Vec<NodeId>,
	culled_node_count: u32,
	// 다시 쓰는 빈 자리표. (컴포넌트를 잠깐 꺼낼 때마다 새로 잡지 않는다. 겹쳐 꺼내는 깊이만큼 늘어난다)
	placeholder_pool: Vec<Box<dyn Component>>,
}

impl NodeTree {
	//==============================================================================
	// 생성. (루트 노드 "root" 를 함께 만든다. 루트의 피봇은 왼쪽 위)
	//==============================================================================
	pub fn new() -> NodeTree {
		let mut nodes: SlotMap<NodeId, WorldNode> = SlotMap::with_key();
		let mut root = WorldNode::new("root");
		root.set_pivot(Vector2::zero());
		let root_id = nodes.insert(root);
		NodeTree {
			nodes,
			root_id,
			traversal_depth: 0,
			pending_destroy_ids: Vec::new(),
			culled_node_count: 0,
			placeholder_pool: Vec::new(),
		}
	}

	//==============================================================================
	// 빈 자리표 하나 꺼내기. (남은 것이 없으면 새로 만든다. 꺼진 상태로 돌려준다)
	//==============================================================================
	fn take_placeholder(&mut self) -> Box<dyn Component> {
		match self.placeholder_pool.pop() {
			Some(mut placeholder) => {
				placeholder.set_enable(false);
				placeholder
			}
			None => {
				Box::new(ComponentPlaceholder::new())
			}
		}
	}

	//==============================================================================
	// 꺼냈던 컴포넌트를 빈 자리표 자리에 되돌리고, 자리표는 다시 쓰게 담아 둔다. (자리표가 없으면 컴포넌트는 버린다)
	//==============================================================================
	fn restore_component(&mut self, node_id: NodeId, component: Box<dyn Component>) {
		let node = match self.nodes.get_mut(node_id) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		let placeholder_index = node.components.iter().position(|candidate| {
			let candidate_reference: &dyn Component = candidate.as_ref();
			candidate_reference.as_any().is::<ComponentPlaceholder>()
		});
		if let Some(placeholder_index) = placeholder_index {
			let placeholder = std::mem::replace(&mut node.components[placeholder_index], component);
			self.placeholder_pool.push(placeholder);
		}
	}

	//==============================================================================
	// 꺼냈던 컴포넌트를 그 차례 번호의 빈 자리표 자리에 되돌린다. (붙는 동안 같은 노드에 또 붙여 자리표가 여럿일 때. 그 자리가 자리표가 아니면 첫 자리표 자리)
	//==============================================================================
	fn restore_component_at(&mut self, node_id: NodeId, component_index: usize, component: Box<dyn Component>) {
		let is_placeholder_slot = self.nodes.get(node_id).and_then(|node| node.components.get(component_index)).map(|candidate| {
			let candidate_reference: &dyn Component = candidate.as_ref();
			candidate_reference.as_any().is::<ComponentPlaceholder>()
		}).unwrap_or(false);
		if !is_placeholder_slot {
			self.restore_component(node_id, component);
			return;
		}
		let placeholder = std::mem::replace(&mut self.nodes[node_id].components[component_index], component);
		self.placeholder_pool.push(placeholder);
	}

	//==============================================================================
	// 루트 노드 핸들.
	//==============================================================================
	pub fn get_root_id(&self) -> NodeId {
		self.root_id
	}

	//==============================================================================
	// 노드 수. (루트와 아직 트리에 붙지 않은 노드까지)
	//==============================================================================
	pub fn get_node_count(&self) -> usize {
		self.nodes.len()
	}

	//==============================================================================
	// 노드가 살아 있는지.
	//==============================================================================
	pub fn contains(&self, node_id: NodeId) -> bool {
		self.nodes.contains_key(node_id)
	}

	//==============================================================================
	// 노드 반환.
	//==============================================================================
	pub fn get_node(&self, node_id: NodeId) -> Option<&WorldNode> {
		self.nodes.get(node_id)
	}

	//==============================================================================
	// 노드 반환. (변경용)
	//==============================================================================
	pub fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut WorldNode> {
		self.nodes.get_mut(node_id)
	}

	//==============================================================================
	// 노드 만들기. (아직 어느 부모에도 붙지 않는다. add_child 로 붙여야 그려진다)
	//==============================================================================
	pub fn create_node(&mut self, name: &str) -> NodeId {
		self.nodes.insert(WorldNode::new(name))
	}

	//==============================================================================
	// 자식 노드 만들기. (만들어서 parent_id 의 맨 뒤 자식으로 붙인다)
	//==============================================================================
	pub fn create_child(&mut self, parent_id: NodeId, name: &str) -> NodeId {
		let child_id = self.create_node(name);
		self.add_child(parent_id, child_id);
		child_id
	}

	//==============================================================================
	// 자식 붙이기. (맨 뒤. 다른 부모에 붙어 있으면 떼어서 옮긴다)
	// - 자기 자신이나 자기 조상 밑으로는 붙이지 않는다. (false)
	//==============================================================================
	pub fn add_child(&mut self, parent_id: NodeId, child_id: NodeId) -> bool {
		let child_count = self.get_child_count(parent_id);
		self.insert_child(parent_id, child_id, child_count)
	}

	//==============================================================================
	// 자식 끼워 넣기. (child_index 자리, 자식 수보다 크면 맨 뒤)
	//==============================================================================
	pub fn insert_child(&mut self, parent_id: NodeId, child_id: NodeId, child_index: usize) -> bool {
		if !self.nodes.contains_key(parent_id) || !self.nodes.contains_key(child_id) {
			return false;
		}
		if child_id == self.root_id || self.is_ancestor_or_self(child_id, parent_id) {
			return false;
		}
		self.remove_from_parent(child_id);
		let parent_node = &mut self.nodes[parent_id];
		let insert_index = child_index.min(parent_node.child_ids.len());
		parent_node.child_ids.insert(insert_index, child_id);
		self.nodes[child_id].parent_id = Some(parent_id);
		true
	}

	//==============================================================================
	// 부모에서 떼기. (노드는 지우지 않는다. 다시 add_child 할 수 있다)
	//==============================================================================
	pub fn remove_from_parent(&mut self, child_id: NodeId) {
		let parent_id = match self.nodes.get(child_id).and_then(|child_node| child_node.parent_id) {
			Some(parent_id) => {
				parent_id
			}
			None => {
				return;
			}
		};
		if let Some(parent_node) = self.nodes.get_mut(parent_id) {
			parent_node.child_ids.retain(|candidate_id| *candidate_id != child_id);
		}
		self.nodes[child_id].parent_id = None;
	}

	//==============================================================================
	// 부모 설정. (None 이면 부모에서 뗀다, 자기 자신이나 자손 밑으로는 붙이지 않는다, 원본 setParent)
	//==============================================================================
	pub fn set_parent(&mut self, child_id: NodeId, parent_id: Option<NodeId>) -> bool {
		match parent_id {
			Some(parent_id) => {
				if self.get_parent_id(child_id) == Some(parent_id) {
					return true;
				}
				self.add_child(parent_id, child_id)
			}
			None => {
				self.remove_from_parent(child_id);
				true
			}
		}
	}

	//==============================================================================
	// 자식 떼기. (그 부모의 자식일 때만, 노드는 지우지 않는다, 원본 removeChild)
	//==============================================================================
	pub fn remove_child(&mut self, parent_id: NodeId, child_id: NodeId) {
		if self.get_parent_id(child_id) == Some(parent_id) {
			self.remove_from_parent(child_id);
		}
	}

	//==============================================================================
	// 그 자리의 자식 떼기. (원본 removeChildAt)
	//==============================================================================
	pub fn remove_child_at(&mut self, parent_id: NodeId, child_index: usize) {
		if let Some(child_id) = self.get_child_ids(parent_id).get(child_index).copied() {
			self.remove_from_parent(child_id);
		}
	}

	//==============================================================================
	// 모든 자식 떼기. (바로 아래만 떼고 자식들의 계층은 그대로, 원본 removeChildren)
	//==============================================================================
	pub fn remove_children(&mut self, parent_id: NodeId) {
		let child_ids: Vec<NodeId> = self.get_child_ids(parent_id).to_vec();
		for child_id in child_ids {
			self.remove_from_parent(child_id);
		}
	}

	//==============================================================================
	// 이름으로 자식 떼기. (바로 아래 처음 하나, 원본 removeChildByName)
	//==============================================================================
	pub fn remove_child_by_name(&mut self, parent_id: NodeId, name: &str) {
		if let Some(child_id) = self.find_child_by_name(parent_id, name) {
			self.remove_from_parent(child_id);
		}
	}

	//==============================================================================
	// 노드 지우기. (자손까지. 루트는 지우지 않는다)
	// - tick / 터치를 도는 중이면 비활성으로 두고 다 돈 뒤에 지운다.
	//==============================================================================
	pub fn destroy_node(&mut self, node_id: NodeId) {
		if node_id == self.root_id || !self.nodes.contains_key(node_id) {
			return;
		}
		if self.traversal_depth > 0 {
			self.nodes[node_id].set_active(false);
			self.pending_destroy_ids.push(node_id);
			return;
		}
		self.remove_from_parent(node_id);
		let mut destroy_stack: Vec<NodeId> = vec![node_id];
		while let Some(destroy_id) = destroy_stack.pop() {
			if let Some(destroyed_node) = self.nodes.remove(destroy_id) {
				destroy_stack.extend(destroyed_node.child_ids.iter().copied());
			}
		}
	}

	//==============================================================================
	// 자식 모두 지우기.
	//==============================================================================
	pub fn destroy_children(&mut self, parent_id: NodeId) {
		let child_ids: Vec<NodeId> = self.get_child_ids(parent_id).to_vec();
		for child_id in child_ids {
			self.destroy_node(child_id);
		}
	}

	//==============================================================================
	// 조건에 맞는 자식 지우기. (지운 수)
	//==============================================================================
	pub fn destroy_children_where(&mut self, parent_id: NodeId, predicate: impl Fn(&WorldNode) -> bool) -> usize {
		let child_ids: Vec<NodeId> = self.get_child_ids(parent_id).to_vec();
		let mut destroyed_count = 0;
		for child_id in child_ids {
			let is_matched = match self.nodes.get(child_id) {
				Some(child_node) => {
					predicate(child_node)
				}
				None => {
					false
				}
			};
			if is_matched {
				self.destroy_node(child_id);
				destroyed_count += 1;
			}
		}
		destroyed_count
	}

	//==============================================================================
	// 미뤄 둔 지우기 처리. (tick / 터치를 다 돈 뒤)
	//==============================================================================
	fn flush_pending_destroy(&mut self) {
		if self.traversal_depth > 0 || self.pending_destroy_ids.is_empty() {
			return;
		}
		let pending_destroy_ids = std::mem::take(&mut self.pending_destroy_ids);
		for pending_destroy_id in pending_destroy_ids {
			self.destroy_node(pending_destroy_id);
		}
	}

	//==============================================================================
	// 부모 핸들.
	//==============================================================================
	pub fn get_parent_id(&self, node_id: NodeId) -> Option<NodeId> {
		self.nodes.get(node_id).and_then(|node| node.parent_id)
	}

	//==============================================================================
	// 자식 핸들 목록. (그리는 순서)
	//==============================================================================
	pub fn get_child_ids(&self, node_id: NodeId) -> &[NodeId] {
		match self.nodes.get(node_id) {
			Some(node) => {
				&node.child_ids
			}
			None => {
				&[]
			}
		}
	}

	//==============================================================================
	// 자식 수.
	//==============================================================================
	pub fn get_child_count(&self, node_id: NodeId) -> usize {
		self.get_child_ids(node_id).len()
	}

	//==============================================================================
	// 자식의 자리. (그리는 순서의 번호, parent_id 의 자식이 아니면 None)
	//==============================================================================
	pub fn get_child_index(&self, parent_id: NodeId, child_id: NodeId) -> Option<usize> {
		self.nodes.get(parent_id).and_then(|parent_node| parent_node.get_child_index(child_id))
	}

	//==============================================================================
	// 형제 목록. (자기 자신을 포함한 부모의 자식 핸들, 부모가 없으면 None)
	//==============================================================================
	pub fn get_sibling_ids(&self, node_id: NodeId) -> Option<&[NodeId]> {
		let parent_id = self.get_parent_id(node_id)?;
		Some(self.get_child_ids(parent_id))
	}

	//==============================================================================
	// 형제 사이의 자리. (부모가 없으면 None)
	//==============================================================================
	pub fn get_sibling_index(&self, node_id: NodeId) -> Option<usize> {
		let parent_id = self.get_parent_id(node_id)?;
		self.get_child_index(parent_id, node_id)
	}

	//==============================================================================
	// 형제 사이의 자리 설정. (자식 수 - 1 까지로 자르고, 그 자리로 옮긴다. 부모가 없으면 아무것도 하지 않는다)
	// - 앞일수록 먼저 그려져 뒤에 깔린다.
	//==============================================================================
	pub fn set_sibling_index(&mut self, node_id: NodeId, child_index: usize) {
		let parent_id = match self.get_parent_id(node_id) {
			Some(parent_id) => {
				parent_id
			}
			None => {
				return;
			}
		};
		let parent_node = match self.nodes.get_mut(parent_id) {
			Some(parent_node) => {
				parent_node
			}
			None => {
				return;
			}
		};
		let old_child_index = match parent_node.get_child_index(node_id) {
			Some(old_child_index) => {
				old_child_index
			}
			None => {
				return;
			}
		};
		let new_child_index = child_index.min(parent_node.child_ids.len() - 1);
		if old_child_index == new_child_index {
			return;
		}
		parent_node.child_ids.remove(old_child_index);
		parent_node.child_ids.insert(new_child_index, node_id);
	}

	//==============================================================================
	// 자기부터 루트까지 모두 활성인지. (하나라도 꺼져 있으면 false, 없는 노드도 false)
	//==============================================================================
	pub fn is_active_in_hierarchy(&self, node_id: NodeId) -> bool {
		if !self.nodes.contains_key(node_id) {
			return false;
		}
		let mut current_id = Some(node_id);
		while let Some(candidate_id) = current_id {
			let candidate_node = match self.nodes.get(candidate_id) {
				Some(candidate_node) => {
					candidate_node
				}
				None => {
					break;
				}
			};
			if !candidate_node.is_active() {
				return false;
			}
			current_id = candidate_node.parent_id;
		}
		true
	}

	//==============================================================================
	// 자기부터 루트까지 모두 보이는지. (활성, 보이기 플래그, 불투명도 > 0 가운데 하나라도 아니면 false, 없는 노드도 false)
	//==============================================================================
	pub fn is_visible_in_hierarchy(&self, node_id: NodeId) -> bool {
		if !self.nodes.contains_key(node_id) {
			return false;
		}
		let mut current_id = Some(node_id);
		while let Some(candidate_id) = current_id {
			let candidate_node = match self.nodes.get(candidate_id) {
				Some(candidate_node) => {
					candidate_node
				}
				None => {
					break;
				}
			};
			let is_candidate_visible = candidate_node.is_active() && candidate_node.is_visible() && candidate_node.get_local_opacity() > 0.0;
			if !is_candidate_visible {
				return false;
			}
			current_id = candidate_node.parent_id;
		}
		true
	}

	//==============================================================================
	// ancestor_id 가 node_id 자신이거나 조상인지.
	//==============================================================================
	pub fn is_ancestor_or_self(&self, ancestor_id: NodeId, node_id: NodeId) -> bool {
		let mut current_id = Some(node_id);
		while let Some(candidate_id) = current_id {
			if candidate_id == ancestor_id {
				return true;
			}
			current_id = self.get_parent_id(candidate_id);
		}
		false
	}

	//==============================================================================
	// 조건으로 자식 찾기. (바로 아래만, 처음 하나, 원본 findChild)
	//==============================================================================
	pub fn find_child(&self, parent_id: NodeId, predicate: impl Fn(&WorldNode) -> bool) -> Option<NodeId> {
		self.get_child_ids(parent_id).iter().copied().find(|child_id| predicate(&self.nodes[*child_id]))
	}

	//==============================================================================
	// 조건으로 자손 찾기. (깊이 우선, 그리는 순서로 처음 하나, 원본 findChildRecursive)
	//==============================================================================
	pub fn find_child_recursive(&self, parent_id: NodeId, predicate: &dyn Fn(&WorldNode) -> bool) -> Option<NodeId> {
		for child_id in self.get_child_ids(parent_id).iter().copied() {
			if predicate(&self.nodes[child_id]) {
				return Some(child_id);
			}
			if let Some(found_id) = self.find_child_recursive(child_id, predicate) {
				return Some(found_id);
			}
		}
		None
	}

	//==============================================================================
	// 이름으로 자손 찾기. (원본 findChildRecursiveByName, find_descendant_by_name 과 같다)
	//==============================================================================
	pub fn find_child_recursive_by_name(&self, parent_id: NodeId, name: &str) -> Option<NodeId> {
		self.find_descendant_by_name(parent_id, name)
	}

	//==============================================================================
	// 이름으로 자식 찾기. (바로 아래만, 처음 하나)
	//==============================================================================
	pub fn find_child_by_name(&self, parent_id: NodeId, name: &str) -> Option<NodeId> {
		self.get_child_ids(parent_id).iter().copied().find(|child_id| self.nodes[*child_id].get_name() == name)
	}

	//==============================================================================
	// 이름으로 자손 찾기. (깊이 우선, 그리는 순서로 처음 하나)
	//==============================================================================
	pub fn find_descendant_by_name(&self, parent_id: NodeId, name: &str) -> Option<NodeId> {
		for child_id in self.get_child_ids(parent_id).iter().copied() {
			if self.nodes[child_id].get_name() == name {
				return Some(child_id);
			}
			if let Some(found_id) = self.find_descendant_by_name(child_id, name) {
				return Some(found_id);
			}
		}
		None
	}

	//==============================================================================
	// 전역 위치. (피벗 점의 전역 위치, 원본 WorldNode.getPosition)
	// - 로컬 위치는 부모 내용 영역의 왼쪽 위에서 본 자리라, 부모 피벗 점(부모 내용 크기 x 부모 피벗) 기준으로 옮긴 뒤 부모의 전역 회전과 크기를 걸고
	//   부모 피벗 점의 전역 위치에 더한다. (앵커는 넣지 않는다) 그리는 변환(calculate_world_matrix)이 피벗 점을 놓는 자리와 같다.
	//==============================================================================
	pub fn get_position(&self, node_id: NodeId) -> Vector2 {
		let local_position = self.nodes[node_id].get_local_position();
		let parent_id = match self.get_parent_id(node_id) {
			Some(parent_id) => {
				parent_id
			}
			None => {
				return local_position;
			}
		};
		let parent_position = self.get_position(parent_id);
		let parent_node = &self.nodes[parent_id];
		let parent_content_size = parent_node.get_content_size();
		let parent_pivot = parent_node.get_pivot();
		let parent_rotation = self.get_rotation(parent_id);
		let parent_scale = self.get_scale(parent_id);
		let total_local_x = local_position.x - parent_content_size.x * parent_pivot.x;
		let total_local_y = local_position.y - parent_content_size.y * parent_pivot.y;
		let radian = math::degree_to_radian(parent_rotation);
		let cosine = math::cos(radian);
		let sine = math::sin(radian);
		let scaled_x = total_local_x * parent_scale.x;
		let scaled_y = total_local_y * parent_scale.y;
		let rotated_x = scaled_x * cosine - scaled_y * sine;
		let rotated_y = scaled_x * sine + scaled_y * cosine;
		Vector2::create(parent_position.x + rotated_x, parent_position.y + rotated_y)
	}

	//==============================================================================
	// 전역 위치 설정. (부모가 없으면 로컬 위치, get_position 의 거꾸로. 원본 WorldNode.setPosition)
	// - 부모 피벗 점의 전역 위치에서 본 차이를 부모의 전역 회전과 크기를 풀어 부모 피벗 점 기준으로 되돌리고, 부모 내용 영역의 왼쪽 위 기준(로컬 위치)으로 옮긴다.
	// - 부모의 전역 크기가 0 인 축은 부모 피벗 점에 놓는다.
	//==============================================================================
	pub fn set_position(&mut self, node_id: NodeId, position: Vector2) {
		let parent_id = match self.get_parent_id(node_id) {
			Some(parent_id) => {
				parent_id
			}
			None => {
				self.nodes[node_id].set_local_position(position);
				return;
			}
		};
		let parent_position = self.get_position(parent_id);
		let parent_node = &self.nodes[parent_id];
		let parent_content_size = parent_node.get_content_size();
		let parent_pivot = parent_node.get_pivot();
		let parent_rotation = self.get_rotation(parent_id);
		let parent_scale = self.get_scale(parent_id);
		let delta_x = position.x - parent_position.x;
		let delta_y = position.y - parent_position.y;
		let radian = math::degree_to_radian(-parent_rotation);
		let cosine = math::cos(radian);
		let sine = math::sin(radian);
		let rotated_x = delta_x * cosine - delta_y * sine;
		let rotated_y = delta_x * sine + delta_y * cosine;
		let offset_x = if parent_scale.x != 0.0 { rotated_x / parent_scale.x } else { 0.0 };
		let offset_y = if parent_scale.y != 0.0 { rotated_y / parent_scale.y } else { 0.0 };
		let local_x = offset_x + parent_content_size.x * parent_pivot.x;
		let local_y = offset_y + parent_content_size.y * parent_pivot.y;
		self.nodes[node_id].set_local_position(Vector2::create(local_x, local_y));
	}

	//==============================================================================
	// 전역 크기. (부모들의 크기를 곱한다, 원본 TransformNode.getScale)
	//==============================================================================
	pub fn get_scale(&self, node_id: NodeId) -> Vector2 {
		let local_scale = self.nodes[node_id].get_local_scale();
		match self.get_parent_id(node_id) {
			Some(parent_id) => {
				let parent_scale = self.get_scale(parent_id);
				Vector2::create(parent_scale.x * local_scale.x, parent_scale.y * local_scale.y)
			}
			None => {
				local_scale
			}
		}
	}

	//==============================================================================
	// 전역 크기 설정. (부모의 전역 크기로 나눈다, 0 이면 0, 원본 TransformNode.setScale)
	//==============================================================================
	pub fn set_scale(&mut self, node_id: NodeId, scale: Vector2) {
		let local_scale = match self.get_parent_id(node_id) {
			Some(parent_id) => {
				let parent_scale = self.get_scale(parent_id);
				let local_x = if parent_scale.x != 0.0 { scale.x / parent_scale.x } else { 0.0 };
				let local_y = if parent_scale.y != 0.0 { scale.y / parent_scale.y } else { 0.0 };
				Vector2::create(local_x, local_y)
			}
			None => {
				scale
			}
		};
		self.nodes[node_id].set_local_scale(local_scale);
	}

	//==============================================================================
	// 전역 회전. (도, 부모들의 회전을 더한다, 원본 TransformNode.getRotation)
	//==============================================================================
	pub fn get_rotation(&self, node_id: NodeId) -> f32 {
		let local_rotation = self.nodes[node_id].get_local_rotation();
		match self.get_parent_id(node_id) {
			Some(parent_id) => {
				self.get_rotation(parent_id) + local_rotation
			}
			None => {
				local_rotation
			}
		}
	}

	//==============================================================================
	// 전역 회전 설정. (도, 원본 TransformNode.setRotation)
	//==============================================================================
	pub fn set_rotation(&mut self, node_id: NodeId, rotation: f32) {
		let local_rotation = match self.get_parent_id(node_id) {
			Some(parent_id) => {
				rotation - self.get_rotation(parent_id)
			}
			None => {
				rotation
			}
		};
		self.nodes[node_id].set_local_rotation(local_rotation);
	}

	//==============================================================================
	// 전역 투명도. (부모들의 투명도를 곱한다, 원본 TransformNode.getOpacity)
	//==============================================================================
	pub fn get_opacity(&self, node_id: NodeId) -> f32 {
		let mut opacity = self.nodes[node_id].get_local_opacity();
		let mut current_id = self.get_parent_id(node_id);
		while let Some(parent_id) = current_id {
			opacity *= self.nodes[parent_id].get_local_opacity();
			current_id = self.get_parent_id(parent_id);
		}
		opacity
	}

	//==============================================================================
	// 실제 화면에 그려지는 영역의 네 모서리. (원본 WorldNode.getWorldCorners, OBB. 왼쪽 위, 오른쪽 위, 오른쪽 아래, 왼쪽 아래)
	// - 크기는 전역 크기의 절댓값을 곱하고, 피벗 점의 전역 위치를 중심으로 전역 회전만큼 돌린다.
	//==============================================================================
	pub fn get_world_corners(&self, node_id: NodeId) -> [Vector2; 4] {
		let position = self.get_position(node_id);
		let scale = self.get_scale(node_id);
		let rotation = self.get_rotation(node_id);
		let node = &self.nodes[node_id];
		let content_size = node.get_content_size();
		let pivot = node.get_pivot();
		let width = content_size.x * scale.x.abs();
		let height = content_size.y * scale.y.abs();
		let left = -(width * pivot.x);
		let right = width * (1.0 - pivot.x);
		let top = -(height * pivot.y);
		let bottom = height * (1.0 - pivot.y);
		let radian = math::degree_to_radian(rotation);
		let cosine = math::cos(radian);
		let sine = math::sin(radian);
		let rotate_corner = |corner_x: f32, corner_y: f32| -> Vector2 {
			Vector2::create(corner_x * cosine - corner_y * sine + position.x, corner_x * sine + corner_y * cosine + position.y)
		};
		[rotate_corner(left, top), rotate_corner(right, top), rotate_corner(right, bottom), rotate_corner(left, bottom)]
	}

	//==============================================================================
	// 네 모서리를 감싸는 사각형. (원본 WorldNode.getWorldBounds)
	//==============================================================================
	pub fn get_world_bounds(&self, node_id: NodeId) -> Rect {
		let world_corners = self.get_world_corners(node_id);
		let mut minimum = Vector2::create(f32::INFINITY, f32::INFINITY);
		let mut maximum = Vector2::create(f32::NEG_INFINITY, f32::NEG_INFINITY);
		for world_corner in world_corners.iter() {
			if minimum.x > world_corner.x {
				minimum.x = world_corner.x;
			}
			if minimum.y > world_corner.y {
				minimum.y = world_corner.y;
			}
			if maximum.x < world_corner.x {
				maximum.x = world_corner.x;
			}
			if maximum.y < world_corner.y {
				maximum.y = world_corner.y;
			}
		}
		Rect::create(minimum.x, minimum.y, maximum.x - minimum.x, maximum.y - minimum.y)
	}

	//==============================================================================
	// 월드 행렬. (루트부터 이 노드까지 로컬 행렬을 곱한 것. 루트의 부모 좌표계 = 뷰 좌표계)
	//==============================================================================
	pub fn calculate_world_matrix(&self, node_id: NodeId) -> TransformMatrix {
		let mut chain_ids: Vec<NodeId> = Vec::new();
		let mut current_id = Some(node_id);
		while let Some(candidate_id) = current_id {
			if !self.nodes.contains_key(candidate_id) {
				break;
			}
			chain_ids.push(candidate_id);
			current_id = self.get_parent_id(candidate_id);
		}
		let mut world_matrix = TransformMatrix::identity();
		for chain_id in chain_ids.iter().rev() {
			let local_matrix = self.nodes[*chain_id].get_transform().calculate_local_matrix();
			world_matrix = world_matrix.multiply(&local_matrix);
		}
		world_matrix
	}

	//==============================================================================
	// 노드 영역 안의 점 → 뷰 좌표.
	//==============================================================================
	pub fn convert_node_position_to_view_position(&self, node_id: NodeId, node_position: Vector2) -> Vector2 {
		let world_matrix = self.calculate_world_matrix(node_id);
		world_matrix.transform_point(node_position.x, node_position.y)
	}

	//==============================================================================
	// 뷰 좌표 → 노드 영역 안의 점. (행렬이 뒤집히지 않으면 None)
	//==============================================================================
	pub fn convert_view_position_to_node_position(&self, node_id: NodeId, view_position: Vector2) -> Option<Vector2> {
		let world_matrix = self.calculate_world_matrix(node_id);
		let inverse_matrix = world_matrix.invert()?;
		Some(inverse_matrix.transform_point(view_position.x, view_position.y))
	}

	//==============================================================================
	// 뷰 좌표 → 노드의 로컬 좌표. (내용 영역의 왼쪽 위가 원점, 부모들의 변환을 모두 거친다, 원본 WorldNode.convertViewToLocal)
	// - convert_view_position_to_node_position 과 같다. 행렬이 뒤집히지 않으면(배율 0) None.
	//==============================================================================
	pub fn convert_view_to_local(&self, node_id: NodeId, view_position: Vector2) -> Option<Vector2> {
		self.convert_view_position_to_node_position(node_id, view_position)
	}

	//==============================================================================
	// 노드의 컴포넌트를 잠깐 꺼내 하나씩 부른다. (꺼낸 동안 붙인 컴포넌트는 뒤에 이어 붙인다)
	//==============================================================================
	fn visit_components(&mut self, node_id: NodeId, engine: &Engine, visitor: &mut dyn FnMut(&mut dyn Component, &mut ComponentContext)) {
		let component_count = match self.nodes.get(node_id) {
			Some(node) => {
				node.components.len()
			}
			None => {
				return;
			}
		};
		self.traversal_depth += 1;
		for component_index in 0..component_count {
			// 부르는 컴포넌트만 꺼내고 그 자리에 빈 자리표를 둔다. (형제 컴포넌트는 노드에 그대로 남는다)
			let is_present = match self.nodes.get(node_id) {
				Some(node) => {
					component_index < node.components.len()
				}
				None => {
					false
				}
			};
			if !is_present {
				break;
			}
			let placeholder = self.take_placeholder();
			let mut component = std::mem::replace(&mut self.nodes[node_id].components[component_index], placeholder);
			if component.is_enable() {
				let mut context = ComponentContext {
					node_tree: self,
					node_id,
					engine,
				};
				visitor(component.as_mut(), &mut context);
			}

			// 빈 자리표가 있는 곳에 되돌린다. (도는 중에 다른 컴포넌트를 지웠으면 자리가 당겨졌을 수 있다)
			self.restore_component(node_id, component);
		}
		self.traversal_depth -= 1;
	}

	//==============================================================================
	// 주기적 갱신. (루트부터 깊이 우선: 컴포넌트 → 자식 순, 원본 ComponentNode.tick)
	//==============================================================================
	pub fn tick(&mut self, engine: &Engine, time_delta: f32) {
		let root_id = self.root_id;
		self.traversal_depth += 1;
		self.tick_node(root_id, engine, time_delta);
		self.traversal_depth -= 1;
		self.flush_pending_destroy();
	}

	//==============================================================================
	// 컴포넌트 붙이기. (attach 다음 attach_to_tree 를 불러, 컴포넌트가 자식 노드를 만들 수 있게 한다)
	// - UIView 처럼 자식 노드를 만드는 컴포넌트는 이것으로 붙인다. 붙인 컴포넌트를 돌려준다.
	// - 원본 addComponent 처럼 자기 자리를 먼저 잡고 붙는다. 붙는 동안 함께 붙인 컴포넌트(UIScrollView 의 Mask 같은 것)는 그 뒤에 온다.
	//==============================================================================
	pub fn add_component<T: Component + 'static>(&mut self, node_id: NodeId, component: T) -> Option<&mut T> {
		if !self.nodes.contains_key(node_id) {
			return None;
		}
		let component_index = self.nodes[node_id].components.len();
		let placeholder = self.take_placeholder();
		self.nodes[node_id].components.push(placeholder);
		let mut boxed_component: Box<dyn Component> = Box::new(component);
		boxed_component.attach(self.nodes[node_id].get_transform_mut());
		boxed_component.attach_to_tree(self, node_id);
		self.restore_component_at(node_id, component_index, boxed_component);
		let added_component = self.nodes.get_mut(node_id)?.components.get_mut(component_index)?;
		let component_reference: &mut dyn Component = added_component.as_mut();
		component_reference.as_any_mut().downcast_mut::<T>()
	}

	//==============================================================================
	// 컴포넌트 하나를 트리와 함께 쥐고 부르기. (씬 코드에서 위젯처럼 다른 노드도 만지는 컴포넌트를 바로 바꿀 때)
	// - node_id 노드의 첫 번째 T 컴포넌트만 잠깐 빼고, 그 컴포넌트와 트리를 action 에 넘긴다. 없으면 None.
	// - 예: node_tree.with_component_mut::<UIScrollView, _>(scroll_id, |scroll_view, tree| scroll_view.set_scroll_offset_in_tree(tree, scroll_id, offset))
	//==============================================================================
	pub fn with_component_mut<T: Component + 'static, R>(&mut self, node_id: NodeId, action: impl FnOnce(&mut T, &mut NodeTree) -> R) -> Option<R> {
		let component_index = self.nodes.get(node_id)?.components.iter().position(|candidate| {
			let candidate_reference: &dyn Component = candidate.as_ref();
			candidate_reference.as_any().is::<T>()
		})?;
		let placeholder = self.take_placeholder();
		let mut component = std::mem::replace(&mut self.nodes[node_id].components[component_index], placeholder);
		self.traversal_depth += 1;
		let result = {
			let component_reference: &mut dyn Component = component.as_mut();
			let typed_component = component_reference.as_any_mut().downcast_mut::<T>().expect("component type");
			action(typed_component, self)
		};
		self.traversal_depth -= 1;
		self.restore_component(node_id, component);
		self.flush_pending_destroy();
		Some(result)
	}

	//==============================================================================
	// 차례 번호로 컴포넌트 하나를 트리와 함께 쥐고 부르기. (종류를 모르는 자리, UIDocument 가 트리를 받는 반사 세터를 부를 때)
	// - node_id 노드의 component_index 번째 컴포넌트를 잠깐 빼고, 그 컴포넌트와 트리를 action 에 넘긴다. 없으면 None.
	//==============================================================================
	pub fn with_component_at_mut<R>(&mut self, node_id: NodeId, component_index: usize, action: impl FnOnce(&mut dyn Component, &mut NodeTree) -> R) -> Option<R> {
		if component_index >= self.nodes.get(node_id)?.components.len() {
			return None;
		}
		let placeholder = self.take_placeholder();
		let mut component = std::mem::replace(&mut self.nodes[node_id].components[component_index], placeholder);
		self.traversal_depth += 1;
		let result = action(component.as_mut(), self);
		self.traversal_depth -= 1;
		self.restore_component_at(node_id, component_index, component);
		self.flush_pending_destroy();
		Some(result)
	}

	//==============================================================================
	// 노드 하나와 그 자손 갱신.
	//==============================================================================
	fn tick_node(&mut self, node_id: NodeId, engine: &Engine, time_delta: f32) {
		let is_tick_needed = match self.nodes.get(node_id) {
			Some(node) => {
				if !node.is_active() {
					return;
				}
				node.components.iter().any(|component| component.is_enable() && component.is_tick_needed())
			}
			None => {
				return;
			}
		};
		if is_tick_needed {
			self.visit_components(node_id, engine, &mut |component, context| {
				if component.is_tick_needed() {
					component.tick(context, time_delta);
				}
			});
		}

		// 자식. (도는 중에 자식 목록이 바뀌어도 건너뛰거나 두 번 돌지 않게, 지금 자리의 자식이 그대로일 때만 다음으로 간다)
		let mut child_index = 0;
		while let Some(child_id) = self.nodes.get(node_id).and_then(|node| node.child_ids.get(child_index).copied()) {
			self.tick_node(child_id, engine, time_delta);
			let is_same_child = self.nodes.get(node_id).and_then(|node| node.child_ids.get(child_index).copied()) == Some(child_id);
			if is_same_child {
				child_index += 1;
			}
		}
	}

	//==============================================================================
	// 출력. (루트부터 재귀, 원본 Graphic.draw_node)
	//==============================================================================
	pub fn draw(&mut self, graphic: &mut Graphic) {
		self.culled_node_count = 0;
		let root_id = self.root_id;
		self.draw_node(root_id, graphic);
		graphic.add_culled_node_count(self.culled_node_count);
	}

	//==============================================================================
	// 노드 하나와 그 자손 출력.
	// - push_transform → 컴포넌트의 push_transform → 컴포넌트 → (Mask 면 클립 시작) → 자식 → (클립 끝) → 기즈모 → 컴포넌트의 pop_transform → pop_transform
	//==============================================================================
	pub fn draw_node(&mut self, node_id: NodeId, graphic: &mut Graphic) {
		let node = match self.nodes.get_mut(node_id) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		if !node.is_active() {
			return;
		}
		node.push_transform(graphic);
		node.push_component_transforms(graphic);

		if node.is_visible() {
			// 배율이 움직이는 노드의 글은 묶은 배율로 굽는다. (컴포넌트를 다 그리면 푼다, 자식은 저마다 다시 정한다)
			let last_drawn_maximum_scale = node.get_last_drawn_maximum_scale();
			let drawn_maximum_scale = graphic.update_text_bake_scale_moving(last_drawn_maximum_scale);
			node.set_last_drawn_maximum_scale(drawn_maximum_scale);

			// 컴포넌트. (화면 밖이면 건너뛴다)
			let (clip_round_size, is_culled) = node.draw_components(graphic);
			graphic.set_text_bake_scale_moving(false);
			if is_culled {
				self.culled_node_count += 1;
			}

			// 마스크가 있으면 자식 출력 전에 클리핑 시작.
			if let Some(round_size) = clip_round_size {
				let transform = self.nodes[node_id].get_transform().clone();
				Mask::begin_clip(&transform, round_size, graphic);
			}

			// 자식. (그리는 동안에는 트리가 바뀌지 않는다)
			let child_count = self.nodes[node_id].child_ids.len();
			for child_index in 0..child_count {
				let child_id = self.nodes[node_id].child_ids[child_index];
				self.draw_node(child_id, graphic);
			}

			// 마스크가 있으면 클리핑 종료.
			if clip_round_size.is_some() {
				Mask::end_clip(graphic);
			}
		}

		let node = &mut self.nodes[node_id];
		let is_force_gizmos_visible = graphic.is_force_gizmos_visible();
		if is_force_gizmos_visible || node.is_gizmo_visible() {
			node.draw_gizmos(graphic);
		}
		node.pop_component_transforms(graphic);
		node.pop_transform(graphic);
	}

	//==============================================================================
	// 그리는 순서로 돌며 점 아래 맨 위의 interactable 노드 찾기. (원본 TouchRecognizer 의 대상 고르기)
	//==============================================================================
	pub fn find_topmost_interactable_node(&self, view_position: Vector2) -> Option<NodeId> {
		let mut found_id: Option<NodeId> = None;
		let identity_matrix = TransformMatrix::identity();
		self.find_topmost_interactable_node_recursive(self.root_id, &identity_matrix, view_position, &mut found_id);
		found_id
	}

	fn find_topmost_interactable_node_recursive(&self, node_id: NodeId, parent_matrix: &TransformMatrix, view_position: Vector2, found_id: &mut Option<NodeId>) {
		let node = match self.nodes.get(node_id) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		if !node.is_active() || !node.is_visible() {
			return;
		}
		let local_matrix = node.get_transform().calculate_local_matrix();
		let world_matrix = parent_matrix.multiply(&local_matrix);
		if node.is_interactable() && node.contains_point(&world_matrix, view_position) {
			*found_id = Some(node_id);
		}

		// 잘라 그리는(Mask) 노드 밖이면 자손은 보이지 않으므로 맞지 않는다. (원본 TouchRaycaster.isMaskedOut)
		let is_clipping = node.components.iter().any(|component| component.is_enable() && component.get_clip_round_size().is_some());
		if is_clipping && !node.contains_point(&world_matrix, view_position) {
			return;
		}
		for child_id in node.child_ids.iter().copied() {
			self.find_topmost_interactable_node_recursive(child_id, &world_matrix, view_position, found_id);
		}
	}

	//==============================================================================
	// 터치 전달. (노드의 컴포넌트들에게)
	//==============================================================================
	pub fn dispatch_touch(&mut self, node_id: NodeId, engine: &Engine, touch_kind: TouchKind, view_input_position: Vector2) {
		self.traversal_depth += 1;
		self.visit_components(node_id, engine, &mut |component, context| {
			match touch_kind {
				TouchKind::Press => {
					component.touch_press(context, view_input_position);
				}
				TouchKind::Moved => {
					component.touch_move(context, view_input_position);
				}
				TouchKind::Release => {
					component.touch_release(context, view_input_position);
				}
				TouchKind::Cancel => {
					component.touch_cancel(context, view_input_position);
				}
			}
		});
		self.traversal_depth -= 1;
		self.flush_pending_destroy();
	}

	//==============================================================================
	// 그 자리에서 휠을 받을 노드 찾기. (find_topmost_interactable_node 와 같은 그리는 순서, 없으면 None)
	// - 그 자리를 품은 노드 가운데 누를 수 있는 노드와 휠을 받는 칸(Component::is_wheel_receiver)의 노드를 보고 가장 앞(나중에 그리는 것)을 고른다.
	//   스크롤 칸이 가장 앞이면 그 칸의 노드다. (칸 안의 글이나 그림처럼 누를 수 없는 것 위에서 굴려도 칸이 받는다)
	// - 누를 수 있는 노드가 가장 앞이면 그 노드다. (dispatch_wheel 이 부모 쪽으로 올라가며 묻는다, 스크롤 칸을 덮은 막이나 창이 휠을 가린다)
	//==============================================================================
	pub fn find_wheel_target_node(&self, view_position: Vector2) -> Option<NodeId> {
		let mut found_id: Option<NodeId> = None;
		let identity_matrix = TransformMatrix::identity();
		self.find_wheel_target_node_recursive(self.root_id, &identity_matrix, view_position, &mut found_id);
		found_id
	}

	fn find_wheel_target_node_recursive(&self, node_id: NodeId, parent_matrix: &TransformMatrix, view_position: Vector2, found_id: &mut Option<NodeId>) {
		let node = match self.nodes.get(node_id) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		if !node.is_active() || !node.is_visible() {
			return;
		}
		let local_matrix = node.get_transform().calculate_local_matrix();
		let world_matrix = parent_matrix.multiply(&local_matrix);
		let is_wheel_receiver = node.components.iter().any(|component| component.is_enable() && component.is_wheel_receiver());
		if (node.is_interactable() || is_wheel_receiver) && node.contains_point(&world_matrix, view_position) {
			*found_id = Some(node_id);
		}
		let is_clipping = node.components.iter().any(|component| component.is_enable() && component.get_clip_round_size().is_some());
		if is_clipping && !node.contains_point(&world_matrix, view_position) {
			return;
		}
		for child_id in node.child_ids.iter().copied() {
			self.find_wheel_target_node_recursive(child_id, &world_matrix, view_position, found_id);
		}
	}

	//==============================================================================
	// 그 자리에 휠 전달. (find_wheel_target_node 로 찾은 노드부터 부모 쪽으로, 받아 쓴 컴포넌트가 있으면 true)
	//==============================================================================
	pub fn dispatch_wheel_at(&mut self, view_position: Vector2, engine: &Engine, wheel_delta: Vector2) -> bool {
		match self.find_wheel_target_node(view_position) {
			Some(target_id) => {
				self.dispatch_wheel(target_id, engine, wheel_delta)
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 휠 전달. (node_id 자신부터 부모 쪽으로 올라가며 처음 받아 쓴 컴포넌트에서 멈춘다. 원본 touchWheel)
	//==============================================================================
	pub fn dispatch_wheel(&mut self, node_id: NodeId, engine: &Engine, wheel_delta: Vector2) -> bool {
		let mut current_id = Some(node_id);
		while let Some(candidate_id) = current_id {
			let mut is_handled = false;
			self.traversal_depth += 1;
			self.visit_components(candidate_id, engine, &mut |component, context| {
				if !is_handled && component.wheel(context, wheel_delta) {
					is_handled = true;
				}
			});
			self.traversal_depth -= 1;
			if is_handled {
				self.flush_pending_destroy();
				return true;
			}
			current_id = self.get_parent_id(candidate_id);
		}
		self.flush_pending_destroy();
		false
	}

	//==============================================================================
	// 노드 경계 사각형을 부모 영역 기준으로. (회전 없을 때 정확, 충돌 검사 등)
	//==============================================================================
	pub fn get_local_bounds(&self, node_id: NodeId) -> Option<Rect> {
		self.nodes.get(node_id).map(|node| node.get_transform().get_local_bounds())
	}
}

impl Default for NodeTree {
	fn default() -> NodeTree {
		NodeTree::new()
	}
}

impl Index<NodeId> for NodeTree {
	type Output = WorldNode;

	//==============================================================================
	// tree[node_id]. (지운 노드면 멈춘다. 있을지 모르면 get_node)
	//==============================================================================
	fn index(&self, node_id: NodeId) -> &WorldNode {
		self.nodes.get(node_id).expect("NodeTree: destroyed or unknown node id")
	}
}

impl IndexMut<NodeId> for NodeTree {
	fn index_mut(&mut self, node_id: NodeId) -> &mut WorldNode {
		self.nodes.get_mut(node_id).expect("NodeTree: destroyed or unknown node id")
	}
}

//==============================================================================
// 빈 자리표. (tick / 터치 중인 컴포넌트가 잠깐 비운 자리. 아무것도 하지 않는다)
//==============================================================================
struct ComponentPlaceholder {
	component_base: ComponentBase,
}

impl ComponentPlaceholder {
	fn new() -> ComponentPlaceholder {
		let mut component_base = ComponentBase::new();
		component_base.set_enable(false);
		ComponentPlaceholder {
			component_base,
		}
	}
}

impl Component for ComponentPlaceholder {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ComponentPlaceholder"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}
}

//==============================================================================
// 컴포넌트 문맥. (tick / 터치 때 컴포넌트가 받는 것: 자기 노드, 트리 전체, 엔진)
// - 원본 컴포넌트의 get_node(), get_node().getParent(), Engine.get_engine() 자리.
//==============================================================================
pub struct ComponentContext<'a> {
	node_tree: &'a mut NodeTree,
	node_id: NodeId,
	engine: &'a Engine,
}

impl<'a> ComponentContext<'a> {
	//==============================================================================
	// 자기 노드 핸들.
	//==============================================================================
	pub fn get_node_id(&self) -> NodeId {
		self.node_id
	}

	//==============================================================================
	// 자기 노드. (지금 불리는 컴포넌트만 빠져 있고, 같은 노드의 다른 컴포넌트는 get_component 로 찾는다)
	//==============================================================================
	pub fn get_node(&self) -> &WorldNode {
		&self.node_tree[self.node_id]
	}

	//==============================================================================
	// 자기 노드. (변경용)
	//==============================================================================
	pub fn get_node_mut(&mut self) -> &mut WorldNode {
		let node_id = self.node_id;
		&mut self.node_tree[node_id]
	}

	//==============================================================================
	// 자기 노드 트랜스폼.
	//==============================================================================
	pub fn get_transform(&self) -> &NodeTransform {
		self.node_tree[self.node_id].get_transform()
	}

	//==============================================================================
	// 자기 노드 트랜스폼. (변경용)
	//==============================================================================
	pub fn get_transform_mut(&mut self) -> &mut NodeTransform {
		let node_id = self.node_id;
		self.node_tree[node_id].get_transform_mut()
	}

	//==============================================================================
	// 트리 전체.
	//==============================================================================
	pub fn get_node_tree(&self) -> &NodeTree {
		self.node_tree
	}

	//==============================================================================
	// 트리 전체. (변경용. 다른 노드를 바꾸거나 만들고 지울 때)
	//==============================================================================
	pub fn get_node_tree_mut(&mut self) -> &mut NodeTree {
		self.node_tree
	}

	//==============================================================================
	// 엔진. (입력, 시간, 뷰)
	//==============================================================================
	pub fn get_engine(&self) -> &Engine {
		self.engine
	}

	//==============================================================================
	// 다른 노드(또는 자기 노드)의 T 컴포넌트를 그 노드의 문맥과 함께 부르기. (없거나 지금 꺼내져 있으면 None)
	// - 위젯 안의 행 버튼이 위젯 본체를 부를 때 쓴다: context.with_component_mut::<UIDropdown, _>(dropdown_id, |dropdown, dropdown_context| ...)
	//==============================================================================
	pub fn with_component_mut<T: Component + 'static, R>(&mut self, node_id: NodeId, action: impl FnOnce(&mut T, &mut ComponentContext) -> R) -> Option<R> {
		let engine = self.engine;
		self.node_tree.with_component_mut::<T, R>(node_id, |component: &mut T, node_tree: &mut NodeTree| {
			let mut context = ComponentContext {
				node_tree,
				node_id,
				engine,
			};
			action(component, &mut context)
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::vector2::Vector2;
	use crate::core::component::mask::Mask;
	use crate::core::component::{Component, ComponentBase};
	use crate::core::engine::{Engine, EngineConfiguration};

	//==============================================================================
	// tick 마다 자기 노드를 지우는 시험 컴포넌트.
	//==============================================================================
	struct SelfDestroyer {
		component_base: ComponentBase,
	}

	impl Component for SelfDestroyer {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"SelfDestroyer"
		}

		fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
			let node_id = context.get_node_id();
			context.get_node_tree_mut().destroy_node(node_id);
		}
	}

	//==============================================================================
	// tick 마다 오른쪽으로 1 옮기는 시험 컴포넌트. (처음에 형제 노드도 하나 만든다)
	//==============================================================================
	struct Mover {
		component_base: ComponentBase,
		is_sibling_created: bool,
	}

	impl Component for Mover {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"Mover"
		}

		fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
			let local_position = context.get_transform().get_local_position();
			context.get_transform_mut().set_local_position(Vector2::create(local_position.x + 1.0, local_position.y));
			if !self.is_sibling_created {
				self.is_sibling_created = true;
				let parent_id = context.get_node().get_parent_id().expect("parent");
				context.get_node_tree_mut().create_child(parent_id, "sibling");
			}
		}
	}

	//==============================================================================
	// 휠을 받아 쓰는 시험 컴포넌트.
	//==============================================================================
	struct WheelCatcher {
		component_base: ComponentBase,
		wheel_total: f32,
	}

	impl Component for WheelCatcher {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"WheelCatcher"
		}

		fn wheel(&mut self, _context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
			self.wheel_total += wheel_delta.y;
			true
		}
	}

	//==============================================================================
	// 시험용 엔진. (창 없이 만든다)
	//==============================================================================
	fn create_test_engine() -> Engine {
		Engine::new(EngineConfiguration::new())
	}

	//==============================================================================
	// 만들기, 붙이기, 찾기, 지우기, 옛 핸들.
	//==============================================================================
	#[test]
	fn create_find_destroy() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let layer_id = node_tree.create_child(root_id, "layer");
		let first_id = node_tree.create_child(layer_id, "first");
		let second_id = node_tree.create_child(layer_id, "second");
		let grandchild_id = node_tree.create_child(second_id, "grandchild");
		assert_eq!(node_tree.get_child_ids(layer_id), &[first_id, second_id]);
		assert_eq!(node_tree.get_parent_id(grandchild_id), Some(second_id));
		assert_eq!(node_tree.find_child_by_name(layer_id, "second"), Some(second_id));
		assert_eq!(node_tree.find_descendant_by_name(root_id, "grandchild"), Some(grandchild_id));
		assert_eq!(node_tree.get_node_count(), 5);

		node_tree.destroy_node(second_id);
		assert!(node_tree.get_node(second_id).is_none());
		assert!(node_tree.get_node(grandchild_id).is_none());
		assert_eq!(node_tree.get_child_ids(layer_id), &[first_id]);
		assert_eq!(node_tree.get_node_count(), 3);

		// 지운 칸을 다시 써도 옛 핸들은 걸러진다.
		let reused_id = node_tree.create_child(layer_id, "reused");
		assert!(node_tree.get_node(second_id).is_none());
		assert!(node_tree.get_node(reused_id).is_some());

		// 루트는 지우지 않는다.
		node_tree.destroy_node(root_id);
		assert!(node_tree.contains(root_id));
	}

	//==============================================================================
	// 옮기기, 끼워 넣기, 자기 자손 밑으로 붙이기 막기.
	//==============================================================================
	#[test]
	fn reparent_and_cycle_guard() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let parent_id = node_tree.create_child(root_id, "parent");
		let child_id = node_tree.create_child(parent_id, "child");
		let other_id = node_tree.create_child(root_id, "other");

		assert!(!node_tree.add_child(child_id, parent_id));
		assert!(!node_tree.add_child(parent_id, parent_id));
		assert!(!node_tree.add_child(child_id, root_id));

		assert!(node_tree.add_child(other_id, child_id));
		assert_eq!(node_tree.get_parent_id(child_id), Some(other_id));
		assert!(node_tree.get_child_ids(parent_id).is_empty());

		let first_id = node_tree.create_child(other_id, "first");
		assert!(node_tree.insert_child(other_id, first_id, 0));
		assert_eq!(node_tree.get_child_ids(other_id), &[first_id, child_id]);

		node_tree.remove_from_parent(child_id);
		assert_eq!(node_tree.get_parent_id(child_id), None);
		assert!(node_tree.contains(child_id));
	}

	//==============================================================================
	// 형제 순서: 자리 읽기, 옮기기(끝을 넘으면 맨 뒤), 부모가 없으면 None 이고 옮기지 않는다.
	//==============================================================================
	#[test]
	fn sibling_order() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let parent_id = node_tree.create_child(root_id, "parent");
		let first_id = node_tree.create_child(parent_id, "first");
		let second_id = node_tree.create_child(parent_id, "second");
		let third_id = node_tree.create_child(parent_id, "third");
		let detached_id = node_tree.create_node("detached");

		assert_eq!(node_tree.get_sibling_index(second_id), Some(1));
		assert_eq!(node_tree.get_child_index(parent_id, third_id), Some(2));
		assert_eq!(node_tree.get_child_index(parent_id, detached_id), None);
		assert_eq!(node_tree[parent_id].get_child_index(first_id), Some(0));
		assert_eq!(node_tree.get_sibling_ids(first_id), Some(&[first_id, second_id, third_id][..]));
		assert_eq!(node_tree.get_sibling_ids(detached_id), None);
		assert_eq!(node_tree.get_sibling_index(detached_id), None);
		assert_eq!(node_tree.get_sibling_index(root_id), None);

		node_tree.set_sibling_index(third_id, 0);
		assert_eq!(node_tree.get_child_ids(parent_id), &[third_id, first_id, second_id]);
		node_tree.set_sibling_index(third_id, 100);
		assert_eq!(node_tree.get_child_ids(parent_id), &[first_id, second_id, third_id]);
		node_tree.set_sibling_index(first_id, 1);
		assert_eq!(node_tree.get_child_ids(parent_id), &[second_id, first_id, third_id]);
		node_tree.set_sibling_index(first_id, 1);
		assert_eq!(node_tree.get_child_ids(parent_id), &[second_id, first_id, third_id]);
		node_tree.set_sibling_index(detached_id, 0);
		assert_eq!(node_tree.get_parent_id(detached_id), None);
	}

	//==============================================================================
	// 계층 판정: 조상 하나라도 꺼지거나(활성) 안 보이면(보이기 플래그, 불투명도 0) false.
	//==============================================================================
	#[test]
	fn active_and_visible_in_hierarchy() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let parent_id = node_tree.create_child(root_id, "parent");
		let child_id = node_tree.create_child(parent_id, "child");
		assert!(node_tree.is_active_in_hierarchy(child_id));
		assert!(node_tree.is_visible_in_hierarchy(child_id));

		node_tree[parent_id].set_active(false);
		assert!(!node_tree.is_active_in_hierarchy(child_id));
		assert!(!node_tree.is_visible_in_hierarchy(child_id));
		assert!(node_tree.is_active_in_hierarchy(root_id));
		node_tree[parent_id].set_active(true);

		node_tree[parent_id].set_local_opacity(0.0);
		assert!(node_tree.is_active_in_hierarchy(child_id));
		assert!(!node_tree.is_visible_in_hierarchy(child_id));
		node_tree[parent_id].set_local_opacity(0.5);
		assert!(node_tree.is_visible_in_hierarchy(child_id));

		node_tree[child_id].set_visible(false);
		assert!(!node_tree.is_visible_in_hierarchy(child_id));
		assert!(node_tree.is_visible_in_hierarchy(parent_id));

		node_tree.destroy_node(child_id);
		assert!(!node_tree.is_active_in_hierarchy(child_id));
		assert!(!node_tree.is_visible_in_hierarchy(child_id));
	}

	//==============================================================================
	// tick 중에 지우면 다 돈 뒤에 지워지고, 옆 형제는 그대로 돈다.
	//==============================================================================
	#[test]
	fn destroy_during_tick_is_deferred() {
		let engine = create_test_engine();
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let doomed_id = node_tree.create_child(root_id, "doomed");
		node_tree[doomed_id].add_component(SelfDestroyer {
			component_base: ComponentBase::new(),
		});
		let mover_id = node_tree.create_child(root_id, "mover");
		node_tree[mover_id].add_component(Mover {
			component_base: ComponentBase::new(),
			is_sibling_created: false,
		});

		node_tree.tick(&engine, 0.016);
		assert!(node_tree.get_node(doomed_id).is_none());
		assert_eq!(node_tree[mover_id].get_local_position().x, 1.0);
		assert!(node_tree.find_child_by_name(root_id, "sibling").is_some());
		assert!(node_tree[mover_id].has_component::<Mover>());

		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[mover_id].get_local_position().x, 2.0);
	}

	//==============================================================================
	// 맨 위 노드 찾기. (그리는 순서로 뒤가 위, 마스크 밖 자손은 맞지 않는다)
	//==============================================================================
	#[test]
	fn topmost_touch_and_mask_clip() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let panel_id = node_tree.create_child(root_id, "panel");
		let panel = &mut node_tree[panel_id];
		panel.set_pivot(Vector2::zero());
		panel.set_local_position(Vector2::create(100.0, 100.0));
		panel.set_content_size(Vector2::create(100.0, 100.0));
		panel.add_component(Mask::new());

		let inside_id = node_tree.create_child(panel_id, "inside");
		let inside = &mut node_tree[inside_id];
		inside.set_pivot(Vector2::zero());
		inside.set_content_size(Vector2::create(300.0, 50.0));
		inside.set_interactable(true);

		let over_id = node_tree.create_child(root_id, "over");
		let over = &mut node_tree[over_id];
		over.set_pivot(Vector2::zero());
		over.set_local_position(Vector2::create(150.0, 100.0));
		over.set_content_size(Vector2::create(10.0, 10.0));
		over.set_interactable(true);

		assert_eq!(node_tree.find_topmost_interactable_node(Vector2::create(120.0, 120.0)), Some(inside_id));
		assert_eq!(node_tree.find_topmost_interactable_node(Vector2::create(155.0, 105.0)), Some(over_id));
		// inside 는 폭이 300 이지만 패널(마스크) 밖인 250 에서는 맞지 않는다.
		assert_eq!(node_tree.find_topmost_interactable_node(Vector2::create(250.0, 120.0)), None);
	}

	//==============================================================================
	// 휠은 맞은 노드부터 부모 쪽으로 올라가 처음 받아 쓴 컴포넌트에서 멈춘다.
	//==============================================================================
	#[test]
	fn wheel_bubbles_to_ancestor() {
		let engine = create_test_engine();
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let scroll_id = node_tree.create_child(root_id, "scroll");
		node_tree[scroll_id].add_component(WheelCatcher {
			component_base: ComponentBase::new(),
			wheel_total: 0.0,
		});
		let item_id = node_tree.create_child(scroll_id, "item");
		let is_handled = node_tree.dispatch_wheel(item_id, &engine, Vector2::create(0.0, 40.0));
		assert!(is_handled);
		let wheel_total = node_tree[scroll_id].get_component::<WheelCatcher>().expect("catcher").wheel_total;
		assert_eq!(wheel_total, 40.0);
		let is_root_handled = node_tree.dispatch_wheel(root_id, &engine, Vector2::create(0.0, 40.0));
		assert!(!is_root_handled);
	}
}

#[cfg(test)]
mod world_corner_tests {
	use super::*;

	//==============================================================================
	// 네 모서리와 감싸는 사각형: 부모 피벗 기준 위치, 전역 크기의 절댓값, 전역 회전. (원본 WorldNode.getWorldCorners, getWorldBounds)
	//==============================================================================
	#[test]
	fn world_corners_and_bounds_match_original() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(200.0, 100.0));
		node_tree[root_id].set_pivot(Vector2::create(0.5, 0.5));
		let node_id = node_tree.create_child(root_id, "box");
		let node = &mut node_tree[node_id];
		node.set_local_position(Vector2::create(150.0, 50.0));
		node.set_content_size(Vector2::create(20.0, 10.0));
		node.set_pivot(Vector2::create(0.5, 0.5));
		node.set_local_scale(Vector2::create(-2.0, 1.0));
		let world_corners = node_tree.get_world_corners(node_id);
		assert_eq!(world_corners[0], Vector2::create(30.0, -5.0));
		assert_eq!(world_corners[2], Vector2::create(70.0, 5.0));
		assert_eq!(node_tree.get_world_bounds(node_id), Rect::create(30.0, -5.0, 40.0, 10.0));

		node_tree[node_id].set_local_rotation(90.0);
		let rotated_bounds = node_tree.get_world_bounds(node_id);
		assert!((rotated_bounds.position.x - 45.0).abs() < 1e-4);
		assert!((rotated_bounds.position.y + 20.0).abs() < 1e-4);
		assert!((rotated_bounds.size.x - 10.0).abs() < 1e-4);
		assert!((rotated_bounds.size.y - 40.0).abs() < 1e-4);
	}
}

#[cfg(test)]
mod sibling_tests {
	use super::*;
	use crate::base::color::Color;
	use crate::core::component::mask::Mask;
	use crate::core::component::paint::Paint;
	use crate::core::component::{Component, ComponentBase};
	use crate::core::engine::{Engine, EngineConfiguration};

	//==============================================================================
	// tick 에서 같은 노드의 Paint 색을 바꾸는 시험 컴포넌트.
	//==============================================================================
	struct SiblingPainter {
		component_base: ComponentBase,
	}

	impl Component for SiblingPainter {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"SiblingPainter"
		}

		fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
			let node = context.get_node_mut();
			let is_self_visible = node.has_component::<SiblingPainter>();
			if let Some(paint) = node.get_component_mut::<Paint>() {
				let red = if is_self_visible {
					0.5
				}
				else {
					1.0
				};
				paint.set_color(Color::create(red, 0.0, 0.0, 1.0));
			}
		}
	}

	//==============================================================================
	// 씬 코드에서 컴포넌트와 트리를 같이 쥐고 부를 수 있다. (다른 노드를 만들고 형제도 본다)
	//==============================================================================
	#[test]
	fn with_component_mut_gives_tree() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "painted");
		node_tree[node_id].add_component(Paint::create_with_color(Color::white()));
		node_tree[node_id].add_component(SiblingPainter {
			component_base: ComponentBase::new(),
		});
		let created_id = node_tree.with_component_mut::<SiblingPainter, _>(node_id, |_painter, tree| {
			let has_paint = tree[node_id].has_component::<Paint>();
			assert!(has_paint);
			tree.create_child(node_id, "created")
		}).expect("painter");
		assert_eq!(node_tree.get_parent_id(created_id), Some(node_id));
		assert!(node_tree[node_id].has_component::<SiblingPainter>());
		let missing = node_tree.with_component_mut::<Mask, _>(node_id, |_mask, _tree| 0);
		assert!(missing.is_none());
	}

	//==============================================================================
	// tick 중인 컴포넌트가 형제 컴포넌트를 찾아 바꿀 수 있다. (자기 자신은 그동안 빠져 있다)
	//==============================================================================
	#[test]
	fn component_reaches_sibling_during_tick() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "painted");
		node_tree[node_id].add_component(Paint::create_with_color(Color::white()));
		node_tree[node_id].add_component(SiblingPainter {
			component_base: ComponentBase::new(),
		});
		node_tree.tick(&engine, 0.016);
		let paint_color = node_tree[node_id].get_component::<Paint>().expect("paint").get_color();
		assert_eq!(paint_color.red, 1.0);
		assert_eq!(paint_color.green, 0.0);
		assert!(node_tree[node_id].has_component::<SiblingPainter>());
		assert_eq!(node_tree[node_id].get_all_components().len(), 2);
	}

	//==============================================================================
	// tick 이 컴포넌트를 꺼낼 때 쓰는 빈 자리표는 다시 써서, 몇 번을 돌아도 하나만 남는다.
	//==============================================================================
	#[test]
	fn placeholder_is_reused_across_ticks() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		for node_index in 0..3 {
			let node_id = node_tree.create_child(root_id, &format!("painted{}", node_index));
			node_tree[node_id].add_component(Paint::create_with_color(Color::white()));
			node_tree[node_id].add_component(SiblingPainter {
				component_base: ComponentBase::new(),
			});
		}
		node_tree.tick(&engine, 0.016);
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree.placeholder_pool.len(), 1);
		let is_placeholder_left = node_tree.nodes.values().any(|node| {
			node.get_all_components().iter().any(|component| {
				let component_reference: &dyn Component = component.as_ref();
				component_reference.as_any().is::<ComponentPlaceholder>()
			})
		});
		assert!(!is_placeholder_left);
	}

	//==============================================================================
	// 원본 Node 의 계층 함수: 부모 설정, 떼기(지우지 않음), 조건 찾기, 뿌리와 잎.
	//==============================================================================
	#[test]
	fn original_hierarchy_functions() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let layer_id = node_tree.create_child(root_id, "layer");
		let first_id = node_tree.create_child(layer_id, "first");
		let second_id = node_tree.create_child(layer_id, "second");
		let deep_id = node_tree.create_child(second_id, "deep");
		assert!(node_tree[root_id].is_root());
		assert!(node_tree[first_id].is_leaf());
		assert!(node_tree[second_id].has_parent());
		assert_eq!(node_tree[layer_id].get_child(1), Some(second_id));
		assert_eq!(node_tree[layer_id].get_child(2), None);
		assert!(node_tree[layer_id].has_child(first_id));
		assert_eq!(node_tree.find_child(layer_id, |node| node.get_name().starts_with("sec")), Some(second_id));
		assert_eq!(node_tree.find_child_recursive(root_id, &|node| node.get_name() == "deep"), Some(deep_id));
		assert_eq!(node_tree.find_child_recursive_by_name(root_id, "deep"), Some(deep_id));

		// 떼기는 노드를 지우지 않는다.
		node_tree.remove_child(layer_id, first_id);
		assert!(node_tree.contains(first_id));
		assert!(node_tree[first_id].is_root());
		assert!(node_tree.set_parent(first_id, Some(second_id)));
		assert_eq!(node_tree.get_parent_id(first_id), Some(second_id));
		assert!(!node_tree.set_parent(second_id, Some(deep_id)));
		node_tree.remove_child_by_name(second_id, "deep");
		assert!(node_tree[deep_id].is_root());
		node_tree.remove_child_at(second_id, 0);
		assert!(node_tree[second_id].is_leaf());
		node_tree.remove_children(root_id);
		assert!(node_tree[root_id].is_leaf());
		assert!(node_tree.contains(layer_id));
		assert!(node_tree.set_parent(layer_id, None));
	}

	//==============================================================================
	// 의존 컴포넌트는 붙기 전에 없을 때만 붙고, 떼어질 때 detach 가 불린다. get_or_add_component, get_components.
	//==============================================================================
	#[derive(Default)]
	struct RequiredMarker {
		component_base: ComponentBase,
	}

	impl Component for RequiredMarker {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"RequiredMarker"
		}

		fn detach(&mut self, transform: &mut NodeTransform) {
			transform.set_local_opacity(0.25);
		}
	}

	#[derive(Default)]
	struct NeedsMarker {
		component_base: ComponentBase,
	}

	impl Component for NeedsMarker {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"NeedsMarker"
		}

		fn require(&self) -> Vec<crate::core::component::RequiredComponent> {
			vec![crate::core::component::RequiredComponent::of::<RequiredMarker>()]
		}
	}

	#[test]
	fn required_components_detach_and_get_components() {
		let mut node = WorldNode::new("node");
		node.add_component(NeedsMarker::default());
		node.add_component(NeedsMarker::default());
		assert_eq!(node.get_components::<RequiredMarker>().len(), 1);
		assert_eq!(node.get_components::<NeedsMarker>().len(), 2);
		node.get_or_add_component::<NeedsMarker>();
		assert_eq!(node.get_components::<NeedsMarker>().len(), 2);
		assert_eq!(node.remove_component::<RequiredMarker>(), 1);
		assert_eq!(node.get_local_opacity(), 0.25);
		node.get_or_add_component::<RequiredMarker>();
		assert!(node.has_component::<RequiredMarker>());
	}

	//==============================================================================
	// 전역 위치, 크기, 회전, 투명도. (원본 TransformNode 식, 설정한 값이 다시 읽힌다)
	//==============================================================================
	#[test]
	fn global_transform_round_trip() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let parent_id = node_tree.create_child(root_id, "parent");
		let child_id = node_tree.create_child(parent_id, "child");
		node_tree[parent_id].set_local_position(Vector2::create(100.0, 50.0));
		node_tree[parent_id].set_local_rotation(90.0);
		node_tree[parent_id].set_local_scale(Vector2::create(2.0, 2.0));
		node_tree[parent_id].set_local_opacity(0.5);
		node_tree[child_id].set_local_position(Vector2::create(10.0, 0.0));
		node_tree[child_id].set_local_opacity(0.5);
		let position = node_tree.get_position(child_id);
		assert!((position.x - 100.0).abs() < 1e-3 && (position.y - 70.0).abs() < 1e-3);
		node_tree.set_position(child_id, Vector2::create(80.0, 50.0));
		let local_position = node_tree[child_id].get_local_position();
		assert!(local_position.x.abs() < 1e-3 && (local_position.y - 10.0).abs() < 1e-3);
		assert_eq!(node_tree.get_scale(child_id), Vector2::create(2.0, 2.0));
		node_tree.set_scale(child_id, Vector2::create(4.0, 1.0));
		assert_eq!(node_tree[child_id].get_local_scale(), Vector2::create(2.0, 0.5));
		node_tree.set_rotation(child_id, 30.0);
		assert_eq!(node_tree[child_id].get_local_rotation(), -60.0);
		assert_eq!(node_tree.get_rotation(child_id), 30.0);
		assert_eq!(node_tree.get_opacity(child_id), 0.25);
	}

	//==============================================================================
	// 전역 위치는 부모 피벗 점 기준으로 셈한 피벗 점의 자리다. (원본 WorldNode.getPosition, 그리는 변환이 놓는 자리와 같고 set_position 이 거꾸로 되돌린다)
	//==============================================================================
	#[test]
	fn global_position_follows_parent_pivot() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let parent_id = node_tree.create_child(root_id, "parent");
		let child_id = node_tree.create_child(parent_id, "child");
		node_tree[parent_id].set_local_position(Vector2::create(300.0, 200.0));
		node_tree[parent_id].set_content_size(Vector2::create(100.0, 60.0));
		node_tree[parent_id].set_pivot(Vector2::create(0.5, 0.5));
		node_tree[parent_id].set_local_rotation(90.0);
		node_tree[parent_id].set_local_scale(Vector2::create(2.0, 2.0));
		node_tree[child_id].set_content_size(Vector2::create(20.0, 10.0));
		node_tree[child_id].set_pivot(Vector2::create(0.5, 0.5));
		node_tree[child_id].set_local_position(Vector2::create(60.0, 30.0));

		// 부모 피벗 점(50, 30)에서 (10, 0) 떨어진 자리를 크기 2 와 90 도로 옮기면 (0, 20) 이다.
		let position = node_tree.get_position(child_id);
		assert!((position.x - 300.0).abs() < 1e-3 && (position.y - 220.0).abs() < 1e-3);
		let drawn_pivot_position = node_tree.convert_node_position_to_view_position(child_id, Vector2::create(10.0, 5.0));
		assert!((drawn_pivot_position.x - position.x).abs() < 1e-3 && (drawn_pivot_position.y - position.y).abs() < 1e-3);
		let world_corners = node_tree.get_world_corners(child_id);
		let corner_center_x = (world_corners[0].x + world_corners[2].x) * 0.5;
		let corner_center_y = (world_corners[0].y + world_corners[2].y) * 0.5;
		assert!((corner_center_x - position.x).abs() < 1e-3 && (corner_center_y - position.y).abs() < 1e-3);

		node_tree.set_position(child_id, Vector2::create(280.0, 200.0));
		let local_position = node_tree[child_id].get_local_position();
		assert!((local_position.x - 50.0).abs() < 1e-3 && (local_position.y - 40.0).abs() < 1e-3);
		let round_trip_position = node_tree.get_position(child_id);
		assert!((round_trip_position.x - 280.0).abs() < 1e-3 && (round_trip_position.y - 200.0).abs() < 1e-3);
	}
}
