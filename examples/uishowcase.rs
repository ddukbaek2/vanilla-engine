//==============================================================================
// UI 쇼케이스 예제. "VANILLA CONSOLE" (원본 examples/uishowcase 와 같은 장면)
// - 서비스 관리 대시보드 앱. 화면 넷(대시보드, 스토어, 설정, 컴포넌트)과 왼쪽 레일, 팝업은 UIEditor 가 저장하는 uiasset(JSON) 여섯 벌이고
//   UIDocument 가 그대로 읽는다. 목록 항목은 문서에 템플릿 한 줄만 있고, 실제 항목은 코드가 데이터로 UIListView 에 채운다.
// - 테마(Dim, Dark, Light)는 문서에 구워진 Dim 색을 역할로 되찾아 두었다가 역할 표로 다시 칠한다. 고른 테마는 LocalStorage 에 남긴다.
// - 팝업(PopupMotion + Typewriter), 토스트(UIToast), 문맥 메뉴(UIContextMenu, 오른쪽 단추나 길게 누름), 끌기 칩(UIDraggable),
//   드롭다운(UIDropdown), 스피너(UISpinner), 라인과 막대 차트, 반응형 배치(화면 크기에 따라 오른쪽 패널이 끝에 붙고 본문 폭이 변한다).
// - examples/assets/uishowcase/*.uiasset.json 여섯은 원본 저장소 examples/uishowcase/assets 와 같은 파일이다.
// - 처음 화면은 환경 변수 VANILLA_EXAMPLE_MODE 로 고른다. (home, shop, settings, components)
// - 실행: cargo run --release --example uishowcase
//   화면 저장: VANILLA_CAPTURE_PATH=shots/uishowcase.png VANILLA_CAPTURE_FRAMES=90 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::ui::uiview::is_view_position_inside_node;
use vanilla::{
	easing_function, format, Color, ComponentContext, Engine, EngineConfiguration, Graphic, LocalStorage, MouseButton, NodeId, NodeTree, Paint, Pivot, PopupMotion, Scene, SceneBase, SeededRandom, TextAsset, TouchRecognizer, Tween, TweenValues, Typewriter,
	TypewriterOptions, TypewriterTarget, UIBarChart, UIButton, UIContextMenu, UIContextMenuItem, UIContextMenuOptions, UIDocument, UIDraggable, UIDropdown, UIDropdownOptions, UILabel, UILineChart, UIListView, UIProgressView, UISlider, UISpinner,
	UISpinnerOptions, UIToast, UIToastOptions, UIToggleButton, Vector2, ViewScaleMode,
};
use vanilla::BeepPlayer;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const ASSET_DIRECTORY: &str = "examples/assets/uishowcase";
const ASSET_FILE_NAMES: [&str; 6] = ["home", "shop", "settings", "components", "navigation", "popup"];
const SCREEN_NAMES: [&str; 4] = ["home", "shop", "settings", "components"];
const NAVIGATION_BUTTON_NAMES: [&str; 4] = ["NavDASHBOARD", "NavSTORE", "NavSETTINGS", "NavCOMPONENTS"];
const CATALOG_TOTAL_COUNT: usize = 500;
const CATALOG_INITIAL_COUNT: usize = 12;
const CATALOG_PAGE_COUNT: usize = 20;
const CATALOG_LOAD_SECONDS: f64 = 0.7;
const SIDE_PANEL_OFFSET: f64 = 356.0; // 화면 오른쪽 끝에서 사이드 패널 왼쪽까지.
const THEME_STORAGE_KEY: &str = "vanillaConsole.theme";
const THEME_KEYS: [&str; 3] = ["dim", "dark", "light"];
const TYPEWRITER_TEXT: &str = "Text reveal is driven by the engine Typewriter -\nit exposes characters one by one on any label,\nwith skip and completion callbacks.";
const VIEW_WIDTH: f64 = 1680.0;
const VIEW_HEIGHT: f64 = 1000.0;

// 테마 역할 번호. (문서가 구워진 Dim 색을 되찾을 때 앞의 열넷만 맞춰 본다)
const ROLE_NAMES: [&str; 14] = ["background", "surface", "card", "cardRaised", "hairline", "text", "textDim", "textFaint", "accent", "positive", "negative", "ink", "toggleOff", "overlay"];
const ROLE_SURFACE: usize = 1;
const ROLE_CARD: usize = 2;
const ROLE_CARD_RAISED: usize = 3;
const ROLE_TEXT: usize = 5;
const ROLE_TEXT_DIM: usize = 6;
const ROLE_TEXT_FAINT: usize = 7;
const ROLE_ACCENT: usize = 8;
const ROLE_POSITIVE: usize = 9;
const ROLE_NEGATIVE: usize = 10;
const ROLE_INK: usize = 11;
const ROLE_TOGGLE_OFF: usize = 12;
const ROLE_TEXT_ON_ACCENT: usize = 14;

const EVENT_MESSAGES: [(&str, &str); 10] = [
	("Deploy finished on production", "deploy"),
	("Nightly backup completed", "system"),
	("New member joined the workspace", "auth"),
	("API key rotated", "auth"),
	("Invoice paid", "billing"),
	("Build #4132 passed", "ci"),
	("Alert resolved: high latency", "system"),
	("Config updated by admin", "system"),
	("Weekly report generated", "report"),
	("Cache purged on edge nodes", "deploy"),
];

//==============================================================================
// 테마 표. (역할 -> 색. 문서는 Dim 팔레트로 구워져 있고, 로드 때 역할로 캐시해 전환한다)
//==============================================================================
struct Theme {
	label: &'static str,
	colors: [Color; 15],
	bar_track: Color,
	grid_line: Color,
}

fn theme_color(hex_text: &str) -> Color {
	let channel = |start: usize| -> f64 { (i64::from_str_radix(&hex_text[start..start + 2], 16).unwrap() as f64 / 255.0 * 1000.0).round() / 1000.0 };
	Color::create(channel(1), channel(3), channel(5), 1.0)
}

fn make_theme(label: &'static str, hex_list: [&str; 15], bar_track: &str, grid_line: Color) -> Theme {
	let colors = hex_list.map(theme_color);
	Theme { label, colors, bar_track: theme_color(bar_track), grid_line }
}

fn create_theme_table() -> Vec<Theme> {
	vec![
		make_theme(
			"Dim",
			["#14171e", "#1b202a", "#232937", "#2c3344", "#394154", "#e8ebf2", "#9aa2b2", "#6b7387", "#6d7dff", "#45b585", "#e06058", "#12151c", "#3c4457", "#06080d", "#ffffff"],
			"#333a4b",
			Color::create(1.0, 1.0, 1.0, 0.07),
		),
		make_theme(
			"Dark",
			["#0b0d12", "#12151c", "#171b24", "#1e2430", "#262c38", "#e6e9ef", "#8a91a0", "#545b69", "#5b6cff", "#3fb27f", "#d9564f", "#0b0d12", "#2c3442", "#04060a", "#e6e9ef"],
			"#1e2431",
			Color::create(1.0, 1.0, 1.0, 0.07),
		),
		make_theme(
			"Light",
			["#eef1f6", "#ffffff", "#f4f6fa", "#e6eaf2", "#d9dfe9", "#1d2433", "#5b6474", "#97a0b1", "#4f5fe8", "#1f9e6e", "#d5483f", "#ffffff", "#c9d1dd", "#2a3145", "#ffffff"],
			"#dde2ec",
			Color::create(0.0, 0.0, 0.0, 0.08),
		),
	]
}

//==============================================================================
// 현지 시각의 시와 분. (원본 Date 의 getHours, getMinutes. 분 오프셋을 더한다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn local_hour_minute(minute_offset: i64) -> (u32, u32) {
	let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(js_sys::Date::now() + minute_offset as f64 * 60000.0));
	(date.get_hours(), date.get_minutes())
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
fn local_hour_minute(minute_offset: i64) -> (u32, u32) {
	#[repr(C)]
	struct Tm {
		tm_sec: i32,
		tm_min: i32,
		tm_hour: i32,
		tm_mday: i32,
		tm_mon: i32,
		tm_year: i32,
		tm_wday: i32,
		tm_yday: i32,
		tm_isdst: i32,
		tm_gmtoff: i64,
		tm_zone: *const u8,
	}
	extern "C" {
		fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
	}
	let epoch_seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_secs() as i64).unwrap_or(0) + minute_offset * 60;
	let mut local_time = Tm { tm_sec: 0, tm_min: 0, tm_hour: 0, tm_mday: 0, tm_mon: 0, tm_year: 0, tm_wday: 0, tm_yday: 0, tm_isdst: 0, tm_gmtoff: 0, tm_zone: std::ptr::null() };
	unsafe {
		localtime_r(&epoch_seconds, &mut local_time);
	}
	(local_time.tm_hour as u32, local_time.tm_min as u32)
}

#[cfg(all(not(unix), not(target_arch = "wasm32")))]
fn local_hour_minute(minute_offset: i64) -> (u32, u32) {
	let epoch_seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_secs() as i64).unwrap_or(0) + minute_offset * 60;
	(((epoch_seconds / 3600) % 24) as u32, ((epoch_seconds / 60) % 60) as u32)
}

//==============================================================================
// 데이터. (전부 코드 소관, 문서에는 템플릿만 있다)
//==============================================================================
#[derive(Clone)]
struct PackData {
	title: String,
	caption: String,
	price: f64,
}

#[derive(Clone)]
struct ItemData {
	title: String,
	caption: String,
	price: f64,
	glyph: String,
}

#[derive(Clone)]
struct EventData {
	time: String,
	text: String,
	tag: String,
}

fn make_event_data(random: &mut SeededRandom, minute_offset: i64) -> EventData {
	let (message_text, message_tag) = EVENT_MESSAGES[random.next_int(0, EVENT_MESSAGES.len() as i32 - 1) as usize];
	let (hour, minute) = local_hour_minute(minute_offset);
	EventData { time: format!("{:02}:{:02}", hour, minute), text: String::from(message_text), tag: String::from(message_tag) }
}

//==============================================================================
// 팝업이 확인을 받았을 때 할 일.
//==============================================================================
#[derive(Clone, Copy)]
enum PopupAction {
	Checkout,
	ResetSettings,
	DeleteWorkspace,
}

//==============================================================================
// 타자기가 글을 넘길 라벨 어댑터.
//==============================================================================
struct LabelTarget<'a>(&'a mut UILabel);

impl TypewriterTarget for LabelTarget<'_> {
	fn set_text(&mut self, text: &str) {
		self.0.set_text(text);
	}

	fn get_text(&self) -> &str {
		self.0.get_text()
	}

	fn set_visible_character_count(&mut self, visible_character_count: i32) {
		self.0.set_visible_character_count(visible_character_count);
	}
}

//==============================================================================
// 쇼케이스 상태. (노드 번호, 테마, 목록 데이터, 팝업 전환)
//==============================================================================
struct Ui {
	theme_table: Vec<Theme>,
	theme_index: usize,
	screen_ids: Vec<NodeId>,
	navigation_id: NodeId,
	popup_id: NodeId,
	popup_dialog_id: NodeId,
	toast_id: NodeId,
	context_menu_id: NodeId,
	theme_dropdown_id: Option<NodeId>,
	region_dropdown_id: Option<NodeId>,
	loading_spinner_id: NodeId,
	demo_spinner_ids: Vec<NodeId>,
	paint_role_cache: Vec<(NodeId, usize, f64)>,
	label_role_cache: Vec<(NodeId, usize)>,
	beep_player: BeepPlayer,
	popup_motion: PopupMotion,
	popup_typewriter: Option<Typewriter>,
	demo_typewriter: Typewriter,
	popup_action: Option<PopupAction>,
	active_screen_index: usize,
	active_nav_index: usize,
	elapsed_seconds: f64,
	applied_view_size: Vector2,
	main_width: f64,
	random: SeededRandom,
	pending_theme_index: Option<usize>,
	pending_tweens: Vec<Tween>,
	long_press_button_id: Option<NodeId>,
	mini_chart_phase: f64,
	// 대시보드.
	event_list: Vec<EventData>,
	second_accumulator: f64,
	event_accumulator: f64,
	requests_per_second: f64,
	active_user_count: f64,
	revenue_value: f64,
	cpu_ratio: f64,
	memory_ratio: f64,
	// 스토어.
	featured_packs: Vec<PackData>,
	catalog_items: Vec<ItemData>,
	catalog_loaded_count: usize,
	catalog_load_remain_seconds: f64,
	cart: Vec<usize>,
	order_list: Vec<String>,
	// 처음 화면.
	scene_background: Color,
}

type Shared = Rc<RefCell<Ui>>;

//==============================================================================
// 노드 찾기와 라벨 다루기 보조.
//==============================================================================
fn find(tree: &NodeTree, root_id: NodeId, name: &str) -> Option<NodeId> {
	tree.find_descendant_by_name(root_id, name).or_else(|| if tree[root_id].get_name() == name { Some(root_id) } else { None })
}

fn set_label_text(tree: &mut NodeTree, root_id: NodeId, name: &str, text: &str) {
	if let Some(node_id) = find(tree, root_id, name) {
		if let Some(label) = tree[node_id].get_component_mut::<UILabel>() {
			label.set_text(text);
		}
	}
}

fn set_label_color(tree: &mut NodeTree, root_id: NodeId, name: &str, color: Color) {
	if let Some(node_id) = find(tree, root_id, name) {
		if let Some(label) = tree[node_id].get_component_mut::<UILabel>() {
			label.set_text_color(color);
		}
	}
}

fn set_position(tree: &mut NodeTree, root_id: NodeId, name: &str, x: f64, y: f64) {
	if let Some(node_id) = find(tree, root_id, name) {
		tree[node_id].set_local_position(Vector2::create(x, y));
	}
}

fn set_size(tree: &mut NodeTree, root_id: NodeId, name: &str, width: f64, height: f64) {
	if let Some(node_id) = find(tree, root_id, name) {
		tree[node_id].set_content_size(Vector2::create(width, height));
	}
}

fn set_paint_color(tree: &mut NodeTree, node_id: NodeId, color: Color) {
	if let Some(paint) = tree[node_id].get_component_mut::<Paint>() {
		paint.set_color(color);
	}
}

fn with_color_alpha(color: Color, alpha: f64) -> Color {
	Color::create(color.red, color.green, color.blue, alpha)
}

// 이름으로 진행바 색을 맞춘다.
fn style_progress_bar(tree: &mut NodeTree, root_id: NodeId, name: &str, theme: &Theme, fill_color: Color) {
	if let Some(node_id) = find(tree, root_id, name) {
		if let Some(progress) = tree[node_id].get_component_mut::<UIProgressView>() {
			progress.set_track_color(theme.bar_track);
			progress.set_fill_color(fill_color);
			progress.set_corner_radius(3.0);
		}
	}
}

fn set_progress_value(tree: &mut NodeTree, root_id: NodeId, name: &str, value: f64) {
	if let Some(node_id) = find(tree, root_id, name) {
		if let Some(progress) = tree[node_id].get_component_mut::<UIProgressView>() {
			progress.set_value(value);
		}
	}
}

// 노드 트리 전체 라벨에 굵기를 준다. (목록 항목은 나중에 생기므로 바인딩마다 부른다. 서체는 시스템 기본)
fn apply_font_to_tree(tree: &mut NodeTree, node_id: NodeId) {
	let node_name = tree[node_id].get_name().to_string();
	if let Some(label) = tree[node_id].get_component_mut::<UILabel>() {
		let is_bold = node_name == "Title" || node_name == "Brand" || node_name.starts_with("NavLabel") || node_name.ends_with("Value") || node_name.ends_with("Button");
		if is_bold {
			label.set_bold(true);
		}
	}
	let child_ids: Vec<NodeId> = tree.get_child_ids(node_id).to_vec();
	for child_id in child_ids {
		apply_font_to_tree(tree, child_id);
	}
}

fn format_number(value: f64) -> String {
	format::format_number(value, "en-US").unwrap_or_default()
}

fn show_toast(tree: &mut NodeTree, toast_id: NodeId, message_text: &str) {
	tree.with_component_mut::<UIToast, _>(toast_id, |toast, _tree| {
		toast.show(message_text);
	});
}

fn play_beep(shared: &Shared, preset_name: &str) {
	shared.borrow().beep_player.play_preset(preset_name);
}

// 이름으로 단추를 찾아 눌림 처리를 붙인다. (눌림 소리가 먼저 난다)
fn bind_button(tree: &mut NodeTree, shared: &Shared, root_id: NodeId, name: &str, click_handler: impl Fn(&mut NodeTree, &Shared) + 'static) {
	let button_id = match find(tree, root_id, name) {
		Some(button_id) => button_id,
		None => return,
	};
	let shared_for_click = shared.clone();
	if let Some(button) = tree[button_id].get_component_mut::<UIButton>() {
		button.set_minimum_pressed_seconds(0.05);
		button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
			play_beep(&shared_for_click, "click");
			click_handler(context.get_node_tree_mut(), &shared_for_click);
		}));
	}
}

//==============================================================================
// 대시보드 동작. (KPI, 실시간 차트, 이벤트 목록, 시스템 게이지)
//==============================================================================
fn bind_dashboard(tree: &mut NodeTree, shared: &Shared) {
	let dashboard_id = shared.borrow().screen_ids[0];
	let accent = shared.borrow().theme_table[0].colors[ROLE_ACCENT];

	// 실시간 라인 차트. (매초 push_value)
	let history_values: Vec<f64> = {
		let mut ui = shared.borrow_mut();
		(0..90).map(|sample_index| 120.0 + ui.random.next_int(0, 70) as f64 + (sample_index as f64 * 0.2).sin() * 24.0).collect()
	};
	if let Some(live_chart_id) = find(tree, dashboard_id, "LiveChart") {
		let live_chart = tree[live_chart_id].add_component(UILineChart::new());
		live_chart.set_max_sample_count(90);
		live_chart.set_line_color(accent);
		live_chart.set_value_range(Some(0.0), Some(260.0));
		live_chart.set_value_list(&history_values);
	}

	// 매출 바 차트. (14일)
	let revenue_values: Vec<f64> = {
		let mut ui = shared.borrow_mut();
		(0..14).map(|_| 2400.0 + ui.random.next_int(0, 2800) as f64).collect()
	};
	if let Some(revenue_bars_id) = find(tree, dashboard_id, "RevenueBars") {
		let revenue_bars = tree[revenue_bars_id].add_component(UIBarChart::new());
		revenue_bars.set_value_list(&revenue_values);
		revenue_bars.set_bar_color(Color::create(0.357, 0.424, 1.0, 0.45));
		revenue_bars.set_highlight_color(accent);
		revenue_bars.set_highlight_index(Some(13));
	}
	let best_value = revenue_values.iter().cloned().fold(0.0, f64::max);
	set_label_text(tree, dashboard_id, "BarCaption", &format!("Best day  ${}, highlighted: today", format_number(best_value)));

	// 이벤트 목록. (템플릿 + 데이터 -> UIListView, 주기적으로 새 이벤트가 위에 쌓인다)
	{
		let mut ui = shared.borrow_mut();
		for event_index in 0..24 {
			let event_data = make_event_data(&mut ui.random, -((event_index + 1) * 7));
			ui.event_list.push(event_data);
		}
	}
	let event_list_id = find(tree, dashboard_id, "EventList");
	let event_template_id = find(tree, dashboard_id, "EventTemplate");
	if let (Some(event_list_id), Some(event_template_id)) = (event_list_id, event_template_id) {
		let shared_for_bind = shared.clone();
		let event_count = shared.borrow().event_list.len();
		tree.add_component(event_list_id, UIListView::new());
		tree.with_component_mut::<UIListView, _>(event_list_id, |list_view, list_tree| {
			list_view.set_template_node(list_tree, event_template_id);
			list_view.set_spacing(6.0);
			list_view.set_bind_item_event(Box::new(move |bind_tree: &mut NodeTree, item_id: NodeId, item_index: usize| {
				bind_event_row(bind_tree, &shared_for_bind, item_id, item_index);
			}));
			list_view.set_item_count(event_count);
		});
	}

	// 시스템 게이지.
	{
		let ui = shared.borrow();
		let theme = &ui.theme_table[0];
		for bar_name in ["SystemCpuBar", "SystemMemoryBar", "SystemDiskBar"] {
			style_progress_bar(tree, dashboard_id, bar_name, theme, accent);
		}
	}
	if let Some(disk_bar_id) = find(tree, dashboard_id, "SystemDiskBar") {
		if let Some(disk_bar) = tree[disk_bar_id].get_component_mut::<UIProgressView>() {
			disk_bar.set_value(0.4);
			disk_bar.set_secondary_value(Some(0.08));
			disk_bar.set_secondary_fill_color(Color::create(0.851, 0.337, 0.31, 0.7));
		}
	}
	refresh_dashboard_stats(tree, shared);
}

// 이벤트 한 줄 바인딩.
fn bind_event_row(tree: &mut NodeTree, shared: &Shared, item_id: NodeId, item_index: usize) {
	let (event_data, main_width, theme_colors) = {
		let ui = shared.borrow();
		let event_data = match ui.event_list.get(item_index) {
			Some(event_data) => event_data.clone(),
			None => return,
		};
		(event_data, ui.main_width, ui.theme_table[ui.theme_index].colors)
	};
	apply_font_to_tree(tree, item_id);
	tree[item_id].set_content_size(Vector2::create(main_width - 32.0, 40.0));
	if let Some(tag_id) = find(tree, item_id, "EventTag") {
		tree[tag_id].set_local_position(Vector2::create(main_width - 132.0, 0.0));
	}
	set_paint_color(tree, item_id, theme_colors[ROLE_CARD]);
	set_label_text(tree, item_id, "EventTime", &event_data.time);
	set_label_text(tree, item_id, "EventText", &event_data.text);
	set_label_text(tree, item_id, "EventTag", &event_data.tag);
	set_label_color(tree, item_id, "EventTime", theme_colors[ROLE_TEXT_FAINT]);
	set_label_color(tree, item_id, "EventText", theme_colors[ROLE_TEXT]);
	set_label_color(tree, item_id, "EventTag", theme_colors[ROLE_TEXT_DIM]);
}

// KPI 카드 갱신.
fn refresh_dashboard_stats(tree: &mut NodeTree, shared: &Shared) {
	let (dashboard_id, revenue_value, active_user_count, requests_per_second, theme_colors) = {
		let ui = shared.borrow();
		(ui.screen_ids[0], ui.revenue_value, ui.active_user_count, ui.requests_per_second, ui.theme_table[ui.theme_index].colors)
	};
	set_label_text(tree, dashboard_id, "StatRevenueValue", &format!("${}", format_number(revenue_value.round())));
	set_stat_delta(tree, dashboard_id, "StatRevenueDelta", "+4.2% vs last month", true, &theme_colors);
	set_label_text(tree, dashboard_id, "StatUsersValue", &format_number(active_user_count));
	set_stat_delta(tree, dashboard_id, "StatUsersDelta", "+318 today", true, &theme_colors);
	set_label_text(tree, dashboard_id, "StatRequestsValue", &format_number((requests_per_second * 60.0).round()));
	set_stat_delta(tree, dashboard_id, "StatRequestsDelta", if requests_per_second >= 140.0 { "above baseline" } else { "below baseline" }, requests_per_second >= 140.0, &theme_colors);
	set_label_text(tree, dashboard_id, "StatUptimeValue", "99.98%");
	set_stat_delta(tree, dashboard_id, "StatUptimeDelta", "SLA met", true, &theme_colors);
	set_label_text(tree, dashboard_id, "ChartValue", &format!("{} rps", requests_per_second.round()));
}

fn set_stat_delta(tree: &mut NodeTree, root_id: NodeId, name: &str, text: &str, is_positive: bool, theme_colors: &[Color; 15]) {
	set_label_text(tree, root_id, name, text);
	set_label_color(tree, root_id, name, if is_positive { theme_colors[ROLE_POSITIVE] } else { theme_colors[ROLE_NEGATIVE] });
}

//==============================================================================
// 스토어 동작. (추천 띠, 카탈로그, 카트, 최근 주문)
//==============================================================================
fn bind_store(tree: &mut NodeTree, shared: &Shared) {
	let store_id = shared.borrow().screen_ids[1];

	// 추천 띠. (가로 UIListView)
	let featured_list_id = find(tree, store_id, "FeaturedList");
	let featured_template_id = find(tree, store_id, "FeaturedTemplate");
	if let (Some(featured_list_id), Some(featured_template_id)) = (featured_list_id, featured_template_id) {
		let shared_for_bind = shared.clone();
		let pack_count = shared.borrow().featured_packs.len();
		tree.add_component(featured_list_id, UIListView::new());
		tree.with_component_mut::<UIListView, _>(featured_list_id, |list_view, list_tree| {
			list_view.set_horizontal(true);
			list_view.set_template_node(list_tree, featured_template_id);
			list_view.set_spacing(10.0);
			list_view.set_bind_item_event(Box::new(move |bind_tree: &mut NodeTree, item_id: NodeId, item_index: usize| {
				let (pack_data, theme_colors) = {
					let ui = shared_for_bind.borrow();
					(ui.featured_packs[item_index].clone(), ui.theme_table[ui.theme_index].colors)
				};
				apply_font_to_tree(bind_tree, item_id);
				set_paint_color(bind_tree, item_id, with_color_alpha(theme_colors[ROLE_SURFACE], 1.0));
				set_label_text(bind_tree, item_id, "FeaturedTitle", &pack_data.title);
				set_label_text(bind_tree, item_id, "FeaturedCaption", &pack_data.caption);
				set_label_text(bind_tree, item_id, "FeaturedPrice", &format!("${:.2}", pack_data.price));
				set_label_color(bind_tree, item_id, "FeaturedTitle", theme_colors[ROLE_TEXT]);
				set_label_color(bind_tree, item_id, "FeaturedCaption", theme_colors[ROLE_TEXT_DIM]);
				set_label_color(bind_tree, item_id, "FeaturedPrice", theme_colors[ROLE_ACCENT]);
			}));
			list_view.set_item_count(pack_count);
		});
	}

	// 카탈로그. (세로 UIListView, 끝에 닿으면 다음 페이지를 불러온다)
	let item_list_id = find(tree, store_id, "ItemList");
	let item_template_id = find(tree, store_id, "ItemTemplate");
	if let (Some(item_list_id), Some(item_template_id)) = (item_list_id, item_template_id) {
		let shared_for_bind = shared.clone();
		let shared_for_end = shared.clone();
		tree.add_component(item_list_id, UIListView::new());
		tree.with_component_mut::<UIListView, _>(item_list_id, |list_view, list_tree| {
			list_view.set_template_node(list_tree, item_template_id);
			list_view.set_spacing(6.0);
			list_view.set_bind_item_event(Box::new(move |bind_tree: &mut NodeTree, item_id: NodeId, item_index: usize| {
				bind_catalog_row(bind_tree, &shared_for_bind, item_id, item_index);
			}));
			list_view.set_reach_end_event(Box::new(move |_list_view: &mut UIListView, _context: &mut ComponentContext| {
				let mut ui = shared_for_end.borrow_mut();
				if ui.catalog_loaded_count < CATALOG_TOTAL_COUNT && ui.catalog_load_remain_seconds <= 0.0 {
					ui.catalog_load_remain_seconds = CATALOG_LOAD_SECONDS;
				}
			}));
		});
	}
	apply_catalog_item_count(tree, shared);

	bind_button(tree, shared, store_id, "CartClearButton", |click_tree, click_shared| {
		if click_shared.borrow().cart.is_empty() {
			return;
		}
		click_shared.borrow_mut().cart.clear();
		refresh_cart_panel(click_tree, click_shared);
		refresh_catalog_list(click_tree, click_shared);
		let toast_id = click_shared.borrow().toast_id;
		show_toast(click_tree, toast_id, "Cart cleared.");
	});
	bind_button(tree, shared, store_id, "CartCheckoutButton", |click_tree, click_shared| {
		let (item_count, toast_id) = {
			let ui = click_shared.borrow();
			(ui.cart.len(), ui.toast_id)
		};
		if item_count == 0 {
			show_toast(click_tree, toast_id, "Cart is empty.");
			return;
		}
		let total_price = get_cart_total(click_shared);
		open_popup(click_tree, click_shared, "Confirm order", &format!("Buy {} item{} for ${:.2}?", item_count, if item_count > 1 { "s" } else { "" }, total_price), Some(PopupAction::Checkout));
	});
	refresh_cart_panel(tree, shared);
	refresh_order_panel(tree, shared);
}

fn refresh_catalog_list(tree: &mut NodeTree, shared: &Shared) {
	let store_id = shared.borrow().screen_ids[1];
	if let Some(item_list_id) = find(tree, store_id, "ItemList") {
		tree.with_component_mut::<UIListView, _>(item_list_id, |list_view, list_tree| {
			list_view.refresh_in_tree(list_tree);
		});
	}
}

// 카탈로그 한 줄 바인딩. (마지막 칸은 로딩 줄)
fn bind_catalog_row(tree: &mut NodeTree, shared: &Shared, item_id: NodeId, item_index: usize) {
	apply_font_to_tree(tree, item_id);
	let (main_width, loaded_count, loading_spinner_id, theme_colors, item_data, is_in_cart) = {
		let ui = shared.borrow();
		(ui.main_width, ui.catalog_loaded_count, ui.loading_spinner_id, ui.theme_table[ui.theme_index].colors, ui.catalog_items.get(item_index).cloned(), ui.cart.contains(&item_index))
	};
	let icon_id = find(tree, item_id, "ItemIcon");
	let add_button_id = find(tree, item_id, "ItemAddButton");
	let (icon_id, add_button_id) = match (icon_id, add_button_id) {
		(Some(icon_id), Some(add_button_id)) => (icon_id, add_button_id),
		_ => return,
	};
	let row_width = main_width - 32.0;
	tree[item_id].set_content_size(Vector2::create(row_width, 44.0));
	if let Some(price_id) = find(tree, item_id, "ItemPrice") {
		tree[price_id].set_local_position(Vector2::create(row_width - 194.0, 0.0));
	}
	tree[add_button_id].set_local_position(Vector2::create(row_width - 86.0, 9.0));

	// 로딩 줄. (다음 페이지 자리, 스피너 + 문구)
	if item_index >= loaded_count {
		tree[icon_id].set_active(false);
		tree[add_button_id].set_active(false);
		set_label_text(tree, item_id, "ItemPrice", "");
		set_label_text(tree, item_id, "ItemCaption", "");
		set_label_text(tree, item_id, "ItemTitle", "        Loading");
		set_label_color(tree, item_id, "ItemTitle", theme_colors[ROLE_TEXT_FAINT]);
		if tree.get_parent_id(loading_spinner_id) != Some(item_id) {
			tree.add_child(item_id, loading_spinner_id);
		}
		tree[loading_spinner_id].set_local_position(Vector2::create(26.0, 22.0));
		tree[loading_spinner_id].set_active(true);
		set_paint_color(tree, item_id, Color::create(0.09, 0.106, 0.141, 0.5));
		return;
	}
	if tree.get_parent_id(loading_spinner_id) == Some(item_id) {
		tree[loading_spinner_id].set_active(false);
	}
	let item_data = match item_data {
		Some(item_data) => item_data,
		None => return,
	};
	tree[icon_id].set_active(true);
	set_paint_color(tree, icon_id, theme_colors[ROLE_CARD_RAISED]);
	tree[add_button_id].set_active(true);
	set_label_text(tree, item_id, "ItemGlyph", &item_data.glyph);
	set_label_text(tree, item_id, "ItemTitle", &item_data.title);
	set_label_color(tree, item_id, "ItemTitle", theme_colors[ROLE_TEXT]);
	set_label_text(tree, item_id, "ItemCaption", &item_data.caption);
	set_label_text(tree, item_id, "ItemPrice", &format!("${:.2}", item_data.price));
	set_paint_color(tree, item_id, if is_in_cart { theme_colors[ROLE_CARD_RAISED] } else { theme_colors[ROLE_CARD] });
	set_paint_color(tree, add_button_id, if is_in_cart { theme_colors[ROLE_ACCENT] } else { theme_colors[ROLE_CARD_RAISED] });
	set_label_color(tree, item_id, "ItemGlyph", theme_colors[ROLE_TEXT_DIM]);
	set_label_color(tree, item_id, "ItemCaption", theme_colors[ROLE_TEXT_FAINT]);
	set_label_color(tree, item_id, "ItemPrice", theme_colors[ROLE_TEXT_DIM]);
	set_label_text(tree, item_id, "ItemAddButton", if is_in_cart { "\u{2713} ADDED" } else { "ADD" });
	set_label_color(tree, item_id, "ItemAddButton", if is_in_cart { theme_colors[ROLE_TEXT_ON_ACCENT] } else { theme_colors[ROLE_TEXT] });
	let shared_for_click = shared.clone();
	let item_node_id = item_id;
	bind_button(tree, shared, item_id, "ItemAddButton", move |click_tree, click_shared| {
		{
			let mut ui = click_shared.borrow_mut();
			if let Some(position) = ui.cart.iter().position(|cart_index| *cart_index == item_index) {
				ui.cart.remove(position);
			}
			else {
				ui.cart.push(item_index);
			}
		}
		bind_catalog_row(click_tree, &shared_for_click, item_node_id, item_index);
		refresh_cart_panel(click_tree, click_shared);
	});
}

// 카트 합계.
fn get_cart_total(shared: &Shared) -> f64 {
	let ui = shared.borrow();
	ui.cart.iter().fold(0.0, |total_price, item_index| total_price + ui.catalog_items[*item_index].price)
}

// 카트 패널 갱신.
fn refresh_cart_panel(tree: &mut NodeTree, shared: &Shared) {
	let store_id = shared.borrow().screen_ids[1];
	let cart_items: Vec<ItemData> = {
		let ui = shared.borrow();
		ui.cart.iter().map(|item_index| ui.catalog_items[*item_index].clone()).collect()
	};
	for row_index in 0..8usize {
		let name_node = find(tree, store_id, &format!("CartRowName{}", row_index));
		let price_node = find(tree, store_id, &format!("CartRowPrice{}", row_index));
		if name_node.is_none() || price_node.is_none() {
			continue;
		}
		let (name_text, price_text) = if row_index == 7 && cart_items.len() > 8 {
			(format!("+ {} more", cart_items.len() - 7), String::new())
		}
		else if row_index < cart_items.len() {
			(cart_items[row_index].title.clone(), format!("${:.2}", cart_items[row_index].price))
		}
		else {
			(String::from(if row_index == 0 { "Nothing selected." } else { "" }), String::new())
		};
		set_label_text(tree, store_id, &format!("CartRowName{}", row_index), &name_text);
		set_label_text(tree, store_id, &format!("CartRowPrice{}", row_index), &price_text);
	}
	let total_text = format!("${:.2}", get_cart_total(shared));
	set_label_text(tree, store_id, "CartTotalValue", &total_text);
}

// 최근 주문 패널 갱신.
fn refresh_order_panel(tree: &mut NodeTree, shared: &Shared) {
	let (store_id, order_list) = {
		let ui = shared.borrow();
		(ui.screen_ids[1], ui.order_list.clone())
	};
	if let Some(empty_id) = find(tree, store_id, "OrderEmptyCaption") {
		tree[empty_id].set_active(order_list.is_empty());
	}
	for row_index in 0..8usize {
		let row_text = order_list.get(row_index).cloned().unwrap_or_default();
		set_label_text(tree, store_id, &format!("OrderRow{}", row_index), &row_text);
	}
}

// 카탈로그 항목 수 반영. (더 남았으면 로딩 줄 하나를 덧붙인다)
fn apply_catalog_item_count(tree: &mut NodeTree, shared: &Shared) {
	let (store_id, loaded_count) = {
		let ui = shared.borrow();
		(ui.screen_ids[1], ui.catalog_loaded_count)
	};
	let has_more = loaded_count < CATALOG_TOTAL_COUNT;
	if let Some(item_list_id) = find(tree, store_id, "ItemList") {
		tree.with_component_mut::<UIListView, _>(item_list_id, |list_view, _list_tree| {
			list_view.set_item_count(loaded_count + if has_more { 1 } else { 0 });
		});
	}
	set_label_text(tree, store_id, "ItemsCountCaption", &format!("{} / {} loaded", loaded_count, CATALOG_TOTAL_COUNT));
}

//==============================================================================
// 설정 화면 동작.
//==============================================================================
fn bind_settings(tree: &mut NodeTree, shared: &Shared) {
	let settings_id = shared.borrow().screen_ids[2];

	// 슬라이더. (알림 소리는 비프 음량과 실제로 연동)
	for row_name in ["Scale", "Sound"] {
		let slider_id = match find(tree, settings_id, &format!("{}Slider", row_name)) {
			Some(slider_id) => slider_id,
			None => continue,
		};
		let caption_id = find(tree, settings_id, &format!("{}Caption", row_name));
		let shared_for_slider = shared.clone();
		if let Some(slider) = tree[slider_id].get_component_mut::<UISlider>() {
			slider.set_thumb_color(Color::create(1.0, 1.0, 1.0, 1.0));
			slider.set_value_changed_event(Box::new(move |slider: &mut UISlider, context: &mut ComponentContext| {
				let slider_value = slider.get_value();
				if let Some(caption_id) = caption_id {
					let caption_text = if row_name == "Scale" { format!("{}%", (50.0 + slider_value * 100.0).round()) } else { format!("{}%", (slider_value * 100.0).round()) };
					if let Some(caption_label) = context.get_node_tree_mut()[caption_id].get_component_mut::<UILabel>() {
						caption_label.set_text(&caption_text);
					}
				}
				if row_name == "Sound" {
					shared_for_slider.borrow_mut().beep_player.set_volume(slider_value * 0.3);
				}
			}));
		}
	}

	// 토글.
	for row_name in ["Autosave", "Telemetry"] {
		let toggle_id = match find(tree, settings_id, &format!("{}Toggle", row_name)) {
			Some(toggle_id) => toggle_id,
			None => continue,
		};
		let shared_for_toggle = shared.clone();
		if let Some(toggle) = tree[toggle_id].get_component_mut::<UIToggleButton>() {
			toggle.set_is_on(row_name == "Autosave");
			toggle.set_toggled_event(Box::new(move |toggle: &mut UIToggleButton, context: &mut ComponentContext| {
				let is_on = toggle.get_is_on();
				let (accent, toast_id) = {
					let ui = shared_for_toggle.borrow();
					(ui.theme_table[ui.theme_index].colors[ROLE_ACCENT], ui.toast_id)
				};
				let context_tree = context.get_node_tree_mut();
				set_paint_color(context_tree, toggle_id, if is_on { accent } else { Color::create(0.173, 0.204, 0.259, 1.0) });
				if let Some(label) = context_tree[toggle_id].get_component_mut::<UILabel>() {
					label.set_text(if is_on { "ON" } else { "OFF" });
				}
				show_toast(context_tree, toast_id, &format!("{}{}", row_name, if is_on { " enabled." } else { " disabled." }));
			}));
		}
	}

	// 드롭다운. (엔진 UIDropdown, 테마가 바뀌면 새 색으로 다시 만든다)
	rebuild_dropdowns(tree, shared);

	// 저장 공간 게이지. (보조 게이지 = 시스템 예약 영역)
	{
		let ui = shared.borrow();
		style_progress_bar(tree, settings_id, "StorageBar", &ui.theme_table[0], ui.theme_table[0].colors[ROLE_ACCENT]);
	}
	if let Some(storage_bar_id) = find(tree, settings_id, "StorageBar") {
		if let Some(storage_bar) = tree[storage_bar_id].get_component_mut::<UIProgressView>() {
			storage_bar.set_secondary_value(Some(0.12));
			storage_bar.set_secondary_fill_color(Color::create(0.851, 0.337, 0.31, 0.7));
		}
	}
	bind_button(tree, shared, settings_id, "ResetButton", |click_tree, click_shared| {
		open_popup(click_tree, click_shared, "Reset settings?", "All preferences go back to their defaults.", Some(PopupAction::ResetSettings));
	});
}

// 설정 드롭다운 재구성. (테마 색을 생성자에 넣으므로 전환 때마다 새로 만든다)
fn rebuild_dropdowns(tree: &mut NodeTree, shared: &Shared) {
	let (settings_id, theme_colors, theme_index, previous_theme_dropdown_id, previous_region_dropdown_id) = {
		let ui = shared.borrow();
		(ui.screen_ids[2], ui.theme_table[ui.theme_index].colors, ui.theme_index, ui.theme_dropdown_id, ui.region_dropdown_id)
	};
	let accent = theme_colors[ROLE_ACCENT];
	let make_options = || UIDropdownOptions {
		width: 176.0,
		height: 28.0,
		option_height: 26.0,
		base_color: theme_colors[ROLE_CARD_RAISED],
		popup_color: theme_colors[ROLE_SURFACE],
		highlight_color: Color::create(accent.red, accent.green, accent.blue, 0.25),
		text_color: theme_colors[ROLE_TEXT],
		..UIDropdownOptions::default()
	};

	let theme_slot_id = find(tree, settings_id, "ThemeDropdownSlot");
	if let Some(theme_slot_id) = theme_slot_id {
		let previous_theme_index = previous_theme_dropdown_id.and_then(|dropdown_id| tree[dropdown_id].get_component::<UIDropdown>()).map(|dropdown| dropdown.get_selected_index()).unwrap_or(theme_index);
		if let Some(previous_id) = previous_theme_dropdown_id {
			tree.remove_child(theme_slot_id, previous_id);
		}
		let dropdown_id = UIDropdown::create(tree, theme_slot_id, make_options());
		let shared_for_change = shared.clone();
		let option_list: Vec<&str> = {
			let ui = shared.borrow();
			ui.theme_table.iter().map(|theme| theme.label).collect()
		};
		tree.with_component_mut::<UIDropdown, _>(dropdown_id, |dropdown, dropdown_tree| {
			dropdown.set_option_list(dropdown_tree, &option_list);
			dropdown.set_selected_index(dropdown_tree, previous_theme_index);
			dropdown.set_changed_event(Box::new(move |dropdown: &mut UIDropdown, _context: &mut ComponentContext| {
				// 드롭다운 자신을 지우고 다시 만드는 일이라 이 알림 밖(다음 갱신)에서 한다.
				shared_for_change.borrow_mut().pending_theme_index = Some(dropdown.get_selected_index());
			}));
		});
		apply_font_to_tree(tree, dropdown_id);
		shared.borrow_mut().theme_dropdown_id = Some(dropdown_id);
	}

	let region_slot_id = find(tree, settings_id, "RegionDropdownSlot");
	if let Some(region_slot_id) = region_slot_id {
		let previous_region_index = previous_region_dropdown_id.and_then(|dropdown_id| tree[dropdown_id].get_component::<UIDropdown>()).map(|dropdown| dropdown.get_selected_index()).unwrap_or(0);
		if let Some(previous_id) = previous_region_dropdown_id {
			tree.remove_child(region_slot_id, previous_id);
		}
		let dropdown_id = UIDropdown::create(tree, region_slot_id, make_options());
		let shared_for_change = shared.clone();
		tree.with_component_mut::<UIDropdown, _>(dropdown_id, |dropdown, dropdown_tree| {
			dropdown.set_option_list(dropdown_tree, &["Seoul", "Tokyo", "Frankfurt", "Oregon"]);
			dropdown.set_selected_index(dropdown_tree, previous_region_index);
			dropdown.set_changed_event(Box::new(move |dropdown: &mut UIDropdown, context: &mut ComponentContext| {
				let toast_id = shared_for_change.borrow().toast_id;
				show_toast(context.get_node_tree_mut(), toast_id, &format!("Region: {}", dropdown.get_selected_text()));
			}));
		});
		apply_font_to_tree(tree, dropdown_id);
		shared.borrow_mut().region_dropdown_id = Some(dropdown_id);
	}
}

//==============================================================================
// 테마 역할 캐시 구축. (문서에 구워진 Dim 색을 역할로 되찾아 둔다, 로드 때 한 번)
//==============================================================================
fn build_theme_role_cache(tree: &mut NodeTree, shared: &Shared, root_id: NodeId) {
	let dim_colors = shared.borrow().theme_table[0].colors;
	let match_role = |color: Color| -> Option<usize> {
		for (role_index, _role_name) in ROLE_NAMES.iter().enumerate() {
			let role_color = dim_colors[role_index];
			if (color.red - role_color.red).abs() < 0.01 && (color.green - role_color.green).abs() < 0.01 && (color.blue - role_color.blue).abs() < 0.01 {
				return Some(role_index);
			}
		}
		None
	};
	let mut paint_role_cache: Vec<(NodeId, usize, f64)> = Vec::new();
	let mut label_role_cache: Vec<(NodeId, usize)> = Vec::new();
	let mut visit_stack: Vec<NodeId> = vec![root_id];
	while let Some(node_id) = visit_stack.pop() {
		if let Some(paint) = tree[node_id].get_component::<Paint>() {
			let paint_color = paint.get_color();
			if let Some(role_index) = match_role(paint_color) {
				paint_role_cache.push((node_id, role_index, paint_color.alpha));
			}
		}
		if let Some(label) = tree[node_id].get_component::<UILabel>() {
			if let Some(role_index) = match_role(label.get_text_color()) {
				label_role_cache.push((node_id, role_index));
			}
		}
		for child_id in tree.get_child_ids(node_id).iter().rev() {
			visit_stack.push(*child_id);
		}
	}

	// 액센트 판 위의 글자는 전용 역할로. (라이트 테마에서 어두운 글자가 되지 않게)
	let accent_node_list: Vec<NodeId> = paint_role_cache.iter().filter(|entry| entry.1 == ROLE_ACCENT).map(|entry| entry.0).collect();
	for entry in label_role_cache.iter_mut() {
		if (entry.1 == ROLE_TEXT || entry.1 == ROLE_INK) && accent_node_list.contains(&entry.0) {
			entry.1 = ROLE_TEXT_ON_ACCENT;
		}
	}
	let mut ui = shared.borrow_mut();
	ui.paint_role_cache = paint_role_cache;
	ui.label_role_cache = label_role_cache;
}

//==============================================================================
// 테마 적용. (역할 캐시 리매핑 + 코드 소관 위젯 재채색)
//==============================================================================
fn apply_theme(tree: &mut NodeTree, shared: &Shared, theme_index: usize) {
	if theme_index >= THEME_KEYS.len() {
		return;
	}
	shared.borrow_mut().theme_index = theme_index;
	let (colors, grid_line, paint_role_cache, label_role_cache, screen_ids, toast_id, loading_spinner_id, demo_spinner_ids, previous_context_menu_id) = {
		let mut ui = shared.borrow_mut();
		let theme = &ui.theme_table[theme_index];
		let (colors, grid_line) = (theme.colors, theme.grid_line);
		ui.scene_background = colors[0];
		(colors, grid_line, ui.paint_role_cache.clone(), ui.label_role_cache.clone(), ui.screen_ids.clone(), ui.toast_id, ui.loading_spinner_id, ui.demo_spinner_ids.clone(), ui.context_menu_id)
	};
	let accent = colors[ROLE_ACCENT];

	// 문서 색 리매핑.
	for (node_id, role_index, alpha) in paint_role_cache {
		set_paint_color(tree, node_id, with_color_alpha(colors[role_index], alpha));
	}
	for (node_id, role_index) in label_role_cache {
		if let Some(label) = tree[node_id].get_component_mut::<UILabel>() {
			label.set_text_color(with_color_alpha(colors[role_index], 1.0));
		}
	}

	// 진행바.
	let (dashboard_id, settings_id, components_id) = (screen_ids[0], screen_ids[2], screen_ids[3]);
	{
		let ui = shared.borrow();
		let theme = &ui.theme_table[theme_index];
		for bar_name in ["SystemCpuBar", "SystemMemoryBar", "SystemDiskBar"] {
			style_progress_bar(tree, dashboard_id, bar_name, theme, accent);
		}
		style_progress_bar(tree, settings_id, "StorageBar", theme, accent);
		style_progress_bar(tree, components_id, "LongPressBar", theme, accent);
		style_progress_bar(tree, components_id, "LoopProgressBar", theme, accent);
	}
	let negative_fill = Color::create(colors[ROLE_NEGATIVE].red, colors[ROLE_NEGATIVE].green, colors[ROLE_NEGATIVE].blue, 0.7);
	for (root_id, bar_name) in [(dashboard_id, "SystemDiskBar"), (settings_id, "StorageBar")] {
		if let Some(bar_id) = find(tree, root_id, bar_name) {
			if let Some(bar) = tree[bar_id].get_component_mut::<UIProgressView>() {
				bar.set_secondary_fill_color(negative_fill);
			}
		}
	}

	// 차트.
	let bar_fill = Color::create(accent.red, accent.green, accent.blue, 0.45);
	for (root_id, chart_name) in [(dashboard_id, "LiveChart"), (components_id, "MiniLineChart")] {
		if let Some(chart_id) = find(tree, root_id, chart_name) {
			if let Some(chart) = tree[chart_id].get_component_mut::<UILineChart>() {
				chart.set_line_color(accent);
				chart.set_grid_color(grid_line);
			}
		}
	}
	for (root_id, chart_name) in [(dashboard_id, "RevenueBars"), (components_id, "MiniBarChart")] {
		if let Some(chart_id) = find(tree, root_id, chart_name) {
			if let Some(chart) = tree[chart_id].get_component_mut::<UIBarChart>() {
				chart.set_bar_color(bar_fill);
				chart.set_highlight_color(accent);
			}
		}
	}

	// 토스트, 스피너, 문맥 메뉴, 드롭다운.
	set_paint_color(tree, toast_id, Color::create(colors[ROLE_SURFACE].red, colors[ROLE_SURFACE].green, colors[ROLE_SURFACE].blue, 0.97));
	if let Some(label) = tree[toast_id].get_component_mut::<UILabel>() {
		label.set_text_color(colors[ROLE_TEXT]);
	}
	if let Some(spinner) = tree[loading_spinner_id].get_component_mut::<UISpinner>() {
		spinner.set_dot_color(colors[ROLE_TEXT_DIM]);
	}
	for (spinner_index, spinner_id) in demo_spinner_ids.iter().enumerate() {
		if let Some(spinner) = tree[*spinner_id].get_component_mut::<UISpinner>() {
			spinner.set_dot_color(if spinner_index == 1 { accent } else { colors[ROLE_TEXT_DIM] });
		}
	}
	let root_id = tree.get_root_id();
	tree.remove_child(root_id, previous_context_menu_id);
	let context_menu_id = UIContextMenu::create(tree, root_id, UIContextMenuOptions { panel_color: colors[ROLE_SURFACE], text_color: colors[ROLE_TEXT], ..UIContextMenuOptions::default() });
	shared.borrow_mut().context_menu_id = context_menu_id;
	rebuild_dropdowns(tree, shared);

	// 토글은 현재 상태에 맞춰 다시 칠한다.
	for row_name in ["Autosave", "Telemetry"] {
		if let Some(toggle_id) = find(tree, settings_id, &format!("{}Toggle", row_name)) {
			let is_on = tree[toggle_id].get_component::<UIToggleButton>().map(|toggle| toggle.get_is_on()).unwrap_or(false);
			set_paint_color(tree, toggle_id, if is_on { accent } else { colors[ROLE_TOGGLE_OFF] });
			if let Some(label) = tree[toggle_id].get_component_mut::<UILabel>() {
				label.set_text_color(if is_on { colors[ROLE_TEXT_ON_ACCENT] } else { colors[ROLE_INK] });
			}
		}
	}

	// 레일 강조 글자.
	refresh_navigation_labels(tree, shared);

	// 목록은 새 색으로 다시 바인딩.
	if let Some(event_list_id) = find(tree, dashboard_id, "EventList") {
		tree.with_component_mut::<UIListView, _>(event_list_id, |list_view, list_tree| {
			list_view.refresh_in_tree(list_tree);
		});
	}
	refresh_catalog_list(tree, shared);
	refresh_dashboard_stats(tree, shared);
	LocalStorage::set_item(THEME_STORAGE_KEY, THEME_KEYS[theme_index]);
}

// 레일 글자색 갱신. (활성 탭은 액센트 판 위 글자색)
fn refresh_navigation_labels(tree: &mut NodeTree, shared: &Shared) {
	let (navigation_id, colors, active_nav_index) = {
		let ui = shared.borrow();
		(ui.navigation_id, ui.theme_table[ui.theme_index].colors, ui.active_nav_index)
	};
	for (target_index, button_name) in NAVIGATION_BUTTON_NAMES.iter().enumerate() {
		let label_name = button_name.replacen("Nav", "NavLabel", 1);
		set_label_color(tree, navigation_id, &label_name, if target_index == active_nav_index { colors[ROLE_TEXT_ON_ACCENT] } else { colors[ROLE_TEXT] });
	}
}

//==============================================================================
// 컴포넌트 화면 동작. (버튼 상태, 문맥 메뉴, 끌기, 진행, 타자기, 피드백, 차트)
//==============================================================================
fn bind_components(tree: &mut NodeTree, shared: &Shared) {
	let components_id = shared.borrow().screen_ids[3];
	let theme_colors = shared.borrow().theme_table[0].colors;
	let accent = theme_colors[ROLE_ACCENT];

	// 버튼 상태.
	bind_button(tree, shared, components_id, "NormalButton", move |click_tree, _click_shared| {
		let root_id = click_tree.get_root_id();
		set_label_text(click_tree, root_id, "ButtonsResult", "Clicked.");
	});
	if let Some(disabled_id) = find(tree, components_id, "DisabledButton") {
		if let Some(disabled_button) = tree[disabled_id].get_component_mut::<UIButton>() {
			disabled_button.set_interactable(false);
		}
		if let Some(disabled_label) = tree[disabled_id].get_component_mut::<UILabel>() {
			disabled_label.set_text_color(theme_colors[ROLE_TEXT_FAINT]);
		}
	}
	if let Some(long_press_id) = find(tree, components_id, "LongPressButton") {
		let shared_for_long = shared.clone();
		let shared_for_click = shared.clone();
		if let Some(long_press_button) = tree[long_press_id].get_component_mut::<UIButton>() {
			long_press_button.set_long_press_seconds(1.0);
			long_press_button.set_long_pressed_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				play_beep(&shared_for_long, "confirm");
				let root_id = context.get_node_tree().get_root_id();
				set_label_text(context.get_node_tree_mut(), root_id, "ButtonsResult", "Long press fired.  (click was swallowed)");
			}));
			long_press_button.set_clicked_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				play_beep(&shared_for_click, "click");
				let root_id = context.get_node_tree().get_root_id();
				set_label_text(context.get_node_tree_mut(), root_id, "ButtonsResult", "Released before 1s - normal click.");
			}));
		}
		shared.borrow_mut().long_press_button_id = Some(long_press_id);
	}
	{
		let ui = shared.borrow();
		style_progress_bar(tree, components_id, "LongPressBar", &ui.theme_table[0], accent);
	}

	// 문맥 메뉴. (오른쪽 단추는 갱신에서, 길게 누름은 버튼 이벤트로 같은 메뉴를 연다)
	if let Some(zone_id) = find(tree, components_id, "RightClickZone") {
		if tree[zone_id].get_component::<UIButton>().is_none() {
			let shared_for_zone = shared.clone();
			let zone_button = tree[zone_id].add_component(UIButton::new());
			zone_button.set_pressed_tint_color(Color::create(1.0, 1.0, 1.0, 0.04));
			zone_button.set_long_press_seconds(0.55);
			zone_button.set_long_pressed_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
				let view_position = context.get_engine().get_input_manager().get_view_input_position();
				open_context_menu_at(context.get_node_tree_mut(), &shared_for_zone, view_position);
			}));
		}
	}

	// 끌기와 놓기. (엔진 UIDraggable, 칩을 슬롯에 끌어다 놓는다)
	if let Some(drag_panel_id) = find(tree, components_id, "DragPanel") {
		let chip_names = ["A", "B", "C"];
		for (chip_index, chip_name) in chip_names.iter().enumerate() {
			let chip_id = tree.create_child(drag_panel_id, &format!("DragChip{}", chip_name));
			{
				let chip_node = &mut tree[chip_id];
				chip_node.set_pivot(Pivot::TOP_LEFT);
				chip_node.set_anchor(Pivot::TOP_LEFT);
				chip_node.set_content_size(Vector2::create(76.0, 30.0));
				chip_node.set_local_position(Vector2::create(16.0 + chip_index as f64 * 92.0, 48.0));
				let chip_paint = chip_node.add_component(Paint::create_with_color(theme_colors[ROLE_CARD_RAISED]));
				chip_paint.set_round_size(8.0);
				let chip_label = chip_node.add_component(UILabel::create_with_text(&format!("Chip {}", chip_name), 10.5));
				chip_label.set_text_color(theme_colors[ROLE_TEXT]);
			}
			let chip_name_text = String::from(*chip_name);
			let draggable = tree.add_component(chip_id, UIDraggable::new()).expect("draggable");
			draggable.set_snap_back_enabled(true);
			draggable.set_drag_end_event(Box::new(move |draggable: &mut UIDraggable, context: &mut ComponentContext, _view_position: Vector2| {
				try_drop_chip(context.get_node_tree_mut(), chip_id, draggable, drag_panel_id, &chip_name_text);
			}));
			apply_font_to_tree(tree, chip_id);
		}
	}

	// 진행 표시. (스피너 두 개 + 순환 진행바)
	if let Some(progress_panel_id) = find(tree, components_id, "ProgressPanel") {
		let small_spinner_id = UISpinner::create(tree, progress_panel_id, UISpinnerOptions { radius: 8.0, dot_radius: 1.8, color: theme_colors[ROLE_TEXT_DIM], ..UISpinnerOptions::default() });
		tree[small_spinner_id].set_local_position(Vector2::create(180.0, 68.0));
		let large_spinner_id = UISpinner::create(tree, progress_panel_id, UISpinnerOptions { radius: 13.0, dot_radius: 2.6, color: accent, rotation_speed: 0.8, ..UISpinnerOptions::default() });
		tree[large_spinner_id].set_local_position(Vector2::create(250.0, 68.0));
		shared.borrow_mut().demo_spinner_ids = vec![small_spinner_id, large_spinner_id];
	}
	{
		let ui = shared.borrow();
		style_progress_bar(tree, components_id, "LoopProgressBar", &ui.theme_table[0], accent);
	}

	// 타자기.
	if let Some(type_body_id) = find(tree, components_id, "TypeBody") {
		if let Some(type_body_label) = tree[type_body_id].get_component_mut::<UILabel>() {
			type_body_label.set_word_wrap_width(398.0);
			let mut ui = shared.borrow_mut();
			ui.demo_typewriter.start(&mut LabelTarget(type_body_label), Some(TYPEWRITER_TEXT));
		}
	}
	bind_button(tree, shared, components_id, "TypeReplayButton", |click_tree, click_shared| {
		let components_id = click_shared.borrow().screen_ids[3];
		if let Some(type_body_id) = find(click_tree, components_id, "TypeBody") {
			if let Some(type_body_label) = click_tree[type_body_id].get_component_mut::<UILabel>() {
				click_shared.borrow_mut().demo_typewriter.start(&mut LabelTarget(type_body_label), Some(TYPEWRITER_TEXT));
			}
		}
	});

	// 피드백.
	bind_button(tree, shared, components_id, "ToastButton", |click_tree, click_shared| {
		let toast_id = click_shared.borrow().toast_id;
		show_toast(click_tree, toast_id, "Saved to workspace.");
	});
	bind_button(tree, shared, components_id, "DialogButton", |click_tree, click_shared| {
		open_popup(click_tree, click_shared, "Delete workspace?", "This removes 3 projects and their history.\nThis action cannot be undone.", Some(PopupAction::DeleteWorkspace));
	});
	bind_button(tree, shared, components_id, "QueueToastButton", |click_tree, click_shared| {
		let toast_id = click_shared.borrow().toast_id;
		show_toast(click_tree, toast_id, "First toast.");
		show_toast(click_tree, toast_id, "Second toast, queued.");
		show_toast(click_tree, toast_id, "Third toast, queued.");
	});

	// 차트 데모.
	if let Some(mini_line_id) = find(tree, components_id, "MiniLineChart") {
		let mini_line_chart = tree[mini_line_id].add_component(UILineChart::new());
		mini_line_chart.set_max_sample_count(120);
		mini_line_chart.set_line_color(accent);
		mini_line_chart.set_value_range(Some(-1.3), Some(1.3));
	}
	let mini_bar_values: Vec<f64> = {
		let mut ui = shared.borrow_mut();
		(0..12).map(|_| 20.0 + ui.random.next_int(0, 80) as f64).collect()
	};
	if let Some(mini_bar_id) = find(tree, components_id, "MiniBarChart") {
		let mini_bar_chart = tree[mini_bar_id].add_component(UIBarChart::new());
		mini_bar_chart.set_bar_color(Color::create(0.357, 0.424, 1.0, 0.45));
		mini_bar_chart.set_highlight_color(accent);
		mini_bar_chart.set_value_list(&mini_bar_values);
	}
}

// 끌기 칩 놓기 판정. (슬롯 안이면 눌러앉고, 아니면 스냅백)
fn try_drop_chip(tree: &mut NodeTree, chip_id: NodeId, draggable: &mut UIDraggable, drag_panel_id: NodeId, chip_name: &str) {
	let root_id = tree.get_root_id();
	let chip_position = tree[chip_id].get_local_position();
	let chip_center_x = chip_position.x + 38.0;
	let chip_center_y = chip_position.y + 15.0;
	for slot_name in ["DragSlotA", "DragSlotB", "DragSlotC"] {
		let slot_id = match find(tree, drag_panel_id, slot_name) {
			Some(slot_id) => slot_id,
			None => continue,
		};
		let slot_position = tree[slot_id].get_local_position();
		let slot_size = tree[slot_id].get_content_size();
		if chip_center_x >= slot_position.x && chip_center_x <= slot_position.x + slot_size.x && chip_center_y >= slot_position.y && chip_center_y <= slot_position.y + slot_size.y {
			tree[chip_id].set_local_position(Vector2::create(slot_position.x + (slot_size.x - 76.0) * 0.5, slot_position.y + (slot_size.y - 30.0) * 0.5));
			draggable.settle_here(tree);
			set_label_text(tree, root_id, "DragResult", &format!("Chip {} \u{2192} {}", chip_name, slot_name.replace("DragSlot", "Slot ")));
			return;
		}
	}
	set_label_text(tree, root_id, "DragResult", "Missed - snapped back.");
}

// 문맥 메뉴 열기. (컴포넌트 화면의 지정 영역 안일 때만)
fn try_open_context_menu(tree: &mut NodeTree, shared: &Shared, view_position: Vector2) {
	let (active_screen_index, components_id) = {
		let ui = shared.borrow();
		(ui.active_screen_index, ui.screen_ids[3])
	};
	if active_screen_index != 3 {
		return;
	}
	let zone_id = match find(tree, components_id, "RightClickZone") {
		Some(zone_id) => zone_id,
		None => return,
	};
	if !is_view_position_inside_node(tree, zone_id, view_position) {
		return;
	}
	open_context_menu_at(tree, shared, view_position);
}

fn open_context_menu_at(tree: &mut NodeTree, shared: &Shared, view_position: Vector2) {
	let (context_menu_id, components_id) = {
		let ui = shared.borrow();
		(ui.context_menu_id, ui.screen_ids[3])
	};
	let make_handler = |message: &'static str| -> Box<dyn FnOnce(&mut ComponentContext)> {
		Box::new(move |context: &mut ComponentContext| {
			set_label_text(context.get_node_tree_mut(), components_id, "MenuResult", message);
		})
	};
	let item_list = vec![
		UIContextMenuItem::create("Duplicate", make_handler("Duplicate selected.")),
		UIContextMenuItem::create("Rename", make_handler("Rename selected.")),
		UIContextMenuItem::create_disabled("Share (soon)"),
		UIContextMenuItem::create("Delete", make_handler("Delete selected.")),
	];
	tree.with_component_mut::<UIContextMenu, _>(context_menu_id, |context_menu, menu_tree| {
		context_menu.show(menu_tree, view_position, item_list);
	});
	apply_font_to_tree(tree, context_menu_id);
}

//==============================================================================
// 왼쪽 레일 동작.
//==============================================================================
fn bind_navigation(tree: &mut NodeTree, shared: &Shared) {
	let navigation_id = shared.borrow().navigation_id;
	for (target_index, button_name) in NAVIGATION_BUTTON_NAMES.iter().enumerate() {
		bind_button(tree, shared, navigation_id, button_name, move |click_tree, click_shared| {
			switch_screen(click_tree, click_shared, target_index);
		});
		let button_id = find(tree, navigation_id, button_name);
		let highlight_id = find(tree, navigation_id, &button_name.replacen("Nav", "NavActive", 1));
		if let (Some(button_id), Some(highlight_id)) = (button_id, highlight_id) {
			if let Some(button) = tree[button_id].get_component_mut::<UIButton>() {
				button.exclude_node_from_tint(highlight_id);
			}
		}
	}
}

//==============================================================================
// 팝업 동작.
//==============================================================================
fn bind_popup(tree: &mut NodeTree, shared: &Shared) {
	let popup_id = shared.borrow().popup_id;
	if let Some(message_id) = find(tree, popup_id, "DialogMessage") {
		if let Some(message_label) = tree[message_id].get_component_mut::<UILabel>() {
			message_label.set_word_wrap_width(336.0);
		}
	}
	bind_button(tree, shared, popup_id, "Overlay", |click_tree, click_shared| {
		close_popup(click_tree, click_shared);
	});
	bind_button(tree, shared, popup_id, "CancelButton", |click_tree, click_shared| {
		close_popup(click_tree, click_shared);
	});
	bind_button(tree, shared, popup_id, "ConfirmButton", |click_tree, click_shared| {
		let popup_action = click_shared.borrow().popup_action;
		close_popup(click_tree, click_shared);
		if let Some(popup_action) = popup_action {
			run_popup_action(click_tree, click_shared, popup_action);
		}
	});
}

// 팝업 확인 동작.
fn run_popup_action(tree: &mut NodeTree, shared: &Shared, popup_action: PopupAction) {
	let toast_id = shared.borrow().toast_id;
	match popup_action {
		PopupAction::Checkout => {
			let item_count = shared.borrow().cart.len();
			let total_price = get_cart_total(shared);
			{
				let mut ui = shared.borrow_mut();
				ui.order_list.insert(0, format!("{} items, ${:.2}, just now", item_count, total_price));
				ui.order_list.truncate(8);
				ui.cart.clear();
			}
			refresh_cart_panel(tree, shared);
			refresh_order_panel(tree, shared);
			refresh_catalog_list(tree, shared);
			play_beep(shared, "confirm");
			show_toast(tree, toast_id, &format!("Order placed, ${:.2}", total_price));
		}
		PopupAction::ResetSettings => {
			show_toast(tree, toast_id, "Settings restored.");
		}
		PopupAction::DeleteWorkspace => {
			show_toast(tree, toast_id, "Workspace deleted.");
		}
	}
}

//==============================================================================
// 화면 전환. (내용은 떠오르고, 레일 강조는 이전 자리에서 미끄러져 온다)
//==============================================================================
fn switch_screen(tree: &mut NodeTree, shared: &Shared, screen_index: usize) {
	let (previous_screen_id, next_screen_id, previous_nav_index, navigation_id) = {
		let mut ui = shared.borrow_mut();
		if screen_index == ui.active_screen_index {
			return;
		}
		let previous_screen_id = ui.screen_ids[ui.active_screen_index];
		let previous_nav_index = ui.active_nav_index;
		ui.active_screen_index = screen_index;
		ui.active_nav_index = screen_index;
		(previous_screen_id, ui.screen_ids[screen_index], previous_nav_index, ui.navigation_id)
	};
	tree[previous_screen_id].set_active(false);
	tree[next_screen_id].set_active(true);
	tree[next_screen_id].set_local_opacity(0.0);
	tree[next_screen_id].set_local_position(Vector2::create(0.0, 14.0));
	let content_tween = Tween::new(TweenValues::new().with("progress", 0.0))
		.to(TweenValues::new().with("progress", 1.0), 0.26)
		.easing(easing_function::quadratic::ease_out)
		.set_update(move |values, update_tree| {
			let progress = values.get_number("progress").unwrap_or(1.0);
			update_tree[next_screen_id].set_local_opacity(progress);
			update_tree[next_screen_id].set_local_position(Vector2::create(0.0, 14.0 * (1.0 - progress)));
		})
		.set_complete(move |complete_tree| {
			complete_tree[next_screen_id].set_local_opacity(1.0);
			complete_tree[next_screen_id].set_local_position(Vector2::create(0.0, 0.0));
		});
	shared.borrow_mut().pending_tweens.push(content_tween);

	// 레일 강조 알약 슬라이드.
	for (target_index, button_name) in NAVIGATION_BUTTON_NAMES.iter().enumerate() {
		if let Some(highlight_id) = find(tree, navigation_id, &button_name.replacen("Nav", "NavActive", 1)) {
			tree[highlight_id].set_active(target_index == screen_index);
		}
	}
	refresh_navigation_labels(tree, shared);
	if let Some(active_highlight_id) = find(tree, navigation_id, &NAVIGATION_BUTTON_NAMES[screen_index].replacen("Nav", "NavActive", 1)) {
		let start_offset_y = (previous_nav_index as f64 - screen_index as f64) * 42.0;
		tree[active_highlight_id].set_local_position(Vector2::create(0.0, start_offset_y));
		let slide_tween = Tween::new(TweenValues::new().with("progress", 0.0))
			.to(TweenValues::new().with("progress", 1.0), 0.22)
			.easing(easing_function::quadratic::ease_out)
			.set_update(move |values, update_tree| {
				let progress = values.get_number("progress").unwrap_or(1.0);
				update_tree[active_highlight_id].set_local_position(Vector2::create(0.0, start_offset_y * (1.0 - progress)));
			})
			.set_complete(move |complete_tree| {
				complete_tree[active_highlight_id].set_local_position(Vector2::create(0.0, 0.0));
			});
		shared.borrow_mut().pending_tweens.push(slide_tween);
	}
}

//==============================================================================
// 팝업 열기.
//==============================================================================
fn open_popup(tree: &mut NodeTree, shared: &Shared, title_text: &str, message_text: &str, popup_action: Option<PopupAction>) {
	let popup_id = shared.borrow().popup_id;
	let cancel_id = find(tree, popup_id, "CancelButton");
	let confirm_id = find(tree, popup_id, "ConfirmButton");
	set_label_text(tree, popup_id, "DialogTitle", title_text);
	shared.borrow_mut().popup_action = popup_action;

	// 확인만 있는 알림이면 취소를 감추고 확인을 가운데로 넓힌다.
	let has_confirm = popup_action.is_some();
	if let (Some(cancel_id), Some(confirm_id)) = (cancel_id, confirm_id) {
		tree[cancel_id].set_active(has_confirm);
		tree[confirm_id].set_local_position(if has_confirm { Vector2::create(196.0, 134.0) } else { Vector2::create(22.0, 134.0) });
		tree[confirm_id].set_content_size(if has_confirm { Vector2::create(162.0, 40.0) } else { Vector2::create(336.0, 40.0) });
		set_label_text(tree, confirm_id, "ConfirmButton", if has_confirm { "CONFIRM" } else { "OK" });
	}

	// 메시지는 타자기 효과로 드러낸다. (엔진 Typewriter)
	if let Some(message_id) = find(tree, popup_id, "DialogMessage") {
		if let Some(message_label) = tree[message_id].get_component_mut::<UILabel>() {
			let mut typewriter = Typewriter::new(TypewriterOptions { characters_per_second: 110.0, ..TypewriterOptions::default() });
			typewriter.start(&mut LabelTarget(message_label), Some(message_text));
			shared.borrow_mut().popup_typewriter = Some(typewriter);
		}
	}
	tree[popup_id].set_active(true);
	shared.borrow_mut().popup_motion.open();
	apply_popup_motion(tree, shared);
}

// 팝업 전환 산출값 반영. (막 알파 + 대화 상자 배율)
fn apply_popup_motion(tree: &mut NodeTree, shared: &Shared) {
	let (popup_id, popup_dialog_id, dim_ratio, dialog_scale) = {
		let ui = shared.borrow();
		(ui.popup_id, ui.popup_dialog_id, ui.popup_motion.get_dim_ratio(), ui.popup_motion.get_scale())
	};
	tree[popup_id].set_local_opacity(dim_ratio);
	tree[popup_dialog_id].set_local_scale(Vector2::create(dialog_scale, dialog_scale));
}

// 팝업 닫기.
fn close_popup(tree: &mut NodeTree, shared: &Shared) {
	let popup_id = shared.borrow().popup_id;
	if !tree[popup_id].is_active() {
		return;
	}
	let mut ui = shared.borrow_mut();
	ui.popup_action = None;
	ui.popup_motion.close();
}

//==============================================================================
// 반응형 자리 반영. (화면 크기가 바뀔 때만, 우측 패널을 화면 오른쪽 끝에 붙인다)
//==============================================================================
fn apply_responsive_layout(tree: &mut NodeTree, shared: &Shared, view_size: Vector2) {
	{
		let ui = shared.borrow();
		if view_size.x == ui.applied_view_size.x && view_size.y == ui.applied_view_size.y {
			return;
		}
	}
	let (screen_ids, navigation_id, popup_id, popup_dialog_id, toast_id) = {
		let mut ui = shared.borrow_mut();
		ui.applied_view_size = view_size;
		(ui.screen_ids.clone(), ui.navigation_id, ui.popup_id, ui.popup_dialog_id, ui.toast_id)
	};
	let view_width = view_size.x;
	let view_height = view_size.y;
	let root_id = tree.get_root_id();
	tree[root_id].set_content_size(Vector2::create(view_width, view_height));

	// 왼쪽 레일. (세로 전체)
	tree[navigation_id].set_content_size(Vector2::create(200.0, view_height));
	set_size(tree, navigation_id, "RailHairline", 1.0, view_height);
	set_position(tree, navigation_id, "RailVersion", 22.0, view_height - 36.0);

	// 화면 문서. (배경이 화면 전체를 덮고, 본문 컬럼은 남는 폭을 전부 쓴다)
	let side_panel_names: [&[&str]; 4] = [&["BarPanel", "SystemPanel"], &["CartPanel", "OrderPanel"], &["AboutPanel"], &["DragPanel", "FeedbackPanel"]];
	let side_x = view_width - SIDE_PANEL_OFFSET;
	let main_width = side_x - 224.0 - 16.0;
	let full_width = view_width - 224.0 - 16.0;
	shared.borrow_mut().main_width = main_width;
	for (screen_index, screen_id) in screen_ids.iter().enumerate() {
		tree[*screen_id].set_content_size(Vector2::create(view_width, view_height));
		set_position(tree, *screen_id, "StatusChip", view_width - 260.0, 26.0);
		for panel_name in side_panel_names[screen_index] {
			if let Some(panel_id) = find(tree, *screen_id, panel_name) {
				let panel_position = tree[panel_id].get_local_position();
				tree[panel_id].set_local_position(Vector2::create(side_x, panel_position.y));
			}
		}
	}
	apply_fluid_dashboard(tree, shared, screen_ids[0], main_width);
	apply_fluid_store(tree, shared, screen_ids[1], main_width, full_width);
	apply_fluid_settings(tree, screen_ids[2], main_width);
	apply_fluid_components(tree, screen_ids[3], main_width, full_width);

	// 팝업, 토스트.
	if let Some(overlay_id) = find(tree, popup_id, "Overlay") {
		tree[overlay_id].set_content_size(Vector2::create(view_width, view_height));
	}
	tree[popup_id].set_content_size(Vector2::create(view_width, view_height));
	tree[popup_dialog_id].set_local_position(Vector2::create(view_width * 0.5, view_height * 0.5 - 10.0));
	tree.with_component_mut::<UIToast, _>(toast_id, |toast, _toast_tree| {
		toast.set_rest_x(Some((224.0 + view_width) * 0.5));
		toast.set_rest_y(view_height - 52.0);
	});
}

// 가변 폭: 대시보드. (KPI 카드 4분할, 차트 패널, 이벤트 목록)
fn apply_fluid_dashboard(tree: &mut NodeTree, shared: &Shared, dashboard_id: NodeId, main_width: f64) {
	let card_width = (main_width - 48.0) / 4.0;
	for (stat_index, stat_name) in ["Revenue", "Users", "Requests", "Uptime"].iter().enumerate() {
		let card_name = format!("Stat{}", stat_name);
		set_position(tree, dashboard_id, &card_name, 224.0 + stat_index as f64 * (card_width + 16.0), 82.0);
		set_size(tree, dashboard_id, &card_name, card_width, 78.0);
	}
	set_size(tree, dashboard_id, "ChartPanel", main_width, 300.0);
	set_position(tree, dashboard_id, "ChartValue", main_width - 156.0, 8.0);
	set_size(tree, dashboard_id, "LiveChart", main_width - 32.0, 244.0);
	set_size(tree, dashboard_id, "EventList", main_width, 458.0);
	if let Some(event_list_id) = find(tree, dashboard_id, "EventList") {
		let event_count = shared.borrow().event_list.len();
		tree.with_component_mut::<UIListView, _>(event_list_id, |list_view, list_tree| {
			list_view.set_item_count(event_count);
			list_view.refresh_in_tree(list_tree);
		});
	}
}

// 가변 폭: 스토어. (추천 띠, 카탈로그 목록)
fn apply_fluid_store(tree: &mut NodeTree, shared: &Shared, store_id: NodeId, main_width: f64, full_width: f64) {
	set_size(tree, store_id, "FeaturedList", full_width, 96.0);
	set_size(tree, store_id, "ItemList", main_width, 732.0);
	apply_catalog_item_count(tree, shared);
	refresh_catalog_list(tree, shared);
}

// 가변 폭: 설정. (행과 오른쪽 컨트롤 자리)
fn apply_fluid_settings(tree: &mut NodeTree, settings_id: NodeId, main_width: f64) {
	set_size(tree, settings_id, "SettingsPanel", main_width, 420.0);
	let row_width = main_width - 32.0;
	for row_name in ["Scale", "Sound", "Autosave", "Telemetry", "Theme", "Region", "Storage"] {
		set_size(tree, settings_id, &format!("Row{}", row_name), row_width, 40.0);
	}
	for slider_name in ["Scale", "Sound"] {
		set_position(tree, settings_id, &format!("{}Slider", slider_name), row_width - 288.0, 9.0);
	}
	for toggle_name in ["Autosave", "Telemetry"] {
		set_position(tree, settings_id, &format!("{}Toggle", toggle_name), row_width - 72.0, 9.0);
	}
	for slot_name in ["Theme", "Region"] {
		set_position(tree, settings_id, &format!("{}DropdownSlot", slot_name), row_width - 200.0, 6.0);
	}
	set_position(tree, settings_id, "StorageBar", row_width - 288.0, 16.0);
	set_size(tree, settings_id, "ResetButton", row_width, 32.0);
}

// 가변 폭: 컴포넌트. (두 열 패널, 차트 패널 반분할)
fn apply_fluid_components(tree: &mut NodeTree, components_id: NodeId, main_width: f64, full_width: f64) {
	let half_width = (main_width - 16.0) / 2.0;
	let second_column_x = 224.0 + half_width + 16.0;
	set_size(tree, components_id, "ButtonsPanel", half_width, 262.0);
	set_position(tree, components_id, "MenuPanel", second_column_x, 82.0);
	set_size(tree, components_id, "MenuPanel", half_width, 262.0);
	set_size(tree, components_id, "RightClickZone", half_width - 32.0, 150.0);
	set_size(tree, components_id, "RightClickHint", half_width - 32.0, 150.0);
	set_size(tree, components_id, "ProgressPanel", half_width, 220.0);
	set_size(tree, components_id, "LoopProgressBar", half_width - 32.0, 7.0);
	set_position(tree, components_id, "TypePanel", second_column_x, 360.0);
	set_size(tree, components_id, "TypePanel", half_width, 220.0);
	set_size(tree, components_id, "TypeBody", half_width - 32.0, 110.0);
	if let Some(type_body_id) = find(tree, components_id, "TypeBody") {
		if let Some(type_body_label) = tree[type_body_id].get_component_mut::<UILabel>() {
			type_body_label.set_word_wrap_width(half_width - 32.0);
		}
	}
	set_size(tree, components_id, "ChartsPanel", full_width, 380.0);
	let chart_width = (full_width - 48.0) / 2.0;
	set_size(tree, components_id, "MiniLineChart", chart_width, 290.0);
	set_position(tree, components_id, "MiniBarCaption", 16.0 + chart_width + 16.0, 36.0);
	set_position(tree, components_id, "MiniBarChart", 16.0 + chart_width + 16.0, 58.0);
	set_size(tree, components_id, "MiniBarChart", chart_width, 290.0);
}

//==============================================================================
// 갱신. (실시간 지표, 팝업 전환, 타자기, 로딩, 반응형 자리)
//==============================================================================
fn update(tree: &mut NodeTree, shared: &Shared, engine: &Engine, time_delta: f64) {
	// 드롭다운이 맡긴 테마 전환.
	let pending_theme_index = shared.borrow_mut().pending_theme_index.take();
	if let Some(theme_index) = pending_theme_index {
		apply_theme(tree, shared, theme_index);
		let (toast_id, theme_label) = {
			let ui = shared.borrow();
			(ui.toast_id, ui.theme_table[theme_index].label)
		};
		show_toast(tree, toast_id, &format!("Theme: {}", theme_label));
	}
	shared.borrow_mut().elapsed_seconds += time_delta;
	let view_size = engine.get_view_manager().get_view_size();
	apply_responsive_layout(tree, shared, view_size);

	// 오른쪽 단추는 컴포넌트 화면의 지정 영역에서만 문맥 메뉴를 연다.
	let (is_right_pressed, view_input_position) = {
		let input_manager = engine.get_input_manager();
		(input_manager.is_mouse_button_just_pressed(MouseButton::Right), input_manager.get_view_input_position())
	};
	if is_right_pressed {
		try_open_context_menu(tree, shared, view_input_position);
	}

	let (popup_id, components_id, dashboard_id) = {
		let ui = shared.borrow();
		(ui.popup_id, ui.screen_ids[3], ui.screen_ids[0])
	};

	// 타자기. (팝업 메시지, 컴포넌트 화면 본문)
	{
		let popup_message_id = find(tree, popup_id, "DialogMessage");
		let mut ui = shared.borrow_mut();
		if let (Some(message_id), Some(typewriter)) = (popup_message_id, ui.popup_typewriter.as_mut()) {
			if let Some(message_label) = tree[message_id].get_component_mut::<UILabel>() {
				typewriter.tick(&mut LabelTarget(message_label), time_delta);
			}
		}
	}
	if let Some(type_body_id) = find(tree, components_id, "TypeBody") {
		if let Some(type_body_label) = tree[type_body_id].get_component_mut::<UILabel>() {
			shared.borrow_mut().demo_typewriter.tick(&mut LabelTarget(type_body_label), time_delta);
		}
	}

	// 팝업 전환.
	if tree[popup_id].is_active() {
		{
			shared.borrow_mut().popup_motion.tick(time_delta);
		}
		apply_popup_motion(tree, shared);
		if !shared.borrow().popup_motion.is_visible() {
			tree[popup_id].set_active(false);
		}
	}

	// 매초 지표 갱신. (실시간 라인 차트, KPI)
	let (second_ready, requests_per_second) = {
		let mut ui = shared.borrow_mut();
		ui.second_accumulator += time_delta;
		if ui.second_accumulator >= 1.0 {
			ui.second_accumulator -= 1.0;
			let next_requests = ui.requests_per_second + (random_unit() - 0.48) * 30.0;
			ui.requests_per_second = next_requests.min(250.0).max(40.0);
			ui.active_user_count += ((random_unit() - 0.35) * 9.0).round();
			ui.revenue_value += random_unit() * 14.0;
			(true, ui.requests_per_second)
		}
		else {
			(false, ui.requests_per_second)
		}
	};
	if second_ready {
		if let Some(live_chart_id) = find(tree, dashboard_id, "LiveChart") {
			if let Some(live_chart) = tree[live_chart_id].get_component_mut::<UILineChart>() {
				live_chart.push_value(requests_per_second);
			}
		}
		refresh_dashboard_stats(tree, shared);
		let elapsed_seconds = shared.borrow().elapsed_seconds;
		if let Some(mini_bar_id) = find(tree, components_id, "MiniBarChart") {
			if let Some(mini_bar_chart) = tree[mini_bar_id].get_component_mut::<UIBarChart>() {
				mini_bar_chart.set_highlight_index(Some((elapsed_seconds.floor() as usize) % 12));
			}
		}
	}

	// 이벤트 목록에 주기적으로 새 항목이 위에 쌓인다.
	let event_ready = {
		let mut ui = shared.borrow_mut();
		ui.event_accumulator += time_delta;
		if ui.event_accumulator >= 5.0 {
			ui.event_accumulator -= 5.0;
			let mut random = std::mem::replace(&mut ui.random, SeededRandom::new(0));
			let event_data = make_event_data(&mut random, 0);
			ui.random = random;
			ui.event_list.insert(0, event_data);
			true
		}
		else {
			false
		}
	};
	if event_ready {
		if let Some(event_list_id) = find(tree, dashboard_id, "EventList") {
			let event_count = shared.borrow().event_list.len();
			tree.with_component_mut::<UIListView, _>(event_list_id, |list_view, list_tree| {
				list_view.set_item_count(event_count);
				list_view.refresh_in_tree(list_tree);
			});
		}
	}

	// 시스템 게이지 흔들림.
	if tree[dashboard_id].is_active() {
		let (cpu_ratio, memory_ratio) = {
			let mut ui = shared.borrow_mut();
			let elapsed_seconds = ui.elapsed_seconds;
			let requests_per_second = ui.requests_per_second;
			ui.cpu_ratio += ((0.28 + (elapsed_seconds * 0.7).sin() * 0.1 + requests_per_second * 0.0008) - ui.cpu_ratio) * (time_delta * 2.0).min(1.0);
			ui.memory_ratio += ((0.55 + (elapsed_seconds * 0.13).sin() * 0.05) - ui.memory_ratio) * time_delta.min(1.0);
			(ui.cpu_ratio, ui.memory_ratio)
		};
		set_progress_value(tree, dashboard_id, "SystemCpuBar", cpu_ratio);
		set_progress_value(tree, dashboard_id, "SystemMemoryBar", memory_ratio);
	}

	// 카탈로그 다음 페이지 로딩. (짧은 지연으로 스피너 로딩 줄을 보여 준다)
	let catalog_loaded = {
		let mut ui = shared.borrow_mut();
		if ui.catalog_load_remain_seconds > 0.0 {
			ui.catalog_load_remain_seconds -= time_delta;
			if ui.catalog_load_remain_seconds <= 0.0 {
				ui.catalog_load_remain_seconds = 0.0;
				ui.catalog_loaded_count = CATALOG_TOTAL_COUNT.min(ui.catalog_loaded_count + CATALOG_PAGE_COUNT);
				true
			}
			else {
				false
			}
		}
		else {
			false
		}
	};
	if catalog_loaded {
		apply_catalog_item_count(tree, shared);
		refresh_catalog_list(tree, shared);
	}

	// 길게 누름 게이지, 순환 진행바, 흐르는 사인 라인 차트.
	if tree[components_id].is_active() {
		let long_press_button_id = shared.borrow().long_press_button_id;
		if let Some(long_press_button_id) = long_press_button_id {
			let hold_ratio = tree[long_press_button_id].get_component::<UIButton>().map(|button| (button.get_pressed_elapsed_seconds() / button.get_long_press_seconds()).min(1.0)).unwrap_or(0.0);
			set_progress_value(tree, components_id, "LongPressBar", hold_ratio);
		}
		let elapsed_seconds = shared.borrow().elapsed_seconds;
		let loop_ratio = (elapsed_seconds % 3.0) / 3.0;
		set_progress_value(tree, components_id, "LoopProgressBar", loop_ratio);
		set_label_text(tree, components_id, "LoopProgressValue", &format!("{}%", (loop_ratio * 100.0).round()));
		let mini_chart_phase = {
			let mut ui = shared.borrow_mut();
			ui.mini_chart_phase += time_delta * 2.0;
			ui.mini_chart_phase
		};
		if let Some(mini_line_id) = find(tree, components_id, "MiniLineChart") {
			if let Some(mini_line_chart) = tree[mini_line_id].get_component_mut::<UILineChart>() {
				let wave_values: Vec<f64> = (0..120).map(|sample_index| (sample_index as f64 * 0.12 + mini_chart_phase).sin() * (0.6 + 0.35 * (sample_index as f64 * 0.031 + mini_chart_phase * 0.4).sin())).collect();
				mini_line_chart.set_value_list(&wave_values);
			}
		}
	}
}

// 0 이상 1 미만 난수. (원본 Math.random, 지표 흔들림 전용이라 시드를 쓰지 않는다)
fn random_unit() -> f64 {
	thread_local! {
		static STATE: std::cell::Cell<u64> = std::cell::Cell::new(0x9E37_79B9_7F4A_7C15 ^ local_seed());
	}
	STATE.with(|state| {
		let mut value = state.get();
		value ^= value << 13;
		value ^= value >> 7;
		value ^= value << 17;
		state.set(value);
		(value >> 11) as f64 / (1u64 << 53) as f64
	})
}

fn local_seed() -> u64 {
	let (hour, minute) = local_hour_minute(0);
	(hour as u64 * 60 + minute as u64) | 1
}

//==============================================================================
// UI 쇼케이스 씬.
//==============================================================================
struct ShowcaseScene {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	document_assets: Vec<TextAsset>,
	shared: Option<Shared>,
	start_screen_index: usize,
	background_color: Color,
}

impl ShowcaseScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> ShowcaseScene {
		let start_screen_index = std::env::var("VANILLA_EXAMPLE_MODE").ok().and_then(|requested| SCREEN_NAMES.iter().position(|screen_name| *screen_name == requested)).unwrap_or(0);
		ShowcaseScene {
			scene_base: SceneBase::new(),
			touch_recognizer: TouchRecognizer::new(),
			document_assets: ASSET_FILE_NAMES.iter().map(|_| TextAsset::new()).collect(),
			shared: None,
			start_screen_index,
			background_color: Color::create(0.078, 0.09, 0.118, 1.0),
		}
	}
}

impl Scene for ShowcaseScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (uiasset 여섯을 요청해 맡긴다)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		for (asset_index, file_name) in ASSET_FILE_NAMES.iter().enumerate() {
			self.document_assets[asset_index].load(engine, &format!("{}.uiasset.json", file_name));
			self.scene_base.track_asset(&self.document_assets[asset_index]);
		}
	}

	//==============================================================================
	// 초기화. (문서를 노드 트리로 복원하고 이름으로 찾아 동작을 붙인다)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		self.scene_base.set_scene_background_color(self.background_color);
		self.scene_base.get_root_mut().set_pivot(Pivot::TOP_LEFT);
		self.scene_base.get_root_mut().set_anchor(Pivot::TOP_LEFT);
		let root_id = self.scene_base.get_root_id();
		let mut document_text_list: Vec<String> = Vec::new();
		for (asset_index, file_name) in ASSET_FILE_NAMES.iter().enumerate() {
			match self.document_assets[asset_index].get_text() {
				Some(document_text) => document_text_list.push(document_text.to_string()),
				None => {
					eprintln!("uishowcase: uiasset 파일을 읽지 못했습니다 ({})", file_name);
					return;
				}
			}
		}
		let node_tree = self.scene_base.get_node_tree_mut();
		let mut document = UIDocument::new();
		let mut document_root_ids: Vec<NodeId> = Vec::new();
		for (asset_index, document_text) in document_text_list.iter().enumerate() {
			match document.from_json_text(node_tree, document_text) {
				Ok(document_root_id) => document_root_ids.push(document_root_id),
				Err(error) => {
					eprintln!("uishowcase: {} ({})", error, ASSET_FILE_NAMES[asset_index]);
					return;
				}
			}
		}
		let screen_ids: Vec<NodeId> = document_root_ids[0..4].to_vec();
		for (screen_index, screen_id) in screen_ids.iter().enumerate() {
			node_tree.add_child(root_id, *screen_id);
			node_tree[*screen_id].set_active(screen_index == 0);
		}
		let navigation_id = document_root_ids[4];
		node_tree.add_child(root_id, navigation_id);
		let popup_id = document_root_ids[5];
		node_tree[popup_id].set_active(false);
		node_tree.add_child(root_id, popup_id);
		let popup_dialog_id = find(node_tree, popup_id, "Dialog").expect("popup dialog");

		// 토스트. (큐, 등장, 퇴장을 엔진 UIToast 가 맡는다)
		let theme_table = create_theme_table();
		let toast_id = UIToast::create(
			node_tree,
			root_id,
			UIToastOptions {
				width: 340.0,
				height: 36.0,
				font_size: 12.0,
				hold_seconds: 1.8,
				background_color: Color::create(0.071, 0.086, 0.125, 0.97),
				text_color: theme_table[0].colors[ROLE_TEXT],
				..UIToastOptions::default()
			},
		);

		// 문맥 메뉴. (맨 위에 그려지도록 마지막에 붙인다)
		let context_menu_id = UIContextMenu::create(node_tree, root_id, UIContextMenuOptions::default());

		// 로딩 줄 스피너. (처음엔 어디에도 붙지 않은 것처럼 감춰 둔다)
		let loading_spinner_id = UISpinner::create(node_tree, root_id, UISpinnerOptions { radius: 8.0, dot_radius: 1.8, color: theme_table[0].colors[ROLE_TEXT_DIM], ..UISpinnerOptions::default() });
		node_tree[loading_spinner_id].set_active(false);

		// 데이터. (시드를 고정해 원본과 같은 목록이 나온다)
		let mut random = SeededRandom::new(20260901);
		let pack_names = ["Team Bundle", "Pro Toolkit", "Analytics Pack", "Security Suite", "CI Booster", "Design Kit", "Data Bridge", "API Gateway", "Backup Plus", "Insight Pack", "Workflow Kit", "Edge Cache"];
		let featured_packs: Vec<PackData> = pack_names
			.iter()
			.map(|pack_name| {
				let add_on_count = random.next_int(3, 9);
				let price = random.next_int(19, 89) as f64 + 0.99;
				PackData { title: String::from(*pack_name), caption: format!("Bundle, {} add-ons", add_on_count), price }
			})
			.collect();
		let prefix_names = ["Auto", "Smart", "Quick", "Pro", "Team", "Cloud", "Secure", "Batch", "Live", "Meta"];
		let tool_names = ["Backup", "Formatter", "Linter", "Themes", "Sync", "Deploy", "Monitor", "Export", "Webhooks", "Search"];
		let category_names = ["Productivity", "Integration", "Developer tools", "Analytics", "Security"];
		let mut catalog_items: Vec<ItemData> = Vec::new();
		for item_index in 0..CATALOG_TOTAL_COUNT {
			let item_title = format!("{} {}", prefix_names[random.next_int(0, prefix_names.len() as i32 - 1) as usize], tool_names[item_index % tool_names.len()]);
			let category_name = category_names[random.next_int(0, category_names.len() as i32 - 1) as usize];
			let major = random.next_int(1, 4);
			let minor = random.next_int(0, 9);
			let price = random.next_int(2, 49) as f64 + 0.99;
			catalog_items.push(ItemData { glyph: item_title.chars().next().map(String::from).unwrap_or_default(), title: item_title, caption: format!("{}, v{}.{}", category_name, major, minor), price });
		}

		let beep_player = BeepPlayer::new(&engine.get_audio_manager(), 0.18);
		let ui = Ui {
			theme_table,
			theme_index: 0,
			screen_ids,
			navigation_id,
			popup_id,
			popup_dialog_id,
			toast_id,
			context_menu_id,
			theme_dropdown_id: None,
			region_dropdown_id: None,
			loading_spinner_id,
			demo_spinner_ids: Vec::new(),
			paint_role_cache: Vec::new(),
			label_role_cache: Vec::new(),
			beep_player,
			popup_motion: PopupMotion::new(),
			popup_typewriter: None,
			demo_typewriter: Typewriter::new(TypewriterOptions { characters_per_second: 45.0, ..TypewriterOptions::default() }),
			popup_action: None,
			active_screen_index: 0,
			active_nav_index: 0,
			elapsed_seconds: 0.0,
			applied_view_size: Vector2::create(0.0, 0.0),
			main_width: 884.0,
			random,
			pending_theme_index: None,
			pending_tweens: Vec::new(),
			long_press_button_id: None,
			mini_chart_phase: 0.0,
			event_list: Vec::new(),
			second_accumulator: 0.0,
			event_accumulator: 0.0,
			requests_per_second: 142.0,
			active_user_count: 12847.0,
			revenue_value: 182540.0,
			cpu_ratio: 0.34,
			memory_ratio: 0.58,
			featured_packs,
			catalog_items,
			catalog_loaded_count: CATALOG_INITIAL_COUNT,
			catalog_load_remain_seconds: 0.0,
			cart: Vec::new(),
			order_list: Vec::new(),
			scene_background: Color::create(0.078, 0.09, 0.118, 1.0),
		};
		let shared: Shared = Rc::new(RefCell::new(ui));
		bind_dashboard(node_tree, &shared);
		bind_store(node_tree, &shared);
		bind_settings(node_tree, &shared);
		bind_components(node_tree, &shared);
		bind_navigation(node_tree, &shared);
		bind_popup(node_tree, &shared);
		apply_font_to_tree(node_tree, root_id);
		build_theme_role_cache(node_tree, &shared, root_id);
		let saved_theme_index = LocalStorage::get_item(THEME_STORAGE_KEY).and_then(|stored_key| THEME_KEYS.iter().position(|theme_key| *theme_key == stored_key)).unwrap_or(0);
		apply_theme(node_tree, &shared, saved_theme_index);
		if self.start_screen_index != 0 {
			switch_screen(node_tree, &shared, self.start_screen_index);
		}
		self.shared = Some(shared);
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		let shared = match &self.shared {
			Some(shared) => shared.clone(),
			None => return,
		};
		update(self.scene_base.get_node_tree_mut(), &shared, engine, time_delta);
		let pending_tween_list: Vec<Tween> = std::mem::take(&mut shared.borrow_mut().pending_tweens);
		for pending_tween in pending_tween_list {
			self.scene_base.start_tween(pending_tween);
		}
		self.background_color = shared.borrow().scene_background;
		let node_tree = self.scene_base.get_node_tree_mut();
		node_tree.tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
		self.scene_base.tick_tweens(time_delta);
	}

	//==============================================================================
	// 출력 전 처리. (테마 배경을 채운다)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.set_scene_background_color(self.background_color);
		self.scene_base.draw_scene_background(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("UI 쇼케이스 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.window_size = Vector2::create(1260.0, 750.0);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShortExpandLong;
	engine_configuration.asset_directory_paths = vec![PathBuf::from(ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ShowcaseScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
