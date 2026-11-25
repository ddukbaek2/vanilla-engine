//==============================================================================
// 드롭다운. (원본 ui/uidropdown.js 의 UIDropdown, 콤보박스)
// - 닫힌 머리 단추에 지금 값이 보이고, 누르면 아래로 선택지 목록이 펼쳐진다. 행을 누르면 고르고 접힌다.
// - 펼칠 때 목록을 트리 뿌리의 마지막 자식으로 옮겨, 나중에 그려지는 형제 노드에 가리지 않게 한다. (접으면 머리 단추 아래로 되돌린다)
// - 머리 단추의 틴트 대상에서 펼침 목록 노드를 뺀다. (exclude_node_from_tint, 빼지 않으면 고른 뒤 머리 단추가 매 tick 눌렀을 때 담아 둔 옛 행 색을 다시 써서
//   선택 강조가 덮인다) 강조를 바꾼 뒤에는 행 단추들이 틴트 원래 색을 다시 담게 한다. (원본은 그 자리에서, 러스트 판은 다음 tick 에)
// - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//   호스트를 꾸미고(왼쪽 위 피봇, 크기, interactable) Paint + UIButton + UILabel(머리 글)을 호스트에, 화살표 노드와 펼침 목록 노드를 자식으로 만든다.
//   UIDropdown::create(node_tree, parent_id, options) 가 원본 new UIDropdown(options) + add_child 자리다.
// - 사용:
//     let dropdown_id = UIDropdown::create(node_tree, panel_id, UIDropdownOptions { width: 180.0, ..UIDropdownOptions::default() });
//     node_tree.with_component_mut::<UIDropdown, _>(dropdown_id, |dropdown, tree| dropdown.set_option_list(tree, &["Dark", "Light", "System"]));
//     node_tree[dropdown_id].get_component_mut::<UIDropdown>().unwrap().set_changed_event(Box::new(|dropdown, context| { dropdown.get_selected_text(); }));
// - 원본과 다른 것:
//   - 트리를 바꾸는 함수(set_option_list, set_open, set_selected_index)는 트리를 받는다. 씬 코드는 node_tree.with_component_mut 로 부른다.
//     select_index 는 바뀜 이벤트에 문맥을 넘기려고 문맥을 받는다. (행 단추가 ComponentContext::with_component_mut 로 부른다)
//   - 바뀜 이벤트는 드롭다운과 문맥을 받는다. options 객체는 UIDropdownOptions 구조체다.
//   - 행 강조는 펼침 목록의 자식 순서 대신 만든 행 노드 목록으로 고친다. (tick 중에 다시 만들면 지운 행이 잠깐 자식으로 남는다)
//   - 화살표 글자는 원본의 작은 삼각형(▾ ▴, U+25BE / U+25B4)이 기본 글꼴(맑은 고딕, Segoe UI, Arial)에 없어
//     WGL4 에 든 ▼ ▲ 를 글자 크기 0.75 배로 쓴다.
//   - 노드 이름은 원본처럼 "DropdownArrow", "DropdownPopup", "DropdownOption{번호}" 다. 호스트 이름은 만든 쪽이 정한다. (create 는 "UIDropdown")
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uibutton::UIButton;
use crate::ui::uilabel::UILabel;

// 펼친 화살표 / 접힌 화살표. (원본 ▴ ▾ 는 기본 글꼴에 없어 ▲ ▼ 를 작게 쓴다)
const ARROW_OPEN_TEXT: &str = "▲";
const ARROW_CLOSED_TEXT: &str = "▼";
const ARROW_FONT_SCALE: f32 = 0.75;

//==============================================================================
// 드롭다운 설정. (원본 options = { width = 180, height = 30, option_height = 28, font_size = 12, base_color?, popup_color?, highlight_color?, text_color? })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIDropdownOptions {
	pub width: f32,
	pub height: f32,
	// 선택지 행 높이.
	pub option_height: f32,
	pub font_size: f32,
	// 머리 단추 바탕색.
	pub base_color: Color,
	// 펼침 목록 바탕색.
	pub popup_color: Color,
	// 고른 행 강조색.
	pub highlight_color: Color,
	pub text_color: Color,
}

impl Default for UIDropdownOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIDropdownOptions {
		UIDropdownOptions {
			width: 180.0,
			height: 30.0,
			option_height: 28.0,
			font_size: 12.0,
			base_color: Color::create(0.114, 0.141, 0.212, 1.0),
			popup_color: Color::create(0.086, 0.106, 0.165, 1.0),
			highlight_color: Color::create(0.424, 0.482, 1.0, 0.25),
			text_color: Color::create(0.91, 0.925, 0.957, 1.0),
		}
	}
}

//==============================================================================
// 드롭다운 이벤트. (드롭다운과 문맥을 받는다)
//==============================================================================
pub type UIDropdownEvent = Box<dyn FnMut(&mut UIDropdown, &mut ComponentContext)>;

//==============================================================================
// 드롭다운.
//==============================================================================
pub struct UIDropdown {
	component_base: ComponentBase,
	options: UIDropdownOptions,
	node_id: Option<NodeId>,
	option_list: Vec<String>,
	selected_index: usize,
	is_open: bool,
	changed_event: Option<UIDropdownEvent>,
	arrow_node_id: Option<NodeId>,
	popup_node_id: Option<NodeId>,
	row_node_ids: Vec<NodeId>,
}

impl UIDropdown {
	//==============================================================================
	// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UIDropdown {
		UIDropdown::create_with_options(UIDropdownOptions::default())
	}

	//==============================================================================
	// 설정으로 생성. (노드에는 node_tree.add_component 로 붙인다)
	//==============================================================================
	pub fn create_with_options(options: UIDropdownOptions) -> UIDropdown {
		UIDropdown {
			component_base: ComponentBase::new(),
			options,
			node_id: None,
			option_list: Vec::new(),
			selected_index: 0,
			is_open: false,
			changed_event: None,
			arrow_node_id: None,
			popup_node_id: None,
			row_node_ids: Vec::new(),
		}
	}

	//==============================================================================
	// 노드 만들기. (parent_id 밑에 "UIDropdown" 노드를 만들어 붙인다. 원본 new UIDropdown(options) + add_child)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, parent_id: NodeId, options: UIDropdownOptions) -> NodeId {
		let dropdown_id = node_tree.create_child(parent_id, "UIDropdown");
		node_tree.add_component(dropdown_id, UIDropdown::create_with_options(options));
		dropdown_id
	}

	//==============================================================================
	// 선택지 목록 설정. (펼침 목록 행을 다시 만든다)
	//==============================================================================
	pub fn set_option_list(&mut self, node_tree: &mut NodeTree, option_list: &[impl AsRef<str>]) {
		self.option_list = option_list.iter().map(|option_text| option_text.as_ref().to_string()).collect();
		self.selected_index = self.selected_index.min(self.option_list.len().saturating_sub(1));
		let (host_id, popup_id) = match (self.node_id, self.popup_node_id) {
			(Some(host_id), Some(popup_id)) => {
				(host_id, popup_id)
			}
			_ => {
				return;
			}
		};
		if !node_tree.contains(host_id) || !node_tree.contains(popup_id) {
			return;
		}

		// 기존 행 지우기.
		for row_node_id in self.row_node_ids.drain(..) {
			node_tree.destroy_node(row_node_id);
		}

		let content_size = node_tree[host_id].get_content_size();
		let option_height = self.options.option_height;
		let popup_height = self.option_list.len() as f32 * option_height + 8.0;
		node_tree[popup_id].set_content_size(Vector2::create(content_size.x, popup_height));
		for option_index in 0..self.option_list.len() {
			let row_name = format!("DropdownOption{}", option_index);
			let row_id = node_tree.create_child(popup_id, &row_name);
			let row_node = &mut node_tree[row_id];
			row_node.set_pivot(Pivot::TOP_LEFT);
			row_node.set_anchor(Pivot::TOP_LEFT);
			row_node.set_content_size(Vector2::create(content_size.x - 8.0, option_height));
			row_node.set_local_position(Vector2::create(4.0, 4.0 + option_index as f32 * option_height));
			row_node.set_interactable(true);
			let row_paint = row_node.add_component(Paint::create_with_color(Color::transparent()));
			row_paint.set_round_size(6.0);
			if let Some(row_button) = node_tree.add_component(row_id, UIButton::new()) {
				row_button.set_pressed_tint_color(Color::create(1.0, 1.0, 1.0, 0.08));
				row_button.set_clicked_event(Box::new(move |button: &mut UIButton, context: &mut ComponentContext| {
					context.with_component_mut::<UIDropdown, _>(host_id, |dropdown, dropdown_context| {
						dropdown.select_index(dropdown_context, option_index);
					});
					// 누를 때 담은 행 색은 강조를 바꾸기 전 것이라, 다음 tick 에 다시 담는다.
					button.request_color_target_collect();
				}));
			}
			if let Some(row_label) = node_tree.add_component(row_id, UILabel::new()) {
				row_label.set_text(&self.option_list[option_index]);
				row_label.set_font_size(self.options.font_size);
				row_label.set_text_color(self.options.text_color);
			}
			self.row_node_ids.push(row_id);
		}
		self.refresh_header(node_tree);
		self.refresh_highlight(node_tree);
	}

	//==============================================================================
	// 고르기. (접고, 머리 글과 강조를 고치고, 바뀌었으면 바뀜 이벤트를 부른다. 문맥은 드롭다운 호스트의 것)
	//==============================================================================
	pub fn select_index(&mut self, context: &mut ComponentContext, option_index: usize) {
		if option_index >= self.option_list.len() {
			return;
		}
		let is_changed = self.selected_index != option_index;
		self.selected_index = option_index;
		let node_tree = context.get_node_tree_mut();
		self.set_open(node_tree, false);
		self.refresh_header(node_tree);
		self.refresh_highlight(node_tree);
		if is_changed {
			self.fire_changed_event(context);
		}
	}

	//==============================================================================
	// 바뀜 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fire_changed_event(&mut self, context: &mut ComponentContext) {
		let mut changed_event = match self.changed_event.take() {
			Some(changed_event) => {
				changed_event
			}
			None => {
				return;
			}
		};
		changed_event(self, context);
		if self.changed_event.is_none() {
			self.changed_event = Some(changed_event);
		}
	}

	//==============================================================================
	// 펼침 / 접힘. (펼치면 목록을 트리 뿌리로 옮기고, 접으면 되돌린다)
	//==============================================================================
	pub fn set_open(&mut self, node_tree: &mut NodeTree, is_open: bool) {
		self.is_open = is_open;
		if is_open {
			self.attach_popup_to_root(node_tree);
		}
		else {
			self.detach_popup_from_root(node_tree);
		}
		if let Some(popup_node) = self.popup_node_id.and_then(|popup_id| node_tree.get_node_mut(popup_id)) {
			popup_node.set_active(is_open);
		}
		let arrow_text = if is_open {
			ARROW_OPEN_TEXT
		}
		else {
			ARROW_CLOSED_TEXT
		};
		if let Some(arrow_label) = self.arrow_node_id.and_then(|arrow_id| node_tree.get_node_mut(arrow_id)).and_then(|arrow_node| arrow_node.get_component_mut::<UILabel>()) {
			arrow_label.set_text(arrow_text);
		}
	}

	//==============================================================================
	// 펼침 목록을 트리 뿌리로 옮긴다. (머리 단추 왼쪽 아래 + 4 의 뷰 좌표를 뿌리 로컬 좌표로 바꾼다)
	//==============================================================================
	fn attach_popup_to_root(&mut self, node_tree: &mut NodeTree) {
		let (host_id, popup_id) = match (self.node_id, self.popup_node_id) {
			(Some(host_id), Some(popup_id)) => {
				(host_id, popup_id)
			}
			_ => {
				return;
			}
		};
		if !node_tree.contains(host_id) || !node_tree.contains(popup_id) {
			return;
		}
		let mut root_id = host_id;
		while let Some(parent_id) = node_tree.get_parent_id(root_id) {
			root_id = parent_id;
		}
		if root_id == host_id {
			return;
		}
		let host_content_size = node_tree[host_id].get_content_size();
		let bottom_left = node_tree.convert_node_position_to_view_position(host_id, Vector2::create(0.0, host_content_size.y));
		let popup_view_position = Vector2::create(bottom_left.x, bottom_left.y + 4.0);
		let popup_local_position = node_tree.convert_view_position_to_node_position(root_id, popup_view_position).unwrap_or(popup_view_position);
		node_tree.add_child(root_id, popup_id);
		node_tree[popup_id].set_local_position(popup_local_position);
	}

	//==============================================================================
	// 펼침 목록을 머리 단추 아래로 되돌린다.
	//==============================================================================
	fn detach_popup_from_root(&mut self, node_tree: &mut NodeTree) {
		let (host_id, popup_id) = match (self.node_id, self.popup_node_id) {
			(Some(host_id), Some(popup_id)) => {
				(host_id, popup_id)
			}
			_ => {
				return;
			}
		};
		if !node_tree.contains(host_id) || !node_tree.contains(popup_id) {
			return;
		}
		if node_tree.get_parent_id(popup_id) == Some(host_id) {
			return;
		}
		node_tree.add_child(host_id, popup_id);
		let host_content_size = node_tree[host_id].get_content_size();
		node_tree[popup_id].set_local_position(Vector2::create(0.0, host_content_size.y + 4.0));
	}

	//==============================================================================
	// 머리 글 갱신. (호스트의 UILabel)
	//==============================================================================
	fn refresh_header(&mut self, node_tree: &mut NodeTree) {
		let selected_text = self.get_selected_text().to_string();
		if let Some(header_label) = self.node_id.and_then(|host_id| node_tree.get_node_mut(host_id)).and_then(|host_node| host_node.get_component_mut::<UILabel>()) {
			header_label.set_text(&selected_text);
		}
	}

	//==============================================================================
	// 고른 행 강조 갱신. (행 단추가 틴트 원래 색을 다음 tick 에 다시 담게 한다)
	//==============================================================================
	fn refresh_highlight(&mut self, node_tree: &mut NodeTree) {
		for (row_index, row_node_id) in self.row_node_ids.iter().copied().enumerate() {
			let row_color = if row_index == self.selected_index {
				self.options.highlight_color
			}
			else {
				Color::transparent()
			};
			let row_node = match node_tree.get_node_mut(row_node_id) {
				Some(row_node) => {
					row_node
				}
				None => {
					continue;
				}
			};
			if let Some(row_paint) = row_node.get_component_mut::<Paint>() {
				row_paint.set_color(row_color);
			}
			if let Some(row_button) = row_node.get_component_mut::<UIButton>() {
				row_button.request_color_target_collect();
			}
		}
	}

	//==============================================================================
	// 바뀜 이벤트 설정.
	//==============================================================================
	pub fn set_changed_event(&mut self, changed_event: UIDropdownEvent) {
		self.changed_event = Some(changed_event);
	}

	//==============================================================================
	// 고른 번호 설정. (범위로 자른다. 바뀜 이벤트는 부르지 않는다. 원본과 같음)
	//==============================================================================
	pub fn set_selected_index(&mut self, node_tree: &mut NodeTree, selected_index: usize) {
		self.selected_index = selected_index.min(self.option_list.len().saturating_sub(1));
		self.refresh_header(node_tree);
		self.refresh_highlight(node_tree);
	}

	//==============================================================================
	// 고른 번호 반환.
	//==============================================================================
	pub fn get_selected_index(&self) -> usize {
		self.selected_index
	}

	//==============================================================================
	// 고른 글 반환. (선택지가 없으면 빈 글)
	//==============================================================================
	pub fn get_selected_text(&self) -> &str {
		self.option_list.get(self.selected_index).map(String::as_str).unwrap_or("")
	}

	//==============================================================================
	// 선택지 목록 반환.
	//==============================================================================
	pub fn get_option_list(&self) -> &[String] {
		&self.option_list
	}

	//==============================================================================
	// 펼쳐져 있는지.
	//==============================================================================
	pub fn is_open(&self) -> bool {
		self.is_open
	}

	//==============================================================================
	// 펼침 목록 노드 핸들. (펼친 동안은 트리 뿌리의 자식)
	//==============================================================================
	pub fn get_popup_node(&self) -> Option<NodeId> {
		self.popup_node_id
	}

	//==============================================================================
	// 선택지 행 노드 핸들 목록.
	//==============================================================================
	pub fn get_option_nodes(&self) -> &[NodeId] {
		&self.row_node_ids
	}
}

impl Default for UIDropdown {
	fn default() -> UIDropdown {
		UIDropdown::new()
	}
}

impl Component for UIDropdown {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UIDropdown"
	}

	//==============================================================================
	// 트리에 붙음. (원본 생성자: 호스트 꾸미기, 머리 단추, 화살표, 펼침 목록)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.node_id = Some(node_id);
		let options = self.options;
		let host_node = &mut node_tree[node_id];
		host_node.set_pivot(Pivot::TOP_LEFT);
		host_node.set_anchor(Pivot::TOP_LEFT);
		host_node.set_content_size(Vector2::create(options.width, options.height));
		host_node.set_interactable(true);

		// 머리 단추.
		let header_paint = host_node.add_component(Paint::create_with_color(options.base_color));
		header_paint.set_round_size(8.0);
		if let Some(header_button) = node_tree.add_component(node_id, UIButton::new()) {
			header_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				context.with_component_mut::<UIDropdown, _>(node_id, |dropdown, dropdown_context| {
					let next_is_open = !dropdown.is_open();
					dropdown.set_open(dropdown_context.get_node_tree_mut(), next_is_open);
				});
			}));
		}
		if let Some(header_label) = node_tree.add_component(node_id, UILabel::new()) {
			header_label.set_text("");
			header_label.set_font_size(options.font_size);
			header_label.set_text_color(options.text_color);
		}

		// 펼침 화살표.
		let arrow_id = node_tree.create_child(node_id, "DropdownArrow");
		let arrow_node = &mut node_tree[arrow_id];
		arrow_node.set_pivot(Pivot::TOP_LEFT);
		arrow_node.set_anchor(Pivot::TOP_LEFT);
		arrow_node.set_content_size(Vector2::create(20.0, options.height));
		arrow_node.set_local_position(Vector2::create(options.width - 22.0, 0.0));
		if let Some(arrow_label) = node_tree.add_component(arrow_id, UILabel::new()) {
			arrow_label.set_text(ARROW_CLOSED_TEXT);
			arrow_label.set_font_size(options.font_size * ARROW_FONT_SCALE);
			arrow_label.set_text_color(options.text_color);
		}
		self.arrow_node_id = Some(arrow_id);

		// 펼침 목록. (닫혀 있으면 비활성)
		let popup_id = node_tree.create_child(node_id, "DropdownPopup");
		let popup_node = &mut node_tree[popup_id];
		popup_node.set_pivot(Pivot::TOP_LEFT);
		popup_node.set_anchor(Pivot::TOP_LEFT);
		popup_node.set_local_position(Vector2::create(0.0, options.height + 4.0));
		popup_node.set_active(false);
		let popup_paint = popup_node.add_component(Paint::create_with_color(options.popup_color));
		popup_paint.set_round_size(8.0);
		self.popup_node_id = Some(popup_id);

		// 머리 단추 틴트가 펼침 목록 행까지 닿지 않게 한다.
		if let Some(header_button) = node_tree[node_id].get_component_mut::<UIButton>() {
			header_button.exclude_node_from_tint(popup_id);
		}

		// 붙기 전에 정해 둔 선택지가 있으면 행을 만든다.
		if !self.option_list.is_empty() {
			let option_list = self.option_list.clone();
			self.set_option_list(node_tree, &option_list);
		}
	}

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
	// 시험 장면. (800 x 600 뿌리, (100, 50) 에 놓은 드롭다운, 선택지 셋)
	//==============================================================================
	fn create_dropdown_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
		let dropdown_id = UIDropdown::create(&mut node_tree, root_id, UIDropdownOptions::default());
		node_tree[dropdown_id].set_local_position(Vector2::create(100.0, 50.0));
		node_tree.with_component_mut::<UIDropdown, _>(dropdown_id, |dropdown, tree| {
			dropdown.set_option_list(tree, &["Dark", "Light", "System"]);
		});
		(node_tree, dropdown_id)
	}

	//==============================================================================
	// 노드 가운데를 누르고 뗀다.
	//==============================================================================
	fn click_node(node_tree: &mut NodeTree, engine: &Engine, node_id: NodeId) {
		let content_size = node_tree[node_id].get_content_size();
		let center = node_tree.convert_node_position_to_view_position(node_id, content_size.multiply_scalar(0.5));
		node_tree.dispatch_touch(node_id, engine, TouchKind::Press, center);
		node_tree.dispatch_touch(node_id, engine, TouchKind::Release, center);
	}

	//==============================================================================
	// 드롭다운 반환.
	//==============================================================================
	fn get_dropdown(node_tree: &mut NodeTree, dropdown_id: NodeId) -> &mut UIDropdown {
		node_tree[dropdown_id].get_component_mut::<UIDropdown>().expect("dropdown")
	}

	//==============================================================================
	// 붙으면 호스트가 꾸며지고, 선택지 행과 머리 글이 생긴다.
	//==============================================================================
	#[test]
	fn option_list_builds_rows() {
		let (mut node_tree, dropdown_id) = create_dropdown_tree();
		assert_eq!(node_tree[dropdown_id].get_content_size(), Vector2::create(180.0, 30.0));
		assert!(node_tree[dropdown_id].is_interactable());
		assert_eq!(node_tree[dropdown_id].get_component::<UILabel>().expect("header").get_text(), "Dark");
		let popup_id = get_dropdown(&mut node_tree, dropdown_id).get_popup_node().expect("popup");
		assert!(!node_tree[popup_id].is_active());
		assert_eq!(node_tree[popup_id].get_content_size(), Vector2::create(180.0, 92.0));
		let row_ids = get_dropdown(&mut node_tree, dropdown_id).get_option_nodes().to_vec();
		assert_eq!(row_ids.len(), 3);
		assert_eq!(node_tree[row_ids[2]].get_local_position(), Vector2::create(4.0, 60.0));
		assert_eq!(node_tree[row_ids[0]].get_component::<Paint>().expect("paint").get_color(), UIDropdownOptions::default().highlight_color);
		assert_eq!(node_tree[row_ids[1]].get_component::<Paint>().expect("paint").get_color(), Color::transparent());
	}

	//==============================================================================
	// 머리를 누르면 펼쳐져 목록이 뿌리로 옮겨 가고, 행을 누르면 고르고 접히며 바뀜 이벤트가 한 번 온다.
	//==============================================================================
	#[test]
	fn open_select_and_close() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, dropdown_id) = create_dropdown_tree();
		let root_id = node_tree.get_root_id();
		let changed_count = Rc::new(Cell::new(0));
		let changed_counter = changed_count.clone();
		get_dropdown(&mut node_tree, dropdown_id).set_changed_event(Box::new(move |dropdown: &mut UIDropdown, _context: &mut ComponentContext| {
			assert_eq!(dropdown.get_selected_text(), "System");
			changed_counter.set(changed_counter.get() + 1);
		}));
		let popup_id = get_dropdown(&mut node_tree, dropdown_id).get_popup_node().expect("popup");

		click_node(&mut node_tree, &engine, dropdown_id);
		assert!(get_dropdown(&mut node_tree, dropdown_id).is_open());
		assert!(node_tree[popup_id].is_active());
		assert_eq!(node_tree.get_parent_id(popup_id), Some(root_id));
		assert_eq!(node_tree[popup_id].get_local_position(), Vector2::create(100.0, 84.0));

		// 펼친 목록의 행은 맨 위에 그려지므로 그 자리의 맨 위 노드다.
		let row_ids = get_dropdown(&mut node_tree, dropdown_id).get_option_nodes().to_vec();
		let row_center = node_tree.convert_node_position_to_view_position(row_ids[2], Vector2::create(86.0, 14.0));
		assert_eq!(node_tree.find_topmost_interactable_node(row_center), Some(row_ids[2]));

		click_node(&mut node_tree, &engine, row_ids[2]);
		assert!(!get_dropdown(&mut node_tree, dropdown_id).is_open());
		assert_eq!(get_dropdown(&mut node_tree, dropdown_id).get_selected_index(), 2);
		assert_eq!(changed_count.get(), 1);
		assert!(!node_tree[popup_id].is_active());
		assert_eq!(node_tree.get_parent_id(popup_id), Some(dropdown_id));
		assert_eq!(node_tree[popup_id].get_local_position(), Vector2::create(0.0, 34.0));
		assert_eq!(node_tree[dropdown_id].get_component::<UILabel>().expect("header").get_text(), "System");
		assert_eq!(node_tree[row_ids[2]].get_component::<Paint>().expect("paint").get_color(), UIDropdownOptions::default().highlight_color);
		assert_eq!(node_tree[row_ids[0]].get_component::<Paint>().expect("paint").get_color(), Color::transparent());

		// 머리 단추가 tick 을 돌아도 행 강조는 그대로다.
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[row_ids[2]].get_component::<Paint>().expect("paint").get_color(), UIDropdownOptions::default().highlight_color);

		// 다시 펼쳐 행 단추들이 tick 을 돌아도 강조는 그대로다.
		click_node(&mut node_tree, &engine, dropdown_id);
		node_tree.tick(&engine, 0.016);
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[row_ids[2]].get_component::<Paint>().expect("paint").get_color(), UIDropdownOptions::default().highlight_color);
		assert_eq!(node_tree[row_ids[0]].get_component::<Paint>().expect("paint").get_color(), Color::transparent());

		// 같은 행을 다시 고르면 이벤트가 오지 않는다.
		click_node(&mut node_tree, &engine, row_ids[2]);
		assert_eq!(changed_count.get(), 1);
	}

	//==============================================================================
	// set_selected_index 는 범위로 자르고 이벤트 없이 머리 글만 고친다.
	//==============================================================================
	#[test]
	fn set_selected_index_clamps_without_event() {
		let (mut node_tree, dropdown_id) = create_dropdown_tree();
		node_tree.with_component_mut::<UIDropdown, _>(dropdown_id, |dropdown, tree| {
			dropdown.set_selected_index(tree, 7);
		});
		assert_eq!(get_dropdown(&mut node_tree, dropdown_id).get_selected_index(), 2);
		assert_eq!(get_dropdown(&mut node_tree, dropdown_id).get_selected_text(), "System");
		assert_eq!(node_tree[dropdown_id].get_component::<UILabel>().expect("header").get_text(), "System");
	}
}
