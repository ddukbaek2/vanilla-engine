//==============================================================================
//! 그림자 갱신 방식의 찍기 시험. (원본 tests/graphics3d/shadowcachechrome.test.mjs 와 같은 장면, 같은 검사)
//! - Engine::new 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 160 x 120 프레임버퍼(RGBA8, 깊이 24, MSAA 없음)에 그린다.
//!   같은 그리기 줄을 해 그림자와 점광 그림자 판 두 벌에 그린다. 한 벌은 EveryFrame, 한 벌은 OnChange.
//!   상자는 움직이다 서고(구워질 만큼 오래) 다시 움직이고, 인스턴스 하나가 한 번 옮겨 가고, 해와 점광이 한 번씩 움직이고, 모형 하나가 빠졌다 돌아온다.
//!   프레임마다 두 벌로 장면을 그려 화소가 모두 같은지 본다. (OnChange 의 그림자가 한 프레임도 늦게 따라오지 않는다)
//! - 모두 그대로인 프레임은 OnChange 가 그림자 깊이를 하나도 그리지 않고, 움직이는 것만 있는 프레임은 그것만 그리는지 센다. (렌더러의 드로우콜 셈)
//! - 가위 시험을 켠 채(2D 자르기 안의 바깥 패스처럼) begin_render 를 불러도 해 그림자 판과 점광 판 모두 가위를 끄는지 본다.
//! - Interval 4 는 0, 4, 8 프레임에만 그리고 그 프레임의 화면이 EveryFrame 과 같은지 본다.
//! - 원본에 없는 것: 110 프레임 뒤 request_update 한 프레임은 OnChange 가 구운 판까지 전부(EveryFrame 과 같은 수) 다시 그리고, 그다음 프레임은 움직이는 것만 그리는지 본다.
//! - 찍은 60 프레임(상자 섬, 구운 판과 덧그림 없음)과 65 프레임(상자 움직임) 화면을 VANILLA_SHADOW_CACHE_DUMP 폴더에 RGBA 날것으로 남긴다. (주면, 원본과 견줄 때)
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test shadowcache
//==============================================================================
use vanilla::{create_box_geometry, create_octahedron_geometry, create_sphere_geometry, Camera3D, Engine, EngineConfiguration, GlContext, Matrix4, MeshBuilder, ShadowAtlas, ShadowMap, ShadowUpdateMode, ToonDrawOptions, ToonDrawQueue, ToonGeometry, ToonInstance, ToonMesh, ToonPartOptions, ToonPointLight, ToonRenderer, Vector3, DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION};

/// 찍는 너비. (원본 캔버스와 같다)
const WIDTH: usize = 160;
/// 찍는 높이.
const HEIGHT: usize = 120;
/// 그리는 프레임 수.
const FRAME_COUNT: usize = 110;

//==============================================================================
/// 검사 하나. (어기면 글과 함께 멈춘다)
//==============================================================================
fn check(check_count: &mut u32, condition: bool, message: &str) {
	*check_count += 1;
	if !condition {
		panic!("시험 실패: {}", message);
	}
}

//==============================================================================
/// 프레임 하나에서 읽은 것.
//==============================================================================
struct FrameResult {
	is_same: bool,
	is_interval_same: Option<bool>,
	change_draw_count: u32,
	interval_draw_count: u32,
	lit_count: usize,
	error: u32,
}

//==============================================================================
/// 모형 하나 짓기. (바깥선 없음)
//==============================================================================
fn create_mesh(gl: &GlContext, renderer: &mut ToonRenderer, geometry: &ToonGeometry, color: &str) -> ToonMesh {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, &Matrix4::create_identity(), color, ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
	renderer.create_mesh(gl, &builder.read_arrays()).expect("모형")
}

//==============================================================================
/// 판 읽기.
//==============================================================================
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
	let dump_path = std::env::var("VANILLA_SHADOW_CACHE_DUMP").ok();

	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let ground_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(8.0, 0.2, 8.0), "#a0a8b0");
	let box_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(0.6, 0.8, 0.6), "#e0a070");
	let ball_mesh = create_mesh(gl, &mut renderer, &create_sphere_geometry(0.35, 16, 12, std::f64::consts::PI), "#80c0e0");
	let pebble_mesh = create_mesh(gl, &mut renderer, &create_octahedron_geometry(0.2), "#c0b090");
	let mut camera = Camera3D::new();
	camera.set_perspective(0.9, 0.1, 50.0);
	camera.set_aspect_ratio(WIDTH as f64 / HEIGHT as f64);
	camera.look_at(Vector3::create(0.0, 4.0, 5.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	let mut every_shadow_map = ShadowMap::new(gl, 512).expect("그림자 판");
	let mut change_shadow_map = ShadowMap::new(gl, 512).expect("그림자 판");
	let mut interval_shadow_map = ShadowMap::new(gl, 512).expect("그림자 판");
	every_shadow_map.set_update_mode(ShadowUpdateMode::EveryFrame, 1);
	change_shadow_map.set_update_mode(ShadowUpdateMode::OnChange, 1);
	interval_shadow_map.set_update_mode(ShadowUpdateMode::Interval, 4);
	let mut every_atlas = ShadowAtlas::new(gl, 512, DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION).expect("그림자 아틀라스");
	let mut change_atlas = ShadowAtlas::new(gl, 512, DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION).expect("그림자 아틀라스");
	every_atlas.set_update_mode(ShadowUpdateMode::EveryFrame, 1);
	check(&mut check_count, change_atlas.get_update_mode() == ShadowUpdateMode::OnChange && change_atlas.get_update_interval() == 1, "갱신 방식 기본은 OnChange, 1");
	check(&mut check_count, interval_shadow_map.get_update_mode() == ShadowUpdateMode::Interval && interval_shadow_map.get_update_interval() == 4, "Interval 4 를 돌려줌");

	// 판에 장면 그리기. (가져온 그림자 판으로 받는다)
	let render_scene = |renderer: &mut ToonRenderer, queue: &ToonDrawQueue, shadow_map: &ShadowMap, shadow_atlas: &ShadowAtlas| -> Vec<u8> {
		renderer.set_shadow(Some(shadow_map), 0.7);
		renderer.set_local_shadows(Some(shadow_atlas), 1.0);
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.viewport(0, 0, WIDTH as i32, HEIGHT as i32);
			gl.clear_color(0.1, 0.1, 0.1, 1.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
		queue.draw(gl, renderer, &camera);
		read_frame(gl)
	};
	// 그림자 깊이를 그린 수. (렌더러의 드로우콜 셈이 그림자 패스에서 는 몫)
	let count_draws = |renderer: &mut ToonRenderer, draw: &mut dyn FnMut(&mut ToonRenderer)| -> u32 {
		let before = renderer.get_frame_statistics().draw_call_count;
		draw(renderer);
		renderer.get_frame_statistics().draw_call_count - before
	};

	let mut frame_list: Vec<FrameResult> = Vec::new();
	let mut light_position = [-1.5, 2.5, 1.0];
	let mut sun_direction = Vector3::create(-0.5, -1.0, -0.4);
	let mut pebble_list: Vec<ToonInstance> = Vec::new();
	let mut request_counts = (0u32, 0u32, 0u32);
	for frame in 0..=FRAME_COUNT + 1 {
		// 상자: 0 ~ 9 움직임, 10 ~ 59 섬(40 에 구워짐), 60 ~ 69 움직임, 그 뒤 섬.
		let box_x = if frame < 10 {
			-1.0 + frame as f64 * 0.1
		}
		else if frame < 60 {
			0.0
		}
		else if frame < 70 {
			(frame - 60) as f64 * 0.08
		}
		else {
			0.8
		};
		// 인스턴스 하나는 90 에 옮겨 간다. 공은 95 ~ 99 에 빠진다.
		pebble_list.clear();
		for index in 0..5 {
			let offset = if index == 2 && frame >= 90 {
				0.6
			}
			else {
				0.0
			};
			pebble_list.push(ToonInstance { model_matrix: Matrix4::create_translation(-2.0 + index as f64, 0.2, 1.5 + offset), tint_color: [1.0, 1.0, 1.0, 1.0], ..ToonInstance::default() });
		}
		if frame == 75 {
			sun_direction = Vector3::create(0.4, -1.0, -0.5);
		}
		if frame == 82 {
			light_position = [1.2, 2.2, 0.5];
		}
		renderer.set_point_lights(&[ToonPointLight { position: light_position, color: [1.0, 0.9, 0.7], intensity: 3.0, range: 8.0, casts_shadow: true, ..ToonPointLight::default() }]);
		let light_source_list = renderer.get_light_set().read_shadow_light_sources();
		every_atlas.update_lights(&light_source_list);
		change_atlas.update_lights(&light_source_list);
		every_shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), sun_direction, 4.0, 0.1, 20.0);
		change_shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), sun_direction, 4.0, 0.1, 20.0);
		interval_shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), sun_direction, 4.0, 0.1, 20.0);
		let ground_matrix = Matrix4::create_translation(0.0, -0.1, 0.0);
		let box_matrix = Matrix4::create_translation(box_x, 0.4, 0.0);
		let ball_matrix = Matrix4::create_translation(-1.0, 0.35, -1.0);
		let mut queue = ToonDrawQueue::new();
		queue.add_mesh(&ground_mesh, &ground_matrix, ToonDrawOptions::default());
		queue.add_mesh(&box_mesh, &box_matrix, ToonDrawOptions::default());
		if !(95..100).contains(&frame) {
			queue.add_mesh(&ball_mesh, &ball_matrix, ToonDrawOptions::default());
		}
		queue.add_mesh_instanced(&pebble_mesh, &pebble_list, ToonDrawOptions::default());
		if frame >= FRAME_COUNT {
			// 원본에 없는 것: request_update 한 프레임과 그다음 프레임.
			if frame == FRAME_COUNT {
				change_shadow_map.request_update();
				change_atlas.request_update();
				request_counts.2 = count_draws(&mut renderer, &mut |renderer| {
					queue.draw_shadows(gl, renderer, &every_shadow_map);
					queue.draw_local_shadows(gl, renderer, &every_atlas);
				});
			}
			let draw_count = count_draws(&mut renderer, &mut |renderer| {
				queue.draw_shadows(gl, renderer, &change_shadow_map);
				queue.draw_local_shadows(gl, renderer, &change_atlas);
			});
			if frame == FRAME_COUNT {
				request_counts.0 = draw_count;
			}
			else {
				request_counts.1 = draw_count;
			}
			continue;
		}
		queue.draw_shadows(gl, &mut renderer, &every_shadow_map);
		queue.draw_local_shadows(gl, &mut renderer, &every_atlas);
		let every_pixels = render_scene(&mut renderer, &queue, &every_shadow_map, &every_atlas);
		let change_draw_count = count_draws(&mut renderer, &mut |renderer| {
			queue.draw_shadows(gl, renderer, &change_shadow_map);
			queue.draw_local_shadows(gl, renderer, &change_atlas);
		});
		let change_pixels = render_scene(&mut renderer, &queue, &change_shadow_map, &change_atlas);
		let interval_draw_count = count_draws(&mut renderer, &mut |renderer| {
			queue.draw_shadows(gl, renderer, &interval_shadow_map);
		});
		let interval_pixels = if frame % 4 == 0 {
			Some(render_scene(&mut renderer, &queue, &interval_shadow_map, &every_atlas))
		}
		else {
			None
		};
		let lit_count = every_pixels.chunks(4).filter(|pixel| pixel[0] as u32 + pixel[1] as u32 + pixel[2] as u32 > 120).count();
		if let Some(dump_path) = dump_path.as_ref() {
			if frame == 60 || frame == 65 {
				std::fs::write(format!("{}/rust_frame{}.rgba", dump_path, frame), &change_pixels).expect("화면 남기기");
			}
		}
		let error = unsafe { gl.get_error() };
		frame_list.push(FrameResult { is_same: every_pixels == change_pixels, is_interval_same: interval_pixels.map(|pixels| pixels == every_pixels), change_draw_count, interval_draw_count, lit_count, error });
	}

	// 가위 시험을 켠 채 begin_render. (판 전체를 비우고 그려야 한다, 해 그림자 판과 점광 판 같게)
	let mut scissor_result = [true, true];
	for (index, scissor_slot) in scissor_result.iter_mut().enumerate() {
		unsafe {
			gl.enable(glow::SCISSOR_TEST);
			gl.scissor(0, 0, 8, 8);
		}
		if index == 0 {
			every_shadow_map.begin_render(gl);
		}
		else {
			every_atlas.begin_render(gl);
		}
		unsafe {
			*scissor_slot = gl.is_enabled(glow::SCISSOR_TEST);
			gl.disable(glow::SCISSOR_TEST);
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		}
	}
	for shadow_map in [&every_shadow_map, &change_shadow_map, &interval_shadow_map] {
		shadow_map.destroy(gl);
	}
	every_atlas.destroy(gl);
	change_atlas.destroy(gl);
	unsafe {
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		gl.delete_framebuffer(framebuffer);
		gl.delete_renderbuffer(depth_renderbuffer);
		gl.delete_texture(color_texture);
	}

	let draw_count_list: Vec<u32> = frame_list.iter().map(|frame| frame.change_draw_count).collect();
	println!("  OnChange 그림자 그리기 수 (프레임 0 ~ {}): {:?}", FRAME_COUNT - 1, draw_count_list);
	println!("  request_update 프레임 {} (EveryFrame {}), 그다음 {}", request_counts.0, request_counts.2, request_counts.1);
	for (frame, result) in frame_list.iter().enumerate() {
		check(&mut check_count, result.error == 0, &format!("{} 프레임 GL 오류 {}", frame, result.error));
		check(&mut check_count, result.lit_count > 1000, &format!("{} 프레임 장면이 그려짐 ({})", frame, result.lit_count));
		check(&mut check_count, result.is_same, &format!("{} 프레임 OnChange 화면이 EveryFrame 과 같음", frame));
		if let Some(is_interval_same) = result.is_interval_same {
			check(&mut check_count, is_interval_same, &format!("{} 프레임 Interval 화면이 EveryFrame 과 같음", frame));
		}
		check(&mut check_count, (result.interval_draw_count > 0) == (frame % 4 == 0), &format!("{} 프레임 Interval 4 는 네 프레임마다만 그림 ({})", frame, result.interval_draw_count));
	}
	// 다 그대로인 프레임(45 ~ 59, 상자가 40 에 구워진 뒤)은 하나도 그리지 않는다.
	for frame in 45..60 {
		check(&mut check_count, frame_list[frame].change_draw_count == 0, &format!("{} 프레임 OnChange 가 그리지 않음 ({})", frame, frame_list[frame].change_draw_count));
	}
	// 상자만 움직이는 프레임(62 ~ 69)은 상자만 그린다. (해 하나와 점광 여섯 칸, 칸 밖은 걸러져 그보다 적을 수 있다)
	for frame in 62..70 {
		check(&mut check_count, frame_list[frame].change_draw_count > 0 && frame_list[frame].change_draw_count <= 7, &format!("{} 프레임 OnChange 는 움직이는 것만 그림 ({})", frame, frame_list[frame].change_draw_count));
	}
	// 첫 프레임은 전부 그린다.
	check(&mut check_count, frame_list[0].change_draw_count > 7, &format!("첫 프레임은 전부 그림 ({})", frame_list[0].change_draw_count));
	// request_update 프레임은 구운 판까지 전부 다시 그리고, 그다음은 움직이는 것(100 에 돌아온 공, 130 에 구워진다)만 109 프레임처럼 그린다.
	check(&mut check_count, request_counts.0 == request_counts.2 && request_counts.0 > 7, &format!("request_update 프레임은 전부 그림 ({}, EveryFrame {})", request_counts.0, request_counts.2));
	check(&mut check_count, request_counts.1 == frame_list[FRAME_COUNT - 1].change_draw_count && request_counts.1 < request_counts.0, &format!("request_update 다음 프레임은 움직이는 것만 그림 ({}, 109 프레임 {})", request_counts.1, frame_list[FRAME_COUNT - 1].change_draw_count));
	check(&mut check_count, !scissor_result[0], "ShadowMap::begin_render 가 가위 시험을 끔");
	check(&mut check_count, !scissor_result[1], "ShadowAtlas::begin_render 가 가위 시험을 끔");
	println!("그림자 갱신 방식 찍기 시험 통과 ({} 검사)", check_count);
}
