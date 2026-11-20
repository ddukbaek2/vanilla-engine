//==============================================================================
// 입력 필드 갤러리. (바닐라엔진 UIInputField, RichText 예제)
// - 입력 필드 넷: 포커스를 받아 한글을 치는 칸, 안내 글만 있는 칸, 선택 영역이 남은 칸, 캔버스 모드(use_dominput 꺼짐) 칸.
// - 리치텍스트 라벨 셋: 굵게, 색, 크기, 밑줄, 취소선, 기울임, 모르는 태그. 정렬을 달리한다.
// - 사람이 없어도 확인할 수 있게, 정해 둔 프레임에 입력을 흉내 낸다. (엔진 글 입력 줄에 직접 넣는다)
//   5 프레임: 첫 칸을 누름 → 12, 16: 확정 글 "안녕하세요 ", "vanilla" → 30 ~ 36: 조합 "ㅎ" → "하" → "한" (밑줄과 커서)
//   130: 셋째 칸에 오른쪽 단추(문맥 메뉴) → 140 부터: 메뉴의 "복사" 위에 마우스.
// - 200 프레임이 지나면 스스로 끝난다. (화면 저장: VANILLA_CAPTURE_PATH / VANILLA_CAPTURE_FRAMES / VANILLA_CAPTURE_EXIT)
//==============================================================================
#![allow(clippy::too_many_arguments)]

use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{Engine, EngineConfiguration, Graphic, NodeId, NodeTree, Paint, Pivot, Scene, SceneBase, TextInputEventKind, UIInputField, UILabel, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#e9edf3";
const PANEL_COLOR: &str = "#ffffff";
const TITLE_COLOR: &str = "#1f2937";
const CAPTION_COLOR: &str = "#6b7280";
const ACCENT_COLOR: &str = "#2563eb";
const FIELD_WIDTH: f32 = 520.0;
const FIELD_HEIGHT: f32 = 64.0;
const EXIT_FRAME_COUNT: u32 = 200;

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
fn create_label_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, font_size: f32, color: &str) -> NodeId {
	let label_id = create_box_node(node_tree, parent_id, name, position, size);
	let label = node_tree.add_component(label_id, UILabel::create_with_text(text, font_size)).expect("label");
	label.set_text_color(color);
	label.set_text_align(TextAlign::Left);
	label.set_text_baseline(TextBaseline::Middle);
	label_id
}

//==============================================================================
// 입력 필드 노드 만들기. (위에 설명 글, DOM 입력 여부, attach)
//==============================================================================
fn create_field_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, caption: &str, use_dominput: bool) -> NodeId {
	create_label_node(node_tree, parent_id, "caption", Vector2::create(position.x, position.y - 28.0), Vector2::create(FIELD_WIDTH, 24.0), caption, 16.0, CAPTION_COLOR);
	let field_id = create_box_node(node_tree, parent_id, name, position, Vector2::create(FIELD_WIDTH, FIELD_HEIGHT));
	let field = node_tree.add_component(field_id, UIInputField::new()).expect("field");
	field.set_use_dominput(use_dominput);
	field.set_background_color("#f8fafc");
	field.set_cursor_color(ACCENT_COLOR);
	field.set_focus_border_color(ACCENT_COLOR);
	field.set_focus_border_width(2.0);
	field.set_round_size(12.0);
	field.attach();
	field_id
}

//==============================================================================
// 리치텍스트 라벨 만들기.
//==============================================================================
fn create_rich_label_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, markup: &str, text_align: TextAlign) -> NodeId {
	let label_id = create_box_node(node_tree, parent_id, name, position, size);
	node_tree[label_id].add_component(Paint::create_with_color("#f1f5f9")).set_round_size(10.0);
	let label = node_tree.add_component(label_id, UILabel::create_with_text(markup, 26.0)).expect("label");
	label.use_rich_text(true);
	label.set_text_color(TITLE_COLOR);
	label.set_text_align(text_align);
	label.set_text_baseline(TextBaseline::Middle);
	label_id
}

//==============================================================================
// 노드 안의 점 → 뷰 좌표.
//==============================================================================
fn calculate_node_point(node_tree: &NodeTree, node_id: NodeId, local_position: Vector2) -> Vector2 {
	node_tree.convert_node_position_to_view_position(node_id, local_position)
}

//==============================================================================
// 입력 필드 갤러리 씬.
//==============================================================================
struct InputGalleryScene {
	scene_base: SceneBase,
	frame_count: u32,
	typing_field_id: Option<NodeId>,
	selection_field_id: Option<NodeId>,
}

impl InputGalleryScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> InputGalleryScene {
		InputGalleryScene {
			scene_base: SceneBase::new(),
			frame_count: 0,
			typing_field_id: None,
			selection_field_id: None,
		}
	}

	//==============================================================================
	// 흉내 낸 입력. (누름, 확정 글, 조합 글, 문맥 메뉴)
	//==============================================================================
	fn simulate_input(&mut self, engine: &mut Engine) {
		let frame_count = self.frame_count;
		let node_tree = self.scene_base.get_node_tree();
		let typing_point = self.typing_field_id.map(|field_id| calculate_node_point(node_tree, field_id, Vector2::create(FIELD_WIDTH - 40.0, FIELD_HEIGHT * 0.5)));
		let selection_point = self.selection_field_id.map(|field_id| calculate_node_point(node_tree, field_id, Vector2::create(160.0, FIELD_HEIGHT * 0.5)));

		let input_manager = engine.get_input_manager_mut();
		let input_position = if frame_count < 20 {
			typing_point
		}
		else if frame_count >= 140 {
			// 문맥 메뉴의 둘째 항목(복사) 위. (메뉴 왼쪽 위는 오른쪽 단추를 누른 자리)
			selection_point.map(|point| Vector2::create(point.x + 40.0, point.y + 4.0 + 34.0 * 1.5))
		}
		else {
			None
		};
		input_manager.set_view_input_position(input_position.unwrap_or(Vector2::create(-100.0, -100.0)));
		if frame_count == 5 {
			input_manager.set_touch_pressed(true);
			input_manager.set_touch_down(true);
		}
		if frame_count == 7 {
			input_manager.set_touch_released(true);
			input_manager.set_touch_down(false);
		}

		let text_input = input_manager.get_text_input_mut();
		match frame_count {
			12 => {
				text_input.push_focused_field_event(TextInputEventKind::Commit(String::from("안녕하세요 ")));
			}
			16 => {
				text_input.push_focused_field_event(TextInputEventKind::Commit(String::from("vanilla")));
			}
			30 => {
				text_input.push_focused_field_event(TextInputEventKind::CompositionStart);
				text_input.push_focused_field_event(TextInputEventKind::Preedit {
					text: String::from("ㅎ"),
					cursor_index: Some(1),
				});
			}
			33 => {
				text_input.push_focused_field_event(TextInputEventKind::Preedit {
					text: String::from("하"),
					cursor_index: Some(1),
				});
			}
			36 => {
				text_input.push_focused_field_event(TextInputEventKind::Preedit {
					text: String::from("한"),
					cursor_index: Some(1),
				});
			}
			130 => {
				text_input.set_context_menu_request_position(selection_point);
			}
			_ => {
			}
		}
	}
}

impl Scene for InputGalleryScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (필드와 리치텍스트 라벨을 놓는다)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = create_label_node(node_tree, root_id, "title", Vector2::create(48.0, 24.0), Vector2::create(900.0, 50.0), "바닐라엔진 입력 필드와 리치텍스트", 32.0, TITLE_COLOR);
		if let Some(title_label) = node_tree[title_id].get_component_mut::<UILabel>() {
			title_label.set_bold(true);
		}
		let left_panel_id = create_box_node(node_tree, root_id, "leftPanel", Vector2::create(32.0, 96.0), Vector2::create(584.0, 580.0));
		node_tree[left_panel_id].add_component(Paint::create_with_color(PANEL_COLOR)).set_round_size(16.0);
		let right_panel_id = create_box_node(node_tree, root_id, "rightPanel", Vector2::create(648.0, 96.0), Vector2::create(600.0, 580.0));
		node_tree[right_panel_id].add_component(Paint::create_with_color(PANEL_COLOR)).set_round_size(16.0);

		// 입력 필드.
		let typing_field_id = create_field_node(node_tree, left_panel_id, "typingField", Vector2::create(32.0, 60.0), "포커스 + 확정 글 + 조합 중인 글 (useDOMInput, maxLength 20)", true);
		if let Some(field) = node_tree[typing_field_id].get_component_mut::<UIInputField>() {
			field.set_placeholder("닉네임");
			field.set_max_length(20);
		}
		let placeholder_field_id = create_field_node(node_tree, left_panel_id, "placeholderField", Vector2::create(32.0, 196.0), "빈 칸 (안내 글)", true);
		if let Some(field) = node_tree[placeholder_field_id].get_component_mut::<UIInputField>() {
			field.set_placeholder("여기에 입력하세요");
		}
		let selection_field_id = create_field_node(node_tree, left_panel_id, "selectionField", Vector2::create(32.0, 332.0), "선택 영역 (오른쪽 단추 → 문맥 메뉴)", true);
		if let Some(field) = node_tree[selection_field_id].get_component_mut::<UIInputField>() {
			field.set_text("선택된 글자 영역입니다");
			field.move_cursor(3, false);
			field.move_cursor(8, true);
		}
		let canvas_field_id = create_field_node(node_tree, left_panel_id, "canvasField", Vector2::create(32.0, 468.0), "캔버스 모드 (useDOMInput 꺼짐, 색 바꿈)", false);
		if let Some(field) = node_tree[canvas_field_id].get_component_mut::<UIInputField>() {
			field.set_text("canvas mode ABC 123");
			field.set_background_color("#1f2937");
			field.set_text_color("#f9fafb");
			field.set_font_size(24.0);
			field.set_round_size(24.0);
			field.set_padding(24.0);
		}
		self.typing_field_id = Some(typing_field_id);
		self.selection_field_id = Some(selection_field_id);

		// 리치텍스트 라벨.
		create_label_node(node_tree, right_panel_id, "richCaption", Vector2::create(32.0, 24.0), Vector2::create(536.0, 24.0), "RichText (UILabel.useRichText)", 16.0, CAPTION_COLOR);
		create_rich_label_node(node_tree, right_panel_id, "richLeft", Vector2::create(32.0, 60.0), Vector2::create(536.0, 80.0), "<b>굵게</b> 보통 <color=#ef4444>빨강</color> <size=40>크게</size> 끝", TextAlign::Left);
		create_rich_label_node(node_tree, right_panel_id, "richCenter", Vector2::create(32.0, 160.0), Vector2::create(536.0, 80.0), "<u>밑줄</u> <s>취소선</s> <color=teal><i>기울임</i></color> <color=#2563eb80>반투명</color>", TextAlign::Center);
		create_rich_label_node(node_tree, right_panel_id, "richRight", Vector2::create(32.0, 260.0), Vector2::create(536.0, 80.0), "<unknown>모르는 태그</unknown> <size=18>작게</size> <b><color=orange>굵은 주황</color></b>", TextAlign::Right);
		create_rich_label_node(node_tree, right_panel_id, "richPlain", Vector2::create(32.0, 360.0), Vector2::create(536.0, 80.0), "마크업 없는 글은 Text 로 그린다", TextAlign::Center);
	}

	//==============================================================================
	// 주기적 갱신. (흉내 낸 입력 → 노드 갱신과 터치)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.frame_count += 1;
		if self.frame_count >= EXIT_FRAME_COUNT {
			engine.exit_application();
			return;
		}
		self.simulate_input(engine);
		self.scene_base.tick(engine, time_delta);
	}

	//==============================================================================
	// 이전 출력. (배경 칠하고 뷰 좌표계 걸기)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("입력 필드 갤러리 (바닐라엔진)");
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	// 화면 저장 때도 프레임 간격이 일정하도록 60 FPS 로 묶는다. (커서 깜빡임이 프레임 수와 맞게)
	engine_configuration.maximum_frame_per_second = 60.0;
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(InputGalleryScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
