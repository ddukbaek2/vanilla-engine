//==============================================================================
//! 창 층. (vanilla.js 의 ui/uiwindowlayer.js, UIWindowLayer)
//! - 영역 위에 UIWindow 를 쌓는다. 창을 띄우면 맨 위에 쌓고, 맨 위의 창만 보인다. 위의 창이 닫히면 아래 창이 다시 보인다.
//! - 닫으면 목록에서 바로 빠져 누름과 덮임이 바로 풀리고, 닫는 움직임(PopupMotion)이 끝난 뒤에 창 노드를 층에서 떼어 낸다. 덮여 있던 창을 닫으면 바로 떼어 낸다.
//!   떼어 낸 창 노드는 지우지 않아 open_window 로 다시 띄울 수 있다. (원본과 같음) 다시 쓰지 않을 창은 씬이 destroy_node 로 지운다.
//! - 휠은 맨 위의 창이 먼저 받고(UIWindowBehavior::handle_wheel), 받지 않으면 그 자리의 스크롤 뷰가 받는다. (scroll_window_at, GameSceneBase 가 창 층 안을 가리킨 휠을 넘긴다)
//! - 이 층은 UI 배율(UIScale::read)만큼 커지고, 창에는 그만큼 나눈 영역의 크기와 세이프에어리어 너비를 알린다. (작은 화면에서도 글자가 읽히게)
//!   set_ui_scale_options(None) 이면 키우지 않는다.
//! - 창이 하나라도 떠 있는 동안은 놀이 밖이다. 창이 뜨고 닫힐 때 알린다(set_window_change_event, 떠 있는 창 수를 넘긴다). 씬이 has_open_window 를 보고 놀이를 멈춰도 된다.
//! - 사용: (영역 노드의 맨 위 자식으로 두고, 영역 크기가 바뀔 때마다 알린다)
//!   ```text
//!     let layer_id = UIWindowLayer::create(node_tree, play_area_id);
//!     node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |layer, tree| {
//!         layer.set_view_size(tree, width, height, view_manager.get_css_pixel_scale());
//!         layer.set_view_bleed(tree, top, right, bottom, left);
//!     });
//!     let window_id = UIConfirmWindow::create(node_tree, layer_id, "quit", UIConfirmWindowOptions { title: ..., ..UIConfirmWindowOptions::default() });
//!     UIWindowLayer::open_window_in_tree(node_tree, layer_id, window_id);
//!   ```
//! - 원본과 다른 것:
//!   - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 층 노드에 붙는 컴포넌트이고 창은 그 노드의 자식 노드다. 바꾸는 함수는 트리를 받는다.
//!   - 원본은 떼어 낸 창 객체를 들고 있으면 다시 띄운다. 러스트 판은 떼어 낸 노드가 트리 저장소에 남아 있으므로 그 핸들로 다시 띄운다.
//!     원본에서 아무도 들지 않은 창은 가비지 컬렉터가 치우지만, 러스트 판에서는 씬이 지우거나 씬이 끝날 때 함께 지워진다.
//!   - 원본은 층이 UIScale 에 엔진의 CSS 배율을 묻는다. 러스트 판은 set_view_size 가 CSS 배율(ViewManager::get_css_pixel_scale)을 받는다.
//!   - 창 알림은 `Box<dyn FnMut(usize)>` 다. (떠 있는 창 수)
//==============================================================================
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::engine::Engine;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uiscale::{UIScale, UIScaleOptions};
use crate::ui::uiview::is_view_position_inside_node;
use crate::ui::uiwindow::UIWindow;

//==============================================================================
/// 창이 뜨거나 닫힐 때 알림. (떠 있는 창 수)
//==============================================================================
pub type UIWindowChangeEvent = Box<dyn FnMut(usize)>;

//==============================================================================
/// 창 층.
///
/// # 예제
/// 영역 노드 밑에 층을 두고 확인 창 띄우기
/// ```rust
/// use vanilla::{NodeTree, UIConfirmWindow, UIConfirmWindowOptions, UIWindowLayer, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let play_area_id = node_tree.create_child(root_id, "PlayArea");
/// node_tree[play_area_id].set_content_size(Vector2::create(800.0, 600.0));
/// let layer_id = UIWindowLayer::create(&mut node_tree, play_area_id);
/// node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
/// 	window_layer.set_view_size(tree, 800.0, 600.0, 1.0);
/// 	window_layer.set_view_bleed(tree, 40.0, 0.0, 30.0, 0.0);
/// });
/// let options = UIConfirmWindowOptions {
/// 	title: String::from("Quit"),
/// 	message: String::from("Leave the game?"),
/// 	..UIConfirmWindowOptions::default()
/// };
/// let window_id = UIConfirmWindow::create(&mut node_tree, layer_id, "quit", options);
/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
/// let window_layer = node_tree[layer_id].get_component::<UIWindowLayer>().expect("layer");
/// assert!(window_layer.has_open_window());
/// assert_eq!(window_layer.get_top_window(), Some(window_id));
/// assert_eq!(window_layer.find_window(&node_tree, "quit"), Some(window_id));
/// ```
/// <!-- image: window-stack.webp -->
//==============================================================================
pub struct UIWindowLayer {
	component_base: ComponentBase,
	node_id: Option<NodeId>,
	/// 띄운 창. (아래에서 위로)
	window_ids: Vec<NodeId>,
	/// 닫는 움직임 중인 창. (끝나면 지움)
	closing_window_ids: Vec<NodeId>,
	/// 영역의 크기. (부모 단위)
	view_width: f32,
	view_height: f32,
	/// 영역 밖으로 더 덮는 너비. (위, 오른쪽, 아래, 왼쪽, 부모 단위)
	view_bleed: [f32; 4],
	/// 마지막으로 받은 뷰 한 단위의 CSS 픽셀.
	css_pixel_scale: f32,
	ui_scale_options: Option<UIScaleOptions>,
	ui_scale: f32,
	window_change_event: Option<UIWindowChangeEvent>,
}

#[crate::reflect]
impl UIWindowLayer {
	//==============================================================================
	/// 생성. (노드에는 node_tree.add_component 로 붙인다, 보통은 UIWindowLayer::create)
	///
	/// # 돌려줌
	/// 붙기 전의 층. (띄운 창 없음, UI 배율 설정은 UIScaleOptions::default)
	//==============================================================================
	pub fn new() -> UIWindowLayer {
		UIWindowLayer {
			component_base: ComponentBase::new(),
			node_id: None,
			window_ids: Vec::new(),
			closing_window_ids: Vec::new(),
			view_width: 1.0,
			view_height: 1.0,
			view_bleed: [0.0; 4],
			css_pixel_scale: 1.0,
			ui_scale_options: Some(UIScaleOptions::default()),
			ui_scale: 1.0,
			window_change_event: None,
		}
	}

	//==============================================================================
	/// 층 노드 만들기. (parent_id 밑에 "UIWindowLayer" 노드를 만들어 붙인다, 원본 new UIWindowLayer() + add_child)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `parent_id`: 영역 노드. (층은 그 맨 위 자식으로 두고 쓴다)
	///
	/// # 돌려줌
	/// 만든 층 노드.
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, parent_id: NodeId) -> NodeId {
		let layer_id = node_tree.create_child(parent_id, "UIWindowLayer");
		node_tree.add_component(layer_id, UIWindowLayer::new());
		layer_id
	}

	//==============================================================================
	/// 창 띄우기. (씬 코드용, with_component_mut 을 대신 부른다)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `layer_id`: 창 층 노드. (UIWindowLayer 가 없으면 아무 일도 없다)
	/// - `window_id`: 띄울 창 노드. (UIWindow::create 나 UIConfirmWindow::create 가 돌려준 것)
	//==============================================================================
	pub fn open_window_in_tree(node_tree: &mut NodeTree, layer_id: NodeId, window_id: NodeId) {
		node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
			window_layer.open_window(tree, window_id);
		});
	}

	//==============================================================================
	/// 창이 뜨거나 닫힐 때 알림 설정.
	///
	/// # 인자
	/// - `window_change_event`: 알림. (떠 있는 창 수를 받는다, None 이면 끈다)
	///
	/// # 예제
	/// 창이 떠 있는 동안 놀이 멈추기
	/// ```rust
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	/// use vanilla::{NodeTree, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// let is_paused = Rc::new(Cell::new(false));
	/// let is_paused_event = is_paused.clone();
	/// node_tree[layer_id].get_component_mut::<UIWindowLayer>().expect("layer").set_window_change_event(Some(Box::new(move |window_count| {
	/// 	is_paused_event.set(window_count > 0);
	/// })));
	/// let window_id = UIWindow::create(&mut node_tree, layer_id, "pause", Box::new(UIWindowDefaultBehavior));
	/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
	/// assert!(is_paused.get());
	/// UIWindow::close_in_tree(&mut node_tree, window_id);
	/// assert!(!is_paused.get());
	/// ```
	//==============================================================================
	pub fn set_window_change_event(&mut self, window_change_event: Option<UIWindowChangeEvent>) {
		self.window_change_event = window_change_event;
	}

	//==============================================================================
	/// UI 배율 설정. (None 이면 키우지 않는다, 기본은 UIScaleOptions::default)
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리. (마지막 영역의 크기로 다시 놓는다)
	/// - `ui_scale_options`: UI 배율 설정. (None 이면 배율 1)
	//==============================================================================
	pub fn set_ui_scale_options(&mut self, node_tree: &mut NodeTree, ui_scale_options: Option<UIScaleOptions>) {
		self.ui_scale_options = ui_scale_options;
		let (view_width, view_height, css_pixel_scale) = (self.view_width, self.view_height, self.css_pixel_scale);
		self.set_view_size(node_tree, view_width, view_height, css_pixel_scale);
	}

	//==============================================================================
	/// 지금의 UI 배율.
	///
	/// # 돌려줌
	/// 층을 키운 배율. (마지막 set_view_size 에서 읽은 값, 처음은 1)
	//==============================================================================
	pub fn get_ui_scale(&self) -> f32 {
		self.ui_scale
	}

	//==============================================================================
	/// 창이 하나라도 떠 있는지. (닫는 움직임 중인 창은 셈하지 않는다)
	///
	/// # 돌려줌
	/// 떠 있는 창이 있으면 true.
	//==============================================================================
	pub fn has_open_window(&self) -> bool {
		!self.window_ids.is_empty()
	}

	//==============================================================================
	/// 떠 있는 창의 수.
	///
	/// # 돌려줌
	/// 창 수. (닫는 움직임 중인 창은 빼고)
	//==============================================================================
	pub fn get_window_count(&self) -> usize {
		self.window_ids.len()
	}

	//==============================================================================
	/// 맨 위의 창. (없으면 None)
	///
	/// # 돌려줌
	/// 맨 위 창 노드. (떠 있는 창이 없으면 None)
	//==============================================================================
	pub fn get_top_window(&self) -> Option<NodeId> {
		self.window_ids.last().copied()
	}

	//==============================================================================
	/// 그 이름의 창. (떠 있는 것 가운데 위에서부터, 없으면 None)
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	/// - `window_name`: 찾을 창 노드 이름. (UIWindow::create 의 name)
	///
	/// # 돌려줌
	/// 그 이름의 창 노드 가운데 가장 위의 것. (떠 있는 창에 없으면 None)
	//==============================================================================
	pub fn find_window(&self, node_tree: &NodeTree, window_name: &str) -> Option<NodeId> {
		self.window_ids.iter().rev().copied().find(|window_id| node_tree.get_node(*window_id).map(|window_node| window_node.get_name() == window_name).unwrap_or(false))
	}

	//==============================================================================
	/// 영역의 크기 알리기. (UI 배율을 다시 읽고, 띄운 창을 모두 다시 놓는다, css_pixel_scale 은 ViewManager::get_css_pixel_scale)
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	/// - `view_width`: 영역의 가로. (부모 단위, 1 보다 작으면 1)
	/// - `view_height`: 영역의 세로. (부모 단위, 1 보다 작으면 1)
	/// - `css_pixel_scale`: 뷰 한 단위의 CSS 픽셀. (UIScale::read 가 배율을 정할 때 쓴다)
	///
	/// # 예제
	/// 작은 화면에서는 층이 커지고 창에는 나눈 크기를 알린다
	/// ```rust
	/// use vanilla::{NodeTree, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
	/// 	window_layer.set_view_size(tree, 800.0, 600.0, 0.4);
	/// });
	/// let ui_scale = node_tree[layer_id].get_component::<UIWindowLayer>().expect("layer").get_ui_scale();
	/// assert!((ui_scale - 1.8).abs() < 0.0001);
	/// assert!((node_tree[layer_id].get_content_size().x - 800.0 / 1.8).abs() < 0.001);
	/// node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
	/// 	window_layer.set_ui_scale_options(tree, None);
	/// });
	/// assert_eq!(node_tree[layer_id].get_component::<UIWindowLayer>().expect("layer").get_ui_scale(), 1.0);
	/// ```
	//==============================================================================
	pub fn set_view_size(&mut self, node_tree: &mut NodeTree, view_width: f32, view_height: f32, css_pixel_scale: f32) {
		self.view_width = view_width.max(1.0);
		self.view_height = view_height.max(1.0);
		self.css_pixel_scale = css_pixel_scale;
		self.ui_scale = match self.ui_scale_options.as_ref() {
			Some(ui_scale_options) => {
				UIScale::read(self.view_width, self.view_height, css_pixel_scale, ui_scale_options)
			}
			None => {
				1.0
			}
		};
		if let Some(layer_node) = self.node_id.and_then(|layer_id| node_tree.get_node_mut(layer_id)) {
			layer_node.set_local_scale(Vector2::create(self.ui_scale, self.ui_scale));
			layer_node.set_content_size(Vector2::create(self.view_width / self.ui_scale, self.view_height / self.ui_scale));
		}
		self.place_all_windows(node_tree);
	}

	//==============================================================================
	/// 영역 밖으로 더 덮는 너비 알리기. (세이프에어리어, 부모 단위)
	/// - 창에는 UI 배율로 나눈 너비를 알린다.
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	/// - `top`: 위로 더 덮는 너비. (0 보다 작으면 0)
	/// - `right`: 오른쪽으로 더 덮는 너비. (0 보다 작으면 0)
	/// - `bottom`: 아래로 더 덮는 너비. (0 보다 작으면 0)
	/// - `left`: 왼쪽으로 더 덮는 너비. (0 보다 작으면 0)
	//==============================================================================
	pub fn set_view_bleed(&mut self, node_tree: &mut NodeTree, top: f32, right: f32, bottom: f32, left: f32) {
		self.view_bleed = [top.max(0.0), right.max(0.0), bottom.max(0.0), left.max(0.0)];
		self.place_all_windows(node_tree);
	}

	//==============================================================================
	/// 띄운 창과 닫는 중인 창을 모두 다시 놓기.
	//==============================================================================
	fn place_all_windows(&self, node_tree: &mut NodeTree) {
		for window_id in self.window_ids.iter().chain(self.closing_window_ids.iter()) {
			self.place_window(node_tree, *window_id);
		}
	}

	//==============================================================================
	/// 창 하나 놓기. (UI 배율로 나눈 영역의 크기와 덮는 너비를 알린다)
	//==============================================================================
	fn place_window(&self, node_tree: &mut NodeTree, window_id: NodeId) {
		let ui_scale = self.ui_scale;
		let [bleed_top, bleed_right, bleed_bottom, bleed_left] = self.view_bleed;
		let (view_width, view_height) = (self.view_width / ui_scale, self.view_height / ui_scale);
		node_tree.with_component_mut::<UIWindow, _>(window_id, |window, tree| {
			window.set_view_bleed(tree, bleed_top / ui_scale, bleed_right / ui_scale, bleed_bottom / ui_scale, bleed_left / ui_scale);
			window.set_view_size(tree, view_width, view_height);
		});
	}

	//==============================================================================
	/// 창 띄우기. (맨 위에 쌓는다, 이미 떠 있으면 맨 위로 올린다, 층 노드의 자식이 아니면 붙인다. 떼어 낸 창도 다시 띄운다)
	/// - 층이 트리에 붙기 전이거나 창 노드가 트리에 없으면 아무 일도 없다.
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	/// - `window_id`: 띄울 창 노드. (UIWindow 가 붙은 노드)
	//==============================================================================
	pub fn open_window(&mut self, node_tree: &mut NodeTree, window_id: NodeId) {
		let layer_id = match self.node_id {
			Some(layer_id) => {
				layer_id
			}
			None => {
				return;
			}
		};
		if !node_tree.contains(window_id) {
			return;
		}
		if node_tree.get_parent_id(window_id) != Some(layer_id) {
			node_tree.add_child(layer_id, window_id);
		}
		self.window_ids.retain(|candidate_id| *candidate_id != window_id);
		self.closing_window_ids.retain(|candidate_id| *candidate_id != window_id);
		let child_count = node_tree.get_child_count(layer_id);
		node_tree.set_sibling_index(window_id, child_count.saturating_sub(1));
		self.window_ids.push(window_id);
		self.place_window(node_tree, window_id);
		node_tree.with_component_mut::<UIWindow, _>(window_id, |window, tree| {
			window.set_window_layer_id(Some(layer_id));
			window.start_open_motion(tree);
		});
		self.refresh_window_covers(node_tree);
		self.notify_window_change();
	}

	//==============================================================================
	/// 창 닫기. (목록에서 빼고 닫는 움직임을 시작한다, 떠 있지 않으면 아무 일도 없다)
	/// - 덮여 숨은 창이나 즉시 전환인 창은 바로 떼어 낸다.
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	/// - `window_id`: 닫을 창 노드.
	//==============================================================================
	pub fn close_window(&mut self, node_tree: &mut NodeTree, window_id: NodeId) {
		let window_index = match self.window_ids.iter().position(|candidate_id| *candidate_id == window_id) {
			Some(window_index) => {
				window_index
			}
			None => {
				return;
			}
		};
		self.window_ids.remove(window_index);
		// 덮여 숨은 창은 움직임을 보일 수 없으니 바로 지운다.
		let is_covered = !node_tree.get_node(window_id).map(|window_node| window_node.is_active()).unwrap_or(false);
		let is_finished = node_tree.with_component_mut::<UIWindow, _>(window_id, |window, tree| {
			window.start_close_motion(tree);
			window.handle_closed();
			window.is_close_motion_finished()
		}).unwrap_or(true);
		if is_covered || is_finished {
			UIWindowLayer::remove_window_node(node_tree, window_id);
		}
		else {
			self.closing_window_ids.push(window_id);
		}
		self.refresh_window_covers(node_tree);
		self.notify_window_change();
	}

	//==============================================================================
	/// 창 모두 닫기. (위에서부터)
	/// - 바깥 닫기를 끈 창도 닫는다.
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	//==============================================================================
	pub fn close_all_windows(&mut self, node_tree: &mut NodeTree) {
		while let Some(top_window_id) = self.window_ids.last().copied() {
			self.close_window(node_tree, top_window_id);
		}
	}

	//==============================================================================
	/// 맨 위의 창 닫기. (기기의 뒤로 가기 자리, 바깥 닫기를 끈 창이면 닫지 않는다, 닫았으면 true)
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	///
	/// # 돌려줌
	/// 닫았으면 true. (떠 있는 창이 없거나 맨 위 창이 바깥 닫기를 껐으면 false)
	///
	/// # 예제
	/// 뒤로 가기를 창이 먼저 받기
	/// ```rust
	/// use vanilla::{NodeTree, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// let window_id = UIWindow::create(&mut node_tree, layer_id, "tutorial", Box::new(UIWindowDefaultBehavior));
	/// node_tree[window_id].get_component_mut::<UIWindow>().expect("window").set_outside_close_enabled(false);
	/// UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
	/// let is_closed = node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
	/// 	window_layer.close_top_window(tree)
	/// });
	/// assert_eq!(is_closed, Some(false));
	/// node_tree[window_id].get_component_mut::<UIWindow>().expect("window").set_outside_close_enabled(true);
	/// let is_closed = node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
	/// 	window_layer.close_top_window(tree)
	/// });
	/// assert_eq!(is_closed, Some(true));
	/// ```
	//==============================================================================
	pub fn close_top_window(&mut self, node_tree: &mut NodeTree) -> bool {
		let top_window_id = match self.get_top_window() {
			Some(top_window_id) => {
				top_window_id
			}
			None => {
				return false;
			}
		};
		let is_outside_close_enabled = node_tree.get_node(top_window_id).and_then(|window_node| window_node.get_component::<UIWindow>()).map(|window| window.is_outside_close_enabled()).unwrap_or(false);
		if !is_outside_close_enabled {
			return false;
		}
		self.close_window(node_tree, top_window_id);
		true
	}

	//==============================================================================
	/// 창 노드 떼어 내기. (UIWindowBehavior::dispose 뒤 층에서 뗀다, 노드는 남아 다시 띄울 수 있다)
	//==============================================================================
	fn remove_window_node(node_tree: &mut NodeTree, window_id: NodeId) {
		node_tree.with_component_mut::<UIWindow, _>(window_id, |window, tree| {
			window.set_window_layer_id(None);
			window.dispose(tree);
		});
		node_tree.remove_from_parent(window_id);
	}

	//==============================================================================
	/// 마우스 휠을 맨 위의 창에 넘기기. (창이 받지 않으면 그 자리의 스크롤 뷰, 받았으면 true, 원본 scrollWindowAt)
	///
	/// # 인자
	/// - `node_tree`: 층이 든 노드 트리.
	/// - `engine`: 엔진. (스크롤 뷰에 휠을 넘길 때 쓴다)
	/// - `layer_id`: 창 층 노드.
	/// - `view_input_position`: 휠을 굴린 자리. (뷰 좌표)
	/// - `wheel_delta`: 휠을 굴린 양.
	///
	/// # 돌려줌
	/// 맨 위 창이나 그 자리의 스크롤 뷰가 받았으면 true. (떠 있는 창이 없거나 받을 곳이 없으면 false)
	///
	/// # 예제
	/// 씬의 휠을 창 층에 먼저 넘기기
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, NodeId, NodeTree, UIWindowLayer, Vector2};
	///
	/// fn handle_wheel(node_tree: &mut NodeTree, engine: &Engine, layer_id: NodeId, wheel_delta: Vector2) -> bool {
	/// 	let view_input_position = engine.get_input_manager().get_view_input_position();
	/// 	UIWindowLayer::scroll_window_at(node_tree, engine, layer_id, view_input_position, wheel_delta)
	/// }
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// assert!(!handle_wheel(&mut node_tree, &engine, layer_id, Vector2::create(0.0, 120.0)));
	/// ```
	//==============================================================================
	pub fn scroll_window_at(node_tree: &mut NodeTree, engine: &Engine, layer_id: NodeId, view_input_position: Vector2, wheel_delta: Vector2) -> bool {
		let top_window_id = match node_tree.get_node(layer_id).and_then(|layer_node| layer_node.get_component::<UIWindowLayer>()).and_then(|window_layer| window_layer.get_top_window()) {
			Some(top_window_id) => {
				top_window_id
			}
			None => {
				return false;
			}
		};
		let is_handled = node_tree.with_component_mut::<UIWindow, _>(top_window_id, |window, tree| window.handle_wheel(tree, view_input_position, wheel_delta)).unwrap_or(false);
		if is_handled {
			return true;
		}
		let scroll_view_node_id = match UIWindowLayer::find_scroll_view_at(node_tree, top_window_id, view_input_position) {
			Some(scroll_view_node_id) => {
				scroll_view_node_id
			}
			None => {
				return false;
			}
		};
		node_tree.dispatch_wheel(scroll_view_node_id, engine, wheel_delta);
		true
	}

	//==============================================================================
	/// 그 자리를 덮는 맨 앞(나중에 그리는) 스크롤 뷰 노드 찾기. (켜진 노드만, 휠을 받는 칸(Component::is_wheel_receiver)이 붙은 노드, 없으면 None)
	///
	/// # 인자
	/// - `node_tree`: 찾을 노드 트리.
	/// - `node_id`: 찾기 시작할 노드. (그 노드와 자손을 뒤의 자식부터 본다)
	/// - `view_input_position`: 찾을 자리. (뷰 좌표)
	///
	/// # 돌려줌
	/// 그 자리를 덮는 맨 앞 스크롤 뷰 노드. (시작 노드가 꺼졌거나 그런 노드가 없으면 None)
	//==============================================================================
	pub fn find_scroll_view_at(node_tree: &NodeTree, node_id: NodeId, view_input_position: Vector2) -> Option<NodeId> {
		let node = node_tree.get_node(node_id)?;
		if !node.is_active() {
			return None;
		}
		for child_id in node_tree.get_child_ids(node_id).iter().rev() {
			if let Some(scroll_view_node_id) = UIWindowLayer::find_scroll_view_at(node_tree, *child_id, view_input_position) {
				return Some(scroll_view_node_id);
			}
		}
		if !is_view_position_inside_node(node_tree, node_id, view_input_position) {
			return None;
		}
		let has_scroll_view = node.get_all_components().iter().any(|component| component.is_enable() && component.is_wheel_receiver());
		if has_scroll_view {
			return Some(node_id);
		}
		None
	}

	//==============================================================================
	/// 노드가 들어 있는 창 층 노드 찾기. (자기부터 부모 쪽으로, 없으면 None, 원본 findWindowLayerOf)
	///
	/// # 인자
	/// - `node_tree`: 찾을 노드 트리.
	/// - `node_id`: 찾기 시작할 노드. (창 안의 단추 같은)
	///
	/// # 돌려줌
	/// UIWindowLayer 가 붙은 가장 가까운 조상(자기 포함) 노드. (뿌리까지 없으면 None)
	///
	/// # 예제
	/// 판 안의 노드에서 창 층 찾기
	/// ```rust
	/// use vanilla::{NodeTree, UIWindow, UIWindowDefaultBehavior, UIWindowLayer};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
	/// let window_id = UIWindow::create(&mut node_tree, layer_id, "inventory", Box::new(UIWindowDefaultBehavior));
	/// let panel_id = node_tree[window_id].get_component::<UIWindow>().expect("window").get_panel_node().expect("panel");
	/// let item_id = node_tree.create_child(panel_id, "Item");
	/// assert_eq!(UIWindowLayer::find_window_layer_of(&node_tree, item_id), Some(layer_id));
	/// assert_eq!(UIWindowLayer::find_window_layer_of(&node_tree, root_id), None);
	/// ```
	//==============================================================================
	pub fn find_window_layer_of(node_tree: &NodeTree, node_id: NodeId) -> Option<NodeId> {
		let mut current_id = Some(node_id);
		while let Some(candidate_id) = current_id {
			let is_window_layer = node_tree.get_node(candidate_id).map(|candidate_node| candidate_node.get_component::<UIWindowLayer>().is_some()).unwrap_or(false);
			if is_window_layer {
				return Some(candidate_id);
			}
			current_id = node_tree.get_parent_id(candidate_id);
		}
		None
	}

	//==============================================================================
	/// 창 덮기. (맨 위의 창만 보이고 아래 창은 숨긴다)
	//==============================================================================
	fn refresh_window_covers(&self, node_tree: &mut NodeTree) {
		let top_index = self.window_ids.len().saturating_sub(1);
		for (window_index, window_id) in self.window_ids.iter().enumerate() {
			let is_covered = window_index != top_index;
			node_tree.with_component_mut::<UIWindow, _>(*window_id, |window, tree| {
				window.set_covered(tree, is_covered);
			});
		}
	}

	//==============================================================================
	/// 창이 뜨거나 닫혔다고 알리기.
	//==============================================================================
	fn notify_window_change(&mut self) {
		let window_count = self.window_ids.len();
		if let Some(window_change_event) = self.window_change_event.as_mut() {
			window_change_event(window_count);
		}
	}
}

impl Default for UIWindowLayer {
	//==============================================================================
	/// 기본값. (UIWindowLayer::new 와 같다)
	//==============================================================================
	fn default() -> UIWindowLayer {
		UIWindowLayer::new()
	}
}

#[crate::reflect]
impl Component for UIWindowLayer {
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
	/// 컴포넌트 타입 이름. ("UIWindowLayer")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIWindowLayer"
	}

	//==============================================================================
	/// 트리에 붙음. (층 노드 꾸미기: 왼쪽 위 피봇과 앵커)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.node_id = Some(node_id);
		let layer_node = &mut node_tree[node_id];
		layer_node.set_pivot(Pivot::TOP_LEFT);
		layer_node.set_anchor(Pivot::TOP_LEFT);
	}

	//==============================================================================
	/// 갱신. (닫는 움직임이 끝난 창을 지운다. 층은 자식보다 먼저 돌아서 끝난 창은 다음 프레임에 지운다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		self.node_id = Some(context.get_node_id());
		let node_tree = context.get_node_tree_mut();
		let finished_window_ids: Vec<NodeId> = self.closing_window_ids.iter().copied().filter(|window_id| node_tree.get_node(*window_id).and_then(|window_node| window_node.get_component::<UIWindow>()).map(|window| window.is_close_motion_finished()).unwrap_or(true)).collect();
		for window_id in finished_window_ids {
			self.closing_window_ids.retain(|candidate_id| *candidate_id != window_id);
			UIWindowLayer::remove_window_node(node_tree, window_id);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::rect::Rect;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use crate::core::graphic::TextDirection;
	use crate::ui::uiconfirmwindow::{UIConfirmWindow, UIConfirmWindowOptions};
	use crate::ui::uilabel::UILabel;
	use crate::ui::uiwindow::{UIWindowBehavior, UIWindowDefaultBehavior};
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	/// 시험 장면. (800 x 600 뿌리 밑의 창 층, CSS 배율 1)
	//==============================================================================
	fn create_layer_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
		let layer_id = UIWindowLayer::create(&mut node_tree, root_id);
		node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| {
			window_layer.set_view_size(tree, 800.0, 600.0, 1.0);
			window_layer.set_view_bleed(tree, 40.0, 0.0, 30.0, 0.0);
		});
		(node_tree, layer_id)
	}

	//==============================================================================
	/// 층 반환.
	//==============================================================================
	fn get_layer(node_tree: &mut NodeTree, layer_id: NodeId) -> &mut UIWindowLayer {
		node_tree[layer_id].get_component_mut::<UIWindowLayer>().expect("layer")
	}

	//==============================================================================
	/// 창 반환.
	//==============================================================================
	fn get_window(node_tree: &mut NodeTree, window_id: NodeId) -> &mut UIWindow {
		node_tree[window_id].get_component_mut::<UIWindow>().expect("window")
	}

	//==============================================================================
	/// 노드 가운데를 누르고 뗀다.
	//==============================================================================
	fn click_node(node_tree: &mut NodeTree, engine: &Engine, node_id: NodeId) {
		let content_size = node_tree[node_id].get_content_size();
		let center = node_tree.convert_node_position_to_view_position(node_id, content_size.multiply_scalar(0.5));
		node_tree.dispatch_touch(node_id, engine, TouchKind::Press, center);
		node_tree.dispatch_touch(node_id, engine, TouchKind::Release, center);
	}

	//==============================================================================
	/// 창 둘을 쌓으면 맨 위만 보이고, 바깥을 누르면 닫혀 취소 알림, 아래 창이 다시 보이고, 움직임 뒤 지운다.
	//==============================================================================
	#[test]
	fn stack_cover_outside_close_and_remove() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, layer_id) = create_layer_tree();
		let change_counts = Rc::new(Cell::new(0usize));
		let change_counts_event = change_counts.clone();
		get_layer(&mut node_tree, layer_id).set_window_change_event(Some(Box::new(move |_window_count| {
			change_counts_event.set(change_counts_event.get() + 1);
		})));
		let first_window_id = UIWindow::create(&mut node_tree, layer_id, "first", Box::new(UIWindowDefaultBehavior));
		assert!(!node_tree[first_window_id].is_active());
		let close_count = Rc::new(Cell::new(0));
		let close_count_event = close_count.clone();
		get_window(&mut node_tree, first_window_id).set_panel_height(300.0);
		get_window(&mut node_tree, first_window_id).set_close_event(Some(Box::new(move || {
			close_count_event.set(close_count_event.get() + 1);
		})));
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, first_window_id);
		let confirm_window_id = UIConfirmWindow::create(&mut node_tree, layer_id, "confirm", UIConfirmWindowOptions { title: String::from("확인 창"), message: String::from("정말 할까요?"), ..UIConfirmWindowOptions::default() });
		let cancel_count = Rc::new(Cell::new(0));
		let cancel_count_event = cancel_count.clone();
		get_window(&mut node_tree, confirm_window_id).get_behavior_mut::<UIConfirmWindow>().expect("confirm").set_cancel_event(Some(Box::new(move || {
			cancel_count_event.set(cancel_count_event.get() + 1);
		})));
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, confirm_window_id);
		assert_eq!(get_layer(&mut node_tree, layer_id).get_window_count(), 2);
		assert_eq!(get_layer(&mut node_tree, layer_id).get_top_window(), Some(confirm_window_id));
		let found_window_id = node_tree[layer_id].get_component::<UIWindowLayer>().expect("layer").find_window(&node_tree, "first");
		assert_eq!(found_window_id, Some(first_window_id));
		assert!(!node_tree[first_window_id].is_active());
		assert!(node_tree[confirm_window_id].is_active());
		assert_eq!(change_counts.get(), 2);

		// 어둠은 세이프에어리어까지 덮고, 첫 창의 판은 가운데 480 x 300.
		let dim_id = get_window(&mut node_tree, first_window_id).get_dim_node().expect("dim");
		assert_eq!(node_tree[dim_id].get_local_position(), Vector2::create(0.0, -40.0));
		assert_eq!(node_tree[dim_id].get_content_size(), Vector2::create(800.0, 670.0));
		let first_parts = get_window(&mut node_tree, first_window_id).get_parts().expect("parts");
		assert_eq!(first_parts.panel_rect, Rect::create(160.0, 150.0, 480.0, 300.0));
		assert_eq!(node_tree[first_parts.panel_id].get_content_size(), Vector2::create(480.0, 300.0));

		// 바깥 누르기.
		node_tree.tick(&engine, 0.5);
		let confirm_dim_id = get_window(&mut node_tree, confirm_window_id).get_dim_node().expect("dim");
		click_node(&mut node_tree, &engine, confirm_dim_id);
		assert_eq!(cancel_count.get(), 1);
		assert_eq!(get_layer(&mut node_tree, layer_id).get_window_count(), 1);
		assert!(node_tree[first_window_id].is_active());
		assert!(node_tree.contains(confirm_window_id));
		assert_eq!(change_counts.get(), 3);
		node_tree.tick(&engine, 0.3);
		node_tree.tick(&engine, 0.016);
		assert!(node_tree.contains(confirm_window_id));
		assert_eq!(node_tree.get_parent_id(confirm_window_id), None);

		// 떼어 낸 창을 다시 띄운다. (원본처럼 같은 창)
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, confirm_window_id);
		assert_eq!(node_tree.get_parent_id(confirm_window_id), Some(layer_id));
		assert_eq!(get_layer(&mut node_tree, layer_id).get_top_window(), Some(confirm_window_id));
		assert!(node_tree[confirm_window_id].is_active());
		assert!(!node_tree[first_window_id].is_active());
		node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| window_layer.close_window(tree, confirm_window_id));
		assert_eq!(cancel_count.get(), 2);
		node_tree.tick(&engine, 0.3);
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree.get_parent_id(confirm_window_id), None);

		// 바깥 닫기를 끈 창은 뒤로 가기로 닫히지 않는다. 모두 닫기는 닫는다.
		get_window(&mut node_tree, first_window_id).set_outside_close_enabled(false);
		let is_closed = node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| window_layer.close_top_window(tree)).expect("layer");
		assert!(!is_closed);
		node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| window_layer.close_all_windows(tree));
		assert!(!get_layer(&mut node_tree, layer_id).has_open_window());
		assert_eq!(close_count.get(), 1);
	}

	//==============================================================================
	/// 확인 단추는 확인 알림 뒤 닫고 취소 알림은 없다. 오른쪽에서 왼쪽이면 확인이 왼쪽.
	//==============================================================================
	#[test]
	fn confirm_button_and_right_to_left() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, layer_id) = create_layer_tree();
		let window_id = UIConfirmWindow::create(&mut node_tree, layer_id, "confirm", UIConfirmWindowOptions { message: String::from("هل أنت متأكد؟"), ..UIConfirmWindowOptions::default() });
		let confirm_count = Rc::new(Cell::new(0));
		let cancel_count = Rc::new(Cell::new(0));
		{
			let confirm_count_event = confirm_count.clone();
			let cancel_count_event = cancel_count.clone();
			let confirm_window = get_window(&mut node_tree, window_id).get_behavior_mut::<UIConfirmWindow>().expect("confirm");
			confirm_window.set_right_to_left(true);
			confirm_window.set_confirm_event(Some(Box::new(move || {
				confirm_count_event.set(confirm_count_event.get() + 1);
			})));
			confirm_window.set_cancel_event(Some(Box::new(move || {
				cancel_count_event.set(cancel_count_event.get() + 1);
			})));
		}
		get_window(&mut node_tree, window_id).get_popup_motion_mut().set_instant_enabled(true);
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
		let (cancel_id, confirm_id, title_id) = {
			let confirm_window = get_window(&mut node_tree, window_id).get_behavior::<UIConfirmWindow>().expect("confirm");
			(confirm_window.get_cancel_button_node().expect("cancel"), confirm_window.get_confirm_button_node().expect("confirm"), confirm_window.get_title_node().expect("title"))
		};
		assert!(!node_tree[title_id].is_active());
		assert!(node_tree[confirm_id].get_local_position().x < node_tree[cancel_id].get_local_position().x);
		assert_eq!(node_tree[title_id].get_component::<UILabel>().expect("title").get_text_direction(), TextDirection::Rtl);
		UIConfirmWindow::set_right_to_left_in_tree(&mut node_tree, window_id, false);
		assert_eq!(node_tree[title_id].get_component::<UILabel>().expect("title").get_text_direction(), TextDirection::Inherit);
		UIConfirmWindow::set_right_to_left_in_tree(&mut node_tree, window_id, true);
		click_node(&mut node_tree, &engine, confirm_id);
		assert_eq!(confirm_count.get(), 1);
		assert_eq!(cancel_count.get(), 0);
		assert!(!get_layer(&mut node_tree, layer_id).has_open_window());
		// 즉시 전환이라 바로 떼어 낸다.
		assert_eq!(node_tree.get_parent_id(window_id), None);
	}

	//==============================================================================
	/// 작은 화면은 층이 커지고 창에는 나눈 크기를 알린다. 영역보다 큰 판은 통째로 줄인다. 배율을 끄면 1.
	//==============================================================================
	#[test]
	fn ui_scale_and_tall_panel() {
		let (mut node_tree, layer_id) = create_layer_tree();
		let window_id = UIWindow::create(&mut node_tree, layer_id, "tall", Box::new(UIWindowDefaultBehavior));
		get_window(&mut node_tree, window_id).set_panel_height(2000.0);
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
		let parts = get_window(&mut node_tree, window_id).get_parts().expect("parts");
		assert!((parts.panel_scale - 568.0 / 2000.0).abs() < 0.0001);
		node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| window_layer.set_view_size(tree, 800.0, 600.0, 0.4));
		assert!((get_layer(&mut node_tree, layer_id).get_ui_scale() - 1.8).abs() < 0.0001);
		assert!((node_tree[layer_id].get_content_size().x - 800.0 / 1.8).abs() < 0.001);
		let parts = get_window(&mut node_tree, window_id).get_parts().expect("parts");
		assert!((parts.view_height - 600.0 / 1.8).abs() < 0.001);
		node_tree.with_component_mut::<UIWindowLayer, _>(layer_id, |window_layer, tree| window_layer.set_ui_scale_options(tree, None));
		assert_eq!(get_layer(&mut node_tree, layer_id).get_ui_scale(), 1.0);
	}
	//==============================================================================
	/// 확인 창은 Graphic 없이도 처음 놓을 때부터 글 줄 수를 재 판의 세로를 정한다. (글꼴 파일이 없는 PC 는 건너뛴다)
	//==============================================================================
	#[test]
	fn confirm_window_measures_lines_before_first_draw() {
		let font_bytes = match crate::platform::find_fallback_font_file("가", 400, false, false).and_then(|source| std::fs::read(source.path).ok()) {
			Some(font_bytes) => {
				font_bytes
			}
			None => {
				return;
			}
		};
		let mut text_string_texture_cache = crate::core::textstringtexturecache::TextStringTextureCache::new();
		text_string_texture_cache.set_default_font(false, font_bytes).expect("default font");
		let (mut node_tree, layer_id) = create_layer_tree();
		let short_window_id = UIConfirmWindow::create(&mut node_tree, layer_id, "short", UIConfirmWindowOptions { message: String::from("짧은 글"), ..UIConfirmWindowOptions::default() });
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, short_window_id);
		let short_panel_height = get_window(&mut node_tree, short_window_id).get_panel_rect().size.y;
		let long_window_id = UIConfirmWindow::create(&mut node_tree, layer_id, "long", UIConfirmWindowOptions { message: "줄이 여러 번 바뀌는 긴 글이에요. ".repeat(6), ..UIConfirmWindowOptions::default() });
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, long_window_id);
		let long_panel_height = get_window(&mut node_tree, long_window_id).get_panel_rect().size.y;
		assert!(long_panel_height > short_panel_height + 40.0, "{} {}", long_panel_height, short_panel_height);
		let is_layout_pending = {
			let window = node_tree[long_window_id].get_component::<UIWindow>().expect("window");
			let parts = window.get_parts().expect("parts");
			let confirm_window = window.get_behavior::<UIConfirmWindow>().expect("confirm");
			confirm_window.is_layout_pending(&node_tree, &parts)
		};
		assert!(!is_layout_pending);
	}

	//==============================================================================
	/// 휠: 맨 위 창이 받지 않으면 그 자리의 스크롤 뷰가 받는다. 창 층 찾기는 자기부터 부모 쪽으로.
	//==============================================================================
	#[test]
	fn scroll_window_at_goes_to_top_window_scroll_view() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, layer_id) = create_layer_tree();
		let window_id = UIConfirmWindow::create(&mut node_tree, layer_id, "confirm", UIConfirmWindowOptions { message: "아주 긴 글. ".repeat(400), ..UIConfirmWindowOptions::default() });
		get_window(&mut node_tree, window_id).get_popup_motion_mut().set_instant_enabled(true);
		UIWindowLayer::open_window_in_tree(&mut node_tree, layer_id, window_id);
		node_tree.tick(&engine, 0.016);
		let scroll_id = get_window(&mut node_tree, window_id).get_behavior::<UIConfirmWindow>().expect("confirm").get_message_scroll_node().expect("scroll");
		let scroll_size = node_tree[scroll_id].get_content_size();
		let scroll_center = node_tree.convert_node_position_to_view_position(scroll_id, scroll_size.multiply_scalar(0.5));
		assert_eq!(UIWindowLayer::find_window_layer_of(&node_tree, scroll_id), Some(layer_id));
		assert_eq!(UIWindowLayer::find_scroll_view_at(&node_tree, window_id, scroll_center), Some(scroll_id));
		let content_height = node_tree[scroll_id].get_component::<crate::ui::uiscrollview::UIScrollView>().expect("scroll view").get_scroll_content_size().y;
		if content_height <= scroll_size.y {
			return;
		}
		assert!(UIWindowLayer::scroll_window_at(&mut node_tree, &engine, layer_id, scroll_center, Vector2::create(0.0, 120.0)));
		node_tree.tick(&engine, 0.5);
		let scroll_offset = node_tree[scroll_id].get_component::<crate::ui::uiscrollview::UIScrollView>().expect("scroll view").get_scroll_offset();
		assert!(scroll_offset.y < 0.0, "{:?}", scroll_offset);
	}
}
