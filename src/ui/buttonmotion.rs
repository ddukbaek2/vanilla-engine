//==============================================================================
//! 단추 반응. (vanilla.js ui/buttonmotion.js 의 ButtonMotion, ButtonMotionStyle, attachButtonMotion, setButtonMotionArea, shakeButton)
//! - 단추 노드에 UIButton 다음으로 붙이면, 프레임마다 그 단추의 상태를 읽어 노드를 가운데를 두고 키우고 줄인다.
//!   올리면 커지고, 누르는 동안 줄고, 떼면 통 튀고, 누를 수 없을 때 shake 를 부르면 좌우로 흔들린다.
//! - 색은 건드리지 않는다. (글 단추의 바탕과 글자 색은 화면의 코드가 정하고, 그림 단추는 UIButton 의 틴트가 맡는다)
//! - 손가락이 주된 기기는 올림을 쓰지 않는다. (뗀 뒤에도 손가락 자리가 남아 올린 채로 보이지 않게)
//! - 반응이 없을 때는 노드의 자리와 크기를 건드리지 않아, 다른 곳이 같은 노드를 키우는 연출과 겹치지 않는다.
//! - 꼴은 set_style 로 단추마다 정하거나, 비워 두면 위 노드 가운데 ButtonMotionArea 를 건 가장 가까운 것을 따르고, 없으면 Normal 이다.
//! - Inset 꼴에서 올린 동안은 단추 바탕 Paint 의 색을 밝히고 안쪽에 흰 테를 그린다. (바탕 Paint 가 없거나 보이지 않는 단추는 밝히지 않는다)
//!   단추에 얹은 꾸밈이 제 모양으로 밝히게 하려면 set_lit_handler 로 함수를 건다. (그때 단추의 눌림 색은 투명으로 둔다)
//!   건 함수는 `Rc<RefCell>` 에 담는다. get_lit_handler 는 그 Rc 를 돌려주고, 부르는 쪽은 borrow_mut 로 빌려 부른다.
//! - 원본과 다른 것: 가운데를 두는 셈이 피봇을 읽는다. (원본은 왼쪽 위 피봇만), 꼴을 거는 표시는 ButtonMotionArea 컴포넌트다. (원본은 WeakMap)
//!   밝힌 바탕과 흰 테를 자식 노드로 두지 않는다. (원본은 맨 밑 자식 둘, 러스트 예제처럼 글이 같은 노드의 컴포넌트면 자식이 글을 덮는다)
//==============================================================================
use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;

use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uibutton::{ButtonState, UIButton};
use crate::ui::uiscrollview::UIScrollView;
use crate::ui::uiview::is_view_position_inside_node;

const RELEASE_SECONDS: f32 = 0.15;
/// 밝힐 때 바탕을 흰색 쪽으로 섞는 비와 안쪽 테의 굵기와 색.
const LIT_LIGHTEN: f32 = 0.1;
const LIT_RING_WIDTH: f32 = 2.0;
const LIT_RING_COLOR: Color = Color::create(1.0, 1.0, 1.0, 0.4);
/// 누를 수 없을 때 좌우로 흔들리는 폭과 초, 왕복 수.
const SHAKE_DISTANCE: f32 = 2.0;
const SHAKE_SECONDS: f32 = 0.25;
const SHAKE_COUNT: f32 = 3.0;
/// 크기가 이만큼 보통에 가까우면 다 돌아온 것으로 본다.
const SCALE_EPSILON: f32 = 0.001;

/// 밝힘을 맡는 함수. (문맥, 밝히는지, 누르는 중인지)
pub type ButtonLitHandler = Box<dyn FnMut(&mut ComponentContext, bool, bool)>;

//==============================================================================
/// 단추 반응의 꼴.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonMotionStyle {
	/// 올리면 커지고, 누르는 동안 줄고, 떼면 통 튄다.
	Normal,
	/// 창 안의 단추. 1 배를 넘게 커지지 않아 창 안쪽 여백을 넘지 않는다. 올리면 바탕이 밝아지고 안쪽에 흰 테가 서며, 누르면 조금 줄었다 돌아온다.
	Inset,
}

// 단추 반응 꼴의 반사 값. (원본 ButtonMotionStyle 글)
crate::core::reflection::impl_reflect_names!(ButtonMotionStyle, [
	(ButtonMotionStyle::Normal, "normal"),
	(ButtonMotionStyle::Inset, "inset"),
]);

//==============================================================================
/// 꼴마다의 값. (올렸을 때, 누르는 동안의 크기와 그리로 가는 초, 떼었을 때 통 튀는 크기, 올렸을 때 밝히는지)
//==============================================================================
struct MotionValues {
	hover_scale: f32,
	hover_seconds: f32,
	press_scale: f32,
	press_seconds: f32,
	release_scale: f32,
	is_hover_lit: bool,
}

impl ButtonMotionStyle {
	fn read_values(&self) -> MotionValues {
		match self {
			ButtonMotionStyle::Normal => {
				MotionValues { hover_scale: 1.06, hover_seconds: 0.08, press_scale: 0.92, press_seconds: 0.05, release_scale: 1.04, is_hover_lit: false }
			}
			ButtonMotionStyle::Inset => {
				MotionValues { hover_scale: 1.0, hover_seconds: 0.08, press_scale: 0.96, press_seconds: 0.08, release_scale: 1.0, is_hover_lit: true }
			}
		}
	}
}

//==============================================================================
/// 노드 밑의 단추가 따를 꼴을 거는 표시. (창이 제 판에 건다)
//==============================================================================
pub struct ButtonMotionArea {
	component_base: ComponentBase,
	style: ButtonMotionStyle,
}

#[crate::reflect]
impl ButtonMotionArea {
	pub fn new(style: ButtonMotionStyle) -> ButtonMotionArea {
		ButtonMotionArea { component_base: ComponentBase::new(), style }
	}

	pub fn get_style(&self) -> ButtonMotionStyle {
		self.style
	}
}

#[crate::reflect]
impl Component for ButtonMotionArea {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ButtonMotionArea"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}
}

//==============================================================================
/// 노드 밑의 단추가 따를 꼴 걸기. (창이 제 판에 Inset 을 걸면 그 안의 단추는 커지지 않는다, 단추가 set_style 로 정한 꼴이 먼저)
//==============================================================================
pub fn set_button_motion_area(node_tree: &mut NodeTree, node_id: NodeId, style: ButtonMotionStyle) {
	if let Some(node) = node_tree.get_node_mut(node_id) {
		if let Some(area) = node.get_component_mut::<ButtonMotionArea>() {
			area.style = style;
			return;
		}
	}
	node_tree.add_component(node_id, ButtonMotionArea::new(style));
}

pub struct ButtonMotion {
	component_base: ComponentBase,
	scale: f32,
	release_seconds: f32,
	shake_seconds: f32,
	was_pressed: bool,
	is_moving: bool,
	base_position: Option<Vector2>,
	applied_position: Option<Vector2>,
	style: Option<ButtonMotionStyle>,
	base_paint_color: Option<Color>,
	applied_lit_color: Option<Color>,
	lit_round_size: f32,
	is_lit: bool,
	pressed_tint_color: Option<Color>,
	is_press_tint_muted: bool,
	was_input_held: bool,
	scroll_hold: Option<Vector2>,
	lit_handler: Option<Rc<RefCell<ButtonLitHandler>>>,
}

#[crate::reflect]
impl ButtonMotion {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> ButtonMotion {
		ButtonMotion {
			component_base: ComponentBase::new(),
			scale: 1.0,
			release_seconds: 0.0,
			shake_seconds: 0.0,
			was_pressed: false,
			is_moving: false,
			base_position: None,
			applied_position: None,
			style: None,
			base_paint_color: None,
			applied_lit_color: None,
			lit_round_size: 0.0,
			is_lit: false,
			pressed_tint_color: None,
			is_press_tint_muted: false,
			was_input_held: false,
			scroll_hold: None,
			lit_handler: None,
		}
	}

	//==============================================================================
	/// 이 단추의 꼴 설정. (None 이면 위 노드에 건 꼴을 따른다)
	//==============================================================================
	pub fn set_style(&mut self, style: Option<ButtonMotionStyle>) {
		self.style = style;
	}

	//==============================================================================
	/// 누를 수 없을 때 흔들기. (누른 쪽에서 부른다)
	//==============================================================================
	pub fn shake(&mut self) {
		self.shake_seconds = SHAKE_SECONDS;
	}

	//==============================================================================
	/// 밝힘을 맡는 함수 설정. (None 이면 이 부품이 흰 테 판과 밝힌 바탕 판을 둔다)
	//==============================================================================
	pub fn set_lit_handler(&mut self, lit_handler: Option<ButtonLitHandler>) {
		self.lit_handler = lit_handler.map(|lit_handler| Rc::new(RefCell::new(lit_handler)));
	}

	//==============================================================================
	/// 밝힘을 맡는 함수 반환. (부를 때는 borrow_mut 로 빌린다)
	//==============================================================================
	pub fn get_lit_handler(&self) -> Option<Rc<RefCell<ButtonLitHandler>>> {
		self.lit_handler.clone()
	}

	//==============================================================================
	/// 이 단추가 따를 꼴. (정한 것, 없으면 위 노드에 건 가장 가까운 꼴, 그것도 없으면 Normal)
	//==============================================================================
	fn read_motion_style(&self, node_tree: &NodeTree, node_id: NodeId) -> ButtonMotionStyle {
		if let Some(style) = self.style {
			return style;
		}
		let mut area_id = node_tree.get_parent_id(node_id);
		while let Some(current_id) = area_id {
			if let Some(area) = node_tree.get_node(current_id).and_then(|node| node.get_component::<ButtonMotionArea>()) {
				return area.get_style();
			}
			area_id = node_tree.get_parent_id(current_id);
		}
		ButtonMotionStyle::Normal
	}

	//==============================================================================
	/// 손(마우스나 손가락)이 노드 위인지.
	//==============================================================================
	fn is_input_over(context: &ComponentContext) -> bool {
		let view_input_position = context.get_engine().get_input_manager().get_view_input_position();
		is_view_position_inside_node(context.get_node_tree(), context.get_node_id(), view_input_position)
	}

	//==============================================================================
	/// 스크롤 칸 안에서 누르고 있는지.
	/// - 엔진은 스크롤 칸 안 단추에 누르는 동안 눌림을 주지 않으므로, 누름이 이 단추 위에서 시작했고 손이 아직 단추 위이며
	///   스크롤 칸이 밀리지 않았으면 눌린 것으로 그린다. 밀리기 시작하면 푼다.
	//==============================================================================
	fn read_scroll_pressed(&mut self, context: &ComponentContext, button_state: ButtonState) -> bool {
		let node_tree = context.get_node_tree();
		let mut scroll_offset: Option<Vector2> = None;
		let mut parent_id = node_tree.get_parent_id(context.get_node_id());
		while let Some(current_id) = parent_id {
			if let Some(scroll_view) = node_tree.get_node(current_id).and_then(|node| node.get_component::<UIScrollView>()) {
				scroll_offset = Some(scroll_view.get_scroll_offset());
				break;
			}
			parent_id = node_tree.get_parent_id(current_id);
		}
		let scroll_offset = match scroll_offset {
			Some(scroll_offset) => {
				scroll_offset
			}
			None => {
				return false;
			}
		};
		let is_input_held = context.get_engine().get_input_manager().is_touch_down();
		let is_hold_started = is_input_held && !self.was_input_held;
		self.was_input_held = is_input_held;
		if !is_input_held || button_state == ButtonState::Disabled {
			self.scroll_hold = None;
			return false;
		}
		if is_hold_started {
			self.scroll_hold = if ButtonMotion::is_input_over(context) { Some(scroll_offset) } else { None };
		}
		let scroll_hold = match self.scroll_hold {
			Some(scroll_hold) => {
				scroll_hold
			}
			None => {
				return false;
			}
		};
		if (scroll_offset.x - scroll_hold.x).abs() > 1.0 || (scroll_offset.y - scroll_hold.y).abs() > 1.0 {
			self.scroll_hold = None;
			return false;
		}
		ButtonMotion::is_input_over(context)
	}

	//==============================================================================
	/// 단추의 눌림 색을 투명으로 바꾸거나 되돌리기. (밝히는 함수가 눌림을 그리는 동안 투명)
	//==============================================================================
	fn mute_press_tint(&mut self, context: &mut ComponentContext, is_muted: bool) {
		if is_muted == self.is_press_tint_muted {
			return;
		}
		let button = match context.get_node_mut().get_component_mut::<UIButton>() {
			Some(button) => {
				button
			}
			None => {
				return;
			}
		};
		let saved_tint_color = *self.pressed_tint_color.get_or_insert(button.get_pressed_tint_color());
		let tint_color = if is_muted { saved_tint_color.with_alpha(0.0) } else { saved_tint_color };
		button.set_pressed_tint_color(tint_color);
		self.is_press_tint_muted = is_muted;
	}

	//==============================================================================
	/// 밝히기. (단추 바탕 Paint 의 색을 흰색 쪽으로 섞고, 안쪽 흰 테는 draw 가 그린다)
	/// - 밝힌 동안 화면의 코드가 바탕색을 바꾸면(지금 색이 이 부품이 넣은 색과 다르면) 그 색을 새 바탕으로 삼는다.
	/// - 끌 때는 바탕색으로 되돌린다.
	//==============================================================================
	fn place_lit_parts(&mut self, context: &mut ComponentContext, is_lit: bool, is_pressed: bool) {
		let has_lit_handler = self.lit_handler.is_some();
		self.mute_press_tint(context, has_lit_handler);
		if let Some(lit_handler) = self.get_lit_handler() {
			if let Ok(mut lit_function) = lit_handler.try_borrow_mut() {
				(*lit_function)(context, is_lit, is_pressed);
			}
			return;
		}
		let paint = match context.get_node_mut().get_component_mut::<Paint>() {
			Some(paint) => {
				paint
			}
			None => {
				self.is_lit = false;
				return;
			}
		};
		let current_color = paint.get_color();
		if self.applied_lit_color != Some(current_color) {
			self.base_paint_color = None;
			self.applied_lit_color = None;
		}
		if !is_lit || current_color.alpha <= 0.0 {
			if let (Some(base_paint_color), true) = (self.base_paint_color, self.applied_lit_color.is_some()) {
				paint.set_color(base_paint_color);
			}
			self.base_paint_color = None;
			self.applied_lit_color = None;
			self.is_lit = false;
			return;
		}
		let base_color = *self.base_paint_color.get_or_insert(current_color);
		let lit_color = Color::create(base_color.red + (1.0 - base_color.red) * LIT_LIGHTEN, base_color.green + (1.0 - base_color.green) * LIT_LIGHTEN, base_color.blue + (1.0 - base_color.blue) * LIT_LIGHTEN, base_color.alpha);
		paint.set_color(lit_color);
		self.applied_lit_color = Some(lit_color);
		self.lit_round_size = paint.get_round_size();
		self.is_lit = true;
	}

	//==============================================================================
	/// 노드를 가운데를 두고 그 크기로 놓기. (커진 만큼 피봇에 맞춰 자리를 옮긴다, 그 사이 다시 놓았으면 그 자리를 새 기준으로)
	//==============================================================================
	fn place_around(&mut self, context: &mut ComponentContext, scale: f32, shake_offset: f32) {
		let transform = context.get_transform_mut();
		let local_position = transform.get_local_position();
		if self.applied_position != Some(local_position) || self.base_position.is_none() {
			self.base_position = Some(local_position);
		}
		let base_position = self.base_position.unwrap_or(local_position);
		let content_size = transform.get_content_size();
		let pivot = transform.get_pivot();
		let position = Vector2::create(base_position.x - content_size.x * (scale - 1.0) * (0.5 - pivot.x) + shake_offset, base_position.y - content_size.y * (scale - 1.0) * (0.5 - pivot.y));
		transform.set_local_position(position);
		transform.set_local_scale(Vector2::create(scale, scale));
		self.applied_position = Some(position);
	}
}

impl Default for ButtonMotion {
	fn default() -> ButtonMotion {
		ButtonMotion::new()
	}
}

#[crate::reflect]
impl Component for ButtonMotion {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ButtonMotion"
	}

	//==============================================================================
	/// 갱신. (단추 상태에 따라 크기를 맞추고, 노드의 가운데를 두고 놓는다)
	//==============================================================================
	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (밝힌 동안 안쪽 흰 테)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if !self.is_lit || self.lit_handler.is_some() {
			return;
		}
		let content_size = transform.get_content_size();
		let half_width = LIT_RING_WIDTH * 0.5;
		let rect = Rect::create(half_width, half_width, (content_size.x - LIT_RING_WIDTH).max(0.0), (content_size.y - LIT_RING_WIDTH).max(0.0));
		graphic.set_stroke_color(LIT_RING_COLOR);
		graphic.draw_stroke_round_rect(rect, (self.lit_round_size - half_width).max(0.0), LIT_RING_WIDTH);
	}

	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		let button_state = match context.get_node().get_component::<UIButton>() {
			Some(button) => {
				button.get_button_state()
			}
			None => {
				return;
			}
		};
		let is_coarse_pointer = context.get_engine().is_coarse_pointer();
		let is_scroll_pressed = self.read_scroll_pressed(context, button_state);
		let is_pressed = button_state == ButtonState::Pressed || is_scroll_pressed;
		let is_hovered = button_state == ButtonState::Hover && !is_coarse_pointer;

		// 꼴은 반응하는 동안만 읽는다. (위 노드를 거슬러 오르므로 가만히 있는 단추는 건너뛴다)
		let is_reacting = is_pressed || is_hovered || self.was_pressed || self.is_moving || self.is_lit;
		let motion_style = if is_reacting { self.read_motion_style(context.get_node_tree(), context.get_node_id()) } else { ButtonMotionStyle::Normal };
		let motion = motion_style.read_values();

		// 눌렀다 떼었는데 손이 아직 단추 위면 누른 것이라 통 튄다. (밖으로 끌고 나가 뗀 것은 누르지 않은 것)
		if self.was_pressed && !is_pressed && button_state != ButtonState::Disabled && ButtonMotion::is_input_over(context) && motion.release_scale > 1.0 {
			self.release_seconds = RELEASE_SECONDS;
		}
		self.was_pressed = is_pressed;
		let mut target_scale = 1.0;
		let mut seconds = motion.hover_seconds;
		if is_pressed {
			target_scale = motion.press_scale;
			seconds = motion.press_seconds;
		}
		else if is_hovered {
			target_scale = motion.hover_scale;
		}
		let is_lit = motion.is_hover_lit && !is_coarse_pointer && (is_hovered || is_pressed);
		self.place_lit_parts(context, is_lit, is_pressed);
		let step = if time_delta > 0.0 { (time_delta / seconds).min(1.0) } else { 0.0 };
		self.scale += (target_scale - self.scale) * step;
		let mut scale = self.scale;
		if self.release_seconds > 0.0 {
			self.release_seconds = (self.release_seconds - time_delta).max(0.0);
			let release_ratio = 1.0 - self.release_seconds / RELEASE_SECONDS;
			scale += (ButtonMotionStyle::Normal.read_values().release_scale - 1.0) * (release_ratio * PI).sin();
		}
		let mut shake_offset = 0.0;
		if self.shake_seconds > 0.0 {
			self.shake_seconds = (self.shake_seconds - time_delta).max(0.0);
			let shake_ratio = 1.0 - self.shake_seconds / SHAKE_SECONDS;
			shake_offset = (shake_ratio * PI * 2.0 * SHAKE_COUNT).sin() * SHAKE_DISTANCE * (1.0 - shake_ratio);
		}
		let is_resting = (scale - 1.0).abs() < SCALE_EPSILON && self.release_seconds <= 0.0 && self.shake_seconds <= 0.0 && target_scale == 1.0;
		if is_resting && !self.is_moving {
			return;
		}
		self.place_around(context, if is_resting { 1.0 } else { scale }, if is_resting { 0.0 } else { shake_offset });
		self.is_moving = !is_resting;
		if is_resting {
			self.scale = 1.0;
			self.base_position = None;
			self.applied_position = None;
		}
	}
}

//==============================================================================
/// 단추 노드에 반응 붙이기. (UIButton 이 이미 붙은 노드, 붙인 ButtonMotion 을 돌려준다. 원본 attachButtonMotion)
//==============================================================================
pub fn attach_button_motion(node_tree: &mut NodeTree, node_id: NodeId) -> Option<&mut ButtonMotion> {
	node_tree.add_component(node_id, ButtonMotion::new())
}

//==============================================================================
/// 단추 흔들기. (노드의 ButtonMotion 을 찾아 흔든다, 없으면 아무 일도 없다)
//==============================================================================
pub fn shake_button(node_tree: &mut NodeTree, node_id: NodeId) {
	if let Some(button_motion) = node_tree.get_node_mut(node_id).and_then(|node| node.get_component_mut::<ButtonMotion>()) {
		button_motion.shake();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 건 밝힘 함수를 get_lit_handler 로 돌려받고, None 을 걸면 비운다.
	//==============================================================================
	#[test]
	fn lit_handler_getter() {
		let mut button_motion = ButtonMotion::new();
		assert!(button_motion.get_lit_handler().is_none());
		button_motion.set_lit_handler(Some(Box::new(|_context: &mut ComponentContext, _is_lit: bool, _is_pressed: bool| {})));
		assert!(button_motion.get_lit_handler().is_some());
		button_motion.set_lit_handler(None);
		assert!(button_motion.get_lit_handler().is_none());
	}
}
