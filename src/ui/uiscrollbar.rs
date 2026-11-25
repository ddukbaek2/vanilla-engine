//==============================================================================
// 스크롤바. (원본 ui/uiscrollbar.js 의 UIScrollBar, ScrollBarAxis)
// - 대상 스크롤 뷰의 스크롤 상태를 트랙(바탕) + 썸(끄는 손잡이)으로 보인다. 썸이나 트랙을 끌면 대상의 오프셋을 바로 바꾼다.
// - 매 tick 대상의 오프셋 / 콘텐트 크기 / 보이는 영역을 읽어 썸 위치와 크기를 맞춘다.
// - 원본과 다른 것:
//   - 원본 UIScrollBar 는 WorldNode 를 상속한 노드이고, 트랙 Paint 와 썸 노드(Paint)를 자기 아래에 만든다.
//     러스트 판은 노드에 붙는 컴포넌트이고, 트랙과 썸을 직접 그린다. (썸 노드 없이 썸 사각형만 든다)
//   - 대상은 스크롤 뷰 컴포넌트 대신 UIScrollView 가 붙은 노드의 핸들이다. (set_scroll_view / get_scroll_view)
//   - 트랙 누름 자리와 썸 안 판정은 노드 행렬의 역으로 바꾼 로컬 좌표로 한다. (원본은 월드 위치를 뺀다. 회전 / 크기가 없으면 같다)
//   - 썸 색은 원본이 0 ~ 255 를 섞어 쓴 Color(255, 255, 255, 0.6) 이다. 러스트 판은 같은 뜻의 (1, 1, 1, 0.6) 으로 둔다.
// - UI 문서(UIDocument)는 register_node_type 으로 등록하면 원본 노드 꼴로 쓰고 읽는다. (노드 type "UIScrollBar", 트랙 Paint, 썸 노드, uidocument.rs 의 UIDocumentNode)
// - 보통은 UIScrollView::set_shows_vertical_scroll_bar 가 만든다. 직접 만들 때는 node_tree.add_component 로 붙인다. (피봇 / 앵커 왼쪽 위, interactable)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiscrollview::UIScrollView;

// 트랙 / 썸 둥근 크기.
const SCROLLBAR_ROUND_SIZE: f32 = 4.0;

//==============================================================================
// 스크롤바 축.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollBarAxis {
	Horizontal,
	Vertical,
}

// 스크롤바 축의 반사 값. (원본 ScrollBarAxis 글)
crate::core::reflection::impl_reflect_names!(ScrollBarAxis, [
	(ScrollBarAxis::Horizontal, "horizontal"),
	(ScrollBarAxis::Vertical, "vertical"),
]);

//==============================================================================
// 대상 스크롤 상태. (스크롤바가 tick / 터치 때 대상 노드에서 읽는다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ScrollState {
	viewport_size: Vector2,
	scroll_content_size: Vector2,
	scroll_offset: Vector2,
}

//==============================================================================
// 축 길이 계산 결과.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ThumbMetrics {
	thumb_length: f32,
	scrollable_content: f32,
	scrollable_track: f32,
}

//==============================================================================
// 스크롤바.
//==============================================================================
pub struct UIScrollBar {
	component_base: ComponentBase,
	scroll_view_node_id: Option<NodeId>,
	axis: ScrollBarAxis,
	track_color: Color,
	thumb_color: Color,
	thumb_rect: Option<Rect>,
	is_dragging_thumb: bool,
	drag_start_view_position: Vector2,
	drag_start_scroll_offset: Vector2,
	min_thumb_size: f32,
	auto_hide: bool,
}

#[crate::reflect]
impl UIScrollBar {
	//==============================================================================
	// 생성. (세로, 트랙 옅은 검정, 썸 반투명 흰색, 썸 최소 길이 24, 자동 숨김)
	//==============================================================================
	pub fn new() -> UIScrollBar {
		UIScrollBar {
			component_base: ComponentBase::new(),
			scroll_view_node_id: None,
			axis: ScrollBarAxis::Vertical,
			track_color: Color::create(0.0, 0.0, 0.0, 0.15),
			thumb_color: Color::create(1.0, 1.0, 1.0, 0.6),
			thumb_rect: None,
			is_dragging_thumb: false,
			drag_start_view_position: Vector2::zero(),
			drag_start_scroll_offset: Vector2::zero(),
			min_thumb_size: 24.0,
			auto_hide: true,
		}
	}

	//==============================================================================
	// 대상 스크롤 뷰 설정. (UIScrollView 가 붙은 노드)
	//==============================================================================
	pub fn set_scroll_view(&mut self, scroll_view_node_id: Option<NodeId>) {
		self.scroll_view_node_id = scroll_view_node_id;
	}

	//==============================================================================
	// 대상 스크롤 뷰 반환. (TouchRecognizer 가 자기 스크롤 뷰로 드래그를 넘기지 않으려고 읽는다)
	//==============================================================================
	pub fn get_scroll_view(&self) -> Option<NodeId> {
		self.scroll_view_node_id
	}

	//==============================================================================
	// 축 설정.
	//==============================================================================
	pub fn set_axis(&mut self, axis: ScrollBarAxis) {
		self.axis = axis;
	}

	//==============================================================================
	// 축 반환.
	//==============================================================================
	pub fn get_axis(&self) -> ScrollBarAxis {
		self.axis
	}

	//==============================================================================
	// 트랙 색 설정.
	//==============================================================================
	pub fn set_track_color(&mut self, color: impl Into<Color>) {
		self.track_color = color.into();
	}

	//==============================================================================
	// 썸 색 설정.
	//==============================================================================
	pub fn set_thumb_color(&mut self, color: impl Into<Color>) {
		self.thumb_color = color.into();
	}

	//==============================================================================
	// 트랙 색 반환. (원본은 트랙 Paint 컴포넌트의 색이다. 러스트 판은 직접 그려 색을 든다)
	//==============================================================================
	pub fn get_track_color(&self) -> Color {
		self.track_color
	}

	//==============================================================================
	// 썸 색 반환. (원본은 썸 노드 Paint 컴포넌트의 색이다)
	//==============================================================================
	pub fn get_thumb_color(&self) -> Color {
		self.thumb_color
	}

	//==============================================================================
	// 썸 최소 길이 설정.
	//==============================================================================
	pub fn set_min_thumb_size(&mut self, min_thumb_size: f32) {
		self.min_thumb_size = min_thumb_size;
	}

	//==============================================================================
	// 자동 숨김 설정. (콘텐트가 보이는 영역에 들어오면 트랙까지 숨긴다)
	//==============================================================================
	pub fn set_auto_hide(&mut self, auto_hide: bool) {
		self.auto_hide = auto_hide;
	}

	//==============================================================================
	// 지금 썸 사각형. (스크롤바 노드 영역 안, 썸이 꺼져 있으면 None)
	//==============================================================================
	pub fn get_thumb_rect(&self) -> Option<Rect> {
		self.thumb_rect
	}

	//==============================================================================
	// 대상 스크롤 상태 읽기.
	//==============================================================================
	fn read_scroll_state(&self, node_tree: &NodeTree) -> Option<ScrollState> {
		let scroll_view_node_id = self.scroll_view_node_id?;
		let scroll_node = node_tree.get_node(scroll_view_node_id)?;
		let scroll_view = scroll_node.get_component::<UIScrollView>()?;
		Some(ScrollState {
			viewport_size: scroll_node.get_content_size(),
			scroll_content_size: scroll_view.get_scroll_content_size(),
			scroll_offset: scroll_view.get_scroll_offset(),
		})
	}

	//==============================================================================
	// 축 방향 값 고르기.
	//==============================================================================
	fn pick_axis(&self, vector: Vector2) -> f32 {
		match self.axis {
			ScrollBarAxis::Vertical => {
				vector.y
			}
			ScrollBarAxis::Horizontal => {
				vector.x
			}
		}
	}

	//==============================================================================
	// 썸 길이와 스크롤 가능한 길이. (스크롤할 것이 없으면 None)
	//==============================================================================
	fn calculate_thumb_metrics(&self, scroll_state: &ScrollState, track_size: Vector2) -> Option<ThumbMetrics> {
		let viewport_length = self.pick_axis(scroll_state.viewport_size);
		let content_length = self.pick_axis(scroll_state.scroll_content_size);
		let track_length = self.pick_axis(track_size);
		if content_length <= viewport_length || track_length <= 0.0 {
			return None;
		}
		let ratio = viewport_length / content_length;
		let thumb_length = self.min_thumb_size.max(track_length * ratio);
		Some(ThumbMetrics {
			thumb_length,
			scrollable_content: content_length - viewport_length,
			scrollable_track: track_length - thumb_length,
		})
	}

	//==============================================================================
	// 트랙 길이 / 보이는 영역 / 콘텐트 크기로 썸 위치와 크기를 맞춘다.
	//==============================================================================
	fn update_thumb(&mut self, context: &mut ComponentContext) {
		let node_tree = context.get_node_tree();
		let scroll_state = match self.read_scroll_state(node_tree) {
			Some(scroll_state) => {
				scroll_state
			}
			None => {
				self.thumb_rect = None;
				return;
			}
		};
		let track_size = context.get_transform().get_content_size();
		let thumb_metrics = match self.calculate_thumb_metrics(&scroll_state, track_size) {
			Some(thumb_metrics) => {
				thumb_metrics
			}
			None => {
				// 콘텐트가 보이는 영역 안이면 스크롤할 것이 없다.
				self.thumb_rect = None;
				if self.auto_hide {
					context.get_node_mut().set_active(false);
				}
				return;
			}
		};
		context.get_node_mut().set_active(true);

		let offset_along_axis = self.pick_axis(scroll_state.scroll_offset);
		let progress = if thumb_metrics.scrollable_content > 0.0 {
			math::clamp01(-offset_along_axis / thumb_metrics.scrollable_content)
		}
		else {
			0.0
		};
		let thumb_position_along_axis = thumb_metrics.scrollable_track * progress;
		let thumb_rect = match self.axis {
			ScrollBarAxis::Vertical => {
				Rect::create(0.0, thumb_position_along_axis, track_size.x, thumb_metrics.thumb_length)
			}
			ScrollBarAxis::Horizontal => {
				Rect::create(thumb_position_along_axis, 0.0, thumb_metrics.thumb_length, track_size.y)
			}
		};
		self.thumb_rect = Some(thumb_rect);
	}

	//==============================================================================
	// 뷰 좌표 → 스크롤바 노드 로컬 좌표.
	//==============================================================================
	fn convert_to_local_position(context: &ComponentContext, view_input_position: Vector2) -> Option<Vector2> {
		let node_id = context.get_node_id();
		context.get_node_tree().convert_view_position_to_node_position(node_id, view_input_position)
	}

	//==============================================================================
	// 트랙을 누른 자리로 콘텐트 오프셋을 옮긴다. (썸 가운데가 누른 자리에 오도록)
	//==============================================================================
	fn apply_track_hit_position(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		let scroll_view_node_id = match self.scroll_view_node_id {
			Some(scroll_view_node_id) => {
				scroll_view_node_id
			}
			None => {
				return;
			}
		};
		let node_tree = context.get_node_tree();
		let scroll_state = match self.read_scroll_state(node_tree) {
			Some(scroll_state) => {
				scroll_state
			}
			None => {
				return;
			}
		};
		let track_size = context.get_transform().get_content_size();
		let thumb_metrics = match self.calculate_thumb_metrics(&scroll_state, track_size) {
			Some(thumb_metrics) => {
				thumb_metrics
			}
			None => {
				return;
			}
		};
		if thumb_metrics.scrollable_track <= 0.0 {
			return;
		}
		let local_position = match UIScrollBar::convert_to_local_position(context, view_input_position) {
			Some(local_position) => {
				local_position
			}
			None => {
				return;
			}
		};
		let local_on_axis = self.pick_axis(local_position);
		let desired_thumb_head = math::clamp(local_on_axis - thumb_metrics.thumb_length * 0.5, 0.0, thumb_metrics.scrollable_track);
		let progress = desired_thumb_head / thumb_metrics.scrollable_track;
		let new_offset_on_axis = -progress * thumb_metrics.scrollable_content;
		let current_offset = scroll_state.scroll_offset;
		let new_offset = match self.axis {
			ScrollBarAxis::Vertical => {
				Vector2::create(current_offset.x, new_offset_on_axis)
			}
			ScrollBarAxis::Horizontal => {
				Vector2::create(new_offset_on_axis, current_offset.y)
			}
		};
		let node_tree = context.get_node_tree_mut();
		UIScrollView::set_scroll_offset_in_tree(node_tree, scroll_view_node_id, new_offset);
	}

	//==============================================================================
	// 뷰 좌표가 썸 안인지.
	//==============================================================================
	fn is_inside_thumb(&self, context: &ComponentContext, view_input_position: Vector2) -> bool {
		let thumb_rect = match self.thumb_rect {
			Some(thumb_rect) => {
				thumb_rect
			}
			None => {
				return false;
			}
		};
		match UIScrollBar::convert_to_local_position(context, view_input_position) {
			Some(local_position) => {
				thumb_rect.contains(local_position)
			}
			None => {
				false
			}
		}
	}
}

impl Default for UIScrollBar {
	fn default() -> UIScrollBar {
		UIScrollBar::new()
	}
}

#[crate::reflect]
impl Component for UIScrollBar {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UIScrollBar"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자처럼 피봇 / 앵커 왼쪽 위, interactable)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		let scroll_bar_node = &mut node_tree[node_id];
		scroll_bar_node.set_pivot(Pivot::TOP_LEFT);
		scroll_bar_node.set_anchor(Pivot::TOP_LEFT);
		scroll_bar_node.set_interactable(true);
	}

	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		self.update_thumb(context);
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	// 출력. (트랙 → 썸)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let track_size = transform.get_content_size();
		let track_rect = Rect::create(0.0, 0.0, track_size.x, track_size.y);
		graphic.set_fill_color(self.track_color);
		graphic.draw_round_rect(track_rect, SCROLLBAR_ROUND_SIZE);
		if let Some(thumb_rect) = self.thumb_rect {
			graphic.set_fill_color(self.thumb_color);
			graphic.draw_round_rect(thumb_rect, SCROLLBAR_ROUND_SIZE);
		}
	}

	//==============================================================================
	// 터치 누름. (썸이면 그 자리에서 끌기 시작, 트랙이면 그 자리로 뛴 뒤 끌기)
	//==============================================================================
	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if self.scroll_view_node_id.is_none() {
			return;
		}
		if !self.is_inside_thumb(context, view_input_position) {
			self.apply_track_hit_position(context, view_input_position);
		}
		self.is_dragging_thumb = true;
		self.drag_start_view_position = view_input_position;
		let node_tree = context.get_node_tree();
		if let Some(scroll_state) = self.read_scroll_state(node_tree) {
			self.drag_start_scroll_offset = scroll_state.scroll_offset;
		}
	}

	//==============================================================================
	// 터치 이동. (끈 픽셀만큼 썸이 움직이고, 그 비율로 콘텐트가 반대로 움직인다)
	//==============================================================================
	fn touch_move(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if !self.is_dragging_thumb {
			return;
		}
		let scroll_view_node_id = match self.scroll_view_node_id {
			Some(scroll_view_node_id) => {
				scroll_view_node_id
			}
			None => {
				return;
			}
		};
		let node_tree = context.get_node_tree();
		let scroll_state = match self.read_scroll_state(node_tree) {
			Some(scroll_state) => {
				scroll_state
			}
			None => {
				return;
			}
		};
		let track_size = context.get_transform().get_content_size();
		let thumb_metrics = match self.calculate_thumb_metrics(&scroll_state, track_size) {
			Some(thumb_metrics) => {
				thumb_metrics
			}
			None => {
				return;
			}
		};
		if thumb_metrics.scrollable_track <= 0.0 {
			return;
		}
		let drag_delta = self.pick_axis(view_input_position.subtract(self.drag_start_view_position));
		let offset_delta = -drag_delta * (thumb_metrics.scrollable_content / thumb_metrics.scrollable_track);
		let start_offset = self.drag_start_scroll_offset;
		let new_offset = match self.axis {
			ScrollBarAxis::Vertical => {
				Vector2::create(start_offset.x, start_offset.y + offset_delta)
			}
			ScrollBarAxis::Horizontal => {
				Vector2::create(start_offset.x + offset_delta, start_offset.y)
			}
		};
		let node_tree = context.get_node_tree_mut();
		UIScrollView::set_scroll_offset_in_tree(node_tree, scroll_view_node_id, new_offset);
	}

	fn touch_release(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.is_dragging_thumb = false;
	}

	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.is_dragging_thumb = false;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;

	//==============================================================================
	// 썸 크기와 자리는 보이는 비율과 오프셋을 따르고, 썸을 끌면 콘텐트가 반대로 움직인다.
	//==============================================================================
	#[test]
	fn thumb_follows_offset_and_drag_scrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let scroll_id = node_tree.create_child(root_id, "scroll");
		let scroll_node = &mut node_tree[scroll_id];
		scroll_node.set_pivot(Pivot::TOP_LEFT);
		scroll_node.set_content_size(Vector2::create(200.0, 112.0));
		{
			let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_scroll_content_size(Vector2::create(200.0, 448.0));
			scroll_view.set_shows_vertical_scroll_bar(true);
		}
		node_tree.tick(&engine, 0.016);
		let scroll_bar_id = node_tree[scroll_id].get_component::<UIScrollView>().expect("scroll view").get_vertical_scroll_bar().expect("scroll bar");
		// 트랙 길이 100, 보이는 비율 1/4 → 썸 길이 25.
		let thumb_rect = node_tree[scroll_bar_id].get_component::<UIScrollBar>().expect("scroll bar").get_thumb_rect().expect("thumb");
		assert_eq!(thumb_rect, Rect::create(0.0, 0.0, 12.0, 25.0));

		// 썸을 끝까지 끌면 콘텐트가 끝까지 간다. (썸 75 픽셀 = 콘텐트 336 픽셀)
		let thumb_center = Vector2::create(182.0 + 6.0, 6.0 + 12.0);
		node_tree.dispatch_touch(scroll_bar_id, &engine, TouchKind::Press, thumb_center);
		node_tree.dispatch_touch(scroll_bar_id, &engine, TouchKind::Moved, Vector2::create(thumb_center.x, thumb_center.y + 75.0));
		let scroll_offset = node_tree[scroll_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_offset();
		assert!((scroll_offset.y + 336.0).abs() < 0.001);
		node_tree.dispatch_touch(scroll_bar_id, &engine, TouchKind::Release, thumb_center);
		node_tree.tick(&engine, 0.016);
		let moved_thumb_rect = node_tree[scroll_bar_id].get_component::<UIScrollBar>().expect("scroll bar").get_thumb_rect().expect("thumb");
		assert!((moved_thumb_rect.position.y - 75.0).abs() < 0.001);

		// 트랙의 맨 위를 누르면 썸이 맨 위로 뛴다.
		node_tree.dispatch_touch(scroll_bar_id, &engine, TouchKind::Press, Vector2::create(188.0, 7.0));
		let jumped_offset = node_tree[scroll_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_offset();
		assert_eq!(jumped_offset.y, 0.0);
	}
}
