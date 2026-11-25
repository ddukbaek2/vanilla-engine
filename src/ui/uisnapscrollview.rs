//==============================================================================
// 스냅 스크롤 뷰. (원본 ui/uisnapscrollview.js 의 UISnapScrollView)
// - 드래그를 놓으면 content 자식(아이템) 하나의 가운데가 보이는 영역 가운데에 오도록 미끄러진다. 아이템 크기가 달라도 된다.
// - 가로 스냅: set_horizontal(true), set_vertical(false) (기본). 세로 스냅: set_horizontal(false), set_vertical(true).
// - 한 번 끌면 최대 한 칸만 움직인다. 끈 거리가 다음 아이템 크기의 set_page_change_threshold 비율(기본 0.35) 이상이면 넘어간다.
//   끄는 동안에는 오프셋이 지금 칸 ±1 칸 범위를 넘지 않는다.
// - page_to(node_tree, index, smooth): 지정 칸으로 이동. smooth 면 0.3 초 cubic.out 으로 미끄러지고, 아니면 바로 옮긴다.
// - 원본은 UIScrollView 를 상속한다. 러스트 판은 UIScrollView 를 품고(scroll_view 필드) 터치, 휠, 그리기, 붙기를 넘겨준다.
//   스크롤 뷰 설정은 get_scroll_view_mut 로 품은 뷰에 한다. 자주 쓰는 것(콘텐트 크기, 오프셋, 축)은 이 위젯에도 있다.
//   node_tree.add_component(node_id, UISnapScrollView::new()) 로 붙인다. (content 노드, Mask, interactable 은 품은 뷰가 만든다)
// - 원본과 다른 것:
//   - 칸을 바꾸는 함수(page_to, set_snap_current_index, play_snap_tween 등)는 content 자식을 읽어야 해서 트리를 받는다.
//     씬 코드에서는 node_tree.with_component_mut::<UISnapScrollView, _>(snap_id, |snap_view, tree| snap_view.page_to(tree, 2, true)) 로 부른다.
//   - 스냅 미끄러짐은 원본 Tween 객체 대신 같은 계산(0.3 초, cubic.out, 만든 tick 에 첫 걸음)을 이 위젯 안에서 한다.
//     (원본 트윈의 갱신 콜백이 스크롤 뷰 자신을 부르는데, 러스트 트윈 콜백은 트리만 받아 tick 중에 꺼내진 이 위젯에 닿지 않는다)
//   - 미끄러지는 동안 content 노드는 그 tick 안에서 바로 옮긴다. (품은 뷰의 반영은 다음 tick 이라 한 프레임 늦지 않게)
//   - set_snap_to_nearest, set_velocity_threshold 는 원본처럼 값만 든다. (원본 play_snap_tween 도 이 값을 읽지 않는다)
//   - 원본은 스냅 뷰가 UIScrollView 이기도 해서 TouchRecognizer, UIScrollBar, UIListView 가 그대로 알아본다.
//     러스트 판은 TouchRecognizer 와 UIListView 가 find_scroll_view 로 알아본다. UIScrollBar 는 UIScrollView 컴포넌트만 찾아서
//     스냅 뷰에는 스크롤바를 붙일 수 없다. (setShows...ScrollBar 는 품은 뷰에 값만 들어간다)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::{NodeTransform, WorldNode};
use crate::core::tween::easing_function;
use crate::ui::uiscrollview::{ScrollMode, UIScrollView};

// 스냅 미끄러짐 시간. (초, 원본 트윈 0.3)
const SNAP_TWEEN_DURATION: f32 = 0.3;

//==============================================================================
// 스냅 미끄러짐 하나. (원본 #snap_tween 자리)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct SnapTween {
	start_offset: Vector2,
	target_offset: Vector2,
	elapsed_seconds: f32,
}

//==============================================================================
// 스냅 스크롤 뷰.
//==============================================================================
pub struct UISnapScrollView {
	scroll_view: UIScrollView,
	node_id: Option<NodeId>,
	snap_tween: Option<SnapTween>,
	snap_current_index: usize,
	previous_is_dragging: bool,
	is_snap_to_nearest: bool,
	velocity_threshold: f32,
	page_change_threshold: f32,
}

#[crate::reflect(base = scroll_view)]
impl UISnapScrollView {
	//==============================================================================
	// 생성. (clamp 모드, 가로만 스크롤)
	//==============================================================================
	pub fn new() -> UISnapScrollView {
		let mut scroll_view = UIScrollView::new();
		scroll_view.set_scroll_mode(ScrollMode::Clamp);
		scroll_view.set_horizontal(true);
		scroll_view.set_vertical(false);
		UISnapScrollView {
			scroll_view,
			node_id: None,
			snap_tween: None,
			snap_current_index: 0,
			previous_is_dragging: false,
			is_snap_to_nearest: false,
			velocity_threshold: 200.0,
			page_change_threshold: 0.35,
		}
	}

	//==============================================================================
	// 품은 스크롤 뷰 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_scroll_view(&self) -> &UIScrollView {
		&self.scroll_view
	}

	//==============================================================================
	// 품은 스크롤 뷰 반환. (변경용)
	//==============================================================================
	pub fn get_scroll_view_mut(&mut self) -> &mut UIScrollView {
		&mut self.scroll_view
	}

	//==============================================================================
	// 컨텐트 노드 핸들. (아이템 노드를 여기에 붙인다)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.scroll_view.get_content()
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.scroll_view.set_background_color(background_color);
	}

	//==============================================================================
	// 스크롤 콘텐츠 크기 설정.
	//==============================================================================
	#[reflect(skip)]
	pub fn set_scroll_content_size(&mut self, scroll_content_size: Vector2) {
		self.scroll_view.set_scroll_content_size(scroll_content_size);
	}

	//==============================================================================
	// 스크롤 콘텐츠 크기 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_scroll_content_size(&self) -> Vector2 {
		self.scroll_view.get_scroll_content_size()
	}

	//==============================================================================
	// 오프셋 설정. (범위로 자른다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_scroll_offset(&mut self, offset: Vector2) {
		self.scroll_view.set_scroll_offset(offset);
	}

	//==============================================================================
	// 오프셋 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_scroll_offset(&self) -> Vector2 {
		self.scroll_view.get_scroll_offset()
	}

	//==============================================================================
	// 가로 스크롤 켜기 설정.
	//==============================================================================
	#[reflect(skip)]
	pub fn set_horizontal(&mut self, is_horizontal: bool) {
		self.scroll_view.set_horizontal(is_horizontal);
	}

	//==============================================================================
	// 가로 스크롤 켜짐 여부.
	//==============================================================================
	#[reflect(skip)]
	pub fn is_horizontal(&self) -> bool {
		self.scroll_view.is_horizontal()
	}

	//==============================================================================
	// 세로 스크롤 켜기 설정.
	//==============================================================================
	#[reflect(skip)]
	pub fn set_vertical(&mut self, is_vertical: bool) {
		self.scroll_view.set_vertical(is_vertical);
	}

	//==============================================================================
	// 세로 스크롤 켜짐 여부.
	//==============================================================================
	#[reflect(skip)]
	pub fn is_vertical(&self) -> bool {
		self.scroll_view.is_vertical()
	}

	//==============================================================================
	// 드래그 중인지.
	//==============================================================================
	#[reflect(skip)]
	pub fn is_dragging(&self) -> bool {
		self.scroll_view.is_dragging()
	}

	//==============================================================================
	// 스냅 모드 설정. (원본처럼 값만 든다)
	//==============================================================================
	pub fn set_snap_to_nearest(&mut self, is_snap_to_nearest: bool) {
		self.is_snap_to_nearest = is_snap_to_nearest;
	}

	//==============================================================================
	// 스냅 모드 반환.
	//==============================================================================
	pub fn get_snap_to_nearest(&self) -> bool {
		self.is_snap_to_nearest
	}

	//==============================================================================
	// 방향성 스냅 최소 속도 설정. (px/s, 원본처럼 값만 든다)
	//==============================================================================
	pub fn set_velocity_threshold(&mut self, velocity_threshold: f32) {
		self.velocity_threshold = velocity_threshold;
	}

	//==============================================================================
	// 방향성 스냅 최소 속도 반환.
	//==============================================================================
	pub fn get_velocity_threshold(&self) -> f32 {
		self.velocity_threshold
	}

	//==============================================================================
	// 칸을 넘길 끈 거리 비율 설정. (다음 아이템 크기 대비, 기본 0.35)
	//==============================================================================
	pub fn set_page_change_threshold(&mut self, page_change_threshold: f32) {
		self.page_change_threshold = page_change_threshold;
	}

	//==============================================================================
	// 칸을 넘길 끈 거리 비율 반환.
	//==============================================================================
	pub fn get_page_change_threshold(&self) -> f32 {
		self.page_change_threshold
	}

	//==============================================================================
	// 지금 스냅 칸 반환.
	//==============================================================================
	pub fn get_snap_current_index(&self) -> usize {
		self.snap_current_index
	}

	//==============================================================================
	// 미끄러지는 중인지.
	//==============================================================================
	pub fn is_snapping(&self) -> bool {
		self.snap_tween.is_some()
	}

	//==============================================================================
	// content 자식 수. (트리에 붙기 전이면 0)
	//==============================================================================
	fn get_item_count(&self, node_tree: &NodeTree) -> usize {
		match self.get_content() {
			Some(content_id) => {
				node_tree.get_child_count(content_id)
			}
			None => {
				0
			}
		}
	}

	//==============================================================================
	// 진행 축의 값 고르기.
	//==============================================================================
	fn select_axis(&self, vector: Vector2) -> f32 {
		if self.is_horizontal() {
			vector.x
		}
		else {
			vector.y
		}
	}

	//==============================================================================
	// 진행 축만 바꾼 오프셋. (다른 축은 지금 오프셋 그대로)
	//==============================================================================
	fn build_axis_offset(&self, axis_offset: f32) -> Vector2 {
		let current_offset = self.get_scroll_offset();
		if self.is_horizontal() {
			Vector2::create(axis_offset, current_offset.y)
		}
		else {
			Vector2::create(current_offset.x, axis_offset)
		}
	}

	//==============================================================================
	// content 노드를 지금 오프셋 자리로 바로 옮긴다.
	//==============================================================================
	fn sync_content_position(&self, node_tree: &mut NodeTree) {
		let scroll_offset = self.get_scroll_offset();
		if let Some(content) = self.get_content().and_then(|content_id| node_tree.get_node_mut(content_id)) {
			content.set_local_position(scroll_offset);
		}
	}

	//==============================================================================
	// content 자식마다 스냅 오프셋 계산. (각 아이템 가운데가 보이는 영역 가운데에 오는 오프셋, 크기가 달라도 맞다)
	//==============================================================================
	pub fn compute_snap_offsets(&self, node_tree: &NodeTree) -> Vec<f32> {
		let mut snap_offsets: Vec<f32> = Vec::new();
		let viewport_size = match self.node_id.and_then(|node_id| node_tree.get_node(node_id)) {
			Some(node) => {
				node.get_content_size()
			}
			None => {
				return snap_offsets;
			}
		};
		let content_id = match self.get_content() {
			Some(content_id) => {
				content_id
			}
			None => {
				return snap_offsets;
			}
		};
		let child_ids = node_tree.get_child_ids(content_id);
		snap_offsets.reserve(child_ids.len());
		for child_id in child_ids.iter().copied() {
			let child = &node_tree[child_id];
			let child_local_position = child.get_local_position();
			let child_content_size = child.get_content_size();
			let snap_offset = self.select_axis(viewport_size) * 0.5 - self.select_axis(child_local_position) - self.select_axis(child_content_size) * 0.5;
			snap_offsets.push(snap_offset);
		}
		snap_offsets
	}

	//==============================================================================
	// 끄는 동안 오프셋을 지금 칸의 앞뒤 칸 범위로 자른다.
	//==============================================================================
	pub fn clamp_scroll_offset_to_adjacent_pages(&mut self, node_tree: &mut NodeTree) {
		let snap_offsets = self.compute_snap_offsets(node_tree);
		if snap_offsets.is_empty() {
			return;
		}
		let maximum_index = snap_offsets.len() - 1;
		let snap_current_index = self.snap_current_index.min(maximum_index);
		let previous_index = snap_current_index.saturating_sub(1);
		let next_index = (snap_current_index + 1).min(maximum_index);
		let maximum_offset = snap_offsets[previous_index];
		let minimum_offset = snap_offsets[next_index];
		let current_axis_offset = self.select_axis(self.get_scroll_offset());
		let clamped_axis_offset = math::clamp(current_axis_offset, minimum_offset, maximum_offset);
		if clamped_axis_offset != current_axis_offset {
			let clamped_offset = self.build_axis_offset(clamped_axis_offset);
			self.set_scroll_offset(clamped_offset);
		}
	}

	//==============================================================================
	// 지금 오프셋으로 목표 칸을 정하고 미끄러짐을 시작한다. (끈 방향으로 최대 한 칸, 다음 칸이 비율 이상 드러나면 넘김)
	//==============================================================================
	pub fn play_snap_tween(&mut self, node_tree: &mut NodeTree) {
		let snap_offsets = self.compute_snap_offsets(node_tree);
		if snap_offsets.is_empty() {
			return;
		}
		let content_id = match self.get_content() {
			Some(content_id) => {
				content_id
			}
			None => {
				return;
			}
		};
		let maximum_index = snap_offsets.len() - 1;
		let current_axis_offset = self.select_axis(self.get_scroll_offset());
		let snap_current_index = self.snap_current_index.min(maximum_index);
		let current_snap_offset = snap_offsets[snap_current_index];
		let drag_amount = current_axis_offset - current_snap_offset;
		let direction: i64 = if drag_amount < 0.0 {
			1
		}
		else {
			-1
		};
		let candidate_index = (snap_current_index as i64 + direction).clamp(0, maximum_index as i64) as usize;
		let target_index = if candidate_index != snap_current_index {
			let candidate_child_id = node_tree.get_child_ids(content_id)[candidate_index];
			let candidate_child_content_size = node_tree[candidate_child_id].get_content_size();
			let target_item_size = self.select_axis(candidate_child_content_size);
			if drag_amount.abs() >= target_item_size * self.get_page_change_threshold() {
				candidate_index
			}
			else {
				snap_current_index
			}
		}
		else {
			snap_current_index
		};
		self.set_snap_current_index(node_tree, target_index);
	}

	//==============================================================================
	// 지정 칸으로 이동. (smooth 면 미끄러지고, 아니면 바로 옮긴다)
	//==============================================================================
	pub fn page_to(&mut self, node_tree: &mut NodeTree, index: usize, smooth: bool) {
		let item_count = self.get_item_count(node_tree);
		if item_count == 0 {
			return;
		}
		let clamped_index = index.min(item_count - 1);
		if smooth {
			self.set_snap_current_index(node_tree, clamped_index);
			return;
		}
		self.snap_current_index = clamped_index;
		self.snap_tween = None;
		let snap_offsets = self.compute_snap_offsets(node_tree);
		let target_snap_offset = snap_offsets[self.snap_current_index];
		let target_offset = self.build_axis_offset(target_snap_offset);
		self.set_scroll_offset(target_offset);
		self.sync_content_position(node_tree);
	}

	//==============================================================================
	// 지정 칸으로 미끄러짐 시작. (지금 오프셋에서 그 칸의 스냅 오프셋까지)
	//==============================================================================
	pub fn set_snap_current_index(&mut self, node_tree: &mut NodeTree, index: usize) {
		let item_count = self.get_item_count(node_tree);
		if item_count == 0 {
			return;
		}
		self.snap_current_index = index.min(item_count - 1);
		let snap_offsets = self.compute_snap_offsets(node_tree);
		let target_snap_offset = snap_offsets[self.snap_current_index];
		let start_offset = self.get_scroll_offset();
		let target_offset = self.build_axis_offset(target_snap_offset);
		self.snap_tween = Some(SnapTween {
			start_offset,
			target_offset,
			elapsed_seconds: 0.0,
		});
	}

	//==============================================================================
	// 미끄러짐 한 걸음. (원본 Tween.tick 과 같은 계산: 진행률 = 지난 시간 / 0.3, cubic.out, 다 차면 끝)
	//==============================================================================
	fn tick_snap_tween(&mut self, time_delta: f32) {
		let mut snap_tween = match self.snap_tween {
			Some(snap_tween) => {
				snap_tween
			}
			None => {
				return;
			}
		};
		snap_tween.elapsed_seconds += time_delta;
		let progress = math::clamp(snap_tween.elapsed_seconds / SNAP_TWEEN_DURATION, 0.0, 1.0);
		let eased_progress = easing_function::cubic::out(progress);
		let offset_x = math::lerp(snap_tween.start_offset.x, snap_tween.target_offset.x, eased_progress);
		let offset_y = math::lerp(snap_tween.start_offset.y, snap_tween.target_offset.y, eased_progress);
		self.set_scroll_offset(Vector2::create(offset_x, offset_y));
		if progress >= 1.0 {
			self.snap_tween = None;
		}
		else {
			self.snap_tween = Some(snap_tween);
		}
	}
}

impl Default for UISnapScrollView {
	fn default() -> UISnapScrollView {
		UISnapScrollView::new()
	}
}

#[crate::reflect]
impl Component for UISnapScrollView {
	fn get_component_base(&self) -> &ComponentBase {
		self.scroll_view.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.scroll_view.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"SnapScrollView"
	}

	fn is_wheel_receiver(&self) -> bool {
		true
	}

	//==============================================================================
	// 노드에 붙음. (품은 뷰가 보이는 영역 크기를 읽는다)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		self.scroll_view.attach(transform);
	}

	//==============================================================================
	// 트리에 붙음. (품은 뷰가 content, Mask, interactable 을 만든다. 호스트 핸들을 기억한다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.scroll_view.attach_to_tree(node_tree, node_id);
		self.node_id = Some(node_id);
	}

	//==============================================================================
	// 갱신. (품은 뷰 → 드래그 시작이면 미끄러짐 취소 → 끄는 중 칸 범위 자르기 → 놓았으면 스냅 시작 → 미끄러짐)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.node_id = Some(context.get_node_id());
		let previous_is_dragging = self.previous_is_dragging;
		self.scroll_view.tick(context, time_delta);
		let current_is_dragging = self.is_dragging();
		self.previous_is_dragging = current_is_dragging;

		// 드래그 시작이면 미끄러짐 취소.
		if !previous_is_dragging && current_is_dragging {
			self.snap_tween = None;
		}

		// 끄는 중에는 지금 칸 앞뒤 범위로 자른다.
		let mut is_offset_changed = false;
		if current_is_dragging {
			self.clamp_scroll_offset_to_adjacent_pages(context.get_node_tree_mut());
			is_offset_changed = true;
		}

		// 놓았으면 목표 칸을 정하고 미끄러짐 시작.
		if previous_is_dragging && !current_is_dragging {
			self.play_snap_tween(context.get_node_tree_mut());
		}

		// 미끄러짐 한 걸음. (만든 tick 에도 바로 한 걸음 간다. 원본과 같음)
		if self.snap_tween.is_some() {
			self.tick_snap_tween(time_delta);
			is_offset_changed = true;
		}
		if is_offset_changed {
			self.sync_content_position(context.get_node_tree_mut());
		}
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.scroll_view.draw(transform, graphic);
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.scroll_view.touch_press(context, view_input_position);
	}

	fn touch_move(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.scroll_view.touch_move(context, view_input_position);
	}

	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.scroll_view.touch_release(context, view_input_position);
	}

	fn touch_cancel(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.scroll_view.touch_cancel(context, view_input_position);
	}

	//==============================================================================
	// 휠. (원본처럼 품은 뷰 그대로: 자유롭게 움직이고 스냅하지 않는다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
		self.scroll_view.wheel(context, wheel_delta)
	}
}

//==============================================================================
// 노드의 스크롤 뷰 찾기. (UIScrollView, 없으면 UISnapScrollView 가 품은 뷰. 원본 getComponents(UIScrollView) 가 하위 클래스까지 잡는 자리)
//==============================================================================
pub fn find_scroll_view(node: &WorldNode) -> Option<&UIScrollView> {
	if let Some(scroll_view) = node.get_component::<UIScrollView>() {
		return Some(scroll_view);
	}
	node.get_component::<UISnapScrollView>().map(|snap_scroll_view| snap_scroll_view.get_scroll_view())
}

//==============================================================================
// 노드의 스크롤 뷰 찾기. (변경용)
//==============================================================================
pub fn find_scroll_view_mut(node: &mut WorldNode) -> Option<&mut UIScrollView> {
	if node.has_component::<UIScrollView>() {
		return node.get_component_mut::<UIScrollView>();
	}
	node.get_component_mut::<UISnapScrollView>().map(|snap_scroll_view| snap_scroll_view.get_scroll_view_mut())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use crate::ui::touchrecognizer::TouchRecognizer;
	use crate::ui::uibutton::UIButton;

	//==============================================================================
	// 시험 장면. (왼쪽 위 (0, 0), 200 x 100 인 가로 스냅 뷰, 200 x 100 칸 다섯 개)
	//==============================================================================
	fn create_snap_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let snap_id = node_tree.create_child(root_id, "snap");
		let snap_node = &mut node_tree[snap_id];
		snap_node.set_pivot(Pivot::TOP_LEFT);
		snap_node.set_content_size(Vector2::create(200.0, 100.0));
		let content_id = {
			let snap_view = node_tree.add_component(snap_id, UISnapScrollView::new()).expect("snap view");
			snap_view.set_scroll_content_size(Vector2::create(1000.0, 100.0));
			snap_view.get_content().expect("content")
		};
		for page_index in 0..5 {
			let page_id = node_tree.create_child(content_id, "page");
			let page_node = &mut node_tree[page_id];
			page_node.set_pivot(Pivot::TOP_LEFT);
			page_node.set_local_position(Vector2::create(page_index as f32 * 200.0, 0.0));
			page_node.set_content_size(Vector2::create(200.0, 100.0));
		}
		(node_tree, snap_id)
	}

	//==============================================================================
	// 스냅 뷰 반환.
	//==============================================================================
	fn get_snap_view(node_tree: &mut NodeTree, snap_id: NodeId) -> &mut UISnapScrollView {
		node_tree[snap_id].get_component_mut::<UISnapScrollView>().expect("snap view")
	}

	//==============================================================================
	// 끌고 놓기. (누름 → 이동 → tick → 뗌 → 0.5 초 동안 tick)
	//==============================================================================
	fn drag_and_release(node_tree: &mut NodeTree, engine: &Engine, snap_id: NodeId, from_x: f32, to_x: f32) {
		node_tree.dispatch_touch(snap_id, engine, TouchKind::Press, Vector2::create(from_x, 50.0));
		node_tree.dispatch_touch(snap_id, engine, TouchKind::Moved, Vector2::create(to_x, 50.0));
		node_tree.tick(engine, 0.016);
		node_tree.dispatch_touch(snap_id, engine, TouchKind::Release, Vector2::create(to_x, 50.0));
		for _ in 0..30 {
			node_tree.tick(engine, 0.016);
		}
	}

	//==============================================================================
	// 스냅 오프셋은 칸 가운데가 영역 가운데에 오는 값이다.
	//==============================================================================
	#[test]
	fn snap_offsets_center_each_item() {
		let (node_tree, snap_id) = create_snap_tree();
		let snap_offsets = node_tree[snap_id].get_component::<UISnapScrollView>().expect("snap view").compute_snap_offsets(&node_tree);
		assert_eq!(snap_offsets, vec![0.0, -200.0, -400.0, -600.0, -800.0]);
	}

	//==============================================================================
	// 칸 크기의 35% 이상 끌면 한 칸 넘어가고, 덜 끌면 제자리로 돌아온다. 아무리 멀리 끌어도 한 칸만 간다.
	//==============================================================================
	#[test]
	fn drag_selects_at_most_one_neighbor() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, snap_id) = create_snap_tree();
		let content_id = get_snap_view(&mut node_tree, snap_id).get_content().expect("content");

		// 100 픽셀 왼쪽으로 → 다음 칸.
		drag_and_release(&mut node_tree, &engine, snap_id, 150.0, 50.0);
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_snap_current_index(), 1);
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_scroll_offset(), Vector2::create(-200.0, 0.0));
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(-200.0, 0.0));
		assert!(!get_snap_view(&mut node_tree, snap_id).is_snapping());

		// 40 픽셀만 → 제자리.
		drag_and_release(&mut node_tree, &engine, snap_id, 150.0, 110.0);
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_snap_current_index(), 1);
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_scroll_offset(), Vector2::create(-200.0, 0.0));

		// 490 픽셀 → 끄는 동안 다음 칸(-400)에서 막히고, 놓으면 한 칸만 간다.
		node_tree.dispatch_touch(snap_id, &engine, TouchKind::Press, Vector2::create(190.0, 50.0));
		node_tree.dispatch_touch(snap_id, &engine, TouchKind::Moved, Vector2::create(-300.0, 50.0));
		node_tree.tick(&engine, 0.016);
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_scroll_offset(), Vector2::create(-400.0, 0.0));
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(-400.0, 0.0));
		node_tree.dispatch_touch(snap_id, &engine, TouchKind::Release, Vector2::create(-300.0, 50.0));
		for _ in 0..30 {
			node_tree.tick(&engine, 0.016);
		}
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_snap_current_index(), 2);

		// 오른쪽으로 끌면 앞 칸.
		drag_and_release(&mut node_tree, &engine, snap_id, 50.0, 150.0);
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_snap_current_index(), 1);
	}

	//==============================================================================
	// 미끄러짐은 0.3 초 cubic.out 이다. (놓은 tick 에 첫 걸음)
	//==============================================================================
	#[test]
	fn snap_tween_follows_cubic_out() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, snap_id) = create_snap_tree();
		node_tree.dispatch_touch(snap_id, &engine, TouchKind::Press, Vector2::create(150.0, 50.0));
		node_tree.dispatch_touch(snap_id, &engine, TouchKind::Moved, Vector2::create(50.0, 50.0));
		node_tree.tick(&engine, 0.016);
		node_tree.dispatch_touch(snap_id, &engine, TouchKind::Release, Vector2::create(50.0, 50.0));
		node_tree.tick(&engine, 0.15);
		let expected_x = math::lerp(-100.0, -200.0, easing_function::cubic::out(0.5));
		let offset = get_snap_view(&mut node_tree, snap_id).get_scroll_offset();
		assert!((offset.x - expected_x).abs() < 0.001);
		assert!(get_snap_view(&mut node_tree, snap_id).is_snapping());
		node_tree.tick(&engine, 0.15);
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_scroll_offset(), Vector2::create(-200.0, 0.0));
		assert!(!get_snap_view(&mut node_tree, snap_id).is_snapping());
	}

	//==============================================================================
	// 칸 위의 버튼을 끌면 TouchRecognizer 가 스냅 뷰로 넘기고, 놓으면 다음 칸으로 간다.
	//==============================================================================
	#[test]
	fn touch_recognizer_hands_drag_to_snap_view() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, snap_id) = create_snap_tree();
		let content_id = get_snap_view(&mut node_tree, snap_id).get_content().expect("content");
		let page_id = node_tree.get_child_ids(content_id)[0];
		node_tree.add_component(page_id, UIButton::new());
		let press_position = Vector2::create(150.0, 50.0);
		assert_eq!(node_tree.find_topmost_interactable_node(press_position), Some(page_id));
		let mut touch_recognizer = TouchRecognizer::new();
		touch_recognizer.touch_press(&mut node_tree, &engine, press_position);
		touch_recognizer.touch_move(&mut node_tree, &engine, Vector2::create(50.0, 50.0));
		assert!(get_snap_view(&mut node_tree, snap_id).is_dragging());
		node_tree.tick(&engine, 0.016);
		touch_recognizer.touch_release(&mut node_tree, &engine, Vector2::create(50.0, 50.0));
		for _ in 0..30 {
			node_tree.tick(&engine, 0.016);
		}
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_snap_current_index(), 1);
		// 미룬 누름은 드래그가 되며 버려져 버튼은 눌리지 않았다.
		assert_ne!(node_tree[page_id].get_component::<UIButton>().expect("button").get_button_state(), crate::ui::uibutton::ButtonState::Pressed);
	}

	//==============================================================================
	// page_to: 바로 옮기기는 content 까지 그 자리에서, 미끄러지기는 tick 으로.
	//==============================================================================
	#[test]
	fn page_to_moves_immediately_or_smoothly() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, snap_id) = create_snap_tree();
		let content_id = get_snap_view(&mut node_tree, snap_id).get_content().expect("content");
		node_tree.with_component_mut::<UISnapScrollView, _>(snap_id, |snap_view, tree| {
			snap_view.page_to(tree, 9, false);
		});
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_snap_current_index(), 4);
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(-800.0, 0.0));
		node_tree.with_component_mut::<UISnapScrollView, _>(snap_id, |snap_view, tree| {
			snap_view.page_to(tree, 2, true);
		});
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_scroll_offset(), Vector2::create(-800.0, 0.0));
		for _ in 0..20 {
			node_tree.tick(&engine, 0.016);
		}
		assert_eq!(get_snap_view(&mut node_tree, snap_id).get_scroll_offset(), Vector2::create(-400.0, 0.0));
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(-400.0, 0.0));
	}
}
