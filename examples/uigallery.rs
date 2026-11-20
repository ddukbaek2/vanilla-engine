//==============================================================================
// UI 위젯 갤러리. (바닐라엔진 ui 모듈 예제)
// - 옮긴 위젯을 한 화면에 모두 놓는다: UIView, UILabel, UIButton, UIToggleButton, UISlider, UIProgressView, UIImageView, UIScrollView + UIScrollBar.
// - 터치는 원본 GameScene 처럼 TouchRecognizer 로 받는다. (스크롤 뷰 안의 버튼을 끌어도 스크롤된다)
// - 사람이 없어도 확인할 수 있게, 정해 둔 프레임에 입력을 흉내 낸다:
//   휠로 스크롤 → 버튼 클릭 → 슬라이더 누름 → 토글 클릭 → 스크롤 뷰 안 버튼을 위로 끌기 → 버튼을 누른 채 두기.
// - 240 프레임이 지나면 스스로 끝난다. (화면 저장: VANILLA_CAPTURE_PATH / VANILLA_CAPTURE_FRAMES / VANILLA_CAPTURE_EXIT)
//==============================================================================
#![allow(clippy::too_many_arguments)]

use std::cell::Cell;
use std::rc::Rc;
use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{
	Color, ComponentContext, Engine, EngineConfiguration, Graphic, Image, NodeId, NodeTree, Paint, Pivot, ProgressDirection, Scene, SceneBase, Sprite, TouchRecognizer, UIButton, UIImageView, UILabel, UIProgressView, UIScrollView, UISlider, UIToggleButton, UIView, Vector2, ViewScaleMode,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#1d1814";
const PANEL_COLOR: &str = "#2c241e";
const ACCENT_COLOR: &str = "#f3e5ab";
const DARK_TEXT_COLOR: &str = "#2b2118";
const LIGHT_TEXT_COLOR: &str = "#f1e9d8";
const CAPTION_TEXT_COLOR: &str = "#b8a690";
const SCROLL_ROW_COUNT: usize = 14;
const SCROLL_ROW_HEIGHT: f32 = 52.0;
const SCROLL_ROW_GAP: f32 = 8.0;
const EXIT_FRAME_COUNT: u32 = 240;

//==============================================================================
// 원 그림 만들기. (가운데가 밝은 둥근 무늬. 코드로 픽셀을 찍는다)
//==============================================================================
fn create_disc_image(image_size: u32, inner_color: Color, outer_color: Color) -> Rc<Image> {
	let mut pixels = vec![0u8; (image_size * image_size * 4) as usize];
	let half_size = image_size as f32 * 0.5;
	for pixel_y in 0..image_size {
		for pixel_x in 0..image_size {
			let delta_x = (pixel_x as f32 + 0.5 - half_size) / half_size;
			let delta_y = (pixel_y as f32 + 0.5 - half_size) / half_size;
			let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
			let coverage = ((1.0 - distance) * half_size).clamp(0.0, 1.0);
			if coverage <= 0.0 {
				continue;
			}
			let stripe = if ((delta_x + delta_y) * 4.0).floor() as i32 % 2 == 0 {
				1.0
			}
			else {
				0.85
			};
			let pixel_color = Color::lerp(inner_color, outer_color, distance);
			let pixel_index = ((pixel_y * image_size + pixel_x) * 4) as usize;
			pixels[pixel_index] = (pixel_color.red * stripe * 255.0) as u8;
			pixels[pixel_index + 1] = (pixel_color.green * stripe * 255.0) as u8;
			pixels[pixel_index + 2] = (pixel_color.blue * stripe * 255.0) as u8;
			pixels[pixel_index + 3] = (coverage * 255.0) as u8;
		}
	}
	Image::create_from_pixels(image_size, image_size, pixels)
}

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
fn create_label_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, font_size: f32, color: &str, text_align: TextAlign) -> NodeId {
	let label_id = create_box_node(node_tree, parent_id, name, position, size);
	let label = node_tree.add_component(label_id, UILabel::create_with_text(text, font_size)).expect("label");
	label.set_text_color(color);
	label.set_text_align(text_align);
	label.set_text_baseline(TextBaseline::Middle);
	label_id
}

//==============================================================================
// 글자 버튼 만들기. (둥근 Paint 바탕 + UIButton + 같은 노드의 UILabel)
//==============================================================================
fn create_text_button_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, text: &str, fill_color: &str) -> NodeId {
	let button_id = create_box_node(node_tree, parent_id, name, position, size);
	let button_paint = node_tree[button_id].add_component(Paint::create_with_color(fill_color));
	button_paint.set_round_size(size.y * 0.5);
	let label = node_tree.add_component(button_id, UILabel::create_with_text(text, 22.0)).expect("label");
	label.set_text_color(DARK_TEXT_COLOR);
	label.set_bold(true);
	node_tree.add_component(button_id, UIButton::new());
	button_id
}

//==============================================================================
// 노드 가운데의 뷰 좌표. (흉내 낸 입력이 누를 자리)
//==============================================================================
fn calculate_node_center(node_tree: &NodeTree, node_id: NodeId) -> Vector2 {
	let content_size = node_tree[node_id].get_content_size();
	node_tree.convert_node_position_to_view_position(node_id, content_size.multiply_scalar(0.5))
}

//==============================================================================
// 갤러리 씬.
//==============================================================================
struct UIGalleryScene {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	frame_count: u32,
	elapsed_seconds: f32,
	click_count: Rc<Cell<u32>>,
	click_button_id: Option<NodeId>,
	hold_button_id: Option<NodeId>,
	slider_id: Option<NodeId>,
	toggle_id: Option<NodeId>,
	scroll_id: Option<NodeId>,
	scroll_row_id: Option<NodeId>,
	drag_start_position: Option<Vector2>,
	progress_id: Option<NodeId>,
	vertical_progress_id: Option<NodeId>,
}

impl UIGalleryScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> UIGalleryScene {
		UIGalleryScene {
			scene_base: SceneBase::new(),
			touch_recognizer: TouchRecognizer::new(),
			frame_count: 0,
			elapsed_seconds: 0.0,
			click_count: Rc::new(Cell::new(0)),
			click_button_id: None,
			hold_button_id: None,
			slider_id: None,
			toggle_id: None,
			scroll_id: None,
			scroll_row_id: None,
			drag_start_position: None,
			progress_id: None,
			vertical_progress_id: None,
		}
	}

	//==============================================================================
	// 구역 판 만들기. (배경색을 정한 UIView + 구역 이름)
	//==============================================================================
	fn create_section_panel(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2, size: Vector2, caption: &str) -> NodeId {
		let panel_id = create_box_node(node_tree, parent_id, name, position, size);
		let panel_view = node_tree.add_component(panel_id, UIView::new()).expect("view");
		panel_view.set_background_color(PANEL_COLOR);
		create_label_node(node_tree, panel_id, "caption", Vector2::create(14.0, 6.0), Vector2::create(size.x - 28.0, 26.0), caption, 17.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		panel_id
	}

	//==============================================================================
	// 버튼 구역. (클릭 셈, 누른 채 두는 버튼, 쓸 수 없는 그림 버튼)
	//==============================================================================
	fn create_button_section(&mut self, root_id: NodeId) {
		let click_count = self.click_count.clone();
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = UIGalleryScene::create_section_panel(node_tree, root_id, "buttonSection", Vector2::create(40.0, 90.0), Vector2::create(380.0, 250.0), "UIButton");

		let counter_label_id = create_label_node(node_tree, panel_id, "clickCounter", Vector2::create(210.0, 44.0), Vector2::create(160.0, 48.0), "클릭 0번", 22.0, LIGHT_TEXT_COLOR, TextAlign::Left);
		let click_button_id = create_text_button_node(node_tree, panel_id, "clickButton", Vector2::create(20.0, 44.0), Vector2::create(180.0, 48.0), "눌러 보세요", ACCENT_COLOR);
		let click_button = node_tree[click_button_id].get_component_mut::<UIButton>().expect("button");
		// 클릭 이벤트는 문맥을 받으므로, 콜백 안에서 다른 노드(셈 글자)를 바로 바꾼다.
		click_button.set_click_event(Box::new(move |_button: &mut UIButton, context: &mut ComponentContext| {
			let next_click_count = click_count.get() + 1;
			click_count.set(next_click_count);
			let counter_label = context.get_node_tree_mut().get_node_mut(counter_label_id).and_then(|counter_node| counter_node.get_component_mut::<UILabel>());
			if let Some(counter_label) = counter_label {
				let counter_text = format!("클릭 {}번", next_click_count);
				counter_label.set_text(&counter_text);
			}
		}));

		let hold_button_id = create_text_button_node(node_tree, panel_id, "holdButton", Vector2::create(20.0, 108.0), Vector2::create(180.0, 48.0), "누르고 있어요", ACCENT_COLOR);
		create_label_node(node_tree, panel_id, "holdCaption", Vector2::create(210.0, 108.0), Vector2::create(160.0, 48.0), "눌림 틴트", 18.0, CAPTION_TEXT_COLOR, TextAlign::Left);

		// 쓸 수 없는 그림 버튼. (스프라이트에 어두운 덮는 색)
		let disc_image = create_disc_image(96, Color::from("#9ad0ff"), Color::from("#3060c0"));
		let disabled_button_id = create_box_node(node_tree, panel_id, "disabledButton", Vector2::create(20.0, 172.0), Vector2::create(64.0, 64.0));
		node_tree[disabled_button_id].add_component(Sprite::create_with_image(disc_image.clone()));
		let disabled_button = node_tree.add_component(disabled_button_id, UIButton::new()).expect("button");
		disabled_button.set_interactable(false);
		let enabled_button_id = create_box_node(node_tree, panel_id, "enabledButton", Vector2::create(100.0, 172.0), Vector2::create(64.0, 64.0));
		node_tree[enabled_button_id].add_component(Sprite::create_with_image(disc_image));
		node_tree.add_component(enabled_button_id, UIButton::new());
		create_label_node(node_tree, panel_id, "disabledCaption", Vector2::create(180.0, 180.0), Vector2::create(190.0, 48.0), "왼쪽: 쓸 수 없음", 18.0, CAPTION_TEXT_COLOR, TextAlign::Left);

		self.click_button_id = Some(click_button_id);
		self.hold_button_id = Some(hold_button_id);
	}

	//==============================================================================
	// 토글 구역. (처음부터 켠 것, 흉내 낸 클릭으로 켜지는 것)
	//==============================================================================
	fn create_toggle_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = UIGalleryScene::create_section_panel(node_tree, root_id, "toggleSection", Vector2::create(40.0, 360.0), Vector2::create(380.0, 130.0), "UIToggleButton");
		let mut toggle_ids: Vec<NodeId> = Vec::new();
		for (toggle_index, toggle_text) in ["켜 둠", "클릭으로 켬", "꺼짐"].iter().enumerate() {
			let toggle_id = create_box_node(node_tree, panel_id, "toggle", Vector2::create(20.0 + toggle_index as f32 * 118.0, 50.0), Vector2::create(106.0, 56.0));
			let toggle_paint = node_tree[toggle_id].add_component(Paint::create_with_color("#e8d9c4"));
			toggle_paint.set_round_size(12.0);
			let label = node_tree.add_component(toggle_id, UILabel::create_with_text(toggle_text, 18.0)).expect("label");
			label.set_text_color(DARK_TEXT_COLOR);
			let toggle_button = node_tree.add_component(toggle_id, UIToggleButton::new()).expect("toggle");
			toggle_button.set_on_tint_color(Color::create(0.85, 0.45, 0.1, 0.6));
			// 글자까지 같은 색으로 물들면 읽기 어려워 글자는 틴트에서 뺀다.
			toggle_button.get_uibutton_mut().exclude_component_from_tint::<UILabel>(toggle_id);
			if toggle_index == 0 {
				toggle_button.set_is_on(true);
			}
			toggle_ids.push(toggle_id);
		}
		self.toggle_id = Some(toggle_ids[1]);
	}

	//==============================================================================
	// 슬라이더 / 진행률 구역.
	//==============================================================================
	fn create_value_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_id = UIGalleryScene::create_section_panel(node_tree, root_id, "valueSection", Vector2::create(440.0, 90.0), Vector2::create(380.0, 400.0), "UISlider / UIProgressView");

		// 가로 슬라이더. (값이 바뀌면 문맥으로 값 글자를 바꾼다)
		let value_label_id = create_label_node(node_tree, panel_id, "sliderValue", Vector2::create(250.0, 48.0), Vector2::create(110.0, 40.0), "값 0.30", 20.0, LIGHT_TEXT_COLOR, TextAlign::Left);
		let slider_id = create_box_node(node_tree, panel_id, "slider", Vector2::create(30.0, 58.0), Vector2::create(200.0, 20.0));
		let slider = node_tree.add_component(slider_id, UISlider::new()).expect("slider");
		slider.set_value(0.3);
		slider.set_track_thickness(8.0);
		slider.set_corner_radius(4.0);
		slider.set_thumb_radius(12.0);
		slider.set_fill_color(ACCENT_COLOR);
		slider.set_track_color("#5a4a3c");
		slider.set_value_changed_event(Box::new(move |slider: &mut UISlider, context: &mut ComponentContext| {
			let value_text = format!("값 {:.2}", slider.get_value());
			if let Some(value_label) = context.get_node_tree_mut().get_node_mut(value_label_id).and_then(|value_node| value_node.get_component_mut::<UILabel>()) {
				value_label.set_text(&value_text);
			}
		}));

		// 손잡이 노드를 쓰는 세로 슬라이더.
		let vertical_slider_id = create_box_node(node_tree, panel_id, "verticalSlider", Vector2::create(330.0, 110.0), Vector2::create(12.0, 260.0));
		let thumb_id = create_box_node(node_tree, vertical_slider_id, "thumb", Vector2::zero(), Vector2::create(28.0, 28.0));
		let thumb_paint = node_tree[thumb_id].add_component(Paint::create_with_color("#ff9a5a"));
		thumb_paint.set_round_size(8.0);
		let vertical_slider = node_tree.add_component(vertical_slider_id, UISlider::new()).expect("slider");
		vertical_slider.set_direction(ProgressDirection::Vertical);
		vertical_slider.set_corner_radius(6.0);
		vertical_slider.set_value(0.65);
		vertical_slider.set_thumb_node(Some(thumb_id));

		// 가로 진행률. (반대편 값도 함께)
		let progress_id = create_box_node(node_tree, panel_id, "progress", Vector2::create(30.0, 120.0), Vector2::create(260.0, 24.0));
		let progress_view = node_tree.add_component(progress_id, UIProgressView::new()).expect("progress");
		progress_view.set_corner_radius(12.0);
		progress_view.set_track_color("#5a4a3c");
		progress_view.set_fill_color("#7fd38a");
		progress_view.set_secondary_value(Some(0.2));
		create_label_node(node_tree, panel_id, "progressCaption", Vector2::create(30.0, 150.0), Vector2::create(260.0, 24.0), "초록은 차오르고, 빨강은 반대편 값", 15.0, CAPTION_TEXT_COLOR, TextAlign::Left);

		// 세로 진행률 셋.
		let mut vertical_progress_id: Option<NodeId> = None;
		for (progress_index, progress_value) in [0.25, 0.6, 0.9].iter().enumerate() {
			let bar_id = create_box_node(node_tree, panel_id, "verticalProgress", Vector2::create(40.0 + progress_index as f32 * 50.0, 190.0), Vector2::create(28.0, 180.0));
			let bar_view = node_tree.add_component(bar_id, UIProgressView::new()).expect("progress");
			bar_view.set_direction(ProgressDirection::Vertical);
			bar_view.set_corner_radius(6.0);
			bar_view.set_track_color("#5a4a3c");
			bar_view.set_value(*progress_value);
			if progress_index == 0 {
				vertical_progress_id = Some(bar_id);
			}
		}

		// 이미지 뷰 둘. (그대로, 좌우 뒤집고 색 입힘. 뒤집은 스프라이트는 원본처럼 노드 왼쪽으로 그려져 노드를 한 폭 오른쪽에 둔다)
		let image_view_image = create_disc_image(128, Color::from("#ffe6a0"), Color::from("#d0702a"));
		let image_view_id = create_box_node(node_tree, panel_id, "imageView", Vector2::create(190.0, 200.0), Vector2::create(110.0, 80.0));
		let image_view = node_tree.add_component(image_view_id, UIImageView::create_with_image(image_view_image.clone())).expect("image view");
		image_view.set_background_color(Color::create(1.0, 1.0, 1.0, 0.08));
		let tinted_image_view_id = create_box_node(node_tree, panel_id, "tintedImageView", Vector2::create(300.0, 290.0), Vector2::create(110.0, 80.0));
		let tinted_image_view = node_tree.add_component(tinted_image_view_id, UIImageView::create_with_image(image_view_image)).expect("image view");
		tinted_image_view.set_horizontal_flip(true);
		tinted_image_view.set_color(Color::create(0.3, 0.6, 1.0, 0.5));

		self.slider_id = Some(slider_id);
		self.progress_id = Some(progress_id);
		self.vertical_progress_id = vertical_progress_id;
	}

	//==============================================================================
	// 스크롤 뷰 구역. (줄 버튼 목록 + 세로 스크롤바)
	//==============================================================================
	fn create_scroll_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		create_label_node(node_tree, root_id, "scrollCaption", Vector2::create(840.0, 90.0), Vector2::create(400.0, 26.0), "UIScrollView + UIScrollBar (휠, 끌기)", 17.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		let scroll_width = 400.0;
		let scroll_height = 470.0;
		let scroll_id = create_box_node(node_tree, root_id, "scroll", Vector2::create(840.0, 120.0), Vector2::create(scroll_width, scroll_height));
		let scroll_content_height = SCROLL_ROW_GAP + SCROLL_ROW_COUNT as f32 * (SCROLL_ROW_HEIGHT + SCROLL_ROW_GAP);
		let content_id = {
			let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(PANEL_COLOR);
			scroll_view.set_scroll_content_size(Vector2::create(scroll_width, scroll_content_height));
			scroll_view.set_shows_vertical_scroll_bar(true);
			scroll_view.set_horizontal(false);
			scroll_view.get_content().expect("content")
		};
		let row_width = scroll_width - 24.0 - SCROLL_ROW_GAP * 2.0;
		let mut first_row_id: Option<NodeId> = None;
		for row_index in 0..SCROLL_ROW_COUNT {
			let row_color = if row_index % 2 == 0 {
				"#4a3c30"
			}
			else {
				"#5a4a3c"
			};
			let row_position = Vector2::create(SCROLL_ROW_GAP, SCROLL_ROW_GAP + row_index as f32 * (SCROLL_ROW_HEIGHT + SCROLL_ROW_GAP));
			let row_id = create_box_node(node_tree, content_id, "row", row_position, Vector2::create(row_width, SCROLL_ROW_HEIGHT));
			let row_paint = node_tree[row_id].add_component(Paint::create_with_color(row_color));
			row_paint.set_round_size(8.0);
			let row_text = format!("  {}번째 줄 (눌러도 되고 끌어도 돼요)", row_index + 1);
			let row_label = node_tree.add_component(row_id, UILabel::create_with_text(&row_text, 18.0)).expect("label");
			row_label.set_text_align(TextAlign::Left);
			row_label.set_text_color(LIGHT_TEXT_COLOR);
			node_tree.add_component(row_id, UIButton::new());
			if row_index == 3 {
				first_row_id = Some(row_id);
			}
		}
		self.scroll_id = Some(scroll_id);
		self.scroll_row_id = first_row_id;
	}

	//==============================================================================
	// 흉내 낸 입력. (정해 둔 프레임에 누름 / 이동 / 뗌 / 휠)
	//==============================================================================
	fn simulate_input(&mut self, engine: &mut Engine) {
		let frame_count = self.frame_count;
		let node_tree = self.scene_base.get_node_tree();
		let scroll_center = self.scroll_id.map(|scroll_id| calculate_node_center(node_tree, scroll_id));
		let click_button_center = self.click_button_id.map(|click_button_id| calculate_node_center(node_tree, click_button_id));
		let hold_button_center = self.hold_button_id.map(|hold_button_id| calculate_node_center(node_tree, hold_button_id));
		let toggle_center = self.toggle_id.map(|toggle_id| calculate_node_center(node_tree, toggle_id));
		let slider_position = self.slider_id.map(|slider_id| node_tree.convert_node_position_to_view_position(slider_id, Vector2::create(200.0 * 0.7, 10.0)));
		let scroll_row_center = self.scroll_row_id.map(|scroll_row_id| calculate_node_center(node_tree, scroll_row_id));
		// 끄는 동안 줄도 따라 움직이므로, 누른 자리는 누를 때 한 번만 정한다.
		if frame_count == 58 {
			self.drag_start_position = scroll_row_center;
		}
		let drag_start_position = self.drag_start_position;

		let input_manager = engine.get_input_manager_mut();
		let mut input_position: Option<Vector2> = None;
		if (20..30).contains(&frame_count) {
			input_position = scroll_center;
			if frame_count == 20 {
				input_manager.add_wheel_delta(0.0, 120.0);
			}
		}
		else if (30..40).contains(&frame_count) {
			input_position = click_button_center;
		}
		else if (40..50).contains(&frame_count) {
			input_position = slider_position;
		}
		else if (50..58).contains(&frame_count) {
			input_position = toggle_center;
		}
		else if (58..75).contains(&frame_count) {
			// 줄 버튼을 누르고 10 프레임 동안 60 픽셀 위로 끈다. (임계를 넘으면 스크롤 뷰가 가져간다)
			let drag_frame_count = frame_count.saturating_sub(59).min(10) as f32;
			input_position = drag_start_position.map(|drag_start_position| Vector2::create(drag_start_position.x, drag_start_position.y - drag_frame_count * 6.0));
		}
		else if frame_count >= 75 {
			input_position = hold_button_center;
		}
		if let Some(input_position) = input_position {
			input_manager.set_view_input_position(input_position);
		}
		else {
			input_manager.set_view_input_position(Vector2::create(-100.0, -100.0));
		}

		let press_frames = [30, 40, 50, 58, 75];
		let release_frames = [33, 43, 53, 70];
		if press_frames.contains(&frame_count) {
			input_manager.set_touch_pressed(true);
			input_manager.set_touch_down(true);
		}
		if (59..=69).contains(&frame_count) {
			input_manager.set_touch_moved(true);
		}
		if release_frames.contains(&frame_count) {
			input_manager.set_touch_released(true);
			input_manager.set_touch_down(false);
		}
	}

	//==============================================================================
	// 진행률을 시간에 따라 움직인다.
	//==============================================================================
	fn animate_progress(&mut self) {
		let progress_value = 0.15 + 0.5 * (0.5 + 0.5 * (self.elapsed_seconds * 1.5).sin());
		let progress_id = self.progress_id;
		let vertical_progress_id = self.vertical_progress_id;
		let node_tree = self.scene_base.get_node_tree_mut();
		if let Some(progress_view) = progress_id.and_then(|progress_id| node_tree.get_node_mut(progress_id)).and_then(|progress_node| progress_node.get_component_mut::<UIProgressView>()) {
			progress_view.set_value(progress_value);
		}
		if let Some(progress_view) = vertical_progress_id.and_then(|progress_id| node_tree.get_node_mut(progress_id)).and_then(|progress_node| progress_node.get_component_mut::<UIProgressView>()) {
			progress_view.set_value(1.0 - progress_value);
		}
	}
}

impl Scene for UIGalleryScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (위젯을 모두 놓는다)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = create_label_node(node_tree, root_id, "title", Vector2::create(40.0, 24.0), Vector2::create(800.0, 50.0), "바닐라엔진 UI 위젯 갤러리", 34.0, ACCENT_COLOR, TextAlign::Left);
		if let Some(title_label) = node_tree[title_id].get_component_mut::<UILabel>() {
			title_label.set_bold(true);
		}
		create_label_node(node_tree, root_id, "help", Vector2::create(40.0, 650.0), Vector2::create(1200.0, 40.0), "입력은 흉내 낸 것: 휠 스크롤 → 버튼 클릭 → 슬라이더 70% → 토글 클릭 → 목록 끌기 → 버튼 누른 채", 17.0, CAPTION_TEXT_COLOR, TextAlign::Left);
		self.create_button_section(root_id);
		self.create_toggle_section(root_id);
		self.create_value_section(root_id);
		self.create_scroll_section(root_id);
	}

	//==============================================================================
	// 주기적 갱신. (흉내 낸 입력 → 노드 갱신 → TouchRecognizer)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.frame_count += 1;
		self.elapsed_seconds += time_delta;
		if self.frame_count >= EXIT_FRAME_COUNT {
			engine.exit_application();
			return;
		}
		self.simulate_input(engine);
		self.animate_progress();
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
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
	engine_configuration.title = String::from("UI 위젯 갤러리 (바닐라엔진)");
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	// 화면 저장 때도 프레임 간격이 일정하도록 60 FPS 로 묶는다. (틴트 트랜지션이 프레임 수와 맞게)
	engine_configuration.maximum_frame_per_second = 60.0;
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(UIGalleryScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
