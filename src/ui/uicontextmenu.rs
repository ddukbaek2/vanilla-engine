//==============================================================================
//! 컨텍스트 메뉴. (원본 ui/uicontextmenu.js 의 UIContextMenu, 우클릭 / 길게 누름 메뉴)
//! - show(node_tree, position, item_list) 로 항목 목록을 그 자리에 띄운다. 바깥을 누르면 닫히고, 항목을 누르면 닫힌 뒤 처리기를 부른다.
//! - 메뉴가 부모 영역 밖으로 나가지 않게 오른쪽 / 아래 끝에 붙인다. (부모 크기를 모르면 960 x 640)
//! - 씬 뿌리의 맨 위(마지막 자식)에 두고 쓴다.
//! - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//!   호스트를 꾸미고(왼쪽 위 피봇, 비활성) 바깥 누름 막(투명 UIButton)과 메뉴 판(둥근 Paint)을 자식으로 만든다.
//!   UIContextMenu::create(node_tree, parent_id, options) 가 원본 new UIContextMenu(options) + add_child 자리다.
//! - 사용:
//!   ```text
//!     let menu_id = UIContextMenu::create(node_tree, root_id, UIContextMenuOptions::default());
//!     node_tree.with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
//!         context_menu.show(tree, view_position, vec![UIContextMenuItem::create("복사", Box::new(|context| { ... })), UIContextMenuItem::create_disabled("삭제")]);
//!     });
//!   ```
//! - 원본과 다른 것:
//!   - show / close 는 노드를 바꾸므로 트리를 받는다. 씬 코드는 node_tree.with_component_mut 로 부른다.
//!   - 항목은 UIContextMenuItem 구조체({ text, handler?, is_disabled }), 처리기는 누른 행 노드의 문맥을 받는 한 번짜리 함수(FnOnce)다.
//!   - is_showing 은 노드 활성 대신 show / close 가 고치는 값을 돌려준다.
//!   - 이전 행은 떼기(removeChild) 대신 지운다. 행 노드 이름은 원본처럼 "ContextMenuItem{번호}" 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::TextAlign;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uibutton::UIButton;
use crate::ui::uilabel::UILabel;

/// 부모 크기를 모를 때 쓰는 영역. (원본과 같다)
const DEFAULT_AREA_WIDTH: f32 = 960.0;
const DEFAULT_AREA_HEIGHT: f32 = 640.0;

//==============================================================================
/// 컨텍스트 메뉴 설정. (원본 options = { menu_width = 168, item_height = 28, font_size = 12, panel_color?, text_color? })
///
/// # 예제
/// 넓은 메뉴, 높은 행
/// ```rust
/// use vanilla::{NodeTree, UIContextMenu, UIContextMenuItem, UIContextMenuOptions, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
/// let options = UIContextMenuOptions {
/// 	menu_width: 200.0,
/// 	item_height: 32.0,
/// 	..UIContextMenuOptions::default()
/// };
/// let menu_id = UIContextMenu::create(&mut node_tree, root_id, options);
/// node_tree.with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
/// 	context_menu.show(tree, Vector2::create(100.0, 100.0), vec![UIContextMenuItem::create_disabled("Paste")]);
/// });
/// let panel_id = node_tree[menu_id].get_component::<UIContextMenu>().expect("menu").get_panel_node().expect("panel");
/// assert_eq!(node_tree[panel_id].get_content_size(), Vector2::create(200.0, 40.0));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIContextMenuOptions {
	/// 메뉴 판 가로. (기본 168)
	pub menu_width: f32,
	/// 항목 행 높이. (기본 28, 판 세로는 항목 수 x 행 높이 + 8)
	pub item_height: f32,
	/// 항목 글 크기. (기본 12)
	pub font_size: f32,
	/// 메뉴 판 색.
	pub panel_color: Color,
	/// 누를 수 있는 항목의 글 색. (누를 수 없는 항목은 회색)
	pub text_color: Color,
}

impl Default for UIContextMenuOptions {
	//==============================================================================
	/// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIContextMenuOptions {
		UIContextMenuOptions {
			menu_width: 168.0,
			item_height: 28.0,
			font_size: 12.0,
			panel_color: Color::create(0.086, 0.106, 0.165, 1.0),
			text_color: Color::create(0.91, 0.925, 0.957, 1.0),
		}
	}
}

//==============================================================================
/// 항목 처리기. (누른 행 노드의 문맥을 받는다. 한 번만 불린다)
//==============================================================================
pub type UIContextMenuHandler = Box<dyn FnOnce(&mut ComponentContext)>;

//==============================================================================
/// 메뉴 항목. (원본 { text, handler?, is_disabled? })
///
/// # 예제
/// 누를 수 있는 항목과 누를 수 없는 항목
/// ```rust
/// use vanilla::{ComponentContext, UIContextMenuItem};
///
/// let copy_item = UIContextMenuItem::create("Copy", Box::new(|_context: &mut ComponentContext| {}));
/// let delete_item = UIContextMenuItem::create_disabled("Delete");
/// assert!(copy_item.handler.is_some());
/// assert!(!copy_item.is_disabled);
/// assert!(delete_item.is_disabled);
/// assert_eq!(delete_item.text, "Delete");
/// ```
//==============================================================================
pub struct UIContextMenuItem {
	/// 항목 글.
	pub text: String,
	/// 누르면 메뉴를 닫은 뒤 부르는 처리기. (None 이면 닫기만 한다)
	pub handler: Option<UIContextMenuHandler>,
	/// true 면 누를 수 없고 회색 글로 보인다.
	pub is_disabled: bool,
}

impl UIContextMenuItem {
	//==============================================================================
	/// 처리기가 있는 항목 생성.
	///
	/// # 인자
	/// - `text`: 항목 글.
	/// - `handler`: 누르면 메뉴를 닫은 뒤 부르는 처리기. (누른 행 노드의 문맥을 받는다)
	///
	/// # 돌려줌
	/// 누를 수 있는 항목.
	//==============================================================================
	pub fn create(text: &str, handler: UIContextMenuHandler) -> UIContextMenuItem {
		UIContextMenuItem {
			text: text.to_string(),
			handler: Some(handler),
			is_disabled: false,
		}
	}

	//==============================================================================
	/// 누를 수 없는 항목 생성. (회색 글자)
	///
	/// # 인자
	/// - `text`: 항목 글.
	///
	/// # 돌려줌
	/// 처리기 없이 누를 수 없는 항목.
	//==============================================================================
	pub fn create_disabled(text: &str) -> UIContextMenuItem {
		UIContextMenuItem {
			text: text.to_string(),
			handler: None,
			is_disabled: true,
		}
	}
}

//==============================================================================
/// 컨텍스트 메뉴.
///
/// # 예제
/// 항목을 누르면 처리기를 부르고 닫힌다
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{ComponentContext, Engine, EngineConfiguration, NodeTree, TouchKind, UIContextMenu, UIContextMenuItem, UIContextMenuOptions, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
/// let menu_id = UIContextMenu::create(&mut node_tree, root_id, UIContextMenuOptions::default());
/// let copy_count = Rc::new(Cell::new(0));
/// let copy_counter = copy_count.clone();
/// let item_list = vec![
/// 	UIContextMenuItem::create("Copy", Box::new(move |_context: &mut ComponentContext| {
/// 		copy_counter.set(copy_counter.get() + 1);
/// 	})),
/// 	UIContextMenuItem::create_disabled("Delete"),
/// ];
/// node_tree.with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
/// 	context_menu.show(tree, Vector2::create(100.0, 100.0), item_list);
/// });
/// let copy_item_id = node_tree[menu_id].get_component::<UIContextMenu>().expect("menu").get_item_nodes()[0];
/// let item_center = node_tree.convert_node_position_to_view_position(copy_item_id, Vector2::create(80.0, 14.0));
/// node_tree.dispatch_touch(copy_item_id, &engine, TouchKind::Press, item_center);
/// node_tree.dispatch_touch(copy_item_id, &engine, TouchKind::Release, item_center);
/// assert_eq!(copy_count.get(), 1);
/// assert!(!node_tree[menu_id].get_component::<UIContextMenu>().expect("menu").is_showing());
/// ```
//==============================================================================
pub struct UIContextMenu {
	component_base: ComponentBase,
	options: UIContextMenuOptions,
	node_id: Option<NodeId>,
	overlay_node_id: Option<NodeId>,
	panel_node_id: Option<NodeId>,
	row_node_ids: Vec<NodeId>,
	is_showing: bool,
}

#[crate::reflect]
impl UIContextMenu {
	//==============================================================================
	/// 생성. (기본 설정)
	///
	/// # 돌려줌
	/// 붙기 전의 메뉴. (UIContextMenuOptions::default)
	//==============================================================================
	pub fn new() -> UIContextMenu {
		UIContextMenu::create_with_options(UIContextMenuOptions::default())
	}

	//==============================================================================
	/// 설정으로 생성. (노드에는 node_tree.add_component 로 붙인다)
	///
	/// # 인자
	/// - `options`: 메뉴 설정.
	///
	/// # 돌려줌
	/// 붙기 전의 메뉴.
	//==============================================================================
	pub fn create_with_options(options: UIContextMenuOptions) -> UIContextMenu {
		UIContextMenu {
			component_base: ComponentBase::new(),
			options,
			node_id: None,
			overlay_node_id: None,
			panel_node_id: None,
			row_node_ids: Vec::new(),
			is_showing: false,
		}
	}

	//==============================================================================
	/// 노드 만들기. (parent_id 밑에 "UIContextMenu" 노드를 만들어 붙인다. 원본 new UIContextMenu(options) + add_child)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `parent_id`: 부모 노드. (보통 씬 뿌리, 그 content_size 가 메뉴를 가두는 영역이다)
	/// - `options`: 메뉴 설정.
	///
	/// # 돌려줌
	/// 만든 호스트 노드. (열기 전이라 꺼져 있다)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, parent_id: NodeId, options: UIContextMenuOptions) -> NodeId {
		let menu_id = node_tree.create_child(parent_id, "UIContextMenu");
		node_tree.add_component(menu_id, UIContextMenu::create_with_options(options));
		menu_id
	}

	//==============================================================================
	/// 메뉴 열기. (position 은 메뉴 왼쪽 위의 뷰 좌표. 행을 다시 만들고 영역 안에 붙여 띄운다)
	/// - 트리에 붙기 전이면 아무 일도 없다.
	///
	/// # 인자
	/// - `node_tree`: 메뉴가 든 노드 트리.
	/// - `position`: 메뉴 판 왼쪽 위 자리. (판의 로컬 자리로 쓰고, 영역 오른쪽, 아래 끝에서 4 안쪽을 넘지 않게 자른다)
	/// - `item_list`: 항목 목록. (차례대로 위에서 아래로 놓는다)
	///
	/// # 예제
	/// 오른쪽 아래 끝에서 연 메뉴는 영역 안으로 붙는다
	/// ```rust
	/// use vanilla::{NodeTree, UIContextMenu, UIContextMenuItem, UIContextMenuOptions, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
	/// let menu_id = UIContextMenu::create(&mut node_tree, root_id, UIContextMenuOptions::default());
	/// let item_list = vec![UIContextMenuItem::create_disabled("Undo"), UIContextMenuItem::create_disabled("Redo")];
	/// node_tree.with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
	/// 	context_menu.show(tree, Vector2::create(780.0, 590.0), item_list);
	/// });
	/// let panel_id = node_tree[menu_id].get_component::<UIContextMenu>().expect("menu").get_panel_node().expect("panel");
	/// assert_eq!(node_tree[panel_id].get_local_position(), Vector2::create(628.0, 532.0));
	/// ```
	//==============================================================================
	pub fn show(&mut self, node_tree: &mut NodeTree, position: Vector2, item_list: Vec<UIContextMenuItem>) {
		let (menu_id, overlay_id, panel_id) = match (self.node_id, self.overlay_node_id, self.panel_node_id) {
			(Some(menu_id), Some(overlay_id), Some(panel_id)) => {
				(menu_id, overlay_id, panel_id)
			}
			_ => {
				return;
			}
		};
		if !node_tree.contains(menu_id) || !node_tree.contains(panel_id) {
			return;
		}

		// 이전 행을 지우고 다시 만든다. (항목 수가 적어 부담 없다)
		for row_node_id in self.row_node_ids.drain(..) {
			node_tree.destroy_node(row_node_id);
		}
		let options = self.options;
		let panel_height = item_list.len() as f32 * options.item_height + 8.0;
		node_tree[panel_id].set_content_size(Vector2::create(options.menu_width, panel_height));
		for (item_index, item_data) in item_list.into_iter().enumerate() {
			let row_name = format!("ContextMenuItem{}", item_index);
			let row_id = node_tree.create_child(panel_id, &row_name);
			let row_node = &mut node_tree[row_id];
			row_node.set_pivot(Pivot::TOP_LEFT);
			row_node.set_anchor(Pivot::TOP_LEFT);
			row_node.set_content_size(Vector2::create(options.menu_width - 8.0, options.item_height));
			row_node.set_local_position(Vector2::create(4.0, 4.0 + item_index as f32 * options.item_height));
			row_node.set_interactable(true);
			let row_paint = row_node.add_component(Paint::create_with_color(Color::transparent()));
			row_paint.set_round_size(6.0);
			let is_disabled = item_data.is_disabled;
			if let Some(row_button) = node_tree.add_component(row_id, UIButton::new()) {
				row_button.set_pressed_tint_color(Color::create(1.0, 1.0, 1.0, 0.08));
				if is_disabled {
					row_button.set_interactable(false);
				}
				else {
					let mut handler = item_data.handler;
					row_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
						context.get_node_tree_mut().with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
							context_menu.close(tree);
						});
						if let Some(handler) = handler.take() {
							handler(context);
						}
					}));
				}
			}
			if let Some(row_label) = node_tree.add_component(row_id, UILabel::new()) {
				row_label.set_text(&item_data.text);
				row_label.set_font_size(options.font_size);
				row_label.set_text_align(TextAlign::Left);
				let label_color = if is_disabled {
					Color::create(0.4, 0.43, 0.5, 1.0)
				}
				else {
					options.text_color
				};
				row_label.set_text_color(label_color);
			}
			self.row_node_ids.push(row_id);
		}

		// 부모 크기에 맞춰 막을 깔고, 메뉴가 영역 밖으로 나가지 않게 붙인다.
		let mut area_width = DEFAULT_AREA_WIDTH;
		let mut area_height = DEFAULT_AREA_HEIGHT;
		if let Some(parent_node) = node_tree.get_parent_id(menu_id).and_then(|parent_id| node_tree.get_node(parent_id)) {
			let parent_size = parent_node.get_content_size();
			if parent_size.x > 0.0 && parent_size.y > 0.0 {
				area_width = parent_size.x;
				area_height = parent_size.y;
			}
		}
		let area_size = Vector2::create(area_width, area_height);
		node_tree[menu_id].set_content_size(area_size);
		if let Some(overlay_node) = node_tree.get_node_mut(overlay_id) {
			overlay_node.set_content_size(area_size);
		}
		let menu_x = position.x.min(area_width - options.menu_width - 4.0);
		let menu_y = position.y.min(area_height - panel_height - 4.0);
		node_tree[panel_id].set_local_position(Vector2::create(menu_x, menu_y));
		node_tree[menu_id].set_active(true);
		self.is_showing = true;
	}

	//==============================================================================
	/// 닫기.
	///
	/// # 인자
	/// - `node_tree`: 메뉴가 든 노드 트리. (호스트 노드를 끈다, 행 노드는 다음 show 까지 남는다)
	//==============================================================================
	pub fn close(&mut self, node_tree: &mut NodeTree) {
		if let Some(menu_node) = self.node_id.and_then(|menu_id| node_tree.get_node_mut(menu_id)) {
			menu_node.set_active(false);
		}
		self.is_showing = false;
	}

	//==============================================================================
	/// 보이는 중인지.
	///
	/// # 돌려줌
	/// show 뒤 close 전이면 true.
	//==============================================================================
	pub fn is_showing(&self) -> bool {
		self.is_showing
	}

	//==============================================================================
	/// 바깥 누름 막 노드 핸들.
	///
	/// # 돌려줌
	/// 투명한 막 노드. (트리에 붙기 전이면 None)
	//==============================================================================
	pub fn get_overlay_node(&self) -> Option<NodeId> {
		self.overlay_node_id
	}

	//==============================================================================
	/// 메뉴 판 노드 핸들.
	///
	/// # 돌려줌
	/// 메뉴 판 노드. (트리에 붙기 전이면 None)
	//==============================================================================
	pub fn get_panel_node(&self) -> Option<NodeId> {
		self.panel_node_id
	}

	//==============================================================================
	/// 항목 행 노드 핸들 목록.
	///
	/// # 돌려줌
	/// 마지막 show 가 만든 행 노드. (항목 차례, show 마다 다시 만든다)
	//==============================================================================
	pub fn get_item_nodes(&self) -> &[NodeId] {
		&self.row_node_ids
	}
}

impl Default for UIContextMenu {
	//==============================================================================
	/// 기본값. (UIContextMenu::new 와 같다)
	//==============================================================================
	fn default() -> UIContextMenu {
		UIContextMenu::new()
	}
}

#[crate::reflect]
impl Component for UIContextMenu {
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
	/// 컴포넌트 타입 이름. ("UIContextMenu")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIContextMenu"
	}

	//==============================================================================
	/// 트리에 붙음. (원본 생성자: 호스트 꾸미기, 바깥 누름 막, 메뉴 판)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.node_id = Some(node_id);
		let menu_node = &mut node_tree[node_id];
		menu_node.set_pivot(Pivot::TOP_LEFT);
		menu_node.set_anchor(Pivot::TOP_LEFT);
		menu_node.set_active(false);

		// 바깥 누름 감지 막. (투명)
		let overlay_id = node_tree.create_child(node_id, "ContextMenuOverlay");
		let overlay_node = &mut node_tree[overlay_id];
		overlay_node.set_pivot(Pivot::TOP_LEFT);
		overlay_node.set_anchor(Pivot::TOP_LEFT);
		overlay_node.set_interactable(true);
		if let Some(overlay_button) = node_tree.add_component(overlay_id, UIButton::new()) {
			overlay_button.set_pressed_tint_color(Color::transparent());
			overlay_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				context.get_node_tree_mut().with_component_mut::<UIContextMenu, _>(node_id, |context_menu, tree| {
					context_menu.close(tree);
				});
			}));
		}
		self.overlay_node_id = Some(overlay_id);

		// 메뉴 판.
		let panel_id = node_tree.create_child(node_id, "ContextMenuPanel");
		let panel_node = &mut node_tree[panel_id];
		panel_node.set_pivot(Pivot::TOP_LEFT);
		panel_node.set_anchor(Pivot::TOP_LEFT);
		let panel_paint = panel_node.add_component(Paint::create_with_color(self.options.panel_color));
		panel_paint.set_round_size(8.0);
		self.panel_node_id = Some(panel_id);
	}

	//==============================================================================
	/// tick 이 필요한지. (돌 일이 없어 false)
	//==============================================================================
	fn is_tick_needed(&self) -> bool {
		false
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
	/// 시험 장면. (800 x 600 뿌리 밑의 메뉴)
	//==============================================================================
	fn create_menu_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
		let menu_id = UIContextMenu::create(&mut node_tree, root_id, UIContextMenuOptions::default());
		(node_tree, menu_id)
	}

	//==============================================================================
	/// 셈하는 항목.
	//==============================================================================
	fn create_counting_item(text: &str, counter: &Rc<Cell<u32>>) -> UIContextMenuItem {
		let counter = counter.clone();
		UIContextMenuItem::create(text, Box::new(move |_context: &mut ComponentContext| {
			counter.set(counter.get() + 1);
		}))
	}

	//==============================================================================
	/// 메뉴 열기. (셈 항목 둘 + 누를 수 없는 항목 하나)
	//==============================================================================
	fn show_menu(node_tree: &mut NodeTree, menu_id: NodeId, position: Vector2, copy_count: &Rc<Cell<u32>>, paste_count: &Rc<Cell<u32>>) {
		let item_list = vec![create_counting_item("복사", copy_count), create_counting_item("붙여넣기", paste_count), UIContextMenuItem::create_disabled("삭제")];
		node_tree.with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
			context_menu.show(tree, position, item_list);
		});
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
	/// 열면 영역 안으로 붙고, 항목을 누르면 처리기를 부르고 닫힌다. 바깥을 누르면 처리기 없이 닫힌다.
	//==============================================================================
	#[test]
	fn show_clamp_item_click_and_overlay_close() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, menu_id) = create_menu_tree();
		assert!(!node_tree[menu_id].is_active());
		let copy_count = Rc::new(Cell::new(0));
		let paste_count = Rc::new(Cell::new(0));
		show_menu(&mut node_tree, menu_id, Vector2::create(700.0, 580.0), &copy_count, &paste_count);
		let (panel_id, overlay_id, item_ids) = {
			let context_menu = node_tree[menu_id].get_component::<UIContextMenu>().expect("menu");
			assert!(context_menu.is_showing());
			(context_menu.get_panel_node().expect("panel"), context_menu.get_overlay_node().expect("overlay"), context_menu.get_item_nodes().to_vec())
		};
		assert!(node_tree[menu_id].is_active());
		assert_eq!(node_tree[overlay_id].get_content_size(), Vector2::create(800.0, 600.0));
		assert_eq!(node_tree[panel_id].get_content_size(), Vector2::create(168.0, 92.0));
		assert_eq!(node_tree[panel_id].get_local_position(), Vector2::create(628.0, 504.0));
		assert_eq!(item_ids.len(), 3);
		assert_eq!(node_tree[item_ids[1]].get_name(), "ContextMenuItem1");

		click_node(&mut node_tree, &engine, item_ids[1]);
		assert_eq!(paste_count.get(), 1);
		assert_eq!(copy_count.get(), 0);
		assert!(!node_tree[menu_id].is_active());
		assert!(!node_tree[menu_id].get_component::<UIContextMenu>().expect("menu").is_showing());

		// 다시 열고 누를 수 없는 항목을 누르면 아무 일도 없다. 바깥을 누르면 닫힌다.
		show_menu(&mut node_tree, menu_id, Vector2::create(100.0, 100.0), &copy_count, &paste_count);
		let item_ids = node_tree[menu_id].get_component::<UIContextMenu>().expect("menu").get_item_nodes().to_vec();
		assert_eq!(node_tree[panel_id].get_local_position(), Vector2::create(100.0, 100.0));
		click_node(&mut node_tree, &engine, item_ids[2]);
		assert!(node_tree[menu_id].is_active());
		node_tree.dispatch_touch(overlay_id, &engine, TouchKind::Press, Vector2::create(10.0, 10.0));
		node_tree.dispatch_touch(overlay_id, &engine, TouchKind::Release, Vector2::create(10.0, 10.0));
		assert!(!node_tree[menu_id].is_active());
		assert_eq!(copy_count.get() + paste_count.get(), 1);
	}
}
