//==============================================================================
//! 해 그림자 치우침의 찍기 시험. (원본 tests/graphics3d/shadowbiaschrome.test.mjs 와 같은 장면, 같은 검사)
//! - Engine::new 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 400 x 300 프레임버퍼(RGBA8, 깊이 24, MSAA 없음)에 직교 원 행렬 카메라로 바닥과 벽을 그리고,
//!   ShadowMap(near 1, far 100) 을 ToonDrawQueue::draw_shadows 로 그려 set_shadow 로 받는다. 같은 장면을 그림자 받기 켜고 끄고 그려 견준다.
//! - 1. 여드름: extent 40, 해상도 512 에서 해를 받는 벽의 두 옆면(빛과 약 84 도, 73 도)이 제 그림자를 받지 않는다.
//!   (치우침을 셰이더에 0.0015 깊이와 0.02 노멀로 박아 두었을 때는 두 면이 그림자 깊은 곳만큼 어두워졌다)
//! - 2. 떠 보임: extent 2, 해상도 2048 에서 높이 0.1 의 낮은 벽도 밑동에 붙은 그림자를 드리운다. (박아 둔 치우침은 월드 0.15 라 그림자가 아예 없었다)
//! - 3. 높이 2 의 벽 그림자는 extent 8 에서 밑동부터 해 방향대로 끝(z -2.1)까지 닿는다.
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test shadowbias
//==============================================================================
use vanilla::{create_box_geometry, Engine, EngineConfiguration, GlContext, Matrix4, MeshBuilder, ShadowMap, ToonDrawOptions, ToonDrawQueue, ToonPartOptions, ToonRenderer, Vector3};

/// 찍는 너비. (원본 캔버스와 같다)
const WIDTH: usize = 400;
/// 찍는 높이.
const HEIGHT: usize = 300;

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
/// 한 장면에서 읽은 것. (벽의 두 옆면 7x7 화소, 벽 그늘 쪽 바닥 x=0 줄의 z -1.5 ~ -2.3 화소 밝기)
//==============================================================================
struct Captured {
	side: Vec<u8>,
	front: Vec<u8>,
	line: Vec<i32>,
}

//==============================================================================
/// 재어 낸 것. (면마다 RGB 평균 어두워진 몫, 바닥 줄은 그림자 깊은 곳 절반 넘게 어두운 첫 자리와 끝 자리의 z)
//==============================================================================
struct Measured {
	side: f64,
	front: f64,
	deep: i32,
	band_start: Option<f64>,
	band_end: Option<f64>,
}

//==============================================================================
/// 월드 자리를 화소 자리로. (원본 readPoint 와 같이 클립의 x, y 를 0 ~ 1 로 옮겨 내림)
//==============================================================================
fn read_point(view_projection_matrix: &Matrix4, x: f64, y: f64, z: f64) -> [i32; 2] {
	let clip = view_projection_matrix.transform_point(x, y, z);
	[((clip.x * 0.5 + 0.5) * WIDTH as f64).floor() as i32, ((clip.y * 0.5 + 0.5) * HEIGHT as f64).floor() as i32]
}

//==============================================================================
/// 한 장면 그리기.
//==============================================================================
fn capture(gl: &GlContext, framebuffer: glow::Framebuffer, extent: f64, resolution: i32, wall_height: f64, receives_shadow: bool) -> Captured {
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let mut floor_builder = MeshBuilder::new();
	floor_builder.add_part(&create_box_geometry(12.0, 0.2, 12.0), &Matrix4::create_translation(0.0, -0.1, 0.0), "#d0c0a0", ToonPartOptions::default());
	let mut wall_builder = MeshBuilder::new();
	wall_builder.add_part(&create_box_geometry(1.0, wall_height, 3.0), &Matrix4::create_translation(0.0, wall_height * 0.5, 0.0), "#a0a0d0", ToonPartOptions::default());
	let floor_mesh = renderer.create_mesh(gl, &floor_builder.read_arrays()).expect("바닥 모형");
	let wall_mesh = renderer.create_mesh(gl, &wall_builder.read_arrays()).expect("벽 모형");
	let mut shadow_map = ShadowMap::new(gl, resolution).expect("그림자 판");
	// 빛이 나아가는 방향. (+x 면과 약 84 도, +z 면과 약 73 도로 스친다, 그림자는 -x, -z 쪽)
	let light_direction = [-0.1, -1.0, -0.3];
	// 면을 보는 카메라와 그늘 쪽 바닥을 내려다보는 카메라. (둘 다 직교 원 행렬)
	let side_view_matrix = Matrix4::create_look_at(Vector3::create(8.0, 6.0, 8.0), Vector3::create(0.0, 1.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	let side_projection_matrix = Matrix4::create_orthographic(-4.0, 4.0, -3.0, 3.0, 0.1, 40.0);
	let floor_view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 8.0, -1.801), Vector3::create(0.0, 0.0, -1.8), Vector3::create(0.0, 0.0, 1.0));
	let floor_projection_matrix = Matrix4::create_orthographic(-1.0, 1.0, -0.75, 0.75, 0.1, 40.0);
	let draw = |renderer: &mut ToonRenderer, shadow_map: &mut ShadowMap, view_matrix: &Matrix4, projection_matrix: &Matrix4| {
		shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(light_direction[0], light_direction[1], light_direction[2]), extent, 1.0, 100.0);
		let mut queue = ToonDrawQueue::new();
		queue.add_mesh(&floor_mesh, &Matrix4::create_identity(), ToonDrawOptions { has_outline: false, receives_shadow, ..ToonDrawOptions::default() });
		queue.add_mesh(&wall_mesh, &Matrix4::create_identity(), ToonDrawOptions { has_outline: false, receives_shadow, ..ToonDrawOptions::default() });
		queue.draw_shadows(gl, renderer, shadow_map);
		renderer.set_sun_direction(light_direction);
		renderer.set_shadow(Some(shadow_map), 1.0);
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.viewport(0, 0, WIDTH as i32, HEIGHT as i32);
			gl.clear_color(0.0, 0.0, 0.0, 1.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
		queue.draw_with_matrices(gl, renderer, view_matrix, projection_matrix);
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		}
	};
	let read_pixels = |x: i32, y: i32, width: i32, height: i32| -> Vec<u8> {
		let mut pixels = vec![0u8; (width * height * 4) as usize];
		unsafe {
			gl.read_pixels(x, y, width, height, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
		}
		pixels
	};
	draw(&mut renderer, &mut shadow_map, &side_view_matrix, &side_projection_matrix);
	let mut side_view_projection_matrix = side_projection_matrix;
	side_view_projection_matrix.multiply(&side_view_matrix);
	let side_point = read_point(&side_view_projection_matrix, 0.5, wall_height * 0.5, 0.0);
	let front_point = read_point(&side_view_projection_matrix, 0.0, wall_height * 0.5, 1.5);
	let side = read_pixels(side_point[0] - 3, side_point[1] - 3, 7, 7);
	let front = read_pixels(front_point[0] - 3, front_point[1] - 3, 7, 7);
	draw(&mut renderer, &mut shadow_map, &floor_view_matrix, &floor_projection_matrix);
	let mut floor_view_projection_matrix = floor_projection_matrix;
	floor_view_projection_matrix.multiply(&floor_view_matrix);
	let mut line = Vec::new();
	for step in 0..160 {
		let point = read_point(&floor_view_projection_matrix, 0.0, 0.0, -1.5 - step as f64 * 0.005);
		let pixel = read_pixels(point[0], point[1], 1, 1);
		line.push(pixel[0] as i32 + pixel[1] as i32 + pixel[2] as i32);
	}
	let error = unsafe { gl.get_error() };
	assert_eq!(error, 0, "시험 실패: GL 오류 없음 ({})", error);
	renderer.delete_mesh(gl, floor_mesh);
	renderer.delete_mesh(gl, wall_mesh);
	shadow_map.destroy(gl);
	Captured { side, front, line }
}

//==============================================================================
/// 그림자 받기 켜고 끈 두 장의 차이.
//==============================================================================
fn measure(gl: &GlContext, framebuffer: glow::Framebuffer, extent: f64, resolution: i32, wall_height: f64) -> Measured {
	let with_shadow = capture(gl, framebuffer, extent, resolution, wall_height, true);
	let without_shadow = capture(gl, framebuffer, extent, resolution, wall_height, false);
	let read_darkening = |shadowed: &[u8], unshadowed: &[u8]| -> f64 {
		let mut sum = 0.0;
		let mut count = 0.0;
		for index in (0..shadowed.len()).step_by(4) {
			for channel in 0..3 {
				sum += unshadowed[index + channel] as f64 - shadowed[index + channel] as f64;
				count += 1.0;
			}
		}
		sum / count
	};
	let dark_list: Vec<i32> = with_shadow.line.iter().zip(without_shadow.line.iter()).map(|(value, other)| other - value).collect();
	let deep = dark_list.iter().copied().max().unwrap_or(0);
	let mut first: Option<usize> = None;
	let mut last: Option<usize> = None;
	for (index, value) in dark_list.iter().enumerate() {
		if *value as f64 > deep as f64 * 0.5 {
			first = first.or(Some(index));
			last = Some(index);
		}
	}
	Measured {
		side: read_darkening(&with_shadow.side, &without_shadow.side),
		front: read_darkening(&with_shadow.front, &without_shadow.front),
		deep,
		band_start: first.map(|index| -1.5 - index as f64 * 0.005),
		band_end: last.map(|index| -1.5 - index as f64 * 0.005),
	}
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
	let coarse = measure(gl, framebuffer, 40.0, 512, 2.0);
	let low = measure(gl, framebuffer, 2.0, 2048, 0.1);
	let tall = measure(gl, framebuffer, 8.0, 2048, 2.0);
	unsafe {
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		gl.delete_framebuffer(framebuffer);
		gl.delete_renderbuffer(depth_renderbuffer);
		gl.delete_texture(color_texture);
	}
	println!("  extent 40 해상도 512: 84 도 면 {:.1}, 73 도 면 {:.1}", coarse.side, coarse.front);
	println!("  extent 2 높이 0.1: 그림자 {:?} ~ {:?} (깊이 {}), extent 8 높이 2: {:?} ~ {:?}", low.band_start, low.band_end, low.deep, tall.band_start, tall.band_end);
	// 1. 여드름이 없다.
	check(&mut check_count, coarse.side < 1.0, &format!("빛과 84 도로 스치는 면이 제 그림자를 받지 않음 (어두워진 몫 {:.1})", coarse.side));
	check(&mut check_count, coarse.front < 1.0, &format!("빛과 73 도로 스치는 면이 제 그림자를 받지 않음 (어두워진 몫 {:.1})", coarse.front));
	// 2. 낮은 벽도 붙은 그림자를 드리운다. (높이 0.1 x 해의 기울기 0.3 = 밑동에서 0.03)
	check(&mut check_count, low.band_start.is_some() && low.deep > 60, &format!("높이 0.1 의 벽이 그림자를 드리움 (깊이 {})", low.deep));
	check(&mut check_count, low.band_start.unwrap_or(-9.0) >= -1.51, &format!("낮은 벽의 그림자가 밑동에 붙음 (시작 {:?})", low.band_start));
	check(&mut check_count, (low.band_end.unwrap_or(-9.0) - -1.53).abs() <= 0.01, &format!("낮은 벽의 그림자 끝이 해 방향대로 z -1.53 (끝 {:?})", low.band_end));
	// 3. 높은 벽의 그림자는 밑동부터 끝까지.
	check(&mut check_count, tall.band_start.unwrap_or(-9.0) >= -1.51, &format!("높은 벽의 그림자가 밑동에 붙음 (시작 {:?})", tall.band_start));
	check(&mut check_count, (tall.band_end.unwrap_or(-9.0) - -2.1).abs() <= 0.02, &format!("높은 벽의 그림자 끝이 해 방향대로 z -2.1 (끝 {:?})", tall.band_end));
	println!("해 그림자 치우침 찍기 시험 통과 ({} 검사)", check_count);
}
