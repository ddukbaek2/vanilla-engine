//==============================================================================
//! 툰 재질 블록과 자동 인스턴싱 시험. (원본 vanilla.js tests/graphics3d/toonuniformblockchrome, toonautoinstancechrome 과 같은 검사)
//! - 숨긴 GL 컨텍스트(창을 띄우지 않는다)에서 장면을 128 x 128 프레임버퍼에 그린다.
//! - 화면 해시는 TOON_BATCH_HASH 가 있으면 줄마다 `해시 이름 값` 으로 찍는다. (고치기 전 코드와 견주기)
//==============================================================================
use super::toondrawqueue::{ToonDrawQueue, ToonDrawStage};
use super::toonrenderer::{ToonDrawOptions, ToonMesh, ToonRenderer, UPLOAD_COUNTS};
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
		raw.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, SIZE, SIZE, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		let depth = raw.create_renderbuffer().unwrap();
		raw.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
		raw.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, SIZE, SIZE);
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
