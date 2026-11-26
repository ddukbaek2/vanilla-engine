//==============================================================================
//! 동적 AABB 나무. (2D, 넓은 단계와 공간 거르기)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/spatial/dynamictree.js 와 같은 API, 같은 셈이다.
//! - Box2D 의 b2DynamicTree 와 같은 꼴의 이진 경계 부피 계층(BVH)이다. 잎 하나가 프록시 하나(상자, 분류 비트, 사용자 값)다.
//!   넣을 때는 표면적 휴리스틱(SAH, 2D 는 둘레)으로 형제를 고르고, 올라가며 나무 돌리기(Kensler 2008)로 둘레 합을 줄인다.
//! - 물리의 넓은 단계뿐 아니라 노드 거르기(화면 상자로 query), 터치 맞추기(점 상자로 query), 광선 맞추기(ray_cast)에도 쓴다.
//! - 상자는 넣은 그대로 담는다. 움직임마다 다시 넣지 않으려면 쓰는 쪽이 여유를 더한 상자(뚱뚱한 상자)를 주고,
//!   새 상자가 담긴 상자를 벗어날 때만 move_proxy 를 부른다. (2D 물리가 그렇게 한다)
//! - 같은 차례로 넣고 빼면 원본과 나무 모양이 같다.
//! - 원본과 다른 점: 사용자 값은 제네릭 T 다. 프록시 번호는 usize 다. 잘못된 번호의 사용자 값은 None 이다.
//==============================================================================
// 옮긴 원문: Box2D 의 b2DynamicTree 에서 옮김, MIT, THIRD_PARTY_LICENSES.md
use super::aabb2d::AABB2D;

/// 비어 있는 노드 번호.
const NULL_NODE: usize = usize::MAX;

//==============================================================================
/// 나무의 노드 하나. (잎은 프록시 하나, 안쪽 노드는 두 자식을 감싼 상자)
//==============================================================================
#[derive(Clone, Debug)]
struct TreeNode<T> {
	min_x: f64,
	min_y: f64,
	max_x: f64,
	max_y: f64,
	/// 부모. (빈 노드면 다음 빈 노드)
	parent: usize,
	child1: usize,
	child2: usize,
	/// 잎은 0, 빈 노드는 -1.
	height: i32,
	/// 잎은 제 값, 안쪽 노드는 자식들의 합.
	category_bits: u32,
	user_data: Option<T>,
}

impl<T> TreeNode<T> {
	//==============================================================================
	/// 생성.
	//==============================================================================
	fn new() -> TreeNode<T> {
		TreeNode {
			min_x: 0.0,
			min_y: 0.0,
			max_x: 0.0,
			max_y: 0.0,
			parent: NULL_NODE,
			child1: NULL_NODE,
			child2: NULL_NODE,
			height: 0,
			category_bits: 0,
			user_data: None,
		}
	}

	//==============================================================================
	/// 둘레.
	//==============================================================================
	fn get_perimeter(&self) -> f64 {
		2.0 * ((self.max_x - self.min_x) + (self.max_y - self.min_y))
	}
}

//==============================================================================
/// 두 노드를 감싼 상자의 둘레.
//==============================================================================
fn combined_perimeter<T>(first: &TreeNode<T>, second: &TreeNode<T>) -> f64 {
	2.0 * ((first.max_x.max(second.max_x) - first.min_x.min(second.min_x)) + (first.max_y.max(second.max_y) - first.min_y.min(second.min_y)))
}

//==============================================================================
/// 동적 AABB 나무.
///
/// # 예제
/// 프록시를 넣고 상자와 광선으로 찾기
/// ```rust
/// use vanilla::experimental::spatial::{AABB2D, DynamicTree};
///
/// let mut tree: DynamicTree<String> = DynamicTree::new();
/// for index in 0..10 {
/// 	let x = index as f64 * 30.0;
/// 	tree.create_proxy(&AABB2D::create(x, 0.0, x + 20.0, 20.0), u32::MAX, format!("box{}", index));
/// }
/// let mut found_list = Vec::new();
/// tree.query(&AABB2D::create(25.0, 5.0, 95.0, 15.0), u32::MAX, |proxy_id| {
/// 	found_list.push(proxy_id);
/// 	true
/// });
/// assert_eq!(found_list.len(), 3);
///
/// let mut first_hit = None;
/// tree.ray_cast(-10.0, 10.0, 400.0, 0.0, u32::MAX, |proxy_id, maximum_fraction| {
/// 	let aabb = tree.get_aabb(proxy_id);
/// 	let fraction = (aabb.min_x + 10.0) / 400.0;
/// 	if fraction < maximum_fraction {
/// 		first_hit = tree.get_user_data(proxy_id).cloned();
/// 		return fraction;
/// 	}
/// 	maximum_fraction
/// });
/// assert_eq!(first_hit.as_deref(), Some("box0"));
/// assert!(tree.validate());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct DynamicTree<T> {
	nodes: Vec<TreeNode<T>>,
	root: usize,
	free_list: usize,
	proxy_count: usize,
}

impl<T> Default for DynamicTree<T> {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> DynamicTree<T> {
		DynamicTree::new()
	}
}

impl<T> DynamicTree<T> {
	//==============================================================================
	/// 생성. (빈 나무)
	///
	/// # 돌려줌
	/// 빈 나무.
	//==============================================================================
	pub fn new() -> DynamicTree<T> {
		DynamicTree {
			nodes: Vec::new(),
			root: NULL_NODE,
			free_list: NULL_NODE,
			proxy_count: 0,
		}
	}

	//==============================================================================
	/// 뿌리 노드 번호 반환.
	///
	/// # 돌려줌
	/// 뿌리 노드 번호. (비었으면 None)
	//==============================================================================
	pub fn get_root(&self) -> Option<usize> {
		if self.root == NULL_NODE {
			return None;
		}
		Some(self.root)
	}

	//==============================================================================
	/// 프록시 수 반환.
	///
	/// # 돌려줌
	/// 들어 있는 프록시(잎)의 수.
	//==============================================================================
	pub fn get_proxy_count(&self) -> usize {
		self.proxy_count
	}

	//==============================================================================
	/// 빈 노드 하나 얻기. (빈 목록에서 꺼내거나 새로 만든다)
	//==============================================================================
	fn allocate_node(&mut self) -> usize {
		if self.free_list == NULL_NODE {
			self.nodes.push(TreeNode::new());
			return self.nodes.len() - 1;
		}
		let index = self.free_list;
		let node = &mut self.nodes[index];
		self.free_list = node.parent;
		node.parent = NULL_NODE;
		node.child1 = NULL_NODE;
		node.child2 = NULL_NODE;
		node.height = 0;
		node.category_bits = 0;
		node.user_data = None;
		index
	}

	//==============================================================================
	/// 노드 돌려주기. (빈 목록 맨 앞에 넣는다)
	//==============================================================================
	fn free_node(&mut self, node_index: usize) {
		let free_list = self.free_list;
		let node = &mut self.nodes[node_index];
		node.parent = free_list;
		node.height = -1;
		node.user_data = None;
		self.free_list = node_index;
	}

	//==============================================================================
	/// 프록시 만들기.
	///
	/// # 인자
	/// - `aabb`: 프록시의 상자. (그대로 담는다)
	/// - `category_bits`: 분류 비트. (query, ray_cast 의 mask_bits 와 겹쳐야 찾힌다)
	/// - `user_data`: 사용자 값.
	///
	/// # 돌려줌
	/// 프록시 번호.
	//==============================================================================
	pub fn create_proxy(&mut self, aabb: &AABB2D, category_bits: u32, user_data: T) -> usize {
		let proxy_id = self.allocate_node();
		let node = &mut self.nodes[proxy_id];
		node.min_x = aabb.min_x;
		node.min_y = aabb.min_y;
		node.max_x = aabb.max_x;
		node.max_y = aabb.max_y;
		node.category_bits = category_bits;
		node.user_data = Some(user_data);
		node.height = 0;
		self.insert_leaf(proxy_id);
		self.proxy_count += 1;
		proxy_id
	}

	//==============================================================================
	/// 프록시 없애기. (잘못된 번호면 아무것도 하지 않는다)
	///
	/// # 인자
	/// - `proxy_id`: 없앨 프록시 번호.
	//==============================================================================
	pub fn destroy_proxy(&mut self, proxy_id: usize) {
		if !self.is_proxy(proxy_id) {
			return;
		}
		self.remove_leaf(proxy_id);
		self.free_node(proxy_id);
		self.proxy_count -= 1;
	}

	//==============================================================================
	/// 프록시 옮기기. (상자를 바꿔 다시 넣는다)
	///
	/// # 인자
	/// - `proxy_id`: 옮길 프록시 번호.
	/// - `aabb`: 새 상자.
	//==============================================================================
	pub fn move_proxy(&mut self, proxy_id: usize, aabb: &AABB2D) {
		if !self.is_proxy(proxy_id) {
			return;
		}
		self.remove_leaf(proxy_id);
		let node = &mut self.nodes[proxy_id];
		node.min_x = aabb.min_x;
		node.min_y = aabb.min_y;
		node.max_x = aabb.max_x;
		node.max_y = aabb.max_y;
		self.insert_leaf(proxy_id);
	}

	//==============================================================================
	/// 살아 있는 프록시 번호인지.
	///
	/// # 인자
	/// - `proxy_id`: 볼 번호.
	///
	/// # 돌려줌
	/// 잎 노드 번호면 true.
	//==============================================================================
	pub fn is_proxy(&self, proxy_id: usize) -> bool {
		proxy_id < self.nodes.len() && self.nodes[proxy_id].height == 0
	}

	//==============================================================================
	/// 프록시의 상자 반환.
	///
	/// # 인자
	/// - `proxy_id`: 프록시 번호.
	///
	/// # 돌려줌
	/// 담긴 상자. (잘못된 번호면 모두 0)
	//==============================================================================
	pub fn get_aabb(&self, proxy_id: usize) -> AABB2D {
		if !self.is_proxy(proxy_id) {
			return AABB2D::new();
		}
		let node = &self.nodes[proxy_id];
		AABB2D::create(node.min_x, node.min_y, node.max_x, node.max_y)
	}

	//==============================================================================
	/// 프록시의 사용자 값 반환.
	///
	/// # 인자
	/// - `proxy_id`: 프록시 번호.
	///
	/// # 돌려줌
	/// 사용자 값. (잘못된 번호면 None)
	//==============================================================================
	pub fn get_user_data(&self, proxy_id: usize) -> Option<&T> {
		if !self.is_proxy(proxy_id) {
			return None;
		}
		self.nodes[proxy_id].user_data.as_ref()
	}

	//==============================================================================
	/// 프록시의 분류 비트 반환.
	///
	/// # 인자
	/// - `proxy_id`: 프록시 번호.
	///
	/// # 돌려줌
	/// 분류 비트. (잘못된 번호면 0)
	//==============================================================================
	pub fn get_category_bits(&self, proxy_id: usize) -> u32 {
		if !self.is_proxy(proxy_id) {
			return 0;
		}
		self.nodes[proxy_id].category_bits
	}

	//==============================================================================
	/// 나무 높이 반환.
	///
	/// # 돌려줌
	/// 뿌리의 높이. (비었거나 잎 하나면 0)
	//==============================================================================
	pub fn get_height(&self) -> i32 {
		if self.root == NULL_NODE {
			return 0;
		}
		self.nodes[self.root].height
	}

	//==============================================================================
	/// 둘레 비 반환. (안쪽 노드 둘레의 합 / 뿌리 둘레, 작을수록 나무가 좋다)
	///
	/// # 돌려줌
	/// 둘레 비. (비었으면 0)
	//==============================================================================
	pub fn get_area_ratio(&self) -> f64 {
		if self.root == NULL_NODE {
			return 0.0;
		}
		let root_perimeter = self.nodes[self.root].get_perimeter();
		if root_perimeter <= 0.0 {
			return 0.0;
		}
		let mut total_perimeter = 0.0;
		for node in self.nodes.iter() {
			if node.height <= 0 {
				continue;
			}
			total_perimeter += node.get_perimeter();
		}
		total_perimeter / root_perimeter
	}

	//==============================================================================
	/// 잎 넣기. (표면적 휴리스틱으로 형제를 고르고 올라가며 상자와 높이를 고친다)
	//==============================================================================
	fn insert_leaf(&mut self, leaf: usize) {
		if self.root == NULL_NODE {
			self.root = leaf;
			self.nodes[leaf].parent = NULL_NODE;
			return;
		}

		// 형제 고르기. (새 부모를 지금 노드에 두는 값과, 더 내려갈 때 자식마다 드는 값을 견준다)
		let mut index = self.root;
		while self.nodes[index].height > 0 {
			let node = &self.nodes[index];
			let leaf_node = &self.nodes[leaf];
			let child1 = node.child1;
			let child2 = node.child2;
			let perimeter = node.get_perimeter();
			let combined = combined_perimeter(node, leaf_node);
			let cost = 2.0 * combined;
			let inheritance_cost = 2.0 * (combined - perimeter);
			let cost1 = self.calculate_descend_cost(child1, leaf) + inheritance_cost;
			let cost2 = self.calculate_descend_cost(child2, leaf) + inheritance_cost;
			if cost < cost1 && cost < cost2 {
				break;
			}
			if cost1 < cost2 {
				index = child1;
			}
			else {
				index = child2;
			}
		}
		let sibling = index;

		// 새 부모.
		let old_parent = self.nodes[sibling].parent;
		let new_parent = self.allocate_node();
		let (leaf_min_x, leaf_min_y, leaf_max_x, leaf_max_y, leaf_category_bits) = {
			let leaf_node = &self.nodes[leaf];
			(leaf_node.min_x, leaf_node.min_y, leaf_node.max_x, leaf_node.max_y, leaf_node.category_bits)
		};
		let (sibling_min_x, sibling_min_y, sibling_max_x, sibling_max_y, sibling_category_bits, sibling_height) = {
			let sibling_node = &self.nodes[sibling];
			(sibling_node.min_x, sibling_node.min_y, sibling_node.max_x, sibling_node.max_y, sibling_node.category_bits, sibling_node.height)
		};
		{
			let new_parent_node = &mut self.nodes[new_parent];
			new_parent_node.parent = old_parent;
			new_parent_node.user_data = None;
			new_parent_node.min_x = leaf_min_x.min(sibling_min_x);
			new_parent_node.min_y = leaf_min_y.min(sibling_min_y);
			new_parent_node.max_x = leaf_max_x.max(sibling_max_x);
			new_parent_node.max_y = leaf_max_y.max(sibling_max_y);
			new_parent_node.category_bits = leaf_category_bits | sibling_category_bits;
			new_parent_node.height = sibling_height + 1;
			new_parent_node.child1 = sibling;
			new_parent_node.child2 = leaf;
		}
		self.nodes[sibling].parent = new_parent;
		self.nodes[leaf].parent = new_parent;
		if old_parent != NULL_NODE {
			let old_parent_node = &mut self.nodes[old_parent];
			if old_parent_node.child1 == sibling {
				old_parent_node.child1 = new_parent;
			}
			else {
				old_parent_node.child2 = new_parent;
			}
		}
		else {
			self.root = new_parent;
		}

		// 올라가며 고치고 돌린다.
		let start_index = self.nodes[leaf].parent;
		self.refit_ancestors(start_index);
	}

	//==============================================================================
	/// 잎을 child 쪽으로 더 내려 보낼 때 드는 값.
	//==============================================================================
	fn calculate_descend_cost(&self, child: usize, leaf: usize) -> f64 {
		let child_node = &self.nodes[child];
		let leaf_node = &self.nodes[leaf];
		let combined = combined_perimeter(child_node, leaf_node);
		if child_node.height == 0 {
			return combined;
		}
		combined - child_node.get_perimeter()
	}

	//==============================================================================
	/// 잎 빼기. (부모를 없애고 형제를 그 자리에 올린다)
	//==============================================================================
	fn remove_leaf(&mut self, leaf: usize) {
		if leaf == self.root {
			self.root = NULL_NODE;
			return;
		}
		let parent = self.nodes[leaf].parent;
		let grand_parent = self.nodes[parent].parent;
		let mut sibling = self.nodes[parent].child1;
		if sibling == leaf {
			sibling = self.nodes[parent].child2;
		}
		if grand_parent != NULL_NODE {
			let grand_parent_node = &mut self.nodes[grand_parent];
			if grand_parent_node.child1 == parent {
				grand_parent_node.child1 = sibling;
			}
			else {
				grand_parent_node.child2 = sibling;
			}
			self.nodes[sibling].parent = grand_parent;
			self.free_node(parent);
			self.refit_ancestors(grand_parent);
		}
		else {
			self.root = sibling;
			self.nodes[sibling].parent = NULL_NODE;
			self.free_node(parent);
		}
		self.nodes[leaf].parent = NULL_NODE;
	}

	//==============================================================================
	/// 안쪽 노드의 상자, 분류 비트, 높이를 두 자식으로 다시 셈.
	//==============================================================================
	fn refit_node(&mut self, index: usize) {
		let child1 = self.nodes[index].child1;
		let child2 = self.nodes[index].child2;
		let (child1_min_x, child1_min_y, child1_max_x, child1_max_y, child1_category_bits, child1_height) = {
			let child_node = &self.nodes[child1];
			(child_node.min_x, child_node.min_y, child_node.max_x, child_node.max_y, child_node.category_bits, child_node.height)
		};
		let (child2_min_x, child2_min_y, child2_max_x, child2_max_y, child2_category_bits, child2_height) = {
			let child_node = &self.nodes[child2];
			(child_node.min_x, child_node.min_y, child_node.max_x, child_node.max_y, child_node.category_bits, child_node.height)
		};
		let node = &mut self.nodes[index];
		node.min_x = child1_min_x.min(child2_min_x);
		node.min_y = child1_min_y.min(child2_min_y);
		node.max_x = child1_max_x.max(child2_max_x);
		node.max_y = child1_max_y.max(child2_max_y);
		node.category_bits = child1_category_bits | child2_category_bits;
		node.height = 1 + child1_height.max(child2_height);
	}

	//==============================================================================
	/// index 부터 뿌리까지 상자, 분류 비트, 높이를 고치고 노드마다 돌리기를 해 본다.
	//==============================================================================
	fn refit_ancestors(&mut self, start_index: usize) {
		let mut index = start_index;
		while index != NULL_NODE {
			self.refit_node(index);
			self.rotate_nodes(index);
			index = self.nodes[index].parent;
		}
	}

	//==============================================================================
	/// 나무 돌리기. (자식 하나와 반대쪽 손자 하나를 바꿔 바뀌는 자식의 둘레가 가장 많이 줄면 바꾼다)
	/// - A 의 자식이 B, C 이고 B 의 자식이 D, E, C 의 자식이 F, G 일 때 B 와 F, B 와 G, C 와 D, C 와 E 네 가지를 견준다.
	//==============================================================================
	fn rotate_nodes(&mut self, index_a: usize) {
		if self.nodes[index_a].height < 2 {
			return;
		}
		let index_b = self.nodes[index_a].child1;
		let index_c = self.nodes[index_a].child2;
		let node_b = &self.nodes[index_b];
		let node_c = &self.nodes[index_c];

		// 0: 바꾸지 않음, 1: B 와 F, 2: B 와 G, 3: C 와 D, 4: C 와 E.
		let mut best_rotation = 0;
		let mut best_reduction = 0.0;
		if node_c.height > 0 {
			let perimeter_c = node_c.get_perimeter();
			let node_f = &self.nodes[node_c.child1];
			let node_g = &self.nodes[node_c.child2];
			let perimeter_bg = combined_perimeter(node_b, node_g);
			let perimeter_bf = combined_perimeter(node_b, node_f);
			let reduction_bf = perimeter_c - perimeter_bg;
			let reduction_bg = perimeter_c - perimeter_bf;
			if reduction_bf > best_reduction {
				best_reduction = reduction_bf;
				best_rotation = 1;
			}
			if reduction_bg > best_reduction {
				best_reduction = reduction_bg;
				best_rotation = 2;
			}
		}
		if node_b.height > 0 {
			let perimeter_b = node_b.get_perimeter();
			let node_d = &self.nodes[node_b.child1];
			let node_e = &self.nodes[node_b.child2];
			let perimeter_ce = combined_perimeter(node_c, node_e);
			let perimeter_cd = combined_perimeter(node_c, node_d);
			let reduction_cd = perimeter_b - perimeter_ce;
			let reduction_ce = perimeter_b - perimeter_cd;
			if reduction_cd > best_reduction {
				best_reduction = reduction_cd;
				best_rotation = 3;
			}
			if reduction_ce > best_reduction {
				best_rotation = 4;
			}
		}

		match best_rotation {
			1 => {
				self.swap_child_with_grandchild(index_a, index_b, index_c, true);
			}
			2 => {
				self.swap_child_with_grandchild(index_a, index_b, index_c, false);
			}
			3 => {
				self.swap_child_with_grandchild(index_a, index_c, index_b, true);
			}
			4 => {
				self.swap_child_with_grandchild(index_a, index_c, index_b, false);
			}
			_ => {}
		}
	}

	//==============================================================================
	/// A 의 자식 child 와 A 의 다른 자식 other 의 자식(is_first 면 첫째, 아니면 둘째)을 바꾼다.
	//==============================================================================
	fn swap_child_with_grandchild(&mut self, index_a: usize, child: usize, other: usize, is_first: bool) {
		let grandchild = if is_first {
			self.nodes[other].child1
		}
		else {
			self.nodes[other].child2
		};
		if self.nodes[index_a].child1 == child {
			self.nodes[index_a].child1 = grandchild;
		}
		else {
			self.nodes[index_a].child2 = grandchild;
		}
		if is_first {
			self.nodes[other].child1 = child;
		}
		else {
			self.nodes[other].child2 = child;
		}
		self.nodes[child].parent = other;
		self.nodes[grandchild].parent = index_a;
		self.refit_node(other);
		let a_child1_height = self.nodes[self.nodes[index_a].child1].height;
		let a_child2_height = self.nodes[self.nodes[index_a].child2].height;
		self.nodes[index_a].height = 1 + a_child1_height.max(a_child2_height);
	}

	//==============================================================================
	/// 상자와 겹치는 프록시 찾기.
	/// - 분류 비트가 mask_bits 와 겹치고 상자가 닿거나 겹치는 잎마다 callback(proxy_id) 를 부른다. false 를 돌려주면 멈춘다.
	///
	/// # 인자
	/// - `aabb`: 찾을 상자.
	/// - `mask_bits`: 찾을 분류.
	/// - `callback`: (proxy_id) -> 이어 갈지.
	//==============================================================================
	pub fn query(&self, aabb: &AABB2D, mask_bits: u32, mut callback: impl FnMut(usize) -> bool) {
		if self.root == NULL_NODE {
			return;
		}
		let mut stack = vec![self.root];
		while let Some(index) = stack.pop() {
			let node = &self.nodes[index];
			if node.category_bits & mask_bits == 0 {
				continue;
			}
			if aabb.min_x > node.max_x || aabb.min_y > node.max_y || node.min_x > aabb.max_x || node.min_y > aabb.max_y {
				continue;
			}
			if node.height == 0 {
				if !callback(index) {
					return;
				}
			}
			else {
				stack.push(node.child1);
				stack.push(node.child2);
			}
		}
	}

	//==============================================================================
	/// 광선(선분)이 지나는 프록시 찾기.
	/// - 선분은 origin 에서 origin + translation 까지다. 상자와 선분이 겹칠 수 있는 잎마다 callback(proxy_id, maximum_fraction) 을 부른다.
	/// - callback 이 돌려주는 값: 0 이면 멈춘다, 0 보다 작으면 그 잎을 무시한다, 그 밖에는 그 값(지금 maximum_fraction 보다 작을 때)으로 선분을 줄인다.
	///   가장 가까운 것만 찾을 때는 맞은 몫을, 모두 찾을 때는 받은 maximum_fraction 을 그대로 돌려준다.
	///
	/// # 인자
	/// - `origin_x`: 시작점 x.
	/// - `origin_y`: 시작점 y.
	/// - `translation_x`: 끝점까지의 x 이동.
	/// - `translation_y`: 끝점까지의 y 이동.
	/// - `mask_bits`: 찾을 분류.
	/// - `callback`: (proxy_id, maximum_fraction) -> 새 몫.
	//==============================================================================
	pub fn ray_cast(&self, origin_x: f64, origin_y: f64, translation_x: f64, translation_y: f64, mask_bits: u32, mut callback: impl FnMut(usize, f64) -> f64) {
		if self.root == NULL_NODE {
			return;
		}
		let length_squared = translation_x * translation_x + translation_y * translation_y;
		if length_squared <= 0.0 {
			return;
		}
		let length = length_squared.sqrt();
		let direction_x = translation_x / length;
		let direction_y = translation_y / length;
		// 선분에 수직인 축. (선분이 상자와 떨어져 있는지 이 축으로 본다)
		let perpendicular_x = -direction_y;
		let perpendicular_y = direction_x;
		let absolute_perpendicular_x = perpendicular_x.abs();
		let absolute_perpendicular_y = perpendicular_y.abs();

		let mut maximum_fraction = 1.0;
		let mut end_x = origin_x + maximum_fraction * translation_x;
		let mut end_y = origin_y + maximum_fraction * translation_y;
		let mut segment_min_x = origin_x.min(end_x);
		let mut segment_min_y = origin_y.min(end_y);
		let mut segment_max_x = origin_x.max(end_x);
		let mut segment_max_y = origin_y.max(end_y);

		let mut stack = vec![self.root];
		while let Some(index) = stack.pop() {
			let node = &self.nodes[index];
			if node.category_bits & mask_bits == 0 {
				continue;
			}
			if segment_min_x > node.max_x || segment_min_y > node.max_y || node.min_x > segment_max_x || node.min_y > segment_max_y {
				continue;
			}
			let center_x = 0.5 * (node.min_x + node.max_x);
			let center_y = 0.5 * (node.min_y + node.max_y);
			let extent_x = 0.5 * (node.max_x - node.min_x);
			let extent_y = 0.5 * (node.max_y - node.min_y);
			let separation = (perpendicular_x * (origin_x - center_x) + perpendicular_y * (origin_y - center_y)).abs() - (absolute_perpendicular_x * extent_x + absolute_perpendicular_y * extent_y);
			if separation > 0.0 {
				continue;
			}
			if node.height == 0 {
				let value = callback(index, maximum_fraction);
				if value == 0.0 {
					return;
				}
				if value > 0.0 && value <= maximum_fraction {
					maximum_fraction = value;
					end_x = origin_x + maximum_fraction * translation_x;
					end_y = origin_y + maximum_fraction * translation_y;
					segment_min_x = origin_x.min(end_x);
					segment_min_y = origin_y.min(end_y);
					segment_max_x = origin_x.max(end_x);
					segment_max_y = origin_y.max(end_y);
				}
			}
			else {
				stack.push(node.child1);
				stack.push(node.child2);
			}
		}
	}

	//==============================================================================
	/// 나무 검사. (부모와 자식 연결, 높이, 상자 감싸기, 분류 비트, 프록시 수가 맞는지, 시험용)
	///
	/// # 돌려줌
	/// 모두 맞으면 true.
	//==============================================================================
	pub fn validate(&self) -> bool {
		if self.root == NULL_NODE {
			return self.proxy_count == 0;
		}
		if self.nodes[self.root].parent != NULL_NODE {
			return false;
		}
		let mut leaf_count = 0;
		let mut stack = vec![self.root];
		while let Some(index) = stack.pop() {
			let node = &self.nodes[index];
			if node.height == 0 {
				leaf_count += 1;
				continue;
			}
			let child1_node = &self.nodes[node.child1];
			let child2_node = &self.nodes[node.child2];
			if child1_node.parent != index || child2_node.parent != index {
				return false;
			}
			if node.height != 1 + child1_node.height.max(child2_node.height) {
				return false;
			}
			if node.min_x != child1_node.min_x.min(child2_node.min_x) || node.min_y != child1_node.min_y.min(child2_node.min_y) {
				return false;
			}
			if node.max_x != child1_node.max_x.max(child2_node.max_x) || node.max_y != child1_node.max_y.max(child2_node.max_y) {
				return false;
			}
			if node.category_bits != (child1_node.category_bits | child2_node.category_bits) {
				return false;
			}
			stack.push(node.child1);
			stack.push(node.child2);
		}
		leaf_count == self.proxy_count
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 결정론 난수. (시험 장면용)
	//==============================================================================
	fn next_random(seed: &mut u64) -> f64 {
		*seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
		((*seed >> 11) as f64) / ((1u64 << 53) as f64)
	}

	//==============================================================================
	/// 넣고, 옮기고, 빼도 나무가 맞고, 질의 결과가 모두 훑기와 같다.
	//==============================================================================
	#[test]
	fn tree_insert_move_remove_query() {
		let mut tree: DynamicTree<usize> = DynamicTree::new();
		let mut seed = 7u64;
		let mut box_list = Vec::new();
		let mut proxy_list = Vec::new();
		for index in 0..300 {
			let x = next_random(&mut seed) * 1000.0;
			let y = next_random(&mut seed) * 1000.0;
			let size = 1.0 + next_random(&mut seed) * 30.0;
			let aabb = AABB2D::create(x, y, x + size, y + size);
			proxy_list.push(tree.create_proxy(&aabb, 1 << (index % 3), index));
			box_list.push(aabb);
		}
		assert!(tree.validate());
		for index in 0..300 {
			if index % 3 == 0 {
				let x = next_random(&mut seed) * 1000.0;
				let y = next_random(&mut seed) * 1000.0;
				let aabb = AABB2D::create(x, y, x + 10.0, y + 10.0);
				tree.move_proxy(proxy_list[index], &aabb);
				box_list[index] = aabb;
			}
		}
		for index in (0..300).step_by(7) {
			tree.destroy_proxy(proxy_list[index]);
		}
		assert!(tree.validate());
		assert_eq!(tree.get_proxy_count(), 300 - 43);
		// 균형. (잎 257 개면 log2 로 9 남짓)
		assert!(tree.get_height() < 20, "높이 {}", tree.get_height());

		let query_box = AABB2D::create(200.0, 200.0, 500.0, 450.0);
		let mut found_list = Vec::new();
		tree.query(&query_box, 0b101, |proxy_id| {
			found_list.push(*tree.get_user_data(proxy_id).expect("값"));
			true
		});
		found_list.sort_unstable();
		let mut expected_list = Vec::new();
		for index in 0..300 {
			if index % 7 == 0 || (1u32 << (index % 3)) & 0b101 == 0 {
				continue;
			}
			if box_list[index].overlaps(&query_box) {
				expected_list.push(index);
			}
		}
		assert_eq!(found_list, expected_list);
	}

	//==============================================================================
	/// 광선은 가장 가까운 상자를 찾고, 0 을 돌려주면 멈춘다.
	//==============================================================================
	#[test]
	fn tree_ray_cast() {
		let mut tree: DynamicTree<usize> = DynamicTree::new();
		for index in 0..20 {
			let x = 100.0 + index as f64 * 50.0;
			tree.create_proxy(&AABB2D::create(x, -10.0, x + 20.0, 10.0), 1, index);
		}
		let mut closest = None;
		let mut visit_count = 0;
		tree.ray_cast(0.0, 0.0, 2000.0, 0.0, 1, |proxy_id, maximum_fraction| {
			visit_count += 1;
			let aabb = tree.get_aabb(proxy_id);
			let fraction = aabb.min_x / 2000.0;
			if fraction <= maximum_fraction {
				closest = tree.get_user_data(proxy_id).copied();
				return fraction;
			}
			maximum_fraction
		});
		assert_eq!(closest, Some(0));
		assert!(visit_count < 20);
		let mut stop_count = 0;
		tree.ray_cast(0.0, 0.0, 2000.0, 0.0, 1, |_, _| {
			stop_count += 1;
			0.0
		});
		assert_eq!(stop_count, 1);
		// 위로 비켜난 광선은 아무것도 맞지 않는다.
		let mut miss_count = 0;
		tree.ray_cast(0.0, 50.0, 2000.0, 0.0, 1, |_, maximum_fraction| {
			miss_count += 1;
			maximum_fraction
		});
		assert_eq!(miss_count, 0);
	}
}
