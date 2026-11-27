//==============================================================================
//! 그래픽.
//! - GL 컨텍스트로 출력하는 렌더러. 원본(WebGL2)과 같은 셰이더와 같은 상태 모델을 쓴다.
//! - Canvas2D 시절의 좌표계(좌상 원점, 픽셀 단위)와 상태 모델(변환/알파 스택)을 그대로 재현한다.
//! - 모든 드로우는 draw_vertices() 한 지점으로 수렴한다. 단색 도형도 1x1 흰색 텍스처로 텍스처 경로와 통일.
//! - 클리핑은 스텐실 버퍼 기반. 텍스트는 문자열 텍스처로 구워 출력.
//! - 묶어 그리기(기본 켬): 같은 텍스처를 잇달아 그리면 정점을 CPU 에서 변환해 모았다가 한 번에 그린다.
//!   원본과 같은 묶음 셰이더로 정점마다 색과 틴트를 실어, 틴트가 걸린 그리기도 묶는다. 클립 영역 기록은 묶지 않는다.
//!   블렌드, 클립, 파티클 그리기, 프레임 끝에서 모은 것을 먼저 내보낸다(flush_batch). 원본도 기본으로 켠다. (setSpriteBatchingEnabled)
//! - 화면 밖 거르기(기본 켬): NodeTree 가 노드를 그리기 전에 is_local_rect_visible 로 묻는다. 원본도 기본으로 켜고 WorldNode 가 같은 함수로 묻는다. (setCullingEnabled)
//! - 셰이더 바꿔 끼우기(set_shader_program_override): ShaderSprite 같은 효과가 기본 프로그램 대신 제 프로그램으로 그린다. 바꿔 끼운 동안은 묶지 않는다.
//!   바꿔 끼울 프로그램은 resolve_shader_program 이 이름으로 담아 두고, 어트리뷰트 위치를 기본 프로그램과 같게 고정해 기본 버텍스 어레이를 같이 쓴다.
//! - 마지막으로 건 셰이더 프로그램과 텍스처 유닛 0 의 텍스처를 기억해, 같은 것을 다시 거는 GL 호출을 건너뛴다. (원본도 같다)
//!   GL 을 밖에서 만질 수 있는 때(get_gl 로 컨텍스트를 내줄 때, restore_render_state, 프레임 시작과 끝)에는 잊고 다음에 다시 건다.
//! - 밉맵 단계 보정(set_image_mipmap_bias)은 기본, 묶음, 파티클 셰이더의 textureLodBias 유니폼으로 건다. 바꿔 끼운 셰이더는 그 유니폼이 있으면 받는다. (원본과 같음)
//! - 글자 방향(set_text_direction, 원본 setTextDirection)은 상태에 들고, 구운 문자열을 그 문단 방향으로 모양 짓는다. (textshaping.rs 의 양방향 정렬과 모양 짓기)
//! - 블렌드 모드 26 가지(원본과 같음): 13 가지는 고정 블렌딩, 13 가지는 그릴 영역 아래를 지금 프레임버퍼에서 blit_framebuffer 로 떠 와 W3C 공식대로 셰이더로 섞는다.
//!   Multiply 는 기본 프레임버퍼에서는 고정 블렌딩, 렌더 타겟에 그릴 때는 셰이더로 섞는다. (바탕이 반투명하면 고정 블렌딩이 W3C 식과 달라서, 원본과 같음)
//!   셰이더 글은 원본과 한 글자도 다르지 않다. 바꿔 끼운 셰이더는 덧그림 칸(데스크탑은 늘 RGBA16F, 웹은 원본처럼 EXT_color_buffer_float 가 있을 때 RGBA16F)에 먼저 그리고 섞어 옮긴다.
//! - 기본 프레임버퍼에 그릴 때는 알파를 쓰지 않는다(apply_color_mask). 원본과 웹은 캔버스가 alpha: false 라 화면 알파가 늘 1 인데, 데스크탑 기본 프레임버퍼는 알파가 있어
//!   알파를 낮추는 모드 뒤에 DST_ALPHA 를 쓰는 모드와 셰이더 섞기가 웹과 달랐다. 렌더 타겟에는 알파도 쓴다.
//==============================================================================
use crate::core::glcontext::GlContext;
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::image::Image;
use crate::core::imagetexturecache::ImageTextureCache;
use crate::core::shaderprogram::ShaderProgram;
use crate::core::textshaping::ShapingDirection;
use crate::core::textstringtexturecache::{TextDrawKind, TextEntry, TextMetrics, TextStringTextureCache};
use glow::HasContext;
use crate::core::gluniform::GlUniformF64;
use std::collections::HashMap;
use std::rc::Rc;

/// GL 객체 타입. (데스크탑은 glow::Native*, 웹은 WebGL 객체. glow 컨텍스트가 정한 것을 그대로 쓴다)
pub type GlTexture = <glow::Context as glow::HasContext>::Texture;
/// GL 버퍼 객체. (버텍스 버퍼 같은 것)
pub type GlBuffer = <glow::Context as glow::HasContext>::Buffer;
/// GL 버텍스 어레이 객체.
pub type GlVertexArray = <glow::Context as glow::HasContext>::VertexArray;
/// GL 셰이더 프로그램 객체.
pub type GlProgram = <glow::Context as glow::HasContext>::Program;
/// GL 셰이더 객체. (버텍스나 프래그먼트 셰이더 하나)
pub type GlShader = <glow::Context as glow::HasContext>::Shader;
/// GL 유니폼 위치. (데스크탑은 정수 하나를 감싼 값, 웹은 WebGL 위치 객체)
pub type GlUniformLocation = <glow::Context as glow::HasContext>::UniformLocation;
/// GL 프레임버퍼 객체. (렌더 타겟)
pub type GlFramebuffer = <glow::Context as glow::HasContext>::Framebuffer;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 버텍스 셰이더. (픽셀 좌표 → 클립 좌표)
const VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
uniform mat3 projectionMatrix;
uniform mat3 modelMatrix;
out vec2 fragmentTextureCoordinate;
void main() {
	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
}
";

/// 프래그먼트 셰이더. (프리멀티플라이드 알파 출력)
/// - 단색 도형은 1x1 흰색 텍스처를 바인드해 같은 경로로 처리한다.
/// - tintColor 는 이미지 실루엣(알파) 안쪽에만 색을 덮는 틴트. (alpha 0 이면 비활성)
const FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D mainTexture;
uniform vec4 mainColor;
uniform vec4 tintColor;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	vec3 tintedColor = mix(textureColor.rgb, tintColor.rgb * textureColor.a, tintColor.a);
	float finalAlpha = textureColor.a * mainColor.a * globalAlpha;
	vec3 finalColor = tintedColor * mainColor.rgb * mainColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

/// 파티클 배치 버텍스 셰이더. (정점마다 색을 실어 한 번의 드로우로 수백 개를 그린다)
const PARTICLE_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
in vec4 vertexColor;
uniform mat3 projectionMatrix;
uniform mat3 modelMatrix;
out vec2 fragmentTextureCoordinate;
out vec4 fragmentColor;
void main() {
	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
	fragmentColor = vertexColor;
}
";

/// 파티클 배치 프래그먼트 셰이더. (프리멀티플라이드 알파 출력)
const PARTICLE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
in vec4 fragmentColor;
uniform sampler2D mainTexture;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	float finalAlpha = textureColor.a * fragmentColor.a * globalAlpha;
	vec3 finalColor = textureColor.rgb * fragmentColor.rgb * fragmentColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

/// 묶어 그리기의 자르기 사각형 표 칸 수. (0 번은 자르지 않는 칸, 차면 그때까지 쌓인 것을 그린다, 원본과 같음)
const BATCH_CLIP_SLOT_CAPACITY: usize = 32;

/// 묶어 그리기의 텍스처 칸 수 상한. (기기의 MAX_TEXTURE_IMAGE_UNITS 와 이 값 가운데 작은 것, PixiJS 와 같은 16, 원본과 같음)
const BATCH_TEXTURE_SLOT_LIMIT: usize = 16;

/// 자르지 않는 칸의 사각형 끝. (디바이스 픽셀, f32 로 정확한 2^24)
const BATCH_UNCLIPPED_EXTENT: f32 = 16777216.0;

/// 셰이더 자르기 경계가 화소 경계(또는 화소 가운데)에 놓였다고 보는 차이. (디바이스 픽셀, 변환 셈의 오차만 받는다)
const CLIP_EDGE_EPSILON: f64 = 1.0 / 256.0;

//==============================================================================
/// 자르기 경계가 화소 가운데(정수 + 0.5)에 걸쳤는지. (래스터 동점 규칙이 기기마다 다를 수 있는 자리)
//==============================================================================
fn is_clip_edge_on_pixel_center(edge: f64) -> bool {
	let fraction = edge - edge.floor();
	(fraction - 0.5).abs() < CLIP_EDGE_EPSILON
}

/// 묶어 그리기 버텍스 셰이더. (정점은 쌓을 때 이미 디바이스 픽셀 좌표로 바꿔 둔다, 원본과 같음)
/// - vertexSlot.x 는 텍스처 칸 번호, vertexSlot.y 는 자르기 사각형 표의 칸 번호다. (텍스처와 축에 맞는 자르기가 바뀌어도 묶음이 이어진다)
/// - 표 칸 수는 BATCH_CLIP_SLOT_CAPACITY 와 같다.
const BATCH_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
in vec4 vertexColor;
in vec4 vertexTintColor;
in vec2 vertexSlot;
uniform mat3 projectionMatrix;
uniform vec4 clipRectList[32];
out vec2 fragmentTextureCoordinate;
out vec4 fragmentColor;
out vec4 fragmentTintColor;
flat out int fragmentTextureSlot;
flat out vec4 fragmentClipRect;
flat out float fragmentParticleAlpha;
void main() {
	vec3 transformedPosition = projectionMatrix * vec3(vertexPosition, 1.0);
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
	fragmentColor = vertexColor;
	fragmentTintColor = vertexTintColor;
	fragmentTextureSlot = int(vertexSlot.x + 0.5);
	fragmentClipRect = clipRectList[int(vertexSlot.y + 0.5)];
	fragmentParticleAlpha = vertexTintColor.a < 0.0 ? vertexTintColor.r : -1.0;
}
";

//==============================================================================
/// 묶어 그리기 프래그먼트 셰이더 글 짓기. (기본 셰이더와 같은 셈. 정점 색은 mainColor.rgb * mainColor.a * globalAlpha 와 mainColor.a * globalAlpha, 원본 createBatchFragmentShaderSource 와 같은 글)
/// - 텍스처는 칸 수만큼의 샘플러 배열에 묶고 칸 번호로 고른다. (GLSL ES 3.00 은 샘플러 배열을 상수로만 짚을 수 있어 칸마다 갈래를 둔다, PixiJS 와 같은 꼴)
/// - 자르기는 화소 가운데(gl_FragCoord)를 디바이스 픽셀로 옮겨 사각형 [왼쪽, 오른쪽) x [위, 아래) 밖이면 버린다. (스텐실에 사각형을 그려 자른 것과 화소까지 같다)
/// - 틴트 알파가 음수인 정점은 정점 색 쿼드(draw_colored_quads)다. 정점 색은 곱하지 않은 색 그대로, 틴트 빨강 칸에 전역 알파를 싣고 파티클 셰이더와 같은 식으로 셈한다.
//==============================================================================
fn create_batch_fragment_shader_source(texture_slot_count: usize) -> String {
	let mut texture_sample_source = String::new();
	for slot_index in 0..texture_slot_count {
		let branch_source = if slot_index == 0 {
			format!("if (fragmentTextureSlot == {})", slot_index)
		}
		else if slot_index == texture_slot_count - 1 {
			String::from("else")
		}
		else {
			format!("else if (fragmentTextureSlot == {})", slot_index)
		};
		texture_sample_source.push_str(&format!("\t{} {{\n\t\ttextureColor = texture(mainTextureList[{}], fragmentTextureCoordinate, textureLodBias);\n\t}}\n", branch_source, slot_index));
	}
	format!(
		"#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
in vec4 fragmentColor;
in vec4 fragmentTintColor;
flat in int fragmentTextureSlot;
flat in vec4 fragmentClipRect;
flat in float fragmentParticleAlpha;
uniform sampler2D mainTextureList[{}];
uniform float textureLodBias;
uniform float framebufferHeight;
out vec4 outputColor;
void main() {{
	vec2 devicePosition = vec2(gl_FragCoord.x, framebufferHeight - gl_FragCoord.y);
	if (devicePosition.x < fragmentClipRect.x || devicePosition.y < fragmentClipRect.y || devicePosition.x >= fragmentClipRect.z || devicePosition.y >= fragmentClipRect.w) {{
		discard;
	}}
	vec4 textureColor = vec4(0.0);
{}	if (fragmentParticleAlpha >= 0.0) {{
		float finalAlpha = textureColor.a * fragmentColor.a * fragmentParticleAlpha;
		vec3 finalColor = textureColor.rgb * fragmentColor.rgb * fragmentColor.a * fragmentParticleAlpha;
		outputColor = vec4(finalColor, finalAlpha);
		return;
	}}
	vec3 tintedColor = mix(textureColor.rgb, fragmentTintColor.rgb * textureColor.a, fragmentTintColor.a);
	outputColor = vec4(tintedColor * fragmentColor.rgb, textureColor.a * fragmentColor.a);
}}
",
		texture_slot_count, texture_sample_source
	)
}

/// 셰이더 섞기 함수. (고정 블렌딩으로 안 되는 블렌드 모드에서 그릴 영역 아래와 섞는다, 원본과 같은 글)
/// - backdropTexture 는 그리기 직전에 지금 프레임버퍼에서 그릴 영역만 떠 온 것이다. (프레임버퍼와 같은 크기와 형식, 프리멀티플라이드)
/// - 공식은 W3C Compositing and Blending Level 1 (https://www.w3.org/TR/compositing-1/, 2024-03-21 후보 권고안) 그대로다.
///   10.1 분리 섞기: multiply = Cb x Cs, overlay = HardLight(Cs, Cb), darken = min, lighten = max, color-dodge, color-burn, hard-light, soft-light (D(Cb) 포함), difference, exclusion.
///   10.2 비분리 섞기: hue, saturation, color, luminosity. (Lum = 0.3 R + 0.59 G + 0.11 B, ClipColor, SetLum, Sat, SetSat)
///   섞은 색은 범위로 자른다. (10 장 "The result of the mixing formula must be clamped")
/// - 6 장 일반식대로 Cs' = (1 - αb) x Cs + αb x B(Cb, Cs) 로 섞은 색을 (Cs' x αs, αs) 로 내보내고,
///   그 뒤 source-over 합성 co = cs' + cb x (1 - αs) 는 고정 블렌딩(ONE, ONE_MINUS_SRC_ALPHA)이 한다.
/// - blendModeIndex 는 BlendMode::get_shader_blend_mode_index 의 번호다. Multiply 는 렌더 타겟에 그릴 때만 MULTIPLY_SHADER_BLEND_MODE_INDEX 로 섞는다.
/// - 아래 두 셰이더 글에 concat! 으로 끼우려고 글자 그대로 내는 매크로로 둔다.
macro_rules! blend_function_source {
	() => {
"uniform highp sampler2D backdropTexture;
uniform int blendModeIndex;

float blendColorDodge(float backdropValue, float sourceValue) {
	if (backdropValue == 0.0) {
		return 0.0;
	}
	if (sourceValue == 1.0) {
		return 1.0;
	}
	return min(1.0, backdropValue / (1.0 - sourceValue));
}

float blendColorBurn(float backdropValue, float sourceValue) {
	if (backdropValue == 1.0) {
		return 1.0;
	}
	if (sourceValue == 0.0) {
		return 0.0;
	}
	return 1.0 - min(1.0, (1.0 - backdropValue) / sourceValue);
}

float blendHardLight(float backdropValue, float sourceValue) {
	if (sourceValue <= 0.5) {
		return backdropValue * 2.0 * sourceValue;
	}
	float screenValue = 2.0 * sourceValue - 1.0;
	return backdropValue + screenValue - backdropValue * screenValue;
}

float blendSoftLight(float backdropValue, float sourceValue) {
	if (sourceValue <= 0.5) {
		return backdropValue - (1.0 - 2.0 * sourceValue) * backdropValue * (1.0 - backdropValue);
	}
	float darkenedValue = sqrt(backdropValue);
	if (backdropValue <= 0.25) {
		darkenedValue = ((16.0 * backdropValue - 12.0) * backdropValue + 4.0) * backdropValue;
	}
	return backdropValue + (2.0 * sourceValue - 1.0) * (darkenedValue - backdropValue);
}

float getLuminosity(vec3 color) {
	return 0.3 * color.r + 0.59 * color.g + 0.11 * color.b;
}

vec3 clipColor(vec3 color) {
	float luminosity = getLuminosity(color);
	float minimumValue = min(min(color.r, color.g), color.b);
	float maximumValue = max(max(color.r, color.g), color.b);
	vec3 clippedColor = color;
	if (minimumValue < 0.0) {
		clippedColor = luminosity + (clippedColor - luminosity) * luminosity / (luminosity - minimumValue);
	}
	if (maximumValue > 1.0) {
		clippedColor = luminosity + (clippedColor - luminosity) * (1.0 - luminosity) / (maximumValue - luminosity);
	}
	return clippedColor;
}

vec3 setLuminosity(vec3 color, float luminosity) {
	float luminosityDelta = luminosity - getLuminosity(color);
	return clipColor(color + luminosityDelta);
}

float getSaturation(vec3 color) {
	return max(max(color.r, color.g), color.b) - min(min(color.r, color.g), color.b);
}

vec3 setSaturation(vec3 color, float saturation) {
	float minimumValue = min(min(color.r, color.g), color.b);
	float maximumValue = max(max(color.r, color.g), color.b);
	if (maximumValue > minimumValue) {
		return (color - minimumValue) * saturation / (maximumValue - minimumValue);
	}
	return vec3(0.0);
}

vec3 blendColors(vec3 backdropColor, vec3 sourceColor) {
	if (blendModeIndex == 1) {
		return min(backdropColor, sourceColor);
	}
	if (blendModeIndex == 2) {
		return max(backdropColor, sourceColor);
	}
	if (blendModeIndex == 3) {
		return vec3(blendHardLight(sourceColor.r, backdropColor.r), blendHardLight(sourceColor.g, backdropColor.g), blendHardLight(sourceColor.b, backdropColor.b));
	}
	if (blendModeIndex == 4) {
		return vec3(blendColorDodge(backdropColor.r, sourceColor.r), blendColorDodge(backdropColor.g, sourceColor.g), blendColorDodge(backdropColor.b, sourceColor.b));
	}
	if (blendModeIndex == 5) {
		return vec3(blendColorBurn(backdropColor.r, sourceColor.r), blendColorBurn(backdropColor.g, sourceColor.g), blendColorBurn(backdropColor.b, sourceColor.b));
	}
	if (blendModeIndex == 6) {
		return vec3(blendHardLight(backdropColor.r, sourceColor.r), blendHardLight(backdropColor.g, sourceColor.g), blendHardLight(backdropColor.b, sourceColor.b));
	}
	if (blendModeIndex == 7) {
		return vec3(blendSoftLight(backdropColor.r, sourceColor.r), blendSoftLight(backdropColor.g, sourceColor.g), blendSoftLight(backdropColor.b, sourceColor.b));
	}
	if (blendModeIndex == 8) {
		return abs(backdropColor - sourceColor);
	}
	if (blendModeIndex == 9) {
		return backdropColor + sourceColor - 2.0 * backdropColor * sourceColor;
	}
	if (blendModeIndex == 10) {
		return setLuminosity(setSaturation(sourceColor, getSaturation(backdropColor)), getLuminosity(backdropColor));
	}
	if (blendModeIndex == 11) {
		return setLuminosity(setSaturation(backdropColor, getSaturation(sourceColor)), getLuminosity(backdropColor));
	}
	if (blendModeIndex == 12) {
		return setLuminosity(sourceColor, getLuminosity(backdropColor));
	}
	if (blendModeIndex == 13) {
		return setLuminosity(backdropColor, getLuminosity(sourceColor));
	}
	if (blendModeIndex == 14) {
		return backdropColor * sourceColor;
	}
	return sourceColor;
}

vec4 blendWithBackdrop(vec4 sourceColor) {
	vec4 backdropColor = texelFetch(backdropTexture, ivec2(gl_FragCoord.xy), 0);
	float sourceAlpha = clamp(sourceColor.a, 0.0, 1.0);
	float backdropAlpha = clamp(backdropColor.a, 0.0, 1.0);
	vec3 straightSourceColor = vec3(0.0);
	if (sourceAlpha > 0.0) {
		straightSourceColor = clamp(sourceColor.rgb / sourceAlpha, 0.0, 1.0);
	}
	vec3 straightBackdropColor = vec3(0.0);
	if (backdropAlpha > 0.0) {
		straightBackdropColor = clamp(backdropColor.rgb / backdropAlpha, 0.0, 1.0);
	}
	vec3 mixedColor = clamp(blendColors(straightBackdropColor, straightSourceColor), 0.0, 1.0);
	vec3 blendedColor = (1.0 - backdropAlpha) * straightSourceColor + backdropAlpha * mixedColor;
	return vec4(blendedColor * sourceAlpha, sourceAlpha);
}
"
	};
}

/// 셰이더 섞기 프래그먼트 셰이더. (기본 셰이더와 같은 색 셈 뒤 그릴 영역 아래와 섞는다, 원본과 같은 글)
const BLEND_FRAGMENTSHADER_SOURCE: &str = concat!("#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D mainTexture;
uniform vec4 mainColor;
uniform vec4 tintColor;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
", blend_function_source!(), "
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	vec3 tintedColor = mix(textureColor.rgb, tintColor.rgb * textureColor.a, tintColor.a);
	float finalAlpha = textureColor.a * mainColor.a * globalAlpha;
	vec3 finalColor = tintedColor * mainColor.rgb * mainColor.a * globalAlpha;
	outputColor = blendWithBackdrop(vec4(finalColor, finalAlpha));
}
");

/// 파티클 셰이더 섞기 프래그먼트 셰이더. (파티클 셰이더와 같은 색 셈 뒤 그릴 영역 아래와 섞는다, 원본과 같은 글)
const PARTICLE_BLEND_FRAGMENTSHADER_SOURCE: &str = concat!("#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
in vec4 fragmentColor;
uniform sampler2D mainTexture;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
", blend_function_source!(), "
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	float finalAlpha = textureColor.a * fragmentColor.a * globalAlpha;
	vec3 finalColor = textureColor.rgb * fragmentColor.rgb * fragmentColor.a * globalAlpha;
	outputColor = blendWithBackdrop(vec4(finalColor, finalAlpha));
}
");

/// Multiply 를 셰이더로 섞을 때의 번호. (blend_function_source 의 blendModeIndex, 원본 MULTIPLY_SHADER_BLEND_MODE_INDEX)
/// - Multiply 고정 블렌딩(DST_COLOR, ONE_MINUS_SRC_ALPHA)은 바탕이 불투명할 때만 W3C 식과 같다. 바탕 알파가 1 보다 작으면 cs x (1 - αb) 항이 빠지고
///   알파가 바탕 알파로 남는다. 기본 프레임버퍼는 바탕 알파가 늘 1 이라(apply_color_mask) 고정 블렌딩으로 그리고, 렌더 타겟에 그릴 때만 이 번호로 셰이더로 섞는다.
const MULTIPLY_SHADER_BLEND_MODE_INDEX: i32 = 14;

/// 셰이더 섞기로 파티클을 그릴 때 한 번에 그리는 쿼드 수 상한. (서로 겹치지 않는 쿼드끼리 한 번 떠 와 함께 그린다, 겹침을 따질 때 견주는 수)
const SHADER_BLEND_PARTICLE_GROUP_CAPACITY: usize = 32;

/// 기본 프레임버퍼의 색 버퍼 이름. (붙인 색 텍스처의 형식을 물을 때. 데스크탑 GL 3.3 은 BACK_LEFT, WebGL2 는 BACK 을 받는다)
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_FRAMEBUFFER_COLOR_BUFFER: u32 = glow::BACK_LEFT;
#[cfg(target_arch = "wasm32")]
const DEFAULT_FRAMEBUFFER_COLOR_BUFFER: u32 = glow::BACK;

/// 버텍스 1개당 float 수. (x, y, u, v)
pub const FLOATS_PER_VERTEX: usize = 4;
/// 파티클 버텍스 1개당 float 수. (x, y, u, v, r, g, b, a)
pub const PARTICLE_FLOATS_PER_VERTEX: usize = 8;
const PARTICLE_VERTEX_CAPACITY: usize = 8192;
/// 묶어 그리기 버텍스 1개당 float 수. (x, y, u, v, r, g, b, a, 틴트 r, g, b, a, 텍스처 칸, 자르기 칸)
const BATCH_FLOATS_PER_VERTEX: usize = 14;

/// 스크래치 버텍스 버퍼 용량. (버텍스 수)
const VERTEX_CAPACITY: usize = 4096;

/// 묶어 그리기 버퍼 용량. (버텍스 수, 넘으면 내보내고 다시 모은다)
const BATCH_VERTEX_CAPACITY: usize = 6 * 8192;

/// 라운드 사각형 / 원의 코너 아크 분할 수.
const CORNER_SEGMENT_COUNT: usize = 8;
const ROUND_RECT_POSITION_COUNT: usize = 4 * (CORNER_SEGMENT_COUNT + 1);

/// 둥근 사각형 꼭짓점의 단위 원 좌표 표. (코너 4 개 x 분할 9 개, 우상 → 우하 → 좌하 → 좌상)
fn build_round_rect_unit_table() -> [(f64, f64); ROUND_RECT_POSITION_COUNT] {
	let start_radian_list: [f64; 4] = [-math::PI / 2.0, 0.0, math::PI / 2.0, math::PI];
	let mut unit_table = [(0.0, 0.0); ROUND_RECT_POSITION_COUNT];
	for (corner_index, start_radian) in start_radian_list.into_iter().enumerate() {
		for segment_index in 0..=CORNER_SEGMENT_COUNT {
			let radian = start_radian + (math::PI / 2.0) * (segment_index as f64 / CORNER_SEGMENT_COUNT as f64);
			unit_table[corner_index * (CORNER_SEGMENT_COUNT + 1) + segment_index] = (radian.cos(), radian.sin());
		}
	}
	unit_table
}

/// 텍스트 베이크 스케일 상한. (과도한 확대 굽기 방지)
const MAXIMUM_TEXT_BAKE_SCALE: f64 = 8.0;
const DEFAULT_TEXT_BAKE_SCALE_STEP: f64 = 0.25;

/// 부드러운 원 텍스처 크기.
const SOFT_DISC_TEXTURE_SIZE: usize = 64;

/// 원본 이미지가 사라진 텍스처를 치우는 간격. (프레임)
const IMAGE_TEXTURE_SWEEP_INTERVAL: u64 = 600;

//==============================================================================
/// 블렌드 모드. (원본 SpriteBlendMode, Canvas2D globalCompositeOperation 대응)
/// - 26 가지 모두 그린다. 13 가지(포터 더프 연산자, Lighter, Multiply, Screen)는 고정 블렌딩, 나머지 13 가지는 그릴 영역 아래를 떠 와 셰이더로 섞는다. (원본과 같음)
/// - Canvas2D 는 SourceIn, SourceOut, DestinationIn, DestinationAtop, Copy 가 그린 도형 밖(자르기 안 전체)도 지우지만, Graphic 은 그린 도형 안만 바꾼다. (원본과 같음)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
	/// 기본 알파 합성. ("source-over", 고정 블렌딩)
	Normal,
	/// 바탕과 겹친 곳만 그린다. ("source-in", 바탕 알파를 마스크로, 고정 블렌딩)
	SourceIn,
	/// 바탕과 겹치지 않은 곳만 그린다. ("source-out", 고정 블렌딩)
	SourceOut,
	/// 바탕 위의 겹친 곳에만 합성한다. ("source-atop", 고정 블렌딩)
	SourceAtop,
	/// 바탕 아래에 합성한다. ("destination-over", 고정 블렌딩)
	DestinationOver,
	/// 바탕에서 겹친 곳만 남긴다. ("destination-in", 고정 블렌딩)
	DestinationIn,
	/// 바탕에서 겹치지 않은 곳만 남긴다. ("destination-out", 고정 블렌딩)
	DestinationOut,
	/// 겹친 곳은 바탕을 남기고, 바탕이 없는 곳은 그린 것을 둔다. ("destination-atop", 고정 블렌딩)
	DestinationAtop,
	/// 색을 더한다. ("lighter", 고정 블렌딩)
	Lighter,
	/// 바탕을 보지 않고 그린 것만 둔다. ("copy", 고정 블렌딩)
	Copy,
	/// 겹친 곳을 빼고 둘 다 둔다. ("xor", 고정 블렌딩)
	Xor,
	/// 곱하기 합성. ("multiply", 어두워진다, 기본 프레임버퍼는 고정 블렌딩, 렌더 타겟은 셰이더 섞기)
	Multiply,
	/// 스크린 합성. ("screen", 밝아진다, 고정 블렌딩)
	Screen,
	/// 오버레이. ("overlay", 명암을 키운다, 셰이더 섞기)
	Overlay,
	/// 어두운 쪽을 고른다. ("darken", 셰이더 섞기)
	Darken,
	/// 밝은 쪽을 고른다. ("lighten", 셰이더 섞기)
	Lighten,
	/// 컬러 닷지. ("color-dodge", 밝아진다, 셰이더 섞기)
	ColorDodge,
	/// 컬러 번. ("color-burn", 어두워진다, 셰이더 섞기)
	ColorBurn,
	/// 하드 라이트. ("hard-light", 셰이더 섞기)
	HardLight,
	/// 소프트 라이트. ("soft-light", 셰이더 섞기)
	SoftLight,
	/// 차이 합성. ("difference", 셰이더 섞기)
	Difference,
	/// 대비가 낮은 차이 합성. ("exclusion", 셰이더 섞기)
	Exclusion,
	/// 그린 것의 색조에 바탕의 채도와 밝기. ("hue", 셰이더 섞기)
	Hue,
	/// 그린 것의 채도에 바탕의 색조와 밝기. ("saturation", 셰이더 섞기)
	Saturation,
	/// 그린 것의 색조와 채도에 바탕의 밝기. ("color", 셰이더 섞기)
	Color,
	/// 그린 것의 밝기에 바탕의 색조와 채도. ("luminosity", 셰이더 섞기)
	Luminosity,
}

/// 블렌드 모드와 원본 이름(globalCompositeOperation 문자열)의 짝. (선언 차례)
const BLEND_MODE_NAME_TABLE: [(BlendMode, &str); 26] = [
	(BlendMode::Normal, "source-over"),
	(BlendMode::SourceIn, "source-in"),
	(BlendMode::SourceOut, "source-out"),
	(BlendMode::SourceAtop, "source-atop"),
	(BlendMode::DestinationOver, "destination-over"),
	(BlendMode::DestinationIn, "destination-in"),
	(BlendMode::DestinationOut, "destination-out"),
	(BlendMode::DestinationAtop, "destination-atop"),
	(BlendMode::Lighter, "lighter"),
	(BlendMode::Copy, "copy"),
	(BlendMode::Xor, "xor"),
	(BlendMode::Multiply, "multiply"),
	(BlendMode::Screen, "screen"),
	(BlendMode::Overlay, "overlay"),
	(BlendMode::Darken, "darken"),
	(BlendMode::Lighten, "lighten"),
	(BlendMode::ColorDodge, "color-dodge"),
	(BlendMode::ColorBurn, "color-burn"),
	(BlendMode::HardLight, "hard-light"),
	(BlendMode::SoftLight, "soft-light"),
	(BlendMode::Difference, "difference"),
	(BlendMode::Exclusion, "exclusion"),
	(BlendMode::Hue, "hue"),
	(BlendMode::Saturation, "saturation"),
	(BlendMode::Color, "color"),
	(BlendMode::Luminosity, "luminosity"),
];

// 블렌드 모드의 반사 값. (원본 globalCompositeOperation 글, 원본 SpriteBlendMode 값)
crate::core::reflection::impl_reflect_names!(BlendMode, BLEND_MODE_NAME_TABLE);

impl BlendMode {
	//==============================================================================
	/// 원본 이름 반환. ("source-over" 같은 문자열)
	///
	/// # 돌려줌
	/// 원본 globalCompositeOperation 이름. (Normal 은 "source-over")
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		BLEND_MODE_NAME_TABLE[*self as usize].1
	}

	//==============================================================================
	/// 원본 이름으로 찾기. (모르는 이름이면 None. 타임라인, 파티클 서술처럼 JSON 에서 읽을 때 쓴다)
	///
	/// # 인자
	/// - `name`: 원본 globalCompositeOperation 이름. ("multiply" 처럼 소문자와 하이픈, 대소문자를 가린다)
	///
	/// # 돌려줌
	/// 이름에 맞는 블렌드 모드. 모르는 이름이면 None.
	///
	/// # 예제
	/// JSON 의 이름 글을 블렌드 모드로
	/// ```rust
	/// use vanilla::BlendMode;
	///
	/// assert_eq!(BlendMode::from_name("multiply"), Some(BlendMode::Multiply));
	/// assert_eq!(BlendMode::from_name("source-over"), Some(BlendMode::Normal));
	/// assert_eq!(BlendMode::from_name("unknown"), None);
	/// assert_eq!(BlendMode::ColorDodge.get_name(), "color-dodge");
	/// ```
	//==============================================================================
	pub fn from_name(name: &str) -> Option<BlendMode> {
		BLEND_MODE_NAME_TABLE.iter().find(|(_, blend_mode_name)| *blend_mode_name == name).map(|(blend_mode, _)| *blend_mode)
	}

	//==============================================================================
	/// 고정 블렌딩 계수. ((소스 계수, 대상 계수), 프리멀티플라이드 알파 기준. 셰이더로 섞는 모드는 None)
	/// - 포터 더프 연산자는 W3C Compositing and Blending Level 1 의 9.1 장 Fa, Fb 를 그대로 계수로 쓴다. (co = cs x Fa + cb x Fb, 알파도 같은 계수라 blend_func 하나로 된다)
	///   SourceOut: Fa = 1 - αb, Fb = 0 / SourceAtop: Fa = αb, Fb = 1 - αs / DestinationAtop: Fa = 1 - αb, Fb = αs / Xor: Fa = 1 - αb, Fb = 1 - αs
	//==============================================================================
	fn get_blend_function(&self) -> Option<(u32, u32)> {
		match self {
			BlendMode::Normal => {
				Some((glow::ONE, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::Lighter => {
				Some((glow::ONE, glow::ONE))
			}
			BlendMode::Multiply => {
				Some((glow::DST_COLOR, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::Screen => {
				Some((glow::ONE, glow::ONE_MINUS_SRC_COLOR))
			}
			BlendMode::DestinationOver => {
				Some((glow::ONE_MINUS_DST_ALPHA, glow::ONE))
			}
			BlendMode::DestinationOut => {
				Some((glow::ZERO, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::SourceIn => {
				Some((glow::DST_ALPHA, glow::ZERO))
			}
			BlendMode::DestinationIn => {
				Some((glow::ZERO, glow::SRC_ALPHA))
			}
			BlendMode::Copy => {
				Some((glow::ONE, glow::ZERO))
			}
			BlendMode::SourceOut => {
				Some((glow::ONE_MINUS_DST_ALPHA, glow::ZERO))
			}
			BlendMode::SourceAtop => {
				Some((glow::DST_ALPHA, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::DestinationAtop => {
				Some((glow::ONE_MINUS_DST_ALPHA, glow::SRC_ALPHA))
			}
			BlendMode::Xor => {
				Some((glow::ONE_MINUS_DST_ALPHA, glow::ONE_MINUS_SRC_ALPHA))
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 셰이더 섞기 번호. (BLEND_FUNCTION_SOURCE 의 blendModeIndex, 고정 블렌딩이면 0. 원본 SHADER_BLEND_MODE_INDEX_TABLE)
	/// - Darken, Lighten 도 셰이더로 섞는다. blend_equation MIN / MAX 는 계수 없이 min(cs, cb), max(cs, cb) 를 내는데,
	///   W3C 식을 프리멀티플라이드로 풀면 darken 은 cs + cb - max(cs x αb, cb x αs), lighten 은 cs + cb - min(cs x αb, cb x αs) 라서 소스나 바탕이 반투명이면 같지 않다.
	///   (불투명한 바탕 cb = 0.2 에 검정을 알파 0.5 로 그리면 W3C darken 은 0.1, MIN 은 0. 흰색을 알파 0.5 로 그리면 W3C lighten 은 0.6, MAX 는 0.5)
	//==============================================================================
	fn get_shader_blend_mode_index(&self) -> i32 {
		match self {
			BlendMode::Darken => {
				1
			}
			BlendMode::Lighten => {
				2
			}
			BlendMode::Overlay => {
				3
			}
			BlendMode::ColorDodge => {
				4
			}
			BlendMode::ColorBurn => {
				5
			}
			BlendMode::HardLight => {
				6
			}
			BlendMode::SoftLight => {
				7
			}
			BlendMode::Difference => {
				8
			}
			BlendMode::Exclusion => {
				9
			}
			BlendMode::Hue => {
				10
			}
			BlendMode::Saturation => {
				11
			}
			BlendMode::Color => {
				12
			}
			BlendMode::Luminosity => {
				13
			}
			_ => {
				0
			}
		}
	}
}

//==============================================================================
/// 오프스크린 칸의 색 텍스처 형식. (셰이더 섞기에서 그릴 영역 아래를 떠 올 칸과 덧그림 칸)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum OffscreenFormat {
	Rgb8,
	Rgba8,
	Rgba16Float,
	Rgba32Float,
}

impl OffscreenFormat {
	//==============================================================================
	/// GL 텍스처 형식. (내부 형식, 픽셀 형식, 픽셀 자료형)
	//==============================================================================
	fn get_texture_format(&self) -> (u32, u32, u32) {
		match self {
			OffscreenFormat::Rgb8 => {
				(glow::RGB8, glow::RGB, glow::UNSIGNED_BYTE)
			}
			OffscreenFormat::Rgba8 => {
				(glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE)
			}
			OffscreenFormat::Rgba16Float => {
				(glow::RGBA16F, glow::RGBA, glow::HALF_FLOAT)
			}
			OffscreenFormat::Rgba32Float => {
				(glow::RGBA32F, glow::RGBA, glow::FLOAT)
			}
		}
	}
}

//==============================================================================
/// 오프스크린 칸. (색 텍스처 하나를 붙인 프레임버퍼)
//==============================================================================
#[derive(Clone, Copy)]
struct OffscreenTarget {
	texture: GlTexture,
	framebuffer: GlFramebuffer,
	width: i32,
	height: i32,
}

//==============================================================================
/// 투명도 층 칸. (화면과 같은 크기의 RGBA8 색 텍스처와 스텐실, begin_opacity_layer 가 쓰고 다 쓰면 모아 두었다 다시 쓴다)
//==============================================================================
struct OpacityLayerTarget {
	texture: GlTexture,
	framebuffer: GlFramebuffer,
	stencil_renderbuffer: glow::Renderbuffer,
	width: i32,
	height: i32,
}

//==============================================================================
/// 쌓인 투명도 층 하나. (end_opacity_layer 가 꺼내 앞 프레임버퍼에 얹는다)
//==============================================================================
struct OpacityLayer {
	/// 층 칸. (못 지었으면 None 이고, 그동안은 층 없이 그대로 그린다)
	target: Option<OpacityLayerTarget>,
	/// 층을 열기 전에 그리던 프레임버퍼.
	previous_framebuffer: Option<GlFramebuffer>,
	/// 층을 얹을 투명도. (층을 열 때의 전역 알파)
	global_alpha: f64,
}

//==============================================================================
/// 떠 올 영역. (GL 픽셀, 왼쪽 아래 원점)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
struct BackdropRect {
	x: i32,
	y: i32,
	width: i32,
	height: i32,
}

impl BackdropRect {
	//==============================================================================
	/// 다른 영역과 겹치는지. (변을 맞댄 것은 겹치지 않는다)
	//==============================================================================
	fn is_overlapping(&self, other: &BackdropRect) -> bool {
		self.x < other.x + other.width && other.x < self.x + self.width && self.y < other.y + other.height && other.y < self.y + self.height
	}

	//==============================================================================
	/// 다른 영역까지 덮는 경계 상자.
	//==============================================================================
	fn union(&self, other: &BackdropRect) -> BackdropRect {
		let left_x = self.x.min(other.x);
		let bottom_y = self.y.min(other.y);
		let right_x = (self.x + self.width).max(other.x + other.width);
		let top_y = (self.y + self.height).max(other.y + other.height);
		BackdropRect {
			x: left_x,
			y: bottom_y,
			width: right_x - left_x,
			height: top_y - bottom_y,
		}
	}
}

//==============================================================================
/// 가로 정렬.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
	/// 왼쪽 맞춤. (x 가 글의 왼쪽 끝)
	Left,
	/// 가운데 맞춤. (x 가 글의 가로 가운데)
	Center,
	/// 오른쪽 맞춤. (x 가 글의 오른쪽 끝)
	Right,
	/// 글 방향의 시작 쪽 맞춤. (Ltr 은 왼쪽, Rtl 은 오른쪽, Graphic 의 처음 값)
	Start,
	/// 글 방향의 끝 쪽 맞춤. (Ltr 은 오른쪽, Rtl 은 왼쪽)
	End,
}

// 가로 정렬의 반사 값. (원본 TextAlign 글)
crate::core::reflection::impl_reflect_names!(TextAlign, [
	(TextAlign::Left, "left"),
	(TextAlign::Center, "center"),
	(TextAlign::Right, "right"),
	(TextAlign::Start, "start"),
	(TextAlign::End, "end"),
]);

//==============================================================================
/// 세로 기준선.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBaseline {
	/// 위 끝 기준. (y 가 글꼴 어센트의 위 끝)
	Top,
	/// 가운데 기준. (y 가 어센트 위 끝과 디센트 아래 끝의 가운데)
	Middle,
	/// 아래 끝 기준. (y 가 글꼴 디센트의 아래 끝)
	Bottom,
	/// 표의 문자 기준선. (Bottom 과 같이 y 가 디센트의 아래 끝)
	Ideographic,
	/// 매달린 기준선. (y 가 기준선에서 어센트의 0.8 배 위)
	Hanging,
	/// 알파벳 기준선. (y 가 기준선, Graphic 의 처음 값)
	Alphabetic,
}

// 세로 기준선의 반사 값. (원본 TextBaseline 글)
crate::core::reflection::impl_reflect_names!(TextBaseline, [
	(TextBaseline::Top, "top"),
	(TextBaseline::Middle, "middle"),
	(TextBaseline::Bottom, "bottom"),
	(TextBaseline::Ideographic, "ideographic"),
	(TextBaseline::Hanging, "hanging"),
	(TextBaseline::Alphabetic, "alphabetic"),
]);

//==============================================================================
/// 글자 방향. (원본 "inherit" | "ltr" | "rtl" | "auto", Canvas2D direction)
/// - Inherit 는 Graphic 에서는 왼쪽에서 오른쪽으로 본다. 글자 컴포넌트에서는 그리는 때의 Graphic 방향을 따른다는 뜻이다.
/// - Auto 는 문자열마다 처음 나오는 방향이 강한 글자로 정한다. (HTML 의 dir="auto", 없으면 왼쪽에서 오른쪽)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextDirection {
	/// 물려받기. (Graphic 의 처음 값, Graphic 에서는 Ltr 로 본다)
	Inherit,
	/// 왼쪽에서 오른쪽.
	Ltr,
	/// 오른쪽에서 왼쪽.
	Rtl,
	/// 문자열마다 처음 나오는 방향이 강한 글자로 정한다. (없으면 Ltr)
	Auto,
}

// 글자 방향의 반사 값. (원본 글)
crate::core::reflection::impl_reflect_names!(TextDirection, [
	(TextDirection::Inherit, "inherit"),
	(TextDirection::Ltr, "ltr"),
	(TextDirection::Rtl, "rtl"),
	(TextDirection::Auto, "auto"),
]);

impl TextDirection {
	//==============================================================================
	/// 원본 글로 읽기. ("ltr", "rtl", "auto", 그 밖은 Inherit)
	///
	/// # 인자
	/// - `name`: 원본 글. ("inherit", "ltr", "rtl", "auto", 대소문자를 가린다)
	///
	/// # 돌려줌
	/// 읽은 방향. 모르는 글이면 Inherit.
	//==============================================================================
	pub fn from_name(name: &str) -> TextDirection {
		match name {
			"ltr" => {
				TextDirection::Ltr
			}
			"rtl" => {
				TextDirection::Rtl
			}
			"auto" => {
				TextDirection::Auto
			}
			_ => {
				TextDirection::Inherit
			}
		}
	}

	//==============================================================================
	/// 원본 글. ("inherit", "ltr", "rtl", "auto")
	///
	/// # 돌려줌
	/// 원본 글. ("inherit", "ltr", "rtl", "auto" 가운데 하나)
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		match self {
			TextDirection::Inherit => {
				"inherit"
			}
			TextDirection::Ltr => {
				"ltr"
			}
			TextDirection::Rtl => {
				"rtl"
			}
			TextDirection::Auto => {
				"auto"
			}
		}
	}

	//==============================================================================
	/// 모양 짓기 방향. (Rtl 만 오른쪽에서 왼쪽, 실제 방향으로 푼 뒤에 쓴다)
	///
	/// # 돌려줌
	/// Rtl 이면 `ShapingDirection::RightToLeft`, 그 밖(Inherit, Auto 포함)은 `ShapingDirection::LeftToRight`.
	//==============================================================================
	pub fn to_shaping_direction(&self) -> ShapingDirection {
		if *self == TextDirection::Rtl {
			ShapingDirection::RightToLeft
		}
		else {
			ShapingDirection::LeftToRight
		}
	}
}

//==============================================================================
/// 오른쪽에서 왼쪽으로 쓰는 글자인지. (원본 RIGHT_TO_LEFT_CHARACTER_PATTERN: 히브리어, 아랍어, 시리아어, 타나, 은코 등과 그 표현형)
///
/// # 인자
/// - `character`: 볼 글자.
///
/// # 돌려줌
/// 코드 포인트가 U+0590 ~ U+08FF, U+FB1D ~ U+FDFF, U+FE70 ~ U+FEFF, U+10800 ~ U+10FFF, U+1E800 ~ U+1EFFF 안이면 true.
//==============================================================================
pub fn is_right_to_left_character(character: char) -> bool {
	let code_point = character as u32;
	(0x0590..=0x08FF).contains(&code_point) || (0xFB1D..=0xFDFF).contains(&code_point) || (0xFE70..=0xFEFF).contains(&code_point) || (0x10800..=0x10FFF).contains(&code_point) || (0x1E800..=0x1EFFF).contains(&code_point)
}

//==============================================================================
/// 글의 첫 강한 방향. (원본 resolveTextDirection 의 "auto": 글자 갈래 문자와 LRM, RLM, ALM 가운데 처음 것, 없으면 Ltr)
///
/// # 인자
/// - `text`: 볼 문자열.
///
/// # 돌려줌
/// 처음 나오는 강한 글자(글자 갈래 문자와 LRM, RLM, ALM)가 오른쪽에서 왼쪽 글자나 RLM(U+200F), ALM(U+061C)이면 Rtl, 그 밖이면 Ltr. 강한 글자가 없으면 Ltr.
///
/// # 예제
/// 숫자와 기호는 건너뛰고 첫 글자로 정한다
/// ```rust
/// use vanilla::{detect_text_direction, TextDirection};
///
/// assert_eq!(detect_text_direction("Hello"), TextDirection::Ltr);
/// assert_eq!(detect_text_direction("123 \u{05E9}\u{05DC}\u{05D5}\u{05DD}"), TextDirection::Rtl);
/// assert_eq!(detect_text_direction("\u{200F}abc"), TextDirection::Rtl);
/// assert_eq!(detect_text_direction("2024!"), TextDirection::Ltr);
/// ```
//==============================================================================
pub fn detect_text_direction(text: &str) -> TextDirection {
	for character in text.chars() {
		let is_strong = character.is_alphabetic() || character == '\u{200E}' || character == '\u{200F}' || character == '\u{061C}';
		if !is_strong {
			continue;
		}
		if character == '\u{200E}' {
			return TextDirection::Ltr;
		}
		if is_right_to_left_character(character) || character == '\u{200F}' || character == '\u{061C}' {
			return TextDirection::Rtl;
		}
		return TextDirection::Ltr;
	}
	TextDirection::Ltr
}

//==============================================================================
/// push_state 로 저장하는 상태. (Canvas2D save 대응)
//==============================================================================
#[derive(Clone)]
struct GraphicState {
	transform_matrix: TransformMatrix,
	global_alpha: f64,
	blend_mode: BlendMode,
	fill_color: Color,
	stroke_color: Color,
	image_tint_color: Option<Color>,
	font_string: String,
	text_align: TextAlign,
	text_baseline: TextBaseline,
	text_direction: TextDirection,
}

//==============================================================================
/// 클립 스택 항목.
//==============================================================================
struct ClipEntry {
	rect: Rect,
	round_size: f64,
	transform_matrix: TransformMatrix,
	/// 이 자르기를 셰이더로 자르는지. (false 면 스텐실)
	is_shader_clip: bool,
	/// 이 칸이 맨 위일 때 셰이더 자르기가 걸리는지. (이 자르기나 바깥 자르기가 셰이더 자르기)
	is_shader_clip_active: bool,
	/// 걸린 셰이더 자르기를 모두 겹친 사각형. (디바이스 픽셀, 왼쪽, 위, 오른쪽, 아래)
	shader_clip_rect: [f32; 4],
	/// 셰이더 자르기의 표 칸 번호와 그 칸을 적은 묶음 세대. (세대가 지금 묶음이 아니면 다시 적는다)
	shader_clip_slot: usize,
	shader_clip_slot_generation: i64,
	/// 걸린 자르기(겹친 것 모두)의 디바이스 픽셀 경계 상자. (거르기가 쓴다)
	cull_minimum_x: f64,
	cull_minimum_y: f64,
	cull_maximum_x: f64,
	cull_maximum_y: f64,
}

//==============================================================================
/// 이미지 스무딩 퀄리티. (원본 setImageSmoothingQuality 의 "low", "medium", "high")
/// - GL 은 LINEAR / NEAREST 둘뿐이라 원본처럼 보관만 한다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSmoothingQuality {
	/// 낮음. ("low")
	Low,
	/// 중간. ("medium")
	Medium,
	/// 높음. ("high", Graphic 의 처음 값)
	High,
}

//==============================================================================
/// 그래픽. (원본 Graphic, GL 컨텍스트에 Canvas2D 처럼 그리는 2D 렌더러)
/// - 엔진이 하나 들어 씬과 노드의 그리기 함수에 `&mut Graphic` 으로 넘기고, 그리기 밖에서는 `engine.get_graphic()`, `get_graphic_mut()` 로 꺼낸다. 좌표는 좌상 원점, 픽셀 단위다.
/// - 상태(변환, 전역 투명도, 블렌드 모드, 색, 글꼴, 정렬, 글자 방향)는 push_state, pop_state 로 쌓고 되돌린다. 프레임 첫머리(apply_settings)에 변환, 전역 투명도, 틴트, 블렌드 모드가 처음 값으로 돌아간다.
///
/// # 예제
/// 씬의 draw 에서 도형과 글자 그리기
/// ```rust,no_run
/// use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, TextAlign, TextBaseline, Vector2};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn draw(&mut self, graphic: &mut Graphic) {
/// 		graphic.push_state();
/// 		graphic.translate(640.0, 360.0);
/// 		graphic.rotate(0.25);
/// 		graphic.set_fill_color(Colors::WHITE);
/// 		graphic.draw_rect(Rect::create(-100.0, -60.0, 200.0, 120.0));
/// 		graphic.pop_state();
/// 		graphic.set_stroke_color("#ff8800");
/// 		graphic.draw_stroke_circle(Vector2::create(200.0, 200.0), 50.0, 4.0);
/// 		graphic.set_font_string("24px sans-serif");
/// 		graphic.set_text_align(TextAlign::Center);
/// 		graphic.set_text_baseline(TextBaseline::Middle);
/// 		graphic.set_fill_color(Colors::VANILLA);
/// 		graphic.draw_fill_text("Hello", 640.0, 620.0);
/// 	}
/// }
///
/// fn main() {
/// 	let mut engine_configuration = EngineConfiguration::new();
/// 	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
/// 	let engine = Engine::new(engine_configuration);
/// 	let main_scene = MainScene {
/// 		scene_base: SceneBase::new(),
/// 	};
/// 	if let Err(error) = engine.run(Box::new(main_scene)) {
/// 		eprintln!("{}", error);
/// 	}
/// }
/// ```
//==============================================================================
pub struct Graphic {
	gl: Rc<glow::Context>,
	is_force_gizmos_visible: bool,
	is_image_smoothing_enabled: bool,
	image_smoothing_quality: ImageSmoothingQuality,
	is_image_mipmap_enabled: bool,
	image_mipmap_bias: f64,
	shader_program: ShaderProgram,
	/// 드로우에 잠시 바꿔 끼우는 프로그램. (ShaderSprite 등)
	shader_program_override: Option<Rc<ShaderProgram>>,
	/// 이름으로 담아 둔 바꿔 끼울 프로그램. ((묶음 이름, 프로그램 이름) → 프로그램, 만들기에 실패했으면 None)
	shader_program_cache_table: HashMap<(&'static str, &'static str), Option<Rc<ShaderProgram>>>,
	/// 아래 유니폼 자리를 찾아 둔 셰이더 프로그램.
	uniform_location_program: Option<GlProgram>,
	model_matrix_location: Option<GlUniformLocation>,
	main_color_location: Option<GlUniformLocation>,
	tint_color_location: Option<GlUniformLocation>,
	global_alpha_location: Option<GlUniformLocation>,
	/// 기본 프로그램에 마지막으로 올린 틴트와 전역 투명도를 알고 있는지.
	is_uploaded_uniform_known: bool,
	/// 기본 프로그램에 마지막으로 올린 틴트. (r, g, b, a)
	uploaded_tint_color_value_list: [f64; 4],
	/// 기본 프로그램에 마지막으로 올린 전역 투명도.
	uploaded_global_alpha: f64,
	vertex_array: GlVertexArray,
	vertex_buffer: GlBuffer,
	vertex_data: Vec<f32>,
	white_texture: GlTexture,
	particle_shader_program: Option<ShaderProgram>,
	particle_vertex_array: Option<GlVertexArray>,
	particle_vertex_buffer: Option<GlBuffer>,
	particle_vertex_data: Vec<f32>,
	soft_disc_texture: Option<GlTexture>,
	batch_shader_program: Option<ShaderProgram>,
	batch_vertex_array: Option<GlVertexArray>,
	batch_vertex_buffer: Option<GlBuffer>,
	image_texture_cache: ImageTextureCache,
	text_string_texture_cache: TextStringTextureCache,
	transform_matrix: TransformMatrix,
	/// 상태 스택. (앞의 state_stack_depth 개가 쓰는 칸, 뒤 칸은 폰트 문자열 저장소를 다시 쓰려고 남겨 둔다)
	state_stack: Vec<GraphicState>,
	state_stack_depth: usize,
	global_alpha: f64,
	blend_mode: BlendMode,
	/// 지금 블렌드 모드의 셰이더 섞기 번호. (0 이면 고정 블렌딩)
	shader_blend_mode_index: i32,
	/// 셰이더 섞기 프로그램과 파티클 셰이더 섞기 프로그램. (처음 쓸 때 만든다)
	blend_shader_program: Option<ShaderProgram>,
	particle_blend_shader_program: Option<ShaderProgram>,
	/// 그릴 영역 아래를 떠 올 칸. (텍스처 형식마다 하나)
	backdrop_target_table: HashMap<OffscreenFormat, OffscreenTarget>,
	/// 프레임버퍼마다 색 텍스처 형식. (None 은 기본 프레임버퍼, 프레임마다 잊는다)
	framebuffer_format_table: HashMap<Option<GlFramebuffer>, OffscreenFormat>,
	/// 바꿔 끼운 셰이더로 먼저 그려 둘 덧그림 칸. (처음 쓸 때 만든다)
	layer_target: Option<OffscreenTarget>,
	/// 열린 투명도 층. (begin_opacity_layer 가 쌓는다)
	opacity_layer_stack: Vec<OpacityLayer>,
	/// 다 쓴 투명도 층 칸. (다시 쓴다)
	opacity_layer_target_pool: Vec<OpacityLayerTarget>,
	/// 셰이더 섞기로 함께 그릴 파티클 쿼드의 영역. (저장소를 다시 쓴다)
	particle_group_rect_list: Vec<BackdropRect>,
	fill_color: Color,
	stroke_color: Color,
	white_color: Color,
	image_tint_color: Option<Color>,
	font_string: String,
	text_align: TextAlign,
	text_baseline: TextBaseline,
	text_direction: TextDirection,
	clip_stack: Vec<ClipEntry>,
	projection_matrix_array: [f32; 9],
	model_matrix_array: [f32; 9],
	applied_viewport_width: i32,
	applied_viewport_height: i32,
	text_bake_scale_step: f64,
	/// 배율이 움직이는 노드의 텍스트 베이크 스케일 양자화 단위. (text_bake_scale_step 이 0 일 때만 쓴다)
	text_bake_motion_scale_step: f64,
	/// 지금 그리는 노드의 배율이 지난 프레임과 달라 움직이는 중인지.
	is_text_bake_scale_moving: bool,
	is_text_pixel_snap_enabled: bool,
	draw_call_count: u32,
	draw_vertex_count: u32,
	last_frame_draw_call_count: u32,
	last_frame_draw_vertex_count: u32,
	culled_node_count: u32,
	last_frame_culled_node_count: u32,
	frame_number: u64,
	round_rect_unit_table: [(f64, f64); ROUND_RECT_POSITION_COUNT],
	is_batching_enabled: bool,
	batch_suspend_depth: u32,
	batch_vertex_data: Vec<f32>,
	batch_vertex_count: usize,
	/// 쌓인 정점이 쓰는 텍스처. (칸 번호 차례, 앞의 batch_texture_count 칸)
	batch_texture_list: [Option<GlTexture>; BATCH_TEXTURE_SLOT_LIMIT],
	batch_texture_count: usize,
	/// 텍스처 칸 수. (기기의 MAX_TEXTURE_IMAGE_UNITS 와 BATCH_TEXTURE_SLOT_LIMIT 가운데 작은 것, 묶음 자원을 지을 때 정한다)
	batch_texture_slot_count: usize,
	/// 자르기 사각형 표. (칸마다 왼쪽, 위, 오른쪽, 아래 디바이스 픽셀)
	batch_clip_rect_data: [f32; BATCH_CLIP_SLOT_CAPACITY * 4],
	/// 표에 적은 칸 수. (0 번 자르지 않는 칸 포함)
	batch_clip_rect_count: usize,
	/// 묶음을 그릴 때마다 늘어나는 번호. (자르기 칸 번호가 지금 묶음 것인지 견준다)
	batch_generation: i64,
	/// 축에 맞는 자르기(셰이더로 자르기)가 걸렸는지.
	is_shader_clip_active: bool,
	/// 걸린 셰이더 자르기를 모두 겹친 사각형. (디바이스 픽셀, 왼쪽, 위, 오른쪽, 아래)
	shader_clip_rect: [f32; 4],
	/// 지금 셰이더 자르기의 표 칸 번호. (shader_clip_slot_generation 이 지금 묶음이 아니면 다시 적는다)
	shader_clip_slot: usize,
	shader_clip_slot_generation: i64,
	/// 스텐실에 적은 자르기 수. (돌아간 자르기, 둥근 자르기)
	stencil_clip_depth: i32,
	/// 묶지 않는 그리기에 셰이더 자르기를 가위로 걸어 두었는지.
	is_clip_scissor_enabled: bool,
	/// 기본 프레임버퍼가 멀티샘플인지. (-1 모름, 0 아님, 1 멀티샘플)
	framebuffer_sample_state: i32,
	is_culling_enabled: bool,
	/// 프레임이 끝난 뒤에도 그린 것을 남기는지. (원본 생성 인자 preserveDrawingBuffer, 켜면 프레임 첫머리에 색을 비우지 않는다)
	is_drawing_buffer_preserved: bool,
}

//==============================================================================
/// run_external_pass 가 끝나면(패닉해도) 2D 상태를 되돌리는 가드.
//==============================================================================
struct ExternalPassGuard<'a> {
	graphic: &'a mut Graphic,
	/// 패스 앞에서 스텐실에 적어 둔 셰이더 자르기 수. (되돌릴 때 지운다)
	shader_clip_count: usize,
}

impl Drop for ExternalPassGuard<'_> {
	//==============================================================================
	/// 2D 상태 되돌리기.
	//==============================================================================
	fn drop(&mut self) {
		// pass 가 받은 GlContext 는 상태 캐시를 거치므로 잊지 않고 되돌린다. (원본 runExternalPass 와 같음)
		self.graphic.restore_render_state();
		if self.shader_clip_count > 0 {
			self.graphic.write_shader_clips_to_stencil(false);
		}
		else {
			self.graphic.restore_stencil_clip_state();
		}
	}
}

impl Graphic {
	//==============================================================================
	/// 생성. (preserve_drawing_buffer 는 원본 생성 인자 preserveDrawingBuffer, 엔진 설정의 같은 이름 값을 넘긴다)
	///
	/// # 인자
	/// - `gl`: 그릴 GL 컨텍스트. (Graphic 이 가진다)
	/// - `preserve_drawing_buffer`: 프레임이 끝난 뒤에도 그린 것을 남길지. (켜면 프레임 첫머리에 색을 비우지 않는다)
	///
	/// # 돌려줌
	/// 만든 그래픽. 기본 셰이더의 컴파일이나 링크, GL 객체(버텍스 어레이, 버퍼, 텍스처) 만들기에 실패하면 그 오류 글을 담은 Err.
	//==============================================================================
	pub fn new(gl: glow::Context, preserve_drawing_buffer: bool) -> Result<Graphic, String> {
		// 그림 텍스처를 되읽는 쪽(Canvas2D 의 drawImage)도 같은 컨텍스트를 쓴다.
		let gl = Rc::new(gl);
		crate::core::imagetexturecache::register_pixel_read_context(&gl);
		// 상태 캐시. (원본 WebGLStateCache.attach, 엔진의 상태 함수가 이것을 거쳐 부르고, 그리는 중 상태 읽기는 기억한 값으로 답한다)
		crate::core::glstatecache::attach(&gl);
		// 읽은 그림을 미리 올릴 수 있는 주인으로 알린다. (원본 DecodedImageStore.registerOwner)
		crate::core::decodedimagestore::register_owner(&gl);
		// 셰이더 프로그램.
		let shader_program = ShaderProgram::new(GlContext::from_context(&gl), VERTEXSHADER_SOURCE, FRAGMENTSHADER_SOURCE)?;

		// 버텍스 어레이 / 버텍스 버퍼. (스트리밍, 인터리브 x/y/u/v)
		let vertex_data = vec![0.0f32; VERTEX_CAPACITY * FLOATS_PER_VERTEX];
		let (vertex_array, vertex_buffer, white_texture) = unsafe {
			let vertex_array = crate::core::glstatecache::create_vertex_array(&gl)?;
			let vertex_buffer = crate::core::glstatecache::create_buffer(&gl)?;
			crate::core::glstatecache::bind_vertex_array(&gl, Some(vertex_array));
			crate::core::glstatecache::bind_buffer(&gl, glow::ARRAY_BUFFER, Some(vertex_buffer));
			gl.buffer_data_size(glow::ARRAY_BUFFER, (vertex_data.len() * 4) as i32, glow::DYNAMIC_DRAW);
			let stride_bytes = (FLOATS_PER_VERTEX * 4) as i32;
			let vertex_position_location = shader_program.get_attribute_location(GlContext::from_context(&gl), "vertexPosition");
			let vertex_texture_coordinate_location = shader_program.get_attribute_location(GlContext::from_context(&gl), "vertexTextureCoordinate");
			if let Some(vertex_position_location) = vertex_position_location {
				gl.enable_vertex_attrib_array(vertex_position_location);
				gl.vertex_attrib_pointer_f32(vertex_position_location, 2, glow::FLOAT, false, stride_bytes, 0);
			}
			if let Some(vertex_texture_coordinate_location) = vertex_texture_coordinate_location {
				gl.enable_vertex_attrib_array(vertex_texture_coordinate_location);
				gl.vertex_attrib_pointer_f32(vertex_texture_coordinate_location, 2, glow::FLOAT, false, stride_bytes, 8);
			}
			crate::core::glstatecache::bind_vertex_array(&gl, None);

			// 단색 도형용 1x1 흰색 텍스처.
			let white_texture = crate::core::glstatecache::create_texture(&gl)?;
			crate::core::glstatecache::bind_texture(&gl, glow::TEXTURE_2D, Some(white_texture));
			let white_pixel: [u8; 4] = [255, 255, 255, 255];
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&white_pixel)));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);

			// 고정 렌더 상태.
			crate::core::glstatecache::enable(&gl, glow::BLEND);
			crate::core::glstatecache::blend_func(&gl, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			crate::core::glstatecache::disable(&gl, glow::DEPTH_TEST);
			crate::core::glstatecache::disable(&gl, glow::CULL_FACE);

			(vertex_array, vertex_buffer, white_texture)
		};

		// 텍스처 유닛 고정.
		shader_program.use_program(GlContext::from_context(&gl));
		let main_texture_location = shader_program.get_uniform_location(GlContext::from_context(&gl), "mainTexture");
		let texture_lod_bias_location = shader_program.get_uniform_location(GlContext::from_context(&gl), "textureLodBias");
		unsafe {
			gl.uniform_1_i32(main_texture_location.as_ref(), 0);
			gl.uniform_1_f64(texture_lod_bias_location.as_ref(), 0.0);
			crate::core::glstatecache::active_texture(&gl, glow::TEXTURE0);
		}

		Ok(Graphic {
			gl,
			is_force_gizmos_visible: false,
			is_image_smoothing_enabled: true,
			image_smoothing_quality: ImageSmoothingQuality::High,
			is_image_mipmap_enabled: false,
			image_mipmap_bias: 0.0,
			shader_program,
			shader_program_override: None,
			shader_program_cache_table: HashMap::new(),
			uniform_location_program: None,
			model_matrix_location: None,
			main_color_location: None,
			tint_color_location: None,
			global_alpha_location: None,
			is_uploaded_uniform_known: false,
			uploaded_tint_color_value_list: [0.0; 4],
			uploaded_global_alpha: 1.0,
			vertex_array,
			vertex_buffer,
			vertex_data,
			white_texture,
			particle_shader_program: None,
			particle_vertex_array: None,
			particle_vertex_buffer: None,
			particle_vertex_data: Vec::new(),
			soft_disc_texture: None,
			batch_shader_program: None,
			batch_vertex_array: None,
			batch_vertex_buffer: None,
			image_texture_cache: ImageTextureCache::new(),
			text_string_texture_cache: TextStringTextureCache::new(),
			transform_matrix: TransformMatrix::identity(),
			state_stack: Vec::new(),
			state_stack_depth: 0,
			global_alpha: 1.0,
			blend_mode: BlendMode::Normal,
			shader_blend_mode_index: 0,
			blend_shader_program: None,
			particle_blend_shader_program: None,
			backdrop_target_table: HashMap::new(),
			framebuffer_format_table: HashMap::new(),
			layer_target: None,
			opacity_layer_stack: Vec::new(),
			opacity_layer_target_pool: Vec::new(),
			particle_group_rect_list: Vec::with_capacity(SHADER_BLEND_PARTICLE_GROUP_CAPACITY),
			fill_color: Color::black(),
			stroke_color: Color::black(),
			white_color: Color::white(),
			image_tint_color: None,
			font_string: String::from("10px sans-serif"),
			text_align: TextAlign::Start,
			text_baseline: TextBaseline::Alphabetic,
			text_direction: TextDirection::Inherit,
			clip_stack: Vec::new(),
			projection_matrix_array: [0.0; 9],
			model_matrix_array: [0.0; 9],
			applied_viewport_width: 0,
			applied_viewport_height: 0,
			text_bake_scale_step: DEFAULT_TEXT_BAKE_SCALE_STEP,
			text_bake_motion_scale_step: DEFAULT_TEXT_BAKE_SCALE_STEP,
			is_text_bake_scale_moving: false,
			is_text_pixel_snap_enabled: false,
			draw_call_count: 0,
			draw_vertex_count: 0,
			last_frame_draw_call_count: 0,
			last_frame_draw_vertex_count: 0,
			culled_node_count: 0,
			last_frame_culled_node_count: 0,
			frame_number: 0,
			round_rect_unit_table: build_round_rect_unit_table(),
			is_batching_enabled: true,
			batch_suspend_depth: 0,
			batch_vertex_data: vec![0.0; BATCH_VERTEX_CAPACITY * BATCH_FLOATS_PER_VERTEX],
			batch_vertex_count: 0,
			batch_texture_list: [None; BATCH_TEXTURE_SLOT_LIMIT],
			batch_texture_count: 0,
			batch_texture_slot_count: 1,
			batch_clip_rect_data: create_batch_clip_rect_data(),
			batch_clip_rect_count: 1,
			batch_generation: 0,
			is_shader_clip_active: false,
			shader_clip_rect: [0.0; 4],
			shader_clip_slot: 0,
			shader_clip_slot_generation: -1,
			stencil_clip_depth: 0,
			is_clip_scissor_enabled: false,
			framebuffer_sample_state: -1,
			is_culling_enabled: true,
			is_drawing_buffer_preserved: preserve_drawing_buffer,
		})
	}

	//==============================================================================
	/// 프레임이 끝난 뒤에도 그린 것을 남기는지. (원본 preserveDrawingBuffer)
	///
	/// # 돌려줌
	/// 생성 인자 preserve_drawing_buffer 의 값.
	//==============================================================================
	pub fn is_drawing_buffer_preserved(&self) -> bool {
		self.is_drawing_buffer_preserved
	}

	//==============================================================================
	/// 설정 반영. (매 프레임 시작 시 엔진이 호출)
	/// - 드로잉 버퍼 크기 변화 감지 및 뷰포트/프로젝션 갱신.
	/// - 프레임 시작 기준 상태로 리셋.
	/// - 브라우저는 preserveDrawingBuffer 가 꺼져 있으면 화면에 낸 뒤 드로잉 버퍼를 비워 준다. 창은 화면을 넘긴 뒤의 뒷 버퍼가 정해져 있지 않아 여기서 직접 비운다.
	///   켜져 있으면 원본처럼 색을 비우지 않는다. (원본 applySettings 는 스텐실만 비운다. 지난 프레임 그림 위에 그리고, 데스크탑은 플랫폼이 지난 프레임 그림을 되살려 둔다)
	///
	/// # 인자
	/// - `drawing_buffer_width`: 드로잉 버퍼 너비. (픽셀, 뷰포트와 프로젝션에 쓴다)
	/// - `drawing_buffer_height`: 드로잉 버퍼 높이. (픽셀)
	//==============================================================================
	pub fn apply_settings(&mut self, drawing_buffer_width: i32, drawing_buffer_height: i32) {
		self.frame_number += 1;
		self.text_string_texture_cache.begin_frame();
		self.shader_program_override = None;

		// 뷰포트/프로젝션 갱신.
		if drawing_buffer_width != self.applied_viewport_width || drawing_buffer_height != self.applied_viewport_height {
			unsafe {
				crate::core::glstatecache::viewport(&self.gl, 0, 0, drawing_buffer_width, drawing_buffer_height);
			}
			let safe_width = drawing_buffer_width.max(1) as f64;
			let safe_height = drawing_buffer_height.max(1) as f64;
			self.projection_matrix_array = [(2.0 / safe_width) as f32, 0.0, 0.0, 0.0, (-2.0 / safe_height) as f32, 0.0, -1.0, 1.0, 1.0];
			self.applied_viewport_width = drawing_buffer_width;
			self.applied_viewport_height = drawing_buffer_height;
		}

		// 프로그램/버퍼 바인딩 및 프로젝션 업로드. (바깥 GL 이 바꾼 묶음은 상태 캐시가 알므로 따로 잊지 않는다)
		self.use_shader_program(&self.shader_program);
		let projection_matrix_location = self.shader_program.get_uniform_location(GlContext::from_context(&self.gl), "projectionMatrix");
		unsafe {
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(self.vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
		}

		// 프레임 시작 기준 상태로 리셋. (프레임버퍼의 색 형식은 프레임 사이에 렌더 타겟을 다시 만들었을 수 있어 잊는다)
		self.state_stack_depth = 0;
		self.is_uploaded_uniform_known = false;
		self.framebuffer_format_table.clear();
		self.transform_matrix.set_identity();
		self.global_alpha = 1.0;
		self.image_tint_color = None;
		self.batch_vertex_count = 0;
		self.clear_batch_textures();
		self.batch_clip_rect_count = 1;
		self.apply_blend_mode(BlendMode::Normal);

		// 기본 프레임버퍼가 멀티샘플인지 처음 한 번 읽어 둔다. (프레임 첫머리는 기본 프레임버퍼를 칠한다)
		if self.framebuffer_sample_state < 0 {
			if let Some(is_multisampled) = read_default_framebuffer_multisampled(GlContext::from_context(&self.gl)) {
				self.framebuffer_sample_state = if is_multisampled {
					1
				}
				else {
					0
				};
			}
		}

		// 클리핑 상태 리셋. (비정상 종료로 스택이 남아 있어도 복구)
		unsafe {
			if !self.clip_stack.is_empty() {
				self.clip_stack.clear();
				crate::core::glstatecache::disable(&self.gl, glow::STENCIL_TEST);
			}
			self.stencil_clip_depth = 0;
			self.is_shader_clip_active = false;
			self.shader_clip_slot = 0;
			if self.is_clip_scissor_enabled {
				crate::core::glstatecache::disable(&self.gl, glow::SCISSOR_TEST);
				self.is_clip_scissor_enabled = false;
			}
			crate::core::glstatecache::color_mask(&self.gl, true, true, true, true);
			self.gl.clear_stencil(0);
			if self.is_drawing_buffer_preserved {
				self.gl.clear(glow::STENCIL_BUFFER_BIT);
			}
			else {
				crate::core::glstatecache::clear_color(&self.gl, 0.0, 0.0, 0.0, 1.0);
				self.gl.clear(glow::COLOR_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
			}
		}
	}

	//==============================================================================
	/// 프레임 마무리. (엔진이 화면을 넘기기 전에 호출)
	//==============================================================================
	pub fn end_frame(&mut self) {
		self.flush_batch();
		if self.frame_number.is_multiple_of(IMAGE_TEXTURE_SWEEP_INTERVAL) {
			self.image_texture_cache.sweep(GlContext::from_context(&self.gl));
		}
		self.release_color_mask();
	}

	//==============================================================================
	/// 셰이더 프로그램을 사용 중으로 건다. (원본 useShaderProgram, 상태 캐시가 이미 이 프로그램을 쓴다고 기억하면 ShaderProgram::use_program 이 건너뛴다)
	/// - Graphic 은 쓰게 한 프로그램을 따로 기억하지 않는다. 게임이 같은 컨텍스트로 다른 프로그램을 쓰게 해도 다음 그리기가 제 프로그램을 다시 건다.
	//==============================================================================
	fn use_shader_program(&self, shader_program: &ShaderProgram) {
		shader_program.use_program(GlContext::from_context(&self.gl));
	}

	//==============================================================================
	/// 텍스처 유닛 0 에 텍스처를 바인드한다. (원본 bindTexture, 상태 캐시가 이미 묶였다고 기억하면 다시 묶지 않는다)
	/// - 활성 유닛이 0 번이 아니면 0 번으로 돌린다. (게임이 다른 유닛을 활성으로 두고 돌아와도 0 번에 묶는다)
	//==============================================================================
	fn bind_main_texture(&self, texture: GlTexture) {
		unsafe {
			if !crate::core::glstatecache::is_active_texture(&self.gl, glow::TEXTURE0) {
				crate::core::glstatecache::active_texture(&self.gl, glow::TEXTURE0);
			}
			if crate::core::glstatecache::is_texture_bound(&self.gl, glow::TEXTURE0, glow::TEXTURE_2D, Some(texture)) {
				return;
			}
			crate::core::glstatecache::bind_texture(&self.gl, glow::TEXTURE_2D, Some(texture));
		}
	}

	//==============================================================================
	/// 2D 그리기 상태 맞추기. (원본 applyDrawState, 그리기 바로 앞에서 부른다)
	/// - 섞기(켬, FUNC_ADD, 지금 블렌드 모드의 인자), 깊이 시험과 면 가리기 끔, 드로잉 버퍼 전체 뷰포트, 자르기 가위가 아니면 가위 끔.
	/// - 상태 캐시가 이미 그렇다고 기억하는 것은 다시 걸지 않는다. (바뀌지 않은 프레임에는 GL 을 더 부르지 않는다)
	///   게임이 run_external_pass 밖에서 같은 컨텍스트로 이것들을 바꿔 두고 돌아와도 다음 2D 그리기가 맞게 된다.
	//==============================================================================
	fn apply_draw_state(&self) {
		let gl: &GlContext = GlContext::from_context(&self.gl);
		unsafe {
			if !crate::core::glstatecache::is_capability_set(gl, glow::BLEND, true) {
				crate::core::glstatecache::enable(gl, glow::BLEND);
			}
			if !crate::core::glstatecache::is_capability_set(gl, glow::DEPTH_TEST, false) {
				crate::core::glstatecache::disable(gl, glow::DEPTH_TEST);
			}
			if !crate::core::glstatecache::is_capability_set(gl, glow::CULL_FACE, false) {
				crate::core::glstatecache::disable(gl, glow::CULL_FACE);
			}
			if !self.is_clip_scissor_enabled && !crate::core::glstatecache::is_capability_set(gl, glow::SCISSOR_TEST, false) {
				crate::core::glstatecache::disable(gl, glow::SCISSOR_TEST);
			}
			if !crate::core::glstatecache::is_blend_equation(gl, glow::FUNC_ADD) {
				crate::core::glstatecache::blend_equation(gl, glow::FUNC_ADD);
			}
			let (source_factor, destination_factor) = self.blend_mode.get_blend_function().unwrap_or((glow::ONE, glow::ONE_MINUS_SRC_ALPHA));
			if !crate::core::glstatecache::is_blend_function(gl, source_factor, destination_factor) {
				crate::core::glstatecache::blend_func(gl, source_factor, destination_factor);
			}
			if !crate::core::glstatecache::is_viewport(gl, 0, 0, self.applied_viewport_width, self.applied_viewport_height) {
				crate::core::glstatecache::viewport(gl, 0, 0, self.applied_viewport_width, self.applied_viewport_height);
			}
		}
	}

	//==============================================================================
	/// 기본 경로의 버텍스 어레이와 정점 버퍼 묶기. (원본 bindDefaultVertexArray, 상태 캐시가 이미 묶였다고 기억하면 다시 묶지 않는다)
	//==============================================================================
	fn bind_default_vertex_array(&self) {
		unsafe {
			if !crate::core::glstatecache::is_vertex_array_bound(&self.gl, Some(self.vertex_array)) {
				crate::core::glstatecache::bind_vertex_array(&self.gl, Some(self.vertex_array));
			}
			if !crate::core::glstatecache::is_array_buffer_bound(&self.gl, Some(self.vertex_buffer)) {
				crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			}
		}
	}

	//==============================================================================
	/// 색 쓰기 마스크를 모든 채널로 되돌리기. (GL 을 밖에 내줄 때와 프레임 끝, 상태 캐시가 이미 그렇다고 기억하면 부르지 않는다)
	/// - apply_color_mask 가 데스크탑 기본 프레임버퍼에서 알파를 끈다. 밖의 GL 코드(렌더 타겟에 그리는 3D 같은 것)는 모든 채널을 쓰는 마스크를 기대한다.
	///   (원본과 웹은 기본 프레임버퍼에 알파가 없어 마스크를 바꾸지 않는다)
	//==============================================================================
	fn release_color_mask(&self) {
		if crate::core::glstatecache::is_color_mask(&self.gl, true, true, true, true) {
			return;
		}
		unsafe {
			crate::core::glstatecache::color_mask(&self.gl, true, true, true, true);
		}
	}

	//==============================================================================
	/// 색 쓰기 마스크 걸기. (기본 프레임버퍼에 그릴 때는 알파를 쓰지 않는다, 그리기 직전에 부른다)
	/// - 원본과 웹은 캔버스가 alpha: false 라 화면의 알파가 늘 1 이다. 데스크탑 기본 프레임버퍼는 알파 8 비트가 있어,
	///   알파를 낮추는 모드(source-out, xor, destination-out 같은 것) 뒤에 DST_ALPHA 를 쓰는 모드와 셰이더 섞기가 웹과 다르게 나왔다.
	///   프레임 첫머리에 알파를 1 로 비우고 기본 프레임버퍼에는 알파를 쓰지 않아 웹과 같게 한다. (웹 기본 프레임버퍼는 알파가 없어 걸어도 그대로다)
	/// - 렌더 타겟에 그릴 때는 알파도 쓴다. 지금 프레임버퍼와 마스크는 상태 캐시에 견주어, 이미 맞으면 GL 을 부르지 않는다. (따로 기억하지 않는다)
	/// - 스텐실에 클립 영역을 적는 동안은 색을 쓰지 않는 마스크가 걸려 있으므로 건드리지 않는다.
	//==============================================================================
	fn apply_color_mask(&self) {
		if self.batch_suspend_depth > 0 {
			return;
		}
		let is_alpha_written = read_framebuffer_binding(GlContext::from_context(&self.gl)).is_some();
		if crate::core::glstatecache::is_color_mask(&self.gl, true, true, true, is_alpha_written) {
			return;
		}
		unsafe {
			crate::core::glstatecache::color_mask(&self.gl, true, true, true, is_alpha_written);
		}
	}

	//==============================================================================
	/// 깊이 버퍼 비우기. (원본 clearDepthBuffer, 2D 그리기 사이에 3D 를 끼우기 전에)
	/// - 자르기(begin_clip_rect, 스텐실)를 건 뒤 그리는 도중에 깊이를 비우면 갤럭시 폴드3(Adreno 660)의 드라이버가 3D 를 하나도 그리지 못한 일이 있다. (원본 게임)
	///   자르기 안에 3D 를 그릴 노드는 begin_clip_rect 앞에서 이것을 부르고, run_external_pass 에는 is_depth_cleared 를 false 로 준다.
	//==============================================================================
	pub fn clear_depth_buffer(&mut self) {
		self.flush_batch();
		unsafe {
			crate::core::glstatecache::depth_mask(&self.gl, true);
			self.gl.clear(glow::DEPTH_BUFFER_BIT);
		}
	}

	//==============================================================================
	/// 2D 그리기 사이에 GL 패스 끼우기. (원본 runExternalPass)
	/// - 앞의 2D 묶음을 내보내고, is_depth_cleared 면 깊이를 비운 뒤 pass 를 부르고, 끝나면 restore_render_state 를 부른다.
	/// - pass 는 GL 컨텍스트를 받아 프로그램, 버텍스 어레이, 뷰포트, 섞기, 깊이, 면 가리기를 바꿔도 된다. (스텐실 자르기는 그대로 걸려 있고, 끝나면 스텐실 자르기 상태를 되돌린다)
	/// - 받은 컨텍스트(GlContext)의 상태 함수는 GL 상태 캐시를 거치므로, pass 안에서 상태를 바꾼 뒤 엔진 그리기를 이어 불러도 엔진이 바뀐 상태를 안다.
	/// - 이 함수 밖에서 같은 컨텍스트로 GL 을 써도 다음 2D 그리기가 프로그램, 버텍스 어레이, 정점 버퍼, 텍스처 유닛, 섞기, 깊이, 면 가리기, 가위, 뷰포트를 상태 캐시에 견주어 다시 건다.
	///   남는 약속은 둘이다. 2D 는 지금 묶인 프레임 버퍼에 그리므로 게임 프레임 버퍼를 묶었으면 들어올 때 것으로 돌려 두고, 스텐실 상태는 바꾸지 않는다.
	/// - pass 가 돌려준 값을 그대로 돌려준다. pass 가 패닉해도 풀리는 동안 가드가 restore_render_state 를 부른다. (원본은 던져도 되돌린다)
	///
	/// # 인자
	/// - `is_depth_cleared`: pass 앞에서 깊이 버퍼를 비울지. (자르기 안에 3D 를 그릴 때는 false 로 주고 begin_clip_rect 앞에서 clear_depth_buffer 를 부른다)
	/// - `pass`: GL 컨텍스트를 받아 그리는 함수.
	///
	/// # 돌려줌
	/// pass 가 돌려준 값.
	///
	/// # 예제
	/// GL 로 직접 그리고 값 돌려받기
	/// ```rust,no_run
	/// use vanilla::Graphic;
	///
	/// fn draw(graphic: &mut Graphic) {
	/// 	let maximum_texture_size = graphic.run_external_pass(true, |gl| {
	/// 		unsafe {
	/// 			gl.enable(glow::DEPTH_TEST);
	/// 			gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE)
	/// 		}
	/// 	});
	/// 	assert!(maximum_texture_size > 0);
	/// }
	/// ```
	//==============================================================================
	pub fn run_external_pass<R>(&mut self, is_depth_cleared: bool, pass: impl FnOnce(&GlContext) -> R) -> R {
		self.flush_batch();
		if is_depth_cleared {
			self.clear_depth_buffer();
		}

		// 셰이더 자르기는 바깥 패스가 모르므로 그동안 스텐실에 적어 둔다.
		let shader_clip_count = self.write_shader_clips_to_stencil(true);
		self.release_color_mask();
		let guard = ExternalPassGuard { graphic: self, shader_clip_count };
		pass(GlContext::from_context(&guard.graphic.gl))
	}

	//==============================================================================
	/// 이미지 스무딩 활성화 여부 설정.
	/// - 도트(픽셀) 스프라이트 게임은 false 로 설정해 선명하게 렌더링.
	///
	/// # 인자
	/// - `value`: true 면 LINEAR 로 부드럽게, false 면 NEAREST 로 픽셀 그대로. (처음 true)
	//==============================================================================
	pub fn set_image_smoothing_enabled(&mut self, value: bool) {
		self.is_image_smoothing_enabled = value;
	}

	//==============================================================================
	/// 이미지 스무딩 활성화 여부 반환.
	///
	/// # 돌려줌
	/// 스무딩이 켜져 있으면 true.
	//==============================================================================
	pub fn is_image_smoothing_enabled(&self) -> bool {
		self.is_image_smoothing_enabled
	}

	//==============================================================================
	/// 이미지 스무딩 퀄리티 설정. (GL 은 LINEAR / NEAREST 이분법이라 보관만 한다, 처음은 High)
	///
	/// # 인자
	/// - `value`: 스무딩 퀄리티. (보관만 하고 그리기에는 쓰지 않는다)
	//==============================================================================
	pub fn set_image_smoothing_quality(&mut self, value: ImageSmoothingQuality) {
		self.image_smoothing_quality = value;
	}

	//==============================================================================
	/// 이미지 스무딩 퀄리티 반환.
	///
	/// # 돌려줌
	/// 설정한 퀄리티. (처음 High)
	//==============================================================================
	pub fn get_image_smoothing_quality(&self) -> ImageSmoothingQuality {
		self.image_smoothing_quality
	}

	//==============================================================================
	/// 이미지 밉맵 사용 여부 설정. (크게 줄여 그리는 이미지의 자글거림을 줄인다)
	///
	/// # 인자
	/// - `value`: true 면 이미지 텍스처를 줄일 때 밉맵 필터를 쓴다. (처음 false)
	//==============================================================================
	pub fn set_image_mipmap_enabled(&mut self, value: bool) {
		self.is_image_mipmap_enabled = value;
	}

	//==============================================================================
	/// 이미지 밉맵 사용 여부 반환.
	///
	/// # 돌려줌
	/// 밉맵을 쓰면 true.
	//==============================================================================
	pub fn is_image_mipmap_enabled(&self) -> bool {
		self.is_image_mipmap_enabled
	}

	//==============================================================================
	/// 이미지 밉맵 단계 보정 설정. (기본 0, 원본 setImageMipmapBias)
	/// - 밉맵 단계를 고를 때 더하는 값. 음수면 한 단계 큰 그림 쪽을 골라 또렷해지고, 너무 작으면 다시 자글거린다. (-0.5 안팎)
	/// - 기본 셰이더(낱장, 묶어 그리기, 파티클)에 적용한다. 바꿔 끼운 셰이더는 textureLodBias 유니폼이 있으면 받는다.
	///
	/// # 인자
	/// - `value`: 밉맵 단계에 더할 값. (음수면 또렷하게, 0 이면 보정 없음)
	//==============================================================================
	pub fn set_image_mipmap_bias(&mut self, value: f64) {
		self.flush_batch();
		self.image_mipmap_bias = value;
		let shader_program_list: [Option<&ShaderProgram>; 6] = [Some(&self.shader_program), self.batch_shader_program.as_ref(), self.particle_shader_program.as_ref(), self.blend_shader_program.as_ref(), self.particle_blend_shader_program.as_ref(), self.shader_program_override.as_deref()];
		for shader_program in shader_program_list.into_iter().flatten() {
			self.use_shader_program(shader_program);
			self.apply_image_mipmap_bias(shader_program);
		}
		self.use_shader_program(self.get_shader_program());
	}

	//==============================================================================
	/// 이미지 밉맵 단계 보정 반환.
	///
	/// # 돌려줌
	/// 설정한 보정 값. (처음 0)
	//==============================================================================
	pub fn get_image_mipmap_bias(&self) -> f64 {
		self.image_mipmap_bias
	}

	//==============================================================================
	/// 사용 중인 셰이더 프로그램에 밉맵 단계 보정 적용. (유니폼이 없는 프로그램은 넘어간다)
	//==============================================================================
	fn apply_image_mipmap_bias(&self, shader_program: &ShaderProgram) {
		let texture_lod_bias_location = match shader_program.get_uniform_location(GlContext::from_context(&self.gl), "textureLodBias") {
			Some(texture_lod_bias_location) => {
				texture_lod_bias_location
			}
			None => {
				return;
			}
		};
		unsafe {
			self.gl.uniform_1_f64(Some(&texture_lod_bias_location), self.image_mipmap_bias);
		}
	}

	//==============================================================================
	/// 모든 노드의 기즈모를 강제로 보일지 설정.
	///
	/// # 인자
	/// - `is_force_gizmos_visible`: true 면 노드마다 기즈모 보이기 설정과 상관없이 그린다. (처음 false)
	//==============================================================================
	pub fn set_force_gizmos_visible(&mut self, is_force_gizmos_visible: bool) {
		self.is_force_gizmos_visible = is_force_gizmos_visible;
	}

	//==============================================================================
	/// 모든 노드의 기즈모를 강제로 보일지 반환.
	///
	/// # 돌려줌
	/// 강제로 보이게 했으면 true.
	//==============================================================================
	pub fn is_force_gizmos_visible(&self) -> bool {
		self.is_force_gizmos_visible
	}

	//==============================================================================
	/// 상태 저장. (Canvas2D save 대응)
	/// - 한 번 쓴 칸은 남겨 두고 다시 쓴다. 폰트 문자열은 칸의 저장소에 옮겨 적어 매번 새로 잡지 않는다.
	//==============================================================================
	pub fn push_state(&mut self) {
		if self.state_stack_depth < self.state_stack.len() {
			let state_snapshot = &mut self.state_stack[self.state_stack_depth];
			state_snapshot.transform_matrix = self.transform_matrix;
			state_snapshot.global_alpha = self.global_alpha;
			state_snapshot.blend_mode = self.blend_mode;
			state_snapshot.fill_color = self.fill_color;
			state_snapshot.stroke_color = self.stroke_color;
			state_snapshot.image_tint_color = self.image_tint_color;
			state_snapshot.font_string.clear();
			state_snapshot.font_string.push_str(&self.font_string);
			state_snapshot.text_align = self.text_align;
			state_snapshot.text_baseline = self.text_baseline;
			state_snapshot.text_direction = self.text_direction;
		}
		else {
			self.state_stack.push(GraphicState {
				transform_matrix: self.transform_matrix,
				global_alpha: self.global_alpha,
				blend_mode: self.blend_mode,
				fill_color: self.fill_color,
				stroke_color: self.stroke_color,
				image_tint_color: self.image_tint_color,
				font_string: self.font_string.clone(),
				text_align: self.text_align,
				text_baseline: self.text_baseline,
				text_direction: self.text_direction,
			});
		}
		self.state_stack_depth += 1;
	}

	//==============================================================================
	/// 상태 복원. (Canvas2D restore 대응)
	/// - 폰트 문자열은 칸의 저장소와 맞바꾼다.
	//==============================================================================
	pub fn pop_state(&mut self) {
		if self.state_stack_depth == 0 {
			return;
		}
		self.state_stack_depth -= 1;
		let state_snapshot = &mut self.state_stack[self.state_stack_depth];
		self.transform_matrix = state_snapshot.transform_matrix;
		self.global_alpha = state_snapshot.global_alpha;
		self.fill_color = state_snapshot.fill_color;
		self.stroke_color = state_snapshot.stroke_color;
		self.image_tint_color = state_snapshot.image_tint_color;
		std::mem::swap(&mut self.font_string, &mut state_snapshot.font_string);
		self.text_align = state_snapshot.text_align;
		self.text_baseline = state_snapshot.text_baseline;
		self.text_direction = state_snapshot.text_direction;
		let blend_mode = state_snapshot.blend_mode;
		if self.blend_mode != blend_mode {
			self.set_blend_mode(blend_mode);
		}
	}

	//==============================================================================
	/// 이동 변환 누적.
	///
	/// # 인자
	/// - `x`: 가로 이동. (지금 좌표계 단위, 픽셀)
	/// - `y`: 세로 이동. (아래가 +)
	//==============================================================================
	pub fn translate(&mut self, x: f64, y: f64) {
		self.transform_matrix.translate(x, y);
	}

	//==============================================================================
	/// 회전 변환 누적.
	///
	/// # 인자
	/// - `radian`: 회전 각도. (라디안, 화면에서 시계 방향이 +)
	//==============================================================================
	pub fn rotate(&mut self, radian: f64) {
		self.transform_matrix.rotate(radian);
	}

	//==============================================================================
	/// 크기 변환 누적.
	///
	/// # 인자
	/// - `x`: 가로 배율. (1 이면 그대로, 음수면 뒤집힌다)
	/// - `y`: 세로 배율.
	//==============================================================================
	pub fn scale(&mut self, x: f64, y: f64) {
		self.transform_matrix.scale(x, y);
	}

	//==============================================================================
	/// 변환 행렬 직접 설정. (Canvas2D setTransform(a, b, c, d, e, f) 대응)
	/// - 지금 행렬에 곱하지 않고 바꿔 넣는다. 점 (x, y) 는 (a x + c y + e, b x + d y + f) 로 옮겨진다.
	///
	/// # 인자
	/// - `a`: 가로 배율. (x 에 곱해 새 x 에 더한다)
	/// - `b`: 세로 기울임. (x 에 곱해 새 y 에 더한다)
	/// - `c`: 가로 기울임. (y 에 곱해 새 x 에 더한다)
	/// - `d`: 세로 배율. (y 에 곱해 새 y 에 더한다)
	/// - `e`: 가로 이동. (픽셀)
	/// - `f`: 세로 이동. (픽셀)
	//==============================================================================
	pub fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
		self.transform_matrix.set_transform(a, b, c, d, e, f);
	}

	//==============================================================================
	/// 변환 행렬 초기화.
	//==============================================================================
	pub fn reset_transform(&mut self) {
		self.transform_matrix.set_identity();
	}

	//==============================================================================
	/// 현재 변환 행렬 반환.
	///
	/// # 돌려줌
	/// 지금 변환 행렬의 복사본.
	//==============================================================================
	pub fn get_transform_matrix(&self) -> TransformMatrix {
		self.transform_matrix
	}

	//==============================================================================
	/// 전역 투명도 설정.
	///
	/// # 인자
	/// - `value`: 전역 투명도. (0 ~ 1 로 자른다, 처음 1)
	//==============================================================================
	pub fn set_global_alpha(&mut self, value: f64) {
		self.global_alpha = math::clamp01(value);
	}

	//==============================================================================
	/// 전역 투명도 반환.
	///
	/// # 돌려줌
	/// 지금 전역 투명도. (0 ~ 1)
	//==============================================================================
	pub fn get_global_alpha(&self) -> f64 {
		self.global_alpha
	}

	//==============================================================================
	/// 전역 투명도 곱셈 누적. (노드 트리의 투명도 상속용)
	///
	/// # 인자
	/// - `value`: 지금 전역 투명도에 곱할 값. (곱한 결과를 0 ~ 1 로 자른다)
	//==============================================================================
	pub fn multiply_global_alpha(&mut self, value: f64) {
		self.global_alpha = math::clamp01(self.global_alpha * value);
	}

	//==============================================================================
	/// 블렌드 모드 설정. (원본 setBlendMode)
	/// - 고정 블렌딩으로 되는 13 가지는 blend_func 로 건다.
	/// - 나머지 13 가지(Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity)는
	///   그리기마다 그릴 영역 아래를 떠 와 셰이더로 섞는다. 이 모드에서는 묶어 그리지 않고, 그리기 하나에 영역 복사 한 번과 드로우 콜 한 번이 든다.
	///   (바꿔 끼운 셰이더로 그릴 때는 덧그림 칸에 먼저 그리고 옮기므로 드로우 콜이 두 번)
	/// - Multiply 는 렌더 타겟(기본 프레임버퍼가 아닌 것)에 그릴 때만 셰이더로 섞는다. (get_draw_shader_blend_mode_index)
	///
	/// # 인자
	/// - `blend_mode`: 다음 그리기부터 쓸 블렌드 모드. (지금과 같으면 아무것도 하지 않는다, 바뀌면 모아 둔 묶음을 먼저 내보낸다)
	//==============================================================================
	pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
		if blend_mode == self.blend_mode {
			return;
		}
		self.flush_batch();
		self.apply_blend_mode(blend_mode);
	}

	//==============================================================================
	/// 블렌드 모드를 GL 에 건다. (같은 모드여도 건다)
	//==============================================================================
	fn apply_blend_mode(&mut self, blend_mode: BlendMode) {
		// 셰이더로 섞은 색은 source-over 로 합성한다.
		let (source_factor, destination_factor) = blend_mode.get_blend_function().unwrap_or((glow::ONE, glow::ONE_MINUS_SRC_ALPHA));
		self.blend_mode = blend_mode;
		self.shader_blend_mode_index = blend_mode.get_shader_blend_mode_index();
		unsafe {
			crate::core::glstatecache::blend_func(&self.gl, source_factor, destination_factor);
		}
	}

	//==============================================================================
	/// 블렌드 모드 반환.
	///
	/// # 돌려줌
	/// 지금 블렌드 모드. (프레임 첫머리에 Normal)
	//==============================================================================
	pub fn get_blend_mode(&self) -> BlendMode {
		self.blend_mode
	}

	//==============================================================================
	/// 이번 그리기의 셰이더 섞기 번호. (0 이면 고정 블렌딩, 원본 getDrawShaderBlendModeIndex)
	/// - Multiply 는 기본 프레임버퍼에서는 고정 블렌딩으로 그리고(묶어 그리기도 그대로), 렌더 타겟에 그릴 때만 셰이더로 섞는다. (MULTIPLY_SHADER_BLEND_MODE_INDEX)
	//==============================================================================
	fn get_draw_shader_blend_mode_index(&self) -> i32 {
		if self.blend_mode != BlendMode::Multiply {
			return self.shader_blend_mode_index;
		}
		if read_framebuffer_binding(GlContext::from_context(&self.gl)).is_none() {
			return 0;
		}
		MULTIPLY_SHADER_BLEND_MODE_INDEX
	}

	//==============================================================================
	/// 셰이더 섞기의 합성 계수로 바꾸거나 되돌리기. (셰이더로 섞은 색은 source-over 로 합성한다, 원본 applyShaderBlendCompositeFunction)
	/// - 셰이더로만 섞는 13 가지는 apply_blend_mode 가 이미 source-over 계수를 걸어 두므로 그대로 둔다.
	///   Multiply 는 고정 블렌딩 계수가 걸려 있어, 렌더 타겟에서 셰이더로 섞는 동안만 source-over 로 바꾸고(is_shader_blending 이 참) 다 그린 뒤 되돌린다.
	//==============================================================================
	fn apply_shader_blend_composite_function(&self, is_shader_blending: bool) {
		if self.blend_mode != BlendMode::Multiply {
			return;
		}
		let blend_function = if is_shader_blending {
			BlendMode::Normal.get_blend_function()
		}
		else {
			self.blend_mode.get_blend_function()
		};
		if let Some((source_factor, destination_factor)) = blend_function {
			unsafe {
				crate::core::glstatecache::blend_func(&self.gl, source_factor, destination_factor);
			}
		}
	}

	//==============================================================================
	/// 이미지 틴트 색상 설정. (None 이면 해제)
	/// - 이미지 실루엣(알파) 안쪽에만 tint.alpha 비율로 색을 덮는다.
	///
	/// # 인자
	/// - `color`: 덮을 색. (알파가 덮는 비율, None 이면 틴트를 끈다, 프레임 첫머리에 None)
	//==============================================================================
	pub fn set_image_tint_color(&mut self, color: Option<Color>) {
		self.image_tint_color = color;
	}

	//==============================================================================
	/// 이미지 틴트 색상 반환.
	///
	/// # 돌려줌
	/// 지금 틴트 색. 걸려 있지 않으면 None.
	//==============================================================================
	pub fn get_image_tint_color(&self) -> Option<Color> {
		self.image_tint_color
	}

	//==============================================================================
	/// 채우기 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	///
	/// # 인자
	/// - `color`: 채우기 색. (Color 나 "#rrggbb" 같은 글, 처음 검정)
	//==============================================================================
	pub fn set_fill_color(&mut self, color: impl Into<Color>) {
		self.fill_color = color.into();
	}

	//==============================================================================
	/// 채우기 색상 반환.
	///
	/// # 돌려줌
	/// 지금 채우기 색.
	//==============================================================================
	pub fn get_fill_color(&self) -> Color {
		self.fill_color
	}

	//==============================================================================
	/// 선 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	///
	/// # 인자
	/// - `color`: 선 색. (Color 나 "#rrggbb" 같은 글, 처음 검정)
	//==============================================================================
	pub fn set_stroke_color(&mut self, color: impl Into<Color>) {
		self.stroke_color = color.into();
	}

	//==============================================================================
	/// 선 색상 반환.
	///
	/// # 돌려줌
	/// 지금 선 색.
	//==============================================================================
	pub fn get_stroke_color(&self) -> Color {
		self.stroke_color
	}

	//==============================================================================
	/// 버텍스 데이터에 쿼드(삼각형 2개, 버텍스 6개) 기록.
	/// - 기록 후의 오프셋(플로트 단위)을 반환.
	/// - 스크래치 배열(get_vertex_data, 정점 4096 개, 플로트 16384 개)을 넘게 쓰면 패닉한다.
	///
	/// # 인자
	/// - `offset`: 기록을 시작할 자리. (스크래치 배열의 플로트 번호, 정점 하나에 x, y, u, v 플로트 4 개)
	/// - `left_x`: 왼쪽 x. (지금 변환 전의 좌표, 픽셀)
	/// - `top_y`: 위쪽 y. (픽셀)
	/// - `right_x`: 오른쪽 x. (픽셀)
	/// - `bottom_y`: 아래쪽 y. (픽셀)
	/// - `left_u`: 왼쪽 텍스처 좌표. (0 ~ 1)
	/// - `top_v`: 위쪽 텍스처 좌표. (0 ~ 1, 0 이 그림의 위)
	/// - `right_u`: 오른쪽 텍스처 좌표. (0 ~ 1)
	/// - `bottom_v`: 아래쪽 텍스처 좌표. (0 ~ 1)
	///
	/// # 돌려줌
	/// 기록한 뒤의 오프셋. (offset + 24, 4 로 나누면 그때까지의 정점 수)
	//==============================================================================
	pub fn write_quad(&mut self, offset: usize, left_x: f64, top_y: f64, right_x: f64, bottom_y: f64, left_u: f64, top_v: f64, right_u: f64, bottom_v: f64) -> usize {
		let quad_values: [f32; 24] = [
			left_x as f32, top_y as f32, left_u as f32, top_v as f32,
			right_x as f32, top_y as f32, right_u as f32, top_v as f32,
			left_x as f32, bottom_y as f32, left_u as f32, bottom_v as f32,
			left_x as f32, bottom_y as f32, left_u as f32, bottom_v as f32,
			right_x as f32, top_y as f32, right_u as f32, top_v as f32,
			right_x as f32, bottom_y as f32, right_u as f32, bottom_v as f32,
		];
		self.vertex_data[offset..offset + 24].copy_from_slice(&quad_values);
		offset + 24
	}

	//==============================================================================
	/// 기록된 버텍스 출력. (모든 드로우가 수렴하는 단일 지점)
	///
	/// # 인자
	/// - `vertex_count`: 그릴 정점 수. (스크래치 배열 앞에서부터, 삼각형마다 3 개, 0 이면 아무것도 하지 않는다)
	/// - `texture`: 입힐 텍스처. (단색은 get_white_texture)
	/// - `color`: 텍스처 색에 곱할 색. (그림 그대로는 get_white_color)
	///
	/// # 예제
	/// 쿼드 두 개를 써서 한 번에 그리기
	/// ```rust,no_run
	/// use vanilla::core::graphic::FLOATS_PER_VERTEX;
	/// use vanilla::{Color, Graphic};
	///
	/// fn draw(graphic: &mut Graphic) {
	/// 	let mut offset = graphic.write_quad(0, 10.0, 10.0, 110.0, 60.0, 0.0, 0.0, 1.0, 1.0);
	/// 	offset = graphic.write_quad(offset, 10.0, 80.0, 110.0, 130.0, 0.0, 0.0, 1.0, 1.0);
	/// 	let white_texture = graphic.get_white_texture();
	/// 	graphic.draw_vertices(offset / FLOATS_PER_VERTEX, white_texture, Color::create(1.0, 0.5, 0.0, 1.0));
	/// }
	/// ```
	//==============================================================================
	pub fn draw_vertices(&mut self, vertex_count: usize, texture: GlTexture, color: Color) {
		if vertex_count == 0 {
			return;
		}

		// 셰이더 섞기 모드는 그리기마다 그 아래를 떠 와 섞으므로 묶지 않는다. (클립 영역을 스텐실에 적을 때는 색을 쓰지 않으므로 그냥 그린다)
		let shader_blend_mode_index = if self.batch_suspend_depth == 0 { self.get_draw_shader_blend_mode_index() } else { 0 };
		let is_shader_blend_draw = shader_blend_mode_index > 0;

		// 묶어 그리기. (클립 영역 기록 중이 아니고 바꿔 끼운 셰이더가 없을 때. 바꿔 끼운 셰이더는 제 유니폼을 쓰므로 바로 그린다)
		let is_batchable = self.is_batching_enabled && self.batch_suspend_depth == 0 && self.shader_program_override.is_none() && !is_shader_blend_draw;
		if is_batchable && self.prepare_batch_resources() {
			self.append_batch_vertices(vertex_count, texture, color);
			return;
		}
		self.flush_batch();
		self.apply_draw_state();
		self.bind_default_vertex_array();

		// 묶지 않고 그리는 것에는 셰이더 자르기를 가위로 건다. (화소 가운데 기준이 같아 결과가 같다)
		self.apply_clip_scissor();
		if is_shader_blend_draw {
			self.apply_shader_blend_composite_function(true);
			self.draw_vertices_with_shader_blend(vertex_count, texture, color, shader_blend_mode_index);
			self.apply_shader_blend_composite_function(false);
			self.release_clip_scissor();
			return;
		}

		// 버텍스 업로드. (쓸 만큼만 새 저장소를 잡으며 올린다. 지난 드로우와 저장소가 겹치지 않는다)
		let float_count = vertex_count * FLOATS_PER_VERTEX;
		let vertex_bytes = float_slice_to_bytes(&self.vertex_data[..float_count]);
		unsafe {
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);
		}

		// 유니폼 업로드. (묶음을 내보낸 뒤에는 묶음 프로그램이 걸려 있을 수 있어 지금 쓰는 프로그램을 건다)
		self.use_shader_program(self.get_shader_program());
		self.upload_draw_uniforms(color, false);

		// 텍스처 바인드 및 출력.
		self.bind_main_texture(texture);
		self.apply_color_mask();
		unsafe {
			crate::core::glstatecache::draw_arrays(&self.gl, glow::TRIANGLES, 0, vertex_count as i32);
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += vertex_count as u32;
		self.release_clip_scissor();
	}

	//==============================================================================
	/// 묶음 자원 준비. (처음 쓸 때 짓는다. 지을 수 없으면 알리고 묶어 그리기를 꺼 낱장으로 그린다)
	///
	/// # 돌려줌
	/// 묶음에 쌓을 수 있으면 true.
	//==============================================================================
	fn prepare_batch_resources(&mut self) -> bool {
		if self.batch_shader_program.is_some() {
			return true;
		}
		match self.ensure_batch_resources() {
			Ok(()) => {
				true
			}
			Err(error) => {
				eprintln!("Graphic: batch resources failed: {}", error);
				self.is_batching_enabled = false;
				false
			}
		}
	}

	//==============================================================================
	/// 드로우 콜 하나의 유니폼 업로드. (draw_vertices 가 부른다)
	/// - 유니폼 자리는 셰이더 프로그램이 바뀔 때만 다시 찾는다.
	/// - 기본 프로그램의 틴트와 전역 투명도는 지난번에 올린 값과 같으면 다시 올리지 않는다. (프레임 시작과 프로그램이 바뀔 때 잊는다)
	/// - 바꿔 끼운 셰이더 프로그램과 섞기 셰이더 프로그램(is_blend_program)에는 늘 올린다.
	//==============================================================================
	fn upload_draw_uniforms(&mut self, color: Color, is_blend_program: bool) {
		let is_default_program = self.shader_program_override.is_none() && !is_blend_program;
		let shader_program = match (&self.blend_shader_program, &self.shader_program_override) {
			(Some(blend_shader_program), _) if is_blend_program => {
				blend_shader_program
			}
			(_, Some(shader_program_override)) => {
				shader_program_override.as_ref()
			}
			_ => {
				&self.shader_program
			}
		};
		let program = shader_program.get_program();
		if self.uniform_location_program != Some(program) {
			let model_matrix_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "modelMatrix");
			let main_color_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "mainColor");
			let tint_color_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "tintColor");
			let global_alpha_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "globalAlpha");
			self.uniform_location_program = Some(program);
			self.model_matrix_location = model_matrix_location;
			self.main_color_location = main_color_location;
			self.tint_color_location = tint_color_location;
			self.global_alpha_location = global_alpha_location;
			self.is_uploaded_uniform_known = false;
		}

		self.transform_matrix.write_to_float32_array(&mut self.model_matrix_array);

		// 틴트. (없으면 0, 0, 0, 0)
		let tint_color_value_list = match self.image_tint_color {
			Some(image_tint_color) => {
				[image_tint_color.red, image_tint_color.green, image_tint_color.blue, image_tint_color.alpha]
			}
			None => {
				[0.0, 0.0, 0.0, 0.0]
			}
		};
		let global_alpha = self.global_alpha;

		let is_uploaded_uniform_known = is_default_program && self.is_uploaded_uniform_known;
		let is_same_tint_color = is_uploaded_uniform_known && self.uploaded_tint_color_value_list == tint_color_value_list;
		let is_same_global_alpha = is_uploaded_uniform_known && self.uploaded_global_alpha == global_alpha;
		unsafe {
			self.gl.uniform_matrix_3_f32_slice(self.model_matrix_location.as_ref(), false, &self.model_matrix_array);
			self.gl.uniform_4_f64(self.main_color_location.as_ref(), color.red, color.green, color.blue, color.alpha);
			if !is_same_tint_color {
				self.gl.uniform_4_f64(self.tint_color_location.as_ref(), tint_color_value_list[0], tint_color_value_list[1], tint_color_value_list[2], tint_color_value_list[3]);
			}
			if !is_same_global_alpha {
				self.gl.uniform_1_f64(self.global_alpha_location.as_ref(), global_alpha);
			}
		}

		if is_default_program {
			self.uploaded_tint_color_value_list = tint_color_value_list;
			self.uploaded_global_alpha = global_alpha;
			self.is_uploaded_uniform_known = true;
		}
	}

	//==============================================================================
	/// 기록된 버텍스를 셰이더 섞기로 출력. (draw_vertices 가 셰이더 섞기 모드에서 부른다, 원본 drawVerticesWithShaderBlend)
	/// - 그릴 영역(정점의 경계 상자)만 지금 프레임버퍼에서 떠 온 뒤 섞기 셰이더로 그린다.
	/// - 바꿔 끼운 셰이더(ShaderSprite 등)는 섞기 함수를 모르므로, 그 셰이더로 덧그림 칸에 먼저 그리고 그 칸을 섞기 셰이더로 옮긴다.
	///   덧그림 칸에는 스텐실이 없어 자르기는 옮겨 그릴 때 걸린다.
	//==============================================================================
	fn draw_vertices_with_shader_blend(&mut self, vertex_count: usize, texture: GlTexture, color: Color, shader_blend_mode_index: i32) {
		let float_count = vertex_count * FLOATS_PER_VERTEX;
		let backdrop_rect = match measure_backdrop_rect(&self.vertex_data[..float_count], FLOATS_PER_VERTEX, self.transform_matrix, self.applied_viewport_width, self.applied_viewport_height) {
			Some(backdrop_rect) => {
				backdrop_rect
			}
			None => {
				return;
			}
		};
		let framebuffer = read_framebuffer_binding(GlContext::from_context(&self.gl));
		if let Err(error) = self.copy_backdrop(framebuffer, backdrop_rect) {
			eprintln!("Graphic: backdrop copy failed: {}", error);
			return;
		}
		let vertex_bytes = float_slice_to_bytes(&self.vertex_data[..float_count]);
		unsafe {
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);
		}
		if self.shader_program_override.is_none() {
			self.draw_with_blend_shader_program(vertex_count, texture, color, shader_blend_mode_index);
			return;
		}

		// 바꿔 끼운 셰이더로 덧그림 칸의 그릴 영역을 비우고 그린다.
		let layer_target = match self.ensure_layer_target(framebuffer) {
			Ok(layer_target) => {
				layer_target
			}
			Err(error) => {
				eprintln!("Graphic: layer target failed: {}", error);
				return;
			}
		};
		unsafe {
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, Some(layer_target.framebuffer));
			crate::core::glstatecache::color_mask(&self.gl, true, true, true, true);
			crate::core::glstatecache::enable(&self.gl, glow::SCISSOR_TEST);
			crate::core::glstatecache::scissor(&self.gl, backdrop_rect.x, backdrop_rect.y, backdrop_rect.width, backdrop_rect.height);
			self.gl.clear_buffer_f32_slice(glow::COLOR, 0, &[0.0, 0.0, 0.0, 0.0]);
			crate::core::glstatecache::disable(&self.gl, glow::SCISSOR_TEST);
		}
		// 비운 가위를 셰이더 자르기 가위로 되돌린다. (자르기가 없으면 가위를 끈다)
		// 떠 오기는 가위에 잘려 자르기 안만 떠 왔으므로, 옮겨 섞을 때 가위가 없으면 자르기 밖을 낡은 떠 온 값으로 덮는다.
		self.is_clip_scissor_enabled = false;
		self.apply_clip_scissor();
		self.use_shader_program(self.get_shader_program());
		self.upload_draw_uniforms(color, false);
		self.bind_main_texture(texture);
		unsafe {
			crate::core::glstatecache::draw_arrays(&self.gl, glow::TRIANGLES, 0, vertex_count as i32);
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, framebuffer);
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += vertex_count as u32;

		// 덧그림 칸의 그릴 영역을 단위 행렬, 흰색, 틴트 없이 섞어 옮긴다. (칸은 화면과 같은 크기라 디바이스 픽셀 그대로 짚는다)
		let viewport_width = self.applied_viewport_width as f64;
		let viewport_height = self.applied_viewport_height as f64;
		let left_x = backdrop_rect.x as f64;
		let right_x = (backdrop_rect.x + backdrop_rect.width) as f64;
		let top_y = viewport_height - (backdrop_rect.y + backdrop_rect.height) as f64;
		let bottom_y = viewport_height - backdrop_rect.y as f64;
		let layer_vertex_count = self.write_quad(0, left_x, top_y, right_x, bottom_y, left_x / viewport_width, (viewport_height - top_y) / viewport_height, right_x / viewport_width, (viewport_height - bottom_y) / viewport_height) / FLOATS_PER_VERTEX;
		let layer_vertex_bytes = float_slice_to_bytes(&self.vertex_data[..layer_vertex_count * FLOATS_PER_VERTEX]);
		unsafe {
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, layer_vertex_bytes, glow::DYNAMIC_DRAW);
		}
		let transform_matrix = self.transform_matrix;
		let global_alpha = self.global_alpha;
		let image_tint_color = self.image_tint_color;
		self.transform_matrix = TransformMatrix::identity();
		self.global_alpha = 1.0;
		self.image_tint_color = None;
		let white_color = self.white_color;
		self.draw_with_blend_shader_program(layer_vertex_count, layer_target.texture, white_color, shader_blend_mode_index);
		self.transform_matrix = transform_matrix;
		self.global_alpha = global_alpha;
		self.image_tint_color = image_tint_color;
	}

	//==============================================================================
	/// 올려 둔 버텍스를 섞기 셰이더로 그리기. (그릴 영역 아래는 이미 떠 와 유닛 1 에 바인드되어 있다)
	//==============================================================================
	fn draw_with_blend_shader_program(&mut self, vertex_count: usize, texture: GlTexture, color: Color, shader_blend_mode_index: i32) {
		if let Err(error) = self.ensure_blend_shader_program() {
			eprintln!("Graphic: blend shader program failed: {}", error);
			return;
		}
		let blend_shader_program = match &self.blend_shader_program {
			Some(blend_shader_program) => {
				blend_shader_program
			}
			None => {
				return;
			}
		};
		self.use_shader_program(blend_shader_program);
		let projection_matrix_location = blend_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "projectionMatrix");
		let blend_mode_index_location = blend_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "blendModeIndex");
		unsafe {
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_1_i32(blend_mode_index_location.as_ref(), shader_blend_mode_index);
		}
		self.upload_draw_uniforms(color, true);
		self.bind_main_texture(texture);
		self.apply_color_mask();
		unsafe {
			crate::core::glstatecache::draw_arrays(&self.gl, glow::TRIANGLES, 0, vertex_count as i32);
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += vertex_count as u32;
	}

	//==============================================================================
	/// 그릴 영역 아래 떠 오기. (지금 프레임버퍼의 그 영역을 같은 형식, 같은 자리의 칸에 blit_framebuffer 로 옮기고 유닛 1 에 바인드한다)
	/// - 같은 자리로 옮기므로 멀티샘플 기본 프레임버퍼(데스크탑의 멀티샘플 4, 웹의 안티에일리어싱)에서도 된다. copy_tex_sub_image_2d 는 멀티샘플에서 안 된다.
	/// - 렌더 타겟에 그리는 중이어도 그 색 텍스처를 바로 읽지 않고 떠 온다. 그리는 대상의 텍스처를 셰이더가 읽으면 피드백 루프라 GL 이 그리기를 막거나 결과가 정해지지 않는다.
	//==============================================================================
	fn copy_backdrop(&mut self, framebuffer: Option<GlFramebuffer>, backdrop_rect: BackdropRect) -> Result<(), String> {
		let backdrop_target = self.ensure_backdrop_target(framebuffer)?;
		let left_x = backdrop_rect.x;
		let bottom_y = backdrop_rect.y;
		let right_x = backdrop_rect.x + backdrop_rect.width;
		let top_y = backdrop_rect.y + backdrop_rect.height;
		unsafe {
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::READ_FRAMEBUFFER, framebuffer);
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::DRAW_FRAMEBUFFER, Some(backdrop_target.framebuffer));
			self.gl.blit_framebuffer(left_x, bottom_y, right_x, top_y, left_x, bottom_y, right_x, top_y, glow::COLOR_BUFFER_BIT, glow::NEAREST);
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, framebuffer);
			if !crate::core::glstatecache::is_texture_bound(&self.gl, glow::TEXTURE1, glow::TEXTURE_2D, Some(backdrop_target.texture)) {
				crate::core::glstatecache::active_texture(&self.gl, glow::TEXTURE1);
				crate::core::glstatecache::bind_texture(&self.gl, glow::TEXTURE_2D, Some(backdrop_target.texture));
				crate::core::glstatecache::active_texture(&self.gl, glow::TEXTURE0);
			}
		}
		Ok(())
	}

	//==============================================================================
	/// 프레임버퍼의 색 텍스처 형식 읽기. (웹 기본 프레임버퍼는 alpha: false 라 RGB8, 데스크탑 기본 프레임버퍼와 렌더 타겟은 RGBA8 이나 실수 RGBA16F / RGBA32F)
	//==============================================================================
	fn read_framebuffer_format(&self, framebuffer: Option<GlFramebuffer>) -> OffscreenFormat {
		let attachment = match framebuffer {
			Some(_) => {
				glow::COLOR_ATTACHMENT0
			}
			None => {
				DEFAULT_FRAMEBUFFER_COLOR_BUFFER
			}
		};
		unsafe {
			let component_type = self.gl.get_framebuffer_attachment_parameter_i32(glow::FRAMEBUFFER, attachment, glow::FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE) as u32;
			if component_type == glow::FLOAT {
				let red_size = self.gl.get_framebuffer_attachment_parameter_i32(glow::FRAMEBUFFER, attachment, glow::FRAMEBUFFER_ATTACHMENT_RED_SIZE);
				if red_size > 16 {
					return OffscreenFormat::Rgba32Float;
				}
				return OffscreenFormat::Rgba16Float;
			}
			let alpha_size = self.gl.get_framebuffer_attachment_parameter_i32(glow::FRAMEBUFFER, attachment, glow::FRAMEBUFFER_ATTACHMENT_ALPHA_SIZE);
			if alpha_size > 0 {
				OffscreenFormat::Rgba8
			}
			else {
				OffscreenFormat::Rgb8
			}
		}
	}

	//==============================================================================
	/// 그릴 영역 아래를 떠 올 칸 준비. (지금 프레임버퍼와 같은 형식, 화면과 같은 크기. 형식마다 하나, 크기가 바뀌면 다시 만든다)
	//==============================================================================
	fn ensure_backdrop_target(&mut self, framebuffer: Option<GlFramebuffer>) -> Result<OffscreenTarget, String> {
		let offscreen_format = match self.framebuffer_format_table.get(&framebuffer) {
			Some(offscreen_format) => {
				*offscreen_format
			}
			None => {
				let offscreen_format = self.read_framebuffer_format(framebuffer);
				self.framebuffer_format_table.insert(framebuffer, offscreen_format);
				offscreen_format
			}
		};
		let viewport_width = self.applied_viewport_width;
		let viewport_height = self.applied_viewport_height;
		if let Some(backdrop_target) = self.backdrop_target_table.get(&offscreen_format).copied() {
			if backdrop_target.width == viewport_width && backdrop_target.height == viewport_height {
				return Ok(backdrop_target);
			}
			self.destroy_offscreen_target(backdrop_target);
		}
		let backdrop_target = self.create_offscreen_target(framebuffer, offscreen_format, viewport_width, viewport_height)?;
		self.backdrop_target_table.insert(offscreen_format, backdrop_target);
		Ok(backdrop_target)
	}

	//==============================================================================
	/// 바꿔 끼운 셰이더로 먼저 그려 둘 덧그림 칸 준비. (화면과 같은 크기, 크기가 바뀌면 다시 만든다)
	/// - 실수 색을 그릴 수 있으면 RGBA16F 로 만든다. (데스크탑 GL 3.3 은 늘, 웹은 EXT_color_buffer_float 가 있을 때)
	///   RGBA8 이면 옮길 때 프리멀티플라이드를 되돌리며 알파가 작은 가장자리의 색이 8 비트로 뭉개져,
	///   color-dodge, color-burn, hue 처럼 나눗셈이 든 섞기가 바꿔 끼우지 않은 그리기와 15 단계까지 달라졌다. (원본 시험, RGBA16F 는 거의 2 단계 안)
	//==============================================================================
	fn ensure_layer_target(&mut self, framebuffer: Option<GlFramebuffer>) -> Result<OffscreenTarget, String> {
		let viewport_width = self.applied_viewport_width;
		let viewport_height = self.applied_viewport_height;
		if let Some(layer_target) = self.layer_target {
			if layer_target.width == viewport_width && layer_target.height == viewport_height {
				return Ok(layer_target);
			}
			self.destroy_offscreen_target(layer_target);
			self.layer_target = None;
		}
		#[cfg(not(target_arch = "wasm32"))]
		let offscreen_format = OffscreenFormat::Rgba16Float;
		#[cfg(target_arch = "wasm32")]
		let offscreen_format = if self.gl.supported_extensions().contains("EXT_color_buffer_float") {
			OffscreenFormat::Rgba16Float
		}
		else {
			OffscreenFormat::Rgba8
		};
		let layer_target = self.create_offscreen_target(framebuffer, offscreen_format, viewport_width, viewport_height)?;
		self.layer_target = Some(layer_target);
		Ok(layer_target)
	}

	//==============================================================================
	/// 오프스크린 칸 만들기. (색 텍스처 하나를 붙인 프레임버퍼, 거르기 NEAREST. 만든 뒤 framebuffer 를 다시 바인드한다)
	//==============================================================================
	fn create_offscreen_target(&self, framebuffer: Option<GlFramebuffer>, offscreen_format: OffscreenFormat, width: i32, height: i32) -> Result<OffscreenTarget, String> {
		let (internal_format, pixel_format, pixel_type) = offscreen_format.get_texture_format();
		unsafe {
			let texture = crate::core::glstatecache::create_texture(&self.gl)?;
			self.bind_main_texture(texture);
			self.gl.tex_image_2d(glow::TEXTURE_2D, 0, internal_format as i32, width, height, 0, pixel_format, pixel_type, glow::PixelUnpackData::Slice(None));
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			let target_framebuffer = crate::core::glstatecache::create_framebuffer(&self.gl)?;
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, Some(target_framebuffer));
			self.gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, framebuffer);
			Ok(OffscreenTarget {
				texture,
				framebuffer: target_framebuffer,
				width,
				height,
			})
		}
	}

	//==============================================================================
	/// 오프스크린 칸 지우기. (지운 텍스처의 묶음 기억은 상태 캐시가 잊는다. 데스크탑은 지운 이름을 다시 쓰므로 남으면 새 텍스처를 묶지 않게 된다)
	//==============================================================================
	fn destroy_offscreen_target(&self, offscreen_target: OffscreenTarget) {
		unsafe {
			crate::core::glstatecache::delete_framebuffer(&self.gl, offscreen_target.framebuffer);
			crate::core::glstatecache::delete_texture(&self.gl, offscreen_target.texture);
		}
	}

	//==============================================================================
	/// 섞기 셰이더 프로그램 준비. (기본 버텍스 셰이더, 어트리뷰트 위치를 기본 프로그램과 같게 고정해 기본 버텍스 어레이를 같이 쓴다)
	//==============================================================================
	fn ensure_blend_shader_program(&mut self) -> Result<(), String> {
		if self.blend_shader_program.is_some() {
			return Ok(());
		}
		let mut attribute_location_list: Vec<(&str, u32)> = Vec::new();
		for attribute_name in ["vertexPosition", "vertexTextureCoordinate"] {
			if let Some(attribute_location) = self.shader_program.get_attribute_location(GlContext::from_context(&self.gl), attribute_name) {
				attribute_location_list.push((attribute_name, attribute_location));
			}
		}
		let blend_shader_program = ShaderProgram::create_with_attribute_locations(GlContext::from_context(&self.gl), VERTEXSHADER_SOURCE, BLEND_FRAGMENTSHADER_SOURCE, &attribute_location_list)?;
		self.use_shader_program(&blend_shader_program);
		self.apply_blend_texture_units(&blend_shader_program);
		self.blend_shader_program = Some(blend_shader_program);
		Ok(())
	}

	//==============================================================================
	/// 파티클 섞기 셰이더 프로그램 준비. (파티클 버텍스 셰이더, 어트리뷰트 위치를 파티클 프로그램과 같게 고정해 파티클 버텍스 어레이를 같이 쓴다)
	//==============================================================================
	fn ensure_particle_blend_shader_program(&mut self) -> Result<(), String> {
		if self.particle_blend_shader_program.is_some() {
			return Ok(());
		}
		let particle_shader_program = match &self.particle_shader_program {
			Some(particle_shader_program) => {
				particle_shader_program
			}
			None => {
				return Err(String::from("파티클 프로그램이 없습니다"));
			}
		};
		let mut attribute_location_list: Vec<(&str, u32)> = Vec::new();
		for attribute_name in ["vertexPosition", "vertexTextureCoordinate", "vertexColor"] {
			if let Some(attribute_location) = particle_shader_program.get_attribute_location(GlContext::from_context(&self.gl), attribute_name) {
				attribute_location_list.push((attribute_name, attribute_location));
			}
		}
		let particle_blend_shader_program = ShaderProgram::create_with_attribute_locations(GlContext::from_context(&self.gl), PARTICLE_VERTEXSHADER_SOURCE, PARTICLE_BLEND_FRAGMENTSHADER_SOURCE, &attribute_location_list)?;
		self.use_shader_program(&particle_blend_shader_program);
		self.apply_blend_texture_units(&particle_blend_shader_program);
		self.particle_blend_shader_program = Some(particle_blend_shader_program);
		Ok(())
	}

	//==============================================================================
	/// 사용 중인 섞기 셰이더 프로그램에 텍스처 유닛(그릴 그림 0 번, 떠 온 아래 1 번)과 밉맵 단계 보정 걸기.
	//==============================================================================
	fn apply_blend_texture_units(&self, shader_program: &ShaderProgram) {
		let main_texture_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "mainTexture");
		let backdrop_texture_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "backdropTexture");
		unsafe {
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
			self.gl.uniform_1_i32(backdrop_texture_location.as_ref(), 1);
		}
		self.apply_image_mipmap_bias(shader_program);
	}

	//==============================================================================
	/// 묶음에 정점 더하기. (현재 변환을 CPU 에서 곱하고, 기본 셰이더와 같은 색 셈을 정점 색 하나로 미리 곱해 틴트와 함께 정점마다 싣는다)
	//==============================================================================
	fn append_batch_vertices(&mut self, vertex_count: usize, texture: GlTexture, color: Color) {
		let texture_slot = self.prepare_batch(vertex_count, texture) as f32;
		let clip_slot = self.shader_clip_slot as f32;
		let matrix = self.transform_matrix;
		let color_alpha = color.alpha * self.global_alpha;
		let color_red = color.red * color_alpha;
		let color_green = color.green * color_alpha;
		let color_blue = color.blue * color_alpha;
		let tint_color_value_list = match self.image_tint_color {
			Some(image_tint_color) => {
				[image_tint_color.red, image_tint_color.green, image_tint_color.blue, image_tint_color.alpha]
			}
			None => {
				[0.0, 0.0, 0.0, 0.0]
			}
		};
		let mut write_offset = self.batch_vertex_count * BATCH_FLOATS_PER_VERTEX;
		for vertex_index in 0..vertex_count {
			let read_offset = vertex_index * FLOATS_PER_VERTEX;
			let position_x = self.vertex_data[read_offset];
			let position_y = self.vertex_data[read_offset + 1];
			let batch_vertex = &mut self.batch_vertex_data[write_offset..write_offset + BATCH_FLOATS_PER_VERTEX];
			batch_vertex[0] = (matrix.a * position_x as f64 + matrix.c * position_y as f64 + matrix.e) as f32;
			batch_vertex[1] = (matrix.b * position_x as f64 + matrix.d * position_y as f64 + matrix.f) as f32;
			batch_vertex[2] = self.vertex_data[read_offset + 2];
			batch_vertex[3] = self.vertex_data[read_offset + 3];
			batch_vertex[4] = color_red as f32;
			batch_vertex[5] = color_green as f32;
			batch_vertex[6] = color_blue as f32;
			batch_vertex[7] = color_alpha as f32;
			batch_vertex[8..12].copy_from_slice(&tint_color_value_list.map(|value| value as f32));
			batch_vertex[12] = texture_slot;
			batch_vertex[13] = clip_slot;
			write_offset += BATCH_FLOATS_PER_VERTEX;
		}
		self.batch_vertex_count += vertex_count;
	}

	//==============================================================================
	/// 묶음에 정점을 쌓을 자리 마련. (텍스처 칸 번호를 돌려준다, 원본 prepareSpriteBatch)
	/// - 텍스처가 묶음에 없으면 빈 칸에 넣는다. 칸이 다 찼거나, 셰이더 자르기의 새 표 칸이 없거나, 정점 자리가 모자라면 그때까지 쌓인 것을 먼저 그린다.
	/// - 셰이더 자르기의 표 칸 번호는 shader_clip_slot 에 맞춰 둔다. (자르기가 걸리지 않았으면 0)
	/// - 묶음 자원은 부르는 쪽(draw_vertices, draw_colored_quads)이 먼저 지어 둔다.
	//==============================================================================
	fn prepare_batch(&mut self, vertex_count: usize, texture: GlTexture) -> usize {
		let mut texture_slot = self.find_batch_texture_slot(texture);
		let is_texture_slot_missing = texture_slot.is_none() && self.batch_texture_count >= self.batch_texture_slot_count;
		let is_clip_slot_missing = self.is_shader_clip_active && self.shader_clip_slot_generation != self.batch_generation && self.batch_clip_rect_count >= BATCH_CLIP_SLOT_CAPACITY;
		let is_vertex_capacity_missing = self.batch_vertex_count + vertex_count > BATCH_VERTEX_CAPACITY;
		if is_texture_slot_missing || is_clip_slot_missing || is_vertex_capacity_missing {
			self.flush_batch();
			texture_slot = None;
		}
		let texture_slot = match texture_slot {
			Some(texture_slot) => {
				texture_slot
			}
			None => {
				let texture_slot = self.batch_texture_count;
				self.batch_texture_list[texture_slot] = Some(texture);
				self.batch_texture_count += 1;
				texture_slot
			}
		};
		if !self.is_shader_clip_active {
			self.shader_clip_slot = 0;
		}
		else if self.shader_clip_slot_generation != self.batch_generation {
			let clip_slot = self.batch_clip_rect_count;
			self.batch_clip_rect_data[clip_slot * 4..clip_slot * 4 + 4].copy_from_slice(&self.shader_clip_rect);
			self.batch_clip_rect_count += 1;
			self.shader_clip_slot = clip_slot;
			self.shader_clip_slot_generation = self.batch_generation;

			// 맨 위 자르기 칸에도 적어, 안쪽 자르기를 마치고 돌아올 때 같은 묶음이면 표 칸을 다시 쓴다.
			if let Some(top_entry) = self.clip_stack.last_mut() {
				top_entry.shader_clip_slot = clip_slot;
				top_entry.shader_clip_slot_generation = self.batch_generation;
			}
		}
		texture_slot
	}

	//==============================================================================
	/// 묶음에서 텍스처의 칸 번호 찾기. (없으면 None, 원본 findSpriteBatchTextureSlot)
	//==============================================================================
	fn find_batch_texture_slot(&self, texture: GlTexture) -> Option<usize> {
		(0..self.batch_texture_count).rev().find(|&slot_index| self.batch_texture_list[slot_index] == Some(texture))
	}

	//==============================================================================
	/// 묶음의 텍스처 칸 비우기.
	//==============================================================================
	fn clear_batch_textures(&mut self) {
		for slot_index in 0..self.batch_texture_count {
			self.batch_texture_list[slot_index] = None;
		}
		self.batch_texture_count = 0;
	}

	//==============================================================================
	/// 정점 색 쿼드(x, y, u, v, r, g, b, a)를 묶음에 쌓기. (draw_colored_quads 가 고정 블렌딩일 때 부른다, 원본 appendColoredQuadsToSpriteBatch)
	/// - 정점 색은 그대로 싣고 틴트 칸에 (전역 알파, 0, 0, -1) 을 적어, 묶음 셰이더가 파티클 셰이더와 같은 식으로 셈하게 한다. (화소까지 같게)
	//==============================================================================
	fn append_batch_colored_quads(&mut self, vertex_count: usize, texture: GlTexture) {
		let texture_slot = self.prepare_batch(vertex_count, texture) as f32;
		let clip_slot = self.shader_clip_slot as f32;
		let global_alpha = self.global_alpha as f32;
		let matrix = self.transform_matrix;
		let mut write_offset = self.batch_vertex_count * BATCH_FLOATS_PER_VERTEX;
		for vertex_index in 0..vertex_count {
			let read_offset = vertex_index * PARTICLE_FLOATS_PER_VERTEX;
			let particle_vertex = &self.particle_vertex_data[read_offset..read_offset + PARTICLE_FLOATS_PER_VERTEX];
			let position_x = particle_vertex[0] as f64;
			let position_y = particle_vertex[1] as f64;
			let batch_vertex = &mut self.batch_vertex_data[write_offset..write_offset + BATCH_FLOATS_PER_VERTEX];
			batch_vertex[0] = (matrix.a * position_x + matrix.c * position_y + matrix.e) as f32;
			batch_vertex[1] = (matrix.b * position_x + matrix.d * position_y + matrix.f) as f32;
			batch_vertex[2..8].copy_from_slice(&particle_vertex[2..8]);
			batch_vertex[8] = global_alpha;
			batch_vertex[9] = 0.0;
			batch_vertex[10] = 0.0;
			batch_vertex[11] = -1.0;
			batch_vertex[12] = texture_slot;
			batch_vertex[13] = clip_slot;
			write_offset += BATCH_FLOATS_PER_VERTEX;
		}
		self.batch_vertex_count += vertex_count;
	}

	//==============================================================================
	/// 모아 둔 묶음 내보내기. (GL 상태를 바꾸기 전, 프레임 끝, GL 을 직접 쓰기 전에 부른다)
	/// - 묶음의 텍스처를 0 번부터의 유닛에 묶는다. 쓰지 않는 칸의 유닛에는 흰 텍스처를 묶어, 지난 그리기의 텍스처가 지금 그리는 칸(렌더 타겟)이어도 되먹임이 생기지 않게 한다. (원본과 같음)
	//==============================================================================
	pub fn flush_batch(&mut self) {
		if self.batch_vertex_count == 0 {
			return;
		}
		let vertex_count = self.batch_vertex_count;
		let texture_count = self.batch_texture_count;
		let batch_texture_list = self.batch_texture_list;
		let first_texture = batch_texture_list[0];
		self.batch_vertex_count = 0;
		self.clear_batch_textures();
		let clip_rect_count = self.batch_clip_rect_count;
		self.batch_clip_rect_count = 1;
		self.batch_generation += 1;
		let first_texture = match first_texture {
			Some(first_texture) if texture_count > 0 => {
				first_texture
			}
			_ => {
				return;
			}
		};
		self.release_clip_scissor();
		self.apply_draw_state();
		let batch_shader_program = match &self.batch_shader_program {
			Some(batch_shader_program) => {
				batch_shader_program
			}
			None => {
				return;
			}
		};
		self.use_shader_program(batch_shader_program);
		let projection_matrix_location = batch_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "projectionMatrix");
		let framebuffer_height_location = batch_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "framebufferHeight");
		let clip_rect_list_location = batch_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "clipRectList");
		let vertex_bytes = float_slice_to_bytes(&self.batch_vertex_data[..vertex_count * BATCH_FLOATS_PER_VERTEX]);
		unsafe {
			crate::core::glstatecache::bind_vertex_array(&self.gl, self.batch_vertex_array);
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, self.batch_vertex_buffer);
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_1_f32(framebuffer_height_location.as_ref(), self.applied_viewport_height as f32);
			self.gl.uniform_4_f32_slice(clip_rect_list_location.as_ref(), &self.batch_clip_rect_data[..clip_rect_count * 4]);
		}

		// 텍스처 묶기. (상태 캐시가 이미 묶였다고 기억하는 유닛은 건너뛴다, 1 번 유닛은 셰이더 섞기의 떠 온 아래와 같이 쓴다)
		// 데스크탑 GL 은 지운 텍스처 이름을 다시 쓰지만(글자 캐시가 프레임 중에 지운다) 지울 때 상태 캐시가 그 묶음을 잊으므로, 같은 이름의 새 텍스처는 다시 묶는다.
		self.bind_main_texture(first_texture);
		let mut is_texture_unit_changed = false;
		for slot_index in 1..self.batch_texture_slot_count {
			let texture = if slot_index < texture_count {
				batch_texture_list[slot_index]
			}
			else {
				Some(self.white_texture)
			};
			let texture_unit = glow::TEXTURE0 + slot_index as u32;
			if crate::core::glstatecache::is_texture_bound(&self.gl, texture_unit, glow::TEXTURE_2D, texture) {
				continue;
			}
			unsafe {
				crate::core::glstatecache::active_texture(&self.gl, texture_unit);
				crate::core::glstatecache::bind_texture(&self.gl, glow::TEXTURE_2D, texture);
			}
			is_texture_unit_changed = true;
		}
		if is_texture_unit_changed {
			unsafe {
				crate::core::glstatecache::active_texture(&self.gl, glow::TEXTURE0);
			}
		}
		self.apply_color_mask();
		unsafe {
			crate::core::glstatecache::draw_arrays(&self.gl, glow::TRIANGLES, 0, vertex_count as i32);

			// 기본 버텍스 어레이로 복귀. (프로그램은 다음에 낱장으로 그릴 때 건다)
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(self.vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(self.vertex_buffer));
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += vertex_count as u32;
	}

	//==============================================================================
	/// 묶어 그리기 켜고 끄기. (기본 켬. 끄면 원본처럼 그리기 하나에 드로우 콜 하나)
	/// - 켜면 연이은 드로우(이미지, 도형, 글자, 정점 색 쿼드)를 정점에 변환과 색을 실어 쌓아 두었다가 한 번의 드로우 콜로 그린다.
	/// - 텍스처는 한 묶음에 여러 장(기기 텍스처 유닛 수와 16 가운데 작은 것)까지 묶어 텍스처가 바뀌어도 이어진다. (PixiJS 와 같은 꼴)
	/// - 축에 맞고 화소 경계에 놓인 자르기(begin_clip_rect)는 정점의 자르기 사각형으로 셰이더가 잘라 묶음을 끊지 않는다. (돌아갔거나 둥근 자르기는 스텐실)
	/// - 텍스처 칸이 차거나, 블렌드, 스텐실 자르기, 바꿔 끼운 셰이더가 바뀌거나 버퍼가 차면 그때까지 쌓인 것을 그리고, 프레임 끝에 엔진이 남은 것을 그린다.
	///
	/// # 인자
	/// - `is_batching_enabled`: true 면 연이은 그리기를 모아 한 번에 그린다. (끌 때 모아 둔 것을 먼저 내보낸다)
	//==============================================================================
	pub fn set_batching_enabled(&mut self, is_batching_enabled: bool) {
		if !is_batching_enabled {
			self.flush_batch();
		}
		self.is_batching_enabled = is_batching_enabled;
	}

	//==============================================================================
	/// 묶어 그리기 여부.
	///
	/// # 돌려줌
	/// 묶어 그리기가 켜져 있으면 true. (처음 true)
	//==============================================================================
	pub fn is_batching_enabled(&self) -> bool {
		self.is_batching_enabled
	}

	//==============================================================================
	/// 화면 밖 거르기 켜고 끄기. (기본 켬)
	///
	/// # 인자
	/// - `is_culling_enabled`: true 면 화면 밖 노드의 그리기를 건너뛴다.
	//==============================================================================
	pub fn set_culling_enabled(&mut self, is_culling_enabled: bool) {
		self.is_culling_enabled = is_culling_enabled;
	}

	//==============================================================================
	/// 화면 밖 거르기 여부.
	///
	/// # 돌려줌
	/// 화면 밖 거르기가 켜져 있으면 true. (처음 true)
	//==============================================================================
	pub fn is_culling_enabled(&self) -> bool {
		self.is_culling_enabled
	}

	//==============================================================================
	/// 현재 변환 기준의 사각형이 화면(드로잉 버퍼)과 걸린 자르기 안에 걸치는지. (네 꼭짓점을 변환한 경계 상자로 본다)
	/// - begin_clip_rect 로 건 자르기(겹친 것 모두)의 경계 상자 밖이면 보이지 않는 것으로 본다. (유니티 RectMask2D 처럼 스크롤 칸 밖 줄을 그리지 않는다)
	///
	/// # 인자
	/// - `rect`: 지금 변환 기준의 사각형. (로컬 좌표, 픽셀)
	///
	/// # 돌려줌
	/// 변환한 네 꼭짓점의 경계 상자가 드로잉 버퍼(0, 0 에서 너비, 높이까지)와 걸린 자르기의 경계 상자 안에 걸치면 true. (경계에 닿기만 해도 true)
	//==============================================================================
	pub fn is_local_rect_visible(&self, rect: Rect) -> bool {
		let matrix = self.transform_matrix;
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = rect.position.x + rect.size.x;
		let bottom_y = rect.position.y + rect.size.y;
		let corner_list: [(f64, f64); 4] = [(left_x, top_y), (right_x, top_y), (left_x, bottom_y), (right_x, bottom_y)];
		let mut minimum_x = f64::MAX;
		let mut minimum_y = f64::MAX;
		let mut maximum_x = f64::MIN;
		let mut maximum_y = f64::MIN;
		for (corner_x, corner_y) in corner_list {
			let transformed_x = matrix.a * corner_x + matrix.c * corner_y + matrix.e;
			let transformed_y = matrix.b * corner_x + matrix.d * corner_y + matrix.f;
			minimum_x = minimum_x.min(transformed_x);
			minimum_y = minimum_y.min(transformed_y);
			maximum_x = maximum_x.max(transformed_x);
			maximum_y = maximum_y.max(transformed_y);
		}
		let viewport_width = self.applied_viewport_width as f64;
		let viewport_height = self.applied_viewport_height as f64;

		// 걸린 자르기의 경계 상자도 본다. (자르기 밖은 보이지 않는다, 유니티 RectMask2D 와 같다)
		let mut visible_minimum_x = 0.0;
		let mut visible_minimum_y = 0.0;
		let mut visible_maximum_x = viewport_width;
		let mut visible_maximum_y = viewport_height;
		if let Some(clip_entry) = self.clip_stack.last() {
			visible_minimum_x = clip_entry.cull_minimum_x.max(visible_minimum_x);
			visible_minimum_y = clip_entry.cull_minimum_y.max(visible_minimum_y);
			visible_maximum_x = clip_entry.cull_maximum_x.min(visible_maximum_x);
			visible_maximum_y = clip_entry.cull_maximum_y.min(visible_maximum_y);
		}
		maximum_x >= visible_minimum_x && maximum_y >= visible_minimum_y && minimum_x <= visible_maximum_x && minimum_y <= visible_maximum_y
	}

	//==============================================================================
	/// 이번 프레임에 거른 노드 수 더하기. (NodeTree 가 부른다)
	///
	/// # 인자
	/// - `culled_node_count`: 더할 노드 수.
	//==============================================================================
	pub fn add_culled_node_count(&mut self, culled_node_count: u32) {
		self.culled_node_count += culled_node_count;
	}

	//==============================================================================
	/// 지난 프레임에 화면 밖이라 거른 노드 수.
	///
	/// # 돌려줌
	/// 지난 프레임에 거른 노드 수. (begin_frame_statistics 가 넘겨 둔 값)
	//==============================================================================
	pub fn get_last_frame_culled_node_count(&self) -> u32 {
		self.last_frame_culled_node_count
	}

	//==============================================================================
	/// 프레임 그리기 통계 시작.
	/// - 엔진이 매 프레임 씬을 그리기 전에 부른다. 지난 프레임의 드로우 콜 / 정점 수를 넘겨 두고 이번 프레임 값을 비운다.
	//==============================================================================
	pub fn begin_frame_statistics(&mut self) {
		self.flush_batch();
		self.last_frame_draw_call_count = self.draw_call_count;
		self.last_frame_draw_vertex_count = self.draw_vertex_count;
		self.last_frame_culled_node_count = self.culled_node_count;
		self.draw_call_count = 0;
		self.draw_vertex_count = 0;
		self.culled_node_count = 0;
	}

	//==============================================================================
	/// 지난 프레임의 드로우 콜 수.
	///
	/// # 돌려줌
	/// 지난 프레임의 드로우 콜 수. (begin_frame_statistics 가 넘겨 둔 값)
	//==============================================================================
	pub fn get_last_frame_draw_call_count(&self) -> u32 {
		self.last_frame_draw_call_count
	}

	//==============================================================================
	/// 지난 프레임에 그린 정점 수.
	///
	/// # 돌려줌
	/// 지난 프레임에 그린 정점 수.
	//==============================================================================
	pub fn get_last_frame_vertex_count(&self) -> u32 {
		self.last_frame_draw_vertex_count
	}

	//==============================================================================
	/// 지난 프레임에 그린 삼각형 수.
	///
	/// # 돌려줌
	/// 지난 프레임의 정점 수를 3 으로 나눈 값.
	//==============================================================================
	pub fn get_last_frame_triangle_count(&self) -> u32 {
		self.last_frame_draw_vertex_count / 3
	}

	//==============================================================================
	/// 묶어 그리기 자원 준비. (셰이더, 버텍스 어레이, 버텍스 버퍼. 처음 쓸 때 만든다)
	//==============================================================================
	fn ensure_batch_resources(&mut self) -> Result<(), String> {
		if self.batch_shader_program.is_some() {
			return Ok(());
		}
		let maximum_texture_unit_count = unsafe { crate::core::glstatecache::get_parameter_i32(&self.gl, glow::MAX_TEXTURE_IMAGE_UNITS) };
		let texture_slot_count = (maximum_texture_unit_count.max(1) as usize).min(BATCH_TEXTURE_SLOT_LIMIT);
		let batch_shader_program = ShaderProgram::new(GlContext::from_context(&self.gl), BATCH_VERTEXSHADER_SOURCE, &create_batch_fragment_shader_source(texture_slot_count))?;
		self.batch_texture_slot_count = texture_slot_count;
		self.clear_batch_textures();
		self.batch_clip_rect_data = create_batch_clip_rect_data();
		self.batch_clip_rect_count = 1;
		unsafe {
			let batch_vertex_array = crate::core::glstatecache::create_vertex_array(&self.gl)?;
			let batch_vertex_buffer = crate::core::glstatecache::create_buffer(&self.gl)?;
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(batch_vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(batch_vertex_buffer));
			let stride_bytes = (BATCH_FLOATS_PER_VERTEX * 4) as i32;
			let attribute_layout: [(&str, i32, i32); 5] = [("vertexPosition", 2, 0), ("vertexTextureCoordinate", 2, 8), ("vertexColor", 4, 16), ("vertexTintColor", 4, 32), ("vertexSlot", 2, 48)];
			for (attribute_name, component_count, byte_offset) in attribute_layout {
				let attribute_location = batch_shader_program.get_attribute_location(GlContext::from_context(&self.gl), attribute_name);
				if let Some(attribute_location) = attribute_location {
					self.gl.enable_vertex_attrib_array(attribute_location);
					self.gl.vertex_attrib_pointer_f32(attribute_location, component_count, glow::FLOAT, false, stride_bytes, byte_offset);
				}
			}
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(self.vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.batch_vertex_array = Some(batch_vertex_array);
			self.batch_vertex_buffer = Some(batch_vertex_buffer);
		}
		self.use_shader_program(&batch_shader_program);
		let texture_unit_list: Vec<i32> = (0..texture_slot_count as i32).collect();
		let main_texture_list_location = batch_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "mainTextureList");
		unsafe {
			self.gl.uniform_1_i32_slice(main_texture_list_location.as_ref(), &texture_unit_list);
		}
		self.apply_image_mipmap_bias(&batch_shader_program);
		self.batch_shader_program = Some(batch_shader_program);
		Ok(())
	}

	//==============================================================================
	/// 파티클 배치 자원 준비. (처음 쓸 때 만든다)
	//==============================================================================
	fn ensure_particle_batch_resources(&mut self) -> Result<(), String> {
		if self.particle_shader_program.is_some() {
			return Ok(());
		}
		let particle_shader_program = ShaderProgram::new(GlContext::from_context(&self.gl), PARTICLE_VERTEXSHADER_SOURCE, PARTICLE_FRAGMENTSHADER_SOURCE)?;
		unsafe {
			let particle_vertex_array = crate::core::glstatecache::create_vertex_array(&self.gl)?;
			let particle_vertex_buffer = crate::core::glstatecache::create_buffer(&self.gl)?;
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(particle_vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(particle_vertex_buffer));
			self.gl.buffer_data_size(glow::ARRAY_BUFFER, (PARTICLE_VERTEX_CAPACITY * PARTICLE_FLOATS_PER_VERTEX * 4) as i32, glow::DYNAMIC_DRAW);
			let stride_bytes = (PARTICLE_FLOATS_PER_VERTEX * 4) as i32;
			let attribute_layout: [(&str, i32, i32); 3] = [("vertexPosition", 2, 0), ("vertexTextureCoordinate", 2, 8), ("vertexColor", 4, 16)];
			for (attribute_name, component_count, byte_offset) in attribute_layout {
				let attribute_location = particle_shader_program.get_attribute_location(GlContext::from_context(&self.gl), attribute_name);
				if let Some(attribute_location) = attribute_location {
					self.gl.enable_vertex_attrib_array(attribute_location);
					self.gl.vertex_attrib_pointer_f32(attribute_location, component_count, glow::FLOAT, false, stride_bytes, byte_offset);
				}
			}
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(self.vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.particle_vertex_array = Some(particle_vertex_array);
			self.particle_vertex_buffer = Some(particle_vertex_buffer);
		}
		self.use_shader_program(&particle_shader_program);
		let main_texture_location = particle_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "mainTexture");
		unsafe {
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
		}
		self.apply_image_mipmap_bias(&particle_shader_program);
		self.particle_shader_program = Some(particle_shader_program);
		Ok(())
	}

	//==============================================================================
	/// 파티클 버텍스 기록용 배열 반환. (minimum_float_count 이상, 용량을 넘으면 용량까지)
	///
	/// # 인자
	/// - `minimum_float_count`: 쓸 플로트 수. (정점 하나에 PARTICLE_FLOATS_PER_VERTEX 개: x, y, u, v, r, g, b, a)
	///
	/// # 돌려줌
	/// 앞에서부터 minimum_float_count 개의 배열. (용량 정점 8192 개를 넘게 달라면 용량까지, 지난번 값이 남아 있을 수 있다)
	//==============================================================================
	pub fn get_particle_vertex_data(&mut self, minimum_float_count: usize) -> &mut [f32] {
		let capacity_float_count = PARTICLE_VERTEX_CAPACITY * PARTICLE_FLOATS_PER_VERTEX;
		let float_count = minimum_float_count.min(capacity_float_count);
		if self.particle_vertex_data.len() < float_count {
			self.particle_vertex_data.resize(float_count, 0.0);
		}
		&mut self.particle_vertex_data[..float_count]
	}

	//==============================================================================
	/// 정점마다 색을 실은 쿼드 출력. (get_particle_vertex_data 에 기록한 것)
	///
	/// # 인자
	/// - `vertex_count`: 그릴 정점 수. (쿼드마다 6 개, 용량과 get_particle_vertex_data 로 받은 길이까지만 그린다, 0 이면 아무것도 하지 않는다)
	/// - `texture`: 입힐 텍스처. (get_soft_disc_texture, get_image_texture 같은 것)
	///
	/// # 예제
	/// 정점마다 색을 실은 부드러운 원 두 개
	/// ```rust,no_run
	/// use vanilla::core::graphic::PARTICLE_FLOATS_PER_VERTEX;
	/// use vanilla::Graphic;
	///
	/// fn draw(graphic: &mut Graphic) {
	/// 	let soft_disc_texture = graphic.get_soft_disc_texture();
	/// 	let particle_list: [(f64, f64, [f64; 4]); 2] = [(100.0, 100.0, [1.0, 0.5, 0.0, 1.0]), (140.0, 120.0, [0.2, 0.6, 1.0, 0.5])];
	/// 	let half_size = 24.0;
	/// 	let corner_list: [[f64; 4]; 6] = [[-half_size, -half_size, 0.0, 0.0], [half_size, -half_size, 1.0, 0.0], [half_size, half_size, 1.0, 1.0], [-half_size, -half_size, 0.0, 0.0], [half_size, half_size, 1.0, 1.0], [-half_size, half_size, 0.0, 1.0]];
	/// 	let vertex_data = graphic.get_particle_vertex_data(particle_list.len() * 6 * PARTICLE_FLOATS_PER_VERTEX);
	/// 	let mut offset = 0;
	/// 	for (center_x, center_y, color) in particle_list {
	/// 		for corner in corner_list {
	/// 			let vertex = [center_x + corner[0], center_y + corner[1], corner[2], corner[3], color[0], color[1], color[2], color[3]];
	/// 			vertex_data[offset..offset + PARTICLE_FLOATS_PER_VERTEX].copy_from_slice(&vertex.map(|value| value as f32));
	/// 			offset += PARTICLE_FLOATS_PER_VERTEX;
	/// 		}
	/// 	}
	/// 	graphic.draw_colored_quads(offset / PARTICLE_FLOATS_PER_VERTEX, soft_disc_texture);
	/// }
	/// ```
	//==============================================================================
	pub fn draw_colored_quads(&mut self, vertex_count: usize, texture: GlTexture) {
		if vertex_count == 0 {
			return;
		}

		// 고정 블렌딩이면 묶음에 이어 쌓는다. (이미지, 도형, 글자와 같은 드로우 콜에 들어간다, 원본과 같음)
		let shader_blend_mode_index = self.get_draw_shader_blend_mode_index();
		let is_shader_blend_draw = shader_blend_mode_index > 0;
		let stored_vertex_count = vertex_count.min(self.particle_vertex_data.len() / PARTICLE_FLOATS_PER_VERTEX);
		if self.is_batching_enabled && self.batch_suspend_depth == 0 && !is_shader_blend_draw && stored_vertex_count <= BATCH_VERTEX_CAPACITY && self.prepare_batch_resources() {
			self.append_batch_colored_quads(stored_vertex_count, texture);
			return;
		}
		self.flush_batch();
		self.apply_draw_state();
		if let Err(error) = self.ensure_particle_batch_resources() {
			eprintln!("Graphic: particle resources failed: {}", error);
			return;
		}
		let vertex_count = stored_vertex_count.min(PARTICLE_VERTEX_CAPACITY);

		// 셰이더 섞기 모드는 섞기 프로그램으로 그린다.
		if is_shader_blend_draw {
			if let Err(error) = self.ensure_particle_blend_shader_program() {
				eprintln!("Graphic: particle blend shader program failed: {}", error);
				return;
			}
		}
		self.apply_clip_scissor();
		let selected_shader_program = if is_shader_blend_draw {
			&self.particle_blend_shader_program
		}
		else {
			&self.particle_shader_program
		};
		let particle_shader_program = match selected_shader_program {
			Some(particle_shader_program) => {
				particle_shader_program
			}
			None => {
				return;
			}
		};
		self.transform_matrix.write_to_float32_array(&mut self.model_matrix_array);
		self.use_shader_program(particle_shader_program);
		let projection_matrix_location = particle_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "projectionMatrix");
		let model_matrix_location = particle_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "modelMatrix");
		let global_alpha_location = particle_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "globalAlpha");
		let blend_mode_index_location = particle_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "blendModeIndex");
		let vertex_bytes = float_slice_to_bytes(&self.particle_vertex_data[..vertex_count * PARTICLE_FLOATS_PER_VERTEX]);
		unsafe {
			crate::core::glstatecache::bind_vertex_array(&self.gl, self.particle_vertex_array);
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, self.particle_vertex_buffer);
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_matrix_3_f32_slice(model_matrix_location.as_ref(), false, &self.model_matrix_array);
			self.gl.uniform_1_f64(global_alpha_location.as_ref(), self.global_alpha);
			if is_shader_blend_draw {
				self.gl.uniform_1_i32(blend_mode_index_location.as_ref(), shader_blend_mode_index);
			}
		}
		if is_shader_blend_draw {
			self.apply_shader_blend_composite_function(true);
			self.draw_colored_quad_groups(vertex_count, texture);
			self.apply_shader_blend_composite_function(false);
		}
		else {
			self.bind_main_texture(texture);
			self.apply_color_mask();
			unsafe {
				crate::core::glstatecache::draw_arrays(&self.gl, glow::TRIANGLES, 0, vertex_count as i32);
			}
			self.draw_call_count += 1;
			self.draw_vertex_count += vertex_count as u32;
		}

		// 기본 버텍스 어레이로 복귀. (프로그램은 다음에 낱장으로 그릴 때 건다)
		unsafe {
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(self.vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(self.vertex_buffer));
		}
		self.release_clip_scissor();
	}

	//==============================================================================
	/// 올려 둔 파티클 쿼드를 셰이더 섞기로 그리기. (draw_colored_quads 가 섞기 프로그램을 쓰게 한 뒤 부른다, 원본 drawColoredQuadGroups)
	/// - Canvas2D 처럼 쿼드마다 앞 쿼드가 그린 위에 섞이도록, 서로 겹치지 않는 쿼드를 앞에서부터 모아 그 영역을 한 번 떠 와 함께 그린다.
	///   모은 쿼드와 겹치는 쿼드가 나오거나 SHADER_BLEND_PARTICLE_GROUP_CAPACITY 개가 차면 거기서 끊는다. (모두 겹치면 쿼드 하나에 복사 한 번, 드로우 콜 한 번)
	//==============================================================================
	fn draw_colored_quad_groups(&mut self, vertex_count: usize, texture: GlTexture) {
		let framebuffer = read_framebuffer_binding(GlContext::from_context(&self.gl));
		let quad_float_count = 6 * PARTICLE_FLOATS_PER_VERTEX;
		let quad_count = vertex_count / 6;
		let mut group_rect_list = std::mem::take(&mut self.particle_group_rect_list);
		group_rect_list.clear();
		let mut group_rect: Option<BackdropRect> = None;
		let mut group_start_quad_index = 0;
		for quad_index in 0..quad_count {
			let quad_values = &self.particle_vertex_data[quad_index * quad_float_count..(quad_index + 1) * quad_float_count];
			let quad_rect = match measure_backdrop_rect(quad_values, PARTICLE_FLOATS_PER_VERTEX, self.transform_matrix, self.applied_viewport_width, self.applied_viewport_height) {
				Some(quad_rect) => {
					quad_rect
				}
				None => {
					continue;
				}
			};
			let is_overlapped = group_rect_list.len() >= SHADER_BLEND_PARTICLE_GROUP_CAPACITY || group_rect_list.iter().any(|rect| rect.is_overlapping(&quad_rect));
			if is_overlapped {
				self.draw_colored_quad_group(framebuffer, group_rect, group_start_quad_index, quad_index, texture);
				group_start_quad_index = quad_index;
				group_rect_list.clear();
				group_rect = None;
			}

			// 영역을 모은다. (모은 영역 전체의 경계 상자를 떠 온다)
			group_rect_list.push(quad_rect);
			group_rect = match group_rect {
				Some(group_rect) => {
					Some(group_rect.union(&quad_rect))
				}
				None => {
					Some(quad_rect)
				}
			};
		}
		self.draw_colored_quad_group(framebuffer, group_rect, group_start_quad_index, quad_count, texture);
		self.particle_group_rect_list = group_rect_list;
	}

	//==============================================================================
	/// 모은 파티클 쿼드 한 묶음 그리기. (모은 영역이 없으면 그리지 않는다)
	//==============================================================================
	fn draw_colored_quad_group(&mut self, framebuffer: Option<GlFramebuffer>, group_rect: Option<BackdropRect>, start_quad_index: usize, end_quad_index: usize, texture: GlTexture) {
		let group_rect = match group_rect {
			Some(group_rect) => {
				group_rect
			}
			None => {
				return;
			}
		};
		if let Err(error) = self.copy_backdrop(framebuffer, group_rect) {
			eprintln!("Graphic: backdrop copy failed: {}", error);
			return;
		}

		// 떠 올 칸을 처음 만들 때 유닛 0 을 쓰므로 그림은 떠 온 뒤에 바인드한다.
		self.bind_main_texture(texture);
		let group_vertex_count = (end_quad_index - start_quad_index) * 6;
		self.apply_color_mask();
		unsafe {
			crate::core::glstatecache::draw_arrays(&self.gl, glow::TRIANGLES, (start_quad_index * 6) as i32, group_vertex_count as i32);
		}
		self.draw_call_count += 1;
		self.draw_vertex_count += group_vertex_count as u32;
	}

	//==============================================================================
	/// 부드러운 원 텍스처 반환. (가장자리가 잦아드는 64x64 원, 파티클 기본 스프라이트. 지연 생성)
	/// - GL 텍스처를 만들지 못하면 패닉한다.
	///
	/// # 돌려줌
	/// 64 x 64 픽셀 텍스처. (반지름 0.7 까지 꽉 차고 0.7 에서 1 까지 smoothstep 으로 사라지는 흰 원, 프리멀티플라이드)
	//==============================================================================
	pub fn get_soft_disc_texture(&mut self) -> GlTexture {
		if let Some(soft_disc_texture) = self.soft_disc_texture {
			return soft_disc_texture;
		}
		let mut pixels = vec![0u8; SOFT_DISC_TEXTURE_SIZE * SOFT_DISC_TEXTURE_SIZE * 4];
		let half_size = SOFT_DISC_TEXTURE_SIZE as f64 * 0.5;
		for pixel_y in 0..SOFT_DISC_TEXTURE_SIZE {
			for pixel_x in 0..SOFT_DISC_TEXTURE_SIZE {
				let delta_x = (pixel_x as f64 + 0.5 - half_size) / half_size;
				let delta_y = (pixel_y as f64 + 0.5 - half_size) / half_size;
				let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();

				// 중심은 꽉 차고 가장자리에서 부드럽게 사라진다.
				let mut alpha_ratio = math::clamp01(1.0 - (distance - 0.7) / 0.3);
				alpha_ratio = alpha_ratio * alpha_ratio * (3.0 - 2.0 * alpha_ratio);
				let alpha = (alpha_ratio * 255.0).round() as u8;
				let pixel_index = (pixel_y * SOFT_DISC_TEXTURE_SIZE + pixel_x) * 4;
				pixels[pixel_index] = alpha;
				pixels[pixel_index + 1] = alpha;
				pixels[pixel_index + 2] = alpha;
				pixels[pixel_index + 3] = alpha;
			}
		}
		let soft_disc_texture = unsafe {
			let texture = crate::core::glstatecache::create_texture(&self.gl).expect("soft disc texture");
			crate::core::glstatecache::bind_texture(&self.gl, glow::TEXTURE_2D, Some(texture));
			self.gl.tex_image_2d(
				glow::TEXTURE_2D,
				0,
				glow::RGBA8 as i32,
				SOFT_DISC_TEXTURE_SIZE as i32,
				SOFT_DISC_TEXTURE_SIZE as i32,
				0,
				glow::RGBA,
				glow::UNSIGNED_BYTE,
				glow::PixelUnpackData::Slice(Some(&pixels)),
			);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			texture
		};
		self.soft_disc_texture = Some(soft_disc_texture);
		soft_disc_texture
	}

	//==============================================================================
	/// 그림을 GPU 에 미리 올리기. (원본 prepareImage, 처음 그리는 프레임에 올리기가 몰리지 않게 씬을 불러오는 동안 부른다)
	/// - 지금 이미지 스무딩과 밉맵 설정으로 올린다. (그릴 때 설정이 다르면 그때 거르기만 바꾼다)
	/// - 이미 올린 그림이면 아무것도 하지 않는다. 크기가 0 인 그림은 올리지 않는다.
	/// - 코드로 지은 그림(Image::create_from_pixels 같은 것)처럼 ImageAsset 으로 읽지 않은 그림을 씬의 load 에서 올려 둘 때 쓴다.
	///
	/// # 인자
	/// - `image`: 올릴 그림.
	///
	/// # 돌려줌
	/// 텍스처가 있으면 true. (크기가 0 이거나 GL 텍스처를 만들지 못하면 false)
	///
	/// # 예제
	/// 씬을 불러오는 동안 코드로 지은 그림을 올려 두기
	/// ```rust,no_run
	/// use std::rc::Rc;
	/// use vanilla::{Engine, Image};
	///
	/// fn load(engine: &Engine) -> Rc<Image> {
	/// 	let dot_image = Image::create_from_pixels(32, 32, [255u8, 102, 0, 255].repeat(32 * 32));
	/// 	if let Some(mut graphic) = engine.get_graphic_mut() {
	/// 		let is_prepared = graphic.prepare_image(&dot_image);
	/// 		println!("{}", is_prepared);
	/// 	}
	/// 	dot_image
	/// }
	/// ```
	//==============================================================================
	pub fn prepare_image(&mut self, image: &Rc<Image>) -> bool {
		if image.get_width() == 0 || image.get_height() == 0 {
			return false;
		}
		// 텍스처를 묶기 전에 쌓아 둔 묶음을 먼저 그린다.
		self.flush_batch();
		self.get_image_texture(image).is_some()
	}

	//==============================================================================
	/// 읽었지만 아직 올리지 않은 그림 올리기. (원본 prepareLoadedImages, ImageAsset 과 AssetLoader 가 읽어 푼 그림, 푼 차례로)
	/// - 엔진이 씬의 맡긴 애셋이 다 읽힌 뒤(로딩 화면 동안) 프레임마다 나눠 부른다. 씬이 따로 부를 일은 없다.
	/// - 시간 한도를 주면 그 시간을 넘긴 뒤에는 멈추고 남은 수를 돌려준다. (한 번에 적어도 하나는 올린다, 유니티의 asyncUploadTimeSlice 와 같다)
	/// - 제 것(이 Graphic 을 든 엔진으로 읽은 그림)만 올린다. 주인을 모르는 그림은 Graphic 이 하나뿐일 때만 올린다. (원본과 같음)
	///
	/// # 인자
	/// - `time_limit_milliseconds`: 이번에 쓸 시간 한도. (밀리초, f64::INFINITY 면 모두)
	///
	/// # 돌려줌
	/// 아직 올리지 않고 남은 그림 수. (0 이면 다 올렸다)
	///
	/// # 예제
	/// 그림을 읽은 뒤 바로 올리기
	/// ```rust,no_run
	/// use vanilla::{Engine, ImageAsset};
	///
	/// async fn load(engine: &Engine, image_asset: &mut ImageAsset) {
	/// 	let _ = image_asset.load(engine, "examples/assets/spritesheet.png").await;
	/// 	if let Some(mut graphic) = engine.get_graphic_mut() {
	/// 		let remaining_count = graphic.prepare_loaded_images(f64::INFINITY);
	/// 		println!("{}", remaining_count);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn prepare_loaded_images(&mut self, time_limit_milliseconds: f64) -> usize {
		let pending_image_list = crate::core::decodedimagestore::get_pending_image_list(self.read_image_owner());
		let start_instant = web_time::Instant::now();
		let mut image_index = 0;
		while image_index < pending_image_list.len() {
			let image = &pending_image_list[image_index];
			image_index += 1;
			// 올렸거나 크기가 0 이라 올리지 못하는 그림은 맡긴 것에서 뺀다. (GL 텍스처를 만들지 못한 그림은 그릴 때 다시 본다)
			self.prepare_image(image);
			crate::core::decodedimagestore::release(image);
			if start_instant.elapsed().as_secs_f64() * 1000.0 >= time_limit_milliseconds {
				break;
			}
		}
		pending_image_list.len() - image_index
	}

	//==============================================================================
	/// 읽은 그림의 주인 번호. (이 Graphic 의 GL 컨텍스트, ImageAsset::load 가 손잡이에 건다)
	//==============================================================================
	pub(crate) fn read_image_owner(&self) -> usize {
		crate::core::decodedimagestore::read_owner(GlContext::from_context(&self.gl))
	}

	//==============================================================================
	/// 이미지의 텍스처 반환. (파티클처럼 직접 쿼드를 쓰는 곳에서)
	///
	/// # 인자
	/// - `image`: 텍스처를 받을 이미지. (처음이면 올리고 CPU 픽셀을 놓는다)
	///
	/// # 돌려줌
	/// 지금 스무딩과 밉맵 설정을 건 텍스처. GL 텍스처를 만들지 못하면 None.
	//==============================================================================
	pub fn get_image_texture(&mut self, image: &Rc<Image>) -> Option<GlTexture> {
		let is_image_smoothing_enabled = self.is_image_smoothing_enabled();
		let is_image_mipmap_enabled = self.is_image_mipmap_enabled();
		if let Some(texture) = self.image_texture_cache.find_texture(image, is_image_smoothing_enabled, is_image_mipmap_enabled) {
			return Some(texture);
		}

		// 새로 올리거나 필터를 다시 걸면 텍스처 바인드가 바뀐다. (상태 캐시를 거쳐 묶으므로 따로 잊지 않는다)
		self.image_texture_cache.get_texture(GlContext::from_context(&self.gl), image, is_image_smoothing_enabled, is_image_mipmap_enabled)
	}

	//==============================================================================
	/// GPU 에 올리며 놓은 이미지의 CPU 픽셀을 텍스처에서 다시 읽어 되돌린다. (ShadowImage 굽기처럼 픽셀을 다시 읽어야 할 때)
	/// - 픽셀이 남아 있으면 그대로 true. 올린 텍스처가 없거나 읽지 못하면 false.
	/// - 텍스처를 프레임버퍼에 붙여 읽고, 그 전에 걸려 있던 프레임버퍼로 되돌린다. (올린 그대로의 프리멀티플라이드 RGBA 가 나온다)
	//==============================================================================
	pub(crate) fn restore_image_pixels(&mut self, image: &Rc<Image>) -> bool {
		if image.has_pixels() {
			return true;
		}
		let texture = match self.image_texture_cache.find_uploaded_texture(image) {
			Some(texture) => {
				texture
			}
			None => {
				return false;
			}
		};
		self.flush_batch();
		match crate::core::imagetexturecache::read_texture_pixels(GlContext::from_context(&self.gl), texture, image.get_width(), image.get_height()) {
			Some(pixels) => {
				image.restore_pixels(pixels);
				true
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 선 출력.
	/// - set_stroke_color()
	///
	/// # 인자
	/// - `positions`: 이을 점들. (2 개 미만이면 그리지 않는다)
	/// - `size`: 선 두께. (픽셀)
	//==============================================================================
	pub fn draw_line(&mut self, positions: &[Vector2], size: f64) {
		self.draw_polyline(positions, size, false);
	}

	//==============================================================================
	/// 닫힌 선 출력. (마지막 점과 첫 점을 잇는 루프)
	/// - set_stroke_color()
	///
	/// # 인자
	/// - `positions`: 이을 점들. (2 개 미만이면 그리지 않는다)
	/// - `size`: 선 두께. (픽셀)
	//==============================================================================
	pub fn draw_line_loop(&mut self, positions: &[Vector2], size: f64) {
		self.draw_polyline(positions, size, true);
	}

	//==============================================================================
	/// 폴리라인 출력. (세그먼트별 두께 확장 쿼드)
	/// - 내부 조인트는 반두께 연장으로 이음새를 메운다.
	///
	/// # 인자
	/// - `positions`: 이을 점들. (2 개 미만이면 그리지 않는다, 스크래치 배열에 들어가는 선분 682 개까지만 그린다)
	/// - `size`: 선 두께. (픽셀)
	/// - `is_closed`: true 면 마지막 점과 첫 점도 잇는다.
	//==============================================================================
	pub fn draw_polyline(&mut self, positions: &[Vector2], size: f64, is_closed: bool) {
		if positions.len() < 2 {
			return;
		}

		// 정점 버퍼가 차면 쌓인 것을 그리고 비운 뒤 이어 적는다. (점이 많아도 선분을 버리지 않는다)
		let half_size = size / 2.0;
		let segment_count = read_polyline_segment_count(positions.len(), is_closed);
		let stroke_color = self.get_stroke_color();
		let white_texture = self.get_white_texture();
		let mut segment_index = 0;
		while segment_index < segment_count {
			let (vertex_count, next_segment_index) = write_polyline_vertices(&mut self.vertex_data, positions, half_size, is_closed, segment_index);
			self.draw_vertices(vertex_count, white_texture, stroke_color);
			segment_index = next_segment_index;
		}
	}

	//==============================================================================
	/// 사각형 출력.
	/// - set_fill_color()
	///
	/// # 인자
	/// - `rect`: 칠할 사각형. (지금 변환 전의 좌표, 픽셀)
	//==============================================================================
	pub fn draw_rect(&mut self, rect: Rect) {
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = left_x + rect.size.x;
		let bottom_y = top_y + rect.size.y;
		let vertex_count_offset = self.write_quad(0, left_x, top_y, right_x, bottom_y, 0.5, 0.5, 0.5, 0.5);
		let fill_color = self.get_fill_color();
		let white_texture = self.get_white_texture();
		self.draw_vertices(vertex_count_offset / FLOATS_PER_VERTEX, white_texture, fill_color);
	}

	//==============================================================================
	/// 사각형 외곽선 출력. (Canvas2D strokeRect 대응, 선 중심이 경계선 위)
	/// - set_stroke_color()
	///
	/// # 인자
	/// - `rect`: 외곽선을 그을 사각형. (픽셀)
	/// - `line_width`: 선 두께. (픽셀, 경계선 양쪽으로 절반씩)
	//==============================================================================
	pub fn draw_stroke_rect(&mut self, rect: Rect, line_width: f64) {
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = left_x + rect.size.x;
		let bottom_y = top_y + rect.size.y;
		let positions = [
			Vector2::create(left_x, top_y),
			Vector2::create(right_x, top_y),
			Vector2::create(right_x, bottom_y),
			Vector2::create(left_x, bottom_y),
		];
		self.draw_line_loop(&positions, line_width);
	}

	//==============================================================================
	/// 라운드 사각형 경로 좌표 목록 생성. (시계 방향, 볼록 다각형)
	///
	/// # 인자
	/// - `rect`: 사각형. (픽셀)
	/// - `round_size`: 모서리 반지름. (가로세로 가운데 짧은 쪽의 절반을 넘으면 그 값으로 줄인다)
	///
	/// # 돌려줌
	/// 모서리마다 9 개씩 36 개의 점. (우상, 우하, 좌하, 좌상 모서리 차례)
	//==============================================================================
	pub fn build_round_rect_positions(&self, rect: Rect, round_size: f64) -> [Vector2; ROUND_RECT_POSITION_COUNT] {
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = left_x + rect.size.x;
		let bottom_y = top_y + rect.size.y;
		let clamped_round_size = round_size.min(rect.size.x.abs().min(rect.size.y.abs()) / 2.0);

		// 코너 중심 4개. (우상 → 우하 → 좌하 → 좌상)
		let corner_centers: [(f64, f64); 4] = [
			(right_x - clamped_round_size, top_y + clamped_round_size),
			(right_x - clamped_round_size, bottom_y - clamped_round_size),
			(left_x + clamped_round_size, bottom_y - clamped_round_size),
			(left_x + clamped_round_size, top_y + clamped_round_size),
		];

		// 꼭짓점 각도는 늘 같으니 처음에 구해 둔 cos / sin 표를 쓴다. (힙 할당 없이 스택 배열로 돌려준다)
		let mut positions = [Vector2::create(0.0, 0.0); ROUND_RECT_POSITION_COUNT];
		for (corner_index, (corner_center_x, corner_center_y)) in corner_centers.into_iter().enumerate() {
			for segment_index in 0..=CORNER_SEGMENT_COUNT {
				let position_index = corner_index * (CORNER_SEGMENT_COUNT + 1) + segment_index;
				let (cosine, sine) = self.round_rect_unit_table[position_index];
				positions[position_index] = Vector2::create(corner_center_x + cosine * clamped_round_size, corner_center_y + sine * clamped_round_size);
			}
		}
		positions
	}

	//==============================================================================
	/// 볼록 다각형 채움 출력. (첫 점 기준 팬 분할)
	/// - 정점 버퍼가 차면 쌓인 삼각형을 그리고 비운 뒤 이어 적는다. (점이 많아도 삼각형을 버리지 않는다)
	///
	/// # 인자
	/// - `positions`: 볼록 다각형의 꼭짓점. (3 개 미만이면 그리지 않는다)
	/// - `color`: 칠할 색. (채우기 색은 쓰지 않는다)
	//==============================================================================
	pub fn draw_convex_polygon(&mut self, positions: &[Vector2], color: Color) {
		if positions.len() < 3 {
			return;
		}

		let white_texture = self.get_white_texture();
		let mut offset = 0;
		let mut vertex_count = 0;
		let first_position = positions[0];
		for position_index in 1..positions.len() - 1 {
			if (vertex_count + 3) * FLOATS_PER_VERTEX > self.vertex_data.len() {
				self.draw_vertices(vertex_count, white_texture, color);
				offset = 0;
				vertex_count = 0;
			}
			let second_position = positions[position_index];
			let third_position = positions[position_index + 1];
			let triangle_values: [f32; 12] = [
				first_position.x as f32, first_position.y as f32, 0.5, 0.5,
				second_position.x as f32, second_position.y as f32, 0.5, 0.5,
				third_position.x as f32, third_position.y as f32, 0.5, 0.5,
			];
			self.vertex_data[offset..offset + 12].copy_from_slice(&triangle_values);
			offset += 12;
			vertex_count += 3;
		}

		self.draw_vertices(vertex_count, white_texture, color);
	}

	//==============================================================================
	/// 라운드 사각형 출력.
	/// - set_fill_color()
	///
	/// # 인자
	/// - `rect`: 칠할 사각형. (픽셀)
	/// - `round_size`: 모서리 반지름. (0 이하면 draw_rect 와 같다)
	//==============================================================================
	pub fn draw_round_rect(&mut self, rect: Rect, round_size: f64) {
		if round_size > 0.0 {
			let positions = self.build_round_rect_positions(rect, round_size);
			let fill_color = self.get_fill_color();
			self.draw_convex_polygon(&positions, fill_color);
		}
		else {
			self.draw_rect(rect);
		}
	}

	//==============================================================================
	/// 라운드 사각형 외곽선 출력.
	/// - set_stroke_color()
	///
	/// # 인자
	/// - `rect`: 외곽선을 그을 사각형. (픽셀)
	/// - `round_size`: 모서리 반지름. (0 이하면 draw_stroke_rect 와 같다)
	/// - `line_width`: 선 두께. (픽셀)
	//==============================================================================
	pub fn draw_stroke_round_rect(&mut self, rect: Rect, round_size: f64, line_width: f64) {
		if round_size > 0.0 {
			let positions = self.build_round_rect_positions(rect, round_size);
			self.draw_line_loop(&positions, line_width);
		}
		else {
			self.draw_stroke_rect(rect, line_width);
		}
	}

	//==============================================================================
	/// 원 둘레 좌표 목록. (화면상 크기에 비례한 분할 수, 16 ~ 64)
	//==============================================================================
	fn build_circle_positions(&self, center: Vector2, radius: f64) -> Vec<Vector2> {
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		let segment_count = math::clamp((radius * maximum_scale * 0.5).ceil(), 16.0, 64.0) as usize;
		let mut positions = Vec::with_capacity(segment_count);
		for segment_index in 0..segment_count {
			let radian = (math::PI * 2.0) * (segment_index as f64 / segment_count as f64);
			let position_x = center.x + radian.cos() * radius;
			let position_y = center.y + radian.sin() * radius;
			positions.push(Vector2::create(position_x, position_y));
		}
		positions
	}

	//==============================================================================
	/// 원 출력.
	/// - set_fill_color()
	///
	/// # 인자
	/// - `center`: 가운데. (픽셀)
	/// - `radius`: 반지름. (픽셀, 0 이하면 그리지 않는다)
	//==============================================================================
	pub fn draw_circle(&mut self, center: Vector2, radius: f64) {
		if radius <= 0.0 {
			return;
		}
		let positions = self.build_circle_positions(center, radius);
		let fill_color = self.get_fill_color();
		self.draw_convex_polygon(&positions, fill_color);
	}

	//==============================================================================
	/// 원 외곽선 출력.
	/// - set_stroke_color()
	///
	/// # 인자
	/// - `center`: 가운데. (픽셀)
	/// - `radius`: 반지름. (픽셀, 0 이하면 그리지 않는다)
	/// - `line_width`: 선 두께. (픽셀, 반지름 양쪽으로 절반씩)
	//==============================================================================
	pub fn draw_stroke_circle(&mut self, center: Vector2, radius: f64, line_width: f64) {
		if radius <= 0.0 {
			return;
		}
		let positions = self.build_circle_positions(center, radius);
		self.draw_line_loop(&positions, line_width);
	}

	//==============================================================================
	/// 고리(띠) 출력. (안쪽 반지름과 바깥 반지름 사이를 start_radian 부터 end_radian 까지 칠한다)
	/// - set_fill_color()
	/// - 각도는 라디안, 0 이 오른쪽이고 화면에서 시계 방향으로 는다. (12 시에서 시작하려면 -PI / 2, 한 바퀴는 0 에서 PI * 2)
	/// - 조각마다 네모 하나를 겹치지 않게 이어 붙여 한 번에 그리므로, 반투명 색이어도 조각의 이음매가 짙어지지 않는다.
	/// - 조각 수는 draw_circle 처럼 화면에서의 바깥 반지름을 따라 정하고(한 바퀴 16 ~ 64), 그리는 각도만큼만 쓴다.
	///
	/// # 인자
	/// - `center`: 가운데. (픽셀)
	/// - `inner_radius`: 안쪽 반지름. (픽셀, 0 이면 부채꼴)
	/// - `outer_radius`: 바깥 반지름. (픽셀, 0 이하거나 inner_radius 이하면 그리지 않는다)
	/// - `start_radian`: 시작 각도. (라디안)
	/// - `end_radian`: 끝 각도. (라디안, 시작과 같으면 그리지 않는다, 차이는 한 바퀴까지만 쓴다)
	///
	/// # 예제
	/// 12 시부터 시계 방향으로 차오르는 진행 고리
	/// ```rust,no_run
	/// use vanilla::{math, Graphic, Vector2};
	///
	/// fn draw_progress(graphic: &mut Graphic, progress: f64) {
	/// 	let start_radian = -math::PI / 2.0;
	/// 	graphic.set_fill_color("#40c057");
	/// 	graphic.draw_ring(Vector2::create(200.0, 200.0), 40.0, 50.0, start_radian, start_radian + math::PI * 2.0 * progress);
	/// }
	/// ```
	//==============================================================================
	pub fn draw_ring(&mut self, center: Vector2, inner_radius: f64, outer_radius: f64, start_radian: f64, end_radian: f64) {
		let fill_color = self.get_fill_color();
		self.draw_ring_with_color(center, inner_radius, outer_radius, start_radian, end_radian, fill_color);
	}

	//==============================================================================
	/// 호 출력. (반지름을 따라 line_width 두께의 선으로 start_radian 부터 end_radian 까지)
	/// - set_stroke_color()
	/// - 각도와 조각 수는 draw_ring 과 같다. 선의 두께 가운데가 반지름에 온다.
	///
	/// # 인자
	/// - `center`: 가운데. (픽셀)
	/// - `radius`: 반지름. (픽셀, 선 두께의 가운데)
	/// - `start_radian`: 시작 각도. (라디안)
	/// - `end_radian`: 끝 각도. (라디안)
	/// - `line_width`: 선 두께. (픽셀)
	//==============================================================================
	pub fn draw_arc(&mut self, center: Vector2, radius: f64, start_radian: f64, end_radian: f64, line_width: f64) {
		let half_width = line_width * 0.5;
		let stroke_color = self.get_stroke_color();
		self.draw_ring_with_color(center, (radius - half_width).max(0.0), radius + half_width, start_radian, end_radian, stroke_color);
	}

	//==============================================================================
	/// 고리 출력의 셈. (draw_ring, draw_arc 가 부른다)
	///
	/// # 인자
	/// - `center`: 가운데. (픽셀)
	/// - `inner_radius`: 안쪽 반지름. (픽셀)
	/// - `outer_radius`: 바깥 반지름. (픽셀, 0 이하거나 inner_radius 이하면 그리지 않는다)
	/// - `start_radian`: 시작 각도. (라디안, 0 이 오른쪽)
	/// - `end_radian`: 끝 각도. (라디안, 시작과 같으면 그리지 않는다, 차이는 한 바퀴까지만 쓴다)
	/// - `color`: 칠할 색.
	//==============================================================================
	pub fn draw_ring_with_color(&mut self, center: Vector2, inner_radius: f64, outer_radius: f64, start_radian: f64, end_radian: f64, color: Color) {
		let span_radian = end_radian - start_radian;
		if outer_radius <= 0.0 || outer_radius <= inner_radius || span_radian == 0.0 {
			return;
		}
		let full_radian = math::PI * 2.0;
		let clamped_span_radian = math::clamp(span_radian, -full_radian, full_radian);
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		let full_segment_count = math::clamp((outer_radius * maximum_scale * 0.5).ceil(), 16.0, 64.0);
		let segment_count = ((full_segment_count * clamped_span_radian.abs() / full_radian).ceil() as usize).max(1);
		let mut offset = 0;
		let mut vertex_count = 0;
		let mut previous_cosine = start_radian.cos();
		let mut previous_sine = start_radian.sin();
		for segment_index in 1..=segment_count {
			if (vertex_count + 6) * FLOATS_PER_VERTEX > self.vertex_data.len() {
				break;
			}
			let radian = start_radian + clamped_span_radian * (segment_index as f64 / segment_count as f64);
			let cosine = radian.cos();
			let sine = radian.sin();
			let previous_inner_x = center.x + previous_cosine * inner_radius;
			let previous_inner_y = center.y + previous_sine * inner_radius;
			let previous_outer_x = center.x + previous_cosine * outer_radius;
			let previous_outer_y = center.y + previous_sine * outer_radius;
			let inner_x = center.x + cosine * inner_radius;
			let inner_y = center.y + sine * inner_radius;
			let outer_x = center.x + cosine * outer_radius;
			let outer_y = center.y + sine * outer_radius;
			let segment_values: [f32; 24] = [
				previous_inner_x as f32, previous_inner_y as f32, 0.5, 0.5,
				previous_outer_x as f32, previous_outer_y as f32, 0.5, 0.5,
				outer_x as f32, outer_y as f32, 0.5, 0.5,
				previous_inner_x as f32, previous_inner_y as f32, 0.5, 0.5,
				outer_x as f32, outer_y as f32, 0.5, 0.5,
				inner_x as f32, inner_y as f32, 0.5, 0.5,
			];
			self.vertex_data[offset..offset + 24].copy_from_slice(&segment_values);
			offset += 24;
			vertex_count += 6;
			previous_cosine = cosine;
			previous_sine = sine;
		}
		let white_texture = self.get_white_texture();
		self.draw_vertices(vertex_count, white_texture, color);
	}

	//==============================================================================
	/// 선분 여럿 출력. (위치 배열 하나에 선분마다 x0, y0, x1, y1 넷씩, 한 번에 그린다)
	/// - set_stroke_color()
	/// - 선분마다 두께만큼 넓힌 네모 하나다. (draw_line 과 달리 이음새를 메우지 않는다, 따로 떨어진 선분 용)
	/// - Vector2 배열을 만들지 않고 숫자 배열을 그대로 읽는다. 길이가 0 인 선분은 건너뛴다.
	/// - 정점 버퍼가 차면 거기까지 그리고 이어서 담는다. (묶어 그리기가 켜져 있으면 같은 텍스처라 한 번의 드로우 콜로 이어진다)
	///
	/// # 인자
	/// - `position_values`: 선분마다 x0, y0, x1, y1 넷씩 놓인 숫자 배열. (픽셀, 넷이 안 되는 끝은 버린다, 4 개 미만이면 그리지 않는다)
	/// - `size`: 선 두께. (픽셀)
	///
	/// # 예제
	/// 격자 선을 한 번에 그리기
	/// ```rust,no_run
	/// use vanilla::{Colors, Graphic};
	///
	/// fn draw_grid(graphic: &mut Graphic) {
	/// 	let mut position_values: Vec<f64> = Vec::new();
	/// 	for line_index in 0..=10 {
	/// 		let offset = line_index as f64 * 32.0;
	/// 		position_values.extend_from_slice(&[offset, 0.0, offset, 320.0]);
	/// 		position_values.extend_from_slice(&[0.0, offset, 320.0, offset]);
	/// 	}
	/// 	graphic.set_stroke_color(Colors::WHITE);
	/// 	graphic.draw_line_segments(&position_values, 1.0);
	/// }
	/// ```
	//==============================================================================
	pub fn draw_line_segments(&mut self, position_values: &[f64], size: f64) {
		if position_values.len() < 4 {
			return;
		}
		let half_size = size / 2.0;
		let stroke_color = self.get_stroke_color();
		let white_texture = self.get_white_texture();
		let mut offset = 0;
		let mut vertex_count = 0;
		for segment_values in position_values.chunks_exact(4) {
			let start_x = segment_values[0];
			let start_y = segment_values[1];
			let end_x = segment_values[2];
			let end_y = segment_values[3];
			let delta_x = end_x - start_x;
			let delta_y = end_y - start_y;
			let length = (delta_x * delta_x + delta_y * delta_y).sqrt();
			if !(length > 0.0) {
				continue;
			}
			if (vertex_count + 6) * FLOATS_PER_VERTEX > self.vertex_data.len() {
				self.draw_vertices(vertex_count, white_texture, stroke_color);
				offset = 0;
				vertex_count = 0;
			}
			let normal_x = -delta_y / length * half_size;
			let normal_y = delta_x / length * half_size;
			let quad_values: [f32; 24] = [
				(start_x + normal_x) as f32, (start_y + normal_y) as f32, 0.5, 0.5,
				(end_x + normal_x) as f32, (end_y + normal_y) as f32, 0.5, 0.5,
				(start_x - normal_x) as f32, (start_y - normal_y) as f32, 0.5, 0.5,
				(start_x - normal_x) as f32, (start_y - normal_y) as f32, 0.5, 0.5,
				(end_x + normal_x) as f32, (end_y + normal_y) as f32, 0.5, 0.5,
				(end_x - normal_x) as f32, (end_y - normal_y) as f32, 0.5, 0.5,
			];
			self.vertex_data[offset..offset + 24].copy_from_slice(&quad_values);
			offset += 24;
			vertex_count += 6;
		}
		self.draw_vertices(vertex_count, white_texture, stroke_color);
	}

	//==============================================================================
	/// 이미지 출력.
	///
	/// # 인자
	/// - `image`: 그릴 이미지. (채우기 색은 쓰지 않고 틴트와 전역 투명도만 먹는다)
	/// - `position`: 왼쪽 위. (픽셀)
	/// - `content_size`: 그릴 크기. (픽셀, 이미지 전체를 이 크기로 늘린다)
	//==============================================================================
	pub fn draw_image(&mut self, image: &Rc<Image>, position: Vector2, content_size: Vector2) {
		let image_width = image.get_width() as f64;
		let image_height = image.get_height() as f64;
		self.draw_image_with_source_and_destination(image, 0.0, 0.0, image_width, image_height, position.x, position.y, content_size.x, content_size.y);
	}

	//==============================================================================
	/// 이미지의 일부만 출력. (image_rect 가 비었으면 전체)
	///
	/// # 인자
	/// - `image`: 그릴 이미지.
	/// - `position`: 왼쪽 위. (픽셀)
	/// - `content_size`: 그릴 크기. (픽셀)
	/// - `image_rect`: 그릴 이미지 영역. (이미지 픽셀, 네 값이 모두 0 이면 전체)
	//==============================================================================
	pub fn draw_image_with_image_rect(&mut self, image: &Rc<Image>, position: Vector2, content_size: Vector2, image_rect: Rect) {
		let source_rect = if image_rect.is_zero() {
			Rect::create(0.0, 0.0, image.get_width() as f64, image.get_height() as f64)
		}
		else {
			image_rect
		};
		self.draw_image_with_source_and_destination(image, source_rect.position.x, source_rect.position.y, source_rect.size.x, source_rect.size.y, position.x, position.y, content_size.x, content_size.y);
	}

	//==============================================================================
	/// 이미지의 소스 영역을 지정 영역에 출력. (Canvas2D drawImage 9인자 대응)
	/// - 이미지의 텍스처를 얻지 못하면 그리지 않는다.
	///
	/// # 인자
	/// - `image`: 그릴 이미지.
	/// - `source_x`: 소스 영역 왼쪽. (이미지 픽셀)
	/// - `source_y`: 소스 영역 위. (이미지 픽셀)
	/// - `source_width`: 소스 영역 너비. (이미지 픽셀)
	/// - `source_height`: 소스 영역 높이. (이미지 픽셀)
	/// - `destination_x`: 그릴 자리 왼쪽. (픽셀)
	/// - `destination_y`: 그릴 자리 위. (픽셀)
	/// - `destination_width`: 그릴 너비. (픽셀)
	/// - `destination_height`: 그릴 높이. (픽셀)
	//==============================================================================
	pub fn draw_image_with_source_and_destination(&mut self, image: &Rc<Image>, source_x: f64, source_y: f64, source_width: f64, source_height: f64, destination_x: f64, destination_y: f64, destination_width: f64, destination_height: f64) {
		let texture = match self.get_image_texture(image) {
			Some(texture) => {
				texture
			}
			None => {
				return;
			}
		};

		let image_width = image.get_width() as f64;
		let image_height = image.get_height() as f64;
		let left_u = source_x / image_width;
		let top_v = source_y / image_height;
		let right_u = (source_x + source_width) / image_width;
		let bottom_v = (source_y + source_height) / image_height;
		let vertex_count_offset = self.write_quad(0, destination_x, destination_y, destination_x + destination_width, destination_y + destination_height, left_u, top_v, right_u, bottom_v);
		let white_color = self.get_white_color();
		self.draw_vertices(vertex_count_offset / FLOATS_PER_VERTEX, texture, white_color);
	}

	//==============================================================================
	/// 이미지 나인패치 출력.
	/// - nine_slice 의 position.x / position.y / size.x / size.y 가 각각 왼쪽 / 위 / 오른쪽 / 아래 두께.
	///
	/// # 인자
	/// - `image`: 그릴 이미지.
	/// - `position`: 왼쪽 위. (픽셀, 내림한다)
	/// - `size`: 그릴 크기. (픽셀, 올림하고, 양쪽 두께의 합보다 작으면 그 합)
	/// - `nine_slice`: 네 테두리 두께. (이미지 픽셀이자 그릴 픽셀, 두께가 모두 0 이면 그냥 늘려 그린다)
	///
	/// # 예제
	/// 테두리 16 픽셀은 그대로 두고 가운데만 늘린 판
	/// ```rust,no_run
	/// use std::rc::Rc;
	/// use vanilla::{Graphic, Image, Rect, Vector2};
	///
	/// fn draw_panel(graphic: &mut Graphic, panel_image: &Rc<Image>) {
	/// 	graphic.draw_image_with_nine_slice(panel_image, Vector2::create(20.0, 20.0), Vector2::create(300.0, 120.0), Rect::create(16.0, 16.0, 16.0, 16.0));
	/// }
	/// ```
	//==============================================================================
	pub fn draw_image_with_nine_slice(&mut self, image: &Rc<Image>, position: Vector2, size: Vector2, nine_slice: Rect) {
		let source_width = image.get_width() as f64;
		let source_height = image.get_height() as f64;
		let destination_x = position.x.floor();
		let destination_y = position.y.floor();
		let left = nine_slice.position.x;
		let top = nine_slice.position.y;
		let right = nine_slice.size.x;
		let bottom = nine_slice.size.y;

		// 최소 사이즈 제한 적용.
		let destination_width = size.x.ceil().max(left + right);
		let destination_height = size.y.ceil().max(top + bottom);

		let has_horizontal = left > 0.0 || right > 0.0;
		let has_vertical = top > 0.0 || bottom > 0.0;

		if has_horizontal && has_vertical {
			// 가로세로 다 쪼개기.
			let center_source_width = source_width - left - right;
			let center_source_height = source_height - top - bottom;
			let center_destination_width = destination_width - left - right;
			let center_destination_height = destination_height - top - bottom;

			// 위쪽.
			self.draw_image_with_source_and_destination(image, 0.0, 0.0, left, top, destination_x, destination_y, left + 1.0, top + 1.0);
			self.draw_image_with_source_and_destination(image, left, 0.0, center_source_width, top, destination_x + left, destination_y, center_destination_width + 1.0, top + 1.0);
			self.draw_image_with_source_and_destination(image, source_width - right, 0.0, right, top, destination_x + destination_width - right, destination_y, right, top + 1.0);

			// 가운데쪽.
			self.draw_image_with_source_and_destination(image, 0.0, top, left, center_source_height, destination_x, destination_y + top, left + 1.0, center_destination_height + 1.0);
			self.draw_image_with_source_and_destination(image, left, top, center_source_width, center_source_height, destination_x + left, destination_y + top, center_destination_width + 1.0, center_destination_height + 1.0);
			self.draw_image_with_source_and_destination(image, source_width - right, top, right, center_source_height, destination_x + destination_width - right, destination_y + top, right, center_destination_height + 1.0);

			// 아래쪽.
			self.draw_image_with_source_and_destination(image, 0.0, source_height - bottom, left, bottom, destination_x, destination_y + destination_height - bottom, left + 1.0, bottom);
			self.draw_image_with_source_and_destination(image, left, source_height - bottom, center_source_width, bottom, destination_x + left, destination_y + destination_height - bottom, center_destination_width + 1.0, bottom);
			self.draw_image_with_source_and_destination(image, source_width - right, source_height - bottom, right, bottom, destination_x + destination_width - right, destination_y + destination_height - bottom, right, bottom);
		}
		else if has_horizontal {
			// 가로만 쪼개기.
			let center_source_width = source_width - left - right;
			let center_destination_width = destination_width - left - right;

			self.draw_image_with_source_and_destination(image, 0.0, 0.0, left, source_height, destination_x, destination_y, left + 1.0, destination_height);
			self.draw_image_with_source_and_destination(image, left, 0.0, center_source_width, source_height, destination_x + left, destination_y, center_destination_width + 1.0, destination_height);
			self.draw_image_with_source_and_destination(image, source_width - right, 0.0, right, source_height, destination_x + destination_width - right, destination_y, right, destination_height);
		}
		else if has_vertical {
			// 세로만 쪼개기.
			let center_source_height = source_height - top - bottom;
			let center_destination_height = destination_height - top - bottom;

			self.draw_image_with_source_and_destination(image, 0.0, 0.0, source_width, top, destination_x, destination_y, destination_width, top + 1.0);
			self.draw_image_with_source_and_destination(image, 0.0, top, source_width, center_source_height, destination_x, destination_y + top, destination_width, center_destination_height + 1.0);
			self.draw_image_with_source_and_destination(image, 0.0, source_height - bottom, source_width, bottom, destination_x, destination_y + destination_height - bottom, destination_width, bottom);
		}
		else {
			// 쪼개기 없음.
			self.draw_image_with_source_and_destination(image, 0.0, 0.0, source_width, source_height, destination_x, destination_y, destination_width, destination_height);
		}
	}

	//==============================================================================
	/// 폰트 문자열 설정. (Canvas2D font 대응, 예: "16px DOSGothic")
	///
	/// # 인자
	/// - `font_string`: Canvas2D font 꼴 글. ("bold 16px sans-serif" 처럼, 빈 글이면 바꾸지 않는다)
	//==============================================================================
	pub fn set_font_string(&mut self, font_string: &str) {
		if !font_string.is_empty() && self.font_string != font_string {
			self.font_string.clear();
			self.font_string.push_str(font_string);
		}
	}

	//==============================================================================
	/// 폰트 문자열 반환.
	///
	/// # 돌려줌
	/// 지금 폰트 문자열. (처음 "10px sans-serif")
	//==============================================================================
	pub fn get_font_string(&self) -> &str {
		&self.font_string
	}

	//==============================================================================
	/// 가로 정렬 설정.
	///
	/// # 인자
	/// - `text_align`: 가로 정렬. (처음 Start)
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		self.text_align = text_align;
	}

	//==============================================================================
	/// 가로 정렬 반환.
	///
	/// # 돌려줌
	/// 지금 가로 정렬.
	//==============================================================================
	pub fn get_text_align(&self) -> TextAlign {
		self.text_align
	}

	//==============================================================================
	/// 글자 방향 설정. (원본 setTextDirection, Canvas2D direction 대응)
	/// - 문단의 방향이다. 글자 안의 순서(양방향 알고리즘)와 정렬 Start / End 의 왼쪽, 오른쪽이 이것을 따른다.
	/// - Inherit 는 Ltr 로 본다. (기본) Auto 는 문자열마다 처음 나오는 방향이 강한 글자로 정한다. 사용자가 쓴 글(닉네임 등)에 쓴다.
	///
	/// # 인자
	/// - `text_direction`: 글자 방향. (처음 Inherit)
	//==============================================================================
	pub fn set_text_direction(&mut self, text_direction: TextDirection) {
		self.text_direction = text_direction;
	}

	//==============================================================================
	/// 글자 방향 반환.
	///
	/// # 돌려줌
	/// 설정한 글자 방향. (Inherit, Auto 를 풀지 않은 그대로, 실제 방향은 resolve_text_direction)
	//==============================================================================
	pub fn get_text_direction(&self) -> TextDirection {
		self.text_direction
	}

	//==============================================================================
	/// 문자열의 실제 방향 계산. (Ltr 또는 Rtl, 원본 resolveTextDirection)
	///
	/// # 인자
	/// - `text`: Auto 일 때 방향을 볼 문자열.
	///
	/// # 돌려줌
	/// 설정이 Rtl 이면 Rtl, Auto 면 detect_text_direction(text), 그 밖(Inherit, Ltr)은 Ltr.
	//==============================================================================
	pub fn resolve_text_direction(&self, text: &str) -> TextDirection {
		match self.text_direction {
			TextDirection::Rtl => {
				TextDirection::Rtl
			}
			TextDirection::Auto => {
				detect_text_direction(text)
			}
			_ => {
				TextDirection::Ltr
			}
		}
	}

	//==============================================================================
	/// 실제 가로 정렬 계산. (Start / End 를 방향에 따라 Left / Right 로, 원본 resolveTextAlign)
	///
	/// # 인자
	/// - `text_align`: 풀 정렬.
	/// - `text_direction`: 글자 방향. (Rtl 만 오른쪽에서 왼쪽으로 본다)
	///
	/// # 돌려줌
	/// Start 는 Rtl 이면 Right, 아니면 Left. End 는 그 반대. 그 밖은 그대로.
	//==============================================================================
	pub fn resolve_text_align(&self, text_align: TextAlign, text_direction: TextDirection) -> TextAlign {
		let is_right_to_left = text_direction == TextDirection::Rtl;
		match text_align {
			TextAlign::Start => {
				if is_right_to_left {
					TextAlign::Right
				}
				else {
					TextAlign::Left
				}
			}
			TextAlign::End => {
				if is_right_to_left {
					TextAlign::Left
				}
				else {
					TextAlign::Right
				}
			}
			_ => {
				text_align
			}
		}
	}

	//==============================================================================
	/// 세로 기준선 설정.
	///
	/// # 인자
	/// - `text_baseline`: 세로 기준선. (처음 Alphabetic)
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: TextBaseline) {
		self.text_baseline = text_baseline;
	}

	//==============================================================================
	/// 세로 기준선 반환.
	///
	/// # 돌려줌
	/// 지금 세로 기준선.
	//==============================================================================
	pub fn get_text_baseline(&self) -> TextBaseline {
		self.text_baseline
	}

	//==============================================================================
	/// 문자열 측정. (현재 폰트 문자열 기준)
	///
	/// # 인자
	/// - `text`: 잴 문자열.
	///
	/// # 돌려줌
	/// 잰 값. (픽셀, 지금 변환은 보지 않는다) 데스크탑에서 그릴 얼굴이 하나도 없으면 모두 0 이다.
	///
	/// # 예제
	/// 글 너비에 맞춘 이름표
	/// ```rust,no_run
	/// use vanilla::{Colors, Graphic, Rect, TextAlign, TextBaseline};
	///
	/// fn draw_label(graphic: &mut Graphic, text: &str) {
	/// 	graphic.set_font_string("20px sans-serif");
	/// 	let text_metrics = graphic.measure_text(text);
	/// 	let padding = 8.0;
	/// 	let box_width = text_metrics.width + padding * 2.0;
	/// 	let box_height = text_metrics.font_ascent + text_metrics.font_descent + padding * 2.0;
	/// 	graphic.set_fill_color(Colors::BLACK);
	/// 	graphic.draw_rect(Rect::create(20.0, 20.0, box_width, box_height));
	/// 	graphic.set_text_align(TextAlign::Left);
	/// 	graphic.set_text_baseline(TextBaseline::Top);
	/// 	graphic.set_fill_color(Colors::WHITE);
	/// 	graphic.draw_fill_text(text, 20.0 + padding, 20.0 + padding);
	/// }
	/// ```
	//==============================================================================
	pub fn measure_text(&self, text: &str) -> TextMetrics {
		let font_string = self.get_font_string();
		self.text_string_texture_cache.measure_text(font_string, text)
	}

	//==============================================================================
	/// 문자열 측정. (주어진 폰트 문자열 기준, 현재 폰트 문자열은 바꾸지 않는다)
	///
	/// # 인자
	/// - `font_string`: 잴 때 쓸 폰트 문자열. (Canvas2D font 꼴)
	/// - `text`: 잴 문자열.
	///
	/// # 돌려줌
	/// 잰 값. (픽셀, 지금 변환은 보지 않는다) 데스크탑에서 그릴 얼굴이 하나도 없으면 모두 0 이다.
	//==============================================================================
	pub fn measure_text_with_font_string(&self, font_string: &str, text: &str) -> TextMetrics {
		self.text_string_texture_cache.measure_text(font_string, text)
	}

	//==============================================================================
	/// 텍스트 베이크 스케일 양자화 단위 설정.
	/// - 기본 0.25. 확대 축소가 잦은 글자가 스케일마다 새로 구워지지 않게 묶는다.
	/// - 0 이면 양자화하지 않고 실제 스케일 그대로 굽는다.
	///
	/// # 인자
	/// - `text_bake_scale_step`: 양자화 단위. (0 미만은 0)
	//==============================================================================
	pub fn set_text_bake_scale_step(&mut self, text_bake_scale_step: f64) {
		self.text_bake_scale_step = text_bake_scale_step.max(0.0);
	}

	//==============================================================================
	/// 텍스트 베이크 스케일 양자화 단위 반환.
	///
	/// # 돌려줌
	/// 양자화 단위. (처음 0.25)
	//==============================================================================
	pub fn get_text_bake_scale_step(&self) -> f64 {
		self.text_bake_scale_step
	}

	//==============================================================================
	/// 배율이 움직이는 노드의 텍스트 베이크 스케일 양자화 단위 설정. (기본 0.25)
	/// - 양자화 단위(set_text_bake_scale_step)가 0 일 때, 그리는 노드의 변환 배율이 지난 프레임과 다르면(확대 움직임, 팝업, 트윈, 카메라 확대)
	///   이 단위로 묶은 배율로 굽는다. 프레임마다 다른 배율로 새로 굽지 않는다.
	/// - 배율이 멈추면(지난 프레임과 같으면) 정확한 배율로 다시 구워 1:1 로 또렷하다.
	/// - 0 이면 움직이는 동안에도 묶지 않는다. (지금까지와 같다)
	///
	/// # 인자
	/// - `text_bake_motion_scale_step`: 배율이 움직이는 동안의 양자화 단위. (0 미만은 0)
	//==============================================================================
	pub fn set_text_bake_motion_scale_step(&mut self, text_bake_motion_scale_step: f64) {
		self.text_bake_motion_scale_step = text_bake_motion_scale_step.max(0.0);
	}

	//==============================================================================
	/// 배율이 움직이는 노드의 텍스트 베이크 스케일 양자화 단위 반환.
	///
	/// # 돌려줌
	/// 배율이 움직이는 동안의 양자화 단위. (처음 0.25)
	//==============================================================================
	pub fn get_text_bake_motion_scale_step(&self) -> f64 {
		self.text_bake_motion_scale_step
	}

	//==============================================================================
	/// 지금 그리는 노드의 배율이 움직이는 중인지 설정. (NodeTree 가 정한다, 노드 밖에서 직접 그릴 때 부르는 쪽이 정할 수 있다)
	///
	/// # 인자
	/// - `is_text_bake_scale_moving`: 배율이 움직이는 중이면 true.
	//==============================================================================
	pub fn set_text_bake_scale_moving(&mut self, is_text_bake_scale_moving: bool) {
		self.is_text_bake_scale_moving = is_text_bake_scale_moving;
	}

	//==============================================================================
	/// 지금 그리는 노드의 배율이 움직이는 중인지 반환.
	///
	/// # 돌려줌
	/// 움직이는 중으로 정해져 있으면 true.
	//==============================================================================
	pub fn is_text_bake_scale_moving(&self) -> bool {
		self.is_text_bake_scale_moving
	}

	//==============================================================================
	/// 노드의 변환 배율이 지난 프레임과 달라 움직이는 중인지 셈해 정하고, 노드에 기억할 배율을 돌려준다. (NodeTree 가 push_transform 뒤에 부른다)
	/// - last_drawn_maximum_scale 은 그 노드를 지난번 그릴 때의 값이다. (0 이면 아직 모른다, 처음 그리는 노드는 움직이지 않는 것으로 본다)
	/// - 양자화 단위가 0 이 아니면 늘 묶어 굽으므로 셈하지 않고 받은 값을 그대로 돌려준다.
	///
	/// # 인자
	/// - `last_drawn_maximum_scale`: 그 노드를 지난번 그릴 때 돌려받은 배율. (0 이면 처음)
	///
	/// # 돌려줌
	/// 노드에 기억할 배율. (지금 변환의 가장 큰 배율, 양자화 단위가 0 이 아니거나 움직임 단위가 0 이면 받은 값 그대로)
	//==============================================================================
	pub fn update_text_bake_scale_moving(&mut self, last_drawn_maximum_scale: f64) -> f64 {
		if self.text_bake_scale_step > 0.0 || self.text_bake_motion_scale_step <= 0.0 {
			self.is_text_bake_scale_moving = false;
			return last_drawn_maximum_scale;
		}
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		self.is_text_bake_scale_moving = last_drawn_maximum_scale > 0.0 && last_drawn_maximum_scale != maximum_scale;
		maximum_scale
	}

	//==============================================================================
	/// 텍스트 픽셀 스냅 설정.
	/// - 켜면 회전 / 기울임이 없는 변환에서 글자 쿼드의 왼쪽 위를 디바이스 픽셀 격자에 맞춘다.
	///
	/// # 인자
	/// - `is_text_pixel_snap_enabled`: true 면 글자를 디바이스 픽셀에 맞춘다. (처음 false)
	//==============================================================================
	pub fn set_text_pixel_snap_enabled(&mut self, is_text_pixel_snap_enabled: bool) {
		self.is_text_pixel_snap_enabled = is_text_pixel_snap_enabled;
	}

	//==============================================================================
	/// 텍스트 픽셀 스냅 여부 반환.
	///
	/// # 돌려줌
	/// 픽셀 스냅이 켜져 있으면 true.
	//==============================================================================
	pub fn is_text_pixel_snap_enabled(&self) -> bool {
		self.is_text_pixel_snap_enabled
	}

	//==============================================================================
	/// 글자 안티앨리어싱 켬 / 끔. (원본 Graphic.setTextAntialiasEnabled)
	/// - 끄면 구운 글자의 알파를 문턱값 128 로 0 과 255 로 가르고 텍스처를 NEAREST 로 뽑는다. (도트 글꼴용)
	/// - 켜고 끈 다음에 굽는 글자부터 적용된다. 이미 구워 둔 글자는 그대로다.
	///
	/// # 인자
	/// - `is_text_antialias_enabled`: true 면 켠다. (처음 true)
	//==============================================================================
	pub fn set_text_antialias_enabled(&mut self, is_text_antialias_enabled: bool) {
		self.text_string_texture_cache.set_text_antialias_enabled(is_text_antialias_enabled);
	}

	//==============================================================================
	/// 글자 안티앨리어싱이 켜져 있는지. (원본 Graphic.isTextAntialiasEnabled)
	///
	/// # 돌려줌
	/// 켜져 있으면 true.
	//==============================================================================
	pub fn is_text_antialias_enabled(&self) -> bool {
		self.text_string_texture_cache.is_text_antialias_on()
	}

	//==============================================================================
	/// 텍스트 베이크 스케일 계산. (현재 변환 행렬 기반, 양자화 단위는 set_text_bake_scale_step)
	///
	/// # 돌려줌
	/// 지금 변환의 가장 큰 배율을 양자화 단위로 반올림한 값. (0.25 ~ 8 로 자른다, 단위가 0 이고 움직이는 중이면 움직임 단위로, 둘 다 0 이면 반올림하지 않는다)
	//==============================================================================
	pub fn calculate_text_bake_scale(&self) -> f64 {
		let maximum_scale = self.transform_matrix.get_maximum_scale();
		let mut text_bake_scale_step = self.get_text_bake_scale_step();
		if text_bake_scale_step <= 0.0 && self.is_text_bake_scale_moving() {
			text_bake_scale_step = self.get_text_bake_motion_scale_step();
		}
		let quantized_scale = if text_bake_scale_step > 0.0 {
			(maximum_scale / text_bake_scale_step).round() * text_bake_scale_step
		}
		else {
			maximum_scale
		};
		math::clamp(quantized_scale, 0.25, MAXIMUM_TEXT_BAKE_SCALE)
	}

	//==============================================================================
	/// 글 배치 엔트리를 정렬/베이스라인에 맞춰 글리프 쿼드로 출력.
	/// - 문자열을 통째로 구웠을 때의 사각형 자리(픽셀 스냅 포함)를 셈하고, 그 안의 글리프 사각형을 아틀라스에서 뽑아 그린다.
	/// - text_direction 은 정렬 Start / End 를 가를 방향이다. (Ltr 또는 Rtl, 원본 drawTextEntry 의 textDirection)
	///
	/// # 인자
	/// - `entry`: 글 배치 엔트리. (TextStringTextureCache::get_entry)
	/// - `x`: 기준 x. (가로 정렬의 기준점, 픽셀)
	/// - `y`: 기준 y. (세로 기준선의 기준점, 픽셀)
	/// - `color`: 곱할 색. (None 이면 흰색, 구운 그대로)
	/// - `text_direction`: 정렬 Start, End 를 가를 방향.
	//==============================================================================
	pub fn draw_text_entry(&mut self, entry: TextEntry, x: f64, y: f64, color: Option<Color>, text_direction: TextDirection) {
		// 가로 정렬 오프셋.
		let text_align = self.resolve_text_align(self.get_text_align(), text_direction);
		let align_offset = match text_align {
			TextAlign::Center => {
				-entry.advance_width / 2.0
			}
			TextAlign::Right => {
				-entry.advance_width
			}
			_ => {
				0.0
			}
		};

		// 요청 베이스라인 → 알파벳 베이스라인 오프셋.
		let text_baseline = self.get_text_baseline();
		let baseline_offset = match text_baseline {
			TextBaseline::Top => {
				entry.font_ascent
			}
			TextBaseline::Hanging => {
				entry.font_ascent * 0.8
			}
			TextBaseline::Middle => {
				(entry.font_ascent - entry.font_descent) / 2.0
			}
			TextBaseline::Ideographic | TextBaseline::Bottom => {
				-entry.font_descent
			}
			TextBaseline::Alphabetic => {
				0.0
			}
		};

		let mut quad_x = x + align_offset - entry.pen_offset_x;
		let mut quad_y = y + baseline_offset - entry.baseline_offset_y;

		// 픽셀 스냅. (회전 / 기울임이 없을 때만, 쿼드의 왼쪽 위를 디바이스 픽셀에 맞춘다)
		let is_text_pixel_snap_enabled = self.is_text_pixel_snap_enabled();
		let transform_matrix = self.transform_matrix;
		if is_text_pixel_snap_enabled && transform_matrix.b == 0.0 && transform_matrix.c == 0.0 && transform_matrix.a > 0.0 && transform_matrix.d > 0.0 {
			let device_point = transform_matrix.transform_point(quad_x, quad_y);
			quad_x += (device_point.x.round() - device_point.x) / transform_matrix.a;
			quad_y += (device_point.y.round() - device_point.y) / transform_matrix.d;
		}

		// 글리프 사각형을 문자열 사각형 자리에 놓아 그린다. (같은 아틀라스 장의 글리프는 한 번에 넘겨 묶음에 들어간다)
		let white_color = self.get_white_color();
		let draw_color = color.unwrap_or(white_color);
		let maximum_float_count = VERTEX_CAPACITY * FLOATS_PER_VERTEX - 6 * FLOATS_PER_VERTEX;
		let mut float_count = 0;
		let mut run_texture: Option<GlTexture> = None;
		for (glyph_texture, glyph_rect) in entry.glyph_textures.iter().zip(entry.glyph_rects.iter()) {
			if let Some(texture) = run_texture {
				if float_count > 0 && (texture != *glyph_texture || float_count > maximum_float_count) {
					self.draw_vertices(float_count / FLOATS_PER_VERTEX, texture, draw_color);
					float_count = 0;
				}
			}
			run_texture = Some(*glyph_texture);
			float_count = self.write_quad(float_count, quad_x + glyph_rect[0], quad_y + glyph_rect[1], quad_x + glyph_rect[2], quad_y + glyph_rect[3], glyph_rect[4], glyph_rect[5], glyph_rect[6], glyph_rect[7]);
		}
		if let Some(texture) = run_texture {
			if float_count > 0 {
				self.draw_vertices(float_count / FLOATS_PER_VERTEX, texture, draw_color);
			}
		}
	}

	//==============================================================================
	/// 텍스트 채움 출력. (Canvas2D fillText 대응)
	/// - set_fill_color() / set_font_string() / set_text_align() / set_text_baseline()
	///
	/// # 인자
	/// - `text`: 그릴 문자열. (빈 글이면 그리지 않는다)
	/// - `x`: 기준 x. (set_text_align 의 기준점, 픽셀)
	/// - `y`: 기준 y. (set_text_baseline 의 기준점, 픽셀)
	//==============================================================================
	pub fn draw_fill_text(&mut self, text: &str, x: f64, y: f64) {
		if text.is_empty() {
			return;
		}

		// 글자는 흰색으로 굽고 그릴 때 색을 곱한다.
		let fill_color = self.get_fill_color();
		let bake_scale = self.calculate_text_bake_scale();
		let text_direction = self.resolve_text_direction(text);
		let entry = self.get_text_entry(TextDrawKind::Fill, text, 0.0, bake_scale, text_direction);
		if let Some(entry) = entry {
			self.draw_text_entry(entry, x, y, Some(fill_color), text_direction);
		}
	}

	//==============================================================================
	/// 텍스트 외곽선 출력. (Canvas2D strokeText 대응)
	/// - set_stroke_color() / set_font_string() / set_text_align() / set_text_baseline()
	///
	/// # 인자
	/// - `text`: 그릴 문자열. (빈 글이면 그리지 않는다)
	/// - `x`: 기준 x. (set_text_align 의 기준점, 픽셀)
	/// - `y`: 기준 y. (set_text_baseline 의 기준점, 픽셀)
	/// - `line_width`: 외곽선 두께. (픽셀)
	//==============================================================================
	pub fn draw_stroke_text(&mut self, text: &str, x: f64, y: f64, line_width: f64) {
		if text.is_empty() {
			return;
		}

		let stroke_color = self.get_stroke_color();
		let bake_scale = self.calculate_text_bake_scale();
		let text_direction = self.resolve_text_direction(text);
		let entry = self.get_text_entry(TextDrawKind::Stroke, text, line_width, bake_scale, text_direction);
		if let Some(entry) = entry {
			self.draw_text_entry(entry, x, y, Some(stroke_color), text_direction);
		}
	}

	//==============================================================================
	/// 현재 폰트 문자열로 구운 문자열 반환. (없으면 굽는다)
	//==============================================================================
	fn get_text_entry(&mut self, text_draw_kind: TextDrawKind, text: &str, line_width: f64, bake_scale: f64, text_direction: TextDirection) -> Option<TextEntry> {
		let shaping_direction = text_direction.to_shaping_direction();
		if !self.text_string_texture_cache.has_pending_deleted_textures() {
			if let Some(entry) = self.text_string_texture_cache.find_entry(text_draw_kind, text, &self.font_string, line_width, bake_scale, shaping_direction) {
				return Some(entry);
			}
		}

		// 버릴 아틀라스 장 텍스처는 쌓아 둔 묶음이 쓸 수 있어 묶음을 먼저 내보내고 지운다. (원본 beforeTextureOverwrite)
		// 새 글리프는 장의 빈 칸에만 올리므로 묶음을 내보내지 않는다. (올리며 텍스처를 묶는 것은 상태 캐시를 거친다)
		if self.text_string_texture_cache.has_pending_deleted_textures() {
			self.flush_batch();
			self.text_string_texture_cache.delete_pending_textures(GlContext::from_context(&self.gl));
		}
		let entry = self.text_string_texture_cache.get_entry(GlContext::from_context(&self.gl), text_draw_kind, text, &self.font_string, line_width, bake_scale, shaping_direction);
		if self.text_string_texture_cache.has_pending_deleted_textures() {
			self.flush_batch();
			self.text_string_texture_cache.delete_pending_textures(GlContext::from_context(&self.gl));
		}
		entry
	}

	//==============================================================================
	/// 출력 영역 제한 시작.
	/// - 스텐실 버퍼 기반이라 회전/스케일이 걸린 영역도 정확히 잘린다.
	/// - round_size 를 주면 둥근 사각형으로 잘린다.
	/// - 상태를 push_state 로 쌓으므로 end_clip_rect 와 짝을 맞춰 부른다. (end_clip_rect 가 pop_state 로 되돌린다) 겹쳐 걸면 교집합으로 잘린다.
	///
	/// # 인자
	/// - `rect`: 자를 사각형. (지금 변환 기준, 픽셀)
	/// - `round_size`: 모서리 반지름. (0 이하면 네모)
	///
	/// # 예제
	/// 둥근 칸 안에만 원 그리기
	/// ```rust,no_run
	/// use vanilla::{Colors, Graphic, Rect, Vector2};
	///
	/// fn draw(graphic: &mut Graphic) {
	/// 	graphic.begin_clip_rect(Rect::create(100.0, 100.0, 200.0, 100.0), 12.0);
	/// 	graphic.set_fill_color(Colors::WHITE);
	/// 	graphic.draw_circle(Vector2::create(100.0, 100.0), 80.0);
	/// 	graphic.end_clip_rect();
	/// }
	/// ```
	//==============================================================================
	pub fn begin_clip_rect(&mut self, rect: Rect, round_size: f64) {
		// 상태 저장. (Canvas2D save 대응)
		self.push_state();

		// 디바이스 픽셀 경계 상자. (네 꼭짓점을 옮긴 것, 거르기와 셰이더 자르기에 쓴다)
		let matrix = self.transform_matrix;
		let left_x = rect.position.x;
		let top_y = rect.position.y;
		let right_x = left_x + rect.size.x;
		let bottom_y = top_y + rect.size.y;
		let corner_list: [(f64, f64); 4] = [(left_x, top_y), (right_x, top_y), (left_x, bottom_y), (right_x, bottom_y)];
		let mut minimum_x = f64::MAX;
		let mut minimum_y = f64::MAX;
		let mut maximum_x = f64::MIN;
		let mut maximum_y = f64::MIN;
		for (corner_x, corner_y) in corner_list {
			let transformed_x = matrix.a * corner_x + matrix.c * corner_y + matrix.e;
			let transformed_y = matrix.b * corner_x + matrix.d * corner_y + matrix.f;
			minimum_x = minimum_x.min(transformed_x);
			minimum_y = minimum_y.min(transformed_y);
			maximum_x = maximum_x.max(transformed_x);
			maximum_y = maximum_y.max(transformed_y);
		}

		// 축에 맞는 각진 사각형은 스텐실 대신 셰이더로 자른다. (묶음을 끊지 않는다, 유니티 RectMask2D 와 같은 꼴)
		// 돌아갔거나 둥근 자르기, 화소 가운데 기준과 스텐실 결과가 갈릴 수 있는 경계는 스텐실로 자른다. 바깥 셰이더 자르기는 그 안에서도 그대로 걸린다.
		let is_axis_aligned = (matrix.b == 0.0 && matrix.c == 0.0) || (matrix.a == 0.0 && matrix.d == 0.0);
		let mut is_shader_clip = false;
		let mut shader_minimum_x = minimum_x;
		let mut shader_minimum_y = minimum_y;
		let mut shader_maximum_x = maximum_x;
		let mut shader_maximum_y = maximum_y;
		if round_size <= 0.0 && is_axis_aligned {
			if self.is_framebuffer_multisampled() {
				// 멀티샘플 화면에서 스텐실은 샘플마다 걸려 화소 사이 경계가 반쯤 칠해진다. 경계가 화소 경계에 놓일 때만 화소 가운데 기준과 같다. (반올림은 원본 Math.round 와 같게)
				shader_minimum_x = (minimum_x + 0.5).floor();
				shader_minimum_y = (minimum_y + 0.5).floor();
				shader_maximum_x = (maximum_x + 0.5).floor();
				shader_maximum_y = (maximum_y + 0.5).floor();
				is_shader_clip = (shader_minimum_x - minimum_x).abs() < CLIP_EDGE_EPSILON && (shader_minimum_y - minimum_y).abs() < CLIP_EDGE_EPSILON && (shader_maximum_x - maximum_x).abs() < CLIP_EDGE_EPSILON && (shader_maximum_y - maximum_y).abs() < CLIP_EDGE_EPSILON;
			}
			else {
				// 샘플이 하나면 스텐실도 화소 가운데로 고른다. 가운데에 딱 걸친 경계(동점 규칙이 기기마다 다를 수 있다)만 스텐실로 둔다.
				is_shader_clip = !is_clip_edge_on_pixel_center(minimum_x) && !is_clip_edge_on_pixel_center(minimum_y) && !is_clip_edge_on_pixel_center(maximum_x) && !is_clip_edge_on_pixel_center(maximum_y);
			}
		}
		let mut clip_entry = ClipEntry {
			rect,
			round_size,
			transform_matrix: matrix,
			is_shader_clip,
			is_shader_clip_active: is_shader_clip,
			shader_clip_rect: [shader_minimum_x as f32, shader_minimum_y as f32, shader_maximum_x as f32, shader_maximum_y as f32],
			shader_clip_slot: 0,
			shader_clip_slot_generation: -1,
			cull_minimum_x: minimum_x,
			cull_minimum_y: minimum_y,
			cull_maximum_x: maximum_x,
			cull_maximum_y: maximum_y,
		};
		if let Some(parent_entry) = self.clip_stack.last() {
			clip_entry.cull_minimum_x = parent_entry.cull_minimum_x.max(minimum_x);
			clip_entry.cull_minimum_y = parent_entry.cull_minimum_y.max(minimum_y);
			clip_entry.cull_maximum_x = parent_entry.cull_maximum_x.min(maximum_x);
			clip_entry.cull_maximum_y = parent_entry.cull_maximum_y.min(maximum_y);
			if parent_entry.is_shader_clip_active {
				let parent_rect = parent_entry.shader_clip_rect;
				clip_entry.is_shader_clip_active = true;
				clip_entry.shader_clip_rect = parent_rect;
				clip_entry.shader_clip_slot = parent_entry.shader_clip_slot;
				clip_entry.shader_clip_slot_generation = parent_entry.shader_clip_slot_generation;
				if is_shader_clip {
					clip_entry.shader_clip_rect = [(parent_rect[0] as f64).max(shader_minimum_x) as f32, (parent_rect[1] as f64).max(shader_minimum_y) as f32, (parent_rect[2] as f64).min(shader_maximum_x) as f32, (parent_rect[3] as f64).min(shader_maximum_y) as f32];
					clip_entry.shader_clip_slot_generation = -1;
				}
			}
		}
		self.clip_stack.push(clip_entry);
		self.apply_shader_clip_from_stack();
		if is_shader_clip {
			return;
		}

		// 클리핑 전에 쌓인 것을 먼저 그린다. (스텐실 상태가 바뀌기 전)
		self.flush_batch();
		let previous_stencil_clip_depth = self.stencil_clip_depth;
		self.stencil_clip_depth += 1;
		unsafe {
			if previous_stencil_clip_depth == 0 {
				crate::core::glstatecache::enable(&self.gl, glow::STENCIL_TEST);
			}

			// 스텐실에 클립 영역 기록. (색상 출력 없이 INCR)
			crate::core::glstatecache::color_mask(&self.gl, false, false, false, false);
			self.gl.stencil_func(glow::ALWAYS, 0, 0xFF);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::INCR);
		}
		self.batch_suspend_depth += 1;
		if round_size > 0.0 {
			self.draw_round_rect(rect, round_size);
		}
		else {
			self.draw_rect(rect);
		}
		self.batch_suspend_depth -= 1;
		unsafe {
			crate::core::glstatecache::color_mask(&self.gl, true, true, true, true);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
			self.gl.stencil_func(glow::EQUAL, self.stencil_clip_depth, 0xFF);
		}
	}

	//==============================================================================
	/// 투명도 층 시작. (CSS opacity, 어도비 투명도 묶음처럼 end_opacity_layer 까지 그린 것을 따로 칸에 모았다가 지금 전역 알파로 한 번에 얹는다)
	/// - 층 안의 그림은 서로 겹친 자리가 비치지 않는다. (노드마다 투명도를 곱해 따로 얹으면 겹친 자리에서 아래 그림이 비친다)
	/// - 층 안은 전역 알파 1 에서 시작한다. 층 칸은 화면과 같은 크기의 RGBA8 이고 깊이를 비우고 스텐실을 지금 자르기 깊이로 채워 둬 층 안의 자르기와 3D 패스(run_external_pass 의 is_depth_cleared 거짓)도 그대로 된다.
	///   바깥 자르기는 얹을 때 걸린다. 층은 겹쳐 열 수 있다. 칸을 지을 수 없으면 층 없이 그대로 그린다.
	/// - NodeTree 는 set_opacity_layer 를 켠 노드의 투명도가 1 보다 작으면 그 노드와 자손을 층으로 그린다.
	///
	/// # 예제
	/// 겹친 두 사각형을 묶어 반투명하게 (겹친 자리에서 아래 사각형이 비치지 않는다)
	/// ```rust,no_run
	/// use vanilla::{Graphic, Rect};
	///
	/// fn draw(graphic: &mut Graphic) {
	/// 	graphic.push_state();
	/// 	graphic.set_global_alpha(0.5);
	/// 	graphic.begin_opacity_layer();
	/// 	graphic.set_fill_color("#ff0000");
	/// 	graphic.draw_rect(Rect::create(100.0, 100.0, 200.0, 100.0));
	/// 	graphic.set_fill_color("#0000ff");
	/// 	graphic.draw_rect(Rect::create(200.0, 150.0, 200.0, 100.0));
	/// 	graphic.end_opacity_layer();
	/// 	graphic.pop_state();
	/// }
	/// ```
	//==============================================================================
	pub fn begin_opacity_layer(&mut self) {
		self.flush_batch();
		let previous_framebuffer = read_framebuffer_binding(GlContext::from_context(&self.gl));
		let global_alpha = self.global_alpha;
		let target = match self.acquire_opacity_layer_target(previous_framebuffer) {
			Ok(target) => {
				Some(target)
			}
			Err(error) => {
				eprintln!("Graphic: opacity layer failed: {}", error);
				None
			}
		};
		if let Some(target) = &target {
			let clip_depth = self.stencil_clip_depth;
			unsafe {
				crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, Some(target.framebuffer));
				crate::core::glstatecache::color_mask(&self.gl, true, true, true, true);
				self.gl.stencil_mask(0xFF);
				crate::core::glstatecache::depth_mask(&self.gl, true);
				self.gl.clear_buffer_f32_slice(glow::COLOR, 0, &[0.0, 0.0, 0.0, 0.0]);
				// 깊이는 1 로, 스텐실은 지금 자르기 깊이로 비운다. (지난 층의 깊이가 남으면 층 안의 깊이를 비우지 않는 3D 패스가 가려진다, 원본 clearBufferfi)
				self.gl.clear_buffer_depth_stencil(glow::DEPTH_STENCIL, 0, 1.0, clip_depth);
			}
			self.global_alpha = 1.0;
		}
		self.opacity_layer_stack.push(OpacityLayer {
			target,
			previous_framebuffer,
			global_alpha,
		});
	}

	//==============================================================================
	/// 투명도 층 끝. (begin_opacity_layer 뒤에 그린 것을 앞 프레임버퍼에 층을 열 때의 전역 알파로 얹는다)
	//==============================================================================
	pub fn end_opacity_layer(&mut self) {
		let opacity_layer = match self.opacity_layer_stack.pop() {
			Some(opacity_layer) => {
				opacity_layer
			}
			None => {
				return;
			}
		};
		self.flush_batch();
		self.global_alpha = opacity_layer.global_alpha;
		let target = match opacity_layer.target {
			Some(target) => {
				target
			}
			None => {
				return;
			}
		};
		unsafe {
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, opacity_layer.previous_framebuffer);
		}

		// 층 칸을 단위 행렬, 흰색, 틴트 없이 source-over 로 얹는다. (칸은 화면과 같은 크기라 디바이스 픽셀 그대로 짚는다, 텍스처는 아래가 0 행)
		let viewport_width = self.applied_viewport_width as f64;
		let viewport_height = self.applied_viewport_height as f64;
		let transform_matrix = self.transform_matrix;
		let image_tint_color = self.image_tint_color;
		let blend_mode = self.blend_mode;
		self.transform_matrix = TransformMatrix::identity();
		self.image_tint_color = None;
		self.set_blend_mode(BlendMode::Normal);
		let vertex_count = self.write_quad(0, 0.0, 0.0, viewport_width, viewport_height, 0.0, 1.0, 1.0, 0.0) / FLOATS_PER_VERTEX;
		let white_color = self.white_color;
		self.batch_suspend_depth += 1;
		self.draw_vertices(vertex_count, target.texture, white_color);
		self.batch_suspend_depth -= 1;
		self.set_blend_mode(blend_mode);
		self.transform_matrix = transform_matrix;
		self.image_tint_color = image_tint_color;
		self.opacity_layer_target_pool.push(target);
	}

	//==============================================================================
	/// 투명도 층 칸 꺼내기. (모아 둔 것에서 화면 크기와 같은 것, 없으면 짓는다)
	//==============================================================================
	fn acquire_opacity_layer_target(&mut self, framebuffer: Option<GlFramebuffer>) -> Result<OpacityLayerTarget, String> {
		let viewport_width = self.applied_viewport_width;
		let viewport_height = self.applied_viewport_height;
		while let Some(target) = self.opacity_layer_target_pool.pop() {
			if target.width == viewport_width && target.height == viewport_height {
				return Ok(target);
			}
			self.destroy_opacity_layer_target(target);
		}
		unsafe {
			let texture = crate::core::glstatecache::create_texture(&self.gl)?;
			self.bind_main_texture(texture);
			self.gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, viewport_width, viewport_height, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			let stencil_renderbuffer = self.gl.create_renderbuffer()?;
			self.gl.bind_renderbuffer(glow::RENDERBUFFER, Some(stencil_renderbuffer));
			self.gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, viewport_width, viewport_height);
			self.gl.bind_renderbuffer(glow::RENDERBUFFER, None);
			let target_framebuffer = crate::core::glstatecache::create_framebuffer(&self.gl)?;
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, Some(target_framebuffer));
			self.gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
			self.gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(stencil_renderbuffer));
			let status = self.gl.check_framebuffer_status(glow::FRAMEBUFFER);
			crate::core::glstatecache::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, framebuffer);
			let target = OpacityLayerTarget {
				texture,
				framebuffer: target_framebuffer,
				stencil_renderbuffer,
				width: viewport_width,
				height: viewport_height,
			};
			if status != glow::FRAMEBUFFER_COMPLETE {
				self.destroy_opacity_layer_target(target);
				return Err(format!("framebuffer status 0x{:X}", status));
			}
			Ok(target)
		}
	}

	//==============================================================================
	/// 투명도 층 칸 지우기.
	//==============================================================================
	fn destroy_opacity_layer_target(&self, target: OpacityLayerTarget) {
		unsafe {
			crate::core::glstatecache::delete_framebuffer(&self.gl, target.framebuffer);
			self.gl.delete_renderbuffer(target.stencil_renderbuffer);
			crate::core::glstatecache::delete_texture(&self.gl, target.texture);
		}
	}

	//==============================================================================
	/// 출력 영역 제한 종료.
	//==============================================================================
	pub fn end_clip_rect(&mut self) {
		let clip_entry = match self.clip_stack.pop() {
			Some(clip_entry) => {
				clip_entry
			}
			None => {
				return;
			}
		};

		// 셰이더 자르기는 바깥 자르기로 돌아가기만 한다. (묶음을 끊지 않는다)
		if clip_entry.is_shader_clip {
			self.apply_shader_clip_from_stack();
			self.pop_state();
			return;
		}

		// 클립 안에서 모아 둔 것을 먼저 내보낸다.
		self.flush_batch();

		// 기록 시점의 변환 행렬로 같은 영역을 DECR 해 스텐실 원복.
		// (begin 과 end 사이에 변환이 바뀌었을 수 있으므로 스냅샷 사용)
		self.transform_matrix = clip_entry.transform_matrix;
		unsafe {
			crate::core::glstatecache::color_mask(&self.gl, false, false, false, false);
			self.gl.stencil_func(glow::ALWAYS, 0, 0xFF);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::DECR);
		}
		self.batch_suspend_depth += 1;
		if clip_entry.round_size > 0.0 {
			self.draw_round_rect(clip_entry.rect, clip_entry.round_size);
		}
		else {
			self.draw_rect(clip_entry.rect);
		}
		self.batch_suspend_depth -= 1;
		unsafe {
			crate::core::glstatecache::color_mask(&self.gl, true, true, true, true);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);

			self.stencil_clip_depth -= 1;
			if self.stencil_clip_depth == 0 {
				crate::core::glstatecache::disable(&self.gl, glow::STENCIL_TEST);
			}
			else {
				self.gl.stencil_func(glow::EQUAL, self.stencil_clip_depth, 0xFF);
			}
		}
		self.apply_shader_clip_from_stack();

		// 상태 복원. (Canvas2D restore 대응)
		self.pop_state();
	}

	//==============================================================================
	/// 기본 프레임버퍼가 멀티샘플인지. (프레임 첫머리에 한 번 읽어 둔다, 모르면 멀티샘플로 본다)
	/// - 멀티샘플로 보면 셰이더 자르기를 정수 경계에만 써 어느 쪽이든 스텐실과 화소가 같다.
	//==============================================================================
	pub(crate) fn is_framebuffer_multisampled(&self) -> bool {
		self.framebuffer_sample_state != 0
	}

	//==============================================================================
	/// 자르기 칸 맨 위의 셰이더 자르기를 지금 상태로 삼기. (begin_clip_rect, end_clip_rect 가 부른다, 원본 applyShaderClipFromStack)
	//==============================================================================
	fn apply_shader_clip_from_stack(&mut self) {
		match self.clip_stack.last() {
			Some(top_entry) if top_entry.is_shader_clip_active => {
				self.is_shader_clip_active = true;
				self.shader_clip_rect = top_entry.shader_clip_rect;
				self.shader_clip_slot = top_entry.shader_clip_slot;
				self.shader_clip_slot_generation = top_entry.shader_clip_slot_generation;
			}
			_ => {
				self.is_shader_clip_active = false;
				self.shader_clip_slot = 0;
			}
		}
	}

	//==============================================================================
	/// 묶지 않고 그리는 것에 셰이더 자르기를 가위로 걸기. (셰이더 자르기가 없으면 아무것도 하지 않는다, 원본 applyClipScissor)
	/// - 화소 가운데가 [왼쪽, 오른쪽) x [위, 아래) 안인 화소만 남기는 것은 셰이더 자르기와 같다. (가위 칸은 화소 단위라 가운데 기준으로 올림한다)
	/// - 그린 뒤 release_clip_scissor 로 푼다. (묶음을 그릴 때와 지우기는 가위를 쓰지 않는다)
	//==============================================================================
	fn apply_clip_scissor(&mut self) {
		if !self.is_shader_clip_active {
			return;
		}
		let shader_clip_rect = self.shader_clip_rect;
		let left_x = (shader_clip_rect[0] as f64 - 0.5).ceil();
		let top_y = (shader_clip_rect[1] as f64 - 0.5).ceil();
		let right_x = left_x.max((shader_clip_rect[2] as f64 - 0.5).ceil());
		let bottom_y = top_y.max((shader_clip_rect[3] as f64 - 0.5).ceil());
		unsafe {
			crate::core::glstatecache::enable(&self.gl, glow::SCISSOR_TEST);
			crate::core::glstatecache::scissor(&self.gl, left_x as i32, self.applied_viewport_height - bottom_y as i32, (right_x - left_x) as i32, (bottom_y - top_y) as i32);
		}
		self.is_clip_scissor_enabled = true;
	}

	//==============================================================================
	/// apply_clip_scissor 로 건 가위 풀기.
	//==============================================================================
	fn release_clip_scissor(&mut self) {
		if !self.is_clip_scissor_enabled {
			return;
		}
		unsafe {
			crate::core::glstatecache::disable(&self.gl, glow::SCISSOR_TEST);
		}
		self.is_clip_scissor_enabled = false;
	}

	//==============================================================================
	/// 셰이더 자르기를 스텐실에 적거나 지우기. (바깥 GL 패스가 스텐실 자르기만 알므로 run_external_pass 가 앞뒤로 부른다, 원본 writeShaderClipsToStencil)
	/// - 걸린 셰이더 자르기 사각형을 저마다 걸 때의 변환으로 INCR(적기) 또는 DECR(지우기) 한다. 적은 동안 스텐실 비교 값은 걸린 자르기 수 모두다.
	/// - 지울 때는 패스가 스텐실 테스트를 껐을 수 있어 다시 켜고 지운다. (꺼져 있으면 DECR 이 스텐실에 닿지 않는다)
	///
	/// # 인자
	/// - `is_writing`: 적을지. (false 면 지운다)
	///
	/// # 돌려줌
	/// 적거나 지운 셰이더 자르기 수.
	//==============================================================================
	fn write_shader_clips_to_stencil(&mut self, is_writing: bool) -> usize {
		let shader_clip_list: Vec<(Rect, TransformMatrix)> = self.clip_stack.iter().filter(|clip_entry| clip_entry.is_shader_clip).map(|clip_entry| (clip_entry.rect, clip_entry.transform_matrix)).collect();
		let shader_clip_count = shader_clip_list.len();
		if shader_clip_count == 0 {
			return 0;
		}
		self.flush_batch();
		let is_shader_clip_active = self.is_shader_clip_active;
		self.is_shader_clip_active = false;
		self.push_state();
		unsafe {
			if !is_writing || self.stencil_clip_depth == 0 {
				crate::core::glstatecache::enable(&self.gl, glow::STENCIL_TEST);
			}
			crate::core::glstatecache::color_mask(&self.gl, false, false, false, false);
			self.gl.stencil_func(glow::ALWAYS, 0, 0xFF);
			let stencil_operation = if is_writing {
				glow::INCR
			}
			else {
				glow::DECR
			};
			self.gl.stencil_op(glow::KEEP, glow::KEEP, stencil_operation);
		}
		self.batch_suspend_depth += 1;
		for (rect, transform_matrix) in shader_clip_list {
			self.transform_matrix = transform_matrix;
			self.draw_rect(rect);
		}
		self.batch_suspend_depth -= 1;
		unsafe {
			crate::core::glstatecache::color_mask(&self.gl, true, true, true, true);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
			let stencil_depth = if is_writing {
				self.clip_stack.len() as i32
			}
			else {
				self.stencil_clip_depth
			};
			if stencil_depth == 0 {
				crate::core::glstatecache::disable(&self.gl, glow::STENCIL_TEST);
			}
			else {
				self.gl.stencil_func(glow::EQUAL, stencil_depth, 0xFF);
			}
		}
		self.pop_state();
		self.is_shader_clip_active = is_shader_clip_active;
		shader_clip_count
	}

	//==============================================================================
	/// 셰이더 프로그램 반환. (바꿔 끼운 것이 있으면 그것, 없으면 기본 프로그램)
	///
	/// # 돌려줌
	/// 바꿔 끼운 프로그램이 있으면 그것, 없으면 기본 프로그램.
	//==============================================================================
	pub fn get_shader_program(&self) -> &ShaderProgram {
		match &self.shader_program_override {
			Some(shader_program_override) => {
				shader_program_override
			}
			None => {
				&self.shader_program
			}
		}
	}

	//==============================================================================
	/// 셰이더 프로그램 바꿔 끼우기. (None 이면 기본 프로그램으로 복귀)
	/// - 기본 프로그램과 같은 어트리뷰트 위치 / 유니폼 이름(projectionMatrix, modelMatrix, mainColor, tintColor, globalAlpha, mainTexture)을 갖춰야 한다.
	/// - 바꿔 끼운 프로그램이 쓰이는 동안 그 프로그램이 사용 중이다. 효과 유니폼은 바꿔 끼운 바로 뒤에 GL 로 직접 올린다.
	///
	/// # 인자
	/// - `shader_program`: 바꿔 끼울 프로그램. (resolve_shader_program 으로 받는다, None 이면 기본 프로그램)
	//==============================================================================
	pub fn set_shader_program_override(&mut self, shader_program: Option<Rc<ShaderProgram>>) {
		self.flush_batch();
		self.shader_program_override = shader_program;
		let active_shader_program = self.get_shader_program();
		self.use_shader_program(active_shader_program);
		let projection_matrix_location = active_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "projectionMatrix");
		let main_texture_location = active_shader_program.get_uniform_location(GlContext::from_context(&self.gl), "mainTexture");
		unsafe {
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
		}
		self.apply_image_mipmap_bias(active_shader_program);
	}

	//==============================================================================
	/// 이름으로 담아 둔 바꿔 끼울 셰이더 프로그램 반환. (없으면 create_shader_source 가 준 버텍스 / 프래그먼트 소스로 만들어 담는다)
	/// - 원본 ShaderSprite 의 컨텍스트별 프로그램 캐시 자리. 러스트 판은 GL 컨텍스트를 든 Graphic 이 담는다.
	/// - 어트리뷰트 위치를 기본 프로그램과 같게 고정해 링크하므로 기본 버텍스 어레이를 그대로 쓴다.
	/// - 만들기에 실패하면 한 번만 알리고 None 을 담아 두어 다시 만들지 않는다.
	///
	/// # 인자
	/// - `program_group_name`: 묶음 이름. (효과 종류 같은 것, 열쇠의 앞)
	/// - `program_name`: 프로그램 이름. (열쇠의 뒤)
	/// - `create_shader_source`: 담긴 것이 없을 때만 부르는 함수. ((버텍스 소스, 프래그먼트 소스), GLSL ES 3.00)
	///
	/// # 돌려줌
	/// 담아 둔 프로그램. 만들기에 실패했으면 None. (그 뒤로도 None)
	///
	/// # 예제
	/// 흑백 셰이더로 바꿔 끼워 그림 그리기
	/// ```rust,no_run
	/// use std::rc::Rc;
	/// use vanilla::{Graphic, Image, Vector2};
	///
	/// const GRAY_VERTEXSHADER_SOURCE: &str = "#version 300 es
	/// in vec2 vertexPosition;
	/// in vec2 vertexTextureCoordinate;
	/// uniform mat3 projectionMatrix;
	/// uniform mat3 modelMatrix;
	/// out vec2 fragmentTextureCoordinate;
	/// void main() {
	/// 	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	/// 	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	/// 	fragmentTextureCoordinate = vertexTextureCoordinate;
	/// }
	/// ";
	///
	/// const GRAY_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
	/// precision highp float;
	/// in vec2 fragmentTextureCoordinate;
	/// uniform sampler2D mainTexture;
	/// uniform vec4 mainColor;
	/// uniform float globalAlpha;
	/// out vec4 outputColor;
	/// void main() {
	/// 	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate);
	/// 	float gray = dot(textureColor.rgb, vec3(0.299, 0.587, 0.114));
	/// 	float alpha = textureColor.a * mainColor.a * globalAlpha;
	/// 	outputColor = vec4(vec3(gray) * mainColor.a * globalAlpha, alpha);
	/// }
	/// ";
	///
	/// fn draw_gray(graphic: &mut Graphic, image: &Rc<Image>) {
	/// 	let shader_program = graphic.resolve_shader_program("example", "gray", || (GRAY_VERTEXSHADER_SOURCE.to_string(), GRAY_FRAGMENTSHADER_SOURCE.to_string()));
	/// 	graphic.set_shader_program_override(shader_program);
	/// 	graphic.draw_image(image, Vector2::create(0.0, 0.0), Vector2::create(128.0, 128.0));
	/// 	graphic.set_shader_program_override(None);
	/// }
	/// ```
	//==============================================================================
	pub fn resolve_shader_program(&mut self, program_group_name: &'static str, program_name: &'static str, create_shader_source: impl FnOnce() -> (String, String)) -> Option<Rc<ShaderProgram>> {
		let cache_key = (program_group_name, program_name);
		if let Some(cached_shader_program) = self.shader_program_cache_table.get(&cache_key) {
			return cached_shader_program.clone();
		}
		let (vertex_shader_source, fragment_shader_source) = create_shader_source();
		let mut attribute_location_list: Vec<(&str, u32)> = Vec::new();
		for attribute_name in ["vertexPosition", "vertexTextureCoordinate"] {
			if let Some(attribute_location) = self.shader_program.get_attribute_location(GlContext::from_context(&self.gl), attribute_name) {
				attribute_location_list.push((attribute_name, attribute_location));
			}
		}
		let shader_program = match ShaderProgram::create_with_attribute_locations(GlContext::from_context(&self.gl), &vertex_shader_source, &fragment_shader_source, &attribute_location_list) {
			Ok(shader_program) => {
				Some(Rc::new(shader_program))
			}
			Err(error) => {
				eprintln!("Graphic: shader program \"{}.{}\" failed: {}", program_group_name, program_name, error);
				None
			}
		};
		self.shader_program_cache_table.insert(cache_key, shader_program.clone());
		shader_program
	}

	//==============================================================================
	/// 렌더 상태 복구. (외부 GL 패스가 프로그램 / 버텍스 어레이 / 뷰포트 / 블렌드를 바꾼 뒤 Graphic 출력을 이어갈 때, 원본 restoreRenderState)
	/// - 스텐실 자르기 상태는 되돌리지 않는다. (run_external_pass 가 끝에 되돌린다, 원본과 같음)
	//==============================================================================
	pub fn restore_render_state(&mut self) {
		unsafe {
			crate::core::glstatecache::viewport(&self.gl, 0, 0, self.applied_viewport_width, self.applied_viewport_height);
			crate::core::glstatecache::disable(&self.gl, glow::SCISSOR_TEST);
		}
		self.is_clip_scissor_enabled = false;
		let shader_program = self.get_shader_program();
		self.use_shader_program(shader_program);
		let projection_matrix_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "projectionMatrix");
		let main_texture_location = shader_program.get_uniform_location(GlContext::from_context(&self.gl), "mainTexture");
		unsafe {
			crate::core::glstatecache::bind_vertex_array(&self.gl, Some(self.vertex_array));
			crate::core::glstatecache::bind_buffer(&self.gl, glow::ARRAY_BUFFER, Some(self.vertex_buffer));
			self.gl.uniform_matrix_3_f32_slice(projection_matrix_location.as_ref(), false, &self.projection_matrix_array);
			self.gl.uniform_1_i32(main_texture_location.as_ref(), 0);
			crate::core::glstatecache::active_texture(&self.gl, glow::TEXTURE0);
			crate::core::glstatecache::enable(&self.gl, glow::BLEND);
			crate::core::glstatecache::disable(&self.gl, glow::DEPTH_TEST);
			crate::core::glstatecache::disable(&self.gl, glow::CULL_FACE);
		}
		let blend_mode = self.get_blend_mode();
		self.apply_blend_mode(blend_mode);
	}

	//==============================================================================
	/// 스텐실 자르기 상태 되돌리기. (원본 restoreStencilClipState, 셰이더 자르기가 없을 때 run_external_pass 가 끝에 부른다)
	/// - 바깥 패스가 스텐실 테스트를 끄거나 스텐실 함수, 연산, 마스크를 바꿨어도 지금 스텐실 자르기 깊이대로 다시 건다.
	//==============================================================================
	fn restore_stencil_clip_state(&mut self) {
		unsafe {
			self.gl.stencil_mask(0xFF);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
			let stencil_depth = self.stencil_clip_depth;
			if stencil_depth == 0 {
				crate::core::glstatecache::disable(&self.gl, glow::STENCIL_TEST);
				return;
			}
			crate::core::glstatecache::enable(&self.gl, glow::STENCIL_TEST);
			self.gl.stencil_func(glow::EQUAL, stencil_depth, 0xFF);
		}
	}

	//==============================================================================
	/// 드로잉 버퍼 너비 반환. (픽셀, 원본 webGL2RenderingContext.drawingBufferWidth)
	///
	/// # 돌려줌
	/// 마지막 apply_settings 의 드로잉 버퍼 너비. (픽셀)
	//==============================================================================
	pub fn get_drawing_buffer_width(&self) -> i32 {
		self.applied_viewport_width
	}

	//==============================================================================
	/// 드로잉 버퍼 높이 반환. (픽셀, 원본 webGL2RenderingContext.drawingBufferHeight)
	///
	/// # 돌려줌
	/// 마지막 apply_settings 의 드로잉 버퍼 높이. (픽셀)
	//==============================================================================
	pub fn get_drawing_buffer_height(&self) -> i32 {
		self.applied_viewport_height
	}

	//==============================================================================
	/// 지금 그린 화면 읽기. (너비, 높이, 위에서 아래로 놓인 RGBA. 화면을 넘기기 전에 부른다)
	/// - 원본 캔버스처럼 불투명으로 본다. (알파 255)
	///
	/// # 돌려줌
	/// (너비, 높이, 픽셀). 픽셀은 위 줄부터 놓인 RGBA 바이트(너비 x 높이 x 4)이고 알파는 모두 255 다. (지금 읽기 프레임버퍼에서 읽는다)
	//==============================================================================
	pub fn read_framebuffer_pixels(&self) -> (u32, u32, Vec<u8>) {
		let width = self.applied_viewport_width.max(0) as usize;
		let height = self.applied_viewport_height.max(0) as usize;
		let row_byte_count = width * 4;
		let mut bottom_up_pixels = vec![0u8; row_byte_count * height];
		unsafe {
			crate::core::glstatecache::pixel_store_i32(&self.gl, glow::PACK_ALIGNMENT, 1);
			self.gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut bottom_up_pixels)));
		}
		let mut top_down_pixels = vec![0u8; row_byte_count * height];
		for row_index in 0..height {
			let source_start = (height - 1 - row_index) * row_byte_count;
			let destination_start = row_index * row_byte_count;
			top_down_pixels[destination_start..destination_start + row_byte_count].copy_from_slice(&bottom_up_pixels[source_start..source_start + row_byte_count]);
		}
		for pixel in top_down_pixels.as_chunks_mut::<4>().0 {
			pixel[3] = 255;
		}
		(width as u32, height as u32, top_down_pixels)
	}

	//==============================================================================
	/// GL 컨텍스트 반환. (원본 getWebGL2RenderingContext)
	/// - 받은 컨텍스트(GlContext)의 상태 함수는 GL 상태 캐시를 거치므로, 받은 뒤 바꾼 상태를 엔진이 안다.
	///   Graphic 은 묶음과 2D 상태를 따로 기억하지 않고 그리기 앞에 상태 캐시에 견주므로, 받은 쪽이 프로그램, 텍스처, 섞기 같은 것을 바꿔도 다음 2D 그리기가 다시 건다.
	/// - 색 쓰기 마스크는 모든 채널로 되돌려 준다. (데스크탑 기본 프레임버퍼에 알파를 쓰지 않으려 Graphic 이 끈 것)
	/// - GL 상태 캐시는 잊지 않는다. (원본과 같음, 안의 glow::Context 는 꺼낼 수 없어 받은 쪽이 상태 캐시를 건너뛰지 못한다)
	///
	/// # 돌려줌
	/// Graphic 이 든 GL 컨텍스트. 엔진 함수의 `&GlContext` 인자로 그대로 넘긴다. (안의 glow::Context 는 꺼낼 수 없다)
	//==============================================================================
	pub fn get_gl(&self) -> &GlContext {
		self.release_color_mask();
		GlContext::from_context(&self.gl)
	}

	//==============================================================================
	/// GL 컨텍스트 반환. (엔진 안에서, 상태 캐시를 거쳐 GL 을 부르는 쪽이 쓴다, 상태 캐시를 잊지 않는다)
	//==============================================================================
	pub(crate) fn get_tracked_gl(&self) -> &GlContext {
		self.release_color_mask();
		GlContext::from_context(&self.gl)
	}

	//==============================================================================
	/// 지금 그리는 프레임 버퍼 반환. (원본 getCurrentFramebuffer, 상태 캐시가 기억한 값, 그리는 중 GL 에 묻지 않는다)
	///
	/// # 돌려줌
	/// 지금 묶인 그리기 프레임 버퍼. 기본 프레임 버퍼(창, 캔버스)면 None.
	///
	/// # 예제
	/// 장면에서 지금 프레임 버퍼와 뷰포트 읽기
	/// ```rust,no_run
	/// use vanilla::Graphic;
	///
	/// fn draw(graphic: &mut Graphic) {
	/// 	let framebuffer = graphic.get_current_framebuffer();
	/// 	let viewport = graphic.get_current_viewport();
	/// 	println!("{} {} {}", framebuffer.is_none(), viewport.get_width(), viewport.get_height());
	/// 	graphic.reset_gl_state_cache();
	/// }
	/// ```
	//==============================================================================
	pub fn get_current_framebuffer(&self) -> Option<GlFramebuffer> {
		read_framebuffer_binding(GlContext::from_context(&self.gl))
	}

	//==============================================================================
	/// 지금 뷰포트 반환. (원본 getCurrentViewport, 픽셀, 왼쪽 아래가 원점, 상태 캐시가 기억한 값)
	///
	/// # 돌려줌
	/// 뷰포트 사각형. (x, y, 너비, 높이)
	//==============================================================================
	pub fn get_current_viewport(&self) -> Rect {
		let mut viewport = [0i32; 4];
		unsafe {
			crate::core::glstatecache::get_parameter_i32_slice(&self.gl, glow::VIEWPORT, &mut viewport);
		}
		Rect::create(viewport[0] as f64, viewport[1] as f64, viewport[2] as f64, viewport[3] as f64)
	}

	//==============================================================================
	/// GL 상태 캐시 잊기. (원본 resetGLStateCache, 상태 캐시를 거치지 않고 GL 상태를 바꾼 뒤 부른다, 다음에 읽을 때 GL 에 한 번씩 묻는다)
	/// - get_gl, run_external_pass, 툰 렌더러의 콜백이 주는 GlContext 는 상태 함수가 상태 캐시를 거치므로, 같은 콜백 안에서 엔진 그리기를 이어 불러도 부를 것 없다.
	/// - GlContext 는 안의 glow::Context 를 꺼낼 길이 없어 게임 코드로는 상태 캐시를 건너뛰지 못한다.
	///   다른 라이브러리가 같은 GL 컨텍스트의 상태를 바꾼 뒤 같은 콜백 안에서 엔진 그리기를 이어 부를 때 부른다.
	/// - 엔진은 GL 을 넘기는 자리(get_gl, run_external_pass, 툰 렌더러의 콜백)에서 스스로 잊지 않는다. (원본과 같음)
	/// - 상태 캐시를 거치지 않은 호출을 찾으려면 환경 변수 VANILLA_GL_STATE_CHECK=1 로 돌린다. (엔진 그리기 앞마다 기억한 값과 진짜 GL 을 견주어 어긋난 곳과 앞의 상태 호출을 표준 오류에 남긴다)
	//==============================================================================
	pub fn reset_gl_state_cache(&self) {
		crate::core::glstatecache::reset(&self.gl);
	}

	//==============================================================================
	/// 버텍스 어레이 반환. (기본 그리기의 것, GL 을 만지려면 get_gl 로 컨텍스트를 받는다)
	///
	/// # 돌려줌
	/// 기본 버텍스 어레이.
	//==============================================================================
	pub fn get_vertex_array(&self) -> GlVertexArray {
		self.vertex_array
	}

	//==============================================================================
	/// 버텍스 버퍼 반환.
	///
	/// # 돌려줌
	/// 기본 버텍스 버퍼.
	//==============================================================================
	pub fn get_vertex_buffer(&self) -> GlBuffer {
		self.vertex_buffer
	}

	//==============================================================================
	/// 스크래치 버텍스 데이터 반환. (write_quad 와 draw_vertices 가 쓰는 배열)
	///
	/// # 돌려줌
	/// 스크래치 버텍스 배열. (정점 4096 개, 정점마다 x, y, u, v 플로트 4 개)
	//==============================================================================
	pub fn get_vertex_data(&mut self) -> &mut [f32] {
		&mut self.vertex_data
	}

	//==============================================================================
	/// 흰색 텍스처 반환.
	///
	/// # 돌려줌
	/// 1 x 1 흰색 텍스처. (단색 그리기에 쓴다)
	//==============================================================================
	pub fn get_white_texture(&self) -> GlTexture {
		self.white_texture
	}

	//==============================================================================
	/// 흰색 반환.
	///
	/// # 돌려줌
	/// 흰색. (draw_vertices 에 텍스처 색 그대로 그릴 때 넘긴다)
	//==============================================================================
	pub fn get_white_color(&self) -> Color {
		self.white_color
	}

	//==============================================================================
	/// 이미지 텍스처 캐시 반환.
	///
	/// # 돌려줌
	/// 이미지 텍스처 캐시.
	//==============================================================================
	pub fn get_image_texture_cache(&self) -> &ImageTextureCache {
		&self.image_texture_cache
	}

	//==============================================================================
	/// 문자열 텍스처 캐시 반환. (폰트 등록)
	///
	/// # 돌려줌
	/// 문자열 텍스처 캐시.
	//==============================================================================
	pub fn get_text_string_texture_cache(&mut self) -> &mut TextStringTextureCache {
		&mut self.text_string_texture_cache
	}
}

//==============================================================================
/// GL 의 묶인 것이 있는지. (read_*_binding 이 glow 로 읽기 전에 본다)
/// - 웹의 glow 는 묶인 것이 없을 때(null) 읽으면 "A resource was created externally" 로 패닉한다.
///   그래서 웹은 플랫폼 층이 들고 있는 WebGL2 컨텍스트로 먼저 null 인지 보고, 묶인 것이 있을 때만 glow 로 읽는다. 데스크탑 glow 는 0 을 None 으로 준다.
//==============================================================================
fn is_gl_binding_present(parameter: u32) -> bool {
	#[cfg(target_arch = "wasm32")]
	{
		crate::platform::is_gl_binding_present(parameter)
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		let _ = parameter;
		true
	}
}

//==============================================================================
/// 기본 프레임버퍼가 멀티샘플인지 읽기. (원본 getContextAttributes().antialias, 기본 프레임버퍼가 묶여 있지 않으면 None)
/// - 웹은 원본처럼 컨텍스트 속성 antialias 를 본다. 데스크탑은 기본 프레임버퍼의 SAMPLES 를 읽는다. (창을 멀티샘플 4 로 짓고, 안 되면 표본 하나로 짓는다)
//==============================================================================
fn read_default_framebuffer_multisampled(gl: &GlContext) -> Option<bool> {
	#[cfg(target_arch = "wasm32")]
	{
		let _ = gl;
		Some(crate::platform::is_default_framebuffer_antialiased())
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		if read_framebuffer_binding(gl).is_some() {
			return None;
		}
		let sample_count = unsafe { crate::core::glstatecache::get_parameter_i32(gl, glow::SAMPLES) };
		Some(sample_count > 1)
	}
}

//==============================================================================
/// 자르기 사각형 표 짓기. (0 번 칸은 자르지 않는 칸)
//==============================================================================
fn create_batch_clip_rect_data() -> [f32; BATCH_CLIP_SLOT_CAPACITY * 4] {
	let mut clip_rect_data = [0.0f32; BATCH_CLIP_SLOT_CAPACITY * 4];
	clip_rect_data[0] = -BATCH_UNCLIPPED_EXTENT;
	clip_rect_data[1] = -BATCH_UNCLIPPED_EXTENT;
	clip_rect_data[2] = BATCH_UNCLIPPED_EXTENT;
	clip_rect_data[3] = BATCH_UNCLIPPED_EXTENT;
	clip_rect_data
}

//==============================================================================
/// 지금 묶인 프레임버퍼 읽기. (앞의 것을 돌려 놓을 때, 원본 getParameter(FRAMEBUFFER_BINDING), 기본 프레임버퍼면 None)
//==============================================================================
pub(crate) fn read_framebuffer_binding(gl: &GlContext) -> Option<<glow::Context as glow::HasContext>::Framebuffer> {
	crate::core::glstatecache::read_draw_framebuffer(gl, || {
		if !is_gl_binding_present(glow::FRAMEBUFFER_BINDING) {
			return None;
		}
		unsafe {
			gl.get_glow_context_unchecked().get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING)
		}
	})
}

//==============================================================================
/// 지금 유닛에 묶인 2D 텍스처 읽기. (원본 getParameter(TEXTURE_BINDING_2D))
//==============================================================================
pub(crate) fn read_texture_binding(gl: &GlContext) -> Option<GlTexture> {
	crate::core::glstatecache::read_texture_binding_2d(gl, || {
		if !is_gl_binding_present(glow::TEXTURE_BINDING_2D) {
			return None;
		}
		unsafe {
			gl.get_glow_context_unchecked().get_parameter_texture(glow::TEXTURE_BINDING_2D)
		}
	})
}

//==============================================================================
/// 지금 쓰는 셰이더 프로그램 읽기. (원본 getParameter(CURRENT_PROGRAM))
//==============================================================================
pub(crate) fn read_program_binding(gl: &GlContext) -> Option<GlProgram> {
	crate::core::glstatecache::read_program(gl, || {
		if !is_gl_binding_present(glow::CURRENT_PROGRAM) {
			return None;
		}
		unsafe {
			gl.get_glow_context_unchecked().get_parameter_program(glow::CURRENT_PROGRAM)
		}
	})
}

//==============================================================================
/// 지금 묶인 버텍스 어레이 읽기. (원본 getParameter(VERTEX_ARRAY_BINDING))
//==============================================================================
pub(crate) fn read_vertex_array_binding(gl: &GlContext) -> Option<GlVertexArray> {
	crate::core::glstatecache::read_vertex_array(gl, || {
		if !is_gl_binding_present(glow::VERTEX_ARRAY_BINDING) {
			return None;
		}
		unsafe {
			gl.get_glow_context_unchecked().get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING)
		}
	})
}

//==============================================================================
/// 폴리라인의 선분 수. (닫힌 선은 마지막 점과 첫 점을 잇는 선분 하나가 더 있다)
///
/// # 인자
/// - `position_count`: 점 수.
/// - `is_closed`: 닫힌 선인지.
///
/// # 돌려줌
/// 선분 수. (점이 2 개보다 적으면 0)
//==============================================================================
fn read_polyline_segment_count(position_count: usize, is_closed: bool) -> usize {
	if position_count < 2 {
		return 0;
	}
	if is_closed {
		position_count
	}
	else {
		position_count - 1
	}
}

//==============================================================================
/// 폴리라인의 선분을 정점 버퍼에 들어가는 만큼 적는다. (세그먼트별 두께 확장 쿼드, draw_polyline 이 버퍼가 찰 때마다 그리고 이어 부른다)
/// - 내부 조인트는 반두께 연장으로 이음새를 메운다. 길이가 0 인 선분은 건너뛴다.
///
/// # 인자
/// - `vertex_data`: 적을 정점 버퍼. (정점마다 x, y, u, v, 처음부터 적는다)
/// - `positions`: 선이 지날 점 목록.
/// - `half_size`: 선 굵기의 반.
/// - `is_closed`: 참이면 마지막 점과 첫 점을 잇는다.
/// - `first_segment_index`: 이번에 처음 적을 선분 번호.
///
/// # 돌려줌
/// (적은 정점 수, 다음에 이어 적을 선분 번호). 다음 선분 번호가 선분 수와 같으면 다 적은 것이다.
//==============================================================================
fn write_polyline_vertices(vertex_data: &mut [f32], positions: &[Vector2], half_size: f64, is_closed: bool, first_segment_index: usize) -> (usize, usize) {
	let segment_count = read_polyline_segment_count(positions.len(), is_closed);
	let mut offset = 0;
	let mut vertex_count = 0;
	let mut segment_index = first_segment_index;
	while segment_index < segment_count {
		let start_position = positions[segment_index];
		let end_position = positions[(segment_index + 1) % positions.len()];
		let delta_x = end_position.x - start_position.x;
		let delta_y = end_position.y - start_position.y;
		let length = (delta_x * delta_x + delta_y * delta_y).sqrt();
		if length <= 0.0 {
			segment_index += 1;
			continue;
		}

		// 쿼드(삼각형 2개)가 들어갈 자리가 없으면 여기까지.
		if (vertex_count + 6) * FLOATS_PER_VERTEX > vertex_data.len() {
			break;
		}

		let direction_x = delta_x / length;
		let direction_y = delta_y / length;
		let normal_x = -direction_y * half_size;
		let normal_y = direction_x * half_size;

		// 내부 조인트 이음새 메움. (열린 선의 양 끝은 연장하지 않음)
		let has_start_joint = is_closed || segment_index > 0;
		let has_end_joint = is_closed || segment_index < segment_count - 1;
		let start_extension = if has_start_joint {
			half_size
		}
		else {
			0.0
		};
		let end_extension = if has_end_joint {
			half_size
		}
		else {
			0.0
		};
		let extended_start_x = start_position.x - direction_x * start_extension;
		let extended_start_y = start_position.y - direction_y * start_extension;
		let extended_end_x = end_position.x + direction_x * end_extension;
		let extended_end_y = end_position.y + direction_y * end_extension;

		// 쿼드(삼각형 2개) 기록.
		let segment_values: [f32; 24] = [
			(extended_start_x + normal_x) as f32, (extended_start_y + normal_y) as f32, 0.5, 0.5,
			(extended_end_x + normal_x) as f32, (extended_end_y + normal_y) as f32, 0.5, 0.5,
			(extended_start_x - normal_x) as f32, (extended_start_y - normal_y) as f32, 0.5, 0.5,
			(extended_start_x - normal_x) as f32, (extended_start_y - normal_y) as f32, 0.5, 0.5,
			(extended_end_x + normal_x) as f32, (extended_end_y + normal_y) as f32, 0.5, 0.5,
			(extended_end_x - normal_x) as f32, (extended_end_y - normal_y) as f32, 0.5, 0.5,
		];
		vertex_data[offset..offset + 24].copy_from_slice(&segment_values);
		offset += 24;
		vertex_count += 6;
		segment_index += 1;
	}
	(vertex_count, segment_index)
}

//==============================================================================
/// 정점들이 덮는 영역을 떠 올 영역으로. (원본 measureBackdropRect)
/// - 변환으로 옮긴 경계 상자를 픽셀 경계로 넓히고 화면으로 자른 GL 픽셀(왼쪽 아래 원점) 영역이다. 화면에 걸치지 않으면 None.
//==============================================================================
fn measure_backdrop_rect(vertex_values: &[f32], floats_per_vertex: usize, matrix: TransformMatrix, viewport_width: i32, viewport_height: i32) -> Option<BackdropRect> {
	let mut minimum_x = f64::INFINITY;
	let mut minimum_y = f64::INFINITY;
	let mut maximum_x = f64::NEG_INFINITY;
	let mut maximum_y = f64::NEG_INFINITY;
	for vertex_values in vertex_values.chunks_exact(floats_per_vertex) {
		let position_x = vertex_values[0];
		let position_y = vertex_values[1];
		let device_x = matrix.a * position_x as f64 + matrix.c * position_y as f64 + matrix.e;
		let device_y = matrix.b * position_x as f64 + matrix.d * position_y as f64 + matrix.f;
		minimum_x = minimum_x.min(device_x);
		minimum_y = minimum_y.min(device_y);
		maximum_x = maximum_x.max(device_x);
		maximum_y = maximum_y.max(device_y);
	}
	let left_x = minimum_x.floor().max(0.0);
	let top_y = minimum_y.floor().max(0.0);
	let right_x = maximum_x.ceil().min(viewport_width as f64);
	let bottom_y = maximum_y.ceil().min(viewport_height as f64);
	let is_visible = right_x > left_x && bottom_y > top_y;
	if !is_visible {
		return None;
	}
	Some(BackdropRect {
		x: left_x as i32,
		y: viewport_height - bottom_y as i32,
		width: (right_x - left_x) as i32,
		height: (bottom_y - top_y) as i32,
	})
}

//==============================================================================
/// f64 배열을 바이트 배열로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 시험에 쓰는 프리멀티플라이드 색. (r, g, b, a, 알파를 곱한 값)
	const SAMPLE_COLOR_LIST: [[f64; 4]; 6] = [[0.0, 0.0, 0.0, 0.0], [0.2, 0.1, 0.05, 0.35], [0.5, 0.25, 0.0, 0.5], [0.9, 0.4, 0.1, 1.0], [0.0, 0.6, 0.3, 0.75], [1.0, 1.0, 1.0, 1.0]];

	//==============================================================================
	/// GL 블렌드 계수 값. (채널마다, 알파 채널은 색 계수도 알파 성분을 쓴다)
	//==============================================================================
	fn read_factor_value(factor: u32, source: [f64; 4], destination: [f64; 4], channel: usize) -> f64 {
		match factor {
			glow::ZERO => {
				0.0
			}
			glow::ONE => {
				1.0
			}
			glow::SRC_ALPHA => {
				source[3]
			}
			glow::ONE_MINUS_SRC_ALPHA => {
				1.0 - source[3]
			}
			glow::DST_ALPHA => {
				destination[3]
			}
			glow::ONE_MINUS_DST_ALPHA => {
				1.0 - destination[3]
			}
			glow::DST_COLOR => {
				destination[channel]
			}
			glow::ONE_MINUS_SRC_COLOR => {
				1.0 - source[channel]
			}
			_ => {
				panic!("시험이 모르는 계수 {}", factor);
			}
		}
	}

	//==============================================================================
	/// 고정 블렌딩 한 번. (GL 의 FUNC_ADD, 결과는 0 ~ 1 로 자른다)
	//==============================================================================
	fn blend_with_function(blend_mode: BlendMode, source: [f64; 4], destination: [f64; 4]) -> [f64; 4] {
		let (source_factor, destination_factor) = blend_mode.get_blend_function().expect("고정 블렌딩 모드");
		let mut result = [0.0; 4];
		for channel in 0..4 {
			let value = source[channel] * read_factor_value(source_factor, source, destination, channel) + destination[channel] * read_factor_value(destination_factor, source, destination, channel);
			result[channel] = value.clamp(0.0, 1.0);
		}
		result
	}

	//==============================================================================
	/// W3C 9.1 장의 Fa, Fb. (포터 더프 연산자)
	//==============================================================================
	fn read_porter_duff_fractions(blend_mode: BlendMode, source_alpha: f64, backdrop_alpha: f64) -> (f64, f64) {
		match blend_mode {
			BlendMode::Copy => {
				(1.0, 0.0)
			}
			BlendMode::Normal => {
				(1.0, 1.0 - source_alpha)
			}
			BlendMode::DestinationOver => {
				(1.0 - backdrop_alpha, 1.0)
			}
			BlendMode::SourceIn => {
				(backdrop_alpha, 0.0)
			}
			BlendMode::DestinationIn => {
				(0.0, source_alpha)
			}
			BlendMode::SourceOut => {
				(1.0 - backdrop_alpha, 0.0)
			}
			BlendMode::DestinationOut => {
				(0.0, 1.0 - source_alpha)
			}
			BlendMode::SourceAtop => {
				(backdrop_alpha, 1.0 - source_alpha)
			}
			BlendMode::DestinationAtop => {
				(1.0 - backdrop_alpha, source_alpha)
			}
			BlendMode::Xor => {
				(1.0 - backdrop_alpha, 1.0 - source_alpha)
			}
			BlendMode::Lighter => {
				(1.0, 1.0)
			}
			_ => {
				panic!("포터 더프 연산자가 아님 {:?}", blend_mode);
			}
		}
	}

	//==============================================================================
	/// W3C 6 장 일반식의 source-over 섞기. (co = cs x (1 - αb) + cb x (1 - αs) + αs x αb x B(Cb, Cs), 알파는 source-over)
	//==============================================================================
	fn blend_with_general_formula(source: [f64; 4], backdrop: [f64; 4], blend: impl Fn(f64, f64) -> f64) -> [f64; 4] {
		let source_alpha = source[3];
		let backdrop_alpha = backdrop[3];
		let mut result = [0.0; 4];
		for channel in 0..3 {
			let straight_source = if source_alpha > 0.0 {
				source[channel] / source_alpha
			}
			else {
				0.0
			};
			let straight_backdrop = if backdrop_alpha > 0.0 {
				backdrop[channel] / backdrop_alpha
			}
			else {
				0.0
			};
			result[channel] = source[channel] * (1.0 - backdrop_alpha) + backdrop[channel] * (1.0 - source_alpha) + source_alpha * backdrop_alpha * blend(straight_backdrop, straight_source).clamp(0.0, 1.0);
		}
		result[3] = source_alpha + backdrop_alpha * (1.0 - source_alpha);
		result
	}

	//==============================================================================
	/// 두 색이 거의 같은지.
	//==============================================================================
	fn assert_color_near(actual: [f64; 4], expected: [f64; 4], description: &str) {
		for channel in 0..4 {
			assert!((actual[channel] - expected[channel]).abs() < 1e-5, "{} 채널 {}: {} 이어야 하는데 {}", description, channel, expected[channel], actual[channel]);
		}
	}

	//==============================================================================
	/// 26 가지가 고정 블렌딩 13 가지와 셰이더 섞기 13 가지로 빠짐없이 나뉘고, 셰이더 섞기 번호는 1 ~ 13 이 하나씩인지.
	//==============================================================================
	#[test]
	fn blend_modes_split_into_fixed_and_shader() {
		let mut fixed_count = 0;
		let mut shader_index_list = Vec::new();
		for (blend_mode, blend_mode_name) in BLEND_MODE_NAME_TABLE {
			assert_eq!(BlendMode::from_name(blend_mode_name), Some(blend_mode));
			let is_fixed = blend_mode.get_blend_function().is_some();
			let shader_blend_mode_index = blend_mode.get_shader_blend_mode_index();
			assert!(is_fixed != (shader_blend_mode_index > 0), "{} 는 고정과 셰이더 가운데 하나여야 한다", blend_mode_name);
			if is_fixed {
				fixed_count += 1;
			}
			else {
				shader_index_list.push(shader_blend_mode_index);
			}
		}
		shader_index_list.sort();
		assert_eq!(fixed_count, 13);
		assert_eq!(shader_index_list, (1..=13).collect::<Vec<i32>>());
	}

	//==============================================================================
	/// 포터 더프 연산자의 고정 블렌딩 계수가 W3C 9.1 장 식(co = αs x Fa x Cs + αb x Fb x Cb, αo = αs x Fa + αb x Fb)과 같은지. (바탕이 반투명, 투명이어도)
	//==============================================================================
	#[test]
	fn fixed_blend_factors_match_porter_duff() {
		let porter_duff_mode_list = [BlendMode::Copy, BlendMode::Normal, BlendMode::DestinationOver, BlendMode::SourceIn, BlendMode::DestinationIn, BlendMode::SourceOut, BlendMode::DestinationOut, BlendMode::SourceAtop, BlendMode::DestinationAtop, BlendMode::Xor, BlendMode::Lighter];
		for blend_mode in porter_duff_mode_list {
			for source in SAMPLE_COLOR_LIST {
				for backdrop in SAMPLE_COLOR_LIST {
					let (source_fraction, backdrop_fraction) = read_porter_duff_fractions(blend_mode, source[3], backdrop[3]);
					let mut expected = [0.0; 4];
					for channel in 0..4 {
						expected[channel] = (source[channel] * source_fraction + backdrop[channel] * backdrop_fraction).clamp(0.0, 1.0);
					}
					let actual = blend_with_function(blend_mode, source, backdrop);
					assert_color_near(actual, expected, blend_mode.get_name());
				}
			}
		}
	}

	//==============================================================================
	/// Screen 은 어느 바탕에서나, Multiply 는 불투명한 바탕에서 고정 블렌딩이 W3C 일반식과 같은지.
	/// - Multiply 고정 블렌딩(DST_COLOR, ONE_MINUS_SRC_ALPHA)은 바탕이 반투명하면 cs x (1 - αb) 항이 빠지고 알파가 바탕 알파로 남는다. (기본 화면은 불투명이다)
	///   그래서 렌더 타겟에서는 셰이더로 섞는다. (multiply_shader_blend_index_is_separate, shader_output_with_source_over_matches_general_formula)
	//==============================================================================
	#[test]
	fn multiply_and_screen_factors_match_general_formula() {
		for source in SAMPLE_COLOR_LIST {
			for backdrop in SAMPLE_COLOR_LIST {
				let screen_expected = blend_with_general_formula(source, backdrop, |backdrop_value, source_value| backdrop_value + source_value - backdrop_value * source_value);
				assert_color_near(blend_with_function(BlendMode::Screen, source, backdrop), screen_expected, "screen");
				if backdrop[3] == 1.0 {
					let multiply_expected = blend_with_general_formula(source, backdrop, |backdrop_value, source_value| backdrop_value * source_value);
					assert_color_near(blend_with_function(BlendMode::Multiply, source, backdrop), multiply_expected, "multiply");
				}
			}
		}
	}

	//==============================================================================
	/// blend_equation MIN / MAX 가 반투명 소스에서 W3C darken, lighten 과 다른지. (Darken, Lighten 을 셰이더로 섞는 까닭)
	//==============================================================================
	#[test]
	fn minimum_maximum_equations_differ_from_darken_lighten() {
		let backdrop = [0.2, 0.2, 0.2, 1.0];
		let half_black = [0.0, 0.0, 0.0, 0.5];
		let half_white = [0.5, 0.5, 0.5, 0.5];
		let darken = blend_with_general_formula(half_black, backdrop, |backdrop_value: f64, source_value: f64| backdrop_value.min(source_value));
		let lighten = blend_with_general_formula(half_white, backdrop, |backdrop_value: f64, source_value: f64| backdrop_value.max(source_value));
		assert!((darken[0] - 0.1).abs() < 1e-6);
		assert!((lighten[0] - 0.6).abs() < 1e-6);
		assert!((half_black[0].min(backdrop[0]) - darken[0]).abs() > 0.05);
		assert!((half_white[0].max(backdrop[0]) - lighten[0]).abs() > 0.05);
	}

	//==============================================================================
	/// 셰이더가 내보내는 (Cs' x αs, αs) 를 고정 블렌딩 source-over 로 합성하면 W3C 6 장 일반식과 같은지. (blendWithBackdrop 뒤의 셈)
	//==============================================================================
	#[test]
	fn shader_output_with_source_over_matches_general_formula() {
		let blend_list: [fn(f64, f64) -> f64; 4] = [
			|backdrop_value, source_value| backdrop_value * source_value,
			|backdrop_value, source_value| (backdrop_value - source_value).abs(),
			|backdrop_value, source_value| backdrop_value + source_value - 2.0 * backdrop_value * source_value,
			|backdrop_value, source_value| {
				if source_value <= 0.5 {
					backdrop_value * 2.0 * source_value
				}
				else {
					let screen_value = 2.0 * source_value - 1.0;
					backdrop_value + screen_value - backdrop_value * screen_value
				}
			},
		];
		for blend in blend_list {
			for source in SAMPLE_COLOR_LIST {
				for backdrop in SAMPLE_COLOR_LIST {
					let source_alpha = source[3];
					let backdrop_alpha = backdrop[3];
					let mut shader_output = [0.0, 0.0, 0.0, source_alpha];
					for channel in 0..3 {
						let straight_source = if source_alpha > 0.0 {
							source[channel] / source_alpha
						}
						else {
							0.0
						};
						let straight_backdrop = if backdrop_alpha > 0.0 {
							backdrop[channel] / backdrop_alpha
						}
						else {
							0.0
						};
						let blended_value = (1.0 - backdrop_alpha) * straight_source + backdrop_alpha * blend(straight_backdrop, straight_source).clamp(0.0, 1.0);
						shader_output[channel] = blended_value * source_alpha;
					}
					let actual = blend_with_function(BlendMode::Normal, shader_output, backdrop);
					let expected = blend_with_general_formula(source, backdrop, blend);
					assert_color_near(actual, expected, "shader output");
				}
			}
		}
	}

	//==============================================================================
	/// Multiply 를 렌더 타겟에서 섞는 셰이더 번호는 셰이더 섞기 13 가지와 겹치지 않고, 두 섞기 셰이더 글에 그 번호의 Cb x Cs 가 있는지.
	//==============================================================================
	#[test]
	fn multiply_shader_blend_index_is_separate() {
		for (blend_mode, _) in BLEND_MODE_NAME_TABLE {
			assert_ne!(blend_mode.get_shader_blend_mode_index(), MULTIPLY_SHADER_BLEND_MODE_INDEX);
		}
		assert_eq!(BlendMode::Multiply.get_shader_blend_mode_index(), 0);
		let multiply_branch = format!("if (blendModeIndex == {}) {{\n\t\treturn backdropColor * sourceColor;", MULTIPLY_SHADER_BLEND_MODE_INDEX);
		assert!(BLEND_FRAGMENTSHADER_SOURCE.contains(&multiply_branch));
		assert!(PARTICLE_BLEND_FRAGMENTSHADER_SOURCE.contains(&multiply_branch));
	}

	//==============================================================================
	/// 떠 올 영역이 변환을 받은 경계 상자를 픽셀로 넓히고 화면으로 자른 GL 픽셀 영역인지. (위아래가 뒤집힌다, 화면 밖은 None)
	//==============================================================================
	#[test]
	fn backdrop_rect_covers_transformed_bounds() {
		let quad_values = [10.2, 20.5, 0.0, 0.0, 40.0, 20.5, 1.0, 0.0, 10.2, 50.7, 0.0, 1.0];
		let backdrop_rect = measure_backdrop_rect(&quad_values, FLOATS_PER_VERTEX, TransformMatrix::identity(), 100, 80);
		assert_eq!(backdrop_rect, Some(BackdropRect { x: 10, y: 80 - 51, width: 30, height: 31 }));
		let mut matrix = TransformMatrix::identity();
		matrix.translate(-60.0, 0.0);
		assert_eq!(measure_backdrop_rect(&quad_values, FLOATS_PER_VERTEX, matrix, 100, 80), None);
		matrix.set_identity();
		matrix.translate(80.0, 40.0);
		assert_eq!(measure_backdrop_rect(&quad_values, FLOATS_PER_VERTEX, matrix, 100, 80), Some(BackdropRect { x: 90, y: 0, width: 10, height: 20 }));
		let left_rect = BackdropRect { x: 0, y: 0, width: 10, height: 10 };
		let touching_rect = BackdropRect { x: 10, y: 0, width: 10, height: 10 };
		let overlapping_rect = BackdropRect { x: 9, y: 9, width: 4, height: 4 };
		assert!(!left_rect.is_overlapping(&touching_rect));
		assert!(left_rect.is_overlapping(&overlapping_rect));
		assert_eq!(left_rect.union(&overlapping_rect), BackdropRect { x: 0, y: 0, width: 13, height: 13 });
	}

	//==============================================================================
	/// 폴리라인 정점이 버퍼보다 많아도 선분을 버리지 않고 이어 적는지. (버퍼가 찰 때마다 그 자리에서 끊고 다음 선분부터 잇는다)
	//==============================================================================
	#[test]
	fn polyline_keeps_segments_past_vertex_capacity() {
		// 점 10 개, 열린 선은 선분 9 개. 버퍼는 쿼드 4 개(정점 24 개)만 들어간다.
		let mut positions = Vec::new();
		for position_index in 0..10 {
			positions.push(Vector2::create(position_index as f64 * 10.0, 0.0));
		}
		let segment_count = read_polyline_segment_count(positions.len(), false);
		assert_eq!(segment_count, 9);
		let mut vertex_data = vec![0.0f32; 24 * FLOATS_PER_VERTEX];
		let mut written_vertex_count_list = Vec::new();
		let mut last_end_x = 0.0;
		let mut segment_index = 0;
		while segment_index < segment_count {
			let (vertex_count, next_segment_index) = write_polyline_vertices(&mut vertex_data, &positions, 1.0, false, segment_index);
			assert!(next_segment_index > segment_index);
			written_vertex_count_list.push(vertex_count);
			// 이번에 적은 마지막 쿼드의 끝 x. (쿼드의 다섯째 정점이 끝 쪽 + 노멀)
			last_end_x = vertex_data[(vertex_count - 2) * FLOATS_PER_VERTEX];
			segment_index = next_segment_index;
		}
		assert_eq!(written_vertex_count_list, vec![24, 24, 6]);
		assert_eq!(written_vertex_count_list.iter().sum::<usize>(), segment_count * 6);
		// 마지막 선분은 마지막 점(90, 0)에서 끝난다. (열린 선의 끝은 늘이지 않는다)
		assert_eq!(last_end_x, 90.0);

		// 닫힌 선은 마지막 점과 첫 점을 잇는 선분이 하나 더 있고, 길이 0 인 선분은 건너뛴다.
		let closed_positions = [Vector2::create(0.0, 0.0), Vector2::create(0.0, 0.0), Vector2::create(10.0, 0.0), Vector2::create(10.0, 10.0)];
		let closed_segment_count = read_polyline_segment_count(closed_positions.len(), true);
		assert_eq!(closed_segment_count, 4);
		let (closed_vertex_count, closed_next_segment_index) = write_polyline_vertices(&mut vertex_data, &closed_positions, 1.0, true, 0);
		assert_eq!(closed_vertex_count, 3 * 6);
		assert_eq!(closed_next_segment_index, closed_segment_count);
	}

	//==============================================================================
	// 진짜 GL 시험. (맥 CGL 로 창 없이 짓은 컨텍스트에 16 x 16 칸을 묶고 Graphic 으로 그린다, 원본 batchclipchrome, externalglstatechrome 과 같은 꼴)
	//==============================================================================
	#[cfg(target_os = "macos")]
	mod gl_tests {
		use super::*;

		/// 시험 칸 한 변. (픽셀)
		const TARGET_SIZE: i32 = 16;
		/// 바탕색. (원본 시험의 #406080)
		const BACKGROUND_BYTES: [u8; 3] = [64, 96, 128];
		const FULLSCREEN_VERTEX_SHADER_SOURCE: &str = "#version 300 es\nvoid main() {\n\tvec2 position = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;\n\tgl_Position = vec4(position, 0.0, 1.0);\n}\n";
		const RED_FRAGMENT_SHADER_SOURCE: &str = "#version 300 es\nprecision highp float;\nout vec4 outputColor;\nvoid main() {\n\toutputColor = vec4(1.0, 0.0, 0.0, 1.0);\n}\n";

		/// 창 없는 컨텍스트에 Graphic 을 짓고, 색과 깊이 스텐실이 있는 16 x 16 칸을 묶어 프레임을 연다. (못 지으면 None, 시험을 건너뛴다)
		fn create_test_graphic() -> Option<Graphic> {
			let gl = crate::core::glstatecache::tests::create_surfaceless_context()?;
			let gl = Rc::try_unwrap(gl).ok()?;
			let mut graphic = Graphic::new(gl, false).ok()?;
			unsafe {
				let gl = graphic.get_gl();
				let texture = gl.create_texture().unwrap();
				gl.bind_texture(glow::TEXTURE_2D, Some(texture));
				gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, TARGET_SIZE, TARGET_SIZE, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
				let renderbuffer = gl.create_renderbuffer().unwrap();
				gl.bind_renderbuffer(glow::RENDERBUFFER, Some(renderbuffer));
				gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, TARGET_SIZE, TARGET_SIZE);
				let framebuffer = gl.create_framebuffer().unwrap();
				gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
				gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
				gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(renderbuffer));
				assert_eq!(gl.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE);
			}
			graphic.apply_settings(TARGET_SIZE, TARGET_SIZE);
			Some(graphic)
		}

		/// 2D 좌표(왼쪽 위 원점)의 픽셀 읽기. (RGB)
		fn read_pixel(graphic: &mut Graphic, x: i32, y: i32) -> [u8; 3] {
			graphic.flush_batch();
			let mut pixel = [0u8; 4];
			unsafe {
				crate::core::glstatecache::pixel_store_i32(&graphic.gl, glow::PACK_ALIGNMENT, 1);
				graphic.gl.read_pixels(x, TARGET_SIZE - 1 - y, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixel)));
			}
			[pixel[0], pixel[1], pixel[2]]
		}

		/// 칸 전체를 한 색으로 칠하기.
		fn fill_target(graphic: &mut Graphic, bytes: [u8; 3]) {
			graphic.set_fill_color(Color::create_from_bytes(bytes[0], bytes[1], bytes[2], 255));
			graphic.draw_rect(Rect::create(0.0, 0.0, TARGET_SIZE as f64, TARGET_SIZE as f64));
		}

		/// 축에 맞는 자르기(셰이더 자르기) 안에서 바꿔 끼운 셰이더에 셰이더 섞기(overlay)를 주면, 덧그림 칸을 옮겨 섞을 때 자르기 밖을 덮지 않는지. (원본 eb4338b)
		#[test]
		fn shader_blend_override_keeps_outside_clip() {
			let Some(mut graphic) = create_test_graphic() else {
				eprintln!("graphic: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
				return;
			};
			let vertex_shader_source = "#version 300 es\nin vec2 vertexPosition; in vec2 vertexTextureCoordinate; uniform mat3 projectionMatrix; uniform mat3 modelMatrix; out vec2 textureCoordinate;\nvoid main() { vec3 position = projectionMatrix * modelMatrix * vec3(vertexPosition, 1.0); gl_Position = vec4(position.xy, 0.0, 1.0); textureCoordinate = vertexTextureCoordinate; }";
			let fragment_shader_source = "#version 300 es\nprecision highp float; in vec2 textureCoordinate; uniform sampler2D mainTexture; uniform vec4 mainColor; uniform float globalAlpha; out vec4 outputColor;\nvoid main() { float alpha = texture(mainTexture, textureCoordinate).a * mainColor.a * globalAlpha; outputColor = vec4(vec3(1.0, 0.3, 0.1) * alpha, alpha); }";
			let override_program = Rc::new(ShaderProgram::new(GlContext::from_context(&graphic.gl), vertex_shader_source, fragment_shader_source).unwrap());
			fill_target(&mut graphic, BACKGROUND_BYTES);
			graphic.begin_clip_rect(Rect::create(8.0, 0.0, 8.0, 16.0), 0.0);
			graphic.set_blend_mode(BlendMode::Overlay);
			graphic.set_shader_program_override(Some(override_program));
			graphic.set_fill_color(Color::create_from_bytes(255, 255, 255, 255));
			graphic.draw_rect(Rect::create(0.0, 0.0, 16.0, 16.0));
			graphic.set_shader_program_override(None);
			graphic.set_blend_mode(BlendMode::Normal);
			graphic.end_clip_rect();
			assert_eq!(read_pixel(&mut graphic, 3, 8), BACKGROUND_BYTES, "자르기 밖이 바탕색이 아니다");
			assert_ne!(read_pixel(&mut graphic, 12, 8), BACKGROUND_BYTES, "자르기 안은 섞여야 한다");
		}

		/// 투명도 층을 다시 열 때 지난 층의 깊이가 남지 않는지. (층 안의 깊이를 비우지 않는 바깥 패스, 깊이 0.5 의 사각형을 LESS 로, 원본 4e05c55)
		/// - 원본은 판 끝에 버린 깊이를 무시하는 드라이버를 흉내 낸다. 러스트는 층 칸을 버리지 않고 다시 쓰므로 지난 층의 깊이 0.5 가 그대로 남는 것으로 본다.
		#[test]
		fn opacity_layer_clears_depth() {
			let Some(mut graphic) = create_test_graphic() else {
				eprintln!("graphic: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
				return;
			};
			let (red_program, vertex_array) = {
				let gl = graphic.get_gl();
				let red_program = ShaderProgram::new(gl, FULLSCREEN_VERTEX_SHADER_SOURCE, RED_FRAGMENT_SHADER_SOURCE).unwrap();
				let vertex_array = unsafe { gl.create_vertex_array().unwrap() };
				(red_program, vertex_array)
			};
			fill_target(&mut graphic, BACKGROUND_BYTES);
			for _ in 0..2 {
				fill_target(&mut graphic, BACKGROUND_BYTES);
				graphic.begin_opacity_layer();
				graphic.run_external_pass(false, |gl| unsafe {
					red_program.use_program(gl);
					gl.bind_vertex_array(Some(vertex_array));
					gl.disable(glow::BLEND);
					gl.enable(glow::DEPTH_TEST);
					gl.depth_func(glow::LESS);
					gl.depth_mask(true);
					crate::core::glstatecache::draw_arrays(gl, glow::TRIANGLES, 0, 3);
				});
				graphic.end_opacity_layer();
				assert_eq!(read_pixel(&mut graphic, 8, 8), [255, 0, 0], "투명도 층 안의 깊이를 비우지 않는 3D 패스가 가려졌다");
			}
		}

		/// 읽은 그림 미리 올리기: 제 것과 주인을 모르는 것(Graphic 이 하나뿐일 때)만 올리고, 시간 한도를 넘기면 남은 수를 돌려주는지. (원본 prepareLoadedImages)
		#[test]
		fn prepare_loaded_images_uploads_own_images() {
			let Some(mut graphic) = create_test_graphic() else {
				eprintln!("graphic: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
				return;
			};
			let create_image = || Image::create_from_pixels(2, 2, [255u8, 102, 0, 255].repeat(4));
			let own_image = create_image();
			let unknown_owner_image = create_image();
			let other_owner_image = create_image();
			let empty_image = Image::create_from_pixels(0, 0, Vec::new());
			let owner = graphic.read_image_owner();
			crate::core::decodedimagestore::store(&own_image, Some(owner));
			crate::core::decodedimagestore::store(&unknown_owner_image, None);
			crate::core::decodedimagestore::store(&other_owner_image, Some(owner + 1));
			crate::core::decodedimagestore::store(&empty_image, Some(owner));
			assert!(!graphic.prepare_image(&empty_image), "크기가 0 인 그림은 올리지 않는다");
			// 시간 한도 0 이면 하나만 올리고 남은 수를 돌려준다.
			assert_eq!(graphic.prepare_loaded_images(0.0), 2);
			assert_eq!(graphic.prepare_loaded_images(f64::INFINITY), 0);
			assert_eq!(graphic.prepare_loaded_images(f64::INFINITY), 0);
			let is_uploaded = |graphic: &Graphic, image: &Rc<Image>| graphic.image_texture_cache.find_texture(image, true, false).is_some();
			assert!(is_uploaded(&graphic, &own_image), "제 그림을 올리지 않았다");
			assert!(is_uploaded(&graphic, &unknown_owner_image), "주인을 모르는 그림을 올리지 않았다 (Graphic 하나)");
			assert!(!is_uploaded(&graphic, &other_owner_image), "남의 그림을 올렸다");
			assert_eq!(crate::core::decodedimagestore::get_pending_image_list(owner + 1).len(), 1, "남의 그림은 맡긴 채로 남는다");
		}

		/// 게임이 run_external_pass 밖에서 GlContext 로 상태를 바꿔 두고 돌아와도 다음 2D 그리기가 제 상태를 다시 거는지. (원본 db99208, externalglstatechrome)
		#[test]
		fn draw_after_outside_gl_change_draws_right() {
			let Some(mut graphic) = create_test_graphic() else {
				eprintln!("graphic: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
				return;
			};
			let red_program = ShaderProgram::new(GlContext::from_context(&graphic.gl), FULLSCREEN_VERTEX_SHADER_SOURCE, RED_FRAGMENT_SHADER_SOURCE).unwrap();
			let (other_vertex_array, other_buffer) = unsafe {
				let gl = graphic.get_gl();
				let vertex_array = gl.create_vertex_array().unwrap();
				let buffer = gl.create_buffer().unwrap();
				gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
				gl.buffer_data_size(glow::ARRAY_BUFFER, 4096, glow::DYNAMIC_DRAW);
				(vertex_array, buffer)
			};
			let change_list: Vec<(&str, Box<dyn Fn(&GlContext)>)> = vec![
				("프로그램", Box::new(|gl: &GlContext| red_program.use_program(gl))),
				("섞기 끔", Box::new(|gl: &GlContext| unsafe { gl.disable(glow::BLEND) })),
				("섞기 인자", Box::new(|gl: &GlContext| unsafe { gl.blend_func(glow::ZERO, glow::ONE) })),
				("섞기 식", Box::new(|gl: &GlContext| unsafe { gl.blend_equation(glow::FUNC_REVERSE_SUBTRACT) })),
				("깊이 시험", Box::new(|gl: &GlContext| unsafe {
					gl.enable(glow::DEPTH_TEST);
					gl.depth_func(glow::NEVER);
				})),
				("면 가리기", Box::new(|gl: &GlContext| unsafe {
					gl.enable(glow::CULL_FACE);
					gl.cull_face(glow::FRONT_AND_BACK);
				})),
				("가위", Box::new(|gl: &GlContext| unsafe {
					gl.enable(glow::SCISSOR_TEST);
					gl.scissor(0, 0, 1, 1);
				})),
				("뷰포트", Box::new(|gl: &GlContext| unsafe { gl.viewport(0, 0, 2, 2) })),
				("버텍스 어레이", Box::new(|gl: &GlContext| unsafe { gl.bind_vertex_array(Some(other_vertex_array)) })),
				("정점 버퍼", Box::new(|gl: &GlContext| unsafe { gl.bind_buffer(glow::ARRAY_BUFFER, Some(other_buffer)) })),
				("활성 유닛", Box::new(|gl: &GlContext| unsafe { gl.active_texture(glow::TEXTURE3) })),
			];
			// 묶어 그리기와 낱장 그리기 둘 다. (오른쪽 반을 칠해 낡은 정점(칸 전체)을 그리면 왼쪽도 칠해져 잡힌다)
			let mut failure_list: Vec<String> = Vec::new();
			for is_batching_enabled in [true, false] {
				graphic.set_batching_enabled(is_batching_enabled);
				for (change_index, (change_name, change)) in change_list.iter().enumerate() {
					fill_target(&mut graphic, BACKGROUND_BYTES);
					graphic.flush_batch();
					change(graphic.get_gl());
					let color_bytes = [10 + change_index as u8 * 20, 200, 30];
					graphic.set_fill_color(Color::create_from_bytes(color_bytes[0], color_bytes[1], color_bytes[2], 255));
					graphic.draw_rect(Rect::create(8.0, 0.0, 8.0, 16.0));
					let pixel_list = [read_pixel(&mut graphic, 12, 8), read_pixel(&mut graphic, 15, 0), read_pixel(&mut graphic, 3, 8)];
					if pixel_list[0] != color_bytes || pixel_list[1] != color_bytes || pixel_list[2] != BACKGROUND_BYTES {
						failure_list.push(format!("{}{} {:?}", if is_batching_enabled { "묶음 " } else { "낱장 " }, change_name, pixel_list));
					}
					// 다음 갈래가 이 갈래에 끌리지 않게 바꾼 것을 되돌린다.
					unsafe {
						let gl = graphic.get_gl();
						gl.blend_equation(glow::FUNC_ADD);
						gl.depth_func(glow::LESS);
						gl.cull_face(glow::BACK);
						gl.active_texture(glow::TEXTURE0);
					}
					graphic.restore_render_state();
				}
			}
			assert!(failure_list.is_empty(), "바깥 GL 뒤 2D 그리기가 틀렸다: {:?}", failure_list);
		}
	}
}
