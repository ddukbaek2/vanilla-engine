//==============================================================================
// 입력 필드 갤러리. (바닐라엔진 UIInputField, RichText 예제)
// - 입력 필드 넷: 포커스를 받아 한글을 치는 칸, 안내 글만 있는 칸, 선택 영역이 남은 칸, 캔버스 모드(use_dom_input 꺼짐) 칸.
// - 리치텍스트 라벨 셋: 굵게, 색, 크기, 밑줄, 취소선, 기울임, 모르는 태그. 정렬을 달리한다.
// - 사람이 없어도 확인할 수 있게, 정해 둔 프레임에 입력을 흉내 낸다. (엔진 글 입력 줄에 직접 넣는다)
//   프레임은 씬 tick 수다. 찍기 번호(VANILLA_CAPTURE_FRAMES, Engine::get_frame_number)는 엔진 프레임이라 씬 로딩 대기(0.3초, 60Hz 에서 17 프레임쯤)만큼 크다.
//   메뉴의 "복사" 올림은 찍기 번호 160 넘어서 보인다. (150 은 씬 133 이라 아직 올림 전)
//   5 프레임: 첫 칸을 누름 → 12, 16: 확정 글 "안녕하세요 ", "vanilla" → 30 ~ 36: 조합 "ㅎ" → "하" → "한" (밑줄과 커서)
//   130: 셋째 칸에 오른쪽 단추(문맥 메뉴) → 140 부터: 메뉴의 "복사" 위에 마우스.
// - 오른쪽 아래: 입력 동작(InputActions), 손가락 목록, 마지막으로 쓴 장치. 정해 둔 프레임에 키와 손가락, 바깥 입력원을 흉내 낸다.
//   40 ~ 120: ArrowRight 누름(반복), 50 ~ 110: KeyW 누름(이동 축 대각선), 60: 두 손가락, 70: 둘째 손가락 움직임, 85: 첫째 손가락 뗌,
//   80 ~ 95: 바깥 입력원이 menu 를 누름(플레이스테이션 패드로 적힘).
// - 패드 진동 시험: 실제 게임패드의 A(크로스)는 무거운 모터 0.5초, B(서클)는 가벼운 모터 0.5초, X(스퀘어)는 둘 다 1초, Y(트라이앵글)는 멈춤.
// - 200 프레임이 지나면 스스로 끝난다. 실제 게임패드가 붙어 있으면 사람이 시험하는 것으로 보고 창을 닫을 때까지 돈다.
//   (화면 저장: VANILLA_CAPTURE_PATH / VANILLA_CAPTURE_FRAMES / VANILLA_CAPTURE_EXIT)
//==============================================================================
#![allow(clippy::too_many_arguments)]

use std::cell::Cell;
use std::rc::Rc;
use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{Engine, EngineConfiguration, GamepadButtonCode, GamepadKind, Graphic, InputActionSource, NodeId, NodeTree, Paint, Pivot, Scene, SceneBase, TextInputEventKind, UIInputField, UILabel, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;
const BACKGROUND_COLOR: &str = "#e9edf3";
const PANEL_COLOR: &str = "#ffffff";
const TITLE_COLOR: &str = "#1f2937";
const CAPTION_COLOR: &str = "#6b7280";
const ACCENT_COLOR: &str = "#2563eb";
const FIELD_WIDTH: f64 = 520.0;
const FIELD_HEIGHT: f64 = 64.0;
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
fn create_label_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, font_size: f64, color: &str) -> NodeId {
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
fn create_field_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, caption: &str, use_dom_input: bool) -> NodeId {
	create_label_node(node_tree, parent_id, "caption", Vector2::create(position.x, position.y - 28.0), Vector2::create(FIELD_WIDTH, 24.0), caption, 16.0, CAPTION_COLOR);
	let field_id = create_box_node(node_tree, parent_id, name, position, Vector2::create(FIELD_WIDTH, FIELD_HEIGHT));
	let field = node_tree.add_component(field_id, UIInputField::new()).expect("field");
	field.set_use_dom_input(use_dom_input);
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
// 흉내 낸 바깥 입력원. (플랫폼 SDK 의 패드 자리, 정한 프레임 동안 menu 를 누른다)
//==============================================================================
struct GallerySource {
	frame_count: Rc<Cell<u32>>,
}

impl InputActionSource for GallerySource {
	fn read_action_value(&mut self, action: &str) -> f64 {
		let frame_count = self.frame_count.get();
		if action == "menu" && (80..95).contains(&frame_count) {
			1.0
		}
		else {
			0.0
		}
	}

	fn read_gamepad_kind(&mut self) -> Option<GamepadKind> {
		Some(GamepadKind::PlayStation)
	}
}

//==============================================================================
// 입력 필드 갤러리 씬.
//==============================================================================
struct InputGalleryScene {
	scene_base: SceneBase,
	frame_count: u32,
	typing_field_id: Option<NodeId>,
	selection_field_id: Option<NodeId>,
	shared_frame_count: Rc<Cell<u32>>,
	action_label_id: Option<NodeId>,
	touch_label_id: Option<NodeId>,
	right_just_pressed_count: u32,
	ended_touch_count: u32,
	gamepad_label_id: Option<NodeId>,
	vibration_text: &'static str,
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
			shared_frame_count: Rc::new(Cell::new(0)),
			action_label_id: None,
			touch_label_id: None,
			right_just_pressed_count: 0,
			ended_touch_count: 0,
			gamepad_label_id: None,
			vibration_text: "없음",
		}
	}

	//==============================================================================
	// 흉내 낸 키와 손가락. (씬 tick 에서 넣으므로 입력 동작은 다음 프레임에 본다, 손가락의 캔버스 자리는 보이지 않아 0 으로 넣는다)
	//==============================================================================
	fn simulate_action_input(&mut self, engine: &mut Engine) {
		let frame_count = self.frame_count;
		let first_finger_view = Vector2::create(900.0, 300.0);
		let second_finger_view = Vector2::create(1000.0, 330.0);
		let moved_finger_view = Vector2::create(1040.0, 360.0);
		let first_finger_canvas = Vector2::zero();
		let second_finger_canvas = Vector2::zero();
		let moved_finger_canvas = Vector2::zero();
		let input_manager = engine.get_input_manager_mut();
		match frame_count {
			40 => {
				input_manager.push_key("ArrowRight");
			}
			50 => {
				input_manager.push_key("KeyW");
			}
			60 => {
				input_manager.begin_touch_point(1, first_finger_canvas, first_finger_view);
				input_manager.begin_touch_point(2, second_finger_canvas, second_finger_view);
			}
			70 => {
				input_manager.move_touch_point(2, moved_finger_canvas, moved_finger_view);
			}
			85 => {
				input_manager.end_touch_point(1, first_finger_canvas, first_finger_view, false);
			}
			110 => {
				input_manager.pop_key("KeyW");
			}
			120 => {
				input_manager.pop_key("ArrowRight");
			}
			_ => {
			}
		}
	}

	//==============================================================================
	// 입력 동작, 손가락, 마지막 장치 글.
	//==============================================================================
	fn update_action_status(&mut self, engine: &Engine) {
		let input_manager = engine.get_input_manager();
		let input_actions = input_manager.get_input_actions();
		if input_actions.is_just_pressed("right") {
			self.right_just_pressed_count += 1;
		}
		self.ended_touch_count += input_manager.get_ended_touch_points().len() as u32;
		let mark = |is_pressed: bool| if is_pressed { "●" } else { "○" };
		let move_axis = input_actions.read_move_axis();
		let action_text = format!("동작 confirm {}  right {} (누름과 반복 {}번)  menu {}  이동 ({:.2}, {:.2})", mark(input_actions.is_pressed("confirm")), mark(input_actions.is_pressed("right")), self.right_just_pressed_count, mark(input_actions.is_pressed("menu")), move_axis.x, move_axis.y);
		let touch_text_list: Vec<String> = input_manager.get_touch_points().iter().map(|touch_point| format!("#{} {:?} ({:.0}, {:.0})", touch_point.identifier, touch_point.phase, touch_point.view_position.x, touch_point.view_position.y)).collect();
		let touch_text = format!("손가락 {}개 {}  뗀 것 {}  장치 {:?} ({:?})  입력칸 {}", touch_text_list.len(), touch_text_list.join(" "), self.ended_touch_count, input_manager.get_last_input_device(), input_manager.get_last_gamepad_kind(), input_manager.is_typing_in_text_field());
		let node_tree = self.scene_base.get_node_tree_mut();
		for (label_id, text) in [(self.action_label_id, action_text), (self.touch_label_id, touch_text)] {
			if let Some(label) = label_id.and_then(|label_id| node_tree[label_id].get_component_mut::<UILabel>()) {
				label.set_text(&text);
			}
		}
	}

	//==============================================================================
	// 패드 진동 시험. (실제 패드의 앞면 단추를 누르면 연결된 패드를 모두 떤다)
	//==============================================================================
	fn update_gamepad_vibration(&mut self, engine: &mut Engine) {
		let gamepad_manager = engine.get_input_manager_mut().get_gamepad_manager_mut();
		let gamepad_index_list: Vec<usize> = gamepad_manager.get_all_connected_gamepads().iter().map(|gamepad| gamepad.index).collect();
		let vibration_list = [
			(GamepadButtonCode::A_CROSS, 1.0, 0.0, 0.5, "무거운 모터 0.5초"),
			(GamepadButtonCode::B_CIRCLE, 0.0, 1.0, 0.5, "가벼운 모터 0.5초"),
			(GamepadButtonCode::X_SQUARE, 1.0, 1.0, 1.0, "둘 다 1초"),
			(GamepadButtonCode::Y_TRIANGLE, 0.0, 0.0, 0.0, "멈춤"),
		];
		for gamepad_index in gamepad_index_list.iter() {
			for (button_index, strong_magnitude, weak_magnitude, seconds, vibration_text) in vibration_list {
				if gamepad_manager.is_button_just_pressed(*gamepad_index, button_index) {
					gamepad_manager.vibrate(strong_magnitude, weak_magnitude, seconds);
					self.vibration_text = vibration_text;
					println!("[inputgallery] 패드 {} 진동 요청: {}", gamepad_index, vibration_text);
				}
			}
		}
		let gamepad_text = format!("패드 {}개  진동: A 무거운 모터, B 가벼운 모터, X 둘 다, Y 멈춤  (마지막: {})", gamepad_index_list.len(), self.vibration_text);
		let node_tree = self.scene_base.get_node_tree_mut();
		if let Some(label) = self.gamepad_label_id.and_then(|label_id| node_tree[label_id].get_component_mut::<UILabel>()) {
			label.set_text(&gamepad_text);
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
	fn create(&mut self, engine: &mut Engine) {
		// 입력 동작. (키를 묶고 반복을 걸고, 흉내 낸 바깥 입력원을 끼운다)
		let input_manager = engine.get_input_manager_mut();
		input_manager.set_prevented_keys(&["Tab", "Space"]);
		let input_actions = input_manager.get_input_actions_mut();
		input_actions.bind_key("confirm", "Space");
		input_actions.bind_key("right", "ArrowRight");
		input_actions.set_repeat("right", 0.25, 0.1);
		input_actions.bind_key("menu", "Escape");
		input_actions.add_source(Box::new(GallerySource { frame_count: self.shared_frame_count.clone() }));
		// 첫 입력 필드에 포커스가 있어 글자 입력 중에도 키를 듣게 한다. (기본은 듣지 않는다)
		input_actions.set_keys_ignored_while_typing(false);

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

		// 입력 동작, 손가락, 마지막 장치.
		create_label_node(node_tree, right_panel_id, "actionCaption", Vector2::create(32.0, 456.0), Vector2::create(536.0, 24.0), "InputActions, 손가락, 마지막 장치 (흉내 낸 입력)", 16.0, CAPTION_COLOR);
		let action_label_id = create_label_node(node_tree, right_panel_id, "actionStatus", Vector2::create(32.0, 486.0), Vector2::create(536.0, 30.0), "", 16.0, TITLE_COLOR);
		let touch_label_id = create_label_node(node_tree, right_panel_id, "touchStatus", Vector2::create(32.0, 522.0), Vector2::create(536.0, 30.0), "", 16.0, TITLE_COLOR);
		let gamepad_label_id = create_label_node(node_tree, right_panel_id, "gamepadStatus", Vector2::create(32.0, 552.0), Vector2::create(536.0, 22.0), "", 14.0, CAPTION_COLOR);
		self.action_label_id = Some(action_label_id);
		self.touch_label_id = Some(touch_label_id);
		self.gamepad_label_id = Some(gamepad_label_id);
	}

	//==============================================================================
	// 주기적 갱신. (흉내 낸 입력 → 패드 진동 시험 → 노드 갱신과 터치)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.frame_count += 1;
		let connected_gamepad_count = engine.get_input_manager().get_gamepad_manager().get_connected_gamepad_count();
		if self.frame_count >= EXIT_FRAME_COUNT && connected_gamepad_count == 0 {
			engine.exit_application();
			return;
		}
		self.shared_frame_count.set(self.frame_count);
		self.simulate_input(engine);
		self.simulate_action_input(engine);
		self.update_action_status(engine);
		self.update_gamepad_vibration(engine);
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
	engine_configuration.auto_resize_on_window_resize = true;
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
