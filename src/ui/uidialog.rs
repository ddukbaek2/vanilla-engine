//==============================================================================
//! 확인 대화 상자. (원본 ui/uidialog.js 의 UIDialog)
//! - 어둡게 가리는 막 + 가운데 카드 + 확인 / 취소 단추를 show 한 번으로 띄운다. 등장 / 퇴장은 PopupMotion(되튕김 확대 / 수축)으로 움직인다.
//! - 막을 누르면 취소로 처리한다. on_cancel 을 넘기지 않으면 취소 단추를 감추고 확인 단추만 넓게 둔 알림이 된다.
//! - 씬 뿌리의 맨 위(마지막 자식)에 두고 쓴다. 부모 content_size 를 막 크기로 쓴다. (없으면 960 x 640)
//! - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼 호스트를 꾸미고
//!   (왼쪽 위 피봇, 비활성) 막, 카드, 메시지, 단추 노드를 자식으로 만든다. UIDialog::create(node_tree, parent_id, options) 가 new + add_child 자리다.
//! - 사용:
//!   ```text
//!     let dialog_id = UIDialog::create(node_tree, root_id, UIDialogOptions::default());
//!     node_tree.with_component_mut::<UIDialog, _>(dialog_id, |dialog, tree| {
//!         dialog.show(tree, "정말 삭제할까요?", UIDialogShowOptions { on_confirm: Some(Box::new(|dialog, context| { ... })), on_cancel: Some(Box::new(|_, _| {})), ..UIDialogShowOptions::default() });
//!     });
//!   ```
//! - 원본과 다른 것:
//!   - show 는 노드를 바꾸므로 트리를 받는다. 씬 코드는 node_tree.with_component_mut 로, 콜백에서는 context.get_node_tree_mut() 로 부른다.
//!   - 확인 / 취소 처리기는 대화 상자와 그 호스트의 문맥을 받는 한 번짜리 함수(FnOnce)다. dismiss 는 처리기에 문맥을 넘기려고 문맥을 받는다.
//!   ```text
//!     문맥이 없는 씬 코드에서는 원본에도 있는 request_dismiss 를 부르면 다음 tick 에 dismiss 한다.
//!   ```
//!   - 원본은 PopupMotion 닫힘 알림에서 set_active(false) 한다. 러스트 판은 tick 에서 보이던 것이 닫힘이 되면(즉시 전환이면 dismiss 에서) 끈다.
//!   - 원본 tick 은 자식 tick 다음에 전환기를 돌린다. 러스트 판은 컴포넌트가 자식보다 먼저 돈다. (보이는 차이는 없다)
//!   - 전환 시간은 PopupMotion 그대로다. (등장 0.32 초, 퇴장 0.22 초. Tween 은 쓰지 않는다)
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::popupmotion::PopupMotion;
use crate::ui::uibutton::UIButton;
use crate::ui::uilabel::UILabel;

/// 부모 크기를 모를 때 쓰는 영역. (원본과 같다)
const DEFAULT_AREA_WIDTH: f32 = 960.0;
const DEFAULT_AREA_HEIGHT: f32 = 640.0;

/// 단추 높이와 카드 안쪽 여백. (원본과 같다)
const BUTTON_HEIGHT: f32 = 52.0;
const PANEL_PADDING: f32 = 28.0;
const BUTTON_GAP: f32 = 16.0;
const BUTTON_BOTTOM_OFFSET: f32 = 76.0;

//==============================================================================
/// 대화 상자 설정. (원본 options = { width = 420, height = 240, confirm_text = "확인", cancel_text = "취소", panel_color?, accent_color?, text_color? })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct UIDialogOptions {
	pub width: f32,
	pub height: f32,
	pub confirm_text: String,
	pub cancel_text: String,
	/// 카드 바탕색.
	pub panel_color: Color,
	/// 확인 단추 색.
	pub accent_color: Color,
	pub text_color: Color,
}

impl Default for UIDialogOptions {
	//==============================================================================
	/// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIDialogOptions {
		UIDialogOptions {
			width: 420.0,
			height: 240.0,
			confirm_text: String::from("확인"),
			cancel_text: String::from("취소"),
			panel_color: Color::create(0.13, 0.15, 0.2, 1.0),
			accent_color: Color::create(0.23, 0.51, 0.96, 1.0),
			text_color: Color::create(0.96, 0.97, 1.0, 1.0),
		}
	}
}

//==============================================================================
/// 확인 / 취소 처리기. (대화 상자와 그 호스트의 문맥을 받는다. 한 번만 불린다)
//==============================================================================
pub type UIDialogHandler = Box<dyn FnOnce(&mut UIDialog, &mut ComponentContext)>;

//==============================================================================
/// 열기 설정. (원본 show 의 options = { on_confirm?, on_cancel?, confirm_text?, cancel_text? })
//==============================================================================
#[derive(Default)]
pub struct UIDialogShowOptions {
	pub on_confirm: Option<UIDialogHandler>,
	/// 없으면 취소 단추를 감춘 알림이 된다.
	pub on_cancel: Option<UIDialogHandler>,
	pub confirm_text: Option<String>,
	pub cancel_text: Option<String>,
}

//==============================================================================
/// 확인 대화 상자.
//==============================================================================
pub struct UIDialog {
	component_base: ComponentBase,
	options: UIDialogOptions,
	node_id: Option<NodeId>,
	popup_motion: PopupMotion,
	overlay_node_id: Option<NodeId>,
	panel_node_id: Option<NodeId>,
	message_node_id: Option<NodeId>,
	confirm_node_id: Option<NodeId>,
	cancel_node_id: Option<NodeId>,
	confirm_handler: Option<UIDialogHandler>,
	cancel_handler: Option<UIDialogHandler>,
	pending_dismiss: Option<bool>,
}

#[crate::reflect]
impl UIDialog {
	//==============================================================================
	/// 생성. (기본 설정)
	//==============================================================================
	pub fn new() -> UIDialog {
		UIDialog::create_with_options(UIDialogOptions::default())
	}

	//==============================================================================
	/// 설정으로 생성. (노드에는 node_tree.add_component 로 붙인다)
	//==============================================================================
	pub fn create_with_options(options: UIDialogOptions) -> UIDialog {
		UIDialog {
			component_base: ComponentBase::new(),
			options,
			node_id: None,
			popup_motion: PopupMotion::new(),
			overlay_node_id: None,
			panel_node_id: None,
			message_node_id: None,
			confirm_node_id: None,
			cancel_node_id: None,
			confirm_handler: None,
			cancel_handler: None,
			pending_dismiss: None,
		}
	}

	//==============================================================================
	/// 노드 만들기. (parent_id 밑에 "UIDialog" 노드를 만들어 붙인다. 원본 new UIDialog(options) + add_child)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, parent_id: NodeId, options: UIDialogOptions) -> NodeId {
		let dialog_id = node_tree.create_child(parent_id, "UIDialog");
		node_tree.add_component(dialog_id, UIDialog::create_with_options(options));
		dialog_id
	}

	//==============================================================================
	/// 단추 노드 만들기. (둥근 Paint + UIButton + UILabel)
	//==============================================================================
	fn create_button_node(node_tree: &mut NodeTree, parent_id: NodeId, node_name: &str, label_text: &str, fill_color: Color, text_color: Color, button_width: f32) -> NodeId {
		let button_id = node_tree.create_child(parent_id, node_name);
		let button_node = &mut node_tree[button_id];
		button_node.set_pivot(Pivot::TOP_LEFT);
		button_node.set_anchor(Pivot::TOP_LEFT);
		button_node.set_content_size(Vector2::create(button_width, BUTTON_HEIGHT));
		button_node.set_interactable(true);
		let paint = button_node.add_component(Paint::create_with_color(fill_color));
		paint.set_round_size(14.0);
		node_tree.add_component(button_id, UIButton::new());
		if let Some(label) = node_tree.add_component(button_id, UILabel::new()) {
			label.set_text(label_text);
			label.set_font_size(16.0);
			label.set_text_color(text_color);
		}
		button_id
	}

	//==============================================================================
	/// 단추 클릭이 대화 상자를 닫게 한다. (is_confirmed 에 따라 확인 / 취소)
	//==============================================================================
	fn bind_dismiss_button(node_tree: &mut NodeTree, button_id: NodeId, dialog_id: NodeId, is_confirmed: bool) {
		if let Some(button) = node_tree[button_id].get_component_mut::<UIButton>() {
			button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				context.with_component_mut::<UIDialog, _>(dialog_id, |dialog, dialog_context| {
					dialog.dismiss(dialog_context, is_confirmed);
				});
			}));
		}
	}

	//==============================================================================
	/// 노드의 UILabel 글 바꾸기.
	//==============================================================================
	fn set_node_label_text(node_tree: &mut NodeTree, node_id: Option<NodeId>, text: &str) {
		if let Some(label) = node_id.and_then(|node_id| node_tree.get_node_mut(node_id)).and_then(|node| node.get_component_mut::<UILabel>()) {
			label.set_text(text);
		}
	}

	//==============================================================================
	/// 대화 상자 열기. (on_cancel 이 없으면 취소 단추를 감추고 확인 단추를 가운데로 넓힌다)
	//==============================================================================
	pub fn show(&mut self, node_tree: &mut NodeTree, message_text: &str, options: UIDialogShowOptions) {
		let (host_id, panel_id) = match (self.node_id, self.panel_node_id) {
			(Some(host_id), Some(panel_id)) => {
				(host_id, panel_id)
			}
			_ => {
				return;
			}
		};
		if !node_tree.contains(host_id) || !node_tree.contains(panel_id) {
			return;
		}
		UIDialog::set_node_label_text(node_tree, self.message_node_id, message_text);
		let has_cancel = options.on_cancel.is_some();
		self.confirm_handler = options.on_confirm;
		self.cancel_handler = options.on_cancel;
		self.pending_dismiss = None;
		if let Some(confirm_text) = options.confirm_text.as_deref() {
			UIDialog::set_node_label_text(node_tree, self.confirm_node_id, confirm_text);
		}
		if let Some(cancel_text) = options.cancel_text.as_deref() {
			UIDialog::set_node_label_text(node_tree, self.cancel_node_id, cancel_text);
		}

		let panel_size = node_tree[panel_id].get_content_size();
		let button_width = (panel_size.x - PANEL_PADDING * 2.0 - BUTTON_GAP) * 0.5;
		if let Some(cancel_node) = self.cancel_node_id.and_then(|cancel_id| node_tree.get_node_mut(cancel_id)) {
			cancel_node.set_active(has_cancel);
		}
		if let Some(confirm_node) = self.confirm_node_id.and_then(|confirm_id| node_tree.get_node_mut(confirm_id)) {
			if has_cancel {
				confirm_node.set_content_size(Vector2::create(button_width, BUTTON_HEIGHT));
				confirm_node.set_local_position(Vector2::create(PANEL_PADDING + button_width + BUTTON_GAP, panel_size.y - BUTTON_BOTTOM_OFFSET));
			}
			else {
				confirm_node.set_content_size(Vector2::create(panel_size.x - PANEL_PADDING * 2.0, BUTTON_HEIGHT));
				confirm_node.set_local_position(Vector2::create(PANEL_PADDING, panel_size.y - BUTTON_BOTTOM_OFFSET));
			}
		}

		node_tree[host_id].set_active(true);
		self.update_layout_to_parent(node_tree);
		self.popup_motion.open();
		self.apply_motion(node_tree);
	}

	//==============================================================================
	/// 닫기. (다 열린 뒤에만. is_confirmed 에 따라 확인 / 취소 처리기를 부른다. 문맥은 대화 상자 호스트의 것)
	//==============================================================================
	pub fn dismiss(&mut self, context: &mut ComponentContext, is_confirmed: bool) {
		if !self.popup_motion.is_interactive() {
			return;
		}
		let handler = if is_confirmed {
			self.confirm_handler.take()
		}
		else {
			self.cancel_handler.take()
		};
		self.confirm_handler = None;
		self.cancel_handler = None;
		self.popup_motion.close();
		// 즉시 전환이면 바로 닫혔으므로 호스트를 끈다. (원본 닫힘 알림)
		if !self.popup_motion.is_visible() {
			if let Some(host_node) = self.node_id.and_then(|host_id| context.get_node_tree_mut().get_node_mut(host_id)) {
				host_node.set_active(false);
			}
		}
		if let Some(handler) = handler {
			handler(self, context);
		}
	}

	//==============================================================================
	/// 다음 tick 에 닫기. (문맥이 없는 씬 코드용)
	//==============================================================================
	pub fn request_dismiss(&mut self, is_confirmed: bool) {
		self.pending_dismiss = Some(is_confirmed);
	}

	//==============================================================================
	/// 전환기 산출값을 노드에 건다. (막 투명도, 카드 배율)
	//==============================================================================
	fn apply_motion(&self, node_tree: &mut NodeTree) {
		let dim_ratio = self.popup_motion.get_dim_ratio();
		if let Some(overlay_node) = self.overlay_node_id.and_then(|overlay_id| node_tree.get_node_mut(overlay_id)) {
			overlay_node.set_local_opacity(dim_ratio);
		}
		let panel_scale = self.popup_motion.get_scale();
		if let Some(panel_node) = self.panel_node_id.and_then(|panel_id| node_tree.get_node_mut(panel_id)) {
			panel_node.set_local_scale(Vector2::create(panel_scale, panel_scale));
		}
	}

	//==============================================================================
	/// 부모 크기에 맞춰 막과 카드 자리를 고친다.
	//==============================================================================
	fn update_layout_to_parent(&self, node_tree: &mut NodeTree) {
		let host_id = match self.node_id {
			Some(host_id) => {
				host_id
			}
			None => {
				return;
			}
		};
		let mut area_width = DEFAULT_AREA_WIDTH;
		let mut area_height = DEFAULT_AREA_HEIGHT;
		if let Some(parent_node) = node_tree.get_parent_id(host_id).and_then(|parent_id| node_tree.get_node(parent_id)) {
			let parent_size = parent_node.get_content_size();
			if parent_size.x > 0.0 && parent_size.y > 0.0 {
				area_width = parent_size.x;
				area_height = parent_size.y;
			}
		}
		let area_size = Vector2::create(area_width, area_height);
		node_tree[host_id].set_content_size(area_size);
		if let Some(overlay_node) = self.overlay_node_id.and_then(|overlay_id| node_tree.get_node_mut(overlay_id)) {
			overlay_node.set_content_size(area_size);
			overlay_node.set_local_position(Vector2::zero());
		}
		if let Some(panel_node) = self.panel_node_id.and_then(|panel_id| node_tree.get_node_mut(panel_id)) {
			panel_node.set_local_position(Vector2::create(area_width * 0.5, area_height * 0.5));
		}
	}

	//==============================================================================
	/// 보이는 중인지. (등장 / 열림 / 퇴장 중이면 참)
	//==============================================================================
	pub fn is_showing(&self) -> bool {
		self.popup_motion.is_visible()
	}

	//==============================================================================
	/// 즉시 전환 설정. (연출 없는 UI 톤)
	//==============================================================================
	pub fn set_instant_enabled(&mut self, is_instant_enabled: bool) {
		self.popup_motion.set_instant_enabled(is_instant_enabled);
	}

	//==============================================================================
	/// 전환기 반환.
	//==============================================================================
	pub fn get_popup_motion(&self) -> &PopupMotion {
		&self.popup_motion
	}

	//==============================================================================
	/// 막 노드 핸들.
	//==============================================================================
	pub fn get_overlay_node(&self) -> Option<NodeId> {
		self.overlay_node_id
	}

	//==============================================================================
	/// 카드 노드 핸들.
	//==============================================================================
	pub fn get_panel_node(&self) -> Option<NodeId> {
		self.panel_node_id
	}

	//==============================================================================
	/// 확인 단추 노드 핸들.
	//==============================================================================
	pub fn get_confirm_node(&self) -> Option<NodeId> {
		self.confirm_node_id
	}

	//==============================================================================
	/// 취소 단추 노드 핸들.
	//==============================================================================
	pub fn get_cancel_node(&self) -> Option<NodeId> {
		self.cancel_node_id
	}
}

impl Default for UIDialog {
	fn default() -> UIDialog {
		UIDialog::new()
	}
}

#[crate::reflect]
impl Component for UIDialog {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UIDialog"
	}

	//==============================================================================
	/// 트리에 붙음. (원본 생성자: 호스트 꾸미기, 막, 카드, 메시지, 단추)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.node_id = Some(node_id);
		let panel_width = self.options.width;
		let panel_height = self.options.height;
		let text_color = self.options.text_color;
		let host_node = &mut node_tree[node_id];
		host_node.set_pivot(Pivot::TOP_LEFT);
		host_node.set_anchor(Pivot::TOP_LEFT);
		host_node.set_active(false);

		// 어둡게 가리는 막. (누르면 취소)
		let overlay_id = node_tree.create_child(node_id, "DialogOverlay");
		let overlay_node = &mut node_tree[overlay_id];
		overlay_node.set_pivot(Pivot::TOP_LEFT);
		overlay_node.set_anchor(Pivot::TOP_LEFT);
		overlay_node.set_interactable(true);
		overlay_node.add_component(Paint::create_with_color(Color::create(0.0, 0.0, 0.0, 0.55)));
		if let Some(overlay_button) = node_tree.add_component(overlay_id, UIButton::new()) {
			overlay_button.set_pressed_tint_color(Color::transparent());
		}
		UIDialog::bind_dismiss_button(node_tree, overlay_id, node_id, false);
		self.overlay_node_id = Some(overlay_id);

		// 카드.
		let panel_id = node_tree.create_child(node_id, "DialogPanel");
		let panel_node = &mut node_tree[panel_id];
		panel_node.set_pivot(Pivot::MIDDLE_CENTER);
		panel_node.set_anchor(Pivot::TOP_LEFT);
		panel_node.set_content_size(Vector2::create(panel_width, panel_height));
		let panel_paint = panel_node.add_component(Paint::create_with_color(self.options.panel_color));
		panel_paint.set_round_size(18.0);
		self.panel_node_id = Some(panel_id);

		// 메시지.
		let message_id = node_tree.create_child(panel_id, "DialogMessage");
		let message_node = &mut node_tree[message_id];
		message_node.set_pivot(Pivot::TOP_LEFT);
		message_node.set_anchor(Pivot::TOP_LEFT);
		message_node.set_content_size(Vector2::create(panel_width - PANEL_PADDING * 2.0, panel_height - 120.0));
		message_node.set_local_position(Vector2::create(PANEL_PADDING, 24.0));
		if let Some(message_label) = node_tree.add_component(message_id, UILabel::new()) {
			message_label.set_text("");
			message_label.set_font_size(17.0);
			message_label.set_text_color(text_color);
			message_label.set_word_wrap_width(panel_width - PANEL_PADDING * 2.0);
		}
		self.message_node_id = Some(message_id);

		// 단추.
		let button_width = (panel_width - PANEL_PADDING * 2.0 - BUTTON_GAP) * 0.5;
		let cancel_id = UIDialog::create_button_node(node_tree, panel_id, "DialogCancel", &self.options.cancel_text, Color::create(0.22, 0.25, 0.32, 1.0), text_color, button_width);
		node_tree[cancel_id].set_local_position(Vector2::create(PANEL_PADDING, panel_height - BUTTON_BOTTOM_OFFSET));
		let confirm_id = UIDialog::create_button_node(node_tree, panel_id, "DialogConfirm", &self.options.confirm_text, self.options.accent_color, text_color, button_width);
		node_tree[confirm_id].set_local_position(Vector2::create(PANEL_PADDING + button_width + BUTTON_GAP, panel_height - BUTTON_BOTTOM_OFFSET));
		UIDialog::bind_dismiss_button(node_tree, cancel_id, node_id, false);
		UIDialog::bind_dismiss_button(node_tree, confirm_id, node_id, true);
		self.cancel_node_id = Some(cancel_id);
		self.confirm_node_id = Some(confirm_id);
	}

	//==============================================================================
	/// 갱신. (미뤄 둔 닫기 → 전환기 → 노드에 반영 → 다 닫혔으면 호스트를 끈다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.node_id = Some(context.get_node_id());
		if let Some(is_confirmed) = self.pending_dismiss.take() {
			self.dismiss(context, is_confirmed);
		}
		let was_visible = self.popup_motion.is_visible();
		self.popup_motion.tick(time_delta);
		self.apply_motion(context.get_node_tree_mut());
		if was_visible && !self.popup_motion.is_visible() {
			context.get_node_mut().set_active(false);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use crate::ui::popupmotion::PopupMotionState;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	/// 시험 장면. (800 x 600 뿌리 밑의 대화 상자)
	//==============================================================================
	fn create_dialog_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
		let dialog_id = UIDialog::create(&mut node_tree, root_id, UIDialogOptions::default());
		(node_tree, dialog_id)
	}

	//==============================================================================
	/// 셈하는 처리기.
	//==============================================================================
	fn create_counting_handler(counter: &Rc<Cell<u32>>) -> UIDialogHandler {
		let counter = counter.clone();
		Box::new(move |_dialog: &mut UIDialog, _context: &mut ComponentContext| {
			counter.set(counter.get() + 1);
		})
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
	/// 대화 상자 반환.
	//==============================================================================
	fn get_dialog(node_tree: &mut NodeTree, dialog_id: NodeId) -> &mut UIDialog {
		node_tree[dialog_id].get_component_mut::<UIDialog>().expect("dialog")
	}

	//==============================================================================
	/// 열기 → 다 열리기 전 누름은 무시 → 확인 → 퇴장 뒤 호스트가 꺼진다.
	//==============================================================================
	#[test]
	fn show_confirm_and_close() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, dialog_id) = create_dialog_tree();
		assert!(!node_tree[dialog_id].is_active());
		let confirm_count = Rc::new(Cell::new(0));
		let cancel_count = Rc::new(Cell::new(0));
		let show_options = UIDialogShowOptions {
			on_confirm: Some(create_counting_handler(&confirm_count)),
			on_cancel: Some(create_counting_handler(&cancel_count)),
			confirm_text: Some(String::from("삭제")),
			..UIDialogShowOptions::default()
		};
		node_tree.with_component_mut::<UIDialog, _>(dialog_id, |dialog, tree| {
			dialog.show(tree, "정말 삭제할까요?", show_options);
		});
		let (overlay_id, panel_id, confirm_id, cancel_id) = {
			let dialog = get_dialog(&mut node_tree, dialog_id);
			(dialog.get_overlay_node().expect("overlay"), dialog.get_panel_node().expect("panel"), dialog.get_confirm_node().expect("confirm"), dialog.get_cancel_node().expect("cancel"))
		};
		assert!(node_tree[dialog_id].is_active());
		assert!(get_dialog(&mut node_tree, dialog_id).is_showing());
		assert_eq!(node_tree[dialog_id].get_content_size(), Vector2::create(800.0, 600.0));
		assert_eq!(node_tree[overlay_id].get_content_size(), Vector2::create(800.0, 600.0));
		assert_eq!(node_tree[panel_id].get_local_position(), Vector2::create(400.0, 300.0));
		assert_eq!(node_tree[overlay_id].get_local_opacity(), 0.0);
		assert!((node_tree[panel_id].get_local_scale().x - 0.45).abs() < 0.0001);
		assert!(node_tree[cancel_id].is_active());
		assert_eq!(node_tree[confirm_id].get_component::<UILabel>().expect("label").get_text(), "삭제");

		// 다 열리기 전에는 눌러도 닫히지 않는다.
		node_tree.tick(&engine, 0.1);
		click_node(&mut node_tree, &engine, confirm_id);
		assert_eq!(confirm_count.get(), 0);
		node_tree.tick(&engine, 0.25);
		assert_eq!(get_dialog(&mut node_tree, dialog_id).get_popup_motion().get_state(), PopupMotionState::Open);
		assert_eq!(node_tree[overlay_id].get_local_opacity(), 1.0);
		assert_eq!(node_tree[panel_id].get_local_scale(), Vector2::create(1.0, 1.0));

		click_node(&mut node_tree, &engine, confirm_id);
		assert_eq!(confirm_count.get(), 1);
		assert_eq!(cancel_count.get(), 0);
		assert_eq!(get_dialog(&mut node_tree, dialog_id).get_popup_motion().get_state(), PopupMotionState::Closing);
		assert!(node_tree[dialog_id].is_active());
		node_tree.tick(&engine, 0.25);
		assert!(!get_dialog(&mut node_tree, dialog_id).is_showing());
		assert!(!node_tree[dialog_id].is_active());
	}

	//==============================================================================
	/// 막을 누르면 취소. 취소 처리기가 없으면 취소 단추를 감추고 확인 단추를 넓힌다.
	//==============================================================================
	#[test]
	fn overlay_cancels_and_alert_layout() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, dialog_id) = create_dialog_tree();
		let cancel_count = Rc::new(Cell::new(0));
		let show_options = UIDialogShowOptions {
			on_cancel: Some(create_counting_handler(&cancel_count)),
			..UIDialogShowOptions::default()
		};
		node_tree.with_component_mut::<UIDialog, _>(dialog_id, |dialog, tree| {
			dialog.show(tree, "가운데를 벗어나 누르면 취소", show_options);
		});
		node_tree.tick(&engine, 0.4);
		let overlay_id = get_dialog(&mut node_tree, dialog_id).get_overlay_node().expect("overlay");
		node_tree.dispatch_touch(overlay_id, &engine, TouchKind::Press, Vector2::create(20.0, 20.0));
		node_tree.dispatch_touch(overlay_id, &engine, TouchKind::Release, Vector2::create(20.0, 20.0));
		assert_eq!(cancel_count.get(), 1);
		node_tree.tick(&engine, 0.3);
		assert!(!node_tree[dialog_id].is_active());

		// 알림 꼴: 취소 단추 없음, 확인 단추가 넓다.
		node_tree.with_component_mut::<UIDialog, _>(dialog_id, |dialog, tree| {
			dialog.show(tree, "저장했어요", UIDialogShowOptions::default());
		});
		let (confirm_id, cancel_id) = {
			let dialog = get_dialog(&mut node_tree, dialog_id);
			(dialog.get_confirm_node().expect("confirm"), dialog.get_cancel_node().expect("cancel"))
		};
		assert!(!node_tree[cancel_id].is_active());
		assert_eq!(node_tree[confirm_id].get_content_size(), Vector2::create(364.0, 52.0));
		assert_eq!(node_tree[confirm_id].get_local_position(), Vector2::create(28.0, 164.0));
	}

	//==============================================================================
	/// 즉시 전환: 열면 바로 열리고, 닫으면 바로 호스트가 꺼진다. request_dismiss 는 다음 tick 에 닫는다.
	//==============================================================================
	#[test]
	fn instant_mode_and_request_dismiss() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, dialog_id) = create_dialog_tree();
		let confirm_count = Rc::new(Cell::new(0));
		get_dialog(&mut node_tree, dialog_id).set_instant_enabled(true);
		let show_options = UIDialogShowOptions {
			on_confirm: Some(create_counting_handler(&confirm_count)),
			..UIDialogShowOptions::default()
		};
		node_tree.with_component_mut::<UIDialog, _>(dialog_id, |dialog, tree| {
			dialog.show(tree, "바로 열림", show_options);
		});
		assert!(get_dialog(&mut node_tree, dialog_id).get_popup_motion().is_interactive());
		get_dialog(&mut node_tree, dialog_id).request_dismiss(true);
		assert!(node_tree[dialog_id].is_active());
		node_tree.tick(&engine, 0.016);
		assert_eq!(confirm_count.get(), 1);
		assert!(!node_tree[dialog_id].is_active());
		assert!(!get_dialog(&mut node_tree, dialog_id).is_showing());
	}
}
