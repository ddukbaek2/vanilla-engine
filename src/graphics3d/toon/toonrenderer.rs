//==============================================================================
// 툰 렌더러. (vanilla.js experimental/graphics/toon/toonrenderer.js 의 ToonRenderer)
// - MeshBuilder 의 배열이나 read_glb_arrays 로 읽은 모형을 한 색 재질과 바깥선으로 그린다.
// - 빛: 해(방향과 색)를 받는 면과, 하늘색과 땅색을 노멀의 위아래로 섞은 바탕빛. set_band_count 로 칸 수를 주면 셀 셰이딩이 된다.
// - 재질: 꼭짓점의 빛남(더해 밝힘)과 빛을 받지 않음. 모형마다 틴트 색(곱함)과 불투명도를 준다.
// - 바깥선: MeshBuilder 가 지은 껍데기를 뒷면만 그린다.
// - 뼈대: write_skin_pose 가 셈한 본 행렬을 draw_mesh 에 넘긴다. (TOON_MAXIMUM_JOINTS 까지, 엔진 SkinnedModelRenderer 와 같은 96)
// - GPU 인스턴싱: draw_mesh_instanced 로 같은 모형 여럿을 인스턴스마다 행렬과 틴트로 한 번에 그린다. (몸과 바깥선 모두)
// - 화면 밖 거르기: 모형의 경계 구를 보기와 투영의 절두체에 대어 밖이면 그리지 않는다. (인스턴스는 하나씩 걸러 남은 것만 올린다)
// - 그림자: 엔진 ShadowMap 에 begin_shadow_pass 와 draw_mesh_shadow 로 드리우고, set_shadow 로 받는다. (3x3 PCF)
// - HDR: set_hdr_output(true) 면 감마를 걸지 않은 선형 색을 낸다. 실수 렌더 타겟(ToonPostProcess)에 그리고 블룸과 톤매핑으로 마무리한다.
// - 사용: create_mesh(gl, arrays) → 매 프레임 begin_frame(gl, 보기와 투영) → draw_mesh / draw_mesh_instanced → end_frame(gl) → 다 쓰면 delete_mesh.
//   begin_frame 이 앞의 그리기 상태(섞기, 깊이, 면 가리기)를 적어 두고 깊이 시험과 면 가리기를 켜며, end_frame 이 적어 둔 상태로 돌린다.
//   (엔진 2D 그리기 사이에 끼울 때, 그 전에 Graphic 의 묶음을 내보낸다)
// - read_program_sources 로 ShaderWarmer 에 넘길 소스를 준다.
//==============================================================================
use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::core::graphic::{GlBuffer, GlTexture, GlVertexArray};
use crate::graphics3d::GlFramebuffer;
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::shadowmap::ShadowMap;
use crate::graphics3d::toon::meshbuilder::{ToonMeshArrays, SKIN_FLOAT_COUNT, VERTEX_FLOAT_COUNT};

// 한 모형이 쓰는 뼈대 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_JOINTS: usize = 96;

// 인스턴스 하나의 수 개수. (행렬 16, 틴트 4)
pub const TOON_INSTANCE_FLOAT_COUNT: usize = 20;

// 그림자 텍스처를 묶는 유닛.
const SHADOW_TEXTURE_UNIT: u32 = 3;

// 어트리뷰트 자리. (모든 프로그램이 같은 자리를 쓴다)
pub const TOON_ATTRIBUTE_LOCATIONS: [(&str, u32); 12] = [
	("vertexPosition", 0),
	("vertexNormal", 1),
	("vertexColor", 2),
	("vertexMaterial", 3),
	("vertexDetail", 4),
	("vertexJoints", 5),
	("vertexWeights", 6),
	("instanceMatrix0", 7),
	("instanceMatrix1", 8),
	("instanceMatrix2", 9),
	("instanceMatrix3", 10),
	("instanceTint", 11),
];

// 꼭짓점을 본으로 움직이고 인스턴스 행렬을 고르는 셰이더 조각.
const VERTEX_COMMON_CHUNK: &str = "in vec4 vertexJoints;
in vec4 vertexWeights;
in vec4 instanceMatrix0;
in vec4 instanceMatrix1;
in vec4 instanceMatrix2;
in vec4 instanceMatrix3;
in vec4 instanceTint;
uniform mat4 jointMatrices[96];
uniform float skinned;
uniform float instanced;
uniform mat4 viewProjectionMatrix;
uniform mat4 modelMatrix;
mat4 readSkinMatrix() {
	if (skinned < 0.5) {
		return mat4(1.0);
	}
	return jointMatrices[int(vertexJoints.x)] * vertexWeights.x + jointMatrices[int(vertexJoints.y)] * vertexWeights.y + jointMatrices[int(vertexJoints.z)] * vertexWeights.z + jointMatrices[int(vertexJoints.w)] * vertexWeights.w;
}
mat4 readModelMatrix() {
	if (instanced < 0.5) {
		return modelMatrix;
	}
	return mat4(instanceMatrix0, instanceMatrix1, instanceMatrix2, instanceMatrix3);
}
vec4 readInstanceTint() {
	if (instanced < 0.5) {
		return vec4(1.0);
	}
	return instanceTint;
}
";

const SOLID_VERTEX_HEAD: &str = "#version 300 es
in vec3 vertexPosition;
in vec3 vertexNormal;
in vec3 vertexColor;
in vec2 vertexMaterial;
in vec4 vertexDetail;
";

const SOLID_VERTEX_BODY: &str = "uniform mat4 lightViewProjectionMatrix;
out vec3 worldNormal;
out vec3 surfaceColor;
out vec2 surfaceMaterial;
out vec4 surfaceTint;
out vec4 lightSpacePosition;
void main() {
	mat4 skinMatrix = readSkinMatrix();
	mat4 objectMatrix = readModelMatrix();
	vec4 position = objectMatrix * skinMatrix * vec4(vertexPosition, 1.0);
	worldNormal = normalize(mat3(objectMatrix) * mat3(skinMatrix) * vertexNormal);
	surfaceColor = vertexColor;
	surfaceMaterial = vertexMaterial;
	surfaceTint = readInstanceTint();
	lightSpacePosition = lightViewProjectionMatrix * vec4(position.xyz + worldNormal * 0.02, 1.0);
	gl_Position = viewProjectionMatrix * position;
}
";

const SOLID_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
precision highp sampler2DShadow;
in vec3 worldNormal;
in vec3 surfaceColor;
in vec2 surfaceMaterial;
in vec4 surfaceTint;
in vec4 lightSpacePosition;
uniform vec3 sunDirection;
uniform vec3 sunColor;
uniform vec3 skyColor;
uniform vec3 groundColor;
uniform float bandCount;
uniform vec4 tintColor;
uniform float alpha;
uniform float hdrOutput;
uniform float shadowEnabled;
uniform float shadowStrength;
uniform float shadowTexelSize;
uniform sampler2DShadow shadowTexture;
out vec4 fragmentColor;
vec3 toLinear(vec3 color) {
	return pow(color, vec3(2.2));
}
vec3 toOutput(vec3 color) {
	if (hdrOutput > 0.5) {
		return max(color, vec3(0.0));
	}
	return pow(clamp(color, 0.0, 1.0), vec3(1.0 / 2.2));
}
float readShadow() {
	if (shadowEnabled < 0.5) {
		return 1.0;
	}
	vec3 projected = lightSpacePosition.xyz / lightSpacePosition.w * 0.5 + 0.5;
	if (projected.x < 0.0 || projected.x > 1.0 || projected.y < 0.0 || projected.y > 1.0 || projected.z > 1.0) {
		return 1.0;
	}
	float lit = 0.0;
	for (int offsetY = -1; offsetY <= 1; ++offsetY) {
		for (int offsetX = -1; offsetX <= 1; ++offsetX) {
			lit += texture(shadowTexture, vec3(projected.xy + vec2(float(offsetX), float(offsetY)) * shadowTexelSize, projected.z - 0.0015));
		}
	}
	return mix(1.0, lit / 9.0, shadowStrength);
}
void main() {
	vec4 tint = tintColor * surfaceTint;
	vec3 albedo = toLinear(surfaceColor) * toLinear(tint.rgb);
	float glow = surfaceMaterial.x;
	float unlit = surfaceMaterial.y;
	float outputAlpha = alpha * tint.a;
	if (unlit > 0.5) {
		fragmentColor = vec4(toOutput(albedo * (1.0 + glow)), outputAlpha);
		return;
	}
	vec3 normal = normalize(worldNormal);
	float facing = max(dot(normal, -normalize(sunDirection)), 0.0) * readShadow();
	if (bandCount > 0.5) {
		facing = floor(facing * bandCount + 0.5) / bandCount;
	}
	float upward = normal.y * 0.5 + 0.5;
	vec3 ambient = mix(groundColor, skyColor, upward);
	vec3 lit = albedo * (ambient + sunColor * facing) + albedo * glow;
	fragmentColor = vec4(toOutput(lit), outputAlpha);
}
";

const POSITION_VERTEX_HEAD: &str = "#version 300 es
in vec3 vertexPosition;
";

const OUTLINE_VERTEX_BODY: &str = "out float outlineTint;
void main() {
	mat4 skinMatrix = readSkinMatrix();
	outlineTint = readInstanceTint().a;
	gl_Position = viewProjectionMatrix * readModelMatrix() * skinMatrix * vec4(vertexPosition, 1.0);
}
";

const OUTLINE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in float outlineTint;
uniform vec4 outlineColor;
uniform float alpha;
uniform float hdrOutput;
out vec4 fragmentColor;
void main() {
	vec3 color = hdrOutput > 0.5 ? pow(outlineColor.rgb, vec3(2.2)) : outlineColor.rgb;
	fragmentColor = vec4(color, outlineColor.a * alpha * outlineTint);
}
";

const DEPTH_VERTEX_BODY: &str = "void main() {
	gl_Position = viewProjectionMatrix * readModelMatrix() * readSkinMatrix() * vec4(vertexPosition, 1.0);
}
";

const DEPTH_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
void main() {
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
	instance_buffer: GlBuffer,
	buffer_list: Vec<GlBuffer>,
	is_skinned: bool,
	bounding_center: [f32; 3],
	bounding_radius: f32,
}

impl ToonMesh {
	//==============================================================================
	// 뼈대로 움직이는 모형인지.
	//==============================================================================
	pub fn is_skinned(&self) -> bool {
		self.is_skinned
	}

	//==============================================================================
	// 경계 구. (모형 좌표의 가운데와 반지름, 뼈대 모형은 쉬는 자세의 1.5 배)
	//==============================================================================
	pub fn get_bounding_sphere(&self) -> ([f32; 3], f32) {
		(self.bounding_center, self.bounding_radius)
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
	pub is_culled: bool,
}

impl Default for ToonDrawOptions<'_> {
	fn default() -> Self {
		ToonDrawOptions { tint_color: [1.0; 4], alpha: 1.0, joint_matrices: None, has_outline: true, is_culled: true }
	}
}

//==============================================================================
// 인스턴스 하나. (모형 행렬과 틴트, 틴트의 알파는 바깥선에도 곱한다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonInstance {
	pub model_matrix: Matrix4,
	pub tint_color: [f32; 4],
}

//==============================================================================
// 받을 그림자. (깊이 텍스처, 빛의 보기와 투영, 해상도, 세기 0 ~ 1)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonShadowInput {
	pub depth_texture: GlTexture,
	pub light_view_projection_matrix: Matrix4,
	pub resolution: i32,
	pub strength: f32,
}

impl ToonShadowInput {
	//==============================================================================
	// 엔진 ShadowMap 에서. (세기 0.6)
	//==============================================================================
	pub fn from_shadow_map(shadow_map: &ShadowMap) -> ToonShadowInput {
		ToonShadowInput { depth_texture: shadow_map.get_depth_texture(), light_view_projection_matrix: *shadow_map.get_light_view_projection_matrix(), resolution: shadow_map.get_resolution(), strength: 0.6 }
	}
}

//==============================================================================
// 그린 수와 거른 수. (한 프레임, begin_frame 이 비운다)
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct ToonFrameStatistics {
	pub draw_call_count: u32,
	pub drawn_object_count: u32,
	pub culled_object_count: u32,
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
	viewport: [i32; 4],
	framebuffer: Option<GlFramebuffer>,
}

pub struct ToonRenderer {
	solid_program: ShaderProgram,
	outline_program: ShaderProgram,
	depth_program: ShaderProgram,
	view_projection_matrix: Matrix4,
	frustum_planes: [[f32; 4]; 6],
	sun_direction: [f32; 3],
	sun_color: [f32; 3],
	sky_color: [f32; 3],
	ground_color: [f32; 3],
	band_count: f32,
	outline_color: [f32; 4],
	is_hdr_output: bool,
	shadow_input: Option<ToonShadowInput>,
	is_shadow_pass: bool,
	saved_state: Option<SavedState>,
	instance_data: Vec<f32>,
	frame_statistics: ToonFrameStatistics,
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

//==============================================================================
// 보기와 투영 행렬에서 절두체의 면 여섯. (ax + by + cz + d >= 0 이 안쪽, 정규화)
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
// 행렬의 가장 큰 크기. (세 축 길이 가운데 큰 것)
//==============================================================================
fn read_maximum_scale(matrix: &Matrix4) -> f32 {
	let elements = &matrix.elements;
	let scale_x = (elements[0] * elements[0] + elements[1] * elements[1] + elements[2] * elements[2]).sqrt();
	let scale_y = (elements[4] * elements[4] + elements[5] * elements[5] + elements[6] * elements[6]).sqrt();
	let scale_z = (elements[8] * elements[8] + elements[9] * elements[9] + elements[10] * elements[10]).sqrt();
	scale_x.max(scale_y).max(scale_z)
}

//==============================================================================
// 꼭짓점 배열의 경계 구. (자리 셋씩, 가운데는 상자의 가운데)
//==============================================================================
fn read_bounding_sphere(positions: &[f32], stride: usize) -> ([f32; 3], f32) {
	let mut minimum = [f32::MAX; 3];
	let mut maximum = [f32::MIN; 3];
	for point in positions.chunks(stride) {
		if point.len() < 3 {
			continue;
		}
		for axis in 0..3 {
			minimum[axis] = minimum[axis].min(point[axis]);
			maximum[axis] = maximum[axis].max(point[axis]);
		}
	}
	if minimum[0] > maximum[0] {
		return ([0.0; 3], 0.0);
	}
	let center = [(minimum[0] + maximum[0]) * 0.5, (minimum[1] + maximum[1]) * 0.5, (minimum[2] + maximum[2]) * 0.5];
	let mut radius_squared: f32 = 0.0;
	for point in positions.chunks(stride) {
		if point.len() < 3 {
			continue;
		}
		let delta = [point[0] - center[0], point[1] - center[1], point[2] - center[2]];
		radius_squared = radius_squared.max(delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]);
	}
	(center, radius_squared.sqrt())
}

impl ToonRenderer {
	//==============================================================================
	// 셰이더 소스. ((정점, 조각) 세 벌: 몸, 바깥선, 그림자 깊이)
	//==============================================================================
	pub fn read_program_sources() -> [(String, String); 3] {
		[
			(format!("{}{}{}", SOLID_VERTEX_HEAD, VERTEX_COMMON_CHUNK, SOLID_VERTEX_BODY), String::from(SOLID_FRAGMENT_SHADER)),
			(format!("{}{}{}", POSITION_VERTEX_HEAD, VERTEX_COMMON_CHUNK, OUTLINE_VERTEX_BODY), String::from(OUTLINE_FRAGMENT_SHADER)),
			(format!("{}{}{}", POSITION_VERTEX_HEAD, VERTEX_COMMON_CHUNK, DEPTH_VERTEX_BODY), String::from(DEPTH_FRAGMENT_SHADER)),
		]
	}

	//==============================================================================
	// 생성. (세 프로그램을 짓는다)
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<ToonRenderer, String> {
		let [(solid_vertex_source, solid_fragment_source), (outline_vertex_source, outline_fragment_source), (depth_vertex_source, depth_fragment_source)] = ToonRenderer::read_program_sources();
		let solid_program = ShaderProgram::create_with_attribute_locations(gl, &solid_vertex_source, &solid_fragment_source, &TOON_ATTRIBUTE_LOCATIONS)?;
		let outline_program = ShaderProgram::create_with_attribute_locations(gl, &outline_vertex_source, &outline_fragment_source, &TOON_ATTRIBUTE_LOCATIONS)?;
		let depth_program = ShaderProgram::create_with_attribute_locations(gl, &depth_vertex_source, &depth_fragment_source, &TOON_ATTRIBUTE_LOCATIONS)?;
		Ok(ToonRenderer {
			solid_program,
			outline_program,
			depth_program,
			view_projection_matrix: Matrix4::default(),
			frustum_planes: [[0.0, 0.0, 0.0, 1.0]; 6],
			sun_direction: [-0.4, -1.0, -0.3],
			sun_color: [0.9, 0.85, 0.8],
			sky_color: [0.45, 0.5, 0.6],
			ground_color: [0.2, 0.18, 0.16],
			band_count: 0.0,
			outline_color: [0.05, 0.05, 0.08, 1.0],
			is_hdr_output: false,
			shadow_input: None,
			is_shadow_pass: false,
			saved_state: None,
			instance_data: Vec::new(),
			frame_statistics: ToonFrameStatistics::default(),
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
		let (bounding_center, mut bounding_radius) = read_bounding_sphere(&arrays.vertex_data, VERTEX_FLOAT_COUNT);
		if is_skinned {
			bounding_radius *= 1.5;
		}
		let mut buffer_list = Vec::new();
		unsafe {
			// 인스턴스 버퍼. (보통 그리기에서도 인스턴스 어트리뷰트가 켜져 있으므로 한 인스턴스 분량을 늘 채워 둔다, 비면 드라이버가 버퍼 밖을 읽는다)
			let instance_buffer = gl.create_buffer().ok()?;
			let mut default_instance = Matrix4::default().elements.to_vec();
			default_instance.extend_from_slice(&[1.0; 4]);
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&default_instance), glow::DYNAMIC_DRAW);

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
			ToonRenderer::attach_instance_attributes(gl, instance_buffer);
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
				ToonRenderer::attach_instance_attributes(gl, instance_buffer);
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
				instance_buffer,
				buffer_list,
				is_skinned,
				bounding_center,
				bounding_radius,
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
	// 인스턴스 버퍼를 지금 묶인 버텍스 어레이의 인스턴스 어트리뷰트로. (행렬 네 줄, 틴트, 인스턴스마다 하나씩)
	//==============================================================================
	unsafe fn attach_instance_attributes(gl: &glow::Context, instance_buffer: GlBuffer) {
		let stride = (TOON_INSTANCE_FLOAT_COUNT * 4) as i32;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buffer));
		for column_index in 0..5u32 {
			let location = 7 + column_index;
			gl.enable_vertex_attrib_array(location);
			gl.vertex_attrib_pointer_f32(location, 4, glow::FLOAT, false, stride, (column_index * 16) as i32);
			gl.vertex_attrib_divisor(location, 1);
		}
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
			gl.delete_buffer(mesh.instance_buffer);
			for buffer in mesh.buffer_list {
				gl.delete_buffer(buffer);
			}
		}
	}

	//==============================================================================
	// 그리기 전의 상태 적어 두기.
	//==============================================================================
	fn save_state(&mut self, gl: &glow::Context) {
		unsafe {
			let mut viewport = [0i32; 4];
			gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
			self.saved_state = Some(SavedState {
				is_blend_enabled: gl.is_enabled(glow::BLEND),
				is_depth_test_enabled: gl.is_enabled(glow::DEPTH_TEST),
				is_cull_face_enabled: gl.is_enabled(glow::CULL_FACE),
				depth_mask: gl.get_parameter_bool(glow::DEPTH_WRITEMASK),
				blend_source_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
				blend_destination_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
				blend_source_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
				blend_destination_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
				viewport,
				framebuffer: gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING),
			});
		}
	}

	//==============================================================================
	// 적어 둔 상태로 돌리기. (프로그램과 버텍스 어레이는 풀어 둔다)
	//==============================================================================
	fn restore_state(&mut self, gl: &glow::Context, is_framebuffer_restored: bool) {
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
			if is_framebuffer_restored {
				gl.bind_framebuffer(glow::FRAMEBUFFER, saved_state.framebuffer);
				gl.viewport(saved_state.viewport[0], saved_state.viewport[1], saved_state.viewport[2], saved_state.viewport[3]);
			}
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
	// 프레임 시작. (보기와 투영과 절두체를 정하고, 앞의 상태를 적어 둔 뒤 깊이 시험과 면 가리기를 켠다, 깊이를 비우는 것은 부르는 쪽이 정한다)
	//==============================================================================
	pub fn begin_frame(&mut self, gl: &glow::Context, view_projection_matrix: &Matrix4) {
		self.view_projection_matrix = *view_projection_matrix;
		self.frustum_planes = extract_frustum_planes(view_projection_matrix);
		self.frame_statistics = ToonFrameStatistics::default();
		self.is_shadow_pass = false;
		self.save_state(gl);
		unsafe {
			gl.disable(glow::BLEND);
			gl.enable(glow::DEPTH_TEST);
			gl.depth_func(glow::LEQUAL);
			gl.depth_mask(true);
			gl.enable(glow::CULL_FACE);
			gl.front_face(glow::CCW);
		}
	}

	//==============================================================================
	// 프레임 끝. (적어 둔 상태로 돌린다)
	//==============================================================================
	pub fn end_frame(&mut self, gl: &glow::Context) {
		self.restore_state(gl, false);
	}

	//==============================================================================
	// 그림자 깊이 패스 시작. (ShadowMap 의 프레임버퍼에 빛의 보기와 투영으로, 앞의 프레임버퍼와 뷰포트는 end_shadow_pass 가 돌린다)
	//==============================================================================
	pub fn begin_shadow_pass(&mut self, gl: &glow::Context, shadow_map: &ShadowMap) {
		self.save_state(gl);
		self.view_projection_matrix = *shadow_map.get_light_view_projection_matrix();
		self.frustum_planes = extract_frustum_planes(&self.view_projection_matrix);
		self.is_shadow_pass = true;
		shadow_map.begin_render(gl);
		unsafe {
			gl.disable(glow::BLEND);
			gl.depth_func(glow::LEQUAL);
			gl.enable(glow::CULL_FACE);
			gl.cull_face(glow::BACK);
		}
	}

	//==============================================================================
	// 그림자 깊이 패스 끝. (프레임버퍼와 뷰포트와 상태를 돌린다)
	//==============================================================================
	pub fn end_shadow_pass(&mut self, gl: &glow::Context) {
		self.is_shadow_pass = false;
		self.restore_state(gl, true);
	}

	//==============================================================================
	// 경계 구가 절두체 안에 드는지. (모형 행렬로 옮긴 가운데, 가장 큰 크기를 곱한 반지름)
	//==============================================================================
	pub fn is_visible(&self, mesh: &ToonMesh, model_matrix: &Matrix4) -> bool {
		let center = mesh.bounding_center;
		let transformed_center = model_matrix.transform_point(center[0], center[1], center[2]);
		let radius = mesh.bounding_radius * read_maximum_scale(model_matrix);
		self.frustum_planes.iter().all(|plane| plane[0] * transformed_center.x + plane[1] * transformed_center.y + plane[2] * transformed_center.z + plane[3] >= -radius)
	}

	//==============================================================================
	// 모형 하나 그리기. (그림자 패스면 깊이만, 화면 밖이면 건너뛴다)
	//==============================================================================
	pub fn draw_mesh(&mut self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions) {
		if options.is_culled && !self.is_visible(mesh, model_matrix) {
			self.frame_statistics.culled_object_count += 1;
			return;
		}
		self.frame_statistics.drawn_object_count += 1;
		self.draw_with_count(gl, mesh, Some(model_matrix), 0, options);
	}

	//==============================================================================
	// 같은 모형 여럿을 한 번에 그리기. (GPU 인스턴싱, 화면 밖 인스턴스는 걸러 남은 것만 올린다)
	// - 뼈대 모형은 모든 인스턴스가 같은 자세다. (options 의 joint_matrices)
	//==============================================================================
	pub fn draw_mesh_instanced(&mut self, gl: &glow::Context, mesh: &ToonMesh, instance_list: &[ToonInstance], options: ToonDrawOptions) {
		let mut instance_data = std::mem::take(&mut self.instance_data);
		instance_data.clear();
		let mut instance_count = 0;
		for instance in instance_list {
			if options.is_culled && !self.is_visible(mesh, &instance.model_matrix) {
				self.frame_statistics.culled_object_count += 1;
				continue;
			}
			instance_data.extend_from_slice(&instance.model_matrix.elements);
			instance_data.extend_from_slice(&instance.tint_color);
			instance_count += 1;
		}
		self.frame_statistics.drawn_object_count += instance_count;
		if instance_count > 0 {
			unsafe {
				gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.instance_buffer));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&instance_data), glow::DYNAMIC_DRAW);
			}
			self.draw_with_count(gl, mesh, None, instance_count as i32, options);
		}
		self.instance_data = instance_data;
	}

	//==============================================================================
	// 그리기 본체. (model_matrix 가 없으면 instance_count 만큼 인스턴스로)
	//==============================================================================
	fn draw_with_count(&mut self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: Option<&Matrix4>, instance_count: i32, options: ToonDrawOptions) {
		let is_skinned = mesh.is_skinned && options.joint_matrices.is_some();
		let identity_matrix = Matrix4::default();
		let model_matrix = model_matrix.unwrap_or(&identity_matrix);
		let is_instanced = instance_count > 0;
		let draw_elements = |gl: &glow::Context, index_count: i32| unsafe {
			if is_instanced {
				gl.draw_elements_instanced(glow::TRIANGLES, index_count, glow::UNSIGNED_INT, 0, instance_count);
			}
			else {
				gl.draw_elements(glow::TRIANGLES, index_count, glow::UNSIGNED_INT, 0);
			}
		};

		// 그림자 패스. (몸의 깊이만)
		if self.is_shadow_pass {
			let depth_program = &self.depth_program;
			depth_program.use_program(gl);
			self.upload_common_uniforms(gl, depth_program, model_matrix, is_skinned, is_instanced, options.joint_matrices);
			unsafe {
				gl.bind_vertex_array(Some(mesh.body_vertex_array));
			}
			draw_elements(gl, mesh.body_index_count);
			self.frame_statistics.draw_call_count += 1;
			return;
		}

		let is_transparent = options.alpha < 1.0 || options.tint_color[3] < 1.0;
		let hdr_output = if self.is_hdr_output { 1.0 } else { 0.0 };
		unsafe {
			if is_transparent {
				gl.enable(glow::BLEND);
				gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
			}

			// 바깥선. (껍데기의 뒷면만)
			if let (true, Some(outline_vertex_array)) = (options.has_outline, mesh.outline_vertex_array) {
				let outline_program = &self.outline_program;
				outline_program.use_program(gl);
				self.upload_common_uniforms(gl, outline_program, model_matrix, is_skinned, is_instanced, options.joint_matrices);
				let outline_color = self.outline_color;
				gl.uniform_4_f32(outline_program.get_uniform_location(gl, "outlineColor").as_ref(), outline_color[0], outline_color[1], outline_color[2], outline_color[3]);
				gl.uniform_1_f32(outline_program.get_uniform_location(gl, "alpha").as_ref(), options.alpha);
				gl.uniform_1_f32(outline_program.get_uniform_location(gl, "hdrOutput").as_ref(), hdr_output);
				gl.cull_face(glow::FRONT);
				gl.bind_vertex_array(Some(outline_vertex_array));
				draw_elements(gl, mesh.outline_index_count);
				self.frame_statistics.draw_call_count += 1;
			}

			// 몸.
			let solid_program = &self.solid_program;
			solid_program.use_program(gl);
			self.upload_common_uniforms(gl, solid_program, model_matrix, is_skinned, is_instanced, options.joint_matrices);
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
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "hdrOutput").as_ref(), hdr_output);
			match self.shadow_input {
				Some(shadow_input) => {
					gl.active_texture(glow::TEXTURE0 + SHADOW_TEXTURE_UNIT);
					gl.bind_texture(glow::TEXTURE_2D, Some(shadow_input.depth_texture));
					gl.uniform_1_i32(solid_program.get_uniform_location(gl, "shadowTexture").as_ref(), SHADOW_TEXTURE_UNIT as i32);
					gl.uniform_1_f32(solid_program.get_uniform_location(gl, "shadowEnabled").as_ref(), 1.0);
					gl.uniform_1_f32(solid_program.get_uniform_location(gl, "shadowStrength").as_ref(), shadow_input.strength);
					gl.uniform_1_f32(solid_program.get_uniform_location(gl, "shadowTexelSize").as_ref(), 1.0 / shadow_input.resolution.max(1) as f32);
					gl.uniform_matrix_4_f32_slice(solid_program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &shadow_input.light_view_projection_matrix.elements);
					gl.active_texture(glow::TEXTURE0);
				}
				None => {
					gl.uniform_1_f32(solid_program.get_uniform_location(gl, "shadowEnabled").as_ref(), 0.0);
					gl.uniform_1_i32(solid_program.get_uniform_location(gl, "shadowTexture").as_ref(), SHADOW_TEXTURE_UNIT as i32);
				}
			}
			gl.cull_face(glow::BACK);
			gl.bind_vertex_array(Some(mesh.body_vertex_array));
			draw_elements(gl, mesh.body_index_count);
			self.frame_statistics.draw_call_count += 1;
			if is_transparent {
				gl.disable(glow::BLEND);
			}
		}
	}

	//==============================================================================
	// 세 프로그램이 함께 쓰는 유니폼 올리기. (보기와 투영, 모형 행렬, 뼈대, 인스턴싱)
	//==============================================================================
	fn upload_common_uniforms(&self, gl: &glow::Context, shader_program: &ShaderProgram, model_matrix: &Matrix4, is_skinned: bool, is_instanced: bool, joint_matrices: Option<&[f32]>) {
		unsafe {
			gl.uniform_matrix_4_f32_slice(shader_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &self.view_projection_matrix.elements);
			gl.uniform_matrix_4_f32_slice(shader_program.get_uniform_location(gl, "modelMatrix").as_ref(), false, &model_matrix.elements);
			gl.uniform_1_f32(shader_program.get_uniform_location(gl, "instanced").as_ref(), if is_instanced { 1.0 } else { 0.0 });
			gl.uniform_1_f32(shader_program.get_uniform_location(gl, "skinned").as_ref(), if is_skinned { 1.0 } else { 0.0 });
			if let (true, Some(joint_matrices)) = (is_skinned, joint_matrices) {
				let joint_float_count = joint_matrices.len().min(TOON_MAXIMUM_JOINTS * 16) / 16 * 16;
				gl.uniform_matrix_4_f32_slice(shader_program.get_uniform_location(gl, "jointMatrices").as_ref(), false, &joint_matrices[..joint_float_count]);
			}
		}
	}

	//==============================================================================
	// 빛 설정. (해가 비추는 방향, 해의 색, 하늘색, 땅색, 모두 선형 색 0 ~, HDR 이면 1 을 넘어도 된다)
	//==============================================================================
	pub fn set_light(&mut self, sun_direction: [f32; 3], sun_color: [f32; 3], sky_color: [f32; 3], ground_color: [f32; 3]) {
		self.sun_direction = sun_direction;
		self.sun_color = sun_color;
		self.sky_color = sky_color;
		self.ground_color = ground_color;
	}

	//==============================================================================
	// 해가 비추는 방향 반환. (ShadowMap::update_light_matrix 에 그대로 넘긴다)
	//==============================================================================
	pub fn get_sun_direction(&self) -> [f32; 3] {
		self.sun_direction
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

	//==============================================================================
	// HDR 출력 설정. (true 면 감마를 걸지 않은 선형 색을 낸다, ToonPostProcess 의 실수 렌더 타겟에 그릴 때)
	//==============================================================================
	pub fn set_hdr_output(&mut self, is_hdr_output: bool) {
		self.is_hdr_output = is_hdr_output;
	}

	//==============================================================================
	// 받을 그림자 설정. (None 이면 받지 않는다)
	//==============================================================================
	pub fn set_shadow(&mut self, shadow_input: Option<ToonShadowInput>) {
		self.shadow_input = shadow_input;
	}

	//==============================================================================
	// 이번 프레임의 그린 수와 거른 수.
	//==============================================================================
	pub fn get_frame_statistics(&self) -> ToonFrameStatistics {
		self.frame_statistics
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::vector3::Vector3;

	#[test]
	fn frustum_keeps_center_and_drops_behind() {
		let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 10.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let mut view_projection_matrix = Matrix4::create_perspective(0.8, 1.0, 0.1, 100.0);
		view_projection_matrix.multiply(&view_matrix);
		let planes = extract_frustum_planes(&view_projection_matrix);
		let is_inside = |point: [f32; 3], radius: f32| planes.iter().all(|plane| plane[0] * point[0] + plane[1] * point[1] + plane[2] * point[2] + plane[3] >= -radius);
		assert!(is_inside([0.0, 0.0, 0.0], 1.0));
		assert!(!is_inside([0.0, 0.0, 20.0], 1.0));
		assert!(!is_inside([50.0, 0.0, 0.0], 1.0));
		assert!(is_inside([5.0, 0.0, 0.0], 2.0));
	}
}
