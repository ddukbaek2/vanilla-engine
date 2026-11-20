//==============================================================================
// 툰 렌더러. (vanilla.js experimental/graphics/toon/toonrenderer.js 의 ToonRenderer)
// - MeshBuilder 의 배열이나 read_glb_arrays 로 읽은 모형을 한 색 재질과 바깥선으로 그린다.
// - 빛: 해(방향과 색)를 받는 면과, 하늘색과 땅색을 노멀의 위아래로 섞은 바탕빛. set_band_count 로 칸 수를 주면 셀 셰이딩이 된다.
// - 재질: 꼭짓점의 빛남(더해 밝힘)과 빛을 받지 않음. 모형마다 틴트 색(곱함)과 불투명도를 준다.
// - 바깥선: MeshBuilder 가 지은 껍데기를 뒷면만 그린다.
// - 뼈대: write_skin_pose 가 셈한 본 행렬을 draw_mesh 에 넘긴다. (TOON_MAXIMUM_JOINTS 까지, 엔진 SkinnedModelRenderer 와 같은 96)
// - 사용: create_mesh(gl, arrays) → 매 프레임 begin_frame(gl, 보기와 투영) → draw_mesh → end_frame(gl) → 다 쓰면 delete_mesh.
//   begin_frame 이 앞의 그리기 상태(섞기, 깊이, 면 가리기, 프로그램, 버텍스 어레이)를 적어 두고 깊이 시험과 면 가리기를 켜며,
//   end_frame 이 적어 둔 상태로 돌린다. (엔진 2D 그리기 사이에 끼울 때, 그 전에 Graphic 의 묶음을 내보낸다)
// - read_program_sources 로 ShaderWarmer 에 넘길 소스를 준다.
//==============================================================================
use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::core::graphic::{GlBuffer, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::toon::meshbuilder::{ToonMeshArrays, SKIN_FLOAT_COUNT, VERTEX_FLOAT_COUNT};

// 한 모형이 쓰는 뼈대 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_JOINTS: usize = 96;

// 어트리뷰트 자리. (모든 프로그램이 같은 자리를 쓴다)
pub const TOON_ATTRIBUTE_LOCATIONS: [(&str, u32); 7] = [("vertexPosition", 0), ("vertexNormal", 1), ("vertexColor", 2), ("vertexMaterial", 3), ("vertexDetail", 4), ("vertexJoints", 5), ("vertexWeights", 6)];

// 꼭짓점을 본으로 움직이는 셰이더 조각. (뼈대가 없으면 그대로)
const SKIN_SHADER_CHUNK: &str = "in vec4 vertexJoints;
in vec4 vertexWeights;
uniform mat4 jointMatrices[96];
uniform float skinned;
mat4 readSkinMatrix() {
	if (skinned < 0.5) {
		return mat4(1.0);
	}
	return jointMatrices[int(vertexJoints.x)] * vertexWeights.x + jointMatrices[int(vertexJoints.y)] * vertexWeights.y + jointMatrices[int(vertexJoints.z)] * vertexWeights.z + jointMatrices[int(vertexJoints.w)] * vertexWeights.w;
}
";

const SOLID_VERTEX_HEAD: &str = "#version 300 es
in vec3 vertexPosition;
in vec3 vertexNormal;
in vec3 vertexColor;
in vec2 vertexMaterial;
in vec4 vertexDetail;
";

const SOLID_VERTEX_BODY: &str = "uniform mat4 viewProjectionMatrix;
uniform mat4 modelMatrix;
out vec3 worldNormal;
out vec3 surfaceColor;
out vec2 surfaceMaterial;
void main() {
	mat4 skinMatrix = readSkinMatrix();
	vec4 position = modelMatrix * skinMatrix * vec4(vertexPosition, 1.0);
	worldNormal = normalize(mat3(modelMatrix) * mat3(skinMatrix) * vertexNormal);
	surfaceColor = vertexColor;
	surfaceMaterial = vertexMaterial;
	gl_Position = viewProjectionMatrix * position;
}
";

const SOLID_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec3 worldNormal;
in vec3 surfaceColor;
in vec2 surfaceMaterial;
uniform vec3 sunDirection;
uniform vec3 sunColor;
uniform vec3 skyColor;
uniform vec3 groundColor;
uniform float bandCount;
uniform vec4 tintColor;
uniform float alpha;
out vec4 fragmentColor;
vec3 toLinear(vec3 color) {
	return pow(color, vec3(2.2));
}
vec3 toDisplay(vec3 color) {
	return pow(clamp(color, 0.0, 1.0), vec3(1.0 / 2.2));
}
void main() {
	vec3 albedo = toLinear(surfaceColor) * toLinear(tintColor.rgb);
	float glow = surfaceMaterial.x;
	float unlit = surfaceMaterial.y;
	if (unlit > 0.5) {
		fragmentColor = vec4(toDisplay(albedo * (1.0 + glow)), alpha * tintColor.a);
		return;
	}
	vec3 normal = normalize(worldNormal);
	float facing = max(dot(normal, -normalize(sunDirection)), 0.0);
	if (bandCount > 0.5) {
		facing = floor(facing * bandCount + 0.5) / bandCount;
	}
	float upward = normal.y * 0.5 + 0.5;
	vec3 ambient = mix(groundColor, skyColor, upward);
	vec3 lit = albedo * (ambient + sunColor * facing) + albedo * glow;
	fragmentColor = vec4(toDisplay(lit), alpha * tintColor.a);
}
";

const OUTLINE_VERTEX_HEAD: &str = "#version 300 es
in vec3 vertexPosition;
";

const OUTLINE_VERTEX_BODY: &str = "uniform mat4 viewProjectionMatrix;
uniform mat4 modelMatrix;
void main() {
	mat4 skinMatrix = readSkinMatrix();
	gl_Position = viewProjectionMatrix * modelMatrix * skinMatrix * vec4(vertexPosition, 1.0);
}
";

const OUTLINE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
uniform vec4 outlineColor;
uniform float alpha;
out vec4 fragmentColor;
void main() {
	fragmentColor = vec4(outlineColor.rgb, outlineColor.a * alpha);
}
";

//==============================================================================
// GPU 에 올린 모형.
//==============================================================================
pub struct ToonMesh {
	body_vertex_array: GlVertexArray,
	body_index_count: i32,
	outline_vertex_array: Option<GlVertexArray>,
	outline_index_count: i32,
	buffer_list: Vec<GlBuffer>,
	is_skinned: bool,
}

impl ToonMesh {
	//==============================================================================
	// 뼈대로 움직이는 모형인지.
	//==============================================================================
	pub fn is_skinned(&self) -> bool {
		self.is_skinned
	}
}

//==============================================================================
// 모형 하나를 그릴 때의 꼴.
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonDrawOptions<'a> {
	pub tint_color: [f32; 4],
	pub alpha: f32,
	pub joint_matrices: Option<&'a [f32]>,
	pub has_outline: bool,
}

impl Default for ToonDrawOptions<'_> {
	fn default() -> Self {
		ToonDrawOptions { tint_color: [1.0; 4], alpha: 1.0, joint_matrices: None, has_outline: true }
	}
}

//==============================================================================
// begin_frame 앞의 그리기 상태.
//==============================================================================
struct SavedState {
	is_blend_enabled: bool,
	is_depth_test_enabled: bool,
	is_cull_face_enabled: bool,
	depth_mask: bool,
	blend_source_rgb: i32,
	blend_destination_rgb: i32,
	blend_source_alpha: i32,
	blend_destination_alpha: i32,
}

pub struct ToonRenderer {
	solid_program: ShaderProgram,
	outline_program: ShaderProgram,
	view_projection_elements: [f32; 16],
	sun_direction: [f32; 3],
	sun_color: [f32; 3],
	sky_color: [f32; 3],
	ground_color: [f32; 3],
	band_count: f32,
	outline_color: [f32; 4],
	saved_state: Option<SavedState>,
}

//==============================================================================
// f32, u32 배열을 바이트로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

fn index_slice_to_bytes(values: &[u32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

impl ToonRenderer {
	//==============================================================================
	// 셰이더 소스. ((정점, 조각) 두 벌: 몸, 바깥선)
	//==============================================================================
	pub fn read_program_sources() -> [(String, String); 2] {
		[
			(format!("{}{}{}", SOLID_VERTEX_HEAD, SKIN_SHADER_CHUNK, SOLID_VERTEX_BODY), String::from(SOLID_FRAGMENT_SHADER)),
			(format!("{}{}{}", OUTLINE_VERTEX_HEAD, SKIN_SHADER_CHUNK, OUTLINE_VERTEX_BODY), String::from(OUTLINE_FRAGMENT_SHADER)),
		]
	}

	//==============================================================================
	// 생성. (두 프로그램을 짓는다)
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<ToonRenderer, String> {
		let [(solid_vertex_source, solid_fragment_source), (outline_vertex_source, outline_fragment_source)] = ToonRenderer::read_program_sources();
		let solid_program = ShaderProgram::create_with_attribute_locations(gl, &solid_vertex_source, &solid_fragment_source, &TOON_ATTRIBUTE_LOCATIONS)?;
		let outline_program = ShaderProgram::create_with_attribute_locations(gl, &outline_vertex_source, &outline_fragment_source, &TOON_ATTRIBUTE_LOCATIONS)?;
		Ok(ToonRenderer {
			solid_program,
			outline_program,
			view_projection_elements: Matrix4::default().elements,
			sun_direction: [-0.4, -1.0, -0.3],
			sun_color: [0.9, 0.85, 0.8],
			sky_color: [0.45, 0.5, 0.6],
			ground_color: [0.2, 0.18, 0.16],
			band_count: 0.0,
			outline_color: [0.05, 0.05, 0.08, 1.0],
			saved_state: None,
		})
	}

	//==============================================================================
	// 모형 올리기. (몸이 비면 None)
	//==============================================================================
	pub fn create_mesh(&self, gl: &glow::Context, arrays: &ToonMeshArrays) -> Option<ToonMesh> {
		if arrays.index_data.is_empty() {
			return None;
		}
		let is_skinned = !arrays.skin_data.is_empty();
		let mut buffer_list = Vec::new();
		unsafe {
			// 몸.
			let body_vertex_array = gl.create_vertex_array().ok()?;
			gl.bind_vertex_array(Some(body_vertex_array));
			let vertex_buffer = gl.create_buffer().ok()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&arrays.vertex_data), glow::STATIC_DRAW);
			buffer_list.push(vertex_buffer);
			let stride = (VERTEX_FLOAT_COUNT * 4) as i32;
			for (location, size, offset) in [(0u32, 3i32, 0i32), (1, 3, 12), (2, 3, 24), (3, 2, 36), (4, 4, 44)] {
				gl.enable_vertex_attrib_array(location);
				gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
			}
			if is_skinned {
				buffer_list.push(ToonRenderer::create_skin_attributes(gl, &arrays.skin_data)?);
			}
			let body_index_buffer = gl.create_buffer().ok()?;
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(body_index_buffer));
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_slice_to_bytes(&arrays.index_data), glow::STATIC_DRAW);
			buffer_list.push(body_index_buffer);

			// 바깥선 껍데기. (자리만)
			let mut outline_vertex_array = None;
			let mut outline_index_count = 0;
			if !arrays.outline_index_data.is_empty() {
				let vertex_array = gl.create_vertex_array().ok()?;
				gl.bind_vertex_array(Some(vertex_array));
				let outline_buffer = gl.create_buffer().ok()?;
				gl.bind_buffer(glow::ARRAY_BUFFER, Some(outline_buffer));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&arrays.outline_vertex_data), glow::STATIC_DRAW);
				buffer_list.push(outline_buffer);
				gl.enable_vertex_attrib_array(0);
				gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
				if is_skinned && !arrays.outline_skin_data.is_empty() {
					buffer_list.push(ToonRenderer::create_skin_attributes(gl, &arrays.outline_skin_data)?);
				}
				let outline_index_buffer = gl.create_buffer().ok()?;
				gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(outline_index_buffer));
				gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_slice_to_bytes(&arrays.outline_index_data), glow::STATIC_DRAW);
				buffer_list.push(outline_index_buffer);
				outline_vertex_array = Some(vertex_array);
				outline_index_count = arrays.outline_index_data.len() as i32;
			}
			gl.bind_vertex_array(None);
			Some(ToonMesh {
				body_vertex_array,
				body_index_count: arrays.index_data.len() as i32,
				outline_vertex_array,
				outline_index_count,
				buffer_list,
				is_skinned,
			})
		}
	}

	//==============================================================================
	// 뼈대 번호와 무게의 버퍼. (지금 묶인 버텍스 어레이에 어트리뷰트로 붙인다)
	//==============================================================================
	unsafe fn create_skin_attributes(gl: &glow::Context, skin_data: &[f32]) -> Option<GlBuffer> {
		let stride = (SKIN_FLOAT_COUNT * 4) as i32;
		let skin_buffer = gl.create_buffer().ok()?;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(skin_buffer));
		gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(skin_data), glow::STATIC_DRAW);
		gl.enable_vertex_attrib_array(5);
		gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, stride, 0);
		gl.enable_vertex_attrib_array(6);
		gl.vertex_attrib_pointer_f32(6, 4, glow::FLOAT, false, stride, 16);
		Some(skin_buffer)
	}

	//==============================================================================
	// 모형 지우기.
	//==============================================================================
	pub fn delete_mesh(&self, gl: &glow::Context, mesh: ToonMesh) {
		unsafe {
			gl.delete_vertex_array(mesh.body_vertex_array);
			if let Some(outline_vertex_array) = mesh.outline_vertex_array {
				gl.delete_vertex_array(outline_vertex_array);
			}
			for buffer in mesh.buffer_list {
				gl.delete_buffer(buffer);
			}
		}
	}

	//==============================================================================
	// 프레임 시작. (보기와 투영을 정하고, 앞의 상태를 적어 둔 뒤 깊이 시험과 면 가리기를 켠다, 깊이를 비우는 것은 부르는 쪽이 정한다)
	//==============================================================================
	pub fn begin_frame(&mut self, gl: &glow::Context, view_projection_matrix: &Matrix4) {
		self.view_projection_elements = view_projection_matrix.elements;
		unsafe {
			self.saved_state = Some(SavedState {
				is_blend_enabled: gl.is_enabled(glow::BLEND),
				is_depth_test_enabled: gl.is_enabled(glow::DEPTH_TEST),
				is_cull_face_enabled: gl.is_enabled(glow::CULL_FACE),
				depth_mask: gl.get_parameter_bool(glow::DEPTH_WRITEMASK),
				blend_source_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
				blend_destination_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
				blend_source_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
				blend_destination_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
			});
			gl.disable(glow::BLEND);
			gl.enable(glow::DEPTH_TEST);
			gl.depth_func(glow::LEQUAL);
			gl.depth_mask(true);
			gl.enable(glow::CULL_FACE);
			gl.front_face(glow::CCW);
		}
	}

	//==============================================================================
	// 모형 하나 그리기.
	//==============================================================================
	pub fn draw_mesh(&self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions) {
		let is_skinned = mesh.is_skinned && options.joint_matrices.is_some();
		let is_transparent = options.alpha < 1.0 || options.tint_color[3] < 1.0;
		unsafe {
			if is_transparent {
				gl.enable(glow::BLEND);
				gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
			}

			// 바깥선. (껍데기의 뒷면만)
			if let (true, Some(outline_vertex_array)) = (options.has_outline, mesh.outline_vertex_array) {
				let outline_program = &self.outline_program;
				outline_program.use_program(gl);
				self.upload_common_uniforms(gl, outline_program, model_matrix, is_skinned, options.joint_matrices);
				let outline_color = self.outline_color;
				gl.uniform_4_f32(outline_program.get_uniform_location(gl, "outlineColor").as_ref(), outline_color[0], outline_color[1], outline_color[2], outline_color[3]);
				gl.uniform_1_f32(outline_program.get_uniform_location(gl, "alpha").as_ref(), options.alpha);
				gl.cull_face(glow::FRONT);
				gl.bind_vertex_array(Some(outline_vertex_array));
				gl.draw_elements(glow::TRIANGLES, mesh.outline_index_count, glow::UNSIGNED_INT, 0);
			}

			// 몸.
			let solid_program = &self.solid_program;
			solid_program.use_program(gl);
			self.upload_common_uniforms(gl, solid_program, model_matrix, is_skinned, options.joint_matrices);
			let sun_direction = self.sun_direction;
			let sun_color = self.sun_color;
			let sky_color = self.sky_color;
			let ground_color = self.ground_color;
			let tint_color = options.tint_color;
			gl.uniform_3_f32(solid_program.get_uniform_location(gl, "sunDirection").as_ref(), sun_direction[0], sun_direction[1], sun_direction[2]);
			gl.uniform_3_f32(solid_program.get_uniform_location(gl, "sunColor").as_ref(), sun_color[0], sun_color[1], sun_color[2]);
			gl.uniform_3_f32(solid_program.get_uniform_location(gl, "skyColor").as_ref(), sky_color[0], sky_color[1], sky_color[2]);
			gl.uniform_3_f32(solid_program.get_uniform_location(gl, "groundColor").as_ref(), ground_color[0], ground_color[1], ground_color[2]);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "bandCount").as_ref(), self.band_count);
			gl.uniform_4_f32(solid_program.get_uniform_location(gl, "tintColor").as_ref(), tint_color[0], tint_color[1], tint_color[2], tint_color[3]);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "alpha").as_ref(), options.alpha);
			gl.cull_face(glow::BACK);
			gl.bind_vertex_array(Some(mesh.body_vertex_array));
			gl.draw_elements(glow::TRIANGLES, mesh.body_index_count, glow::UNSIGNED_INT, 0);
			if is_transparent {
				gl.disable(glow::BLEND);
			}
		}
	}

	//==============================================================================
	// 두 프로그램이 함께 쓰는 유니폼 올리기. (보기와 투영, 모형 행렬, 뼈대)
	//==============================================================================
	fn upload_common_uniforms(&self, gl: &glow::Context, shader_program: &ShaderProgram, model_matrix: &Matrix4, is_skinned: bool, joint_matrices: Option<&[f32]>) {
		unsafe {
			gl.uniform_matrix_4_f32_slice(shader_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &self.view_projection_elements);
			gl.uniform_matrix_4_f32_slice(shader_program.get_uniform_location(gl, "modelMatrix").as_ref(), false, &model_matrix.elements);
			gl.uniform_1_f32(shader_program.get_uniform_location(gl, "skinned").as_ref(), if is_skinned { 1.0 } else { 0.0 });
			if let (true, Some(joint_matrices)) = (is_skinned, joint_matrices) {
				let joint_float_count = joint_matrices.len().min(TOON_MAXIMUM_JOINTS * 16) / 16 * 16;
				gl.uniform_matrix_4_f32_slice(shader_program.get_uniform_location(gl, "jointMatrices").as_ref(), false, &joint_matrices[..joint_float_count]);
			}
		}
	}

	//==============================================================================
	// 프레임 끝. (적어 둔 상태로 돌린다, 프로그램과 버텍스 어레이는 풀어 둔다)
	//==============================================================================
	pub fn end_frame(&mut self, gl: &glow::Context) {
		unsafe {
			gl.cull_face(glow::BACK);
			gl.bind_vertex_array(None);
			gl.use_program(None);
			let saved_state = match self.saved_state.take() {
				Some(saved_state) => {
					saved_state
				}
				None => {
					return;
				}
			};
			gl.depth_mask(saved_state.depth_mask);
			gl.blend_func_separate(saved_state.blend_source_rgb as u32, saved_state.blend_destination_rgb as u32, saved_state.blend_source_alpha as u32, saved_state.blend_destination_alpha as u32);
			for (capability, is_enabled) in [(glow::BLEND, saved_state.is_blend_enabled), (glow::DEPTH_TEST, saved_state.is_depth_test_enabled), (glow::CULL_FACE, saved_state.is_cull_face_enabled)] {
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
	// 빛 설정. (해가 비추는 방향, 해의 색, 하늘색, 땅색, 모두 선형 색 0 ~)
	//==============================================================================
	pub fn set_light(&mut self, sun_direction: [f32; 3], sun_color: [f32; 3], sky_color: [f32; 3], ground_color: [f32; 3]) {
		self.sun_direction = sun_direction;
		self.sun_color = sun_color;
		self.sky_color = sky_color;
		self.ground_color = ground_color;
	}

	//==============================================================================
	// 셀 셰이딩 칸 수 설정. (0 이면 부드럽게)
	//==============================================================================
	pub fn set_band_count(&mut self, band_count: u32) {
		self.band_count = band_count as f32;
	}

	//==============================================================================
	// 바깥선 색 설정. (sRGB 0 ~ 1 과 알파)
	//==============================================================================
	pub fn set_outline_color(&mut self, outline_color: [f32; 4]) {
		self.outline_color = outline_color;
	}
}
