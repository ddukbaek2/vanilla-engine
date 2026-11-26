//==============================================================================
//! 드래그 가능 컴포넌트. (원본 ui/uidraggable.js 의 UIDraggable)
//! - 노드에 붙이면 눌러서 끌 수 있다. 놓으면 끝 이벤트로 알리고, set_snap_back_enabled(true) 면 잡기 전 자리로 미끄러져 돌아간다.
//! - 이동량은 뷰 좌표 차이를 그대로 로컬 좌표에 더한다. (조상 노드에 배율이 있으면 감도가 달라진다. 원본과 같음)
//! - 원본은 UIControl 을 상속한다. 러스트 판은 UIControl 을 품고(ui_control 필드) content, 배경색을 넘겨준다.
//!   attach_to_tree 에서 호스트 노드를 interactable 로 켠다. node_tree.add_component(node_id, UIDraggable::new()) 로 붙인다.
//! - 사용:
//!   ```text
//!     let draggable = node_tree.add_component(card_id, UIDraggable::new()).expect("draggable");
//!     draggable.set_snap_back_enabled(true);
//!     draggable.set_drag_end_event(Box::new(|draggable, context, view_position| { if 놓을 자리면 { draggable.settle_here(context.get_node_tree_mut()); } }));
//!   ```
//! - 원본과 다른 것:
//!   - 이벤트는 드래그 컴포넌트와 문맥을 받는다. (이동 / 끝은 뷰 좌표도) 원본 콜백은 컴포넌트(와 뷰 좌표)만 받는다.
//!   - settle_here / snap_home 은 노드 자리를 읽고 써야 해서 트리를 받는다. (끝 이벤트 안에서는 context.get_node_tree_mut())
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uicontrol::UIControl;

//==============================================================================
/// 드래그 이벤트. (드래그 컴포넌트와 문맥을 받는다. 시작 이벤트)
//==============================================================================
pub type UIDraggableEvent = Box<dyn FnMut(&mut UIDraggable, &mut ComponentContext)>;

//==============================================================================
/// 드래그 자리 이벤트. (드래그 컴포넌트, 문맥, 뷰 좌표. 이동 / 끝 이벤트)
//==============================================================================
pub type UIDraggablePositionEvent = Box<dyn FnMut(&mut UIDraggable, &mut ComponentContext, Vector2)>;

//==============================================================================
/// 드래그 가능 컴포넌트.
///
/// # 예제
/// 놓을 자리에 놓으면 눌러앉고, 아니면 제자리로 돌아가는 카드
/// ```rust
/// use vanilla::{ComponentContext, Engine, EngineConfiguration, NodeTree, TouchKind, UIDraggable, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let card_id = node_tree.create_child(root_id, "Card");
/// node_tree[card_id].set_local_position(Vector2::create(100.0, 100.0));
/// node_tree[card_id].set_content_size(Vector2::create(50.0, 50.0));
/// let draggable = node_tree.add_component(card_id, UIDraggable::new()).expect("draggable");
/// draggable.set_snap_back_enabled(true);
/// draggable.set_drag_end_event(Box::new(|draggable: &mut UIDraggable, context: &mut ComponentContext, view_position: Vector2| {
/// 	if view_position.x > 300.0 {
/// 		draggable.settle_here(context.get_node_tree_mut());
/// 	}
/// }));
/// node_tree.dispatch_touch(card_id, &engine, TouchKind::Press, Vector2::create(100.0, 100.0));
/// node_tree.dispatch_touch(card_id, &engine, TouchKind::Moved, Vector2::create(400.0, 100.0));
/// node_tree.dispatch_touch(card_id, &engine, TouchKind::Release, Vector2::create(400.0, 100.0));
/// node_tree.tick(&engine, 0.5);
/// assert_eq!(node_tree[card_id].get_local_position(), Vector2::create(400.0, 100.0));
/// assert_eq!(node_tree[card_id].get_component::<UIDraggable>().expect("draggable").get_home_position(), Some(Vector2::create(400.0, 100.0)));
/// ```
//==============================================================================
pub struct UIDraggable {
	ui_control: UIControl,
	node_id: Option<NodeId>,
	is_dragging: bool,
	previous_view_position: Option<Vector2>,
	home_position: Option<Vector2>,
	is_snap_back_enabled: bool,
	is_returning: bool,
	snap_back_speed: f32,
	drag_start_event: Option<UIDraggableEvent>,
	drag_move_event: Option<UIDraggablePositionEvent>,
	drag_end_event: Option<UIDraggablePositionEvent>,
}

#[crate::reflect(base = ui_control)]
impl UIDraggable {
	//==============================================================================
	/// 생성. (복귀 꺼짐, 복귀 속도 14)
	///
	/// # 돌려줌
	/// 붙기 전의 드래그 컴포넌트.
	//==============================================================================
	pub fn new() -> UIDraggable {
		UIDraggable {
			ui_control: UIControl::new(),
			node_id: None,
			is_dragging: false,
			previous_view_position: None,
			home_position: None,
			is_snap_back_enabled: false,
			is_returning: false,
			snap_back_speed: 14.0,
			drag_start_event: None,
			drag_move_event: None,
			drag_end_event: None,
		}
	}

	//==============================================================================
	/// 품은 컨트롤 반환.
	///
	/// # 돌려줌
	/// 품은 UIControl.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uicontrol(&self) -> &UIControl {
		&self.ui_control
	}

	//==============================================================================
	/// 품은 컨트롤 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 UIControl.
	//==============================================================================
	pub fn get_uicontrol_mut(&mut self) -> &mut UIControl {
		&mut self.ui_control
	}

	//==============================================================================
	/// 컨텐트 노드 핸들.
	///
	/// # 돌려줌
	/// 컨텐트 노드. (트리에 붙기 전이면 None)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_control.get_content()
	}

	//==============================================================================
	/// 배경색 설정.
	///
	/// # 인자
	/// - `background_color`: 배경색. (품은 UIControl 이 그린다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.ui_control.set_background_color(background_color);
	}

	//==============================================================================
	/// 시작 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fire_drag_start_event(&mut self, context: &mut ComponentContext) {
		let mut drag_start_event = match self.drag_start_event.take() {
			Some(drag_start_event) => {
				drag_start_event
			}
			None => {
				return;
			}
		};
		drag_start_event(self, context);
		if self.drag_start_event.is_none() {
			self.drag_start_event = Some(drag_start_event);
		}
	}

	//==============================================================================
	/// 이동 이벤트 부르기.
	//==============================================================================
	fn fire_drag_move_event(&mut self, context: &mut ComponentContext, view_position: Vector2) {
		let mut drag_move_event = match self.drag_move_event.take() {
			Some(drag_move_event) => {
				drag_move_event
			}
			None => {
				return;
			}
		};
		drag_move_event(self, context, view_position);
		if self.drag_move_event.is_none() {
			self.drag_move_event = Some(drag_move_event);
		}
	}

	//==============================================================================
	/// 끝 이벤트 부르기.
	//==============================================================================
	fn fire_drag_end_event(&mut self, context: &mut ComponentContext, view_position: Vector2) {
		let mut drag_end_event = match self.drag_end_event.take() {
			Some(drag_end_event) => {
				drag_end_event
			}
			None => {
				return;
			}
		};
		drag_end_event(self, context, view_position);
		if self.drag_end_event.is_none() {
			self.drag_end_event = Some(drag_end_event);
		}
	}

	//==============================================================================
	/// 지금 자리에 눌러앉기. (끝 이벤트에서 놓기에 성공했을 때 부른다. 복귀를 멈춘다)
	///
	/// # 인자
	/// - `node_tree`: 드래그 노드가 든 노드 트리. (지금 로컬 자리를 제자리로 삼는다, 끝 이벤트 안에서는 context.get_node_tree_mut())
	//==============================================================================
	pub fn settle_here(&mut self, node_tree: &mut NodeTree) {
		if let Some(node) = self.node_id.and_then(|node_id| node_tree.get_node(node_id)) {
			self.home_position = Some(node.get_local_position());
		}
		self.is_returning = false;
	}

	//==============================================================================
	/// 제자리로 바로 되돌리기.
	///
	/// # 인자
	/// - `node_tree`: 드래그 노드가 든 노드 트리. (잡은 적이 없으면 자리는 그대로이고 복귀만 멈춘다)
	//==============================================================================
	pub fn snap_home(&mut self, node_tree: &mut NodeTree) {
		if let Some(home_position) = self.home_position {
			if let Some(node) = self.node_id.and_then(|node_id| node_tree.get_node_mut(node_id)) {
				node.set_local_position(home_position);
			}
		}
		self.is_returning = false;
	}

	//==============================================================================
	/// 놓으면 제자리로 돌아갈지 설정.
	///
	/// # 인자
	/// - `is_snap_back_enabled`: 놓으면 제자리로 미끄러져 돌아가면 true. (기본 false)
	//==============================================================================
	pub fn set_snap_back_enabled(&mut self, is_snap_back_enabled: bool) {
		self.is_snap_back_enabled = is_snap_back_enabled;
	}

	//==============================================================================
	/// 복귀 속도 설정. (초당 남은 거리를 줄이는 비율 계수)
	///
	/// # 인자
	/// - `snap_back_speed`: 복귀 속도. (tick 마다 남은 거리의 속도 x 초 몫만큼 다가가고, 그 몫이 1 을 넘으면 바로 닿는다. 기본 14)
	///
	/// # 예제
	/// 속도 10 이면 0.05 초에 남은 거리의 반을 간다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, NodeTree, TouchKind, UIDraggable, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let card_id = node_tree.create_child(root_id, "Card");
	/// node_tree[card_id].set_local_position(Vector2::create(100.0, 100.0));
	/// if let Some(draggable) = node_tree.add_component(card_id, UIDraggable::new()) {
	/// 	draggable.set_snap_back_enabled(true);
	/// 	draggable.set_snap_back_speed(10.0);
	/// }
	/// node_tree.dispatch_touch(card_id, &engine, TouchKind::Press, Vector2::create(100.0, 100.0));
	/// node_tree.dispatch_touch(card_id, &engine, TouchKind::Moved, Vector2::create(200.0, 100.0));
	/// node_tree.dispatch_touch(card_id, &engine, TouchKind::Release, Vector2::create(200.0, 100.0));
	/// node_tree.tick(&engine, 0.05);
	/// assert!((node_tree[card_id].get_local_position().x - 150.0).abs() < 0.001);
	/// ```
	//==============================================================================
	pub fn set_snap_back_speed(&mut self, snap_back_speed: f32) {
		self.snap_back_speed = snap_back_speed;
	}

	//==============================================================================
	/// 시작 이벤트 설정.
	///
	/// # 인자
	/// - `drag_start_event`: 눌러 잡을 때 부르는 이벤트. (잡기 전 자리를 기억한 뒤)
	//==============================================================================
	pub fn set_drag_start_event(&mut self, drag_start_event: UIDraggableEvent) {
		self.drag_start_event = Some(drag_start_event);
	}

	//==============================================================================
	/// 이동 이벤트 설정.
	///
	/// # 인자
	/// - `drag_move_event`: 끄는 동안 움직일 때마다 부르는 이벤트. (노드를 옮긴 뒤, 터치의 뷰 좌표를 받는다)
	//==============================================================================
	pub fn set_drag_move_event(&mut self, drag_move_event: UIDraggablePositionEvent) {
		self.drag_move_event = Some(drag_move_event);
	}

	//==============================================================================
	/// 끝 이벤트 설정.
	///
	/// # 인자
	/// - `drag_end_event`: 놓거나 터치가 취소되면 부르는 이벤트. (터치의 뷰 좌표를 받는다, 복귀를 켰으면 돌아가기 시작한 뒤라 settle_here 로 멈춘다)
	//==============================================================================
	pub fn set_drag_end_event(&mut self, drag_end_event: UIDraggablePositionEvent) {
		self.drag_end_event = Some(drag_end_event);
	}

	//==============================================================================
	/// 끄는 중인지.
	///
	/// # 돌려줌
	/// 눌러 잡은 뒤 놓기 전이면 true.
	//==============================================================================
	pub fn is_dragging(&self) -> bool {
		self.is_dragging
	}

	//==============================================================================
	/// 돌아가는 중인지.
	///
	/// # 돌려줌
	/// 제자리로 미끄러지는 중이면 true. (0.5 픽셀 안에 들면 false)
	//==============================================================================
	#[reflect(skip)]
	pub fn is_returning(&self) -> bool {
		self.is_returning
	}

	//==============================================================================
	/// 잡기 전 로컬 자리. (한 번도 잡지 않았으면 None)
	///
	/// # 돌려줌
	/// 제자리. (마지막으로 잡기 전 로컬 자리, settle_here 뒤에는 눌러앉은 자리, 잡은 적이 없으면 None)
	//==============================================================================
	pub fn get_home_position(&self) -> Option<Vector2> {
		self.home_position
	}
}

impl Default for UIDraggable {
	//==============================================================================
	/// 기본값. (UIDraggable::new 와 같다)
	//==============================================================================
	fn default() -> UIDraggable {
		UIDraggable::new()
	}
}

#[crate::reflect]
impl Component for UIDraggable {
	//==============================================================================
	/// 품은 기반 컴포넌트. (원본 상위 클래스 UIControl 자리, get_component::<UIControl>() 로도 찾힌다)
	//==============================================================================
	fn get_base_component(&self) -> Option<&dyn Component> {
		Some(&self.ui_control)
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (변경용)
	//==============================================================================
	fn get_base_component_mut(&mut self) -> Option<&mut dyn Component> {
		Some(&mut self.ui_control)
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (품은 UIControl 의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_control.get_component_base()
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_control.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 타입 이름. ("Draggable")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Draggable"
	}

	//==============================================================================
	/// 트리에 붙음. (content 노드, interactable)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_control.get_uiview_mut().create_content_node(node_tree, node_id);
		node_tree[node_id].set_interactable(true);
		self.node_id = Some(node_id);
	}

	//==============================================================================
	/// 갱신. (제자리로 미끄러지기. 0.5 픽셀 안으로 들어오면 딱 맞춘다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.node_id = Some(context.get_node_id());
		let home_position = match self.home_position {
			Some(home_position) => {
				home_position
			}
			None => {
				return;
			}
		};
		if !self.is_returning {
			return;
		}
		let node = context.get_node_mut();
		let local_position = node.get_local_position();
		let blend = (self.snap_back_speed * time_delta).min(1.0);
		let next_x = local_position.x + (home_position.x - local_position.x) * blend;
		let next_y = local_position.y + (home_position.y - local_position.y) * blend;
		node.set_local_position(Vector2::create(next_x, next_y));
		let remain_x = home_position.x - next_x;
		let remain_y = home_position.y - next_y;
		if remain_x * remain_x + remain_y * remain_y < 0.25 {
			node.set_local_position(home_position);
			self.is_returning = false;
		}
	}

	//==============================================================================
	/// 출력. (품은 UIControl 이 배경을 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.ui_control.draw(transform, graphic);
	}

	//==============================================================================
	/// 컨텐트 안에 그리는지. (true)
	//==============================================================================
	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 터치 눌림. (잡기 전 자리를 기억하고 시작 이벤트)
	//==============================================================================
	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.is_dragging = true;
		self.is_returning = false;
		self.previous_view_position = Some(view_input_position);
		self.home_position = Some(context.get_node().get_local_position());
		self.fire_drag_start_event(context);
	}

	//==============================================================================
	/// 터치 이동. (뷰 좌표 차이만큼 옮기고 이동 이벤트)
	//==============================================================================
	fn touch_move(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if !self.is_dragging {
			return;
		}
		let previous_view_position = self.previous_view_position.unwrap_or(view_input_position);
		let delta_x = view_input_position.x - previous_view_position.x;
		let delta_y = view_input_position.y - previous_view_position.y;
		self.previous_view_position = Some(view_input_position);
		let node = context.get_node_mut();
		let local_position = node.get_local_position();
		node.set_local_position(Vector2::create(local_position.x + delta_x, local_position.y + delta_y));
		self.fire_drag_move_event(context, view_input_position);
	}

	//==============================================================================
	/// 터치 뗌. (복귀를 켰으면 돌아가기 시작하고 끝 이벤트)
	//==============================================================================
	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if !self.is_dragging {
			return;
		}
		self.is_dragging = false;
		if self.is_snap_back_enabled {
			self.is_returning = true;
		}
		self.fire_drag_end_event(context, view_input_position);
	}

	//==============================================================================
	/// 터치 취소. (뗌과 같다)
	//==============================================================================
	fn touch_cancel(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.touch_release(context, view_input_position);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	/// 시험 장면. ((100, 100) 에 놓은 50 x 50 카드)
	//==============================================================================
	fn create_card_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let card_id = node_tree.create_child(root_id, "card");
		let card_node = &mut node_tree[card_id];
		card_node.set_pivot(Pivot::MIDDLE_CENTER);
		card_node.set_local_position(Vector2::create(100.0, 100.0));
		card_node.set_content_size(Vector2::create(50.0, 50.0));
		node_tree.add_component(card_id, UIDraggable::new());
		(node_tree, card_id)
	}

	//==============================================================================
	/// 드래그 컴포넌트 반환.
	//==============================================================================
	fn get_draggable(node_tree: &mut NodeTree, card_id: NodeId) -> &mut UIDraggable {
		node_tree[card_id].get_component_mut::<UIDraggable>().expect("draggable")
	}

	//==============================================================================
	/// 끌면 끈 만큼 옮겨지고, 이벤트가 차례로 온다. 복귀를 끄면 놓은 자리에 남는다.
	//==============================================================================
	#[test]
	fn drag_moves_node_and_fires_events() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, card_id) = create_card_tree();
		assert!(node_tree[card_id].is_interactable());
		let event_log = Rc::new(Cell::new(0));
		let start_log = event_log.clone();
		let move_log = event_log.clone();
		let end_log = event_log.clone();
		{
			let draggable = get_draggable(&mut node_tree, card_id);
			draggable.set_drag_start_event(Box::new(move |_draggable: &mut UIDraggable, _context: &mut ComponentContext| {
				start_log.set(start_log.get() + 1);
			}));
			draggable.set_drag_move_event(Box::new(move |_draggable: &mut UIDraggable, _context: &mut ComponentContext, _view_position: Vector2| {
				move_log.set(move_log.get() + 10);
			}));
			draggable.set_drag_end_event(Box::new(move |_draggable: &mut UIDraggable, _context: &mut ComponentContext, view_position: Vector2| {
				assert_eq!(view_position, Vector2::create(160.0, 80.0));
				end_log.set(end_log.get() + 100);
			}));
		}
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Press, Vector2::create(100.0, 100.0));
		assert!(get_draggable(&mut node_tree, card_id).is_dragging());
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Moved, Vector2::create(130.0, 90.0));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Moved, Vector2::create(160.0, 80.0));
		assert_eq!(node_tree[card_id].get_local_position(), Vector2::create(160.0, 80.0));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Release, Vector2::create(160.0, 80.0));
		assert_eq!(event_log.get(), 121);
		assert!(!get_draggable(&mut node_tree, card_id).is_dragging());
		for _ in 0..10 {
			node_tree.tick(&engine, 0.016);
		}
		assert_eq!(node_tree[card_id].get_local_position(), Vector2::create(160.0, 80.0));
		assert_eq!(get_draggable(&mut node_tree, card_id).get_home_position(), Some(Vector2::create(100.0, 100.0)));
	}

	//==============================================================================
	/// 복귀를 켜면 놓은 뒤 제자리로 미끄러져 딱 맞춰진다. 끝 이벤트에서 눌러앉으면 돌아가지 않는다.
	//==============================================================================
	#[test]
	fn snap_back_and_settle() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, card_id) = create_card_tree();
		get_draggable(&mut node_tree, card_id).set_snap_back_enabled(true);
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Press, Vector2::create(100.0, 100.0));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Moved, Vector2::create(200.0, 100.0));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Release, Vector2::create(200.0, 100.0));
		assert!(get_draggable(&mut node_tree, card_id).is_returning());
		node_tree.tick(&engine, 0.05);
		// 0.05 x 14 = 0.7 만큼 다가간다: 200 → 130.
		assert!((node_tree[card_id].get_local_position().x - 130.0).abs() < 0.001);
		for _ in 0..30 {
			node_tree.tick(&engine, 0.05);
		}
		assert_eq!(node_tree[card_id].get_local_position(), Vector2::create(100.0, 100.0));
		assert!(!get_draggable(&mut node_tree, card_id).is_returning());

		// 끝 이벤트에서 눌러앉기.
		get_draggable(&mut node_tree, card_id).set_drag_end_event(Box::new(|draggable: &mut UIDraggable, context: &mut ComponentContext, _view_position: Vector2| {
			draggable.settle_here(context.get_node_tree_mut());
		}));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Press, Vector2::create(100.0, 100.0));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Moved, Vector2::create(100.0, 300.0));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Release, Vector2::create(100.0, 300.0));
		assert!(!get_draggable(&mut node_tree, card_id).is_returning());
		node_tree.tick(&engine, 0.05);
		assert_eq!(node_tree[card_id].get_local_position(), Vector2::create(100.0, 300.0));
		assert_eq!(get_draggable(&mut node_tree, card_id).get_home_position(), Some(Vector2::create(100.0, 300.0)));

		// 바로 되돌리기.
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Press, Vector2::create(100.0, 300.0));
		node_tree.dispatch_touch(card_id, &engine, TouchKind::Moved, Vector2::create(150.0, 300.0));
		node_tree.with_component_mut::<UIDraggable, _>(card_id, |draggable, tree| {
			draggable.snap_home(tree);
		});
		assert_eq!(node_tree[card_id].get_local_position(), Vector2::create(100.0, 300.0));
	}
}
