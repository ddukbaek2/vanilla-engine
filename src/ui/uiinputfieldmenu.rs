//==============================================================================
//! 입력 필드의 브라우저 문맥 메뉴. (데스크탑, 러스트 판에만 있는 파일)
//! - 원본에서 setUseDirectTouch 로 숨긴 DOM 입력 칸을 직접 누르게 한 필드는 오른쪽 클릭이 그 입력 칸에 가서 크롬이 편집 칸 문맥 메뉴를 띄운다.
//!   (그 밖의 필드는 원본 엔진이 그린 잘라내기, 복사, 붙여넣기 메뉴다, ui/uiinputfield.rs)
//!   데스크탑에는 브라우저가 없어 엔진이 같은 항목과 단축키 글로 메뉴를 그리고 같은 동작을 한다.
//! - 항목과 차례는 크로미엄 render_view_context_menu.cc 와 그 단위 시험 EditableTextMenu_EmptySelectionOrder 대로다.
//!   - 윈도우: 그림 이모티콘 Win+마침표 / ─ / 실행 취소 Ctrl+Z, 다시 실행 Ctrl+Shift+Z / ─ / 잘라내기 Ctrl+X, 복사 Ctrl+C, 붙여넣기 Ctrl+V,
//!     일반 텍스트로 붙여넣기 Ctrl+Shift+V, 전체 선택 Ctrl+A / ─ / 쓰기 방향 ▸ / ─ / 검사
//!   - 리눅스: 이모티콘 창이 없어 그림 이모티콘과 그 구분선이 없다.
//!   - 맥: 그림 이모티콘 및 기호, 붙여넣고 스타일 일치시킴, 모두 선택이고 단축키 글이 없다. (맥 크롬 문맥 메뉴는 단축키를 보이지 않는다)
//!   - 맥이 아니면 선택이 있을 때 실행 취소, 다시 실행과 그 구분선이 빠진다. (ITEM_GROUP_PRINT)
//!   - 켜짐: 실행 취소와 다시 실행은 되돌릴 것이 있을 때, 잘라내기와 복사는 선택이 있을 때, 붙여넣기 둘은 클립보드에 글이 있을 때,
//!     전체 선택은 칸이 비어 있지 않을 때(블링크 EnabledSelectAll), 쓰기 방향의 기본값은 늘 꺼짐.
//! - 글은 브라우저 화면 언어 자리인 시스템 언어의 크롬 번역이다. (core/browseruitext.rs)
//! - 동작: 그림 이모티콘은 운영체제 이모티콘 창(맥 문자 팔레트, 윈도우 Win+.), 일반 텍스트로 붙여넣기는 붙여넣기(글만 받는 칸이라 같다),
//!   쓰기 방향은 칸의 방향 값을 바꾼다(원본도 숨긴 입력 칸의 dir 만 바뀌고 엔진이 그린 글은 그대로다), 검사는 엔진 개발자 도구를 열어 이 필드의 노드를 고른다.
//! - 원본 메뉴에 있으나 브라우저 기능이라 페이지에 결과가 없는 것은 넣지 않는다: 읽기 모드로 열기(브라우저 옆 패널), 맞춤법 검사와 언어 설정(브라우저 사전 설정,
//!   필드가 spellcheck=false 라 페이지에 보이는 것이 없다), 선택이 있을 때의 검색, 인쇄, 번역, 맥의 찾기와 음성(운영체제 서비스), 비밀번호와 Gemini 항목.
//! - 모양은 크롬 메뉴를 본뜬 값이다. (윈도우와 리눅스 Views 메뉴, 맥 NSMenu) 창 안쪽 논리 픽셀 크기로 그리려고 노드 배율로 뷰 배율을 되돌린다.
//! - 손가락으로 길게 눌러 고르면 크롬 데스크탑 터치처럼 빠른 메뉴(UIInputFieldQuickMenu)가 뜬다. 고른 것이 없고 클립보드도 비면 위 문맥 메뉴다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::browseruitext::{read_browser_ui_text, BrowserUiText};
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::inputmanager::KeyCode;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::core::textstringtexturecache::measure_text_without_graphic;
use crate::ui::uiinputfield::{UIInputField, UIInputFieldMenuAction, UIInputFieldWritingDirection};
use crate::ui::uiview::is_view_position_inside_node;

/// 모양. (맥은 NSMenu, 그 밖은 크롬 Views 메뉴를 본뜬 값)
#[cfg(target_os = "macos")]
const ITEM_HEIGHT: f32 = 22.0;
#[cfg(not(target_os = "macos"))]
const ITEM_HEIGHT: f32 = 28.0;
#[cfg(target_os = "macos")]
const MENU_PADDING_Y: f32 = 5.0;
#[cfg(not(target_os = "macos"))]
const MENU_PADDING_Y: f32 = 8.0;
#[cfg(target_os = "macos")]
const SEPARATOR_HEIGHT: f32 = 11.0;
#[cfg(not(target_os = "macos"))]
const SEPARATOR_HEIGHT: f32 = 9.0;
#[cfg(target_os = "macos")]
const MENU_CORNER_RADIUS: f32 = 6.0;
#[cfg(not(target_os = "macos"))]
const MENU_CORNER_RADIUS: f32 = 8.0;
const CHECK_COLUMN_WIDTH: f32 = 24.0;
const LABEL_PADDING_RIGHT: f32 = 16.0;
const ACCELERATOR_GAP: f32 = 32.0;
const SUBMENU_ARROW_WIDTH: f32 = 16.0;
const MENU_MINIMUM_WIDTH: f32 = 160.0;
const MENU_FONT_STRING: &str = "13px system-ui";
const COLOR_MENU_BACKGROUND: &str = "#ffffff";
const COLOR_MENU_BORDER: &str = "#e1e3e1";
const COLOR_TEXT: &str = "#1f1f1f";
const COLOR_ACCELERATOR: &str = "#5f6368";
const COLOR_DISABLED: Color = Color::create(31.0 / 255.0, 31.0 / 255.0, 31.0 / 255.0, 0.38);
#[cfg(target_os = "macos")]
const COLOR_HOVER_BACKGROUND: &str = "#0a64d6";
#[cfg(not(target_os = "macos"))]
const COLOR_HOVER_BACKGROUND: &str = "#ebebeb";

//==============================================================================
/// 메뉴 항목 하나.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrowserMenuItem {
	pub action: UIInputFieldMenuAction,
	pub label: String,
	pub accelerator: String,
	pub is_enabled: bool,
	pub is_checked: bool,
}

//==============================================================================
/// 메뉴 줄. (항목, 구분선, 하위 메뉴)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BrowserMenuEntry {
	Item(BrowserMenuItem),
	Separator,
	Submenu {
		label: String,
		item_list: Vec<BrowserMenuItem>,
	},
}

//==============================================================================
/// 메뉴를 지을 때의 필드 상태.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BrowserMenuFieldState {
	pub can_undo: bool,
	pub can_redo: bool,
	pub has_selection: bool,
	pub has_clipboard_text: bool,
	pub is_empty: bool,
	pub writing_direction: UIInputFieldWritingDirection,
}

//==============================================================================
/// 메뉴 항목 하나 짓기.
//==============================================================================
fn create_item(action: UIInputFieldMenuAction, text: BrowserUiText, accelerator: String, is_enabled: bool) -> BrowserMenuItem {
	BrowserMenuItem {
		action,
		label: read_browser_ui_text(text),
		accelerator,
		is_enabled,
		is_checked: false,
	}
}

//==============================================================================
/// 단축키 글. (맥은 문맥 메뉴에 보이지 않는다. 그 밖은 크롬 accelerator.cc 처럼 수식 키를 + 로 잇는다)
//==============================================================================
fn create_accelerator(is_shift: bool, key_text: &str) -> String {
	if cfg!(target_os = "macos") {
		return String::new();
	}
	if is_shift {
		return format!("Ctrl+Shift+{}", key_text);
	}
	format!("Ctrl+{}", key_text)
}

//==============================================================================
/// 크롬 편집 칸 메뉴 짓기. (플랫폼별 차례와 켜짐, 머리 주석)
//==============================================================================
pub(crate) fn build_browser_menu(field_state: &BrowserMenuFieldState) -> Vec<BrowserMenuEntry> {
	let is_mac = cfg!(target_os = "macos");
	let mut entry_list: Vec<BrowserMenuEntry> = Vec::new();
	if crate::platform::desktop::emojipanel::is_emoji_panel_supported() {
		let (emoji_text, emoji_accelerator) = if is_mac {
			(BrowserUiText::EmojiMac, String::new())
		}
		else {
			(BrowserUiText::Emoji, format!("{}+{}", read_browser_ui_text(BrowserUiText::WindowsKey), read_browser_ui_text(BrowserUiText::PeriodKey)))
		};
		entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::Emoji, emoji_text, emoji_accelerator, true)));
		entry_list.push(BrowserMenuEntry::Separator);
	}
	if is_mac || !field_state.has_selection {
		entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::Undo, BrowserUiText::Undo, create_accelerator(false, "Z"), field_state.can_undo)));
		entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::Redo, BrowserUiText::Redo, create_accelerator(true, "Z"), field_state.can_redo)));
		entry_list.push(BrowserMenuEntry::Separator);
	}
	entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::Cut, BrowserUiText::Cut, create_accelerator(false, "X"), field_state.has_selection)));
	entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::Copy, BrowserUiText::Copy, create_accelerator(false, "C"), field_state.has_selection)));
	let (paste_text, paste_plain_text, select_all_text) = if is_mac {
		(BrowserUiText::PasteMac, BrowserUiText::PasteMatchStyle, BrowserUiText::SelectAllMac)
	}
	else {
		(BrowserUiText::Paste, BrowserUiText::PastePlainText, BrowserUiText::SelectAll)
	};
	entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::Paste, paste_text, create_accelerator(false, "V"), field_state.has_clipboard_text)));
	entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::PasteAsPlainText, paste_plain_text, create_accelerator(true, "V"), field_state.has_clipboard_text)));
	entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::SelectAll, select_all_text, create_accelerator(false, "A"), !field_state.is_empty)));
	entry_list.push(BrowserMenuEntry::Separator);
	let mut default_item = create_item(UIInputFieldMenuAction::WritingDirectionDefault, BrowserUiText::WritingDirectionDefault, String::new(), false);
	default_item.is_checked = field_state.writing_direction == UIInputFieldWritingDirection::Default;
	let mut left_to_right_item = create_item(UIInputFieldMenuAction::WritingDirectionLeftToRight, BrowserUiText::WritingDirectionLeftToRight, String::new(), true);
	left_to_right_item.is_checked = field_state.writing_direction == UIInputFieldWritingDirection::LeftToRight;
	let mut right_to_left_item = create_item(UIInputFieldMenuAction::WritingDirectionRightToLeft, BrowserUiText::WritingDirectionRightToLeft, String::new(), true);
	right_to_left_item.is_checked = field_state.writing_direction == UIInputFieldWritingDirection::RightToLeft;
	entry_list.push(BrowserMenuEntry::Submenu {
		label: read_browser_ui_text(BrowserUiText::WritingDirection),
		item_list: vec![default_item, left_to_right_item, right_to_left_item],
	});
	entry_list.push(BrowserMenuEntry::Separator);
	entry_list.push(BrowserMenuEntry::Item(create_item(UIInputFieldMenuAction::Inspect, BrowserUiText::Inspect, String::new(), true)));
	entry_list
}

//==============================================================================
/// 글 폭. (Graphic 없이 메뉴 글꼴로 잰다, 글꼴이 없으면 글자마다 7)
//==============================================================================
fn measure_menu_text(text: &str) -> f32 {
	match measure_text_without_graphic(MENU_FONT_STRING, text) {
		Some(text_metrics) => {
			text_metrics.width
		}
		None => {
			text.chars().count() as f32 * 7.0
		}
	}
}

//==============================================================================
/// 메뉴 판 하나의 크기와 줄 자리. (위에서부터 줄의 위 끝)
//==============================================================================
fn calculate_panel_layout(label_accelerator_list: &[(String, String, bool)], is_separator_list: &[bool]) -> (Vector2, Vec<f32>) {
	let mut maximum_width: f32 = 0.0;
	for (label, accelerator, has_arrow) in label_accelerator_list {
		let mut width = CHECK_COLUMN_WIDTH + measure_menu_text(label) + LABEL_PADDING_RIGHT;
		if !accelerator.is_empty() {
			width += ACCELERATOR_GAP + measure_menu_text(accelerator);
		}
		if *has_arrow {
			width += SUBMENU_ARROW_WIDTH;
		}
		maximum_width = maximum_width.max(width);
	}
	let mut row_top_list: Vec<f32> = Vec::new();
	let mut row_top = MENU_PADDING_Y;
	for is_separator in is_separator_list {
		row_top_list.push(row_top);
		row_top += if *is_separator {
			SEPARATOR_HEIGHT
		}
		else {
			ITEM_HEIGHT
		};
	}
	(Vector2::create(maximum_width.ceil().max(MENU_MINIMUM_WIDTH), row_top + MENU_PADDING_Y), row_top_list)
}

//==============================================================================
/// 메뉴 판에 그릴 줄. (글, 단축키 글, 하위 메뉴 화살표, 구분선인지, 켜짐, 체크)
//==============================================================================
#[derive(Clone, Debug)]
struct PanelRow {
	label: String,
	accelerator: String,
	is_submenu: bool,
	is_separator: bool,
	is_enabled: bool,
	is_checked: bool,
}

//==============================================================================
/// 메뉴 판. (자리는 메뉴 노드 좌표, 줄의 위 끝 목록)
//==============================================================================
#[derive(Clone, Debug)]
struct MenuPanel {
	origin: Vector2,
	size: Vector2,
	row_list: Vec<PanelRow>,
	row_top_list: Vec<f32>,
}

impl MenuPanel {
	//==============================================================================
	/// 줄 목록으로 짓기.
	//==============================================================================
	fn new(origin: Vector2, row_list: Vec<PanelRow>) -> MenuPanel {
		let label_accelerator_list: Vec<(String, String, bool)> = row_list.iter().filter(|row| !row.is_separator).map(|row| (row.label.clone(), row.accelerator.clone(), row.is_submenu)).collect();
		let is_separator_list: Vec<bool> = row_list.iter().map(|row| row.is_separator).collect();
		let (size, row_top_list) = calculate_panel_layout(&label_accelerator_list, &is_separator_list);
		MenuPanel {
			origin,
			size,
			row_list,
			row_top_list,
		}
	}

	//==============================================================================
	/// 메뉴 노드 좌표 아래 줄 자리. (구분선과 판 밖은 None)
	//==============================================================================
	fn find_row_index(&self, local_position: Vector2) -> Option<usize> {
		let panel_x = local_position.x - self.origin.x;
		let panel_y = local_position.y - self.origin.y;
		if panel_x < 0.0 || panel_x > self.size.x || panel_y < 0.0 || panel_y > self.size.y {
			return None;
		}
		for (row_index, row_top) in self.row_top_list.iter().enumerate() {
			let row = &self.row_list[row_index];
			let row_height = if row.is_separator {
				SEPARATOR_HEIGHT
			}
			else {
				ITEM_HEIGHT
			};
			if panel_y >= *row_top && panel_y < row_top + row_height {
				if row.is_separator {
					return None;
				}
				return Some(row_index);
			}
		}
		None
	}

	//==============================================================================
	/// 메뉴 노드 좌표의 점이 판 안인지.
	//==============================================================================
	fn contains(&self, local_position: Vector2) -> bool {
		let panel_x = local_position.x - self.origin.x;
		let panel_y = local_position.y - self.origin.y;
		panel_x >= 0.0 && panel_x <= self.size.x && panel_y >= 0.0 && panel_y <= self.size.y
	}
}

//==============================================================================
/// 메뉴 줄 → 판 줄.
//==============================================================================
fn convert_entry_to_row(entry: &BrowserMenuEntry) -> PanelRow {
	match entry {
		BrowserMenuEntry::Item(item) => {
			convert_item_to_row(item)
		}
		BrowserMenuEntry::Separator => {
			PanelRow {
				label: String::new(),
				accelerator: String::new(),
				is_submenu: false,
				is_separator: true,
				is_enabled: false,
				is_checked: false,
			}
		}
		BrowserMenuEntry::Submenu { label, .. } => {
			PanelRow {
				label: label.clone(),
				accelerator: String::new(),
				is_submenu: true,
				is_separator: false,
				is_enabled: true,
				is_checked: false,
			}
		}
	}
}

//==============================================================================
/// 항목 → 판 줄.
//==============================================================================
fn convert_item_to_row(item: &BrowserMenuItem) -> PanelRow {
	PanelRow {
		label: item.label.clone(),
		accelerator: item.accelerator.clone(),
		is_submenu: false,
		is_separator: false,
		is_enabled: item.is_enabled,
		is_checked: item.is_checked,
	}
}

//==============================================================================
/// 입력 필드의 브라우저 문맥 메뉴. (루트의 마지막 자식 노드에 붙는다. 항목을 누르고 떼면 주인 필드에 항목을 부탁하고 스스로 지운다)
//==============================================================================
pub(crate) struct UIInputFieldBrowserMenu {
	component_base: ComponentBase,
	owner_node_id: NodeId,
	entry_list: Vec<BrowserMenuEntry>,
	main_panel: MenuPanel,
	submenu_panel: Option<(usize, MenuPanel)>,
	hovered_row: Option<(bool, usize)>,
	pressed_row: Option<(bool, usize)>,
	is_first_tick: bool,
}

impl UIInputFieldBrowserMenu {
	//==============================================================================
	/// 생성. (판 크기는 지을 때 잰다)
	//==============================================================================
	pub(crate) fn new(owner_node_id: NodeId, entry_list: Vec<BrowserMenuEntry>) -> UIInputFieldBrowserMenu {
		let row_list: Vec<PanelRow> = entry_list.iter().map(convert_entry_to_row).collect();
		UIInputFieldBrowserMenu {
			component_base: ComponentBase::new(),
			owner_node_id,
			entry_list,
			main_panel: MenuPanel::new(Vector2::zero(), row_list),
			submenu_panel: None,
			hovered_row: None,
			pressed_row: None,
			is_first_tick: true,
		}
	}

	//==============================================================================
	/// 메뉴 판 크기. (노드 크기로 쓴다)
	//==============================================================================
	pub(crate) fn get_main_panel_size(&self) -> Vector2 {
		self.main_panel.size
	}

	//==============================================================================
	/// 메뉴 노드 좌표 아래 줄. ((하위 메뉴인지, 줄 자리))
	//==============================================================================
	fn find_row(&self, local_position: Vector2) -> Option<(bool, usize)> {
		if let Some((_, submenu_panel)) = &self.submenu_panel {
			if let Some(row_index) = submenu_panel.find_row_index(local_position) {
				return Some((true, row_index));
			}
		}
		self.main_panel.find_row_index(local_position).map(|row_index| (false, row_index))
	}

	//==============================================================================
	/// 하위 메뉴 열기. (그 줄 오른쪽에)
	//==============================================================================
	fn open_submenu(&mut self, row_index: usize) {
		if let Some((open_row_index, _)) = &self.submenu_panel {
			if *open_row_index == row_index {
				return;
			}
		}
		if let BrowserMenuEntry::Submenu { item_list, .. } = &self.entry_list[row_index] {
			let row_list: Vec<PanelRow> = item_list.iter().map(convert_item_to_row).collect();
			let origin = Vector2::create(self.main_panel.origin.x + self.main_panel.size.x - 2.0, self.main_panel.row_top_list[row_index] - MENU_PADDING_Y);
			self.submenu_panel = Some((row_index, MenuPanel::new(origin, row_list)));
		}
	}

	//==============================================================================
	/// 줄의 항목. (하위 메뉴 줄이나 구분선은 None)
	//==============================================================================
	fn find_item(&self, row: (bool, usize)) -> Option<&BrowserMenuItem> {
		let (is_submenu_row, row_index) = row;
		if is_submenu_row {
			let (open_row_index, _) = self.submenu_panel.as_ref()?;
			if let BrowserMenuEntry::Submenu { item_list, .. } = &self.entry_list[*open_row_index] {
				return item_list.get(row_index);
			}
			return None;
		}
		match self.entry_list.get(row_index)? {
			BrowserMenuEntry::Item(item) => {
				Some(item)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 메뉴 닫기. (자기 노드를 지운다)
	//==============================================================================
	fn close(&self, context: &mut ComponentContext) {
		let node_id = context.get_node_id();
		context.get_node_tree_mut().destroy_node(node_id);
	}

	//==============================================================================
	/// 판 하나 그리기.
	//==============================================================================
	fn draw_panel(&self, graphic: &mut Graphic, panel: &MenuPanel, hovered_row_index: Option<usize>) {
		let panel_rect = Rect::create(panel.origin.x, panel.origin.y, panel.size.x, panel.size.y);
		for (blur_size, layer_alpha) in [(8.0f32, 0.2f32), (12.0f32, 0.1f32)] {
			let step_count = 3;
			for step_index in 0..step_count {
				let spread = blur_size * 0.5 * (step_index as f32 + 1.0) / step_count as f32;
				graphic.set_fill_color(Color::create(60.0 / 255.0, 64.0 / 255.0, 67.0 / 255.0, layer_alpha / step_count as f32));
				graphic.draw_round_rect(Rect::create(panel_rect.position.x - spread, panel_rect.position.y + 2.0 - spread, panel_rect.size.x + spread * 2.0, panel_rect.size.y + spread * 2.0), MENU_CORNER_RADIUS + spread);
			}
		}
		graphic.set_fill_color(COLOR_MENU_BACKGROUND);
		graphic.draw_round_rect(panel_rect, MENU_CORNER_RADIUS);
		graphic.set_stroke_color(COLOR_MENU_BORDER);
		graphic.draw_stroke_round_rect(Rect::create(panel_rect.position.x + 0.5, panel_rect.position.y + 0.5, panel_rect.size.x - 1.0, panel_rect.size.y - 1.0), MENU_CORNER_RADIUS - 0.5, 1.0);
		graphic.set_font_string(MENU_FONT_STRING);
		graphic.set_text_baseline(TextBaseline::Middle);
		for (row_index, row) in panel.row_list.iter().enumerate() {
			let row_top = panel.origin.y + panel.row_top_list[row_index];
			if row.is_separator {
				graphic.set_fill_color(COLOR_MENU_BORDER);
				graphic.draw_rect(Rect::create(panel.origin.x, row_top + SEPARATOR_HEIGHT * 0.5 - 0.5, panel.size.x, 1.0));
				continue;
			}
			let is_hovered = row.is_enabled && hovered_row_index == Some(row_index);
			if is_hovered {
				graphic.set_fill_color(COLOR_HOVER_BACKGROUND);
				if cfg!(target_os = "macos") {
					graphic.draw_round_rect(Rect::create(panel.origin.x + 5.0, row_top, panel.size.x - 10.0, ITEM_HEIGHT), 4.0);
				}
				else {
					graphic.draw_rect(Rect::create(panel.origin.x, row_top, panel.size.x, ITEM_HEIGHT));
				}
			}
			let text_color = if !row.is_enabled {
				COLOR_DISABLED
			}
			else if is_hovered && cfg!(target_os = "macos") {
				Color::white()
			}
			else {
				Color::from(COLOR_TEXT)
			};
			let middle_y = row_top + ITEM_HEIGHT * 0.5;
			graphic.set_text_align(TextAlign::Left);
			if row.is_checked {
				graphic.set_fill_color(text_color);
				graphic.draw_fill_text("\u{2713}", panel.origin.x + 7.0, middle_y);
			}
			graphic.set_fill_color(text_color);
			graphic.draw_fill_text(&row.label, panel.origin.x + CHECK_COLUMN_WIDTH, middle_y);
			graphic.set_text_align(TextAlign::Right);
			if !row.accelerator.is_empty() {
				let accelerator_color = if row.is_enabled {
					Color::from(COLOR_ACCELERATOR)
				}
				else {
					COLOR_DISABLED
				};
				graphic.set_fill_color(accelerator_color);
				graphic.draw_fill_text(&row.accelerator, panel.origin.x + panel.size.x - LABEL_PADDING_RIGHT, middle_y);
			}
			if row.is_submenu {
				graphic.set_fill_color(text_color);
				graphic.draw_fill_text("\u{203a}", panel.origin.x + panel.size.x - LABEL_PADDING_RIGHT * 0.75, middle_y);
			}
		}
	}
}

impl Component for UIInputFieldBrowserMenu {
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
	/// 컴포넌트 종류 이름 반환. ("UIInputFieldBrowserMenu")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIInputFieldBrowserMenu"
	}

	//==============================================================================
	/// 트리에 붙을 때. (노드를 interactable 로 켠다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		node_tree[node_id].set_interactable(true);
	}

	//==============================================================================
	/// 갱신. (필드가 포커스를 잃었거나 Esc 나 메뉴 밖 누름이면 닫고, 하위 메뉴를 열고 닫고, 누른 줄에서 뗀 항목을 필드에 부탁한다)
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
		let input_manager = context.get_engine().get_input_manager();
		if !is_owner_ready || input_manager.is_key_just_pressed(KeyCode::ESCAPE) {
			self.close(context);
			return;
		}
		let view_input_position = input_manager.get_view_input_position();
		let is_touch_pressed = input_manager.is_touch_pressed();
		let is_touch_released = input_manager.is_touch_released();
		let context_menu_request_position = input_manager.get_text_input().get_context_menu_request_position();
		let node_id = context.get_node_id();
		let local_position = context.get_node_tree().convert_view_position_to_node_position(node_id, view_input_position);
		self.hovered_row = local_position.and_then(|local_position| self.find_row(local_position));
		// 하위 메뉴 줄 위에 오면 연다. 본 메뉴의 다른 줄로 가면 닫는다.
		match self.hovered_row {
			Some((false, row_index)) => {
				if matches!(self.entry_list[row_index], BrowserMenuEntry::Submenu { .. }) {
					self.open_submenu(row_index);
				}
				else {
					self.submenu_panel = None;
				}
			}
			_ => {
			}
		}
		let is_inside_menu = local_position.map(|local_position| self.main_panel.contains(local_position) || self.submenu_panel.as_ref().map(|(_, submenu_panel)| submenu_panel.contains(local_position)).unwrap_or(false)).unwrap_or(false);
		if !self.is_first_tick {
			let is_requested_outside = match context_menu_request_position {
				Some(request_position) => {
					context.get_node_tree().convert_view_position_to_node_position(node_id, request_position).map(|request_local_position| !self.main_panel.contains(request_local_position)).unwrap_or(true)
				}
				None => {
					false
				}
			};
			if (is_touch_pressed && !is_inside_menu) || is_requested_outside {
				self.close(context);
				return;
			}
		}
		self.is_first_tick = false;
		if is_touch_pressed {
			self.pressed_row = self.hovered_row;
		}
		if is_touch_released {
			let pressed_row = self.pressed_row.take();
			if let Some(row) = self.hovered_row.filter(|hovered_row| Some(*hovered_row) == pressed_row || pressed_row.is_none()) {
				let menu_action = self.find_item(row).filter(|item| item.is_enabled).map(|item| item.action);
				if let Some(menu_action) = menu_action {
					let owner_node_id = self.owner_node_id;
					if let Some(owner_field) = context.get_node_tree_mut().get_node_mut(owner_node_id).and_then(|owner_node| owner_node.get_component_mut::<UIInputField>()) {
						owner_field.request_menu_action(menu_action);
					}
					self.close(context);
				}
			}
		}
	}

	//==============================================================================
	/// 그리기. (본 메뉴 판과 열린 하위 메뉴 판, 마우스 아래 줄은 바탕을 칠한다)
	//==============================================================================
	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		graphic.push_state();
		let main_hovered_row_index = match self.hovered_row {
			Some((false, row_index)) => {
				Some(row_index)
			}
			_ => {
				self.submenu_panel.as_ref().map(|(open_row_index, _)| *open_row_index)
			}
		};
		self.draw_panel(graphic, &self.main_panel, main_hovered_row_index);
		if let Some((_, submenu_panel)) = &self.submenu_panel {
			let submenu_hovered_row_index = match self.hovered_row {
				Some((true, row_index)) => {
					Some(row_index)
				}
				_ => {
					None
				}
			};
			self.draw_panel(graphic, submenu_panel, submenu_hovered_row_index);
		}
		graphic.pop_state();
	}
}

//==============================================================================
/// 메뉴 노드 만들기. (루트의 마지막 자식, 창 안쪽 논리 픽셀 크기가 되게 뷰 배율을 되돌리고, 창 밖으로 넘치면 크롬처럼 왼쪽, 위로 뒤집는다)
//==============================================================================
pub(crate) fn create_browser_menu_node(node_tree: &mut NodeTree, owner_node_id: NodeId, view_position: Vector2, view_scale: Vector2, view_size: Vector2, entry_list: Vec<BrowserMenuEntry>, is_touch_anchor: bool) -> NodeId {
	let menu = UIInputFieldBrowserMenu::new(owner_node_id, entry_list);
	let panel_size = menu.get_main_panel_size();
	let inverse_scale = Vector2::create(1.0 / view_scale.x.max(1e-4), 1.0 / view_scale.y.max(1e-4));
	let mut menu_position = view_position;
	// 터치로 연 메뉴는 점 위 가운데. (크롬 toolkit_delegate_views 의 kBottomCenter, menu_controller 의 kTouchYPadding 15, 위로 넘치면 아래)
	if is_touch_anchor {
		let panel_view_size = Vector2::create(panel_size.x * inverse_scale.x, panel_size.y * inverse_scale.y);
		let touch_padding = 15.0 * inverse_scale.y;
		menu_position.x = (view_position.x - panel_view_size.x * 0.5).min(view_size.x - panel_view_size.x).max(0.0);
		menu_position.y = view_position.y - panel_view_size.y - touch_padding;
		if menu_position.y < 0.0 {
			menu_position.y = view_position.y + touch_padding;
		}
	}
	if menu_position.x + panel_size.x * inverse_scale.x > view_size.x {
		menu_position.x = (menu_position.x - panel_size.x * inverse_scale.x).max(0.0);
	}
	if menu_position.y + panel_size.y * inverse_scale.y > view_size.y {
		menu_position.y = (menu_position.y - panel_size.y * inverse_scale.y).max(0.0);
	}
	let root_id = node_tree.get_root_id();
	let local_position = node_tree.convert_view_position_to_node_position(root_id, menu_position).unwrap_or(menu_position);
	let menu_id = node_tree.create_child(root_id, "uiInputFieldBrowserMenu");
	{
		let menu_node = &mut node_tree[menu_id];
		menu_node.set_pivot(Pivot::TOP_LEFT);
		menu_node.set_anchor(Vector2::zero());
		menu_node.set_local_position(local_position);
		menu_node.set_content_size(panel_size);
	}
	node_tree.set_scale(menu_id, inverse_scale);
	node_tree.add_component(menu_id, menu);
	menu_id
}

//==============================================================================
/// 점이 메뉴 노드 안인지. (본 판, 필드의 밖 누름 판정이 쓴다. 열린 하위 메뉴는 노드 밖이라 메뉴가 스스로 본다)
//==============================================================================
pub(crate) fn is_view_position_inside_menu(node_tree: &NodeTree, menu_id: NodeId, view_position: Vector2) -> bool {
	if is_view_position_inside_node(node_tree, menu_id, view_position) {
		return true;
	}
	let local_position = match node_tree.convert_view_position_to_node_position(menu_id, view_position) {
		Some(local_position) => {
			local_position
		}
		None => {
			return false;
		}
	};
	match node_tree.get_node(menu_id).and_then(|menu_node| menu_node.get_component::<UIInputFieldBrowserMenu>()) {
		Some(menu) => {
			menu.submenu_panel.as_ref().map(|(_, submenu_panel)| submenu_panel.contains(local_position)).unwrap_or(false)
		}
		None => {
			false
		}
	}
}

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
		// 표준 그림자. (메뉴와 같은 겹친 둥근 사각형으로 갈음한다)
		for (blur_size, layer_alpha) in [(8.0f32, 0.2f32), (12.0f32, 0.1f32)] {
			let step_count = 3;
			for step_index in 0..step_count {
				let spread = blur_size * 0.5 * (step_index as f32 + 1.0) / step_count as f32;
				graphic.set_fill_color(Color::create(60.0 / 255.0, 64.0 / 255.0, 67.0 / 255.0, layer_alpha / step_count as f32));
				graphic.draw_round_rect(Rect::create(-spread, 2.0 - spread, menu_rect.size.x + spread * 2.0, menu_rect.size.y + spread * 2.0), QUICK_MENU_CORNER_RADIUS + spread);
			}
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

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 차례와 켜짐: 선택이 없을 때와 있을 때. (크롬 EditableTextMenu_EmptySelectionOrder 의 차례에서 브라우저 기능 항목을 뺀 것)
	//==============================================================================
	#[test]
	fn menu_order_and_enabled_state() {
		let field_state = BrowserMenuFieldState {
			can_undo: true,
			can_redo: false,
			has_selection: false,
			has_clipboard_text: true,
			is_empty: false,
			writing_direction: UIInputFieldWritingDirection::LeftToRight,
		};
		let entry_list = build_browser_menu(&field_state);
		let action_list: Vec<Option<UIInputFieldMenuAction>> = entry_list.iter().map(|entry| match entry {
			BrowserMenuEntry::Item(item) => {
				Some(item.action)
			}
			_ => {
				None
			}
		}).collect();
		let mut expected_list: Vec<Option<UIInputFieldMenuAction>> = Vec::new();
		if cfg!(any(target_os = "macos", target_os = "windows")) {
			expected_list.push(Some(UIInputFieldMenuAction::Emoji));
			expected_list.push(None);
		}
		expected_list.extend([Some(UIInputFieldMenuAction::Undo), Some(UIInputFieldMenuAction::Redo), None, Some(UIInputFieldMenuAction::Cut), Some(UIInputFieldMenuAction::Copy), Some(UIInputFieldMenuAction::Paste), Some(UIInputFieldMenuAction::PasteAsPlainText), Some(UIInputFieldMenuAction::SelectAll), None, None, None, Some(UIInputFieldMenuAction::Inspect)]);
		assert_eq!(action_list, expected_list);
		let find_item = |action: UIInputFieldMenuAction| -> BrowserMenuItem {
			entry_list.iter().find_map(|entry| match entry {
				BrowserMenuEntry::Item(item) if item.action == action => {
					Some(item.clone())
				}
				_ => {
					None
				}
			}).expect("item")
		};
		assert!(find_item(UIInputFieldMenuAction::Undo).is_enabled);
		assert!(!find_item(UIInputFieldMenuAction::Redo).is_enabled);
		assert!(!find_item(UIInputFieldMenuAction::Cut).is_enabled);
		assert!(find_item(UIInputFieldMenuAction::Paste).is_enabled);
		assert!(find_item(UIInputFieldMenuAction::SelectAll).is_enabled);
		if !cfg!(target_os = "macos") {
			assert_eq!(find_item(UIInputFieldMenuAction::Undo).accelerator, "Ctrl+Z");
			assert_eq!(find_item(UIInputFieldMenuAction::PasteAsPlainText).accelerator, "Ctrl+Shift+V");
		}
		let submenu = entry_list.iter().find_map(|entry| match entry {
			BrowserMenuEntry::Submenu { item_list, .. } => {
				Some(item_list.clone())
			}
			_ => {
				None
			}
		}).expect("submenu");
		assert!(!submenu[0].is_enabled);
		assert!(submenu[1].is_checked);

		// 선택이 있으면 맥이 아닌 곳에서는 실행 취소와 다시 실행이 빠진다.
		let selected_entry_list = build_browser_menu(&BrowserMenuFieldState {
			has_selection: true,
			..field_state
		});
		let has_undo = selected_entry_list.iter().any(|entry| matches!(entry, BrowserMenuEntry::Item(item) if item.action == UIInputFieldMenuAction::Undo));
		assert_eq!(has_undo, cfg!(target_os = "macos"));
	}
}
