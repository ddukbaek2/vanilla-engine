//==============================================================================
// 창 쌓기의 창 하나. (vanilla.js 의 ui/uiwindow.js, UIWindow)
// - 영역 전체를 어둡게 덮고(뒤로 누름이 가지 않는다) 가운데에 판을 띄운다. 어둠은 세이프에어리어(set_view_bleed)까지 덮는다.
// - 판은 영역의 가로에 맞춰 넓어지되 maximum_width 를 넘지 않는다. 세로는 layout_content 가 돌려준 값이고, 영역보다 크면 판을 통째로 줄인다.
// - 열고 닫는 움직임은 PopupMotion 이다. 닫는 움직임이 끝나면 창 층(UIWindowLayer)이 노드를 지운다. (그동안은 누름을 받지 않는다)
// - 바깥(어둠)을 누르면 닫힌다. 창마다 set_outside_close_enabled 로 끈다. 창 층의 close_top_window(뒤로 가기)도 이 값을 따른다.
// - 노드 구성: 창(영역 크기) ├ 어둠(Paint + UIButton) └ 틀(판 가운데 피봇, 움직임 배율) └ 판(Paint 한 장, 내용은 여기)
// - 원본과 다른 것:
//   - 원본은 WorldNode 를 상속한 노드이고 파생 클래스가 layoutContent, layoutPanel, applyMotion, setCovered, handleWheel, dispose 를 덮어쓴다.
//     러스트 판은 창 노드에 붙는 컴포넌트이고, 덮어쓸 자리는 UIWindowBehavior 트레이트다. (기본 구현은 UIWindowDefaultBehavior,
//     덮어쓴 쪽이 원본 super 처럼 기본을 부르려면 UIWindow::layout_panel_default, apply_motion_default, set_covered_default)
//     판 모양(그림자, 테, 글로시)은 게임이 build 에서 틀에 꾸밈 노드를 붙이고(판 뒤는 set_sibling_index(0)) layout_panel 에서 놓는다.
//   - 창 노드는 UIWindow::create(node_tree, layer_id, name, behavior) 로 창 층 노드 밑에 만든다. 닫는 움직임이 끝나면 층에서 떼어 내고 지우지 않으므로
//     원본처럼 같은 창 노드를 다시 띄울 수 있다. (UIWindowLayer::open_window)
//   - 닫기는 창 층을 거친다. 씬과 콜백 코드는 UIWindow::close_in_tree(node_tree, window_id) 를 부른다.
//   - 알림(set_close_event)은 인자 없는 Box<dyn FnMut()> 다. (PopupMotion 알림과 같은 꼴, 노드를 바꿀 일은 씬이 tick 에서 상태를 보고 한다)
//   - 휠은 원본처럼 GameSceneBase 가 창 층 안을 가리킨 휠을 UIWindowLayer::scroll_window_at 으로 넘겨 맨 위 창(UIWindowBehavior::handle_wheel)이 먼저 받고,
//     받지 않으면 그 자리의 스크롤 뷰가 받는다. GameSceneBase 를 거치지 않는 엔진의 휠 전달(가리킨 노드부터 부모 쪽)로 창에 온 휠도 창이 받고 늘 삼킨다.
//   - 판과 어둠의 Paint 는 노드의 컴포넌트라 get_panel_paint, get_dim_paint 가 트리를 받는다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::popupmotion::PopupMotion;
use crate::ui::uibutton::UIButton;
use crate::ui::uiwindowlayer::UIWindowLayer;
use std::any::Any;

// 기본 모양. (창의 가로 한계, 영역 가장자리와 띄우는 거리, 판의 처음 세로, 둥근 정도)
const DEFAULT_MAXIMUM_WIDTH: f32 = 480.0;
const DEFAULT_MARGIN: f32 = 16.0;
const DEFAULT_PANEL_HEIGHT: f32 = 240.0;
const DEFAULT_PANEL_ROUND_SIZE: f32 = 16.0;

//==============================================================================
// 알림. (인자 없음)
//==============================================================================
pub type UIWindowEvent = Box<dyn FnMut()>;

//==============================================================================
// 창의 노드와 지금 놓인 값. (UIWindowBehavior 가 받는다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIWindowParts {
	pub window_id: NodeId,
	pub dim_id: NodeId,
	pub frame_id: NodeId,
	pub panel_id: NodeId,
	// 마지막으로 받은 영역의 크기. (UI 층 단위)
	pub view_width: f32,
	pub view_height: f32,
	pub margin: f32,
	// layout_content 기본값이 돌려주는 판의 세로.
	pub panel_height: f32,
	// 마지막으로 놓은 판의 자리. (창 좌표, 움직임 전, 줄인 배율을 곱한 크기)
	pub panel_rect: Rect,
	// 판이 영역보다 커서 줄인 배율. (1 이하)
	pub panel_scale: f32,
}

//==============================================================================
// 창의 덮어쓸 자리. (원본 UIWindow 파생 클래스가 덮어쓰는 메서드)
//==============================================================================
pub trait UIWindowBehavior: Any {
	//==============================================================================
	// 구체 타입으로 꺼내기. (UIWindow::get_behavior_mut 이 쓴다)
	//==============================================================================
	fn as_any(&self) -> &dyn Any;
	fn as_any_mut(&mut self) -> &mut dyn Any;

	//==============================================================================
	// 창 노드를 다 만든 뒤 한 번. (내용과 꾸밈 노드를 만든다, 원본 파생 클래스 생성자 자리)
	//==============================================================================
	fn build(&mut self, _node_tree: &mut NodeTree, _parts: &UIWindowParts) {
	}

	//==============================================================================
	// 내용 놓기. (판 안에 내용을 놓고 판의 세로를 돌려준다, 기본은 parts.panel_height)
	//==============================================================================
	fn layout_content(&mut self, _node_tree: &mut NodeTree, parts: &UIWindowParts, _panel_width: f32) -> f32 {
		parts.panel_height
	}

	//==============================================================================
	// 판 놓기. (판의 크기를 정한다, 꾸밈이 있으면 덮어써 layout_panel_default 뒤에 놓는다)
	//==============================================================================
	fn layout_panel(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, panel_width: f32, panel_height: f32) {
		UIWindow::layout_panel_default(node_tree, parts, panel_width, panel_height);
	}

	//==============================================================================
	// 움직임을 틀과 어둠에 걸기. (기본: 틀 배율 = 줄인 배율 x 움직임 배율, 어둠 진하기 = 움직임 비율)
	//==============================================================================
	fn apply_motion(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, popup_motion: &PopupMotion) {
		UIWindow::apply_motion_default(node_tree, parts, popup_motion);
	}

	//==============================================================================
	// 다른 창에 덮였는지 알리기. (기본은 덮이면 창 노드를 끄고 드러나면 켠다)
	//==============================================================================
	fn set_covered(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, is_covered: bool) {
		UIWindow::set_covered_default(node_tree, parts, is_covered);
	}

	//==============================================================================
	// 그 자리의 스크롤 뷰가 받지 않은 휠. (받았으면 true, 창은 어느 쪽이든 휠을 삼킨다)
	//==============================================================================
	fn handle_wheel(&mut self, _node_tree: &mut NodeTree, _parts: &UIWindowParts, _view_input_position: Vector2, _wheel_delta: Vector2) -> bool {
		false
	}

	//==============================================================================
	// 창 층에서 내려감. (닫기, 바깥 누르기, close_all_windows 모두, set_close_event 알림 다음)
	//==============================================================================
	fn handle_closed(&mut self) {
	}

	//==============================================================================
	// 레이아웃을 다시 해야 하는지. (그린 뒤에야 알 수 있는 크기를 기다릴 때 true, 창이 tick 에서 다시 놓는다)
	//==============================================================================
	fn is_layout_pending(&self, _node_tree: &NodeTree, _parts: &UIWindowParts) -> bool {
		false
	}

	//==============================================================================
	// 지우기 직전. (엔진 밖에 붙인 것을 정리한다)
	//==============================================================================
	fn dispose(&mut self, _node_tree: &mut NodeTree, _parts: &UIWindowParts) {
	}
}

//==============================================================================
// 기본 창. (덮어쓴 것 없음, 판 세로는 UIWindow::set_panel_height)
//==============================================================================
pub struct UIWindowDefaultBehavior;

impl UIWindowBehavior for UIWindowDefaultBehavior {
	fn as_any(&self) -> &dyn Any {
		self
	}

	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}
}

//==============================================================================
// 창.
//==============================================================================
pub struct UIWindow {
	component_base: ComponentBase,
	node_id: Option<NodeId>,
	dim_id: Option<NodeId>,
	frame_id: Option<NodeId>,
	panel_id: Option<NodeId>,
	behavior: Option<Box<dyn UIWindowBehavior>>,
	popup_motion: PopupMotion,
	is_outside_close_enabled: bool,
	close_event: Option<UIWindowEvent>,
	// 띄운 창 층의 노드. (띄우기 전이나 내려간 뒤에는 None)
	window_layer_id: Option<NodeId>,
	view_width: f32,
	view_height: f32,
	// 영역 밖으로 더 덮는 너비. (위, 오른쪽, 아래, 왼쪽)
	view_bleed: [f32; 4],
	maximum_width: f32,
	margin: f32,
	panel_height: f32,
	panel_rect: Rect,
	panel_scale: f32,
	dim_color: Color,
	panel_color: Color,
}

impl UIWindow {
	//==============================================================================
	// 생성. (노드에는 node_tree.add_component 로 붙인다, 보통은 UIWindow::create)
	//==============================================================================
	pub fn create_with_behavior(behavior: Box<dyn UIWindowBehavior>) -> UIWindow {
		UIWindow {
			component_base: ComponentBase::new(),
			node_id: None,
			dim_id: None,
			frame_id: None,
			panel_id: None,
			behavior: Some(behavior),
			popup_motion: PopupMotion::new(),
			is_outside_close_enabled: true,
			close_event: None,
			window_layer_id: None,
			view_width: 1.0,
			view_height: 1.0,
			view_bleed: [0.0; 4],
			maximum_width: DEFAULT_MAXIMUM_WIDTH,
			margin: DEFAULT_MARGIN,
			panel_height: DEFAULT_PANEL_HEIGHT,
			panel_rect: Rect::create(0.0, 0.0, 1.0, 1.0),
			panel_scale: 1.0,
			dim_color: Color::create(0.0, 0.0, 0.0, 0.55),
			panel_color: Color::create(0.13, 0.15, 0.2, 1.0),
		}
	}

	//==============================================================================
	// 창 노드 만들기. (layer_id 창 층 노드 밑에 name 노드를 만들고 창을 붙인다, 띄우는 것은 UIWindowLayer::open_window)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, layer_id: NodeId, name: &str, behavior: Box<dyn UIWindowBehavior>) -> NodeId {
		let window_id = node_tree.create_child(layer_id, name);
		node_tree.add_component(window_id, UIWindow::create_with_behavior(behavior));
		window_id
	}

	//==============================================================================
	// 창 닫기. (창 노드의 창 층을 찾아 close_window, 씬 코드와 단추 콜백에서 부른다, 떠 있지 않으면 아무 일도 없다)
	//==============================================================================
	pub fn close_in_tree(node_tree: &mut NodeTree, window_id: NodeId) {
		let window_layer_id = match node_tree.get_node(window_id).and_then(|window_node| window_node.get_component::<UIWindow>()).and_then(|window| window.get_window_layer_id()) {
			Some(window_layer_id) => {
				window_layer_id
			}
			None => {
				return;
			}
		};
		node_tree.with_component_mut::<UIWindowLayer, _>(window_layer_id, |window_layer, tree| {
			window_layer.close_window(tree, window_id);
		});
	}

	//==============================================================================
	// 창 다시 놓기. (내용이 바뀌어 판의 세로가 달라질 때, 씬 코드에서)
	//==============================================================================
	pub fn refresh_layout_in_tree(node_tree: &mut NodeTree, window_id: NodeId) {
		node_tree.with_component_mut::<UIWindow, _>(window_id, |window, tree| {
			window.refresh_layout(tree);
		});
	}

	//==============================================================================
	// 덮어쓸 자리 꺼내기. (구체 타입이 맞으면)
	//==============================================================================
	pub fn get_behavior<T: UIWindowBehavior>(&self) -> Option<&T> {
		self.behavior.as_ref().and_then(|behavior| behavior.as_any().downcast_ref::<T>())
	}

	//==============================================================================
	// 덮어쓸 자리 꺼내기. (변경용)
	//==============================================================================
	pub fn get_behavior_mut<T: UIWindowBehavior>(&mut self) -> Option<&mut T> {
		self.behavior.as_mut().and_then(|behavior| behavior.as_any_mut().downcast_mut::<T>())
	}

	//==============================================================================
	// 창의 노드와 지금 놓인 값. (만들기 전이면 None)
	//==============================================================================
	pub fn get_parts(&self) -> Option<UIWindowParts> {
		match (self.node_id, self.dim_id, self.frame_id, self.panel_id) {
			(Some(window_id), Some(dim_id), Some(frame_id), Some(panel_id)) => {
				Some(UIWindowParts {
					window_id,
					dim_id,
					frame_id,
					panel_id,
					view_width: self.view_width,
					view_height: self.view_height,
					margin: self.margin,
					panel_height: self.panel_height,
					panel_rect: self.panel_rect,
					panel_scale: self.panel_scale,
				})
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 판 노드. (내용을 여기에 붙인다)
	//==============================================================================
	pub fn get_panel_node(&self) -> Option<NodeId> {
		self.panel_id
	}

	//==============================================================================
	// 틀 노드. (판과 함께 커지고 주는 꾸밈을 붙인다)
	//==============================================================================
	pub fn get_frame_node(&self) -> Option<NodeId> {
		self.frame_id
	}

	//==============================================================================
	// 어둠 노드.
	//==============================================================================
	pub fn get_dim_node(&self) -> Option<NodeId> {
		self.dim_id
	}

	//==============================================================================
	// 판의 Paint. (기본 판 그림, 바꾸거나 끌 때. 붙기 전이면 None)
	//==============================================================================
	pub fn get_panel_paint<'a>(&self, node_tree: &'a NodeTree) -> Option<&'a Paint> {
		self.panel_id.and_then(|panel_id| node_tree.get_node(panel_id)).and_then(|panel_node| panel_node.get_component::<Paint>())
	}

	//==============================================================================
	// 판의 Paint. (변경용)
	//==============================================================================
	pub fn get_panel_paint_mut<'a>(&self, node_tree: &'a mut NodeTree) -> Option<&'a mut Paint> {
		self.panel_id.and_then(move |panel_id| node_tree.get_node_mut(panel_id)).and_then(|panel_node| panel_node.get_component_mut::<Paint>())
	}

	//==============================================================================
	// 어둠의 Paint. (붙기 전이면 None)
	//==============================================================================
	pub fn get_dim_paint<'a>(&self, node_tree: &'a NodeTree) -> Option<&'a Paint> {
		self.dim_id.and_then(|dim_id| node_tree.get_node(dim_id)).and_then(|dim_node| dim_node.get_component::<Paint>())
	}

	//==============================================================================
	// 어둠의 Paint. (변경용)
	//==============================================================================
	pub fn get_dim_paint_mut<'a>(&self, node_tree: &'a mut NodeTree) -> Option<&'a mut Paint> {
		self.dim_id.and_then(move |dim_id| node_tree.get_node_mut(dim_id)).and_then(|dim_node| dim_node.get_component_mut::<Paint>())
	}

	//==============================================================================
	// 마지막으로 받은 영역의 가로. (UI 층 단위)
	//==============================================================================
	pub fn get_view_width(&self) -> f32 {
		self.view_width
	}

	//==============================================================================
	// 마지막으로 받은 영역의 세로. (UI 층 단위)
	//==============================================================================
	pub fn get_view_height(&self) -> f32 {
		self.view_height
	}

	//==============================================================================
	// 영역 가장자리와 판 사이 거리 반환.
	//==============================================================================
	pub fn get_margin(&self) -> f32 {
		self.margin
	}

	//==============================================================================
	// 판의 세로 반환. (set_panel_height 의 값)
	//==============================================================================
	pub fn get_panel_height(&self) -> f32 {
		self.panel_height
	}

	//==============================================================================
	// 마지막으로 놓은 판의 자리. (창 좌표, 움직임 전, 줄인 배율을 곱한 크기)
	//==============================================================================
	pub fn get_panel_rect(&self) -> Rect {
		self.panel_rect
	}

	//==============================================================================
	// 판이 영역보다 커서 줄인 배율. (1 이하)
	//==============================================================================
	pub fn get_panel_scale(&self) -> f32 {
		self.panel_scale
	}

	//==============================================================================
	// 휠 받기. (UIWindowBehavior::handle_wheel, 받았으면 true, 원본 handleWheel)
	//==============================================================================
	pub fn handle_wheel(&mut self, node_tree: &mut NodeTree, view_input_position: Vector2, wheel_delta: Vector2) -> bool {
		match (self.get_parts(), self.behavior.as_mut()) {
			(Some(parts), Some(behavior)) => {
				behavior.handle_wheel(node_tree, &parts, view_input_position, wheel_delta)
			}
			_ => {
				false
			}
		}
	}

	//==============================================================================
	// 움직임 반환.
	//==============================================================================
	pub fn get_popup_motion(&self) -> &PopupMotion {
		&self.popup_motion
	}

	//==============================================================================
	// 움직임 반환. (변경용, set_instant_enabled 로 움직임 없이 열고 닫는다)
	//==============================================================================
	pub fn get_popup_motion_mut(&mut self) -> &mut PopupMotion {
		&mut self.popup_motion
	}

	//==============================================================================
	// 바깥 누르기(와 창 층의 close_top_window)로 닫히는지 설정. (기본 true)
	//==============================================================================
	pub fn set_outside_close_enabled(&mut self, is_outside_close_enabled: bool) {
		self.is_outside_close_enabled = is_outside_close_enabled;
	}

	//==============================================================================
	// 바깥 누르기로 닫히는지.
	//==============================================================================
	pub fn is_outside_close_enabled(&self) -> bool {
		self.is_outside_close_enabled
	}

	//==============================================================================
	// 창 층에서 내려갈 때 알림 설정.
	//==============================================================================
	pub fn set_close_event(&mut self, close_event: Option<UIWindowEvent>) {
		self.close_event = close_event;
	}

	//==============================================================================
	// 창의 가로 한계 설정. (영역이 좁으면 그보다 좁아진다, 다음 놓을 때부터)
	//==============================================================================
	pub fn set_maximum_width(&mut self, maximum_width: f32) {
		self.maximum_width = maximum_width.max(1.0);
	}

	//==============================================================================
	// 창의 가로 한계 반환.
	//==============================================================================
	pub fn get_maximum_width(&self) -> f32 {
		self.maximum_width
	}

	//==============================================================================
	// 영역 가장자리와 판 사이 거리 설정.
	//==============================================================================
	pub fn set_margin(&mut self, margin: f32) {
		self.margin = margin.max(0.0);
	}

	//==============================================================================
	// 판의 세로 설정. (layout_content 를 덮어쓰지 않은 창이 쓴다)
	//==============================================================================
	pub fn set_panel_height(&mut self, panel_height: f32) {
		self.panel_height = panel_height.max(1.0);
	}

	//==============================================================================
	// 어둠 색 설정. (붙기 전에 정하거나, 붙은 뒤면 어둠 노드의 Paint 를 바꾼다)
	//==============================================================================
	pub fn set_dim_color(&mut self, dim_color: impl Into<Color>) {
		self.dim_color = dim_color.into();
	}

	//==============================================================================
	// 판 색 설정. (붙기 전에 정하거나, 붙은 뒤면 판 노드의 Paint 를 바꾼다)
	//==============================================================================
	pub fn set_panel_color(&mut self, panel_color: impl Into<Color>) {
		self.panel_color = panel_color.into();
	}

	//==============================================================================
	// 띄운 창 층 노드. (띄우기 전이나 내려간 뒤면 None)
	//==============================================================================
	pub fn get_window_layer_id(&self) -> Option<NodeId> {
		self.window_layer_id
	}

	//==============================================================================
	// 띄운 창 층 노드 설정. (창 층이 부른다)
	//==============================================================================
	pub fn set_window_layer_id(&mut self, window_layer_id: Option<NodeId>) {
		self.window_layer_id = window_layer_id;
	}

	//==============================================================================
	// 영역 밖으로 더 덮는 너비 반환. (위, 오른쪽, 아래, 왼쪽)
	//==============================================================================
	pub fn get_view_bleed(&self) -> [f32; 4] {
		self.view_bleed
	}

	//==============================================================================
	// 영역 밖으로 더 덮는 너비 알리기. (세이프에어리어, UI 층 단위, 어둠이 그 밑까지 덮는다)
	//==============================================================================
	pub fn set_view_bleed(&mut self, node_tree: &mut NodeTree, top: f32, right: f32, bottom: f32, left: f32) {
		self.view_bleed = [top.max(0.0), right.max(0.0), bottom.max(0.0), left.max(0.0)];
		self.place_dim_node(node_tree);
	}

	//==============================================================================
	// 어둠 놓기. (영역과 그 밖의 세이프에어리어까지)
	//==============================================================================
	fn place_dim_node(&self, node_tree: &mut NodeTree) {
		let [bleed_top, bleed_right, bleed_bottom, bleed_left] = self.view_bleed;
		if let Some(dim_node) = self.dim_id.and_then(|dim_id| node_tree.get_node_mut(dim_id)) {
			dim_node.set_local_position(Vector2::create(-bleed_left, -bleed_top));
			dim_node.set_content_size(Vector2::create(self.view_width + bleed_left + bleed_right, self.view_height + bleed_top + bleed_bottom));
		}
	}

	//==============================================================================
	// 영역의 크기 알리기. (어둠을 덮고, 판의 가로를 정해 내용을 놓은 뒤 가운데에 둔다)
	//==============================================================================
	pub fn set_view_size(&mut self, node_tree: &mut NodeTree, view_width: f32, view_height: f32) {
		self.view_width = view_width.max(1.0);
		self.view_height = view_height.max(1.0);
		self.refresh_layout(node_tree);
	}

	//==============================================================================
	// 다시 놓기. (마지막 영역의 크기로)
	//==============================================================================
	pub fn refresh_layout(&mut self, node_tree: &mut NodeTree) {
		let window_id = match self.node_id {
			Some(window_id) => {
				window_id
			}
			None => {
				return;
			}
		};
		if let Some(window_node) = node_tree.get_node_mut(window_id) {
			window_node.set_content_size(Vector2::create(self.view_width, self.view_height));
		}
		self.place_dim_node(node_tree);
		let panel_width = (self.view_width - self.margin * 2.0).min(self.maximum_width).max(1.0);
		let mut behavior = match self.behavior.take() {
			Some(behavior) => {
				behavior
			}
			None => {
				return;
			}
		};
		if let Some(parts) = self.get_parts() {
			let panel_height = behavior.layout_content(node_tree, &parts, panel_width).max(1.0);
			// 판이 영역보다 키가 크면 통째로 줄인다. (배치는 그대로, 글자와 단추가 함께 작아진다)
			let available_height = (self.view_height - self.margin * 2.0).max(1.0);
			let panel_scale = if panel_height > available_height {
				available_height / panel_height
			}
			else {
				1.0
			};
			let scaled_width = panel_width * panel_scale;
			let scaled_height = panel_height * panel_scale;
			let panel_left = (self.view_width - scaled_width) * 0.5;
			let panel_top = (self.margin * panel_scale).max((self.view_height - scaled_height) * 0.5);
			self.panel_rect = Rect::create(panel_left, panel_top, scaled_width, scaled_height);
			self.panel_scale = panel_scale;
			if let Some(frame_node) = node_tree.get_node_mut(parts.frame_id) {
				frame_node.set_content_size(Vector2::create(panel_width, panel_height));
				frame_node.set_local_position(Vector2::create(panel_left + scaled_width * 0.5, panel_top + scaled_height * 0.5));
			}
			if let Some(placed_parts) = self.get_parts() {
				behavior.layout_panel(node_tree, &placed_parts, panel_width, panel_height);
				behavior.apply_motion(node_tree, &placed_parts, &self.popup_motion);
			}
		}
		self.behavior = Some(behavior);
	}

	//==============================================================================
	// 판 놓기 기본. (판을 틀의 왼쪽 위에 그 크기로)
	//==============================================================================
	pub fn layout_panel_default(node_tree: &mut NodeTree, parts: &UIWindowParts, panel_width: f32, panel_height: f32) {
		if let Some(panel_node) = node_tree.get_node_mut(parts.panel_id) {
			panel_node.set_local_position(Vector2::zero());
			panel_node.set_content_size(Vector2::create(panel_width, panel_height));
		}
	}

	//==============================================================================
	// 움직임 걸기 기본. (틀 배율 = 줄인 배율 x 움직임 배율, 어둠 진하기 = 움직임 비율)
	//==============================================================================
	pub fn apply_motion_default(node_tree: &mut NodeTree, parts: &UIWindowParts, popup_motion: &PopupMotion) {
		let frame_scale = parts.panel_scale * popup_motion.get_scale();
		if let Some(frame_node) = node_tree.get_node_mut(parts.frame_id) {
			frame_node.set_local_scale(Vector2::create(frame_scale, frame_scale));
		}
		if let Some(dim_node) = node_tree.get_node_mut(parts.dim_id) {
			dim_node.set_local_opacity(popup_motion.get_dim_ratio());
		}
	}

	//==============================================================================
	// 덮기 기본. (덮이면 창 노드를 끄고 드러나면 켠다)
	//==============================================================================
	pub fn set_covered_default(node_tree: &mut NodeTree, parts: &UIWindowParts, is_covered: bool) {
		if let Some(window_node) = node_tree.get_node_mut(parts.window_id) {
			window_node.set_active(!is_covered);
		}
	}

	//==============================================================================
	// 덮였는지 알리기. (창 층이 부른다)
	//==============================================================================
	pub fn set_covered(&mut self, node_tree: &mut NodeTree, is_covered: bool) {
		if let (Some(parts), Some(behavior)) = (self.get_parts(), self.behavior.as_mut()) {
			behavior.set_covered(node_tree, &parts, is_covered);
		}
	}

	//==============================================================================
	// 노드에 움직임 걸기.
	//==============================================================================
	fn apply_motion(&mut self, node_tree: &mut NodeTree) {
		if let (Some(parts), Some(behavior)) = (self.get_parts(), self.behavior.as_mut()) {
			behavior.apply_motion(node_tree, &parts, &self.popup_motion);
		}
	}

	//==============================================================================
	// 어둠과 판의 누름 켜고 끄기.
	//==============================================================================
	fn set_parts_interactable(&self, node_tree: &mut NodeTree, is_interactable: bool) {
		for part_id in [self.dim_id, self.panel_id].into_iter().flatten() {
			if let Some(part_node) = node_tree.get_node_mut(part_id) {
				part_node.set_interactable(is_interactable);
			}
		}
	}

	//==============================================================================
	// 여는 움직임 시작. (창 층이 띄울 때 부른다, 창 노드를 켠다)
	//==============================================================================
	pub fn start_open_motion(&mut self, node_tree: &mut NodeTree) {
		if let Some(window_node) = self.node_id.and_then(|window_id| node_tree.get_node_mut(window_id)) {
			window_node.set_active(true);
		}
		self.set_parts_interactable(node_tree, true);
		self.popup_motion.open();
		self.apply_motion(node_tree);
	}

	//==============================================================================
	// 닫는 움직임 시작. (창 층이 내릴 때 부른다, 그동안은 누름을 받지 않는다)
	//==============================================================================
	pub fn start_close_motion(&mut self, node_tree: &mut NodeTree) {
		self.set_parts_interactable(node_tree, false);
		self.popup_motion.close();
		self.apply_motion(node_tree);
	}

	//==============================================================================
	// 닫는 움직임이 끝났는지.
	//==============================================================================
	pub fn is_close_motion_finished(&self) -> bool {
		!self.popup_motion.is_visible()
	}

	//==============================================================================
	// 창 층에서 내려감. (창 층이 부른다: 알림 → UIWindowBehavior::handle_closed)
	//==============================================================================
	pub fn handle_closed(&mut self) {
		self.window_layer_id = None;
		if let Some(close_event) = self.close_event.as_mut() {
			close_event();
		}
		if let Some(behavior) = self.behavior.as_mut() {
			behavior.handle_closed();
		}
	}

	//==============================================================================
	// 지우기 직전. (창 층이 부른다)
	//==============================================================================
	pub fn dispose(&mut self, node_tree: &mut NodeTree) {
		if let (Some(parts), Some(behavior)) = (self.get_parts(), self.behavior.as_mut()) {
			behavior.dispose(node_tree, &parts);
		}
	}
}

impl Component for UIWindow {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UIWindow"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 창 꾸미기(왼쪽 위 피봇, 띄우기 전이라 끔), 어둠, 틀, 판, 그다음 UIWindowBehavior::build)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.node_id = Some(node_id);
		let name = node_tree[node_id].get_name().to_string();
		let window_node = &mut node_tree[node_id];
		window_node.set_pivot(Pivot::TOP_LEFT);
		window_node.set_anchor(Pivot::TOP_LEFT);
		window_node.set_active(false);

		// 바깥. (누름을 삼키고, 바깥 닫기가 켜져 있으면 닫는다)
		let dim_id = node_tree.create_child(node_id, &format!("{}Dim", name));
		let dim_node = &mut node_tree[dim_id];
		dim_node.set_pivot(Pivot::TOP_LEFT);
		dim_node.set_anchor(Pivot::TOP_LEFT);
		dim_node.set_interactable(true);
		dim_node.add_component(Paint::create_with_color(self.dim_color));
		if let Some(dim_button) = node_tree.add_component(dim_id, UIButton::new()) {
			dim_button.exclude_component_from_tint::<Paint>(dim_id);
			dim_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				let is_outside_close_enabled = context.get_node_tree().get_node(node_id).and_then(|window_node| window_node.get_component::<UIWindow>()).map(|window| window.is_outside_close_enabled()).unwrap_or(false);
				if is_outside_close_enabled {
					UIWindow::close_in_tree(context.get_node_tree_mut(), node_id);
				}
			}));
		}
		self.dim_id = Some(dim_id);

		// 틀. (판 가운데를 두고 커지고 준다)
		let frame_id = node_tree.create_child(node_id, &format!("{}Frame", name));
		let frame_node = &mut node_tree[frame_id];
		frame_node.set_pivot(Pivot::MIDDLE_CENTER);
		frame_node.set_anchor(Pivot::TOP_LEFT);
		self.frame_id = Some(frame_id);

		// 판. (누름을 삼켜 바깥 닫기로 닫히지 않게 한다)
		let panel_id = node_tree.create_child(frame_id, &format!("{}Panel", name));
		let panel_node = &mut node_tree[panel_id];
		panel_node.set_pivot(Pivot::TOP_LEFT);
		panel_node.set_anchor(Pivot::TOP_LEFT);
		panel_node.set_interactable(true);
		let panel_paint = panel_node.add_component(Paint::create_with_color(self.panel_color));
		panel_paint.set_round_size(DEFAULT_PANEL_ROUND_SIZE);
		self.panel_id = Some(panel_id);

		if let (Some(parts), Some(behavior)) = (self.get_parts(), self.behavior.as_mut()) {
			behavior.build(node_tree, &parts);
		}
	}

	//==============================================================================
	// 갱신. (움직임 → 노드에 반영, 그린 뒤 알게 된 크기를 기다리던 창은 다시 놓는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.node_id = Some(context.get_node_id());
		self.popup_motion.tick(time_delta);
		let node_tree = context.get_node_tree_mut();
		let is_layout_pending = match (self.get_parts(), self.behavior.as_ref()) {
			(Some(parts), Some(behavior)) => {
				behavior.is_layout_pending(node_tree, &parts)
			}
			_ => {
				false
			}
		};
		if is_layout_pending {
			self.refresh_layout(node_tree);
		}
		else {
			self.apply_motion(node_tree);
		}
	}

	//==============================================================================
	// 휠. (그 자리의 스크롤 뷰가 받지 않은 휠, 창이 받고 늘 삼킨다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
		let view_input_position = context.get_engine().get_input_manager().get_view_input_position();
		self.handle_wheel(context.get_node_tree_mut(), view_input_position, wheel_delta);
		true
	}
}
