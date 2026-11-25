//==============================================================================
// 툰 화면 공간 바깥선. (vanilla.js experimental/graphics/toon/toonscreenoutline.js 의 ToonScreenOutline)
// - 노멀, 선 번호, 깊이를 선의 판에 그리고 한 번 훑어 가면을 쓴 뒤 화면에 섞는다.
// - ToonRenderer 의 create_mesh 결과를 같은 행렬로 한 번 더 그려 번호를 쓴다. 껍데기 바깥선(ToonRenderer)과 함께 써도 된다.
// - 선이 서는 곳: 앞에 있는 것의 실루엣 바깥, 다른 번호와 맞닿은 곳(먼 쪽, 깊이가 거의 같으면 번호가 큰 쪽), 한 번호 안에서 면이 꺾임 각보다 더 꺾이는 곳.
//   같은 번호끼리 맞닿은 곳은 긋지 않는다. 어느 모형, 어느 부품(index_start, index_count 범위)에 어떤 번호를 줄지는 부르는 쪽이 정한다.
// - 굵기: set_width 로 월드 길이나 픽셀. 선의 판 픽셀로 셈해 한 픽셀 밑으로 내리지 않는다.
// - 깊이: 투영 행렬로 직교와 원근을 가린다. 원근은 깊이를 가까운 면과 먼 면으로 풀어 거리(미터)로 견준다.
// - 값: set_resolution_scale 로 선의 판을 그리는 영역의 몫으로 줄이고(가면은 섞을 때 부드럽게 늘임), set_sample_count 로 볼 이웃 수를 줄인다(가벼운 길 4, 2).
// - 사용: 매 프레임 begin_line_ids(gl, 보기, 투영) → draw_mesh / draw_mesh_instanced → end_line_ids(gl) (가면까지 셈) → 툰 몸 그리기 → draw_outline(gl).
//   begin_line_ids 는 그때의 뷰포트를 그리는 영역으로 적어 두고, 선의 판을 그 크기에 맞춰 짓는다. end_line_ids 가 프레임버퍼, 뷰포트, 상태를 돌린다.
//   (엔진 2D 그리기 사이에 끼울 때, 그 전에 Graphic 의 묶음을 내보내고 끝에 restore_render_state)
// - 인스턴스마다의 번호는 ToonInstance 의 line_id 다. (원본 인스턴스 객체의 lineId, None 이면 공통 번호)
//==============================================================================
use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::core::graphic::{GlProgram, GlTexture, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::toon::toonrenderer::{ToonInstance, ToonMesh, TOON_ATTRIBUTE_LOCATIONS, TOON_INSTANCE_FLOAT_COUNT, TOON_MAXIMUM_JOINTS};
use crate::graphics3d::GlFramebuffer;

// 볼 이웃의 수 상한. (셰이더의 OFFSETS 를 가까운 차례로, 3 픽셀 안)
pub const TOON_SCREEN_OUTLINE_MAXIMUM_SAMPLES: u32 = 28;

// 꺾임 선을 볼 곧은 쪽의 수 상한.
pub const TOON_SCREEN_OUTLINE_MAXIMUM_CREASE_SAMPLES: u32 = 4;

// 선의 판과 가면을 묶는 텍스처 유닛. (툰 렌더러의 그림자 유닛 3 과 겹치지 않게)
const LINE_TEXTURE_UNIT: u32 = 5;
const DEPTH_TEXTURE_UNIT: u32 = 6;

//==============================================================================
// 굵기의 단위.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonOutlineWidthUnit {
	// 월드 길이. (직교는 투영의 세로 배율로, 원근은 초점 거리에서 화면 픽셀로 셈)
	World,
	// 그리는 버퍼의 픽셀.
	Pixel,
}

//==============================================================================
// 번호를 그릴 때의 꼴.
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonLineDrawOptions<'a> {
	pub joint_matrices: Option<&'a [f32]>,
	pub is_culled: bool,
	// 몸 번호 버퍼의 일부만. (부품에 다른 번호를 줄 때, 시작 번호와 수, 수가 None 이면 끝까지)
	pub index_start: usize,
	pub index_count: Option<usize>,
}

impl Default for ToonLineDrawOptions<'_> {
	fn default() -> Self {
		ToonLineDrawOptions { joint_matrices: None, is_culled: true, index_start: 0, index_count: None }
	}
}

//==============================================================================
// 선의 판. (노멀과 번호 RGBA8 과 깊이 24, 가면 R8)
//==============================================================================
pub struct ToonLineTarget {
	pub framebuffer: GlFramebuffer,
	pub line_texture: GlTexture,
	pub depth_texture: GlTexture,
	pub mask_framebuffer: GlFramebuffer,
	pub mask_texture: GlTexture,
	pub width: i32,
	pub height: i32,
}

//==============================================================================
// 이번 프레임의 카메라.
//==============================================================================
#[derive(Clone, Copy)]
struct LineFrame {
	view_matrix: Matrix4,
	frustum_planes: [[f32; 4]; 6],
	view_direction: [f32; 3],
	is_perspective: bool,
	depth_range: [f32; 2],
	projection_scale: f32,
	viewport: [i32; 4],
}

//==============================================================================
// 앞의 그리기 상태.
//==============================================================================
struct SavedState {
	is_blend_enabled: bool,
	is_depth_test_enabled: bool,
	is_cull_face_enabled: bool,
	is_scissor_test_enabled: bool,
	depth_mask: bool,
	depth_function: i32,
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

pub struct ToonScreenOutline {
	line_program: ShaderProgram,
	mask_program: ShaderProgram,
	composite_program: ShaderProgram,
	empty_vertex_array: GlVertexArray,
	line_target: Option<ToonLineTarget>,
	color: [f32; 4],
	width: f32,
	width_unit: ToonOutlineWidthUnit,
	focus_distance: f32,
	depth_threshold: f32,
	contact_tolerance: f32,
	crease_cosine: f32,
	crease_strength: f32,
	crease_extent: f32,
	resolution_scale: f32,
	sample_count: u32,
	crease_sample_count: u32,
	is_hdr_output: bool,
	frame: Option<LineFrame>,
	saved_state: Option<SavedState>,
	is_drawing_line_ids: bool,
	instance_data: Vec<f32>,
	drawn_distance_sum: f32,
	drawn_distance_count: u32,
	outline_radius: f32,
}

//==============================================================================
// f32 배열을 바이트로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
// 보기와 투영 행렬에서 절두체의 면 여섯. (ToonRenderer 와 같은 셈, ax + by + cz + d >= 0 이 안쪽)
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
// 투영 행렬에서 원근인지, 깊이 범위(가까운 면, 먼 면), 세로 배율. (Matrix4 의 create_perspective, create_orthographic 꼴)
//==============================================================================
fn read_projection(projection_matrix: &Matrix4) -> (bool, [f32; 2], f32) {
	let elements = &projection_matrix.elements;
	let is_perspective = elements[11].abs() > 0.5;
	let depth_range = if is_perspective {
		[elements[14] / (elements[10] - 1.0), elements[14] / (elements[10] + 1.0)]
	}
	else {
		let depth_span = -2.0 / elements[10];
		let depth_center = elements[14] / elements[10];
		[depth_center - depth_span * 0.5, depth_center + depth_span * 0.5]
	};
	(is_perspective, depth_range, elements[5].abs())
}

//==============================================================================
// 바깥선 반지름. (선의 판 픽셀, 한 픽셀 밑으로 내리지 않음, 원근의 월드 굵기는 초점 거리에서)
//==============================================================================
fn compute_outline_radius(width: f32, width_unit: ToonOutlineWidthUnit, viewport_height: i32, line_height: i32, projection_scale: f32, is_perspective: bool, focus_distance: f32) -> f32 {
	let scale = line_height as f32 / viewport_height.max(1) as f32;
	if width_unit == ToonOutlineWidthUnit::Pixel {
		return (width * scale).max(1.0);
	}
	let mut pixels_per_unit = viewport_height as f32 * 0.5 * projection_scale;
	if is_perspective {
		pixels_per_unit /= focus_distance.max(0.001);
	}
	(width * pixels_per_unit * scale).max(1.0)
}

impl ToonScreenOutline {
	//==============================================================================
	// 셰이더 소스. ((정점, 조각) 세 벌: 번호, 선 찾기, 섞기)
	//==============================================================================
	pub fn read_program_sources() -> [(&'static str, &'static str); 3] {
		[(LINE_VERTEX_SHADER, LINE_FRAGMENT_SHADER), (SCREEN_TRIANGLE_VERTEX_SHADER, LINE_MASK_FRAGMENT_SHADER), (SCREEN_TRIANGLE_VERTEX_SHADER, COMPOSITE_FRAGMENT_SHADER)]
	}

	//==============================================================================
	// 생성. (세 프로그램과 빈 버텍스 어레이를 짓는다)
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<ToonScreenOutline, String> {
		let line_program = ShaderProgram::create_with_attribute_locations(gl, LINE_VERTEX_SHADER, LINE_FRAGMENT_SHADER, &TOON_ATTRIBUTE_LOCATIONS)?;
		let mask_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, LINE_MASK_FRAGMENT_SHADER)?;
		let composite_program = ShaderProgram::new(gl, SCREEN_TRIANGLE_VERTEX_SHADER, COMPOSITE_FRAGMENT_SHADER)?;
		let empty_vertex_array = unsafe { gl.create_vertex_array()? };
		Ok(ToonScreenOutline {
			line_program,
			mask_program,
			composite_program,
			empty_vertex_array,
			line_target: None,
			color: [0.08, 0.07, 0.1, 1.0],
			width: 1.5,
			width_unit: ToonOutlineWidthUnit::Pixel,
			focus_distance: 0.0,
			depth_threshold: 0.05,
			contact_tolerance: 0.01,
			crease_cosine: (std::f32::consts::PI / 3.0).cos(),
			crease_strength: 0.5,
			crease_extent: 2.0,
			resolution_scale: 1.0,
			sample_count: TOON_SCREEN_OUTLINE_MAXIMUM_SAMPLES,
			crease_sample_count: TOON_SCREEN_OUTLINE_MAXIMUM_CREASE_SAMPLES,
			is_hdr_output: false,
			frame: None,
			saved_state: None,
			is_drawing_line_ids: false,
			instance_data: Vec::new(),
			drawn_distance_sum: 0.0,
			drawn_distance_count: 0,
			outline_radius: 1.0,
		})
	}

	//==============================================================================
	// 선 색 설정. (sRGB 0 ~ 1 과 알파)
	//==============================================================================
	pub fn set_color(&mut self, color: [f32; 4]) {
		self.color = color;
	}

	//==============================================================================
	// 굵기 설정. (월드 길이나 픽셀, 반지름이 아니라 선 하나의 굵기)
	//==============================================================================
	pub fn set_width(&mut self, width: f32, unit: ToonOutlineWidthUnit) {
		self.width = width.max(0.0);
		self.width_unit = unit;
	}

	//==============================================================================
	// 초점 거리 설정. (원근 카메라에서 월드 굵기를 픽셀로 셀 거리, 보통 카메라에서 보는 점까지, 0 이면 그린 모형들의 평균 거리)
	//==============================================================================
	pub fn set_focus_distance(&mut self, focus_distance: f32) {
		self.focus_distance = focus_distance.max(0.0);
	}

	//==============================================================================
	// 깊이 문턱 설정. (실루엣 깊이 차이와, 다른 번호와 맞닿은 두 면을 번호가 큰 쪽에만 긋는 깊이 차이, 월드 길이)
	//==============================================================================
	pub fn set_depth_threshold(&mut self, depth_threshold: f32, contact_tolerance: f32) {
		self.depth_threshold = depth_threshold.max(0.0);
		self.contact_tolerance = contact_tolerance.max(0.0);
	}

	//==============================================================================
	// 꺾임 선 설정. (면이 이 각(도)보다 더 꺾이면 선, 걸린 곳 하나의 진하기, 꺾인 면이 더 이어져야 할 픽셀, 180 도면 긋지 않음)
	//==============================================================================
	pub fn set_crease(&mut self, angle_degrees: f32, strength: f32, extent_pixels: f32) {
		self.crease_cosine = angle_degrees.clamp(0.0, 180.0).to_radians().cos();
		self.crease_strength = strength.max(0.0);
		self.crease_extent = extent_pixels.max(0.0);
	}

	//==============================================================================
	// 선의 판 해상도 몫 설정. (그리는 영역에 대한 몫 0.25 ~ 1)
	//==============================================================================
	pub fn set_resolution_scale(&mut self, resolution_scale: f32) {
		self.resolution_scale = resolution_scale.clamp(0.25, 1.0);
	}

	//==============================================================================
	// 볼 이웃 수 설정. (바깥선 4 ~ 28, 꺾임 선 0 ~ 4, 가벼운 길은 4 와 2)
	//==============================================================================
	pub fn set_sample_count(&mut self, sample_count: u32, crease_sample_count: u32) {
		self.sample_count = sample_count.clamp(4, TOON_SCREEN_OUTLINE_MAXIMUM_SAMPLES);
		self.crease_sample_count = crease_sample_count.min(TOON_SCREEN_OUTLINE_MAXIMUM_CREASE_SAMPLES);
	}

	//==============================================================================
	// HDR 출력 설정. (true 면 선 색을 선형으로 바꿔 섞는다, ToonRenderer::set_hdr_output 과 맞춘다)
	//==============================================================================
	pub fn set_hdr_output(&mut self, is_hdr_output: bool) {
		self.is_hdr_output = is_hdr_output;
	}

	//==============================================================================
	// 이번 프레임의 바깥선 반지름. (선의 판 픽셀, end_line_ids 뒤에 정해짐)
	//==============================================================================
	pub fn get_outline_radius(&self) -> f32 {
		self.outline_radius
	}

	//==============================================================================
	// 선의 판 반환. (없으면 None, 번호와 깊이와 가면 텍스처, 크기)
	//==============================================================================
	pub fn get_line_target(&self) -> Option<&ToonLineTarget> {
		self.line_target.as_ref()
	}

	//==============================================================================
	// 바깥선 반지름 셈. (선의 판 픽셀, 한 픽셀 밑으로 내리지 않음, 번호를 그리는 프레임이나 선의 판이 없으면 1)
	//==============================================================================
	pub fn read_outline_radius(&self) -> f32 {
		let (frame, line_target) = match (self.frame.as_ref(), self.line_target.as_ref()) {
			(Some(frame), Some(line_target)) => {
				(frame, line_target)
			}
			_ => {
				return 1.0;
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
		compute_outline_radius(self.width, self.width_unit, frame.viewport[3], line_target.height, frame.projection_scale, frame.is_perspective, focus_distance)
	}

	//==============================================================================
	// 번호 그리기 시작. (지금 뷰포트를 그리는 영역으로 적고, 선의 판을 묶어 비운다, 판을 짓지 못하면 false)
	//==============================================================================
	pub fn begin_line_ids(&mut self, gl: &glow::Context, view_matrix: &Matrix4, projection_matrix: &Matrix4) -> bool {
		let mut viewport = [0i32; 4];
		unsafe {
			gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
		}
		let mut view_projection_matrix = *projection_matrix;
		view_projection_matrix.multiply(view_matrix);
		let (is_perspective, depth_range, projection_scale) = read_projection(projection_matrix);
		let view_elements = &view_matrix.elements;
		let view_direction = [-view_elements[2], -view_elements[6], -view_elements[10]];
		let view_direction_length = (view_direction[0] * view_direction[0] + view_direction[1] * view_direction[1] + view_direction[2] * view_direction[2]).sqrt().max(0.000001);
		self.frame = Some(LineFrame {
			view_matrix: *view_matrix,
			frustum_planes: extract_frustum_planes(&view_projection_matrix),
			view_direction: [view_direction[0] / view_direction_length, view_direction[1] / view_direction_length, view_direction[2] / view_direction_length],
			is_perspective,
			depth_range,
			projection_scale,
			viewport,
		});
		self.drawn_distance_sum = 0.0;
		self.drawn_distance_count = 0;
		let width = ((viewport[2] as f32 * self.resolution_scale).ceil() as i32).max(1);
		let height = ((viewport[3] as f32 * self.resolution_scale).ceil() as i32).max(1);
		let framebuffer = match self.read_line_target(gl, width, height) {
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
	// 모형 하나의 번호 그리기. (ToonRenderer::draw_mesh 와 같은 행렬과 자세, 번호 1 ~ 65535, 화면 밖이면 건너뛴다)
	//==============================================================================
	pub fn draw_mesh(&mut self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, line_id: u32, options: ToonLineDrawOptions) {
		if !self.is_drawing_line_ids {
			return;
		}
		if options.is_culled && !self.is_visible(mesh, model_matrix) {
			return;
		}
		self.add_drawn_distance(mesh, model_matrix);
		self.draw_with_count(gl, mesh, model_matrix, 0, line_id, false, options);
	}

	//==============================================================================
	// 같은 모형 여럿의 번호를 한 번에 그리기. (인스턴스에 line_id 가 있으면 그 번호, 없으면 공통 번호)
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
			self.draw_with_count(gl, mesh, &Matrix4::create_identity(), instance_count as i32, line_id, true, options);
		}
		self.instance_data = instance_data;
	}

	//==============================================================================
	// 번호 그리기 본체. (instance_count 가 0 이면 보통 그리기)
	//==============================================================================
	fn draw_with_count(&self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, instance_count: i32, line_id: u32, is_instance_line_id: bool, options: ToonLineDrawOptions) {
		let line_program = &self.line_program;
		let is_skinned = mesh.is_skinned() && options.joint_matrices.is_some();
		let (body_vertex_array, body_index_count) = mesh.get_body();
		let index_start = options.index_start.min(body_index_count as usize);
		let remaining_count = body_index_count as usize - index_start;
		let index_count = options.index_count.unwrap_or(remaining_count).min(remaining_count);
		if index_count == 0 {
			return;
		}
		unsafe {
			gl.uniform_matrix_4_f32_slice(line_program.get_uniform_location(gl, "modelMatrix").as_ref(), false, &model_matrix.elements);
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "instanced").as_ref(), if instance_count > 0 { 1.0 } else { 0.0 });
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "instanceLineIds").as_ref(), if is_instance_line_id { 1.0 } else { 0.0 });
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "skinned").as_ref(), if is_skinned { 1.0 } else { 0.0 });
			gl.uniform_1_f32(line_program.get_uniform_location(gl, "lineId").as_ref(), line_id as f32);
			if let (true, Some(joint_matrices)) = (is_skinned, options.joint_matrices) {
				let joint_float_count = joint_matrices.len().min(TOON_MAXIMUM_JOINTS * 16) / 16 * 16;
				gl.uniform_matrix_4_f32_slice(line_program.get_uniform_location(gl, "jointMatrices").as_ref(), false, &joint_matrices[..joint_float_count]);
			}
			gl.bind_vertex_array(Some(body_vertex_array));
			let byte_offset = (index_start * 4) as i32;
			if instance_count > 0 {
				gl.draw_elements_instanced(glow::TRIANGLES, index_count as i32, glow::UNSIGNED_INT, byte_offset, instance_count);
			}
			else {
				gl.draw_elements(glow::TRIANGLES, index_count as i32, glow::UNSIGNED_INT, byte_offset);
			}
		}
	}

	//==============================================================================
	// 번호 그리기 끝. (선 찾기로 가면을 쓰고, 프레임버퍼와 뷰포트와 상태를 돌린다)
	//==============================================================================
	pub fn end_line_ids(&mut self, gl: &glow::Context) {
		if !self.is_drawing_line_ids {
			return;
		}
		self.is_drawing_line_ids = false;
		let (frame, line_target) = match (self.frame, self.line_target.as_ref()) {
			(Some(frame), Some(line_target)) => {
				(frame, line_target)
			}
			_ => {
				self.restore_state(gl);
				return;
			}
		};
		let outline_radius = self.read_outline_radius();
		let mask_program = &self.mask_program;
		unsafe {
			gl.bind_vertex_array(None);
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(line_target.mask_framebuffer));
			gl.viewport(0, 0, line_target.width, line_target.height);
			gl.disable(glow::DEPTH_TEST);
			gl.depth_mask(false);
			gl.disable(glow::CULL_FACE);
			gl.disable(glow::BLEND);
			mask_program.use_program(gl);
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(line_target.line_texture));
			gl.active_texture(glow::TEXTURE0 + DEPTH_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(line_target.depth_texture));
			gl.active_texture(glow::TEXTURE0);
			let location = |name: &str| mask_program.get_uniform_location(gl, name);
			gl.uniform_1_i32(location("lineTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_1_i32(location("depthTexture").as_ref(), DEPTH_TEXTURE_UNIT as i32);
			gl.uniform_1_f32(location("outlineRadius").as_ref(), outline_radius);
			gl.uniform_1_f32(location("creaseRadius").as_ref(), (outline_radius * 0.5).max(1.0));
			gl.uniform_1_f32(location("creaseStrength").as_ref(), self.crease_strength);
			gl.uniform_1_f32(location("creaseCosine").as_ref(), self.crease_cosine);
			gl.uniform_1_f32(location("creaseExtent").as_ref(), self.crease_extent);
			gl.uniform_1_f32(location("depthThreshold").as_ref(), self.depth_threshold);
			gl.uniform_1_f32(location("contactTolerance").as_ref(), self.contact_tolerance);
			gl.uniform_1_f32(location("isPerspective").as_ref(), if frame.is_perspective { 1.0 } else { 0.0 });
			gl.uniform_2_f32(location("depthRange").as_ref(), frame.depth_range[0], frame.depth_range[1]);
			gl.uniform_1_i32(location("sampleCount").as_ref(), self.sample_count as i32);
			gl.uniform_1_i32(location("creaseCount").as_ref(), self.crease_sample_count as i32);
			gl.uniform_3_f32(location("viewDirection").as_ref(), frame.view_direction[0], frame.view_direction[1], frame.view_direction[2]);
			gl.bind_vertex_array(Some(self.empty_vertex_array));
			gl.draw_arrays(glow::TRIANGLES, 0, 3);
			gl.bind_vertex_array(None);
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0 + DEPTH_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}
		self.outline_radius = outline_radius;
		self.restore_state(gl);
	}

	//==============================================================================
	// 바깥선 긋기. (가면을 begin_line_ids 때의 그리는 영역에 섞는다, 깊이는 보지도 쓰지도 않는다, 스텐실은 그대로)
	//==============================================================================
	pub fn draw_outline(&mut self, gl: &glow::Context) {
		let (frame, mask_texture) = match (self.frame, self.line_target.as_ref()) {
			(Some(frame), Some(line_target)) => {
				(frame, line_target.mask_texture)
			}
			_ => {
				return;
			}
		};
		self.save_state(gl);
		let composite_program = &self.composite_program;
		let viewport = frame.viewport;
		let color = self.color;
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.depth_mask(false);
			gl.disable(glow::CULL_FACE);
			gl.enable(glow::BLEND);
			gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			composite_program.use_program(gl);
			gl.active_texture(glow::TEXTURE0 + LINE_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(mask_texture));
			gl.active_texture(glow::TEXTURE0);
			gl.uniform_1_i32(composite_program.get_uniform_location(gl, "maskTexture").as_ref(), LINE_TEXTURE_UNIT as i32);
			gl.uniform_4_f32(composite_program.get_uniform_location(gl, "viewportRect").as_ref(), viewport[0] as f32, viewport[1] as f32, viewport[2].max(1) as f32, viewport[3].max(1) as f32);
			gl.uniform_4_f32(composite_program.get_uniform_location(gl, "outlineColor").as_ref(), color[0], color[1], color[2], color[3]);
			gl.uniform_1_f32(composite_program.get_uniform_location(gl, "hdrOutput").as_ref(), if self.is_hdr_output { 1.0 } else { 0.0 });
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
	// 그린 모형의 거리 더하기. (초점 거리를 주지 않은 원근에서 쓴다, 보기 공간의 앞쪽 거리)
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
	// 경계 구가 이번 프레임의 절두체 안에 드는지.
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
	// 선의 판 읽기. (크기가 바뀌면 다시 지음, 짓지 못하면 None)
	//==============================================================================
	fn read_line_target(&mut self, gl: &glow::Context, width: i32, height: i32) -> Option<&ToonLineTarget> {
		let is_same_size = matches!(self.line_target.as_ref(), Some(line_target) if line_target.width == width && line_target.height == height);
		if !is_same_size {
			self.delete_line_target(gl);
			self.line_target = ToonScreenOutline::create_line_target(gl, width, height);
		}
		self.line_target.as_ref()
	}

	//==============================================================================
	// 선의 판 짓기. (노멀과 번호 RGBA8 과 깊이 24, 가면 R8, 앞의 프레임버퍼와 텍스처는 돌린다)
	//==============================================================================
	fn create_line_target(gl: &glow::Context, width: i32, height: i32) -> Option<ToonLineTarget> {
		unsafe {
			let previous_framebuffer = crate::graphics3d::read_framebuffer_binding(gl);
			let previous_texture = crate::graphics3d::read_texture_binding(gl);
			let create_texture = |internal_format: u32, format: u32, data_type: u32, filter: u32| -> Option<GlTexture> {
				let texture = gl.create_texture().ok()?;
				gl.bind_texture(glow::TEXTURE_2D, Some(texture));
				gl.tex_image_2d(glow::TEXTURE_2D, 0, internal_format as i32, width, height, 0, format, data_type, glow::PixelUnpackData::Slice(None));
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
				Some(texture)
			};
			let line_texture = create_texture(glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, glow::NEAREST)?;
			let depth_texture = create_texture(glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::NEAREST)?;
			let mask_texture = create_texture(glow::R8, glow::RED, glow::UNSIGNED_BYTE, glow::LINEAR)?;
			gl.bind_texture(glow::TEXTURE_2D, previous_texture);
			let framebuffer = gl.create_framebuffer().ok()?;
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(line_texture), 0);
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(depth_texture), 0);
			let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
			let mask_framebuffer = gl.create_framebuffer().ok()?;
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(mask_framebuffer));
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(mask_texture), 0);
			let mask_status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
			gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
			let line_target = ToonLineTarget { framebuffer, line_texture, depth_texture, mask_framebuffer, mask_texture, width, height };
			if status != glow::FRAMEBUFFER_COMPLETE || mask_status != glow::FRAMEBUFFER_COMPLETE {
				ToonScreenOutline::delete_target_objects(gl, &line_target);
				return None;
			}
			Some(line_target)
		}
	}

	//==============================================================================
	// 선의 판 지우기.
	//==============================================================================
	fn delete_line_target(&mut self, gl: &glow::Context) {
		if let Some(line_target) = self.line_target.take() {
			ToonScreenOutline::delete_target_objects(gl, &line_target);
		}
	}

	fn delete_target_objects(gl: &glow::Context, line_target: &ToonLineTarget) {
		unsafe {
			gl.delete_framebuffer(line_target.framebuffer);
			gl.delete_framebuffer(line_target.mask_framebuffer);
			gl.delete_texture(line_target.line_texture);
			gl.delete_texture(line_target.depth_texture);
			gl.delete_texture(line_target.mask_texture);
		}
	}

	//==============================================================================
	// 그리기 상태 적어 두기.
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
	// 적어 둔 상태로 돌리기.
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
	// 다 쓴 뒤 지우기.
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.delete_line_target(gl);
		unsafe {
			gl.delete_vertex_array(self.empty_vertex_array);
			gl.delete_program(self.line_program.get_program());
			gl.delete_program(self.mask_program.get_program());
			gl.delete_program(self.composite_program.get_program());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn projection_reads_depth_range_for_both_kinds() {
		let (is_perspective, depth_range, projection_scale) = read_projection(&Matrix4::create_perspective(0.8, 1.5, 0.1, 3000.0));
		assert!(is_perspective);
		assert!((depth_range[0] - 0.1).abs() < 0.001);
		assert!((depth_range[1] / 3000.0 - 1.0).abs() < 0.01);
		assert!((projection_scale - 1.0 / 0.4f32.tan()).abs() < 0.0001);
		let (is_perspective, depth_range, projection_scale) = read_projection(&Matrix4::create_orthographic(-4.0, 4.0, -2.0, 2.0, -400.0, 400.0));
		assert!(!is_perspective);
		assert!((depth_range[0] + 400.0).abs() < 0.01);
		assert!((depth_range[1] - 400.0).abs() < 0.01);
		assert!((projection_scale - 0.5).abs() < 0.0001);
	}

	#[test]
	fn outline_radius_follows_unit_and_minimum() {
		assert_eq!(compute_outline_radius(3.0, ToonOutlineWidthUnit::Pixel, 720, 360, 1.0, false, 0.0), 1.5);
		assert_eq!(compute_outline_radius(0.5, ToonOutlineWidthUnit::Pixel, 720, 360, 1.0, false, 0.0), 1.0);
		let orthographic_radius = compute_outline_radius(0.03, ToonOutlineWidthUnit::World, 720, 720, 2.0 / 8.1, false, 0.0);
		assert!((orthographic_radius - 0.03 * 360.0 * 2.0 / 8.1).abs() < 0.001);
		let near_radius = compute_outline_radius(0.022, ToonOutlineWidthUnit::World, 720, 720, 2.0, true, 2.0);
		let far_radius = compute_outline_radius(0.022, ToonOutlineWidthUnit::World, 720, 720, 2.0, true, 4.0);
		assert!((near_radius - far_radius * 2.0).abs() < 0.001);
	}
}

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
	worldNormal = mat3(objectMatrix) * mat3(skinMatrix) * vertexNormal;
	surfaceLineId = instanced > 0.5 && instanceLineIds > 0.5 ? instanceTint.x : lineId;
	gl_Position = viewProjectionMatrix * objectMatrix * skinMatrix * vec4(vertexPosition, 1.0);
}
";

const LINE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec3 worldNormal;
flat in float surfaceLineId;
out vec4 fragmentLine;
vec2 wrapOctahedron(vec2 value) {
	return (1.0 - abs(value.yx)) * vec2(value.x >= 0.0 ? 1.0 : -1.0, value.y >= 0.0 ? 1.0 : -1.0);
}
void main() {
	vec3 normal = normalize(worldNormal);
	normal /= abs(normal.x) + abs(normal.y) + abs(normal.z);
	vec2 folded = normal.y >= 0.0 ? normal.xz : wrapOctahedron(normal.xz);
	float lowPart = mod(surfaceLineId, 256.0);
	float highPart = floor(surfaceLineId / 256.0);
	fragmentLine = vec4(folded * 0.5 + 0.5, lowPart / 255.0, highPart / 255.0);
}
";

const SCREEN_TRIANGLE_VERTEX_SHADER: &str = "#version 300 es
const vec2 TRIANGLE_POINTS[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
void main() {
	gl_Position = vec4(TRIANGLE_POINTS[gl_VertexID], 0.0, 1.0);
}
";

const LINE_MASK_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
uniform highp sampler2D lineTexture;
uniform highp sampler2D depthTexture;
uniform float outlineRadius;
uniform float creaseRadius;
uniform float creaseStrength;
uniform float creaseCosine;
uniform float creaseExtent;
uniform float depthThreshold;
uniform float contactTolerance;
uniform float isPerspective;
uniform vec2 depthRange;
uniform int sampleCount;
uniform int creaseCount;
uniform vec3 viewDirection;
out vec4 fragmentMask;
const float FACING_MINIMUM = 0.25;
const float INNER_STRENGTH = 0.5;
const vec2 DIRECTIONS[4] = vec2[4](vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0));
const ivec2 OFFSETS[28] = ivec2[28](ivec2(1, 0), ivec2(-1, 0), ivec2(0, 1), ivec2(0, -1), ivec2(1, 1), ivec2(-1, 1), ivec2(1, -1), ivec2(-1, -1), ivec2(2, 0), ivec2(-2, 0), ivec2(0, 2), ivec2(0, -2), ivec2(2, 1), ivec2(-2, 1), ivec2(2, -1), ivec2(-2, -1), ivec2(1, 2), ivec2(-1, 2), ivec2(1, -2), ivec2(-1, -2), ivec2(2, 2), ivec2(-2, 2), ivec2(2, -2), ivec2(-2, -2), ivec2(3, 0), ivec2(-3, 0), ivec2(0, 3), ivec2(0, -3));
const float OFFSET_DISTANCES[28] = float[28](1.0000, 1.0000, 1.0000, 1.0000, 1.4142, 1.4142, 1.4142, 1.4142, 2.0000, 2.0000, 2.0000, 2.0000, 2.2361, 2.2361, 2.2361, 2.2361, 2.2361, 2.2361, 2.2361, 2.2361, 2.8284, 2.8284, 2.8284, 2.8284, 3.0000, 3.0000, 3.0000, 3.0000);
float readLineId(vec4 value) {
	return floor(value.b * 255.0 + 0.5) + floor(value.a * 255.0 + 0.5) * 256.0;
}
vec3 readNormal(vec4 value) {
	vec2 folded = value.rg * 2.0 - 1.0;
	vec3 normal = vec3(folded.x, 1.0 - abs(folded.x) - abs(folded.y), folded.y);
	float fold = max(-normal.y, 0.0);
	normal.x += normal.x >= 0.0 ? -fold : fold;
	normal.z += normal.z >= 0.0 ? -fold : fold;
	return normalize(normal);
}
float readDistance(ivec2 pixel) {
	float depth = texelFetch(depthTexture, pixel, 0).r;
	if (isPerspective < 0.5) {
		return depth * (depthRange.y - depthRange.x);
	}
	float clipDepth = depth * 2.0 - 1.0;
	return 2.0 * depthRange.x * depthRange.y / (depthRange.y + depthRange.x - clipDepth * (depthRange.y - depthRange.x));
}
bool readOutlineHit(vec4 sampleValue, float sampleDistance, float centerId, float centerDistance, float threshold) {
	float id = readLineId(sampleValue);
	if (id < 0.5) {
		return false;
	}
	if (centerId < 0.5) {
		return true;
	}
	float nearer = centerDistance - sampleDistance;
	if (nearer > threshold) {
		return true;
	}
	return abs(id - centerId) > 0.5 && nearer > -contactTolerance && (nearer > contactTolerance || centerId > id);
}
float readThreshold(vec4 value, float id) {
	float facing = id > 0.5 ? max(abs(dot(readNormal(value), viewDirection)), FACING_MINIMUM) : 1.0;
	return depthThreshold / facing;
}
void main() {
	ivec2 lastPixel = textureSize(depthTexture, 0) - 1;
	ivec2 center = ivec2(gl_FragCoord.xy);
	vec4 centerValue = texelFetch(lineTexture, center, 0);
	float centerId = readLineId(centerValue);
	float centerDistance = readDistance(center);
	vec3 centerNormal = readNormal(centerValue);
	float threshold = readThreshold(centerValue, centerId);
	float reach = outlineRadius + 1.0;
	float outerStrength = 0.0;
	float innerStrength = 0.0;
	for (int index = 0; index < 4; ++index) {
		ivec2 pixel = clamp(center + OFFSETS[index], ivec2(0), lastPixel);
		vec4 value = texelFetch(lineTexture, pixel, 0);
		float sampleDistance = readDistance(pixel);
		if (outerStrength <= 0.0 && readOutlineHit(value, sampleDistance, centerId, centerDistance, threshold)) {
			outerStrength = clamp(reach - OFFSET_DISTANCES[index], 0.0, 1.0);
		}
		float sampleId = readLineId(value);
		if (centerId > 0.5 && readOutlineHit(centerValue, centerDistance, sampleId, sampleDistance, readThreshold(value, sampleId))) {
			innerStrength = INNER_STRENGTH;
		}
	}
	for (int index = 4; index < 28; ++index) {
		if (outerStrength > 0.0 || index >= sampleCount || OFFSET_DISTANCES[index] >= reach) {
			break;
		}
		ivec2 pixel = clamp(center + OFFSETS[index], ivec2(0), lastPixel);
		vec4 value = texelFetch(lineTexture, pixel, 0);
		if (readOutlineHit(value, readDistance(pixel), centerId, centerDistance, threshold)) {
			outerStrength = clamp(reach - OFFSET_DISTANCES[index], 0.0, 1.0);
		}
	}
	float creaseHitCount = 0.0;
	if (outerStrength < 1.0 && centerId > 0.5) {
		for (int index = 0; index < 4; ++index) {
			if (index >= creaseCount) {
				break;
			}
			ivec2 pixel = clamp(center + ivec2(round(DIRECTIONS[index] * creaseRadius)), ivec2(0), lastPixel);
			vec4 value = texelFetch(lineTexture, pixel, 0);
			if (abs(readLineId(value) - centerId) > 0.5) {
				continue;
			}
			float nearer = centerDistance - readDistance(pixel);
			if (abs(nearer) < threshold * 2.0 && dot(readNormal(value), centerNormal) < creaseCosine) {
				if (creaseExtent <= 0.0) {
					creaseHitCount += 1.0;
					continue;
				}
				ivec2 farPixel = clamp(center + ivec2(round(DIRECTIONS[index] * (creaseRadius + creaseExtent))), ivec2(0), lastPixel);
				vec4 farValue = texelFetch(lineTexture, farPixel, 0);
				if (abs(readLineId(farValue) - centerId) < 0.5 && dot(readNormal(farValue), centerNormal) < creaseCosine) {
					creaseHitCount += 1.0;
				}
			}
		}
	}
	fragmentMask = vec4(min(1.0, max(outerStrength, innerStrength) + creaseHitCount * creaseStrength), 0.0, 0.0, 1.0);
}
";

const COMPOSITE_FRAGMENT_SHADER: &str = "#version 300 es
precision mediump float;
uniform sampler2D maskTexture;
uniform vec4 viewportRect;
uniform vec4 outlineColor;
uniform float hdrOutput;
out vec4 fragmentColor;
void main() {
	float strength = texture(maskTexture, (gl_FragCoord.xy - viewportRect.xy) / viewportRect.zw).r * outlineColor.a;
	if (strength < 0.02) {
		discard;
	}
	vec3 color = hdrOutput > 0.5 ? pow(outlineColor.rgb, vec3(2.2)) : outlineColor.rgb;
	fragmentColor = vec4(color * strength, strength);
}
";
