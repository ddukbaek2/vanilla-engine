//==============================================================================
// 노인 얼굴 예제. (원본 examples/oldface 의 HumanSkinRenderer 장면을 엔진 루프 안에서 돌린다)
// - 원본 애셋(character_lod1.glb, hair_lod2.glb, expressions.json, wrinkles.json, 피부, 눈, 머리카락, 수염 텍스처)을 BlobAsset 으로 읽는다.
//   애셋은 examples/oldface_assets 에 있다. (MetaHuman, Epic Games, 언리얼 엔진 EULA, 데모 전용. 웹은 페이지 기준 상대 주소로 받는다)
// - 원본 기본값 그대로: 머리 LOD 1, 머리카락 HIGH(hair_lod2, 프린지 블렌드, 흔들림 0.8), 2K 그림자 8 표본, 렌더 기준 1080P, FXAA.
// - 패스는 원본과 같다: 섀도우 깊이 → 피부 MRT(배경 + 불투명) → 블렌드(머리카락 프린지, 눈꺼풀 차폐, 눈물선) → AO → SSS
//   → 합성(HDR) → 블룸 → 톤 매핑(LDR) → FXAA(화면) → graphic.restore_render_state() → draw 에서 2D 글.
// - 원본 예제의 HTML 패널(옵션, 슬라이더, 머티리얼 그래프, HUD)은 옮기지 않는다. 표정은 4초마다 차례로 바뀌고,
//   숫자 키 1 ~ 9 로 고르면 자동 전환이 멈춘다. 끌기로 궤도 회전, 휠로 줌.
// - 원본 FRAME RATE 기본처럼 30fps 로 제한하고, 갱신 시간은 원본처럼 지난 시간(0.1초 상한)을 쓴다.
//   화면 저장(VANILLA_CAPTURE_PATH)을 켜면 1/30초씩 고정으로 나아간다. (프레임 번호로 같은 장면을 잡도록)
// - 실행: cargo run --release --example oldface
//   화면 저장: VANILLA_CAPTURE_PATH=shots/oldface_{frame}.png VANILLA_CAPTURE_FRAMES=61 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use serde_json::Value;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::{
	AmbientOcclusionEffect, GlContext, BlobAsset, BloomEffect, Color, Engine, EngineConfiguration, FullscreenPass, Graphic, HumanSkinRenderer, ImageDescription, ImageDescriptionOptions, KeyCode, Material, Matrix4, MouseButton, Rect, RenderTarget, RenderTargetOptions, Scene, SceneBase, SeededRandom, ShadowMap, SkinnedModel, SubsurfaceScatteringEffect, TextAlign, TextBaseline, Vector2, Vector3, SKIN_SHADING_EYE, SKIN_SHADING_FACIALHAIR, SKIN_SHADING_FLUID, SKIN_SHADING_HAIR, SKIN_SHADING_OCCLUSION, SKIN_SHADING_SKIN, SKIN_TEXTURE_UNIT_DETAILHEIGHT, SKIN_TEXTURE_UNIT_LAYERCOLOR, SKIN_TEXTURE_UNIT_LAYERNORMAL, SKIN_TEXTURE_UNIT_MASKATLAS, SKIN_TEXTURE_UNIT_MICRO,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 예제 애셋 폴더. (MetaHuman 자산은 Epic Games 의 것으로 언리얼 엔진 EULA 를 따르고, 이 데모 안에서만 쓴다. 라이선스는 폴더의 ASSETS_LICENSE.md)
const EXAMPLE_ASSET_DIRECTORY: &str = "examples/oldface_assets";
/// 화면 아래 저작권 고지.
const ATTRIBUTION_TEXT: &str = "MetaHuman, Epic Games, 데모 전용, https://www.metahuman.com/ , https://www.metahuman.com/en-US/license";

// 카메라. (원본 값)
const CAMERA_FIELD_OF_VIEW: f64 = 0.44;
const CAMERA_NEAR_DISTANCE: f64 = 0.05;
const CAMERA_FAR_DISTANCE: f64 = 20.0;
const CAMERA_MINIMUM_DISTANCE: f64 = 0.42;
const CAMERA_MAXIMUM_DISTANCE: f64 = 1.9;
const CAMERA_DEFAULT_PITCH: f64 = 0.02;
const CAMERA_DEFAULT_DISTANCE: f64 = 0.68;

// 그림자. (원본 SHADOW MAP 기본 단계 2K, 표본 8)
const SHADOWMAP_RESOLUTION: i32 = 2048;
const SHADOW_SAMPLE_COUNT: i32 = 8;

// 내부 렌더 기준 세로 해상도. (원본 RENDER BASE 기본 1080P, 화면이 더 크면 줄이고 작으면 슈퍼샘플링)
const RENDER_BASE_HEIGHT: f64 = 1080.0;

// 높이 디테일 텍스처 한 변. (원본 DETAIL_TEXTURE_SIZE)
const DETAIL_TEXTURE_SIZE: f64 = 4096.0;

// 프레임 제한과 화면 저장 때의 고정 갱신 걸음. (원본 FRAME RATE 기본 30fps)
const FRAME_RATE: f64 = 30.0;
const FIXED_TIME_STEP: f64 = 1.0 / FRAME_RATE;

// 표정 자동 전환 간격. (초)
const EXPRESSION_CYCLE_SECONDS: f64 = 4.0;

// 애셋 파일. (원본 HEAD_LOD_FILES[1], HAIR_LEVEL_DEFINITIONS[3])
const HEAD_MODEL_FILE: &str = "character_lod1.glb";
const HAIR_MODEL_FILE: &str = "hair_lod2.glb";
const HAIR_SWAY_SCALE: f64 = 0.8;
const DETAIL_HEIGHT_FILE: &str = "skin_detail.jpg";

// 조명 방향. (원본 KEY / FILL / RIM_LIGHT_DIRECTION, 정규화 전)
const KEY_LIGHT_DIRECTION: [f64; 3] = [0.5, 0.62, 0.62];
const FILL_LIGHT_DIRECTION: [f64; 3] = [-0.75, 0.1, 0.6];
const RIM_LIGHT_DIRECTION: [f64; 3] = [0.55, 0.45, -0.72];

// 표정 프리셋 차례. (원본 EXPRESSION_DEFINITIONS, expressions.json 의 키와 표시 이름)
const EXPRESSION_DEFINITIONS: [(&str, &str); 9] = [("serious", "SERIOUS"), ("happy", "HAPPY"), ("grin", "GRIN"), ("excited", "EXCITED"), ("angry", "ANGRY"), ("furious", "FURIOUS"), ("disgusted", "DISGUSTED"), ("bored", "BORED"), ("confused", "CONFUSED")];

// 숫자 키. (표정 고르기)
const DIGIT_KEYS: [&str; 9] = [KeyCode::DIGIT1, KeyCode::DIGIT2, KeyCode::DIGIT3, KeyCode::DIGIT4, KeyCode::DIGIT5, KeyCode::DIGIT6, KeyCode::DIGIT7, KeyCode::DIGIT8, KeyCode::DIGIT9];

//==============================================================================
// 부위별 머티리얼 정의. (원본 PART_MATERIAL_DEFINITIONS, glb 머티리얼 이름 부분 일치 → 텍스처 / 팩터 / 산란 마스크)
//==============================================================================
struct PartMaterialDefinition {
	keyword: &'static str,
	shading_mode: i32,
	albedo: Option<&'static str>,
	normal: Option<&'static str>,
	roughness: Option<&'static str>,
	specular: Option<&'static str>,
	opacity: Option<&'static str>,
	base_color_factor: Option<[f64; 3]>,
	use_detail: bool,
	roughness_factor: f64,
	subsurface_factor: f64,
	extra: &'static [(&'static str, u32, &'static str)],
}

const PART_MATERIAL_DEFINITIONS: [PartMaterialDefinition; 11] = [
	PartMaterialDefinition { keyword: "eyeshell", shading_mode: SKIN_SHADING_OCCLUSION, albedo: Some("eye_occlusion.png"), normal: None, roughness: None, specular: None, opacity: None, base_color_factor: Some([0.75, 1.0, 1.0]), use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[] },
	PartMaterialDefinition { keyword: "eyeEdge", shading_mode: SKIN_SHADING_FLUID, albedo: None, normal: None, roughness: None, specular: None, opacity: None, base_color_factor: Some([0.07, 1.0, 1.0]), use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[] },
	PartMaterialDefinition { keyword: "saliva", shading_mode: SKIN_SHADING_FLUID, albedo: None, normal: None, roughness: None, specular: None, opacity: None, base_color_factor: Some([0.4, 1.0, 1.0]), use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[] },
	PartMaterialDefinition { keyword: "eyeball", shading_mode: SKIN_SHADING_EYE, albedo: Some("eye_sclera.jpg"), normal: Some("eye_sclera_normal.jpg"), roughness: None, specular: None, opacity: None, base_color_factor: None, use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[("layerColorTexture", SKIN_TEXTURE_UNIT_LAYERCOLOR, "eye_iris.png"), ("layerNormalTexture", SKIN_TEXTURE_UNIT_LAYERNORMAL, "eye_iris_normal.png")] },
	PartMaterialDefinition { keyword: "eyelashes", shading_mode: SKIN_SHADING_SKIN, albedo: Some("eyelash.png"), normal: None, roughness: None, specular: None, opacity: Some("eyelash.png"), base_color_factor: None, use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[] },
	PartMaterialDefinition { keyword: "head", shading_mode: SKIN_SHADING_SKIN, albedo: Some("head_albedo.jpg"), normal: Some("head_normal.jpg"), roughness: Some("head_roughness.jpg"), specular: Some("head_specular.jpg"), opacity: None, base_color_factor: None, use_detail: true, roughness_factor: 1.0, subsurface_factor: 1.0, extra: &[("microTexture", SKIN_TEXTURE_UNIT_MICRO, "skin_micro.png"), ("maskAtlasTexture", SKIN_TEXTURE_UNIT_MASKATLAS, "wrinkle_masks.png"), ("layerNormalTexture", SKIN_TEXTURE_UNIT_LAYERNORMAL, "wrinkle_normal.jpg"), ("layerColorTexture", SKIN_TEXTURE_UNIT_LAYERCOLOR, "wrinkle_color.jpg")] },
	PartMaterialDefinition { keyword: "teeth", shading_mode: SKIN_SHADING_SKIN, albedo: Some("teeth_albedo.jpg"), normal: Some("teeth_normal.jpg"), roughness: None, specular: None, opacity: None, base_color_factor: None, use_detail: false, roughness_factor: 0.4, subsurface_factor: 0.25, extra: &[] },
	PartMaterialDefinition { keyword: "hair", shading_mode: SKIN_SHADING_HAIR, albedo: Some("hair.png"), normal: None, roughness: None, specular: None, opacity: None, base_color_factor: None, use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[("layerNormalTexture", SKIN_TEXTURE_UNIT_LAYERNORMAL, "hair_tangent.png")] },
	PartMaterialDefinition { keyword: "beard", shading_mode: SKIN_SHADING_FACIALHAIR, albedo: Some("beard.png"), normal: None, roughness: None, specular: None, opacity: None, base_color_factor: None, use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[("layerNormalTexture", SKIN_TEXTURE_UNIT_LAYERNORMAL, "beard_tangent.png")] },
	PartMaterialDefinition { keyword: "mustache", shading_mode: SKIN_SHADING_FACIALHAIR, albedo: Some("mustache.png"), normal: None, roughness: None, specular: None, opacity: None, base_color_factor: None, use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[("layerNormalTexture", SKIN_TEXTURE_UNIT_LAYERNORMAL, "mustache_tangent.png")] },
	PartMaterialDefinition { keyword: "eyebrows", shading_mode: SKIN_SHADING_FACIALHAIR, albedo: Some("eyebrows.png"), normal: None, roughness: None, specular: None, opacity: None, base_color_factor: None, use_detail: false, roughness_factor: 1.0, subsurface_factor: 0.0, extra: &[("layerNormalTexture", SKIN_TEXTURE_UNIT_LAYERNORMAL, "eyebrows_tangent.png")] },
];

//==============================================================================
// 셰이더 소스 목록. (원본 예제와 같다)
//==============================================================================
// 합성. (산란된 조도 x 알베도 + 스펙큘러 → 선형 HDR)
const COMBINE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D irradianceTexture;
uniform sampler2D albedoTexture;
uniform sampler2D specularTexture;
uniform sampler2D occlusionTexture;
uniform float aspectRatio;
uniform float occlusionStrength;
out vec4 outputColor;

void main() {
	vec4 irradiance = texture(irradianceTexture, fragmentTextureCoordinate);
	vec4 albedo = texture(albedoTexture, fragmentTextureCoordinate);
	vec3 specular = texture(specularTexture, fragmentTextureCoordinate).rgb;
	float visibility = mix(1.0, texture(occlusionTexture, fragmentTextureCoordinate).r, occlusionStrength);
	vec3 color = irradiance.rgb * albedo.rgb * visibility + specular * mix(1.0, visibility, 0.6);
	outputColor = vec4(color, 1.0);
}";

// 배경. (씬 MRT 에 직접, 어두운 스튜디오에 두상 뒤가 은은하게 밝은 방사형 그라디언트. 조도 = 배경색, 알베도 1, 산란 / 노멀 없음)
const BACKGROUND_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform float aspectRatio;
layout(location = 0) out vec4 irradianceOutput;
layout(location = 1) out vec4 albedoOutput;
layout(location = 2) out vec4 specularOutput;
layout(location = 3) out vec4 normalOutput;

void main() {
	vec2 centered = (fragmentTextureCoordinate - vec2(0.5, 0.56)) * vec2(aspectRatio, 1.0);
	float radial = dot(centered, centered);
	vec3 backgroundColor = vec3(0.012, 0.012, 0.015) + vec3(0.075, 0.068, 0.07) * exp(-radial * 3.2);
	backgroundColor += vec3(0.02, 0.016, 0.012) * smoothstep(0.2, -0.6, fragmentTextureCoordinate.y - 0.5);
	irradianceOutput = vec4(backgroundColor, 0.0);
	albedoOutput = vec4(1.0, 1.0, 1.0, 1.0);
	specularOutput = vec4(0.0, 0.0, 0.0, 1.0);
	normalOutput = vec4(0.0, 0.0, 0.0, 0.0);
}";

// 톤 매핑 합성. (노출 → 블룸 가산 → ACES → 비네트 → 그레인 → sRGB)
const COMPOSITE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform float exposure;
uniform float bloomStrength;
uniform float vignetteStrength;
uniform float grainStrength;
uniform float elapsedTime;
uniform vec2 resolution;
out vec4 outputColor;

vec3 acesToneMap(vec3 color) {
	const float a = 2.51;
	const float b = 0.03;
	const float c = 2.43;
	const float d = 0.59;
	const float e = 0.14;
	return clamp((color * (a * color + b)) / (color * (c * color + d) + e), 0.0, 1.0);
}

float hash21(vec2 point) {
	point = fract(point * vec2(234.34, 435.345));
	point += dot(point, point + 34.23);
	return fract(point.x * point.y);
}

void main() {
	vec3 sceneColor = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	vec3 bloomColor = texture(bloomTexture, fragmentTextureCoordinate).rgb;
	vec3 color = sceneColor * exposure + bloomColor * bloomStrength;
	color = acesToneMap(color);

	// 비네트.
	vec2 centered = fragmentTextureCoordinate - 0.5;
	float vignette = 1.0 - smoothstep(0.35, 0.95, length(centered * vec2(1.0, 0.85)) * 1.25);
	color *= mix(1.0, vignette, vignetteStrength);

	// 필름 그레인. (중간 밝기에서 가장 보이고 어두운 곳은 절반)
	float grain = hash21(fragmentTextureCoordinate * resolution + fract(elapsedTime * 13.7) * 100.0) - 0.5;
	float grainLuminance = dot(color, vec3(0.299, 0.587, 0.114));
	color += grain * grainStrength * mix(0.5, 1.0, grainLuminance) * (1.0 - grainLuminance * 0.5);

	// sRGB 인코딩.
	vec3 encoded = mix(color * 12.92, 1.055 * pow(max(color, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055, step(0.0031308, color));
	outputColor = vec4(encoded, 1.0);
}";

// FXAA. (휘도 대비 기반 가장자리 방향 검출 + 방향 블러, LDR 출력에 적용)
const FXAA_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform vec2 texelSize;
uniform float sharpenStrength;
out vec4 outputColor;

float luminance(vec3 color) {
	return dot(color, vec3(0.299, 0.587, 0.114));
}

// 대비 적응 샤픈. (이웃 4개와의 최소 / 최대 대비가 낮은 곳만 또렷하게, 가장자리는 과하게 세우지 않는다)
vec3 sharpen(vec3 color, vec2 coordinate) {
	vec3 north = texture(sourceTexture, coordinate + vec2(0.0, -1.0) * texelSize).rgb;
	vec3 south = texture(sourceTexture, coordinate + vec2(0.0, 1.0) * texelSize).rgb;
	vec3 west = texture(sourceTexture, coordinate + vec2(-1.0, 0.0) * texelSize).rgb;
	vec3 east = texture(sourceTexture, coordinate + vec2(1.0, 0.0) * texelSize).rgb;
	vec3 minimumColor = min(color, min(min(north, south), min(west, east)));
	vec3 maximumColor = max(color, max(max(north, south), max(west, east)));
	vec3 contrast = min(minimumColor, 1.0 - maximumColor) / max(maximumColor, vec3(0.001));
	vec3 weight = sqrt(clamp(contrast, 0.0, 1.0)) * sharpenStrength;
	vec3 sharpened = (color + (north + south + west + east) * weight) / (1.0 + 4.0 * weight);
	return mix(color, color * 2.0 - sharpened, weight);
}

void main() {
	vec3 colorNorthWest = texture(sourceTexture, fragmentTextureCoordinate + vec2(-1.0, -1.0) * texelSize).rgb;
	vec3 colorNorthEast = texture(sourceTexture, fragmentTextureCoordinate + vec2(1.0, -1.0) * texelSize).rgb;
	vec3 colorSouthWest = texture(sourceTexture, fragmentTextureCoordinate + vec2(-1.0, 1.0) * texelSize).rgb;
	vec3 colorSouthEast = texture(sourceTexture, fragmentTextureCoordinate + vec2(1.0, 1.0) * texelSize).rgb;
	vec3 colorCenter = texture(sourceTexture, fragmentTextureCoordinate).rgb;
	float lumaNorthWest = luminance(colorNorthWest);
	float lumaNorthEast = luminance(colorNorthEast);
	float lumaSouthWest = luminance(colorSouthWest);
	float lumaSouthEast = luminance(colorSouthEast);
	float lumaCenter = luminance(colorCenter);
	float lumaMinimum = min(lumaCenter, min(min(lumaNorthWest, lumaNorthEast), min(lumaSouthWest, lumaSouthEast)));
	float lumaMaximum = max(lumaCenter, max(max(lumaNorthWest, lumaNorthEast), max(lumaSouthWest, lumaSouthEast)));

	vec2 direction;
	direction.x = -((lumaNorthWest + lumaNorthEast) - (lumaSouthWest + lumaSouthEast));
	direction.y = ((lumaNorthWest + lumaSouthWest) - (lumaNorthEast + lumaSouthEast));
	float directionReduce = max((lumaNorthWest + lumaNorthEast + lumaSouthWest + lumaSouthEast) * (0.25 * 0.125), 1.0 / 128.0);
	float reciprocalDirectionMinimum = 1.0 / (min(abs(direction.x), abs(direction.y)) + directionReduce);
	direction = min(vec2(8.0), max(vec2(-8.0), direction * reciprocalDirectionMinimum)) * texelSize;

	vec3 colorA = 0.5 * (texture(sourceTexture, fragmentTextureCoordinate + direction * (1.0 / 3.0 - 0.5)).rgb + texture(sourceTexture, fragmentTextureCoordinate + direction * (2.0 / 3.0 - 0.5)).rgb);
	vec3 colorB = colorA * 0.5 + 0.25 * (texture(sourceTexture, fragmentTextureCoordinate + direction * -0.5).rgb + texture(sourceTexture, fragmentTextureCoordinate + direction * 0.5).rgb);
	float lumaB = luminance(colorB);
	vec3 color = (lumaB < lumaMinimum || lumaB > lumaMaximum) ? colorA : colorB;
	if (sharpenStrength > 0.0) {
		color = sharpen(color, fragmentTextureCoordinate);
	}
	outputColor = vec4(color, 1.0);
}";

//==============================================================================
// initialize 에서 만드는 GL 리소스.
//==============================================================================
struct FaceResources {
	skin_renderer: HumanSkinRenderer,
	head_model: SkinnedModel,
	hair_model: SkinnedModel,
	shadow_map: ShadowMap,
	scene_render_target: RenderTarget,
	hdr_render_target: RenderTarget,
	ldr_render_target: RenderTarget,
	subsurface_effect: SubsurfaceScatteringEffect,
	ambient_occlusion_effect: AmbientOcclusionEffect,
	bloom_effect: BloomEffect,
	combine_pass: FullscreenPass,
	background_pass: FullscreenPass,
	composite_pass: FullscreenPass,
	antialias_pass: FullscreenPass,
	maximum_render_target_size: i32,
	camera_target: Vector3,
	head_pivot: Vector3,
	shadow_focus_position: Vector3,
	expression_table: Value,
	wrinkle_table: Value,
	expression_unit_names: Vec<String>,
	target_weights: Vec<f64>,
	current_weights: Vec<f64>,
	is_first_frame_checked: bool,
}

//==============================================================================
// 노인 얼굴 씬.
//==============================================================================
struct OldFaceScene {
	scene_base: SceneBase,
	asset_file_names: Vec<String>,
	asset_list: Vec<BlobAsset>,
	resources: Option<FaceResources>,
	load_error_message: Option<String>,
	load_seconds: f64,
	use_fixed_time_step: bool,
	elapsed_time: f64,
	active_expression_index: usize,
	is_expression_cycle_enabled: bool,
	next_blink_time: f64,
	blink_start_time: f64,
	random: SeededRandom,
	head_yaw: f64,
	head_pitch: f64,
	camera_yaw: f64,
	camera_pitch: f64,
	camera_distance: f64,
	camera_target_yaw: f64,
	camera_target_pitch: f64,
	camera_target_distance: f64,
	last_pointer_position: Option<Vector2>,
}

impl OldFaceScene {
	//==============================================================================
	// 생성. (읽을 애셋 이름 모으기, 같은 파일은 한 번만)
	//==============================================================================
	fn new() -> OldFaceScene {
		// 확인용: 환경 변수 VANILLA_OLDFACE_YAW(도)로 처음 카메라 옆각을 준다. (화면 저장으로 옆얼굴을 찍을 때)
		let start_camera_yaw = std::env::var("VANILLA_OLDFACE_YAW").ok().and_then(|text| text.parse::<f64>().ok()).map(|degree| degree.to_radians()).unwrap_or(0.0);
		let mut asset_file_names: Vec<String> = Vec::new();
		let mut add_name = |file_name: &str| {
			if !asset_file_names.iter().any(|name| name == file_name) {
				asset_file_names.push(file_name.to_string());
			}
		};
		for definition in PART_MATERIAL_DEFINITIONS.iter() {
			for file_name in [definition.albedo, definition.normal, definition.roughness, definition.specular, definition.opacity].into_iter().flatten() {
				add_name(file_name);
			}
			for (_uniform_name, _texture_unit, file_name) in definition.extra.iter() {
				add_name(file_name);
			}
		}
		for file_name in [DETAIL_HEIGHT_FILE, HEAD_MODEL_FILE, HAIR_MODEL_FILE, "expressions.json", "wrinkles.json"] {
			add_name(file_name);
		}
		let asset_list = asset_file_names.iter().map(|_| BlobAsset::new()).collect();
		OldFaceScene {
			scene_base: SceneBase::new(),
			asset_file_names,
			asset_list,
			resources: None,
			load_error_message: None,
			load_seconds: 0.0,
			use_fixed_time_step: std::env::var("VANILLA_CAPTURE_PATH").is_ok(),
			elapsed_time: 0.0,
			active_expression_index: 0,
			is_expression_cycle_enabled: true,
			next_blink_time: 2.5,
			blink_start_time: -10.0,
			random: SeededRandom::new(7),
			head_yaw: 0.0,
			head_pitch: 0.0,
			camera_yaw: start_camera_yaw,
			camera_pitch: CAMERA_DEFAULT_PITCH,
			camera_distance: CAMERA_DEFAULT_DISTANCE,
			camera_target_yaw: start_camera_yaw,
			camera_target_pitch: CAMERA_DEFAULT_PITCH,
			camera_target_distance: CAMERA_DEFAULT_DISTANCE,
			last_pointer_position: None,
		}
	}

	//==============================================================================
	// 애셋 바이트 꺼내기.
	//==============================================================================
	fn read_bytes(&self, file_name: &str) -> Result<Rc<Vec<u8>>, String> {
		for (name, blob_asset) in self.asset_file_names.iter().zip(self.asset_list.iter()) {
			if name == file_name {
				return blob_asset.get_blob().ok_or_else(|| format!("{} 읽기 실패", file_name));
			}
		}
		Err(format!("{} 는 읽을 목록에 없다", file_name))
	}

	//==============================================================================
	// 이미지 서술 만들기. (원본 loadAssetImage, flip_y 없음, 색 데이터 아님, 이방성 8)
	//==============================================================================
	fn read_image_description(&self, file_name: Option<&str>) -> Result<Option<ImageDescription>, String> {
		let file_name = match file_name {
			Some(file_name) => {
				file_name
			}
			None => {
				return Ok(None);
			}
		};
		let bytes = self.read_bytes(file_name)?;
		let mime_type = if file_name.ends_with(".png") {
			String::from("image/png")
		}
		else {
			String::from("image/jpeg")
		};
		let options = ImageDescriptionOptions { flip_y: false, is_color_data: false, anisotropy: 8.0 };
		Ok(Some(Material::create_image_description(bytes.as_ref().clone(), Some(mime_type), Some(options))))
	}

	//==============================================================================
	// GL 리소스와 모델 만들기. (원본 예제의 초기화와 애셋 로드 부분)
	//==============================================================================
	fn create_resources(&self, gl: &GlContext) -> Result<FaceResources, String> {
		let mut skin_renderer = HumanSkinRenderer::new(gl)?;
		let subsurface_effect = SubsurfaceScatteringEffect::new(gl)?;
		let ambient_occlusion_effect = AmbientOcclusionEffect::new(gl)?;
		let mut bloom_effect = BloomEffect::new(gl)?;
		let shadow_map = ShadowMap::new(gl, SHADOWMAP_RESOLUTION)?;
		let maximum_render_target_size = unsafe {
			gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE).min(gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE))
		};
		let scene_render_target = RenderTarget::new(gl, 2, 2, true, Some(RenderTargetOptions { color_attachment_count: 4, use_float_color: true, use_depth_texture: true, ..RenderTargetOptions::default() }))?;
		let hdr_render_target = RenderTarget::new(gl, 2, 2, false, Some(RenderTargetOptions { color_attachment_count: 1, use_float_color: true, use_depth_texture: false, ..RenderTargetOptions::default() }))?;
		let ldr_render_target = RenderTarget::new(gl, 2, 2, false, None)?;
		let combine_pass = FullscreenPass::new(gl, COMBINE_FRAGMENTSHADER_SOURCE)?;
		let background_pass = FullscreenPass::new(gl, BACKGROUND_FRAGMENTSHADER_SOURCE)?;
		let composite_pass = FullscreenPass::new(gl, COMPOSITE_FRAGMENTSHADER_SOURCE)?;
		let antialias_pass = FullscreenPass::new(gl, FXAA_FRAGMENTSHADER_SOURCE)?;

		// 생성 때 기본값. (원본 예제와 같다, 나머지는 프레임마다 슬라이더 기본값으로 넣는다)
		skin_renderer.set_detail_texel_size(1.0 / DETAIL_TEXTURE_SIZE);
		skin_renderer.set_shadow_sample_count(SHADOW_SAMPLE_COUNT);
		bloom_effect.set_iteration_count(3);

		// 부위별 머티리얼. (기본 슬롯 + 높이 디테일 + 추가 슬롯, 셰이딩 모드 등록)
		let detail_description = self.read_image_description(Some(DETAIL_HEIGHT_FILE))?;
		let mut detail_height_texture = None;
		let mut material_by_keyword: Vec<(&str, Rc<Material>)> = Vec::new();
		for definition in PART_MATERIAL_DEFINITIONS.iter() {
			let mut description = Material::create_default_description();
			description.base_color_image = self.read_image_description(definition.albedo)?;
			description.normal_image = self.read_image_description(definition.normal)?;
			description.metallic_roughness_image = self.read_image_description(definition.roughness)?;
			description.specular_image = self.read_image_description(definition.specular)?;
			description.opacity_image = self.read_image_description(definition.opacity)?;
			description.roughness_factor = definition.roughness_factor;
			if let Some(base_color_factor) = definition.base_color_factor {
				description.base_color_factor = base_color_factor;
			}
			let material = Rc::new(Material::create_from_description(gl, skin_renderer.get_shader_program(), &description)?);
			material.set_subsurface_factor(definition.subsurface_factor);
			if definition.use_detail {
				if detail_height_texture.is_none() {
					detail_height_texture = Some(Material::create_texture(gl, detail_description.as_ref(), [128, 128, 128, 255])?);
				}
				material.set_texture("detailHeightTexture", SKIN_TEXTURE_UNIT_DETAILHEIGHT, detail_height_texture);
			}
			for (uniform_name, texture_unit, file_name) in definition.extra.iter() {
				let extra_description = self.read_image_description(Some(file_name))?;
				let extra_texture = Material::create_texture(gl, extra_description.as_ref(), [128, 128, 255, 255])?;
				material.set_texture(uniform_name, *texture_unit, Some(extra_texture));
			}
			skin_renderer.set_material_shading(&material, definition.shading_mode);
			material_by_keyword.push((definition.keyword, material));
		}

		// 모델. (머티리얼 이름 부분 일치로 머티리얼 교체)
		let mut head_model = SkinnedModel::load_from_bytes(gl, &self.read_bytes(HEAD_MODEL_FILE)?, &skin_renderer)?;
		let mut hair_model = SkinnedModel::load_from_bytes(gl, &self.read_bytes(HAIR_MODEL_FILE)?, &skin_renderer)?;
		for model in [&mut head_model, &mut hair_model] {
			assign_materials(model, &material_by_keyword)?;
			model.update(0.0);
		}

		// 카메라 초점. (양쪽 눈알 프리미티브 정점의 평균 = 두 눈 사이)
		let mut eye_center = Vector3::create(0.0, 1.6, 0.1);
		let mut position_sum = [0.0f64; 3];
		let mut eye_vertex_count = 0usize;
		for drawable in head_model.get_drawable_list().iter() {
			if !drawable.material_name.contains("eyeball") {
				continue;
			}
			let positions = &drawable.mesh_description.positions;
			for vertex_index in 0..positions.len() / 3 {
				position_sum[0] += positions[vertex_index * 3] as f64;
				position_sum[1] += positions[vertex_index * 3 + 1] as f64;
				position_sum[2] += positions[vertex_index * 3 + 2] as f64;
			}
			eye_vertex_count += positions.len() / 3;
		}
		if eye_vertex_count > 0 {
			let count = eye_vertex_count as f64;
			eye_center = Vector3::create((position_sum[0] / count) as f64, (position_sum[1] / count) as f64, (position_sum[2] / count) as f64);
		}
		let camera_target = Vector3::create(eye_center.x, eye_center.y - 0.045, eye_center.z - 0.08);
		let head_pivot = Vector3::create(eye_center.x, eye_center.y - 0.22, eye_center.z - 0.06);
		let shadow_focus_position = Vector3::create(eye_center.x, eye_center.y - 0.08, eye_center.z - 0.09);

		// 표정 / 주름 정의.
		let expression_table: Value = serde_json::from_slice(&self.read_bytes("expressions.json")?).map_err(|error| format!("expressions.json: {}", error))?;
		let wrinkle_table: Value = serde_json::from_slice(&self.read_bytes("wrinkles.json")?).map_err(|error| format!("wrinkles.json: {}", error))?;
		let mut expression_unit_names: Vec<String> = Vec::new();
		if let Some(preset_table) = expression_table.as_object() {
			for preset in preset_table.values() {
				if let Some(unit_table) = preset.as_object() {
					for unit_name in unit_table.keys() {
						if !expression_unit_names.iter().any(|name| name == unit_name) {
							expression_unit_names.push(unit_name.clone());
						}
					}
				}
			}
		}
		let unit_count = expression_unit_names.len();
		let mut resources = FaceResources {
			skin_renderer,
			head_model,
			hair_model,
			shadow_map,
			scene_render_target,
			hdr_render_target,
			ldr_render_target,
			subsurface_effect,
			ambient_occlusion_effect,
			bloom_effect,
			combine_pass,
			background_pass,
			composite_pass,
			antialias_pass,
			maximum_render_target_size,
			camera_target,
			head_pivot,
			shadow_focus_position,
			expression_table,
			wrinkle_table,
			expression_unit_names,
			target_weights: vec![0.0; unit_count],
			current_weights: vec![0.0; unit_count],
			is_first_frame_checked: false,
		};
		select_expression(&mut resources, EXPRESSION_DEFINITIONS[0].0);
		Ok(resources)
	}

	//==============================================================================
	// 표정 갱신. (원본 updateExpressions, 목표 가중치로 지수 접근 + 깜빡임)
	//==============================================================================
	fn update_expressions(&mut self, delta_time: f64) {
		let elapsed_time = self.elapsed_time;
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		let approach = (delta_time * 6.0).min(1.0);
		for unit_index in 0..resources.expression_unit_names.len() {
			let current_weight = resources.current_weights[unit_index];
			let next_weight = current_weight + (resources.target_weights[unit_index] - current_weight) * approach;
			resources.current_weights[unit_index] = next_weight;
			let unit_name = &resources.expression_unit_names[unit_index];
			resources.head_model.set_morph_weight(unit_name, next_weight);
			resources.hair_model.set_morph_weight(unit_name, next_weight);
		}

		// 깜빡임. (눈 감기 단위, 표정이 감은 만큼보다 크게 덮어쓴다, 간격은 원본 BLINK RATE 기본 1)
		if elapsed_time >= self.next_blink_time {
			self.blink_start_time = elapsed_time;
			self.next_blink_time = elapsed_time + 2.8 + self.random.next_value() as f64 * 3.6;
		}
		let blink_local = elapsed_time - self.blink_start_time;
		let blink = if (0.0..0.3).contains(&blink_local) {
			(blink_local / 0.3 * std::f64::consts::PI).sin()
		}
		else {
			0.0
		};
		let left_closed = read_unit_weight(resources, "EyeBlinkLeft").max(blink);
		let right_closed = read_unit_weight(resources, "EyeBlinkRight").max(blink);
		for model in [&mut resources.head_model, &mut resources.hair_model] {
			model.set_morph_weight("EyeBlinkLeft", left_closed);
			model.set_morph_weight("EyeBlinkRight", right_closed);
		}
		resources.skin_renderer.set_eye_closure(left_closed.max(right_closed));
	}

	//==============================================================================
	// 입력. (숫자 키 표정, 끌기 궤도 회전, 휠 줌, 원본 예제의 감도)
	//==============================================================================
	fn handle_input(&mut self, engine: &Engine) {
		let input_manager = engine.get_input_manager();
		let mut selected_index = None;
		for (key_index, key) in DIGIT_KEYS.iter().enumerate() {
			if input_manager.is_key_just_pressed(key) {
				selected_index = Some(key_index);
			}
		}
		if input_manager.is_mouse_button_down(MouseButton::Left) {
			let pointer_position = input_manager.get_canvas_native_input_position();
			if let Some(last_pointer_position) = self.last_pointer_position {
				let delta_x = pointer_position.x - last_pointer_position.x;
				let delta_y = pointer_position.y - last_pointer_position.y;
				self.camera_target_yaw = (self.camera_target_yaw - delta_x * 0.006).clamp(-1.25, 1.25);
				self.camera_target_pitch = (self.camera_target_pitch + delta_y * 0.004).clamp(-0.4, 0.55);
			}
			self.last_pointer_position = Some(pointer_position);
		}
		else {
			self.last_pointer_position = None;
		}
		let wheel_delta_y = input_manager.get_wheel_delta_y();
		if wheel_delta_y != 0.0 {
			self.camera_target_distance = (self.camera_target_distance * (1.0 + wheel_delta_y * 0.0012)).clamp(CAMERA_MINIMUM_DISTANCE, CAMERA_MAXIMUM_DISTANCE);
		}
		if let Some(selected_index) = selected_index {
			self.is_expression_cycle_enabled = false;
			self.change_expression(selected_index);
		}
		else if self.is_expression_cycle_enabled {
			let cycle_index = ((self.elapsed_time / EXPRESSION_CYCLE_SECONDS) as usize) % EXPRESSION_DEFINITIONS.len();
			if cycle_index != self.active_expression_index {
				self.change_expression(cycle_index);
			}
		}
	}

	//==============================================================================
	// 표정 바꾸기.
	//==============================================================================
	fn change_expression(&mut self, expression_index: usize) {
		self.active_expression_index = expression_index;
		if let Some(resources) = &mut self.resources {
			select_expression(resources, EXPRESSION_DEFINITIONS[expression_index].0);
		}
	}

	//==============================================================================
	// 렌더러 파라미터. (원본 renderFrame 이 슬라이더 기본값으로 프레임마다 넣는 값)
	//==============================================================================
	fn apply_render_parameters(resources: &mut FaceResources, elapsed_time: f64) {
		let skin_renderer = &mut resources.skin_renderer;
		skin_renderer.set_detail_strength(3.5);
		skin_renderer.set_cavity_strength(2.5);
		skin_renderer.set_micro_detail(38.0, 0.55);
		skin_renderer.set_normal_strength(1.5);
		skin_renderer.set_wrinkle_maps(true, 1.0, 0.45);
		skin_renderer.set_specular_strength(1.25);
		skin_renderer.set_roughness_range(0.9 * 0.85, 0.9 * 1.1);
		skin_renderer.set_sheen_strength(0.12);
		skin_renderer.set_shadow_softness(3.0);
		let key_light_direction = Vector3::create(KEY_LIGHT_DIRECTION[0], KEY_LIGHT_DIRECTION[1], KEY_LIGHT_DIRECTION[2]).normalize();
		let fill_light_direction = Vector3::create(FILL_LIGHT_DIRECTION[0], FILL_LIGHT_DIRECTION[1], FILL_LIGHT_DIRECTION[2]).normalize();
		let rim_light_direction = Vector3::create(RIM_LIGHT_DIRECTION[0], RIM_LIGHT_DIRECTION[1], RIM_LIGHT_DIRECTION[2]).normalize();
		skin_renderer.set_light(0, key_light_direction.x, key_light_direction.y, key_light_direction.z, 1.0, 0.92, 0.82, 2.6);
		skin_renderer.set_light(1, fill_light_direction.x, fill_light_direction.y, fill_light_direction.z, 0.6, 0.7, 0.9, 1.1);
		skin_renderer.set_light(2, rim_light_direction.x, rim_light_direction.y, rim_light_direction.z, 0.95, 0.97, 1.0, 1.4);
		skin_renderer.set_ambient_colors([0.2, 0.21, 0.25], [0.13, 0.12, 0.12], [0.09, 0.08, 0.075]);
		skin_renderer.set_hair_fringe(true);
		skin_renderer.set_hair_sway(0.25 * HAIR_SWAY_SCALE * 0.012, elapsed_time, 1.6);
		skin_renderer.set_hair_parameters([-0.1, 0.16], [160.0, 24.0], [0.24 * 0.5, 0.52 * 0.5, 0.2], 0.0);
		skin_renderer.set_eye_parameters(0.14, 0.042, 0.05);
		resources.subsurface_effect.set_scatter_width(8.0 * 0.001);
		resources.ambient_occlusion_effect.set_parameters(0.028, 1.15, 0.0015);
		resources.bloom_effect.set_threshold(1.05, 1.05 + 0.85);
	}

	//==============================================================================
	// 3D 그리기. (원본 renderFrame 의 1 ~ 7 단계, 2D 상태는 호출자가 되돌린다)
	//==============================================================================
	fn render_scene(&mut self, gl: &GlContext, canvas_width: i32, canvas_height: i32) -> Result<(), String> {
		let elapsed_time = self.elapsed_time;
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return Ok(());
			}
		};

		// 내부 렌더 해상도. (RENDER BASE 의 픽셀 수를 캔버스 종횡비로, GPU 상한으로 자른다)
		let render_pixel_budget = RENDER_BASE_HEIGHT * (16.0 / 9.0) * RENDER_BASE_HEIGHT;
		let pixel_budget_scale = (render_pixel_budget / ((canvas_width * canvas_height).max(1) as f64)).sqrt();
		let longest_display_side = canvas_width.max(canvas_height).max(1) as f64;
		let size_limit_scale = (resources.maximum_render_target_size as f64 / (longest_display_side * pixel_budget_scale)).min(1.0);
		let render_width = ((canvas_width as f64 * pixel_budget_scale * size_limit_scale).floor() as i32).max(2);
		let render_height = ((canvas_height as f64 * pixel_budget_scale * size_limit_scale).floor() as i32).max(2);
		if resources.scene_render_target.get_width() != render_width || resources.scene_render_target.get_height() != render_height {
			resources.scene_render_target.resize(gl, render_width, render_height)?;
			resources.hdr_render_target.resize(gl, render_width, render_height)?;
			resources.ldr_render_target.resize(gl, render_width, render_height)?;
			resources.subsurface_effect.resize(gl, render_width, render_height)?;
			resources.ambient_occlusion_effect.resize(gl, render_width, render_height)?;
			resources.bloom_effect.resize(gl, render_width, render_height)?;
		}
		let aspect_ratio = canvas_width as f64 / canvas_height.max(1) as f64;

		// 카메라. (미세한 자동 흔들림, 세로 화면은 거리를 늘린다)
		let sway_yaw = self.camera_yaw + (elapsed_time * 0.19).sin() * 0.02;
		let sway_pitch = self.camera_pitch + (elapsed_time * 0.27 + 1.0).sin() * 0.01;
		let portrait_factor = if aspect_ratio < 1.2 {
			(1.2 / aspect_ratio).powf(0.55)
		}
		else {
			1.0
		};
		let camera_distance = self.camera_distance * portrait_factor;
		let camera_target = resources.camera_target;
		let camera_x = camera_target.x + sway_yaw.sin() * sway_pitch.cos() * camera_distance;
		let camera_y = camera_target.y + sway_pitch.sin() * camera_distance;
		let camera_z = camera_target.z + sway_yaw.cos() * sway_pitch.cos() * camera_distance;
		let camera_position = Vector3::create(camera_x, camera_y, camera_z);
		let view_matrix = Matrix4::create_look_at(camera_position, camera_target, Vector3::up());
		let mut view_projection_matrix = Matrix4::create_perspective(CAMERA_FIELD_OF_VIEW, aspect_ratio, CAMERA_NEAR_DISTANCE, CAMERA_FAR_DISTANCE);
		view_projection_matrix.multiply(&view_matrix);
		let view_projection_elements = *view_projection_matrix.get_elements();

		// 모델 행렬. (머리 회전 중심 기준 Y, X 회전)
		let head_pivot = resources.head_pivot;
		let mut model_matrix = Matrix4::create_translation(head_pivot.x, head_pivot.y, head_pivot.z);
		model_matrix.multiply(&Matrix4::create_rotation_y(self.head_yaw * 0.8));
		model_matrix.multiply(&Matrix4::create_rotation_x(self.head_pitch * 0.7));
		model_matrix.multiply(&Matrix4::create_translation(-head_pivot.x, -head_pivot.y, -head_pivot.z));
		let model_elements = *model_matrix.get_elements();

		// 1. 섀도우 깊이 패스. (키 라이트)
		unsafe {
			gl.enable(glow::CULL_FACE);
			gl.cull_face(glow::BACK);
			gl.disable(glow::BLEND);
		}
		let key_light_direction = Vector3::create(KEY_LIGHT_DIRECTION[0], KEY_LIGHT_DIRECTION[1], KEY_LIGHT_DIRECTION[2]).normalize();
		resources.shadow_map.update_light_matrix(resources.shadow_focus_position, key_light_direction.multiply_scalar(-1.0), 0.45, 0.2, 4.0);
		let light_view_projection_elements = *resources.shadow_map.get_light_view_projection_matrix().get_elements();
		resources.shadow_map.begin_render(gl);
		for model in [&mut resources.head_model, &mut resources.hair_model] {
			resources.skin_renderer.draw_depth(gl, model, &light_view_projection_elements, &model_elements);
		}
		let shadow_depth_texture = resources.shadow_map.get_depth_texture();
		resources.skin_renderer.set_shadow_map(Some(shadow_depth_texture), Some(&light_view_projection_elements), 1.0 / SHADOWMAP_RESOLUTION as f64);
		resources.skin_renderer.set_shadow_strength(0.85);

		// 2. 피부 MRT 패스. (배경 → 조도 / 알베도 / 스펙큘러 / 노멀 + 깊이)
		resources.scene_render_target.bind(gl);
		unsafe {
			gl.enable(glow::DEPTH_TEST);
			gl.depth_mask(true);
			gl.clear_color(0.0, 0.0, 0.0, 0.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
			gl.disable(glow::DEPTH_TEST);
		}
		resources.background_pass.use_program(gl);
		unsafe {
			gl.uniform_1_f32(resources.background_pass.get_uniform_location(gl, "aspectRatio").as_ref(), aspect_ratio as f32);
		}
		resources.background_pass.draw(gl);
		unsafe {
			gl.enable(glow::DEPTH_TEST);
		}
		for model in [&mut resources.head_model, &mut resources.hair_model] {
			resources.skin_renderer.draw(gl, model, &view_projection_elements, &model_elements, camera_x, camera_y, camera_z);
		}

		// 2b. 블렌드 패스. (머리카락 프린지 / 눈꺼풀 차폐 / 눈물선 / 침, 깊이 쓰기 없이 어태치먼트별 알파 블렌드)
		unsafe {
			gl.enable(glow::BLEND);
			gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
			gl.depth_mask(false);
			gl.disable(glow::CULL_FACE);
		}
		for model in [&mut resources.head_model, &mut resources.hair_model] {
			resources.skin_renderer.draw_blended(gl, model, &view_projection_elements, &model_elements, camera_x, camera_y, camera_z);
		}
		unsafe {
			gl.enable(glow::CULL_FACE);
			gl.depth_mask(true);
			gl.disable(glow::BLEND);
		}

		// 3. 화면 공간 앰비언트 오클루전 / 서브서피스 스캐터링.
		let irradiance_texture = resources.scene_render_target.get_color_texture(0);
		let albedo_texture = resources.scene_render_target.get_color_texture(1);
		let specular_texture = resources.scene_render_target.get_color_texture(2);
		let normal_texture = resources.scene_render_target.get_color_texture(3);
		let scene_depth_texture = resources.scene_render_target.get_depth_texture();
		resources.ambient_occlusion_effect.render(gl, scene_depth_texture, normal_texture, view_matrix.get_elements(), CAMERA_FIELD_OF_VIEW, aspect_ratio, CAMERA_NEAR_DISTANCE, CAMERA_FAR_DISTANCE);
		let occlusion_result_texture = resources.ambient_occlusion_effect.get_result_texture();
		resources.subsurface_effect.render(gl, irradiance_texture, scene_depth_texture, CAMERA_FIELD_OF_VIEW, aspect_ratio, CAMERA_NEAR_DISTANCE, CAMERA_FAR_DISTANCE);
		let scattered_irradiance_texture = resources.subsurface_effect.get_result_texture();

		// 4. 합성 → HDR. (조도 x 알베도 + 스펙큘러 + 배경)
		resources.hdr_render_target.bind(gl);
		let combine_pass = &resources.combine_pass;
		unsafe {
			gl.disable(glow::DEPTH_TEST);
		}
		combine_pass.use_program(gl);
		unsafe {
			gl.uniform_1_i32(combine_pass.get_uniform_location(gl, "irradianceTexture").as_ref(), 0);
			gl.uniform_1_i32(combine_pass.get_uniform_location(gl, "albedoTexture").as_ref(), 1);
			gl.uniform_1_i32(combine_pass.get_uniform_location(gl, "specularTexture").as_ref(), 2);
			gl.uniform_1_i32(combine_pass.get_uniform_location(gl, "occlusionTexture").as_ref(), 3);
			gl.uniform_1_f32(combine_pass.get_uniform_location(gl, "aspectRatio").as_ref(), aspect_ratio as f32);
			gl.uniform_1_f32(combine_pass.get_uniform_location(gl, "occlusionStrength").as_ref(), 1.15f32.min(1.0));
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, scattered_irradiance_texture);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, albedo_texture);
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, specular_texture);
			gl.active_texture(glow::TEXTURE3);
			gl.bind_texture(glow::TEXTURE_2D, occlusion_result_texture);
			gl.active_texture(glow::TEXTURE0);
		}
		combine_pass.draw(gl);
		unsafe {
			for texture_unit in 1..4 {
				gl.active_texture(glow::TEXTURE0 + texture_unit);
				gl.bind_texture(glow::TEXTURE_2D, None);
			}
			gl.active_texture(glow::TEXTURE0);
		}

		// 5. 블룸.
		let hdr_color_texture = resources.hdr_render_target.get_color_texture(0);
		resources.bloom_effect.render(gl, hdr_color_texture);

		// 6. 톤 매핑 합성 → LDR.
		resources.ldr_render_target.bind(gl);
		let composite_pass = &resources.composite_pass;
		composite_pass.use_program(gl);
		unsafe {
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "exposure").as_ref(), 0.95);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), 0.12);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "vignetteStrength").as_ref(), 0.5);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "grainStrength").as_ref(), 0.0);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
			gl.uniform_2_f32(composite_pass.get_uniform_location(gl, "resolution").as_ref(), render_width as f32, render_height as f32);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, hdr_color_texture);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, resources.bloom_effect.get_result_texture());
			gl.active_texture(glow::TEXTURE0);
		}
		composite_pass.draw(gl);

		// 7. FXAA → 화면.
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.viewport(0, 0, canvas_width, canvas_height);
		}
		let antialias_pass = &resources.antialias_pass;
		antialias_pass.use_program(gl);
		unsafe {
			gl.uniform_1_i32(antialias_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
			gl.uniform_2_f32(antialias_pass.get_uniform_location(gl, "texelSize").as_ref(), 1.0 / render_width as f32, 1.0 / render_height as f32);
			gl.uniform_1_f32(antialias_pass.get_uniform_location(gl, "sharpenStrength").as_ref(), 0.0);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, resources.ldr_render_target.get_color_texture(0));
		}
		antialias_pass.draw(gl);
		unsafe {
			gl.disable(glow::CULL_FACE);
		}

		// 첫 프레임에 GL 오류를 한 번 본다.
		if !resources.is_first_frame_checked {
			resources.is_first_frame_checked = true;
			let error_code = unsafe {
				gl.get_error()
			};
			println!("oldface: 첫 3D 프레임 GL 오류 코드 0x{:X}, 렌더 {}x{}", error_code, render_width, render_height);
		}
		Ok(())
	}
}

impl Scene for OldFaceScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (애셋 바이트 요청, 데스크탑은 그 자리에서 읽힌다)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		for (file_name, blob_asset) in self.asset_file_names.iter().zip(self.asset_list.iter_mut()) {
			blob_asset.load(engine, file_name);
		}
		for blob_asset in self.asset_list.iter() {
			self.scene_base.track_asset(blob_asset);
		}
	}

	//==============================================================================
	// 초기화. (맡긴 애셋을 다 받은 뒤, 원본 예제의 초기화처럼 GL 리소스와 모델을 엔진의 그래픽으로 만든다)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		let graphic = match engine.get_graphic() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let load_start_instant = web_time::Instant::now();
		match self.create_resources(graphic.get_gl()) {
			Ok(resources) => {
				self.load_seconds = load_start_instant.elapsed().as_secs_f64();
				println!("oldface: 준비 {:.2}초, 머리 드로어블 {}, 머리카락 드로어블 {}", self.load_seconds, resources.head_model.get_drawable_list().len(), resources.hair_model.get_drawable_list().len());
				self.resources = Some(resources);
			}
			Err(error) => {
				eprintln!("oldface: {}", error);
				self.load_error_message = Some(error);
			}
		}
	}

	//==============================================================================
	// 갱신. (원본 renderFrame 의 카메라, 표정, 머리 방향, 주름, 파라미터, 스키닝 갱신)
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		if self.resources.is_none() {
			return;
		}
		let delta_time = if self.use_fixed_time_step {
			FIXED_TIME_STEP
		}
		else {
			time_delta.min(0.1)
		};
		self.elapsed_time += delta_time;
		let elapsed_time = self.elapsed_time;
		self.handle_input(engine);

		// 카메라. (목표값 지수 접근)
		let camera_approach = (delta_time * 5.0).min(1.0);
		self.camera_yaw += (self.camera_target_yaw - self.camera_yaw) * camera_approach;
		self.camera_pitch += (self.camera_target_pitch - self.camera_pitch) * camera_approach;
		self.camera_distance += (self.camera_target_distance - self.camera_distance) * camera_approach;

		// 표정 / 머리 방향. (머리는 느린 배회, 원본 HEAD FOLLOW POINTER 기본 0)
		self.update_expressions(delta_time);
		let wander_yaw = 0.035 * (elapsed_time * 0.23).sin() + 0.02 * (elapsed_time * 0.61 + 1.3).sin();
		let wander_pitch = 0.015 * (elapsed_time * 0.31 + 0.6).sin();
		let head_approach = (delta_time * 2.2).min(1.0);
		self.head_yaw += (wander_yaw.clamp(-0.6, 0.6) - self.head_yaw) * head_approach;
		self.head_pitch += (wander_pitch.clamp(-0.35, 0.35) - self.head_pitch) * head_approach;

		if let Some(resources) = &mut self.resources {
			update_wrinkle_weights(resources);
			OldFaceScene::apply_render_parameters(resources, elapsed_time);
			resources.head_model.update(delta_time);
			resources.hair_model.update(delta_time);
		}
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 3D 패스 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		graphic.flush_batch();
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		if let Err(error) = self.render_scene(graphic.get_gl(), canvas_pixel_size.x as i32, canvas_pixel_size.y as i32) {
			eprintln!("oldface: {}", error);
		}
		graphic.restore_render_state();
	}

	//==============================================================================
	// 2D 글 출력. (표정 이름과 조작 안내)
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		engine.get_view_manager().apply_canvas_native_rect(graphic);
		let status_line = match &self.resources {
			Some(_) => {
				format!("{}  (1-9 EXPRESSION, DRAG ORBIT, WHEEL ZOOM)  t {:.1}s", EXPRESSION_DEFINITIONS[self.active_expression_index].1, self.elapsed_time)
			}
			None => {
				self.load_error_message.clone().unwrap_or_else(|| String::from("LOADING"))
			}
		};
		let canvas_native_size = engine.get_view_manager().get_canvas_native_size();
		graphic.set_fill_color(Color::create(0.0, 0.0, 0.0, 0.45));
		graphic.draw_rect(Rect::create(12.0, canvas_native_size.y - 40.0, 520.0, 28.0));
		graphic.set_font_string("14px default");
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_fill_color(Color::create(0.95, 0.92, 0.86, 0.85));
		graphic.draw_fill_text(&status_line, 22.0, canvas_native_size.y - 26.0);
		// MetaHuman 고지.
		graphic.set_font_string("11px default");
		graphic.set_fill_color(Color::create(0.95, 0.92, 0.86, 0.6));
		graphic.draw_fill_text(ATTRIBUTION_TEXT, 14.0, canvas_native_size.y - 52.0);
	}
}

//==============================================================================
// 머티리얼 교체. (원본 assignMaterials, 드로어블 머티리얼 이름에 키워드가 들어 있는 첫 정의)
//==============================================================================
fn assign_materials(model: &mut SkinnedModel, material_by_keyword: &[(&str, Rc<Material>)]) -> Result<(), String> {
	let material_name_list: Vec<String> = model.get_drawable_list().iter().map(|drawable| drawable.material_name.clone()).collect();
	for (drawable_index, material_name) in material_name_list.iter().enumerate() {
		for (keyword, material) in material_by_keyword.iter() {
			if material_name.contains(keyword) {
				model.set_material(drawable_index, material.clone())?;
				break;
			}
		}
	}
	Ok(())
}

//==============================================================================
// 표정 고르기. (원본 selectExpression, 프리셋에 없는 단위는 0)
//==============================================================================
fn select_expression(resources: &mut FaceResources, preset_name: &str) {
	let preset = resources.expression_table.get(preset_name).cloned().unwrap_or(Value::Null);
	for (unit_index, unit_name) in resources.expression_unit_names.iter().enumerate() {
		resources.target_weights[unit_index] = preset.get(unit_name).and_then(Value::as_f64).unwrap_or(0.0) as f64;
	}
}

//==============================================================================
// 표정 단위 현재 가중치 읽기. (없으면 0)
//==============================================================================
fn read_unit_weight(resources: &FaceResources, unit_name: &str) -> f64 {
	match resources.expression_unit_names.iter().position(|name| name == unit_name) {
		Some(unit_index) => {
			resources.current_weights[unit_index]
		}
		None => {
			0.0
		}
	}
}

//==============================================================================
// 주름 가중치 갱신. (원본 updateWrinkleWeights, 셰이프 → 영역 값 합 → 마스크 채널마다 주름 맵 1 / 2 / 3 기여)
//==============================================================================
fn update_wrinkle_weights(resources: &mut FaceResources) {
	let mut region_values: Vec<(String, f64)> = Vec::new();
	let shape_table = resources.wrinkle_table.get("shapes");
	for (unit_index, unit_name) in resources.expression_unit_names.iter().enumerate() {
		let weight = resources.current_weights[unit_index] as f64;
		let region_table = match shape_table.and_then(|table| table.get(unit_name)).and_then(Value::as_object) {
			Some(region_table) => {
				region_table
			}
			None => {
				continue;
			}
		};
		if weight < 0.001 {
			continue;
		}
		for (region_name, region_weight) in region_table.iter() {
			let contribution = region_weight.as_f64().unwrap_or(0.0) * weight;
			match region_values.iter_mut().find(|entry| entry.0 == *region_name) {
				Some(entry) => {
					entry.1 += contribution;
				}
				None => {
					region_values.push((region_name.clone(), contribution));
				}
			}
		}
	}
	if let Some(channel_list) = resources.wrinkle_table.get("channels").and_then(Value::as_array) {
		for (channel_index, channel) in channel_list.iter().enumerate() {
			let region_name = channel.get("region").and_then(Value::as_str).unwrap_or("");
			let region_value = region_values.iter().find(|entry| entry.0 == region_name).map(|entry| entry.1.min(1.0)).unwrap_or(0.0) as f64;
			let read_map = |map_index: usize| channel.get("maps").and_then(|maps| maps.get(map_index)).and_then(Value::as_f64).unwrap_or(0.0) as f64;
			resources.skin_renderer.set_wrinkle_channel_weight(channel_index, region_value * read_map(0), region_value * read_map(1), region_value * read_map(2));
		}
	}
	resources.skin_renderer.finish_wrinkle_channel_weights();
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("바닐라엔진 graphics3d oldface");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.window_size = Vector2::create(1280.0, 720.0);
	engine_configuration.maximum_frame_per_second = FRAME_RATE;
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(OldFaceScene::new())) {
		eprintln!("{}", error);
	}
}
