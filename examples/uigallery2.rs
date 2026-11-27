//==============================================================================
// UI 위젯 갤러리 2. (바닐라엔진 ui 모듈의 나머지 위젯 예제)
// - 새로 옮긴 위젯을 한 화면에 놓는다: UIListView, UISnapScrollView, UIDropdown, UIDraggable, UILineChart, UIBarChart, UISpinner,
//   UIToast, UIContextMenu, UIDialog(PopupMotion).
// - 터치는 원본 GameScene 처럼 TouchRecognizer 로 받는다.
// - 사람이 없어도 확인할 수 있게, 정해 둔 프레임에 입력을 흉내 낸다:
//   목록 휠 → 스냅 뷰 넘기기 → 드롭다운 열고 고르기 → 토스트 단추 → 칩 둘 끌기 → 드롭다운 열기 → 메뉴 단추 (90 프레임 화면)
//   → 메뉴 바깥 누름 → 드롭다운 고르기 → 대화 상자 단추 → 토스트 (180 프레임 화면) → 확인 단추.
// - 240 프레임이 지나면 스스로 끝난다. (화면 저장: VANILLA_CAPTURE_PATH / VANILLA_CAPTURE_FRAMES / VANILLA_CAPTURE_EXIT)
//==============================================================================
#![allow(clippy::too_many_arguments)]

use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::ui::uiview::is_view_position_inside_node;
use vanilla::{
	Color, ComponentContext, Engine, EngineConfiguration, Graphic, NodeId, NodeTree, Paint, Pivot, Scene, SceneBase, TouchRecognizer, UIBarChart, UIButton, UIContextMenu, UIContextMenuItem, UIContextMenuOptions, UIDialog, UIDialogOptions, UIDialogShowOptions, UIDraggable, UIDropdown, UIDropdownOptions, UILabel, UILineChart, UIListView, UIScrollView, UISnapScrollView, UISpinner, UISpinnerOptions, UIToast, UIToastOptions, UIView, Vector2, ViewScaleMode,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;
const BACKGROUND_COLOR: &str = "#14171f";
const PANEL_COLOR: &str = "#1d2230";
const ACCENT_COLOR: &str = "#6c7bff";
const LIGHT_TEXT_COLOR: &str = "#e8ecf4";
const CAPTION_TEXT_COLOR: &str = "#8d97ad";
const LIST_ITEM_COUNT: usize = 500;
const LIST_ITEM_HEIGHT: f64 = 40.0;
const SNAP_PAGE_WIDTH: f64 = 260.0;
const SNAP_PAGE_GAP: f64 = 10.0;
const SNAP_PAGE_COUNT: usize = 5;
const EXIT_FRAME_COUNT: u32 = 240;

//==============================================================================
// 왼쪽 위 기준 상자 노드 만들기.
//==============================================================================
fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2) -> NodeId {
	let node_id = node_tree.create_child(parent_id, name);
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_local_position(position);
	node.set_content_size(size);
	node_id
}

//==============================================================================
// 글자 노드 만들기. (UILabel)
//==============================================================================
fn create_label_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, font_size: f64, color: &str, text_align: TextAlign) -> NodeId {
	let label_id = create_box_node(node_tree, parent_id, name, position, size);
	let label = node_tree.add_component(label_id, UILabel::create_with_text(text, font_size)).expect("label");
	label.set_text_color(color);
	label.set_text_align(text_align);
	label.set_text_baseline(TextBaseline::Middle);
	label_id
}

//==============================================================================
// 글자 단추 만들기. (둥근 Paint 바탕 + UILabel + UIButton)
//==============================================================================
fn create_text_button_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, text: &str) -> NodeId {
	let button_id = create_box_node(node_tree, parent_id, name, position, size);
	let button_paint = node_tree[button_id].add_component(Paint::create_with_color(ACCENT_COLOR));
	button_paint.set_round_size(10.0);
	let label = node_tree.add_component(button_id, UILabel::create_with_text(text, 17.0)).expect("label");
	label.set_text_color(LIGHT_TEXT_COLOR);
	label.set_bold(true);
	node_tree.add_component(button_id, UIButton::new());
	button_id
}

//==============================================================================
// 구역 판 만들기. (배경색을 정한 UIView + 구역 이름)
//==============================================================================
fn create_section_panel(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, caption: &str) -> NodeId {
	let panel_id = create_box_node(node_tree, parent_id, name, position, size);
	let panel_view = node_tree.add_component(panel_id, UIView::new()).expect("view");
	panel_view.set_background_color(PANEL_COLOR);
	create_label_node(node_tree, panel_id, "caption", Vector2::create(14.0, 6.0), Vector2::create(size.x - 28.0, 26.0), caption, 16.0, CAPTION_TEXT_COLOR, TextAlign::Left);
	panel_id
}

//==============================================================================
// 노드 가운데의 뷰 좌표. (흉내 낸 입력이 누를 자리)
//==============================================================================
fn calculate_node_center(node_tree: &NodeTree, node_id: NodeId) -> Vector2 {
	let content_size = node_tree[node_id].get_content_size();
	node_tree.convert_node_position_to_view_position(node_id, content_size.multiply_scalar(0.5))
}

//==============================================================================
// 토스트 띄우기. (토스트 노드가 있으면)
//==============================================================================
fn show_toast(node_tree: &mut NodeTree, toast_id: NodeId, message_text: &str) {
	if let Some(toast) = node_tree.get_node_mut(toast_id).and_then(|toast_node| toast_node.get_component_mut::<UIToast>()) {
		toast.show(message_text);
	}
}

//==============================================================================
// 흉내 낸 입력 하나. (그 프레임에 할 일)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputAction {
	None,
	Press,
	Moved,
	Release,
}

//==============================================================================
// 갤러리 씬.
//==============================================================================
struct UIGallery2Scene {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	frame_count: u32,
	elapsed_seconds: f64,
	list_id: Option<NodeId>,
	list_count_label_id: Option<NodeId>,
	snap_id: Option<NodeId>,
	snap_index_label_id: Option<NodeId>,
	dropdown_id: Option<NodeId>,
	chip_ids: Vec<NodeId>,
	slot_id: Option<NodeId>,
	line_chart_id: Option<NodeId>,
	dialog_button_id: Option<NodeId>,
	toast_button_id: Option<NodeId>,
	menu_button_id: Option<NodeId>,
	dialog_id: Option<NodeId>,
	toast_id: Option<NodeId>,
	menu_id: Option<NodeId>,
	input_position: Vector2,
	drag_start_position: Vector2,
}

impl UIGallery2Scene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> UIGallery2Scene {
		UIGallery2Scene {
			scene_base: SceneBase::new(),
			touch_recognizer: TouchRecognizer::new(),
			frame_count: 0,
			elapsed_seconds: 0.0,
			list_id: None,
			list_count_label_id: None,
			snap_id: None,
			snap_index_label_id: None,
			dropdown_id: None,
			chip_ids: Vec::new(),
			slot_id: None,
			line_chart_id: None,
			dialog_button_id: None,
			toast_button_id: None,
			menu_button_id: None,
			dialog_id: None,
			toast_id: None,
			menu_id: None,
			input_position: Vector2::create(-100.0, -100.0),
			drag_start_position: Vector2::zero(),
		}
	}

	//==============================================================================
	// 목록 구역. (500 줄 재활용 목록 + 세로 스크롤바 + 만든 노드 수)
	//==============================================================================
	fn create_list_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = create_section_panel(node_tree, root_id, "listSection", Vector2::create(40.0, 90.0), Vector2::create(380.0, 380.0), "UIListView (500줄, 보이는 만큼만 노드)");
		let list_width = 352.0;
		let list_id = create_box_node(node_tree, panel_id, "list", Vector2::create(14.0, 40.0), Vector2::create(list_width, 290.0));
		{
			let list_view = node_tree.add_component(list_id, UIListView::new()).expect("list view");
			list_view.set_spacing(4.0);
			list_view.set_create_item_event(Box::new(move |tree: &mut NodeTree| {
				let item_id = tree.create_node("item");
				let item_node = &mut tree[item_id];
				item_node.set_pivot(Pivot::TOP_LEFT);
				item_node.set_content_size(Vector2::create(list_width - 24.0, LIST_ITEM_HEIGHT));
				let item_paint = item_node.add_component(Paint::create_with_color("#2a3145"));
				item_paint.set_round_size(6.0);
				let item_label = tree.add_component(item_id, UILabel::create_with_text("", 16.0)).expect("label");
				item_label.set_text_align(TextAlign::Left);
				item_label.set_text_color(LIGHT_TEXT_COLOR);
				item_id
			}));
			list_view.set_bind_item_event(Box::new(|tree: &mut NodeTree, item_id: NodeId, item_index: usize| {
				let item_node = &mut tree[item_id];
				let item_color = if item_index.is_multiple_of(2) {
					"#2a3145"
				}
				else {
					"#323a52"
				};
				if let Some(item_paint) = item_node.get_component_mut::<Paint>() {
					item_paint.set_color(item_color);
				}
				if let Some(item_label) = item_node.get_component_mut::<UILabel>() {
					let item_text = format!("   {}번 항목", item_index + 1);
					item_label.set_text(&item_text);
				}
			}));
			list_view.set_item_count(LIST_ITEM_COUNT);
		}
		if let Some(scroll_view) = node_tree[list_id].get_component_mut::<UIScrollView>() {
			scroll_view.set_horizontal(false);
			scroll_view.set_shows_vertical_scroll_bar(true);
		}
		let list_count_label_id = create_label_node(node_tree, panel_id, "listCount", Vector2::create(14.0, 338.0), Vector2::create(352.0, 30.0), "", 15.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		self.list_id = Some(list_id);
		self.list_count_label_id = Some(list_count_label_id);
	}

	//==============================================================================
	// 스냅 뷰 구역. (가운데로 맞춰지는 칸 다섯)
	//==============================================================================
	fn create_snap_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = create_section_panel(node_tree, root_id, "snapSection", Vector2::create(440.0, 90.0), Vector2::create(400.0, 230.0), "UISnapScrollView (한 번에 한 칸)");
		let snap_width = 372.0;
		let side_padding = (snap_width - SNAP_PAGE_WIDTH) * 0.5;
		let snap_id = create_box_node(node_tree, panel_id, "snap", Vector2::create(14.0, 40.0), Vector2::create(snap_width, 150.0));
		let content_id = {
			let snap_view = node_tree.add_component(snap_id, UISnapScrollView::new()).expect("snap view");
			snap_view.set_background_color("#161a25");
			let content_width = side_padding * 2.0 + SNAP_PAGE_COUNT as f64 * (SNAP_PAGE_WIDTH + SNAP_PAGE_GAP) - SNAP_PAGE_GAP;
			snap_view.set_scroll_content_size(Vector2::create(content_width, 150.0));
			snap_view.get_content().expect("content")
		};
		let page_colors = ["#ff7a7a", "#ffb35c", "#7fd38a", "#5cc8ff", "#b48cff"];
		for (page_index, page_color) in page_colors.iter().enumerate() {
			let page_position = Vector2::create(side_padding + page_index as f64 * (SNAP_PAGE_WIDTH + SNAP_PAGE_GAP), 12.0);
			let page_id = create_box_node(node_tree, content_id, "page", page_position, Vector2::create(SNAP_PAGE_WIDTH, 126.0));
			let page_paint = node_tree[page_id].add_component(Paint::create_with_color(*page_color));
			page_paint.set_round_size(14.0);
			let page_text = format!("{}", page_index + 1);
			let page_label = node_tree.add_component(page_id, UILabel::create_with_text(&page_text, 48.0)).expect("label");
			page_label.set_text_color("#14171f");
			page_label.set_bold(true);
		}
		let snap_index_label_id = create_label_node(node_tree, panel_id, "snapIndex", Vector2::create(14.0, 196.0), Vector2::create(372.0, 26.0), "", 15.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		self.snap_id = Some(snap_id);
		self.snap_index_label_id = Some(snap_index_label_id);
	}

	//==============================================================================
	// 드롭다운 구역. (고르면 문맥으로 글자를 바꾼다)
	//==============================================================================
	fn create_dropdown_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = create_section_panel(node_tree, root_id, "dropdownSection", Vector2::create(40.0, 490.0), Vector2::create(380.0, 140.0), "UIDropdown");
		let selection_label_id = create_label_node(node_tree, panel_id, "selection", Vector2::create(14.0, 96.0), Vector2::create(352.0, 30.0), "고른 값: 어두운 테마", 15.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		let dropdown_options = UIDropdownOptions {
			width: 220.0,
			height: 36.0,
			option_height: 32.0,
			font_size: 16.0,
			..UIDropdownOptions::default()
		};
		let dropdown_id = UIDropdown::create(node_tree, panel_id, dropdown_options);
		node_tree[dropdown_id].set_local_position(Vector2::create(14.0, 48.0));
		node_tree.with_component_mut::<UIDropdown, _>(dropdown_id, |dropdown, tree| {
			dropdown.set_option_list(tree, &["어두운 테마", "밝은 테마", "시스템 설정", "고대비"]);
			dropdown.set_changed_event(Box::new(move |dropdown: &mut UIDropdown, context: &mut ComponentContext| {
				let selection_text = format!("고른 값: {}", dropdown.get_selected_text());
				if let Some(selection_label) = context.get_node_tree_mut().get_node_mut(selection_label_id).and_then(|label_node| label_node.get_component_mut::<UILabel>()) {
					selection_label.set_text(&selection_text);
				}
			}));
		});
		self.dropdown_id = Some(dropdown_id);
	}

	//==============================================================================
	// 끌기 구역. (칸에 놓으면 눌러앉고, 밖에 놓으면 돌아가는 칩 둘)
	//==============================================================================
	fn create_draggable_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = create_section_panel(node_tree, root_id, "dragSection", Vector2::create(440.0, 340.0), Vector2::create(400.0, 290.0), "UIDraggable (칸에 놓으면 남고, 밖이면 돌아감)");
		let slot_id = create_box_node(node_tree, panel_id, "slot", Vector2::create(230.0, 70.0), Vector2::create(150.0, 150.0));
		let slot_paint = node_tree[slot_id].add_component(Paint::create_with_color("#262d40"));
		slot_paint.set_round_size(16.0);
		create_label_node(node_tree, slot_id, "slotCaption", Vector2::create(0.0, 110.0), Vector2::create(150.0, 30.0), "놓는 칸", 15.0, CAPTION_TEXT_COLOR, TextAlign::Center);
		let chip_colors = ["#ffb35c", "#7fd38a"];
		for (chip_index, chip_color) in chip_colors.iter().enumerate() {
			let chip_position = Vector2::create(70.0, 90.0 + chip_index as f64 * 100.0);
			let chip_id = node_tree.create_child(panel_id, "chip");
			let chip_node = &mut node_tree[chip_id];
			chip_node.set_local_position(chip_position);
			chip_node.set_content_size(Vector2::create(96.0, 60.0));
			let chip_paint = chip_node.add_component(Paint::create_with_color(*chip_color));
			chip_paint.set_round_size(12.0);
			let chip_text = format!("칩 {}", chip_index + 1);
			let chip_label = node_tree.add_component(chip_id, UILabel::create_with_text(&chip_text, 18.0)).expect("label");
			chip_label.set_text_color("#14171f");
			chip_label.set_bold(true);
			let draggable = node_tree.add_component(chip_id, UIDraggable::new()).expect("draggable");
			draggable.set_snap_back_enabled(true);
			draggable.set_drag_end_event(Box::new(move |draggable: &mut UIDraggable, context: &mut ComponentContext, view_position: Vector2| {
				if is_view_position_inside_node(context.get_node_tree(), slot_id, view_position) {
					draggable.settle_here(context.get_node_tree_mut());
				}
			}));
			self.chip_ids.push(chip_id);
		}
		self.slot_id = Some(slot_id);
	}

	//==============================================================================
	// 차트 / 스피너 / 단추 구역.
	//==============================================================================
	fn create_chart_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = create_section_panel(node_tree, root_id, "chartSection", Vector2::create(860.0, 90.0), Vector2::create(380.0, 540.0), "UILineChart / UIBarChart / UISpinner");

		// 실시간 라인 차트.
		let line_chart_id = create_box_node(node_tree, panel_id, "lineChart", Vector2::create(14.0, 40.0), Vector2::create(352.0, 120.0));
		node_tree[line_chart_id].add_component(Paint::create_with_color("#161a25"));
		let line_chart = node_tree[line_chart_id].add_component(UILineChart::new());
		line_chart.set_max_sample_count(90);
		line_chart.set_line_color(ACCENT_COLOR);
		line_chart.set_line_width(2.0);
		line_chart.set_value_range(Some(0.0), Some(260.0));
		let history_values: Vec<f64> = (0..60).map(|sample_index| 130.0 + (sample_index as f64 * 0.35).sin() * 60.0 + ((sample_index * 37) % 23) as f64).collect();
		line_chart.set_value_list(&history_values);

		// 바 차트. (마지막 막대 강조)
		let bar_chart_id = create_box_node(node_tree, panel_id, "barChart", Vector2::create(14.0, 176.0), Vector2::create(352.0, 110.0));
		let bar_chart = node_tree[bar_chart_id].add_component(UIBarChart::new());
		let bar_values: Vec<f64> = (0..12).map(|day_index| 2400.0 + ((day_index * 1733) % 2800) as f64).collect();
		bar_chart.set_value_list(&bar_values);
		bar_chart.set_bar_color(Color::create(0.357, 0.424, 1.0, 0.45));
		bar_chart.set_highlight_color(ACCENT_COLOR);
		bar_chart.set_highlight_index(Some(11));

		// 스피너 둘.
		create_label_node(node_tree, panel_id, "spinnerCaption", Vector2::create(14.0, 300.0), Vector2::create(200.0, 30.0), "UISpinner", 15.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		let small_spinner_id = UISpinner::create(node_tree, panel_id, UISpinnerOptions {
			radius: 10.0,
			dot_radius: 2.2,
			color: Color::from(CAPTION_TEXT_COLOR),
			..UISpinnerOptions::default()
		});
		node_tree[small_spinner_id].set_local_position(Vector2::create(130.0, 315.0));
		let large_spinner_id = UISpinner::create(node_tree, panel_id, UISpinnerOptions {
			radius: 18.0,
			dot_count: 12,
			dot_radius: 3.2,
			color: Color::from(ACCENT_COLOR),
			rotation_speed: 0.8,
		});
		node_tree[large_spinner_id].set_local_position(Vector2::create(190.0, 315.0));

		// 대화 상자 / 토스트 / 메뉴 단추. (누름 처리는 겹 위젯을 만든 뒤 bind_overlay_buttons 에서 건다)
		let dialog_button_id = create_text_button_node(node_tree, panel_id, "dialogButton", Vector2::create(14.0, 360.0), Vector2::create(352.0, 46.0), "대화 상자 열기");
		let toast_button_id = create_text_button_node(node_tree, panel_id, "toastButton", Vector2::create(14.0, 418.0), Vector2::create(352.0, 46.0), "토스트 두 개 띄우기");
		let menu_button_id = create_text_button_node(node_tree, panel_id, "menuButton", Vector2::create(14.0, 476.0), Vector2::create(352.0, 46.0), "컨텍스트 메뉴 열기");
		self.line_chart_id = Some(line_chart_id);
		self.dialog_button_id = Some(dialog_button_id);
		self.toast_button_id = Some(toast_button_id);
		self.menu_button_id = Some(menu_button_id);
	}

	//==============================================================================
	// 겹 위젯. (뿌리의 마지막 자식들: 대화 상자, 컨텍스트 메뉴, 토스트)
	//==============================================================================
	fn create_overlays(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let dialog_id = UIDialog::create(node_tree, root_id, UIDialogOptions::default());
		let menu_id = UIContextMenu::create(node_tree, root_id, UIContextMenuOptions {
			menu_width: 200.0,
			item_height: 34.0,
			font_size: 16.0,
			..UIContextMenuOptions::default()
		});
		// 줄 선 토스트가 화면 저장 프레임 안에 차례로 보이도록 머무는 시간을 줄인다.
		let toast_id = UIToast::create(node_tree, root_id, UIToastOptions {
			rest_y: 660.0,
			hold_seconds: 0.8,
			font_size: 17.0,
			background_color: Color::create(0.24, 0.28, 0.4, 0.95),
			..UIToastOptions::default()
		});
		self.dialog_id = Some(dialog_id);
		self.menu_id = Some(menu_id);
		self.toast_id = Some(toast_id);
	}

	//==============================================================================
	// 단추 누름 처리 걸기. (대화 상자 열기, 토스트 띄우기, 메뉴 열기)
	//==============================================================================
	fn bind_overlay_buttons(&mut self) {
		let (dialog_id, menu_id, toast_id) = match (self.dialog_id, self.menu_id, self.toast_id) {
			(Some(dialog_id), Some(menu_id), Some(toast_id)) => {
				(dialog_id, menu_id, toast_id)
			}
			_ => {
				return;
			}
		};
		let node_tree = self.scene_base.get_node_tree_mut();
		if let Some(dialog_button) = self.dialog_button_id.and_then(|button_id| node_tree[button_id].get_component_mut::<UIButton>()) {
			dialog_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				let show_options = UIDialogShowOptions {
					on_confirm: Some(Box::new(move |_dialog: &mut UIDialog, dialog_context: &mut ComponentContext| {
						show_toast(dialog_context.get_node_tree_mut(), toast_id, "확인을 눌렀어요");
					})),
					on_cancel: Some(Box::new(move |_dialog: &mut UIDialog, dialog_context: &mut ComponentContext| {
						show_toast(dialog_context.get_node_tree_mut(), toast_id, "취소했어요");
					})),
					confirm_text: Some(String::from("지우기")),
					..UIDialogShowOptions::default()
				};
				context.get_node_tree_mut().with_component_mut::<UIDialog, _>(dialog_id, |dialog, tree| {
					dialog.show(tree, "이 항목을 정말 지울까요?\n지운 뒤에는 되돌릴 수 없어요.", show_options);
				});
			}));
		}
		if let Some(toast_button) = self.toast_button_id.and_then(|button_id| node_tree[button_id].get_component_mut::<UIButton>()) {
			toast_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				show_toast(context.get_node_tree_mut(), toast_id, "저장했어요");
				show_toast(context.get_node_tree_mut(), toast_id, "두 번째 알림은 줄을 서요");
			}));
		}
		if let Some(menu_button) = self.menu_button_id.and_then(|button_id| node_tree[button_id].get_component_mut::<UIButton>()) {
			menu_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				let menu_position = context.get_engine().get_input_manager().get_view_input_position();
				let item_list = vec![
					UIContextMenuItem::create("  복사", Box::new(move |item_context: &mut ComponentContext| {
						show_toast(item_context.get_node_tree_mut(), toast_id, "복사했어요");
					})),
					UIContextMenuItem::create("  붙여넣기", Box::new(move |item_context: &mut ComponentContext| {
						show_toast(item_context.get_node_tree_mut(), toast_id, "붙여넣었어요");
					})),
					UIContextMenuItem::create_disabled("  지우기 (쓸 수 없음)"),
				];
				context.get_node_tree_mut().with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
					context_menu.show(tree, menu_position, item_list);
				});
			}));
		}
	}

	//==============================================================================
	// 흉내 낸 입력. (정해 둔 프레임에 누름 / 이동 / 뗌 / 휠)
	//==============================================================================
	fn simulate_input(&mut self, engine: &Engine) {
		let frame_count = self.frame_count;
		let node_tree = self.scene_base.get_node_tree();
		let mut input_action = InputAction::None;
		let mut wheel_delta_y = 0.0;

		// 목록 휠.
		if frame_count == 8 {
			if let Some(list_id) = self.list_id {
				self.input_position = calculate_node_center(node_tree, list_id);
			}
		}
		if frame_count == 10 {
			wheel_delta_y = 1500.0;
		}

		// 스냅 뷰를 왼쪽으로 200 픽셀 끈다.
		if frame_count == 15 {
			if let Some(snap_id) = self.snap_id {
				self.drag_start_position = calculate_node_center(node_tree, snap_id).add(Vector2::create(100.0, 0.0));
				self.input_position = self.drag_start_position;
				input_action = InputAction::Press;
			}
		}
		if (16..=25).contains(&frame_count) {
			let drag_frame_count = (frame_count - 15) as f64;
			self.input_position = self.drag_start_position.add(Vector2::create(-20.0 * drag_frame_count, 0.0));
			input_action = InputAction::Moved;
		}
		if frame_count == 28 {
			input_action = InputAction::Release;
		}

		// 드롭다운을 열고 셋째 줄을 고른다.
		let dropdown_option_ids: Vec<NodeId> = self.dropdown_id.and_then(|dropdown_id| node_tree[dropdown_id].get_component::<UIDropdown>()).map(|dropdown| dropdown.get_option_nodes().to_vec()).unwrap_or_default();
		if frame_count == 34 || frame_count == 84 {
			if let Some(dropdown_id) = self.dropdown_id {
				self.input_position = calculate_node_center(node_tree, dropdown_id);
				input_action = InputAction::Press;
			}
		}
		if frame_count == 36 || frame_count == 86 {
			input_action = InputAction::Release;
		}
		if frame_count == 42 || frame_count == 106 {
			let option_index = if frame_count == 42 {
				2
			}
			else {
				1
			};
			if let Some(option_id) = dropdown_option_ids.get(option_index) {
				self.input_position = calculate_node_center(node_tree, *option_id);
				input_action = InputAction::Press;
			}
		}
		if frame_count == 44 || frame_count == 108 {
			input_action = InputAction::Release;
		}

		// 토스트 단추.
		if frame_count == 48 {
			if let Some(toast_button_id) = self.toast_button_id {
				self.input_position = calculate_node_center(node_tree, toast_button_id);
				input_action = InputAction::Press;
			}
		}
		if frame_count == 50 {
			input_action = InputAction::Release;
		}

		// 칩 1 은 칸 밖에, 칩 2 는 칸 안에 놓는다.
		for (chip_index, start_frame) in [56u32, 70u32].iter().copied().enumerate() {
			if frame_count == start_frame {
				if let Some(chip_id) = self.chip_ids.get(chip_index) {
					self.drag_start_position = calculate_node_center(node_tree, *chip_id);
					self.input_position = self.drag_start_position;
					input_action = InputAction::Press;
				}
			}
			if frame_count > start_frame && frame_count <= start_frame + 8 {
				let drag_frame_count = (frame_count - start_frame) as f64;
				let drag_step = if chip_index == 0 {
					Vector2::create(6.0, -4.0)
				}
				else {
					Vector2::create(22.0, -8.0)
				};
				self.input_position = self.drag_start_position.add(drag_step.multiply_scalar(drag_frame_count));
				input_action = InputAction::Moved;
			}
			if frame_count == start_frame + 10 {
				input_action = InputAction::Release;
			}
		}

		// 메뉴 단추. (메뉴는 누른 자리에 뜬다. 90 프레임 화면에 메뉴와 펼친 드롭다운이 함께 보인다)
		if frame_count == 87 {
			if let Some(menu_button_id) = self.menu_button_id {
				self.input_position = calculate_node_center(node_tree, menu_button_id).add(Vector2::create(-120.0, 0.0));
				input_action = InputAction::Press;
			}
		}
		if frame_count == 89 {
			input_action = InputAction::Release;
		}

		// 메뉴 바깥을 눌러 닫는다.
		if frame_count == 100 {
			self.input_position = Vector2::create(1250.0, 700.0);
			input_action = InputAction::Press;
		}
		if frame_count == 102 {
			input_action = InputAction::Release;
		}

		// 대화 상자 단추. (180 프레임 화면에 다 열린 대화 상자와 토스트)
		if frame_count == 120 {
			if let Some(dialog_button_id) = self.dialog_button_id {
				self.input_position = calculate_node_center(node_tree, dialog_button_id);
				input_action = InputAction::Press;
			}
		}
		if frame_count == 122 {
			input_action = InputAction::Release;
		}

		// 확인 단추.
		if frame_count == 200 {
			let confirm_id = self.dialog_id.and_then(|dialog_id| node_tree[dialog_id].get_component::<UIDialog>()).and_then(|dialog| dialog.get_confirm_node());
			if let Some(confirm_id) = confirm_id {
				self.input_position = calculate_node_center(node_tree, confirm_id);
				input_action = InputAction::Press;
			}
		}
		if frame_count == 202 {
			input_action = InputAction::Release;
		}

		let input_position = self.input_position;
		let mut input_manager = engine.get_input_manager_mut();
		input_manager.set_view_input_position(input_position);
		if wheel_delta_y != 0.0 {
			input_manager.add_wheel_delta(0.0, wheel_delta_y);
		}
		match input_action {
			InputAction::Press => {
				input_manager.set_touch_pressed(true);
				input_manager.set_touch_down(true);
			}
			InputAction::Moved => {
				input_manager.set_touch_moved(true);
			}
			InputAction::Release => {
				input_manager.set_touch_released(true);
				input_manager.set_touch_down(false);
			}
			InputAction::None => {
			}
		}
	}

	//==============================================================================
	// 보이는 상태 글자와 실시간 차트 갱신.
	//==============================================================================
	fn update_status(&mut self) {
		let elapsed_seconds = self.elapsed_seconds;
		let frame_count = self.frame_count;
		let (list_id, list_count_label_id, snap_id, snap_index_label_id, line_chart_id, toast_id) = (self.list_id, self.list_count_label_id, self.snap_id, self.snap_index_label_id, self.line_chart_id, self.toast_id);
		let node_tree = self.scene_base.get_node_tree_mut();
		if let (Some(list_id), Some(list_count_label_id)) = (list_id, list_count_label_id) {
			let active_item_count = node_tree[list_id].get_component::<UIListView>().map(|list_view| list_view.get_active_item_count()).unwrap_or(0);
			let content_id = node_tree[list_id].get_component::<UIScrollView>().and_then(|scroll_view| scroll_view.get_content());
			let content_child_count = content_id.map(|content_id| node_tree.get_child_count(content_id)).unwrap_or(0);
			let count_text = format!("500줄 가운데 보이는 {}줄, 만든 항목 노드 {}개", active_item_count, content_child_count);
			if let Some(count_label) = node_tree[list_count_label_id].get_component_mut::<UILabel>() {
				count_label.set_text(&count_text);
			}
		}
		if let (Some(snap_id), Some(snap_index_label_id)) = (snap_id, snap_index_label_id) {
			let snap_index = node_tree[snap_id].get_component::<UISnapScrollView>().map(|snap_view| snap_view.get_snap_current_index()).unwrap_or(0);
			let snap_text = format!("지금 칸: {} / {}", snap_index + 1, SNAP_PAGE_COUNT);
			if let Some(snap_label) = node_tree[snap_index_label_id].get_component_mut::<UILabel>() {
				snap_label.set_text(&snap_text);
			}
		}
		if let Some(line_chart) = line_chart_id.and_then(|line_chart_id| node_tree[line_chart_id].get_component_mut::<UILineChart>()) {
			let live_value = 130.0 + (elapsed_seconds * 4.0).sin() * 60.0 + ((frame_count * 37) % 23) as f64;
			line_chart.push_value(live_value);
		}
		if frame_count == 150 {
			if let Some(toast_id) = toast_id {
				show_toast(node_tree, toast_id, "대화 상자 위에도 토스트가 떠요");
			}
		}
	}
}

impl Scene for UIGallery2Scene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (위젯을 모두 놓고, 겹 위젯을 맨 뒤에 붙인다)
	//==============================================================================
	fn create(&mut self, _engine: &Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = create_label_node(node_tree, root_id, "title", Vector2::create(40.0, 24.0), Vector2::create(900.0, 50.0), "바닐라엔진 UI 위젯 갤러리 2", 32.0, ACCENT_COLOR, TextAlign::Left);
		if let Some(title_label) = node_tree[title_id].get_component_mut::<UILabel>() {
			title_label.set_bold(true);
		}
		create_label_node(node_tree, root_id, "help", Vector2::create(40.0, 650.0), Vector2::create(1200.0, 40.0), "입력은 흉내 낸 것: 목록 휠 → 스냅 넘김 → 드롭다운 → 토스트 → 칩 끌기 → 메뉴 → 대화 상자 → 확인", 16.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		self.create_list_section(root_id);
		self.create_snap_section(root_id);
		self.create_draggable_section(root_id);
		self.create_dropdown_section(root_id);
		self.create_chart_section(root_id);
		self.create_overlays(root_id);
		self.bind_overlay_buttons();
	}

	//==============================================================================
	// 주기적 갱신. (흉내 낸 입력 → 상태 글자 → 노드 갱신 → TouchRecognizer)
	//==============================================================================
	fn tick(&mut self, engine: &Engine, time_delta: f64) {
		self.frame_count += 1;
		self.elapsed_seconds += time_delta;
		if self.frame_count >= EXIT_FRAME_COUNT {
			engine.exit_application();
			return;
		}
		self.simulate_input(engine);
		self.update_status();
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
	}

	//==============================================================================
	// 이전 출력. (배경 칠하고 뷰 좌표계 걸기)
	//==============================================================================
	fn pre_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("UI 위젯 갤러리 2 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	// 화면 저장 때도 프레임 간격이 일정하도록 60 FPS 로 묶는다.
	engine_configuration.maximum_frame_per_second = 60.0;
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(UIGallery2Scene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
