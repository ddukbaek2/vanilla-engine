//==============================================================================
// 2D 묶음의 셰이더 자르기와 자르기 안 거르기 확인. (원본 tests/core/batchclipchrome.test.mjs 와 같은 장면)
// - 축에 맞고 화소 경계에 놓인 자르기는 스텐실 대신 묶음 셰이더로 자른다. 같은 장면을 아주 조금 돌려(1e-9 라디안) 스텐실 길로 그린 것과 화소가 모두 같은지 본다.
//   장면: 스크롤 칸(UIScrollView, 줄 40 개), 겹친 자르기, 둥근 자르기 안팎, 정점 색 쿼드, 더하기와 셰이더 섞기, 불투명도 층, 바깥 GL 패스.
// - 씬이 그려진 다섯째 프레임을 찍고 끝낸다. (씬 draw 안에서 센다, 로딩 화면은 세지 않는다)
//   프레임마다 드로 콜과 거른 노드 수를 표준 출력에 "drawCalls=..., culledNodes=..." 로 적는다.
// - 창 크기와 렌더 배율은 BATCHCLIP_WINDOW_WIDTH, BATCHCLIP_WINDOW_HEIGHT, BATCHCLIP_PIXEL_RATIO 로 바꾼다. (기본 1280 x 720, 1.5)
//   자르기 경계가 화소 경계에 놓이지 않는 배율(예: 높이 730, 배율 1)로 찍으면 멀티샘플 화면은 스텐실로 자르므로 두 길이 여전히 같아야 한다.
// - BATCHCLIP_SCENE=settings 면 설정 창 같은 장면(창 하나, UIScrollView, 줄 50 개, 줄마다 글자와 단추와 도형)으로 드로 콜만 잰다.
// - 실행: BATCHCLIP_OUTPUT=shots/batchclip.png [BATCHCLIP_ROTATION=1] VANILLA_CAPTURE_PATH=shots/unused.png cargo run --release --example batchclip
//   (VANILLA_CAPTURE_PATH 는 창을 숨기려고 건다, 찍는 프레임은 이 예제가 정한다)
//==============================================================================

use glow::HasContext;
use vanilla::core::graphic::TextAlign;
use vanilla::core::shaderprogram::ShaderProgram;
use vanilla::{BlendMode, Color, Engine, EngineConfiguration, Graphic, NodeId, NodeTree, Paint, Pivot, Rect, Scene, SceneBase, UIButton, UILabel, UIScrollView, UIView, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f64 = 640.0;
const VIEW_HEIGHT: f64 = 360.0;
const CAPTURE_DRAWN_FRAME_COUNT: u32 = 5;
const CLIP_ROW_COUNT: usize = 40;
const SETTINGS_ROW_COUNT: usize = 50;
// 바깥 GL 패스 셰이더. (정점 셋으로 화면을 덮는 삼각형, 원본 시험과 같은 글)
const EXTERNAL_VERTEXSHADER_SOURCE: &str = "#version 300 es
void main() { vec2 corner = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)); gl_Position = vec4(corner * 2.0 - 1.0, 0.0, 1.0); }";
const EXTERNAL_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float; out vec4 outputColor; void main() { outputColor = vec4(0.9, 0.2, 0.6, 1.0); }";

//==============================================================================
// 왼쪽 위 기준 상자 노드 만들기.
//==============================================================================
fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, position: Vector2, size: Vector2) -> NodeId {
	let node_id = node_tree.create_child(parent_id, "box");
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_local_position(position);
	node.set_content_size(size);
	node_id
}

//==============================================================================
// 장면 종류.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq)]
enum SceneKind {
	// 원본 batchclipchrome 시험 장면.
	Clip,
	// 설정 창 같은 장면.
	Settings,
}

//==============================================================================
// 묶음 자르기 씬.
//==============================================================================
struct BatchClipScene {
	scene_base: SceneBase,
	scene_kind: SceneKind,
	rotation: f64,
	drawn_frame_count: u32,
	capture_frame_number: Option<u64>,
	output_path: Option<String>,
	external_program: Option<ShaderProgram>,
	draw_call_count_list: Vec<u32>,
	culled_node_count_list: Vec<u32>,
}

impl BatchClipScene {
	//==============================================================================
	// 원본 시험의 스크롤 칸. (줄 40 개, 줄마다 둥근 Paint, UILabel, UIButton)
	//==============================================================================
	fn create_clip_scene(&mut self) {
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let scroll_id = create_box_node(node_tree, root_id, Vector2::create(20.0, 20.0), Vector2::create(300.0, 200.0));
		let content_id = {
			let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(Color::create_from_hex("#1d1814"));
			scroll_view.set_scroll_content_size(Vector2::create(300.0, CLIP_ROW_COUNT as f64 * 30.0));
			scroll_view.set_horizontal(false);
			scroll_view.set_scroll_offset(Vector2::create(0.0, -95.0));
			scroll_view.get_content().expect("content")
		};
		for row_index in 0..CLIP_ROW_COUNT {
			let row_id = create_box_node(node_tree, content_id, Vector2::create(4.0, row_index as f64 * 30.0), Vector2::create(280.0, 26.0));
			let row_color = if row_index % 2 == 0 {
				"#4a3c30"
			}
			else {
				"#5a4a3c"
			};
			let row_paint = node_tree[row_id].add_component(Paint::create_with_color(row_color));
			row_paint.set_round_size(6.0);
			let label = node_tree.add_component(row_id, UILabel::create_with_text(&format!("줄 {}", row_index), 14.0)).expect("label");
			label.set_text_align(TextAlign::Left);
			label.set_text_color("#f1e9d8");
			node_tree.add_component(row_id, UIButton::new());
		}
	}

	//==============================================================================
	// 설정 창 같은 장면. (창 하나, 제목, UIScrollView, 줄 50 개, 줄마다 도형과 글자와 단추)
	//==============================================================================
	fn create_settings_scene(&mut self) {
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let window_id = create_box_node(node_tree, root_id, Vector2::create(120.0, 20.0), Vector2::create(400.0, 320.0));
		node_tree.add_component(window_id, UIView::new()).expect("view").set_background_color("#2c241e");
		let title_id = create_box_node(node_tree, window_id, Vector2::create(16.0, 8.0), Vector2::create(368.0, 28.0));
		let title_label = node_tree.add_component(title_id, UILabel::create_with_text("설정", 18.0)).expect("label");
		title_label.set_text_align(TextAlign::Left);
		title_label.set_text_color("#f3e5ab");
		let scroll_id = create_box_node(node_tree, window_id, Vector2::create(16.0, 44.0), Vector2::create(368.0, 260.0));
		let content_id = {
			let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color("#1d1814");
			scroll_view.set_scroll_content_size(Vector2::create(368.0, SETTINGS_ROW_COUNT as f64 * 32.0));
			scroll_view.set_horizontal(false);
			scroll_view.get_content().expect("content")
		};
		for row_index in 0..SETTINGS_ROW_COUNT {
			let row_id = create_box_node(node_tree, content_id, Vector2::create(4.0, row_index as f64 * 32.0), Vector2::create(352.0, 28.0));
			let row_paint = node_tree[row_id].add_component(Paint::create_with_color("#4a3c30"));
			row_paint.set_round_size(6.0);
			let label_id = create_box_node(node_tree, row_id, Vector2::create(8.0, 0.0), Vector2::create(220.0, 28.0));
			let label = node_tree.add_component(label_id, UILabel::create_with_text(&format!("설정 항목 {}", row_index + 1), 14.0)).expect("label");
			label.set_text_align(TextAlign::Left);
			label.set_text_color("#f1e9d8");
			let button_id = create_box_node(node_tree, row_id, Vector2::create(272.0, 3.0), Vector2::create(72.0, 22.0));
			node_tree[button_id].add_component(Paint::create_with_color("#f3e5ab")).set_round_size(11.0);
			let button_label = node_tree.add_component(button_id, UILabel::create_with_text("바꾸기", 12.0)).expect("label");
			button_label.set_text_color("#2b2118");
			node_tree.add_component(button_id, UIButton::new());
		}
	}

	//==============================================================================
	// 정점 색 쿼드 여덟. (원본 시험과 같은 값)
	//==============================================================================
	fn draw_colored_quads(graphic: &mut Graphic, left: f64, top: f64) {
		let corner_list: [(f32, f32); 6] = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)];
		let vertex_data = graphic.get_particle_vertex_data(8 * 48);
		let mut offset = 0;
		for quad_index in 0..8 {
			for (corner_x, corner_y) in corner_list {
				vertex_data[offset] = (left + quad_index as f64 * 13.5 + corner_x as f64 * 20.0) as f32;
				vertex_data[offset + 1] = (top + corner_y as f64 * 20.0) as f32;
				vertex_data[offset + 2] = corner_x;
				vertex_data[offset + 3] = corner_y;
				vertex_data[offset + 4] = (quad_index as f64 / 8.0) as f32;
				vertex_data[offset + 5] = corner_x;
				vertex_data[offset + 6] = (corner_y as f64 * 0.7) as f32;
				vertex_data[offset + 7] = (0.4 + 0.3 * corner_x as f64) as f32;
				offset += 8;
			}
		}
		let soft_disc_texture = graphic.get_soft_disc_texture();
		graphic.draw_colored_quads(8 * 6, soft_disc_texture);
	}

	//==============================================================================
	// 원본 시험의 자르기 장면. (노드 트리 뒤에 그린다)
	//==============================================================================
	fn draw_clip_shapes(&mut self, graphic: &mut Graphic) {
		graphic.begin_clip_rect(Rect::create(340.0, 20.0, 120.0, 80.0), 0.0);
		graphic.set_fill_color("rgba(80, 160, 255, 0.8)");
		graphic.draw_circle(Vector2::create(400.0, 60.0), 70.0);
		graphic.begin_clip_rect(Rect::create(360.0, 40.0, 60.0, 80.0), 0.0);
		graphic.set_fill_color("rgba(255, 60, 60, 0.6)");
		graphic.draw_rect(Rect::create(300.0, 0.0, 300.0, 200.0));
		Self::draw_colored_quads(graphic, 340.0, 70.0);
		graphic.end_clip_rect();
		graphic.begin_clip_rect(Rect::create(380.0, 30.0, 100.0, 60.0), 12.0);
		graphic.set_fill_color("rgba(60, 255, 60, 0.5)");
		graphic.draw_rect(Rect::create(300.0, 0.0, 300.0, 200.0));
		graphic.end_clip_rect();
		graphic.set_blend_mode(BlendMode::Lighter);
		Self::draw_colored_quads(graphic, 330.0, 30.0);
		graphic.set_blend_mode(BlendMode::Overlay);
		graphic.set_fill_color("rgba(255, 120, 40, 0.7)");
		graphic.draw_circle(Vector2::create(450.0, 90.0), 40.0);
		graphic.set_blend_mode(BlendMode::Normal);
		graphic.end_clip_rect();
		graphic.begin_clip_rect(Rect::create(480.0, 20.0, 140.0, 100.0), 16.0);
		graphic.begin_clip_rect(Rect::create(470.0, 40.0, 100.0, 100.0), 0.0);
		graphic.push_state();
		graphic.set_global_alpha(0.5);
		graphic.begin_opacity_layer();
		graphic.set_fill_color("#ff0000");
		graphic.draw_rect(Rect::create(460.0, 30.0, 100.0, 60.0));
		graphic.set_fill_color("#0000ff");
		graphic.draw_rect(Rect::create(500.0, 60.0, 100.0, 60.0));
		graphic.end_opacity_layer();
		graphic.pop_state();
		graphic.end_clip_rect();
		graphic.end_clip_rect();
		graphic.begin_clip_rect(Rect::create(340.0, 140.0, 100.0, 60.0), 0.0);
		graphic.begin_clip_rect(Rect::create(380.0, 120.0, 100.0, 60.0), 0.0);
		// 바깥 패스는 화면 전체를 분홍으로 칠한다. (걸린 자르기 둘이 겹친 칸에만 남아야 한다)
		let mut external_program = self.external_program.take();
		graphic.run_external_pass(false, |gl| unsafe {
			if external_program.is_none() {
				external_program = ShaderProgram::new(gl, EXTERNAL_VERTEXSHADER_SOURCE, EXTERNAL_FRAGMENTSHADER_SOURCE).ok();
			}
			if let Some(external_program) = &external_program {
				let empty_vertex_array = gl.create_vertex_array().ok();
				gl.use_program(Some(external_program.get_program()));
				gl.bind_vertex_array(empty_vertex_array);
				gl.disable(glow::BLEND);
				gl.draw_arrays(glow::TRIANGLES, 0, 3);
				gl.bind_vertex_array(None);
				if let Some(empty_vertex_array) = empty_vertex_array {
					gl.delete_vertex_array(empty_vertex_array);
				}
			}
		});
		self.external_program = external_program;
		graphic.end_clip_rect();
		graphic.end_clip_rect();
	}
}

impl Scene for BatchClipScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성.
	//==============================================================================
	fn create(&mut self, _engine: &Engine) {
		match self.scene_kind {
			SceneKind::Clip => {
				self.create_clip_scene();
			}
			SceneKind::Settings => {
				self.create_settings_scene();
			}
		}
	}

	//==============================================================================
	// 출력. (씬이 그려진 다섯째 프레임을 찍고, 그다음 프레임에 끝낸다)
	//==============================================================================
	fn draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		if let Some(capture_frame_number) = self.capture_frame_number {
			if engine.get_frame_number() > capture_frame_number {
				println!("drawCalls={:?}", self.draw_call_count_list);
				println!("culledNodes={:?}", self.culled_node_count_list);
				engine.exit_application();
				return;
			}
		}
		self.drawn_frame_count += 1;
		if self.drawn_frame_count == CAPTURE_DRAWN_FRAME_COUNT {
			let frame_number = engine.get_frame_number();
			self.capture_frame_number = Some(frame_number);
			if let Some(output_path) = self.output_path.clone() {
				engine.request_frame_capture(
					frame_number,
					Box::new(move |png_bytes: &[u8]| {
						if let Err(error) = std::fs::write(&output_path, png_bytes) {
							eprintln!("batchclip: capture write failed: {}", error);
						}
					}),
				);
			}
		}
		graphic.rotate(self.rotation);
		self.scene_base.draw(graphic);
		if self.scene_kind == SceneKind::Clip {
			self.draw_clip_shapes(graphic);
		}
		self.draw_call_count_list.push(graphic.get_last_frame_draw_call_count());
		self.culled_node_count_list.push(graphic.get_last_frame_culled_node_count());
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let scene_kind = if std::env::var("BATCHCLIP_SCENE").map(|value| value == "settings").unwrap_or(false) {
		SceneKind::Settings
	}
	else {
		SceneKind::Clip
	};
	let rotation = if std::env::var("BATCHCLIP_ROTATION").map(|value| value == "1").unwrap_or(false) {
		1e-9
	}
	else {
		0.0
	};
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("묶음 자르기");
	let window_width = std::env::var("BATCHCLIP_WINDOW_WIDTH").ok().and_then(|value| value.parse::<f64>().ok()).unwrap_or(VIEW_WIDTH * 2.0);
	let window_height = std::env::var("BATCHCLIP_WINDOW_HEIGHT").ok().and_then(|value| value.parse::<f64>().ok()).unwrap_or(VIEW_HEIGHT * 2.0);
	engine_configuration.window_size = Vector2::create(window_width, window_height);
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	let engine = Engine::new(engine_configuration);
	let pixel_ratio = std::env::var("BATCHCLIP_PIXEL_RATIO").ok().and_then(|value| value.parse::<f64>().ok()).unwrap_or(1.5);
	engine.get_view_manager_mut().set_max_render_pixel_ratio(pixel_ratio);
	let batch_clip_scene = BatchClipScene {
		scene_base: SceneBase::new(),
		scene_kind,
		rotation,
		drawn_frame_count: 0,
		capture_frame_number: None,
		output_path: std::env::var("BATCHCLIP_OUTPUT").ok(),
		external_program: None,
		draw_call_count_list: Vec::new(),
		culled_node_count_list: Vec::new(),
	};
	if let Err(error) = engine.run(Box::new(batch_clip_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
