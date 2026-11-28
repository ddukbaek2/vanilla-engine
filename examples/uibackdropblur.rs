//==============================================================================
// UIView 뒤 화면 흐림(set_backdrop_blur, set_backdrop_corner_radius) 확인. (원본 tests/ui/uibackdropblurcss.test.mjs 와 같은 장면)
// - 흰 줄과 검은 줄(폭 10), 색 사각형 셋, 비스듬한 노란 점이 깔린 화면 위에 흐림 표준편차 UIBACKDROPBLUR_RADIUS(기본 6, CSS blur(Npx) 의 N)와
//   모서리 반지름 UIBACKDROPBLUR_CORNER(기본 0)의 투명 뷰(200, 100, 240 x 160)를 얹어 열째로 그려진 프레임을 찍는다.
// - 원본 엔진 장면과 화소를 견주어 두 엔진의 흐림이 같은지 본다.
// - 실행: UIBACKDROPBLUR_OUTPUT=shots/blur.png VANILLA_CAPTURE_PATH=shots/unused.png cargo run --example uibackdropblur
//   (VANILLA_CAPTURE_PATH 는 창을 숨기려고 건다, 찍는 프레임은 이 예제가 정한다)
//==============================================================================

use vanilla::{Color, Engine, EngineConfiguration, Graphic, NodeId, NodeTree, Pivot, Scene, SceneBase, UIView, Vector2, ViewScaleMode};

const VIEW_WIDTH: f64 = 640.0;
const VIEW_HEIGHT: f64 = 360.0;
const CAPTURE_DRAWN_FRAME_COUNT: u32 = 10;

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
// 뒤 화면 흐림 씬.
//==============================================================================
struct BackdropBlurScene {
	scene_base: SceneBase,
	blur_radius: f64,
	corner_radius: f64,
	drawn_frame_count: u32,
	capture_frame_number: Option<u64>,
	output_path: Option<String>,
}

impl Scene for BackdropBlurScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (줄 64 개와 흐림 패널)
	//==============================================================================
	fn load(&mut self, _engine: &Engine) {
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		for stripe_index in 0..64 {
			let stripe_id = create_box_node(node_tree, root_id, Vector2::create(stripe_index as f64 * 10.0, 0.0), Vector2::create(10.0, 360.0));
			let stripe_view = node_tree.add_component(stripe_id, UIView::new()).expect("view");
			stripe_view.set_background_color(if stripe_index % 2 == 0 {
				Color::create_from_hex("#ffffff")
			}
			else {
				Color::create_from_hex("#000000")
			});
		}
		let mut box_list = vec![(150.0, 60.0, 120.0, 80.0, "#e03030"), (330.0, 200.0, 160.0, 90.0, "#2060e0"), (260.0, 140.0, 100.0, 100.0, "#30c050")];
		for line_index in 0..12 {
			box_list.push((205.0 + line_index as f64 * 18.0, 105.0 + line_index as f64 * 12.0, 6.0, 6.0, "#ffd000"));
		}
		for (x, y, width, height, hex) in box_list {
			let box_id = create_box_node(node_tree, root_id, Vector2::create(x, y), Vector2::create(width, height));
			node_tree.add_component(box_id, UIView::new()).expect("view").set_background_color(Color::create_from_hex(hex));
		}
		let panel_id = create_box_node(node_tree, root_id, Vector2::create(200.0, 100.0), Vector2::create(240.0, 160.0));
		let panel_view = node_tree.add_component(panel_id, UIView::new()).expect("view");
		panel_view.set_backdrop_blur(self.blur_radius);
		panel_view.set_backdrop_corner_radius(self.corner_radius);
	}

	//==============================================================================
	// 출력. (씬이 그려진 열째 프레임을 찍고, 그다음 프레임에 끝낸다)
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		if let Some(capture_frame_number) = self.capture_frame_number {
			if engine.get_frame_number() > capture_frame_number {
				// 웹은 스스로 끝내지 않는다. (끝내면 탭이 닫혀 실기기 확인 도구의 창이 죽는다)
				if !cfg!(target_arch = "wasm32") {
					engine.exit_application();
					return;
				}
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
							eprintln!("uibackdropblur: capture write failed: {}", error);
						}
					}),
				);
			}
		}
		self.scene_base.draw(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let blur_radius = std::env::var("UIBACKDROPBLUR_RADIUS").ok().and_then(|value| value.parse::<f64>().ok()).unwrap_or(6.0);
	let corner_radius = std::env::var("UIBACKDROPBLUR_CORNER").ok().and_then(|value| value.parse::<f64>().ok()).unwrap_or(0.0);
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("뒤 화면 흐림");
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	let engine = Engine::new(engine_configuration);
	engine.get_view_manager_mut().set_max_render_pixel_ratio(1.0);
	let scene = BackdropBlurScene {
		scene_base: SceneBase::new(),
		blur_radius,
		corner_radius,
		drawn_frame_count: 0,
		capture_frame_number: None,
		output_path: std::env::var("UIBACKDROPBLUR_OUTPUT").ok(),
	};
	if let Err(error) = engine.run(Box::new(scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
