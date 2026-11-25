//==============================================================================
// 3D 모델 보기 예제. (데스크탑, 원본 examples/ancientmountain 의 3D 경로를 엔진 루프 안에서 돌린다)
// - 원본 예제의 soldier.glb, fox.glb(노멀 없음), Remy.fbx + anims/idle.fbx, walk.fbx 를 BlobAsset 으로 읽는다.
//   애셋 폴더 후보에 원본 저장소의 절대 경로를 넣는다. (읽기만 한다)
// - 땅은 코드로 만든 GLB 다. (정적 메시 → 합성 스킨 경로)
// - 원본 3D 씬은 자체 rAF 를 돌리지만, 여기서는 씬의 pre_draw 에서 그린다:
//   섀도우 깊이 패스 → 씬 패스(RenderTarget) → 블룸 → 합성(화면) → graphic.restore_render_state() → draw 에서 2D HUD.
// - 모델은 tick 에서 update 한다. (update 는 GL 을 쓰지 않는다) GL 리소스는 첫 pre_draw 에서 만든다. (create 에는 GL 이 없다)
// - 실행: cargo run --release --example modelviewer (갱신 600번 뒤 스스로 끝난다, 60fps 면 10초)
//   화면 저장: VANILLA_CAPTURE_PATH=shots/model_{frame}.png VANILLA_CAPTURE_FRAMES=60,180 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::path::PathBuf;
use vanilla::{
	AmbientOcclusionEffect, BlobAsset, BloomEffect, Color, Engine, EngineConfiguration, FullscreenPass, Graphic, Matrix4, Rect, RenderTarget, Scene, SceneBase, ShadowMap, SkinnedModel, SkinnedModelRenderer, SubsurfaceScatteringEffect, TextAlign, TextBaseline, Vector3,
};
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 원본 예제 애셋 폴더. (vanilla.js 저장소를 이 저장소 옆에 받아 둔 것으로 본다. 읽기만 한다)
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

// 섀도우 맵 해상도.
const SHADOWMAP_RESOLUTION: i32 = 2048;

// 스스로 끝내는 시각. (초)
const EXIT_SECONDS: f32 = 10.0;

// 갱신 걸음. (화면 저장이 프레임 번호로 같은 장면을 잡도록 프레임마다 1/60초씩 고정으로 나아간다)
const FIXED_TIME_STEP: f32 = 1.0 / 60.0;

// 병사 클립 순서와 바꾸는 간격. (초)
const SOLDIER_CLIP_NAMES: [&str; 3] = ["Idle", "Walk", "Run"];
const SOLDIER_CLIP_SECONDS: f32 = 2.5;

// 합성 패스. (씬 + 블룸 가산 + 약한 비네트)
const COMPOSITE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform float bloomStrength;
out vec4 outputColor;
void main() {
	vec3 sceneColor = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	vec3 bloomColor = texture(bloomTexture, fragmentTextureCoordinate).rgb;
	vec2 centered = fragmentTextureCoordinate - 0.5;
	float vignette = 1.0 - dot(centered, centered) * 0.6;
	outputColor = vec4((sceneColor + bloomColor * bloomStrength) * vignette, 1.0);
}";

//==============================================================================
// 그려 둘 모델 하나. (모델 + 놓을 자리 + 크기 배율)
//==============================================================================
struct PlacedModel {
	skinned_model: SkinnedModel,
	position_x: f32,
	position_z: f32,
	rotation_y: f32,
	scale: f32,
	label: String,
}

impl PlacedModel {
	//==============================================================================
	// 모델 행렬. (이동 x Y 회전 x 크기)
	//==============================================================================
	fn create_model_matrix(&self) -> Matrix4 {
		let mut model_matrix = Matrix4::create_translation(self.position_x, 0.0, self.position_z);
		model_matrix.rotate_y(self.rotation_y);
		model_matrix.scale(self.scale, self.scale, self.scale);
		model_matrix
	}
}

//==============================================================================
// 첫 pre_draw 에서 만드는 GL 리소스.
//==============================================================================
struct ViewerResources {
	skinned_model_renderer: SkinnedModelRenderer,
	ground_model: SkinnedModel,
	placed_model_list: Vec<PlacedModel>,
	shadow_map: ShadowMap,
	scene_render_target: RenderTarget,
	bloom_effect: BloomEffect,
	composite_pass: FullscreenPass,
	ambient_occlusion_effect: AmbientOcclusionEffect,
	subsurface_scattering_effect: SubsurfaceScatteringEffect,
	is_first_frame_checked: bool,
}

//==============================================================================
// 모델 보기 씬.
//==============================================================================
struct ModelViewerScene {
	scene_base: SceneBase,
	soldier_asset: BlobAsset,
	fox_asset: BlobAsset,
	remy_asset: BlobAsset,
	idle_animation_asset: BlobAsset,
	walk_animation_asset: BlobAsset,
	resources: Option<ViewerResources>,
	load_error_message: Option<String>,
	elapsed_time: f32,
	soldier_clip_index: usize,
	remy_clip_name: String,
	load_seconds: f32,
}

impl ModelViewerScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> ModelViewerScene {
		ModelViewerScene {
			scene_base: SceneBase::new(),
			soldier_asset: BlobAsset::new(),
			fox_asset: BlobAsset::new(),
			remy_asset: BlobAsset::new(),
			idle_animation_asset: BlobAsset::new(),
			walk_animation_asset: BlobAsset::new(),
			resources: None,
			load_error_message: None,
			elapsed_time: 0.0,
			soldier_clip_index: 0,
			remy_clip_name: String::from("Idle"),
			load_seconds: 0.0,
		}
	}

	//==============================================================================
	// GL 리소스와 모델 만들기. (BlobAsset 바이트 → 렌더러, 모델, 그림자, 렌더 타겟, 이펙트)
	//==============================================================================
	fn create_resources(&self, gl: &glow::Context) -> Result<ViewerResources, String> {
		let mut skinned_model_renderer = SkinnedModelRenderer::new(gl, None, None)?;
		let sun_direction = Vector3::create(0.5, 0.8, 0.35).normalize();
		skinned_model_renderer.set_sun_direction(sun_direction.x, sun_direction.y, sun_direction.z);
		skinned_model_renderer.set_fog_color(0.62, 0.72, 0.86);
		skinned_model_renderer.set_fog_range(25.0, 80.0);

		let read_bytes = |blob_asset: &BlobAsset, label: &str| blob_asset.get_blob().ok_or_else(|| format!("{} 읽기 실패", label));
		let mut placed_model_list = Vec::new();

		// 병사. (GLB, 인덱스 + 노멀맵)
		let soldier_bytes = read_bytes(&self.soldier_asset, "soldier.glb")?;
		let mut soldier_model = SkinnedModel::load_from_bytes(gl, &soldier_bytes, &skinned_model_renderer)?;
		soldier_model.play_animation(SOLDIER_CLIP_NAMES[0], 0.0);
		placed_model_list.push(PlacedModel { scale: measure_model_scale(&mut soldier_model, 1.8), skinned_model: soldier_model, position_x: -1.0, position_z: 0.0, rotation_y: std::f32::consts::PI + 0.4, label: String::from("soldier.glb") });

		// 여우. (GLB, 노멀 없음 → 스무스 노멀, 인덱스 없음)
		let fox_bytes = read_bytes(&self.fox_asset, "fox.glb")?;
		let mut fox_model = SkinnedModel::load_from_bytes(gl, &fox_bytes, &skinned_model_renderer)?;
		fox_model.play_animation("Walk", 0.0);
		placed_model_list.push(PlacedModel { scale: measure_model_scale(&mut fox_model, 0.7), skinned_model: fox_model, position_x: 0.1, position_z: 1.1, rotation_y: 1.2, label: String::from("fox.glb") });

		// 레미. (FBX + 애니메이션 FBX 두 개를 본 이름으로 붙인다)
		let remy_bytes = read_bytes(&self.remy_asset, "Remy.fbx")?;
		let mut remy_model = SkinnedModel::load_from_fbx_bytes(gl, &remy_bytes, &skinned_model_renderer)?;
		remy_model.add_animations_from_fbx_bytes(&read_bytes(&self.idle_animation_asset, "idle.fbx")?, "Idle", false)?;
		remy_model.add_animations_from_fbx_bytes(&read_bytes(&self.walk_animation_asset, "walk.fbx")?, "Walk", true)?;
		remy_model.play_animation("Idle", 0.0);
		placed_model_list.push(PlacedModel { scale: measure_model_scale(&mut remy_model, 1.75), skinned_model: remy_model, position_x: 1.0, position_z: -0.2, rotation_y: -0.4, label: String::from("Remy.fbx") });

		// 땅. (코드로 만든 GLB, 스킨 없는 정적 메시)
		let mut ground_model = SkinnedModel::load_from_bytes(gl, &build_ground_glb(6.0, [0.46, 0.52, 0.4]), &skinned_model_renderer)?;
		ground_model.update(0.0);

		// 그림자, 씬 렌더 타겟, 블룸, 합성.
		let shadow_map = ShadowMap::new(gl, SHADOWMAP_RESOLUTION)?;
		let scene_render_target = RenderTarget::new(gl, 2, 2, true, None)?;
		let mut bloom_effect = BloomEffect::new(gl)?;
		bloom_effect.set_threshold(0.7, 0.95);
		let composite_pass = FullscreenPass::new(gl, COMPOSITE_FRAGMENTSHADER_SOURCE)?;

		// 쓰지는 않지만 셰이더가 데스크탑 GL 에서 컴파일되는지 본다.
		let mut ambient_occlusion_effect = AmbientOcclusionEffect::new(gl)?;
		ambient_occlusion_effect.resize(gl, 64, 64)?;
		let mut subsurface_scattering_effect = SubsurfaceScatteringEffect::new(gl)?;
		subsurface_scattering_effect.resize(gl, 64, 64)?;

		Ok(ViewerResources {
			skinned_model_renderer,
			ground_model,
			placed_model_list,
			shadow_map,
			scene_render_target,
			bloom_effect,
			composite_pass,
			ambient_occlusion_effect,
			subsurface_scattering_effect,
			is_first_frame_checked: false,
		})
	}

	//==============================================================================
	// 3D 그리기. (그림자 → 씬 → 블룸 → 합성, 2D 상태는 호출자가 되돌린다)
	//==============================================================================
	fn render_scene(resources: &mut ViewerResources, gl: &glow::Context, drawing_buffer_width: i32, drawing_buffer_height: i32, elapsed_time: f32) -> Result<(), String> {
		resources.scene_render_target.resize(gl, drawing_buffer_width, drawing_buffer_height)?;
		resources.bloom_effect.resize(gl, drawing_buffer_width, drawing_buffer_height)?;

		// 카메라. (천천히 도는 궤도)
		let orbit_angle = 0.35 + elapsed_time * 0.2;
		let camera_position = Vector3::create(orbit_angle.sin() * 4.6, 1.7, orbit_angle.cos() * 4.6);
		let camera_target = Vector3::create(0.0, 0.8, 0.2);
		let view_matrix = Matrix4::create_look_at(camera_position, camera_target, Vector3::up());
		let aspect_ratio = drawing_buffer_width as f32 / drawing_buffer_height.max(1) as f32;
		let mut view_projection_matrix = Matrix4::create_perspective(std::f32::consts::PI / 3.5, aspect_ratio, 0.1, 100.0);
		view_projection_matrix.multiply(&view_matrix);
		let model_matrix_list: Vec<Matrix4> = resources.placed_model_list.iter().map(|placed_model| placed_model.create_model_matrix()).collect();
		let identity_matrix = Matrix4::create_identity();

		// 1. 섀도우 깊이 패스.
		let sun_direction = Vector3::create(0.5, 0.8, 0.35).normalize();
		let light_travel_direction = sun_direction.multiply_scalar(-1.0);
		resources.shadow_map.update_light_matrix(Vector3::create(0.0, 0.8, 0.2), light_travel_direction, 4.0, 0.5, 30.0);
		let light_view_projection_elements = *resources.shadow_map.get_light_view_projection_matrix().get_elements();
		resources.shadow_map.begin_render(gl);
		unsafe {
			gl.disable(glow::BLEND);
			gl.disable(glow::CULL_FACE);
		}
		for (placed_model, model_matrix) in resources.placed_model_list.iter_mut().zip(model_matrix_list.iter()) {
			resources.skinned_model_renderer.draw_depth(gl, &mut placed_model.skinned_model, &light_view_projection_elements, model_matrix.get_elements());
		}
		let shadow_depth_texture = resources.shadow_map.get_depth_texture();
		resources.skinned_model_renderer.set_shadow_map(Some(shadow_depth_texture), Some(&light_view_projection_elements), 1.0 / SHADOWMAP_RESOLUTION as f32);
		resources.skinned_model_renderer.set_shadow_strength(1.0);

		// 2. 씬 패스. (렌더 타겟에 하늘색으로 비우고 땅과 모델)
		resources.scene_render_target.bind(gl);
		unsafe {
			gl.clear_color(0.62, 0.72, 0.86, 1.0);
			gl.depth_mask(true);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
			gl.enable(glow::DEPTH_TEST);
			gl.enable(glow::CULL_FACE);
		}
		resources.skinned_model_renderer.draw(gl, &mut resources.ground_model, view_projection_matrix.get_elements(), identity_matrix.get_elements(), camera_position.x, camera_position.y, camera_position.z);
		for (placed_model, model_matrix) in resources.placed_model_list.iter_mut().zip(model_matrix_list.iter()) {
			resources.skinned_model_renderer.draw(gl, &mut placed_model.skinned_model, view_projection_matrix.get_elements(), model_matrix.get_elements(), camera_position.x, camera_position.y, camera_position.z);
		}
		unsafe {
			gl.disable(glow::CULL_FACE);
		}

		// 3. 블룸.
		let scene_color_texture = resources.scene_render_target.get_color_texture(0);
		resources.bloom_effect.render(gl, scene_color_texture);

		// 4. 합성 → 화면.
		let composite_pass = &resources.composite_pass;
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.viewport(0, 0, drawing_buffer_width, drawing_buffer_height);
		}
		composite_pass.use_program(gl);
		unsafe {
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), 0.8);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, resources.bloom_effect.get_result_texture());
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, scene_color_texture);
		}
		composite_pass.draw(gl);
		unsafe {
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}

		// 첫 프레임에 GL 오류를 한 번 본다.
		if !resources.is_first_frame_checked {
			resources.is_first_frame_checked = true;
			let error_code = unsafe {
				gl.get_error()
			};
			println!("modelviewer: 첫 3D 프레임 GL 오류 코드 0x{:X}", error_code);
			let _ = (resources.ambient_occlusion_effect.get_result_texture(), resources.subsurface_scattering_effect.get_result_texture());
		}
		Ok(())
	}
}

impl Scene for ModelViewerScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (모델 바이트 요청, 데스크탑은 그 자리에서 읽힌다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.soldier_asset.load(engine, "soldier.glb");
		self.fox_asset.load(engine, "fox.glb");
		self.remy_asset.load(engine, "Remy.fbx");
		self.idle_animation_asset.load(engine, "anims/idle.fbx");
		self.walk_animation_asset.load(engine, "anims/walk.fbx");
		self.scene_base.track_asset(&self.soldier_asset);
		self.scene_base.track_asset(&self.fox_asset);
		self.scene_base.track_asset(&self.remy_asset);
		self.scene_base.track_asset(&self.idle_animation_asset);
		self.scene_base.track_asset(&self.walk_animation_asset);
	}

	//==============================================================================
	// 갱신. (클립 바꾸기 + 모델 포즈 계산, 정해진 시각에 끝내기)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, _time_delta: f32) {
		let delta_time = FIXED_TIME_STEP;
		self.elapsed_time += delta_time;
		if self.elapsed_time >= EXIT_SECONDS {
			engine.exit_application();
		}
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		let soldier_clip_index = ((self.elapsed_time / SOLDIER_CLIP_SECONDS) as usize) % SOLDIER_CLIP_NAMES.len();
		if soldier_clip_index != self.soldier_clip_index {
			self.soldier_clip_index = soldier_clip_index;
			resources.placed_model_list[0].skinned_model.play_animation(SOLDIER_CLIP_NAMES[soldier_clip_index], 0.3);
		}
		let remy_clip_name = if ((self.elapsed_time / 2.0) as usize).is_multiple_of(2) {
			"Idle"
		}
		else {
			"Walk"
		};
		if remy_clip_name != self.remy_clip_name {
			self.remy_clip_name = remy_clip_name.to_string();
			resources.placed_model_list[2].skinned_model.play_animation(remy_clip_name, 0.3);
		}
		for placed_model in resources.placed_model_list.iter_mut() {
			placed_model.skinned_model.update(delta_time);
		}
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 3D 패스 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		graphic.flush_batch();
		if self.resources.is_none() && self.load_error_message.is_none() {
			let load_start_instant = std::time::Instant::now();
			match self.create_resources(graphic.get_gl()) {
				Ok(resources) => {
					self.load_seconds = load_start_instant.elapsed().as_secs_f32();
					println!("modelviewer: 모델 준비 {:.2}초", self.load_seconds);
					for placed_model in resources.placed_model_list.iter() {
						println!("modelviewer: {} 드로어블 {} 노드 {} 클립 {:?} 배율 {:.4}", placed_model.label, placed_model.skinned_model.get_drawable_list().len(), placed_model.skinned_model.get_node_list().len(), placed_model.skinned_model.get_animation_name_list(), placed_model.scale);
					}
					self.resources = Some(resources);
				}
				Err(error) => {
					eprintln!("modelviewer: {}", error);
					self.load_error_message = Some(error);
				}
			}
		}
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		let elapsed_time = self.elapsed_time;
		if let Some(resources) = &mut self.resources {
			if let Err(error) = ModelViewerScene::render_scene(resources, graphic.get_gl(), canvas_pixel_size.x as i32, canvas_pixel_size.y as i32, elapsed_time) {
				eprintln!("modelviewer: {}", error);
			}
		}
		graphic.restore_render_state();
	}

	//==============================================================================
	// 2D HUD 출력. (3D 위에 반투명 판과 글)
	//==============================================================================
	fn draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		engine.get_view_manager().apply_canvas_native_rect(graphic);
		let mut line_list = vec![String::from("바닐라엔진 graphics3d modelviewer")];
		match &self.resources {
			Some(resources) => {
				line_list.push(format!("soldier.glb  {}", SOLDIER_CLIP_NAMES[self.soldier_clip_index]));
				line_list.push(String::from("fox.glb  Walk (smooth normals)"));
				line_list.push(format!("Remy.fbx  {} (anims/*.fbx retarget)", self.remy_clip_name));
				line_list.push(format!("shadow {} + bloom, load {:.1}s, t {:.1}s", resources.shadow_map.get_resolution(), self.load_seconds, self.elapsed_time));
			}
			None => {
				line_list.push(self.load_error_message.clone().unwrap_or_else(|| String::from("loading...")));
			}
		}
		graphic.set_fill_color(Color::create(0.0, 0.0, 0.0, 0.55));
		graphic.draw_rect(Rect::create(12.0, 12.0, 430.0, 18.0 + line_list.len() as f32 * 22.0));
		graphic.set_font_string("16px default");
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		for (line_index, line) in line_list.iter().enumerate() {
			let text_color = if line_index == 0 {
				Color::create(1.0, 0.85, 0.45, 1.0)
			}
			else {
				Color::white()
			};
			graphic.set_fill_color(text_color);
			graphic.draw_fill_text(line, 24.0, 32.0 + line_index as f32 * 22.0);
		}
	}
}

//==============================================================================
// 모델 크기 배율 구하기. (바인드/첫 포즈에서 노드 월드 위치의 가장 높은 Y 를 target_height 로 맞춘다)
//==============================================================================
fn measure_model_scale(skinned_model: &mut SkinnedModel, target_height: f32) -> f32 {
	skinned_model.update(0.0);
	let maximum_height = skinned_model.get_node_list().iter().map(|node| node.world_matrix.elements[13]).fold(0.0f32, f32::max);
	if maximum_height > 0.0 {
		target_height / maximum_height
	}
	else {
		1.0
	}
}

//==============================================================================
// 땅 GLB 만들기. (가로세로 2 x half_size 사각형, 위쪽 노멀, 16비트 인덱스, 단색 머티리얼)
//==============================================================================
fn build_ground_glb(half_size: f32, base_color: [f32; 3]) -> Vec<u8> {
	let position_values = [-half_size, 0.0, -half_size, -half_size, 0.0, half_size, half_size, 0.0, half_size, half_size, 0.0, -half_size];
	let normal_values = [0.0f32, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];
	let index_values = [0u16, 1, 2, 0, 2, 3];
	let mut binary_bytes: Vec<u8> = Vec::new();
	binary_bytes.extend(position_values.iter().flat_map(|value| value.to_le_bytes()));
	binary_bytes.extend(normal_values.iter().flat_map(|value| value.to_le_bytes()));
	binary_bytes.extend(index_values.iter().flat_map(|value| value.to_le_bytes()));
	while !binary_bytes.len().is_multiple_of(4) {
		binary_bytes.push(0);
	}
	let json_text = format!(
		r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"name":"Ground","mesh":0}}],
		"materials":[{{"name":"ground","pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},1],"metallicFactor":0,"roughnessFactor":1}}}}],
		"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2,"material":0}}]}}],
		"accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}}],
		"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":48}},{{"buffer":0,"byteOffset":96,"byteLength":12}}],
		"buffers":[{{"byteLength":{}}}]}}"#,
		base_color[0], base_color[1], base_color[2], binary_bytes.len()
	);
	let mut json_bytes = json_text.into_bytes();
	while !json_bytes.len().is_multiple_of(4) {
		json_bytes.push(b' ');
	}
	let total_length = 12 + 8 + json_bytes.len() + 8 + binary_bytes.len();
	let mut glb_bytes = Vec::with_capacity(total_length);
	glb_bytes.extend_from_slice(&0x46546c67u32.to_le_bytes());
	glb_bytes.extend_from_slice(&2u32.to_le_bytes());
	glb_bytes.extend_from_slice(&(total_length as u32).to_le_bytes());
	glb_bytes.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
	glb_bytes.extend_from_slice(&0x4e4f534au32.to_le_bytes());
	glb_bytes.extend_from_slice(&json_bytes);
	glb_bytes.extend_from_slice(&(binary_bytes.len() as u32).to_le_bytes());
	glb_bytes.extend_from_slice(&0x004e4942u32.to_le_bytes());
	glb_bytes.extend_from_slice(&binary_bytes);
	glb_bytes
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("바닐라엔진 graphics3d modelviewer");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.window_size = vanilla::Vector2::create(1280.0, 720.0);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ModelViewerScene::new())) {
		eprintln!("{}", error);
	}
}
