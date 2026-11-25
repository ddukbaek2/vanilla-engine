//==============================================================================
// 툰 후처리와 효과 예제. (ToonPostProcess 옵션, MultiLevelBloomEffect, ToonVfx 의 새 꼴)
// - 세 판에 같은 장면을 그린다. 시간은 프레임 수 / 60 이라 찍을 때마다 같은 그림이다.
//   왼쪽: ToonPostProcess 기본(멀티샘플 없음, 반 해상도 9 탭 블룸).
//   가운데: 멀티샘플 4, 깊이 24, 다단 블룸 5 단(문턱 1, 무릎 0.25). 가장자리 계단과 블룸 번짐을 왼쪽과 견준다.
//   오른쪽: 실수 판을 쓰지 않는 LDR 길(화면 색을 그리고 톤매핑 없이), 밝기 0.45 에 상자 둘레 밝힌 자리(깊이 텍스처로 되짚은 수평 거리)와 채도 0.6.
// - 효과: 물든 자국(stain), 기름 윤(sheen), 깜빡이는 그을음, 발밑 그늘(queue_blob_shadow), 덮는 꼴(rim 1)의 불덩이와 충격파와 리본,
//   빛 점 120 개(상한 160), 불꽃 줄기 밝기 배 5. 불덩이는 3 초마다 사건 열쇠와 나이로 한 번만 낸다.
// - 실행: cargo run --example toonpost
//==============================================================================
use std::f32::consts::PI;
use vanilla::{
	create_box_geometry, create_part_matrix, create_sphere_geometry, Camera3D, Engine, EngineConfiguration, Graphic, Matrix4, MeshBuilder, NodeId, NodeTree, Paint, Pivot, Rect, Scene, SceneBase, Text, TextAlign, ToonBlobShadowOptions, ToonDecalOptions, ToonDrawOptions, ToonDrawQueue,
	ToonFireballOptions, ToonGeometry, ToonGlintOptions, ToonMesh, ToonMeshArrays, ToonParticleOptions, ToonPartOptions, ToonPostProcess, ToonPostProcessOptions, ToonRenderer, ToonRibbonOptions, ToonShockwaveOptions, ToonView3D, ToonVfx, ToonVfxDecalMode, ToonVfxLook, ToonVfxRange,
	ToonVfxShape, ToonVfxSlot, Vector2, Vector3, TOON_VFX_DEFAULT_PARTICLE_CAPACITY,
};

// 한 프레임의 초. (찍을 때마다 같은 그림)
const FRAME_SECONDS: f32 = 1.0 / 60.0;
// 불덩이 사건의 되풀이 초.
const BLAST_CYCLE_SECONDS: f32 = 3.0;
// 판의 꼴.
const PANEL_MODE_DEFAULT: u32 = 0;
const PANEL_MODE_MULTISAMPLE: u32 = 1;
const PANEL_MODE_LOW_DYNAMIC_RANGE: u32 = 2;

//==============================================================================
// 판에 그리는 모형.
//==============================================================================
struct PanelMeshes {
	ground: ToonMesh,
	box_mesh: ToonMesh,
	pole: ToonMesh,
	sphere: ToonMesh,
}

//==============================================================================
// 판 하나의 그리기 상태.
//==============================================================================
struct PanelDrawer {
	panel_mode: u32,
	meshes: Option<PanelMeshes>,
	post_process: Option<ToonPostProcess>,
	vfx: Option<ToonVfx>,
	frame_count: u32,
}

//==============================================================================
// 부품 하나의 배열.
//==============================================================================
fn build_arrays(geometry: &ToonGeometry, position: [f32; 3], color_text: &str, options: ToonPartOptions) -> ToonMeshArrays {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, &create_part_matrix(None, position, None, None), color_text, options);
	builder.read_arrays()
}

impl PanelDrawer {
	fn new(panel_mode: u32) -> PanelDrawer {
		PanelDrawer { panel_mode, meshes: None, post_process: None, vfx: None, frame_count: 0 }
	}

	//==============================================================================
	// 모형 짓기. (처음 그릴 때 한 번)
	//==============================================================================
	fn create_meshes(renderer: &ToonRenderer, gl: &glow::Context) -> Option<PanelMeshes> {
		let outline_options = ToonPartOptions { outline_width: 0.03, ..ToonPartOptions::default() };
		Some(PanelMeshes {
			ground: renderer.create_mesh(gl, &build_arrays(&create_box_geometry(8.0, 0.2, 6.0), [0.0, -0.1, 0.0], "#8a857b", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() }))?,
			box_mesh: renderer.create_mesh(gl, &build_arrays(&create_box_geometry(1.0, 1.0, 1.0), [0.0, 0.5, 0.0], "#ff8844", outline_options))?,
			pole: renderer.create_mesh(gl, &build_arrays(&create_box_geometry(0.05, 2.0, 0.05), [0.0, 1.0, 0.0], "#30343c", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() }))?,
			sphere: renderer.create_mesh(gl, &build_arrays(&create_sphere_geometry(0.4, 24, 16, PI), [0.0, 0.4, 0.0], "#ffd257", ToonPartOptions { glow: 2.5, outline_width: 0.02, ..ToonPartOptions::default() }))?,
		})
	}

	//==============================================================================
	// 후처리 짓기. (판의 꼴마다)
	//==============================================================================
	fn create_post_process(&self, gl: &glow::Context, width: i32, height: i32) -> Result<ToonPostProcess, String> {
		match self.panel_mode {
			PANEL_MODE_MULTISAMPLE => {
				let mut post_process = ToonPostProcess::new_with_options(gl, width, height, ToonPostProcessOptions { sample_count: 4, depth_bits: 24, ..ToonPostProcessOptions::default() })?;
				post_process.set_bloom_levels(gl, 5, 1.0, 0.25)?;
				Ok(post_process)
			}
			PANEL_MODE_LOW_DYNAMIC_RANGE => {
				let mut post_process = ToonPostProcess::new_with_options(gl, width, height, ToonPostProcessOptions { use_float_color: false, use_depth_texture: true, ..ToonPostProcessOptions::default() })?;
				post_process.set_bloom(false, 0.0, 0.9, 1.6);
				post_process.set_brightness(0.45);
				post_process.set_color_grade(0.6, 1.0, [1.0, 1.0, 1.0], 0.0, [0.0, 0.0, 0.0], 0.0);
				Ok(post_process)
			}
			_ => {
				ToonPostProcess::new(gl, width, height)
			}
		}
	}

	//==============================================================================
	// 효과 짓기. (램프, 테 색, 꼴 밝기 배, 빛 점 상한, 오래 가는 바닥 자국)
	//==============================================================================
	fn create_vfx(gl: &glow::Context) -> Result<ToonVfx, String> {
		let mut vfx = ToonVfx::new(gl, TOON_VFX_DEFAULT_PARTICLE_CAPACITY)?;
		vfx.set_ramps(&[vec!["#ffffff", "#ffe08a", "#ff9a3c", "#d4441c", "#5a1a10", "#1a0a08"], vec!["#ffffff", "#bff6ff", "#5fd0ff", "#2a7ad8", "#123c7a", "#06142a"]]);
		vfx.set_rim_color([0.078, 0.086, 0.11]);
		vfx.set_look_gain(ToonVfxLook::Streak, 5.0);
		vfx.set_glint_capacity(160);
		vfx.add_decal(&ToonDecalOptions { x: 1.6, z: 1.3, radius: 0.85, life: 100000.0, slot: ToonVfxSlot::Scorch, mode: ToonVfxDecalMode::Stain, tint: [0.35, 0.55, 0.85], alpha: 0.8, angle: Some(0.3), key: Some(1), ..ToonDecalOptions::default() });
		vfx.add_decal(&ToonDecalOptions { x: -0.3, z: 1.6, radius: 0.75, life: 100000.0, slot: ToonVfxSlot::Scorch, mode: ToonVfxDecalMode::Sheen, tint: [0.12, 0.1, 0.09], alpha: 0.9, angle: Some(1.1), key: Some(2), ..ToonDecalOptions::default() });
		vfx.add_decal(&ToonDecalOptions { x: 2.4, z: -0.9, radius: 0.7, life: 100000.0, slot: ToonVfxSlot::Scorch, mode: ToonVfxDecalMode::Scorch, alpha: 0.85, angle: Some(2.0), flicker: 0.5, key: Some(3), ..ToonDecalOptions::default() });
		// 같은 열쇠라 다시 내지 않는다.
		vfx.add_decal(&ToonDecalOptions { x: 1.6, z: 1.3, radius: 0.85, life: 100000.0, mode: ToonVfxDecalMode::Stain, key: Some(1), ..ToonDecalOptions::default() });
		Ok(vfx)
	}

	//==============================================================================
	// 이번 프레임의 효과. (불덩이 사건은 되풀이마다 열쇠 하나와 그 나이로, 프레임마다 다시 불러도 한 번만 난다)
	//==============================================================================
	fn emit_effects(vfx: &mut ToonVfx, elapsed_seconds: f32) {
		let cycle_index = (elapsed_seconds / BLAST_CYCLE_SECONDS).floor();
		let age = elapsed_seconds - cycle_index * BLAST_CYCLE_SECONDS;
		let cycle_key = cycle_index as u64;
		let blast = [1.4, 0.6, -0.4];
		vfx.add_fireball(&ToonFireballOptions { position: blast, radius: 0.55, life: 2.4, ramp: 0, grow_seconds: 0.4, rise: 0.5, rim: 1.0, age, key: Some(100 + cycle_key), ..ToonFireballOptions::default() });
		vfx.add_shockwave(&ToonShockwaveOptions { x: blast[0], z: blast[2], start_radius: 0.2, end_radius: 1.8, width: 0.45, life: 0.8, delay: 0.5, ramp: 0, rim: 1.0, age, key: Some(200 + cycle_key), ..ToonShockwaveOptions::default() });
		vfx.emit_particles(&ToonParticleOptions {
			count: 48,
			position: blast,
			shape: ToonVfxShape::Sphere,
			radius: 0.2,
			is_upward: true,
			speed: ToonVfxRange::new(2.5, 5.5),
			rise: ToonVfxRange::new(0.5, 2.0),
			gravity: 7.0,
			drag: 1.0,
			size: ToonVfxRange::new(0.05, 0.08),
			life: ToonVfxRange::new(0.8, 1.6),
			look: ToonVfxLook::Streak,
			stretch: 0.05,
			ramp: 0,
			floor: Some(0.02),
			age,
			key: Some(300 + cycle_key),
			..ToonParticleOptions::default()
		});
		// 발밑 그늘.
		vfx.queue_blob_shadow(&ToonBlobShadowOptions { x: -1.7, z: 0.2, radius: 0.85, ..ToonBlobShadowOptions::default() });
		vfx.queue_blob_shadow(&ToonBlobShadowOptions { x: -0.2, z: -0.9, radius: 0.6, alpha: 0.6 });
		// 덮는 꼴 리본. (공 둘레의 고리)
		let mut points = Vec::new();
		for point_index in 0..=40 {
			let angle = point_index as f32 / 40.0 * PI * 1.6 + elapsed_seconds;
			points.push([-0.2 + angle.cos() * 0.9, 1.1 + (angle * 2.0).sin() * 0.15, -0.9 + angle.sin() * 0.9]);
		}
		vfx.queue_ribbon(ToonRibbonOptions { points, width: 0.14, color: [0.6, 0.9, 1.0], ramp: 1, alpha: 1.0, brightness: 1.4, rim: 1.0, ..ToonRibbonOptions::default() });
		// 빛 점 120 개. (기본 상한 96 을 넘는다)
		for glint_index in 0..120 {
			let angle = glint_index as f32 / 120.0 * PI * 2.0;
			vfx.queue_glint(&ToonGlintOptions { position: [angle.cos() * 3.4, 0.08, angle.sin() * 2.5], size: 0.14, color: [1.0, 0.85, 0.4], brightness: 3.0, ..ToonGlintOptions::default() });
		}
	}

	//==============================================================================
	// 판 그리기. (ToonView3D 의 그리기 함수)
	//==============================================================================
	fn draw(&mut self, renderer: &mut ToonRenderer, camera: &Camera3D, screen_rect: Rect, gl: &glow::Context, drawing_buffer_height: i32) {
		if self.meshes.is_none() {
			self.meshes = PanelDrawer::create_meshes(renderer, gl);
		}
		let width = screen_rect.size.x.round().max(1.0) as i32;
		let height = screen_rect.size.y.round().max(1.0) as i32;
		if self.post_process.is_none() {
			match self.create_post_process(gl, width, height) {
				Ok(post_process) => {
					self.post_process = Some(post_process);
				}
				Err(error) => {
					eprintln!("toonpost: {}", error);
					return;
				}
			}
		}
		if self.vfx.is_none() {
			match PanelDrawer::create_vfx(gl) {
				Ok(vfx) => {
					self.vfx = Some(vfx);
				}
				Err(error) => {
					eprintln!("toonpost: {}", error);
					return;
				}
			}
		}
		let (Some(meshes), Some(post_process), Some(vfx)) = (self.meshes.as_ref(), self.post_process.as_mut(), self.vfx.as_mut()) else {
			return;
		};
		let elapsed_seconds = self.frame_count as f32 * FRAME_SECONDS;
		renderer.set_band_count(3);
		renderer.set_light([-0.5, -1.0, -0.35], [0.85, 0.8, 0.75], [0.55, 0.6, 0.7], [0.25, 0.22, 0.2]);
		let mut queue = ToonDrawQueue::new();
		queue.add_mesh(&meshes.ground, &Matrix4::create_identity(), ToonDrawOptions::default());
		let mut box_matrix = Matrix4::create_translation(-1.7, 0.0, 0.2);
		box_matrix.rotate_y(0.6 + elapsed_seconds * 0.4);
		queue.add_mesh(&meshes.box_mesh, &box_matrix, ToonDrawOptions::default());
		let mut pole_matrix = Matrix4::create_translation(0.6, 0.0, -1.2);
		pole_matrix.rotate_z(0.45);
		queue.add_mesh(&meshes.pole, &pole_matrix, ToonDrawOptions::default());
		queue.add_mesh(&meshes.sphere, &Matrix4::create_translation(-0.2, 0.0, -0.9), ToonDrawOptions::default());

		let is_low_dynamic_range = self.panel_mode == PANEL_MODE_LOW_DYNAMIC_RANGE;
		if is_low_dynamic_range {
			let inverse_view_projection_matrix = camera.get_inverse_view_projection_matrix();
			post_process.set_lit_area([-1.7, 0.0, 0.2], 0.9, 1.6, &inverse_view_projection_matrix);
		}
		let _ = post_process.resize(gl, width, height);
		renderer.clear_screen_rect();
		renderer.set_hdr_output(!is_low_dynamic_range && post_process.get_use_float_color());
		let clear_color = if is_low_dynamic_range { [0.25, 0.27, 0.33] } else { [0.05, 0.06, 0.09] };
		post_process.begin(gl, clear_color);
		queue.draw(gl, renderer, camera);
		vfx.update(if self.frame_count == 0 { 0.0 } else { FRAME_SECONDS });
		PanelDrawer::emit_effects(vfx, elapsed_seconds);
		vfx.draw(gl, &camera.get_view_matrix(), &camera.get_projection_matrix());
		let bottom = drawing_buffer_height - screen_rect.position.y.round() as i32 - height;
		post_process.end(gl, None, [screen_rect.position.x.round() as i32, bottom, width, height]);
		self.frame_count += 1;
		if self.frame_count == 30 {
			println!("toonpost: 판 {} 표본 수 {} 실수 판 {} 블룸 단 {}", self.panel_mode, post_process.get_sample_count(), post_process.get_use_float_color(), post_process.get_bloom_level_count());
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

struct ToonPostScene {
	scene_base: SceneBase,
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
// 글 노드.
//==============================================================================
fn create_text(node_tree: &mut NodeTree, parent_id: NodeId, position: Vector2, text_value: &str) -> NodeId {
	let node_id = node_tree.create_child(parent_id, "text");
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_local_position(position);
	node.set_content_size(Vector2::create(1200.0, 24.0));
	let text = node.add_component(Text::create_with_text(text_value, 16.0));
	text.set_text_color("#ffffff");
	text.set_text_align(TextAlign::Left);
	node_id
}

impl Scene for ToonPostScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 바탕을 칠하고 화면 배율을 건다. (3D 판은 노드가 그린다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color("#000000");
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let panel_list = [(PANEL_MODE_DEFAULT, "기본: 멀티샘플 없음, 9 탭 블룸"), (PANEL_MODE_MULTISAMPLE, "멀티샘플 4, 깊이 24, 다단 블룸 5 단"), (PANEL_MODE_LOW_DYNAMIC_RANGE, "LDR 길: 밝기 0.45, 밝힌 자리, 채도 0.6")];
		for (panel_index, (panel_mode, label)) in panel_list.iter().enumerate() {
			let panel_x = 20.0 + panel_index as f32 * 420.0;
			let panel_id = create_panel(node_tree, root_id, Rect::create(panel_x, 80.0, 400.0, 420.0), "#000000");
			let mut panel_drawer = PanelDrawer::new(*panel_mode);
			let toon_view = node_tree[panel_id].add_component(ToonView3D::new());
			toon_view.set_draw_callback(Some(Box::new(move |renderer, camera, screen_rect, gl, drawing_buffer_height| {
				panel_drawer.draw(renderer, camera, screen_rect, gl, drawing_buffer_height);
			})));
			toon_view.set_fallback_draw_callback(Some(Box::new(draw_fallback)));
			let camera = toon_view.get_camera_mut();
			camera.set_perspective(0.8, 0.1, 100.0);
			camera.look_at(Vector3::create(1.0, 3.4, 5.6), Vector3::create(0.0, 0.4, 0.0), Vector3::create(0.0, 1.0, 0.0));
			create_text(node_tree, root_id, Vector2::create(panel_x, 54.0), label);
		}
		create_text(node_tree, root_id, Vector2::create(20.0, 16.0), "툰 후처리와 효과: 물든 자국, 기름 윤, 깜빡이는 그을음, 발밑 그늘, 덮는 꼴 불덩이와 충격파와 리본, 빛 점 120 (상한 160)");
		create_text(node_tree, root_id, Vector2::create(20.0, 520.0), "불덩이는 3 초마다 사건 열쇠와 나이로 한 번만 낸다. 불꽃 줄기 밝기 배 5. 시간은 프레임 수 / 60.");
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("툰 후처리와 효과 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let scene = ToonPostScene { scene_base: SceneBase::new() };
	if let Err(error) = engine.run(Box::new(scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
