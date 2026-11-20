//==============================================================================
// 버튼. (원본 ui/uibutton.js 의 UIButton, ButtonState)
// - 누름 / 뗌 / 취소 / 클릭 / 길게 누름 / 마우스 오버 이벤트와, 누르거나 올렸을 때 색이 변하는 틴트 트랜지션.
// - 클릭은 눌렀다가 버튼 영역 안에서 뗄 때만 일어난다. (길게 누름이 발화한 누름은 클릭을 삼킨다)
// - 틴트는 원본처럼 호스트 노드와 모든 자손의 Sprite / UIImageView(덮는 색), Paint / Text / UILabel(원래 색 보간)에 건다.
//   호스트 노드의 다른 컴포넌트는 tick / 터치 중에 형제로 찾는다. (NodeTree 가 부르는 컴포넌트만 잠깐 꺼낸다)
//   원본은 Sprite 가 Paint 를 상속해 getComponents(Paint) 에도 잡힌다. 그래서 스프라이트에는 덮는 색 다음에 원래 색 보간이 한 번 더 걸려
//   뒤의 것이 이긴다. 같은 결과를 내려고 러스트 판도 스프라이트(와 UIImageView 의 스프라이트)를 두 번 담는다.
// - 원본과 다른 것:
//   - 이벤트는 Box<dyn FnMut(&mut UIButton, &mut ComponentContext)> 다. 원본 콜백은 버튼만 받는다.
//     문맥을 함께 넘겨 콜백에서 다른 노드를 바꾸거나 만들고 지울 수 있게 했다. (클릭 이벤트의 bool 반환은 원본에서도 쓰지 않아 뺐다)
//   - 원본 set_interactable / UIToggleButton.set_is_on 은 그 자리에서 색 대상을 모으고 틴트를 건다. 러스트 판은 노드에 닿을 수 없는 자리라
//     다음 tick 에 모으고 건다. (tick 이 매 프레임 틴트를 다시 거므로 보이는 결과는 같다)
//   - 제외 컴포넌트는 인스턴스 대신 (노드, 컴포넌트 타입) 으로 정한다: exclude_component_from_tint::<Paint>(node_id).
//   - setStateEvent(원본도 빈 함수)와 이벤트 getter 는 옮기지 않았다.
// - attach_to_tree 에서 호스트 노드를 interactable 로 켠다. node_tree.add_component(node_id, UIButton::new()) 로 붙인다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::sprite::Sprite;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uicontrol::UIControl;
use crate::ui::uiimageview::UIImageView;
use crate::ui::uilabel::UILabel;
use crate::ui::uiview::is_view_position_inside_node;
use std::any::{Any, TypeId};

//==============================================================================
// 버튼 상태.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
	Normal,
	Hover,
	Pressed,
	Released,
	Disabled,
}

//==============================================================================
// 버튼 이벤트. (버튼과 문맥을 받는다)
//==============================================================================
pub type UIButtonEvent = Box<dyn FnMut(&mut UIButton, &mut ComponentContext)>;

//==============================================================================
// 이벤트 종류. (이벤트 칸을 고를 때)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ButtonEventKind {
	Click,
	Pressed,
	Released,
	Clicked,
	LongPressed,
	Hover,
}

//==============================================================================
// 색 대상 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorEntryKind {
	// Sprite 에 덮는 색. (원본 "sprite")
	SpriteOverlay,
	// UIImageView 에 덮는 색. (원본 "imageview")
	ImageViewOverlay,
	// Paint 원래 색 보간. (원본 "paint")
	Paint,
	// Sprite 원래 색 보간. (원본에서 Sprite 가 getComponents(Paint) 에 잡혀 생기는 "paint")
	SpritePaint,
	// UIImageView 의 스프라이트 원래 색 보간. (원본에서 UIImageView 의 Sprite 가 getComponents(Paint) 에 잡혀 생기는 "paint")
	ImageViewPaint,
	// Text 글자 색 보간. (원본 "text")
	Text,
	// UILabel 글자 색 보간. (원본 "uilabel")
	Label,
}

//==============================================================================
// 색 대상. (노드, 그 노드 컴포넌트 목록의 자리, 모을 때의 원래 색)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ColorEntry {
	node_id: NodeId,
	component_index: usize,
	kind: ColorEntryKind,
	original_color: Color,
}

//==============================================================================
// 최소 눌림 시간을 채우려고 미뤄 둔 뗌.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct PendingRelease {
	view_input_position: Vector2,
	remain_seconds: f32,
}

//==============================================================================
// 버튼.
//==============================================================================
pub struct UIButton {
	ui_control: UIControl,
	button_state: ButtonState,
	click_event: Option<UIButtonEvent>,
	pressed_event: Option<UIButtonEvent>,
	released_event: Option<UIButtonEvent>,
	clicked_event: Option<UIButtonEvent>,
	long_pressed_event: Option<UIButtonEvent>,
	hover_event: Option<UIButtonEvent>,
	is_press_tracking: bool,
	minimum_pressed_seconds: f32,
	pressed_elapsed_seconds: f32,
	pending_release: Option<PendingRelease>,
	long_press_seconds: f32,
	has_long_press_fired: bool,
	pressed_tint_color: Color,
	hover_tint_color: Color,
	transition_duration: f32,
	tint_progress: f32,
	color_entries: Vec<ColorEntry>,
	is_interactable: bool,
	disabled_tint_color: Color,
	tint_excluded_node_ids: Vec<NodeId>,
	tint_excluded_components: Vec<(NodeId, TypeId)>,
	is_color_target_collect_pending: bool,
}

impl UIButton {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIButton {
		UIButton {
			ui_control: UIControl::new(),
			button_state: ButtonState::Normal,
			click_event: None,
			pressed_event: None,
			released_event: None,
			clicked_event: None,
			long_pressed_event: None,
			hover_event: None,
			is_press_tracking: false,
			minimum_pressed_seconds: 0.0,
			pressed_elapsed_seconds: 0.0,
			pending_release: None,
			long_press_seconds: 0.5,
			has_long_press_fired: false,
			pressed_tint_color: Color::create(0.0, 0.0, 0.0, 0.3),
			hover_tint_color: Color::create(1.0, 1.0, 1.0, 0.12),
			transition_duration: 0.3,
			tint_progress: 0.0,
			color_entries: Vec::new(),
			is_interactable: true,
			disabled_tint_color: Color::create(0.0, 0.0, 0.0, 0.5),
			tint_excluded_node_ids: Vec::new(),
			tint_excluded_components: Vec::new(),
			is_color_target_collect_pending: false,
		}
	}

	//==============================================================================
	// 품은 컨트롤 반환.
	//==============================================================================
	pub fn get_uicontrol(&self) -> &UIControl {
		&self.ui_control
	}

	//==============================================================================
	// 품은 컨트롤 반환. (변경용)
	//==============================================================================
	pub fn get_uicontrol_mut(&mut self) -> &mut UIControl {
		&mut self.ui_control
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_control.get_content()
	}

	//==============================================================================
	// 트랜지션 대상에서 노드를 제외. (그 노드와 모든 자손의 컴포넌트가 색 대상에서 빠진다)
	//==============================================================================
	pub fn exclude_node_from_tint(&mut self, node_id: NodeId) {
		if !self.tint_excluded_node_ids.contains(&node_id) {
			self.tint_excluded_node_ids.push(node_id);
		}
	}

	//==============================================================================
	// 트랜지션 대상에서 그 노드의 T 타입 컴포넌트만 제외. (원본은 컴포넌트 인스턴스를 받는다)
	//==============================================================================
	pub fn exclude_component_from_tint<T: Component + 'static>(&mut self, node_id: NodeId) {
		let excluded_component = (node_id, TypeId::of::<T>());
		if !self.tint_excluded_components.contains(&excluded_component) {
			self.tint_excluded_components.push(excluded_component);
		}
	}

	//==============================================================================
	// 제외 목록 비우기.
	//==============================================================================
	pub fn clear_tint_exclusions(&mut self) {
		self.tint_excluded_node_ids.clear();
		self.tint_excluded_components.clear();
	}

	//==============================================================================
	// 이벤트 칸 고르기.
	//==============================================================================
	fn get_event_slot(&mut self, event_kind: ButtonEventKind) -> &mut Option<UIButtonEvent> {
		match event_kind {
			ButtonEventKind::Click => {
				&mut self.click_event
			}
			ButtonEventKind::Pressed => {
				&mut self.pressed_event
			}
			ButtonEventKind::Released => {
				&mut self.released_event
			}
			ButtonEventKind::Clicked => {
				&mut self.clicked_event
			}
			ButtonEventKind::LongPressed => {
				&mut self.long_pressed_event
			}
			ButtonEventKind::Hover => {
				&mut self.hover_event
			}
		}
	}

	//==============================================================================
	// 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fire_event(&mut self, context: &mut ComponentContext, event_kind: ButtonEventKind) {
		let mut event = match self.get_event_slot(event_kind).take() {
			Some(event) => {
				event
			}
			None => {
				return;
			}
		};
		event(self, context);
		let event_slot = self.get_event_slot(event_kind);
		if event_slot.is_none() {
			*event_slot = Some(event);
		}
	}

	//==============================================================================
	// 클릭 알림만 부르기. (누르지 않고 클릭이 난 것처럼 Clicked, Click 차례로, UIHitArea 가 넓힌 자리의 클릭을 넘길 때)
	//==============================================================================
	pub fn fire_click_events(&mut self, context: &mut ComponentContext) {
		self.fire_event(context, ButtonEventKind::Clicked);
		self.fire_event(context, ButtonEventKind::Click);
	}

	//==============================================================================
	// 누름 추적과 미뤄 둔 뗌 갱신. (뗌을 처리해 클릭이 났으면 true)
	// - UIToggleButton 이 클릭을 받아 켜짐을 뒤집으려고 따로 부른다.
	//==============================================================================
	pub(crate) fn tick_press(&mut self, context: &mut ComponentContext, time_delta: f32) -> bool {
		if self.is_color_target_collect_pending {
			self.is_color_target_collect_pending = false;
			self.collect_color_targets(context);
		}
		if self.is_press_tracking {
			self.pressed_elapsed_seconds += time_delta;

			// 길게 누름. (임계 시간을 넘기는 순간 한 번만 알린다)
			if self.long_pressed_event.is_some() && !self.has_long_press_fired && self.pressed_elapsed_seconds >= self.long_press_seconds {
				self.has_long_press_fired = true;
				self.fire_event(context, ButtonEventKind::LongPressed);
			}
		}

		// 최소 눌림 표시 시간을 채우려고 미뤄 둔 뗌 처리를 마저 한다.
		if let Some(pending_release) = self.pending_release.as_mut() {
			pending_release.remain_seconds -= time_delta;
			if pending_release.remain_seconds <= 0.0 {
				let release_position = pending_release.view_input_position;
				self.pending_release = None;
				return self.perform_release(context, release_position);
			}
		}
		false
	}

	//==============================================================================
	// 마우스 오버 상태 갱신. (누르고 있는 중이거나 사용 불가일 때는 건드리지 않는다)
	//==============================================================================
	pub(crate) fn update_hover_state(&mut self, context: &mut ComponentContext) {
		if !self.get_interactable() {
			return;
		}
		let current_button_state = self.get_button_state();
		if current_button_state == ButtonState::Pressed {
			context.get_engine().get_pointer_cursor().mark_over_button();
		}
		if current_button_state == ButtonState::Pressed || current_button_state == ButtonState::Disabled {
			return;
		}
		let view_input_position = context.get_engine().get_input_manager().get_view_input_position();
		let node_id = context.get_node_id();
		let node_tree = context.get_node_tree();
		let is_hovering = is_view_position_inside_node(node_tree, node_id, view_input_position);
		if is_hovering && current_button_state != ButtonState::Hover {
			self.set_button_state(ButtonState::Hover);
			self.collect_color_targets(context);
			self.tint_progress = 0.0;
			self.fire_event(context, ButtonEventKind::Hover);
		}
		else if !is_hovering && current_button_state == ButtonState::Hover {
			self.set_button_state(ButtonState::Normal);
			self.collect_color_targets(context);
			self.tint_progress = 0.0;
		}
		if is_hovering {
			context.get_engine().get_pointer_cursor().mark_over_button();
		}
	}

	//==============================================================================
	// 틴트 트랜지션 갱신. (누르거나 올려 두면 차오르고, 아니면 빠진다. 사용 불가면 어두운 틴트)
	//==============================================================================
	pub(crate) fn update_tint_transition(&mut self, context: &mut ComponentContext, time_delta: f32) {
		if !self.get_interactable() {
			self.apply_disabled_tint(context);
			return;
		}
		let button_state = self.get_button_state();
		let is_pressed = button_state == ButtonState::Pressed;
		let is_hovering = button_state == ButtonState::Hover;
		let transition_duration = self.get_transition_duration();
		if is_pressed || is_hovering {
			self.tint_progress = (self.tint_progress + time_delta / transition_duration).min(1.0);
		}
		else {
			self.tint_progress = (self.tint_progress - time_delta / transition_duration).max(0.0);
		}
		let tint_progress = self.tint_progress;
		self.apply_tint_progress(context, tint_progress);
	}

	//==============================================================================
	// 뗌 처리 본문. (클릭이 났으면 true)
	//==============================================================================
	pub(crate) fn perform_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) -> bool {
		self.set_button_state(ButtonState::Released);
		self.tint_progress = 0.0;
		self.apply_tint_progress(context, 0.0);
		self.fire_event(context, ButtonEventKind::Released);
		let node_id = context.get_node_id();
		let node_tree = context.get_node_tree();
		let is_inside_bounds = is_view_position_inside_node(node_tree, node_id, view_input_position);
		let is_clicked = is_inside_bounds && !self.has_long_press_fired;
		if is_clicked {
			self.fire_event(context, ButtonEventKind::Clicked);
			self.fire_event(context, ButtonEventKind::Click);
		}
		self.set_button_state(ButtonState::Normal);
		is_clicked
	}

	//==============================================================================
	// 터치 누름 처리.
	//==============================================================================
	pub(crate) fn handle_touch_press(&mut self, context: &mut ComponentContext) {
		if !self.get_interactable() {
			return;
		}
		self.is_press_tracking = true;
		self.pressed_elapsed_seconds = 0.0;
		self.pending_release = None;
		self.has_long_press_fired = false;
		self.set_button_state(ButtonState::Pressed);
		self.collect_color_targets(context);
		self.fire_event(context, ButtonEventKind::Pressed);
	}

	//==============================================================================
	// 터치 뗌 처리. (바로 클릭이 났으면 true. 최소 눌림 시간이 남았으면 미루고 false)
	//==============================================================================
	pub(crate) fn handle_touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) -> bool {
		if !self.get_interactable() {
			return false;
		}
		if !self.is_press_tracking {
			return false;
		}
		self.is_press_tracking = false;

		// 최소 눌림 표시 시간을 아직 못 채웠으면, 그만큼 눌린 모습을 유지한 뒤 처리한다.
		if self.minimum_pressed_seconds > 0.0 && self.pressed_elapsed_seconds < self.minimum_pressed_seconds {
			self.pending_release = Some(PendingRelease {
				view_input_position,
				remain_seconds: self.minimum_pressed_seconds - self.pressed_elapsed_seconds,
			});
			return false;
		}
		self.perform_release(context, view_input_position)
	}

	//==============================================================================
	// 터치 취소 처리.
	//==============================================================================
	pub(crate) fn handle_touch_cancel(&mut self, context: &mut ComponentContext) {
		self.pending_release = None;
		if !self.is_press_tracking {
			return;
		}
		self.is_press_tracking = false;
		self.tint_progress = 0.0;
		self.apply_tint_progress(context, 0.0);
		self.set_button_state(ButtonState::Normal);
	}

	//==============================================================================
	// 틴트 적용. (덮는 색은 알파만 차오르고, 원래 색은 틴트 색 쪽으로 보간한다)
	//==============================================================================
	pub(crate) fn apply_tint_progress(&mut self, context: &mut ComponentContext, progress: f32) {
		let tint_color = if self.get_button_state() == ButtonState::Hover {
			self.get_hover_tint_color()
		}
		else {
			self.get_pressed_tint_color()
		};
		let overlay_alpha = math::lerp(0.0, tint_color.alpha, progress);
		let overlay_color = Color::create(tint_color.red, tint_color.green, tint_color.blue, overlay_alpha);
		let node_tree = context.get_node_tree_mut();
		for color_entry in self.color_entries.iter() {
			let entry_color = match color_entry.kind {
				ColorEntryKind::SpriteOverlay | ColorEntryKind::ImageViewOverlay => {
					overlay_color
				}
				_ => {
					let original_color = color_entry.original_color;
					let tinted_red = math::lerp(original_color.red, tint_color.red, tint_color.alpha * progress);
					let tinted_green = math::lerp(original_color.green, tint_color.green, tint_color.alpha * progress);
					let tinted_blue = math::lerp(original_color.blue, tint_color.blue, tint_color.alpha * progress);
					Color::create(tinted_red, tinted_green, tinted_blue, original_color.alpha)
				}
			};
			set_color_entry_color(node_tree, color_entry, entry_color);
		}
	}

	//==============================================================================
	// 사용 불가 틴트 적용. (덮는 색 대상에만)
	//==============================================================================
	fn apply_disabled_tint(&mut self, context: &mut ComponentContext) {
		let disabled_tint_color = self.disabled_tint_color;
		let node_tree = context.get_node_tree_mut();
		for color_entry in self.color_entries.iter() {
			if color_entry.kind == ColorEntryKind::SpriteOverlay || color_entry.kind == ColorEntryKind::ImageViewOverlay {
				set_color_entry_color(node_tree, color_entry, disabled_tint_color);
			}
		}
	}

	//==============================================================================
	// 색 대상 모으기. (호스트 노드부터 자손까지)
	//==============================================================================
	pub(crate) fn collect_color_targets(&mut self, context: &mut ComponentContext) {
		self.color_entries.clear();
		let node_id = context.get_node_id();
		let node_tree = context.get_node_tree();
		self.collect_from_node(node_tree, node_id);
	}

	//==============================================================================
	// 그 노드의 T 타입 컴포넌트가 제외 목록에 있는지.
	//==============================================================================
	fn is_component_excluded(&self, node_id: NodeId, component_type_id: TypeId) -> bool {
		self.tint_excluded_components.contains(&(node_id, component_type_id))
	}

	//==============================================================================
	// 노드에서 색 대상을 재귀로 모은다. (원본 collect_from_node 와 같은 순서)
	// - exclude_node_from_tint 로 정한 노드는 자신과 자손까지 통째로 건너뛴다.
	//==============================================================================
	fn collect_from_node(&mut self, node_tree: &NodeTree, node_id: NodeId) {
		if self.tint_excluded_node_ids.contains(&node_id) {
			return;
		}
		let node = match node_tree.get_node(node_id) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		let components = node.get_all_components();

		// UIImageView 가 있으면 그것을 통해 색을 다루고, 없으면 Sprite 에 덮는 색을 건다.
		let has_image_view = components.iter().any(|component| {
			let component_reference: &dyn Component = component.as_ref();
			component_reference.as_any().is::<UIImageView>()
		});
		for (component_index, component) in components.iter().enumerate() {
			let component_reference: &dyn Component = component.as_ref();
			let any_reference: &dyn Any = component_reference.as_any();
			let component_type_id = any_reference.type_id();
			if self.is_component_excluded(node_id, component_type_id) {
				continue;
			}
			if has_image_view {
				if let Some(image_view) = any_reference.downcast_ref::<UIImageView>() {
					let original_color = image_view.get_color();
					self.push_color_entry(node_id, component_index, ColorEntryKind::ImageViewOverlay, original_color);
				}
			}
			else if let Some(sprite) = any_reference.downcast_ref::<Sprite>() {
				let original_color = sprite.get_color();
				self.push_color_entry(node_id, component_index, ColorEntryKind::SpriteOverlay, original_color);
			}
		}

		// Paint (단색 배경) 도 원래 색을 들고 보간한다. (원본은 Sprite 와 UIImageView 의 Sprite 도 여기 잡힌다)
		for (component_index, component) in components.iter().enumerate() {
			let component_reference: &dyn Component = component.as_ref();
			let any_reference: &dyn Any = component_reference.as_any();
			let component_type_id = any_reference.type_id();
			if self.is_component_excluded(node_id, component_type_id) {
				continue;
			}
			if let Some(paint) = any_reference.downcast_ref::<Paint>() {
				let original_color = paint.get_color();
				self.push_color_entry(node_id, component_index, ColorEntryKind::Paint, original_color);
			}
			else if let Some(sprite) = any_reference.downcast_ref::<Sprite>() {
				let original_color = sprite.get_color();
				self.push_color_entry(node_id, component_index, ColorEntryKind::SpritePaint, original_color);
			}
			else if let Some(image_view) = any_reference.downcast_ref::<UIImageView>() {
				let original_color = image_view.get_color();
				self.push_color_entry(node_id, component_index, ColorEntryKind::ImageViewPaint, original_color);
			}
		}

		// 글자도 같다. UILabel 이 있으면 그것만, 없으면 Text.
		let has_label = components.iter().any(|component| {
			let component_reference: &dyn Component = component.as_ref();
			component_reference.as_any().is::<UILabel>()
		});
		for (component_index, component) in components.iter().enumerate() {
			let component_reference: &dyn Component = component.as_ref();
			let any_reference: &dyn Any = component_reference.as_any();
			let component_type_id = any_reference.type_id();
			if self.is_component_excluded(node_id, component_type_id) {
				continue;
			}
			if has_label {
				if let Some(label) = any_reference.downcast_ref::<UILabel>() {
					let original_color = label.get_text_color();
					self.push_color_entry(node_id, component_index, ColorEntryKind::Label, original_color);
				}
			}
			else if let Some(text) = any_reference.downcast_ref::<Text>() {
				let original_color = text.get_text_color();
				self.push_color_entry(node_id, component_index, ColorEntryKind::Text, original_color);
			}
		}

		for child_id in node_tree.get_child_ids(node_id).iter().copied() {
			self.collect_from_node(node_tree, child_id);
		}
	}

	//==============================================================================
	// 색 대상 하나 담기.
	//==============================================================================
	fn push_color_entry(&mut self, node_id: NodeId, component_index: usize, kind: ColorEntryKind, original_color: Color) {
		self.color_entries.push(ColorEntry {
			node_id,
			component_index,
			kind,
			original_color,
		});
	}

	//==============================================================================
	// 버튼 상태 설정.
	//==============================================================================
	pub fn set_button_state(&mut self, button_state: ButtonState) {
		self.button_state = button_state;
	}

	//==============================================================================
	// 버튼 상태 반환.
	//==============================================================================
	pub fn get_button_state(&self) -> ButtonState {
		self.button_state
	}

	//==============================================================================
	// 길게 누름 이벤트 설정. (임계 시간을 넘기면 한 번 알리고, 그 누름의 클릭은 삼킨다)
	//==============================================================================
	pub fn set_long_pressed_event(&mut self, long_pressed_event: UIButtonEvent) {
		self.long_pressed_event = Some(long_pressed_event);
	}

	//==============================================================================
	// 길게 누름 판정 시간 설정. (초)
	//==============================================================================
	pub fn set_long_press_seconds(&mut self, long_press_seconds: f32) {
		self.long_press_seconds = long_press_seconds;
	}

	//==============================================================================
	// 길게 누름 판정 시간 반환.
	//==============================================================================
	pub fn get_long_press_seconds(&self) -> f32 {
		self.long_press_seconds
	}

	//==============================================================================
	// 이번 누름이 이어진 시간 반환. (누르고 있지 않으면 0. 게이지 연출용)
	//==============================================================================
	pub fn get_pressed_elapsed_seconds(&self) -> f32 {
		if self.is_press_tracking {
			self.pressed_elapsed_seconds
		}
		else {
			0.0
		}
	}

	//==============================================================================
	// 최소 눌림 표시 시간 설정. (짧게 눌러도 이 시간만큼 눌린 모습을 보인 뒤 클릭한다. 기본 0 = 바로)
	//==============================================================================
	pub fn set_minimum_pressed_seconds(&mut self, minimum_pressed_seconds: f32) {
		self.minimum_pressed_seconds = minimum_pressed_seconds.max(0.0);
	}

	//==============================================================================
	// 최소 눌림 표시 시간 반환.
	//==============================================================================
	pub fn get_minimum_pressed_seconds(&self) -> f32 {
		self.minimum_pressed_seconds
	}

	//==============================================================================
	// 클릭 이벤트 설정. (클릭됨 이벤트 다음에 불린다)
	//==============================================================================
	pub fn set_click_event(&mut self, click_event: UIButtonEvent) {
		self.click_event = Some(click_event);
	}

	//==============================================================================
	// 누름 이벤트 설정.
	//==============================================================================
	pub fn set_pressed_event(&mut self, pressed_event: UIButtonEvent) {
		self.pressed_event = Some(pressed_event);
	}

	//==============================================================================
	// 뗌 이벤트 설정. (영역 밖에서 떼도 불린다)
	//==============================================================================
	pub fn set_released_event(&mut self, released_event: UIButtonEvent) {
		self.released_event = Some(released_event);
	}

	//==============================================================================
	// 클릭됨 이벤트 설정.
	//==============================================================================
	pub fn set_clicked_event(&mut self, clicked_event: UIButtonEvent) {
		self.clicked_event = Some(clicked_event);
	}

	//==============================================================================
	// 오버 진입 이벤트 설정.
	//==============================================================================
	pub fn set_hover_event(&mut self, hover_event: UIButtonEvent) {
		self.hover_event = Some(hover_event);
	}

	//==============================================================================
	// 오버 틴트 색 설정.
	//==============================================================================
	pub fn set_hover_tint_color(&mut self, color: impl Into<Color>) {
		self.hover_tint_color = color.into();
	}

	//==============================================================================
	// 오버 틴트 색 반환.
	//==============================================================================
	pub fn get_hover_tint_color(&self) -> Color {
		self.hover_tint_color
	}

	//==============================================================================
	// 눌림 틴트 색 설정.
	//==============================================================================
	pub fn set_pressed_tint_color(&mut self, color: impl Into<Color>) {
		self.pressed_tint_color = color.into();
	}

	//==============================================================================
	// 눌림 틴트 색 반환.
	//==============================================================================
	pub fn get_pressed_tint_color(&self) -> Color {
		self.pressed_tint_color
	}

	//==============================================================================
	// 트랜지션 시간 설정. (초)
	//==============================================================================
	pub fn set_transition_duration(&mut self, transition_duration: f32) {
		self.transition_duration = transition_duration;
	}

	//==============================================================================
	// 트랜지션 시간 반환.
	//==============================================================================
	pub fn get_transition_duration(&self) -> f32 {
		self.transition_duration
	}

	//==============================================================================
	// 사용 가능 설정. (끄면 다음 tick 에 색 대상을 모아 어두운 틴트를 건다)
	//==============================================================================
	pub fn set_interactable(&mut self, is_interactable: bool) {
		if self.get_interactable() == is_interactable {
			return;
		}
		self.is_interactable = is_interactable;
		if !is_interactable {
			self.is_color_target_collect_pending = true;
			self.set_button_state(ButtonState::Disabled);
		}
		else {
			self.tint_progress = 0.0;
			self.set_button_state(ButtonState::Normal);
		}
	}

	//==============================================================================
	// 사용 가능 여부.
	//==============================================================================
	pub fn get_interactable(&self) -> bool {
		self.is_interactable
	}

	//==============================================================================
	// 다음 tick 에 색 대상을 다시 모으게 한다. (UIToggleButton.set_is_on)
	//==============================================================================
	pub(crate) fn request_color_target_collect(&mut self) {
		self.is_color_target_collect_pending = true;
	}
}

impl Default for UIButton {
	fn default() -> UIButton {
		UIButton::new()
	}
}

//==============================================================================
// 색 대상 하나에 색 넣기. (자리의 컴포넌트가 바뀌었으면 건너뛴다)
//==============================================================================
fn set_color_entry_color(node_tree: &mut NodeTree, color_entry: &ColorEntry, color: Color) {
	let node = match node_tree.get_node_mut(color_entry.node_id) {
		Some(node) => {
			node
		}
		None => {
			return;
		}
	};
	let component = match node.components.get_mut(color_entry.component_index) {
		Some(component) => {
			component
		}
		None => {
			return;
		}
	};
	let component_reference: &mut dyn Component = component.as_mut();
	let any_reference: &mut dyn Any = component_reference.as_any_mut();
	match color_entry.kind {
		ColorEntryKind::SpriteOverlay | ColorEntryKind::SpritePaint => {
			if let Some(sprite) = any_reference.downcast_mut::<Sprite>() {
				sprite.set_color(color);
			}
		}
		ColorEntryKind::ImageViewOverlay | ColorEntryKind::ImageViewPaint => {
			if let Some(image_view) = any_reference.downcast_mut::<UIImageView>() {
				image_view.set_color(color);
			}
		}
		ColorEntryKind::Paint => {
			if let Some(paint) = any_reference.downcast_mut::<Paint>() {
				paint.set_color(color);
			}
		}
		ColorEntryKind::Text => {
			if let Some(text) = any_reference.downcast_mut::<Text>() {
				text.set_text_color(color);
			}
		}
		ColorEntryKind::Label => {
			if let Some(label) = any_reference.downcast_mut::<UILabel>() {
				label.set_text_color(color);
			}
		}
	}
}

impl Component for UIButton {
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_control.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_control.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"Button"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_control.get_uiview_mut().create_content_node(node_tree, node_id);
		node_tree[node_id].set_interactable(true);
	}

	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.tick_press(context, time_delta);
		self.update_hover_state(context);
		self.update_tint_transition(context, time_delta);
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
	}

	fn touch_press(&mut self, context: &mut ComponentContext, _view_input_position: Vector2) {
		self.handle_touch_press(context);
	}

	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.handle_touch_release(context, view_input_position);
	}

	fn touch_cancel(&mut self, context: &mut ComponentContext, _view_input_position: Vector2) {
		self.handle_touch_cancel(context);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (가운데 (200, 100), 크기 100 x 40 인 버튼. 바탕은 Paint)
	//==============================================================================
	fn create_button_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let button_id = node_tree.create_child(root_id, "button");
		let button_node = &mut node_tree[button_id];
		button_node.set_local_position(Vector2::create(200.0, 100.0));
		button_node.set_content_size(Vector2::create(100.0, 40.0));
		button_node.add_component(Paint::create_with_color(Color::white()));
		node_tree.add_component(button_id, UIButton::new());
		(node_tree, button_id)
	}

	//==============================================================================
	// 셈하는 이벤트.
	//==============================================================================
	fn create_counting_event(counter: &Rc<Cell<u32>>) -> UIButtonEvent {
		let counter = counter.clone();
		Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
			counter.set(counter.get() + 1);
		})
	}

	//==============================================================================
	// 버튼 반환.
	//==============================================================================
	fn get_button(node_tree: &mut NodeTree, button_id: NodeId) -> &mut UIButton {
		node_tree[button_id].get_component_mut::<UIButton>().expect("button")
	}

	//==============================================================================
	// 붙이면 노드가 interactable 이 되고 content 가 생긴다.
	//==============================================================================
	#[test]
	fn attach_makes_node_interactable() {
		let (mut node_tree, button_id) = create_button_tree();
		assert!(node_tree[button_id].is_interactable());
		let content_id = get_button(&mut node_tree, button_id).get_content().expect("content");
		assert_eq!(node_tree.get_parent_id(content_id), Some(button_id));
	}

	//==============================================================================
	// 누름 → 안에서 뗌: 클릭 한 번. 밖에서 뗌: 클릭 없음. 취소: 클릭 없음.
	//==============================================================================
	#[test]
	fn click_fires_only_on_release_inside() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, button_id) = create_button_tree();
		let click_count = Rc::new(Cell::new(0));
		let press_count = Rc::new(Cell::new(0));
		let release_count = Rc::new(Cell::new(0));
		{
			let button = get_button(&mut node_tree, button_id);
			button.set_click_event(create_counting_event(&click_count));
			button.set_pressed_event(create_counting_event(&press_count));
			button.set_released_event(create_counting_event(&release_count));
		}
		let inside = Vector2::create(210.0, 105.0);
		let outside = Vector2::create(400.0, 105.0);

		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		assert_eq!(get_button(&mut node_tree, button_id).get_button_state(), ButtonState::Pressed);
		assert_eq!(press_count.get(), 1);
		assert_eq!(click_count.get(), 0);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, inside);
		assert_eq!(click_count.get(), 1);
		assert_eq!(release_count.get(), 1);
		assert_eq!(get_button(&mut node_tree, button_id).get_button_state(), ButtonState::Normal);

		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, outside);
		assert_eq!(click_count.get(), 1);
		assert_eq!(release_count.get(), 2);

		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Cancel, inside);
		assert_eq!(get_button(&mut node_tree, button_id).get_button_state(), ButtonState::Normal);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, inside);
		assert_eq!(click_count.get(), 1);

		// 누르지 않은 뗌은 무시한다.
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, inside);
		assert_eq!(click_count.get(), 1);
		assert_eq!(release_count.get(), 2);
	}

	//==============================================================================
	// 사용 불가면 누름을 받지 않는다.
	//==============================================================================
	#[test]
	fn disabled_button_ignores_touch() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, button_id) = create_button_tree();
		let click_count = Rc::new(Cell::new(0));
		{
			let button = get_button(&mut node_tree, button_id);
			button.set_click_event(create_counting_event(&click_count));
			button.set_interactable(false);
			assert_eq!(button.get_button_state(), ButtonState::Disabled);
		}
		let inside = Vector2::create(210.0, 105.0);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, inside);
		assert_eq!(click_count.get(), 0);
	}

	//==============================================================================
	// 길게 누르면 한 번 알리고 그 누름의 클릭은 삼킨다.
	//==============================================================================
	#[test]
	fn long_press_swallows_click() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, button_id) = create_button_tree();
		let click_count = Rc::new(Cell::new(0));
		let long_press_count = Rc::new(Cell::new(0));
		{
			let button = get_button(&mut node_tree, button_id);
			button.set_click_event(create_counting_event(&click_count));
			button.set_long_pressed_event(create_counting_event(&long_press_count));
		}
		let inside = Vector2::create(210.0, 105.0);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		node_tree.tick(&engine, 0.3);
		assert_eq!(long_press_count.get(), 0);
		assert!((get_button(&mut node_tree, button_id).get_pressed_elapsed_seconds() - 0.3).abs() < 0.0001);
		node_tree.tick(&engine, 0.3);
		node_tree.tick(&engine, 0.3);
		assert_eq!(long_press_count.get(), 1);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, inside);
		assert_eq!(click_count.get(), 0);
		assert_eq!(get_button(&mut node_tree, button_id).get_pressed_elapsed_seconds(), 0.0);
	}

	//==============================================================================
	// 최소 눌림 시간을 채우지 못한 뗌은 미뤘다가 tick 에서 클릭한다.
	//==============================================================================
	#[test]
	fn minimum_pressed_seconds_delays_click() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, button_id) = create_button_tree();
		let click_count = Rc::new(Cell::new(0));
		{
			let button = get_button(&mut node_tree, button_id);
			button.set_click_event(create_counting_event(&click_count));
			button.set_minimum_pressed_seconds(0.2);
		}
		let inside = Vector2::create(210.0, 105.0);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, inside);
		assert_eq!(click_count.get(), 0);
		assert_eq!(get_button(&mut node_tree, button_id).get_button_state(), ButtonState::Pressed);
		node_tree.tick(&engine, 0.1);
		assert_eq!(click_count.get(), 0);
		node_tree.tick(&engine, 0.15);
		assert_eq!(click_count.get(), 1);
		assert_eq!(get_button(&mut node_tree, button_id).get_button_state(), ButtonState::Normal);
	}

	//==============================================================================
	// 누르고 있으면 형제 Paint 색이 눌림 틴트 쪽으로 차오르고, 떼면 원래 색으로 돌아온다. 제외한 컴포넌트는 그대로다.
	//==============================================================================
	#[test]
	fn press_tint_reaches_sibling_paint_and_child_text() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, button_id) = create_button_tree();
		let label_id = node_tree.create_child(button_id, "label");
		node_tree[label_id].add_component(Text::create_with_text("ok", 20.0));
		let excluded_id = node_tree.create_child(button_id, "excluded");
		node_tree[excluded_id].add_component(Paint::create_with_color(Color::white()));
		get_button(&mut node_tree, button_id).exclude_node_from_tint(excluded_id);

		let inside = Vector2::create(210.0, 105.0);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		node_tree.tick(&engine, 0.3);
		let pressed_paint_color = node_tree[button_id].get_component::<Paint>().expect("paint").get_color();
		assert!((pressed_paint_color.red - 0.7).abs() < 0.001);
		assert_eq!(pressed_paint_color.alpha, 1.0);
		let pressed_text_color = node_tree[label_id].get_component::<Text>().expect("text").get_text_color();
		assert!((pressed_text_color.green - 0.7).abs() < 0.001);
		let excluded_color = node_tree[excluded_id].get_component::<Paint>().expect("paint").get_color();
		assert_eq!(excluded_color, Color::white());

		node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, inside);
		let released_paint_color = node_tree[button_id].get_component::<Paint>().expect("paint").get_color();
		assert_eq!(released_paint_color, Color::white());
	}

	//==============================================================================
	// 형제 Paint 를 타입으로 제외하면 색이 바뀌지 않는다.
	//==============================================================================
	#[test]
	fn excluded_sibling_component_keeps_color() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, button_id) = create_button_tree();
		get_button(&mut node_tree, button_id).exclude_component_from_tint::<Paint>(button_id);
		let inside = Vector2::create(210.0, 105.0);
		node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, inside);
		node_tree.tick(&engine, 0.3);
		let paint_color = node_tree[button_id].get_component::<Paint>().expect("paint").get_color();
		assert_eq!(paint_color, Color::white());
	}
}
