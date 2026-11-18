//==============================================================================
// 노드 레이아웃.
// - 원본 misc/nodelayout.js 를 옮긴 것이다. 지역 변수 없이 노드 계층을 선언하듯 짜는 플루언트 빌더다.
// - Vector2 없이 숫자쌍(x, y)으로 위치와 크기를 정한다.
// - 사용:
//     let panelId = NodeLayout::create()
//         .pivot(Pivot::middleCenter)
//         .contentSize(680.0, 900.0)
//         .componentWith(Paint::new(), |paint| { paint.setColor(Color::create(0.1, 0.1, 0.1, 1.0)); })
//         .children([
//             NodeLayout::create()
//                 .contentSize(120.0, 70.0)
//                 .componentWith(Text::new(), |text| { text.setText("확인"); }),
//         ])
//         .build(nodeTree, Some(parentId));
// - 원본과 다른 점:
//   러스트 판은 노드가 트리(NodeTree) 안에만 있어서, 빌더는 설정을 차례대로 모아 두었다가 build(nodeTree, parentId) 때 노드를 만들고 그 순서대로 건다.
//   그래서 컴포넌트도 build 때 nodeTree.addComponent 로 붙는다. (attachToTree 가 불려 자식 노드를 만드는 컴포넌트도 된다)
//   노드 클래스가 없으므로 create(nodeClass) 대신 create() 다.
//   component(type, callback) 은 component(값) 과 componentWith(값, 콜백) 둘로 나눴다. (러스트는 생략 인자가 없다)
//   children(...layouts) 의 가변 인자는 배열이나 Vec 하나로 받는다.
//   anchoredPosition 은 build 때 부모 contentSize 기준으로 건다. 원본은 부모가 붙기 전에 불려서 사실상 localPosition 과 같았다.
//   arrangeFan 은 원본이 System 을 가져오지 않아 부르면 멈추고, 라디안 값을 도 단위 회전에 넣는다. 러스트 판은 각도를 도로 바꿔 건다.
//   maskEnabled(false) 는 원본처럼 첫 마스크만이 아니라 그 노드의 Mask 를 모두 뗀다.
//==============================================================================
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::mask::Mask;
use crate::core::component::Component;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::WorldNode;

// build 때 노드에 차례대로 걸 설정 하나.
type NodeLayoutAction = Box<dyn FnOnce(&mut NodeTree, NodeId)>;

//==============================================================================
// 부채꼴 배치 설정. (원본 options = { centerX, centerY, radius, angleStepRadian = 0.12, baseAngleRadian = -PI/2, rotateNodes = true })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeLayoutFanOptions {
	// 호의 중심.
	pub centerX: f32,
	pub centerY: f32,
	// 호의 반지름.
	pub radius: f32,
	// 이웃한 항목 사이 각도. (라디안)
	pub angleStepRadian: f32,
	// 가운데 항목이 바라보는 방향. (라디안, 기본 위쪽)
	pub baseAngleRadian: f32,
	// 항목 회전도 호의 접선에 맞출지.
	pub rotateNodes: bool,
}

impl Default for NodeLayoutFanOptions {
	//==============================================================================
	// 기본값. (중심 0, 반지름 0, 간격 0.12 라디안, 위쪽, 회전함)
	//==============================================================================
	fn default() -> NodeLayoutFanOptions {
		NodeLayoutFanOptions {
			centerX: 0.0,
			centerY: 0.0,
			radius: 0.0,
			angleStepRadian: 0.12,
			baseAngleRadian: -math::PI / 2.0,
			rotateNodes: true,
		}
	}
}

//==============================================================================
// 노드 레이아웃.
//==============================================================================
pub struct NodeLayout {
	actions: Vec<NodeLayoutAction>,
	childLayouts: Vec<NodeLayout>,
	parentId: Option<NodeId>,
}

impl NodeLayout {
	//==============================================================================
	// 생성. (빈 설정)
	//==============================================================================
	pub fn new() -> NodeLayout {
		NodeLayout {
			actions: Vec::new(),
			childLayouts: Vec::new(),
			parentId: None,
		}
	}

	//==============================================================================
	// 레이아웃 만들기. (원본 NodeLayout.create(WorldNode))
	//==============================================================================
	pub fn create() -> NodeLayout {
		NodeLayout::new()
	}

	//==============================================================================
	// 부채꼴 배치. (손패 카드처럼 노드들을 호 위에 펼친다)
	// - 가운데 항목이 baseAngleRadian 방향을 보도록 좌우 대칭으로 벌린다.
	// - rotateNodes 면 각 노드의 회전(도)도 호의 접선에 맞춘다.
	//==============================================================================
	pub fn arrangeFan(nodeTree: &mut NodeTree, nodeIds: &[NodeId], options: NodeLayoutFanOptions) {
		let count = nodeIds.len();
		if count == 0 {
			return;
		}
		let middleIndex = (count as f32 - 1.0) / 2.0;
		for (index, nodeId) in nodeIds.iter().enumerate() {
			let node = match nodeTree.getNodeMut(*nodeId) {
				Some(node) => {
					node
				}
				None => {
					continue;
				}
			};
			let angleRadian = options.baseAngleRadian + (index as f32 - middleIndex) * options.angleStepRadian;
			let positionX = options.centerX + angleRadian.cos() * options.radius;
			let positionY = options.centerY + angleRadian.sin() * options.radius;
			node.setLocalPosition(Vector2::create(positionX, positionY));
			if options.rotateNodes {
				let rotationDegree = math::radianToDegree(angleRadian - options.baseAngleRadian);
				node.setLocalRotation(rotationDegree);
			}
		}
	}

	//==============================================================================
	// 설정 하나를 뒤에 붙인다. (build 때 붙인 순서대로 걸린다)
	//==============================================================================
	fn pushAction(mut self, action: NodeLayoutAction) -> NodeLayout {
		self.actions.push(action);
		self
	}

	//==============================================================================
	// 노드 직접 접근. (빌더에 없는 속성을 정할 때)
	//==============================================================================
	pub fn apply(self, callback: impl FnOnce(&mut WorldNode) + 'static) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			callback(&mut nodeTree[nodeId]);
		}))
	}

	//==============================================================================
	// 활성화 설정.
	//==============================================================================
	pub fn active(self, isActive: bool) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree[nodeId].setActive(isActive);
		}))
	}

	//==============================================================================
	// 이름 설정.
	//==============================================================================
	pub fn name(self, name: &str) -> NodeLayout {
		let name = name.to_string();
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree[nodeId].setName(&name);
		}))
	}

	//==============================================================================
	// 컴포넌트 추가. (설정은 넘기기 전에 값에 해 둔다)
	//==============================================================================
	pub fn component<T: Component + 'static>(self, component: T) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree.addComponent(nodeId, component);
		}))
	}

	//==============================================================================
	// 컴포넌트 추가 후 콜백으로 설정. (원본 component(type, callback))
	//==============================================================================
	pub fn componentWith<T: Component + 'static>(self, component: T, callback: impl FnOnce(&mut T) + 'static) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			if let Some(addedComponent) = nodeTree.addComponent(nodeId, component) {
				callback(addedComponent);
			}
		}))
	}

	//==============================================================================
	// 자식 레이아웃 추가. (여러 개를 한 번에, 붙인 순서대로 그려진다)
	//==============================================================================
	pub fn children(mut self, layouts: impl IntoIterator<Item = NodeLayout>) -> NodeLayout {
		for layout in layouts {
			self.childLayouts.push(layout);
		}
		self
	}

	//==============================================================================
	// 로컬 위치 설정.
	//==============================================================================
	pub fn localPosition(self, x: f32, y: f32) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree[nodeId].setLocalPosition(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	// 피봇 설정.
	//==============================================================================
	pub fn pivot(self, pivot: Vector2) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree[nodeId].setPivot(pivot);
		}))
	}

	//==============================================================================
	// 컨텐츠 크기 설정.
	//==============================================================================
	pub fn contentSize(self, x: f32, y: f32) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree[nodeId].setContentSize(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	// 로컬 크기 설정.
	//==============================================================================
	pub fn localScale(self, x: f32, y: f32) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree[nodeId].setLocalScale(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	// 부모 노드 설정. (build 에 부모를 넘기지 않으면 이 부모의 자식으로 붙는다)
	//==============================================================================
	pub fn parent(mut self, parentId: NodeId) -> NodeLayout {
		self.parentId = Some(parentId);
		self
	}

	//==============================================================================
	// 앵커 설정.
	//==============================================================================
	pub fn anchor(self, x: f32, y: f32) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			nodeTree[nodeId].setAnchor(Vector2::create(x, y));
		}))
	}

	//==============================================================================
	// 앵커 기준 위치 설정. (부모가 있으면 부모 contentSize 의 앵커 점 기준, 없으면 로컬 위치 그대로)
	//==============================================================================
	pub fn anchoredPosition(self, x: f32, y: f32) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			let anchoredPosition = Vector2::create(x, y);
			let parentContentSize = match nodeTree.getParentId(nodeId) {
				Some(parentId) => {
					nodeTree[parentId].getContentSize()
				}
				None => {
					Vector2::zero()
				}
			};
			nodeTree[nodeId].getTransformMut().setAnchoredPosition(anchoredPosition, parentContentSize);
		}))
	}

	//==============================================================================
	// 마스크 켜기 / 끄기. (켜면 Mask 가 없을 때만 붙이고, 끄면 뗀다)
	//==============================================================================
	pub fn maskEnabled(self, isEnabled: bool) -> NodeLayout {
		self.pushAction(Box::new(move |nodeTree, nodeId| {
			if isEnabled {
				let hasMask = nodeTree[nodeId].hasComponent::<Mask>();
				if !hasMask {
					nodeTree.addComponent(nodeId, Mask::new());
				}
			}
			else {
				nodeTree[nodeId].removeComponent::<Mask>();
			}
		}))
	}

	//==============================================================================
	// 빌드. (노드를 만들어 부모에 붙이고, 모은 설정을 차례대로 건 뒤, 자식 레이아웃을 재귀로 빌드한다)
	// - parentId 를 넘기면 그 부모에, 없으면 parent() 로 정한 부모에 붙인다. 둘 다 없으면 어디에도 붙지 않는다.
	// - 만든 노드의 핸들을 돌려준다.
	//==============================================================================
	pub fn build(self, nodeTree: &mut NodeTree, parentId: Option<NodeId>) -> NodeId {
		let nodeId = nodeTree.createNode("");
		let targetParentId = parentId.or(self.parentId);
		if let Some(targetParentId) = targetParentId {
			nodeTree.addChild(targetParentId, nodeId);
		}
		for action in self.actions {
			action(nodeTree, nodeId);
		}
		for childLayout in self.childLayouts {
			childLayout.build(nodeTree, Some(nodeId));
		}
		nodeId
	}
}

impl Default for NodeLayout {
	//==============================================================================
	// 기본값. (빈 설정)
	//==============================================================================
	fn default() -> NodeLayout {
		NodeLayout::new()
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::color::Color;
	use crate::base::pivot::Pivot;
	use crate::core::component::paint::Paint;
	use crate::core::component::text::Text;

	//==============================================================================
	// 중첩 레이아웃이 이름, 크기, 위치, 컴포넌트, 부모 관계를 그대로 만든다.
	//==============================================================================
	#[test]
	fn buildCreatesNestedNodes() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let panelId = NodeLayout::create()
			.name("panel")
			.pivot(Pivot::topLeft)
			.contentSize(680.0, 900.0)
			.localPosition(10.0, 20.0)
			.componentWith(Paint::new(), |paint| {
				paint.setColor(Color::create(0.1, 0.2, 0.3, 1.0));
			})
			.apply(|node| {
				node.setInteractable(true);
			})
			.children([
				NodeLayout::create().name("title").contentSize(120.0, 70.0).componentWith(Text::new(), |text| {
					text.setText("확인");
				}),
				NodeLayout::create().name("corner").anchor(1.0, 1.0).anchoredPosition(-10.0, -20.0).active(false),
			])
			.build(&mut nodeTree, Some(rootId));

		let panel = &nodeTree[panelId];
		assert_eq!(panel.getName(), "panel");
		assert_eq!(panel.getContentSize(), Vector2::create(680.0, 900.0));
		assert_eq!(panel.getLocalPosition(), Vector2::create(10.0, 20.0));
		assert_eq!(panel.getPivot(), Pivot::topLeft);
		assert!(panel.isInteractable());
		assert_eq!(panel.getComponent::<Paint>().expect("paint").getColor().green, 0.2);
		assert_eq!(nodeTree.getParentId(panelId), Some(rootId));

		let titleId = nodeTree.findChildByName(panelId, "title").expect("title");
		assert_eq!(nodeTree[titleId].getComponent::<Text>().expect("text").getText(), "확인");
		let cornerId = nodeTree.findChildByName(panelId, "corner").expect("corner");
		assert_eq!(nodeTree[cornerId].getLocalPosition(), Vector2::create(670.0, 880.0));
		assert!(!nodeTree[cornerId].isActive());
		assert_eq!(nodeTree.getChildIds(panelId), &[titleId, cornerId]);
	}

	//==============================================================================
	// parent() 로 정한 부모, 마스크 켜고 끄기.
	//==============================================================================
	#[test]
	fn parentAndMaskToggle() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let maskedId = NodeLayout::create().parent(rootId).maskEnabled(true).maskEnabled(true).build(&mut nodeTree, None);
		assert_eq!(nodeTree.getParentId(maskedId), Some(rootId));
		let maskCount = nodeTree[maskedId].getAllComponents().len();
		assert_eq!(maskCount, 1);
		let unmaskedId = NodeLayout::create().maskEnabled(true).maskEnabled(false).build(&mut nodeTree, None);
		assert!(!nodeTree[unmaskedId].hasComponent::<Mask>());
		assert_eq!(nodeTree.getParentId(unmaskedId), None);
	}

	//==============================================================================
	// 부채꼴 배치. (가운데는 위쪽, 좌우 대칭, 회전은 도)
	//==============================================================================
	#[test]
	fn arrangeFanSpreadsSymmetrically() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let nodeIds: Vec<NodeId> = (0..3).map(|_| nodeTree.createChild(rootId, "card")).collect();
		NodeLayout::arrangeFan(&mut nodeTree, &nodeIds, NodeLayoutFanOptions {
			centerX: 100.0,
			centerY: 500.0,
			radius: 200.0,
			..NodeLayoutFanOptions::default()
		});
		let middlePosition = nodeTree[nodeIds[1]].getLocalPosition();
		assert!((middlePosition.x - 100.0).abs() < 0.001);
		assert!((middlePosition.y - 300.0).abs() < 0.001);
		assert_eq!(nodeTree[nodeIds[1]].getLocalRotation(), 0.0);
		let leftPosition = nodeTree[nodeIds[0]].getLocalPosition();
		let rightPosition = nodeTree[nodeIds[2]].getLocalPosition();
		assert!((leftPosition.x - 100.0 + (rightPosition.x - 100.0)).abs() < 0.001);
		assert!((leftPosition.y - rightPosition.y).abs() < 0.001);
		let rightRotation = nodeTree[nodeIds[2]].getLocalRotation();
		assert!((rightRotation - math::radianToDegree(0.12)).abs() < 0.001);
	}
}
