//==============================================================================
// 툰 셰이딩과 바깥선 견주기 예제. (vanilla.js examples/toonstyle 과 같은 장면, 같은 값)
// - 각진 상자, 둥근 공, 큰 면 결정(팔면체), 잔 면 결정 덩이, 고리, 비스듬한 판, 바닥을 해와 그림자, 점광, 스포트라이트, 림, 하이라이트로 칠한다.
// - 그늘은 바탕, 1 차 그늘(서늘한 보라), 2 차 그늘 셋이고, 그림자는 그늘 색으로 들어간다. 결정은 매트캡을 더한다.
// - 점광(고리 곁)과 스포트라이트(상자 위)는 그림자 아틀라스로 그림자를 드리운다. 그늘 쪽에는 푸른 림, 짙은 공에는 2 번 UV 로 읽는 천사 고리(머리카락 꼴의 반사 띠).
// - 껍데기 바깥선은 8 미터에서 화면 굵기를 맞추고, 16 미터에서 사라지며, 카메라 쪽 깊이로 조금 민다.
// - 화면 공간 바깥선은 그리기 줄에 맡긴다. (ToonDrawQueue::set_screen_outline)
// - TOON_STYLE_SCALE 에 0.25 ~ 1 을 주면 선의 판을 그 몫으로 줄여 그린다. (원본 ?scale=)
// - TOON_STYLE_TOGGLES=1 이면 바닥은 선 없이 가리기만, 결정 덩이는 꺾임 선 없이 그린다. (원본 ?toggles=1)
// - TOON_STYLE_SHELL=1 이면 화면 공간 바깥선 대신 껍데기 바깥선(거리 배를 건 inverted hull, 모형마다 색과 굵기 배)으로 그린다. (원본 ?shell=1)
// - TOON_STYLE_ROLL 에 라디안을 주면 카메라를 그만큼 기울인다. 천사 고리는 머리에 붙어 있다. (원본 ?roll=)
// - 빛 세기 막기(유니티 툰 셰이더의 Limit Light Intensity)는 기본대로 켠다.
// - 실행: cargo run --example toonstyle (화면 견주기는 VANILLA_CAPTURE_PATH 로 숨긴 창에서 찍는다)
//==============================================================================
use glow::HasContext;
use vanilla::{
	create_box_geometry, create_faceted_chunk_geometry, create_octahedron_geometry, create_sphere_geometry, create_torus_geometry, Camera3D, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, Scene, SceneBase, ShadowAtlas, ShadowMap, ToonDrawOptions, DEFAULT_SHADOW_ATLAS_RESOLUTION, DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION, ToonDrawQueue, ToonGeometry, ToonMatcapBlend, ToonMesh, ToonPartOptions, ToonPointLight,
	ToonRenderer, ToonScreenOutline, ToonShade, ToonTextureOptions, Vector2, Vector3,
};

// 해가 비추는 방향. (빛이 나아가는 쪽)
const SUN_DIRECTION: [f64; 3] = [-0.45, -1.0, -0.55];

// 결정 덩이 꼭짓점 열둘의 반지름 배.
const CHUNK_SCALES: [f64; 12] = [1.0, 0.85, 1.1, 0.95, 1.05, 0.9, 1.0, 1.12, 0.88, 1.0, 0.95, 1.08];

//==============================================================================
// 도형 하나로 지은 모형. (has_shell 이면 껍데기 바깥선도)
//==============================================================================
fn build_mesh(renderer: &ToonRenderer, gl: &glow::Context, geometry: &ToonGeometry, color_text: &str, has_shell: bool) -> Result<ToonMesh, String> {
	let mut geometry = geometry.clone();
	if !geometry.uvs.is_empty() && geometry.uvs1.is_empty() {
		geometry.uvs1 = geometry.uvs.clone();
	}
	let mut builder = MeshBuilder::new();
	builder.add_part(&geometry, &Matrix4::create_identity(), color_text, ToonPartOptions { has_outline: has_shell, outline_width: 0.025, ..ToonPartOptions::default() });
	renderer.create_mesh(gl, &builder.read_arrays()).ok_or_else(|| String::from("모형을 짓지 못했다."))
}

//==============================================================================
// 매트캡 그림. (64x64, 왼쪽 위의 밝은 점과 둘레로 갈수록 엷어지는 빛, 더하기로 쓴다)
//==============================================================================
fn create_matcap_pixels() -> Vec<u8> {
	let size = 64usize;
	let mut pixels = vec![0u8; size * size * 4];
	for pixel_y in 0..size {
		for pixel_x in 0..size {
			let normal_x = (pixel_x as f64 + 0.5) / size as f64 * 2.0 - 1.0;
			let normal_y = 1.0 - (pixel_y as f64 + 0.5) / size as f64 * 2.0;
			let spot = (1.0 - ((normal_x + 0.35) * (normal_x + 0.35) + (normal_y - 0.4) * (normal_y - 0.4)).sqrt() / 0.32).max(0.0);
			let glow = (1.0 - (normal_x * normal_x + normal_y * normal_y).sqrt()).max(0.0) * 0.25;
			let value = ((spot.powf(1.5) + glow) * 255.0).round().min(255.0) as u8;
			let offset = (pixel_y * size + pixel_x) * 4;
			pixels[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
		}
	}
	pixels
}

//==============================================================================
// 천사 고리 그림. (64x64, 가로로 긴 밝은 띠 하나, v 0.3 둘레, 가운데가 가장 밝고 양옆으로 엷어짐)
//==============================================================================
fn create_angel_ring_pixels() -> Vec<u8> {
	let size = 64usize;
	let mut pixels = vec![0u8; size * size * 4];
	for pixel_y in 0..size {
		for pixel_x in 0..size {
			let u = (pixel_x as f64 + 0.5) / size as f64;
			let v = (pixel_y as f64 + 0.5) / size as f64;
			let band = (1.0 - (v - 0.3).abs() / 0.06).max(0.0);
			let across = (1.0 - (u - 0.5).abs() / 0.4).max(0.0);
			let value = (band * across * 255.0).round().min(255.0) as u8;
			let offset = (pixel_y * size + pixel_x) * 4;
			pixels[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
		}
	}
	pixels
}

//==============================================================================
// 그릴 것 하나. (모형, 행렬, 그리기 꼴의 값)
//==============================================================================
struct StyleItem {
	mesh: ToonMesh,
	matrix: Matrix4,
	line_id: u32,
	has_screen_outline: bool,
	has_crease_line: bool,
	specular_scale: f64,
	rim_scale: f64,
	has_matcap: bool,
	outline_color: Option<[f64; 4]>,
	outline_width_scale: f64,
	has_outline: bool,
	has_angel_ring: bool,
	outline_offset_z: f64,
}

//==============================================================================
// 그릴 것 하나 짓기. (그리기 꼴은 기본값)
//==============================================================================
fn create_item(mesh: ToonMesh, matrix: Matrix4, line_id: u32) -> StyleItem {
	StyleItem { mesh, matrix, line_id, has_screen_outline: true, has_crease_line: true, specular_scale: 1.0, rim_scale: 1.0, has_matcap: false, outline_color: None, outline_width_scale: 1.0, has_outline: true, has_angel_ring: false, outline_offset_z: 0.0 }
}

//==============================================================================
// 자리, 돌림(y 다음 x), 크기의 행렬.
//==============================================================================
fn create_placed(translation: [f64; 3], rotation_x: f64, rotation_y: f64, scale: [f64; 3]) -> Matrix4 {
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
	is_shell: bool,
	shadow_map: ShadowMap,
	shadow_atlas: ShadowAtlas,
	matcap_texture: glow::Texture,
	angel_ring_texture: glow::Texture,
	item_list: Vec<StyleItem>,
}

impl StyleResources {
	//==============================================================================
	// 짓기. (선의 판 몫, 모형마다 바깥선 꼴을 바꿔 보일지, 껍데기 바깥선으로 그릴지)
	//==============================================================================
	fn new(gl: &glow::Context, line_scale: f64, is_toggled: bool, is_shell: bool) -> Result<StyleResources, String> {
		let mut renderer = ToonRenderer::new(gl)?;
		renderer.set_sun_direction(SUN_DIRECTION);
		renderer.set_shade(ToonShade { step: 0.5, feather: 0.02, color: [0.55, 0.52, 0.72], second_step: 0.2, second_feather: 0.02, second_color: [0.38, 0.35, 0.56], is_half_lambert: true });
		renderer.set_rim_light([0.42, 0.48, 0.62], 2.6, 0.1, 0.6);
		renderer.set_specular(0.6, 56.0, 0.0, [1.0; 3]);
		renderer.set_shade_rim_light([0.18, 0.3, 0.62], 2.2);
		renderer.set_point_lights(&[
			ToonPointLight { position: [0.9, 1.2, 2.6], color: [1.0, 0.55, 0.25], intensity: 2.0, range: 3.5, casts_shadow: true, ..ToonPointLight::default() },
			ToonPointLight { position: [-2.6, 3.4, 2.4], color: [0.45, 0.7, 1.0], intensity: 6.0, range: 7.0, direction: Some([0.25, -1.0, -0.6]), inner_cone_angle: 0.2, outer_cone_angle: 0.38, casts_shadow: true, ..ToonPointLight::default() },
		]);
		renderer.set_outline_distance_scale(8.0, 0.5, 3.0);
		renderer.set_outline_fade_distance(9.0, 16.0);
		let mut shadow_atlas = ShadowAtlas::new(gl, DEFAULT_SHADOW_ATLAS_RESOLUTION, DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION)?;
		shadow_atlas.update_lights(&renderer.get_light_set().read_shadow_light_sources());
		renderer.set_local_shadows(Some(&shadow_atlas), 1.0);
		let angel_ring_texture = renderer.create_texture(gl, 64, 64, &create_angel_ring_pixels(), ToonTextureOptions { is_mipmapped: false, is_repeated: false, ..ToonTextureOptions::default() })?;
		let matcap_texture = renderer.create_texture(gl, 64, 64, &create_matcap_pixels(), ToonTextureOptions { is_mipmapped: false, is_repeated: false, ..ToonTextureOptions::default() })?;
		let mut outline = ToonScreenOutline::new(gl)?;
		outline.set_resolution_scale(line_scale);
		let mut shadow_map = ShadowMap::new(gl, 2048)?;
		shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(SUN_DIRECTION[0], SUN_DIRECTION[1], SUN_DIRECTION[2]), 7.0, 0.5, 40.0);
		renderer.set_shadow(Some(&shadow_map), 1.0);
		let mesh_of = |geometry: ToonGeometry, color_text: &str| build_mesh(&renderer, gl, &geometry, color_text, is_shell);
		let item_list = vec![
			StyleItem { has_screen_outline: !is_toggled, has_outline: false, specular_scale: 0.0, rim_scale: 0.0, ..create_item(mesh_of(create_box_geometry(14.0, 0.2, 10.0), "#9aa4b4")?, Matrix4::create_translation(0.0, -0.1, 0.0), 1) },
			StyleItem { specular_scale: 0.0, ..create_item(mesh_of(create_box_geometry(2.4, 0.1, 3.0), "#86a07e")?, create_placed([-3.4, 0.55, -1.6], 0.5, 0.25, [1.0, 1.0, 1.0]), 2) },
			create_item(mesh_of(create_box_geometry(1.2, 1.2, 1.2), "#e8a860")?, create_placed([-2.0, 0.6, 0.7], 0.0, 0.55, [1.0, 1.0, 1.0]), 3),
			StyleItem { has_angel_ring: true, outline_offset_z: 0.02, ..create_item(mesh_of(create_sphere_geometry(0.8, 48, 32, std::f64::consts::PI), "#4a5a8a")?, Matrix4::create_translation(-0.2, 0.8, 0.2), 4) },
			StyleItem { has_matcap: true, outline_color: Some([0.1, 0.22, 0.4, 1.0]), ..create_item(mesh_of(create_octahedron_geometry(0.6), "#80c8ff")?, create_placed([1.5, 0.96, 0.6], 0.0, 0.4, [1.0, 1.6, 1.0]), 5) },
			StyleItem { has_crease_line: !is_toggled, ..create_item(mesh_of(create_faceted_chunk_geometry(0.55, &CHUNK_SCALES, [1.0, 0.8, 1.0]), "#b0a0d0")?, Matrix4::create_translation(3.0, 0.45, 0.2), 6) },
			StyleItem { outline_color: Some([0.35, 0.08, 0.12, 1.0]), outline_width_scale: 1.4, ..create_item(mesh_of(create_torus_geometry(0.45, 0.17, 16, 40, std::f64::consts::PI * 2.0), "#d06070")?, create_placed([0.9, 0.3, 2.1], 1.2, 0.3, [1.0, 1.0, 1.0]), 7) },
		];
		Ok(StyleResources { renderer, outline, is_shell, shadow_map, shadow_atlas, matcap_texture, angel_ring_texture, item_list })
	}

	//==============================================================================
	// 한 프레임 그리기. (그림자 → 그리기 줄, 그리기 줄이 선의 판과 화면 공간 바깥선을 스스로)
	//==============================================================================
	fn draw(&mut self, gl: &glow::Context, camera: &Camera3D) {
		let mut queue = ToonDrawQueue::new();
		if !self.is_shell {
			queue.set_screen_outline(Some(&mut self.outline));
		}
		for item in self.item_list.iter() {
			queue.add_mesh(
				&item.mesh,
				&item.matrix,
				ToonDrawOptions {
					line_id: Some(item.line_id),
					has_screen_outline: item.has_screen_outline,
					has_crease_line: item.has_crease_line,
					specular_scale: item.specular_scale,
					rim_scale: item.rim_scale,
					matcap_texture: if item.has_matcap { Some(self.matcap_texture) } else { None },
					matcap_strength: 0.7,
					matcap_blend: ToonMatcapBlend::Add,
					outline_color: item.outline_color,
					outline_width_scale: item.outline_width_scale,
					has_outline: item.has_outline,
					outline_offset_z: item.outline_offset_z,
					angel_ring_texture: if item.has_angel_ring { Some(self.angel_ring_texture) } else { None },
					angel_ring_color: [0.9, 0.85, 0.7],
					angel_ring_offset_v: 0.3,
					..ToonDrawOptions::default()
				},
			);
		}
		queue.draw_shadows(gl, &mut self.renderer, &self.shadow_map);
		queue.draw_local_shadows(gl, &mut self.renderer, &self.shadow_atlas);
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
	line_scale: f64,
	is_toggled: bool,
	is_shell: bool,
}

impl ToonStyleScene {
	//==============================================================================
	// 생성. (선의 판 몫과 모형별 꼴은 데스크탑 환경 변수에서)
	//==============================================================================
	fn new() -> ToonStyleScene {
		let mut camera = Camera3D::new();
		camera.set_perspective(0.7, 0.1, 100.0);
		camera.look_at(Vector3::create(0.0, 3.4, 7.8), Vector3::create(0.0, 0.55, 0.3), create_rolled_up([0.0, 3.4, 7.8], [0.0, 0.55, 0.3], read_environment_number("TOON_STYLE_ROLL", 0.0)));
		ToonStyleScene { scene_base: SceneBase::new(), resources: None, camera, line_scale: read_line_scale(), is_toggled: read_environment_flag("TOON_STYLE_TOGGLES"), is_shell: read_environment_flag("TOON_STYLE_SHELL") }
	}
}

//==============================================================================
// 선의 판 몫. (데스크탑은 TOON_STYLE_SCALE, 웹은 1)
//==============================================================================
fn read_line_scale() -> f64 {
	read_environment_number("TOON_STYLE_SCALE", 1.0)
}

//==============================================================================
// 환경 변수의 수. (데스크탑만, 없거나 웹이면 기본값)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_environment_number(name: &str, default_value: f64) -> f64 {
	std::env::var(name).ok().and_then(|text| text.parse::<f64>().ok()).unwrap_or(default_value)
}

#[cfg(target_arch = "wasm32")]
fn read_environment_number(_name: &str, default_value: f64) -> f64 {
	default_value
}

//==============================================================================
// 기울인 카메라의 위쪽. (보는 쪽 둘레로 위쪽을 roll 만큼 돌린다, 오른쪽이 위로 들린다, 원본 createRolledUp)
//==============================================================================
fn create_rolled_up(eye: [f64; 3], target: [f64; 3], roll: f64) -> Vector3 {
	let forward = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
	let forward_length = (forward[0] * forward[0] + forward[1] * forward[1] + forward[2] * forward[2]).sqrt();
	let front = [forward[0] / forward_length, forward[1] / forward_length, forward[2] / forward_length];
	let right_raw = [-front[2], 0.0, front[0]];
	let right_length = (right_raw[0] * right_raw[0] + right_raw[2] * right_raw[2]).sqrt();
	let right = [right_raw[0] / right_length, right_raw[1] / right_length, right_raw[2] / right_length];
	let up = [right[1] * front[2] - right[2] * front[1], right[2] * front[0] - right[0] * front[2], right[0] * front[1] - right[1] * front[0]];
	let cosine = roll.cos();
	let sine = roll.sin();
	Vector3::create(up[0] * cosine - right[0] * sine, up[1] * cosine - right[1] * sine, up[2] * cosine - right[2] * sine)
}

//==============================================================================
// 환경 변수 켬. (데스크탑은 이름=1 이면 true, 웹은 끔)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_environment_flag(name: &str) -> bool {
	std::env::var(name).map(|text| text == "1").unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
fn read_environment_flag(_name: &str) -> bool {
	false
}

impl Scene for ToonStyleScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (원본 생성자처럼 렌더러, 그림자, 모형을 엔진의 그래픽으로 짓는다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.scene_base.set_scene_background_color("#202633");
		if let Some(graphic) = engine.get_graphic() {
			match StyleResources::new(graphic.get_gl(), self.line_scale, self.is_toggled, self.is_shell) {
				Ok(resources) => {
					self.resources = Some(resources);
				}
				Err(error) => {
					eprintln!("toonstyle: {}", error);
				}
			}
		}
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 깊이 비우기 → 툰 그리기 줄 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		graphic.flush_batch();
		let gl = graphic.get_gl();
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
