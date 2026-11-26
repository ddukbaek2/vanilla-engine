//==============================================================================
//! 툰 화면 공간 바깥선. (vanilla.js graphics3d/toon/toonscreenoutline.js 의 ToonScreenOutline)
//! - 노멀, 선 번호, 깊이를 선의 판에 그리고, 바로 이웃 사이의 경계에서 선을 잡아 점프 플러딩으로 넓힌 뒤 거리로 덮는 몫을 셈해 섞는다.
//! - 선의 판: ToonRenderer 의 create_mesh 결과를 같은 행렬로 한 번 더 그려 노멀(팔면체 접기), 선 번호(14 비트), 꼴(바깥 선, 꺾임 선을 켤지)과 깊이를 쓴다.
//! - 잡기: 화소와 바로 이웃 넷 사이의 경계마다 한 번 본다. 선은 화소가 아니라 화소 사이의 경계 가운데에 선다. (같은 경계를 두 번 긋지 않는다)
//!   바깥 선은 빈 곳과 맞닿은 실루엣, 다른 번호와 맞닿은 곳, 같은 번호라도 서로의 접평면에서 깊이 문턱보다 떨어진 곳(가려진 실루엣)이다.
//!   꺾임 선은 같은 번호 안에서 이웃한 두 노멀이 꺾임 각보다 더 벌어진 곳이다. 화소 하나 사이의 노멀만 보므로 둥근 면의 실루엣 둘레나 비스듬한 면에 띠가 서지 않는다.
//!   깊이를 접평면 거리로 견주므로 비스듬히 보는 바닥에 가짜 선이 서지 않는다. 문턱에 그 깊이의 화소 크기 두 배를 더해 먼 것도 같은 뜻이 된다.
//! - 넓히기: 경계 토막까지의 거리를 점프 플러딩으로 퍼뜨린다. 굵기에 상관없이 걸음 수는 log2 이고, 바깥 선과 꺾임 선은 따로 퍼뜨려 굵기를 따로 준다.
//! - 섞기: 그리는 영역의 화소마다 가장 가까운 경계까지의 거리로 clamp(굵기 / 2 - 거리 + 0.5, 0, 1) 만큼 덮는다. (가장자리가 부드럽고, 선의 판을 줄여도 늘인 선이 점선이 되지 않는다)
//! - 모형마다: ToonLineDrawOptions 의 has_screen_outline 이 false 면 그 모형은 선을 내지 않고 뒤의 선만 가린다. has_crease_line 이 false 면 꺾임 선만 끈다.
//! - 굵기: set_width 와 set_crease 의 굵기는 선 하나의 굵기다. (경계를 가운데로 양쪽에 반씩) 단위는 ToonOutlineWidthUnit, 한 픽셀 밑으로 내리지 않는다.
//! - 깊이: 투영 행렬로 직교와 원근을 가리고, 깊이를 보기 공간 자리로 풀어 견준다. (화면 사각형처럼 클립 x, y 를 옮긴 투영도 된다)
//! - 값: set_resolution_scale 로 넓히기(점프 플러딩) 판을 줄인다. (폰의 가벼운 길) 선의 판과 씨앗은 그리는 영역 해상도라 줄여도 선 자리는 그대로다.
//! - 사용: 매 프레임 begin_line_ids(gl, 보기, 투영) → draw_mesh / draw_mesh_instanced → end_line_ids(gl) (씨앗과 넓히기까지 셈) → 툰 몸 그리기 → draw_outline(gl).
//!   begin_line_ids 는 그때의 뷰포트를 그리는 영역으로 적어 두고, 선의 판을 그 크기에 맞춰 짓는다. end_line_ids 와 draw_outline 이 프레임버퍼, 뷰포트, 상태를 돌린다.
//!   ToonDrawQueue::set_screen_outline 으로 맡기면 그리기 줄이 이 차례를 스스로 한다. (엔진 2D 그리기 사이에 끼울 때, 그 전에 Graphic 의 묶음을 내보내고 끝에 restore_render_state)
//! - 인스턴스마다의 번호는 ToonInstance 의 line_id 다. (원본 인스턴스 객체의 lineId, None 이면 공통 번호)
//!
//! # 예제
//! 툰 화면 공간 바깥선을 한 프레임에 긋기
//! ```rust,no_run
//! use vanilla::{create_box_geometry, Matrix4, MeshBuilder, ToonDrawOptions, ToonLineDrawOptions, ToonOutlineWidthUnit, ToonPartOptions, ToonRenderer, ToonScreenOutline, Vector3};
//!
//! fn draw_outlined_box(gl: &glow::Context) -> Result<(), String> {
//! 	let mut renderer = ToonRenderer::new(gl)?;
//! 	let mut outline = ToonScreenOutline::new(gl)?;
//! 	outline.set_width(2.0, ToonOutlineWidthUnit::Pixel);
//! 	outline.set_crease(50.0, 1.0, 0.8);
//! 	let mut builder = MeshBuilder::new();
//! 	builder.add_part(&create_box_geometry(1.0, 1.0, 1.0), &Matrix4::create_identity(), "#e0c8a8", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
//! 	let mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("mesh failed")?;
//! 	let view_matrix = Matrix4::create_look_at(Vector3::create(2.0, 2.0, 3.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
//! 	let projection_matrix = Matrix4::create_perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
//! 	let model_matrix = Matrix4::create_identity();
//! 	if outline.begin_line_ids(gl, &view_matrix, &projection_matrix) {
//! 		outline.draw_mesh(gl, &mesh, &model_matrix, 1, ToonLineDrawOptions::default());
//! 		outline.end_line_ids(gl);
//! 	}
//! 	renderer.begin_frame_with_matrices(gl, &view_matrix, &projection_matrix);
//! 	renderer.draw_mesh(gl, &mesh, &model_matrix, ToonDrawOptions::default());
//! 	renderer.end_frame(gl);
//! 	outline.draw_outline(gl);
//! 	Ok(())
//! }
//! ```
//! <!-- image: toon-style.webp -->
//==============================================================================
use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::core::graphic::{GlProgram, GlTexture, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::toon::toonrenderer::{read_matrix_determinant, ToonInstance, ToonMesh, TOON_ATTRIBUTE_LOCATIONS, TOON_INSTANCE_FLOAT_COUNT, TOON_MAXIMUM_JOINTS};
use crate::graphics3d::GlFramebuffer;

/// 선 번호의 상한. (선의 판에 14 비트로 쓴다, 0 은 빈 곳)
pub const TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID: u32 = 16383;

/// 선의 판과 씨앗 판을 묶는 텍스처 유닛. (툰 렌더러의 그림자 유닛 3 과 겹치지 않게)
const LINE_TEXTURE_UNIT: u32 = 5;
const DEPTH_TEXTURE_UNIT: u32 = 6;

//==============================================================================
/// 굵기의 단위.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonOutlineWidthUnit {
	/// 월드 길이. (직교는 투영의 세로 배율로, 원근은 초점 거리에서 화면 픽셀로 셈해 멀수록 가늘어진다)
	World,
	/// 그리는 영역의 픽셀. (거리와 상관없이 같은 굵기)
	Pixel,
}

//==============================================================================
/// 번호를 그릴 때의 꼴. (원본 drawMesh 의 options)
///
/// # 예제
/// 바닥은 선 없이 가리기만, 결정은 꺾임 선 없이
/// ```rust,no_run
/// use vanilla::{create_box_geometry, create_octahedron_geometry, Matrix4, MeshBuilder, ToonLineDrawOptions, ToonPartOptions, ToonRenderer, ToonScreenOutline, Vector3};
///
/// fn draw_floor_and_crystal(gl: &glow::Context, renderer: &ToonRenderer, outline: &mut ToonScreenOutline) -> Option<()> {
/// 	let part_options = ToonPartOptions { has_outline: false, ..ToonPartOptions::default() };
/// 	let mut floor_builder = MeshBuilder::new();
/// 	floor_builder.add_part(&create_box_geometry(6.0, 0.2, 6.0), &Matrix4::create_translation(0.0, -0.1, 0.0), "#506070", part_options);
/// 	let floor_mesh = renderer.create_mesh(gl, &floor_builder.read_arrays())?;
/// 	let mut crystal_builder = MeshBuilder::new();
/// 	crystal_builder.add_part(&create_octahedron_geometry(0.5), &Matrix4::create_translation(0.0, 0.5, 0.0), "#90d0ff", part_options);
/// 	let crystal_mesh = renderer.create_mesh(gl, &crystal_builder.read_arrays())?;
/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(3.0, 3.0, 4.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	let projection_matrix = Matrix4::create_perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
/// 	if outline.begin_line_ids(gl, &view_matrix, &projection_matrix) {
/// 		outline.draw_mesh(gl, &floor_mesh, &Matrix4::create_identity(), 1, ToonLineDrawOptions { has_screen_outline: false, ..ToonLineDrawOptions::default() });
/// 		outline.draw_mesh(gl, &crystal_mesh, &Matrix4::create_identity(), 2, ToonLineDrawOptions { has_crease_line: false, ..ToonLineDrawOptions::default() });
/// 		outline.end_line_ids(gl);
/// 	}
/// 	Some(())
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonLineDrawOptions<'a> {
	/// 뼈대 모형의 본 행렬. (본마다 수 16, TOON_MAXIMUM_JOINTS 개까지, ToonRenderer::draw_mesh 와 같은 자세로 주고, None 이면 자세 없이 그린다)
	pub joint_matrices: Option<&'a [f32]>,
	/// 화면 밖이면 건너뛸지. (경계 구가 절두체 밖이면 그리지 않는다, 기본 true)
	pub is_culled: bool,
	/// 몸 번호 버퍼의 일부만. (부품에 다른 번호를 줄 때, 시작 번호와 수, 수가 None 이면 끝까지)
	pub index_start: usize,
	/// 몸 번호 버퍼에서 그릴 수. (None 이면 index_start 부터 끝까지)
	pub index_count: Option<usize>,
	/// 감기를 뒤집어 그릴지. (None 이면 모형 행렬의 행렬식이 음수일 때, 인스턴스는 None 이면 뒤집지 않음)
	pub is_mirrored: Option<bool>,
	/// 바깥 선을 낼지. (false 면 선은 내지 않고 뒤의 선만 가린다)
	pub has_screen_outline: bool,
	/// 꺾임 선을 낼지.
	pub has_crease_line: bool,
}

impl Default for ToonLineDrawOptions<'_> {
	//==============================================================================
	/// 기본값. (거르기 켬, 몸 전체, 바깥 선과 꺾임 선 켬)
	//==============================================================================
	fn default() -> Self {
		ToonLineDrawOptions { joint_matrices: None, is_culled: true, index_start: 0, index_count: None, is_mirrored: None, has_screen_outline: true, has_crease_line: true }
	}
}

//==============================================================================
/// 선의 판. (그리는 영역 해상도의 노멀과 번호 RGBA8, 깊이 24, 씨앗 RGBA16UI, 넓히기 판 RGBA16UI 둘, jump_flood_index 는 넓히기를 마친 쪽)
///
/// # 예제
/// 선의 판과 넓히기 판의 크기 읽기
/// ```rust,no_run
/// use vanilla::ToonScreenOutline;
///
/// fn print_line_target(outline: &ToonScreenOutline) {
/// 	if let Some(line_target) = outline.get_line_target() {
/// 		println!("line {} x {}, jump flood {} x {}", line_target.width, line_target.height, line_target.jump_flood_width, line_target.jump_flood_height);
/// 	}
/// }
/// ```
//==============================================================================
pub struct ToonLineTarget {
	/// 선의 판 프레임버퍼. (노멀과 번호 텍스처, 깊이 텍스처)
	pub framebuffer: GlFramebuffer,
	/// 노멀과 번호 텍스처. (RGBA8, 노멀은 팔면체로 접어 rg, 번호와 꼴은 ba)
	pub line_texture: GlTexture,
	/// 깊이 텍스처. (깊이 24)
	pub depth_texture: GlTexture,
	/// 씨앗 판 프레임버퍼.
	pub seed_framebuffer: GlFramebuffer,
	/// 온 해상도 씨앗 텍스처. (RGBA16UI, rg 는 바깥 선, ba 는 꺾임 선)
	pub seed_texture: GlTexture,
	/// 넓히기 판 프레임버퍼 둘. (번갈아 쓴다)
	pub jump_flood_framebuffers: [GlFramebuffer; 2],
	/// 넓히기 판 텍스처 둘. (RGBA16UI)
	pub jump_flood_textures: [GlTexture; 2],
	/// 넓히기를 마친 쪽. (jump_flood_textures 의 차례, 0 이나 1)
	pub jump_flood_index: usize,
	/// 선의 판 너비. (그리는 영역 픽셀)
	pub width: i32,
	/// 선의 판 높이. (그리는 영역 픽셀)
	pub height: i32,
	/// 넓히기 판 너비. (픽셀)
	pub jump_flood_width: i32,
	/// 넓히기 판 높이. (픽셀)
	pub jump_flood_height: i32,
}

//==============================================================================
/// 이번 프레임의 카메라.
//==============================================================================
#[derive(Clone, Copy)]
struct LineFrame {
	view_matrix: Matrix4,
	projection_matrix: Matrix4,
	frustum_planes: [[f32; 4]; 6],
	is_perspective: bool,
	viewport: [i32; 4],
}

//==============================================================================
/// 앞의 그리기 상태.
//==============================================================================
struct SavedState {
	is_blend_enabled: bool,
	is_depth_test_enabled: bool,
	is_cull_face_enabled: bool,
	is_scissor_test_enabled: bool,
	depth_mask: bool,
	depth_function: i32,
	front_face: i32,
	blend_source_rgb: i32,
	blend_destination_rgb: i32,
	blend_source_alpha: i32,
	blend_destination_alpha: i32,
	clear_color: [f32; 4],
	program: Option<GlProgram>,
	vertex_array: Option<GlVertexArray>,
	viewport: [i32; 4],
	framebuffer: Option<GlFramebuffer>,
}

//==============================================================================
/// 툰 화면 공간 바깥선. (머리말 참고)
///
/// # 예제
/// 꺾임 선 없이 실루엣만 굵게
/// ```rust,no_run
/// use vanilla::{ToonOutlineWidthUnit, ToonScreenOutline};
///
/// fn create_silhouette_outline(gl: &glow::Context) -> Result<ToonScreenOutline, String> {
/// 	let mut outline = ToonScreenOutline::new(gl)?;
/// 	outline.set_color([0.1, 0.08, 0.12, 1.0]);
/// 	outline.set_width(3.0, ToonOutlineWidthUnit::Pixel);
/// 	outline.set_crease(180.0, 1.0, 1.0);
/// 	outline.set_resolution_scale(0.5);
/// 	Ok(outline)
/// }
/// ```
//==============================================================================
pub struct ToonScreenOutline {
	line_program: ShaderProgram,
	seed_program: ShaderProgram,
	seed_gather_program: ShaderProgram,
	jump_flood_program: ShaderProgram,
	composite_program: ShaderProgram,
	empty_vertex_array: GlVertexArray,
	line_target: Option<ToonLineTarget>,
	color: [f32; 4],
	width: f32,
	width_unit: ToonOutlineWidthUnit,
	focus_distance: f32,
	depth_threshold: f32,
	crease_cosine: f32,
	crease_width: f32,
	crease_opacity: f32,
	resolution_scale: f32,
	is_hdr_output: bool,
	frame: Option<LineFrame>,
	saved_state: Option<SavedState>,
	is_drawing_line_ids: bool,
	instance_data: Vec<f32>,
	drawn_distance_sum: f32,
	drawn_distance_count: u32,
	line_widths: [f32; 2],
}

//==============================================================================
/// f32 배열을 바이트로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
/// 보기와 투영 행렬에서 절두체의 면 여섯. (ToonRenderer 와 같은 셈, ax + by + cz + d >= 0 이 안쪽)
//==============================================================================
fn extract_frustum_planes(matrix: &Matrix4) -> [[f32; 4]; 6] {
	let elements = &matrix.elements;
	let row = |row_index: usize| [elements[row_index], elements[4 + row_index], elements[8 + row_index], elements[12 + row_index]];
	let row0 = row(0);
	let row1 = row(1);
	let row2 = row(2);
	let row3 = row(3);
	let combine = |first: [f32; 4], second: [f32; 4], sign: f32| [first[0] + second[0] * sign, first[1] + second[1] * sign, first[2] + second[2] * sign, first[3] + second[3] * sign];
	let mut planes = [combine(row3, row0, 1.0), combine(row3, row0, -1.0), combine(row3, row1, 1.0), combine(row3, row1, -1.0), combine(row3, row2, 1.0), combine(row3, row2, -1.0)];
	for plane in planes.iter_mut() {
		let length = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
		if length > 0.0 {
			for value in plane.iter_mut() {
				*value /= length;
			}
		}
	}
	planes
}

//==============================================================================
/// 점프 플러딩의 걸음 목록. (씨앗을 reach 화소까지 넓히는 데 드는 걸음, 큰 것부터 반씩 줄여 1 까지)
/// - 걸음 k, k/2, ..., 1 은 씨앗을 2k - 1 화소까지 옮긴다. (Rong, Tan 2006)
///
/// # 인자
/// - `reach`: 씨앗을 옮길 거리. (선의 판 픽셀)
///
/// # 돌려줌
/// 큰 것부터 1 까지의 걸음.
///
/// # 예제
/// 4 화소까지 넓히는 걸음
/// ```rust
/// use vanilla::read_jump_flood_steps;
///
/// assert_eq!(read_jump_flood_steps(4.0), vec![4, 2, 1]);
/// ```
//==============================================================================
pub fn read_jump_flood_steps(reach: f32) -> Vec<i32> {
	let distance = (reach.ceil() as i32).max(1);
	let mut step = 1;
	while step * 2 - 1 < distance {
		step *= 2;
	}
	let mut steps = Vec::new();
	while step >= 1 {
		steps.push(step);
		step /= 2;
	}
	steps
}

//==============================================================================
/// 선 굵기 셈. ([바깥 선, 꺾임 선] 의 그리는 영역 픽셀, 0 이 아니면 한 픽셀 밑으로 내리지 않음, 원근의 월드 굵기는 초점 거리에서)
//==============================================================================
fn compute_line_widths(widths: [f32; 2], width_unit: ToonOutlineWidthUnit, viewport_height: i32, projection_scale: f32, is_perspective: bool, focus_distance: f32) -> [f32; 2] {
	let mut pixels_per_unit = 1.0;
	if width_unit == ToonOutlineWidthUnit::World {
		pixels_per_unit = viewport_height as f32 * 0.5 * projection_scale;
		if is_perspective {
			pixels_per_unit /= focus_distance.max(0.001);
		}
	}
	let to_pixels = |width: f32| if width > 0.0 { (width * pixels_per_unit).max(1.0) } else { 0.0 };
	[to_pixels(widths[0]), to_pixels(widths[1])]
}

impl ToonScreenOutline {
	//==============================================================================
	/// 셰이더 소스. ((정점, 조각) 다섯 벌: 번호, 씨앗, 모으기, 넓히기, 섞기)
	///
	/// # 돌려줌
	/// (정점 셰이더, 조각 셰이더) 다섯. (번호, 씨앗, 모으기, 넓히기, 섞기, 번호 셰이더의 어트리뷰트 자리는 TOON_ATTRIBUTE_LOCATIONS)
	//==============================================================================
	pub fn read_program_sources() -> [(&'static str, &'static str); 5] {
		[(LINE_VERTEX_SHADER, LINE_FRAGMENT_SHADER), (SCREEN_TRIANGLE_VERTEX_SHADER, SEED_FRAGMENT_SHADER), (SCREEN_TRIANGLE_VERTEX_SHADER, SEED_GATHER_FRAGMENT_SHADER), (SCREEN_TRIANGLE_VERTEX_SHADER, JUMP_FLOOD_FRAGMENT_SHADER), (SCREEN_TRIANGLE_VERTEX_SHADER, COMPOSITE_FRAGMENT_SHADER)]
	}

	//==============================================================================
	/// 생성. (다섯 프로그램과 빈 버텍스 어레이를 짓는다, 선의 판은 첫 begin_line_ids 에서)
	///
	/// # 인자
	/// - `gl`: 툰 렌더러와 같은 컨텍스트.
	///
	/// # 돌려줌
	/// 새 화면 공간 바깥선. 셰이더를 짓지 못하거나 버텍스 어레이를 못 만들면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<ToonScreenOutline, String> {
		let line_program = ShaderProgram::create_with_attribute_locations(gl, LINE_VERTEX_SHADER, LINE_FRAGMENT_SHADER, &TOON_ATTRIBUTE_LOCATIONS)?;
		let seed_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, SEED_FRAGMENT_SHADER)?;
		let seed_gather_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, SEED_GATHER_FRAGMENT_SHADER)?;
		let jump_flood_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, JUMP_FLOOD_FRAGMENT_SHADER)?;
		let composite_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, COMPOSITE_FRAGMENT_SHADER)?;
		let empty_vertex_array = unsafe { gl.create_vertex_array()? };
		Ok(ToonScreenOutline {
			line_program,
			seed_program,
			seed_gather_program,
			jump_flood_program,
			composite_program,
			empty_vertex_array,
			line_target: None,
			color: [0.08, 0.07, 0.1, 1.0],
			width: 2.0,
			width_unit: ToonOutlineWidthUnit::Pixel,
			focus_distance: 0.0,
			depth_threshold: 0.05,
			crease_cosine: (std::f32::consts::PI / 3.0).cos(),
			crease_width: 1.0,
			crease_opacity: 1.0,
			resolution_scale: 1.0,
			is_hdr_output: false,
			frame: None,
			saved_state: None,
			is_drawing_line_ids: false,
			instance_data: Vec::new(),
			drawn_distance_sum: 0.0,
			drawn_distance_count: 0,
			line_widths: [2.0, 1.0],
		})
	}

	//==============================================================================
	/// 선 색 설정. (sRGB 0 ~ 1 과 알파, 바깥 선과 꺾임 선이 같이 쓴다)
	///
	/// # 인자
	/// - `color`: [r, g, b, a] 의 sRGB.
	//==============================================================================
	pub fn set_color(&mut self, color: [f32; 4]) {
		self.color = color;
	}

	//==============================================================================
	/// 바깥 선 굵기 설정. (실루엣과 다른 번호와 맞닿은 곳의 선, 반지름이 아니라 선 하나의 굵기, 0 이면 긋지 않음, 꺾임 선 굵기도 이 단위로 센다)
	///
	/// # 인자
	/// - `width`: 선 하나의 굵기. (경계를 가운데로 양쪽에 반씩)
	/// - `unit`: 굵기의 단위. (원본 기본 Pixel)
	//==============================================================================
	pub fn set_width(&mut self, width: f32, unit: ToonOutlineWidthUnit) {
		self.width = width.max(0.0);
		self.width_unit = unit;
	}

	//==============================================================================
	/// 꺾임 선 설정. (같은 번호 안에서 이웃한 면이 이 각(도)보다 더 꺾이면 선, 굵기는 set_width 의 단위, 진하기 0 ~ 1, 180 도나 굵기 0 이면 긋지 않음)
	///
	/// # 인자
	/// - `angle_degrees`: 꺾임 각. (180 이면 꺾임 선을 긋지 않는다)
	/// - `width`: 꺾임 선 하나의 굵기. (원본 기본 1, 0 이면 긋지 않는다)
	/// - `opacity`: 꺾임 선의 진하기. (원본 기본 1, 선 색의 알파에 곱한다)
	//==============================================================================
	pub fn set_crease(&mut self, angle_degrees: f32, width: f32, opacity: f32) {
		self.crease_cosine = angle_degrees.clamp(0.0, 180.0).to_radians().cos();
		self.crease_width = width.max(0.0);
		self.crease_opacity = opacity.clamp(0.0, 1.0);
	}

	//==============================================================================
	/// 초점 거리 설정. (원근 카메라에서 월드 굵기를 픽셀로 셀 거리, 보통 카메라에서 보는 점까지, 0 이면 그린 모형들의 평균 거리)
	///
	/// # 인자
	/// - `focus_distance`: 카메라에서의 거리. (월드 길이)
	//==============================================================================
	pub fn set_focus_distance(&mut self, focus_distance: f32) {
		self.focus_distance = focus_distance.max(0.0);
	}

	//==============================================================================
	/// 깊이 문턱 설정. (같은 번호의 이웃 화소가 서로의 접평면에서 이만큼 넘게 떨어지면 가려진 실루엣으로 긋는다, 월드 길이, 그 깊이의 화소 크기 두 배를 더해 쓴다)
	///
	/// # 인자
	/// - `depth_threshold`: 접평면 거리 문턱. (월드 길이)
	//==============================================================================
	pub fn set_depth_threshold(&mut self, depth_threshold: f32) {
		self.depth_threshold = depth_threshold.max(0.0);
	}

	//==============================================================================
	/// 넓히기 판의 해상도 몫 설정. (그리는 영역에 대한 몫 0.25 ~ 1, 선의 판과 씨앗은 그리는 영역 해상도로 잡고 점프 플러딩만 줄인 판에서 해서, 줄여도 계단과 점선이 없다)
	///
	/// # 인자
	/// - `resolution_scale`: 0.25 ~ 1.
	//==============================================================================
	pub fn set_resolution_scale(&mut self, resolution_scale: f32) {
		self.resolution_scale = resolution_scale.clamp(0.25, 1.0);
	}

	//==============================================================================
	/// HDR 출력 설정. (true 면 선 색을 선형으로 바꿔 섞는다, ToonRenderer::set_hdr_output 과 맞춘다)
	///
	/// # 인자
	/// - `is_hdr_output`: true 면 선 색을 선형으로 바꿔 섞는다.
	//==============================================================================
	pub fn set_hdr_output(&mut self, is_hdr_output: bool) {
		self.is_hdr_output = is_hdr_output;
	}

	//==============================================================================
	/// 이번 프레임의 선 굵기. ([바깥 선, 꺾임 선] 의 그리는 영역 픽셀, end_line_ids 뒤에 정해짐, 긋지 않으면 0)
	///
	/// # 돌려줌
	/// [바깥 선 굵기, 꺾임 선 굵기]. (긋지 않으면 0)
	//==============================================================================
	pub fn get_line_widths(&self) -> [f32; 2] {
		self.line_widths
	}

	//==============================================================================
	/// 선의 판 반환. (없으면 None, 번호와 깊이와 씨앗 텍스처, 크기)
	///
	/// # 돌려줌
	/// 선의 판. (노멀과 번호 텍스처, 깊이 텍스처, 씨앗 텍스처, 넓히기 판 둘과 마친 쪽, 크기, 아직 짓지 않았거나 짓지 못했으면 None)
	//==============================================================================
	pub fn get_line_target(&self) -> Option<&ToonLineTarget> {
		self.line_target.as_ref()
	}

	//==============================================================================
	/// 선 굵기 셈. ([바깥 선, 꺾임 선] 의 그리는 영역 픽셀, 번호를 그리는 프레임이 없으면 지금 값)
	///
	/// # 돌려줌
	/// [바깥 선 굵기, 꺾임 선 굵기].
	//==============================================================================
	pub fn read_line_widths(&self) -> [f32; 2] {
		let frame = match self.frame.as_ref() {
			Some(frame) => {
				frame
			}
			None => {
				return self.line_widths;
			}
		};
		let average_distance = if self.drawn_distance_count > 0 {
			self.drawn_distance_sum / self.drawn_distance_count as f32
		}
		else {
			1.0
		};
		let focus_distance = if self.focus_distance > 0.0 {
			self.focus_distance
		}
		else {
			average_distance
		};
		compute_line_widths([self.width, self.crease_width], self.width_unit, frame.viewport[3], frame.projection_matrix.elements[5].abs(), frame.is_perspective, focus_distance)
	}

	//==============================================================================
	/// 번호 그리기 시작. (지금 뷰포트를 그리는 영역으로 적고, 선의 판을 묶어 비운다, 판을 짓지 못하면 false)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `view_matrix`: 보기 행렬.
	/// - `projection_matrix`: 투영 행렬. (원근이나 직교)
	///
	/// # 돌려줌
	/// 선의 판을 짓지 못했으면 false. (그때 draw_mesh 는 아무것도 하지 않는다)
	//==============================================================================
	pub fn begin_line_ids(&mut self, gl: &glow::Context, view_matrix: &Matrix4, projection_matrix: &Matrix4) -> bool {
		let mut viewport = [0i32; 4];
		unsafe {
			gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
		}
		let mut view_projection_matrix = *projection_matrix;
		view_projection_matrix.multiply(view_matrix);
		self.frame = Some(LineFrame {
			view_matrix: *view_matrix,
			projection_matrix: *projection_matrix,
			frustum_planes: extract_frustum_planes(&view_projection_matrix),
			is_perspective: projection_matrix.elements[11].abs() > 0.5,
			viewport,
		});
		self.drawn_distance_sum = 0.0;
		self.drawn_distance_count = 0;
		let width = viewport[2].max(1);
		let height = viewport[3].max(1);
		let jump_flood_width = ((width as f32 * self.resolution_scale).ceil() as i32).max(1);
		let jump_flood_height = ((height as f32 * self.resolution_scale).ceil() as i32).max(1);
		let framebuffer = match self.read_line_target(gl, width, height, jump_flood_width, jump_flood_height) {
			Some(line_target) => {
				line_target.framebuffer
			}
			None => {
				return false;
			}
		};
		self.save_state(gl);
		self.is_drawing_line_ids = true;
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.viewport(0, 0, width, height);
			gl.disable(glow::SCISSOR_TEST);
			gl.disable(glow::BLEND);
			gl.clear_color(0.0, 0.0, 0.0, 0.0);
			gl.depth_mask(true);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
			gl.enable(glow::DEPTH_TEST);
			gl.depth_func(glow::LEQUAL);
			gl.enable(glow::CULL_FACE);
			gl.cull_face(glow::BACK);
			gl.front_face(glow::CCW);
			self.line_program.use_program(gl);
			gl.uniform_matrix_4_f32_slice(self.line_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_matrix.elements);
		}
		true
	}

	//==============================================================================
	/// 모형 하나의 번호 그리기. (ToonRenderer::draw_mesh 와 같은 행렬과 자세, 번호 1 ~ TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID, 같은 번호끼리 맞닿은 곳은 긋지 않음, 화면 밖이면 건너뛴다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `mesh`: ToonRenderer::create_mesh 의 결과.
	/// - `model_matrix`: 모형 행렬.
	/// - `line_id`: 1 ~ TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID. (0 은 빈 곳이라 쓰지 않는다, 같은 번호끼리 맞닿은 곳은 긋지 않는다)
	/// - `options`: 그리기 꼴. (본 행렬, 거르기, 몸 번호 버퍼의 일부, 감기 뒤집기, 바깥 선과 꺾임 선을 낼지)
	//==============================================================================
	pub fn draw_mesh(&mut self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, line_id: u32, options: ToonLineDrawOptions) {
		if !self.is_drawing_line_ids {
			return;
		}
		if options.is_culled && !self.is_visible(mesh, model_matrix) {
			return;
		}
		self.add_drawn_distance(mesh, model_matrix);
		let is_mirrored = options.is_mirrored.unwrap_or_else(|| read_matrix_determinant(model_matrix) < 0.0);
		self.draw_with_count(gl, mesh, model_matrix, 0, line_id, false, is_mirrored, options);
	}

	//==============================================================================
	/// 같은 모형 여럿의 번호를 한 번에 그리기. (인스턴스에 line_id 가 있으면 그 번호, 없으면 공통 번호, 꼴은 모든 인스턴스에 같다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `mesh`: ToonRenderer::create_mesh 의 결과.
	/// - `instance_list`: 인스턴스 목록. (모형 행렬과 번호, 번호가 None 이면 공통 번호)
	/// - `line_id`: 공통 번호.
	/// - `options`: draw_mesh 와 같다. (꼴은 모든 인스턴스에 같다, is_mirrored 가 None 이면 뒤집지 않는다)
	//==============================================================================
	pub fn draw_mesh_instanced(&mut self, gl: &glow::Context, mesh: &ToonMesh, instance_list: &[ToonInstance], line_id: u32, options: ToonLineDrawOptions) {
		if !self.is_drawing_line_ids {
			return;
		}
		let mut instance_data = std::mem::take(&mut self.instance_data);
		instance_data.clear();
		let mut instance_count = 0;
		for instance in instance_list {
			if options.is_culled && !self.is_visible(mesh, &instance.model_matrix) {
				continue;
			}
			self.add_drawn_distance(mesh, &instance.model_matrix);
			let instance_line_id = instance.line_id.unwrap_or(line_id);
			instance_data.extend_from_slice(&instance.model_matrix.elements);
			instance_data.extend_from_slice(&[instance_line_id as f32, 0.0, 0.0, 1.0]);
			instance_count += 1;
		}
		if instance_count > 0 {
			debug_assert_eq!(instance_data.len(), instance_count * TOON_INSTANCE_FLOAT_COUNT);
			unsafe {
				gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.get_instance_buffer()));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&instance_data), glow::DYNAMIC_DRAW);
			}
			self.draw_with_count(gl, mesh, &Matrix4::create_identity(), instance_count as i32, line_id, true, options.is_mirrored == Some(true), options);
		}
		self.instance_data = instance_data;
	}

	//==============================================================================
	/// 번호 그리기 본체. (instance_count 가 0 이면 보통 그리기)
	//==============================================================================
	#[allow(clippy::too_many_arguments)]
	fn draw_with_count(&self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, instance_count: i32, line_id: u32, is_instance_line_id: bool, is_mirrored: bool, options: ToonLineDrawOptions) {
		let line_program = &self.line_program;
		let is_skinned = mesh.is_skinned() && options.joint_matrices.is_some();
		let (body_vertex_array, body_index_count) = mesh.get_body();
		let index_start = options.index_start.min(body_index_count as usize);
		let remaining_count = body_index_count as usize - index_start;
		let index_count = options.index_count.unwrap_or(remaining_count).min(remaining_count);
		let line_flags = if options.has_crease_line { 1.0 } else { 0.0 } + if options.has_screen_outline { 2.0 } else { 0.0 };
		unsafe {
			gl.uniform_matrix_4_f32_slice(line_program.get_uniform_location(gl, "modelMatrix").as_ref(), false, &model_matrix.elements);
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "instanced").as_ref(), if instance_count > 0 { 1.0 } else { 0.0 });
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "instanceLineIds").as_ref(), if is_instance_line_id { 1.0 } else { 0.0 });
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "skinned").as_ref(), if is_skinned { 1.0 } else { 0.0 });
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "lineId").as_ref(), line_id.min(TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID) as f32);
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "lineFlags").as_ref(), line_flags);
			if let (true, Some(joint_matrices)) = (is_skinned, options.joint_matrices) {
				let joint_float_count = joint_matrices.len().min(TOON_MAXIMUM_JOINTS * 16) / 16 * 16;
				gl.uniform_matrix_4_f32_slice(line_program.get_uniform_location(gl, "jointMatrices").as_ref(), false, &joint_matrices[..joint_float_count]);
			}
			if index_count == 0 {
				return;
			}
			if is_mirrored {
				gl.front_face(glow::CW);
			}
			gl.bind_vertex_array(Some(body_vertex_array));
			let byte_offset = (index_start * 4) as i32;
			if instance_count > 0 {
				gl.draw_elements_instanced(glow::TRIANGLES, index_count as i32, glow::UNSIGNED_INT, byte_offset, instance_count);
			}
			else {
				gl.draw_elements(glow::TRIANGLES, index_count as i32, glow::UNSIGNED_INT, byte_offset);
			}
			if is_mirrored {
				gl.front_face(glow::CCW);
			}
		}
	}

	//==============================================================================
	/// 번호 그리기 끝. (씨앗을 고르고 점프 플러딩으로 넓힌 뒤, 프레임버퍼와 뷰포트와 상태를 돌린다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn end_line_ids(&mut self, gl: &glow::Context) {
		if !self.is_drawing_line_ids {
			return;
		}
		self.is_drawing_line_ids = false;
		let line_widths = self.read_line_widths();
		self.line_widths = line_widths;
		let (frame, line_target) = match (self.frame, self.line_target.as_mut()) {
			(Some(frame), Some(line_target)) => {
				(frame, line_target)
			}
			_ => {
				self.restore_state(gl);
				return;
			}
		};
		let seed_program = &self.seed_program;
		let jump_flood_program = &self.jump_flood_program;
		let projection = &frame.projection_matrix.elements;
		let view = &frame.view_matrix.elements;
		let mut seed_index = 0usize;
		unsafe {
			gl.bind_vertex_array(Some(self.empty_vertex_array));
			gl.viewport(0, 0, line_target.width, line_target.height);
			gl.disable(glow::DEPTH_TEST);
			gl.depth_mask(false);
			gl.disable(glow::CULL_FACE);
			gl.disable(glow::BLEND);

			// 1. 씨앗 고르기.
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(line_target.seed_framebuffer));
			seed_program.use_program(gl);
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(line_target.line_texture));
			gl.active_texture(glow::TEXTURE0 + DEPTH_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(line_target.depth_texture));
			let location = |name: &str| seed_program.get_uniform_location(gl, name);
			gl.uniform_1_i32(location("lineTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("depthTexture").as_ref(), DEPTH_TEXTURE_UNIT as i32);
			gl.uniform_matrix_3_f32_slice(location("viewRotation").as_ref(), false, &[view[0], view[1], view[2], view[4], view[5], view[6], view[8], view[9], view[10]]);
			gl.uniform_4_f32(location("projectionScale").as_ref(), projection[0], projection[5], projection[8], projection[9]);
			gl.uniform_4_f32(location("projectionDepth").as_ref(), projection[10], projection[14], projection[12], projection[13]);
			gl.uniform_1_f32(location("isPerspective").as_ref(), if frame.is_perspective { 1.0 } else { 0.0 });
			gl.uniform_1_f32(location("depthThreshold").as_ref(), self.depth_threshold);
			gl.uniform_1_f32(location("creaseCosine").as_ref(), if line_widths[1] > 0.0 { self.crease_cosine } else { -2.0 });
			gl.draw_arrays(glow::TRIANGLES, 0, 3);
			gl.active_texture(glow::TEXTURE0 + DEPTH_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, None);

			// 2. 넓히기 판으로 모으기. (줄인 판이면 칸마다 가장 가까운 씨앗)
			let point_scale_x = line_target.width as f32 / line_target.jump_flood_width as f32;
			let point_scale_y = line_target.height as f32 / line_target.jump_flood_height as f32;
			let seed_gather_program = &self.seed_gather_program;
			gl.viewport(0, 0, line_target.jump_flood_width, line_target.jump_flood_height);
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(line_target.jump_flood_framebuffers[0]));
			seed_gather_program.use_program(gl);
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(line_target.seed_texture));
			gl.uniform_1_i32(seed_gather_program.get_uniform_location(gl, "seedTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_2_f32(seed_gather_program.get_uniform_location(gl, "pointScale").as_ref(), point_scale_x, point_scale_y);
			gl.draw_arrays(glow::TRIANGLES, 0, 3);

			// 3. 점프 플러딩. (그리는 영역 굵기의 반과 덮는 몫의 반 픽셀을 넓히기 판 칸으로, 섞을 때 보는 2x2 칸만큼 더)
			jump_flood_program.use_program(gl);
			gl.uniform_1_i32(jump_flood_program.get_uniform_location(gl, "seedTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_2_f32(jump_flood_program.get_uniform_location(gl, "pointScale").as_ref(), point_scale_x, point_scale_y);
			let reach = (line_widths[0].max(line_widths[1]) * 0.5 + 0.5) / point_scale_x.min(point_scale_y) + 1.5;
			for step_size in read_jump_flood_steps(reach) {
				gl.bind_framebuffer(glow::FRAMEBUFFER, Some(line_target.jump_flood_framebuffers[1 - seed_index]));
				gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
				gl.bind_texture(glow::TEXTURE_2D, Some(line_target.jump_flood_textures[seed_index]));
				gl.uniform_1_i32(jump_flood_program.get_uniform_location(gl, "stepSize").as_ref(), step_size);
				gl.draw_arrays(glow::TRIANGLES, 0, 3);
				seed_index = 1 - seed_index;
			}
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_vertex_array(None);
		}
		line_target.jump_flood_index = seed_index;
		self.restore_state(gl);
	}

	//==============================================================================
	/// 바깥선 긋기. (넓힌 씨앗 판으로 begin_line_ids 때의 그리는 영역에 섞는다, 깊이는 보지도 쓰지도 않는다, 스텐실은 그대로)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn draw_outline(&mut self, gl: &glow::Context) {
		let (frame, seed_texture, point_scale) = match (self.frame, self.line_target.as_ref()) {
			(Some(frame), Some(line_target)) => {
				(frame, line_target.jump_flood_textures[line_target.jump_flood_index], [line_target.width as f32 / line_target.jump_flood_width as f32, line_target.height as f32 / line_target.jump_flood_height as f32])
			}
			_ => {
				return;
			}
		};
		self.save_state(gl);
		let composite_program = &self.composite_program;
		let viewport = frame.viewport;
		let color = self.color;
		let line_widths = self.line_widths;
		unsafe {
			gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
			gl.disable(glow::DEPTH_TEST);
			gl.depth_mask(false);
			gl.disable(glow::CULL_FACE);
			gl.enable(glow::BLEND);
			gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			composite_program.use_program(gl);
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(seed_texture));
			gl.active_texture(glow::TEXTURE0);
			let location = |name: &str| composite_program.get_uniform_location(gl, name);
			gl.uniform_1_i32(location("seedTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_4_f32(location("viewportRect").as_ref(), viewport[0] as f32, viewport[1] as f32, viewport[2].max(1) as f32, viewport[3].max(1) as f32);
			gl.uniform_2_f32(location("pointScale").as_ref(), point_scale[0], point_scale[1]);
			gl.uniform_2_f32(location("lineWidths").as_ref(), line_widths[0], line_widths[1]);
			gl.uniform_1_f32(location("creaseOpacity").as_ref(), self.crease_opacity);
			gl.uniform_4_f32(location("outlineColor").as_ref(), color[0], color[1], color[2], color[3]);
			gl.uniform_1_f32(location("hdrOutput").as_ref(), if self.is_hdr_output { 1.0 } else { 0.0 });
			gl.bind_vertex_array(Some(self.empty_vertex_array));
			gl.draw_arrays(glow::TRIANGLES, 0, 3);
			gl.bind_vertex_array(None);
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}
		self.restore_state(gl);
	}

	//==============================================================================
	/// 그린 모형의 거리 더하기. (초점 거리를 주지 않은 원근에서 쓴다, 보기 공간의 앞쪽 거리)
	//==============================================================================
	fn add_drawn_distance(&mut self, mesh: &ToonMesh, model_matrix: &Matrix4) {
		let frame = match self.frame.as_ref() {
			Some(frame) => {
				frame
			}
			None => {
				return;
			}
		};
		let (center, _) = mesh.get_bounding_sphere();
		let world_point = model_matrix.transform_point(center[0], center[1], center[2]);
		let view_point = frame.view_matrix.transform_point(world_point.x, world_point.y, world_point.z);
		if view_point.z < 0.0 {
			self.drawn_distance_sum += -view_point.z;
			self.drawn_distance_count += 1;
		}
	}

	//==============================================================================
	/// 경계 구가 이번 프레임의 절두체 안에 드는지.
	//==============================================================================
	fn is_visible(&self, mesh: &ToonMesh, model_matrix: &Matrix4) -> bool {
		let frame = match self.frame.as_ref() {
			Some(frame) => {
				frame
			}
			None => {
				return false;
			}
		};
		let (center, bounding_radius) = mesh.get_bounding_sphere();
		let transformed_center = model_matrix.transform_point(center[0], center[1], center[2]);
		let elements = &model_matrix.elements;
		let scale_x = (elements[0] * elements[0] + elements[1] * elements[1] + elements[2] * elements[2]).sqrt();
		let scale_y = (elements[4] * elements[4] + elements[5] * elements[5] + elements[6] * elements[6]).sqrt();
		let scale_z = (elements[8] * elements[8] + elements[9] * elements[9] + elements[10] * elements[10]).sqrt();
		let radius = bounding_radius * scale_x.max(scale_y).max(scale_z);
		frame.frustum_planes.iter().all(|plane| plane[0] * transformed_center.x + plane[1] * transformed_center.y + plane[2] * transformed_center.z + plane[3] >= -radius)
	}

	//==============================================================================
	/// 선의 판 읽기. (크기가 바뀌면 다시 지음, 짓지 못하면 None)
	//==============================================================================
	fn read_line_target(&mut self, gl: &glow::Context, width: i32, height: i32, jump_flood_width: i32, jump_flood_height: i32) -> Option<&ToonLineTarget> {
		let is_same_size = matches!(self.line_target.as_ref(), Some(line_target) if line_target.width == width && line_target.height == height && line_target.jump_flood_width == jump_flood_width && line_target.jump_flood_height == jump_flood_height);
		if !is_same_size {
			self.delete_line_target(gl);
			self.line_target = ToonScreenOutline::create_line_target(gl, width, height, jump_flood_width, jump_flood_height);
		}
		self.line_target.as_ref()
	}

	//==============================================================================
	/// 선의 판 짓기. (그리는 영역 해상도의 노멀과 번호 RGBA8, 깊이 24, 씨앗 RGBA16UI, 넓히기 판 RGBA16UI 둘, 앞의 프레임버퍼와 텍스처는 돌린다)
	//==============================================================================
	fn create_line_target(gl: &glow::Context, width: i32, height: i32, jump_flood_width: i32, jump_flood_height: i32) -> Option<ToonLineTarget> {
		unsafe {
			let previous_framebuffer = crate::graphics3d::read_framebuffer_binding(gl);
			let previous_texture = crate::graphics3d::read_texture_binding(gl);
			let create_texture = |internal_format: u32, format: u32, data_type: u32, texture_width: i32, texture_height: i32| -> Option<GlTexture> {
				let texture = gl.create_texture().ok()?;
				gl.bind_texture(glow::TEXTURE_2D, Some(texture));
				gl.tex_image_2d(glow::TEXTURE_2D, 0, internal_format as i32, texture_width, texture_height, 0, format, data_type, glow::PixelUnpackData::Slice(None));
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
				Some(texture)
			};
			let line_texture = create_texture(glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, width, height)?;
			let depth_texture = create_texture(glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, width, height)?;
			let seed_texture = create_texture(glow::RGBA16UI, glow::RGBA_INTEGER, glow::UNSIGNED_SHORT, width, height)?;
			let jump_flood_textures = [create_texture(glow::RGBA16UI, glow::RGBA_INTEGER, glow::UNSIGNED_SHORT, jump_flood_width, jump_flood_height)?, create_texture(glow::RGBA16UI, glow::RGBA_INTEGER, glow::UNSIGNED_SHORT, jump_flood_width, jump_flood_height)?];
			gl.bind_texture(glow::TEXTURE_2D, previous_texture);
			let create_framebuffer = |color_texture: GlTexture, depth_texture: Option<GlTexture>| -> Option<(GlFramebuffer, bool)> {
				let framebuffer = gl.create_framebuffer().ok()?;
				gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
				gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
				if depth_texture.is_some() {
					gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, depth_texture, 0);
				}
				Some((framebuffer, gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE))
			};
			let (framebuffer, is_line_complete) = create_framebuffer(line_texture, Some(depth_texture))?;
			let (seed_framebuffer, is_seed_complete) = create_framebuffer(seed_texture, None)?;
			let (first_jump_flood_framebuffer, is_first_complete) = create_framebuffer(jump_flood_textures[0], None)?;
			let (second_jump_flood_framebuffer, is_second_complete) = create_framebuffer(jump_flood_textures[1], None)?;
			gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
			let line_target = ToonLineTarget {
				framebuffer,
				line_texture,
				depth_texture,
				seed_framebuffer,
				seed_texture,
				jump_flood_framebuffers: [first_jump_flood_framebuffer, second_jump_flood_framebuffer],
				jump_flood_textures,
				jump_flood_index: 0,
				width,
				height,
				jump_flood_width,
				jump_flood_height,
			};
			if !is_line_complete || !is_seed_complete || !is_first_complete || !is_second_complete {
				ToonScreenOutline::delete_target_objects(gl, &line_target);
				return None;
			}
			Some(line_target)
		}
	}

	//==============================================================================
	/// 선의 판 지우기.
	//==============================================================================
	fn delete_line_target(&mut self, gl: &glow::Context) {
		if let Some(line_target) = self.line_target.take() {
			ToonScreenOutline::delete_target_objects(gl, &line_target);
		}
	}

	//==============================================================================
	/// 선의 판의 GL 객체 지우기.
	//==============================================================================
	fn delete_target_objects(gl: &glow::Context, line_target: &ToonLineTarget) {
		unsafe {
			gl.delete_framebuffer(line_target.framebuffer);
			gl.delete_framebuffer(line_target.seed_framebuffer);
			for jump_flood_framebuffer in line_target.jump_flood_framebuffers {
				gl.delete_framebuffer(jump_flood_framebuffer);
			}
			gl.delete_texture(line_target.line_texture);
			gl.delete_texture(line_target.depth_texture);
			gl.delete_texture(line_target.seed_texture);
			for jump_flood_texture in line_target.jump_flood_textures {
				gl.delete_texture(jump_flood_texture);
			}
		}
	}

	//==============================================================================
	/// 그리기 상태 적어 두기.
	//==============================================================================
	fn save_state(&mut self, gl: &glow::Context) {
		unsafe {
			let mut viewport = [0i32; 4];
			gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
			let mut clear_color = [0.0f32; 4];
			gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut clear_color);
			self.saved_state = Some(SavedState {
				is_blend_enabled: gl.is_enabled(glow::BLEND),
				is_depth_test_enabled: gl.is_enabled(glow::DEPTH_TEST),
				is_cull_face_enabled: gl.is_enabled(glow::CULL_FACE),
				is_scissor_test_enabled: gl.is_enabled(glow::SCISSOR_TEST),
				depth_mask: gl.get_parameter_bool(glow::DEPTH_WRITEMASK),
				depth_function: gl.get_parameter_i32(glow::DEPTH_FUNC),
				front_face: gl.get_parameter_i32(glow::FRONT_FACE),
				blend_source_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
				blend_destination_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
				blend_source_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
				blend_destination_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
				clear_color,
				program: crate::graphics3d::read_program_binding(gl),
				vertex_array: crate::graphics3d::read_vertex_array_binding(gl),
				viewport,
				framebuffer: crate::graphics3d::read_framebuffer_binding(gl),
			});
		}
	}

	//==============================================================================
	/// 적어 둔 상태로 돌리기.
	//==============================================================================
	fn restore_state(&mut self, gl: &glow::Context) {
		let saved_state = match self.saved_state.take() {
			Some(saved_state) => {
				saved_state
			}
			None => {
				return;
			}
		};
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, saved_state.framebuffer);
			gl.viewport(saved_state.viewport[0], saved_state.viewport[1], saved_state.viewport[2], saved_state.viewport[3]);
			gl.clear_color(saved_state.clear_color[0], saved_state.clear_color[1], saved_state.clear_color[2], saved_state.clear_color[3]);
			gl.bind_vertex_array(saved_state.vertex_array);
			gl.use_program(saved_state.program);
			gl.depth_mask(saved_state.depth_mask);
			gl.depth_func(saved_state.depth_function as u32);
			gl.front_face(saved_state.front_face as u32);
			gl.cull_face(glow::BACK);
			gl.blend_func_separate(saved_state.blend_source_rgb as u32, saved_state.blend_destination_rgb as u32, saved_state.blend_source_alpha as u32, saved_state.blend_destination_alpha as u32);
			for (capability, is_enabled) in [(glow::BLEND, saved_state.is_blend_enabled), (glow::DEPTH_TEST, saved_state.is_depth_test_enabled), (glow::CULL_FACE, saved_state.is_cull_face_enabled), (glow::SCISSOR_TEST, saved_state.is_scissor_test_enabled)] {
				if is_enabled {
					gl.enable(capability);
				}
				else {
					gl.disable(capability);
				}
			}
		}
	}

	//==============================================================================
	/// 다 쓴 뒤 지우기. (선의 판, 버텍스 어레이, 프로그램 다섯)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.delete_line_target(gl);
		unsafe {
			gl.delete_vertex_array(self.empty_vertex_array);
			gl.delete_program(self.line_program.get_program());
			gl.delete_program(self.seed_program.get_program());
			gl.delete_program(self.seed_gather_program.get_program());
			gl.delete_program(self.jump_flood_program.get_program());
			gl.delete_program(self.composite_program.get_program());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 점프 플러딩 걸음이 바라는 거리를 덮는지.
	//==============================================================================
	#[test]
	fn jump_flood_steps_cover_reach() {
		assert_eq!(read_jump_flood_steps(0.5), vec![1]);
		assert_eq!(read_jump_flood_steps(1.0), vec![1]);
		assert_eq!(read_jump_flood_steps(2.0), vec![2, 1]);
		assert_eq!(read_jump_flood_steps(4.0), vec![4, 2, 1]);
		assert_eq!(read_jump_flood_steps(10.0), vec![8, 4, 2, 1]);
		for reach in 1..200 {
			let steps = read_jump_flood_steps(reach as f32);
			let covered: i32 = steps.iter().sum();
			assert!(covered >= reach);
		}
	}

	//==============================================================================
	/// 선 굵기가 단위를 따르고 한 픽셀 밑으로 내려가지 않는지. (원근의 월드 굵기는 거리에 반비례)
	//==============================================================================
	#[test]
	fn line_widths_follow_unit_and_minimum() {
		assert_eq!(compute_line_widths([3.0, 1.0], ToonOutlineWidthUnit::Pixel, 720, 1.0, false, 0.0), [3.0, 1.0]);
		assert_eq!(compute_line_widths([0.5, 0.0], ToonOutlineWidthUnit::Pixel, 720, 1.0, false, 0.0), [1.0, 0.0]);
		let orthographic_widths = compute_line_widths([0.03, 0.01], ToonOutlineWidthUnit::World, 720, 2.0 / 8.1, false, 0.0);
		assert!((orthographic_widths[0] - 0.03 * 360.0 * 2.0 / 8.1).abs() < 0.001);
		let near_widths = compute_line_widths([0.022, 0.0], ToonOutlineWidthUnit::World, 720, 2.0, true, 2.0);
		let far_widths = compute_line_widths([0.022, 0.0], ToonOutlineWidthUnit::World, 720, 2.0, true, 4.0);
		assert!((near_widths[0] - far_widths[0] * 2.0).abs() < 0.001);
	}

	//==============================================================================
	/// 셰이더가 원본과 같은 꼴인지. (꼴 비트, 씨앗 출력, 걸음 유니폼, 덮는 몫 셈)
	//==============================================================================
	#[test]
	fn shaders_match_original_layout() {
		assert!(LINE_FRAGMENT_SHADER.contains("lineFlags * 64.0"));
		assert!(SEED_FRAGMENT_SHADER.contains("out uvec4 fragmentSeed"));
		assert!(JUMP_FLOOD_FRAGMENT_SHADER.contains("uniform int stepSize"));
		assert!(COMPOSITE_FRAGMENT_SHADER.contains("lineWidths.x * 0.5 - contourDistance + 0.5"));
	}
}

// @shaders-begin
const LINE_VERTEX_SHADER: &str = "#version 300 es
in vec3 vertexPosition;
in vec3 vertexNormal;
in vec4 vertexJoints;
in vec4 vertexWeights;
in vec4 instanceMatrix0;
in vec4 instanceMatrix1;
in vec4 instanceMatrix2;
in vec4 instanceMatrix3;
in vec4 instanceTint;
uniform mat4 jointMatrices[96];
uniform float skinned;
uniform float instanced;
uniform float instanceLineIds;
uniform mat4 viewProjectionMatrix;
uniform mat4 modelMatrix;
uniform float lineId;
out vec3 worldNormal;
flat out float surfaceLineId;
mat4 readSkinMatrix() {
	if (skinned < 0.5) {
		return mat4(1.0);
	}
	return jointMatrices[int(vertexJoints.x)] * vertexWeights.x + jointMatrices[int(vertexJoints.y)] * vertexWeights.y + jointMatrices[int(vertexJoints.z)] * vertexWeights.z + jointMatrices[int(vertexJoints.w)] * vertexWeights.w;
}
void main() {
	mat4 skinMatrix = readSkinMatrix();
	mat4 objectMatrix = instanced > 0.5 ? mat4(instanceMatrix0, instanceMatrix1, instanceMatrix2, instanceMatrix3) : modelMatrix;
	worldNormal = transpose(inverse(mat3(objectMatrix) * mat3(skinMatrix))) * vertexNormal;
	surfaceLineId = instanced > 0.5 && instanceLineIds > 0.5 ? instanceTint.x : lineId;
	gl_Position = viewProjectionMatrix * objectMatrix * skinMatrix * vec4(vertexPosition, 1.0);
}
";

const LINE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec3 worldNormal;
flat in float surfaceLineId;
uniform float lineFlags;
out vec4 fragmentLine;
vec2 wrapOctahedron(vec2 value) {
	return (1.0 - abs(value.yx)) * vec2(value.x >= 0.0 ? 1.0 : -1.0, value.y >= 0.0 ? 1.0 : -1.0);
}
void main() {
	vec3 normal = normalize(worldNormal);
	normal /= abs(normal.x) + abs(normal.y) + abs(normal.z);
	vec2 folded = normal.y >= 0.0 ? normal.xz : wrapOctahedron(normal.xz);
	float lowPart = mod(surfaceLineId, 256.0);
	float highPart = mod(floor(surfaceLineId / 256.0), 64.0);
	fragmentLine = vec4(folded * 0.5 + 0.5, lowPart / 255.0, (highPart + lineFlags * 64.0) / 255.0);
}
";

const SCREEN_TRIANGLE_VERTEX_SHADER: &str = "#version 300 es
const vec2 TRIANGLE_POINTS[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
void main() {
	gl_Position = vec4(TRIANGLE_POINTS[gl_VertexID], 0.0, 1.0);
}
";

const SEED_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D lineTexture;
uniform highp sampler2D depthTexture;
uniform mat3 viewRotation;
uniform vec4 projectionScale;
uniform vec4 projectionDepth;
uniform float isPerspective;
uniform float depthThreshold;
uniform float creaseCosine;
out uvec4 fragmentSeed;
const uint NO_SEED = 65535u;
const ivec2 SIDES[4] = ivec2[4](ivec2(1, 0), ivec2(-1, 0), ivec2(0, 1), ivec2(0, -1));
struct LinePixel {
	float id;
	float flags;
	vec3 normal;
	vec3 position;
	float footprint;
};
vec3 readNormal(vec2 encoded) {
	vec2 folded = encoded * 2.0 - 1.0;
	vec3 normal = vec3(folded.x, 1.0 - abs(folded.x) - abs(folded.y), folded.y);
	float fold = max(-normal.y, 0.0);
	normal.x += normal.x >= 0.0 ? -fold : fold;
	normal.z += normal.z >= 0.0 ? -fold : fold;
	return normalize(normal);
}
LinePixel readPixel(ivec2 pixel, vec2 size) {
	vec4 value = texelFetch(lineTexture, pixel, 0);
	float highPart = floor(value.a * 255.0 + 0.5);
	LinePixel result;
	result.id = floor(value.b * 255.0 + 0.5) + mod(highPart, 64.0) * 256.0;
	result.flags = floor(highPart / 64.0);
	result.normal = normalize(viewRotation * readNormal(value.rg));
	float clipDepth = texelFetch(depthTexture, pixel, 0).r * 2.0 - 1.0;
	vec2 clipPoint = (vec2(pixel) + 0.5) / size * 2.0 - 1.0;
	if (isPerspective > 0.5) {
		float viewZ = -projectionDepth.y / (clipDepth + projectionDepth.x);
		result.position = vec3(-viewZ * (clipPoint.x + projectionScale.z) / projectionScale.x, -viewZ * (clipPoint.y + projectionScale.w) / projectionScale.y, viewZ);
		result.footprint = -viewZ * 2.0 / (projectionScale.y * size.y);
	}
	else {
		float viewZ = (clipDepth - projectionDepth.y) / projectionDepth.x;
		result.position = vec3((clipPoint.x - projectionDepth.z) / projectionScale.x, (clipPoint.y - projectionDepth.w) / projectionScale.y, viewZ);
		result.footprint = 2.0 / (projectionScale.y * size.y);
	}
	return result;
}
bool hasContour(float flags) {
	return flags >= 1.5;
}
bool hasCrease(float flags) {
	return mod(flags, 2.0) > 0.5;
}
int readEdge(LinePixel first, LinePixel second) {
	if (first.id < 0.5 && second.id < 0.5) {
		return 0;
	}
	bool isFirstNearer = second.id < 0.5 || (first.id > 0.5 && first.position.z >= second.position.z);
	float nearFlags = isFirstNearer ? first.flags : second.flags;
	float farId = isFirstNearer ? second.id : first.id;
	if (farId < 0.5) {
		return hasContour(nearFlags) ? 2 : 0;
	}
	vec3 offset = second.position - first.position;
	float planeDistance = min(abs(dot(offset, first.normal)), abs(dot(offset, second.normal)));
	bool isSeparated = planeDistance > depthThreshold + 2.0 * max(first.footprint, second.footprint);
	if (isSeparated) {
		return hasContour(nearFlags) ? 2 : 0;
	}
	if (abs(first.id - second.id) > 0.5) {
		return hasContour(first.flags) || hasContour(second.flags) ? 2 : 0;
	}
	if (hasCrease(nearFlags) && dot(first.normal, second.normal) < creaseCosine) {
		return 1;
	}
	return 0;
}
void main() {
	ivec2 size = textureSize(lineTexture, 0);
	vec2 sizeValue = vec2(size);
	ivec2 pixel = ivec2(gl_FragCoord.xy);
	LinePixel center = readPixel(pixel, sizeValue);
	uvec2 contourSeed = uvec2(NO_SEED);
	uvec2 creaseSeed = uvec2(NO_SEED);
	for (int index = 0; index < 4; ++index) {
		ivec2 neighborPixel = pixel + SIDES[index];
		if (neighborPixel.x < 0 || neighborPixel.y < 0 || neighborPixel.x >= size.x || neighborPixel.y >= size.y) {
			continue;
		}
		int edge = readEdge(center, readPixel(neighborPixel, sizeValue));
		uvec2 seed = uvec2(pixel * 2 + SIDES[index]);
		if (edge == 2 && contourSeed.x == NO_SEED) {
			contourSeed = seed;
		}
		if (edge == 1 && creaseSeed.x == NO_SEED) {
			creaseSeed = seed;
		}
	}
	fragmentSeed = uvec4(contourSeed, creaseSeed);
}
";

const SEED_GATHER_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp usampler2D seedTexture;
uniform vec2 pointScale;
out uvec4 fragmentSeed;
const uint NO_SEED = 65535u;
float readSeedDistance(vec2 point, uvec2 seed) {
	if (seed.x == NO_SEED) {
		return 1.0e9;
	}
	vec2 offset = abs(point - vec2(seed) * 0.5);
	if ((seed.x & 1u) == 1u) {
		offset.y = max(offset.y - 0.5, 0.0);
	}
	else {
		offset.x = max(offset.x - 0.5, 0.0);
	}
	return length(offset);
}
void main() {
	ivec2 size = textureSize(seedTexture, 0);
	ivec2 pixel = ivec2(gl_FragCoord.xy);
	vec2 point = (vec2(pixel) + 0.5) * pointScale - 0.5;
	ivec2 start = ivec2(floor(vec2(pixel) * pointScale));
	ivec2 end = min(ivec2(ceil((vec2(pixel) + 1.0) * pointScale)), size);
	uvec4 best = uvec4(NO_SEED);
	float bestContour = 1.0e9;
	float bestCrease = 1.0e9;
	for (int offsetY = 0; offsetY < 5; ++offsetY) {
		for (int offsetX = 0; offsetX < 5; ++offsetX) {
			ivec2 samplePixel = start + ivec2(offsetX, offsetY);
			if (samplePixel.x >= end.x || samplePixel.y >= end.y) {
				continue;
			}
			uvec4 seed = texelFetch(seedTexture, samplePixel, 0);
			float contourDistance = readSeedDistance(point, seed.xy);
			if (contourDistance < bestContour) {
				bestContour = contourDistance;
				best.xy = seed.xy;
			}
			float creaseDistance = readSeedDistance(point, seed.zw);
			if (creaseDistance < bestCrease) {
				bestCrease = creaseDistance;
				best.zw = seed.zw;
			}
		}
	}
	fragmentSeed = best;
}
";

const JUMP_FLOOD_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp usampler2D seedTexture;
uniform int stepSize;
uniform vec2 pointScale;
out uvec4 fragmentSeed;
const uint NO_SEED = 65535u;
float readSeedDistance(vec2 point, uvec2 seed) {
	if (seed.x == NO_SEED) {
		return 1.0e9;
	}
	vec2 offset = abs(point - vec2(seed) * 0.5);
	if ((seed.x & 1u) == 1u) {
		offset.y = max(offset.y - 0.5, 0.0);
	}
	else {
		offset.x = max(offset.x - 0.5, 0.0);
	}
	return length(offset);
}
void main() {
	ivec2 size = textureSize(seedTexture, 0);
	ivec2 pixel = ivec2(gl_FragCoord.xy);
	vec2 point = (vec2(pixel) + 0.5) * pointScale - 0.5;
	uvec4 best = uvec4(NO_SEED);
	float bestContour = 1.0e9;
	float bestCrease = 1.0e9;
	for (int offsetY = -1; offsetY <= 1; ++offsetY) {
		for (int offsetX = -1; offsetX <= 1; ++offsetX) {
			ivec2 samplePixel = pixel + ivec2(offsetX, offsetY) * stepSize;
			if (samplePixel.x < 0 || samplePixel.y < 0 || samplePixel.x >= size.x || samplePixel.y >= size.y) {
				continue;
			}
			uvec4 seed = texelFetch(seedTexture, samplePixel, 0);
			float contourDistance = readSeedDistance(point, seed.xy);
			if (contourDistance < bestContour) {
				bestContour = contourDistance;
				best.xy = seed.xy;
			}
			float creaseDistance = readSeedDistance(point, seed.zw);
			if (creaseDistance < bestCrease) {
				bestCrease = creaseDistance;
				best.zw = seed.zw;
			}
		}
	}
	fragmentSeed = best;
}
";

const COMPOSITE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp usampler2D seedTexture;
uniform vec4 viewportRect;
uniform vec2 pointScale;
uniform vec2 lineWidths;
uniform float creaseOpacity;
uniform vec4 outlineColor;
uniform float hdrOutput;
out vec4 fragmentColor;
const uint NO_SEED = 65535u;
float readSeedDistance(vec2 point, uvec2 seed, vec2 toOutput) {
	if (seed.x == NO_SEED) {
		return 1.0e9;
	}
	vec2 offset = abs(point - vec2(seed) * 0.5);
	if ((seed.x & 1u) == 1u) {
		offset.y = max(offset.y - 0.5, 0.0);
	}
	else {
		offset.x = max(offset.x - 0.5, 0.0);
	}
	return length(offset * toOutput);
}
void main() {
	ivec2 size = textureSize(seedTexture, 0);
	vec2 toOutput = vec2(1.0);
	vec2 point = gl_FragCoord.xy - viewportRect.xy - 0.5;
	ivec2 basePixel = ivec2(floor((point + 0.5) / pointScale - 0.5));
	float contourDistance = 1.0e9;
	float creaseDistance = 1.0e9;
	for (int offsetY = 0; offsetY <= 1; ++offsetY) {
		for (int offsetX = 0; offsetX <= 1; ++offsetX) {
			ivec2 samplePixel = clamp(basePixel + ivec2(offsetX, offsetY), ivec2(0), size - 1);
			uvec4 seed = texelFetch(seedTexture, samplePixel, 0);
			contourDistance = min(contourDistance, readSeedDistance(point, seed.xy, toOutput));
			creaseDistance = min(creaseDistance, readSeedDistance(point, seed.zw, toOutput));
		}
	}
	float contour = lineWidths.x > 0.0 ? clamp(lineWidths.x * 0.5 - contourDistance + 0.5, 0.0, 1.0) : 0.0;
	float crease = lineWidths.y > 0.0 ? clamp(lineWidths.y * 0.5 - creaseDistance + 0.5, 0.0, 1.0) * creaseOpacity : 0.0;
	float strength = max(contour, crease) * outlineColor.a;
	if (strength < 0.004) {
		discard;
	}
	vec3 color = hdrOutput > 0.5 ? pow(outlineColor.rgb, vec3(2.2)) : outlineColor.rgb;
	fragmentColor = vec4(color * strength, strength);
}
";
// @shaders-end
