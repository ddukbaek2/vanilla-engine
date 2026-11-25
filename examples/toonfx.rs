//==============================================================================
// 툰 화면 공간 바깥선과 3D 효과 예제. (ToonScreenOutline, ToonVfx)
// - 바닥판 위의 도형과 인스턴싱한 돌을 ToonRenderer 로 칠하고(껍데기 바깥선 없이), ToonScreenOutline 으로 실루엣, 맞닿은 곳, 꺾인 곳에 선을 긋는다.
//   카메라는 원근이고 굵기는 월드 길이 0.022 (보는 점까지의 거리에서 셈), 깊이 문턱 0.08, 맞닿음 0.015, 꺾임 60 도.
//   돌 일곱은 인스턴스마다 번호를 따로, 나머지 일곱은 한 번호로 그려 서로 맞닿은 곳의 선이 다른 것을 본다.
// - 실행: cargo run --example toonfx
//==============================================================================
use glow::HasContext;
use std::f32::consts::PI;
use vanilla::{
	create_box_geometry, create_cylinder_geometry, create_octahedron_geometry, create_part_matrix, create_rounded_box_geometry, create_sphere_geometry, create_torus_geometry, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, Scene, SceneBase, ToonDrawOptions, ToonGeometry, ToonInstance, ToonLineDrawOptions, ToonMesh, ToonOutlineWidthUnit, ToonPartOptions, ToonRenderer, ToonScreenOutline, Vector2, Vector3,
};

//==============================================================================
// 놓을 모형 하나.
//==============================================================================
struct PlacedToon {
	mesh: ToonMesh,
	position: [f32; 3],
	spin: f32,
}

struct ToonFxScene {
	scene_base: SceneBase,
	renderer: Option<ToonRenderer>,
	screen_outline: Option<ToonScreenOutline>,
	ground_mesh: Option<ToonMesh>,
	rock_mesh: Option<ToonMesh>,
	placed_list: Vec<PlacedToon>,
	rock_instance_list: Vec<ToonInstance>,
	rock_line_id_list: Vec<u32>,
	elapsed_seconds: f32,
}

impl ToonFxScene {
	fn new() -> ToonFxScene {
		ToonFxScene {
			scene_base: SceneBase::new(),
			renderer: None,
			screen_outline: None,
			ground_mesh: None,
			rock_mesh: None,
			placed_list: Vec::new(),
			rock_instance_list: Vec::new(),
			rock_line_id_list: Vec::new(),
			elapsed_seconds: 0.0,
		}
	}

	//==============================================================================
	// 도형 하나로 모형 짓기. (껍데기 바깥선 없이)
	//==============================================================================
	fn build_mesh(renderer: &ToonRenderer, gl: &glow::Context, geometry: &ToonGeometry, matrix: &Matrix4, color: &str) -> Option<ToonMesh> {
		let mut builder = MeshBuilder::new();
		builder.add_part(geometry, matrix, color, ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
		renderer.create_mesh(gl, &builder.read_arrays())
	}

	//==============================================================================
	// 렌더러, 바깥선, 모형 준비. (처음 그릴 때 한 번)
	//==============================================================================
	fn create_resources(&mut self, gl: &glow::Context) -> Result<(), String> {
		let mut renderer = ToonRenderer::new(gl)?;
		renderer.set_band_count(3);
		renderer.set_light([-0.5, -1.0, -0.35], [0.85, 0.8, 0.75], [0.55, 0.6, 0.7], [0.25, 0.22, 0.2]);
		self.ground_mesh = ToonFxScene::build_mesh(&renderer, gl, &create_box_geometry(8.0, 0.2, 5.0), &create_part_matrix(None, [0.0, -0.1, 0.0], None, None), "#c9c2b4");
		let shape_list: [(ToonGeometry, Matrix4, &str, [f32; 3], f32); 5] = [
			(create_box_geometry(1.0, 1.0, 1.0), create_part_matrix(None, [0.0, 0.5, 0.0], None, None), "#ff8844", [-2.4, 0.0, -0.8], 0.5),
			(create_rounded_box_geometry(1.0, 1.0, 1.0, 3, 0.2), create_part_matrix(None, [0.0, 0.5, 0.0], None, None), "#45b585", [-1.35, 0.0, -0.8], 0.3),
			(create_sphere_geometry(0.55, 24, 16, PI), create_part_matrix(None, [0.0, 0.55, 0.0], None, None), "#ffd257", [0.2, 0.0, -0.9], 0.0),
			(create_torus_geometry(0.45, 0.16, 12, 32, PI * 2.0), create_part_matrix(None, [0.0, 0.6, 0.0], Some([PI * 0.5, 0.0, 0.0]), None), "#78c8ff", [1.6, 0.0, -0.6], 0.7),
			(create_cylinder_geometry(0.0, 0.5, 1.0, 20), create_part_matrix(None, [0.0, 0.5, 0.0], None, None), "#8d7dff", [2.7, 0.0, -0.9], 0.0),
		];
		for (geometry, matrix, color, position, spin) in shape_list.iter() {
			if let Some(mesh) = ToonFxScene::build_mesh(&renderer, gl, geometry, matrix, color) {
				self.placed_list.push(PlacedToon { mesh, position: *position, spin: *spin });
			}
		}
		self.rock_mesh = ToonFxScene::build_mesh(&renderer, gl, &create_octahedron_geometry(0.22), &create_part_matrix(None, [0.0, 0.15, 0.0], None, Some([1.0, 0.7, 1.0])), "#a89a88");
		for rock_index in 0..14 {
			let mut model_matrix = Matrix4::create_translation(-3.0 + rock_index as f32 * 0.46, 0.0, 1.3 + (rock_index % 2) as f32 * 0.25);
			model_matrix.rotate_y(rock_index as f32 * 0.9);
			self.rock_instance_list.push(ToonInstance { model_matrix, tint_color: [1.0; 4] });
			self.rock_line_id_list.push(if rock_index < 7 { 100 + rock_index } else { 150 });
		}
		let mut screen_outline = ToonScreenOutline::new(gl)?;
		screen_outline.set_color([0.078, 0.086, 0.11, 1.0]);
		screen_outline.set_width(0.022, ToonOutlineWidthUnit::World);
		screen_outline.set_depth_threshold(0.08, 0.015);
		screen_outline.set_crease(60.0, 0.5, 2.0);
		self.screen_outline = Some(screen_outline);
		self.renderer = Some(renderer);
		Ok(())
	}
}

impl Scene for ToonFxScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color("#21242b");
	}

	fn tick(&mut self, _engine: &mut Engine, time_delta: f32) {
		self.elapsed_seconds += time_delta;
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 선 번호 → 툰 몸 → 바깥선 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		graphic.flush_batch();
		if self.renderer.is_none() {
			if let Err(error) = self.create_resources(graphic.get_gl()) {
				eprintln!("toonfx: {}", error);
			}
		}
		let gl = graphic.get_gl();
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		let aspect_ratio = canvas_pixel_size.x / canvas_pixel_size.y.max(1.0);
		let eye_position = Vector3::create(0.0, 4.2, 6.2);
		let look_position = Vector3::create(0.0, 0.4, 0.0);
		let view_matrix = Matrix4::create_look_at(eye_position, look_position, Vector3::create(0.0, 1.0, 0.0));
		let projection_matrix = Matrix4::create_perspective(0.75, aspect_ratio, 0.1, 100.0);
		let mut view_projection_matrix = projection_matrix;
		view_projection_matrix.multiply(&view_matrix);
		let (renderer, screen_outline, ground_mesh, rock_mesh) = match (self.renderer.as_mut(), self.screen_outline.as_mut(), self.ground_mesh.as_ref(), self.rock_mesh.as_ref()) {
			(Some(renderer), Some(screen_outline), Some(ground_mesh), Some(rock_mesh)) => {
				(renderer, screen_outline, ground_mesh, rock_mesh)
			}
			_ => {
				graphic.restore_render_state();
				return;
			}
		};
		let model_matrix_list: Vec<Matrix4> = self.placed_list.iter().map(|placed| {
			let mut model_matrix = Matrix4::create_translation(placed.position[0], placed.position[1], placed.position[2]);
			model_matrix.rotate_y(0.6 + placed.spin + self.elapsed_seconds * placed.spin * 0.3);
			model_matrix
		}).collect();
		let identity_matrix = Matrix4::create_identity();

		// 1. 선 번호. (바닥 1, 도형 10 ~, 돌은 인스턴스마다)
		screen_outline.set_focus_distance(eye_position.subtract(look_position).length());
		if screen_outline.begin_line_ids(gl, &view_matrix, &projection_matrix) {
			screen_outline.draw_mesh(gl, ground_mesh, &identity_matrix, 1, ToonLineDrawOptions::default());
			for (placed_index, placed) in self.placed_list.iter().enumerate() {
				screen_outline.draw_mesh(gl, &placed.mesh, &model_matrix_list[placed_index], 10 + placed_index as u32, ToonLineDrawOptions::default());
			}
			screen_outline.draw_mesh_instanced(gl, rock_mesh, &self.rock_instance_list, Some(&self.rock_line_id_list), 99, ToonLineDrawOptions::default());
			screen_outline.end_line_ids(gl);
		}

		// 2. 툰 몸. (앞 프레임의 깊이를 비우고)
		unsafe {
			gl.depth_mask(true);
			gl.clear(glow::DEPTH_BUFFER_BIT);
		}
		let no_outline = ToonDrawOptions { has_outline: false, ..ToonDrawOptions::default() };
		renderer.begin_frame(gl, &view_projection_matrix);
		renderer.draw_mesh(gl, ground_mesh, &identity_matrix, no_outline);
		for (placed_index, placed) in self.placed_list.iter().enumerate() {
			renderer.draw_mesh(gl, &placed.mesh, &model_matrix_list[placed_index], no_outline);
		}
		renderer.draw_mesh_instanced(gl, rock_mesh, &self.rock_instance_list, no_outline);
		renderer.end_frame(gl);

		// 3. 바깥선.
		screen_outline.draw_outline(gl);
		graphic.restore_render_state();
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("툰 바깥선과 효과 (바닐라엔진)");
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ToonFxScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
