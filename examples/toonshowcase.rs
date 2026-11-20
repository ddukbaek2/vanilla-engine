//==============================================================================
// 툰 3D 예제. (MeshBuilder 의 기본 도형, GLB 모형과 뼈대 움직임을 ToonRenderer 로 그린다)
// - 바닥판 위에 상자, 둥근 상자, 구, 원뿔, 캡슐, 고리, 팔면체를 짓고 셀 셰이딩과 바깥선으로 그린다.
// - vanilla.js 예제의 soldier.glb 를 read_glb_arrays 로 읽어 첫 움직임을 write_skin_pose 로 돌린다. (텍스처는 쓰지 않고 재질 색만)
// - TOON_MODEL_PATH 에 GLB 경로를 주면 그 모형도 하나 더 놓는다. (데스크탑만)
// - 실행: cargo run --example toonshowcase
//==============================================================================
use std::f32::consts::PI;
use std::path::PathBuf;
use vanilla::{
	create_box_geometry, create_capsule_geometry, create_cylinder_geometry, create_octahedron_geometry, create_part_matrix, create_rounded_box_geometry, create_sphere_geometry, create_torus_geometry, read_glb_arrays, write_skin_pose, BlobAsset, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, Scene, SceneBase, ToonDrawOptions, ToonMesh, ToonMeshArrays, ToonOutlineOptions, ToonPartOptions, ToonRenderer, Vector2, Vector3,
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
	placed_list: Vec<PlacedToon>,
	elapsed_seconds: f32,
	joint_matrices: Vec<f32>,
}

impl ToonShowcaseScene {
	fn new() -> ToonShowcaseScene {
		ToonShowcaseScene {
			scene_base: SceneBase::new(),
			soldier_asset: BlobAsset::new(),
			renderer: None,
			placed_list: Vec::new(),
			elapsed_seconds: 0.0,
			joint_matrices: vec![0.0; TOON_MAXIMUM_JOINTS * 16],
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
					builder.add_part(&create_sphere_geometry(0.5, 20, 14, PI), &create_part_matrix(None, [0.0, 0.5, 0.0], None, None), "#ffcc55", ToonPartOptions { glow: 0.5, ..options });
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
		if let Some(renderer) = self.renderer.as_mut() {
			unsafe {
				use glow::HasContext;
				gl.viewport(0, 0, canvas_pixel_size.x as i32, canvas_pixel_size.y as i32);
				gl.clear(glow::DEPTH_BUFFER_BIT);
			}
			renderer.begin_frame(gl, &view_projection_matrix);
			for placed in self.placed_list.iter() {
				let mut model_matrix = Matrix4::create_translation(placed.position[0], placed.position[1], placed.position[2]);
				model_matrix.rotate_y(elapsed_seconds * placed.spin);
				model_matrix.scale(placed.scale, placed.scale, placed.scale);
				let mut options = ToonDrawOptions::default();
				if let Some(skeleton) = placed.arrays.skeleton.as_ref() {
					let mut animation_name_list: Vec<&String> = skeleton.animations.keys().collect();
					animation_name_list.sort();
					let walk_name = animation_name_list.iter().find(|name| name.to_lowercase().contains("walk")).or(animation_name_list.first()).map(|name| name.as_str()).unwrap_or("");
					write_skin_pose(skeleton, walk_name, elapsed_seconds, &mut self.joint_matrices, None, None);
					options.joint_matrices = Some(&self.joint_matrices);
				}
				renderer.draw_mesh(gl, &placed.mesh, &model_matrix, options);
			}
			renderer.end_frame(gl);
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
