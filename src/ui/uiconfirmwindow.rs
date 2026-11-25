//==============================================================================
// 확인 창. (vanilla.js 의 ui/uiconfirmwindow.js, UIConfirmWindow)
// - 제목, 묻는 글, 취소와 확인 두 단추. UIWindow 라 UIWindowLayer 로 띄운다.
// - 묻는 글은 판의 너비에서 줄을 바꾸고 줄 수에 맞춰 판의 세로가 정해진다. 글이 길어 영역의 세로를 넘으면 글 칸만 스크롤되고 두 단추는 늘 보인다.
// - 단추는 왼쪽이 취소, 오른쪽이 확인(강조)이다. 오른쪽에서 왼쪽 언어(set_right_to_left)면 차례를 뒤집는다.
// - 확인을 누르면 확인 알림 뒤 닫힌다. 취소, 바깥 누르기, close_top_window, close_all_windows 로 닫히면 취소 알림이다.
// - 사용:
//     let window_id = UIConfirmWindow::create(node_tree, layer_id, "quit", UIConfirmWindowOptions { title: String::from("나가기"), message: String::from("정말 나갈까요?"), ..UIConfirmWindowOptions::default() });
//     if let Some(confirm_window) = node_tree[window_id].get_component_mut::<UIWindow>().and_then(|window| window.get_behavior_mut::<UIConfirmWindow>()) {
//         confirm_window.set_confirm_event(Some(Box::new(move || { leave_requested.set(true); })));
//     }
//     UIWindowLayer::open_window_in_tree(node_tree, layer_id, window_id);
// - 원본과 다른 것:
//   - 원본은 UIWindow 를 상속한다. 러스트 판은 UIWindow 의 덮어쓸 자리(UIWindowBehavior)를 구현한 값이다. (UIWindow::get_behavior_mut 으로 꺼낸다)
//   - 글 줄 수는 Graphic 의 글꼴로 재야 해서 레이아웃 때 바로 알 수 없다. 처음에는 한 줄로 놓고, 글이 한 번 그려져 줄을 나누면
//     (Text::get_measured_line_count) 다음 tick 에 다시 놓는다. 여는 움직임의 첫 프레임이라 보이는 차이는 거의 없다.
//   - 제목과 묻는 글은 UILabel 대신 Text 를 바로 붙인다. (줄 수를 읽으려고) 글의 오른쪽에서 왼쪽 방향은 러스트 Text 에 없어 단추 차례만 뒤집는다.
//   - 알림은 인자 없는 Box<dyn FnMut()> 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::text::Text;
use crate::core::graphic::TextBaseline;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uibutton::UIButton;
use crate::ui::uilabel::UILabel;
use crate::ui::uiscrollview::UIScrollView;
use crate::ui::uiwindow::{UIWindow, UIWindowBehavior, UIWindowEvent, UIWindowParts};
use std::any::Any;

// 기본 모양. (판 안쪽 여백, 줄 사이, 제목 높이와 글 크기, 글 크기와 줄 간격, 단추 높이와 사이, 둥근 정도)
const CONFIRM_PADDING: f32 = 24.0;
const CONFIRM_ROW_GAP: f32 = 16.0;
const CONFIRM_TITLE_HEIGHT: f32 = 32.0;
const CONFIRM_TITLE_FONT_SIZE: f32 = 22.0;
const CONFIRM_MESSAGE_FONT_SIZE: f32 = 17.0;
const CONFIRM_LINE_SPACING: f32 = 1.35;
const CONFIRM_BUTTON_HEIGHT: f32 = 52.0;
const CONFIRM_BUTTON_GAP: f32 = 12.0;
const CONFIRM_BUTTON_ROUND_SIZE: f32 = 12.0;
// 글 칸이 영역에 비해 아무리 좁아도 지키는 세로. (넘는 것은 스크롤)
const CONFIRM_MINIMUM_MESSAGE_HEIGHT: f32 = 40.0;

//==============================================================================
// 확인 창 설정. (원본 options = { title = "", message = "", cancelText = "취소", confirmText = "확인" })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct UIConfirmWindowOptions {
	// 비면 제목 줄을 없앤다.
	pub title: String,
	pub message: String,
	pub cancel_text: String,
	pub confirm_text: String,
}

impl Default for UIConfirmWindowOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIConfirmWindowOptions {
		UIConfirmWindowOptions {
			title: String::new(),
			message: String::new(),
			cancel_text: String::from("취소"),
			confirm_text: String::from("확인"),
		}
	}
}

//==============================================================================
// 확인 창.
//==============================================================================
pub struct UIConfirmWindow {
	options: UIConfirmWindowOptions,
	title_id: Option<NodeId>,
	message_scroll_id: Option<NodeId>,
	message_id: Option<NodeId>,
	cancel_button_id: Option<NodeId>,
	confirm_button_id: Option<NodeId>,
	confirm_event: Option<UIWindowEvent>,
	cancel_event: Option<UIWindowEvent>,
	// 확인으로 닫히는지. (아니면 닫힐 때 취소 알림)
	is_confirmed: bool,
	is_right_to_left: bool,
	// 글 줄 수를 아직 못 재 한 줄로 놓았는지.
	is_measure_pending: bool,
}

impl UIConfirmWindow {
	//==============================================================================
	// 생성. (UIWindow::create 에 넘긴다, 보통은 UIConfirmWindow::create)
	//==============================================================================
	pub fn create_with_options(options: UIConfirmWindowOptions) -> UIConfirmWindow {
		UIConfirmWindow {
			options,
			title_id: None,
			message_scroll_id: None,
			message_id: None,
			cancel_button_id: None,
			confirm_button_id: None,
			confirm_event: None,
			cancel_event: None,
			is_confirmed: false,
			is_right_to_left: false,
			is_measure_pending: false,
		}
	}

	//==============================================================================
	// 확인 창 노드 만들기. (layer_id 창 층 노드 밑에, 띄우는 것은 UIWindowLayer::open_window)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, layer_id: NodeId, name: &str, options: UIConfirmWindowOptions) -> NodeId {
		UIWindow::create(node_tree, layer_id, name, Box::new(UIConfirmWindow::create_with_options(options)))
	}

	//==============================================================================
	// 확인. (확인 알림을 부르고 닫는다, 떠 있지 않으면 아무 일도 없다. 확인 단추가 부른다)
	//==============================================================================
	pub fn confirm_in_tree(node_tree: &mut NodeTree, window_id: NodeId) {
		let is_confirmed = node_tree.with_component_mut::<UIWindow, _>(window_id, |window, _tree| {
			if window.get_window_layer_id().is_none() {
				return false;
			}
			match window.get_behavior_mut::<UIConfirmWindow>() {
				Some(confirm_window) => {
					confirm_window.mark_confirmed()
				}
				None => {
					false
				}
			}
		}).unwrap_or(false);
		if is_confirmed {
			UIWindow::close_in_tree(node_tree, window_id);
		}
	}

	//==============================================================================
	// 확인으로 적고 확인 알림 부르기. (이미 확인했으면 false)
	//==============================================================================
	fn mark_confirmed(&mut self) -> bool {
		if self.is_confirmed {
			return false;
		}
		self.is_confirmed = true;
		if let Some(confirm_event) = self.confirm_event.as_mut() {
			confirm_event();
		}
		true
	}

	//==============================================================================
	// 확인 알림 설정.
	//==============================================================================
	pub fn set_confirm_event(&mut self, confirm_event: Option<UIWindowEvent>) {
		self.confirm_event = confirm_event;
	}

	//==============================================================================
	// 취소 알림 설정. (취소 단추, 바깥 누르기, 창 층이 닫을 때)
	//==============================================================================
	pub fn set_cancel_event(&mut self, cancel_event: Option<UIWindowEvent>) {
		self.cancel_event = cancel_event;
	}

	//==============================================================================
	// 오른쪽에서 왼쪽 언어인지 설정. (참이면 취소가 오른쪽, 확인이 왼쪽, 다음 놓을 때부터)
	//==============================================================================
	pub fn set_right_to_left(&mut self, is_right_to_left: bool) {
		self.is_right_to_left = is_right_to_left;
	}

	//==============================================================================
	// 오른쪽에서 왼쪽 언어인지.
	//==============================================================================
	pub fn is_right_to_left(&self) -> bool {
		self.is_right_to_left
	}

	//==============================================================================
	// 제목 바꾸기. (빈 글이면 제목 줄을 없앤다, 떠 있으면 UIWindow::refresh_layout 으로 다시 놓는다)
	//==============================================================================
	pub fn set_title(&mut self, node_tree: &mut NodeTree, title: &str) {
		self.options.title = title.to_string();
		if let Some(title_node) = self.title_id.and_then(|title_id| node_tree.get_node_mut(title_id)) {
			title_node.set_active(!title.is_empty());
			if let Some(title_text) = title_node.get_component_mut::<Text>() {
				title_text.set_text(title);
			}
		}
	}

	//==============================================================================
	// 묻는 글 바꾸기. (떠 있으면 UIWindow::refresh_layout 으로 다시 놓는다)
	//==============================================================================
	pub fn set_message(&mut self, node_tree: &mut NodeTree, message: &str) {
		self.options.message = message.to_string();
		if let Some(message_text) = self.message_id.and_then(|message_id| node_tree.get_node_mut(message_id)).and_then(|message_node| message_node.get_component_mut::<Text>()) {
			message_text.set_text(message);
		}
	}

	//==============================================================================
	// 제목 노드. (Text)
	//==============================================================================
	pub fn get_title_node(&self) -> Option<NodeId> {
		self.title_id
	}

	//==============================================================================
	// 묻는 글 노드. (Text)
	//==============================================================================
	pub fn get_message_node(&self) -> Option<NodeId> {
		self.message_id
	}

	//==============================================================================
	// 묻는 글의 스크롤 칸 노드. (UIScrollView)
	//==============================================================================
	pub fn get_message_scroll_node(&self) -> Option<NodeId> {
		self.message_scroll_id
	}

	//==============================================================================
	// 취소 단추 노드. (Paint, UIButton, UILabel)
	//==============================================================================
	pub fn get_cancel_button_node(&self) -> Option<NodeId> {
		self.cancel_button_id
	}

	//==============================================================================
	// 확인 단추 노드. (Paint, UIButton, UILabel)
	//==============================================================================
	pub fn get_confirm_button_node(&self) -> Option<NodeId> {
		self.confirm_button_id
	}

	//==============================================================================
	// 왼쪽 위 피봇의 빈 노드 만들기.
	//==============================================================================
	fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, node_name: &str) -> NodeId {
		let node_id = node_tree.create_child(parent_id, node_name);
		node_tree[node_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[node_id].set_anchor(Pivot::TOP_LEFT);
		node_id
	}

	//==============================================================================
	// 글 노드 만들기. (Text, 흰 글)
	//==============================================================================
	fn create_text_node(node_tree: &mut NodeTree, parent_id: NodeId, node_name: &str, text: &str, font_size: f32) -> NodeId {
		let text_id = UIConfirmWindow::create_box_node(node_tree, parent_id, node_name);
		let text_component = node_tree[text_id].add_component(Text::create_with_text(text, font_size));
		text_component.set_text_color(Color::create(0.96, 0.97, 1.0, 1.0));
		text_id
	}

	//==============================================================================
	// 단추 노드 만들기. (둥근 Paint + UIButton + UILabel)
	//==============================================================================
	fn create_button_node(node_tree: &mut NodeTree, parent_id: NodeId, node_name: &str, label_text: &str, fill_color: Color) -> NodeId {
		let button_id = UIConfirmWindow::create_box_node(node_tree, parent_id, node_name);
		let button_node = &mut node_tree[button_id];
		button_node.set_content_size(Vector2::create(1.0, CONFIRM_BUTTON_HEIGHT));
		button_node.set_interactable(true);
		let paint = button_node.add_component(Paint::create_with_color(fill_color));
		paint.set_round_size(CONFIRM_BUTTON_ROUND_SIZE);
		node_tree.add_component(button_id, UIButton::new());
		if let Some(label) = node_tree.add_component(button_id, UILabel::new()) {
			label.set_text(label_text);
			label.set_font_size(CONFIRM_MESSAGE_FONT_SIZE);
			label.set_text_color(Color::create(0.96, 0.97, 1.0, 1.0));
		}
		button_id
	}

	//==============================================================================
	// 단추 클릭 걸기.
	//==============================================================================
	fn bind_button(node_tree: &mut NodeTree, button_id: NodeId, window_id: NodeId, is_confirm: bool) {
		if let Some(button) = node_tree[button_id].get_component_mut::<UIButton>() {
			button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				if is_confirm {
					UIConfirmWindow::confirm_in_tree(context.get_node_tree_mut(), window_id);
				}
				else {
					UIWindow::close_in_tree(context.get_node_tree_mut(), window_id);
				}
			}));
		}
	}

	//==============================================================================
	// 그 너비로 줄을 바꾼 묻는 글의 세로. (아직 줄을 못 쟀으면 한 줄과 false)
	//==============================================================================
	fn measure_message_height(&self, node_tree: &mut NodeTree, text_width: f32) -> (f32, bool) {
		let message_text = match self.message_id.and_then(|message_id| node_tree.get_node_mut(message_id)).and_then(|message_node| message_node.get_component_mut::<Text>()) {
			Some(message_text) => {
				message_text
			}
			None => {
				return (1.0, true);
			}
		};
		message_text.set_word_wrap_width(text_width);
		let line_height = message_text.get_font_size() * message_text.get_line_spacing();
		match message_text.get_measured_line_count() {
			Some(line_count) => {
				((line_count as f32 * line_height).ceil(), true)
			}
			None => {
				(line_height.ceil(), false)
			}
		}
	}
}

impl UIWindowBehavior for UIConfirmWindow {
	fn as_any(&self) -> &dyn Any {
		self
	}

	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}

	//==============================================================================
	// 내용 만들기. (제목, 묻는 글 스크롤 칸, 두 단추)
	//==============================================================================
	fn build(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts) {
		let window_name = node_tree[parts.window_id].get_name().to_string();
		let panel_id = parts.panel_id;
		let title_id = UIConfirmWindow::create_text_node(node_tree, panel_id, &format!("{}Title", window_name), &self.options.title, CONFIRM_TITLE_FONT_SIZE);
		if let Some(title_text) = node_tree[title_id].get_component_mut::<Text>() {
			title_text.set_bold(true);
		}
		node_tree[title_id].set_active(!self.options.title.is_empty());
		self.title_id = Some(title_id);

		let message_scroll_id = UIConfirmWindow::create_box_node(node_tree, panel_id, &format!("{}MessageScroll", window_name));
		let mut message_parent_id = message_scroll_id;
		if let Some(scroll_view) = node_tree.add_component(message_scroll_id, UIScrollView::new()) {
			scroll_view.set_horizontal(false);
			if let Some(content_id) = scroll_view.get_content() {
				message_parent_id = content_id;
			}
		}
		self.message_scroll_id = Some(message_scroll_id);
		let message_id = UIConfirmWindow::create_text_node(node_tree, message_parent_id, &format!("{}Message", window_name), &self.options.message, CONFIRM_MESSAGE_FONT_SIZE);
		if let Some(message_text) = node_tree[message_id].get_component_mut::<Text>() {
			message_text.set_text_baseline(TextBaseline::Top);
			message_text.set_line_spacing(CONFIRM_LINE_SPACING);
		}
		self.message_id = Some(message_id);

		let cancel_button_id = UIConfirmWindow::create_button_node(node_tree, panel_id, &format!("{}Cancel", window_name), &self.options.cancel_text, Color::create(0.22, 0.25, 0.32, 1.0));
		UIConfirmWindow::bind_button(node_tree, cancel_button_id, parts.window_id, false);
		self.cancel_button_id = Some(cancel_button_id);
		let confirm_button_id = UIConfirmWindow::create_button_node(node_tree, panel_id, &format!("{}Confirm", window_name), &self.options.confirm_text, Color::create(0.23, 0.51, 0.96, 1.0));
		UIConfirmWindow::bind_button(node_tree, confirm_button_id, parts.window_id, true);
		self.confirm_button_id = Some(confirm_button_id);
	}

	//==============================================================================
	// 내용 놓기. (제목 → 글 스크롤 칸 → 두 단추, 판의 세로를 돌려준다)
	//==============================================================================
	fn layout_content(&mut self, node_tree: &mut NodeTree, parts: &UIWindowParts, panel_width: f32) -> f32 {
		let inner_width = (panel_width - CONFIRM_PADDING * 2.0).max(1.0);
		let mut top = CONFIRM_PADDING;
		if let Some(title_node) = self.title_id.and_then(|title_id| node_tree.get_node_mut(title_id)) {
			if title_node.is_active() {
				title_node.set_local_position(Vector2::create(CONFIRM_PADDING, top));
				title_node.set_content_size(Vector2::create(inner_width, CONFIRM_TITLE_HEIGHT));
				top += CONFIRM_TITLE_HEIGHT + CONFIRM_ROW_GAP;
			}
		}
		// 글 칸은 영역의 세로에서 위와 단추 줄을 뺀 만큼까지, 넘는 글은 스크롤한다.
		let buttons_height = CONFIRM_ROW_GAP + CONFIRM_BUTTON_HEIGHT + CONFIRM_PADDING;
		let available_height = (parts.view_height - parts.margin * 2.0 - top - buttons_height).max(CONFIRM_MINIMUM_MESSAGE_HEIGHT);
		let (measured_height, is_measured) = self.measure_message_height(node_tree, inner_width);
		self.is_measure_pending = !is_measured;
		let message_height = measured_height.max(1.0);
		let scroll_height = message_height.min(available_height);
		if let Some(message_node) = self.message_id.and_then(|message_id| node_tree.get_node_mut(message_id)) {
			message_node.set_local_position(Vector2::zero());
			message_node.set_content_size(Vector2::create(inner_width, message_height));
		}
		if let Some(message_scroll_id) = self.message_scroll_id {
			if let Some(message_scroll_node) = node_tree.get_node_mut(message_scroll_id) {
				message_scroll_node.set_local_position(Vector2::create(CONFIRM_PADDING, top));
				message_scroll_node.set_content_size(Vector2::create(inner_width, scroll_height));
			}
			if let Some(scroll_view) = node_tree.get_node_mut(message_scroll_id).and_then(|message_scroll_node| message_scroll_node.get_component_mut::<UIScrollView>()) {
				scroll_view.set_scroll_content_size(Vector2::create(inner_width, message_height));
				scroll_view.set_scroll_offset(Vector2::zero());
			}
		}
		top += scroll_height + CONFIRM_ROW_GAP;
		// 단추. (오른쪽에서 왼쪽 언어면 차례를 뒤집는다)
		let button_width = ((inner_width - CONFIRM_BUTTON_GAP) * 0.5).max(1.0);
		let left_button_left = CONFIRM_PADDING;
		let right_button_left = CONFIRM_PADDING + button_width + CONFIRM_BUTTON_GAP;
		let (cancel_left, confirm_left) = if self.is_right_to_left {
			(right_button_left, left_button_left)
		}
		else {
			(left_button_left, right_button_left)
		};
		for (button_id, button_left) in [(self.cancel_button_id, cancel_left), (self.confirm_button_id, confirm_left)] {
			if let Some(button_node) = button_id.and_then(|button_id| node_tree.get_node_mut(button_id)) {
				button_node.set_local_position(Vector2::create(button_left, top));
				button_node.set_content_size(Vector2::create(button_width, CONFIRM_BUTTON_HEIGHT));
			}
		}
		top + CONFIRM_BUTTON_HEIGHT + CONFIRM_PADDING
	}

	//==============================================================================
	// 창 층에서 내려감. (확인이 아니면 취소 알림)
	//==============================================================================
	fn handle_closed(&mut self) {
		let is_confirmed = self.is_confirmed;
		self.is_confirmed = false;
		if !is_confirmed {
			if let Some(cancel_event) = self.cancel_event.as_mut() {
				cancel_event();
			}
		}
	}

	//==============================================================================
	// 글 줄 수를 못 재 한 줄로 놓았는데 이제 쟀으면 다시 놓는다.
	//==============================================================================
	fn is_layout_pending(&self, node_tree: &NodeTree, _parts: &UIWindowParts) -> bool {
		if !self.is_measure_pending {
			return false;
		}
		self.message_id.and_then(|message_id| node_tree.get_node(message_id)).and_then(|message_node| message_node.get_component::<Text>()).and_then(|message_text| message_text.get_measured_line_count()).is_some()
	}
}
