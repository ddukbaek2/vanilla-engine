//==============================================================================
// 회전 로딩 표시. (원본 ui/uispinner.js 의 UISpinner)
// - 원 둘레에 점 여러 개를 놓고, 밝기가 꼬리처럼 돌아가는 표준 스피너. (점 투명도 0.15 ~ 1)
// - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//   호스트를 꾸미고(가운데 피봇, 크기 = (반지름 + 점 반지름) x 2) 점 노드(둥근 Paint)를 자식으로 만든다.
//   UISpinner::create(nodeTree, parentId, options) 가 원본 new UISpinner(options) + addChild 자리다.
// - 사용:
//     let spinnerId = UISpinner::create(nodeTree, panelId, UISpinnerOptions { radius: 9.0, dotCount: 10, ..UISpinnerOptions::default() });
//     nodeTree[spinnerId].setLocalPosition(Vector2::create(x, y));
// - 원본과 다른 것:
//   - setDotColor 는 색을 정해 두고 다음 tick 에 점 노드에 건다. (점 노드는 씬 코드가 이 위젯을 쥔 동안 닿을 수 없다)
//   - 위상(phase)은 0 ~ 1 로 접어 둔다. 꼬리 계산이 1 주기라 보이는 결과는 같고, 오래 돌아도 f32 정밀도가 떨어지지 않는다.
//   - 점 노드 이름은 원본처럼 "SpinnerDot{번호}" 다. options 객체는 UISpinnerOptions 구조체다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};

//==============================================================================
// 스피너 설정. (원본 options = { radius = 9, dotCount = 10, dotRadius = 2, color?, rotationSpeed = 1 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UISpinnerOptions {
	// 점이 놓이는 원의 반지름.
	pub radius: f32,
	pub dotCount: usize,
	pub dotRadius: f32,
	// 점 색. (기본 흰색)
	pub color: Color,
	// 초당 바퀴 수.
	pub rotationSpeed: f32,
}

impl Default for UISpinnerOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UISpinnerOptions {
		UISpinnerOptions {
			radius: 9.0,
			dotCount: 10,
			dotRadius: 2.0,
			color: Color::white(),
			rotationSpeed: 1.0,
		}
	}
}

//==============================================================================
// 회전 로딩 표시.
//==============================================================================
pub struct UISpinner {
	componentBase: ComponentBase,
	options: UISpinnerOptions,
	dotNodeIds: Vec<NodeId>,
	rotationSpeed: f32,
	phase: f32,
	dotColor: Color,
	isDotColorPending: bool,
}

impl UISpinner {
	//==============================================================================
	// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UISpinner {
		UISpinner::createWithOptions(UISpinnerOptions::default())
	}

	//==============================================================================
	// 설정으로 생성. (노드에는 nodeTree.addComponent 로 붙인다)
	//==============================================================================
	pub fn createWithOptions(options: UISpinnerOptions) -> UISpinner {
		UISpinner {
			componentBase: ComponentBase::new(),
			options,
			dotNodeIds: Vec::new(),
			rotationSpeed: options.rotationSpeed,
			phase: 0.0,
			dotColor: options.color,
			isDotColorPending: false,
		}
	}

	//==============================================================================
	// 노드 만들기. (parentId 밑에 "UISpinner" 노드를 만들어 붙인다. 원본 new UISpinner(options) + addChild)
	//==============================================================================
	pub fn create(nodeTree: &mut NodeTree, parentId: NodeId, options: UISpinnerOptions) -> NodeId {
		let spinnerId = nodeTree.createChild(parentId, "UISpinner");
		nodeTree.addComponent(spinnerId, UISpinner::createWithOptions(options));
		spinnerId
	}

	//==============================================================================
	// 점 투명도 계산. (점 순서 비율에서 위상을 뺀 꼬리 비율이 작을수록 밝다)
	//==============================================================================
	pub fn calculateDotOpacity(&self, dotIndex: usize) -> f32 {
		let orderRatio = dotIndex as f32 / self.options.dotCount as f32;
		let mut tailRatio = (orderRatio - self.phase) % 1.0;
		if tailRatio < 0.0 {
			tailRatio += 1.0;
		}
		0.15 + 0.85 * (1.0 - tailRatio)
	}

	//==============================================================================
	// 점 색 설정. (다음 tick 에 점 노드에 건다)
	//==============================================================================
	pub fn setDotColor(&mut self, dotColor: impl Into<Color>) {
		self.dotColor = dotColor.into();
		self.isDotColorPending = true;
	}

	//==============================================================================
	// 점 색 반환.
	//==============================================================================
	pub fn getDotColor(&self) -> Color {
		self.dotColor
	}

	//==============================================================================
	// 회전 속도 설정. (초당 바퀴 수)
	//==============================================================================
	pub fn setRotationSpeed(&mut self, rotationSpeed: f32) {
		self.rotationSpeed = rotationSpeed;
	}

	//==============================================================================
	// 회전 속도 반환.
	//==============================================================================
	pub fn getRotationSpeed(&self) -> f32 {
		self.rotationSpeed
	}

	//==============================================================================
	// 점 노드 핸들 목록.
	//==============================================================================
	pub fn getDotNodes(&self) -> &[NodeId] {
		&self.dotNodeIds
	}
}

impl Default for UISpinner {
	fn default() -> UISpinner {
		UISpinner::new()
	}
}

impl Component for UISpinner {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UISpinner"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 가운데 피봇, 크기, 둘레의 점 노드)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		let options = self.options;
		let center = options.radius + options.dotRadius;
		let spinnerNode = &mut nodeTree[nodeId];
		spinnerNode.setPivot(Pivot::middleCenter);
		spinnerNode.setAnchor(Pivot::topLeft);
		spinnerNode.setContentSize(Vector2::create(center * 2.0, center * 2.0));
		for dotIndex in 0..options.dotCount {
			let angle = (dotIndex as f32 / options.dotCount as f32) * math::PI * 2.0;
			let dotName = format!("SpinnerDot{}", dotIndex);
			let dotId = nodeTree.createChild(nodeId, &dotName);
			let dotNode = &mut nodeTree[dotId];
			dotNode.setPivot(Pivot::middleCenter);
			dotNode.setAnchor(Pivot::topLeft);
			dotNode.setContentSize(Vector2::create(options.dotRadius * 2.0, options.dotRadius * 2.0));
			dotNode.setLocalPosition(Vector2::create(center + angle.cos() * options.radius, center + angle.sin() * options.radius));
			let paint = dotNode.addComponent(Paint::createWithColor(self.dotColor));
			paint.setRoundSize(options.dotRadius);
			self.dotNodeIds.push(dotId);
		}
	}

	//==============================================================================
	// 갱신. (위상을 돌리고 점마다 투명도를 건다. 미뤄 둔 점 색도 건다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		self.phase += timeDelta * self.rotationSpeed;
		self.phase -= self.phase.floor();
		let isDotColorPending = self.isDotColorPending;
		self.isDotColorPending = false;
		let nodeTree = context.getNodeTreeMut();
		for (dotIndex, dotNodeId) in self.dotNodeIds.iter().copied().enumerate() {
			let dotOpacity = self.calculateDotOpacity(dotIndex);
			let dotNode = match nodeTree.getNodeMut(dotNodeId) {
				Some(dotNode) => {
					dotNode
				}
				None => {
					continue;
				}
			};
			dotNode.setLocalOpacity(dotOpacity);
			if isDotColorPending {
				if let Some(paint) = dotNode.getComponentMut::<Paint>() {
					paint.setColor(self.dotColor);
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};

	//==============================================================================
	// 점은 원 둘레에 놓이고, 꼬리 밝기가 위상을 따라 돈다. 점 색은 다음 tick 에 걸린다.
	//==============================================================================
	#[test]
	fn dotsCircleAndTailRotates() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let options = UISpinnerOptions {
			dotCount: 4,
			..UISpinnerOptions::default()
		};
		let spinnerId = UISpinner::create(&mut nodeTree, rootId, options);
		assert_eq!(nodeTree[spinnerId].getContentSize(), Vector2::create(22.0, 22.0));
		let dotIds = nodeTree[spinnerId].getComponent::<UISpinner>().expect("spinner").getDotNodes().to_vec();
		assert_eq!(dotIds.len(), 4);
		assert_eq!(nodeTree[dotIds[0]].getLocalPosition(), Vector2::create(20.0, 11.0));
		assert_eq!(nodeTree[dotIds[0]].getContentSize(), Vector2::create(4.0, 4.0));
		assert_eq!(nodeTree[dotIds[1]].getName(), "SpinnerDot1");

		nodeTree.tick(&engine, 0.25);
		let expectedOpacities = [0.3625, 1.0, 0.7875, 0.575];
		for (dotIndex, dotId) in dotIds.iter().enumerate() {
			assert!((nodeTree[*dotId].getLocalOpacity() - expectedOpacities[dotIndex]).abs() < 0.0001);
		}

		// 한 바퀴를 넘겨도 같은 위상이면 같은 밝기다.
		nodeTree.tick(&engine, 1.0);
		assert!((nodeTree[dotIds[1]].getLocalOpacity() - 1.0).abs() < 0.0001);

		nodeTree[spinnerId].getComponentMut::<UISpinner>().expect("spinner").setDotColor("#ff0000");
		assert_eq!(nodeTree[dotIds[2]].getComponent::<Paint>().expect("paint").getColor(), Color::white());
		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[dotIds[2]].getComponent::<Paint>().expect("paint").getColor(), Color::create(1.0, 0.0, 0.0, 1.0));
	}
}
