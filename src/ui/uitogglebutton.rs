//==============================================================================
// 토글 버튼. (원본 ui/uitogglebutton.js 의 UIToggleButton)
// - 클릭할 때마다 켜짐 / 꺼짐이 바뀐다. 켜져 있는 동안은 눌림 틴트를 끝까지 건 채로 둔다.
// - 원본은 UIButton 을 상속하고 자기 클릭 처리를 super.set_clicked_event 에 건다. 러스트 판은 UIButton 을 품고,
//   품은 버튼이 클릭을 알려 주면(뗌 처리의 반환값) 켜짐을 뒤집는다.
//   그래서 순서가 조금 다르다: 원본은 뗌 → [뒤집기, 토글 이벤트, 클릭됨 이벤트] → 클릭 이벤트, 러스트 판은 뗌 → 클릭 이벤트 → [뒤집기, 토글 이벤트, 클릭됨 이벤트].
// - 토글 / 클릭됨 이벤트는 토글 버튼과 문맥을 받는다. 그 밖의 버튼 이벤트와 설정은 get_uibutton_mut 로 품은 버튼에 한다.
//   받은 이벤트는 Rc<RefCell> 에 담는다. get_toggled_event 는 그 Rc 를 돌려주고, 부르는 쪽은 borrow_mut 로 빌려 부른다.
//   get_clicked_event 는 원본(UIButton 에서 물려받은 getClickedEvent)처럼 토글 버튼 안쪽의 클릭 처리 함수를 돌려준다.
//   부르면 켜짐을 뒤집고 토글 이벤트, 클릭됨 이벤트를 부른다. (set_clicked_event 로 건 함수가 아니다)
// - set_is_on 의 틴트는 다음 tick 에 걸린다. (UIButton.set_interactable 과 같은 까닭)
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uibutton::UIButton;
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
// 토글 버튼 이벤트. (토글 버튼과 문맥을 받는다)
//==============================================================================
pub type UIToggleButtonEvent = Box<dyn FnMut(&mut UIToggleButton, &mut ComponentContext)>;

//==============================================================================
// 토글 버튼.
//==============================================================================
pub struct UIToggleButton {
	ui_button: UIButton,
	is_on: bool,
	on_tint_color: Color,
	toggled_event: Option<Rc<RefCell<UIToggleButtonEvent>>>,
	external_clicked_event: Option<Rc<RefCell<UIToggleButtonEvent>>>,
	// 안쪽의 클릭 처리 함수. (원본 super.setClickedEvent 에 건 #onClicked, get_clicked_event 가 돌려준다)
	clicked_handler: Rc<RefCell<UIToggleButtonEvent>>,
}

#[crate::reflect(base = ui_button)]
impl UIToggleButton {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIToggleButton {
		UIToggleButton {
			ui_button: UIButton::new(),
			is_on: false,
			on_tint_color: Color::create(0.0, 0.0, 0.0, 0.3),
			toggled_event: None,
			external_clicked_event: None,
			clicked_handler: Rc::new(RefCell::new(Box::new(|toggle_button: &mut UIToggleButton, context: &mut ComponentContext| {
				toggle_button.on_clicked(context);
			}))),
		}
	}

	//==============================================================================
	// 품은 버튼 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uibutton(&self) -> &UIButton {
		&self.ui_button
	}

	//==============================================================================
	// 품은 버튼 반환. (변경용)
	//==============================================================================
	pub fn get_uibutton_mut(&mut self) -> &mut UIButton {
		&mut self.ui_button
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_button.get_content()
	}

	//==============================================================================
	// 틴트 트랜지션 갱신. (켜져 있으면 끝까지 건 채로 둔다)
	//==============================================================================
	fn update_tint_transition(&mut self, context: &mut ComponentContext, time_delta: f32) {
		if self.get_is_on() {
			self.ui_button.apply_tint_progress(context, 1.0);
			return;
		}
		self.ui_button.update_tint_transition(context, time_delta);
	}

	//==============================================================================
	// 클릭 이벤트 설정. (원본처럼 품은 버튼이 아니라 토글 버튼이 따로 든다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_clicked_event(&mut self, clicked_event: UIToggleButtonEvent) {
		self.external_clicked_event = Some(Rc::new(RefCell::new(clicked_event)));
	}

	//==============================================================================
	// 클릭 이벤트 반환. (원본처럼 안쪽의 클릭 처리 함수, 부르면 뒤집고 토글 이벤트와 클릭됨 이벤트를 부른다, 부를 때는 borrow_mut 로 빌린다)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_clicked_event(&self) -> Option<Rc<RefCell<UIToggleButtonEvent>>> {
		Some(self.clicked_handler.clone())
	}

	//==============================================================================
	// 클릭 처리. (뒤집고 틴트를 건 뒤 토글 이벤트, 클릭됨 이벤트)
	//==============================================================================
	fn on_clicked(&mut self, context: &mut ComponentContext) {
		self.is_on = !self.get_is_on();
		self.ui_button.collect_color_targets(context);
		let tint_progress = if self.get_is_on() {
			1.0
		}
		else {
			0.0
		};
		self.ui_button.apply_tint_progress(context, tint_progress);
		if let Some(toggled_event) = self.get_toggled_event() {
			if let Ok(mut event_function) = toggled_event.try_borrow_mut() {
				(*event_function)(self, context);
			}
		}
		if let Some(external_clicked_event) = self.external_clicked_event.clone() {
			if let Ok(mut event_function) = external_clicked_event.try_borrow_mut() {
				(*event_function)(self, context);
			}
		}
	}

	//==============================================================================
	// 켜짐 설정. (이벤트는 부르지 않는다. 틴트는 다음 tick 에 걸린다)
	//==============================================================================
	pub fn set_is_on(&mut self, is_on: bool) {
		if self.get_is_on() == is_on {
			return;
		}
		self.is_on = is_on;
		self.ui_button.request_color_target_collect();
	}

	//==============================================================================
	// 켜짐 여부.
	//==============================================================================
	pub fn get_is_on(&self) -> bool {
		self.is_on
	}

	//==============================================================================
	// 켜짐 틴트 색 설정. (품은 버튼의 눌림 틴트 색도 같이 바꾼다)
	//==============================================================================
	pub fn set_on_tint_color(&mut self, color: impl Into<Color>) {
		let on_tint_color: Color = color.into();
		self.on_tint_color = on_tint_color;
		self.ui_button.set_pressed_tint_color(on_tint_color);
	}

	//==============================================================================
	// 켜짐 틴트 색 반환.
	//==============================================================================
	pub fn get_on_tint_color(&self) -> Color {
		self.on_tint_color
	}

	//==============================================================================
	// 토글 이벤트 설정.
	//==============================================================================
	pub fn set_toggled_event(&mut self, toggled_event: UIToggleButtonEvent) {
		self.toggled_event = Some(Rc::new(RefCell::new(toggled_event)));
	}

	//==============================================================================
	// 토글 이벤트 반환. (부를 때는 borrow_mut 로 빌린다)
	//==============================================================================
	pub fn get_toggled_event(&self) -> Option<Rc<RefCell<UIToggleButtonEvent>>> {
		self.toggled_event.clone()
	}
}

impl Default for UIToggleButton {
	fn default() -> UIToggleButton {
		UIToggleButton::new()
	}
}

#[crate::reflect]
impl Component for UIToggleButton {
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_button.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_button.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"ToggleButton"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_button.attach_to_tree(node_tree, node_id);
	}

	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		let is_clicked = self.ui_button.tick_press(context, time_delta);
		if is_clicked {
			self.on_clicked(context);
		}
		self.ui_button.update_hover_state(context);
		self.update_tint_transition(context, time_delta);
	}

	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	fn touch_press(&mut self, context: &mut ComponentContext, _view_input_position: Vector2) {
		self.ui_button.handle_touch_press(context);
	}

	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		let is_clicked = self.ui_button.handle_touch_release(context, view_input_position);
		if is_clicked {
			self.on_clicked(context);
		}
	}

	fn touch_cancel(&mut self, context: &mut ComponentContext, _view_input_position: Vector2) {
		self.ui_button.handle_touch_cancel(context);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::component::paint::Paint;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (가운데 (100, 100), 크기 60 x 60 인 토글 버튼. 바탕은 흰 Paint)
	//==============================================================================
	fn create_toggle_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let toggle_id = node_tree.create_child(root_id, "toggle");
		let toggle_node = &mut node_tree[toggle_id];
		toggle_node.set_local_position(Vector2::create(100.0, 100.0));
		toggle_node.set_content_size(Vector2::create(60.0, 60.0));
		toggle_node.add_component(Paint::create_with_color(Color::white()));
		node_tree.add_component(toggle_id, UIToggleButton::new());
		(node_tree, toggle_id)
	}

	//==============================================================================
	// 바탕 색 반환.
	//==============================================================================
	fn get_paint_color(node_tree: &NodeTree, toggle_id: NodeId) -> Color {
		node_tree[toggle_id].get_component::<Paint>().expect("paint").get_color()
	}

	//==============================================================================
	// 클릭마다 켜짐이 뒤집히고 토글 이벤트가 난다. 켜져 있는 동안 바탕은 눌림 틴트를 끝까지 건 색이다.
	//==============================================================================
	#[test]
	fn click_toggles_and_holds_tint() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, toggle_id) = create_toggle_tree();
		let toggled_count = Rc::new(Cell::new(0));
		let last_is_on = Rc::new(Cell::new(false));
		{
			let toggle_button = node_tree[toggle_id].get_component_mut::<UIToggleButton>().expect("toggle");
			let toggled_count_for_event = toggled_count.clone();
			let last_is_on_for_event = last_is_on.clone();
			toggle_button.set_toggled_event(Box::new(move |toggle_button: &mut UIToggleButton, _context: &mut ComponentContext| {
				toggled_count_for_event.set(toggled_count_for_event.get() + 1);
				last_is_on_for_event.set(toggle_button.get_is_on());
			}));
			assert!(toggle_button.get_toggled_event().is_some());
		}
		let inside = Vector2::create(100.0, 100.0);
		node_tree.dispatch_touch(toggle_id, &engine, TouchKind::Press, inside);
		node_tree.dispatch_touch(toggle_id, &engine, TouchKind::Release, inside);
		assert_eq!(toggled_count.get(), 1);
		assert!(last_is_on.get());
		node_tree.tick(&engine, 1.0);
		let on_color = get_paint_color(&node_tree, toggle_id);
		assert!((on_color.red - 0.7).abs() < 0.001);

		node_tree.dispatch_touch(toggle_id, &engine, TouchKind::Press, inside);
		node_tree.dispatch_touch(toggle_id, &engine, TouchKind::Release, inside);
		assert_eq!(toggled_count.get(), 2);
		assert!(!last_is_on.get());
		// 원본 버릇: 켜진 채로 누르면 틴트가 걸린 색을 원래 색으로 다시 모아, 끈 뒤에도 그 색(0.7)에 남는다.
		node_tree.tick(&engine, 1.0);
		let off_color = get_paint_color(&node_tree, toggle_id);
		assert!((off_color.red - 0.7).abs() < 0.001);
	}

	//==============================================================================
	// set_is_on 은 이벤트 없이 바꾸고 다음 tick 에 틴트를 건다.
	//==============================================================================
	#[test]
	fn set_is_on_applies_tint_next_tick() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, toggle_id) = create_toggle_tree();
		let toggled_count = Rc::new(Cell::new(0));
		{
			let toggle_button = node_tree[toggle_id].get_component_mut::<UIToggleButton>().expect("toggle");
			let toggled_count_for_event = toggled_count.clone();
			toggle_button.set_toggled_event(Box::new(move |_toggle_button: &mut UIToggleButton, _context: &mut ComponentContext| {
				toggled_count_for_event.set(toggled_count_for_event.get() + 1);
			}));
			toggle_button.set_is_on(true);
			assert!(toggle_button.get_is_on());
		}
		assert_eq!(get_paint_color(&node_tree, toggle_id), Color::white());
		node_tree.tick(&engine, 0.016);
		let on_color = get_paint_color(&node_tree, toggle_id);
		assert!((on_color.red - 0.7).abs() < 0.001);
		assert_eq!(toggled_count.get(), 0);
	}

	//==============================================================================
	// get_clicked_event 로 꺼낸 안쪽 클릭 처리 함수를 다른 버튼의 클릭에서 부르면 원본처럼 뒤집고 토글 이벤트를 부른다.
	//==============================================================================
	#[test]
	fn clicked_event_getter_toggles() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, toggle_id) = create_toggle_tree();
		let toggled_count = Rc::new(Cell::new(0));
		{
			let toggle_button = node_tree[toggle_id].get_component_mut::<UIToggleButton>().expect("toggle");
			let toggled_count_for_event = toggled_count.clone();
			toggle_button.set_toggled_event(Box::new(move |_toggle_button: &mut UIToggleButton, _context: &mut ComponentContext| {
				toggled_count_for_event.set(toggled_count_for_event.get() + 1);
			}));
		}
		let root_id = node_tree.get_root_id();
		let relay_id = node_tree.create_child(root_id, "relay");
		node_tree[relay_id].set_local_position(Vector2::create(300.0, 100.0));
		node_tree[relay_id].set_content_size(Vector2::create(60.0, 60.0));
		let relay_button = node_tree.add_component(relay_id, UIButton::new()).expect("relay");
		relay_button.set_click_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
			context.with_component_mut::<UIToggleButton, _>(toggle_id, |toggle_button, toggle_context| {
				if let Some(clicked_event) = toggle_button.get_clicked_event() {
					(*clicked_event.borrow_mut())(toggle_button, toggle_context);
				}
			});
		}));
		let inside = Vector2::create(300.0, 100.0);
		node_tree.dispatch_touch(relay_id, &engine, TouchKind::Press, inside);
		node_tree.dispatch_touch(relay_id, &engine, TouchKind::Release, inside);
		assert!(node_tree[toggle_id].get_component::<UIToggleButton>().expect("toggle").get_is_on());
		assert_eq!(toggled_count.get(), 1);
	}
}
