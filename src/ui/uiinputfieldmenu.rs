//==============================================================================
//! 입력 필드의 터치 빠른 메뉴. (데스크탑, 러스트 판에만 있는 파일)
//! - 원본에서 setUseDirectTouch 로 숨긴 DOM 입력 칸을 직접 누르게 한 필드는 손가락으로 길게 누르면 크롬이 낱말을 고르고 빠른 메뉴를 띄운다.
//!   데스크탑에는 브라우저가 없어 엔진이 크롬 데스크탑 터치처럼 빠른 메뉴(UIInputFieldQuickMenu)를 그린다. 고른 것이 없고 클립보드도 비면 엔진 문맥 메뉴다.
//! - 오른쪽 단추 문맥 메뉴는 두 엔진 모두 엔진이 그린 같은 메뉴(잘라내기, 복사, 붙여넣기, 모두 선택)다. (ui/uiinputfield.rs, 원본 showContextMenu)
//!   원본도 직접 누름 필드에서 브라우저 편집 칸 메뉴를 막고 그 메뉴를 띄운다.
//! - 글은 브라우저 화면 언어 자리인 시스템 언어의 크롬 번역이다. (core/browseruitext.rs)
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::blurshadow::{views_shadow_blur_to_sigma, BlurredRoundRectShadow};
use crate::core::browseruitext::{read_browser_ui_text, BrowserUiText};
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::core::textstringtexturecache::measure_text_without_graphic;
use crate::ui::uiinputfield::UIInputField;

/// 빠른 메뉴 색. (크롬 기본 밝은 팔레트)
const COLOR_MENU_BACKGROUND: &str = "#ffffff";
const COLOR_MENU_BORDER: &str = "#e1e3e1";
const COLOR_TEXT: &str = "#1f1f1f";

//==============================================================================
/// 터치 빠른 메뉴. (크롬 데스크탑 aura 의 TouchSelectionMenuViews, 손가락으로 길게 눌러 고른 뒤 뜬다)
//==============================================================================
const QUICK_MENU_BUTTON_MINIMUM_HEIGHT: f32 = 40.0;
const QUICK_MENU_BUTTON_PADDING_X: f32 = 16.0;
const QUICK_MENU_CORNER_RADIUS: f32 = 8.0;
const QUICK_MENU_ANCHOR_PADDING: f32 = 8.0;
const QUICK_MENU_SEPARATOR_WIDTH: f32 = 1.0;
const QUICK_MENU_FONT_STRING: &str = "14px system-ui";
const COLOR_QUICK_MENU_SEPARATOR: &str = "#d3e3fd";
const COLOR_QUICK_MENU_PRESSED_BACKGROUND: Color = Color::create(31.0 / 255.0, 31.0 / 255.0, 31.0 / 255.0, 0.08);

//==============================================================================
/// 빠른 메뉴 단추가 하는 일.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuickMenuCommand {
	Cut,
	Copy,
	Paste,
	/// "..." (보통 문맥 메뉴)
	Ellipsis,
}

//==============================================================================
/// 빠른 메뉴에 켤 명령. (크롬 IsCommandIdEnabled: 잘라내기는 선택이 있고 편집할 수 있을 때, 복사는 선택이 있을 때, 붙여넣기는 클립보드에 글이 있을 때. "..." 는 늘)
//==============================================================================
pub(crate) fn build_quick_menu_command_list(has_selection: bool, can_paste: bool) -> Vec<QuickMenuCommand> {
	let mut command_list: Vec<QuickMenuCommand> = Vec::new();
	if has_selection {
		command_list.push(QuickMenuCommand::Cut);
		command_list.push(QuickMenuCommand::Copy);
	}
	if can_paste {
		command_list.push(QuickMenuCommand::Paste);
	}
	command_list.push(QuickMenuCommand::Ellipsis);
	command_list
}

//==============================================================================
/// 빠른 메뉴를 띄울 수 있는지. (크롬 IsQuickMenuAvailable: "..." 말고 켜진 명령이 하나라도 있는지)
//==============================================================================
pub(crate) fn is_quick_menu_available(has_selection: bool, can_paste: bool) -> bool {
	has_selection || can_paste
}

//==============================================================================
/// 빠른 메뉴 단추 하나. (메뉴 안 왼쪽 x 와 폭)
//==============================================================================
struct QuickMenuButton {
	command: QuickMenuCommand,
	label: String,
	left: f32,
	width: f32,
}

//==============================================================================
/// 빠른 메뉴 컴포넌트. (루트의 마지막 자식 노드, 창 안쪽 논리 픽셀 크기)
//==============================================================================
pub(crate) struct UIInputFieldQuickMenu {
	component_base: ComponentBase,
	owner_node_id: NodeId,
	button_list: Vec<QuickMenuButton>,
	size: Vector2,
	pressed_button_index: Option<usize>,
	/// 그림자 두 겹의 구운 그림.
	shadow_list: [BlurredRoundRectShadow; 2],
}

impl UIInputFieldQuickMenu {
	//==============================================================================
	/// 만들기. (단추 폭을 잰다)
	//==============================================================================
	fn new(owner_node_id: NodeId, command_list: &[QuickMenuCommand]) -> UIInputFieldQuickMenu {
		let mut button_list: Vec<QuickMenuButton> = Vec::new();
		let mut left = 0.0;
		for command in command_list {
			let label = match command {
				QuickMenuCommand::Cut => {
					read_browser_ui_text(BrowserUiText::TouchCut)
				}
				QuickMenuCommand::Copy => {
					read_browser_ui_text(BrowserUiText::TouchCopy)
				}
				QuickMenuCommand::Paste => {
					read_browser_ui_text(BrowserUiText::TouchPaste)
				}
				QuickMenuCommand::Ellipsis => {
					String::from("...")
				}
			};
			let text_width = match measure_text_without_graphic(QUICK_MENU_FONT_STRING, &label) {
				Some(text_metrics) => {
					text_metrics.width
				}
				None => {
					label.chars().count() as f32 * 8.0
				}
			};
			let width = (text_width + QUICK_MENU_BUTTON_PADDING_X * 2.0).ceil();
			button_list.push(QuickMenuButton {
				command: *command,
				label,
				left,
				width,
			});
			left += width;
			// 단추 뒤 구분선. ("..." 뒤에는 없다)
			if *command != QuickMenuCommand::Ellipsis {
				left += QUICK_MENU_SEPARATOR_WIDTH;
			}
		}
		UIInputFieldQuickMenu {
			component_base: ComponentBase::new(),
			owner_node_id,
			button_list,
			size: Vector2::create(left, QUICK_MENU_BUTTON_MINIMUM_HEIGHT),
			pressed_button_index: None,
			shadow_list: Default::default(),
		}
	}

	//==============================================================================
	/// 메뉴 안 자리의 단추. (없으면 None)
	//==============================================================================
	fn find_button_index(&self, local_position: Vector2) -> Option<usize> {
		if local_position.y < 0.0 || local_position.y > self.size.y {
			return None;
		}
		self.button_list.iter().position(|button| local_position.x >= button.left && local_position.x < button.left + button.width)
	}

	//==============================================================================
	/// 닫기.
	//==============================================================================
	fn close(&self, context: &mut ComponentContext) {
		let node_id = context.get_node_id();
		context.get_node_tree_mut().destroy_node(node_id);
	}
}

impl Component for UIInputFieldQuickMenu {
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
	/// 컴포넌트 종류 이름 반환. ("UIInputFieldQuickMenu")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIInputFieldQuickMenu"
	}

	//==============================================================================
	/// 트리에 붙을 때. (노드를 interactable 로 켠다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		node_tree[node_id].set_interactable(true);
	}

	//==============================================================================
	/// 갱신. (필드가 포커스를 잃으면 닫고, 누른 단추에서 떼면 닫은 뒤 필드에 그 명령을 부탁한다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let is_owner_ready = match context.get_node_tree().get_node(self.owner_node_id).and_then(|owner_node| owner_node.get_component::<UIInputField>()) {
			Some(owner_field) => {
				owner_field.is_focused() && owner_field.is_attached()
			}
			None => {
				false
			}
		};
		if !is_owner_ready {
			self.close(context);
			return;
		}
		let input_manager = context.get_engine().get_input_manager();
		let view_input_position = input_manager.get_view_input_position();
		let is_touch_pressed = input_manager.is_touch_pressed();
		let is_touch_released = input_manager.is_touch_released();
		let node_id = context.get_node_id();
		let button_index = context.get_node_tree().convert_view_position_to_node_position(node_id, view_input_position).and_then(|local_position| self.find_button_index(local_position));
		if is_touch_pressed {
			self.pressed_button_index = button_index;
		}
		if is_touch_released {
			let pressed_button_index = self.pressed_button_index.take();
			if let Some(button_index) = button_index.filter(|button_index| Some(*button_index) == pressed_button_index) {
				let command = self.button_list[button_index].command;
				let owner_node_id = self.owner_node_id;
				self.close(context);
				if let Some(owner_field) = context.get_node_tree_mut().get_node_mut(owner_node_id).and_then(|owner_node| owner_node.get_component_mut::<UIInputField>()) {
					owner_field.request_quick_menu_command(command);
				}
			}
		}
	}

	//==============================================================================
	/// 그리기. (그림자, 둥근 판, 누른 단추 바탕, 단추 글과 나눔 선, 테두리)
	//==============================================================================
	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		graphic.push_state();
		let menu_rect = Rect::create(0.0, 0.0, self.size.x, self.size.y);
		// 표준 그림자. (BubbleBorder STANDARD_SHADOW 에 높이를 주지 않은 두 겹, 질문 창과 같은 값을 views 시그마의 가우스로)
		for (shadow_index, (shadow_blur, shadow_alpha)) in [(8, 0x33u8), (12, 0x1Au8)].iter().enumerate() {
			let shadow_color = Color::create(60.0 / 255.0, 64.0 / 255.0, 67.0 / 255.0, *shadow_alpha as f32 / 255.0);
			self.shadow_list[shadow_index].draw(graphic, menu_rect, QUICK_MENU_CORNER_RADIUS, Vector2::create(0.0, 2.0), views_shadow_blur_to_sigma(*shadow_blur), shadow_color);
		}
		graphic.set_fill_color(COLOR_MENU_BACKGROUND);
		graphic.draw_round_rect(menu_rect, QUICK_MENU_CORNER_RADIUS);
		graphic.set_font_string(QUICK_MENU_FONT_STRING);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_text_align(TextAlign::Center);
		for (button_index, button) in self.button_list.iter().enumerate() {
			if self.pressed_button_index == Some(button_index) {
				graphic.set_fill_color(COLOR_QUICK_MENU_PRESSED_BACKGROUND);
				graphic.draw_rect(Rect::create(button.left, 0.0, button.width, self.size.y));
			}
			graphic.set_fill_color(COLOR_TEXT);
			graphic.draw_fill_text(&button.label, button.left + button.width * 0.5, self.size.y * 0.5);
			if button.command != QuickMenuCommand::Ellipsis {
				graphic.set_fill_color(COLOR_QUICK_MENU_SEPARATOR);
				graphic.draw_rect(Rect::create(button.left + button.width, 0.0, QUICK_MENU_SEPARATOR_WIDTH, self.size.y));
			}
		}
		graphic.set_stroke_color(COLOR_MENU_BORDER);
		graphic.draw_stroke_round_rect(Rect::create(0.5, 0.5, menu_rect.size.x - 1.0, menu_rect.size.y - 1.0), QUICK_MENU_CORNER_RADIUS - 0.5, 1.0);
		graphic.pop_state();
	}
}

//==============================================================================
/// 빠른 메뉴 노드 만들기. (anchor_rect 는 뷰 좌표의 선택 사각형, handle_size 는 창 픽셀의 손잡이 크기)
/// - 크롬 ShowMenu: 메뉴 폭이 선택 폭 - 손잡이 폭보다 크면 아래를 손잡이 높이만큼 늘리고, 사방 8 을 키워 그 위 가운데에 메뉴 아래 가운데를 맞춘다.
///   위로 넘쳐 아래로 뒤집으면 더 많이 보일 때 뒤집고, 가로는 창 안으로 민다.
//==============================================================================
pub(crate) fn create_quick_menu_node(node_tree: &mut NodeTree, owner_node_id: NodeId, anchor_rect: Rect, handle_size: Vector2, view_scale: Vector2, view_size: Vector2, command_list: &[QuickMenuCommand]) -> NodeId {
	let menu = UIInputFieldQuickMenu::new(owner_node_id, command_list);
	let inverse_scale = Vector2::create(1.0 / view_scale.x.max(1e-4), 1.0 / view_scale.y.max(1e-4));
	let menu_size = Vector2::create(menu.size.x * inverse_scale.x, menu.size.y * inverse_scale.y);
	let mut adjusted_rect = anchor_rect;
	if menu_size.x > anchor_rect.size.x - handle_size.x * inverse_scale.x {
		adjusted_rect.size.y += handle_size.y * inverse_scale.y;
	}
	let padding = Vector2::create(QUICK_MENU_ANCHOR_PADDING * inverse_scale.x, QUICK_MENU_ANCHOR_PADDING * inverse_scale.y);
	adjusted_rect = Rect::create(adjusted_rect.position.x - padding.x, adjusted_rect.position.y - padding.y, adjusted_rect.size.x + padding.x * 2.0, adjusted_rect.size.y + padding.y * 2.0);
	let center_x = adjusted_rect.position.x + adjusted_rect.size.x * 0.5;
	let mut menu_y = adjusted_rect.position.y - menu_size.y;
	if menu_y < 0.0 {
		// 아래로 뒤집으면 더 많이 보일 때만.
		let below_y = adjusted_rect.position.y + adjusted_rect.size.y;
		let visible_above = (menu_y + menu_size.y).min(view_size.y) - menu_y.max(0.0);
		let visible_below = (below_y + menu_size.y).min(view_size.y) - below_y.max(0.0);
		if visible_below > visible_above {
			menu_y = below_y;
		}
	}
	let menu_x = (center_x - menu_size.x * 0.5).min(view_size.x - menu_size.x).max(0.0);
	let menu_y = menu_y.min(view_size.y - menu_size.y).max(0.0);
	let root_id = node_tree.get_root_id();
	let menu_view_position = Vector2::create(menu_x, menu_y);
	let local_position = node_tree.convert_view_position_to_node_position(root_id, menu_view_position).unwrap_or(menu_view_position);
	let menu_id = node_tree.create_child(root_id, "uiInputFieldQuickMenu");
	{
		let menu_node = &mut node_tree[menu_id];
		menu_node.set_pivot(Pivot::TOP_LEFT);
		menu_node.set_anchor(Vector2::zero());
		menu_node.set_local_position(local_position);
		menu_node.set_content_size(menu.size);
	}
	node_tree.set_scale(menu_id, inverse_scale);
	node_tree.add_component(menu_id, menu);
	menu_id
}
