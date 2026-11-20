//==============================================================================
// 토스트. (원본 ui/uitoast.js 의 UIToast)
// - show(message) 로 알약 모양 알림이 아래에서 떠올라(0.24 초) 잠시 머문 뒤(기본 1.8 초) 위로 빠지며 사라진다(0.2 초).
// - 여러 번 부르면 줄을 세워 하나씩 보여 준다.
// - 씬 뿌리의 맨 위(마지막 자식)에 두고 쓴다. 가로 자리를 정하지 않으면 부모 가로 가운데에 선다. (부모 크기를 모르면 480)
// - 원본은 WorldNode 를 상속한 노드이고 노드 자신이 알약이다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//   호스트를 꾸미고(가운데 피봇, 크기) Paint(둥근 알약)와 UILabel 을 호스트에 붙인다. UIToast::create(node_tree, parent_id, options) 가 new + add_child 자리다.
// - 사용:
//     let toast_id = UIToast::create(node_tree, root_id, UIToastOptions::default());
//     node_tree[toast_id].get_component_mut::<UIToast>().unwrap().show("저장되었습니다.");
// - 원본과 다른 것:
//   - show 는 줄에 넣기만 하고, 꺼내서 보이는 것은 다음 tick 이 한다. (같은 tick 에 첫 걸음도 가므로 시간 흐름은 원본과 같다)
//     그래서 원본은 숨을 때 set_active(false) 하지만 러스트 판은 tick 이 돌아야 해서 set_visible(false) 로 숨긴다.
//   - 등장 / 머묾 / 퇴장 시간과 곡선은 원본 그대로 이 안에서 센다. (Tween 은 쓰지 않는다)
//   - 가로 자리 rest_x 는 Option<f32> 다. (원본 null = 부모 가운데) options 객체는 UIToastOptions 구조체다.
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
// 토스트 설정. (원본 options = { width = 400, height = 48, rest_y = 560, rest_x = null, hold_seconds = 1.8, font_size = 15, background_color?, text_color? })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIToastOptions {
	pub width: f32,
	pub height: f32,
	// 머무는 자리의 가운데 세로 좌표.
	pub rest_y: f32,
	// 머무는 자리의 가운데 가로 좌표. (None 이면 부모 가로 가운데)
	pub rest_x: Option<f32>,
	// 머무는 시간. (초)
	pub hold_seconds: f32,
	pub font_size: f32,
	pub background_color: Color,
	pub text_color: Color,
}

impl Default for UIToastOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIToastOptions {
		UIToastOptions {
			width: 400.0,
			height: 48.0,
			rest_y: 560.0,
			rest_x: None,
			hold_seconds: 1.8,
			font_size: 15.0,
			background_color: Color::create(0.09, 0.1, 0.13, 0.92),
			text_color: Color::create(0.96, 0.97, 1.0, 1.0),
		}
	}
}

//==============================================================================
// 토스트 단계. (원본 "hidden" | "enter" | "hold" | "exit")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToastPhase {
	Hidden,
	Enter,
	Hold,
	Exit,
}

//==============================================================================
// 토스트.
//==============================================================================
pub struct UIToast {
	component_base: ComponentBase,
	options: UIToastOptions,
	message_queue: VecDeque<String>,
	phase: ToastPhase,
	phase_seconds: f32,
	hold_seconds: f32,
	rest_y: f32,
	rest_x: Option<f32>,
}

impl UIToast {
	//==============================================================================
	// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UIToast {
		UIToast::create_with_options(UIToastOptions::default())
	}

	//==============================================================================
	// 설정으로 생성. (노드에는 node_tree.add_component 로 붙인다)
	//==============================================================================
	pub fn create_with_options(options: UIToastOptions) -> UIToast {
		UIToast {
			component_base: ComponentBase::new(),
			options,
			message_queue: VecDeque::new(),
			phase: ToastPhase::Hidden,
			phase_seconds: 0.0,
			hold_seconds: options.hold_seconds,
			rest_y: options.rest_y,
			rest_x: options.rest_x,
		}
	}

	//==============================================================================
	// 노드 만들기. (parent_id 밑에 "UIToast" 노드를 만들어 붙인다. 원본 new UIToast(options) + add_child)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, parent_id: NodeId, options: UIToastOptions) -> NodeId {
		let toast_id = node_tree.create_child(parent_id, "UIToast");
		node_tree.add_component(toast_id, UIToast::create_with_options(options));
		toast_id
	}

	//==============================================================================
	// 토스트 표시. (줄에 넣는다. 보이는 중이면 차례를 기다린다)
	//==============================================================================
	pub fn show(&mut self, message_text: &str) {
		self.message_queue.push_back(message_text.to_string());
	}

	//==============================================================================
	// 다음 메시지 꺼내 보이기. (줄이 비었으면 숨긴다)
	//==============================================================================
	fn present_next(&mut self, context: &mut ComponentContext) {
		let message_text = match self.message_queue.pop_front() {
			Some(message_text) => {
				message_text
			}
			None => {
				self.phase = ToastPhase::Hidden;
				context.get_node_mut().set_visible(false);
				return;
			}
		};
		if let Some(label) = context.get_node_mut().get_component_mut::<UILabel>() {
			label.set_text(&message_text);
		}
		self.phase = ToastPhase::Enter;
		self.phase_seconds = 0.0;
		let toast_node = context.get_node_mut();
		toast_node.set_visible(true);
		toast_node.set_local_opacity(0.0);
		self.update_position(context, 0.0);
	}

	//==============================================================================
	// 진행 비율로 자리 고치기. (0 = 아래에서 출발, 1 = 제자리)
	//==============================================================================
	fn update_position(&self, context: &mut ComponentContext, progress_ratio: f32) {
		let center_x = match self.rest_x {
			Some(rest_x) => {
				rest_x
			}
			None => {
				let node_id = context.get_node_id();
				let node_tree = context.get_node_tree();
				let parent_width = node_tree.get_parent_id(node_id).and_then(|parent_id| node_tree.get_node(parent_id)).map(|parent_node| parent_node.get_content_size().x).unwrap_or(0.0);
				if parent_width > 0.0 {
					parent_width * 0.5
				}
				else {
					DEFAULT_CENTER_X
				}
			}
		};
		let position_y = self.rest_y + RISE_DISTANCE * (1.0 - progress_ratio);
		context.get_node_mut().set_local_position(Vector2::create(center_x, position_y));
	}

	//==============================================================================
	// 머무는 자리(가운데 세로 좌표) 설정.
	//==============================================================================
	pub fn set_rest_y(&mut self, rest_y: f32) {
		self.rest_y = rest_y;
	}

	//==============================================================================
	// 머무는 자리(가운데 가로 좌표) 설정. (None 이면 부모 가로 가운데)
	//==============================================================================
	pub fn set_rest_x(&mut self, rest_x: Option<f32>) {
		self.rest_x = rest_x;
	}

	//==============================================================================
	// 머무는 시간 설정. (초)
	//==============================================================================
	pub fn set_hold_seconds(&mut self, hold_seconds: f32) {
		self.hold_seconds = hold_seconds;
	}

	//==============================================================================
	// 보이는 중인지. (줄에 남은 메시지가 있어도 참)
	//==============================================================================
	pub fn is_showing(&self) -> bool {
		self.phase != ToastPhase::Hidden || !self.message_queue.is_empty()
	}
}

impl Default for UIToast {
	fn default() -> UIToast {
		UIToast::new()
	}
}

impl Component for UIToast {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UIToast"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 가운데 피봇, 크기, 숨김, 알약 Paint, UILabel)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		let options = self.options;
		let toast_node = &mut node_tree[node_id];
		toast_node.set_pivot(Pivot::MIDDLE_CENTER);
		toast_node.set_anchor(Pivot::TOP_LEFT);
		toast_node.set_content_size(Vector2::create(options.width, options.height));
		toast_node.set_visible(false);
		let paint = toast_node.add_component(Paint::create_with_color(options.background_color));
		paint.set_round_size(options.height * 0.5);
		if let Some(label) = node_tree.add_component(node_id, UILabel::new()) {
			label.set_text("");
			label.set_font_size(options.font_size);
			label.set_text_color(options.text_color);
		}
	}

	//==============================================================================
	// 갱신. (숨은 동안 줄에 메시지가 있으면 꺼내고, 등장 → 머묾 → 퇴장을 센다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		if self.phase == ToastPhase::Hidden {
			if self.message_queue.is_empty() {
				return;
			}
			self.present_next(context);
		}
		self.phase_seconds += time_delta;
		match self.phase {
			ToastPhase::Enter => {
				let ratio = (self.phase_seconds / ENTER_SECONDS).min(1.0);
				let eased = 1.0 - (1.0 - ratio) * (1.0 - ratio);
				context.get_node_mut().set_local_opacity(eased);
				self.update_position(context, eased);
				if ratio >= 1.0 {
					self.phase = ToastPhase::Hold;
					self.phase_seconds = 0.0;
				}
			}
			ToastPhase::Hold => {
				if self.phase_seconds >= self.hold_seconds {
					self.phase = ToastPhase::Exit;
					self.phase_seconds = 0.0;
				}
			}
			ToastPhase::Exit => {
				let ratio = (self.phase_seconds / EXIT_SECONDS).min(1.0);
				context.get_node_mut().set_local_opacity(1.0 - ratio);
				self.update_position(context, 1.0 + ratio * 0.4);
				if ratio >= 1.0 {
					self.present_next(context);
				}
			}
			ToastPhase::Hidden => {
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
	fn create_toast_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
		let toast_id = UIToast::create(&mut node_tree, root_id, UIToastOptions::default());
		(node_tree, toast_id)
	}

	//==============================================================================
	// 토스트 반환.
	//==============================================================================
	fn get_toast(node_tree: &mut NodeTree, toast_id: NodeId) -> &mut UIToast {
		node_tree[toast_id].get_component_mut::<UIToast>().expect("toast")
	}

	//==============================================================================
	// 두 메시지가 차례로: 등장 0.24 → 머묾 1.8 → 퇴장 0.2 → 다음 메시지 → 끝나면 숨는다.
	//==============================================================================
	#[test]
	fn queued_messages_play_in_order() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, toast_id) = create_toast_tree();
		assert!(!node_tree[toast_id].is_visible());
		assert_eq!(node_tree[toast_id].get_content_size(), Vector2::create(400.0, 48.0));
		get_toast(&mut node_tree, toast_id).show("첫째");
		get_toast(&mut node_tree, toast_id).show("둘째");
		assert!(get_toast(&mut node_tree, toast_id).is_showing());

		// 등장 절반: 0.12 초 → ratio 0.5, eased 0.75.
		node_tree.tick(&engine, 0.12);
		assert!(node_tree[toast_id].is_visible());
		assert_eq!(node_tree[toast_id].get_component::<UILabel>().expect("label").get_text(), "첫째");
		assert!((node_tree[toast_id].get_local_opacity() - 0.75).abs() < 0.0001);
		let half_position = node_tree[toast_id].get_local_position();
		assert_eq!(half_position.x, 400.0);
		assert!((half_position.y - (560.0 + 28.0 * 0.25)).abs() < 0.0001);
		node_tree.tick(&engine, 0.12);
		assert_eq!(node_tree[toast_id].get_local_opacity(), 1.0);
		assert_eq!(node_tree[toast_id].get_local_position(), Vector2::create(400.0, 560.0));

		// 머묾.
		node_tree.tick(&engine, 1.0);
		assert_eq!(node_tree[toast_id].get_local_opacity(), 1.0);
		node_tree.tick(&engine, 0.8);

		// 퇴장 절반: 투명도 0.5, 위로 0.2 만큼.
		node_tree.tick(&engine, 0.1);
		assert!((node_tree[toast_id].get_local_opacity() - 0.5).abs() < 0.0001);
		assert!((node_tree[toast_id].get_local_position().y - (560.0 - 28.0 * 0.2)).abs() < 0.0001);
		node_tree.tick(&engine, 0.1);
		assert_eq!(node_tree[toast_id].get_component::<UILabel>().expect("label").get_text(), "둘째");
		assert_eq!(node_tree[toast_id].get_local_opacity(), 0.0);

		// 둘째도 끝나면 숨는다.
		node_tree.tick(&engine, 0.24);
		node_tree.tick(&engine, 1.8);
		node_tree.tick(&engine, 0.2);
		assert!(!node_tree[toast_id].is_visible());
		assert!(!get_toast(&mut node_tree, toast_id).is_showing());
	}

	//==============================================================================
	// 가로 자리를 정하면 그 자리에 선다.
	//==============================================================================
	#[test]
	fn rest_xoverrides_parent_center() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, toast_id) = create_toast_tree();
		{
			let toast = get_toast(&mut node_tree, toast_id);
			toast.set_rest_x(Some(250.0));
			toast.set_rest_y(100.0);
			toast.show("자리");
		}
		node_tree.tick(&engine, 0.24);
		assert_eq!(node_tree[toast_id].get_local_position(), Vector2::create(250.0, 100.0));
	}
}
