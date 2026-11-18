//==============================================================================
// 노드 트리.
// - 씬의 노드를 저장소 하나(slotmap)에 담고, 부모와 자식을 핸들(NodeId)로 잇는다.
// - 게임 코드는 NodeId 를 들고 다니다가 getNode / getNodeMut (또는 tree[nodeId]) 로 노드를 꺼낸다.
//   지운 노드의 NodeId 는 getNode 가 None 을 돌려준다. (원본 isDestroyed 자리)
// - 원본은 부모가 children[] 으로 자식 객체를 들고 자식이 parent 로 거꾸로 가리킨다.
//   러스트에서 그 꼴(Rc<RefCell>)은 실행 중 빌림 검사 비용과 순환 참조가 생겨서 핸들로 바꿨다.
// - tick / 터치를 돌 때는 부르는 컴포넌트 하나만 노드에서 잠깐 꺼내(그 자리에는 빈 자리표를 둔다) ComponentContext(자기 노드 + 트리 전체 + 엔진)를 넘긴다.
//   그래서 컴포넌트가 다른 노드를 찾고 바꾸고 만들 수 있고, 같은 노드의 다른 컴포넌트(형제)도 getNodeMut().getComponentMut::<T>() 로 찾는다.
// - 도는 중에 지운 노드는 그 자리에서 비활성으로 두고, 다 돈 뒤에 실제로 지운다.
//==============================================================================
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
// 터치 종류. (NodeTree::dispatchTouch 가 컴포넌트의 어느 함수를 부를지)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchKind {
	press,
	moved,
	release,
	cancel,
}

//==============================================================================
// 노드 트리.
//==============================================================================
pub struct NodeTree {
	nodes: SlotMap<NodeId, WorldNode>,
	rootId: NodeId,
	traversalDepth: u32,
	pendingDestroyIds: Vec<NodeId>,
	culledNodeCount: u32,
}

impl NodeTree {
	//==============================================================================
	// 생성. (루트 노드 "root" 를 함께 만든다. 루트의 피봇은 왼쪽 위)
	//==============================================================================
	pub fn new() -> NodeTree {
		let mut nodes: SlotMap<NodeId, WorldNode> = SlotMap::with_key();
		let mut root = WorldNode::new("root");
		root.setPivot(Vector2::zero());
		let rootId = nodes.insert(root);
		NodeTree {
			nodes,
			rootId,
			traversalDepth: 0,
			pendingDestroyIds: Vec::new(),
			culledNodeCount: 0,
		}
	}

	//==============================================================================
	// 루트 노드 핸들.
	//==============================================================================
	pub fn getRootId(&self) -> NodeId {
		self.rootId
	}

	//==============================================================================
	// 노드 수. (루트와 아직 트리에 붙지 않은 노드까지)
	//==============================================================================
	pub fn getNodeCount(&self) -> usize {
		self.nodes.len()
	}

	//==============================================================================
	// 노드가 살아 있는지.
	//==============================================================================
	pub fn contains(&self, nodeId: NodeId) -> bool {
		self.nodes.contains_key(nodeId)
	}

	//==============================================================================
	// 노드 반환.
	//==============================================================================
	pub fn getNode(&self, nodeId: NodeId) -> Option<&WorldNode> {
		self.nodes.get(nodeId)
	}

	//==============================================================================
	// 노드 반환. (변경용)
	//==============================================================================
	pub fn getNodeMut(&mut self, nodeId: NodeId) -> Option<&mut WorldNode> {
		self.nodes.get_mut(nodeId)
	}

	//==============================================================================
	// 노드 만들기. (아직 어느 부모에도 붙지 않는다. addChild 로 붙여야 그려진다)
	//==============================================================================
	pub fn createNode(&mut self, name: &str) -> NodeId {
		self.nodes.insert(WorldNode::new(name))
	}

	//==============================================================================
	// 자식 노드 만들기. (만들어서 parentId 의 맨 뒤 자식으로 붙인다)
	//==============================================================================
	pub fn createChild(&mut self, parentId: NodeId, name: &str) -> NodeId {
		let childId = self.createNode(name);
		self.addChild(parentId, childId);
		childId
	}

	//==============================================================================
	// 자식 붙이기. (맨 뒤. 다른 부모에 붙어 있으면 떼어서 옮긴다)
	// - 자기 자신이나 자기 조상 밑으로는 붙이지 않는다. (false)
	//==============================================================================
	pub fn addChild(&mut self, parentId: NodeId, childId: NodeId) -> bool {
		let childCount = self.getChildCount(parentId);
		self.insertChild(parentId, childId, childCount)
	}

	//==============================================================================
	// 자식 끼워 넣기. (childIndex 자리, 자식 수보다 크면 맨 뒤)
	//==============================================================================
	pub fn insertChild(&mut self, parentId: NodeId, childId: NodeId, childIndex: usize) -> bool {
		if !self.nodes.contains_key(parentId) || !self.nodes.contains_key(childId) {
			return false;
		}
		if childId == self.rootId || self.isAncestorOrSelf(childId, parentId) {
			return false;
		}
		self.removeFromParent(childId);
		let parentNode = &mut self.nodes[parentId];
		let insertIndex = childIndex.min(parentNode.childIds.len());
		parentNode.childIds.insert(insertIndex, childId);
		self.nodes[childId].parentId = Some(parentId);
		true
	}

	//==============================================================================
	// 부모에서 떼기. (노드는 지우지 않는다. 다시 addChild 할 수 있다)
	//==============================================================================
	pub fn removeFromParent(&mut self, childId: NodeId) {
		let parentId = match self.nodes.get(childId).and_then(|childNode| childNode.parentId) {
			Some(parentId) => {
				parentId
			}
			None => {
				return;
			}
		};
		if let Some(parentNode) = self.nodes.get_mut(parentId) {
			parentNode.childIds.retain(|candidateId| *candidateId != childId);
		}
		self.nodes[childId].parentId = None;
	}

	//==============================================================================
	// 노드 지우기. (자손까지. 루트는 지우지 않는다)
	// - tick / 터치를 도는 중이면 비활성으로 두고 다 돈 뒤에 지운다.
	//==============================================================================
	pub fn destroyNode(&mut self, nodeId: NodeId) {
		if nodeId == self.rootId || !self.nodes.contains_key(nodeId) {
			return;
		}
		if self.traversalDepth > 0 {
			self.nodes[nodeId].setActive(false);
			self.pendingDestroyIds.push(nodeId);
			return;
		}
		self.removeFromParent(nodeId);
		let mut destroyStack: Vec<NodeId> = vec![nodeId];
		while let Some(destroyId) = destroyStack.pop() {
			if let Some(destroyedNode) = self.nodes.remove(destroyId) {
				destroyStack.extend(destroyedNode.childIds.iter().copied());
			}
		}
	}

	//==============================================================================
	// 자식 모두 지우기.
	//==============================================================================
	pub fn destroyChildren(&mut self, parentId: NodeId) {
		let childIds: Vec<NodeId> = self.getChildIds(parentId).to_vec();
		for childId in childIds {
			self.destroyNode(childId);
		}
	}

	//==============================================================================
	// 조건에 맞는 자식 지우기. (지운 수)
	//==============================================================================
	pub fn destroyChildrenWhere(&mut self, parentId: NodeId, predicate: impl Fn(&WorldNode) -> bool) -> usize {
		let childIds: Vec<NodeId> = self.getChildIds(parentId).to_vec();
		let mut destroyedCount = 0;
		for childId in childIds {
			let isMatched = match self.nodes.get(childId) {
				Some(childNode) => {
					predicate(childNode)
				}
				None => {
					false
				}
			};
			if isMatched {
				self.destroyNode(childId);
				destroyedCount += 1;
			}
		}
		destroyedCount
	}

	//==============================================================================
	// 미뤄 둔 지우기 처리. (tick / 터치를 다 돈 뒤)
	//==============================================================================
	fn flushPendingDestroy(&mut self) {
		if self.traversalDepth > 0 || self.pendingDestroyIds.is_empty() {
			return;
		}
		let pendingDestroyIds = std::mem::take(&mut self.pendingDestroyIds);
		for pendingDestroyId in pendingDestroyIds {
			self.destroyNode(pendingDestroyId);
		}
	}

	//==============================================================================
	// 부모 핸들.
	//==============================================================================
	pub fn getParentId(&self, nodeId: NodeId) -> Option<NodeId> {
		self.nodes.get(nodeId).and_then(|node| node.parentId)
	}

	//==============================================================================
	// 자식 핸들 목록. (그리는 순서)
	//==============================================================================
	pub fn getChildIds(&self, nodeId: NodeId) -> &[NodeId] {
		match self.nodes.get(nodeId) {
			Some(node) => {
				&node.childIds
			}
			None => {
				&[]
			}
		}
	}

	//==============================================================================
	// 자식 수.
	//==============================================================================
	pub fn getChildCount(&self, nodeId: NodeId) -> usize {
		self.getChildIds(nodeId).len()
	}

	//==============================================================================
	// ancestorId 가 nodeId 자신이거나 조상인지.
	//==============================================================================
	pub fn isAncestorOrSelf(&self, ancestorId: NodeId, nodeId: NodeId) -> bool {
		let mut currentId = Some(nodeId);
		while let Some(candidateId) = currentId {
			if candidateId == ancestorId {
				return true;
			}
			currentId = self.getParentId(candidateId);
		}
		false
	}

	//==============================================================================
	// 이름으로 자식 찾기. (바로 아래만, 처음 하나)
	//==============================================================================
	pub fn findChildByName(&self, parentId: NodeId, name: &str) -> Option<NodeId> {
		self.getChildIds(parentId).iter().copied().find(|childId| self.nodes[*childId].getName() == name)
	}

	//==============================================================================
	// 이름으로 자손 찾기. (깊이 우선, 그리는 순서로 처음 하나)
	//==============================================================================
	pub fn findDescendantByName(&self, parentId: NodeId, name: &str) -> Option<NodeId> {
		for childId in self.getChildIds(parentId).iter().copied() {
			if self.nodes[childId].getName() == name {
				return Some(childId);
			}
			if let Some(foundId) = self.findDescendantByName(childId, name) {
				return Some(foundId);
			}
		}
		None
	}

	//==============================================================================
	// 월드 행렬. (루트부터 이 노드까지 로컬 행렬을 곱한 것. 루트의 부모 좌표계 = 뷰 좌표계)
	//==============================================================================
	pub fn calculateWorldMatrix(&self, nodeId: NodeId) -> TransformMatrix {
		let mut chainIds: Vec<NodeId> = Vec::new();
		let mut currentId = Some(nodeId);
		while let Some(candidateId) = currentId {
			if !self.nodes.contains_key(candidateId) {
				break;
			}
			chainIds.push(candidateId);
			currentId = self.getParentId(candidateId);
		}
		let mut worldMatrix = TransformMatrix::identity();
		for chainId in chainIds.iter().rev() {
			let localMatrix = self.nodes[*chainId].getTransform().calculateLocalMatrix();
			worldMatrix = worldMatrix.multiply(&localMatrix);
		}
		worldMatrix
	}

	//==============================================================================
	// 노드 영역 안의 점 → 뷰 좌표.
	//==============================================================================
	pub fn convertNodePositionToViewPosition(&self, nodeId: NodeId, nodePosition: Vector2) -> Vector2 {
		let worldMatrix = self.calculateWorldMatrix(nodeId);
		worldMatrix.transformPoint(nodePosition.x, nodePosition.y)
	}

	//==============================================================================
	// 뷰 좌표 → 노드 영역 안의 점. (행렬이 뒤집히지 않으면 None)
	//==============================================================================
	pub fn convertViewPositionToNodePosition(&self, nodeId: NodeId, viewPosition: Vector2) -> Option<Vector2> {
		let worldMatrix = self.calculateWorldMatrix(nodeId);
		let inverseMatrix = worldMatrix.invert()?;
		Some(inverseMatrix.transformPoint(viewPosition.x, viewPosition.y))
	}

	//==============================================================================
	// 노드의 컴포넌트를 잠깐 꺼내 하나씩 부른다. (꺼낸 동안 붙인 컴포넌트는 뒤에 이어 붙인다)
	//==============================================================================
	fn visitComponents(&mut self, nodeId: NodeId, engine: &Engine, visitor: &mut dyn FnMut(&mut dyn Component, &mut ComponentContext)) {
		let componentCount = match self.nodes.get(nodeId) {
			Some(node) => {
				node.components.len()
			}
			None => {
				return;
			}
		};
		self.traversalDepth += 1;
		for componentIndex in 0..componentCount {
			// 부르는 컴포넌트만 꺼내고 그 자리에 빈 자리표를 둔다. (형제 컴포넌트는 노드에 그대로 남는다)
			let mut component = match self.nodes.get_mut(nodeId) {
				Some(node) => {
					if componentIndex >= node.components.len() {
						break;
					}
					let placeholder: Box<dyn Component> = Box::new(ComponentPlaceholder::new());
					std::mem::replace(&mut node.components[componentIndex], placeholder)
				}
				None => {
					break;
				}
			};
			if component.isEnable() {
				let mut context = ComponentContext {
					nodeTree: self,
					nodeId,
					engine,
				};
				visitor(component.as_mut(), &mut context);
			}

			// 빈 자리표가 있는 곳에 되돌린다. (도는 중에 다른 컴포넌트를 지웠으면 자리가 당겨졌을 수 있다)
			if let Some(node) = self.nodes.get_mut(nodeId) {
				let placeholderIndex = node.components.iter().position(|candidate| {
					let candidateReference: &dyn Component = candidate.as_ref();
					candidateReference.asAny().is::<ComponentPlaceholder>()
				});
				if let Some(placeholderIndex) = placeholderIndex {
					node.components[placeholderIndex] = component;
				}
			}
		}
		self.traversalDepth -= 1;
	}

	//==============================================================================
	// 주기적 갱신. (루트부터 깊이 우선: 컴포넌트 → 자식 순, 원본 ComponentNode.tick)
	//==============================================================================
	pub fn tick(&mut self, engine: &Engine, timeDelta: f32) {
		let rootId = self.rootId;
		self.traversalDepth += 1;
		self.tickNode(rootId, engine, timeDelta);
		self.traversalDepth -= 1;
		self.flushPendingDestroy();
	}

	//==============================================================================
	// 컴포넌트 붙이기. (attach 다음 attachToTree 를 불러, 컴포넌트가 자식 노드를 만들 수 있게 한다)
	// - UIView 처럼 자식 노드를 만드는 컴포넌트는 이것으로 붙인다. 붙인 컴포넌트를 돌려준다.
	//==============================================================================
	pub fn addComponent<T: Component + 'static>(&mut self, nodeId: NodeId, component: T) -> Option<&mut T> {
		if !self.nodes.contains_key(nodeId) {
			return None;
		}
		let mut component = component;
		component.attach(self.nodes[nodeId].getTransformMut());
		component.attachToTree(self, nodeId);
		let node = self.nodes.get_mut(nodeId)?;
		node.components.push(Box::new(component));
		let lastComponent = node.components.last_mut()?;
		let componentReference: &mut dyn Component = lastComponent.as_mut();
		componentReference.asAnyMut().downcast_mut::<T>()
	}

	//==============================================================================
	// 컴포넌트 하나를 트리와 함께 쥐고 부르기. (씬 코드에서 위젯처럼 다른 노드도 만지는 컴포넌트를 바로 바꿀 때)
	// - nodeId 노드의 첫 번째 T 컴포넌트만 잠깐 빼고, 그 컴포넌트와 트리를 action 에 넘긴다. 없으면 None.
	// - 예: nodeTree.withComponentMut::<UIScrollView, _>(scrollId, |scrollView, tree| scrollView.setScrollOffsetInTree(tree, scrollId, offset))
	//==============================================================================
	pub fn withComponentMut<T: Component + 'static, R>(&mut self, nodeId: NodeId, action: impl FnOnce(&mut T, &mut NodeTree) -> R) -> Option<R> {
		let componentIndex = self.nodes.get(nodeId)?.components.iter().position(|candidate| {
			let candidateReference: &dyn Component = candidate.as_ref();
			candidateReference.asAny().is::<T>()
		})?;
		let placeholder: Box<dyn Component> = Box::new(ComponentPlaceholder::new());
		let mut component = std::mem::replace(&mut self.nodes[nodeId].components[componentIndex], placeholder);
		self.traversalDepth += 1;
		let result = {
			let componentReference: &mut dyn Component = component.as_mut();
			let typedComponent = componentReference.asAnyMut().downcast_mut::<T>().expect("component type");
			action(typedComponent, self)
		};
		self.traversalDepth -= 1;
		if let Some(node) = self.nodes.get_mut(nodeId) {
			let placeholderIndex = node.components.iter().position(|candidate| {
				let candidateReference: &dyn Component = candidate.as_ref();
				candidateReference.asAny().is::<ComponentPlaceholder>()
			});
			if let Some(placeholderIndex) = placeholderIndex {
				node.components[placeholderIndex] = component;
			}
		}
		self.flushPendingDestroy();
		Some(result)
	}

	//==============================================================================
	// 노드 하나와 그 자손 갱신.
	//==============================================================================
	fn tickNode(&mut self, nodeId: NodeId, engine: &Engine, timeDelta: f32) {
		let isTickNeeded = match self.nodes.get(nodeId) {
			Some(node) => {
				if !node.isActive() {
					return;
				}
				node.components.iter().any(|component| component.isEnable() && component.isTickNeeded())
			}
			None => {
				return;
			}
		};
		if isTickNeeded {
			self.visitComponents(nodeId, engine, &mut |component, context| {
				if component.isTickNeeded() {
					component.tick(context, timeDelta);
				}
			});
		}

		// 자식. (도는 중에 자식 목록이 바뀌어도 건너뛰거나 두 번 돌지 않게, 지금 자리의 자식이 그대로일 때만 다음으로 간다)
		let mut childIndex = 0;
		while let Some(childId) = self.nodes.get(nodeId).and_then(|node| node.childIds.get(childIndex).copied()) {
			self.tickNode(childId, engine, timeDelta);
			let isSameChild = self.nodes.get(nodeId).and_then(|node| node.childIds.get(childIndex).copied()) == Some(childId);
			if isSameChild {
				childIndex += 1;
			}
		}
	}

	//==============================================================================
	// 출력. (루트부터 재귀, 원본 Graphic.drawNode)
	//==============================================================================
	pub fn draw(&mut self, graphic: &mut Graphic) {
		self.culledNodeCount = 0;
		let rootId = self.rootId;
		self.drawNode(rootId, graphic);
		graphic.addCulledNodeCount(self.culledNodeCount);
	}

	//==============================================================================
	// 노드 하나와 그 자손 출력.
	// - pushTransform → 컴포넌트 → (Mask 면 클립 시작) → 자식 → (클립 끝) → 기즈모 → popTransform
	//==============================================================================
	pub fn drawNode(&mut self, nodeId: NodeId, graphic: &mut Graphic) {
		let node = match self.nodes.get_mut(nodeId) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		if !node.isActive() {
			return;
		}
		node.pushTransform(graphic);

		if node.isVisible() {
			// 컴포넌트. (화면 밖이면 건너뛴다)
			let (clipRoundSize, isCulled) = node.drawComponents(graphic);
			if isCulled {
				self.culledNodeCount += 1;
			}

			// 마스크가 있으면 자식 출력 전에 클리핑 시작.
			if let Some(roundSize) = clipRoundSize {
				let transform = self.nodes[nodeId].getTransform().clone();
				Mask::beginClip(&transform, roundSize, graphic);
			}

			// 자식. (그리는 동안에는 트리가 바뀌지 않는다)
			let childCount = self.nodes[nodeId].childIds.len();
			for childIndex in 0..childCount {
				let childId = self.nodes[nodeId].childIds[childIndex];
				self.drawNode(childId, graphic);
			}

			// 마스크가 있으면 클리핑 종료.
			if clipRoundSize.is_some() {
				Mask::endClip(graphic);
			}
		}

		let node = &mut self.nodes[nodeId];
		let isForceGizmosVisible = graphic.isForceGizmosVisible();
		if isForceGizmosVisible || node.isGizmoVisible() {
			node.drawGizmos(graphic);
		}
		node.popTransform(graphic);
	}

	//==============================================================================
	// 그리는 순서로 돌며 점 아래 맨 위의 interactable 노드 찾기. (원본 TouchRecognizer 의 대상 고르기)
	//==============================================================================
	pub fn findTopmostInteractableNode(&self, viewPosition: Vector2) -> Option<NodeId> {
		let mut foundId: Option<NodeId> = None;
		let identityMatrix = TransformMatrix::identity();
		self.findTopmostInteractableNodeRecursive(self.rootId, &identityMatrix, viewPosition, &mut foundId);
		foundId
	}

	fn findTopmostInteractableNodeRecursive(&self, nodeId: NodeId, parentMatrix: &TransformMatrix, viewPosition: Vector2, foundId: &mut Option<NodeId>) {
		let node = match self.nodes.get(nodeId) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		if !node.isActive() || !node.isVisible() {
			return;
		}
		let localMatrix = node.getTransform().calculateLocalMatrix();
		let worldMatrix = parentMatrix.multiply(&localMatrix);
		if node.isInteractable() && node.containsPoint(&worldMatrix, viewPosition) {
			*foundId = Some(nodeId);
		}

		// 잘라 그리는(Mask) 노드 밖이면 자손은 보이지 않으므로 맞지 않는다. (원본 TouchRaycaster.isMaskedOut)
		let isClipping = node.components.iter().any(|component| component.isEnable() && component.getClipRoundSize().is_some());
		if isClipping && !node.containsPoint(&worldMatrix, viewPosition) {
			return;
		}
		for childId in node.childIds.iter().copied() {
			self.findTopmostInteractableNodeRecursive(childId, &worldMatrix, viewPosition, foundId);
		}
	}

	//==============================================================================
	// 터치 전달. (노드의 컴포넌트들에게)
	//==============================================================================
	pub fn dispatchTouch(&mut self, nodeId: NodeId, engine: &Engine, touchKind: TouchKind, viewInputPosition: Vector2) {
		self.traversalDepth += 1;
		self.visitComponents(nodeId, engine, &mut |component, context| {
			match touchKind {
				TouchKind::press => {
					component.touchPress(context, viewInputPosition);
				}
				TouchKind::moved => {
					component.touchMove(context, viewInputPosition);
				}
				TouchKind::release => {
					component.touchRelease(context, viewInputPosition);
				}
				TouchKind::cancel => {
					component.touchCancel(context, viewInputPosition);
				}
			}
		});
		self.traversalDepth -= 1;
		self.flushPendingDestroy();
	}

	//==============================================================================
	// 휠 전달. (nodeId 자신부터 부모 쪽으로 올라가며 처음 받아 쓴 컴포넌트에서 멈춘다. 원본 touchWheel)
	//==============================================================================
	pub fn dispatchWheel(&mut self, nodeId: NodeId, engine: &Engine, wheelDelta: Vector2) -> bool {
		let mut currentId = Some(nodeId);
		while let Some(candidateId) = currentId {
			let mut isHandled = false;
			self.traversalDepth += 1;
			self.visitComponents(candidateId, engine, &mut |component, context| {
				if !isHandled && component.wheel(context, wheelDelta) {
					isHandled = true;
				}
			});
			self.traversalDepth -= 1;
			if isHandled {
				self.flushPendingDestroy();
				return true;
			}
			currentId = self.getParentId(candidateId);
		}
		self.flushPendingDestroy();
		false
	}

	//==============================================================================
	// 노드 경계 사각형을 부모 영역 기준으로. (회전 없을 때 정확, 충돌 검사 등)
	//==============================================================================
	pub fn getLocalBounds(&self, nodeId: NodeId) -> Option<Rect> {
		self.nodes.get(nodeId).map(|node| node.getTransform().getLocalBounds())
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
	// tree[nodeId]. (지운 노드면 멈춘다. 있을지 모르면 getNode)
	//==============================================================================
	fn index(&self, nodeId: NodeId) -> &WorldNode {
		self.nodes.get(nodeId).expect("NodeTree: destroyed or unknown node id")
	}
}

impl IndexMut<NodeId> for NodeTree {
	fn index_mut(&mut self, nodeId: NodeId) -> &mut WorldNode {
		self.nodes.get_mut(nodeId).expect("NodeTree: destroyed or unknown node id")
	}
}

//==============================================================================
// 빈 자리표. (tick / 터치 중인 컴포넌트가 잠깐 비운 자리. 아무것도 하지 않는다)
//==============================================================================
struct ComponentPlaceholder {
	componentBase: ComponentBase,
}

impl ComponentPlaceholder {
	fn new() -> ComponentPlaceholder {
		let mut componentBase = ComponentBase::new();
		componentBase.setEnable(false);
		ComponentPlaceholder {
			componentBase,
		}
	}
}

impl Component for ComponentPlaceholder {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"ComponentPlaceholder"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}
}

//==============================================================================
// 컴포넌트 문맥. (tick / 터치 때 컴포넌트가 받는 것: 자기 노드, 트리 전체, 엔진)
// - 원본 컴포넌트의 getNode(), getNode().getParent(), Engine.getEngine() 자리.
//==============================================================================
pub struct ComponentContext<'a> {
	nodeTree: &'a mut NodeTree,
	nodeId: NodeId,
	engine: &'a Engine,
}

impl<'a> ComponentContext<'a> {
	//==============================================================================
	// 자기 노드 핸들.
	//==============================================================================
	pub fn getNodeId(&self) -> NodeId {
		self.nodeId
	}

	//==============================================================================
	// 자기 노드. (지금 불리는 컴포넌트만 빠져 있고, 같은 노드의 다른 컴포넌트는 getComponent 로 찾는다)
	//==============================================================================
	pub fn getNode(&self) -> &WorldNode {
		&self.nodeTree[self.nodeId]
	}

	//==============================================================================
	// 자기 노드. (변경용)
	//==============================================================================
	pub fn getNodeMut(&mut self) -> &mut WorldNode {
		let nodeId = self.nodeId;
		&mut self.nodeTree[nodeId]
	}

	//==============================================================================
	// 자기 노드 트랜스폼.
	//==============================================================================
	pub fn getTransform(&self) -> &NodeTransform {
		self.nodeTree[self.nodeId].getTransform()
	}

	//==============================================================================
	// 자기 노드 트랜스폼. (변경용)
	//==============================================================================
	pub fn getTransformMut(&mut self) -> &mut NodeTransform {
		let nodeId = self.nodeId;
		self.nodeTree[nodeId].getTransformMut()
	}

	//==============================================================================
	// 트리 전체.
	//==============================================================================
	pub fn getNodeTree(&self) -> &NodeTree {
		self.nodeTree
	}

	//==============================================================================
	// 트리 전체. (변경용. 다른 노드를 바꾸거나 만들고 지울 때)
	//==============================================================================
	pub fn getNodeTreeMut(&mut self) -> &mut NodeTree {
		self.nodeTree
	}

	//==============================================================================
	// 엔진. (입력, 시간, 뷰)
	//==============================================================================
	pub fn getEngine(&self) -> &Engine {
		self.engine
	}

	//==============================================================================
	// 다른 노드(또는 자기 노드)의 T 컴포넌트를 그 노드의 문맥과 함께 부르기. (없거나 지금 꺼내져 있으면 None)
	// - 위젯 안의 행 버튼이 위젯 본체를 부를 때 쓴다: context.withComponentMut::<UIDropdown, _>(dropdownId, |dropdown, dropdownContext| ...)
	//==============================================================================
	pub fn withComponentMut<T: Component + 'static, R>(&mut self, nodeId: NodeId, action: impl FnOnce(&mut T, &mut ComponentContext) -> R) -> Option<R> {
		let engine = self.engine;
		self.nodeTree.withComponentMut::<T, R>(nodeId, |component: &mut T, nodeTree: &mut NodeTree| {
			let mut context = ComponentContext {
				nodeTree,
				nodeId,
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
		componentBase: ComponentBase,
	}

	impl Component for SelfDestroyer {
		fn getComponentBase(&self) -> &ComponentBase {
			&self.componentBase
		}

		fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
			&mut self.componentBase
		}

		fn getComponentType(&self) -> &'static str {
			"SelfDestroyer"
		}

		fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
			let nodeId = context.getNodeId();
			context.getNodeTreeMut().destroyNode(nodeId);
		}
	}

	//==============================================================================
	// tick 마다 오른쪽으로 1 옮기는 시험 컴포넌트. (처음에 형제 노드도 하나 만든다)
	//==============================================================================
	struct Mover {
		componentBase: ComponentBase,
		isSiblingCreated: bool,
	}

	impl Component for Mover {
		fn getComponentBase(&self) -> &ComponentBase {
			&self.componentBase
		}

		fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
			&mut self.componentBase
		}

		fn getComponentType(&self) -> &'static str {
			"Mover"
		}

		fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
			let localPosition = context.getTransform().getLocalPosition();
			context.getTransformMut().setLocalPosition(Vector2::create(localPosition.x + 1.0, localPosition.y));
			if !self.isSiblingCreated {
				self.isSiblingCreated = true;
				let parentId = context.getNode().getParentId().expect("parent");
				context.getNodeTreeMut().createChild(parentId, "sibling");
			}
		}
	}

	//==============================================================================
	// 휠을 받아 쓰는 시험 컴포넌트.
	//==============================================================================
	struct WheelCatcher {
		componentBase: ComponentBase,
		wheelTotal: f32,
	}

	impl Component for WheelCatcher {
		fn getComponentBase(&self) -> &ComponentBase {
			&self.componentBase
		}

		fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
			&mut self.componentBase
		}

		fn getComponentType(&self) -> &'static str {
			"WheelCatcher"
		}

		fn wheel(&mut self, _context: &mut ComponentContext, wheelDelta: Vector2) -> bool {
			self.wheelTotal += wheelDelta.y;
			true
		}
	}

	//==============================================================================
	// 시험용 엔진. (창 없이 만든다)
	//==============================================================================
	fn createTestEngine() -> Engine {
		Engine::new(EngineConfiguration::new())
	}

	//==============================================================================
	// 만들기, 붙이기, 찾기, 지우기, 옛 핸들.
	//==============================================================================
	#[test]
	fn createFindDestroy() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let layerId = nodeTree.createChild(rootId, "layer");
		let firstId = nodeTree.createChild(layerId, "first");
		let secondId = nodeTree.createChild(layerId, "second");
		let grandchildId = nodeTree.createChild(secondId, "grandchild");
		assert_eq!(nodeTree.getChildIds(layerId), &[firstId, secondId]);
		assert_eq!(nodeTree.getParentId(grandchildId), Some(secondId));
		assert_eq!(nodeTree.findChildByName(layerId, "second"), Some(secondId));
		assert_eq!(nodeTree.findDescendantByName(rootId, "grandchild"), Some(grandchildId));
		assert_eq!(nodeTree.getNodeCount(), 5);

		nodeTree.destroyNode(secondId);
		assert!(nodeTree.getNode(secondId).is_none());
		assert!(nodeTree.getNode(grandchildId).is_none());
		assert_eq!(nodeTree.getChildIds(layerId), &[firstId]);
		assert_eq!(nodeTree.getNodeCount(), 3);

		// 지운 칸을 다시 써도 옛 핸들은 걸러진다.
		let reusedId = nodeTree.createChild(layerId, "reused");
		assert!(nodeTree.getNode(secondId).is_none());
		assert!(nodeTree.getNode(reusedId).is_some());

		// 루트는 지우지 않는다.
		nodeTree.destroyNode(rootId);
		assert!(nodeTree.contains(rootId));
	}

	//==============================================================================
	// 옮기기, 끼워 넣기, 자기 자손 밑으로 붙이기 막기.
	//==============================================================================
	#[test]
	fn reparentAndCycleGuard() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let parentId = nodeTree.createChild(rootId, "parent");
		let childId = nodeTree.createChild(parentId, "child");
		let otherId = nodeTree.createChild(rootId, "other");

		assert!(!nodeTree.addChild(childId, parentId));
		assert!(!nodeTree.addChild(parentId, parentId));
		assert!(!nodeTree.addChild(childId, rootId));

		assert!(nodeTree.addChild(otherId, childId));
		assert_eq!(nodeTree.getParentId(childId), Some(otherId));
		assert!(nodeTree.getChildIds(parentId).is_empty());

		let firstId = nodeTree.createChild(otherId, "first");
		assert!(nodeTree.insertChild(otherId, firstId, 0));
		assert_eq!(nodeTree.getChildIds(otherId), &[firstId, childId]);

		nodeTree.removeFromParent(childId);
		assert_eq!(nodeTree.getParentId(childId), None);
		assert!(nodeTree.contains(childId));
	}

	//==============================================================================
	// tick 중에 지우면 다 돈 뒤에 지워지고, 옆 형제는 그대로 돈다.
	//==============================================================================
	#[test]
	fn destroyDuringTickIsDeferred() {
		let engine = createTestEngine();
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let doomedId = nodeTree.createChild(rootId, "doomed");
		nodeTree[doomedId].addComponent(SelfDestroyer {
			componentBase: ComponentBase::new(),
		});
		let moverId = nodeTree.createChild(rootId, "mover");
		nodeTree[moverId].addComponent(Mover {
			componentBase: ComponentBase::new(),
			isSiblingCreated: false,
		});

		nodeTree.tick(&engine, 0.016);
		assert!(nodeTree.getNode(doomedId).is_none());
		assert_eq!(nodeTree[moverId].getLocalPosition().x, 1.0);
		assert!(nodeTree.findChildByName(rootId, "sibling").is_some());
		assert!(nodeTree[moverId].hasComponent::<Mover>());

		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[moverId].getLocalPosition().x, 2.0);
	}

	//==============================================================================
	// 맨 위 노드 찾기. (그리는 순서로 뒤가 위, 마스크 밖 자손은 맞지 않는다)
	//==============================================================================
	#[test]
	fn topmostTouchAndMaskClip() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let panelId = nodeTree.createChild(rootId, "panel");
		let panel = &mut nodeTree[panelId];
		panel.setPivot(Vector2::zero());
		panel.setLocalPosition(Vector2::create(100.0, 100.0));
		panel.setContentSize(Vector2::create(100.0, 100.0));
		panel.addComponent(Mask::new());

		let insideId = nodeTree.createChild(panelId, "inside");
		let inside = &mut nodeTree[insideId];
		inside.setPivot(Vector2::zero());
		inside.setContentSize(Vector2::create(300.0, 50.0));
		inside.setInteractable(true);

		let overId = nodeTree.createChild(rootId, "over");
		let over = &mut nodeTree[overId];
		over.setPivot(Vector2::zero());
		over.setLocalPosition(Vector2::create(150.0, 100.0));
		over.setContentSize(Vector2::create(10.0, 10.0));
		over.setInteractable(true);

		assert_eq!(nodeTree.findTopmostInteractableNode(Vector2::create(120.0, 120.0)), Some(insideId));
		assert_eq!(nodeTree.findTopmostInteractableNode(Vector2::create(155.0, 105.0)), Some(overId));
		// inside 는 폭이 300 이지만 패널(마스크) 밖인 250 에서는 맞지 않는다.
		assert_eq!(nodeTree.findTopmostInteractableNode(Vector2::create(250.0, 120.0)), None);
	}

	//==============================================================================
	// 휠은 맞은 노드부터 부모 쪽으로 올라가 처음 받아 쓴 컴포넌트에서 멈춘다.
	//==============================================================================
	#[test]
	fn wheelBubblesToAncestor() {
		let engine = createTestEngine();
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let scrollId = nodeTree.createChild(rootId, "scroll");
		nodeTree[scrollId].addComponent(WheelCatcher {
			componentBase: ComponentBase::new(),
			wheelTotal: 0.0,
		});
		let itemId = nodeTree.createChild(scrollId, "item");
		let isHandled = nodeTree.dispatchWheel(itemId, &engine, Vector2::create(0.0, 40.0));
		assert!(isHandled);
		let wheelTotal = nodeTree[scrollId].getComponent::<WheelCatcher>().expect("catcher").wheelTotal;
		assert_eq!(wheelTotal, 40.0);
		let isRootHandled = nodeTree.dispatchWheel(rootId, &engine, Vector2::create(0.0, 40.0));
		assert!(!isRootHandled);
	}
}

#[cfg(test)]
mod siblingTests {
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
		componentBase: ComponentBase,
	}

	impl Component for SiblingPainter {
		fn getComponentBase(&self) -> &ComponentBase {
			&self.componentBase
		}

		fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
			&mut self.componentBase
		}

		fn getComponentType(&self) -> &'static str {
			"SiblingPainter"
		}

		fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
			let node = context.getNodeMut();
			let isSelfVisible = node.hasComponent::<SiblingPainter>();
			if let Some(paint) = node.getComponentMut::<Paint>() {
				let red = if isSelfVisible {
					0.5
				}
				else {
					1.0
				};
				paint.setColor(Color::create(red, 0.0, 0.0, 1.0));
			}
		}
	}

	//==============================================================================
	// 씬 코드에서 컴포넌트와 트리를 같이 쥐고 부를 수 있다. (다른 노드를 만들고 형제도 본다)
	//==============================================================================
	#[test]
	fn withComponentMutGivesTree() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let nodeId = nodeTree.createChild(rootId, "painted");
		nodeTree[nodeId].addComponent(Paint::createWithColor(Color::white()));
		nodeTree[nodeId].addComponent(SiblingPainter {
			componentBase: ComponentBase::new(),
		});
		let createdId = nodeTree.withComponentMut::<SiblingPainter, _>(nodeId, |_painter, tree| {
			let hasPaint = tree[nodeId].hasComponent::<Paint>();
			assert!(hasPaint);
			tree.createChild(nodeId, "created")
		}).expect("painter");
		assert_eq!(nodeTree.getParentId(createdId), Some(nodeId));
		assert!(nodeTree[nodeId].hasComponent::<SiblingPainter>());
		let missing = nodeTree.withComponentMut::<Mask, _>(nodeId, |_mask, _tree| 0);
		assert!(missing.is_none());
	}

	//==============================================================================
	// tick 중인 컴포넌트가 형제 컴포넌트를 찾아 바꿀 수 있다. (자기 자신은 그동안 빠져 있다)
	//==============================================================================
	#[test]
	fn componentReachesSiblingDuringTick() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let nodeId = nodeTree.createChild(rootId, "painted");
		nodeTree[nodeId].addComponent(Paint::createWithColor(Color::white()));
		nodeTree[nodeId].addComponent(SiblingPainter {
			componentBase: ComponentBase::new(),
		});
		nodeTree.tick(&engine, 0.016);
		let paintColor = nodeTree[nodeId].getComponent::<Paint>().expect("paint").getColor();
		assert_eq!(paintColor.red, 1.0);
		assert_eq!(paintColor.green, 0.0);
		assert!(nodeTree[nodeId].hasComponent::<SiblingPainter>());
		assert_eq!(nodeTree[nodeId].getAllComponents().len(), 2);
	}
}
