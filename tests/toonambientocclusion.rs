//==============================================================================
//! 툰 모서리 어둠(화면 공간 AO)의 찍기 시험. (원본 tests/graphics3d/toonambientocclusionchrome.test.mjs 와 같은 장면, 같은 검사)
//! - Engine::new 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 방 구석(바닥, 뒷벽, 왼벽), 상자, 다리 둘 달린 로봇을 ToonDrawQueue 와 ToonPostProcess 로
//!   640 x 360 프레임버퍼(RGBA8, 깊이 24, MSAA 없음)에 그린다. (원근과 직교)
//! - 꺼짐: 켠 적 없음, 켰다 끔 모두 화소가 같다. TOON_AO_BASE_PATH(고치기 전 사본이 남긴 .rgba 폴더)를 주면 고치기 전의 같은 장면과 바이트까지 같다.
//! - 켬: 벽과 바닥이 만나는 곳, 상자 옆 바닥, 로봇 발밑이 어두워지고 트인 바닥은 그대로다. 해 칸 경계(로봇 몸의 밝기 단계)는 그 자리 그대로다.
//! - 직접 빛 몫(set_ambient_occlusion 다섯째 인자)을 0.25, 0.5, 1 로 올릴수록 해가 비추는 구석이 더 어두워진다. 트인 바닥과 로봇 머리는 몫을 올려도 끔과 바이트까지 같다.
//!   바깥선(ToonScreenOutline)을 켠 장면은 그 선의 판을 다시 쓰고, 끈 장면은 후처리의 미리 그리기(선 없이 선의 판만)를 쓴다. 둘의 모서리 어둠 판은 같다.
//! - TOON_AO_PICTURE_PATH 를 주면 화면과 AO 판을 그 폴더에 .rgba(왼쪽 아래 원점, 640 x 360)로 남긴다. (두 엔진 화면 견주기)
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test toonambientocclusion
//==============================================================================
use vanilla::{create_box_geometry, create_cylinder_geometry, create_sphere_geometry, Engine, EngineConfiguration, GlContext, Matrix4, MeshBuilder, ToonDrawOptions, ToonDrawQueue, ToonGeometry, ToonMesh, ToonOutlineWidthUnit, ToonPartOptions, ToonPostProcess, ToonPostProcessOptions, ToonRenderer, ToonScreenOutline, Vector3};

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
/// 월드 자리를 화소 자리로. (원본 project 와 같다)
//==============================================================================
fn project_point(scene: &RoomScene, projection_matrix: &Matrix4, point: [f64; 3]) -> [i32; 2] {
	let mut matrix = *projection_matrix;
	matrix.multiply(&scene.view_matrix);
	let elements = matrix.read_elements();
	let clip_x = elements[0] * point[0] + elements[4] * point[1] + elements[8] * point[2] + elements[12];
	let clip_y = elements[1] * point[0] + elements[5] * point[1] + elements[9] * point[2] + elements[13];
	let clip_w = elements[3] * point[0] + elements[7] * point[1] + elements[11] * point[2] + elements[15];
	[((clip_x / clip_w * 0.5 + 0.5) * WIDTH as f64).floor() as i32, ((clip_y / clip_w * 0.5 + 0.5) * HEIGHT as f64).floor() as i32]
}

//==============================================================================
/// 한 경우. (이름, 직교인지, 바깥선, 모서리 어둠 켬, 직접 빛 몫)
//==============================================================================
struct CaseItem {
	name: &'static str,
	is_orthographic: bool,
	has_outline: bool,
	is_enabled: bool,
	direct_lighting_strength: f64,
}

//==============================================================================
/// 한 경우의 결과. (화면, 켰으면 AO 판의 r 을 회색으로, 재는 자리 여섯)
//==============================================================================
struct CaseResult {
	pixels: Vec<u8>,
	occlusion: Option<Vec<u8>>,
	probes: Vec<(&'static str, [i32; 2])>,
}

//==============================================================================
/// 다른 바이트 수.
//==============================================================================
fn count_different(first: &[u8], second: &[u8]) -> usize {
	first.iter().zip(second.iter()).filter(|(left, right)| left != right).count()
}

//==============================================================================
/// 3 x 3 평균 밝기. (원본 readLuminance)
//==============================================================================
fn read_luminance(pixels: &[u8], point: [i32; 2]) -> f64 {
	let mut sum = 0.0;
	for offset_y in -1..=1 {
		for offset_x in -1..=1 {
			let index = (((point[1] + offset_y) as usize) * WIDTH + (point[0] + offset_x) as usize) * 4;
			sum += pixels[index] as f64 * 0.2126 + pixels[index + 1] as f64 * 0.7152 + pixels[index + 2] as f64 * 0.0722;
		}
	}
	sum / 9.0
}

//==============================================================================
/// 가운데 둘레 (2 x 반 + 1) 칸의 다른 바이트 수.
//==============================================================================
fn count_different_around(first: &[u8], second: &[u8], center: [i32; 2], half: i32) -> usize {
	let mut count = 0;
	for offset_y in -half..=half {
		for offset_x in -half..=half {
			let index = (((center[1] + offset_y) as usize) * WIDTH + (center[0] + offset_x) as usize) * 4;
			for channel in 0..4 {
				count += if first[index + channel] != second[index + channel] { 1 } else { 0 };
			}
		}
	}
	count
}

fn main() {
	let mut check_count = 0u32;
	let base_path = std::env::var("TOON_AO_BASE_PATH").ok();
	let picture_path = std::env::var("TOON_AO_PICTURE_PATH").ok();
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
	outline.set_width(2.0, ToonOutlineWidthUnit::Pixel);
	outline.set_hdr_output(post_process.get_use_float_color());
	let scene = create_room_scene(gl, &mut renderer);
	let case_list = [
		CaseItem { name: "offPerspective", is_orthographic: false, has_outline: false, is_enabled: false, direct_lighting_strength: 0.0 },
		CaseItem { name: "offOrthographic", is_orthographic: true, has_outline: false, is_enabled: false, direct_lighting_strength: 0.0 },
		CaseItem { name: "offOutline", is_orthographic: true, has_outline: true, is_enabled: false, direct_lighting_strength: 0.0 },
		CaseItem { name: "onPerspective", is_orthographic: false, has_outline: false, is_enabled: true, direct_lighting_strength: 0.0 },
		CaseItem { name: "onOrthographic", is_orthographic: true, has_outline: false, is_enabled: true, direct_lighting_strength: 0.0 },
		CaseItem { name: "onOutline", is_orthographic: true, has_outline: true, is_enabled: true, direct_lighting_strength: 0.0 },
		CaseItem { name: "offAgainPerspective", is_orthographic: false, has_outline: false, is_enabled: false, direct_lighting_strength: 0.0 },
		CaseItem { name: "offAgainOutline", is_orthographic: true, has_outline: true, is_enabled: false, direct_lighting_strength: 0.0 },
		CaseItem { name: "directQuarterPerspective", is_orthographic: false, has_outline: false, is_enabled: true, direct_lighting_strength: 0.25 },
		CaseItem { name: "directHalfPerspective", is_orthographic: false, has_outline: false, is_enabled: true, direct_lighting_strength: 0.5 },
		CaseItem { name: "directFullPerspective", is_orthographic: false, has_outline: false, is_enabled: true, direct_lighting_strength: 1.0 },
		CaseItem { name: "directFullOutline", is_orthographic: true, has_outline: true, is_enabled: true, direct_lighting_strength: 1.0 },
		CaseItem { name: "offAfterDirectPerspective", is_orthographic: false, has_outline: false, is_enabled: false, direct_lighting_strength: 0.0 },
	];
	let mut result_table: std::collections::HashMap<&'static str, CaseResult> = std::collections::HashMap::new();
	for case_item in case_list.iter() {
		let projection_matrix = if case_item.is_orthographic { scene.orthographic_matrix } else { scene.perspective_matrix };
		post_process.set_ambient_occlusion(case_item.is_enabled, 0.5, 0.5, 0.5, case_item.direct_lighting_strength);
		post_process.begin(gl, [0.1, 0.1, 0.12]);
		{
			let mut queue = ToonDrawQueue::new();
			if case_item.has_outline {
				queue.set_screen_outline(Some(&mut outline));
			}
			queue.set_post_process(Some(&mut post_process));
			queue.add_mesh(&scene.floor_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(1), has_screen_outline: false, ..ToonDrawOptions::default() });
			queue.add_mesh(&scene.wall_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(2), has_screen_outline: false, ..ToonDrawOptions::default() });
			queue.add_mesh(&scene.crate_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(3), ..ToonDrawOptions::default() });
			queue.add_mesh(&scene.robot_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(4), ..ToonDrawOptions::default() });
			queue.draw_with_matrices(gl, &mut renderer, &scene.view_matrix, &projection_matrix);
		}
		check(&mut check_count, renderer.get_ambient_occlusion_texture().is_none(), &format!("{}: 그린 뒤 렌더러의 모서리 어둠을 푼다", case_item.name));
		post_process.end(gl, Some(target.framebuffer), [0, 0, WIDTH as i32, HEIGHT as i32]);
		let pixels = read_target_pixels(gl, target.framebuffer, WIDTH, HEIGHT);
		let occlusion = if case_item.is_enabled {
			let texture = post_process.get_ambient_occlusion_texture();
			check(&mut check_count, texture.is_some(), &format!("{}: 켬이면 AO 판이 있음", case_item.name));
			unsafe {
				let framebuffer = gl.create_framebuffer().unwrap();
				gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
				gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, texture, 0);
				let raw = read_target_pixels(gl, framebuffer, WIDTH, HEIGHT);
				gl.bind_framebuffer(glow::FRAMEBUFFER, None);
				gl.delete_framebuffer(framebuffer);
				let mut gray = vec![255u8; WIDTH * HEIGHT * 4];
				for index in 0..WIDTH * HEIGHT {
					gray[index * 4] = raw[index * 4];
					gray[index * 4 + 1] = raw[index * 4];
					gray[index * 4 + 2] = raw[index * 4];
				}
				Some(gray)
			}
		}
		else {
			check(&mut check_count, post_process.get_ambient_occlusion_texture().is_none(), &format!("{}: 끔이면 AO 판을 내주지 않음", case_item.name));
			None
		};
		let probes = vec![
			("wallCorner", project_point(&scene, &projection_matrix, [1.2, 0.02, -1.96])),
			("leftCorner", project_point(&scene, &projection_matrix, [-1.96, 0.02, -0.2])),
			("crateSide", project_point(&scene, &projection_matrix, [1.03, 0.01, -0.3])),
			("robotFoot", project_point(&scene, &projection_matrix, [-0.6, 0.01, 0.64])),
			("openFloor", project_point(&scene, &projection_matrix, [1.7, 0.0, 1.4])),
			("robotHead", project_point(&scene, &projection_matrix, [-0.6, 1.05, 0.6])),
		];
		if let Some(picture_path) = picture_path.as_ref() {
			std::fs::create_dir_all(picture_path).unwrap();
			std::fs::write(format!("{}/toonao-{}.rgba", picture_path, case_item.name), &pixels).unwrap();
			if let Some(occlusion) = occlusion.as_ref() {
				std::fs::write(format!("{}/toonao-{}Occlusion.rgba", picture_path, case_item.name), occlusion).unwrap();
			}
		}
		result_table.insert(case_item.name, CaseResult { pixels, occlusion, probes });
	}
	let error = unsafe { gl.get_error() };
	check(&mut check_count, error == 0, &format!("GL 오류 없음 ({})", error));
	// 꺼짐은 바이트까지 같다.
	check(&mut check_count, count_different(&result_table["offPerspective"].pixels, &result_table["offAgainPerspective"].pixels) == 0, "원근: 켰다 끈 화면이 켠 적 없는 화면과 같음");
	check(&mut check_count, count_different(&result_table["offOutline"].pixels, &result_table["offAgainOutline"].pixels) == 0, "바깥선: 켰다 끈 화면이 켠 적 없는 화면과 같음");
	check(&mut check_count, count_different(&result_table["offPerspective"].pixels, &result_table["offAfterDirectPerspective"].pixels) == 0, "원근: 직접 빛 몫 1 로 켰다 끈 화면이 켠 적 없는 화면과 같음");
	match base_path.as_ref() {
		Some(base_path) => {
			for name in ["offPerspective", "offOrthographic", "offOutline"] {
				let base = std::fs::read(format!("{}/{}.rgba", base_path, name)).expect("고치기 전 화면");
				let different = count_different(&result_table[name].pixels, &base);
				println!("  꺼짐 {}: 고치기 전과 다른 바이트 {}", name, different);
				check(&mut check_count, base.len() == WIDTH * HEIGHT * 4 && different == 0, &format!("{}: 꺼짐이 고치기 전과 바이트까지 같음", name));
			}
		}
		None => {
			println!("  (TOON_AO_BASE_PATH 를 주지 않아 고치기 전과는 견주지 않음)");
		}
	}
	// 켬: 화소 밝기 견주기.
	let mut darkening_table: std::collections::HashMap<&'static str, std::collections::HashMap<&'static str, f64>> = std::collections::HashMap::new();
	for (on_name, off_name) in [("onPerspective", "offPerspective"), ("onOrthographic", "offOrthographic"), ("onOutline", "offOutline"), ("directQuarterPerspective", "offPerspective"), ("directHalfPerspective", "offPerspective"), ("directFullPerspective", "offPerspective"), ("directFullOutline", "offOutline")] {
		let on_result = &result_table[on_name];
		let off_pixels = &result_table[off_name].pixels;
		let occlusion_pixels = on_result.occlusion.as_ref().unwrap();
		let mut line_list = Vec::new();
		let mut darkening = std::collections::HashMap::new();
		for (probe_name, point) in on_result.probes.iter() {
			let value = read_luminance(off_pixels, *point) - read_luminance(&on_result.pixels, *point);
			darkening.insert(*probe_name, value);
			line_list.push(format!("{} {:.1} (AO {:.0})", probe_name, value, read_luminance(occlusion_pixels, *point)));
		}
		println!("  {} 어두워진 밝기: {}", on_name, line_list.join(", "));
		let probe = |name: &str| on_result.probes.iter().find(|(probe_name, _)| *probe_name == name).unwrap().1;
		// 바깥선 장면은 선이 3 x 3 평균에 섞여(선은 켜고 끔이 같다) 덜 드러나므로 1 이상만 본다.
		let minimum_darkening = if on_name == "onOutline" { 1.0 } else { 2.0 };
		for probe_name in ["wallCorner", "leftCorner", "crateSide", "robotFoot"] {
			check(&mut check_count, darkening[probe_name] >= minimum_darkening, &format!("{}: {} (벽과 바닥이 만나는 곳, 상자 옆, 로봇 발밑) 이 어두워짐", on_name, probe_name));
			check(&mut check_count, read_luminance(occlusion_pixels, probe(probe_name)) <= 235.0, &format!("{}: {} 의 AO 판이 어두움", on_name, probe_name));
		}
		check(&mut check_count, darkening["openFloor"].abs() <= 1.5, &format!("{}: 트인 바닥은 거의 그대로", on_name));
		check(&mut check_count, read_luminance(occlusion_pixels, probe("openFloor")) >= 250.0, &format!("{}: 트인 바닥의 AO 판은 거의 1", on_name));
		// 해 칸 경계: 로봇 머리를 가로지르는 세로 줄에서 밝기가 가장 크게 뛰는 자리가 같다.
		let read_edge = |pixels: &[u8]| -> i32 {
			let center = probe("robotHead");
			let mut best_row = -1;
			let mut best_jump = 0.0;
			for row in center[1] - 25..center[1] + 25 {
				let jump = (read_luminance(pixels, [center[0], row + 1]) - read_luminance(pixels, [center[0], row])).abs();
				if jump > best_jump {
					best_jump = jump;
					best_row = row;
				}
			}
			best_row
		};
		let off_edge = read_edge(off_pixels);
		let on_edge = read_edge(&on_result.pixels);
		let head_different = count_different_around(&on_result.pixels, off_pixels, probe("robotHead"), 12);
		println!("  {} 해 칸 경계 줄: 끔 {}, 켬 {}, 머리 가운데 다른 바이트 {}", on_name, off_edge, on_edge, head_different);
		check(&mut check_count, head_different == 0, &format!("{}: 로봇 머리 가운데(해 칸 경계)는 그대로", on_name));
		if on_name != "onOutline" && on_name != "directFullOutline" {
			check(&mut check_count, off_edge == on_edge, &format!("{}: 해 칸 경계 자리가 그대로", on_name));
		}
		// 트인 바닥 가운데 9 x 9 는 직접 빛 몫과 상관없이 끔과 바이트까지 같다. (보임이 정확히 1)
		let open_different = count_different_around(&on_result.pixels, off_pixels, probe("openFloor"), 4);
		check(&mut check_count, open_different == 0, &format!("{}: 트인 바닥 가운데는 끔과 바이트까지 같음 ({})", on_name, open_different));
		darkening_table.insert(on_name, darkening);
	}
	// 직접 빛 몫을 올릴수록 해가 비추는 구석이 더 어두워진다. (0 → 0.25 → 0.5 → 1)
	for probe_name in ["wallCorner", "leftCorner", "crateSide", "robotFoot"] {
		let steps: Vec<f64> = ["onPerspective", "directQuarterPerspective", "directHalfPerspective", "directFullPerspective"].iter().map(|name| darkening_table[name][probe_name]).collect();
		println!("  직접 빛 몫 0 / 0.25 / 0.5 / 1 의 {} 어두워진 밝기: {}", probe_name, steps.iter().map(|value| format!("{:.1}", value)).collect::<Vec<String>>().join(" / "));
		for index in 1..steps.len() {
			check(&mut check_count, steps[index] > steps[index - 1] + 0.5, &format!("{}: 직접 빛 몫을 올리면 더 어두워짐 ({:.1} → {:.1})", probe_name, steps[index - 1], steps[index]));
		}
	}
	check(&mut check_count, darkening_table["directFullOutline"]["wallCorner"] > darkening_table["onOutline"]["wallCorner"] + 0.5, "바깥선 직교: 직접 빛 몫 1 이면 벽 구석이 더 어두워짐");
	// 바깥선의 선의 판을 다시 쓴 AO 판과 미리 그리기의 AO 판은 같다. (같은 직교 장면)
	let occlusion_different = count_different(result_table["onOrthographic"].occlusion.as_ref().unwrap(), result_table["onOutline"].occlusion.as_ref().unwrap());
	println!("  바깥선 선의 판과 미리 그리기의 AO 판 다른 바이트: {}", occlusion_different);
	check(&mut check_count, occlusion_different == 0, "바깥선의 선의 판을 다시 써도 AO 판이 같음");
	// 설정 읽기와 자르기.
	post_process.set_ambient_occlusion(true, 2.0, -1.0, 0.1, 3.0);
	let ambient_occlusion = post_process.get_ambient_occlusion();
	check(&mut check_count, ambient_occlusion.is_enabled && ambient_occlusion.intensity == 1.0 && ambient_occlusion.radius == 0.0001 && ambient_occlusion.resolution_scale == 0.25 && ambient_occlusion.direct_lighting_strength == 1.0, "set_ambient_occlusion 의 자르기 (세기 1, 반지름 0.0001, 배율 0.25, 몫 1)");
	check(&mut check_count, post_process.get_ambient_occlusion_direct_lighting_strength() == 1.0 && post_process.is_ambient_occlusion_enabled(), "직접 빛 몫과 켬 읽기");
	post_process.set_ambient_occlusion(false, 0.5, 0.5, 0.5, 0.0);
	renderer.set_ambient_occlusion_texture(None, 0.7);
	check(&mut check_count, renderer.get_ambient_occlusion_direct_lighting_strength() == 0.0, "렌더러: 텍스처를 걸지 않으면 직접 빛 몫 0");
	unsafe {
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		gl.delete_framebuffer(target.framebuffer);
		gl.delete_renderbuffer(target.depth_renderbuffer);
		gl.delete_texture(target.color_texture);
	}
	println!("툰 모서리 어둠 찍기 시험 통과 ({} 검사)", check_count);
}
