//==============================================================================
// 스킨드 모델 렌더러. (원본 experimental/graphics/skinnedmodelrenderer.js)
// - 스키닝 PBR 셰이더 템플릿 + 환경 상태(섀도우 맵 / 태양 / 안개) 소유, 씬당 하나. SkinnedModel 은 데이터/포즈만 갖고,
//   렌더링은 이 객체가 모델 인스턴스를 받아 한다.
// - 프래그먼트 셰이더 소스를 주입하면 같은 스키닝/모프 버텍스 경로 위에 다른 셰이딩 템플릿을 세울 수 있다.
//   (파생 렌더러는 셰이더를 쓴 뒤 고유 유니폼을 설정하고 draw() 를 부른다)
// - GLSL 은 원본과 같다. (쿡-토런스 GGX, 화면공간 미분 코탄젠트 노멀맵, 3x3 PCF 섀도우, 반구 앰비언트, 거리 안개)
//   원본 템플릿 문자열의 ${MORPH_GLSL} / ${MORPH_TARGET_MAXIMUM} 은 concat! 과 숫자 96 으로 옮겼다. (시험이 둘이 같은지 본다)
// - 텍스처 유닛: 0 = baseColor, 1 = 섀도우 맵, 2 ~ 8 = 머티리얼 슬롯, 15 = 모프 타깃 텍스처.
// - 원본과 다른 것:
//   - 그리기 함수는 gl 과 &mut SkinnedModel 을 받는다. 바뀐 모프 타깃 업로드(원본 update 6단계)와 와이어프레임 인덱스 업로드를
//     여기서 먼저 한다. 행렬 인자는 원본 getElements() 처럼 &[f32; 16] 이다. (matrix.getElements())
//   - setShadowMap 은 원본처럼 배열을 참조로 들지 않고 16개를 복사해 둔다.
//   - 생성은 셰이더 컴파일 실패를 오류(Result)로 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::material::TEXTURE_UNIT_SHADOWMAP;
use crate::graphics3d::skinnedmodel::{Drawable, SkinnedModel};
use glow::HasContext;
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 모프 타깃 텍스처 유닛. (머티리얼 슬롯 0 / 2~8, 섀도우 맵 1, 추가 슬롯 9~ 와 겹치지 않는 값)
pub const MORPH_TEXTURE_UNIT: u32 = 15;

// 모프 타깃 GLSL 본문. (버텍스 셰이더 삽입, 실수 텍스처의 위치/노멀 델타를 가중 합산, 배열 크기는 MORPH_TARGET_MAXIMUM)
// - 텍셀 인덱스 = gl_VertexID, 타깃 블록 = [위치 행들][노멀 행들] 순서.
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

// 모프 타깃 GLSL. (원본 MORPH_GLSL, 파생 렌더러가 대체 버텍스 셰이더에 넣어 쓴다)
pub const MORPH_GLSL: &str = morphGlslSource!();

// 스키닝 버텍스 셰이더. (모프 타깃 → 조인트 4개 가중 혼합, 어트리뷰트 위치 고정)
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

// 스키닝 프래그먼트 셰이더. (metallic-roughness PBR + 태양/반구 라이팅 + 섀도우 맵 + 거리 안개)
// - 노멀 매핑은 화면 공간 미분 기반 코탄젠트 프레임 사용. (탄젠트 어트리뷰트 불필요)
// - FBX 스펙큘러/글로스 경로: useGlossiness=1 이면 러프니스 = 1 - 글로스, 스펙큘러 맵은 F0 변조.
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

// 깊이 전용 스키닝 버텍스 셰이더. (섀도우 맵 캐스팅, 모프 타깃 포함)
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

// 깊이 전용 프래그먼트 셰이더.
const SKINNED_DEPTH_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
void main() {
}";

// 와이어프레임 버텍스 셰이더. (모프 + 스키닝을 그대로 따르는 선 출력)
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

// 와이어프레임 프래그먼트 셰이더. (단색)
const SKINNED_WIREFRAME_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
uniform vec4 wireframeColor;
out vec4 outputColor;
void main() {
	outputColor = wireframeColor;
}";

//==============================================================================
// 스킨드 모델 렌더러.
//==============================================================================
pub struct SkinnedModelRenderer {
	shaderProgram: Rc<ShaderProgram>,
	depthShaderProgram: ShaderProgram,
	wireframeShaderProgram: ShaderProgram,
	shadowMapTexture: Option<GlTexture>,
	lightViewProjectionElements: Option<[f32; 16]>,
	shadowStrength: f32,
	shadowTexelSize: f32,
	sunDirection: [f32; 3],
	fogColor: [f32; 3],
	fogRange: [f32; 2],
}

impl SkinnedModelRenderer {
	//==============================================================================
	// 생성. (대체 프래그먼트 / 버텍스 셰이더 소스, None 이면 기본 PBR / 기본 스키닝)
	// - 대체 버텍스 셰이더는 같은 어트리뷰트 위치 / 출력 규약을 지켜야 한다.
	//==============================================================================
	pub fn new(gl: &glow::Context, fragmentShaderSource: Option<&str>, vertexShaderSource: Option<&str>) -> Result<SkinnedModelRenderer, String> {
		let resolvedFragmentShaderSource = fragmentShaderSource.unwrap_or(SKINNED_FRAGMENTSHADER_SOURCE);
		let resolvedVertexShaderSource = vertexShaderSource.unwrap_or(SKINNED_VERTEXSHADER_SOURCE);
		let shaderProgram = ShaderProgram::new(gl, resolvedVertexShaderSource.trim(), resolvedFragmentShaderSource.trim())?;
		let depthShaderProgram = ShaderProgram::new(gl, SKINNED_DEPTH_VERTEXSHADER_SOURCE.trim(), SKINNED_DEPTH_FRAGMENTSHADER_SOURCE.trim())?;
		let wireframeShaderProgram = ShaderProgram::new(gl, SKINNED_WIREFRAME_VERTEXSHADER_SOURCE.trim(), SKINNED_WIREFRAME_FRAGMENTSHADER_SOURCE.trim())?;
		Ok(SkinnedModelRenderer {
			shaderProgram: Rc::new(shaderProgram),
			depthShaderProgram,
			wireframeShaderProgram,
			shadowMapTexture: None,
			lightViewProjectionElements: None,
			shadowStrength: 0.0,
			shadowTexelSize: 1.0 / 2048.0,
			sunDirection: [0.0, 1.0, 0.0],
			fogColor: [0.8, 0.85, 0.92],
			fogRange: [55.0, 250.0],
		})
	}

	//==============================================================================
	// 출력. (모델 인스턴스의 드로어블 순회, 머티리얼 적용 + 조인트 행렬 업로드)
	// - 깊이 테스트 / 컬링 / 블렌드 / 프레임버퍼는 호출자가 잡는다. (원본과 같다)
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context, skinnedModel: &mut SkinnedModel, viewProjectionElements: &[f32; 16], modelMatrixElements: &[f32; 16], cameraX: f32, cameraY: f32, cameraZ: f32) {
		skinnedModel.uploadPendingMorphTargets(gl);
		let shaderProgram = &*self.shaderProgram;
		shaderProgram.useProgram(gl);
		let viewProjectionLocation = shaderProgram.getUniformLocation(gl, "viewProjectionMatrix");
		let modelMatrixLocation = shaderProgram.getUniformLocation(gl, "modelMatrix");
		let sunDirectionLocation = shaderProgram.getUniformLocation(gl, "sunDirection");
		let cameraPositionLocation = shaderProgram.getUniformLocation(gl, "cameraPosition");
		let fogColorLocation = shaderProgram.getUniformLocation(gl, "fogColor");
		let fogRangeLocation = shaderProgram.getUniformLocation(gl, "fogRange");
		let shadowStrengthLocation = shaderProgram.getUniformLocation(gl, "shadowStrength");
		let shadowMapTextureLocation = shaderProgram.getUniformLocation(gl, "shadowMapTexture");
		unsafe {
			gl.uniform_matrix_4_f32_slice(viewProjectionLocation.as_ref(), false, viewProjectionElements);
			gl.uniform_matrix_4_f32_slice(modelMatrixLocation.as_ref(), false, modelMatrixElements);
			gl.uniform_3_f32(sunDirectionLocation.as_ref(), self.sunDirection[0], self.sunDirection[1], self.sunDirection[2]);
			gl.uniform_3_f32(cameraPositionLocation.as_ref(), cameraX, cameraY, cameraZ);
			gl.uniform_3_f32(fogColorLocation.as_ref(), self.fogColor[0], self.fogColor[1], self.fogColor[2]);
			gl.uniform_2_f32(fogRangeLocation.as_ref(), self.fogRange[0], self.fogRange[1]);
		}

		// 섀도우 맵 수신. (섀도우 샘플러 타입 불일치로 드로우가 무효화되지 않도록 강도와 무관하게 항상 바인드)
		match (self.shadowMapTexture, &self.lightViewProjectionElements) {
			(Some(shadowMapTexture), Some(lightViewProjectionElements)) => {
				let lightViewProjectionLocation = shaderProgram.getUniformLocation(gl, "lightViewProjectionMatrix");
				let shadowTexelSizeLocation = shaderProgram.getUniformLocation(gl, "shadowTexelSize");
				unsafe {
					gl.uniform_matrix_4_f32_slice(lightViewProjectionLocation.as_ref(), false, lightViewProjectionElements);
					gl.uniform_1_i32(shadowMapTextureLocation.as_ref(), TEXTURE_UNIT_SHADOWMAP as i32);
					gl.uniform_1_f32(shadowTexelSizeLocation.as_ref(), self.shadowTexelSize);
					gl.uniform_1_f32(shadowStrengthLocation.as_ref(), self.shadowStrength);
					gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT_SHADOWMAP);
					gl.bind_texture(glow::TEXTURE_2D, Some(shadowMapTexture));
					gl.active_texture(glow::TEXTURE0);
				}
			}
			_ => {
				unsafe {
					gl.uniform_1_i32(shadowMapTextureLocation.as_ref(), TEXTURE_UNIT_SHADOWMAP as i32);
					gl.uniform_1_f32(shadowStrengthLocation.as_ref(), 0.0);
				}
			}
		}

		let jointMatricesLocation = shaderProgram.getUniformLocation(gl, "jointMatrices[0]");
		let skinList = skinnedModel.getSkinList();
		for drawable in skinnedModel.getDrawableList().iter() {
			let skin = match skinList.get(drawable.skinIndex) {
				Some(skin) => {
					skin
				}
				None => {
					continue;
				}
			};
			unsafe {
				gl.uniform_matrix_4_f32_slice(jointMatricesLocation.as_ref(), false, &skin.jointMatrixArray);
			}
			self.applyMorphUniforms(gl, shaderProgram, drawable);
			drawable.material.apply(gl);
			drawDrawableTriangles(gl, drawable);
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	// 깊이 패스 출력. (섀도우 맵 캐스팅, ShadowMap.beginRender() 이후 호출)
	//==============================================================================
	pub fn drawDepth(&self, gl: &glow::Context, skinnedModel: &mut SkinnedModel, lightViewProjectionElements: &[f32; 16], modelMatrixElements: &[f32; 16]) {
		skinnedModel.uploadPendingMorphTargets(gl);
		let depthShaderProgram = &self.depthShaderProgram;
		depthShaderProgram.useProgram(gl);
		let lightViewProjectionLocation = depthShaderProgram.getUniformLocation(gl, "lightViewProjectionMatrix");
		let modelMatrixLocation = depthShaderProgram.getUniformLocation(gl, "modelMatrix");
		let jointMatricesLocation = depthShaderProgram.getUniformLocation(gl, "jointMatrices[0]");
		unsafe {
			gl.uniform_matrix_4_f32_slice(lightViewProjectionLocation.as_ref(), false, lightViewProjectionElements);
			gl.uniform_matrix_4_f32_slice(modelMatrixLocation.as_ref(), false, modelMatrixElements);
		}
		let skinList = skinnedModel.getSkinList();
		for drawable in skinnedModel.getDrawableList().iter() {
			let skin = match skinList.get(drawable.skinIndex) {
				Some(skin) => {
					skin
				}
				None => {
					continue;
				}
			};
			unsafe {
				gl.uniform_matrix_4_f32_slice(jointMatricesLocation.as_ref(), false, &skin.jointMatrixArray);
			}
			self.applyMorphUniforms(gl, depthShaderProgram, drawable);
			drawDrawableTriangles(gl, drawable);
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	// 와이어프레임 출력. (모서리 선, 모프 / 스키닝 반영, 블렌드 / 깊이 상태는 호출자가 잡는다)
	// - 드로어블 버텍스 어레이의 엘리먼트 버퍼를 잠시 선 인덱스로 바꿔 그리고 원래 인덱스로 되돌린다.
	//==============================================================================
	pub fn drawWireframe(&self, gl: &glow::Context, skinnedModel: &mut SkinnedModel, viewProjectionElements: &[f32; 16], modelMatrixElements: &[f32; 16], red: f32, green: f32, blue: f32, alpha: f32) {
		skinnedModel.uploadPendingMorphTargets(gl);
		for drawableIndex in 0..skinnedModel.getDrawableList().len() {
			if skinnedModel.getDrawableList()[drawableIndex].wireframeIndexBuffer.is_none() {
				if let Err(error) = skinnedModel.uploadWireframeIndices(gl, drawableIndex) {
					eprintln!("SkinnedModelRenderer: {}", error);
				}
			}
		}
		let wireframeShaderProgram = &self.wireframeShaderProgram;
		wireframeShaderProgram.useProgram(gl);
		let viewProjectionLocation = wireframeShaderProgram.getUniformLocation(gl, "viewProjectionMatrix");
		let modelMatrixLocation = wireframeShaderProgram.getUniformLocation(gl, "modelMatrix");
		let wireframeColorLocation = wireframeShaderProgram.getUniformLocation(gl, "wireframeColor");
		let jointMatricesLocation = wireframeShaderProgram.getUniformLocation(gl, "jointMatrices[0]");
		unsafe {
			gl.uniform_matrix_4_f32_slice(viewProjectionLocation.as_ref(), false, viewProjectionElements);
			gl.uniform_matrix_4_f32_slice(modelMatrixLocation.as_ref(), false, modelMatrixElements);
			gl.uniform_4_f32(wireframeColorLocation.as_ref(), red, green, blue, alpha);
		}
		let skinList = skinnedModel.getSkinList();
		for drawable in skinnedModel.getDrawableList().iter() {
			let (skin, wireframeIndexBuffer) = match (skinList.get(drawable.skinIndex), drawable.wireframeIndexBuffer) {
				(Some(skin), Some(wireframeIndexBuffer)) => {
					(skin, wireframeIndexBuffer)
				}
				_ => {
					continue;
				}
			};
			unsafe {
				gl.uniform_matrix_4_f32_slice(jointMatricesLocation.as_ref(), false, &skin.jointMatrixArray);
			}
			self.applyMorphUniforms(gl, wireframeShaderProgram, drawable);
			unsafe {
				gl.bind_vertex_array(Some(drawable.vertexArray));
				gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(wireframeIndexBuffer));
				gl.draw_elements(glow::LINES, drawable.wireframeIndexCount as i32, drawable.wireframeIndexComponentType, 0);
				gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, drawable.indexBuffer);
			}
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	// 모프 유니폼 적용. (드로어블의 모프 텍스처/가중치, 타깃이 없으면 개수 0 으로 비활성)
	//==============================================================================
	pub fn applyMorphUniforms(&self, gl: &glow::Context, shaderProgram: &ShaderProgram, drawable: &Drawable) {
		let morphTargetCountLocation = shaderProgram.getUniformLocation(gl, "morphTargetCount");
		let targetCount = match drawable.morphTexture {
			Some(_) => {
				drawable.morphTargetList.len()
			}
			None => {
				0
			}
		};
		unsafe {
			gl.uniform_1_i32(morphTargetCountLocation.as_ref(), targetCount as i32);
		}
		if targetCount == 0 {
			return;
		}
		let morphRowsPerTargetLocation = shaderProgram.getUniformLocation(gl, "morphRowsPerTarget");
		let morphTextureWidthLocation = shaderProgram.getUniformLocation(gl, "morphTextureWidth");
		let morphWeightsLocation = shaderProgram.getUniformLocation(gl, "morphWeights[0]");
		let morphTextureLocation = shaderProgram.getUniformLocation(gl, "morphTexture");
		unsafe {
			gl.uniform_1_i32(morphRowsPerTargetLocation.as_ref(), drawable.morphRowsPerTarget);
			gl.uniform_1_i32(morphTextureWidthLocation.as_ref(), drawable.morphTextureWidth);
			gl.uniform_1_f32_slice(morphWeightsLocation.as_ref(), &drawable.morphWeights);
			gl.uniform_1_i32(morphTextureLocation.as_ref(), MORPH_TEXTURE_UNIT as i32);
			gl.active_texture(glow::TEXTURE0 + MORPH_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, drawable.morphTexture);
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	// 섀도우 맵 설정. (본 패스에서 그림자 수신, 행렬은 복사해 둔다)
	//==============================================================================
	pub fn setShadowMap(&mut self, shadowMapTexture: Option<GlTexture>, lightViewProjectionElements: Option<&[f32; 16]>, texelSize: f32) {
		self.shadowMapTexture = shadowMapTexture;
		self.lightViewProjectionElements = lightViewProjectionElements.copied();
		self.shadowTexelSize = texelSize;
	}

	//==============================================================================
	// 그림자 강도 설정. (0 = 수신 안 함)
	//==============================================================================
	pub fn setShadowStrength(&mut self, shadowStrength: f32) {
		self.shadowStrength = shadowStrength;
	}

	//==============================================================================
	// 태양 방향 설정.
	//==============================================================================
	pub fn setSunDirection(&mut self, x: f32, y: f32, z: f32) {
		self.sunDirection = [x, y, z];
	}

	//==============================================================================
	// 안개 색 설정.
	//==============================================================================
	pub fn setFogColor(&mut self, red: f32, green: f32, blue: f32) {
		self.fogColor = [red, green, blue];
	}

	//==============================================================================
	// 안개 거리 범위 설정.
	//==============================================================================
	pub fn setFogRange(&mut self, startDistance: f32, endDistance: f32) {
		self.fogRange = [startDistance, endDistance];
	}

	//==============================================================================
	// 셰이더 프로그램 반환. (스키닝 PBR 템플릿, 모델과 머티리얼이 나눠 든다)
	//==============================================================================
	pub fn getShaderProgram(&self) -> Rc<ShaderProgram> {
		self.shaderProgram.clone()
	}

	//==============================================================================
	// 깊이 셰이더 프로그램 반환.
	//==============================================================================
	pub fn getDepthShaderProgram(&self) -> &ShaderProgram {
		&self.depthShaderProgram
	}

	//==============================================================================
	// 와이어프레임 셰이더 프로그램 반환.
	//==============================================================================
	pub fn getWireframeShaderProgram(&self) -> &ShaderProgram {
		&self.wireframeShaderProgram
	}
}

//==============================================================================
// 드로어블 삼각형 출력. (인덱스가 있으면 drawElements, 없으면 drawArrays)
//==============================================================================
fn drawDrawableTriangles(gl: &glow::Context, drawable: &Drawable) {
	unsafe {
		gl.bind_vertex_array(Some(drawable.vertexArray));
		if drawable.isIndexed {
			gl.draw_elements(glow::TRIANGLES, drawable.indexCount as i32, drawable.indexComponentType, drawable.indexByteOffset);
		}
		else {
			gl.draw_arrays(glow::TRIANGLES, 0, drawable.vertexCount as i32);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graphics3d::skinnedmodel::MORPH_TARGET_MAXIMUM;

	//==============================================================================
	// 셰이더 소스의 배열 크기가 상수와 같은지. (원본 템플릿 ${MORPH_TARGET_MAXIMUM} 자리)
	//==============================================================================
	#[test]
	fn morphGlslMatchesMaximum() {
		let expectedDeclaration = format!("uniform float morphWeights[{}];", MORPH_TARGET_MAXIMUM);
		assert!(MORPH_GLSL.contains(&expectedDeclaration));
		for source in [SKINNED_VERTEXSHADER_SOURCE, SKINNED_DEPTH_VERTEXSHADER_SOURCE, SKINNED_WIREFRAME_VERTEXSHADER_SOURCE] {
			assert!(source.starts_with("#version 300 es"));
			assert!(source.contains(MORPH_GLSL));
			assert!(source.contains("uniform mat4 jointMatrices[96];"));
		}
	}
}
