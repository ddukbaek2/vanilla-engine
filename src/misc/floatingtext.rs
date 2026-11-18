//==============================================================================
// 떠오르는 텍스트 관리 컴포넌트. (데미지 숫자 / +코인 등)
// - 원본 misc/floatingtext.js 를 옮긴 것이다. spawn 으로 띄우면 위로 떠오르며 사라지고, 끝난 라벨 노드는 다시 쓴다.
// - 씬 뿌리 밑에 노드 하나를 만들어 두고 월드 좌표로 띄운다.
// - 사용:
//     let floatingTextId = FloatingText::create(nodeTree, rootId);
//     FloatingText::spawn(nodeTree, floatingTextId, "-120", position, FloatingTextOptions { color: Color::create(1.0, 0.4, 0.4, 1.0), ..FloatingTextOptions::default() });
// - 원본과 다른 점:
//   원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드에 붙는 컴포넌트이고, create 가 노드를 만들어 붙여 준다.
//   spawn / clear 는 라벨 노드를 만들고 바꿔야 해서 트리와 노드 핸들을 받는 연관 함수다. (컴포넌트가 없으면 아무것도 하지 않는다)
//   라벨은 UILabel 대신 Text 컴포넌트로 그린다. (설정은 같다: 글, 크기, 색, 굵게)
//   options 객체는 FloatingTextOptions 구조체다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};

//==============================================================================
// 띄우기 설정. (원본 options = { color?, fontSize = 22, rise = 46, duration = 0.8, jitter = 10, bold = true })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatingTextOptions {
	// 글자 색. (기본 흰색)
	pub color: Color,
	pub fontSize: f32,
	// 떠오르는 높이. (픽셀)
	pub rise: f32,
	// 사라질 때까지 걸리는 시간. (초)
	pub duration: f32,
	// 가로로 흩뿌리는 폭. (±픽셀)
	pub jitter: f32,
	pub isBold: bool,
}

impl Default for FloatingTextOptions {
	//==============================================================================
	// 기본값. (흰색, 22, 46 픽셀, 0.8 초, ±10, 굵게)
	//==============================================================================
	fn default() -> FloatingTextOptions {
		FloatingTextOptions {
			color: Color::white(),
			fontSize: 22.0,
			rise: 46.0,
			duration: 0.8,
			jitter: 10.0,
			isBold: true,
		}
	}
}

//==============================================================================
// 떠 있는 항목 하나.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct FloatingTextItem {
	nodeId: NodeId,
	startPosition: Vector2,
	rise: f32,
	duration: f32,
	elapsedSeconds: f32,
}

//==============================================================================
// 떠오르는 텍스트 관리 컴포넌트.
//==============================================================================
pub struct FloatingText {
	componentBase: ComponentBase,
	activeList: Vec<FloatingTextItem>,
	freeList: Vec<NodeId>,
}

impl FloatingText {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> FloatingText {
		FloatingText {
			componentBase: ComponentBase::new(),
			activeList: Vec::new(),
			freeList: Vec::new(),
		}
	}

	//==============================================================================
	// 노드 만들기. (parentId 밑에 "FloatingText" 노드를 만들고 컴포넌트를 붙인다. 원본 new FloatingText() + addChild)
	//==============================================================================
	pub fn create(nodeTree: &mut NodeTree, parentId: NodeId) -> NodeId {
		let floatingTextNodeId = nodeTree.createChild(parentId, "FloatingText");
		let floatingTextNode = &mut nodeTree[floatingTextNodeId];
		floatingTextNode.setPivot(Pivot::topLeft);
		floatingTextNode.setAnchor(Pivot::topLeft);
		nodeTree.addComponent(floatingTextNodeId, FloatingText::new());
		floatingTextNodeId
	}

	//==============================================================================
	// 텍스트 띄우기. (쉬는 라벨 노드가 있으면 다시 쓰고, 없으면 만든다. 띄운 라벨 노드를 돌려준다)
	//==============================================================================
	pub fn spawn(nodeTree: &mut NodeTree, floatingTextNodeId: NodeId, text: &str, worldPosition: Vector2, options: FloatingTextOptions) -> Option<NodeId> {
		nodeTree.getNode(floatingTextNodeId)?.getComponent::<FloatingText>()?;

		// 쉬는 라벨 노드 꺼내기. (밖에서 지웠거나 옮긴 노드는 버린다)
		let mut labelNodeId: Option<NodeId> = None;
		loop {
			let candidateId = match nodeTree.getNodeMut(floatingTextNodeId).and_then(|node| node.getComponentMut::<FloatingText>()) {
				Some(floatingText) => {
					floatingText.freeList.pop()
				}
				None => {
					None
				}
			};
			let candidateId = match candidateId {
				Some(candidateId) => {
					candidateId
				}
				None => {
					break;
				}
			};
			if nodeTree.getParentId(candidateId) == Some(floatingTextNodeId) {
				labelNodeId = Some(candidateId);
				break;
			}
		}
		let labelNodeId = match labelNodeId {
			Some(labelNodeId) => {
				labelNodeId
			}
			None => {
				let createdNodeId = nodeTree.createChild(floatingTextNodeId, "FloatingTextItem");
				let createdNode = &mut nodeTree[createdNodeId];
				createdNode.setPivot(Pivot::middleCenter);
				createdNode.setAnchor(Pivot::topLeft);
				createdNode.setContentSize(Vector2::create(240.0, 40.0));
				nodeTree.addComponent(createdNodeId, Text::new());
				createdNodeId
			}
		};

		let jitterX = math::randomRange(-options.jitter, options.jitter);
		let startPosition = Vector2::create(worldPosition.x + jitterX, worldPosition.y);
		let labelNode = &mut nodeTree[labelNodeId];
		labelNode.setActive(true);
		labelNode.setLocalOpacity(1.0);
		labelNode.setLocalPosition(startPosition);
		if let Some(labelText) = labelNode.getComponentMut::<Text>() {
			labelText.setText(text);
			labelText.setFontSize(options.fontSize);
			labelText.setTextColor(options.color);
			labelText.setBold(options.isBold);
		}

		let floatingText = nodeTree.getNodeMut(floatingTextNodeId)?.getComponentMut::<FloatingText>()?;
		floatingText.activeList.push(FloatingTextItem {
			nodeId: labelNodeId,
			startPosition,
			rise: options.rise,
			duration: options.duration,
			elapsedSeconds: 0.0,
		});
		Some(labelNodeId)
	}

	//==============================================================================
	// 떠 있는 항목 수 반환.
	//==============================================================================
	pub fn getActiveCount(&self) -> usize {
		self.activeList.len()
	}

	//==============================================================================
	// 모두 지우기. (떠 있는 라벨을 끄고 쉬는 목록으로 돌린다)
	//==============================================================================
	pub fn clear(nodeTree: &mut NodeTree, floatingTextNodeId: NodeId) {
		let activeList = match nodeTree.getNodeMut(floatingTextNodeId).and_then(|node| node.getComponentMut::<FloatingText>()) {
			Some(floatingText) => {
				std::mem::take(&mut floatingText.activeList)
			}
			None => {
				return;
			}
		};
		let mut releasedNodeIds: Vec<NodeId> = Vec::new();
		for item in activeList {
			if let Some(labelNode) = nodeTree.getNodeMut(item.nodeId) {
				labelNode.setActive(false);
				releasedNodeIds.push(item.nodeId);
			}
		}
		if let Some(floatingText) = nodeTree.getNodeMut(floatingTextNodeId).and_then(|node| node.getComponentMut::<FloatingText>()) {
			floatingText.freeList.extend(releasedNodeIds);
		}
	}
}

impl Default for FloatingText {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> FloatingText {
		FloatingText::new()
	}
}

impl Component for FloatingText {
	//==============================================================================
	// 공통 상태 반환.
	//==============================================================================
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	//==============================================================================
	// 공통 상태 반환. (변경용)
	//==============================================================================
	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	//==============================================================================
	// 컴포넌트 이름.
	//==============================================================================
	fn getComponentType(&self) -> &'static str {
		"FloatingText"
	}

	//==============================================================================
	// 갱신. (떠오름, 사라짐, 다 끝난 라벨 회수. 뒤에서부터 돈다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		let nodeTree = context.getNodeTreeMut();
		let mut itemIndex = self.activeList.len();
		while itemIndex > 0 {
			itemIndex -= 1;
			let item = &mut self.activeList[itemIndex];
			item.elapsedSeconds += timeDelta;
			let linearRatio = (item.elapsedSeconds / item.duration).min(1.0);
			let easedRatio = 1.0 - (1.0 - linearRatio) * (1.0 - linearRatio);
			let labelNodeId = item.nodeId;
			let labelPosition = Vector2::create(item.startPosition.x, item.startPosition.y - item.rise * easedRatio);
			let labelNode = match nodeTree.getNodeMut(labelNodeId) {
				Some(labelNode) => {
					labelNode
				}
				None => {
					// 밖에서 지운 라벨은 목록에서만 뺀다.
					self.activeList.remove(itemIndex);
					continue;
				}
			};
			labelNode.setLocalPosition(labelPosition);
			labelNode.setLocalOpacity(1.0 - linearRatio * linearRatio);
			if linearRatio >= 1.0 {
				labelNode.setActive(false);
				self.freeList.push(labelNodeId);
				self.activeList.remove(itemIndex);
			}
		}
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};

	//==============================================================================
	// 띄우면 떠오르며 흐려지고, 끝나면 꺼져서 다음 spawn 이 그 노드를 다시 쓴다.
	//==============================================================================
	#[test]
	fn spawnRisesFadesAndReuses() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let floatingTextId = FloatingText::create(&mut nodeTree, rootId);
		let options = FloatingTextOptions {
			jitter: 0.0,
			duration: 1.0,
			rise: 40.0,
			..FloatingTextOptions::default()
		};
		let labelId = FloatingText::spawn(&mut nodeTree, floatingTextId, "-120", Vector2::create(100.0, 200.0), options).expect("label");
		assert_eq!(nodeTree[labelId].getComponent::<Text>().expect("text").getText(), "-120");
		assert_eq!(nodeTree[floatingTextId].getComponent::<FloatingText>().expect("floating text").getActiveCount(), 1);

		nodeTree.tick(&engine, 0.5);
		let labelPosition = nodeTree[labelId].getLocalPosition();
		assert_eq!(labelPosition.x, 100.0);
		assert!((labelPosition.y - (200.0 - 40.0 * 0.75)).abs() < 0.001);
		assert!((nodeTree[labelId].getLocalOpacity() - 0.75).abs() < 0.001);

		nodeTree.tick(&engine, 0.5);
		assert!(!nodeTree[labelId].isActive());
		assert_eq!(nodeTree[floatingTextId].getComponent::<FloatingText>().expect("floating text").getActiveCount(), 0);

		let reusedId = FloatingText::spawn(&mut nodeTree, floatingTextId, "+5", Vector2::create(0.0, 0.0), options).expect("label");
		assert_eq!(reusedId, labelId);
		assert!(nodeTree[reusedId].isActive());
		assert_eq!(nodeTree.getChildCount(floatingTextId), 1);

		FloatingText::clear(&mut nodeTree, floatingTextId);
		assert!(!nodeTree[reusedId].isActive());
		assert_eq!(nodeTree[floatingTextId].getComponent::<FloatingText>().expect("floating text").getActiveCount(), 0);
	}
}
