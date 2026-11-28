//==============================================================================
//! 툰 재질 블록과 자동 인스턴싱 시험. (원본 vanilla.js tests/graphics3d/toonuniformblockchrome, toonautoinstancechrome 과 같은 검사)
//! - 숨긴 GL 컨텍스트(창을 띄우지 않는다)에서 장면을 128 x 128 프레임버퍼에 그린다.
//! - 화면 해시는 TOON_BATCH_HASH 가 있으면 줄마다 `해시 이름 값` 으로 찍는다. (고치기 전 코드와 견주기)
//==============================================================================
use super::toondrawqueue::{ToonDrawQueue, ToonDrawStage};
use super::toonrenderer::{ToonDrawOptions, ToonMesh, ToonRenderer, BUILT_FEATURE_VARIANTS, DISABLE_FEATURE_VARIANTS, FORCE_DEFERRED_PREWARM, UPLOAD_COUNTS};
use super::toonscreenoutline::ToonScreenOutline;
use crate::base::matrix4::Matrix4;
use crate::core::glcontext::GlContext;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::shadowmap::{ShadowMap, ShadowUpdateMode};
use crate::{create_box_geometry, create_sphere_geometry, MeshBuilder, ToonPartOptions, Vector3};

const SIZE: i32 = 128;

//==============================================================================
/// 시험 판. (RGBA8 색, 깊이 24, 창 없는 컨텍스트)
//==============================================================================
struct Target<'a> {
	gl: &'a GlContext,
	raw: &'a glow::Context,
	framebuffer: glow::Framebuffer,
}

fn create_target<'a>(gl: &'a GlContext) -> Target<'a> {
	use glow::HasContext;
	let raw: &glow::Context = gl.get_glow_context_unchecked();
	let framebuffer = unsafe {
		let color_texture = crate::core::glstatecache::create_texture(gl).unwrap();
		crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(color_texture));
		GlContext::from_context(&raw).tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, SIZE, SIZE, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		let depth = GlContext::from_context(&raw).create_renderbuffer().unwrap();
		raw.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
		GlContext::from_context(&raw).renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, SIZE, SIZE);
		let framebuffer = crate::core::glstatecache::create_framebuffer(gl).unwrap();
		crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(framebuffer));
		raw.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
		raw.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
		assert_eq!(raw.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE);
		framebuffer
	};
	Target { gl, raw, framebuffer }
}

impl Target<'_> {
	fn clear(&self) {
		use glow::HasContext;
		unsafe {
			crate::core::glstatecache::bind_framebuffer(self.gl, glow::FRAMEBUFFER, Some(self.framebuffer));
			crate::core::glstatecache::viewport(self.gl, 0, 0, SIZE, SIZE);
			self.raw.clear_color(0.1, 0.1, 0.1, 1.0);
			self.raw.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
	}

	fn read_pixels(&self) -> Vec<u8> {
		use glow::HasContext;
		let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
		unsafe {
			crate::core::glstatecache::bind_framebuffer(self.gl, glow::FRAMEBUFFER, Some(self.framebuffer));
			self.raw.read_pixels(0, 0, SIZE, SIZE, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
		}
		pixels
	}
}

fn read_hash(pixels: &[u8]) -> u64 {
	pixels.iter().fold(0xcbf29ce484222325u64, |hash, &byte| (hash ^ byte as u64).wrapping_mul(0x100000001b3))
}

fn print_hash(name: &str, pixels: &[u8]) {
	if std::env::var("TOON_BATCH_HASH").is_ok() {
		eprintln!("해시 {} {:016x}", name, read_hash(pixels));
	}
}

fn create_mesh(gl: &GlContext, renderer: &mut ToonRenderer, geometry: &crate::ToonGeometry, color: &str) -> ToonMesh {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, &Matrix4::create_identity(), color, ToonPartOptions { has_outline: true, ..ToonPartOptions::default() });
	renderer.create_mesh(gl, &builder.read_arrays()).expect("모형")
}

fn create_camera() -> Camera3D {
	let mut camera = Camera3D::new();
	camera.set_perspective(0.9, 0.1, 50.0);
	camera.set_aspect_ratio(1.0);
	camera.look_at(Vector3::create(0.0, 5.0, 7.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	camera
}

//==============================================================================
/// 같은 상자 열하나와 공 셋을 담아 그리기. (is_distinct 면 꼴마다 다른 index_count 를 줘 묶지 못하게, 모형의 번호 수보다 커서 그리는 범위는 같다)
/// - 돌려줌: (화소, 드로 콜 수, 그린 수, 올림 수 [묶음 자리, 블록 버퍼, 블록 묶기])
//==============================================================================
fn draw_scene(gl: &GlContext, renderer: &mut ToonRenderer, target: &Target, box_mesh: &ToonMesh, sphere_mesh: &ToonMesh, shadow_map: Option<&ShadowMap>, is_distinct: bool, outline: Option<&mut ToonScreenOutline>) -> (Vec<u8>, u32, u32, [u32; 5]) {
	let camera = create_camera();
	let before = UPLOAD_COUNTS.with(|counts| counts.get());
	let mut queue = ToonDrawQueue::new();
	for index in 0..14usize {
		let (mesh, count) = if index < 11 { (box_mesh, index) } else { (sphere_mesh, index) };
		let options = ToonDrawOptions { index_count: if is_distinct { 100000 + index } else { 0 }, ..ToonDrawOptions::default() };
		let column = count as f64 % 7.0;
		let row = (index / 7) as f64;
		queue.add_mesh(mesh, &Matrix4::create_translation(-3.0 + column * 1.0, 0.5, -1.0 + row * 2.0), options);
	}
	queue.add_pass(ToonDrawStage::AfterOpaque, |_gl: &GlContext, _renderer: &mut ToonRenderer| {});
	queue.set_screen_outline(outline);
	if let Some(shadow_map) = shadow_map {
		queue.draw_shadows(gl, renderer, shadow_map);
	}
	target.clear();
	queue.draw(gl, renderer, &camera);
	let statistics = renderer.get_frame_statistics();
	let after = UPLOAD_COUNTS.with(|counts| counts.get());
	(target.read_pixels(), statistics.draw_call_count, statistics.drawn_object_count, [after[0] - before[0], after[1] - before[1], after[2] - before[2], after[3] - before[3], after[4] - before[4]])
}

//==============================================================================
/// 자동 인스턴싱: 같은 모형과 재질 블록의 불투명한 것은 몸과 바깥선 껍데기를 인스턴스 그리기 하나로, 화면은 묶지 않은 것과 바이트까지 같고, 멈춘 프레임은 올림이 없다.
//==============================================================================
#[test]
fn auto_instancing_batches_and_keeps_pixels() {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let box_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(0.8, 0.8, 0.8), "#c08060");
	let sphere_mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.4, 12, 8, std::f64::consts::PI), "#6080c0");
	let mut shadow_map = ShadowMap::new(gl, 256).expect("그림자 판");
	shadow_map.set_update_mode(ShadowUpdateMode::EveryFrame, 1);
	shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(-0.5, -1.0, -0.4), 6.0, 0.1, 20.0);
	renderer.set_shadow(Some(&shadow_map), 0.7);
	let (reference_pixels, reference_draw_calls, reference_drawn, _) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, Some(&shadow_map), true, None);
	let (batch_pixels, batch_draw_calls, batch_drawn, batch_upload) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, Some(&shadow_map), false, None);
	eprintln!("자동 인스턴싱: 드로 콜 묶지 않음 {} 묶음 {}, 그린 수 {} {}", reference_draw_calls, batch_draw_calls, reference_drawn, batch_drawn);
	print_hash("autoinstance", &batch_pixels);
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
	assert!(reference_draw_calls >= 28, "묶지 않은 쪽은 몸 열넷과 바깥선 열넷 ({})", reference_draw_calls);
	assert!(batch_draw_calls <= 4, "묶은 쪽은 몸 둘과 바깥선 둘 ({})", batch_draw_calls);
	assert_eq!(reference_drawn, batch_drawn, "그린 수가 같다");
	assert!(batch_upload[0] >= 2, "처음 묶는 프레임은 묶음 자리를 올린다 ({})", batch_upload[0]);
	assert!(reference_pixels == batch_pixels, "묶은 화면이 묶지 않은 화면과 바이트까지 같다");
	assert!(batch_pixels.chunks(4).any(|pixel| pixel[0] != 26), "무엇인가 그려졌다");
	// 멈춘 프레임: 같은 장면을 다시 그리면 묶음 자리를 다시 올리지 않고 블록도 새로 짓지 않는다.
	let (_, _, _, first_upload) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, Some(&shadow_map), false, None);
	let (stopped_pixels, _, _, stopped_upload) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, Some(&shadow_map), false, None);
	eprintln!("자동 인스턴싱: 올림 [묶음 자리, 블록 버퍼, 블록 묶기] 앞 프레임 {:?} 멈춘 프레임 {:?}", first_upload, stopped_upload);
	assert_eq!(stopped_upload[0], 0, "멈춘 프레임은 묶음 자리를 올리지 않는다");
	assert_eq!(stopped_upload[1], 0, "멈춘 프레임은 블록을 짓지 않는다");
	assert!(stopped_pixels == batch_pixels, "멈춘 프레임도 같은 화면");
}

unsafe fn glow_context_error(glow_context: &glow::Context) -> u32 {
	use glow::HasContext;
	unsafe { glow_context.get_error() }
}

//==============================================================================
/// 재질 블록: 값이 같으면 같은 블록, 한도(1024)에서 오래 안 쓴 하나를 내보내고 그 버퍼에 쓴다, 내보낸 뒤에도 화면이 같다.
//==============================================================================
#[test]
fn material_block_shares_values_and_evicts() {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let box_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(1.6, 1.6, 1.6), "#c08060");
	let camera = create_camera();
	let red = ToonDrawOptions { tint_color: [1.0, 0.2, 0.2, 1.0], tint_amount: Some(0.8), ..ToonDrawOptions::default() };
	let same = ToonDrawOptions { tint_color: [1.0, 0.2, 0.2, 1.0], tint_amount: Some(0.8), ..ToonDrawOptions::default() };
	let blue = ToonDrawOptions { tint_color: [0.2, 0.2, 1.0, 1.0], tint_amount: Some(0.8), ..ToonDrawOptions::default() };
	let red_block = renderer.read_material_block(gl, &red);
	assert_eq!(red_block, renderer.read_material_block(gl, &same), "값이 같은 꼴은 같은 블록");
	assert_ne!(red_block, renderer.read_material_block(gl, &blue), "값이 다른 꼴은 다른 블록");
	let draw_red = |renderer: &mut ToonRenderer| {
		let mut queue = ToonDrawQueue::new();
		queue.add_mesh(&box_mesh, &Matrix4::create_translation(0.0, 0.0, 0.0), red);
		target.clear();
		queue.draw(gl, renderer, &camera);
		target.read_pixels()
	};
	let first_pixels = draw_red(&mut renderer);
	print_hash("materialblock", &first_pixels);
	let before = UPLOAD_COUNTS.with(|counts| counts.get());
	let stopped_pixels = draw_red(&mut renderer);
	let after = UPLOAD_COUNTS.with(|counts| counts.get());
	assert_eq!(after[1] - before[1], 0, "값이 같으면 블록 버퍼를 올리지 않는다");
	assert!(stopped_pixels == first_pixels);
	// 서로 다른 값 1100 개: 1024 에서 멈추고 내보내며 새 버퍼를 짓지 않는다.
	let before = UPLOAD_COUNTS.with(|counts| counts.get());
	for index in 0..1100u32 {
		let options = ToonDrawOptions { tint_color: [index as f64 / 1100.0, 0.5, 0.5, 1.0], ..ToonDrawOptions::default() };
		renderer.read_material_block(gl, &options);
	}
	let after = UPLOAD_COUNTS.with(|counts| counts.get());
	assert!(after[1] - before[1] >= 1000, "새 값마다 버퍼를 올린다 ({})", after[1] - before[1]);
	let evicted_pixels = draw_red(&mut renderer);
	assert!(evicted_pixels == first_pixels, "내보낸 뒤 다시 읽어도 같은 화면");
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
}

//==============================================================================
/// 선의 판 묶음: 같은 모형과 선 꼴은 인스턴스 그리기 하나로, 화면은 묶지 않은 것과 같고, 멈춘 프레임은 묶음 자리를 올리지 않는다.
//==============================================================================
#[test]
fn line_batching_keeps_pixels_and_stops_uploads() {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let box_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(0.8, 0.8, 0.8), "#c08060");
	let sphere_mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.4, 12, 8, std::f64::consts::PI), "#6080c0");
	let mut outline = ToonScreenOutline::new(gl).expect("화면 바깥선");
	let (reference_pixels, _, _, reference_counts) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, None, true, Some(&mut outline));
	let (batch_pixels, _, _, batch_counts) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, None, false, Some(&mut outline));
	eprintln!("선의 판 묶음: 선의 판 드로 콜 묶지 않음 {} 묶음 {}, 묶음 자리 올림 {} {}", reference_counts[4], batch_counts[4], reference_counts[3], batch_counts[3]);
	print_hash("linebatch", &batch_pixels);
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
	assert_eq!(reference_counts[4], 14, "묶지 않은 쪽은 모형마다 하나씩 ({})", reference_counts[4]);
	assert_eq!(batch_counts[4], 2, "묶은 쪽은 상자 하나와 공 하나 ({})", batch_counts[4]);
	assert!(reference_pixels == batch_pixels, "묶은 선의 판 화면이 묶지 않은 것과 바이트까지 같다");
	let (_, _, _, first_counts) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, None, false, Some(&mut outline));
	let (stopped_pixels, _, _, stopped_counts) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, None, false, Some(&mut outline));
	eprintln!("선의 판 묶음: 묶음 자리 올림 앞 프레임 {} 멈춘 프레임 {}", first_counts[3], stopped_counts[3]);
	assert_eq!(stopped_counts[3], 0, "멈춘 프레임은 선의 판 묶음 자리를 올리지 않는다");
	assert!(stopped_pixels == batch_pixels, "멈춘 프레임도 같은 화면");
}

//==============================================================================
/// 원본 toonautoinstancechrome 과 같은 장면: 같은 모형과 재질 값만 묶고 머티리얼, 모형, 틴트, 뒤집은 감기가 다르면 묶지 않는다.
/// - 그림자, 몸, 바깥선 껍데기는 [상자 같은 꼴 6, 상자 빨강 3, 공 3, 상자 머티리얼 2, 상자 뒤집은 감기 3], 선의 판은 [상자 11, 공 3, 상자 뒤집은 감기 3 (뒤집은 것은 뒤집은 것끼리 묶음)] 이라 인스턴스 수 목록이 원본과 같다.
//==============================================================================
#[test]
fn auto_instancing_matches_original_instance_counts() {
	use super::toonmaterial::{ToonMaterial, ToonMaterialChunks};
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let box_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(0.5, 0.5, 0.5), "#e0d0c0");
	let sphere_mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.3, 12, 8, std::f64::consts::PI), "#e0d0c0");
	let ground_mesh = {
		let mut builder = MeshBuilder::new();
		builder.add_part(&create_box_geometry(6.0, 0.1, 5.0), &Matrix4::create_translation(0.0, -0.05, 0.0), "#808890", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
		renderer.create_mesh(gl, &builder.read_arrays()).expect("바닥")
	};
	let stripe = ToonMaterial::new(ToonMaterialChunks { name: "autoStripe".to_string(), surface_chunk: "void applySurface(inout ToonSurface surface) { surface.baseColor *= 0.7; }".to_string(), ..ToonMaterialChunks::default() });
	let mut outline = ToonScreenOutline::new(gl).expect("화면 바깥선");
	outline.set_width(1.0, super::toonscreenoutline::ToonOutlineWidthUnit::Pixel);
	let mut shadow_map = ShadowMap::new(gl, 256).expect("그림자 판");
	shadow_map.set_update_mode(ShadowUpdateMode::EveryFrame, 1);
	shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(-0.3, -1.0, -0.2), 6.0, 0.1, 20.0);
	renderer.set_shadow(Some(&shadow_map), 0.6);
	let mut camera = Camera3D::new();
	camera.set_perspective(0.9, 0.1, 30.0);
	camera.set_aspect_ratio(1.0);
	camera.look_at(Vector3::create(0.0, 4.0, 5.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	// 담을 것: [모형, 꼴, 머티리얼 쓰는지, 뒤집었는지]
	let plain = [0.9, 0.8, 0.7, 1.0];
	let red = [1.0, 0.4, 0.4, 1.0];
	let mut entry_list: Vec<(bool, [f64; 4], bool, bool)> = Vec::new(); // (공인지, 틴트, 머티리얼, 뒤집음)
	for _ in 0..6 {
		entry_list.push((false, plain, false, false));
	}
	for _ in 0..3 {
		entry_list.push((false, red, false, false));
		entry_list.push((true, plain, false, false));
	}
	entry_list.push((false, plain, true, false));
	entry_list.push((false, plain, true, false));
	entry_list.push((false, plain, false, true));
	entry_list.push((false, plain, false, true));
	entry_list.push((false, plain, false, true));
	let mut matrix_list: Vec<Matrix4> = Vec::new();
	for (index, entry) in entry_list.iter().enumerate() {
		let mut matrix = Matrix4::create_translation((index % 5) as f64 * 0.9 - 1.8, 0.25, (index / 5) as f64 * 0.9 - 1.2);
		matrix.multiply(&Matrix4::create_rotation_y(index as f64 * 0.4));
		if entry.3 {
			matrix.multiply(&Matrix4::create_scale(-1.0, 1.0, 1.0));
		}
		matrix_list.push(matrix);
	}
	let transparent_matrix = Matrix4::create_translation(0.4, 0.6, 0.6);
	let ground_matrix = Matrix4::create_identity();
	let mut results: Vec<(Vec<i32>, u32, Vec<u8>)> = Vec::new();
	let mut upload_frames: Vec<[u32; 5]> = Vec::new();
	for is_reference in [false, true] {
		let mut frame_results = Vec::new();
		for _frame in 0..3 {
			let _ = crate::core::glstatecache::take_draw_record();
			let upload_before = UPLOAD_COUNTS.with(|counts| counts.get());
			let mut queue = ToonDrawQueue::new();
			queue.set_screen_outline(Some(&mut outline));
			queue.add_mesh(&ground_mesh, &ground_matrix, ToonDrawOptions { line_id: Some(1), has_screen_outline: false, ..ToonDrawOptions::default() });
			for (index, entry) in entry_list.iter().enumerate() {
				let mut options = ToonDrawOptions { tint_color: entry.1, line_id: Some(2 + index as u32), ..ToonDrawOptions::default() };
				if entry.2 {
					options.material = Some(&stripe);
				}
				if is_reference {
					options.index_count = 100000 + index;
				}
				queue.add_mesh(if entry.0 { &sphere_mesh } else { &box_mesh }, &matrix_list[index], options);
			}
			queue.add_mesh(&box_mesh, &transparent_matrix, ToonDrawOptions { tint_color: [0.5, 0.8, 1.0, 1.0], alpha: 0.5, line_id: Some(40), ..ToonDrawOptions::default() });
			queue.draw_shadows(gl, &mut renderer, &shadow_map);
			target.clear();
			queue.draw(gl, &mut renderer, &camera);
			let upload_after = UPLOAD_COUNTS.with(|counts| counts.get());
			if !is_reference {
				upload_frames.push([upload_after[0] - upload_before[0], upload_after[1] - upload_before[1], upload_after[2] - upload_before[2], upload_after[3] - upload_before[3], upload_after[4] - upload_before[4]]);
			}
			let (plain_count, mut instance_counts) = crate::core::glstatecache::take_draw_record();
			instance_counts.sort_by(|first, second| second.cmp(first));
			frame_results.push((instance_counts, plain_count, target.read_pixels()));
		}
		results.push(frame_results.remove(0));
		let (_, _, last_pixels) = frame_results.pop().unwrap();
		let first_pixels = results.last().unwrap().2.clone();
		assert!(first_pixels == last_pixels, "프레임을 되풀이해도 같은 화면");
	}
	let expected = vec![11, 6, 6, 6, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 2, 2, 2];
	eprintln!("자동 인스턴싱(원본 장면): 인스턴스 수 {:?}, 보통 그리기 묶음 {} 묶지 않음 {}", results[0].0, results[0].1, results[1].1);
	print_hash("autoinstance_original_scene", &results[0].2);
	eprintln!("자동 인스턴싱(원본 장면): 프레임마다 올림 [몸과 그림자 묶음 자리, 블록 버퍼, 블록 묶기, 선의 판 묶음 자리, 선의 판 드로 콜] {:?}", upload_frames);
	assert_eq!(upload_frames[1][0], 0, "멈춘 프레임은 몸과 그림자 묶음 자리를 올리지 않는다");
	assert_eq!(upload_frames[1][3], 0, "멈춘 프레임은 선의 판 묶음 자리를 올리지 않는다");
	assert_eq!(upload_frames[2][0], 0);
	assert_eq!(results[0].0, expected, "같은 모형과 재질 값만 묶는다");
	assert!(results[1].0.is_empty(), "비교 쪽은 묶지 않는다 ({:?})", results[1].0);
	assert!(results[0].1 + (results[0].0.len() as u32) < results[1].1, "그리기 수가 준다");
	assert!(results[0].2 == results[1].2, "화면이 묶지 않고 그린 것과 바이트까지 같다");
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
}

//==============================================================================
/// 원본 toonlinebatchchrome 의 같은 확인: 행렬 하나만 바뀌면 그 묶음 하나만 올린다.
//==============================================================================
#[test]
fn line_batching_uploads_only_changed_group() {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let box_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(0.8, 0.8, 0.8), "#c08060");
	let sphere_mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.4, 12, 8, std::f64::consts::PI), "#6080c0");
	let mut outline = ToonScreenOutline::new(gl).expect("화면 바깥선");
	let camera = create_camera();
	let draw_frame = |renderer: &mut ToonRenderer, outline: &mut ToonScreenOutline, shift: f64| {
		let before = UPLOAD_COUNTS.with(|counts| counts.get());
		let mut queue = ToonDrawQueue::new();
		queue.set_screen_outline(Some(outline));
		for index in 0..9usize {
			let (mesh, count) = if index < 6 { (&box_mesh, index) } else { (&sphere_mesh, index) };
			let moved = if index == 8 { shift } else { 0.0 };
			queue.add_mesh(mesh, &Matrix4::create_translation(-3.0 + count as f64 * 0.8 + moved, 0.5, 0.0), ToonDrawOptions::default());
		}
		target.clear();
		queue.draw(gl, renderer, &camera);
		let after = UPLOAD_COUNTS.with(|counts| counts.get());
		(target.read_pixels(), after[3] - before[3])
	};
	let (first_pixels, first_upload) = draw_frame(&mut renderer, &mut outline, 0.0);
	let (second_pixels, second_upload) = draw_frame(&mut renderer, &mut outline, 0.0);
	let (moved_pixels, moved_upload) = draw_frame(&mut renderer, &mut outline, 0.4);
	let (still_pixels, still_upload) = draw_frame(&mut renderer, &mut outline, 0.4);
	// 처음부터 그 행렬로 그린 새 렌더러와 같은 화면.
	let mut fresh_outline = ToonScreenOutline::new(gl).expect("화면 바깥선");
	let (fresh_pixels, _) = draw_frame(&mut renderer, &mut fresh_outline, 0.4);
	eprintln!("선의 판 묶음: 올림 처음 {} 멈춤 {} 옮김 {} 다시 멈춤 {}", first_upload, second_upload, moved_upload, still_upload);
	assert_eq!(first_upload, 2, "처음 프레임은 묶음 둘(상자 6, 공 3)을 올림");
	assert_eq!(second_upload, 0, "멈춘 프레임은 올림 0");
	assert!(second_pixels == first_pixels, "멈춘 프레임의 화면은 앞 프레임과 바이트까지 같다");
	assert_eq!(moved_upload, 1, "행렬 하나가 바뀌면 그 묶음 하나만 올림");
	assert!(moved_pixels != first_pixels, "옮긴 프레임은 화면이 바뀜");
	assert!(moved_pixels == fresh_pixels, "바뀐 프레임의 화면은 처음부터 그린 것과 바이트까지 같다");
	assert_eq!(still_upload, 0, "다시 멈추면 올림 0");
	assert!(still_pixels == moved_pixels, "다시 멈추면 화면 같음");
	destroy_outline(gl, outline, fresh_outline);
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
}

fn destroy_outline(gl: &GlContext, mut first: ToonScreenOutline, mut second: ToonScreenOutline) {
	first.destroy(gl);
	second.destroy(gl);
}

//==============================================================================
/// 셰이더 짓기 실패 알림. (원본 tests/graphics3d/toonshaderbuilderrorchrome.test.mjs 와 같은 검사)
/// 일부러 틀린 머티리얼 조각은 그래픽 오류 리스너에 한 번 알리고 기억해 다시 알리지 않는다. 컨텍스트를 잃은 동안의 실패는 알리지도 기억하지도 않아 되살린 뒤 다시 짓는다.
//==============================================================================
#[test]
fn shader_build_failure_is_reported_once_and_not_remembered_while_context_lost() {
	use crate::core::engine::{Engine, EngineConfiguration, GraphicError, GraphicErrorKind, GraphicErrorListener};
	use crate::ToonMaterialChunks;
	use std::cell::RefCell;
	use std::rc::Rc;
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let engine = Engine::new(EngineConfiguration::new());
	let received_list: Rc<RefCell<Vec<GraphicError>>> = Rc::new(RefCell::new(Vec::new()));
	let listener_received_list = received_list.clone();
	let graphic_error_listener: GraphicErrorListener = Rc::new(RefCell::new(move |graphic_error: &GraphicError| {
		listener_received_list.borrow_mut().push(graphic_error.clone());
	}));
	engine.add_graphic_error_listener(&graphic_error_listener);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let bad_material = renderer.create_material(ToonMaterialChunks { name: String::from("badMaterial"), surface_chunk: String::from("void applySurface(inout ToonSurface surface) { this is not glsl }"), ..Default::default() });
	let good_material = renderer.create_material(ToonMaterialChunks { name: String::from("goodMaterial"), surface_chunk: String::from("void applySurface(inout ToonSurface surface) { surface.baseColor *= 0.5; }"), ..Default::default() });
	assert!(!renderer.prepare_material(gl, Some(&bad_material), None));
	assert!(!renderer.prepare_material(gl, Some(&bad_material), None));
	assert!(renderer.prepare_material(gl, Some(&good_material), None));
	assert_eq!(received_list.borrow().len(), 1);
	assert_eq!(received_list.borrow()[0].kind, GraphicErrorKind::ShaderBuildFailed);
	assert_eq!(received_list.borrow()[0].name, "ToonRenderer.badMaterial");
	assert!(!received_list.borrow()[0].error.is_empty());
	// 컨텍스트를 잃은 동안의 실패. (다른 틀린 머티리얼, 알리지도 기억하지도 않는다)
	let lost_material = renderer.create_material(ToonMaterialChunks { name: String::from("lostMaterial"), surface_chunk: String::from("void applySurface(inout ToonSurface surface) { this is not glsl }"), ..Default::default() });
	GlContext::set_context_lost(true);
	assert!(!renderer.prepare_material(gl, Some(&lost_material), None));
	assert_eq!(received_list.borrow().len(), 1);
	GlContext::set_context_lost(false);
	assert!(!renderer.prepare_material(gl, Some(&lost_material), None));
	assert_eq!(received_list.borrow().len(), 2, "잃은 동안의 실패를 기억하지 않아 되살린 뒤 다시 지어 한 번 알린다");
	assert!(!renderer.prepare_material(gl, Some(&lost_material), None));
	assert_eq!(received_list.borrow().len(), 2);
	renderer.destroy(gl);
}

//==============================================================================
/// 몸 셰이더 기능 변형: 쓰지 않는 갈래를 뺀 몸 프로그램은 그리기 꼴마다 하나만 짓고, 화면은 모든 갈래를 켠 기본 프로그램과 바이트까지 같다.
/// - 기본 장면(텍스처 없음, 그림자 받음, 기본 빛)은 변형 하나, 그림자 받지 않는 그리기는 다른 변형 하나, 같은 조합은 다시 짓지 않는다.
//==============================================================================
#[test]
fn body_feature_variant_keeps_pixels() {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let box_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(0.8, 0.8, 0.8), "#c08060");
	let sphere_mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.4, 12, 8, std::f64::consts::PI), "#6080c0");
	let mut shadow_map = ShadowMap::new(gl, 256).expect("그림자 판");
	shadow_map.set_update_mode(ShadowUpdateMode::EveryFrame, 1);
	shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(-0.5, -1.0, -0.4), 6.0, 0.1, 20.0);
	DISABLE_FEATURE_VARIANTS.with(|flag| flag.set(true));
	let (base_pixels, _, _, _) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, Some(&shadow_map), false, None);
	assert_eq!(BUILT_FEATURE_VARIANTS.with(|count| count.get()), 0, "변형을 끄면 짓지 않는다");
	DISABLE_FEATURE_VARIANTS.with(|flag| flag.set(false));
	let (variant_pixels, _, _, _) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, Some(&shadow_map), false, None);
	let built_once = BUILT_FEATURE_VARIANTS.with(|count| count.get());
	let (again_pixels, _, _, _) = draw_scene(gl, &mut renderer, &target, &box_mesh, &sphere_mesh, Some(&shadow_map), false, None);
	eprintln!("몸 기능 변형: 지은 변형 {} 다시 그린 뒤 {}", built_once, BUILT_FEATURE_VARIANTS.with(|count| count.get()));
	print_hash("bodyvariant", &variant_pixels);
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
	assert_eq!(built_once, 1, "쓰지 않는 갈래를 뺀 몸 프로그램 하나를 짓는다");
	assert_eq!(BUILT_FEATURE_VARIANTS.with(|count| count.get()), built_once, "같은 조합은 다시 짓지 않는다");
	assert!(base_pixels.chunks(4).any(|pixel| pixel[0] != 26), "무엇인가 그려졌다");
	assert!(base_pixels == variant_pixels, "변형 프로그램 화면이 기본 프로그램과 바이트까지 같다");
	assert!(variant_pixels == again_pixels, "다시 그려도 같은 화면");
}

//==============================================================================
/// 프로그램 변형 미리 짓기 시험의 한 갈래 결과. (원본 크롬 시험 toonprogramprewarmchrome 의 plain, warmed, early, deleted)
//==============================================================================
struct PrewarmOutcome {
	/// 그리기 프레임이 지은 프로그램 수. (미리 짓기 호출은 뺀다, 둘째 프레임)
	frame_built: u32,
	second_frame_built: u32,
	/// 미리 짓기 호출이 지은 프로그램 수.
	prewarm_built: u32,
	hash: u64,
	second_hash: u64,
	was_ready_at_once: bool,
	is_ready: bool,
	pending_count: usize,
}

//==============================================================================
/// 렌더러 하나와 그것이 그릴 장면으로 한 갈래 돌리기. (몸 기능 비트가 다른 셋: 기본, 림 빛 묶음, 머티리얼)
/// - kind 0: 미리 짓기 없이, 1: 미리 짓고 끝난 뒤, 2: 미리 짓기가 끝나기 전에 그린다, 3: 미리 짓고 머티리얼을 지운 뒤 다시 미리 짓는다.
//==============================================================================
fn run_prewarm_scenario(gl: &GlContext, target: &Target, kind: u8) -> PrewarmOutcome {
	use crate::core::shaderprogram::BUILT_PROGRAM_COUNT;
	use crate::ToonMaterialChunks;
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.4, 12, 8, std::f64::consts::PI), "#c8b090");
	let material = renderer.create_material(ToonMaterialChunks { name: String::from("tint"), surface_chunk: String::from("void applySurface(inout ToonSurface surface) { surface.baseColor *= 0.8; }"), ..Default::default() });
	let mut rim_light_set = renderer.create_light_set();
	rim_light_set.set_rim_light([0.6, 0.65, 0.8], 2.5, 0.0, 1.0);
	let camera = create_camera();
	let mut queue = ToonDrawQueue::new();
	queue.add_mesh(&mesh, &Matrix4::create_translation(-1.2, 0.5, 0.0), ToonDrawOptions { casts_shadow: false, ..ToonDrawOptions::default() });
	queue.add_mesh(&mesh, &Matrix4::create_translation(1.2, 0.5, 0.0), ToonDrawOptions { casts_shadow: false, light_set: Some(&rim_light_set), ..ToonDrawOptions::default() });
	queue.add_mesh(&mesh, &Matrix4::create_translation(0.0, 0.5, 1.0), ToonDrawOptions { casts_shadow: false, material: Some(&material), ..ToonDrawOptions::default() });
	let draw_frame = |renderer: &mut ToonRenderer| -> (u32, u64) {
		target.clear();
		let before = BUILT_PROGRAM_COUNT.with(|count| count.get());
		queue.draw(gl, renderer, &camera);
		let built = BUILT_PROGRAM_COUNT.with(|count| count.get()) - before;
		(built, read_hash(&target.read_pixels()))
	};
	let mut outcome = PrewarmOutcome { frame_built: 0, second_frame_built: 0, prewarm_built: 0, hash: 0, second_hash: 0, was_ready_at_once: false, is_ready: false, pending_count: 0 };
	match kind {
		0 => {
			(outcome.frame_built, outcome.hash) = draw_frame(&mut renderer);
			(outcome.second_frame_built, outcome.second_hash) = draw_frame(&mut renderer);
			outcome.is_ready = true;
		}
		1 => {
			let before = BUILT_PROGRAM_COUNT.with(|count| count.get());
			let mut prewarm = renderer.prewarm_draw_queue(gl, &queue);
			outcome.was_ready_at_once = prewarm.is_ready();
			prewarm.wait(gl, &mut renderer);
			outcome.prewarm_built = BUILT_PROGRAM_COUNT.with(|count| count.get()) - before;
			outcome.is_ready = prewarm.is_ready();
			(outcome.frame_built, outcome.hash) = draw_frame(&mut renderer);
			(outcome.second_frame_built, outcome.second_hash) = draw_frame(&mut renderer);
		}
		2 => {
			let mut prewarm = renderer.prewarm_draw_queue(gl, &queue);
			outcome.was_ready_at_once = prewarm.is_ready();
			(outcome.frame_built, outcome.hash) = draw_frame(&mut renderer);
			prewarm.wait(gl, &mut renderer);
			outcome.is_ready = prewarm.is_ready();
			(outcome.second_frame_built, outcome.second_hash) = draw_frame(&mut renderer);
		}
		_ => {
			let _ = renderer.prewarm_draw_queue(gl, &queue);
			renderer.delete_material(gl, &material);
			let mut prewarm = renderer.prewarm_draw_queue(gl, &queue);
			prewarm.wait(gl, &mut renderer);
			outcome.is_ready = prewarm.is_ready();
			(outcome.frame_built, outcome.hash) = draw_frame(&mut renderer);
		}
	}
	outcome.pending_count = renderer.get_pending_build_count();
	renderer.destroy(gl);
	outcome
}

//==============================================================================
/// 프로그램 변형 미리 짓기: 미리 짓고 나면 첫 그리기 프레임이 프로그램을 짓지 않고(미리 짓기 없이는 짓는다), 끝나기 전에 그려도 화면이 같고, 머티리얼을 지워도 오류 없이 끝난다.
/// - 이 기계(맥 GL)는 병렬 컴파일 확장이 없어 호출 안에서 짓는 길을, FORCE_DEFERRED_PREWARM 으로 맡기는 길(UNIT 단위 한 걸음씩)을 둘 다 돌린다. 화면은 모두 바이트까지 같다.
//==============================================================================
#[test]
fn program_prewarm_removes_first_frame_builds_and_keeps_pixels() {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let is_parallel = crate::ShaderProgram::is_parallel_compile_supported(gl);
	let plain = run_prewarm_scenario(gl, &target, 0);
	assert!(plain.frame_built > 0, "미리 짓기 없이는 첫 프레임에 프로그램을 짓는다 ({})", plain.frame_built);
	assert_eq!(plain.second_frame_built, 0, "미리 짓기 없이도 둘째 프레임은 짓지 않는다");
	assert!(plain.hash == plain.second_hash);
	assert!(read_hash(&target.read_pixels()) != 0);
	for is_forced_deferred in [false, true] {
		FORCE_DEFERRED_PREWARM.with(|flag| flag.set(is_forced_deferred));
		let warmed = run_prewarm_scenario(gl, &target, 1);
		let early = run_prewarm_scenario(gl, &target, 2);
		let deleted = run_prewarm_scenario(gl, &target, 3);
		FORCE_DEFERRED_PREWARM.with(|flag| flag.set(false));
		eprintln!("미리 짓기(병렬 확장 {}, 맡기는 길 {}): 미리 짓기 없이 첫 프레임 짓기 {}, 미리 짓기가 지은 것 {}, 미리 짓기 뒤 첫 프레임 {}, 끝나기 전에 그린 프레임 {}", is_parallel, is_forced_deferred, plain.frame_built, warmed.prewarm_built, warmed.frame_built, early.frame_built);
		assert!(warmed.prewarm_built > 0, "미리 짓기가 프로그램을 지었다");
		assert_eq!(warmed.frame_built, 0, "미리 짓기 뒤 첫 프레임에 프로그램 짓기 0 번");
		assert_eq!(warmed.second_frame_built, 0, "미리 짓기 뒤 둘째 프레임도 0 번");
		assert!(warmed.hash == plain.hash && warmed.second_hash == plain.hash, "미리 지은 화면이 미리 짓기 없는 화면과 바이트까지 같다");
		assert!(early.hash == plain.hash, "끝나기 전에 그린 화면이 같다");
		assert!(early.second_hash == plain.hash && early.second_frame_built == 0, "끝난 뒤에도 같은 화면, 더 짓지 않는다");
		assert!(early.is_ready && warmed.is_ready && deleted.is_ready, "wait 가 끝난 뒤 is_ready");
		assert!(deleted.hash == plain.hash, "머티리얼을 지우고 다시 미리 지어도 같은 화면");
		for outcome in [&warmed, &early, &deleted] {
			assert_eq!(outcome.pending_count, 0, "맡긴 칸이 남지 않는다");
		}
		if is_forced_deferred || is_parallel {
			assert!(!warmed.was_ready_at_once, "맡기는 길은 맡기고 돌아온다 (한 걸음에 끝내는 수보다 변형이 많다)");
		}
		else {
			assert!(warmed.was_ready_at_once, "확장이 없으면 호출 안에서 끝");
		}
	}
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
}

//==============================================================================
/// 장면 중간에 빛 묶음 쪽 기능 비트가 바뀌는 장면 하나를 그리고 프레임마다 (지은 프로그램 수, 화면 해시) 를 돌려준다. (원본 크롬 시험 toonprogramprewarmchrome 의 5 번 runSteps)
/// - 림, 림에 음영 림, 램프 텍스처가 차례로 켜진다. 빛 묶음을 바꾸므로 프레임마다 그리기 줄을 다시 담는다.
/// - mode 0: 미리 짓기 없이 동기, 1: 지금 빛만 미리 짓고 동기, 2: light_feature_list 로 미리 짓고 동기, 3: light_set_list 로 미리 짓고 동기, 4: 미리 짓기 없이 맡기고 기본 몸으로(기본값, 맡기는 길 강제), 5: 4 와 같고 단계마다 지어질 때까지 한 번 더 그린다.
//==============================================================================
fn run_light_steps(gl: &GlContext, target: &Target, mode: u8) -> Vec<(u32, u64)> {
	use crate::core::shaderprogram::BUILT_PROGRAM_COUNT;
	use crate::ToonProgramPrewarmOptions;
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.9, 12, 8, std::f64::consts::PI), "#c8b090");
	let camera = create_camera();
	let mut light = renderer.create_light_set();
	let ramp_pixels: [u8; 16] = [60, 60, 70, 255, 120, 120, 130, 255, 200, 200, 210, 255, 255, 255, 255, 255];
	let ramp_texture = renderer.create_texture(gl, 4, 1, &ramp_pixels, crate::ToonTextureOptions { is_mipmapped: false, is_repeated: false, ..crate::ToonTextureOptions::default() }).expect("램프 텍스처");
	let mut future_light_list: Vec<crate::ToonLightSet> = Vec::new();
	for step in 1..4 {
		let mut future_light = renderer.create_light_set();
		future_light.set_rim_light([0.6, 0.65, 0.8], 2.5, 0.0, 1.0);
		if step >= 2 {
			future_light.set_shade_rim_light([0.3, 0.3, 0.5], 2.0);
		}
		if step >= 3 {
			future_light.set_ramp_texture(Some(ramp_texture), true);
		}
		future_light_list.push(future_light);
	}
	renderer.set_async_program_build(mode >= 4);
	FORCE_DEFERRED_PREWARM.with(|flag| flag.set(mode >= 4));
	let identity = Matrix4::create_identity();
	if mode >= 1 && mode <= 3 {
		let mut queue = ToonDrawQueue::new();
		queue.add_mesh(&mesh, &identity, ToonDrawOptions { casts_shadow: false, light_set: Some(&light), ..ToonDrawOptions::default() });
		let rim: &[&str] = &["rim"];
		let rim_shade_rim: &[&str] = &["rim", "shadeRim"];
		let rim_shade_rim_ramp: &[&str] = &["rim", "shadeRim", "rampTexture"];
		let feature_list: [&[&str]; 3] = [rim, rim_shade_rim, rim_shade_rim_ramp];
		let light_set_list: Vec<&crate::ToonLightSet> = future_light_list.iter().collect();
		let options = match mode {
			2 => ToonProgramPrewarmOptions { light_feature_list: &feature_list, ..ToonProgramPrewarmOptions::default() },
			3 => ToonProgramPrewarmOptions { light_set_list: &light_set_list, ..ToonProgramPrewarmOptions::default() },
			_ => ToonProgramPrewarmOptions::default(),
		};
		let mut prewarm = renderer.prewarm_draw_queue_with_options(gl, &queue, &options).expect("미리 짓기");
		prewarm.wait(gl, &mut renderer);
	}
	let mut frame_list: Vec<(u32, u64)> = Vec::new();
	for step in 0..4 {
		match step {
			1 => {
				light.set_rim_light([0.6, 0.65, 0.8], 2.5, 0.0, 1.0);
			}
			2 => {
				light.set_shade_rim_light([0.3, 0.3, 0.5], 2.0);
			}
			3 => {
				light.set_ramp_texture(Some(ramp_texture), true);
			}
			_ => {}
		}
		let draw_frame = |renderer: &mut ToonRenderer| -> (u32, u64) {
			let mut queue = ToonDrawQueue::new();
			queue.add_mesh(&mesh, &identity, ToonDrawOptions { casts_shadow: false, light_set: Some(&light), ..ToonDrawOptions::default() });
			target.clear();
			let before = BUILT_PROGRAM_COUNT.with(|count| count.get());
			queue.draw(gl, renderer, &camera);
			(BUILT_PROGRAM_COUNT.with(|count| count.get()) - before, read_hash(&target.read_pixels()))
		};
		if mode == 5 {
			draw_frame(&mut renderer); // 처음 보는 변형이면 맡기고 기본 몸으로 그린다.
		}
		frame_list.push(draw_frame(&mut renderer));
	}
	FORCE_DEFERRED_PREWARM.with(|flag| flag.set(false));
	renderer.destroy(gl);
	frame_list
}

//==============================================================================
/// 미리 짓기 인자와 맡기고 기본 몸으로 그리기: 장면 중간에 빛 묶음이 바뀌어도 조합 목록으로 미리 지으면 프레임마다 짓기 0 번이고, 맡기고 기본 몸으로 그린 프레임도 화면이 바이트까지 같다. (원본 이슈 2)
//==============================================================================
#[test]
fn prewarm_options_and_async_program_build_keep_pixels() {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let sync_list = run_light_steps(gl, &target, 0);
	let narrow_list = run_light_steps(gl, &target, 1);
	let listed_list = run_light_steps(gl, &target, 2);
	let set_list = run_light_steps(gl, &target, 3);
	let async_list = run_light_steps(gl, &target, 4);
	let settled_list = run_light_steps(gl, &target, 5);
	let built = |list: &Vec<(u32, u64)>| -> Vec<u32> { list.iter().map(|frame| frame.0).collect() };
	let hash = |list: &Vec<(u32, u64)>| -> Vec<u64> { list.iter().map(|frame| frame.1).collect() };
	eprintln!("빛 묶음이 바뀌는 프레임별 짓기 수: 미리 짓기 없이 {:?}, 지금 값만 {:?}, 조합 목록 {:?}, 빛 묶음 목록 {:?}, 맡기고 기본 몸으로 {:?}, 지어질 때까지 한 번 더 {:?}", built(&sync_list), built(&narrow_list), built(&listed_list), built(&set_list), built(&async_list), built(&settled_list));
	assert!(built(&sync_list).iter().skip(1).all(|count| *count > 0), "미리 짓기 없이는 빛이 바뀔 때마다 짓는다");
	assert!(built(&narrow_list).iter().skip(1).all(|count| *count > 0), "지금 빛 값만 미리 지으면 장면 중간에 켜진 조합은 그때 짓는다");
	assert_eq!(built(&listed_list), vec![0, 0, 0, 0], "light_feature_list 로 미리 지으면 프레임마다 짓기 0 번");
	assert_eq!(built(&set_list), vec![0, 0, 0, 0], "light_set_list 로 미리 지으면 프레임마다 짓기 0 번");
	assert_eq!(hash(&listed_list), hash(&sync_list), "조합 목록이 있어도 화면은 바이트까지 같다");
	assert_eq!(hash(&set_list), hash(&sync_list));
	assert_eq!(hash(&narrow_list), hash(&sync_list));
	assert_eq!(hash(&async_list), hash(&sync_list), "기본 몸으로 그린 프레임이 짓고 그린 화면과 바이트까지 같다");
	assert_eq!(hash(&settled_list), hash(&sync_list), "지어진 뒤 변형으로 바뀌어도 같은 화면");
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	assert!(renderer.is_async_program_build(), "맡기고 기본 몸으로 그리기가 기본");
	renderer.set_async_program_build(false);
	assert!(!renderer.is_async_program_build());
	let queue = ToonDrawQueue::new();
	let bad_name: &[&str] = &["noSuchFeature"];
	let bad_list: [&[&str]; 1] = [bad_name];
	let error = renderer.prewarm_draw_queue_with_options(gl, &queue, &crate::ToonProgramPrewarmOptions { light_feature_list: &bad_list, ..crate::ToonProgramPrewarmOptions::default() }).err().expect("잘못된 이름은 Err");
	assert!(error.contains("noSuchFeature"), "{}", error);
	renderer.destroy(gl);
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
}
