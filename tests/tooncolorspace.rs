//==============================================================================
//! 색 공간 칸의 찍기 시험. (원본 tests/graphics3d/tooncolorspacechrome.test.mjs 와 같은 검사)
//! - Engine::new(color_space Linear) 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 감마와 선형 색 공간의 툰 렌더러를 짓는다.
//! - 텍스처 형식: 선형 색 공간의 색 텍스처는 SRGB8_ALPHA8(프레임버퍼에 붙이면 색 부호화 SRGB), 자료 텍스처(is_color_data: false)와 감마 색 공간의 텍스처는 LINEAR.
//! - 셰이더: 선형만 소스마다 #define VANILLA_COLOR_SPACE_LINEAR 1 이 #version 다음 줄에 있고, 감마 소스는 정의가 없다. 선형 프로그램, 후처리, 화면 바깥선이 지어진다.
//! - 섞기: 빛을 받지 않는 검정 몸을 흰 틴트와 반씩 섞으면 감마는 sRGB 로 섞어 128 쪽, 선형은 선형으로 섞어 188 쪽이다. (sRGB 표준 곡선 0.5 → 0.735)
//!   TOON_COLOR_SPACE_BASE_PATH(고치기 전 사본이 남긴 .rgba 폴더)를 주면 감마 섞기 화면이 고치기 전과 바이트까지 같다.
//! - 엔진: EngineConfiguration::color_space 기본은 Gamma, Linear 로 지은 엔진은 Engine, Graphic, 툰 렌더러, 후처리, 화면 바깥선 기본값이 모두 Linear.
//!   (엔진이 없으면 Gamma 인 것은 colorspace.rs 의 단위 시험)
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test tooncolorspace
//==============================================================================
use vanilla::{compose_toon_program_sources, create_box_geometry, ColorSpace, Engine, EngineConfiguration, GlContext, Matrix4, MeshBuilder, ToonDrawOptions, ToonPartOptions, ToonPostProcess, ToonPostProcessOptions, ToonRenderer, ToonRendererOptions, ToonScreenOutline, ToonScreenOutlineOptions, ToonTextureOptions, Vector3};

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
/// 텍스처의 색 부호화 읽기. (프레임버퍼에 붙여 FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING)
//==============================================================================
fn read_encoding(gl: &GlContext, texture: glow::Texture) -> &'static str {
	unsafe {
		let framebuffer = gl.create_framebuffer().unwrap();
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
		let encoding = gl.get_framebuffer_attachment_parameter_i32(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING) as u32;
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		gl.delete_framebuffer(framebuffer);
		gl.delete_texture(texture);
		match encoding {
			glow::SRGB => {
				"srgb"
			}
			glow::LINEAR => {
				"linear"
			}
			_ => {
				"other"
			}
		}
	}
}

//==============================================================================
/// 검정 몸을 흰 틴트와 반씩 섞은 64 x 64 화면. (원본 readMixedPixel, 왼쪽 아래 원점 RGBA)
//==============================================================================
fn read_mixed_pixels(gl: &GlContext, renderer: &mut ToonRenderer) -> Vec<u8> {
	let mut pixels = vec![0u8; 64 * 64 * 4];
	unsafe {
		let color_texture = gl.create_texture().unwrap();
		gl.bind_texture(glow::TEXTURE_2D, Some(color_texture));
		gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 64, 64, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
		gl.bind_texture(glow::TEXTURE_2D, None);
		let depth_renderbuffer = gl.create_renderbuffer().unwrap();
		gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_renderbuffer));
		gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, 64, 64);
		gl.bind_renderbuffer(glow::RENDERBUFFER, None);
		let framebuffer = gl.create_framebuffer().unwrap();
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
		gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth_renderbuffer));
		let mut builder = MeshBuilder::new();
		builder.add_part(&create_box_geometry(4.0, 4.0, 0.2), &Matrix4::create_identity(), "#000000", ToonPartOptions { is_unlit: true, has_outline: false, ..ToonPartOptions::default() });
		let mesh = renderer.create_mesh(gl, &builder.read_arrays()).unwrap();
		gl.viewport(0, 0, 64, 64);
		gl.clear_color(0.0, 0.0, 0.0, 1.0);
		gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 3.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let projection_matrix = Matrix4::create_perspective(0.8, 1.0, 0.1, 10.0);
		renderer.begin_frame_with_matrices(gl, &view_matrix, &projection_matrix);
		renderer.draw_mesh(gl, &mesh, &Matrix4::create_identity(), ToonDrawOptions { tint_color: [1.0, 1.0, 1.0, 1.0], tint_amount: Some(0.5), has_outline: false, ..ToonDrawOptions::default() });
		renderer.end_frame(gl);
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.read_pixels(0, 0, 64, 64, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		renderer.delete_mesh(gl, mesh);
		gl.delete_framebuffer(framebuffer);
		gl.delete_renderbuffer(depth_renderbuffer);
		gl.delete_texture(color_texture);
	}
	pixels
}

fn main() {
	let mut check_count = 0u32;
	check(&mut check_count, EngineConfiguration::new().color_space == ColorSpace::Gamma, "EngineConfiguration::color_space 기본은 Gamma");
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.color_space = ColorSpace::Linear;
	let engine = Engine::new(engine_configuration);
	let graphic = engine.get_graphic();
	check(&mut check_count, graphic.is_some(), "GL 컨텍스트를 짓지 못함 (화면 서버)");
	let graphic = graphic.unwrap();
	let gl = graphic.get_gl();
	let mut gamma_renderer = ToonRenderer::new_with_options(gl, ToonRendererOptions { color_space: ColorSpace::Gamma }).expect("감마 렌더러");
	let mut linear_renderer = ToonRenderer::new_with_options(gl, ToonRendererOptions { color_space: ColorSpace::Linear }).expect("선형 렌더러");
	let pixel = [200u8, 100, 50, 255];
	let not_mipmapped = ToonTextureOptions { is_mipmapped: false, ..ToonTextureOptions::default() };
	let gamma_color_encoding = read_encoding(gl, gamma_renderer.create_texture(gl, 1, 1, &pixel, not_mipmapped).unwrap());
	let gamma_data_encoding = read_encoding(gl, gamma_renderer.create_texture(gl, 1, 1, &pixel, ToonTextureOptions { is_color_data: false, ..not_mipmapped }).unwrap());
	let linear_color_encoding = read_encoding(gl, linear_renderer.create_texture(gl, 1, 1, &pixel, ToonTextureOptions::default()).unwrap());
	let linear_data_encoding = read_encoding(gl, linear_renderer.create_texture(gl, 1, 1, &pixel, ToonTextureOptions { is_color_data: false, ..ToonTextureOptions::default() }).unwrap());
	let gamma_sources = compose_toon_program_sources(None, None, false, ColorSpace::Gamma);
	let linear_sources = compose_toon_program_sources(None, None, false, ColorSpace::Linear);
	let read_list = |sources: &vanilla::ToonProgramSources| -> Vec<String> {
		vec![sources.solid_vertex_shader.clone(), sources.solid_fragment_shader.clone(), sources.outline_vertex_shader.clone(), sources.outline_fragment_shader.clone(), sources.depth_vertex_shader.clone(), sources.depth_fragment_shader.clone()]
	};
	let gamma_has_define = read_list(&gamma_sources).iter().any(|source| source.contains("VANILLA_COLOR_SPACE_LINEAR 1"));
	let linear_has_define = read_list(&linear_sources).iter().all(|source| source.starts_with("#version 300 es\n#define VANILLA_COLOR_SPACE_LINEAR 1\n"));
	let engine_sources = ToonRenderer::read_program_sources();
	let linear_list = read_list(&linear_sources);
	let is_engine_default_linear = engine_sources.iter().enumerate().all(|(index, (vertex_shader, fragment_shader))| *vertex_shader == linear_list[index * 2] && *fragment_shader == linear_list[index * 2 + 1]);
	let linear_post_process = ToonPostProcess::new_with_options(gl, 64, 64, ToonPostProcessOptions { color_space: ColorSpace::Linear, ..ToonPostProcessOptions::default() }).expect("선형 후처리");
	let gamma_post_process = ToonPostProcess::new_with_options(gl, 64, 64, ToonPostProcessOptions { color_space: ColorSpace::Gamma, ..ToonPostProcessOptions::default() }).expect("감마 후처리");
	let linear_screen_outline = ToonScreenOutline::new_with_options(gl, ToonScreenOutlineOptions { color_space: ColorSpace::Linear }).expect("선형 화면 바깥선");
	let linear_ambient_occlusion_sources = compose_toon_program_sources(None, None, true, ColorSpace::Linear);
	let gamma_mixed = read_mixed_pixels(gl, &mut gamma_renderer);
	let linear_mixed = read_mixed_pixels(gl, &mut linear_renderer);
	let center = (32 * 64 + 32) * 4;
	let gamma_center = &gamma_mixed[center..center + 4];
	let linear_center = &linear_mixed[center..center + 4];
	println!("  감마 섞기 {:?}, 선형 섞기 {:?}, 부호화 감마 {}/{}, 선형 {}/{}", gamma_center, linear_center, gamma_color_encoding, gamma_data_encoding, linear_color_encoding, linear_data_encoding);
	let error = unsafe { gl.get_error() };
	check(&mut check_count, error == 0, &format!("GL 오류 없음 ({})", error));
	check(&mut check_count, gamma_renderer.get_color_space() == ColorSpace::Gamma, "color_space: Gamma 로 지은 툰 렌더러");
	check(&mut check_count, linear_renderer.get_color_space() == ColorSpace::Linear, "color_space: Linear 로 지은 툰 렌더러");
	check(&mut check_count, gamma_color_encoding == "linear" && gamma_data_encoding == "linear", "감마 색 공간의 텍스처는 RGBA (부호화 LINEAR)");
	check(&mut check_count, linear_color_encoding == "srgb", "선형 색 공간의 색 텍스처는 SRGB8_ALPHA8 (부호화 SRGB)");
	check(&mut check_count, linear_data_encoding == "linear", "선형 색 공간의 자료 텍스처(is_color_data: false)는 RGBA8 (부호화 LINEAR)");
	check(&mut check_count, !gamma_has_define, "감마 소스에는 정의가 없음");
	check(&mut check_count, linear_has_define, "선형 소스마다 #version 다음 줄에 정의");
	check(&mut check_count, linear_ambient_occlusion_sources.solid_fragment_shader.starts_with("#version 300 es\n#define VANILLA_COLOR_SPACE_LINEAR 1\n#define TOON_AMBIENT_OCCLUSION\n"), "선형 모서리 어둠 몸 소스는 색 공간 정의 다음에 모서리 어둠 정의 (원본과 같은 차례)");
	check(&mut check_count, is_engine_default_linear, "엔진이 선형이면 read_program_sources 는 선형 소스");
	check(&mut check_count, linear_post_process.get_color_space() == ColorSpace::Linear && gamma_post_process.get_color_space() == ColorSpace::Gamma && linear_screen_outline.get_color_space() == ColorSpace::Linear, "후처리와 화면 바깥선이 준 색 공간으로 지어짐");
	check(&mut check_count, (gamma_center[0] as i32 - 128).abs() <= 1 && gamma_center[0] == gamma_center[1], &format!("감마는 sRGB 로 섞음 ({:?})", gamma_center));
	check(&mut check_count, (linear_center[0] as i32 - 188).abs() <= 1 && linear_center[0] == linear_center[1], &format!("선형은 선형으로 섞음 ({:?})", linear_center));
	match std::env::var("TOON_COLOR_SPACE_BASE_PATH") {
		Ok(base_path) => {
			let base = std::fs::read(format!("{}/gammaMixed.rgba", base_path)).expect("고치기 전 감마 섞기 화면");
			let different = gamma_mixed.iter().zip(base.iter()).filter(|(left, right)| left != right).count();
			println!("  감마 섞기: 고치기 전과 다른 바이트 {}", different);
			check(&mut check_count, base.len() == gamma_mixed.len() && different == 0, "감마 섞기 화면이 고치기 전과 바이트까지 같음");
		}
		Err(_) => {
			println!("  (TOON_COLOR_SPACE_BASE_PATH 를 주지 않아 고치기 전과는 견주지 않음)");
		}
	}
	check(&mut check_count, engine.get_color_space() == ColorSpace::Linear && graphic.get_color_space() == ColorSpace::Linear, "엔진과 Graphic 의 색 공간");
	let engine_renderer = ToonRenderer::new(gl).expect("엔진 기본 렌더러");
	let engine_post_process = ToonPostProcess::new(gl, 64, 64).expect("엔진 기본 후처리");
	let engine_screen_outline = ToonScreenOutline::new(gl).expect("엔진 기본 화면 바깥선");
	check(&mut check_count, engine_renderer.get_color_space() == ColorSpace::Linear && engine_post_process.get_color_space() == ColorSpace::Linear && engine_screen_outline.get_color_space() == ColorSpace::Linear, "툰 렌더러, 후처리, 화면 바깥선 기본값은 엔진의 색 공간");
	println!("툰 색 공간 찍기 시험 통과 ({} 검사)", check_count);
}
