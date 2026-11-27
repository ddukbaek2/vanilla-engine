//==============================================================================
//! 그림자 캐시가 깊이에 닿지 않는 값(바탕색 텍스처)에 다시 굽지 않는지 보는 찍기 시험. (원본 tests/graphics3d/shadowcasterorderchrome.test.mjs 와 같은 내용)
//! - Engine::new 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 같은 모형 셋을 나눠 쓰는 드리우개 스물넷을 해 그림자 판 두 벌(EveryFrame, OnChange)에 그린다.
//!   꼴 하나의 바탕색 텍스처가 매 프레임 바뀐다. (러스트 정렬 번호가 재질, 모형, 바탕색 텍스처 차례라 불투명한 것의 정렬 차례가 바뀐다, 원본은 재질 블록)
//!   바탕색 텍스처는 그림자 깊이에 닿지 않으므로 OnChange 는 다시 굽지 않아야 한다. (드리우개는 물체마다 변환, 모형, 뼈대로 견준다)
//! - 40 ~ 79 프레임(텍스처만 바뀜)은 드리우개가 모두 구운 것이고 그림자 깊이를 하나도 그리지 않는다. (고치기 전에는 매 프레임 다시 그렸다)
//!   80 프레임에 구운 상자 하나가 움직이기 시작하면 그 프레임에 다시 굽는다. 81 ~ 109 프레임은 다시 굽지 않고 그 상자만 덧그린다.
//! - 프레임마다 두 벌로 장면을 그려 화소가 모두 같은지 본다. (OnChange 의 그림자가 EveryFrame 과 같다)
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test shadowcasterorder
//==============================================================================
use vanilla::{create_box_geometry, create_octahedron_geometry, create_sphere_geometry, Camera3D, Engine, EngineConfiguration, GlContext, Matrix4, MeshBuilder, ShadowMap, ShadowUpdateMode, ToonDrawOptions, ToonDrawQueue, ToonGeometry, ToonMesh, ToonPartOptions, ToonRenderer, ToonTextureOptions, Vector3};

const WIDTH: usize = 200;
const HEIGHT: usize = 150;
const FRAME_COUNT: usize = 120;

fn check(check_count: &mut u32, condition: bool, message: &str) {
	*check_count += 1;
	if !condition {
		panic!("시험 실패: {}", message);
	}
}

fn create_mesh(gl: &GlContext, renderer: &mut ToonRenderer, geometry: &ToonGeometry, color: &str) -> ToonMesh {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, &Matrix4::create_identity(), color, ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
	renderer.create_mesh(gl, &builder.read_arrays()).expect("모형")
}

fn read_frame(gl: &GlContext) -> Vec<u8> {
	let mut pixels = vec![0u8; WIDTH * HEIGHT * 4];
	unsafe {
		gl.read_pixels(0, 0, WIDTH as i32, HEIGHT as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
	}
	pixels
}

fn main() {
	let mut check_count = 0u32;
	let engine = Engine::new(EngineConfiguration::new());
	let graphic = engine.get_graphic();
	check(&mut check_count, graphic.is_some(), "GL 컨텍스트를 짓지 못함 (화면 서버)");
	let graphic = graphic.unwrap();
	let gl = graphic.get_gl();
	let (framebuffer, color_texture, depth_renderbuffer) = unsafe {
		let color_texture = gl.create_texture().unwrap();
		gl.bind_texture(glow::TEXTURE_2D, Some(color_texture));
		gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, WIDTH as i32, HEIGHT as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
		gl.bind_texture(glow::TEXTURE_2D, None);
		let depth_renderbuffer = gl.create_renderbuffer().unwrap();
		gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_renderbuffer));
		gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, WIDTH as i32, HEIGHT as i32);
		gl.bind_renderbuffer(glow::RENDERBUFFER, None);
		let framebuffer = gl.create_framebuffer().unwrap();
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
		gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth_renderbuffer));
		assert_eq!(gl.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE, "시험 실패: 프레임버퍼를 짓지 못함");
		(framebuffer, color_texture, depth_renderbuffer)
	};

	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let ground_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(10.0, 0.2, 10.0), "#a0a8b0");
	let mesh_list = [
		create_mesh(gl, &mut renderer, &create_box_geometry(0.5, 0.9, 0.5), "#e0a070"),
		create_mesh(gl, &mut renderer, &create_sphere_geometry(0.3, 16, 12, std::f64::consts::PI), "#80c0e0"),
		create_mesh(gl, &mut renderer, &create_octahedron_geometry(0.3), "#c0b090"),
	];
	let pattern: Vec<u8> = (0..8 * 8).flat_map(|index| if index % 9 < 4 { [224, 160, 112, 255] } else { [96, 48, 32, 255] }).collect();
	let texture = renderer.create_texture(gl, 8, 8, &pattern, ToonTextureOptions { is_mipmapped: false, ..ToonTextureOptions::default() }).expect("텍스처");
	let mut camera = Camera3D::new();
	camera.set_perspective(0.9, 0.1, 50.0);
	camera.set_aspect_ratio(WIDTH as f64 / HEIGHT as f64);
	camera.look_at(Vector3::create(0.0, 6.0, 7.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	let mut every_shadow_map = ShadowMap::new(gl, 512).expect("그림자 판");
	let mut change_shadow_map = ShadowMap::new(gl, 512).expect("그림자 판");
	every_shadow_map.set_update_mode(ShadowUpdateMode::EveryFrame, 1);
	change_shadow_map.set_update_mode(ShadowUpdateMode::OnChange, 1);
	let sun_direction = Vector3::create(-0.5, -1.0, -0.4);
	every_shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), sun_direction, 6.0, 0.1, 20.0);
	change_shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), sun_direction, 6.0, 0.1, 20.0);

	let render_scene = |renderer: &mut ToonRenderer, queue: &ToonDrawQueue, shadow_map: &ShadowMap| -> Vec<u8> {
		renderer.set_shadow(Some(shadow_map), 0.7);
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.viewport(0, 0, WIDTH as i32, HEIGHT as i32);
			gl.clear_color(0.1, 0.1, 0.1, 1.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
		queue.draw(gl, renderer, &camera);
		read_frame(gl)
	};

	// (그림자 깊이를 그린 수, 화면이 같은지, 장면이 그려진 화소 수, GL 오류)
	let mut frame_list: Vec<(u32, bool, usize, u32)> = Vec::new();
	for frame in 0..FRAME_COUNT {
		let mut queue = ToonDrawQueue::new();
		queue.add_mesh(&ground_mesh, &Matrix4::create_translation(0.0, -0.1, 0.0), ToonDrawOptions::default());
		for index in 0..24usize {
			let column = index % 6;
			let row = index / 6;
			// 7 번(구운 상자)은 80 ~ 84 프레임에 움직이고 그 뒤 선다.
			let offset = if index == 7 && frame >= 80 { (frame - 79).min(5) as f64 * 0.1 } else { 0.0 };
			// 꼴 0 의 바탕색 텍스처가 매 프레임 있다 없다 한다. (같은 모형을 쓰는 것끼리 정렬 차례가 바뀐다)
			let base_color_texture = if index % 4 == 0 && (frame + index / 4) % 2 == 0 { Some(texture) } else { None };
			queue.add_mesh(&mesh_list[index % 3], &Matrix4::create_translation(-3.0 + column as f64 * 1.2 + offset, 0.45, -2.0 + row as f64 * 1.3), ToonDrawOptions { base_color_texture, ..ToonDrawOptions::default() });
		}
		queue.draw_shadows(gl, &mut renderer, &every_shadow_map);
		let every_pixels = render_scene(&mut renderer, &queue, &every_shadow_map);
		let before = renderer.get_frame_statistics().draw_call_count;
		queue.draw_shadows(gl, &mut renderer, &change_shadow_map);
		let draw_count = renderer.get_frame_statistics().draw_call_count - before;
		let change_pixels = render_scene(&mut renderer, &queue, &change_shadow_map);
		let lit_count = every_pixels.chunks(4).filter(|pixel| pixel[0] as u32 + pixel[1] as u32 + pixel[2] as u32 > 120).count();
		let error = unsafe { gl.get_error() };
		frame_list.push((draw_count, every_pixels == change_pixels, lit_count, error));
	}
	every_shadow_map.destroy(gl);
	change_shadow_map.destroy(gl);
	unsafe {
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		gl.delete_framebuffer(framebuffer);
		gl.delete_renderbuffer(depth_renderbuffer);
		gl.delete_texture(color_texture);
	}
	println!("  OnChange 그림자 그리기 수: {:?}", frame_list.iter().map(|frame| frame.0).collect::<Vec<u32>>());

	for (frame, result) in frame_list.iter().enumerate() {
		check(&mut check_count, result.3 == 0, &format!("{} 프레임 GL 오류 {}", frame, result.3));
		check(&mut check_count, result.2 > 1000, &format!("{} 프레임 장면이 그려짐 ({})", frame, result.2));
		check(&mut check_count, result.1, &format!("{} 프레임 OnChange 화면이 EveryFrame 과 같음", frame));
	}
	// 텍스처만 바뀌는 프레임은 그림자 깊이를 하나도 그리지 않는다. (드리우개 스물다섯이 모두 구운 것)
	for frame in 40..80 {
		check(&mut check_count, frame_list[frame].0 == 0, &format!("{} 프레임 OnChange 가 그림자 깊이를 그리지 않음 ({})", frame, frame_list[frame].0));
	}
	// 구운 상자가 움직이기 시작한 프레임은 다시 굽고, 그 뒤 선 프레임은 움직인 상자만 덧그린다. (구운 판을 옮기고 상자 하나)
	check(&mut check_count, frame_list[80].0 > 1, &format!("구운 상자가 움직인 80 프레임에 다시 구움 ({})", frame_list[80].0));
	for frame in 81..110 {
		check(&mut check_count, frame_list[frame].0 == 1, &format!("{} 프레임 움직인 상자만 덧그림 ({})", frame, frame_list[frame].0));
	}
	println!("그림자 드리우개 차례 찍기 시험 통과 ({} 검사)", check_count);
}
