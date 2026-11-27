//==============================================================================
// 블렌드 모드 예제. (Graphic 의 블렌드 모드 26 가지를 칸마다 하나씩 건다)
// - 칸마다 네 가지 색 줄무늬와 흰색, 회색, 검정, 보라 바탕 위에, 그 칸의 모드로 그린다.
//   고정 블렌딩 13 가지와 그릴 영역 아래를 떠 와 셰이더로 섞는 13 가지(overlay ~ luminosity)가 모두 나온다.
// - 60 프레임마다 장면이 바뀐다. (오른쪽 아래에 장면 이름)
//   0. 반투명 그림, 사각형, 다각형. (원본 시험과 같은 장면이라 크롬 Canvas2D 로 그린 것과 픽셀로 견줄 수 있다)
//   1. 글자, 나인슬라이스, 자르기 안의 그림.
//   2. 겹치는 파티클 24 개를 draw_colored_quads 로. 3. 같은 쿼드를 하나씩 draw_vertices 로. (둘이 같아야 한다)
//   4. 기본 셰이더와 같은 글로 만든 바꿔 끼운 셰이더로 돌리고 틴트를 건 그림과 자르기 안의 나인슬라이스. 5. 같은 장면을 기본 셰이더로. (둘이 같아야 한다)
//   6, 7. 투명하게 비운 렌더 타겟(RGBA8)에 바탕 열마다 알파를 255, 192, 102, 0 으로 깔고 장면 0 을 그린 뒤, 6 은 검정 위에, 7 은 흰색 위에 옮긴다.
//      (반투명 바탕의 섞기를 크롬의 반투명 Canvas2D 와 견줄 수 있다. 검정 위는 프리멀티플라이드 색이고, 흰색 위와의 차이에서 알파가 나온다)
// - 렌더 배율을 1 로 묶어 창 크기(1120x640) 그대로 그린다.
// - 실행: cargo run --release --example blendmodes
//   화면 저장: VANILLA_CAPTURE_PATH=shots/blend_{frame}.png VANILLA_CAPTURE_FRAMES=30,90,150,210,270,330,390,450 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::rc::Rc;
use vanilla::{BlendMode, Color, Engine, EngineConfiguration, Graphic, Image, Rect, RenderTarget, Scene, SceneBase, TextAlign, TextBaseline, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f64 = 1120.0;
const VIEW_HEIGHT: f64 = 640.0;
const CELL_SIZE: f64 = 160.0;
const COLUMN_COUNT: usize = 7;
const PAGE_FRAME_COUNT: u64 = 60;
const PAGE_COUNT: u64 = 8;
const IMAGE_SIZE: u32 = 96;
const NINE_SLICE_IMAGE_SIZE: u32 = 30;
const PARTICLE_COUNT: usize = 24;
const BLEND_MODE_NAME_LIST: [&str; 26] = ["source-over", "source-in", "source-out", "source-atop", "destination-over", "destination-in", "destination-out", "destination-atop", "lighter", "copy", "xor", "multiply", "screen", "overlay", "darken", "lighten", "color-dodge", "color-burn", "hard-light", "soft-light", "difference", "exclusion", "hue", "saturation", "color", "luminosity"];
const STRIPE_COLOR_LIST: [[u8; 3]; 4] = [[224, 60, 49], [47, 168, 79], [45, 108, 223], [242, 193, 78]];
const BAND_COLOR_LIST: [[u8; 3]; 4] = [[255, 255, 255], [128, 128, 128], [0, 0, 0], [138, 43, 226]];
// 반투명 렌더 타겟 장면의 바탕 열 알파. (원본 시험과 같다)
const TRANSLUCENT_COLUMN_ALPHA_LIST: [u8; 4] = [255, 192, 102, 0];
const OPAQUE_COLUMN_ALPHA_LIST: [u8; 4] = [255, 255, 255, 255];
const PAGE_NAME_LIST: [&str; 8] = ["그림, 사각형, 다각형", "글자, 나인슬라이스, 자르기", "파티클 (묶음)", "파티클 (낱장)", "바꿔 끼운 셰이더", "기본 셰이더", "반투명 렌더 타겟 (검정 위)", "반투명 렌더 타겟 (흰색 위)"];

// 기본 셰이더와 같은 글의 바꿔 끼울 셰이더. (덧그림 칸을 거쳐 섞어도 기본 셰이더로 그린 것과 같아야 한다)
const IDENTITY_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
uniform mat3 projectionMatrix;
uniform mat3 modelMatrix;
out vec2 fragmentTextureCoordinate;
void main() {
	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
}
";
const IDENTITY_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D mainTexture;
uniform vec4 mainColor;
uniform vec4 tintColor;
uniform float globalAlpha;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate);
	vec3 tintedColor = mix(textureColor.rgb, tintColor.rgb * textureColor.a, tintColor.a);
	float finalAlpha = textureColor.a * mainColor.a * globalAlpha;
	vec3 finalColor = tintedColor * mainColor.rgb * mainColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

//==============================================================================
// 칸의 왼쪽 위.
//==============================================================================
fn get_cell_origin(mode_index: usize) -> (f64, f64) {
	((mode_index % COLUMN_COUNT) as f64 * CELL_SIZE, (mode_index / COLUMN_COUNT) as f64 * CELL_SIZE)
}

//==============================================================================
// 시험 그림. (96x96, 왼쪽에서 오른쪽으로 빨강이, 위에서 아래로 초록이 늘고 가운데에서 멀수록 투명해진다)
//==============================================================================
fn create_test_image() -> Rc<Image> {
	let mut pixels = vec![0u8; (IMAGE_SIZE * IMAGE_SIZE * 4) as usize];
	for y in 0..IMAGE_SIZE as i32 {
		for x in 0..IMAGE_SIZE as i32 {
			let offset = ((y * IMAGE_SIZE as i32 + x) * 4) as usize;
			let delta_x = x - 48;
			let delta_y = y - 48;
			let distance_squared = delta_x * delta_x + delta_y * delta_y;
			pixels[offset] = (x * 255 / 95) as u8;
			pixels[offset + 1] = (y * 255 / 95) as u8;
			pixels[offset + 2] = ((95 - x) * 255 / 95) as u8;
			pixels[offset + 3] = if distance_squared >= 2304 {
				0
			}
			else {
				(255 - distance_squared * 255 / 2304) as u8
			};
		}
	}
	Image::create_from_pixels(IMAGE_SIZE, IMAGE_SIZE, pixels)
}

//==============================================================================
// 나인슬라이스 그림. (30x30, 테두리 6 픽셀은 짙은 반투명, 가운데는 옅은 반투명)
//==============================================================================
fn create_nine_slice_image() -> Rc<Image> {
	let mut pixels = vec![0u8; (NINE_SLICE_IMAGE_SIZE * NINE_SLICE_IMAGE_SIZE * 4) as usize];
	for y in 0..NINE_SLICE_IMAGE_SIZE {
		for x in 0..NINE_SLICE_IMAGE_SIZE {
			let offset = ((y * NINE_SLICE_IMAGE_SIZE + x) * 4) as usize;
			let is_border = x < 6 || y < 6 || x >= 24 || y >= 24;
			let pixel: [u8; 4] = if is_border {
				[250, 230, 40, 230]
			}
			else {
				[90, 200, 250, 120]
			};
			pixels[offset..offset + 4].copy_from_slice(&pixel);
		}
	}
	Image::create_from_pixels(NINE_SLICE_IMAGE_SIZE, NINE_SLICE_IMAGE_SIZE, pixels)
}

//==============================================================================
// 원을 닮은 다각형의 꼭짓점. (Graphic::draw_circle 과 같은 분할)
//==============================================================================
fn create_polygon_positions(center_x: f64, center_y: f64, radius: f64) -> Vec<Vector2> {
	let segment_count = (radius * 0.5).ceil().clamp(16.0, 64.0) as usize;
	let mut positions = Vec::with_capacity(segment_count);
	for segment_index in 0..segment_count {
		let radian = std::f64::consts::PI * 2.0 * (segment_index as f64 / segment_count as f64);
		positions.push(Vector2::create(center_x + radian.cos() * radius, center_y + radian.sin() * radius));
	}
	positions
}

//==============================================================================
// 파티클 하나. (가운데, 크기, 색)
//==============================================================================
struct ParticleQuad {
	x: f64,
	y: f64,
	size: f64,
	color: [u8; 4],
}

//==============================================================================
// 칸의 파티클. (서로 겹치는 부드러운 원 24 개, 색 여러 가지)
//==============================================================================
fn list_particle_quads(cell_x: f64, cell_y: f64) -> Vec<ParticleQuad> {
	let mut particle_quad_list = Vec::with_capacity(PARTICLE_COUNT);
	for index in 0..PARTICLE_COUNT {
		particle_quad_list.push(ParticleQuad {
			x: cell_x + 20.0 + ((index * 37) % 120) as f64,
			y: cell_y + 20.0 + ((index * 53) % 120) as f64,
			size: 18.0 + ((index * 7) % 30) as f64,
			color: [((index * 97) % 256) as u8, ((index * 61 + 80) % 256) as u8, ((index * 31 + 150) % 256) as u8, (120 + (index * 17) % 136) as u8],
		});
	}
	particle_quad_list
}

//==============================================================================
// 블렌드 모드 장면.
//==============================================================================
struct BlendModeScene {
	scene_base: SceneBase,
	test_image: Rc<Image>,
	nine_slice_image: Rc<Image>,
	// 장면 6, 7 의 렌더 타겟. (처음 그릴 때 드로잉 버퍼 크기로 만든다)
	render_target: Option<RenderTarget>,
}

impl BlendModeScene {
	//==============================================================================
	// 칸 바탕. (네 가지 색 줄무늬와 흰색, 회색, 검정, 보라 띠, 열마다 column_alpha_list 의 알파, source-over)
	//==============================================================================
	fn draw_cell_background(&self, graphic: &mut Graphic, cell_x: f64, cell_y: f64, column_alpha_list: [u8; 4]) {
		graphic.set_blend_mode(BlendMode::Normal);
		for column_index in 0..4 {
			let column_x = cell_x + column_index as f64 * 40.0;
			let column_alpha = column_alpha_list[column_index];
			let stripe_color = STRIPE_COLOR_LIST[column_index];
			graphic.set_fill_color(Color::create_from_bytes(stripe_color[0], stripe_color[1], stripe_color[2], column_alpha));
			graphic.draw_rect(Rect::create(column_x, cell_y, 40.0, 120.0));
			let band_color = BAND_COLOR_LIST[column_index];
			graphic.set_fill_color(Color::create_from_bytes(band_color[0], band_color[1], band_color[2], column_alpha));
			graphic.draw_rect(Rect::create(column_x, cell_y + 120.0, 40.0, 40.0));
		}
	}

	//==============================================================================
	// 장면 0. (반투명 그림, 반투명 사각형, 반투명 다각형)
	//==============================================================================
	fn draw_shape_page(&self, graphic: &mut Graphic, cell_x: f64, cell_y: f64) {
		graphic.draw_image(&self.test_image, Vector2::create(cell_x + 8.0, cell_y + 8.0), Vector2::create(IMAGE_SIZE as f64, IMAGE_SIZE as f64));
		graphic.set_fill_color(Color::create_from_bytes(40, 220, 255, 140));
		graphic.draw_rect(Rect::create(cell_x + 56.0, cell_y + 28.0, 88.0, 52.0));
		let polygon_positions = create_polygon_positions(cell_x + 104.0, cell_y + 104.0, 38.0);
		graphic.draw_convex_polygon(&polygon_positions, Color::create_from_bytes(255, 140, 0, 179));
	}

	//==============================================================================
	// 장면 1. (글자, 나인슬라이스, 자르기 안의 그림)
	//==============================================================================
	fn draw_path_page(&self, graphic: &mut Graphic, cell_x: f64, cell_y: f64) {
		graphic.set_font_string("bold 30px sans-serif");
		graphic.set_fill_color(Color::create_from_bytes(255, 255, 255, 204));
		graphic.set_text_baseline(TextBaseline::Alphabetic);
		graphic.set_text_align(TextAlign::Left);
		graphic.draw_fill_text("Ag가", cell_x + 6.0, cell_y + 36.0);
		graphic.draw_image_with_nine_slice(&self.nine_slice_image, Vector2::create(cell_x + 8.0, cell_y + 50.0), Vector2::create(92.0, 56.0), Rect::create(10.0, 10.0, 10.0, 10.0));
		graphic.begin_clip_rect(Rect::create(cell_x + 104.0, cell_y + 8.0, 48.0, 144.0), 0.0);
		graphic.draw_image(&self.test_image, Vector2::create(cell_x + 90.0, cell_y + 60.0), Vector2::create(IMAGE_SIZE as f64, IMAGE_SIZE as f64));
		graphic.end_clip_rect();
	}

	//==============================================================================
	// 장면 2, 3. (겹치는 파티클, is_batched 면 draw_colored_quads 한 번, 아니면 쿼드마다 draw_vertices)
	//==============================================================================
	fn draw_particle_page(&self, graphic: &mut Graphic, cell_x: f64, cell_y: f64, is_batched: bool) {
		let soft_disc_texture = graphic.get_soft_disc_texture();
		let particle_quad_list = list_particle_quads(cell_x, cell_y);
		if is_batched {
			let vertex_data = graphic.get_particle_vertex_data(particle_quad_list.len() * 48);
			let mut offset = 0;
			for particle_quad in particle_quad_list.iter() {
				let half_size = particle_quad.size * 0.5;
				let corner_list: [[f64; 4]; 6] = [[-half_size, -half_size, 0.0, 0.0], [half_size, -half_size, 1.0, 0.0], [half_size, half_size, 1.0, 1.0], [-half_size, -half_size, 0.0, 0.0], [half_size, half_size, 1.0, 1.0], [-half_size, half_size, 0.0, 1.0]];
				for corner in corner_list {
					let vertex = [particle_quad.x + corner[0], particle_quad.y + corner[1], corner[2], corner[3], particle_quad.color[0] as f64 / 255.0, particle_quad.color[1] as f64 / 255.0, particle_quad.color[2] as f64 / 255.0, particle_quad.color[3] as f64 / 255.0];
					vertex_data[offset..offset + 8].copy_from_slice(&vertex.map(|value| value as f32));
					offset += 8;
				}
			}
			graphic.draw_colored_quads(offset / 8, soft_disc_texture);
		}
		else {
			for particle_quad in particle_quad_list.iter() {
				let half_size = particle_quad.size * 0.5;
				let vertex_count = graphic.write_quad(0, particle_quad.x - half_size, particle_quad.y - half_size, particle_quad.x + half_size, particle_quad.y + half_size, 0.0, 0.0, 1.0, 1.0) / 4;
				let color = Color::create_from_bytes(particle_quad.color[0], particle_quad.color[1], particle_quad.color[2], particle_quad.color[3]);
				graphic.draw_vertices(vertex_count, soft_disc_texture, color);
			}
		}
	}

	//==============================================================================
	// 장면 4, 5. (돌리고 틴트를 건 그림과 자르기 안의 나인슬라이스, is_overridden 이면 바꿔 끼운 셰이더로)
	//==============================================================================
	fn draw_override_page(&self, graphic: &mut Graphic, cell_x: f64, cell_y: f64, is_overridden: bool) {
		if is_overridden {
			let identity_shader_program = graphic.resolve_shader_program("blendmodes", "identity", || (IDENTITY_VERTEXSHADER_SOURCE.to_string(), IDENTITY_FRAGMENTSHADER_SOURCE.to_string()));
			graphic.set_shader_program_override(identity_shader_program);
		}
		graphic.push_state();
		graphic.translate(cell_x + 60.0, cell_y + 60.0);
		graphic.rotate(0.3);
		graphic.set_global_alpha(0.85);
		graphic.set_image_tint_color(Some(Color::create(1.0, 0.0, 0.5, 0.3)));
		graphic.draw_image(&self.test_image, Vector2::create(-48.0, -48.0), Vector2::create(IMAGE_SIZE as f64, IMAGE_SIZE as f64));
		graphic.set_image_tint_color(None);
		graphic.pop_state();
		graphic.begin_clip_rect(Rect::create(cell_x + 80.0, cell_y + 70.0, 70.0, 80.0), 0.0);
		graphic.draw_image_with_nine_slice(&self.nine_slice_image, Vector2::create(cell_x + 60.0, cell_y + 80.0), Vector2::create(96.0, 70.0), Rect::create(10.0, 10.0, 10.0, 10.0));
		graphic.end_clip_rect();
		if is_overridden {
			graphic.set_shader_program_override(None);
		}
	}

	//==============================================================================
	// 장면 6, 7. (투명하게 비운 렌더 타겟에 반투명 바탕과 장면 0 을 칸마다 그 칸의 모드로 그리고, 검정이나 흰색 위에 옮긴다)
	//==============================================================================
	fn draw_render_target_page(&mut self, graphic: &mut Graphic, is_white_background: bool) {
		let drawing_buffer_width = graphic.get_drawing_buffer_width();
		let drawing_buffer_height = graphic.get_drawing_buffer_height();
		graphic.flush_batch();
		if self.render_target.is_none() {
			match RenderTarget::new(graphic.get_gl(), drawing_buffer_width, drawing_buffer_height, false, None) {
				Ok(render_target) => {
					self.render_target = Some(render_target);
				}
				Err(error) => {
					eprintln!("blendmodes: render target failed: {}", error);
					return;
				}
			}
		}
		let render_target_texture = match self.render_target.as_ref().and_then(|render_target| render_target.get_color_texture(0)) {
			Some(render_target_texture) => {
				render_target_texture
			}
			None => {
				return;
			}
		};
		if let Some(render_target) = &self.render_target {
			let gl = graphic.get_gl();
			render_target.bind(gl);
			unsafe {
				gl.clear_color(0.0, 0.0, 0.0, 0.0);
				gl.clear(glow::COLOR_BUFFER_BIT);
			}
		}
		for (mode_index, blend_mode_name) in BLEND_MODE_NAME_LIST.iter().enumerate() {
			let blend_mode = match BlendMode::from_name(blend_mode_name) {
				Some(blend_mode) => {
					blend_mode
				}
				None => {
					continue;
				}
			};
			let (cell_x, cell_y) = get_cell_origin(mode_index);
			self.draw_cell_background(graphic, cell_x, cell_y, TRANSLUCENT_COLUMN_ALPHA_LIST);
			graphic.set_blend_mode(blend_mode);
			self.draw_shape_page(graphic, cell_x, cell_y);
		}
		graphic.set_blend_mode(BlendMode::Normal);
		graphic.flush_batch();
		{
			let gl = graphic.get_gl();
			unsafe {
				gl.bind_framebuffer(glow::FRAMEBUFFER, None);
				gl.viewport(0, 0, drawing_buffer_width, drawing_buffer_height);
			}
		}

		// 바탕을 칠하고 렌더 타겟을 그대로 옮긴다. (렌더 타겟은 아래가 0 이라 위아래를 뒤집어 짚는다, 프리멀티플라이드라 source-over)
		let background_color = if is_white_background {
			Color::white()
		}
		else {
			Color::black()
		};
		graphic.set_fill_color(background_color);
		graphic.draw_rect(Rect::create(0.0, 0.0, VIEW_WIDTH, VIEW_HEIGHT));
		let vertex_count = graphic.write_quad(0, 0.0, 0.0, VIEW_WIDTH, VIEW_HEIGHT, 0.0, 1.0, 1.0, 0.0) / 4;
		graphic.draw_vertices(vertex_count, render_target_texture, Color::white());
	}
}

impl Scene for BlendModeScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
	}

	//==============================================================================
	// 출력. (칸마다 바탕을 깔고 그 칸의 블렌드 모드로 지금 장면을 그린다)
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		let page_index = (engine.get_frame_number() / PAGE_FRAME_COUNT) % PAGE_COUNT;
		if page_index >= 6 {
			self.draw_render_target_page(graphic, page_index == 7);
		}
		else {
			for (mode_index, blend_mode_name) in BLEND_MODE_NAME_LIST.iter().enumerate() {
				let blend_mode = match BlendMode::from_name(blend_mode_name) {
					Some(blend_mode) => {
						blend_mode
					}
					None => {
						continue;
					}
				};
				let (cell_x, cell_y) = get_cell_origin(mode_index);
				self.draw_cell_background(graphic, cell_x, cell_y, OPAQUE_COLUMN_ALPHA_LIST);
				graphic.set_blend_mode(blend_mode);
				match page_index {
					0 => {
						self.draw_shape_page(graphic, cell_x, cell_y);
					}
					1 => {
						self.draw_path_page(graphic, cell_x, cell_y);
					}
					2 | 3 => {
						self.draw_particle_page(graphic, cell_x, cell_y, page_index == 2);
					}
					_ => {
						self.draw_override_page(graphic, cell_x, cell_y, page_index == 4);
					}
				}
			}
		}

		// 남는 두 칸에 장면 이름.
		graphic.set_blend_mode(BlendMode::Normal);
		graphic.set_font_string("20px sans-serif");
		graphic.set_fill_color(Color::white());
		graphic.set_text_baseline(TextBaseline::Top);
		graphic.set_text_align(TextAlign::Left);
		graphic.draw_fill_text("블렌드 모드 26 가지", 816.0, 500.0);
		graphic.draw_fill_text(PAGE_NAME_LIST[page_index as usize], 816.0, 532.0);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("블렌드 모드");
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::ReferenceResolution;
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	engine.get_view_manager_mut().set_max_render_pixel_ratio(1.0);
	let blend_mode_scene = BlendModeScene {
		scene_base: SceneBase::new(),
		test_image: create_test_image(),
		nine_slice_image: create_nine_slice_image(),
		render_target: None,
	};
	if let Err(error) = engine.run(Box::new(blend_mode_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
