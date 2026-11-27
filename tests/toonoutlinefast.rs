//==============================================================================
//! 툰 화면 바깥선 가는 선 길, 배율, 판 앞 비우기의 찍기 시험. (원본 vanilla.js dev 0c5bd2b 의 툰 화면 바깥선 고침과 같은 검사)
//! - Engine::new 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 방 구석, 상자, 로봇을 ToonDrawQueue 와 ToonPostProcess 로 640 x 360 프레임버퍼에 그린다.
//! - 길: 굵기가 약 3.24 선의 판 픽셀(굵기 x 배율) 이하면 가는 선 길(씨앗 쪽 판 R8UI 하나와 섞기 하나, 씨앗과 넓히기 판은 짓지 않음), 넘으면 점프 플러딩 길.
//!   배율이 1 보다 작은 가는 선은 선의 판 해상도에서 덮는 몫 판(RGBA8)을 셈해 쌍선형으로 한 번 읽는다. 선의 판은 그리는 영역 x 배율로 줄어든다.
//! - TOON_OUTLINE_PICTURE_PATH 를 주면 경우마다 화면을 그 폴더에 .rgba(왼쪽 아래 원점, 640 x 360)로 남긴다. (두 엔진 화면 견주기)
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test toonoutlinefast
//==============================================================================
use vanilla::{create_box_geometry, create_cylinder_geometry, create_sphere_geometry, Engine, EngineConfiguration, GlContext, Matrix4, MeshBuilder, ToonDrawOptions, ToonDrawQueue, ToonGeometry, ToonMesh, ToonOutlinePath, ToonOutlineWidthUnit, ToonPartOptions, ToonPostProcess, ToonPostProcessOptions, ToonRenderer, ToonScreenOutline, Vector3};

//==============================================================================
/// 검사 하나. (어기면 글과 함께 멈춘다)
//==============================================================================
fn check(check_count: &mut u32, condition: bool, message: &str) {
	*check_count += 1;
	if !condition {
		panic!("시험 실패: {}", message);
	}
}

/// 찍는 너비. (원본 toonambientocclusionchrome 과 같다)
const WIDTH: usize = 640;
/// 찍는 높이.
const HEIGHT: usize = 360;

//==============================================================================
/// 찍을 판. (RGBA8 색 텍스처, 깊이 24 렌더버퍼, MSAA 없음)
//==============================================================================
struct CaptureTarget {
	framebuffer: glow::Framebuffer,
	color_texture: glow::Texture,
	depth_renderbuffer: glow::Renderbuffer,
}

//==============================================================================
/// 찍을 판 짓기.
//==============================================================================
fn create_capture_target(gl: &GlContext, width: usize, height: usize) -> CaptureTarget {
	unsafe {
		let color_texture = gl.create_texture().unwrap();
		gl.bind_texture(glow::TEXTURE_2D, Some(color_texture));
		gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, width as i32, height as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
		gl.bind_texture(glow::TEXTURE_2D, None);
		let depth_renderbuffer = gl.create_renderbuffer().unwrap();
		gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_renderbuffer));
		gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, width as i32, height as i32);
		gl.bind_renderbuffer(glow::RENDERBUFFER, None);
		let framebuffer = gl.create_framebuffer().unwrap();
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
		gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth_renderbuffer));
		assert_eq!(gl.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE, "시험 실패: 프레임버퍼를 짓지 못함");
		CaptureTarget { framebuffer, color_texture, depth_renderbuffer }
	}
}

//==============================================================================
/// 판의 화소 읽기. (왼쪽 아래 원점, 원본 readPixels 와 같은 줄 차례)
//==============================================================================
fn read_target_pixels(gl: &GlContext, framebuffer: glow::Framebuffer, width: usize, height: usize) -> Vec<u8> {
	let mut pixels = vec![0u8; width * height * 4];
	unsafe {
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
	}
	pixels
}

//==============================================================================
/// 방 구석 장면의 모형. (원본 시험과 같은 바닥, 두 벽, 상자, 다리 둘 달린 로봇)
//==============================================================================
struct RoomScene {
	floor_mesh: ToonMesh,
	wall_mesh: ToonMesh,
	crate_mesh: ToonMesh,
	robot_mesh: ToonMesh,
	view_matrix: Matrix4,
	perspective_matrix: Matrix4,
	orthographic_matrix: Matrix4,
}

//==============================================================================
/// 방 구석 장면 짓기.
//==============================================================================
fn create_room_scene(gl: &GlContext, renderer: &mut ToonRenderer) -> RoomScene {
	let create_mesh = |parts: &[(ToonGeometry, Matrix4, &str)]| -> ToonMesh {
		let mut builder = MeshBuilder::new();
		for (geometry, matrix, color) in parts {
			builder.add_part(geometry, matrix, color, ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
		}
		renderer.create_mesh(gl, &builder.read_arrays()).expect("모형")
	};
	let pi = std::f64::consts::PI;
	let floor_mesh = create_mesh(&[(create_box_geometry(8.0, 0.2, 8.0), Matrix4::create_translation(0.0, -0.1, 0.0), "#b8b0a4")]);
	let wall_mesh = create_mesh(&[(create_box_geometry(8.0, 3.0, 0.2), Matrix4::create_translation(0.0, 1.5, -2.1), "#a8b4c0"), (create_box_geometry(0.2, 3.0, 8.0), Matrix4::create_translation(-2.1, 1.5, 0.0), "#a8b4c0")]);
	let crate_mesh = create_mesh(&[(create_box_geometry(0.8, 0.8, 0.8), Matrix4::create_translation(0.6, 0.4, -0.6), "#c89060")]);
	let robot_mesh = create_mesh(&[
		(create_sphere_geometry(0.38, 32, 20, pi), Matrix4::create_translation(-0.6, 1.05, 0.6), "#e0e4ea"),
		(create_cylinder_geometry(0.07, 0.07, 0.62, 16), Matrix4::create_translation(-0.75, 0.37, 0.6), "#707880"),
		(create_cylinder_geometry(0.07, 0.07, 0.62, 16), Matrix4::create_translation(-0.45, 0.37, 0.6), "#707880"),
		(create_box_geometry(0.16, 0.06, 0.24), Matrix4::create_translation(-0.75, 0.03, 0.64), "#505860"),
		(create_box_geometry(0.16, 0.06, 0.24), Matrix4::create_translation(-0.45, 0.03, 0.64), "#505860"),
	]);
	let view_matrix = Matrix4::create_look_at(Vector3::create(3.2, 3.0, 4.2), Vector3::create(-0.2, 0.3, -0.4), Vector3::create(0.0, 1.0, 0.0));
	let aspect_ratio = WIDTH as f64 / HEIGHT as f64;
	let perspective_matrix = Matrix4::create_perspective(0.75, aspect_ratio, 0.1, 40.0);
	let orthographic_height = 2.6;
	let orthographic_matrix = Matrix4::create_orthographic(-orthographic_height * aspect_ratio, orthographic_height * aspect_ratio, -orthographic_height, orthographic_height, 0.1, 40.0);
	RoomScene { floor_mesh, wall_mesh, crate_mesh, robot_mesh, view_matrix, perspective_matrix, orthographic_matrix }
}

//==============================================================================
/// 렌더러 빛 맞추기. (원본 시험과 같은 해, 하늘, 땅, 칸 셋)
//==============================================================================
fn apply_room_light(renderer: &mut ToonRenderer) {
	renderer.set_light([-0.45, -1.0, -0.35], [1.6, 1.5, 1.35], [0.55, 0.6, 0.75], [0.3, 0.27, 0.24]);
	renderer.set_band_count(3);
}

//==============================================================================
/// 한 경우. (이름, 굵기, 배율, 가는 선 길이 맞는 길, 번호 3 의 선 색을 붉게 할지)
//==============================================================================
struct CaseItem {
	name: &'static str,
	width: f64,
	scale: f64,
	path: ToonOutlinePath,
	is_colored: bool,
}

//==============================================================================
/// 한 프레임 그리기. (바깥선을 켠 큐로 방을 그리고 선의 판 위에 섞는다)
//==============================================================================
fn draw_frame(gl: &GlContext, renderer: &mut ToonRenderer, post_process: &mut ToonPostProcess, outline: &mut ToonScreenOutline, scene: &RoomScene, target: &CaptureTarget) -> Vec<u8> {
	post_process.begin(gl, [0.1, 0.1, 0.12]);
	{
		let mut queue = ToonDrawQueue::new();
		queue.set_screen_outline(Some(outline));
		queue.set_post_process(Some(post_process));
		queue.add_mesh(&scene.floor_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(1), has_screen_outline: false, ..ToonDrawOptions::default() });
		queue.add_mesh(&scene.wall_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(2), has_screen_outline: false, ..ToonDrawOptions::default() });
		queue.add_mesh(&scene.crate_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(3), ..ToonDrawOptions::default() });
		queue.add_mesh(&scene.robot_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(4), ..ToonDrawOptions::default() });
		queue.draw_with_matrices(gl, renderer, &scene.view_matrix, &scene.perspective_matrix);
	}
	post_process.end(gl, Some(target.framebuffer), [0, 0, WIDTH as i32, HEIGHT as i32]);
	read_target_pixels(gl, target.framebuffer, WIDTH, HEIGHT)
}

//==============================================================================
/// 다른 바이트 수.
//==============================================================================
fn count_different(first: &[u8], second: &[u8]) -> usize {
	first.iter().zip(second.iter()).filter(|(left, right)| left != right).count()
}

fn main() {
	let mut check_count = 0u32;
	let picture_path = std::env::var("TOON_OUTLINE_PICTURE_PATH").ok();
	let engine = Engine::new(EngineConfiguration::new());
	let graphic = engine.get_graphic();
	check(&mut check_count, graphic.is_some(), "GL 컨텍스트를 짓지 못함 (화면 서버)");
	let graphic = graphic.unwrap();
	let gl = graphic.get_gl();
	let target = create_capture_target(gl, WIDTH, HEIGHT);
	let mut post_process = ToonPostProcess::new_with_options(gl, WIDTH as i32, HEIGHT as i32, ToonPostProcessOptions { depth_bits: 24, ..ToonPostProcessOptions::default() }).expect("후처리");
	post_process.set_bloom(false, 0.0, 1.0, 2.0);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	renderer.set_hdr_output(post_process.get_use_float_color());
	apply_room_light(&mut renderer);
	let mut outline = ToonScreenOutline::new(gl).expect("화면 바깥선");
	outline.set_hdr_output(post_process.get_use_float_color());
	let scene = create_room_scene(gl, &mut renderer);
	// 선 없는 기준 화면. (굵기 둘 다 0 이면 선의 판만 남기고 긋지 않는다)
	outline.set_width(0.0, ToonOutlineWidthUnit::Pixel);
	outline.set_crease(60.0, 0.0, 1.0);
	let base_pixels = draw_frame(gl, &mut renderer, &mut post_process, &mut outline, &scene, &target);
	{
		let line_target = outline.get_line_target().expect("선의 판");
		check(&mut check_count, line_target.outline_path == ToonOutlinePath::None, "굵기 0: 긋지 않는 길");
		check(&mut check_count, line_target.seed_texture.is_none() && line_target.jump_flood_textures.is_empty(), "굵기 0: 씨앗과 넓히기 판을 짓지 않음");
	}
	let case_list = [
		CaseItem { name: "w2s1", width: 2.0, scale: 1.0, path: ToonOutlinePath::Thin, is_colored: false },
		CaseItem { name: "w262s1", width: 2.62, scale: 1.0, path: ToonOutlinePath::Thin, is_colored: false },
		CaseItem { name: "w3s1", width: 3.0, scale: 1.0, path: ToonOutlinePath::Thin, is_colored: false },
		CaseItem { name: "w6s1", width: 6.0, scale: 1.0, path: ToonOutlinePath::JumpFlood, is_colored: false },
		CaseItem { name: "w2s075", width: 2.0, scale: 0.75, path: ToonOutlinePath::ThinScaled, is_colored: false },
		CaseItem { name: "w262s075", width: 2.62, scale: 0.75, path: ToonOutlinePath::ThinScaled, is_colored: false },
		CaseItem { name: "w3s075", width: 3.0, scale: 0.75, path: ToonOutlinePath::ThinScaled, is_colored: false },
		CaseItem { name: "w6s075", width: 6.0, scale: 0.75, path: ToonOutlinePath::JumpFlood, is_colored: false },
		CaseItem { name: "w2s05", width: 2.0, scale: 0.5, path: ToonOutlinePath::ThinScaled, is_colored: false },
		CaseItem { name: "w262s05", width: 2.62, scale: 0.5, path: ToonOutlinePath::ThinScaled, is_colored: false },
		CaseItem { name: "w3s05", width: 3.0, scale: 0.5, path: ToonOutlinePath::ThinScaled, is_colored: false },
		CaseItem { name: "w6s05", width: 6.0, scale: 0.5, path: ToonOutlinePath::ThinScaled, is_colored: false },
		CaseItem { name: "w2s05Color", width: 2.0, scale: 0.5, path: ToonOutlinePath::ThinScaled, is_colored: true },
		CaseItem { name: "w2s1Color", width: 2.0, scale: 1.0, path: ToonOutlinePath::Thin, is_colored: true },
		CaseItem { name: "w2s1Again", width: 2.0, scale: 1.0, path: ToonOutlinePath::Thin, is_colored: false },
	];
	let mut result_table: std::collections::HashMap<&'static str, Vec<u8>> = std::collections::HashMap::new();
	for (case_index, case_item) in case_list.iter().enumerate() {
		outline.set_width(case_item.width, ToonOutlineWidthUnit::Pixel);
		outline.set_crease(60.0, 1.0, 1.0);
		outline.set_resolution_scale(case_item.scale);
		outline.clear_line_colors();
		if case_item.is_colored {
			outline.set_line_color(3, [1.0, 0.1, 0.1, 1.0]);
		}
		let pixels = draw_frame(gl, &mut renderer, &mut post_process, &mut outline, &scene, &target);
		{
			let line_target = outline.get_line_target().expect("선의 판");
			check(&mut check_count, line_target.outline_path == case_item.path, &format!("{}: 길 {:?} (맞는 길 {:?})", case_item.name, line_target.outline_path, case_item.path));
			check(&mut check_count, line_target.width == ((WIDTH as f64 * case_item.scale).ceil() as i32) && line_target.height == ((HEIGHT as f64 * case_item.scale).ceil() as i32), &format!("{}: 선의 판은 그리는 영역 x 배율 ({} x {})", case_item.name, line_target.width, line_target.height));
			check(&mut check_count, line_target.jump_flood_width == line_target.width && line_target.jump_flood_height == line_target.height, &format!("{}: 넓히기 판도 같은 크기", case_item.name));
			if case_item.path == ToonOutlinePath::JumpFlood {
				check(&mut check_count, line_target.seed_texture.is_some() && line_target.jump_flood_textures.len() == 2 && line_target.jump_flood_index.is_some(), &format!("{}: 굵은 선은 씨앗과 넓히기 판을 지어 넓힘", case_item.name));
			}
			else {
				check(&mut check_count, line_target.jump_flood_index.is_none(), &format!("{}: 가는 선은 넓히지 않음", case_item.name));
			}
			if case_item.path == ToonOutlinePath::ThinScaled {
				check(&mut check_count, line_target.coverage_texture.is_some() && line_target.coverage_framebuffer.is_some(), &format!("{}: 줄인 가는 선은 덮는 몫 판을 지음", case_item.name));
			}
			if case_index == 0 {
				check(&mut check_count, line_target.seed_texture.is_none() && line_target.jump_flood_textures.is_empty() && line_target.coverage_texture.is_none(), "가는 선만 그린 처음에는 씨앗, 넓히기, 덮는 몫 판이 없음");
			}
		}
		let different = count_different(&pixels, &base_pixels);
		println!("  {}: 선 없는 화면과 다른 바이트 {}", case_item.name, different);
		check(&mut check_count, different > 2000, &format!("{}: 선이 그려짐", case_item.name));
		if case_item.is_colored {
			let mut red_count = 0;
			for index in 0..WIDTH * HEIGHT {
				if pixels[index * 4] as i32 > pixels[index * 4 + 1] as i32 + 60 && pixels[index * 4] > 100 && base_pixels[index * 4] as i32 <= base_pixels[index * 4 + 1] as i32 + 60 {
					red_count += 1;
				}
			}
			println!("  {}: 붉은 선 화소 {}", case_item.name, red_count);
			check(&mut check_count, red_count > 50, &format!("{}: 번호 3 의 선이 붉음 (번호 색 표가 덮는 몫 길에서도 읽힘)", case_item.name));
		}
		if let Some(picture_path) = picture_path.as_ref() {
			std::fs::create_dir_all(picture_path).unwrap();
			std::fs::write(format!("{}/outline-{}.rgba", picture_path, case_item.name), &pixels).unwrap();
		}
		result_table.insert(case_item.name, pixels);
	}
	let error = unsafe { gl.get_error() };
	check(&mut check_count, error == 0, &format!("GL 오류 없음 ({})", error));
	// 같은 값으로 다시 그리면 같다. (판을 비운 뒤 다시 그리므로 앞 프레임의 내용이 남지 않는다, 점프 플러딩과 줄인 판을 거친 뒤에도)
	check(&mut check_count, count_different(&result_table["w2s1"], &result_table["w2s1Again"]) == 0, "굵은 선과 줄인 판을 거친 뒤 같은 값으로 다시 그린 화면이 처음과 바이트까지 같음");
	// 굵을수록 선 화소가 늘어난다.
	let thin = count_different(&result_table["w2s1"], &base_pixels);
	let middle = count_different(&result_table["w3s1"], &base_pixels);
	let thick = count_different(&result_table["w6s1"], &base_pixels);
	check(&mut check_count, thin < middle && middle < thick, &format!("굵기 2, 3, 6 의 선 화소가 늘어남 ({} < {} < {})", thin, middle, thick));
	// 배율을 줄여도 선은 사라지지 않고 비슷한 자리에 선다. (줄이면 가장자리가 부드러워져 다른 화소가 는다)
	for (full, scaled) in [("w2s1", "w2s075"), ("w2s1", "w2s05"), ("w3s1", "w3s05")] {
		let full_count = count_different(&result_table[full], &base_pixels) as f64;
		let scaled_count = count_different(&result_table[scaled], &base_pixels) as f64;
		check(&mut check_count, scaled_count > full_count * 0.6 && scaled_count < full_count * 3.0, &format!("{} 의 선 화소가 {} 의 0.6 ~ 3 배 ({} 대 {})", scaled, full, scaled_count, full_count));
	}
	println!("시험 통과 ({} 검사)", check_count);
}
