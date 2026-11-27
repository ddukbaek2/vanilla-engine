//==============================================================================
// 노드 사각형 안 툰 3D 예제. (ToonView3D, Camera3D, ToonDrawQueue, ToonPostProcess 색 보정)
// - 왼쪽 판: 2D 바탕(Paint) 위에 ToonView3D 가 노드 사각형 안에만 툰을 그린다. 불투명한 것, 바깥선, 비치는 공 셋(먼 것부터 섞음),
//   MeshBuilder::combine 으로 이은 각진 돌덩이 셋, 인스턴싱한 작은 돌.
//   노란 점과 이름표는 렌더러와 같은 Camera3D 의 project_to_view 로 3D 점 위에 얹은 2D 다. 판 다음 형제인 노란 띠는 3D 위에 그려진다.
// - 오른쪽 판: 같은 장면을 ToonPostProcess 에 그려 채도 낮춤, 따뜻한 색조, 안개빛, 대비, 가장자리 어둡게를 건다.
// - 두 판은 ToonView3D::set_renderer 로 렌더러 하나를 나눠 쓰고, 그릴 것은 set_draw_callback 의 클로저가 들고 있는다.
// - 그리기 함수의 화면 사각형은 Rect 의 position, size 이고, 출력(ToonViewOutput)의 output_framebuffer, output_viewport 로 후처리를 낸다. (원본은 사각형 객체의 outputFramebuffer, outputViewport)
// - 환경 변수 VANILLA_RENDER_SCALE=0.5 처럼 주면 두 판의 3D 만 그 배율로 그려 늘여 얹는다. (원본 ?renderScale=0.5, ToonView3D::set_render_scale, 이름표와 띠 같은 2D 는 기기 화소 그대로)
// - 실행: cargo run --example toonview
//==============================================================================
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;
use vanilla::{
	create_box_geometry, create_faceted_chunk_geometry, create_part_matrix, create_sphere_geometry, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, NodeId, NodeTree, Paint, Pivot, Rect, Scene, SceneBase, Text, TextAlign, ToonDrawOptions, ToonDrawQueue, ToonGeometry, ToonInstance,
	ToonMesh, ToonMeshArrays, ToonMeshPiece, ToonPartOptions, Camera3D, ToonPostProcess, ToonRenderer, ToonView3D, ToonViewOutput, Vector2, Vector3,
};

//==============================================================================
// 판에 그리는 모형.
//==============================================================================
struct PanelMeshes {
	ground: ToonMesh,
	box_mesh: ToonMesh,
	sphere: ToonMesh,
	glass: ToonMesh,
	pile: ToonMesh,
	pebble: ToonMesh,
}

//==============================================================================
// 판 하나의 그리기 상태. (오른쪽 판은 ToonPostProcess 로 색 보정, ToonView3D 의 그리기 함수가 들고 있는다)
//==============================================================================
struct PanelDrawer {
	is_graded: bool,
	elapsed_seconds: Rc<Cell<f64>>,
	meshes: Option<PanelMeshes>,
	pebble_instance_list: Vec<ToonInstance>,
	post_process: Option<ToonPostProcess>,
	frame_count: u32,
}

//==============================================================================
// 부품 하나의 배열.
//==============================================================================
fn build_arrays(geometry: &ToonGeometry, position: [f64; 3], color_text: &str, options: ToonPartOptions) -> ToonMeshArrays {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, &create_part_matrix(None, position, None, None), color_text, options);
	builder.read_arrays()
}

impl PanelDrawer {
	fn new(is_graded: bool, elapsed_seconds: Rc<Cell<f64>>) -> PanelDrawer {
		let mut pebble_instance_list = Vec::new();
		for pebble_index in 0..24 {
			let angle = pebble_index as f64 / 24.0 * PI * 2.0;
			let mut model_matrix = Matrix4::create_translation(angle.cos() * 3.3, 0.0, angle.sin() * 2.4);
			model_matrix.rotate_y(angle * 2.0);
			pebble_instance_list.push(ToonInstance { model_matrix, ..ToonInstance::default() });
		}
		PanelDrawer { is_graded, elapsed_seconds, meshes: None, pebble_instance_list, post_process: None, frame_count: 0 }
	}

	//==============================================================================
	// 모형 짓기. (처음 그릴 때 한 번)
	//==============================================================================
	fn create_meshes(renderer: &ToonRenderer, gl: &glow::Context) -> Option<PanelMeshes> {
		let outline_options = ToonPartOptions { outline_width: 0.03, ..ToonPartOptions::default() };
		let chunk = build_arrays(&create_faceted_chunk_geometry(0.4, &[1.0, 0.85, 1.1, 0.95, 1.05, 0.9], [1.0, 0.8, 1.0]), [0.0, 0.3, 0.0], "#9a8a7a", ToonPartOptions::default());
		let pile = MeshBuilder::combine(&[ToonMeshPiece { arrays: &chunk, offset: [0.0, 0.0, 0.0] }, ToonMeshPiece { arrays: &chunk, offset: [0.7, 0.0, 0.2] }, ToonMeshPiece { arrays: &chunk, offset: [0.3, 0.45, 0.1] }]);
		Some(PanelMeshes {
			ground: renderer.create_mesh(gl, &build_arrays(&create_box_geometry(8.0, 0.2, 6.0), [0.0, -0.1, 0.0], "#6b8f5a", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() }))?,
			box_mesh: renderer.create_mesh(gl, &build_arrays(&create_box_geometry(1.0, 1.0, 1.0), [0.0, 0.5, 0.0], "#ff8844", outline_options))?,
			sphere: renderer.create_mesh(gl, &build_arrays(&create_sphere_geometry(0.5, 24, 16, PI), [0.0, 0.5, 0.0], "#ffd257", outline_options))?,
			glass: renderer.create_mesh(gl, &build_arrays(&create_sphere_geometry(0.45, 24, 16, PI), [0.0, 0.45, 0.0], "#66ccff", ToonPartOptions { outline_width: 0.02, ..ToonPartOptions::default() }))?,
			pile: renderer.create_mesh(gl, &pile)?,
			pebble: renderer.create_mesh(gl, &build_arrays(&create_faceted_chunk_geometry(0.12, &[1.0, 0.8, 1.2], [1.0, 0.7, 1.0]), [0.0, 0.08, 0.0], "#c9b8a0", ToonPartOptions { outline_width: 0.012, ..ToonPartOptions::default() }))?,
		})
	}

	//==============================================================================
	// 판 그리기. (ToonView3D 의 그리기 함수)
	//==============================================================================
	fn draw(&mut self, renderer: &mut ToonRenderer, camera: &Camera3D, screen_rect: Rect, gl: &glow::Context, output: ToonViewOutput) {
		if self.meshes.is_none() {
			self.meshes = PanelDrawer::create_meshes(renderer, gl);
		}
		let meshes = match self.meshes.as_ref() {
			Some(meshes) => {
				meshes
			}
			None => {
				return;
			}
		};
		renderer.set_band_count(3);
		renderer.set_light([-0.5, -1.0, -0.35], [0.85, 0.8, 0.75], [0.55, 0.6, 0.7], [0.25, 0.22, 0.2]);

		// 담기. (불투명, 인스턴싱, 비치는 공 셋은 앞뒤로 겹쳐 놓아 먼 것부터 섞여야 바르게 보임)
		let mut queue = ToonDrawQueue::new();
		queue.add_mesh(&meshes.ground, &Matrix4::create_identity(), ToonDrawOptions::default());
		let mut box_matrix = Matrix4::create_translation(-1.6, 0.0, 0.0);
		box_matrix.rotate_y(0.5 + self.elapsed_seconds.get() * 0.5);
		queue.add_mesh(&meshes.box_mesh, &box_matrix, ToonDrawOptions::default());
		queue.add_mesh(&meshes.sphere, &Matrix4::create_translation(0.2, 0.0, -0.6), ToonDrawOptions::default());
		queue.add_mesh(&meshes.pile, &Matrix4::create_translation(1.5, 0.0, 0.3), ToonDrawOptions::default());
		queue.add_mesh_instanced(&meshes.pebble, &self.pebble_instance_list, ToonDrawOptions::default());
		let glass_colors = [[1.0, 0.4, 0.4, 1.0], [0.4, 1.0, 0.4, 1.0], [0.5, 0.6, 1.0, 1.0]];
		for (glass_index, tint_color) in glass_colors.iter().enumerate() {
			let glass_matrix = Matrix4::create_translation(-0.4 + glass_index as f64 * 0.35, 0.1, 1.6 - glass_index as f64 * 0.7);
			queue.add_mesh(&meshes.glass, &glass_matrix, ToonDrawOptions { alpha: 0.55, tint_color: *tint_color, ..ToonDrawOptions::default() });
		}

		if !self.is_graded {
			renderer.set_hdr_output(false);
			queue.draw(gl, renderer, camera);
		}
		else {
			// 실수 렌더 타겟에 그리고 색 보정을 걸어 노드 사각형에 낸다. (렌더러의 화면 사각형은 푼다)
			let width = screen_rect.size.x.round().max(1.0) as i32;
			let height = screen_rect.size.y.round().max(1.0) as i32;
			if self.post_process.is_none() {
				match ToonPostProcess::new(gl, width, height) {
					Ok(mut post_process) => {
						post_process.set_bloom(false, 0.0, 0.9, 1.6);
						post_process.set_color_grade(0.45, 1.15, [1.0, 0.8, 0.55], 0.6, [0.35, 0.3, 0.45], 0.25);
						post_process.set_vignette(0.8);
						self.post_process = Some(post_process);
					}
					Err(error) => {
						eprintln!("toonview: {}", error);
						return;
					}
				}
			}
			let post_process = match self.post_process.as_mut() {
				Some(post_process) => {
					post_process
				}
				None => {
					return;
				}
			};
			let _ = post_process.resize(gl, width, height);
			renderer.clear_screen_rect();
			renderer.set_hdr_output(true);
			post_process.begin(gl, [0.05, 0.06, 0.09]);
			queue.draw(gl, renderer, camera);
			post_process.end(gl, output.output_framebuffer, output.output_viewport);
		}
		self.frame_count += 1;
		if self.frame_count == 30 {
			let frame_statistics = renderer.get_frame_statistics();
			println!("toonview: 판 {} 그린 것 {} 거른 것 {} 드로우콜 {}", if self.is_graded { "오른쪽" } else { "왼쪽" }, frame_statistics.drawn_object_count, frame_statistics.culled_object_count, frame_statistics.draw_call_count);
		}
	}
}

//==============================================================================
// 렌더러를 짓지 못한 기기. (판에 글만, ToonView3D 의 대체 그리기 함수)
//==============================================================================
fn draw_fallback(graphic: &mut Graphic, content_size: Vector2) {
	graphic.set_fill_color("#ffffff");
	graphic.draw_fill_text("3D 를 그릴 수 없는 기기", content_size.x * 0.5, content_size.y * 0.5);
}

//==============================================================================
// 3D 점 위의 2D 표시 하나.
//==============================================================================
struct Marker {
	point: [f64; 3],
	marker_id: NodeId,
	label_id: NodeId,
}

struct ToonViewScene {
	scene_base: SceneBase,
	left_panel_id: Option<NodeId>,
	right_panel_id: Option<NodeId>,
	marker_list: Vec<Marker>,
	elapsed_seconds: Rc<Cell<f64>>,
	is_renderer_shared: bool,
}

//==============================================================================
// 색 판 노드.
//==============================================================================
fn create_panel(node_tree: &mut NodeTree, parent_id: NodeId, rect: Rect, color_text: &str) -> NodeId {
	let node_id = node_tree.create_child(parent_id, "panel");
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_local_position(rect.position);
	node.set_content_size(rect.size);
	node.add_component(Paint::create_with_color(color_text));
	node_id
}

//==============================================================================
// 3D 렌더 스케일 읽기. (환경 변수 VANILLA_RENDER_SCALE, 원본 ?renderScale, 없거나 0 이거나 수가 아니면 1)
//==============================================================================
fn read_render_scale() -> f64 {
	let render_scale = std::env::var("VANILLA_RENDER_SCALE").ok().and_then(|render_scale_text| render_scale_text.parse::<f64>().ok()).unwrap_or(1.0);
	if render_scale == 0.0 || render_scale.is_nan() {
		1.0
	}
	else {
		render_scale
	}
}

//==============================================================================
// 글 노드.
//==============================================================================
fn create_text(node_tree: &mut NodeTree, parent_id: NodeId, position: Vector2, text_value: &str, color_text: &str) -> NodeId {
	let node_id = node_tree.create_child(parent_id, "text");
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_local_position(position);
	node.set_content_size(Vector2::create(700.0, 24.0));
	let text = node.add_component(Text::create_with_text(text_value, 18.0));
	text.set_text_color(color_text);
	text.set_text_align(TextAlign::Left);
	node_id
}

impl Scene for ToonViewScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, _engine: &Engine) {
		self.scene_base.set_scene_background_color("#000000");
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let left_panel_id = create_panel(node_tree, root_id, Rect::create(40.0, 60.0, 700.0, 460.0), "#2d3a4f");
		let right_panel_id = create_panel(node_tree, root_id, Rect::create(780.0, 60.0, 460.0, 460.0), "#000000");
		for (panel_id, is_graded) in [(left_panel_id, false), (right_panel_id, true)] {
			let mut panel_drawer = PanelDrawer::new(is_graded, self.elapsed_seconds.clone());
			let toon_view = node_tree[panel_id].add_component(ToonView3D::new());
			toon_view.set_render_scale(read_render_scale());
			toon_view.set_draw_callback(Some(Box::new(move |renderer, camera, screen_rect, gl, output| {
				panel_drawer.draw(renderer, camera, screen_rect, gl, output);
			})));
			toon_view.set_fallback_draw_callback(Some(Box::new(draw_fallback)));
			toon_view.get_camera_mut().set_perspective(0.75, 0.1, 100.0);
		}
		let overlay_id = create_panel(node_tree, root_id, Rect::create(60.0, 470.0, 260.0, 36.0), "#ffcc33");
		create_text(node_tree, overlay_id, Vector2::create(8.0, 6.0), "2D 가 3D 위에", "#000000");
		create_text(node_tree, root_id, Vector2::create(40.0, 20.0), "왼쪽: 노드 사각형 안 툰, 비치는 공은 먼 것부터, 노란 점은 Camera3D::project_to_view", "#ffffff");
		create_text(node_tree, root_id, Vector2::create(780.0, 530.0), "오른쪽: ToonPostProcess 색 보정", "#ffffff");
		for (point, name) in [([-1.6, 1.15, 0.0], "상자"), ([0.2, 1.15, -0.6], "공"), ([1.85, 1.3, 0.4], "돌무더기")] {
			let marker_id = create_panel(node_tree, root_id, Rect::create(0.0, 0.0, 10.0, 10.0), "#ffee00");
			node_tree[marker_id].set_pivot(Pivot::MIDDLE_CENTER);
			let label_id = create_text(node_tree, root_id, Vector2::zero(), name, "#ffee00");
			self.marker_list.push(Marker { point, marker_id, label_id });
		}
		self.left_panel_id = Some(left_panel_id);
		self.right_panel_id = Some(right_panel_id);
	}

	//==============================================================================
	// 카메라를 돌리고, 같은 카메라로 3D 점 위에 2D 표시를 놓는다.
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let elapsed_seconds = self.elapsed_seconds.get() + time_delta;
		self.elapsed_seconds.set(elapsed_seconds);
		let eye_position = Vector3::create((elapsed_seconds * 0.2).sin() * 1.5, 4.2, 6.5);
		let target_position = Vector3::create(0.0, 0.4, 0.0);
		let up_direction = Vector3::create(0.0, 1.0, 0.0);
		let node_tree = self.scene_base.get_node_tree_mut();
		for panel_id in [self.left_panel_id, self.right_panel_id].into_iter().flatten() {
			if let Some(toon_view) = node_tree[panel_id].get_component_mut::<ToonView3D>() {
				toon_view.get_camera_mut().look_at(eye_position, target_position, up_direction);
			}
		}
		if let Some(left_panel_id) = self.left_panel_id {
			let panel_rect = Rect { position: node_tree[left_panel_id].get_local_position(), size: node_tree[left_panel_id].get_content_size() };
			let projected_list: Vec<_> = match node_tree[left_panel_id].get_component_mut::<ToonView3D>() {
				Some(toon_view) => {
					let camera = toon_view.get_camera_mut();
					camera.set_aspect_ratio(panel_rect.size.x / panel_rect.size.y);
					self.marker_list.iter().map(|marker| camera.project_to_view(marker.point[0], marker.point[1], marker.point[2], panel_rect)).collect()
				}
				None => {
					Vec::new()
				}
			};
			for (marker, projected) in self.marker_list.iter().zip(projected_list) {
				node_tree[marker.marker_id].set_local_position(Vector2::create(projected.x, projected.y));
				node_tree[marker.label_id].set_local_position(Vector2::create(projected.x - 20.0, projected.y - 30.0));
			}
		}
		self.scene_base.tick(time_delta);
	}

	//==============================================================================
	// 처음 그릴 때 렌더러 하나를 지어 두 판에 나눠 준다. (ToonView3D::set_renderer)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		if !self.is_renderer_shared {
			self.is_renderer_shared = true;
			match ToonRenderer::new(graphic.get_gl()) {
				Ok(renderer) => {
					let shared_renderer = Rc::new(RefCell::new(renderer));
					let node_tree = self.scene_base.get_node_tree_mut();
					for panel_id in [self.left_panel_id, self.right_panel_id].into_iter().flatten() {
						if let Some(toon_view) = node_tree[panel_id].get_component_mut::<ToonView3D>() {
							toon_view.set_renderer(Some(shared_renderer.clone()));
						}
					}
				}
				Err(error) => {
					eprintln!("toonview: {}", error);
				}
			}
			graphic.restore_render_state();
		}
		self.scene_base.draw_scene_background(graphic);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("노드 사각형 안 툰 3D (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let scene = ToonViewScene { scene_base: SceneBase::new(), left_panel_id: None, right_panel_id: None, marker_list: Vec::new(), elapsed_seconds: Rc::new(Cell::new(0.0)), is_renderer_shared: false };
	if let Err(error) = engine.run(Box::new(scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
