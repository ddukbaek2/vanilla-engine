//==============================================================================
// 툰 3D 예제. (MeshBuilder 의 기본 도형, GLB 모형과 뼈대 움직임을 ToonRenderer 로 그린다)
// - 바닥판 위에 상자, 둥근 상자, 구, 원뿔, 캡슐, 고리, 팔면체를 짓고 셀 셰이딩과 바깥선으로 그린다.
// - vanilla.js 예제의 soldier.glb 를 read_glb_arrays 로 읽어 첫 움직임을 write_skin_pose 로 돌린다. (텍스처는 쓰지 않고 재질 색만)
// - 둘레에 작은 바위 2 천 개를 GPU 인스턴싱 한 번으로 그리고, 화면 밖 바위는 거른다. (그린 수와 거른 수를 표준 출력에)
// - 엔진 ShadowMap 으로 그림자를 드리우고 받는다. HDR 실수 렌더 타겟에 그려 빛나는 공에 블룸을 걸고 톤매핑한다. (ToonPostProcess)
// - TOON_MODEL_PATH 에 GLB 경로를 주면 그 모형도 하나 더 놓는다. (데스크탑만)
// - 실행: cargo run --example toonshowcase
//==============================================================================
use std::f32::consts::PI;
use std::path::PathBuf;
use vanilla::{
	create_box_geometry, create_capsule_geometry, create_cylinder_geometry, create_octahedron_geometry, create_part_matrix, create_rounded_box_geometry, create_sphere_geometry, create_torus_geometry, read_glb_arrays, write_skin_pose, BlobAsset, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, Scene, SceneBase, ToonDrawOptions, ToonMesh, ToonMeshArrays, ToonOutlineOptions, ToonPartOptions, ToonRenderer, Vector2, Vector3, ShadowMap, ToonInstance, ToonPostProcess, ToonShadowInput,
	TOON_MAXIMUM_JOINTS,
};

const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

//==============================================================================
// 놓을 모형 하나.
//==============================================================================
struct PlacedToon {
	mesh: ToonMesh,
	arrays: ToonMeshArrays,
	position: [f32; 3],
	scale: f32,
	spin: f32,
}

struct ToonShowcaseScene {
	scene_base: SceneBase,
	soldier_asset: BlobAsset,
	renderer: Option<ToonRenderer>,
	shadow_map: Option<ShadowMap>,
	post_process: Option<ToonPostProcess>,
	rock_mesh: Option<ToonMesh>,
	rock_instance_list: Vec<ToonInstance>,
	frame_count: u32,
	placed_list: Vec<PlacedToon>,
	elapsed_seconds: f32,
}

impl ToonShowcaseScene {
	fn new() -> ToonShowcaseScene {
		ToonShowcaseScene {
			scene_base: SceneBase::new(),
			soldier_asset: BlobAsset::new(),
			renderer: None,
			shadow_map: None,
			post_process: None,
			rock_mesh: None,
			rock_instance_list: Vec::new(),
			frame_count: 0,
			placed_list: Vec::new(),
			elapsed_seconds: 0.0,
		}
	}

	//==============================================================================
	// 기본 도형으로 지은 배열.
	//==============================================================================
	fn build_shapes() -> Vec<(ToonMeshArrays, [f32; 3], f32)> {
		let mut shape_list = Vec::new();
		let mut ground_builder = MeshBuilder::new();
		let ground_options = ToonPartOptions { has_outline: false, ..ToonPartOptions::default() };
		ground_builder.add_part(&create_rounded_box_geometry(9.0, 0.3, 5.0, 3, 0.12), &create_part_matrix(None, [0.0, -0.15, 0.0], None, None), "#5a4a6e", ground_options);
		shape_list.push((ground_builder.read_arrays(), [0.0, 0.0, 0.0], 0.0));
		let shapes: [(&str, [f32; 3]); 7] = [("box", [-3.4, 0.0, -1.2]), ("roundedBox", [-1.7, 0.0, -1.2]), ("sphere", [0.0, 0.0, -1.4]), ("cone", [1.7, 0.0, -1.2]), ("capsule", [3.4, 0.0, -1.2]), ("torus", [-3.0, 0.0, 1.4]), ("octahedron", [3.0, 0.0, 1.4])];
		for (shape_name, position) in shapes {
			let mut builder = MeshBuilder::new();
			let options = ToonPartOptions { outline_width: 0.03, ..ToonPartOptions::default() };
			match shape_name {
				"box" => {
					builder.add_part(&create_box_geometry(0.9, 0.9, 0.9), &create_part_matrix(None, [0.0, 0.45, 0.0], None, None), "#ff8844", options);
				}
				"roundedBox" => {
					builder.add_part(&create_rounded_box_geometry(0.9, 0.9, 0.9, 3, 0.2), &create_part_matrix(None, [0.0, 0.45, 0.0], None, None), "#45b585", options);
				}
				"sphere" => {
					builder.add_part(&create_sphere_geometry(0.5, 20, 14, PI), &create_part_matrix(None, [0.0, 0.5, 0.0], None, None), "#ffcc55", ToonPartOptions { glow: 4.0, ..options });
				}
				"cone" => {
					builder.add_part(&create_cylinder_geometry(0.0, 0.5, 1.0, 20), &create_part_matrix(None, [0.0, 0.5, 0.0], None, None), "#6d7dff", options);
				}
				"capsule" => {
					builder.add_part(&create_capsule_geometry(0.3, 0.6, 6, 16), &create_part_matrix(None, [0.0, 0.6, 0.0], None, None), "#e06058", options);
				}
				"torus" => {
					builder.add_part(&create_torus_geometry(0.45, 0.15, 12, 32, PI * 2.0), &create_part_matrix(None, [0.0, 0.6, 0.0], Some([PI * 0.5, 0.0, 0.0]), None), "#78c8ff", options);
				}
				_ => {
					builder.add_part(&create_octahedron_geometry(0.5), &create_part_matrix(None, [0.0, 0.5, 0.0], None, None), "#c27bff", options);
				}
			}
			shape_list.push((builder.read_arrays(), position, 0.6));
		}
		shape_list
	}

	//==============================================================================
	// 렌더러와 모형 준비. (처음 그릴 때 한 번)
	//==============================================================================
	fn create_resources(&mut self, gl: &glow::Context) -> Result<(), String> {
		let mut renderer = ToonRenderer::new(gl)?;
		renderer.set_band_count(3);
		for (arrays, position, spin) in ToonShowcaseScene::build_shapes() {
			if let Some(mesh) = renderer.create_mesh(gl, &arrays) {
				self.placed_list.push(PlacedToon { mesh, arrays, position, scale: 1.0, spin });
			}
		}
		let outline_options = ToonOutlineOptions { width: 0.02, ..ToonOutlineOptions::default() };
		let mut model_byte_list: Vec<(Vec<u8>, [f32; 3], f32)> = Vec::new();
		if let Some(soldier_bytes) = self.soldier_asset.get_blob() {
			model_byte_list.push(((*soldier_bytes).clone(), [-1.2, 0.0, 1.2], 1.0));
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			if let Ok(model_path) = std::env::var("TOON_MODEL_PATH") {
				let model_bytes = std::fs::read(&model_path).map_err(|error| format!("{} 읽기 실패: {}", model_path, error))?;
				model_byte_list.push((model_bytes, [1.2, 0.0, 1.2], 1.0));
			}
		}
		for (model_bytes, position, scale) in model_byte_list {
			let mut arrays = read_glb_arrays(&model_bytes, None)?;
			if arrays.outline_index_data.is_empty() {
				arrays = read_glb_arrays(&model_bytes, Some(&outline_options))?;
			}
			if let Some(mesh) = renderer.create_mesh(gl, &arrays) {
				self.placed_list.push(PlacedToon { mesh, arrays, position, scale, spin: 0.3 });
			}
		}
		// 인스턴싱할 바위. (팔면체 하나, 바닥판 둘레의 띠에 2 천 개, 자리와 크기와 색을 흩는다)
		let mut rock_builder = MeshBuilder::new();
		rock_builder.add_part(&create_octahedron_geometry(0.12), &create_part_matrix(None, [0.0, 0.08, 0.0], None, Some([1.0, 0.7, 1.0])), "#c9b8a0", ToonPartOptions { outline_width: 0.012, ..ToonPartOptions::default() });
		self.rock_mesh = renderer.create_mesh(gl, &rock_builder.read_arrays());
		for rock_index in 0..2000 {
			let ratio = rock_index as f32 / 2000.0;
			let angle = ratio * PI * 2.0 * 37.0;
			let distance = 5.5 + (rock_index % 97) as f32 / 97.0 * 9.0;
			let mut model_matrix = Matrix4::create_translation(angle.cos() * distance, -0.3, angle.sin() * distance * 0.7);
			model_matrix.rotate_y(angle * 3.0);
			let scale = 0.6 + (rock_index % 13) as f32 / 13.0 * 1.4;
			model_matrix.scale(scale, scale, scale);
			let shade = 0.7 + (rock_index % 7) as f32 / 7.0 * 0.5;
			self.rock_instance_list.push(ToonInstance { model_matrix, tint_color: [shade, shade * 0.95, shade * 0.9, 1.0] });
		}
		renderer.set_hdr_output(true);
		renderer.set_light([-0.5, -1.0, -0.4], [1.6, 1.5, 1.4], [0.35, 0.4, 0.5], [0.15, 0.13, 0.12]);
		self.shadow_map = Some(ShadowMap::new(gl, 2048)?);
		self.renderer = Some(renderer);
		Ok(())
	}
}

impl Scene for ToonShowcaseScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.scene_base.set_scene_background_color("#14171e");
		self.soldier_asset.load(engine, "soldier.glb");
		self.scene_base.track_asset(&self.soldier_asset);
	}

	fn tick(&mut self, _engine: &mut Engine, time_delta: f32) {
		self.elapsed_seconds += time_delta;
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 툰 패스 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		graphic.flush_batch();
		if self.renderer.is_none() {
			if let Err(error) = self.create_resources(graphic.get_gl()) {
				eprintln!("toonshowcase: {}", error);
			}
		}
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		let aspect_ratio = canvas_pixel_size.x / canvas_pixel_size.y.max(1.0);
		let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 4.2, 7.5), Vector3::create(0.0, 0.5, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let mut view_projection_matrix = Matrix4::create_perspective(0.7, aspect_ratio, 0.1, 100.0);
		view_projection_matrix.multiply(&view_matrix);
		let elapsed_seconds = self.elapsed_seconds;
		let gl = graphic.get_gl();
		let pixel_width = canvas_pixel_size.x as i32;
		let pixel_height = canvas_pixel_size.y as i32;
		if self.post_process.is_none() {
			match ToonPostProcess::new(gl, pixel_width, pixel_height) {
				Ok(post_process) => {
					self.post_process = Some(post_process);
				}
				Err(error) => {
					eprintln!("toonshowcase: {}", error);
				}
			}
		}
		let (renderer, shadow_map, post_process) = match (self.renderer.as_mut(), self.shadow_map.as_mut(), self.post_process.as_mut()) {
			(Some(renderer), Some(shadow_map), Some(post_process)) => {
				(renderer, shadow_map, post_process)
			}
			_ => {
				graphic.restore_render_state();
				return;
			}
		};
		let _ = post_process.resize(gl, pixel_width, pixel_height);

		// 모형마다의 행렬과 자세.
		let mut model_matrix_list: Vec<Matrix4> = Vec::new();
		let mut pose_list: Vec<Option<Vec<f32>>> = Vec::new();
		for placed in self.placed_list.iter() {
			let mut model_matrix = Matrix4::create_translation(placed.position[0], placed.position[1], placed.position[2]);
			model_matrix.rotate_y(elapsed_seconds * placed.spin);
			model_matrix.scale(placed.scale, placed.scale, placed.scale);
			model_matrix_list.push(model_matrix);
			let pose = placed.arrays.skeleton.as_ref().map(|skeleton| {
				let mut joint_matrices = vec![0.0; TOON_MAXIMUM_JOINTS * 16];
				let mut animation_name_list: Vec<&String> = skeleton.animations.keys().collect();
				animation_name_list.sort();
				let walk_name = animation_name_list.iter().find(|name| name.to_lowercase().contains("walk")).or(animation_name_list.first()).map(|name| name.as_str()).unwrap_or("");
				write_skin_pose(skeleton, walk_name, elapsed_seconds, &mut joint_matrices, None, None);
				joint_matrices
			});
			pose_list.push(pose);
		}

		// 1. 그림자 깊이, 2. HDR 씬. (같은 그리기를 두 번)
		let sun_direction = renderer.get_sun_direction();
		shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(sun_direction[0], sun_direction[1], sun_direction[2]), 9.0, 0.1, 40.0);
		for pass_index in 0..2 {
			if pass_index == 0 {
				renderer.begin_shadow_pass(gl, shadow_map);
			}
			else {
				renderer.set_shadow(Some(ToonShadowInput::from_shadow_map(shadow_map)));
				post_process.begin(gl, [0.006, 0.007, 0.012]);
				renderer.begin_frame(gl, &view_projection_matrix);
			}
			for (placed_index, placed) in self.placed_list.iter().enumerate() {
				let options = ToonDrawOptions { joint_matrices: pose_list[placed_index].as_deref(), ..ToonDrawOptions::default() };
				renderer.draw_mesh(gl, &placed.mesh, &model_matrix_list[placed_index], options);
			}
			if let Some(rock_mesh) = self.rock_mesh.as_ref() {
				renderer.draw_mesh_instanced(gl, rock_mesh, &self.rock_instance_list, ToonDrawOptions::default());
			}
			if pass_index == 0 {
				renderer.end_shadow_pass(gl);
			}
		}
		let frame_statistics = renderer.get_frame_statistics();
		renderer.end_frame(gl);

		// 3. 블룸과 톤매핑을 화면에.
		post_process.end(gl, None, [0, 0, pixel_width, pixel_height]);
		self.frame_count += 1;
		if self.frame_count % 60 == 1 {
			println!("toonshowcase: 그린 것 {} 거른 것 {} 드로우콜 {}", frame_statistics.drawn_object_count, frame_statistics.culled_object_count, frame_statistics.draw_call_count);
		}
		graphic.restore_render_state();
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("툰 3D (바닐라엔진)");
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ToonShowcaseScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
