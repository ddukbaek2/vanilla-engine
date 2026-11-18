//==============================================================================
// 토스트. (원본 ui/uitoast.js 의 UIToast)
// - show(message) 로 알약 모양 알림이 아래에서 떠올라(0.24 초) 잠시 머문 뒤(기본 1.8 초) 위로 빠지며 사라진다(0.2 초).
// - 여러 번 부르면 줄을 세워 하나씩 보여 준다.
// - 씬 뿌리의 맨 위(마지막 자식)에 두고 쓴다. 가로 자리를 정하지 않으면 부모 가로 가운데에 선다. (부모 크기를 모르면 480)
// - 원본은 WorldNode 를 상속한 노드이고 노드 자신이 알약이다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//   호스트를 꾸미고(가운데 피봇, 크기) Paint(둥근 알약)와 UILabel 을 호스트에 붙인다. UIToast::create(nodeTree, parentId, options) 가 new + addChild 자리다.
// - 사용:
//     let toastId = UIToast::create(nodeTree, rootId, UIToastOptions::default());
//     nodeTree[toastId].getComponentMut::<UIToast>().unwrap().show("저장되었습니다.");
// - 원본과 다른 것:
//   - show 는 줄에 넣기만 하고, 꺼내서 보이는 것은 다음 tick 이 한다. (같은 tick 에 첫 걸음도 가므로 시간 흐름은 원본과 같다)
//     그래서 원본은 숨을 때 setActive(false) 하지만 러스트 판은 tick 이 돌아야 해서 setVisible(false) 로 숨긴다.
//   - 등장 / 머묾 / 퇴장 시간과 곡선은 원본 그대로 이 안에서 센다. (Tween 은 쓰지 않는다)
//   - 가로 자리 restX 는 Option<f32> 다. (원본 null = 부모 가운데) options 객체는 UIToastOptions 구조체다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uilabel::UILabel;
use std::collections::VecDeque;

// 등장 / 퇴장 시간. (초, 원본과 같다)
const ENTER_SECONDS: f32 = 0.24;
const EXIT_SECONDS: f32 = 0.2;

// 떠오르는 거리. (픽셀, 원본과 같다)
const RISE_DISTANCE: f32 = 28.0;

// 부모 크기를 모를 때의 가운데. (원본과 같다)
const DEFAULT_CENTER_X: f32 = 480.0;

//==============================================================================
// 토스트 설정. (원본 options = { width = 400, height = 48, restY = 560, restX = null, holdSeconds = 1.8, fontSize = 15, backgroundColor?, textColor? })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIToastOptions {
	pub width: f32,
	pub height: f32,
	// 머무는 자리의 가운데 세로 좌표.
	pub restY: f32,
	// 머무는 자리의 가운데 가로 좌표. (None 이면 부모 가로 가운데)
	pub restX: Option<f32>,
	// 머무는 시간. (초)
	pub holdSeconds: f32,
	pub fontSize: f32,
	pub backgroundColor: Color,
	pub textColor: Color,
}

impl Default for UIToastOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIToastOptions {
		UIToastOptions {
			width: 400.0,
			height: 48.0,
			restY: 560.0,
			restX: None,
			holdSeconds: 1.8,
			fontSize: 15.0,
			backgroundColor: Color::create(0.09, 0.1, 0.13, 0.92),
			textColor: Color::create(0.96, 0.97, 1.0, 1.0),
		}
	}
}

//==============================================================================
// 토스트 단계. (원본 "hidden" | "enter" | "hold" | "exit")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToastPhase {
	hidden,
	enter,
	hold,
	exit,
}

//==============================================================================
// 토스트.
//==============================================================================
pub struct UIToast {
	componentBase: ComponentBase,
	options: UIToastOptions,
	messageQueue: VecDeque<String>,
	phase: ToastPhase,
	phaseSeconds: f32,
	holdSeconds: f32,
	restY: f32,
	restX: Option<f32>,
}

impl UIToast {
	//==============================================================================
	// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UIToast {
		UIToast::createWithOptions(UIToastOptions::default())
	}

	//==============================================================================
	// 설정으로 생성. (노드에는 nodeTree.addComponent 로 붙인다)
	//==============================================================================
	pub fn createWithOptions(options: UIToastOptions) -> UIToast {
		UIToast {
			componentBase: ComponentBase::new(),
			options,
			messageQueue: VecDeque::new(),
			phase: ToastPhase::hidden,
			phaseSeconds: 0.0,
			holdSeconds: options.holdSeconds,
			restY: options.restY,
			restX: options.restX,
		}
	}

	//==============================================================================
	// 노드 만들기. (parentId 밑에 "UIToast" 노드를 만들어 붙인다. 원본 new UIToast(options) + addChild)
	//==============================================================================
	pub fn create(nodeTree: &mut NodeTree, parentId: NodeId, options: UIToastOptions) -> NodeId {
		let toastId = nodeTree.createChild(parentId, "UIToast");
		nodeTree.addComponent(toastId, UIToast::createWithOptions(options));
		toastId
	}

	//==============================================================================
	// 토스트 표시. (줄에 넣는다. 보이는 중이면 차례를 기다린다)
	//==============================================================================
	pub fn show(&mut self, messageText: &str) {
		self.messageQueue.push_back(messageText.to_string());
	}

	//==============================================================================
	// 다음 메시지 꺼내 보이기. (줄이 비었으면 숨긴다)
	//==============================================================================
	fn presentNext(&mut self, context: &mut ComponentContext) {
		let messageText = match self.messageQueue.pop_front() {
			Some(messageText) => {
				messageText
			}
			None => {
				self.phase = ToastPhase::hidden;
				context.getNodeMut().setVisible(false);
				return;
			}
		};
		if let Some(label) = context.getNodeMut().getComponentMut::<UILabel>() {
			label.setText(&messageText);
		}
		self.phase = ToastPhase::enter;
		self.phaseSeconds = 0.0;
		let toastNode = context.getNodeMut();
		toastNode.setVisible(true);
		toastNode.setLocalOpacity(0.0);
		self.updatePosition(context, 0.0);
	}

	//==============================================================================
	// 진행 비율로 자리 고치기. (0 = 아래에서 출발, 1 = 제자리)
	//==============================================================================
	fn updatePosition(&self, context: &mut ComponentContext, progressRatio: f32) {
		let centerX = match self.restX {
			Some(restX) => {
				restX
			}
			None => {
				let nodeId = context.getNodeId();
				let nodeTree = context.getNodeTree();
				let parentWidth = nodeTree.getParentId(nodeId).and_then(|parentId| nodeTree.getNode(parentId)).map(|parentNode| parentNode.getContentSize().x).unwrap_or(0.0);
				if parentWidth > 0.0 {
					parentWidth * 0.5
				}
				else {
					DEFAULT_CENTER_X
				}
			}
		};
		let positionY = self.restY + RISE_DISTANCE * (1.0 - progressRatio);
		context.getNodeMut().setLocalPosition(Vector2::create(centerX, positionY));
	}

	//==============================================================================
	// 머무는 자리(가운데 세로 좌표) 설정.
	//==============================================================================
	pub fn setRestY(&mut self, restY: f32) {
		self.restY = restY;
	}

	//==============================================================================
	// 머무는 자리(가운데 가로 좌표) 설정. (None 이면 부모 가로 가운데)
	//==============================================================================
	pub fn setRestX(&mut self, restX: Option<f32>) {
		self.restX = restX;
	}

	//==============================================================================
	// 머무는 시간 설정. (초)
	//==============================================================================
	pub fn setHoldSeconds(&mut self, holdSeconds: f32) {
		self.holdSeconds = holdSeconds;
	}

	//==============================================================================
	// 보이는 중인지. (줄에 남은 메시지가 있어도 참)
	//==============================================================================
	pub fn isShowing(&self) -> bool {
		self.phase != ToastPhase::hidden || !self.messageQueue.is_empty()
	}
}

impl Default for UIToast {
	fn default() -> UIToast {
		UIToast::new()
	}
}

impl Component for UIToast {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIToast"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 가운데 피봇, 크기, 숨김, 알약 Paint, UILabel)
	//==============================================================================
	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		let options = self.options;
		let toastNode = &mut nodeTree[nodeId];
		toastNode.setPivot(Pivot::middleCenter);
		toastNode.setAnchor(Pivot::topLeft);
		toastNode.setContentSize(Vector2::create(options.width, options.height));
		toastNode.setVisible(false);
		let paint = toastNode.addComponent(Paint::createWithColor(options.backgroundColor));
		paint.setRoundSize(options.height * 0.5);
		if let Some(label) = nodeTree.addComponent(nodeId, UILabel::new()) {
			label.setText("");
			label.setFontSize(options.fontSize);
			label.setTextColor(options.textColor);
		}
	}

	//==============================================================================
	// 갱신. (숨은 동안 줄에 메시지가 있으면 꺼내고, 등장 → 머묾 → 퇴장을 센다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		if self.phase == ToastPhase::hidden {
			if self.messageQueue.is_empty() {
				return;
			}
			self.presentNext(context);
		}
		self.phaseSeconds += timeDelta;
		match self.phase {
			ToastPhase::enter => {
				let ratio = (self.phaseSeconds / ENTER_SECONDS).min(1.0);
				let eased = 1.0 - (1.0 - ratio) * (1.0 - ratio);
				context.getNodeMut().setLocalOpacity(eased);
				self.updatePosition(context, eased);
				if ratio >= 1.0 {
					self.phase = ToastPhase::hold;
					self.phaseSeconds = 0.0;
				}
			}
			ToastPhase::hold => {
				if self.phaseSeconds >= self.holdSeconds {
					self.phase = ToastPhase::exit;
					self.phaseSeconds = 0.0;
				}
			}
			ToastPhase::exit => {
				let ratio = (self.phaseSeconds / EXIT_SECONDS).min(1.0);
				context.getNodeMut().setLocalOpacity(1.0 - ratio);
				self.updatePosition(context, 1.0 + ratio * 0.4);
				if ratio >= 1.0 {
					self.presentNext(context);
				}
			}
			ToastPhase::hidden => {
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};

	//==============================================================================
	// 시험 장면. (800 x 600 뿌리 밑의 토스트)
	//==============================================================================
	fn createToastTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		nodeTree[rootId].setContentSize(Vector2::create(800.0, 600.0));
		let toastId = UIToast::create(&mut nodeTree, rootId, UIToastOptions::default());
		(nodeTree, toastId)
	}

	//==============================================================================
	// 토스트 반환.
	//==============================================================================
	fn getToast(nodeTree: &mut NodeTree, toastId: NodeId) -> &mut UIToast {
		nodeTree[toastId].getComponentMut::<UIToast>().expect("toast")
	}

	//==============================================================================
	// 두 메시지가 차례로: 등장 0.24 → 머묾 1.8 → 퇴장 0.2 → 다음 메시지 → 끝나면 숨는다.
	//==============================================================================
	#[test]
	fn queuedMessagesPlayInOrder() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, toastId) = createToastTree();
		assert!(!nodeTree[toastId].isVisible());
		assert_eq!(nodeTree[toastId].getContentSize(), Vector2::create(400.0, 48.0));
		getToast(&mut nodeTree, toastId).show("첫째");
		getToast(&mut nodeTree, toastId).show("둘째");
		assert!(getToast(&mut nodeTree, toastId).isShowing());

		// 등장 절반: 0.12 초 → ratio 0.5, eased 0.75.
		nodeTree.tick(&engine, 0.12);
		assert!(nodeTree[toastId].isVisible());
		assert_eq!(nodeTree[toastId].getComponent::<UILabel>().expect("label").getText(), "첫째");
		assert!((nodeTree[toastId].getLocalOpacity() - 0.75).abs() < 0.0001);
		let halfPosition = nodeTree[toastId].getLocalPosition();
		assert_eq!(halfPosition.x, 400.0);
		assert!((halfPosition.y - (560.0 + 28.0 * 0.25)).abs() < 0.0001);
		nodeTree.tick(&engine, 0.12);
		assert_eq!(nodeTree[toastId].getLocalOpacity(), 1.0);
		assert_eq!(nodeTree[toastId].getLocalPosition(), Vector2::create(400.0, 560.0));

		// 머묾.
		nodeTree.tick(&engine, 1.0);
		assert_eq!(nodeTree[toastId].getLocalOpacity(), 1.0);
		nodeTree.tick(&engine, 0.8);

		// 퇴장 절반: 투명도 0.5, 위로 0.2 만큼.
		nodeTree.tick(&engine, 0.1);
		assert!((nodeTree[toastId].getLocalOpacity() - 0.5).abs() < 0.0001);
		assert!((nodeTree[toastId].getLocalPosition().y - (560.0 - 28.0 * 0.2)).abs() < 0.0001);
		nodeTree.tick(&engine, 0.1);
		assert_eq!(nodeTree[toastId].getComponent::<UILabel>().expect("label").getText(), "둘째");
		assert_eq!(nodeTree[toastId].getLocalOpacity(), 0.0);

		// 둘째도 끝나면 숨는다.
		nodeTree.tick(&engine, 0.24);
		nodeTree.tick(&engine, 1.8);
		nodeTree.tick(&engine, 0.2);
		assert!(!nodeTree[toastId].isVisible());
		assert!(!getToast(&mut nodeTree, toastId).isShowing());
	}

	//==============================================================================
	// 가로 자리를 정하면 그 자리에 선다.
	//==============================================================================
	#[test]
	fn restXOverridesParentCenter() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, toastId) = createToastTree();
		{
			let toast = getToast(&mut nodeTree, toastId);
			toast.setRestX(Some(250.0));
			toast.setRestY(100.0);
			toast.show("자리");
		}
		nodeTree.tick(&engine, 0.24);
		assert_eq!(nodeTree[toastId].getLocalPosition(), Vector2::create(250.0, 100.0));
	}
}
