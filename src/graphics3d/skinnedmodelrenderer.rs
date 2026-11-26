//==============================================================================
//! 스킨드 모델 렌더러. (원본 graphics3d/skinnedmodelrenderer.js)
//! - 스키닝 PBR 셰이더 템플릿 + 환경 상태(섀도우 맵 / 태양 / 안개) 소유, 씬당 하나. SkinnedModel 은 데이터/포즈만 갖고,
//!   렌더링은 이 객체가 모델 인스턴스를 받아 한다.
//! - 프래그먼트 셰이더 소스를 주입하면 같은 스키닝/모프 버텍스 경로 위에 다른 셰이딩 템플릿을 세울 수 있다.
//!   (파생 렌더러는 셰이더를 쓴 뒤 고유 유니폼을 설정하고 draw() 를 부른다)
//! - GLSL 은 원본과 같다. (쿡-토런스 GGX, 화면공간 미분 코탄젠트 노멀맵, 3x3 PCF 섀도우, 반구 앰비언트, 거리 안개)
//!   원본 템플릿 문자열의 ${MORPH_GLSL} / ${MORPH_TARGET_MAXIMUM} 은 concat! 과 숫자 96 으로 옮겼다. (시험이 둘이 같은지 본다)
//! - 텍스처 유닛: 0 = base_color, 1 = 섀도우 맵, 2 ~ 8 = 머티리얼 슬롯, 15 = 모프 타깃 텍스처.
//!   섀도우 맵이 없으면 섀도우 유닛에 1x1 비교 깊이 텍스처를 묶는다. (원본과 같다)
//! - 원본과 다른 것:
//!   - 그리기 함수는 gl 과 &mut SkinnedModel 을 받는다. 바뀐 모프 타깃 업로드(원본 update 6단계)와 와이어프레임 인덱스 업로드를
//!     여기서 먼저 한다. 행렬 인자는 원본 getElements() 처럼 &[f64; 16] 이다. (matrix.get_elements())
//!   - set_shadow_map 은 원본처럼 배열을 참조로 들지 않고 16개를 복사해 둔다.
//!   - 생성은 셰이더 컴파일 실패를 오류(Result)로 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::material::TEXTURE_UNIT_SHADOWMAP;
use crate::graphics3d::skinnedmodel::{Drawable, SkinnedModel};
use glow::HasContext;
use crate::core::gluniform::GlUniformF64;
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 모프 타깃 텍스처 유닛. (머티리얼 슬롯 0 / 2~8, 섀도우 맵 1, 추가 슬롯 9~ 와 겹치지 않는 값)
pub const MORPH_TEXTURE_UNIT: u32 = 15;

/// 모프 타깃 GLSL 본문. (버텍스 셰이더 삽입, 실수 텍스처의 위치/노멀 델타를 가중 합산, 배열 크기는 MORPH_TARGET_MAXIMUM)
/// - 텍셀 인덱스 = gl_VertexID, 타깃 블록 = [위치 행들][노멀 행들] 순서.
macro_rules! morphGlslSource {
	() => {
		"
uniform sampler2D morphTexture;
uniform int morphTargetCount;
uniform int morphRowsPerTarget;
uniform int morphTextureWidth;
uniform float morphWeights[96];

vec3 fetchMorphDelta(int blockRow, int vertexIndex) {
	int texelX = vertexIndex % morphTextureWidth;
	int texelY = blockRow + vertexIndex / morphTextureWidth;
	return texelFetch(morphTexture, ivec2(texelX, texelY), 0).xyz;
}

void applyMorphTargets(inout vec3 position, inout vec3 normal) {
	for (int targetIndex = 0; targetIndex < morphTargetCount; ++targetIndex) {
		float weight = morphWeights[targetIndex];
		if (abs(weight) < 0.0001) {
			continue;
		}
		int blockRow = targetIndex * 2 * morphRowsPerTarget;
		position += weight * fetchMorphDelta(blockRow, gl_VertexID);
		normal += weight * fetchMorphDelta(blockRow + morphRowsPerTarget, gl_VertexID);
	}
}
"
	};
}

/// 모프 타깃 GLSL. (원본 MORPH_GLSL, 파생 렌더러가 대체 버텍스 셰이더에 넣어 쓴다)
pub const MORPH_GLSL: &str = morphGlslSource!();

/// 스키닝 버텍스 셰이더. (모프 타깃 → 조인트 4개 가중 혼합, 어트리뷰트 위치 고정)
pub const SKINNED_VERTEXSHADER_SOURCE: &str = concat!("#version 300 es
layout(location = 0) in vec3 vertexPosition;
layout(location = 1) in vec3 vertexNormal;
layout(location = 2) in vec2 vertexTextureCoordinate;
layout(location = 3) in uvec4 vertexJoints;
layout(location = 4) in vec4 vertexWeights;
uniform mat4 modelMatrix;
uniform mat4 viewProjectionMatrix;
uniform mat4 lightViewProjectionMatrix;
uniform mat4 jointMatrices[96];
out vec3 worldPosition;
out vec3 worldNormal;
out vec2 fragmentTextureCoordinate;
out vec4 lightSpacePosition;
", morphGlslSource!(), "
void main() {
	// 노멀 미보유 모델 가드. (비활성 어트리뷰트는 영벡터, 위쪽으로 대체)
	vec3 safeNormal = dot(vertexNormal, vertexNormal) < 0.0001 ? vec3(0.0, 1.0, 0.0) : vertexNormal;
	vec3 morphedPosition = vertexPosition;
	vec3 morphedNormal = safeNormal;
	applyMorphTargets(morphedPosition, morphedNormal);

	mat4 skinMatrix = vertexWeights.x * jointMatrices[vertexJoints.x]
		+ vertexWeights.y * jointMatrices[vertexJoints.y]
		+ vertexWeights.z * jointMatrices[vertexJoints.z]
		+ vertexWeights.w * jointMatrices[vertexJoints.w];
	vec4 skinnedPosition = modelMatrix * skinMatrix * vec4(morphedPosition, 1.0);
	worldPosition = skinnedPosition.xyz;
	worldNormal = normalize(mat3(modelMatrix) * mat3(skinMatrix) * normalize(morphedNormal));
	fragmentTextureCoordinate = vertexTextureCoordinate;
	lightSpacePosition = lightViewProjectionMatrix * skinnedPosition;
	gl_Position = viewProjectionMatrix * skinnedPosition;
}
");

/// 스키닝 프래그먼트 셰이더. (metallic-roughness PBR + 태양/반구 라이팅 + 섀도우 맵 + 거리 안개)
/// - 노멀 매핑은 화면 공간 미분 기반 코탄젠트 프레임 사용. (탄젠트 어트리뷰트 불필요)
/// - FBX 스펙큘러/글로스 경로: use_glossiness=1 이면 러프니스 = 1 - 글로스, 스펙큘러 맵은 F0 변조.
pub const SKINNED_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec3 worldPosition;
in vec3 worldNormal;
in vec2 fragmentTextureCoordinate;
in vec4 lightSpacePosition;
uniform sampler2D baseColorTexture;
uniform sampler2D normalTexture;
uniform sampler2D metallicRoughnessTexture;
uniform sampler2D glossinessTexture;
uniform sampler2D occlusionTexture;
uniform sampler2D emissiveTexture;
uniform sampler2D specularTexture;
uniform sampler2D opacityTexture;
uniform vec3 baseColorFactor;
uniform float metallicFactor;
uniform float roughnessFactor;
uniform vec3 emissiveFactor;
uniform int useGlossiness;
uniform int useAlphaCutout;
uniform highp sampler2DShadow shadowMapTexture;
uniform float shadowStrength;
uniform float shadowTexelSize;
uniform vec3 sunDirection;
uniform vec3 cameraPosition;
uniform vec3 fogColor;
uniform vec2 fogRange;
out vec4 outputColor;

float sampleShadowFactor() {
	vec3 projected = lightSpacePosition.xyz / lightSpacePosition.w;
	projected = projected * 0.5 + 0.5;
	if (projected.x < 0.0 || projected.x > 1.0 || projected.y < 0.0 || projected.y > 1.0 || projected.z > 1.0) {
		return 1.0;
	}
	float shadowSum = 0.0;
	for (int offsetY = -1; offsetY <= 1; ++offsetY) {
		for (int offsetX = -1; offsetX <= 1; ++offsetX) {
			vec2 tapOffset = vec2(float(offsetX), float(offsetY)) * shadowTexelSize;
			shadowSum += texture(shadowMapTexture, vec3(projected.xy + tapOffset, projected.z - 0.0018));
		}
	}
	return shadowSum / 9.0;
}

vec3 applyNormalMap(vec3 geometryNormal) {
	vec3 tangentNormal = texture(normalTexture, fragmentTextureCoordinate).xyz * 2.0 - 1.0;
	vec3 positionDx = dFdx(worldPosition);
	vec3 positionDy = dFdy(worldPosition);
	vec2 textureDx = dFdx(fragmentTextureCoordinate);
	vec2 textureDy = dFdy(fragmentTextureCoordinate);
	vec3 perpendicularX = cross(positionDy, geometryNormal);
	vec3 perpendicularY = cross(geometryNormal, positionDx);
	vec3 tangent = perpendicularX * textureDx.x + perpendicularY * textureDy.x;
	vec3 bitangent = perpendicularX * textureDx.y + perpendicularY * textureDy.y;
	float maxLengthSquared = max(dot(tangent, tangent), dot(bitangent, bitangent));
	if (maxLengthSquared < 1e-16) {
		return geometryNormal;
	}
	float inverseLength = inversesqrt(maxLengthSquared);
	mat3 tangentFrame = mat3(tangent * inverseLength, bitangent * inverseLength, geometryNormal);
	return normalize(tangentFrame * tangentNormal);
}

void main() {
	// 알파 컷아웃. (오파시티 맵 보유 재질, 머리카락/속눈썹의 투명 영역 제거)
	if (useAlphaCutout == 1) {
		float opacity = texture(opacityTexture, fragmentTextureCoordinate).r;
		if (opacity < 0.5) {
			discard;
		}
	}

	vec3 albedo = texture(baseColorTexture, fragmentTextureCoordinate).rgb * baseColorFactor;
	float metallic = metallicFactor;
	float roughness = roughnessFactor;
	if (useGlossiness == 1) {
		float glossiness = texture(glossinessTexture, fragmentTextureCoordinate).r;
		roughness = clamp(roughnessFactor * (1.0 - glossiness), 0.045, 1.0);
	}
	else {
		vec4 metallicRoughnessSample = texture(metallicRoughnessTexture, fragmentTextureCoordinate);
		roughness = clamp(roughnessFactor * metallicRoughnessSample.g, 0.045, 1.0);
		metallic = metallicFactor * metallicRoughnessSample.b;
	}
	float occlusion = texture(occlusionTexture, fragmentTextureCoordinate).r;
	vec3 emissive = texture(emissiveTexture, fragmentTextureCoordinate).rgb * emissiveFactor;
	vec3 specularTint = texture(specularTexture, fragmentTextureCoordinate).rgb;

	vec3 geometryNormal = normalize(worldNormal);
	vec3 viewDirection = normalize(cameraPosition - worldPosition);
	if (dot(geometryNormal, viewDirection) < 0.0) {
		geometryNormal = -geometryNormal;
	}
	vec3 surfaceNormal = applyNormalMap(geometryNormal);

	float shadowFactor = 1.0;
	if (shadowStrength > 0.001) {
		shadowFactor = mix(1.0, sampleShadowFactor(), shadowStrength);
	}

	// 쿡-토런스 GGX 직접광. (태양)
	vec3 lightDirection = normalize(sunDirection);
	vec3 halfVector = normalize(viewDirection + lightDirection);
	float normalDotLight = max(dot(surfaceNormal, lightDirection), 0.0);
	float normalDotView = max(dot(surfaceNormal, viewDirection), 0.0001);
	float normalDotHalf = max(dot(surfaceNormal, halfVector), 0.0);
	float viewDotHalf = max(dot(viewDirection, halfVector), 0.0);
	float alpha = roughness * roughness;
	float alphaSquared = alpha * alpha;
	float distributionDenominator = normalDotHalf * normalDotHalf * (alphaSquared - 1.0) + 1.0;
	float distribution = alphaSquared / (3.14159265 * distributionDenominator * distributionDenominator);
	float geometryK = alpha * 0.5;
	float geometryTerm = (normalDotLight / (normalDotLight * (1.0 - geometryK) + geometryK)) * (normalDotView / (normalDotView * (1.0 - geometryK) + geometryK));
	vec3 fresnelBase = mix(vec3(0.04) * specularTint, albedo, metallic);
	vec3 fresnel = fresnelBase + (vec3(1.0) - fresnelBase) * pow(1.0 - viewDotHalf, 5.0);
	vec3 specular = fresnel * (distribution * geometryTerm / max(4.0 * normalDotView * normalDotLight, 0.0001));
	vec3 diffuse = albedo * (1.0 - metallic) * (vec3(1.0) - fresnel) / 3.14159265;
	vec3 sunColor = vec3(1.15, 1.05, 0.9) * 3.14159265;
	vec3 directLight = (diffuse + specular) * sunColor * normalDotLight * shadowFactor;

	// 반구 앰비언트 + 이미시브.
	vec3 skyAmbient = mix(vec3(0.3, 0.33, 0.36), vec3(0.52, 0.58, 0.66), surfaceNormal.y * 0.5 + 0.5);
	vec3 ambientLight = albedo * skyAmbient * (0.6 + 0.4 * shadowFactor) * occlusion;
	vec3 color = directLight + ambientLight + emissive;
	float viewDistance = distance(cameraPosition, worldPosition);
	float fogBlend = smoothstep(fogRange.x, fogRange.y, viewDistance);
	color = mix(color, fogColor, fogBlend);
	outputColor = vec4(color, 1.0);
}";

/// 깊이 전용 스키닝 버텍스 셰이더. (섀도우 맵 캐스팅, 모프 타깃 포함)
const SKINNED_DEPTH_VERTEXSHADER_SOURCE: &str = concat!("#version 300 es
layout(location = 0) in vec3 vertexPosition;
layout(location = 3) in uvec4 vertexJoints;
layout(location = 4) in vec4 vertexWeights;
uniform mat4 modelMatrix;
uniform mat4 lightViewProjectionMatrix;
uniform mat4 jointMatrices[96];
", morphGlslSource!(), "
void main() {
	vec3 morphedPosition = vertexPosition;
	vec3 morphedNormal = vec3(0.0, 1.0, 0.0);
	applyMorphTargets(morphedPosition, morphedNormal);
	mat4 skinMatrix = vertexWeights.x * jointMatrices[vertexJoints.x]
		+ vertexWeights.y * jointMatrices[vertexJoints.y]
		+ vertexWeights.z * jointMatrices[vertexJoints.z]
		+ vertexWeights.w * jointMatrices[vertexJoints.w];
	gl_Position = lightViewProjectionMatrix * modelMatrix * skinMatrix * vec4(morphedPosition, 1.0);
}
");

/// 깊이 전용 프래그먼트 셰이더.
const SKINNED_DEPTH_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
void main() {
}";

/// 와이어프레임 버텍스 셰이더. (모프 + 스키닝을 그대로 따르는 선 출력)
const SKINNED_WIREFRAME_VERTEXSHADER_SOURCE: &str = concat!("#version 300 es
layout(location = 0) in vec3 vertexPosition;
layout(location = 3) in uvec4 vertexJoints;
layout(location = 4) in vec4 vertexWeights;
uniform mat4 modelMatrix;
uniform mat4 viewProjectionMatrix;
uniform mat4 jointMatrices[96];
", morphGlslSource!(), "
void main() {
	vec3 morphedPosition = vertexPosition;
	vec3 morphedNormal = vec3(0.0, 1.0, 0.0);
	applyMorphTargets(morphedPosition, morphedNormal);
	mat4 skinMatrix = vertexWeights.x * jointMatrices[vertexJoints.x]
		+ vertexWeights.y * jointMatrices[vertexJoints.y]
		+ vertexWeights.z * jointMatrices[vertexJoints.z]
		+ vertexWeights.w * jointMatrices[vertexJoints.w];
	gl_Position = viewProjectionMatrix * modelMatrix * skinMatrix * vec4(morphedPosition, 1.0);
}
");

/// 와이어프레임 프래그먼트 셰이더. (단색)
const SKINNED_WIREFRAME_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
uniform vec4 wireframeColor;
out vec4 outputColor;
void main() {
	outputColor = wireframeColor;
}";

//==============================================================================
/// 스킨드 모델 렌더러.
///
/// # 예제
/// 렌더러를 짓고 태양과 안개를 맞춘 뒤 모델 하나 그리기
/// ```rust,no_run
/// use glow::HasContext;
/// use vanilla::{Graphic, Matrix4, SkinnedModel, SkinnedModelRenderer, Vector3};
///
/// fn create_renderer(graphic: &Graphic) -> Result<SkinnedModelRenderer, String> {
/// 	let mut skinned_model_renderer = SkinnedModelRenderer::new(graphic.get_gl(), None, None)?;
/// 	let sun_direction = Vector3::create(0.5, 0.8, 0.35).normalize();
/// 	skinned_model_renderer.set_sun_direction(sun_direction.x, sun_direction.y, sun_direction.z);
/// 	skinned_model_renderer.set_fog_color(0.62, 0.72, 0.86);
/// 	skinned_model_renderer.set_fog_range(25.0, 80.0);
/// 	Ok(skinned_model_renderer)
/// }
///
/// fn draw_model(graphic: &mut Graphic, skinned_model_renderer: &SkinnedModelRenderer, skinned_model: &mut SkinnedModel) {
/// 	graphic.flush_batch();
/// 	let aspect_ratio = graphic.get_drawing_buffer_width() as f64 / graphic.get_drawing_buffer_height().max(1) as f64;
/// 	let camera_position = Vector3::create(0.0, 1.6, 4.0);
/// 	let mut view_projection_matrix = Matrix4::create_perspective(std::f64::consts::PI / 3.0, aspect_ratio, 0.1, 100.0);
/// 	view_projection_matrix.multiply(&Matrix4::create_look_at(camera_position, Vector3::create(0.0, 1.0, 0.0), Vector3::up()));
/// 	let model_matrix = Matrix4::create_identity();
/// 	let gl = graphic.get_gl();
/// 	unsafe {
/// 		gl.enable(glow::DEPTH_TEST);
/// 		gl.clear(glow::DEPTH_BUFFER_BIT);
/// 	}
/// 	skinned_model_renderer.draw(gl, skinned_model, view_projection_matrix.get_elements(), model_matrix.get_elements(), camera_position.x, camera_position.y, camera_position.z);
/// 	graphic.restore_render_state();
/// }
/// ```
/// <!-- image: model-viewer.webp -->
//==============================================================================
pub struct SkinnedModelRenderer {
	shader_program: Rc<ShaderProgram>,
	depth_shader_program: ShaderProgram,
	wireframe_shader_program: ShaderProgram,
	shadow_map_texture: Option<GlTexture>,
	/// 그림자를 받지 않을 때 섀도우 유닛에 묶는 1x1 비교 깊이 텍스처.
	empty_shadow_texture: GlTexture,
	light_view_projection_elements: Option<[f32; 16]>,
	shadow_strength: f64,
	shadow_texel_size: f64,
	sun_direction: [f64; 3],
	fog_color: [f64; 3],
	fog_range: [f64; 2],
}

impl SkinnedModelRenderer {
	//==============================================================================
	/// 생성. (대체 프래그먼트 / 버텍스 셰이더 소스, None 이면 기본 PBR / 기본 스키닝)
	/// - 대체 버텍스 셰이더는 같은 어트리뷰트 위치 / 출력 규약을 지켜야 한다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `fragment_shader_source`: 대체 프래그먼트 셰이더 소스. (GLSL ES 3.0, None 이면 기본 PBR)
	/// - `vertex_shader_source`: 대체 버텍스 셰이더 소스. (None 이면 기본 스키닝)
	///
	/// # 돌려줌
	/// 새 렌더러. (그림자 강도 0, 태양 방향 (0, 1, 0), 안개 색 (0.8, 0.85, 0.92), 안개 거리 55 ~ 250) 셰이더 컴파일이나 링크, 빈 그림자 텍스처 만들기에 실패하면 Err.
	///
	/// # 예제
	/// 기본 스키닝 버텍스 경로 위에 단색 셰이딩 끼우기
	/// ```rust,no_run
	/// use vanilla::SkinnedModelRenderer;
	///
	/// const FLAT_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
	/// precision highp float;
	/// out vec4 outputColor;
	/// void main() {
	/// 	outputColor = vec4(1.0, 0.5, 0.2, 1.0);
	/// }";
	///
	/// fn create_flat_renderer(gl: &glow::Context) -> Result<SkinnedModelRenderer, String> {
	/// 	SkinnedModelRenderer::new(gl, Some(FLAT_FRAGMENTSHADER_SOURCE), None)
	/// }
	/// ```
	//==============================================================================
	pub fn new(gl: &glow::Context, fragment_shader_source: Option<&str>, vertex_shader_source: Option<&str>) -> Result<SkinnedModelRenderer, String> {
		let resolved_fragment_shader_source = fragment_shader_source.unwrap_or(SKINNED_FRAGMENTSHADER_SOURCE);
		let resolved_vertex_shader_source = vertex_shader_source.unwrap_or(SKINNED_VERTEXSHADER_SOURCE);
		let shader_program = ShaderProgram::new(gl, resolved_vertex_shader_source.trim(), resolved_fragment_shader_source.trim())?;
		let depth_shader_program = ShaderProgram::new(gl, SKINNED_DEPTH_VERTEXSHADER_SOURCE.trim(), SKINNED_DEPTH_FRAGMENTSHADER_SOURCE.trim())?;
		let wireframe_shader_program = ShaderProgram::new(gl, SKINNED_WIREFRAME_VERTEXSHADER_SOURCE.trim(), SKINNED_WIREFRAME_FRAGMENTSHADER_SOURCE.trim())?;
		let empty_shadow_texture = SkinnedModelRenderer::create_empty_shadow_texture(gl)?;
		Ok(SkinnedModelRenderer {
			shader_program: Rc::new(shader_program),
			depth_shader_program,
			wireframe_shader_program,
			shadow_map_texture: None,
			empty_shadow_texture,
			light_view_projection_elements: None,
			shadow_strength: 0.0,
			shadow_texel_size: 1.0 / 2048.0,
			sun_direction: [0.0, 1.0, 0.0],
			fog_color: [0.8, 0.85, 0.92],
			fog_range: [55.0, 250.0],
		})
	}

	//==============================================================================
	/// 빈 그림자 텍스처 짓기. (1x1 깊이 1, 비교 샘플링)
	/// - sampler2DShadow 유닛에 비교 모드가 없는 텍스처가 묶이면 WebGL 이 그리기를 거부하고, 깊이 텍스처가 없으면 맥 드라이버가 알린다.
	//==============================================================================
	fn create_empty_shadow_texture(gl: &glow::Context) -> Result<GlTexture, String> {
		unsafe {
			let texture = gl.create_texture()?;
			gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT_SHADOWMAP);
			gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			// 웹의 glow 는 UNSIGNED_INT 바이트를 u32 배열로 보므로 4 바이트 맞춘 u32 에서 바이트를 읽는다.
			let depth_values = [u32::MAX];
			let depth_bytes = std::slice::from_raw_parts(depth_values.as_ptr() as *const u8, std::mem::size_of_val(&depth_values));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, 1, 1, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(Some(depth_bytes)));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
			Ok(texture)
		}
	}

	//==============================================================================
	/// 출력. (모델 인스턴스의 드로어블 순회, 머티리얼 적용 + 조인트 행렬 업로드)
	/// - 깊이 테스트 / 컬링 / 블렌드 / 프레임버퍼는 호출자가 잡는다. (원본과 같다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `skinned_model`: 그릴 모델. (바뀐 모프 타깃을 먼저 올린다)
	/// - `view_projection_elements`: 뷰 x 투영 행렬. (열 우선 16개, matrix.get_elements())
	/// - `model_matrix_elements`: 모델 행렬. (열 우선 16개)
	/// - `camera_x`: 카메라 월드 X. (정반사와 안개 거리에 쓴다)
	/// - `camera_y`: 카메라 월드 Y.
	/// - `camera_z`: 카메라 월드 Z.
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context, skinned_model: &mut SkinnedModel, view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16], camera_x: f64, camera_y: f64, camera_z: f64) {
		skinned_model.upload_pending_morph_targets(gl);
		let shader_program = &*self.shader_program;
		shader_program.use_program(gl);
		let view_projection_location = shader_program.get_uniform_location(gl, "viewProjectionMatrix");
		let model_matrix_location = shader_program.get_uniform_location(gl, "modelMatrix");
		let sun_direction_location = shader_program.get_uniform_location(gl, "sunDirection");
		let camera_position_location = shader_program.get_uniform_location(gl, "cameraPosition");
		let fog_color_location = shader_program.get_uniform_location(gl, "fogColor");
		let fog_range_location = shader_program.get_uniform_location(gl, "fogRange");
		let shadow_strength_location = shader_program.get_uniform_location(gl, "shadowStrength");
		let shadow_map_texture_location = shader_program.get_uniform_location(gl, "shadowMapTexture");
		unsafe {
			gl.uniform_matrix_4_f32_slice(view_projection_location.as_ref(), false, view_projection_elements);
			gl.uniform_matrix_4_f32_slice(model_matrix_location.as_ref(), false, model_matrix_elements);
			gl.uniform_3_f64(sun_direction_location.as_ref(), self.sun_direction[0], self.sun_direction[1], self.sun_direction[2]);
			gl.uniform_3_f64(camera_position_location.as_ref(), camera_x, camera_y, camera_z);
			gl.uniform_3_f64(fog_color_location.as_ref(), self.fog_color[0], self.fog_color[1], self.fog_color[2]);
			gl.uniform_2_f64(fog_range_location.as_ref(), self.fog_range[0], self.fog_range[1]);
		}

		// 섀도우 맵 수신. (섀도우 샘플러 타입 불일치로 드로우가 무효화되지 않도록 강도와 무관하게 항상 바인드)
		match (self.shadow_map_texture, &self.light_view_projection_elements) {
			(Some(shadow_map_texture), Some(light_view_projection_elements)) => {
				let light_view_projection_location = shader_program.get_uniform_location(gl, "lightViewProjectionMatrix");
				let shadow_texel_size_location = shader_program.get_uniform_location(gl, "shadowTexelSize");
				unsafe {
					gl.uniform_matrix_4_f32_slice(light_view_projection_location.as_ref(), false, light_view_projection_elements);
					gl.uniform_1_i32(shadow_map_texture_location.as_ref(), TEXTURE_UNIT_SHADOWMAP as i32);
					gl.uniform_1_f64(shadow_texel_size_location.as_ref(), self.shadow_texel_size);
					gl.uniform_1_f64(shadow_strength_location.as_ref(), self.shadow_strength);
					gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT_SHADOWMAP);
					gl.bind_texture(glow::TEXTURE_2D, Some(shadow_map_texture));
					gl.active_texture(glow::TEXTURE0);
				}
			}
			_ => {
				unsafe {
					gl.uniform_1_i32(shadow_map_texture_location.as_ref(), TEXTURE_UNIT_SHADOWMAP as i32);
					gl.uniform_1_f64(shadow_strength_location.as_ref(), 0.0);
					gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT_SHADOWMAP);
					gl.bind_texture(glow::TEXTURE_2D, Some(self.empty_shadow_texture));
					gl.active_texture(glow::TEXTURE0);
				}
			}
		}

		let joint_matrices_location = shader_program.get_uniform_location(gl, "jointMatrices[0]");
		let skin_list = skinned_model.get_skin_list();
		for drawable in skinned_model.get_drawable_list().iter() {
			let skin = match skin_list.get(drawable.skin_index) {
				Some(skin) => {
					skin
				}
				None => {
					continue;
				}
			};
			unsafe {
				gl.uniform_matrix_4_f32_slice(joint_matrices_location.as_ref(), false, &skin.joint_matrix_array);
			}
			self.apply_morph_uniforms(gl, shader_program, drawable);
			drawable.material.apply(gl);
			draw_drawable_triangles(gl, drawable);
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	/// 깊이 패스 출력. (섀도우 맵 캐스팅, ShadowMap.begin_render() 이후 호출)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `skinned_model`: 그림자를 드리울 모델. (바뀐 모프 타깃을 먼저 올린다)
	/// - `light_view_projection_elements`: 빛의 뷰 x 투영 행렬. (열 우선 16개, ShadowMap::get_light_view_projection_matrix)
	/// - `model_matrix_elements`: 모델 행렬. (열 우선 16개)
	///
	/// # 예제
	/// 섀도우 맵에 깊이를 그리고 화면 패스에서 그림자 받기
	/// ```rust,no_run
	/// use glow::HasContext;
	/// use vanilla::{Matrix4, ShadowMap, SkinnedModel, SkinnedModelRenderer, Vector3};
	///
	/// fn draw_with_shadow(gl: &glow::Context, skinned_model_renderer: &mut SkinnedModelRenderer, shadow_map: &mut ShadowMap, skinned_model: &mut SkinnedModel, view_projection_matrix: &Matrix4, camera_position: Vector3, drawing_buffer_width: i32, drawing_buffer_height: i32) {
	/// 	let model_matrix = Matrix4::create_identity();
	/// 	let sun_direction = Vector3::create(0.5, 0.8, 0.35).normalize();
	/// 	shadow_map.update_light_matrix(Vector3::zero(), sun_direction.multiply_scalar(-1.0), 4.0, 0.5, 30.0);
	/// 	let light_view_projection_elements = *shadow_map.get_light_view_projection_matrix().get_elements();
	/// 	shadow_map.begin_render(gl);
	/// 	skinned_model_renderer.draw_depth(gl, skinned_model, &light_view_projection_elements, model_matrix.get_elements());
	/// 	skinned_model_renderer.set_shadow_map(Some(shadow_map.get_depth_texture()), Some(&light_view_projection_elements), 1.0 / shadow_map.get_resolution() as f64);
	/// 	skinned_model_renderer.set_shadow_strength(1.0);
	/// 	skinned_model_renderer.set_sun_direction(sun_direction.x, sun_direction.y, sun_direction.z);
	/// 	unsafe {
	/// 		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
	/// 		gl.viewport(0, 0, drawing_buffer_width, drawing_buffer_height);
	/// 		gl.clear(glow::DEPTH_BUFFER_BIT);
	/// 	}
	/// 	skinned_model_renderer.draw(gl, skinned_model, view_projection_matrix.get_elements(), model_matrix.get_elements(), camera_position.x, camera_position.y, camera_position.z);
	/// }
	/// ```
	//==============================================================================
	pub fn draw_depth(&self, gl: &glow::Context, skinned_model: &mut SkinnedModel, light_view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16]) {
		skinned_model.upload_pending_morph_targets(gl);
		let depth_shader_program = &self.depth_shader_program;
		depth_shader_program.use_program(gl);
		let light_view_projection_location = depth_shader_program.get_uniform_location(gl, "lightViewProjectionMatrix");
		let model_matrix_location = depth_shader_program.get_uniform_location(gl, "modelMatrix");
		let joint_matrices_location = depth_shader_program.get_uniform_location(gl, "jointMatrices[0]");
		unsafe {
			gl.uniform_matrix_4_f32_slice(light_view_projection_location.as_ref(), false, light_view_projection_elements);
			gl.uniform_matrix_4_f32_slice(model_matrix_location.as_ref(), false, model_matrix_elements);
		}
		let skin_list = skinned_model.get_skin_list();
		for drawable in skinned_model.get_drawable_list().iter() {
			let skin = match skin_list.get(drawable.skin_index) {
				Some(skin) => {
					skin
				}
				None => {
					continue;
				}
			};
			unsafe {
				gl.uniform_matrix_4_f32_slice(joint_matrices_location.as_ref(), false, &skin.joint_matrix_array);
			}
			self.apply_morph_uniforms(gl, depth_shader_program, drawable);
			draw_drawable_triangles(gl, drawable);
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	/// 와이어프레임 출력. (모서리 선, 모프 / 스키닝 반영, 블렌드 / 깊이 상태는 호출자가 잡는다)
	/// - 드로어블 버텍스 어레이의 엘리먼트 버퍼를 잠시 선 인덱스로 바꿔 그리고 원래 인덱스로 되돌린다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `skinned_model`: 그릴 모델. (선 인덱스가 없는 드로어블은 여기서 먼저 올린다)
	/// - `view_projection_elements`: 뷰 x 투영 행렬. (열 우선 16개)
	/// - `model_matrix_elements`: 모델 행렬. (열 우선 16개)
	/// - `red`: 선 색 빨강. (0 ~ 1)
	/// - `green`: 선 색 초록. (0 ~ 1)
	/// - `blue`: 선 색 파랑. (0 ~ 1)
	/// - `alpha`: 선 불투명도. (0 ~ 1, 블렌드를 켜야 반투명이 된다)
	//==============================================================================
	pub fn draw_wireframe(&self, gl: &glow::Context, skinned_model: &mut SkinnedModel, view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16], red: f64, green: f64, blue: f64, alpha: f64) {
		skinned_model.upload_pending_morph_targets(gl);
		for drawable_index in 0..skinned_model.get_drawable_list().len() {
			if skinned_model.get_drawable_list()[drawable_index].wireframe_index_buffer.is_none() {
				if let Err(error) = skinned_model.upload_wireframe_indices(gl, drawable_index) {
					eprintln!("SkinnedModelRenderer: {}", error);
				}
			}
		}
		let wireframe_shader_program = &self.wireframe_shader_program;
		wireframe_shader_program.use_program(gl);
		let view_projection_location = wireframe_shader_program.get_uniform_location(gl, "viewProjectionMatrix");
		let model_matrix_location = wireframe_shader_program.get_uniform_location(gl, "modelMatrix");
		let wireframe_color_location = wireframe_shader_program.get_uniform_location(gl, "wireframeColor");
		let joint_matrices_location = wireframe_shader_program.get_uniform_location(gl, "jointMatrices[0]");
		unsafe {
			gl.uniform_matrix_4_f32_slice(view_projection_location.as_ref(), false, view_projection_elements);
			gl.uniform_matrix_4_f32_slice(model_matrix_location.as_ref(), false, model_matrix_elements);
			gl.uniform_4_f64(wireframe_color_location.as_ref(), red, green, blue, alpha);
		}
		let skin_list = skinned_model.get_skin_list();
		for drawable in skinned_model.get_drawable_list().iter() {
			let (skin, wireframe_index_buffer) = match (skin_list.get(drawable.skin_index), drawable.wireframe_index_buffer) {
				(Some(skin), Some(wireframe_index_buffer)) => {
					(skin, wireframe_index_buffer)
				}
				_ => {
					continue;
				}
			};
			unsafe {
				gl.uniform_matrix_4_f32_slice(joint_matrices_location.as_ref(), false, &skin.joint_matrix_array);
			}
			self.apply_morph_uniforms(gl, wireframe_shader_program, drawable);
			unsafe {
				gl.bind_vertex_array(Some(drawable.vertex_array));
				gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(wireframe_index_buffer));
				gl.draw_elements(glow::LINES, drawable.wireframe_index_count as i32, drawable.wireframe_index_component_type, 0);
				gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, drawable.index_buffer);
			}
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	/// 모프 유니폼 적용. (드로어블의 모프 텍스처/가중치, 타깃이 없으면 개수 0 으로 비활성)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shader_program`: 유니폼을 넣을 셰이더. (지금 쓰는 프로그램)
	/// - `drawable`: 모프 상태를 읽을 드로어블.
	//==============================================================================
	pub fn apply_morph_uniforms(&self, gl: &glow::Context, shader_program: &ShaderProgram, drawable: &Drawable) {
		let morph_target_count_location = shader_program.get_uniform_location(gl, "morphTargetCount");
		let target_count = match drawable.morph_texture {
			Some(_) => {
				drawable.morph_target_list.len()
			}
			None => {
				0
			}
		};
		unsafe {
			gl.uniform_1_i32(morph_target_count_location.as_ref(), target_count as i32);
		}
		if target_count == 0 {
			return;
		}
		let morph_rows_per_target_location = shader_program.get_uniform_location(gl, "morphRowsPerTarget");
		let morph_texture_width_location = shader_program.get_uniform_location(gl, "morphTextureWidth");
		let morph_weights_location = shader_program.get_uniform_location(gl, "morphWeights[0]");
		let morph_texture_location = shader_program.get_uniform_location(gl, "morphTexture");
		unsafe {
			gl.uniform_1_i32(morph_rows_per_target_location.as_ref(), drawable.morph_rows_per_target);
			gl.uniform_1_i32(morph_texture_width_location.as_ref(), drawable.morph_texture_width);
			gl.uniform_1_f32_slice(morph_weights_location.as_ref(), &drawable.morph_weights);
			gl.uniform_1_i32(morph_texture_location.as_ref(), MORPH_TEXTURE_UNIT as i32);
			gl.active_texture(glow::TEXTURE0 + MORPH_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, drawable.morph_texture);
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	/// 섀도우 맵 설정. (본 패스에서 그림자 수신, 행렬은 복사해 둔다)
	///
	/// # 인자
	/// - `shadow_map_texture`: 섀도우 맵 깊이 텍스처. (None 이면 그림자를 받지 않는다)
	/// - `light_view_projection_elements`: 빛의 뷰 x 투영 행렬. (16개를 복사해 둔다, None 이면 그림자를 받지 않는다)
	/// - `texel_size`: 섀도우 맵 텍셀 크기. (1 / 해상도, PCF 표본 간격, 기본 1 / 2048)
	//==============================================================================
	pub fn set_shadow_map(&mut self, shadow_map_texture: Option<GlTexture>, light_view_projection_elements: Option<&[f32; 16]>, texel_size: f64) {
		self.shadow_map_texture = shadow_map_texture;
		self.light_view_projection_elements = light_view_projection_elements.copied();
		self.shadow_texel_size = texel_size;
	}

	//==============================================================================
	/// 그림자 강도 설정. (0 = 수신 안 함)
	///
	/// # 인자
	/// - `shadow_strength`: 그림자 강도. (0 ~ 1, 0 이면 받지 않는다, 기본 0)
	//==============================================================================
	pub fn set_shadow_strength(&mut self, shadow_strength: f64) {
		self.shadow_strength = shadow_strength;
	}

	//==============================================================================
	/// 태양 방향 설정.
	///
	/// # 인자
	/// - `x`: 태양 쪽을 가리키는 방향 X. (셰이더가 정규화한다, 기본 (0, 1, 0))
	/// - `y`: 방향 Y.
	/// - `z`: 방향 Z.
	//==============================================================================
	pub fn set_sun_direction(&mut self, x: f64, y: f64, z: f64) {
		self.sun_direction = [x, y, z];
	}

	//==============================================================================
	/// 안개 색 설정.
	///
	/// # 인자
	/// - `red`: 빨강. (0 ~ 1)
	/// - `green`: 초록. (0 ~ 1)
	/// - `blue`: 파랑. (0 ~ 1)
	//==============================================================================
	pub fn set_fog_color(&mut self, red: f64, green: f64, blue: f64) {
		self.fog_color = [red, green, blue];
	}

	//==============================================================================
	/// 안개 거리 범위 설정.
	///
	/// # 인자
	/// - `start_distance`: 안개가 끼기 시작하는 카메라 거리. (월드 단위, 기본 55)
	/// - `end_distance`: 안개가 다 덮는 카메라 거리. (월드 단위, 기본 250, 사이는 smoothstep)
	//==============================================================================
	pub fn set_fog_range(&mut self, start_distance: f64, end_distance: f64) {
		self.fog_range = [start_distance, end_distance];
	}

	//==============================================================================
	/// 빈 그림자 텍스처 반환. (그림자를 받지 않을 때 섀도우 유닛에 묶는 1x1 비교 깊이 텍스처, 파생 렌더러가 같이 쓴다)
	///
	/// # 돌려줌
	/// 1x1 비교 깊이 텍스처. (렌더러가 들고 있으니 지우지 않는다)
	//==============================================================================
	pub fn get_empty_shadow_texture(&self) -> GlTexture {
		self.empty_shadow_texture
	}

	//==============================================================================
	/// 셰이더 프로그램 반환. (스키닝 PBR 템플릿, 모델과 머티리얼이 나눠 든다)
	///
	/// # 돌려줌
	/// 셰이더 프로그램. (Rc 를 복제해 준다)
	//==============================================================================
	pub fn get_shader_program(&self) -> Rc<ShaderProgram> {
		self.shader_program.clone()
	}

	//==============================================================================
	/// 깊이 셰이더 프로그램 반환.
	///
	/// # 돌려줌
	/// 깊이 패스 셰이더 프로그램. (draw_depth 가 쓴다)
	//==============================================================================
	pub fn get_depth_shader_program(&self) -> &ShaderProgram {
		&self.depth_shader_program
	}

	//==============================================================================
	/// 와이어프레임 셰이더 프로그램 반환.
	///
	/// # 돌려줌
	/// 와이어프레임 셰이더 프로그램. (draw_wireframe 이 쓴다)
	//==============================================================================
	pub fn get_wireframe_shader_program(&self) -> &ShaderProgram {
		&self.wireframe_shader_program
	}
}

//==============================================================================
/// 드로어블 삼각형 출력. (인덱스가 있으면 drawElements, 없으면 drawArrays)
//==============================================================================
fn draw_drawable_triangles(gl: &glow::Context, drawable: &Drawable) {
	unsafe {
		gl.bind_vertex_array(Some(drawable.vertex_array));
		if drawable.is_indexed {
			gl.draw_elements(glow::TRIANGLES, drawable.index_count as i32, drawable.index_component_type, drawable.index_byte_offset);
		}
		else {
			gl.draw_arrays(glow::TRIANGLES, 0, drawable.vertex_count as i32);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graphics3d::skinnedmodel::MORPH_TARGET_MAXIMUM;

	//==============================================================================
	/// 셰이더 소스의 배열 크기가 상수와 같은지. (원본 템플릿 ${MORPH_TARGET_MAXIMUM} 자리)
	//==============================================================================
	#[test]
	fn morph_glsl_matches_maximum() {
		let expected_declaration = format!("uniform float morphWeights[{}];", MORPH_TARGET_MAXIMUM);
		assert!(MORPH_GLSL.contains(&expected_declaration));
		for source in [SKINNED_VERTEXSHADER_SOURCE, SKINNED_DEPTH_VERTEXSHADER_SOURCE, SKINNED_WIREFRAME_VERTEXSHADER_SOURCE] {
			assert!(source.starts_with("#version 300 es"));
			assert!(source.contains(MORPH_GLSL));
			assert!(source.contains("uniform mat4 jointMatrices[96];"));
		}
	}
}
