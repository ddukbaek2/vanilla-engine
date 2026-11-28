//==============================================================================
// 노인 얼굴 예제. (원본 examples/oldface 의 HumanSkinRenderer 장면을 엔진 루프 안에서 돌린다)
// - 원본 애셋(character_lod1.glb, hair_lod2.glb, expressions.json, wrinkles.json, 피부, 눈, 머리카락, 수염 텍스처)을 BlobAsset 으로 읽는다.
//   애셋은 examples/oldface_assets 에 있다. (MetaHuman, Epic Games, 언리얼 엔진 EULA, 데모 전용. 웹은 페이지 기준 상대 주소로 받는다)
// - 원본 기본값 그대로: 머리 LOD 1, 머리카락 HIGH(hair_lod2, 프린지 블렌드, 흔들림 0.8), 2K 그림자 8 표본, 렌더 기준 1080P, FXAA.
// - 패스는 원본과 같다: 섀도우 깊이 → 피부 MRT(배경 + 불투명) → 블렌드(머리카락 프린지, 눈꺼풀 차폐, 눈물선) → AO → SSS
//   → 합성(HDR) → 블룸 → 톤 매핑(LDR) → FXAA(화면) → graphic.restore_render_state() → draw 에서 2D 글.
// - 원본 OPTIONS 패널(토글 9 개, 슬라이더 38 개, 같은 묶음, 차례, 범위, 기본값, 제목을 눌러 접고 펼침)을 엔진 UI(UISlider, UIScrollView, UIButton)로 짓고
//   원본이 바꾸는 렌더러 값을 같게 바꾼다. 숫자 키 1 ~ 9 는 토글 전환(원본과 같음). 표정은 4초마다 차례로 바뀌고 단추로 고르면 자동 전환이 멈춘다.
//   머티리얼 그래프와 RENDERING INFO 는 아직 옮기지 않았다(토글만 있다). 끌기로 궤도 회전, 휠로 줌.
// - 원본 FRAME RATE 기본처럼 30fps 로 제한하고, 갱신 시간은 원본처럼 지난 시간(0.1초 상한)을 쓴다.
//   화면 저장(VANILLA_CAPTURE_PATH)을 켜면 1/30초씩 고정으로 나아간다. (프레임 번호로 같은 장면을 잡도록)
// - 실행: cargo run --release --example oldface
//   화면 저장: VANILLA_CAPTURE_PATH=shots/oldface_{frame}.png VANILLA_CAPTURE_FRAMES=61 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::graphics3d::material::decode_image_bytes;

type GlTexture = <glow::Context as glow::HasContext>::Texture;
use vanilla::{
	AmbientOcclusionEffect, AssetLoadHandle, GlContext, ImageSource, Rect, UIContextMenu, UIContextMenuItem, UIContextMenuOptions, BlobAsset, BloomEffect, Color, Engine, EngineConfiguration, FullscreenPass, Graphic, HumanSkinRenderer, ImageDescription, ImageDescriptionOptions, KeyCode, Material, Matrix4, MouseButton, RenderTarget, RenderTargetOptions, create_bezier_easing, Scene, SceneBase, SeededRandom, ComponentContext, NodeId, NodeTree, Paint, Pivot, TouchRecognizer, UIButton, UILabel, UIScrollView, UISlider, ShadowMap, SkinnedModel, SubsurfaceScatteringEffect, TextAlign, TextBaseline, Vector2, Vector3, SKIN_SHADING_EYE, SKIN_SHADING_FACIALHAIR, SKIN_SHADING_FLUID, SKIN_SHADING_HAIR, SKIN_SHADING_OCCLUSION, SKIN_SHADING_SKIN, SKIN_TEXTURE_UNIT_DETAILHEIGHT, SKIN_TEXTURE_UNIT_LAYERCOLOR, SKIN_TEXTURE_UNIT_LAYERNORMAL, SKIN_TEXTURE_UNIT_MASKATLAS, SKIN_TEXTURE_UNIT_MICRO,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 예제 애셋 폴더. (MetaHuman 자산은 Epic Games 의 것으로 언리얼 엔진 EULA 를 따르고, 이 데모 안에서만 쓴다. 라이선스는 폴더의 ASSETS_LICENSE.md)
const EXAMPLE_ASSET_DIRECTORY: &str = "examples/oldface_assets";

// 화면 위 UI. (원본 index.html 의 CSS 값을 1280x720 화면 기준 픽셀로 옮긴 것)
// 표정 단추 너비. (원본 단추는 글자 사이를 띄운 글자 너비 + 안쪽 여백이라 1280 화면에서 잰 값)
const EXPRESSION_BUTTON_WIDTHS: [f64; 9] = [86.0, 70.0, 60.0, 84.0, 72.0, 86.0, 102.0, 73.0, 98.0];
const EXPRESSION_BUTTON_HEIGHT: f64 = 27.0;
const EXPRESSION_BUTTON_GAP: f64 = 6.0;
const EXPRESSION_ROW_LENGTH: usize = 7;
const FOOTER_LINES: [&str; 3] = [
	"DRAG TO ORBIT, WHEEL OR PINCH TO ZOOM",
	"CHARACTER: EPIC METAHUMAN (WALTER PRESET, EXPRESSION WRINKLE MAPS, RIGLOGIC BAKED TO ARKIT SHAPES, GROOM CARDS), EXPRESSIONS: ARKIT BLENDSHAPE PRESETS",
	"METAHUMAN, EPIC GAMES, DEMO ONLY: METAHUMAN.COM / LICENSE",
];

// 옵션 패널. (원본 optionDefinitions 와 sliderDefinitions 의 차례, 범위, 기본값)
const TOGGLE_LABELS: [&str; 9] = ["SUBSURFACE SCATTERING", "4K PORE DETAIL", "KEY LIGHT SHADOW", "BLOOM", "AMBIENT OCCLUSION", "EXPRESSION WRINKLE MAPS", "WIREFRAME", "MATERIAL GRAPH", "RENDERING INFO"];
const TOGGLE_DEFAULTS: [bool; 9] = [true, true, true, true, true, true, false, false, false];
const TOGGLE_SUBSURFACE: usize = 0;
const TOGGLE_DETAIL: usize = 1;
const TOGGLE_SHADOW: usize = 2;
const TOGGLE_BLOOM: usize = 3;
const TOGGLE_OCCLUSION: usize = 4;
const TOGGLE_WRINKLE: usize = 5;
const TOGGLE_WIREFRAME: usize = 6;
const TOGGLE_GRAPH: usize = 7;
const TOGGLE_HUD: usize = 8;
// 머티리얼 그래프 꼴. (원본 GRAPH_ 상수, 그래픽 설정 패널과 같은 규격)
const GRAPH_PANEL_MARGIN: f64 = 18.0;
const GRAPH_PANEL_WIDTH: f64 = 372.0;
const GRAPH_TITLE_HEIGHT: f64 = 34.0;
const GRAPH_PADDING: f64 = 12.0;
const GRAPH_NODE_PADDING: f64 = 10.0;
const GRAPH_NODE_GAP: f64 = 8.0;
const GRAPH_IMAGE_THUMBNAIL_SIZE: f64 = 72.0;
const GRAPH_BUFFER_THUMBNAIL_WIDTH: f64 = 104.0;
const GRAPH_LINE_HEIGHT: f64 = 20.0;
const GRAPH_TITLE_FONT: &str = "600 10px default";
const GRAPH_TITLE_SPACING: f64 = 3.2;
const GRAPH_NODE_TITLE_FONT: &str = "600 9px default";
const GRAPH_NODE_TITLE_SPACING: f64 = 2.7;
const GRAPH_TEXT_FONT: &str = "400 10.5px default";
const GRAPH_TEXT_SPACING: f64 = 1.26;
const GRAPH_LABEL_FONT: &str = "400 9px default";
const GRAPH_LABEL_SPACING: f64 = 1.6;
// 그래프 썸네일에 쓰는 그림. (이름, 파일. 원본 createThumbnail 의 일곱 장, 96x96 으로 줄인다)
const THUMBNAIL_FILES: [(&str, &str); 7] = [("albedo", "head_albedo.jpg"), ("normal", "head_normal.jpg"), ("detail", "skin_detail.jpg"), ("hair", "hair.png"), ("eye", "eye_iris.png"), ("teeth", "teeth_albedo.jpg"), ("eyelash", "eyelash.png")];
const THUMBNAIL_SIZE: usize = 96;
const CONTEXT_MENU_LABELS: [&str; 3] = ["RESET VIEW", "RESET OPTIONS", "TOGGLE MATERIAL GRAPH"];
const CONTEXT_MENU_ITEM_HEIGHT: f64 = 29.0;
const BLOOM_ITERATION_COUNT: u32 = 3;
const SLIDER_COUNT: usize = 38;
const SLIDER_FRAME_RATE: usize = 0;
const SLIDER_RENDER_BASE: usize = 1;
const SLIDER_RENDER_SCALE: usize = 2;
const SLIDER_ANTIALIAS: usize = 3;
const SLIDER_EXPOSURE: usize = 4;
const SLIDER_BLOOM_STRENGTH: usize = 5;
const SLIDER_BLOOM_THRESHOLD: usize = 6;
const SLIDER_VIGNETTE: usize = 7;
const SLIDER_GRAIN: usize = 8;
const SLIDER_HEAD_LOD: usize = 9;
const SLIDER_HAIR_QUALITY: usize = 10;
const SLIDER_TEXTURE_SIZE: usize = 11;
const SLIDER_SHADOW_QUALITY: usize = 12;
const SLIDER_SHADOW_SOFTNESS: usize = 13;
const SLIDER_SHADOW_STRENGTH: usize = 14;
const SLIDER_NORMAL_STRENGTH: usize = 15;
const SLIDER_PORE_DETAIL: usize = 16;
const SLIDER_CAVITY: usize = 17;
const SLIDER_MICRO_NORMAL: usize = 18;
const SLIDER_WRINKLE_DEPTH: usize = 19;
const SLIDER_SUBSURFACE_WIDTH: usize = 20;
const SLIDER_SPECULAR: usize = 21;
const SLIDER_ROUGHNESS: usize = 22;
const SLIDER_SHEEN: usize = 23;
const SLIDER_OCCLUSION_STRENGTH: usize = 24;
const SLIDER_OCCLUSION_RADIUS: usize = 25;
const SLIDER_KEY_INTENSITY: usize = 26;
const SLIDER_FILL_INTENSITY: usize = 27;
const SLIDER_RIM_INTENSITY: usize = 28;
const SLIDER_AMBIENT_INTENSITY: usize = 29;
const SLIDER_HAIR_SWAY: usize = 30;
const SLIDER_HAIR_SPECULAR: usize = 31;
const SLIDER_HAIR_ROOT_SHADE: usize = 32;
const SLIDER_IRIS_SIZE: usize = 33;
const SLIDER_PUPIL_SIZE: usize = 34;
const SLIDER_IRIS_DEPTH: usize = 35;
const SLIDER_HEAD_FOLLOW: usize = 36;
const SLIDER_BLINK_RATE: usize = 37;

struct SliderDefinition {
	group: &'static str,
	label: &'static str,
	minimum: f64,
	maximum: f64,
	step: f64,
	value: f64,
}

const fn slider(group: &'static str, label: &'static str, minimum: f64, maximum: f64, step: f64, value: f64) -> SliderDefinition {
	SliderDefinition { group, label, minimum, maximum, step, value }
}

const SLIDER_DEFINITIONS: [SliderDefinition; SLIDER_COUNT] = [
	slider("RENDER", "FRAME RATE", 0.0, 1.0, 1.0, 0.0),
	slider("RENDER", "RENDER BASE", 0.0, 3.0, 1.0, 1.0),
	slider("RENDER", "RESOLUTION SCALE", 0.5, 2.0, 0.05, 1.0),
	slider("RENDER", "ANTI-ALIASING", 0.0, 2.0, 1.0, 1.0),
	slider("RENDER", "EXPOSURE", 0.3, 1.5, 0.01, 0.95),
	slider("RENDER", "BLOOM STRENGTH", 0.0, 0.4, 0.01, 0.12),
	slider("RENDER", "BLOOM THRESHOLD", 0.5, 2.5, 0.05, 1.05),
	slider("RENDER", "VIGNETTE", 0.0, 1.0, 0.02, 0.5),
	slider("RENDER", "FILM GRAIN", 0.0, 0.06, 0.002, 0.0),
	slider("GEOMETRY", "HEAD MESH LOD", 0.0, 2.0, 1.0, 1.0),
	slider("GEOMETRY", "HAIR CARDS", 0.0, 5.0, 1.0, 3.0),
	slider("GEOMETRY", "TEXTURE SIZE", 0.0, 3.0, 1.0, 3.0),
	slider("SHADOW", "SHADOW MAP", 0.0, 4.0, 1.0, 1.0),
	slider("SHADOW", "SHADOW SOFTNESS", 0.0, 8.0, 0.25, 3.0),
	slider("SHADOW", "SHADOW STRENGTH", 0.0, 1.0, 0.02, 0.85),
	slider("SKIN", "NORMAL MAP", 0.0, 3.0, 0.05, 1.5),
	slider("SKIN", "PORE DETAIL", 0.0, 8.0, 0.1, 3.5),
	slider("SKIN", "CAVITY", 0.0, 6.0, 0.1, 2.5),
	slider("SKIN", "MICRO NORMAL", 0.0, 1.5, 0.05, 0.55),
	slider("SKIN", "WRINKLE DEPTH", 0.0, 2.0, 0.05, 1.0),
	slider("SKIN", "SSS WIDTH", 0.0, 20.0, 0.5, 8.0),
	slider("SKIN", "SPECULAR", 0.0, 2.0, 0.05, 1.25),
	slider("SKIN", "ROUGHNESS", 0.5, 1.4, 0.02, 0.9),
	slider("SKIN", "PEACH FUZZ SHEEN", 0.0, 0.6, 0.02, 0.12),
	slider("AMBIENT OCCLUSION", "AO STRENGTH", 0.0, 2.0, 0.05, 1.15),
	slider("AMBIENT OCCLUSION", "AO RADIUS", 0.01, 0.08, 0.002, 0.028),
	slider("LIGHTING", "KEY LIGHT", 0.0, 5.0, 0.1, 2.6),
	slider("LIGHTING", "FILL LIGHT", 0.0, 3.0, 0.1, 1.1),
	slider("LIGHTING", "RIM LIGHT", 0.0, 4.0, 0.1, 1.4),
	slider("LIGHTING", "AMBIENT", 0.0, 2.5, 0.05, 1.0),
	slider("HAIR", "HAIR SWAY", 0.0, 1.0, 0.05, 0.25),
	slider("HAIR", "HAIR SPECULAR", 0.0, 1.0, 0.05, 0.5),
	slider("HAIR", "HAIR ROOT SHADE", 0.0, 1.0, 0.05, 0.0),
	slider("EYES", "IRIS SIZE", 0.1, 0.18, 0.002, 0.14),
	slider("EYES", "PUPIL SIZE", 0.015, 0.075, 0.001, 0.042),
	slider("EYES", "IRIS DEPTH", 0.0, 0.1, 0.005, 0.05),
	slider("ANIMATION", "HEAD FOLLOW POINTER", 0.0, 1.0, 0.05, 0.0),
	slider("ANIMATION", "BLINK RATE", 0.0, 2.0, 0.1, 1.0),
];

const FRAME_RATE_OPTIONS: [f64; 2] = [30.0, 60.0];
const RENDER_BASE_OPTIONS: [f64; 4] = [720.0, 1080.0, 1440.0, 2160.0];
const RENDER_BASE_LABELS: [&str; 4] = ["720P", "1080P", "1440P", "4K"];
const HEAD_LOD_FILES: [&str; 3] = ["character_lod0.glb", "character_lod1.glb", "character_lod2.glb"];
// 머리카락 단계. (이름, 파일, 프린지 블렌드, 흔들림 배율)
const HAIR_LEVEL_DEFINITIONS: [(&str, Option<&str>, bool, f64); 6] = [
	("NONE", None, false, 0.0),
	("LOW", Some("hair_lod4.glb"), false, 0.0),
	("MEDIUM", Some("hair_lod3.glb"), false, 0.5),
	("HIGH", Some("hair_lod2.glb"), true, 0.8),
	("ULTRA", Some("hair_lod1.glb"), true, 1.0),
	("CINEMATIC", Some("hair_lod0.glb"), true, 1.0),
];
// 그림자 단계. (이름, 해상도, 표본 수)
const SHADOW_LEVEL_DEFINITIONS: [(&str, i32, i32); 5] = [("1K HARD", 1024, 4), ("2K", 2048, 8), ("4K", 4096, 12), ("4K FINE", 4096, 20), ("8K FINE", 8192, 24)];
const TEXTURE_SIZE_LABELS: [&str; 4] = ["EIGHTH", "QUARTER", "HALF", "FULL"];
const ANTIALIAS_LABELS: [&str; 3] = ["OFF", "FXAA", "FXAA + SHARPEN"];

// 슬라이더 값 글자. (원본 sliderDefinitions 의 format)
fn format_slider(index: usize, value: f64) -> String {
	let level = value.max(0.0) as usize;
	match index {
		SLIDER_FRAME_RATE => format!("{} FPS", FRAME_RATE_OPTIONS[level] as i64),
		SLIDER_RENDER_BASE => RENDER_BASE_LABELS[level].to_string(),
		SLIDER_RENDER_SCALE => format!("{}%", (value * 100.0).round() as i64),
		SLIDER_ANTIALIAS => ANTIALIAS_LABELS[level].to_string(),
		SLIDER_GRAIN => format!("{:.3}", value),
		SLIDER_HEAD_LOD => format!("LOD {}", 2 - level),
		SLIDER_HAIR_QUALITY => HAIR_LEVEL_DEFINITIONS[level].0.to_string(),
		SLIDER_TEXTURE_SIZE => TEXTURE_SIZE_LABELS[level].to_string(),
		SLIDER_SHADOW_QUALITY => SHADOW_LEVEL_DEFINITIONS[level].0.to_string(),
		SLIDER_PORE_DETAIL | SLIDER_CAVITY | SLIDER_KEY_INTENSITY | SLIDER_FILL_INTENSITY | SLIDER_RIM_INTENSITY => format!("{:.1}", value),
		SLIDER_SUBSURFACE_WIDTH => format!("{:.1} MM", value),
		SLIDER_OCCLUSION_RADIUS => format!("{:.1} CM", value * 100.0),
		SLIDER_IRIS_SIZE | SLIDER_PUPIL_SIZE | SLIDER_IRIS_DEPTH => format!("{:.3}", value),
		SLIDER_BLINK_RATE => format!("{:.1}X", value),
		_ => format!("{:.2}", value),
	}
}

// 옵션 값. (원본 optionStates, sliderStates, 패널 접힘. 기본은 접힘)
#[derive(Clone)]
struct OptionValues {
	toggles: [bool; 9],
	sliders: [f64; SLIDER_COUNT],
	is_collapsed: bool,
}

impl OptionValues {
	fn new() -> OptionValues {
		let mut sliders = [0.0; SLIDER_COUNT];
		for (index, definition) in SLIDER_DEFINITIONS.iter().enumerate() {
			sliders[index] = definition.value;
		}
		OptionValues { toggles: TOGGLE_DEFAULTS, sliders, is_collapsed: true }
	}
}

// 그림 줄이기. (원본 createThumbnail 의 96x96 drawImage, 알파를 곱해 평균 낸 뒤 되돌린다)
fn downscale_pixels(width: usize, height: usize, pixels: &[u8]) -> Vec<u8> {
	let mut result = vec![0u8; THUMBNAIL_SIZE * THUMBNAIL_SIZE * 4];
	for target_y in 0..THUMBNAIL_SIZE {
		let source_y_start = target_y * height / THUMBNAIL_SIZE;
		let source_y_end = (((target_y + 1) * height + THUMBNAIL_SIZE - 1) / THUMBNAIL_SIZE).max(source_y_start + 1).min(height);
		for target_x in 0..THUMBNAIL_SIZE {
			let source_x_start = target_x * width / THUMBNAIL_SIZE;
			let source_x_end = (((target_x + 1) * width + THUMBNAIL_SIZE - 1) / THUMBNAIL_SIZE).max(source_x_start + 1).min(width);
			let mut sum = [0.0f64; 4];
			for source_y in source_y_start..source_y_end {
				for source_x in source_x_start..source_x_end {
					let index = (source_y * width + source_x) * 4;
					let alpha = pixels[index + 3] as f64;
					sum[0] += pixels[index] as f64 * alpha;
					sum[1] += pixels[index + 1] as f64 * alpha;
					sum[2] += pixels[index + 2] as f64 * alpha;
					sum[3] += alpha;
				}
			}
			let count = ((source_y_end - source_y_start) * (source_x_end - source_x_start)) as f64;
			let target_index = (target_y * THUMBNAIL_SIZE + target_x) * 4;
			if sum[3] > 0.0 {
				result[target_index] = (sum[0] / sum[3]).round() as u8;
				result[target_index + 1] = (sum[1] / sum[3]).round() as u8;
				result[target_index + 2] = (sum[2] / sum[3]).round() as u8;
			}
			result[target_index + 3] = (sum[3] / count).round() as u8;
		}
	}
	result
}

// 천 단위 쉼표. (원본 toLocaleString("en-US"))
fn format_with_commas(value: u64) -> String {
	let digits = value.to_string();
	let mut result = String::new();
	for (index, character) in digits.chars().enumerate() {
		if index > 0 && (digits.len() - index) % 3 == 0 {
			result.push(',');
		}
		result.push(character);
	}
	result
}

// 메시 바이트. (원본 estimateGpuMemoryBytes 의 정점, 인덱스 배열 합, 값 하나를 JS 배열 칸 크기로 셈)
fn mesh_array_bytes(model: &SkinnedModel) -> f64 {
	let mut bytes = 0.0;
	for drawable in model.get_drawable_list().iter() {
		let mesh = &drawable.mesh_description;
		bytes += mesh.positions.len() as f64 * 4.0;
		bytes += mesh.normals.as_ref().map(|values| values.len()).unwrap_or(0) as f64 * 4.0;
		bytes += mesh.texture_coordinates.as_ref().map(|values| values.len()).unwrap_or(0) as f64 * 4.0;
		bytes += mesh.joints.len() as f64 * 2.0;
		bytes += mesh.weights.len() as f64 * 4.0;
		bytes += match &mesh.indices {
			Some(vanilla::graphics3d::skinnedmodel::IndexArray::UnsignedShort(values)) => values.len() as f64 * 2.0,
			Some(vanilla::graphics3d::skinnedmodel::IndexArray::UnsignedInt(values)) => values.len() as f64 * 4.0,
			None => 0.0,
		};
	}
	bytes
}

// 활성 모델. (머리, 머리카락 차례. 캐시에서 이름으로 찾는다)
fn collect_active<'a>(model_cache: &'a mut Vec<(String, SkinnedModel)>, head_file: &str, hair_file: Option<&str>) -> Vec<&'a mut SkinnedModel> {
	let mut head_model = None;
	let mut hair_model = None;
	for (name, model) in model_cache.iter_mut() {
		if name == head_file {
			head_model = Some(model);
		}
		else if Some(name.as_str()) == hair_file {
			hair_model = Some(model);
		}
	}
	let mut model_list = Vec::new();
	model_list.extend(head_model);
	model_list.extend(hair_model);
	model_list
}

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
// 버퍼 미리보기. (머티리얼 그래프 패널, 중간 버퍼를 작은 뷰포트에 톤 매핑해 표시)
const PREVIEW_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform float displayScale;
uniform int useCoverage;
out vec4 outputColor;

void main() {
	vec4 sourceSample = texture(sourceTexture, fragmentTextureCoordinate);
	vec3 color = sourceSample.rgb * displayScale;
	color = color / (1.0 + color);
	if (useCoverage == 1) {
		color = mix(vec3(0.05, 0.05, 0.06), color, sourceSample.a);
	}
	outputColor = vec4(pow(color, vec3(1.0 / 2.2)), 1.0);
}";

// 그림 썸네일. (원본은 2D 캔버스 drawImage, 줄인 그림을 위 행부터 그대로 알파 합성)
const THUMBNAIL_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform float alphaScale;
out vec4 outputColor;

void main() {
	vec4 sourceSample = texture(sourceTexture, vec2(fragmentTextureCoordinate.x, 1.0 - fragmentTextureCoordinate.y));
	outputColor = vec4(sourceSample.rgb * sourceSample.a, sourceSample.a) * alphaScale;
}";

// 패스별 GPU 시간. (원본 EXT_disjoint_timer_query_webgl2, 결과는 몇 프레임 뒤에 도착하므로 대기 목록을 매 프레임 확인)
struct PassTimer {
	is_supported: bool,
	pending_list: Vec<(&'static str, glow::Query)>,
	current: Option<(&'static str, glow::Query)>,
	milliseconds: Vec<(&'static str, f64)>,
}

impl PassTimer {
	fn new(gl: &GlContext) -> PassTimer {
		let is_supported = gl.supported_extensions().iter().any(|extension_name| extension_name.contains("timer_query"));
		PassTimer { is_supported, pending_list: Vec::new(), current: None, milliseconds: Vec::new() }
	}

	fn begin_pass(&mut self, gl: &GlContext, pass_name: &'static str) {
		if !self.is_supported || self.current.is_some() {
			return;
		}
		unsafe {
			if let Ok(query) = gl.create_query() {
				gl.begin_query(glow::TIME_ELAPSED, query);
				self.current = Some((pass_name, query));
			}
		}
	}

	fn end_pass(&mut self, gl: &GlContext) {
		if let Some(current) = self.current.take() {
			unsafe {
				gl.end_query(glow::TIME_ELAPSED);
			}
			self.pending_list.push(current);
		}
	}

	fn poll(&mut self, gl: &GlContext) {
		if !self.is_supported {
			return;
		}
		let mut remaining_list = Vec::new();
		for (pass_name, query) in self.pending_list.drain(..) {
			let is_available = unsafe {
				gl.get_query_parameter_u32(query, glow::QUERY_RESULT_AVAILABLE) != 0
			};
			if !is_available {
				remaining_list.push((pass_name, query));
				continue;
			}
			let nanoseconds = unsafe {
				gl.get_query_parameter_u32(query, glow::QUERY_RESULT)
			};
			let measured = nanoseconds as f64 / 1e6;
			match self.milliseconds.iter_mut().find(|(name, _)| *name == pass_name) {
				Some(entry) => {
					entry.1 = entry.1 * 0.9 + measured * 0.1;
				}
				None => {
					self.milliseconds.push((pass_name, measured));
				}
			}
			unsafe {
				gl.delete_query(query);
			}
		}
		self.pending_list = remaining_list;
	}

	fn format(&self, pass_name: &str) -> String {
		match self.milliseconds.iter().find(|(name, _)| *name == pass_name) {
			Some((_, milliseconds)) => {
				format!("{:.2} ms", milliseconds)
			}
			None => {
				String::from("n/a")
			}
		}
	}
}

// 렌더링 정보 값. (원본 statistics)
struct RenderStatistics {
	smoothed_frame_seconds: f64,
	draw_call_count: u32,
	hud_top: f64,
	cpu_frame_milliseconds: f64,
	memory_sample_instant: Option<web_time::Instant>,
	gpu_memory_bytes: f64,
	gpu_memory_text: String,
	triangle_count: f64,
	vertex_count: usize,
	morph_target_count: usize,
	scene_draw_call_count: u32,
}

// 머티리얼 그래프 스크롤과 자리. (원본 materialGraphState)
struct GraphState {
	scroll_offset: f64,
	maximum_scroll: f64,
	left: f64,
	top: f64,
	right: f64,
	bottom: f64,
	is_dragging: bool,
	last_pointer_y: f64,
}

struct GraphThumbnail {
	label: &'static str,
	image_index: Option<usize>,
	buffer_index: Option<usize>,
	display_scale: f64,
	use_coverage: i32,
	x: f64,
	y: f64,
	width: f64,
	height: f64,
}

struct GraphNode {
	title: &'static str,
	is_active: bool,
	lines: Vec<String>,
	thumbnail_list: Vec<GraphThumbnail>,
	x: f64,
	y: f64,
	width: f64,
	height: f64,
	text_y: f64,
}

// 모델 비동기 읽기. (원본 loadModelFile().then, 다 읽히면 해석해 캐시에 넣고 그때까지 이전 모델을 유지)
struct PendingModel {
	file_name: String,
	blob_asset: BlobAsset,
	load_handle: AssetLoadHandle,
	is_head: bool,
	value: f64,
}

struct FaceResources {
	skin_renderer: HumanSkinRenderer,
	model_cache: Vec<(String, SkinnedModel)>,
	head_file: String,
	hair_file: Option<String>,
	material_list: Vec<(&'static str, Rc<Material>)>,
	shadow_resolution: i32,
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
	preview_pass: FullscreenPass,
	thumbnail_pass: FullscreenPass,
	thumbnail_texture_list: Vec<GlTexture>,
	image_byte_total: f64,
	retained_image_byte_total: f64,
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
	touch_recognizer: TouchRecognizer,
	ui: Option<OldFaceUi>,
	pending_expression_index: Rc<Cell<Option<usize>>>,
	option_values: Rc<RefCell<OptionValues>>,
	applied_sliders: [f64; SLIDER_COUNT],
	pointer_position: Option<Vector2>,
	has_pointer_moved: bool,
	intro_start: web_time::Instant,
	statistics: RenderStatistics,
	graph: GraphState,
	pass_timer: PassTimer,
	preview_texture_list: [Option<GlTexture>; 5],
	pending_model_list: Vec<PendingModel>,
	pending_head_value: Option<f64>,
	pending_hair_value: Option<f64>,
	image_record_list: RefCell<Vec<(String, u32, u32, usize)>>,
	thumbnail_pixel_list: RefCell<Vec<(String, Vec<u8>)>>,
	pending_menu_action: Rc<Cell<Option<usize>>>,
	frame_start_instant: web_time::Instant,
	last_tick_instant: Option<web_time::Instant>,
	menu_test_frame: u32,
}

//==============================================================================
// 화면 위 UI 노드 모음. (원본 index.html 의 titleOverlay, optionPanel, expressionPanel, footerNote, homeLink)
//==============================================================================
struct OldFaceUi {
	title_id: NodeId,
	divider_id: NodeId,
	subtitle_id: NodeId,
	options_id: NodeId,
	options_fill_id: NodeId,
	options_title_id: NodeId,
	options_hit_id: NodeId,
	options_divider_id: NodeId,
	options_scroll_id: NodeId,
	options_content_height: f64,
	toggle_state_ids: Vec<NodeId>,
	slider_value_ids: Vec<NodeId>,
	value_texts: Vec<String>,
	laid_out_panel: (bool, f64, f64),
	expression_button_ids: Vec<NodeId>,
	expression_fill_ids: Vec<NodeId>,
	home_id: NodeId,
	footer_ids: Vec<NodeId>,
	slider_ids: Vec<NodeId>,
	menu_id: NodeId,
}

fn create_color(red: f64, green: f64, blue: f64, alpha: f64) -> Color {
	Color::create(red / 255.0, green / 255.0, blue / 255.0, alpha)
}

fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, size: Vector2) -> NodeId {
	let node_id = node_tree.create_child(parent_id, name);
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_content_size(size);
	node_id
}

fn create_label_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, size: Vector2, text: &str, font_size: f64, color: Color, text_align: TextAlign) -> NodeId {
	let label_id = create_box_node(node_tree, parent_id, name, size);
	let label = node_tree.add_component(label_id, UILabel::create_with_text(text, font_size)).expect("label");
	label.set_text_color(color);
	label.set_text_align(text_align);
	label.set_text_baseline(TextBaseline::Middle);
	label_id
}

// 테두리 있는 둥근 단추. (바깥 Paint 가 테두리 색, 안쪽 자식 Paint 가 바탕색. 바탕 노드를 돌려준다)
fn create_pill_button(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, size: Vector2, text: &str, font_size: f64, text_color: Color, border_color: Color, fill_color: Color, round_size: f64) -> (NodeId, NodeId) {
	let button_id = create_box_node(node_tree, parent_id, name, size);
	let border_paint = node_tree[button_id].add_component(Paint::create_with_color(border_color));
	border_paint.set_round_size(round_size);
	let fill_size = Vector2::create(size.x - 2.0, size.y - 2.0);
	let fill_id = create_box_node(node_tree, button_id, "fill", fill_size);
	node_tree[fill_id].set_local_position(Vector2::create(1.0, 1.0));
	let fill_paint = node_tree[fill_id].add_component(Paint::create_with_color(fill_color));
	fill_paint.set_round_size(round_size - 1.0);
	let label_id = create_label_node(node_tree, fill_id, "text", fill_size, text, font_size, text_color, TextAlign::Center);
	let _ = label_id;
	node_tree.add_component(button_id, UIButton::new());
	(button_id, fill_id)
}

impl OldFaceScene {
	//==============================================================================
	// UI 만들기. (원본 CSS 의 자리, 크기, 색을 따라 노드로 놓는다)
	//==============================================================================
	fn create_ui(&mut self) {
		let pending_expression_index = self.pending_expression_index.clone();
		let node_tree = self.scene_base.get_node_tree_mut();
		let root_id = node_tree.get_root_id();
		let light = create_color(242.0, 236.0, 226.0, 0.95);
		let title_id = create_label_node(node_tree, root_id, "title", Vector2::create(900.0, 80.0), "O\u{2002}L\u{2002}D\u{2002}\u{2002}\u{2002}F\u{2002}A\u{2002}C\u{2002}E", 58.0, create_color(242.0, 236.0, 226.0, 1.0), TextAlign::Center);
		let divider_id = create_box_node(node_tree, root_id, "divider", Vector2::create(240.0, 1.0));
		node_tree[divider_id].add_component(Paint::create_with_color(create_color(242.0, 236.0, 226.0, 0.55)));
		let subtitle_id = create_label_node(node_tree, root_id, "subtitle", Vector2::create(600.0, 20.0), "V A N I L L A   E N G I N E", 12.0, create_color(230.0, 226.0, 220.0, 0.78), TextAlign::Center);
		let mut expression_button_ids = Vec::new();
		let mut expression_fill_ids = Vec::new();
		for (expression_index, definition) in EXPRESSION_DEFINITIONS.iter().enumerate() {
			let size = Vector2::create(EXPRESSION_BUTTON_WIDTHS[expression_index], EXPRESSION_BUTTON_HEIGHT);
			let (button_id, fill_id) = create_pill_button(node_tree, root_id, definition.0, size, definition.1, 10.0, create_color(242.0, 236.0, 226.0, 0.85), create_color(70.0, 67.0, 68.0, 1.0), create_color(30.0, 28.0, 30.0, 0.85), size.y * 0.5);
			let pending = pending_expression_index.clone();
			if let Some(button) = node_tree[button_id].get_component_mut::<UIButton>() {
				button.set_click_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
					pending.set(Some(expression_index));
				}));
			}
			expression_button_ids.push(button_id);
			expression_fill_ids.push(fill_id);
		}
		let (home_id, _) = create_pill_button(node_tree, root_id, "home", Vector2::create(244.0, 34.0), "VANILLAPUNCH.COM", 12.0, light, create_color(80.0, 77.0, 76.0, 1.0), create_color(78.0, 72.0, 66.0, 0.75), 17.0);
		let mut footer_ids = Vec::new();
		for (line_index, line) in FOOTER_LINES.iter().enumerate() {
			footer_ids.push(create_label_node(node_tree, root_id, &format!("footer{}", line_index), Vector2::create(1200.0, 18.0), line, 9.5, create_color(242.0, 236.0, 226.0, 0.5), TextAlign::Left));
		}
		// 옵션 패널은 맨 뒤에 만든다. (원본 z-index 5, 표정 단추 위에 그려진다)
		let option_values = self.option_values.clone();
		let gold = create_color(232.0, 201.0, 138.0, 0.95);
		let panel_text = create_color(242.0, 236.0, 226.0, 0.86);
		let options_id = create_box_node(node_tree, root_id, "options", Vector2::create(122.0, 34.0));
		node_tree[options_id].add_component(Paint::create_with_color(create_color(60.0, 57.0, 59.0, 1.0))).set_round_size(12.0);
		let options_fill_id = create_box_node(node_tree, options_id, "fill", Vector2::create(120.0, 32.0));
		node_tree[options_fill_id].set_local_position(Vector2::create(1.0, 1.0));
		node_tree[options_fill_id].add_component(Paint::create_with_color(create_color(30.0, 28.0, 30.0, 0.9))).set_round_size(11.0);
		let options_title_id = create_label_node(node_tree, options_fill_id, "text", Vector2::create(120.0, 32.0), "OPTIONS", 10.0, gold, TextAlign::Center);
		let options_divider_id = create_box_node(node_tree, options_fill_id, "titleDivider", Vector2::create(370.0, 1.0));
		node_tree[options_divider_id].set_local_position(Vector2::create(0.0, 33.0));
		node_tree[options_divider_id].add_component(Paint::create_with_color(create_color(242.0, 236.0, 226.0, 0.1)));
		node_tree[options_divider_id].set_visible(false);
		let options_hit_id = create_box_node(node_tree, options_id, "titleHit", Vector2::create(122.0, 34.0));
		node_tree.add_component(options_hit_id, UIButton::new());
		{
			let option_values = option_values.clone();
			if let Some(button) = node_tree[options_hit_id].get_component_mut::<UIButton>() {
				button.set_click_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
					let mut values = option_values.borrow_mut();
					values.is_collapsed = !values.is_collapsed;
				}));
			}
		}
		let options_scroll_id = create_box_node(node_tree, options_id, "scroll", Vector2::create(370.0, 100.0));
		node_tree[options_scroll_id].set_local_position(Vector2::create(1.0, 35.0));
		let content_id = {
			let scroll_view = node_tree.add_component(options_scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(Color::create(0.0, 0.0, 0.0, 0.0));
			scroll_view.set_horizontal(false);
			scroll_view.get_content().expect("content")
		};
		let mut content_y = 0.0;
		let mut toggle_state_ids = Vec::new();
		for (toggle_index, label) in TOGGLE_LABELS.iter().enumerate() {
			let row_id = create_box_node(node_tree, content_id, "toggleRow", Vector2::create(370.0, 28.0));
			node_tree[row_id].set_local_position(Vector2::create(0.0, content_y));
			let key_id = create_box_node(node_tree, row_id, "key", Vector2::create(18.0, 16.0));
			node_tree[key_id].set_local_position(Vector2::create(12.0, 6.0));
			node_tree[key_id].add_component(Paint::create_with_color(create_color(242.0, 236.0, 226.0, 0.22))).set_round_size(4.0);
			let key_fill_id = create_box_node(node_tree, key_id, "keyFill", Vector2::create(16.0, 14.0));
			node_tree[key_fill_id].set_local_position(Vector2::create(1.0, 1.0));
			node_tree[key_fill_id].add_component(Paint::create_with_color(create_color(28.0, 26.0, 28.0, 1.0))).set_round_size(3.0);
			create_label_node(node_tree, key_fill_id, "digit", Vector2::create(16.0, 14.0), &format!("{}", toggle_index + 1), 9.0, create_color(242.0, 236.0, 226.0, 0.6), TextAlign::Center);
			let label_id = create_label_node(node_tree, row_id, "label", Vector2::create(240.0, 28.0), label, 10.5, panel_text, TextAlign::Left);
			node_tree[label_id].set_local_position(Vector2::create(40.0, 0.0));
			let state_id = create_label_node(node_tree, row_id, "state", Vector2::create(60.0, 28.0), "", 9.5, create_color(242.0, 236.0, 226.0, 0.4), TextAlign::Right);
			node_tree[state_id].set_local_position(Vector2::create(296.0, 0.0));
			toggle_state_ids.push(state_id);
			node_tree.add_component(row_id, UIButton::new());
			let option_values = option_values.clone();
			if let Some(button) = node_tree[row_id].get_component_mut::<UIButton>() {
				button.set_click_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
					let mut values = option_values.borrow_mut();
					values.toggles[toggle_index] = !values.toggles[toggle_index];
				}));
			}
			content_y += 28.0;
		}
		let mut slider_value_ids = Vec::new();
		let mut slider_ids = Vec::new();
		let mut previous_group = "";
		for (slider_index, definition) in SLIDER_DEFINITIONS.iter().enumerate() {
			if definition.group != previous_group {
				previous_group = definition.group;
				let group_id = create_box_node(node_tree, content_id, "group", Vector2::create(370.0, 25.0));
				node_tree[group_id].set_local_position(Vector2::create(0.0, content_y));
				let line_id = create_box_node(node_tree, group_id, "line", Vector2::create(370.0, 1.0));
				node_tree[line_id].add_component(Paint::create_with_color(create_color(242.0, 236.0, 226.0, 0.08)));
				let group_label_id = create_label_node(node_tree, group_id, "groupLabel", Vector2::create(340.0, 12.0), definition.group, 9.0, create_color(232.0, 201.0, 138.0, 0.7), TextAlign::Left);
				node_tree[group_label_id].set_local_position(Vector2::create(12.0, 9.0));
				content_y += 25.0;
			}
			let row_id = create_box_node(node_tree, content_id, "sliderRow", Vector2::create(370.0, 26.0));
			node_tree[row_id].set_local_position(Vector2::create(0.0, content_y));
			let label_id = create_label_node(node_tree, row_id, "label", Vector2::create(118.0, 26.0), definition.label, 9.5, panel_text, TextAlign::Left);
			node_tree[label_id].set_local_position(Vector2::create(12.0, 0.0));
			let slider_id = create_box_node(node_tree, row_id, "slider", Vector2::create(132.0, 12.0));
			node_tree[slider_id].set_local_position(Vector2::create(134.0, 7.0));
			slider_ids.push(slider_id);
			// 손잡이. (원본 ::-webkit-slider-thumb 지름 10, 금색 바탕 위에 어두운 테두리 1px rgba(20, 16, 12, 0.8))
			let thumb_id = create_box_node(node_tree, slider_id, "thumb", Vector2::create(10.0, 10.0));
			node_tree[thumb_id].add_component(Paint::create_with_color(create_color(212.0, 176.0, 106.0, 1.0))).set_round_size(5.0);
			let thumb_border_id = create_box_node(node_tree, thumb_id, "thumbBorder", Vector2::create(10.0, 10.0));
			node_tree[thumb_border_id].add_component(Paint::create_with_color(create_color(20.0, 16.0, 12.0, 0.8))).set_round_size(5.0);
			let thumb_fill_id = create_box_node(node_tree, thumb_border_id, "thumbFill", Vector2::create(8.0, 8.0));
			node_tree[thumb_fill_id].set_local_position(Vector2::create(1.0, 1.0));
			node_tree[thumb_fill_id].add_component(Paint::create_with_color(create_color(212.0, 176.0, 106.0, 1.0))).set_round_size(4.0);
			{
				let slider = node_tree.add_component(slider_id, UISlider::new()).expect("slider");
				slider.set_range(definition.minimum, definition.maximum);
				slider.set_value(definition.value);
				slider.set_track_thickness(2.0);
				slider.set_thumb_radius(5.0);
				slider.set_thumb_node(Some(thumb_id));
				slider.set_track_color(create_color(242.0, 236.0, 226.0, 0.18));
				slider.set_fill_color(create_color(242.0, 236.0, 226.0, 0.18));
				slider.set_thumb_color(create_color(212.0, 176.0, 106.0, 1.0));
				let option_values = option_values.clone();
				let step = definition.step;
				let minimum = definition.minimum;
				let maximum = definition.maximum;
				slider.set_value_changed_event(Box::new(move |slider: &mut UISlider, _context: &mut ComponentContext| {
					let snapped = (minimum + ((slider.get_value() - minimum) / step).round() * step).clamp(minimum, maximum);
					if (snapped - slider.get_value()).abs() > 1e-9 {
						slider.set_value(snapped);
					}
					option_values.borrow_mut().sliders[slider_index] = snapped;
				}));
			}
			let value_id = create_label_node(node_tree, row_id, "value", Vector2::create(82.0, 26.0), "", 9.0, create_color(232.0, 201.0, 138.0, 0.9), TextAlign::Right);
			node_tree[value_id].set_local_position(Vector2::create(276.0, 0.0));
			slider_value_ids.push(value_id);
			content_y += 26.0;
		}
		let options_content_height = content_y;
		if let Some(scroll_view) = node_tree[options_scroll_id].get_component_mut::<UIScrollView>() {
			scroll_view.set_scroll_content_size(Vector2::create(370.0, options_content_height));
		}
		node_tree[options_scroll_id].set_visible(false);
		// 우클릭 메뉴. (원본 #contextMenu, 맨 위에 둔다)
		let menu_id = UIContextMenu::create(node_tree, root_id, UIContextMenuOptions { menu_width: 182.0, item_height: CONTEXT_MENU_ITEM_HEIGHT, font_size: 10.0, panel_color: create_color(14.0, 12.0, 14.0, 0.72), text_color: create_color(242.0, 236.0, 226.0, 0.86) });
		self.ui = Some(OldFaceUi { title_id, divider_id, subtitle_id, options_id, options_fill_id, options_title_id, options_hit_id, options_divider_id, options_scroll_id, options_content_height, toggle_state_ids, slider_value_ids, value_texts: vec![String::new(); SLIDER_COUNT + 9], laid_out_panel: (true, 0.0, 0.0), expression_button_ids, expression_fill_ids, home_id, footer_ids, slider_ids, menu_id });
		self.refresh_expression_buttons();
	}

	//==============================================================================
	// UI 자리 잡기. (화면 크기가 바뀌어도 원본처럼 가운데, 아래, 오른쪽 위 기준)
	//==============================================================================
	fn layout_ui(&mut self, canvas_size: Vector2) {
		let is_collapsed = self.option_values.borrow().is_collapsed;
		let hud_top = if self.statistics.hud_top > 0.0 { self.statistics.hud_top } else { canvas_size.y - 18.0 };
		let ui = match &mut self.ui {
			Some(ui) => ui,
			None => {
				return;
			}
		};
		let node_tree = self.scene_base.get_node_tree_mut();
		let center_x = canvas_size.x * 0.5;
		let title_font = (canvas_size.x * 0.05).clamp(26.0, 58.0);
		let sub_font = (canvas_size.x * 0.012).clamp(9.0, 12.0);
		node_tree[ui.title_id].set_local_position(Vector2::create(center_x - 450.0, canvas_size.y * 0.07 - 5.0));
		if let Some(label) = node_tree[ui.title_id].get_component_mut::<UILabel>() {
			label.set_font_size(title_font);
		}
		node_tree[ui.divider_id].set_local_position(Vector2::create(center_x - 120.0, canvas_size.y * 0.07 - 5.0 + 80.0 + 8.0));
		node_tree[ui.subtitle_id].set_local_position(Vector2::create(center_x - 300.0, canvas_size.y * 0.07 - 5.0 + 80.0 + 18.0));
		if let Some(label) = node_tree[ui.subtitle_id].get_component_mut::<UILabel>() {
			label.set_font_size(sub_font);
		}
		// 옵션 패널. (접힘은 제목 알약, 펼침은 폭 372 에 높이는 화면 높이 - 48 까지(원본 updatePanelLimits). 폭 720 이하는 폭 316, 여백 10)
		let is_narrow = canvas_size.x <= 720.0;
		let panel_width = if is_collapsed { 122.0 } else if is_narrow { 318.0 } else { 374.0 };
		let panel_margin = if is_narrow { 10.0 } else { 18.0 };
		let panel_height = if is_collapsed { 34.0 } else { (34.0 + 1.0 + ui.options_content_height + 1.0).min((hud_top - 30.0).max(120.0) + 2.0) };
		node_tree[ui.options_id].set_local_position(Vector2::create(canvas_size.x - panel_margin - panel_width, panel_margin));
		if ui.laid_out_panel != (is_collapsed, panel_width, panel_height) {
			ui.laid_out_panel = (is_collapsed, panel_width, panel_height);
			node_tree[ui.options_id].set_content_size(Vector2::create(panel_width, panel_height));
			node_tree[ui.options_fill_id].set_content_size(Vector2::create(panel_width - 2.0, panel_height - 2.0));
			node_tree[ui.options_hit_id].set_content_size(Vector2::create(panel_width, 34.0));
			node_tree[ui.options_title_id].set_content_size(Vector2::create(panel_width - 2.0, 32.0));
			node_tree[ui.options_title_id].set_local_position(Vector2::create(if is_collapsed { 0.0 } else { 15.0 }, 0.0));
			if let Some(label) = node_tree[ui.options_title_id].get_component_mut::<UILabel>() {
				label.set_text_align(if is_collapsed { TextAlign::Center } else { TextAlign::Left });
			}
			node_tree[ui.options_divider_id].set_content_size(Vector2::create(panel_width - 2.0, 1.0));
			node_tree[ui.options_divider_id].set_visible(!is_collapsed);
			node_tree[ui.options_scroll_id].set_visible(!is_collapsed);
			node_tree[ui.options_scroll_id].set_content_size(Vector2::create(panel_width - 2.0, (panel_height - 36.0).max(0.0)));
			if let Some(paint) = node_tree[ui.options_id].get_component_mut::<Paint>() {
				paint.set_color(if is_collapsed { create_color(60.0, 57.0, 59.0, 1.0) } else { create_color(242.0, 236.0, 226.0, 0.14) });
			}
			if let Some(paint) = node_tree[ui.options_fill_id].get_component_mut::<Paint>() {
				paint.set_color(if is_collapsed { create_color(30.0, 28.0, 30.0, 0.9) } else { create_color(14.0, 12.0, 14.0, 0.5) });
			}
		}
		// 표정 단추 두 줄. (가운데 맞춤, 아래 쪽 줄의 아래 끝이 화면 아래에서 5.5% + 56px)
		let bottom_y = canvas_size.y - (canvas_size.y * 0.055 + 56.0);
		let row_ranges = [(0, EXPRESSION_ROW_LENGTH), (EXPRESSION_ROW_LENGTH, EXPRESSION_BUTTON_WIDTHS.len())];
		for (row_index, (start, end)) in row_ranges.iter().enumerate() {
			let row_width: f64 = EXPRESSION_BUTTON_WIDTHS[*start..*end].iter().sum::<f64>() + EXPRESSION_BUTTON_GAP * ((end - start - 1) as f64);
			let mut x = center_x - row_width * 0.5;
			let y = bottom_y - EXPRESSION_BUTTON_HEIGHT - (1 - row_index) as f64 * (EXPRESSION_BUTTON_HEIGHT + EXPRESSION_BUTTON_GAP);
			for button_index in *start..*end {
				node_tree[ui.expression_button_ids[button_index]].set_local_position(Vector2::create(x, y));
				x += EXPRESSION_BUTTON_WIDTHS[button_index] + EXPRESSION_BUTTON_GAP;
			}
		}
		node_tree[ui.home_id].set_local_position(Vector2::create(center_x - 122.0, canvas_size.y - canvas_size.y * 0.055 - 34.0));
		for (line_index, footer_id) in ui.footer_ids.iter().enumerate() {
			node_tree[*footer_id].set_local_position(Vector2::create(18.0, canvas_size.y - 16.0 - 18.0 * (FOOTER_LINES.len() - line_index) as f64));
		}
	}

	//==============================================================================
	// 등장 애니메이션. (원본 CSS: 제목, 알약, 사이트 단추는 3.2초 titleEntrance(불투명 0 에서 1, 위 14 에서 제자리, 블러는 못 함),
	// 표정 단추, 옵션 패널, 아래 안내는 2.4초 panelEntrance(불투명). 곡선은 cubic-bezier(0.16, 1, 0.3, 1). 시작은 화면을 만든 때)
	//==============================================================================
	fn apply_intro(&mut self) {
		let ease = create_bezier_easing(0.16, 1.0, 0.3, 1.0);
		let seconds = self.intro_start.elapsed().as_secs_f64();
		let title_progress = ease((seconds / 3.2).min(1.0));
		let panel_progress = ease((seconds / 2.4).min(1.0));
		let ui = match &self.ui {
			Some(ui) => {
				ui
			}
			None => {
				return;
			}
		};
		let node_tree = self.scene_base.get_node_tree_mut();
		for node_id in [ui.title_id, ui.divider_id, ui.subtitle_id, ui.home_id] {
			let position = node_tree[node_id].get_local_position();
			node_tree[node_id].set_local_position(Vector2::create(position.x, position.y - 14.0 * (1.0 - title_progress)));
			node_tree[node_id].set_local_opacity(title_progress);
		}
		let mut panel_id_list = ui.expression_button_ids.clone();
		panel_id_list.push(ui.options_id);
		panel_id_list.extend(ui.footer_ids.iter().copied());
		for node_id in panel_id_list {
			node_tree[node_id].set_local_opacity(panel_progress);
		}
	}

	//==============================================================================
	// 옵션 변경 처리. (프레임 제한, 머리 LOD, 머리카락 단계, 텍스처 크기, 그림자 단계는 바뀐 때만. 모델은 필요할 때 읽어 캐시)
	//==============================================================================
	fn apply_option_changes(&mut self, engine: &Engine) {
		let values = self.option_values.borrow().clone();
		let applied = self.applied_sliders;
		self.applied_sliders = values.sliders;
		if values.sliders[SLIDER_HEAD_LOD] != applied[SLIDER_HEAD_LOD] {
			let value = values.sliders[SLIDER_HEAD_LOD];
			self.request_model(engine, HEAD_LOD_FILES[2 - value as usize], true, value);
		}
		if values.sliders[SLIDER_HAIR_QUALITY] != applied[SLIDER_HAIR_QUALITY] {
			let value = values.sliders[SLIDER_HAIR_QUALITY];
			match HAIR_LEVEL_DEFINITIONS[value as usize].1 {
				Some(file_name) => {
					self.request_model(engine, file_name, false, value);
				}
				None => {
					self.pending_hair_value = Some(value);
					if let Some(resources) = &mut self.resources {
						resources.hair_file = None;
					}
				}
			}
		}
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		let graphic = match engine.get_graphic() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let gl = graphic.get_gl();
		if values.sliders[SLIDER_FRAME_RATE] != applied[SLIDER_FRAME_RATE] {
			engine.set_maximum_frame_per_second(FRAME_RATE_OPTIONS[values.sliders[SLIDER_FRAME_RATE] as usize]);
		}
		if values.sliders[SLIDER_TEXTURE_SIZE] != applied[SLIDER_TEXTURE_SIZE] {
			// 모든 머티리얼 텍스처의 최소 밉 레벨을 올려 실효 해상도를 낮춘다.
			let level = (3.0 - values.sliders[SLIDER_TEXTURE_SIZE]) as f32;
			for (_keyword, material) in resources.material_list.iter() {
				for (_name, _unit, texture) in material.get_texture_bindings() {
					if let Some(texture) = texture {
						unsafe {
							gl.bind_texture(glow::TEXTURE_2D, Some(texture));
							gl.tex_parameter_f32(glow::TEXTURE_2D, glow::TEXTURE_MIN_LOD, level);
						}
					}
				}
			}
			unsafe {
				gl.bind_texture(glow::TEXTURE_2D, None);
			}
		}
		if values.sliders[SLIDER_SHADOW_QUALITY] != applied[SLIDER_SHADOW_QUALITY] {
			let shadow_level = &SHADOW_LEVEL_DEFINITIONS[values.sliders[SLIDER_SHADOW_QUALITY] as usize];
			if shadow_level.1 != resources.shadow_resolution {
				match ShadowMap::new(gl, shadow_level.1) {
					Ok(shadow_map) => {
						resources.shadow_map = shadow_map;
						resources.shadow_resolution = shadow_level.1;
					}
					Err(error) => {
						eprintln!("oldface: {}", error);
					}
				}
			}
			resources.skin_renderer.set_shadow_sample_count(shadow_level.2);
		}
	}

	//==============================================================================
	// 옵션 글자 고치기. (토글 ON / OFF, 슬라이더 값 글자. 바뀐 것만)
	//==============================================================================
	fn refresh_option_labels(&mut self) {
		let values = self.option_values.borrow().clone();
		let ui = match &mut self.ui {
			Some(ui) => {
				ui
			}
			None => {
				return;
			}
		};
		let node_tree = self.scene_base.get_node_tree_mut();
		for toggle_index in 0..9 {
			let text = if values.toggles[toggle_index] { "ON" } else { "OFF" };
			if ui.value_texts[SLIDER_COUNT + toggle_index] != text {
				ui.value_texts[SLIDER_COUNT + toggle_index] = text.to_string();
				if let Some(label) = node_tree[ui.toggle_state_ids[toggle_index]].get_component_mut::<UILabel>() {
					label.set_text(text);
					label.set_text_color(if values.toggles[toggle_index] { create_color(232.0, 201.0, 138.0, 1.0) } else { create_color(242.0, 236.0, 226.0, 0.4) });
				}
			}
		}
		for slider_index in 0..SLIDER_COUNT {
			let text = format_slider(slider_index, values.sliders[slider_index]);
			if ui.value_texts[slider_index] != text {
				if let Some(label) = node_tree[ui.slider_value_ids[slider_index]].get_component_mut::<UILabel>() {
					label.set_text(&text);
				}
				ui.value_texts[slider_index] = text;
			}
		}
	}

	//==============================================================================
	// 고른 표정 단추를 노란 바탕으로. (원본 .expressionButton.active)
	//==============================================================================
	fn refresh_expression_buttons(&mut self) {
		let active_index = self.active_expression_index;
		let ui = match &self.ui {
			Some(ui) => ui,
			None => {
				return;
			}
		};
		let node_tree = self.scene_base.get_node_tree_mut();
		for (expression_index, fill_id) in ui.expression_fill_ids.iter().enumerate() {
			let is_active = expression_index == active_index;
			if let Some(paint) = node_tree[*fill_id].get_component_mut::<Paint>() {
				paint.set_color(if is_active { create_color(232.0, 201.0, 138.0, 0.95) } else { create_color(30.0, 28.0, 30.0, 0.85) });
			}
			let text_id = node_tree[*fill_id].get_child(0).expect("text");
			if let Some(label) = node_tree[text_id].get_component_mut::<UILabel>() {
				label.set_text_color(if is_active { create_color(42.0, 32.0, 22.0, 1.0) } else { create_color(242.0, 236.0, 226.0, 0.85) });
			}
		}
	}
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
			touch_recognizer: TouchRecognizer::new(),
			ui: None,
			pending_expression_index: Rc::new(Cell::new(None)),
			option_values: Rc::new(RefCell::new({
				// 확인용: 환경 변수 VANILLA_OLDFACE_OPEN=1 이면 옵션 패널을 펼친 채로 시작한다.
				let mut values = OptionValues::new();
				values.is_collapsed = std::env::var("VANILLA_OLDFACE_OPEN").map(|value| value != "1").unwrap_or(true);
				// 확인용: VANILLA_OLDFACE_SET="s9=0,s10=5,t6=1" 이면 슬라이더 9 번을 0 으로, 토글 6 번을 켠다. (번호는 옵션 표의 차례, 0 부터)
				if let Ok(text) = std::env::var("VANILLA_OLDFACE_SET") {
					for item in text.split(',') {
						if let Some((key, value)) = item.split_once('=') {
							let index = key[1..].parse::<usize>().unwrap_or(usize::MAX);
							let number = value.parse::<f64>().unwrap_or(0.0);
							if key.starts_with('s') && index < SLIDER_COUNT {
								values.sliders[index] = number;
							}
							else if key.starts_with('t') && index < 9 {
								values.toggles[index] = number != 0.0;
							}
						}
					}
				}
				values
			})),
			applied_sliders: OptionValues::new().sliders,
			pointer_position: None,
			has_pointer_moved: false,
			intro_start: web_time::Instant::now(),
			statistics: RenderStatistics { smoothed_frame_seconds: 1.0 / 60.0, draw_call_count: 0, hud_top: 0.0, cpu_frame_milliseconds: 0.0, memory_sample_instant: None, gpu_memory_bytes: 0.0, gpu_memory_text: String::new(), triangle_count: 0.0, vertex_count: 0, morph_target_count: 0, scene_draw_call_count: 0 },
			graph: GraphState { scroll_offset: 0.0, maximum_scroll: 0.0, left: 0.0, top: 0.0, right: 0.0, bottom: 0.0, is_dragging: false, last_pointer_y: 0.0 },
			pass_timer: PassTimer { is_supported: false, pending_list: Vec::new(), current: None, milliseconds: Vec::new() },
			preview_texture_list: [None; 5],
			pending_model_list: Vec::new(),
			pending_head_value: None,
			pending_hair_value: None,
			image_record_list: RefCell::new(Vec::new()),
			thumbnail_pixel_list: RefCell::new(Vec::new()),
			pending_menu_action: Rc::new(Cell::new(None)),
			frame_start_instant: web_time::Instant::now(),
			last_tick_instant: None,
			menu_test_frame: 0,
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
		// 한 번 풀어 크기를 적어 둔다. (GPU 메모리 셈과 그래프 썸네일에 쓰고, 푼 픽셀은 그대로 텍스처가 된다)
		let (width, height, pixels) = decode_image_bytes(bytes.as_ref())?;
		if !self.image_record_list.borrow().iter().any(|(name, _, _, _)| name == file_name) {
			self.image_record_list.borrow_mut().push((file_name.to_string(), width, height, bytes.len()));
		}
		if THUMBNAIL_FILES.iter().any(|(_, thumbnail_file)| *thumbnail_file == file_name) && !self.thumbnail_pixel_list.borrow().iter().any(|(name, _)| name == file_name) {
			self.thumbnail_pixel_list.borrow_mut().push((file_name.to_string(), downscale_pixels(width as usize, height as usize, &pixels)));
		}
		let mime_type = if file_name.ends_with(".png") {
			String::from("image/png")
		}
		else {
			String::from("image/jpeg")
		};
		let options = ImageDescriptionOptions { flip_y: false, is_color_data: false, anisotropy: 8.0 };
		let mut description = Material::create_image_description(bytes.as_ref().clone(), Some(mime_type), Some(options));
		description.source = Some(Rc::new(ImageSource { width, height, pixels }));
		Ok(Some(description))
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
		let preview_pass = FullscreenPass::new(gl, PREVIEW_FRAGMENTSHADER_SOURCE)?;
		let thumbnail_pass = FullscreenPass::new(gl, THUMBNAIL_FRAGMENTSHADER_SOURCE)?;

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
		// 그래프 썸네일 텍스처와 메모리 셈 바탕. (원본 imageDescriptionCache 의 크기 합, 읽은 그림 바이트 합)
		let mut thumbnail_texture_list = Vec::new();
		for (_name, file_name) in THUMBNAIL_FILES.iter() {
			let pixels = self.thumbnail_pixel_list.borrow().iter().find(|(name, _)| name == file_name).map(|(_, pixels)| pixels.clone()).ok_or_else(|| format!("{} 썸네일 없음", file_name))?;
			let mut description = Material::create_image_description(Vec::new(), None, None);
			description.source = Some(Rc::new(ImageSource { width: THUMBNAIL_SIZE as u32, height: THUMBNAIL_SIZE as u32, pixels }));
			thumbnail_texture_list.push(Material::create_texture(gl, Some(&description), [0, 0, 0, 0])?);
		}
		let image_byte_total: f64 = self.image_record_list.borrow().iter().map(|(_, width, height, _)| *width as f64 * *height as f64 * 4.0 * 1.3333).sum();
		let retained_image_byte_total: f64 = self.image_record_list.borrow().iter().map(|(_, _, _, byte_count)| *byte_count as f64).sum();
		let mut resources = FaceResources {
			skin_renderer,
			model_cache: vec![(HEAD_MODEL_FILE.to_string(), head_model), (HAIR_MODEL_FILE.to_string(), hair_model)],
			head_file: HEAD_MODEL_FILE.to_string(),
			hair_file: Some(HAIR_MODEL_FILE.to_string()),
			material_list: material_by_keyword,
			shadow_resolution: SHADOWMAP_RESOLUTION,
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
			preview_pass,
			thumbnail_pass,
			thumbnail_texture_list,
			image_byte_total,
			retained_image_byte_total,
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
			for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
				model.set_morph_weight(unit_name, next_weight);
			}
		}

		// 깜빡임. (눈 감기 단위, 표정이 감은 만큼보다 크게 덮어쓴다, 간격은 원본 BLINK RATE 기본 1)
		if elapsed_time >= self.next_blink_time {
			self.blink_start_time = elapsed_time;
			let blink_rate = self.option_values.borrow().sliders[SLIDER_BLINK_RATE];
			self.next_blink_time = elapsed_time + if blink_rate > 0.001 { (2.8 + self.random.next_value() as f64 * 3.6) / blink_rate } else { 1e9 };
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
		for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
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
				let mut values = self.option_values.borrow_mut();
				values.toggles[key_index] = !values.toggles[key_index];
			}
		}
		if let Some(pending_index) = self.pending_expression_index.take() {
			selected_index = Some(pending_index);
		}
		let current_pointer_position = input_manager.get_canvas_native_input_position();
		if self.pointer_position.map(|position| position.x != current_pointer_position.x || position.y != current_pointer_position.y).unwrap_or(current_pointer_position.x != 0.0 || current_pointer_position.y != 0.0) {
			self.has_pointer_moved = true;
		}
		self.pointer_position = Some(current_pointer_position);
		// 머티리얼 그래프 위 끌기와 휠은 그래프를 스크롤한다. (원본 pointerdown / wheel)
		if input_manager.is_mouse_button_just_pressed(MouseButton::Left) && self.is_pointer_on_graph(current_pointer_position) {
			self.graph.is_dragging = true;
			self.graph.last_pointer_y = current_pointer_position.y;
		}
		if self.graph.is_dragging {
			if input_manager.is_mouse_button_down(MouseButton::Left) {
				self.graph.scroll_offset = (self.graph.scroll_offset - (current_pointer_position.y - self.graph.last_pointer_y)).clamp(0.0, self.graph.maximum_scroll);
				self.graph.last_pointer_y = current_pointer_position.y;
			}
			else {
				self.graph.is_dragging = false;
			}
		}
		// 우클릭 메뉴. (옵션 패널과 표정 패널 위에서는 열지 않는다)
		if input_manager.is_mouse_button_just_pressed(MouseButton::Right) && !self.is_pointer_over_panel(current_pointer_position) {
			self.open_context_menu(current_pointer_position);
		}
		if input_manager.is_mouse_button_down(MouseButton::Left) && !self.graph.is_dragging && !self.is_pointer_over_ui(engine) && !self.is_context_menu_showing() {
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
		if wheel_delta_y != 0.0 && self.is_pointer_on_graph(current_pointer_position) {
			self.graph.scroll_offset = (self.graph.scroll_offset + wheel_delta_y).clamp(0.0, self.graph.maximum_scroll);
		}
		else if wheel_delta_y != 0.0 && !self.is_pointer_over_ui(engine) {
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
	fn is_pointer_over_ui(&self, engine: &Engine) -> bool {
		let ui = match &self.ui {
			Some(ui) => ui,
			None => {
				return false;
			}
		};
		let pointer_position = engine.get_input_manager().get_canvas_native_input_position();
		let node_tree = self.scene_base.get_node_tree();
		let mut button_id_list = ui.expression_button_ids.clone();
		button_id_list.push(ui.options_id);
		button_id_list.push(ui.home_id);
		button_id_list.iter().any(|button_id| {
			let position = node_tree[*button_id].get_local_position();
			let size = node_tree[*button_id].get_content_size();
			pointer_position.x >= position.x && pointer_position.x <= position.x + size.x && pointer_position.y >= position.y && pointer_position.y <= position.y + size.y
		})
	}

	fn change_expression(&mut self, expression_index: usize) {
		self.active_expression_index = expression_index;
		self.refresh_expression_buttons();
		if let Some(resources) = &mut self.resources {
			select_expression(resources, EXPRESSION_DEFINITIONS[expression_index].0);
		}
	}

	//==============================================================================
	// 렌더러 파라미터. (원본 renderFrame 이 슬라이더 기본값으로 프레임마다 넣는 값)
	//==============================================================================
	fn apply_render_parameters(resources: &mut FaceResources, elapsed_time: f64, values: &OptionValues) {
		let slider = &values.sliders;
		let skin_renderer = &mut resources.skin_renderer;
		skin_renderer.set_detail_strength(if values.toggles[TOGGLE_DETAIL] { slider[SLIDER_PORE_DETAIL] } else { 0.0 });
		skin_renderer.set_cavity_strength(if values.toggles[TOGGLE_DETAIL] { slider[SLIDER_CAVITY] } else { 0.0 });
		skin_renderer.set_micro_detail(38.0, if values.toggles[TOGGLE_DETAIL] { slider[SLIDER_MICRO_NORMAL] } else { 0.0 });
		skin_renderer.set_normal_strength(slider[SLIDER_NORMAL_STRENGTH]);
		skin_renderer.set_wrinkle_maps(values.toggles[TOGGLE_WRINKLE], slider[SLIDER_WRINKLE_DEPTH], (slider[SLIDER_WRINKLE_DEPTH] * 0.45).min(1.0));
		skin_renderer.set_specular_strength(slider[SLIDER_SPECULAR]);
		skin_renderer.set_roughness_range(slider[SLIDER_ROUGHNESS] * 0.85, slider[SLIDER_ROUGHNESS] * 1.1);
		skin_renderer.set_sheen_strength(slider[SLIDER_SHEEN]);
		skin_renderer.set_shadow_softness(slider[SLIDER_SHADOW_SOFTNESS]);
		let key_light_direction = Vector3::create(KEY_LIGHT_DIRECTION[0], KEY_LIGHT_DIRECTION[1], KEY_LIGHT_DIRECTION[2]).normalize();
		let fill_light_direction = Vector3::create(FILL_LIGHT_DIRECTION[0], FILL_LIGHT_DIRECTION[1], FILL_LIGHT_DIRECTION[2]).normalize();
		let rim_light_direction = Vector3::create(RIM_LIGHT_DIRECTION[0], RIM_LIGHT_DIRECTION[1], RIM_LIGHT_DIRECTION[2]).normalize();
		skin_renderer.set_light(0, key_light_direction.x, key_light_direction.y, key_light_direction.z, 1.0, 0.92, 0.82, slider[SLIDER_KEY_INTENSITY]);
		skin_renderer.set_light(1, fill_light_direction.x, fill_light_direction.y, fill_light_direction.z, 0.6, 0.7, 0.9, slider[SLIDER_FILL_INTENSITY]);
		skin_renderer.set_light(2, rim_light_direction.x, rim_light_direction.y, rim_light_direction.z, 0.95, 0.97, 1.0, slider[SLIDER_RIM_INTENSITY]);
		let ambient_scale = slider[SLIDER_AMBIENT_INTENSITY];
		skin_renderer.set_ambient_colors([0.2 * ambient_scale, 0.21 * ambient_scale, 0.25 * ambient_scale], [0.13 * ambient_scale, 0.12 * ambient_scale, 0.12 * ambient_scale], [0.09 * ambient_scale, 0.08 * ambient_scale, 0.075 * ambient_scale]);
		let hair_level = &HAIR_LEVEL_DEFINITIONS[slider[SLIDER_HAIR_QUALITY] as usize];
		skin_renderer.set_hair_fringe(hair_level.2);
		skin_renderer.set_hair_sway(slider[SLIDER_HAIR_SWAY] * hair_level.3 * 0.012, elapsed_time, 1.6);
		skin_renderer.set_hair_parameters([-0.1, 0.16], [160.0, 24.0], [0.24 * slider[SLIDER_HAIR_SPECULAR], 0.52 * slider[SLIDER_HAIR_SPECULAR], 0.2], slider[SLIDER_HAIR_ROOT_SHADE]);
		skin_renderer.set_eye_parameters(slider[SLIDER_IRIS_SIZE], slider[SLIDER_PUPIL_SIZE], slider[SLIDER_IRIS_DEPTH]);
		resources.subsurface_effect.set_scatter_width(slider[SLIDER_SUBSURFACE_WIDTH] * 0.001);
		resources.ambient_occlusion_effect.set_parameters(slider[SLIDER_OCCLUSION_RADIUS], slider[SLIDER_OCCLUSION_STRENGTH], 0.0015);
		resources.bloom_effect.set_threshold(slider[SLIDER_BLOOM_THRESHOLD], slider[SLIDER_BLOOM_THRESHOLD] + 0.85);
	}

	//==============================================================================
	// 3D 그리기. (원본 renderFrame 의 1 ~ 7 단계, 2D 상태는 호출자가 되돌린다)
	//==============================================================================
	fn render_scene(&mut self, gl: &GlContext, canvas_width: i32, canvas_height: i32) -> Result<(), String> {
		let elapsed_time = self.elapsed_time;
		let values = self.option_values.borrow().clone();
		let slider = values.sliders;
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return Ok(());
			}
		};

		// 내부 렌더 해상도. (RENDER BASE 의 픽셀 수를 캔버스 종횡비로, GPU 상한으로 자른다)
		let render_base_height = RENDER_BASE_OPTIONS[slider[SLIDER_RENDER_BASE] as usize];
		let render_pixel_budget = render_base_height * (16.0 / 9.0) * render_base_height;
		let pixel_budget_scale = (render_pixel_budget / ((canvas_width * canvas_height).max(1) as f64)).sqrt() * slider[SLIDER_RENDER_SCALE];
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

		let mut draw_call_count: u32 = 0;
		// 1. 섀도우 깊이 패스. (키 라이트)
		self.pass_timer.begin_pass(gl, "shadow");
		unsafe {
			gl.enable(glow::CULL_FACE);
			gl.cull_face(glow::BACK);
			gl.disable(glow::BLEND);
		}
		let key_light_direction = Vector3::create(KEY_LIGHT_DIRECTION[0], KEY_LIGHT_DIRECTION[1], KEY_LIGHT_DIRECTION[2]).normalize();
		resources.shadow_map.update_light_matrix(resources.shadow_focus_position, key_light_direction.multiply_scalar(-1.0), 0.45, 0.2, 4.0);
		let light_view_projection_elements = *resources.shadow_map.get_light_view_projection_matrix().get_elements();
		if values.toggles[TOGGLE_SHADOW] {
			resources.shadow_map.begin_render(gl);
			for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
				resources.skin_renderer.draw_depth(gl, model, &light_view_projection_elements, &model_elements);
				draw_call_count += model.get_drawable_list().len() as u32;
			}
		}
		self.pass_timer.end_pass(gl);
		let shadow_depth_texture = resources.shadow_map.get_depth_texture();
		resources.skin_renderer.set_shadow_map(Some(shadow_depth_texture), Some(&light_view_projection_elements), 1.0 / resources.shadow_resolution as f64);
		resources.skin_renderer.set_shadow_strength(if values.toggles[TOGGLE_SHADOW] { slider[SLIDER_SHADOW_STRENGTH] } else { 0.0 });

		// 2. 피부 MRT 패스. (배경 → 조도 / 알베도 / 스펙큘러 / 노멀 + 깊이)
		self.pass_timer.begin_pass(gl, "scene");
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
		draw_call_count += 1;
		for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
			resources.skin_renderer.draw(gl, model, &view_projection_elements, &model_elements, camera_x, camera_y, camera_z);
			draw_call_count += model.get_drawable_list().len() as u32;
		}

		// 2b. 블렌드 패스. (머리카락 프린지 / 눈꺼풀 차폐 / 눈물선 / 침, 깊이 쓰기 없이 어태치먼트별 알파 블렌드)
		unsafe {
			gl.enable(glow::BLEND);
			gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
			gl.depth_mask(false);
			gl.disable(glow::CULL_FACE);
		}
		for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
			resources.skin_renderer.draw_blended(gl, model, &view_projection_elements, &model_elements, camera_x, camera_y, camera_z);
		}
		unsafe {
			gl.enable(glow::CULL_FACE);
			gl.depth_mask(true);
			gl.disable(glow::BLEND);
		}
		draw_call_count += 6;
		self.pass_timer.end_pass(gl);

		// 3. 화면 공간 앰비언트 오클루전 / 서브서피스 스캐터링.
		let irradiance_texture = resources.scene_render_target.get_color_texture(0);
		let albedo_texture = resources.scene_render_target.get_color_texture(1);
		let specular_texture = resources.scene_render_target.get_color_texture(2);
		let normal_texture = resources.scene_render_target.get_color_texture(3);
		let scene_depth_texture = resources.scene_render_target.get_depth_texture();
		self.pass_timer.begin_pass(gl, "scatter");
		let mut occlusion_result_texture = albedo_texture;
		if values.toggles[TOGGLE_OCCLUSION] {
			resources.ambient_occlusion_effect.render(gl, scene_depth_texture, normal_texture, view_matrix.get_elements(), CAMERA_FIELD_OF_VIEW, aspect_ratio, CAMERA_NEAR_DISTANCE, CAMERA_FAR_DISTANCE);
			occlusion_result_texture = resources.ambient_occlusion_effect.get_result_texture();
			draw_call_count += 3;
		}
		let mut scattered_irradiance_texture = irradiance_texture;
		if values.toggles[TOGGLE_SUBSURFACE] {
			resources.subsurface_effect.render(gl, irradiance_texture, scene_depth_texture, CAMERA_FIELD_OF_VIEW, aspect_ratio, CAMERA_NEAR_DISTANCE, CAMERA_FAR_DISTANCE);
			scattered_irradiance_texture = resources.subsurface_effect.get_result_texture();
			draw_call_count += 2;
		}
		self.pass_timer.end_pass(gl);

		// 4. 합성 → HDR. (조도 x 알베도 + 스펙큘러 + 배경)
		self.pass_timer.begin_pass(gl, "post");
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
			gl.uniform_1_f32(combine_pass.get_uniform_location(gl, "occlusionStrength").as_ref(), if values.toggles[TOGGLE_OCCLUSION] { (slider[SLIDER_OCCLUSION_STRENGTH] as f32).min(1.0) } else { 0.0 });
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
		draw_call_count += 1;
		unsafe {
			for texture_unit in 1..4 {
				gl.active_texture(glow::TEXTURE0 + texture_unit);
				gl.bind_texture(glow::TEXTURE_2D, None);
			}
			gl.active_texture(glow::TEXTURE0);
		}

		// 5. 블룸.
		let hdr_color_texture = resources.hdr_render_target.get_color_texture(0);
		if values.toggles[TOGGLE_BLOOM] {
			resources.bloom_effect.render(gl, hdr_color_texture);
			draw_call_count += 1 + BLOOM_ITERATION_COUNT * 2;
		}

		// 6. 톤 매핑 합성 → LDR(FXAA 켜짐) 또는 화면.
		let use_antialias = slider[SLIDER_ANTIALIAS] > 0.0;
		if use_antialias {
			resources.ldr_render_target.bind(gl);
		}
		else {
			unsafe {
				gl.bind_framebuffer(glow::FRAMEBUFFER, None);
				gl.viewport(0, 0, canvas_width, canvas_height);
			}
		}
		let composite_pass = &resources.composite_pass;
		composite_pass.use_program(gl);
		unsafe {
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "exposure").as_ref(), slider[SLIDER_EXPOSURE] as f32);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), if values.toggles[TOGGLE_BLOOM] { slider[SLIDER_BLOOM_STRENGTH] as f32 } else { 0.0 });
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "vignetteStrength").as_ref(), slider[SLIDER_VIGNETTE] as f32);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "grainStrength").as_ref(), slider[SLIDER_GRAIN] as f32);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
			gl.uniform_2_f32(composite_pass.get_uniform_location(gl, "resolution").as_ref(), render_width as f32, render_height as f32);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, hdr_color_texture);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, if values.toggles[TOGGLE_BLOOM] { resources.bloom_effect.get_result_texture() } else { hdr_color_texture });
			gl.active_texture(glow::TEXTURE0);
		}
		composite_pass.draw(gl);
		draw_call_count += 1;

		// 7. FXAA(와 샤픈) → 화면.
		if use_antialias {
			unsafe {
				gl.bind_framebuffer(glow::FRAMEBUFFER, None);
				gl.viewport(0, 0, canvas_width, canvas_height);
			}
			let antialias_pass = &resources.antialias_pass;
			antialias_pass.use_program(gl);
			unsafe {
				gl.uniform_1_i32(antialias_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
				gl.uniform_2_f32(antialias_pass.get_uniform_location(gl, "texelSize").as_ref(), 1.0 / render_width as f32, 1.0 / render_height as f32);
				gl.uniform_1_f32(antialias_pass.get_uniform_location(gl, "sharpenStrength").as_ref(), if slider[SLIDER_ANTIALIAS] >= 2.0 { 0.5 } else { 0.0 });
				gl.active_texture(glow::TEXTURE1);
				gl.bind_texture(glow::TEXTURE_2D, None);
				gl.active_texture(glow::TEXTURE0);
				gl.bind_texture(glow::TEXTURE_2D, resources.ldr_render_target.get_color_texture(0));
			}
			antialias_pass.draw(gl);
			draw_call_count += 1;
		}
		self.pass_timer.end_pass(gl);

		// 8. 와이어프레임. (화면 깊이 프리패스로 앞면 모서리만)
		if values.toggles[TOGGLE_WIREFRAME] {
			unsafe {
				gl.bind_framebuffer(glow::FRAMEBUFFER, None);
				gl.viewport(0, 0, canvas_width, canvas_height);
				gl.enable(glow::DEPTH_TEST);
				gl.depth_mask(true);
				gl.clear(glow::DEPTH_BUFFER_BIT);
				gl.color_mask(false, false, false, false);
				gl.enable(glow::POLYGON_OFFSET_FILL);
				gl.polygon_offset(1.5, 2.0);
			}
			for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
				resources.skin_renderer.draw_depth(gl, model, &view_projection_elements, &model_elements);
			}
			unsafe {
				gl.disable(glow::POLYGON_OFFSET_FILL);
				gl.color_mask(true, true, true, true);
				gl.depth_mask(false);
				gl.depth_func(glow::LEQUAL);
				gl.enable(glow::BLEND);
				gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
			}
			for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
				resources.skin_renderer.draw_wireframe(gl, model, &view_projection_elements, &model_elements, 0.91, 0.79, 0.54, 0.55);
			}
			unsafe {
				gl.disable(glow::BLEND);
				gl.depth_mask(true);
				gl.depth_func(glow::LESS);
				gl.disable(glow::DEPTH_TEST);
			}
			if self.statistics.triangle_count > 0.0 {
				draw_call_count += collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()).len() as u32 * 2;
			}
		}
		unsafe {
			gl.disable(glow::CULL_FACE);
		}
		self.statistics.scene_draw_call_count = draw_call_count;
		self.preview_texture_list = [irradiance_texture, albedo_texture, specular_texture, scattered_irradiance_texture, hdr_color_texture];

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

impl OldFaceScene {
	//==============================================================================
	// 모델 읽기 요청. (원본 loadModelFile, 캐시에 있으면 바로 쓰고 없으면 비동기로 읽는다. 읽는 동안 화면은 이전 모델로 돈다)
	//==============================================================================
	fn request_model(&mut self, engine: &Engine, file_name: &str, is_head: bool, value: f64) {
		if is_head {
			self.pending_head_value = Some(value);
		}
		else {
			self.pending_hair_value = Some(value);
		}
		let is_cached = self.resources.as_ref().map(|resources| resources.model_cache.iter().any(|(name, _)| name == file_name)).unwrap_or(false);
		if is_cached {
			self.apply_model(file_name, is_head, value);
			return;
		}
		if let Some(pending) = self.pending_model_list.iter_mut().find(|pending| pending.file_name == file_name) {
			pending.is_head = is_head;
			pending.value = value;
			return;
		}
		let mut blob_asset = BlobAsset::new();
		let load_handle = blob_asset.load(engine, file_name);
		self.pending_model_list.push(PendingModel { file_name: file_name.to_string(), blob_asset, load_handle, is_head, value });
	}

	// 읽은 모델 쓰기. (원본 pendingHeadLod === value 확인, 그 사이 슬라이더가 또 바뀌었으면 쓰지 않는다)
	fn apply_model(&mut self, file_name: &str, is_head: bool, value: f64) {
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		if is_head {
			if self.pending_head_value == Some(value) {
				resources.head_file = file_name.to_string();
			}
		}
		else if self.pending_hair_value == Some(value) {
			resources.hair_file = Some(file_name.to_string());
		}
	}

	// 다 읽힌 모델 해석. (한 프레임에 하나씩)
	fn poll_pending_models(&mut self, engine: &Engine) {
		let graphic = match engine.get_graphic() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		let position = match self.pending_model_list.iter().position(|pending| pending.load_handle.is_loaded() || pending.load_handle.is_failed()) {
			Some(position) => {
				position
			}
			None => {
				return;
			}
		};
		let pending = self.pending_model_list.remove(position);
		let bytes = match pending.blob_asset.get_blob() {
			Some(bytes) => {
				bytes
			}
			None => {
				eprintln!("oldface: {} 읽기 실패", pending.file_name);
				return;
			}
		};
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		match SkinnedModel::load_from_bytes(graphic.get_gl(), &bytes, &resources.skin_renderer) {
			Ok(mut model) => {
				if let Err(error) = assign_materials(&mut model, &resources.material_list) {
					eprintln!("oldface: {}", error);
					return;
				}
				model.update(0.0);
				resources.model_cache.push((pending.file_name.clone(), model));
				self.apply_model(&pending.file_name, pending.is_head, pending.value);
			}
			Err(error) => {
				eprintln!("oldface: {}: {}", pending.file_name, error);
			}
		}
	}

	// 통계 합계. (원본 refreshStatistics, 활성 모델 전체)
	fn refresh_statistics(&mut self) {
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		let mut triangle_count = 0.0;
		let mut vertex_count = 0;
		let mut morph_target_count = 0;
		for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
			for drawable in model.get_drawable_list().iter() {
				triangle_count += if drawable.is_indexed { drawable.index_count as f64 / 3.0 } else { drawable.vertex_count as f64 / 3.0 };
				vertex_count += drawable.vertex_count;
				morph_target_count = morph_target_count.max(drawable.morph_target_list.len());
			}
		}
		self.statistics.triangle_count = triangle_count;
		self.statistics.vertex_count = vertex_count;
		self.statistics.morph_target_count = morph_target_count;
	}

	// GPU 메모리 추정. (원본 estimateGpuMemoryBytes, 텍스처 밉 포함 + 읽은 모델의 정점, 인덱스, 모프 텍스처 + 렌더 타겟 + 섀도우 맵)
	fn estimate_gpu_memory_bytes(&self) -> f64 {
		let resources = match &self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return 0.0;
			}
		};
		let mut bytes = resources.image_byte_total;
		for (_name, model) in resources.model_cache.iter() {
			bytes += mesh_array_bytes(model);
			for drawable in model.get_drawable_list().iter() {
				if drawable.morph_texture.is_some() {
					bytes += drawable.morph_texture_width as f64 * drawable.morph_rows_per_target as f64 * 2.0 * drawable.morph_target_list.len() as f64 * 16.0;
				}
			}
		}
		let render_pixels = resources.scene_render_target.get_width() as f64 * resources.scene_render_target.get_height() as f64;
		bytes += render_pixels * (4.0 * 8.0 + 4.0);
		bytes += render_pixels * 8.0;
		bytes += render_pixels * 4.0;
		bytes += render_pixels * 8.0 * 2.0;
		bytes += render_pixels * 0.25 * 8.0 * 3.0;
		bytes += render_pixels * 8.0 * 0.4;
		bytes += resources.shadow_resolution as f64 * resources.shadow_resolution as f64 * 4.0;
		bytes
	}

	//==============================================================================
	// 우클릭 메뉴와 시점, 옵션 되돌리기. (원본 CONTEXT_MENU_ITEMS, resetView, resetOptions)
	//==============================================================================
	fn is_pointer_on_graph(&self, position: Vector2) -> bool {
		self.option_values.borrow().toggles[TOGGLE_GRAPH] && position.x >= self.graph.left && position.x < self.graph.right && position.y >= self.graph.top && position.y < self.graph.bottom
	}

	fn is_context_menu_showing(&self) -> bool {
		match &self.ui {
			Some(ui) => {
				self.scene_base.get_node_tree()[ui.menu_id].get_component::<UIContextMenu>().map(|menu| menu.is_showing()).unwrap_or(false)
			}
			None => {
				false
			}
		}
	}

	// 옵션 패널과 표정 패널 위인지. (원본 closest("#optionPanel"), closest("#expressionPanel"))
	fn is_pointer_over_panel(&self, position: Vector2) -> bool {
		let ui = match &self.ui {
			Some(ui) => ui,
			None => {
				return false;
			}
		};
		let node_tree = self.scene_base.get_node_tree();
		let options_position = node_tree[ui.options_id].get_local_position();
		let options_size = node_tree[ui.options_id].get_content_size();
		if position.x >= options_position.x && position.x <= options_position.x + options_size.x && position.y >= options_position.y && position.y <= options_position.y + options_size.y {
			return true;
		}
		let mut minimum = Vector2::create(f64::MAX, f64::MAX);
		let mut maximum = Vector2::create(f64::MIN, f64::MIN);
		for button_id in ui.expression_button_ids.iter() {
			let button_position = node_tree[*button_id].get_local_position();
			let button_size = node_tree[*button_id].get_content_size();
			minimum = Vector2::create(minimum.x.min(button_position.x), minimum.y.min(button_position.y));
			maximum = Vector2::create(maximum.x.max(button_position.x + button_size.x), maximum.y.max(button_position.y + button_size.y));
		}
		position.x >= minimum.x && position.x <= maximum.x && position.y >= minimum.y && position.y <= maximum.y
	}

	fn open_context_menu(&mut self, pointer_position: Vector2) {
		let menu_id = match &self.ui {
			Some(ui) => ui.menu_id,
			None => {
				return;
			}
		};
		let view_size = Engine::get_engine().unwrap().get_view_manager().get_view_size();
		let menu_width = 182.0;
		let menu_height = CONTEXT_MENU_LABELS.len() as f64 * CONTEXT_MENU_ITEM_HEIGHT + 8.0;
		let position = Vector2::create(pointer_position.x.min(view_size.x - menu_width - 8.0), pointer_position.y.min(view_size.y - menu_height - 8.0));
		let mut item_list = Vec::new();
		for (item_index, label) in CONTEXT_MENU_LABELS.iter().enumerate() {
			let pending_action = self.pending_menu_action.clone();
			item_list.push(UIContextMenuItem::create(label, Box::new(move |_context: &mut ComponentContext| {
				pending_action.set(Some(item_index));
			})));
		}
		self.scene_base.get_node_tree_mut().with_component_mut::<UIContextMenu, _>(menu_id, |context_menu, tree| {
			context_menu.show(tree, position, item_list);
		});
	}

	fn handle_menu_action(&mut self) {
		match self.pending_menu_action.take() {
			Some(0) => {
				self.camera_target_yaw = 0.0;
				self.camera_target_pitch = CAMERA_DEFAULT_PITCH;
				self.camera_target_distance = CAMERA_DEFAULT_DISTANCE;
			}
			Some(1) => {
				self.reset_options();
			}
			Some(2) => {
				let mut values = self.option_values.borrow_mut();
				values.toggles[TOGGLE_GRAPH] = !values.toggles[TOGGLE_GRAPH];
			}
			_ => {}
		}
		// 확인용: VANILLA_OLDFACE_MENU="x,y" 이면 8 프레임째에 그 자리로 메뉴를 연다. (화면 저장으로 메뉴를 찍을 때)
		self.menu_test_frame += 1;
		if self.menu_test_frame == 8 {
			if let Ok(text) = std::env::var("VANILLA_OLDFACE_MENU") {
				let numbers: Vec<f64> = text.split(',').filter_map(|part| part.parse::<f64>().ok()).collect();
				if numbers.len() == 2 {
					self.open_context_menu(Vector2::create(numbers[0], numbers[1]));
				}
			}
		}
	}

	// 옵션 되돌리기. (토글과 슬라이더를 기본값으로, 슬라이더 노드 값도 같이)
	fn reset_options(&mut self) {
		{
			let mut values = self.option_values.borrow_mut();
			values.toggles = TOGGLE_DEFAULTS;
			for (index, definition) in SLIDER_DEFINITIONS.iter().enumerate() {
				values.sliders[index] = definition.value;
			}
		}
		let slider_ids = match &self.ui {
			Some(ui) => ui.slider_ids.clone(),
			None => {
				return;
			}
		};
		let node_tree = self.scene_base.get_node_tree_mut();
		for (index, slider_id) in slider_ids.iter().enumerate() {
			if let Some(slider) = node_tree[*slider_id].get_component_mut::<UISlider>() {
				slider.set_value(SLIDER_DEFINITIONS[index].value);
			}
		}
	}

	//==============================================================================
	// 화면 위 그림. (머티리얼 그래프와 렌더링 정보. 3D 가 그려진 화면 위, UI 노드 아래)
	//==============================================================================
	fn draw_overlays(&mut self, graphic: &mut Graphic, engine: &Engine) {
		let view_size = engine.get_view_manager().get_view_size();
		let pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		let values = self.option_values.borrow().clone();
		graphic.set_global_alpha(1.0);
		let mut preview_draw_call_count = 0;
		if values.toggles[TOGGLE_GRAPH] {
			preview_draw_call_count = self.draw_material_graph(graphic, view_size, pixel_size);
		}
		self.statistics.draw_call_count = self.statistics.scene_draw_call_count + preview_draw_call_count;
		if values.toggles[TOGGLE_HUD] {
			self.draw_hud(graphic, view_size, pixel_size);
		}
		else {
			self.statistics.hud_top = view_size.y - 18.0;
		}
	}

	// 자간 있는 텍스트. (원본 drawSpacedText, 글자마다 간격을 더한다)
	fn draw_spaced_text(graphic: &mut Graphic, text: &str, x: f64, y: f64, spacing: f64) -> f64 {
		let mut cursor_x = x;
		for character in text.chars() {
			let character_text = character.to_string();
			graphic.draw_fill_text(&character_text, cursor_x, y);
			cursor_x += graphic.measure_text(&character_text).width + spacing;
		}
		cursor_x - x - spacing
	}

	fn build_graph_nodes(&self) -> Vec<GraphNode> {
		let values = self.option_values.borrow().clone();
		let slider = values.sliders;
		let resources = self.resources.as_ref().expect("resources");
		let active_unit_count = resources.current_weights.iter().filter(|weight| **weight > 0.02).count();
		let make_node = |title: &'static str, is_active: bool, lines: Vec<String>, thumbnail_list: Vec<GraphThumbnail>| GraphNode { title, is_active, lines, thumbnail_list, x: 0.0, y: 0.0, width: 0.0, height: 0.0, text_y: 0.0 };
		let image = |label: &'static str, image_index: usize| GraphThumbnail { label, image_index: Some(image_index), buffer_index: None, display_scale: 1.0, use_coverage: 0, x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
		let buffer = |label: &'static str, buffer_index: usize, display_scale: f64, use_coverage: i32| GraphThumbnail { label, image_index: None, buffer_index: Some(buffer_index), display_scale, use_coverage, x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
		vec![
			make_node("SKIN TEXTURES", true, vec![], vec![image("ALBEDO", 0), image("NORMAL", 1), image("CAVITY", 2)]),
			make_node("EYES, TEETH, LASHES AND GROOM", true, vec![String::from("refractive cornea, iris parallax and normal, eyelid occlusion shell"), String::from("kajiya-kay dual lobe hair cards, cutout + blended fringe")], vec![image("EYE", 4), image("TEETH", 5), image("LASHES", 6), image("HAIR", 3)]),
			make_node("SURFACE", true, vec![String::from("normal = metahuman normal x 1.5 + wrinkle maps x mask weights + micro normal x38"), String::from("cavity = height - blur(height) x micro cavity, roughness from cavity"), String::from("skin F0 0.028, eyes 0.04 glossy")], vec![]),
			make_node("LIGHTING", true, vec![format!("key 2.6 warm, shadow {}, fill 0.85 cool, rim 1.4", if values.toggles[TOGGLE_SHADOW] { "on" } else { "off" }), String::from("hemisphere ambient, dual-lobe GGX, sheen, env BRDF")], vec![]),
			make_node("G-BUFFER (MRT RGBA16F)", true, vec![], vec![buffer("IRRADIANCE", 0, 0.5, 1), buffer("ALBEDO", 1, 1.6, 1), buffer("SPECULAR", 2, 3.0, 1)]),
			make_node("SUBSURFACE AND COMBINE", values.toggles[TOGGLE_SUBSURFACE], vec![String::from("separable SSS, width 8 mm, depth aware"), String::from("irradiance x albedo + specular + background")], vec![buffer("SCATTERED", 3, 0.5, 1), buffer("HDR", 4, 0.72, 0)]),
			make_node("EXPRESSION", true, vec![format!("preset {}, {} ARKit shapes baked from RigLogic, {} active", EXPRESSION_DEFINITIONS[self.active_expression_index].0, self.statistics.morph_target_count, active_unit_count), String::from("blink from eye blink shapes")], vec![]),
			make_node("POST", true, vec![format!("bloom {}, exposure {:.2}, ACES, vignette, grain", if values.toggles[TOGGLE_BLOOM] { format!("{:.2}", slider[SLIDER_BLOOM_STRENGTH]) } else { String::from("off") }, slider[SLIDER_EXPOSURE]), format!("resolution scale {}%, {}, {} hair, head lod {}", (slider[SLIDER_RENDER_SCALE] * 100.0).round() as i64, ANTIALIAS_LABELS[slider[SLIDER_ANTIALIAS] as usize].to_lowercase(), HAIR_LEVEL_DEFINITIONS[slider[SLIDER_HAIR_QUALITY] as usize].0.to_lowercase(), 2 - slider[SLIDER_HEAD_LOD] as i64)], vec![]),
		]
	}

	// 노드 배치. (패널 좌표계, 내용 영역의 y 는 제목 바 아래 0 부터)
	fn layout_graph_nodes(graphic: &mut Graphic, node_list: &mut [GraphNode], panel_width: f64, aspect_ratio: f64) -> f64 {
		let content_width = panel_width - GRAPH_PADDING * 2.0;
		let text_width = content_width - GRAPH_NODE_PADDING * 2.0;
		let mut cursor_y = GRAPH_PADDING;
		for node in node_list.iter_mut() {
			// 설명을 노드 폭에 맞춰 단어 단위로 줄바꿈한다. (실제 글꼴로 측정)
			graphic.set_font_string(GRAPH_TEXT_FONT);
			let mut wrapped_lines = Vec::new();
			for line in node.lines.iter() {
				let mut current_line = String::new();
				for word in line.split(' ') {
					let candidate = if !current_line.is_empty() { format!("{} {}", current_line, word) } else { word.to_string() };
					let candidate_width = graphic.measure_text(&candidate).width + candidate.chars().count() as f64 * GRAPH_TEXT_SPACING;
					if candidate_width > text_width && !current_line.is_empty() {
						wrapped_lines.push(current_line);
						current_line = word.to_string();
					}
					else {
						current_line = candidate;
					}
				}
				if !current_line.is_empty() {
					wrapped_lines.push(current_line);
				}
			}
			node.lines = wrapped_lines;
			node.x = GRAPH_PADDING;
			node.y = cursor_y;
			let mut content_y = node.y + 28.0;
			if !node.thumbnail_list.is_empty() {
				let is_image_row = node.thumbnail_list[0].image_index.is_some();
				let thumbnail_width = if is_image_row { GRAPH_IMAGE_THUMBNAIL_SIZE } else { GRAPH_BUFFER_THUMBNAIL_WIDTH };
				let thumbnail_height = if is_image_row { GRAPH_IMAGE_THUMBNAIL_SIZE } else { (GRAPH_BUFFER_THUMBNAIL_WIDTH / aspect_ratio).round() };
				let spacing = ((text_width - node.thumbnail_list.len() as f64 * thumbnail_width) / ((node.thumbnail_list.len() as f64 - 1.0).max(1.0))).clamp(6.0, 30.0);
				let node_x = node.x;
				for (thumbnail_index, thumbnail) in node.thumbnail_list.iter_mut().enumerate() {
					thumbnail.x = node_x + GRAPH_NODE_PADDING + thumbnail_index as f64 * (thumbnail_width + spacing);
					thumbnail.y = content_y;
					thumbnail.width = thumbnail_width;
					thumbnail.height = thumbnail_height;
				}
				content_y += thumbnail_height + 20.0;
			}
			node.text_y = content_y;
			content_y += node.lines.len() as f64 * GRAPH_LINE_HEIGHT;
			node.width = content_width;
			node.height = content_y - node.y + GRAPH_NODE_PADDING;
			cursor_y = node.y + node.height + GRAPH_NODE_GAP;
		}
		cursor_y - GRAPH_NODE_GAP + GRAPH_PADDING
	}

	// 머티리얼 그래프. (원본 renderMaterialGraph, 좌측에 텍스처 입력부터 합성, 포스트까지의 흐름을 노드로 표시)
	// - 텍스처는 축소본 그림, 중간 버퍼는 매 프레임 GL 블릿으로 실시간 표시한다.
	fn draw_material_graph(&mut self, graphic: &mut Graphic, view_size: Vector2, pixel_size: Vector2) -> u32 {
		let pixel_ratio = pixel_size.x / view_size.x.max(1.0);
		let aspect_ratio = pixel_size.x / pixel_size.y.max(1.0);
		let is_narrow = view_size.x < 640.0;
		let margin = if is_narrow { 10.0 } else { GRAPH_PANEL_MARGIN };
		let panel_width = if is_narrow { view_size.x - margin * 2.0 } else { GRAPH_PANEL_WIDTH };
		let panel_y = if is_narrow { margin + 34.0 + 8.0 } else { margin };
		let hud_top = if self.statistics.hud_top > 0.0 { self.statistics.hud_top } else { view_size.y - 18.0 };
		let maximum_height = (GRAPH_TITLE_HEIGHT + 60.0).max(view_size.y.min(hud_top) - panel_y - 12.0);
		let frame_x = margin;
		let mut node_list = self.build_graph_nodes();
		let content_height = OldFaceScene::layout_graph_nodes(graphic, &mut node_list, panel_width, aspect_ratio);
		let panel_height = (GRAPH_TITLE_HEIGHT + content_height).min(maximum_height);
		let view_height = panel_height - GRAPH_TITLE_HEIGHT;
		self.graph.maximum_scroll = (content_height - view_height).max(0.0);
		self.graph.scroll_offset = self.graph.scroll_offset.clamp(0.0, self.graph.maximum_scroll);
		self.graph.left = frame_x;
		self.graph.top = panel_y;
		self.graph.right = frame_x + panel_width;
		self.graph.bottom = panel_y + panel_height;
		let content_top = panel_y + GRAPH_TITLE_HEIGHT;
		let origin_y = content_top - self.graph.scroll_offset;
		let content_rect = Rect::create(frame_x, content_top, panel_width, view_height);
		let active_title_color = create_color(232.0, 201.0, 138.0, 0.95);
		let inactive_title_color = create_color(232.0, 201.0, 138.0, 0.38);
		let active_text_color = create_color(242.0, 236.0, 226.0, 0.74);
		let inactive_text_color = create_color(242.0, 236.0, 226.0, 0.32);
		let label_color = create_color(242.0, 236.0, 226.0, 0.5);

		// 1. 패널, 제목 바, 노드 배경. (내용은 제목 아래 영역으로 자른다)
		let panel_rect = Rect::create(frame_x, panel_y, panel_width, panel_height);
		graphic.set_fill_color(create_color(14.0, 12.0, 14.0, 0.5));
		graphic.draw_round_rect(panel_rect, 12.0);
		graphic.set_stroke_color(create_color(242.0, 236.0, 226.0, 0.14));
		graphic.draw_stroke_round_rect(panel_rect, 12.0, 1.0);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_font_string(GRAPH_TITLE_FONT);
		graphic.set_fill_color(active_title_color);
		OldFaceScene::draw_spaced_text(graphic, "MATERIAL GRAPH", frame_x + 16.0, panel_y + GRAPH_TITLE_HEIGHT * 0.5 + 1.0, GRAPH_TITLE_SPACING);
		if self.graph.maximum_scroll > 0.0 {
			graphic.set_font_string(GRAPH_LABEL_FONT);
			graphic.set_fill_color(label_color);
			let hint_text = "SCROLL";
			let hint_width = graphic.measure_text(hint_text).width + hint_text.len() as f64 * GRAPH_LABEL_SPACING;
			OldFaceScene::draw_spaced_text(graphic, hint_text, frame_x + panel_width - 16.0 - hint_width, panel_y + GRAPH_TITLE_HEIGHT * 0.5 + 1.0, GRAPH_LABEL_SPACING);
		}
		graphic.set_stroke_color(create_color(242.0, 236.0, 226.0, 0.1));
		graphic.draw_line(&[Vector2::create(frame_x, content_top), Vector2::create(frame_x + panel_width, content_top)], 1.0);
		graphic.begin_clip_rect(content_rect, 0.0);
		for node in node_list.iter() {
			let node_rect = Rect::create(frame_x + node.x, origin_y + node.y, node.width, node.height);
			graphic.set_fill_color(if node.is_active { create_color(255.0, 255.0, 255.0, 0.045) } else { create_color(255.0, 255.0, 255.0, 0.02) });
			graphic.draw_round_rect(node_rect, 8.0);
			for thumbnail in node.thumbnail_list.iter() {
				graphic.set_fill_color(create_color(0.0, 0.0, 0.0, 0.4));
				graphic.draw_rect(Rect::create(frame_x + thumbnail.x, origin_y + thumbnail.y, thumbnail.width, thumbnail.height));
			}
		}
		graphic.end_clip_rect();

		// 2. 중간 버퍼 블릿과 그림 썸네일. (뷰포트를 썸네일 사각형으로 제한, 버퍼는 내용 영역 안에 온전히 들어온 것만, 그림은 내용 영역으로 자른다)
		let canvas_width = pixel_size.x as i32;
		let canvas_height = pixel_size.y as i32;
		let mut buffer_blit_list: Vec<(GlTexture, [i32; 4], f64, i32)> = Vec::new();
		let mut image_blit_list: Vec<(GlTexture, [i32; 4])> = Vec::new();
		let resources = self.resources.as_ref().expect("resources");
		for node in node_list.iter() {
			for thumbnail in node.thumbnail_list.iter() {
				let thumbnail_top = origin_y + thumbnail.y;
				let viewport = [((frame_x + thumbnail.x) * pixel_ratio).round() as i32, (pixel_size.y - (thumbnail_top + thumbnail.height) * pixel_ratio).round() as i32, (thumbnail.width * pixel_ratio).round() as i32, (thumbnail.height * pixel_ratio).round() as i32];
				if let Some(buffer_index) = thumbnail.buffer_index {
					if thumbnail_top < content_top || thumbnail_top + thumbnail.height > content_top + view_height {
						continue;
					}
					if let Some(texture) = self.preview_texture_list[buffer_index] {
						buffer_blit_list.push((texture, viewport, thumbnail.display_scale, thumbnail.use_coverage));
					}
				}
				else if let Some(image_index) = thumbnail.image_index {
					if thumbnail_top + thumbnail.height < content_top || thumbnail_top > content_top + view_height {
						continue;
					}
					image_blit_list.push((resources.thumbnail_texture_list[image_index], viewport));
				}
			}
		}
		let scissor = [(frame_x * pixel_ratio).round() as i32, (pixel_size.y - (content_top + view_height) * pixel_ratio).round() as i32, (panel_width * pixel_ratio).round() as i32, (view_height * pixel_ratio).round() as i32];
		let preview_draw_call_count = graphic.run_external_pass(false, |gl| {
			let mut drawn_count = 0;
			unsafe {
				gl.disable(glow::BLEND);
				resources.preview_pass.use_program(gl);
				gl.uniform_1_i32(resources.preview_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
				gl.active_texture(glow::TEXTURE0);
				for (texture, viewport, display_scale, use_coverage) in buffer_blit_list.iter() {
					gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
					gl.uniform_1_f32(resources.preview_pass.get_uniform_location(gl, "displayScale").as_ref(), *display_scale as f32);
					gl.uniform_1_i32(resources.preview_pass.get_uniform_location(gl, "useCoverage").as_ref(), *use_coverage);
					gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
					resources.preview_pass.draw(gl);
					drawn_count += 1;
				}
				gl.enable(glow::BLEND);
				gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
				gl.enable(glow::SCISSOR_TEST);
				gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
				resources.thumbnail_pass.use_program(gl);
				gl.uniform_1_i32(resources.thumbnail_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
				gl.uniform_1_f32(resources.thumbnail_pass.get_uniform_location(gl, "alphaScale").as_ref(), 1.0);
				for (texture, viewport) in image_blit_list.iter() {
					gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
					gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
					resources.thumbnail_pass.draw(gl);
				}
				gl.disable(glow::SCISSOR_TEST);
				gl.disable(glow::BLEND);
				gl.bind_texture(glow::TEXTURE_2D, None);
				gl.viewport(0, 0, canvas_width, canvas_height);
			}
			drawn_count
		});

		// 3. 테두리, 제목, 설명, 연결선. (자른다)
		Engine::get_engine().unwrap().get_view_manager().apply_view_rect(graphic);
		graphic.set_global_alpha(1.0);
		graphic.set_text_baseline(TextBaseline::Middle);
		graphic.set_text_align(TextAlign::Left);
		graphic.begin_clip_rect(content_rect, 0.0);
		for (node_index, node) in node_list.iter().enumerate() {
			let node_x = frame_x + node.x;
			let node_y = origin_y + node.y;
			for thumbnail in node.thumbnail_list.iter() {
				let thumbnail_x = frame_x + thumbnail.x;
				let thumbnail_y = origin_y + thumbnail.y;
				graphic.set_stroke_color(create_color(242.0, 236.0, 226.0, 0.18));
				graphic.draw_stroke_rect(Rect::create(thumbnail_x, thumbnail_y, thumbnail.width, thumbnail.height), 1.0);
				graphic.set_font_string(GRAPH_LABEL_FONT);
				graphic.set_fill_color(label_color);
				OldFaceScene::draw_spaced_text(graphic, thumbnail.label, thumbnail_x, thumbnail_y + thumbnail.height + 9.0, GRAPH_LABEL_SPACING);
			}
			graphic.set_font_string(GRAPH_NODE_TITLE_FONT);
			graphic.set_fill_color(if node.is_active { active_title_color } else { inactive_title_color });
			OldFaceScene::draw_spaced_text(graphic, node.title, node_x + GRAPH_NODE_PADDING, node_y + 14.0, GRAPH_NODE_TITLE_SPACING);
			graphic.set_font_string(GRAPH_TEXT_FONT);
			graphic.set_fill_color(if node.is_active { active_text_color } else { inactive_text_color });
			for (line_index, line) in node.lines.iter().enumerate() {
				OldFaceScene::draw_spaced_text(graphic, line, node_x + GRAPH_NODE_PADDING, origin_y + node.text_y + line_index as f64 * GRAPH_LINE_HEIGHT + 9.0, GRAPH_TEXT_SPACING);
			}
			if node_index < node_list.len() - 1 {
				let connector_x = node_x + 22.0;
				let connector_top = node_y + node.height;
				graphic.set_stroke_color(create_color(232.0, 201.0, 138.0, 0.5));
				graphic.draw_line(&[Vector2::create(connector_x, connector_top), Vector2::create(connector_x, connector_top + GRAPH_NODE_GAP)], 1.0);
				graphic.set_fill_color(create_color(232.0, 201.0, 138.0, 0.8));
				graphic.draw_circle(Vector2::create(connector_x, connector_top + GRAPH_NODE_GAP * 0.5), 1.8);
			}
		}
		graphic.end_clip_rect();
		preview_draw_call_count
	}

	// 렌더링 정보. (원본 renderHud, 우측 하단. 홈 링크와 겹치면 그 위로)
	fn draw_hud(&mut self, graphic: &mut Graphic, view_size: Vector2, pixel_size: Vector2) {
		// 메모리 수치는 1초에 한 번 표본을 읽고, 직전 표시값 대비 5% 넘게 변할 때만 바꾼다.
		if self.statistics.memory_sample_instant.map(|instant| instant.elapsed().as_secs_f64() >= 1.0).unwrap_or(true) {
			self.statistics.memory_sample_instant = Some(web_time::Instant::now());
			let gpu_memory_bytes = self.estimate_gpu_memory_bytes();
			if (gpu_memory_bytes - self.statistics.gpu_memory_bytes).abs() > self.statistics.gpu_memory_bytes * 0.05 {
				self.statistics.gpu_memory_bytes = gpu_memory_bytes;
				self.statistics.gpu_memory_text = format!("{:.0} MB", gpu_memory_bytes / 1048576.0);
			}
		}
		let resources = self.resources.as_ref().expect("resources");
		let row_height = 16.0;
		let mut row_list: Vec<(String, String)> = vec![
			(String::from("fps"), format!("{}", (1.0 / self.statistics.smoothed_frame_seconds.max(0.0001)).round() as i64)),
			(String::from("display"), format!("{} x {}", pixel_size.x as i64, pixel_size.y as i64)),
			(String::from("render buffer"), format!("{} x {}", resources.scene_render_target.get_width(), resources.scene_render_target.get_height())),
			(String::from("triangles"), format_with_commas(self.statistics.triangle_count.round() as u64)),
			(String::from("vertices"), format_with_commas(self.statistics.vertex_count as u64)),
			(String::from("draw calls"), self.statistics.draw_call_count.to_string()),
			(String::from("morph targets"), self.statistics.morph_target_count.to_string()),
			(String::from("cpu frame"), format!("{:.2} ms", self.statistics.cpu_frame_milliseconds)),
		];
		// 읽을 수 없는 값의 행은 아예 만들지 않는다. (GPU 타이머가 없으면 패스 시간, 러스트에는 JS 힙이 없다)
		if self.pass_timer.is_supported {
			row_list.push((String::from("gpu shadow"), self.pass_timer.format("shadow")));
			row_list.push((String::from("gpu scene"), self.pass_timer.format("scene")));
			row_list.push((String::from("gpu sss + ao"), self.pass_timer.format("scatter")));
			row_list.push((String::from("gpu post"), self.pass_timer.format("post")));
		}
		row_list.push((String::from("gpu memory (est.)"), self.statistics.gpu_memory_text.clone()));
		let is_narrow_screen = view_size.x < 640.0;
		if is_narrow_screen {
			row_list.truncate(4);
		}
		let panel_width = if is_narrow_screen { 150.0 } else { 214.0 };
		let panel_height = row_list.len() as f64 * row_height + 18.0;
		let panel_x = view_size.x - panel_width - 18.0;
		let mut panel_y = view_size.y - panel_height - 18.0;
		if let Some(ui) = &self.ui {
			let node_tree = self.scene_base.get_node_tree();
			let home_position = node_tree[ui.home_id].get_local_position();
			let home_size = node_tree[ui.home_id].get_content_size();
			if home_position.x + home_size.x > panel_x && home_position.y < panel_y + panel_height {
				panel_y = home_position.y - panel_height - 12.0;
			}
		}
		self.statistics.hud_top = panel_y;
		let panel_rect = Rect::create(panel_x, panel_y, panel_width, panel_height);
		graphic.set_fill_color(create_color(14.0, 12.0, 14.0, 0.55));
		graphic.draw_round_rect(panel_rect, 10.0);
		graphic.set_stroke_color(create_color(242.0, 236.0, 226.0, 0.14));
		graphic.draw_stroke_round_rect(panel_rect, 10.0, 1.0);
		graphic.set_font_string("400 10.5px default");
		graphic.set_text_baseline(TextBaseline::Middle);
		for (row_index, (name, value)) in row_list.iter().enumerate() {
			let row_y = panel_y + 9.0 + row_height * row_index as f64 + row_height * 0.5;
			graphic.set_text_align(TextAlign::Left);
			graphic.set_fill_color(create_color(242.0, 236.0, 226.0, 0.45));
			graphic.draw_fill_text(name, panel_x + 14.0, row_y);
			graphic.set_text_align(TextAlign::Right);
			graphic.set_fill_color(create_color(242.0, 236.0, 226.0, 0.88));
			graphic.draw_fill_text(value, panel_x + panel_width - 14.0, row_y);
		}
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
		self.create_ui();
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
				self.pass_timer = PassTimer::new(graphic.get_gl());
				println!("oldface: 준비 {:.2}초, 모델 {}", self.load_seconds, resources.model_cache.len());
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
		self.frame_start_instant = web_time::Instant::now();
		let view_size = engine.get_view_manager().get_view_size();
		self.scene_base.get_root_mut().set_content_size(view_size);
		self.layout_ui(view_size);
		self.apply_intro();
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
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
		self.statistics.smoothed_frame_seconds = self.statistics.smoothed_frame_seconds * 0.92 + delta_time * 0.08;
		self.handle_input(engine);
		self.handle_menu_action();

		// 카메라. (목표값 지수 접근)
		let camera_approach = (delta_time * 5.0).min(1.0);
		self.camera_yaw += (self.camera_target_yaw - self.camera_yaw) * camera_approach;
		self.camera_pitch += (self.camera_target_pitch - self.camera_pitch) * camera_approach;
		self.camera_distance += (self.camera_target_distance - self.camera_distance) * camera_approach;

		// 표정 / 머리 방향. (머리는 느린 배회, 원본 HEAD FOLLOW POINTER 기본 0)
		self.update_expressions(delta_time);
		let head_follow = self.option_values.borrow().sliders[SLIDER_HEAD_FOLLOW];
		let canvas_native_size = engine.get_view_manager().get_canvas_native_size();
		let (follow_yaw, follow_pitch) = match (self.has_pointer_moved, self.pointer_position) {
			(true, Some(position)) => ((position.x / canvas_native_size.x.max(1.0) * 2.0 - 1.0).clamp(-1.0, 1.0) * 0.42 * head_follow, (position.y / canvas_native_size.y.max(1.0) * 2.0 - 1.0).clamp(-1.0, 1.0) * 0.22 * head_follow),
			_ => (0.0, 0.0),
		};
		let wander_yaw = 0.035 * (elapsed_time * 0.23).sin() + 0.02 * (elapsed_time * 0.61 + 1.3).sin() + follow_yaw;
		let wander_pitch = 0.015 * (elapsed_time * 0.31 + 0.6).sin() + follow_pitch;
		let head_approach = (delta_time * 2.2).min(1.0);
		self.head_yaw += (wander_yaw.clamp(-0.6, 0.6) - self.head_yaw) * head_approach;
		self.head_pitch += (wander_pitch.clamp(-0.35, 0.35) - self.head_pitch) * head_approach;

		self.apply_option_changes(engine);
		self.poll_pending_models(engine);
		self.refresh_statistics();
		if let Some(graphic) = engine.get_graphic() {
			self.pass_timer.poll(graphic.get_gl());
		}
		self.refresh_option_labels();
		let values = self.option_values.borrow().clone();
		if let Some(resources) = &mut self.resources {
			update_wrinkle_weights(resources, values.toggles[TOGGLE_WRINKLE]);
			OldFaceScene::apply_render_parameters(resources, elapsed_time, &values);
			for model in collect_active(&mut resources.model_cache, &resources.head_file, resources.hair_file.as_deref()) {
				model.update(delta_time);
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
		engine.get_view_manager().apply_view_rect(graphic);
		if self.resources.is_some() {
			self.draw_overlays(graphic, engine);
			engine.get_view_manager().apply_view_rect(graphic);
		}
		self.scene_base.draw(graphic);
		self.statistics.cpu_frame_milliseconds = self.statistics.cpu_frame_milliseconds * 0.9 + self.frame_start_instant.elapsed().as_secs_f64() * 1000.0 * 0.1;
		engine.get_view_manager().apply_canvas_native_rect(graphic);
		if self.resources.is_none() {
			let canvas_native_size = engine.get_view_manager().get_canvas_native_size();
			let status_line = self.load_error_message.clone().unwrap_or_else(|| String::from("LOADING"));
			graphic.set_font_string("14px default");
			graphic.set_text_align(TextAlign::Center);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.set_fill_color(Color::create(0.95, 0.92, 0.86, 0.6));
			graphic.draw_fill_text(&status_line, canvas_native_size.x * 0.5, canvas_native_size.y * 0.5);
		}
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
fn update_wrinkle_weights(resources: &mut FaceResources, is_enabled: bool) {
	let mut region_values: Vec<(String, f64)> = Vec::new();
	let shape_table = resources.wrinkle_table.get("shapes");
	for (unit_index, unit_name) in resources.expression_unit_names.iter().enumerate() {
		if !is_enabled {
			break;
		}
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
