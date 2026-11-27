//==============================================================================
// 툰 확장 자리와 빛 예제. (ToonMaterial 셰이더 조각, 텍스처와 데칼, 점광, 반 램버트 띠, 더하기 홀로그램, 평면 거울, 영구 인스턴스 버퍼)
// - 왼쪽 판: 머티리얼의 표면 조각이 UV 로 칠 획 숫자를 그린다. (게임 고유 무늬는 조각으로 두는 자리, 숫자는 0.5 초마다 바뀜)
// - 상자: 바탕색 텍스처(바둑판)와 데칼 묶음 그림의 한 칸(고리와 십자)을 덮고 그 색으로 빛난다.
// - 고리: 그 그리기만 다른 빛 묶음(반 램버트 4 단 띠 0.3725 / 0.6471 / 0.8627 / 1.0)으로 칠한다.
// - 오른쪽 공: 둘레를 도는 점광 셋(감쇠 (1 - 거리 / 범위)^2)과 하이라이트, 림 빛, 채움 빛을 받는다. (기본 두 단계 그늘, 하이라이트와 림은 부드러운 꼴)
// - 홀로그램 둘: 더하기 섞기와 소프트 엣지. 왼쪽은 깊이 선패스 없이(겹친 곳이 두 겹), 오른쪽은 한 덩어리로(묶음과 깊이 선패스).
// - 바닥은 비치게 두고, 그리기 줄의 불투명 앞 단계에서 바닥에 비친 모습(create_reflection_matrix, 감기는 렌더러가 뒤집음)을 어둡게(color_scale) 그린다.
// - 뒤의 바위 27 개는 영구 인스턴스 버퍼 하나(꼴: 돌림과 크기 4, 자리 3, 색 3)를 머티리얼 정점 조각이 읽어 놓는다.
// - vanilla.js 시험 페이지와 같은 장면, 같은 값이다. TOON_MATERIAL_FRAME 에 프레임 번호를 주면 그 시각으로 멈춰 그린다. (데스크탑, 화면 견주기)
// - 실행: cargo run --example toonmaterial
//==============================================================================
use std::f64::consts::PI;

use glow::HasContext;
use vanilla::{
	create_box_geometry, create_capsule_geometry, create_octahedron_geometry, create_reflection_matrix, create_sphere_geometry, create_torus_geometry, Camera3D, Engine, EngineConfiguration, GlContext, Graphic, Matrix4, MeshBuilder, Scene, SceneBase, ShaderProgram, ToonBlendMode, ToonBufferUsage, ToonDrawOptions,
	ToonDrawQueue, ToonDrawStage, ToonInstanceBuffer, ToonLightSet, ToonMaterial, ToonMaterialChunks, ToonMesh, ToonPartOptions, ToonPointLight, ToonProgramKind, ToonRenderer, ToonTextureOptions, Vector2, Vector3,
};

// 칠 획 숫자의 유니폼과 함수. (vanilla.js 시험 페이지와 같은 글)
const SEGMENT_CHUNK: &str = "uniform float segmentMask;
uniform vec3 segmentColor;
float readSegmentDistance(vec2 point, vec2 start, vec2 end) {
	vec2 along = end - start;
	float ratio = clamp(dot(point - start, along) / dot(along, along), 0.0, 1.0);
	return length(point - start - along * ratio);
}
float readSegmentInk(vec2 point) {
	vec2 starts[7] = vec2[7](vec2(-0.4, 0.9), vec2(0.45, 0.82), vec2(0.45, -0.08), vec2(-0.4, -0.9), vec2(-0.45, -0.08), vec2(-0.45, 0.82), vec2(-0.4, 0.0));
	vec2 ends[7] = vec2[7](vec2(0.4, 0.9), vec2(0.45, 0.08), vec2(0.45, -0.82), vec2(0.4, -0.9), vec2(-0.45, -0.82), vec2(-0.45, 0.08), vec2(0.4, 0.0));
	int mask = int(segmentMask + 0.5);
	float ink = 0.0;
	for (int index = 0; index < 7; ++index) {
		float strength = ((mask >> index) & 1) == 1 ? 1.0 : 0.12;
		float distance = readSegmentDistance(point, starts[index], ends[index]);
		ink = max(ink, (1.0 - smoothstep(0.07, 0.1, distance)) * strength);
	}
	return ink;
}
";

// 칠 획 숫자를 칠하는 표면 조각.
const SEGMENT_SURFACE_CHUNK: &str = "void applySurface(inout ToonSurface surface) {
	vec2 point = vec2((surface.uv.x - 0.5) * 1.6, 0.5 - surface.uv.y) * 2.2;
	float ink = readSegmentInk(point);
	surface.baseColor = mix(surface.baseColor, segmentColor, ink);
	surface.emission += toLinear(segmentColor) * ink * 1.5;
}
";

// 바위 인스턴스를 놓는 정점 조각. (돌림의 cos, sin, 가로 크기, 세로 크기와 자리, 색)
const ROCK_VERTEX_CHUNK: &str = "void applyVertex(inout ToonVertex vertex) {
	float scaleAcross = instanceShape.z;
	float scaleUp = instanceShape.w;
	vertex.modelMatrix = mat4(vec4(instanceShape.x * scaleAcross, 0.0, -instanceShape.y * scaleAcross, 0.0), vec4(0.0, scaleUp, 0.0, 0.0), vec4(instanceShape.y * scaleAcross, 0.0, instanceShape.x * scaleAcross, 0.0), vec4(instanceOffset, 1.0));
	vertex.color *= instanceColor;
}
";

// GL 텍스처. (데스크탑과 웹의 glow 텍스처)
type GlTexture = <glow::Context as HasContext>::Texture;

// 숫자마다 켜는 획. (a b c d e f g 가 비트 0 ~ 6)
const DIGIT_SEGMENT_MASKS: [u32; 10] = [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f];
// 바위 수. (3 줄 x 9)
const ROCK_COUNT: usize = 27;

//==============================================================================
// 도형 하나로 지은 배열.
//==============================================================================
fn build_mesh(renderer: &ToonRenderer, gl: &glow::Context, geometry: &vanilla::ToonGeometry, matrix: &Matrix4, color_text: &str, options: ToonPartOptions) -> Option<ToonMesh> {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, matrix, color_text, options);
	renderer.create_mesh(gl, &builder.read_arrays())
}

//==============================================================================
// 바둑판 64x64 RGBA.
//==============================================================================
fn create_checker_pixels() -> Vec<u8> {
	let size = 64;
	let mut data = vec![0u8; size * size * 4];
	for y in 0..size {
		for x in 0..size {
			let is_light = ((x >> 3) + (y >> 3)) % 2 == 0;
			let offset = (y * size + x) * 4;
			data[offset..offset + 4].copy_from_slice(&if is_light { [235, 225, 210, 255] } else { [120, 140, 170, 255] });
		}
	}
	data
}

//==============================================================================
// 데칼 묶음 128x64 RGBA. (왼쪽 칸 고리, 오른쪽 칸 십자, 흰색에 알파)
//==============================================================================
fn create_decal_pixels() -> Vec<u8> {
	let (width, height) = (128usize, 64usize);
	let mut data = vec![0u8; width * height * 4];
	for y in 0..height {
		for x in 0..width {
			let local_x = ((x % 64) as f64 + 0.5) / 64.0 - 0.5;
			let local_y = (y as f64 + 0.5) / 64.0 - 0.5;
			let is_covered = if x < 64 {
				let radius = (local_x * local_x + local_y * local_y).sqrt();
				radius > 0.2 && radius < 0.32
			}
			else {
				(local_x.abs() < 0.07 && local_y.abs() < 0.32) || (local_y.abs() < 0.07 && local_x.abs() < 0.32)
			};
			let offset = (y * width + x) * 4;
			data[offset..offset + 4].copy_from_slice(&[255, 255, 255, if is_covered { 255 } else { 0 }]);
		}
	}
	data
}


//==============================================================================
// 장면의 메시와 텍스처. (씬을 만들 때 한 번)
//==============================================================================
struct MaterialResources {
	renderer: ToonRenderer,
	band_light_set: ToonLightSet,
	floor: ToonMesh,
	screen: ToonMesh,
	cube: ToonMesh,
	torus: ToonMesh,
	sphere: ToonMesh,
	hologram_ball: ToonMesh,
	hologram_body: ToonMesh,
	rock: ToonMesh,
	segment_material: ToonMaterial,
	rock_material: ToonMaterial,
	rock_buffer: ToonInstanceBuffer,
	checker_texture: GlTexture,
	decal_texture: GlTexture,
}

impl MaterialResources {
	fn new(gl: &glow::Context) -> Result<MaterialResources, String> {
		let mut renderer = ToonRenderer::new(gl)?;
		renderer.set_light([-0.4, -1.0, -0.3], [0.9, 0.85, 0.8], [0.32, 0.36, 0.46], [0.14, 0.12, 0.11]);
		renderer.set_fill_light([0.7, -0.2, 0.6], [0.12, 0.1, 0.16]);
		renderer.set_rim_light([0.22, 0.28, 0.4], 3.0, 1.0, 0.0);
		renderer.set_specular(0.35, 48.0, 1.0, [1.0; 3]);
		let mut band_light_set = renderer.create_light_set();
		band_light_set.set_light([-0.4, -1.0, -0.3], [0.9, 0.85, 0.8], [0.32, 0.36, 0.46], [0.14, 0.12, 0.11]);
		band_light_set.set_band_ramp(&[0.25, 0.5, 0.75], &[0.3725, 0.6471, 0.8627, 1.0], true);
		let identity_matrix = Matrix4::create_identity();
		let default_options = ToonPartOptions::default();
		let unlit_options = ToonPartOptions { is_unlit: true, has_outline: false, ..ToonPartOptions::default() };
		let missing = || String::from("메시를 짓지 못했다");
		let floor = build_mesh(&renderer, gl, &create_box_geometry(9.0, 0.2, 6.0), &Matrix4::create_translation(0.0, -0.1, 0.0), "#3a4660", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() }).ok_or_else(missing)?;
		let screen = build_mesh(&renderer, gl, &create_box_geometry(1.6, 1.0, 0.12), &identity_matrix, "#1a2230", default_options).ok_or_else(missing)?;
		let cube = build_mesh(&renderer, gl, &create_box_geometry(1.1, 1.1, 1.1), &identity_matrix, "#ffffff", default_options).ok_or_else(missing)?;
		let torus = build_mesh(&renderer, gl, &create_torus_geometry(0.5, 0.2, 16, 40, PI * 2.0), &identity_matrix, "#e0a050", default_options).ok_or_else(missing)?;
		let sphere = build_mesh(&renderer, gl, &create_sphere_geometry(0.55, 32, 20, PI), &identity_matrix, "#d8d8e0", default_options).ok_or_else(missing)?;
		let hologram_ball = build_mesh(&renderer, gl, &create_sphere_geometry(0.42, 24, 16, PI), &identity_matrix, "#ffffff", unlit_options).ok_or_else(missing)?;
		let hologram_body = build_mesh(&renderer, gl, &create_capsule_geometry(0.28, 0.7, 8, 16), &identity_matrix, "#ffffff", unlit_options).ok_or_else(missing)?;
		let rock = build_mesh(&renderer, gl, &create_octahedron_geometry(0.22), &identity_matrix, "#a89880", default_options).ok_or_else(missing)?;
		let segment_material = renderer.create_material(ToonMaterialChunks { name: String::from("segment"), uniform_chunk: String::from(SEGMENT_CHUNK), surface_chunk: String::from(SEGMENT_SURFACE_CHUNK), ..ToonMaterialChunks::default() });
		let rock_layout = renderer.create_instance_layout(&[("instanceShape", 4), ("instanceOffset", 3), ("instanceColor", 3)])?;
		let rock_material = renderer.create_material(ToonMaterialChunks { name: String::from("rock"), vertex_chunk: String::from(ROCK_VERTEX_CHUNK), ..ToonMaterialChunks::default() });
		let mut rock_data: Vec<f32> = Vec::new();
		for row in 0..3usize {
			for column in 0..9usize {
				let angle = (row * 9 + column) as f64 * 0.7;
				let scale_across = 0.8 + ((row * 5 + column * 3) % 4) as f64 * 0.15;
				let scale_up = 0.7 + ((row * 3 + column) % 3) as f64 * 0.3;
				let shade = 0.75 + ((row + column * 2) % 3) as f64 * 0.12;
				rock_data.extend_from_slice(&[angle.cos(), angle.sin(), scale_across, scale_up, -3.6 + column as f64 * 0.9, 0.2 * scale_up, -2.3 - row as f64 * 0.45, shade, shade * 0.95, shade * 0.9].map(|value| value as f32));
			}
		}
		let rock_buffer = renderer.create_instance_buffer(gl, &rock_layout, &rock_data, ToonBufferUsage::Static)?;
		let checker_texture = renderer.create_texture(gl, 64, 64, &create_checker_pixels(), ToonTextureOptions { is_filtered: false, ..ToonTextureOptions::default() })?;
		let decal_texture = renderer.create_texture(gl, 128, 64, &create_decal_pixels(), ToonTextureOptions { is_repeated: false, ..ToonTextureOptions::default() })?;
		Ok(MaterialResources { renderer, band_light_set, floor, screen, cube, torus, sphere, hologram_ball, hologram_body, rock, segment_material, rock_material, rock_buffer, checker_texture, decal_texture })
	}
}

//==============================================================================
// 멈춰 그릴 프레임. (데스크탑은 TOON_MATERIAL_FRAME, 웹은 없음)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn read_fixed_frame() -> Option<u32> {
	std::env::var("TOON_MATERIAL_FRAME").ok().and_then(|text| text.parse::<u32>().ok())
}

#[cfg(target_arch = "wasm32")]
fn read_fixed_frame() -> Option<u32> {
	None
}

struct ToonMaterialScene {
	scene_base: SceneBase,
	resources: Option<MaterialResources>,
	camera: Camera3D,
	frame_index: u32,
	fixed_frame: Option<u32>,
}

impl ToonMaterialScene {
	fn new() -> ToonMaterialScene {
		let mut camera = Camera3D::new();
		camera.set_perspective(0.8, 0.1, 100.0);
		camera.look_at(Vector3::create(0.0, 3.6, 8.2), Vector3::create(0.0, 0.5, -0.4), Vector3::create(0.0, 1.0, 0.0));
		ToonMaterialScene { scene_base: SceneBase::new(), resources: None, camera, frame_index: 0, fixed_frame: read_fixed_frame() }
	}

	//==============================================================================
	// 한 프레임 그리기. (time 초의 자리)
	//==============================================================================
	fn draw_scene(resources: &mut MaterialResources, camera: &Camera3D, gl: &glow::Context, time: f64) {
		let mut point_light_list = Vec::new();
		let light_color_list = [[1.0, 0.35, 0.3], [0.3, 1.0, 0.45], [0.35, 0.5, 1.0]];
		for (index, color) in light_color_list.iter().enumerate() {
			let angle = time * 0.8 + index as f64 * PI * 2.0 / 3.0;
			point_light_list.push(ToonPointLight { position: [2.6 + angle.cos() * 1.4, 1.1, -0.2 + angle.sin() * 1.4], color: *color, intensity: 2.2, range: 3.2, decay: 0.0, range_exponent: 1.0, ..ToonPointLight::default() });
		}
		resources.renderer.set_point_lights(&point_light_list);
		let digit = ((time * 2.0).floor() as usize) % 10;
		let screen_matrix = Matrix4::create_translation(-2.7, 0.75, 0.3);
		let mut cube_matrix = Matrix4::create_translation(-0.9, 0.55, 0.5);
		cube_matrix.rotate_y(time * 0.5);
		let mut torus_matrix = Matrix4::create_translation(0.9, 0.62, 0.6);
		torus_matrix.rotate_x(0.55);
		torus_matrix.rotate_y(time * 0.4);
		let sphere_matrix = Matrix4::create_translation(2.6, 0.55, -0.2);
		let segment_draw = move |program: &ShaderProgram, gl: &GlContext, kind: ToonProgramKind| {
			if kind == ToonProgramKind::Solid {
				unsafe {
					gl.uniform_1_f32(program.get_uniform_location(gl, "segmentMask").as_ref(), (DIGIT_SEGMENT_MASKS[digit] as f64) as f32);
					gl.uniform_3_f32(program.get_uniform_location(gl, "segmentColor").as_ref(), 0.35, 1.0, 0.85);
				}
			}
		};
		let opaque_list: [(&ToonMesh, Matrix4, ToonDrawOptions); 4] = [
			(&resources.screen, screen_matrix, ToonDrawOptions { material: Some(&resources.segment_material), on_before_draw: Some(&segment_draw), ..ToonDrawOptions::default() }),
			(&resources.cube, cube_matrix, ToonDrawOptions { base_color_texture: Some(resources.checker_texture), uv_transform: [0.0, 0.0, 2.0, 2.0], decal_texture: Some(resources.decal_texture), decal_uv_transform: [if digit % 2 == 0 { 0.0 } else { 0.5 }, 0.0, 0.5, 1.0], decal_color: [1.0, 0.85, 0.2], decal_emission: 0.6, ..ToonDrawOptions::default() }),
			(&resources.torus, torus_matrix, ToonDrawOptions { light_set: Some(&resources.band_light_set), ..ToonDrawOptions::default() }),
			(&resources.sphere, sphere_matrix, ToonDrawOptions::default()),
		];
		let mut queue = ToonDrawQueue::new();
		// 평면 거울: 바닥(y = 0)에 비친 불투명한 것을 불투명 앞에 어둡게. (비친 행렬은 행렬식이 음수라 렌더러가 감기를 뒤집는다)
		let reflection_matrix = create_reflection_matrix([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
		let opaque_reference = &opaque_list;
		queue.add_pass(ToonDrawStage::BeforeOpaque, move |gl, pass_renderer| {
			for (mesh, matrix, options) in opaque_reference.iter() {
				let mut mirrored_matrix = reflection_matrix;
				mirrored_matrix.multiply(matrix);
				pass_renderer.draw_mesh(gl, mesh, &mirrored_matrix, ToonDrawOptions { color_scale: 0.3, has_outline: false, ..*options });
			}
		});
		for (mesh, matrix, options) in opaque_list.iter() {
			queue.add_mesh(mesh, matrix, *options);
		}
		let rock = &resources.rock;
		let rock_buffer = &resources.rock_buffer;
		let rock_material = &resources.rock_material;
		queue.add_pass(ToonDrawStage::AfterOpaque, move |gl, pass_renderer| {
			pass_renderer.draw_mesh_instance_buffer(gl, rock, rock_buffer, ROCK_COUNT, ToonDrawOptions { material: Some(rock_material), ..ToonDrawOptions::default() });
		});
		queue.add_mesh(&resources.floor, &Matrix4::create_identity(), ToonDrawOptions { alpha: 0.72, ..ToonDrawOptions::default() });
		// 홀로그램 둘: 왼쪽은 깊이 선패스 없이(겹친 곳이 두 겹), 오른쪽은 한 덩어리.
		for (group, offset_x, is_depth_prepassed) in [(1u32, -0.5f64, false), (2, 0.7, true)] {
			let flicker = 0.85 + 0.15 * (time * 9.0 + group as f64).sin();
			let options = ToonDrawOptions { blend_mode: ToonBlendMode::Additive, soft_edge: 2.5, tint_color: [0.3, 0.9, 1.0, 0.7], tint_amount: Some(1.0), has_outline: false, group: Some(group), is_depth_prepassed, alpha: flicker, ..ToonDrawOptions::default() };
			queue.add_mesh(&resources.hologram_ball, &Matrix4::create_translation(offset_x, 1.55, -1.3), options);
			queue.add_mesh(&resources.hologram_body, &Matrix4::create_translation(offset_x, 1.0, -1.3), options);
		}
		queue.draw(gl, &mut resources.renderer, camera);
	}
}

impl Scene for ToonMaterialScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (원본 생성자처럼 렌더러, 메시, 텍스처를 엔진의 그래픽으로 짓는다)
	//==============================================================================
	fn create(&mut self, engine: &Engine) {
		self.scene_base.set_scene_background_color("#10121a");
		if let Some(graphic) = engine.get_graphic() {
			match MaterialResources::new(graphic.get_gl()) {
				Ok(resources) => {
					self.resources = Some(resources);
				}
				Err(error) => {
					eprintln!("toonmaterial: {}", error);
				}
			}
		}
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 깊이 비우기 → 툰 그리기 줄 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		graphic.flush_batch();
		let gl = graphic.get_gl();
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		self.camera.set_aspect_ratio(canvas_pixel_size.x / canvas_pixel_size.y.max(1.0));
		let frame = self.fixed_frame.unwrap_or(self.frame_index);
		if let Some(resources) = self.resources.as_mut() {
			unsafe {
				gl.clear_depth_f32(1.0);
				gl.clear(glow::DEPTH_BUFFER_BIT);
			}
			ToonMaterialScene::draw_scene(resources, &self.camera, gl, frame as f64 / 60.0);
			if self.frame_index % 60 == 1 {
				let frame_statistics = resources.renderer.get_frame_statistics();
				println!("toonmaterial: 그린 것 {} 드로우콜 {}", frame_statistics.drawn_object_count, frame_statistics.draw_call_count);
			}
		}
		self.frame_index += 1;
		graphic.restore_render_state();
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("툰 확장 자리와 빛 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ToonMaterialScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
