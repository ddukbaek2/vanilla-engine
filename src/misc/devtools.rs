//==============================================================================
// 개발자 도구. (원본 misc/devtools.js 의 DEVTools)
// - F2 키로 보이고 숨긴다. 패널은 뷰 스케일 모드와 상관없이 창 안쪽 논리 좌표(canvas native)로 오른쪽 아래에 붙어 나온다.
// - 탭 넷: Statistics(엔진, 화면, 시간 정보), Node Hierarchy(왼쪽 트리 + 오른쪽 인스펙터), Local Storage(키와 값 보기, 더하기, 고치기, 지우기),
//   Settings(바탕 어둡게, 모든 기즈모, 화면 비율 가이드, FPS 표시. 로컬 스토리지 "hierarchy.settings" 에 원본과 같은 JSON 으로 남긴다)
// - 타이틀바를 끌어 옮기고, 모서리와 꼭짓점을 끌어 크기를 바꾸고, 가운데 구분선을 끌어 좌우 비율을 바꾼다.
// - 트리: 펼치기와 접기, 액티브(A)와 기즈모(G) 칸, 고르기, 끌어 스크롤. 인스펙터: 컴포넌트 머리를 눌러 속성을 펼치고 접는다. (기본 펼침)
// - 러스트 판은 씬이 DevTools 를 품고 함수마다 부른다:
//     tick 에서 dev_tools.tick(engine, node_tree, unscaled_time_delta), post_draw 에서 dev_tools.draw(engine, node_tree, graphic).
//     is_visible() && is_pointer_inside_panel(engine) 이면 씬은 그 입력을 노드에 넘기지 않는다. (원본 GameScene.isDevToolsCapturingInput)
// - 원본과 다른 점:
//   - 전역 엔진이 없어 원본 setEngine 이 없다. tick, draw, is_pointer_inside_panel 이 엔진을 받는다.
//   - 노드 대신 NodeId 로 가리킨다. 루트 목록은 set_root_nodes(Vec<NodeId>) 이고, 펼친 노드는 NodeId 집합, 접은 컴포넌트는 (NodeId, 컴포넌트 차례) 집합이다.
//     지운 노드는 트리와 인스펙터에서 빠지고, 고른 노드가 지워지면 고르지 않은 것으로 돌아간다.
//   - Graphic 은 엔진이 아니라 EngineRunner 가 들어서 tick 에서는 만질 수 없다. 원본이 tick 과 눌림에서 부르는 setForceGizmosVisible 은
//     바랄 값만 적어 두었다가 다음 draw 첫머리에 Graphic 에 건다. (원본 setEngine 이 거는 false 도 첫 draw 에 건다)
//   - 노드 종류는 WorldNode 하나뿐이라 트리의 Type 칸과 인스펙터의 type 은 늘 "WorldNode" 다.
//   - 리플렉션이 없어 원본 getComponentProperties(get*, is* 게터를 모두 불러 보이기)를 옮길 수 없다.
//     대신 UIDocument 에 등록된 컴포넌트는 UIDocument 가 저장하는 키와 값(serialize_component)을 속성으로 보인다. 이름은 저장 키 그대로다.
//     처음 문서는 엔진 컴포넌트만 등록된 것이고, 게임이 자기 컴포넌트를 등록한 문서를 set_ui_document 로 걸면(또는 get_ui_document_mut 로 등록하면) 그 컴포넌트도 보인다.
//     값은 원본 formatPropertyValue 처럼 적는다. (수는 소수 둘째 자리, 수 둘 배열은 벡터, 이름에 color 가 든 수 넷 배열은 rgba, 그 밖의 수 넷 배열은 사각형,
//     노드 참조는 [WorldNode]) 등록하지 않은 컴포넌트는 종류 이름만 보인다.
//   - Local Storage 탭의 키 입력, 값 고치기, 지울지 묻기: 웹은 원본처럼 window.prompt, window.confirm 을 부른다.
//     데스크탑에는 그런 창이 없어 패널 안에 작은 입력 창을 띄운다. (글은 TextInput 줄에서 받아 IME 도 된다. Enter 나 OK 는 확인, Escape 나 Cancel 은 취소)
//   - Statistics 의 clientNativeSize 는 ViewManager::get_client_native_size(웹은 페이지 전체 크기, 데스크탑은 창 안쪽 논리 크기)다.
//     메모리는 웹에서 원본처럼 Chrome 의 performance.memory 를 읽고, 없는 브라우저와 데스크탑(자바스크립트 힙이 없다)은 "(N/A)" 다.
//     viewScaleMode 는 러스트 열거 이름(StretchHeight 등)으로 적는다.
//   - 화면 비율 가이드는 원본처럼 그때의 좌표계에 기대지 않고 스스로 뷰 좌표계(apply_view_rect)를 걸고 그린다. (원본 GameScene 에서 같은 결과)
//   - 원본은 FPS 표시 설정을 is_frame_per_second_visible 로 알려 주기만 하고 스스로 그리지 않는다. 러스트 판도 같다.
// - 옮기지 않은 것: 없음. (원본에서 아무도 부르지 않는 그리기 오류 무시 try/catch 는 러스트에서 필요 없다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::engine::Engine;
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::inputmanager::KeyCode;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::viewmanager::ViewScaleMode;
use crate::misc::localstorage::LocalStorage;
use crate::ui::uidocument::UIDocument;
use serde_json::Value;
use std::collections::HashSet;

#[cfg(not(target_arch = "wasm32"))]
use crate::core::textinput::{count_characters, TextInputEventKind, TextInputFieldState, TextInputKey};

//==============================================================================
// 고정 레이아웃 상수.
//==============================================================================
const TITLE_HEIGHT: f32 = 28.0;
const TAB_HEIGHT: f32 = 24.0;
const ITEM_HEIGHT: f32 = 20.0;
const INDENT_WIDTH: f32 = 14.0;
const SCROLL_BAR_WIDTH: f32 = 6.0;
const GIZMO_CHECKBOX_SIZE: f32 = 12.0;
const GIZMO_CHECKBOX_MARGIN: f32 = 4.0;
const PADDING: f32 = 7.0;
const LABEL_COLUMN_WIDTH: f32 = 144.0;
const RESIZE_HANDLE_SIZE: f32 = 10.0;
const SPLITTER_HIT_SIZE: f32 = 8.0;
const MIN_PANEL_WIDTH: f32 = 300.0;
const MIN_PANEL_HEIGHT: f32 = 200.0;
const MIN_LEFT_WIDTH: f32 = 80.0;
const TABS: [&str; 4] = ["Statistics", "Node Hierarchy", "Local Storage", "Settings"];
const DEFAULT_PANEL_WIDTH: f32 = 810.0;
const DEFAULT_PANEL_HEIGHT: f32 = 520.0;
const DEFAULT_LEFT_WIDTH: f32 = 405.0;
const SETTINGS_STORAGE_KEY: &str = "hierarchy.settings";

// 글꼴. (원본 FONT_SIZE 12 로 짓는 `${FONT_SIZE}px monospace`, `bold ...`, `${FONT_SIZE - 1}px ...`)
const FONT_STRING: &str = "12px monospace";
const BOLD_FONT_STRING: &str = "bold 12px monospace";
const SMALL_FONT_STRING: &str = "11px monospace";

// 로컬 스토리지 탭의 단추와 칸.
const LOCAL_STORAGE_BUTTON_WIDTH: f32 = 48.0;
const LOCAL_STORAGE_DELETE_COLUMN_WIDTH: f32 = 24.0;
const CONTEXT_MENU_HEIGHT: f32 = ITEM_HEIGHT + 8.0;
const CONTEXT_BUTTON_TEXTS: [&str; 4] = ["Edit Key", "Edit Value", "Duplicate", "Remove"];

// 세팅 탭 줄. (글, 칸의 x 오프셋)
const SETTINGS_ROWS: [(&str, f32); 5] = [
	("Dim Background", 120.0),
	("Show All Gizmos", 120.0),
	("Show Height Aspect Guide", 168.0),
	("Show Width Aspect Guide", 168.0),
	("Show FPS", 120.0),
];

// 노드 종류 이름. (러스트 판의 노드는 WorldNode 하나다)
const NODE_TYPE_NAME: &str = "WorldNode";

// UIDocument 가 노드 참조를 적는 키.
const NODE_REFERENCE_KEY: &str = "$nodeRef";

// 데스크탑 입력 창. (TextInput 줄의 필드 id 와 포커스 차례. 게임 필드보다 늘 나중이라 글을 가져온다)
#[cfg(not(target_arch = "wasm32"))]
const DIALOG_FIELD_ID: u64 = u64::MAX;
#[cfg(not(target_arch = "wasm32"))]
const DIALOG_WIDTH: f32 = 360.0;
#[cfg(not(target_arch = "wasm32"))]
const DIALOG_BUTTON_WIDTH: f32 = 56.0;

//==============================================================================
// 색상 상수. (원본 CSS 색, 0 ~ 255)
//==============================================================================
const COLOR_BACKGROUND: Color = create_color(22.0, 18.0, 12.0, 0.97);
const COLOR_TITLE_BACKGROUND: Color = create_color(40.0, 32.0, 18.0, 1.0);
const COLOR_BORDER_NORMAL: Color = create_color(110.0, 88.0, 50.0, 1.0);
const COLOR_BORDER_ACTIVE: Color = create_color(212.0, 180.0, 106.0, 0.9);
const COLOR_SPLITTER: Color = create_color(75.0, 60.0, 35.0, 1.0);
const COLOR_SPLITTER_ACTIVE: Color = create_color(212.0, 180.0, 106.0, 0.85);
const COLOR_ITEM_SELECTED: Color = create_color(120.0, 90.0, 35.0, 0.85);
const COLOR_SECTION_HEADER_BACKGROUND: Color = create_color(48.0, 38.0, 20.0, 0.9);
const COLOR_COMPONENT_HEADER_BACKGROUND: Color = create_color(36.0, 28.0, 14.0, 0.85);
const COLOR_TEXT: Color = create_color(212.0, 184.0, 150.0, 1.0);
const COLOR_TEXT_DIM: Color = create_color(122.0, 106.0, 80.0, 1.0);
const COLOR_ACCENT: Color = create_color(212.0, 180.0, 106.0, 1.0);
const COLOR_COMPONENT: Color = create_color(168.0, 200.0, 112.0, 1.0);
const COLOR_PROPERTY_VALUE: Color = create_color(232.0, 213.0, 163.0, 1.0);
const COLOR_CLOSE_BUTTON: Color = create_color(140.0, 55.0, 28.0, 0.95);
const COLOR_INACTIVE_TEXT: Color = create_color(140.0, 116.0, 85.0, 1.0);
const COLOR_SCROLLBAR: Color = create_color(110.0, 88.0, 50.0, 0.55);
const COLOR_TRUE: Color = create_color(140.0, 200.0, 120.0, 1.0);
const COLOR_FALSE: Color = create_color(200.0, 120.0, 120.0, 1.0);
// 빈 체크 칸의 속. (원본 "#16120a")
const COLOR_CHECKBOX_EMPTY: Color = create_color(22.0, 18.0, 10.0, 1.0);
// 인스펙터 구분선. (원본 "rgba(75, 60, 35, 0.8)")
const COLOR_INSPECTOR_SEPARATOR: Color = create_color(75.0, 60.0, 35.0, 0.8);
// 바탕 어둡게. (원본 "rgba(0, 0, 0, 0.5)")
const COLOR_DIM: Color = create_color(0.0, 0.0, 0.0, 0.5);
// 로컬 스토리지 단추.
const COLOR_ADD_BUTTON: Color = create_color(60.0, 100.0, 60.0, 0.9);
const COLOR_CLEAR_BUTTON: Color = create_color(100.0, 40.0, 30.0, 0.9);
const COLOR_DELETE_BUTTON: Color = create_color(100.0, 40.0, 30.0, 0.6);
const CONTEXT_BUTTON_COLORS: [Color; 4] = [
	create_color(50.0, 80.0, 50.0, 0.9),
	create_color(40.0, 60.0, 90.0, 0.9),
	create_color(40.0, 70.0, 100.0, 0.9),
	create_color(100.0, 40.0, 30.0, 0.9),
];
const CONTEXT_BUTTON_TEXT_COLORS: [Color; 4] = [
	create_color(140.0, 200.0, 120.0, 1.0),
	create_color(168.0, 200.0, 232.0, 1.0),
	create_color(120.0, 180.0, 200.0, 1.0),
	create_color(200.0, 120.0, 120.0, 1.0),
];
// 화면 비율 가이드. (세로 기준, 가로 기준)
const COLOR_HEIGHT_ASPECT_GUIDE: Color = create_color(80.0, 200.0, 240.0, 1.0);
const COLOR_WIDTH_ASPECT_GUIDE: Color = create_color(240.0, 180.0, 80.0, 1.0);

//==============================================================================
// 0 ~ 255 색 값으로 색 만들기. (알파는 0 ~ 1)
//==============================================================================
const fn create_color(red: f32, green: f32, blue: f32, alpha: f32) -> Color {
	Color::create(red / 255.0, green / 255.0, blue / 255.0, alpha)
}

//==============================================================================
// 리사이즈 모드.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeMode {
	None,
	Top,
	Bottom,
	Left,
	Right,
	TopLeft,
	BottomLeft,
	BottomRight,
	Splitter,
}

//==============================================================================
// 트리 목록 항목.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
struct DevToolsFlatItem {
	node_id: NodeId,
	depth: usize,
	// 자기와 조상이 모두 액티브인지.
	effective_active: bool,
}

//==============================================================================
// 인스펙터 라인.
//==============================================================================
#[derive(Clone, Debug)]
struct InspectorLine {
	text: Option<String>,
	value: String,
	color: Color,
	is_separator: bool,
	is_section_header: bool,
	is_component_header: bool,
	// 컴포넌트 머리가 가리키는 컴포넌트. (노드, 컴포넌트 차례)
	component_reference: Option<(NodeId, usize)>,
	is_component_property: bool,
}

impl InspectorLine {
	//==============================================================================
	// 생성. (원본 생성자와 같은 차례)
	//==============================================================================
	fn new(text: Option<&str>, value: &str, color: Color, is_separator: bool, is_section_header: bool, is_component_header: bool, component_reference: Option<(NodeId, usize)>, is_component_property: bool) -> InspectorLine {
		InspectorLine {
			text: text.map(String::from),
			value: String::from(value),
			color,
			is_separator,
			is_section_header,
			is_component_header,
			component_reference,
			is_component_property,
		}
	}
}

//==============================================================================
// 통계 항목.
//==============================================================================
#[derive(Clone, Debug)]
struct StatisticsItem {
	key: String,
	value: String,
	is_separator: bool,
	is_section_header: bool,
}

impl StatisticsItem {
	//==============================================================================
	// 키와 값 항목.
	//==============================================================================
	fn create(key: &str, value: String) -> StatisticsItem {
		StatisticsItem {
			key: String::from(key),
			value,
			is_separator: false,
			is_section_header: false,
		}
	}

	//==============================================================================
	// 하위 섹션 머리 항목.
	//==============================================================================
	fn create_section_header(key: &str) -> StatisticsItem {
		StatisticsItem {
			key: String::from(key),
			value: String::new(),
			is_separator: false,
			is_section_header: true,
		}
	}
}

//==============================================================================
// 로컬 스토리지 입력 창이 끝나면 할 일. (원본 prompt, confirm 뒤의 코드)
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
enum LocalStorageDialogAction {
	// "키 입력:" 뒤 값을 묻는다.
	AddKey,
	// "값 입력:" 뒤 키에 값을 쓴다.
	AddValue(String),
	// 키 이름 바꾸기.
	EditKey(String),
	// 값 바꾸기.
	EditValue(String),
	// 모두 지우기.
	Clear,
	// 하나 지우기.
	Remove(String),
}

//==============================================================================
// 데스크탑 입력 창. (원본 window.prompt, window.confirm 자리)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
struct LocalStorageDialog {
	message: String,
	value: String,
	// IME 조합 중인 글.
	preedit_text: String,
	// 글을 받는 창인지. (아니면 확인만 묻는다)
	is_prompt: bool,
	action: LocalStorageDialogAction,
	value_revision: u64,
	// 마지막으로 그린 커서의 x. (창 안쪽 논리 좌표, IME 후보 창 자리)
	caret_x: f32,
}

//==============================================================================
// 개발자 도구 패널.
//==============================================================================
pub struct DevTools {
	root_ids: Vec<NodeId>,
	is_visible: bool,
	panel_x: f32,
	panel_y: f32,
	panel_width: f32,
	panel_height: f32,
	left_width: f32,
	is_dragging_panel: bool,
	drag_start_mouse_x: f32,
	drag_start_mouse_y: f32,
	drag_start_panel_x: f32,
	drag_start_panel_y: f32,
	resize_mode: ResizeMode,
	resize_drag_start_mouse_x: f32,
	resize_drag_start_mouse_y: f32,
	resize_drag_start_panel_x: f32,
	resize_drag_start_panel_y: f32,
	resize_drag_start_panel_width: f32,
	resize_drag_start_panel_height: f32,
	resize_drag_start_left_width: f32,
	selected_node_id: Option<NodeId>,
	tree_scroll_y: f32,
	inspector_scroll_y: f32,
	previous_is_key_f2: bool,
	flat_list: Vec<DevToolsFlatItem>,
	expanded_node_ids: HashSet<NodeId>,
	// 접은 컴포넌트. (노드, 컴포넌트 차례. 없으면 펼친 것)
	collapsed_components: HashSet<(NodeId, usize)>,
	cached_inspector_lines: Vec<InspectorLine>,
	is_tree_scroll_dragging: bool,
	tree_scroll_drag_start_mouse_y: f32,
	tree_scroll_drag_start_scroll_y: f32,
	is_inspector_scroll_dragging: bool,
	inspector_scroll_drag_start_mouse_y: f32,
	inspector_scroll_drag_start_scroll_y: f32,
	active_tab: usize,
	local_storage_scroll_y: f32,
	is_local_storage_scroll_dragging: bool,
	local_storage_scroll_drag_start_mouse_y: f32,
	local_storage_scroll_drag_start_scroll_y: f32,
	selected_local_storage_key: Option<String>,
	is_dim_enabled: bool,
	is_all_gizmos_visible: bool,
	is_height_aspect_guide_visible: bool,
	is_width_aspect_guide_visible: bool,
	is_frame_per_second_visible: bool,
	// 로컬 스토리지 선택 메뉴 단추 자리. (원본 ctxButtonRects, 그릴 때 정한다)
	context_button_rects: Vec<Rect>,
	selected_statistics_key: Option<String>,
	statistics_scroll_y: f32,
	is_statistics_scroll_dragging: bool,
	statistics_scroll_drag_start_mouse_y: f32,
	statistics_scroll_drag_start_scroll_y: f32,
	// 인스펙터의 컴포넌트 속성을 읽는 문서. (처음은 엔진 컴포넌트만 등록된 것, set_ui_document 로 게임 문서를 건다)
	ui_document: UIDocument,
	// 다음 draw 에 Graphic 에 걸 모든 기즈모 강제 표시 값.
	pending_force_gizmos_visible: Option<bool>,
	#[cfg(not(target_arch = "wasm32"))]
	local_storage_dialog: Option<LocalStorageDialog>,
}

impl DevTools {
	//==============================================================================
	// 생성. (숨긴 채, Node Hierarchy 탭, 저장한 설정을 읽는다)
	//==============================================================================
	pub fn new() -> DevTools {
		let mut dev_tools = DevTools {
			root_ids: Vec::new(),
			is_visible: false,
			panel_x: 10.0,
			panel_y: 50.0,
			panel_width: DEFAULT_PANEL_WIDTH,
			panel_height: DEFAULT_PANEL_HEIGHT,
			left_width: DEFAULT_LEFT_WIDTH,
			is_dragging_panel: false,
			drag_start_mouse_x: 0.0,
			drag_start_mouse_y: 0.0,
			drag_start_panel_x: 0.0,
			drag_start_panel_y: 0.0,
			resize_mode: ResizeMode::None,
			resize_drag_start_mouse_x: 0.0,
			resize_drag_start_mouse_y: 0.0,
			resize_drag_start_panel_x: 0.0,
			resize_drag_start_panel_y: 0.0,
			resize_drag_start_panel_width: 0.0,
			resize_drag_start_panel_height: 0.0,
			resize_drag_start_left_width: 0.0,
			selected_node_id: None,
			tree_scroll_y: 0.0,
			inspector_scroll_y: 0.0,
			previous_is_key_f2: false,
			flat_list: Vec::new(),
			expanded_node_ids: HashSet::new(),
			collapsed_components: HashSet::new(),
			cached_inspector_lines: Vec::new(),
			is_tree_scroll_dragging: false,
			tree_scroll_drag_start_mouse_y: 0.0,
			tree_scroll_drag_start_scroll_y: 0.0,
			is_inspector_scroll_dragging: false,
			inspector_scroll_drag_start_mouse_y: 0.0,
			inspector_scroll_drag_start_scroll_y: 0.0,
			active_tab: 1,
			local_storage_scroll_y: 0.0,
			is_local_storage_scroll_dragging: false,
			local_storage_scroll_drag_start_mouse_y: 0.0,
			local_storage_scroll_drag_start_scroll_y: 0.0,
			selected_local_storage_key: None,
			is_dim_enabled: false,
			is_all_gizmos_visible: true,
			is_height_aspect_guide_visible: false,
			is_width_aspect_guide_visible: false,
			is_frame_per_second_visible: false,
			context_button_rects: Vec::new(),
			selected_statistics_key: None,
			statistics_scroll_y: 0.0,
			is_statistics_scroll_dragging: false,
			statistics_scroll_drag_start_mouse_y: 0.0,
			statistics_scroll_drag_start_scroll_y: 0.0,
			ui_document: UIDocument::new(),
			pending_force_gizmos_visible: Some(false),
			#[cfg(not(target_arch = "wasm32"))]
			local_storage_dialog: None,
		};
		dev_tools.load_settings();
		dev_tools
	}

	//==============================================================================
	// 루트 노드 목록 설정. (루트는 펼친 채로 둔다)
	//==============================================================================
	pub fn set_root_nodes(&mut self, root_ids: Vec<NodeId>) {
		for root_id in root_ids.iter() {
			self.expanded_node_ids.insert(*root_id);
		}
		self.root_ids = root_ids;
	}

	//==============================================================================
	// 표시 여부 반환.
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.is_visible
	}

	//==============================================================================
	// 보이기와 숨기기. (F2 와 같다. 보일 때 패널을 창 오른쪽 아래에 붙이고, 모든 기즈모 설정을 건다)
	//==============================================================================
	pub fn set_visible(&mut self, engine: &Engine, is_visible: bool) {
		self.is_visible = is_visible;
		self.pending_force_gizmos_visible = Some(is_visible && self.is_all_gizmos_visible);
		if is_visible {
			// 패널 좌표는 canvas-native 픽셀 기준. 뷰 스케일 모드와 상관없이 늘 같은 크기로 오른쪽 아래에 붙는다.
			let canvas_native_size = engine.get_view_manager().get_canvas_native_size();
			self.panel_x = canvas_native_size.x - self.panel_width;
			self.panel_y = canvas_native_size.y - self.panel_height;
		}
		else {
			self.close_local_storage_dialog();
		}
	}

	//==============================================================================
	// 지금 탭 반환. (0 Statistics, 1 Node Hierarchy, 2 Local Storage, 3 Settings)
	//==============================================================================
	pub fn get_active_tab(&self) -> usize {
		self.active_tab
	}

	//==============================================================================
	// 탭 설정. (없는 탭이면 무시)
	//==============================================================================
	pub fn set_active_tab(&mut self, tab_index: usize) {
		if tab_index < TABS.len() {
			self.active_tab = tab_index;
		}
	}

	//==============================================================================
	// 고른 노드 반환.
	//==============================================================================
	pub fn get_selected_node(&self) -> Option<NodeId> {
		self.selected_node_id
	}

	//==============================================================================
	// 노드 고르기. (인스펙터 스크롤은 맨 위로)
	//==============================================================================
	pub fn set_selected_node(&mut self, node_id: Option<NodeId>) {
		self.selected_node_id = node_id;
		self.inspector_scroll_y = 0.0;
	}

	//==============================================================================
	// 트리에서 노드를 펼쳤는지.
	//==============================================================================
	pub fn is_node_expanded(&self, node_id: NodeId) -> bool {
		self.expanded_node_ids.contains(&node_id)
	}

	//==============================================================================
	// 트리에서 노드 펼치기와 접기.
	//==============================================================================
	pub fn set_node_expanded(&mut self, node_id: NodeId, is_expanded: bool) {
		if is_expanded {
			self.expanded_node_ids.insert(node_id);
		}
		else {
			self.expanded_node_ids.remove(&node_id);
		}
	}

	//==============================================================================
	// 인스펙터가 컴포넌트 속성을 읽을 문서 설정. (게임이 자기 컴포넌트를 등록한 UIDocument 를 걸면 그 컴포넌트의 속성도 보인다)
	//==============================================================================
	pub fn set_ui_document(&mut self, ui_document: UIDocument) {
		self.ui_document = ui_document;
	}

	//==============================================================================
	// 인스펙터가 컴포넌트 속성을 읽는 문서. (변경용, 컴포넌트 종류를 더 등록할 때)
	//==============================================================================
	pub fn get_ui_document_mut(&mut self) -> &mut UIDocument {
		&mut self.ui_document
	}

	//==============================================================================
	// 패널 영역 반환. (창 안쪽 논리 좌표)
	//==============================================================================
	pub fn get_panel_rect(&self) -> Rect {
		Rect::create(self.panel_x, self.panel_y, self.panel_width, self.panel_height)
	}

	//==============================================================================
	// 현재 터치 위치가 패널 내부에 있는지 반환.
	//==============================================================================
	pub fn is_pointer_inside_panel(&self, engine: &Engine) -> bool {
		if !self.is_visible {
			return false;
		}
		let touch_position = engine.get_input_manager().get_canvas_native_input_position();
		let is_inside_x = touch_position.x >= self.panel_x && touch_position.x <= self.panel_x + self.panel_width;
		let is_inside_y = touch_position.y >= self.panel_y && touch_position.y <= self.panel_y + self.panel_height;
		is_inside_x && is_inside_y
	}

	//==============================================================================
	// 갱신. (F2 토글, 트리 목록, 터치)
	//==============================================================================
	pub fn tick(&mut self, engine: &mut Engine, node_tree: &mut NodeTree, _time_delta: f32) {
		let is_key_f2 = engine.get_input_manager().is_key_pressed(KeyCode::F2);

		// F2 를 막 누름.
		if is_key_f2 && !self.previous_is_key_f2 {
			let is_next_visible = !self.is_visible;
			self.set_visible(engine, is_next_visible);
		}
		self.previous_is_key_f2 = is_key_f2;

		if !self.is_visible {
			return;
		}

		// 고른 노드가 지워졌으면 고르지 않은 것으로 돌린다.
		if let Some(selected_node_id) = self.selected_node_id {
			if !node_tree.contains(selected_node_id) {
				self.selected_node_id = None;
			}
		}

		// 트리 목록 갱신.
		self.rebuild_flat_list(node_tree);

		// 데스크탑 입력 창의 글.
		#[cfg(not(target_arch = "wasm32"))]
		{
			if self.local_storage_dialog.is_some() {
				self.tick_local_storage_dialog(engine);
			}
		}

		let input_manager = engine.get_input_manager();
		let is_touch_pressed = input_manager.is_touch_pressed();
		let is_touch_released = input_manager.is_touch_released();
		let is_touch_moved = input_manager.is_touch_moved();
		// canvas-native 좌표로 입력 처리. (패널 좌표계와 같다)
		let touch_position = input_manager.get_canvas_native_input_position();
		let touch_x = touch_position.x;
		let touch_y = touch_position.y;

		if is_touch_pressed {
			self.handle_press(engine, node_tree, touch_x, touch_y);
		}
		else if is_touch_moved {
			self.handle_move(touch_x, touch_y);
		}
		else if is_touch_released {
			self.handle_release(touch_x, touch_y);
		}
	}

	//==============================================================================
	// 리사이즈 드래그 시작.
	//==============================================================================
	fn start_resize(&mut self, resize_mode: ResizeMode, touch_x: f32, touch_y: f32) {
		self.resize_mode = resize_mode;
		self.resize_drag_start_mouse_x = touch_x;
		self.resize_drag_start_mouse_y = touch_y;
		self.resize_drag_start_panel_x = self.panel_x;
		self.resize_drag_start_panel_y = self.panel_y;
		self.resize_drag_start_panel_width = self.panel_width;
		self.resize_drag_start_panel_height = self.panel_height;
		self.resize_drag_start_left_width = self.left_width;
	}

	//==============================================================================
	// 눌림 처리.
	//==============================================================================
	fn handle_press(&mut self, engine: &Engine, node_tree: &mut NodeTree, touch_x: f32, touch_y: f32) {
		let panel_x = self.panel_x;
		let panel_y = self.panel_y;
		let panel_width = self.panel_width;
		let panel_height = self.panel_height;
		let left_width = self.left_width;

		// 패널 범위 밖이면 무시.
		let is_inside_x = touch_x >= panel_x && touch_x <= panel_x + panel_width;
		let is_inside_y = touch_y >= panel_y && touch_y <= panel_y + panel_height;
		if !is_inside_x || !is_inside_y {
			return;
		}

		// 데스크탑 입력 창이 떠 있으면 창의 단추만 받는다. (원본 prompt 처럼 다른 것을 막는다)
		#[cfg(not(target_arch = "wasm32"))]
		{
			if self.local_storage_dialog.is_some() {
				self.handle_local_storage_dialog_press(touch_x, touch_y);
				return;
			}
		}

		// 엣지/코너 근접 여부.
		let is_near_left = touch_x <= panel_x + RESIZE_HANDLE_SIZE;
		let is_near_right = touch_x >= panel_x + panel_width - RESIZE_HANDLE_SIZE;
		let is_near_top = touch_y <= panel_y + RESIZE_HANDLE_SIZE;
		let is_near_bottom = touch_y >= panel_y + panel_height - RESIZE_HANDLE_SIZE;

		// 닫기 버튼 영역. (오른쪽 위 전용)
		let is_in_close_button = touch_y <= panel_y + TITLE_HEIGHT && touch_x >= panel_x + panel_width - TITLE_HEIGHT;

		// 코너 리사이즈. (최우선, 닫기 버튼 제외)
		if is_near_left && is_near_top && !is_in_close_button {
			self.start_resize(ResizeMode::TopLeft, touch_x, touch_y);
			return;
		}
		if is_near_left && is_near_bottom {
			self.start_resize(ResizeMode::BottomLeft, touch_x, touch_y);
			return;
		}
		if is_near_right && is_near_bottom {
			self.start_resize(ResizeMode::BottomRight, touch_x, touch_y);
			return;
		}

		// 엣지 리사이즈.
		if is_near_left {
			self.start_resize(ResizeMode::Left, touch_x, touch_y);
			return;
		}
		if is_near_right && !is_in_close_button {
			self.start_resize(ResizeMode::Right, touch_x, touch_y);
			return;
		}
		if is_near_top && !is_in_close_button {
			self.start_resize(ResizeMode::Top, touch_x, touch_y);
			return;
		}
		if is_near_bottom {
			self.start_resize(ResizeMode::Bottom, touch_x, touch_y);
			return;
		}

		// 타이틀바 영역.
		if touch_y <= panel_y + TITLE_HEIGHT {
			// 닫기 버튼.
			if is_in_close_button {
				self.is_visible = false;
				self.pending_force_gizmos_visible = Some(false);
				return;
			}
			// 패널 드래그 시작.
			self.is_dragging_panel = true;
			self.drag_start_mouse_x = touch_x;
			self.drag_start_mouse_y = touch_y;
			self.drag_start_panel_x = panel_x;
			self.drag_start_panel_y = panel_y;
			return;
		}

		// 탭바 영역.
		if touch_y <= panel_y + TITLE_HEIGHT + TAB_HEIGHT {
			let tab_width = panel_width / TABS.len() as f32;
			let tab_index = ((touch_x - panel_x) / tab_width).floor();
			if tab_index >= 0.0 && (tab_index as usize) < TABS.len() {
				self.active_tab = tab_index as usize;
			}
			return;
		}

		let content_y = panel_y + TITLE_HEIGHT + TAB_HEIGHT + 1.0;
		let content_height = panel_height - TITLE_HEIGHT - TAB_HEIGHT - 1.0;

		// Statistics 탭.
		if self.active_tab == 0 {
			self.handle_statistics_press(engine, touch_y, content_y);
			return;
		}

		// Local Storage 탭.
		if self.active_tab == 2 {
			self.handle_local_storage_press(touch_x, touch_y, panel_x, content_y, panel_width, content_height);
			return;
		}

		// Settings 탭.
		if self.active_tab == 3 {
			self.handle_settings_press(touch_x, touch_y, panel_x, content_y);
			return;
		}

		// Node Hierarchy 탭에서만 트리/인스펙터 상호작용 처리.
		if self.active_tab != 1 {
			return;
		}

		// 스플리터 드래그.
		let splitter_x = panel_x + left_width;
		let is_near_splitter = touch_x >= splitter_x - SPLITTER_HIT_SIZE / 2.0 && touch_x <= splitter_x + SPLITTER_HIT_SIZE / 2.0;
		if is_near_splitter {
			self.start_resize(ResizeMode::Splitter, touch_x, touch_y);
			return;
		}

		// 트리 영역.
		if touch_x < panel_x + left_width {
			let tree_items_y = content_y + ITEM_HEIGHT * 2.0;
			let local_y = touch_y - tree_items_y + self.tree_scroll_y;
			let clicked_index = (local_y / ITEM_HEIGHT).floor();

			if clicked_index >= 0.0 && (clicked_index as usize) < self.flat_list.len() {
				let clicked_index = clicked_index as usize;
				let clicked_node_id = self.flat_list[clicked_index].node_id;
				let child_count = node_tree.get_child_count(clicked_node_id);

				// 기즈모 체크박스 클릭 감지.
				let checkbox_right_x = panel_x + left_width - SCROLL_BAR_WIDTH - GIZMO_CHECKBOX_MARGIN;
				let checkbox_x = checkbox_right_x - GIZMO_CHECKBOX_SIZE;
				let item_top_y = tree_items_y + clicked_index as f32 * ITEM_HEIGHT - self.tree_scroll_y;
				let item_middle_y = item_top_y + ITEM_HEIGHT * 0.5;
				let checkbox_y = item_middle_y - GIZMO_CHECKBOX_SIZE * 0.5;
				let checkbox_rect = Rect::create(checkbox_x, checkbox_y, GIZMO_CHECKBOX_SIZE, GIZMO_CHECKBOX_SIZE);
				if is_point_in_rect(touch_x, touch_y, checkbox_rect) {
					if let Some(clicked_node) = node_tree.get_node_mut(clicked_node_id) {
						let is_gizmo_visible = clicked_node.is_gizmo_visible();
						clicked_node.set_gizmo_visible(!is_gizmo_visible);
					}
					return;
				}

				// 액티브 체크박스 클릭 감지.
				let active_checkbox_right_x = checkbox_x - GIZMO_CHECKBOX_MARGIN;
				let active_checkbox_x = active_checkbox_right_x - GIZMO_CHECKBOX_SIZE;
				let active_checkbox_rect = Rect::create(active_checkbox_x, checkbox_y, GIZMO_CHECKBOX_SIZE, GIZMO_CHECKBOX_SIZE);
				if is_point_in_rect(touch_x, touch_y, active_checkbox_rect) {
					if let Some(clicked_node) = node_tree.get_node_mut(clicked_node_id) {
						let is_active = clicked_node.is_active();
						clicked_node.set_active(!is_active);
					}
					return;
				}

				if child_count > 0 {
					if self.expanded_node_ids.contains(&clicked_node_id) {
						self.expanded_node_ids.remove(&clicked_node_id);
					}
					else {
						self.expanded_node_ids.insert(clicked_node_id);
					}
				}

				self.selected_node_id = Some(clicked_node_id);
				self.inspector_scroll_y = 0.0;
			}

			self.is_tree_scroll_dragging = true;
			self.tree_scroll_drag_start_mouse_y = touch_y;
			self.tree_scroll_drag_start_scroll_y = self.tree_scroll_y;
			return;
		}

		// 인스펙터 영역. (컴포넌트 머리 클릭 감지)
		let local_y = touch_y - content_y + self.inspector_scroll_y;
		let clicked_line_index = (local_y / ITEM_HEIGHT).floor();

		if clicked_line_index >= 0.0 && (clicked_line_index as usize) < self.cached_inspector_lines.len() {
			let clicked_line = &self.cached_inspector_lines[clicked_line_index as usize];
			if clicked_line.is_component_header {
				if let Some(component_reference) = clicked_line.component_reference {
					if self.collapsed_components.contains(&component_reference) {
						self.collapsed_components.remove(&component_reference);
					}
					else {
						self.collapsed_components.insert(component_reference);
					}
					return;
				}
			}
		}

		self.is_inspector_scroll_dragging = true;
		self.inspector_scroll_drag_start_mouse_y = touch_y;
		self.inspector_scroll_drag_start_scroll_y = self.inspector_scroll_y;
	}

	//==============================================================================
	// 이동 처리.
	//==============================================================================
	fn handle_move(&mut self, touch_x: f32, touch_y: f32) {
		// 패널 드래그.
		if self.is_dragging_panel {
			let delta_x = touch_x - self.drag_start_mouse_x;
			let delta_y = touch_y - self.drag_start_mouse_y;
			self.panel_x = self.drag_start_panel_x + delta_x;
			self.panel_y = self.drag_start_panel_y + delta_y;
		}

		// 리사이즈.
		if self.resize_mode != ResizeMode::None {
			let delta_x = touch_x - self.resize_drag_start_mouse_x;
			let delta_y = touch_y - self.resize_drag_start_mouse_y;

			// 오른쪽 엣지 관련. (오른쪽으로 넓혀짐)
			let is_right_mode = self.resize_mode == ResizeMode::Right || self.resize_mode == ResizeMode::BottomRight;
			if is_right_mode {
				let new_width = self.resize_drag_start_panel_width + delta_x;
				self.panel_width = MIN_PANEL_WIDTH.max(new_width);
			}

			// 왼쪽 엣지 관련. (왼쪽으로 넓혀짐 = panel_x 이동 + 너비 변화)
			let is_left_mode = self.resize_mode == ResizeMode::Left || self.resize_mode == ResizeMode::TopLeft || self.resize_mode == ResizeMode::BottomLeft;
			if is_left_mode {
				let new_width = self.resize_drag_start_panel_width - delta_x;
				if new_width >= MIN_PANEL_WIDTH {
					self.panel_width = new_width;
					self.panel_x = self.resize_drag_start_panel_x + delta_x;
				}
			}

			// 아래쪽 엣지 관련. (아래로 높여짐)
			let is_bottom_mode = self.resize_mode == ResizeMode::Bottom || self.resize_mode == ResizeMode::BottomRight || self.resize_mode == ResizeMode::BottomLeft;
			if is_bottom_mode {
				let new_height = self.resize_drag_start_panel_height + delta_y;
				self.panel_height = MIN_PANEL_HEIGHT.max(new_height);
			}

			// 위쪽 엣지 관련. (위로 높여짐 = panel_y 이동 + 높이 변화)
			let is_top_mode = self.resize_mode == ResizeMode::Top || self.resize_mode == ResizeMode::TopLeft;
			if is_top_mode {
				let new_height = self.resize_drag_start_panel_height - delta_y;
				if new_height >= MIN_PANEL_HEIGHT {
					self.panel_height = new_height;
					self.panel_y = self.resize_drag_start_panel_y + delta_y;
				}
			}

			// 스플리터 드래그.
			if self.resize_mode == ResizeMode::Splitter {
				let new_left_width = self.resize_drag_start_left_width + delta_x;
				let maximum_left_width = self.panel_width - MIN_LEFT_WIDTH;
				self.left_width = MIN_LEFT_WIDTH.max(maximum_left_width.min(new_left_width));
			}
		}

		// 트리 스크롤.
		if self.is_tree_scroll_dragging {
			let delta_y = touch_y - self.tree_scroll_drag_start_mouse_y;
			let new_scroll_y = self.tree_scroll_drag_start_scroll_y - delta_y;
			self.tree_scroll_y = new_scroll_y.max(0.0);
		}

		// 인스펙터 스크롤.
		if self.is_inspector_scroll_dragging {
			let delta_y = touch_y - self.inspector_scroll_drag_start_mouse_y;
			let new_scroll_y = self.inspector_scroll_drag_start_scroll_y - delta_y;
			self.inspector_scroll_y = new_scroll_y.max(0.0);
		}

		// 로컬 스토리지 스크롤.
		if self.is_local_storage_scroll_dragging {
			let delta_y = touch_y - self.local_storage_scroll_drag_start_mouse_y;
			let new_scroll_y = self.local_storage_scroll_drag_start_scroll_y - delta_y;
			self.local_storage_scroll_y = new_scroll_y.max(0.0);
		}

		// 통계 스크롤.
		if self.is_statistics_scroll_dragging {
			let delta_y = touch_y - self.statistics_scroll_drag_start_mouse_y;
			let new_scroll_y = self.statistics_scroll_drag_start_scroll_y - delta_y;
			self.statistics_scroll_y = new_scroll_y.max(0.0);
		}
	}

	//==============================================================================
	// 뗌 처리.
	//==============================================================================
	fn handle_release(&mut self, _touch_x: f32, _touch_y: f32) {
		self.is_dragging_panel = false;
		self.resize_mode = ResizeMode::None;
		self.is_tree_scroll_dragging = false;
		self.is_inspector_scroll_dragging = false;
		self.is_local_storage_scroll_dragging = false;
		self.is_statistics_scroll_dragging = false;
	}

	//==============================================================================
	// 로컬 스토리지 탭 눌림 처리.
	//==============================================================================
	fn handle_local_storage_press(&mut self, touch_x: f32, touch_y: f32, panel_x: f32, panel_y: f32, panel_width: f32, panel_height: f32) {
		let button_height = ITEM_HEIGHT - 4.0;
		let button_y = panel_y + 2.0;
		let clear_button_x = panel_x + panel_width - PADDING - LOCAL_STORAGE_BUTTON_WIDTH;
		let add_button_x = clear_button_x - PADDING - LOCAL_STORAGE_BUTTON_WIDTH;

		// + Add 버튼 클릭.
		if is_point_in_rect(touch_x, touch_y, Rect::create(add_button_x, button_y, LOCAL_STORAGE_BUTTON_WIDTH, button_height)) {
			self.open_local_storage_prompt("키 입력:", "", LocalStorageDialogAction::AddKey);
			return;
		}

		// Clear 버튼 클릭.
		if is_point_in_rect(touch_x, touch_y, Rect::create(clear_button_x, button_y, LOCAL_STORAGE_BUTTON_WIDTH, button_height)) {
			self.open_local_storage_confirm("모든 로컬 스토리지 항목을 삭제하시겠습니까?", LocalStorageDialogAction::Clear);
			return;
		}

		// 선택 컨텍스트 메뉴 클릭.
		if let Some(selected_key) = self.selected_local_storage_key.clone() {
			let context_menu_y = panel_y + panel_height - CONTEXT_MENU_HEIGHT;
			if touch_y >= context_menu_y {
				let button_index = self.context_button_rects.iter().position(|button_rect| is_point_in_rect(touch_x, touch_y, *button_rect));
				match button_index {
					Some(0) => {
						// Edit Key.
						let message = "키 이름 수정:";
						self.open_local_storage_prompt(message, &selected_key, LocalStorageDialogAction::EditKey(selected_key.clone()));
					}
					Some(1) => {
						// Edit Value.
						let current_value = LocalStorage::get_string(&selected_key, "");
						let message = format!("\"{}\" 값 수정:", selected_key);
						self.open_local_storage_prompt(&message, &current_value, LocalStorageDialogAction::EditValue(selected_key.clone()));
					}
					Some(2) => {
						// Duplicate.
						let clone_key = format!("{} (Clone)", selected_key);
						let clone_value = LocalStorage::get_string(&selected_key, "");
						LocalStorage::set_string(&clone_key, &clone_value);
						self.selected_local_storage_key = Some(clone_key);
					}
					Some(3) => {
						// Remove.
						let message = format!("\"{}\" 항목을 삭제하시겠습니까?", selected_key);
						self.open_local_storage_confirm(&message, LocalStorageDialogAction::Remove(selected_key.clone()));
					}
					_ => {
					}
				}
				return;
			}
		}

		// 아이템 목록 영역 클릭.
		let items_y = panel_y + ITEM_HEIGHT * 2.0;
		let keys = LocalStorage::get_keys();
		let local_y = touch_y - items_y + self.local_storage_scroll_y;
		let clicked_index = (local_y / ITEM_HEIGHT).floor();

		if touch_y >= items_y && clicked_index >= 0.0 && (clicked_index as usize) < keys.len() {
			let clicked_key = keys[clicked_index as usize].clone();

			// 삭제(×) 버튼 클릭.
			let delete_button_x = panel_x + panel_width - SCROLL_BAR_WIDTH - LOCAL_STORAGE_DELETE_COLUMN_WIDTH;
			if touch_x >= delete_button_x {
				let message = format!("\"{}\" 항목을 삭제하시겠습니까?", clicked_key);
				self.open_local_storage_confirm(&message, LocalStorageDialogAction::Remove(clicked_key));
				return;
			}

			// 행 클릭 → 선택 / 선택해제 토글.
			if self.selected_local_storage_key.as_deref() == Some(clicked_key.as_str()) {
				self.selected_local_storage_key = None;
			}
			else {
				self.selected_local_storage_key = Some(clicked_key);
			}
		}
		else if touch_y >= items_y {
			// 아이템 아래 빈 영역 클릭 → 선택 해제.
			self.selected_local_storage_key = None;
		}

		// 아이템 영역 전체에서 스크롤 드래그 시작.
		if touch_y >= items_y {
			self.is_local_storage_scroll_dragging = true;
			self.local_storage_scroll_drag_start_mouse_y = touch_y;
			self.local_storage_scroll_drag_start_scroll_y = self.local_storage_scroll_y;
		}
	}

	//==============================================================================
	// 글을 묻는다. (웹은 원본처럼 window.prompt, 데스크탑은 패널 안 입력 창)
	//==============================================================================
	fn open_local_storage_prompt(&mut self, message: &str, default_value: &str, action: LocalStorageDialogAction) {
		#[cfg(target_arch = "wasm32")]
		{
			let result = web_sys::window().and_then(|window| window.prompt_with_message_and_default(message, default_value).ok().flatten());
			self.complete_local_storage_dialog(action, result);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.local_storage_dialog = Some(LocalStorageDialog {
				message: String::from(message),
				value: String::from(default_value),
				preedit_text: String::new(),
				is_prompt: true,
				action,
				value_revision: 0,
				caret_x: 0.0,
			});
		}
	}

	//==============================================================================
	// 확인을 묻는다. (웹은 원본처럼 window.confirm, 데스크탑은 패널 안 입력 창)
	//==============================================================================
	fn open_local_storage_confirm(&mut self, message: &str, action: LocalStorageDialogAction) {
		#[cfg(target_arch = "wasm32")]
		{
			let is_confirmed = web_sys::window().map(|window| window.confirm_with_message(message).unwrap_or(false)).unwrap_or(false);
			let result = if is_confirmed {
				Some(String::new())
			}
			else {
				None
			};
			self.complete_local_storage_dialog(action, result);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.local_storage_dialog = Some(LocalStorageDialog {
				message: String::from(message),
				value: String::new(),
				preedit_text: String::new(),
				is_prompt: false,
				action,
				value_revision: 0,
				caret_x: 0.0,
			});
		}
	}

	//==============================================================================
	// 입력 창이 끝난 뒤 할 일. (result 는 확인이면 Some(입력한 글), 취소면 None. 원본 prompt 의 null 과 confirm 의 false 가 None)
	//==============================================================================
	fn complete_local_storage_dialog(&mut self, action: LocalStorageDialogAction, result: Option<String>) {
		match action {
			LocalStorageDialogAction::AddKey => {
				if let Some(new_key) = result {
					let trimmed_key = new_key.trim();
					if !trimmed_key.is_empty() {
						self.open_local_storage_prompt("값 입력:", "", LocalStorageDialogAction::AddValue(String::from(trimmed_key)));
					}
				}
			}
			LocalStorageDialogAction::AddValue(key) => {
				// 원본처럼 취소해도 빈 값으로 쓴다. (prompt(...) || "")
				let new_value = result.unwrap_or_default();
				LocalStorage::set_string(&key, &new_value);
			}
			LocalStorageDialogAction::EditKey(selected_key) => {
				if let Some(new_key) = result {
					let trimmed_key = new_key.trim();
					if !trimmed_key.is_empty() && trimmed_key != selected_key {
						let existing_value = LocalStorage::get_string(&selected_key, "");
						LocalStorage::remove(&selected_key);
						LocalStorage::set_string(trimmed_key, &existing_value);
						self.selected_local_storage_key = Some(String::from(trimmed_key));
					}
				}
			}
			LocalStorageDialogAction::EditValue(selected_key) => {
				if let Some(new_value) = result {
					LocalStorage::set_string(&selected_key, &new_value);
				}
			}
			LocalStorageDialogAction::Clear => {
				if result.is_some() {
					LocalStorage::clear();
					self.selected_local_storage_key = None;
				}
			}
			LocalStorageDialogAction::Remove(key) => {
				if result.is_some() {
					LocalStorage::remove(&key);
					if self.selected_local_storage_key.as_deref() == Some(key.as_str()) {
						self.selected_local_storage_key = None;
					}
				}
			}
		}
	}

	//==============================================================================
	// 떠 있는 입력 창을 할 일 없이 닫는다. (패널을 숨길 때. 웹은 창이 막고 있어 남는 일이 없다)
	//==============================================================================
	fn close_local_storage_dialog(&mut self) {
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.local_storage_dialog = None;
		}
	}

	//==============================================================================
	// 트리 목록 재구성.
	//==============================================================================
	fn rebuild_flat_list(&mut self, node_tree: &NodeTree) {
		self.flat_list.clear();
		let root_ids = self.root_ids.clone();
		for root_id in root_ids.iter() {
			self.collect_node(node_tree, *root_id, 0, true);
		}
	}

	//==============================================================================
	// 노드를 플랫 목록에 재귀 추가. (지운 노드는 건너뛴다)
	//==============================================================================
	fn collect_node(&mut self, node_tree: &NodeTree, node_id: NodeId, depth: usize, parent_active: bool) {
		let node = match node_tree.get_node(node_id) {
			Some(node) => {
				node
			}
			None => {
				return;
			}
		};
		let effective_active = parent_active && node.is_active();
		self.flat_list.push(DevToolsFlatItem {
			node_id,
			depth,
			effective_active,
		});

		if !self.expanded_node_ids.contains(&node_id) {
			return;
		}

		for child_id in node.get_child_ids().iter() {
			self.collect_node(node_tree, *child_id, depth + 1, effective_active);
		}
	}

	//==============================================================================
	// 인스펙터 라인 목록 구성. (cached_inspector_lines 에 담는다)
	//==============================================================================
	fn build_inspector_lines(&mut self, node_tree: &NodeTree, node_id: NodeId) {
		let mut lines: Vec<InspectorLine> = Vec::new();
		let node = match node_tree.get_node(node_id) {
			Some(node) => {
				node
			}
			None => {
				self.cached_inspector_lines = lines;
				return;
			}
		};

		// Node 섹션 헤더.
		lines.push(InspectorLine::new(Some("Node"), "", COLOR_ACCENT, false, true, false, None, false));

		// 기본 정보.
		let node_name = if node.get_name().is_empty() {
			"(unnamed)"
		}
		else {
			node.get_name()
		};
		let is_active = node.is_active();
		let child_count = node.get_child_count();
		let active_color = if is_active {
			COLOR_TRUE
		}
		else {
			COLOR_FALSE
		};
		lines.push(InspectorLine::new(Some("name"), node_name, COLOR_TEXT, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("type"), NODE_TYPE_NAME, COLOR_ACCENT, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("active"), format_boolean(is_active), active_color, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("children"), &child_count.to_string(), COLOR_TEXT, false, false, false, None, false));

		// 트랜스폼 정보.
		let local_position = node.get_local_position();
		let local_scale = node.get_local_scale();
		let content_size = node.get_content_size();
		let pivot = node.get_pivot();
		lines.push(InspectorLine::new(Some("localPos"), &format!("({:.1}, {:.1})", local_position.x, local_position.y), COLOR_TEXT, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("localScale"), &format!("({:.3}, {:.3})", local_scale.x, local_scale.y), COLOR_TEXT, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("localRot"), &format!("{:.2} deg", node.get_local_rotation()), COLOR_TEXT, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("opacity"), &format!("{:.3}", node.get_local_opacity()), COLOR_TEXT, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("contentSize"), &format!("({:.1}, {:.1})", content_size.x, content_size.y), COLOR_TEXT, false, false, false, None, false));
		lines.push(InspectorLine::new(Some("pivot"), &format!("({:.3}, {:.3})", pivot.x, pivot.y), COLOR_TEXT, false, false, false, None, false));

		// 구분선.
		lines.push(InspectorLine::new(None, "", COLOR_TEXT, true, false, false, None, false));

		// Components 섹션 헤더.
		lines.push(InspectorLine::new(Some("Components"), "", COLOR_ACCENT, false, true, false, None, false));

		let components = node.get_all_components();
		if components.is_empty() {
			lines.push(InspectorLine::new(None, "(없음)", COLOR_TEXT_DIM, false, false, false, None, false));
		}
		else {
			let reference_root_id = node_tree.get_root_id();
			for (component_index, component) in components.iter().enumerate() {
				let component_reference = (node_id, component_index);
				// 기본 펼침: collapsed_components 에 없으면 펼쳐진 상태.
				let is_expanded = !self.collapsed_components.contains(&component_reference);

				// 컴포넌트 헤더.
				lines.push(InspectorLine::new(None, component.get_component_type(), COLOR_COMPONENT, false, false, true, Some(component_reference), false));

				// 펼쳐진 경우 프로퍼티 목록. (UIDocument 에 등록하지 않은 컴포넌트는 종류 이름만)
				if !is_expanded {
					continue;
				}
				let component_properties = match self.ui_document.serialize_component(node_tree, reference_root_id, component.as_ref()) {
					Some(component_properties) => {
						component_properties
					}
					None => {
						continue;
					}
				};
				if component_properties.is_empty() {
					lines.push(InspectorLine::new(None, "(프로퍼티 없음)", COLOR_TEXT_DIM, false, false, false, None, true));
				}
				else {
					for (property_name, property_value) in component_properties.iter() {
						let property_text = format_property_value(property_name, property_value);
						lines.push(InspectorLine::new(Some(property_name), &property_text, COLOR_PROPERTY_VALUE, false, false, false, None, true));
					}
				}
			}
		}

		// 캐시 저장.
		self.cached_inspector_lines = lines;
	}

	//==============================================================================
	// 출력. (창 안쪽 논리 좌표로 그리고, 끝나면 들어올 때의 그리기 상태로 돌린다)
	//==============================================================================
	pub fn draw(&mut self, engine: &Engine, node_tree: &NodeTree, graphic: &mut Graphic) {
		if let Some(is_force_gizmos_visible) = self.pending_force_gizmos_visible.take() {
			graphic.set_force_gizmos_visible(is_force_gizmos_visible);
		}

		if !self.is_visible {
			return;
		}

		graphic.push_state();

		let panel_x = self.panel_x;
		let panel_y = self.panel_y;
		let panel_width = self.panel_width;
		let panel_height = self.panel_height;
		let left_width = self.left_width;

		// 딤드 배경. (캔버스 전체를 덮는다. 뒤에 패널을 그리므로 패널 바깥만 어두워진다)
		if self.is_dim_enabled {
			graphic.push_state();
			graphic.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
			graphic.set_fill_color(COLOR_DIM);
			let drawing_buffer_width = graphic.get_drawing_buffer_width() as f32;
			let drawing_buffer_height = graphic.get_drawing_buffer_height() as f32;
			graphic.draw_rect(Rect::create(0.0, 0.0, drawing_buffer_width, drawing_buffer_height));
			graphic.pop_state();
		}

		// 화면 비율 가이드라인 출력. (뷰 좌표계에서 그려야 의미가 있음)
		self.draw_aspect_guide(engine, graphic);

		// 패널 렌더링은 뷰 스케일 모드와 상관없이 canvas-native 좌표계에서.
		let view_manager = engine.get_view_manager();
		view_manager.apply_canvas_native_rect(graphic);

		// 배경.
		graphic.set_fill_color(COLOR_BACKGROUND);
		graphic.draw_rect(Rect::create(panel_x, panel_y, panel_width, panel_height));

		// 타이틀바 배경.
		graphic.set_fill_color(COLOR_TITLE_BACKGROUND);
		graphic.draw_rect(Rect::create(panel_x, panel_y, panel_width, TITLE_HEIGHT));

		// 타이틀 텍스트.
		graphic.set_fill_color(COLOR_ACCENT);
		graphic.set_font_string(BOLD_FONT_STRING);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.draw_fill_text("Runtime DEV Tools", panel_x + PADDING, panel_y + TITLE_HEIGHT * 0.5);

		// 닫기 버튼.
		let close_button_x = panel_x + panel_width - TITLE_HEIGHT;
		graphic.set_fill_color(COLOR_CLOSE_BUTTON);
		graphic.draw_rect(Rect::create(close_button_x, panel_y, TITLE_HEIGHT, TITLE_HEIGHT));
		graphic.set_fill_color(Color::white());
		graphic.set_font_string(BOLD_FONT_STRING);
		graphic.set_text_align(TextAlign::Center);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.draw_fill_text("X", close_button_x + TITLE_HEIGHT * 0.5, panel_y + TITLE_HEIGHT * 0.5);

		// 타이틀바 하단 구분선.
		graphic.set_stroke_color(COLOR_SPLITTER);
		graphic.draw_line(&[Vector2::create(panel_x, panel_y + TITLE_HEIGHT + 0.5), Vector2::create(panel_x + panel_width, panel_y + TITLE_HEIGHT + 0.5)], 1.0);

		// 탭바.
		let tab_width = panel_width / TABS.len() as f32;
		for (tab_index, tab_text) in TABS.iter().enumerate() {
			let tab_x = panel_x + tab_index as f32 * tab_width;
			let is_active_tab = tab_index == self.active_tab;
			if is_active_tab {
				graphic.set_fill_color(COLOR_SECTION_HEADER_BACKGROUND);
			}
			else {
				graphic.set_fill_color(COLOR_COMPONENT_HEADER_BACKGROUND);
			}
			graphic.draw_rect(Rect::create(tab_x, panel_y + TITLE_HEIGHT, tab_width, TAB_HEIGHT));
			if is_active_tab {
				graphic.set_fill_color(COLOR_ACCENT);
			}
			else {
				graphic.set_fill_color(COLOR_TEXT_DIM);
			}
			graphic.set_font_string(FONT_STRING);
			graphic.set_text_align(TextAlign::Center);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.draw_fill_text(tab_text, tab_x + tab_width * 0.5, panel_y + TITLE_HEIGHT + TAB_HEIGHT * 0.5);
		}
		// 탭바 하단 구분선.
		graphic.set_stroke_color(COLOR_SPLITTER);
		graphic.draw_line(&[Vector2::create(panel_x, panel_y + TITLE_HEIGHT + TAB_HEIGHT + 0.5), Vector2::create(panel_x + panel_width, panel_y + TITLE_HEIGHT + TAB_HEIGHT + 0.5)], 1.0);

		let content_y = panel_y + TITLE_HEIGHT + TAB_HEIGHT + 1.0;
		let content_height = panel_height - TITLE_HEIGHT - TAB_HEIGHT - 1.0;
		let right_panel_x = panel_x + left_width + 1.0;
		let right_panel_width = panel_width - left_width - 1.0;

		if self.active_tab == 0 {
			// 통계 패널 출력.
			self.draw_statistics_panel(engine, graphic, panel_x, content_y, panel_width, content_height);
		}
		else if self.active_tab == 1 {
			// 트리 패널 출력.
			self.draw_tree_panel(node_tree, graphic, panel_x, content_y, left_width, content_height);

			// 좌우 구분선.
			if self.resize_mode == ResizeMode::Splitter {
				graphic.set_fill_color(COLOR_SPLITTER_ACTIVE);
			}
			else {
				graphic.set_fill_color(COLOR_SPLITTER);
			}
			graphic.draw_rect(Rect::create(panel_x + left_width, content_y, 1.0, content_height));

			// 인스펙터 패널 출력.
			self.draw_inspector_panel(node_tree, graphic, right_panel_x, content_y, right_panel_width, content_height);
		}
		else if self.active_tab == 2 {
			// 로컬 스토리지 패널 출력.
			self.draw_local_storage_panel(graphic, panel_x, content_y, panel_width, content_height);
		}
		else {
			// 세팅 패널 출력.
			self.draw_settings_panel(graphic, panel_x, content_y, panel_width, content_height);
		}

		// 데스크탑 입력 창.
		#[cfg(not(target_arch = "wasm32"))]
		{
			if self.local_storage_dialog.is_some() {
				self.draw_local_storage_dialog(graphic, panel_x, content_y, panel_width, content_height);
			}
		}

		// 테두리. (리사이즈 중이면 강조, 늘 맨 위에 그린다)
		let is_resizing = self.resize_mode != ResizeMode::None && self.resize_mode != ResizeMode::Splitter;
		let border_width = if is_resizing {
			graphic.set_stroke_color(COLOR_BORDER_ACTIVE);
			2.0
		}
		else {
			graphic.set_stroke_color(COLOR_BORDER_NORMAL);
			1.0
		};
		graphic.draw_stroke_rect(Rect::create(panel_x + 0.5, panel_y + 0.5, panel_width - 1.0, panel_height - 1.0), border_width);

		graphic.pop_state();
	}

	//==============================================================================
	// 트리 패널 출력.
	//==============================================================================
	fn draw_tree_panel(&mut self, node_tree: &NodeTree, graphic: &mut Graphic, panel_x: f32, panel_y: f32, panel_width: f32, panel_height: f32) {
		graphic.begin_clip_rect(Rect::create(panel_x, panel_y, panel_width, panel_height), 0.0);

		// 트리 패널 제목.
		graphic.set_fill_color(COLOR_SECTION_HEADER_BACKGROUND);
		graphic.draw_rect(Rect::create(panel_x, panel_y, panel_width, ITEM_HEIGHT));
		graphic.set_fill_color(COLOR_ACCENT);
		graphic.set_font_string(BOLD_FONT_STRING);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_text_align(TextAlign::Left);
		graphic.draw_fill_text("Hierarchy", panel_x + PADDING, panel_y + ITEM_HEIGHT * 0.5);

		// 컬럼 헤더 행.
		let column_header_y = panel_y + ITEM_HEIGHT;
		let column_header_middle_y = column_header_y + ITEM_HEIGHT * 0.5;
		graphic.set_fill_color(COLOR_COMPONENT_HEADER_BACKGROUND);
		graphic.draw_rect(Rect::create(panel_x, column_header_y, panel_width, ITEM_HEIGHT));
		let header_checkbox_right_x = panel_x + panel_width - SCROLL_BAR_WIDTH - GIZMO_CHECKBOX_MARGIN;
		let header_gizmo_checkbox_x = header_checkbox_right_x - GIZMO_CHECKBOX_SIZE;
		let header_active_checkbox_x = header_gizmo_checkbox_x - GIZMO_CHECKBOX_MARGIN - GIZMO_CHECKBOX_SIZE;
		graphic.set_font_string(SMALL_FONT_STRING);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_fill_color(COLOR_TEXT_DIM);
		graphic.set_text_align(TextAlign::Left);
		graphic.draw_fill_text("Name", panel_x + PADDING + 12.0, column_header_middle_y);
		graphic.set_text_align(TextAlign::Right);
		graphic.draw_fill_text("Type", header_active_checkbox_x - GIZMO_CHECKBOX_MARGIN, column_header_middle_y);
		graphic.set_text_align(TextAlign::Center);
		graphic.draw_fill_text("A", header_active_checkbox_x + GIZMO_CHECKBOX_SIZE * 0.5, column_header_middle_y);
		graphic.draw_fill_text("G", header_gizmo_checkbox_x + GIZMO_CHECKBOX_SIZE * 0.5, column_header_middle_y);

		let items_y = panel_y + ITEM_HEIGHT * 2.0;
		let items_height = panel_height - ITEM_HEIGHT * 2.0;
		let total_items_height = self.flat_list.len() as f32 * ITEM_HEIGHT;

		if total_items_height > items_height {
			let maximum_tree_scroll_y = total_items_height - items_height;
			self.tree_scroll_y = self.tree_scroll_y.min(maximum_tree_scroll_y);
		}
		else {
			self.tree_scroll_y = 0.0;
		}

		// 항목 영역만 클립. (헤더가 스크롤에 가려지지 않도록)
		graphic.begin_clip_rect(Rect::create(panel_x, items_y, panel_width, items_height), 0.0);

		for (item_index, flat_item) in self.flat_list.iter().enumerate() {
			let item_y = items_y + item_index as f32 * ITEM_HEIGHT - self.tree_scroll_y;

			if item_y + ITEM_HEIGHT < items_y || item_y > items_y + items_height {
				continue;
			}

			let node = match node_tree.get_node(flat_item.node_id) {
				Some(node) => {
					node
				}
				None => {
					continue;
				}
			};
			let is_selected = self.selected_node_id == Some(flat_item.node_id);
			let has_children = node.get_child_count() > 0;
			let is_expanded = self.expanded_node_ids.contains(&flat_item.node_id);
			let is_active = node.is_active();
			let item_middle_y = item_y + ITEM_HEIGHT * 0.5;

			if is_selected {
				graphic.set_fill_color(COLOR_ITEM_SELECTED);
				graphic.draw_rect(Rect::create(panel_x, item_y, panel_width, ITEM_HEIGHT));
			}

			let indent_x = panel_x + PADDING + flat_item.depth as f32 * INDENT_WIDTH;

			if has_children {
				let triangle_text = if is_expanded {
					graphic.set_fill_color(COLOR_ACCENT);
					"v"
				}
				else {
					graphic.set_fill_color(COLOR_TEXT_DIM);
					">"
				};
				graphic.set_font_string(FONT_STRING);
				graphic.set_text_baseline(TextBaseline::Middle);
				graphic.set_text_align(TextAlign::Left);
				graphic.draw_fill_text(triangle_text, indent_x, item_middle_y);
			}

			let node_name = if node.get_name().is_empty() {
				"(unnamed)"
			}
			else {
				node.get_name()
			};
			if flat_item.effective_active {
				graphic.set_fill_color(COLOR_TEXT);
			}
			else {
				graphic.set_fill_color(COLOR_INACTIVE_TEXT);
			}
			graphic.set_font_string(FONT_STRING);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.set_text_align(TextAlign::Left);
			graphic.draw_fill_text(node_name, indent_x + 12.0, item_middle_y);

			graphic.set_font_string(SMALL_FONT_STRING);
			graphic.set_text_align(TextAlign::Right);

			// 기즈모 체크박스. (오른쪽 끝 SCROLL_BAR_WIDTH + GIZMO_CHECKBOX_MARGIN 안쪽)
			let checkbox_right_x = panel_x + panel_width - SCROLL_BAR_WIDTH - GIZMO_CHECKBOX_MARGIN;
			let checkbox_x = checkbox_right_x - GIZMO_CHECKBOX_SIZE;
			let checkbox_y = (item_middle_y - GIZMO_CHECKBOX_SIZE * 0.5).floor();
			// 외곽은 늘 강조색, 꺼졌으면 속을 비운다.
			draw_checkbox(graphic, checkbox_x, checkbox_y, COLOR_ACCENT, node.is_gizmo_visible());

			// 액티브 체크박스. (기즈모 체크박스 왼쪽)
			let active_checkbox_right_x = checkbox_x - GIZMO_CHECKBOX_MARGIN;
			let active_checkbox_x = active_checkbox_right_x - GIZMO_CHECKBOX_SIZE;
			let active_checkbox_color = if is_active {
				COLOR_TRUE
			}
			else {
				COLOR_FALSE
			};
			draw_checkbox(graphic, active_checkbox_x, checkbox_y, active_checkbox_color, is_active);

			// 노드 타입 텍스트. (액티브 체크박스 왼쪽에 오른쪽 정렬)
			graphic.set_fill_color(COLOR_TEXT_DIM);
			graphic.draw_fill_text(NODE_TYPE_NAME, active_checkbox_x - GIZMO_CHECKBOX_MARGIN, item_middle_y);
		}

		graphic.end_clip_rect();

		if total_items_height > items_height {
			draw_scroll_bar(graphic, panel_x + panel_width - SCROLL_BAR_WIDTH, items_y, items_height, total_items_height, self.tree_scroll_y);
		}

		graphic.end_clip_rect();
	}

	//==============================================================================
	// 인스펙터 패널 출력.
	//==============================================================================
	fn draw_inspector_panel(&mut self, node_tree: &NodeTree, graphic: &mut Graphic, panel_x: f32, panel_y: f32, panel_width: f32, panel_height: f32) {
		graphic.begin_clip_rect(Rect::create(panel_x, panel_y, panel_width, panel_height), 0.0);

		let selected_node_id = match self.selected_node_id.filter(|selected_node_id| node_tree.contains(*selected_node_id)) {
			Some(selected_node_id) => {
				selected_node_id
			}
			None => {
				self.cached_inspector_lines.clear();
				graphic.set_fill_color(COLOR_TEXT_DIM);
				graphic.set_font_string(FONT_STRING);
				graphic.set_text_align(TextAlign::Center);
				graphic.set_text_baseline(TextBaseline::Middle);
				graphic.draw_fill_text("노드를 선택하세요", panel_x + panel_width * 0.5, panel_y + panel_height * 0.5);
				graphic.end_clip_rect();
				return;
			}
		};

		self.build_inspector_lines(node_tree, selected_node_id);
		let total_content_height = self.cached_inspector_lines.len() as f32 * ITEM_HEIGHT;

		if total_content_height > panel_height {
			let maximum_inspector_scroll_y = total_content_height - panel_height;
			self.inspector_scroll_y = self.inspector_scroll_y.min(maximum_inspector_scroll_y);
		}
		else {
			self.inspector_scroll_y = 0.0;
		}

		for (line_index, line) in self.cached_inspector_lines.iter().enumerate() {
			let line_y = panel_y + line_index as f32 * ITEM_HEIGHT - self.inspector_scroll_y;

			if line_y + ITEM_HEIGHT < panel_y || line_y > panel_y + panel_height {
				continue;
			}

			let line_middle_y = line_y + ITEM_HEIGHT * 0.5;

			// 구분선.
			if line.is_separator {
				graphic.set_stroke_color(COLOR_INSPECTOR_SEPARATOR);
				graphic.draw_line(&[Vector2::create(panel_x + PADDING, line_middle_y + 0.5), Vector2::create(panel_x + panel_width - PADDING, line_middle_y + 0.5)], 1.0);
				continue;
			}

			// 섹션 헤더.
			if line.is_section_header {
				graphic.set_fill_color(COLOR_SECTION_HEADER_BACKGROUND);
				graphic.draw_rect(Rect::create(panel_x, line_y, panel_width, ITEM_HEIGHT));
				graphic.set_fill_color(line.color);
				graphic.set_font_string(BOLD_FONT_STRING);
				graphic.set_text_baseline(TextBaseline::Middle);
				graphic.set_text_align(TextAlign::Left);
				graphic.draw_fill_text(line.text.as_deref().unwrap_or(""), panel_x + PADDING, line_middle_y);
				continue;
			}

			// 컴포넌트 헤더.
			if line.is_component_header {
				graphic.set_fill_color(COLOR_COMPONENT_HEADER_BACKGROUND);
				graphic.draw_rect(Rect::create(panel_x, line_y, panel_width, ITEM_HEIGHT));
				let is_expanded = match line.component_reference {
					Some(component_reference) => {
						!self.collapsed_components.contains(&component_reference)
					}
					None => {
						true
					}
				};
				let expand_text = if is_expanded {
					graphic.set_fill_color(COLOR_ACCENT);
					"v"
				}
				else {
					graphic.set_fill_color(COLOR_TEXT_DIM);
					">"
				};
				graphic.set_font_string(FONT_STRING);
				graphic.set_text_baseline(TextBaseline::Middle);
				graphic.set_text_align(TextAlign::Left);
				graphic.draw_fill_text(expand_text, panel_x + PADDING, line_middle_y);
				graphic.set_fill_color(line.color);
				graphic.set_font_string(BOLD_FONT_STRING);
				graphic.draw_fill_text(&line.value, panel_x + PADDING + 14.0, line_middle_y);
				continue;
			}

			graphic.set_font_string(FONT_STRING);
			graphic.set_text_baseline(TextBaseline::Middle);

			// 컴포넌트 프로퍼티. (노드 프로퍼티와 같은 x 위치, 레이블이 없어도 값은 값 칸에)
			if line.is_component_property {
				graphic.set_fill_color(COLOR_TEXT_DIM);
				graphic.set_text_align(TextAlign::Left);
				graphic.draw_fill_text(line.text.as_deref().unwrap_or(""), panel_x + PADDING, line_middle_y);
				graphic.set_fill_color(line.color);
				graphic.draw_fill_text(&line.value, panel_x + PADDING + LABEL_COLUMN_WIDTH, line_middle_y);
				continue;
			}

			// 일반 레이블 + 값. (노드 프로퍼티)
			if let Some(text) = &line.text {
				graphic.set_fill_color(COLOR_TEXT_DIM);
				graphic.set_text_align(TextAlign::Left);
				graphic.draw_fill_text(text, panel_x + PADDING, line_middle_y);
				graphic.set_fill_color(line.color);
				graphic.draw_fill_text(&line.value, panel_x + PADDING + LABEL_COLUMN_WIDTH, line_middle_y);
				continue;
			}

			// 값만.
			graphic.set_fill_color(line.color);
			graphic.set_text_align(TextAlign::Left);
			graphic.draw_fill_text(&line.value, panel_x + PADDING, line_middle_y);
		}

		if total_content_height > panel_height {
			draw_scroll_bar(graphic, panel_x + panel_width - SCROLL_BAR_WIDTH, panel_y, panel_height, total_content_height, self.inspector_scroll_y);
		}

		graphic.end_clip_rect();
	}

	//==============================================================================
	// 로컬 스토리지 패널 출력.
	//==============================================================================
	fn draw_local_storage_panel(&mut self, graphic: &mut Graphic, panel_x: f32, panel_y: f32, panel_width: f32, panel_height: f32) {
		graphic.begin_clip_rect(Rect::create(panel_x, panel_y, panel_width, panel_height), 0.0);

		// 헤더 배경과 타이틀.
		draw_section_title(graphic, "Local Storage", panel_x, panel_y, panel_width);

		// + Add 버튼.
		let button_height = ITEM_HEIGHT - 4.0;
		let button_y = panel_y + 2.0;
		let clear_button_x = panel_x + panel_width - PADDING - LOCAL_STORAGE_BUTTON_WIDTH;
		let add_button_x = clear_button_x - PADDING - LOCAL_STORAGE_BUTTON_WIDTH;
		graphic.set_fill_color(COLOR_ADD_BUTTON);
		graphic.draw_rect(Rect::create(add_button_x, button_y, LOCAL_STORAGE_BUTTON_WIDTH, button_height));
		graphic.set_fill_color(COLOR_TRUE);
		graphic.set_font_string(SMALL_FONT_STRING);
		graphic.set_text_align(TextAlign::Center);
		graphic.draw_fill_text("+ Add", add_button_x + LOCAL_STORAGE_BUTTON_WIDTH * 0.5, panel_y + ITEM_HEIGHT * 0.5);

		// Clear 버튼.
		graphic.set_fill_color(COLOR_CLEAR_BUTTON);
		graphic.draw_rect(Rect::create(clear_button_x, button_y, LOCAL_STORAGE_BUTTON_WIDTH, button_height));
		graphic.set_fill_color(COLOR_FALSE);
		graphic.set_text_align(TextAlign::Center);
		graphic.draw_fill_text("Clear", clear_button_x + LOCAL_STORAGE_BUTTON_WIDTH * 0.5, panel_y + ITEM_HEIGHT * 0.5);

		// 컬럼 헤더.
		let column_header_y = panel_y + ITEM_HEIGHT;
		graphic.set_fill_color(COLOR_COMPONENT_HEADER_BACKGROUND);
		graphic.draw_rect(Rect::create(panel_x, column_header_y, panel_width, ITEM_HEIGHT));
		let available_width = panel_width - LOCAL_STORAGE_DELETE_COLUMN_WIDTH - SCROLL_BAR_WIDTH;
		let key_column_width = (available_width * 0.4).floor();
		let value_column_width = available_width - key_column_width;
		graphic.set_fill_color(COLOR_TEXT_DIM);
		graphic.set_font_string(SMALL_FONT_STRING);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.draw_fill_text("Key", panel_x + PADDING, column_header_y + ITEM_HEIGHT * 0.5);
		graphic.draw_fill_text("Value", panel_x + key_column_width + PADDING, column_header_y + ITEM_HEIGHT * 0.5);
		graphic.set_text_align(TextAlign::Center);
		graphic.draw_fill_text("×", panel_x + panel_width - SCROLL_BAR_WIDTH - LOCAL_STORAGE_DELETE_COLUMN_WIDTH * 0.5, column_header_y + ITEM_HEIGHT * 0.5);

		// 아이템 목록.
		let keys = LocalStorage::get_keys();
		let items_y = panel_y + ITEM_HEIGHT * 2.0;
		let has_selection = self.selected_local_storage_key.is_some();
		let context_menu_space = if has_selection {
			CONTEXT_MENU_HEIGHT
		}
		else {
			0.0
		};
		let items_height = panel_height - ITEM_HEIGHT * 2.0 - context_menu_space;
		let total_items_height = keys.len() as f32 * ITEM_HEIGHT;

		if total_items_height > items_height {
			let maximum_scroll_y = total_items_height - items_height;
			self.local_storage_scroll_y = self.local_storage_scroll_y.min(maximum_scroll_y);
		}
		else {
			self.local_storage_scroll_y = 0.0;
		}

		// 항목 영역만 클립. (헤더가 스크롤에 가려지지 않도록)
		graphic.begin_clip_rect(Rect::create(panel_x, items_y, panel_width, items_height), 0.0);

		let maximum_key_characters = (key_column_width / 7.0).floor() as usize;
		let maximum_value_characters = (value_column_width / 7.0).floor() as usize;
		for (key_index, key) in keys.iter().enumerate() {
			let item_y = items_y + key_index as f32 * ITEM_HEIGHT - self.local_storage_scroll_y;

			if item_y + ITEM_HEIGHT < items_y || item_y > items_y + items_height {
				continue;
			}

			let item_middle_y = item_y + ITEM_HEIGHT * 0.5;
			let is_selected = self.selected_local_storage_key.as_deref() == Some(key.as_str());

			if is_selected {
				graphic.set_fill_color(COLOR_ITEM_SELECTED);
				graphic.draw_rect(Rect::create(panel_x, item_y, panel_width - SCROLL_BAR_WIDTH, ITEM_HEIGHT));
			}

			// 행 구분선.
			graphic.set_stroke_color(COLOR_SPLITTER);
			graphic.draw_line(&[Vector2::create(panel_x, item_y + ITEM_HEIGHT - 0.5), Vector2::create(panel_x + panel_width - SCROLL_BAR_WIDTH, item_y + ITEM_HEIGHT - 0.5)], 1.0);

			// Key 텍스트.
			graphic.set_fill_color(COLOR_ACCENT);
			graphic.set_font_string(FONT_STRING);
			graphic.set_text_align(TextAlign::Left);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.draw_fill_text(&truncate_text(key, maximum_key_characters), panel_x + PADDING, item_middle_y);

			// Value 텍스트.
			graphic.set_fill_color(COLOR_PROPERTY_VALUE);
			let value_text = LocalStorage::get_item(key).unwrap_or_else(|| String::from("(null)"));
			graphic.draw_fill_text(&truncate_text(&value_text, maximum_value_characters), panel_x + key_column_width + PADDING, item_middle_y);

			// 삭제(×) 버튼.
			let delete_button_x = panel_x + panel_width - SCROLL_BAR_WIDTH - LOCAL_STORAGE_DELETE_COLUMN_WIDTH;
			graphic.set_fill_color(COLOR_DELETE_BUTTON);
			graphic.draw_rect(Rect::create(delete_button_x + 2.0, item_y + 2.0, LOCAL_STORAGE_DELETE_COLUMN_WIDTH - 4.0, ITEM_HEIGHT - 4.0));
			graphic.set_fill_color(COLOR_FALSE);
			graphic.set_font_string(FONT_STRING);
			graphic.set_text_align(TextAlign::Center);
			graphic.draw_fill_text("×", delete_button_x + LOCAL_STORAGE_DELETE_COLUMN_WIDTH * 0.5, item_middle_y);
		}

		// 항목 없음 안내.
		if keys.is_empty() {
			graphic.set_fill_color(COLOR_TEXT_DIM);
			graphic.set_font_string(FONT_STRING);
			graphic.set_text_align(TextAlign::Center);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.draw_fill_text("(비어있음)", panel_x + panel_width * 0.5, items_y + items_height * 0.5);
		}

		graphic.end_clip_rect();

		// 스크롤바.
		if total_items_height > items_height {
			draw_scroll_bar(graphic, panel_x + panel_width - SCROLL_BAR_WIDTH, items_y, items_height, total_items_height, self.local_storage_scroll_y);
		}

		// 선택 컨텍스트 메뉴.
		if has_selection {
			let context_menu_y = panel_y + panel_height - CONTEXT_MENU_HEIGHT;
			graphic.set_fill_color(COLOR_COMPONENT_HEADER_BACKGROUND);
			graphic.draw_rect(Rect::create(panel_x, context_menu_y, panel_width, CONTEXT_MENU_HEIGHT));
			graphic.set_stroke_color(COLOR_BORDER_NORMAL);
			graphic.draw_line(&[Vector2::create(panel_x, context_menu_y + 0.5), Vector2::create(panel_x + panel_width, context_menu_y + 0.5)], 1.0);

			let context_button_gap = 4.0;
			let context_button_height = CONTEXT_MENU_HEIGHT - 8.0;
			let context_button_text_padding = 10.0;
			graphic.set_font_string(SMALL_FONT_STRING);
			// 텍스트 크기 기반으로 각 버튼 너비 계산.
			let context_button_widths: Vec<f32> = CONTEXT_BUTTON_TEXTS.iter().map(|context_button_text| graphic.measure_text(context_button_text).width.ceil() + context_button_text_padding * 2.0).collect();
			let total_context_button_width = context_button_gap * (CONTEXT_BUTTON_TEXTS.len() as f32 - 1.0) + context_button_widths.iter().sum::<f32>();
			let context_button_start_x = panel_x + panel_width - PADDING - total_context_button_width;
			let context_button_y = context_menu_y + (CONTEXT_MENU_HEIGHT - context_button_height) * 0.5;
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.set_text_align(TextAlign::Center);
			self.context_button_rects.clear();
			let mut current_context_button_x = context_button_start_x;
			for (button_index, context_button_text) in CONTEXT_BUTTON_TEXTS.iter().enumerate() {
				let context_button_width = context_button_widths[button_index];
				let context_button_rect = Rect::create(current_context_button_x, context_button_y, context_button_width, context_button_height);
				self.context_button_rects.push(context_button_rect);
				graphic.set_fill_color(CONTEXT_BUTTON_COLORS[button_index]);
				graphic.draw_rect(context_button_rect);
				graphic.set_fill_color(CONTEXT_BUTTON_TEXT_COLORS[button_index]);
				graphic.draw_fill_text(context_button_text, current_context_button_x + context_button_width * 0.5, context_menu_y + CONTEXT_MENU_HEIGHT * 0.5);
				current_context_button_x += context_button_width + context_button_gap;
			}
		}

		graphic.end_clip_rect();
	}

	//==============================================================================
	// 화면 비율 가이드라인 출력. (늘이는 뷰 스케일 모드에서만, 뷰 좌표계)
	//==============================================================================
	fn draw_aspect_guide(&self, engine: &Engine, graphic: &mut Graphic) {
		if !self.is_height_aspect_guide_visible && !self.is_width_aspect_guide_visible {
			return;
		}
		let view_manager = engine.get_view_manager();
		let view_scale_mode = view_manager.get_view_scale_mode();
		let is_stretch_mode = matches!(view_scale_mode, ViewScaleMode::StretchWidth | ViewScaleMode::StretchHeight | ViewScaleMode::StretchShort | ViewScaleMode::StretchWidthExpandHeight | ViewScaleMode::StretchHeightExpandWidth | ViewScaleMode::StretchShortExpandLong);
		if !is_stretch_mode {
			return;
		}
		let view_size = view_manager.get_view_size();
		let view_width = view_size.x;
		let view_height = view_size.y;
		let center_x = view_width * 0.5;
		let center_y = view_height * 0.5;
		let ratios = [1.0, 0.75, 0.5];
		graphic.push_state();
		view_manager.apply_view_rect(graphic);

		// 세로 기준. (높이 고정 100%, 너비 = 높이 * 비율)
		if self.is_height_aspect_guide_visible {
			graphic.set_stroke_color(COLOR_HEIGHT_ASPECT_GUIDE);
			for ratio in ratios.iter() {
				let height_based_width = view_height * ratio;
				graphic.draw_stroke_rect(Rect::create(center_x - height_based_width * 0.5, center_y - view_height * 0.5, height_based_width, view_height), 2.0);
			}
		}

		// 가로 기준. (너비 고정 100%, 높이 = 너비 * 비율)
		if self.is_width_aspect_guide_visible {
			graphic.set_stroke_color(COLOR_WIDTH_ASPECT_GUIDE);
			for ratio in ratios.iter() {
				let width_based_height = view_width * ratio;
				graphic.draw_stroke_rect(Rect::create(center_x - view_width * 0.5, center_y - width_based_height * 0.5, view_width, width_based_height), 2.0);
			}
		}

		graphic.pop_state();
	}

	//==============================================================================
	// 세팅 줄의 값. (SETTINGS_ROWS 차례)
	//==============================================================================
	fn get_setting_values(&self) -> [bool; 5] {
		[self.is_dim_enabled, self.is_all_gizmos_visible, self.is_height_aspect_guide_visible, self.is_width_aspect_guide_visible, self.is_frame_per_second_visible]
	}

	//==============================================================================
	// 세팅 줄의 체크 칸 자리.
	//==============================================================================
	fn calculate_setting_checkbox_rect(row_index: usize, panel_x: f32, panel_y: f32) -> Rect {
		let row_y = panel_y + ITEM_HEIGHT * (row_index as f32 + 1.0);
		let row_middle_y = row_y + ITEM_HEIGHT * 0.5;
		let checkbox_x = panel_x + PADDING + SETTINGS_ROWS[row_index].1;
		let checkbox_y = (row_middle_y - GIZMO_CHECKBOX_SIZE * 0.5).floor();
		Rect::create(checkbox_x, checkbox_y, GIZMO_CHECKBOX_SIZE, GIZMO_CHECKBOX_SIZE)
	}

	//==============================================================================
	// 세팅 패널 출력.
	//==============================================================================
	fn draw_settings_panel(&self, graphic: &mut Graphic, panel_x: f32, panel_y: f32, panel_width: f32, panel_height: f32) {
		graphic.begin_clip_rect(Rect::create(panel_x, panel_y, panel_width, panel_height), 0.0);

		// 헤더 배경과 타이틀.
		draw_section_title(graphic, "Settings", panel_x, panel_y, panel_width);

		// 항목. (글과 체크 칸, 켜졌으면 초록, 꺼졌으면 강조색 테두리의 빈 칸)
		let setting_values = self.get_setting_values();
		for (row_index, (row_text, _)) in SETTINGS_ROWS.iter().enumerate() {
			let row_middle_y = panel_y + ITEM_HEIGHT * (row_index as f32 + 1.0) + ITEM_HEIGHT * 0.5;
			graphic.set_fill_color(COLOR_TEXT);
			graphic.set_font_string(FONT_STRING);
			graphic.set_text_align(TextAlign::Left);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.draw_fill_text(row_text, panel_x + PADDING, row_middle_y);

			let checkbox_rect = DevTools::calculate_setting_checkbox_rect(row_index, panel_x, panel_y);
			let is_checked = setting_values[row_index];
			let checkbox_color = if is_checked {
				COLOR_TRUE
			}
			else {
				COLOR_ACCENT
			};
			draw_checkbox(graphic, checkbox_rect.position.x, checkbox_rect.position.y, checkbox_color, is_checked);
		}

		graphic.end_clip_rect();
	}

	//==============================================================================
	// 세팅 탭 눌림 처리.
	//==============================================================================
	fn handle_settings_press(&mut self, touch_x: f32, touch_y: f32, panel_x: f32, panel_y: f32) {
		let hit_row_index = (0..SETTINGS_ROWS.len()).find(|row_index| is_point_in_rect(touch_x, touch_y, DevTools::calculate_setting_checkbox_rect(*row_index, panel_x, panel_y)));
		match hit_row_index {
			Some(0) => {
				self.is_dim_enabled = !self.is_dim_enabled;
			}
			Some(1) => {
				self.is_all_gizmos_visible = !self.is_all_gizmos_visible;
				self.pending_force_gizmos_visible = Some(self.is_all_gizmos_visible);
			}
			Some(2) => {
				self.is_height_aspect_guide_visible = !self.is_height_aspect_guide_visible;
			}
			Some(3) => {
				self.is_width_aspect_guide_visible = !self.is_width_aspect_guide_visible;
			}
			Some(4) => {
				self.is_frame_per_second_visible = !self.is_frame_per_second_visible;
			}
			_ => {
				return;
			}
		}
		self.save_settings();
	}

	//==============================================================================
	// 설정 불러오기. (없거나 깨졌으면 기본값, 논리 값이 아닌 칸은 건너뛴다)
	//==============================================================================
	fn load_settings(&mut self) {
		if !LocalStorage::contains_key(SETTINGS_STORAGE_KEY) {
			return;
		}
		let settings_json = LocalStorage::get_string(SETTINGS_STORAGE_KEY, "");
		let settings_object: Value = match serde_json::from_str(&settings_json) {
			Ok(settings_object) => {
				settings_object
			}
			Err(_) => {
				return;
			}
		};
		if let Some(is_dim_enabled) = settings_object.get("dimEnabled").and_then(Value::as_bool) {
			self.is_dim_enabled = is_dim_enabled;
		}
		if let Some(is_all_gizmos_visible) = settings_object.get("allGizmosVisible").and_then(Value::as_bool) {
			self.is_all_gizmos_visible = is_all_gizmos_visible;
		}
		if let Some(is_height_aspect_guide_visible) = settings_object.get("heightAspectGuideVisible").and_then(Value::as_bool) {
			self.is_height_aspect_guide_visible = is_height_aspect_guide_visible;
		}
		if let Some(is_width_aspect_guide_visible) = settings_object.get("widthAspectGuideVisible").and_then(Value::as_bool) {
			self.is_width_aspect_guide_visible = is_width_aspect_guide_visible;
		}
		if let Some(is_frame_per_second_visible) = settings_object.get("framePerSecondVisible").and_then(Value::as_bool) {
			self.is_frame_per_second_visible = is_frame_per_second_visible;
		}
	}

	//==============================================================================
	// 설정 저장. (원본 JSON.stringify 와 같은 글)
	//==============================================================================
	fn save_settings(&self) {
		let settings_object = serde_json::json!({
			"dimEnabled": self.is_dim_enabled,
			"allGizmosVisible": self.is_all_gizmos_visible,
			"heightAspectGuideVisible": self.is_height_aspect_guide_visible,
			"widthAspectGuideVisible": self.is_width_aspect_guide_visible,
			"framePerSecondVisible": self.is_frame_per_second_visible,
		});
		LocalStorage::set_string(SETTINGS_STORAGE_KEY, &settings_object.to_string());
	}

	//==============================================================================
	// FPS 표시 여부 반환.
	//==============================================================================
	pub fn is_frame_per_second_visible(&self) -> bool {
		self.is_frame_per_second_visible
	}

	//==============================================================================
	// 통계 항목 수집.
	//==============================================================================
	fn gather_statistics_items(&self, engine: &Engine) -> Vec<StatisticsItem> {
		let mut items: Vec<StatisticsItem> = Vec::new();
		let platform = engine.get_platform();
		let view_manager = engine.get_view_manager();
		let input_manager = engine.get_input_manager();
		let time_manager = engine.get_time_manager();

		items.push(StatisticsItem::create_section_header("Platform"));
		items.push(StatisticsItem::create("engineVersion", engine.get_version_string()));
		items.push(StatisticsItem::create("platformName", String::from(platform.get_platform_name().get_name())));
		items.push(StatisticsItem::create("browserName", String::from(platform.get_browser_name().get_name())));

		items.push(StatisticsItem::create_section_header("Screen"));
		items.push(StatisticsItem::create("clientNativeSize", format_vector2(view_manager.get_client_native_size())));
		items.push(StatisticsItem::create("canvasNativeSize", format_vector2(view_manager.get_canvas_native_size())));
		items.push(StatisticsItem::create("canvasPixelSize", format_vector2(view_manager.get_canvas_pixel_size())));
		items.push(StatisticsItem::create("referenceResolutionSize", format_vector2(view_manager.get_reference_resolution_size())));
		items.push(StatisticsItem::create("screenSize", String::from("(N/A)")));
		items.push(StatisticsItem::create("viewScaleMode", format!("{:?}", view_manager.get_view_scale_mode())));
		let view_native_rect = view_manager.get_view_native_rect();
		items.push(StatisticsItem::create("viewNativeRect", format!("{} - {}", format_vector2(view_native_rect.position), format_vector2(view_native_rect.size))));
		items.push(StatisticsItem::create("viewSize", format_vector2(view_manager.get_view_size())));
		items.push(StatisticsItem::create("canvasNativeInputPosition", format_vector2(input_manager.get_canvas_native_input_position())));
		items.push(StatisticsItem::create("viewInputPosition", format_vector2(input_manager.get_view_input_position())));

		items.push(StatisticsItem::create_section_header("Time"));
		items.push(StatisticsItem::create("realtimeSinceStartup", format!("{:.2}s", time_manager.get_realtime_since_startup())));
		items.push(StatisticsItem::create("time", format!("{:.2}s", time_manager.get_time())));
		items.push(StatisticsItem::create("framePerSecond", time_manager.get_frame_per_second().to_string()));
		items.push(StatisticsItem::create("timeDelta", format!("{:.3}s", time_manager.get_time_delta())));

		items.push(StatisticsItem::create_section_header("Memory (Chrome Only)"));
		match read_performance_memory() {
			Some(performance_memory) => {
				items.push(StatisticsItem::create("usedJSHeapSize", format_megabytes(performance_memory.used_heap_size)));
				items.push(StatisticsItem::create("totalJSHeapSize", format_megabytes(performance_memory.total_heap_size)));
				items.push(StatisticsItem::create("jsHeapSizeLimit", format_megabytes(performance_memory.heap_size_limit)));
			}
			None => {
				items.push(StatisticsItem::create("usedJSHeapSize", String::from("(N/A)")));
				items.push(StatisticsItem::create("totalJSHeapSize", String::from("(N/A)")));
				items.push(StatisticsItem::create("jsHeapSizeLimit", String::from("(N/A)")));
			}
		}

		items
	}

	//==============================================================================
	// 통계 패널 출력.
	//==============================================================================
	fn draw_statistics_panel(&mut self, engine: &Engine, graphic: &mut Graphic, panel_x: f32, panel_y: f32, panel_width: f32, panel_height: f32) {
		graphic.begin_clip_rect(Rect::create(panel_x, panel_y, panel_width, panel_height), 0.0);

		// 헤더 배경과 타이틀.
		draw_section_title(graphic, "Statistics", panel_x, panel_y, panel_width);

		// 항목 목록.
		let available_width = panel_width - SCROLL_BAR_WIDTH;
		let key_column_width = (available_width * 0.4).floor();
		let statistics_items = self.gather_statistics_items(engine);
		let items_y = panel_y + ITEM_HEIGHT;
		let items_height = panel_height - ITEM_HEIGHT;
		let total_items_height = statistics_items.len() as f32 * ITEM_HEIGHT;

		if total_items_height > items_height {
			let maximum_statistics_scroll_y = total_items_height - items_height;
			self.statistics_scroll_y = self.statistics_scroll_y.min(maximum_statistics_scroll_y);
		}
		else {
			self.statistics_scroll_y = 0.0;
		}

		let value_column_width = available_width - key_column_width;
		let maximum_key_characters = (key_column_width / 7.0).floor() as usize;
		let maximum_value_characters = (value_column_width / 7.0).floor() as usize;

		// 항목 영역만 클립. (헤더 타이틀이 스크롤에 가려지지 않도록)
		graphic.begin_clip_rect(Rect::create(panel_x, items_y, panel_width, items_height), 0.0);

		for (item_index, statistics_item) in statistics_items.iter().enumerate() {
			let item_y = items_y + item_index as f32 * ITEM_HEIGHT - self.statistics_scroll_y;

			if item_y + ITEM_HEIGHT < items_y || item_y > items_y + items_height {
				continue;
			}

			let item_middle_y = item_y + ITEM_HEIGHT * 0.5;

			// 구분선.
			if statistics_item.is_separator {
				graphic.set_stroke_color(COLOR_SPLITTER);
				graphic.draw_line(&[Vector2::create(panel_x, item_middle_y + 0.5), Vector2::create(panel_x + available_width, item_middle_y + 0.5)], 1.0);
				continue;
			}

			// 하위 섹션 헤더.
			if statistics_item.is_section_header {
				graphic.set_fill_color(COLOR_COMPONENT_HEADER_BACKGROUND);
				graphic.draw_rect(Rect::create(panel_x, item_y, available_width, ITEM_HEIGHT));
				// 왼쪽 강조선.
				graphic.set_fill_color(COLOR_ACCENT);
				graphic.draw_rect(Rect::create(panel_x + PADDING, item_y + 4.0, 2.0, ITEM_HEIGHT - 8.0));
				graphic.set_fill_color(COLOR_TEXT);
				graphic.set_font_string(FONT_STRING);
				graphic.set_text_align(TextAlign::Left);
				graphic.set_text_baseline(TextBaseline::Middle);
				graphic.draw_fill_text(&statistics_item.key, panel_x + PADDING + 8.0, item_middle_y);
				continue;
			}

			let is_selected = self.selected_statistics_key.as_deref() == Some(statistics_item.key.as_str());

			if is_selected {
				graphic.set_fill_color(COLOR_ITEM_SELECTED);
				graphic.draw_rect(Rect::create(panel_x, item_y, available_width, ITEM_HEIGHT));
			}

			// 행 구분선.
			graphic.set_stroke_color(COLOR_SPLITTER);
			graphic.draw_line(&[Vector2::create(panel_x, item_y + ITEM_HEIGHT - 0.5), Vector2::create(panel_x + available_width, item_y + ITEM_HEIGHT - 0.5)], 1.0);

			graphic.set_font_string(FONT_STRING);
			graphic.set_text_align(TextAlign::Left);
			graphic.set_text_baseline(TextBaseline::Middle);

			// Key 텍스트.
			graphic.set_fill_color(COLOR_ACCENT);
			graphic.draw_fill_text(&truncate_text(&statistics_item.key, maximum_key_characters), panel_x + PADDING, item_middle_y);

			// Value 텍스트.
			graphic.set_fill_color(COLOR_PROPERTY_VALUE);
			graphic.draw_fill_text(&truncate_text(&statistics_item.value, maximum_value_characters), panel_x + key_column_width + PADDING, item_middle_y);
		}

		graphic.end_clip_rect();

		// 스크롤바.
		if total_items_height > items_height {
			draw_scroll_bar(graphic, panel_x + panel_width - SCROLL_BAR_WIDTH, items_y, items_height, total_items_height, self.statistics_scroll_y);
		}

		graphic.end_clip_rect();
	}

	//==============================================================================
	// 통계 탭 눌림 처리.
	//==============================================================================
	fn handle_statistics_press(&mut self, engine: &Engine, touch_y: f32, panel_y: f32) {
		let statistics_items = self.gather_statistics_items(engine);
		let items_y = panel_y + ITEM_HEIGHT;
		let local_y = touch_y - items_y + self.statistics_scroll_y;
		let clicked_index = (local_y / ITEM_HEIGHT).floor();

		if touch_y >= items_y && clicked_index >= 0.0 && (clicked_index as usize) < statistics_items.len() {
			let clicked_item = &statistics_items[clicked_index as usize];
			if !clicked_item.is_separator && !clicked_item.is_section_header {
				if self.selected_statistics_key.as_deref() == Some(clicked_item.key.as_str()) {
					self.selected_statistics_key = None;
				}
				else {
					self.selected_statistics_key = Some(clicked_item.key.clone());
				}
			}
		}
		else if touch_y >= items_y {
			self.selected_statistics_key = None;
		}

		if touch_y >= items_y {
			self.is_statistics_scroll_dragging = true;
			self.statistics_scroll_drag_start_mouse_y = touch_y;
			self.statistics_scroll_drag_start_scroll_y = self.statistics_scroll_y;
		}
	}
}

//==============================================================================
// 데스크탑 입력 창. (원본 window.prompt, window.confirm 자리)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
impl DevTools {
	//==============================================================================
	// 입력 창 갱신. (TextInput 줄의 글과 키를 받고, IME 가 켜지도록 필드 상태를 올린다)
	//==============================================================================
	fn tick_local_storage_dialog(&mut self, engine: &mut Engine) {
		let event_kind_list = engine.get_input_manager().get_text_input().take_events_for_field(DIALOG_FIELD_ID, true);
		let mut is_confirmed = false;
		let is_cancelled = engine.get_input_manager().is_key_just_pressed(KeyCode::ESCAPE);
		let dialog_layout = self.calculate_local_storage_dialog_layout(true);
		let dialog = match self.local_storage_dialog.as_mut() {
			Some(dialog) => {
				dialog
			}
			None => {
				return;
			}
		};
		for event_kind in event_kind_list {
			match event_kind {
				TextInputEventKind::Commit(text) | TextInputEventKind::Paste(text) => {
					let printable_text: String = text.chars().filter(|character| !character.is_control()).collect();
					if dialog.is_prompt && !printable_text.is_empty() {
						dialog.value.push_str(&printable_text);
						dialog.value_revision += 1;
					}
				}
				TextInputEventKind::Preedit { text, .. } => {
					dialog.preedit_text = text;
				}
				TextInputEventKind::CompositionEnd => {
					dialog.preedit_text.clear();
				}
				TextInputEventKind::Key { key: TextInputKey::Backspace, .. } => {
					if dialog.is_prompt && dialog.value.pop().is_some() {
						dialog.value_revision += 1;
					}
				}
				TextInputEventKind::Key { key: TextInputKey::Paste, .. } => {
					if dialog.is_prompt {
						if let Some(clipboard_text) = engine.read_clipboard_text() {
							let printable_text: String = clipboard_text.chars().filter(|character| !character.is_control()).collect();
							dialog.value.push_str(&printable_text);
							dialog.value_revision += 1;
						}
					}
				}
				TextInputEventKind::Key { key: TextInputKey::Enter, .. } => {
					is_confirmed = true;
				}
				_ => {
				}
			}
		}

		// 글을 받는 창이면 필드 상태를 올려 데스크탑 IME 를 켠다. (후보 창은 커서 자리)
		if dialog.is_prompt {
			let field_rect = dialog_layout[1];
			let value_length = count_characters(&dialog.value);
			engine.get_input_manager().get_text_input().register_field_state(TextInputFieldState {
				field_id: DIALOG_FIELD_ID,
				focus_serial: u64::MAX,
				is_focused: true,
				is_attached: true,
				use_dominput: true,
				use_touch_end_focus: false,
				use_direct_touch: false,
				is_visible: true,
				field_rect,
				caret_rect: Rect::create(dialog.caret_x, field_rect.position.y, 1.0, field_rect.size.y),
				value: dialog.value.clone(),
				selection_start: value_length,
				cursor_index: value_length,
				max_length: 0,
				value_revision: dialog.value_revision,
			});
		}

		if is_confirmed {
			self.finish_local_storage_dialog(true);
		}
		else if is_cancelled {
			self.finish_local_storage_dialog(false);
		}
	}

	//==============================================================================
	// 입력 창 끝내기. (확인이면 입력한 글, 취소면 None 으로 할 일을 마친다)
	//==============================================================================
	fn finish_local_storage_dialog(&mut self, is_confirmed: bool) {
		let dialog = match self.local_storage_dialog.take() {
			Some(dialog) => {
				dialog
			}
			None => {
				return;
			}
		};
		let result = if is_confirmed {
			Some(dialog.value)
		}
		else {
			None
		};
		self.complete_local_storage_dialog(dialog.action, result);
	}

	//==============================================================================
	// 입력 창 자리. ([창, 글 칸, OK, Cancel], 패널 내용 영역 가운데)
	//==============================================================================
	fn calculate_local_storage_dialog_layout(&self, is_prompt: bool) -> [Rect; 4] {
		let content_y = self.panel_y + TITLE_HEIGHT + TAB_HEIGHT + 1.0;
		let content_height = self.panel_height - TITLE_HEIGHT - TAB_HEIGHT - 1.0;
		let box_width = (self.panel_width - PADDING * 2.0).min(DIALOG_WIDTH);
		let row_count = if is_prompt {
			3.0
		}
		else {
			2.0
		};
		let box_height = ITEM_HEIGHT * row_count + PADDING * (row_count + 1.0);
		let box_x = self.panel_x + (self.panel_width - box_width) * 0.5;
		let box_y = content_y + (content_height - box_height) * 0.5;
		let value_rect = Rect::create(box_x + PADDING, box_y + PADDING * 2.0 + ITEM_HEIGHT, box_width - PADDING * 2.0, ITEM_HEIGHT);
		let button_y = box_y + box_height - PADDING - ITEM_HEIGHT;
		let cancel_rect = Rect::create(box_x + box_width - PADDING - DIALOG_BUTTON_WIDTH, button_y, DIALOG_BUTTON_WIDTH, ITEM_HEIGHT);
		let ok_rect = Rect::create(cancel_rect.position.x - PADDING - DIALOG_BUTTON_WIDTH, button_y, DIALOG_BUTTON_WIDTH, ITEM_HEIGHT);
		[Rect::create(box_x, box_y, box_width, box_height), value_rect, ok_rect, cancel_rect]
	}

	//==============================================================================
	// 입력 창 눌림. (OK 는 확인, Cancel 은 취소, 다른 곳은 무시)
	//==============================================================================
	fn handle_local_storage_dialog_press(&mut self, touch_x: f32, touch_y: f32) {
		let is_prompt = match &self.local_storage_dialog {
			Some(dialog) => {
				dialog.is_prompt
			}
			None => {
				return;
			}
		};
		let dialog_layout = self.calculate_local_storage_dialog_layout(is_prompt);
		if is_point_in_rect(touch_x, touch_y, dialog_layout[2]) {
			self.finish_local_storage_dialog(true);
		}
		else if is_point_in_rect(touch_x, touch_y, dialog_layout[3]) {
			self.finish_local_storage_dialog(false);
		}
	}

	//==============================================================================
	// 입력 창 출력. (패널 내용을 어둡게 덮고 가운데에 창)
	//==============================================================================
	fn draw_local_storage_dialog(&mut self, graphic: &mut Graphic, panel_x: f32, content_y: f32, panel_width: f32, content_height: f32) {
		let is_prompt = match &self.local_storage_dialog {
			Some(dialog) => {
				dialog.is_prompt
			}
			None => {
				return;
			}
		};
		let dialog_layout = self.calculate_local_storage_dialog_layout(is_prompt);
		let box_rect = dialog_layout[0];
		let value_rect = dialog_layout[1];
		let dialog = match self.local_storage_dialog.as_mut() {
			Some(dialog) => {
				dialog
			}
			None => {
				return;
			}
		};

		// 패널 내용 덮기.
		graphic.set_fill_color(COLOR_DIM);
		graphic.draw_rect(Rect::create(panel_x, content_y, panel_width, content_height));

		// 창.
		graphic.set_fill_color(COLOR_TITLE_BACKGROUND);
		graphic.draw_rect(box_rect);
		graphic.set_stroke_color(COLOR_BORDER_ACTIVE);
		graphic.draw_stroke_rect(Rect::create(box_rect.position.x + 0.5, box_rect.position.y + 0.5, box_rect.size.x - 1.0, box_rect.size.y - 1.0), 1.0);

		// 묻는 글.
		graphic.set_fill_color(COLOR_TEXT);
		graphic.set_font_string(FONT_STRING);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.draw_fill_text(&dialog.message, box_rect.position.x + PADDING, box_rect.position.y + PADDING + ITEM_HEIGHT * 0.5);

		// 글 칸. (값, 조합 중인 글, 커서)
		if dialog.is_prompt {
			graphic.set_fill_color(COLOR_BACKGROUND);
			graphic.draw_rect(value_rect);
			graphic.set_stroke_color(COLOR_BORDER_NORMAL);
			graphic.draw_stroke_rect(Rect::create(value_rect.position.x + 0.5, value_rect.position.y + 0.5, value_rect.size.x - 1.0, value_rect.size.y - 1.0), 1.0);
			graphic.begin_clip_rect(value_rect, 0.0);
			let text_x = value_rect.position.x + 4.0;
			let value_middle_y = value_rect.position.y + ITEM_HEIGHT * 0.5;
			graphic.set_fill_color(COLOR_PROPERTY_VALUE);
			graphic.draw_fill_text(&dialog.value, text_x, value_middle_y);
			let value_width = graphic.measure_text(&dialog.value).width;
			graphic.set_fill_color(COLOR_TEXT_DIM);
			graphic.draw_fill_text(&dialog.preedit_text, text_x + value_width, value_middle_y);
			let preedit_width = graphic.measure_text(&dialog.preedit_text).width;
			dialog.caret_x = text_x + value_width + preedit_width;
			graphic.set_fill_color(COLOR_ACCENT);
			graphic.draw_rect(Rect::create(dialog.caret_x, value_rect.position.y + 3.0, 1.0, ITEM_HEIGHT - 6.0));
			graphic.end_clip_rect();
		}

		// 단추.
		let button_texts = ["OK", "Cancel"];
		let button_colors = [COLOR_ADD_BUTTON, COLOR_CLEAR_BUTTON];
		let button_text_colors = [COLOR_TRUE, COLOR_FALSE];
		graphic.set_font_string(SMALL_FONT_STRING);
		graphic.set_text_align(TextAlign::Center);
		for button_index in 0..2 {
			let button_rect = dialog_layout[2 + button_index];
			graphic.set_fill_color(button_colors[button_index]);
			graphic.draw_rect(button_rect);
			graphic.set_fill_color(button_text_colors[button_index]);
			graphic.draw_fill_text(button_texts[button_index], button_rect.position.x + button_rect.size.x * 0.5, button_rect.position.y + button_rect.size.y * 0.5);
		}
	}
}

impl Default for DevTools {
	fn default() -> DevTools {
		DevTools::new()
	}
}

//==============================================================================
// 점이 사각형 안(경계 포함)에 있는지. (원본의 >= / <= 비교)
//==============================================================================
fn is_point_in_rect(x: f32, y: f32, rect: Rect) -> bool {
	x >= rect.position.x && x <= rect.position.x + rect.size.x && y >= rect.position.y && y <= rect.position.y + rect.size.y
}

//==============================================================================
// 체크 칸 출력. (테두리 색으로 채우고, 꺼졌으면 속을 빈 칸 색으로 덮는다)
//==============================================================================
fn draw_checkbox(graphic: &mut Graphic, checkbox_x: f32, checkbox_y: f32, border_color: Color, is_checked: bool) {
	graphic.set_fill_color(border_color);
	graphic.draw_rect(Rect::create(checkbox_x, checkbox_y, GIZMO_CHECKBOX_SIZE, GIZMO_CHECKBOX_SIZE));
	if !is_checked {
		graphic.set_fill_color(COLOR_CHECKBOX_EMPTY);
		graphic.draw_rect(Rect::create(checkbox_x + 2.0, checkbox_y + 2.0, GIZMO_CHECKBOX_SIZE - 4.0, GIZMO_CHECKBOX_SIZE - 4.0));
	}
}

//==============================================================================
// 탭 내용의 제목 줄 출력. (Local Storage, Settings, Statistics 의 헤더 배경과 타이틀)
//==============================================================================
fn draw_section_title(graphic: &mut Graphic, title: &str, panel_x: f32, panel_y: f32, panel_width: f32) {
	graphic.set_fill_color(COLOR_SECTION_HEADER_BACKGROUND);
	graphic.draw_rect(Rect::create(panel_x, panel_y, panel_width, ITEM_HEIGHT));
	graphic.set_fill_color(COLOR_ACCENT);
	graphic.set_font_string(BOLD_FONT_STRING);
	graphic.set_text_align(TextAlign::Left);
	graphic.set_text_baseline(TextBaseline::Middle);
	graphic.draw_fill_text(title, panel_x + PADDING, panel_y + ITEM_HEIGHT * 0.5);
}

//==============================================================================
// 세로 스크롤바 출력. (막대 높이는 보이는 비율, 최소 20)
//==============================================================================
fn draw_scroll_bar(graphic: &mut Graphic, bar_x: f32, area_y: f32, area_height: f32, total_height: f32, scroll_y: f32) {
	let maximum_scroll_y = total_height - area_height;
	let scroll_ratio = scroll_y / maximum_scroll_y;
	let bar_height = (area_height * area_height / total_height).max(20.0);
	let bar_y = area_y + scroll_ratio * (area_height - bar_height);
	graphic.set_fill_color(COLOR_SCROLLBAR);
	graphic.draw_rect(Rect::create(bar_x, bar_y, SCROLL_BAR_WIDTH, bar_height));
}

//==============================================================================
// 글자 수로 자르기. (넘치면 앞의 maximum_characters - 3 글자 + "...", 원본 slice 와 같다)
//==============================================================================
fn truncate_text(text: &str, maximum_characters: usize) -> String {
	if text.chars().count() > maximum_characters {
		let kept_text: String = text.chars().take(maximum_characters.saturating_sub(3)).collect();
		format!("{}...", kept_text)
	}
	else {
		String::from(text)
	}
}

//==============================================================================
// 논리 값 글.
//==============================================================================
fn format_boolean(value: bool) -> &'static str {
	if value {
		"true"
	}
	else {
		"false"
	}
}

//==============================================================================
// 벡터 글. (원본 `(${x}, ${y})`, 수는 가장 짧은 십진)
//==============================================================================
fn format_vector2(vector: Vector2) -> String {
	format!("({}, {})", vector.x, vector.y)
}

//==============================================================================
// 바이트를 메가바이트 글로. (원본 (바이트 / (1024 * 1024)).toFixed(2) MB)
//==============================================================================
fn format_megabytes(bytes: f64) -> String {
	format!("{:.2} MB", bytes / (1024.0 * 1024.0))
}

//==============================================================================
// 자바스크립트 힙 크기. (원본 performance.memory 의 usedJSHeapSize, totalJSHeapSize, jsHeapSizeLimit, 바이트)
//==============================================================================
struct PerformanceMemory {
	used_heap_size: f64,
	total_heap_size: f64,
	heap_size_limit: f64,
}

//==============================================================================
// 자바스크립트 힙 크기 읽기. (웹: Chrome 의 performance.memory, 없는 브라우저는 None)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_performance_memory() -> Option<PerformanceMemory> {
	use wasm_bindgen::JsValue;
	let performance = web_sys::window()?.performance()?;
	let memory = js_sys::Reflect::get(&performance, &JsValue::from_str("memory")).ok()?;
	if memory.is_undefined() || memory.is_null() {
		return None;
	}
	let read_size = |key: &str| -> Option<f64> {
		js_sys::Reflect::get(&memory, &JsValue::from_str(key)).ok()?.as_f64()
	};
	Some(PerformanceMemory {
		used_heap_size: read_size("usedJSHeapSize")?,
		total_heap_size: read_size("totalJSHeapSize")?,
		heap_size_limit: read_size("jsHeapSizeLimit")?,
	})
}

//==============================================================================
// 자바스크립트 힙 크기 읽기. (데스크탑에는 자바스크립트 힙이 없어 None, 원본에서 performance.memory 가 없는 브라우저와 같다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_performance_memory() -> Option<PerformanceMemory> {
	None
}

//==============================================================================
// 프로퍼티 값을 문자열로 포맷. (원본 formatPropertyValue 를 UIDocument 의 JSON 값에 맞춘 것)
// - 수 둘 배열은 벡터, 이름에 color 가 든 수 넷 배열은 rgba, 그 밖의 수 넷 배열은 사각형으로 본다.
//==============================================================================
fn format_property_value(property_name: &str, value: &Value) -> String {
	match value {
		Value::Null => {
			String::from("null")
		}
		Value::Bool(boolean_value) => {
			String::from(format_boolean(*boolean_value))
		}
		Value::Number(number_value) => {
			format!("{:.2}", number_value.as_f64().unwrap_or(0.0))
		}
		Value::String(string_value) => {
			let truncated_text = if string_value.chars().count() > 28 {
				let kept_text: String = string_value.chars().take(25).collect();
				format!("{}...", kept_text)
			}
			else {
				string_value.clone()
			};
			format!("\"{}\"", truncated_text)
		}
		Value::Array(item_values) => {
			let numbers: Option<Vec<f64>> = item_values.iter().map(Value::as_f64).collect();
			match numbers {
				Some(numbers) if numbers.len() == 2 => {
					format!("({:.2}, {:.2})", numbers[0], numbers[1])
				}
				Some(numbers) if numbers.len() == 4 && property_name.to_lowercase().contains("color") => {
					let red = (numbers[0] * 255.0).round();
					let green = (numbers[1] * 255.0).round();
					let blue = (numbers[2] * 255.0).round();
					format!("rgba({}, {}, {}, {:.2})", red, green, blue, numbers[3])
				}
				Some(numbers) if numbers.len() == 4 => {
					format!("({:.1}, {:.1}, {:.1}, {:.1})", numbers[0], numbers[1], numbers[2], numbers[3])
				}
				_ => {
					String::from("[Array]")
				}
			}
		}
		Value::Object(object_value) => {
			if object_value.contains_key(NODE_REFERENCE_KEY) {
				format!("[{}]", NODE_TYPE_NAME)
			}
			else {
				String::from("[Object]")
			}
		}
	}
}

//==============================================================================
// 시험. (데스크탑, 로컬 스토리지는 임시 폴더에 쓴다)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use crate::core::component::paint::Paint;
	use crate::core::component::{Component, ComponentBase};
	use crate::core::engine::EngineConfiguration;
	use std::path::PathBuf;

	//==============================================================================
	// 시험마다 다른 임시 폴더를 로컬 스토리지 폴더로 정한다.
	//==============================================================================
	fn use_temporary_storage_directory(test_name: &str) -> PathBuf {
		let unique_suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0);
		let storage_directory_path = std::env::temp_dir().join(format!("vanilla-devtools-{}-{}-{}", test_name, std::process::id(), unique_suffix));
		LocalStorage::set_storage_directory(Some(storage_directory_path.clone()));
		storage_directory_path
	}

	//==============================================================================
	// 시험용 트리. (root → a(b, c(d)), e)
	//==============================================================================
	fn create_test_tree() -> (NodeTree, [NodeId; 6]) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let a_id = node_tree.create_child(root_id, "a");
		let b_id = node_tree.create_child(a_id, "b");
		let c_id = node_tree.create_child(a_id, "c");
		let d_id = node_tree.create_child(c_id, "d");
		let e_id = node_tree.create_child(root_id, "e");
		(node_tree, [root_id, a_id, b_id, c_id, d_id, e_id])
	}

	//==============================================================================
	// 플랫 목록의 노드 차례.
	//==============================================================================
	fn read_flat_node_ids(dev_tools: &DevTools) -> Vec<NodeId> {
		dev_tools.flat_list.iter().map(|flat_item| flat_item.node_id).collect()
	}

	//==============================================================================
	// 1280 x 720 창의 엔진.
	//==============================================================================
	fn create_test_engine() -> Engine {
		let mut engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(1280, 720, 1.0);
		engine
	}

	//==============================================================================
	// 시험용 컴포넌트. (UIDocument 에 등록하지 않은 종류)
	//==============================================================================
	struct UnregisteredComponent {
		component_base: ComponentBase,
	}

	impl Component for UnregisteredComponent {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"UnregisteredComponent"
		}
	}

	//==============================================================================
	// 펼치기와 접기에 따라 플랫 목록과 깊이가 바뀐다.
	//==============================================================================
	#[test]
	fn flat_list_follows_expand_and_collapse() {
		let storage_directory_path = use_temporary_storage_directory("flatlist");
		let (node_tree, [root_id, a_id, b_id, c_id, d_id, e_id]) = create_test_tree();
		let mut dev_tools = DevTools::new();
		dev_tools.set_root_nodes(vec![root_id]);
		dev_tools.rebuild_flat_list(&node_tree);
		assert_eq!(read_flat_node_ids(&dev_tools), vec![root_id, a_id, e_id]);

		dev_tools.set_node_expanded(a_id, true);
		dev_tools.set_node_expanded(c_id, true);
		dev_tools.rebuild_flat_list(&node_tree);
		assert_eq!(read_flat_node_ids(&dev_tools), vec![root_id, a_id, b_id, c_id, d_id, e_id]);
		let depths: Vec<usize> = dev_tools.flat_list.iter().map(|flat_item| flat_item.depth).collect();
		assert_eq!(depths, vec![0, 1, 2, 2, 3, 1]);

		// 접은 노드의 자손은 펼친 표시가 남아 있어도 빠진다.
		dev_tools.set_node_expanded(a_id, false);
		dev_tools.rebuild_flat_list(&node_tree);
		assert_eq!(read_flat_node_ids(&dev_tools), vec![root_id, a_id, e_id]);
		dev_tools.set_node_expanded(root_id, false);
		dev_tools.rebuild_flat_list(&node_tree);
		assert_eq!(read_flat_node_ids(&dev_tools), vec![root_id]);
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	// 비활성 노드 아래는 모두 effective_active 가 거짓이다. 지운 노드는 빠진다.
	//==============================================================================
	#[test]
	fn effective_active_propagates_to_descendants() {
		let storage_directory_path = use_temporary_storage_directory("active");
		let (mut node_tree, [root_id, a_id, b_id, c_id, d_id, e_id]) = create_test_tree();
		node_tree[a_id].set_active(false);
		let mut dev_tools = DevTools::new();
		dev_tools.set_root_nodes(vec![root_id]);
		dev_tools.set_node_expanded(a_id, true);
		dev_tools.set_node_expanded(c_id, true);
		dev_tools.rebuild_flat_list(&node_tree);
		let effective_actives: Vec<bool> = dev_tools.flat_list.iter().map(|flat_item| flat_item.effective_active).collect();
		assert_eq!(effective_actives, vec![true, false, false, false, false, true]);

		node_tree.destroy_node(c_id);
		dev_tools.rebuild_flat_list(&node_tree);
		assert_eq!(read_flat_node_ids(&dev_tools), vec![root_id, a_id, b_id, e_id]);
		assert!(!read_flat_node_ids(&dev_tools).contains(&d_id));
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	// 크기 바꾸기는 최소 크기와 스플리터 범위에서 멈춘다.
	//==============================================================================
	#[test]
	fn panel_resize_is_clamped() {
		let storage_directory_path = use_temporary_storage_directory("resize");
		let mut dev_tools = DevTools::new();
		dev_tools.panel_x = 100.0;
		dev_tools.panel_y = 100.0;

		// 오른쪽: 최소 너비에서 멈춘다.
		dev_tools.start_resize(ResizeMode::Right, 910.0, 300.0);
		dev_tools.handle_move(100.0, 300.0);
		assert_eq!(dev_tools.panel_width, MIN_PANEL_WIDTH);
		dev_tools.handle_release(100.0, 300.0);
		assert_eq!(dev_tools.resize_mode, ResizeMode::None);

		// 왼쪽: 최소보다 작아지면 움직이지 않고, 넓히면 x 도 같이 움직인다.
		dev_tools.start_resize(ResizeMode::Left, 100.0, 300.0);
		dev_tools.handle_move(150.0, 300.0);
		assert_eq!(dev_tools.panel_width, MIN_PANEL_WIDTH);
		assert_eq!(dev_tools.panel_x, 100.0);
		dev_tools.handle_move(40.0, 300.0);
		assert_eq!(dev_tools.panel_width, MIN_PANEL_WIDTH + 60.0);
		assert_eq!(dev_tools.panel_x, 40.0);
		dev_tools.handle_release(40.0, 300.0);

		// 위: 높이 최소, 아래 오른쪽: 둘 다.
		dev_tools.start_resize(ResizeMode::Top, 200.0, 100.0);
		dev_tools.handle_move(200.0, 500.0);
		assert_eq!(dev_tools.panel_height, DEFAULT_PANEL_HEIGHT);
		dev_tools.handle_move(200.0, 50.0);
		assert_eq!(dev_tools.panel_height, DEFAULT_PANEL_HEIGHT + 50.0);
		assert_eq!(dev_tools.panel_y, 50.0);
		dev_tools.handle_release(200.0, 50.0);
		dev_tools.start_resize(ResizeMode::BottomRight, 400.0, 620.0);
		dev_tools.handle_move(0.0, 0.0);
		assert_eq!(dev_tools.panel_width, MIN_PANEL_WIDTH);
		assert_eq!(dev_tools.panel_height, MIN_PANEL_HEIGHT);
		dev_tools.handle_release(0.0, 0.0);

		// 스플리터: MIN_LEFT_WIDTH ~ panel_width - MIN_LEFT_WIDTH.
		dev_tools.start_resize(ResizeMode::Splitter, 300.0, 300.0);
		dev_tools.handle_move(-1000.0, 300.0);
		assert_eq!(dev_tools.left_width, MIN_LEFT_WIDTH);
		dev_tools.handle_move(5000.0, 300.0);
		assert_eq!(dev_tools.left_width, dev_tools.panel_width - MIN_LEFT_WIDTH);
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	// 설정은 원본과 같은 JSON 으로 남고 새 DevTools 가 읽는다. 깨진 글은 기본값.
	//==============================================================================
	#[test]
	fn settings_round_trip_through_local_storage() {
		let storage_directory_path = use_temporary_storage_directory("settings");
		let mut dev_tools = DevTools::new();
		assert!(dev_tools.is_all_gizmos_visible);
		assert!(!dev_tools.is_frame_per_second_visible());
		dev_tools.is_dim_enabled = true;
		dev_tools.is_all_gizmos_visible = false;
		dev_tools.is_width_aspect_guide_visible = true;
		dev_tools.is_frame_per_second_visible = true;
		dev_tools.save_settings();
		assert_eq!(LocalStorage::get_string(SETTINGS_STORAGE_KEY, ""), "{\"dimEnabled\":true,\"allGizmosVisible\":false,\"heightAspectGuideVisible\":false,\"widthAspectGuideVisible\":true,\"framePerSecondVisible\":true}");

		let loaded_dev_tools = DevTools::new();
		assert_eq!(loaded_dev_tools.get_setting_values(), [true, false, false, true, true]);

		// 논리 값이 아닌 칸은 건너뛰고, 깨진 글은 모두 기본값.
		LocalStorage::set_string(SETTINGS_STORAGE_KEY, "{\"dimEnabled\":1,\"framePerSecondVisible\":true}");
		assert_eq!(DevTools::new().get_setting_values(), [false, true, false, false, true]);
		LocalStorage::set_string(SETTINGS_STORAGE_KEY, "{broken");
		assert_eq!(DevTools::new().get_setting_values(), [false, true, false, false, false]);

		// 세팅 탭의 칸을 누르면 바뀌고 저장된다. (모든 기즈모는 다음 draw 에 걸 값도 적는다)
		let mut pressed_dev_tools = DevTools::new();
		let checkbox_rect = DevTools::calculate_setting_checkbox_rect(1, 0.0, 0.0);
		pressed_dev_tools.handle_settings_press(checkbox_rect.position.x + 1.0, checkbox_rect.position.y + 1.0, 0.0, 0.0);
		assert!(!pressed_dev_tools.is_all_gizmos_visible);
		assert_eq!(pressed_dev_tools.pending_force_gizmos_visible, Some(false));
		assert_eq!(DevTools::new().get_setting_values()[1], false);
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	// 프로퍼티 값 글은 원본 formatPropertyValue 와 같은 꼴이다.
	//==============================================================================
	#[test]
	fn property_values_are_formatted_like_original() {
		assert_eq!(format_property_value("enable", &serde_json::json!(true)), "true");
		assert_eq!(format_property_value("roundSize", &serde_json::json!(4)), "4.00");
		assert_eq!(format_property_value("fontSize", &serde_json::json!(12.345)), "12.35");
		assert_eq!(format_property_value("text", &serde_json::json!("hello")), "\"hello\"");
		assert_eq!(format_property_value("text", &serde_json::json!("abcdefghijklmnopqrstuvwxyz0123")), "\"abcdefghijklmnopqrstuvwxy...\"");
		assert_eq!(format_property_value("anchor", &serde_json::json!([0.5, 1])), "(0.50, 1.00)");
		assert_eq!(format_property_value("textColor", &serde_json::json!([1, 0.5, 0, 0.25])), "rgba(255, 128, 0, 0.25)");
		assert_eq!(format_property_value("nineSlice", &serde_json::json!([1, 2, 3.5, 4])), "(1.0, 2.0, 3.5, 4.0)");
		assert_eq!(format_property_value("list", &serde_json::json!(["a"])), "[Array]");
		assert_eq!(format_property_value("target", &serde_json::json!({ "$nodeRef": "a/b" })), "[WorldNode]");
		assert_eq!(format_property_value("other", &serde_json::json!({ "x": 1 })), "[Object]");
		assert_eq!(format_property_value("nothing", &Value::Null), "null");
		assert_eq!(truncate_text("abcdefghij", 8), "abcde...");
		assert_eq!(truncate_text("abc", 8), "abc");
	}

	//==============================================================================
	// 인스펙터는 노드 정보와 UIDocument 가 저장하는 컴포넌트 속성을 보이고, 접은 컴포넌트는 머리만 남긴다.
	//==============================================================================
	#[test]
	fn inspector_lines_show_serialized_component_properties() {
		let storage_directory_path = use_temporary_storage_directory("inspector");
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "box");
		node_tree[node_id].add_component(Paint::create_with_color("#ff8000"));
		node_tree[node_id].add_component(UnregisteredComponent {
			component_base: ComponentBase::new(),
		});
		let mut dev_tools = DevTools::new();
		dev_tools.build_inspector_lines(&node_tree, node_id);
		let line_texts: Vec<(Option<String>, String)> = dev_tools.cached_inspector_lines.iter().map(|line| (line.text.clone(), line.value.clone())).collect();
		assert!(line_texts.contains(&(Some(String::from("name")), String::from("box"))));
		assert!(line_texts.contains(&(Some(String::from("type")), String::from("WorldNode"))));
		assert!(line_texts.contains(&(Some(String::from("color")), String::from("rgba(255, 128, 0, 1.00)"))));
		assert!(line_texts.contains(&(Some(String::from("roundSize")), String::from("0.00"))));
		let header_lines: Vec<&InspectorLine> = dev_tools.cached_inspector_lines.iter().filter(|line| line.is_component_header).collect();
		assert_eq!(header_lines.len(), 2);
		assert_eq!(header_lines[0].value, "Paint");
		assert_eq!(header_lines[1].value, "UnregisteredComponent");
		// 등록하지 않은 컴포넌트는 머리가 마지막 줄이다.
		assert!(dev_tools.cached_inspector_lines.last().map(|line| line.is_component_header).unwrap_or(false));

		// Paint 를 접으면 속성 줄이 빠진다.
		let line_count = dev_tools.cached_inspector_lines.len();
		dev_tools.collapsed_components.insert((node_id, 0));
		dev_tools.build_inspector_lines(&node_tree, node_id);
		assert_eq!(dev_tools.cached_inspector_lines.len(), line_count - 2);
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	// F2 로 오른쪽 아래에 보이고, 트리 줄을 눌러 고르고 펼치고, A, G 칸으로 액티브와 기즈모를 바꾼다.
	//==============================================================================
	#[test]
	fn tick_toggles_with_f2_and_tree_press_changes_nodes() {
		let storage_directory_path = use_temporary_storage_directory("tick");
		let mut engine = create_test_engine();
		let (mut node_tree, [root_id, a_id, b_id, _c_id, _d_id, _e_id]) = create_test_tree();
		let mut dev_tools = DevTools::new();
		dev_tools.set_root_nodes(vec![root_id]);

		engine.get_input_manager_mut().push_key(KeyCode::F2);
		dev_tools.tick(&mut engine, &mut node_tree, 0.016);
		assert!(dev_tools.is_visible());
		assert_eq!(dev_tools.get_panel_rect(), Rect::create(1280.0 - DEFAULT_PANEL_WIDTH, 720.0 - DEFAULT_PANEL_HEIGHT, DEFAULT_PANEL_WIDTH, DEFAULT_PANEL_HEIGHT));
		assert_eq!(dev_tools.pending_force_gizmos_visible, Some(true));

		// 누른 채면 다시 토글하지 않는다.
		dev_tools.tick(&mut engine, &mut node_tree, 0.016);
		assert!(dev_tools.is_visible());

		// 둘째 줄(a)의 이름을 누르면 고르고 펼친다.
		let panel_rect = dev_tools.get_panel_rect();
		let content_y = panel_rect.position.y + TITLE_HEIGHT + TAB_HEIGHT + 1.0;
		let row_middle_y = content_y + ITEM_HEIGHT * 2.0 + ITEM_HEIGHT * 1.5;
		let name_x = panel_rect.position.x + 60.0;
		dev_tools.handle_press(&engine, &mut node_tree, name_x, row_middle_y);
		assert_eq!(dev_tools.get_selected_node(), Some(a_id));
		assert!(dev_tools.is_node_expanded(a_id));
		dev_tools.handle_release(name_x, row_middle_y);
		dev_tools.rebuild_flat_list(&node_tree);
		assert_eq!(dev_tools.flat_list[2].node_id, b_id);

		// G 칸과 A 칸.
		let gizmo_checkbox_x = panel_rect.position.x + DEFAULT_LEFT_WIDTH - SCROLL_BAR_WIDTH - GIZMO_CHECKBOX_MARGIN - GIZMO_CHECKBOX_SIZE * 0.5;
		dev_tools.handle_press(&engine, &mut node_tree, gizmo_checkbox_x, row_middle_y);
		assert!(node_tree[a_id].is_gizmo_visible());
		let active_checkbox_x = gizmo_checkbox_x - GIZMO_CHECKBOX_SIZE - GIZMO_CHECKBOX_MARGIN;
		dev_tools.handle_press(&engine, &mut node_tree, active_checkbox_x, row_middle_y);
		assert!(!node_tree[a_id].is_active());

		// 탭 누르기와 패널 안 판정.
		dev_tools.handle_press(&engine, &mut node_tree, panel_rect.position.x + 20.0, panel_rect.position.y + TITLE_HEIGHT + TAB_HEIGHT * 0.5);
		assert_eq!(dev_tools.get_active_tab(), 0);
		engine.get_input_manager_mut().set_canvas_native_input_position(Vector2::create(panel_rect.position.x + 5.0, 700.0));
		assert!(dev_tools.is_pointer_inside_panel(&engine));
		engine.get_input_manager_mut().set_canvas_native_input_position(Vector2::create(10.0, 10.0));
		assert!(!dev_tools.is_pointer_inside_panel(&engine));

		// 닫기 단추.
		dev_tools.handle_press(&engine, &mut node_tree, panel_rect.position.x + panel_rect.size.x - 5.0, panel_rect.position.y + 14.0);
		assert!(!dev_tools.is_visible());
		assert_eq!(dev_tools.pending_force_gizmos_visible, Some(false));
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}

	//==============================================================================
	// 로컬 스토리지 입력 창: 키 → 값 차례로 묻고, 고치고, 지운다. (데스크탑 창)
	//==============================================================================
	#[test]
	fn local_storage_dialog_actions() {
		let storage_directory_path = use_temporary_storage_directory("dialog");
		let mut dev_tools = DevTools::new();
		dev_tools.complete_local_storage_dialog(LocalStorageDialogAction::AddKey, Some(String::from("  game.name ")));
		let dialog_action = dev_tools.local_storage_dialog.as_ref().map(|dialog| dialog.action.clone());
		assert_eq!(dialog_action, Some(LocalStorageDialogAction::AddValue(String::from("game.name"))));
		dev_tools.local_storage_dialog.as_mut().expect("dialog").value = String::from("바닐라");
		dev_tools.finish_local_storage_dialog(true);
		assert_eq!(LocalStorage::get_string("game.name", ""), "바닐라");
		assert!(dev_tools.local_storage_dialog.is_none());

		// 빈 키는 묻지 않는다.
		dev_tools.complete_local_storage_dialog(LocalStorageDialogAction::AddKey, Some(String::from("   ")));
		assert!(dev_tools.local_storage_dialog.is_none());

		dev_tools.selected_local_storage_key = Some(String::from("game.name"));
		dev_tools.complete_local_storage_dialog(LocalStorageDialogAction::EditKey(String::from("game.name")), Some(String::from("game.title")));
		assert_eq!(LocalStorage::get_string("game.title", ""), "바닐라");
		assert!(!LocalStorage::contains_key("game.name"));
		assert_eq!(dev_tools.selected_local_storage_key.as_deref(), Some("game.title"));

		dev_tools.complete_local_storage_dialog(LocalStorageDialogAction::EditValue(String::from("game.title")), None);
		assert_eq!(LocalStorage::get_string("game.title", ""), "바닐라");
		dev_tools.complete_local_storage_dialog(LocalStorageDialogAction::Remove(String::from("game.title")), None);
		assert!(LocalStorage::contains_key("game.title"));
		dev_tools.complete_local_storage_dialog(LocalStorageDialogAction::Remove(String::from("game.title")), Some(String::new()));
		assert!(!LocalStorage::contains_key("game.title"));
		assert!(dev_tools.selected_local_storage_key.is_none());
		let _ = std::fs::remove_dir_all(&storage_directory_path);
	}
}
