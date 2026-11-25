//==============================================================================
// 터치 인식기. (원본 ui/touchrecognizer.js 의 TouchRecognizer)
// - 누른 노드의 부모 쪽에 UIScrollView(또는 UISnapScrollView)가 있으면, 끈 거리가 임계(10px)를 넘을 때 누른 노드의 누름을 취소하고
//   스크롤 뷰에 처음 누른 자리부터 누름 → 이동을 다시 보내 스크롤을 넘긴다. (스크롤 뷰 안의 버튼을 끌어도 스크롤된다)
// - 부모에 스크롤 뷰가 있으면 누른 노드에 가는 누름을 150ms 미룬다. 그 사이 드래그가 되면 누름 자체를 보내지 않는다.
//   짧게 톡 친 것은 뗄 때 미룬 누름을 바로 보낸 뒤 뗌을 잇는다. (UIKit 의 delaysContentTouches)
// - 스크롤 뷰가 set_passes_cross_axis_drag(true) 면 스크롤하지 않는 축으로 더 많이 끈 손가락은 넘기지 않는다.
// - 원본과 다른 것:
//   - 원본은 GameScene 이 늘 쓴다. 러스트 판의 SceneBase::tick_touch 는 원본 TouchRaycaster(맨 위 노드에 그대로 보냄)다.
//     이 손짓이 필요한 씬은 TouchRecognizer 를 들고, SceneBase::tick 대신 노드 갱신과 이것을 부른다:
//       self.scene_base.get_node_tree_mut().tick(engine, time_delta);
//       self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
//     그때 SceneBase 의 첫 터치 콜백과 get_touch_target 은 쓰지 않는다. (이쪽 get_touch_target 을 쓴다)
//   - 원본 setTimeout(150ms) 은 프레임 간격(배율을 곱하지 않은 시간)을 쌓아 센다.
//   - 스크롤 뷰에 넘기는 터치는 그 노드의 컴포넌트 모두에게 간다. (NodeTree::dispatch_touch)
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::engine::Engine;
use crate::core::node::nodetree::{NodeId, NodeTree, TouchKind};
use crate::ui::uiscrollbar::UIScrollBar;
use crate::ui::uisnapscrollview::find_scroll_view;

// 드래그로 바뀌었다고 볼 거리. (px)
const DRAG_THRESHOLD: f32 = 10.0;

// 부모에 스크롤 뷰가 있을 때 누른 노드에 가는 누름을 미루는 시간. (초, 원본 150ms)
const DELAY_PRESS_SECONDS: f32 = 0.15;

//==============================================================================
// 터치 인식기.
//==============================================================================
pub struct TouchRecognizer {
	target_id: Option<NodeId>,
	scroll_view_node_id: Option<NodeId>,
	press_position: Option<Vector2>,
	is_dragging: bool,
	press_delay_remain_seconds: Option<f32>,
	is_press_delivered: bool,
	is_touching: bool,
	// 이번 프레임의 휠을 받아 쓴 컴포넌트가 있었는지.
	is_wheel_handled: bool,
}

impl TouchRecognizer {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TouchRecognizer {
		TouchRecognizer {
			target_id: None,
			scroll_view_node_id: None,
			press_position: None,
			is_dragging: false,
			press_delay_remain_seconds: None,
			is_press_delivered: false,
			is_touching: false,
			is_wheel_handled: false,
		}
	}

	//==============================================================================
	// 이번 프레임의 휠을 받아 쓴 컴포넌트(스크롤 칸 등)가 있었는지. (tick_touch 뒤에 읽는다)
	//==============================================================================
	pub fn is_wheel_handled(&self) -> bool {
		self.is_wheel_handled
	}

	//==============================================================================
	// 지금 누름을 받는 노드. (드래그를 넘겼으면 스크롤 뷰 노드)
	//==============================================================================
	pub fn get_touch_target(&self) -> Option<NodeId> {
		if self.is_dragging {
			return self.scroll_view_node_id;
		}
		self.target_id
	}

	//==============================================================================
	// 누르고 있는 중인지. (누름부터 뗌이나 취소까지)
	//==============================================================================
	pub fn is_touching(&self) -> bool {
		self.is_touching
	}

	//==============================================================================
	// 터치 갱신. (SceneBase::tick_touch 자리. 휠 → 미룬 누름 → 누름 / 이동 / 뗌 / 취소)
	//==============================================================================
	pub fn tick_touch(&mut self, node_tree: &mut NodeTree, engine: &Engine) {
		let elapsed_seconds = engine.get_time_manager().get_unscale_delta_time();
		self.update_press_delay(node_tree, engine, elapsed_seconds);

		let input_manager = engine.get_input_manager();
		let view_input_position = input_manager.get_view_input_position();

		// 휠. (누름과 상관없이 그 자리의 가장 앞 스크롤 칸이나 누를 수 있는 노드부터 부모 쪽으로, NodeTree::dispatch_wheel_at)
		let wheel_delta_x = input_manager.get_wheel_delta_x();
		let wheel_delta_y = input_manager.get_wheel_delta_y();
		let wheel_delta = Vector2::create(wheel_delta_x, wheel_delta_y);
		self.is_wheel_handled = false;
		if wheel_delta.x != 0.0 || wheel_delta.y != 0.0 {
			self.is_wheel_handled = node_tree.dispatch_wheel_at(view_input_position, engine, wheel_delta);
		}

		if input_manager.is_touch_pressed() {
			self.touch_press(node_tree, engine, view_input_position);
		}
		if !self.is_touching {
			return;
		}
		// 원본 Scene.tickTouch 처럼 뗌이 먼저, 누르고 있음은 누른 프레임에는 넘기지 않는다.
		if input_manager.is_touch_released() {
			self.touch_release(node_tree, engine, view_input_position);
		}
		else if input_manager.is_touch_moved() && !input_manager.is_touch_pressed() {
			self.touch_move(node_tree, engine, view_input_position);
		}
		else if input_manager.is_touch_cancelled() {
			self.touch_cancel(node_tree, engine, view_input_position);
		}
	}

	//==============================================================================
	// 미룬 누름의 남은 시간을 줄이고, 다 되면 누른 노드에 누름을 보낸다. (원본 setTimeout 자리)
	//==============================================================================
	pub fn update_press_delay(&mut self, node_tree: &mut NodeTree, engine: &Engine, elapsed_seconds: f32) {
		let remain_seconds = match self.press_delay_remain_seconds {
			Some(remain_seconds) => {
				remain_seconds - elapsed_seconds
			}
			None => {
				return;
			}
		};
		if remain_seconds > 0.0 {
			self.press_delay_remain_seconds = Some(remain_seconds);
			return;
		}
		self.press_delay_remain_seconds = None;
		if self.is_dragging {
			return;
		}
		if let (Some(target_id), Some(press_position)) = (self.target_id, self.press_position) {
			self.is_press_delivered = true;
			node_tree.dispatch_touch(target_id, engine, TouchKind::Press, press_position);
		}
	}

	//==============================================================================
	// 터치 누름. (맞은 노드와 그 부모 쪽 스크롤 뷰를 찾아 두고, 스크롤 뷰가 있으면 누름을 미룬다)
	//==============================================================================
	pub fn touch_press(&mut self, node_tree: &mut NodeTree, engine: &Engine, view_input_position: Vector2) {
		let target_id = node_tree.find_topmost_interactable_node(view_input_position);
		self.target_id = target_id;
		self.press_position = Some(view_input_position);
		self.is_dragging = false;
		self.scroll_view_node_id = self.find_ancestor_scroll_view(node_tree, target_id);
		self.is_press_delivered = false;
		self.press_delay_remain_seconds = None;
		self.is_touching = true;

		let target_id = match target_id {
			Some(target_id) => {
				target_id
			}
			None => {
				return;
			}
		};
		if self.scroll_view_node_id.is_some() {
			self.press_delay_remain_seconds = Some(DELAY_PRESS_SECONDS);
		}
		else {
			self.is_press_delivered = true;
			node_tree.dispatch_touch(target_id, engine, TouchKind::Press, view_input_position);
		}
	}

	//==============================================================================
	// 터치 이동.
	// - 드래그를 넘겼으면 스크롤 뷰에만 보낸다.
	// - 스크롤 뷰가 있고 임계를 넘으면 넘긴다: 미룬 누름은 버리고(이미 보냈으면 누른 노드에 취소), 스크롤 뷰에 누름(처음 자리) → 이동.
	// - 그 밖에는 누른 노드에 이동. (누름을 아직 보내지 않았으면 이동도 보내지 않는다)
	//==============================================================================
	pub fn touch_move(&mut self, node_tree: &mut NodeTree, engine: &Engine, view_input_position: Vector2) {
		if self.is_dragging {
			if let Some(scroll_view_node_id) = self.scroll_view_node_id {
				node_tree.dispatch_touch(scroll_view_node_id, engine, TouchKind::Moved, view_input_position);
			}
			return;
		}

		if let (Some(scroll_view_node_id), Some(press_position)) = (self.scroll_view_node_id, self.press_position) {
			let delta_x = view_input_position.x - press_position.x;
			let delta_y = view_input_position.y - press_position.y;
			let is_over_threshold = delta_x.abs() > DRAG_THRESHOLD || delta_y.abs() > DRAG_THRESHOLD;
			let is_scroll_drag = match node_tree.get_node(scroll_view_node_id).and_then(find_scroll_view) {
				Some(scroll_view) => {
					scroll_view.is_scroll_drag(delta_x, delta_y)
				}
				None => {
					true
				}
			};
			if is_over_threshold && !is_scroll_drag {
				// 스크롤하지 않는 축의 드래그. 이 손가락은 끝까지 누른 노드의 것이다.
				self.scroll_view_node_id = None;
				if self.press_delay_remain_seconds.is_some() {
					self.press_delay_remain_seconds = None;
					if let Some(target_id) = self.target_id {
						self.is_press_delivered = true;
						node_tree.dispatch_touch(target_id, engine, TouchKind::Press, press_position);
					}
				}
			}
			else if is_over_threshold {
				self.is_dragging = true;
				// 미룬 누름이 있으면 버린다. 누름 자체를 보내지 않았으니 취소도 필요 없다.
				if self.press_delay_remain_seconds.is_some() {
					self.press_delay_remain_seconds = None;
				}
				else if self.is_press_delivered {
					if let Some(target_id) = self.target_id {
						node_tree.dispatch_touch(target_id, engine, TouchKind::Cancel, view_input_position);
					}
				}
				// 누른 노드는 더 이상 따라가지 않는다.
				self.target_id = None;
				self.is_press_delivered = false;
				// 스크롤 뷰가 자연스러운 시작 자리를 갖도록 처음 누른 자리부터 다시 보낸다.
				node_tree.dispatch_touch(scroll_view_node_id, engine, TouchKind::Press, press_position);
				node_tree.dispatch_touch(scroll_view_node_id, engine, TouchKind::Moved, view_input_position);
				return;
			}
		}

		if self.is_press_delivered {
			if let Some(target_id) = self.target_id {
				node_tree.dispatch_touch(target_id, engine, TouchKind::Moved, view_input_position);
			}
		}
	}

	//==============================================================================
	// 터치 뗌. (드래그면 스크롤 뷰에, 아니면 누른 노드에. 미룬 누름이 남았으면 그것을 먼저 보낸다)
	//==============================================================================
	pub fn touch_release(&mut self, node_tree: &mut NodeTree, engine: &Engine, view_input_position: Vector2) {
		if self.is_dragging {
			if let Some(scroll_view_node_id) = self.scroll_view_node_id {
				node_tree.dispatch_touch(scroll_view_node_id, engine, TouchKind::Release, view_input_position);
			}
		}
		else if let Some(target_id) = self.target_id {
			if self.press_delay_remain_seconds.is_some() {
				self.press_delay_remain_seconds = None;
				let press_position = self.press_position.unwrap_or(view_input_position);
				node_tree.dispatch_touch(target_id, engine, TouchKind::Press, press_position);
				self.is_press_delivered = true;
			}
			node_tree.dispatch_touch(target_id, engine, TouchKind::Release, view_input_position);
		}
		self.reset();
	}

	//==============================================================================
	// 터치 취소. (미룬 누름이 남았으면 누른 노드에는 취소를 보내지 않는다)
	//==============================================================================
	pub fn touch_cancel(&mut self, node_tree: &mut NodeTree, engine: &Engine, view_input_position: Vector2) {
		if self.is_dragging {
			if let Some(scroll_view_node_id) = self.scroll_view_node_id {
				node_tree.dispatch_touch(scroll_view_node_id, engine, TouchKind::Cancel, view_input_position);
			}
		}
		else if let Some(target_id) = self.target_id {
			if self.is_press_delivered {
				node_tree.dispatch_touch(target_id, engine, TouchKind::Cancel, view_input_position);
			}
		}
		self.reset();
	}

	//==============================================================================
	// 추적 상태 비우기.
	//==============================================================================
	pub fn reset(&mut self) {
		self.press_delay_remain_seconds = None;
		self.target_id = None;
		self.scroll_view_node_id = None;
		self.press_position = None;
		self.is_dragging = false;
		self.is_press_delivered = false;
		self.is_touching = false;
	}

	//==============================================================================
	// 노드의 부모 쪽에서 UIScrollView(또는 UISnapScrollView)가 붙은 가장 가까운 노드. (없으면 None)
	// - 누른 노드가 스크롤바면 자기가 다루는 스크롤 뷰는 빼고 찾는다. (스크롤바 드래그가 그 스크롤 뷰로 넘어가면 안 된다)
	//==============================================================================
	pub fn find_ancestor_scroll_view(&self, node_tree: &NodeTree, target_id: Option<NodeId>) -> Option<NodeId> {
		let target_id = target_id?;
		let owned_scroll_view_node_id = node_tree.get_node(target_id).and_then(|target_node| target_node.get_component::<UIScrollBar>()).and_then(|scroll_bar| scroll_bar.get_scroll_view());
		let mut current_id = node_tree.get_parent_id(target_id);
		while let Some(candidate_id) = current_id {
			let has_scroll_view = node_tree.get_node(candidate_id).map(|candidate_node| find_scroll_view(candidate_node).is_some()).unwrap_or(false);
			if has_scroll_view && Some(candidate_id) != owned_scroll_view_node_id {
				return Some(candidate_id);
			}
			current_id = node_tree.get_parent_id(candidate_id);
		}
		None
	}
}

impl Default for TouchRecognizer {
	fn default() -> TouchRecognizer {
		TouchRecognizer::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::component::paint::Paint;
	use crate::core::node::nodetree::ComponentContext;
	use crate::core::engine::EngineConfiguration;
	use crate::ui::uibutton::UIButton;
	use crate::ui::uiscrollview::UIScrollView;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (스크롤 뷰 (0,0 ~ 200,100), 콘텐트 안에 버튼 (0,0 ~ 200,40))
	//==============================================================================
	fn create_scroll_with_button(click_count: &Rc<Cell<u32>>) -> (NodeTree, NodeId, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let scroll_id = node_tree.create_child(root_id, "scroll");
		let scroll_node = &mut node_tree[scroll_id];
		scroll_node.set_pivot(Pivot::TOP_LEFT);
		scroll_node.set_content_size(Vector2::create(200.0, 100.0));
		let content_id = {
			let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_scroll_content_size(Vector2::create(200.0, 400.0));
			scroll_view.get_content().expect("content")
		};
		let button_id = node_tree.create_child(content_id, "button");
		let button_node = &mut node_tree[button_id];
		button_node.set_pivot(Pivot::TOP_LEFT);
		button_node.set_content_size(Vector2::create(200.0, 40.0));
		button_node.add_component(Paint::new());
		let click_count_for_event = click_count.clone();
		node_tree.add_component(button_id, UIButton::new()).expect("button").set_click_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
			click_count_for_event.set(click_count_for_event.get() + 1);
		}));
		(node_tree, scroll_id, button_id)
	}

	//==============================================================================
	// 짧게 톡 치면 미룬 누름을 보낸 뒤 뗌을 이어 버튼이 클릭된다.
	//==============================================================================
	#[test]
	fn short_tap_clicks_button_inside_scroll_view() {
		let engine = Engine::new(EngineConfiguration::new());
		let click_count = Rc::new(Cell::new(0));
		let (mut node_tree, scroll_id, button_id) = create_scroll_with_button(&click_count);
		let mut touch_recognizer = TouchRecognizer::new();
		let tap_position = Vector2::create(50.0, 20.0);
		touch_recognizer.touch_press(&mut node_tree, &engine, tap_position);
		assert_eq!(touch_recognizer.find_ancestor_scroll_view(&node_tree, Some(button_id)), Some(scroll_id));
		assert_eq!(touch_recognizer.get_touch_target(), Some(button_id));
		touch_recognizer.touch_release(&mut node_tree, &engine, tap_position);
		assert_eq!(click_count.get(), 1);
	}

	//==============================================================================
	// 버튼을 누르고 임계를 넘게 끌면 버튼은 클릭되지 않고 스크롤 뷰가 움직인다.
	//==============================================================================
	#[test]
	fn drag_over_button_scrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let click_count = Rc::new(Cell::new(0));
		let (mut node_tree, scroll_id, _button_id) = create_scroll_with_button(&click_count);
		let mut touch_recognizer = TouchRecognizer::new();
		touch_recognizer.touch_press(&mut node_tree, &engine, Vector2::create(50.0, 30.0));
		// 미룬 시간이 지나 누름이 버튼에 간 뒤에 끌어도 취소되고 넘어간다.
		touch_recognizer.update_press_delay(&mut node_tree, &engine, 0.2);
		touch_recognizer.touch_move(&mut node_tree, &engine, Vector2::create(50.0, 0.0));
		assert_eq!(touch_recognizer.get_touch_target(), Some(scroll_id));
		node_tree.tick(&engine, 0.016);
		let scroll_offset = node_tree[scroll_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_offset();
		assert_eq!(scroll_offset, Vector2::create(0.0, -30.0));
		touch_recognizer.touch_release(&mut node_tree, &engine, Vector2::create(50.0, 0.0));
		assert_eq!(click_count.get(), 0);
	}
}
