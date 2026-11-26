//==============================================================================
//! 창 쌓기의 창 하나. (vanilla.js 의 ui/uiwindow.js, UIWindow)
//! - 영역 전체를 어둡게 덮고(뒤로 누름이 가지 않는다) 가운데에 판을 띄운다. 어둠은 세이프에어리어(set_view_bleed)까지 덮는다.
//! - 판은 영역의 가로에 맞춰 넓어지되 maximum_width 를 넘지 않는다. 세로는 layout_content 가 돌려준 값이고, 영역보다 크면 판을 통째로 줄인다.
//! - 열고 닫는 움직임은 PopupMotion 이다. 닫는 움직임이 끝나면 창 층(UIWindowLayer)이 노드를 지운다. (그동안은 누름을 받지 않는다)
//! - 바깥(어둠)을 누르면 닫힌다. 창마다 set_outside_close_enabled 로 끈다. 창 층의 close_top_window(뒤로 가기)도 이 값을 따른다.
//! - 노드 구성: 창(영역 크기) ├ 어둠(Paint + UIButton) └ 틀(판 가운데 피봇, 움직임 배율) └ 판(Paint 한 장, 내용은 여기)
//! - 원본과 다른 것:
//!   - 원본은 WorldNode 를 상속한 노드이고 파생 클래스가 layoutContent, layoutPanel, applyMotion, setCovered, handleWheel, dispose 를 덮어쓴다.
//!     러스트 판은 창 노드에 붙는 컴포넌트이고, 덮어쓸 자리는 UIWindowBehavior 트레이트다. (기본 구현은 UIWindowDefaultBehavior,
//!     덮어쓴 쪽이 원본 super 처럼 기본을 부르려면 UIWindow::layout_panel_default, apply_motion_default, set_covered_default)
//!     판 모양(그림자, 테, 글로시)은 게임이 build 에서 틀에 꾸밈 노드를 붙이고(판 뒤는 set_sibling_index(0)) layout_panel 에서 놓는다.
//!   - 창 노드는 UIWindow::create(node_tree, layer_id, name, behavior) 로 창 층 노드 밑에 만든다. 닫는 움직임이 끝나면 층에서 떼어 내고 지우지 않으므로
//!     원본처럼 같은 창 노드를 다시 띄울 수 있다. (UIWindowLayer::open_window) 원본 창은 띄울 때 층에 붙으므로 create 는 띄우기 전까지 노드를 끈다.
//!     붙을 때는 원본 생성자처럼 노드를 끄지 않는다. (UI 문서가 창 노드를 원본처럼 켜진 채로 읽고 쓴다)
//!   - 닫기는 창 층을 거친다. 씬과 콜백 코드는 UIWindow::close_in_tree(node_tree, window_id) 를 부른다.
//!   - 알림(set_close_event)은 인자 없는 `Box<dyn FnMut()>` 다. (PopupMotion 알림과 같은 꼴, 노드를 바꿀 일은 씬이 tick 에서 상태를 보고 한다)
//!   - 휠은 원본처럼 GameSceneBase 가 창 층 안을 가리킨 휠을 UIWindowLayer::scroll_window_at 으로 넘겨 맨 위 창(UIWindowBehavior::handle_wheel)이 먼저 받고,
//!     받지 않으면 그 자리의 스크롤 뷰가 받는다. GameSceneBase 를 거치지 않는 엔진의 휠 전달(가리킨 노드부터 부모 쪽)로 창에 온 휠도 창이 받고 늘 삼킨다.
//!   - 판과 어둠의 Paint 는 노드의 컴포넌트라 get_panel_paint, get_dim_paint 가 트리를 받는다.
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

/// 기본 모양. (창의 가로 한계, 영역 가장자리와 띄우는 거리, 판의 처음 세로, 둥근 정도)
const DEFAULT_MAXIMUM_WIDTH: f64 = 480.0;
const DEFAULT_MARGIN: f64 = 16.0;
const DEFAULT_PANEL_HEIGHT: f64 = 240.0;
const DEFAULT_PANEL_ROUND_SIZE: f64 = 16.0;

//==============================================================================
/// 알림. (인자 없음)
//==============================================================================
pub type UIWindowEvent = Box<dyn FnMut()>;

//==============================================================================
/// 창의 노드와 지금 놓인 값. (UIWindowBehavior 가 받는다)
///
/// # 예제
/// 띄운 창의 판 자리 읽기
/// ```rust
/// use vanilla::{NodeTree, Rect, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
/// node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
/// 	window_layer.set_view_size(tree, 800.0, 600.0, 1.0);
/// });
/// let window_id = UIWindow::create(&mut node_tree, layer_id, "notice", Box::new(UIWindowDefaultBehavior));
/// node_tree[window_id].get_component_mut::<UIWindow>().expect("window").set_panel_height(300.0);
/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
/// let parts = node_tree[window_id].get_component::<UIWindow>().expect("window").get_parts().expect("parts");
/// assert_eq!(parts.window_id, window_id);
/// assert_eq!(parts.view_width, 800.0);
/// assert_eq!(parts.panel_rect, Rect::create(160.0, 150.0, 480.0, 300.0));
/// assert_eq!(parts.panel_scale, 1.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIWindowParts {
	/// 창 노드. (UIWindow 가 붙은 노드, 영역 크기)
	pub window_id: NodeId,
	/// 어둠 노드. (Paint + UIButton, 바깥 누르기를 받는다)
	pub dim_id: NodeId,
	/// 틀 노드. (판 가운데 피봇, 움직임 배율이 걸린다)
	pub frame_id: NodeId,
	/// 판 노드. (Paint 한 장, 내용을 여기에 붙인다)
	pub panel_id: NodeId,
	/// 마지막으로 받은 영역의 크기. (UI 층 단위)
	pub view_width: f64,
	/// 마지막으로 받은 영역의 세로. (UI 층 단위)
	pub view_height: f64,
	/// 영역 가장자리와 판 사이 거리. (UI 층 단위, 기본 16)
	pub margin: f64,
	/// layout_content 기본값이 돌려주는 판의 세로.
	pub panel_height: f64,
	/// 마지막으로 놓은 판의 자리. (창 좌표, 움직임 전, 줄인 배율을 곱한 크기)
	pub panel_rect: Rect,
	/// 판이 영역보다 커서 줄인 배율. (1 이하)
	pub panel_scale: f64,
}

//==============================================================================
/// 창의 덮어쓸 자리. (원본 UIWindow 파생 클래스가 덮어쓰는 메서드)
/// - as_any, as_any_mut 만 꼭 구현하고(`self` 를 돌려준다) 나머지는 필요한 것만 덮어쓴다.
///
/// # 예제
/// 판 세로를 정하고 닫힘을 받는 창
/// ```rust
/// use std::any::Any;
/// use vanilla::{NodeTree, UIWindow, UIWindowBehavior, UIWindowLayer, UIWindowParts};
///
/// struct NoticeWindow {
/// 	is_closed: bool,
/// }
///
/// impl UIWindowBehavior for NoticeWindow {
/// 	fn as_any(&self) -> &dyn Any {
/// 		self
/// 	}
///
/// 	fn as_any_mut(&mut self) -> &mut dyn Any {
/// 		self
/// 	}
///
/// 	fn layout_content(&mut self, _node_tree: &mut NodeTree, _parts: &UIWindowParts, _panel_width: f64) -> f64 {
/// 		200.0
/// 	}
///
/// 	fn handle_closed(&mut self) {
/// 		self.is_closed = true;
/// 	}
/// }
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
/// node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
/// 	window_layer.set_view_size(tree, 800.0, 600.0, 1.0);
/// });
/// let window_id = UIWindow::create(&mut node_tree, layer_id, "notice", Box::new(NoticeWindow { is_closed: false }));
/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
/// assert_eq!(node_tree[window_id].get_component::<UIWindow>().expect("window").get_panel_rect().size.y, 200.0);
/// UIWindow::close_in_tree(&mut node_tree, window_id);
/// let window = node_tree[window_id].get_component::<UIWindow>().expect("window");
/// assert!(window.get_behavior::<NoticeWindow>().expect("notice").is_closed);
/// ```
//==============================================================================
pub trait UIWindowBehavior: Any {
	//==============================================================================
	/// 구체 타입으로 꺼내기. (UIWindow::get_behavior 가 쓴다, 변경용은 as_any_mut 으로 get_behavior_mut 이 쓴다)
	///
	/// # 돌려줌
	/// 자기 자신. (`&dyn Any`, UIWindow::get_behavior 가 downcast_ref 로 꺼낸다)
	//==============================================================================
	fn as_any(&self) -> &dyn Any;

	//==============================================================================
	/// 구체 타입으로 꺼내기. (변경용, UIWindow::get_behavior_mut 이 쓴다)
	///
	/// # 돌려줌
	/// 자기 자신. (`&mut dyn Any`, UIWindow::get_behavior_mut 이 downcast_mut 으로 꺼낸다)
	//==============================================================================
	fn as_any_mut(&mut self) -> &mut dyn Any;

	//==============================================================================
	/// 창 노드를 다 만든 뒤 한 번. (내용과 꾸밈 노드를 만든다, 원본 파생 클래스 생성자 자리)
	///
	/// # 인자
	/// - `_node_tree`: 창이 든 노드 트리.
	/// - `_parts`: 창의 노드. (내용은 판 parts.panel_id 밑에, 판과 함께 커지고 주는 꾸밈은 틀 parts.frame_id 밑에 붙인다. 영역 크기는 아직 처음 값 1)
	//==============================================================================
	fn build(&mut self, _node_tree: &mut NodeTree, _parts: &UIWindowParts) {
	}

	//==============================================================================
	/// 내용 놓기. (판 안에 내용을 놓고 판의 세로를 돌려준다, 기본은 parts.panel_height)
	///
	/// # 인자
	/// - `_node_tree`: 창이 든 노드 트리.
	/// - `parts`: 창의 노드와 지금 놓인 값. (view_width, view_height 는 이번 영역의 크기, panel_rect 와 panel_scale 은 아직 지난번 값)
	/// - `_panel_width`: 판의 가로. (영역 가로 - margin x 2 를 가로 한계로 자른 값, 1 이상, UI 층 단위)
	///
	/// # 돌려줌
	/// 판의 세로. (UI 층 단위, 1 보다 작으면 1 로 올린다. 영역 세로 - margin x 2 보다 크면 창이 판을 통째로 줄인다)
	//==============================================================================
	fn layout_content(&mut self, _node_tree: &mut NodeTree, parts: &UIWindowParts, _panel_width: f64) -> f64 {
		parts.panel_height
	}

	//==============================================================================
	/// 판 놓기. (판의 크기를 정한다, 꾸밈이 있으면 덮어써 layout_panel_default 뒤에 놓는다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `parts`: 창의 노드와 이번에 놓인 값. (panel_rect, panel_scale 이 정해진 뒤)
	/// - `panel_width`: 판의 가로. (줄이기 전, UI 층 단위)
	/// - `panel_height`: layout_content 가 돌려준 판의 세로. (줄이기 전, 1 이상)
	//==============================================================================
	fn layout_panel(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, panel_width: f64, panel_height: f64) {
		UIWindow::layout_panel_default(node_tree, parts, panel_width, panel_height);
	}

	//==============================================================================
	/// 움직임을 틀과 어둠에 걸기. (기본: 틀 배율 = 줄인 배율 x 움직임 배율, 어둠 진하기 = 움직임 비율)
	/// - 다시 놓을 때, 열고 닫는 움직임을 시작할 때, 매 tick 부른다.
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `parts`: 창의 노드와 지금 놓인 값.
	/// - `popup_motion`: 창의 움직임. (get_scale 은 틀 배율, get_dim_ratio 는 0 ~ 1 어둠 진하기)
	//==============================================================================
	fn apply_motion(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, popup_motion: &PopupMotion) {
		UIWindow::apply_motion_default(node_tree, parts, popup_motion);
	}

	//==============================================================================
	/// 다른 창에 덮였는지 알리기. (기본은 덮이면 창 노드를 끄고 드러나면 켠다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `parts`: 창의 노드와 지금 놓인 값.
	/// - `is_covered`: 위에 다른 창이 떠 있으면 true, 맨 위가 되면 false.
	//==============================================================================
	fn set_covered(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, is_covered: bool) {
		UIWindow::set_covered_default(node_tree, parts, is_covered);
	}

	//==============================================================================
	/// 그 자리의 스크롤 뷰가 받지 않은 휠. (받았으면 true, 창은 어느 쪽이든 휠을 삼킨다)
	///
	/// # 인자
	/// - `_node_tree`: 창이 든 노드 트리.
	/// - `_parts`: 창의 노드와 지금 놓인 값.
	/// - `_view_input_position`: 휠을 굴린 자리. (뷰 좌표)
	/// - `_wheel_delta`: 휠을 굴린 양.
	///
	/// # 돌려줌
	/// 받았으면 true. (기본 false, UIWindowLayer::scroll_window_at 에서 false 면 그 자리의 스크롤 뷰에 넘긴다)
	//==============================================================================
	fn handle_wheel(&mut self, _node_tree: &mut NodeTree, _parts: &UIWindowParts, _view_input_position: Vector2, _wheel_delta: Vector2) -> bool {
		false
	}

	//==============================================================================
	/// 창 층에서 내려감. (닫기, 바깥 누르기, close_all_windows 모두, set_close_event 알림 다음)
	//==============================================================================
	fn handle_closed(&mut self) {
	}

	//==============================================================================
	/// 레이아웃을 다시 해야 하는지. (그린 뒤에야 알 수 있는 크기를 기다릴 때 true, 창이 tick 에서 다시 놓는다)
	///
	/// # 인자
	/// - `_node_tree`: 창이 든 노드 트리.
	/// - `_parts`: 창의 노드와 지금 놓인 값.
	///
	/// # 돌려줌
	/// 다시 놓아야 하면 true. (true 면 그 tick 에 UIWindow::refresh_layout 으로 다시 놓는다, 기본 false)
	//==============================================================================
	fn is_layout_pending(&self, _node_tree: &NodeTree, _parts: &UIWindowParts) -> bool {
		false
	}

	//==============================================================================
	/// 창 층이 창을 떼어 낼 때. (엔진 밖에 붙인 것을 정리한다, 노드는 지우지 않고 남아 다시 띄울 수 있다)
	///
	/// # 인자
	/// - `_node_tree`: 창이 든 노드 트리.
	/// - `_parts`: 창의 노드와 지금 놓인 값.
	//==============================================================================
	fn dispose(&mut self, _node_tree: &mut NodeTree, _parts: &UIWindowParts) {
	}
}

//==============================================================================
/// 기본 창. (덮어쓴 것 없음, 판 세로는 UIWindow::set_panel_height)
///
/// # 예제
/// 판 세로만 정한 창
/// ```rust
/// use vanilla::{UIWindow, UIWindowDefaultBehavior};
///
/// let mut window = UIWindow::create_with_behavior(Box::new(UIWindowDefaultBehavior));
/// window.set_panel_height(320.0);
/// assert_eq!(window.get_panel_height(), 320.0);
/// assert!(window.get_behavior::<UIWindowDefaultBehavior>().is_some());
/// ```
//==============================================================================
pub struct UIWindowDefaultBehavior;

impl UIWindowBehavior for UIWindowDefaultBehavior {
	//==============================================================================
	/// 구체 타입으로 꺼내기.
	//==============================================================================
	fn as_any(&self) -> &dyn Any {
		self
	}

	//==============================================================================
	/// 구체 타입으로 꺼내기. (변경용)
	//==============================================================================
	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}
}

//==============================================================================
/// 창.
///
/// # 예제
/// 창 층에 띄우고 닫기
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{Engine, EngineConfiguration, NodeTree, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
/// node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
/// 	window_layer.set_view_size(tree, 800.0, 600.0, 1.0);
/// });
/// let window_id = UIWindow::create(&mut node_tree, layer_id, "settings", Box::new(UIWindowDefaultBehavior));
/// let close_count = Rc::new(Cell::new(0));
/// let close_counter = close_count.clone();
/// if let Some(window) = node_tree[window_id].get_component_mut::<UIWindow>() {
/// 	window.set_panel_height(300.0);
/// 	window.set_close_event(Some(Box::new(move || {
/// 		close_counter.set(close_counter.get() + 1);
/// 	})));
/// }
/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
/// assert!(node_tree[window_id].is_active());
/// node_tree.tick(&engine, 0.5);
/// UIWindow::close_in_tree(&mut node_tree, window_id);
/// assert_eq!(close_count.get(), 1);
/// node_tree.tick(&engine, 0.3);
/// node_tree.tick(&engine, 0.016);
/// assert_eq!(node_tree.get_parent_id(window_id), None);
/// ```
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
	/// 띄운 창 층의 노드. (띄우기 전이나 내려간 뒤에는 None)
	window_layer_id: Option<NodeId>,
	view_width: f64,
	view_height: f64,
	/// 영역 밖으로 더 덮는 너비. (위, 오른쪽, 아래, 왼쪽)
	view_bleed: [f64; 4],
	maximum_width: f64,
	margin: f64,
	panel_height: f64,
	panel_rect: Rect,
	panel_scale: f64,
	dim_color: Color,
	panel_color: Color,
}

#[crate::reflect]
impl UIWindow {
	//==============================================================================
	/// 생성. (노드에는 node_tree.add_component 로 붙인다, 보통은 UIWindow::create)
	///
	/// # 인자
	/// - `behavior`: 덮어쓸 자리. (UIWindowDefaultBehavior 나 게임이 구현한 값)
	///
	/// # 돌려줌
	/// 붙기 전의 창. (가로 한계 480, 여백 16, 판 세로 240, 바깥 닫기 켬)
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
	/// 창 노드 만들기. (layer_id 창 층 노드 밑에 name 노드를 만들고 창을 붙인다, 띄우는 것은 UIWindowLayer::open_window)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `layer_id`: 창 층 노드. (UIWindowLayer::create 가 돌려준 것)
	/// - `name`: 창 노드 이름. (어둠, 틀, 판 노드는 이 이름 뒤에 Dim, Frame, Panel 을 붙인 이름)
	/// - `behavior`: 덮어쓸 자리.
	///
	/// # 돌려줌
	/// 만든 창 노드. (띄우기 전이라 꺼져 있다)
	///
	/// # 예제
	/// 만든 창은 띄울 때까지 꺼져 있다
	/// ```rust
	/// use vanilla::{NodeTree, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// let window_id = UIWindow::create(&mut node_tree, layer_id, "help", Box::new(UIWindowDefaultBehavior));
	/// assert!(!node_tree[window_id].is_active());
	/// let panel_id = node_tree[window_id].get_component::<UIWindow>().expect("window").get_panel_node().expect("panel");
	/// assert_eq!(node_tree[panel_id].get_name(), "helpPanel");
	/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
	/// assert!(node_tree[window_id].is_active());
	/// ```
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, layer_id: NodeId, name: &str, behavior: Box<dyn UIWindowBehavior>) -> NodeId {
		let window_id = node_tree.create_child(layer_id, name);
		node_tree.add_component(window_id, UIWindow::create_with_behavior(behavior));
		// 원본 창은 띄울 때 층에 붙는다. 러스트 판은 층 밑에 미리 만들어 두므로 띄우기 전에는 끈다.
		node_tree[window_id].set_active(false);
		window_id
	}

	//==============================================================================
	/// 창 닫기. (창 노드의 창 층을 찾아 close_window, 씬 코드와 단추 콜백에서 부른다, 떠 있지 않으면 아무 일도 없다)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리. (단추 콜백에서는 context.get_node_tree_mut())
	/// - `window_id`: 닫을 창 노드.
	///
	/// # 예제
	/// 판 안의 닫기 단추
	/// ```rust
	/// use vanilla::{ComponentContext, NodeId, UIButton, UIWindow};
	///
	/// fn bind_close_button(button: &mut UIButton, window_id: NodeId) {
	/// 	button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
	/// 		UIWindow::close_in_tree(context.get_node_tree_mut(), window_id);
	/// 	}));
	/// }
	/// # let mut button = UIButton::new();
	/// # bind_close_button(&mut button, vanilla::NodeTree::new().get_root_id());
	/// ```
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
	/// 창 다시 놓기. (내용이 바뀌어 판의 세로가 달라질 때, 씬 코드에서)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `window_id`: 다시 놓을 창 노드. (UIWindow 가 없으면 아무 일도 없다)
	///
	/// # 예제
	/// 판 세로를 바꾸고 다시 놓기
	/// ```rust
	/// use vanilla::{NodeTree, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
	/// 	window_layer.set_view_size(tree, 800.0, 600.0, 1.0);
	/// });
	/// let window_id = UIWindow::create(&mut node_tree, layer_id, "list", Box::new(UIWindowDefaultBehavior));
	/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
	/// node_tree[window_id].get_component_mut::<UIWindow>().expect("window").set_panel_height(400.0);
	/// UIWindow::refresh_layout_in_tree(&mut node_tree, window_id);
	/// assert_eq!(node_tree[window_id].get_component::<UIWindow>().expect("window").get_panel_rect().size.y, 400.0);
	/// ```
	//==============================================================================
	pub fn refresh_layout_in_tree(node_tree: &mut NodeTree, window_id: NodeId) {
		node_tree.with_component_mut::<UIWindow, _>(window_id, |window, tree| {
			window.refresh_layout(tree);
		});
	}

	//==============================================================================
	/// 덮어쓸 자리 꺼내기. (구체 타입이 맞으면)
	///
	/// # 돌려줌
	/// 덮어쓸 자리가 T 면 `Some(&T)`, 다른 타입이면 None.
	//==============================================================================
	pub fn get_behavior<T: UIWindowBehavior>(&self) -> Option<&T> {
		self.behavior.as_ref().and_then(|behavior| behavior.as_any().downcast_ref::<T>())
	}

	//==============================================================================
	/// 덮어쓸 자리 꺼내기. (변경용)
	///
	/// # 돌려줌
	/// 덮어쓸 자리가 T 면 `Some(&mut T)`, 다른 타입이면 None.
	///
	/// # 예제
	/// 확인 창에 확인 알림 걸기
	/// ```rust
	/// use vanilla::{NodeTree, UIConfirmWindow, UIConfirmWindowOptions, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// let window_id = UIConfirmWindow::create(&mut node_tree, layer_id, "quit", UIConfirmWindowOptions::default());
	/// let window = node_tree[window_id].get_component_mut::<UIWindow>().expect("window");
	/// if let Some(confirm_window) = window.get_behavior_mut::<UIConfirmWindow>() {
	/// 	confirm_window.set_confirm_event(Some(Box::new(|| {})));
	/// }
	/// assert!(window.get_behavior_mut::<UIWindowDefaultBehavior>().is_none());
	/// ```
	//==============================================================================
	pub fn get_behavior_mut<T: UIWindowBehavior>(&mut self) -> Option<&mut T> {
		self.behavior.as_mut().and_then(|behavior| behavior.as_any_mut().downcast_mut::<T>())
	}

	//==============================================================================
	/// 창의 노드와 지금 놓인 값. (만들기 전이면 None)
	///
	/// # 돌려줌
	/// 노드와 놓인 값. (트리에 붙기 전이면 None)
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
	/// 판 노드. (내용을 여기에 붙인다)
	///
	/// # 돌려줌
	/// 판 노드. (트리에 붙기 전이면 None)
	//==============================================================================
	pub fn get_panel_node(&self) -> Option<NodeId> {
		self.panel_id
	}

	//==============================================================================
	/// 틀 노드. (판과 함께 커지고 주는 꾸밈을 붙인다)
	///
	/// # 돌려줌
	/// 틀 노드. (트리에 붙기 전이면 None)
	//==============================================================================
	pub fn get_frame_node(&self) -> Option<NodeId> {
		self.frame_id
	}

	//==============================================================================
	/// 어둠 노드.
	///
	/// # 돌려줌
	/// 어둠 노드. (트리에 붙기 전이면 None)
	//==============================================================================
	pub fn get_dim_node(&self) -> Option<NodeId> {
		self.dim_id
	}

	//==============================================================================
	/// 판의 Paint. (기본 판 그림, 바꾸거나 끌 때. 붙기 전이면 None)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	///
	/// # 돌려줌
	/// 판 노드의 Paint. (붙기 전이거나 판 노드나 Paint 가 없으면 None)
	//==============================================================================
	pub fn get_panel_paint<'a>(&self, node_tree: &'a NodeTree) -> Option<&'a Paint> {
		self.panel_id.and_then(|panel_id| node_tree.get_node(panel_id)).and_then(|panel_node| panel_node.get_component::<Paint>())
	}

	//==============================================================================
	/// 판의 Paint. (변경용)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	///
	/// # 돌려줌
	/// 판 노드의 Paint. (붙기 전이거나 판 노드나 Paint 가 없으면 None)
	///
	/// # 예제
	/// 붙은 뒤 판 색 바꾸기
	/// ```rust
	/// use vanilla::{Color, NodeTree, Paint, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// let window_id = UIWindow::create(&mut node_tree, layer_id, "shop", Box::new(UIWindowDefaultBehavior));
	/// let panel_id = node_tree.with_component_mut::<UIWindow, _>(window_id, |window, tree| {
	/// 	if let Some(panel_paint) = window.get_panel_paint_mut(tree) {
	/// 		panel_paint.set_color(Color::create(0.2, 0.1, 0.1, 1.0));
	/// 	}
	/// 	window.get_panel_node()
	/// }).flatten().expect("panel");
	/// assert_eq!(node_tree[panel_id].get_component::<Paint>().expect("paint").get_color(), Color::create(0.2, 0.1, 0.1, 1.0));
	/// ```
	//==============================================================================
	pub fn get_panel_paint_mut<'a>(&self, node_tree: &'a mut NodeTree) -> Option<&'a mut Paint> {
		self.panel_id.and_then(move |panel_id| node_tree.get_node_mut(panel_id)).and_then(|panel_node| panel_node.get_component_mut::<Paint>())
	}

	//==============================================================================
	/// 어둠의 Paint. (붙기 전이면 None)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	///
	/// # 돌려줌
	/// 어둠 노드의 Paint. (붙기 전이거나 어둠 노드나 Paint 가 없으면 None)
	//==============================================================================
	pub fn get_dim_paint<'a>(&self, node_tree: &'a NodeTree) -> Option<&'a Paint> {
		self.dim_id.and_then(|dim_id| node_tree.get_node(dim_id)).and_then(|dim_node| dim_node.get_component::<Paint>())
	}

	//==============================================================================
	/// 어둠의 Paint. (변경용)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	///
	/// # 돌려줌
	/// 어둠 노드의 Paint. (붙기 전이거나 어둠 노드나 Paint 가 없으면 None)
	//==============================================================================
	pub fn get_dim_paint_mut<'a>(&self, node_tree: &'a mut NodeTree) -> Option<&'a mut Paint> {
		self.dim_id.and_then(move |dim_id| node_tree.get_node_mut(dim_id)).and_then(|dim_node| dim_node.get_component_mut::<Paint>())
	}

	//==============================================================================
	/// 마지막으로 받은 영역의 가로. (UI 층 단위)
	///
	/// # 돌려줌
	/// 영역의 가로. (1 이상, 받기 전에는 1)
	//==============================================================================
	pub fn get_view_width(&self) -> f64 {
		self.view_width
	}

	//==============================================================================
	/// 마지막으로 받은 영역의 세로. (UI 층 단위)
	///
	/// # 돌려줌
	/// 영역의 세로. (1 이상, 받기 전에는 1)
	//==============================================================================
	pub fn get_view_height(&self) -> f64 {
		self.view_height
	}

	//==============================================================================
	/// 영역 가장자리와 판 사이 거리 반환.
	///
	/// # 돌려줌
	/// 거리. (UI 층 단위, 기본 16)
	//==============================================================================
	pub fn get_margin(&self) -> f64 {
		self.margin
	}

	//==============================================================================
	/// 판의 세로 반환. (set_panel_height 의 값)
	///
	/// # 돌려줌
	/// 판의 세로. (UI 층 단위, 기본 240)
	//==============================================================================
	pub fn get_panel_height(&self) -> f64 {
		self.panel_height
	}

	//==============================================================================
	/// 마지막으로 놓은 판의 자리. (창 좌표, 움직임 전, 줄인 배율을 곱한 크기)
	///
	/// # 돌려줌
	/// 판의 사각형. (놓기 전에는 (0, 0, 1, 1))
	//==============================================================================
	pub fn get_panel_rect(&self) -> Rect {
		self.panel_rect
	}

	//==============================================================================
	/// 판이 영역보다 커서 줄인 배율. (1 이하)
	///
	/// # 돌려줌
	/// 배율. (0 보다 크고 1 이하, 줄이지 않았으면 1)
	//==============================================================================
	pub fn get_panel_scale(&self) -> f64 {
		self.panel_scale
	}

	//==============================================================================
	/// 휠 받기. (UIWindowBehavior::handle_wheel, 받았으면 true, 원본 handleWheel)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `view_input_position`: 휠을 굴린 자리. (뷰 좌표)
	/// - `wheel_delta`: 휠을 굴린 양.
	///
	/// # 돌려줌
	/// 덮어쓸 자리가 받았으면 true. (트리에 붙기 전이면 false)
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
	/// 움직임 반환.
	///
	/// # 돌려줌
	/// 창의 열고 닫는 움직임.
	//==============================================================================
	pub fn get_popup_motion(&self) -> &PopupMotion {
		&self.popup_motion
	}

	//==============================================================================
	/// 움직임 반환. (변경용, set_instant_enabled 로 움직임 없이 열고 닫는다)
	///
	/// # 돌려줌
	/// 창의 열고 닫는 움직임.
	//==============================================================================
	pub fn get_popup_motion_mut(&mut self) -> &mut PopupMotion {
		&mut self.popup_motion
	}

	//==============================================================================
	/// 바깥 누르기(와 창 층의 close_top_window)로 닫히는지 설정. (기본 true)
	///
	/// # 인자
	/// - `is_outside_close_enabled`: 바깥 누르기와 뒤로 가기로 닫히면 true.
	//==============================================================================
	pub fn set_outside_close_enabled(&mut self, is_outside_close_enabled: bool) {
		self.is_outside_close_enabled = is_outside_close_enabled;
	}

	//==============================================================================
	/// 바깥 누르기로 닫히는지.
	///
	/// # 돌려줌
	/// 닫히면 true. (기본 true)
	//==============================================================================
	pub fn is_outside_close_enabled(&self) -> bool {
		self.is_outside_close_enabled
	}

	//==============================================================================
	/// 창 층에서 내려갈 때 알림 설정.
	///
	/// # 인자
	/// - `close_event`: 알림. (None 이면 끈다, UIWindowBehavior::handle_closed 보다 먼저 부른다)
	//==============================================================================
	pub fn set_close_event(&mut self, close_event: Option<UIWindowEvent>) {
		self.close_event = close_event;
	}

	//==============================================================================
	/// 창의 가로 한계 설정. (영역이 좁으면 그보다 좁아진다, 다음 놓을 때부터)
	///
	/// # 인자
	/// - `maximum_width`: 판의 가로 한계. (UI 층 단위, 1 보다 작으면 1)
	//==============================================================================
	pub fn set_maximum_width(&mut self, maximum_width: f64) {
		self.maximum_width = maximum_width.max(1.0);
	}

	//==============================================================================
	/// 창의 가로 한계 반환.
	///
	/// # 돌려줌
	/// 판의 가로 한계. (UI 층 단위, 기본 480)
	//==============================================================================
	pub fn get_maximum_width(&self) -> f64 {
		self.maximum_width
	}

	//==============================================================================
	/// 영역 가장자리와 판 사이 거리 설정.
	///
	/// # 인자
	/// - `margin`: 거리. (UI 층 단위, 0 보다 작으면 0, 다음 놓을 때부터)
	//==============================================================================
	pub fn set_margin(&mut self, margin: f64) {
		self.margin = margin.max(0.0);
	}

	//==============================================================================
	/// 판의 세로 설정. (layout_content 를 덮어쓰지 않은 창이 쓴다)
	///
	/// # 인자
	/// - `panel_height`: 판의 세로. (UI 층 단위, 1 보다 작으면 1, 다음 놓을 때부터)
	//==============================================================================
	pub fn set_panel_height(&mut self, panel_height: f64) {
		self.panel_height = panel_height.max(1.0);
	}

	//==============================================================================
	/// 어둠 색 설정. (값만 담는다, 붙기 전에 정한다. 붙은 뒤에 바꾸려면 어둠 노드의 Paint 색을 바꾼다)
	///
	/// # 인자
	/// - `dim_color`: 어둠 색. (기본 `Color::create(0.0, 0.0, 0.0, 0.55)`, 트리에 붙을 때 어둠 Paint 에 쓴다)
	//==============================================================================
	pub fn set_dim_color(&mut self, dim_color: impl Into<Color>) {
		self.dim_color = dim_color.into();
	}

	//==============================================================================
	/// 판 색 설정. (값만 담는다, 붙기 전에 정한다. 붙은 뒤에 바꾸려면 판 노드의 Paint 색을 바꾼다)
	///
	/// # 인자
	/// - `panel_color`: 판 색. (기본 `Color::create(0.13, 0.15, 0.2, 1.0)`, 트리에 붙을 때 판 Paint 에 쓴다)
	//==============================================================================
	pub fn set_panel_color(&mut self, panel_color: impl Into<Color>) {
		self.panel_color = panel_color.into();
	}

	//==============================================================================
	/// 띄운 창 층 노드. (띄우기 전이나 내려간 뒤면 None)
	///
	/// # 돌려줌
	/// 창 층 노드. (떠 있을 때만 Some)
	//==============================================================================
	pub fn get_window_layer_id(&self) -> Option<NodeId> {
		self.window_layer_id
	}

	//==============================================================================
	/// 띄운 창 층 노드 설정. (창 층이 부른다)
	///
	/// # 인자
	/// - `window_layer_id`: 띄운 창 층 노드. (내려가면 None)
	//==============================================================================
	pub fn set_window_layer_id(&mut self, window_layer_id: Option<NodeId>) {
		self.window_layer_id = window_layer_id;
	}

	//==============================================================================
	/// 영역 밖으로 더 덮는 너비 반환. (위, 오른쪽, 아래, 왼쪽)
	///
	/// # 돌려줌
	/// `[위, 오른쪽, 아래, 왼쪽]` 너비. (UI 층 단위, 처음은 모두 0)
	//==============================================================================
	pub fn get_view_bleed(&self) -> [f64; 4] {
		self.view_bleed
	}

	//==============================================================================
	/// 영역 밖으로 더 덮는 너비 알리기. (세이프에어리어, UI 층 단위, 어둠이 그 밑까지 덮는다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `top`: 위로 더 덮는 너비. (0 보다 작으면 0)
	/// - `right`: 오른쪽으로 더 덮는 너비. (0 보다 작으면 0)
	/// - `bottom`: 아래로 더 덮는 너비. (0 보다 작으면 0)
	/// - `left`: 왼쪽으로 더 덮는 너비. (0 보다 작으면 0)
	//==============================================================================
	pub fn set_view_bleed(&mut self, node_tree: &mut NodeTree, top: f64, right: f64, bottom: f64, left: f64) {
		self.view_bleed = [top.max(0.0), right.max(0.0), bottom.max(0.0), left.max(0.0)];
		self.place_dim_node(node_tree);
	}

	//==============================================================================
	/// 어둠 놓기. (영역과 그 밖의 세이프에어리어까지)
	//==============================================================================
	fn place_dim_node(&self, node_tree: &mut NodeTree) {
		let [bleed_top, bleed_right, bleed_bottom, bleed_left] = self.view_bleed;
		if let Some(dim_node) = self.dim_id.and_then(|dim_id| node_tree.get_node_mut(dim_id)) {
			dim_node.set_local_position(Vector2::create(-bleed_left, -bleed_top));
			dim_node.set_content_size(Vector2::create(self.view_width + bleed_left + bleed_right, self.view_height + bleed_top + bleed_bottom));
		}
	}

	//==============================================================================
	/// 영역의 크기 알리기. (어둠을 덮고, 판의 가로를 정해 내용을 놓은 뒤 가운데에 둔다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `view_width`: 영역의 가로. (UI 층 단위, 1 보다 작으면 1)
	/// - `view_height`: 영역의 세로. (UI 층 단위, 1 보다 작으면 1)
	//==============================================================================
	pub fn set_view_size(&mut self, node_tree: &mut NodeTree, view_width: f64, view_height: f64) {
		self.view_width = view_width.max(1.0);
		self.view_height = view_height.max(1.0);
		self.refresh_layout(node_tree);
	}

	//==============================================================================
	/// 다시 놓기. (마지막 영역의 크기로)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리. (트리에 붙기 전이면 아무 일도 없다)
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
	/// 판 놓기 기본. (판을 틀의 왼쪽 위에 그 크기로)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `parts`: 창의 노드와 지금 놓인 값.
	/// - `panel_width`: 판의 가로. (줄이기 전, UI 층 단위)
	/// - `panel_height`: 판의 세로. (줄이기 전, UI 층 단위)
	///
	/// # 예제
	/// 기본 판 놓기 뒤에 틀의 그림자 꾸밈 놓기
	/// ```rust
	/// use std::any::Any;
	/// use vanilla::{NodeId, NodeTree, UIWindow, UIWindowBehavior, UIWindowParts, Vector2};
	///
	/// struct ShadowWindow {
	/// 	shadow_id: Option<NodeId>,
	/// }
	///
	/// impl UIWindowBehavior for ShadowWindow {
	/// 	fn as_any(&self) -> &dyn Any {
	/// 		self
	/// 	}
	///
	/// 	fn as_any_mut(&mut self) -> &mut dyn Any {
	/// 		self
	/// 	}
	///
	/// 	fn build(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts) {
	/// 		let shadow_id = node_tree.create_child(parts.frame_id, "Shadow");
	/// 		node_tree.set_sibling_index(shadow_id, 0);
	/// 		self.shadow_id = Some(shadow_id);
	/// 	}
	///
	/// 	fn layout_panel(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, panel_width: f64, panel_height: f64) {
	/// 		UIWindow::layout_panel_default(node_tree, parts, panel_width, panel_height);
	/// 		if let Some(shadow_node) = self.shadow_id.and_then(|shadow_id| node_tree.get_node_mut(shadow_id)) {
	/// 			shadow_node.set_content_size(Vector2::create(panel_width + 8.0, panel_height + 8.0));
	/// 		}
	/// 	}
	/// }
	/// # let _ = ShadowWindow { shadow_id: None };
	/// ```
	//==============================================================================
	pub fn layout_panel_default(node_tree: &mut NodeTree, parts: &UIWindowParts, panel_width: f64, panel_height: f64) {
		if let Some(panel_node) = node_tree.get_node_mut(parts.panel_id) {
			panel_node.set_local_position(Vector2::zero());
			panel_node.set_content_size(Vector2::create(panel_width, panel_height));
		}
	}

	//==============================================================================
	/// 움직임 걸기 기본. (틀 배율 = 줄인 배율 x 움직임 배율, 어둠 진하기 = 움직임 비율)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `parts`: 창의 노드와 지금 놓인 값. (panel_scale 을 쓴다)
	/// - `popup_motion`: 창의 움직임. (get_scale, get_dim_ratio 를 쓴다)
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
	/// 덮기 기본. (덮이면 창 노드를 끄고 드러나면 켠다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `parts`: 창의 노드와 지금 놓인 값.
	/// - `is_covered`: 위에 다른 창이 떠 있으면 true.
	//==============================================================================
	pub fn set_covered_default(node_tree: &mut NodeTree, parts: &UIWindowParts, is_covered: bool) {
		if let Some(window_node) = node_tree.get_node_mut(parts.window_id) {
			window_node.set_active(!is_covered);
		}
	}

	//==============================================================================
	/// 덮였는지 알리기. (창 층이 부른다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	/// - `is_covered`: 위에 다른 창이 떠 있으면 true. (UIWindowBehavior::set_covered 에 넘긴다)
	//==============================================================================
	pub fn set_covered(&mut self, node_tree: &mut NodeTree, is_covered: bool) {
		if let (Some(parts), Some(behavior)) = (self.get_parts(), self.behavior.as_mut()) {
			behavior.set_covered(node_tree, &parts, is_covered);
		}
	}

	//==============================================================================
	/// 노드에 움직임 걸기.
	//==============================================================================
	fn apply_motion(&mut self, node_tree: &mut NodeTree) {
		if let (Some(parts), Some(behavior)) = (self.get_parts(), self.behavior.as_mut()) {
			behavior.apply_motion(node_tree, &parts, &self.popup_motion);
		}
	}

	//==============================================================================
	/// 어둠과 판의 누름 켜고 끄기.
	//==============================================================================
	fn set_parts_interactable(&self, node_tree: &mut NodeTree, is_interactable: bool) {
		for part_id in [self.dim_id, self.panel_id].into_iter().flatten() {
			if let Some(part_node) = node_tree.get_node_mut(part_id) {
				part_node.set_interactable(is_interactable);
			}
		}
	}

	//==============================================================================
	/// 여는 움직임 시작. (창 층이 띄울 때 부른다, 창 노드를 켠다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
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
	/// 닫는 움직임 시작. (창 층이 내릴 때 부른다, 그동안은 누름을 받지 않는다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리.
	//==============================================================================
	pub fn start_close_motion(&mut self, node_tree: &mut NodeTree) {
		self.set_parts_interactable(node_tree, false);
		self.popup_motion.close();
		self.apply_motion(node_tree);
	}

	//==============================================================================
	/// 닫는 움직임이 끝났는지.
	///
	/// # 돌려줌
	/// 움직임이 닫힘 상태면 true. (띄운 적이 없어도 true, 즉시 전환이면 닫자마자 true)
	//==============================================================================
	pub fn is_close_motion_finished(&self) -> bool {
		!self.popup_motion.is_visible()
	}

	//==============================================================================
	/// 창 층에서 내려감. (창 층이 부른다: 알림 → UIWindowBehavior::handle_closed)
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
	/// 창 층이 창을 떼어 낼 때. (창 층이 부른다, 노드는 지우지 않고 남아 다시 띄울 수 있다)
	///
	/// # 인자
	/// - `node_tree`: 창이 든 노드 트리. (UIWindowBehavior::dispose 에 넘긴다)
	//==============================================================================
	pub fn dispose(&mut self, node_tree: &mut NodeTree) {
		if let (Some(parts), Some(behavior)) = (self.get_parts(), self.behavior.as_mut()) {
			behavior.dispose(node_tree, &parts);
		}
	}
}

#[crate::reflect]
impl Component for UIWindow {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 타입 이름. ("UIWindow")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIWindow"
	}

	//==============================================================================
	/// 트리에 붙음. (원본 생성자: 창 꾸미기(왼쪽 위 피봇), 어둠, 틀, 판, 그다음 UIWindowBehavior::build)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.node_id = Some(node_id);
		let name = node_tree[node_id].get_name().to_string();
		let window_node = &mut node_tree[node_id];
		window_node.set_pivot(Pivot::TOP_LEFT);
		window_node.set_anchor(Pivot::TOP_LEFT);

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
	/// 갱신. (움직임 → 노드에 반영, 그린 뒤 알게 된 크기를 기다리던 창은 다시 놓는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f64) {
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
	/// 휠. (그 자리의 스크롤 뷰가 받지 않은 휠, 창이 받고 늘 삼킨다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
		let view_input_position = context.get_engine().get_input_manager().get_view_input_position();
		self.handle_wheel(context.get_node_tree_mut(), view_input_position, wheel_delta);
		true
	}
}
