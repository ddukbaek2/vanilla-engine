//==============================================================================
// 툰 셰이딩과 바깥선 견주기 예제. (vanilla.js examples/toonstyle 과 같은 장면, 같은 값)
// - 각진 상자, 둥근 공, 큰 면 결정(팔면체), 잔 면 결정 덩이, 고리, 비스듬한 판, 바닥을 해와 그림자, 점광, 림, 정반사로 칠한다.
// - 화면 공간 바깥선은 그리기 줄에 맡긴다. (ToonDrawQueue::set_screen_outline)
// - TOON_STYLE_SCALE 에 0.25 ~ 1 을 주면 선의 판을 그 몫으로 줄여 그린다. (원본 ?scale=)
// - TOON_STYLE_TOGGLES=1 이면 바닥은 선 없이 가리기만, 결정 덩이는 꺾임 선 없이 그린다. (원본 ?toggles=1)
// - 실행: cargo run --example toonstyle (화면 견주기는 VANILLA_CAPTURE_PATH 로 숨긴 창에서 찍는다)
//==============================================================================
use glow::HasContext;
use vanilla::{
	create_box_geometry, create_faceted_chunk_geometry, create_octahedron_geometry, create_sphere_geometry, create_torus_geometry, Camera3D, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, Scene, SceneBase, ShadowMap, ToonDrawOptions, ToonDrawQueue, ToonGeometry, ToonMesh, ToonPartOptions, ToonPointLight, ToonRenderer,
	ToonScreenOutline, Vector2, Vector3,
};

// 해가 비추는 방향. (빛이 나아가는 쪽)
const SUN_DIRECTION: [f32; 3] = [-0.45, -1.0, -0.55];

// 결정 덩이 꼭짓점 열둘의 반지름 배.
const CHUNK_SCALES: [f32; 12] = [1.0, 0.85, 1.1, 0.95, 1.05, 0.9, 1.0, 1.12, 0.88, 1.0, 0.95, 1.08];

//==============================================================================
// 도형 하나로 지은 모형. (껍데기 바깥선 없이)
//==============================================================================
fn build_mesh(renderer: &ToonRenderer, gl: &glow::Context, geometry: &ToonGeometry, color_text: &str) -> Result<ToonMesh, String> {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, &Matrix4::create_identity(), color_text, ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
	renderer.create_mesh(gl, &builder.read_arrays()).ok_or_else(|| String::from("모형을 짓지 못했다."))
}

//==============================================================================
// 자리, 돌림(y 다음 x), 크기의 행렬.
//==============================================================================
fn create_placed(translation: [f32; 3], rotation_x: f32, rotation_y: f32, scale: [f32; 3]) -> Matrix4 {
	let mut matrix = Matrix4::create_translation(translation[0], translation[1], translation[2]);
	matrix.rotate_y(rotation_y);
	matrix.rotate_x(rotation_x);
	matrix.scale(scale[0], scale[1], scale[2]);
	matrix
}

//==============================================================================
// 장면이 드는 GL 자원.
//==============================================================================
struct StyleResources {
	renderer: ToonRenderer,
	outline: ToonScreenOutline,
	shadow_map: ShadowMap,
	item_list: Vec<(ToonMesh, Matrix4, u32, bool, bool)>,
}

impl StyleResources {
	//==============================================================================
	// 짓기. (선의 판 몫, 모형마다 바깥선 꼴을 바꿔 보일지)
	//==============================================================================
	fn new(gl: &glow::Context, line_scale: f32, is_toggled: bool) -> Result<StyleResources, String> {
		let mut renderer = ToonRenderer::new(gl)?;
		renderer.set_light(SUN_DIRECTION, [1.0, 0.95, 0.88], [0.42, 0.48, 0.6], [0.18, 0.16, 0.14]);
		renderer.set_rim_light([0.35, 0.4, 0.5], 3.0);
		renderer.set_specular(0.5, 40.0);
		renderer.set_point_lights(&[ToonPointLight { position: [0.9, 1.2, 2.6], color: [1.0, 0.55, 0.25], intensity: 2.0, range: 3.5, ..ToonPointLight::default() }]);
		let mut outline = ToonScreenOutline::new(gl)?;
		outline.set_resolution_scale(line_scale);
		let mut shadow_map = ShadowMap::new(gl, 2048)?;
		shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(SUN_DIRECTION[0], SUN_DIRECTION[1], SUN_DIRECTION[2]), 7.0, 0.5, 40.0);
		renderer.set_shadow(Some(&shadow_map), 1.0);
		let item_list = vec![
			(build_mesh(&renderer, gl, &create_box_geometry(14.0, 0.2, 10.0), "#9aa4b4")?, Matrix4::create_translation(0.0, -0.1, 0.0), 1, !is_toggled, true),
			(build_mesh(&renderer, gl, &create_box_geometry(2.4, 0.1, 3.0), "#86a07e")?, create_placed([-3.4, 0.55, -1.6], 0.5, 0.25, [1.0, 1.0, 1.0]), 2, true, true),
			(build_mesh(&renderer, gl, &create_box_geometry(1.2, 1.2, 1.2), "#e8a860")?, create_placed([-2.0, 0.6, 0.7], 0.0, 0.55, [1.0, 1.0, 1.0]), 3, true, true),
			(build_mesh(&renderer, gl, &create_sphere_geometry(0.8, 48, 32, std::f32::consts::PI), "#f2f0ec")?, Matrix4::create_translation(-0.2, 0.8, 0.2), 4, true, true),
			(build_mesh(&renderer, gl, &create_octahedron_geometry(0.6), "#80c8ff")?, create_placed([1.5, 0.96, 0.6], 0.0, 0.4, [1.0, 1.6, 1.0]), 5, true, true),
			(build_mesh(&renderer, gl, &create_faceted_chunk_geometry(0.55, &CHUNK_SCALES, [1.0, 0.8, 1.0]), "#b0a0d0")?, Matrix4::create_translation(3.0, 0.45, 0.2), 6, true, !is_toggled),
			(build_mesh(&renderer, gl, &create_torus_geometry(0.45, 0.17, 16, 40, std::f32::consts::PI * 2.0), "#d06070")?, create_placed([0.9, 0.3, 2.1], 1.2, 0.3, [1.0, 1.0, 1.0]), 7, true, true),
		];
		Ok(StyleResources { renderer, outline, shadow_map, item_list })
	}

	//==============================================================================
	// 한 프레임 그리기. (그림자 → 그리기 줄, 그리기 줄이 선의 판과 바깥선을 스스로)
	//==============================================================================
	fn draw(&mut self, gl: &glow::Context, camera: &Camera3D) {
		let mut queue = ToonDrawQueue::new();
		queue.set_screen_outline(Some(&mut self.outline));
		for (mesh, matrix, line_id, has_screen_outline, has_crease_line) in self.item_list.iter() {
			queue.add_mesh(mesh, matrix, ToonDrawOptions { line_id: Some(*line_id), has_screen_outline: *has_screen_outline, has_crease_line: *has_crease_line, ..ToonDrawOptions::default() });
		}
		queue.draw_shadows(gl, &mut self.renderer, &self.shadow_map);
		queue.draw(gl, &mut self.renderer, camera);
	}
}

//==============================================================================
// 견주기 장면.
//==============================================================================
struct ToonStyleScene {
	scene_base: SceneBase,
	resources: Option<StyleResources>,
	camera: Camera3D,
	line_scale: f32,
	is_toggled: bool,
}

impl ToonStyleScene {
	//==============================================================================
	// 생성. (선의 판 몫과 모형별 꼴은 데스크탑 환경 변수에서)
	//==============================================================================
	fn new() -> ToonStyleScene {
		let mut camera = Camera3D::new();
		camera.set_perspective(0.7, 0.1, 100.0);
		camera.look_at(Vector3::create(0.0, 3.4, 7.8), Vector3::create(0.0, 0.55, 0.3), Vector3::create(0.0, 1.0, 0.0));
		ToonStyleScene { scene_base: SceneBase::new(), resources: None, camera, line_scale: read_line_scale(), is_toggled: read_is_toggled() }
	}
}

//==============================================================================
// 선의 판 몫. (데스크탑은 TOON_STYLE_SCALE, 웹은 1)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_line_scale() -> f32 {
	std::env::var("TOON_STYLE_SCALE").ok().and_then(|text| text.parse::<f32>().ok()).unwrap_or(1.0)
}

#[cfg(target_arch = "wasm32")]
fn read_line_scale() -> f32 {
	1.0
}

//==============================================================================
// 모형마다 바깥선 꼴을 바꿔 보일지. (데스크탑은 TOON_STYLE_TOGGLES=1, 웹은 끔)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_is_toggled() -> bool {
	std::env::var("TOON_STYLE_TOGGLES").map(|text| text == "1").unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
fn read_is_toggled() -> bool {
	false
}

impl Scene for ToonStyleScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color("#202633");
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 깊이 비우기 → 툰 그리기 줄 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		graphic.flush_batch();
		let gl = graphic.get_gl();
		if self.resources.is_none() {
			match StyleResources::new(gl, self.line_scale, self.is_toggled) {
				Ok(resources) => {
					self.resources = Some(resources);
				}
				Err(error) => {
					eprintln!("toonstyle: {}", error);
				}
			}
		}
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		self.camera.set_aspect_ratio(canvas_pixel_size.x / canvas_pixel_size.y.max(1.0));
		if let Some(resources) = self.resources.as_mut() {
			unsafe {
				gl.clear_depth_f32(1.0);
				gl.clear(glow::DEPTH_BUFFER_BIT);
			}
			resources.draw(gl, &self.camera);
		}
		graphic.restore_render_state();
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("툰 셰이딩과 바깥선 견주기 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ToonStyleScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
