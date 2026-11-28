//==============================================================================
//! 툰 화면 공간 바깥선. (vanilla.js graphics3d/toon/toonscreenoutline.js 의 ToonScreenOutline)
//! - 노멀, 선 번호, 깊이를 선의 판에 그리고, 바로 이웃 사이의 경계에서 선을 잡아 가는 선은 둘레 칸을 바로 보고, 굵은 선은 점프 플러딩으로 넓힌 뒤 거리로 덮는 몫을 셈해 섞는다.
//! - 선의 판: ToonRenderer 의 create_mesh 결과를 같은 행렬로 한 번 더 그려 노멀(팔면체 접기), 선 번호(14 비트), 꼴(바깥 선, 꺾임 선을 켤지)과 깊이를 쓴다.
//! - 잡기: 화소와 바로 이웃 넷 사이의 경계마다 한 번 본다. 선은 화소가 아니라 화소 사이의 경계 가운데에 선다. (같은 경계를 두 번 긋지 않는다)
//!   바깥 선은 빈 곳과 맞닿은 실루엣, 다른 번호와 맞닿은 곳, 같은 번호라도 가린 화소가 가려진 화소의 접평면에서 깊이 문턱과 오차 여유보다 떨어진 곳(가려진 실루엣)이다.
//!   꺾임 선은 같은 번호 안에서 이웃한 두 노멀이 꺾임 각보다 더 벌어진 곳이다. 화소 하나 사이의 노멀만 보므로 둥근 면의 실루엣 둘레나 비스듬한 면에 띠가 서지 않는다.
//!   깊이를 접평면 거리로 견주므로 비스듬히 보는 바닥에 가짜 선이 서지 않는다. 문턱에 잰 오차(노멀 8 비트 각 오차 x 두 자리 사이 거리, 꺾임의 화소 한 칸 x 사인, 깊이 24 비트 한 칸) x 배율을 더해 먼 것도 같은 뜻이 되고, 나란한 낮은 턱은 화소 한 칸보다 낮아도 잡힌다.
//! - 가는 선(굵기 약 3.24 픽셀까지): 씨앗 고르기와 같은 차례로 화소마다 첫 경계 쪽(R8UI 한 바이트)을 고르고, 섞기가 둘레 3 x 3 화소의 씨앗을 점프 플러딩 마지막 걸음과 같은 차례로 보아 가장 가까운 것까지의 거리를 잰다.
//!   화면 전체 패스가 씨앗 쪽 판 하나와 섞기 하나다. (모바일 타일 GPU 에서 패스 수와 판 대역폭이 굵은 선 길의 몇 분의 1)
//! - 넓히기(굵은 선): 경계 토막까지의 거리를 점프 플러딩으로 퍼뜨린다. 굵기에 상관없이 걸음 수는 log2 이고, 바깥 선과 꺾임 선은 따로 퍼뜨려 굵기를 따로 준다.
//! - 섞기: 그리는 영역의 화소마다 가장 가까운 경계까지의 거리로 clamp(굵기 / 2 - 거리 + 0.5, 0, 1) 만큼 덮는다. (가장자리가 부드럽고, 선의 판을 줄여도 늘인 선이 점선이 되지 않는다)
//! - 모형마다: ToonLineDrawOptions 의 has_screen_outline 이 false 면 그 모형은 선을 내지 않고 뒤의 선만 가린다. has_crease_line 이 false 면 꺾임 선만 끈다.
//! - 번호마다 색: set_line_color 로 번호마다 선 색을 준다. (언리얼 Custom Stencil 꼴, 맞닿은 선은 앞쪽 번호의 색, 표에 없는 번호와 표가 빈 때는 set_color 의 색 그대로)
//! - 굵기: set_width 와 set_crease 의 굵기는 선 하나의 굵기다. (경계를 가운데로 양쪽에 반씩) 단위는 ToonOutlineWidthUnit, 한 픽셀 밑으로 내리지 않는다.
//! - 깊이: 투영 행렬로 직교와 원근을 가리고, 깊이를 보기 공간 자리로 풀어 견준다. (화면 사각형처럼 클립 x, y 를 옮긴 투영도 된다)
//! - 값: set_resolution_scale 로 섞기 앞의 모든 판(선의 판, 가면, 씨앗, 넓히기 판)을 줄인다. (기본 1 은 그리는 영역 그대로, 줄이면 가는 선은 선의 판 해상도에서 덮는 몫 판 하나를 셈해 쌍선형으로 한 번 읽어 섞는다)
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
//! fn draw_outlined_box(gl: &vanilla::GlContext) -> Result<(), String> {
//! 	let mut renderer = ToonRenderer::new(gl)?;
//! 	let mut outline = ToonScreenOutline::new(gl)?;
//! 	outline.set_width(2.0, ToonOutlineWidthUnit::Pixel);
//! 	outline.set_crease(50.0, 1.0, 0.8);
//! 	let mut builder = MeshBuilder::new();
//! 	builder.add_part(&create_box_geometry(1.0, 1.0, 1.0), &Matrix4::create_identity(), "#e0c8a8", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
//! 	let mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("mesh failed")?;
//! 	let view_matrix = Matrix4::create_look_at(Vector3::create(2.0, 2.0, 3.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
//! 	let projection_matrix = Matrix4::create_perspective(std::f64::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
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
use crate::core::glcontext::GlContext;
use std::cell::Cell;
use crate::core::gluniform::GlUniformF64;

use crate::base::matrix4::Matrix4;
use crate::core::graphic::{GlBuffer, GlProgram, GlTexture, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use crate::core::colorspace::{add_color_space_define, read_engine_color_space, ColorSpace};
use crate::graphics3d::toon::toonmaterial::{TOON_INSTANCE_LINE_ID_LOCATION, TOON_INSTANCE_MATRIX_NAME};
use crate::graphics3d::toon::toonrenderer::{create_mesh_instance_vertex_array, is_sphere_in_frustum, read_instance_buffer_sphere, read_matrix_determinant, ToonInstance, ToonInstanceBuffer, ToonMesh, TOON_ATTRIBUTE_LOCATIONS, TOON_INSTANCE_FLOAT_COUNT};
use crate::graphics3d::jointblock::{joint_block_glsl_source, JointBlockBuffer};
use crate::graphics3d::GlFramebuffer;

/// 선 번호의 상한. (선의 판에 14 비트로 쓴다, 0 은 빈 곳)
pub const TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID: u32 = 16383;

/// 묶음 자리마다 버텍스 어레이를 지어 둘 모형 수. (넘으면 그 자리의 것을 모두 지우고 다시, 몸 묶음 자리와 같은 값)
const LINE_BATCH_SLOT_MESH_LIMIT: usize = 8;

/// 선의 판과 씨앗 판을 묶는 텍스처 유닛. (툰 렌더러의 그림자 유닛 3 과 겹치지 않게, 섞기는 번호 색 표를 4, 선의 판을 6, 깊이를 7 에)
const LINE_TEXTURE_UNIT: u32 = 5;
const DEPTH_TEXTURE_UNIT: u32 = 6;
const LINE_COLOR_TEXTURE_UNIT: u32 = 4;
const COMPOSITE_LINE_TEXTURE_UNIT: u32 = 6;
const COMPOSITE_DEPTH_TEXTURE_UNIT: u32 = 7;

/// 판을 덮어쓰기 앞에 비울 값. (정수 판은 u32, 곱한 색 판은 f32)
const ZERO_CLEAR_VALUE: [u32; 4] = [0; 4];
const ZERO_CLEAR_FLOAT_VALUE: [f32; 4] = [0.0; 4];

/// 가는 선 길의 가장 큰 굵기. (그리는 영역 픽셀, 2 x sqrt(4.5) - 1 = 약 3.24)
/// - 덮는 몫이 0 보다 크려면 거리가 굵기 / 2 + 0.5 보다 작아야 한다. 이 굵기까지는 덮는 토막이 모두 화소 하나 거리(0.5, 0.71)나 두 칸 사이(1.5, 1.58)에 있고,
///   그 토막은 둘레 3 x 3 화소의 첫 경계(씨앗)에 든다. 섞기가 그 아홉을 점프 플러딩 마지막 걸음과 같은 차례로 본다. 이보다 굵으면 점프 플러딩 길로 간다. (광부 키우기 1.7 ~ 2.7, 기본 2)
fn thin_line_maximum_width() -> f64 {
	2.0 * 4.5f64.sqrt() - 1.0
}

/// 번호 색 표 텍스처의 너비. (선 번호 0 ~ TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID 를 128 x 128 칸에 하나씩, 번호 % 128 이 x, 번호 / 128 이 y)
const LINE_COLOR_TABLE_WIDTH: u32 = 128;

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
/// fn draw_floor_and_crystal(gl: &vanilla::GlContext, renderer: &ToonRenderer, outline: &mut ToonScreenOutline) -> Option<()> {
/// 	let part_options = ToonPartOptions { has_outline: false, ..ToonPartOptions::default() };
/// 	let mut floor_builder = MeshBuilder::new();
/// 	floor_builder.add_part(&create_box_geometry(6.0, 0.2, 6.0), &Matrix4::create_translation(0.0, -0.1, 0.0), "#506070", part_options);
/// 	let floor_mesh = renderer.create_mesh(gl, &floor_builder.read_arrays())?;
/// 	let mut crystal_builder = MeshBuilder::new();
/// 	crystal_builder.add_part(&create_octahedron_geometry(0.5), &Matrix4::create_translation(0.0, 0.5, 0.0), "#90d0ff", part_options);
/// 	let crystal_mesh = renderer.create_mesh(gl, &crystal_builder.read_arrays())?;
/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(3.0, 3.0, 4.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	let projection_matrix = Matrix4::create_perspective(std::f64::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
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
/// 이번 프레임의 바깥선 길. (원본 outlinePath 의 "none", "thin", "thinScaled", "jumpFlood")
///
/// # 예제
/// 가는 선 길로 갔는지 읽기
/// ```rust,no_run
/// use vanilla::{ToonOutlinePath, ToonScreenOutline};
///
/// fn is_thin_path(outline: &ToonScreenOutline) -> bool {
/// 	match outline.get_line_target() {
/// 		Some(line_target) => matches!(line_target.outline_path, ToonOutlinePath::Thin | ToonOutlinePath::ThinScaled),
/// 		None => false,
/// 	}
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonOutlinePath {
	/// 긋지 않음. (굵기가 둘 다 0 이라 선의 판만 남겼거나 아직 end_line_ids 전)
	None,
	/// 가는 선. (씨앗 쪽 판 하나와 섞기 하나, 굵기가 약 3.24 선의 판 픽셀 이하)
	Thin,
	/// 줄인 선의 판의 가는 선. (선의 판 해상도에서 덮는 몫 판 하나를 셈하고 쌍선형으로 한 번 읽어 섞는다)
	ThinScaled,
	/// 굵은 선. (씨앗, 모으기, 넓히기 걸음, 섞기)
	JumpFlood,
}

//==============================================================================
/// 선의 판. (선의 판 해상도의 노멀과 번호 RGBA8, 깊이 24, 가는 선의 씨앗 쪽 판 R8UI, 굵은 선을 그을 때 짓는 씨앗 RGBA16UI 와 넓히기 판 RGBA16UI 둘, jump_flood_index 는 넓히기를 마친 쪽)
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
	/// 가는 선의 씨앗 쪽 판 프레임버퍼.
	pub edge_mask_framebuffer: GlFramebuffer,
	/// 가는 선의 씨앗 쪽 판. (R8UI, 아래 3 비트 바깥 선, 위 3 비트 꺾임 선의 첫 경계 쪽 0 없음, 1 오른쪽, 2 왼쪽, 3 위, 4 아래)
	pub edge_mask_texture: GlTexture,
	/// 씨앗 판 프레임버퍼. (굵은 선을 처음 그을 때 짓는다, 그 전에는 None)
	pub seed_framebuffer: Option<GlFramebuffer>,
	/// 씨앗 텍스처. (RGBA16UI, rg 는 바깥 선, ba 는 꺾임 선, 굵은 선을 긋기 전에는 None)
	pub seed_texture: Option<GlTexture>,
	/// 넓히기 판 프레임버퍼 둘. (번갈아 쓴다, 굵은 선을 긋기 전에는 빔)
	pub jump_flood_framebuffers: Vec<GlFramebuffer>,
	/// 넓히기 판 텍스처 둘. (RGBA16UI, 굵은 선을 긋기 전에는 빔)
	pub jump_flood_textures: Vec<GlTexture>,
	/// 넓히기를 마친 쪽. (jump_flood_textures 의 차례, 0 이나 1, 넓히지 않았으면 None, 원본 -1)
	pub jump_flood_index: Option<usize>,
	/// 줄인 선의 판의 가는 선 덮는 몫 프레임버퍼. (줄여 가는 선을 처음 그을 때 짓는다, 그 전에는 None)
	pub coverage_framebuffer: Option<GlFramebuffer>,
	/// 줄인 선의 판의 가는 선 덮는 몫 판. (RGBA8 곱한 색, 쌍선형, 줄여 가는 선을 긋기 전에는 None)
	pub coverage_texture: Option<GlTexture>,
	/// 이번 프레임의 길. (end_line_ids 가 정한다)
	pub outline_path: ToonOutlinePath,
	/// 선의 판 너비. (그리는 영역 x set_resolution_scale 의 몫, 픽셀)
	pub width: i32,
	/// 선의 판 높이. (그리는 영역 x set_resolution_scale 의 몫, 픽셀)
	pub height: i32,
	/// 넓히기 판 너비. (선의 판과 같다, 픽셀)
	pub jump_flood_width: i32,
	/// 넓히기 판 높이. (선의 판과 같다, 픽셀)
	pub jump_flood_height: i32,
}

//==============================================================================
/// 선의 판 자동 인스턴싱 묶음 자리. (원본 #batchSlots 의 칸)
//==============================================================================
struct LineBatchSlot {
	buffer: GlBuffer,
	values: Vec<f32>,
	count: usize,
	/// 모형 번호마다 몸 버텍스 어레이.
	vertex_arrays: Vec<(u64, GlVertexArray)>,
}

//==============================================================================
/// 이번 프레임의 카메라. (원본 #frame)
//==============================================================================
#[derive(Clone, Copy)]
struct LineFrame {
	view_matrix: Matrix4,
	projection_matrix: Matrix4,
	frustum_planes: [[f64; 4]; 6],
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
/// fn create_silhouette_outline(gl: &vanilla::GlContext) -> Result<ToonScreenOutline, String> {
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
	/// 가는 선의 씨앗 쪽 판 고르기.
	edge_mask_program: ShaderProgram,
	/// 가는 선을 한 패스로 섞기.
	thin_composite_program: ShaderProgram,
	/// 줄인 선의 판에서 가는 선의 덮는 몫 셈하기.
	thin_coverage_program: ShaderProgram,
	/// 덮는 몫 판을 쌍선형으로 읽어 섞기.
	coverage_blit_program: ShaderProgram,
	empty_vertex_array: GlVertexArray,
	line_target: Option<ToonLineTarget>,
	color: [f64; 4],
	width: f64,
	width_unit: ToonOutlineWidthUnit,
	focus_distance: f64,
	depth_threshold: f64,
	/// 깊이 문턱에 더하는 오차 여유의 배율. (잰 오차의 몇 배)
	depth_margin: f64,
	crease_cosine: f64,
	crease_width: f64,
	crease_opacity: f64,
	resolution_scale: f64,
	is_hdr_output: bool,
	frame: Option<LineFrame>,
	saved_state: Option<SavedState>,
	is_drawing_line_ids: bool,
	instance_data: Vec<f32>,
	/// 자동 인스턴싱 묶음 자리. (원본 #batchSlots, 그리기 줄이 프레임마다 같은 차례로 주고 값이 같으면 다시 올리지 않는다)
	batch_slots: Vec<LineBatchSlot>,
	/// 선의 판에 올린 instanced, instanceLineIds, skinned, lineFlags. (같으면 건너뜀, begin_line_ids 가 잊는다)
	line_uniform_values: Cell<[f32; 4]>,
	drawn_distance_sum: f64,
	drawn_distance_count: u32,
	line_widths: [f64; 2],
	line_color_table: Vec<u8>,
	line_color_count: u32,
	line_color_texture: Option<GlTexture>,
	/// 뼈 행렬 유니폼 블록의 버퍼. (JointBlock)
	joint_block_buffer: JointBlockBuffer,
	is_line_color_table_dirty: bool,
	/// 색 공간. (원본 #colorSpace, 생성 때 정함, HDR 출력에서 선 색을 선형으로 바꾸는 곡선)
	color_space: ColorSpace,
}

//==============================================================================
/// 화면 공간 바깥선을 지을 때의 꼴. (원본 생성 options)
///
/// # 예제
/// 선형 색 공간으로 짓기
/// ```rust,no_run
/// use vanilla::{ColorSpace, ToonScreenOutline, ToonScreenOutlineOptions};
///
/// fn create_linear_outline(gl: &vanilla::GlContext) -> Result<ToonScreenOutline, String> {
/// 	ToonScreenOutline::new_with_options(gl, ToonScreenOutlineOptions { color_space: ColorSpace::Linear })
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToonScreenOutlineOptions {
	/// 색 공간. (원본 colorSpace, 기본은 엔진의 색 공간, ToonRenderer 와 같은 값)
	pub color_space: ColorSpace,
}

impl Default for ToonScreenOutlineOptions {
	//==============================================================================
	/// 기본값. (색 공간은 엔진의 것)
	//==============================================================================
	fn default() -> Self {
		ToonScreenOutlineOptions { color_space: read_engine_color_space() }
	}
}

//==============================================================================
/// f64 배열을 바이트로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
/// 보기와 투영 행렬에서 절두체의 면 여섯. (ToonRenderer 와 같은 셈, ax + by + cz + d >= 0 이 안쪽)
//==============================================================================
fn extract_frustum_planes(matrix: &Matrix4) -> [[f64; 4]; 6] {
	let elements = matrix.read_elements();
	let row = |row_index: usize| [elements[row_index], elements[4 + row_index], elements[8 + row_index], elements[12 + row_index]];
	let row0 = row(0);
	let row1 = row(1);
	let row2 = row(2);
	let row3 = row(3);
	let combine = |first: [f64; 4], second: [f64; 4], sign: f64| [first[0] + second[0] * sign, first[1] + second[1] * sign, first[2] + second[2] * sign, first[3] + second[3] * sign];
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
pub fn read_jump_flood_steps(reach: f64) -> Vec<i32> {
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
/// 가는 선의 덮는 몫을 줄인 선의 판 해상도에서 한 번 셈하는 프로그램의 소스. (가는 선 섞기 셰이더와 같은 셈, discard 대신 0 을 쓴다)
//==============================================================================
fn add_coverage_pass_define(source: &str) -> String {
	match source.find('\n') {
		Some(line_end) => {
			let mut result = String::with_capacity(source.len() + 32);
			result.push_str(&source[..line_end + 1]);
			result.push_str("#define VANILLA_COVERAGE_PASS\n");
			result.push_str(&source[line_end + 1..]);
			result
		}
		None => {
			String::from(source)
		}
	}
}

//==============================================================================
/// 선 굵기 셈. ([바깥 선, 꺾임 선] 의 그리는 영역 픽셀, 0 이 아니면 한 픽셀 밑으로 내리지 않음, 원근의 월드 굵기는 초점 거리에서)
//==============================================================================
fn compute_line_widths(widths: [f64; 2], width_unit: ToonOutlineWidthUnit, viewport_height: i32, projection_scale: f64, is_perspective: bool, focus_distance: f64) -> [f64; 2] {
	let mut pixels_per_unit = 1.0;
	if width_unit == ToonOutlineWidthUnit::World {
		pixels_per_unit = viewport_height as f64 * 0.5 * projection_scale;
		if is_perspective {
			pixels_per_unit /= focus_distance.max(0.001);
		}
	}
	let to_pixels = |width: f64| if width > 0.0 { (width * pixels_per_unit).max(1.0) } else { 0.0 };
	[to_pixels(widths[0]), to_pixels(widths[1])]
}

impl ToonScreenOutline {
	//==============================================================================
	/// 셰이더 소스. ((정점, 조각) 아홉 벌: 번호, 씨앗, 모으기, 넓히기, 섞기, 가는 선의 씨앗 쪽 판, 가는 선 섞기, 줄인 판의 가는 선 덮는 몫, 덮는 몫 판 섞기)
	///
	/// # 인자
	/// - `color_space`: 색 공간. (원본 기본은 엔진의 색 공간, Linear 면 섞기 셰이더에 #define VANILLA_COLOR_SPACE_LINEAR 1 이 들어간다)
	///
	/// # 돌려줌
	/// (정점 셰이더, 조각 셰이더) 아홉. (번호, 씨앗, 모으기, 넓히기, 섞기, 가는 선의 씨앗 쪽 판, 가는 선 섞기, 줄인 판의 가는 선 덮는 몫, 덮는 몫 판 섞기, 번호 셰이더의 어트리뷰트 자리는 TOON_ATTRIBUTE_LOCATIONS)
	//==============================================================================
	pub fn read_program_sources(color_space: ColorSpace) -> [(String, String); 9] {
		[
			(String::from(LINE_VERTEX_SHADER), String::from(LINE_FRAGMENT_SHADER)),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), String::from(SEED_FRAGMENT_SHADER)),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), String::from(SEED_GATHER_FRAGMENT_SHADER)),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), String::from(JUMP_FLOOD_FRAGMENT_SHADER)),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), add_color_space_define(COMPOSITE_FRAGMENT_SHADER, color_space)),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), String::from(EDGE_MASK_FRAGMENT_SHADER)),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), add_color_space_define(THIN_COMPOSITE_FRAGMENT_SHADER, color_space)),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), add_coverage_pass_define(&add_color_space_define(THIN_COMPOSITE_FRAGMENT_SHADER, color_space))),
			(String::from(SCREEN_TRIANGLE_VERTEX_SHADER), String::from(COVERAGE_BLIT_FRAGMENT_SHADER)),
		]
	}

	//==============================================================================
	/// 생성. (아홉 프로그램과 빈 버텍스 어레이를 짓는다, 선의 판은 첫 begin_line_ids 에서, 색 공간은 엔진의 것)
	///
	/// # 인자
	/// - `gl`: 툰 렌더러와 같은 컨텍스트.
	///
	/// # 돌려줌
	/// 새 화면 공간 바깥선. 셰이더를 짓지 못하거나 버텍스 어레이를 못 만들면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &GlContext) -> Result<ToonScreenOutline, String> {
		ToonScreenOutline::new_with_options(gl, ToonScreenOutlineOptions::default())
	}

	//==============================================================================
	/// 꼴을 주어 생성. (원본 new ToonScreenOutline(gl, options))
	///
	/// # 인자
	/// - `gl`: 툰 렌더러와 같은 컨텍스트.
	/// - `options`: 색 공간. (ToonScreenOutlineOptions, HDR 출력에서 선 색을 선형으로 바꾸는 곡선)
	///
	/// # 돌려줌
	/// 새 화면 공간 바깥선. 셰이더를 짓지 못하거나 버텍스 어레이를 못 만들면 Err(오류 글).
	//==============================================================================
	pub fn new_with_options(gl: &GlContext, options: ToonScreenOutlineOptions) -> Result<ToonScreenOutline, String> {
		let joint_block_buffer = JointBlockBuffer::new(gl)?;
		let line_program = ShaderProgram::create_with_attribute_locations(gl, LINE_VERTEX_SHADER, LINE_FRAGMENT_SHADER, &read_line_attribute_locations())?;
		let seed_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, SEED_FRAGMENT_SHADER)?;
		let seed_gather_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, SEED_GATHER_FRAGMENT_SHADER)?;
		let jump_flood_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, JUMP_FLOOD_FRAGMENT_SHADER)?;
		let composite_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, &add_color_space_define(COMPOSITE_FRAGMENT_SHADER, options.color_space))?;
		let edge_mask_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, EDGE_MASK_FRAGMENT_SHADER)?;
		let thin_composite_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, &add_color_space_define(THIN_COMPOSITE_FRAGMENT_SHADER, options.color_space))?;
		let thin_coverage_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, &add_coverage_pass_define(&add_color_space_define(THIN_COMPOSITE_FRAGMENT_SHADER, options.color_space)))?;
		let coverage_blit_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, COVERAGE_BLIT_FRAGMENT_SHADER)?;
		let empty_vertex_array = unsafe { crate::core::glstatecache::create_vertex_array(gl)? };
		Ok(ToonScreenOutline {
			line_program,
			seed_program,
			seed_gather_program,
			jump_flood_program,
			composite_program,
			edge_mask_program,
			thin_composite_program,
			thin_coverage_program,
			coverage_blit_program,
			empty_vertex_array,
			line_target: None,
			color: [0.08, 0.07, 0.1, 1.0],
			width: 2.0,
			width_unit: ToonOutlineWidthUnit::Pixel,
			focus_distance: 0.0,
			depth_threshold: 0.05,
			depth_margin: 2.0,
			crease_cosine: (std::f64::consts::PI / 3.0).cos(),
			crease_width: 1.0,
			crease_opacity: 1.0,
			resolution_scale: 1.0,
			is_hdr_output: false,
			frame: None,
			saved_state: None,
			is_drawing_line_ids: false,
			instance_data: Vec::new(),
			batch_slots: Vec::new(),
			line_uniform_values: Cell::new([f32::NAN; 4]),
			drawn_distance_sum: 0.0,
			drawn_distance_count: 0,
			line_widths: [2.0, 1.0],
			line_color_table: vec![0; (TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID as usize + 1) * 4],
			line_color_count: 0,
			line_color_texture: None,
			joint_block_buffer,
			is_line_color_table_dirty: false,
			color_space: options.color_space,
		})
	}

	//==============================================================================
	/// 색 공간 반환. (원본 getColorSpace, 생성 때 정한 값, HDR 출력에서 선 색을 선형으로 바꾸는 곡선, Gamma 는 pow 2.2, Linear 는 sRGB 표준 곡선)
	///
	/// # 돌려줌
	/// ColorSpace::Gamma 나 ColorSpace::Linear.
	//==============================================================================
	pub fn get_color_space(&self) -> ColorSpace {
		self.color_space
	}

	//==============================================================================
	/// 선 색 설정. (sRGB 0 ~ 1 과 알파, 바깥 선과 꺾임 선이 같이 쓴다)
	///
	/// # 인자
	/// - `color`: [r, g, b, a] 의 sRGB.
	//==============================================================================
	pub fn set_color(&mut self, color: [f64; 4]) {
		self.color = color;
	}

	//==============================================================================
	/// 번호마다 선 색 설정. (그 번호의 모형이 앞에 선 바깥 선과 꺾임 선을 이 색으로, 표에 없는 번호는 set_color 의 색 그대로)
	/// - 언리얼 Custom Stencil 의 후처리 바깥선처럼 선의 판에 이미 있는 번호로 색을 고른다. 고른 모형을 강조하거나 편마다 색을 달리할 때.
	/// - 표는 선 번호 전부(1 ~ TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID)를 덮는 128 x 128 찾기 텍스처다. 넣는 번호 수에 상한이 없고, 화소마다 한 번 읽는다.
	/// - 두 번호가 맞닿은 선은 앞(카메라에 가까운) 쪽 번호의 색이다. 색은 sRGB 8 비트로 담고, 알파 0 은 1/255 로 담는다. (표에 없음과 가르려고, 보이지 않는 것은 같다)
	///
	/// # 인자
	/// - `line_id`: 1 ~ TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID. (draw_mesh 와 draw_mesh_instanced 에 준 번호, 범위 밖이면 하지 않는다)
	/// - `color`: [r, g, b, a] 의 sRGB 0 ~ 1.
	///
	/// # 예제
	/// 가리킨 바위 하나의 선만 노랗게
	/// ```rust,no_run
	/// use vanilla::{create_octahedron_geometry, Matrix4, MeshBuilder, ToonDrawOptions, ToonInstance, ToonLineDrawOptions, ToonPartOptions, ToonRenderer, ToonScreenOutline, Vector3};
	///
	/// fn draw_picked_rock(gl: &vanilla::GlContext) -> Result<(), String> {
	/// 	let mut renderer = ToonRenderer::new(gl)?;
	/// 	let mut outline = ToonScreenOutline::new(gl)?;
	/// 	let mut builder = MeshBuilder::new();
	/// 	builder.add_part(&create_octahedron_geometry(0.3), &Matrix4::create_identity(), "#a89880", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
	/// 	let rock_mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("mesh failed")?;
	/// 	let instance_list: Vec<ToonInstance> = (0..3).map(|index| ToonInstance { model_matrix: Matrix4::create_translation(index as f64 * 0.7, 0.0, 0.0), line_id: Some(10 + index), ..ToonInstance::default() }).collect();
	/// 	outline.set_line_color(11, [1.0, 0.85, 0.2, 1.0]);
	/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(0.7, 2.0, 3.0), Vector3::create(0.7, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	/// 	let projection_matrix = Matrix4::create_perspective(std::f64::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
	/// 	if outline.begin_line_ids(gl, &view_matrix, &projection_matrix) {
	/// 		outline.draw_mesh_instanced(gl, &rock_mesh, &instance_list, 10, ToonLineDrawOptions::default());
	/// 		outline.end_line_ids(gl);
	/// 	}
	/// 	renderer.begin_frame_with_matrices(gl, &view_matrix, &projection_matrix);
	/// 	renderer.draw_mesh_instanced(gl, &rock_mesh, &instance_list, ToonDrawOptions::default());
	/// 	renderer.end_frame(gl);
	/// 	outline.draw_outline(gl);
	/// 	Ok(())
	/// }
	/// ```
	//==============================================================================
	pub fn set_line_color(&mut self, line_id: u32, color: [f64; 4]) {
		if !(1..=TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID).contains(&line_id) {
			return;
		}
		let offset = line_id as usize * 4;
		if self.line_color_table[offset + 3] == 0 {
			self.line_color_count += 1;
		}
		let to_byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
		self.line_color_table[offset] = to_byte(color[0]);
		self.line_color_table[offset + 1] = to_byte(color[1]);
		self.line_color_table[offset + 2] = to_byte(color[2]);
		self.line_color_table[offset + 3] = to_byte(color[3]).max(1);
		self.is_line_color_table_dirty = true;
	}

	//==============================================================================
	/// 번호 하나를 색 표에서 빼기. (그 번호의 선은 set_color 의 색으로 돌아간다)
	///
	/// # 인자
	/// - `line_id`: set_line_color 에 준 번호.
	//==============================================================================
	pub fn remove_line_color(&mut self, line_id: u32) {
		if !(1..=TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID).contains(&line_id) {
			return;
		}
		let offset = line_id as usize * 4;
		if self.line_color_table[offset + 3] == 0 {
			return;
		}
		self.line_color_table[offset..offset + 4].fill(0);
		self.line_color_count -= 1;
		self.is_line_color_table_dirty = true;
	}

	//==============================================================================
	/// 색 표 비우기. (모든 선이 set_color 의 색으로 돌아간다)
	///
	/// # 예제
	/// 프레임마다 강조할 번호를 다시 넣기
	/// ```rust,no_run
	/// use vanilla::ToonScreenOutline;
	///
	/// fn mark_lines(outline: &mut ToonScreenOutline) -> u32 {
	/// 	outline.clear_line_colors();
	/// 	outline.set_line_color(3, [1.0, 1.0, 1.0, 1.0]);
	/// 	outline.set_line_color(7, [1.0, 0.3, 0.2, 1.0]);
	/// 	outline.get_line_color_count()
	/// }
	/// ```
	//==============================================================================
	pub fn clear_line_colors(&mut self) {
		if self.line_color_count == 0 {
			return;
		}
		self.line_color_table.fill(0);
		self.line_color_count = 0;
		self.is_line_color_table_dirty = true;
	}

	//==============================================================================
	/// 색 표에 든 번호 수 반환.
	///
	/// # 돌려줌
	/// set_line_color 로 넣고 빼지 않은 번호 수.
	//==============================================================================
	pub fn get_line_color_count(&self) -> u32 {
		self.line_color_count
	}

	//==============================================================================
	/// 바깥 선 굵기 설정. (실루엣과 다른 번호와 맞닿은 곳의 선, 반지름이 아니라 선 하나의 굵기, 0 이면 긋지 않음, 꺾임 선 굵기도 이 단위로 센다)
	///
	/// # 인자
	/// - `width`: 선 하나의 굵기. (경계를 가운데로 양쪽에 반씩)
	/// - `unit`: 굵기의 단위. (원본 기본 Pixel)
	//==============================================================================
	pub fn set_width(&mut self, width: f64, unit: ToonOutlineWidthUnit) {
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
	pub fn set_crease(&mut self, angle_degrees: f64, width: f64, opacity: f64) {
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
	pub fn set_focus_distance(&mut self, focus_distance: f64) {
		self.focus_distance = focus_distance.max(0.0);
	}

	//==============================================================================
	/// 깊이 문턱 설정. (같은 번호의 이웃 화소 가운데 가린 쪽이 가려진 쪽의 접평면에서 이만큼 넘게 떨어지면 가려진 실루엣으로 긋는다, 월드 길이)
	/// - `depth_threshold` 는 면에 수직인 거리이고, 화소 여유가 더해진다. (set_depth_margin 의 배율 x 잰 오차, 오차는 화소 크기와 면의 기울기를 따른다)
	/// - 장면의 크기에 맞춘다. (사람 크기 장면이면 0.02 ~ 0.1, 0 이면 오차 여유만으로 가른다)
	///
	/// # 인자
	/// - `depth_threshold`: 접평면 거리 문턱. (월드 길이, 면에 수직, 0 이상)
	///
	/// # 예제
	/// 낮은 턱까지 실루엣으로 긋기
	/// ```rust,no_run
	/// use vanilla::{GlContext, ToonScreenOutline};
	///
	/// fn make_outline(gl: &GlContext) -> Result<ToonScreenOutline, String> {
	/// 	let mut outline = ToonScreenOutline::new(gl)?;
	/// 	outline.set_depth_threshold(0.01);
	/// 	assert_eq!(outline.depth_threshold(), 0.01);
	/// 	Ok(outline)
	/// }
	/// ```
	//==============================================================================
	pub fn set_depth_threshold(&mut self, depth_threshold: f64) {
		self.depth_threshold = depth_threshold.max(0.0);
	}

	//==============================================================================
	/// 깊이 문턱 읽기. (월드 길이, 면에 수직)
	///
	/// # 돌려줌
	/// 접평면 거리 문턱. (기본 0.05)
	//==============================================================================
	pub fn depth_threshold(&self) -> f64 {
		self.depth_threshold
	}

	//==============================================================================
	/// 깊이 여유 배율 설정. (깊이 문턱에 더하는 화소 여유가 잰 오차의 몇 배인지, 기본 2)
	/// - 이어진 면에서도 생기는 접평면 거리의 오차를 화소마다 잰다. 노멀 8 비트 팔면체의 각 오차(0.0165 rad) x 두 화소 자리 사이 거리(면이 비스듬할수록 길다), 꺾인 줄을 사이에 둔 두 화소의 화소 한 칸 x 두 노멀이 벌어진 사인, 깊이 24 비트 한 칸.
	/// - 두 면이 나란한 낮은 턱은 꺾임 몫이 0 이라 화소 한 칸보다 낮아도 잡힌다. 둥근 면을 굵은 다각형으로 지어 보간 노멀이 면에서 크게 벗어나는 모형에서 깊이 문턱을 아주 낮추면 잔선이 생기니, 그때 배율을 올린다.
	/// - 0 이면 깊이 문턱만으로 가른다.
	///
	/// # 인자
	/// - `depth_margin`: 잰 오차에 곱할 배율. (0 이상, 0 밑은 0)
	///
	/// # 예제
	/// 여유를 줄여 낮은 턱을 더 잡기
	/// ```rust,no_run
	/// use vanilla::{GlContext, ToonScreenOutline};
	///
	/// fn make_outline(gl: &GlContext) -> Result<ToonScreenOutline, String> {
	/// 	let mut outline = ToonScreenOutline::new(gl)?;
	/// 	outline.set_depth_threshold(0.02);
	/// 	outline.set_depth_margin(1.0);
	/// 	assert_eq!(outline.depth_margin(), 1.0);
	/// 	Ok(outline)
	/// }
	/// ```
	//==============================================================================
	pub fn set_depth_margin(&mut self, depth_margin: f64) {
		self.depth_margin = depth_margin.max(0.0);
	}

	//==============================================================================
	/// 깊이 여유 배율 읽기.
	///
	/// # 돌려줌
	/// 잰 오차에 곱할 배율. (기본 2)
	//==============================================================================
	pub fn depth_margin(&self) -> f64 {
		self.depth_margin
	}

	//==============================================================================
	/// 선의 판 해상도 몫 설정. (그리는 영역에 대한 몫 0.25 ~ 1, 기본 1, 폰과 태블릿의 가벼운 길은 낮춘다)
	/// - 이름대로 섞기 앞의 모든 판(선의 판 = 노멀과 번호와 깊이, 가면, 씨앗, 넓히기 판)을 그리는 영역 x 몫 크기로 잡는다. 선의 판은 몫만큼 작은 해상도로 한 번 더 그려지므로 값이 몫의 제곱으로 준다.
	///   기본 1 은 그리는 영역과 같은 크기라 화면이 바이트까지 그대로다.
	/// - 몫이 1 보다 작을 때 가는 선(굵기 x 몫이 약 3.24 선의 판 픽셀 이하)은 선의 판 해상도에서 덮는 몫(진하기와 번호 색을 곱한 RGBA8)을 한 번 셈하고, 마지막 섞기가 그것을 쌍선형으로 한 번 읽는다. (읽기 9 에서 1)
	///   굵은 선은 선의 판 해상도에서 점프 플러딩을 하고 섞을 때 거리를 그리는 영역 픽셀로 곱해 잰다. 어느 길이든 선 굵기는 그리는 영역 픽셀(set_width)이고 선 자리는 몫이 작을수록 가장자리가 부드러워진다.
	/// - get_line_target() 의 line_texture, depth_texture 해상도는 그리는 영역 x 몫이다. (굵기 0 이면 선의 판만 그리는 동작은 그대로, 해상도만 그리는 영역과 다를 수 있다)
	/// - CSS 화소당 1.5 화소로 맞추려면 몫 = 1.5 / (그리는 버퍼 화소 / CSS 화소) 로 준다.
	///
	/// # 인자
	/// - `resolution_scale`: 0.25 ~ 1. (1 이면 그리는 영역 해상도 그대로)
	//==============================================================================
	pub fn set_resolution_scale(&mut self, resolution_scale: f64) {
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
	pub fn get_line_widths(&self) -> [f64; 2] {
		self.line_widths
	}

	//==============================================================================
	/// 선의 판 반환. (없으면 None, 번호와 깊이와 가면 텍스처, 크기)
	///
	/// # 돌려줌
	/// 선의 판. (노멀과 번호 텍스처, 깊이 텍스처, 가는 선의 씨앗 쪽 판, 굵은 선을 그었으면 씨앗과 넓히기 판 둘과 마친 쪽, 줄여 가는 선을 그었으면 덮는 몫 판, 이번 프레임의 길, 크기, 아직 짓지 않았거나 짓지 못했으면 None)
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
	pub fn read_line_widths(&self) -> [f64; 2] {
		let frame = match self.frame.as_ref() {
			Some(frame) => {
				frame
			}
			None => {
				return self.line_widths;
			}
		};
		let average_distance = if self.drawn_distance_count > 0 {
			self.drawn_distance_sum / self.drawn_distance_count as f64
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
		compute_line_widths([self.width, self.crease_width], self.width_unit, frame.viewport[3], (frame.projection_matrix.elements[5].abs()) as f64, frame.is_perspective, focus_distance)
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
	pub fn begin_line_ids(&mut self, gl: &GlContext, view_matrix: &Matrix4, projection_matrix: &Matrix4) -> bool {
		let mut viewport = [0i32; 4];
		unsafe {
			crate::core::glstatecache::get_parameter_i32_slice(gl, glow::VIEWPORT, &mut viewport);
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
		let line_width = ((width as f64 * self.resolution_scale).ceil() as i32).max(1);
		let line_height = ((height as f64 * self.resolution_scale).ceil() as i32).max(1);
		let (framebuffer, target_width, target_height) = match self.read_line_target(gl, line_width, line_height, line_width, line_height) {
			Some(line_target) => {
				(line_target.framebuffer, line_target.width, line_target.height)
			}
			None => {
				return false;
			}
		};
		self.save_state(gl);
		self.is_drawing_line_ids = true;
		unsafe {
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(framebuffer));
			crate::core::glstatecache::viewport(gl, 0, 0, target_width, target_height);
			crate::core::glstatecache::disable(gl, glow::SCISSOR_TEST);
			crate::core::glstatecache::disable(gl, glow::BLEND);
			crate::core::glstatecache::clear_color(gl, 0.0, 0.0, 0.0, 0.0);
			crate::core::glstatecache::depth_mask(gl, true);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
			crate::core::glstatecache::enable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::depth_func(gl, glow::LEQUAL);
			crate::core::glstatecache::enable(gl, glow::CULL_FACE);
			crate::core::glstatecache::cull_face(gl, glow::BACK);
			crate::core::glstatecache::front_face(gl, glow::CCW);
			self.line_program.use_program(gl);
			gl.uniform_matrix_4_f32_slice(self.line_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_matrix.elements);
		}
		self.line_uniform_values.set([f32::NAN; 4]);
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
	pub fn draw_mesh(&mut self, gl: &GlContext, mesh: &ToonMesh, model_matrix: &Matrix4, line_id: u32, options: ToonLineDrawOptions) {
		if !self.is_drawing_line_ids {
			return;
		}
		if options.is_culled && !self.is_visible(mesh, model_matrix) {
			return;
		}
		self.add_drawn_distance(mesh, model_matrix);
		let is_mirrored = options.is_mirrored.unwrap_or_else(|| read_matrix_determinant(model_matrix) < 0.0);
		let (body_vertex_array, _) = mesh.get_body();
		self.draw_with_count(gl, mesh, model_matrix, 0, line_id, 0, is_mirrored, options, body_vertex_array);
	}

	//==============================================================================
	/// 모형 하나를 그릴지 가리고 초점 거리에 더하기. (원본 acceptMesh, draw_mesh 가 그리기 앞에 하는 것, 그리기 줄이 담은 차례로 부르고 그리기는 draw_mesh_batch 로 몰아 한다)
	///
	/// # 돌려줌
	/// 그릴 것이면 true.
	//==============================================================================
	pub(crate) fn accept_mesh(&mut self, mesh: &ToonMesh, model_matrix: &Matrix4, is_culled: bool) -> bool {
		if !self.is_drawing_line_ids || (is_culled && !self.is_visible(mesh, model_matrix)) {
			return false;
		}
		self.add_drawn_distance(mesh, model_matrix);
		true
	}

	//==============================================================================
	/// 같은 모형과 꼴의 여럿의 번호를 인스턴스 그리기 한 번으로. (원본 drawMeshBatch, 그리기 줄의 자동 인스턴싱, accept_mesh 로 고른 것, 감기를 뒤집는 것은 뒤집는 것끼리)
	/// - 번호는 인스턴스마다 틴트 칸의 x 로. (draw_mesh_instanced 와 같은 길, 하나면 보통 그리기)
	/// - 묶음 자리(slot_index)의 값이 앞 프레임과 같으면 다시 올리지 않는다. (멈춘 장면은 올림 없음, 몸 묶음 자리와 같은 꼴)
	///
	/// # 인자
	/// - `model_matrices`: 모형 행렬 목록.
	/// - `line_ids`: 번호 목록. (행렬과 같은 자리)
	/// - `options`: 모두에 같은 그리기 꼴.
	/// - `slot_index`: 묶음 자리 번호. (그리기 줄이 프레임마다 같은 차례로 준다)
	/// - `is_mirrored`: 감기를 뒤집어 그릴지. (모두에 같다)
	//==============================================================================
	pub(crate) fn draw_mesh_batch(&mut self, gl: &GlContext, mesh: &ToonMesh, model_matrices: &[Matrix4], line_ids: &[u32], options: ToonLineDrawOptions, slot_index: usize, is_mirrored: bool) {
		if !self.is_drawing_line_ids || model_matrices.is_empty() {
			return;
		}
		let count = model_matrices.len();
		let (body_vertex_array, _) = mesh.get_body();
		if count == 1 {
			self.draw_with_count(gl, mesh, &model_matrices[0], 0, line_ids[0], 0, is_mirrored, options, body_vertex_array);
			return;
		}
		let float_count = count * TOON_INSTANCE_FLOAT_COUNT;
		let mut instance_data = std::mem::take(&mut self.instance_data);
		instance_data.clear();
		for (model_matrix, &line_id) in model_matrices.iter().zip(line_ids.iter()) {
			instance_data.extend_from_slice(&model_matrix.elements);
			instance_data.extend_from_slice(&[line_id.min(TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID) as f32, 0.0, 0.0, 1.0]);
		}
		while self.batch_slots.len() <= slot_index {
			let Ok(buffer) = (unsafe { crate::core::glstatecache::create_buffer(gl) }) else {
				self.instance_data = instance_data;
				return;
			};
			self.batch_slots.push(LineBatchSlot { buffer, values: Vec::new(), count: 0, vertex_arrays: Vec::new() });
		}
		let slot = &mut self.batch_slots[slot_index];
		let is_same = slot.count == count && slot.values.len() >= float_count && slot.values[..float_count].iter().zip(instance_data.iter()).all(|(value, other)| value == other && value.is_sign_negative() == other.is_sign_negative());
		if !is_same {
			slot.values.clear();
			slot.values.extend_from_slice(&instance_data);
			slot.count = count;
			#[cfg(test)]
			crate::graphics3d::toon::toonrenderer::count_upload(3);
			unsafe {
				crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(slot.buffer));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&instance_data), glow::DYNAMIC_DRAW);
			}
		}
		let mesh_id = mesh.get_mesh_id();
		let found = slot.vertex_arrays.iter().find(|entry| entry.0 == mesh_id).map(|entry| entry.1);
		let vertex_array = match found {
			Some(vertex_array) => {
				vertex_array
			}
			None => {
				if slot.vertex_arrays.len() >= LINE_BATCH_SLOT_MESH_LIMIT {
					for (_, old_vertex_array) in slot.vertex_arrays.drain(..) {
						unsafe {
							crate::core::glstatecache::delete_vertex_array(gl, old_vertex_array);
						}
					}
				}
				let Some(vertex_array) = create_mesh_instance_vertex_array(gl, mesh, slot.buffer) else {
					self.instance_data = instance_data;
					return;
				};
				slot.vertex_arrays.push((mesh_id, vertex_array));
				vertex_array
			}
		};
		self.instance_data = instance_data;
		self.draw_with_count(gl, mesh, &Matrix4::create_identity(), count as i32, line_ids[0], 1, is_mirrored, options, vertex_array);
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
	pub fn draw_mesh_instanced(&mut self, gl: &GlContext, mesh: &ToonMesh, instance_list: &[ToonInstance], line_id: u32, options: ToonLineDrawOptions) {
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
			instance_data.extend_from_slice(&[(instance_line_id as f64) as f32, 0.0, 0.0, 1.0]);
			instance_count += 1;
		}
		if instance_count > 0 {
			debug_assert_eq!(instance_data.len(), instance_count * TOON_INSTANCE_FLOAT_COUNT);
			unsafe {
				crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(mesh.get_instance_buffer()));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&instance_data), glow::DYNAMIC_DRAW);
			}
			#[cfg(test)]
			crate::graphics3d::toon::toonrenderer::count_instance_upload();
			let (body_vertex_array, _) = mesh.get_body();
			self.draw_with_count(gl, mesh, &Matrix4::create_identity(), instance_count as i32, line_id, 1, options.is_mirrored == Some(true), options, body_vertex_array);
		}
		self.instance_data = instance_data;
	}

	//==============================================================================
	/// 영구 인스턴스 버퍼로 번호 그리기. (몸과 같은 GPU 버퍼를 쓰고 다시 올리지 않는다, 버퍼 전체의 경계로 통째 거른다, 원본 drawMeshInstanceBuffer)
	/// - 인스턴스마다 번호는 꼴의 instanceLineId 칸, 없으면 공통 번호. 꼴에 행렬이 없으면 자리를 몰라 그리지 않는다. (정점 조각은 선의 판에서 돌지 않는다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `mesh`: ToonRenderer::create_mesh 의 결과.
	/// - `instance_buffer`: ToonRenderer::create_instance_buffer 의 결과. (몸과 같은 모형과 버퍼의 버텍스 어레이를 같이 쓴다)
	/// - `instance_count`: 그릴 인스턴스 수. (버퍼에 든 수를 넘으면 자른다)
	/// - `line_id`: 공통 번호. (꼴에 instanceLineId 가 없을 때)
	/// - `options`: draw_mesh 와 같다. (꼴은 모든 인스턴스에 같다, is_mirrored 가 None 이면 뒤집지 않는다)
	///
	/// # 예제
	/// 돌 버퍼 하나로 몸과 선의 판을 같이 그리기
	/// ```rust,no_run
	/// use vanilla::{create_octahedron_geometry, Matrix4, MeshBuilder, ToonBufferUsage, ToonDrawOptions, ToonLineDrawOptions, ToonPartOptions, ToonRenderer, ToonScreenOutline, Vector3};
	///
	/// fn draw_rocks(gl: &vanilla::GlContext) -> Result<(), String> {
	/// 	let mut renderer = ToonRenderer::new(gl)?;
	/// 	let mut outline = ToonScreenOutline::new(gl)?;
	/// 	let mut builder = MeshBuilder::new();
	/// 	builder.add_part(&create_octahedron_geometry(0.3), &Matrix4::create_identity(), "#a89880", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
	/// 	let rock_mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("mesh failed")?;
	/// 	let instance_layout = renderer.create_instance_layout(&[("instanceMatrix", 16), ("instanceTint", 4), ("instanceLineId", 1)])?;
	/// 	let mut data: Vec<f32> = Vec::new();
	/// 	for index in 0..3 {
	/// 		data.extend_from_slice(Matrix4::create_translation(index as f64 * 0.5, 0.0, 0.0).get_elements());
	/// 		data.extend_from_slice(&[1.0, 1.0, 1.0, 1.0, 10.0 + index as f32]);
	/// 	}
	/// 	let instance_buffer = renderer.create_instance_buffer(gl, &instance_layout, &data, ToonBufferUsage::Static)?;
	/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(0.5, 2.0, 3.0), Vector3::create(0.5, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	/// 	let projection_matrix = Matrix4::create_perspective(std::f64::consts::PI / 4.0, 1.0, 0.1, 100.0);
	/// 	if outline.begin_line_ids(gl, &view_matrix, &projection_matrix) {
	/// 		outline.draw_mesh_instance_buffer(gl, &rock_mesh, &instance_buffer, 3, 10, ToonLineDrawOptions::default());
	/// 		outline.end_line_ids(gl);
	/// 	}
	/// 	renderer.begin_frame_with_matrices(gl, &view_matrix, &projection_matrix);
	/// 	renderer.draw_mesh_instance_buffer(gl, &rock_mesh, &instance_buffer, 3, ToonDrawOptions::default());
	/// 	renderer.end_frame(gl);
	/// 	outline.draw_outline(gl);
	/// 	Ok(())
	/// }
	/// ```
	//==============================================================================
	pub fn draw_mesh_instance_buffer(&mut self, gl: &GlContext, mesh: &ToonMesh, instance_buffer: &ToonInstanceBuffer, instance_count: usize, line_id: u32, options: ToonLineDrawOptions) {
		if !self.is_drawing_line_ids {
			return;
		}
		let instance_layout = instance_buffer.get_layout();
		if !instance_layout.has_matrix() {
			return;
		}
		let count = instance_count.min(instance_buffer.get_instance_count());
		if count == 0 {
			return;
		}
		if options.is_culled {
			if let (Some(sphere), Some(frame)) = (read_instance_buffer_sphere(mesh, instance_buffer), self.frame.as_ref()) {
				if !is_sphere_in_frustum(&frame.frustum_planes, &sphere) {
					return;
				}
			}
		}
		// 초점 거리를 모형 거리로 셀 때만 인스턴스마다의 거리를 더한다. (draw_mesh_instanced 와 같은 굵기)
		let is_perspective = self.frame.as_ref().map(|frame| frame.is_perspective).unwrap_or(false);
		if self.width_unit == ToonOutlineWidthUnit::World && self.focus_distance <= 0.0 && is_perspective {
			if let Some(data) = instance_buffer.get_data() {
				let layout_float_count = instance_layout.get_float_count();
				let matrix_offset = instance_layout.get_attributes().iter().find(|attribute| attribute.name == TOON_INSTANCE_MATRIX_NAME).map(|attribute| attribute.offset).unwrap_or(0);
				for instance_index in 0..count {
					let offset = instance_index * layout_float_count + matrix_offset;
					let mut model_matrix = Matrix4::create_identity();
					model_matrix.elements.copy_from_slice(&data[offset..offset + 16]);
					if !options.is_culled || self.is_visible(mesh, &model_matrix) {
						self.add_drawn_distance(mesh, &model_matrix);
					}
				}
			}
		}
		let Some((body_vertex_array, _)) = instance_buffer.read_vertex_arrays(gl, mesh) else {
			return;
		};
		self.draw_with_count(gl, mesh, &Matrix4::create_identity(), count as i32, line_id, if instance_layout.has_line_id() { 2 } else { 0 }, options.is_mirrored == Some(true), options, body_vertex_array);
	}

	//==============================================================================
	/// 번호 그리기 본체. (instance_count 가 0 이면 보통 그리기)
	//==============================================================================
	#[allow(clippy::too_many_arguments)]
	fn draw_with_count(&self, gl: &GlContext, mesh: &ToonMesh, model_matrix: &Matrix4, instance_count: i32, line_id: u32, line_id_source: u32, is_mirrored: bool, options: ToonLineDrawOptions, vertex_array: GlVertexArray) {
		let line_program = &self.line_program;
		let is_skinned = mesh.is_skinned() && options.joint_matrices.is_some();
		let (_, body_index_count) = mesh.get_body();
		let index_start = options.index_start.min(body_index_count as usize);
		let remaining_count = body_index_count as usize - index_start;
		let index_count = options.index_count.unwrap_or(remaining_count).min(remaining_count);
		let line_flags = if options.has_crease_line { 1.0 } else { 0.0 } + if options.has_screen_outline { 2.0 } else { 0.0 };
		unsafe {
			gl.uniform_matrix_4_f32_slice(line_program.get_uniform_location(gl, "modelMatrix").as_ref(), false, &model_matrix.elements);
			// 그리기마다 거의 같은 넷은 바뀔 때만 올린다. (begin_line_ids 가 잊는다)
			let mut line_uniform_values = self.line_uniform_values.get();
			let instanced_value = if instance_count > 0 { 1.0 } else { 0.0 };
			let line_id_source_value = line_id_source as f32;
			let skinned_value = if is_skinned { 1.0 } else { 0.0 };
			if line_uniform_values[0] != instanced_value {
				line_uniform_values[0] = instanced_value;
				gl.uniform_1_f64(line_program.get_uniform_location(gl, "instanced").as_ref(), instanced_value as f64);
			}
			if line_uniform_values[1] != line_id_source_value {
				line_uniform_values[1] = line_id_source_value;
				gl.uniform_1_f64(line_program.get_uniform_location(gl, "instanceLineIds").as_ref(), line_id_source_value as f64);
			}
			if line_uniform_values[2] != skinned_value {
				line_uniform_values[2] = skinned_value;
				gl.uniform_1_f64(line_program.get_uniform_location(gl, "skinned").as_ref(), skinned_value as f64);
			}
			gl.uniform_1_f64(line_program.get_uniform_location(gl, "lineId").as_ref(), line_id.min(TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID) as f64);
			if line_uniform_values[3] != line_flags as f32 {
				line_uniform_values[3] = line_flags as f32;
				gl.uniform_1_f64(line_program.get_uniform_location(gl, "lineFlags").as_ref(), line_flags);
			}
			self.line_uniform_values.set(line_uniform_values);
			if let (true, Some(joint_matrices)) = (is_skinned, options.joint_matrices) {
				self.joint_block_buffer.upload(gl, joint_matrices);
			}
			if index_count == 0 {
				return;
			}
			if is_mirrored {
				crate::core::glstatecache::front_face(gl, glow::CW);
			}
			crate::core::glstatecache::bind_vertex_array(gl, Some(vertex_array));
			let byte_offset = (index_start * 4) as i32;
			#[cfg(test)]
			crate::graphics3d::toon::toonrenderer::count_upload(4);
			if instance_count > 0 {
				crate::core::glstatecache::draw_elements_instanced(gl, glow::TRIANGLES, index_count as i32, glow::UNSIGNED_INT, byte_offset, instance_count);
			}
			else {
				crate::core::glstatecache::draw_elements(gl, glow::TRIANGLES, index_count as i32, glow::UNSIGNED_INT, byte_offset);
			}
			if is_mirrored {
				crate::core::glstatecache::front_face(gl, glow::CCW);
			}
		}
	}

	//==============================================================================
	/// 번호 그리기 끝. (가는 선은 씨앗 쪽 판 하나를 고르고, 굵은 선은 씨앗을 고르고 점프 플러딩으로 넓힌 뒤, 프레임버퍼와 뷰포트와 상태를 돌린다)
	/// - 가는 선(굵기가 약 3.24 선의 판 픽셀 이하): 화면 전체 패스가 씨앗 쪽 판 하나다. 섞기가 둘레 3 x 3 화소의 씨앗을 본다. (씨앗, 모으기, 넓히기 없음)
	/// - 굵은 선: 씨앗, 모으기, 넓히기 걸음마다 한 패스. (씨앗 판과 넓히기 판은 처음 굵은 선을 그을 때 짓는다)
	/// - 판을 통째로 덮어쓰는 패스는 앞에 판 비우기(clear_buffer_u32_slice, clear_buffer_f32_slice)를 불러 옛 내용을 읽어 들이지 않게 한다. (타일 GPU 의 판 불러오기를 없앰, 원본과 같은 자리)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn end_line_ids(&mut self, gl: &GlContext) {
		if !self.is_drawing_line_ids {
			return;
		}
		self.is_drawing_line_ids = false;
		let line_widths = self.read_line_widths();
		self.line_widths = line_widths;
		let (frame, mut line_target) = match (self.frame, self.line_target.take()) {
			(Some(frame), Some(line_target)) => {
				(frame, line_target)
			}
			(_, line_target) => {
				self.line_target = line_target;
				self.restore_state(gl);
				return;
			}
		};
		// 줄인 선의 판의 덮는 몫 셈이 번호 색 표를 읽으므로 표를 먼저 올린다. (바뀐 때만 올림)
		let is_scaled = line_target.width != frame.viewport[2].max(1) || line_target.height != frame.viewport[3].max(1);
		let is_line_color_enabled = is_scaled && self.line_color_count > 0 && self.upload_line_color_table(gl);
		self.draw_end_passes(gl, &frame, &mut line_target, line_widths, is_line_color_enabled);
		self.line_target = Some(line_target);
		self.restore_state(gl);
	}

	//==============================================================================
	/// 번호 그리기 끝의 패스들. (end_line_ids 가 부른다, 길을 가리고 가면이나 씨앗과 넓히기까지 셈, 상태 되돌리기는 부른 쪽)
	//==============================================================================
	fn draw_end_passes(&self, gl: &GlContext, frame: &LineFrame, line_target: &mut ToonLineTarget, line_widths: [f64; 2], is_line_color_enabled: bool) {
		line_target.jump_flood_index = None;
		line_target.outline_path = ToonOutlinePath::None;
		// 바깥 선과 꺾임 선이 모두 0 이면 선의 판(노멀, 번호, 깊이)만 남긴다. (모서리 어둠의 미리 그리기, draw_outline 은 긋지 않는다)
		if line_widths[0] <= 0.0 && line_widths[1] <= 0.0 {
			return;
		}
		// 선의 판이 그리는 영역보다 작으면(set_resolution_scale) 굵기를 선의 판 화소로 재어 가는 선 길을 가린다.
		let viewport_width = frame.viewport[2].max(1);
		let viewport_height = frame.viewport[3].max(1);
		let line_scale_x = line_target.width as f64 / viewport_width as f64;
		let line_scale_y = line_target.height as f64 / viewport_height as f64;
		let is_scaled = line_target.width != viewport_width || line_target.height != viewport_height;
		let is_thin_line = line_widths[0].max(line_widths[1]) * line_scale_x.min(line_scale_y) <= thin_line_maximum_width();
		if !is_thin_line && !ToonScreenOutline::read_jump_flood_target(gl, line_target) {
			return;
		}
		unsafe {
			crate::core::glstatecache::bind_vertex_array(gl, Some(self.empty_vertex_array));
			crate::core::glstatecache::viewport(gl, 0, 0, line_target.width, line_target.height);
			crate::core::glstatecache::disable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::depth_mask(gl, false);
			crate::core::glstatecache::disable(gl, glow::CULL_FACE);
			crate::core::glstatecache::disable(gl, glow::BLEND);

			// 가는 선: 씨앗 쪽 판 고르기 한 패스.
			if is_thin_line {
				crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(line_target.edge_mask_framebuffer));
				gl.clear_buffer_u32_slice(glow::COLOR, 0, &ZERO_CLEAR_VALUE);
				self.use_edge_program(gl, &self.edge_mask_program, line_target, frame, line_widths);
				crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);
				line_target.outline_path = ToonOutlinePath::Thin;
				if is_scaled && ToonScreenOutline::read_coverage_target(gl, line_target) {
					self.draw_thin_coverage(gl, line_target, line_widths, line_scale_x, line_scale_y, is_line_color_enabled);
					line_target.outline_path = ToonOutlinePath::ThinScaled;
				}
				self.unbind_end_textures(gl);
				return;
			}

			// 1. 씨앗 고르기.
			let (seed_framebuffer, seed_texture) = match (line_target.seed_framebuffer, line_target.seed_texture) {
				(Some(seed_framebuffer), Some(seed_texture)) => {
					(seed_framebuffer, seed_texture)
				}
				_ => {
					return;
				}
			};
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(seed_framebuffer));
			gl.clear_buffer_u32_slice(glow::COLOR, 0, &ZERO_CLEAR_VALUE);
			self.use_edge_program(gl, &self.seed_program, line_target, frame, line_widths);
			crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + DEPTH_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);

			// 2. 넓히기 판으로 모으기. (줄인 판이면 칸마다 가장 가까운 씨앗)
			let point_scale_x = line_target.width as f64 / line_target.jump_flood_width as f64;
			let point_scale_y = line_target.height as f64 / line_target.jump_flood_height as f64;
			let seed_gather_program = &self.seed_gather_program;
			crate::core::glstatecache::viewport(gl, 0, 0, line_target.jump_flood_width, line_target.jump_flood_height);
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(line_target.jump_flood_framebuffers[0]));
			gl.clear_buffer_u32_slice(glow::COLOR, 0, &ZERO_CLEAR_VALUE);
			seed_gather_program.use_program(gl);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(seed_texture));
			gl.uniform_1_i32(seed_gather_program.get_uniform_location(gl, "seedTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_2_f64(seed_gather_program.get_uniform_location(gl, "pointScale").as_ref(), point_scale_x, point_scale_y);
			crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);

			// 3. 점프 플러딩. (그리는 영역 굵기의 반과 덮는 몫의 반 픽셀을 넓히기 판 칸으로, 섞을 때 보는 2x2 칸만큼 더)
			let jump_flood_program = &self.jump_flood_program;
			jump_flood_program.use_program(gl);
			gl.uniform_1_i32(jump_flood_program.get_uniform_location(gl, "seedTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_2_f64(jump_flood_program.get_uniform_location(gl, "pointScale").as_ref(), point_scale_x, point_scale_y);
			let reach = (line_widths[0].max(line_widths[1]) * 0.5 + 0.5) * line_scale_x.min(line_scale_y) / point_scale_x.min(point_scale_y) + 1.5;
			let mut seed_index = 0usize;
			for step_size in read_jump_flood_steps(reach) {
				crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(line_target.jump_flood_framebuffers[1 - seed_index]));
				gl.clear_buffer_u32_slice(glow::COLOR, 0, &ZERO_CLEAR_VALUE);
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_TEXTURE_UNIT);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.jump_flood_textures[seed_index]));
				gl.uniform_1_i32(jump_flood_program.get_uniform_location(gl, "stepSize").as_ref(), step_size);
				crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);
				seed_index = 1 - seed_index;
			}
			line_target.jump_flood_index = Some(seed_index);
			line_target.outline_path = ToonOutlinePath::JumpFlood;
			self.unbind_end_textures(gl);
		}
	}

	//==============================================================================
	/// 줄인 선의 판 해상도에서 가는 선의 덮는 몫(곱한 색)을 한 번 셈하기. (섞기와 같은 셈, 거리는 그리는 영역 화소로 잰다)
	//==============================================================================
	fn draw_thin_coverage(&self, gl: &GlContext, line_target: &ToonLineTarget, line_widths: [f64; 2], line_scale_x: f64, line_scale_y: f64, is_line_color_enabled: bool) {
		let coverage_framebuffer = match line_target.coverage_framebuffer {
			Some(coverage_framebuffer) => {
				coverage_framebuffer
			}
			None => {
				return;
			}
		};
		let program = &self.thin_coverage_program;
		let line_color_texture = if is_line_color_enabled { self.line_color_texture } else { None };
		let color = self.color;
		unsafe {
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(coverage_framebuffer));
			gl.clear_buffer_f32_slice(glow::COLOR, 0, &ZERO_CLEAR_FLOAT_VALUE);
			program.use_program(gl);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.edge_mask_texture));
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + COMPOSITE_LINE_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.line_texture));
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + COMPOSITE_DEPTH_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.depth_texture));
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_COLOR_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, line_color_texture);
			let location = |name: &str| program.get_uniform_location(gl, name);
			gl.uniform_1_i32(location("edgeMaskTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("lineTexture").as_ref(), COMPOSITE_LINE_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("depthTexture").as_ref(), COMPOSITE_DEPTH_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("lineColorTexture").as_ref(), LINE_COLOR_TEXTURE_UNIT as i32);
			gl.uniform_1_f64(location("lineColorEnabled").as_ref(), if is_line_color_enabled { 1.0 } else { 0.0 });
			gl.uniform_2_f64(location("lineScale").as_ref(), line_scale_x, line_scale_y);
			gl.uniform_2_f64(location("lineWidths").as_ref(), line_widths[0], line_widths[1]);
			gl.uniform_1_f64(location("creaseOpacity").as_ref(), self.crease_opacity);
			gl.uniform_4_f64(location("outlineColor").as_ref(), color[0], color[1], color[2], color[3]);
			gl.uniform_1_f64(location("hdrOutput").as_ref(), if self.is_hdr_output { 1.0 } else { 0.0 });
			crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_COLOR_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + COMPOSITE_DEPTH_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
		}
	}

	//==============================================================================
	/// 경계를 보는 프로그램(씨앗, 가는 선의 씨앗 쪽 판)을 쓰고 선의 판, 깊이, 카메라, 문턱을 묶기.
	//==============================================================================
	fn use_edge_program(&self, gl: &GlContext, program: &ShaderProgram, line_target: &ToonLineTarget, frame: &LineFrame, line_widths: [f64; 2]) {
		let projection = frame.projection_matrix.read_elements();
		let view = frame.view_matrix.read_elements();
		unsafe {
			program.use_program(gl);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.line_texture));
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + DEPTH_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.depth_texture));
			let location = |name: &str| program.get_uniform_location(gl, name);
			gl.uniform_1_i32(location("lineTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("depthTexture").as_ref(), DEPTH_TEXTURE_UNIT as i32);
			gl.uniform_matrix_3_f32_slice(location("viewRotation").as_ref(), false, &[(view[0]) as f32, (view[1]) as f32, (view[2]) as f32, (view[4]) as f32, (view[5]) as f32, (view[6]) as f32, (view[8]) as f32, (view[9]) as f32, (view[10]) as f32]);
			gl.uniform_4_f64(location("projectionScale").as_ref(), projection[0], projection[5], projection[8], projection[9]);
			gl.uniform_4_f64(location("projectionDepth").as_ref(), projection[10], projection[14], projection[12], projection[13]);
			gl.uniform_1_f64(location("isPerspective").as_ref(), if frame.is_perspective { 1.0 } else { 0.0 });
			gl.uniform_1_f64(location("depthThreshold").as_ref(), self.depth_threshold);
			gl.uniform_1_f64(location("depthMargin").as_ref(), self.depth_margin);
			gl.uniform_1_f64(location("creaseCosine").as_ref(), if line_widths[1] > 0.0 { self.crease_cosine } else { -2.0 });
		}
	}

	//==============================================================================
	/// end_line_ids 가 묶은 텍스처 풀기. (선의 판 유닛과 깊이 유닛, 버텍스 어레이)
	//==============================================================================
	fn unbind_end_textures(&self, gl: &GlContext) {
		unsafe {
			for texture_unit in [DEPTH_TEXTURE_UNIT, LINE_TEXTURE_UNIT] {
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + texture_unit);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			}
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			crate::core::glstatecache::bind_vertex_array(gl, None);
		}
	}

	//==============================================================================
	/// 바깥선 긋기. (가는 선은 씨앗 쪽 판, 굵은 선은 넓힌 씨앗 판으로 begin_line_ids 때의 그리는 영역에 섞는다, 깊이는 보지도 쓰지도 않는다, 스텐실은 그대로)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn draw_outline(&mut self, gl: &GlContext) {
		let (frame, outline_path) = match (self.frame, self.line_target.as_ref()) {
			(Some(frame), Some(line_target)) if line_target.outline_path != ToonOutlinePath::None => {
				(frame, line_target.outline_path)
			}
			_ => {
				return;
			}
		};
		if outline_path == ToonOutlinePath::ThinScaled {
			self.draw_coverage_blit(gl);
			return;
		}
		let is_thin_line = outline_path == ToonOutlinePath::Thin;
		let (seed_texture, line_texture, depth_texture, point_scale, line_scale) = match self.line_target.as_ref() {
			Some(line_target) => {
				let seed_texture = if is_thin_line { Some(line_target.edge_mask_texture) } else { line_target.jump_flood_index.and_then(|jump_flood_index| line_target.jump_flood_textures.get(jump_flood_index).copied()) };
				match seed_texture {
					Some(seed_texture) => {
						(seed_texture, line_target.line_texture, line_target.depth_texture, [line_target.width as f64 / line_target.jump_flood_width as f64, line_target.height as f64 / line_target.jump_flood_height as f64], [line_target.width as f64 / frame.viewport[2].max(1) as f64, line_target.height as f64 / frame.viewport[3].max(1) as f64])
					}
					None => {
						return;
					}
				}
			}
			None => {
				return;
			}
		};
		self.save_state(gl);
		let is_line_color_enabled = self.line_color_count > 0 && self.upload_line_color_table(gl);
		let line_color_texture = if is_line_color_enabled { self.line_color_texture } else { None };
		let composite_program = if is_thin_line { &self.thin_composite_program } else { &self.composite_program };
		let viewport = frame.viewport;
		let color = self.color;
		let line_widths = self.line_widths;
		unsafe {
			crate::core::glstatecache::viewport(gl, viewport[0], viewport[1], viewport[2], viewport[3]);
			crate::core::glstatecache::disable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::depth_mask(gl, false);
			crate::core::glstatecache::disable(gl, glow::CULL_FACE);
			crate::core::glstatecache::enable(gl, glow::BLEND);
			crate::core::glstatecache::blend_func(gl, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			composite_program.use_program(gl);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(seed_texture));
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + COMPOSITE_LINE_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_texture));
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + COMPOSITE_DEPTH_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(depth_texture));
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_COLOR_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, line_color_texture);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			let location = |name: &str| composite_program.get_uniform_location(gl, name);
			if is_thin_line {
				gl.uniform_1_i32(location("edgeMaskTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			}
			else {
				gl.uniform_1_i32(location("seedTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
				gl.uniform_2_f64(location("pointScale").as_ref(), point_scale[0], point_scale[1]);
				gl.uniform_2_f64(location("lineScale").as_ref(), line_scale[0], line_scale[1]);
			}
			gl.uniform_1_i32(location("lineTexture").as_ref(), COMPOSITE_LINE_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("depthTexture").as_ref(), COMPOSITE_DEPTH_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("lineColorTexture").as_ref(), LINE_COLOR_TEXTURE_UNIT as i32);
			gl.uniform_1_f64(location("lineColorEnabled").as_ref(), if is_line_color_enabled { 1.0 } else { 0.0 });
			gl.uniform_4_f64(location("viewportRect").as_ref(), viewport[0] as f64, viewport[1] as f64, viewport[2].max(1) as f64, viewport[3].max(1) as f64);
			gl.uniform_2_f64(location("lineWidths").as_ref(), line_widths[0], line_widths[1]);
			gl.uniform_1_f64(location("creaseOpacity").as_ref(), self.crease_opacity);
			gl.uniform_4_f64(location("outlineColor").as_ref(), color[0], color[1], color[2], color[3]);
			gl.uniform_1_f64(location("hdrOutput").as_ref(), if self.is_hdr_output { 1.0 } else { 0.0 });
			crate::core::glstatecache::bind_vertex_array(gl, Some(self.empty_vertex_array));
			crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);
			crate::core::glstatecache::bind_vertex_array(gl, None);
			for texture_unit in [LINE_TEXTURE_UNIT, COMPOSITE_LINE_TEXTURE_UNIT, COMPOSITE_DEPTH_TEXTURE_UNIT, LINE_COLOR_TEXTURE_UNIT] {
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + texture_unit);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			}
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
		}
		self.restore_state(gl);
	}

	//==============================================================================
	/// 줄인 선의 판의 덮는 몫 판을 그리는 영역에 쌍선형으로 한 번 읽어 섞기.
	//==============================================================================
	fn draw_coverage_blit(&mut self, gl: &GlContext) {
		let (frame, coverage_texture) = match (self.frame, self.line_target.as_ref().and_then(|line_target| line_target.coverage_texture)) {
			(Some(frame), Some(coverage_texture)) => {
				(frame, coverage_texture)
			}
			_ => {
				return;
			}
		};
		self.save_state(gl);
		let viewport = frame.viewport;
		let program = &self.coverage_blit_program;
		unsafe {
			crate::core::glstatecache::viewport(gl, viewport[0], viewport[1], viewport[2], viewport[3]);
			crate::core::glstatecache::disable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::depth_mask(gl, false);
			crate::core::glstatecache::disable(gl, glow::CULL_FACE);
			crate::core::glstatecache::enable(gl, glow::BLEND);
			crate::core::glstatecache::blend_func(gl, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			program.use_program(gl);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(coverage_texture));
			gl.uniform_1_i32(program.get_uniform_location(gl, "coverageTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_4_f64(program.get_uniform_location(gl, "viewportRect").as_ref(), viewport[0] as f64, viewport[1] as f64, viewport[2].max(1) as f64, viewport[3].max(1) as f64);
			crate::core::glstatecache::bind_vertex_array(gl, Some(self.empty_vertex_array));
			crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);
			crate::core::glstatecache::bind_vertex_array(gl, None);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
		}
		self.restore_state(gl);
	}

	//==============================================================================
	/// 번호 색 표를 텍스처에 올리기. (처음이면 짓고, 바뀐 때만 통째로 64 KB 를 올린다)
	///
	/// # 돌려줌
	/// 텍스처가 있으면 true.
	//==============================================================================
	fn upload_line_color_table(&mut self, gl: &GlContext) -> bool {
		let texture = match self.line_color_texture {
			Some(texture) => {
				texture
			}
			None => {
				match unsafe { crate::core::glstatecache::create_texture(gl) } {
					Ok(texture) => {
						self.line_color_texture = Some(texture);
						self.is_line_color_table_dirty = true;
						texture
					}
					Err(_) => {
						return false;
					}
				}
			}
		};
		if !self.is_line_color_table_dirty {
			return true;
		}
		unsafe {
			let unpack_alignment = crate::core::glstatecache::get_parameter_i32(gl, glow::UNPACK_ALIGNMENT);
			crate::core::glstatecache::pixel_store_i32(gl, glow::UNPACK_ALIGNMENT, 4);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + LINE_COLOR_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(texture));
			let height = (TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID + 1) / LINE_COLOR_TABLE_WIDTH;
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, LINE_COLOR_TABLE_WIDTH as i32, height as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&self.line_color_table)));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			crate::core::glstatecache::pixel_store_i32(gl, glow::UNPACK_ALIGNMENT, unpack_alignment);
		}
		self.is_line_color_table_dirty = false;
		true
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
		let elements = model_matrix.read_elements();
		let scale_x = (elements[0] * elements[0] + elements[1] * elements[1] + elements[2] * elements[2]).sqrt();
		let scale_y = (elements[4] * elements[4] + elements[5] * elements[5] + elements[6] * elements[6]).sqrt();
		let scale_z = (elements[8] * elements[8] + elements[9] * elements[9] + elements[10] * elements[10]).sqrt();
		let radius = bounding_radius * (scale_x.max(scale_y).max(scale_z));
		frame.frustum_planes.iter().all(|plane| plane[0] * transformed_center.x + plane[1] * transformed_center.y + plane[2] * transformed_center.z + plane[3] >= -radius)
	}

	//==============================================================================
	/// 선의 판 읽기. (크기가 바뀌면 다시 지음, 짓지 못하면 None)
	//==============================================================================
	fn read_line_target(&mut self, gl: &GlContext, width: i32, height: i32, jump_flood_width: i32, jump_flood_height: i32) -> Option<&ToonLineTarget> {
		let is_same_size = matches!(self.line_target.as_ref(), Some(line_target) if line_target.width == width && line_target.height == height && line_target.jump_flood_width == jump_flood_width && line_target.jump_flood_height == jump_flood_height);
		if !is_same_size {
			self.delete_line_target(gl);
			self.line_target = ToonScreenOutline::create_line_target(gl, width, height, jump_flood_width, jump_flood_height);
		}
		self.line_target.as_ref()
	}

	//==============================================================================
	/// 선의 판 텍스처 하나 짓기. (가까운 칸 거르기, 가장자리 늘이기, 묶은 채로 둔다)
	//==============================================================================
	unsafe fn create_target_texture(gl: &GlContext, internal_format: u32, format: u32, data_type: u32, texture_width: i32, texture_height: i32) -> Option<GlTexture> {
		unsafe {
			let texture = crate::core::glstatecache::create_texture(gl).ok()?;
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(texture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, internal_format as i32, texture_width, texture_height, 0, format, data_type, glow::PixelUnpackData::Slice(None));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			Some(texture)
		}
	}

	//==============================================================================
	/// 선의 판 프레임버퍼 하나 짓기. (색 하나와 깊이, 묶은 채로 둔다, 다 됐는지 함께 돌려준다)
	//==============================================================================
	unsafe fn create_target_framebuffer(gl: &GlContext, color_texture: GlTexture, depth_texture: Option<GlTexture>) -> Option<(GlFramebuffer, bool)> {
		unsafe {
			let framebuffer = crate::core::glstatecache::create_framebuffer(gl).ok()?;
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(framebuffer));
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
			if depth_texture.is_some() {
				gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, depth_texture, 0);
			}
			Some((framebuffer, gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE))
		}
	}

	//==============================================================================
	/// 선의 판 짓기. (선의 판 해상도의 노멀과 번호 RGBA8, 깊이 24, 가는 선의 씨앗 쪽 판 R8UI, 씨앗과 넓히기 판은 굵은 선을 그을 때, 앞의 프레임버퍼와 텍스처는 돌린다)
	//==============================================================================
	fn create_line_target(gl: &GlContext, width: i32, height: i32, jump_flood_width: i32, jump_flood_height: i32) -> Option<ToonLineTarget> {
		unsafe {
			let previous_framebuffer = crate::graphics3d::read_framebuffer_binding(gl);
			let previous_texture = crate::graphics3d::read_texture_binding(gl);
			let line_texture = ToonScreenOutline::create_target_texture(gl, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, width, height)?;
			let depth_texture = ToonScreenOutline::create_target_texture(gl, glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, width, height)?;
			let edge_mask_texture = ToonScreenOutline::create_target_texture(gl, glow::R8UI, glow::RED_INTEGER, glow::UNSIGNED_BYTE, width, height)?;
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, previous_texture);
			let (framebuffer, is_line_complete) = ToonScreenOutline::create_target_framebuffer(gl, line_texture, Some(depth_texture))?;
			let (edge_mask_framebuffer, is_edge_mask_complete) = ToonScreenOutline::create_target_framebuffer(gl, edge_mask_texture, None)?;
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, previous_framebuffer);
			let line_target = ToonLineTarget {
				framebuffer,
				line_texture,
				depth_texture,
				edge_mask_framebuffer,
				edge_mask_texture,
				seed_framebuffer: None,
				seed_texture: None,
				jump_flood_framebuffers: Vec::new(),
				jump_flood_textures: Vec::new(),
				jump_flood_index: None,
				coverage_framebuffer: None,
				coverage_texture: None,
				outline_path: ToonOutlinePath::None,
				width,
				height,
				jump_flood_width,
				jump_flood_height,
			};
			if !is_line_complete || !is_edge_mask_complete {
				ToonScreenOutline::delete_target_objects(gl, &line_target);
				return None;
			}
			Some(line_target)
		}
	}

	//==============================================================================
	/// 굵은 선의 씨앗 판과 넓히기 판 짓기. (처음 굵은 선을 그을 때 한 번, 가는 선만 쓰면 짓지 않는다)
	///
	/// # 돌려줌
	/// 판이 있으면 true. (못 지으면 false, 그 프레임은 긋지 않는다)
	//==============================================================================
	fn read_jump_flood_target(gl: &GlContext, line_target: &mut ToonLineTarget) -> bool {
		if line_target.seed_texture.is_some() {
			return true;
		}
		unsafe {
			let previous_framebuffer = crate::graphics3d::read_framebuffer_binding(gl);
			let previous_texture = crate::graphics3d::read_texture_binding(gl);
			let mut texture_list: Vec<GlTexture> = Vec::new();
			let mut framebuffer_list: Vec<GlFramebuffer> = Vec::new();
			let mut is_complete = true;
			let size_list = [(line_target.width, line_target.height), (line_target.jump_flood_width, line_target.jump_flood_height), (line_target.jump_flood_width, line_target.jump_flood_height)];
			for (texture_width, texture_height) in size_list {
				match ToonScreenOutline::create_target_texture(gl, glow::RGBA16UI, glow::RGBA_INTEGER, glow::UNSIGNED_SHORT, texture_width, texture_height) {
					Some(texture) => {
						texture_list.push(texture);
					}
					None => {
						is_complete = false;
					}
				}
			}
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, previous_texture);
			if is_complete {
				for texture in texture_list.iter() {
					match ToonScreenOutline::create_target_framebuffer(gl, *texture, None) {
						Some((framebuffer, is_framebuffer_complete)) => {
							framebuffer_list.push(framebuffer);
							is_complete = is_complete && is_framebuffer_complete;
						}
						None => {
							is_complete = false;
						}
					}
				}
			}
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, previous_framebuffer);
			if !is_complete {
				for framebuffer in framebuffer_list {
					crate::core::glstatecache::delete_framebuffer(gl, framebuffer);
				}
				for texture in texture_list {
					crate::core::glstatecache::delete_texture(gl, texture);
				}
				return false;
			}
			line_target.seed_texture = Some(texture_list[0]);
			line_target.seed_framebuffer = Some(framebuffer_list[0]);
			line_target.jump_flood_textures = vec![texture_list[1], texture_list[2]];
			line_target.jump_flood_framebuffers = vec![framebuffer_list[1], framebuffer_list[2]];
		}
		true
	}

	//==============================================================================
	/// 줄인 선의 판의 가는 선 덮는 몫 판 짓기. (RGBA8 곱한 색, 쌍선형, 처음 줄여 가는 선을 그을 때 한 번)
	///
	/// # 돌려줌
	/// 판이 있으면 true. (못 지으면 false, 그때 가는 선은 섞기 한 패스 길로 간다)
	//==============================================================================
	fn read_coverage_target(gl: &GlContext, line_target: &mut ToonLineTarget) -> bool {
		if line_target.coverage_texture.is_some() {
			return true;
		}
		unsafe {
			let previous_framebuffer = crate::graphics3d::read_framebuffer_binding(gl);
			let previous_texture = crate::graphics3d::read_texture_binding(gl);
			let texture = match ToonScreenOutline::create_target_texture(gl, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, line_target.width, line_target.height) {
				Some(texture) => {
					texture
				}
				None => {
					crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, previous_texture);
					return false;
				}
			};
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, previous_texture);
			let framebuffer = ToonScreenOutline::create_target_framebuffer(gl, texture, None);
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, previous_framebuffer);
			match framebuffer {
				Some((framebuffer, true)) => {
					line_target.coverage_framebuffer = Some(framebuffer);
					line_target.coverage_texture = Some(texture);
					true
				}
				Some((framebuffer, false)) => {
					crate::core::glstatecache::delete_framebuffer(gl, framebuffer);
					crate::core::glstatecache::delete_texture(gl, texture);
					false
				}
				None => {
					crate::core::glstatecache::delete_texture(gl, texture);
					false
				}
			}
		}
	}

	//==============================================================================
	/// 선의 판 지우기.
	//==============================================================================
	fn delete_line_target(&mut self, gl: &GlContext) {
		if let Some(line_target) = self.line_target.take() {
			ToonScreenOutline::delete_target_objects(gl, &line_target);
		}
	}

	//==============================================================================
	/// 선의 판의 GL 객체 지우기. (굵은 선과 덮는 몫 판은 지었으면)
	//==============================================================================
	fn delete_target_objects(gl: &GlContext, line_target: &ToonLineTarget) {
		unsafe {
			crate::core::glstatecache::delete_framebuffer(gl, line_target.framebuffer);
			crate::core::glstatecache::delete_framebuffer(gl, line_target.edge_mask_framebuffer);
			crate::core::glstatecache::delete_texture(gl, line_target.line_texture);
			crate::core::glstatecache::delete_texture(gl, line_target.depth_texture);
			crate::core::glstatecache::delete_texture(gl, line_target.edge_mask_texture);
			if let Some(seed_framebuffer) = line_target.seed_framebuffer {
				crate::core::glstatecache::delete_framebuffer(gl, seed_framebuffer);
			}
			if let Some(seed_texture) = line_target.seed_texture {
				crate::core::glstatecache::delete_texture(gl, seed_texture);
			}
			for jump_flood_framebuffer in line_target.jump_flood_framebuffers.iter() {
				crate::core::glstatecache::delete_framebuffer(gl, *jump_flood_framebuffer);
			}
			for jump_flood_texture in line_target.jump_flood_textures.iter() {
				crate::core::glstatecache::delete_texture(gl, *jump_flood_texture);
			}
			if let Some(coverage_framebuffer) = line_target.coverage_framebuffer {
				crate::core::glstatecache::delete_framebuffer(gl, coverage_framebuffer);
			}
			if let Some(coverage_texture) = line_target.coverage_texture {
				crate::core::glstatecache::delete_texture(gl, coverage_texture);
			}
		}
	}

	//==============================================================================
	/// 그리기 상태 적어 두기.
	//==============================================================================
	fn save_state(&mut self, gl: &GlContext) {
		unsafe {
			let mut viewport = [0i32; 4];
			crate::core::glstatecache::get_parameter_i32_slice(gl, glow::VIEWPORT, &mut viewport);
			let mut clear_color = [0.0f32; 4];
			crate::core::glstatecache::get_parameter_f32_slice(gl, glow::COLOR_CLEAR_VALUE, &mut clear_color);
			self.saved_state = Some(SavedState {
				is_blend_enabled: crate::core::glstatecache::is_enabled(gl, glow::BLEND),
				is_depth_test_enabled: crate::core::glstatecache::is_enabled(gl, glow::DEPTH_TEST),
				is_cull_face_enabled: crate::core::glstatecache::is_enabled(gl, glow::CULL_FACE),
				is_scissor_test_enabled: crate::core::glstatecache::is_enabled(gl, glow::SCISSOR_TEST),
				depth_mask: crate::core::glstatecache::get_parameter_bool(gl, glow::DEPTH_WRITEMASK),
				depth_function: crate::core::glstatecache::get_parameter_i32(gl, glow::DEPTH_FUNC),
				front_face: crate::core::glstatecache::get_parameter_i32(gl, glow::FRONT_FACE),
				blend_source_rgb: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_SRC_RGB),
				blend_destination_rgb: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_DST_RGB),
				blend_source_alpha: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_SRC_ALPHA),
				blend_destination_alpha: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_DST_ALPHA),
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
	fn restore_state(&mut self, gl: &GlContext) {
		let saved_state = match self.saved_state.take() {
			Some(saved_state) => {
				saved_state
			}
			None => {
				return;
			}
		};
		unsafe {
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, saved_state.framebuffer);
			crate::core::glstatecache::viewport(gl, saved_state.viewport[0], saved_state.viewport[1], saved_state.viewport[2], saved_state.viewport[3]);
			crate::core::glstatecache::clear_color(gl, saved_state.clear_color[0], saved_state.clear_color[1], saved_state.clear_color[2], saved_state.clear_color[3]);
			crate::core::glstatecache::bind_vertex_array(gl, saved_state.vertex_array);
			crate::core::glstatecache::use_program(gl, saved_state.program);
			crate::core::glstatecache::depth_mask(gl, saved_state.depth_mask);
			crate::core::glstatecache::depth_func(gl, saved_state.depth_function as u32);
			crate::core::glstatecache::front_face(gl, saved_state.front_face as u32);
			crate::core::glstatecache::cull_face(gl, glow::BACK);
			crate::core::glstatecache::blend_func_separate(gl, saved_state.blend_source_rgb as u32, saved_state.blend_destination_rgb as u32, saved_state.blend_source_alpha as u32, saved_state.blend_destination_alpha as u32);
			for (capability, is_enabled) in [(glow::BLEND, saved_state.is_blend_enabled), (glow::DEPTH_TEST, saved_state.is_depth_test_enabled), (glow::CULL_FACE, saved_state.is_cull_face_enabled), (glow::SCISSOR_TEST, saved_state.is_scissor_test_enabled)] {
				if is_enabled {
					crate::core::glstatecache::enable(gl, capability);
				}
				else {
					crate::core::glstatecache::disable(gl, capability);
				}
			}
		}
	}

	//==============================================================================
	/// 다 쓴 뒤 지우기. (선의 판, 버텍스 어레이, 프로그램 아홉)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&mut self, gl: &GlContext) {
		self.delete_line_target(gl);
		self.joint_block_buffer.destroy(gl);
		for slot in self.batch_slots.drain(..) {
			unsafe {
				for (_, vertex_array) in slot.vertex_arrays {
					crate::core::glstatecache::delete_vertex_array(gl, vertex_array);
				}
				crate::core::glstatecache::delete_buffer(gl, slot.buffer);
			}
		}
		unsafe {
			if let Some(line_color_texture) = self.line_color_texture.take() {
				crate::core::glstatecache::delete_texture(gl, line_color_texture);
			}
			crate::core::glstatecache::delete_vertex_array(gl, self.empty_vertex_array);
			crate::core::glstatecache::delete_program(gl, self.line_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.seed_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.seed_gather_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.jump_flood_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.composite_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.edge_mask_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.thin_composite_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.thin_coverage_program.get_program());
			crate::core::glstatecache::delete_program(gl, self.coverage_blit_program.get_program());
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
			let steps = read_jump_flood_steps(reach as f64);
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

// 번호 셰이더의 어트리뷰트 자리. (툰 몸 어트리뷰트 자리에 인스턴스 버퍼의 선 번호 자리를 더한 것)
fn read_line_attribute_locations() -> Vec<(&'static str, u32)> {
	let mut locations: Vec<(&'static str, u32)> = TOON_ATTRIBUTE_LOCATIONS.to_vec();
	locations.push(("instanceLineId", TOON_INSTANCE_LINE_ID_LOCATION));
	locations
}

// @shaders-begin
const LINE_VERTEX_SHADER: &str = concat!("#version 300 es
in vec3 vertexPosition;
in vec3 vertexNormal;
in vec4 vertexJoints;
in vec4 vertexWeights;
in vec4 instanceMatrix0;
in vec4 instanceMatrix1;
in vec4 instanceMatrix2;
in vec4 instanceMatrix3;
in vec4 instanceTint;
in float instanceLineId;
", joint_block_glsl_source!(), "
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
	mat4 objectMatrix = instanced > 0.5 ? mat4(vec4(instanceMatrix0.xyz, 0.0), vec4(instanceMatrix1.xyz, 0.0), vec4(instanceMatrix2.xyz, 0.0), vec4(instanceMatrix3.xyz, 1.0)) : modelMatrix;
	worldNormal = transpose(inverse(mat3(objectMatrix) * mat3(skinMatrix))) * vertexNormal;
	surfaceLineId = instanced < 0.5 || instanceLineIds < 0.5 ? lineId : (instanceLineIds < 1.5 ? instanceTint.x : instanceLineId);
	gl_Position = viewProjectionMatrix * objectMatrix * skinMatrix * vec4(vertexPosition, 1.0);
}
");

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

// 씨앗을 고르는 셰이더. (화소와 바로 이웃 넷 사이의 경계마다 선인지 보고, 선이면 그 경계의 가운데를 씨앗으로 쓴다)
// - 씨앗은 반 화소 단위 자리다. (x 가 홀수면 가로 이웃 사이의 세로 경계, y 가 홀수면 세로 이웃 사이의 가로 경계) rg 는 바깥 선, ba 는 꺾임 선.
// - 같은 경계를 양쪽 화소가 함께 씨앗으로 들어도 선은 경계 한가운데 하나다. (선을 화소가 아니라 화소 사이에 둔다)
// - 바깥 선: 한쪽이 빈 곳, 번호가 다름, 같은 번호라도 가린 화소가 가려진 화소의 접평면에서 깊이 문턱과 오차 여유를 더한 것보다 떨어짐. (가린 쪽의 꼴을 따르고, 맞닿은 두 번호는 한쪽만 켜져 있어도 긋는다)
//   가려진 쪽의 접평면만 쓴다. 가린 쪽이 둥근 면의 실루엣이면 그 접평면이 시선을 품어, 뒤의 면이 얼마나 떨어져 있든 거리가 화소 한 칸 밑으로 줄기 때문이다.
//   오차 여유는 이어진 면에서도 생기는 거리의 오차를 잰 것이다: 노멀 8 비트 팔면체의 각 오차 x 두 자리 사이 거리(면이 비스듬할수록 길다), 꺾인 줄을 사이에 둔 두 화소의 화소 한 칸 x 두 노멀이 벌어진 사인, 깊이 24 비트 한 칸.
//   거기에 여유 배율(setDepthMargin)을 곱한다. 두 면이 나란한 턱은 꺾임 몫이 0 이라 화소 한 칸보다 낮아도 잡힌다.
// - 꺾임 선: 같은 번호이고 떨어지지 않았는데 두 노멀이 꺾임 각보다 더 벌어짐. (비스듬한 면과 둥근 면은 접평면 안이라 바깥 선으로 잡히지 않는다)
// - 경계를 보는 함수(readPixel, readEdge)는 가는 선의 씨앗 쪽 판 셰이더와 같이 쓴다. (LINE_EDGE_SHADER_PART)
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
uniform float depthMargin;
uniform float creaseCosine;
const float NORMAL_ERROR = 0.0165;
const float DEPTH_STEP = 1.0 / 16777215.0;
const ivec2 SIDES[4] = ivec2[4](ivec2(1, 0), ivec2(-1, 0), ivec2(0, 1), ivec2(0, -1));
struct LinePixel {
	float id;
	float flags;
	vec3 normal;
	vec3 position;
	float footprint;
	float depthError;
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
		result.depthError = DEPTH_STEP * viewZ * viewZ / abs(projectionDepth.y) * length(result.position) / max(-viewZ, 1e-6);
	}
	else {
		float viewZ = (clipDepth - projectionDepth.y) / projectionDepth.x;
		result.position = vec3((clipPoint.x - projectionDepth.z) / projectionScale.x, (clipPoint.y - projectionDepth.w) / projectionScale.y, viewZ);
		result.footprint = 2.0 / (projectionScale.y * size.y);
		result.depthError = DEPTH_STEP / abs(projectionDepth.x);
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
	LinePixel nearPixel = first;
	LinePixel farPixel = second;
	if (!isFirstNearer) {
		nearPixel = second;
		farPixel = first;
	}
	vec3 offset = nearPixel.position - farPixel.position;
	float planeDistance = abs(dot(offset, farPixel.normal));
	float pixelError = length(offset) * NORMAL_ERROR + nearPixel.footprint * length(cross(nearPixel.normal, farPixel.normal)) + nearPixel.depthError + farPixel.depthError;
	bool isSeparated = planeDistance > depthThreshold + depthMargin * pixelError;
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
out uvec4 fragmentSeed;
const uint NO_SEED = 65535u;
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

// 가는 선의 씨앗 쪽 판을 고르는 셰이더. (씨앗 셰이더와 같은 차례로 화소마다 바깥 선과 꺾임 선의 첫 경계 쪽을 고르고, 씨앗 자리 대신 쪽 번호만 담는다)
// - 아래 3 비트는 바깥 선, 그 위 3 비트는 꺾임 선의 쪽이다. 0 은 없음, 1 ~ 4 는 SIDES 차례(오른쪽, 왼쪽, 위, 아래)에서 처음 선이 선 쪽. (R8UI 한 바이트)
// - 씨앗은 화소 * 2 + 쪽이므로 씨앗 판과 같은 것을 한 바이트에 담는다. (씨앗 판은 RGBA16UI 8 바이트)
const EDGE_MASK_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D lineTexture;
uniform highp sampler2D depthTexture;
uniform mat3 viewRotation;
uniform vec4 projectionScale;
uniform vec4 projectionDepth;
uniform float isPerspective;
uniform float depthThreshold;
uniform float depthMargin;
uniform float creaseCosine;
const float NORMAL_ERROR = 0.0165;
const float DEPTH_STEP = 1.0 / 16777215.0;
const ivec2 SIDES[4] = ivec2[4](ivec2(1, 0), ivec2(-1, 0), ivec2(0, 1), ivec2(0, -1));
struct LinePixel {
	float id;
	float flags;
	vec3 normal;
	vec3 position;
	float footprint;
	float depthError;
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
		result.depthError = DEPTH_STEP * viewZ * viewZ / abs(projectionDepth.y) * length(result.position) / max(-viewZ, 1e-6);
	}
	else {
		float viewZ = (clipDepth - projectionDepth.y) / projectionDepth.x;
		result.position = vec3((clipPoint.x - projectionDepth.z) / projectionScale.x, (clipPoint.y - projectionDepth.w) / projectionScale.y, viewZ);
		result.footprint = 2.0 / (projectionScale.y * size.y);
		result.depthError = DEPTH_STEP / abs(projectionDepth.x);
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
	LinePixel nearPixel = first;
	LinePixel farPixel = second;
	if (!isFirstNearer) {
		nearPixel = second;
		farPixel = first;
	}
	vec3 offset = nearPixel.position - farPixel.position;
	float planeDistance = abs(dot(offset, farPixel.normal));
	float pixelError = length(offset) * NORMAL_ERROR + nearPixel.footprint * length(cross(nearPixel.normal, farPixel.normal)) + nearPixel.depthError + farPixel.depthError;
	bool isSeparated = planeDistance > depthThreshold + depthMargin * pixelError;
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
out uint fragmentMask;
void main() {
	ivec2 size = textureSize(lineTexture, 0);
	vec2 sizeValue = vec2(size);
	ivec2 pixel = ivec2(gl_FragCoord.xy);
	LinePixel center = readPixel(pixel, sizeValue);
	uint contourSide = 0u;
	uint creaseSide = 0u;
	for (int index = 0; index < 4; ++index) {
		ivec2 neighborPixel = pixel + SIDES[index];
		if (neighborPixel.x < 0 || neighborPixel.y < 0 || neighborPixel.x >= size.x || neighborPixel.y >= size.y) {
			continue;
		}
		int edge = readEdge(center, readPixel(neighborPixel, sizeValue));
		if (edge == 2 && contourSide == 0u) {
			contourSide = uint(index + 1);
		}
		if (edge == 1 && creaseSide == 0u) {
			creaseSide = uint(index + 1);
		}
	}
	fragmentMask = contourSide | (creaseSide << 3u);
}
";

// 온 해상도 씨앗을 넓히기 판으로 모으는 셰이더. (넓히기 판 한 칸이 덮는 씨앗 판 칸들 가운데 칸 가운데에 가장 가까운 씨앗을 고른다, 줄이지 않으면 그대로 옮김)
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

// 씨앗 판의 가까운 씨앗을 넓히는 점프 플러딩 한 번. (stepSize 떨어진 이웃 여덟과 자기를 보고 경계 토막까지 가장 가까운 씨앗을 남긴다)
// - 넓히기 판은 줄인 판일 수 있다. 칸 가운데를 그리는 영역 화소 좌표(pointScale 배)로 옮겨 재므로 씨앗 자리는 늘 그리는 영역 화소다.
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

// 그리는 영역의 화소마다 가까운 씨앗까지의 거리(그리는 영역 픽셀)로 선이 덮는 몫을 셈해 섞는 셰이더. (미리 곱한 알파)
// - 넓히기 판에서 둘레 2x2 칸의 씨앗을 보고 경계 토막까지의 거리를 그리는 영역 화소로 잰다. 씨앗이 그리는 영역 화소의 경계라 넓히기 판을 줄여도 계단과 점선이 없다.
// - 덮는 몫은 clamp(굵기 / 2 - 거리 + 0.5, 0, 1) 이다. (픽셀 상자로 거른 선, 가장자리가 부드럽다)
// - 번호 색 표를 켜면(lineColorEnabled) 가장 가까운 씨앗의 경계 양쪽 화소에서 앞쪽(깊이가 작은 쪽, 한쪽이 빈 곳이면 다른 쪽) 번호를 읽어 표에서 색을 찾는다.
//   표의 알파가 0 이면 표에 없는 번호라 선 색 그대로다. 바깥 선과 꺾임 선이 따로 찾고, 더 진한 쪽의 색을 쓴다.
// - 색 찾기, 거리, 덮는 몫을 색으로 쓰는 앞부분(COMPOSITE_SHADER_PART)은 가는 선 섞기와 같이 쓴다.
const COMPOSITE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D lineTexture;
uniform highp sampler2D depthTexture;
uniform highp sampler2D lineColorTexture;
uniform float lineColorEnabled;
uniform vec4 viewportRect;
uniform vec2 lineWidths;
uniform float creaseOpacity;
uniform vec4 outlineColor;
uniform float hdrOutput;
uniform vec2 lineScale; // 선의 판 화소 수 / 그리는 영역 화소 수. (1 이면 같은 해상도)
out vec4 fragmentColor;
const uint NO_SEED = 65535u;
float readLineId(ivec2 pixel) {
	vec4 value = texelFetch(lineTexture, pixel, 0);
	return floor(value.b * 255.0 + 0.5) + mod(floor(value.a * 255.0 + 0.5), 64.0) * 256.0;
}
vec4 readSeedColor(uvec2 seed) {
	if (seed.x == NO_SEED) {
		return outlineColor;
	}
	ivec2 size = textureSize(lineTexture, 0);
	ivec2 firstPixel = clamp(ivec2(seed >> 1u), ivec2(0), size - 1);
	ivec2 secondPixel = clamp(ivec2((seed + 1u) >> 1u), ivec2(0), size - 1);
	float firstId = readLineId(firstPixel);
	float secondId = readLineId(secondPixel);
	bool isFirstNearer = secondId < 0.5 || (firstId > 0.5 && texelFetch(depthTexture, firstPixel, 0).r <= texelFetch(depthTexture, secondPixel, 0).r);
	int id = int(isFirstNearer ? firstId : secondId);
	vec4 tableColor = texelFetch(lineColorTexture, ivec2(id % 128, id / 128), 0);
	return tableColor.a > 0.0 ? tableColor : outlineColor;
}
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
void writeLineColor(float contourDistance, float creaseDistance, uvec2 contourSeed, uvec2 creaseSeed) {
	float contour = lineWidths.x > 0.0 ? clamp(lineWidths.x * 0.5 - contourDistance + 0.5, 0.0, 1.0) : 0.0;
	float crease = lineWidths.y > 0.0 ? clamp(lineWidths.y * 0.5 - creaseDistance + 0.5, 0.0, 1.0) * creaseOpacity : 0.0;
	vec4 lineColor = outlineColor;
	float strength = max(contour, crease) * outlineColor.a;
	if (lineColorEnabled > 0.5 && max(contour, crease) > 0.0) {
		vec4 contourColor = contour > 0.0 ? readSeedColor(contourSeed) : outlineColor;
		vec4 creaseColor = crease > 0.0 ? readSeedColor(creaseSeed) : outlineColor;
		float contourStrength = contour * contourColor.a;
		float creaseStrength = crease * creaseColor.a;
		lineColor = contourStrength >= creaseStrength ? contourColor : creaseColor;
		strength = max(contourStrength, creaseStrength);
	}
	if (strength < 0.004) {
#ifdef VANILLA_COVERAGE_PASS
		fragmentColor = vec4(0.0);
		return;
#else
		discard;
#endif
	}
#ifdef VANILLA_COLOR_SPACE_LINEAR
	vec3 linearColor = max(lineColor.rgb, vec3(0.0));
	linearColor = mix(linearColor / 12.92, pow((linearColor + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), linearColor));
	vec3 color = hdrOutput > 0.5 ? linearColor : lineColor.rgb;
#else
	vec3 color = hdrOutput > 0.5 ? pow(lineColor.rgb, vec3(2.2)) : lineColor.rgb;
#endif
	fragmentColor = vec4(color * strength, strength);
}
uniform highp usampler2D seedTexture;
uniform vec2 pointScale;
void main() {
	ivec2 size = textureSize(seedTexture, 0);
	vec2 toOutput = 1.0 / lineScale;
	vec2 point = (gl_FragCoord.xy - viewportRect.xy) * lineScale - 0.5;
	ivec2 basePixel = ivec2(floor((point + 0.5) / pointScale - 0.5));
	float contourDistance = 1.0e9;
	float creaseDistance = 1.0e9;
	uvec2 contourSeed = uvec2(NO_SEED);
	uvec2 creaseSeed = uvec2(NO_SEED);
	for (int offsetY = 0; offsetY <= 1; ++offsetY) {
		for (int offsetX = 0; offsetX <= 1; ++offsetX) {
			ivec2 samplePixel = clamp(basePixel + ivec2(offsetX, offsetY), ivec2(0), size - 1);
			uvec4 seed = texelFetch(seedTexture, samplePixel, 0);
			float sampleContourDistance = readSeedDistance(point, seed.xy, toOutput);
			if (sampleContourDistance < contourDistance) {
				contourDistance = sampleContourDistance;
				contourSeed = seed.xy;
			}
			float sampleCreaseDistance = readSeedDistance(point, seed.zw, toOutput);
			if (sampleCreaseDistance < creaseDistance) {
				creaseDistance = sampleCreaseDistance;
				creaseSeed = seed.zw;
			}
		}
	}
	writeLineColor(contourDistance, creaseDistance, contourSeed, creaseSeed);
}
";

// 가는 선을 한 패스로 섞는 셰이더. (점프 플러딩 없이 둘레 3 x 3 화소의 씨앗을 점프 플러딩 마지막 걸음과 같은 차례로 보고 가장 가까운 것까지의 거리로 덮는 몫을 셈한다)
// - 화소 q 의 씨앗은 q * 2 + 쪽이다. 아래 줄부터, 줄 안은 왼쪽부터 보고 거리가 더 작을 때만 바꾼다. (점프 플러딩 걸음 1 과 같은 차례, 같은 거리면 먼저 본 것)
//   그래서 선 색이 다른 두 번호가 맞닿아 같은 거리 토막이 여럿인 자리도 점프 플러딩 길과 거의 같은 쪽을 고른다. (앞 걸음에서 씨앗을 넘겨받은 빈 화소만 다를 수 있다)
// - 씨앗에서 화소까지의 자리는 고리 칸마다 정해진 반 정수라 거리가 정확하다. 거리는 제곱으로 견주고 끝에 한 번 sqrt 한다. 고르기는 갈래 없이 고른다.
// - 색 찾기와 덮는 몫은 점프 플러딩 길과 같은 함수다.
const THIN_COMPOSITE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D lineTexture;
uniform highp sampler2D depthTexture;
uniform highp sampler2D lineColorTexture;
uniform float lineColorEnabled;
uniform vec4 viewportRect;
uniform vec2 lineWidths;
uniform float creaseOpacity;
uniform vec4 outlineColor;
uniform float hdrOutput;
uniform vec2 lineScale; // 선의 판 화소 수 / 그리는 영역 화소 수. (1 이면 같은 해상도)
out vec4 fragmentColor;
const uint NO_SEED = 65535u;
float readLineId(ivec2 pixel) {
	vec4 value = texelFetch(lineTexture, pixel, 0);
	return floor(value.b * 255.0 + 0.5) + mod(floor(value.a * 255.0 + 0.5), 64.0) * 256.0;
}
vec4 readSeedColor(uvec2 seed) {
	if (seed.x == NO_SEED) {
		return outlineColor;
	}
	ivec2 size = textureSize(lineTexture, 0);
	ivec2 firstPixel = clamp(ivec2(seed >> 1u), ivec2(0), size - 1);
	ivec2 secondPixel = clamp(ivec2((seed + 1u) >> 1u), ivec2(0), size - 1);
	float firstId = readLineId(firstPixel);
	float secondId = readLineId(secondPixel);
	bool isFirstNearer = secondId < 0.5 || (firstId > 0.5 && texelFetch(depthTexture, firstPixel, 0).r <= texelFetch(depthTexture, secondPixel, 0).r);
	int id = int(isFirstNearer ? firstId : secondId);
	vec4 tableColor = texelFetch(lineColorTexture, ivec2(id % 128, id / 128), 0);
	return tableColor.a > 0.0 ? tableColor : outlineColor;
}
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
void writeLineColor(float contourDistance, float creaseDistance, uvec2 contourSeed, uvec2 creaseSeed) {
	float contour = lineWidths.x > 0.0 ? clamp(lineWidths.x * 0.5 - contourDistance + 0.5, 0.0, 1.0) : 0.0;
	float crease = lineWidths.y > 0.0 ? clamp(lineWidths.y * 0.5 - creaseDistance + 0.5, 0.0, 1.0) * creaseOpacity : 0.0;
	vec4 lineColor = outlineColor;
	float strength = max(contour, crease) * outlineColor.a;
	if (lineColorEnabled > 0.5 && max(contour, crease) > 0.0) {
		vec4 contourColor = contour > 0.0 ? readSeedColor(contourSeed) : outlineColor;
		vec4 creaseColor = crease > 0.0 ? readSeedColor(creaseSeed) : outlineColor;
		float contourStrength = contour * contourColor.a;
		float creaseStrength = crease * creaseColor.a;
		lineColor = contourStrength >= creaseStrength ? contourColor : creaseColor;
		strength = max(contourStrength, creaseStrength);
	}
	if (strength < 0.004) {
#ifdef VANILLA_COVERAGE_PASS
		fragmentColor = vec4(0.0);
		return;
#else
		discard;
#endif
	}
#ifdef VANILLA_COLOR_SPACE_LINEAR
	vec3 linearColor = max(lineColor.rgb, vec3(0.0));
	linearColor = mix(linearColor / 12.92, pow((linearColor + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), linearColor));
	vec3 color = hdrOutput > 0.5 ? linearColor : lineColor.rgb;
#else
	vec3 color = hdrOutput > 0.5 ? pow(lineColor.rgb, vec3(2.2)) : lineColor.rgb;
#endif
	fragmentColor = vec4(color * strength, strength);
}
uniform highp usampler2D edgeMaskTexture;
const vec2 SIDE_OFFSETS[5] = vec2[5](vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0));
float readSideDistance(vec2 base, vec2 side) {
	vec2 offset = max(abs(base + side * 0.5) - abs(side.yx) * 0.5, vec2(0.0));
#ifdef VANILLA_COVERAGE_PASS
	offset *= 1.0 / lineScale;
#endif
	return dot(offset, offset);
}
void main() {
	ivec2 size = textureSize(lineTexture, 0);
#ifdef VANILLA_COVERAGE_PASS
	vec2 point = gl_FragCoord.xy - 0.5;
#else
	vec2 point = gl_FragCoord.xy - viewportRect.xy - 0.5;
#endif
	ivec2 pixel = ivec2(floor(point + 0.5));
	float contourDistance = 1.0e18;
	float creaseDistance = 1.0e18;
	uvec2 contourSeed = uvec2(NO_SEED);
	uvec2 creaseSeed = uvec2(NO_SEED);
	for (int offsetY = -1; offsetY <= 1; ++offsetY) {
		for (int offsetX = -1; offsetX <= 1; ++offsetX) {
			ivec2 wantedPixel = pixel + ivec2(offsetX, offsetY);
			ivec2 samplePixel = clamp(wantedPixel, ivec2(0), size - 1);
			uint sides = texelFetch(edgeMaskTexture, samplePixel, 0).r;
			sides = samplePixel == wantedPixel ? sides : 0u;
			uint contourSide = sides & 7u;
			uint creaseSide = (sides >> 3u) & 7u;
			vec2 base = vec2(samplePixel) - point;
			vec2 contourOffset = SIDE_OFFSETS[contourSide];
			vec2 creaseOffset = SIDE_OFFSETS[creaseSide];
			float sampleContourDistance = readSideDistance(base, contourOffset);
			float sampleCreaseDistance = readSideDistance(base, creaseOffset);
			bool isTaken = contourSide != 0u && sampleContourDistance < contourDistance;
			contourDistance = isTaken ? sampleContourDistance : contourDistance;
			contourSeed = isTaken ? uvec2(samplePixel * 2 + ivec2(contourOffset)) : contourSeed;
			isTaken = creaseSide != 0u && sampleCreaseDistance < creaseDistance;
			creaseDistance = isTaken ? sampleCreaseDistance : creaseDistance;
			creaseSeed = isTaken ? uvec2(samplePixel * 2 + ivec2(creaseOffset)) : creaseSeed;
		}
	}
	writeLineColor(sqrt(contourDistance), sqrt(creaseDistance), contourSeed, creaseSeed);
}
";

// 줄인 선의 판의 덮는 몫 판(RGBA8, 곱한 색)을 그리는 영역에 쌍선형으로 한 번 읽어 섞는 셰이더. (선의 판을 줄였을 때의 가는 선 길의 마지막 패스, 읽기 한 번)
const COVERAGE_BLIT_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
uniform highp sampler2D coverageTexture;
uniform vec4 viewportRect;
out vec4 fragmentColor;
void main() {
	vec4 color = texture(coverageTexture, (gl_FragCoord.xy - viewportRect.xy) / viewportRect.zw);
	if (color.a < 0.004) {
		discard;
	}
	fragmentColor = color;
}
";

// @shaders-end
