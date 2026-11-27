//==============================================================================
//! 툰 HDR 마무리. (vanilla.js graphics3d/toon/toonpostprocess.js 의 ToonPostProcess)
//! - 실수 렌더 타겟에 그린 선형 색을 블룸, 노출, 톤매핑, 감마로 화면에 낸다.
//! - 엔진의 RenderTarget(반정밀 실수 색, 깊이), BloomEffect(또는 MultiLevelBloomEffect), FullscreenPass 를 엮는다.
//! - 사용: ToonRenderer::set_hdr_output(true) 뒤
//!   begin(gl, 바탕 선형 색) → ToonRenderer 로 그리기 → end(gl, 낼 프레임버퍼, 뷰포트)
//!   end 는 (멀티샘플이면 풀고) 블룸을 셈하고 톤매핑한 색을 낼 프레임버퍼(None 이면 화면)에 그린다. 2D 와 섞으려면 그 뒤 Graphic::restore_render_state.
//! - 생성 옵션: new_with_options(gl, 너비, 높이, ToonPostProcessOptions { sample_count, use_float_color, depth_bits, use_depth_texture, color_space }) (원본 생성자의 options, new 는 기본 옵션으로 지금과 같은 RGBA16F, 깊이 16, 멀티샘플 없음)
//!   sample_count 가 2 이상이면 씬을 멀티샘플(MSAA) 렌더버퍼에 그리고 end 앞에서 텍스처로 푼다. 기기가 받는 수로 줄인다. (get_sample_count, set_sample_count)
//!   depth_bits 24 는 깊이 24 비트. (먼 면이 먼 장면) use_depth_texture 는 씬 깊이를 텍스처로 남긴다. (set_lit_area 가 쓴다)
//! - 색 공간: 씬 렌더 타겟에는 선형 색을 쓴다. (ToonRenderer::set_hdr_output(true), ToonScreenOutline::set_hdr_output(true), ToonVfx 색도 선형으로 본다)
//!   감마를 건 색 g 를 실수 판에 쓰고 합성에서 g^2.2 로 되돌리던 게임은 그 값을 미리 선형으로 바꿔 넘긴다. 색은 c^2.2, 감마 쪽에서 곱하던 밝기 배 k 는 선형 k^2.2 다.
//!   (예: 1.37 은 선형 2, 1.6 은 2.81, 2.08 은 5, 밝기 1.6 에 배 1.6 이면 (1.6 x 1.6)^2.2 = 7.9) 블룸 문턱과 무릎은 선형 밝기다.
//!   이 예는 감마 쪽(감마를 건 색 g)에서 곱하던 배일 때만 맞다. 선형 색에 곱하던 배와 블룸 문턱, 무릎처럼 처음부터 선형인 값은 바꾸지 않고 그대로 넘긴다.
//! - 실수 판을 못 쓰는 기기: use_float_color(기본 true)여도 is_float_color_supported 가 false 면 RGBA8 판으로 짓는다. (get_use_float_color 가 false)
//!   그때와 use_float_color: false 는 LDR 길이다. 씬에는 화면 색(감마를 건 색, ToonRenderer::set_hdr_output(false))을 그리고, 합성은 톤매핑과 감마 없이 블룸을 더해 색 보정만 건다.
//! - 블룸: 기본은 반 해상도 9 탭 가우시안(BloomEffect, 문턱 0.9 ~ 1.6, 주고받는 판 RGBA8, set_bloom_float_color 로 RGBA16F).
//!   set_bloom_levels(gl, 단 수, 문턱, 무릎) 로 단 수를 1 이상 주면 MultiLevelBloomEffect(반씩 줄였다 천막 꼴로 키우며 더함, 씬과 같은 정밀도의 판)로 바꾼다. 0 이면 기본으로 돌린다. 세기는 set_bloom 이 둘 다에 쓴다.
//! - 톤매핑은 ACES 근사(Krzysztof Narkowicz 2016, 원문이 CC0 또는 MIT 로 쓰게 함)다. 노출(set_exposure)을 곱한 뒤 매핑하고 sRGB 감마를 건다.
//! - 색 보정: 감마를 건 뒤 밝기(set_brightness, 밝힌 자리 set_lit_area 안은 1 쪽으로), 채도, 색조(밝기 1 로 맞춘 색을 곱함), 안개빛(어두운 곳을 그 색 쪽으로), 대비, 가장자리 어둡게 차례로 건다. (set_color_grade, set_vignette)
//!   기본은 밝기 1, 채도 1, 대비 1, 색조 0, 안개빛 0, 가장자리 0 이라 보정이 없다.
//! - 해상도 배율: set_resolution_scale(gl, 배율) 이면 씬 판을 출력 크기 x 배율로 짓고 end 의 뷰포트로 키워(선형 거르기) 낸다. (3D 만 낮춰 그리고 UI 는 기기 화소 그대로 둘 때, 기본 1)
//! - 노드 사각형 안에 낼 때: resize(사각형 너비, 높이) → begin → 그리기(ToonRenderer 의 화면 사각형은 풀어 둔다) → end(None, [x, 드로잉 버퍼 높이 - y - 높이, 너비, 높이]).
//! - 색 공간: 톤매핑 뒤 화면 색의 곡선은 감마 색 공간(기본)은 pow(1 / 2.2), 선형 색 공간(options.color_space 나 엔진의 ColorSpace::Linear)은 sRGB 표준 곡선(IEC 61966-2-1)이다. (ToonRenderer 의 LDR 출력과 같은 곡선)
//! - 모서리 어둠(화면 공간 AO): set_ambient_occlusion(켬, 세기, 반지름, 해상도 배율, 직접 빛 몫) 로 켠다. (기본 꺼짐, 세기 0.5, 반지름 0.5 월드 길이, 반 해상도, 직접 빛 몫 0)
//!   반 해상도 GTAO(Jimenez 2016) → 가로 흐림 → 세로 흐림과 온 해상도로 키우기, 세 패스다. 흐림은 깊이와 노멀로 가장자리를 지킨다.
//!   노멀과 깊이는 화면 바깥선(ToonScreenOutline)의 선의 판을 다시 쓴다. 바깥선이 없는 장면은 선은 긋지 않고 선의 판만 그리는 get_ambient_occlusion_prepass() 를 쓴다.
//!   ToonDrawQueue::set_post_process(이것) 이면 줄이 선의 판 뒤, 몸 앞에 draw_ambient_occlusion 을 부르고 렌더러에 결과를 건다. 몸 셰이더는 바탕빛(하늘과 땅 빛)에 곱하고, 직접 빛 몫만큼 해, 채움빛, 점광의 확산과 반사광에도 곱한다. (칸을 고른 뒤라 칸 경계는 그대로, 몫 0 이면 바탕빛에만)
//! - 원본과 다른 점: GL 객체를 다시 짓는 설정(set_sample_count, set_resolution_scale, set_bloom_levels, set_bloom_float_color)은 gl 을 받고 Result 를 돌려준다.
//==============================================================================
use crate::core::glcontext::GlContext;
use crate::core::gluniform::GlUniformF64;

use crate::base::matrix4::Matrix4;
use crate::core::fullscreenpass::FullscreenPass;
use crate::graphics3d::bloomeffect::BloomEffect;
use crate::graphics3d::multilevelbloomeffect::MultiLevelBloomEffect;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use crate::graphics3d::{read_framebuffer_binding, read_texture_binding, GlFramebuffer};
use crate::core::colorspace::{add_color_space_define, read_engine_color_space, ColorSpace};
use crate::core::graphic::GlTexture;
use crate::graphics3d::toon::toonscreenoutline::{ToonLineTarget, ToonOutlineWidthUnit, ToonScreenOutline, ToonScreenOutlineOptions};

/// 화면 공간 모서리 어둠(GTAO) 셰이더의 공용 머리. (선의 판의 노멀과 깊이를 읽는다)
/// - 깊이는 역투영 행렬로 보기 공간 자리로 되돌려 원근과 직교를 함께 받는다. 노멀은 선의 판의 팔면체 노멀(월드)을 보기 회전으로 돌린다.
/// - 반 해상도 칸 j 가 읽는 온 해상도 화소는 floor(j x 배 + 0.5) 다. (흐림과 키우기가 같은 화소를 다시 찾는다)
macro_rules! ambient_occlusion_common_chunk {
	() => {
		"#version 300 es
precision highp float;
uniform sampler2D lineTexture;
uniform sampler2D depthTexture;
uniform mat4 inverseProjectionMatrix;
uniform mat3 viewRotation;
uniform vec2 sourceSize;
uniform vec2 sourceScale;
out vec4 fragmentColor;
float readDepth(ivec2 pixel) {
	return texelFetch(depthTexture, pixel, 0).r;
}
vec3 readViewPosition(ivec2 pixel, float depth) {
	vec2 point = (vec2(pixel) + 0.5) / sourceSize;
	vec4 position = inverseProjectionMatrix * vec4(point * 2.0 - 1.0, depth * 2.0 - 1.0, 1.0);
	return position.xyz / position.w;
}
vec3 readViewNormal(ivec2 pixel) {
	vec2 folded = texelFetch(lineTexture, pixel, 0).rg * 2.0 - 1.0;
	vec3 normal = vec3(folded.x, 1.0 - abs(folded.x) - abs(folded.y), folded.y);
	float fold = max(-normal.y, 0.0);
	normal.x += normal.x >= 0.0 ? -fold : fold;
	normal.z += normal.z >= 0.0 ? -fold : fold;
	return normalize(viewRotation * normal);
}
ivec2 readSourcePixel(ivec2 cell) {
	return ivec2(floor(vec2(cell) * sourceScale + 0.5));
}
"
	};
}
/// 반 해상도 GTAO. (Jimenez, Wu, Pesce, Jarabo 2016 "Practical Realtime Strategies for Accurate Indirect Occlusion" 의 지평선 적분)
/// - 칸마다 화면 방향 조각 SLICE_COUNT 개를 돌려 양쪽으로 STEP_COUNT 걸음씩 가장 높은 지평선을 찾고, 투영한 노멀로 코사인 가중 보임을 닫힌 식으로 적분한다.
/// - 조각 각과 걸음 자리는 인터리브드 그래디언트 잡음(Jimenez 2014)으로 칸마다 흩는다. 프레임마다 바꾸지 않으니 멈춘 화면은 그대로다. (뒤의 흐림이 잡음을 지운다)
/// - 반지름 밖 표본은 0.6 반지름부터 반지름까지 줄여 가장 낮은 지평선 쪽으로 보낸다. (먼 벽이 가리지 않게)
/// - 보임을 0.96 으로 나눠 1 에서 자른다. (트인 바닥과 볼록한 면은 정확히 1 이라 화면이 그대로다)
const AMBIENT_OCCLUSION_FRAGMENT_SHADER: &str = concat!(ambient_occlusion_common_chunk!(), "
uniform vec4 projectionRow;
uniform float isPerspective;
uniform float radius;
uniform float pixelsPerUnit;
const int SLICE_COUNT = 4;
const int STEP_COUNT = 6;
const float PI = 3.14159265;
const float HALF_PI = 1.57079633;
// 트인 면의 보임이 이 값 이상이면 1 로 본다. (8 비트 팔면체 노멀과 깊이 정밀도, 조각 넷의 잡음으로 평평하거나 볼록한 면이 0.98 쯤으로 셈해지는 것을 지운다)
const float SELF_OCCLUSION_LIMIT = 0.96;
float readNoise(vec2 pixel) {
	return fract(52.9829189 * fract(dot(pixel, vec2(0.06711056, 0.00583715))));
}
float readArc(float horizon, float normalAngle) {
	return 0.25 * (-cos(2.0 * horizon - normalAngle) + cos(normalAngle) + 2.0 * horizon * sin(normalAngle));
}
void main() {
	ivec2 cell = ivec2(gl_FragCoord.xy);
	ivec2 pixel = readSourcePixel(cell);
	float depth = readDepth(pixel);
	if (depth >= 1.0) {
		fragmentColor = vec4(1.0);
		return;
	}
	vec3 position = readViewPosition(pixel, depth);
	vec3 normal = readViewNormal(pixel);
	vec3 viewVector = isPerspective > 0.5 ? normalize(-position) : vec3(0.0, 0.0, 1.0);
	float radiusPixels = min(radius * pixelsPerUnit / max(dot(projectionRow, vec4(position, 1.0)), 0.0001), sourceSize.y * 0.25);
	if (radiusPixels < 1.0) {
		fragmentColor = vec4(1.0);
		return;
	}
	float sliceNoise = readNoise(gl_FragCoord.xy);
	float stepNoise = readNoise(gl_FragCoord.yx + vec2(23.0, 61.0));
	float falloffScale = 1.0 / (radius * 0.4);
	ivec2 maximumPixel = ivec2(sourceSize) - 1;
	vec2 center = vec2(pixel);
	float visibility = 0.0;
	for (int sliceIndex = 0; sliceIndex < SLICE_COUNT; ++sliceIndex) {
		float angle = (float(sliceIndex) + sliceNoise) * (PI / float(SLICE_COUNT));
		vec2 direction = vec2(cos(angle), sin(angle));
		vec3 directionVector = vec3(direction, 0.0);
		vec3 orthoDirection = directionVector - dot(directionVector, viewVector) * viewVector;
		vec3 axis = normalize(cross(directionVector, viewVector));
		vec3 projectedNormal = normal - axis * dot(normal, axis);
		float projectedLength = length(projectedNormal);
		float cosineNormal = clamp(dot(projectedNormal, viewVector) / max(projectedLength, 0.0001), -1.0, 1.0);
		float normalAngle = (dot(orthoDirection, projectedNormal) >= 0.0 ? 1.0 : -1.0) * acos(cosineNormal);
		float lowCosine0 = cos(normalAngle + HALF_PI);
		float lowCosine1 = cos(normalAngle - HALF_PI);
		float horizonCosine0 = lowCosine0;
		float horizonCosine1 = lowCosine1;
		for (int stepIndex = 0; stepIndex < STEP_COUNT; ++stepIndex) {
			float fraction = (float(stepIndex) + stepNoise) / float(STEP_COUNT);
			vec2 offset = direction * max(fraction * fraction * radiusPixels, float(stepIndex) + 1.0);
			ivec2 samplePixel0 = ivec2(floor(center + offset + 0.5));
			ivec2 samplePixel1 = ivec2(floor(center - offset + 0.5));
			if (all(greaterThanEqual(samplePixel0, ivec2(0))) && all(lessThanEqual(samplePixel0, maximumPixel))) {
				vec3 delta = readViewPosition(samplePixel0, readDepth(samplePixel0)) - position;
				float deltaLength = length(delta);
				float weight = clamp((radius - deltaLength) * falloffScale, 0.0, 1.0);
				horizonCosine0 = max(horizonCosine0, mix(lowCosine0, dot(delta, viewVector) / max(deltaLength, 0.0001), weight));
			}
			if (all(greaterThanEqual(samplePixel1, ivec2(0))) && all(lessThanEqual(samplePixel1, maximumPixel))) {
				vec3 delta = readViewPosition(samplePixel1, readDepth(samplePixel1)) - position;
				float deltaLength = length(delta);
				float weight = clamp((radius - deltaLength) * falloffScale, 0.0, 1.0);
				horizonCosine1 = max(horizonCosine1, mix(lowCosine1, dot(delta, viewVector) / max(deltaLength, 0.0001), weight));
			}
		}
		float horizon0 = normalAngle + min(acos(clamp(horizonCosine0, -1.0, 1.0)) - normalAngle, HALF_PI);
		float horizon1 = normalAngle + max(-acos(clamp(horizonCosine1, -1.0, 1.0)) - normalAngle, -HALF_PI);
		visibility += projectedLength * (readArc(horizon0, normalAngle) + readArc(horizon1, normalAngle));
	}
	fragmentColor = vec4(clamp(visibility / (float(SLICE_COUNT) * SELF_OCCLUSION_LIMIT), 0.0, 1.0), 0.0, 0.0, 1.0);
}
");
/// 가장자리 지키는 흐림의 무게. (탭 자리가 가운데의 접평면에서 떨어진 거리와 두 노멀의 각으로, Kopf 2007 joint bilateral 꼴)
macro_rules! ambient_occlusion_weight_chunk {
	() => {
		"
uniform float planeTolerance;
uniform sampler2D occlusionTexture;
uniform vec2 occlusionSize;
float readTapWeight(vec3 position, vec3 normal, ivec2 cell, out float occlusion) {
	ivec2 clampedCell = clamp(cell, ivec2(0), ivec2(occlusionSize) - 1);
	occlusion = texelFetch(occlusionTexture, clampedCell, 0).r;
	ivec2 pixel = readSourcePixel(clampedCell);
	float depth = readDepth(pixel);
	if (depth >= 1.0) {
		return 0.0;
	}
	vec3 tapPosition = readViewPosition(pixel, depth);
	float planeDistance = abs(dot(tapPosition - position, normal)) / planeTolerance;
	float normalWeight = pow(max(dot(readViewNormal(pixel), normal), 0.0), 8.0);
	return exp(-planeDistance * planeDistance) * normalWeight;
}
"
	};
}
/// 가로 흐림. (반 해상도, 가우스 7 탭)
const AMBIENT_OCCLUSION_BLUR_FRAGMENT_SHADER: &str = concat!(ambient_occlusion_common_chunk!(), ambient_occlusion_weight_chunk!(), "
const float GAUSSIAN_WEIGHTS[4] = float[4](1.0, 0.8, 0.45, 0.2);
void main() {
	ivec2 cell = ivec2(gl_FragCoord.xy);
	ivec2 pixel = readSourcePixel(cell);
	float depth = readDepth(pixel);
	if (depth >= 1.0) {
		fragmentColor = vec4(1.0);
		return;
	}
	vec3 position = readViewPosition(pixel, depth);
	vec3 normal = readViewNormal(pixel);
	float occlusionSum = 0.0;
	float weightSum = 0.0;
	for (int offset = -3; offset <= 3; ++offset) {
		float occlusion;
		float weight = GAUSSIAN_WEIGHTS[abs(offset)] * readTapWeight(position, normal, cell + ivec2(offset, 0), occlusion);
		occlusionSum += occlusion * weight;
		weightSum += weight;
	}
	fragmentColor = vec4(weightSum > 0.0001 ? occlusionSum / weightSum : 1.0, 0.0, 0.0, 1.0);
}
");
/// 세로 흐림과 온 해상도로 키우기. (화소에서 가까운 반 해상도 두 줄 x 7 탭을 화소의 깊이와 노멀로 고른다, 세기를 걸고 깊이 24 비트를 gba 에)
/// - 몸 셰이더가 제 깊이와 견주어 같은 면일 때만 읽는다. (비치는 것, 선의 판에 없는 것은 어둡히지 않는다)
const AMBIENT_OCCLUSION_UPSAMPLE_FRAGMENT_SHADER: &str = concat!(ambient_occlusion_common_chunk!(), ambient_occlusion_weight_chunk!(), "
uniform float intensity;
const float GAUSSIAN_WEIGHTS[4] = float[4](1.0, 0.8, 0.45, 0.2);
vec3 packDepth(float depth) {
	float value = floor(depth * 16777215.0 + 0.5);
	float high = floor(value / 65536.0);
	float middle = floor((value - high * 65536.0) / 256.0);
	return vec3(high, middle, value - high * 65536.0 - middle * 256.0) / 255.0;
}
void main() {
	ivec2 pixel = ivec2(gl_FragCoord.xy);
	float depth = readDepth(pixel);
	if (depth >= 1.0) {
		fragmentColor = vec4(1.0);
		return;
	}
	vec3 position = readViewPosition(pixel, depth);
	vec3 normal = readViewNormal(pixel);
	vec2 point = (gl_FragCoord.xy - 0.5) / sourceScale;
	ivec2 nearestCell = ivec2(floor(point + 0.5));
	int sideX = point.x >= float(nearestCell.x) ? 1 : -1;
	float occlusionSum = 0.0;
	float weightSum = 0.0;
	for (int column = 0; column < 2; ++column) {
		int cellX = nearestCell.x + column * sideX;
		float columnWeight = column == 0 ? 1.0 : 0.5;
		for (int offset = -3; offset <= 3; ++offset) {
			float occlusion;
			float weight = columnWeight * GAUSSIAN_WEIGHTS[abs(offset)] * readTapWeight(position, normal, ivec2(cellX, nearestCell.y + offset), occlusion);
			occlusionSum += occlusion * weight;
			weightSum += weight;
		}
	}
	float visibility = weightSum > 0.0001 ? occlusionSum / weightSum : 1.0;
	fragmentColor = vec4(1.0 - intensity * (1.0 - visibility), packDepth(depth));
}
");

/// 합성 프래그먼트 셰이더. (씬에 블룸을 더해 HDR 길은 노출, 톤매핑, 감마를 걸고 LDR 길은 그대로 자른 뒤, 밝기(밝힌 자리 안은 1 쪽으로), 채도, 색조, 안개빛, 대비, 가장자리 어둡게 차례로 건다)
const COMPOSITE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform sampler2D depthTexture;
uniform float bloomStrength;
uniform float exposure;
uniform float hdrInput;
uniform float brightness;
uniform vec4 litCenter;
uniform float litInnerRadius;
uniform mat4 inverseViewProjectionMatrix;
uniform float saturation;
uniform float contrast;
uniform vec3 toneColor;
uniform float toneAmount;
uniform vec3 hazeColor;
uniform float hazeAmount;
uniform float vignetteStrength;
out vec4 fragmentColor;
vec3 mapTone(vec3 color) {
	color *= exposure;
	return clamp((color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14), 0.0, 1.0);
}
#ifdef VANILLA_COLOR_SPACE_LINEAR
vec3 toOutput(vec3 color) {
	return mix(color * 12.92, 1.055 * pow(color, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), color));
}
#endif
void main() {
	vec3 scene = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	vec3 bloom = texture(bloomTexture, fragmentTextureCoordinate).rgb;
#ifdef VANILLA_COLOR_SPACE_LINEAR
	vec3 color = hdrInput > 0.5 ? toOutput(mapTone(scene + bloom * bloomStrength)) : clamp(scene + bloom * bloomStrength, 0.0, 1.0);
#else
	vec3 color = hdrInput > 0.5 ? pow(mapTone(scene + bloom * bloomStrength), vec3(1.0 / 2.2)) : clamp(scene + bloom * bloomStrength, 0.0, 1.0);
#endif
	float localBrightness = brightness;
	if (litCenter.w > 0.0) {
		float depth = texture(depthTexture, fragmentTextureCoordinate).r;
		vec4 worldPoint = inverseViewProjectionMatrix * vec4(fragmentTextureCoordinate * 2.0 - 1.0, depth * 2.0 - 1.0, 1.0);
		float litDistance = length(worldPoint.xz / worldPoint.w - litCenter.xz);
		localBrightness = mix(brightness, 1.0, 1.0 - smoothstep(litInnerRadius, litCenter.w, litDistance));
	}
	color *= localBrightness;
	float luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
	color = mix(vec3(luminance), color, saturation);
	color = mix(color, color * toneColor, toneAmount);
	color = mix(color, hazeColor, hazeAmount * (1.0 - luminance));
	color = (color - 0.5) * contrast + 0.5;
	vec2 centered = fragmentTextureCoordinate * 2.0 - 1.0;
	color *= 1.0 - vignetteStrength * smoothstep(0.45, 1.42, length(centered));
	fragmentColor = vec4(clamp(color, 0.0, 1.0), 1.0);
}
";

//==============================================================================
/// 생성 옵션. (원본 생성자의 options, 기본값은 멀티샘플 없음 / 실수 판 / 깊이 16 / 깊이 텍스처 없음)
///
/// # 예제
/// 멀티샘플 넷에 깊이 24 비트 옵션
/// ```rust
/// use vanilla::ToonPostProcessOptions;
///
/// let options = ToonPostProcessOptions { sample_count: 4, depth_bits: 24, ..ToonPostProcessOptions::default() };
/// assert!(options.use_float_color);
/// assert!(!options.use_depth_texture);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToonPostProcessOptions {
	/// 멀티샘플 표본 수. (2 밑이면 없음)
	pub sample_count: i32,
	/// 실수 판을 쓸지. (기기가 못 쓰면 false 로 짓는다)
	pub use_float_color: bool,
	/// 깊이 렌더버퍼 비트 수. (16 이나 24)
	pub depth_bits: i32,
	/// 씬 깊이를 텍스처로 남길지. (set_lit_area)
	pub use_depth_texture: bool,
	/// 색 공간. (원본 colorSpace, 기본은 엔진의 색 공간, ToonRenderer 와 같은 값)
	pub color_space: ColorSpace,
}

impl Default for ToonPostProcessOptions {
	//==============================================================================
	/// 기본 옵션. (멀티샘플 없음, 실수 판, 깊이 16, 깊이 텍스처 없음, 엔진의 색 공간)
	//==============================================================================
	fn default() -> ToonPostProcessOptions {
		ToonPostProcessOptions { sample_count: 0, use_float_color: true, depth_bits: 16, use_depth_texture: false, color_space: read_engine_color_space() }
	}
}

//==============================================================================
/// 밝힌 자리. (가운데, 안쪽과 바깥 반지름, 보기와 투영의 역행렬)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct LitArea {
	center: [f64; 3],
	inner_radius: f64,
	outer_radius: f64,
	inverse_view_projection_matrix: Matrix4,
}

//==============================================================================
/// 툰 HDR 마무리. (머리말 참고)
///
/// # 예제
/// 툰 장면을 실수 판에 그리고 블룸과 톤매핑으로 내기
/// ```rust,no_run
/// use vanilla::{create_sphere_geometry, Camera3D, Matrix4, MeshBuilder, ToonDrawOptions, ToonPartOptions, ToonPostProcess, ToonPostProcessOptions, ToonRenderer, Vector3};
///
/// fn draw_glowing_ball(gl: &vanilla::GlContext, width: i32, height: i32) -> Result<(), String> {
/// 	let mut post_process = ToonPostProcess::new_with_options(gl, width, height, ToonPostProcessOptions { sample_count: 4, ..ToonPostProcessOptions::default() })?;
/// 	post_process.set_exposure(1.2);
/// 	post_process.set_bloom(true, 0.8, 0.9, 1.6);
/// 	let mut renderer = ToonRenderer::new(gl)?;
/// 	renderer.set_hdr_output(post_process.get_use_float_color());
/// 	let mut builder = MeshBuilder::new();
/// 	builder.add_part(&create_sphere_geometry(0.5, 32, 20, std::f64::consts::PI), &Matrix4::create_identity(), "#ffd080", ToonPartOptions { glow: 2.0, ..ToonPartOptions::default() });
/// 	let mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("mesh failed")?;
/// 	let mut camera = Camera3D::new();
/// 	camera.set_perspective(0.8, 0.1, 100.0);
/// 	camera.set_aspect_ratio(width as f64 / height as f64);
/// 	camera.look_at(Vector3::create(0.0, 1.0, 3.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	post_process.begin(gl, [0.02, 0.02, 0.04]);
/// 	renderer.begin_frame_with_camera(gl, &camera);
/// 	renderer.draw_mesh(gl, &mesh, &Matrix4::create_identity(), ToonDrawOptions::default());
/// 	renderer.end_frame(gl);
/// 	post_process.end(gl, None, [0, 0, width, height]);
/// 	Ok(())
/// }
/// ```
//==============================================================================
pub struct ToonPostProcess {
	/// 씬을 그리는 렌더 타겟. (출력 크기 x 해상도 배율, 멀티샘플이면 end 앞에서 푼다)
	scene_render_target: RenderTarget,
	bloom_effect: BloomEffect,
	/// 단 수를 주면 쓰는 다단 블룸.
	multi_level_bloom_effect: Option<MultiLevelBloomEffect>,
	composite_pass: FullscreenPass,
	/// 씬 판이 실수인지. (HDR 길)
	use_float_color: bool,
	/// 요청한 멀티샘플 표본 수. (실제 수는 씬 판의 get_sample_count)
	requested_sample_count: i32,
	depth_bits: i32,
	use_depth_texture: bool,
	/// resize 로 받은 출력 크기.
	output_width: i32,
	output_height: i32,
	resolution_scale: f64,
	exposure: f64,
	bloom_strength: f64,
	is_bloom_enabled: bool,
	/// 다단 블룸의 문턱과 무릎.
	bloom_threshold: f64,
	bloom_knee: f64,
	brightness: f64,
	/// 밝힌 자리. (깊이 텍스처가 있을 때만 쓴다)
	lit_area: Option<LitArea>,
	saturation: f64,
	contrast: f64,
	/// 밝기 1 로 맞춘 색조 곱.
	tone_color: [f64; 3],
	tone_amount: f64,
	haze_color: [f64; 3],
	haze_amount: f64,
	vignette_strength: f64,
	/// 모서리 어둠 설정. (원본 #ambientOcclusion)
	ambient_occlusion: ToonAmbientOcclusion,
	/// 모서리 어둠의 세 패스. (모으기, 가로 흐림, 세로 흐림과 키우기, 처음 셀 때 짓는다)
	ambient_occlusion_passes: Option<[FullscreenPass; 3]>,
	/// 모서리 어둠 판. (반 해상도 둘, 온 해상도 하나)
	ambient_occlusion_target: Option<AmbientOcclusionTarget>,
	/// 바깥선이 없는 장면의 노멀과 깊이 미리 그리기. (선은 긋지 않는다)
	ambient_occlusion_prepass: Option<ToonScreenOutline>,
	/// 색 공간. (원본 #colorSpace, 생성 때 정함, 톤매핑 뒤 화면 색의 곡선, Gamma 는 pow 1 / 2.2, Linear 는 sRGB 표준 곡선)
	color_space: ColorSpace,
}

//==============================================================================
/// 모서리 어둠 설정. (원본 getAmbientOcclusion 의 { isEnabled, intensity, radius, resolutionScale, directLightingStrength })
///
/// # 예제
/// 기본값 읽기
/// ```rust
/// use vanilla::ToonAmbientOcclusion;
///
/// let ambient_occlusion = ToonAmbientOcclusion::default();
/// assert!(!ambient_occlusion.is_enabled);
/// assert_eq!((ambient_occlusion.intensity, ambient_occlusion.radius, ambient_occlusion.resolution_scale, ambient_occlusion.direct_lighting_strength), (0.5, 0.5, 0.5, 0.0));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonAmbientOcclusion {
	/// 켰는지.
	pub is_enabled: bool,
	/// 세기. (0 ~ 1, 0 이면 어둡히지 않는다)
	pub intensity: f64,
	/// 가리는 것을 찾는 반지름. (월드 길이)
	pub radius: f64,
	/// 셈하는 판의 해상도 배율. (0.25 ~ 1, 선의 판 크기에 곱한다)
	pub resolution_scale: f64,
	/// 직접 빛에 미치는 몫. (0 ~ 1, URP 의 Direct Lighting Strength)
	pub direct_lighting_strength: f64,
}

impl Default for ToonAmbientOcclusion {
	//==============================================================================
	/// 기본값. (꺼짐, 세기 0.5, 반지름 0.5, 반 해상도, 직접 빛 몫 0)
	//==============================================================================
	fn default() -> Self {
		ToonAmbientOcclusion { is_enabled: false, intensity: 0.5, radius: 0.5, resolution_scale: 0.5, direct_lighting_strength: 0.0 }
	}
}

//==============================================================================
/// 모서리 어둠 판. (원본 #ambientOcclusionTarget)
//==============================================================================
struct AmbientOcclusionTarget {
	occlusion_texture: GlTexture,
	occlusion_framebuffer: GlFramebuffer,
	blur_texture: GlTexture,
	blur_framebuffer: GlFramebuffer,
	result_texture: GlTexture,
	result_framebuffer: GlFramebuffer,
	width: i32,
	height: i32,
	cell_width: i32,
	cell_height: i32,
}

/// 밝기의 무게. (Rec. 709)
const LUMINANCE_WEIGHTS: [f64; 3] = [0.2126, 0.7152, 0.0722];

impl ToonPostProcess {
	//==============================================================================
	/// 생성. (화면 픽셀 크기, 기본 옵션)
	///
	/// # 인자
	/// - `gl`: 툰 렌더러와 같은 컨텍스트.
	/// - `width`: 출력 너비. (픽셀)
	/// - `height`: 출력 높이. (픽셀)
	///
	/// # 돌려줌
	/// 새 툰 HDR 마무리. 씬 렌더 타겟, 블룸, 합성 셰이더를 못 지으면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &GlContext, width: i32, height: i32) -> Result<ToonPostProcess, String> {
		ToonPostProcess::new_with_options(gl, width, height, ToonPostProcessOptions::default())
	}

	//==============================================================================
	/// 옵션을 주어 생성. (원본 new ToonPostProcess(gl, 너비, 높이, options))
	///
	/// # 인자
	/// - `gl`: 툰 렌더러와 같은 컨텍스트.
	/// - `width`: 출력 너비. (픽셀)
	/// - `height`: 출력 높이. (픽셀)
	/// - `options`: 생성 옵션. (멀티샘플 표본 수, 실수 판, 깊이 비트 16 이나 24, 깊이 텍스처, 색 공간)
	///
	/// # 돌려줌
	/// 새 툰 HDR 마무리. 씬 렌더 타겟, 블룸, 합성 셰이더를 못 지으면 Err(오류 글).
	//==============================================================================
	pub fn new_with_options(gl: &GlContext, width: i32, height: i32, options: ToonPostProcessOptions) -> Result<ToonPostProcess, String> {
		let use_float_color = options.use_float_color && ToonPostProcess::is_float_color_supported(gl);
		let depth_bits = if options.depth_bits == 24 {
			24
		}
		else {
			16
		};
		let scene_options = RenderTargetOptions { color_attachment_count: 1, use_float_color, use_depth_texture: options.use_depth_texture, sample_count: options.sample_count.max(0), depth_bits };
		let scene_render_target = RenderTarget::new(gl, width.max(1), height.max(1), true, Some(scene_options))?;
		let mut bloom_effect = BloomEffect::new(gl)?;
		bloom_effect.resize(gl, width.max(1), height.max(1))?;
		bloom_effect.set_threshold(0.9, 1.6);
		let composite_pass = FullscreenPass::new(gl, &add_color_space_define(COMPOSITE_FRAGMENT_SHADER, options.color_space))?;
		Ok(ToonPostProcess {
			scene_render_target,
			bloom_effect,
			multi_level_bloom_effect: None,
			composite_pass,
			use_float_color,
			requested_sample_count: options.sample_count.max(0),
			depth_bits,
			use_depth_texture: options.use_depth_texture,
			output_width: width.max(1),
			output_height: height.max(1),
			resolution_scale: 1.0,
			exposure: 1.0,
			bloom_strength: 0.6,
			is_bloom_enabled: true,
			bloom_threshold: 1.0,
			bloom_knee: 0.25,
			brightness: 1.0,
			lit_area: None,
			saturation: 1.0,
			contrast: 1.0,
			tone_color: [1.0; 3],
			tone_amount: 0.0,
			haze_color: [0.0; 3],
			haze_amount: 0.0,
			vignette_strength: 0.0,
			ambient_occlusion: ToonAmbientOcclusion::default(),
			ambient_occlusion_passes: None,
			ambient_occlusion_target: None,
			ambient_occlusion_prepass: None,
			color_space: options.color_space,
		})
	}

	//==============================================================================
	/// 실수 컬러 판을 그릴 수 있는지. (웹은 EXT_color_buffer_float 나 EXT_color_buffer_half_float 가 있어야, 1 x 1 RGBA16F 를 붙인 프레임버퍼가 완성되는지)
	///
	/// # 인자
	/// - `gl`: 알아볼 컨텍스트.
	///
	/// # 돌려줌
	/// RGBA16F 판에 그릴 수 있으면 true.
	//==============================================================================
	pub fn is_float_color_supported(gl: &GlContext) -> bool {
		#[cfg(target_arch = "wasm32")]
		{
			let supported_extensions = gl.supported_extensions();
			if !supported_extensions.contains("EXT_color_buffer_float") && !supported_extensions.contains("EXT_color_buffer_half_float") {
				return false;
			}
		}
		let previous_framebuffer = read_framebuffer_binding(gl);
		let previous_texture = read_texture_binding(gl);
		unsafe {
			let Ok(test_texture) = crate::core::glstatecache::create_texture(gl) else {
				return false;
			};
			let Ok(test_framebuffer) = crate::core::glstatecache::create_framebuffer(gl) else {
				crate::core::glstatecache::delete_texture(gl, test_texture);
				return false;
			};
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(test_texture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, 1, 1, 0, glow::RGBA, glow::HALF_FLOAT, glow::PixelUnpackData::Slice(None));
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(test_framebuffer));
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(test_texture), 0);
			let is_complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, previous_framebuffer);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, previous_texture);
			crate::core::glstatecache::delete_framebuffer(gl, test_framebuffer);
			crate::core::glstatecache::delete_texture(gl, test_texture);
			is_complete
		}
	}

	//==============================================================================
	/// 씬 렌더 타겟 짓기. (출력 크기 x 해상도 배율, 지금 옵션으로)
	//==============================================================================
	fn create_scene_render_target(&self, gl: &GlContext) -> Result<RenderTarget, String> {
		let options = RenderTargetOptions { color_attachment_count: 1, use_float_color: self.use_float_color, use_depth_texture: self.use_depth_texture, sample_count: self.requested_sample_count, depth_bits: self.depth_bits };
		RenderTarget::new(gl, self.read_scene_width(), self.read_scene_height(), true, Some(options))
	}

	//==============================================================================
	/// 씬 판의 너비. (출력 너비 x 해상도 배율, 1 이상)
	//==============================================================================
	fn read_scene_width(&self) -> i32 {
		((self.output_width as f64 * self.resolution_scale).round() as i32).max(1)
	}

	//==============================================================================
	/// 씬 판의 높이. (출력 높이 x 해상도 배율, 1 이상)
	//==============================================================================
	fn read_scene_height(&self) -> i32 {
		((self.output_height as f64 * self.resolution_scale).round() as i32).max(1)
	}

	//==============================================================================
	/// 크기 바꾸기. (출력 크기, 씬 판은 해상도 배율을 곱함, 같으면 그대로)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `width`: 출력 너비. (픽셀)
	/// - `height`: 출력 높이. (픽셀)
	///
	/// # 돌려줌
	/// 판을 맞췄으면 Ok. 다시 짓다 GL 객체를 못 만들면 Err(GL 의 오류 글).
	//==============================================================================
	pub fn resize(&mut self, gl: &GlContext, width: i32, height: i32) -> Result<(), String> {
		self.output_width = width.max(1);
		self.output_height = height.max(1);
		let scene_width = self.read_scene_width();
		let scene_height = self.read_scene_height();
		if self.scene_render_target.get_width() == scene_width && self.scene_render_target.get_height() == scene_height {
			return Ok(());
		}
		self.scene_render_target.resize(gl, scene_width, scene_height)?;
		self.bloom_effect.resize(gl, scene_width, scene_height)?;
		if let Some(multi_level_bloom_effect) = self.multi_level_bloom_effect.as_mut() {
			multi_level_bloom_effect.resize(gl, scene_width, scene_height)?;
		}
		Ok(())
	}

	//==============================================================================
	/// 해상도 배율 설정. (씬 판 = 출력 크기 x 배율, 0.05 ~ 4, 기본 1)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `resolution_scale`: 씬 판의 배율. (0.05 ~ 4)
	///
	/// # 돌려줌
	/// 판을 맞췄으면 Ok. 다시 짓다 GL 객체를 못 만들면 Err(GL 의 오류 글).
	//==============================================================================
	pub fn set_resolution_scale(&mut self, gl: &GlContext, resolution_scale: f64) -> Result<(), String> {
		self.resolution_scale = resolution_scale.clamp(0.05, 4.0);
		self.resize(gl, self.output_width, self.output_height)
	}

	//==============================================================================
	/// 해상도 배율 반환.
	///
	/// # 돌려줌
	/// 씬 판의 배율.
	//==============================================================================
	pub fn get_resolution_scale(&self) -> f64 {
		self.resolution_scale
	}

	//==============================================================================
	/// 멀티샘플 표본 수 설정. (2 밑이면 없음, 바뀌면 씬 판을 다시 짓는다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `sample_count`: 멀티샘플 표본 수. (2 밑이면 멀티샘플 없음)
	///
	/// # 돌려줌
	/// 같은 수거나 씬 판을 다시 지었으면 Ok. 새 판을 못 지으면 Err(GL 의 오류 글, 앞의 판은 그대로 둔다).
	//==============================================================================
	pub fn set_sample_count(&mut self, gl: &GlContext, sample_count: i32) -> Result<(), String> {
		let safe_sample_count = sample_count.max(0);
		if safe_sample_count == self.requested_sample_count {
			return Ok(());
		}
		self.requested_sample_count = safe_sample_count;
		let scene_render_target = self.create_scene_render_target(gl)?;
		self.scene_render_target.destroy(gl);
		self.scene_render_target = scene_render_target;
		Ok(())
	}

	//==============================================================================
	/// 실제로 쓰는 멀티샘플 표본 수 반환. (0 이면 없음)
	///
	/// # 돌려줌
	/// 씬 판이 실제로 쓰는 표본 수. (기기가 받는 수로 줄인 것, 0 이면 없음)
	//==============================================================================
	pub fn get_sample_count(&self) -> i32 {
		self.scene_render_target.get_sample_count()
	}

	//==============================================================================
	/// 씬 판이 실수인지 반환. (false 면 LDR 길)
	///
	/// # 돌려줌
	/// 씬 판이 실수면 true. (ToonRenderer::set_hdr_output 에 그대로 넘긴다)
	//==============================================================================
	pub fn get_use_float_color(&self) -> bool {
		self.use_float_color
	}

	//==============================================================================
	/// 씬 그리기 시작. (씬 렌더 타겟을 묶고 바탕 색과 깊이로 비운다, HDR 길은 선형 색, LDR 길은 화면 색)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `clear_color`: [r, g, b] 바탕 색. (HDR 길은 선형 색, LDR 길은 화면 색)
	//==============================================================================
	pub fn begin(&self, gl: &GlContext, clear_color: [f64; 3]) {
		self.scene_render_target.bind(gl);
		unsafe {
			crate::core::glstatecache::viewport(gl, 0, 0, self.scene_render_target.get_width(), self.scene_render_target.get_height());
			crate::core::glstatecache::depth_mask(gl, true);
			crate::core::glstatecache::clear_color(gl, (clear_color[0]) as f32, (clear_color[1]) as f32, (clear_color[2]) as f32, 1.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
	}

	//==============================================================================
	/// 마무리. (멀티샘플이면 풂 → 블룸 → 톤매핑해 낼 프레임버퍼에, viewport 는 [x, y, 너비, 높이])
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `output_framebuffer`: 낼 프레임버퍼. (None 이면 화면)
	/// - `viewport`: [x, y, 너비, 높이] 낼 자리. (드로잉 버퍼 픽셀, 왼쪽 아래가 원점)
	//==============================================================================
	pub fn end(&self, gl: &GlContext, output_framebuffer: Option<GlFramebuffer>, viewport: [i32; 4]) {
		self.scene_render_target.resolve(gl);
		let scene_texture = self.scene_render_target.get_color_texture(0);
		let mut bloom_texture = scene_texture;
		if self.is_bloom_enabled {
			if let Some(multi_level_bloom_effect) = self.multi_level_bloom_effect.as_ref() {
				multi_level_bloom_effect.render(gl, scene_texture);
				bloom_texture = multi_level_bloom_effect.get_result_texture();
			}
			else {
				self.bloom_effect.render(gl, scene_texture);
				bloom_texture = self.bloom_effect.get_result_texture();
			}
		}
		unsafe {
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, output_framebuffer);
			crate::core::glstatecache::viewport(gl, viewport[0], viewport[1], viewport[2], viewport[3]);
			crate::core::glstatecache::disable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::disable(gl, glow::BLEND);
		}
		let composite_pass = &self.composite_pass;
		composite_pass.use_program(gl);
		let depth_texture = self.scene_render_target.get_depth_texture();
		let applied_lit_area = if depth_texture.is_some() {
			self.lit_area
		}
		else {
			None
		};
		unsafe {
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, scene_texture);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE1);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, bloom_texture);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			// 깊이 칸. (밝힌 자리를 쓰지 않으면 읽지 않지만 빈 칸이면 드라이버가 알리므로 씬 텍스처를 묶어 둔다)
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE2);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, if applied_lit_area.is_some() { depth_texture } else { scene_texture });
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "depthTexture").as_ref(), 2);
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), if self.is_bloom_enabled { self.bloom_strength } else { 0.0 });
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "exposure").as_ref(), self.exposure);
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "hdrInput").as_ref(), if self.use_float_color { 1.0 } else { 0.0 });
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "brightness").as_ref(), self.brightness);
			if let Some(lit_area) = applied_lit_area {
				let center = lit_area.center;
				gl.uniform_4_f64(composite_pass.get_uniform_location(gl, "litCenter").as_ref(), center[0], center[1], center[2], lit_area.outer_radius);
				gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "litInnerRadius").as_ref(), lit_area.inner_radius);
				gl.uniform_matrix_4_f32_slice(composite_pass.get_uniform_location(gl, "inverseViewProjectionMatrix").as_ref(), false, &lit_area.inverse_view_projection_matrix.elements);
			}
			else {
				gl.uniform_4_f64(composite_pass.get_uniform_location(gl, "litCenter").as_ref(), 0.0, 0.0, 0.0, 0.0);
			}
			let tone_color = self.tone_color;
			let haze_color = self.haze_color;
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "saturation").as_ref(), self.saturation);
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "contrast").as_ref(), self.contrast);
			gl.uniform_3_f64(composite_pass.get_uniform_location(gl, "toneColor").as_ref(), tone_color[0], tone_color[1], tone_color[2]);
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "toneAmount").as_ref(), self.tone_amount);
			gl.uniform_3_f64(composite_pass.get_uniform_location(gl, "hazeColor").as_ref(), haze_color[0], haze_color[1], haze_color[2]);
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "hazeAmount").as_ref(), self.haze_amount);
			gl.uniform_1_f64(composite_pass.get_uniform_location(gl, "vignetteStrength").as_ref(), self.vignette_strength);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
		}
		composite_pass.draw(gl);
		unsafe {
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE2);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
		}
	}

	//==============================================================================
	/// 노출 설정. (톤매핑 전에 곱한다, 1 이 기본)
	///
	/// # 인자
	/// - `exposure`: 노출 배. (0 이상, 1 이 기본)
	//==============================================================================
	pub fn set_exposure(&mut self, exposure: f64) {
		self.exposure = exposure.max(0.0);
	}

	//==============================================================================
	/// 블룸 설정. (켜기, 세기, 밝은 곳을 뽑는 문턱의 시작과 끝, 문턱은 기본 9 탭 블룸에만 쓴다)
	///
	/// # 인자
	/// - `is_bloom_enabled`: 블룸을 켤지.
	/// - `bloom_strength`: 블룸 세기. (씬 색에 더하는 배, 기본 0.6)
	/// - `threshold_start`: 밝은 곳을 뽑기 시작하는 선형 밝기. (기본 0.9)
	/// - `threshold_end`: 다 뽑는 선형 밝기. (기본 1.6)
	//==============================================================================
	pub fn set_bloom(&mut self, is_bloom_enabled: bool, bloom_strength: f64, threshold_start: f64, threshold_end: f64) {
		self.is_bloom_enabled = is_bloom_enabled;
		self.bloom_strength = bloom_strength.max(0.0);
		self.bloom_effect.set_threshold(threshold_start, threshold_end);
	}

	//==============================================================================
	/// 블룸 단 수 설정. (0 이면 기본 9 탭 블룸, 1 이상이면 그 단 수의 다단 블룸, 문턱과 무릎은 다단 블룸의 선형 밝기)
	/// - 다단 블룸 판은 씬 판과 같은 정밀도다. (HDR 길은 RGBA16F)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `level_count`: 단 수. (0 이면 기본 9 탭 블룸으로 돌린다)
	/// - `threshold`: 다단 블룸의 문턱. (선형 밝기)
	/// - `knee`: 문턱 둘레를 부드럽게 넘기는 폭. (선형 밝기)
	///
	/// # 돌려줌
	/// 바꿨으면 Ok. 다단 블룸의 셰이더나 판을 못 지으면 Err(오류 글).
	//==============================================================================
	pub fn set_bloom_levels(&mut self, gl: &GlContext, level_count: usize, threshold: f64, knee: f64) -> Result<(), String> {
		self.bloom_threshold = threshold.max(0.0);
		self.bloom_knee = knee.max(0.0);
		if level_count == 0 {
			if let Some(mut multi_level_bloom_effect) = self.multi_level_bloom_effect.take() {
				multi_level_bloom_effect.destroy(gl);
			}
			return Ok(());
		}
		if self.multi_level_bloom_effect.is_none() {
			let mut multi_level_bloom_effect = MultiLevelBloomEffect::new(gl, level_count, self.use_float_color)?;
			multi_level_bloom_effect.resize(gl, self.read_scene_width(), self.read_scene_height())?;
			self.multi_level_bloom_effect = Some(multi_level_bloom_effect);
		}
		if let Some(multi_level_bloom_effect) = self.multi_level_bloom_effect.as_mut() {
			multi_level_bloom_effect.set_level_count(gl, level_count)?;
			multi_level_bloom_effect.set_threshold(self.bloom_threshold, self.bloom_knee);
		}
		Ok(())
	}

	//==============================================================================
	/// 블룸 단 수 반환. (0 이면 기본 9 탭 블룸)
	///
	/// # 돌려줌
	/// 다단 블룸의 단 수. (0 이면 기본 9 탭 블룸)
	//==============================================================================
	pub fn get_bloom_level_count(&self) -> usize {
		self.multi_level_bloom_effect.as_ref().map_or(0, |multi_level_bloom_effect| multi_level_bloom_effect.get_level_count())
	}

	//==============================================================================
	/// 기본 9 탭 블룸의 주고받는 판을 실수로. (true 면 RGBA16F, 기본 false 는 RGBA8, 씬 판이 실수가 아니면 무시)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `is_float_color`: true 면 RGBA16F, false 면 RGBA8.
	///
	/// # 돌려줌
	/// 바꿨거나 이미 같으면 Ok. 새 판을 못 지으면 Err(GL 의 오류 글, 앞의 판은 그대로 둔다).
	//==============================================================================
	pub fn set_bloom_float_color(&mut self, gl: &GlContext, is_float_color: bool) -> Result<(), String> {
		let use_float_color = is_float_color && self.use_float_color;
		self.bloom_effect.set_use_float_color(gl, use_float_color)
	}

	//==============================================================================
	/// 밝기 설정. (색 보정 맨 앞에서 곱한다, 1 이 그대로)
	///
	/// # 인자
	/// - `brightness`: 밝기 배. (0 이상, 1 이 그대로)
	//==============================================================================
	pub fn set_brightness(&mut self, brightness: f64) {
		self.brightness = brightness.max(0.0);
	}

	//==============================================================================
	/// 밝힌 자리 설정. (그 자리에서 수평 거리 inner_radius 안은 밝기 1, outer_radius 밖은 set_brightness 의 밝기, 사이는 부드럽게)
	/// - 씬 깊이 텍스처(옵션 use_depth_texture)와 그 프레임의 보기와 투영의 역행렬로 화소마다 월드 자리를 되짚는다. 깊이 텍스처가 없으면 무시한다.
	/// - 카메라가 움직이면 프레임마다 다시 준다. 아무것도 그리지 않은 바탕은 먼 면의 자리로 셈한다.
	///
	/// # 인자
	/// - `center`: 밝힐 자리. ([x, y, z] 월드 자리, 수평 거리는 x 와 z 로 잰다)
	/// - `inner_radius`: 밝기 1 인 수평 반지름. (월드 길이)
	/// - `outer_radius`: 이 밖은 set_brightness 의 밝기인 수평 반지름. (월드 길이)
	/// - `inverse_view_projection_matrix`: 이번 프레임 보기와 투영의 역행렬.
	//==============================================================================
	pub fn set_lit_area(&mut self, center: [f64; 3], inner_radius: f64, outer_radius: f64, inverse_view_projection_matrix: &Matrix4) {
		let safe_outer_radius = outer_radius.max(0.0001);
		self.lit_area = Some(LitArea { center, inner_radius: inner_radius.min(safe_outer_radius).max(0.0), outer_radius: safe_outer_radius, inverse_view_projection_matrix: *inverse_view_projection_matrix });
	}

	//==============================================================================
	/// 밝힌 자리 풀기.
	//==============================================================================
	pub fn clear_lit_area(&mut self) {
		self.lit_area = None;
	}

	//==============================================================================
	/// 색 보정 설정. (채도와 대비는 1 이 그대로, 색조와 안개빛은 sRGB [r, g, b] 0 ~ 1 과 섞는 양 0 ~ 1)
	/// - 색조 색은 밝기(Rec. 709)를 1 로 맞춰 곱하므로 밝기는 그대로 두고 빛깔만 물든다.
	/// - 안개빛은 어두운 곳일수록 그 색 쪽으로 끌어올린다. (양 x (1 - 밝기))
	///
	/// # 인자
	/// - `saturation`: 채도. (1 이 그대로, 0 이면 흑백)
	/// - `contrast`: 대비. (1 이 그대로)
	/// - `tone_color`: [r, g, b] 색조 색. (sRGB 0 ~ 1, 밝기를 1 로 맞춰 곱한다)
	/// - `tone_amount`: 색조를 섞는 양. (0 ~ 1)
	/// - `haze_color`: [r, g, b] 안개빛 색. (sRGB 0 ~ 1)
	/// - `haze_amount`: 안개빛을 섞는 양. (0 ~ 1, 어두운 곳일수록 세다)
	//==============================================================================
	pub fn set_color_grade(&mut self, saturation: f64, contrast: f64, tone_color: [f64; 3], tone_amount: f64, haze_color: [f64; 3], haze_amount: f64) {
		let tone_luminance = tone_color[0] * LUMINANCE_WEIGHTS[0] + tone_color[1] * LUMINANCE_WEIGHTS[1] + tone_color[2] * LUMINANCE_WEIGHTS[2];
		let tone_scale = if tone_luminance > 0.0 { 1.0 / tone_luminance } else { 1.0 };
		self.saturation = saturation.max(0.0);
		self.contrast = contrast.max(0.0);
		self.tone_color = [tone_color[0] * tone_scale, tone_color[1] * tone_scale, tone_color[2] * tone_scale];
		self.tone_amount = tone_amount.clamp(0.0, 1.0);
		self.haze_color = haze_color;
		self.haze_amount = haze_amount.clamp(0.0, 1.0);
	}

	//==============================================================================
	/// 가장자리 어둡게 설정. (0 이 없음, 1 이면 구석이 까맣게)
	///
	/// # 인자
	/// - `vignette_strength`: 가장자리를 어둡게 하는 세기. (0 이 없음, 1 이면 구석이 까맣게)
	//==============================================================================
	pub fn set_vignette(&mut self, vignette_strength: f64) {
		self.vignette_strength = vignette_strength.max(0.0);
	}

	//==============================================================================
	/// 씬 렌더 타겟. (다른 후처리를 끼울 때)
	///
	/// # 돌려줌
	/// 씬을 그리는 렌더 타겟. (해상도 배율이나 표본 수를 바꾸면 다시 지어질 수 있다)
	//==============================================================================
	pub fn get_scene_render_target(&self) -> &RenderTarget {
		&self.scene_render_target
	}

	//==============================================================================
	/// 지우기.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&mut self, gl: &GlContext) {
		self.scene_render_target.destroy(gl);
		self.bloom_effect.destroy(gl);
		if let Some(mut multi_level_bloom_effect) = self.multi_level_bloom_effect.take() {
			multi_level_bloom_effect.destroy(gl);
		}
		self.composite_pass.destroy(gl);
		self.delete_ambient_occlusion_target(gl);
		if let Some(passes) = self.ambient_occlusion_passes.take() {
			for pass in passes.iter() {
				pass.destroy(gl);
			}
		}
		if let Some(mut prepass) = self.ambient_occlusion_prepass.take() {
			prepass.destroy(gl);
		}
	}

	//==============================================================================
	/// 색 공간 반환. (원본 getColorSpace, 생성 때 정한 값, 톤매핑 뒤 화면 색을 내는 곡선을 가른다)
	///
	/// # 돌려줌
	/// ColorSpace::Gamma 나 ColorSpace::Linear.
	//==============================================================================
	pub fn get_color_space(&self) -> ColorSpace {
		self.color_space
	}

	//==============================================================================
	/// 모서리 어둠(화면 공간 AO) 설정. (원본 setAmbientOcclusion, 기본 꺼짐, 켜면 draw_ambient_occlusion 이 셈하고 몸 셰이더가 바탕빛에 곱한다)
	///
	/// # 인자
	/// - `is_enabled`: 켤지.
	/// - `intensity`: 세기. (0 ~ 1 로 자름, 원본 기본 0.5)
	/// - `radius`: 가리는 것을 찾는 반지름. (월드 길이, 0.0001 이상, 원본 기본 0.5)
	/// - `resolution_scale`: 셈하는 판의 해상도 배율. (0.25 ~ 1 로 자름, 원본 기본 0.5)
	/// - `direct_lighting_strength`: 직접 빛(해, 채움빛, 점광의 확산과 반사광)에 미치는 몫. (0 ~ 1 로 자름, 원본 기본 0 이면 바탕빛에만)
	///
	/// # 예제
	/// 바깥선 없는 장면에 모서리 어둠을 켜고 그리기 줄에 맡기기
	/// ```rust,no_run
	/// use vanilla::{Matrix4, ToonDrawOptions, ToonDrawQueue, ToonMesh, ToonPostProcess, ToonRenderer};
	///
	/// fn draw_room(gl: &vanilla::GlContext, renderer: &mut ToonRenderer, post_process: &mut ToonPostProcess, room_mesh: &ToonMesh, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
	/// 	post_process.set_ambient_occlusion(true, 0.5, 0.5, 0.5, 0.5);
	/// 	assert_eq!(post_process.get_ambient_occlusion_direct_lighting_strength(), 0.5);
	/// 	post_process.begin(gl, [0.1, 0.1, 0.12]);
	/// 	{
	/// 		let mut queue = ToonDrawQueue::new();
	/// 		queue.set_post_process(Some(&mut *post_process));
	/// 		queue.add_mesh(room_mesh, &Matrix4::create_identity(), ToonDrawOptions::default());
	/// 		queue.draw_with_matrices(gl, renderer, view_matrix, projection_matrix);
	/// 	}
	/// 	post_process.end(gl, None, [0, 0, 640, 360]);
	/// }
	/// ```
	//==============================================================================
	pub fn set_ambient_occlusion(&mut self, is_enabled: bool, intensity: f64, radius: f64, resolution_scale: f64, direct_lighting_strength: f64) {
		self.ambient_occlusion = ToonAmbientOcclusion {
			is_enabled,
			intensity: intensity.clamp(0.0, 1.0),
			radius: radius.max(0.0001),
			resolution_scale: resolution_scale.clamp(0.25, 1.0),
			direct_lighting_strength: direct_lighting_strength.clamp(0.0, 1.0),
		};
	}

	//==============================================================================
	/// 모서리 어둠 설정 반환. (원본 getAmbientOcclusion)
	///
	/// # 돌려줌
	/// 설정. (ToonAmbientOcclusion, 베낀 것)
	//==============================================================================
	pub fn get_ambient_occlusion(&self) -> ToonAmbientOcclusion {
		self.ambient_occlusion
	}

	//==============================================================================
	/// 모서리 어둠이 직접 빛에 미치는 몫 반환. (원본 getAmbientOcclusionDirectLightingStrength, set_ambient_occlusion 의 다섯째 인자, 그리기 줄이 렌더러에 건다)
	///
	/// # 돌려줌
	/// 0 ~ 1.
	//==============================================================================
	pub fn get_ambient_occlusion_direct_lighting_strength(&self) -> f64 {
		self.ambient_occlusion.direct_lighting_strength
	}

	//==============================================================================
	/// 모서리 어둠을 켰는지. (원본 isAmbientOcclusionEnabled)
	///
	/// # 돌려줌
	/// set_ambient_occlusion 으로 켰으면 true.
	//==============================================================================
	pub fn is_ambient_occlusion_enabled(&self) -> bool {
		self.ambient_occlusion.is_enabled
	}

	//==============================================================================
	/// 바깥선이 없는 장면의 미리 그리기 반환. (원본 getAmbientOcclusionPrepass, 선 굵기 0 의 화면 바깥선, 처음 부를 때 짓는다)
	///
	/// # 인자
	/// - `gl`: 툰 렌더러와 같은 컨텍스트.
	///
	/// # 돌려줌
	/// 선의 판만 그리는 화면 바깥선. (begin_line_ids, draw_mesh, end_line_ids 로 그리고 get_line_target 을 draw_ambient_occlusion 에 준다) 짓지 못하면 Err(오류 글).
	//==============================================================================
	pub fn get_ambient_occlusion_prepass(&mut self, gl: &GlContext) -> Result<&mut ToonScreenOutline, String> {
		if self.ambient_occlusion_prepass.is_none() {
			let mut prepass = ToonScreenOutline::new_with_options(gl, ToonScreenOutlineOptions { color_space: self.color_space })?;
			prepass.set_width(0.0, ToonOutlineWidthUnit::Pixel);
			prepass.set_crease(180.0, 0.0, 1.0);
			self.ambient_occlusion_prepass = Some(prepass);
		}
		match self.ambient_occlusion_prepass.as_mut() {
			Some(prepass) => {
				Ok(prepass)
			}
			None => {
				unreachable!("바로 앞에서 지었다")
			}
		}
	}

	//==============================================================================
	/// 미리 그리기의 선의 판으로 모서리 어둠 셈하기. (draw_ambient_occlusion 에 get_ambient_occlusion_prepass 의 선의 판을 준 것과 같다, 미리 그리기가 없거나 꺼졌으면 None)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `view_matrix`: 선의 판을 그린 보기 행렬.
	/// - `projection_matrix`: 선의 판을 그린 투영 행렬.
	///
	/// # 돌려줌
	/// 결과 텍스처. (draw_ambient_occlusion 과 같다)
	//==============================================================================
	pub fn draw_ambient_occlusion_with_prepass(&mut self, gl: &GlContext, view_matrix: &Matrix4, projection_matrix: &Matrix4) -> Option<GlTexture> {
		let prepass = self.ambient_occlusion_prepass.take()?;
		let texture = match prepass.get_line_target() {
			Some(line_target) => {
				self.draw_ambient_occlusion(gl, line_target, view_matrix, projection_matrix)
			}
			None => {
				None
			}
		};
		self.ambient_occlusion_prepass = Some(prepass);
		texture
	}

	//==============================================================================
	/// 모서리 어둠 셈하기. (원본 drawAmbientOcclusion, 선의 판의 노멀과 깊이로 반 해상도 GTAO → 가로 흐림 → 세로 흐림과 키우기, 꺼졌으면 None)
	/// - 결과 텍스처는 선의 판 크기의 RGBA8 이다. (r 세기를 건 보임, gba 깊이 24 비트) ToonRenderer::set_ambient_occlusion_texture 에 준다.
	/// - 프레임버퍼, 뷰포트, 프로그램, 버텍스 어레이, 고른 텍스처 유닛, 깊이 시험, 섞기, 가위, 면 가리기를 되돌린다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `line_target`: 이번 프레임의 선의 판. (ToonScreenOutline::get_line_target)
	/// - `view_matrix`: 선의 판을 그린 보기 행렬.
	/// - `projection_matrix`: 선의 판을 그린 투영 행렬.
	///
	/// # 돌려줌
	/// 결과 텍스처. 꺼졌거나 판이나 셰이더를 짓지 못하면 None.
	//==============================================================================
	pub fn draw_ambient_occlusion(&mut self, gl: &GlContext, line_target: &ToonLineTarget, view_matrix: &Matrix4, projection_matrix: &Matrix4) -> Option<GlTexture> {
		let ambient_occlusion = self.ambient_occlusion;
		if !ambient_occlusion.is_enabled {
			return None;
		}
		let width = line_target.width;
		let height = line_target.height;
		let cell_width = ((width as f64 * ambient_occlusion.resolution_scale).ceil() as i32).max(1);
		let cell_height = ((height as f64 * ambient_occlusion.resolution_scale).ceil() as i32).max(1);
		if !self.ensure_ambient_occlusion_target(gl, width, height, cell_width, cell_height) {
			return None;
		}
		if self.ambient_occlusion_passes.is_none() {
			let occlusion_pass = FullscreenPass::new(gl, AMBIENT_OCCLUSION_FRAGMENT_SHADER);
			let blur_pass = FullscreenPass::new(gl, AMBIENT_OCCLUSION_BLUR_FRAGMENT_SHADER);
			let upsample_pass = FullscreenPass::new(gl, AMBIENT_OCCLUSION_UPSAMPLE_FRAGMENT_SHADER);
			match (occlusion_pass, blur_pass, upsample_pass) {
				(Ok(occlusion_pass), Ok(blur_pass), Ok(upsample_pass)) => {
					self.ambient_occlusion_passes = Some([occlusion_pass, blur_pass, upsample_pass]);
				}
				(occlusion_pass, blur_pass, upsample_pass) => {
					for pass in [occlusion_pass, blur_pass, upsample_pass] {
						match pass {
							Ok(pass) => {
								pass.destroy(gl);
							}
							Err(error) => {
								eprintln!("[ToonPostProcess] 모서리 어둠 셰이더를 짓지 못했다. {}", error);
							}
						}
					}
					return None;
				}
			}
		}
		let (target, passes) = match (self.ambient_occlusion_target.as_ref(), self.ambient_occlusion_passes.as_ref()) {
			(Some(target), Some(passes)) => {
				(target, passes)
			}
			_ => {
				return None;
			}
		};
		unsafe {
			let previous_framebuffer = read_framebuffer_binding(gl);
			let mut previous_viewport = [0i32; 4];
			crate::core::glstatecache::get_parameter_i32_slice(gl, glow::VIEWPORT, &mut previous_viewport);
			let previous_program = crate::graphics3d::read_program_binding(gl);
			let previous_vertex_array = crate::graphics3d::read_vertex_array_binding(gl);
			let previous_active_texture = crate::core::glstatecache::get_parameter_i32(gl, glow::ACTIVE_TEXTURE) as u32;
			let is_depth_test_enabled = crate::core::glstatecache::is_enabled(gl, glow::DEPTH_TEST);
			let is_blend_enabled = crate::core::glstatecache::is_enabled(gl, glow::BLEND);
			let is_scissor_test_enabled = crate::core::glstatecache::is_enabled(gl, glow::SCISSOR_TEST);
			let is_cull_face_enabled = crate::core::glstatecache::is_enabled(gl, glow::CULL_FACE);
			crate::core::glstatecache::disable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::disable(gl, glow::BLEND);
			crate::core::glstatecache::disable(gl, glow::SCISSOR_TEST);
			crate::core::glstatecache::disable(gl, glow::CULL_FACE);
			let view_elements = view_matrix.get_elements();
			let projection_elements = projection_matrix.get_elements();
			let inverse_projection_matrix = Matrix4::create_inverse(projection_matrix);
			let view_rotation = [view_elements[0], view_elements[1], view_elements[2], view_elements[4], view_elements[5], view_elements[6], view_elements[8], view_elements[9], view_elements[10]];
			let source_scale_x = width as f64 / cell_width as f64;
			let source_scale_y = height as f64 / cell_height as f64;
			let bind_common = |pass: &FullscreenPass, occlusion_texture: GlTexture| {
				pass.use_program(gl);
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.line_texture));
				gl.uniform_1_i32(pass.get_uniform_location(gl, "lineTexture").as_ref(), 0);
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE1);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(line_target.depth_texture));
				gl.uniform_1_i32(pass.get_uniform_location(gl, "depthTexture").as_ref(), 1);
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE2);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(occlusion_texture));
				gl.uniform_1_i32(pass.get_uniform_location(gl, "occlusionTexture").as_ref(), 2);
				gl.uniform_matrix_4_f32_slice(pass.get_uniform_location(gl, "inverseProjectionMatrix").as_ref(), false, &inverse_projection_matrix.elements);
				gl.uniform_matrix_3_f32_slice(pass.get_uniform_location(gl, "viewRotation").as_ref(), false, &view_rotation);
				gl.uniform_2_f64(pass.get_uniform_location(gl, "sourceSize").as_ref(), width as f64, height as f64);
				gl.uniform_2_f64(pass.get_uniform_location(gl, "sourceScale").as_ref(), source_scale_x, source_scale_y);
				gl.uniform_2_f64(pass.get_uniform_location(gl, "occlusionSize").as_ref(), cell_width as f64, cell_height as f64);
				gl.uniform_1_f64(pass.get_uniform_location(gl, "planeTolerance").as_ref(), ambient_occlusion.radius * 0.1);
			};

			// 1. 반 해상도 GTAO. (빈 칸 유닛 2 에는 결과 판이 아닌 흐림 판을 묶어 둔다)
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(target.occlusion_framebuffer));
			crate::core::glstatecache::viewport(gl, 0, 0, cell_width, cell_height);
			let occlusion_pass = &passes[0];
			bind_common(occlusion_pass, target.blur_texture);
			gl.uniform_4_f64(occlusion_pass.get_uniform_location(gl, "projectionRow").as_ref(), projection_elements[3] as f64, projection_elements[7] as f64, projection_elements[11] as f64, projection_elements[15] as f64);
			gl.uniform_1_f64(occlusion_pass.get_uniform_location(gl, "isPerspective").as_ref(), if projection_elements[11].abs() > 0.5 { 1.0 } else { 0.0 });
			gl.uniform_1_f64(occlusion_pass.get_uniform_location(gl, "radius").as_ref(), ambient_occlusion.radius);
			gl.uniform_1_f64(occlusion_pass.get_uniform_location(gl, "pixelsPerUnit").as_ref(), 0.5 * (projection_elements[0] as f64).abs() * width as f64);
			occlusion_pass.draw(gl);

			// 2. 가로 흐림. (반 해상도)
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(target.blur_framebuffer));
			bind_common(&passes[1], target.occlusion_texture);
			passes[1].draw(gl);

			// 3. 세로 흐림과 키우기. (선의 판 해상도, 세기를 건다)
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(target.result_framebuffer));
			crate::core::glstatecache::viewport(gl, 0, 0, width, height);
			let upsample_pass = &passes[2];
			bind_common(upsample_pass, target.blur_texture);
			gl.uniform_1_f64(upsample_pass.get_uniform_location(gl, "intensity").as_ref(), ambient_occlusion.intensity);
			upsample_pass.draw(gl);

			for unit in [2u32, 1, 0] {
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + unit);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			}
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, previous_framebuffer);
			crate::core::glstatecache::viewport(gl, previous_viewport[0], previous_viewport[1], previous_viewport[2], previous_viewport[3]);
			crate::core::glstatecache::use_program(gl, previous_program);
			crate::core::glstatecache::bind_vertex_array(gl, previous_vertex_array);
			crate::core::glstatecache::active_texture(gl, previous_active_texture);
			for (capability, is_enabled) in [(glow::DEPTH_TEST, is_depth_test_enabled), (glow::BLEND, is_blend_enabled), (glow::SCISSOR_TEST, is_scissor_test_enabled), (glow::CULL_FACE, is_cull_face_enabled)] {
				if is_enabled {
					crate::core::glstatecache::enable(gl, capability);
				}
			}
		}
		Some(target.result_texture)
	}

	//==============================================================================
	/// 모서리 어둠 결과 텍스처 반환. (원본 getAmbientOcclusionTexture, 아직 셈하지 않았거나 꺼졌으면 None)
	///
	/// # 돌려줌
	/// 결과 텍스처. (RGBA8, r 보임, gba 깊이)
	//==============================================================================
	pub fn get_ambient_occlusion_texture(&self) -> Option<GlTexture> {
		match (self.ambient_occlusion.is_enabled, self.ambient_occlusion_target.as_ref()) {
			(true, Some(target)) => {
				Some(target.result_texture)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 모서리 어둠 판이 그 크기로 있게 하기. (원본 readAmbientOcclusionTarget, 크기가 같으면 있던 것, 다르면 다시 짓는다, 짓지 못하면 false)
	//==============================================================================
	fn ensure_ambient_occlusion_target(&mut self, gl: &GlContext, width: i32, height: i32, cell_width: i32, cell_height: i32) -> bool {
		if let Some(target) = self.ambient_occlusion_target.as_ref() {
			if target.width == width && target.height == height && target.cell_width == cell_width && target.cell_height == cell_height {
				return true;
			}
		}
		self.delete_ambient_occlusion_target(gl);
		unsafe {
			let previous_texture = read_texture_binding(gl);
			let previous_framebuffer = read_framebuffer_binding(gl);
			let mut is_complete = true;
			let mut create_layer = |texture_width: i32, texture_height: i32| -> Option<(GlTexture, GlFramebuffer)> {
				let texture = crate::core::glstatecache::create_texture(gl).ok()?;
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(texture));
				gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, texture_width, texture_height);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
				let framebuffer = match crate::core::glstatecache::create_framebuffer(gl) {
					Ok(framebuffer) => {
						framebuffer
					}
					Err(_) => {
						crate::core::glstatecache::delete_texture(gl, texture);
						is_complete = false;
						return None;
					}
				};
				crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(framebuffer));
				gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
				is_complete = is_complete && gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
				Some((texture, framebuffer))
			};
			let occlusion_layer = create_layer(cell_width, cell_height);
			let blur_layer = create_layer(cell_width, cell_height);
			let result_layer = create_layer(width, height);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, previous_texture);
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, previous_framebuffer);
			let layers = [occlusion_layer, blur_layer, result_layer];
			if !is_complete || layers.iter().any(|layer| layer.is_none()) {
				for (texture, framebuffer) in layers.into_iter().flatten() {
					crate::core::glstatecache::delete_texture(gl, texture);
					crate::core::glstatecache::delete_framebuffer(gl, framebuffer);
				}
				return false;
			}
			let [occlusion_layer, blur_layer, result_layer] = layers.map(|layer| layer.unwrap_or_else(|| unreachable!("앞에서 걸렀다")));
			self.ambient_occlusion_target = Some(AmbientOcclusionTarget {
				occlusion_texture: occlusion_layer.0,
				occlusion_framebuffer: occlusion_layer.1,
				blur_texture: blur_layer.0,
				blur_framebuffer: blur_layer.1,
				result_texture: result_layer.0,
				result_framebuffer: result_layer.1,
				width,
				height,
				cell_width,
				cell_height,
			});
		}
		true
	}

	//==============================================================================
	/// 모서리 어둠 판 지우기. (원본 deleteAmbientOcclusionTarget)
	//==============================================================================
	fn delete_ambient_occlusion_target(&mut self, gl: &GlContext) {
		if let Some(target) = self.ambient_occlusion_target.take() {
			unsafe {
				for texture in [target.occlusion_texture, target.blur_texture, target.result_texture] {
					crate::core::glstatecache::delete_texture(gl, texture);
				}
				for framebuffer in [target.occlusion_framebuffer, target.blur_framebuffer, target.result_framebuffer] {
					crate::core::glstatecache::delete_framebuffer(gl, framebuffer);
				}
			}
		}
	}
}
