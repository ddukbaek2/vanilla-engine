//==============================================================================
//! 툰 렌더러. (vanilla.js graphics3d/toon/toonrenderer.js 의 ToonRenderer)
//! - MeshBuilder 의 배열이나 read_glb_arrays 로 읽은 모형을 한 색 재질과 바깥선으로 그린다.
//! - 빛: ToonLightSet 하나를 기본으로 든다. 해(방향과 색, 그림자), 하늘색과 땅색의 바탕빛, 채움 빛, 점광과 스포트라이트(TOON_MAXIMUM_POINT_LIGHTS 까지), 림 빛, 하이라이트, 빛 꼴(÷π).
//!   셀 셰이딩은 그늘 램프다. (유니티 툰 셰이더의 바탕, 1 차 그늘, 2 차 그늘과 Step, Feather, 그림자는 그늘 색으로 들어간다, 경계는 fwidth 로 계단 없이, 램프 텍스처도 된다)
//!   해는 램프 색을 통째로, 채움 빛과 점광과 스포트라이트는 램프의 밝기 단계를 받는다. 하이라이트와 림은 단단한 툰 꼴에서 부드러운 꼴까지. 셈은 toonlightset.rs 머리말에 있다.
//!   set_light, set_sun_direction, set_fill_light, set_shade, set_shade_ramp, clear_shade_ramp, set_band_count, set_band_ramp, set_ramp_texture, set_rim_light, set_specular, set_point_lights,
//!   set_point_light_limit, set_light_model 은 기본 묶음을 고친다. 그리기마다 ToonDrawOptions 의 light_set 으로 다른 묶음(create_light_set)을 줄 수 있다.
//! - 노멀: 모형 행렬과 본 행렬의 왼쪽 위 3x3 의 역행렬의 전치로 옮긴다. (늘이고 줄인 모형도 빛이 맞다, 정점 조각이 고친 행렬도 같다)
//! - 재질: 꼭짓점의 색(sRGB), 빛남(바탕색에 곱해 더함), 빛을 받지 않음. 꼭짓점 UV(arrays.uv_data, 2 씩)가 있으면 텍스처를 읽는다.
//! - 그리기 꼴(ToonDrawOptions, 기본값이면 지금까지와 같은 화면):
//!   tint_color [r, g, b, a] sRGB 와 alpha. tint_amount 를 주면(0 ~ 1) 틴트를 곱하지 않고 sRGB 로 섞는다. (맞은 번쩍임, 고장 빨강)
//!   glow_scale 꼭짓점 빛남의 배(기본 1), color_scale 마지막 선형 색의 배(기본 1, 거울에 비친 모습을 어둡게), emissive_color [r, g, b] sRGB 와 emissive_intensity(선형 배, 기본 1, 색 기본 검정).
//!   base_color_texture(바탕색에 곱함, 알파는 불투명도에), emissive_texture(빛남 색에 곱함), uv_transform [u 옮김, v 옮김, u 배, v 배] (KHR_texture_transform 의 offset, scale).
//!   decal_texture 는 바탕색 위에 알파로 덮는다(색 = 텍스처 x decal_color). decal_emission 만큼 그 색으로 빛난다. decal_uv_transform 은 묶음 그림의 한 칸을 고를 때.
//!   alpha_cutoff 는 불투명도가 그보다 작은 화소를 버린다. (0 이면 끔, glTF MASK)
//!   blend_mode: ToonBlendMode::Alpha(기본, 곧은 알파), Premultiplied(미리 곱한 알파), Additive(더하기). Alpha 가 아니면 비치는 것으로 그린다.
//!   soft_edge 는 pow(|dot(N, V)|, soft_edge) 를 불투명도에 곱해 가장자리를 엷게 한다. (0 이면 끔, 프레넬 꼴)
//!   is_mirrored 는 감기를 뒤집어 그린다. None 이면 모형 행렬의 행렬식이 음수일 때 스스로 뒤집는다. 보기 행렬이 거울에 비쳤으면 그것도 더해 뒤집는다.
//!   index_start, index_count 는 몸의 꼭짓점 번호 가운데 그 자리만 그린다. (0 이면 끝까지, GLB 재질 범위, 숨긴 부분) outline_index_start, outline_index_count 는 바깥선 껍데기.
//!   material(ToonMaterial), textures([(유니폼 이름, 텍스처)]), on_before_draw(program, gl, ToonProgramKind) 는 확장 자리다. (toonmaterial.rs 머리말)
//!   light_set(ToonLightSet) 은 그 그리기의 빛 묶음이다. group, is_depth_prepassed 는 ToonDrawQueue 만 읽는다.
//!   receives_shadow (기본 true) 가 false 면 해의 그림자를 보지 않는다. shade_offset 은 빛을 받는 몫에 더해 그늘 경계를 옮긴다. (길티기어 Xrd 의 꼭짓점 색 문턱, 유니티 툰 셰이더의 Shading Grade Map 꼴)
//!   specular_scale, rim_scale (기본 1) 은 그 그리기의 하이라이트와 림 배다. (0 이면 없음, 재질마다의 세기)
//!   matcap_texture 는 보기 공간 노멀로 읽는 매트캡이다. matcap_strength (기본 1), matcap_blend (ToonMatcapBlend::Multiply 기본, Add)
//!   outline_color [r, g, b, a] sRGB 는 그 그리기의 껍데기 바깥선 색 (None 이면 set_outline_color), outline_width_scale (기본 1) 은 지은 굵기의 배다.
//!   outline_offset_z (기본 0) 는 껍데기를 카메라에서 먼 쪽으로 미는 월드 길이다. (몸 앞으로 비치지 않게, 유니티 툰 셰이더의 Offset Z)
//!   angel_ring_texture 는 천사 고리 텍스처다. u 는 보기 공간 노멀의 x, v 는 2 번 UV 의 y 와 보기 노멀의 y 를 angel_ring_offset_v (기본 0.3) 로 섞은 것이다. (유니티 툰 셰이더의 Angel Ring)
//!   angel_ring_color [r, g, b] (선형 배, 기본 흰색), angel_ring_offset_u (0 ~ 0.5, 기본 0), is_angel_ring_alpha (기본 false 면 더하고 true 면 텍스처 알파로 덮는다)
//!   천사 고리 UV 는 카메라 기울임만큼 되돌려 돌린다. (read_camera_roll, 유니티 툰 셰이더의 Camera Rolling Stabilizer, 카메라를 기울여도 고리는 머리에 붙어 있다)
//!   line_id, has_screen_outline, has_crease_line 은 ToonDrawQueue 가 화면 공간 바깥선에 넘기는 값이다. (렌더러는 읽지 않는다)
//! - 바깥선 껍데기(inverted hull): MeshBuilder 가 지은 껍데기의 바탕 자리와 미는 쪽(지은 굵기, 같은 자리의 노멀을 평균한 쪽)을 올리고, 셰이더가 outline_width_scale 만큼 밀어 뒷면만 그린다.
//!   set_outline_distance_scale(기준 거리, 가장 작은 배, 가장 큰 배) 를 주면 카메라까지의 거리 / 기준 거리 를 굵기에 곱해 화면 굵기를 맞춘다. (원근만)
//!   set_outline_fade_distance(가까운 거리, 먼 거리) 를 주면 모형의 자리에서 카메라까지가 먼 거리에 다가갈수록 굵기가 0 이 된다. (smoothstep, 유니티 툰 셰이더의 Nearest, Farthest Distance)
//!   껍데기 색은 빛 묶음의 outline 빛 색 쓰기가 켜져 있으면(기본) 해의 색을 곱한다. (세기가 1 이상이면 세기로 나눈 색, 유니티 툰 셰이더 바깥선과 같다)
//! - 뼈대: write_skin_pose 가 셈한 본 행렬을 draw_mesh 에 넘긴다. (TOON_MAXIMUM_JOINTS 까지, 엔진 SkinnedModelRenderer 와 같은 96)
//! - GPU 인스턴싱: draw_mesh_instanced 로 같은 모형 여럿을 인스턴스마다 행렬과 틴트로 한 번에 그린다. (몸과 바깥선 모두, 그릴 때마다 걸러서 올림)
//!   오래 두는 인스턴스는 create_instance_layout 과 create_instance_buffer 로 영구 버퍼를 짓고 draw_mesh_instance_buffer 로 그린다. (거르지 않음, 머티리얼 정점 조각이 사용자 어트리뷰트를 읽음)
//! - 화면 밖 거르기: 모형의 경계 구를 보기와 투영의 절두체에 대어 밖이면 그리지 않는다. (인스턴스는 하나씩 걸러 남은 것만 올린다)
//! - 그림자: 엔진 ShadowMap 에 begin_shadow_pass 와 draw_mesh 로 드리우고, set_shadow 로 받는다. (3x3 PCF)
//!   점광과 스포트라이트 그림자는 ShadowAtlas 에 begin_shadow_atlas_pass, 칸마다 set_shadow_atlas_tile 과 draw_mesh 로 드리우고, set_local_shadows 로 받는다. (점광은 큐브 여섯 칸, 스포트라이트는 원근 한 칸)
//!   그 빛의 받는 몫에 그림자를 곱해 그늘 램프에 넣는다. (해와 같은 꼴) casts_shadow 가 true 인 점광만 칸을 받는다. (SHADOW_ATLAS_MAXIMUM_LIGHTS 까지)
//!   그림자를 받지 않을 때도 두 그림자 유닛에 1x1 비교 깊이 텍스처를 묶는다. (sampler2DShadow 유닛에 깊이 텍스처가 없으면 맥 드라이버가 알리고, 색 텍스처가 묶여 있으면 WebGL 이 그리기를 거부한다)
//! - HDR: set_hdr_output(true) 면 감마를 걸지 않은 선형 색을 낸다. 실수 렌더 타겟(ToonPostProcess)에 그리고 블룸과 톤매핑으로 마무리한다.
//! - 사용: create_mesh(gl, arrays) → 매 프레임 begin_frame(gl, 보기와 투영) → draw_mesh / draw_mesh_instanced → end_frame(gl) → 다 쓰면 delete_mesh.
//!   begin_frame 이 앞의 그리기 상태(섞기, 깊이, 면 가리기, 프로그램, 버텍스 어레이)를 적어 두고 깊이 시험과 면 가리기를 켜며, end_frame 이 적어 둔 상태로 돌린다.
//!   (엔진 2D 그리기 사이에 끼울 때, 그 전에 Graphic 의 묶음을 내보낸다)
//!   begin_frame_with_matrices(gl, 보기, 투영) 는 두 행렬로 시작한다. (직교 원 행렬 카메라) 카메라의 자리와 보기 돌림(하이라이트, 림, 소프트 엣지, 매트캡)은 보기와 투영에서 읽는다.
//! - 프로그램마다 프레임 값(보기와 투영, 카메라, 그림자, HDR)과 빛 묶음은 바뀔 때만 올린다. 그리기마다 바뀌는 값(모형 행렬, 틴트, 텍스처)만 매번 올린다.
//! - read_program_sources 로 ShaderWarmer 에 넘길 기본 소스를, read_material_program_sources(머티리얼, 인스턴스 꼴) 로 그 프로그램의 소스를 준다. (원본은 한 함수에 선택 인자)
//! - 노드 사각형 안에 그리기: set_screen_rect(x, y, 너비, 높이) 를 주면 begin_frame 이 보기와 투영의 클립 공간을 그 사각형으로 옮기고 가위로 밖을 자른다.
//!   사각형은 begin_frame 때의 뷰포트(보통 드로잉 버퍼 전체) 안의 픽셀이고 왼쪽 위가 원점이다. 화면 밖 거르기는 옮기기 전의 절두체로 한다. end_frame 이 가위를 푼다.
//! - begin_frame_with_camera(Camera3D) 는 카메라의 보기와 투영으로 begin_frame 을 부른다. 2D 표시(Camera3D::project_to_view)와 같은 카메라를 쓴다.
//! - draw_mesh_outline 은 바깥선 껍데기만 그린다. (ToonDrawQueue 가 불투명한 몸을 다 그린 뒤 바깥선을 몰아 그릴 때, 그린 수는 세지 않는다)
//! - ToonDrawOptions 의 casts_shadow 는 ToonDrawQueue::draw_shadows 만 읽는다. is_body_drawn 이 false 면 바깥선만 그린다. (원본은 꼴 객체에 이름으로 단다)
//! - 텍스처: create_texture(gl, 너비, 높이, RGBA 바이트, ToonTextureOptions) 로 짓는다. UV (0, 0) 이 그림의 왼쪽 위다. (glTF 와 같음)
//! - 텍스처 유닛: 점광 그림자 아틀라스 0, 램프 1, 매트캡 2, 해 그림자 3, 바탕색 4, 빛남 5, 데칼 6, 천사 고리 7, 머티리얼 8 ~ 15. 유닛 0 의 묶음은 begin_frame 이 적어 두고 end_frame 이 돌린다.
//! - 2 번 UV: 배열에 uv1_data 가 있으면 vertexUv 가 (UV, 2 번 UV) 넷이다. 셰이더 조각은 ToonSurface.uv1 로 읽는다. (없으면 (0, 1))
//! - 빛 색 쓰기(set_light_color_usage), 그늘 쪽 림(set_shade_rim_light), 빛 세기 막기(set_light_intensity_limited)는 빛 묶음(ToonLightSet)에 둔다.
//==============================================================================
use crate::core::glcontext::GlContext;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::gluniform::GlUniformF64;

use crate::base::rect::Rect;
use crate::graphics3d::camera3d::Camera3D;

use crate::base::matrix4::Matrix4;
use crate::core::graphic::{GlBuffer, GlProgram, GlTexture, GlUniformLocation, GlVertexArray};
use crate::graphics3d::GlFramebuffer;
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::shadowmap::{ShadowAtlas, ShadowAtlasLight, ShadowMap, SHADOW_ATLAS_MAXIMUM_LIGHTS};
use crate::graphics3d::toon::meshbuilder::{ToonMeshArrays, SKIN_FLOAT_COUNT, VERTEX_FLOAT_COUNT};
use crate::graphics3d::toon::toonlightset::{ToonLightColorUsage, ToonLightModel, ToonLightSet, ToonPointLight, ToonShade, TOON_MAXIMUM_BAND_STEPS, TOON_MAXIMUM_POINT_LIGHTS};
use crate::graphics3d::toon::toonmaterial::{ToonBlendMode, ToonBufferUsage, ToonMatcapBlend, ToonInstanceLayout, ToonMaterial, ToonMaterialChunks, TOON_INSTANCE_MATRIX_NAME, TOON_INSTANCE_TINT_NAME};

/// 한 모형이 쓰는 뼈대 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_JOINTS: usize = 96;

/// 인스턴스 하나의 수 개수. (행렬 16, 틴트 4)
/// - 행렬은 아핀 3x4 로 읽는다. 아래 줄 네 칸(원소 3, 7, 11, 15)은 인스턴스 빛남(sRGB r, g, b 와 선형 세기)이다. (Godot MultiMesh, 유니티 BatchRendererGroup 의 3x4 행렬 꼴)
///   Matrix4 원소를 그대로 담으면 아래 줄이 0, 0, 0, 1 이라 빛남이 없다.
pub const TOON_INSTANCE_FLOAT_COUNT: usize = 20;

/// 텍스처 유닛. (점광 그림자 아틀라스 0, 램프, 매트캡, 해 그림자, 바탕색, 빛남, 데칼, 천사 고리, 머티리얼 텍스처는 8 부터 15 까지)
/// - 유닛 0 은 엔진 2D 가 기억하는 유닛이라 begin_frame 이 묶음을 적어 두고 end_frame 이 돌린다.
const LOCAL_SHADOW_TEXTURE_UNIT: u32 = 0;
const RAMP_TEXTURE_UNIT: u32 = 1;
const MATCAP_TEXTURE_UNIT: u32 = 2;
const SHADOW_TEXTURE_UNIT: u32 = 3;
const BASE_COLOR_TEXTURE_UNIT: u32 = 4;
const EMISSIVE_TEXTURE_UNIT: u32 = 5;
const DECAL_TEXTURE_UNIT: u32 = 6;
const ANGEL_RING_TEXTURE_UNIT: u32 = 7;
const FIRST_MATERIAL_TEXTURE_UNIT: u32 = 8;

/// 머티리얼이 그리기마다 묶을 수 있는 텍스처 수의 상한. (유닛 8 ~ 15)
pub const TOON_MAXIMUM_MATERIAL_TEXTURES: usize = 8;

/// 어트리뷰트 자리. (모든 프로그램이 같은 자리를 쓴다, 인스턴스 꼴의 사용자 어트리뷰트는 13 부터)
pub const TOON_ATTRIBUTE_LOCATIONS: [(&str, u32); 13] = [
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
	("vertexUv", 12),
];

/// 자주 쓰는 어트리뷰트 자리.
const INSTANCE_MATRIX_LOCATION: u32 = 7;
const INSTANCE_TINT_LOCATION: u32 = 11;
const UV_LOCATION: u32 = 12;

/// 모형마다 다른 번호를 주는 셈. (인스턴스 버퍼가 모형마다 지은 버텍스 어레이를 가린다)
static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);

//==============================================================================
/// 기억한 유니폼 하나. (원본 readUniformSlot 의 칸, serial 이 렌더러의 판 번호와 같을 때만 values 가 GL 의 값과 같다)
//==============================================================================
struct ToonUniformSlot {
	location: Option<GlUniformLocation>,
	serial: u64,
	values: Vec<f64>,
}

//==============================================================================
/// 같은 수인지. (원본 isSameNumber, 0 과 -0 을 가르고 NaN 은 늘 다르다)
//==============================================================================
fn is_same_number(left: f64, right: f64) -> bool {
	left == right && left.is_sign_negative() == right.is_sign_negative()
}

//==============================================================================
/// 그리기의 프로그램 종류. (on_before_draw 의 셋째 인자)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonProgramKind {
	/// 몸.
	Solid,
	/// 바깥선 껍데기.
	Outline,
	/// 그림자 깊이.
	Depth,
}

/// 세 정점 셰이더의 머리. (몸에만 있는 어트리뷰트도 모두 적는다, 바깥선 버텍스 어레이에 없는 것은 기본값으로 읽힘)
const VERTEX_HEAD_CHUNK: &str = "#version 300 es
in vec3 vertexPosition;
in vec3 vertexNormal;
in vec3 vertexColor;
in vec2 vertexMaterial;
in vec4 vertexDetail;
in vec4 vertexUv;
";

/// 꼭짓점을 본으로 움직이고 인스턴스 행렬을 고르는 셰이더 조각.
/// - instanced: 0 이면 인스턴스 아님, 1 이면 인스턴스 행렬과 틴트, 2 면 인스턴스 틴트만(모형 행렬은 유니폼).
/// - 인스턴스 행렬은 아핀 3x4 로 짓고, 네 열의 w(행렬의 아래 줄)는 인스턴스 빛남으로 읽는다. (TOON_INSTANCE_FLOAT_COUNT 머리말)
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
	if (instanced < 0.5 || instanced > 1.5) {
		return modelMatrix;
	}
	return mat4(vec4(instanceMatrix0.xyz, 0.0), vec4(instanceMatrix1.xyz, 0.0), vec4(instanceMatrix2.xyz, 0.0), vec4(instanceMatrix3.xyz, 1.0));
}
vec4 readInstanceTint() {
	if (instanced < 0.5) {
		return vec4(1.0);
	}
	return instanceTint;
}
vec4 readInstanceEmissive() {
	if (instanced < 0.5 || instanced > 1.5) {
		return vec4(0.0);
	}
	return vec4(instanceMatrix0.w, instanceMatrix1.w, instanceMatrix2.w, instanceMatrix3.w);
}
";

/// 정점 조각이 고칠 꼭짓점. (월드로 옮기기 전)
const VERTEX_STRUCT_CHUNK: &str = "struct ToonVertex {
	vec3 localPosition;
	vec3 localNormal;
	vec3 color;
	vec2 material;
	vec4 detail;
	vec2 uv;
	vec2 uv1;
	vec4 tint;
	vec4 emissive;
	mat4 modelMatrix;
};
ToonVertex readVertex() {
	ToonVertex vertex;
	vertex.localPosition = vertexPosition;
	vertex.localNormal = vertexNormal;
	vertex.color = vertexColor;
	vertex.material = vertexMaterial;
	vertex.detail = vertexDetail;
	vertex.uv = vertexUv.xy;
	vertex.uv1 = vertexUv.zw;
	vertex.tint = readInstanceTint();
	vertex.emissive = readInstanceEmissive();
	vertex.modelMatrix = readModelMatrix();
	return vertex;
}
";

/// 머티리얼에 조각이 없을 때의 빈 함수.
const DEFAULT_VERTEX_CHUNK: &str = "void applyVertex(inout ToonVertex vertex) {
}
";

const DEFAULT_SURFACE_CHUNK: &str = "void applySurface(inout ToonSurface surface) {
}
";

const DEFAULT_LIGHT_CHUNK: &str = "void applyLight(ToonSurface surface, inout ToonLighting lighting) {
}
";

/// 몸을 그리는 정점 셰이더의 몸통.
const SOLID_VERTEX_BODY: &str = "uniform mat4 lightViewProjectionMatrix;
out vec3 worldPosition;
out vec3 localPosition;
out vec3 worldNormal;
out vec3 surfaceColor;
out vec2 surfaceMaterial;
out vec4 surfaceDetail;
out vec2 surfaceUv;
out vec2 surfaceUv1;
out vec4 surfaceTint;
out vec4 surfaceEmissive;
out vec4 lightSpacePosition;
void main() {
	ToonVertex vertex = readVertex();
	applyVertex(vertex);
	mat4 skinMatrix = readSkinMatrix();
	mat4 objectMatrix = vertex.modelMatrix;
	vec4 position = objectMatrix * skinMatrix * vec4(vertex.localPosition, 1.0);
	worldPosition = position.xyz;
	localPosition = vertex.localPosition;
	worldNormal = normalize(transpose(inverse(mat3(objectMatrix) * mat3(skinMatrix))) * vertex.localNormal);
	surfaceColor = vertex.color;
	surfaceMaterial = vertex.material;
	surfaceDetail = vertex.detail;
	surfaceUv = vertex.uv;
	surfaceUv1 = vertex.uv1;
	surfaceTint = vertex.tint;
	surfaceEmissive = vertex.emissive;
	lightSpacePosition = lightViewProjectionMatrix * vec4(position.xyz + worldNormal * 0.02, 1.0);
	gl_Position = viewProjectionMatrix * position;
}
";

/// 몸을 그리는 조각 셰이더의 머리. (유니폼, 표면과 빛 구조, 공용 함수)
const SOLID_FRAGMENT_HEAD: &str = "#version 300 es
precision highp float;
precision highp sampler2DShadow;
in vec3 worldPosition;
in vec3 localPosition;
in vec3 worldNormal;
in vec3 surfaceColor;
in vec2 surfaceMaterial;
in vec4 surfaceDetail;
in vec2 surfaceUv;
in vec2 surfaceUv1;
in vec4 surfaceTint;
in vec4 surfaceEmissive;
in vec4 lightSpacePosition;
uniform vec3 sunDirection;
uniform vec3 sunColor;
uniform vec3 skyColor;
uniform vec3 groundColor;
uniform vec3 fillDirection;
uniform vec3 fillColor;
uniform vec4 rimLight;
uniform vec2 rimShape;
uniform vec3 specularLight;
uniform vec3 specularColor;
uniform float energyNormalized;
uniform float lightIntensityLimit;
uniform int shadeStopCount;
uniform float shadeStopPositions[8];
uniform float shadeStopFeathers[8];
uniform vec3 shadeStopColors[8];
uniform float shadeHalfLambert;
uniform float rampTextureEnabled;
uniform sampler2D rampTexture;
uniform int pointLightCount;
uniform vec4 pointLightPositions[8];
uniform vec3 pointLightColors[8];
uniform vec3 pointLightShapes[8];
uniform vec4 pointLightSpots[8];
uniform vec2 pointLightCones[8];
uniform float pointLightShadowSlots[8];
uniform vec4 localShadowAtlas;
uniform vec4 localShadowPositions[4];
uniform vec4 localShadowForwards[4];
uniform vec4 localShadowUps[4];
uniform float localShadowTiles[4];
uniform sampler2DShadow localShadowTexture;
uniform vec4 lightColorUsage;
uniform vec3 lightColorUsageExtra;
uniform vec4 shadeRimLight;
uniform vec4 cameraPosition;
uniform vec3 cameraForward;
uniform mat3 viewRotation;
uniform vec2 cameraRoll;
uniform vec4 tintColor;
uniform float tintAmount;
uniform float alpha;
uniform float glowScale;
uniform float colorScale;
uniform vec4 emissiveColor;
uniform float softEdge;
uniform float alphaCutoff;
uniform float premultipliedOutput;
uniform float hdrOutput;
uniform vec3 textureEnabled;
uniform sampler2D baseColorTexture;
uniform sampler2D emissiveTexture;
uniform sampler2D decalTexture;
uniform vec4 uvTransform;
uniform vec4 decalUvTransform;
uniform vec4 decalColor;
uniform float receivesShadow;
uniform float shadeOffset;
uniform vec2 highlightScale;
uniform vec3 matcapSettings;
uniform sampler2D matcapTexture;
uniform vec4 angelRingSettings;
uniform vec3 angelRingColor;
uniform sampler2D angelRingTexture;
uniform float shadowEnabled;
uniform float shadowStrength;
uniform float shadowTexelSize;
uniform sampler2DShadow shadowTexture;
out vec4 fragmentColor;
const float RECIPROCAL_PI = 0.3183098862;
struct ToonSurface {
	vec3 baseColor;
	float alpha;
	vec3 emission;
	vec3 normal;
	float glow;
	bool isUnlit;
	vec4 detail;
	vec2 uv;
	vec2 uv1;
	vec3 localPosition;
	vec3 worldPosition;
	vec3 viewDirection;
	float shadeOffset;
	float specularMask;
	float rimMask;
};
struct ToonLighting {
	vec3 irradiance;
	vec3 specular;
	vec3 rim;
	float shadow;
	float sunLevel;
};
vec3 toLinear(vec3 color) {
	return pow(color, vec3(2.2));
}
vec3 toOutput(vec3 color) {
	if (hdrOutput > 0.5) {
		return max(color, vec3(0.0));
	}
	return pow(clamp(color, 0.0, 1.0), vec3(1.0 / 2.2));
}
float readLuminance(vec3 color) {
	return dot(color, vec3(0.2126, 0.7152, 0.0722));
}
float readLightIntensity(vec3 color) {
	return dot(color, vec3(0.299, 0.587, 0.114));
}
vec3 readLimitedDirectionalColor(vec3 color) {
	float energyScale = energyNormalized > 0.5 ? RECIPROCAL_PI : 1.0;
	return lightIntensityLimit > 0.5 ? clamp(color * energyScale, 0.0, 1.0) / energyScale : color;
}
vec3 readLimitedLocalColor(vec3 color) {
	float energyScale = energyNormalized > 0.5 ? RECIPROCAL_PI : 1.0;
	return lightIntensityLimit > 0.5 ? color * min(1.0, 1.0 / max(0.001, readLightIntensity(color * energyScale))) : color;
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
float readLocalShadow(int slot, vec3 position, vec3 normal) {
	vec4 lightPosition = localShadowPositions[slot];
	vec4 forwardFar = localShadowForwards[slot];
	vec4 upTangent = localShadowUps[slot];
	float tile = localShadowTiles[slot];
	vec3 toPoint = position - lightPosition.xyz;
	vec3 forward = forwardFar.xyz;
	vec3 up = upTangent.xyz;
	float tangent = upTangent.w;
	if (tangent <= 0.0) {
		vec3 magnitude = abs(toPoint);
		if (magnitude.x >= magnitude.y && magnitude.x >= magnitude.z) {
			forward = vec3(toPoint.x >= 0.0 ? 1.0 : -1.0, 0.0, 0.0);
			up = vec3(0.0, 1.0, 0.0);
			tile += toPoint.x >= 0.0 ? 0.0 : 1.0;
		}
		else if (magnitude.y >= magnitude.z) {
			forward = vec3(0.0, toPoint.y >= 0.0 ? 1.0 : -1.0, 0.0);
			up = vec3(0.0, 0.0, 1.0);
			tile += toPoint.y >= 0.0 ? 2.0 : 3.0;
		}
		else {
			forward = vec3(0.0, 0.0, toPoint.z >= 0.0 ? 1.0 : -1.0);
			up = vec3(0.0, 1.0, 0.0);
			tile += toPoint.z >= 0.0 ? 4.0 : 5.0;
		}
		tangent = 1.0;
	}
	float near = lightPosition.w;
	float far = forwardFar.w;
	float tileSize = localShadowAtlas.y;
	float texel = localShadowAtlas.z;
	float along = dot(forward, toPoint);
	if (along <= near || along >= far) {
		return 1.0;
	}
	float slope = 1.0 - clamp(dot(normal, -normalize(toPoint)), 0.0, 1.0);
	vec3 offsetPoint = toPoint + normal * (2.0 * along * tangent * texel / tileSize) * (1.0 + 3.0 * slope);
	along = max(dot(forward, offsetPoint), near);
	vec3 right = cross(forward, up);
	vec2 projected = vec2(dot(right, offsetPoint), dot(up, offsetPoint)) / (along * tangent);
	if (abs(projected.x) > 1.0 || abs(projected.y) > 1.0) {
		return 1.0;
	}
	float depth = ((far + near) / (far - near) - 2.0 * far * near / ((far - near) * along)) * 0.5 + 0.5 - 0.0005;
	float grid = localShadowAtlas.x;
	vec2 origin = vec2(mod(tile, grid), floor(tile / grid)) * tileSize;
	vec2 center = origin + (projected * 0.5 + 0.5) * tileSize;
	vec2 low = origin + vec2(texel * 1.5);
	vec2 high = origin + vec2(tileSize - texel * 1.5);
	float lit = 0.0;
	for (int offsetY = -1; offsetY <= 1; ++offsetY) {
		for (int offsetX = -1; offsetX <= 1; ++offsetX) {
			lit += texture(localShadowTexture, vec3(clamp(center + vec2(float(offsetX), float(offsetY)) * texel, low, high), depth));
		}
	}
	return mix(1.0, lit / 9.0, localShadowAtlas.w);
}
vec3 readLightColor(vec3 color, float usage) {
	return mix(vec3(readLightIntensity(color)), color, usage);
}
float readToonCoordinate(float cosine, float offset) {
	return (shadeHalfLambert > 0.5 ? cosine * 0.5 + 0.5 : max(cosine, 0.0)) + offset;
}
vec3 readShadeRamp(float coordinate) {
	if (rampTextureEnabled > 0.5) {
		return toLinear(textureLod(rampTexture, vec2(clamp(coordinate, 0.0, 1.0), 0.5), 0.0).rgb);
	}
	if (shadeStopCount < 2) {
		return vec3(clamp(coordinate, 0.0, 1.0));
	}
	float edgeWidth = max(fwidth(coordinate), 0.0001);
	vec3 color = shadeStopColors[0];
	for (int index = 1; index < 8; ++index) {
		if (index >= shadeStopCount) {
			break;
		}
		float feather = max(shadeStopFeathers[index], edgeWidth);
		color = mix(color, shadeStopColors[index], clamp((coordinate - shadeStopPositions[index] + feather) / feather, 0.0, 1.0));
	}
	return color;
}
float readShadeLevel(vec3 rampColor, float cosine) {
	float bottom;
	float top;
	if (rampTextureEnabled > 0.5) {
		bottom = readLuminance(toLinear(textureLod(rampTexture, vec2(0.0, 0.5), 0.0).rgb));
		top = readLuminance(toLinear(textureLod(rampTexture, vec2(1.0, 0.5), 0.0).rgb));
	}
	else if (shadeStopCount < 2) {
		return clamp(cosine, 0.0, 1.0);
	}
	else {
		bottom = readLuminance(shadeStopColors[0]);
		top = readLuminance(shadeStopColors[shadeStopCount - 1]);
	}
	return clamp((readLuminance(rampColor) - bottom) / max(top - bottom, 0.0001), 0.0, 1.0);
}
float readToonSpecular(vec3 normal, vec3 toLight, vec3 viewDirection) {
	vec3 halfway = normalize(toLight + viewDirection);
	float blinn = pow(max(dot(normal, halfway), 0.0), specularLight.y);
	float edgeWidth = max(fwidth(blinn), 0.0001);
	return mix(smoothstep(0.5 - edgeWidth, 0.5 + edgeWidth, blinn), blinn, specularLight.z);
}
vec3 readViewDirection() {
	if (cameraPosition.w > 0.5) {
		return normalize(cameraPosition.xyz - worldPosition);
	}
	return -cameraForward;
}
";

/// 몸을 그리는 조각 셰이더의 몸통. (표면 → 표면 조각 → 틴트 → 빛 → 빛 조각 → 합치기)
const SOLID_FRAGMENT_BODY: &str = "ToonSurface readSurface() {
	ToonSurface surface;
	vec3 color = surfaceColor * surfaceTint.rgb;
	float opacity = alpha * tintColor.a * surfaceTint.a;
	vec2 uv = surfaceUv * uvTransform.zw + uvTransform.xy;
	if (textureEnabled.x > 0.5) {
		vec4 texel = texture(baseColorTexture, uv);
		color *= texel.rgb;
		opacity *= texel.a;
	}
	vec3 emission = toLinear(emissiveColor.rgb) * emissiveColor.a;
	if (textureEnabled.y > 0.5) {
		emission *= toLinear(texture(emissiveTexture, uv).rgb);
	}
	if (textureEnabled.z > 0.5) {
		vec4 decal = texture(decalTexture, surfaceUv * decalUvTransform.zw + decalUvTransform.xy);
		vec3 decalTint = decal.rgb * decalColor.rgb;
		color = mix(color, decalTint, decal.a);
		emission += toLinear(decalTint) * decal.a * decalColor.a;
	}
	surface.baseColor = color;
	surface.alpha = opacity;
	surface.emission = emission + toLinear(surfaceEmissive.rgb) * surfaceEmissive.a;
	surface.normal = normalize(worldNormal);
	surface.glow = surfaceMaterial.x;
	surface.isUnlit = surfaceMaterial.y > 0.5;
	surface.detail = surfaceDetail;
	surface.uv = surfaceUv;
	surface.uv1 = surfaceUv1;
	surface.localPosition = localPosition;
	surface.worldPosition = worldPosition;
	surface.viewDirection = readViewDirection();
	surface.shadeOffset = shadeOffset;
	surface.specularMask = highlightScale.x;
	surface.rimMask = highlightScale.y;
	return surface;
}
ToonLighting readLighting(ToonSurface surface) {
	ToonLighting lighting;
	vec3 normal = surface.normal;
	vec3 viewDirection = surface.viewDirection;
	float shadow = receivesShadow > 0.5 ? readShadow() : 1.0;
	float specularNormalization = energyNormalized > 0.5 ? (specularLight.y + 8.0) * RECIPROCAL_PI / 8.0 : 1.0;
	bool hasSpecular = specularLight.x > 0.0;
	vec3 specularSum = vec3(0.0);
	vec3 sunLightColor = readLimitedDirectionalColor(sunColor);
	vec3 toSun = -normalize(sunDirection);
	float sunCosine = dot(normal, toSun);
	vec3 sunRamp = readShadeRamp(readToonCoordinate(sunCosine, surface.shadeOffset) * shadow);
	float sunLevel = readShadeLevel(sunRamp, sunCosine * shadow);
	vec3 sunLight = sunRamp * mix(readLightColor(sunLightColor, lightColorUsage.y), readLightColor(sunLightColor, lightColorUsage.x), sunLevel);
	vec3 irradiance = sunLight + mix(groundColor, skyColor, normal.y * 0.5 + 0.5);
	if (hasSpecular) {
		specularSum += readLightColor(sunLightColor, lightColorUsage.z) * readToonSpecular(normal, toSun, viewDirection) * sunLevel;
	}
	vec3 toFill = -normalize(fillDirection);
	float fillCosine = dot(normal, toFill);
	irradiance += readLightColor(readLimitedDirectionalColor(fillColor), lightColorUsage.x) * readShadeLevel(readShadeRamp(readToonCoordinate(fillCosine, surface.shadeOffset)), fillCosine);
	for (int index = 0; index < 8; ++index) {
		if (index >= pointLightCount) {
			break;
		}
		vec3 toLight = pointLightPositions[index].xyz - surface.worldPosition;
		float lightDistance = length(toLight);
		vec3 lightDirection = toLight / max(lightDistance, 0.0001);
		float lightRange = pointLightPositions[index].w;
		vec3 shape = pointLightShapes[index];
		float window = 1.0;
		if (lightRange > 0.0) {
			window = clamp(1.0 - pow(lightDistance / lightRange, shape.y), 0.0, 1.0);
			window *= window;
		}
		float falloff = shape.x > 0.0 ? window / max(pow(lightDistance, shape.x), 0.01) : window;
		vec4 spot = pointLightSpots[index];
		if (spot.w > 0.5) {
			vec2 cone = pointLightCones[index];
			float angular = clamp(dot(spot.xyz, -lightDirection) * cone.x + cone.y, 0.0, 1.0);
			falloff *= angular * angular;
		}
		float lightShadow = 1.0;
		int shadowSlot = int(pointLightShadowSlots[index] + 0.5) - 1;
		if (shadowSlot >= 0 && receivesShadow > 0.5) {
			lightShadow = readLocalShadow(shadowSlot, surface.worldPosition, normal);
		}
		float cosine = (dot(normal, lightDirection) + shape.z) / (1.0 + shape.z);
		float level = readShadeLevel(readShadeRamp(readToonCoordinate(cosine, surface.shadeOffset) * lightShadow), cosine * lightShadow);
		vec3 pointColor = readLimitedLocalColor(pointLightColors[index] * falloff);
		irradiance += readLightColor(pointColor, lightColorUsage.x) * level;
		if (hasSpecular) {
			specularSum += readLightColor(pointColor, lightColorUsage.z) * readToonSpecular(normal, lightDirection, viewDirection) * level;
		}
	}
	lighting.irradiance = irradiance;
	lighting.specular = hasSpecular ? specularSum * specularColor * specularLight.x * specularNormalization : vec3(0.0);
	lighting.rim = vec3(0.0);
	if (rimLight.a > 0.0) {
		float rimTerm = pow(1.0 - clamp(dot(normal, viewDirection), 0.0, 1.0), rimLight.a);
		float edgeWidth = max(fwidth(rimTerm), 0.0001);
		float rim = mix(smoothstep(0.5 - edgeWidth, 0.5 + edgeWidth, rimTerm), rimTerm, rimShape.x);
		lighting.rim = rimLight.rgb * mix(vec3(1.0), sunLightColor, lightColorUsage.w) * rim * mix(1.0, sunLevel, rimShape.y);
	}
	if (shadeRimLight.a > 0.0) {
		float shadeRimTerm = pow(1.0 - clamp(dot(normal, viewDirection), 0.0, 1.0), shadeRimLight.a);
		float edgeWidth = max(fwidth(shadeRimTerm), 0.0001);
		float shadeRim = mix(smoothstep(0.5 - edgeWidth, 0.5 + edgeWidth, shadeRimTerm), shadeRimTerm, rimShape.x);
		lighting.rim += shadeRimLight.rgb * mix(vec3(1.0), sunLightColor, lightColorUsageExtra.x) * clamp(shadeRim - (sunCosine * 0.5 + 0.5), 0.0, 1.0);
	}
	lighting.shadow = shadow;
	lighting.sunLevel = sunLevel;
	return lighting;
}
void main() {
	ToonSurface surface = readSurface();
	applySurface(surface);
	if (alphaCutoff > 0.0 && surface.alpha < alphaCutoff) {
		discard;
	}
	vec3 tintedColor = tintAmount < 0.0 ? surface.baseColor * tintColor.rgb : mix(surface.baseColor, tintColor.rgb, tintAmount);
	vec3 albedo = toLinear(tintedColor);
	vec3 emission = surface.emission + albedo * surface.glow * glowScale;
	vec3 color;
	if (surface.isUnlit) {
		color = albedo + emission;
	}
	else {
		ToonLighting lighting = readLighting(surface);
		applyLight(surface, lighting);
		float diffuseScale = energyNormalized > 0.5 ? RECIPROCAL_PI : 1.0;
		color = albedo * lighting.irradiance * diffuseScale + lighting.specular * surface.specularMask + lighting.rim * surface.rimMask + emission;
	}
	vec3 viewNormal = normalize(viewRotation * surface.normal);
	vec3 sunLightColor = readLimitedDirectionalColor(sunColor);
	if (matcapSettings.x > 0.5) {
		vec3 matcap = toLinear(texture(matcapTexture, vec2(viewNormal.x * 0.5 + 0.5, 0.5 - viewNormal.y * 0.5)).rgb) * mix(vec3(1.0), sunLightColor, lightColorUsageExtra.y);
		color = matcapSettings.z > 0.5 ? color + matcap * matcapSettings.y : color * mix(vec3(1.0), matcap, matcapSettings.y);
	}
	if (angelRingSettings.x > 0.5) {
		vec2 ringOffset = mix(viewNormal, vec3(0.0, 0.0, 1.0), angelRingSettings.y).xy * 0.5;
		vec2 ringNormal = vec2(ringOffset.x * cameraRoll.x + ringOffset.y * cameraRoll.y, ringOffset.y * cameraRoll.x - ringOffset.x * cameraRoll.y) + 0.5;
		vec2 ringUv = vec2(ringNormal.x, 1.0 - mix(1.0 - surface.uv1.y, ringNormal.y, angelRingSettings.z));
		vec4 ring = texture(angelRingTexture, ringUv);
		vec3 ringColor = toLinear(ring.rgb) * angelRingColor * mix(vec3(1.0), sunLightColor, lightColorUsageExtra.z);
		color = angelRingSettings.w > 0.5 ? color * (1.0 - ring.a) + ringColor * ring.a : color + ringColor;
	}
	float opacity = surface.alpha;
	if (softEdge > 0.0) {
		opacity *= pow(abs(dot(surface.normal, surface.viewDirection)), softEdge);
	}
	vec3 outputColor = toOutput(color * colorScale);
	if (premultipliedOutput > 0.5) {
		outputColor *= opacity;
	}
	fragmentColor = vec4(outputColor, opacity);
}
";

/// 바깥선 껍데기를 그리는 셰이더. (껍데기는 뒷면만 그려 몸 둘레에 테로 보인다)
const OUTLINE_VERTEX_BODY: &str = "uniform float outlineWidthScale;
uniform vec3 outlineDistanceScale;
uniform vec2 outlineFadeDistance;
uniform float outlineOffsetZ;
uniform vec4 cameraPosition;
uniform vec3 cameraForward;
out float outlineTint;
void main() {
	ToonVertex vertex = readVertex();
	applyVertex(vertex);
	mat4 objectMatrix = vertex.modelMatrix * readSkinMatrix();
	vec3 basePosition = (objectMatrix * vec4(vertex.localPosition, 1.0)).xyz;
	vec3 offset = mat3(objectMatrix) * vertex.localNormal;
	float scale = outlineWidthScale;
	if (outlineDistanceScale.x > 0.0 && cameraPosition.w > 0.5) {
		scale *= clamp(distance(cameraPosition.xyz, basePosition) / outlineDistanceScale.x, outlineDistanceScale.y, outlineDistanceScale.z);
	}
	if (outlineFadeDistance.y > outlineFadeDistance.x && cameraPosition.w > 0.5) {
		scale *= smoothstep(outlineFadeDistance.y, outlineFadeDistance.x, distance(cameraPosition.xyz, vertex.modelMatrix[3].xyz));
	}
	vec3 position = basePosition + offset * scale;
	vec3 awayFromCamera = cameraPosition.w > 0.5 ? normalize(position - cameraPosition.xyz) : cameraForward;
	outlineTint = vertex.tint.a;
	gl_Position = viewProjectionMatrix * vec4(position + awayFromCamera * outlineOffsetZ, 1.0);
}
";

const OUTLINE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in float outlineTint;
uniform vec4 outlineColor;
uniform vec3 outlineLightColor;
uniform float alpha;
uniform float hdrOutput;
uniform float premultipliedOutput;
out vec4 fragmentColor;
void main() {
	vec3 color = (hdrOutput > 0.5 ? pow(outlineColor.rgb, vec3(2.2)) : outlineColor.rgb) * outlineLightColor;
	float outputAlpha = outlineColor.a * alpha * outlineTint;
	if (premultipliedOutput > 0.5) {
		color *= outputAlpha;
	}
	fragmentColor = vec4(color, outputAlpha);
}
";

/// 그림자 깊이를 그리는 셰이더.
const DEPTH_VERTEX_BODY: &str = "void main() {
	ToonVertex vertex = readVertex();
	applyVertex(vertex);
	gl_Position = viewProjectionMatrix * vertex.modelMatrix * readSkinMatrix() * vec4(vertex.localPosition, 1.0);
}
";

const DEPTH_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
void main() {
}
";

//==============================================================================
/// 세 프로그램의 소스와 어트리뷰트 자리. (compose_toon_program_sources 의 결과)
///
/// # 예제
/// 머티리얼 조각과 사용자 인스턴스 어트리뷰트가 든 소스 짓기
/// ```rust
/// use vanilla::{compose_toon_program_sources, ToonInstanceLayout, ToonMaterial, ToonMaterialChunks};
///
/// let material = ToonMaterial::new(ToonMaterialChunks {
/// 	name: String::from("pulse"),
/// 	uniform_chunk: String::from("uniform float pulse;\n"),
/// 	surface_chunk: String::from("void applySurface(inout ToonSurface surface) {\n\tsurface.emission += vec3(pulse);\n}\n"),
/// 	..ToonMaterialChunks::default()
/// });
/// let instance_layout = ToonInstanceLayout::new(&[("instanceOffset", 3)]).unwrap();
/// let sources = compose_toon_program_sources(Some(&material), Some(&instance_layout));
/// assert!(sources.solid_fragment_shader.contains("uniform float pulse;"));
/// assert!(sources.solid_vertex_shader.contains("in vec3 instanceOffset;"));
/// assert!(sources.attribute_locations.iter().any(|(name, _)| name == "instanceOffset"));
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonProgramSources {
	/// 몸 정점 셰이더.
	pub solid_vertex_shader: String,
	/// 몸 조각 셰이더.
	pub solid_fragment_shader: String,
	/// 바깥선 껍데기 정점 셰이더.
	pub outline_vertex_shader: String,
	/// 바깥선 껍데기 조각 셰이더.
	pub outline_fragment_shader: String,
	/// 그림자 깊이 정점 셰이더.
	pub depth_vertex_shader: String,
	/// 그림자 깊이 조각 셰이더.
	pub depth_fragment_shader: String,
	/// 어트리뷰트 이름과 자리. (TOON_ATTRIBUTE_LOCATIONS 에 인스턴스 꼴의 사용자 어트리뷰트를 더한 것)
	pub attribute_locations: Vec<(String, u32)>,
}

//==============================================================================
/// 머티리얼과 인스턴스 꼴로 세 프로그램의 소스 짓기. (둘 다 None 이면 기본 프로그램)
///
/// # 인자
/// - `material`: 셰이더 조각을 끼울 머티리얼. (None 이면 빈 조각)
/// - `instance_layout`: 사용자 어트리뷰트를 선언할 인스턴스 꼴. (None 이면 없음)
///
/// # 돌려줌
/// 세 프로그램의 소스와 어트리뷰트 자리. (ToonProgramSources)
//==============================================================================
pub fn compose_toon_program_sources(material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> ToonProgramSources {
	let pick = |chunk: Option<&str>, fallback: &'static str| -> String {
		match chunk {
			Some(chunk) if !chunk.is_empty() => {
				String::from(chunk)
			}
			_ => {
				String::from(fallback)
			}
		}
	};
	let vertex_chunk = pick(material.map(|material| material.get_vertex_chunk()), DEFAULT_VERTEX_CHUNK);
	let uniform_chunk = material.map(|material| String::from(material.get_uniform_chunk())).unwrap_or_default();
	let surface_chunk = pick(material.map(|material| material.get_surface_chunk()), DEFAULT_SURFACE_CHUNK);
	let light_chunk = pick(material.map(|material| material.get_light_chunk()), DEFAULT_LIGHT_CHUNK);
	let instance_chunk = instance_layout.map(|instance_layout| instance_layout.read_declaration_source()).unwrap_or_default();
	let vertex_head = format!("{}{}{}{}{}", VERTEX_HEAD_CHUNK, VERTEX_COMMON_CHUNK, instance_chunk, VERTEX_STRUCT_CHUNK, vertex_chunk);
	let mut attribute_locations: Vec<(String, u32)> = TOON_ATTRIBUTE_LOCATIONS.iter().map(|(name, location)| (String::from(*name), *location)).collect();
	if let Some(instance_layout) = instance_layout {
		for attribute in instance_layout.get_attributes() {
			if let Some(location) = attribute.location {
				attribute_locations.push((attribute.name.clone(), location));
			}
		}
	}
	ToonProgramSources {
		solid_vertex_shader: format!("{}{}", vertex_head, SOLID_VERTEX_BODY),
		solid_fragment_shader: format!("{}{}{}{}{}", SOLID_FRAGMENT_HEAD, uniform_chunk, surface_chunk, light_chunk, SOLID_FRAGMENT_BODY),
		outline_vertex_shader: format!("{}{}", vertex_head, OUTLINE_VERTEX_BODY),
		outline_fragment_shader: String::from(OUTLINE_FRAGMENT_SHADER),
		depth_vertex_shader: format!("{}{}", vertex_head, DEPTH_VERTEX_BODY),
		depth_fragment_shader: String::from(DEPTH_FRAGMENT_SHADER),
		attribute_locations,
	}
}

//==============================================================================
/// GPU 에 올린 모형.
///
/// # 예제
/// 부품 둘을 한 모형으로 올리고 다 쓰면 지우기
/// ```rust,no_run
/// use vanilla::{create_box_geometry, create_sphere_geometry, Matrix4, MeshBuilder, ToonPartOptions, ToonRenderer};
///
/// fn upload_mesh(gl: &vanilla::GlContext, renderer: &ToonRenderer) {
/// 	let mut builder = MeshBuilder::new();
/// 	builder.add_part(&create_box_geometry(1.0, 1.0, 1.0), &Matrix4::create_identity(), "#d06060", ToonPartOptions::default());
/// 	builder.add_part(&create_sphere_geometry(0.4, 16, 12, std::f64::consts::PI), &Matrix4::create_translation(0.0, 0.9, 0.0), "#f0c8a0", ToonPartOptions::default());
/// 	if let Some(mesh) = renderer.create_mesh(gl, &builder.read_arrays()) {
/// 		let (center, radius) = mesh.get_bounding_sphere();
/// 		let (body_index_count, outline_index_count) = mesh.get_index_counts();
/// 		println!("{:?} {} {} {} {}", center, radius, body_index_count, outline_index_count, mesh.has_outline());
/// 		renderer.delete_mesh(gl, mesh);
/// 	}
/// }
/// ```
//==============================================================================
pub struct ToonMesh {
	mesh_id: u64,
	body_vertex_array: GlVertexArray,
	body_index_count: i32,
	outline_vertex_array: Option<GlVertexArray>,
	outline_index_count: i32,
	instance_buffer: GlBuffer,
	buffer_list: Vec<GlBuffer>,
	is_skinned: bool,
	has_uv: bool,
	has_second_uv: bool,
	bounding_center: [f64; 3],
	bounding_radius: f64,
	vertex_buffer: GlBuffer,
	skin_buffer: Option<GlBuffer>,
	uv_buffer: Option<GlBuffer>,
	body_index_buffer: GlBuffer,
	outline_buffer: Option<GlBuffer>,
	outline_skin_buffer: Option<GlBuffer>,
	outline_index_buffer: Option<GlBuffer>,
}

impl ToonMesh {
	//==============================================================================
	/// 뼈대로 움직이는 모형인지.
	///
	/// # 돌려줌
	/// 뼈대(skin_data)가 있으면 true.
	//==============================================================================
	pub fn is_skinned(&self) -> bool {
		self.is_skinned
	}

	//==============================================================================
	/// 텍스처 좌표가 있는지.
	///
	/// # 돌려줌
	/// 꼭짓점마다 UV 가 있으면 true.
	//==============================================================================
	pub fn has_uv(&self) -> bool {
		self.has_uv
	}

	//==============================================================================
	/// 경계 구. (모형 좌표의 가운데와 반지름, 뼈대 모형은 쉬는 자세의 1.5 배)
	///
	/// # 돌려줌
	/// (모형 좌표의 가운데 [x, y, z], 반지름)
	//==============================================================================
	pub fn get_bounding_sphere(&self) -> ([f64; 3], f64) {
		(self.bounding_center, self.bounding_radius)
	}

	//==============================================================================
	/// 몸과 바깥선 껍데기의 꼭짓점 번호 수.
	///
	/// # 돌려줌
	/// (몸의 꼭짓점 번호 수, 바깥선 껍데기의 꼭짓점 번호 수)
	//==============================================================================
	pub fn get_index_counts(&self) -> (usize, usize) {
		(self.body_index_count as usize, self.outline_index_count as usize)
	}

	//==============================================================================
	/// 바깥선 껍데기가 있는지.
	///
	/// # 돌려줌
	/// 바깥선 껍데기가 있으면 true.
	//==============================================================================
	pub fn has_outline(&self) -> bool {
		self.outline_vertex_array.is_some()
	}

	//==============================================================================
	/// 몸의 버텍스 어레이와 번호 수. (화면 공간 바깥선이 같은 모형을 한 번 더 그릴 때)
	//==============================================================================
	pub(crate) fn get_body(&self) -> (GlVertexArray, i32) {
		(self.body_vertex_array, self.body_index_count)
	}

	//==============================================================================
	/// 인스턴스 버퍼. (행렬 16, 틴트 4 씩)
	//==============================================================================
	pub(crate) fn get_instance_buffer(&self) -> GlBuffer {
		self.instance_buffer
	}
}

//==============================================================================
/// 영구 인스턴스 버퍼. (create_instance_buffer 의 결과, 모형마다 함께 묶은 버텍스 어레이를 처음 그릴 때 지어 둔다)
///
/// # 예제
/// 나무 아홉 그루의 행렬과 틴트를 한 번 올리기
/// ```rust,no_run
/// use vanilla::{Matrix4, ToonBufferUsage, ToonDrawOptions, ToonMesh, ToonRenderer};
///
/// fn draw_trees(gl: &vanilla::GlContext, renderer: &mut ToonRenderer, mesh: &ToonMesh, view_projection_matrix: &Matrix4) -> Result<(), String> {
/// 	let instance_layout = renderer.create_instance_layout(&[("instanceMatrix", 16), ("instanceTint", 4)])?;
/// 	let mut data: Vec<f32> = Vec::new();
/// 	for index in 0..9 {
/// 		let model_matrix = Matrix4::create_translation((index % 3) as f64 - 1.0, 0.0, (index / 3) as f64 - 1.0);
/// 		data.extend_from_slice(model_matrix.get_elements());
/// 		data.extend_from_slice(&[1.0, 1.0 - index as f32 * 0.05, 1.0, 1.0]);
/// 	}
/// 	let instance_buffer = renderer.create_instance_buffer(gl, &instance_layout, &data, ToonBufferUsage::Static)?;
/// 	renderer.begin_frame(gl, view_projection_matrix);
/// 	renderer.draw_mesh_instance_buffer(gl, mesh, &instance_buffer, instance_buffer.get_instance_count(), ToonDrawOptions::default());
/// 	renderer.end_frame(gl);
/// 	renderer.delete_instance_buffer(gl, instance_buffer);
/// 	Ok(())
/// }
/// ```
//==============================================================================
pub struct ToonInstanceBuffer {
	layout: ToonInstanceLayout,
	buffer: GlBuffer,
	usage: u32,
	float_count: Cell<usize>,
	vertex_array_list: RefCell<Vec<(u64, GlVertexArray, Option<GlVertexArray>)>>,
}

impl ToonInstanceBuffer {
	//==============================================================================
	/// 인스턴스 꼴 반환.
	///
	/// # 돌려줌
	/// 버퍼의 인스턴스 꼴.
	//==============================================================================
	pub fn get_layout(&self) -> &ToonInstanceLayout {
		&self.layout
	}

	//==============================================================================
	/// 담은 수 개수.
	///
	/// # 돌려줌
	/// 담은 수 개수.
	//==============================================================================
	pub fn get_float_count(&self) -> usize {
		self.float_count.get()
	}

	//==============================================================================
	/// 담은 인스턴스 수. (수 개수를 꼴의 수 개수로 나눈 것)
	///
	/// # 돌려줌
	/// 담은 인스턴스 수.
	//==============================================================================
	pub fn get_instance_count(&self) -> usize {
		self.float_count.get() / self.layout.get_float_count().max(1)
	}
}

//==============================================================================
/// 텍스처를 지을 때의 꼴. (밉맵, 되풀이, 고르게 읽기)
///
/// # 예제
/// 2x2 매트캡을 밉맵 없이 가장자리 붙임으로 짓기
/// ```rust,no_run
/// use vanilla::core::graphic::GlTexture;
/// use vanilla::{ToonRenderer, ToonTextureOptions};
///
/// fn create_matcap_texture(gl: &vanilla::GlContext, renderer: &ToonRenderer) -> Result<GlTexture, String> {
/// 	let pixels = [60, 60, 70, 255, 255, 255, 255, 255, 60, 60, 70, 255, 160, 160, 180, 255];
/// 	renderer.create_texture(gl, 2, 2, &pixels, ToonTextureOptions { is_mipmapped: false, is_repeated: false, ..ToonTextureOptions::default() })
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToonTextureOptions {
	/// 밉맵을 지을지. (기본 true)
	pub is_mipmapped: bool,
	/// 되풀이할지. (기본 true, false 면 가장자리 붙임)
	pub is_repeated: bool,
	/// 고르게 읽을지. (기본 true, false 면 가까운 화소)
	pub is_filtered: bool,
}

impl Default for ToonTextureOptions {
	//==============================================================================
	/// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonTextureOptions { is_mipmapped: true, is_repeated: true, is_filtered: true }
	}
}

//==============================================================================
/// 그리기 전에 부르는 콜백. (프로그램, GL, 프로그램 종류, 머티리얼의 유니폼을 쓸 때)
//==============================================================================
pub type ToonBeforeDrawCallback<'a> = &'a dyn Fn(&ShaderProgram, &GlContext, ToonProgramKind);

//==============================================================================
/// 모형 하나를 그릴 때의 꼴.
///
/// # 예제
/// 틴트, 바깥선 색과 굵기, 반투명을 그리기마다 바꾸기
/// ```rust
/// use vanilla::{ToonBlendMode, ToonDrawOptions};
///
/// let options = ToonDrawOptions {
/// 	tint_color: [1.0, 0.6, 0.6, 1.0],
/// 	alpha: 0.5,
/// 	outline_color: Some([0.1, 0.06, 0.08, 1.0]),
/// 	outline_width_scale: 1.5,
/// 	..ToonDrawOptions::default()
/// };
/// assert!(options.has_outline && options.is_culled);
/// assert_eq!(options.blend_mode, ToonBlendMode::Alpha);
/// ```
//==============================================================================
#[derive(Clone, Copy)]
pub struct ToonDrawOptions<'a> {
	/// 틴트 색. ([r, g, b, a] sRGB, 기본 흰색, tint_amount 가 없으면 바탕색에 곱한다, 알파는 불투명도에 곱한다)
	pub tint_color: [f64; 4],
	/// 불투명도. (기본 1, 1 보다 작으면 비치는 것으로 그린다)
	pub alpha: f64,
	/// 본 행렬의 수. (write_skin_pose 가 셈한 것, 본마다 16, None 이면 뼈대로 움직이지 않는다)
	pub joint_matrices: Option<&'a [f32]>,
	/// 바깥선 껍데기를 그릴지. (기본 true)
	pub has_outline: bool,
	/// 화면 밖이면 거를지. (기본 true)
	pub is_culled: bool,
	/// 그림자를 드리울지. (기본 true, ToonDrawQueue::draw_shadows 만 읽는다)
	pub casts_shadow: bool,
	/// 몸을 그릴지. (기본 true, false 면 바깥선만 그린다)
	pub is_body_drawn: bool,
	/// 틴트를 섞는 몫. (0 ~ 1, 주면 틴트를 곱하지 않고 sRGB 로 섞는다, 맞은 번쩍임, 고장 빨강)
	pub tint_amount: Option<f64>,
	/// 꼭짓점 빛남의 배. (기본 1)
	pub glow_scale: f64,
	/// 마지막 선형 색의 배. (기본 1, 거울에 비친 모습을 어둡게)
	pub color_scale: f64,
	/// 빛남 색. ([r, g, b] sRGB, 기본 검정)
	pub emissive_color: [f64; 3],
	/// 빛남 색의 세기. (선형 배, 기본 1)
	pub emissive_intensity: f64,
	/// 바탕색 텍스처. (바탕색에 곱함, 알파는 불투명도에, None 이면 없음)
	pub base_color_texture: Option<GlTexture>,
	/// 빛남 텍스처. (빛남 색에 곱함, None 이면 없음)
	pub emissive_texture: Option<GlTexture>,
	/// UV 옮김과 배. ([u 옮김, v 옮김, u 배, v 배], KHR_texture_transform 의 offset, scale)
	pub uv_transform: [f64; 4],
	/// 데칼 텍스처. (바탕색 위에 알파로 덮는다, 색 = 텍스처 x decal_color, None 이면 없음)
	pub decal_texture: Option<GlTexture>,
	/// 데칼 색. (텍스처에 곱한다, 기본 흰색)
	pub decal_color: [f64; 3],
	/// 데칼이 그 색으로 빛나는 몫. (기본 0)
	pub decal_emission: f64,
	/// 데칼 UV 옮김과 배. ([u 옮김, v 옮김, u 배, v 배], 묶음 그림의 한 칸을 고를 때)
	pub decal_uv_transform: [f64; 4],
	/// 불투명도가 그보다 작은 화소를 버린다. (0 이면 끔, glTF MASK)
	pub alpha_cutoff: f64,
	/// 섞는 방식. (기본 ToonBlendMode::Alpha, Alpha 가 아니면 비치는 것으로 그린다)
	pub blend_mode: ToonBlendMode,
	/// 가장자리를 엷게 하는 지수. (pow(|dot(N, V)|, soft_edge) 를 불투명도에 곱한다, 0 이면 끔, 프레넬 꼴)
	pub soft_edge: f64,
	/// 감기를 뒤집어 그릴지. (None 이면 모형 행렬의 행렬식이 음수일 때 스스로 뒤집는다, 보기 행렬이 거울에 비쳤으면 그것도 더해 뒤집는다)
	pub is_mirrored: Option<bool>,
	/// 몸의 꼭짓점 번호 가운데 그릴 첫 자리. (GLB 재질 범위, 숨긴 부분)
	pub index_start: usize,
	/// 몸의 꼭짓점 번호 가운데 그릴 수. (0 이면 끝까지)
	pub index_count: usize,
	/// 바깥선 껍데기의 꼭짓점 번호 가운데 그릴 첫 자리.
	pub outline_index_start: usize,
	/// 바깥선 껍데기의 꼭짓점 번호 가운데 그릴 수. (0 이면 끝까지)
	pub outline_index_count: usize,
	/// 셰이더 조각을 끼울 머티리얼. (None 이면 기본 프로그램, toonmaterial.rs 머리말)
	pub material: Option<&'a ToonMaterial>,
	/// 머티리얼 텍스처. ([(유니폼 이름, 텍스처)], 유닛 8 부터 TOON_MAXIMUM_MATERIAL_TEXTURES 개까지)
	pub textures: &'a [(&'a str, GlTexture)],
	/// 그리기 전에 부르는 콜백. (프로그램, GL, 프로그램 종류, 머티리얼의 유니폼을 쓸 때)
	pub on_before_draw: Option<ToonBeforeDrawCallback<'a>>,
	/// 그 그리기의 빛 묶음. (None 이면 렌더러의 기본 묶음)
	pub light_set: Option<&'a ToonLightSet>,
	/// 비치는 것의 묶음 번호. (같은 번호는 한 덩어리로 정렬한다, ToonDrawQueue 만 읽는다)
	pub group: Option<u32>,
	/// 묶음의 깊이만 먼저 그릴지. (ToonDrawQueue 만 읽는다)
	pub is_depth_prepassed: bool,
	/// 그림자를 받을지. (false 면 해의 그림자를 보지 않는다)
	pub receives_shadow: bool,
	/// 빛을 받는 몫 x 에 더할 값. (그늘 경계를 옮긴다, 길티기어 Xrd 의 꼭짓점 색 문턱, 유니티 툰 셰이더의 Shading Grade Map 꼴, 표면 조각이 surface.shadeOffset 으로 더 고칠 수 있다)
	pub shade_offset: f64,
	/// 하이라이트 배. (0 이면 이 그리기는 하이라이트가 없다, 재질마다의 세기)
	pub specular_scale: f64,
	/// 림 빛 배. (0 이면 이 그리기는 림 빛이 없다)
	pub rim_scale: f64,
	/// 매트캡 텍스처. (보기 공간 노멀로 읽는 공 그림, None 이면 없음)
	pub matcap_texture: Option<GlTexture>,
	/// 매트캡 세기.
	pub matcap_strength: f64,
	/// 매트캡 섞기. (곱하기나 더하기)
	pub matcap_blend: ToonMatcapBlend,
	/// 껍데기 바깥선 색. (sRGB 와 알파, None 이면 렌더러의 set_outline_color)
	pub outline_color: Option<[f64; 4]>,
	/// 껍데기 바깥선 굵기 배. (지은 굵기에 곱한다)
	pub outline_width_scale: f64,
	/// 껍데기 바깥선을 카메라에서 먼 쪽으로 밀 거리. (월드 길이, 몸 앞으로 비치지 않게, 유니티 툰 셰이더의 Offset Z 꼴)
	pub outline_offset_z: f64,
	/// 천사 고리 텍스처. (보기 공간 노멀의 x 와 2 번 UV 의 y 로 읽는 머리카락 반사 띠, None 이면 없음, 유니티 툰 셰이더의 Angel Ring)
	pub angel_ring_texture: Option<GlTexture>,
	/// 천사 고리 색. (선형 배)
	pub angel_ring_color: [f64; 3],
	/// 천사 고리 u 를 화면 가운데 쪽으로 당기는 몫. (0 ~ 0.5, AR_OffsetU)
	pub angel_ring_offset_u: f64,
	/// 천사 고리 v 를 2 번 UV 에서 보기 노멀 쪽으로 옮기는 몫. (0 ~ 1, AR_OffsetV, 기본 0.3)
	pub angel_ring_offset_v: f64,
	/// 천사 고리를 텍스처 알파로 덮을지. (false 면 더한다, ARSampler_AlphaOn)
	pub is_angel_ring_alpha: bool,
	/// 화면 공간 바깥선의 선 번호. (ToonDrawQueue 가 선의 판에 그릴 때, None 이면 담은 차례 + 1)
	pub line_id: Option<u32>,
	/// 화면 공간 바깥선을 낼지. (false 면 선은 내지 않고 뒤의 선만 가린다)
	pub has_screen_outline: bool,
	/// 화면 공간 꺾임 선을 낼지.
	pub has_crease_line: bool,
}

impl Default for ToonDrawOptions<'_> {
	//==============================================================================
	/// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonDrawOptions {
			tint_color: [1.0; 4],
			alpha: 1.0,
			joint_matrices: None,
			has_outline: true,
			is_culled: true,
			casts_shadow: true,
			is_body_drawn: true,
			tint_amount: None,
			glow_scale: 1.0,
			color_scale: 1.0,
			emissive_color: [0.0; 3],
			emissive_intensity: 1.0,
			base_color_texture: None,
			emissive_texture: None,
			uv_transform: [0.0, 0.0, 1.0, 1.0],
			decal_texture: None,
			decal_color: [1.0; 3],
			decal_emission: 0.0,
			decal_uv_transform: [0.0, 0.0, 1.0, 1.0],
			alpha_cutoff: 0.0,
			blend_mode: ToonBlendMode::Alpha,
			soft_edge: 0.0,
			is_mirrored: None,
			index_start: 0,
			index_count: 0,
			outline_index_start: 0,
			outline_index_count: 0,
			material: None,
			textures: &[],
			on_before_draw: None,
			light_set: None,
			group: None,
			is_depth_prepassed: false,
			receives_shadow: true,
			shade_offset: 0.0,
			specular_scale: 1.0,
			rim_scale: 1.0,
			matcap_texture: None,
			matcap_strength: 1.0,
			matcap_blend: ToonMatcapBlend::Multiply,
			outline_color: None,
			outline_width_scale: 1.0,
			outline_offset_z: 0.0,
			angel_ring_texture: None,
			angel_ring_color: [1.0; 3],
			angel_ring_offset_u: 0.0,
			angel_ring_offset_v: 0.3,
			is_angel_ring_alpha: false,
			line_id: None,
			has_screen_outline: true,
			has_crease_line: true,
		}
	}
}

//==============================================================================
/// 꼴 글로 보이기. (콜백은 있는지만)
//==============================================================================
impl std::fmt::Debug for ToonDrawOptions<'_> {
	//==============================================================================
	/// 디버그 출력. (GL 객체는 이름만)
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("ToonDrawOptions")
			.field("tint_color", &self.tint_color)
			.field("alpha", &self.alpha)
			.field("has_joint_matrices", &self.joint_matrices.is_some())
			.field("has_outline", &self.has_outline)
			.field("is_culled", &self.is_culled)
			.field("casts_shadow", &self.casts_shadow)
			.field("is_body_drawn", &self.is_body_drawn)
			.field("tint_amount", &self.tint_amount)
			.field("glow_scale", &self.glow_scale)
			.field("color_scale", &self.color_scale)
			.field("emissive_color", &self.emissive_color)
			.field("emissive_intensity", &self.emissive_intensity)
			.field("blend_mode", &self.blend_mode)
			.field("soft_edge", &self.soft_edge)
			.field("is_mirrored", &self.is_mirrored)
			.field("index_start", &self.index_start)
			.field("index_count", &self.index_count)
			.field("material", &self.material.map(|material| material.get_id()))
			.field("texture_count", &self.textures.len())
			.field("has_on_before_draw", &self.on_before_draw.is_some())
			.field("light_set", &self.light_set.map(|light_set| light_set.get_id()))
			.field("group", &self.group)
			.field("is_depth_prepassed", &self.is_depth_prepassed)
			.finish()
	}
}

//==============================================================================
/// 인스턴스 하나. (모형 행렬과 틴트, 틴트의 알파는 바깥선에도 곱한다)
/// - emissive_color 는 그 인스턴스에 더하는 빛남 sRGB(None 이면 없음, 명암과 무늬 위에 더해 그 인스턴스만 밝힌다), emissive_intensity 는 빛남의 선형 배(원본 기본 1)다.
///   모형 행렬은 아핀으로 읽는다. (TOON_INSTANCE_FLOAT_COUNT 머리말, 빛남은 행렬의 아래 줄에 싣는다)
/// - line_id 는 ToonScreenOutline::draw_mesh_instanced 가 읽는 이 인스턴스의 선 번호다. (원본 lineId, None 이면 공통 번호, ToonRenderer 는 읽지 않는다)
///
/// # 예제
/// 같은 바위 스무 개를 인스턴스마다 다른 틴트로
/// ```rust,no_run
/// use vanilla::{Matrix4, ToonDrawOptions, ToonInstance, ToonMesh, ToonRenderer};
///
/// fn draw_rocks(gl: &vanilla::GlContext, renderer: &mut ToonRenderer, mesh: &ToonMesh, view_projection_matrix: &Matrix4) {
/// 	let instance_list: Vec<ToonInstance> = (0..20).map(|index| {
/// 		let shade = 0.6 + (index % 5) as f64 * 0.1;
/// 		ToonInstance {
/// 			model_matrix: Matrix4::create_translation((index % 5) as f64 * 1.5, 0.0, (index / 5) as f64 * 1.5),
/// 			tint_color: [shade, shade, shade, 1.0],
/// 			..ToonInstance::default()
/// 		}
/// 	}).collect();
/// 	renderer.begin_frame(gl, view_projection_matrix);
/// 	renderer.draw_mesh_instanced(gl, mesh, &instance_list, ToonDrawOptions::default());
/// 	renderer.end_frame(gl);
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonInstance {
	/// 모형 행렬.
	pub model_matrix: Matrix4,
	/// 틴트 색. ([r, g, b, a] sRGB, 알파는 바깥선에도 곱한다)
	pub tint_color: [f64; 4],
	/// 화면 공간 바깥선의 선 번호. (None 이면 공통 번호, ToonRenderer 는 읽지 않는다)
	pub line_id: Option<u32>,
	/// 더하는 빛남 색. ([r, g, b] sRGB, None 이면 빛남 없음)
	pub emissive_color: Option<[f64; 3]>,
	/// 빛남의 선형 배. (원본 기본 1, emissive_color 가 None 이면 읽지 않는다)
	pub emissive_intensity: f64,
}

impl Default for ToonInstance {
	//==============================================================================
	/// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonInstance { model_matrix: Matrix4::create_identity(), tint_color: [1.0, 1.0, 1.0, 1.0], line_id: None, emissive_color: None, emissive_intensity: 1.0 }
	}
}

//==============================================================================
/// 받을 그림자. (깊이 텍스처, 빛의 보기와 투영, 해상도, 세기 0 ~ 1)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ToonShadowInput {
	depth_texture: GlTexture,
	light_view_projection_matrix: Matrix4,
	resolution: i32,
	strength: f64,
}

//==============================================================================
/// 그린 수와 거른 수. (한 프레임, begin_frame 이 비운다)
///
/// # 예제
/// 한 프레임을 그린 뒤 드로우콜과 거른 수 찍기
/// ```rust,no_run
/// use vanilla::{ToonFrameStatistics, ToonRenderer};
///
/// fn print_statistics(renderer: &ToonRenderer) {
/// 	let statistics: ToonFrameStatistics = renderer.get_frame_statistics();
/// 	println!("draw calls {}, drawn {}, culled {}", statistics.draw_call_count, statistics.drawn_object_count, statistics.culled_object_count);
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct ToonFrameStatistics {
	/// 드로우콜 수.
	pub draw_call_count: u32,
	/// 그린 모형 수. (인스턴스는 하나씩 센다)
	pub drawn_object_count: u32,
	/// 화면 밖이라 거른 모형 수. (인스턴스는 하나씩 센다)
	pub culled_object_count: u32,
}

//==============================================================================
/// 보기와 투영에서 읽은 카메라. (position 은 원근이면 [x, y, z, 1], 직교면 [0, 0, 0, 0], forward 는 보는 방향, is_mirrored 는 보기 행렬이 거울에 비쳤는지)
///
/// # 예제
/// 보기와 투영 행렬에서 눈의 자리와 보는 방향 읽기
/// ```rust
/// use vanilla::{read_frame_camera, Matrix4, Vector3};
///
/// let mut view_projection_matrix = Matrix4::create_perspective(0.8, 16.0 / 9.0, 0.1, 100.0);
/// view_projection_matrix.multiply(&Matrix4::create_look_at(Vector3::create(0.0, 2.0, 5.0), Vector3::create(0.0, 2.0, 0.0), Vector3::create(0.0, 1.0, 0.0)));
/// let frame_camera = read_frame_camera(&view_projection_matrix);
/// assert_eq!(frame_camera.position[3], 1.0);
/// assert!((frame_camera.position[2] - 5.0).abs() < 0.001);
/// assert!(frame_camera.forward[2] < -0.999);
/// assert!(!frame_camera.is_mirrored);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonFrameCamera {
	/// 눈의 자리. ([x, y, z, 1] 원근, [0, 0, 0, 0] 직교)
	pub position: [f64; 4],
	/// 보는 방향. ([x, y, z], 단위)
	pub forward: [f64; 3],
	/// 보기 행렬이 거울에 비쳤는지.
	pub is_mirrored: bool,
}

//==============================================================================
/// begin_frame 앞의 그리기 상태.
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
	program: Option<GlProgram>,
	vertex_array: Option<GlVertexArray>,
	viewport: [i32; 4],
	framebuffer: Option<GlFramebuffer>,
	/// 고른 텍스처 유닛.
	active_texture: i32,
	/// 유닛 0 에 묶인 텍스처. (엔진 2D 가 기억하는 유닛)
	first_unit_texture: Option<GlTexture>,
}

//==============================================================================
/// 받을 점광과 스포트라이트 그림자. (ShadowAtlas 에서 읽은 값)
//==============================================================================
struct ToonLocalShadowInput {
	depth_texture: GlTexture,
	light_list: Vec<ShadowAtlasLight>,
	grid_size: f64,
	tile_size: f64,
	texel_size: f64,
	strength: f64,
}

//==============================================================================
/// 프로그램 하나와 마지막으로 올린 프레임 값, 빛 묶음. (Cell 이라 빌린 채 고친다)
//==============================================================================
struct ToonProgramEntry {
	program: ShaderProgram,
	kind: ToonProgramKind,
	frame_serial: Cell<u64>,
	light_set_id: Cell<u32>,
	light_version: Cell<u64>,
}

impl ToonProgramEntry {
	//==============================================================================
	/// 프로그램 칸 짓기. (올린 프레임 번호와 빛 묶음은 비운 채로)
	//==============================================================================
	fn new(program: ShaderProgram, kind: ToonProgramKind) -> ToonProgramEntry {
		ToonProgramEntry { program, kind, frame_serial: Cell::new(0), light_set_id: Cell::new(0), light_version: Cell::new(u64::MAX) }
	}
}

//==============================================================================
/// 머티리얼과 인스턴스 꼴 하나의 세 프로그램.
//==============================================================================
struct ToonProgramSet {
	solid: ToonProgramEntry,
	outline: ToonProgramEntry,
	depth: ToonProgramEntry,
}

//==============================================================================
/// 툰 렌더러. (머리말 참고)
///
/// # 예제
/// 해와 바깥선 색을 정해 그린 뒤 통계를 읽고 치우기
/// ```rust,no_run
/// use vanilla::{create_capsule_geometry, Matrix4, MeshBuilder, ToonDrawOptions, ToonPartOptions, ToonRenderer, Vector3};
///
/// fn draw_once(gl: &vanilla::GlContext) -> Result<(), String> {
/// 	let mut renderer = ToonRenderer::new(gl)?;
/// 	renderer.set_light([-0.4, -1.0, -0.3], [1.0, 0.96, 0.9], [0.45, 0.5, 0.6], [0.25, 0.22, 0.2]);
/// 	renderer.set_outline_color([0.1, 0.06, 0.08, 1.0]);
/// 	let mut builder = MeshBuilder::new();
/// 	builder.add_part(&create_capsule_geometry(0.3, 0.6, 8, 16), &Matrix4::create_identity(), "#a0d0a0", ToonPartOptions { outline_width: 0.02, ..ToonPartOptions::default() });
/// 	let mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("empty mesh")?;
/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 1.0, 3.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	let projection_matrix = Matrix4::create_perspective(0.8, 1.0, 0.1, 20.0);
/// 	renderer.begin_frame_with_matrices(gl, &view_matrix, &projection_matrix);
/// 	renderer.draw_mesh(gl, &mesh, &Matrix4::create_identity(), ToonDrawOptions::default());
/// 	renderer.draw_mesh(gl, &mesh, &Matrix4::create_translation(0.0, 0.0, 50.0), ToonDrawOptions::default());
/// 	renderer.end_frame(gl);
/// 	let statistics = renderer.get_frame_statistics();
/// 	assert_eq!((statistics.drawn_object_count, statistics.culled_object_count), (1, 1));
/// 	renderer.delete_mesh(gl, mesh);
/// 	renderer.destroy(gl);
/// 	Ok(())
/// }
/// ```
//==============================================================================
pub struct ToonRenderer {
	/// (머티리얼 번호, 인스턴스 꼴 번호) → 세 프로그램. (짓지 못했으면 None, 다시 짓지 않는다)
	program_set_table: HashMap<(u32, u32), Option<ToonProgramSet>>,
	/// 그림자를 받지 않을 때 그림자 유닛에 묶는 1x1 비교 깊이 텍스처. (깊이 1 이라 늘 빛을 받음)
	empty_shadow_texture: GlTexture,
	/// 텍스처를 주지 않은 바탕색, 빛남, 데칼 유닛에 묶는 1x1 흰 텍스처.
	white_texture: GlTexture,
	view_projection_matrix: Matrix4,
	frustum_planes: [[f64; 4]; 6],
	camera_position: [f64; 4],
	camera_forward: [f64; 3],
	is_frame_mirrored: bool,
	light_set: ToonLightSet,
	outline_color: [f64; 4],
	is_hdr_output: bool,
	shadow_input: Option<ToonShadowInput>,
	is_shadow_pass: bool,
	/// 프레임 값이 바뀔 때마다 오르는 번호. (프로그램마다 마지막으로 올린 번호와 견준다)
	frame_serial: u64,
	saved_state: Option<SavedState>,
	instance_data: Vec<f32>,
	frame_statistics: ToonFrameStatistics,
	/// 프로그램마다 유니폼 이름별 마지막에 올린 값. (원본 uniformSlotTable, 같은 값이면 GL 에 다시 올리지 않는다)
	uniform_slot_table: RefCell<HashMap<GlProgram, HashMap<String, ToonUniformSlot>>>,
	/// 유니폼 기억의 판 번호. (원본 uniformSerial, 올리면 모든 프로그램의 기억을 잊는다)
	uniform_serial: Cell<u64>,
	screen_rect: Option<Rect>,
	is_scissor_applied: bool,
	/// 이번 프레임의 보기 돌림 3x3. (열 차례, 매트캡의 보기 공간 노멀)
	view_rotation: [f32; 9],
	/// 이번 프레임의 카메라 기울임을 되돌리는 각. (라디안, 천사 고리)
	camera_roll: f64,
	/// 껍데기 바깥선의 거리 배. ([기준 거리 (0 이면 끔), 가장 작은 배, 가장 큰 배])
	outline_distance_scale: [f64; 3],
	/// 껍데기 바깥선이 사라지는 거리. ([가까운 거리, 먼 거리], 먼 거리가 가까운 거리 이하면 끔)
	outline_fade_distance: [f64; 2],
	/// 받을 점광과 스포트라이트 그림자.
	local_shadow_input: Option<ToonLocalShadowInput>,
}

//==============================================================================
/// f64, u32 배열을 바이트로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
/// u32 배열을 바이트로 본다. (번호 버퍼 업로드용)
//==============================================================================
fn index_slice_to_bytes(values: &[u32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
/// 보기와 투영 행렬에서 절두체의 면 여섯. (ax + by + cz + d >= 0 이 안쪽, 정규화)
///
/// # 인자
/// - `matrix`: 보기와 투영 행렬. (투영 x 보기)
///
/// # 돌려줌
/// 면 여섯의 [a, b, c, d]. (왼쪽, 오른쪽, 아래, 위, 가까운 면, 먼 면)
//==============================================================================
pub fn extract_frustum_planes(matrix: &Matrix4) -> [[f64; 4]; 6] {
	let elements = matrix.read_elements();
	let row = |row_index: usize| [elements[row_index], elements[4 + row_index], elements[8 + row_index], elements[12 + row_index]];
	let row0 = row(0);
	let row1 = row(1);
	let row2 = row(2);
	let row3 = row(3);
	let combine = |first: [f64; 4], second: [f64; 4], sign: f64| [first[0] + second[0] * sign, first[1] + second[1] * sign, first[2] + second[2] * sign, first[3] + second[3] * sign];
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
/// 세 벡터를 행으로 둔 3x3 의 행렬식.
//==============================================================================
fn read_determinant(first: [f64; 3], second: [f64; 3], third: [f64; 3]) -> f64 {
	first[0] * (second[1] * third[2] - second[2] * third[1]) - first[1] * (second[0] * third[2] - second[2] * third[0]) + first[2] * (second[0] * third[1] - second[1] * third[0])
}

//==============================================================================
/// 모형 행렬 왼쪽 위 3x3 의 행렬식. (음수면 거울에 비친 행렬, 감기가 뒤집힌다)
///
/// # 인자
/// - `matrix`: 모형 행렬.
///
/// # 돌려줌
/// 왼쪽 위 3x3 의 행렬식. (음수면 거울에 비친 행렬)
//==============================================================================
pub fn read_matrix_determinant(matrix: &Matrix4) -> f64 {
	let elements = matrix.read_elements();
	read_determinant([(elements[0]), (elements[1]), (elements[2])], [(elements[4]), (elements[5]), (elements[6])], [(elements[8]), (elements[9]), (elements[10])])
}

//==============================================================================
/// 보기와 투영 행렬에서 카메라 읽기. (원근이면 눈의 자리, 직교면 보는 방향, 행렬이 거울에 비친 것인지)
/// - 원근: 클립 x, y, w 가 모두 0 이 되는 점이 눈이다. (행 0, 1, 3 의 연립 방정식)
/// - 직교: 행 3 의 xyz 가 0 이라 눈이 무한히 멀다. 보는 방향은 행 0, 1 의 외적을 깊이가 느는 쪽으로 둔다.
/// - 보통의 투영(가까운 면 < 먼 면)은 왼쪽 위 3x3 의 행렬식이 음수다. 양수면 보기 행렬이 거울에 비친 것이다.
///
/// # 인자
/// - `matrix`: 보기와 투영 행렬.
///
/// # 돌려줌
/// 읽은 카메라. (position [x, y, z, w(1 원근, 0 직교)], forward [x, y, z], is_mirrored)
//==============================================================================
pub fn read_frame_camera(matrix: &Matrix4) -> ToonFrameCamera {
	let elements = matrix.read_elements();
	let row = |row_index: usize| [elements[row_index], elements[4 + row_index], elements[8 + row_index], elements[12 + row_index]];
	let row0 = row(0);
	let row1 = row(1);
	let row2 = row(2);
	let row3 = row(3);
	let xyz = |value: [f64; 4]| [value[0], value[1], value[2]];
	let is_mirrored = read_determinant(xyz(row0), xyz(row1), xyz(row2)) > 0.0;
	let mut forward = [row0[1] * row1[2] - row0[2] * row1[1], row0[2] * row1[0] - row0[0] * row1[2], row0[0] * row1[1] - row0[1] * row1[0]];
	let forward_length = (forward[0] * forward[0] + forward[1] * forward[1] + forward[2] * forward[2]).sqrt();
	if forward_length > 0.0 {
		forward = [forward[0] / forward_length, forward[1] / forward_length, forward[2] / forward_length];
	}
	if forward[0] * row2[0] + forward[1] * row2[1] + forward[2] * row2[2] < 0.0 {
		forward = [-forward[0], -forward[1], -forward[2]];
	}
	let determinant = read_determinant(xyz(row0), xyz(row1), xyz(row3));
	if determinant.abs() < 1e-12 {
		return ToonFrameCamera { position: [0.0; 4], forward, is_mirrored };
	}
	let constant = [-row0[3], -row1[3], -row3[3]];
	let position_x = read_determinant([(constant[0]), (row0[1]), (row0[2])], [(constant[1]), (row1[1]), (row1[2])], [(constant[2]), (row3[1]), (row3[2])]) / determinant;
	let position_y = read_determinant([(row0[0]), (constant[0]), (row0[2])], [(row1[0]), (constant[1]), (row1[2])], [(row3[0]), (constant[2]), (row3[2])]) / determinant;
	let position_z = read_determinant([(row0[0]), (row0[1]), (constant[0])], [(row1[0]), (row1[1]), (constant[1])], [(row3[0]), (row3[1]), (constant[2])]) / determinant;
	ToonFrameCamera { position: [position_x, position_y, position_z, 1.0], forward, is_mirrored }
}

//==============================================================================
/// 보기와 투영 행렬에서 보기 돌림 3x3 읽기. (열 차례, 행이 오른쪽, 위, 뒤, 매트캡의 보기 공간 노멀을 셀 때)
/// - 오른쪽은 행 0 의 xyz 에서 보는 방향 몫을 뺀 것이다. (대칭 원근과 직교, 화면 사각형으로 옮긴 투영 모두)
///
/// # 인자
/// - `view_projection_matrix`: 보기와 투영 행렬.
/// - `forward`: 보는 방향. (read_frame_camera 의 forward)
///
/// # 돌려줌
/// mat3 의 수 9. (열 차례)
//==============================================================================
pub fn read_view_rotation(view_projection_matrix: &Matrix4, forward: [f64; 3]) -> [f64; 9] {
	let elements = view_projection_matrix.read_elements();
	let row_x = [elements[0], elements[4], elements[8]];
	let along = row_x[0] * forward[0] + row_x[1] * forward[1] + row_x[2] * forward[2];
	let mut right = [row_x[0] - forward[0] * along, row_x[1] - forward[1] * along, row_x[2] - forward[2] * along];
	let right_length = crate::base::trigonometry::hypot_list(&right);
	right = if right_length > 0.000001 { [right[0] / right_length, right[1] / right_length, right[2] / right_length] } else { [1.0, 0.0, 0.0] };
	let up = [right[1] * forward[2] - right[2] * forward[1], right[2] * forward[0] - right[0] * forward[2], right[0] * forward[1] - right[1] * forward[0]];
	[right[0], up[0], -forward[0], right[1], up[1], -forward[1], right[2], up[2], -forward[2]]
}

//==============================================================================
/// 카메라 기울임을 되돌리는 각 읽기. (유니티 툰 셰이더의 천사 고리 Camera Rolling Stabilizer 와 같은 셈, 원본 readCameraRoll)
/// - 카메라 오른쪽(보기 돌림의 행 0)과 수평 오른쪽 축이 이루는 각 roll, 오른쪽의 y 가 음수면 쪽 dir 이 -1, 돌려주는 각은 -(dir x roll) 이다.
/// - 유니티는 왼손 좌표라 수평 오른쪽 축이 cross(뒤, 위) 이고, 엔진은 오른손 좌표라 같은 쪽이 cross(앞, 위) 다. 거울에 비친 프레임이면 축을 뒤집는다. (유니티 툰 셰이더의 mirrorFlag)
/// - 바로 위나 아래를 보면 수평 축이 없어 0 이다.
///
/// # 인자
/// - `view_rotation`: mat3 의 수 9. (read_view_rotation 의 결과, 열 차례)
/// - `is_mirrored`: 보기 행렬이 거울에 비쳤는지. (read_frame_camera 의 is_mirrored)
///
/// # 돌려줌
/// 천사 고리 UV 를 (0.5, 0.5) 둘레로 돌릴 각. (라디안)
//==============================================================================
pub fn read_camera_roll(view_rotation: &[f32; 9], is_mirrored: bool) -> f64 {
	let right = [view_rotation[0], view_rotation[3], view_rotation[6]];
	let forward = [-view_rotation[2], -view_rotation[5], -view_rotation[8]];
	let axis_sign = if is_mirrored { -1.0 } else { 1.0 };
	let right_axis = [-forward[2] * axis_sign, 0.0, forward[0] * axis_sign];
	let axis_length = (right_axis[0] * right_axis[0] + right_axis[1] * right_axis[1] + right_axis[2] * right_axis[2]).sqrt();
	let right_length = (right[0] * right[0] + right[1] * right[1] + right[2] * right[2]).sqrt();
	if axis_length < 0.000001 || right_length < 0.000001 {
		return 0.0;
	}
	let cosine = (right_axis[0] * right[0] + right_axis[1] * right[1] + right_axis[2] * right[2]) / (axis_length * right_length);
	let roll = cosine.clamp(-1.0, 1.0).acos();
	let direction = if right[1] < 0.0 { -1.0 } else { 1.0 };
	(-(direction * roll)) as f64
}

//==============================================================================
/// 껍데기 바깥선에 곱할 해의 색 읽기. (유니티 툰 셰이더 바깥선과 같이 세기가 1 이상이면 세기로 나눈다, 세기는 0.299 R + 0.587 G + 0.114 B)
//==============================================================================
fn read_outline_light_color(sun_color: [f64; 3]) -> [f64; 3] {
	let intensity = 0.299 * sun_color[0] + 0.587 * sun_color[1] + 0.114 * sun_color[2];
	if intensity < 1.0 {
		sun_color
	}
	else {
		[sun_color[0] / intensity, sun_color[1] / intensity, sun_color[2] / intensity]
	}
}

//==============================================================================
/// UV 꼭짓점 배열 짓기. (2 번 UV 가 없으면 점마다 UV 2, 있으면 UV 와 2 번 UV 를 섞어 4, 셰이더의 vertexUv 는 vec4 라 2 개만 올리면 zw 는 (0, 1))
///
/// # 인자
/// - `uv_data`: UV. (점마다 2)
/// - `uv1_data`: 2 번 UV. (점마다 2, 없으면 None)
/// - `vertex_count`: 점 수.
///
/// # 돌려줌
/// 점마다 2 나 4.
//==============================================================================
pub fn read_uv_data(uv_data: &[f32], uv1_data: Option<&[f32]>, vertex_count: usize) -> Vec<f32> {
	match uv1_data {
		None => {
			uv_data[..vertex_count * 2].to_vec()
		}
		Some(uv1_data) => {
			let mut data = vec![0.0f32; vertex_count * 4];
			for vertex_index in 0..vertex_count {
				data[vertex_index * 4] = uv_data[vertex_index * 2];
				data[vertex_index * 4 + 1] = uv_data[vertex_index * 2 + 1];
				data[vertex_index * 4 + 2] = uv1_data[vertex_index * 2];
				data[vertex_index * 4 + 3] = uv1_data[vertex_index * 2 + 1];
			}
			data
		}
	}
}

//==============================================================================
/// 껍데기 꼭짓점 배열 짓기. (점마다 바탕 자리 3 과 미는 쪽 3, 미는 쪽은 사본 자리 - 바탕 자리)
/// - 바탕 자리가 없거나 길이가 다르면(GLB 에 지어 둔 껍데기) 사본 자리를 바탕으로 두고 미는 쪽은 0 이다. (굵기 배를 걸지 않는다)
///
/// # 인자
/// - `outline_vertex_data`: 지은 굵기로 민 사본 자리. (3 씩)
/// - `outline_base_data`: 밀기 전 바탕 자리. (3 씩, 길이가 다르면 사본 자리를 바탕으로 둔다)
///
/// # 돌려줌
/// 점마다 6 개.
//==============================================================================
pub fn read_outline_vertex_data(outline_vertex_data: &[f32], outline_base_data: &[f32]) -> Vec<f32> {
	let point_count = outline_vertex_data.len() / 3;
	let has_base = outline_base_data.len() == outline_vertex_data.len();
	let mut data = vec![0.0f32; point_count * 6];
	for point_index in 0..point_count {
		for axis in 0..3 {
			let outline_value = outline_vertex_data[point_index * 3 + axis];
			let base_value = if has_base { outline_base_data[point_index * 3 + axis] } else { outline_value };
			data[point_index * 6 + axis] = base_value;
			data[point_index * 6 + 3 + axis] = outline_value - base_value;
		}
	}
	data
}

//==============================================================================
/// 스포트라이트 원뿔의 각 감쇠 배와 덧셈. (glTF KHR_lights_punctual, 각 감쇠는 clamp(cos 각 x 배 + 덧셈, 0, 1) 의 제곱)
//==============================================================================
fn read_spot_cone(inner_cone_angle: f64, outer_cone_angle: f64) -> (f64, f64) {
	let outer_cosine = outer_cone_angle.cos();
	let cone_scale = 1.0 / (inner_cone_angle.min(outer_cone_angle).cos() - outer_cosine).max(0.001);
	(cone_scale, -outer_cosine * cone_scale)
}

//==============================================================================
/// 평면에 비친 행렬 짓기. (평면 위 한 점과 노멀, 평면 거울의 모형이나 보기 행렬에 곱한다)
/// - 비친 행렬로 그리면 감기가 뒤집힌다. ToonRenderer 는 모형 행렬의 행렬식이 음수면 스스로 앞면을 뒤집는다. (is_mirrored 로 정할 수도 있다)
///
/// # 인자
/// - `plane_point`: 평면 위 한 점. ([x, y, z])
/// - `plane_normal`: 평면의 노멀. ([x, y, z], 길이는 상관없다)
///
/// # 돌려줌
/// 평면에 비친 행렬.
//==============================================================================
pub fn create_reflection_matrix(plane_point: [f64; 3], plane_normal: [f64; 3]) -> Matrix4 {
	let length = (plane_normal[0] * plane_normal[0] + plane_normal[1] * plane_normal[1] + plane_normal[2] * plane_normal[2]).sqrt();
	let (normal_x, normal_y, normal_z) = if length > 0.0 { (plane_normal[0] / length, plane_normal[1] / length, plane_normal[2] / length) } else { (0.0, 1.0, 0.0) };
	let distance = normal_x * plane_point[0] + normal_y * plane_point[1] + normal_z * plane_point[2];
	let mut matrix = Matrix4::create_identity();
	let elements = &mut matrix.elements;
	elements[0] = (1.0 - 2.0 * normal_x * normal_x) as f32;
	elements[1] = (-2.0 * normal_x * normal_y) as f32;
	elements[2] = (-2.0 * normal_x * normal_z) as f32;
	elements[4] = (-2.0 * normal_y * normal_x) as f32;
	elements[5] = (1.0 - 2.0 * normal_y * normal_y) as f32;
	elements[6] = (-2.0 * normal_y * normal_z) as f32;
	elements[8] = (-2.0 * normal_z * normal_x) as f32;
	elements[9] = (-2.0 * normal_z * normal_y) as f32;
	elements[10] = (1.0 - 2.0 * normal_z * normal_z) as f32;
	elements[12] = (2.0 * distance * normal_x) as f32;
	elements[13] = (2.0 * distance * normal_y) as f32;
	elements[14] = (2.0 * distance * normal_z) as f32;
	matrix
}

//==============================================================================
/// 행렬의 가장 큰 크기. (세 축 길이 가운데 큰 것)
//==============================================================================
fn read_maximum_scale(matrix: &Matrix4) -> f64 {
	let elements = matrix.read_elements();
	let scale_x = (elements[0] * elements[0] + elements[1] * elements[1] + elements[2] * elements[2]).sqrt();
	let scale_y = (elements[4] * elements[4] + elements[5] * elements[5] + elements[6] * elements[6]).sqrt();
	let scale_z = (elements[8] * elements[8] + elements[9] * elements[9] + elements[10] * elements[10]).sqrt();
	scale_x.max(scale_y).max(scale_z)
}

//==============================================================================
/// 꼭짓점 배열의 경계 구. (자리 셋씩, 가운데는 상자의 가운데)
//==============================================================================
fn read_bounding_sphere(positions: &[f32], stride: usize) -> ([f64; 3], f64) {
	let mut minimum = [f64::MAX; 3];
	let mut maximum = [f64::MIN; 3];
	for point in positions.chunks(stride) {
		if point.len() < 3 {
			continue;
		}
		for axis in 0..3 {
			minimum[axis] = minimum[axis].min(point[axis] as f64);
			maximum[axis] = maximum[axis].max(point[axis] as f64);
		}
	}
	if minimum[0] > maximum[0] {
		return ([0.0; 3], 0.0);
	}
	let center = [(minimum[0] + maximum[0]) * 0.5, (minimum[1] + maximum[1]) * 0.5, (minimum[2] + maximum[2]) * 0.5];
	// 원본처럼 double 로 빼고 Math.hypot 으로 잰다.
	let mut radius: f64 = 0.0;
	for point in positions.chunks(stride) {
		if point.len() < 3 {
			continue;
		}
		let delta = [point[0] as f64 - center[0], point[1] as f64 - center[1], point[2] as f64 - center[2]];
		radius = radius.max(crate::base::trigonometry::hypot_list(&delta));
	}
	(center, radius)
}

//==============================================================================
/// RGBA 텍스처 올리기. (create_texture 와 흰 텍스처가 함께 쓴다)
//==============================================================================
fn upload_texture(gl: &crate::GlContext, width: u32, height: u32, pixels: &[u8], options: ToonTextureOptions) -> Result<GlTexture, String> {
	let byte_count = width as usize * height as usize * 4;
	if width == 0 || height == 0 || pixels.len() < byte_count {
		return Err(format!("텍스처 바이트가 모자란다: {} x {} 에 {} 바이트", width, height, pixels.len()));
	}
	unsafe {
		let texture = crate::core::glstatecache::create_texture(gl)?;
		crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + BASE_COLOR_TEXTURE_UNIT);
		crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(texture));
		gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA as i32, width as i32, height as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&pixels[..byte_count])));
		let wrap = if options.is_repeated { glow::REPEAT } else { glow::CLAMP_TO_EDGE };
		let magnify_filter = if options.is_filtered { glow::LINEAR } else { glow::NEAREST };
		let mut minify_filter = magnify_filter;
		if options.is_mipmapped {
			gl.generate_mipmap(glow::TEXTURE_2D);
			minify_filter = if options.is_filtered { glow::LINEAR_MIPMAP_LINEAR } else { glow::NEAREST_MIPMAP_NEAREST };
		}
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, minify_filter as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, magnify_filter as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap as i32);
		crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
		crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
		Ok(texture)
	}
}

impl ToonRenderer {
	//==============================================================================
	/// 셰이더 소스. ((정점, 조각) 세 벌: 몸, 바깥선, 그림자 깊이, 기본 프로그램)
	///
	/// # 돌려줌
	/// (정점, 조각) 소스 세 벌. (몸, 바깥선, 그림자 깊이)
	//==============================================================================
	pub fn read_program_sources() -> [(String, String); 3] {
		ToonRenderer::read_material_program_sources(None, None)
	}

	//==============================================================================
	/// 머티리얼과 인스턴스 꼴의 셰이더 소스. ((정점, 조각) 세 벌, 어트리뷰트 자리는 compose_toon_program_sources 의 attribute_locations)
	///
	/// # 인자
	/// - `material`: 셰이더 조각을 끼울 머티리얼. (None 이면 빈 조각)
	/// - `instance_layout`: 인스턴스 꼴. (None 이면 없음)
	///
	/// # 돌려줌
	/// (정점, 조각) 소스 세 벌. (몸, 바깥선, 그림자 깊이)
	//==============================================================================
	pub fn read_material_program_sources(material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> [(String, String); 3] {
		let sources = compose_toon_program_sources(material, instance_layout);
		[(sources.solid_vertex_shader, sources.solid_fragment_shader), (sources.outline_vertex_shader, sources.outline_fragment_shader), (sources.depth_vertex_shader, sources.depth_fragment_shader)]
	}

	//==============================================================================
	/// 생성. (기본 세 프로그램을 짓는다)
	///
	/// # 인자
	/// - `gl`: 그릴 GL 컨텍스트.
	///
	/// # 돌려줌
	/// 새 렌더러. 기본 프로그램이나 텍스처를 짓지 못하면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &crate::GlContext) -> Result<ToonRenderer, String> {
		let default_program_set = ToonRenderer::create_program_set(gl, None, None)?;
		let empty_shadow_texture = ToonRenderer::create_empty_shadow_texture(gl)?;
		let white_texture = upload_texture(gl, 1, 1, &[255, 255, 255, 255], ToonTextureOptions { is_mipmapped: false, ..ToonTextureOptions::default() })?;
		let mut program_set_table = HashMap::new();
		program_set_table.insert((0, 0), Some(default_program_set));
		Ok(ToonRenderer {
			program_set_table,
			empty_shadow_texture,
			white_texture,
			view_projection_matrix: Matrix4::default(),
			frustum_planes: [[0.0, 0.0, 0.0, 1.0]; 6],
			camera_position: [0.0; 4],
			camera_forward: [0.0, 0.0, -1.0],
			is_frame_mirrored: false,
			light_set: ToonLightSet::new(),
			outline_color: [0.05, 0.05, 0.08, 1.0],
			is_hdr_output: false,
			shadow_input: None,
			is_shadow_pass: false,
			frame_serial: 1,
			saved_state: None,
			instance_data: Vec::new(),
			frame_statistics: ToonFrameStatistics::default(),
			uniform_slot_table: RefCell::new(HashMap::new()),
			uniform_serial: Cell::new(1),
			screen_rect: None,
			is_scissor_applied: false,
			view_rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
			camera_roll: 0.0,
			outline_distance_scale: [0.0, 0.5, 4.0],
			outline_fade_distance: [0.0, 0.0],
			local_shadow_input: None,
		})
	}

	//==============================================================================
	/// 세 프로그램 짓기. (짓지 못하면 Err, 몸 프로그램의 텍스처 유닛을 정해 둔다)
	//==============================================================================
	fn create_program_set(gl: &crate::GlContext, material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> Result<ToonProgramSet, String> {
		let sources = compose_toon_program_sources(material, instance_layout);
		let attribute_locations: Vec<(&str, u32)> = sources.attribute_locations.iter().map(|(name, location)| (name.as_str(), *location)).collect();
		let solid_program = ShaderProgram::create_with_attribute_locations(gl, &sources.solid_vertex_shader, &sources.solid_fragment_shader, &attribute_locations)?;
		let outline_program = match ShaderProgram::create_with_attribute_locations(gl, &sources.outline_vertex_shader, &sources.outline_fragment_shader, &attribute_locations) {
			Ok(program) => {
				program
			}
			Err(error) => {
				unsafe {
					crate::core::glstatecache::delete_program(gl, solid_program.get_program());
				}
				return Err(error);
			}
		};
		let depth_program = match ShaderProgram::create_with_attribute_locations(gl, &sources.depth_vertex_shader, &sources.depth_fragment_shader, &attribute_locations) {
			Ok(program) => {
				program
			}
			Err(error) => {
				unsafe {
					crate::core::glstatecache::delete_program(gl, solid_program.get_program());
					crate::core::glstatecache::delete_program(gl, outline_program.get_program());
				}
				return Err(error);
			}
		};
		unsafe {
			let previous_program = crate::graphics3d::read_program_binding(gl);
			solid_program.use_program(gl);
			for (name, unit) in [("shadowTexture", SHADOW_TEXTURE_UNIT), ("baseColorTexture", BASE_COLOR_TEXTURE_UNIT), ("emissiveTexture", EMISSIVE_TEXTURE_UNIT), ("decalTexture", DECAL_TEXTURE_UNIT), ("rampTexture", RAMP_TEXTURE_UNIT), ("matcapTexture", MATCAP_TEXTURE_UNIT), ("localShadowTexture", LOCAL_SHADOW_TEXTURE_UNIT), ("angelRingTexture", ANGEL_RING_TEXTURE_UNIT)] {
				gl.uniform_1_i32(solid_program.get_uniform_location(gl, name).as_ref(), unit as i32);
			}
			crate::core::glstatecache::use_program(gl, previous_program);
		}
		Ok(ToonProgramSet {
			solid: ToonProgramEntry::new(solid_program, ToonProgramKind::Solid),
			outline: ToonProgramEntry::new(outline_program, ToonProgramKind::Outline),
			depth: ToonProgramEntry::new(depth_program, ToonProgramKind::Depth),
		})
	}

	//==============================================================================
	/// 머티리얼과 인스턴스 꼴의 프로그램이 있게 하기. (없으면 짓고, 짓지 못한 것은 알리고 None 을 기억해 다시 짓지 않는다, 있으면 true)
	//==============================================================================
	fn ensure_program_set(&mut self, gl: &crate::GlContext, material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> bool {
		let key = (material.map(|material| material.get_id()).unwrap_or(0), instance_layout.map(|instance_layout| instance_layout.get_id()).unwrap_or(0));
		if let Some(program_set) = self.program_set_table.get(&key) {
			return program_set.is_some();
		}
		let program_set = match ToonRenderer::create_program_set(gl, material, instance_layout) {
			Ok(program_set) => {
				Some(program_set)
			}
			Err(error) => {
				let name = material.map(|material| String::from(material.get_name())).filter(|name| !name.is_empty()).unwrap_or_else(|| format!("{}/{}", key.0, key.1));
				eprintln!("[ToonRenderer] 머티리얼 {} 의 셰이더를 짓지 못했다. {}", name, error);
				None
			}
		};
		let is_created = program_set.is_some();
		self.program_set_table.insert(key, program_set);
		is_created
	}

	//==============================================================================
	/// 머티리얼 짓기. (셰이더 조각, toonmaterial.rs 머리말)
	///
	/// # 인자
	/// - `chunks`: 셰이더 조각. (name, uniform_chunk, surface_chunk, light_chunk, vertex_chunk, 빈 글이면 엔진의 빈 함수)
	///
	/// # 돌려줌
	/// 지은 머티리얼.
	//==============================================================================
	pub fn create_material(&self, chunks: ToonMaterialChunks) -> ToonMaterial {
		ToonMaterial::new(chunks)
	}

	//==============================================================================
	/// 머티리얼의 프로그램을 미리 짓기. (처음 그릴 때의 멈춤을 피할 때, 짓지 못하면 false)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `material`: 미리 지을 머티리얼. (None 이면 기본 프로그램)
	/// - `instance_layout`: 함께 쓸 인스턴스 꼴. (None 이면 없음)
	///
	/// # 돌려줌
	/// 짓지 못하면 false.
	//==============================================================================
	pub fn prepare_material(&mut self, gl: &crate::GlContext, material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> bool {
		self.ensure_program_set(gl, material, instance_layout)
	}

	//==============================================================================
	/// 머티리얼의 프로그램 지우기. (모든 인스턴스 꼴의 것, 다시 그리면 새로 짓는다)
	///
	/// # 인자
	/// - `gl`: 프로그램을 지은 GL 컨텍스트.
	/// - `material`: 프로그램을 지울 머티리얼.
	//==============================================================================
	pub fn delete_material(&mut self, gl: &crate::GlContext, material: &ToonMaterial) {
		let material_id = material.get_id();
		let key_list: Vec<(u32, u32)> = self.program_set_table.keys().filter(|key| key.0 == material_id).copied().collect();
		for key in key_list {
			if let Some(Some(program_set)) = self.program_set_table.remove(&key) {
				ToonRenderer::delete_program_set(gl, &program_set);
				// 지운 프로그램의 이름은 GL 이 다시 줄 수 있어 기억한 유니폼 값도 버린다.
				let uniform_slot_table = self.uniform_slot_table.get_mut();
				for program in [&program_set.solid.program, &program_set.outline.program, &program_set.depth.program] {
					uniform_slot_table.remove(&program.get_program());
				}
			}
		}
	}

	//==============================================================================
	/// 세 프로그램 지우기.
	//==============================================================================
	fn delete_program_set(gl: &crate::GlContext, program_set: &ToonProgramSet) {
		unsafe {
			crate::core::glstatecache::delete_program(gl, program_set.solid.program.get_program());
			crate::core::glstatecache::delete_program(gl, program_set.outline.program.get_program());
			crate::core::glstatecache::delete_program(gl, program_set.depth.program.get_program());
		}
	}

	//==============================================================================
	/// 빛 묶음 짓기. (그리기마다 ToonDrawOptions 의 light_set 으로 줄 것, 기본 빛과 같은 값으로 시작)
	///
	/// # 돌려줌
	/// 기본 빛과 같은 값으로 시작하는 새 빛 묶음.
	//==============================================================================
	pub fn create_light_set(&self) -> ToonLightSet {
		ToonLightSet::new()
	}

	//==============================================================================
	/// 기본 빛 묶음 반환.
	///
	/// # 돌려줌
	/// 기본 빛 묶음.
	//==============================================================================
	pub fn get_light_set(&self) -> &ToonLightSet {
		&self.light_set
	}

	//==============================================================================
	/// 기본 빛 묶음 반환. (고칠 때)
	///
	/// # 돌려줌
	/// 기본 빛 묶음. (고칠 수 있는 참조)
	//==============================================================================
	pub fn get_light_set_mut(&mut self) -> &mut ToonLightSet {
		&mut self.light_set
	}

	//==============================================================================
	/// 인스턴스 꼴 짓기. ((이름, 크기) 목록, toonmaterial.rs 의 ToonInstanceLayout)
	///
	/// # 인자
	/// - `attributes`: (이름, 크기) 의 목록.
	///
	/// # 돌려줌
	/// 지은 인스턴스 꼴. 이름이 셰이더 이름 꼴이 아니거나 겹치거나, 크기가 맞지 않거나, 사용자 어트리뷰트가 너무 많으면 Err(오류 글).
	//==============================================================================
	pub fn create_instance_layout(&self, attributes: &[(&str, usize)]) -> Result<ToonInstanceLayout, String> {
		ToonInstanceLayout::new(attributes)
	}

	//==============================================================================
	/// 영구 인스턴스 버퍼 짓기. (꼴의 수 개수씩 담은 data, ToonBufferUsage::Static 이면 한 번 올리고 오래 그림)
	///
	/// # 인자
	/// - `gl`: 버퍼를 지을 GL 컨텍스트.
	/// - `instance_layout`: 인스턴스 하나의 꼴.
	/// - `data`: 꼴의 수 개수씩 인스턴스를 이어 담은 수.
	/// - `usage`: ToonBufferUsage 의 값. (Static 이면 한 번 올리고 오래 그림)
	///
	/// # 돌려줌
	/// 인스턴스 버퍼. 버퍼를 짓지 못하면 Err(GL 의 오류 글).
	//==============================================================================
	pub fn create_instance_buffer(&self, gl: &crate::GlContext, instance_layout: &ToonInstanceLayout, data: &[f32], usage: ToonBufferUsage) -> Result<ToonInstanceBuffer, String> {
		let gl_usage = match usage {
			ToonBufferUsage::Static => {
				glow::STATIC_DRAW
			}
			ToonBufferUsage::Dynamic => {
				glow::DYNAMIC_DRAW
			}
		};
		unsafe {
			let buffer = crate::core::glstatecache::create_buffer(gl)?;
			crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(data), gl_usage);
			Ok(ToonInstanceBuffer { layout: instance_layout.clone(), buffer, usage: gl_usage, float_count: Cell::new(data.len()), vertex_array_list: RefCell::new(Vec::new()) })
		}
	}

	//==============================================================================
	/// 인스턴스 버퍼 고치기. (전체를 다시 올림)
	///
	/// # 인자
	/// - `gl`: 버퍼를 지은 GL 컨텍스트.
	/// - `instance_buffer`: create_instance_buffer 의 결과.
	/// - `data`: 새 수. (인스턴스 수가 바뀌어도 된다)
	//==============================================================================
	pub fn update_instance_buffer(&self, gl: &crate::GlContext, instance_buffer: &ToonInstanceBuffer, data: &[f32]) {
		unsafe {
			crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(instance_buffer.buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(data), instance_buffer.usage);
		}
		instance_buffer.float_count.set(data.len());
	}

	//==============================================================================
	/// 인스턴스 버퍼 지우기. (그 버퍼로 지은 버텍스 어레이도, 모형보다 먼저 지운다)
	///
	/// # 인자
	/// - `gl`: 버퍼를 지은 GL 컨텍스트.
	/// - `instance_buffer`: create_instance_buffer 의 결과.
	//==============================================================================
	pub fn delete_instance_buffer(&self, gl: &crate::GlContext, instance_buffer: ToonInstanceBuffer) {
		unsafe {
			for (_, body_vertex_array, outline_vertex_array) in instance_buffer.vertex_array_list.borrow().iter() {
				crate::core::glstatecache::delete_vertex_array(gl, *body_vertex_array);
				if let Some(outline_vertex_array) = outline_vertex_array {
					crate::core::glstatecache::delete_vertex_array(gl, *outline_vertex_array);
				}
			}
			crate::core::glstatecache::delete_buffer(gl, instance_buffer.buffer);
		}
	}

	//==============================================================================
	/// 텍스처 짓기. (RGBA 바이트 너비 x 높이 x 4, 그림 풀기는 decode_glb_image 나 image 크레이트)
	///
	/// # 인자
	/// - `gl`: 텍스처를 지을 GL 컨텍스트.
	/// - `width`: 너비. (픽셀)
	/// - `height`: 높이. (픽셀)
	/// - `pixels`: RGBA 바이트. (너비 x 높이 x 4 개 이상)
	/// - `options`: 밉맵, 되풀이, 고르게 읽기. (ToonTextureOptions)
	///
	/// # 돌려줌
	/// 지은 RGBA 텍스처. 너비나 높이가 0 이거나 바이트가 모자라거나 텍스처를 짓지 못하면 Err(오류 글).
	//==============================================================================
	pub fn create_texture(&self, gl: &crate::GlContext, width: u32, height: u32, pixels: &[u8], options: ToonTextureOptions) -> Result<GlTexture, String> {
		upload_texture(gl, width, height, pixels, options)
	}

	//==============================================================================
	/// 텍스처 지우기.
	///
	/// # 인자
	/// - `gl`: 텍스처를 지은 GL 컨텍스트.
	/// - `texture`: 지울 텍스처.
	//==============================================================================
	pub fn delete_texture(&self, gl: &crate::GlContext, texture: GlTexture) {
		unsafe {
			crate::core::glstatecache::delete_texture(gl, texture);
		}
	}

	//==============================================================================
	/// 빈 그림자 텍스처 짓기. (1x1 깊이 1, 비교 샘플링)
	//==============================================================================
	fn create_empty_shadow_texture(gl: &crate::GlContext) -> Result<GlTexture, String> {
		unsafe {
			let texture = crate::core::glstatecache::create_texture(gl)?;
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0 + SHADOW_TEXTURE_UNIT);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(texture));
			// 웹의 glow 는 UNSIGNED_INT 바이트를 u32 배열로 보므로 4 바이트 맞춘 u32 에서 바이트를 읽는다.
			let depth_values = [u32::MAX];
			let depth_bytes = index_slice_to_bytes(&depth_values);
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, 1, 1, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(Some(depth_bytes)));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			Ok(texture)
		}
	}

	//==============================================================================
	/// 렌더러 지우기. (모든 프로그램과 빈 텍스처, 모형과 인스턴스 버퍼와 텍스처는 따로)
	///
	/// # 인자
	/// - `gl`: 렌더러를 지은 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&self, gl: &crate::GlContext) {
		for program_set in self.program_set_table.values().flatten() {
			ToonRenderer::delete_program_set(gl, program_set);
		}
		self.uniform_slot_table.borrow_mut().clear();
		unsafe {
			crate::core::glstatecache::delete_texture(gl, self.empty_shadow_texture);
			crate::core::glstatecache::delete_texture(gl, self.white_texture);
		}
	}

	//==============================================================================
	/// 꼭짓점 버퍼 만들기.
	//==============================================================================
	unsafe fn create_array_buffer(gl: &crate::GlContext, data: &[f32], usage: u32) -> Option<GlBuffer> {
		let buffer = crate::core::glstatecache::create_buffer(gl).ok()?;
		crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(buffer));
		gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(data), usage);
		Some(buffer)
	}

	//==============================================================================
	/// 번호 버퍼 만들기. (지금 묶인 버텍스 어레이에 붙는다)
	//==============================================================================
	unsafe fn create_index_buffer(gl: &crate::GlContext, data: &[u32]) -> Option<GlBuffer> {
		let buffer = crate::core::glstatecache::create_buffer(gl).ok()?;
		crate::core::glstatecache::bind_buffer(gl, glow::ELEMENT_ARRAY_BUFFER, Some(buffer));
		gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_slice_to_bytes(data), glow::STATIC_DRAW);
		Some(buffer)
	}

	//==============================================================================
	/// 모형 올리기. (몸이 비면 None)
	/// - GL 버퍼나 버텍스 어레이를 짓다 실패하면 그때까지 지은 것을 지우고 None 을 돌려준다.
	///
	/// # 인자
	/// - `gl`: 모형을 올릴 GL 컨텍스트.
	/// - `arrays`: MeshBuilder::read_arrays 나 read_glb_arrays 의 결과.
	///
	/// # 돌려줌
	/// GPU 에 올린 모형. (몸의 꼭짓점 번호가 비거나 GL 버퍼를 짓지 못하면 None)
	//==============================================================================
	pub fn create_mesh(&self, gl: &crate::GlContext, arrays: &ToonMeshArrays) -> Option<ToonMesh> {
		ToonRenderer::create_mesh_objects(gl, arrays)
	}

	//==============================================================================
	/// 모형의 GL 객체 짓기. (create_mesh 의 몸, 짓다 실패하면 그때까지 지은 버퍼와 버텍스 어레이를 지운다)
	//==============================================================================
	fn create_mesh_objects(gl: &crate::GlContext, arrays: &ToonMeshArrays) -> Option<ToonMesh> {
		if arrays.index_data.is_empty() {
			return None;
		}
		let mut created_buffer_list = Vec::new();
		let mut created_vertex_array_list = Vec::new();
		let mesh = unsafe {
			ToonRenderer::build_mesh_objects(gl, arrays, &mut created_buffer_list, &mut created_vertex_array_list)
		};
		if mesh.is_none() {
			unsafe {
				crate::core::glstatecache::bind_vertex_array(gl, None);
				for vertex_array in created_vertex_array_list {
					crate::core::glstatecache::delete_vertex_array(gl, vertex_array);
				}
				for buffer in created_buffer_list {
					crate::core::glstatecache::delete_buffer(gl, buffer);
				}
			}
		}
		mesh
	}

	//==============================================================================
	/// 모형의 버퍼와 버텍스 어레이 짓기. (지은 것을 차례로 created_* 에 적는다, 하나라도 짓지 못하면 None)
	//==============================================================================
	unsafe fn build_mesh_objects(gl: &crate::GlContext, arrays: &ToonMeshArrays, created_buffer_list: &mut Vec<GlBuffer>, created_vertex_array_list: &mut Vec<GlVertexArray>) -> Option<ToonMesh> {
		let is_skinned = !arrays.skin_data.is_empty();
		let vertex_count = arrays.vertex_data.len() / VERTEX_FLOAT_COUNT;
		let has_uv = vertex_count > 0 && arrays.uv_data.len() >= vertex_count * 2;
		let has_second_uv = has_uv && arrays.uv1_data.len() >= vertex_count * 2;
		let (bounding_center, mut bounding_radius) = read_bounding_sphere(&arrays.vertex_data, VERTEX_FLOAT_COUNT);
		if is_skinned {
			bounding_radius *= 1.5;
		}
		let mut buffer_list = Vec::new();

		// 인스턴스 버퍼. (보통 그리기에서도 인스턴스 어트리뷰트가 켜져 있으므로 한 인스턴스 분량을 늘 채워 둔다, 비면 드라이버가 버퍼 밖을 읽는다)
		let mut default_instance = Matrix4::default().elements.to_vec();
		default_instance.extend_from_slice(&[1.0; 4]);
		let instance_buffer = ToonRenderer::create_array_buffer(gl, &default_instance, glow::DYNAMIC_DRAW)?;
		created_buffer_list.push(instance_buffer);

		// 몸.
		let vertex_buffer = ToonRenderer::create_array_buffer(gl, &arrays.vertex_data, glow::STATIC_DRAW)?;
		created_buffer_list.push(vertex_buffer);
		buffer_list.push(vertex_buffer);
		let skin_buffer = if is_skinned {
			let skin_buffer = ToonRenderer::create_array_buffer(gl, &arrays.skin_data, glow::STATIC_DRAW)?;
			created_buffer_list.push(skin_buffer);
			buffer_list.push(skin_buffer);
			Some(skin_buffer)
		}
		else {
			None
		};
		let uv_buffer = if has_uv {
			let uv_buffer = ToonRenderer::create_array_buffer(gl, &read_uv_data(&arrays.uv_data, if has_second_uv { Some(&arrays.uv1_data) } else { None }, vertex_count), glow::STATIC_DRAW)?;
			created_buffer_list.push(uv_buffer);
			buffer_list.push(uv_buffer);
			Some(uv_buffer)
		}
		else {
			None
		};
		let body_vertex_array = crate::core::glstatecache::create_vertex_array(gl).ok()?;
		created_vertex_array_list.push(body_vertex_array);
		crate::core::glstatecache::bind_vertex_array(gl, Some(body_vertex_array));
		let body_index_buffer = ToonRenderer::create_index_buffer(gl, &arrays.index_data)?;
		created_buffer_list.push(body_index_buffer);
		buffer_list.push(body_index_buffer);
		let mut mesh = ToonMesh {
			mesh_id: NEXT_MESH_ID.fetch_add(1, Ordering::Relaxed),
			body_vertex_array,
			body_index_count: arrays.index_data.len() as i32,
			outline_vertex_array: None,
			outline_index_count: 0,
			instance_buffer,
			buffer_list: Vec::new(),
			is_skinned,
			has_uv,
			has_second_uv,
			bounding_center,
			bounding_radius,
			vertex_buffer,
			skin_buffer,
			uv_buffer,
			body_index_buffer,
			outline_buffer: None,
			outline_skin_buffer: None,
			outline_index_buffer: None,
		};
		ToonRenderer::attach_body_attributes(gl, &mesh);
		ToonRenderer::attach_instance_attributes(gl, instance_buffer);

		// 바깥선 껍데기. (바탕 자리 3 과 미는 쪽 3, 미는 쪽은 지은 굵기의 사본 자리 - 바탕 자리, 바탕이 없으면 0)
		if !arrays.outline_index_data.is_empty() {
			let outline_buffer = ToonRenderer::create_array_buffer(gl, &read_outline_vertex_data(&arrays.outline_vertex_data, &arrays.outline_base_data), glow::STATIC_DRAW)?;
			created_buffer_list.push(outline_buffer);
			buffer_list.push(outline_buffer);
			if is_skinned && !arrays.outline_skin_data.is_empty() {
				let outline_skin_buffer = ToonRenderer::create_array_buffer(gl, &arrays.outline_skin_data, glow::STATIC_DRAW)?;
				created_buffer_list.push(outline_skin_buffer);
				buffer_list.push(outline_skin_buffer);
				mesh.outline_skin_buffer = Some(outline_skin_buffer);
			}
			let outline_vertex_array = crate::core::glstatecache::create_vertex_array(gl).ok()?;
			created_vertex_array_list.push(outline_vertex_array);
			crate::core::glstatecache::bind_vertex_array(gl, Some(outline_vertex_array));
			let outline_index_buffer = ToonRenderer::create_index_buffer(gl, &arrays.outline_index_data)?;
			created_buffer_list.push(outline_index_buffer);
			buffer_list.push(outline_index_buffer);
			mesh.outline_buffer = Some(outline_buffer);
			mesh.outline_index_buffer = Some(outline_index_buffer);
			ToonRenderer::attach_outline_attributes(gl, &mesh);
			ToonRenderer::attach_instance_attributes(gl, instance_buffer);
			mesh.outline_vertex_array = Some(outline_vertex_array);
			mesh.outline_index_count = arrays.outline_index_data.len() as i32;
		}
		crate::core::glstatecache::bind_vertex_array(gl, None);
		mesh.buffer_list = buffer_list;
		Some(mesh)
	}

	//==============================================================================
	/// 몸의 꼭짓점, 뼈대, UV 어트리뷰트와 번호 버퍼를 지금 묶인 버텍스 어레이에 붙이기.
	//==============================================================================
	unsafe fn attach_body_attributes(gl: &crate::GlContext, mesh: &ToonMesh) {
		let stride = (VERTEX_FLOAT_COUNT * 4) as i32;
		crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(mesh.vertex_buffer));
		for (location, size, offset) in [(0u32, 3i32, 0i32), (1, 3, 12), (2, 3, 24), (3, 2, 36), (4, 4, 44)] {
			gl.enable_vertex_attrib_array(location);
			gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
		}
		if let Some(skin_buffer) = mesh.skin_buffer {
			ToonRenderer::attach_skin_attributes(gl, skin_buffer);
		}
		if let Some(uv_buffer) = mesh.uv_buffer {
			crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(uv_buffer));
			gl.enable_vertex_attrib_array(UV_LOCATION);
			gl.vertex_attrib_pointer_f32(UV_LOCATION, if mesh.has_second_uv { 4 } else { 2 }, glow::FLOAT, false, if mesh.has_second_uv { 16 } else { 8 }, 0);
		}
		crate::core::glstatecache::bind_buffer(gl, glow::ELEMENT_ARRAY_BUFFER, Some(mesh.body_index_buffer));
	}

	//==============================================================================
	/// 바깥선 껍데기의 바탕 자리와 미는 쪽(노멀 자리), 뼈대 어트리뷰트와 번호 버퍼를 지금 묶인 버텍스 어레이에 붙이기.
	//==============================================================================
	unsafe fn attach_outline_attributes(gl: &crate::GlContext, mesh: &ToonMesh) {
		crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, mesh.outline_buffer);
		gl.enable_vertex_attrib_array(0);
		gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
		gl.enable_vertex_attrib_array(1);
		gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
		if let Some(outline_skin_buffer) = mesh.outline_skin_buffer {
			ToonRenderer::attach_skin_attributes(gl, outline_skin_buffer);
		}
		crate::core::glstatecache::bind_buffer(gl, glow::ELEMENT_ARRAY_BUFFER, mesh.outline_index_buffer);
	}

	//==============================================================================
	/// 뼈대 번호와 무게의 버퍼를 지금 묶인 버텍스 어레이에 어트리뷰트로 붙이기.
	//==============================================================================
	unsafe fn attach_skin_attributes(gl: &crate::GlContext, skin_buffer: GlBuffer) {
		let stride = (SKIN_FLOAT_COUNT * 4) as i32;
		crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(skin_buffer));
		gl.enable_vertex_attrib_array(5);
		gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, stride, 0);
		gl.enable_vertex_attrib_array(6);
		gl.vertex_attrib_pointer_f32(6, 4, glow::FLOAT, false, stride, 16);
	}

	//==============================================================================
	/// 인스턴스 버퍼를 지금 묶인 버텍스 어레이의 인스턴스 어트리뷰트로. (행렬 네 줄, 틴트, 인스턴스마다 하나씩)
	//==============================================================================
	unsafe fn attach_instance_attributes(gl: &crate::GlContext, instance_buffer: GlBuffer) {
		let stride = (TOON_INSTANCE_FLOAT_COUNT * 4) as i32;
		crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(instance_buffer));
		for column_index in 0..5u32 {
			let location = INSTANCE_MATRIX_LOCATION + column_index;
			gl.enable_vertex_attrib_array(location);
			gl.vertex_attrib_pointer_f32(location, 4, glow::FLOAT, false, stride, (column_index * 16) as i32);
			gl.vertex_attrib_divisor(location, 1);
		}
	}

	//==============================================================================
	/// 인스턴스 꼴의 버퍼를 지금 묶인 버텍스 어레이의 인스턴스 어트리뷰트로.
	//==============================================================================
	unsafe fn attach_layout_attributes(gl: &crate::GlContext, instance_layout: &ToonInstanceLayout, buffer: GlBuffer) {
		let stride = (instance_layout.get_float_count() * 4) as i32;
		crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(buffer));
		let attach = |location: u32, size: i32, float_offset: usize| {
			gl.enable_vertex_attrib_array(location);
			gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, (float_offset * 4) as i32);
			gl.vertex_attrib_divisor(location, 1);
		};
		for attribute in instance_layout.get_attributes() {
			if attribute.name == TOON_INSTANCE_MATRIX_NAME {
				for column_index in 0..4u32 {
					attach(INSTANCE_MATRIX_LOCATION + column_index, 4, attribute.offset + column_index as usize * 4);
				}
			}
			else if attribute.name == TOON_INSTANCE_TINT_NAME {
				attach(INSTANCE_TINT_LOCATION, 4, attribute.offset);
			}
			else if let Some(location) = attribute.location {
				attach(location, attribute.size as i32, attribute.offset);
			}
		}
	}

	//==============================================================================
	/// 모형과 인스턴스 버퍼를 함께 묶은 버텍스 어레이. (처음 그릴 때 짓고 인스턴스 버퍼에 담아 둔다, (몸, 바깥선))
	//==============================================================================
	fn read_instance_vertex_arrays(&self, gl: &crate::GlContext, mesh: &ToonMesh, instance_buffer: &ToonInstanceBuffer) -> Option<(GlVertexArray, Option<GlVertexArray>)> {
		if let Some((_, body_vertex_array, outline_vertex_array)) = instance_buffer.vertex_array_list.borrow().iter().find(|entry| entry.0 == mesh.mesh_id) {
			return Some((*body_vertex_array, *outline_vertex_array));
		}
		unsafe {
			let body_vertex_array = crate::core::glstatecache::create_vertex_array(gl).ok()?;
			crate::core::glstatecache::bind_vertex_array(gl, Some(body_vertex_array));
			ToonRenderer::attach_body_attributes(gl, mesh);
			ToonRenderer::attach_layout_attributes(gl, &instance_buffer.layout, instance_buffer.buffer);
			let mut outline_vertex_array = None;
			if mesh.outline_vertex_array.is_some() {
				// 바깥선 쪽을 짓지 못하면 먼저 지은 몸 쪽을 지운다.
				let vertex_array = match crate::core::glstatecache::create_vertex_array(gl) {
					Ok(vertex_array) => {
						vertex_array
					}
					Err(_) => {
						crate::core::glstatecache::bind_vertex_array(gl, None);
						crate::core::glstatecache::delete_vertex_array(gl, body_vertex_array);
						return None;
					}
				};
				crate::core::glstatecache::bind_vertex_array(gl, Some(vertex_array));
				ToonRenderer::attach_outline_attributes(gl, mesh);
				ToonRenderer::attach_layout_attributes(gl, &instance_buffer.layout, instance_buffer.buffer);
				outline_vertex_array = Some(vertex_array);
			}
			crate::core::glstatecache::bind_vertex_array(gl, None);
			instance_buffer.vertex_array_list.borrow_mut().push((mesh.mesh_id, body_vertex_array, outline_vertex_array));
			Some((body_vertex_array, outline_vertex_array))
		}
	}

	//==============================================================================
	/// 모형 지우기.
	///
	/// # 인자
	/// - `gl`: 모형을 올린 GL 컨텍스트.
	/// - `mesh`: create_mesh 의 결과.
	//==============================================================================
	pub fn delete_mesh(&self, gl: &crate::GlContext, mesh: ToonMesh) {
		unsafe {
			crate::core::glstatecache::delete_vertex_array(gl, mesh.body_vertex_array);
			if let Some(outline_vertex_array) = mesh.outline_vertex_array {
				crate::core::glstatecache::delete_vertex_array(gl, outline_vertex_array);
			}
			crate::core::glstatecache::delete_buffer(gl, mesh.instance_buffer);
			for buffer in mesh.buffer_list {
				crate::core::glstatecache::delete_buffer(gl, buffer);
			}
		}
	}

	//==============================================================================
	/// 그리기 전의 상태 적어 두기.
	//==============================================================================
	fn save_state(&mut self, gl: &crate::GlContext) {
		unsafe {
			let mut viewport = [0i32; 4];
			crate::core::glstatecache::get_parameter_i32_slice(gl, glow::VIEWPORT, &mut viewport);
			self.saved_state = Some(SavedState {
				is_blend_enabled: crate::core::glstatecache::is_enabled(gl, glow::BLEND),
				is_depth_test_enabled: crate::core::glstatecache::is_enabled(gl, glow::DEPTH_TEST),
				is_cull_face_enabled: crate::core::glstatecache::is_enabled(gl, glow::CULL_FACE),
				depth_mask: crate::core::glstatecache::get_parameter_bool(gl, glow::DEPTH_WRITEMASK),
				blend_source_rgb: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_SRC_RGB),
				blend_destination_rgb: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_DST_RGB),
				blend_source_alpha: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_SRC_ALPHA),
				blend_destination_alpha: crate::core::glstatecache::get_parameter_i32(gl, glow::BLEND_DST_ALPHA),
				program: crate::graphics3d::read_program_binding(gl),
				vertex_array: crate::graphics3d::read_vertex_array_binding(gl),
				viewport,
				framebuffer: crate::graphics3d::read_framebuffer_binding(gl),
				active_texture: crate::core::glstatecache::get_parameter_i32(gl, glow::ACTIVE_TEXTURE),
				first_unit_texture: ToonRenderer::read_first_unit_texture(gl),
			});
		}
	}

	//==============================================================================
	/// 유닛 0 에 묶인 텍스처 읽기. (지금 고른 유닛은 그대로 둔다)
	//==============================================================================
	fn read_first_unit_texture(gl: &crate::GlContext) -> Option<GlTexture> {
		unsafe {
			let active_texture = crate::core::glstatecache::get_parameter_i32(gl, glow::ACTIVE_TEXTURE);
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			let texture = crate::graphics3d::read_texture_binding(gl);
			crate::core::glstatecache::active_texture(gl, active_texture as u32);
			texture
		}
	}

	//==============================================================================
	/// 적어 둔 상태로 돌리기.
	//==============================================================================
	fn restore_state(&mut self, gl: &crate::GlContext, is_framebuffer_restored: bool) {
		self.forget_all_uniform_values();
		unsafe {
			crate::core::glstatecache::cull_face(gl, glow::BACK);
			crate::core::glstatecache::front_face(gl, glow::CCW);
			crate::core::glstatecache::color_mask(gl, true, true, true, true);
			let saved_state = match self.saved_state.take() {
				Some(saved_state) => {
					saved_state
				}
				None => {
					return;
				}
			};
			if is_framebuffer_restored {
				crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, saved_state.framebuffer);
				crate::core::glstatecache::viewport(gl, saved_state.viewport[0], saved_state.viewport[1], saved_state.viewport[2], saved_state.viewport[3]);
			}
			crate::core::glstatecache::bind_vertex_array(gl, saved_state.vertex_array);
			crate::core::glstatecache::use_program(gl, saved_state.program);
			crate::core::glstatecache::depth_mask(gl, saved_state.depth_mask);
			crate::core::glstatecache::blend_func_separate(gl, saved_state.blend_source_rgb as u32, saved_state.blend_destination_rgb as u32, saved_state.blend_source_alpha as u32, saved_state.blend_destination_alpha as u32);
			for (capability, is_enabled) in [(glow::BLEND, saved_state.is_blend_enabled), (glow::DEPTH_TEST, saved_state.is_depth_test_enabled), (glow::CULL_FACE, saved_state.is_cull_face_enabled)] {
				if is_enabled {
					crate::core::glstatecache::enable(gl, capability);
				}
				else {
					crate::core::glstatecache::disable(gl, capability);
				}
			}
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, saved_state.first_unit_texture);
			crate::core::glstatecache::active_texture(gl, saved_state.active_texture as u32);
		}
	}

	//==============================================================================
	/// 프레임의 그리기 상태 걸기. (섞기 끔, 깊이 시험 LEQUAL 과 쓰기, 뒷면 가리기, 앞면 CCW, ToonDrawQueue 가 단계 콜백 뒤에 다시 건다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn apply_frame_state(&self, gl: &crate::GlContext) {
		// 단계 콜백, 화면 공간 바깥선처럼 렌더러 밖의 코드가 유니폼을 바꿨을 수 있다.
		self.forget_all_uniform_values();
		unsafe {
			crate::core::glstatecache::disable(gl, glow::BLEND);
			crate::core::glstatecache::enable(gl, glow::DEPTH_TEST);
			crate::core::glstatecache::depth_func(gl, glow::LEQUAL);
			crate::core::glstatecache::depth_mask(gl, true);
			crate::core::glstatecache::enable(gl, glow::CULL_FACE);
			crate::core::glstatecache::cull_face(gl, glow::BACK);
			crate::core::glstatecache::front_face(gl, glow::CCW);
			crate::core::glstatecache::color_mask(gl, true, true, true, true);
		}
	}

	//==============================================================================
	/// 프레임 시작. (보기와 투영과 절두체를 정하고, 앞의 상태를 적어 둔 뒤 깊이 시험과 면 가리기를 켠다, 깊이를 비우는 것은 부르는 쪽이 정한다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `view_projection_matrix`: 보기와 투영 행렬. (투영 x 보기)
	//==============================================================================
	pub fn begin_frame(&mut self, gl: &crate::GlContext, view_projection_matrix: &Matrix4) {
		self.view_projection_matrix = *view_projection_matrix;
		self.frustum_planes = extract_frustum_planes(view_projection_matrix);
		let frame_camera = read_frame_camera(view_projection_matrix);
		self.camera_position = frame_camera.position;
		self.camera_forward = frame_camera.forward;
		self.is_frame_mirrored = frame_camera.is_mirrored;
		self.view_rotation = read_view_rotation(view_projection_matrix, frame_camera.forward).map(|value| value as f32);
		self.camera_roll = read_camera_roll(&self.view_rotation, frame_camera.is_mirrored);
		self.frame_statistics = ToonFrameStatistics::default();
		self.is_shadow_pass = false;
		self.frame_serial += 1;
		self.save_state(gl);
		self.apply_screen_rect(gl);
		self.apply_frame_state(gl);
	}

	//==============================================================================
	/// 프레임 끝. (적어 둔 상태로 돌린다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn end_frame(&mut self, gl: &crate::GlContext) {
		self.release_screen_rect(gl);
		self.restore_state(gl, false);
	}

	//==============================================================================
	/// 카메라로 프레임 시작. (Camera3D 의 보기와 투영으로 begin_frame)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `camera`: Camera3D. (보기와 투영을 읽는다)
	//==============================================================================
	pub fn begin_frame_with_camera(&mut self, gl: &crate::GlContext, camera: &Camera3D) {
		let view_projection_matrix = camera.get_view_projection_matrix();
		self.begin_frame(gl, &view_projection_matrix);
	}

	//==============================================================================
	/// 보기와 투영 행렬로 프레임 시작. (Camera3D 없이, 투영 x 보기로 begin_frame)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `view_matrix`: 보기 행렬.
	/// - `projection_matrix`: 투영 행렬.
	//==============================================================================
	pub fn begin_frame_with_matrices(&mut self, gl: &crate::GlContext, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		let mut view_projection_matrix = *projection_matrix;
		view_projection_matrix.multiply(view_matrix);
		self.begin_frame(gl, &view_projection_matrix);
	}

	//==============================================================================
	/// 이번 프레임의 카메라 반환. (보기와 투영에서 읽은 것)
	///
	/// # 돌려줌
	/// 이번 프레임의 카메라. (position [x, y, z, w(1 원근, 0 직교)], forward, is_mirrored)
	//==============================================================================
	pub fn get_frame_camera(&self) -> ToonFrameCamera {
		ToonFrameCamera { position: self.camera_position, forward: self.camera_forward, is_mirrored: self.is_frame_mirrored }
	}

	//==============================================================================
	/// 그릴 화면 사각형 설정. (뷰포트 픽셀, 왼쪽 위 원점, 다음 begin_frame 부터)
	///
	/// # 인자
	/// - `x`: 왼쪽 끝. (뷰포트 픽셀)
	/// - `y`: 위쪽 끝. (뷰포트 픽셀, 위가 0)
	/// - `width`: 너비. (픽셀)
	/// - `height`: 높이. (픽셀)
	//==============================================================================
	pub fn set_screen_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
		self.screen_rect = Some(Rect::create(x, y, width.max(0.0), height.max(0.0)));
	}

	//==============================================================================
	/// 그릴 화면 사각형 풀기. (뷰포트 전체에 그림)
	//==============================================================================
	pub fn clear_screen_rect(&mut self) {
		self.screen_rect = None;
	}

	//==============================================================================
	/// 그릴 화면 사각형 반환. (없으면 None)
	///
	/// # 돌려줌
	/// 그릴 화면 사각형. (없으면 None)
	//==============================================================================
	pub fn get_screen_rect(&self) -> Option<Rect> {
		self.screen_rect
	}

	//==============================================================================
	/// 화면 사각형 걸기. (begin_frame 이 상태를 적은 뒤, 클립 x, y 에 사각형의 비를 곱하고 자리를 더한 뒤 가위를 켠다)
	//==============================================================================
	fn apply_screen_rect(&mut self, gl: &crate::GlContext) {
		let (screen_rect, viewport) = match (self.screen_rect, self.saved_state.as_ref()) {
			(Some(screen_rect), Some(saved_state)) => {
				(screen_rect, saved_state.viewport)
			}
			_ => {
				return;
			}
		};
		let viewport_width = viewport[2].max(1) as f64;
		let viewport_height = viewport[3].max(1) as f64;
		let scale_x = screen_rect.size.x / viewport_width;
		let scale_y = screen_rect.size.y / viewport_height;
		let offset_x = (screen_rect.position.x * 2.0 + screen_rect.size.x) / viewport_width - 1.0;
		let offset_y = 1.0 - (screen_rect.position.y * 2.0 + screen_rect.size.y) / viewport_height;
		let elements = &mut self.view_projection_matrix.elements;
		for column_index in 0..4 {
			let clip_w = elements[column_index * 4 + 3] as f64;
			elements[column_index * 4] = (elements[column_index * 4] as f64 * scale_x + clip_w * offset_x) as f32;
			elements[column_index * 4 + 1] = (elements[column_index * 4 + 1] as f64 * scale_y + clip_w * offset_y) as f32;
		}
		let scissor_left = screen_rect.position.x.floor() as i32;
		let scissor_bottom = (viewport_height - screen_rect.position.y - screen_rect.size.y).floor() as i32;
		let scissor_right = (screen_rect.position.x + screen_rect.size.x).ceil() as i32;
		let scissor_top = (viewport_height - screen_rect.position.y).ceil() as i32;
		unsafe {
			crate::core::glstatecache::enable(gl, glow::SCISSOR_TEST);
			crate::core::glstatecache::scissor(gl, viewport[0] + scissor_left, viewport[1] + scissor_bottom, scissor_right - scissor_left, scissor_top - scissor_bottom);
		}
		self.is_scissor_applied = true;
	}

	//==============================================================================
	/// 화면 사각형의 가위 풀기. (apply_screen_rect 가 켰을 때만)
	//==============================================================================
	fn release_screen_rect(&mut self, gl: &crate::GlContext) {
		if !self.is_scissor_applied {
			return;
		}
		unsafe {
			crate::core::glstatecache::disable(gl, glow::SCISSOR_TEST);
		}
		self.is_scissor_applied = false;
	}

	//==============================================================================
	/// 그림자 깊이 패스 시작. (ShadowMap 의 프레임버퍼에 빛의 보기와 투영으로, 앞의 프레임버퍼와 뷰포트는 end_shadow_pass 가 돌린다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shadow_map`: ShadowMap. (깊이를 그릴 곳, update_light_matrix 로 빛 행렬을 정해 둔다)
	//==============================================================================
	pub fn begin_shadow_pass(&mut self, gl: &crate::GlContext, shadow_map: &ShadowMap) {
		self.forget_all_uniform_values();
		self.save_state(gl);
		self.view_projection_matrix = *shadow_map.get_light_view_projection_matrix();
		self.frustum_planes = extract_frustum_planes(&self.view_projection_matrix);
		self.is_shadow_pass = true;
		self.is_frame_mirrored = false;
		self.frame_serial += 1;
		shadow_map.begin_render(gl);
		unsafe {
			crate::core::glstatecache::disable(gl, glow::BLEND);
			crate::core::glstatecache::depth_func(gl, glow::LEQUAL);
			crate::core::glstatecache::enable(gl, glow::CULL_FACE);
			crate::core::glstatecache::cull_face(gl, glow::BACK);
		}
	}

	//==============================================================================
	/// 그림자 아틀라스 깊이 패스 시작. (아틀라스를 묶어 비우고 앞의 상태를 적어 둔다, 칸마다 set_shadow_atlas_tile 뒤에 draw_mesh 가 깊이만 그린다, 끝은 end_shadow_pass)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shadow_atlas`: ShadowAtlas.
	//==============================================================================
	pub fn begin_shadow_atlas_pass(&mut self, gl: &crate::GlContext, shadow_atlas: &ShadowAtlas) {
		self.forget_all_uniform_values();
		self.save_state(gl);
		self.is_shadow_pass = true;
		self.is_frame_mirrored = false;
		self.frame_serial += 1;
		shadow_atlas.begin_render(gl);
		unsafe {
			crate::core::glstatecache::disable(gl, glow::BLEND);
			crate::core::glstatecache::depth_func(gl, glow::LEQUAL);
			crate::core::glstatecache::enable(gl, glow::CULL_FACE);
			crate::core::glstatecache::cull_face(gl, glow::BACK);
		}
	}

	//==============================================================================
	/// 그림자 아틀라스의 칸 고르기. (그 칸의 보기와 투영과 절두체, 뷰포트를 건다, begin_shadow_atlas_pass 와 end_shadow_pass 사이)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shadow_atlas`: ShadowAtlas.
	/// - `tile_index`: 칸 번호.
	//==============================================================================
	pub fn set_shadow_atlas_tile(&mut self, gl: &crate::GlContext, shadow_atlas: &ShadowAtlas, tile_index: usize) {
		self.view_projection_matrix = shadow_atlas.get_tile(tile_index).view_projection_matrix;
		self.frustum_planes = extract_frustum_planes(&self.view_projection_matrix);
		self.frame_serial += 1;
		shadow_atlas.begin_tile(gl, tile_index);
	}

	//==============================================================================
	/// 그림자 깊이 패스 끝. (프레임버퍼와 뷰포트와 상태를 돌린다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn end_shadow_pass(&mut self, gl: &crate::GlContext) {
		self.is_shadow_pass = false;
		self.frame_serial += 1;
		self.restore_state(gl, true);
	}

	//==============================================================================
	/// 경계 구가 절두체 안에 드는지. (모형 행렬로 옮긴 가운데, 가장 큰 크기를 곱한 반지름)
	///
	/// # 인자
	/// - `mesh`: create_mesh 의 결과.
	/// - `model_matrix`: 모형 행렬.
	///
	/// # 돌려줌
	/// 절두체 안에 들면 true.
	//==============================================================================
	pub fn is_visible(&self, mesh: &ToonMesh, model_matrix: &Matrix4) -> bool {
		let center = mesh.bounding_center;
		let transformed_center = model_matrix.transform_point(center[0], center[1], center[2]);
		let radius = mesh.bounding_radius * read_maximum_scale(model_matrix);
		self.frustum_planes.iter().all(|plane| plane[0] * transformed_center.x + plane[1] * transformed_center.y + plane[2] * transformed_center.z + plane[3] >= -radius)
	}

	//==============================================================================
	/// 모형 하나 그리기. (그림자 패스면 깊이만, 화면 밖이면 건너뛴다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `mesh`: create_mesh 의 결과.
	/// - `model_matrix`: 모형 행렬.
	/// - `options`: 그리기 꼴. (ToonDrawOptions, 머리말)
	//==============================================================================
	pub fn draw_mesh(&mut self, gl: &crate::GlContext, mesh: &ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions) {
		if options.is_culled && !self.is_visible(mesh, model_matrix) {
			self.frame_statistics.culled_object_count += 1;
			return;
		}
		self.frame_statistics.drawn_object_count += 1;
		self.draw_with_count(gl, mesh, model_matrix, 0, options, None);
	}

	//==============================================================================
	/// 같은 모형 여럿을 한 번에 그리기. (GPU 인스턴싱, 화면 밖 인스턴스는 걸러 남은 것만 올린다)
	/// - 뼈대 모형은 모든 인스턴스가 같은 자세다. (options 의 joint_matrices)
	/// - 감기는 첫 인스턴스의 행렬로 가린다. (is_mirrored 를 주면 그것)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `mesh`: create_mesh 의 결과.
	/// - `instance_list`: 인스턴스마다의 모형 행렬, 틴트, 빛남. (틴트는 곱하는 sRGB 와 알파, 빛남은 더하는 sRGB 와 선형 배, 모형 행렬은 아핀으로 읽는다)
	/// - `options`: draw_mesh 와 같다. (모든 인스턴스에 같게 건다)
	///
	/// # 예제
	/// 인스턴스 하나만 빛남을 더해 밝히기
	/// ```rust,no_run
	/// use vanilla::{create_octahedron_geometry, Matrix4, MeshBuilder, ToonDrawOptions, ToonInstance, ToonPartOptions, ToonRenderer, Vector3};
	///
	/// fn draw_glowing_rock(gl: &vanilla::GlContext) -> Result<(), String> {
	/// 	let mut renderer = ToonRenderer::new(gl)?;
	/// 	let mut builder = MeshBuilder::new();
	/// 	builder.add_part(&create_octahedron_geometry(0.3), &Matrix4::create_identity(), "#909098", ToonPartOptions { has_outline: true, ..ToonPartOptions::default() });
	/// 	let rock_mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("mesh failed")?;
	/// 	let mut instance_list: Vec<ToonInstance> = (0..3).map(|index| ToonInstance { model_matrix: Matrix4::create_translation(index as f64 - 1.0, 0.0, 0.0), ..ToonInstance::default() }).collect();
	/// 	instance_list[1].emissive_color = Some([1.0, 0.85, 0.3]);
	/// 	instance_list[1].emissive_intensity = 0.35;
	/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 2.0, 3.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
	/// 	renderer.begin_frame_with_matrices(gl, &view_matrix, &Matrix4::create_perspective(0.8, 1.0, 0.1, 30.0));
	/// 	renderer.draw_mesh_instanced(gl, &rock_mesh, &instance_list, ToonDrawOptions::default());
	/// 	renderer.end_frame(gl);
	/// 	Ok(())
	/// }
	/// ```
	//==============================================================================
	pub fn draw_mesh_instanced(&mut self, gl: &crate::GlContext, mesh: &ToonMesh, instance_list: &[ToonInstance], options: ToonDrawOptions) {
		let mut instance_data = std::mem::take(&mut self.instance_data);
		instance_data.clear();
		let mut instance_count = 0;
		let mut first_matrix = Matrix4::create_identity();
		for instance in instance_list {
			if options.is_culled && !self.is_visible(mesh, &instance.model_matrix) {
				self.frame_statistics.culled_object_count += 1;
				continue;
			}
			if instance_count == 0 {
				first_matrix = instance.model_matrix;
			}
			let offset = instance_data.len();
			instance_data.extend_from_slice(&instance.model_matrix.elements);
			let emissive_color = instance.emissive_color.unwrap_or([0.0, 0.0, 0.0]);
			instance_data[offset + 3] = emissive_color[0] as f32;
			instance_data[offset + 7] = emissive_color[1] as f32;
			instance_data[offset + 11] = emissive_color[2] as f32;
			instance_data[offset + 15] = if instance.emissive_color.is_some() { instance.emissive_intensity as f32 } else { 1.0 };
			instance_data.extend_from_slice(&instance.tint_color.map(|value| value as f32));
			instance_count += 1;
		}
		self.frame_statistics.drawn_object_count += instance_count;
		if instance_count > 0 {
			unsafe {
				crate::core::glstatecache::bind_buffer(gl, glow::ARRAY_BUFFER, Some(mesh.instance_buffer));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&instance_data), glow::DYNAMIC_DRAW);
			}
			self.draw_with_count(gl, mesh, &first_matrix, instance_count as i32, options, None);
		}
		self.instance_data = instance_data;
	}

	//==============================================================================
	/// 영구 인스턴스 버퍼로 그리기. (거르지 않음, instance_count 개, 꼴에 행렬이 없으면 정점 조각이 자리를 정한다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `mesh`: create_mesh 의 결과.
	/// - `instance_buffer`: create_instance_buffer 의 결과.
	/// - `instance_count`: 그릴 인스턴스 수. (버퍼에 든 수를 넘으면 자른다)
	/// - `options`: draw_mesh 와 같다. (material 의 정점 조각이 사용자 어트리뷰트를 읽는다)
	//==============================================================================
	pub fn draw_mesh_instance_buffer(&mut self, gl: &crate::GlContext, mesh: &ToonMesh, instance_buffer: &ToonInstanceBuffer, instance_count: usize, options: ToonDrawOptions) {
		let count = instance_count.min(instance_buffer.get_instance_count());
		if count == 0 {
			return;
		}
		self.frame_statistics.drawn_object_count += count as u32;
		self.draw_with_count(gl, mesh, &Matrix4::create_identity(), count as i32, options, Some(instance_buffer));
	}

	//==============================================================================
	/// 모형 하나의 바깥선만 그리기. (거르지도 세지도 않는다, 껍데기가 없거나 has_outline 이 false 거나 그림자 패스면 그리지 않는다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `mesh`: create_mesh 의 결과.
	/// - `model_matrix`: 모형 행렬.
	/// - `options`: draw_mesh 와 같다.
	//==============================================================================
	pub fn draw_mesh_outline(&mut self, gl: &crate::GlContext, mesh: &ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions) {
		if mesh.outline_vertex_array.is_none() || !options.has_outline || self.is_shadow_pass {
			return;
		}
		self.draw_with_count(gl, mesh, model_matrix, 0, ToonDrawOptions { is_body_drawn: false, ..options }, None);
	}

	//==============================================================================
	/// 프로그램의 프레임 값 올리기. (보기와 투영, 카메라, 보기 돌림, HDR, 그림자, 껍데기 거리 배, 이번 프레임에 이미 올렸으면 건너뛴다)
	//==============================================================================
	fn upload_frame_uniforms(&self, gl: &crate::GlContext, entry: &ToonProgramEntry) {
		if entry.frame_serial.get() == self.frame_serial {
			return;
		}
		entry.frame_serial.set(self.frame_serial);
		let program = &entry.program;
		let hdr_output = if self.is_hdr_output { 1.0 } else { 0.0 };
		unsafe {
			gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &self.view_projection_matrix.elements);
			match entry.kind {
				ToonProgramKind::Outline => {
					let camera_position = self.camera_position;
					let outline_distance_scale = self.outline_distance_scale;
					gl.uniform_4_f64(program.get_uniform_location(gl, "cameraPosition").as_ref(), camera_position[0], camera_position[1], camera_position[2], camera_position[3]);
					gl.uniform_3_f64(program.get_uniform_location(gl, "outlineDistanceScale").as_ref(), outline_distance_scale[0], outline_distance_scale[1], outline_distance_scale[2]);
					gl.uniform_2_f64(program.get_uniform_location(gl, "outlineFadeDistance").as_ref(), self.outline_fade_distance[0], self.outline_fade_distance[1]);
					gl.uniform_3_f64(program.get_uniform_location(gl, "cameraForward").as_ref(), self.camera_forward[0], self.camera_forward[1], self.camera_forward[2]);
					gl.uniform_1_f64(program.get_uniform_location(gl, "hdrOutput").as_ref(), hdr_output);
				}
				ToonProgramKind::Solid => {
					let camera_position = self.camera_position;
					let camera_forward = self.camera_forward;
					gl.uniform_4_f64(program.get_uniform_location(gl, "cameraPosition").as_ref(), camera_position[0], camera_position[1], camera_position[2], camera_position[3]);
					gl.uniform_3_f64(program.get_uniform_location(gl, "cameraForward").as_ref(), camera_forward[0], camera_forward[1], camera_forward[2]);
					gl.uniform_matrix_3_f32_slice(program.get_uniform_location(gl, "viewRotation").as_ref(), false, &self.view_rotation);
					gl.uniform_2_f64(program.get_uniform_location(gl, "cameraRoll").as_ref(), self.camera_roll.cos(), self.camera_roll.sin());
					gl.uniform_1_f64(program.get_uniform_location(gl, "hdrOutput").as_ref(), hdr_output);
					match self.shadow_input {
						Some(shadow_input) => {
							gl.uniform_1_f64(program.get_uniform_location(gl, "shadowEnabled").as_ref(), 1.0);
							gl.uniform_1_f64(program.get_uniform_location(gl, "shadowStrength").as_ref(), shadow_input.strength);
							gl.uniform_1_f64(program.get_uniform_location(gl, "shadowTexelSize").as_ref(), 1.0 / shadow_input.resolution.max(1) as f64);
							gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &shadow_input.light_view_projection_matrix.elements);
						}
						None => {
							gl.uniform_1_f64(program.get_uniform_location(gl, "shadowEnabled").as_ref(), 0.0);
						}
					}
					self.upload_local_shadow_uniforms(gl, program);
				}
				ToonProgramKind::Depth => {}
			}
		}
	}

	//==============================================================================
	/// 점광과 스포트라이트 그림자 올리기. (아틀라스의 빛마다 자리, 바탕, 거리, 첫 칸과 점광마다의 칸 번호 + 1)
	//==============================================================================
	fn upload_local_shadow_uniforms(&self, gl: &crate::GlContext, program: &ShaderProgram) {
		let mut slot_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS];
		let mut position_values = [0.0f32; SHADOW_ATLAS_MAXIMUM_LIGHTS * 4];
		let mut forward_values = [0.0f32; SHADOW_ATLAS_MAXIMUM_LIGHTS * 4];
		let mut up_values = [0.0f32; SHADOW_ATLAS_MAXIMUM_LIGHTS * 4];
		let mut tile_values = [0.0f32; SHADOW_ATLAS_MAXIMUM_LIGHTS];
		unsafe {
			if let Some(local_shadow_input) = self.local_shadow_input.as_ref() {
				for (slot, light) in local_shadow_input.light_list.iter().take(SHADOW_ATLAS_MAXIMUM_LIGHTS).enumerate() {
					if light.light_index < TOON_MAXIMUM_POINT_LIGHTS {
						slot_values[light.light_index] = ((slot + 1) as f64) as f32;
					}
					position_values[slot * 4..slot * 4 + 4].copy_from_slice(&[(light.position[0]) as f32, (light.position[1]) as f32, (light.position[2]) as f32, light.near as f32]);
					forward_values[slot * 4..slot * 4 + 4].copy_from_slice(&[(light.forward[0]) as f32, (light.forward[1]) as f32, (light.forward[2]) as f32, light.far as f32]);
					up_values[slot * 4..slot * 4 + 4].copy_from_slice(&[(light.up[0]) as f32, (light.up[1]) as f32, (light.up[2]) as f32, light.tangent as f32]);
					tile_values[slot] = (light.first_tile as f64) as f32;
				}
				gl.uniform_4_f64(program.get_uniform_location(gl, "localShadowAtlas").as_ref(), local_shadow_input.grid_size, local_shadow_input.tile_size, local_shadow_input.texel_size, local_shadow_input.strength);
			}
			gl.uniform_1_f32_slice(program.get_uniform_location(gl, "pointLightShadowSlots").as_ref(), &slot_values);
			gl.uniform_4_f32_slice(program.get_uniform_location(gl, "localShadowPositions").as_ref(), &position_values);
			gl.uniform_4_f32_slice(program.get_uniform_location(gl, "localShadowForwards").as_ref(), &forward_values);
			gl.uniform_4_f32_slice(program.get_uniform_location(gl, "localShadowUps").as_ref(), &up_values);
			gl.uniform_1_f32_slice(program.get_uniform_location(gl, "localShadowTiles").as_ref(), &tile_values);
		}
	}

	//==============================================================================
	/// 몸 프로그램의 빛 묶음 올리기. (같은 묶음, 같은 판이면 건너뛴다)
	//==============================================================================
	fn upload_light_uniforms(gl: &crate::GlContext, entry: &ToonProgramEntry, light_set: &ToonLightSet) {
		if entry.light_set_id.get() == light_set.get_id() && entry.light_version.get() == light_set.get_version() {
			return;
		}
		entry.light_set_id.set(light_set.get_id());
		entry.light_version.set(light_set.get_version());
		let program = &entry.program;
		let sun_direction = light_set.get_sun_direction();
		let sun_color = light_set.get_sun_color();
		let sky_color = light_set.get_sky_color();
		let ground_color = light_set.get_ground_color();
		let (fill_direction, fill_color) = light_set.get_fill_light();
		let rim_light = light_set.get_rim_light();
		let specular = light_set.get_specular();
		let shade_ramp = light_set.get_shade_ramp();
		let mut shade_position_values = [0.0f32; TOON_MAXIMUM_BAND_STEPS];
		let mut shade_feather_values = [0.0f32; TOON_MAXIMUM_BAND_STEPS];
		let mut shade_color_values = [0.0f32; TOON_MAXIMUM_BAND_STEPS * 3];
		let shade_stop_count = shade_ramp.positions.len().min(TOON_MAXIMUM_BAND_STEPS);
		for stop_index in 0..shade_stop_count {
			shade_position_values[stop_index] = (shade_ramp.positions[stop_index]) as f32;
			shade_feather_values[stop_index] = (shade_ramp.feathers[stop_index]) as f32;
			shade_color_values[stop_index * 3..stop_index * 3 + 3].copy_from_slice(&shade_ramp.colors[stop_index].map(|value| value as f32));
		}
		let point_light_count = light_set.get_point_light_count();
		let mut position_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 4];
		let mut color_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 3];
		let mut shape_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 3];
		let mut spot_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 4];
		let mut cone_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 2];
		for (light_index, point_light) in light_set.get_point_lights().iter().take(point_light_count).enumerate() {
			position_values[light_index * 4..light_index * 4 + 4].copy_from_slice(&[(point_light.position[0]) as f32, (point_light.position[1]) as f32, (point_light.position[2]) as f32, point_light.range as f32]);
			color_values[light_index * 3..light_index * 3 + 3].copy_from_slice(&[(point_light.color[0] * point_light.intensity) as f32, (point_light.color[1] * point_light.intensity) as f32, (point_light.color[2] * point_light.intensity) as f32]);
			shape_values[light_index * 3..light_index * 3 + 3].copy_from_slice(&[point_light.decay as f32, point_light.range_exponent as f32, point_light.wrap as f32]);
			if let Some(direction) = point_light.direction {
				let (cone_scale, cone_offset) = read_spot_cone(point_light.inner_cone_angle, point_light.outer_cone_angle);
				spot_values[light_index * 4..light_index * 4 + 4].copy_from_slice(&[(direction[0]) as f32, (direction[1]) as f32, (direction[2]) as f32, 1.0]);
				cone_values[light_index * 2..light_index * 2 + 2].copy_from_slice(&[cone_scale as f32, cone_offset as f32]);
			}
		}
		unsafe {
			gl.uniform_3_f64(program.get_uniform_location(gl, "sunDirection").as_ref(), sun_direction[0], sun_direction[1], sun_direction[2]);
			gl.uniform_3_f64(program.get_uniform_location(gl, "sunColor").as_ref(), sun_color[0], sun_color[1], sun_color[2]);
			gl.uniform_3_f64(program.get_uniform_location(gl, "skyColor").as_ref(), sky_color[0], sky_color[1], sky_color[2]);
			gl.uniform_3_f64(program.get_uniform_location(gl, "groundColor").as_ref(), ground_color[0], ground_color[1], ground_color[2]);
			gl.uniform_3_f64(program.get_uniform_location(gl, "fillDirection").as_ref(), fill_direction[0], fill_direction[1], fill_direction[2]);
			gl.uniform_3_f64(program.get_uniform_location(gl, "fillColor").as_ref(), fill_color[0], fill_color[1], fill_color[2]);
			gl.uniform_4_f64(program.get_uniform_location(gl, "rimLight").as_ref(), rim_light.color[0], rim_light.color[1], rim_light.color[2], rim_light.power);
			gl.uniform_2_f64(program.get_uniform_location(gl, "rimShape").as_ref(), rim_light.feather, rim_light.light_mask);
			let shade_rim_light = light_set.get_shade_rim_light();
			gl.uniform_4_f64(program.get_uniform_location(gl, "shadeRimLight").as_ref(), shade_rim_light.color[0], shade_rim_light.color[1], shade_rim_light.color[2], shade_rim_light.power);
			let light_color_usage = light_set.get_light_color_usage();
			let read_usage = |is_used: bool| if is_used { 1.0 } else { 0.0 };
			gl.uniform_4_f64(program.get_uniform_location(gl, "lightColorUsage").as_ref(), read_usage(light_color_usage.base), read_usage(light_color_usage.shade), read_usage(light_color_usage.highlight), read_usage(light_color_usage.rim));
			gl.uniform_3_f64(program.get_uniform_location(gl, "lightColorUsageExtra").as_ref(), read_usage(light_color_usage.shade_rim), read_usage(light_color_usage.matcap), read_usage(light_color_usage.angel_ring));
			gl.uniform_3_f64(program.get_uniform_location(gl, "specularLight").as_ref(), specular.strength, specular.exponent, specular.feather);
			gl.uniform_3_f64(program.get_uniform_location(gl, "specularColor").as_ref(), specular.color[0], specular.color[1], specular.color[2]);
			gl.uniform_1_f64(program.get_uniform_location(gl, "energyNormalized").as_ref(), if light_set.get_light_model().is_energy_normalized { 1.0 } else { 0.0 });
			gl.uniform_1_f64(program.get_uniform_location(gl, "lightIntensityLimit").as_ref(), if light_set.is_light_intensity_limited() { 1.0 } else { 0.0 });
			gl.uniform_1_i32(program.get_uniform_location(gl, "shadeStopCount").as_ref(), shade_stop_count as i32);
			gl.uniform_1_f32_slice(program.get_uniform_location(gl, "shadeStopPositions").as_ref(), &shade_position_values);
			gl.uniform_1_f32_slice(program.get_uniform_location(gl, "shadeStopFeathers").as_ref(), &shade_feather_values);
			gl.uniform_3_f32_slice(program.get_uniform_location(gl, "shadeStopColors").as_ref(), &shade_color_values);
			gl.uniform_1_f64(program.get_uniform_location(gl, "shadeHalfLambert").as_ref(), if shade_ramp.is_half_lambert { 1.0 } else { 0.0 });
			gl.uniform_1_f64(program.get_uniform_location(gl, "rampTextureEnabled").as_ref(), if light_set.get_ramp_texture().is_some() { 1.0 } else { 0.0 });
			gl.uniform_1_i32(program.get_uniform_location(gl, "pointLightCount").as_ref(), point_light_count as i32);
			gl.uniform_4_f32_slice(program.get_uniform_location(gl, "pointLightPositions").as_ref(), &position_values);
			gl.uniform_3_f32_slice(program.get_uniform_location(gl, "pointLightColors").as_ref(), &color_values);
			gl.uniform_3_f32_slice(program.get_uniform_location(gl, "pointLightShapes").as_ref(), &shape_values);
			gl.uniform_4_f32_slice(program.get_uniform_location(gl, "pointLightSpots").as_ref(), &spot_values);
			gl.uniform_2_f32_slice(program.get_uniform_location(gl, "pointLightCones").as_ref(), &cone_values);
		}
	}

	//==============================================================================
	/// 그리기마다의 공용 유니폼 올리기. (모형 행렬, 뼈대, 인스턴싱 0 아님, 1 인스턴스 행렬과 틴트, 2 인스턴스 틴트만)
	//==============================================================================
	fn upload_draw_uniforms(&self, gl: &crate::GlContext, program: &ShaderProgram, model_matrix: &Matrix4, is_skinned: bool, instanced_mode: f64, joint_matrices: Option<&[f32]>) {
		self.write_uniform_matrix4(gl, program, "modelMatrix", &model_matrix.elements);
		self.write_uniform_floats(gl, program, "instanced", 1, [instanced_mode, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, program, "skinned", 1, [if is_skinned { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]);
		if let (true, Some(joint_matrices)) = (is_skinned, joint_matrices) {
			let joint_float_count = joint_matrices.len().min(TOON_MAXIMUM_JOINTS * 16) / 16 * 16;
			self.write_uniform_matrix4(gl, program, "jointMatrices", &joint_matrices[..joint_float_count]);
		}
	}

	//==============================================================================
	/// 텍스처 유닛에 묶기. (없으면 흰 텍스처)
	//==============================================================================
	fn bind_texture_unit(&self, gl: &crate::GlContext, unit: u32, texture: Option<GlTexture>) {
		ToonRenderer::bind_texture_2d(gl, unit, texture.unwrap_or(self.white_texture));
	}

	//==============================================================================
	/// 텍스처 유닛에 2D 텍스처 묶기. (원본 bindTexture2D, 상태 캐시가 이미 묶였다고 기억하면 건너뛰고, 유닛도 다를 때만 고른다)
	//==============================================================================
	fn bind_texture_2d(gl: &crate::GlContext, unit: u32, texture: GlTexture) {
		let texture_unit = glow::TEXTURE0 + unit;
		if crate::core::glstatecache::is_texture_bound(gl, texture_unit, glow::TEXTURE_2D, Some(texture)) {
			return;
		}
		unsafe {
			if !crate::core::glstatecache::is_active_texture(gl, texture_unit) {
				crate::core::glstatecache::active_texture(gl, texture_unit);
			}
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(texture));
		}
	}

	//==============================================================================
	/// 텍스처 유닛 고르기. (원본 applyActiveTexture, 상태 캐시가 이미 같은 유닛이라고 기억하면 건너뛴다)
	//==============================================================================
	fn apply_active_texture(gl: &crate::GlContext, texture_unit: u32) {
		if !crate::core::glstatecache::is_active_texture(gl, texture_unit) {
			unsafe {
				crate::core::glstatecache::active_texture(gl, texture_unit);
			}
		}
	}

	//==============================================================================
	/// 앞면 감김 방향 걸기. (원본 applyFrontFace, 상태 캐시가 같은 값이라고 기억하면 건너뛴다)
	//==============================================================================
	fn apply_front_face(gl: &crate::GlContext, mode: u32) {
		if !crate::core::glstatecache::is_front_face(gl, mode) {
			unsafe {
				crate::core::glstatecache::front_face(gl, mode);
			}
		}
	}

	//==============================================================================
	/// 가릴 면 걸기. (원본 applyCullFace, 상태 캐시가 같은 값이라고 기억하면 건너뛴다)
	//==============================================================================
	fn apply_cull_face(gl: &crate::GlContext, mode: u32) {
		if !crate::core::glstatecache::is_cull_face(gl, mode) {
			unsafe {
				crate::core::glstatecache::cull_face(gl, mode);
			}
		}
	}

	//==============================================================================
	/// 버텍스 어레이 묶기. (원본 applyVertexArray, 상태 캐시가 같은 것이라고 기억하면 건너뛴다)
	//==============================================================================
	fn apply_vertex_array(gl: &crate::GlContext, vertex_array: GlVertexArray) {
		if !crate::core::glstatecache::is_vertex_array_bound(gl, Some(vertex_array)) {
			unsafe {
				crate::core::glstatecache::bind_vertex_array(gl, Some(vertex_array));
			}
		}
	}

	//==============================================================================
	/// 유니폼 값 기억하며 올리기. (원본 readUniformSlot, 프로그램마다 이름별로 마지막에 올린 값을 기억해 같은 값이면 GL 에 다시 올리지 않는다)
	/// - 유니폼 값은 프로그램에 남으므로 프로그램이 같은 동안 같은 값은 다시 올릴 필요가 없다. (three.js 의 WebGLUniforms 캐시와 같은 꼴)
	/// - 렌더러 밖의 코드가 같은 유니폼을 바꿨을 수 있는 때는 기억을 잊는다:
	///   프레임과 그림자 판의 시작, apply_frame_state(단계 콜백과 화면 공간 바깥선 뒤), 상태 되돌리기, 그 프로그램의 on_before_draw 를 부른 뒤.
	/// - 값은 주어진 수 그대로 견준다. (0 과 -0 도 가른다, NaN 은 늘 다시 올린다) 셰이더에서 빠진 유니폼(위치 None)은 올리지 않는다.
	//==============================================================================
	fn with_uniform_slot<R>(&self, gl: &crate::GlContext, program: &ShaderProgram, uniform_name: &str, callback: impl FnOnce(&mut ToonUniformSlot, u64) -> R) -> R {
		let mut uniform_slot_table = self.uniform_slot_table.borrow_mut();
		let slot_table = uniform_slot_table.entry(program.get_program()).or_default();
		if !slot_table.contains_key(uniform_name) {
			slot_table.insert(uniform_name.to_string(), ToonUniformSlot { location: program.get_uniform_location(gl, uniform_name), serial: 0, values: Vec::new() });
		}
		callback(slot_table.get_mut(uniform_name).unwrap(), self.uniform_serial.get())
	}

	//==============================================================================
	/// 모든 프로그램의 기억한 유니폼 값 잊기. (원본 forgetAllUniformValues, 다음 그리기가 모두 다시 올린다)
	//==============================================================================
	fn forget_all_uniform_values(&self) {
		self.uniform_serial.set(self.uniform_serial.get() + 1);
	}

	//==============================================================================
	/// 한 프로그램의 기억한 유니폼 값 잊기. (원본 forgetUniformValues, on_before_draw 뒤)
	//==============================================================================
	fn forget_uniform_values(&self, program: &ShaderProgram) {
		if let Some(slot_table) = self.uniform_slot_table.borrow_mut().get_mut(&program.get_program()) {
			for slot in slot_table.values_mut() {
				slot.serial = 0;
			}
		}
	}

	//==============================================================================
	/// float 유니폼 하나 올리기. (원본 writeUniformFloats, uniform1f ~ uniform4f, 같은 값이면 건너뛴다)
	//==============================================================================
	fn write_uniform_floats(&self, gl: &crate::GlContext, program: &ShaderProgram, uniform_name: &str, component_count: usize, values: [f64; 4]) {
		self.with_uniform_slot(gl, program, uniform_name, |slot, uniform_serial| {
			let Some(location) = slot.location.as_ref() else {
				return;
			};
			if slot.serial == uniform_serial && slot.values.len() == 4 && (0..4).all(|index| is_same_number(slot.values[index], values[index])) {
				return;
			}
			slot.values.clear();
			slot.values.extend_from_slice(&values);
			slot.serial = uniform_serial;
			unsafe {
				match component_count {
					1 => gl.uniform_1_f64(Some(location), values[0]),
					2 => gl.uniform_2_f64(Some(location), values[0], values[1]),
					3 => gl.uniform_3_f64(Some(location), values[0], values[1], values[2]),
					_ => gl.uniform_4_f64(Some(location), values[0], values[1], values[2], values[3]),
				}
			}
		});
	}

	//==============================================================================
	/// int 유니폼 하나 올리기. (원본 writeUniform1i, 같은 값이면 건너뛴다)
	//==============================================================================
	fn write_uniform_1i(&self, gl: &crate::GlContext, program: &ShaderProgram, uniform_name: &str, value: i32) {
		self.with_uniform_slot(gl, program, uniform_name, |slot, uniform_serial| {
			let Some(location) = slot.location.as_ref() else {
				return;
			};
			if slot.serial == uniform_serial && slot.values.len() == 1 && slot.values[0] == value as f64 {
				return;
			}
			slot.values.clear();
			slot.values.push(value as f64);
			slot.serial = uniform_serial;
			unsafe {
				gl.uniform_1_i32(Some(location), value);
			}
		});
	}

	//==============================================================================
	/// mat4 유니폼이나 그 배열 올리기. (원본 writeUniformMatrix4, uniformMatrix4fv, 전치 없음, 같은 값이면 건너뛴다)
	//==============================================================================
	fn write_uniform_matrix4(&self, gl: &crate::GlContext, program: &ShaderProgram, uniform_name: &str, elements: &[f32]) {
		self.with_uniform_slot(gl, program, uniform_name, |slot, uniform_serial| {
			let Some(location) = slot.location.as_ref() else {
				return;
			};
			if slot.serial == uniform_serial && slot.values.len() == elements.len() && slot.values.iter().zip(elements).all(|(&value, &element)| is_same_number(value, element as f64)) {
				return;
			}
			slot.values.clear();
			slot.values.extend(elements.iter().map(|&element| element as f64));
			slot.serial = uniform_serial;
			unsafe {
				gl.uniform_matrix_4_f32_slice(Some(location), false, elements);
			}
		});
	}

	//==============================================================================
	/// 그리기 본체. (instance_count 가 0 이면 보통, 아니면 인스턴스로, instance_buffer 가 있으면 그 꼴로)
	//==============================================================================
	fn draw_with_count(&mut self, gl: &crate::GlContext, mesh: &ToonMesh, model_matrix: &Matrix4, instance_count: i32, options: ToonDrawOptions, instance_buffer: Option<&ToonInstanceBuffer>) {
		let instance_layout = instance_buffer.map(|instance_buffer| &instance_buffer.layout);
		if !self.ensure_program_set(gl, options.material, instance_layout) {
			return;
		}
		let is_skinned = mesh.is_skinned && options.joint_matrices.is_some();
		let is_instanced = instance_count > 0;
		let (body_vertex_array, outline_vertex_array, instanced_mode) = match instance_buffer {
			Some(instance_buffer) => {
				let (body_vertex_array, outline_vertex_array) = match self.read_instance_vertex_arrays(gl, mesh, instance_buffer) {
					Some(vertex_arrays) => {
						vertex_arrays
					}
					None => {
						return;
					}
				};
				if !instance_buffer.layout.has_tint() {
					unsafe {
						gl.vertex_attrib_4_f64(INSTANCE_TINT_LOCATION, 1.0, 1.0, 1.0, 1.0);
					}
				}
				(body_vertex_array, outline_vertex_array, if instance_buffer.layout.has_matrix() { 1.0 } else { 2.0 })
			}
			None => {
				(mesh.body_vertex_array, mesh.outline_vertex_array, if is_instanced { 1.0 } else { 0.0 })
			}
		};
		let key = (options.material.map(|material| material.get_id()).unwrap_or(0), instance_layout.map(|instance_layout| instance_layout.get_id()).unwrap_or(0));
		let program_set = match self.program_set_table.get(&key) {
			Some(Some(program_set)) => {
				program_set
			}
			_ => {
				return;
			}
		};
		let body_index_count = mesh.body_index_count as usize;
		let outline_index_count_total = mesh.outline_index_count as usize;
		let index_start = options.index_start.min(body_index_count);
		let index_count = if options.index_count > 0 { options.index_count.min(body_index_count - index_start) } else { body_index_count - index_start };
		let outline_index_start = options.outline_index_start.min(outline_index_count_total);
		let outline_index_count = if options.outline_index_count > 0 { options.outline_index_count.min(outline_index_count_total - outline_index_start) } else { outline_index_count_total - outline_index_start };
		let is_mirrored_draw = options.is_mirrored.unwrap_or_else(|| read_matrix_determinant(model_matrix) < 0.0);
		let is_mirrored = is_mirrored_draw != self.is_frame_mirrored;
		let mut draw_call_count = 0u32;
		let mut draw_elements = |gl: &crate::GlContext, start: usize, count: usize| {
			if count == 0 {
				return;
			}
			unsafe {
				if is_instanced {
					crate::core::glstatecache::draw_elements_instanced(gl, glow::TRIANGLES, count as i32, glow::UNSIGNED_INT, (start * 4) as i32, instance_count);
				}
				else {
					crate::core::glstatecache::draw_elements(gl, glow::TRIANGLES, count as i32, glow::UNSIGNED_INT, (start * 4) as i32);
				}
			}
			draw_call_count += 1;
		};
		ToonRenderer::apply_front_face(gl, if is_mirrored { glow::CW } else { glow::CCW });

		// 그림자 패스. (몸의 깊이만)
		if self.is_shadow_pass {
			let depth_entry = &program_set.depth;
			depth_entry.program.use_program(gl);
			self.upload_frame_uniforms(gl, depth_entry);
			self.upload_draw_uniforms(gl, &depth_entry.program, model_matrix, is_skinned, instanced_mode, options.joint_matrices);
			if let Some(on_before_draw) = options.on_before_draw {
				on_before_draw(&depth_entry.program, gl, ToonProgramKind::Depth);
				self.forget_uniform_values(&depth_entry.program);
			}
			ToonRenderer::apply_vertex_array(gl, body_vertex_array);
			draw_elements(gl, index_start, index_count);
			ToonRenderer::apply_front_face(gl, glow::CCW);
			self.frame_statistics.draw_call_count += draw_call_count;
			return;
		}

		let is_transparent = options.alpha < 1.0 || options.tint_color[3] < 1.0 || options.blend_mode != ToonBlendMode::Alpha;
		let premultiplied_output = if options.blend_mode != ToonBlendMode::Alpha { 1.0 } else { 0.0 };
		unsafe {
			if is_transparent {
				// 알파 칸은 덮은 정도(미리 곱한 알파의 over)로 쌓는다. (원본과 같고, 유니티 Blend SrcAlpha OneMinusSrcAlpha, One OneMinusSrcAlpha 와 같다)
				// 캔버스(알파 없음)에는 색이 같고, 투명하게 비운 판(ToonView3D 렌더 스케일, 투명도 층)에 그려 얹을 때 아래가 맞게 비친다.
				// 더하기는 덮지 않으므로 알파를 바꾸지 않는다.
				crate::core::glstatecache::enable(gl, glow::BLEND);
				match options.blend_mode {
					ToonBlendMode::Additive => {
						crate::core::glstatecache::blend_func_separate(gl, glow::ONE, glow::ONE, glow::ZERO, glow::ONE);
					}
					ToonBlendMode::Premultiplied => {
						crate::core::glstatecache::blend_func(gl, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
					}
					ToonBlendMode::Alpha => {
						crate::core::glstatecache::blend_func_separate(gl, glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
					}
				}
			}
		}

		// 바깥선. (껍데기의 뒷면만)
		if let (true, Some(outline_vertex_array)) = (options.has_outline, outline_vertex_array) {
			let outline_entry = &program_set.outline;
			let outline_program = &outline_entry.program;
			outline_program.use_program(gl);
			self.upload_frame_uniforms(gl, outline_entry);
			self.upload_draw_uniforms(gl, outline_program, model_matrix, is_skinned, instanced_mode, options.joint_matrices);
			let outline_color = options.outline_color.unwrap_or(self.outline_color);
			self.write_uniform_floats(gl, outline_program, "outlineColor", 4, [outline_color[0], outline_color[1], outline_color[2], outline_color[3]]);
			self.write_uniform_floats(gl, outline_program, "outlineWidthScale", 1, [options.outline_width_scale.max(0.0), 0.0, 0.0, 0.0]);
			self.write_uniform_floats(gl, outline_program, "outlineOffsetZ", 1, [options.outline_offset_z, 0.0, 0.0, 0.0]);
			let outline_light_set = options.light_set.unwrap_or(&self.light_set);
			let outline_light_color = if outline_light_set.get_light_color_usage().outline { read_outline_light_color(outline_light_set.get_sun_color()) } else { [1.0; 3] };
			self.write_uniform_floats(gl, outline_program, "outlineLightColor", 3, [outline_light_color[0], outline_light_color[1], outline_light_color[2], 0.0]);
			self.write_uniform_floats(gl, outline_program, "alpha", 1, [options.alpha, 0.0, 0.0, 0.0]);
			self.write_uniform_floats(gl, outline_program, "premultipliedOutput", 1, [premultiplied_output, 0.0, 0.0, 0.0]);
			if let Some(on_before_draw) = options.on_before_draw {
				on_before_draw(outline_program, gl, ToonProgramKind::Outline);
				self.forget_uniform_values(outline_program);
			}
			ToonRenderer::apply_cull_face(gl, glow::FRONT);
			ToonRenderer::apply_vertex_array(gl, outline_vertex_array);
			draw_elements(gl, outline_index_start, outline_index_count);
		}
		if !options.is_body_drawn {
			unsafe {
				if is_transparent {
					crate::core::glstatecache::disable(gl, glow::BLEND);
				}
			}
			ToonRenderer::apply_cull_face(gl, glow::BACK);
			ToonRenderer::apply_front_face(gl, glow::CCW);
			self.frame_statistics.draw_call_count += draw_call_count;
			return;
		}

		// 몸.
		let solid_entry = &program_set.solid;
		let solid_program = &solid_entry.program;
		solid_program.use_program(gl);
		self.upload_frame_uniforms(gl, solid_entry);
		let light_set = options.light_set.unwrap_or(&self.light_set);
		ToonRenderer::upload_light_uniforms(gl, solid_entry, light_set);
		let ramp_texture = light_set.get_ramp_texture();
		self.upload_draw_uniforms(gl, solid_program, model_matrix, is_skinned, instanced_mode, options.joint_matrices);
		let tint_color = options.tint_color;
		let emissive_color = options.emissive_color;
		let decal_color = options.decal_color;
		let uv_transform = options.uv_transform;
		let decal_uv_transform = options.decal_uv_transform;
		let enabled = |texture: Option<GlTexture>| if texture.is_some() { 1.0 } else { 0.0 };
		self.write_uniform_floats(gl, solid_program, "tintColor", 4, [tint_color[0], tint_color[1], tint_color[2], tint_color[3]]);
		self.write_uniform_floats(gl, solid_program, "tintAmount", 1, [options.tint_amount.map(|tint_amount| tint_amount.clamp(0.0, 1.0)).unwrap_or(-1.0), 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "alpha", 1, [options.alpha, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "glowScale", 1, [options.glow_scale, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "colorScale", 1, [options.color_scale, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "emissiveColor", 4, [emissive_color[0], emissive_color[1], emissive_color[2], options.emissive_intensity]);
		self.write_uniform_floats(gl, solid_program, "softEdge", 1, [options.soft_edge, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "alphaCutoff", 1, [options.alpha_cutoff, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "premultipliedOutput", 1, [premultiplied_output, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "textureEnabled", 3, [enabled(options.base_color_texture), enabled(options.emissive_texture), enabled(options.decal_texture), 0.0]);
		self.write_uniform_floats(gl, solid_program, "uvTransform", 4, [uv_transform[0], uv_transform[1], uv_transform[2], uv_transform[3]]);
		self.write_uniform_floats(gl, solid_program, "decalUvTransform", 4, [decal_uv_transform[0], decal_uv_transform[1], decal_uv_transform[2], decal_uv_transform[3]]);
		self.write_uniform_floats(gl, solid_program, "decalColor", 4, [decal_color[0], decal_color[1], decal_color[2], options.decal_emission]);
		self.write_uniform_floats(gl, solid_program, "receivesShadow", 1, [if options.receives_shadow { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "shadeOffset", 1, [options.shade_offset, 0.0, 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "highlightScale", 2, [options.specular_scale.max(0.0), options.rim_scale.max(0.0), 0.0, 0.0]);
		self.write_uniform_floats(gl, solid_program, "matcapSettings", 3, [enabled(options.matcap_texture), options.matcap_strength, if options.matcap_blend == ToonMatcapBlend::Add { 1.0 } else { 0.0 }, 0.0]);
		self.write_uniform_floats(gl, solid_program, "angelRingSettings", 4, [enabled(options.angel_ring_texture), options.angel_ring_offset_u, options.angel_ring_offset_v, if options.is_angel_ring_alpha { 1.0 } else { 0.0 }]);
		self.write_uniform_floats(gl, solid_program, "angelRingColor", 3, [options.angel_ring_color[0], options.angel_ring_color[1], options.angel_ring_color[2], 0.0]);
		ToonRenderer::bind_texture_2d(gl, SHADOW_TEXTURE_UNIT, self.shadow_input.map(|shadow_input| shadow_input.depth_texture).unwrap_or(self.empty_shadow_texture));
		self.bind_texture_unit(gl, BASE_COLOR_TEXTURE_UNIT, options.base_color_texture);
		self.bind_texture_unit(gl, EMISSIVE_TEXTURE_UNIT, options.emissive_texture);
		self.bind_texture_unit(gl, DECAL_TEXTURE_UNIT, options.decal_texture);
		self.bind_texture_unit(gl, RAMP_TEXTURE_UNIT, ramp_texture);
		self.bind_texture_unit(gl, MATCAP_TEXTURE_UNIT, options.matcap_texture);
		self.bind_texture_unit(gl, ANGEL_RING_TEXTURE_UNIT, options.angel_ring_texture);
		ToonRenderer::bind_texture_2d(gl, LOCAL_SHADOW_TEXTURE_UNIT, self.local_shadow_input.as_ref().map(|local_shadow_input| local_shadow_input.depth_texture).unwrap_or(self.empty_shadow_texture));
		for (texture_index, (uniform_name, texture)) in options.textures.iter().take(TOON_MAXIMUM_MATERIAL_TEXTURES).enumerate() {
			let unit = FIRST_MATERIAL_TEXTURE_UNIT + texture_index as u32;
			self.bind_texture_unit(gl, unit, Some(*texture));
			self.write_uniform_1i(gl, solid_program, uniform_name, unit as i32);
		}
		ToonRenderer::apply_active_texture(gl, glow::TEXTURE0);
		if let Some(on_before_draw) = options.on_before_draw {
			on_before_draw(solid_program, gl, ToonProgramKind::Solid);
			self.forget_uniform_values(solid_program);
		}
		ToonRenderer::apply_cull_face(gl, glow::BACK);
		ToonRenderer::apply_vertex_array(gl, body_vertex_array);
		draw_elements(gl, index_start, index_count);
		unsafe {
			if is_transparent {
				crate::core::glstatecache::disable(gl, glow::BLEND);
			}
		}
		ToonRenderer::apply_front_face(gl, glow::CCW);
		self.frame_statistics.draw_call_count += draw_call_count;
	}

	//==============================================================================
	/// 빛 설정. (해가 비추는 방향, 해의 색, 하늘색, 땅색, 모두 선형 색 0 ~, HDR 이면 1 을 넘어도 된다, 기본 빛 묶음)
	///
	/// # 인자
	/// - `sun_direction`: 해가 비추는 방향. ([x, y, z], 빛이 나아가는 쪽)
	/// - `sun_color`: 해의 선형 색.
	/// - `sky_color`: 위에서 오는 바탕빛의 선형 색.
	/// - `ground_color`: 아래에서 오는 바탕빛의 선형 색.
	//==============================================================================
	pub fn set_light(&mut self, sun_direction: [f64; 3], sun_color: [f64; 3], sky_color: [f64; 3], ground_color: [f64; 3]) {
		self.light_set.set_light(sun_direction, sun_color, sky_color, ground_color);
	}

	//==============================================================================
	/// 해가 비추는 방향 설정. (빛이 나아가는 쪽, toonlightset.rs 머리말)
	///
	/// # 인자
	/// - `sun_direction`: 빛이 나아가는 방향. ([x, y, z])
	//==============================================================================
	pub fn set_sun_direction(&mut self, sun_direction: [f64; 3]) {
		self.light_set.set_sun_direction(sun_direction);
	}

	//==============================================================================
	/// 해가 비추는 방향 반환. (ShadowMap::update_light_matrix 에 그대로 넘긴다)
	///
	/// # 돌려줌
	/// 해가 비추는 방향. ([x, y, z])
	//==============================================================================
	pub fn get_sun_direction(&self) -> [f64; 3] {
		self.light_set.get_sun_direction()
	}

	//==============================================================================
	/// 채움 빛 설정. (빛이 나아가는 방향과 선형 색)
	///
	/// # 인자
	/// - `fill_direction`: 빛이 나아가는 방향. ([x, y, z])
	/// - `fill_color`: 선형 색. (0 이면 없음)
	//==============================================================================
	pub fn set_fill_light(&mut self, fill_direction: [f64; 3], fill_color: [f64; 3]) {
		self.light_set.set_fill_light(fill_direction, fill_color);
	}

	//==============================================================================
	/// 림 빛 설정. (ToonLightSet::set_rim_light, 선형 색, 거듭제곱, 부드러움 0 ~ 1, 해를 받는 쪽에만 둘 몫 0 ~ 1)
	///
	/// # 인자
	/// - `rim_color`: 더할 선형 색.
	/// - `rim_power`: 거듭제곱. (0 이면 없음)
	/// - `rim_feather`: 0 단단한 테 ~ 1 부드러운 테.
	/// - `rim_light_mask`: 1 이면 해를 받는 쪽에만.
	//==============================================================================
	pub fn set_rim_light(&mut self, rim_color: [f64; 3], rim_power: f64, rim_feather: f64, rim_light_mask: f64) {
		self.light_set.set_rim_light(rim_color, rim_power, rim_feather, rim_light_mask);
	}

	//==============================================================================
	/// 하이라이트 설정. (ToonLightSet::set_specular, 세기, 지수, 부드러움 0 ~ 1, 선형 색)
	///
	/// # 인자
	/// - `specular_strength`: 세기. (0 이면 없음)
	/// - `specular_exponent`: 블린 퐁 지수.
	/// - `specular_feather`: 0 단단한 점 ~ 1 부드러운 빛.
	/// - `specular_color`: 선형 색.
	//==============================================================================
	pub fn set_specular(&mut self, specular_strength: f64, specular_exponent: f64, specular_feather: f64, specular_color: [f64; 3]) {
		self.light_set.set_specular(specular_strength, specular_exponent, specular_feather, specular_color);
	}

	//==============================================================================
	/// 점광과 스포트라이트 설정. (ToonLightSet::set_point_lights)
	///
	/// # 인자
	/// - `point_light_list`: 점광과 스포트라이트 목록. (ToonLightSet::set_point_lights 의 꼴)
	//==============================================================================
	pub fn set_point_lights(&mut self, point_light_list: &[ToonPointLight]) {
		self.light_set.set_point_lights(point_light_list);
	}

	//==============================================================================
	/// 점광 수의 한도 설정. (ToonLightSet::set_point_light_limit)
	///
	/// # 인자
	/// - `point_light_limit`: 받을 점광 수. (0 ~ TOON_MAXIMUM_POINT_LIGHTS)
	//==============================================================================
	pub fn set_point_light_limit(&mut self, point_light_limit: usize) {
		self.light_set.set_point_light_limit(point_light_limit);
	}

	//==============================================================================
	/// 그늘 셋 설정. (ToonLightSet::set_shade, 유니티 툰 셰이더의 바탕, 1 차 그늘, 2 차 그늘)
	///
	/// # 인자
	/// - `shade`: 바탕에서 1 차, 2 차 그늘로 넘어가는 문턱, 부드러움, 색. (ToonShade)
	//==============================================================================
	pub fn set_shade(&mut self, shade: ToonShade) {
		self.light_set.set_shade(shade);
	}

	//==============================================================================
	/// 그늘 램프 설정. (ToonLightSet::set_shade_ramp)
	///
	/// # 인자
	/// - `positions`: 칸마다의 자리.
	/// - `colors`: 칸마다의 선형 배.
	/// - `feathers`: 칸마다의 부드러움.
	/// - `is_half_lambert`: 반 램버트로 셀지.
	//==============================================================================
	pub fn set_shade_ramp(&mut self, positions: &[f64], colors: &[[f64; 3]], feathers: &[f64], is_half_lambert: bool) {
		self.light_set.set_shade_ramp(positions, colors, feathers, is_half_lambert);
	}

	//==============================================================================
	/// 그늘 램프 끄기. (부드러운 램버트)
	//==============================================================================
	pub fn clear_shade_ramp(&mut self) {
		self.light_set.clear_shade_ramp();
	}

	//==============================================================================
	/// 고른 칸 설정. (ToonLightSet::set_band_count, 0 이면 부드럽게)
	///
	/// # 인자
	/// - `band_count`: 칸 수.
	//==============================================================================
	pub fn set_band_count(&mut self, band_count: u32) {
		self.light_set.set_band_count(band_count);
	}

	//==============================================================================
	/// 회색 칸 램프 설정. (ToonLightSet::set_band_ramp)
	///
	/// # 인자
	/// - `thresholds`: 오름차순 문턱.
	/// - `values`: 칸마다의 받는 몫.
	/// - `is_half_lambert`: 반 램버트로 셀지.
	//==============================================================================
	pub fn set_band_ramp(&mut self, thresholds: &[f64], values: &[f64], is_half_lambert: bool) {
		self.light_set.set_band_ramp(thresholds, values, is_half_lambert);
	}

	//==============================================================================
	/// 램프 텍스처 설정. (ToonLightSet::set_ramp_texture, None 이면 칸으로)
	///
	/// # 인자
	/// - `ramp_texture`: create_texture 로 지은 램프 텍스처나 None.
	/// - `is_half_lambert`: 반 램버트로 셀지.
	//==============================================================================
	pub fn set_ramp_texture(&mut self, ramp_texture: Option<GlTexture>, is_half_lambert: bool) {
		self.light_set.set_ramp_texture(ramp_texture, is_half_lambert);
	}

	//==============================================================================
	/// 빛 꼴 설정.
	///
	/// # 인자
	/// - `light_model`: 빛 꼴. (is_energy_normalized 가 true 면 바탕색 x 빛을 π 로 나눈다)
	//==============================================================================
	pub fn set_light_model(&mut self, light_model: ToonLightModel) {
		self.light_set.set_light_model(light_model);
	}

	//==============================================================================
	/// 바깥선 색 설정. (sRGB 0 ~ 1 과 알파)
	///
	/// # 인자
	/// - `outline_color`: [r, g, b, a] (sRGB 0 ~ 1 과 알파)
	//==============================================================================
	pub fn set_outline_color(&mut self, outline_color: [f64; 4]) {
		self.outline_color = outline_color;
		self.frame_serial += 1;
	}

	//==============================================================================
	/// 껍데기 바깥선의 거리 배 설정. (기준 거리에서 지은 굵기, 멀면 그만큼 굵게 밀어 화면 굵기를 맞춘다, 배는 가장 작은 것과 큰 것 사이, 기준 거리 0 이면 끔)
	/// - 원근 카메라에서만 건다. 꼭짓점마다 카메라까지의 거리 / 기준 거리 를 굵기에 곱한다. (길티기어 Xrd 처럼 거리와 상관없이 비슷한 화면 굵기)
	///
	/// # 인자
	/// - `reference_distance`: 지은 굵기 그대로인 거리. (0 이면 끔)
	/// - `minimum_scale`: 가장 작은 배.
	/// - `maximum_scale`: 가장 큰 배.
	//==============================================================================
	pub fn set_outline_distance_scale(&mut self, reference_distance: f64, minimum_scale: f64, maximum_scale: f64) {
		let minimum_scale = minimum_scale.max(0.0);
		self.outline_distance_scale = [reference_distance.max(0.0), minimum_scale, maximum_scale.max(minimum_scale)];
		self.frame_serial += 1;
	}

	//==============================================================================
	/// 껍데기 바깥선이 멀면 사라지는 거리 설정. (가까운 거리 이하는 지은 굵기, 먼 거리 이상은 0, 사이는 smoothstep, 먼 거리가 가까운 거리 이하면 끔, 유니티 툰 셰이더의 Nearest, Farthest Distance)
	///
	/// # 인자
	/// - `near_distance`: 지은 굵기 그대로인 거리.
	/// - `far_distance`: 굵기가 0 이 되는 거리.
	//==============================================================================
	pub fn set_outline_fade_distance(&mut self, near_distance: f64, far_distance: f64) {
		self.outline_fade_distance = [near_distance.max(0.0), far_distance.max(0.0)];
		self.frame_serial += 1;
	}

	//==============================================================================
	/// 껍데기 바깥선이 사라지는 거리 반환. ([가까운 거리, 먼 거리])
	///
	/// # 돌려줌
	/// [가까운 거리, 먼 거리]
	//==============================================================================
	pub fn get_outline_fade_distance(&self) -> [f64; 2] {
		self.outline_fade_distance
	}

	//==============================================================================
	/// 받을 점광과 스포트라이트 그림자 설정. (ShadowAtlas 나 None, 세기 0 ~ 1, 아틀라스는 기본 빛 묶음의 점광 목록으로 지은 것)
	///
	/// # 인자
	/// - `shadow_atlas`: ShadowAtlas 나 None.
	/// - `strength`: 그림자 세기. (0 ~ 1)
	//==============================================================================
	pub fn set_local_shadows(&mut self, shadow_atlas: Option<&ShadowAtlas>, strength: f64) {
		self.frame_serial += 1;
		self.local_shadow_input = shadow_atlas.map(|shadow_atlas| {
			let resolution = shadow_atlas.get_resolution().max(1) as f64;
			ToonLocalShadowInput { depth_texture: shadow_atlas.get_depth_texture(), light_list: shadow_atlas.get_lights().to_vec(), grid_size: shadow_atlas.get_grid_size() as f64, tile_size: shadow_atlas.get_tile_size() as f64 / resolution, texel_size: 1.0 / resolution, strength: strength.clamp(0.0, 1.0) }
		});
	}

	//==============================================================================
	/// 빛 색 쓰기 설정. (ToonLightSet::set_light_color_usage)
	///
	/// # 인자
	/// - `light_color_usage`: 칸마다 해의 색을 쓸지. (ToonLightColorUsage)
	//==============================================================================
	pub fn set_light_color_usage(&mut self, light_color_usage: ToonLightColorUsage) {
		self.light_set.set_light_color_usage(light_color_usage);
	}

	//==============================================================================
	/// 빛 세기 막기 설정. (ToonLightSet::set_light_intensity_limited, 기본 true)
	///
	/// # 인자
	/// - `is_limited`: true 면 빛마다 받은 몫을 1 로 막는다. (기본 true)
	//==============================================================================
	pub fn set_light_intensity_limited(&mut self, is_limited: bool) {
		self.light_set.set_light_intensity_limited(is_limited);
	}

	//==============================================================================
	/// 그늘 쪽 림 빛 설정. (ToonLightSet::set_shade_rim_light)
	///
	/// # 인자
	/// - `shade_rim_color`: 더할 선형 색.
	/// - `shade_rim_power`: 거듭제곱. (0 이면 없음)
	//==============================================================================
	pub fn set_shade_rim_light(&mut self, shade_rim_color: [f64; 3], shade_rim_power: f64) {
		self.light_set.set_shade_rim_light(shade_rim_color, shade_rim_power);
	}

	//==============================================================================
	/// 껍데기 바깥선의 거리 배 반환. ([기준 거리, 가장 작은 배, 가장 큰 배])
	///
	/// # 돌려줌
	/// [기준 거리, 가장 작은 배, 가장 큰 배]
	//==============================================================================
	pub fn get_outline_distance_scale(&self) -> [f64; 3] {
		self.outline_distance_scale
	}

	//==============================================================================
	/// HDR 출력 설정. (true 면 감마를 걸지 않은 선형 색을 낸다, ToonPostProcess 의 실수 렌더 타겟에 그릴 때)
	///
	/// # 인자
	/// - `is_hdr_output`: true 면 감마를 걸지 않은 선형 색을 낸다.
	//==============================================================================
	pub fn set_hdr_output(&mut self, is_hdr_output: bool) {
		self.is_hdr_output = is_hdr_output;
		self.frame_serial += 1;
	}

	//==============================================================================
	/// 받을 그림자 설정. (ShadowMap 이나 None, 세기 0 ~ 1, 원본 기본 세기는 0.6)
	///
	/// # 인자
	/// - `shadow_map`: 받을 ShadowMap. (None 이면 그림자를 받지 않는다)
	/// - `strength`: 그림자 세기 0 ~ 1.
	//==============================================================================
	pub fn set_shadow(&mut self, shadow_map: Option<&ShadowMap>, strength: f64) {
		self.frame_serial += 1;
		self.shadow_input = shadow_map.map(|shadow_map| {
			ToonShadowInput { depth_texture: shadow_map.get_depth_texture(), light_view_projection_matrix: *shadow_map.get_light_view_projection_matrix(), resolution: shadow_map.get_resolution(), strength }
		});
	}

	//==============================================================================
	/// 이번 프레임의 그린 수와 거른 수.
	///
	/// # 돌려줌
	/// 이번 프레임의 셈. (draw_call_count, drawn_object_count, culled_object_count)
	//==============================================================================
	pub fn get_frame_statistics(&self) -> ToonFrameStatistics {
		self.frame_statistics
	}

	//==============================================================================
	/// 머티리얼과 인스턴스 꼴의 프로그램 반환. (지은 것만, 원본 getSolidProgram(material) 들)
	///
	/// # 인자
	/// - `material`: 머티리얼. (None 이면 빈 조각)
	/// - `instance_layout`: 인스턴스 꼴. (None 이면 없음)
	/// - `kind`: 프로그램 종류. (몸, 바깥선, 그림자 깊이)
	///
	/// # 돌려줌
	/// 그 프로그램. (아직 짓지 않았거나 짓지 못했으면 None)
	//==============================================================================
	pub fn get_program(&self, material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>, kind: ToonProgramKind) -> Option<&ShaderProgram> {
		let key = (material.map(|material| material.get_id()).unwrap_or(0), instance_layout.map(|instance_layout| instance_layout.get_id()).unwrap_or(0));
		let program_set = self.program_set_table.get(&key)?.as_ref()?;
		Some(match kind {
			ToonProgramKind::Solid => {
				&program_set.solid.program
			}
			ToonProgramKind::Outline => {
				&program_set.outline.program
			}
			ToonProgramKind::Depth => {
				&program_set.depth.program
			}
		})
	}

	//==============================================================================
	/// 기본 프로그램 셋 반환. (생성에서 지어 늘 있다)
	//==============================================================================
	fn read_default_program_set(&self) -> &ToonProgramSet {
		match self.program_set_table.get(&(0, 0)) {
			Some(Some(program_set)) => {
				program_set
			}
			_ => {
				unreachable!("기본 프로그램은 생성에서 짓는다")
			}
		}
	}

	//==============================================================================
	/// 몸 프로그램 반환. (기본 프로그램)
	///
	/// # 돌려줌
	/// 몸 프로그램.
	//==============================================================================
	pub fn get_solid_program(&self) -> &ShaderProgram {
		&self.read_default_program_set().solid.program
	}

	//==============================================================================
	/// 바깥선 프로그램 반환. (기본 프로그램)
	///
	/// # 돌려줌
	/// 바깥선 프로그램.
	//==============================================================================
	pub fn get_outline_program(&self) -> &ShaderProgram {
		&self.read_default_program_set().outline.program
	}

	//==============================================================================
	/// 그림자 깊이 프로그램 반환. (기본 프로그램)
	///
	/// # 돌려줌
	/// 그림자 깊이 프로그램.
	//==============================================================================
	pub fn get_depth_program(&self) -> &ShaderProgram {
		&self.read_default_program_set().depth.program
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::vector3::Vector3;

	//==============================================================================
	/// 모형을 짓다 버퍼나 버텍스 어레이에서 실패하면 그때까지 지은 GL 객체를 모두 지우고 None. (몸, 뼈대, UV, 바깥선, 바깥선 뼈대가 다 있는 모형, 가짜 GL)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	#[test]
	fn failed_mesh_creation_deletes_created_objects() {
		use crate::core::imagetexturecache::fake_gl::{self, FakeGlObject};
		let arrays = ToonMeshArrays {
			vertex_data: vec![0.0; VERTEX_FLOAT_COUNT * 3],
			index_data: vec![0, 1, 2],
			outline_vertex_data: vec![0.0; 9],
			outline_index_data: vec![0, 1, 2],
			skin_data: vec![0.0; SKIN_FLOAT_COUNT * 3],
			outline_skin_data: vec![0.0; SKIN_FLOAT_COUNT * 3],
			uv_data: vec![0.0; 6],
			..ToonMeshArrays::default()
		};
		// 다 지으면 버퍼 일곱(인스턴스, 몸, 뼈대, UV, 몸 번호, 바깥선, 바깥선 뼈대, 바깥선 번호 가운데 인스턴스 뺀 것은 buffer_list)과 버텍스 어레이 둘.
		let gl = fake_gl::create_context();
		let mesh = ToonRenderer::create_mesh_objects(crate::GlContext::from_context(&gl), &arrays).expect("mesh");
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Buffer), 8);
		assert_eq!(fake_gl::get_live_count(FakeGlObject::VertexArray), 2);
		assert_eq!(mesh.buffer_list.len(), 7);
		assert!(mesh.outline_vertex_array.is_some());
		for success_count in 0..8 {
			let gl = fake_gl::create_context();
			fake_gl::fail_after(FakeGlObject::Buffer, success_count);
			assert!(ToonRenderer::create_mesh_objects(crate::GlContext::from_context(&gl), &arrays).is_none());
			assert_eq!(fake_gl::get_live_count(FakeGlObject::Buffer), 0);
			assert_eq!(fake_gl::get_live_count(FakeGlObject::VertexArray), 0);
		}
		for success_count in 0..2 {
			let gl = fake_gl::create_context();
			fake_gl::fail_after(FakeGlObject::VertexArray, success_count);
			assert!(ToonRenderer::create_mesh_objects(crate::GlContext::from_context(&gl), &arrays).is_none());
			assert_eq!(fake_gl::get_live_count(FakeGlObject::Buffer), 0);
			assert_eq!(fake_gl::get_live_count(FakeGlObject::VertexArray), 0);
		}
	}

	//==============================================================================
	/// 카메라 기울임을 되돌리는 각과 껍데기 바깥선에 곱할 해의 색 보기. (기운 카메라, 바로 아래를 볼 때 0, 세기 1 이상이면 세기로 나눔)
	//==============================================================================
	#[test]
	fn camera_roll_matches_tilted_camera() {
		let roll = 0.3f64;
		let rotation = [roll.cos(), -roll.sin(), 0.0, roll.sin(), roll.cos(), 0.0, 0.0, 0.0, 1.0].map(|value: f64| value as f32);
		assert!((read_camera_roll(&rotation, false) + roll).abs() < 0.0001);
		assert!(read_camera_roll(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], false).abs() < 0.0001);
		assert_eq!(read_camera_roll(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0], false), 0.0);
		let outline_color = read_outline_light_color([2.0, 2.0, 2.0]);
		assert!((outline_color[0] - 1.0).abs() < 0.0001);
		assert_eq!(read_outline_light_color([0.5, 0.4, 0.3]), [0.5, 0.4, 0.3]);
	}

	//==============================================================================
	/// UV 와 2 번 UV 를 점마다 넷으로 섞고, 2 번 UV 가 없으면 둘 그대로인지.
	//==============================================================================
	#[test]
	fn uv_data_mixes_second_uv() {
		let uv_data = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0];
		let uv1_data = [0.5, 0.5, 0.6, 0.5, 0.6, 0.6];
		let data = read_uv_data(&uv_data, Some(&uv1_data), 3);
		assert_eq!(data.len(), 12);
		assert_eq!(data[2], 0.5);
		assert_eq!(data[7], 0.5);
		assert_eq!(read_uv_data(&uv_data, None, 2), vec![0.0, 0.0, 1.0, 0.0]);
	}

	//==============================================================================
	/// 절두체가 가운데 구는 남기고 카메라 뒤와 옆으로 먼 구는 거르는지.
	//==============================================================================
	#[test]
	fn frustum_keeps_center_and_drops_behind() {
		let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 10.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let mut view_projection_matrix = Matrix4::create_perspective(0.8, 1.0, 0.1, 100.0);
		view_projection_matrix.multiply(&view_matrix);
		let planes = extract_frustum_planes(&view_projection_matrix);
		let is_inside = |point: [f64; 3], radius: f64| planes.iter().all(|plane| plane[0] * point[0] + plane[1] * point[1] + plane[2] * point[2] + plane[3] >= -radius);
		assert!(is_inside([0.0, 0.0, 0.0], 1.0));
		assert!(!is_inside([0.0, 0.0, 20.0], 1.0));
		assert!(!is_inside([50.0, 0.0, 0.0], 1.0));
		assert!(is_inside([5.0, 0.0, 0.0], 2.0));
	}

	//==============================================================================
	/// 보기와 투영에서 눈의 자리, 거울에 비친 것, 직교의 보는 방향을 읽는지.
	//==============================================================================
	#[test]
	fn frame_camera_reads_eye_and_mirror() {
		let eye = Vector3::create(1.0, 2.0, 8.0);
		let view_matrix = Matrix4::create_look_at(eye, Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let mut view_projection_matrix = Matrix4::create_perspective(0.8, 1.5, 0.1, 100.0);
		view_projection_matrix.multiply(&view_matrix);
		let frame_camera = read_frame_camera(&view_projection_matrix);
		assert!(!frame_camera.is_mirrored);
		assert!((frame_camera.position[0] - 1.0).abs() < 1e-3 && (frame_camera.position[1] - 2.0).abs() < 1e-3 && (frame_camera.position[2] - 8.0).abs() < 1e-3);
		assert_eq!(frame_camera.position[3], 1.0);
		let mut mirrored_matrix = view_projection_matrix;
		mirrored_matrix.multiply(&create_reflection_matrix([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
		assert!(read_frame_camera(&mirrored_matrix).is_mirrored);
		let mut orthographic_matrix = Matrix4::create_identity();
		orthographic_matrix.elements[10] = -1.0;
		let orthographic_camera = read_frame_camera(&orthographic_matrix);
		assert_eq!(orthographic_camera.position[3], 0.0);
		assert!((orthographic_camera.forward[2] + 1.0).abs() < 1e-6);
	}

	//==============================================================================
	/// 평면에 비친 행렬의 행렬식이 음수이고 점을 평면 건너로 옮기는지.
	//==============================================================================
	#[test]
	fn reflection_matrix_flips_winding() {
		let reflection_matrix = create_reflection_matrix([0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
		assert!(read_matrix_determinant(&reflection_matrix) < 0.0);
		let point = reflection_matrix.transform_point(0.5, 3.0, -2.0);
		assert!((point.x - 0.5).abs() < 1e-6 && (point.y + 1.0).abs() < 1e-6 && (point.z + 2.0).abs() < 1e-6);
	}

	//==============================================================================
	/// 머티리얼 조각과 인스턴스 꼴의 어트리뷰트가 소스와 어트리뷰트 자리에 들어가는지.
	//==============================================================================
	#[test]
	fn material_sources_insert_chunks_and_instance_attributes() {
		let material = ToonMaterial::new(ToonMaterialChunks { name: String::from("test"), uniform_chunk: String::from("uniform float markValue;\n"), surface_chunk: String::from("void applySurface(inout ToonSurface surface) {\n\tsurface.alpha *= markValue;\n}\n"), ..ToonMaterialChunks::default() });
		let instance_layout = ToonInstanceLayout::new(&[("instanceOffset", 3)]).unwrap();
		let sources = compose_toon_program_sources(Some(&material), Some(&instance_layout));
		assert!(sources.solid_fragment_shader.contains("uniform float markValue;"));
		assert!(sources.solid_fragment_shader.contains("surface.alpha *= markValue;"));
		assert!(sources.solid_fragment_shader.contains(DEFAULT_LIGHT_CHUNK));
		assert!(sources.solid_vertex_shader.contains("in vec3 instanceOffset;"));
		assert!(sources.depth_vertex_shader.contains(DEFAULT_VERTEX_CHUNK));
		assert!(sources.attribute_locations.contains(&(String::from("instanceOffset"), 13)));
		let default_sources = compose_toon_program_sources(None, None);
		assert!(default_sources.solid_fragment_shader.contains(DEFAULT_SURFACE_CHUNK));
		assert_eq!(default_sources.attribute_locations.len(), TOON_ATTRIBUTE_LOCATIONS.len());
	}
}
