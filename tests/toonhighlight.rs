//==============================================================================
//! 툰 강조의 찍기 시험. (원본 tests/graphics3d/toonhighlightchrome.test.mjs 와 같은 장면, 같은 검사)
//! - Engine::new 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 480 x 200 프레임버퍼(RGBA8, 깊이 24, MSAA 없음)에 같은 바위 셋을 인스턴스로 그린다. (번호 10, 11, 12, 화면을 세 칸으로 나눠 하나씩)
//! - 번호 색 표: ToonScreenOutline::set_line_color 로 11 만 넣으면 가운데 칸의 선만 바뀌고 양 옆 칸은 화소가 같다. 표를 비우면 넣기 전과 화소가 같다.
//! - 인스턴스 빛남: 가운데 인스턴스에만 emissive_color 를 주면 가운데 칸만 밝아지고 양 옆 칸은 화소가 같다. 가운데 칸 안의 명암 차이는 남는다.
//!   SCREEN_DUMP_PATH 를 주면 찍은 그림을 .rgba 로 남긴다. (원본 시험의 같은 이름 파일과 화면 견주기)
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test toonhighlight
//==============================================================================
use vanilla::{create_octahedron_geometry, Engine, EngineConfiguration, Matrix4, MeshBuilder, ToonDrawOptions, ToonInstance, ToonLineDrawOptions, ToonOutlineWidthUnit, ToonPartOptions, ToonRenderer, ToonScreenOutline, Vector3};

/// 찍는 너비. (원본 캔버스와 같다)
const WIDTH: usize = 480;
/// 찍는 높이.
const HEIGHT: usize = 200;

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
/// 칸마다 다른 화소 수. (칸 0, 1, 2 는 x 0 ~ 159, 160 ~ 319, 320 ~ 479)
//==============================================================================
fn compare_columns(first: &[u8], second: &[u8]) -> [u32; 3] {
	let mut different_counts = [0u32; 3];
	for pixel_y in 0..HEIGHT {
		for pixel_x in 0..WIDTH {
			let index = (pixel_y * WIDTH + pixel_x) * 4;
			if first[index..index + 4] != second[index..index + 4] {
				different_counts[pixel_x / 160] += 1;
			}
		}
	}
	different_counts
}

//==============================================================================
/// 바위 셋. (번호 10, 11, 12)
//==============================================================================
fn create_instances() -> Vec<ToonInstance> {
	[-1.6, 0.0, 1.6].iter().enumerate().map(|(index, x)| ToonInstance { model_matrix: Matrix4::create_translation(*x, 0.0, 0.0), line_id: Some(10 + index as u32), ..ToonInstance::default() }).collect()
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
	let mut outline = ToonScreenOutline::new(gl).expect("화면 바깥선");
	outline.set_width(3.0, ToonOutlineWidthUnit::Pixel);
	outline.set_crease(40.0, 1.5, 1.0);
	let mut builder = MeshBuilder::new();
	builder.add_part(&create_octahedron_geometry(0.55), &Matrix4::create_identity(), "#a89880", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
	let rock_mesh = renderer.create_mesh(gl, &builder.read_arrays()).expect("모형");
	let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 1.6, 4.2), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	let projection_matrix = Matrix4::create_perspective(0.6, WIDTH as f64 / HEIGHT as f64, 0.1, 50.0);
	let read_frame = |renderer: &mut ToonRenderer, outline: &mut ToonScreenOutline, instance_list: &[ToonInstance], has_outline: bool| -> Vec<u8> {
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.viewport(0, 0, WIDTH as i32, HEIGHT as i32);
			gl.clear_color(0.2, 0.25, 0.3, 1.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
		if has_outline && outline.begin_line_ids(gl, &view_matrix, &projection_matrix) {
			outline.draw_mesh_instanced(gl, &rock_mesh, instance_list, 10, ToonLineDrawOptions::default());
			outline.end_line_ids(gl);
		}
		renderer.begin_frame_with_matrices(gl, &view_matrix, &projection_matrix);
		renderer.draw_mesh_instanced(gl, &rock_mesh, instance_list, ToonDrawOptions::default());
		renderer.end_frame(gl);
		if has_outline {
			outline.draw_outline(gl);
		}
		let mut pixels = vec![0u8; WIDTH * HEIGHT * 4];
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.read_pixels(0, 0, WIDTH as i32, HEIGHT as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
		}
		pixels
	};
	let base = read_frame(&mut renderer, &mut outline, &create_instances(), true);
	outline.set_line_color(11, [1.0, 0.85, 0.1, 1.0]);
	let line_color = read_frame(&mut renderer, &mut outline, &create_instances(), true);
	let line_color_count = outline.get_line_color_count();
	outline.set_line_color(0, [1.0, 0.0, 0.0, 1.0]);
	outline.set_line_color(99999, [1.0, 0.0, 0.0, 1.0]);
	let line_color_count_after_invalid = outline.get_line_color_count();
	outline.clear_line_colors();
	let cleared = read_frame(&mut renderer, &mut outline, &create_instances(), true);
	outline.set_line_color(12, [1.0, 0.0, 0.0, 1.0]);
	outline.remove_line_color(12);
	let removed = read_frame(&mut renderer, &mut outline, &create_instances(), true);
	let body_base = read_frame(&mut renderer, &mut outline, &create_instances(), false);
	let mut glow_instances = create_instances();
	glow_instances[1].emissive_color = Some([1.0, 0.85, 0.3]);
	glow_instances[1].emissive_intensity = 0.3;
	let body_glow = read_frame(&mut renderer, &mut outline, &glow_instances, false);
	let error = unsafe { gl.get_error() };
	check(&mut check_count, error == 0, &format!("GL 오류 없음 ({})", error));

	let line_color_difference = compare_columns(&base, &line_color);
	println!("  번호 색 표(11 만): 칸마다 다른 화소 {}, {}, {}", line_color_difference[0], line_color_difference[1], line_color_difference[2]);
	check(&mut check_count, line_color_difference[0] == 0 && line_color_difference[2] == 0, "표에 없는 번호 10, 12 의 칸은 화소가 같음");
	check(&mut check_count, line_color_difference[1] > 200, &format!("표에 넣은 번호 11 의 칸은 선이 바뀜 ({})", line_color_difference[1]));
	check(&mut check_count, line_color_count == 1 && line_color_count_after_invalid == 1, "범위 밖 번호는 표에 들지 않음");
	let mut yellow_count = 0;
	for pixel_y in 0..HEIGHT {
		for pixel_x in 160..320 {
			let index = (pixel_y * WIDTH + pixel_x) * 4;
			if line_color[index] != base[index] && line_color[index] > 200 && line_color[index + 2] < 120 {
				yellow_count += 1;
			}
		}
	}
	check(&mut check_count, yellow_count > 100, &format!("가운데 칸의 선이 표의 노란 색 ({})", yellow_count));
	let cleared_difference = compare_columns(&base, &cleared);
	check(&mut check_count, cleared_difference.iter().all(|count| *count == 0), &format!("clear_line_colors 뒤는 넣기 전과 화소가 같음 ({:?})", cleared_difference));
	let removed_difference = compare_columns(&base, &removed);
	check(&mut check_count, removed_difference.iter().all(|count| *count == 0), &format!("remove_line_color 뒤는 넣기 전과 화소가 같음 ({:?})", removed_difference));

	let glow_difference = compare_columns(&body_base, &body_glow);
	println!("  인스턴스 빛남(가운데만): 칸마다 다른 화소 {}, {}, {}", glow_difference[0], glow_difference[1], glow_difference[2]);
	check(&mut check_count, glow_difference[0] == 0 && glow_difference[2] == 0, "빛남이 없는 인스턴스의 칸은 화소가 같음");
	check(&mut check_count, glow_difference[1] > 1000, &format!("빛남을 준 인스턴스의 칸은 바뀜 ({})", glow_difference[1]));
	let mut darker_count = 0;
	let mut minimum_glow = 765u32;
	let mut maximum_glow = 0u32;
	for pixel_y in 0..HEIGHT {
		for pixel_x in 160..320 {
			let index = (pixel_y * WIDTH + pixel_x) * 4;
			let base_sum = body_base[index] as u32 + body_base[index + 1] as u32 + body_base[index + 2] as u32;
			let glow_sum = body_glow[index] as u32 + body_glow[index + 1] as u32 + body_glow[index + 2] as u32;
			if glow_sum == base_sum {
				continue;
			}
			if glow_sum < base_sum {
				darker_count += 1;
			}
			minimum_glow = minimum_glow.min(glow_sum);
			maximum_glow = maximum_glow.max(glow_sum);
		}
	}
	println!("  빛남 인스턴스 밝기 합 범위 {} ~ {}", minimum_glow, maximum_glow);
	check(&mut check_count, darker_count == 0, &format!("빛남은 더하기라 어두워진 화소가 없음 ({})", darker_count));
	check(&mut check_count, maximum_glow.saturating_sub(minimum_glow) > 60, &format!("빛남 위에 명암이 남음 ({})", maximum_glow.saturating_sub(minimum_glow)));
	if let Ok(dump_path) = std::env::var("SCREEN_DUMP_PATH") {
		for (name, pixels) in [("base", &base), ("lineColor", &line_color), ("cleared", &cleared), ("bodyBase", &body_base), ("bodyGlow", &body_glow)] {
			std::fs::write(format!("{}_{}.rgba", dump_path, name), pixels).unwrap();
		}
	}
	renderer.delete_mesh(gl, rock_mesh);
	outline.destroy(gl);
	unsafe {
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		gl.delete_framebuffer(framebuffer);
		gl.delete_renderbuffer(depth_renderbuffer);
		gl.delete_texture(color_texture);
	}
	println!("툰 강조 찍기 시험 통과 ({} 검사)", check_count);
}
