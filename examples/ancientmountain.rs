//==============================================================================
// 고대 산 예제. (데스크탑, 원본 examples/ancientmountain 과 같은 장면, 같은 UI, 같은 동작)
// - 순환로를 둘러싼 여섯 봉우리와 보조 봉우리, 경계 산맥, 풀과 흙길과 절벽 바위의 3중 텍스처 지형, 정이십면체 바위, 여우 여섯 마리, 정상 깃발.
//   3인칭 마네킹(Quaternius CC0, FBX + 애니메이션 FBX)이 달리고 점프하고 절벽을 오르고 가장자리에서 뛰어내린다.
//   그림자(OFF, SIMPLE 블롭, REALTIME 섀도우 맵), 블룸, 방사형 모션 블러, 비네트, 필름 그레인, ACES 톤맵, 충돌 링과 같은 옵션 패널도 같다.
// - 조작: W A S D 이동, SHIFT 질주, 방향키나 마우스 왼쪽 드래그 시점, SPACE 나 마우스 왼쪽 클릭 점프(X), O 나 마우스 오른쪽 클릭 상호작용(O), 게임패드(왼스틱, 오른스틱, A, B, R2).
//   화면 오른쪽 아래의 X, O 단추는 마우스로도 누를 수 있다.
// - 자산은 원본 저장소(vanilla.js 를 이 저장소 옆에 받아 둔 것)의 examples/ancientmountain/assets 를 읽기만 한다. 라이선스는 THIRD_PARTY_LICENSES.md 의 예제 자산 절.
//   마네킹과 클립은 Quaternius Universal Animation Library (CC0), 여우는 Khronos glTF-Sample-Assets 의 Fox (모형 PixelMannen CC0, 리깅과 애니메이션 tomkranis CC BY 4.0, glTF 변환 AsoboStudio 와 scurest CC BY 4.0),
//   풀, 바위, 흙, 볼더 그림은 이 저장소를 위해 Google 생성 AI 로 만든 것이다.
// - 갱신은 화면 저장 중에는 프레임마다 1/60초 고정이고, 평소에는 실제 시간이다.
// - 실행: cargo run --release --example ancientmountain (60초 뒤 스스로 끝난다)
//   화면 저장: VANILLA_CAPTURE_PATH=shots/mountain_{frame}.png VANILLA_CAPTURE_FRAMES=300 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::core::fullscreenpass::FullscreenPass;
use vanilla::core::graphic::{GlTexture, GlVertexArray};
use vanilla::core::shaderprogram::ShaderProgram;
use vanilla::{BloomEffect, Canvas2D, CanvasTextAlign, CanvasTextBaseline, Color, Engine, EngineConfiguration, GlContext, Graphic, Image, ImageAsset, ImageDescriptionLoadHandle, ImageDescriptionOptions, Material, Matrix4, MouseButton, Rect, RenderTarget, Scene, SceneBase, ShadowMap, SkinnedModel, SkinnedModelLoadHandle, SkinnedModelRenderer, NodeTransform, TextAlign, TextBaseline, TouchPhase, UIView, Vector2, Vector3, VirtualPad};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";
const EXIT_SECONDS: f64 = 60.0;
const FIXED_TIME_STEP: f64 = 1.0 / 60.0;
const SHADOWMAP_RESOLUTION: i32 = 2048;

// 지형 정의.
const PATH_RADIUS: f64 = 115.0;
const WORLD_HALF_SIZE: f64 = 192.0;
const GRID_CELL_SIZE: f64 = 1.5;
const FLAG_POLE_HEIGHT: f64 = 3.2;

// 캐릭터.
const RUN_SPEED: f64 = 4.8;
const SPRINT_SPEED: f64 = 7.4;
const JUMP_VELOCITY: f64 = 5.4;
const CLIMB_ASCEND_SPEED: f64 = 2.6;
const CLIMB_DESCEND_SPEED: f64 = 3.6;
const VAULT_DURATION: f64 = 0.55;
const CLIMB_ATTACH_DURATION: f64 = 0.24;
const BODY_BOX_HALF_WIDTH: f64 = 0.3;
const BODY_BOX_HALF_DEPTH: f64 = 0.24;

// 패널과 단추.
const PANEL_WIDTH: f64 = 236.0;
const PANEL_HEADER_HEIGHT: f64 = 40.0;
const PANEL_ROW_HEIGHT: f64 = 30.0;
const PANEL_MARGIN_RIGHT: f64 = 18.0;
const SLIDER_TRACK_WIDTH: f64 = 90.0;
const BUTTON_RADIUS: f64 = 34.0;
// 지형, 바위 텍스처. (엔진 애셋 읽기 길로 받는다: 데스크탑은 애셋 폴더 파일, 웹은 fetch)
const TEXTURE_NAME_LIST: [&str; 4] = ["grass.jpg", "rock.jpg", "dirt.jpg", "boulder.jpg"];

// 블롭 그림자.
const SHADOW_SEGMENT_COUNT: usize = 16;
const SHADOW_RADIUS: f64 = 0.55;
const STATIC_SHADOW_SEGMENT_COUNT: usize = 12;

// 하늘. (그라디언트 + 태양 + 절차적 구름 + 지평선 헤이즈)
const SKY_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentCoordinate;
uniform vec3 cameraRightDirection;
uniform vec3 cameraUpDirection;
uniform vec3 cameraForwardDirection;
uniform vec3 sunDirection;
uniform float tangentHalfFieldOfView;
uniform float aspectRatio;
uniform float elapsedTime;
out vec4 outputColor;

float hash21(vec2 point) {
	point = fract(point * vec2(234.34, 435.345));
	point += dot(point, point + 34.23);
	return fract(point.x * point.y);
}

float valueNoise(vec2 point) {
	vec2 cellOrigin = floor(point);
	vec2 cellFraction = point - cellOrigin;
	vec2 weight = cellFraction * cellFraction * (3.0 - 2.0 * cellFraction);
	float value00 = hash21(cellOrigin);
	float value10 = hash21(cellOrigin + vec2(1.0, 0.0));
	float value01 = hash21(cellOrigin + vec2(0.0, 1.0));
	float value11 = hash21(cellOrigin + vec2(1.0, 1.0));
	float valueX0 = mix(value00, value10, weight.x);
	float valueX1 = mix(value01, value11, weight.x);
	return mix(valueX0, valueX1, weight.y);
}

float fractalNoise(vec2 point) {
	float total = valueNoise(point) * 0.5;
	total += valueNoise(point * 2.13 + 17.0) * 0.28;
	total += valueNoise(point * 4.41 + 47.0) * 0.22;
	return total;
}

void main() {
	vec3 rayDirection = normalize(cameraForwardDirection
		+ cameraRightDirection * fragmentCoordinate.x * tangentHalfFieldOfView * aspectRatio
		+ cameraUpDirection * fragmentCoordinate.y * tangentHalfFieldOfView);

	float horizonBlend = clamp(rayDirection.y, 0.0, 1.0);
	vec3 zenithColor = vec3(0.2, 0.44, 0.82);
	vec3 horizonColor = vec3(0.78, 0.85, 0.93);
	vec3 color = mix(horizonColor, zenithColor, pow(horizonBlend, 0.55));
	color = mix(vec3(0.8, 0.85, 0.92), color, smoothstep(-0.06, 0.02, rayDirection.y));

	float sunAmount = max(dot(rayDirection, sunDirection), 0.0);
	color += vec3(1.0, 0.93, 0.78) * (pow(sunAmount, 900.0) * 4.0 + pow(sunAmount, 64.0) * 0.35 + pow(sunAmount, 8.0) * 0.12);

	if (rayDirection.y > 0.015) {
		vec2 cloudPoint = rayDirection.xz / (rayDirection.y + 0.12) * 1.7;
		cloudPoint += vec2(elapsedTime * 0.012, elapsedTime * 0.004);
		float cloudNoise = fractalNoise(cloudPoint * 0.6);
		float cloudAmount = smoothstep(0.52, 0.78, cloudNoise) * smoothstep(0.015, 0.12, rayDirection.y);
		color = mix(color, vec3(0.98, 0.985, 1.0), cloudAmount * 0.85);
	}

	outputColor = vec4(color, 1.0);
}";

// 섀도우 맵 샘플링 공용 GLSL. (PCF 9탭)
const SHADOW_SAMPLING_GLSL: &str = "
uniform highp sampler2DShadow shadowMapTexture;
uniform float shadowStrength;
uniform float shadowTexelSize;

float sampleShadowFactor(vec4 lightSpacePosition) {
	if (shadowStrength < 0.001) {
		return 1.0;
	}
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
	return mix(1.0, shadowSum / 9.0, shadowStrength);
}
";

const TERRAIN_VERTEXSHADER_SOURCE: &str = "#version 300 es
layout(location = 0) in vec3 vertexPosition;
layout(location = 1) in vec3 vertexNormal;
uniform mat4 viewProjectionMatrix;
uniform mat4 lightViewProjectionMatrix;
out vec3 worldPosition;
out vec3 worldNormal;
out vec4 lightSpacePosition;
void main() {
	worldPosition = vertexPosition;
	worldNormal = vertexNormal;
	lightSpacePosition = lightViewProjectionMatrix * vec4(vertexPosition, 1.0);
	gl_Position = viewProjectionMatrix * vec4(vertexPosition, 1.0);
}";

const TERRAIN_FRAGMENTSHADER_BODY: &str = "
float hash21(vec2 point) {
	point = fract(point * vec2(234.34, 435.345));
	point += dot(point, point + 34.23);
	return fract(point.x * point.y);
}

float valueNoise(vec2 point) {
	vec2 cellOrigin = floor(point);
	vec2 cellFraction = point - cellOrigin;
	vec2 weight = cellFraction * cellFraction * (3.0 - 2.0 * cellFraction);
	float value00 = hash21(cellOrigin);
	float value10 = hash21(cellOrigin + vec2(1.0, 0.0));
	float value01 = hash21(cellOrigin + vec2(0.0, 1.0));
	float value11 = hash21(cellOrigin + vec2(1.0, 1.0));
	float valueX0 = mix(value00, value10, weight.x);
	float valueX1 = mix(value01, value11, weight.x);
	return mix(valueX0, valueX1, weight.y);
}

float latticeRandom(int cellX, int cellZ) {
	uint hashValue = uint(cellX) * 374761393u + uint(cellZ) * 668265263u;
	hashValue = (hashValue ^ (hashValue >> 13u)) * 1274126177u;
	hashValue = hashValue ^ (hashValue >> 16u);
	return float(hashValue & 65535u) / 65535.0;
}

float latticeValueNoise(vec2 point) {
	vec2 cellOrigin = floor(point);
	vec2 cellFraction = point - cellOrigin;
	vec2 weight = cellFraction * cellFraction * (3.0 - 2.0 * cellFraction);
	int cellX = int(cellOrigin.x);
	int cellZ = int(cellOrigin.y);
	float value00 = latticeRandom(cellX, cellZ);
	float value10 = latticeRandom(cellX + 1, cellZ);
	float value01 = latticeRandom(cellX, cellZ + 1);
	float value11 = latticeRandom(cellX + 1, cellZ + 1);
	float valueX0 = mix(value00, value10, weight.x);
	float valueX1 = mix(value01, value11, weight.x);
	return mix(valueX0, valueX1, weight.y);
}

void main() {
	vec3 surfaceNormal = normalize(worldNormal);
	vec3 grassColor = texture(grassTexture, worldPosition.xz * 0.22).rgb;

	float pathAngleForUV = atan(worldPosition.z, worldPosition.x);
	float pathArcLength = pathAngleForUV * pathRadius;
	float pathRadialOffset = length(worldPosition.xz) - pathRadius;
	vec2 pathAlignedUV = vec2(pathArcLength * 0.16, pathRadialOffset * 0.2);
	vec3 dirtColor = texture(dirtTexture, pathAlignedUV).rgb;
	vec3 normalWeight = abs(surfaceNormal);
	normalWeight = normalWeight / (normalWeight.x + normalWeight.y + normalWeight.z);
	vec3 rockColorX = texture(rockTexture, worldPosition.zy * 0.12).rgb;
	vec3 rockColorY = texture(rockTexture, worldPosition.xz * 0.12).rgb;
	vec3 rockColorZ = texture(rockTexture, worldPosition.xy * 0.12).rgb;
	vec3 rockColor = rockColorX * normalWeight.x + rockColorY * normalWeight.y + rockColorZ * normalWeight.z;

	float slopeAmount = 1.0 - surfaceNormal.y;
	float rockBlend = smoothstep(0.28, 0.5, slopeAmount);
	rockBlend = max(rockBlend, smoothstep(20.0, 32.0, worldPosition.y));

	float pathAngle = atan(worldPosition.z, worldPosition.x);
	float pathWobble = (latticeValueNoise(vec2(cos(pathAngle) * 3.0 + 7.0, sin(pathAngle) * 3.0 + 7.0)) - 0.5) * 16.0;
	float pathWidth = 3.2 + valueNoise(vec2(pathAngle * 2.0 + 31.0, 5.0)) * 2.4;
	float pathDistance = abs(length(worldPosition.xz) - (pathRadius + pathWobble));
	float pathBlend = (1.0 - smoothstep(pathWidth, pathWidth + 3.4, pathDistance)) * (1.0 - rockBlend) * (1.0 - smoothstep(2.5, 4.5, worldPosition.y));

	vec3 albedo = mix(grassColor, dirtColor, pathBlend);
	albedo = mix(albedo, rockColor, rockBlend);

	float macroVariation = 0.9 + 0.2 * valueNoise(worldPosition.xz * 0.035);
	albedo *= macroVariation;

	float sunDiffuse = max(dot(surfaceNormal, sunDirection), 0.0);
	float shadowFactor = sampleShadowFactor(lightSpacePosition);
	vec3 skyAmbient = mix(vec3(0.3, 0.33, 0.36), vec3(0.52, 0.58, 0.66), surfaceNormal.y * 0.5 + 0.5);
	vec3 color = albedo * (vec3(1.15, 1.05, 0.9) * sunDiffuse * shadowFactor + skyAmbient * (0.6 + 0.4 * shadowFactor));

	float viewDistance = distance(cameraPosition, worldPosition);
	float fogBlend = smoothstep(55.0, 250.0, viewDistance);
	color = mix(color, vec3(0.8, 0.85, 0.92), fogBlend);

	outputColor = vec4(color, 1.0);
}";

const TERRAIN_FRAGMENTSHADER_HEAD: &str = "#version 300 es
precision highp float;
in vec3 worldPosition;
in vec3 worldNormal;
in vec4 lightSpacePosition;
uniform sampler2D grassTexture;
uniform sampler2D rockTexture;
uniform sampler2D dirtTexture;
uniform vec3 sunDirection;
uniform vec3 cameraPosition;
uniform float pathRadius;
out vec4 outputColor;
";

const OBJECT_VERTEXSHADER_SOURCE: &str = "#version 300 es
layout(location = 0) in vec3 vertexPosition;
layout(location = 1) in vec3 vertexNormal;
uniform mat4 modelMatrix;
uniform mat4 viewProjectionMatrix;
uniform mat4 lightViewProjectionMatrix;
out vec3 worldPosition;
out vec3 worldNormal;
out vec4 lightSpacePosition;
void main() {
	vec4 transformedPosition = modelMatrix * vec4(vertexPosition, 1.0);
	worldPosition = transformedPosition.xyz;
	worldNormal = normalize(mat3(modelMatrix) * vertexNormal);
	lightSpacePosition = lightViewProjectionMatrix * transformedPosition;
	gl_Position = viewProjectionMatrix * transformedPosition;
}";

const OBJECT_FRAGMENTSHADER_HEAD: &str = "#version 300 es
precision highp float;
in vec3 worldPosition;
in vec3 worldNormal;
in vec4 lightSpacePosition;
uniform vec3 objectColor;
uniform vec3 sunDirection;
uniform vec3 cameraPosition;
out vec4 outputColor;
";

const OBJECT_FRAGMENTSHADER_BODY: &str = "
void main() {
	vec3 surfaceNormal = normalize(worldNormal);
	float sunDiffuse = max(dot(surfaceNormal, sunDirection), 0.0);
	float shadowFactor = sampleShadowFactor(lightSpacePosition);
	vec3 skyAmbient = mix(vec3(0.3, 0.33, 0.36), vec3(0.52, 0.58, 0.66), surfaceNormal.y * 0.5 + 0.5);
	vec3 color = objectColor * (vec3(1.15, 1.05, 0.9) * sunDiffuse * shadowFactor + skyAmbient * (0.6 + 0.4 * shadowFactor));
	float viewDistance = distance(cameraPosition, worldPosition);
	float fogBlend = smoothstep(55.0, 250.0, viewDistance);
	color = mix(color, vec3(0.8, 0.85, 0.92), fogBlend);
	outputColor = vec4(color, 1.0);
}";

const ROCK_FRAGMENTSHADER_HEAD: &str = "#version 300 es
precision highp float;
in vec3 worldPosition;
in vec3 worldNormal;
in vec4 lightSpacePosition;
uniform sampler2D boulderTexture;
uniform vec3 tintColor;
uniform vec3 sunDirection;
uniform vec3 cameraPosition;
out vec4 outputColor;
";

const ROCK_FRAGMENTSHADER_BODY: &str = "
void main() {
	vec3 surfaceNormal = normalize(worldNormal);
	vec3 normalWeight = abs(surfaceNormal);
	normalWeight = normalWeight / (normalWeight.x + normalWeight.y + normalWeight.z);
	vec3 rockColorX = texture(boulderTexture, worldPosition.zy * 0.32).rgb;
	vec3 rockColorY = texture(boulderTexture, worldPosition.xz * 0.32).rgb;
	vec3 rockColorZ = texture(boulderTexture, worldPosition.xy * 0.32).rgb;
	vec3 albedo = (rockColorX * normalWeight.x + rockColorY * normalWeight.y + rockColorZ * normalWeight.z) * tintColor;
	float sunDiffuse = max(dot(surfaceNormal, sunDirection), 0.0);
	float shadowFactor = sampleShadowFactor(lightSpacePosition);
	vec3 skyAmbient = mix(vec3(0.3, 0.33, 0.36), vec3(0.52, 0.58, 0.66), surfaceNormal.y * 0.5 + 0.5);
	vec3 color = albedo * (vec3(1.15, 1.05, 0.9) * sunDiffuse * shadowFactor + skyAmbient * (0.6 + 0.4 * shadowFactor));
	float viewDistance = distance(cameraPosition, worldPosition);
	float fogBlend = smoothstep(55.0, 250.0, viewDistance);
	color = mix(color, vec3(0.8, 0.85, 0.92), fogBlend);
	outputColor = vec4(color, 1.0);
}";

const DEPTH_VERTEXSHADER_SOURCE: &str = "#version 300 es
layout(location = 0) in vec3 vertexPosition;
uniform mat4 modelMatrix;
uniform mat4 lightViewProjectionMatrix;
void main() {
	gl_Position = lightViewProjectionMatrix * modelMatrix * vec4(vertexPosition, 1.0);
}";
const DEPTH_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
void main() {
}";

// 최종 합성. (블룸 가산 + 방사형 모션 블러 + 비네트 + 필름 그레인 + ACES 톤맵)
const COMPOSITE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentCoordinate;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform vec2 resolution;
uniform float elapsedTime;
uniform float bloomStrength;
uniform float motionBlurStrength;
uniform vec2 motionBlurCenter;
uniform float vignetteStrength;
uniform float filmStrength;
out vec4 outputColor;

float hash21(vec2 point) {
	point = fract(point * vec2(234.34, 435.345));
	point += dot(point, point + 34.23);
	return fract(point.x * point.y);
}

void main() {
	vec3 color;
	if (motionBlurStrength > 0.001) {
		vec2 blurVector = (motionBlurCenter - fragmentTextureCoordinate) * motionBlurStrength;
		color = texture(sceneTexture, fragmentTextureCoordinate).rgb;
		for (int sampleIndex = 1; sampleIndex < 8; ++sampleIndex) {
			vec2 sampleOffset = blurVector * (float(sampleIndex) / 7.0);
			color += texture(sceneTexture, fragmentTextureCoordinate + sampleOffset).rgb;
		}
		color /= 8.0;
	}
	else {
		color = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	}

	vec3 bloomColor = texture(bloomTexture, fragmentTextureCoordinate).rgb;
	color += bloomColor * bloomStrength;

	float grain = hash21(fragmentTextureCoordinate * resolution + fract(elapsedTime) * 371.0);
	color += (grain - 0.5) * 0.02 * filmStrength;

	float vignette = 1.0 - dot(fragmentCoordinate * 0.5, fragmentCoordinate * 0.5) * 0.62 * vignetteStrength;
	color *= clamp(vignette, 0.0, 1.0);

	color = clamp((color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14), 0.0, 1.0);

	outputColor = vec4(color, 1.0);
}";

const SHADOW_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec4 vertexShadow;
uniform mat4 viewProjectionMatrix;
out float fragmentAlpha;
void main() {
	fragmentAlpha = vertexShadow.w;
	gl_Position = viewProjectionMatrix * vec4(vertexShadow.xyz, 1.0);
}";
const SHADOW_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in float fragmentAlpha;
uniform float shadowStrength;
out vec4 outputColor;
void main() {
	outputColor = vec4(0.05, 0.07, 0.05, fragmentAlpha * shadowStrength);
}";

//==============================================================================
// 격자 해시와 노이즈. (셰이더와 비트 단위로 같다)
//==============================================================================
fn lattice_random_at(cell_x: i32, cell_z: i32) -> f64 {
	let mut hash_value = (cell_x as u32).wrapping_mul(374761393).wrapping_add((cell_z as u32).wrapping_mul(668265263));
	hash_value = (hash_value ^ (hash_value >> 13)).wrapping_mul(1274126177);
	hash_value ^= hash_value >> 16;
	(hash_value & 65535) as f64 / 65535.0
}

fn smoothstep_value(edge_start: f64, edge_end: f64, value: f64) -> f64 {
	let normalized = ((value - edge_start) / (edge_end - edge_start)).clamp(0.0, 1.0);
	normalized * normalized * (3.0 - 2.0 * normalized)
}

fn value_noise_at(point_x: f64, point_z: f64) -> f64 {
	let cell_origin_x = point_x.floor();
	let cell_origin_z = point_z.floor();
	let fraction_x = point_x - cell_origin_x;
	let fraction_z = point_z - cell_origin_z;
	let weight_x = fraction_x * fraction_x * (3.0 - 2.0 * fraction_x);
	let weight_z = fraction_z * fraction_z * (3.0 - 2.0 * fraction_z);
	let cell_x = cell_origin_x as i32;
	let cell_z = cell_origin_z as i32;
	let value_00 = lattice_random_at(cell_x, cell_z);
	let value_10 = lattice_random_at(cell_x + 1, cell_z);
	let value_01 = lattice_random_at(cell_x, cell_z + 1);
	let value_11 = lattice_random_at(cell_x + 1, cell_z + 1);
	let value_x0 = value_00 + (value_10 - value_00) * weight_x;
	let value_x1 = value_01 + (value_11 - value_01) * weight_x;
	value_x0 + (value_x1 - value_x0) * weight_z
}

fn wrap_angle(mut angle: f64) -> f64 {
	while angle > std::f64::consts::PI {
		angle -= std::f64::consts::PI * 2.0;
	}
	while angle < -std::f64::consts::PI {
		angle += std::f64::consts::PI * 2.0;
	}
	angle
}

fn rgba(red: f64, green: f64, blue: f64, alpha: f64) -> Color {
	Color::create(red / 255.0, green / 255.0, blue / 255.0, alpha)
}

fn float_bytes(value_list: &[f32]) -> Vec<u8> {
	value_list.iter().flat_map(|value| value.to_le_bytes()).collect()
}

//==============================================================================
// 산. (로브 조합 + 가장자리 노이즈)
//==============================================================================
struct Lobe {
	offset_x: f64,
	offset_z: f64,
	radius: f64,
	height: f64,
}

struct Mountain {
	center_x: f64,
	center_z: f64,
	edge_seed: f64,
	edge_amplitude: f64,
	lobe_list: Vec<Lobe>,
	bound_radius: f64,
	summit_x: f64,
	summit_z: f64,
	summit_radius: f64,
	summit_height: f64,
}

impl Mountain {
	fn new(angle_degree: f64, ring_radius: f64, edge_seed: f64, edge_amplitude: f64, lobe_values: &[(f64, f64, f64, f64)]) -> Mountain {
		let angle_radian = angle_degree * std::f64::consts::PI / 180.0;
		let center_x = angle_radian.cos() * ring_radius;
		let center_z = angle_radian.sin() * ring_radius;
		let lobe_list: Vec<Lobe> = lobe_values.iter().map(|value| Lobe { offset_x: value.0, offset_z: value.1, radius: value.2, height: value.3 }).collect();
		let mut summit_index = 0;
		for (lobe_index, lobe) in lobe_list.iter().enumerate() {
			if lobe.height > lobe_list[summit_index].height {
				summit_index = lobe_index;
			}
		}
		let mut bound_radius: f64 = 0.0;
		for lobe in lobe_list.iter() {
			let lobe_reach = (lobe.offset_x * lobe.offset_x + lobe.offset_z * lobe.offset_z).sqrt() + lobe.radius * (1.0 + edge_amplitude) + 2.0;
			bound_radius = bound_radius.max(lobe_reach);
		}
		let summit_lobe = &lobe_list[summit_index];
		Mountain {
			center_x,
			center_z,
			edge_seed,
			edge_amplitude,
			summit_x: center_x + summit_lobe.offset_x,
			summit_z: center_z + summit_lobe.offset_z,
			summit_radius: summit_lobe.radius,
			summit_height: summit_lobe.height,
			bound_radius,
			lobe_list,
		}
	}
}

//==============================================================================
// 세계. (지형 높이 계산, 주 봉우리 여섯 + 보조 봉우리 여섯)
//==============================================================================
struct World {
	mountain_list: Vec<Mountain>,
	hill_count: usize,
}

impl World {
	fn new() -> World {
		let mut mountain_list = vec![
			Mountain::new(0.0, 112.0, 11.0, 0.2, &[(0.0, 0.0, 34.0, 10.0), (5.0, 2.0, 17.0, 17.0)]),
			Mountain::new(55.0, 121.0, 23.0, 0.16, &[(0.0, 0.0, 26.0, 13.0), (-17.0, 10.0, 21.0, 12.0)]),
			Mountain::new(118.0, 116.0, 37.0, 0.26, &[(0.0, 0.0, 22.0, 15.0)]),
			Mountain::new(175.0, 110.0, 41.0, 0.22, &[(0.0, 0.0, 38.0, 8.5)]),
			Mountain::new(242.0, 122.0, 53.0, 0.18, &[(0.0, 0.0, 32.0, 11.0), (-8.0, -10.0, 16.0, 18.0)]),
			Mountain::new(300.0, 115.0, 67.0, 0.24, &[(0.0, 0.0, 24.0, 12.0), (20.0, -12.0, 18.0, 10.0)]),
		];
		let hill_count = mountain_list.len();
		mountain_list.push(Mountain::new(27.0, 138.0, 71.0, 0.24, &[(0.0, 0.0, 26.0, 11.0), (-14.0, 6.0, 18.0, 9.0)]));
		mountain_list.push(Mountain::new(88.0, 142.0, 79.0, 0.2, &[(0.0, 0.0, 30.0, 13.0)]));
		mountain_list.push(Mountain::new(147.0, 136.0, 83.0, 0.26, &[(0.0, 0.0, 24.0, 9.0), (16.0, -8.0, 17.0, 12.0)]));
		mountain_list.push(Mountain::new(210.0, 144.0, 89.0, 0.22, &[(0.0, 0.0, 28.0, 12.0)]));
		mountain_list.push(Mountain::new(272.0, 138.0, 97.0, 0.18, &[(0.0, 0.0, 32.0, 10.0), (-10.0, -12.0, 16.0, 14.0)]));
		mountain_list.push(Mountain::new(330.0, 140.0, 101.0, 0.24, &[(0.0, 0.0, 25.0, 11.0)]));
		World { mountain_list, hill_count }
	}

	// 지형 높이. (길 주변만 완만한 기복 평야 + 중턱 절벽 밴드를 두른 자연 산 + 외곽 경계 산맥)
	fn terrain_height_at(&self, world_x: f64, world_z: f64) -> f64 {
		let center_distance = (world_x * world_x + world_z * world_z).sqrt();
		let path_angle = world_z.atan2(world_x);
		let path_wobble = (value_noise_at(path_angle.cos() * 3.0 + 7.0, path_angle.sin() * 3.0 + 7.0) - 0.5) * 16.0;
		let path_distance = (center_distance - (PATH_RADIUS + path_wobble)).abs();
		let relief_factor = 0.3 + 0.7 * smoothstep_value(4.0, 18.0, path_distance);

		let rolling_large = value_noise_at(world_x * 0.014, world_z * 0.014);
		let rolling_medium = value_noise_at(world_x * 0.04 + 17.0, world_z * 0.04 + 17.0);
		let rolling_small = value_noise_at(world_x * 0.09 + 41.0, world_z * 0.09 + 41.0);
		let mut height = (rolling_large * 4.2 + rolling_medium * 1.8 + rolling_small * 0.6) * relief_factor;

		let ridge_angle_noise = value_noise_at(path_angle.cos() * 2.4 + 13.0, path_angle.sin() * 2.4 + 13.0);
		let ridge_center_radius = 132.0 + (ridge_angle_noise - 0.5) * 22.0;
		let ridge_distance = (center_distance - ridge_center_radius).abs();
		let ridge_profile = (1.0 - ridge_distance / 26.0).max(0.0);
		let ridge_height_noise = value_noise_at(path_angle.cos() * 4.2 + 57.0, path_angle.sin() * 4.2 + 57.0);
		height += ridge_profile * ridge_profile * (5.5 + ridge_height_noise * 4.0) * (0.4 + 0.6 * relief_factor);

		for hill in self.mountain_list.iter() {
			let rough_delta_x = world_x - hill.center_x;
			let rough_delta_z = world_z - hill.center_z;
			if rough_delta_x * rough_delta_x + rough_delta_z * rough_delta_z > hill.bound_radius * hill.bound_radius {
				continue;
			}
			let mut hill_contribution: f64 = 0.0;
			for lobe in hill.lobe_list.iter() {
				let delta_x = world_x - (hill.center_x + lobe.offset_x);
				let delta_z = world_z - (hill.center_z + lobe.offset_z);
				let lobe_distance = (delta_x * delta_x + delta_z * delta_z).sqrt();
				let edge_angle = delta_z.atan2(delta_x);
				let edge_noise = value_noise_at(edge_angle.cos() * 2.6 + hill.edge_seed, edge_angle.sin() * 2.6 + hill.edge_seed);
				let modulated_radius = lobe.radius * (1.0 + (edge_noise * 2.0 - 1.0) * hill.edge_amplitude);
				let falloff_ratio = (1.0 - lobe_distance / modulated_radius).clamp(0.0, 1.0);
				if falloff_ratio > 0.0 {
					let base_shape = falloff_ratio * falloff_ratio;
					let band_half_ratio = 1.6 / lobe.radius;
					let cliff_shape = smoothstep_value(0.5 - band_half_ratio, 0.5 + band_half_ratio, falloff_ratio);
					let surface_detail = value_noise_at(world_x * 0.1 + 71.0, world_z * 0.1 + 71.0) * 0.8 * falloff_ratio;
					let lobe_height = lobe.height * (base_shape * 0.55 + cliff_shape * 0.45) + surface_detail;
					hill_contribution = hill_contribution.max(lobe_height);
				}
			}
			height += hill_contribution;
		}

		if center_distance > 158.0 {
			let boundary_factor = (center_distance - 158.0) / 34.0;
			let ridge_variation = 0.7 + 0.6 * value_noise_at(path_angle.cos() * 6.0 + 9.0, path_angle.sin() * 6.0 + 9.0);
			height += boundary_factor.powf(2.4) * 55.0 * ridge_variation;
		}
		height
	}

	// 지형 표면 높이. (메시와 같게 셀 코너 높이를 쌍선형 보간, 발밑과 카메라용)
	fn terrain_surface_height_at(&self, world_x: f64, world_z: f64) -> f64 {
		let cell_start_x = (world_x / GRID_CELL_SIZE).floor() * GRID_CELL_SIZE;
		let cell_start_z = (world_z / GRID_CELL_SIZE).floor() * GRID_CELL_SIZE;
		let normalized_x = (world_x - cell_start_x) / GRID_CELL_SIZE;
		let normalized_z = (world_z - cell_start_z) / GRID_CELL_SIZE;
		let corner_height_00 = self.terrain_height_at(cell_start_x, cell_start_z);
		let corner_height_10 = self.terrain_height_at(cell_start_x + GRID_CELL_SIZE, cell_start_z);
		let corner_height_01 = self.terrain_height_at(cell_start_x, cell_start_z + GRID_CELL_SIZE);
		let corner_height_11 = self.terrain_height_at(cell_start_x + GRID_CELL_SIZE, cell_start_z + GRID_CELL_SIZE);
		let near_height = corner_height_00 + (corner_height_10 - corner_height_00) * normalized_x;
		let far_height = corner_height_01 + (corner_height_11 - corner_height_01) * normalized_x;
		near_height + (far_height - near_height) * normalized_z
	}

	// 캐릭터 바디 박스가 덮는 지형의 최고 높이. (진행 방향으로 회전한 사각형의 바닥면)
	fn sample_body_box_terrain_height(&self, center_x: f64, center_z: f64, forward_x: f64, forward_z: f64) -> f64 {
		let right_x = -forward_z;
		let right_z = forward_x;
		let mut max_terrain_height = f64::NEG_INFINITY;
		for depth_index in -1..=1 {
			for width_index in -1..=1 {
				let offset_depth = BODY_BOX_HALF_DEPTH * depth_index as f64;
				let offset_width = BODY_BOX_HALF_WIDTH * width_index as f64;
				let sample_x = center_x + forward_x * offset_depth + right_x * offset_width;
				let sample_z = center_z + forward_z * offset_depth + right_z * offset_width;
				max_terrain_height = max_terrain_height.max(self.terrain_surface_height_at(sample_x, sample_z));
			}
		}
		max_terrain_height
	}
}

//==============================================================================
// 바위와 여우.
//==============================================================================
struct Rock {
	matrix: Matrix4,
	tint: [f32; 3],
	variant_index: usize,
	center_x: f64,
	center_z: f64,
	block_radius: f64,
	top_height: f64,
}

struct Fox {
	model: SkinnedModel,
	orbit_radius: f64,
	orbit_speed: f64,
	angle_phase: f64,
	body_scale: f64,
	current_x: f64,
	current_y: f64,
	current_z: f64,
	current_yaw: f64,
	block_radius: f64,
	top_height: f64,
}

struct RockVariant {
	vertex_array: GlVertexArray,
	vertex_count: i32,
}

//==============================================================================
// 정이십면체 1회 세분 + 노이즈 변형 바위 메시. (플랫 셰이딩, 위치와 노멀)
//==============================================================================
fn create_rock_geometry(shape_seed: f64) -> (Vec<f32>, Vec<f32>) {
	let golden_ratio = (1.0 + 5.0f64.sqrt()) / 2.0;
	let base_vertex_list: [[f64; 3]; 12] = [
		[-1.0, golden_ratio, 0.0], [1.0, golden_ratio, 0.0], [-1.0, -golden_ratio, 0.0], [1.0, -golden_ratio, 0.0],
		[0.0, -1.0, golden_ratio], [0.0, 1.0, golden_ratio], [0.0, -1.0, -golden_ratio], [0.0, 1.0, -golden_ratio],
		[golden_ratio, 0.0, -1.0], [golden_ratio, 0.0, 1.0], [-golden_ratio, 0.0, -1.0], [-golden_ratio, 0.0, 1.0],
	];
	let base_face_list: [[usize; 3]; 20] = [
		[0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
		[1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
		[3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
		[4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
	];
	let mut vertex_list: Vec<[f64; 3]> = base_vertex_list.iter().map(|vertex| {
		let length = (vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2]).sqrt();
		[vertex[0] / length, vertex[1] / length, vertex[2] / length]
	}).collect();
	let mut middle_vertex_cache: HashMap<(usize, usize), usize> = HashMap::new();
	let mut get_middle_vertex_index = |vertex_list: &mut Vec<[f64; 3]>, index_a: usize, index_b: usize| -> usize {
		let cache_key = if index_a < index_b { (index_a, index_b) } else { (index_b, index_a) };
		if let Some(middle_index) = middle_vertex_cache.get(&cache_key) {
			return *middle_index;
		}
		let vertex_a = vertex_list[index_a];
		let vertex_b = vertex_list[index_b];
		let middle_x = (vertex_a[0] + vertex_b[0]) / 2.0;
		let middle_y = (vertex_a[1] + vertex_b[1]) / 2.0;
		let middle_z = (vertex_a[2] + vertex_b[2]) / 2.0;
		let middle_length = (middle_x * middle_x + middle_y * middle_y + middle_z * middle_z).sqrt();
		vertex_list.push([middle_x / middle_length, middle_y / middle_length, middle_z / middle_length]);
		let middle_index = vertex_list.len() - 1;
		middle_vertex_cache.insert(cache_key, middle_index);
		middle_index
	};
	let mut face_list: Vec<[usize; 3]> = Vec::new();
	for base_face in base_face_list.iter() {
		let middle_ab = get_middle_vertex_index(&mut vertex_list, base_face[0], base_face[1]);
		let middle_bc = get_middle_vertex_index(&mut vertex_list, base_face[1], base_face[2]);
		let middle_ca = get_middle_vertex_index(&mut vertex_list, base_face[2], base_face[0]);
		face_list.push([base_face[0], middle_ab, middle_ca]);
		face_list.push([base_face[1], middle_bc, middle_ab]);
		face_list.push([base_face[2], middle_ca, middle_bc]);
		face_list.push([middle_ab, middle_bc, middle_ca]);
	}
	let deformed_vertex_list: Vec<[f64; 3]> = vertex_list.iter().map(|vertex| {
		let noise_value = (
			value_noise_at(vertex[0] * 1.8 + shape_seed, vertex[1] * 1.8 + shape_seed * 1.7)
			+ value_noise_at(vertex[1] * 1.8 + shape_seed * 2.3, vertex[2] * 1.8 + shape_seed * 3.1)
			+ value_noise_at(vertex[2] * 1.8 + shape_seed * 4.7, vertex[0] * 1.8 + shape_seed * 5.9)
		) / 3.0;
		let radial_scale = 1.0 + (noise_value * 2.0 - 1.0) * 0.45;
		[vertex[0] * radial_scale, vertex[1] * radial_scale * 0.82, vertex[2] * radial_scale]
	}).collect();
	let mut position_list: Vec<f32> = Vec::new();
	let mut normal_list: Vec<f32> = Vec::new();
	for face in face_list.iter() {
		let vertex_a = deformed_vertex_list[face[0]];
		let vertex_b = deformed_vertex_list[face[1]];
		let vertex_c = deformed_vertex_list[face[2]];
		let edge_ab = [vertex_b[0] - vertex_a[0], vertex_b[1] - vertex_a[1], vertex_b[2] - vertex_a[2]];
		let edge_ac = [vertex_c[0] - vertex_a[0], vertex_c[1] - vertex_a[1], vertex_c[2] - vertex_a[2]];
		let mut normal = [edge_ab[1] * edge_ac[2] - edge_ab[2] * edge_ac[1], edge_ab[2] * edge_ac[0] - edge_ab[0] * edge_ac[2], edge_ab[0] * edge_ac[1] - edge_ab[1] * edge_ac[0]];
		let normal_length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
		normal = [normal[0] / normal_length, normal[1] / normal_length, normal[2] / normal_length];
		let centroid = [(vertex_a[0] + vertex_b[0] + vertex_c[0]) / 3.0, (vertex_a[1] + vertex_b[1] + vertex_c[1]) / 3.0, (vertex_a[2] + vertex_b[2] + vertex_c[2]) / 3.0];
		if normal[0] * centroid[0] + normal[1] * centroid[1] + normal[2] * centroid[2] < 0.0 {
			normal = [-normal[0], -normal[1], -normal[2]];
		}
		for vertex in [vertex_a, vertex_b, vertex_c] {
			position_list.extend_from_slice(&[vertex[0] as f32, vertex[1] as f32, vertex[2] as f32]);
			normal_list.extend_from_slice(&[normal[0] as f32, normal[1] as f32, normal[2] as f32]);
		}
	}
	(position_list, normal_list)
}

//==============================================================================
// 정점 배열 만들기. (위치 3 + 노멀 3 두 버퍼, 인덱스는 바이트와 하나의 모양)
//==============================================================================
fn create_vertex_array_with_normal(gl: &GlContext, position_list: &[f32], normal_list: &[f32], index_bytes: Option<&[u8]>) -> Result<GlVertexArray, String> {
	unsafe {
		let vertex_array = gl.create_vertex_array()?;
		gl.bind_vertex_array(Some(vertex_array));
		let position_buffer = gl.create_buffer()?;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(position_buffer));
		gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(position_list), glow::STATIC_DRAW);
		gl.enable_vertex_attrib_array(0);
		gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 0, 0);
		let normal_buffer = gl.create_buffer()?;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(normal_buffer));
		gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(normal_list), glow::STATIC_DRAW);
		gl.enable_vertex_attrib_array(1);
		gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 0, 0);
		if let Some(index_bytes) = index_bytes {
			let index_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index_buffer));
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_bytes, glow::STATIC_DRAW);
		}
		gl.bind_vertex_array(None);
		Ok(vertex_array)
	}
}

//==============================================================================
// 캐릭터 상태.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum CharacterState {
	Ground,
	ClimbAttach,
	Climbing,
	ClimbCancel,
	Vault,
	JumpDown,
	LandRecover,
}

struct ClimbData {
	wall_direction_x: f64,
	wall_direction_z: f64,
	base_x: f64,
	base_z: f64,
	base_y: f64,
	top_x: f64,
	top_z: f64,
	top_y: f64,
	attach_from_x: f64,
	attach_from_z: f64,
	attach_time: f64,
	vault_time: f64,
	vault_from_x: f64,
	vault_from_z: f64,
	vault_from_y: f64,
}

struct ClimbTarget {
	wall_x: f64,
	wall_z: f64,
	prompt_y: f64,
	forward_x: f64,
	forward_z: f64,
	top_x: f64,
	top_z: f64,
	top_y: f64,
}

struct DescendTarget {
	edge_x: f64,
	edge_z: f64,
	prompt_y: f64,
	forward_x: f64,
	forward_z: f64,
}

//==============================================================================
// 옵션. (토글은 부드럽게 보간)
//==============================================================================
const OPTION_LABEL_LIST: [&str; 7] = ["SHADOW", "BLOOM", "VIGNETTE", "FILM GRAIN", "COLLISION", "GLOW", "MOTION BLUR"];
const SHADOW_STATE_LABEL_LIST: [&str; 3] = ["OFF", "SIMPLE", "REALTIME"];
const OPTION_SHADOW: usize = 0;
const OPTION_GLOW: usize = 5;
const OPTION_MOTION_BLUR: usize = 6;

//==============================================================================
// initialize 에서 만드는 GL 리소스.
//==============================================================================
struct MountainResources {
	sky_pass: FullscreenPass,
	composite_pass: FullscreenPass,
	terrain_program: ShaderProgram,
	object_program: ShaderProgram,
	rock_program: ShaderProgram,
	depth_program: ShaderProgram,
	shadow_program: ShaderProgram,
	terrain_vertex_array: GlVertexArray,
	terrain_index_count: i32,
	cube_vertex_array: GlVertexArray,
	cube_index_count: i32,
	rock_variant_list: Vec<RockVariant>,
	static_shadow_vertex_array: GlVertexArray,
	static_shadow_index_count: i32,
	dynamic_shadow_vertex_array: GlVertexArray,
	dynamic_shadow_vertex_buffer: <glow::Context as glow::HasContext>::Buffer,
	grass_texture: GlTexture,
	rock_texture: GlTexture,
	dirt_texture: GlTexture,
	boulder_texture: GlTexture,
	skinned_model_renderer: SkinnedModelRenderer,
	character_model: SkinnedModel,
	fox_list: Vec<Fox>,
	scene_render_target: RenderTarget,
	bloom_effect: BloomEffect,
	shadow_map: ShadowMap,
	is_first_frame_checked: bool,
}

//==============================================================================
// 고대 산 씬.
//==============================================================================
struct AncientMountainScene {
	scene_base: SceneBase,
	character_load_handle: Option<SkinnedModelLoadHandle>,
	fox_load_handle: Option<SkinnedModelLoadHandle>,
	texture_asset_list: Vec<ImageAsset>,
	texture_description_handle_list: Vec<ImageDescriptionLoadHandle>,
	resources: Option<MountainResources>,
	load_error_message: Option<String>,
	world: World,
	rock_list: Vec<Rock>,
	hill_cleared_list: Vec<bool>,
	hill_cleared_pulse_time_list: Vec<f64>,
	hill_flag_base_height_list: Vec<f64>,
	hill_pole_matrix_list: Vec<Matrix4>,
	elapsed_time: f64,
	delta_time: f64,
	// 입력.
	is_mouse_dragging: bool,
	has_mouse_drag_moved: bool,
	drag_last: Option<Vector2>,
	// 터치. (원본 touchRouteMap, virtualPad, padInputX / Y, isTouchDevice)
	touch_route_list: Vec<(i64, TouchRoute)>,
	virtual_pad: VirtualPad,
	pad_input: Rc<Cell<(f64, f64)>>,
	is_touch_device: bool,
	is_pad_placed: bool,
	// 원본 HTML 오버레이(제목, 링크, 저작자 표기)를 구운 그림. (창 크기, 번짐 단계마다)
	overlay_image_table: HashMap<String, Rc<Image>>,
	overlay_key: String,
	backdrop_view: UIView,
	link_hit_list: Vec<(Rect, &'static str)>,
	pressed_link_index: Option<usize>,
	overlay_measure_canvas: Canvas2D,
	pending_drag_yaw_delta: f64,
	pending_drag_pitch_delta: f64,
	jump_pressed: bool,
	interact_pressed: bool,
	is_jump_button_held: bool,
	is_interact_button_held: bool,
	gamepad_move_x: f64,
	gamepad_move_y: f64,
	gamepad_camera_x: f64,
	gamepad_camera_y: f64,
	gamepad_sprint_held: bool,
	previous_gamepad_jump: bool,
	previous_gamepad_interact: bool,
	has_any_input_happened: bool,
	hint_fade_started: bool,
	hint_fade_start_time: f64,
	prompt_screen_position: Option<Vector2>,
	// 캐릭터.
	player_x: f64,
	player_z: f64,
	player_y: f64,
	character_yaw: f64,
	character_state: CharacterState,
	current_move_speed: f64,
	climb_phase: f64,
	climb_data: Option<ClimbData>,
	climb_available_target: Option<ClimbTarget>,
	descend_available_target: Option<DescendTarget>,
	jump_velocity_y: f64,
	jump_direction_x: f64,
	jump_direction_z: f64,
	jump_horizontal_speed: f64,
	land_recover_time: f64,
	landing_impulse: f64,
	current_move_input_x: f64,
	current_move_input_z: f64,
	current_move_direction_x: f64,
	current_move_direction_z: f64,
	cleared_summit_count: usize,
	all_cleared_time: f64,
	// 카메라.
	camera_yaw: f64,
	camera_pitch: f64,
	camera_yaw_target: f64,
	camera_pitch_target: f64,
	smoothed_pivot_y: f64,
	smoothed_camera_distance_factor: f64,
	smoothed_camera_distance: f64,
	camera_position: [f64; 3],
	view_projection_matrix: Matrix4,
	// 옵션.
	shadow_state: usize,
	option_state_list: [bool; 5],
	option_blend_list: [f64; 5],
	glow_value: f64,
	motion_blur_value: f64,
	active_slider_index: Option<usize>,
	is_panel_collapsed: bool,
	panel_collapse_blend: f64,
	motion_blur_current_strength: f64,
	shadow_realtime_blend: f64,
}

impl AncientMountainScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> AncientMountainScene {
		let world = World::new();
		let spawn_angle_radian = 30.0 * std::f64::consts::PI / 180.0;
		let player_x = spawn_angle_radian.cos() * PATH_RADIUS;
		let player_z = spawn_angle_radian.sin() * PATH_RADIUS;
		let player_y = world.terrain_surface_height_at(player_x, player_z);
		let hill_count = world.hill_count;
		AncientMountainScene {
			scene_base: SceneBase::new(),
			character_load_handle: None,
			fox_load_handle: None,
			texture_asset_list: Vec::new(),
			texture_description_handle_list: Vec::new(),
			resources: None,
			load_error_message: None,
			rock_list: Vec::new(),
			hill_cleared_list: vec![false; hill_count],
			hill_cleared_pulse_time_list: vec![-100.0; hill_count],
			hill_flag_base_height_list: Vec::new(),
			hill_pole_matrix_list: Vec::new(),
			world,
			elapsed_time: 0.0,
			delta_time: 0.0,
			is_mouse_dragging: false,
			has_mouse_drag_moved: false,
			drag_last: None,
			touch_route_list: Vec::new(),
			virtual_pad: VirtualPad::new(),
			pad_input: Rc::new(Cell::new((0.0, 0.0))),
			is_touch_device: false,
			is_pad_placed: false,
			overlay_image_table: HashMap::new(),
			overlay_key: String::new(),
			backdrop_view: UIView::new(),
			link_hit_list: Vec::new(),
			pressed_link_index: None,
			overlay_measure_canvas: Canvas2D::new(1, 1),
			pending_drag_yaw_delta: 0.0,
			pending_drag_pitch_delta: 0.0,
			jump_pressed: false,
			interact_pressed: false,
			is_jump_button_held: false,
			is_interact_button_held: false,
			gamepad_move_x: 0.0,
			gamepad_move_y: 0.0,
			gamepad_camera_x: 0.0,
			gamepad_camera_y: 0.0,
			gamepad_sprint_held: false,
			previous_gamepad_jump: false,
			previous_gamepad_interact: false,
			has_any_input_happened: false,
			hint_fade_started: false,
			hint_fade_start_time: 8.0,
			prompt_screen_position: None,
			player_x,
			player_z,
			player_y,
			character_yaw: spawn_angle_radian,
			character_state: CharacterState::Ground,
			current_move_speed: 0.0,
			climb_phase: 0.0,
			climb_data: None,
			climb_available_target: None,
			descend_available_target: None,
			jump_velocity_y: 0.0,
			jump_direction_x: 0.0,
			jump_direction_z: 0.0,
			jump_horizontal_speed: 0.0,
			land_recover_time: 0.0,
			landing_impulse: 0.0,
			current_move_input_x: 0.0,
			current_move_input_z: 0.0,
			current_move_direction_x: 0.0,
			current_move_direction_z: -1.0,
			cleared_summit_count: 0,
			all_cleared_time: -100.0,
			camera_yaw: spawn_angle_radian,
			camera_pitch: -0.08,
			camera_yaw_target: spawn_angle_radian,
			camera_pitch_target: -0.08,
			smoothed_pivot_y: player_y + 1.55,
			smoothed_camera_distance_factor: 1.0,
			smoothed_camera_distance: 3.6,
			camera_position: [0.0, 0.0, 0.0],
			view_projection_matrix: Matrix4::create_identity(),
			shadow_state: 2,
			option_state_list: [true, true, true, false, false],
			option_blend_list: [1.0, 1.0, 1.0, 1.0, 0.0],
			glow_value: 1.0,
			motion_blur_value: 0.35,
			active_slider_index: None,
			is_panel_collapsed: true,
			panel_collapse_blend: 0.0,
			motion_blur_current_strength: 0.0,
			shadow_realtime_blend: 1.0,
		}
	}

	//==============================================================================
	// 바위 배치. (결정적 난수, 작은 돌 위주 + 가끔 거석, 이동 장애물)
	//==============================================================================
	fn place_rocks(&mut self, variant_count: usize) {
		let mut rock_index: i32 = 0;
		while rock_index < 220 && self.rock_list.len() < 64 {
			let index = rock_index;
			rock_index += 1;
			let random_angle = lattice_random_at(index * 7 + 3, index * 13 + 5) * std::f64::consts::PI * 2.0;
			let random_radius = 12.0 + lattice_random_at(index * 11 + 1, index * 3 + 8) * 138.0;
			let rock_x = random_angle.cos() * random_radius;
			let rock_z = random_angle.sin() * random_radius;
			if (random_radius - PATH_RADIUS).abs() < 10.0 {
				continue;
			}
			let mut is_inside_hill = false;
			for hill in self.world.mountain_list.iter().take(self.world.hill_count) {
				let delta_x = rock_x - hill.center_x;
				let delta_z = rock_z - hill.center_z;
				if (delta_x * delta_x + delta_z * delta_z).sqrt() < hill.bound_radius + 3.0 {
					is_inside_hill = true;
				}
			}
			if is_inside_hill {
				continue;
			}
			let size_random = lattice_random_at(index * 5 + 2, index * 17 + 6);
			let rock_scale = 0.45 + size_random * size_random * 3.0;
			let rock_yaw = lattice_random_at(index * 19 + 4, index * 23 + 9) * std::f64::consts::PI * 2.0;
			let rock_tilt = (lattice_random_at(index * 31 + 8, index * 37 + 2) - 0.5) * 0.3;
			let tint_tone = 0.82 + lattice_random_at(index * 29 + 7, index * 31 + 11) * 0.36;
			let variant_index = ((lattice_random_at(index * 41 + 13, index * 43 + 17) * variant_count as f64).floor() as usize) % variant_count;
			let rock_height = self.world.terrain_surface_height_at(rock_x, rock_z);
			let mut rock_matrix = Matrix4::create_translation(rock_x, rock_height + rock_scale * 0.28, rock_z);
			rock_matrix.rotate_y(rock_yaw);
			rock_matrix.rotate_z(rock_tilt);
			rock_matrix.scale(rock_scale, rock_scale * (0.75 + size_random * 0.3), rock_scale);
			self.rock_list.push(Rock {
				matrix: rock_matrix,
				tint: [tint_tone as f32, tint_tone as f32, (tint_tone * 0.98) as f32],
				variant_index,
				center_x: rock_x,
				center_z: rock_z,
				block_radius: rock_scale * 0.95 + 0.3,
				top_height: rock_height + rock_scale * 1.15,
			});
		}
	}

	//==============================================================================
	// 깃발 기둥. (정적)
	//==============================================================================
	fn place_flags(&mut self) {
		for hill_index in 0..self.world.hill_count {
			let hill = &self.world.mountain_list[hill_index];
			let flag_base_height = self.world.terrain_surface_height_at(hill.summit_x, hill.summit_z);
			let mut pole_matrix = Matrix4::create_translation(hill.summit_x, flag_base_height + FLAG_POLE_HEIGHT * 0.5, hill.summit_z);
			pole_matrix.scale(0.1, FLAG_POLE_HEIGHT, 0.1);
			self.hill_flag_base_height_list.push(flag_base_height);
			self.hill_pole_matrix_list.push(pole_matrix);
		}
	}

	//==============================================================================
	// GL 리소스 만들기. (프로그램, 지형 메시, 큐브, 바위, 블롭 그림자, 텍스처, 모델, 렌더 타겟)
	//==============================================================================
	fn create_resources(&mut self, engine: &Engine, gl: &GlContext, engine_texture_list: &[Option<GlTexture>]) -> Result<MountainResources, String> {
		let sky_pass = FullscreenPass::new(gl, SKY_FRAGMENTSHADER_SOURCE)?;
		let composite_pass = FullscreenPass::new(gl, COMPOSITE_FRAGMENTSHADER_SOURCE)?;
		let terrain_fragment_source = format!("{}{}{}", TERRAIN_FRAGMENTSHADER_HEAD, SHADOW_SAMPLING_GLSL, TERRAIN_FRAGMENTSHADER_BODY);
		let terrain_program = ShaderProgram::new(gl, TERRAIN_VERTEXSHADER_SOURCE, &terrain_fragment_source)?;
		let object_fragment_source = format!("{}{}{}", OBJECT_FRAGMENTSHADER_HEAD, SHADOW_SAMPLING_GLSL, OBJECT_FRAGMENTSHADER_BODY);
		let object_program = ShaderProgram::new(gl, OBJECT_VERTEXSHADER_SOURCE, &object_fragment_source)?;
		let rock_fragment_source = format!("{}{}{}", ROCK_FRAGMENTSHADER_HEAD, SHADOW_SAMPLING_GLSL, ROCK_FRAGMENTSHADER_BODY);
		let rock_program = ShaderProgram::new(gl, OBJECT_VERTEXSHADER_SOURCE, &rock_fragment_source)?;
		let depth_program = ShaderProgram::new(gl, DEPTH_VERTEXSHADER_SOURCE, DEPTH_FRAGMENTSHADER_SOURCE)?;
		let shadow_program = ShaderProgram::create_with_attribute_locations(gl, SHADOW_VERTEXSHADER_SOURCE, SHADOW_FRAGMENTSHADER_SOURCE, &[("vertexShadow", 0)])?;

		// 지형 메시. (높이 그리드 → 정점, 노멀, 인덱스)
		let grid_side_cell_count = ((WORLD_HALF_SIZE * 2.0) / GRID_CELL_SIZE).round() as usize;
		let grid_side_vertex_count = grid_side_cell_count + 1;
		let mut height_grid = vec![0.0f32; grid_side_vertex_count * grid_side_vertex_count];
		for row_index in 0..grid_side_vertex_count {
			for column_index in 0..grid_side_vertex_count {
				let world_x = -WORLD_HALF_SIZE + column_index as f64 * GRID_CELL_SIZE;
				let world_z = -WORLD_HALF_SIZE + row_index as f64 * GRID_CELL_SIZE;
				height_grid[row_index * grid_side_vertex_count + column_index] = self.world.terrain_height_at(world_x, world_z) as f32;
			}
		}
		let mut terrain_position_list = vec![0.0f32; grid_side_vertex_count * grid_side_vertex_count * 3];
		let mut terrain_normal_list = vec![0.0f32; grid_side_vertex_count * grid_side_vertex_count * 3];
		for row_index in 0..grid_side_vertex_count {
			for column_index in 0..grid_side_vertex_count {
				let vertex_index = row_index * grid_side_vertex_count + column_index;
				terrain_position_list[vertex_index * 3] = (-WORLD_HALF_SIZE + column_index as f64 * GRID_CELL_SIZE) as f32;
				terrain_position_list[vertex_index * 3 + 1] = height_grid[vertex_index];
				terrain_position_list[vertex_index * 3 + 2] = (-WORLD_HALF_SIZE + row_index as f64 * GRID_CELL_SIZE) as f32;
				let left_column_index = column_index.saturating_sub(1);
				let right_column_index = (column_index + 1).min(grid_side_vertex_count - 1);
				let near_row_index = row_index.saturating_sub(1);
				let far_row_index = (row_index + 1).min(grid_side_vertex_count - 1);
				let height_left = height_grid[row_index * grid_side_vertex_count + left_column_index] as f64;
				let height_right = height_grid[row_index * grid_side_vertex_count + right_column_index] as f64;
				let height_near = height_grid[near_row_index * grid_side_vertex_count + column_index] as f64;
				let height_far = height_grid[far_row_index * grid_side_vertex_count + column_index] as f64;
				let slope_x = (height_right - height_left) / ((right_column_index - left_column_index) as f64 * GRID_CELL_SIZE);
				let slope_z = (height_far - height_near) / ((far_row_index - near_row_index) as f64 * GRID_CELL_SIZE);
				let normal_length = (slope_x * slope_x + 1.0 + slope_z * slope_z).sqrt();
				terrain_normal_list[vertex_index * 3] = (-slope_x / normal_length) as f32;
				terrain_normal_list[vertex_index * 3 + 1] = (1.0 / normal_length) as f32;
				terrain_normal_list[vertex_index * 3 + 2] = (-slope_z / normal_length) as f32;
			}
		}
		let mut terrain_index_bytes: Vec<u8> = Vec::with_capacity(grid_side_cell_count * grid_side_cell_count * 24);
		for row_index in 0..grid_side_cell_count {
			for column_index in 0..grid_side_cell_count {
				let top_left = (row_index * grid_side_vertex_count + column_index) as u32;
				let top_right = top_left + 1;
				let bottom_left = top_left + grid_side_vertex_count as u32;
				let bottom_right = bottom_left + 1;
				for index_value in [top_left, bottom_left, top_right, top_right, bottom_left, bottom_right] {
					terrain_index_bytes.extend_from_slice(&index_value.to_le_bytes());
				}
			}
		}
		let terrain_index_count = (grid_side_cell_count * grid_side_cell_count * 6) as i32;
		let terrain_vertex_array = create_vertex_array_with_normal(gl, &terrain_position_list, &terrain_normal_list, Some(&terrain_index_bytes))?;

		// 큐브 메시. (캐릭터 파츠, 바위, 깃발 공용, 중심 원점, 한 변 1)
		let face_list: [([f32; 3], [[f32; 3]; 4]); 6] = [
			([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
			([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]]),
			([1.0, 0.0, 0.0], [[0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]]),
			([-1.0, 0.0, 0.0], [[-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5]]),
			([0.0, 1.0, 0.0], [[-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5]]),
			([0.0, -1.0, 0.0], [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]]),
		];
		let mut cube_position_list: Vec<f32> = Vec::new();
		let mut cube_normal_list: Vec<f32> = Vec::new();
		let mut cube_index_bytes: Vec<u8> = Vec::new();
		for (face_index, (normal, corner_list)) in face_list.iter().enumerate() {
			let base_vertex_index = (face_index * 4) as u16;
			for corner in corner_list.iter() {
				cube_position_list.extend_from_slice(corner);
				cube_normal_list.extend_from_slice(normal);
			}
			for index_value in [0u16, 1, 2, 0, 2, 3] {
				cube_index_bytes.extend_from_slice(&(base_vertex_index + index_value).to_le_bytes());
			}
		}
		let cube_vertex_array = create_vertex_array_with_normal(gl, &cube_position_list, &cube_normal_list, Some(&cube_index_bytes))?;

		// 바위 정점 배열. (형태 시드가 다른 4종 변형)
		let mut rock_variant_list = Vec::new();
		for shape_seed in [3.1, 17.7, 42.3, 88.9] {
			let (position_list, normal_list) = create_rock_geometry(shape_seed);
			let vertex_array = create_vertex_array_with_normal(gl, &position_list, &normal_list, None)?;
			rock_variant_list.push(RockVariant { vertex_array, vertex_count: (position_list.len() / 3) as i32 });
		}
		self.place_rocks(rock_variant_list.len());
		self.place_flags();

		// 정적 블롭 그림자. (바위, 깃발 아래, 지형 표면에 맞춰 굽는다)
		let mut static_shadow_position_list: Vec<f32> = Vec::new();
		let mut static_shadow_index_bytes: Vec<u8> = Vec::new();
		let mut disc_list: Vec<(f64, f64, f64, f64)> = Vec::new();
		for rock in self.rock_list.iter() {
			disc_list.push((rock.center_x, rock.center_z, rock.block_radius * 1.05, 0.4));
		}
		for hill in self.world.mountain_list.iter().take(self.world.hill_count) {
			disc_list.push((hill.summit_x, hill.summit_z, 0.6, 0.3));
		}
		for (center_x, center_z, disc_radius, center_alpha) in disc_list {
			let base_vertex_index = (static_shadow_position_list.len() / 4) as u16;
			let center_height = self.world.terrain_surface_height_at(center_x, center_z);
			static_shadow_position_list.extend_from_slice(&[center_x as f32, (center_height + 0.14) as f32, center_z as f32, center_alpha as f32]);
			for segment_index in 0..STATIC_SHADOW_SEGMENT_COUNT {
				let segment_angle = (segment_index as f64 / STATIC_SHADOW_SEGMENT_COUNT as f64) * std::f64::consts::PI * 2.0;
				let segment_x = center_x + segment_angle.cos() * disc_radius;
				let segment_z = center_z + segment_angle.sin() * disc_radius;
				let segment_height = self.world.terrain_surface_height_at(segment_x, segment_z);
				static_shadow_position_list.extend_from_slice(&[segment_x as f32, (segment_height + 0.14) as f32, segment_z as f32, 0.0]);
				for index_value in [base_vertex_index, base_vertex_index + 1 + segment_index as u16, base_vertex_index + 1 + ((segment_index + 1) % STATIC_SHADOW_SEGMENT_COUNT) as u16] {
					static_shadow_index_bytes.extend_from_slice(&index_value.to_le_bytes());
				}
			}
		}
		let static_shadow_index_count = (static_shadow_index_bytes.len() / 2) as i32;
		let mut dynamic_shadow_index_bytes: Vec<u8> = Vec::new();
		for segment_index in 0..SHADOW_SEGMENT_COUNT {
			for index_value in [0u16, 1 + segment_index as u16, 1 + ((segment_index + 1) % SHADOW_SEGMENT_COUNT) as u16] {
				dynamic_shadow_index_bytes.extend_from_slice(&index_value.to_le_bytes());
			}
		}
		let (static_shadow_vertex_array, dynamic_shadow_vertex_array, dynamic_shadow_vertex_buffer) = unsafe {
			let static_shadow_vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(static_shadow_vertex_array));
			let static_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(static_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(&static_shadow_position_list), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(0);
			gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, 0, 0);
			let static_index_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(static_index_buffer));
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &static_shadow_index_bytes, glow::STATIC_DRAW);

			let dynamic_shadow_vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(dynamic_shadow_vertex_array));
			let dynamic_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(dynamic_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(&vec![0.0f32; (SHADOW_SEGMENT_COUNT + 1) * 4]), glow::DYNAMIC_DRAW);
			gl.enable_vertex_attrib_array(0);
			gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, 0, 0);
			let dynamic_index_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(dynamic_index_buffer));
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &dynamic_shadow_index_bytes, glow::STATIC_DRAW);
			gl.bind_vertex_array(None);
			(static_shadow_vertex_array, dynamic_shadow_vertex_array, dynamic_buffer)
		};

		// 텍스처. (미러 반복 랩 모드, 어떤 이미지든 이음매 없이 타일링)
		// - 머티리얼의 이미지 해석 길은 JPEG 를 풀지 못하므로(image 크레이트에 jpeg 기능이 꺼져 있다) 엔진 ImageAsset 이 푼 그림(데스크탑은 엔진 JPEG 해독기, 웹은 브라우저)의 픽셀로 넘긴다.
		let mut texture_list: Vec<GlTexture> = Vec::new();
		for (texture_index, texture_name) in TEXTURE_NAME_LIST.iter().enumerate() {
			let texture = match engine_texture_list[texture_index] {
				// 웹: 브라우저가 푼 그림은 화소를 엔진이 들고 있지 않으니(GPU 에 올린 뒤 놓는다) 엔진이 올린 텍스처를 그대로 쓴다.
				Some(engine_texture) => {
					engine_texture
				}
				None => {
					// 웹에서 그림을 못 받았거나(주소가 없음) 풀지 못했으면 설명 손잡이가 없다. 패닉 대신 까닭을 돌려준다.
					let description_handle = self.texture_description_handle_list.get(texture_index).ok_or_else(|| format!("텍스처를 받지 못함: {}", texture_name))?;
					let image_description = description_handle.get_image_description().ok_or_else(|| format!("텍스처를 아직 받는 중: {}", texture_name))??;
					Material::create_texture(gl, Some(&image_description), [128, 128, 128, 255])?
				}
			};
			unsafe {
				gl.bind_texture(glow::TEXTURE_2D, Some(texture));
				// 웹: 엔진이 올린 텍스처에도 원본 loadTexture 처럼 밉맵과 LINEAR_MIPMAP_LINEAR 을 건다. (데스크탑은 create_texture 가 했다)
				if cfg!(target_arch = "wasm32") {
					gl.generate_mipmap(glow::TEXTURE_2D);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
					gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
					if gl.supported_extensions().iter().any(|extension_name| extension_name.contains("texture_filter_anisotropic")) {
						let maximum_anisotropy = gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT);
						gl.tex_parameter_f32(glow::TEXTURE_2D, glow::TEXTURE_MAX_ANISOTROPY_EXT, maximum_anisotropy.min(8.0));
					}
				}
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::MIRRORED_REPEAT as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::MIRRORED_REPEAT as i32);
				gl.bind_texture(glow::TEXTURE_2D, None);
			}
			texture_list.push(texture);
		}

		// 스킨드 모델 렌더러와 캐릭터, 여우.
		let mut skinned_model_renderer = SkinnedModelRenderer::new(gl, None, None)?;
		let sun_direction = Vector3::create(-0.42, 0.62, -0.35).normalize();
		skinned_model_renderer.set_sun_direction(sun_direction.x, sun_direction.y, sun_direction.z);
		skinned_model_renderer.set_fog_color(0.8, 0.85, 0.92);
		skinned_model_renderer.set_fog_range(55.0, 250.0);
		let mut character_model = self.character_load_handle.as_ref().and_then(|load_handle| load_handle.create_model(gl, &skinned_model_renderer)).ok_or("마네킹을 아직 받는 중")??;
		character_model.add_animations_from_fbx_url(engine, "anims/idle.fbx", "Idle", false);
		character_model.add_animations_from_fbx_url(engine, "anims/walk.fbx", "Walk", false);
		character_model.add_animations_from_fbx_url(engine, "anims/run.fbx", "Run", false);
		character_model.add_animations_from_fbx_url(engine, "anims/fall.fbx", "Fall", false);
		character_model.add_animations_from_fbx_url(engine, "anims/land.fbx", "Land", false);
		character_model.add_animations_from_fbx_url(engine, "anims/climb.fbx", "Climb", true);
		character_model.play_animation("Idle", 0.0);

		let fox_model = self.fox_load_handle.as_ref().and_then(|load_handle| load_handle.create_model(gl, &skinned_model_renderer)).ok_or("여우를 아직 받는 중")??;
		let fox_setup_list: [(f64, f64, f64, f64); 6] = [
			(62.0, 0.02, 0.8, 0.007),
			(78.0, -0.016, 3.6, 0.0062),
			(46.0, 0.024, 2.1, 0.0075),
			(88.0, 0.014, 5.2, 0.0066),
			(55.0, -0.021, 4.4, 0.0058),
			(70.0, -0.018, 1.5, 0.0071),
		];
		let mut fox_list: Vec<Fox> = Vec::new();
		let mut first_fox_model = Some(fox_model);
		for (fox_index, setup) in fox_setup_list.iter().enumerate() {
			let mut model = match first_fox_model.take() {
				Some(first_model) => {
					first_model
				}
				None => {
					fox_list[0].model.create_instance()?
				}
			};
			model.play_animation("Walk", 0.0);
			model.update(fox_index as f64 * 0.37);
			fox_list.push(Fox {
				model,
				orbit_radius: setup.0,
				orbit_speed: setup.1,
				angle_phase: setup.2,
				body_scale: setup.3,
				current_x: 0.0,
				current_y: 0.0,
				current_z: 0.0,
				current_yaw: 0.0,
				block_radius: 0.0,
				top_height: 0.0,
			});
		}

		let scene_render_target = RenderTarget::new(gl, 2, 2, true, None)?;
		let mut bloom_effect = BloomEffect::new(gl)?;
		bloom_effect.set_threshold(0.74, 0.97);
		let shadow_map = ShadowMap::new(gl, SHADOWMAP_RESOLUTION)?;
		self.update_fox_positions(&mut fox_list);

		Ok(MountainResources {
			sky_pass,
			composite_pass,
			terrain_program,
			object_program,
			rock_program,
			depth_program,
			shadow_program,
			terrain_vertex_array,
			terrain_index_count,
			cube_vertex_array,
			cube_index_count: 36,
			rock_variant_list,
			static_shadow_vertex_array,
			static_shadow_index_count,
			dynamic_shadow_vertex_array,
			dynamic_shadow_vertex_buffer,
			grass_texture: texture_list[0],
			rock_texture: texture_list[1],
			dirt_texture: texture_list[2],
			boulder_texture: texture_list[3],
			skinned_model_renderer,
			character_model,
			fox_list,
			scene_render_target,
			bloom_effect,
			shadow_map,
			is_first_frame_checked: false,
		})
	}

	//==============================================================================
	// 여우 위치 갱신. (이동 충돌 판정보다 먼저, 동물도 장애물로 취급)
	//==============================================================================
	fn update_fox_positions(&self, fox_list: &mut [Fox]) {
		for fox in fox_list.iter_mut() {
			let orbit_angle = fox.angle_phase + self.elapsed_time * fox.orbit_speed;
			fox.current_x = orbit_angle.cos() * fox.orbit_radius;
			fox.current_z = orbit_angle.sin() * fox.orbit_radius;
			fox.current_y = self.world.terrain_surface_height_at(fox.current_x, fox.current_z);
			let tangent_sign = if fox.orbit_speed >= 0.0 { 1.0 } else { -1.0 };
			let velocity_x = -orbit_angle.sin() * tangent_sign;
			let velocity_z = orbit_angle.cos() * tangent_sign;
			fox.current_yaw = velocity_x.atan2(-velocity_z);
			fox.block_radius = fox.body_scale * 70.0;
			fox.top_height = fox.current_y + fox.body_scale * 85.0;
		}
	}

	//==============================================================================
	// 등반 대상 탐지. (시야 방향 전방의 절벽을 지형 데이터에서 직접 검출)
	//==============================================================================
	fn detect_climb_target(&self) -> Option<ClimbTarget> {
		let forward_x = self.camera_yaw.sin();
		let forward_z = -self.camera_yaw.cos();
		let mut reach_distance = 0.9;
		while reach_distance <= 2.5 + 1e-9 {
			let wall_x = self.player_x + forward_x * reach_distance;
			let wall_z = self.player_z + forward_z * reach_distance;
			let wall_height = self.world.terrain_height_at(wall_x, wall_z);
			if wall_height - self.player_y > 2.0 {
				let top_distance = reach_distance + 6.0;
				let top_x = self.player_x + forward_x * top_distance;
				let top_z = self.player_z + forward_z * top_distance;
				let top_height = self.world.terrain_height_at(top_x, top_z);
				let rise_amount = top_height - self.player_y;
				let beyond_x = top_x + forward_x * 2.2;
				let beyond_z = top_z + forward_z * 2.2;
				let beyond_height = self.world.terrain_height_at(beyond_x, beyond_z);
				let is_top_flat = (beyond_height - top_height).abs() < 1.6;
				if rise_amount > 2.0 && rise_amount < 18.0 && is_top_flat {
					return Some(ClimbTarget {
						wall_x,
						wall_z,
						prompt_y: ((self.player_y + top_height) * 0.5).min(self.player_y + 2.2),
						forward_x,
						forward_z,
						top_x,
						top_z,
						top_y: top_height,
					});
				}
			}
			reach_distance += 0.4;
		}
		None
	}

	//==============================================================================
	// 뛰어내리기 대상 탐지. (시야 전방의 낙하 가장자리를 지형 데이터에서 검출)
	//==============================================================================
	fn detect_descend_target(&self) -> Option<DescendTarget> {
		let forward_x = self.camera_yaw.sin();
		let forward_z = -self.camera_yaw.cos();
		let mut edge_distance = 1.0;
		while edge_distance <= 4.0 + 1e-9 {
			let edge_x = self.player_x + forward_x * edge_distance;
			let edge_z = self.player_z + forward_z * edge_distance;
			let edge_height = self.world.terrain_height_at(edge_x, edge_z);
			let drop_amount = self.player_y - edge_height;
			if drop_amount > 2.2 && drop_amount < 30.0 {
				let landing_x = self.player_x + forward_x * (edge_distance + 2.2);
				let landing_z = self.player_z + forward_z * (edge_distance + 2.2);
				let landing_height = self.world.terrain_height_at(landing_x, landing_z);
				if self.player_y - landing_height > 2.0 {
					return Some(DescendTarget { edge_x, edge_z, prompt_y: self.player_y - 0.4, forward_x, forward_z });
				}
			}
			edge_distance += 0.5;
		}
		None
	}

	//==============================================================================
	// 등반 벽면 밀착 보정. (등반 중 캐릭터 박스가 절벽, 경사면과 겹치지 않게)
	//==============================================================================
	fn resolve_climb_wall_contact(&mut self, wall_direction_x: f64, wall_direction_z: f64) {
		const STEP_DISTANCE: f64 = 0.12;
		for _ in 0..14 {
			let box_terrain_height = self.world.sample_body_box_terrain_height(self.player_x, self.player_z, wall_direction_x, wall_direction_z);
			if box_terrain_height <= self.player_y {
				break;
			}
			self.player_x -= wall_direction_x * STEP_DISTANCE;
			self.player_z -= wall_direction_z * STEP_DISTANCE;
		}
		for _ in 0..10 {
			let ahead_x = self.player_x + wall_direction_x * STEP_DISTANCE;
			let ahead_z = self.player_z + wall_direction_z * STEP_DISTANCE;
			let ahead_box_height = self.world.sample_body_box_terrain_height(ahead_x, ahead_z, wall_direction_x, wall_direction_z);
			if ahead_box_height > self.player_y {
				break;
			}
			self.player_x = ahead_x;
			self.player_z = ahead_z;
		}
		let remaining_terrain_height = self.world.sample_body_box_terrain_height(self.player_x, self.player_z, wall_direction_x, wall_direction_z);
		if remaining_terrain_height > self.player_y {
			self.player_y = remaining_terrain_height;
		}
	}

	//==============================================================================
	// 캐릭터 갱신. (fox_position_list 는 여우 충돌 판정용 (x, z, 막는 반지름, 윗면 높이))
	//==============================================================================
	fn update_character(&mut self, delta_time: f64, key_list: &KeyState, fox_block_list: &[(f64, f64, f64, f64)]) {
		self.current_move_input_x = 0.0;
		self.current_move_input_z = 0.0;
		let elapsed_time = self.elapsed_time;
		match self.character_state {
			CharacterState::Ground => {
				let mut move_input_x = 0.0;
				let mut move_input_z = 0.0;
				if key_list.move_right {
					move_input_x += 1.0;
				}
				if key_list.move_left {
					move_input_x -= 1.0;
				}
				if key_list.move_forward {
					move_input_z += 1.0;
				}
				if key_list.move_back {
					move_input_z -= 1.0;
				}
				let (pad_input_x, pad_input_y) = self.pad_input.get();
				move_input_x += pad_input_x + self.gamepad_move_x;
				move_input_z += -pad_input_y - self.gamepad_move_y;
				let input_length = (move_input_x * move_input_x + move_input_z * move_input_z).sqrt();
				let input_strength = input_length.min(1.0);
				self.current_move_input_x = move_input_x.clamp(-1.0, 1.0);
				self.current_move_input_z = move_input_z.clamp(-1.0, 1.0);
				if input_strength > 0.05 {
					let camera_forward_x = self.camera_yaw.sin();
					let camera_forward_z = -self.camera_yaw.cos();
					let camera_right_x = self.camera_yaw.cos();
					let camera_right_z = self.camera_yaw.sin();
					let move_direction_x = (camera_forward_x * move_input_z + camera_right_x * move_input_x) / input_length;
					let move_direction_z = (camera_forward_z * move_input_z + camera_right_z * move_input_x) / input_length;
					self.current_move_direction_x = move_direction_x;
					self.current_move_direction_z = move_direction_z;
					let target_yaw = move_direction_x.atan2(-move_direction_z);
					let yaw_difference = wrap_angle(target_yaw - self.character_yaw);
					self.character_yaw += yaw_difference * (6.5 * delta_time).min(1.0);
				}
				let pad_strength = (pad_input_x * pad_input_x + pad_input_y * pad_input_y).sqrt();
				let gamepad_strength = (self.gamepad_move_x * self.gamepad_move_x + self.gamepad_move_y * self.gamepad_move_y).sqrt();
				let is_sprinting = key_list.sprint || self.gamepad_sprint_held || pad_strength > 0.97 || gamepad_strength > 0.97;
				let target_move_speed = (if is_sprinting { SPRINT_SPEED } else { RUN_SPEED }) * input_strength;
				let acceleration_rate = if target_move_speed > self.current_move_speed { 5.5 } else { 7.5 };
				self.current_move_speed += (target_move_speed - self.current_move_speed) * (acceleration_rate * delta_time).min(1.0);

				if self.current_move_speed > 0.05 {
					let heading_x = self.current_move_direction_x;
					let heading_z = self.current_move_direction_z;
					let step_length = self.current_move_speed * delta_time;
					let current_height = self.world.terrain_surface_height_at(self.player_x, self.player_z);
					let mut candidate_x = self.player_x + heading_x * step_length;
					let mut candidate_z = self.player_z + heading_z * step_length;
					let mut candidate_height = self.world.terrain_surface_height_at(candidate_x, candidate_z);
					let rise_rate = (candidate_height - current_height) / step_length.max(0.0001);
					if rise_rate.abs() > 1.15 {
						candidate_x = self.player_x + heading_x * step_length;
						candidate_z = self.player_z;
						candidate_height = self.world.terrain_surface_height_at(candidate_x, candidate_z);
						let rise_rate_axis_x = (candidate_height - current_height) / step_length.max(0.0001);
						if rise_rate_axis_x.abs() > 1.15 {
							candidate_x = self.player_x;
							candidate_z = self.player_z + heading_z * step_length;
							candidate_height = self.world.terrain_surface_height_at(candidate_x, candidate_z);
							let rise_rate_axis_z = (candidate_height - current_height) / step_length.max(0.0001);
							if rise_rate_axis_z.abs() > 1.15 {
								candidate_x = self.player_x;
								candidate_z = self.player_z;
								self.current_move_speed *= 0.5;
							}
						}
					}
					for rock in self.rock_list.iter() {
						let rock_delta_x = candidate_x - rock.center_x;
						let rock_delta_z = candidate_z - rock.center_z;
						let rock_distance = (rock_delta_x * rock_delta_x + rock_delta_z * rock_delta_z).sqrt();
						if rock_distance > 0.001 && rock_distance < rock.block_radius {
							candidate_x = rock.center_x + (rock_delta_x / rock_distance) * rock.block_radius;
							candidate_z = rock.center_z + (rock_delta_z / rock_distance) * rock.block_radius;
						}
					}
					for (fox_x, fox_z, fox_block_radius, _) in fox_block_list.iter() {
						let fox_delta_x = candidate_x - fox_x;
						let fox_delta_z = candidate_z - fox_z;
						let fox_distance = (fox_delta_x * fox_delta_x + fox_delta_z * fox_delta_z).sqrt();
						if fox_distance > 0.001 && fox_distance < *fox_block_radius {
							candidate_x = fox_x + (fox_delta_x / fox_distance) * fox_block_radius;
							candidate_z = fox_z + (fox_delta_z / fox_distance) * fox_block_radius;
						}
					}
					let candidate_center_distance = (candidate_x * candidate_x + candidate_z * candidate_z).sqrt();
					if candidate_center_distance > 175.0 {
						candidate_x *= 175.0 / candidate_center_distance;
						candidate_z *= 175.0 / candidate_center_distance;
					}
					self.player_x = candidate_x;
					self.player_z = candidate_z;
				}
				self.player_y = self.world.terrain_surface_height_at(self.player_x, self.player_z);

				// 정상 도달 판정.
				for hill_index in 0..self.world.hill_count {
					if !self.hill_cleared_list[hill_index] {
						let hill = &self.world.mountain_list[hill_index];
						let summit_delta_x = self.player_x - hill.summit_x;
						let summit_delta_z = self.player_z - hill.summit_z;
						let summit_distance = (summit_delta_x * summit_delta_x + summit_delta_z * summit_delta_z).sqrt();
						if summit_distance < (hill.summit_radius * 0.2).max(4.0) && self.player_y > hill.summit_height * 0.78 {
							self.hill_cleared_list[hill_index] = true;
							self.hill_cleared_pulse_time_list[hill_index] = elapsed_time;
							self.cleared_summit_count += 1;
							if self.cleared_summit_count == self.world.hill_count {
								self.all_cleared_time = elapsed_time;
							}
						}
					}
				}

				// 등반 시작. (O, 상호작용)
				self.climb_available_target = self.detect_climb_target();
				if self.climb_available_target.is_some() && self.interact_pressed {
					let target = self.climb_available_target.take().unwrap();
					self.climb_data = Some(ClimbData {
						wall_direction_x: target.forward_x,
						wall_direction_z: target.forward_z,
						base_x: target.wall_x - target.forward_x * 0.95,
						base_z: target.wall_z - target.forward_z * 0.95,
						base_y: self.player_y,
						top_x: target.top_x + target.forward_x * 0.3,
						top_z: target.top_z + target.forward_z * 0.3,
						top_y: target.top_y,
						attach_from_x: self.player_x,
						attach_from_z: self.player_z,
						attach_time: 0.0,
						vault_time: 0.0,
						vault_from_x: 0.0,
						vault_from_z: 0.0,
						vault_from_y: 0.0,
					});
					self.character_yaw = target.forward_x.atan2(-target.forward_z);
					self.character_state = CharacterState::ClimbAttach;
					self.climb_phase = 0.0;
				}

				// 뛰어내리기. (등반 대상이 없고 아래를 내려보는 시야일 때)
				self.descend_available_target = None;
				if self.character_state == CharacterState::Ground && self.climb_available_target.is_none() && self.camera_pitch < -0.08 {
					self.descend_available_target = self.detect_descend_target();
				}
				if self.descend_available_target.is_some() && self.interact_pressed {
					let target = self.descend_available_target.take().unwrap();
					self.jump_direction_x = target.forward_x;
					self.jump_direction_z = target.forward_z;
					self.jump_horizontal_speed = 5.4;
					self.jump_velocity_y = 3.0;
					self.character_yaw = target.forward_x.atan2(-target.forward_z);
					self.character_state = CharacterState::JumpDown;
				}

				// 점프. (X, 좌클릭, 달리던 관성을 유지한 채 도약)
				if self.jump_pressed && self.character_state == CharacterState::Ground {
					self.jump_direction_x = self.current_move_direction_x;
					self.jump_direction_z = self.current_move_direction_z;
					self.jump_horizontal_speed = self.current_move_speed;
					self.jump_velocity_y = JUMP_VELOCITY;
					self.character_state = CharacterState::JumpDown;
				}
			}
			CharacterState::ClimbAttach => {
				let mut climb_data = self.climb_data.take().unwrap();
				climb_data.attach_time += delta_time;
				let attach_progress = (climb_data.attach_time / CLIMB_ATTACH_DURATION).min(1.0);
				self.player_x = climb_data.attach_from_x + (climb_data.base_x - climb_data.attach_from_x) * attach_progress;
				self.player_z = climb_data.attach_from_z + (climb_data.base_z - climb_data.attach_from_z) * attach_progress;
				self.player_y = climb_data.base_y;
				self.resolve_climb_wall_contact(climb_data.wall_direction_x, climb_data.wall_direction_z);
				if attach_progress >= 1.0 {
					self.character_state = CharacterState::Climbing;
				}
				self.climb_data = Some(climb_data);
			}
			CharacterState::Climbing => {
				let mut climb_data = self.climb_data.take().unwrap();
				self.climb_phase += delta_time * 3.4;
				self.player_y += CLIMB_ASCEND_SPEED * delta_time;
				let climb_span = (climb_data.top_y - climb_data.base_y).max(0.001);
				let climb_progress = ((self.player_y - climb_data.base_y) / climb_span).min(1.0);
				let rim_base_x = climb_data.top_x - climb_data.wall_direction_x * 1.5;
				let rim_base_z = climb_data.top_z - climb_data.wall_direction_z * 1.5;
				self.player_x = climb_data.base_x + (rim_base_x - climb_data.base_x) * climb_progress;
				self.player_z = climb_data.base_z + (rim_base_z - climb_data.base_z) * climb_progress;
				self.resolve_climb_wall_contact(climb_data.wall_direction_x, climb_data.wall_direction_z);
				if self.interact_pressed {
					self.character_state = CharacterState::ClimbCancel;
				}
				else if self.player_y >= climb_data.top_y - 0.9 {
					climb_data.vault_time = 0.0;
					climb_data.vault_from_x = self.player_x;
					climb_data.vault_from_z = self.player_z;
					climb_data.vault_from_y = self.player_y;
					self.character_state = CharacterState::Vault;
				}
				self.climb_data = Some(climb_data);
			}
			CharacterState::ClimbCancel => {
				let climb_data = self.climb_data.take().unwrap();
				self.climb_phase -= delta_time * 3.0;
				self.player_y -= CLIMB_DESCEND_SPEED * delta_time;
				let climb_span = (climb_data.top_y - climb_data.base_y).max(0.001);
				let climb_progress = ((self.player_y - climb_data.base_y) / climb_span).max(0.0);
				let rim_base_x = climb_data.top_x - climb_data.wall_direction_x * 1.5;
				let rim_base_z = climb_data.top_z - climb_data.wall_direction_z * 1.5;
				self.player_x = climb_data.base_x + (rim_base_x - climb_data.base_x) * climb_progress;
				self.player_z = climb_data.base_z + (rim_base_z - climb_data.base_z) * climb_progress;
				self.resolve_climb_wall_contact(climb_data.wall_direction_x, climb_data.wall_direction_z);
				if self.player_y <= climb_data.base_y {
					self.player_y = climb_data.base_y;
					self.character_state = CharacterState::Ground;
				}
				else {
					self.climb_data = Some(climb_data);
				}
			}
			CharacterState::Vault => {
				let mut climb_data = self.climb_data.take().unwrap();
				climb_data.vault_time += delta_time;
				let vault_progress = (climb_data.vault_time / VAULT_DURATION).min(1.0);
				let eased_progress = vault_progress * vault_progress * (3.0 - 2.0 * vault_progress);
				self.player_x = climb_data.vault_from_x + (climb_data.top_x - climb_data.vault_from_x) * eased_progress;
				self.player_z = climb_data.vault_from_z + (climb_data.top_z - climb_data.vault_from_z) * eased_progress;
				let vault_target_y = self.world.terrain_surface_height_at(climb_data.top_x, climb_data.top_z);
				let vault_arc = (vault_progress * std::f64::consts::PI).sin() * 0.35;
				self.player_y = climb_data.vault_from_y + (vault_target_y - climb_data.vault_from_y) * eased_progress + vault_arc;
				if vault_progress >= 1.0 {
					self.player_y = vault_target_y;
					self.character_state = CharacterState::Ground;
				}
				else {
					self.climb_data = Some(climb_data);
				}
			}
			CharacterState::JumpDown => {
				let horizontal_step = self.jump_horizontal_speed * delta_time;
				let ahead_x = self.player_x + self.jump_direction_x * horizontal_step;
				let ahead_z = self.player_z + self.jump_direction_z * horizontal_step;
				let ahead_height = self.world.terrain_surface_height_at(ahead_x, ahead_z);
				let mut is_blocked = false;
				for rock in self.rock_list.iter() {
					let rock_delta_x = ahead_x - rock.center_x;
					let rock_delta_z = ahead_z - rock.center_z;
					let rock_distance = (rock_delta_x * rock_delta_x + rock_delta_z * rock_delta_z).sqrt();
					if rock_distance < rock.block_radius && self.player_y < rock.top_height {
						is_blocked = true;
					}
				}
				for (fox_x, fox_z, fox_block_radius, fox_top_height) in fox_block_list.iter() {
					let fox_delta_x = ahead_x - fox_x;
					let fox_delta_z = ahead_z - fox_z;
					let fox_distance = (fox_delta_x * fox_delta_x + fox_delta_z * fox_delta_z).sqrt();
					if fox_distance < *fox_block_radius && self.player_y < *fox_top_height {
						is_blocked = true;
					}
				}
				if !is_blocked && ahead_height < self.player_y + 0.4 {
					self.player_x = ahead_x;
					self.player_z = ahead_z;
				}
				self.jump_velocity_y -= 16.0 * delta_time;
				self.jump_velocity_y = self.jump_velocity_y.max(-26.0);
				self.player_y += self.jump_velocity_y * delta_time;
				let ground_height = self.world.terrain_surface_height_at(self.player_x, self.player_z);
				if self.player_y <= ground_height {
					self.player_y = ground_height;
					self.landing_impulse = (-self.jump_velocity_y / 16.0).min(1.0);
					self.land_recover_time = 0.0;
					self.character_state = CharacterState::LandRecover;
				}
			}
			CharacterState::LandRecover => {
				self.land_recover_time += delta_time;
				self.player_y = self.world.terrain_surface_height_at(self.player_x, self.player_z);
				if self.land_recover_time >= 0.42 {
					self.character_state = CharacterState::Ground;
				}
			}
		}
	}

	//==============================================================================
	// 캐릭터 애니메이션. (상태 → 스킨드 모델 클립)
	//==============================================================================
	fn update_character_animation(&mut self, delta_time: f64) {
		let character_state = self.character_state;
		let current_move_speed = self.current_move_speed;
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		let model = &mut resources.character_model;
		match character_state {
			CharacterState::Ground => {
				if current_move_speed < 0.15 {
					model.play_animation("Idle", 0.25);
					model.set_time_scale(1.0);
				}
				else if current_move_speed < 3.0 {
					model.play_animation("Walk", 0.2);
					model.set_time_scale((current_move_speed / 1.7).max(0.6));
				}
				else {
					model.play_animation("Run", 0.2);
					model.set_time_scale(current_move_speed / 5.0);
				}
			}
			CharacterState::ClimbAttach | CharacterState::Climbing | CharacterState::ClimbCancel => {
				model.play_animation("Climb", 0.2);
				model.set_time_scale(if character_state == CharacterState::Climbing { 1.0 } else { 0.5 });
			}
			CharacterState::JumpDown => {
				model.play_animation("Fall", 0.08);
				model.set_time_scale(1.0);
			}
			CharacterState::LandRecover => {
				model.play_animation("Land", 0.0);
				model.set_time_scale(1.8);
			}
			CharacterState::Vault => {
				model.play_animation("Idle", 0.2);
				model.set_time_scale(1.0);
			}
		}
		model.update(delta_time);
	}

	//==============================================================================
	// 패널.
	//==============================================================================
	fn calculate_panel_layout(&self, window_width: f64) -> (f64, f64, f64) {
		let panel_x = window_width - PANEL_MARGIN_RIGHT - PANEL_WIDTH;
		let list_height = (OPTION_LABEL_LIST.len() as f64 * PANEL_ROW_HEIGHT + 10.0) * self.panel_collapse_blend;
		(panel_x, 18.0, PANEL_HEADER_HEIGHT + list_height)
	}

	fn slider_track_x(panel_x: f64) -> f64 {
		panel_x + PANEL_WIDTH - 16.0 - SLIDER_TRACK_WIDTH
	}

	fn update_slider_from_x(&mut self, client_x: f64, window_width: f64) {
		if let Some(option_index) = self.active_slider_index {
			let (panel_x, _, _) = self.calculate_panel_layout(window_width);
			let normalized = ((client_x - AncientMountainScene::slider_track_x(panel_x)) / SLIDER_TRACK_WIDTH).clamp(0.0, 1.0);
			if option_index == OPTION_GLOW {
				self.glow_value = 1.5 * normalized;
			}
			else {
				self.motion_blur_value = normalized;
			}
		}
	}

	fn handle_panel_press(&mut self, client_x: f64, client_y: f64, window_width: f64) -> bool {
		let (panel_x, panel_y, panel_height) = self.calculate_panel_layout(window_width);
		if client_x < panel_x || client_x > panel_x + PANEL_WIDTH || client_y < panel_y || client_y > panel_y + panel_height {
			return false;
		}
		if client_y < panel_y + PANEL_HEADER_HEIGHT {
			self.is_panel_collapsed = !self.is_panel_collapsed;
			return true;
		}
		if self.panel_collapse_blend > 0.9 {
			let row_index = ((client_y - panel_y - PANEL_HEADER_HEIGHT - 4.0) / PANEL_ROW_HEIGHT).floor();
			if row_index >= 0.0 && (row_index as usize) < OPTION_LABEL_LIST.len() {
				let row_index = row_index as usize;
				if row_index == OPTION_GLOW || row_index == OPTION_MOTION_BLUR {
					self.active_slider_index = Some(row_index);
					self.update_slider_from_x(client_x, window_width);
				}
				else if row_index == OPTION_SHADOW {
					self.shadow_state = (self.shadow_state + 1) % SHADOW_STATE_LABEL_LIST.len();
				}
				else {
					let state_index = row_index - 1;
					self.option_state_list[state_index] = !self.option_state_list[state_index];
				}
			}
		}
		true
	}

	fn jump_button_center(window_size: Vector2) -> Vector2 {
		Vector2::create(window_size.x - 128.0, window_size.y - 96.0)
	}

	fn interact_button_center(window_size: Vector2) -> Vector2 {
		Vector2::create(window_size.x - 62.0, window_size.y - 160.0)
	}

	fn hit_test_circle(position: Vector2, center: Vector2, radius: f64) -> bool {
		let delta_x = position.x - center.x;
		let delta_y = position.y - center.y;
		delta_x * delta_x + delta_y * delta_y <= radius * radius
	}

	//==============================================================================
	// 월드 좌표 → CSS 픽셀 화면 좌표.
	//==============================================================================
	fn project_world_to_screen(&self, world_x: f64, world_y: f64, world_z: f64, window_size: Vector2) -> Option<Vector2> {
		let clip_position = self.view_projection_matrix.transform_point(world_x, world_y, world_z);
		if clip_position.w <= 0.001 {
			return None;
		}
		let normalized_x = clip_position.x / clip_position.w;
		let normalized_y = clip_position.y / clip_position.w;
		Some(Vector2::create((normalized_x * 0.5 + 0.5) * window_size.x, (1.0 - (normalized_y * 0.5 + 0.5)) * window_size.y))
	}

	//==============================================================================
	// 3D 그리기. (그림자 → 하늘 → 지형 → 바위 → 오브젝트 → 모델 → 블롭 그림자 → 블룸 → 합성)
	//==============================================================================
	fn render_scene(&mut self, gl: &GlContext, width: i32, height: i32) -> Result<(), String> {
		let aspect_ratio = width as f64 / height.max(1) as f64;
		let elapsed_time = self.elapsed_time;
		let camera_yaw = self.camera_yaw;
		let camera_pitch = self.camera_pitch;
		let camera_forward_x = camera_yaw.sin() * camera_pitch.cos();
		let camera_forward_y = camera_pitch.sin();
		let camera_forward_z = -camera_yaw.cos() * camera_pitch.cos();
		let camera_right_x = camera_yaw.cos();
		let camera_right_z = camera_yaw.sin();
		let (camera_position_x, camera_position_y, camera_position_z) = (self.camera_position[0], self.camera_position[1], self.camera_position[2]);
		let speed_factor = (self.current_move_speed / RUN_SPEED).min(1.6);
		let field_of_view_radian = (55.0 + speed_factor * 4.0) * std::f64::consts::PI / 180.0;
		let view_projection_matrix = self.view_projection_matrix;
		let view_projection_elements = *view_projection_matrix.get_elements();
		let sun_direction = Vector3::create(-0.42, 0.62, -0.35).normalize();

		// 캐릭터 모델 행렬. (마네킹은 로컬 +z 정면이라 -yaw + 180도)
		let mut character_model_matrix = Matrix4::create_translation(self.player_x, self.player_y, self.player_z);
		character_model_matrix.rotate_y(-self.character_yaw + std::f64::consts::PI);
		let character_model_elements = *character_model_matrix.get_elements();

		// 깃발천 행렬. (본 패스와 깊이 패스 공용)
		let mut flag_banner_list: Vec<(Matrix4, [f32; 3])> = Vec::new();
		for hill_index in 0..self.world.hill_count {
			let hill = &self.world.mountain_list[hill_index];
			let banner_wave = (elapsed_time * 2.6 + hill_index as f64 * 1.7).sin() * 0.24;
			let cleared_pulse = (1.0 - (elapsed_time - self.hill_cleared_pulse_time_list[hill_index]) * 1.2).max(0.0);
			let banner_scale = 1.0 + cleared_pulse * 0.6;
			let mut banner_matrix = Matrix4::create_translation(hill.summit_x, self.hill_flag_base_height_list[hill_index] + FLAG_POLE_HEIGHT - 0.42, hill.summit_z);
			banner_matrix.rotate_y(banner_wave + hill_index as f64 * 0.9);
			banner_matrix.translate(0.62 * banner_scale, 0.0, 0.0);
			banner_matrix.scale(1.15 * banner_scale, 0.6 * banner_scale, 0.05);
			let banner_color = if self.hill_cleared_list[hill_index] { [1.0, 0.78, 0.2] } else { [0.78, 0.2, 0.16] };
			flag_banner_list.push((banner_matrix, banner_color));
		}

		let mut fox_matrix_list: Vec<Matrix4> = Vec::new();
		let delta_time = self.delta_time;
		let shadow_realtime_blend = self.shadow_realtime_blend;
		let shadow_state = self.shadow_state;
		let option_blend_list = self.option_blend_list;
		let glow_value = self.glow_value;
		let motion_blur_value = self.motion_blur_value;
		let motion_blur_current_strength = self.motion_blur_current_strength;
		let player_x = self.player_x;
		let player_y = self.player_y;
		let player_z = self.player_z;

		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return Ok(());
			}
		};
		resources.scene_render_target.resize(gl, width, height)?;
		resources.bloom_effect.resize(gl, width, height)?;
		for fox in resources.fox_list.iter_mut() {
			fox.model.update(delta_time);
			let mut fox_matrix = Matrix4::create_translation(fox.current_x, fox.current_y, fox.current_z);
			fox_matrix.rotate_y(std::f64::consts::PI - fox.current_yaw);
			fox_matrix.scale(fox.body_scale, fox.body_scale, fox.body_scale);
			fox_matrix_list.push(fox_matrix);
		}
		let identity_matrix = Matrix4::create_identity();
		let identity_elements = *identity_matrix.get_elements();

		unsafe {
			// 0. 섀도우 맵 깊이 패스. (REALTIME 모드)
			let shadow_depth_texture = resources.shadow_map.get_depth_texture();
			let mut light_view_projection_elements = identity_elements;
			if shadow_realtime_blend > 0.01 {
				let light_focus_position = Vector3::create(player_x + camera_forward_x * 12.0, player_y, player_z + camera_forward_z * 12.0);
				let light_travel_direction = Vector3::create(-sun_direction.x, -sun_direction.y, -sun_direction.z);
				resources.shadow_map.update_light_matrix(light_focus_position, light_travel_direction, 48.0, 5.0, 260.0);
				light_view_projection_elements = *resources.shadow_map.get_light_view_projection_matrix().get_elements();
				resources.shadow_map.begin_render(gl);
				gl.disable(glow::BLEND);
				gl.disable(glow::CULL_FACE);
				resources.depth_program.use_program(gl);
				gl.uniform_matrix_4_f32_slice(resources.depth_program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &light_view_projection_elements);
				let depth_model_location = resources.depth_program.get_uniform_location(gl, "modelMatrix");
				gl.uniform_matrix_4_f32_slice(depth_model_location.as_ref(), false, &identity_elements);
				gl.bind_vertex_array(Some(resources.terrain_vertex_array));
				gl.draw_elements(glow::TRIANGLES, resources.terrain_index_count, glow::UNSIGNED_INT, 0);
				for rock in self.rock_list.iter() {
					gl.uniform_matrix_4_f32_slice(depth_model_location.as_ref(), false, rock.matrix.get_elements());
					let rock_variant = &resources.rock_variant_list[rock.variant_index];
					gl.bind_vertex_array(Some(rock_variant.vertex_array));
					gl.draw_arrays(glow::TRIANGLES, 0, rock_variant.vertex_count);
				}
				gl.bind_vertex_array(Some(resources.cube_vertex_array));
				for pole_matrix in self.hill_pole_matrix_list.iter() {
					gl.uniform_matrix_4_f32_slice(depth_model_location.as_ref(), false, pole_matrix.get_elements());
					gl.draw_elements(glow::TRIANGLES, resources.cube_index_count, glow::UNSIGNED_SHORT, 0);
				}
				for (banner_matrix, _) in flag_banner_list.iter() {
					gl.uniform_matrix_4_f32_slice(depth_model_location.as_ref(), false, banner_matrix.get_elements());
					gl.draw_elements(glow::TRIANGLES, resources.cube_index_count, glow::UNSIGNED_SHORT, 0);
				}
				resources.skinned_model_renderer.draw_depth(gl, &mut resources.character_model, &light_view_projection_elements, &character_model_elements);
				for (fox, fox_matrix) in resources.fox_list.iter_mut().zip(fox_matrix_list.iter()) {
					resources.skinned_model_renderer.draw_depth(gl, &mut fox.model, &light_view_projection_elements, fox_matrix.get_elements());
				}
				gl.bind_vertex_array(None);
			}

			// 섀도우 맵 텍스처 바인드. (본 패스 공용, 유닛 3)
			gl.active_texture(glow::TEXTURE3);
			gl.bind_texture(glow::TEXTURE_2D, Some(shadow_depth_texture));
			gl.active_texture(glow::TEXTURE0);
			resources.skinned_model_renderer.set_shadow_map(Some(shadow_depth_texture), Some(&light_view_projection_elements), 1.0 / SHADOWMAP_RESOLUTION as f64);
			resources.skinned_model_renderer.set_shadow_strength(shadow_realtime_blend);

			// 1. 씬 렌더 타겟 비우기.
			resources.scene_render_target.bind(gl);
			gl.disable(glow::BLEND);
			gl.disable(glow::CULL_FACE);
			gl.depth_mask(true);
			gl.clear_color(0.8, 0.85, 0.92, 1.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

			// 2. 하늘.
			gl.disable(glow::DEPTH_TEST);
			resources.sky_pass.use_program(gl);
			let up_basis_x = -camera_pitch.sin() * camera_yaw.sin();
			let up_basis_y = camera_pitch.cos();
			let up_basis_z = camera_pitch.sin() * camera_yaw.cos();
			gl.uniform_3_f32(resources.sky_pass.get_uniform_location(gl, "cameraRightDirection").as_ref(), camera_right_x as f32, 0.0, camera_right_z as f32);
			gl.uniform_3_f32(resources.sky_pass.get_uniform_location(gl, "cameraUpDirection").as_ref(), up_basis_x as f32, up_basis_y as f32, up_basis_z as f32);
			gl.uniform_3_f32(resources.sky_pass.get_uniform_location(gl, "cameraForwardDirection").as_ref(), camera_forward_x as f32, camera_forward_y as f32, camera_forward_z as f32);
			gl.uniform_3_f32(resources.sky_pass.get_uniform_location(gl, "sunDirection").as_ref(), sun_direction.x as f32, sun_direction.y as f32, sun_direction.z as f32);
			gl.uniform_1_f32(resources.sky_pass.get_uniform_location(gl, "tangentHalfFieldOfView").as_ref(), (field_of_view_radian / 2.0).tan() as f32);
			gl.uniform_1_f32(resources.sky_pass.get_uniform_location(gl, "aspectRatio").as_ref(), aspect_ratio as f32);
			gl.uniform_1_f32(resources.sky_pass.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
			resources.sky_pass.draw(gl);

			// 3. 지형.
			gl.enable(glow::DEPTH_TEST);
			let shadow_texel_size = (1.0 / SHADOWMAP_RESOLUTION as f64) as f32;
			resources.terrain_program.use_program(gl);
			gl.bind_vertex_array(Some(resources.terrain_vertex_array));
			gl.uniform_matrix_4_f32_slice(resources.terrain_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_elements);
			gl.uniform_1_i32(resources.terrain_program.get_uniform_location(gl, "grassTexture").as_ref(), 0);
			gl.uniform_1_i32(resources.terrain_program.get_uniform_location(gl, "rockTexture").as_ref(), 1);
			gl.uniform_1_i32(resources.terrain_program.get_uniform_location(gl, "dirtTexture").as_ref(), 2);
			gl.uniform_3_f32(resources.terrain_program.get_uniform_location(gl, "sunDirection").as_ref(), sun_direction.x as f32, sun_direction.y as f32, sun_direction.z as f32);
			gl.uniform_3_f32(resources.terrain_program.get_uniform_location(gl, "cameraPosition").as_ref(), camera_position_x as f32, camera_position_y as f32, camera_position_z as f32);
			gl.uniform_1_f32(resources.terrain_program.get_uniform_location(gl, "pathRadius").as_ref(), PATH_RADIUS as f32);
			gl.uniform_matrix_4_f32_slice(resources.terrain_program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &light_view_projection_elements);
			gl.uniform_1_i32(resources.terrain_program.get_uniform_location(gl, "shadowMapTexture").as_ref(), 3);
			gl.uniform_1_f32(resources.terrain_program.get_uniform_location(gl, "shadowStrength").as_ref(), shadow_realtime_blend as f32);
			gl.uniform_1_f32(resources.terrain_program.get_uniform_location(gl, "shadowTexelSize").as_ref(), shadow_texel_size);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, Some(resources.grass_texture));
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, Some(resources.rock_texture));
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, Some(resources.dirt_texture));
			gl.draw_elements(glow::TRIANGLES, resources.terrain_index_count, glow::UNSIGNED_INT, 0);
			gl.active_texture(glow::TEXTURE0);

			// 4. 바위.
			resources.rock_program.use_program(gl);
			gl.uniform_matrix_4_f32_slice(resources.rock_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_elements);
			gl.uniform_3_f32(resources.rock_program.get_uniform_location(gl, "sunDirection").as_ref(), sun_direction.x as f32, sun_direction.y as f32, sun_direction.z as f32);
			gl.uniform_3_f32(resources.rock_program.get_uniform_location(gl, "cameraPosition").as_ref(), camera_position_x as f32, camera_position_y as f32, camera_position_z as f32);
			gl.uniform_1_i32(resources.rock_program.get_uniform_location(gl, "boulderTexture").as_ref(), 0);
			gl.uniform_matrix_4_f32_slice(resources.rock_program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &light_view_projection_elements);
			gl.uniform_1_i32(resources.rock_program.get_uniform_location(gl, "shadowMapTexture").as_ref(), 3);
			gl.uniform_1_f32(resources.rock_program.get_uniform_location(gl, "shadowStrength").as_ref(), shadow_realtime_blend as f32);
			gl.uniform_1_f32(resources.rock_program.get_uniform_location(gl, "shadowTexelSize").as_ref(), shadow_texel_size);
			gl.bind_texture(glow::TEXTURE_2D, Some(resources.boulder_texture));
			let rock_model_location = resources.rock_program.get_uniform_location(gl, "modelMatrix");
			let rock_tint_location = resources.rock_program.get_uniform_location(gl, "tintColor");
			for rock in self.rock_list.iter() {
				let rock_variant = &resources.rock_variant_list[rock.variant_index];
				gl.bind_vertex_array(Some(rock_variant.vertex_array));
				gl.uniform_matrix_4_f32_slice(rock_model_location.as_ref(), false, rock.matrix.get_elements());
				gl.uniform_3_f32(rock_tint_location.as_ref(), rock.tint[0], rock.tint[1], rock.tint[2]);
				gl.draw_arrays(glow::TRIANGLES, 0, rock_variant.vertex_count);
			}

			// 5. 오브젝트. (깃발)
			resources.object_program.use_program(gl);
			gl.bind_vertex_array(Some(resources.cube_vertex_array));
			gl.uniform_matrix_4_f32_slice(resources.object_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_elements);
			gl.uniform_3_f32(resources.object_program.get_uniform_location(gl, "sunDirection").as_ref(), sun_direction.x as f32, sun_direction.y as f32, sun_direction.z as f32);
			gl.uniform_3_f32(resources.object_program.get_uniform_location(gl, "cameraPosition").as_ref(), camera_position_x as f32, camera_position_y as f32, camera_position_z as f32);
			gl.uniform_matrix_4_f32_slice(resources.object_program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &light_view_projection_elements);
			gl.uniform_1_i32(resources.object_program.get_uniform_location(gl, "shadowMapTexture").as_ref(), 3);
			gl.uniform_1_f32(resources.object_program.get_uniform_location(gl, "shadowStrength").as_ref(), shadow_realtime_blend as f32);
			gl.uniform_1_f32(resources.object_program.get_uniform_location(gl, "shadowTexelSize").as_ref(), shadow_texel_size);
			let object_model_location = resources.object_program.get_uniform_location(gl, "modelMatrix");
			let object_color_location = resources.object_program.get_uniform_location(gl, "objectColor");
			for pole_matrix in self.hill_pole_matrix_list.iter() {
				gl.uniform_matrix_4_f32_slice(object_model_location.as_ref(), false, pole_matrix.get_elements());
				gl.uniform_3_f32(object_color_location.as_ref(), 0.82, 0.8, 0.74);
				gl.draw_elements(glow::TRIANGLES, resources.cube_index_count, glow::UNSIGNED_SHORT, 0);
			}
			for (banner_matrix, banner_color) in flag_banner_list.iter() {
				gl.uniform_matrix_4_f32_slice(object_model_location.as_ref(), false, banner_matrix.get_elements());
				gl.uniform_3_f32(object_color_location.as_ref(), banner_color[0], banner_color[1], banner_color[2]);
				gl.draw_elements(glow::TRIANGLES, resources.cube_index_count, glow::UNSIGNED_SHORT, 0);
			}
			gl.bind_vertex_array(None);

			// 캐릭터와 여우. (스킨드 모델)
			gl.enable(glow::CULL_FACE);
			resources.skinned_model_renderer.draw(gl, &mut resources.character_model, &view_projection_elements, &character_model_elements, camera_position_x, camera_position_y, camera_position_z);
			for (fox, fox_matrix) in resources.fox_list.iter_mut().zip(fox_matrix_list.iter()) {
				resources.skinned_model_renderer.draw(gl, &mut fox.model, &view_projection_elements, fox_matrix.get_elements(), camera_position_x, camera_position_y, camera_position_z);
			}
			gl.disable(glow::CULL_FACE);

			// 6. 블롭 그림자. (SIMPLE 모드, 정적 바위와 깃발 + 동적 캐릭터)
			if shadow_state == 1 {
				gl.enable(glow::BLEND);
				gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
				gl.depth_mask(false);
				resources.shadow_program.use_program(gl);
				gl.uniform_matrix_4_f32_slice(resources.shadow_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_elements);
				let shadow_strength_location = resources.shadow_program.get_uniform_location(gl, "shadowStrength");
				gl.uniform_1_f32(shadow_strength_location.as_ref(), 1.0);
				gl.bind_vertex_array(Some(resources.static_shadow_vertex_array));
				gl.draw_elements(glow::TRIANGLES, resources.static_shadow_index_count, glow::UNSIGNED_SHORT, 0);
				let ground_height_under_player = self.world.terrain_surface_height_at(player_x, player_z);
				let height_above_ground = player_y - ground_height_under_player;
				let shadow_strength = (1.0 - height_above_ground * 0.6).max(0.0);
				if shadow_strength > 0.01 {
					let mut shadow_vertex_data = vec![0.0f32; (SHADOW_SEGMENT_COUNT + 1) * 4];
					shadow_vertex_data[0] = player_x as f32;
					shadow_vertex_data[1] = (ground_height_under_player + 0.12) as f32;
					shadow_vertex_data[2] = player_z as f32;
					shadow_vertex_data[3] = 0.42;
					for segment_index in 0..SHADOW_SEGMENT_COUNT {
						let segment_angle = (segment_index as f64 / SHADOW_SEGMENT_COUNT as f64) * std::f64::consts::PI * 2.0;
						let segment_x = player_x + segment_angle.cos() * SHADOW_RADIUS;
						let segment_z = player_z + segment_angle.sin() * SHADOW_RADIUS;
						let segment_offset = (segment_index + 1) * 4;
						shadow_vertex_data[segment_offset] = segment_x as f32;
						shadow_vertex_data[segment_offset + 1] = (self.world.terrain_surface_height_at(segment_x, segment_z) + 0.12) as f32;
						shadow_vertex_data[segment_offset + 2] = segment_z as f32;
						shadow_vertex_data[segment_offset + 3] = 0.0;
					}
					gl.bind_vertex_array(Some(resources.dynamic_shadow_vertex_array));
					gl.bind_buffer(glow::ARRAY_BUFFER, Some(resources.dynamic_shadow_vertex_buffer));
					gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(&shadow_vertex_data), glow::DYNAMIC_DRAW);
					gl.uniform_1_f32(shadow_strength_location.as_ref(), shadow_strength as f32);
					gl.draw_elements(glow::TRIANGLES, (SHADOW_SEGMENT_COUNT * 3) as i32, glow::UNSIGNED_SHORT, 0);
				}
				gl.bind_vertex_array(None);
				gl.depth_mask(true);
				gl.disable(glow::BLEND);
			}

			// 7. 블룸.
			let scene_color_texture = resources.scene_render_target.get_color_texture(0);
			resources.bloom_effect.render(gl, scene_color_texture);

			// 8. 최종 합성 → 화면.
			gl.disable(glow::DEPTH_TEST);
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.viewport(0, 0, width, height);
			resources.composite_pass.use_program(gl);
			gl.uniform_1_i32(resources.composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.uniform_1_i32(resources.composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			gl.uniform_2_f32(resources.composite_pass.get_uniform_location(gl, "resolution").as_ref(), width as f32, height as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), (option_blend_list[0] * glow_value * 0.9) as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "motionBlurStrength").as_ref(), (motion_blur_current_strength * motion_blur_value * 1.2) as f32);
			let mut motion_blur_center_x = 0.5;
			let mut motion_blur_center_y = 0.5;
			let motion_blur_clip = view_projection_matrix.transform_point(player_x, player_y + 1.0, player_z);
			if motion_blur_clip.w > 0.001 {
				motion_blur_center_x = (motion_blur_clip.x / motion_blur_clip.w) * 0.5 + 0.5;
				motion_blur_center_y = (motion_blur_clip.y / motion_blur_clip.w) * 0.5 + 0.5;
			}
			gl.uniform_2_f32(resources.composite_pass.get_uniform_location(gl, "motionBlurCenter").as_ref(), motion_blur_center_x as f32, motion_blur_center_y as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "vignetteStrength").as_ref(), option_blend_list[1] as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "filmStrength").as_ref(), option_blend_list[2] as f32);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, scene_color_texture);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, resources.bloom_effect.get_result_texture());
			resources.composite_pass.draw(gl);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE3);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);

			if !resources.is_first_frame_checked {
				resources.is_first_frame_checked = true;
				println!("ancientmountain: 첫 3D 프레임 GL 오류 코드 0x{:X}", gl.get_error());
			}
		}
		Ok(())
	}

	//==============================================================================
	// 그림자 곁들인 글.
	//==============================================================================
	fn draw_shadowed_text(graphic: &mut Graphic, text: &str, x: f64, y: f64) {
		graphic.set_fill_color(rgba(10.0, 16.0, 24.0, 0.75));
		graphic.draw_fill_text(text, x + 1.2, y + 1.4);
		graphic.set_fill_color(rgba(250.0, 250.0, 248.0, 0.95));
		graphic.draw_fill_text(text, x, y);
	}

	fn draw_circle_icon(graphic: &mut Graphic, center_x: f64, center_y: f64, icon_radius: f64, icon_color: Color, line_width: f64) {
		graphic.set_stroke_color(icon_color);
		graphic.draw_stroke_circle(Vector2::create(center_x, center_y), icon_radius * 0.78, line_width);
	}

	fn draw_cross_icon(graphic: &mut Graphic, center_x: f64, center_y: f64, icon_radius: f64, icon_color: Color, line_width: f64) {
		graphic.set_stroke_color(icon_color);
		let cross_offset = icon_radius * 0.72;
		graphic.draw_line(&[Vector2::create(center_x - cross_offset, center_y - cross_offset), Vector2::create(center_x + cross_offset, center_y + cross_offset)], line_width);
		graphic.draw_line(&[Vector2::create(center_x + cross_offset, center_y - cross_offset), Vector2::create(center_x - cross_offset, center_y + cross_offset)], line_width);
	}

	// 줄 높이. (크롬 line-height: normal 은 위 길이와 아래 길이를 따로 반올림해 더한다)
	fn measure_normal_line_height(canvas: &mut Canvas2D, font: &str) -> (f64, f64) {
		let (font_ascent, font_descent) = measure_line_height(canvas, font);
		(font_ascent.round(), font_descent.round())
	}

	// 제목, 링크, 저작자 표기 그리기. (원본 index.html 의 #titleOverlay, #homeLink, #creditNote CSS 값 그대로)
	fn draw_overlay(&mut self, graphic: &mut Graphic, window_size: Vector2) {
		let engine = Engine::get_engine().unwrap();
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		let device_scale = if window_size.x > 0.0 { canvas_pixel_size.x / window_size.x } else { 1.0 };
		let overlay_key = format!("{}x{}@{}", window_size.x, window_size.y, device_scale);
		if overlay_key != self.overlay_key {
			self.overlay_image_table.clear();
			self.overlay_key = overlay_key;
		}
		let window_width = window_size.x;
		let window_height = window_size.y;
		let center_x = window_width * 0.5;

		// 제목 배치. (#titleMain: clamp(26px, 5vw, 58px), 굵기 200, letter-spacing 0.5em, text-indent 0.5em)
		let title_font_size = (window_width * 0.05).clamp(26.0, 58.0);
		let title_font = format!("200 {}px {}", title_font_size, OVERLAY_LIGHT_FONT);
		let title_letter_spacing = title_font_size * 0.5;
		let (title_ascent, title_descent) = AncientMountainScene::measure_normal_line_height(&mut self.overlay_measure_canvas, &title_font);
		let title_line_height = title_ascent + title_descent;
		let full_title = "ANCIENT MOUNTAIN";
		let full_title_width = measure_spaced_text(&mut self.overlay_measure_canvas, full_title, &title_font, title_letter_spacing) + title_letter_spacing * 2.0;
		let title_line_list: Vec<&str> = if full_title_width > window_width * 0.5 { vec!["ANCIENT", "MOUNTAIN"] } else { vec![full_title] };
		let title_top = window_height * 0.09;
		let divider_y = title_top + title_line_list.len() as f64 * title_line_height + 14.0;
		let divider_width = 240.0f64.min(window_width * 0.4);
		let sub_font_size = (window_width * 0.012).clamp(9.0, 12.0);
		let sub_font = format!("300 {}px {}", sub_font_size, OVERLAY_LIGHT_FONT);
		let sub_letter_spacing = sub_font_size * 0.85;
		let (sub_ascent, sub_descent) = AncientMountainScene::measure_normal_line_height(&mut self.overlay_measure_canvas, &sub_font);
		let sub_top = divider_y + 1.0 + 12.0;
		let title_bake_margin = 140.0;
		let title_bake_rect = Rect::create((center_x - 520.0).max(0.0), title_top - title_bake_margin, (window_width).min(1040.0), (sub_top + sub_ascent + sub_descent) - title_top + title_bake_margin * 2.0);

		// 링크 배치. (#homeLink: 굵기 300, letter-spacing 0.45em, 안쪽 여백 10px 22px, 테두리 1px)
		let link_font_size = sub_font_size;
		let link_font = format!("300 {}px {}", link_font_size, OVERLAY_SYSTEM_FONT);
		let link_letter_spacing = link_font_size * 0.45;
		let (link_ascent, link_descent) = AncientMountainScene::measure_normal_line_height(&mut self.overlay_measure_canvas, &link_font);
		let link_text = "VANILLAPUNCH.COM";
		let link_text_width = measure_spaced_text(&mut self.overlay_measure_canvas, link_text, &link_font, link_letter_spacing) + link_letter_spacing * 2.0;
		let link_box = Rect::create(0.0, 0.0, link_text_width + 44.0 + 2.0, link_ascent + link_descent + 20.0 + 2.0);
		let link_box = Rect::create(center_x - link_box.size.x * 0.5, window_height - window_height * 0.055 - link_box.size.y, link_box.size.x, link_box.size.y);
		let link_bake_margin = 40.0;
		let link_bake_rect = Rect::create(link_box.position.x - link_bake_margin, link_box.position.y - link_bake_margin, link_box.size.x + link_bake_margin * 2.0, link_box.size.y + link_bake_margin * 2.0);

		// 저작자 표기 배치. (#creditNote: 8.5px, letter-spacing 0.12em, line-height 1.7, 너비 720 이하는 7px, 0.04em, 1.5)
		let is_narrow = window_width <= 720.0;
		let credit_font_size = if is_narrow { 7.0 } else { 8.5 };
		let credit_font = format!("{}px {}", credit_font_size, OVERLAY_SYSTEM_FONT);
		let credit_letter_spacing = credit_font_size * if is_narrow { 0.04 } else { 0.12 };
		let credit_line_height = credit_font_size * if is_narrow { 1.5 } else { 1.7 };
		let (credit_ascent, credit_descent) = AncientMountainScene::measure_normal_line_height(&mut self.overlay_measure_canvas, &credit_font);
		let credit_top = window_height - 4.0 - credit_line_height * 2.0;
		let credit_bake_rect = Rect::create(0.0, credit_top - 20.0, window_width, credit_line_height * 2.0 + 20.0 + 4.0);

		// 등장. (titleEntrance 3.2초, cubic-bezier(0.16, 1, 0.3, 1): 투명도, 위에서 14px 내려옴, 번짐 8px 에서 0)
		let entrance_ease = css_cubic_bezier(0.16, 1.0, 0.3, 1.0, (self.elapsed_time / 3.2).clamp(0.0, 1.0));
		let blur_level = ((1.0 - entrance_ease) * 8.0).round() as i32;
		let entrance_offset_y = -14.0 * (1.0 - entrance_ease);
		let title_color = rgba(247.0, 243.0, 232.0, 1.0);
		let sub_color = rgba(233.0, 236.0, 240.0, 0.8);
		let shadow_dark = rgba(20.0, 34.0, 52.0, 0.8);

		let title_key = format!("title|{}", blur_level);
		if !self.overlay_image_table.contains_key(&title_key) {
			let image = bake_overlay(title_bake_rect.size.x, title_bake_rect.size.y, device_scale, blur_level as f64, &|canvas| {
				canvas.translate(-title_bake_rect.position.x, -title_bake_rect.position.y);
				let title_shadow_list = [TextShadow { color: rgba(20.0, 34.0, 52.0, 0.85), offset_y: 2.0, blur: 18.0 }, TextShadow { color: rgba(255.0, 226.0, 150.0, 0.25), offset_y: 0.0, blur: 42.0 }];
				for (line_index, line_text) in title_line_list.iter().enumerate() {
					let baseline_y = title_top + line_index as f64 * title_line_height + title_ascent;
					draw_css_text(canvas, device_scale, &[(line_text, title_color)], &title_font, title_letter_spacing, if line_index == 0 { center_x } else { center_x - title_letter_spacing * 0.5 }, baseline_y, &title_shadow_list);
				}
				// CSS linear-gradient(90deg, transparent, 색, transparent): 기기 픽셀 한 칸씩 알파를 깐다.
				let divider_step_count = (divider_width * device_scale).round().max(1.0) as i32;
				for step_index in 0..divider_step_count {
					let step_ratio = (step_index as f64 + 0.5) / divider_step_count as f64;
					let step_alpha = 0.75 * (1.0 - (step_ratio * 2.0 - 1.0).abs());
					canvas.set_fill_style(rgba(247.0, 243.0, 232.0, step_alpha));
					canvas.fill_rect(center_x - divider_width * 0.5 + step_index as f64 / device_scale, divider_y.round(), 1.0 / device_scale, 1.0);
				}
				draw_css_text(canvas, device_scale, &[("VANILLA ENGINE", sub_color)], &sub_font, sub_letter_spacing, center_x, sub_top + sub_ascent, &[TextShadow { color: shadow_dark, offset_y: 1.0, blur: 8.0 }]);
			});
			self.overlay_image_table.insert(title_key.clone(), image);
		}
		let pointer_position = engine.get_input_manager().get_canvas_native_input_position();
		let is_link_hovered = pointer_position.x >= link_box.position.x && pointer_position.x <= link_box.position.x + link_box.size.x && pointer_position.y >= link_box.position.y + entrance_offset_y && pointer_position.y <= link_box.position.y + entrance_offset_y + link_box.size.y;
		let link_key = format!("link|{}|{}", blur_level, is_link_hovered);
		if !self.overlay_image_table.contains_key(&link_key) {
			let image = bake_overlay(link_bake_rect.size.x, link_bake_rect.size.y, device_scale, blur_level as f64, &|canvas| {
				canvas.translate(-link_bake_rect.position.x, -link_bake_rect.position.y);
				let radius = link_box.size.y * 0.5;
				let mut path = vanilla::Path2D::new();
				path.round_rect((link_box.position.x + 0.5) as f32, (link_box.position.y + 0.5) as f32, (link_box.size.x - 1.0) as f32, (link_box.size.y - 1.0) as f32, &[radius - 0.5]);
				// 올려 둔 때: 글자 흰색, 테두리 금빛, 바탕 밝게. (원본 #homeLink:hover)
				// 올려 둔 때의 광채. (원본 #homeLink:hover box-shadow, 바깥 광채는 테두리 상자 안을 비우고 그린다)
				if is_link_hovered {
					draw_css_box_shadow(canvas, device_scale, link_box, link_box.size.y * 0.5, 22.0, rgba(255.0, 236.0, 180.0, 0.3), false);
				}
				canvas.set_fill_style(if is_link_hovered { rgba(38.0, 52.0, 70.0, 0.55) } else { rgba(16.0, 26.0, 38.0, 0.4) });
				canvas.fill_path(&path, vanilla::CanvasFillRule::NonZero);
				canvas.set_stroke_style(if is_link_hovered { rgba(255.0, 236.0, 180.0, 0.85) } else { rgba(247.0, 243.0, 232.0, 0.32) });
				canvas.set_line_width(1.0);
				canvas.stroke_path(&path);
				draw_css_text(canvas, device_scale, &[(link_text, if is_link_hovered { rgba(255.0, 255.0, 255.0, 1.0) } else { rgba(247.0, 243.0, 232.0, 0.8) })], &link_font, link_letter_spacing, link_box.position.x + link_box.size.x * 0.5, link_box.position.y + 1.0 + 10.0 + link_ascent, &[TextShadow { color: shadow_dark, offset_y: 1.0, blur: 8.0 }]);
			});
			self.overlay_image_table.insert(link_key.clone(), image);
		}
		let credit_key = String::from("credit");
		if !self.overlay_image_table.contains_key(&credit_key) {
			let image = bake_overlay(credit_bake_rect.size.x, credit_bake_rect.size.y, device_scale, 0.0, &|canvas| {
				canvas.translate(-credit_bake_rect.position.x, -credit_bake_rect.position.y);
				let label_color = rgba(247.0, 243.0, 232.0, 0.42);
				let link_color = rgba(247.0, 243.0, 232.0, 0.55);
				let credit_shadow_list = [TextShadow { color: rgba(0.0, 0.0, 0.0, 0.8), offset_y: 1.0, blur: 6.0 }];
				let credit_line_list: [[(&str, Color); 2]; 2] = [
					[("CHARACTER: ", label_color), ("QUATERNIUS UNIVERSAL ANIMATION LIBRARY (CC0)", link_color)],
					[("FOX: ", label_color), ("MODEL PIXELMANNEN (CC0), RIGGING AND ANIMATION TOMKRANIS (CC BY 4.0), GLTF CONVERSION ASOBOSTUDIO AND SCUREST (CC BY 4.0)", link_color)],
				];
				for (line_index, run_list) in credit_line_list.iter().enumerate() {
					let baseline_y = credit_top + line_index as f64 * credit_line_height + (credit_line_height - (credit_ascent + credit_descent)) * 0.5 + credit_ascent;
					// 줄 끝의 letter-spacing 도 줄 너비에 들어가므로 글은 그 반만큼 왼쪽으로 간다.
					draw_css_text(canvas, device_scale, run_list, &credit_font, credit_letter_spacing, center_x - credit_letter_spacing * 0.5, baseline_y, &credit_shadow_list);
				}
			});
			self.overlay_image_table.insert(credit_key.clone(), image);
		}
		// 눌러 열 링크 자리. (알약과 저작자 표기의 링크 글, 크롬 앵커의 글자 상자)
		self.link_hit_list.clear();
		self.link_hit_list.push((Rect::create(link_box.position.x, link_box.position.y + entrance_offset_y, link_box.size.x, link_box.size.y), "https://vanillapunch.com/tools/"));
		{
			let measure_canvas = &mut self.overlay_measure_canvas;
			measure_canvas.set_font(&credit_font);
			let credit_ascent_descent = credit_ascent + credit_descent;
			let measure_run = |canvas: &mut Canvas2D, text: &str| -> f64 { text.chars().map(|character| canvas.measure_text(&character.to_string()).width).sum::<f64>() + credit_letter_spacing * text.chars().count() as f64 };
			let credit_line_link_list: [(&str, &str, &str); 2] = [
				("CHARACTER: ", "QUATERNIUS UNIVERSAL ANIMATION LIBRARY (CC0)", "https://opengameart.org/content/universal-animation-library"),
				("FOX: ", "MODEL PIXELMANNEN (CC0), RIGGING AND ANIMATION TOMKRANIS (CC BY 4.0), GLTF CONVERSION ASOBOSTUDIO AND SCUREST (CC BY 4.0)", "https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/Fox"),
			];
			for (line_index, (label_text, link_run_text, url)) in credit_line_link_list.iter().enumerate() {
				let label_width = measure_run(measure_canvas, label_text);
				let link_width = measure_run(measure_canvas, link_run_text);
				let line_start_x = center_x - (label_width + link_width) * 0.5;
				let line_top = credit_top + line_index as f64 * credit_line_height + (credit_line_height - credit_ascent_descent) * 0.5;
				self.link_hit_list.push((Rect::create(line_start_x + label_width, line_top, link_width, credit_ascent_descent), *url));
			}
		}
		// 알약 뒤 흐림. (원본 backdrop-filter: blur(8px), border-radius: 999px. 인자는 CSS 와 같은 표준편차 8 에 기기 픽셀 비를 곱한 값, 둥근 모서리 안만)
		{
			let pill_rect = Rect::create(link_box.position.x, link_box.position.y + entrance_offset_y, link_box.size.x, link_box.size.y);
			self.backdrop_view.set_backdrop_blur(8.0 * device_scale);
			self.backdrop_view.set_backdrop_corner_radius(pill_rect.size.y * 0.5);
			let mut transform = NodeTransform::new();
			transform.set_content_size(pill_rect.size);
			graphic.push_state();
			graphic.translate(pill_rect.position.x, pill_rect.position.y);
			self.backdrop_view.draw_background(&transform, graphic);
			graphic.pop_state();
		}
		graphic.set_global_alpha(entrance_ease);
		graphic.draw_image(&self.overlay_image_table[&title_key], Vector2::create(title_bake_rect.position.x, title_bake_rect.position.y + entrance_offset_y), title_bake_rect.size);
		graphic.draw_image(&self.overlay_image_table[&link_key], Vector2::create(link_bake_rect.position.x, link_bake_rect.position.y + entrance_offset_y), link_bake_rect.size);
		graphic.set_global_alpha(1.0);
		graphic.draw_image(&self.overlay_image_table[&credit_key], Vector2::create(credit_bake_rect.position.x, credit_bake_rect.position.y), credit_bake_rect.size);
	}

	// 프롬프트. (아이콘 + 레이블)
	fn draw_prompt(graphic: &mut Graphic, screen: Vector2, label: &str, elapsed_time: f64) {
		let pulse_scale = 1.0 + (elapsed_time * 5.5).sin() * 0.06;
		let prompt_radius = 21.0 * pulse_scale;
		graphic.set_fill_color(rgba(0.0, 0.0, 0.0, 0.82));
		graphic.draw_circle(screen, prompt_radius);
		graphic.set_stroke_color(rgba(255.0, 255.0, 255.0, 0.4));
		graphic.draw_stroke_circle(screen, prompt_radius, 1.6);
		AncientMountainScene::draw_circle_icon(graphic, screen.x, screen.y, prompt_radius * 0.62, rgba(255.0, 255.0, 255.0, 0.98), 3.0);
		graphic.set_font_string(r#"600 15px "Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif"#);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		AncientMountainScene::draw_shadowed_text(graphic, label, screen.x + prompt_radius + 10.0, screen.y);
	}

}

//==============================================================================
// 원본 HTML 오버레이 꼴 굽기. (CSS 의 글자 굵기, letter-spacing, text-shadow, filter: blur 를 엔진 Canvas2D 로 굽는다)
// - 원본은 제목, 링크, 저작자 표기를 HTML 요소로 얹는다. 러스트 판은 같은 CSS 값을 Canvas2D 에 그려 한 장으로 굽고 엔진 Graphic 으로 그린다.
// - 글꼴 문자열은 원본 CSS 그대로다. (운영체제에 없는 이름은 엔진이 다음 이름, 일반 이름으로 넘어간다)
//==============================================================================
const OVERLAY_SYSTEM_FONT: &str = "\"Segoe UI\", \"Malgun Gothic\", \"Apple SD Gothic Neo\", sans-serif";
const OVERLAY_LIGHT_FONT: &str = "\"Segoe UI Light\", \"Segoe UI\", \"Malgun Gothic\", \"Apple SD Gothic Neo\", sans-serif";
const OVERLAY_SHADOW_FAR: f64 = 30000.0;

// CSS text-shadow 한 겹. (x 번짐 없이 y 만 내림)
#[derive(Clone, Copy)]
struct TextShadow {
	color: Color,
	offset_y: f64,
	blur: f64,
}

// 글자 하나씩 자간을 벌려 그린 줄의 가로 길이. (CSS letter-spacing 이 마지막 글자 뒤에도 붙는 것은 text-indent 가 맞춘다)
fn measure_spaced_text(canvas: &mut Canvas2D, text: &str, font: &str, letter_spacing: f64) -> f64 {
	canvas.set_font(font);
	let width_sum: f64 = text.chars().map(|character| canvas.measure_text(&character.to_string()).width).sum();
	width_sum + letter_spacing * (text.chars().count().saturating_sub(1)) as f64
}

// CSS 처럼 그림자를 먼저 깔고 글을 올려 한 줄을 가운데 맞춤으로 그린다. (기준선은 alphabetic, 좌표는 CSS 픽셀, 글 조각마다 색이 다를 수 있다)
fn draw_css_text(canvas: &mut Canvas2D, device_scale: f64, run_list: &[(&str, Color)], font: &str, letter_spacing: f64, center_x: f64, baseline_y: f64, shadow_list: &[TextShadow]) {
	let baseline_y = baseline_y.round();
	canvas.set_font(font);
	canvas.set_text_align(CanvasTextAlign::Left);
	canvas.set_text_baseline(CanvasTextBaseline::Alphabetic);
	let mut character_list: Vec<(String, f64, Color)> = Vec::new();
	for (run_text, run_color) in run_list {
		for character in run_text.chars() {
			let character_text = character.to_string();
			let character_width = canvas.measure_text(&character_text).width;
			character_list.push((character_text, character_width, *run_color));
		}
	}
	let total_width = character_list.iter().map(|character| character.1).sum::<f64>() + letter_spacing * (character_list.len().saturating_sub(1)) as f64;
	let start_x = center_x - total_width * 0.5;
	for shadow in shadow_list {
		// 그림자만: 글은 멀리 밖에 그리고 그림자를 같은 만큼 되돌려 놓는다.
		canvas.set_shadow_color(shadow.color);
		canvas.set_shadow_blur(shadow.blur * device_scale);
		canvas.set_shadow_offset_x(OVERLAY_SHADOW_FAR);
		canvas.set_shadow_offset_y(shadow.offset_y * device_scale);
		canvas.set_fill_style(Color::create(0.0, 0.0, 0.0, 1.0));
		let mut character_x = start_x;
		for (character_text, character_width, _) in character_list.iter() {
			canvas.fill_text(character_text, character_x - OVERLAY_SHADOW_FAR / device_scale, baseline_y, None);
			character_x += character_width + letter_spacing;
		}
	}
	canvas.set_shadow_color(Color::create(0.0, 0.0, 0.0, 0.0));
	canvas.set_shadow_blur(0.0);
	canvas.set_shadow_offset_x(0.0);
	canvas.set_shadow_offset_y(0.0);
	let mut character_x = start_x;
	for (character_text, character_width, character_color) in character_list.iter() {
		canvas.set_fill_style(*character_color);
		canvas.fill_text(character_text, character_x, baseline_y, None);
		character_x += character_width + letter_spacing;
	}
}

// 글꼴의 줄 높이. (CSS line-height: normal 에 가깝게 위 길이와 아래 길이의 합)
fn measure_line_height(canvas: &mut Canvas2D, font: &str) -> (f64, f64) {
	canvas.set_font(font);
	let metrics = canvas.measure_text("M");
	(metrics.font_ascent, metrics.font_descent)
}

// CSS filter: blur(시그마) 를 곱한 알파로. (가로 세로 가우스, 모서리는 투명)
fn blur_image_data(image_data: &mut vanilla::ImageData, sigma: f64) {
	if sigma < 0.3 {
		return;
	}
	let width = image_data.width as usize;
	let height = image_data.height as usize;
	let radius = (sigma * 3.0).ceil() as i32;
	let mut kernel: Vec<f32> = (-radius..=radius).map(|offset| (-((offset * offset) as f64) / (2.0 * sigma * sigma)).exp() as f32).collect();
	let kernel_sum: f32 = kernel.iter().sum();
	for weight in kernel.iter_mut() {
		*weight /= kernel_sum;
	}
	// 곱한 알파로 바꿔 흐린다.
	let mut channel_list: Vec<f32> = vec![0.0; width * height * 4];
	for pixel_index in 0..width * height {
		let alpha = image_data.data[pixel_index * 4 + 3] as f32 / 255.0;
		for channel_index in 0..3 {
			channel_list[pixel_index * 4 + channel_index] = image_data.data[pixel_index * 4 + channel_index] as f32 / 255.0 * alpha;
		}
		channel_list[pixel_index * 4 + 3] = alpha;
	}
	let mut temporary_list = vec![0.0f32; width * height * 4];
	for row_index in 0..height {
		for column_index in 0..width {
			let mut sum = [0.0f32; 4];
			for (kernel_index, weight) in kernel.iter().enumerate() {
				let source_column = column_index as i32 + kernel_index as i32 - radius;
				if source_column >= 0 && (source_column as usize) < width {
					let source_index = (row_index * width + source_column as usize) * 4;
					for channel_index in 0..4 {
						sum[channel_index] += channel_list[source_index + channel_index] * weight;
					}
				}
			}
			temporary_list[(row_index * width + column_index) * 4..(row_index * width + column_index) * 4 + 4].copy_from_slice(&sum);
		}
	}
	for row_index in 0..height {
		for column_index in 0..width {
			let mut sum = [0.0f32; 4];
			for (kernel_index, weight) in kernel.iter().enumerate() {
				let source_row = row_index as i32 + kernel_index as i32 - radius;
				if source_row >= 0 && (source_row as usize) < height {
					let source_index = (source_row as usize * width + column_index) * 4;
					for channel_index in 0..4 {
						sum[channel_index] += temporary_list[source_index + channel_index] * weight;
					}
				}
			}
			let pixel_index = row_index * width + column_index;
			let alpha = sum[3];
			for channel_index in 0..3 {
				image_data.data[pixel_index * 4 + channel_index] = if alpha > 0.0001 { (sum[channel_index] / alpha * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
			}
			image_data.data[pixel_index * 4 + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
		}
	}
}

// 오버레이 굽기 한 장. (draw_function 이 Canvas2D 에 CSS 픽셀 좌표로 그린다, 시그마가 0 보다 크면 흐린다)
// CSS box-shadow 한 겹 그리기. (알약 둘레 광채: 바깥 광채는 테두리 상자 안을 비워 두고, inset 은 상자 안만 칠한다. 블러 반지름은 CSS 값 그대로, 기기 픽셀 비를 곱해 캔버스 그림자 블러로 넘긴다)
fn draw_css_box_shadow(canvas: &mut Canvas2D, device_scale: f64, box_rect: Rect, corner_radius: f64, blur: f64, color: Color, is_inset: bool) {
	let mut box_path = vanilla::Path2D::new();
	box_path.round_rect(box_rect.position.x as f32, box_rect.position.y as f32, box_rect.size.x as f32, box_rect.size.y as f32, &[corner_radius]);
	let mut frame_path = vanilla::Path2D::new();
	frame_path.rect(box_rect.position.x - 400.0, box_rect.position.y - 400.0, box_rect.size.x + 800.0, box_rect.size.y + 800.0);
	frame_path.round_rect(box_rect.position.x as f32, box_rect.position.y as f32, box_rect.size.x as f32, box_rect.size.y as f32, &[corner_radius]);
	canvas.save();
	canvas.set_shadow_color(color);
	canvas.set_shadow_blur(blur * device_scale);
	canvas.set_shadow_offset_x(0.0);
	canvas.set_shadow_offset_y(0.0);
	canvas.set_fill_style(rgba(0.0, 0.0, 0.0, 1.0));
	if is_inset {
		canvas.clip_path(&box_path, vanilla::CanvasFillRule::NonZero);
		canvas.fill_path(&frame_path, vanilla::CanvasFillRule::EvenOdd);
	}
	else {
		canvas.clip_path(&frame_path, vanilla::CanvasFillRule::EvenOdd);
		canvas.fill_path(&box_path, vanilla::CanvasFillRule::NonZero);
	}
	canvas.restore();
}

fn bake_overlay(width: f64, height: f64, device_scale: f64, blur_sigma: f64, draw_function: &dyn Fn(&mut Canvas2D)) -> Rc<Image> {
	let mut canvas = Canvas2D::new((width * device_scale).ceil() as u32, (height * device_scale).ceil() as u32);
	canvas.scale(device_scale, device_scale);
	draw_function(&mut canvas);
	if blur_sigma >= 0.3 {
		let mut image_data = canvas.get_image_data(0, 0, (width * device_scale).ceil() as u32, (height * device_scale).ceil() as u32);
		blur_image_data(&mut image_data, blur_sigma * device_scale);
		canvas.reset_transform();
		canvas.clear_rect(0.0, 0.0, width * device_scale, height * device_scale);
		canvas.put_image_data(&image_data, 0, 0);
	}
	canvas.to_image()
}

// CSS cubic-bezier(x1, y1, x2, y2) 이징. (animation-timing-function)
fn css_cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, progress: f64) -> f64 {
	let curve = |t: f64, p1: f64, p2: f64| -> f64 { 3.0 * (1.0 - t) * (1.0 - t) * t * p1 + 3.0 * (1.0 - t) * t * t * p2 + t * t * t };
	let mut low = 0.0;
	let mut high = 1.0;
	for _ in 0..40 {
		let middle = (low + high) * 0.5;
		if curve(middle, x1, x2) < progress {
			low = middle;
		}
		else {
			high = middle;
		}
	}
	curve((low + high) * 0.5, y1, y2)
}

//==============================================================================
// 한 프레임의 키 상태.
//==============================================================================
//==============================================================================
// 터치 담당 영역. (원본 touchRouteMap 의 값)
//==============================================================================
#[derive(Clone, Copy, PartialEq)]
enum TouchRoute {
	Panel,
	VirtualPad,
	ConfirmButton,
	CancelButton,
	PromptIcon,
	CameraDrag,
}

struct KeyState {
	move_right: bool,
	move_left: bool,
	move_forward: bool,
	move_back: bool,
	sprint: bool,
	jump_held: bool,
	interact_held: bool,
	arrow_right: bool,
	arrow_left: bool,
	arrow_up: bool,
	arrow_down: bool,
}

impl Scene for AncientMountainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (텍스처와 모델 받기 부탁)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		let character_load_handle = SkinnedModel::load_from_fbx_url(engine, "mannequin.fbx");
		let fox_load_handle = SkinnedModel::load_from_url(engine, "fox.glb");
		self.scene_base.track_load_handle(character_load_handle.get_load_handle());
		self.scene_base.track_load_handle(fox_load_handle.get_load_handle());
		self.character_load_handle = Some(character_load_handle);
		self.fox_load_handle = Some(fox_load_handle);
		for texture_name in TEXTURE_NAME_LIST {
			if cfg!(target_arch = "wasm32") {
				// 웹: 브라우저가 푼다.
				let mut texture_asset = ImageAsset::new();
				let texture_load_handle = texture_asset.load(engine, texture_name);
				self.scene_base.track_load_handle(&texture_load_handle);
				self.texture_asset_list.push(texture_asset);
			}
			else {
				// 데스크탑: 엔진 해독기(PNG, JPEG)가 푼다. 이방성 8, 밉맵은 create_texture 가 한다. (원본 loadTexture)
				let description_handle = Material::load_image_description(engine, texture_name, Some(ImageDescriptionOptions { flip_y: false, is_color_data: false, anisotropy: 8.0 }));
				self.scene_base.track_load_handle(description_handle.get_load_handle());
				self.texture_description_handle_list.push(description_handle);
			}
		}
	}

	//==============================================================================
	// 초기화. (다 받은 뒤 GL 리소스를 만든다)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		let mut graphic = match engine.get_graphic_mut() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let mut engine_texture_list: Vec<Option<GlTexture>> = Vec::new();
		for texture_index in 0..TEXTURE_NAME_LIST.len() {
			let engine_texture = if cfg!(target_arch = "wasm32") { self.texture_asset_list[texture_index].get_image().and_then(|image| graphic.get_image_texture(&image)) } else { None };
			engine_texture_list.push(engine_texture);
		}
		match self.create_resources(engine, graphic.get_gl(), &engine_texture_list) {
			Ok(resources) => {
				self.resources = Some(resources);
			}
			Err(error) => {
				eprintln!("ancientmountain: {}", error);
				self.load_error_message = Some(error);
			}
		}
	}

	//==============================================================================
	// 갱신. (입력, 카메라, 캐릭터, 옵션 보간)
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		let is_capture = std::env::var("VANILLA_CAPTURE_PATH").is_ok();
		let delta_time = if is_capture { FIXED_TIME_STEP } else { time_delta.min(0.1) };
		self.delta_time = delta_time;
		self.elapsed_time += delta_time;
		if self.elapsed_time >= EXIT_SECONDS {
			engine.exit_application();
		}
		if self.resources.is_none() {
			return;
		}
		let elapsed_time = self.elapsed_time;
		let window_size = engine.get_view_manager().get_canvas_native_size();

		// 입력 읽기.
		let (key_list, pointer_position, left_just_pressed, left_down, left_just_released, right_just_pressed, jump_key_pressed, interact_key_pressed, any_key_pressed, gamepad_state) = {
			let input_manager = engine.get_input_manager();
			let key_list = KeyState {
				move_right: input_manager.is_key_pressed("KeyD"),
				move_left: input_manager.is_key_pressed("KeyA"),
				move_forward: input_manager.is_key_pressed("KeyW"),
				move_back: input_manager.is_key_pressed("KeyS"),
				sprint: input_manager.is_key_pressed("ShiftLeft") || input_manager.is_key_pressed("ShiftRight"),
				jump_held: input_manager.is_key_pressed("Space") || input_manager.is_key_pressed("KeyX"),
				interact_held: input_manager.is_key_pressed("KeyO") || input_manager.is_key_pressed("Escape"),
				arrow_right: input_manager.is_key_pressed("ArrowRight"),
				arrow_left: input_manager.is_key_pressed("ArrowLeft"),
				arrow_up: input_manager.is_key_pressed("ArrowUp"),
				arrow_down: input_manager.is_key_pressed("ArrowDown"),
			};
			let jump_key_pressed = input_manager.is_key_just_pressed("Space") || input_manager.is_key_just_pressed("Enter") || input_manager.is_key_just_pressed("KeyX");
			let interact_key_pressed = input_manager.is_key_just_pressed("Escape") || input_manager.is_key_just_pressed("KeyO");
			let any_key_pressed = key_list.move_right || key_list.move_left || key_list.move_forward || key_list.move_back || key_list.sprint || key_list.jump_held || key_list.interact_held || key_list.arrow_right || key_list.arrow_left || key_list.arrow_up || key_list.arrow_down;
			let gamepad_manager = input_manager.get_gamepad_manager();
			let gamepad_state = if gamepad_manager.get_connected_gamepad_count() > 0 {
				let axis = |axis_index: usize| -> f64 {
					let value = gamepad_manager.get_axis_value(0, axis_index);
					if value.abs() < 0.16 { 0.0 } else { value }
				};
				Some((axis(0), axis(1), axis(2), axis(3), gamepad_manager.is_button_pressed(0, 0), gamepad_manager.is_button_pressed(0, 1), gamepad_manager.is_button_pressed(0, 10) || gamepad_manager.is_button_pressed(0, 7)))
			}
			else {
				None
			};
			(
				key_list,
				input_manager.get_canvas_native_input_position(),
				input_manager.is_mouse_button_just_pressed(MouseButton::Left),
				input_manager.is_mouse_button_down(MouseButton::Left),
				input_manager.is_mouse_button_just_released(MouseButton::Left),
				input_manager.is_mouse_button_just_pressed(MouseButton::Right),
				jump_key_pressed,
				interact_key_pressed,
				any_key_pressed,
				gamepad_state,
			)
		};
		// 링크 누르기. (원본 <a target="_blank">, 누른 링크 위에서 떼면 연다. 터치는 원본 touchstart 의 preventDefault 로 열리지 않는다)
		if left_just_pressed {
			self.pressed_link_index = self.link_hit_list.iter().position(|(link_rect, _)| pointer_position.x >= link_rect.position.x && pointer_position.x <= link_rect.position.x + link_rect.size.x && pointer_position.y >= link_rect.position.y && pointer_position.y <= link_rect.position.y + link_rect.size.y);
		}
		if left_just_released {
			if let Some(link_index) = self.pressed_link_index.take() {
				let (link_rect, url) = self.link_hit_list[link_index];
				if pointer_position.x >= link_rect.position.x && pointer_position.x <= link_rect.position.x + link_rect.size.x && pointer_position.y >= link_rect.position.y && pointer_position.y <= link_rect.position.y + link_rect.size.y {
					engine.open_url(url);
				}
			}
		}
		if jump_key_pressed {
			self.jump_pressed = true;
		}
		if interact_key_pressed {
			self.interact_pressed = true;
		}
		if any_key_pressed || left_just_pressed || right_just_pressed {
			self.has_any_input_happened = true;
		}
		self.gamepad_move_x = 0.0;
		self.gamepad_move_y = 0.0;
		self.gamepad_camera_x = 0.0;
		self.gamepad_camera_y = 0.0;
		self.gamepad_sprint_held = false;
		if let Some((move_x, move_y, camera_x, camera_y, confirm_state, cancel_state, sprint_held)) = gamepad_state {
			self.gamepad_move_x = move_x;
			self.gamepad_move_y = move_y;
			self.gamepad_camera_x = camera_x;
			self.gamepad_camera_y = camera_y;
			self.gamepad_sprint_held = sprint_held;
			if confirm_state && !self.previous_gamepad_jump {
				self.jump_pressed = true;
				self.has_any_input_happened = true;
			}
			if cancel_state && !self.previous_gamepad_interact {
				self.interact_pressed = true;
			}
			self.previous_gamepad_jump = confirm_state;
			self.previous_gamepad_interact = cancel_state;
		}

		// 가상 조이스틱 처음 설정. (원본 VirtualPad 설정, 자리는 왼쪽 아래)
		if !self.is_pad_placed {
			self.is_pad_placed = true;
			self.virtual_pad.set_pad_radius(62.0);
			self.virtual_pad.set_ball_radius(26.0);
			self.virtual_pad.set_pad_color(Color::create(0.06, 0.09, 0.13, 0.32));
			self.virtual_pad.set_ball_color(Color::create(1.0, 1.0, 1.0, 0.5));
			self.virtual_pad.set_pad_position(Vector2::create(120.0, window_size.y - 140.0));
			let pad_input = self.pad_input.clone();
			self.virtual_pad.set_ball_move_event(move |normalized_direction, pull_strength| {
				pad_input.set(((normalized_direction.x * 2.0 - 1.0) * pull_strength, (normalized_direction.y * 2.0 - 1.0) * pull_strength));
			});
			self.is_touch_device = engine.get_input_manager().is_touch_input();
		}

		// 터치. (원본 touchstart, touchmove, touchend: 손가락마다 담당 영역을 기억한다)
		let (touch_point_list, ended_touch_point_list) = {
			let input_manager = engine.get_input_manager();
			(input_manager.get_touch_points().to_vec(), input_manager.get_ended_touch_points().to_vec())
		};
		let is_touch_frame = !touch_point_list.is_empty() || !ended_touch_point_list.is_empty();
		for touch_point in touch_point_list.iter() {
			let touch_position = touch_point.canvas_native_position;
			let route = self.touch_route_list.iter().find(|(identifier, _)| *identifier == touch_point.identifier).map(|(_, route)| *route);
			match route {
				None => {
					self.is_touch_device = true;
					self.has_any_input_happened = true;
					let new_route = if self.handle_panel_press(touch_position.x, touch_position.y, window_size.x) {
						TouchRoute::Panel
					}
					else if AncientMountainScene::hit_test_circle(touch_position, AncientMountainScene::jump_button_center(window_size), BUTTON_RADIUS + 10.0) {
						self.is_jump_button_held = true;
						self.jump_pressed = true;
						TouchRoute::ConfirmButton
					}
					else if AncientMountainScene::hit_test_circle(touch_position, AncientMountainScene::interact_button_center(window_size), BUTTON_RADIUS + 10.0) {
						self.is_interact_button_held = true;
						self.interact_pressed = true;
						TouchRoute::CancelButton
					}
					else if self.prompt_screen_position.is_some() && AncientMountainScene::hit_test_circle(touch_position, self.prompt_screen_position.unwrap(), 32.0) {
						self.interact_pressed = true;
						TouchRoute::PromptIcon
					}
					else if touch_position.x < window_size.x * 0.42 {
						self.virtual_pad.set_pad_position(touch_position);
						self.virtual_pad.touch_press(touch_position);
						TouchRoute::VirtualPad
					}
					else {
						self.drag_last = Some(touch_position);
						TouchRoute::CameraDrag
					};
					self.touch_route_list.push((touch_point.identifier, new_route));
				}
				Some(route) => {
					if touch_point.phase != TouchPhase::Moved {
						continue;
					}
					match route {
						TouchRoute::Panel => {
							if self.active_slider_index.is_some() {
								self.update_slider_from_x(touch_position.x, window_size.x);
							}
						}
						TouchRoute::VirtualPad => {
							self.virtual_pad.touch_move(touch_position);
						}
						TouchRoute::CameraDrag => {
							if let Some(drag_last) = self.drag_last {
								self.pending_drag_yaw_delta += (touch_position.x - drag_last.x) * 0.0052;
								self.pending_drag_pitch_delta -= (touch_position.y - drag_last.y) * 0.0038;
							}
							self.drag_last = Some(touch_position);
						}
						_ => {
						}
					}
				}
			}
		}
		for touch_point in ended_touch_point_list.iter() {
			let route = match self.touch_route_list.iter().position(|(identifier, _)| *identifier == touch_point.identifier) {
				Some(route_index) => {
					self.touch_route_list.remove(route_index).1
				}
				None => {
					continue;
				}
			};
			match route {
				TouchRoute::Panel => {
					self.active_slider_index = None;
				}
				TouchRoute::VirtualPad => {
					self.virtual_pad.touch_release(touch_point.canvas_native_position);
				}
				TouchRoute::ConfirmButton => {
					self.is_jump_button_held = false;
				}
				TouchRoute::CancelButton => {
					self.is_interact_button_held = false;
				}
				TouchRoute::CameraDrag => {
					self.drag_last = None;
				}
				TouchRoute::PromptIcon => {
				}
			}
		}
		let (left_just_pressed, left_down, left_just_released, right_just_pressed) = if is_touch_frame { (false, false, false, false) } else { (left_just_pressed, left_down, left_just_released, right_just_pressed) };

		// 마우스. (좌클릭 = 점프, 우클릭 = 등반, 하산, 취소, 좌드래그 = 시점 회전)
		if right_just_pressed {
			self.interact_pressed = true;
		}
		if left_just_pressed {
			if !self.handle_panel_press(pointer_position.x, pointer_position.y, window_size.x) {
				if AncientMountainScene::hit_test_circle(pointer_position, AncientMountainScene::jump_button_center(window_size), BUTTON_RADIUS + 6.0) {
					self.is_jump_button_held = true;
					self.jump_pressed = true;
				}
				else if AncientMountainScene::hit_test_circle(pointer_position, AncientMountainScene::interact_button_center(window_size), BUTTON_RADIUS + 6.0) {
					self.is_interact_button_held = true;
					self.interact_pressed = true;
				}
				else if self.prompt_screen_position.is_some() && AncientMountainScene::hit_test_circle(pointer_position, self.prompt_screen_position.unwrap(), 32.0) {
					self.interact_pressed = true;
				}
				else {
					self.is_mouse_dragging = true;
					self.has_mouse_drag_moved = false;
					self.drag_last = Some(pointer_position);
				}
			}
		}
		else if left_down {
			if self.active_slider_index.is_some() {
				self.update_slider_from_x(pointer_position.x, window_size.x);
			}
			else if self.is_mouse_dragging {
				if let Some(drag_last) = self.drag_last {
					let drag_delta_x = pointer_position.x - drag_last.x;
					let drag_delta_y = pointer_position.y - drag_last.y;
					if drag_delta_x.abs() + drag_delta_y.abs() > 3.0 {
						self.has_mouse_drag_moved = true;
					}
					self.pending_drag_yaw_delta += drag_delta_x * 0.0052;
					self.pending_drag_pitch_delta -= drag_delta_y * 0.0038;
				}
				self.drag_last = Some(pointer_position);
			}
		}
		if !is_touch_frame && (left_just_released || !left_down) {
			if left_just_released && self.is_mouse_dragging && !self.has_mouse_drag_moved {
				self.jump_pressed = true;
			}
			self.is_mouse_dragging = false;
			self.is_jump_button_held = false;
			self.is_interact_button_held = false;
			self.active_slider_index = None;
			self.drag_last = None;
		}
		if self.has_any_input_happened && !self.hint_fade_started {
			self.hint_fade_started = true;
			self.hint_fade_start_time = elapsed_time + 4.0;
		}

		// 카메라 회전 입력. (방향키, 드래그, 게임패드 오른스틱, 목표값에 누적)
		let mut camera_yaw_velocity = 0.0;
		let mut camera_pitch_velocity = 0.0;
		if key_list.arrow_right {
			camera_yaw_velocity += 1.3;
		}
		if key_list.arrow_left {
			camera_yaw_velocity -= 1.3;
		}
		if key_list.arrow_up {
			camera_pitch_velocity += 0.9;
		}
		if key_list.arrow_down {
			camera_pitch_velocity -= 0.9;
		}
		camera_yaw_velocity += self.gamepad_camera_x * 1.8;
		camera_pitch_velocity -= self.gamepad_camera_y * 1.3;
		self.camera_yaw_target += camera_yaw_velocity * delta_time + self.pending_drag_yaw_delta * 0.72;
		self.camera_pitch_target += camera_pitch_velocity * delta_time + self.pending_drag_pitch_delta * 0.72;
		self.pending_drag_yaw_delta = 0.0;
		self.pending_drag_pitch_delta = 0.0;
		self.camera_pitch_target = self.camera_pitch_target.clamp(-0.78, 0.55);

		// 이동 중 카메라 소프트 팔로우.
		if self.character_state == CharacterState::Ground && self.current_move_speed > 0.5 && self.drag_last.is_none() {
			let follow_yaw_difference = wrap_angle(self.character_yaw - self.camera_yaw_target);
			let follow_rate = 2.2 * (self.current_move_speed / RUN_SPEED).min(1.0);
			self.camera_yaw_target += follow_yaw_difference * (follow_rate * delta_time).min(1.0);
		}

		// 등반 중 카메라 재정렬.
		if self.character_state == CharacterState::ClimbAttach || self.character_state == CharacterState::Climbing {
			let recenter_rate = if self.character_state == CharacterState::ClimbAttach { 10.0 } else { 2.2 };
			let recenter_yaw_difference = wrap_angle(self.character_yaw - self.camera_yaw_target);
			self.camera_yaw_target += recenter_yaw_difference * (recenter_rate * delta_time).min(1.0);
			self.camera_pitch_target += (0.28 - self.camera_pitch_target) * (recenter_rate * 0.8 * delta_time).min(1.0);
		}

		// 카메라 관성 추적.
		let camera_yaw_follow_difference = wrap_angle(self.camera_yaw_target - self.camera_yaw);
		self.camera_yaw += camera_yaw_follow_difference * (7.5 * delta_time).min(1.0);
		self.camera_pitch += (self.camera_pitch_target - self.camera_pitch) * (7.5 * delta_time).min(1.0);

		// 여우 위치 갱신과 충돌 목록. (캐릭터 갱신에 앞서)
		let mut fox_list = std::mem::take(&mut self.resources.as_mut().unwrap().fox_list);
		self.update_fox_positions(&mut fox_list);
		let fox_block_list: Vec<(f64, f64, f64, f64)> = fox_list.iter().map(|fox| (fox.current_x, fox.current_z, fox.block_radius, fox.top_height)).collect();
		self.resources.as_mut().unwrap().fox_list = fox_list;

		// 캐릭터 갱신.
		self.update_character(delta_time, &key_list, &fox_block_list);

		// 모션 블러 강도와 착지 충격 감쇠.
		let mut motion_blur_target_strength = 0.0;
		if self.character_state == CharacterState::JumpDown {
			motion_blur_target_strength = ((-self.jump_velocity_y).max(0.0) / 26.0).min(0.28);
		}
		motion_blur_target_strength += self.landing_impulse * 0.2;
		motion_blur_target_strength += (self.current_move_speed - RUN_SPEED).max(0.0) / (SPRINT_SPEED - RUN_SPEED) * 0.03;
		let blur_smoothing_rate = if motion_blur_target_strength > self.motion_blur_current_strength { 10.0 } else { 16.0 };
		self.motion_blur_current_strength += (motion_blur_target_strength - self.motion_blur_current_strength) * (blur_smoothing_rate * delta_time).min(1.0);
		if self.motion_blur_current_strength < 0.02 && motion_blur_target_strength < 0.01 {
			self.motion_blur_current_strength = 0.0;
		}
		self.landing_impulse -= self.landing_impulse * 5.5 * delta_time;

		self.update_character_animation(delta_time);

		// 옵션 토글, 패널 접힘, 실시간 그림자 보간.
		for toggle_index in 0..5 {
			let option_target = if self.option_state_list[toggle_index] { 1.0 } else { 0.0 };
			self.option_blend_list[toggle_index] += (option_target - self.option_blend_list[toggle_index]) * (7.0 * delta_time).min(1.0);
		}
		let collapse_target = if self.is_panel_collapsed { 0.0 } else { 1.0 };
		self.panel_collapse_blend += (collapse_target - self.panel_collapse_blend) * (8.0 * delta_time).min(1.0);
		let shadow_realtime_target = if self.shadow_state == 2 { 1.0 } else { 0.0 };
		self.shadow_realtime_blend += (shadow_realtime_target - self.shadow_realtime_blend) * (7.0 * delta_time).min(1.0);

		// 숄더뷰 카메라. (어깨 오프셋 피벗 + 지형 차폐 회피)
		let camera_forward_x = self.camera_yaw.sin() * self.camera_pitch.cos();
		let camera_forward_y = self.camera_pitch.sin();
		let camera_forward_z = -self.camera_yaw.cos() * self.camera_pitch.cos();
		let camera_right_x = self.camera_yaw.cos();
		let camera_right_z = self.camera_yaw.sin();
		let is_climbing_state = matches!(self.character_state, CharacterState::ClimbAttach | CharacterState::Climbing | CharacterState::ClimbCancel);
		let is_air_state = matches!(self.character_state, CharacterState::JumpDown | CharacterState::LandRecover);
		let pivot_target_y = self.player_y + if is_climbing_state { 2.0 } else { 1.55 };
		let pivot_follow_rate = if is_air_state { 3.5 } else { 10.0 };
		self.smoothed_pivot_y += (pivot_target_y - self.smoothed_pivot_y) * (pivot_follow_rate * delta_time).min(1.0);
		let shoulder_side_offset = if is_climbing_state { 0.15 } else { 0.55 };
		let shoulder_pivot_x = self.player_x + camera_right_x * shoulder_side_offset;
		let camera_shake_offset = (elapsed_time * 55.0).sin() * self.landing_impulse * 0.035;
		let shoulder_pivot_y = self.smoothed_pivot_y + camera_shake_offset;
		let shoulder_pivot_z = self.player_z + camera_right_z * shoulder_side_offset;
		let camera_distance_target = if is_climbing_state { 5.4 } else if is_air_state { 4.6 } else { 3.6 };
		self.smoothed_camera_distance += (camera_distance_target - self.smoothed_camera_distance) * (3.2 * delta_time).min(1.0);
		let camera_distance = self.smoothed_camera_distance;
		let mut camera_distance_factor = 1.0;
		for sample_index in 1..=24 {
			let sample_factor = sample_index as f64 / 24.0;
			let sample_x = shoulder_pivot_x - camera_forward_x * camera_distance * sample_factor;
			let sample_y = shoulder_pivot_y - camera_forward_y * camera_distance * sample_factor;
			let sample_z = shoulder_pivot_z - camera_forward_z * camera_distance * sample_factor;
			let sample_ground_y = self.world.terrain_surface_height_at(sample_x, sample_z) + 0.26;
			if sample_y < sample_ground_y {
				camera_distance_factor = (sample_factor - 0.05).max(0.2);
				break;
			}
		}
		let distance_smoothing_rate = if camera_distance_factor < self.smoothed_camera_distance_factor { 9.0 } else { 3.0 };
		self.smoothed_camera_distance_factor += (camera_distance_factor - self.smoothed_camera_distance_factor) * (distance_smoothing_rate * delta_time).min(1.0);
		let camera_position_x = shoulder_pivot_x - camera_forward_x * camera_distance * self.smoothed_camera_distance_factor;
		let mut camera_position_y = shoulder_pivot_y - camera_forward_y * camera_distance * self.smoothed_camera_distance_factor;
		let camera_position_z = shoulder_pivot_z - camera_forward_z * camera_distance * self.smoothed_camera_distance_factor;
		let camera_ground_y = self.world.terrain_surface_height_at(camera_position_x, camera_position_z) + 0.3;
		camera_position_y = camera_position_y.max(camera_ground_y);
		self.camera_position = [camera_position_x, camera_position_y, camera_position_z];

		// 뷰, 투영 행렬. (질주 시 화각 확장)
		let speed_factor = (self.current_move_speed / RUN_SPEED).min(1.6);
		let field_of_view_radian = (55.0 + speed_factor * 4.0) * std::f64::consts::PI / 180.0;
		let aspect_ratio = if window_size.y > 0.0 { window_size.x / window_size.y } else { 1.0 };
		let camera_position = Vector3::create(camera_position_x, camera_position_y, camera_position_z);
		let camera_target = Vector3::create(camera_position_x + camera_forward_x, camera_position_y + camera_forward_y, camera_position_z + camera_forward_z);
		let view_matrix = Matrix4::create_look_at(camera_position, camera_target, Vector3::up());
		let mut view_projection_matrix = Matrix4::create_perspective(field_of_view_radian, aspect_ratio, 0.1, 520.0);
		view_projection_matrix.multiply(&view_matrix);
		self.view_projection_matrix = view_projection_matrix;
		self.jump_pressed_consumed();
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 3D 패스 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		graphic.flush_batch();
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		let aspect_ratio = canvas_pixel_size.x / canvas_pixel_size.y.max(1.0);
		// 투영은 실제 그리는 크기의 가로세로 비로 다시 곱한다. (tick 이 쓴 창 논리 크기의 비와 같은 값이어야 한다)
		let _ = aspect_ratio;
		if let Err(error) = self.render_scene(graphic.get_gl(), canvas_pixel_size.x as i32, canvas_pixel_size.y as i32) {
			eprintln!("ancientmountain: {}", error);
		}
		graphic.restore_render_state();
	}

	//==============================================================================
	// 2D 출력. (제목, 링크, 저작자 표기, HUD, 옵션 패널, 인트로 페이드)
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		engine.get_view_manager().apply_canvas_native_rect(graphic);
		let window_size = engine.get_view_manager().get_canvas_native_size();
		if self.resources.is_none() {
			graphic.set_fill_color(Color::white());
			graphic.set_font_string("16px default");
			graphic.set_text_align(TextAlign::Left);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.draw_fill_text(&self.load_error_message.clone().unwrap_or_else(|| String::from("loading...")), 24.0, 32.0);
			return;
		}
		let elapsed_time = self.elapsed_time;
		graphic.set_global_alpha(1.0);
		graphic.set_text_baseline(TextBaseline::Middle);

		// 제목, 링크, 저작자 표기. (원본 HTML 오버레이를 같은 CSS 값으로 구워 그린다)
		self.draw_overlay(graphic, window_size);
		graphic.set_global_alpha(1.0);
		graphic.set_text_baseline(TextBaseline::Middle);

		// 등반 프롬프트. (절벽의 손 닿는 높이에 O 아이콘 + 레이블)
		self.prompt_screen_position = None;
		if self.character_state == CharacterState::Ground {
			if let Some(target) = &self.climb_available_target {
				if let Some(screen) = self.project_world_to_screen(target.wall_x, target.prompt_y, target.wall_z, window_size) {
					self.prompt_screen_position = Some(screen);
					AncientMountainScene::draw_prompt(graphic, screen, "등반", elapsed_time);
				}
			}
			else if let Some(target) = &self.descend_available_target {
				if let Some(screen) = self.project_world_to_screen(target.edge_x, target.prompt_y, target.edge_z, window_size) {
					self.prompt_screen_position = Some(screen);
					AncientMountainScene::draw_prompt(graphic, screen, "뛰어내리기", elapsed_time);
				}
			}
		}

		// 등반 중 취소 안내.
		if self.character_state == CharacterState::Climbing {
			if let Some(cancel_screen) = self.project_world_to_screen(self.player_x, self.player_y + 2.3, self.player_z, window_size) {
				graphic.set_fill_color(rgba(0.0, 0.0, 0.0, 0.78));
				graphic.draw_circle(cancel_screen, 15.0);
				graphic.set_stroke_color(rgba(255.0, 255.0, 255.0, 0.4));
				graphic.draw_stroke_circle(cancel_screen, 15.0, 1.6);
				AncientMountainScene::draw_circle_icon(graphic, cancel_screen.x, cancel_screen.y, 10.0, rgba(255.0, 255.0, 255.0, 0.95), 2.4);
				graphic.set_font_string(r#"600 13px "Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif"#);
				graphic.set_text_align(TextAlign::Left);
				AncientMountainScene::draw_shadowed_text(graphic, "취소", cancel_screen.x + 24.0, cancel_screen.y);
			}
		}

		// 정상 카운터 칩. (좌상단)
		let counter_rect = Rect::create(14.0, 14.0, 132.0, 30.0);
		graphic.set_fill_color(rgba(12.0, 20.0, 32.0, 0.55));
		graphic.draw_round_rect(counter_rect, 15.0);
		graphic.set_stroke_color(rgba(255.0, 226.0, 150.0, 0.28));
		graphic.draw_stroke_round_rect(counter_rect, 15.0, 1.0);
		graphic.set_font_string(r#"500 14px "Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif"#);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_fill_color(rgba(255.0, 205.0, 90.0, 0.95));
		graphic.draw_fill_text("\u{25b2}", 28.0, 30.0);
		AncientMountainScene::draw_shadowed_text(graphic, &format!("정상 {} / {}", self.cleared_summit_count, self.world.hill_count), 46.0, 30.0);

		// 전체 클리어 배너.
		let all_cleared_elapsed = elapsed_time - self.all_cleared_time;
		if all_cleared_elapsed > 0.0 && all_cleared_elapsed < 7.0 {
			let banner_alpha = (all_cleared_elapsed / 0.8).min(1.0) * (1.0 - smoothstep_value(5.5, 7.0, all_cleared_elapsed));
			graphic.set_global_alpha(banner_alpha);
			graphic.set_font_string(r#"300 34px "Segoe UI Light", "Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif"#);
			graphic.set_text_align(TextAlign::Center);
			AncientMountainScene::draw_shadowed_text(graphic, "모든 정상을 정복했습니다", window_size.x * 0.5, window_size.y * 0.36);
			graphic.set_font_string(r#"300 14px "Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif"#);
			AncientMountainScene::draw_shadowed_text(graphic, "잃어버린 순환로의 여섯 봉우리를 모두 올랐습니다", window_size.x * 0.5, window_size.y * 0.36 + 34.0);
			graphic.set_global_alpha(1.0);
		}

		// 가상 조이스틱. (터치 기기)
		if self.is_touch_device {
			self.virtual_pad.tick(self.delta_time);
			self.virtual_pad.draw(graphic);
		}

		// X, O 단추. (우하단)
		let jump_button_center = AncientMountainScene::jump_button_center(window_size);
		let interact_button_center = AncientMountainScene::interact_button_center(window_size);
		let is_action_ready = self.character_state == CharacterState::Ground && (self.climb_available_target.is_some() || self.descend_available_target.is_some());
		let is_interact_ready = is_action_ready || self.character_state == CharacterState::Climbing;
		graphic.set_fill_color(if self.is_jump_button_held { rgba(45.0, 45.0, 45.0, 0.75) } else { rgba(0.0, 0.0, 0.0, 0.6) });
		graphic.draw_circle(jump_button_center, BUTTON_RADIUS);
		graphic.set_stroke_color(rgba(255.0, 255.0, 255.0, 0.35));
		graphic.draw_stroke_circle(jump_button_center, BUTTON_RADIUS, 1.6);
		AncientMountainScene::draw_cross_icon(graphic, jump_button_center.x, jump_button_center.y, BUTTON_RADIUS * 0.44, rgba(255.0, 255.0, 255.0, 0.95), 3.4);
		let interact_glow_radius = BUTTON_RADIUS * 0.82 + if is_interact_ready { 3.0 + (elapsed_time * 6.0).sin() * 2.0 } else { 0.0 };
		graphic.set_fill_color(if self.is_interact_button_held { rgba(45.0, 45.0, 45.0, 0.75) } else { rgba(0.0, 0.0, 0.0, 0.6) });
		graphic.draw_circle(interact_button_center, interact_glow_radius);
		graphic.set_stroke_color(if is_interact_ready { rgba(255.0, 255.0, 255.0, 0.9) } else { rgba(255.0, 255.0, 255.0, 0.35) });
		graphic.draw_stroke_circle(interact_button_center, interact_glow_radius, if is_interact_ready { 2.6 } else { 1.6 });
		AncientMountainScene::draw_circle_icon(graphic, interact_button_center.x, interact_button_center.y, interact_glow_radius * 0.52, if is_interact_ready { rgba(255.0, 255.0, 255.0, 0.98) } else { rgba(255.0, 255.0, 255.0, 0.8) }, 3.0);

		// 조작 안내. (입력 시작 후 서서히 사라짐)
		let hint_alpha = if self.has_any_input_happened { (1.0 - (elapsed_time - self.hint_fade_start_time) / 3.0).max(0.0) } else { 1.0 };
		if hint_alpha > 0.01 {
			graphic.set_global_alpha(hint_alpha);
			graphic.set_font_string(r#"400 12px "Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif"#);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.set_text_align(TextAlign::Left);
			let hint_base_y = window_size.y - 148.0;
			let hint_card_rect = if self.is_touch_device { Rect::create(10.0, hint_base_y - 16.0, 252.0, 72.0) } else { Rect::create(10.0, hint_base_y - 36.0, 366.0, 116.0) };
			graphic.set_fill_color(rgba(12.0, 20.0, 32.0, 0.5));
			graphic.draw_round_rect(hint_card_rect, 12.0);
			graphic.set_stroke_color(rgba(255.0, 226.0, 150.0, 0.2));
			graphic.draw_stroke_round_rect(hint_card_rect, 12.0, 1.0);
			if self.is_touch_device {
				AncientMountainScene::draw_shadowed_text(graphic, "왼쪽 드래그 \u{2014} 이동 (끝까지 밀면 질주)", 18.0, hint_base_y);
				AncientMountainScene::draw_shadowed_text(graphic, "오른쪽 드래그 \u{2014} 시점", 18.0, hint_base_y + 20.0);
				AncientMountainScene::draw_shadowed_text(graphic, "\u{2715} 점프 \u{00b7} \u{25cb} 등반 / 뛰어내리기", 18.0, hint_base_y + 40.0);
			}
			else {
				AncientMountainScene::draw_shadowed_text(graphic, "W A S D \u{2014} 이동  \u{00b7}  SHIFT / R2 \u{2014} 질주", 18.0, hint_base_y - 20.0);
				AncientMountainScene::draw_shadowed_text(graphic, "방향키 / 마우스 드래그 \u{2014} 시점", 18.0, hint_base_y);
				AncientMountainScene::draw_shadowed_text(graphic, "SPACE / 좌클릭 \u{2014} 점프 (\u{2715})", 18.0, hint_base_y + 20.0);
				AncientMountainScene::draw_shadowed_text(graphic, "O / 우클릭 \u{2014} 등반 \u{00b7} 뛰어내리기 \u{00b7} 취소 (\u{25cb})", 18.0, hint_base_y + 40.0);
				AncientMountainScene::draw_shadowed_text(graphic, "절벽을 바라보면 등반 \u{00b7} 가장자리에서 아래를 보면 뛰어내리기", 18.0, hint_base_y + 60.0);
			}
			graphic.set_global_alpha(1.0);
		}

		// 충돌 영역 표시. (COLLISION 옵션)
		if self.option_state_list[3] {
			let mut ring_list: Vec<(f64, f64, f64, Color)> = Vec::new();
			for rock in self.rock_list.iter() {
				let rock_delta_x = rock.center_x - self.player_x;
				let rock_delta_z = rock.center_z - self.player_z;
				if rock_delta_x * rock_delta_x + rock_delta_z * rock_delta_z < 45.0 * 45.0 {
					ring_list.push((rock.center_x, rock.center_z, rock.block_radius, rgba(120.0, 220.0, 255.0, 0.85)));
				}
			}
			if let Some(resources) = &self.resources {
				for fox in resources.fox_list.iter() {
					ring_list.push((fox.current_x, fox.current_z, fox.block_radius, rgba(255.0, 180.0, 90.0, 0.9)));
				}
			}
			ring_list.push((self.player_x, self.player_z, 0.38, rgba(255.0, 120.0, 200.0, 0.9)));
			for (ring_center_x, ring_center_z, ring_radius, ring_color) in ring_list {
				let mut ring_position_list: Vec<Vector2> = Vec::new();
				let mut is_complete = true;
				for segment_index in 0..16 {
					let segment_angle = (segment_index as f64 / 16.0) * std::f64::consts::PI * 2.0;
					let ring_world_x = ring_center_x + segment_angle.cos() * ring_radius;
					let ring_world_z = ring_center_z + segment_angle.sin() * ring_radius;
					let ring_world_y = self.world.terrain_surface_height_at(ring_world_x, ring_world_z) + 0.15;
					match self.project_world_to_screen(ring_world_x, ring_world_y, ring_world_z, window_size) {
						Some(ring_screen) => {
							ring_position_list.push(ring_screen);
						}
						None => {
							is_complete = false;
							break;
						}
					}
				}
				if is_complete {
					graphic.set_stroke_color(ring_color);
					graphic.draw_line_loop(&ring_position_list, 2.0);
				}
			}
		}

		// 입력 상태 표시. (좌하단, 이동 벡터 + 질주, 점프, 상호작용 눌림)
		let (indicator_sprint_held, indicator_jump_held, indicator_interact_held) = {
			let input_manager = engine.get_input_manager();
			(
				input_manager.is_key_pressed("ShiftLeft") || input_manager.is_key_pressed("ShiftRight") || self.gamepad_sprint_held,
				input_manager.is_key_pressed("Space") || input_manager.is_key_pressed("KeyX") || self.previous_gamepad_jump || self.is_jump_button_held,
				input_manager.is_key_pressed("KeyO") || input_manager.is_key_pressed("Escape") || self.previous_gamepad_interact || self.is_interact_button_held,
			)
		};
		let indicator_center_x = 46.0;
		let indicator_center_y = window_size.y - if self.is_touch_device { 258.0 } else { 40.0 };
		let indicator_center = Vector2::create(indicator_center_x, indicator_center_y);
		graphic.set_fill_color(rgba(12.0, 20.0, 32.0, 0.5));
		graphic.draw_circle(indicator_center, 26.0);
		graphic.set_stroke_color(rgba(255.0, 226.0, 150.0, 0.25));
		graphic.draw_stroke_circle(indicator_center, 26.0, 1.0);
		let move_magnitude = (self.current_move_input_x * self.current_move_input_x + self.current_move_input_z * self.current_move_input_z).sqrt();
		graphic.set_fill_color(if move_magnitude > 0.05 { rgba(255.0, 236.0, 180.0, 0.95) } else { rgba(238.0, 240.0, 244.0, 0.35) });
		graphic.draw_circle(Vector2::create(indicator_center_x + self.current_move_input_x * 15.0, indicator_center_y - self.current_move_input_z * 15.0), 6.0);
		graphic.set_font_string(r#"500 11px "Segoe UI", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif"#);
		graphic.set_text_align(TextAlign::Center);
		let mut indicator_chip_x = indicator_center_x + 38.0;
		for (label, is_held, chip_width) in [("질주", indicator_sprint_held, 46.0), ("점프", indicator_jump_held, 46.0), ("상호작용", indicator_interact_held, 66.0)] {
			let chip_rect = Rect::create(indicator_chip_x, indicator_center_y - 11.0, chip_width, 22.0);
			graphic.set_fill_color(if is_held { rgba(232.0, 186.0, 90.0, 0.85) } else { rgba(12.0, 20.0, 32.0, 0.5) });
			graphic.draw_round_rect(chip_rect, 11.0);
			graphic.set_stroke_color(if is_held { rgba(255.0, 236.0, 180.0, 0.9) } else { rgba(238.0, 240.0, 244.0, 0.2) });
			graphic.draw_stroke_round_rect(chip_rect, 11.0, 1.0);
			graphic.set_fill_color(if is_held { rgba(20.0, 16.0, 8.0, 0.95) } else { rgba(238.0, 240.0, 244.0, 0.6) });
			graphic.draw_fill_text(label, indicator_chip_x + chip_width * 0.5, indicator_center_y + 1.0);
			indicator_chip_x += chip_width + 8.0;
		}

		// 옵션 패널. (우측 상단)
		let (panel_x, panel_y, panel_height) = self.calculate_panel_layout(window_size.x);
		let panel_rect = Rect::create(panel_x, panel_y, PANEL_WIDTH, panel_height);
		graphic.set_fill_color(rgba(12.0, 20.0, 32.0, 0.62));
		graphic.draw_round_rect(panel_rect, 14.0);
		graphic.set_stroke_color(rgba(255.0, 226.0, 150.0, 0.28));
		graphic.draw_stroke_round_rect(panel_rect, 14.0, 1.0);
		graphic.set_font_string(r#"300 11px "Segoe UI", "Apple SD Gothic Neo", sans-serif"#);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_fill_color(rgba(255.0, 236.0, 180.0, 0.92));
		graphic.draw_fill_text("O P T I O N S", panel_x + 16.0, panel_y + PANEL_HEADER_HEIGHT * 0.5 + 1.0);
		graphic.set_text_align(TextAlign::Right);
		graphic.set_fill_color(rgba(255.0, 236.0, 180.0, 0.6));
		graphic.draw_fill_text(if self.is_panel_collapsed { "\u{25b8}" } else { "\u{25be}" }, panel_x + PANEL_WIDTH - 16.0, panel_y + PANEL_HEADER_HEIGHT * 0.5 + 1.0);
		if self.panel_collapse_blend > 0.05 {
			for (row_index, label) in OPTION_LABEL_LIST.iter().enumerate() {
				let row_y = panel_y + PANEL_HEADER_HEIGHT + 4.0 + row_index as f64 * PANEL_ROW_HEIGHT;
				if row_y + PANEL_ROW_HEIGHT > panel_y + panel_height {
					break;
				}
				let row_middle_y = row_y + PANEL_ROW_HEIGHT * 0.5;
				graphic.set_global_alpha(self.panel_collapse_blend);
				graphic.set_text_align(TextAlign::Left);
				graphic.set_font_string(r#"300 11px "Segoe UI", "Apple SD Gothic Neo", sans-serif"#);
				graphic.set_fill_color(rgba(238.0, 240.0, 244.0, 0.85));
				graphic.draw_fill_text(label, panel_x + 16.0, row_middle_y + 1.0);
				if row_index == OPTION_SHADOW {
					let chip_width = 78.0;
					let chip_rect = Rect::create(panel_x + PANEL_WIDTH - 16.0 - chip_width, row_middle_y - 9.5, chip_width, 19.0);
					graphic.set_fill_color(rgba(232.0, 186.0, 90.0, 0.2));
					graphic.draw_round_rect(chip_rect, 9.5);
					graphic.set_stroke_color(rgba(255.0, 226.0, 150.0, 0.5));
					graphic.draw_stroke_round_rect(chip_rect, 9.5, 1.0);
					graphic.set_text_align(TextAlign::Center);
					graphic.set_font_string(r#"600 10px "Segoe UI", "Apple SD Gothic Neo", sans-serif"#);
					graphic.set_fill_color(rgba(255.0, 236.0, 180.0, 0.95));
					graphic.draw_fill_text(SHADOW_STATE_LABEL_LIST[self.shadow_state], panel_x + PANEL_WIDTH - 16.0 - chip_width * 0.5, row_middle_y + 1.0);
				}
				else if row_index == OPTION_GLOW || row_index == OPTION_MOTION_BLUR {
					let slider_normalized = if row_index == OPTION_GLOW { self.glow_value / 1.5 } else { self.motion_blur_value };
					let track_x = AncientMountainScene::slider_track_x(panel_x);
					graphic.set_fill_color(rgba(238.0, 240.0, 244.0, 0.18));
					graphic.draw_round_rect(Rect::create(track_x, row_middle_y - 2.0, SLIDER_TRACK_WIDTH, 4.0), 2.0);
					graphic.set_fill_color(rgba(232.0, 186.0, 90.0, 0.92));
					graphic.draw_round_rect(Rect::create(track_x, row_middle_y - 2.0, SLIDER_TRACK_WIDTH * slider_normalized, 4.0), 2.0);
					graphic.set_fill_color(rgba(255.0, 250.0, 240.0, 1.0));
					graphic.draw_circle(Vector2::create(track_x + SLIDER_TRACK_WIDTH * slider_normalized, row_middle_y), 6.0);
				}
				else {
					let toggle_blend = self.option_blend_list[row_index - 1];
					let switch_x = panel_x + PANEL_WIDTH - 16.0 - 36.0;
					let track_rect = Rect::create(switch_x, row_middle_y - 9.5, 36.0, 19.0);
					let track_red = 0.14 + (0.85 - 0.14) * toggle_blend;
					let track_green = 0.17 + (0.68 - 0.17) * toggle_blend;
					let track_blue = 0.22 + (0.3 - 0.22) * toggle_blend;
					graphic.set_fill_color(Color::create(track_red, track_green, track_blue, 0.85));
					graphic.draw_round_rect(track_rect, 9.5);
					graphic.set_stroke_color(rgba(255.0, 226.0, 150.0, 0.25 + toggle_blend * 0.45));
					graphic.draw_stroke_round_rect(track_rect, 9.5, 1.0);
					graphic.set_fill_color(if toggle_blend > 0.5 { rgba(255.0, 250.0, 240.0, 1.0) } else { rgba(238.0, 240.0, 244.0, 0.75) });
					graphic.draw_circle(Vector2::create(switch_x + 9.5 + toggle_blend * 17.0, row_middle_y), 6.5);
				}
			}
			graphic.set_global_alpha(1.0);
		}

		// 인트로 페이드인. (자연 지형에 어울리는 아침 안개 톤)
		let intro_alpha = 1.0 - (elapsed_time / 1.6).min(1.0);
		if intro_alpha > 0.001 {
			graphic.set_fill_color(rgba(206.0, 214.0, 200.0, intro_alpha));
			graphic.draw_rect(Rect::create(0.0, 0.0, window_size.x, window_size.y));
		}
	}
}

impl AncientMountainScene {
	//==============================================================================
	// 프레임 눌림 플래그 초기화. (tick 끝에서 부른다)
	//==============================================================================
	fn jump_pressed_consumed(&mut self) {
		self.jump_pressed = false;
		self.interact_pressed = false;
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("바닐라엔진 ancientmountain");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.window_size = vanilla::Vector2::create(1280.0, 720.0);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	if let Ok(home_directory) = std::env::var("HOME") {
		engine_configuration.asset_directory_paths.push(PathBuf::from(home_directory).join("Github/vanilla.js/examples/ancientmountain/assets"));
	}
	let engine = Engine::new(engine_configuration);
	// 렌더 배율 상한 2. (원본 index.html 의 Math.min(devicePixelRatio, 2))
	engine.get_view_manager_mut().set_max_render_pixel_ratio(2.0);
	if let Err(error) = engine.run(Box::new(AncientMountainScene::new())) {
		eprintln!("{}", error);
	}
}
