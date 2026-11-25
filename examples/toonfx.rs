//==============================================================================
// 툰 화면 공간 바깥선과 3D 효과 예제. (ToonScreenOutline, ToonVfx)
// - 바닥판 위의 도형과 인스턴싱한 돌을 ToonRenderer 로 칠하고(껍데기 바깥선 없이), ToonScreenOutline 으로 실루엣, 맞닿은 곳, 꺾인 곳에 선을 긋는다.
//   카메라는 원근이고 굵기는 월드 길이 0.022 (보는 점까지의 거리에서 셈), 깊이 문턱 0.08, 맞닿음 0.015, 꺾임 60 도.
//   돌 일곱은 인스턴스마다 번호를 따로, 나머지 일곱은 한 번호로 그려 서로 맞닿은 곳의 선이 다른 것을 본다.
// - ToonVfx 로 3 초마다 터짐(충격파, 불덩이, 그을음, 불꽃 줄기, 연기, 흙 알갱이)과 연기 기둥, 흐르는 테 자국을 내고,
//   프레임마다 빔(겉집과 심), 번개, 리본, 빛 점을 넘긴다. 램프 색은 예제가 정한 넷(불, 푸른 빛, 보라, 연기)이다.
// - 터질 때 화면 섬광(add_flash)과 카메라 흔들림(add_shake, read_shake 를 카메라 오른쪽과 위쪽으로 더함), 1.2 초 사는 번개(add_lightning, 번짐 색)를 낸다.
//   테를 두른 빛살 파티클(rim 1)과 빛 점(rim 1, 하나 걸러), 번짐 색 리본(glow)은 가장자리를 테 색으로 덮는다.
// - 실행: cargo run --example toonfx
//==============================================================================
use glow::HasContext;
use std::f32::consts::PI;
use vanilla::{
	create_box_geometry, create_cylinder_geometry, create_octahedron_geometry, create_part_matrix, create_rounded_box_geometry, create_sphere_geometry, create_torus_geometry, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, Scene, SceneBase, ToonDrawOptions, ToonGeometry, ToonInstance, ToonLineDrawOptions, ToonMesh, ToonOutlineWidthUnit, ToonPartOptions, ToonRenderer, ToonScreenOutline, ToonBeamOptions, ToonDecalOptions, ToonFireballOptions, ToonGlintOptions, ToonLightningOptions, ToonParticleOptions, ToonPlumeOptions, ToonRibbonOptions, ToonShockwaveOptions, ToonFlashOptions, ToonVfx, ToonVfxDecalMode, ToonVfxLook, ToonVfxRange, ToonVfxShape, ToonVfxSlot, Vector2, Vector3, TOON_VFX_DEFAULT_PARTICLE_CAPACITY,
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
	vfx: Option<ToonVfx>,
	next_burst_seconds: f32,
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
			vfx: None,
			next_burst_seconds: 0.0,
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
		// 효과. (예제의 램프 넷, 바닥은 판 윗면 0 이고 판 밖은 -1)
		let mut vfx = ToonVfx::new(gl, TOON_VFX_DEFAULT_PARTICLE_CAPACITY)?;
		let ramp_text_list: [[&str; 6]; 4] = [
			["#ffffff", "#fff0b0", "#ffb040", "#e05020", "#601810", "#180808"],
			["#ffffff", "#c0fff8", "#40e0d0", "#108890", "#083038", "#041014"],
			["#ffffff", "#f0d0ff", "#b070ff", "#6030c0", "#281050", "#100818"],
			["#e8e4de", "#b8b0a8", "#888078", "#5a544e", "#36322e", "#1a1816"],
		];
		let ramp_list: Vec<Vec<&str>> = ramp_text_list.iter().map(|ramp_texts| ramp_texts.to_vec()).collect();
		vfx.set_ramps(&ramp_list);
		vfx.set_floor_height_reader(Some(Box::new(|x: f32, z: f32| if x.abs() <= 4.0 && z.abs() <= 2.5 { 0.0 } else { -1.0 })));
		vfx.prepare(gl);
		self.vfx = Some(vfx);
		self.renderer = Some(renderer);
		Ok(())
	}
}

impl ToonFxScene {
	//==============================================================================
	// 터짐 한 번. (충격파, 불덩이, 그을음, 불꽃 줄기, 연기, 흙 알갱이, 연기 기둥, 흐르는 테)
	//==============================================================================
	fn emit_burst(vfx: &mut ToonVfx) {
		let blast = [-0.4, 0.0, 0.45];
		vfx.add_shockwave(&ToonShockwaveOptions { x: blast[0], z: blast[2], start_radius: 0.2, end_radius: 2.0, width: 0.7, life: 1.1, ramp: 0, ..ToonShockwaveOptions::default() });
		vfx.add_fireball(&ToonFireballOptions { position: [blast[0], 0.45, blast[2]], radius: 0.7, life: 1.5, ramp: 0, ..ToonFireballOptions::default() });
		vfx.add_decal(&ToonDecalOptions { x: blast[0], z: blast[2], radius: 1.1, life: 3.0, heat_seconds: 1.5, slot: ToonVfxSlot::Scorch, mode: ToonVfxDecalMode::Scorch, ramp: 0, alpha: 0.9, ..ToonDecalOptions::default() });
		vfx.emit_particles(&ToonParticleOptions { count: 70, position: [blast[0], 0.3, blast[2]], shape: ToonVfxShape::Sphere, radius: 0.2, is_upward: true, speed: ToonVfxRange::new(3.0, 6.0), gravity: 6.0, drag: 1.2, size: ToonVfxRange::new(0.05, 0.09), life: ToonVfxRange::new(0.5, 1.0), ramp: 0, look: ToonVfxLook::Streak, stretch: 0.05, brightness: 1.6, is_grounded: true, ..ToonParticleOptions::default() });
		vfx.emit_particles(&ToonParticleOptions { count: 24, position: [blast[0], 0.3, blast[2]], shape: ToonVfxShape::Disc, radius: 0.5, speed: ToonVfxRange::new(0.4, 1.0), rise: ToonVfxRange::new(0.6, 1.2), drag: 0.8, size: ToonVfxRange::new(0.5, 0.8), size_end: ToonVfxRange::from(2.2), life: ToonVfxRange::new(1.4, 2.2), color: [0.32, 0.3, 0.29], look: ToonVfxLook::Smoke, alpha: 0.8, fade_in: 0.1, ..ToonParticleOptions::default() });
		vfx.emit_particles(&ToonParticleOptions { count: 30, position: [blast[0], 0.2, blast[2]], shape: ToonVfxShape::Ring, radius: 0.4, speed: ToonVfxRange::new(1.5, 2.5), rise: ToonVfxRange::new(0.2, 0.6), gravity: 2.0, drag: 1.5, size: ToonVfxRange::new(0.06, 0.12), life: ToonVfxRange::new(0.6, 1.2), color: [0.45, 0.4, 0.36], look: ToonVfxLook::Grit, is_grounded: true, ..ToonParticleOptions::default() });
		vfx.add_plume(&ToonPlumeOptions { x: 3.3, z: 0.4, height: 2.4, cap_radius: Some(0.9), stem_radius: Some(0.22), life: 3.0, ramp: 0, ..ToonPlumeOptions::default() });
		vfx.add_decal(&ToonDecalOptions { x: 1.4, z: 0.5, radius: 0.7, life: 3.0, slot: ToonVfxSlot::Glow, mode: ToonVfxDecalMode::Ring, ramp: 1, tint: [0.6, 1.4, 1.4], alpha: 1.0, ring: [0.75, 0.08, 3.0, 0.6], ..ToonDecalOptions::default() });
		vfx.emit_particles(&ToonParticleOptions { count: 14, position: [blast[0], 0.6, blast[2]], shape: ToonVfxShape::Sphere, radius: 0.3, speed: ToonVfxRange::new(1.2, 2.4), drag: 1.5, size: ToonVfxRange::new(0.3, 0.45), size_end: ToonVfxRange::from(0.4), life: ToonVfxRange::new(0.6, 1.0), color: [1.0, 0.8, 0.45], look: ToonVfxLook::Star, brightness: 1.3, rim: 1.0, ..ToonParticleOptions::default() });
		vfx.add_lightning(&ToonLightningOptions { start: [blast[0], 3.2, blast[2]], end: [blast[0], 0.1, blast[2]], radius: 0.1, count: 2, width: 0.12, turns: 0.6, brightness: 1.3, glow: Some([0.55, 0.85, 1.0]), life: 1.2, ..ToonLightningOptions::default() });
		vfx.add_flash(&ToonFlashOptions { color: [1.0, 0.9, 0.7], strength: 0.45, life: 0.25, ..ToonFlashOptions::default() });
		vfx.add_shake(0.08);
	}

	//==============================================================================
	// 이번 프레임에만 그리는 효과. (빔 겉집과 심, 번개, 리본, 빛 점)
	//==============================================================================
	fn queue_frame_effects(vfx: &mut ToonVfx, elapsed_seconds: f32) {
		vfx.queue_beam(&ToonBeamOptions { start: [1.4, 5.0, 0.5], end: [1.4, 0.0, 0.5], radius: 0.28, is_core: false, alpha: 1.0, brightness: 1.3, ramp: 1, wobble: 0.14, flow_speed: 8.0, grain_repeat: 1.4, ..ToonBeamOptions::default() });
		vfx.queue_beam(&ToonBeamOptions { start: [1.4, 5.0, 0.5], end: [1.4, 0.0, 0.5], radius: 0.1, is_core: true, alpha: 1.0, brightness: 1.5, ramp: 1, wobble: 0.05, flow_speed: 14.0, grain_repeat: 2.0, ..ToonBeamOptions::default() });
		vfx.queue_lightning(&ToonLightningOptions { start: [-2.4, 1.1, -0.8], end: [0.2, 1.2, -0.9], radius: 0.12, count: 2, width: 0.07, turns: 1.5, ramp: 2, brightness: 1.4, seed: Some(3.0), ..ToonLightningOptions::default() });
		let arc_points: Vec<[f32; 3]> = (0..=24).map(|index| {
			let ratio = index as f32 / 24.0;
			[2.7 - ratio * 3.0, 0.3 + (ratio * PI).sin() * 1.4, -0.4 + ratio * 1.8]
		}).collect();
		vfx.queue_ribbon(ToonRibbonOptions { points: arc_points, width: 0.14, taper: 0.5, ramp: 2, brightness: 1.2, ..ToonRibbonOptions::default() });
		let glow_points: Vec<[f32; 3]> = (0..=24).map(|index| {
			let ratio = index as f32 / 24.0;
			[-3.4 + ratio * 1.6, 0.4 + ratio * 1.6, 1.4 + (ratio * PI * 2.0 + elapsed_seconds * 2.0).sin() * 0.25]
		}).collect();
		vfx.queue_ribbon(ToonRibbonOptions { points: glow_points, width: 0.22, taper: 0.4, brightness: 1.2, glow: Some([1.0, 0.45, 0.8]), ..ToonRibbonOptions::default() });
		for glint_index in 0..12 {
			let angle = glint_index as f32 / 12.0 * PI * 2.0 + elapsed_seconds;
			let look = if glint_index % 2 == 0 { ToonVfxLook::Star } else { ToonVfxLook::Soft };
			vfx.queue_glint(&ToonGlintOptions { position: [0.2 + angle.cos() * 0.75, 0.55 + (angle * 2.0).sin() * 0.1, -0.9 + angle.sin() * 0.75], size: 0.22, color: [0.6, 1.0, 1.0], look, brightness: 1.3, rim: if glint_index % 2 == 0 { 1.0 } else { 0.0 }, ..ToonGlintOptions::default() });
		}
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
		let vfx = match self.vfx.as_mut() {
			Some(vfx) => {
				vfx
			}
			None => {
				return;
			}
		};
		vfx.update(time_delta.min(0.1));
		if self.elapsed_seconds >= self.next_burst_seconds {
			self.next_burst_seconds = self.elapsed_seconds + 3.0;
			ToonFxScene::emit_burst(vfx);
		}
		// 기둥 밑에서 오르는 불씨.
		if (self.elapsed_seconds * 10.0) as i32 != ((self.elapsed_seconds - time_delta) * 10.0) as i32 {
			vfx.emit_particles(&ToonParticleOptions { count: 6, position: [3.3, 0.2, 0.4], shape: ToonVfxShape::Disc, radius: 0.3, speed: ToonVfxRange::from(0.3), rise: ToonVfxRange::new(1.0, 1.8), size: ToonVfxRange::new(0.04, 0.07), life: ToonVfxRange::new(0.8, 1.4), ramp: 0, look: ToonVfxLook::Ember, brightness: 1.4, ..ToonParticleOptions::default() });
		}
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
		let mut eye_position = Vector3::create(0.0, 4.2, 6.2);
		let mut look_position = Vector3::create(0.0, 0.4, 0.0);
		// 카메라 흔들림. (오른쪽과 위쪽으로 자리와 보는 곳을 같이 옮긴다)
		if let Some(vfx) = self.vfx.as_ref() {
			let shake = vfx.read_shake();
			let forward = look_position.subtract(eye_position).normalize();
			let right = forward.cross(Vector3::create(0.0, 1.0, 0.0)).normalize();
			let up = right.cross(forward);
			let offset = right.multiply_scalar(shake.x).add(up.multiply_scalar(shake.y));
			eye_position = eye_position.add(offset);
			look_position = look_position.add(offset);
		}
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

		// 4. 효과. (프레임마다 넘기는 빔, 번개, 리본, 빛 점)
		if let Some(vfx) = self.vfx.as_mut() {
			ToonFxScene::queue_frame_effects(vfx, self.elapsed_seconds);
			vfx.draw(gl, &view_matrix, &projection_matrix);
		}
		graphic.restore_render_state();
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("툰 바깥선과 효과 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ToonFxScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
