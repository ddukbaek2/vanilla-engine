//==============================================================================
// 툰 렌더러. (vanilla.js experimental/graphics/toon/toonrenderer.js 의 ToonRenderer)
// - MeshBuilder 의 배열이나 read_glb_arrays 로 읽은 모형을 한 색 재질과 바깥선으로 그린다.
// - 빛: ToonLightSet 하나를 기본으로 든다. 해(방향과 색), 하늘색과 땅색의 바탕빛, 채움 빛, 림 빛, 정반사, 점광(TOON_MAXIMUM_POINT_LIGHTS 까지), 띠(칸 수나 램프), 빛 꼴(÷π).
//   set_light, set_sun_direction, set_fill_light, set_rim_light, set_specular, set_point_lights, set_point_light_limit, set_band_count, set_band_ramp, set_light_model 은 기본 묶음을 고친다.
//   그리기마다 ToonDrawOptions 의 light_set 으로 다른 묶음(create_light_set)을 줄 수 있다. 해 방향의 뜻(빛이 나아가는 쪽)은 toonlightset.rs 머리말에 있다.
// - 재질: 꼭짓점의 색(sRGB), 빛남(바탕색에 곱해 더함), 빛을 받지 않음. 꼭짓점 UV(arrays.uv_data, 2 씩)가 있으면 텍스처를 읽는다.
// - 그리기 꼴(ToonDrawOptions, 기본값이면 지금까지와 같은 화면):
//   tint_color [r, g, b, a] sRGB 와 alpha. tint_amount 를 주면(0 ~ 1) 틴트를 곱하지 않고 sRGB 로 섞는다. (맞은 번쩍임, 고장 빨강)
//   glow_scale 꼭짓점 빛남의 배(기본 1), color_scale 마지막 선형 색의 배(기본 1, 거울에 비친 모습을 어둡게), emissive_color [r, g, b] sRGB 와 emissive_intensity(선형 배, 기본 1, 색 기본 검정).
//   base_color_texture(바탕색에 곱함, 알파는 불투명도에), emissive_texture(빛남 색에 곱함), uv_transform [u 옮김, v 옮김, u 배, v 배] (KHR_texture_transform 의 offset, scale).
//   decal_texture 는 바탕색 위에 알파로 덮는다(색 = 텍스처 x decal_color). decal_emission 만큼 그 색으로 빛난다. decal_uv_transform 은 묶음 그림의 한 칸을 고를 때.
//   alpha_cutoff 는 불투명도가 그보다 작은 화소를 버린다. (0 이면 끔, glTF MASK)
//   blend_mode: ToonBlendMode::Alpha(기본, 곧은 알파), Premultiplied(미리 곱한 알파), Additive(더하기). Alpha 가 아니면 비치는 것으로 그린다.
//   soft_edge 는 pow(|dot(N, V)|, soft_edge) 를 불투명도에 곱해 가장자리를 엷게 한다. (0 이면 끔, 프레넬 꼴)
//   is_mirrored 는 감기를 뒤집어 그린다. None 이면 모형 행렬의 행렬식이 음수일 때 스스로 뒤집는다. 보기 행렬이 거울에 비쳤으면 그것도 더해 뒤집는다.
//   index_start, index_count 는 몸의 꼭짓점 번호 가운데 그 자리만 그린다. (0 이면 끝까지, GLB 재질 범위, 숨긴 부분) outline_index_start, outline_index_count 는 바깥선 껍데기.
//   material(ToonMaterial), textures([(유니폼 이름, 텍스처)]), on_before_draw(program, gl, ToonProgramKind) 는 확장 자리다. (toonmaterial.rs 머리말)
//   light_set(ToonLightSet) 은 그 그리기의 빛 묶음이다. group, is_depth_prepassed 는 ToonDrawQueue 만 읽는다.
// - 바깥선: MeshBuilder 가 지은 껍데기를 뒷면만 그린다.
// - 뼈대: write_skin_pose 가 셈한 본 행렬을 draw_mesh 에 넘긴다. (TOON_MAXIMUM_JOINTS 까지, 엔진 SkinnedModelRenderer 와 같은 96)
// - GPU 인스턴싱: draw_mesh_instanced 로 같은 모형 여럿을 인스턴스마다 행렬과 틴트로 한 번에 그린다. (몸과 바깥선 모두, 그릴 때마다 걸러서 올림)
//   오래 두는 인스턴스는 create_instance_layout 과 create_instance_buffer 로 영구 버퍼를 짓고 draw_mesh_instance_buffer 로 그린다. (거르지 않음, 머티리얼 정점 조각이 사용자 어트리뷰트를 읽음)
// - 화면 밖 거르기: 모형의 경계 구를 보기와 투영의 절두체에 대어 밖이면 그리지 않는다. (인스턴스는 하나씩 걸러 남은 것만 올린다)
// - 그림자: 엔진 ShadowMap 에 begin_shadow_pass 와 draw_mesh 로 드리우고, set_shadow 로 받는다. (3x3 PCF)
//   그림자를 받지 않을 때도 그림자 유닛에 1x1 비교 깊이 텍스처를 묶는다. (sampler2DShadow 유닛에 깊이 텍스처가 없으면 맥 드라이버가 알리고, 색 텍스처가 묶여 있으면 WebGL 이 그리기를 거부한다)
// - HDR: set_hdr_output(true) 면 감마를 걸지 않은 선형 색을 낸다. 실수 렌더 타겟(ToonPostProcess)에 그리고 블룸과 톤매핑으로 마무리한다.
// - 사용: create_mesh(gl, arrays) → 매 프레임 begin_frame(gl, 보기와 투영) → draw_mesh / draw_mesh_instanced → end_frame(gl) → 다 쓰면 delete_mesh.
//   begin_frame 이 앞의 그리기 상태(섞기, 깊이, 면 가리기, 프로그램, 버텍스 어레이)를 적어 두고 깊이 시험과 면 가리기를 켜며, end_frame 이 적어 둔 상태로 돌린다.
//   (엔진 2D 그리기 사이에 끼울 때, 그 전에 Graphic 의 묶음을 내보낸다)
//   begin_frame_with_matrices(gl, 보기, 투영) 는 두 행렬로 시작한다. (직교 원 행렬 카메라) 카메라의 자리(정반사, 림, 소프트 엣지)는 보기와 투영에서 읽는다.
// - 프로그램마다 프레임 값(보기와 투영, 카메라, 그림자, HDR)과 빛 묶음은 바뀔 때만 올린다. 그리기마다 바뀌는 값(모형 행렬, 틴트, 텍스처)만 매번 올린다.
// - read_program_sources 로 ShaderWarmer 에 넘길 기본 소스를, read_material_program_sources(머티리얼, 인스턴스 꼴) 로 그 프로그램의 소스를 준다. (원본은 한 함수에 선택 인자)
// - 노드 사각형 안에 그리기: set_screen_rect(x, y, 너비, 높이) 를 주면 begin_frame 이 보기와 투영의 클립 공간을 그 사각형으로 옮기고 가위로 밖을 자른다.
//   사각형은 begin_frame 때의 뷰포트(보통 드로잉 버퍼 전체) 안의 픽셀이고 왼쪽 위가 원점이다. 화면 밖 거르기는 옮기기 전의 절두체로 한다. end_frame 이 가위를 푼다.
// - begin_frame_with_camera(Camera3D) 는 카메라의 보기와 투영으로 begin_frame 을 부른다. 2D 표시(Camera3D::project_to_view)와 같은 카메라를 쓴다.
// - draw_mesh_outline 은 바깥선 껍데기만 그린다. (ToonDrawQueue 가 불투명한 몸을 다 그린 뒤 바깥선을 몰아 그릴 때, 그린 수는 세지 않는다)
// - ToonDrawOptions 의 casts_shadow 는 ToonDrawQueue::draw_shadows 만 읽는다. is_body_drawn 이 false 면 바깥선만 그린다. (원본은 꼴 객체에 이름으로 단다)
// - 텍스처: create_texture(gl, 너비, 높이, RGBA 바이트, ToonTextureOptions) 로 짓는다. UV (0, 0) 이 그림의 왼쪽 위다. (glTF 와 같음)
//==============================================================================
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use glow::HasContext;

use crate::base::rect::Rect;
use crate::graphics3d::camera3d::Camera3D;

use crate::base::matrix4::Matrix4;
use crate::core::graphic::{GlBuffer, GlProgram, GlTexture, GlVertexArray};
use crate::graphics3d::GlFramebuffer;
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::shadowmap::ShadowMap;
use crate::graphics3d::toon::meshbuilder::{ToonMeshArrays, SKIN_FLOAT_COUNT, VERTEX_FLOAT_COUNT};
use crate::graphics3d::toon::toonlightset::{ToonLightModel, ToonLightSet, ToonPointLight, TOON_MAXIMUM_BAND_STEPS, TOON_MAXIMUM_POINT_LIGHTS};
use crate::graphics3d::toon::toonmaterial::{ToonBlendMode, ToonBufferUsage, ToonInstanceLayout, ToonMaterial, ToonMaterialChunks, TOON_INSTANCE_MATRIX_NAME, TOON_INSTANCE_TINT_NAME};

// 한 모형이 쓰는 뼈대 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_JOINTS: usize = 96;

// 인스턴스 하나의 수 개수. (행렬 16, 틴트 4)
pub const TOON_INSTANCE_FLOAT_COUNT: usize = 20;

// 텍스처 유닛. (그림자, 바탕색, 빛남, 데칼, 머티리얼 텍스처는 7 부터 15 까지)
const SHADOW_TEXTURE_UNIT: u32 = 3;
const BASE_COLOR_TEXTURE_UNIT: u32 = 4;
const EMISSIVE_TEXTURE_UNIT: u32 = 5;
const DECAL_TEXTURE_UNIT: u32 = 6;
const FIRST_MATERIAL_TEXTURE_UNIT: u32 = 7;

// 머티리얼이 그리기마다 묶을 수 있는 텍스처 수의 상한. (유닛 7 ~ 15)
pub const TOON_MAXIMUM_MATERIAL_TEXTURES: usize = 9;

// 어트리뷰트 자리. (모든 프로그램이 같은 자리를 쓴다, 인스턴스 꼴의 사용자 어트리뷰트는 13 부터)
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

// 자주 쓰는 어트리뷰트 자리.
const INSTANCE_MATRIX_LOCATION: u32 = 7;
const INSTANCE_TINT_LOCATION: u32 = 11;
const UV_LOCATION: u32 = 12;

// 모형마다 다른 번호를 주는 셈. (인스턴스 버퍼가 모형마다 지은 버텍스 어레이를 가린다)
static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);

//==============================================================================
// 그리기의 프로그램 종류. (on_before_draw 의 셋째 인자)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonProgramKind {
	Solid,
	Outline,
	Depth,
}

// 세 정점 셰이더의 머리. (몸에만 있는 어트리뷰트도 모두 적는다, 바깥선 버텍스 어레이에 없는 것은 기본값으로 읽힘)
const VERTEX_HEAD_CHUNK: &str = "#version 300 es
in vec3 vertexPosition;
in vec3 vertexNormal;
in vec3 vertexColor;
in vec2 vertexMaterial;
in vec4 vertexDetail;
in vec2 vertexUv;
";

// 꼭짓점을 본으로 움직이고 인스턴스 행렬을 고르는 셰이더 조각.
// - instanced: 0 이면 인스턴스 아님, 1 이면 인스턴스 행렬과 틴트, 2 면 인스턴스 틴트만(모형 행렬은 유니폼).
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
	return mat4(instanceMatrix0, instanceMatrix1, instanceMatrix2, instanceMatrix3);
}
vec4 readInstanceTint() {
	if (instanced < 0.5) {
		return vec4(1.0);
	}
	return instanceTint;
}
";

// 정점 조각이 고칠 꼭짓점. (월드로 옮기기 전)
const VERTEX_STRUCT_CHUNK: &str = "struct ToonVertex {
	vec3 localPosition;
	vec3 localNormal;
	vec3 color;
	vec2 material;
	vec4 detail;
	vec2 uv;
	vec4 tint;
	mat4 modelMatrix;
};
ToonVertex readVertex() {
	ToonVertex vertex;
	vertex.localPosition = vertexPosition;
	vertex.localNormal = vertexNormal;
	vertex.color = vertexColor;
	vertex.material = vertexMaterial;
	vertex.detail = vertexDetail;
	vertex.uv = vertexUv;
	vertex.tint = readInstanceTint();
	vertex.modelMatrix = readModelMatrix();
	return vertex;
}
";

// 머티리얼에 조각이 없을 때의 빈 함수.
const DEFAULT_VERTEX_CHUNK: &str = "void applyVertex(inout ToonVertex vertex) {
}
";

const DEFAULT_SURFACE_CHUNK: &str = "void applySurface(inout ToonSurface surface) {
}
";

const DEFAULT_LIGHT_CHUNK: &str = "void applyLight(ToonSurface surface, inout ToonLighting lighting) {
}
";

// 몸을 그리는 정점 셰이더의 몸통.
const SOLID_VERTEX_BODY: &str = "uniform mat4 lightViewProjectionMatrix;
out vec3 worldPosition;
out vec3 localPosition;
out vec3 worldNormal;
out vec3 surfaceColor;
out vec2 surfaceMaterial;
out vec4 surfaceDetail;
out vec2 surfaceUv;
out vec4 surfaceTint;
out vec4 lightSpacePosition;
void main() {
	ToonVertex vertex = readVertex();
	applyVertex(vertex);
	mat4 skinMatrix = readSkinMatrix();
	mat4 objectMatrix = vertex.modelMatrix;
	vec4 position = objectMatrix * skinMatrix * vec4(vertex.localPosition, 1.0);
	worldPosition = position.xyz;
	localPosition = vertex.localPosition;
	worldNormal = normalize(mat3(objectMatrix) * mat3(skinMatrix) * vertex.localNormal);
	surfaceColor = vertex.color;
	surfaceMaterial = vertex.material;
	surfaceDetail = vertex.detail;
	surfaceUv = vertex.uv;
	surfaceTint = vertex.tint;
	lightSpacePosition = lightViewProjectionMatrix * vec4(position.xyz + worldNormal * 0.02, 1.0);
	gl_Position = viewProjectionMatrix * position;
}
";

// 몸을 그리는 조각 셰이더의 머리. (유니폼, 표면과 빛 구조, 공용 함수)
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
in vec4 surfaceTint;
in vec4 lightSpacePosition;
uniform vec3 sunDirection;
uniform vec3 sunColor;
uniform vec3 skyColor;
uniform vec3 groundColor;
uniform vec3 fillDirection;
uniform vec3 fillColor;
uniform vec4 rimLight;
uniform vec2 specularLight;
uniform float energyNormalized;
uniform float bandCount;
uniform int bandRampCount;
uniform float bandRampHalfLambert;
uniform float bandRampThresholds[7];
uniform float bandRampValues[8];
uniform int pointLightCount;
uniform vec4 pointLightPositions[8];
uniform vec3 pointLightColors[8];
uniform vec3 pointLightShapes[8];
uniform vec4 cameraPosition;
uniform vec3 cameraForward;
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
	vec3 localPosition;
	vec3 worldPosition;
	vec3 viewDirection;
};
struct ToonLighting {
	vec3 irradiance;
	vec3 specular;
	float shadow;
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
float readBandRamp(float cosine) {
	float coordinate = bandRampHalfLambert > 0.5 ? cosine * 0.5 + 0.5 : max(cosine, 0.0);
	for (int index = 0; index < 7; ++index) {
		if (index >= bandRampCount - 1) {
			break;
		}
		if (coordinate < bandRampThresholds[index]) {
			return bandRampValues[index];
		}
	}
	return bandRampValues[max(bandRampCount - 1, 0)];
}
float readDirectFacing(float cosine) {
	if (bandRampCount > 0) {
		return readBandRamp(cosine);
	}
	float facing = max(cosine, 0.0);
	if (bandCount > 0.5) {
		facing = floor(facing * bandCount + 0.5) / bandCount;
	}
	return facing;
}
vec3 readViewDirection() {
	if (cameraPosition.w > 0.5) {
		return normalize(cameraPosition.xyz - worldPosition);
	}
	return -cameraForward;
}
";

// 몸을 그리는 조각 셰이더의 몸통. (표면 → 표면 조각 → 틴트 → 빛 → 빛 조각 → 합치기)
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
	surface.emission = emission;
	surface.normal = normalize(worldNormal);
	surface.glow = surfaceMaterial.x;
	surface.isUnlit = surfaceMaterial.y > 0.5;
	surface.detail = surfaceDetail;
	surface.uv = surfaceUv;
	surface.localPosition = localPosition;
	surface.worldPosition = worldPosition;
	surface.viewDirection = readViewDirection();
	return surface;
}
ToonLighting readLighting(ToonSurface surface) {
	ToonLighting lighting;
	vec3 normal = surface.normal;
	float shadow = readShadow();
	vec3 toSun = -normalize(sunDirection);
	float sunCosine = dot(normal, toSun);
	float sunFacing;
	if (bandRampCount > 0) {
		sunFacing = readBandRamp(sunCosine) * shadow;
	}
	else {
		sunFacing = max(sunCosine, 0.0) * shadow;
		if (bandCount > 0.5) {
			sunFacing = floor(sunFacing * bandCount + 0.5) / bandCount;
		}
	}
	vec3 irradiance = sunColor * sunFacing + mix(groundColor, skyColor, normal.y * 0.5 + 0.5);
	irradiance += fillColor * max(dot(normal, -normalize(fillDirection)), 0.0);
	if (rimLight.a > 0.0) {
		irradiance += rimLight.rgb * pow(1.0 - clamp(dot(normal, surface.viewDirection), 0.0, 1.0), rimLight.a);
	}
	for (int index = 0; index < 8; ++index) {
		if (index >= pointLightCount) {
			break;
		}
		vec3 toLight = pointLightPositions[index].xyz - surface.worldPosition;
		float lightDistance = length(toLight);
		float lightRange = pointLightPositions[index].w;
		vec3 shape = pointLightShapes[index];
		float window = 1.0;
		if (lightRange > 0.0) {
			window = clamp(1.0 - pow(lightDistance / lightRange, shape.y), 0.0, 1.0);
			window *= window;
		}
		float falloff = shape.x > 0.0 ? window / max(pow(lightDistance, shape.x), 0.01) : window;
		float cosine = dot(normal, toLight / max(lightDistance, 0.0001));
		irradiance += pointLightColors[index] * readDirectFacing((cosine + shape.z) / (1.0 + shape.z)) * falloff;
	}
	lighting.irradiance = irradiance;
	lighting.specular = vec3(0.0);
	if (specularLight.x > 0.0) {
		vec3 halfway = normalize(toSun + surface.viewDirection);
		float specularNormalization = energyNormalized > 0.5 ? (specularLight.y + 8.0) * RECIPROCAL_PI / 8.0 : 1.0;
		lighting.specular = sunColor * pow(max(dot(normal, halfway), 0.0), specularLight.y) * specularLight.x * specularNormalization * shadow;
	}
	lighting.shadow = shadow;
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
		color = albedo * lighting.irradiance * diffuseScale + lighting.specular + emission;
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

// 바깥선 껍데기를 그리는 셰이더. (껍데기는 뒷면만 그려 몸 둘레에 테로 보인다)
const OUTLINE_VERTEX_BODY: &str = "out float outlineTint;
void main() {
	ToonVertex vertex = readVertex();
	applyVertex(vertex);
	mat4 skinMatrix = readSkinMatrix();
	outlineTint = vertex.tint.a;
	gl_Position = viewProjectionMatrix * vertex.modelMatrix * skinMatrix * vec4(vertex.localPosition, 1.0);
}
";

const OUTLINE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in float outlineTint;
uniform vec4 outlineColor;
uniform float alpha;
uniform float hdrOutput;
uniform float premultipliedOutput;
out vec4 fragmentColor;
void main() {
	vec3 color = hdrOutput > 0.5 ? pow(outlineColor.rgb, vec3(2.2)) : outlineColor.rgb;
	float outputAlpha = outlineColor.a * alpha * outlineTint;
	if (premultipliedOutput > 0.5) {
		color *= outputAlpha;
	}
	fragmentColor = vec4(color, outputAlpha);
}
";

// 그림자 깊이를 그리는 셰이더.
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
// 세 프로그램의 소스와 어트리뷰트 자리. (compose_toon_program_sources 의 결과)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonProgramSources {
	pub solid_vertex_shader: String,
	pub solid_fragment_shader: String,
	pub outline_vertex_shader: String,
	pub outline_fragment_shader: String,
	pub depth_vertex_shader: String,
	pub depth_fragment_shader: String,
	pub attribute_locations: Vec<(String, u32)>,
}

//==============================================================================
// 머티리얼과 인스턴스 꼴로 세 프로그램의 소스 짓기. (둘 다 None 이면 기본 프로그램)
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
// GPU 에 올린 모형.
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
	bounding_center: [f32; 3],
	bounding_radius: f32,
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
	// 뼈대로 움직이는 모형인지.
	//==============================================================================
	pub fn is_skinned(&self) -> bool {
		self.is_skinned
	}

	//==============================================================================
	// 텍스처 좌표가 있는지.
	//==============================================================================
	pub fn has_uv(&self) -> bool {
		self.has_uv
	}

	//==============================================================================
	// 경계 구. (모형 좌표의 가운데와 반지름, 뼈대 모형은 쉬는 자세의 1.5 배)
	//==============================================================================
	pub fn get_bounding_sphere(&self) -> ([f32; 3], f32) {
		(self.bounding_center, self.bounding_radius)
	}

	//==============================================================================
	// 몸과 바깥선 껍데기의 꼭짓점 번호 수.
	//==============================================================================
	pub fn get_index_counts(&self) -> (usize, usize) {
		(self.body_index_count as usize, self.outline_index_count as usize)
	}

	//==============================================================================
	// 바깥선 껍데기가 있는지.
	//==============================================================================
	pub fn has_outline(&self) -> bool {
		self.outline_vertex_array.is_some()
	}

	//==============================================================================
	// 몸의 버텍스 어레이와 번호 수. (화면 공간 바깥선이 같은 모형을 한 번 더 그릴 때)
	//==============================================================================
	pub(crate) fn get_body(&self) -> (GlVertexArray, i32) {
		(self.body_vertex_array, self.body_index_count)
	}

	//==============================================================================
	// 인스턴스 버퍼. (행렬 16, 틴트 4 씩)
	//==============================================================================
	pub(crate) fn get_instance_buffer(&self) -> GlBuffer {
		self.instance_buffer
	}
}

//==============================================================================
// 영구 인스턴스 버퍼. (create_instance_buffer 의 결과, 모형마다 함께 묶은 버텍스 어레이를 처음 그릴 때 지어 둔다)
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
	// 인스턴스 꼴 반환.
	//==============================================================================
	pub fn get_layout(&self) -> &ToonInstanceLayout {
		&self.layout
	}

	//==============================================================================
	// 담은 수 개수.
	//==============================================================================
	pub fn get_float_count(&self) -> usize {
		self.float_count.get()
	}

	//==============================================================================
	// 담은 인스턴스 수. (수 개수를 꼴의 수 개수로 나눈 것)
	//==============================================================================
	pub fn get_instance_count(&self) -> usize {
		self.float_count.get() / self.layout.get_float_count().max(1)
	}
}

//==============================================================================
// 텍스처를 지을 때의 꼴. (밉맵, 되풀이, 고르게 읽기)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToonTextureOptions {
	pub is_mipmapped: bool,
	pub is_repeated: bool,
	pub is_filtered: bool,
}

impl Default for ToonTextureOptions {
	fn default() -> Self {
		ToonTextureOptions { is_mipmapped: true, is_repeated: true, is_filtered: true }
	}
}

//==============================================================================
// 그리기 전에 부르는 콜백. (프로그램, GL, 프로그램 종류, 머티리얼의 유니폼을 쓸 때)
//==============================================================================
pub type ToonBeforeDrawCallback<'a> = &'a dyn Fn(&ShaderProgram, &glow::Context, ToonProgramKind);

//==============================================================================
// 모형 하나를 그릴 때의 꼴.
//==============================================================================
#[derive(Clone, Copy)]
pub struct ToonDrawOptions<'a> {
	pub tint_color: [f32; 4],
	pub alpha: f32,
	pub joint_matrices: Option<&'a [f32]>,
	pub has_outline: bool,
	pub is_culled: bool,
	pub casts_shadow: bool,
	pub is_body_drawn: bool,
	pub tint_amount: Option<f32>,
	pub glow_scale: f32,
	pub color_scale: f32,
	pub emissive_color: [f32; 3],
	pub emissive_intensity: f32,
	pub base_color_texture: Option<GlTexture>,
	pub emissive_texture: Option<GlTexture>,
	pub uv_transform: [f32; 4],
	pub decal_texture: Option<GlTexture>,
	pub decal_color: [f32; 3],
	pub decal_emission: f32,
	pub decal_uv_transform: [f32; 4],
	pub alpha_cutoff: f32,
	pub blend_mode: ToonBlendMode,
	pub soft_edge: f32,
	pub is_mirrored: Option<bool>,
	pub index_start: usize,
	pub index_count: usize,
	pub outline_index_start: usize,
	pub outline_index_count: usize,
	pub material: Option<&'a ToonMaterial>,
	pub textures: &'a [(&'a str, GlTexture)],
	pub on_before_draw: Option<ToonBeforeDrawCallback<'a>>,
	pub light_set: Option<&'a ToonLightSet>,
	pub group: Option<u32>,
	pub is_depth_prepassed: bool,
}

impl Default for ToonDrawOptions<'_> {
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
		}
	}
}

//==============================================================================
// 꼴 글로 보이기. (콜백은 있는지만)
//==============================================================================
impl std::fmt::Debug for ToonDrawOptions<'_> {
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
// 인스턴스 하나. (모형 행렬과 틴트, 틴트의 알파는 바깥선에도 곱한다)
// - line_id 는 ToonScreenOutline::draw_mesh_instanced 가 읽는 이 인스턴스의 선 번호다. (원본 lineId, None 이면 공통 번호, ToonRenderer 는 읽지 않는다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonInstance {
	pub model_matrix: Matrix4,
	pub tint_color: [f32; 4],
	pub line_id: Option<u32>,
}

impl Default for ToonInstance {
	fn default() -> Self {
		ToonInstance { model_matrix: Matrix4::create_identity(), tint_color: [1.0, 1.0, 1.0, 1.0], line_id: None }
	}
}

//==============================================================================
// 받을 그림자. (깊이 텍스처, 빛의 보기와 투영, 해상도, 세기 0 ~ 1)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ToonShadowInput {
	depth_texture: GlTexture,
	light_view_projection_matrix: Matrix4,
	resolution: i32,
	strength: f32,
}

//==============================================================================
// 그린 수와 거른 수. (한 프레임, begin_frame 이 비운다)
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct ToonFrameStatistics {
	pub draw_call_count: u32,
	pub drawn_object_count: u32,
	pub culled_object_count: u32,
}

//==============================================================================
// 보기와 투영에서 읽은 카메라. (position 은 원근이면 [x, y, z, 1], 직교면 [0, 0, 0, 0], forward 는 보는 방향, is_mirrored 는 보기 행렬이 거울에 비쳤는지)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonFrameCamera {
	pub position: [f32; 4],
	pub forward: [f32; 3],
	pub is_mirrored: bool,
}

//==============================================================================
// begin_frame 앞의 그리기 상태.
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
}

//==============================================================================
// 프로그램 하나와 마지막으로 올린 프레임 값, 빛 묶음. (Cell 이라 빌린 채 고친다)
//==============================================================================
struct ToonProgramEntry {
	program: ShaderProgram,
	kind: ToonProgramKind,
	frame_serial: Cell<u64>,
	light_set_id: Cell<u32>,
	light_version: Cell<u64>,
}

impl ToonProgramEntry {
	fn new(program: ShaderProgram, kind: ToonProgramKind) -> ToonProgramEntry {
		ToonProgramEntry { program, kind, frame_serial: Cell::new(0), light_set_id: Cell::new(0), light_version: Cell::new(u64::MAX) }
	}
}

//==============================================================================
// 머티리얼과 인스턴스 꼴 하나의 세 프로그램.
//==============================================================================
struct ToonProgramSet {
	solid: ToonProgramEntry,
	outline: ToonProgramEntry,
	depth: ToonProgramEntry,
}

pub struct ToonRenderer {
	// (머티리얼 번호, 인스턴스 꼴 번호) → 세 프로그램. (짓지 못했으면 None, 다시 짓지 않는다)
	program_set_table: HashMap<(u32, u32), Option<ToonProgramSet>>,
	// 그림자를 받지 않을 때 그림자 유닛에 묶는 1x1 비교 깊이 텍스처. (깊이 1 이라 늘 빛을 받음)
	empty_shadow_texture: GlTexture,
	// 텍스처를 주지 않은 바탕색, 빛남, 데칼 유닛에 묶는 1x1 흰 텍스처.
	white_texture: GlTexture,
	view_projection_matrix: Matrix4,
	frustum_planes: [[f32; 4]; 6],
	camera_position: [f32; 4],
	camera_forward: [f32; 3],
	is_frame_mirrored: bool,
	light_set: ToonLightSet,
	outline_color: [f32; 4],
	is_hdr_output: bool,
	shadow_input: Option<ToonShadowInput>,
	is_shadow_pass: bool,
	// 프레임 값이 바뀔 때마다 오르는 번호. (프로그램마다 마지막으로 올린 번호와 견준다)
	frame_serial: u64,
	saved_state: Option<SavedState>,
	instance_data: Vec<f32>,
	frame_statistics: ToonFrameStatistics,
	screen_rect: Option<Rect>,
	is_scissor_applied: bool,
}

//==============================================================================
// f32, u32 배열을 바이트로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

fn index_slice_to_bytes(values: &[u32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
// 보기와 투영 행렬에서 절두체의 면 여섯. (ax + by + cz + d >= 0 이 안쪽, 정규화)
//==============================================================================
pub fn extract_frustum_planes(matrix: &Matrix4) -> [[f32; 4]; 6] {
	let elements = &matrix.elements;
	let row = |row_index: usize| [elements[row_index], elements[4 + row_index], elements[8 + row_index], elements[12 + row_index]];
	let row0 = row(0);
	let row1 = row(1);
	let row2 = row(2);
	let row3 = row(3);
	let combine = |first: [f32; 4], second: [f32; 4], sign: f32| [first[0] + second[0] * sign, first[1] + second[1] * sign, first[2] + second[2] * sign, first[3] + second[3] * sign];
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
// 세 벡터를 행으로 둔 3x3 의 행렬식.
//==============================================================================
fn read_determinant(first: [f32; 3], second: [f32; 3], third: [f32; 3]) -> f32 {
	first[0] * (second[1] * third[2] - second[2] * third[1]) - first[1] * (second[0] * third[2] - second[2] * third[0]) + first[2] * (second[0] * third[1] - second[1] * third[0])
}

//==============================================================================
// 모형 행렬 왼쪽 위 3x3 의 행렬식. (음수면 거울에 비친 행렬, 감기가 뒤집힌다)
//==============================================================================
pub fn read_matrix_determinant(matrix: &Matrix4) -> f32 {
	let elements = &matrix.elements;
	read_determinant([elements[0], elements[1], elements[2]], [elements[4], elements[5], elements[6]], [elements[8], elements[9], elements[10]])
}

//==============================================================================
// 보기와 투영 행렬에서 카메라 읽기. (원근이면 눈의 자리, 직교면 보는 방향, 행렬이 거울에 비친 것인지)
// - 원근: 클립 x, y, w 가 모두 0 이 되는 점이 눈이다. (행 0, 1, 3 의 연립 방정식)
// - 직교: 행 3 의 xyz 가 0 이라 눈이 무한히 멀다. 보는 방향은 행 0, 1 의 외적을 깊이가 느는 쪽으로 둔다.
// - 보통의 투영(가까운 면 < 먼 면)은 왼쪽 위 3x3 의 행렬식이 음수다. 양수면 보기 행렬이 거울에 비친 것이다.
//==============================================================================
pub fn read_frame_camera(matrix: &Matrix4) -> ToonFrameCamera {
	let elements = &matrix.elements;
	let row = |row_index: usize| [elements[row_index], elements[4 + row_index], elements[8 + row_index], elements[12 + row_index]];
	let row0 = row(0);
	let row1 = row(1);
	let row2 = row(2);
	let row3 = row(3);
	let xyz = |value: [f32; 4]| [value[0], value[1], value[2]];
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
	let position_x = read_determinant([constant[0], row0[1], row0[2]], [constant[1], row1[1], row1[2]], [constant[2], row3[1], row3[2]]) / determinant;
	let position_y = read_determinant([row0[0], constant[0], row0[2]], [row1[0], constant[1], row1[2]], [row3[0], constant[2], row3[2]]) / determinant;
	let position_z = read_determinant([row0[0], row0[1], constant[0]], [row1[0], row1[1], constant[1]], [row3[0], row3[1], constant[2]]) / determinant;
	ToonFrameCamera { position: [position_x, position_y, position_z, 1.0], forward, is_mirrored }
}

//==============================================================================
// 평면에 비친 행렬 짓기. (평면 위 한 점과 노멀, 평면 거울의 모형이나 보기 행렬에 곱한다)
// - 비친 행렬로 그리면 감기가 뒤집힌다. ToonRenderer 는 모형 행렬의 행렬식이 음수면 스스로 앞면을 뒤집는다. (is_mirrored 로 정할 수도 있다)
//==============================================================================
pub fn create_reflection_matrix(plane_point: [f32; 3], plane_normal: [f32; 3]) -> Matrix4 {
	let length = (plane_normal[0] * plane_normal[0] + plane_normal[1] * plane_normal[1] + plane_normal[2] * plane_normal[2]).sqrt();
	let (normal_x, normal_y, normal_z) = if length > 0.0 { (plane_normal[0] / length, plane_normal[1] / length, plane_normal[2] / length) } else { (0.0, 1.0, 0.0) };
	let distance = normal_x * plane_point[0] + normal_y * plane_point[1] + normal_z * plane_point[2];
	let mut matrix = Matrix4::create_identity();
	let elements = &mut matrix.elements;
	elements[0] = 1.0 - 2.0 * normal_x * normal_x;
	elements[1] = -2.0 * normal_x * normal_y;
	elements[2] = -2.0 * normal_x * normal_z;
	elements[4] = -2.0 * normal_y * normal_x;
	elements[5] = 1.0 - 2.0 * normal_y * normal_y;
	elements[6] = -2.0 * normal_y * normal_z;
	elements[8] = -2.0 * normal_z * normal_x;
	elements[9] = -2.0 * normal_z * normal_y;
	elements[10] = 1.0 - 2.0 * normal_z * normal_z;
	elements[12] = 2.0 * distance * normal_x;
	elements[13] = 2.0 * distance * normal_y;
	elements[14] = 2.0 * distance * normal_z;
	matrix
}

//==============================================================================
// 행렬의 가장 큰 크기. (세 축 길이 가운데 큰 것)
//==============================================================================
fn read_maximum_scale(matrix: &Matrix4) -> f32 {
	let elements = &matrix.elements;
	let scale_x = (elements[0] * elements[0] + elements[1] * elements[1] + elements[2] * elements[2]).sqrt();
	let scale_y = (elements[4] * elements[4] + elements[5] * elements[5] + elements[6] * elements[6]).sqrt();
	let scale_z = (elements[8] * elements[8] + elements[9] * elements[9] + elements[10] * elements[10]).sqrt();
	scale_x.max(scale_y).max(scale_z)
}

//==============================================================================
// 꼭짓점 배열의 경계 구. (자리 셋씩, 가운데는 상자의 가운데)
//==============================================================================
fn read_bounding_sphere(positions: &[f32], stride: usize) -> ([f32; 3], f32) {
	let mut minimum = [f32::MAX; 3];
	let mut maximum = [f32::MIN; 3];
	for point in positions.chunks(stride) {
		if point.len() < 3 {
			continue;
		}
		for axis in 0..3 {
			minimum[axis] = minimum[axis].min(point[axis]);
			maximum[axis] = maximum[axis].max(point[axis]);
		}
	}
	if minimum[0] > maximum[0] {
		return ([0.0; 3], 0.0);
	}
	let center = [(minimum[0] + maximum[0]) * 0.5, (minimum[1] + maximum[1]) * 0.5, (minimum[2] + maximum[2]) * 0.5];
	let mut radius_squared: f32 = 0.0;
	for point in positions.chunks(stride) {
		if point.len() < 3 {
			continue;
		}
		let delta = [point[0] - center[0], point[1] - center[1], point[2] - center[2]];
		radius_squared = radius_squared.max(delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]);
	}
	(center, radius_squared.sqrt())
}

//==============================================================================
// RGBA 텍스처 올리기. (create_texture 와 흰 텍스처가 함께 쓴다)
//==============================================================================
fn upload_texture(gl: &glow::Context, width: u32, height: u32, pixels: &[u8], options: ToonTextureOptions) -> Result<GlTexture, String> {
	let byte_count = width as usize * height as usize * 4;
	if width == 0 || height == 0 || pixels.len() < byte_count {
		return Err(format!("텍스처 바이트가 모자란다: {} x {} 에 {} 바이트", width, height, pixels.len()));
	}
	unsafe {
		let texture = gl.create_texture()?;
		gl.active_texture(glow::TEXTURE0 + BASE_COLOR_TEXTURE_UNIT);
		gl.bind_texture(glow::TEXTURE_2D, Some(texture));
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
		gl.bind_texture(glow::TEXTURE_2D, None);
		gl.active_texture(glow::TEXTURE0);
		Ok(texture)
	}
}

impl ToonRenderer {
	//==============================================================================
	// 셰이더 소스. ((정점, 조각) 세 벌: 몸, 바깥선, 그림자 깊이, 기본 프로그램)
	//==============================================================================
	pub fn read_program_sources() -> [(String, String); 3] {
		ToonRenderer::read_material_program_sources(None, None)
	}

	//==============================================================================
	// 머티리얼과 인스턴스 꼴의 셰이더 소스. ((정점, 조각) 세 벌, 어트리뷰트 자리는 compose_toon_program_sources 의 attribute_locations)
	//==============================================================================
	pub fn read_material_program_sources(material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> [(String, String); 3] {
		let sources = compose_toon_program_sources(material, instance_layout);
		[(sources.solid_vertex_shader, sources.solid_fragment_shader), (sources.outline_vertex_shader, sources.outline_fragment_shader), (sources.depth_vertex_shader, sources.depth_fragment_shader)]
	}

	//==============================================================================
	// 생성. (기본 세 프로그램을 짓는다)
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<ToonRenderer, String> {
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
			screen_rect: None,
			is_scissor_applied: false,
		})
	}

	//==============================================================================
	// 세 프로그램 짓기. (짓지 못하면 Err, 몸 프로그램의 텍스처 유닛을 정해 둔다)
	//==============================================================================
	fn create_program_set(gl: &glow::Context, material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> Result<ToonProgramSet, String> {
		let sources = compose_toon_program_sources(material, instance_layout);
		let attribute_locations: Vec<(&str, u32)> = sources.attribute_locations.iter().map(|(name, location)| (name.as_str(), *location)).collect();
		let solid_program = ShaderProgram::create_with_attribute_locations(gl, &sources.solid_vertex_shader, &sources.solid_fragment_shader, &attribute_locations)?;
		let outline_program = match ShaderProgram::create_with_attribute_locations(gl, &sources.outline_vertex_shader, &sources.outline_fragment_shader, &attribute_locations) {
			Ok(program) => {
				program
			}
			Err(error) => {
				unsafe {
					gl.delete_program(solid_program.get_program());
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
					gl.delete_program(solid_program.get_program());
					gl.delete_program(outline_program.get_program());
				}
				return Err(error);
			}
		};
		unsafe {
			let previous_program = crate::graphics3d::read_program_binding(gl);
			solid_program.use_program(gl);
			for (name, unit) in [("shadowTexture", SHADOW_TEXTURE_UNIT), ("baseColorTexture", BASE_COLOR_TEXTURE_UNIT), ("emissiveTexture", EMISSIVE_TEXTURE_UNIT), ("decalTexture", DECAL_TEXTURE_UNIT)] {
				gl.uniform_1_i32(solid_program.get_uniform_location(gl, name).as_ref(), unit as i32);
			}
			gl.use_program(previous_program);
		}
		Ok(ToonProgramSet {
			solid: ToonProgramEntry::new(solid_program, ToonProgramKind::Solid),
			outline: ToonProgramEntry::new(outline_program, ToonProgramKind::Outline),
			depth: ToonProgramEntry::new(depth_program, ToonProgramKind::Depth),
		})
	}

	//==============================================================================
	// 머티리얼과 인스턴스 꼴의 프로그램이 있게 하기. (없으면 짓고, 짓지 못한 것은 알리고 None 을 기억해 다시 짓지 않는다, 있으면 true)
	//==============================================================================
	fn ensure_program_set(&mut self, gl: &glow::Context, material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> bool {
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
	// 머티리얼 짓기. (셰이더 조각, toonmaterial.rs 머리말)
	//==============================================================================
	pub fn create_material(&self, chunks: ToonMaterialChunks) -> ToonMaterial {
		ToonMaterial::new(chunks)
	}

	//==============================================================================
	// 머티리얼의 프로그램을 미리 짓기. (처음 그릴 때의 멈춤을 피할 때, 짓지 못하면 false)
	//==============================================================================
	pub fn prepare_material(&mut self, gl: &glow::Context, material: Option<&ToonMaterial>, instance_layout: Option<&ToonInstanceLayout>) -> bool {
		self.ensure_program_set(gl, material, instance_layout)
	}

	//==============================================================================
	// 머티리얼의 프로그램 지우기. (모든 인스턴스 꼴의 것, 다시 그리면 새로 짓는다)
	//==============================================================================
	pub fn delete_material(&mut self, gl: &glow::Context, material: &ToonMaterial) {
		let material_id = material.get_id();
		let key_list: Vec<(u32, u32)> = self.program_set_table.keys().filter(|key| key.0 == material_id).copied().collect();
		for key in key_list {
			if let Some(Some(program_set)) = self.program_set_table.remove(&key) {
				ToonRenderer::delete_program_set(gl, &program_set);
			}
		}
	}

	//==============================================================================
	// 세 프로그램 지우기.
	//==============================================================================
	fn delete_program_set(gl: &glow::Context, program_set: &ToonProgramSet) {
		unsafe {
			gl.delete_program(program_set.solid.program.get_program());
			gl.delete_program(program_set.outline.program.get_program());
			gl.delete_program(program_set.depth.program.get_program());
		}
	}

	//==============================================================================
	// 빛 묶음 짓기. (그리기마다 ToonDrawOptions 의 light_set 으로 줄 것, 기본 빛과 같은 값으로 시작)
	//==============================================================================
	pub fn create_light_set(&self) -> ToonLightSet {
		ToonLightSet::new()
	}

	//==============================================================================
	// 기본 빛 묶음 반환.
	//==============================================================================
	pub fn get_light_set(&self) -> &ToonLightSet {
		&self.light_set
	}

	//==============================================================================
	// 기본 빛 묶음 반환. (고칠 때)
	//==============================================================================
	pub fn get_light_set_mut(&mut self) -> &mut ToonLightSet {
		&mut self.light_set
	}

	//==============================================================================
	// 인스턴스 꼴 짓기. ((이름, 크기) 목록, toonmaterial.rs 의 ToonInstanceLayout)
	//==============================================================================
	pub fn create_instance_layout(&self, attributes: &[(&str, usize)]) -> Result<ToonInstanceLayout, String> {
		ToonInstanceLayout::new(attributes)
	}

	//==============================================================================
	// 영구 인스턴스 버퍼 짓기. (꼴의 수 개수씩 담은 data, ToonBufferUsage::Static 이면 한 번 올리고 오래 그림)
	//==============================================================================
	pub fn create_instance_buffer(&self, gl: &glow::Context, instance_layout: &ToonInstanceLayout, data: &[f32], usage: ToonBufferUsage) -> Result<ToonInstanceBuffer, String> {
		let gl_usage = match usage {
			ToonBufferUsage::Static => {
				glow::STATIC_DRAW
			}
			ToonBufferUsage::Dynamic => {
				glow::DYNAMIC_DRAW
			}
		};
		unsafe {
			let buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(data), gl_usage);
			Ok(ToonInstanceBuffer { layout: instance_layout.clone(), buffer, usage: gl_usage, float_count: Cell::new(data.len()), vertex_array_list: RefCell::new(Vec::new()) })
		}
	}

	//==============================================================================
	// 인스턴스 버퍼 고치기. (전체를 다시 올림)
	//==============================================================================
	pub fn update_instance_buffer(&self, gl: &glow::Context, instance_buffer: &ToonInstanceBuffer, data: &[f32]) {
		unsafe {
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buffer.buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(data), instance_buffer.usage);
		}
		instance_buffer.float_count.set(data.len());
	}

	//==============================================================================
	// 인스턴스 버퍼 지우기. (그 버퍼로 지은 버텍스 어레이도, 모형보다 먼저 지운다)
	//==============================================================================
	pub fn delete_instance_buffer(&self, gl: &glow::Context, instance_buffer: ToonInstanceBuffer) {
		unsafe {
			for (_, body_vertex_array, outline_vertex_array) in instance_buffer.vertex_array_list.borrow().iter() {
				gl.delete_vertex_array(*body_vertex_array);
				if let Some(outline_vertex_array) = outline_vertex_array {
					gl.delete_vertex_array(*outline_vertex_array);
				}
			}
			gl.delete_buffer(instance_buffer.buffer);
		}
	}

	//==============================================================================
	// 텍스처 짓기. (RGBA 바이트 너비 x 높이 x 4, 그림 풀기는 decode_glb_image 나 image 크레이트)
	//==============================================================================
	pub fn create_texture(&self, gl: &glow::Context, width: u32, height: u32, pixels: &[u8], options: ToonTextureOptions) -> Result<GlTexture, String> {
		upload_texture(gl, width, height, pixels, options)
	}

	//==============================================================================
	// 텍스처 지우기.
	//==============================================================================
	pub fn delete_texture(&self, gl: &glow::Context, texture: GlTexture) {
		unsafe {
			gl.delete_texture(texture);
		}
	}

	//==============================================================================
	// 빈 그림자 텍스처 짓기. (1x1 깊이 1, 비교 샘플링)
	//==============================================================================
	fn create_empty_shadow_texture(gl: &glow::Context) -> Result<GlTexture, String> {
		unsafe {
			let texture = gl.create_texture()?;
			gl.active_texture(glow::TEXTURE0 + SHADOW_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(texture));
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
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
			Ok(texture)
		}
	}

	//==============================================================================
	// 렌더러 지우기. (모든 프로그램과 빈 텍스처, 모형과 인스턴스 버퍼와 텍스처는 따로)
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		for program_set in self.program_set_table.values().flatten() {
			ToonRenderer::delete_program_set(gl, program_set);
		}
		unsafe {
			gl.delete_texture(self.empty_shadow_texture);
			gl.delete_texture(self.white_texture);
		}
	}

	//==============================================================================
	// 꼭짓점 버퍼 만들기.
	//==============================================================================
	unsafe fn create_array_buffer(gl: &glow::Context, data: &[f32], usage: u32) -> Option<GlBuffer> {
		let buffer = gl.create_buffer().ok()?;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
		gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(data), usage);
		Some(buffer)
	}

	//==============================================================================
	// 번호 버퍼 만들기. (지금 묶인 버텍스 어레이에 붙는다)
	//==============================================================================
	unsafe fn create_index_buffer(gl: &glow::Context, data: &[u32]) -> Option<GlBuffer> {
		let buffer = gl.create_buffer().ok()?;
		gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(buffer));
		gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_slice_to_bytes(data), glow::STATIC_DRAW);
		Some(buffer)
	}

	//==============================================================================
	// 모형 올리기. (몸이 비면 None)
	//==============================================================================
	pub fn create_mesh(&self, gl: &glow::Context, arrays: &ToonMeshArrays) -> Option<ToonMesh> {
		if arrays.index_data.is_empty() {
			return None;
		}
		let is_skinned = !arrays.skin_data.is_empty();
		let vertex_count = arrays.vertex_data.len() / VERTEX_FLOAT_COUNT;
		let has_uv = vertex_count > 0 && arrays.uv_data.len() >= vertex_count * 2;
		let (bounding_center, mut bounding_radius) = read_bounding_sphere(&arrays.vertex_data, VERTEX_FLOAT_COUNT);
		if is_skinned {
			bounding_radius *= 1.5;
		}
		let mut buffer_list = Vec::new();
		unsafe {
			// 인스턴스 버퍼. (보통 그리기에서도 인스턴스 어트리뷰트가 켜져 있으므로 한 인스턴스 분량을 늘 채워 둔다, 비면 드라이버가 버퍼 밖을 읽는다)
			let mut default_instance = Matrix4::default().elements.to_vec();
			default_instance.extend_from_slice(&[1.0; 4]);
			let instance_buffer = ToonRenderer::create_array_buffer(gl, &default_instance, glow::DYNAMIC_DRAW)?;

			// 몸.
			let vertex_buffer = ToonRenderer::create_array_buffer(gl, &arrays.vertex_data, glow::STATIC_DRAW)?;
			buffer_list.push(vertex_buffer);
			let skin_buffer = if is_skinned { Some(ToonRenderer::create_array_buffer(gl, &arrays.skin_data, glow::STATIC_DRAW)?) } else { None };
			let uv_buffer = if has_uv { Some(ToonRenderer::create_array_buffer(gl, &arrays.uv_data[..vertex_count * 2], glow::STATIC_DRAW)?) } else { None };
			buffer_list.extend(skin_buffer.iter().chain(uv_buffer.iter()));
			let body_vertex_array = gl.create_vertex_array().ok()?;
			gl.bind_vertex_array(Some(body_vertex_array));
			let body_index_buffer = ToonRenderer::create_index_buffer(gl, &arrays.index_data)?;
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

			// 바깥선 껍데기. (자리만)
			if !arrays.outline_index_data.is_empty() {
				let outline_buffer = ToonRenderer::create_array_buffer(gl, &arrays.outline_vertex_data, glow::STATIC_DRAW)?;
				buffer_list.push(outline_buffer);
				if is_skinned && !arrays.outline_skin_data.is_empty() {
					let outline_skin_buffer = ToonRenderer::create_array_buffer(gl, &arrays.outline_skin_data, glow::STATIC_DRAW)?;
					buffer_list.push(outline_skin_buffer);
					mesh.outline_skin_buffer = Some(outline_skin_buffer);
				}
				let outline_vertex_array = gl.create_vertex_array().ok()?;
				gl.bind_vertex_array(Some(outline_vertex_array));
				let outline_index_buffer = ToonRenderer::create_index_buffer(gl, &arrays.outline_index_data)?;
				buffer_list.push(outline_index_buffer);
				mesh.outline_buffer = Some(outline_buffer);
				mesh.outline_index_buffer = Some(outline_index_buffer);
				ToonRenderer::attach_outline_attributes(gl, &mesh);
				ToonRenderer::attach_instance_attributes(gl, instance_buffer);
				mesh.outline_vertex_array = Some(outline_vertex_array);
				mesh.outline_index_count = arrays.outline_index_data.len() as i32;
			}
			gl.bind_vertex_array(None);
			mesh.buffer_list = buffer_list;
			Some(mesh)
		}
	}

	//==============================================================================
	// 몸의 꼭짓점, 뼈대, UV 어트리뷰트와 번호 버퍼를 지금 묶인 버텍스 어레이에 붙이기.
	//==============================================================================
	unsafe fn attach_body_attributes(gl: &glow::Context, mesh: &ToonMesh) {
		let stride = (VERTEX_FLOAT_COUNT * 4) as i32;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.vertex_buffer));
		for (location, size, offset) in [(0u32, 3i32, 0i32), (1, 3, 12), (2, 3, 24), (3, 2, 36), (4, 4, 44)] {
			gl.enable_vertex_attrib_array(location);
			gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
		}
		if let Some(skin_buffer) = mesh.skin_buffer {
			ToonRenderer::attach_skin_attributes(gl, skin_buffer);
		}
		if let Some(uv_buffer) = mesh.uv_buffer {
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(uv_buffer));
			gl.enable_vertex_attrib_array(UV_LOCATION);
			gl.vertex_attrib_pointer_f32(UV_LOCATION, 2, glow::FLOAT, false, 8, 0);
		}
		gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(mesh.body_index_buffer));
	}

	//==============================================================================
	// 바깥선 껍데기의 자리, 뼈대 어트리뷰트와 번호 버퍼를 지금 묶인 버텍스 어레이에 붙이기.
	//==============================================================================
	unsafe fn attach_outline_attributes(gl: &glow::Context, mesh: &ToonMesh) {
		gl.bind_buffer(glow::ARRAY_BUFFER, mesh.outline_buffer);
		gl.enable_vertex_attrib_array(0);
		gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);
		if let Some(outline_skin_buffer) = mesh.outline_skin_buffer {
			ToonRenderer::attach_skin_attributes(gl, outline_skin_buffer);
		}
		gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, mesh.outline_index_buffer);
	}

	//==============================================================================
	// 뼈대 번호와 무게의 버퍼를 지금 묶인 버텍스 어레이에 어트리뷰트로 붙이기.
	//==============================================================================
	unsafe fn attach_skin_attributes(gl: &glow::Context, skin_buffer: GlBuffer) {
		let stride = (SKIN_FLOAT_COUNT * 4) as i32;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(skin_buffer));
		gl.enable_vertex_attrib_array(5);
		gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, stride, 0);
		gl.enable_vertex_attrib_array(6);
		gl.vertex_attrib_pointer_f32(6, 4, glow::FLOAT, false, stride, 16);
	}

	//==============================================================================
	// 인스턴스 버퍼를 지금 묶인 버텍스 어레이의 인스턴스 어트리뷰트로. (행렬 네 줄, 틴트, 인스턴스마다 하나씩)
	//==============================================================================
	unsafe fn attach_instance_attributes(gl: &glow::Context, instance_buffer: GlBuffer) {
		let stride = (TOON_INSTANCE_FLOAT_COUNT * 4) as i32;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buffer));
		for column_index in 0..5u32 {
			let location = INSTANCE_MATRIX_LOCATION + column_index;
			gl.enable_vertex_attrib_array(location);
			gl.vertex_attrib_pointer_f32(location, 4, glow::FLOAT, false, stride, (column_index * 16) as i32);
			gl.vertex_attrib_divisor(location, 1);
		}
	}

	//==============================================================================
	// 인스턴스 꼴의 버퍼를 지금 묶인 버텍스 어레이의 인스턴스 어트리뷰트로.
	//==============================================================================
	unsafe fn attach_layout_attributes(gl: &glow::Context, instance_layout: &ToonInstanceLayout, buffer: GlBuffer) {
		let stride = (instance_layout.get_float_count() * 4) as i32;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
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
	// 모형과 인스턴스 버퍼를 함께 묶은 버텍스 어레이. (처음 그릴 때 짓고 인스턴스 버퍼에 담아 둔다, (몸, 바깥선))
	//==============================================================================
	fn read_instance_vertex_arrays(&self, gl: &glow::Context, mesh: &ToonMesh, instance_buffer: &ToonInstanceBuffer) -> Option<(GlVertexArray, Option<GlVertexArray>)> {
		if let Some((_, body_vertex_array, outline_vertex_array)) = instance_buffer.vertex_array_list.borrow().iter().find(|entry| entry.0 == mesh.mesh_id) {
			return Some((*body_vertex_array, *outline_vertex_array));
		}
		unsafe {
			let body_vertex_array = gl.create_vertex_array().ok()?;
			gl.bind_vertex_array(Some(body_vertex_array));
			ToonRenderer::attach_body_attributes(gl, mesh);
			ToonRenderer::attach_layout_attributes(gl, &instance_buffer.layout, instance_buffer.buffer);
			let mut outline_vertex_array = None;
			if mesh.outline_vertex_array.is_some() {
				let vertex_array = gl.create_vertex_array().ok()?;
				gl.bind_vertex_array(Some(vertex_array));
				ToonRenderer::attach_outline_attributes(gl, mesh);
				ToonRenderer::attach_layout_attributes(gl, &instance_buffer.layout, instance_buffer.buffer);
				outline_vertex_array = Some(vertex_array);
			}
			gl.bind_vertex_array(None);
			instance_buffer.vertex_array_list.borrow_mut().push((mesh.mesh_id, body_vertex_array, outline_vertex_array));
			Some((body_vertex_array, outline_vertex_array))
		}
	}

	//==============================================================================
	// 모형 지우기.
	//==============================================================================
	pub fn delete_mesh(&self, gl: &glow::Context, mesh: ToonMesh) {
		unsafe {
			gl.delete_vertex_array(mesh.body_vertex_array);
			if let Some(outline_vertex_array) = mesh.outline_vertex_array {
				gl.delete_vertex_array(outline_vertex_array);
			}
			gl.delete_buffer(mesh.instance_buffer);
			for buffer in mesh.buffer_list {
				gl.delete_buffer(buffer);
			}
		}
	}

	//==============================================================================
	// 그리기 전의 상태 적어 두기.
	//==============================================================================
	fn save_state(&mut self, gl: &glow::Context) {
		unsafe {
			let mut viewport = [0i32; 4];
			gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
			self.saved_state = Some(SavedState {
				is_blend_enabled: gl.is_enabled(glow::BLEND),
				is_depth_test_enabled: gl.is_enabled(glow::DEPTH_TEST),
				is_cull_face_enabled: gl.is_enabled(glow::CULL_FACE),
				depth_mask: gl.get_parameter_bool(glow::DEPTH_WRITEMASK),
				blend_source_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
				blend_destination_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
				blend_source_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
				blend_destination_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
				program: crate::graphics3d::read_program_binding(gl),
				vertex_array: crate::graphics3d::read_vertex_array_binding(gl),
				viewport,
				framebuffer: crate::graphics3d::read_framebuffer_binding(gl),
			});
		}
	}

	//==============================================================================
	// 적어 둔 상태로 돌리기.
	//==============================================================================
	fn restore_state(&mut self, gl: &glow::Context, is_framebuffer_restored: bool) {
		unsafe {
			gl.cull_face(glow::BACK);
			gl.front_face(glow::CCW);
			gl.color_mask(true, true, true, true);
			let saved_state = match self.saved_state.take() {
				Some(saved_state) => {
					saved_state
				}
				None => {
					return;
				}
			};
			if is_framebuffer_restored {
				gl.bind_framebuffer(glow::FRAMEBUFFER, saved_state.framebuffer);
				gl.viewport(saved_state.viewport[0], saved_state.viewport[1], saved_state.viewport[2], saved_state.viewport[3]);
			}
			gl.bind_vertex_array(saved_state.vertex_array);
			gl.use_program(saved_state.program);
			gl.depth_mask(saved_state.depth_mask);
			gl.blend_func_separate(saved_state.blend_source_rgb as u32, saved_state.blend_destination_rgb as u32, saved_state.blend_source_alpha as u32, saved_state.blend_destination_alpha as u32);
			for (capability, is_enabled) in [(glow::BLEND, saved_state.is_blend_enabled), (glow::DEPTH_TEST, saved_state.is_depth_test_enabled), (glow::CULL_FACE, saved_state.is_cull_face_enabled)] {
				if is_enabled {
					gl.enable(capability);
				}
				else {
					gl.disable(capability);
				}
			}
		}
	}

	//==============================================================================
	// 프레임의 그리기 상태 걸기. (섞기 끔, 깊이 시험 LEQUAL 과 쓰기, 뒷면 가리기, 앞면 CCW, ToonDrawQueue 가 단계 콜백 뒤에 다시 건다)
	//==============================================================================
	pub fn apply_frame_state(&self, gl: &glow::Context) {
		unsafe {
			gl.disable(glow::BLEND);
			gl.enable(glow::DEPTH_TEST);
			gl.depth_func(glow::LEQUAL);
			gl.depth_mask(true);
			gl.enable(glow::CULL_FACE);
			gl.cull_face(glow::BACK);
			gl.front_face(glow::CCW);
			gl.color_mask(true, true, true, true);
		}
	}

	//==============================================================================
	// 프레임 시작. (보기와 투영과 절두체를 정하고, 앞의 상태를 적어 둔 뒤 깊이 시험과 면 가리기를 켠다, 깊이를 비우는 것은 부르는 쪽이 정한다)
	//==============================================================================
	pub fn begin_frame(&mut self, gl: &glow::Context, view_projection_matrix: &Matrix4) {
		self.view_projection_matrix = *view_projection_matrix;
		self.frustum_planes = extract_frustum_planes(view_projection_matrix);
		let frame_camera = read_frame_camera(view_projection_matrix);
		self.camera_position = frame_camera.position;
		self.camera_forward = frame_camera.forward;
		self.is_frame_mirrored = frame_camera.is_mirrored;
		self.frame_statistics = ToonFrameStatistics::default();
		self.is_shadow_pass = false;
		self.frame_serial += 1;
		self.save_state(gl);
		self.apply_screen_rect(gl);
		self.apply_frame_state(gl);
	}

	//==============================================================================
	// 프레임 끝. (적어 둔 상태로 돌린다)
	//==============================================================================
	pub fn end_frame(&mut self, gl: &glow::Context) {
		self.release_screen_rect(gl);
		self.restore_state(gl, false);
	}

	//==============================================================================
	// 카메라로 프레임 시작. (Camera3D 의 보기와 투영으로 begin_frame)
	//==============================================================================
	pub fn begin_frame_with_camera(&mut self, gl: &glow::Context, camera: &Camera3D) {
		let view_projection_matrix = camera.get_view_projection_matrix();
		self.begin_frame(gl, &view_projection_matrix);
	}

	//==============================================================================
	// 보기와 투영 행렬로 프레임 시작. (Camera3D 없이, 투영 x 보기로 begin_frame)
	//==============================================================================
	pub fn begin_frame_with_matrices(&mut self, gl: &glow::Context, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		let mut view_projection_matrix = *projection_matrix;
		view_projection_matrix.multiply(view_matrix);
		self.begin_frame(gl, &view_projection_matrix);
	}

	//==============================================================================
	// 이번 프레임의 카메라 반환. (보기와 투영에서 읽은 것)
	//==============================================================================
	pub fn get_frame_camera(&self) -> ToonFrameCamera {
		ToonFrameCamera { position: self.camera_position, forward: self.camera_forward, is_mirrored: self.is_frame_mirrored }
	}

	//==============================================================================
	// 그릴 화면 사각형 설정. (뷰포트 픽셀, 왼쪽 위 원점, 다음 begin_frame 부터)
	//==============================================================================
	pub fn set_screen_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
		self.screen_rect = Some(Rect::create(x, y, width.max(0.0), height.max(0.0)));
	}

	//==============================================================================
	// 그릴 화면 사각형 풀기. (뷰포트 전체에 그림)
	//==============================================================================
	pub fn clear_screen_rect(&mut self) {
		self.screen_rect = None;
	}

	//==============================================================================
	// 그릴 화면 사각형 반환. (없으면 None)
	//==============================================================================
	pub fn get_screen_rect(&self) -> Option<Rect> {
		self.screen_rect
	}

	//==============================================================================
	// 화면 사각형 걸기. (begin_frame 이 상태를 적은 뒤, 클립 x, y 에 사각형의 비를 곱하고 자리를 더한 뒤 가위를 켠다)
	//==============================================================================
	fn apply_screen_rect(&mut self, gl: &glow::Context) {
		let (screen_rect, viewport) = match (self.screen_rect, self.saved_state.as_ref()) {
			(Some(screen_rect), Some(saved_state)) => {
				(screen_rect, saved_state.viewport)
			}
			_ => {
				return;
			}
		};
		let viewport_width = viewport[2].max(1) as f32;
		let viewport_height = viewport[3].max(1) as f32;
		let scale_x = screen_rect.size.x / viewport_width;
		let scale_y = screen_rect.size.y / viewport_height;
		let offset_x = (screen_rect.position.x * 2.0 + screen_rect.size.x) / viewport_width - 1.0;
		let offset_y = 1.0 - (screen_rect.position.y * 2.0 + screen_rect.size.y) / viewport_height;
		let elements = &mut self.view_projection_matrix.elements;
		for column_index in 0..4 {
			let clip_w = elements[column_index * 4 + 3];
			elements[column_index * 4] = elements[column_index * 4] * scale_x + clip_w * offset_x;
			elements[column_index * 4 + 1] = elements[column_index * 4 + 1] * scale_y + clip_w * offset_y;
		}
		let scissor_left = screen_rect.position.x.floor() as i32;
		let scissor_bottom = (viewport_height - screen_rect.position.y - screen_rect.size.y).floor() as i32;
		let scissor_right = (screen_rect.position.x + screen_rect.size.x).ceil() as i32;
		let scissor_top = (viewport_height - screen_rect.position.y).ceil() as i32;
		unsafe {
			gl.enable(glow::SCISSOR_TEST);
			gl.scissor(viewport[0] + scissor_left, viewport[1] + scissor_bottom, scissor_right - scissor_left, scissor_top - scissor_bottom);
		}
		self.is_scissor_applied = true;
	}

	//==============================================================================
	// 화면 사각형의 가위 풀기. (apply_screen_rect 가 켰을 때만)
	//==============================================================================
	fn release_screen_rect(&mut self, gl: &glow::Context) {
		if !self.is_scissor_applied {
			return;
		}
		unsafe {
			gl.disable(glow::SCISSOR_TEST);
		}
		self.is_scissor_applied = false;
	}

	//==============================================================================
	// 그림자 깊이 패스 시작. (ShadowMap 의 프레임버퍼에 빛의 보기와 투영으로, 앞의 프레임버퍼와 뷰포트는 end_shadow_pass 가 돌린다)
	//==============================================================================
	pub fn begin_shadow_pass(&mut self, gl: &glow::Context, shadow_map: &ShadowMap) {
		self.save_state(gl);
		self.view_projection_matrix = *shadow_map.get_light_view_projection_matrix();
		self.frustum_planes = extract_frustum_planes(&self.view_projection_matrix);
		self.is_shadow_pass = true;
		self.is_frame_mirrored = false;
		self.frame_serial += 1;
		shadow_map.begin_render(gl);
		unsafe {
			gl.disable(glow::BLEND);
			gl.depth_func(glow::LEQUAL);
			gl.enable(glow::CULL_FACE);
			gl.cull_face(glow::BACK);
		}
	}

	//==============================================================================
	// 그림자 깊이 패스 끝. (프레임버퍼와 뷰포트와 상태를 돌린다)
	//==============================================================================
	pub fn end_shadow_pass(&mut self, gl: &glow::Context) {
		self.is_shadow_pass = false;
		self.frame_serial += 1;
		self.restore_state(gl, true);
	}

	//==============================================================================
	// 경계 구가 절두체 안에 드는지. (모형 행렬로 옮긴 가운데, 가장 큰 크기를 곱한 반지름)
	//==============================================================================
	pub fn is_visible(&self, mesh: &ToonMesh, model_matrix: &Matrix4) -> bool {
		let center = mesh.bounding_center;
		let transformed_center = model_matrix.transform_point(center[0], center[1], center[2]);
		let radius = mesh.bounding_radius * read_maximum_scale(model_matrix);
		self.frustum_planes.iter().all(|plane| plane[0] * transformed_center.x + plane[1] * transformed_center.y + plane[2] * transformed_center.z + plane[3] >= -radius)
	}

	//==============================================================================
	// 모형 하나 그리기. (그림자 패스면 깊이만, 화면 밖이면 건너뛴다)
	//==============================================================================
	pub fn draw_mesh(&mut self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions) {
		if options.is_culled && !self.is_visible(mesh, model_matrix) {
			self.frame_statistics.culled_object_count += 1;
			return;
		}
		self.frame_statistics.drawn_object_count += 1;
		self.draw_with_count(gl, mesh, model_matrix, 0, options, None);
	}

	//==============================================================================
	// 같은 모형 여럿을 한 번에 그리기. (GPU 인스턴싱, 화면 밖 인스턴스는 걸러 남은 것만 올린다)
	// - 뼈대 모형은 모든 인스턴스가 같은 자세다. (options 의 joint_matrices)
	// - 감기는 첫 인스턴스의 행렬로 가린다. (is_mirrored 를 주면 그것)
	//==============================================================================
	pub fn draw_mesh_instanced(&mut self, gl: &glow::Context, mesh: &ToonMesh, instance_list: &[ToonInstance], options: ToonDrawOptions) {
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
			instance_data.extend_from_slice(&instance.model_matrix.elements);
			instance_data.extend_from_slice(&instance.tint_color);
			instance_count += 1;
		}
		self.frame_statistics.drawn_object_count += instance_count;
		if instance_count > 0 {
			unsafe {
				gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.instance_buffer));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&instance_data), glow::DYNAMIC_DRAW);
			}
			self.draw_with_count(gl, mesh, &first_matrix, instance_count as i32, options, None);
		}
		self.instance_data = instance_data;
	}

	//==============================================================================
	// 영구 인스턴스 버퍼로 그리기. (거르지 않음, instance_count 개, 꼴에 행렬이 없으면 정점 조각이 자리를 정한다)
	//==============================================================================
	pub fn draw_mesh_instance_buffer(&mut self, gl: &glow::Context, mesh: &ToonMesh, instance_buffer: &ToonInstanceBuffer, instance_count: usize, options: ToonDrawOptions) {
		let count = instance_count.min(instance_buffer.get_instance_count());
		if count == 0 {
			return;
		}
		self.frame_statistics.drawn_object_count += count as u32;
		self.draw_with_count(gl, mesh, &Matrix4::create_identity(), count as i32, options, Some(instance_buffer));
	}

	//==============================================================================
	// 모형 하나의 바깥선만 그리기. (거르지도 세지도 않는다, 껍데기가 없거나 has_outline 이 false 거나 그림자 패스면 그리지 않는다)
	//==============================================================================
	pub fn draw_mesh_outline(&mut self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions) {
		if mesh.outline_vertex_array.is_none() || !options.has_outline || self.is_shadow_pass {
			return;
		}
		self.draw_with_count(gl, mesh, model_matrix, 0, ToonDrawOptions { is_body_drawn: false, ..options }, None);
	}

	//==============================================================================
	// 프로그램의 프레임 값 올리기. (보기와 투영, 카메라, HDR, 그림자, 바깥선 색, 이번 프레임에 이미 올렸으면 건너뛴다)
	//==============================================================================
	fn upload_frame_uniforms(&self, gl: &glow::Context, entry: &ToonProgramEntry) {
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
					let outline_color = self.outline_color;
					gl.uniform_4_f32(program.get_uniform_location(gl, "outlineColor").as_ref(), outline_color[0], outline_color[1], outline_color[2], outline_color[3]);
					gl.uniform_1_f32(program.get_uniform_location(gl, "hdrOutput").as_ref(), hdr_output);
				}
				ToonProgramKind::Solid => {
					let camera_position = self.camera_position;
					let camera_forward = self.camera_forward;
					gl.uniform_4_f32(program.get_uniform_location(gl, "cameraPosition").as_ref(), camera_position[0], camera_position[1], camera_position[2], camera_position[3]);
					gl.uniform_3_f32(program.get_uniform_location(gl, "cameraForward").as_ref(), camera_forward[0], camera_forward[1], camera_forward[2]);
					gl.uniform_1_f32(program.get_uniform_location(gl, "hdrOutput").as_ref(), hdr_output);
					match self.shadow_input {
						Some(shadow_input) => {
							gl.uniform_1_f32(program.get_uniform_location(gl, "shadowEnabled").as_ref(), 1.0);
							gl.uniform_1_f32(program.get_uniform_location(gl, "shadowStrength").as_ref(), shadow_input.strength);
							gl.uniform_1_f32(program.get_uniform_location(gl, "shadowTexelSize").as_ref(), 1.0 / shadow_input.resolution.max(1) as f32);
							gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "lightViewProjectionMatrix").as_ref(), false, &shadow_input.light_view_projection_matrix.elements);
						}
						None => {
							gl.uniform_1_f32(program.get_uniform_location(gl, "shadowEnabled").as_ref(), 0.0);
						}
					}
				}
				ToonProgramKind::Depth => {}
			}
		}
	}

	//==============================================================================
	// 몸 프로그램의 빛 묶음 올리기. (같은 묶음, 같은 판이면 건너뛴다)
	//==============================================================================
	fn upload_light_uniforms(gl: &glow::Context, entry: &ToonProgramEntry, light_set: &ToonLightSet) {
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
		let (rim_color, rim_power) = light_set.get_rim_light();
		let (specular_strength, specular_exponent) = light_set.get_specular();
		let band_ramp = light_set.get_band_ramp();
		let mut threshold_values = [0.0f32; TOON_MAXIMUM_BAND_STEPS - 1];
		let mut band_values = [0.0f32; TOON_MAXIMUM_BAND_STEPS];
		for (index, threshold) in band_ramp.thresholds.iter().take(threshold_values.len()).enumerate() {
			threshold_values[index] = *threshold;
		}
		for (index, value) in band_ramp.values.iter().take(band_values.len()).enumerate() {
			band_values[index] = *value;
		}
		let point_light_count = light_set.get_point_light_count();
		let mut position_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 4];
		let mut color_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 3];
		let mut shape_values = [0.0f32; TOON_MAXIMUM_POINT_LIGHTS * 3];
		for (light_index, point_light) in light_set.get_point_lights().iter().take(point_light_count).enumerate() {
			position_values[light_index * 4..light_index * 4 + 4].copy_from_slice(&[point_light.position[0], point_light.position[1], point_light.position[2], point_light.range]);
			color_values[light_index * 3..light_index * 3 + 3].copy_from_slice(&[point_light.color[0] * point_light.intensity, point_light.color[1] * point_light.intensity, point_light.color[2] * point_light.intensity]);
			shape_values[light_index * 3..light_index * 3 + 3].copy_from_slice(&[point_light.decay, point_light.range_exponent, point_light.wrap]);
		}
		unsafe {
			gl.uniform_3_f32(program.get_uniform_location(gl, "sunDirection").as_ref(), sun_direction[0], sun_direction[1], sun_direction[2]);
			gl.uniform_3_f32(program.get_uniform_location(gl, "sunColor").as_ref(), sun_color[0], sun_color[1], sun_color[2]);
			gl.uniform_3_f32(program.get_uniform_location(gl, "skyColor").as_ref(), sky_color[0], sky_color[1], sky_color[2]);
			gl.uniform_3_f32(program.get_uniform_location(gl, "groundColor").as_ref(), ground_color[0], ground_color[1], ground_color[2]);
			gl.uniform_3_f32(program.get_uniform_location(gl, "fillDirection").as_ref(), fill_direction[0], fill_direction[1], fill_direction[2]);
			gl.uniform_3_f32(program.get_uniform_location(gl, "fillColor").as_ref(), fill_color[0], fill_color[1], fill_color[2]);
			gl.uniform_4_f32(program.get_uniform_location(gl, "rimLight").as_ref(), rim_color[0], rim_color[1], rim_color[2], rim_power);
			gl.uniform_2_f32(program.get_uniform_location(gl, "specularLight").as_ref(), specular_strength, specular_exponent);
			gl.uniform_1_f32(program.get_uniform_location(gl, "energyNormalized").as_ref(), if light_set.get_light_model().is_energy_normalized { 1.0 } else { 0.0 });
			gl.uniform_1_f32(program.get_uniform_location(gl, "bandCount").as_ref(), light_set.get_band_count() as f32);
			gl.uniform_1_i32(program.get_uniform_location(gl, "bandRampCount").as_ref(), band_ramp.values.len() as i32);
			gl.uniform_1_f32(program.get_uniform_location(gl, "bandRampHalfLambert").as_ref(), if band_ramp.is_half_lambert { 1.0 } else { 0.0 });
			gl.uniform_1_f32_slice(program.get_uniform_location(gl, "bandRampThresholds").as_ref(), &threshold_values);
			gl.uniform_1_f32_slice(program.get_uniform_location(gl, "bandRampValues").as_ref(), &band_values);
			gl.uniform_1_i32(program.get_uniform_location(gl, "pointLightCount").as_ref(), point_light_count as i32);
			gl.uniform_4_f32_slice(program.get_uniform_location(gl, "pointLightPositions").as_ref(), &position_values);
			gl.uniform_3_f32_slice(program.get_uniform_location(gl, "pointLightColors").as_ref(), &color_values);
			gl.uniform_3_f32_slice(program.get_uniform_location(gl, "pointLightShapes").as_ref(), &shape_values);
		}
	}

	//==============================================================================
	// 그리기마다의 공용 유니폼 올리기. (모형 행렬, 뼈대, 인스턴싱 0 아님, 1 인스턴스 행렬과 틴트, 2 인스턴스 틴트만)
	//==============================================================================
	fn upload_draw_uniforms(gl: &glow::Context, program: &ShaderProgram, model_matrix: &Matrix4, is_skinned: bool, instanced_mode: f32, joint_matrices: Option<&[f32]>) {
		unsafe {
			gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "modelMatrix").as_ref(), false, &model_matrix.elements);
			gl.uniform_1_f32(program.get_uniform_location(gl, "instanced").as_ref(), instanced_mode);
			gl.uniform_1_f32(program.get_uniform_location(gl, "skinned").as_ref(), if is_skinned { 1.0 } else { 0.0 });
			if let (true, Some(joint_matrices)) = (is_skinned, joint_matrices) {
				let joint_float_count = joint_matrices.len().min(TOON_MAXIMUM_JOINTS * 16) / 16 * 16;
				gl.uniform_matrix_4_f32_slice(program.get_uniform_location(gl, "jointMatrices").as_ref(), false, &joint_matrices[..joint_float_count]);
			}
		}
	}

	//==============================================================================
	// 텍스처 유닛에 묶기. (없으면 흰 텍스처)
	//==============================================================================
	fn bind_texture_unit(&self, gl: &glow::Context, unit: u32, texture: Option<GlTexture>) {
		unsafe {
			gl.active_texture(glow::TEXTURE0 + unit);
			gl.bind_texture(glow::TEXTURE_2D, Some(texture.unwrap_or(self.white_texture)));
		}
	}

	//==============================================================================
	// 그리기 본체. (instance_count 가 0 이면 보통, 아니면 인스턴스로, instance_buffer 가 있으면 그 꼴로)
	//==============================================================================
	fn draw_with_count(&mut self, gl: &glow::Context, mesh: &ToonMesh, model_matrix: &Matrix4, instance_count: i32, options: ToonDrawOptions, instance_buffer: Option<&ToonInstanceBuffer>) {
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
						gl.vertex_attrib_4_f32(INSTANCE_TINT_LOCATION, 1.0, 1.0, 1.0, 1.0);
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
		let mut draw_elements = |gl: &glow::Context, start: usize, count: usize| {
			if count == 0 {
				return;
			}
			unsafe {
				if is_instanced {
					gl.draw_elements_instanced(glow::TRIANGLES, count as i32, glow::UNSIGNED_INT, (start * 4) as i32, instance_count);
				}
				else {
					gl.draw_elements(glow::TRIANGLES, count as i32, glow::UNSIGNED_INT, (start * 4) as i32);
				}
			}
			draw_call_count += 1;
		};
		unsafe {
			gl.front_face(if is_mirrored { glow::CW } else { glow::CCW });
		}

		// 그림자 패스. (몸의 깊이만)
		if self.is_shadow_pass {
			let depth_entry = &program_set.depth;
			depth_entry.program.use_program(gl);
			self.upload_frame_uniforms(gl, depth_entry);
			ToonRenderer::upload_draw_uniforms(gl, &depth_entry.program, model_matrix, is_skinned, instanced_mode, options.joint_matrices);
			if let Some(on_before_draw) = options.on_before_draw {
				on_before_draw(&depth_entry.program, gl, ToonProgramKind::Depth);
			}
			unsafe {
				gl.bind_vertex_array(Some(body_vertex_array));
			}
			draw_elements(gl, index_start, index_count);
			unsafe {
				gl.front_face(glow::CCW);
			}
			self.frame_statistics.draw_call_count += draw_call_count;
			return;
		}

		let is_transparent = options.alpha < 1.0 || options.tint_color[3] < 1.0 || options.blend_mode != ToonBlendMode::Alpha;
		let premultiplied_output = if options.blend_mode != ToonBlendMode::Alpha { 1.0 } else { 0.0 };
		unsafe {
			if is_transparent {
				gl.enable(glow::BLEND);
				match options.blend_mode {
					ToonBlendMode::Additive => {
						gl.blend_func(glow::ONE, glow::ONE);
					}
					ToonBlendMode::Premultiplied => {
						gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
					}
					ToonBlendMode::Alpha => {
						gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
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
			ToonRenderer::upload_draw_uniforms(gl, outline_program, model_matrix, is_skinned, instanced_mode, options.joint_matrices);
			unsafe {
				gl.uniform_1_f32(outline_program.get_uniform_location(gl, "alpha").as_ref(), options.alpha);
				gl.uniform_1_f32(outline_program.get_uniform_location(gl, "premultipliedOutput").as_ref(), premultiplied_output);
			}
			if let Some(on_before_draw) = options.on_before_draw {
				on_before_draw(outline_program, gl, ToonProgramKind::Outline);
			}
			unsafe {
				gl.cull_face(glow::FRONT);
				gl.bind_vertex_array(Some(outline_vertex_array));
			}
			draw_elements(gl, outline_index_start, outline_index_count);
		}
		if !options.is_body_drawn {
			unsafe {
				if is_transparent {
					gl.disable(glow::BLEND);
				}
				gl.cull_face(glow::BACK);
				gl.front_face(glow::CCW);
			}
			self.frame_statistics.draw_call_count += draw_call_count;
			return;
		}

		// 몸.
		let solid_entry = &program_set.solid;
		let solid_program = &solid_entry.program;
		solid_program.use_program(gl);
		self.upload_frame_uniforms(gl, solid_entry);
		ToonRenderer::upload_light_uniforms(gl, solid_entry, options.light_set.unwrap_or(&self.light_set));
		ToonRenderer::upload_draw_uniforms(gl, solid_program, model_matrix, is_skinned, instanced_mode, options.joint_matrices);
		let tint_color = options.tint_color;
		let emissive_color = options.emissive_color;
		let decal_color = options.decal_color;
		let uv_transform = options.uv_transform;
		let decal_uv_transform = options.decal_uv_transform;
		let enabled = |texture: Option<GlTexture>| if texture.is_some() { 1.0 } else { 0.0 };
		unsafe {
			gl.uniform_4_f32(solid_program.get_uniform_location(gl, "tintColor").as_ref(), tint_color[0], tint_color[1], tint_color[2], tint_color[3]);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "tintAmount").as_ref(), options.tint_amount.map(|tint_amount| tint_amount.clamp(0.0, 1.0)).unwrap_or(-1.0));
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "alpha").as_ref(), options.alpha);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "glowScale").as_ref(), options.glow_scale);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "colorScale").as_ref(), options.color_scale);
			gl.uniform_4_f32(solid_program.get_uniform_location(gl, "emissiveColor").as_ref(), emissive_color[0], emissive_color[1], emissive_color[2], options.emissive_intensity);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "softEdge").as_ref(), options.soft_edge);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "alphaCutoff").as_ref(), options.alpha_cutoff);
			gl.uniform_1_f32(solid_program.get_uniform_location(gl, "premultipliedOutput").as_ref(), premultiplied_output);
			gl.uniform_3_f32(solid_program.get_uniform_location(gl, "textureEnabled").as_ref(), enabled(options.base_color_texture), enabled(options.emissive_texture), enabled(options.decal_texture));
			gl.uniform_4_f32(solid_program.get_uniform_location(gl, "uvTransform").as_ref(), uv_transform[0], uv_transform[1], uv_transform[2], uv_transform[3]);
			gl.uniform_4_f32(solid_program.get_uniform_location(gl, "decalUvTransform").as_ref(), decal_uv_transform[0], decal_uv_transform[1], decal_uv_transform[2], decal_uv_transform[3]);
			gl.uniform_4_f32(solid_program.get_uniform_location(gl, "decalColor").as_ref(), decal_color[0], decal_color[1], decal_color[2], options.decal_emission);
			gl.active_texture(glow::TEXTURE0 + SHADOW_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(self.shadow_input.map(|shadow_input| shadow_input.depth_texture).unwrap_or(self.empty_shadow_texture)));
		}
		self.bind_texture_unit(gl, BASE_COLOR_TEXTURE_UNIT, options.base_color_texture);
		self.bind_texture_unit(gl, EMISSIVE_TEXTURE_UNIT, options.emissive_texture);
		self.bind_texture_unit(gl, DECAL_TEXTURE_UNIT, options.decal_texture);
		for (texture_index, (uniform_name, texture)) in options.textures.iter().take(TOON_MAXIMUM_MATERIAL_TEXTURES).enumerate() {
			let unit = FIRST_MATERIAL_TEXTURE_UNIT + texture_index as u32;
			self.bind_texture_unit(gl, unit, Some(*texture));
			unsafe {
				gl.uniform_1_i32(solid_program.get_uniform_location(gl, uniform_name).as_ref(), unit as i32);
			}
		}
		unsafe {
			gl.active_texture(glow::TEXTURE0);
		}
		if let Some(on_before_draw) = options.on_before_draw {
			on_before_draw(solid_program, gl, ToonProgramKind::Solid);
		}
		unsafe {
			gl.cull_face(glow::BACK);
			gl.bind_vertex_array(Some(body_vertex_array));
		}
		draw_elements(gl, index_start, index_count);
		unsafe {
			if is_transparent {
				gl.disable(glow::BLEND);
			}
			gl.front_face(glow::CCW);
		}
		self.frame_statistics.draw_call_count += draw_call_count;
	}

	//==============================================================================
	// 빛 설정. (해가 비추는 방향, 해의 색, 하늘색, 땅색, 모두 선형 색 0 ~, HDR 이면 1 을 넘어도 된다, 기본 빛 묶음)
	//==============================================================================
	pub fn set_light(&mut self, sun_direction: [f32; 3], sun_color: [f32; 3], sky_color: [f32; 3], ground_color: [f32; 3]) {
		self.light_set.set_light(sun_direction, sun_color, sky_color, ground_color);
	}

	//==============================================================================
	// 해가 비추는 방향 설정. (빛이 나아가는 쪽, toonlightset.rs 머리말)
	//==============================================================================
	pub fn set_sun_direction(&mut self, sun_direction: [f32; 3]) {
		self.light_set.set_sun_direction(sun_direction);
	}

	//==============================================================================
	// 해가 비추는 방향 반환. (ShadowMap::update_light_matrix 에 그대로 넘긴다)
	//==============================================================================
	pub fn get_sun_direction(&self) -> [f32; 3] {
		self.light_set.get_sun_direction()
	}

	//==============================================================================
	// 채움 빛 설정. (빛이 나아가는 방향과 선형 색)
	//==============================================================================
	pub fn set_fill_light(&mut self, fill_direction: [f32; 3], fill_color: [f32; 3]) {
		self.light_set.set_fill_light(fill_direction, fill_color);
	}

	//==============================================================================
	// 림 빛 설정. (선형 색과 거듭제곱)
	//==============================================================================
	pub fn set_rim_light(&mut self, rim_color: [f32; 3], rim_power: f32) {
		self.light_set.set_rim_light(rim_color, rim_power);
	}

	//==============================================================================
	// 정반사 설정. (세기와 지수, 원본 지수 기본 48)
	//==============================================================================
	pub fn set_specular(&mut self, specular_strength: f32, specular_exponent: f32) {
		self.light_set.set_specular(specular_strength, specular_exponent);
	}

	//==============================================================================
	// 점광 설정. (ToonLightSet::set_point_lights)
	//==============================================================================
	pub fn set_point_lights(&mut self, point_light_list: &[ToonPointLight]) {
		self.light_set.set_point_lights(point_light_list);
	}

	//==============================================================================
	// 점광 수의 한도 설정.
	//==============================================================================
	pub fn set_point_light_limit(&mut self, point_light_limit: usize) {
		self.light_set.set_point_light_limit(point_light_limit);
	}

	//==============================================================================
	// 셀 셰이딩 칸 수 설정. (0 이면 부드럽게)
	//==============================================================================
	pub fn set_band_count(&mut self, band_count: u32) {
		self.light_set.set_band_count(band_count);
	}

	//==============================================================================
	// 띠 램프 설정. (ToonLightSet::set_band_ramp)
	//==============================================================================
	pub fn set_band_ramp(&mut self, thresholds: &[f32], values: &[f32], is_half_lambert: bool) {
		self.light_set.set_band_ramp(thresholds, values, is_half_lambert);
	}

	//==============================================================================
	// 띠 램프 끄기.
	//==============================================================================
	pub fn clear_band_ramp(&mut self) {
		self.light_set.clear_band_ramp();
	}

	//==============================================================================
	// 빛 꼴 설정.
	//==============================================================================
	pub fn set_light_model(&mut self, light_model: ToonLightModel) {
		self.light_set.set_light_model(light_model);
	}

	//==============================================================================
	// 바깥선 색 설정. (sRGB 0 ~ 1 과 알파)
	//==============================================================================
	pub fn set_outline_color(&mut self, outline_color: [f32; 4]) {
		self.outline_color = outline_color;
		self.frame_serial += 1;
	}

	//==============================================================================
	// HDR 출력 설정. (true 면 감마를 걸지 않은 선형 색을 낸다, ToonPostProcess 의 실수 렌더 타겟에 그릴 때)
	//==============================================================================
	pub fn set_hdr_output(&mut self, is_hdr_output: bool) {
		self.is_hdr_output = is_hdr_output;
		self.frame_serial += 1;
	}

	//==============================================================================
	// 받을 그림자 설정. (ShadowMap 이나 None, 세기 0 ~ 1, 원본 기본 세기는 0.6)
	//==============================================================================
	pub fn set_shadow(&mut self, shadow_map: Option<&ShadowMap>, strength: f32) {
		self.frame_serial += 1;
		self.shadow_input = shadow_map.map(|shadow_map| {
			ToonShadowInput { depth_texture: shadow_map.get_depth_texture(), light_view_projection_matrix: *shadow_map.get_light_view_projection_matrix(), resolution: shadow_map.get_resolution(), strength }
		});
	}

	//==============================================================================
	// 이번 프레임의 그린 수와 거른 수.
	//==============================================================================
	pub fn get_frame_statistics(&self) -> ToonFrameStatistics {
		self.frame_statistics
	}

	//==============================================================================
	// 머티리얼과 인스턴스 꼴의 프로그램 반환. (지은 것만, 원본 getSolidProgram(material) 들)
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
	// 기본 프로그램 셋 반환. (생성에서 지어 늘 있다)
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
	// 몸 프로그램 반환. (기본 프로그램)
	//==============================================================================
	pub fn get_solid_program(&self) -> &ShaderProgram {
		&self.read_default_program_set().solid.program
	}

	//==============================================================================
	// 바깥선 프로그램 반환. (기본 프로그램)
	//==============================================================================
	pub fn get_outline_program(&self) -> &ShaderProgram {
		&self.read_default_program_set().outline.program
	}

	//==============================================================================
	// 그림자 깊이 프로그램 반환. (기본 프로그램)
	//==============================================================================
	pub fn get_depth_program(&self) -> &ShaderProgram {
		&self.read_default_program_set().depth.program
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::vector3::Vector3;

	#[test]
	fn frustum_keeps_center_and_drops_behind() {
		let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 10.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let mut view_projection_matrix = Matrix4::create_perspective(0.8, 1.0, 0.1, 100.0);
		view_projection_matrix.multiply(&view_matrix);
		let planes = extract_frustum_planes(&view_projection_matrix);
		let is_inside = |point: [f32; 3], radius: f32| planes.iter().all(|plane| plane[0] * point[0] + plane[1] * point[1] + plane[2] * point[2] + plane[3] >= -radius);
		assert!(is_inside([0.0, 0.0, 0.0], 1.0));
		assert!(!is_inside([0.0, 0.0, 20.0], 1.0));
		assert!(!is_inside([50.0, 0.0, 0.0], 1.0));
		assert!(is_inside([5.0, 0.0, 0.0], 2.0));
	}

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

	#[test]
	fn reflection_matrix_flips_winding() {
		let reflection_matrix = create_reflection_matrix([0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
		assert!(read_matrix_determinant(&reflection_matrix) < 0.0);
		let point = reflection_matrix.transform_point(0.5, 3.0, -2.0);
		assert!((point.x - 0.5).abs() < 1e-6 && (point.y + 1.0).abs() < 1e-6 && (point.z + 2.0).abs() < 1e-6);
	}

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
