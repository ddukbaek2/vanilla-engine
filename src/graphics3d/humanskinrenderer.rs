//==============================================================================
//! 인체 피부 렌더러. (원본 graphics3d/humanskinrenderer.js)
//! - SkinnedModelRenderer 의 스키닝/모프 버텍스 경로 위에 피부 / 눈 / 머리카락 셰이딩 템플릿을 세운다.
//! - 선형 HDR 로 조도 / 알베도 / 스펙큘러 / 노멀을 MRT 로 나눠 출력하고, 합성은 SubsurfaceScatteringEffect 이후 호출자가 한다.
//! - 머티리얼마다 셰이딩 모드를 등록한다(set_material_shading). 불투명 패스(draw)는 피부 / 눈 / 머리카락 컷아웃,
//!   블렌드 패스(draw_blended)는 머리카락 프린지 / 차폐 / 유체를 그린다. (호출자가 블렌드 상태를 잡는다)
//! - 환경 상태: 조명 3개(0번 = 키 라이트, 섀도우 수신), 3색 반구 앰비언트, 노멀, 디테일, 러프니스, 스펙큘러, 주름, 마이크로, 눈, 머리카락 파라미터.
//! - GLSL 은 원본과 같다. 원본 템플릿 문자열의 ${WRINKLE_CHANNEL_COUNT} / ${SHADOW_SAMPLE_COUNT} / ${SKIN_SHADING_*} 는 숫자로,
//!   ${MORPH_GLSL} 은 생성할 때 MORPH_GLSL 을 이어 붙여 옮겼다. (시험이 숫자가 상수와 같은지 본다)
//! - 텍스처 유닛: 0 = base_color, 1 = 섀도우 맵, 2 ~ 8 = 머티리얼 슬롯, 9 ~ 14 = 피부 추가 슬롯, 15 = 모프 타깃 텍스처.
//! - 원본과 다른 것:
//!   - 원본은 SkinnedModelRenderer 를 상속한다. 러스트 판은 SkinnedModelRenderer 를 품고 Deref / DerefMut 로 기반 함수
//!     (draw_wireframe, apply_morph_uniforms, set_sun_direction, get_shader_program ...)를 그대로 부르게 한다.
//!     원본이 다시 정의한 draw / draw_depth / set_shadow_map / set_shadow_strength 는 이 타입의 함수가 먼저 불린다.
//!     SkinnedModel::load_from_bytes(gl, bytes, &human_skin_renderer) 처럼 기반 렌더러 자리에 그대로 넘길 수 있다. (Deref 강제 변환)
//!   - 그리기 함수는 기반 렌더러처럼 gl 과 &mut SkinnedModel 을 받고, 바뀐 모프 타깃 업로드를 먼저 한다.
//!   - 머티리얼 셰이딩 모드 표는 원본 `Map<Material, number>` 처럼 머티리얼 인스턴스(Rc 포인터)를 열쇠로 든다.
//!   - set_ambient_colors / set_hair_parameters 는 원본 배열 대신 고정 길이 배열을 받는다. set_shadow_sample_count 는 정수를 받아
//!     원본의 Math.floor 없이 1 ~ 24 로 자른다.
//!   - set_shadow_map 은 기반 렌더러처럼 행렬 16개를 복사해 둔다. 생성은 셰이더 컴파일 실패를 오류(Result)로 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::material::{Material, TEXTURE_UNIT_BASECOLOR, TEXTURE_UNIT_SHADOWMAP};
use crate::graphics3d::skinnedmodel::{Drawable, SkinnedModel};
use crate::graphics3d::skinnedmodelrenderer::{SkinnedModelRenderer, MORPH_GLSL};
use glow::HasContext;
use crate::core::gluniform::GlUniformF64;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 셰이딩 모드. (머티리얼마다 set_material_shading 으로 지정, 같은 셰이더 안에서 분기)
pub const SKIN_SHADING_SKIN: i32 = 0;
/// 눈. (각막 굴절 시차로 홍채를 읽고, 공막, 동공, 각막 하이라이트)
pub const SKIN_SHADING_EYE: i32 = 1;
/// 머리카락. (카드 탄젠트 이방성 하이라이트, 컷아웃과 프린지 블렌드 두 패스, 흔들림 적용)
pub const SKIN_SHADING_HAIR: i32 = 2;
/// 눈꺼풀 차폐 메시. (블렌드 패스 전용, 조도만 어둡게 한다)
pub const SKIN_SHADING_OCCLUSION: i32 = 3;
/// 눈물선, 침 같은 유체. (블렌드 패스 전용, 스펙큘러만 얹는다)
pub const SKIN_SHADING_FLUID: i32 = 4;
/// 눈썹, 수염 같은 얼굴 털. (머리카락과 같은 셰이딩, 흔들림 없음)
pub const SKIN_SHADING_FACIALHAIR: i32 = 5;

/// 추가 텍스처 유닛. (머티리얼 기본 슬롯 0 / 2~8 과 섀도우 1, 모프 15 를 피한 9~14)
/// - 피부: 9 높이 디테일, 10 마이크로(노멀 rgb + 캐비티 a), 11 주름 마스크 아틀라스(4x4), 12 주름 노멀 아틀라스(2x2), 13 주름 색 아틀라스(2x2)
/// - 눈: 12 홍채 노멀, 13 홍채 색 / 머리카락: 12 카드 탄젠트(rgb) + 가닥 좌표(a)
pub const SKIN_TEXTURE_UNIT_DETAILHEIGHT: u32 = 9;
/// 마이크로 노멀(rgb) + 캐비티(a) 유닛. (유니폼 microTexture, set_micro_detail 의 횟수만큼 반복)
pub const SKIN_TEXTURE_UNIT_MICRO: u32 = 10;
/// 주름 마스크 아틀라스 유닛. (유니폼 maskAtlasTexture, 4x4 타일 x RGBA = 64 채널)
pub const SKIN_TEXTURE_UNIT_MASKATLAS: u32 = 11;
/// 겹 노멀 유닛. (유니폼 layerNormalTexture, 피부는 주름 노멀 아틀라스 2x2, 눈은 홍채 노멀, 머리카락은 카드 탄젠트 rgb + 가닥 좌표 a)
pub const SKIN_TEXTURE_UNIT_LAYERNORMAL: u32 = 12;
/// 겹 색 유닛. (유니폼 layerColorTexture, 피부는 주름 색 아틀라스 2x2, 눈은 홍채 색)
pub const SKIN_TEXTURE_UNIT_LAYERCOLOR: u32 = 13;
/// 보조 유닛. (유니폼 auxiliaryTexture, 셰이더에 선언만 있고 읽는 곳은 없다)
pub const SKIN_TEXTURE_UNIT_AUXILIARY: u32 = 14;

/// 주름 마스크 채널 수. (4x4 아틀라스 x RGBA, 채널마다 주름 맵 1 / 2 / 3 기여 가중치 vec3)
pub const WRINKLE_CHANNEL_COUNT: usize = 64;

/// 섀도우 포아송 표본 최대치. (회전 노이즈로 흩어 부드러운 반영, 실제 표본 수는 shadowSampleCount 유니폼)
const SHADOW_SAMPLE_COUNT: i32 = 24;

/// 인체 피부 프래그먼트 셰이더. (선형 HDR, 화면 공간 SSS 를 위해 조도 / 알베도 / 스펙큘러를 분리 출력)
/// - 조명: 방향광 3개(0번은 섀도우 맵 수신) + 3색 반구 앰비언트 + 조명 방향의 소프트박스 반사 환경.
/// - 피부: 노멀 맵 + 표정 주름 맵(마스크 아틀라스 가중 혼합) + 타일링 마이크로 노멀 / 캐비티 + 높이 미분 디테일, 이중 로브 GGX, 잔털.
/// - 눈: 각막 굴절 시차로 홍채를 샘플링, 홍채 노멀 / 림버스 / 동공, 각막 유리 하이라이트 + 환경 반사, 공막 정맥 노멀.
/// - 머리카락: 카드 탄젠트 아틀라스로 Kajiya-Kay 이중 로브 이방성 하이라이트, 뿌리 차폐, 투과 역광, 컷아웃 + 프린지 블렌드 2패스.
/// - 차폐 / 유체: 블렌드 패스 전용, 눈꺼풀 차폐 메시는 조도만 어둡게, 눈물선 / 침은 스펙큘러만 얹는다.
/// - 얼굴 털(눈썹 / 수염): 머리카락과 같은 셰이딩이지만 흔들림은 적용하지 않는다.
/// - 출력 0: 확산 조도(rgb) + SSS 마스크(a) / 1: 선형 알베도(rgb) + 커버리지(a) / 2: 스펙큘러(rgb) / 3: 월드 노멀(rgb).
pub const SKIN_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
precision highp int;
in vec3 worldPosition;
in vec3 worldNormal;
in vec2 fragmentTextureCoordinate;
in vec4 lightSpacePosition;
uniform sampler2D baseColorTexture;
uniform sampler2D normalTexture;
uniform sampler2D specularTexture;
uniform sampler2D metallicRoughnessTexture;
uniform sampler2D occlusionTexture;
uniform sampler2D opacityTexture;
uniform sampler2D detailHeightTexture;
uniform sampler2D microTexture;
uniform sampler2D maskAtlasTexture;
uniform sampler2D layerNormalTexture;
uniform sampler2D layerColorTexture;
uniform sampler2D auxiliaryTexture;
uniform vec3 baseColorFactor;
uniform float roughnessFactor;
uniform int useAlphaCutout;
uniform float subsurfaceFactor;
uniform int shadingMode;
uniform int passMode;
uniform highp sampler2DShadow shadowMapTexture;
uniform float shadowStrength;
uniform float shadowTexelSize;
uniform float shadowSoftness;
uniform int shadowSampleCount;
uniform vec3 cameraPosition;
uniform vec3 lightDirections[3];
uniform vec3 lightColors[3];
uniform vec3 ambientSkyColor;
uniform vec3 ambientHorizonColor;
uniform vec3 ambientGroundColor;
uniform float normalStrength;
uniform float detailStrength;
uniform float detailTexelSize;
uniform float cavityStrength;
uniform vec2 roughnessRange;
uniform float specularStrength;
uniform float sheenStrength;
uniform float subsurfaceAmount;
uniform float microTiling;
uniform float microStrength;
uniform int useWrinkleMaps;
uniform int wrinkleTileMask;
uniform vec3 wrinkleChannelWeights[64];
uniform float wrinkleNormalStrength;
uniform float wrinkleColorStrength;
uniform float eyeIrisRadius;
uniform float eyePupilRadius;
uniform float eyeIrisDepth;
uniform float eyeClosure;
uniform vec3 hairSpecularShift;
uniform vec3 hairSpecularPower;
uniform vec3 hairSpecularIntensity;
uniform float hairRootShade;
layout(location = 0) out vec4 irradianceOutput;
layout(location = 1) out vec4 albedoOutput;
layout(location = 2) out vec4 specularOutput;
layout(location = 3) out vec4 normalOutput;

const float PI = 3.14159265;

/// 포아송 원판. (24 표본, 프래그먼트마다 회전, 앞쪽 표본부터 쓴다)
const vec2 POISSON_DISK[24] = vec2[](
	vec2(-0.326, -0.406), vec2(-0.840, -0.074), vec2(-0.696, 0.457), vec2(-0.203, 0.621),
	vec2(0.962, -0.195), vec2(0.473, -0.480), vec2(0.519, 0.767), vec2(0.185, -0.893),
	vec2(0.507, 0.064), vec2(0.896, 0.412), vec2(-0.322, -0.933), vec2(-0.792, -0.598),
	vec2(0.130, 0.240), vec2(-0.520, 0.120), vec2(0.310, -0.150), vec2(-0.110, -0.680),
	vec2(0.700, 0.240), vec2(-0.450, 0.780), vec2(0.020, 0.930), vec2(-0.950, -0.300),
	vec2(0.630, -0.740), vec2(-0.240, 0.060), vec2(0.280, 0.520), vec2(-0.610, -0.250));

// 인터리브드 그래디언트 잡음. (Jorge Jimenez, SIGGRAPH 2014 Next Generation Post Processing in Call of Duty: Advanced Warfare 발표에 실린 식)
float interleavedGradientNoise(vec2 screenPosition) {
	return fract(52.9829189 * fract(dot(screenPosition, vec2(0.06711056, 0.00583715))));
}

// 섀도우. (회전 포아송 PCF, 비교 샘플러, 경사 비례 바이어스)
float sampleShadowFactor(float normalDotLight) {
	vec3 projected = lightSpacePosition.xyz / lightSpacePosition.w;
	projected = projected * 0.5 + 0.5;
	if (projected.x < 0.0 || projected.x > 1.0 || projected.y < 0.0 || projected.y > 1.0 || projected.z > 1.0) {
		return 1.0;
	}
	float bias = 0.0012 + 0.0035 * (1.0 - normalDotLight);
	float angle = interleavedGradientNoise(gl_FragCoord.xy) * 2.0 * PI;
	float cosine = cos(angle);
	float sine = sin(angle);
	mat2 rotation = mat2(cosine, sine, -sine, cosine);
	float radius = shadowTexelSize * shadowSoftness;
	float shadowSum = 0.0;
	for (int sampleIndex = 0; sampleIndex < 24; ++sampleIndex) {
		if (sampleIndex >= shadowSampleCount) {
			break;
		}
		vec2 tapOffset = rotation * POISSON_DISK[sampleIndex] * radius;
		shadowSum += texture(shadowMapTexture, vec3(projected.xy + tapOffset, projected.z - bias));
	}
	return shadowSum / float(max(shadowSampleCount, 1));
}

// 화면 공간 미분 코탄젠트 프레임. (탄젠트 어트리뷰트 불필요)
mat3 computeTangentFrame(vec3 geometryNormal) {
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
		return mat3(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), geometryNormal);
	}
	float inverseLength = inversesqrt(maxLengthSquared);
	return mat3(tangent * inverseLength, bitangent * inverseLength, geometryNormal);
}

// GGX 스펙큘러 로브. (분포 x 가시성, 프레넬 제외)
float specularLobe(float normalDotHalf, float normalDotLight, float normalDotView, float roughness) {
	float alpha = roughness * roughness;
	float alphaSquared = alpha * alpha;
	float distributionDenominator = normalDotHalf * normalDotHalf * (alphaSquared - 1.0) + 1.0;
	float distribution = alphaSquared / (PI * distributionDenominator * distributionDenominator);
	float geometryK = alpha * 0.5;
	float visibilityLight = normalDotLight / (normalDotLight * (1.0 - geometryK) + geometryK);
	float visibilityView = normalDotView / (normalDotView * (1.0 - geometryK) + geometryK);
	return distribution * visibilityLight * visibilityView / max(4.0 * normalDotLight * normalDotView, 0.0001);
}

// 스튜디오 반사 환경. (3색 반구 + 조명 방향의 소프트박스, 러프니스만큼 넓어진다)
vec3 studioEnvironment(vec3 direction, float roughness) {
	float upness = direction.y;
	vec3 environment = upness > 0.0 ? mix(ambientHorizonColor, ambientSkyColor, upness) : mix(ambientHorizonColor, ambientGroundColor, -upness);
	for (int lightIndex = 0; lightIndex < 3; ++lightIndex) {
		float cosine = dot(direction, normalize(lightDirections[lightIndex]));
		float innerCosine = mix(0.985, 0.55, roughness);
		float outerCosine = mix(0.92, 0.1, roughness);
		environment += lightColors[lightIndex] * smoothstep(outerCosine, innerCosine, cosine) * 0.09;
	}
	return environment;
}

// 환경 BRDF. (분할 합의 F0 크기와 치우침)
// - GGX 분포, Smith Schlick-GGX 가림(k = alpha / 2, alpha = 거칠기 제곱), Schlick 프레넬의 적분을 거칠기 41 x N.V 41 칸에서 직접 셈하고(GGX 중요도 표본 16384), 엔진이 고른 꼴로 최소 제곱 맞춤했다.
// - 꼴: 기저 (1, v, v^2, (1 - v)^5, (1 - v)^10, exp(-v / (0.03 + 0.5 r^2))) 마다 거칠기 r 의 5 차 다항식. 표와의 오차 실효값은 크기 0.002, 치우침 0.006.
const float ENVIRONMENT_BRDF_SCALE[36] = float[36](
	1.01699, -0.462087, -4.78649, 10.9375, -13.0907, 8.23938,
	-0.0482927, 1.36293, 8.33234, -24.4386, 23.2876, -10.5187,
	0.0336012, -0.970407, -3.42552, 14.9976, -17.5714, 7.66053,
	-1.07537, 2.77434, -12.4562, 31.4601, -31.45, 11.1884,
	0.133159, -5.46128, 30.5112, -65.0372, 60.8288, -21.0635,
	-0.110087, 3.88285, -4.01684, -3.5935, 9.59612, -7.3835);
const float ENVIRONMENT_BRDF_BIAS[36] = float[36](
	0.255199, -7.48498, 49.5645, -116.779, 111.03, -37.5437,
	-0.647375, 19.0528, -127.332, 306.515, -301.688, 105.404,
	0.39846, -11.7169, 78.5122, -190.809, 190.983, -67.8913,
	0.263929, 20.7049, -128.3, 269.923, -244.066, 81.3211,
	0.773071, -19.3793, 86.5221, -162.018, 140.409, -46.3276,
	-0.368092, 1.32691, 8.47847, -23.211, 23.5474, -8.63813);

vec2 environmentBrdf(float roughness, float normalDotView) {
	float clampedRoughness = clamp(roughness, 0.0, 1.0);
	float fresnelTerm = pow(clamp(1.0 - normalDotView, 0.0, 1.0), 5.0);
	float basisList[6] = float[6](1.0, normalDotView, normalDotView * normalDotView, fresnelTerm, fresnelTerm * fresnelTerm, exp(-normalDotView / (0.03 + 0.5 * clampedRoughness * clampedRoughness)));
	vec2 result = vec2(0.0);
	for (int basisIndex = 0; basisIndex < 6; basisIndex++) {
		vec2 polynomial = vec2(0.0);
		for (int powerIndex = 5; powerIndex >= 0; powerIndex--) {
			polynomial = polynomial * clampedRoughness + vec2(ENVIRONMENT_BRDF_SCALE[basisIndex * 6 + powerIndex], ENVIRONMENT_BRDF_BIAS[basisIndex * 6 + powerIndex]);
		}
		result += polynomial * basisList[basisIndex];
	}
	return max(result, vec2(0.0));
}

vec3 hemisphereAmbient(vec3 direction) {
	float upness = direction.y;
	return upness > 0.0 ? mix(ambientHorizonColor, ambientSkyColor, upness) : mix(ambientHorizonColor, ambientGroundColor, -upness);
}

// sRGB → 선형. (색 텍스처는 8비트 sRGB 로 올리고 여기서 디코드한다)
vec3 decodeSrgb(vec3 encoded) {
	return pow(max(encoded, vec3(0.0)), vec3(2.2));
}

// UDN 노멀 합성. (탄젠트 공간 xy 누적)
vec3 blendTangentNormal(vec3 baseNormal, vec2 detailXy) {
	return normalize(vec3(baseNormal.xy + detailXy, baseNormal.z));
}

// 주름 맵 가중치. (마스크 아틀라스 4x4 타일 x RGBA 채널 → 주름 맵 1 / 2 / 3 기여 합, 가중치가 있는 타일만 샘플)
vec3 wrinkleWeights(vec2 textureCoordinate) {
	vec3 weights = vec3(0.0);
	for (int tileIndex = 0; tileIndex < 16; ++tileIndex) {
		if ((wrinkleTileMask & (1 << tileIndex)) == 0) {
			continue;
		}
		vec2 tileOffset = vec2(float(tileIndex % 4), float(tileIndex / 4)) * 0.25;
		vec4 mask = texture(maskAtlasTexture, textureCoordinate * 0.25 + tileOffset);
		weights += mask.r * wrinkleChannelWeights[tileIndex * 4];
		weights += mask.g * wrinkleChannelWeights[tileIndex * 4 + 1];
		weights += mask.b * wrinkleChannelWeights[tileIndex * 4 + 2];
		weights += mask.a * wrinkleChannelWeights[tileIndex * 4 + 3];
	}
	return clamp(weights, 0.0, 1.0);
}

// 2x2 아틀라스 타일 좌표. (0 좌하, 1 우하, 2 좌상)
vec2 layerTileCoordinate(vec2 textureCoordinate, int tileIndex) {
	return textureCoordinate * 0.5 + vec2(float(tileIndex % 2), float(tileIndex / 2)) * 0.5;
}

//==============================================================================
// 피부.
//==============================================================================
void shadeSkin(vec2 textureCoordinate, vec3 geometryNormal, vec3 viewDirection, mat3 tangentFrame) {
	vec3 albedo = decodeSrgb(texture(baseColorTexture, textureCoordinate).rgb);
	float specularMap = texture(specularTexture, textureCoordinate).r;
	float roughnessMap = texture(metallicRoughnessTexture, textureCoordinate).g;
	float occlusion = texture(occlusionTexture, textureCoordinate).r;
	vec3 tangentNormal = texture(normalTexture, textureCoordinate).xyz * 2.0 - 1.0;
	tangentNormal.xy *= normalStrength;
	tangentNormal = normalize(tangentNormal);

	// 표정 주름. (마스크 가중치만큼 주름 노멀을 UDN 합성하고 주름 색으로 알베도를 당긴다, 활성 타일이 없으면 통째로 건너뛴다)
	if (useWrinkleMaps == 1 && wrinkleTileMask != 0) {
		vec3 weights = wrinkleWeights(textureCoordinate);
		for (int layerIndex = 0; layerIndex < 3; ++layerIndex) {
			float weight = weights[layerIndex];
			if (weight < 0.002) {
				continue;
			}
			vec2 tileCoordinate = layerTileCoordinate(textureCoordinate, layerIndex);
			vec3 wrinkleNormal = texture(layerNormalTexture, tileCoordinate).xyz * 2.0 - 1.0;
			tangentNormal = blendTangentNormal(tangentNormal, wrinkleNormal.xy * weight * wrinkleNormalStrength);
			// 주름 색 맵은 중간 회색(0.5) 기준의 오버레이, 가중치만큼 밝기 / 색 변화를 곱하되 (눈꺼풀이 파랗게 뜨지 않도록) 폭을 제한한다
			vec3 wrinkleColor = texture(layerColorTexture, tileCoordinate).rgb;
			albedo *= clamp(vec3(1.0) + (wrinkleColor - vec3(0.5)) * 2.0 * weight * wrinkleColorStrength, vec3(0.7), vec3(1.3));
		}
	}

	// 마이크로 노멀 / 캐비티. (타일링, 모공 사이 미세 요철)
	vec4 micro = texture(microTexture, textureCoordinate * microTiling);
	vec2 microNormalXy = (micro.xy * 2.0 - 1.0) * microStrength;
	float microCavity = mix(1.0, micro.a, microStrength * 0.6);

	// 높이 맵 미분 디테일. (모공 캐비티 높이)
	float heightLeft = texture(detailHeightTexture, textureCoordinate - vec2(detailTexelSize, 0.0)).r;
	float heightRight = texture(detailHeightTexture, textureCoordinate + vec2(detailTexelSize, 0.0)).r;
	float heightDown = texture(detailHeightTexture, textureCoordinate - vec2(0.0, detailTexelSize)).r;
	float heightUp = texture(detailHeightTexture, textureCoordinate + vec2(0.0, detailTexelSize)).r;
	vec2 heightGradient = vec2(heightRight - heightLeft, heightUp - heightDown);
	vec3 combinedTangentNormal = blendTangentNormal(tangentNormal, -heightGradient * detailStrength + microNormalXy);
	vec3 surfaceNormal = normalize(tangentFrame * combinedTangentNormal);
	vec3 diffuseNormal = normalize(mix(geometryNormal, normalize(tangentFrame * tangentNormal), 0.75));

	// 캐비티. (높이 - 저역 높이, 모공 / 주름 골 차폐)
	float heightCenter = texture(detailHeightTexture, textureCoordinate).r;
	float heightAverage = textureLod(detailHeightTexture, textureCoordinate, 4.0).r;
	float cavity = clamp(1.0 + (heightCenter - heightAverage) * cavityStrength, 0.2, 1.0) * microCavity;

	// 러프니스 / 피부 F0. (골은 거칠고 스펙큘러 맵이 밝은 부위(입술 / 콧등)는 매끈)
	float roughness = mix(roughnessRange.y, roughnessRange.x, specularMap) * roughnessMap * roughnessFactor;
	roughness = clamp(roughness * mix(1.25, 0.9, cavity), 0.28, 1.0);
	float fresnelBase = 0.028 * specularStrength * mix(0.7, 1.2, specularMap);
	float normalDotView = max(dot(surfaceNormal, viewDirection), 0.0001);
	float geometryDotView = max(dot(geometryNormal, viewDirection), 0.0);

	float keyDotLight = max(dot(diffuseNormal, normalize(lightDirections[0])), 0.0);
	float shadowFactor = 1.0;
	if (shadowStrength > 0.001) {
		shadowFactor = mix(1.0, sampleShadowFactor(keyDotLight), shadowStrength);
	}

	// 직접광. (확산 조도는 부드러운 노멀, 스펙큘러는 디테일 노멀)
	vec3 irradiance = vec3(0.0);
	vec3 specular = vec3(0.0);
	for (int lightIndex = 0; lightIndex < 3; ++lightIndex) {
		vec3 lightDirection = normalize(lightDirections[lightIndex]);
		vec3 lightColor = lightColors[lightIndex];
		float lightShadow = lightIndex == 0 ? shadowFactor : 1.0;
		float diffuseDotLight = max(dot(diffuseNormal, lightDirection), 0.0);
		irradiance += lightColor * diffuseDotLight * lightShadow;

		float normalDotLight = max(dot(surfaceNormal, lightDirection), 0.0);
		if (normalDotLight > 0.0) {
			vec3 halfVector = normalize(viewDirection + lightDirection);
			float normalDotHalf = max(dot(surfaceNormal, halfVector), 0.0);
			float viewDotHalf = max(dot(viewDirection, halfVector), 0.0);
			float fresnel = fresnelBase + (1.0 - fresnelBase) * pow(1.0 - viewDotHalf, 5.0);
			float lobeWide = specularLobe(normalDotHalf, normalDotLight, normalDotView, roughness);
			float lobeTight = specularLobe(normalDotHalf, normalDotLight, normalDotView, clamp(roughness * 0.65, 0.2, 1.0));
			specular += lightColor * fresnel * (0.8 * lobeWide + 0.2 * lobeTight) * PI * normalDotLight * lightShadow;
		}

		// 잔털. (스침각에서 빛을 감싸는 옅은 산란, 림 라이트에 특히 반응, 피부색을 띠고 눈꺼풀 가장자리 같은 좁은 면에서 흰 줄이 되지 않게 약하게)
		float wrapDotLight = max(dot(geometryNormal, lightDirection) * 0.5 + 0.5, 0.0);
		specular += lightColor * mix(vec3(1.0), albedo * 2.0, 0.5) * pow(1.0 - geometryDotView, 5.0) * wrapDotLight * sheenStrength * 0.5 * lightShadow;
	}

	// 앰비언트 확산 + 환경 반사.
	irradiance += hemisphereAmbient(diffuseNormal) * occlusion * mix(1.0, cavity, 0.5);
	vec3 reflection = reflect(-viewDirection, surfaceNormal);
	vec3 environment = studioEnvironment(reflection, roughness);
	vec2 environmentScaleBias = environmentBrdf(roughness, normalDotView);
	specular += environment * (fresnelBase * environmentScaleBias.x + environmentScaleBias.y) * occlusion;
	specular *= cavity;

	irradianceOutput = vec4(irradiance, subsurfaceAmount * subsurfaceFactor);
	albedoOutput = vec4(albedo * baseColorFactor, 1.0);
	specularOutput = vec4(specular, 1.0);
	normalOutput = vec4(surfaceNormal, 1.0);
}

//==============================================================================
// 눈. (기본 색 = 공막, 12 = 홍채 노멀, 13 = 홍채 색, 노멀 맵 = 공막 정맥, UV 중심이 동공)
//==============================================================================
void shadeEye(vec2 textureCoordinate, vec3 geometryNormal, vec3 viewDirection, mat3 tangentFrame) {
	vec2 centered = textureCoordinate - vec2(0.5);
	float radial = length(centered);
	float corneaMask = 1.0 - smoothstep(eyeIrisRadius * 0.95, eyeIrisRadius * 1.25, radial);

	// 각막 굴절 시차. (굴절된 시선을 탄젠트 공간으로 내려 홍채 평면 깊이만큼 UV 를 민다)
	vec3 refracted = refract(-viewDirection, geometryNormal, 1.0 / 1.376);
	vec3 refractedTangent = transpose(tangentFrame) * refracted;
	vec2 parallax = refractedTangent.xy / max(-refractedTangent.z, 0.25) * eyeIrisDepth * corneaMask;
	vec2 irisCoordinate = textureCoordinate + parallax;
	vec2 irisCentered = irisCoordinate - vec2(0.5);
	float irisRadial = length(irisCentered);
	vec2 irisTextureCoordinate = irisCentered / (eyeIrisRadius * 2.0) + vec2(0.5);
	vec4 irisSample = texture(layerColorTexture, irisTextureCoordinate);
	vec3 irisColor = decodeSrgb(irisSample.rgb);
	float irisMask = (1.0 - smoothstep(eyeIrisRadius * 0.97, eyeIrisRadius * 1.03, irisRadial)) * corneaMask;
	float limbus = smoothstep(eyeIrisRadius * 0.72, eyeIrisRadius * 1.0, irisRadial);
	irisColor *= mix(1.0, 0.25, limbus);
	float pupil = 1.0 - smoothstep(eyePupilRadius * 0.8, eyePupilRadius * 1.15, irisRadial);
	irisColor = mix(irisColor, vec3(0.002), pupil);

	vec3 scleraColor = decodeSrgb(texture(baseColorTexture, textureCoordinate).rgb);
	vec3 albedo = mix(scleraColor, irisColor, irisMask);

	// 노멀. (공막 정맥 + 홍채 요철, 각막 하이라이트는 매끈한 기하 노멀)
	vec3 scleraNormal = texture(normalTexture, textureCoordinate).xyz * 2.0 - 1.0;
	scleraNormal.xy *= 0.5 * (1.0 - corneaMask);
	vec3 irisNormal = texture(layerNormalTexture, irisTextureCoordinate).xyz * 2.0 - 1.0;
	vec3 diffuseTangentNormal = normalize(vec3(scleraNormal.xy + irisNormal.xy * irisMask * 0.8, 1.0));
	vec3 diffuseNormal = normalize(tangentFrame * diffuseTangentNormal);
	vec3 corneaNormal = normalize(mix(normalize(tangentFrame * normalize(vec3(scleraNormal.xy, 1.0))), geometryNormal, corneaMask));

	// 홍채 안쪽 자기 차폐. (림버스 쪽 어둡고 동공 쪽으로 파여 있는 느낌)
	float irisDepthShade = mix(1.0, 0.65 + 0.35 * (1.0 - limbus), irisMask);

	float normalDotView = max(dot(corneaNormal, viewDirection), 0.0001);
	float fresnelBase = 0.025;
	float corneaRoughness = mix(0.16, 0.035, corneaMask);
	vec3 irradiance = vec3(0.0);
	vec3 specular = vec3(0.0);
	for (int lightIndex = 0; lightIndex < 3; ++lightIndex) {
		vec3 lightDirection = normalize(lightDirections[lightIndex]);
		vec3 lightColor = lightColors[lightIndex];
		// 홍채는 각막 안쪽에서 굴절된 빛을 받아 밝기가 넓게 퍼진다
		float diffuseDotLight = max(dot(diffuseNormal, lightDirection), 0.0);
		float wrapped = max(dot(diffuseNormal, lightDirection) * 0.6 + 0.4, 0.0);
		irradiance += lightColor * mix(diffuseDotLight, wrapped, irisMask);

		float normalDotLight = max(dot(corneaNormal, lightDirection), 0.0);
		if (normalDotLight > 0.0) {
			vec3 halfVector = normalize(viewDirection + lightDirection);
			float normalDotHalf = max(dot(corneaNormal, halfVector), 0.0);
			float viewDotHalf = max(dot(viewDirection, halfVector), 0.0);
			float fresnel = fresnelBase + (1.0 - fresnelBase) * pow(1.0 - viewDotHalf, 5.0);
			specular += lightColor * fresnel * specularLobe(normalDotHalf, normalDotLight, normalDotView, corneaRoughness) * PI * normalDotLight;
		}
	}
	irradiance += hemisphereAmbient(diffuseNormal) * 1.1;
	irradiance *= irisDepthShade;
	vec3 reflection = reflect(-viewDirection, corneaNormal);
	vec3 environment = studioEnvironment(reflection, corneaRoughness);
	vec2 environmentScaleBias = environmentBrdf(corneaRoughness, normalDotView);
	specular += environment * (fresnelBase * environmentScaleBias.x + environmentScaleBias.y) * 1.1;
	// 눈꺼풀이 감길수록 눈알은 눈꺼풀 그늘에 들어간다 (틈 사이로 흰 하이라이트가 비치지 않게)
	float lidShade = 1.0 - eyeClosure * 0.75;
	irradiance *= lidShade;
	specular *= lidShade * lidShade;

	irradianceOutput = vec4(irradiance * albedo * baseColorFactor, 0.0);
	albedoOutput = vec4(1.0, 1.0, 1.0, 1.0);
	specularOutput = vec4(specular, 1.0);
	normalOutput = vec4(corneaNormal, 1.0);
}

//==============================================================================
// 머리카락. (기본 색 rgb + 커버리지 a, 12 = 카드 탄젠트 rgb + 가닥 좌표 a)
//==============================================================================
void shadeHair(vec2 textureCoordinate, vec3 geometryNormal, vec3 viewDirection, mat3 tangentFrame, float coverage) {
	vec3 albedo = decodeSrgb(texture(baseColorTexture, textureCoordinate).rgb) * baseColorFactor;
	vec4 tangentSample = texture(layerNormalTexture, textureCoordinate);
	vec3 cardTangent = tangentSample.xyz * 2.0 - 1.0;
	vec3 strandTangent = normalize(tangentFrame * cardTangent);
	float strandCoordinate = tangentSample.a;

	// 뿌리 차폐. (두피 쪽 가닥은 겹쳐서 어둡다)
	float rootShade = mix(1.0 - hairRootShade, 1.0, smoothstep(0.0, 0.6, strandCoordinate));
	albedo *= rootShade;

	float keyDotLight = max(dot(geometryNormal, normalize(lightDirections[0])) * 0.5 + 0.5, 0.0);
	float shadowFactor = 1.0;
	if (shadowStrength > 0.001) {
		shadowFactor = mix(1.0, sampleShadowFactor(keyDotLight), shadowStrength * 0.85);
	}

	vec3 irradiance = vec3(0.0);
	vec3 specular = vec3(0.0);
	for (int lightIndex = 0; lightIndex < 3; ++lightIndex) {
		vec3 lightDirection = normalize(lightDirections[lightIndex]);
		vec3 lightColor = lightColors[lightIndex];
		float lightShadow = lightIndex == 0 ? shadowFactor : 1.0;

		// 확산. (Kajiya 가닥 확산과 감싼 램버트를 섞어 카드 티가 덜 나게)
		float tangentDotLight = dot(strandTangent, lightDirection);
		float strandDiffuse = sqrt(max(1.0 - tangentDotLight * tangentDotLight, 0.0));
		float wrappedDiffuse = max(dot(geometryNormal, lightDirection) * 0.5 + 0.5, 0.0);
		irradiance += lightColor * mix(strandDiffuse, wrappedDiffuse, 0.55) * 0.55 * lightShadow;

		// 이방성 하이라이트. (뿌리 쪽으로 민 백색 1차 로브 + 끝 쪽으로 민 색 2차 로브)
		vec3 halfVector = normalize(viewDirection + lightDirection);
		vec3 shiftedPrimary = normalize(strandTangent + geometryNormal * hairSpecularShift.x);
		vec3 shiftedSecondary = normalize(strandTangent + geometryNormal * hairSpecularShift.y);
		float tangentDotHalfPrimary = dot(shiftedPrimary, halfVector);
		float tangentDotHalfSecondary = dot(shiftedSecondary, halfVector);
		float lobePrimary = pow(sqrt(max(1.0 - tangentDotHalfPrimary * tangentDotHalfPrimary, 0.0)), hairSpecularPower.x);
		float lobeSecondary = pow(sqrt(max(1.0 - tangentDotHalfSecondary * tangentDotHalfSecondary, 0.0)), hairSpecularPower.y);
		float lightWrap = smoothstep(-0.35, 0.35, dot(geometryNormal, lightDirection));
		specular += lightColor * (lobePrimary * hairSpecularIntensity.x + lobeSecondary * hairSpecularIntensity.y * albedo) * lightWrap * lightShadow;

		// 투과 역광. (빛을 등진 가닥이 비쳐 보인다)
		float backlight = pow(max(dot(viewDirection, -lightDirection), 0.0), 6.0);
		specular += lightColor * albedo * backlight * hairSpecularIntensity.z * lightShadow;
	}
	irradiance += hemisphereAmbient(geometryNormal) * rootShade;

	// 프린지 블렌드 패스: 조도는 커버리지만큼 줄이고(알파만 기여) 색은 스펙큘러 버퍼에 얹는다 → (1 - a) x 바닥 + a x 머리카락
	if (passMode == 1) {
		irradianceOutput = vec4(0.0, 0.0, 0.0, coverage);
		albedoOutput = vec4(0.0, 0.0, 0.0, 0.0);
		specularOutput = vec4(irradiance * albedo + specular, coverage);
		normalOutput = vec4(0.0, 0.0, 0.0, 0.0);
		return;
	}
	irradianceOutput = vec4(irradiance * albedo, 0.0);
	albedoOutput = vec4(1.0, 1.0, 1.0, 1.0);
	specularOutput = vec4(specular, 1.0);
	normalOutput = vec4(geometryNormal, 1.0);
}

//==============================================================================
// 차폐 / 유체. (블렌드 패스, 차폐는 조도만 어둡게, 유체는 스펙큘러만 얹는다)
//==============================================================================
void shadeOverlay(vec2 textureCoordinate, vec3 geometryNormal, vec3 viewDirection) {
	if (shadingMode == 3) {
		float darkness = texture(baseColorTexture, textureCoordinate).a * baseColorFactor.x;
		irradianceOutput = vec4(0.0, 0.0, 0.0, darkness);
		albedoOutput = vec4(0.0, 0.0, 0.0, 0.0);
		specularOutput = vec4(0.0, 0.0, 0.0, darkness * 0.6);
		normalOutput = vec4(0.0, 0.0, 0.0, 0.0);
		return;
	}
	float normalDotView = max(dot(geometryNormal, viewDirection), 0.0001);
	float fresnelBase = 0.02;
	vec3 specular = vec3(0.0);
	for (int lightIndex = 0; lightIndex < 3; ++lightIndex) {
		vec3 lightDirection = normalize(lightDirections[lightIndex]);
		float normalDotLight = max(dot(geometryNormal, lightDirection), 0.0);
		if (normalDotLight > 0.0) {
			vec3 halfVector = normalize(viewDirection + lightDirection);
			float normalDotHalf = max(dot(geometryNormal, halfVector), 0.0);
			float viewDotHalf = max(dot(viewDirection, halfVector), 0.0);
			float fresnel = fresnelBase + (1.0 - fresnelBase) * pow(1.0 - viewDotHalf, 5.0);
			specular += lightColors[lightIndex] * fresnel * specularLobe(normalDotHalf, normalDotLight, normalDotView, 0.08) * PI * normalDotLight;
		}
	}
	vec3 reflection = reflect(-viewDirection, geometryNormal);
	vec2 environmentScaleBias = environmentBrdf(0.08, normalDotView);
	specular += studioEnvironment(reflection, 0.08) * (fresnelBase * environmentScaleBias.x + environmentScaleBias.y);
	float coverage = texture(baseColorTexture, textureCoordinate).a * baseColorFactor.x;
	irradianceOutput = vec4(0.0, 0.0, 0.0, 0.0);
	albedoOutput = vec4(0.0, 0.0, 0.0, 0.0);
	specularOutput = vec4(specular * 1.2, 0.5 * coverage);
	normalOutput = vec4(0.0, 0.0, 0.0, 0.0);
}

void main() {
	vec2 textureCoordinate = fragmentTextureCoordinate;

	// 커버리지. (머리카락은 기본 색 알파, 그 외 컷아웃 머티리얼은 오파시티 텍스처 알파)
	float coverage = 1.0;
	bool isHairMode = shadingMode == 2 || shadingMode == 5;
	if (isHairMode) {
		coverage = texture(baseColorTexture, textureCoordinate).a;
		if (passMode == 0 && coverage < 0.5) {
			discard;
		}
		if (passMode == 1 && (coverage >= 0.5 || coverage < 0.03)) {
			discard;
		}
		coverage = passMode == 1 ? coverage * 2.0 : 1.0;
	}
	else if (useAlphaCutout == 1) {
		float opacity = texture(opacityTexture, textureCoordinate).a;
		if (opacity < 0.5) {
			discard;
		}
	}

	vec3 geometryNormal = normalize(worldNormal);
	vec3 viewDirection = normalize(cameraPosition - worldPosition);
	if (dot(geometryNormal, viewDirection) < 0.0) {
		geometryNormal = -geometryNormal;
	}
	mat3 tangentFrame = computeTangentFrame(geometryNormal);

	if (shadingMode == 1) {
		shadeEye(textureCoordinate, geometryNormal, viewDirection, tangentFrame);
	}
	else if (isHairMode) {
		shadeHair(textureCoordinate, geometryNormal, viewDirection, tangentFrame, coverage);
	}
	else if (shadingMode == 3 || shadingMode == 4) {
		shadeOverlay(textureCoordinate, geometryNormal, viewDirection);
	}
	else {
		shadeSkin(textureCoordinate, geometryNormal, viewDirection, tangentFrame);
	}
}
";

/// 피부 버텍스 셰이더 앞부분. (원본 템플릿의 ${MORPH_GLSL} 앞, 기본 스키닝 / 모프 경로 + 머리카락 흔들림)
const SKIN_VERTEXSHADER_HEAD_SOURCE: &str = "#version 300 es
layout(location = 0) in vec3 vertexPosition;
layout(location = 1) in vec3 vertexNormal;
layout(location = 2) in vec2 vertexTextureCoordinate;
layout(location = 3) in uvec4 vertexJoints;
layout(location = 4) in vec4 vertexWeights;
uniform mat4 modelMatrix;
uniform mat4 viewProjectionMatrix;
uniform mat4 lightViewProjectionMatrix;
uniform mat4 jointMatrices[96];
uniform int shadingMode;
uniform sampler2D layerNormalTexture;
uniform vec4 hairSway;
out vec3 worldPosition;
out vec3 worldNormal;
out vec2 fragmentTextureCoordinate;
out vec4 lightSpacePosition;
";

/// 피부 버텍스 셰이더 뒷부분. (원본 템플릿의 ${MORPH_GLSL} 뒤, 카드 탄젠트 아틀라스의 가닥 좌표(a)가 클수록(끝) 크게 흔든다)
const SKIN_VERTEXSHADER_TAIL_SOURCE: &str = "
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

	// 머리카락 흔들림. (hairSway: x 진폭(m), y 시간, z 주파수, 가닥 끝일수록, 낮은 주파수 여러 겹)
	if (shadingMode == 2 && hairSway.x > 0.0) {
		float strandCoordinate = textureLod(layerNormalTexture, vertexTextureCoordinate, 0.0).a;
		float tip = strandCoordinate * strandCoordinate;
		float phase = skinnedPosition.x * 7.0 + skinnedPosition.y * 5.0 + hairSway.y * hairSway.z;
		vec3 offset = vec3(sin(phase) * 0.6 + sin(phase * 2.3 + 1.7) * 0.25, -abs(sin(phase * 0.7 + 0.4)) * 0.12, cos(phase * 1.3) * 0.35);
		skinnedPosition.xyz += offset * hairSway.x * tip;
	}
	worldPosition = skinnedPosition.xyz;
	worldNormal = normalize(mat3(modelMatrix) * mat3(skinMatrix) * normalize(morphedNormal));
	fragmentTextureCoordinate = vertexTextureCoordinate;
	lightSpacePosition = lightViewProjectionMatrix * skinnedPosition;
	gl_Position = viewProjectionMatrix * skinnedPosition;
}
";

/// 컷아웃 깊이 버텍스 셰이더 앞부분. (섀도우 맵 캐스팅, 머리카락 / 속눈썹의 투명 영역을 오파시티로 제거하기 위해 UV 를 넘긴다)
const CUTOUT_DEPTH_VERTEXSHADER_HEAD_SOURCE: &str = "#version 300 es
layout(location = 0) in vec3 vertexPosition;
layout(location = 2) in vec2 vertexTextureCoordinate;
layout(location = 3) in uvec4 vertexJoints;
layout(location = 4) in vec4 vertexWeights;
uniform mat4 modelMatrix;
uniform mat4 lightViewProjectionMatrix;
uniform mat4 jointMatrices[96];
out vec2 fragmentTextureCoordinate;
";

/// 컷아웃 깊이 버텍스 셰이더 뒷부분.
const CUTOUT_DEPTH_VERTEXSHADER_TAIL_SOURCE: &str = "
void main() {
	vec3 morphedPosition = vertexPosition;
	vec3 morphedNormal = vec3(0.0, 1.0, 0.0);
	applyMorphTargets(morphedPosition, morphedNormal);
	mat4 skinMatrix = vertexWeights.x * jointMatrices[vertexJoints.x]
		+ vertexWeights.y * jointMatrices[vertexJoints.y]
		+ vertexWeights.z * jointMatrices[vertexJoints.z]
		+ vertexWeights.w * jointMatrices[vertexJoints.w];
	fragmentTextureCoordinate = vertexTextureCoordinate;
	gl_Position = lightViewProjectionMatrix * modelMatrix * skinMatrix * vec4(morphedPosition, 1.0);
}
";

/// 컷아웃 깊이 프래그먼트 셰이더. (커버리지 텍스처 알파 0.5 미만 제거)
const CUTOUT_DEPTH_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D coverageTexture;
void main() {
	if (texture(coverageTexture, fragmentTextureCoordinate).a < 0.5) {
		discard;
	}
}
";

//==============================================================================
/// 피부 버텍스 셰이더 소스 만들기. (원본 SKIN_VERTEXSHADER_SOURCE, 모프 GLSL 을 가운데 넣는다)
///
/// # 돌려줌
/// 피부 버텍스 셰이더 GLSL. (앞부분 + MORPH_GLSL + 뒷부분, `#version 300 es` 로 시작)
//==============================================================================
pub fn create_skin_vertex_shader_source() -> String {
	format!("{}{}{}", SKIN_VERTEXSHADER_HEAD_SOURCE, MORPH_GLSL, SKIN_VERTEXSHADER_TAIL_SOURCE)
}

//==============================================================================
/// 컷아웃 깊이 버텍스 셰이더 소스 만들기. (원본 CUTOUT_DEPTH_VERTEXSHADER_SOURCE)
///
/// # 돌려줌
/// 컷아웃 깊이 버텍스 셰이더 GLSL. (앞부분 + MORPH_GLSL + 뒷부분, `#version 300 es` 로 시작)
//==============================================================================
pub fn create_cutout_depth_vertex_shader_source() -> String {
	format!("{}{}{}", CUTOUT_DEPTH_VERTEXSHADER_HEAD_SOURCE, MORPH_GLSL, CUTOUT_DEPTH_VERTEXSHADER_TAIL_SOURCE)
}

//==============================================================================
/// 인체 피부 렌더러.
/// - 출력은 MRT 넷이다. (0 확산 조도 + 산란 마스크, 1 선형 알베도 + 커버리지, 2 스펙큘러, 3 월드 노멀)
///
/// # 예제
/// 머티리얼 이름으로 셰이딩 모드를 고르고 MRT 판에 불투명, 블렌드 두 패스로 그리기
/// ```rust,no_run
/// use glow::HasContext;
/// use vanilla::{HumanSkinRenderer, Matrix4, RenderTarget, RenderTargetOptions, SkinnedModel, Vector3, SKIN_SHADING_EYE, SKIN_SHADING_HAIR};
///
/// fn create_head(gl: &glow::Context, glb_bytes: &[u8]) -> Result<(HumanSkinRenderer, SkinnedModel, RenderTarget), String> {
/// 	let mut human_skin_renderer = HumanSkinRenderer::new(gl)?;
/// 	let mut skinned_model = SkinnedModel::load_from_bytes(gl, glb_bytes, &human_skin_renderer)?;
/// 	for drawable in skinned_model.get_drawable_list() {
/// 		if drawable.material_name.contains("eyeball") {
/// 			human_skin_renderer.set_material_shading(&drawable.material, SKIN_SHADING_EYE);
/// 		}
/// 		else if drawable.material_name.contains("hair") {
/// 			human_skin_renderer.set_material_shading(&drawable.material, SKIN_SHADING_HAIR);
/// 		}
/// 	}
/// 	skinned_model.update(0.0);
/// 	let options = RenderTargetOptions { color_attachment_count: 4, use_float_color: true, use_depth_texture: true, ..RenderTargetOptions::default() };
/// 	let scene_render_target = RenderTarget::new(gl, 1280, 720, true, Some(options))?;
/// 	Ok((human_skin_renderer, skinned_model, scene_render_target))
/// }
///
/// fn render_head(gl: &glow::Context, human_skin_renderer: &HumanSkinRenderer, skinned_model: &mut SkinnedModel, scene_render_target: &RenderTarget) {
/// 	let aspect_ratio = scene_render_target.get_width() as f64 / scene_render_target.get_height() as f64;
/// 	let camera_position = Vector3::create(0.0, 1.6, 0.6);
/// 	let view_matrix = Matrix4::create_look_at(camera_position, Vector3::create(0.0, 1.6, 0.0), Vector3::up());
/// 	let mut view_projection_matrix = Matrix4::create_perspective(0.5, aspect_ratio, 0.05, 10.0);
/// 	view_projection_matrix.multiply(&view_matrix);
/// 	let model_matrix = Matrix4::create_identity();
///
/// 	scene_render_target.bind(gl);
/// 	unsafe {
/// 		gl.clear_color(0.0, 0.0, 0.0, 0.0);
/// 		gl.depth_mask(true);
/// 		gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
/// 		gl.enable(glow::DEPTH_TEST);
/// 	}
/// 	human_skin_renderer.draw(gl, skinned_model, view_projection_matrix.get_elements(), model_matrix.get_elements(), camera_position.x, camera_position.y, camera_position.z);
/// 	unsafe {
/// 		gl.enable(glow::BLEND);
/// 		gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
/// 		gl.depth_mask(false);
/// 	}
/// 	human_skin_renderer.draw_blended(gl, skinned_model, view_projection_matrix.get_elements(), model_matrix.get_elements(), camera_position.x, camera_position.y, camera_position.z);
/// 	unsafe {
/// 		gl.depth_mask(true);
/// 		gl.disable(glow::BLEND);
/// 	}
/// }
/// ```
//==============================================================================
pub struct HumanSkinRenderer {
	skinned_model_renderer: SkinnedModelRenderer,
	cutout_depth_shader_program: ShaderProgram,
	material_shading_modes: Vec<(Rc<Material>, i32)>,
	shadow_map_texture: Option<GlTexture>,
	light_view_projection_elements: Option<[f32; 16]>,
	shadow_strength: f64,
	shadow_texel_size: f64,
	shadow_softness: f64,
	shadow_sample_count: i32,
	hair_sway: [f64; 3],
	use_hair_fringe: bool,
	light_directions: [f32; 9],
	light_colors: [f32; 9],
	ambient_sky_color: [f64; 3],
	ambient_horizon_color: [f64; 3],
	ambient_ground_color: [f64; 3],
	normal_strength: f64,
	detail_strength: f64,
	detail_texel_size: f64,
	cavity_strength: f64,
	roughness_range: [f64; 2],
	specular_strength: f64,
	sheen_strength: f64,
	subsurface_amount: f64,
	micro_tiling: f64,
	micro_strength: f64,
	use_wrinkle_maps: bool,
	wrinkle_channel_weights: [f32; WRINKLE_CHANNEL_COUNT * 3],
	wrinkle_tile_mask: i32,
	wrinkle_normal_strength: f64,
	wrinkle_color_strength: f64,
	eye_iris_radius: f64,
	eye_pupil_radius: f64,
	eye_iris_depth: f64,
	eye_closure: f64,
	hair_specular_shift: [f64; 3],
	hair_specular_power: [f64; 3],
	hair_specular_intensity: [f64; 3],
	hair_root_shade: f64,
}

impl HumanSkinRenderer {
	//==============================================================================
	/// 생성. (피부 셰이더를 주입한 기반 렌더러 + 컷아웃 깊이 셰이더, 기본 3점 조명)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트. (Graphic::get_gl 로 꺼낸다)
	///
	/// # 돌려줌
	/// 새 피부 렌더러. (기본 3점 조명, 그림자 강도 0, 그림자 표본 12, 머리카락 프린지 켜짐, 주름 맵 꺼짐) 셰이더 컴파일에 실패하면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<HumanSkinRenderer, String> {
		let skin_vertex_shader_source = create_skin_vertex_shader_source();
		let skinned_model_renderer = SkinnedModelRenderer::new(gl, Some(SKIN_FRAGMENTSHADER_SOURCE), Some(&skin_vertex_shader_source))?;
		let cutout_depth_vertex_shader_source = create_cutout_depth_vertex_shader_source();
		let cutout_depth_shader_program = ShaderProgram::new(gl, cutout_depth_vertex_shader_source.trim(), CUTOUT_DEPTH_FRAGMENTSHADER_SOURCE.trim())?;
		let mut human_skin_renderer = HumanSkinRenderer {
			skinned_model_renderer,
			cutout_depth_shader_program,
			material_shading_modes: Vec::new(),
			shadow_map_texture: None,
			light_view_projection_elements: None,
			shadow_strength: 0.0,
			shadow_texel_size: 1.0 / 2048.0,
			shadow_softness: 2.5,
			shadow_sample_count: 12,
			hair_sway: [0.0, 0.0, 1.5],
			use_hair_fringe: true,
			light_directions: [0.0; 9],
			light_colors: [0.0; 9],
			ambient_sky_color: [0.34, 0.36, 0.42],
			ambient_horizon_color: [0.16, 0.15, 0.16],
			ambient_ground_color: [0.05, 0.04, 0.045],
			normal_strength: 1.0,
			detail_strength: 6.0,
			detail_texel_size: 1.0 / 4096.0,
			cavity_strength: 6.0,
			roughness_range: [0.32, 0.62],
			specular_strength: 1.0,
			sheen_strength: 0.35,
			subsurface_amount: 1.0,
			micro_tiling: 0.0,
			micro_strength: 0.0,
			use_wrinkle_maps: false,
			wrinkle_channel_weights: [0.0; WRINKLE_CHANNEL_COUNT * 3],
			wrinkle_tile_mask: 0,
			wrinkle_normal_strength: 1.0,
			wrinkle_color_strength: 1.0,
			eye_iris_radius: 0.133,
			eye_pupil_radius: 0.04,
			eye_iris_depth: 0.05,
			eye_closure: 0.0,
			hair_specular_shift: [-0.12, 0.14, 0.0],
			hair_specular_power: [180.0, 28.0, 0.0],
			hair_specular_intensity: [0.35, 0.5, 0.25],
			hair_root_shade: 0.45,
		};

		// 기본 3점 조명. (키: 좌상 전방 난색 / 필: 우측 한색 약광 / 림: 우후방 상단 백색)
		human_skin_renderer.set_light(0, 0.45, 0.6, 0.65, 1.0, 0.93, 0.85, 3.2);
		human_skin_renderer.set_light(1, -0.7, 0.15, 0.6, 0.55, 0.65, 0.85, 0.8);
		human_skin_renderer.set_light(2, 0.5, 0.4, -0.75, 0.9, 0.95, 1.0, 2.2);
		Ok(human_skin_renderer)
	}

	//==============================================================================
	/// 불투명 패스 출력. (피부 / 눈 / 머리카락 컷아웃, 깊이 쓰기)
	/// - 차폐, 유체 셰이딩 머티리얼은 건너뛴다. 깊이 시험과 쓰기는 부르는 쪽이 켠다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `skinned_model`: 그릴 모델. (바뀐 모프 타깃을 먼저 올린다)
	/// - `view_projection_elements`: 투영 x 보기 행렬 원소. (열 우선 4x4)
	/// - `model_matrix_elements`: 모델 행렬 원소. (열 우선 4x4)
	/// - `camera_x`: 카메라 월드 x. (스펙큘러와 시선 셈)
	/// - `camera_y`: 카메라 월드 y.
	/// - `camera_z`: 카메라 월드 z.
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context, skinned_model: &mut SkinnedModel, view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16], camera_x: f64, camera_y: f64, camera_z: f64) {
		self.draw_pass(gl, skinned_model, view_projection_elements, model_matrix_elements, camera_x, camera_y, camera_z, 0);
	}

	//==============================================================================
	/// 블렌드 패스 출력. (머리카락 프린지 / 차폐 / 유체, 호출자가 블렌드 켜고 깊이 쓰기를 끈 뒤 호출)
	/// - 머리카락과 얼굴 털(프린지를 켰을 때), 차폐, 유체 머티리얼만 그린다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `skinned_model`: 그릴 모델. (바뀐 모프 타깃을 먼저 올린다)
	/// - `view_projection_elements`: 투영 x 보기 행렬 원소. (열 우선 4x4)
	/// - `model_matrix_elements`: 모델 행렬 원소. (열 우선 4x4)
	/// - `camera_x`: 카메라 월드 x. (스펙큘러와 시선 셈)
	/// - `camera_y`: 카메라 월드 y.
	/// - `camera_z`: 카메라 월드 z.
	///
	/// # 예제
	/// 어태치먼트마다 알파 섞기를 켜고 깊이 쓰기를 끈 뒤 블렌드 패스 그리기
	/// ```rust,no_run
	/// use glow::HasContext;
	/// use vanilla::{HumanSkinRenderer, SkinnedModel};
	///
	/// fn render_blended(gl: &glow::Context, human_skin_renderer: &HumanSkinRenderer, skinned_model: &mut SkinnedModel, view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16]) {
	/// 	unsafe {
	/// 		gl.enable(glow::BLEND);
	/// 		gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
	/// 		gl.depth_mask(false);
	/// 	}
	/// 	human_skin_renderer.draw_blended(gl, skinned_model, view_projection_elements, model_matrix_elements, 0.0, 1.6, 0.6);
	/// 	unsafe {
	/// 		gl.depth_mask(true);
	/// 		gl.disable(glow::BLEND);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn draw_blended(&self, gl: &glow::Context, skinned_model: &mut SkinnedModel, view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16], camera_x: f64, camera_y: f64, camera_z: f64) {
		self.draw_pass(gl, skinned_model, view_projection_elements, model_matrix_elements, camera_x, camera_y, camera_z, 1);
	}

	//==============================================================================
	/// 패스 출력. (공통 유니폼 → 드로어블 순회, 패스에 맞는 셰이딩 모드만, pass_mode 0 = 불투명, 1 = 블렌드)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `skinned_model`: 그릴 모델. (바뀐 모프 타깃을 먼저 올린다)
	/// - `view_projection_elements`: 투영 x 보기 행렬 원소. (열 우선 4x4)
	/// - `model_matrix_elements`: 모델 행렬 원소. (열 우선 4x4)
	/// - `camera_x`: 카메라 월드 x. (스펙큘러와 시선 셈)
	/// - `camera_y`: 카메라 월드 y.
	/// - `camera_z`: 카메라 월드 z.
	/// - `pass_mode`: 패스. (0 = 불투명, 1 = 블렌드)
	//==============================================================================
	#[allow(clippy::too_many_arguments)]
	pub fn draw_pass(&self, gl: &glow::Context, skinned_model: &mut SkinnedModel, view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16], camera_x: f64, camera_y: f64, camera_z: f64, pass_mode: i32) {
		skinned_model.upload_pending_morph_targets(gl);
		let shader_program = self.skinned_model_renderer.get_shader_program();
		let shader_program = &*shader_program;
		shader_program.use_program(gl);
		let view_projection_location = shader_program.get_uniform_location(gl, "viewProjectionMatrix");
		let model_matrix_location = shader_program.get_uniform_location(gl, "modelMatrix");
		let camera_position_location = shader_program.get_uniform_location(gl, "cameraPosition");
		unsafe {
			gl.uniform_matrix_4_f32_slice(view_projection_location.as_ref(), false, view_projection_elements);
			gl.uniform_matrix_4_f32_slice(model_matrix_location.as_ref(), false, model_matrix_elements);
			gl.uniform_3_f64(camera_position_location.as_ref(), camera_x, camera_y, camera_z);
		}
		self.apply_environment_uniforms(gl, shader_program);
		self.apply_shadow_uniforms(gl, shader_program);
		let pass_mode_location = shader_program.get_uniform_location(gl, "passMode");
		let hair_sway_location = shader_program.get_uniform_location(gl, "hairSway");
		let shading_mode_location = shader_program.get_uniform_location(gl, "shadingMode");
		unsafe {
			gl.uniform_1_i32(pass_mode_location.as_ref(), pass_mode);
			gl.uniform_4_f64(hair_sway_location.as_ref(), self.hair_sway[0], self.hair_sway[1], self.hair_sway[2], 0.0);
		}

		let joint_matrices_location = shader_program.get_uniform_location(gl, "jointMatrices[0]");
		let skin_list = skinned_model.get_skin_list();
		for drawable in skinned_model.get_drawable_list().iter() {
			let shading_mode = self.get_material_shading_mode(&drawable.material);
			let is_overlay = shading_mode == SKIN_SHADING_OCCLUSION || shading_mode == SKIN_SHADING_FLUID;
			let is_hair = shading_mode == SKIN_SHADING_HAIR || shading_mode == SKIN_SHADING_FACIALHAIR;
			if pass_mode == 0 && is_overlay {
				continue;
			}
			if pass_mode == 1 && !is_overlay && !is_hair {
				continue;
			}
			if pass_mode == 1 && is_hair && !self.use_hair_fringe {
				continue;
			}
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
			self.skinned_model_renderer.apply_morph_uniforms(gl, shader_program, drawable);
			unsafe {
				gl.uniform_1_i32(shading_mode_location.as_ref(), shading_mode);
			}
			drawable.material.apply(gl);
			draw_drawable_triangles(gl, drawable);
		}
		unsafe {
			crate::core::glstatecache::bind_vertex_array(gl, None);
		}
	}

	//==============================================================================
	/// 깊이 패스 출력. (섀도우 맵 캐스팅, 컷아웃 / 머리카락 머티리얼은 커버리지 텍스처로 투명 영역 제거, 차폐 / 유체는 제외)
	/// - 부르기 전에 ShadowMap::begin_render 로 깊이 판을 묶는다. 컷아웃 패스는 0 번 유닛에 커버리지 텍스처를 묶는다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `skinned_model`: 그릴 모델. (바뀐 모프 타깃을 먼저 올린다)
	/// - `light_view_projection_elements`: 광원 뷰 투영 행렬 원소. (ShadowMap::get_light_view_projection_matrix 의 원소)
	/// - `model_matrix_elements`: 모델 행렬 원소. (열 우선 4x4)
	///
	/// # 예제
	/// 키 라이트 섀도우 맵에 깊이를 그리고 본 패스에서 받게 하기
	/// ```rust,no_run
	/// use vanilla::{HumanSkinRenderer, Matrix4, ShadowMap, SkinnedModel, Vector3};
	///
	/// fn render_shadow(gl: &glow::Context, human_skin_renderer: &mut HumanSkinRenderer, shadow_map: &mut ShadowMap, skinned_model: &mut SkinnedModel) {
	/// 	shadow_map.update_light_matrix(Vector3::create(0.0, 1.6, 0.0), Vector3::create(-0.45, -0.6, -0.65), 0.45, 0.2, 4.0);
	/// 	let light_view_projection_elements = *shadow_map.get_light_view_projection_matrix().get_elements();
	/// 	let model_matrix = Matrix4::create_identity();
	/// 	shadow_map.begin_render(gl);
	/// 	human_skin_renderer.draw_depth(gl, skinned_model, &light_view_projection_elements, model_matrix.get_elements());
	/// 	let texel_size = 1.0 / shadow_map.get_resolution() as f64;
	/// 	human_skin_renderer.set_shadow_map(Some(shadow_map.get_depth_texture()), Some(&light_view_projection_elements), texel_size);
	/// 	human_skin_renderer.set_shadow_strength(0.85);
	/// }
	/// ```
	//==============================================================================
	pub fn draw_depth(&self, gl: &glow::Context, skinned_model: &mut SkinnedModel, light_view_projection_elements: &[f32; 16], model_matrix_elements: &[f32; 16]) {
		skinned_model.upload_pending_morph_targets(gl);
		let depth_shader_program = self.skinned_model_renderer.get_depth_shader_program();
		let cutout_depth_shader_program = self.get_cutout_depth_shader_program();
		for (shader_program, use_cutout) in [(depth_shader_program, false), (cutout_depth_shader_program, true)] {
			shader_program.use_program(gl);
			let light_view_projection_location = shader_program.get_uniform_location(gl, "lightViewProjectionMatrix");
			let model_matrix_location = shader_program.get_uniform_location(gl, "modelMatrix");
			let joint_matrices_location = shader_program.get_uniform_location(gl, "jointMatrices[0]");
			let coverage_texture_location = shader_program.get_uniform_location(gl, "coverageTexture");
			unsafe {
				gl.uniform_matrix_4_f32_slice(light_view_projection_location.as_ref(), false, light_view_projection_elements);
				gl.uniform_matrix_4_f32_slice(model_matrix_location.as_ref(), false, model_matrix_elements);
			}
			let skin_list = skinned_model.get_skin_list();
			for drawable in skinned_model.get_drawable_list().iter() {
				let shading_mode = self.get_material_shading_mode(&drawable.material);
				if shading_mode == SKIN_SHADING_OCCLUSION || shading_mode == SKIN_SHADING_FLUID {
					continue;
				}
				let material = &drawable.material;
				let use_alpha_cutout = material.get_use_alpha_cutout();
				let is_hair_material = shading_mode == SKIN_SHADING_HAIR || shading_mode == SKIN_SHADING_FACIALHAIR;
				let needs_cutout = is_hair_material || use_alpha_cutout;
				if needs_cutout != use_cutout {
					continue;
				}
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
				self.skinned_model_renderer.apply_morph_uniforms(gl, shader_program, drawable);
				if use_cutout {
					// 커버리지 텍스처. (머리카락은 기본 색 알파, 그 외는 오파시티 텍스처)
					let coverage_name = if is_hair_material {
						"baseColorTexture"
					}
					else {
						"opacityTexture"
					};
					for (uniform_name, _texture_unit, texture) in material.get_texture_bindings().iter() {
						if uniform_name == coverage_name {
							unsafe {
								gl.uniform_1_i32(coverage_texture_location.as_ref(), TEXTURE_UNIT_BASECOLOR as i32);
								crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + TEXTURE_UNIT_BASECOLOR);
								crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, *texture);
							}
						}
					}
				}
				draw_drawable_triangles(gl, drawable);
			}
		}
		unsafe {
			crate::core::glstatecache::bind_vertex_array(gl, None);
		}
	}

	//==============================================================================
	/// 환경 유니폼 적용. (조명 / 앰비언트 / 피부 / 주름 / 마이크로 / 눈 / 머리카락 파라미터)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shader_program`: 유니폼을 걸 셰이더. (지금 쓰는 프로그램이어야 한다)
	//==============================================================================
	pub fn apply_environment_uniforms(&self, gl: &glow::Context, shader_program: &ShaderProgram) {
		let light_directions_location = shader_program.get_uniform_location(gl, "lightDirections[0]");
		let light_colors_location = shader_program.get_uniform_location(gl, "lightColors[0]");
		let ambient_sky_color_location = shader_program.get_uniform_location(gl, "ambientSkyColor");
		let ambient_horizon_color_location = shader_program.get_uniform_location(gl, "ambientHorizonColor");
		let ambient_ground_color_location = shader_program.get_uniform_location(gl, "ambientGroundColor");
		let normal_strength_location = shader_program.get_uniform_location(gl, "normalStrength");
		let detail_strength_location = shader_program.get_uniform_location(gl, "detailStrength");
		let detail_texel_size_location = shader_program.get_uniform_location(gl, "detailTexelSize");
		let cavity_strength_location = shader_program.get_uniform_location(gl, "cavityStrength");
		let roughness_range_location = shader_program.get_uniform_location(gl, "roughnessRange");
		let specular_strength_location = shader_program.get_uniform_location(gl, "specularStrength");
		let sheen_strength_location = shader_program.get_uniform_location(gl, "sheenStrength");
		let subsurface_amount_location = shader_program.get_uniform_location(gl, "subsurfaceAmount");
		let micro_tiling_location = shader_program.get_uniform_location(gl, "microTiling");
		let micro_strength_location = shader_program.get_uniform_location(gl, "microStrength");
		let use_wrinkle_maps_location = shader_program.get_uniform_location(gl, "useWrinkleMaps");
		let wrinkle_channel_weights_location = shader_program.get_uniform_location(gl, "wrinkleChannelWeights[0]");
		let wrinkle_tile_mask_location = shader_program.get_uniform_location(gl, "wrinkleTileMask");
		let wrinkle_normal_strength_location = shader_program.get_uniform_location(gl, "wrinkleNormalStrength");
		let wrinkle_color_strength_location = shader_program.get_uniform_location(gl, "wrinkleColorStrength");
		let eye_iris_radius_location = shader_program.get_uniform_location(gl, "eyeIrisRadius");
		let eye_pupil_radius_location = shader_program.get_uniform_location(gl, "eyePupilRadius");
		let eye_iris_depth_location = shader_program.get_uniform_location(gl, "eyeIrisDepth");
		let eye_closure_location = shader_program.get_uniform_location(gl, "eyeClosure");
		let hair_specular_shift_location = shader_program.get_uniform_location(gl, "hairSpecularShift");
		let hair_specular_power_location = shader_program.get_uniform_location(gl, "hairSpecularPower");
		let hair_specular_intensity_location = shader_program.get_uniform_location(gl, "hairSpecularIntensity");
		let hair_root_shade_location = shader_program.get_uniform_location(gl, "hairRootShade");
		unsafe {
			gl.uniform_3_f32_slice(light_directions_location.as_ref(), &self.light_directions);
			gl.uniform_3_f32_slice(light_colors_location.as_ref(), &self.light_colors);
			gl.uniform_3_f64(ambient_sky_color_location.as_ref(), self.ambient_sky_color[0], self.ambient_sky_color[1], self.ambient_sky_color[2]);
			gl.uniform_3_f64(ambient_horizon_color_location.as_ref(), self.ambient_horizon_color[0], self.ambient_horizon_color[1], self.ambient_horizon_color[2]);
			gl.uniform_3_f64(ambient_ground_color_location.as_ref(), self.ambient_ground_color[0], self.ambient_ground_color[1], self.ambient_ground_color[2]);
			gl.uniform_1_f64(normal_strength_location.as_ref(), self.normal_strength);
			gl.uniform_1_f64(detail_strength_location.as_ref(), self.detail_strength);
			gl.uniform_1_f64(detail_texel_size_location.as_ref(), self.detail_texel_size);
			gl.uniform_1_f64(cavity_strength_location.as_ref(), self.cavity_strength);
			gl.uniform_2_f64(roughness_range_location.as_ref(), self.roughness_range[0], self.roughness_range[1]);
			gl.uniform_1_f64(specular_strength_location.as_ref(), self.specular_strength);
			gl.uniform_1_f64(sheen_strength_location.as_ref(), self.sheen_strength);
			gl.uniform_1_f64(subsurface_amount_location.as_ref(), self.subsurface_amount);
			gl.uniform_1_f64(micro_tiling_location.as_ref(), self.micro_tiling);
			gl.uniform_1_f64(micro_strength_location.as_ref(), self.micro_strength);
			gl.uniform_1_i32(use_wrinkle_maps_location.as_ref(), i32::from(self.use_wrinkle_maps));
			gl.uniform_3_f32_slice(wrinkle_channel_weights_location.as_ref(), &self.wrinkle_channel_weights);
			gl.uniform_1_i32(wrinkle_tile_mask_location.as_ref(), self.wrinkle_tile_mask);
			gl.uniform_1_f64(wrinkle_normal_strength_location.as_ref(), self.wrinkle_normal_strength);
			gl.uniform_1_f64(wrinkle_color_strength_location.as_ref(), self.wrinkle_color_strength);
			gl.uniform_1_f64(eye_iris_radius_location.as_ref(), self.eye_iris_radius);
			gl.uniform_1_f64(eye_pupil_radius_location.as_ref(), self.eye_pupil_radius);
			gl.uniform_1_f64(eye_iris_depth_location.as_ref(), self.eye_iris_depth);
			gl.uniform_1_f64(eye_closure_location.as_ref(), self.eye_closure);
			gl.uniform_3_f64(hair_specular_shift_location.as_ref(), self.hair_specular_shift[0], self.hair_specular_shift[1], self.hair_specular_shift[2]);
			gl.uniform_3_f64(hair_specular_power_location.as_ref(), self.hair_specular_power[0], self.hair_specular_power[1], self.hair_specular_power[2]);
			gl.uniform_3_f64(hair_specular_intensity_location.as_ref(), self.hair_specular_intensity[0], self.hair_specular_intensity[1], self.hair_specular_intensity[2]);
			gl.uniform_1_f64(hair_root_shade_location.as_ref(), self.hair_root_shade);
		}
	}

	//==============================================================================
	/// 섀도우 유니폼 적용. (섀도우 샘플러 타입 불일치로 드로우가 무효화되지 않도록 강도와 무관하게 항상 바인드)
	/// - 섀도우 맵과 광원 행렬이 둘 다 있으면 설정한 강도를 쓰고, 아니면 강도 0 과 빈 섀도우 텍스처를 묶는다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shader_program`: 유니폼을 걸 셰이더. (지금 쓰는 프로그램이어야 한다)
	//==============================================================================
	pub fn apply_shadow_uniforms(&self, gl: &glow::Context, shader_program: &ShaderProgram) {
		let shadow_map_texture_location = shader_program.get_uniform_location(gl, "shadowMapTexture");
		let shadow_strength_location = shader_program.get_uniform_location(gl, "shadowStrength");
		let shadow_softness_location = shader_program.get_uniform_location(gl, "shadowSoftness");
		let shadow_sample_count_location = shader_program.get_uniform_location(gl, "shadowSampleCount");
		unsafe {
			gl.uniform_1_i32(shadow_map_texture_location.as_ref(), TEXTURE_UNIT_SHADOWMAP as i32);
			gl.uniform_1_f64(shadow_softness_location.as_ref(), self.shadow_softness);
			gl.uniform_1_i32(shadow_sample_count_location.as_ref(), self.shadow_sample_count);
		}
		match (self.shadow_map_texture, &self.light_view_projection_elements) {
			(Some(shadow_map_texture), Some(light_view_projection_elements)) => {
				let light_view_projection_location = shader_program.get_uniform_location(gl, "lightViewProjectionMatrix");
				let shadow_texel_size_location = shader_program.get_uniform_location(gl, "shadowTexelSize");
				unsafe {
					gl.uniform_matrix_4_f32_slice(light_view_projection_location.as_ref(), false, light_view_projection_elements);
					gl.uniform_1_f64(shadow_texel_size_location.as_ref(), self.shadow_texel_size);
					gl.uniform_1_f64(shadow_strength_location.as_ref(), self.shadow_strength);
					crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + TEXTURE_UNIT_SHADOWMAP);
					crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(shadow_map_texture));
					crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
				}
			}
			_ => {
				unsafe {
					gl.uniform_1_f64(shadow_strength_location.as_ref(), 0.0);
					crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + TEXTURE_UNIT_SHADOWMAP);
					crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(self.skinned_model_renderer.get_empty_shadow_texture()));
					crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
				}
			}
		}
	}

	//==============================================================================
	/// 섀도우 맵 설정. (본 패스에서 그림자 수신, 기반 렌더러에도 전달, 행렬은 복사해 둔다)
	///
	/// # 인자
	/// - `shadow_map_texture`: 섀도우 깊이 텍스처. (None 이면 그림자를 받지 않는다)
	/// - `light_view_projection_elements`: 광원 뷰 투영 행렬 원소. (None 이면 그림자를 받지 않는다)
	/// - `texel_size`: 섀도우 텍셀 크기. (1 / 섀도우 맵 해상도)
	//==============================================================================
	pub fn set_shadow_map(&mut self, shadow_map_texture: Option<GlTexture>, light_view_projection_elements: Option<&[f32; 16]>, texel_size: f64) {
		self.skinned_model_renderer.set_shadow_map(shadow_map_texture, light_view_projection_elements, texel_size);
		self.shadow_map_texture = shadow_map_texture;
		self.light_view_projection_elements = light_view_projection_elements.copied();
		self.shadow_texel_size = texel_size;
	}

	//==============================================================================
	/// 그림자 강도 설정. (0 = 수신 안 함)
	///
	/// # 인자
	/// - `shadow_strength`: 그림자 강도. (0 이면 받지 않고 1 이면 다 받는다, 기본 0)
	//==============================================================================
	pub fn set_shadow_strength(&mut self, shadow_strength: f64) {
		self.skinned_model_renderer.set_shadow_strength(shadow_strength);
		self.shadow_strength = shadow_strength;
	}

	//==============================================================================
	/// 그림자 부드러움 설정. (포아송 반경, 섀도우 텍셀 단위)
	///
	/// # 인자
	/// - `shadow_softness`: 포아송 표본을 흩는 반경. (섀도우 텍셀 단위, 기본 2.5)
	//==============================================================================
	pub fn set_shadow_softness(&mut self, shadow_softness: f64) {
		self.shadow_softness = shadow_softness;
	}

	//==============================================================================
	/// 그림자 표본 수 설정. (1 ~ 24, 많을수록 부드럽고 느리다)
	///
	/// # 인자
	/// - `shadow_sample_count`: 표본 수. (1 ~ 24 로 자른다, 기본 12)
	//==============================================================================
	pub fn set_shadow_sample_count(&mut self, shadow_sample_count: i32) {
		self.shadow_sample_count = shadow_sample_count.clamp(1, SHADOW_SAMPLE_COUNT);
	}

	//==============================================================================
	/// 머리카락 흔들림 설정. (진폭 m / 시간 초 / 주파수, 진폭 0 이면 없음)
	///
	/// # 인자
	/// - `amplitude`: 진폭. (미터, 가닥 끝일수록 크게, 0 이면 흔들지 않음, 기본 0)
	/// - `time`: 시간. (초, 프레임마다 흐른 시간을 넣는다)
	/// - `frequency`: 주파수. (기본 1.5)
	///
	/// # 예제
	/// 프레임마다 흐른 시간으로 머리카락 흔들기
	/// ```rust,no_run
	/// use vanilla::HumanSkinRenderer;
	///
	/// fn update_hair(human_skin_renderer: &mut HumanSkinRenderer, elapsed_seconds: f64) {
	/// 	human_skin_renderer.set_hair_sway(0.003, elapsed_seconds, 1.6);
	/// }
	/// ```
	//==============================================================================
	pub fn set_hair_sway(&mut self, amplitude: f64, time: f64, frequency: f64) {
		self.hair_sway = [amplitude, time, frequency];
	}

	//==============================================================================
	/// 머리카락 프린지 블렌드 사용 설정. (false 면 컷아웃만, 성능 위주)
	///
	/// # 인자
	/// - `use_hair_fringe`: true 면 블렌드 패스에서 머리카락과 얼굴 털 프린지를 그린다. (기본 true)
	//==============================================================================
	pub fn set_hair_fringe(&mut self, use_hair_fringe: bool) {
		self.use_hair_fringe = use_hair_fringe;
	}

	//==============================================================================
	/// 머티리얼 셰이딩 모드 등록. (등록하지 않은 머티리얼은 피부, 같은 인스턴스는 값만 바꾼다)
	///
	/// # 인자
	/// - `material`: 드로어블이 든 머티리얼. (Rc 포인터로 인스턴스를 가린다)
	/// - `shading_mode`: 셰이딩 모드. (SKIN_SHADING_SKIN, SKIN_SHADING_EYE 같은 상수)
	//==============================================================================
	pub fn set_material_shading(&mut self, material: &Rc<Material>, shading_mode: i32) {
		for entry in self.material_shading_modes.iter_mut() {
			if Rc::ptr_eq(&entry.0, material) {
				entry.1 = shading_mode;
				return;
			}
		}
		self.material_shading_modes.push((material.clone(), shading_mode));
	}

	//==============================================================================
	/// 머티리얼 셰이딩 모드 반환.
	///
	/// # 인자
	/// - `material`: 볼 머티리얼.
	///
	/// # 돌려줌
	/// 등록한 셰이딩 모드. 등록하지 않은 머티리얼이면 SKIN_SHADING_SKIN.
	//==============================================================================
	pub fn get_material_shading_mode(&self, material: &Material) -> i32 {
		for (registered_material, shading_mode) in self.material_shading_modes.iter() {
			if std::ptr::eq(Rc::as_ptr(registered_material), material) {
				return *shading_mode;
			}
		}
		SKIN_SHADING_SKIN
	}

	//==============================================================================
	/// 조명 설정. (0 = 키 라이트: 섀도우 맵 수신 / 1 = 필 / 2 = 림, 방향은 표면에서 빛을 향하는 벡터)
	///
	/// # 인자
	/// - `light_index`: 조명 번호. (0 = 키, 1 = 필, 2 = 림, 3 이상은 무시)
	/// - `direction_x`: 표면에서 빛을 향하는 방향의 x. (길이는 상관없고 정규화한다)
	/// - `direction_y`: 방향의 y.
	/// - `direction_z`: 방향의 z.
	/// - `red`: 빛 색의 빨강. (선형)
	/// - `green`: 빛 색의 초록. (선형)
	/// - `blue`: 빛 색의 파랑. (선형)
	/// - `intensity`: 세기. (색에 곱한다)
	///
	/// # 예제
	/// 위 앞에서 비추는 따뜻한 키 라이트와 뒤에서 비추는 흰 림 라이트
	/// ```rust,no_run
	/// use vanilla::HumanSkinRenderer;
	///
	/// fn set_portrait_lights(human_skin_renderer: &mut HumanSkinRenderer) {
	/// 	human_skin_renderer.set_light(0, 0.45, 0.6, 0.65, 1.0, 0.93, 0.85, 3.2);
	/// 	human_skin_renderer.set_light(2, 0.5, 0.4, -0.75, 0.9, 0.95, 1.0, 2.2);
	/// }
	/// ```
	//==============================================================================
	#[allow(clippy::too_many_arguments)]
	pub fn set_light(&mut self, light_index: usize, direction_x: f64, direction_y: f64, direction_z: f64, red: f64, green: f64, blue: f64, intensity: f64) {
		if light_index >= 3 {
			return;
		}
		let direction_length = (direction_x * direction_x + direction_y * direction_y + direction_z * direction_z).sqrt();
		let safe_length = if direction_length > 0.0 {
			direction_length
		}
		else {
			1.0
		};
		self.light_directions[light_index * 3] = (direction_x / safe_length) as f32;
		self.light_directions[light_index * 3 + 1] = (direction_y / safe_length) as f32;
		self.light_directions[light_index * 3 + 2] = (direction_z / safe_length) as f32;
		self.light_colors[light_index * 3] = (red * intensity) as f32;
		self.light_colors[light_index * 3 + 1] = (green * intensity) as f32;
		self.light_colors[light_index * 3 + 2] = (blue * intensity) as f32;
	}

	//==============================================================================
	/// 앰비언트 3색 설정. (하늘 / 지평 / 바닥, 선형 색)
	///
	/// # 인자
	/// - `sky_color`: 위를 향한 면의 색. (선형 rgb, 기본 (0.34, 0.36, 0.42))
	/// - `horizon_color`: 옆을 향한 면의 색. (선형 rgb, 기본 (0.16, 0.15, 0.16))
	/// - `ground_color`: 아래를 향한 면의 색. (선형 rgb, 기본 (0.05, 0.04, 0.045))
	//==============================================================================
	pub fn set_ambient_colors(&mut self, sky_color: [f64; 3], horizon_color: [f64; 3], ground_color: [f64; 3]) {
		self.ambient_sky_color = sky_color;
		self.ambient_horizon_color = horizon_color;
		self.ambient_ground_color = ground_color;
	}

	//==============================================================================
	/// 노멀 맵 강도 설정. (1 = 원본, 크면 주름이 깊어진다)
	///
	/// # 인자
	/// - `normal_strength`: 노멀 맵 xy 배율. (1 = 원본, 기본 1)
	//==============================================================================
	pub fn set_normal_strength(&mut self, normal_strength: f64) {
		self.normal_strength = normal_strength;
	}

	//==============================================================================
	/// 높이 디테일 강도 설정. (0 = 디테일 노멀 없음)
	///
	/// # 인자
	/// - `detail_strength`: 높이 디테일 노멀 강도. (0 이면 없음, 기본 6)
	//==============================================================================
	pub fn set_detail_strength(&mut self, detail_strength: f64) {
		self.detail_strength = detail_strength;
	}

	//==============================================================================
	/// 높이 디테일 텍셀 크기 설정. (1 / 높이 맵 해상도)
	///
	/// # 인자
	/// - `detail_texel_size`: 높이 맵 한 텍셀의 UV 크기. (1 / 높이 맵 해상도, 기본 1 / 4096)
	//==============================================================================
	pub fn set_detail_texel_size(&mut self, detail_texel_size: f64) {
		self.detail_texel_size = detail_texel_size;
	}

	//==============================================================================
	/// 캐비티 강도 설정. (0 = 차폐 없음)
	///
	/// # 인자
	/// - `cavity_strength`: 캐비티 강도. (높이 맵의 오목한 곳을 어둡게, 0 이면 없음, 기본 6)
	//==============================================================================
	pub fn set_cavity_strength(&mut self, cavity_strength: f64) {
		self.cavity_strength = cavity_strength;
	}

	//==============================================================================
	/// 러프니스 범위 설정. (스펙큘러 맵 1 → 최소, 0 → 최대)
	///
	/// # 인자
	/// - `minimum_roughness`: 스펙큘러 맵이 1 인 곳의 러프니스. (기본 0.32)
	/// - `maximum_roughness`: 스펙큘러 맵이 0 인 곳의 러프니스. (기본 0.62)
	//==============================================================================
	pub fn set_roughness_range(&mut self, minimum_roughness: f64, maximum_roughness: f64) {
		self.roughness_range = [minimum_roughness, maximum_roughness];
	}

	//==============================================================================
	/// 스펙큘러 강도 설정. (피부 F0 배율)
	///
	/// # 인자
	/// - `specular_strength`: 피부 F0 배율. (기본 1)
	//==============================================================================
	pub fn set_specular_strength(&mut self, specular_strength: f64) {
		self.specular_strength = specular_strength;
	}

	//==============================================================================
	/// 잔털 산란 강도 설정.
	///
	/// # 인자
	/// - `sheen_strength`: 잔털 산란 강도. (비스듬한 가장자리의 밝기, 기본 0.35)
	//==============================================================================
	pub fn set_sheen_strength(&mut self, sheen_strength: f64) {
		self.sheen_strength = sheen_strength;
	}

	//==============================================================================
	/// SSS 마스크 강도 설정. (조도 출력 알파, 0 이면 화면 공간 산란에서 제외)
	///
	/// # 인자
	/// - `subsurface_amount`: SSS 마스크 강도. (머티리얼의 서브서피스 팩터와 곱해 조도 출력 알파로, 기본 1)
	//==============================================================================
	pub fn set_subsurface_amount(&mut self, subsurface_amount: f64) {
		self.subsurface_amount = subsurface_amount;
	}

	//==============================================================================
	/// 마이크로 노멀 설정. (타일링 횟수 / 강도, 0 이면 사용 안 함)
	///
	/// # 인자
	/// - `micro_tiling`: 마이크로 텍스처 UV 반복 횟수. (기본 0)
	/// - `micro_strength`: 마이크로 노멀과 캐비티 강도. (0 이면 없음, 기본 0)
	//==============================================================================
	pub fn set_micro_detail(&mut self, micro_tiling: f64, micro_strength: f64) {
		self.micro_tiling = micro_tiling;
		self.micro_strength = micro_strength;
	}

	//==============================================================================
	/// 주름 맵 사용 설정. (마스크 아틀라스 / 주름 노멀 / 색 아틀라스가 머티리얼에 붙어 있어야 한다)
	///
	/// # 인자
	/// - `use_wrinkle_maps`: 주름 맵을 쓸지. (기본 false)
	/// - `normal_strength`: 주름 노멀 강도. (기본 1)
	/// - `color_strength`: 주름 색 강도. (기본 1)
	//==============================================================================
	pub fn set_wrinkle_maps(&mut self, use_wrinkle_maps: bool, normal_strength: f64, color_strength: f64) {
		self.use_wrinkle_maps = use_wrinkle_maps;
		self.wrinkle_normal_strength = normal_strength;
		self.wrinkle_color_strength = color_strength;
	}

	//==============================================================================
	/// 주름 채널 가중치 설정. (채널 = 마스크 아틀라스 타일 x 4 + RGBA 순서, 값 = 주름 맵 1 / 2 / 3 기여)
	/// - 다 넣은 뒤 finish_wrinkle_channel_weights 를 불러야 셰이더가 가중치가 생긴 타일을 읽는다.
	///
	/// # 인자
	/// - `channel_index`: 채널 번호. (0 ~ 63, 64 이상은 무시)
	/// - `weight1`: 주름 맵 1 의 기여 가중치.
	/// - `weight2`: 주름 맵 2 의 기여 가중치.
	/// - `weight3`: 주름 맵 3 의 기여 가중치.
	///
	/// # 예제
	/// 첫 타일 네 채널에 주름 맵 1 을 반쯤 걸고 마무리하기
	/// ```rust,no_run
	/// use vanilla::HumanSkinRenderer;
	///
	/// fn apply_expression(human_skin_renderer: &mut HumanSkinRenderer) {
	/// 	human_skin_renderer.set_wrinkle_maps(true, 1.0, 0.45);
	/// 	for channel_index in 0..4 {
	/// 		human_skin_renderer.set_wrinkle_channel_weight(channel_index, 0.5, 0.0, 0.0);
	/// 	}
	/// 	human_skin_renderer.finish_wrinkle_channel_weights();
	/// }
	/// ```
	//==============================================================================
	pub fn set_wrinkle_channel_weight(&mut self, channel_index: usize, weight1: f64, weight2: f64, weight3: f64) {
		if channel_index >= WRINKLE_CHANNEL_COUNT {
			return;
		}
		self.wrinkle_channel_weights[channel_index * 3] = weight1 as f32;
		self.wrinkle_channel_weights[channel_index * 3 + 1] = weight2 as f32;
		self.wrinkle_channel_weights[channel_index * 3 + 2] = weight3 as f32;
	}

	//==============================================================================
	/// 주름 채널 가중치 적용 마무리. (가중치가 있는 타일만 골라 비트마스크로, 셰이더가 빈 타일을 건너뛴다)
	//==============================================================================
	pub fn finish_wrinkle_channel_weights(&mut self) {
		self.wrinkle_tile_mask = build_wrinkle_tile_mask(&self.wrinkle_channel_weights);
	}

	//==============================================================================
	/// 눈 파라미터 설정. (UV 단위, 홍채 반경 / 동공 반경 / 홍채 평면 깊이)
	///
	/// # 인자
	/// - `iris_radius`: 홍채 반경. (UV 단위, 기본 0.133)
	/// - `pupil_radius`: 동공 반경. (UV 단위, 기본 0.04)
	/// - `iris_depth`: 홍채 평면 깊이. (각막 굴절 시차의 크기, 기본 0.05)
	//==============================================================================
	pub fn set_eye_parameters(&mut self, iris_radius: f64, pupil_radius: f64, iris_depth: f64) {
		self.eye_iris_radius = iris_radius;
		self.eye_pupil_radius = pupil_radius;
		self.eye_iris_depth = iris_depth;
	}

	//==============================================================================
	/// 눈꺼풀 감김 정도 설정. (0 = 뜬 눈, 1 = 감은 눈, 눈알을 눈꺼풀 그늘만큼 어둡게)
	///
	/// # 인자
	/// - `eye_closure`: 감김 정도. (0 = 뜬 눈, 1 = 감은 눈, 0 ~ 1 로 자른다)
	//==============================================================================
	pub fn set_eye_closure(&mut self, eye_closure: f64) {
		self.eye_closure = eye_closure.clamp(0.0, 1.0);
	}

	//==============================================================================
	/// 머리카락 파라미터 설정. (1차 / 2차 로브 시프트, 지수, 강도, 투과 강도, 뿌리 차폐)
	/// - specular_shift: [1차, 2차], specular_power: [1차, 2차], specular_intensity: [1차, 2차, 투과]
	///
	/// # 인자
	/// - `specular_shift`: 1차, 2차 로브 시프트. (기본 `[-0.12, 0.14]`)
	/// - `specular_power`: 1차, 2차 로브 지수. (기본 `[180.0, 28.0]`)
	/// - `specular_intensity`: 1차, 2차 로브 강도와 투과 강도. (기본 `[0.35, 0.5, 0.25]`)
	/// - `root_shade`: 뿌리 차폐. (0 이면 없음, 기본 0.45)
	//==============================================================================
	pub fn set_hair_parameters(&mut self, specular_shift: [f64; 2], specular_power: [f64; 2], specular_intensity: [f64; 3], root_shade: f64) {
		self.hair_specular_shift = [specular_shift[0], specular_shift[1], 0.0];
		self.hair_specular_power = [specular_power[0], specular_power[1], 0.0];
		self.hair_specular_intensity = specular_intensity;
		self.hair_root_shade = root_shade;
	}

	//==============================================================================
	/// 컷아웃 깊이 셰이더 프로그램 반환.
	///
	/// # 돌려줌
	/// 컷아웃 깊이 셰이더 프로그램. (머리카락, 알파 컷아웃 머티리얼의 섀도우 깊이 패스)
	//==============================================================================
	pub fn get_cutout_depth_shader_program(&self) -> &ShaderProgram {
		&self.cutout_depth_shader_program
	}
}

impl Deref for HumanSkinRenderer {
	type Target = SkinnedModelRenderer;

	//==============================================================================
	/// 기반 렌더러 빌리기. (SkinnedModelRenderer 의 함수를 그대로 부르게)
	//==============================================================================
	fn deref(&self) -> &SkinnedModelRenderer {
		&self.skinned_model_renderer
	}
}

impl DerefMut for HumanSkinRenderer {
	//==============================================================================
	/// 기반 렌더러를 고칠 수 있게 빌리기. (set_sun_direction 같은 기반 설정)
	//==============================================================================
	fn deref_mut(&mut self) -> &mut SkinnedModelRenderer {
		&mut self.skinned_model_renderer
	}
}

//==============================================================================
/// 주름 타일 비트마스크 만들기. (채널 가중치 합이 0.002 를 넘는 채널의 타일 = 채널 / 4 번 비트)
//==============================================================================
fn build_wrinkle_tile_mask(wrinkle_channel_weights: &[f32; WRINKLE_CHANNEL_COUNT * 3]) -> i32 {
	let mut tile_mask = 0;
	for channel_index in 0..WRINKLE_CHANNEL_COUNT {
		let weight_sum = wrinkle_channel_weights[channel_index * 3] + wrinkle_channel_weights[channel_index * 3 + 1] + wrinkle_channel_weights[channel_index * 3 + 2];
		if weight_sum > 0.002 {
			tile_mask |= 1 << (channel_index >> 2);
		}
	}
	tile_mask
}

//==============================================================================
/// 드로어블 삼각형 출력. (인덱스가 있으면 drawElements, 없으면 drawArrays)
//==============================================================================
fn draw_drawable_triangles(gl: &glow::Context, drawable: &Drawable) {
	unsafe {
		crate::core::glstatecache::bind_vertex_array(gl, Some(drawable.vertex_array));
		if drawable.is_indexed {
			crate::core::glstatecache::draw_elements(gl, glow::TRIANGLES, drawable.index_count as i32, drawable.index_component_type, drawable.index_byte_offset);
		}
		else {
			crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, drawable.vertex_count as i32);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 셰이더 소스의 숫자가 상수와 같은지. (원본 템플릿 ${WRINKLE_CHANNEL_COUNT} / ${SHADOW_SAMPLE_COUNT} / ${SKIN_SHADING_*} 자리)
	//==============================================================================
	#[test]
	fn shader_sources_match_constants() {
		let fragment_source = SKIN_FRAGMENTSHADER_SOURCE;
		assert!(fragment_source.starts_with("#version 300 es"));
		assert!(fragment_source.contains(&format!("uniform vec3 wrinkleChannelWeights[{}];", WRINKLE_CHANNEL_COUNT)));
		assert!(fragment_source.contains(&format!("const vec2 POISSON_DISK[{}] = vec2[](", SHADOW_SAMPLE_COUNT)));
		assert!(fragment_source.contains(&format!("sampleIndex < {}; ++sampleIndex", SHADOW_SAMPLE_COUNT)));
		assert!(fragment_source.contains(&format!("if (shadingMode == {}) {{\n\t\tfloat darkness", SKIN_SHADING_OCCLUSION)));
		assert!(fragment_source.contains(&format!("bool isHairMode = shadingMode == {} || shadingMode == {};", SKIN_SHADING_HAIR, SKIN_SHADING_FACIALHAIR)));
		assert!(fragment_source.contains(&format!("if (shadingMode == {}) {{\n\t\tshadeEye(", SKIN_SHADING_EYE)));
		assert!(fragment_source.contains(&format!("else if (shadingMode == {} || shadingMode == {}) {{\n\t\tshadeOverlay(", SKIN_SHADING_OCCLUSION, SKIN_SHADING_FLUID)));
		assert!(!fragment_source.contains("${"));

		let skin_vertex_source = create_skin_vertex_shader_source();
		assert!(skin_vertex_source.starts_with("#version 300 es"));
		assert!(skin_vertex_source.contains(MORPH_GLSL));
		assert!(skin_vertex_source.contains("uniform mat4 jointMatrices[96];"));
		assert!(skin_vertex_source.contains(&format!("if (shadingMode == {} && hairSway.x > 0.0) {{", SKIN_SHADING_HAIR)));
		let cutout_vertex_source = create_cutout_depth_vertex_shader_source();
		assert!(cutout_vertex_source.starts_with("#version 300 es"));
		assert!(cutout_vertex_source.contains(MORPH_GLSL));
		assert!(CUTOUT_DEPTH_FRAGMENTSHADER_SOURCE.contains("uniform sampler2D coverageTexture;"));
		for source in [skin_vertex_source.as_str(), cutout_vertex_source.as_str(), CUTOUT_DEPTH_FRAGMENTSHADER_SOURCE] {
			assert!(!source.contains("${"));
		}
	}

	//==============================================================================
	/// 주름 타일 비트마스크. (가중치가 있는 채널의 타일만, 0.002 이하는 빈 타일)
	//==============================================================================
	#[test]
	fn wrinkle_tile_mask_marks_weighted_tiles() {
		let mut weights = [0.0f32; WRINKLE_CHANNEL_COUNT * 3];
		assert_eq!(build_wrinkle_tile_mask(&weights), 0);
		weights[0] = 0.5;
		weights[5 * 3 + 2] = 0.3;
		weights[63 * 3 + 1] = 1.0;
		weights[40 * 3] = 0.001;
		assert_eq!(build_wrinkle_tile_mask(&weights), (1 << 0) | (1 << 1) | (1 << 15));
	}

	//==============================================================================
	/// 텍스처 유닛이 머티리얼 기본 슬롯, 섀도우, 모프 유닛과 겹치지 않는지. (원본 주석의 9 ~ 14)
	//==============================================================================
	#[test]
	fn texture_units_avoid_reserved_slots() {
		let unit_list = [SKIN_TEXTURE_UNIT_DETAILHEIGHT, SKIN_TEXTURE_UNIT_MICRO, SKIN_TEXTURE_UNIT_MASKATLAS, SKIN_TEXTURE_UNIT_LAYERNORMAL, SKIN_TEXTURE_UNIT_LAYERCOLOR, SKIN_TEXTURE_UNIT_AUXILIARY];
		assert_eq!(unit_list, [9, 10, 11, 12, 13, 14]);
		assert!(unit_list.iter().all(|unit| *unit != crate::graphics3d::skinnedmodelrenderer::MORPH_TEXTURE_UNIT && *unit > crate::graphics3d::material::TEXTURE_UNIT_OPACITY));
	}

	//==============================================================================
	/// 눈 셰이딩은 홍채 색을 13 번(layerColorTexture), 홍채 노멀을 12 번(layerNormalTexture)에서 읽는다.
	//==============================================================================
	#[test]
	fn eye_iris_reads_color_from_layer_color_unit() {
		assert_eq!(SKIN_TEXTURE_UNIT_LAYERNORMAL, 12);
		assert_eq!(SKIN_TEXTURE_UNIT_LAYERCOLOR, 13);
		let fragment_source = SKIN_FRAGMENTSHADER_SOURCE;
		assert!(fragment_source.contains("vec4 irisSample = texture(layerColorTexture, irisTextureCoordinate);"));
		assert!(fragment_source.contains("vec3 irisNormal = texture(layerNormalTexture, irisTextureCoordinate).xyz * 2.0 - 1.0;"));
	}
}
