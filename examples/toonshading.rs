//==============================================================================
// 툰 셰이딩 예제. (원본 examples/toonshading 과 같은 장면: VRM 1.0 캐릭터를 셀 셰이딩으로 그린다)
// - VRM 1.0 한 파일(VRM1_Constraint_Twist_Sample.vrm)을 엔진 SkinnedModel 이 읽고, 확장(MToon, 휴머노이드, 표정)은 이 예제가 JSON 에서 직접 읽는다.
// - 셀 셰이딩 프래그먼트(램프, 얼굴 구면 노멀, 앞머리 그림자, 천사 고리, 계단 스펙큘러, 앞머리 너머 비침), 뒤집은 껍질 바깥선, 스튜디오 바닥, 방사형 배경은 원본 셰이더 글 그대로다.
// - 씬 MRT(색 + 뷰 노멀 / 종류) → 블룸 → 림 라이트 + 노출 + ACES + 비네트 합성 → FXAA 순서도 원본과 같다.
// - 표정 여섯 단추(neutral, happy, relaxed, angry, sad, surprised)와 깜빡임, 동작 여덟 단추(Quaternius Universal Animation Library 클립을 T 포즈 대비 월드 회전 변화로 VRM 뼈에 옮김).
// - 끌면 돌려 보고(궤도), 휠은 줌, 오른쪽 끌기는 초점 높이.
// - 자산은 원본 저장소의 것을 읽는다. (../vanilla.js/examples/toonshading/assets, 읽기만 한다)
//   캐릭터: VRM1 Constraint Twist Sample (c) 2022 pixiv Inc. (VRM Public License 1.0), 동작: Quaternius Universal Animation Library 1, 2 (CC0)
// - 실행: cargo run --release --example toonshading  (저장소 루트에서)
//   화면 저장: VANILLA_CAPTURE_PATH=shots/toonshading_{frame}.png VANILLA_CAPTURE_FRAMES=240 VANILLA_CAPTURE_EXIT=1
//==============================================================================
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use vanilla::{
	BloomEffect, Color, Engine, EngineConfiguration, FullscreenPass, GlContext, Graphic, Material, Matrix4, Quaternion, Rect, RenderTarget, RenderTargetOptions, Scene, SceneBase, ShaderProgram, ShadowMap, SkinnedModel, SkinnedModelRenderer,
	TextAlign, TextBaseline, Vector2, Vector3, MORPH_GLSL,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 원본 예제 자산 폴더. (vanilla.js 저장소를 이 저장소 옆에 받아 둔 것으로 본다. 읽기만 한다)
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/toonshading/assets";
const MODEL_FILE_NAME: &str = "VRM1_Constraint_Twist_Sample.vrm";
const CAMERA_FIELD_OF_VIEW: f64 = 0.5;
const CAMERA_NEAR_DISTANCE: f64 = 0.05;
const CAMERA_FAR_DISTANCE: f64 = 40.0;
const CAMERA_MINIMUM_DISTANCE: f64 = 0.4;
const CAMERA_MAXIMUM_DISTANCE: f64 = 7.0;
const SHADOWMAP_RESOLUTION: i32 = 2048;
// 내부 렌더 기준 해상도. (세로 1080 픽셀 기준 픽셀 수, 원본 RENDER BASE 1080P)
const RENDER_BASE_HEIGHT: f64 = 1080.0;
const BODY_FOCUS_HEIGHT: f64 = 0.95;
const FACE_FOCUS_DISTANCE: f64 = 0.72;
const BODY_FOCUS_DISTANCE: f64 = 4.0;
const FOCUS_HEIGHT_MINIMUM: f64 = 0.15;
const FOCUS_HEIGHT_MAXIMUM: f64 = 1.75;
const TEXTURE_UNIT_OUTLINE_MASK: u32 = 9;
const TEXTURE_UNIT_HAIR_DEPTH: u32 = 11;
const TEXTURE_UNIT_HAIR_MASK: u32 = 12;
const MATERIAL_TYPE_SKIN: i32 = 1;
const MATERIAL_TYPE_FACE: i32 = 2;
const MATERIAL_TYPE_HAIR: i32 = 3;
const MATERIAL_TYPE_CLOTH: i32 = 4;
const MATERIAL_TYPE_EYE_IRIS: i32 = 5;
const MATERIAL_TYPE_EYE_WHITE: i32 = 6;
const MATERIAL_TYPE_EYE_HIGHLIGHT: i32 = 7;
const MATERIAL_TYPE_EYELINE: i32 = 8;
const MATERIAL_TYPE_BROW: i32 = 9;
const MATERIAL_TYPE_MOUTH: i32 = 10;
// VRM 머티리얼 이름 부분 일치 → 종류. (앞에서부터 먼저 맞는 것)
const MATERIAL_TYPE_DEFINITIONS: [(&str, i32); 10] = [
	("EyeIris", MATERIAL_TYPE_EYE_IRIS),
	("EyeWhite", MATERIAL_TYPE_EYE_WHITE),
	("EyeHighlight", MATERIAL_TYPE_EYE_HIGHLIGHT),
	("FaceEyeline", MATERIAL_TYPE_EYELINE),
	("FaceBrow", MATERIAL_TYPE_BROW),
	("FaceMouth", MATERIAL_TYPE_MOUTH),
	("Face", MATERIAL_TYPE_FACE),
	("Body", MATERIAL_TYPE_SKIN),
	("Hair", MATERIAL_TYPE_HAIR),
	("_CLOTH", MATERIAL_TYPE_CLOTH),
];
// 표정 프리셋. (VRM 1.0 preset 이름, 단추 글자)
const EXPRESSION_DEFINITIONS: [(&str, &str); 6] = [("neutral", "NEUTRAL"), ("happy", "HAPPY"), ("relaxed", "RELAXED"), ("angry", "ANGRY"), ("sad", "SAD"), ("surprised", "SURPRISED")];
// 동작. (이름, 단추 글자, 파일)
const ANIMATION_DEFINITIONS: [(&str, &str, &str); 8] = [
	("Idle", "IDLE", "anims/idle.fbx"),
	("Walk", "WALK", "anims/walk.fbx"),
	("Run", "RUN", "anims/run.fbx"),
	("Roll", "ROLL", "anims/roll.fbx"),
	("Yes", "YES", "anims/yes.fbx"),
	("Talk", "TALK", "anims/talk.fbx"),
	("Punch", "PUNCH", "anims/punch.fbx"),
	("Dance", "DANCE", "anims/dance.fbx"),
];
const REST_POSE_FILE: &str = "anims/tpose.fbx";
// 원본 뼈(언리얼 마네킹 이름) → VRM 휴머노이드 뼈. (부모가 먼저 오도록 나열)
const SOURCE_BONE_MAP: [(&str, &str); 52] = [
	("pelvis", "hips"), ("spine_01", "spine"), ("spine_02", "chest"), ("spine_03", "upperChest"), ("neck_01", "neck"), ("Head", "head"),
	("clavicle_l", "leftShoulder"), ("upperarm_l", "leftUpperArm"), ("lowerarm_l", "leftLowerArm"), ("hand_l", "leftHand"),
	("thumb_01_l", "leftThumbMetacarpal"), ("thumb_02_l", "leftThumbProximal"), ("thumb_03_l", "leftThumbDistal"),
	("index_01_l", "leftIndexProximal"), ("index_02_l", "leftIndexIntermediate"), ("index_03_l", "leftIndexDistal"),
	("middle_01_l", "leftMiddleProximal"), ("middle_02_l", "leftMiddleIntermediate"), ("middle_03_l", "leftMiddleDistal"),
	("ring_01_l", "leftRingProximal"), ("ring_02_l", "leftRingIntermediate"), ("ring_03_l", "leftRingDistal"),
	("pinky_01_l", "leftLittleProximal"), ("pinky_02_l", "leftLittleIntermediate"), ("pinky_03_l", "leftLittleDistal"),
	("clavicle_r", "rightShoulder"), ("upperarm_r", "rightUpperArm"), ("lowerarm_r", "rightLowerArm"), ("hand_r", "rightHand"),
	("thumb_01_r", "rightThumbMetacarpal"), ("thumb_02_r", "rightThumbProximal"), ("thumb_03_r", "rightThumbDistal"),
	("index_01_r", "rightIndexProximal"), ("index_02_r", "rightIndexIntermediate"), ("index_03_r", "rightIndexDistal"),
	("middle_01_r", "rightMiddleProximal"), ("middle_02_r", "rightMiddleIntermediate"), ("middle_03_r", "rightMiddleDistal"),
	("ring_01_r", "rightRingProximal"), ("ring_02_r", "rightRingIntermediate"), ("ring_03_r", "rightRingDistal"),
	("pinky_01_r", "rightLittleProximal"), ("pinky_02_r", "rightLittleIntermediate"), ("pinky_03_r", "rightLittleDistal"),
	("thigh_l", "leftUpperLeg"), ("calf_l", "leftLowerLeg"), ("foot_l", "leftFoot"), ("ball_l", "leftToes"),
	("thigh_r", "rightUpperLeg"), ("calf_r", "rightLowerLeg"), ("foot_r", "rightFoot"), ("ball_r", "rightToes"),
];
// 구면 노멀 중심. (눈 중심 기준 오프셋)
const FACE_SPHERE_OFFSET: [f64; 3] = [0.0, -0.025, -0.05];
const HAIR_SPHERE_OFFSET: [f64; 3] = [0.0, 0.02, -0.03];
// 슬라이더 기본값. (원본 옵션 패널의 기본값 그대로, 이 예제는 패널이 없어 고정이다)
const EXPOSURE: f32 = 1.0;
const BLOOM_STRENGTH: f32 = 0.5;
const BLOOM_THRESHOLD: f64 = 0.5;
const VIGNETTE: f32 = 0.4;
const SATURATION: f32 = 1.12;
const CONTRAST: f32 = 1.06;
const LIGHT_AZIMUTH: f64 = -1.4;
const LIGHT_ELEVATION: f64 = 0.72;
const LIGHT_INTENSITY: f32 = 0.95;
const LIGHT_WARMTH: f64 = 0.2;
const AMBIENT: f32 = 0.3;
const SHADING_TOONY: f32 = 0.85;
const SHADING_SHIFT: f32 = -0.02;
const SHADE_STRENGTH: f32 = 1.0;
const SHADE_DARKNESS: f32 = 0.62;
const WARM_BAND: f32 = 0.3;
const NORMAL_MAP: f32 = 0.45;
const FACE_FLATTEN: f64 = 0.9;
const FACE_SHADOW_SOFTNESS: f32 = 0.035;
const HAIR_SHADOW_OFFSET: f64 = 20.0;
const HAIR_SHADOW_STRENGTH: f32 = 0.85;
const HAIR_SMOOTH: f64 = 0.7;
const HAIR_HIGHLIGHT_WIDTH: f32 = 0.3;
const HAIR_HIGHLIGHT_STRENGTH: f32 = 0.7;
const HAIR_HIGHLIGHT_SHIFT: f32 = 0.0;
const SPECULAR_POWER: f32 = 48.0;
const SPECULAR_STRENGTH: f32 = 0.45;
const SPECULAR_SOFTNESS: f32 = 0.1;
const RIM_WIDTH: f32 = 3.0;
const RIM_THRESHOLD: f32 = 0.012;
const RIM_STRENGTH: f32 = 1.2;
const OUTLINE_WIDTH: f32 = 0.0;
const OUTLINE_MINIMUM: f32 = 1.0;
const OUTLINE_MAXIMUM: f32 = 4.0;
const OUTLINE_TINT: f32 = 0.6;
const OUTLINE_BRIGHTNESS: f32 = 1.0;
const SHADOW_STRENGTH: f32 = 0.85;
const SHADOW_SOFTNESS: f32 = 1.5;
const BLINK_RATE: f64 = 1.0;
const SEE_THROUGH_MAXIMUM_ALPHA: f64 = 0.42;

//==============================================================================
// 수학 보조. (쿼터니언은 엔진 Quaternion)
//==============================================================================
fn quaternion_conjugate(quaternion: Quaternion) -> Quaternion {
	Quaternion { x: -quaternion.x, y: -quaternion.y, z: -quaternion.z, w: quaternion.w }
}

// 회전 행렬(열 우선 요소) → 쿼터니언. (크기 1 가정)
fn quaternion_from_matrix_elements(elements: &[f32; 16]) -> Quaternion {
	let (m00, m10, m20) = (elements[0] as f64, elements[1] as f64, elements[2] as f64);
	let (m01, m11, m21) = (elements[4] as f64, elements[5] as f64, elements[6] as f64);
	let (m02, m12, m22) = (elements[8] as f64, elements[9] as f64, elements[10] as f64);
	let trace = m00 + m11 + m22;
	let (x, y, z, w);
	if trace > 0.0 {
		let scale = (trace + 1.0).sqrt() * 2.0;
		w = 0.25 * scale;
		x = (m21 - m12) / scale;
		y = (m02 - m20) / scale;
		z = (m10 - m01) / scale;
	}
	else if m00 > m11 && m00 > m22 {
		let scale = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
		w = (m21 - m12) / scale;
		x = 0.25 * scale;
		y = (m01 + m10) / scale;
		z = (m02 + m20) / scale;
	}
	else if m11 > m22 {
		let scale = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
		w = (m02 - m20) / scale;
		x = (m01 + m10) / scale;
		y = 0.25 * scale;
		z = (m12 + m21) / scale;
	}
	else {
		let scale = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
		w = (m10 - m01) / scale;
		x = (m02 + m20) / scale;
		y = (m12 + m21) / scale;
		z = 0.25 * scale;
	}
	Quaternion { x, y, z, w }.normalize()
}

fn normalize_vector(vector: [f64; 3]) -> [f64; 3] {
	let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
	if length < 1e-12 {
		return [0.0, 0.0, 0.0];
	}
	[vector[0] / length, vector[1] / length, vector[2] / length]
}

fn rotate_vector(quaternion: Quaternion, vector: [f64; 3]) -> [f64; 3] {
	let q = [quaternion.x, quaternion.y, quaternion.z];
	let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
	let once = cross(q, vector);
	let once = [once[0] + vector[0] * quaternion.w, once[1] + vector[1] * quaternion.w, once[2] + vector[2] * quaternion.w];
	let twice = cross(q, once);
	[vector[0] + twice[0] * 2.0, vector[1] + twice[1] * 2.0, vector[2] + twice[2] * 2.0]
}

// 최단 회전. (단위 벡터 from → to)
fn quaternion_from_to(from: [f64; 3], to: [f64; 3]) -> Quaternion {
	let cosine = from[0] * to[0] + from[1] * to[1] + from[2] * to[2];
	if cosine < -0.999999 {
		let mut orthogonal = [0.0, -from[2], from[1]];
		if orthogonal[1] * orthogonal[1] + orthogonal[2] * orthogonal[2] < 1e-8 {
			orthogonal = [from[2], 0.0, -from[0]];
		}
		let axis = normalize_vector(orthogonal);
		return Quaternion { x: axis[0], y: axis[1], z: axis[2], w: 0.0 };
	}
	let axis = [from[1] * to[2] - from[2] * to[1], from[2] * to[0] - from[0] * to[2], from[0] * to[1] - from[1] * to[0]];
	Quaternion { x: axis[0], y: axis[1], z: axis[2], w: 1.0 + cosine }.normalize()
}

fn clamp_value(value: f64, minimum_value: f64, maximum_value: f64) -> f64 {
	value.max(minimum_value).min(maximum_value)
}

//==============================================================================
// 드로어블 정보. (머티리얼 이름 → 종류 / MToon 그림자색 / 아웃라인)
//==============================================================================
struct DrawableInfo {
	drawable_index: usize,
	material_type: i32,
	shade_color: [f32; 3],
	outline_width: f32,
	outline_color: [f32; 3],
	use_outline_mask: bool,
	use_alpha_test: bool,
	is_see_through_part: bool,
	is_hair: bool,
}

//==============================================================================
// 표정 바인딩. (표정 이름 → 모프 타깃 이름과 가중치)
//==============================================================================
struct ExpressionBind {
	target_name: String,
	weight: f64,
}

//==============================================================================
// 동작 옮기기 한 쌍. (원본 뼈 노드, VRM 뼈 노드, 원본 T 포즈 월드 회전의 역)
//==============================================================================
struct RetargetEntry {
	source_index: usize,
	target_index: usize,
	source_rest_inverse: Quaternion,
}

//==============================================================================
// 노드 제약. (VRMC_node_constraint, roll 은 원본 노드의 축 성분 회전을 복사, aim 은 축이 원본 노드를 향하게)
//==============================================================================
struct NodeConstraint {
	is_roll: bool,
	node_index: usize,
	source_index: usize,
	axis: [f64; 3],
	weight: f64,
}

//==============================================================================
// 단추. (표정 줄, 동작 줄)
//==============================================================================
struct ButtonLayout {
	label: String,
	rect: Rect,
	is_expression: bool,
	name: String,
}

//==============================================================================
// GL 리소스와 모델.
//==============================================================================
struct Resources {
	renderer: SkinnedModelRenderer,
	model: SkinnedModel,
	animation_model: SkinnedModel,
	drawable_info_list: Vec<DrawableInfo>,
	parent_index_list: Vec<Option<usize>>,
	rest_world_rotation_list: Vec<Quaternion>,
	retarget_entry_list: Vec<RetargetEntry>,
	node_name_list: Vec<String>,
	animation_node_name_list: Vec<String>,
	constraint_list: Vec<NodeConstraint>,
	hips_source_index: Option<usize>,
	source_hips_rest_position: [f64; 3],
	hips_height_scale: f64,
	head_node_index: Option<usize>,
	expression_bind_table: HashMap<String, Vec<ExpressionBind>>,
	expression_override_blink: HashMap<String, bool>,
	face_center: [f64; 3],
	hair_center: [f64; 3],
	outline_program: ShaderProgram,
	floor_program: ShaderProgram,
	hair_mask_program: ShaderProgram,
	floor_vertex_array: glow::VertexArray,
	bloom_effect: BloomEffect,
	shadow_map: ShadowMap,
	scene_render_target: RenderTarget,
	hair_render_target: RenderTarget,
	ldr_render_target: RenderTarget,
	background_pass: FullscreenPass,
	composite_pass: FullscreenPass,
	antialias_pass: FullscreenPass,
}

//==============================================================================
// 카메라 상태. (목표값 지수 접근)
//==============================================================================
struct CameraState {
	yaw: f64,
	pitch: f64,
	distance: f64,
	focus_height: f64,
	target_yaw: f64,
	target_pitch: f64,
	target_distance: f64,
	target_focus_height: f64,
	is_manual_focus: bool,
}

//==============================================================================
// 툰 셰이딩 씬.
//==============================================================================
struct ToonShadingScene {
	scene_base: SceneBase,
	resources: Option<Resources>,
	load_error_message: Option<String>,
	camera_state: CameraState,
	elapsed_seconds: f64,
	active_expression: String,
	expression_target_weight: HashMap<String, f64>,
	expression_current_weight: HashMap<String, f64>,
	next_blink_time: f64,
	blink_start_time: f64,
	active_animation: String,
	hips_offset: [f64; 3],
	head_position: [f64; 3],
	button_list: Vec<ButtonLayout>,
	drag_position: Option<Vector2>,
	is_panning: bool,
	random_state: u32,
}

impl ToonShadingScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> ToonShadingScene {
		let mut expression_target_weight = HashMap::new();
		let mut expression_current_weight = HashMap::new();
		for (name, _) in EXPRESSION_DEFINITIONS.iter() {
			expression_target_weight.insert(name.to_string(), 0.0);
			expression_current_weight.insert(name.to_string(), 0.0);
		}
		ToonShadingScene {
			scene_base: SceneBase::new(),
			resources: None,
			load_error_message: None,
			camera_state: CameraState {
				yaw: 0.18,
				pitch: 0.1,
				distance: BODY_FOCUS_DISTANCE,
				focus_height: BODY_FOCUS_HEIGHT,
				target_yaw: 0.18,
				target_pitch: 0.1,
				target_distance: BODY_FOCUS_DISTANCE,
				target_focus_height: BODY_FOCUS_HEIGHT,
				is_manual_focus: false,
			},
			elapsed_seconds: 0.0,
			active_expression: String::new(),
			expression_target_weight,
			expression_current_weight,
			next_blink_time: 2.5,
			blink_start_time: -10.0,
			active_animation: String::new(),
			hips_offset: [0.0, 0.0, 0.0],
			head_position: [0.0, 1.45, 0.0],
			button_list: Vec::new(),
			drag_position: None,
			is_panning: false,
			random_state: 12345,
		}
	}

	//==============================================================================
	// 난수. (깜빡임 간격용, 0 ~ 1)
	//==============================================================================
	fn next_random(&mut self) -> f64 {
		self.random_state = self.random_state.wrapping_mul(1664525).wrapping_add(1013904223);
		(self.random_state >> 8) as f64 / 16777216.0
	}

	//==============================================================================
	// GL 리소스와 모델 만들기.
	//==============================================================================
	fn create_resources(&mut self, gl: &GlContext) -> Result<Resources, String> {
		let asset_directory = PathBuf::from(EXAMPLE_ASSET_DIRECTORY);
		let read_asset = |file_name: &str| -> Result<Vec<u8>, String> {
			std::fs::read(asset_directory.join(file_name)).map_err(|error| format!("{} 읽기 실패: {}", file_name, error))
		};
		let morph_glsl = MORPH_GLSL;
		let build_source = |source: &str| -> String { source.replace("${SHADING_COMMON_GLSL}", SHADING_COMMON_GLSL).replace("${MORPH_GLSL}", morph_glsl).trim().to_string() };

		let renderer = SkinnedModelRenderer::new(gl, Some(&build_source(TOON_FRAGMENTSHADER_SOURCE)), None)?;
		let outline_program = ShaderProgram::new(gl, &build_source(OUTLINE_VERTEXSHADER_SOURCE), &build_source(OUTLINE_FRAGMENTSHADER_SOURCE))?;
		let floor_program = ShaderProgram::new(gl, &build_source(FLOOR_VERTEXSHADER_SOURCE), &build_source(FLOOR_FRAGMENTSHADER_SOURCE))?;
		let hair_mask_program = ShaderProgram::new(gl, &build_source(HAIR_MASK_VERTEXSHADER_SOURCE), &build_source(HAIR_MASK_FRAGMENTSHADER_SOURCE))?;
		let mut bloom_effect = BloomEffect::new(gl)?;
		bloom_effect.set_threshold(1.0, 1.8);
		bloom_effect.set_iteration_count(3);
		let shadow_map = ShadowMap::new(gl, SHADOWMAP_RESOLUTION)?;
		let scene_render_target = RenderTarget::new(gl, 2, 2, true, Some(RenderTargetOptions { color_attachment_count: 2, use_float_color: true, use_depth_texture: true, ..RenderTargetOptions::default() }))?;
		let hair_render_target = RenderTarget::new(gl, 2, 2, true, Some(RenderTargetOptions { use_depth_texture: true, ..RenderTargetOptions::default() }))?;
		let ldr_render_target = RenderTarget::new(gl, 2, 2, false, None)?;
		let background_pass = FullscreenPass::new(gl, &build_source(BACKGROUND_FRAGMENTSHADER_SOURCE))?;
		let composite_pass = FullscreenPass::new(gl, &build_source(COMPOSITE_FRAGMENTSHADER_SOURCE))?;
		let antialias_pass = FullscreenPass::new(gl, &build_source(FXAA_FRAGMENTSHADER_SOURCE))?;

		// 바닥 평면. (정적 버텍스 어레이)
		let floor_vertex_array = unsafe {
			let floor_extent = 5.0f32;
			let floor_positions: [f32; 12] = [-floor_extent, 0.0, -floor_extent, floor_extent, 0.0, -floor_extent, floor_extent, 0.0, floor_extent, -floor_extent, 0.0, floor_extent];
			let floor_indices: [u16; 6] = [0, 2, 1, 0, 3, 2];
			let vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(vertex_array));
			let position_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(position_buffer));
			let position_bytes: Vec<u8> = floor_positions.iter().flat_map(|value| value.to_le_bytes()).collect();
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &position_bytes, glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(0);
			gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 0, 0);
			let index_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index_buffer));
			let index_bytes: Vec<u8> = floor_indices.iter().flat_map(|value| value.to_le_bytes()).collect();
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &index_bytes, glow::STATIC_DRAW);
			gl.bind_vertex_array(None);
			vertex_array
		};

		// 캐릭터. (VRM 1.0 = glTF 바이너리 한 파일)
		let model_bytes = read_asset(MODEL_FILE_NAME)?;
		let (vrm_document, binary_buffer) = SkinnedModel::read_glb_chunks(&model_bytes)?;
		let mut model = SkinnedModel::load_from_bytes(gl, &model_bytes, &renderer)?;
		model.update(0.0);
		let node_list_length = model.get_node_list().len();
		let node_name_list: Vec<String> = model.get_node_list().iter().map(|node| node.name.clone()).collect();
		let mut parent_index_list: Vec<Option<usize>> = vec![None; node_list_length];
		for (node_index, node) in model.get_node_list().iter().enumerate() {
			for child_index in node.child_indices.iter() {
				parent_index_list[*child_index] = Some(node_index);
			}
		}
		let rest_world_rotation_list: Vec<Quaternion> = model.get_node_list().iter().map(|node| quaternion_from_matrix_elements(&node.world_matrix.elements)).collect();
		let rest_hips_height_source = vrm_document["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"].clone();
		let mut humanoid_node_index_by_bone: HashMap<String, usize> = HashMap::new();
		if let Some(bone_table) = rest_hips_height_source.as_object() {
			for (bone_name, bone) in bone_table.iter() {
				if let Some(node_index) = bone["node"].as_u64() {
					humanoid_node_index_by_bone.insert(bone_name.clone(), node_index as usize);
				}
			}
		}
		let head_node_index = humanoid_node_index_by_bone.get("head").copied();

		// 드로어블 정보.
		let mut material_by_name: HashMap<String, Value> = HashMap::new();
		if let Some(material_list) = vrm_document["materials"].as_array() {
			for material in material_list.iter() {
				if let Some(name) = material["name"].as_str() {
					material_by_name.insert(name.to_string(), material.clone());
				}
			}
		}
		let mut outline_mask_texture_by_image: HashMap<usize, glow::Texture> = HashMap::new();
		let mut drawable_info_list: Vec<DrawableInfo> = Vec::new();
		for drawable_index in 0..model.get_drawable_list().len() {
			let material_name = model.get_drawable_list()[drawable_index].material_name.clone();
			let mtoon = material_by_name.get(&material_name).map(|material| material["extensions"]["VRMC_materials_mtoon"].clone()).unwrap_or(Value::Null);
			let mut material_type = MATERIAL_TYPE_CLOTH;
			for (keyword, type_value) in MATERIAL_TYPE_DEFINITIONS.iter() {
				if material_name.contains(keyword) {
					material_type = *type_value;
					break;
				}
			}
			let read_color = |value: &Value, default_color: [f32; 3]| -> [f32; 3] {
				match value.as_array() {
					Some(array) if array.len() >= 3 => [array[0].as_f64().unwrap_or(0.0) as f32, array[1].as_f64().unwrap_or(0.0) as f32, array[2].as_f64().unwrap_or(0.0) as f32],
					_ => default_color,
				}
			};
			let shade_color = read_color(&mtoon["shadeColorFactor"], [1.0, 1.0, 1.0]);
			let is_outline_type = [MATERIAL_TYPE_SKIN, MATERIAL_TYPE_FACE, MATERIAL_TYPE_HAIR, MATERIAL_TYPE_CLOTH].contains(&material_type);
			let outline_width = if mtoon["outlineWidthMode"].as_str() == Some("worldCoordinates") && is_outline_type { mtoon["outlineWidthFactor"].as_f64().unwrap_or(0.0) as f32 } else { 0.0 };
			let outline_color = read_color(&mtoon["outlineColorFactor"], [0.0, 0.0, 0.0]);
			let mut use_outline_mask = false;
			if let Some(texture_index) = mtoon["outlineWidthMultiplyTexture"]["index"].as_u64() {
				let image_index = vrm_document["textures"][texture_index as usize]["source"].as_u64().unwrap_or(0) as usize;
				if !outline_mask_texture_by_image.contains_key(&image_index) {
					let image = &vrm_document["images"][image_index];
					let buffer_view = &vrm_document["bufferViews"][image["bufferView"].as_u64().unwrap_or(0) as usize];
					let byte_offset = buffer_view["byteOffset"].as_u64().unwrap_or(0) as usize;
					let byte_length = buffer_view["byteLength"].as_u64().unwrap_or(0) as usize;
					let image_bytes = binary_buffer[byte_offset..byte_offset + byte_length].to_vec();
					let image_description = Material::create_image_description(image_bytes, image["mimeType"].as_str().map(|text| text.to_string()), None);
					let texture = Material::create_texture(gl, Some(&image_description), [255, 255, 255, 255])?;
					outline_mask_texture_by_image.insert(image_index, texture);
				}
				model.get_drawable_list()[drawable_index].material.set_texture("outlineMaskTexture", TEXTURE_UNIT_OUTLINE_MASK, Some(outline_mask_texture_by_image[&image_index]));
				use_outline_mask = true;
			}
			drawable_info_list.push(DrawableInfo {
				drawable_index,
				material_type,
				shade_color,
				outline_width,
				outline_color,
				use_outline_mask,
				use_alpha_test: [MATERIAL_TYPE_EYE_HIGHLIGHT, MATERIAL_TYPE_EYELINE, MATERIAL_TYPE_BROW].contains(&material_type),
				is_see_through_part: [MATERIAL_TYPE_BROW, MATERIAL_TYPE_EYELINE, MATERIAL_TYPE_EYE_IRIS, MATERIAL_TYPE_EYE_WHITE, MATERIAL_TYPE_EYE_HIGHLIGHT].contains(&material_type),
				is_hair: material_type == MATERIAL_TYPE_HAIR,
			});
		}

		// 머리 위치. (구면 노멀 중심은 T 포즈 머리 뼈 월드 위치 기준)
		let head_rest_position = match head_node_index {
			Some(head_index) => {
				let elements = &model.get_node_list()[head_index].world_matrix.elements;
				[elements[12] as f64, elements[13] as f64, elements[14] as f64]
			}
			None => [0.0, 1.45, 0.0],
		};
		let face_center = [head_rest_position[0] + FACE_SPHERE_OFFSET[0], head_rest_position[1] + FACE_SPHERE_OFFSET[1], head_rest_position[2] + FACE_SPHERE_OFFSET[2]];
		let hair_center = [head_rest_position[0] + HAIR_SPHERE_OFFSET[0], head_rest_position[1] + HAIR_SPHERE_OFFSET[1], head_rest_position[2] + HAIR_SPHERE_OFFSET[2]];
		self.head_position = head_rest_position;
		let vrm_hips_rest_height = humanoid_node_index_by_bone.get("hips").map(|index| model.get_node_list()[*index].world_matrix.elements[13] as f64).unwrap_or(1.0);

		// 동작. (T 포즈 클립으로 원본 골격과 T 포즈 표를 잡고, 클립 여덟을 더한다)
		let mut animation_model = SkinnedModel::load_from_fbx_bytes(gl, &read_asset(REST_POSE_FILE)?, &renderer)?;
		let rest_pose_clip_name = animation_model.get_animation_name_list().first().cloned().unwrap_or_default();
		for (name, _, file) in ANIMATION_DEFINITIONS.iter() {
			animation_model.add_animations_from_fbx_bytes(&read_asset(file)?, name, false)?;
		}
		let animation_node_name_list: Vec<String> = animation_model.get_node_list().iter().map(|node| node.name.split('\0').next().unwrap_or("").to_string()).collect();
		let mut animation_node_index_by_bone: HashMap<String, usize> = HashMap::new();
		for (node_index, name) in animation_node_name_list.iter().enumerate() {
			animation_node_index_by_bone.insert(name.clone(), node_index);
		}
		animation_model.play_animation(&rest_pose_clip_name, 0.0);
		animation_model.update(0.0);
		let mut retarget_entry_list: Vec<RetargetEntry> = Vec::new();
		for (source_bone, target_bone) in SOURCE_BONE_MAP.iter() {
			let source_index = animation_node_index_by_bone.get(*source_bone).copied();
			let target_index = humanoid_node_index_by_bone.get(*target_bone).copied();
			if let (Some(source_index), Some(target_index)) = (source_index, target_index) {
				let rest_rotation = quaternion_from_matrix_elements(&animation_model.get_node_list()[source_index].world_matrix.elements);
				retarget_entry_list.push(RetargetEntry { source_index, target_index, source_rest_inverse: quaternion_conjugate(rest_rotation) });
			}
		}
		let hips_source_index = animation_node_index_by_bone.get(SOURCE_BONE_MAP[0].0).copied();
		let source_hips_rest_position = match hips_source_index {
			Some(index) => {
				let elements = &animation_model.get_node_list()[index].world_matrix.elements;
				[elements[12] as f64, elements[13] as f64, elements[14] as f64]
			}
			None => [0.0, 1.0, 0.0],
		};
		let hips_height_scale = vrm_hips_rest_height / source_hips_rest_position[1].max(0.01);

		// 표정. (VRM 1.0 preset → 모프 타깃 바인딩)
		let mut expression_bind_table: HashMap<String, Vec<ExpressionBind>> = HashMap::new();
		let mut expression_override_blink: HashMap<String, bool> = HashMap::new();
		if let Some(preset_table) = vrm_document["extensions"]["VRMC_vrm"]["expressions"]["preset"].as_object() {
			for (preset_name, preset) in preset_table.iter() {
				let mut bind_list: Vec<ExpressionBind> = Vec::new();
				if let Some(bind_array) = preset["morphTargetBinds"].as_array() {
					for bind in bind_array.iter() {
						let node_index = bind["node"].as_u64().unwrap_or(0) as usize;
						let mesh_index = vrm_document["nodes"][node_index]["mesh"].as_u64().unwrap_or(0) as usize;
						let target_index = bind["index"].as_u64().unwrap_or(0) as usize;
						if let Some(target_name) = vrm_document["meshes"][mesh_index]["extras"]["targetNames"][target_index].as_str() {
							bind_list.push(ExpressionBind { target_name: target_name.to_string(), weight: bind["weight"].as_f64().unwrap_or(1.0) });
						}
					}
				}
				expression_bind_table.insert(preset_name.clone(), bind_list);
				let override_blink = matches!(preset["overrideBlink"].as_str(), Some("blend") | Some("block"));
				expression_override_blink.insert(preset_name.clone(), override_blink);
			}
		}

		// 노드 제약.
		let mut constraint_list: Vec<NodeConstraint> = Vec::new();
		if let Some(node_array) = vrm_document["nodes"].as_array() {
			for (node_index, node_description) in node_array.iter().enumerate() {
				let constraint = &node_description["extensions"]["VRMC_node_constraint"]["constraint"];
				if constraint.is_null() {
					continue;
				}
				if !constraint["roll"].is_null() {
					let axis_name = constraint["roll"]["rollAxis"].as_str().unwrap_or("X");
					let axis = [if axis_name == "X" { 1.0 } else { 0.0 }, if axis_name == "Y" { 1.0 } else { 0.0 }, if axis_name == "Z" { 1.0 } else { 0.0 }];
					constraint_list.push(NodeConstraint { is_roll: true, node_index, source_index: constraint["roll"]["source"].as_u64().unwrap_or(0) as usize, axis, weight: constraint["roll"]["weight"].as_f64().unwrap_or(1.0) });
				}
				else if !constraint["aim"].is_null() {
					let axis_name = constraint["aim"]["aimAxis"].as_str().unwrap_or("PositiveX");
					let sign = if axis_name.starts_with("Negative") { -1.0 } else { 1.0 };
					let letter = &axis_name[axis_name.len() - 1..];
					let axis = [if letter == "X" { sign } else { 0.0 }, if letter == "Y" { sign } else { 0.0 }, if letter == "Z" { sign } else { 0.0 }];
					constraint_list.push(NodeConstraint { is_roll: false, node_index, source_index: constraint["aim"]["source"].as_u64().unwrap_or(0) as usize, axis, weight: constraint["aim"]["weight"].as_f64().unwrap_or(1.0) });
				}
			}
		}

		let mut resources = Resources {
			renderer,
			model,
			animation_model,
			drawable_info_list,
			parent_index_list,
			rest_world_rotation_list,
			retarget_entry_list,
			node_name_list,
			animation_node_name_list,
			constraint_list,
			hips_source_index,
			source_hips_rest_position,
			hips_height_scale,
			head_node_index,
			expression_bind_table,
			expression_override_blink,
			face_center,
			hair_center,
			outline_program,
			floor_program,
			hair_mask_program,
			floor_vertex_array,
			bloom_effect,
			shadow_map,
			scene_render_target,
			hair_render_target,
			ldr_render_target,
			background_pass,
			composite_pass,
			antialias_pass,
		};
		ToonShadingScene::rebake_normals(&mut resources, gl, FACE_FLATTEN, HAIR_SMOOTH);
		Ok(resources)
	}

	//==============================================================================
	// 노멀 굽기. (얼굴, 머리카락의 정점 노멀을 구면 노멀과 섞어 노멀 버퍼를 교체)
	//==============================================================================
	fn rebake_normals(resources: &mut Resources, gl: &GlContext, face_blend: f64, hair_blend: f64) {
		for info in resources.drawable_info_list.iter() {
			let is_face = info.material_type == MATERIAL_TYPE_FACE;
			let is_hair = info.material_type == MATERIAL_TYPE_HAIR;
			if !is_face && !is_hair {
				continue;
			}
			let blend = if is_face { face_blend } else { hair_blend };
			let center = if is_face { resources.face_center } else { resources.hair_center };
			let drawable = &resources.model.get_drawable_list()[info.drawable_index];
			let positions = &drawable.mesh_description.positions;
			let original_normals = drawable.mesh_description.normals.as_ref();
			let vertex_count = positions.len() / 3;
			let mut normal_bytes: Vec<u8> = Vec::with_capacity(vertex_count * 12);
			for vertex_index in 0..vertex_count {
				let mut sphere = [positions[vertex_index * 3] - center[0], positions[vertex_index * 3 + 1] - center[1], positions[vertex_index * 3 + 2] - center[2]];
				let sphere_length = (sphere[0] * sphere[0] + sphere[1] * sphere[1] + sphere[2] * sphere[2]).sqrt().max(1e-9);
				for component in sphere.iter_mut() {
					*component /= sphere_length;
				}
				let original = match original_normals {
					Some(normals) => [normals[vertex_index * 3], normals[vertex_index * 3 + 1], normals[vertex_index * 3 + 2]],
					None => sphere,
				};
				let mut mixed = [original[0] * (1.0 - blend) + sphere[0] * blend, original[1] * (1.0 - blend) + sphere[1] * blend, original[2] * (1.0 - blend) + sphere[2] * blend];
				let mixed_length = (mixed[0] * mixed[0] + mixed[1] * mixed[1] + mixed[2] * mixed[2]).sqrt().max(1e-9);
				for component in mixed.iter_mut() {
					*component /= mixed_length;
				}
				for component in mixed.iter() {
					normal_bytes.extend_from_slice(&(*component as f32).to_le_bytes());
				}
			}
			// 정점 버퍼 목록의 둘째가 노멀이다. (위치, 노멀, 텍스처 좌표, 조인트, 가중치 차례로 짓는다)
			if let Some(normal_buffer) = drawable.vertex_buffer_list.get(1) {
				unsafe {
					gl.bind_buffer(glow::ARRAY_BUFFER, Some(*normal_buffer));
					gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &normal_bytes, glow::STATIC_DRAW);
					gl.bind_buffer(glow::ARRAY_BUFFER, None);
				}
			}
		}
	}

	//==============================================================================
	// 표정 고르기.
	//==============================================================================
	fn select_expression(&mut self, preset_name: &str) {
		self.active_expression = preset_name.to_string();
		for (name, _) in EXPRESSION_DEFINITIONS.iter() {
			self.expression_target_weight.insert(name.to_string(), if *name == preset_name { 1.0 } else { 0.0 });
		}
	}

	//==============================================================================
	// 동작 고르기.
	//==============================================================================
	fn select_animation(&mut self, animation_name: &str) {
		if self.active_animation == animation_name {
			return;
		}
		self.active_animation = animation_name.to_string();
		if let Some(resources) = self.resources.as_mut() {
			resources.animation_model.play_animation(animation_name, 0.3);
		}
	}

	//==============================================================================
	// 줌 거리에 맞는 초점 높이. (멀면 몸 가운데, 가까우면 얼굴)
	//==============================================================================
	fn get_automatic_focus_height(&self, distance: f64) -> f64 {
		let zoom_ratio = clamp_value((distance - FACE_FOCUS_DISTANCE) / (BODY_FOCUS_DISTANCE - FACE_FOCUS_DISTANCE), 0.0, 1.0);
		let eased = zoom_ratio * zoom_ratio * (3.0 - 2.0 * zoom_ratio);
		self.head_position[1] + (BODY_FOCUS_HEIGHT - self.head_position[1]) * eased
	}

	fn set_target_distance(&mut self, distance: f64) {
		self.camera_state.target_distance = clamp_value(distance, CAMERA_MINIMUM_DISTANCE, CAMERA_MAXIMUM_DISTANCE);
		if !self.camera_state.is_manual_focus {
			self.camera_state.target_focus_height = self.get_automatic_focus_height(self.camera_state.target_distance);
		}
	}

	//==============================================================================
	// 단추 자리 계산. (화면 가운데 아래, 표정 줄 위에 동작 줄이 아니라 아래, 원본은 표정 줄 아래에 동작 줄)
	//==============================================================================
	fn layout_buttons(&mut self, graphic: &mut Graphic, native_size: Vector2) {
		let scale = native_size.x / 1280.0;
		graphic.set_font_string(&format!("{}px sans-serif", 10.0 * scale));
		let row_height = 28.0 * scale;
		let gap = 6.0 * scale;
		let row_gap = 8.0 * scale;
		let panel_bottom = native_size.y - (native_size.y * 0.055 + 52.0 * scale);
		let mut row_list: Vec<Vec<(String, String, bool, f64)>> = Vec::new();
		let mut expression_row = Vec::new();
		for (name, label) in EXPRESSION_DEFINITIONS.iter() {
			let has_bind = self.resources.as_ref().map(|resources| resources.expression_bind_table.contains_key(*name)).unwrap_or(false);
			if has_bind {
				let width = ToonShadingScene::measure_spaced(graphic, label, 1.8 * scale) + 26.0 * scale;
				expression_row.push((name.to_string(), label.to_string(), true, width));
			}
		}
		let mut animation_row = Vec::new();
		for (name, label, _) in ANIMATION_DEFINITIONS.iter() {
			let width = ToonShadingScene::measure_spaced(graphic, label, 1.8 * scale) + 26.0 * scale;
			animation_row.push((name.to_string(), label.to_string(), false, width));
		}
		row_list.push(expression_row);
		row_list.push(animation_row);
		self.button_list.clear();
		for (row_index, row) in row_list.iter().enumerate() {
			let total_width: f64 = row.iter().map(|entry| entry.3).sum::<f64>() + gap * (row.len().max(1) - 1) as f64;
			let mut x = (native_size.x - total_width) * 0.5;
			let y = panel_bottom - row_height * (2 - row_index) as f64 - row_gap * (1 - row_index as i32).max(0) as f64;
			for (name, label, is_expression, width) in row.iter() {
				self.button_list.push(ButtonLayout { label: label.clone(), rect: Rect::create(x, y, *width, row_height), is_expression: *is_expression, name: name.clone() });
				x += width + gap;
			}
		}
	}

	//==============================================================================
	// 글자 사이를 띄운 폭 재기.
	//==============================================================================
	fn measure_spaced(graphic: &Graphic, text: &str, spacing: f64) -> f64 {
		let mut width = 0.0;
		for character in text.chars() {
			width += graphic.measure_text(&character.to_string()).width + spacing;
		}
		width
	}

	//==============================================================================
	// 글자 사이를 띄워 그리기. (가운데 맞춤이면 x 는 가운데)
	//==============================================================================
	fn draw_spaced_text(graphic: &mut Graphic, text: &str, x: f64, y: f64, spacing: f64, is_centered: bool) {
		let total_width = ToonShadingScene::measure_spaced(graphic, text, spacing) - spacing;
		let mut cursor = if is_centered { x - total_width * 0.5 } else { x };
		graphic.set_text_align(TextAlign::Left);
		for character in text.chars() {
			let character_text = character.to_string();
			graphic.draw_fill_text(&character_text, cursor, y);
			cursor += graphic.measure_text(&character_text).width + spacing;
		}
	}

	//==============================================================================
	// 3D 그리기. (섀도우, 머리카락 깊이 → 씬 MRT → 블룸 → 합성 → FXAA)
	//==============================================================================
	#[allow(clippy::too_many_lines)]
	fn render_scene(&mut self, gl: &GlContext, display_width: i32, display_height: i32) -> Result<(), String> {
		let camera_state = &self.camera_state;
		let elapsed_time = self.elapsed_seconds;
		let hips_offset = self.hips_offset;
		let resources = match self.resources.as_mut() {
			Some(resources) => resources,
			None => {
				return Ok(());
			}
		};

		// 렌더 해상도. (RENDER BASE 1080P 의 픽셀 수를 화면 종횡비로 잡는다)
		let pixel_budget = RENDER_BASE_HEIGHT * (16.0 / 9.0) * RENDER_BASE_HEIGHT;
		let pixel_budget_scale = (pixel_budget / (display_width as f64 * display_height as f64).max(1.0)).sqrt();
		let render_width = ((display_width as f64 * pixel_budget_scale).floor() as i32).max(2);
		let render_height = ((display_height as f64 * pixel_budget_scale).floor() as i32).max(2);
		if resources.scene_render_target.get_width() != render_width || resources.scene_render_target.get_height() != render_height {
			resources.scene_render_target.resize(gl, render_width, render_height)?;
			resources.hair_render_target.resize(gl, render_width, render_height)?;
			resources.ldr_render_target.resize(gl, render_width, render_height)?;
			resources.bloom_effect.resize(gl, render_width, render_height)?;
		}
		let aspect_ratio = display_width as f64 / display_height.max(1) as f64;

		// 모델 행렬. (골반 이동)
		let model_matrix = Matrix4::create_translation(hips_offset[0], hips_offset[1], hips_offset[2]);
		let model_elements = *model_matrix.get_elements();

		// 카메라. (미세한 자동 흔들림)
		let sway_yaw = camera_state.yaw + (elapsed_time * 0.19).sin() * 0.015;
		let sway_pitch = camera_state.pitch + (elapsed_time * 0.27 + 1.0).sin() * 0.008;
		let portrait_factor = if aspect_ratio < 1.2 { (1.2 / aspect_ratio).powf(0.55) } else { 1.0 };
		let camera_distance = camera_state.distance * portrait_factor;
		let camera_target = Vector3::create(0.0, camera_state.focus_height, 0.0);
		let camera_x = camera_target.x + sway_yaw.sin() * sway_pitch.cos() * camera_distance;
		let camera_y = camera_target.y + sway_pitch.sin() * camera_distance;
		let camera_z = camera_target.z + sway_yaw.cos() * sway_pitch.cos() * camera_distance;
		let camera_position = Vector3::create(camera_x, camera_y, camera_z);
		let view_matrix = Matrix4::create_look_at(camera_position, camera_target, Vector3::up());
		let projection_matrix = Matrix4::create_perspective(CAMERA_FIELD_OF_VIEW, aspect_ratio, CAMERA_NEAR_DISTANCE, CAMERA_FAR_DISTANCE);
		let mut view_projection_matrix = projection_matrix;
		view_projection_matrix.multiply(&view_matrix);
		let view_projection_elements = *view_projection_matrix.get_elements();
		let view_elements = *view_matrix.get_elements();
		let projection_scale_y = projection_matrix.get_elements()[5];

		// 조명. (방위, 고도 → 빛을 향하는 방향, 따뜻함 → 색)
		let light_x = LIGHT_AZIMUTH.sin() * LIGHT_ELEVATION.cos();
		let light_y = LIGHT_ELEVATION.sin();
		let light_z = LIGHT_AZIMUTH.cos() * LIGHT_ELEVATION.cos();
		let light_length = (light_x * light_x + light_y * light_y + light_z * light_z).sqrt();
		let light_direction = Vector3::create(light_x / light_length, light_y / light_length, light_z / light_length);
		let light_color = [1.0f32, (1.0 - 0.1 * LIGHT_WARMTH) as f32, (1.0 - 0.26 * LIGHT_WARMTH) as f32];
		let light_view = view_matrix.transform_direction(light_direction.x, light_direction.y, light_direction.z);
		let light_screen_length = (light_view.x * light_view.x + light_view.y * light_view.y).sqrt().max(0.0001);
		let light_screen_direction = [(light_view.x / light_screen_length) as f32, (light_view.y / light_screen_length) as f32];

		// 앞머리 너머 비침. (머리가 카메라 쪽을 볼 때만, 머리는 +Z 정면)
		let head_position = self.head_position;
		let to_camera = [camera_x - head_position[0], camera_y - head_position[1], camera_z - head_position[2]];
		let to_camera_length = (to_camera[0] * to_camera[0] + to_camera[1] * to_camera[1] + to_camera[2] * to_camera[2]).sqrt().max(1e-9);
		let facing = to_camera[2] / to_camera_length;
		let see_through_alpha = (SEE_THROUGH_MAXIMUM_ALPHA * clamp_value((facing - 0.25) / 0.3, 0.0, 1.0)) as f32;

		resources.model.upload_pending_morph_targets(gl);

		// 1. 섀도우 깊이 패스(키 라이트)와 머리카락 깊이 패스(카메라).
		unsafe {
			gl.disable(glow::CULL_FACE);
			gl.disable(glow::BLEND);
		}
		let light_travel_direction = Vector3::create(-light_direction.x, -light_direction.y, -light_direction.z);
		resources.shadow_map.update_light_matrix(Vector3::create(0.0, 0.82, 0.0), light_travel_direction, 1.25, 0.1, 8.0);
		let light_view_projection_elements = *resources.shadow_map.get_light_view_projection_matrix().get_elements();
		resources.shadow_map.begin_render(gl);
		if SHADOW_STRENGTH > 0.001 {
			resources.renderer.draw_depth(gl, &mut resources.model, &light_view_projection_elements, &model_elements);
		}
		resources.hair_render_target.bind(gl);
		unsafe {
			gl.enable(glow::DEPTH_TEST);
			gl.depth_mask(true);
			gl.depth_func(glow::LESS);
			gl.clear_color(0.0, 0.0, 0.0, 0.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
		{
			let depth_program = &resources.hair_mask_program;
			depth_program.use_program(gl);
			unsafe {
				gl.uniform_matrix_4_f32_slice(depth_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_elements);
				gl.uniform_matrix_4_f32_slice(depth_program.get_uniform_location(gl, "modelMatrix").as_ref(), false, &model_elements);
			}
			let joint_matrices_location = depth_program.get_uniform_location(gl, "jointMatrices[0]");
			for info in resources.drawable_info_list.iter() {
				if !info.is_hair {
					continue;
				}
				let drawable = &resources.model.get_drawable_list()[info.drawable_index];
				unsafe {
					gl.uniform_matrix_4_f32_slice(joint_matrices_location.as_ref(), false, &resources.model.get_skin_list()[drawable.skin_index].joint_matrix_array);
				}
				resources.renderer.apply_morph_uniforms(gl, depth_program, drawable);
				ToonShadingScene::draw_drawable(gl, drawable);
			}
			unsafe {
				gl.bind_vertex_array(None);
			}
		}

		// 2. 씬 MRT. (배경 → 바닥 → 셀 셰이딩 → 아웃라인 → 앞머리 비침)
		resources.scene_render_target.bind(gl);
		unsafe {
			gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
			gl.clear_color(0.0, 0.0, 0.0, 0.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
			gl.disable(glow::DEPTH_TEST);
		}
		resources.background_pass.use_program(gl);
		unsafe {
			gl.uniform_1_f32(resources.background_pass.get_uniform_location(gl, "aspectRatio").as_ref(), aspect_ratio as f32);
			gl.uniform_3_f32(resources.background_pass.get_uniform_location(gl, "backgroundColor").as_ref(), 0.014, 0.013, 0.016);
			gl.uniform_3_f32(resources.background_pass.get_uniform_location(gl, "glowColor").as_ref(), 0.085, 0.07, 0.05);
		}
		resources.background_pass.draw(gl);
		unsafe {
			gl.enable(glow::DEPTH_TEST);
			gl.depth_mask(true);
		}
		// 바닥.
		{
			let floor_program = &resources.floor_program;
			floor_program.use_program(gl);
			let shadow_texture = resources.shadow_map.get_depth_texture();
			unsafe {
				gl.uniform_matrix_4_f32_slice(floor_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &view_projection_elements);
				gl.uniform_matrix_4_f32_slice(floor_program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &light_view_projection_elements);
				gl.uniform_1_i32(floor_program.get_uniform_location(gl, "shadowMapTexture").as_ref(), 1);
				gl.uniform_1_f32(floor_program.get_uniform_location(gl, "shadowTexelSize").as_ref(), 1.0 / SHADOWMAP_RESOLUTION as f32);
				gl.uniform_1_f32(floor_program.get_uniform_location(gl, "shadowStrength").as_ref(), SHADOW_STRENGTH);
				gl.uniform_1_f32(floor_program.get_uniform_location(gl, "shadowSoftness").as_ref(), SHADOW_SOFTNESS);
				gl.uniform_3_f32(floor_program.get_uniform_location(gl, "backgroundColor").as_ref(), 0.014, 0.013, 0.016);
				gl.uniform_3_f32(floor_program.get_uniform_location(gl, "glowColor").as_ref(), 0.085, 0.07, 0.05);
				gl.uniform_2_f32(floor_program.get_uniform_location(gl, "screenResolution").as_ref(), render_width as f32, render_height as f32);
				gl.uniform_1_f32(floor_program.get_uniform_location(gl, "aspectRatio").as_ref(), aspect_ratio as f32);
				gl.uniform_3_f32(floor_program.get_uniform_location(gl, "floorColor").as_ref(), 0.05, 0.047, 0.052);
				gl.uniform_3_f32(floor_program.get_uniform_location(gl, "ringColor").as_ref(), 0.9, 0.72, 0.42);
				gl.uniform_1_f32(floor_program.get_uniform_location(gl, "ringRadius").as_ref(), 1.05);
				gl.active_texture(glow::TEXTURE1);
				gl.bind_texture(glow::TEXTURE_2D, Some(shadow_texture));
				gl.active_texture(glow::TEXTURE0);
				gl.bind_vertex_array(Some(resources.floor_vertex_array));
				gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_SHORT, 0);
				gl.bind_vertex_array(None);
			}
		}
		// 셀 셰이딩.
		ToonShadingScene::draw_toon_pass(resources, gl, &view_projection_elements, &view_elements, &model_elements, &light_view_projection_elements, [camera_x, camera_y, camera_z], [light_direction.x as f32, light_direction.y as f32, light_direction.z as f32], light_color, light_screen_direction, render_width, render_height, see_through_alpha, false);
		// 아웃라인. (원본 기본 OUTLINE WIDTH 0 배라 그려지지 않는다)
		if OUTLINE_WIDTH > 0.0 {
			unsafe {
				gl.enable(glow::CULL_FACE);
				gl.cull_face(glow::FRONT);
			}
			ToonShadingScene::draw_outline_pass(resources, gl, &view_projection_elements, &view_elements, &model_elements, render_width, render_height, projection_scale_y as f32);
			unsafe {
				gl.disable(glow::CULL_FACE);
			}
		}
		// 앞머리 너머 비침.
		if see_through_alpha > 0.001 {
			unsafe {
				gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::NONE]);
				gl.enable(glow::BLEND);
				gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
				gl.depth_mask(false);
				gl.depth_func(glow::GREATER);
			}
			ToonShadingScene::draw_toon_pass(resources, gl, &view_projection_elements, &view_elements, &model_elements, &light_view_projection_elements, [camera_x, camera_y, camera_z], [light_direction.x as f32, light_direction.y as f32, light_direction.z as f32], light_color, light_screen_direction, render_width, render_height, see_through_alpha, true);
			unsafe {
				gl.depth_func(glow::LESS);
				gl.depth_mask(true);
				gl.disable(glow::BLEND);
				gl.draw_buffers(&[glow::COLOR_ATTACHMENT0, glow::COLOR_ATTACHMENT1]);
			}
		}
		unsafe {
			gl.disable(glow::DEPTH_TEST);
		}

		// 3. 블룸.
		let scene_color_texture = resources.scene_render_target.get_color_texture(0);
		resources.bloom_effect.set_threshold(BLOOM_THRESHOLD, BLOOM_THRESHOLD + 0.85);
		resources.bloom_effect.render(gl, scene_color_texture);

		// 4. 림 + 톤 매핑 합성 → LDR.
		resources.ldr_render_target.bind(gl);
		{
			let composite_pass = &resources.composite_pass;
			composite_pass.use_program(gl);
			unsafe {
				gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
				gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "normalTexture").as_ref(), 1);
				gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "depthTexture").as_ref(), 2);
				gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 3);
				gl.uniform_2_f32(composite_pass.get_uniform_location(gl, "resolution").as_ref(), render_width as f32, render_height as f32);
				gl.uniform_2_f32(composite_pass.get_uniform_location(gl, "cameraPlanes").as_ref(), CAMERA_NEAR_DISTANCE as f32, CAMERA_FAR_DISTANCE as f32);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "exposure").as_ref(), EXPOSURE);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), BLOOM_STRENGTH);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "vignetteStrength").as_ref(), VIGNETTE);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "saturation").as_ref(), SATURATION);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "contrast").as_ref(), CONTRAST);
				gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "useRim").as_ref(), 1);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "rimWidth").as_ref(), RIM_WIDTH * render_height as f32 / 1080.0);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "rimThreshold").as_ref(), RIM_THRESHOLD);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "rimStrength").as_ref(), RIM_STRENGTH);
				gl.uniform_3_f32(composite_pass.get_uniform_location(gl, "rimColor").as_ref(), light_color[0], light_color[1], light_color[2]);
				gl.uniform_2_f32(composite_pass.get_uniform_location(gl, "rimLightScreenDirection").as_ref(), light_screen_direction[0], light_screen_direction[1]);
				gl.active_texture(glow::TEXTURE0);
				gl.bind_texture(glow::TEXTURE_2D, scene_color_texture);
				gl.active_texture(glow::TEXTURE1);
				gl.bind_texture(glow::TEXTURE_2D, resources.scene_render_target.get_color_texture(1));
				gl.active_texture(glow::TEXTURE2);
				gl.bind_texture(glow::TEXTURE_2D, resources.scene_render_target.get_depth_texture());
				gl.active_texture(glow::TEXTURE3);
				gl.bind_texture(glow::TEXTURE_2D, resources.bloom_effect.get_result_texture());
				gl.active_texture(glow::TEXTURE0);
			}
			composite_pass.draw(gl);
			unsafe {
				// 유닛 1 은 다음 프레임에 섀도우 맵이 쓴다. (일반 텍스처가 남아 있으면 비교 샘플러와 어긋난다)
				gl.active_texture(glow::TEXTURE1);
				gl.bind_texture(glow::TEXTURE_2D, None);
				gl.active_texture(glow::TEXTURE2);
				gl.bind_texture(glow::TEXTURE_2D, None);
				gl.active_texture(glow::TEXTURE0);
			}
		}

		// 5. FXAA → 화면.
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.viewport(0, 0, display_width, display_height);
		}
		{
			let antialias_pass = &resources.antialias_pass;
			antialias_pass.use_program(gl);
			unsafe {
				gl.uniform_1_i32(antialias_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
				gl.uniform_2_f32(antialias_pass.get_uniform_location(gl, "texelSize").as_ref(), 1.0 / render_width as f32, 1.0 / render_height as f32);
				gl.uniform_1_f32(antialias_pass.get_uniform_location(gl, "sharpenStrength").as_ref(), 0.0);
				gl.active_texture(glow::TEXTURE0);
				gl.bind_texture(glow::TEXTURE_2D, resources.ldr_render_target.get_color_texture(0));
			}
			antialias_pass.draw(gl);
		}
		Ok(())
	}

	//==============================================================================
	// 드로어블 출력.
	//==============================================================================
	fn draw_drawable(gl: &GlContext, drawable: &vanilla::Drawable) {
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

	//==============================================================================
	// 셀 셰이딩 패스. (모든 드로어블, 비침 패스면 눈 부위만)
	//==============================================================================
	#[allow(clippy::too_many_arguments)]
	fn draw_toon_pass(resources: &Resources, gl: &GlContext, view_projection_elements: &[f32; 16], view_elements: &[f32; 16], model_elements: &[f32; 16], light_view_projection_elements: &[f32; 16], camera_position: [f64; 3], light_direction: [f32; 3], light_color: [f32; 3], light_screen_direction: [f32; 2], render_width: i32, render_height: i32, see_through_alpha: f32, is_see_through_pass: bool) {
		let shader_program = resources.renderer.get_shader_program();
		shader_program.use_program(gl);
		let location = |name: &str| shader_program.get_uniform_location(gl, name);
		let hair_shadow_pixels = (HAIR_SHADOW_OFFSET * render_height as f64 / 1080.0) as f32;
		unsafe {
			gl.uniform_matrix_4_f32_slice(location("viewProjectionMatrix").as_ref(), false, view_projection_elements);
			gl.uniform_matrix_4_f32_slice(location("modelMatrix").as_ref(), false, model_elements);
			gl.uniform_matrix_4_f32_slice(location("viewMatrix").as_ref(), false, view_elements);
			gl.uniform_3_f32(location("cameraPosition").as_ref(), camera_position[0] as f32, camera_position[1] as f32, camera_position[2] as f32);
			gl.uniform_2_f32(location("screenResolution").as_ref(), render_width as f32, render_height as f32);
			gl.uniform_2_f32(location("cameraPlanes").as_ref(), CAMERA_NEAR_DISTANCE as f32, CAMERA_FAR_DISTANCE as f32);
			gl.uniform_3_f32(location("keyLightDirection").as_ref(), light_direction[0], light_direction[1], light_direction[2]);
			gl.uniform_3_f32(location("keyLightColor").as_ref(), light_color[0], light_color[1], light_color[2]);
			gl.uniform_1_f32(location("keyLightIntensity").as_ref(), LIGHT_INTENSITY);
			gl.uniform_3_f32(location("ambientSkyColor").as_ref(), 0.34 * AMBIENT, 0.36 * AMBIENT, 0.42 * AMBIENT);
			gl.uniform_3_f32(location("ambientGroundColor").as_ref(), 0.2 * AMBIENT, 0.17 * AMBIENT, 0.17 * AMBIENT);
			gl.uniform_1_f32(location("shadingToony").as_ref(), SHADING_TOONY);
			gl.uniform_1_f32(location("shadingShift").as_ref(), SHADING_SHIFT);
			gl.uniform_1_f32(location("shadeStrength").as_ref(), SHADE_STRENGTH);
			gl.uniform_1_f32(location("shadeDarkness").as_ref(), SHADE_DARKNESS);
			gl.uniform_1_f32(location("warmBandStrength").as_ref(), WARM_BAND);
			gl.uniform_1_f32(location("normalMapStrength").as_ref(), NORMAL_MAP);
			gl.uniform_3_f32(location("headForward").as_ref(), 0.0, 0.0, 1.0);
			gl.uniform_1_f32(location("faceFlatten").as_ref(), FACE_FLATTEN as f32);
			gl.uniform_1_f32(location("faceShadowSoftness").as_ref(), FACE_SHADOW_SOFTNESS);
			gl.uniform_1_i32(location("useFaceShadow").as_ref(), 1);
			gl.uniform_1_i32(location("useHairShadow").as_ref(), 1);
			gl.uniform_2_f32(location("hairShadowOffset").as_ref(), light_screen_direction[0] * hair_shadow_pixels, light_screen_direction[1] * hair_shadow_pixels);
			gl.uniform_1_f32(location("hairShadowStrength").as_ref(), HAIR_SHADOW_STRENGTH);
			gl.uniform_1_i32(location("useSpecular").as_ref(), 1);
			gl.uniform_1_f32(location("specularPower").as_ref(), SPECULAR_POWER);
			gl.uniform_1_f32(location("specularStrength").as_ref(), SPECULAR_STRENGTH);
			gl.uniform_1_f32(location("specularSoftness").as_ref(), SPECULAR_SOFTNESS);
			gl.uniform_1_i32(location("useHairHighlight").as_ref(), 1);
			gl.uniform_1_f32(location("hairHighlightWidth").as_ref(), HAIR_HIGHLIGHT_WIDTH);
			gl.uniform_1_f32(location("hairHighlightStrength").as_ref(), HAIR_HIGHLIGHT_STRENGTH);
			gl.uniform_1_f32(location("hairHighlightShift").as_ref(), HAIR_HIGHLIGHT_SHIFT);
			gl.uniform_1_i32(location("hairDepthTexture").as_ref(), TEXTURE_UNIT_HAIR_DEPTH as i32);
			gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT_HAIR_DEPTH);
			gl.bind_texture(glow::TEXTURE_2D, resources.hair_render_target.get_depth_texture());
			gl.uniform_1_i32(location("hairMaskTexture").as_ref(), TEXTURE_UNIT_HAIR_MASK as i32);
			gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT_HAIR_MASK);
			gl.bind_texture(glow::TEXTURE_2D, resources.hair_render_target.get_color_texture(0));
			gl.active_texture(glow::TEXTURE0);
			// 섀도우 맵.
			gl.uniform_matrix_4_f32_slice(location("lightViewProjectionMatrix").as_ref(), false, light_view_projection_elements);
			gl.uniform_1_i32(location("shadowMapTexture").as_ref(), 1);
			gl.uniform_1_f32(location("shadowTexelSize").as_ref(), 1.0 / SHADOWMAP_RESOLUTION as f32);
			gl.uniform_1_f32(location("shadowStrength").as_ref(), SHADOW_STRENGTH);
			gl.uniform_1_f32(location("shadowSoftness").as_ref(), SHADOW_SOFTNESS);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, Some(resources.shadow_map.get_depth_texture()));
			gl.active_texture(glow::TEXTURE0);
			gl.uniform_1_i32(location("isSeeThrough").as_ref(), i32::from(is_see_through_pass));
			gl.uniform_1_f32(location("seeThroughAlpha").as_ref(), see_through_alpha);
		}
		let joint_matrices_location = location("jointMatrices[0]");
		let material_type_location = location("materialType");
		let shade_color_location = location("shadeColor");
		let use_alpha_test_location = location("useAlphaTest");
		for info in resources.drawable_info_list.iter() {
			if is_see_through_pass && !info.is_see_through_part {
				continue;
			}
			let drawable = &resources.model.get_drawable_list()[info.drawable_index];
			unsafe {
				gl.uniform_matrix_4_f32_slice(joint_matrices_location.as_ref(), false, &resources.model.get_skin_list()[drawable.skin_index].joint_matrix_array);
			}
			resources.renderer.apply_morph_uniforms(gl, &shader_program, drawable);
			drawable.material.apply(gl);
			unsafe {
				gl.uniform_1_i32(material_type_location.as_ref(), info.material_type);
				gl.uniform_3_f32(shade_color_location.as_ref(), info.shade_color[0], info.shade_color[1], info.shade_color[2]);
				gl.uniform_1_i32(use_alpha_test_location.as_ref(), i32::from(info.use_alpha_test));
			}
			ToonShadingScene::draw_drawable(gl, drawable);
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	// 아웃라인 패스. (뒤집힌 껍질, 앞면 컬링)
	//==============================================================================
	#[allow(clippy::too_many_arguments)]
	fn draw_outline_pass(resources: &Resources, gl: &GlContext, view_projection_elements: &[f32; 16], view_elements: &[f32; 16], model_elements: &[f32; 16], render_width: i32, render_height: i32, projection_scale_y: f32) {
		let program = &resources.outline_program;
		program.use_program(gl);
		unsafe {
			gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, view_projection_elements);
			gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "modelMatrix").as_ref(), false, model_elements);
			gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "viewMatrix").as_ref(), false, view_elements);
			gl.uniform_2_f32(program.get_uniform_location(gl, "screenResolution").as_ref(), render_width as f32, render_height as f32);
			gl.uniform_1_f32(program.get_uniform_location(gl, "projectionScaleY").as_ref(), projection_scale_y);
			gl.uniform_2_f32(program.get_uniform_location(gl, "outlinePixelRange").as_ref(), OUTLINE_MINIMUM, OUTLINE_MINIMUM.max(OUTLINE_MAXIMUM));
			gl.uniform_1_f32(program.get_uniform_location(gl, "outlineTint").as_ref(), OUTLINE_TINT);
			gl.uniform_1_f32(program.get_uniform_location(gl, "outlineBrightness").as_ref(), OUTLINE_BRIGHTNESS);
		}
		let joint_matrices_location = program.get_uniform_location(gl, "jointMatrices[0]");
		let outline_width_location = program.get_uniform_location(gl, "outlineWidth");
		let use_outline_mask_location = program.get_uniform_location(gl, "useOutlineMask");
		let outline_color_location = program.get_uniform_location(gl, "outlineColorFactor");
		for info in resources.drawable_info_list.iter() {
			if info.outline_width <= 0.0 {
				continue;
			}
			let drawable = &resources.model.get_drawable_list()[info.drawable_index];
			unsafe {
				gl.uniform_matrix_4_f32_slice(joint_matrices_location.as_ref(), false, &resources.model.get_skin_list()[drawable.skin_index].joint_matrix_array);
			}
			resources.renderer.apply_morph_uniforms(gl, program, drawable);
			for (uniform_name, texture_unit, texture) in drawable.material.get_texture_bindings().iter() {
				let uniform_location = program.get_uniform_location(gl, uniform_name);
				if uniform_location.is_none() {
					continue;
				}
				unsafe {
					gl.uniform_1_i32(uniform_location.as_ref(), *texture_unit as i32);
					gl.active_texture(glow::TEXTURE0 + *texture_unit);
					gl.bind_texture(glow::TEXTURE_2D, *texture);
				}
			}
			unsafe {
				gl.active_texture(glow::TEXTURE0);
				gl.uniform_1_f32(outline_width_location.as_ref(), info.outline_width * OUTLINE_WIDTH);
				gl.uniform_1_i32(use_outline_mask_location.as_ref(), i32::from(info.use_outline_mask));
				gl.uniform_3_f32(outline_color_location.as_ref(), info.outline_color[0], info.outline_color[1], info.outline_color[2]);
			}
			ToonShadingScene::draw_drawable(gl, drawable);
		}
		unsafe {
			gl.bind_vertex_array(None);
		}
	}

	//==============================================================================
	// 표정 갱신. (목표값에 부드럽게 접근, 깜빡임은 blink 프리셋)
	//==============================================================================
	fn update_expressions(&mut self, delta_time: f64) {
		let approach = (delta_time * 6.0).min(1.0);
		let mut blink_override: f64 = 0.0;
		let elapsed_time = self.elapsed_seconds;
		let mut weight_list: Vec<(String, f64)> = Vec::new();
		for (name, _) in EXPRESSION_DEFINITIONS.iter() {
			let current_weight = self.expression_current_weight[*name];
			let target_weight = self.expression_target_weight[*name];
			let next_weight = current_weight + (target_weight - current_weight) * approach;
			self.expression_current_weight.insert(name.to_string(), next_weight);
			weight_list.push((name.to_string(), next_weight));
		}
		let mut blink = 0.0;
		if elapsed_time >= self.next_blink_time {
			self.blink_start_time = elapsed_time;
			let interval = if BLINK_RATE > 0.001 { (2.6 + self.next_random() * 3.4) / BLINK_RATE } else { 1e9 };
			self.next_blink_time = elapsed_time + interval;
		}
		let blink_local = elapsed_time - self.blink_start_time;
		if (0.0..0.24).contains(&blink_local) {
			blink = (blink_local / 0.24 * std::f64::consts::PI).sin();
		}
		if let Some(resources) = self.resources.as_mut() {
			for (name, weight) in weight_list.iter() {
				if let Some(bind_list) = resources.expression_bind_table.get(name) {
					for bind in bind_list.iter() {
						resources.model.set_morph_weight(&bind.target_name, weight * bind.weight);
					}
				}
				if resources.expression_override_blink.get(name).copied().unwrap_or(false) {
					blink_override = blink_override.max(*weight);
				}
			}
			if let Some(bind_list) = resources.expression_bind_table.get("blink") {
				for bind in bind_list.iter() {
					resources.model.set_morph_weight(&bind.target_name, blink * (1.0 - blink_override) * bind.weight);
				}
			}
		}
	}

	//==============================================================================
	// 노드 제약 갱신. (직전 프레임 월드 기준)
	//==============================================================================
	fn update_constraints(&mut self) {
		let resources = match self.resources.as_mut() {
			Some(resources) => resources,
			None => {
				return;
			}
		};
		let mut offset_list: Vec<(usize, Quaternion)> = Vec::new();
		for constraint in resources.constraint_list.iter() {
			let node = &resources.model.get_node_list()[constraint.node_index];
			let source_node = &resources.model.get_node_list()[constraint.source_index];
			if constraint.is_roll {
				let source_delta = quaternion_conjugate(source_node.base_rotation).multiply(source_node.current_rotation);
				let projection = source_delta.x * constraint.axis[0] + source_delta.y * constraint.axis[1] + source_delta.z * constraint.axis[2];
				let twist = Quaternion { x: constraint.axis[0] * projection, y: constraint.axis[1] * projection, z: constraint.axis[2] * projection, w: source_delta.w };
				let twist_length = (twist.x * twist.x + twist.y * twist.y + twist.z * twist.z + twist.w * twist.w).sqrt();
				let twist = if twist_length < 0.000001 { Quaternion::identity() } else { twist.normalize() };
				offset_list.push((constraint.node_index, Quaternion::slerp(Quaternion::identity(), twist, constraint.weight)));
			}
			else {
				let parent_rotation = match resources.parent_index_list[constraint.node_index] {
					Some(parent_index) => quaternion_from_matrix_elements(&resources.model.get_node_list()[parent_index].world_matrix.elements),
					None => Quaternion::identity(),
				};
				let rest_world_rotation = parent_rotation.multiply(node.base_rotation);
				let from_direction = rotate_vector(rest_world_rotation, constraint.axis);
				let node_elements = &node.world_matrix.elements;
				let source_elements = &source_node.world_matrix.elements;
				let to_direction = normalize_vector([(source_elements[12] - node_elements[12]) as f64, (source_elements[13] - node_elements[13]) as f64, (source_elements[14] - node_elements[14]) as f64]);
				let aim_rotation = quaternion_from_to(normalize_vector(from_direction), to_direction);
				let weighted = Quaternion::slerp(Quaternion::identity(), aim_rotation, constraint.weight);
				let new_local = quaternion_conjugate(parent_rotation).multiply(weighted).multiply(parent_rotation).multiply(node.base_rotation);
				offset_list.push((constraint.node_index, quaternion_conjugate(node.base_rotation).multiply(new_local)));
			}
		}
		for (node_index, offset) in offset_list.iter() {
			let node_name = resources.node_name_list[*node_index].clone();
			resources.model.set_joint_rotation_offset(&node_name, *offset);
		}
	}

	//==============================================================================
	// 신체 동작 갱신. (원본 골격 재생 → 뼈마다 T 포즈 대비 월드 회전 변화를 VRM 뼈에 그대로, 부모부터 처리)
	//==============================================================================
	fn apply_body_animation(&mut self, delta_time: f64) {
		let resources = match self.resources.as_mut() {
			Some(resources) => resources,
			None => {
				return;
			}
		};
		resources.animation_model.update(delta_time);
		let node_count = resources.model.get_node_list().len();
		let mut desired_world_by_target: Vec<Option<Quaternion>> = vec![None; node_count];
		let mut offset_list: Vec<(usize, Quaternion)> = Vec::new();
		for entry in resources.retarget_entry_list.iter() {
			let source_world = quaternion_from_matrix_elements(&resources.animation_model.get_node_list()[entry.source_index].world_matrix.elements);
			let world_delta = source_world.multiply(entry.source_rest_inverse);
			let desired_world = world_delta.multiply(resources.rest_world_rotation_list[entry.target_index]);
			desired_world_by_target[entry.target_index] = Some(desired_world);
			let parent_world = match resources.parent_index_list[entry.target_index] {
				None => Quaternion::identity(),
				Some(parent_index) => match desired_world_by_target[parent_index] {
					Some(parent_desired) => parent_desired,
					None => quaternion_from_matrix_elements(&resources.model.get_node_list()[parent_index].world_matrix.elements),
				},
			};
			let base_rotation = resources.model.get_node_list()[entry.target_index].base_rotation;
			let new_local = quaternion_conjugate(parent_world).multiply(desired_world);
			offset_list.push((entry.target_index, quaternion_conjugate(base_rotation).multiply(new_local)));
		}
		for (target_index, offset) in offset_list.iter() {
			let node_name = resources.node_name_list[*target_index].clone();
			resources.model.set_joint_rotation_offset(&node_name, *offset);
		}
		// 골반 이동. (클립의 골반 위치 변화를 키 비율로)
		if let Some(hips_source_index) = resources.hips_source_index {
			let elements = &resources.animation_model.get_node_list()[hips_source_index].world_matrix.elements;
			let rest = resources.source_hips_rest_position;
			let scale = resources.hips_height_scale;
			self.hips_offset = [(elements[12] as f64 - rest[0]) * scale, (elements[13] as f64 - rest[1]) * scale, (elements[14] as f64 - rest[2]) * scale];
		}
		let _ = (&resources.animation_node_name_list, resources.head_node_index);
	}
}

impl Scene for ToonShadingScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 초기화. (GL 리소스와 모델을 엔진의 그래픽으로 만든다)
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
		let load_start_instant = std::time::Instant::now();
		match self.create_resources(graphic.get_gl()) {
			Ok(resources) => {
				println!("toonshading: 준비 {:.2}초, 드로어블 {}, 노드 {}", load_start_instant.elapsed().as_secs_f64(), resources.drawable_info_list.len(), resources.model.get_node_list().len());
				self.resources = Some(resources);
				self.select_expression("neutral");
				self.select_animation("Idle");
			}
			Err(error) => {
				eprintln!("toonshading: {}", error);
				self.load_error_message = Some(error);
			}
		}
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let delta_time = time_delta.min(0.1);
		self.elapsed_seconds += delta_time;
		if self.resources.is_none() {
			return;
		}
		// 카메라. (목표값 지수 접근)
		let camera_approach = (delta_time * 5.0).min(1.0);
		let camera_state = &mut self.camera_state;
		camera_state.yaw += (camera_state.target_yaw - camera_state.yaw) * camera_approach;
		camera_state.pitch += (camera_state.target_pitch - camera_state.pitch) * camera_approach;
		camera_state.distance += (camera_state.target_distance - camera_state.distance) * camera_approach;
		camera_state.focus_height += (camera_state.target_focus_height - camera_state.focus_height) * camera_approach;
		self.update_expressions(delta_time);
		self.apply_body_animation(delta_time);
		self.update_constraints();
		if let Some(resources) = self.resources.as_mut() {
			resources.model.update(delta_time);
			if let Some(head_index) = resources.head_node_index {
				let elements = &resources.model.get_node_list()[head_index].world_matrix.elements;
				self.head_position = [elements[12] as f64, elements[13] as f64, elements[14] as f64];
			}
		}
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 3D 패스 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		graphic.flush_batch();
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		if self.resources.is_some() {
			let gl = graphic.get_gl();
			if let Err(error) = self.render_scene(gl, canvas_pixel_size.x as i32, canvas_pixel_size.y as i32) {
				eprintln!("toonshading: {}", error);
			}
		}
		else {
			let gl = graphic.get_gl();
			unsafe {
				gl.bind_framebuffer(glow::FRAMEBUFFER, None);
				gl.clear_color(0.031, 0.031, 0.039, 1.0);
				gl.clear(glow::COLOR_BUFFER_BIT);
			}
		}
		graphic.restore_render_state();
	}

	//==============================================================================
	// 2D 출력. (제목, 단추, 크레딧, 사이트 링크)
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		engine.get_view_manager().apply_canvas_native_rect(graphic);
		let native_size = engine.get_view_manager().get_canvas_native_size();
		let scale = native_size.x / 1280.0;
		graphic.set_text_baseline(TextBaseline::Middle);
		if self.resources.is_none() {
			graphic.set_fill_color(Color::create(0.95, 0.93, 0.89, 0.6));
			graphic.set_font_string(&format!("{}px sans-serif", 11.0 * scale));
			let message = self.load_error_message.clone().unwrap_or_else(|| String::from("LOADING"));
			ToonShadingScene::draw_spaced_text(graphic, &message, native_size.x * 0.5, native_size.y * 0.5, 6.0 * scale, true);
			return;
		}
		// 제목. (위 7%, 큰 글씨, 가는 줄, 작은 글씨)
		let title_font_size = (native_size.x / scale * 0.05).clamp(26.0, 58.0) * scale;
		let title_center_x = native_size.x * 0.5;
		let title_y = native_size.y * 0.07 + title_font_size * 0.5;
		graphic.set_fill_color(Color::create(0.949, 0.925, 0.886, 1.0));
		graphic.set_font_string(&format!("200 {}px sans-serif", title_font_size));
		ToonShadingScene::draw_spaced_text(graphic, "TOON SHADING", title_center_x, title_y, title_font_size * 0.5, true);
		let divider_y = title_y + title_font_size * 0.5 + 14.0 * scale;
		let divider_width = (240.0 * scale).min(native_size.x * 0.4);
		for step in 0..40 {
			let ratio = step as f64 / 39.0;
			let alpha = (1.0 - (ratio * 2.0 - 1.0).abs()) * 0.7;
			graphic.set_fill_color(Color::create(0.949, 0.925, 0.886, alpha));
			graphic.draw_rect(Rect::create(title_center_x - divider_width * 0.5 + divider_width * ratio, divider_y, divider_width / 40.0 + 0.5, scale.max(1.0)));
		}
		let sub_font_size = (native_size.x / scale * 0.012).clamp(9.0, 12.0) * scale;
		graphic.set_fill_color(Color::create(0.902, 0.886, 0.863, 0.78));
		graphic.set_font_string(&format!("300 {}px sans-serif", sub_font_size));
		ToonShadingScene::draw_spaced_text(graphic, "VANILLA-ENGINE", title_center_x, divider_y + 12.0 * scale + sub_font_size * 0.5, sub_font_size * 0.85, true);

		// 단추.
		let button_font_size = 10.0 * scale;
		let button_list_snapshot = {
			self.layout_buttons(graphic, native_size);
			self.button_list.iter().map(|button| (button.label.clone(), button.rect, button.is_expression, button.name.clone())).collect::<Vec<_>>()
		};
		graphic.set_font_string(&format!("{}px sans-serif", button_font_size));
		for (label, rect, is_expression, name) in button_list_snapshot.iter() {
			let is_active = if *is_expression { self.active_expression == *name } else { self.active_animation == *name };
			if is_active {
				graphic.set_fill_color(Color::create(0.91, 0.788, 0.541, 0.95));
			}
			else {
				graphic.set_fill_color(Color::create(0.055, 0.047, 0.055, 0.5));
			}
			graphic.draw_round_rect(*rect, rect.size.y * 0.5);
			if is_active {
				graphic.set_stroke_color(Color::create(0.91, 0.788, 0.541, 1.0));
			}
			else {
				graphic.set_stroke_color(Color::create(0.949, 0.925, 0.886, 0.18));
			}
			graphic.draw_stroke_round_rect(*rect, rect.size.y * 0.5, scale.max(1.0));
			if is_active {
				graphic.set_fill_color(Color::create(0.165, 0.125, 0.086, 1.0));
			}
			else {
				graphic.set_fill_color(Color::create(0.949, 0.925, 0.886, 0.78));
			}
			ToonShadingScene::draw_spaced_text(graphic, label, rect.position.x + rect.size.x * 0.5, rect.position.y + rect.size.y * 0.5, 1.8 * scale, true);
		}

		// 사이트 링크. (아래 가운데 알약)
		let link_font_size = (native_size.x / scale * 0.012).clamp(9.0, 12.0) * scale;
		graphic.set_font_string(&format!("300 {}px sans-serif", link_font_size));
		let link_spacing = link_font_size * 0.45;
		let link_text = "VANILLAPUNCH.COM";
		let link_width = ToonShadingScene::measure_spaced(graphic, link_text, link_spacing) + 44.0 * scale;
		let link_height = link_font_size * 1.2 + 20.0 * scale;
		let link_rect = Rect::create(title_center_x - link_width * 0.5, native_size.y - native_size.y * 0.055 - link_height, link_width, link_height);
		graphic.set_fill_color(Color::create(0.055, 0.047, 0.055, 0.45));
		graphic.draw_round_rect(link_rect, link_height * 0.5);
		graphic.set_stroke_color(Color::create(0.949, 0.925, 0.886, 0.3));
		graphic.draw_stroke_round_rect(link_rect, link_height * 0.5, scale.max(1.0));
		graphic.set_fill_color(Color::create(0.949, 0.925, 0.886, 0.8));
		ToonShadingScene::draw_spaced_text(graphic, link_text, title_center_x, link_rect.position.y + link_height * 0.5, link_spacing, true);

		// 크레딧. (왼쪽 아래, 저작권 있는 자산)
		let footer_font_size = 9.5 * scale;
		graphic.set_font_string(&format!("{}px sans-serif", footer_font_size));
		graphic.set_fill_color(Color::create(0.949, 0.925, 0.886, 0.42));
		let footer_line_list = [
			"DRAG TO ORBIT, WHEEL TO ZOOM, RIGHT-DRAG TO MOVE THE FOCUS",
			"CHARACTER: VRM1 CONSTRAINT TWIST SAMPLE \u{00A9} 2022 PIXIV INC. (VRM PUBLIC LICENSE 1.0)",
			"ANIMATION: QUATERNIUS UNIVERSAL ANIMATION LIBRARY 1, 2 (CC0)",
		];
		let footer_line_height = footer_font_size * 1.9;
		for (line_index, line) in footer_line_list.iter().enumerate() {
			let y = native_size.y - 16.0 * scale - footer_line_height * (footer_line_list.len() - line_index) as f64 + footer_line_height * 0.5;
			ToonShadingScene::draw_spaced_text(graphic, line, 18.0 * scale, y, footer_font_size * 0.16, false);
		}
	}

	//==============================================================================
	// 누름. (단추 또는 끌기 시작)
	//==============================================================================
	fn touch_press(&mut self, _view_input_position: Vector2) {
		let engine = Engine::get_engine().unwrap();
		let native_position = engine.get_input_manager().get_canvas_native_input_position();
		let mut clicked: Option<(bool, String)> = None;
		for button in self.button_list.iter() {
			let rect = button.rect;
			if native_position.x >= rect.position.x && native_position.x <= rect.position.x + rect.size.x && native_position.y >= rect.position.y && native_position.y <= rect.position.y + rect.size.y {
				clicked = Some((button.is_expression, button.name.clone()));
			}
		}
		match clicked {
			Some((true, name)) => {
				self.select_expression(&name);
			}
			Some((false, name)) => {
				self.select_animation(&name);
			}
			None => {
				self.drag_position = Some(native_position);
				self.is_panning = false;
			}
		}
	}

	//==============================================================================
	// 끌기. (궤도 회전)
	//==============================================================================
	fn touch_move(&mut self, _view_input_position: Vector2) {
		let engine = Engine::get_engine().unwrap();
		let native_position = engine.get_input_manager().get_canvas_native_input_position();
		if let Some(drag_position) = self.drag_position {
			let scale = engine.get_view_manager().get_canvas_native_size().x / 1280.0;
			let delta_x = (native_position.x - drag_position.x) / scale;
			let delta_y = (native_position.y - drag_position.y) / scale;
			if self.is_panning {
				self.camera_state.is_manual_focus = true;
				self.camera_state.target_focus_height = clamp_value(self.camera_state.target_focus_height + delta_y * 0.0022 * self.camera_state.target_distance, FOCUS_HEIGHT_MINIMUM, FOCUS_HEIGHT_MAXIMUM);
			}
			else {
				self.camera_state.target_yaw -= delta_x * 0.006;
				self.camera_state.target_pitch = clamp_value(self.camera_state.target_pitch + delta_y * 0.004, -0.5, 1.1);
			}
			self.drag_position = Some(native_position);
		}
	}

	//==============================================================================
	// 놓음.
	//==============================================================================
	fn touch_release(&mut self, _view_input_position: Vector2) {
		self.drag_position = None;
	}

	//==============================================================================
	// 휠. (줌)
	//==============================================================================
	fn touch_wheel(&mut self, _view_input_position: Vector2, wheel_delta: Vector2) {
		let target_distance = self.camera_state.target_distance * (1.0 + wheel_delta.y * 0.0012);
		self.set_target_distance(target_distance);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("toon shading (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.window_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ToonShadingScene::new())) {
		eprintln!("toonshading: {}", error);
		std::process::exit(1);
	}
}

//==============================================================================
// 셰이더 소스 목록. (원본 examples/toonshading/index.html 의 글 그대로)
//==============================================================================
const SHADING_COMMON_GLSL: &str = r####"
	vec3 srgbToLinear(vec3 color) {
		return pow(max(color, vec3(0.0)), vec3(2.2));
	}

	float sampleShadowFactor(vec4 lightSpace, highp sampler2DShadow shadowMap, float texelSize, float softness, float bias) {
		vec3 projected = lightSpace.xyz / lightSpace.w;
		projected = projected * 0.5 + 0.5;
		if (projected.x < 0.0 || projected.x > 1.0 || projected.y < 0.0 || projected.y > 1.0 || projected.z > 1.0) {
			return 1.0;
		}
		float shadowSum = 0.0;
		for (int offsetY = -1; offsetY <= 1; ++offsetY) {
			for (int offsetX = -1; offsetX <= 1; ++offsetX) {
				vec2 tapOffset = vec2(float(offsetX), float(offsetY)) * texelSize * softness;
				shadowSum += texture(shadowMap, vec3(projected.xy + tapOffset, projected.z - bias));
			}
		}
		return shadowSum / 9.0;
	}
	"####;

const TOON_FRAGMENTSHADER_SOURCE: &str = r####"#version 300 es
	precision highp float;
	in vec3 worldPosition;
	in vec3 worldNormal;
	in vec2 fragmentTextureCoordinate;
	in vec4 lightSpacePosition;
	uniform sampler2D baseColorTexture;
	uniform sampler2D normalTexture;
	uniform sampler2D emissiveTexture;
	uniform vec3 baseColorFactor;
	uniform highp sampler2DShadow shadowMapTexture;
	uniform float shadowStrength;
	uniform float shadowTexelSize;
	uniform float shadowSoftness;
	uniform sampler2D hairDepthTexture;
	uniform sampler2D hairMaskTexture;
	uniform vec2 screenResolution;
	uniform vec2 cameraPlanes;
	uniform vec3 cameraPosition;
	uniform mat4 viewMatrix;
	uniform vec3 keyLightDirection;
	uniform vec3 keyLightColor;
	uniform float keyLightIntensity;
	uniform vec3 ambientSkyColor;
	uniform vec3 ambientGroundColor;
	uniform float shadingToony;
	uniform float shadingShift;
	uniform float shadeStrength;
	uniform float shadeDarkness;
	uniform float warmBandStrength;
	uniform float normalMapStrength;
	uniform vec3 headForward;
	uniform float faceFlatten;
	uniform float faceShadowSoftness;
	uniform int useFaceShadow;
	uniform int useHairShadow;
	uniform vec2 hairShadowOffset;
	uniform float hairShadowStrength;
	uniform int useSpecular;
	uniform float specularPower;
	uniform float specularStrength;
	uniform float specularSoftness;
	uniform int useHairHighlight;
	uniform float hairHighlightWidth;
	uniform float hairHighlightStrength;
	uniform float hairHighlightShift;
	uniform int materialType;
	uniform vec3 shadeColor;
	uniform int useAlphaTest;
	uniform int isSeeThrough;
	uniform float seeThroughAlpha;
	layout(location = 0) out vec4 colorOutput;
	layout(location = 1) out vec4 normalOutput;
	${SHADING_COMMON_GLSL}

	float linearizeDepth(float depth) {
		float near = cameraPlanes.x;
		float far = cameraPlanes.y;
		float ndc = depth * 2.0 - 1.0;
		return 2.0 * near * far / (far + near - ndc * (far - near));
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
		vec4 baseSample = texture(baseColorTexture, fragmentTextureCoordinate);
		if (useAlphaTest == 1 && baseSample.a < 0.5) {
			discard;
		}
		vec2 screenCoordinate = gl_FragCoord.xy / screenResolution;
		float fragmentDepth = linearizeDepth(gl_FragCoord.z);

		// 앞머리 너머 비침. (이 픽셀에 머리카락이 있고 그 깊이가 이 픽셀보다 앞일 때만 — 뒤통수에 가려진 눈은 비치지 않는다)
		if (isSeeThrough == 1) {
			float hairMask = texture(hairMaskTexture, screenCoordinate).r;
			float hairDepth = linearizeDepth(texture(hairDepthTexture, screenCoordinate).r);
			vec3 partNormal = normalize(worldNormal);
			vec3 partViewDirection = normalize(cameraPosition - worldPosition);
			if (hairMask < 0.5 || hairDepth > fragmentDepth - 0.0005 || dot(partNormal, partViewDirection) < 0.2) {
				discard;
			}
		}

		vec3 albedo = srgbToLinear(baseSample.rgb) * baseColorFactor;
		vec3 geometryNormal = normalize(worldNormal);
		if (!gl_FrontFacing) {
			geometryNormal = -geometryNormal;
		}
		vec3 viewDirection = normalize(cameraPosition - worldPosition);
		vec3 surfaceNormal = geometryNormal;
		if (normalMapStrength > 0.0 && (materialType == 1 || materialType == 3)) {
			surfaceNormal = normalize(mix(geometryNormal, applyNormalMap(geometryNormal), normalMapStrength));
		}
		vec3 lightDirection = normalize(keyLightDirection);

		// 램프. (얼굴 / 머리카락 노멀은 로드 때 구면 노멀로 굽어 있다 — SDF 얼굴 그림자처럼 빛 각도에 따라 한 줄로 쓸리는 경계)
		vec3 shadingNormal = surfaceNormal;
		float softness = max(0.5 * (1.0 - shadingToony), 0.004);
		if (materialType == 2) {
			softness = faceShadowSoftness;
		}
		float halfLambert = dot(shadingNormal, lightDirection) * 0.5 + 0.5 + shadingShift;
		float litness = smoothstep(0.5 - softness, 0.5 + softness, halfLambert);
		float deepLitness = smoothstep(0.24 - softness, 0.24 + softness, halfLambert);

		// 섀도우 맵. (부드러운 값도 두 단계로)
		if (shadowStrength > 0.001) {
			float shadowFactor = mix(1.0, sampleShadowFactor(lightSpacePosition, shadowMapTexture, shadowTexelSize, shadowSoftness, 0.0015), shadowStrength);
			litness = min(litness, smoothstep(0.25, 0.75, shadowFactor));
		}

		// 앞머리 그림자. (화면 공간 — 빛 방향으로 밀린 자리에 머리카락이 있고 그것이 이 픽셀보다 크게 뒤가 아니면 그늘. VRoid 앞머리는 피부에 거의 붙어 있어 깊이 차만으로는 못 가른다)
		bool receivesHairShadow = materialType == 1 || materialType == 2 || materialType == 5 || materialType == 6 || materialType == 8 || materialType == 9;
		if (useHairShadow == 1 && receivesHairShadow) {
			vec2 offsetCoordinate = screenCoordinate + hairShadowOffset / screenResolution;
			float hairMask = texture(hairMaskTexture, offsetCoordinate).r;
			float hairDepth = linearizeDepth(texture(hairDepthTexture, offsetCoordinate).r);
			if (hairMask > 0.5 && hairDepth < fragmentDepth + 0.008) {
				litness *= 1.0 - hairShadowStrength;
			}
		}

		// 눈 / 눈썹은 그림자에 덜 잠긴다.
		if (materialType == 5 || materialType == 6) {
			litness = max(litness, 0.62);
		}
		else if (materialType == 8 || materialType == 9) {
			litness = max(litness, 0.45);
		}

		vec3 shadeAlbedo = pow(albedo, vec3(1.18)) * mix(vec3(1.0), shadeColor, shadeStrength) * shadeDarkness;
		// 3 톤. (밝음 / 그늘 / 깊은 그늘 — 얼굴은 2 톤)
		vec3 deepAlbedo = shadeAlbedo * 0.74;
		vec3 shadeBlend = materialType == 2 ? shadeAlbedo : mix(deepAlbedo, shadeAlbedo, deepLitness);
		vec3 diffuse = mix(shadeBlend, albedo, litness);
		if (materialType == 1 || materialType == 2) {
			float band = 1.0 - abs(litness * 2.0 - 1.0);
			diffuse += albedo * vec3(0.95, 0.38, 0.22) * band * warmBandStrength * 0.4;
		}
		vec3 color = diffuse * keyLightColor * keyLightIntensity;
		color += albedo * mix(ambientGroundColor, ambientSkyColor, surfaceNormal.y * 0.5 + 0.5);

		// 계단 스펙큘러. (천 > 머리카락 > 피부)
		if (useSpecular == 1 && (materialType == 1 || materialType == 2 || materialType == 3 || materialType == 4)) {
			vec3 halfVector = normalize(lightDirection + viewDirection);
			float specular = pow(max(dot(surfaceNormal, halfVector), 0.0), specularPower);
			float stepped = smoothstep(0.5 - specularSoftness, 0.5 + specularSoftness, specular);
			float typeScale = materialType == 4 ? 1.0 : (materialType == 3 ? 0.35 : 0.22);
			color += keyLightColor * stepped * specularStrength * typeScale * litness;
		}

		// 머리카락 천사 고리. (구면 노멀의 뷰 공간 높이로 잡는 가로 띠 x VRoid 하이라이트 마스크 — 시점에 따라 위아래로 흐른다)
		if (materialType == 3 && useHairHighlight == 1) {
			vec3 hairViewNormal = normalize(mat3(viewMatrix) * surfaceNormal);
			float ringCenter = 0.58 + hairHighlightShift;
			float ringHalfWidth = 0.03 + hairHighlightWidth * 0.16;
			float ring = 1.0 - smoothstep(ringHalfWidth * 0.6, ringHalfWidth, abs(hairViewNormal.y - ringCenter));
			float lowerRing = (1.0 - smoothstep(ringHalfWidth * 0.3, ringHalfWidth * 0.6, abs(hairViewNormal.y - (ringCenter - 0.34)))) * 0.45;
			float mask = smoothstep(0.12, 0.55, texture(emissiveTexture, fragmentTextureCoordinate).r);
			float highlight = max(ring, lowerRing) * mask * (0.55 + 0.45 * litness);
			color += (albedo * 0.35 + keyLightColor * 0.65) * highlight * hairHighlightStrength;
		}

		// 눈 하이라이트는 빛을 받지 않고 밝게. (블룸이 잡는다)
		if (materialType == 7) {
			color = albedo * 1.7;
		}

		float alpha = isSeeThrough == 1 ? seeThroughAlpha : 1.0;
		colorOutput = vec4(color, alpha);
		vec3 viewNormal = normalize(mat3(viewMatrix) * surfaceNormal);
		normalOutput = vec4(viewNormal * 0.5 + 0.5, float(materialType) / 16.0);
	}
	"####;

const HAIR_MASK_VERTEXSHADER_SOURCE: &str = r####"#version 300 es
	layout(location = 0) in vec3 vertexPosition;
	layout(location = 3) in uvec4 vertexJoints;
	layout(location = 4) in vec4 vertexWeights;
	uniform mat4 modelMatrix;
	uniform mat4 viewProjectionMatrix;
	uniform mat4 jointMatrices[96];
	${MORPH_GLSL}
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
	"####;

const HAIR_MASK_FRAGMENTSHADER_SOURCE: &str = r####"#version 300 es
	precision highp float;
	out vec4 outputColor;
	void main() {
		outputColor = vec4(1.0, 1.0, 1.0, 1.0);
	}
	"####;

const OUTLINE_VERTEXSHADER_SOURCE: &str = r####"#version 300 es
	layout(location = 0) in vec3 vertexPosition;
	layout(location = 1) in vec3 vertexNormal;
	layout(location = 2) in vec2 vertexTextureCoordinate;
	layout(location = 3) in uvec4 vertexJoints;
	layout(location = 4) in vec4 vertexWeights;
	uniform mat4 modelMatrix;
	uniform mat4 viewProjectionMatrix;
	uniform mat4 jointMatrices[96];
	uniform sampler2D outlineMaskTexture;
	uniform int useOutlineMask;
	uniform float outlineWidth;
	uniform vec2 outlinePixelRange;
	uniform vec2 screenResolution;
	uniform float projectionScaleY;
	out vec2 fragmentTextureCoordinate;
	out vec3 worldNormal;
	${MORPH_GLSL}
	void main() {
		vec3 safeNormal = dot(vertexNormal, vertexNormal) < 0.0001 ? vec3(0.0, 1.0, 0.0) : vertexNormal;
		vec3 morphedPosition = vertexPosition;
		vec3 morphedNormal = safeNormal;
		applyMorphTargets(morphedPosition, morphedNormal);
		mat4 skinMatrix = vertexWeights.x * jointMatrices[vertexJoints.x]
			+ vertexWeights.y * jointMatrices[vertexJoints.y]
			+ vertexWeights.z * jointMatrices[vertexJoints.z]
			+ vertexWeights.w * jointMatrices[vertexJoints.w];
		vec4 skinnedPosition = modelMatrix * skinMatrix * vec4(morphedPosition, 1.0);
		worldNormal = normalize(mat3(modelMatrix) * mat3(skinMatrix) * normalize(morphedNormal));
		fragmentTextureCoordinate = vertexTextureCoordinate;
		float mask = useOutlineMask == 1 ? textureLod(outlineMaskTexture, vertexTextureCoordinate, 0.0).r : 1.0;
		vec4 clipPosition = viewProjectionMatrix * skinnedPosition;
		vec3 clipNormal = mat3(viewProjectionMatrix) * worldNormal;
		vec2 screenNormal = normalize(clipNormal.xy + vec2(0.000001, 0.0));
		float pixelsPerMeter = screenResolution.y * projectionScaleY / (2.0 * max(clipPosition.w, 0.001));
		float widthPixels = clamp(outlineWidth * mask * pixelsPerMeter, outlinePixelRange.x * mask, outlinePixelRange.y);
		clipPosition.xy += screenNormal * widthPixels * 2.0 / screenResolution * clipPosition.w;
		gl_Position = clipPosition;
	}
	"####;

const OUTLINE_FRAGMENTSHADER_SOURCE: &str = r####"#version 300 es
	precision highp float;
	in vec2 fragmentTextureCoordinate;
	in vec3 worldNormal;
	uniform sampler2D baseColorTexture;
	uniform vec3 outlineColorFactor;
	uniform float outlineTint;
	uniform float outlineBrightness;
	uniform mat4 viewMatrix;
	layout(location = 0) out vec4 colorOutput;
	layout(location = 1) out vec4 normalOutput;
	${SHADING_COMMON_GLSL}
	void main() {
		vec3 albedo = srgbToLinear(texture(baseColorTexture, fragmentTextureCoordinate).rgb);
		vec3 color = mix(outlineColorFactor, albedo * 0.42, outlineTint) * outlineBrightness;
		colorOutput = vec4(color, 1.0);
		vec3 viewNormal = normalize(mat3(viewMatrix) * normalize(worldNormal));
		normalOutput = vec4(viewNormal * 0.5 + 0.5, 15.0 / 16.0);
	}
	"####;

const FLOOR_VERTEXSHADER_SOURCE: &str = r####"#version 300 es
	layout(location = 0) in vec3 vertexPosition;
	uniform mat4 viewProjectionMatrix;
	uniform mat4 lightViewProjectionMatrix;
	out vec3 worldPosition;
	out vec4 lightSpacePosition;
	void main() {
		worldPosition = vertexPosition;
		lightSpacePosition = lightViewProjectionMatrix * vec4(vertexPosition, 1.0);
		gl_Position = viewProjectionMatrix * vec4(vertexPosition, 1.0);
	}
	"####;

const FLOOR_FRAGMENTSHADER_SOURCE: &str = r####"#version 300 es
	precision highp float;
	in vec3 worldPosition;
	in vec4 lightSpacePosition;
	uniform highp sampler2DShadow shadowMapTexture;
	uniform float shadowTexelSize;
	uniform float shadowStrength;
	uniform float shadowSoftness;
	uniform vec3 backgroundColor;
	uniform vec3 glowColor;
	uniform vec2 screenResolution;
	uniform float aspectRatio;
	uniform vec3 floorColor;
	uniform vec3 ringColor;
	uniform float ringRadius;
	layout(location = 0) out vec4 colorOutput;
	layout(location = 1) out vec4 normalOutput;
	${SHADING_COMMON_GLSL}
	void main() {
		float radial = length(worldPosition.xz);
		vec2 screenCoordinate = gl_FragCoord.xy / screenResolution;
		vec2 centered = (screenCoordinate - vec2(0.5, 0.5)) * vec2(aspectRatio, 1.0);
		vec3 backgroundAtPixel = backgroundColor + glowColor * exp(-dot(centered, centered) * 2.6);
		backgroundAtPixel += backgroundColor * 0.6 * smoothstep(0.2, -0.6, screenCoordinate.y - 0.5);
		float shadowFactor = 1.0;
		if (shadowStrength > 0.001) {
			vec3 projected = lightSpacePosition.xyz / lightSpacePosition.w * 0.5 + 0.5;
			float shadowSum = 0.0;
			for (int offsetY = 0; offsetY <= 1; ++offsetY) {
				for (int offsetX = 0; offsetX <= 1; ++offsetX) {
					vec2 tapOffset = (vec2(float(offsetX), float(offsetY)) - 0.5) * shadowTexelSize * shadowSoftness * 1.5;
					shadowSum += texture(shadowMapTexture, vec3(projected.xy + tapOffset, projected.z - 0.0008));
				}
			}
			shadowFactor = mix(1.0, shadowSum * 0.25, shadowStrength);
			shadowFactor = smoothstep(0.2, 0.8, shadowFactor);
		}
		vec3 color = floorColor * mix(0.42, 1.0, shadowFactor);
		float ring = smoothstep(0.03, 0.0, abs(radial - ringRadius) - 0.004);
		color += ringColor * ring * 0.35;
		color += ringColor * smoothstep(ringRadius, 0.0, radial) * 0.06;
		color = mix(color, backgroundAtPixel, smoothstep(1.2, 3.6, radial));
		colorOutput = vec4(color, 1.0);
		normalOutput = vec4(0.5, 1.0, 0.5, 0.0);
	}
	"####;

const BACKGROUND_FRAGMENTSHADER_SOURCE: &str = r####"#version 300 es
	precision highp float;
	in vec2 fragmentTextureCoordinate;
	uniform float aspectRatio;
	uniform vec3 backgroundColor;
	uniform vec3 glowColor;
	layout(location = 0) out vec4 colorOutput;
	layout(location = 1) out vec4 normalOutput;
	void main() {
		vec2 centered = (fragmentTextureCoordinate - vec2(0.5, 0.5)) * vec2(aspectRatio, 1.0);
		float radial = dot(centered, centered);
		vec3 color = backgroundColor + glowColor * exp(-radial * 2.6);
		color += backgroundColor * 0.6 * smoothstep(0.2, -0.6, fragmentTextureCoordinate.y - 0.5);
		colorOutput = vec4(color, 1.0);
		normalOutput = vec4(0.5, 0.5, 0.5, 0.0);
	}
	"####;

const COMPOSITE_FRAGMENTSHADER_SOURCE: &str = r####"#version 300 es
	precision highp float;
	in vec2 fragmentTextureCoordinate;
	uniform sampler2D sceneTexture;
	uniform sampler2D normalTexture;
	uniform sampler2D depthTexture;
	uniform sampler2D bloomTexture;
	uniform vec2 resolution;
	uniform vec2 cameraPlanes;
	uniform float exposure;
	uniform float bloomStrength;
	uniform float vignetteStrength;
	uniform float saturation;
	uniform float contrast;
	uniform int useRim;
	uniform float rimWidth;
	uniform float rimThreshold;
	uniform float rimStrength;
	uniform vec3 rimColor;
	uniform vec2 rimLightScreenDirection;
	out vec4 outputColor;

	float linearizeDepth(float depth) {
		float near = cameraPlanes.x;
		float far = cameraPlanes.y;
		float ndc = depth * 2.0 - 1.0;
		return 2.0 * near * far / (far + near - ndc * (far - near));
	}

	vec3 acesToneMap(vec3 color) {
		const float a = 2.51;
		const float b = 0.03;
		const float c = 2.43;
		const float d = 0.59;
		const float e = 0.14;
		return clamp((color * (a * color + b)) / (color * (c * color + d) + e), 0.0, 1.0);
	}

	void main() {
		vec3 color = texture(sceneTexture, fragmentTextureCoordinate).rgb;

		// 림. (실루엣 안쪽 픽셀에서 노멀 방향으로 조금 떨어진 자리의 깊이가 크게 멀면 가장자리 — 빛 쪽이 밝다)
		if (useRim == 1) {
			vec4 normalSample = texture(normalTexture, fragmentTextureCoordinate);
			float materialId = normalSample.a * 16.0;
			if (materialId > 0.5 && materialId < 14.5) {
				vec3 viewNormal = normalSample.xyz * 2.0 - 1.0;
				vec2 direction = normalize(viewNormal.xy + vec2(0.0001, 0.0));
				vec2 offsetCoordinate = fragmentTextureCoordinate + direction * rimWidth / resolution;
				float depthHere = linearizeDepth(texture(depthTexture, fragmentTextureCoordinate).r);
				float depthThere = linearizeDepth(texture(depthTexture, offsetCoordinate).r);
				float rim = smoothstep(rimThreshold, rimThreshold * 2.5, depthThere - depthHere);
				float lightFacing = clamp(dot(direction, rimLightScreenDirection) * 0.5 + 0.5, 0.0, 1.0);
				rim *= mix(0.12, 1.0, lightFacing);
				float luminance = dot(color, vec3(0.299, 0.587, 0.114));
				color += rimColor * rim * rimStrength * (0.35 + luminance);
			}
		}

		vec3 bloomColor = texture(bloomTexture, fragmentTextureCoordinate).rgb;
		color = color * exposure + bloomColor * bloomStrength;
		color = acesToneMap(color);

		vec2 centered = fragmentTextureCoordinate - 0.5;
		float vignette = 1.0 - smoothstep(0.35, 0.95, length(centered * vec2(1.0, 0.85)) * 1.25);
		color *= mix(1.0, vignette, vignetteStrength);

		// sRGB 인코딩 뒤 채도 / 대비. (지각 공간에서 중간 회색을 축으로 — 선형 공간에서 하면 어두운 배경이 검게 뭉개진다)
		vec3 encoded = mix(color * 12.92, 1.055 * pow(max(color, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055, step(0.0031308, color));
		float luminance = dot(encoded, vec3(0.299, 0.587, 0.114));
		encoded = mix(vec3(luminance), encoded, saturation);
		encoded = clamp((encoded - 0.5) * contrast + 0.5, 0.0, 1.0);
		outputColor = vec4(encoded, 1.0);
	}
	"####;

const FXAA_FRAGMENTSHADER_SOURCE: &str = r####"#version 300 es
	precision highp float;
	in vec2 fragmentTextureCoordinate;
	uniform sampler2D sourceTexture;
	uniform vec2 texelSize;
	uniform float sharpenStrength;
	out vec4 outputColor;

	float luminance(vec3 color) {
		return dot(color, vec3(0.299, 0.587, 0.114));
	}

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
	}
	"####;

