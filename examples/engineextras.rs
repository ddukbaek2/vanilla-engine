//==============================================================================
// 엔진에 더한 범용 기능 예제. (외계행성 광부 키우기에서 엔진으로 옮긴 것)
// - 테(Outline), 진행 고리(UIProgressRing), 부드러운 그늘(SoftShade), 마우스 커서(PointerCursor), 카메라 손짓(CameraGestures),
//   입력 매니저의 마우스 단추와 손가락 목록을 한 화면에 놓는다. 입력은 흉내 내지 않는다. (데스크탑과 웹에서 직접 눌러 본다)
// - 오른쪽 위: 글자 꾸밈. (밑줄, 취소선, 기울임, 그림자 글의 기울임과 밑줄, 글자 단위 줄바꿈, 안티앨리어싱을 끈 도트 글자)
// - 오른쪽 단추로 끌면 가운데 사각형이 돌고, 가운데 단추로 끌면 움직이고, 휠이나 두 손가락으로 키운다.
// - 실행: cargo run --example engineextras
//==============================================================================
use std::cell::Cell;
use std::rc::Rc;
use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{
	Image, ShadowImage, ShadowText,
	set_button_motion_area, ButtonMotion, ButtonMotionStyle, ToastMotion,
	CameraGestures, Color, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Graphic, MouseButton, NodeId, NodeTransform, NodeTree, Outline, Paint, Pivot, Scene, SceneBase, SoftShade, SoftShadeShape, Text, TextWrapMode, TouchRecognizer, UIButton, UIHitArea, UILabel, UIProgressRing, Vector2, ViewScaleMode,
};

// 뷰 크기.
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;

// 색.
const BACKGROUND_COLOR: &str = "#1b1f2a";
const PANEL_COLOR: &str = "#2a3040";
const TEXT_COLOR: &str = "#e8ebf2";
const CAPTION_COLOR: &str = "#9aa2b2";
const ACCENT_COLOR: &str = "#6d7dff";

//==============================================================================
// 상자 노드 만들기. (왼쪽 위 피봇)
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
// 안티앨리어싱을 끄고 굽는 글. (도트 글꼴 흉내. 보통은 게임 시작 때 한 번 끄고, 여기서는 이 글을 그리는 동안만 끄고 되돌린다)
//==============================================================================
struct AliasedText {
	text: Text,
}

impl Component for AliasedText {
	fn get_component_base(&self) -> &ComponentBase {
		self.text.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.text.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"AliasedText"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		graphic.get_text_string_texture_cache().set_text_antialias_enabled(false);
		self.text.draw(transform, graphic);
		graphic.get_text_string_texture_cache().set_text_antialias_enabled(true);
	}
}

//==============================================================================
// 예제 씬.
//==============================================================================
struct EngineExtrasScene {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	camera_gestures: CameraGestures,
	elapsed_seconds: f32,
	click_count: Rc<Cell<u32>>,
	ring_ids: Vec<NodeId>,
	target_id: Option<NodeId>,
	status_id: Option<NodeId>,
	target_rotation: f32,
	target_scale: f32,
	target_position: Vector2,
}

impl EngineExtrasScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> EngineExtrasScene {
		EngineExtrasScene {
			scene_base: SceneBase::new(),
			touch_recognizer: TouchRecognizer::new(),
			camera_gestures: CameraGestures::new(),
			elapsed_seconds: 0.0,
			click_count: Rc::new(Cell::new(0)),
			ring_ids: Vec::new(),
			target_id: None,
			status_id: None,
			target_rotation: 0.0,
			target_scale: 1.0,
			target_position: Vector2::create(980.0, 420.0),
		}
	}

	//==============================================================================
	// 테와 고리 구역.
	//==============================================================================
	fn create_outline_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let card_id = create_box_node(node_tree, root_id, "card", Vector2::create(40.0, 100.0), Vector2::create(260.0, 150.0));
		let card_paint = node_tree[card_id].add_component(Paint::create_with_color(PANEL_COLOR));
		card_paint.set_round_size(22.0);
		let card_outline = node_tree[card_id].add_component(Outline::new());
		card_outline.set_color("#ffcc55");
		card_outline.set_line_width(4.0);
		card_outline.set_round_size(22.0);
		create_label_node(node_tree, card_id, "cardCaption", Vector2::create(20.0, 50.0), Vector2::create(220.0, 50.0), "Outline (둥근 네모)", 20.0, TEXT_COLOR);

		let circle_id = create_box_node(node_tree, root_id, "circle", Vector2::create(330.0, 100.0), Vector2::create(150.0, 150.0));
		let circle_outline = node_tree[circle_id].add_component(Outline::new());
		circle_outline.set_color("#78c8ff");
		circle_outline.set_line_width(6.0);
		circle_outline.set_circle(true);

		for ring_index in 0..2 {
			let ring_position = if ring_index == 0 { Vector2::create(510.0, 100.0) } else { Vector2::create(690.0, 120.0) };
			let ring_size = if ring_index == 0 { Vector2::create(150.0, 150.0) } else { Vector2::create(240.0, 110.0) };
			let ring_id = create_box_node(node_tree, root_id, "ring", ring_position, ring_size);
			let ring = node_tree[ring_id].add_component(UIProgressRing::new());
			ring.set_round_size(if ring_index == 0 { 75.0 } else { 24.0 });
			ring.set_line_width(if ring_index == 0 { 10.0 } else { 6.0 });
			ring.set_fill_color(if ring_index == 0 { "#45b585" } else { "#ff7755" });
			self.ring_ids.push(ring_id);
		}
	}

	//==============================================================================
	// 그늘 구역. (밝은 판 위에 띠 그늘을 깔고 흰 글을 얹는다, 아래에는 바닥 그늘)
	//==============================================================================
	fn create_shade_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let bright_id = create_box_node(node_tree, root_id, "bright", Vector2::create(40.0, 290.0), Vector2::create(560.0, 150.0));
		node_tree[bright_id].add_component(Paint::create_with_color("#c9d1dd"));
		let band_id = create_box_node(node_tree, bright_id, "band", Vector2::create(0.0, 35.0), Vector2::create(560.0, 80.0));
		let band = node_tree[band_id].add_component(SoftShade::new());
		band.set_shade(SoftShadeShape::Band, "#000000", 0.6, 120.0);
		create_label_node(node_tree, bright_id, "bandText", Vector2::create(40.0, 50.0), Vector2::create(500.0, 50.0), "SoftShade 띠 위의 흰 글", 30.0, "#ffffff");
		// 그림자를 깐 글과 그림. (밝은 판 아래쪽)
		let shadow_text_id = create_box_node(node_tree, bright_id, "shadowText", Vector2::create(20.0, 110.0), Vector2::create(300.0, 36.0));
		let shadow_text = node_tree[shadow_text_id].add_component(ShadowText::create_with_text("ShadowText 그림자 글", 26.0));
		shadow_text.set_text_style(26.0, true, "#ffffff");
		shadow_text.set_shadow(vanilla::Color::create(0.0, 0.0, 0.0, 0.8), 3.0, 4.0);
		let mut icon_pixels = vec![0u8; 64 * 64 * 4];
		for y in 0..64usize {
			for x in 0..64usize {
				let is_inside = (x as i32 - 32).abs() * 2 < y as i32 && y > 4 && y < 60;
				if is_inside {
					let offset = (y * 64 + x) * 4;
					icon_pixels[offset..offset + 4].copy_from_slice(&[255, 85, 119, 255]);
				}
			}
		}
		let icon_image = Image::create_from_pixels(64, 64, icon_pixels);
		let shadow_image_id = create_box_node(node_tree, bright_id, "shadowImage", Vector2::create(440.0, 20.0), Vector2::create(100.0, 100.0));
		let shadow_image = node_tree[shadow_image_id].add_component(ShadowImage::new());
		shadow_image.set_image(Some(icon_image), None);
		shadow_image.set_shadow(vanilla::Color::create(0.0, 0.0, 0.0, 0.7), 6.0, 8.0);

		let bottom_id = create_box_node(node_tree, root_id, "bottomShade", Vector2::create(0.0, 620.0), Vector2::create(VIEW_WIDTH, 100.0));
		let bottom = node_tree[bottom_id].add_component(SoftShade::new());
		bottom.set_shade(SoftShadeShape::Bottom, "#4466ff", 0.8, 0.0);
	}

	//==============================================================================
	// 글자 꾸밈 구역. (오른쪽 위)
	//==============================================================================
	fn create_text_decoration_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		create_label_node(node_tree, root_id, "decorationCaption", Vector2::create(960.0, 90.0), Vector2::create(280.0, 22.0), "글자 꾸밈", 15.0, CAPTION_COLOR);
		let underline_id = create_label_node(node_tree, root_id, "underline", Vector2::create(960.0, 114.0), Vector2::create(130.0, 30.0), "밑줄 글", 20.0, TEXT_COLOR);
		if let Some(label) = node_tree[underline_id].get_component_mut::<UILabel>() {
			label.set_underline(true);
		}
		let strikethrough_id = create_label_node(node_tree, root_id, "strikethrough", Vector2::create(1100.0, 114.0), Vector2::create(140.0, 30.0), "취소선 글", 20.0, TEXT_COLOR);
		if let Some(label) = node_tree[strikethrough_id].get_component_mut::<UILabel>() {
			label.set_strikethrough(true);
		}
		let italic_id = create_label_node(node_tree, root_id, "italic", Vector2::create(960.0, 146.0), Vector2::create(130.0, 30.0), "기울임 Italic", 20.0, TEXT_COLOR);
		if let Some(label) = node_tree[italic_id].get_component_mut::<UILabel>() {
			label.set_italic(true);
		}
		let aliased_id = create_box_node(node_tree, root_id, "aliasedText", Vector2::create(1100.0, 146.0), Vector2::create(140.0, 30.0));
		let mut aliased_text = Text::create_with_text("도트 Aliased", 20.0);
		aliased_text.set_text_color("#9fe870");
		aliased_text.set_text_align(TextAlign::Left);
		node_tree[aliased_id].add_component(AliasedText {
			text: aliased_text,
		});
		let shadow_text_id = create_box_node(node_tree, root_id, "decoratedShadowText", Vector2::create(960.0, 178.0), Vector2::create(280.0, 32.0));
		let shadow_text = node_tree[shadow_text_id].add_component(ShadowText::create_with_text("그림자 글 기울임 밑줄", 22.0));
		shadow_text.set_text_style(22.0, true, "#ffcc55");
		shadow_text.set_text_align(TextAlign::Left);
		shadow_text.set_italic(true);
		shadow_text.set_underline(true);
		shadow_text.set_shadow(vanilla::Color::create(0.0, 0.0, 0.0, 0.9), 2.0, 2.0);
		let wrap_id = create_label_node(node_tree, root_id, "characterWrap", Vector2::create(960.0, 214.0), Vector2::create(280.0, 36.0), "Character 줄바꿈은 낱말도 Characters 글자 단위로 끊는다", 14.0, TEXT_COLOR);
		if let Some(label) = node_tree[wrap_id].get_component_mut::<UILabel>() {
			label.set_text_baseline(TextBaseline::Top);
			label.set_word_wrap_width(280.0);
			label.set_wrap_mode(TextWrapMode::Character);
		}
	}

	//==============================================================================
	// 단추와 손짓 구역.
	//==============================================================================
	fn create_input_section(&mut self, root_id: NodeId) {
		let click_count = self.click_count.clone();
		let node_tree = self.scene_base.get_node_tree_mut();
		let button_id = create_box_node(node_tree, root_id, "button", Vector2::create(40.0, 480.0), Vector2::create(220.0, 56.0));
		let button_paint = node_tree[button_id].add_component(Paint::create_with_color(ACCENT_COLOR));
		button_paint.set_round_size(28.0);
		let button_label = node_tree.add_component(button_id, UILabel::create_with_text("눌러 보세요", 22.0)).expect("label");
		button_label.set_text_color("#ffffff");
		button_label.set_bold(true);
		let counter_id = create_label_node(node_tree, root_id, "counter", Vector2::create(280.0, 480.0), Vector2::create(300.0, 56.0), "클릭 0번", 22.0, TEXT_COLOR);
		// 알림 노드. (위에서 내려오는 토스트)
		let toast_id = create_box_node(node_tree, root_id, "toast", Vector2::create(900.0, 30.0), Vector2::create(320.0, 50.0));
		let toast_paint = node_tree[toast_id].add_component(Paint::create_with_color("#45b585"));
		toast_paint.set_round_size(25.0);
		let toast_label = node_tree.add_component(toast_id, UILabel::create_with_text("", 20.0)).expect("label");
		toast_label.set_text_color("#ffffff");
		node_tree[toast_id].set_visible(false);
		let toast_motion = node_tree.add_component(toast_id, ToastMotion::new(-1.0)).expect("toast");
		toast_motion.set_apply_text_handler(Box::new(|context: &mut ComponentContext, text: &str| {
			if let Some(label) = context.get_node_mut().get_component_mut::<UILabel>() {
				label.set_text(text);
			}
		}));

		let button = node_tree.add_component(button_id, UIButton::new()).expect("button");
		button.set_click_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
			let next_click_count = click_count.get() + 1;
			click_count.set(next_click_count);
			let counter_label = context.get_node_tree_mut().get_node_mut(counter_id).and_then(|counter_node| counter_node.get_component_mut::<UILabel>());
			if let Some(counter_label) = counter_label {
				counter_label.set_text(&format!("클릭 {}번", next_click_count));
			}
			if let Some(toast_motion) = context.get_node_tree_mut().get_node_mut(toast_id).and_then(|toast_node| toast_node.get_component_mut::<ToastMotion>()) {
				toast_motion.show(&format!("{}번 눌렀어요", next_click_count), 1.5);
			}
		}));
		node_tree.add_component(button_id, ButtonMotion::new());

		// 창 안 꼴(Inset) 단추. (판에 꼴을 걸고 그 안의 단추는 커지지 않고 밝힌다)
		let inset_panel_id = create_box_node(node_tree, root_id, "insetPanel", Vector2::create(330.0, 640.0), Vector2::create(330.0, 70.0));
		let inset_panel_paint = node_tree[inset_panel_id].add_component(Paint::create_with_color(PANEL_COLOR));
		inset_panel_paint.set_round_size(14.0);
		set_button_motion_area(node_tree, inset_panel_id, ButtonMotionStyle::Inset);
		let inset_button_id = create_box_node(node_tree, inset_panel_id, "insetButton", Vector2::create(12.0, 10.0), Vector2::create(306.0, 50.0));
		let inset_paint = node_tree[inset_button_id].add_component(Paint::create_with_color("#3c4457"));
		inset_paint.set_round_size(10.0);
		let inset_label = node_tree.add_component(inset_button_id, UILabel::create_with_text("창 안 단추 (Inset)", 20.0)).expect("label");
		inset_label.set_text_color(TEXT_COLOR);
		node_tree.add_component(inset_button_id, UIButton::new());
		node_tree.add_component(inset_button_id, ButtonMotion::new());

		let area_id = create_box_node(node_tree, root_id, "gestureArea", Vector2::create(700.0, 260.0), Vector2::create(540.0, 330.0));
		let area_outline = node_tree[area_id].add_component(Outline::new());
		area_outline.set_color("#394154");
		area_outline.set_line_width(2.0);
		area_outline.set_round_size(16.0);
		create_label_node(node_tree, area_id, "areaCaption", Vector2::create(16.0, 6.0), Vector2::create(520.0, 30.0), "CameraGestures: 오른쪽 끌기 돌리기, 가운데 끌기 밀기, 휠 키우기", 16.0, CAPTION_COLOR);

		let target_id = node_tree.create_child(root_id, "target");
		let target_node = &mut node_tree[target_id];
		target_node.set_pivot(Pivot::MIDDLE_CENTER);
		target_node.set_content_size(Vector2::create(120.0, 120.0));
		let target_paint = target_node.add_component(Paint::create_with_color("#f3e5ab"));
		target_paint.set_round_size(16.0);
		self.target_id = Some(target_id);

		// 작은 단추와 넓힌 누름 자리. (누름 자리를 옅게 칠해 보인다)
		let small_click_count = Rc::new(Cell::new(0u32));
		let small_button_id = create_box_node(node_tree, root_id, "smallButton", Vector2::create(520.0, 490.0), Vector2::create(40.0, 30.0));
		let small_paint = node_tree[small_button_id].add_component(Paint::create_with_color("#e06058"));
		small_paint.set_round_size(6.0);
		let small_counter_id = create_label_node(node_tree, root_id, "smallCounter", Vector2::create(600.0, 490.0), Vector2::create(90.0, 30.0), "0", 20.0, TEXT_COLOR);
		let small_button = node_tree.add_component(small_button_id, UIButton::new()).expect("button");
		small_button.set_click_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
			let next_click_count = small_click_count.get() + 1;
			small_click_count.set(next_click_count);
			if let Some(counter_label) = context.get_node_tree_mut().get_node_mut(small_counter_id).and_then(|counter_node| counter_node.get_component_mut::<UILabel>()) {
				counter_label.set_text(&format!("{}", next_click_count));
			}
		}));
		if let Some(hit_id) = UIHitArea::attach(node_tree, small_button_id, 100.0) {
			node_tree[hit_id].add_component(Paint::create_with_color(Color::create(1.0, 1.0, 1.0, 0.12)));
		}

		let status_id = create_label_node(node_tree, root_id, "status", Vector2::create(40.0, 560.0), Vector2::create(620.0, 40.0), "", 18.0, CAPTION_COLOR);
		self.status_id = Some(status_id);
	}

	//==============================================================================
	// 카메라 손짓을 가운데 사각형에 걸기.
	//==============================================================================
	fn apply_gestures(&mut self, engine: &mut Engine) {
		self.camera_gestures.update(engine.get_input_manager(), engine.get_view_manager());
		self.target_rotation += self.camera_gestures.take_rotation().to_degrees();
		let pan = self.camera_gestures.take_pan();
		self.target_position = self.target_position.add(pan);
		self.target_scale = (self.target_scale * self.camera_gestures.take_zoom()).clamp(0.3, 3.0);
		let input_manager = engine.get_input_manager();
		if input_manager.has_wheel_delta() {
			let wheel_delta_y = input_manager.get_wheel_delta_y();
			self.target_scale = (self.target_scale * (1.0 - wheel_delta_y * 0.001)).clamp(0.3, 3.0);
		}
		let drag_cursor = self.camera_gestures.read_mouse_drag_cursor();
		let screen_cursor = if drag_cursor.is_empty() { "default" } else { drag_cursor };
		engine.get_pointer_cursor_mut().set_screen_cursor(screen_cursor);
		if let Some(target_id) = self.target_id {
			let target_node = &mut self.scene_base.get_node_tree_mut()[target_id];
			target_node.set_local_position(self.target_position);
			target_node.set_local_rotation(self.target_rotation);
			target_node.set_local_scale(Vector2::create(self.target_scale, self.target_scale));
		}
	}

	//==============================================================================
	// 입력 상태 글.
	//==============================================================================
	fn update_status(&mut self, engine: &Engine) {
		let input_manager = engine.get_input_manager();
		let button_marks = [MouseButton::Left, MouseButton::Middle, MouseButton::Right].map(|mouse_button| if input_manager.is_mouse_button_down(mouse_button) { "●" } else { "○" });
		let status_text = format!("마우스 왼 {} 가운데 {} 오른 {}  손가락 {}개  장치 {:?} ({:?})  손짓 중 {}", button_marks[0], button_marks[1], button_marks[2], input_manager.get_touch_points().len(), input_manager.get_last_input_device(), input_manager.get_last_gamepad_kind(), self.camera_gestures.is_gesture_active());
		if let Some(status_id) = self.status_id {
			if let Some(status_label) = self.scene_base.get_node_tree_mut()[status_id].get_component_mut::<UILabel>() {
				status_label.set_text(&status_text);
			}
		}
	}
}

impl Scene for EngineExtrasScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성.
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		create_label_node(node_tree, root_id, "title", Vector2::create(40.0, 30.0), Vector2::create(900.0, 50.0), "엔진에 더한 범용 기능", 34.0, TEXT_COLOR);
		self.create_outline_section(root_id);
		self.create_shade_section(root_id);
		self.create_text_decoration_section(root_id);
		self.create_input_section(root_id);
		engine.get_pointer_cursor_mut().set_enabled(true);
	}

	//==============================================================================
	// 주기적 갱신.
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.elapsed_seconds += time_delta;
		let ring_ratio = (self.elapsed_seconds * 0.25).fract();
		for (ring_index, ring_id) in self.ring_ids.clone().into_iter().enumerate() {
			if let Some(ring) = self.scene_base.get_node_tree_mut()[ring_id].get_component_mut::<UIProgressRing>() {
				ring.set_ratio(if ring_index == 0 { ring_ratio } else { 1.0 - ring_ratio });
			}
		}
		self.apply_gestures(engine);
		self.update_status(engine);
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
	}

	//==============================================================================
	// 이전 출력.
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
	engine_configuration.title = String::from("엔진에 더한 범용 기능 (바닐라엔진)");
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(EngineExtrasScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
