//==============================================================================
// 그래픽.
// - GL 컨텍스트로 출력하는 렌더러. 원본(WebGL2)과 같은 셰이더와 같은 상태 모델을 쓴다.
// - Canvas2D 시절의 좌표계(좌상 원점, 픽셀 단위)와 상태 모델(변환/알파 스택)을 그대로 재현한다.
// - 모든 드로우는 draw_vertices() 한 지점으로 수렴한다. 단색 도형도 1x1 흰색 텍스처로 텍스처 경로와 통일.
// - 클리핑은 스텐실 버퍼 기반. 텍스트는 문자열 텍스처로 구워 출력.
// - 묶어 그리기(기본 켬): 같은 텍스처를 잇달아 그리면 정점을 CPU 에서 변환해 모았다가 한 번에 그린다.
//   원본과 같은 묶음 셰이더로 정점마다 색과 틴트를 실어, 틴트가 걸린 그리기도 묶는다. 클립 영역 기록은 묶지 않는다.
//   블렌드, 클립, 파티클 그리기, 프레임 끝에서 모은 것을 먼저 내보낸다(flush_batch). 원본은 setSpriteBatchingEnabled 로 켜야 묶는다.
// - 화면 밖 거르기(기본 켬): NodeTree 가 노드를 그리기 전에 is_local_rect_visible 로 묻는다. 원본은 전부 그린다.
// - 셰이더 바꿔 끼우기(set_shader_program_override): ShaderSprite 같은 효과가 기본 프로그램 대신 제 프로그램으로 그린다. 바꿔 끼운 동안은 묶지 않는다.
//   바꿔 끼울 프로그램은 resolve_shader_program 이 이름으로 담아 두고, 어트리뷰트 위치를 기본 프로그램과 같게 고정해 기본 버텍스 어레이를 같이 쓴다.
// - 마지막으로 건 셰이더 프로그램과 텍스처 유닛 0 의 텍스처를 기억해, 같은 것을 다시 거는 GL 호출을 건너뛴다. (원본은 늘 건다)
//   GL 을 밖에서 만질 수 있는 때(get_gl 로 컨텍스트를 내줄 때, restore_render_state, 프레임 시작과 끝)에는 잊고 다음에 다시 건다.
// - 밉맵 단계 보정(set_image_mipmap_bias)은 기본, 묶음, 파티클 셰이더의 textureLodBias 유니폼으로 건다. 바꿔 끼운 셰이더는 그 유니폼이 있으면 받는다. (원본과 같음)
// - 원본에서 옮기지 않은 것: 오른쪽에서 왼쪽 글자 방향.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::image::Image;
use crate::core::imagetexturecache::ImageTextureCache;
use crate::core::shaderprogram::ShaderProgram;
use crate::core::textstringtexturecache::{TextDrawKind, TextEntry, TextMetrics, TextStringTextureCache};
use glow::HasContext;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

// GL 객체 타입. (데스크탑은 glow::Native*, 웹은 WebGL 객체. glow 컨텍스트가 정한 것을 그대로 쓴다)
pub type GlTexture = <glow::Context as glow::HasContext>::Texture;
pub type GlBuffer = <glow::Context as glow::HasContext>::Buffer;
pub type GlVertexArray = <glow::Context as glow::HasContext>::VertexArray;
pub type GlProgram = <glow::Context as glow::HasContext>::Program;
pub type GlShader = <glow::Context as glow::HasContext>::Shader;
pub type GlUniformLocation = <glow::Context as glow::HasContext>::UniformLocation;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 버텍스 셰이더. (픽셀 좌표 → 클립 좌표)
const VERTEXSHADER_SOURCE: &str = "#version 300 es
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

// 프래그먼트 셰이더. (프리멀티플라이드 알파 출력)
// - 단색 도형은 1x1 흰색 텍스처를 바인드해 같은 경로로 처리한다.
// - tint_color 는 이미지 실루엣(알파) 안쪽에만 색을 덮는 틴트. (alpha 0 이면 비활성)
const FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D mainTexture;
uniform vec4 mainColor;
uniform vec4 tintColor;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	vec3 tintedColor = mix(textureColor.rgb, tintColor.rgb * textureColor.a, tintColor.a);
	float finalAlpha = textureColor.a * mainColor.a * globalAlpha;
	vec3 finalColor = tintedColor * mainColor.rgb * mainColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

// 파티클 배치 버텍스 셰이더. (정점마다 색을 실어 한 번의 드로우로 수백 개를 그린다)
const PARTICLE_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
in vec4 vertexColor;
uniform mat3 projectionMatrix;
uniform mat3 modelMatrix;
out vec2 fragmentTextureCoordinate;
out vec4 fragmentColor;
void main() {
	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
	fragmentColor = vertexColor;
}
";

// 파티클 배치 프래그먼트 셰이더. (프리멀티플라이드 알파 출력)
const PARTICLE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
in vec4 fragmentColor;
uniform sampler2D mainTexture;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	float finalAlpha = textureColor.a * fragmentColor.a * globalAlpha;
	vec3 finalColor = textureColor.rgb * fragmentColor.rgb * fragmentColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

// 묶어 그리기 버텍스 셰이더. (정점은 쌓을 때 이미 디바이스 픽셀 좌표로 바꿔 둔다, 원본과 같음)
const BATCH_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
in vec4 vertexColor;
in vec4 vertexTintColor;
uniform mat3 projectionMatrix;
out vec2 fragmentTextureCoordinate;
out vec4 fragmentColor;
out vec4 fragmentTintColor;
void main() {
	vec3 transformedPosition = projectionMatrix * vec3(vertexPosition, 1.0);
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
	fragmentColor = vertexColor;
	fragmentTintColor = vertexTintColor;
}
";

// 묶어 그리기 프래그먼트 셰이더. (기본 셰이더와 같은 셈. 정점 색은 mainColor.rgb * mainColor.a * globalAlpha 와 mainColor.a * globalAlpha, 원본과 같음)
const BATCH_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
in vec4 fragmentColor;
in vec4 fragmentTintColor;
uniform sampler2D mainTexture;
uniform float textureLodBias;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	vec3 tintedColor = mix(textureColor.rgb, fragmentTintColor.rgb * textureColor.a, fragmentTintColor.a);
	outputColor = vec4(tintedColor * fragmentColor.rgb, textureColor.a * fragmentColor.a);
}
";

// 버텍스 1개당 float 수. (x, y, u, v)
pub const FLOATS_PER_VERTEX: usize = 4;
// 파티클 버텍스 1개당 float 수. (x, y, u, v, r, g, b, a)
pub const PARTICLE_FLOATS_PER_VERTEX: usize = 8;
const PARTICLE_VERTEX_CAPACITY: usize = 8192;
// 묶어 그리기 버텍스 1개당 float 수. (x, y, u, v, r, g, b, a, 틴트 r, g, b, a)
const BATCH_FLOATS_PER_VERTEX: usize = 12;

// 스크래치 버텍스 버퍼 용량. (버텍스 수)
const VERTEX_CAPACITY: usize = 4096;

// 묶어 그리기 버퍼 용량. (버텍스 수, 넘으면 내보내고 다시 모은다)
const BATCH_VERTEX_CAPACITY: usize = 6 * 8192;

// 라운드 사각형 / 원의 코너 아크 분할 수.
const CORNER_SEGMENT_COUNT: usize = 8;
const ROUND_RECT_POSITION_COUNT: usize = 4 * (CORNER_SEGMENT_COUNT + 1);

// 둥근 사각형 꼭짓점의 단위 원 좌표 표. (코너 4 개 x 분할 9 개, 우상 → 우하 → 좌하 → 좌상)
fn build_round_rect_unit_table() -> [(f32, f32); ROUND_RECT_POSITION_COUNT] {
	let start_radian_list: [f32; 4] = [-math::PI / 2.0, 0.0, math::PI / 2.0, math::PI];
	let mut unit_table = [(0.0, 0.0); ROUND_RECT_POSITION_COUNT];
	for (corner_index, start_radian) in start_radian_list.into_iter().enumerate() {
		for segment_index in 0..=CORNER_SEGMENT_COUNT {
			let radian = start_radian + (math::PI / 2.0) * (segment_index as f32 / CORNER_SEGMENT_COUNT as f32);
			unit_table[corner_index * (CORNER_SEGMENT_COUNT + 1) + segment_index] = (radian.cos(), radian.sin());
		}
	}
	unit_table
}

// 텍스트 베이크 스케일 상한. (과도한 확대 굽기 방지)
const MAXIMUM_TEXT_BAKE_SCALE: f32 = 8.0;
const DEFAULT_TEXT_BAKE_SCALE_STEP: f32 = 0.25;

// 부드러운 원 텍스처 크기.
const SOFT_DISC_TEXTURE_SIZE: usize = 64;

// 원본 이미지가 사라진 텍스처를 치우는 간격. (프레임)
const IMAGE_TEXTURE_SWEEP_INTERVAL: u64 = 600;

//==============================================================================
// 블렌드 모드. (원본 SpriteBlendMode, Canvas2D globalCompositeOperation 대응)
// - 고정 블렌딩으로 재현 가능한 9 종만 실제로 적용하고, 나머지는 normal(source-over)로 폴백한다. (원본과 같음)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
	Normal,
	SourceIn,
	SourceOut,
	SourceAtop,
	DestinationOver,
	DestinationIn,
	DestinationOut,
	DestinationAtop,
	Lighter,
	Copy,
	Xor,
	Multiply,
	Screen,
	Overlay,
	Darken,
	Lighten,
	ColorDodge,
	ColorBurn,
	HardLight,
	SoftLight,
	Difference,
	Exclusion,
	Hue,
	Saturation,
	Color,
	Luminosity,
}

// 블렌드 모드와 원본 이름(globalCompositeOperation 문자열)의 짝. (선언 차례)
const BLEND_MODE_NAME_TABLE: [(BlendMode, &str); 26] = [
	(BlendMode::Normal, "source-over"),
	(BlendMode::SourceIn, "source-in"),
	(BlendMode::SourceOut, "source-out"),
	(BlendMode::SourceAtop, "source-atop"),
	(BlendMode::DestinationOver, "destination-over"),
	(BlendMode::DestinationIn, "destination-in"),
	(BlendMode::DestinationOut, "destination-out"),
	(BlendMode::DestinationAtop, "destination-atop"),
	(BlendMode::Lighter, "lighter"),
	(BlendMode::Copy, "copy"),
	(BlendMode::Xor, "xor"),
	(BlendMode::Multiply, "multiply"),
	(BlendMode::Screen, "screen"),
	(BlendMode::Overlay, "overlay"),
	(BlendMode::Darken, "darken"),
	(BlendMode::Lighten, "lighten"),
	(BlendMode::ColorDodge, "color-dodge"),
	(BlendMode::ColorBurn, "color-burn"),
	(BlendMode::HardLight, "hard-light"),
	(BlendMode::SoftLight, "soft-light"),
	(BlendMode::Difference, "difference"),
	(BlendMode::Exclusion, "exclusion"),
	(BlendMode::Hue, "hue"),
	(BlendMode::Saturation, "saturation"),
	(BlendMode::Color, "color"),
	(BlendMode::Luminosity, "luminosity"),
];

impl BlendMode {
	//==============================================================================
	// 원본 이름 반환. ("source-over" 같은 문자열)
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		BLEND_MODE_NAME_TABLE[*self as usize].1
	}

	//==============================================================================
	// 원본 이름으로 찾기. (모르는 이름이면 None. 타임라인, 파티클 서술처럼 JSON 에서 읽을 때 쓴다)
	//==============================================================================
	pub fn from_name(name: &str) -> Option<BlendMode> {
		BLEND_MODE_NAME_TABLE.iter().find(|(_, blend_mode_name)| *blend_mode_name == name).map(|(blend_mode, _)| *blend_mode)
	}
}

//==============================================================================
// 가로 정렬.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
	Left,
	Center,
	Right,
	Start,
	End,
}

//==============================================================================
// 세로 기준선.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBaseline {
	Top,
	Middle,
	Bottom,
	Ideographic,
	Hanging,
	Alphabetic,
}

//==============================================================================
// push_state 로 저장하는 상태. (Canvas2D save 대응)
//==============================================================================
#[derive(Clone)]
struct GraphicState {
	transform_matrix: TransformMatrix,
	global_alpha: f32,
	blend_mode: BlendMode,
	fill_color: Color,
	stroke_color: Color,
	image_tint_color: Option<Color>,
	font_string: String,
	text_align: TextAlign,
	text_baseline: TextBaseline,
}

//==============================================================================
// 클립 스택 항목.
//==============================================================================
struct ClipEntry {
	rect: Rect,
	round_size: f32,
	transform_matrix: TransformMatrix,
}

pub struct Graphic {
	gl: glow::Context,
	is_force_gizmos_visible: bool,
	is_image_smoothing_enabled: bool,
	is_image_mipmap_enabled: bool,
	image_mipmap_bias: f32,
	shader_program: ShaderProgram,
	// 드로우에 잠시 바꿔 끼우는 프로그램. (ShaderSprite 등)
	shader_program_override: Option<Rc<ShaderProgram>>,
	// 이름으로 담아 둔 바꿔 끼울 프로그램. ((묶음 이름, 프로그램 이름) → 프로그램, 만들기에 실패했으면 None)
	shader_program_cache_table: HashMap<(&'static str, &'static str), Option<Rc<ShaderProgram>>>,
	// 아래 유니폼 자리를 찾아 둔 셰이더 프로그램.
	uniform_location_program: Option<GlProgram>,
	model_matrix_location: Option<GlUniformLocation>,
	main_color_location: Option<GlUniformLocation>,
	tint_color_location: Option<GlUniformLocation>,
	global_alpha_location: Option<GlUniformLocation>,
	// 기본 프로그램에 마지막으로 올린 틴트와 전역 투명도를 알고 있는지.
	is_uploaded_uniform_known: bool,
	// 기본 프로그램에 마지막으로 올린 틴트. (r, g, b, a)
	uploaded_tint_color_value_list: [f32; 4],
	// 기본 프로그램에 마지막으로 올린 전역 투명도.
	uploaded_global_alpha: f32,
	vertex_array: GlVertexArray,
	vertex_buffer: GlBuffer,
	vertex_data: Vec<f32>,
	white_texture: GlTexture,
	particle_shader_program: Option<ShaderProgram>,
	particle_vertex_array: Option<GlVertexArray>,
	particle_vertex_buffer: Option<GlBuffer>,
	particle_vertex_data: Vec<f32>,
	soft_disc_texture: Option<GlTexture>,
	batch_shader_program: Option<ShaderProgram>,
	batch_vertex_array: Option<GlVertexArray>,
	batch_vertex_buffer: Option<GlBuffer>,
	image_texture_cache: ImageTextureCache,
	text_string_texture_cache: TextStringTextureCache,
	transform_matrix: TransformMatrix,
	// 상태 스택. (앞의 state_stack_depth 개가 쓰는 칸, 뒤 칸은 폰트 문자열 저장소를 다시 쓰려고 남겨 둔다)
	state_stack: Vec<GraphicState>,
	state_stack_depth: usize,
	global_alpha: f32,
	blend_mode: BlendMode,
	warned_blend_modes: HashSet<BlendMode>,
	fill_color: Color,
	stroke_color: Color,
	white_color: Color,
	image_tint_color: Option<Color>,
	font_string: String,
	text_align: TextAlign,
	text_baseline: TextBaseline,
	clip_stack: Vec<ClipEntry>,
	projection_matrix_array: [f32; 9],
	model_matrix_array: [f32; 9],
	applied_viewport_width: i32,
	applied_viewport_height: i32,
	text_bake_scale_step: f32,
	// 배율이 움직이는 노드의 텍스트 베이크 스케일 양자화 단위. (text_bake_scale_step 이 0 일 때만 쓴다)
	text_bake_motion_scale_step: f32,
	// 지금 그리는 노드의 배율이 지난 프레임과 달라 움직이는 중인지.
	is_text_bake_scale_moving: bool,
	is_text_pixel_snap_enabled: bool,
	draw_call_count: u32,
	draw_vertex_count: u32,
	last_frame_draw_call_count: u32,
	last_frame_draw_vertex_count: u32,
	culled_node_count: u32,
	last_frame_culled_node_count: u32,
	frame_number: u64,
	round_rect_unit_table: [(f32, f32); ROUND_RECT_POSITION_COUNT],
	is_batching_enabled: bool,
	batch_suspend_depth: u32,
	batch_vertex_data: Vec<f32>,
	batch_vertex_count: usize,
	batch_texture: Option<GlTexture>,
	is_culling_enabled: bool,
	// 마지막으로 사용 중으로 건 셰이더 프로그램. (모르면 None, 같으면 use_program 을 건너뛴다)
	bound_program: Cell<Option<GlProgram>>,
	// 텍스처 유닛 0 에 마지막으로 바인드한 텍스처. (모르면 None, 같으면 bind_texture 를 건너뛴다)
	bound_texture: Cell<Option<GlTexture>>,
}

impl Graphic {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(gl: glow::Context) -> Result<Graphic, String> {
		// 셰이더 프로그램.
		let shader_program = ShaderProgram::new(&gl, VERTEXSHADER_SOURCE, FRAGMENTSHADER_SOURCE)?;

		// 버텍스 어레이 / 버텍스 버퍼. (스트리밍, 인터리브 x/y/u/v)
		let vertex_data = vec![0.0f32; VERTEX_CAPACITY * FLOATS_PER_VERTEX];
		let (vertex_array, vertex_buffer, white_texture) = unsafe {
			let vertex_array = gl.create_vertex_array()?;
			let vertex_buffer = gl.create_buffer()?;
			gl.bind_vertex_array(Some(vertex_array));
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
			gl.buffer_data_size(glow::ARRAY_BUFFER, (vertex_data.len() * 4) as i32, glow::DYNAMIC_DRAW);
			let stride_bytes = (FLOATS_PER_VERTEX * 4) as i32;
			let vertex_position_location = shader_program.get_attribute_location(&gl, "vertexPosition");
			let vertex_texture_coordinate_location = shader_program.get_attribute_location(&gl, "vertexTextureCoordinate");
			if let Some(vertex_position_location) = vertex_position_location {
				gl.enable_vertex_attrib_array(vertex_position_location);
				gl.vertex_attrib_pointer_f32(vertex_position_location, 2, glow::FLOAT, false, stride_bytes, 0);
			}
			if let Some(vertex_texture_coordinate_location) = vertex_texture_coordinate_location {
				gl.enable_vertex_attrib_array(vertex_texture_coordinate_location);
				gl.vertex_attrib_pointer_f32(vertex_texture_coordinate_location, 2, glow::FLOAT, false, stride_bytes, 8);
			}
			gl.bind_vertex_array(None);

			// 단색 도형용 1x1 흰색 텍스처.
			let white_texture = gl.create_texture()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(white_texture));
			let white_pixel: [u8; 4] = [255, 255, 255, 255];
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&white_pixel)));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);

			// 고정 렌더 상태.
			gl.enable(glow::BLEND);
			gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::CULL_FACE);

			(vertex_array, vertex_buffer, white_texture)
		};

		// 텍스처 유닛 고정.
		shader_program.use_program(&gl);
		let main_texture_location = shader_program.get_uniform_location(&gl, "mainTexture");
		let texture_lod_bias_location = shader_program.get_uniform_location(&gl, "textureLodBias");
		unsafe {
			gl.uniform_1_i32(main_texture_location.as_ref(), 0);
			gl.uniform_1_f32(texture_lod_bias_location.as_ref(), 0.0);
			gl.active_texture(glow::TEXTURE0);
		}

		Ok(Graphic {
			gl,
			is_force_gizmos_visible: false,
			is_image_smoothing_enabled: true,
			is_image_mipmap_enabled: false,
			image_mipmap_bias: 0.0,
			shader_program,
			shader_program_override: None,
			shader_program_cache_table: HashMap::new(),
			uniform_location_program: None,
			model_matrix_location: None,
			main_color_location: None,
			tint_color_location: None,
			global_alpha_location: None,
			is_uploaded_uniform_known: false,
			uploaded_tint_color_value_list: [0.0; 4],
			uploaded_global_alpha: 1.0,
			vertex_array,
			vertex_buffer,
			vertex_data,
			white_texture,
			particle_shader_program: None,
			particle_vertex_array: None,
			particle_vertex_buffer: None,
			particle_vertex_data: Vec::new(),
			soft_disc_texture: None,
			batch_shader_program: None,
			batch_vertex_array: None,
			batch_vertex_buffer: None,
			image_texture_cache: ImageTextureCache::new(),
			text_string_texture_cache: TextStringTextureCache::new(),
			transform_matrix: TransformMatrix::identity(),
			state_stack: Vec::new(),
			state_stack_depth: 0,
			global_alpha: 1.0,
			blend_mode: BlendMode::Normal,
			warned_blend_modes: HashSet::new(),
			fill_color: Color::black(),
			stroke_color: Color::black(),
			white_color: Color::white(),
			image_tint_color: None,
			font_string: String::from("10px sans-serif"),
			text_align: TextAlign::Start,
			text_baseline: TextBaseline::Alphabetic,
			clip_stack: Vec::new(),
			projection_matrix_array: [0.0; 9],
			model_matrix_array: [0.0; 9],
			applied_viewport_width: 0,
			applied_viewport_height: 0,
			text_bake_scale_step: DEFAULT_TEXT_BAKE_SCALE_STEP,
			text_bake_motion_scale_step: DEFAULT_TEXT_BAKE_SCALE_STEP,
			is_text_bake_scale_moving: false,
			is_text_pixel_snap_enabled: false,
			draw_call_count: 0,
			draw_vertex_count: 0,
			last_frame_draw_call_count: 0,
			last_frame_draw_vertex_count: 0,
			culled_node_count: 0,
			last_frame_culled_node_count: 0,
			frame_number: 0,
			round_rect_unit_table: build_round_rect_unit_table(),
			is_batching_enabled: true,
			batch_suspend_depth: 0,
			batch_vertex_data: vec![0.0f32; BATCH_VERTEX_CAPACITY * BATCH_FLOATS_PER_VERTEX],
			batch_vertex_count: 0,
			batch_texture: None,
			is_culling_enabled: true,
			bound_program: Cell::new(None),
			bound_texture: Cell::new(None),
		})
	}

	//==============================================================================
	// 설정 반영. (매 프레임 시작 시 엔진이 호출)
	// - 드로잉 버퍼 크기 변화 감지 및 뷰포트/프로젝션 갱신.
	// - 프레임 시작 기준 상태로 리셋.
	// - 브라우저는 preserveDrawingBuffer 가 꺼져 있으면 프레임마다 화면을 비워 준다. 창에서는 직접 비운다.
	//==============================================================================
	pub fn apply_settings(&mut self, drawing_buffer_width: i32, drawing_buffer_height: i32) {
		self.frame_number += 1;
		self.text_string_texture_cache.begin_frame();
		self.shader_program_override = None;

		// 뷰포트/프로젝션 갱신.
		if drawing_buffer_width != self.applied_viewport_width || drawing_buffer_height != self.applied_viewport_height {
			unsafe {
				self.gl.viewport(0, 0, drawing_buffer_width, drawing_buffer_height);
			}
			let safe_width = drawing_buffer_width.max(1) as f32;
			let safe_height = drawing_buffer_height.max(1) as f32;
			self.projection_matrix_array = [2.0 / safe_width, 0.0, 0.0, 0.0, -2.0 / safe_height, 0.0, -1.0, 1.0, 1.0];
			self.applied_viewport_width = drawing_buffer_width;
			self.applied_viewport_height = drawing_buffer_height;
		}

		// 프로그램/버퍼 바인딩 및 프로젝션 업로드. (프레임 사이에 GL 이 바뀌었을 수 있어 기억한 것을 잊고 다시 건다)
		self.forget_bound_render_state();
		self.use_shader_program(&self.shader_program);
		let projection_matrix_location = self.shader_program.get_uniform_location(&self.gl, "projectionMatrix");
		unsafe {
			self.gl.bind_vertex_array(Some(self.vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
		}

		// 프레임 시작 기준 상태로 리셋.
		self.state_stack_depth = 0;
		self.is_uploaded_uniform_known = false;
		self.transform_matrix.set_identity();
		self.global_alpha = 1.0;
		self.image_tint_color = None;
		self.batch_vertex_count = 0;
		self.batch_texture = None;
		self.apply_blend_mode(BlendMode::Normal);

		// 클리핑 상태 리셋. (비정상 종료로 스택이 남아 있어도 복구)
		unsafe {
			if !self.clip_stack.is_empty() {
				self.clip_stack.clear();
				self.gl.disable(glow::STENCIL_TEST);
			}
			self.gl.color_mask(true, true, true, true);
			self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
			self.gl.clear_stencil(0);
			self.gl.clear(glow::COLOR_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
		}
	}

	//==============================================================================
	// 프레임 마무리. (엔진이 화면을 넘기기 전에 호출)
	//==============================================================================
	pub fn end_frame(&mut self) {
		self.flush_batch();
		if self.frame_number.is_multiple_of(IMAGE_TEXTURE_SWEEP_INTERVAL) {
			self.image_texture_cache.sweep(&self.gl);
		}
		self.forget_bound_render_state();
	}

	//==============================================================================
	// 셰이더 프로그램을 사용 중으로 건다. (마지막으로 건 것과 같으면 건너뛴다)
	//==============================================================================
	fn use_shader_program(&self, shader_program: &ShaderProgram) {
		let program = shader_program.get_program();
		if self.bound_program.get() == Some(program) {
			return;
		}
		shader_program.use_program(&self.gl);
		self.bound_program.set(Some(program));
	}

	//==============================================================================
	// 텍스처 유닛 0 에 텍스처를 바인드한다. (마지막으로 바인드한 것과 같으면 건너뛴다)
	//==============================================================================
	fn bind_main_texture(&self, texture: GlTexture) {
		if self.bound_texture.get() == Some(texture) {
			return;
		}
		unsafe {
			self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
		}
		self.bound_texture.set(Some(texture));
	}

	//==============================================================================
	// 기억한 프로그램과 텍스처를 잊는다. (GL 을 밖에서 만졌을 수 있을 때. 다음에 거는 것은 건너뛰지 않는다)
	//==============================================================================
	fn forget_bound_render_state(&self) {
		self.bound_program.set(None);
		self.bound_texture.set(None);
	}

	//==============================================================================
	// 묶음 기억 잊기. (원본 invalidateRenderStateCache, Graphic 을 거치지 않고 GL 로 프로그램이나 텍스처를 바꾼 뒤 2D 를 이어 그릴 때)
	// - get_gl, restore_render_state, 프레임 시작과 끝은 스스로 잊는다.
	//==============================================================================
	pub fn invalidate_render_state_cache(&self) {
		self.forget_bound_render_state();
	}

	//==============================================================================
	// 깊이 버퍼 비우기. (원본 clearDepthBuffer, 2D 그리기 사이에 3D 를 끼우기 전에)
	// - 자르기(begin_clip_rect, 스텐실)를 건 뒤 그리는 도중에 깊이를 비우면 갤럭시 폴드3(Adreno 660)의 드라이버가 3D 를 하나도 그리지 못한 일이 있다. (원본 게임)
	//   자르기 안에 3D 를 그릴 노드는 begin_clip_rect 앞에서 이것을 부르고, run_external_pass 에는 is_depth_cleared 를 false 로 준다.
	//==============================================================================
	pub fn clear_depth_buffer(&mut self) {
		self.flush_batch();
		unsafe {
			self.gl.depth_mask(true);
			self.gl.clear(glow::DEPTH_BUFFER_BIT);
		}
	}

	//==============================================================================
	// 2D 그리기 사이에 GL 패스 끼우기. (원본 runExternalPass)
	// - 앞의 2D 묶음을 내보내고, 기억한 프로그램과 텍스처를 잊고, is_depth_cleared 면 깊이를 비운 뒤 pass 를 부르고, 끝나면 restore_render_state 를 부른다.
	// - pass 는 GL 컨텍스트를 받아 프로그램, 버텍스 어레이, 뷰포트, 섞기, 깊이, 면 가리기를 바꿔도 된다. (클립 중이면 스텐실 테스트는 restore_render_state 가 다시 켠다)
	// - 원본은 callback 이 던져도 되돌린다. 러스트 판은 pass 가 돌려준 값을 그대로 돌려준다. (패닉은 되돌리지 않는다)
	//==============================================================================
	pub fn run_external_pass<R>(&mut self, is_depth_cleared: bool, pass: impl FnOnce(&glow::Context) -> R) -> R {
		self.flush_batch();
		if is_depth_cleared {
			self.clear_depth_buffer();
		}
		self.forget_bound_render_state();
		let result = pass(&self.gl);
		self.restore_render_state();
		result
	}

	//==============================================================================
	// 이미지 스무딩 활성화 여부 설정.
	// - 도트(픽셀) 스프라이트 게임은 false 로 설정해 선명하게 렌더링.
	//==============================================================================
	pub fn set_image_smoothing_enabled(&mut self, value: bool) {
		self.is_image_smoothing_enabled = value;
	}

	//==============================================================================
	// 이미지 스무딩 활성화 여부 반환.
	//==============================================================================
	pub fn is_image_smoothing_enabled(&self) -> bool {
		self.is_image_smoothing_enabled
	}

	//==============================================================================
	// 이미지 밉맵 사용 여부 설정. (크게 줄여 그리는 이미지의 자글거림을 줄인다)
	//==============================================================================
	pub fn set_image_mipmap_enabled(&mut self, value: bool) {
		self.is_image_mipmap_enabled = value;
	}

	//==============================================================================
	// 이미지 밉맵 사용 여부 반환.
	//==============================================================================
	pub fn is_image_mipmap_enabled(&self) -> bool {
		self.is_image_mipmap_enabled
	}

	//==============================================================================
	// 이미지 밉맵 단계 보정 설정. (기본 0, 원본 setImageMipmapBias)
	// - 밉맵 단계를 고를 때 더하는 값. 음수면 한 단계 큰 그림 쪽을 골라 또렷해지고, 너무 작으면 다시 자글거린다. (-0.5 안팎)
	// - 기본 셰이더(낱장, 묶어 그리기, 파티클)에 적용한다. 바꿔 끼운 셰이더는 textureLodBias 유니폼이 있으면 받는다.
	//==============================================================================
	pub fn set_image_mipmap_bias(&mut self, value: f32) {
		self.flush_batch();
		self.image_mipmap_bias = value;
		let shader_program_list: [Option<&ShaderProgram>; 4] = [Some(&self.shader_program), self.batch_shader_program.as_ref(), self.particle_shader_program.as_ref(), self.shader_program_override.as_deref()];
		for shader_program in shader_program_list.into_iter().flatten() {
			self.use_shader_program(shader_program);
			self.apply_image_mipmap_bias(shader_program);
		}
		self.use_shader_program(self.get_shader_program());
	}

	//==============================================================================
	// 이미지 밉맵 단계 보정 반환.
	//==============================================================================
	pub fn get_image_mipmap_bias(&self) -> f32 {
		self.image_mipmap_bias
	}

	//==============================================================================
	// 사용 중인 셰이더 프로그램에 밉맵 단계 보정 적용. (유니폼이 없는 프로그램은 넘어간다)
	//==============================================================================
	fn apply_image_mipmap_bias(&self, shader_program: &ShaderProgram) {
		let texture_lod_bias_location = match shader_program.get_uniform_location(&self.gl, "textureLodBias") {
			Some(texture_lod_bias_location) => {
				texture_lod_bias_location
			}
			None => {
				return;
			}
		};
		unsafe {
			self.gl.uniform_1_f32(Some(&texture_lod_bias_location), self.image_mipmap_bias);
		}
	}

	//==============================================================================
	// 모든 노드의 기즈모를 강제로 보일지 설정.
	//==============================================================================
	pub fn set_force_gizmos_visible(&mut self, is_force_gizmos_visible: bool) {
		self.is_force_gizmos_visible = is_force_gizmos_visible;
	}

	//==============================================================================
	// 모든 노드의 기즈모를 강제로 보일지 반환.
	//==============================================================================
	pub fn is_force_gizmos_visible(&self) -> bool {
		self.is_force_gizmos_visible
	}

	//==============================================================================
	// 상태 저장. (Canvas2D save 대응)
	// - 한 번 쓴 칸은 남겨 두고 다시 쓴다. 폰트 문자열은 칸의 저장소에 옮겨 적어 매번 새로 잡지 않는다.
	//==============================================================================
	pub fn push_state(&mut self) {
		if self.state_stack_depth < self.state_stack.len() {
			let state_snapshot = &mut self.state_stack[self.state_stack_depth];
			state_snapshot.transform_matrix = self.transform_matrix;
			state_snapshot.global_alpha = self.global_alpha;
			state_snapshot.blend_mode = self.blend_mode;
			state_snapshot.fill_color = self.fill_color;
			state_snapshot.stroke_color = self.stroke_color;
			state_snapshot.image_tint_color = self.image_tint_color;
			state_snapshot.font_string.clear();
			state_snapshot.font_string.push_str(&self.font_string);
			state_snapshot.text_align = self.text_align;
			state_snapshot.text_baseline = self.text_baseline;
		}
		else {
			self.state_stack.push(GraphicState {
				transform_matrix: self.transform_matrix,
				global_alpha: self.global_alpha,
				blend_mode: self.blend_mode,
				fill_color: self.fill_color,
				stroke_color: self.stroke_color,
				image_tint_color: self.image_tint_color,
				font_string: self.font_string.clone(),
				text_align: self.text_align,
				text_baseline: self.text_baseline,
			});
		}
		self.state_stack_depth += 1;
	}

	//==============================================================================
	// 상태 복원. (Canvas2D restore 대응)
	// - 폰트 문자열은 칸의 저장소와 맞바꾼다.
	//==============================================================================
	pub fn pop_state(&mut self) {
		if self.state_stack_depth == 0 {
			return;
		}
		self.state_stack_depth -= 1;
		let state_snapshot = &mut self.state_stack[self.state_stack_depth];
		self.transform_matrix = state_snapshot.transform_matrix;
		self.global_alpha = state_snapshot.global_alpha;
		self.fill_color = state_snapshot.fill_color;
		self.stroke_color = state_snapshot.stroke_color;
		self.image_tint_color = state_snapshot.image_tint_color;
		std::mem::swap(&mut self.font_string, &mut state_snapshot.font_string);
		self.text_align = state_snapshot.text_align;
		self.text_baseline = state_snapshot.text_baseline;
		let blend_mode = state_snapshot.blend_mode;
		if self.blend_mode != blend_mode {
			self.set_blend_mode(blend_mode);
		}
	}

	//==============================================================================
	// 이동 변환 누적.
	//==============================================================================
	pub fn translate(&mut self, x: f32, y: f32) {
		self.transform_matrix.translate(x, y);
	}

	//==============================================================================
	// 회전 변환 누적.
	//==============================================================================
	pub fn rotate(&mut self, radian: f32) {
		self.transform_matrix.rotate(radian);
	}

	//==============================================================================
	// 크기 변환 누적.
	//==============================================================================
	pub fn scale(&mut self, x: f32, y: f32) {
		self.transform_matrix.scale(x, y);
	}

	//==============================================================================
	// 변환 행렬 직접 설정. (Canvas2D set_transform(a, b, c, d, e, f) 대응)
	//==============================================================================
	pub fn set_transform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
		self.transform_matrix.set_transform(a, b, c, d, e, f);
	}

	//==============================================================================
	// 변환 행렬 초기화.
	//==============================================================================
	pub fn reset_transform(&mut self) {
		self.transform_matrix.set_identity();
	}

	//==============================================================================
	// 현재 변환 행렬 반환.
	//==============================================================================
	pub fn get_transform_matrix(&self) -> TransformMatrix {
		self.transform_matrix
	}

	//==============================================================================
	// 전역 투명도 설정.
	//==============================================================================
	pub fn set_global_alpha(&mut self, value: f32) {
		self.global_alpha = math::clamp01(value);
	}

	//==============================================================================
	// 전역 투명도 반환.
	//==============================================================================
	pub fn get_global_alpha(&self) -> f32 {
		self.global_alpha
	}

	//==============================================================================
	// 전역 투명도 곱셈 누적. (노드 트리의 투명도 상속용)
	//==============================================================================
	pub fn multiply_global_alpha(&mut self, value: f32) {
		self.global_alpha = math::clamp01(self.global_alpha * value);
	}

	//==============================================================================
	// 블렌드 모드 설정.
	// - 고정 블렌딩으로 재현 불가능한 모드는 normal(source-over) 로 폴백.
	//==============================================================================
	pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
		if blend_mode == self.blend_mode {
			return;
		}
		self.flush_batch();
		self.apply_blend_mode(blend_mode);
	}

	//==============================================================================
	// 블렌드 모드를 GL 에 건다. (같은 모드여도 건다)
	//==============================================================================
	fn apply_blend_mode(&mut self, blend_mode: BlendMode) {
		let blend_function = match blend_mode {
			BlendMode::Normal => {
				Some((glow::ONE, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::Lighter => {
				Some((glow::ONE, glow::ONE))
			}
			BlendMode::Multiply => {
				Some((glow::DST_COLOR, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::Screen => {
				Some((glow::ONE, glow::ONE_MINUS_SRC_COLOR))
			}
			BlendMode::DestinationOver => {
				Some((glow::ONE_MINUS_DST_ALPHA, glow::ONE))
			}
			BlendMode::DestinationOut => {
				Some((glow::ZERO, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::SourceIn => {
				Some((glow::DST_ALPHA, glow::ZERO))
			}
			BlendMode::DestinationIn => {
				Some((glow::ZERO, glow::SRC_ALPHA))
			}
			BlendMode::Copy => {
				Some((glow::ONE, glow::ZERO))
			}
			_ => {
				None
			}
		};
		let (source_factor, destination_factor) = match blend_function {
			Some(blend_function) => {
				blend_function
			}
			None => {
				if !self.warned_blend_modes.contains(&blend_mode) {
					self.warned_blend_modes.insert(blend_mode);
					eprintln!("Graphic: unsupported blend mode \"{:?}\", falling back to \"normal\".", blend_mode);
				}
				(glow::ONE, glow::ONE_MINUS_SRC_ALPHA)
			}
		};
		self.blend_mode = blend_mode;
		unsafe {
			self.gl.blend_func(source_factor, destination_factor);
		}
	}

	//==============================================================================
	// 블렌드 모드 반환.
	//==============================================================================
	pub fn get_blend_mode(&self) -> BlendMode {
		self.blend_mode
	}

	//==============================================================================
	// 이미지 틴트 색상 설정. (None 이면 해제)
	// - 이미지 실루엣(알파) 안쪽에만 tint.alpha 비율로 색을 덮는다.
	//==============================================================================
	pub fn set_image_tint_color(&mut self, color: Option<Color>) {
		self.image_tint_color = color;
	}

	//==============================================================================
	// 이미지 틴트 색상 반환.
	//==============================================================================
	pub fn get_image_tint_color(&self) -> Option<Color> {
		self.image_tint_color
	}

	//==============================================================================
	// 채우기 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn set_fill_color(&mut self, color: impl Into<Color>) {
		self.fill_color = color.into();
	}

	//==============================================================================
	// 채우기 색상 반환.
	//==============================================================================
	pub fn get_fill_color(&self) -> Color {
		self.fill_color
	}

	//==============================================================================
	// 선 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn set_stroke_color(&mut self, color: impl Into<Color>) {
		self.stroke_color = color.into();
	}

	//==============================================================================
	// 선 색상 반환.
	//==============================================================================
	pub fn get_stroke_color(&self) -> Color {
		self.stroke_color
	}

	//==============================================================================
	// 버텍스 데이터에 쿼드(삼각형 2개, 버텍스 6개) 기록.
	// - 기록 후의 오프셋(플로트 단위)을 반환.
	//==============================================================================
	pub fn write_quad(&mut self, offset: usize, left_x: f32, top_y: f32, right_x: f32, bottom_y: f32, left_u: f32, top_v: f32, right_u: f32, bottom_v: f32) -> usize {
		let quad_values: [f32; 24] = [
			left_x, top_y, left_u, top_v,
			right_x, top_y, right_u, top_v,
			left_x, bottom_y, left_u, bottom_v,
			left_x, bottom_y, left_u, bottom_v,
			right_x, top_y, right_u, top_v,
			right_x, bottom_y, right_u, bottom_v,
		];
		self.vertex_data[offset..offset + 24].copy_from_slice(&quad_values);
		offset + 24
	}

	//==============================================================================
	// 기록된 버텍스 출력. (모든 드로우가 수렴하는 단일 지점)
	//==============================================================================
	pub fn draw_vertices(&mut self, vertex_count: usize, texture: GlTexture, color: Color) {
		if vertex_count == 0 {
			return;
		}

		// 묶어 그리기. (클립 영역 기록 중이 아니고 바꿔 끼운 셰이더가 없을 때. 바꿔 끼운 셰이더는 제 유니폼을 쓰므로 바로 그린다)
		let is_batchable = self.is_batching_enabled && self.batch_suspend_depth == 0 && self.shader_program_override.is_none();
		if is_batchable {
			self.append_batch_vertices(vertex_count, texture, color);
			return;
		}
		self.flush_batch();

		// 버텍스 업로드. (쓸 만큼만 새 저장소를 잡으며 올린다. 지난 드로우와 저장소가 겹치지 않는다)
		let float_count = vertex_count * FLOATS_PER_VERTEX;
		let vertex_bytes = float_slice_to_bytes(&self.vertex_data[..float_count]);
		unsafe {
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);
		}

		// 유니폼 업로드. (묶음을 내보낸 뒤에는 묶음 프로그램이 걸려 있을 수 있어 지금 쓰는 프로그램을 건다)
		self.use_shader_program(self.get_shader_program());
		self.upload_draw_uniforms(color);

		// 텍스처 바인드 및 출력.
		self.bind_main_texture(texture);
		unsafe {
			self.gl.draw_arrays(glow::TRIANGLES, 0, vertex_count as i32);
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += vertex_count as u32;
	}

	//==============================================================================
	// 드로우 콜 하나의 유니폼 업로드. (draw_vertices 가 부른다)
	// - 유니폼 자리는 셰이더 프로그램이 바뀔 때만 다시 찾는다.
	// - 기본 프로그램의 틴트와 전역 투명도는 지난번에 올린 값과 같으면 다시 올리지 않는다. (프레임 시작과 프로그램이 바뀔 때 잊는다)
	// - 바꿔 끼운 셰이더 프로그램에는 늘 올린다.
	//==============================================================================
	fn upload_draw_uniforms(&mut self, color: Color) {
		let is_default_program = self.shader_program_override.is_none();
		let shader_program = match &self.shader_program_override {
			Some(shader_program_override) => {
				shader_program_override.as_ref()
			}
			None => {
				&self.shader_program
			}
		};
		let program = shader_program.get_program();
		if self.uniform_location_program != Some(program) {
			let model_matrix_location = shader_program.get_uniform_location(&self.gl, "modelMatrix");
			let main_color_location = shader_program.get_uniform_location(&self.gl, "mainColor");
			let tint_color_location = shader_program.get_uniform_location(&self.gl, "tintColor");
			let global_alpha_location = shader_program.get_uniform_location(&self.gl, "globalAlpha");
			self.uniform_location_program = Some(program);
			self.model_matrix_location = model_matrix_location;
			self.main_color_location = main_color_location;
			self.tint_color_location = tint_color_location;
			self.global_alpha_location = global_alpha_location;
			self.is_uploaded_uniform_known = false;
		}

		self.transform_matrix.write_to_float32_array(&mut self.model_matrix_array);

		// 틴트. (없으면 0, 0, 0, 0)
		let tint_color_value_list = match self.image_tint_color {
			Some(image_tint_color) => {
				[image_tint_color.red, image_tint_color.green, image_tint_color.blue, image_tint_color.alpha]
			}
			None => {
				[0.0, 0.0, 0.0, 0.0]
			}
		};
		let global_alpha = self.global_alpha;

		let is_uploaded_uniform_known = is_default_program && self.is_uploaded_uniform_known;
		let is_same_tint_color = is_uploaded_uniform_known && self.uploaded_tint_color_value_list == tint_color_value_list;
		let is_same_global_alpha = is_uploaded_uniform_known && self.uploaded_global_alpha == global_alpha;
		unsafe {
			self.gl.uniform_matrix_3_f32_slice(self.model_matrix_location.as_ref(), false, &self.model_matrix_array);
			self.gl.uniform_4_f32(self.main_color_location.as_ref(), color.red, color.green, color.blue, color.alpha);
			if !is_same_tint_color {
				self.gl.uniform_4_f32(self.tint_color_location.as_ref(), tint_color_value_list[0], tint_color_value_list[1], tint_color_value_list[2], tint_color_value_list[3]);
			}
			if !is_same_global_alpha {
				self.gl.uniform_1_f32(self.global_alpha_location.as_ref(), global_alpha);
			}
		}

		if is_default_program {
			self.uploaded_tint_color_value_list = tint_color_value_list;
			self.uploaded_global_alpha = global_alpha;
			self.is_uploaded_uniform_known = true;
		}
	}

	//==============================================================================
	// 묶음에 정점 더하기. (현재 변환을 CPU 에서 곱하고, 기본 셰이더와 같은 색 셈을 정점 색 하나로 미리 곱해 틴트와 함께 정점마다 싣는다)
	//==============================================================================
	fn append_batch_vertices(&mut self, vertex_count: usize, texture: GlTexture, color: Color) {
		if self.batch_texture != Some(texture) || self.batch_vertex_count + vertex_count > BATCH_VERTEX_CAPACITY {
			self.flush_batch();
			self.batch_texture = Some(texture);
		}
		let matrix = self.transform_matrix;
		let color_alpha = color.alpha * self.global_alpha;
		let color_red = color.red * color_alpha;
		let color_green = color.green * color_alpha;
		let color_blue = color.blue * color_alpha;
		let tint_color_value_list = match self.image_tint_color {
			Some(image_tint_color) => {
				[image_tint_color.red, image_tint_color.green, image_tint_color.blue, image_tint_color.alpha]
			}
			None => {
				[0.0, 0.0, 0.0, 0.0]
			}
		};
		let mut write_offset = self.batch_vertex_count * BATCH_FLOATS_PER_VERTEX;
		for vertex_index in 0..vertex_count {
			let read_offset = vertex_index * FLOATS_PER_VERTEX;
			let position_x = self.vertex_data[read_offset];
			let position_y = self.vertex_data[read_offset + 1];
			let batch_vertex = &mut self.batch_vertex_data[write_offset..write_offset + BATCH_FLOATS_PER_VERTEX];
			batch_vertex[0] = matrix.a * position_x + matrix.c * position_y + matrix.e;
			batch_vertex[1] = matrix.b * position_x + matrix.d * position_y + matrix.f;
			batch_vertex[2] = self.vertex_data[read_offset + 2];
			batch_vertex[3] = self.vertex_data[read_offset + 3];
			batch_vertex[4] = color_red;
			batch_vertex[5] = color_green;
			batch_vertex[6] = color_blue;
			batch_vertex[7] = color_alpha;
			batch_vertex[8..12].copy_from_slice(&tint_color_value_list);
			write_offset += BATCH_FLOATS_PER_VERTEX;
		}
		self.batch_vertex_count += vertex_count;
	}

	//==============================================================================
	// 모아 둔 묶음 내보내기. (GL 상태를 바꾸기 전, 프레임 끝, GL 을 직접 쓰기 전에 부른다)
	//==============================================================================
	pub fn flush_batch(&mut self) {
		if self.batch_vertex_count == 0 {
			return;
		}
		let vertex_count = self.batch_vertex_count;
		self.batch_vertex_count = 0;
		let texture = match self.batch_texture.take() {
			Some(texture) => {
				texture
			}
			None => {
				return;
			}
		};
		if let Err(error) = self.ensure_batch_resources() {
			eprintln!("Graphic: batch resources failed: {}", error);
			return;
		}
		let batch_shader_program = match &self.batch_shader_program {
			Some(batch_shader_program) => {
				batch_shader_program
			}
			None => {
				return;
			}
		};
		self.use_shader_program(batch_shader_program);
		let projection_matrix_location = batch_shader_program.get_uniform_location(&self.gl, "projectionMatrix");
		let vertex_bytes = float_slice_to_bytes(&self.batch_vertex_data[..vertex_count * BATCH_FLOATS_PER_VERTEX]);
		unsafe {
			self.gl.bind_vertex_array(self.batch_vertex_array);
			self.gl.bind_buffer(glow::ARRAY_BUFFER, self.batch_vertex_buffer);
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
		}
		self.bind_main_texture(texture);
		unsafe {
			self.gl.draw_arrays(glow::TRIANGLES, 0, vertex_count as i32);

			// 기본 버텍스 어레이로 복귀. (프로그램은 다음에 낱장으로 그릴 때 건다)
			self.gl.bind_vertex_array(Some(self.vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += vertex_count as u32;
	}

	//==============================================================================
	// 묶어 그리기 켜고 끄기. (기본 켬. 끄면 원본처럼 그리기 하나에 드로우 콜 하나)
	//==============================================================================
	pub fn set_batching_enabled(&mut self, is_batching_enabled: bool) {
		if !is_batching_enabled {
			self.flush_batch();
		}
		self.is_batching_enabled = is_batching_enabled;
	}

	//==============================================================================
	// 묶어 그리기 여부.
	//==============================================================================
	pub fn is_batching_enabled(&self) -> bool {
		self.is_batching_enabled
	}

	//==============================================================================
	// 화면 밖 거르기 켜고 끄기. (기본 켬)
	//==============================================================================
	pub fn set_culling_enabled(&mut self, is_culling_enabled: bool) {
		self.is_culling_enabled = is_culling_enabled;
	}

	//==============================================================================
	// 화면 밖 거르기 여부.
	//==============================================================================
	pub fn is_culling_enabled(&self) -> bool {
		self.is_culling_enabled
	}

	//==============================================================================
	// 현재 변환 기준의 사각형이 화면(드로잉 버퍼)에 걸치는지. (네 꼭짓점을 변환한 경계 상자로 본다)
	//==============================================================================
	pub fn is_local_rect_visible(&self, rect: Rect) -> bool {
		let matrix = self.transform_matrix;
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = rect.position.x + rect.size.x;
		let bottom_y = rect.position.y + rect.size.y;
		let corner_list: [(f32, f32); 4] = [(left_x, top_y), (right_x, top_y), (left_x, bottom_y), (right_x, bottom_y)];
		let mut minimum_x = f32::MAX;
		let mut minimum_y = f32::MAX;
		let mut maximum_x = f32::MIN;
		let mut maximum_y = f32::MIN;
		for (corner_x, corner_y) in corner_list {
			let transformed_x = matrix.a * corner_x + matrix.c * corner_y + matrix.e;
			let transformed_y = matrix.b * corner_x + matrix.d * corner_y + matrix.f;
			minimum_x = minimum_x.min(transformed_x);
			minimum_y = minimum_y.min(transformed_y);
			maximum_x = maximum_x.max(transformed_x);
			maximum_y = maximum_y.max(transformed_y);
		}
		let viewport_width = self.applied_viewport_width as f32;
		let viewport_height = self.applied_viewport_height as f32;
		maximum_x >= 0.0 && maximum_y >= 0.0 && minimum_x <= viewport_width && minimum_y <= viewport_height
	}

	//==============================================================================
	// 이번 프레임에 거른 노드 수 더하기. (NodeTree 가 부른다)
	//==============================================================================
	pub fn add_culled_node_count(&mut self, culled_node_count: u32) {
		self.culled_node_count += culled_node_count;
	}

	//==============================================================================
	// 지난 프레임에 화면 밖이라 거른 노드 수.
	//==============================================================================
	pub fn get_last_frame_culled_node_count(&self) -> u32 {
		self.last_frame_culled_node_count
	}

	//==============================================================================
	// 프레임 그리기 통계 시작.
	// - 엔진이 매 프레임 씬을 그리기 전에 부른다. 지난 프레임의 드로우 콜 / 정점 수를 넘겨 두고 이번 프레임 값을 비운다.
	//==============================================================================
	pub fn begin_frame_statistics(&mut self) {
		self.flush_batch();
		self.last_frame_draw_call_count = self.draw_call_count;
		self.last_frame_draw_vertex_count = self.draw_vertex_count;
		self.last_frame_culled_node_count = self.culled_node_count;
		self.draw_call_count = 0;
		self.draw_vertex_count = 0;
		self.culled_node_count = 0;
	}

	//==============================================================================
	// 지난 프레임의 드로우 콜 수.
	//==============================================================================
	pub fn get_last_frame_draw_call_count(&self) -> u32 {
		self.last_frame_draw_call_count
	}

	//==============================================================================
	// 지난 프레임에 그린 정점 수.
	//==============================================================================
	pub fn get_last_frame_vertex_count(&self) -> u32 {
		self.last_frame_draw_vertex_count
	}

	//==============================================================================
	// 지난 프레임에 그린 삼각형 수.
	//==============================================================================
	pub fn get_last_frame_triangle_count(&self) -> u32 {
		self.last_frame_draw_vertex_count / 3
	}

	//==============================================================================
	// 묶어 그리기 자원 준비. (셰이더, 버텍스 어레이, 버텍스 버퍼. 처음 쓸 때 만든다)
	//==============================================================================
	fn ensure_batch_resources(&mut self) -> Result<(), String> {
		if self.batch_shader_program.is_some() {
			return Ok(());
		}
		let batch_shader_program = ShaderProgram::new(&self.gl, BATCH_VERTEXSHADER_SOURCE, BATCH_FRAGMENTSHADER_SOURCE)?;
		unsafe {
			let batch_vertex_array = self.gl.create_vertex_array()?;
			let batch_vertex_buffer = self.gl.create_buffer()?;
			self.gl.bind_vertex_array(Some(batch_vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(batch_vertex_buffer));
			let stride_bytes = (BATCH_FLOATS_PER_VERTEX * 4) as i32;
			let attribute_layout: [(&str, i32, i32); 4] = [("vertexPosition", 2, 0), ("vertexTextureCoordinate", 2, 8), ("vertexColor", 4, 16), ("vertexTintColor", 4, 32)];
			for (attribute_name, component_count, byte_offset) in attribute_layout {
				let attribute_location = batch_shader_program.get_attribute_location(&self.gl, attribute_name);
				if let Some(attribute_location) = attribute_location {
					self.gl.enable_vertex_attrib_array(attribute_location);
					self.gl.vertex_attrib_pointer_f32(attribute_location, component_count, glow::FLOAT, false, stride_bytes, byte_offset);
				}
			}
			self.gl.bind_vertex_array(Some(self.vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.batch_vertex_array = Some(batch_vertex_array);
			self.batch_vertex_buffer = Some(batch_vertex_buffer);
		}
		self.use_shader_program(&batch_shader_program);
		let main_texture_location = batch_shader_program.get_uniform_location(&self.gl, "mainTexture");
		unsafe {
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
		}
		self.apply_image_mipmap_bias(&batch_shader_program);
		self.batch_shader_program = Some(batch_shader_program);
		Ok(())
	}

	//==============================================================================
	// 파티클 배치 자원 준비. (처음 쓸 때 만든다)
	//==============================================================================
	fn ensure_particle_batch_resources(&mut self) -> Result<(), String> {
		if self.particle_shader_program.is_some() {
			return Ok(());
		}
		let particle_shader_program = ShaderProgram::new(&self.gl, PARTICLE_VERTEXSHADER_SOURCE, PARTICLE_FRAGMENTSHADER_SOURCE)?;
		unsafe {
			let particle_vertex_array = self.gl.create_vertex_array()?;
			let particle_vertex_buffer = self.gl.create_buffer()?;
			self.gl.bind_vertex_array(Some(particle_vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(particle_vertex_buffer));
			self.gl.buffer_data_size(glow::ARRAY_BUFFER, (PARTICLE_VERTEX_CAPACITY * PARTICLE_FLOATS_PER_VERTEX * 4) as i32, glow::DYNAMIC_DRAW);
			let stride_bytes = (PARTICLE_FLOATS_PER_VERTEX * 4) as i32;
			let attribute_layout: [(&str, i32, i32); 3] = [("vertexPosition", 2, 0), ("vertexTextureCoordinate", 2, 8), ("vertexColor", 4, 16)];
			for (attribute_name, component_count, byte_offset) in attribute_layout {
				let attribute_location = particle_shader_program.get_attribute_location(&self.gl, attribute_name);
				if let Some(attribute_location) = attribute_location {
					self.gl.enable_vertex_attrib_array(attribute_location);
					self.gl.vertex_attrib_pointer_f32(attribute_location, component_count, glow::FLOAT, false, stride_bytes, byte_offset);
				}
			}
			self.gl.bind_vertex_array(Some(self.vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.particle_vertex_array = Some(particle_vertex_array);
			self.particle_vertex_buffer = Some(particle_vertex_buffer);
		}
		self.use_shader_program(&particle_shader_program);
		let main_texture_location = particle_shader_program.get_uniform_location(&self.gl, "mainTexture");
		unsafe {
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
		}
		self.apply_image_mipmap_bias(&particle_shader_program);
		self.particle_shader_program = Some(particle_shader_program);
		Ok(())
	}

	//==============================================================================
	// 파티클 버텍스 기록용 배열 반환. (minimum_float_count 이상, 용량을 넘으면 용량까지)
	//==============================================================================
	pub fn get_particle_vertex_data(&mut self, minimum_float_count: usize) -> &mut [f32] {
		let capacity_float_count = PARTICLE_VERTEX_CAPACITY * PARTICLE_FLOATS_PER_VERTEX;
		let float_count = minimum_float_count.min(capacity_float_count);
		if self.particle_vertex_data.len() < float_count {
			self.particle_vertex_data.resize(float_count, 0.0);
		}
		&mut self.particle_vertex_data[..float_count]
	}

	//==============================================================================
	// 정점마다 색을 실은 쿼드 출력. (get_particle_vertex_data 에 기록한 것)
	//==============================================================================
	pub fn draw_colored_quads(&mut self, vertex_count: usize, texture: GlTexture) {
		if vertex_count == 0 {
			return;
		}
		self.flush_batch();
		if let Err(error) = self.ensure_particle_batch_resources() {
			eprintln!("Graphic: particle resources failed: {}", error);
			return;
		}
		let vertex_count = vertex_count.min(PARTICLE_VERTEX_CAPACITY).min(self.particle_vertex_data.len() / PARTICLE_FLOATS_PER_VERTEX);
		let particle_shader_program = match &self.particle_shader_program {
			Some(particle_shader_program) => {
				particle_shader_program
			}
			None => {
				return;
			}
		};
		self.transform_matrix.write_to_float32_array(&mut self.model_matrix_array);
		self.use_shader_program(particle_shader_program);
		let projection_matrix_location = particle_shader_program.get_uniform_location(&self.gl, "projectionMatrix");
		let model_matrix_location = particle_shader_program.get_uniform_location(&self.gl, "modelMatrix");
		let global_alpha_location = particle_shader_program.get_uniform_location(&self.gl, "globalAlpha");
		let vertex_bytes = float_slice_to_bytes(&self.particle_vertex_data[..vertex_count * PARTICLE_FLOATS_PER_VERTEX]);
		unsafe {
			self.gl.bind_vertex_array(self.particle_vertex_array);
			self.gl.bind_buffer(glow::ARRAY_BUFFER, self.particle_vertex_buffer);
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_matrix_3_f32_slice(model_matrix_location.as_ref(), false, &self.model_matrix_array);
			self.gl.uniform_1_f32(global_alpha_location.as_ref(), self.global_alpha);
		}
		self.bind_main_texture(texture);
		unsafe {
			self.gl.draw_arrays(glow::TRIANGLES, 0, vertex_count as i32);

			// 기본 버텍스 어레이로 복귀. (프로그램은 다음에 낱장으로 그릴 때 건다)
			self.gl.bind_vertex_array(Some(self.vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += vertex_count as u32;
	}

	//==============================================================================
	// 부드러운 원 텍스처 반환. (가운데가 불투명하고 가장자리로 갈수록 투명. 지연 생성)
	//==============================================================================
	pub fn get_soft_disc_texture(&mut self) -> GlTexture {
		if let Some(soft_disc_texture) = self.soft_disc_texture {
			return soft_disc_texture;
		}
		let mut pixels = vec![0u8; SOFT_DISC_TEXTURE_SIZE * SOFT_DISC_TEXTURE_SIZE * 4];
		let half_size = SOFT_DISC_TEXTURE_SIZE as f32 * 0.5;
		for pixel_y in 0..SOFT_DISC_TEXTURE_SIZE {
			for pixel_x in 0..SOFT_DISC_TEXTURE_SIZE {
				let delta_x = (pixel_x as f32 + 0.5 - half_size) / half_size;
				let delta_y = (pixel_y as f32 + 0.5 - half_size) / half_size;
				let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
				let falloff = math::clamp01(1.0 - distance);
				let alpha = (falloff * falloff * 255.0).round() as u8;
				let pixel_index = (pixel_y * SOFT_DISC_TEXTURE_SIZE + pixel_x) * 4;
				pixels[pixel_index] = alpha;
				pixels[pixel_index + 1] = alpha;
				pixels[pixel_index + 2] = alpha;
				pixels[pixel_index + 3] = alpha;
			}
		}
		let soft_disc_texture = unsafe {
			let texture = self.gl.create_texture().expect("soft disc texture");
			self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			self.gl.tex_image_2d(
				glow::TEXTURE_2D,
				0,
				glow::RGBA8 as i32,
				SOFT_DISC_TEXTURE_SIZE as i32,
				SOFT_DISC_TEXTURE_SIZE as i32,
				0,
				glow::RGBA,
				glow::UNSIGNED_BYTE,
				glow::PixelUnpackData::Slice(Some(&pixels)),
			);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			texture
		};
		self.bound_texture.set(Some(soft_disc_texture));
		self.soft_disc_texture = Some(soft_disc_texture);
		soft_disc_texture
	}

	//==============================================================================
	// 이미지의 텍스처 반환. (파티클처럼 직접 쿼드를 쓰는 곳에서)
	//==============================================================================
	pub fn get_image_texture(&mut self, image: &Rc<Image>) -> Option<GlTexture> {
		let is_image_smoothing_enabled = self.is_image_smoothing_enabled();
		let is_image_mipmap_enabled = self.is_image_mipmap_enabled();
		if let Some(texture) = self.image_texture_cache.find_texture(image, is_image_smoothing_enabled, is_image_mipmap_enabled) {
			return Some(texture);
		}

		// 새로 올리거나 필터를 다시 걸면 텍스처 바인드가 바뀐다.
		self.bound_texture.set(None);
		self.image_texture_cache.get_texture(&self.gl, image, is_image_smoothing_enabled, is_image_mipmap_enabled)
	}

	//==============================================================================
	// GPU 에 올리며 놓은 이미지의 CPU 픽셀을 텍스처에서 다시 읽어 되돌린다. (ShadowImage 굽기처럼 픽셀을 다시 읽어야 할 때)
	// - 픽셀이 남아 있으면 그대로 true. 올린 텍스처가 없거나 읽지 못하면 false.
	// - 텍스처를 프레임버퍼에 붙여 읽고, 그 전에 걸려 있던 프레임버퍼로 되돌린다. (올린 그대로의 프리멀티플라이드 RGBA 가 나온다)
	//==============================================================================
	pub(crate) fn restore_image_pixels(&mut self, image: &Rc<Image>) -> bool {
		if image.has_pixels() {
			return true;
		}
		let texture = match self.image_texture_cache.find_uploaded_texture(image) {
			Some(texture) => {
				texture
			}
			None => {
				return false;
			}
		};
		self.flush_batch();
		let width = image.get_width() as i32;
		let height = image.get_height() as i32;
		let mut pixels = vec![0u8; (width * height * 4) as usize];
		let is_read = unsafe {
			let previous_framebuffer = self.gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
			let framebuffer = match self.gl.create_framebuffer() {
				Ok(framebuffer) => {
					framebuffer
				}
				Err(_) => {
					return false;
				}
			};
			self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			self.gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
			let is_complete = self.gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
			if is_complete {
				self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
				self.gl.read_pixels(0, 0, width, height, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
			}
			self.gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
			self.gl.delete_framebuffer(framebuffer);
			is_complete
		};
		if !is_read {
			return false;
		}
		image.restore_pixels(pixels);
		true
	}

	//==============================================================================
	// 선 출력.
	// - set_stroke_color()
	//==============================================================================
	pub fn draw_line(&mut self, positions: &[Vector2], size: f32) {
		self.draw_polyline(positions, size, false);
	}

	//==============================================================================
	// 닫힌 선 출력. (마지막 점과 첫 점을 잇는 루프)
	// - set_stroke_color()
	//==============================================================================
	pub fn draw_line_loop(&mut self, positions: &[Vector2], size: f32) {
		self.draw_polyline(positions, size, true);
	}

	//==============================================================================
	// 폴리라인 출력. (세그먼트별 두께 확장 쿼드)
	// - 내부 조인트는 반두께 연장으로 이음새를 메운다.
	//==============================================================================
	pub fn draw_polyline(&mut self, positions: &[Vector2], size: f32, is_closed: bool) {
		if positions.len() < 2 {
			return;
		}

		let half_size = size / 2.0;
		let segment_count = if is_closed {
			positions.len()
		}
		else {
			positions.len() - 1
		};
		let mut offset = 0;
		let mut vertex_count = 0;
		for segment_index in 0..segment_count {
			let start_position = positions[segment_index];
			let end_position = positions[(segment_index + 1) % positions.len()];
			let delta_x = end_position.x - start_position.x;
			let delta_y = end_position.y - start_position.y;
			let length = (delta_x * delta_x + delta_y * delta_y).sqrt();
			if length <= 0.0 {
				continue;
			}

			let direction_x = delta_x / length;
			let direction_y = delta_y / length;
			let normal_x = -direction_y * half_size;
			let normal_y = direction_x * half_size;

			// 내부 조인트 이음새 메움. (열린 선의 양 끝은 연장하지 않음)
			let has_start_joint = is_closed || segment_index > 0;
			let has_end_joint = is_closed || segment_index < segment_count - 1;
			let start_extension = if has_start_joint {
				half_size
			}
			else {
				0.0
			};
			let end_extension = if has_end_joint {
				half_size
			}
			else {
				0.0
			};
			let extended_start_x = start_position.x - direction_x * start_extension;
			let extended_start_y = start_position.y - direction_y * start_extension;
			let extended_end_x = end_position.x + direction_x * end_extension;
			let extended_end_y = end_position.y + direction_y * end_extension;

			// 쿼드(삼각형 2개) 기록.
			if (vertex_count + 6) * FLOATS_PER_VERTEX > self.vertex_data.len() {
				break;
			}
			let segment_values: [f32; 24] = [
				extended_start_x + normal_x, extended_start_y + normal_y, 0.5, 0.5,
				extended_end_x + normal_x, extended_end_y + normal_y, 0.5, 0.5,
				extended_start_x - normal_x, extended_start_y - normal_y, 0.5, 0.5,
				extended_start_x - normal_x, extended_start_y - normal_y, 0.5, 0.5,
				extended_end_x + normal_x, extended_end_y + normal_y, 0.5, 0.5,
				extended_end_x - normal_x, extended_end_y - normal_y, 0.5, 0.5,
			];
			self.vertex_data[offset..offset + 24].copy_from_slice(&segment_values);
			offset += 24;
			vertex_count += 6;
		}

		let stroke_color = self.get_stroke_color();
		let white_texture = self.get_white_texture();
		self.draw_vertices(vertex_count, white_texture, stroke_color);
	}

	//==============================================================================
	// 사각형 출력.
	// - set_fill_color()
	//==============================================================================
	pub fn draw_rect(&mut self, rect: Rect) {
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = left_x + rect.size.x;
		let bottom_y = top_y + rect.size.y;
		let vertex_count_offset = self.write_quad(0, left_x, top_y, right_x, bottom_y, 0.5, 0.5, 0.5, 0.5);
		let fill_color = self.get_fill_color();
		let white_texture = self.get_white_texture();
		self.draw_vertices(vertex_count_offset / FLOATS_PER_VERTEX, white_texture, fill_color);
	}

	//==============================================================================
	// 사각형 외곽선 출력. (Canvas2D strokeRect 대응, 선 중심이 경계선 위)
	// - set_stroke_color()
	//==============================================================================
	pub fn draw_stroke_rect(&mut self, rect: Rect, line_width: f32) {
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = left_x + rect.size.x;
		let bottom_y = top_y + rect.size.y;
		let positions = [
			Vector2::create(left_x, top_y),
			Vector2::create(right_x, top_y),
			Vector2::create(right_x, bottom_y),
			Vector2::create(left_x, bottom_y),
		];
		self.draw_line_loop(&positions, line_width);
	}

	//==============================================================================
	// 라운드 사각형 경로 좌표 목록 생성. (시계 방향, 볼록 다각형)
	//==============================================================================
	pub fn build_round_rect_positions(&self, rect: Rect, round_size: f32) -> [Vector2; ROUND_RECT_POSITION_COUNT] {
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = left_x + rect.size.x;
		let bottom_y = top_y + rect.size.y;
		let clamped_round_size = round_size.min(rect.size.x.abs().min(rect.size.y.abs()) / 2.0);

		// 코너 중심 4개. (우상 → 우하 → 좌하 → 좌상)
		let corner_centers: [(f32, f32); 4] = [
			(right_x - clamped_round_size, top_y + clamped_round_size),
			(right_x - clamped_round_size, bottom_y - clamped_round_size),
			(left_x + clamped_round_size, bottom_y - clamped_round_size),
			(left_x + clamped_round_size, top_y + clamped_round_size),
		];

		// 꼭짓점 각도는 늘 같으니 처음에 구해 둔 cos / sin 표를 쓴다. (힙 할당 없이 스택 배열로 돌려준다)
		let mut positions = [Vector2::create(0.0, 0.0); ROUND_RECT_POSITION_COUNT];
		for (corner_index, (corner_center_x, corner_center_y)) in corner_centers.into_iter().enumerate() {
			for segment_index in 0..=CORNER_SEGMENT_COUNT {
				let position_index = corner_index * (CORNER_SEGMENT_COUNT + 1) + segment_index;
				let (cosine, sine) = self.round_rect_unit_table[position_index];
				positions[position_index] = Vector2::create(corner_center_x + cosine * clamped_round_size, corner_center_y + sine * clamped_round_size);
			}
		}
		positions
	}

	//==============================================================================
	// 볼록 다각형 채움 출력. (첫 점 기준 팬 분할)
	//==============================================================================
	pub fn draw_convex_polygon(&mut self, positions: &[Vector2], color: Color) {
		if positions.len() < 3 {
			return;
		}

		let mut offset = 0;
		let mut vertex_count = 0;
		let first_position = positions[0];
		for position_index in 1..positions.len() - 1 {
			if (vertex_count + 3) * FLOATS_PER_VERTEX > self.vertex_data.len() {
				break;
			}
			let second_position = positions[position_index];
			let third_position = positions[position_index + 1];
			let triangle_values: [f32; 12] = [
				first_position.x, first_position.y, 0.5, 0.5,
				second_position.x, second_position.y, 0.5, 0.5,
				third_position.x, third_position.y, 0.5, 0.5,
			];
			self.vertex_data[offset..offset + 12].copy_from_slice(&triangle_values);
			offset += 12;
			vertex_count += 3;
		}

		let white_texture = self.get_white_texture();
		self.draw_vertices(vertex_count, white_texture, color);
	}

	//==============================================================================
	// 라운드 사각형 출력.
	// - set_fill_color()
	//==============================================================================
	pub fn draw_round_rect(&mut self, rect: Rect, round_size: f32) {
		if round_size > 0.0 {
			let positions = self.build_round_rect_positions(rect, round_size);
			let fill_color = self.get_fill_color();
			self.draw_convex_polygon(&positions, fill_color);
		}
		else {
			self.draw_rect(rect);
		}
	}

	//==============================================================================
	// 라운드 사각형 외곽선 출력.
	// - set_stroke_color()
	//==============================================================================
	pub fn draw_stroke_round_rect(&mut self, rect: Rect, round_size: f32, line_width: f32) {
		if round_size > 0.0 {
			let positions = self.build_round_rect_positions(rect, round_size);
			self.draw_line_loop(&positions, line_width);
		}
		else {
			self.draw_stroke_rect(rect, line_width);
		}
	}

	//==============================================================================
	// 원 둘레 좌표 목록. (화면상 크기에 비례한 분할 수, 16 ~ 64)
	//==============================================================================
	fn build_circle_positions(&self, center: Vector2, radius: f32) -> Vec<Vector2> {
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		let segment_count = math::clamp((radius * maximum_scale * 0.5).ceil(), 16.0, 64.0) as usize;
		let mut positions = Vec::with_capacity(segment_count);
		for segment_index in 0..segment_count {
			let radian = (math::PI * 2.0) * (segment_index as f32 / segment_count as f32);
			let position_x = center.x + radian.cos() * radius;
			let position_y = center.y + radian.sin() * radius;
			positions.push(Vector2::create(position_x, position_y));
		}
		positions
	}

	//==============================================================================
	// 원 출력.
	// - set_fill_color()
	//==============================================================================
	pub fn draw_circle(&mut self, center: Vector2, radius: f32) {
		if radius <= 0.0 {
			return;
		}
		let positions = self.build_circle_positions(center, radius);
		let fill_color = self.get_fill_color();
		self.draw_convex_polygon(&positions, fill_color);
	}

	//==============================================================================
	// 원 외곽선 출력.
	// - set_stroke_color()
	//==============================================================================
	pub fn draw_stroke_circle(&mut self, center: Vector2, radius: f32, line_width: f32) {
		if radius <= 0.0 {
			return;
		}
		let positions = self.build_circle_positions(center, radius);
		self.draw_line_loop(&positions, line_width);
	}

	//==============================================================================
	// 고리(띠) 출력. (안쪽 반지름과 바깥 반지름 사이를 start_radian 부터 end_radian 까지 칠한다)
	// - set_fill_color()
	// - 각도는 라디안, 0 이 오른쪽이고 화면에서 시계 방향으로 는다. (12 시에서 시작하려면 -PI / 2, 한 바퀴는 0 에서 PI * 2)
	// - 조각마다 네모 하나를 겹치지 않게 이어 붙여 한 번에 그리므로, 반투명 색이어도 조각의 이음매가 짙어지지 않는다.
	// - 조각 수는 draw_circle 처럼 화면에서의 바깥 반지름을 따라 정하고(한 바퀴 16 ~ 64), 그리는 각도만큼만 쓴다.
	//==============================================================================
	pub fn draw_ring(&mut self, center: Vector2, inner_radius: f32, outer_radius: f32, start_radian: f32, end_radian: f32) {
		let fill_color = self.get_fill_color();
		self.draw_ring_with_color(center, inner_radius, outer_radius, start_radian, end_radian, fill_color);
	}

	//==============================================================================
	// 호 출력. (반지름을 따라 line_width 두께의 선으로 start_radian 부터 end_radian 까지)
	// - set_stroke_color()
	// - 각도와 조각 수는 draw_ring 과 같다. 선의 두께 가운데가 반지름에 온다.
	//==============================================================================
	pub fn draw_arc(&mut self, center: Vector2, radius: f32, start_radian: f32, end_radian: f32, line_width: f32) {
		let half_width = line_width * 0.5;
		let stroke_color = self.get_stroke_color();
		self.draw_ring_with_color(center, (radius - half_width).max(0.0), radius + half_width, start_radian, end_radian, stroke_color);
	}

	//==============================================================================
	// 고리 출력의 셈. (draw_ring, draw_arc 가 부른다)
	//==============================================================================
	pub fn draw_ring_with_color(&mut self, center: Vector2, inner_radius: f32, outer_radius: f32, start_radian: f32, end_radian: f32, color: Color) {
		let span_radian = end_radian - start_radian;
		if outer_radius <= 0.0 || outer_radius <= inner_radius || span_radian == 0.0 {
			return;
		}
		let full_radian = math::PI * 2.0;
		let clamped_span_radian = math::clamp(span_radian, -full_radian, full_radian);
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		let full_segment_count = math::clamp((outer_radius * maximum_scale * 0.5).ceil(), 16.0, 64.0);
		let segment_count = ((full_segment_count * clamped_span_radian.abs() / full_radian).ceil() as usize).max(1);
		let mut offset = 0;
		let mut vertex_count = 0;
		let mut previous_cosine = start_radian.cos();
		let mut previous_sine = start_radian.sin();
		for segment_index in 1..=segment_count {
			if (vertex_count + 6) * FLOATS_PER_VERTEX > self.vertex_data.len() {
				break;
			}
			let radian = start_radian + clamped_span_radian * (segment_index as f32 / segment_count as f32);
			let cosine = radian.cos();
			let sine = radian.sin();
			let previous_inner_x = center.x + previous_cosine * inner_radius;
			let previous_inner_y = center.y + previous_sine * inner_radius;
			let previous_outer_x = center.x + previous_cosine * outer_radius;
			let previous_outer_y = center.y + previous_sine * outer_radius;
			let inner_x = center.x + cosine * inner_radius;
			let inner_y = center.y + sine * inner_radius;
			let outer_x = center.x + cosine * outer_radius;
			let outer_y = center.y + sine * outer_radius;
			let segment_values: [f32; 24] = [
				previous_inner_x, previous_inner_y, 0.5, 0.5,
				previous_outer_x, previous_outer_y, 0.5, 0.5,
				outer_x, outer_y, 0.5, 0.5,
				previous_inner_x, previous_inner_y, 0.5, 0.5,
				outer_x, outer_y, 0.5, 0.5,
				inner_x, inner_y, 0.5, 0.5,
			];
			self.vertex_data[offset..offset + 24].copy_from_slice(&segment_values);
			offset += 24;
			vertex_count += 6;
			previous_cosine = cosine;
			previous_sine = sine;
		}
		let white_texture = self.get_white_texture();
		self.draw_vertices(vertex_count, white_texture, color);
	}

	//==============================================================================
	// 선분 여럿 출력. (위치 배열 하나에 선분마다 x0, y0, x1, y1 넷씩, 한 번에 그린다)
	// - set_stroke_color()
	// - 선분마다 두께만큼 넓힌 네모 하나다. (draw_line 과 달리 이음새를 메우지 않는다, 따로 떨어진 선분 용)
	// - Vector2 배열을 만들지 않고 숫자 배열을 그대로 읽는다. 길이가 0 인 선분은 건너뛴다.
	// - 정점 버퍼가 차면 거기까지 그리고 이어서 담는다. (묶어 그리기가 켜져 있으면 같은 텍스처라 한 번의 드로우 콜로 이어진다)
	//==============================================================================
	pub fn draw_line_segments(&mut self, position_values: &[f32], size: f32) {
		if position_values.len() < 4 {
			return;
		}
		let half_size = size / 2.0;
		let stroke_color = self.get_stroke_color();
		let white_texture = self.get_white_texture();
		let mut offset = 0;
		let mut vertex_count = 0;
		for segment_values in position_values.chunks_exact(4) {
			let start_x = segment_values[0];
			let start_y = segment_values[1];
			let end_x = segment_values[2];
			let end_y = segment_values[3];
			let delta_x = end_x - start_x;
			let delta_y = end_y - start_y;
			let length = (delta_x * delta_x + delta_y * delta_y).sqrt();
			if !(length > 0.0) {
				continue;
			}
			if (vertex_count + 6) * FLOATS_PER_VERTEX > self.vertex_data.len() {
				self.draw_vertices(vertex_count, white_texture, stroke_color);
				offset = 0;
				vertex_count = 0;
			}
			let normal_x = -delta_y / length * half_size;
			let normal_y = delta_x / length * half_size;
			let quad_values: [f32; 24] = [
				start_x + normal_x, start_y + normal_y, 0.5, 0.5,
				end_x + normal_x, end_y + normal_y, 0.5, 0.5,
				start_x - normal_x, start_y - normal_y, 0.5, 0.5,
				start_x - normal_x, start_y - normal_y, 0.5, 0.5,
				end_x + normal_x, end_y + normal_y, 0.5, 0.5,
				end_x - normal_x, end_y - normal_y, 0.5, 0.5,
			];
			self.vertex_data[offset..offset + 24].copy_from_slice(&quad_values);
			offset += 24;
			vertex_count += 6;
		}
		self.draw_vertices(vertex_count, white_texture, stroke_color);
	}

	//==============================================================================
	// 이미지 출력.
	//==============================================================================
	pub fn draw_image(&mut self, image: &Rc<Image>, position: Vector2, content_size: Vector2) {
		let image_width = image.get_width() as f32;
		let image_height = image.get_height() as f32;
		self.draw_image_with_source_and_destination(image, 0.0, 0.0, image_width, image_height, position.x, position.y, content_size.x, content_size.y);
	}

	//==============================================================================
	// 이미지의 일부만 출력. (image_rect 가 비었으면 전체)
	//==============================================================================
	pub fn draw_image_with_image_rect(&mut self, image: &Rc<Image>, position: Vector2, content_size: Vector2, image_rect: Rect) {
		let source_rect = if image_rect.is_zero() {
			Rect::create(0.0, 0.0, image.get_width() as f32, image.get_height() as f32)
		}
		else {
			image_rect
		};
		self.draw_image_with_source_and_destination(image, source_rect.position.x, source_rect.position.y, source_rect.size.x, source_rect.size.y, position.x, position.y, content_size.x, content_size.y);
	}

	//==============================================================================
	// 이미지의 소스 영역을 지정 영역에 출력. (Canvas2D draw_image 9인자 대응)
	//==============================================================================
	pub fn draw_image_with_source_and_destination(&mut self, image: &Rc<Image>, source_x: f32, source_y: f32, source_width: f32, source_height: f32, destination_x: f32, destination_y: f32, destination_width: f32, destination_height: f32) {
		let texture = match self.get_image_texture(image) {
			Some(texture) => {
				texture
			}
			None => {
				return;
			}
		};

		let image_width = image.get_width() as f32;
		let image_height = image.get_height() as f32;
		let left_u = source_x / image_width;
		let top_v = source_y / image_height;
		let right_u = (source_x + source_width) / image_width;
		let bottom_v = (source_y + source_height) / image_height;
		let vertex_count_offset = self.write_quad(0, destination_x, destination_y, destination_x + destination_width, destination_y + destination_height, left_u, top_v, right_u, bottom_v);
		let white_color = self.get_white_color();
		self.draw_vertices(vertex_count_offset / FLOATS_PER_VERTEX, texture, white_color);
	}

	//==============================================================================
	// 이미지 나인패치 출력.
	// - nine_slice 의 position.x / position.y / size.x / size.y 가 각각 왼쪽 / 위 / 오른쪽 / 아래 두께.
	//==============================================================================
	pub fn draw_image_with_nine_slice(&mut self, image: &Rc<Image>, position: Vector2, size: Vector2, nine_slice: Rect) {
		let source_width = image.get_width() as f32;
		let source_height = image.get_height() as f32;
		let destination_x = position.x.floor();
		let destination_y = position.y.floor();
		let left = nine_slice.position.x;
		let top = nine_slice.position.y;
		let right = nine_slice.size.x;
		let bottom = nine_slice.size.y;

		// 최소 사이즈 제한 적용.
		let destination_width = size.x.ceil().max(left + right);
		let destination_height = size.y.ceil().max(top + bottom);

		let has_horizontal = left > 0.0 || right > 0.0;
		let has_vertical = top > 0.0 || bottom > 0.0;

		if has_horizontal && has_vertical {
			// 가로세로 다 쪼개기.
			let center_source_width = source_width - left - right;
			let center_source_height = source_height - top - bottom;
			let center_destination_width = destination_width - left - right;
			let center_destination_height = destination_height - top - bottom;

			// 위쪽.
			self.draw_image_with_source_and_destination(image, 0.0, 0.0, left, top, destination_x, destination_y, left + 1.0, top + 1.0);
			self.draw_image_with_source_and_destination(image, left, 0.0, center_source_width, top, destination_x + left, destination_y, center_destination_width + 1.0, top + 1.0);
			self.draw_image_with_source_and_destination(image, source_width - right, 0.0, right, top, destination_x + destination_width - right, destination_y, right, top + 1.0);

			// 가운데쪽.
			self.draw_image_with_source_and_destination(image, 0.0, top, left, center_source_height, destination_x, destination_y + top, left + 1.0, center_destination_height + 1.0);
			self.draw_image_with_source_and_destination(image, left, top, center_source_width, center_source_height, destination_x + left, destination_y + top, center_destination_width + 1.0, center_destination_height + 1.0);
			self.draw_image_with_source_and_destination(image, source_width - right, top, right, center_source_height, destination_x + destination_width - right, destination_y + top, right, center_destination_height + 1.0);

			// 아래쪽.
			self.draw_image_with_source_and_destination(image, 0.0, source_height - bottom, left, bottom, destination_x, destination_y + destination_height - bottom, left + 1.0, bottom);
			self.draw_image_with_source_and_destination(image, left, source_height - bottom, center_source_width, bottom, destination_x + left, destination_y + destination_height - bottom, center_destination_width + 1.0, bottom);
			self.draw_image_with_source_and_destination(image, source_width - right, source_height - bottom, right, bottom, destination_x + destination_width - right, destination_y + destination_height - bottom, right, bottom);
		}
		else if has_horizontal {
			// 가로만 쪼개기.
			let center_source_width = source_width - left - right;
			let center_destination_width = destination_width - left - right;

			self.draw_image_with_source_and_destination(image, 0.0, 0.0, left, source_height, destination_x, destination_y, left + 1.0, destination_height);
			self.draw_image_with_source_and_destination(image, left, 0.0, center_source_width, source_height, destination_x + left, destination_y, center_destination_width + 1.0, destination_height);
			self.draw_image_with_source_and_destination(image, source_width - right, 0.0, right, source_height, destination_x + destination_width - right, destination_y, right, destination_height);
		}
		else if has_vertical {
			// 세로만 쪼개기.
			let center_source_height = source_height - top - bottom;
			let center_destination_height = destination_height - top - bottom;

			self.draw_image_with_source_and_destination(image, 0.0, 0.0, source_width, top, destination_x, destination_y, destination_width, top + 1.0);
			self.draw_image_with_source_and_destination(image, 0.0, top, source_width, center_source_height, destination_x, destination_y + top, destination_width, center_destination_height + 1.0);
			self.draw_image_with_source_and_destination(image, 0.0, source_height - bottom, source_width, bottom, destination_x, destination_y + destination_height - bottom, destination_width, bottom);
		}
		else {
			// 쪼개기 없음.
			self.draw_image_with_source_and_destination(image, 0.0, 0.0, source_width, source_height, destination_x, destination_y, destination_width, destination_height);
		}
	}

	//==============================================================================
	// 폰트 문자열 설정. (Canvas2D font 대응, 예: "16px DOSGothic")
	//==============================================================================
	pub fn set_font_string(&mut self, font_string: &str) {
		if !font_string.is_empty() && self.font_string != font_string {
			self.font_string.clear();
			self.font_string.push_str(font_string);
		}
	}

	//==============================================================================
	// 폰트 문자열 반환.
	//==============================================================================
	pub fn get_font_string(&self) -> &str {
		&self.font_string
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		self.text_align = text_align;
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn get_text_align(&self) -> TextAlign {
		self.text_align
	}

	//==============================================================================
	// "start" / "end" 를 실제 방향으로 푼다. (이 판은 왼쪽에서 오른쪽만 다룬다)
	//==============================================================================
	pub fn resolve_text_align(&self, text_align: TextAlign) -> TextAlign {
		match text_align {
			TextAlign::Start => {
				TextAlign::Left
			}
			TextAlign::End => {
				TextAlign::Right
			}
			_ => {
				text_align
			}
		}
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: TextBaseline) {
		self.text_baseline = text_baseline;
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn get_text_baseline(&self) -> TextBaseline {
		self.text_baseline
	}

	//==============================================================================
	// 문자열 측정. (현재 폰트 문자열 기준)
	//==============================================================================
	pub fn measure_text(&self, text: &str) -> TextMetrics {
		let font_string = self.get_font_string();
		self.text_string_texture_cache.measure_text(font_string, text)
	}

	//==============================================================================
	// 문자열 측정. (주어진 폰트 문자열 기준, 현재 폰트 문자열은 바꾸지 않는다)
	//==============================================================================
	pub fn measure_text_with_font_string(&self, font_string: &str, text: &str) -> TextMetrics {
		self.text_string_texture_cache.measure_text(font_string, text)
	}

	//==============================================================================
	// 텍스트 베이크 스케일 양자화 단위 설정.
	// - 기본 0.25. 확대 축소가 잦은 글자가 스케일마다 새로 구워지지 않게 묶는다.
	// - 0 이면 양자화하지 않고 실제 스케일 그대로 굽는다.
	//==============================================================================
	pub fn set_text_bake_scale_step(&mut self, text_bake_scale_step: f32) {
		self.text_bake_scale_step = text_bake_scale_step.max(0.0);
	}

	//==============================================================================
	// 텍스트 베이크 스케일 양자화 단위 반환.
	//==============================================================================
	pub fn get_text_bake_scale_step(&self) -> f32 {
		self.text_bake_scale_step
	}

	//==============================================================================
	// 배율이 움직이는 노드의 텍스트 베이크 스케일 양자화 단위 설정. (기본 0.25)
	// - 양자화 단위(set_text_bake_scale_step)가 0 일 때, 그리는 노드의 변환 배율이 지난 프레임과 다르면(확대 움직임, 팝업, 트윈, 카메라 확대)
	//   이 단위로 묶은 배율로 굽는다. 프레임마다 다른 배율로 새로 굽지 않는다.
	// - 배율이 멈추면(지난 프레임과 같으면) 정확한 배율로 다시 구워 1:1 로 또렷하다.
	// - 0 이면 움직이는 동안에도 묶지 않는다. (지금까지와 같다)
	//==============================================================================
	pub fn set_text_bake_motion_scale_step(&mut self, text_bake_motion_scale_step: f32) {
		self.text_bake_motion_scale_step = text_bake_motion_scale_step.max(0.0);
	}

	//==============================================================================
	// 배율이 움직이는 노드의 텍스트 베이크 스케일 양자화 단위 반환.
	//==============================================================================
	pub fn get_text_bake_motion_scale_step(&self) -> f32 {
		self.text_bake_motion_scale_step
	}

	//==============================================================================
	// 지금 그리는 노드의 배율이 움직이는 중인지 설정. (NodeTree 가 정한다, 노드 밖에서 직접 그릴 때 부르는 쪽이 정할 수 있다)
	//==============================================================================
	pub fn set_text_bake_scale_moving(&mut self, is_text_bake_scale_moving: bool) {
		self.is_text_bake_scale_moving = is_text_bake_scale_moving;
	}

	//==============================================================================
	// 지금 그리는 노드의 배율이 움직이는 중인지 반환.
	//==============================================================================
	pub fn is_text_bake_scale_moving(&self) -> bool {
		self.is_text_bake_scale_moving
	}

	//==============================================================================
	// 노드의 변환 배율이 지난 프레임과 달라 움직이는 중인지 셈해 정하고, 노드에 기억할 배율을 돌려준다. (NodeTree 가 push_transform 뒤에 부른다)
	// - last_drawn_maximum_scale 은 그 노드를 지난번 그릴 때의 값이다. (0 이면 아직 모른다, 처음 그리는 노드는 움직이지 않는 것으로 본다)
	// - 양자화 단위가 0 이 아니면 늘 묶어 굽으므로 셈하지 않고 받은 값을 그대로 돌려준다.
	//==============================================================================
	pub fn update_text_bake_scale_moving(&mut self, last_drawn_maximum_scale: f32) -> f32 {
		if self.text_bake_scale_step > 0.0 || self.text_bake_motion_scale_step <= 0.0 {
			self.is_text_bake_scale_moving = false;
			return last_drawn_maximum_scale;
		}
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		self.is_text_bake_scale_moving = last_drawn_maximum_scale > 0.0 && last_drawn_maximum_scale != maximum_scale;
		maximum_scale
	}

	//==============================================================================
	// 텍스트 픽셀 스냅 설정.
	// - 켜면 회전 / 기울임이 없는 변환에서 글자 쿼드의 왼쪽 위를 디바이스 픽셀 격자에 맞춘다.
	//==============================================================================
	pub fn set_text_pixel_snap_enabled(&mut self, is_text_pixel_snap_enabled: bool) {
		self.is_text_pixel_snap_enabled = is_text_pixel_snap_enabled;
	}

	//==============================================================================
	// 텍스트 픽셀 스냅 여부 반환.
	//==============================================================================
	pub fn is_text_pixel_snap_enabled(&self) -> bool {
		self.is_text_pixel_snap_enabled
	}

	//==============================================================================
	// 텍스트 베이크 스케일 계산. (현재 변환 행렬 기반, 양자화 단위는 set_text_bake_scale_step)
	//==============================================================================
	pub fn calculate_text_bake_scale(&self) -> f32 {
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		let mut text_bake_scale_step = self.get_text_bake_scale_step();
		if text_bake_scale_step <= 0.0 && self.is_text_bake_scale_moving() {
			text_bake_scale_step = self.get_text_bake_motion_scale_step();
		}
		let quantized_scale = if text_bake_scale_step > 0.0 {
			(maximum_scale / text_bake_scale_step).round() * text_bake_scale_step
		}
		else {
			maximum_scale
		};
		math::clamp(quantized_scale, 0.25, MAXIMUM_TEXT_BAKE_SCALE)
	}

	//==============================================================================
	// 문자열 텍스처 엔트리를 정렬/베이스라인에 맞춰 쿼드로 출력.
	//==============================================================================
	pub fn draw_text_entry(&mut self, entry: TextEntry, x: f32, y: f32, color: Option<Color>) {
		// 가로 정렬 오프셋.
		let text_align = self.resolve_text_align(self.get_text_align());
		let align_offset = match text_align {
			TextAlign::Center => {
				-entry.advance_width / 2.0
			}
			TextAlign::Right => {
				-entry.advance_width
			}
			_ => {
				0.0
			}
		};

		// 요청 베이스라인 → 알파벳 베이스라인 오프셋.
		let text_baseline = self.get_text_baseline();
		let baseline_offset = match text_baseline {
			TextBaseline::Top => {
				entry.font_ascent
			}
			TextBaseline::Hanging => {
				entry.font_ascent * 0.8
			}
			TextBaseline::Middle => {
				(entry.font_ascent - entry.font_descent) / 2.0
			}
			TextBaseline::Ideographic | TextBaseline::Bottom => {
				-entry.font_descent
			}
			TextBaseline::Alphabetic => {
				0.0
			}
		};

		let mut quad_x = x + align_offset - entry.pen_offset_x;
		let mut quad_y = y + baseline_offset - entry.baseline_offset_y;

		// 픽셀 스냅. (회전 / 기울임이 없을 때만, 쿼드의 왼쪽 위를 디바이스 픽셀에 맞춘다)
		let is_text_pixel_snap_enabled = self.is_text_pixel_snap_enabled();
		let transform_matrix = self.transform_matrix;
		if is_text_pixel_snap_enabled && transform_matrix.b == 0.0 && transform_matrix.c == 0.0 && transform_matrix.a > 0.0 && transform_matrix.d > 0.0 {
			let device_point = transform_matrix.transform_point(quad_x, quad_y);
			quad_x += (device_point.x.round() - device_point.x) / transform_matrix.a;
			quad_y += (device_point.y.round() - device_point.y) / transform_matrix.d;
		}

		let vertex_count_offset = self.write_quad(0, quad_x, quad_y, quad_x + entry.quad_width, quad_y + entry.quad_height, 0.0, 0.0, 1.0, 1.0);
		let white_color = self.get_white_color();
		let draw_color = color.unwrap_or(white_color);
		self.draw_vertices(vertex_count_offset / FLOATS_PER_VERTEX, entry.texture, draw_color);
	}

	//==============================================================================
	// 텍스트 채움 출력. (Canvas2D fillText 대응)
	// - set_fill_color() / set_font_string() / set_text_align() / set_text_baseline()
	//==============================================================================
	pub fn draw_fill_text(&mut self, text: &str, x: f32, y: f32) {
		if text.is_empty() {
			return;
		}

		// 글자는 흰색으로 굽고 그릴 때 색을 곱한다.
		let fill_color = self.get_fill_color();
		let bake_scale = self.calculate_text_bake_scale();
		let entry = self.get_text_entry(TextDrawKind::Fill, text, 0.0, bake_scale);
		if let Some(entry) = entry {
			self.draw_text_entry(entry, x, y, Some(fill_color));
		}
	}

	//==============================================================================
	// 텍스트 외곽선 출력. (Canvas2D strokeText 대응)
	// - set_stroke_color() / set_font_string() / set_text_align() / set_text_baseline()
	//==============================================================================
	pub fn draw_stroke_text(&mut self, text: &str, x: f32, y: f32, line_width: f32) {
		if text.is_empty() {
			return;
		}

		let stroke_color = self.get_stroke_color();
		let bake_scale = self.calculate_text_bake_scale();
		let entry = self.get_text_entry(TextDrawKind::Stroke, text, line_width, bake_scale);
		if let Some(entry) = entry {
			self.draw_text_entry(entry, x, y, Some(stroke_color));
		}
	}

	//==============================================================================
	// 현재 폰트 문자열로 구운 문자열 반환. (없으면 굽는다)
	//==============================================================================
	fn get_text_entry(&mut self, text_draw_kind: TextDrawKind, text: &str, line_width: f32, bake_scale: f32) -> Option<TextEntry> {
		if let Some(entry) = self.text_string_texture_cache.find_entry(text_draw_kind, text, &self.font_string, line_width, bake_scale) {
			return Some(entry);
		}

		// 새로 굽거나 오래된 것을 버리면 텍스처 바인드가 바뀐다.
		self.bound_texture.set(None);
		self.text_string_texture_cache.get_entry(&self.gl, text_draw_kind, text, &self.font_string, line_width, bake_scale)
	}

	//==============================================================================
	// 출력 영역 제한 시작.
	// - 스텐실 버퍼 기반이라 회전/스케일이 걸린 영역도 정확히 잘린다.
	// - round_size 를 주면 둥근 사각형으로 잘린다.
	//==============================================================================
	pub fn begin_clip_rect(&mut self, rect: Rect, round_size: f32) {
		// 모아 둔 것은 지금 스텐실 상태로 먼저 내보낸다.
		self.flush_batch();

		// 상태 저장. (Canvas2D save 대응)
		self.push_state();

		let previous_clip_depth = self.clip_stack.len() as i32;
		self.clip_stack.push(ClipEntry {
			rect,
			round_size,
			transform_matrix: self.transform_matrix,
		});

		unsafe {
			if previous_clip_depth == 0 {
				self.gl.enable(glow::STENCIL_TEST);
			}

			// 스텐실에 클립 영역 기록. (색상 출력 없이 INCR)
			self.gl.color_mask(false, false, false, false);
			self.gl.stencil_func(glow::ALWAYS, 0, 0xFF);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::INCR);
		}
		self.batch_suspend_depth += 1;
		if round_size > 0.0 {
			self.draw_round_rect(rect, round_size);
		}
		else {
			self.draw_rect(rect);
		}
		self.batch_suspend_depth -= 1;
		unsafe {
			self.gl.color_mask(true, true, true, true);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
			self.gl.stencil_func(glow::EQUAL, previous_clip_depth + 1, 0xFF);
		}
	}

	//==============================================================================
	// 출력 영역 제한 종료.
	//==============================================================================
	pub fn end_clip_rect(&mut self) {
		let clip_entry = match self.clip_stack.pop() {
			Some(clip_entry) => {
				clip_entry
			}
			None => {
				return;
			}
		};

		// 클립 안에서 모아 둔 것을 먼저 내보낸다.
		self.flush_batch();

		// 기록 시점의 변환 행렬로 같은 영역을 DECR 해 스텐실 원복.
		// (begin 과 end 사이에 변환이 바뀌었을 수 있으므로 스냅샷 사용)
		self.transform_matrix = clip_entry.transform_matrix;
		unsafe {
			self.gl.color_mask(false, false, false, false);
			self.gl.stencil_func(glow::ALWAYS, 0, 0xFF);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::DECR);
		}
		self.batch_suspend_depth += 1;
		if clip_entry.round_size > 0.0 {
			self.draw_round_rect(clip_entry.rect, clip_entry.round_size);
		}
		else {
			self.draw_rect(clip_entry.rect);
		}
		self.batch_suspend_depth -= 1;
		unsafe {
			self.gl.color_mask(true, true, true, true);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);

			let clip_depth = self.clip_stack.len() as i32;
			if clip_depth == 0 {
				self.gl.disable(glow::STENCIL_TEST);
			}
			else {
				self.gl.stencil_func(glow::EQUAL, clip_depth, 0xFF);
			}
		}

		// 상태 복원. (Canvas2D restore 대응)
		self.pop_state();
	}

	//==============================================================================
	// 셰이더 프로그램 반환. (바꿔 끼운 것이 있으면 그것, 없으면 기본 프로그램)
	//==============================================================================
	pub fn get_shader_program(&self) -> &ShaderProgram {
		match &self.shader_program_override {
			Some(shader_program_override) => {
				shader_program_override
			}
			None => {
				&self.shader_program
			}
		}
	}

	//==============================================================================
	// 셰이더 프로그램 바꿔 끼우기. (None 이면 기본 프로그램으로 복귀)
	// - 기본 프로그램과 같은 어트리뷰트 위치 / 유니폼 이름(projectionMatrix, model_matrix, mainColor, tint_color, global_alpha, mainTexture)을 갖춰야 한다.
	// - 바꿔 끼운 프로그램이 쓰이는 동안 그 프로그램이 사용 중이다. 효과 유니폼은 바꿔 끼운 바로 뒤에 GL 로 직접 올린다.
	//==============================================================================
	pub fn set_shader_program_override(&mut self, shader_program: Option<Rc<ShaderProgram>>) {
		self.flush_batch();
		self.shader_program_override = shader_program;
		let active_shader_program = self.get_shader_program();
		self.use_shader_program(active_shader_program);
		let projection_matrix_location = active_shader_program.get_uniform_location(&self.gl, "projectionMatrix");
		let main_texture_location = active_shader_program.get_uniform_location(&self.gl, "mainTexture");
		unsafe {
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
		}
		self.apply_image_mipmap_bias(active_shader_program);
	}

	//==============================================================================
	// 이름으로 담아 둔 바꿔 끼울 셰이더 프로그램 반환. (없으면 create_shader_source 가 준 버텍스 / 프래그먼트 소스로 만들어 담는다)
	// - 원본 ShaderSprite 의 컨텍스트별 프로그램 캐시 자리. 러스트 판은 GL 컨텍스트를 든 Graphic 이 담는다.
	// - 어트리뷰트 위치를 기본 프로그램과 같게 고정해 링크하므로 기본 버텍스 어레이를 그대로 쓴다.
	// - 만들기에 실패하면 한 번만 알리고 None 을 담아 두어 다시 만들지 않는다.
	//==============================================================================
	pub fn resolve_shader_program(&mut self, program_group_name: &'static str, program_name: &'static str, create_shader_source: impl FnOnce() -> (String, String)) -> Option<Rc<ShaderProgram>> {
		let cache_key = (program_group_name, program_name);
		if let Some(cached_shader_program) = self.shader_program_cache_table.get(&cache_key) {
			return cached_shader_program.clone();
		}
		let (vertex_shader_source, fragment_shader_source) = create_shader_source();
		let mut attribute_location_list: Vec<(&str, u32)> = Vec::new();
		for attribute_name in ["vertexPosition", "vertexTextureCoordinate"] {
			if let Some(attribute_location) = self.shader_program.get_attribute_location(&self.gl, attribute_name) {
				attribute_location_list.push((attribute_name, attribute_location));
			}
		}
		let shader_program = match ShaderProgram::create_with_attribute_locations(&self.gl, &vertex_shader_source, &fragment_shader_source, &attribute_location_list) {
			Ok(shader_program) => {
				Some(Rc::new(shader_program))
			}
			Err(error) => {
				eprintln!("Graphic: shader program \"{}.{}\" failed: {}", program_group_name, program_name, error);
				None
			}
		};
		self.shader_program_cache_table.insert(cache_key, shader_program.clone());
		shader_program
	}

	//==============================================================================
	// 렌더 상태 복구. (외부 GL 패스가 프로그램 / 버텍스 어레이 / 뷰포트 / 블렌드를 바꾼 뒤 Graphic 출력을 이어갈 때)
	// - 원본과 같고, 클립 중이면 스텐실 테스트도 다시 켠다. (원본은 켜지 않는다)
	// - 기억한 프로그램과 텍스처를 잊고 다시 건다.
	//==============================================================================
	pub fn restore_render_state(&mut self) {
		self.forget_bound_render_state();
		let shader_program = self.get_shader_program();
		self.use_shader_program(shader_program);
		let projection_matrix_location = shader_program.get_uniform_location(&self.gl, "projectionMatrix");
		let main_texture_location = shader_program.get_uniform_location(&self.gl, "mainTexture");
		unsafe {
			self.gl.viewport(0, 0, self.applied_viewport_width, self.applied_viewport_height);
			self.gl.bind_vertex_array(Some(self.vertex_array));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
			self.gl.active_texture(glow::TEXTURE0);
			self.gl.enable(glow::BLEND);
			self.gl.disable(glow::DEPTH_TEST);
			self.gl.disable(glow::CULL_FACE);
			if !self.clip_stack.is_empty() {
				self.gl.enable(glow::STENCIL_TEST);
			}
		}
		let blend_mode = self.get_blend_mode();
		self.apply_blend_mode(blend_mode);
	}

	//==============================================================================
	// 드로잉 버퍼 너비 반환. (픽셀, 원본 webGL2RenderingContext.drawing_buffer_width)
	//==============================================================================
	pub fn get_drawing_buffer_width(&self) -> i32 {
		self.applied_viewport_width
	}

	//==============================================================================
	// 드로잉 버퍼 높이 반환. (픽셀, 원본 webGL2RenderingContext.drawing_buffer_height)
	//==============================================================================
	pub fn get_drawing_buffer_height(&self) -> i32 {
		self.applied_viewport_height
	}

	//==============================================================================
	// 지금 그린 화면 읽기. (너비, 높이, 위에서 아래로 놓인 RGBA. 화면을 넘기기 전에 부른다)
	// - 원본 캔버스처럼 불투명으로 본다. (알파 255)
	//==============================================================================
	pub fn read_framebuffer_pixels(&self) -> (u32, u32, Vec<u8>) {
		let width = self.applied_viewport_width.max(0) as usize;
		let height = self.applied_viewport_height.max(0) as usize;
		let row_byte_count = width * 4;
		let mut bottom_up_pixels = vec![0u8; row_byte_count * height];
		unsafe {
			self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
			self.gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut bottom_up_pixels)));
		}
		let mut top_down_pixels = vec![0u8; row_byte_count * height];
		for row_index in 0..height {
			let source_start = (height - 1 - row_index) * row_byte_count;
			let destination_start = row_index * row_byte_count;
			top_down_pixels[destination_start..destination_start + row_byte_count].copy_from_slice(&bottom_up_pixels[source_start..source_start + row_byte_count]);
		}
		for pixel in top_down_pixels.as_chunks_mut::<4>().0 {
			pixel[3] = 255;
		}
		(width as u32, height as u32, top_down_pixels)
	}

	//==============================================================================
	// GL 컨텍스트 반환.
	// - 받은 쪽이 프로그램이나 텍스처를 바꿀 수 있어, 기억한 프로그램과 텍스처를 잊는다. (다음 그리기에서 다시 건다)
	//==============================================================================
	pub fn get_gl(&self) -> &glow::Context {
		self.forget_bound_render_state();
		&self.gl
	}

	//==============================================================================
	// 흰색 텍스처 반환.
	//==============================================================================
	pub fn get_white_texture(&self) -> GlTexture {
		self.white_texture
	}

	//==============================================================================
	// 흰색 반환.
	//==============================================================================
	pub fn get_white_color(&self) -> Color {
		self.white_color
	}

	//==============================================================================
	// 이미지 텍스처 캐시 반환.
	//==============================================================================
	pub fn get_image_texture_cache(&self) -> &ImageTextureCache {
		&self.image_texture_cache
	}

	//==============================================================================
	// 문자열 텍스처 캐시 반환. (폰트 등록)
	//==============================================================================
	pub fn get_text_string_texture_cache(&mut self) -> &mut TextStringTextureCache {
		&mut self.text_string_texture_cache
	}
}

//==============================================================================
// f32 배열을 바이트 배열로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}
