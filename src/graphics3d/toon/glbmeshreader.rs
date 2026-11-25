//==============================================================================
// 점토 모형(GLB) 읽기와 뼈대 자세. (vanilla.js experimental/graphics/toon/glbmeshreader.js)
// - 재질 색으로 칠하는 모형을 툰 3D 의 MeshBuilder 꼭짓점 배열로 읽는다. 텍스처 좌표와 재질 텍스처가 있으면 그것도 읽는다. (외계행성 광부 키우기에서 왔다)
//   - 부품 하나가 노드 하나이고, 노드의 메시는 색마다 프리미티브 하나다. 꼭짓점은 위치와 노멀, 뼈대가 있으면 본 번호와 무게다.
//   - 재질의 baseColorFactor 는 선형 색이라 sRGB 로 바꿔 담고, emissiveFactor 가 있으면 빛남으로 담는다. (emissiveTexture 가 있으면 빛남은 재질 범위로만 넘긴다)
//   - TEXCOORD_0 이 있으면 uv_data 로, TEXCOORD_1 이 있으면 uv1_data(2 번 UV, 천사 고리)로 담는다. 프리미티브마다 재질 범위(ToonMaterialRange)를 담는다. (재질 이름, extras, 텍스처 그림 차례)
//     부품 안에서 칸을 갈라 칠할 때(강조색 칸, 표시판) 재질 이름이나 extras 로 고르고 ToonDrawOptions 의 index_start, index_count 로 그 칸만 그린다.
//   - 그림은 read_glb_images 로 읽고 decode_glb_image 로 풀어 ToonRenderer::create_texture 에 넘긴다.
//   - 재질 extras 의 outline 은 바깥선 껍데기(뒷면만 그림), basic 은 빛을 받지 않는 부품이다.
//     바깥선 설정을 주면 그 껍데기는 담지 않고, ".outline" 짝이 있는 노드의 몸으로 점마다 같은 굵기의 껍데기를 새로 짓는다.
//   - 뼈대(skin)가 있으면 본 노드와 그 거꾸로 묶는 행렬, 움직임(animations)과 그 extras 를 함께 읽는다. 자세는 write_skin_pose 가 셈한다.
//   - 부품을 따로 숨기거나 칠하는 모형은 read_glb_node_arrays 로 노드마다 따로 읽는다. (노드 extras 와 비치는 정도를 함께)
//   - 한 덩이로 읽은 뼈대 모형의 노드 extras 와 부품의 자리, 따르는 본은 read_glb_node_infos 로 따로 읽는다.
// - 엔진의 SkinnedModel(텍스처 PBR)과는 따로다. 툰 그리기에 쓸 꼭짓점 배열만 뽑는다.
//==============================================================================
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::base::matrix4::Matrix4;
use crate::base::quaternion::Quaternion;
use crate::graphics3d::toon::meshbuilder::{MeshBuilder, ToonMaterialRange, ToonMeshArrays};

// GLB 의 머리와 덩어리 종류.
const GLB_MAGIC: u32 = 0x46546c67;
const CHUNK_TYPE_JSON: u32 = 0x4e4f534a;
const CHUNK_TYPE_BINARY: u32 = 0x004e4942;
// 삼각형 프리미티브. (glTF 의 mode 4, 없으면 4)
const TRIANGLES_MODE: u64 = 4;
// 바깥선 껍데기 노드의 이름 끝. (read_glb_node_arrays 가 앞 이름의 노드에 함께 담는다)
const OUTLINE_NODE_SUFFIX: &str = ".outline";
// 한 번만 트는 움직임의 extras.loop. (끝 시각에 묶어 끝 자세를 둔다)
const LOOP_ONCE: &str = "once";

//==============================================================================
// 바깥선을 새로 지을 때의 설정.
// - width: 껍데기 굵기. (0 이면 껍데기를 하나도 짓지 않는다, GLB 의 껍데기도 담지 않는다)
// - part_widths: 부품(노드 이름)마다 따로 준 굵기. added_parts: ".outline" 짝이 없어도 껍데기를 지을 부품.
// - inner_parts: inner_width 가 0 보다 크면 그 굵기의 껍데기를 하나 더 지어 바깥선 껍데기 끝에 담고 그 자리를 inner_outline_ranges 로 돌려준다.
//   inner_parts 와 그 위에 얹힌 inner_over_parts 의 몸이 든 자리는 inner_body_ranges 로 돌려준다. (유리 같은 부위를 둘레의 가는 선과 함께 한 번 더 그릴 때)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonOutlineOptions {
	pub width: f32,
	pub part_widths: HashMap<String, f32>,
	pub added_parts: Vec<String>,
	pub inner_parts: Vec<String>,
	pub inner_over_parts: Vec<String>,
	pub inner_width: f32,
}

//==============================================================================
// 뼈대의 노드 하나. (쉬는 자세)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonSkeletonNode {
	// 노드 이름. (없으면 빈 글자)
	pub name: String,
	// 부모 노드의 차례. (뿌리면 None)
	pub parent_index: Option<usize>,
	pub translation: [f32; 3],
	// 돌림 사원수. ([x, y, z, w])
	pub rotation: [f32; 4],
	pub scale: [f32; 3],
	// glTF 의 matrix. (있으면 자리, 돌림, 크기 대신 쓰고 움직임을 받지 않는다)
	pub matrix: Option<[f32; 16]>,
}

//==============================================================================
// 움직임의 한 갈래. (노드 하나의 자리, 돌림, 크기 가운데 하나)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToonChannelPath {
	Translation,
	Rotation,
	Scale,
	// 그 밖의 것. (weights 같은, 읽기만 하고 자세에 쓰지 않는다)
	Other,
}

//==============================================================================
// 움직임의 한 갈래. (움직이는 노드, 바꾸는 것, 열쇠 시각과 값)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonAnimationChannel {
	// 움직이는 노드의 차례.
	pub node_index: usize,
	pub path: ToonChannelPath,
	// 열쇠 시각. (초)
	pub times: Vec<f32>,
	// 열쇠마다의 값. (돌림은 넷, 자리와 크기는 셋씩)
	pub values: Vec<f32>,
	// interpolation 이 STEP 인지. (사이를 섞지 않고 앞 키)
	pub is_step: bool,
}

//==============================================================================
// 움직임 하나. (길이, 갈래들, extras)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonAnimation {
	// 길이. (갈래들의 가장 늦은 열쇠 시각, 초)
	pub duration: f32,
	pub channels: Vec<ToonAnimationChannel>,
	// 움직임의 extras. (loop 가 "once" 면 한 번만 튼다, 없으면 빈 것)
	pub extras: Value,
}

//==============================================================================
// 뼈대와 움직임. (첫 skin 만)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonSkeleton {
	// 본 노드의 차례. (skin 의 joints, 꼭짓점의 본 번호가 이 목록의 차례다)
	pub joints: Vec<usize>,
	// 본마다 거꾸로 묶는 행렬. (16 씩, 없으면 단위 행렬)
	pub inverse_bind_matrices: Vec<f32>,
	// 모든 노드의 쉬는 자세. (노드 차례)
	pub nodes: Vec<ToonSkeletonNode>,
	// 이름마다 움직임.
	pub animations: HashMap<String, ToonAnimation>,
}

//==============================================================================
// 노드마다 따로 읽은 부품. (배열, 노드 extras, 비치는 재질의 가장 작은 불투명도)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonPartArrays {
	pub arrays: ToonMeshArrays,
	// 노드 extras. (없으면 빈 것)
	pub extras: Value,
	// 비치는 재질(alphaMode BLEND)의 가장 작은 불투명도. (없으면 1)
	pub alpha: f32,
}

//==============================================================================
// 메시 노드 하나의 정보. (extras, 꼭짓점 범위의 가운데, 가장 무겁게 딸린 본의 skin 안 차례)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonNodeInfo {
	// 노드 extras. (없으면 빈 것)
	pub extras: Value,
	// 꼭짓점 범위의 가운데. (모형 좌표, 꼭짓점이 없으면 원점)
	pub center: [f32; 3],
	// 가장 무겁게 딸린 본의 skin 안 차례. (뼈대로 움직이지 않으면 None)
	pub joint_index: Option<usize>,
}

//==============================================================================
// 자세를 섞을 앞 움직임. (움직임이 바뀔 때 잠깐)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonPoseBlend<'a> {
	// 앞 움직임의 이름.
	pub animation_name: &'a str,
	// 앞 움직임의 시각. (초)
	pub time: f32,
	// 앞 자세의 무게. (0 이면 섞지 않음, 1 이면 앞 자세 그대로)
	pub weight: f32,
}

//==============================================================================
// 노드마다의 흔들림 배율. (움직임 이름 → (노드 차례, 배율) 목록, 1 이면 그대로, 0 이면 쉬는 자세)
//==============================================================================
pub type ToonSwingScales = HashMap<String, Vec<(usize, f32)>>;

//==============================================================================
// GLB 의 JSON 과 바이너리.
//==============================================================================
struct GlbChunks {
	json: Value,
	binary: Vec<u8>,
}

//==============================================================================
// 리틀 엔디언 u32 읽기. (모자라면 None)
//==============================================================================
fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
	let slice = bytes.get(offset..offset + 4)?;
	Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

//==============================================================================
// GLB 의 덩어리 읽기.
//==============================================================================
fn read_glb_chunks(bytes: &[u8]) -> Result<GlbChunks, String> {
	if read_u32(bytes, 0) != Some(GLB_MAGIC) {
		return Err(String::from("GLB 가 아니다."));
	}
	let mut json: Option<Value> = None;
	let mut binary: Option<Vec<u8>> = None;
	let mut chunk_offset = 12usize;
	while chunk_offset + 8 <= bytes.len() {
		let chunk_length = read_u32(bytes, chunk_offset).unwrap_or(0) as usize;
		let chunk_type = read_u32(bytes, chunk_offset + 4).unwrap_or(0);
		let chunk_start = chunk_offset + 8;
		let chunk_bytes = bytes.get(chunk_start..chunk_start + chunk_length).ok_or_else(|| String::from("GLB 의 덩어리가 잘렸다."))?;
		if chunk_type == CHUNK_TYPE_JSON {
			json = Some(serde_json::from_slice(chunk_bytes).map_err(|error| format!("GLB 의 JSON 을 읽지 못했다: {}", error))?);
		}
		else if chunk_type == CHUNK_TYPE_BINARY {
			binary = Some(chunk_bytes.to_vec());
		}
		chunk_offset = chunk_start + chunk_length;
	}
	match (json, binary) {
		(Some(json), Some(binary)) => {
			Ok(GlbChunks { json, binary })
		}
		_ => {
			Err(String::from("GLB 의 덩어리가 모자란다."))
		}
	}
}

//==============================================================================
// 접근자 하나를 수 목록으로 읽기. (촘촘한 것과 byteStride 로 띄운 것, normalized 정수는 0 ~ 1 로)
//==============================================================================
fn read_accessor(chunks: &GlbChunks, accessor_index: usize) -> Vec<f32> {
	let accessor = &chunks.json["accessors"][accessor_index];
	let component_count = match accessor["type"].as_str().unwrap_or("SCALAR") {
		"VEC2" => {
			2
		}
		"VEC3" => {
			3
		}
		"VEC4" => {
			4
		}
		"MAT4" => {
			16
		}
		_ => {
			1
		}
	};
	let component_type = accessor["componentType"].as_u64().unwrap_or(5126);
	let component_size = match component_type {
		5120 | 5121 => {
			1
		}
		5122 | 5123 => {
			2
		}
		_ => {
			4
		}
	};
	let is_normalized = accessor["normalized"].as_bool().unwrap_or(false);
	let count = accessor["count"].as_u64().unwrap_or(0) as usize;
	let buffer_view = &chunks.json["bufferViews"][accessor["bufferView"].as_u64().unwrap_or(0) as usize];
	let element_offset = buffer_view["byteOffset"].as_u64().unwrap_or(0) as usize + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
	let tight_byte_size = component_count * component_size;
	let byte_stride = buffer_view["byteStride"].as_u64().unwrap_or(0) as usize;
	let stride = if byte_stride == 0 { tight_byte_size } else { byte_stride };
	let mut values = Vec::with_capacity(count * component_count);
	for element_index in 0..count {
		for component_index in 0..component_count {
			let offset = element_offset + element_index * stride + component_index * component_size;
			let bytes = match chunks.binary.get(offset..offset + component_size) {
				Some(bytes) => {
					bytes
				}
				None => {
					values.push(0.0);
					continue;
				}
			};
			let value = match component_type {
				5120 => {
					let raw = bytes[0] as i8 as f32;
					if is_normalized { (raw / 127.0).max(-1.0) } else { raw }
				}
				5121 => {
					let raw = bytes[0] as f32;
					if is_normalized { raw / 255.0 } else { raw }
				}
				5122 => {
					let raw = i16::from_le_bytes([bytes[0], bytes[1]]) as f32;
					if is_normalized { (raw / 32767.0).max(-1.0) } else { raw }
				}
				5123 => {
					let raw = u16::from_le_bytes([bytes[0], bytes[1]]) as f32;
					if is_normalized { raw / 65535.0 } else { raw }
				}
				5125 => {
					u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32
				}
				_ => {
					f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
				}
			};
			values.push(value);
		}
	}
	values
}

//==============================================================================
// 무게 접근자 읽기. (정수로 담겼으면 normalized 가 없어도 0 ~ 1 로)
//==============================================================================
fn read_weight_accessor(chunks: &GlbChunks, accessor_index: usize) -> Vec<f32> {
	let values = read_accessor(chunks, accessor_index);
	let accessor = &chunks.json["accessors"][accessor_index];
	if accessor["normalized"].as_bool().unwrap_or(false) {
		return values;
	}
	match accessor["componentType"].as_u64().unwrap_or(5126) {
		5121 => {
			values.iter().map(|value| value / 255.0).collect()
		}
		5123 => {
			values.iter().map(|value| value / 65535.0).collect()
		}
		_ => {
			values
		}
	}
}

//==============================================================================
// 꼭짓점 번호 읽기. (번호 접근자가 없으면 0 부터 차례로)
//==============================================================================
fn read_indices(chunks: &GlbChunks, primitive: &Value, point_count: usize) -> Vec<u32> {
	match primitive["indices"].as_u64() {
		Some(indices_accessor) => {
			read_accessor(chunks, indices_accessor as usize).iter().map(|value| *value as u32).collect()
		}
		None => {
			(0..point_count as u32).collect()
		}
	}
}

//==============================================================================
// JSON 수 배열을 COUNT 개 읽기. (배열이 아니거나 모자라거나 수가 아닌 칸은 fallback)
//==============================================================================
fn read_float_array<const COUNT: usize>(value: &Value, fallback: [f32; COUNT]) -> [f32; COUNT] {
	let mut result = fallback;
	if let Some(array) = value.as_array() {
		for (component_index, component) in array.iter().take(COUNT).enumerate() {
			result[component_index] = component.as_f64().unwrap_or(fallback[component_index] as f64) as f32;
		}
	}
	result
}

//==============================================================================
// 자리, 돌림(사원수), 크기로 행렬 짓기.
//==============================================================================
fn create_transform_matrix(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> Matrix4 {
	let mut matrix = Matrix4::create_translation(translation[0], translation[1], translation[2]);
	let rotation_matrix = Matrix4::create_rotation_from_quaternion(Quaternion::create(rotation[0], rotation[1], rotation[2], rotation[3]));
	matrix.multiply(&rotation_matrix);
	matrix.scale(scale[0], scale[1], scale[2]);
	matrix
}

//==============================================================================
// 노드의 자기 행렬. (matrix 가 있으면 그것, 아니면 자리, 돌림, 크기)
//==============================================================================
fn read_node_matrix(node: &Value) -> Matrix4 {
	if node["matrix"].is_array() {
		let mut matrix = Matrix4::default();
		matrix.elements = read_float_array(&node["matrix"], matrix.elements);
		return matrix;
	}
	create_transform_matrix(read_float_array(&node["translation"], [0.0; 3]), read_float_array(&node["rotation"], [0.0, 0.0, 0.0, 1.0]), read_float_array(&node["scale"], [1.0; 3]))
}

//==============================================================================
// 노드 이름. (없으면 빈 글자)
//==============================================================================
fn read_node_name(node: &Value) -> String {
	String::from(node["name"].as_str().unwrap_or(""))
}

//==============================================================================
// 프리미티브의 재질. (없으면 Null)
//==============================================================================
fn read_material<'a>(chunks: &'a GlbChunks, primitive: &Value) -> &'a Value {
	match primitive["material"].as_u64() {
		Some(material_index) => {
			&chunks.json["materials"][material_index as usize]
		}
		None => {
			&Value::Null
		}
	}
}

//==============================================================================
// 프리미티브 하나 담기. (바깥선 껍데기는 바깥선 배열에, 나머지는 색과 빛남과 함께)
// - 뼈대로 움직이는 프리미티브는 노드 행렬을 쓰지 않는다. (glTF 의 약속, 자리는 본이 정한다)
//==============================================================================
fn add_primitive(builder: &mut MeshBuilder, chunks: &GlbChunks, node: &Value, primitive: &Value, matrix: &Matrix4, is_outline_rebuilt: bool) {
	let mode = primitive["mode"].as_u64().unwrap_or(TRIANGLES_MODE);
	let attributes = &primitive["attributes"];
	let position_accessor = match attributes["POSITION"].as_u64() {
		Some(position_accessor) if mode == TRIANGLES_MODE => {
			position_accessor as usize
		}
		_ => {
			return;
		}
	};
	let positions = read_accessor(chunks, position_accessor);
	let indices = read_indices(chunks, primitive, positions.len() / 3);
	let is_skinned = !node["skin"].is_null() && attributes["JOINTS_0"].is_u64() && attributes["WEIGHTS_0"].is_u64();
	let identity_matrix = Matrix4::default();
	let part_matrix = if is_skinned { &identity_matrix } else { matrix };
	let joints = if is_skinned { Some(read_accessor(chunks, attributes["JOINTS_0"].as_u64().unwrap_or(0) as usize)) } else { None };
	let weights = if is_skinned { Some(read_weight_accessor(chunks, attributes["WEIGHTS_0"].as_u64().unwrap_or(0) as usize)) } else { None };
	let material = read_material(chunks, primitive);
	if material["extras"]["outline"].as_bool() == Some(true) {
		if !is_outline_rebuilt {
			builder.add_outline_triangles(&positions, &indices, part_matrix, joints.as_deref(), weights.as_deref());
		}
		return;
	}
	let normal_accessor = match attributes["NORMAL"].as_u64() {
		Some(normal_accessor) => {
			normal_accessor as usize
		}
		None => {
			return;
		}
	};
	let normals = read_accessor(chunks, normal_accessor);
	let uvs = attributes["TEXCOORD_0"].as_u64().map(|uv_accessor| read_accessor(chunks, uv_accessor as usize));
	let uvs1 = attributes["TEXCOORD_1"].as_u64().map(|uv_accessor| read_accessor(chunks, uv_accessor as usize));
	let base_color = read_float_array(&material["pbrMetallicRoughness"]["baseColorFactor"], [1.0; 4]);
	let color = [base_color[0].max(0.0).powf(1.0 / 2.2), base_color[1].max(0.0).powf(1.0 / 2.2), base_color[2].max(0.0).powf(1.0 / 2.2)];
	// 빛남은 바탕색에 곱하는 세기로 담는다. (셰이더는 바탕색 x 세기를 더한다)
	let mut glow = 0.0;
	let emissive_image = read_texture_image(chunks, &material["emissiveTexture"]);
	if material["emissiveFactor"].is_array() && emissive_image.is_none() {
		let emissive = read_float_array(&material["emissiveFactor"], [0.0; 3]);
		let emissive_maximum = emissive[0].max(emissive[1]).max(emissive[2]);
		let base_maximum = base_color[0].max(base_color[1]).max(base_color[2]).max(0.001);
		glow = emissive_maximum / base_maximum;
	}
	let is_unlit = material["extras"]["basic"].as_bool() == Some(true);
	let index_start = builder.get_index_count();
	builder.add_triangles_with_uvs(&positions, &normals, &indices, part_matrix, color, glow, is_unlit, joints.as_deref(), weights.as_deref(), uvs.as_deref(), uvs1.as_deref());
	builder.add_material_range(ToonMaterialRange {
		name: String::from(material["name"].as_str().unwrap_or("")),
		material_index: primitive["material"].as_u64().map(|material_index| material_index as usize),
		node_name: read_node_name(node),
		index_start,
		index_count: builder.get_index_count() - index_start,
		extras: if material["extras"].is_object() { material["extras"].clone() } else { Value::Object(serde_json::Map::new()) },
		alpha_mode: String::from(material["alphaMode"].as_str().unwrap_or("OPAQUE")),
		alpha_cutoff: material["alphaCutoff"].as_f64().unwrap_or(0.5) as f32,
		base_color_factor: base_color,
		emissive_factor: read_float_array(&material["emissiveFactor"], [0.0; 3]),
		base_color_image: read_texture_image(chunks, &material["pbrMetallicRoughness"]["baseColorTexture"]),
		emissive_image,
		has_uv: uvs.is_some(),
	});
}

//==============================================================================
// 재질의 텍스처 정보가 가리키는 그림 차례. (textures[index].source, 없으면 None)
//==============================================================================
fn read_texture_image(chunks: &GlbChunks, texture_info: &Value) -> Option<usize> {
	let texture_index = texture_info["index"].as_u64()? as usize;
	chunks.json["textures"][texture_index]["source"].as_u64().map(|source| source as usize)
}

//==============================================================================
// GLB 에 든 그림 하나. (images 의 차례, 재질 범위의 base_color_image, emissive_image 가 가리킨다)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonGlbImage {
	// 그림 이름. (없으면 빈 글자)
	pub name: String,
	// 그림 종류. ("image/png" 같은, 없으면 빈 글자)
	pub mime_type: String,
	// 그림 파일 바이트. (바깥 파일 uri 면 빈 것)
	pub data: Vec<u8>,
}

//==============================================================================
// GLB 에 든 그림들. (바이너리 덩어리의 bufferView 로 담긴 그림만 읽는다, 바깥 파일 uri 는 data 가 빈 것)
//==============================================================================
pub fn read_glb_images(bytes: &[u8]) -> Result<Vec<ToonGlbImage>, String> {
	let chunks = read_glb_chunks(bytes)?;
	let image_list = chunks.json["images"].as_array().cloned().unwrap_or_default();
	Ok(image_list.iter().map(|image| {
		let name = String::from(image["name"].as_str().unwrap_or(""));
		let mime_type = String::from(image["mimeType"].as_str().unwrap_or(""));
		let data = match image["bufferView"].as_u64() {
			Some(buffer_view_index) => {
				let buffer_view = &chunks.json["bufferViews"][buffer_view_index as usize];
				let byte_offset = buffer_view["byteOffset"].as_u64().unwrap_or(0) as usize;
				let byte_length = buffer_view["byteLength"].as_u64().unwrap_or(0) as usize;
				chunks.binary.get(byte_offset..byte_offset + byte_length).map(|slice| slice.to_vec()).unwrap_or_default()
			}
			None => {
				Vec::new()
			}
		};
		ToonGlbImage { name, mime_type, data }
	}).collect())
}

//==============================================================================
// read_glb_images 의 그림 하나를 풀기. ((너비, 높이, RGBA 바이트), 알파를 미리 곱하지 않는다, image 크레이트의 png, jpeg, webp, gif, bmp, ico)
//==============================================================================
pub fn decode_glb_image(image: &ToonGlbImage) -> Result<(u32, u32, Vec<u8>), String> {
	let decoded = image::load_from_memory(&image.data).map_err(|error| format!("GLB 그림 {} 을 풀지 못했다: {}", image.name, error))?;
	let rgba = decoded.to_rgba8();
	Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

//==============================================================================
// 몸으로 새로 지을 바깥선 껍데기의 굵기. (".outline" 짝도 added_parts 도 없거나 굵기가 0 이하면 None)
//==============================================================================
fn read_rebuilt_outline_width(node: &Value, node_names: &HashSet<String>, outline_options: &ToonOutlineOptions) -> Option<f32> {
	let node_name = read_node_name(node);
	let outline_name = format!("{}{}", node_name, OUTLINE_NODE_SUFFIX);
	if !node_names.contains(&outline_name) && !outline_options.added_parts.contains(&node_name) {
		return None;
	}
	let width = outline_options.part_widths.get(&node_name).copied().unwrap_or(outline_options.width);
	if width > 0.0 { Some(width) } else { None }
}

//==============================================================================
// 노드의 몸으로 바깥선 껍데기 짓기. (색마다 나뉜 프리미티브를 이어 붙여 한 번에 넘긴다, 바깥선 재질과 노멀이 없는 것은 뺀다)
//==============================================================================
fn add_rebuilt_outline(builder: &mut MeshBuilder, chunks: &GlbChunks, node: &Value, matrix: &Matrix4, outline_width: f32) {
	let mut positions = Vec::new();
	let mut normals = Vec::new();
	let mut indices = Vec::new();
	let is_skinned = !node["skin"].is_null();
	let mut joints = Vec::new();
	let mut weights = Vec::new();
	let mesh_index = node["mesh"].as_u64().unwrap_or(0) as usize;
	let primitives = chunks.json["meshes"][mesh_index]["primitives"].as_array().cloned().unwrap_or_default();
	for primitive in primitives.iter() {
		let mode = primitive["mode"].as_u64().unwrap_or(TRIANGLES_MODE);
		let attributes = &primitive["attributes"];
		if mode != TRIANGLES_MODE || !attributes["POSITION"].is_u64() || !attributes["NORMAL"].is_u64() {
			continue;
		}
		let material = read_material(chunks, primitive);
		let has_skin = attributes["JOINTS_0"].is_u64() && attributes["WEIGHTS_0"].is_u64();
		if material["extras"]["outline"].as_bool() == Some(true) || has_skin != is_skinned {
			continue;
		}
		let start_index = (positions.len() / 3) as u32;
		let primitive_positions = read_accessor(chunks, attributes["POSITION"].as_u64().unwrap_or(0) as usize);
		let primitive_normals = read_accessor(chunks, attributes["NORMAL"].as_u64().unwrap_or(0) as usize);
		let primitive_indices = read_indices(chunks, primitive, primitive_positions.len() / 3);
		positions.extend_from_slice(&primitive_positions);
		normals.extend_from_slice(&primitive_normals);
		indices.extend(primitive_indices.iter().map(|index| start_index + index));
		if is_skinned {
			joints.extend(read_accessor(chunks, attributes["JOINTS_0"].as_u64().unwrap_or(0) as usize));
			weights.extend(read_weight_accessor(chunks, attributes["WEIGHTS_0"].as_u64().unwrap_or(0) as usize));
		}
	}
	if indices.is_empty() {
		return;
	}
	let identity_matrix = Matrix4::default();
	let part_matrix = if is_skinned { &identity_matrix } else { matrix };
	if is_skinned {
		builder.add_triangle_shell(&positions, &normals, &indices, part_matrix, outline_width, Some(&joints), Some(&weights));
	}
	else {
		builder.add_triangle_shell(&positions, &normals, &indices, part_matrix, outline_width, None, None);
	}
}

//==============================================================================
// 노드 이름 모음. (이름이 없는 노드는 빈 글자)
//==============================================================================
fn read_node_names(chunks: &GlbChunks) -> HashSet<String> {
	chunks.json["nodes"].as_array().map(|nodes| nodes.iter().map(read_node_name).collect()).unwrap_or_default()
}

//==============================================================================
// 뼈대와 움직임 읽기. (없으면 None, 첫 skin 만)
//==============================================================================
fn read_skeleton(chunks: &GlbChunks) -> Option<ToonSkeleton> {
	let skin = chunks.json["skins"].as_array()?.first()?;
	let source_nodes = chunks.json["nodes"].as_array()?;
	let mut parent_indices: Vec<Option<usize>> = vec![None; source_nodes.len()];
	for (node_index, node) in source_nodes.iter().enumerate() {
		if let Some(children) = node["children"].as_array() {
			for child in children {
				if let Some(child_index) = child.as_u64() {
					if (child_index as usize) < parent_indices.len() {
						parent_indices[child_index as usize] = Some(node_index);
					}
				}
			}
		}
	}
	let nodes: Vec<ToonSkeletonNode> = source_nodes.iter().enumerate().map(|(node_index, node)| {
		ToonSkeletonNode {
			name: read_node_name(node),
			parent_index: parent_indices[node_index],
			translation: read_float_array(&node["translation"], [0.0; 3]),
			rotation: read_float_array(&node["rotation"], [0.0, 0.0, 0.0, 1.0]),
			scale: read_float_array(&node["scale"], [1.0; 3]),
			matrix: if node["matrix"].is_array() { Some(read_float_array(&node["matrix"], Matrix4::default().elements)) } else { None },
		}
	}).collect();
	let joints: Vec<usize> = skin["joints"].as_array()?.iter().filter_map(|joint| joint.as_u64().map(|joint| joint as usize)).collect();
	let inverse_bind_matrices = match skin["inverseBindMatrices"].as_u64() {
		Some(accessor_index) => {
			read_accessor(chunks, accessor_index as usize)
		}
		None => {
			joints.iter().flat_map(|_| Matrix4::default().elements).collect()
		}
	};
	let mut animations = HashMap::new();
	for animation in chunks.json["animations"].as_array().cloned().unwrap_or_default() {
		let mut channels = Vec::new();
		let mut duration = 0.0f32;
		for channel in animation["channels"].as_array().cloned().unwrap_or_default() {
			let sampler = &animation["samplers"][channel["sampler"].as_u64().unwrap_or(0) as usize];
			let times = read_accessor(chunks, sampler["input"].as_u64().unwrap_or(0) as usize);
			let values = read_accessor(chunks, sampler["output"].as_u64().unwrap_or(0) as usize);
			if let Some(last_time) = times.last() {
				duration = duration.max(*last_time);
			}
			let path = match channel["target"]["path"].as_str().unwrap_or("") {
				"translation" => {
					ToonChannelPath::Translation
				}
				"rotation" => {
					ToonChannelPath::Rotation
				}
				"scale" => {
					ToonChannelPath::Scale
				}
				_ => {
					ToonChannelPath::Other
				}
			};
			channels.push(ToonAnimationChannel {
				node_index: channel["target"]["node"].as_u64().unwrap_or(0) as usize,
				path,
				times,
				values,
				is_step: sampler["interpolation"].as_str() == Some("STEP"),
			});
		}
		let extras = if animation["extras"].is_object() { animation["extras"].clone() } else { Value::Object(serde_json::Map::new()) };
		animations.insert(String::from(animation["name"].as_str().unwrap_or("")), ToonAnimation { duration, channels, extras });
	}
	Some(ToonSkeleton { joints, inverse_bind_matrices, nodes, animations })
}

//==============================================================================
// 장면의 메시 노드를 차례로 돌기. (노드마다 부모까지 곱한 행렬과 함께 visit_event 를 부른다)
//==============================================================================
fn visit_mesh_nodes(chunks: &GlbChunks, visit_event: &mut dyn FnMut(&Value, &Matrix4)) {
	let node_count = chunks.json["nodes"].as_array().map(|nodes| nodes.len()).unwrap_or(0);
	let scene_index = chunks.json["scene"].as_u64().unwrap_or(0) as usize;
	let root_nodes: Vec<usize> = match chunks.json["scenes"][scene_index]["nodes"].as_array() {
		Some(nodes) => {
			nodes.iter().filter_map(|node| node.as_u64().map(|node| node as usize)).collect()
		}
		None => {
			(0..node_count).collect()
		}
	};
	//==============================================================================
	// 노드 하나 돌기. (부모 행렬에 자기 행렬을 곱하고, 메시가 있으면 visit_event, 그다음 자식으로)
	//==============================================================================
	fn visit(chunks: &GlbChunks, node_index: usize, parent_matrix: &Matrix4, visit_event: &mut dyn FnMut(&Value, &Matrix4)) {
		let node = &chunks.json["nodes"][node_index];
		let mut matrix = *parent_matrix;
		let node_matrix = read_node_matrix(node);
		matrix.multiply(&node_matrix);
		if node["mesh"].is_u64() {
			visit_event(node, &matrix);
		}
		if let Some(children) = node["children"].as_array() {
			for child in children {
				if let Some(child_index) = child.as_u64() {
					visit(chunks, child_index as usize, &matrix, visit_event);
				}
			}
		}
	}
	for node_index in root_nodes {
		visit(chunks, node_index, &Matrix4::default(), visit_event);
	}
}

//==============================================================================
// 노드 메시의 프리미티브 목록.
//==============================================================================
fn read_primitives(chunks: &GlbChunks, node: &Value) -> Vec<Value> {
	let mesh_index = node["mesh"].as_u64().unwrap_or(0) as usize;
	chunks.json["meshes"][mesh_index]["primitives"].as_array().cloned().unwrap_or_default()
}

//==============================================================================
// GLB 하나를 꼭짓점 배열로. (뼈대가 있으면 skeleton 도, 바깥선 설정을 주면 껍데기를 새로 짓는다)
//==============================================================================
pub fn read_glb_arrays(bytes: &[u8], outline_options: Option<&ToonOutlineOptions>) -> Result<ToonMeshArrays, String> {
	let chunks = read_glb_chunks(bytes)?;
	let mut builder = MeshBuilder::new();
	let is_outline_rebuilt = outline_options.is_some();
	let node_names = read_node_names(&chunks);
	let empty_options = ToonOutlineOptions::default();
	let options = outline_options.unwrap_or(&empty_options);
	let mut inner_nodes: Vec<(Value, Matrix4)> = Vec::new();
	let mut inner_body_ranges: Vec<(usize, usize)> = Vec::new();
	visit_mesh_nodes(&chunks, &mut |node: &Value, matrix: &Matrix4| {
		let body_start = builder.get_index_count();
		for primitive in read_primitives(&chunks, node).iter() {
			add_primitive(&mut builder, &chunks, node, primitive, matrix, is_outline_rebuilt);
		}
		if !is_outline_rebuilt {
			return;
		}
		if let Some(outline_width) = read_rebuilt_outline_width(node, &node_names, options) {
			add_rebuilt_outline(&mut builder, &chunks, node, matrix, outline_width);
		}
		let node_name = read_node_name(node);
		if options.inner_parts.contains(&node_name) {
			inner_nodes.push((node.clone(), *matrix));
		}
		let body_count = builder.get_index_count() - body_start;
		let is_inner_body = options.inner_parts.contains(&node_name) || options.inner_over_parts.contains(&node_name);
		if is_inner_body && body_count > 0 {
			inner_body_ranges.push((body_start, body_count));
		}
	});
	let mut inner_outline_ranges = Vec::new();
	if options.inner_width > 0.0 {
		for (inner_node, inner_matrix) in inner_nodes.iter() {
			let inner_start = builder.get_outline_index_count();
			add_rebuilt_outline(&mut builder, &chunks, inner_node, inner_matrix, options.inner_width);
			let inner_count = builder.get_outline_index_count() - inner_start;
			if inner_count > 0 {
				inner_outline_ranges.push((inner_start, inner_count));
			}
		}
	}
	let mut arrays = builder.read_arrays();
	arrays.skeleton = read_skeleton(&chunks);
	arrays.inner_outline_ranges = inner_outline_ranges;
	arrays.inner_body_ranges = if inner_nodes.is_empty() { Vec::new() } else { inner_body_ranges };
	Ok(arrays)
}

//==============================================================================
// GLB 하나를 노드 이름마다의 꼭짓점 배열로. (뼈대는 읽지 않는다)
// - 이름 끝의 ".outline" 노드(바깥선 껍데기)는 그 앞 이름의 노드에 함께 담는다.
// - 부품마다 노드 extras 와 비치는 재질(alphaMode BLEND)의 가장 작은 불투명도를 함께 담는다.
//==============================================================================
pub fn read_glb_node_arrays(bytes: &[u8], outline_options: Option<&ToonOutlineOptions>) -> Result<HashMap<String, ToonPartArrays>, String> {
	let chunks = read_glb_chunks(bytes)?;
	let mut builders: HashMap<String, MeshBuilder> = HashMap::new();
	let mut part_extras: HashMap<String, Value> = HashMap::new();
	let mut part_alphas: HashMap<String, f32> = HashMap::new();
	let is_outline_rebuilt = outline_options.is_some();
	let node_names = read_node_names(&chunks);
	visit_mesh_nodes(&chunks, &mut |node: &Value, matrix: &Matrix4| {
		let node_name = read_node_name(node);
		let part_name = String::from(node_name.strip_suffix(OUTLINE_NODE_SUFFIX).unwrap_or(&node_name));
		let builder = builders.entry(part_name.clone()).or_default();
		if part_name == node_name && node["extras"].is_object() {
			part_extras.insert(part_name.clone(), node["extras"].clone());
		}
		for primitive in read_primitives(&chunks, node).iter() {
			add_primitive(builder, &chunks, node, primitive, matrix, is_outline_rebuilt);
			let material = read_material(&chunks, primitive);
			if material["alphaMode"].as_str() == Some("BLEND") {
				let base_color = read_float_array(&material["pbrMetallicRoughness"]["baseColorFactor"], [1.0; 4]);
				let previous_alpha = part_alphas.get(&part_name).copied().unwrap_or(1.0);
				part_alphas.insert(part_name.clone(), previous_alpha.min(base_color[3]));
			}
		}
		if let Some(outline_options) = outline_options {
			if let Some(outline_width) = read_rebuilt_outline_width(node, &node_names, outline_options) {
				add_rebuilt_outline(builder, &chunks, node, matrix, outline_width);
			}
		}
	});
	let mut part_arrays = HashMap::new();
	for (part_name, builder) in builders.into_iter() {
		let extras = part_extras.remove(&part_name).unwrap_or_else(|| Value::Object(serde_json::Map::new()));
		let alpha = part_alphas.get(&part_name).copied().unwrap_or(1.0);
		part_arrays.insert(part_name, ToonPartArrays { arrays: builder.read_arrays(), extras, alpha });
	}
	Ok(part_arrays)
}

//==============================================================================
// GLB 의 메시 노드마다의 extras, 가운데, 따르는 본. (이름 끝이 ".outline" 인 노드와 바깥선 재질은 뺀다)
// - center 는 꼭짓점이 놓인 범위의 가운데다. (모형 좌표, 뼈대로 움직이는 노드는 쉬는 자세의 자리, 꼭짓점이 없으면 원점)
// - joint_index 는 그 노드의 꼭짓점이 가장 무겁게 딸린 본의 skin 안 차례다. (write_skin_pose 가 쓴 본 행렬의 차례)
//==============================================================================
pub fn read_glb_node_infos(bytes: &[u8]) -> Result<HashMap<String, ToonNodeInfo>, String> {
	let chunks = read_glb_chunks(bytes)?;
	let mut node_infos = HashMap::new();
	visit_mesh_nodes(&chunks, &mut |node: &Value, matrix: &Matrix4| {
		let node_name = read_node_name(node);
		if node_name.ends_with(OUTLINE_NODE_SUFFIX) {
			return;
		}
		let mut minimum = [f32::MAX; 3];
		let mut maximum = [f32::MIN; 3];
		let mut joint_weights: HashMap<usize, f32> = HashMap::new();
		for primitive in read_primitives(&chunks, node).iter() {
			let attributes = &primitive["attributes"];
			let position_accessor = match attributes["POSITION"].as_u64() {
				Some(position_accessor) => {
					position_accessor as usize
				}
				None => {
					continue;
				}
			};
			if read_material(&chunks, primitive)["extras"]["outline"].as_bool() == Some(true) {
				continue;
			}
			let is_skinned = !node["skin"].is_null() && attributes["JOINTS_0"].is_u64() && attributes["WEIGHTS_0"].is_u64();
			let positions = read_accessor(&chunks, position_accessor);
			for point in positions.chunks(3) {
				let mut point_value = [point[0], point[1], point[2]];
				if !is_skinned {
					let transformed_point = matrix.transform_point(point[0], point[1], point[2]);
					point_value = [transformed_point.x, transformed_point.y, transformed_point.z];
				}
				for axis in 0..3 {
					minimum[axis] = minimum[axis].min(point_value[axis]);
					maximum[axis] = maximum[axis].max(point_value[axis]);
				}
			}
			if is_skinned {
				let joints = read_accessor(&chunks, attributes["JOINTS_0"].as_u64().unwrap_or(0) as usize);
				let weights = read_weight_accessor(&chunks, attributes["WEIGHTS_0"].as_u64().unwrap_or(0) as usize);
				for (joint, weight) in joints.iter().zip(weights.iter()) {
					*joint_weights.entry(*joint as usize).or_insert(0.0) += *weight;
				}
			}
		}
		let center = if minimum[0] <= maximum[0] { [(minimum[0] + maximum[0]) / 2.0, (minimum[1] + maximum[1]) / 2.0, (minimum[2] + maximum[2]) / 2.0] } else { [0.0; 3] };
		let joint_index = joint_weights.iter().filter(|(_, weight)| **weight > 0.0).max_by(|first, second| first.1.partial_cmp(second.1).unwrap_or(std::cmp::Ordering::Equal)).map(|(joint_index, _)| *joint_index);
		let extras = if node["extras"].is_object() { node["extras"].clone() } else { Value::Object(serde_json::Map::new()) };
		node_infos.insert(node_name, ToonNodeInfo { extras, center, joint_index });
	});
	Ok(node_infos)
}

//==============================================================================
// 움직임의 한 갈래를 그 시각에서 읽기. (LINEAR 는 사이를 섞고 돌림은 사원수를 가까운 쪽으로 섞어 정규화, STEP 은 앞 키)
//==============================================================================
fn sample_channel(channel: &ToonAnimationChannel, time: f32) -> Vec<f32> {
	let times = &channel.times;
	let component_count = if channel.path == ToonChannelPath::Rotation { 4 } else { 3 };
	let values = &channel.values;
	if times.is_empty() || values.len() < component_count {
		return Vec::new();
	}
	if time <= times[0] || times.len() == 1 {
		return values[0..component_count].to_vec();
	}
	let last_index = times.len() - 1;
	if time >= times[last_index] {
		return values[last_index * component_count..last_index * component_count + component_count].to_vec();
	}
	let mut key_index = 0;
	while key_index < last_index - 1 && times[key_index + 1] <= time {
		key_index += 1;
	}
	let start_offset = key_index * component_count;
	let end_offset = start_offset + component_count;
	if channel.is_step {
		return values[start_offset..end_offset].to_vec();
	}
	let ratio = (time - times[key_index]) / (times[key_index + 1] - times[key_index]);
	let mut sign = 1.0;
	if component_count == 4 {
		let dot: f32 = (0..4).map(|component_index| values[start_offset + component_index] * values[end_offset + component_index]).sum();
		sign = if dot < 0.0 { -1.0 } else { 1.0 };
	}
	let mut result: Vec<f32> = (0..component_count).map(|component_index| {
		let start_value = values[start_offset + component_index];
		let end_value = values[end_offset + component_index] * sign;
		start_value + (end_value - start_value) * ratio
	}).collect();
	if component_count == 4 {
		let length = result.iter().map(|value| value * value).sum::<f32>().sqrt();
		if length > 0.0 {
			for value in result.iter_mut() {
				*value /= length;
			}
		}
	}
	result
}

//==============================================================================
// 두 돌림 섞기. (가까운 쪽으로 섞어 정규화, ratio 가 1 이면 other)
//==============================================================================
fn mix_rotation(rotation: [f32; 4], other_rotation: [f32; 4], ratio: f32) -> [f32; 4] {
	let dot: f32 = (0..4).map(|component_index| rotation[component_index] * other_rotation[component_index]).sum();
	let sign = if dot < 0.0 { -1.0 } else { 1.0 };
	let mixed: [f32; 4] = [0, 1, 2, 3].map(|component_index| rotation[component_index] + (other_rotation[component_index] * sign - rotation[component_index]) * ratio);
	let length = mixed.iter().map(|value| value * value).sum::<f32>().sqrt();
	if length <= 0.0 {
		return rotation;
	}
	mixed.map(|value| value / length)
}

//==============================================================================
// 움직임을 그 시각에서 읽은 노드마다의 자리, 돌림, 크기. (움직임이 없으면 쉬는 자세)
//==============================================================================
fn read_animation_locals(skeleton: &ToonSkeleton, animation_name: &str, time: f32, swing_scales: Option<&ToonSwingScales>) -> Vec<ToonSkeletonNode> {
	let mut locals = skeleton.nodes.clone();
	let animation = match skeleton.animations.get(animation_name) {
		Some(animation) => {
			animation
		}
		None => {
			return locals;
		}
	};
	let is_once = animation.extras["loop"].as_str() == Some(LOOP_ONCE);
	let mut local_time = 0.0;
	if animation.duration > 0.0 {
		local_time = if is_once { time.clamp(0.0, animation.duration) } else { time.rem_euclid(animation.duration) };
	}
	for channel in animation.channels.iter() {
		let local = match locals.get_mut(channel.node_index) {
			Some(local) if local.matrix.is_none() => {
				local
			}
			_ => {
				continue;
			}
		};
		let sampled = sample_channel(channel, local_time);
		match channel.path {
			ToonChannelPath::Translation if sampled.len() == 3 => {
				local.translation = [sampled[0], sampled[1], sampled[2]];
			}
			ToonChannelPath::Rotation if sampled.len() == 4 => {
				local.rotation = [sampled[0], sampled[1], sampled[2], sampled[3]];
			}
			ToonChannelPath::Scale if sampled.len() == 3 => {
				local.scale = [sampled[0], sampled[1], sampled[2]];
			}
			_ => {
			}
		}
	}
	// 흔들림 배율을 받은 노드는 돌림을 쉬는 자세 쪽으로 그 몫만큼만 둔다.
	if let Some(scales) = swing_scales.and_then(|swing_scales| swing_scales.get(animation_name)) {
		for (node_index, scale) in scales.iter() {
			let rest_rotation = match skeleton.nodes.get(*node_index) {
				Some(node) => {
					node.rotation
				}
				None => {
					continue;
				}
			};
			if let Some(local) = locals.get_mut(*node_index) {
				if local.matrix.is_none() {
					local.rotation = mix_rotation(rest_rotation, local.rotation, *scale);
				}
			}
		}
	}
	locals
}

//==============================================================================
// 움직임에서 한 노드의 돌림이 쉬는 자세에서 가장 벗어난 각. (라디안, 열쇠 프레임마다 보고, 그 노드의 돌림이 없으면 0)
//==============================================================================
pub fn read_animation_swing(skeleton: &ToonSkeleton, animation_name: &str, node_index: usize) -> f32 {
	let (animation, node) = match (skeleton.animations.get(animation_name), skeleton.nodes.get(node_index)) {
		(Some(animation), Some(node)) => {
			(animation, node)
		}
		_ => {
			return 0.0;
		}
	};
	let mut largest_angle = 0.0f32;
	for channel in animation.channels.iter() {
		if channel.node_index != node_index || channel.path != ToonChannelPath::Rotation {
			continue;
		}
		for rotation in channel.values.chunks(4) {
			if rotation.len() < 4 {
				continue;
			}
			let dot: f32 = (0..4).map(|component_index| rotation[component_index] * node.rotation[component_index]).sum();
			largest_angle = largest_angle.max(2.0 * dot.abs().min(1.0).acos());
		}
	}
	largest_angle
}

//==============================================================================
// 두 자세 섞기. (자리와 크기는 곧게, 돌림은 가까운 쪽으로 섞어 정규화, ratio 가 1 이면 other)
//==============================================================================
fn mix_locals(locals: &mut [ToonSkeletonNode], other_locals: &[ToonSkeletonNode], ratio: f32) {
	for (local, other) in locals.iter_mut().zip(other_locals.iter()) {
		if local.matrix.is_some() {
			continue;
		}
		for component_index in 0..3 {
			local.translation[component_index] += (other.translation[component_index] - local.translation[component_index]) * ratio;
			local.scale[component_index] += (other.scale[component_index] - local.scale[component_index]) * ratio;
		}
		local.rotation = mix_rotation(local.rotation, other.rotation, ratio);
	}
}

//==============================================================================
// 뼈대 자세 셈하기. (움직임을 그 시각에서 읽어 본마다 행렬을 target 에 16 씩 쓴다, 움직임이 없으면 쉬는 자세)
// - blend 를 주면 앞 움직임의 자세를 그 무게만큼 섞는다. (움직임이 바뀔 때 잠깐)
// - swing_scales 를 주면 그 움직임의 그 노드는 돌림을 쉬는 자세 쪽으로 줄인다. (섞는 앞 자세에도 같이)
//==============================================================================
pub fn write_skin_pose(skeleton: &ToonSkeleton, animation_name: &str, time: f32, target: &mut [f32], blend: Option<ToonPoseBlend>, swing_scales: Option<&ToonSwingScales>) {
	let mut locals = read_animation_locals(skeleton, animation_name, time, swing_scales);
	if let Some(blend) = blend {
		if blend.weight > 0.0 {
			let blend_locals = read_animation_locals(skeleton, blend.animation_name, blend.time, swing_scales);
			mix_locals(&mut locals, &blend_locals, blend.weight.min(1.0));
		}
	}
	let mut globals: Vec<Option<Matrix4>> = vec![None; locals.len()];
	//==============================================================================
	// 노드의 모형 좌표 행렬. (부모까지 곱한 것, 한 번 셈한 것은 globals 에 담아 다시 쓴다)
	//==============================================================================
	fn read_global(node_index: usize, locals: &[ToonSkeletonNode], globals: &mut Vec<Option<Matrix4>>) -> Matrix4 {
		if let Some(global_matrix) = globals[node_index] {
			return global_matrix;
		}
		let local = &locals[node_index];
		let local_matrix = match local.matrix {
			Some(elements) => {
				Matrix4 { elements }
			}
			None => {
				create_transform_matrix(local.translation, local.rotation, local.scale)
			}
		};
		let global_matrix = match local.parent_index {
			Some(parent_index) => {
				let mut parent_matrix = read_global(parent_index, locals, globals);
				parent_matrix.multiply(&local_matrix);
				parent_matrix
			}
			None => {
				local_matrix
			}
		};
		globals[node_index] = Some(global_matrix);
		global_matrix
	}
	for (joint_index, node_index) in skeleton.joints.iter().enumerate() {
		let target_offset = joint_index * 16;
		if target_offset + 16 > target.len() || *node_index >= locals.len() {
			break;
		}
		let mut joint_matrix = read_global(*node_index, &locals, &mut globals);
		let mut inverse_bind_matrix = Matrix4::default();
		if let Some(inverse_bind_elements) = skeleton.inverse_bind_matrices.get(target_offset..target_offset + 16) {
			inverse_bind_matrix.elements.copy_from_slice(inverse_bind_elements);
		}
		joint_matrix.multiply(&inverse_bind_matrix);
		target[target_offset..target_offset + 16].copy_from_slice(&joint_matrix.elements);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 시험용 작은 GLB. (네모 하나, TEXCOORD_0 은 정규화한 u16, 재질 이름과 extras, 바탕색 텍스처의 그림)
	//==============================================================================
	fn build_test_glb(image_bytes: &[u8]) -> Vec<u8> {
		let positions: [f32; 12] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
		let normals: [f32; 12] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
		let uvs: [u16; 8] = [0, 65535, 65535, 65535, 65535, 0, 0, 0];
		let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
		let mut part_list: Vec<Vec<u8>> = Vec::new();
		part_list.push(positions.iter().flat_map(|value| value.to_le_bytes()).collect());
		part_list.push(normals.iter().flat_map(|value| value.to_le_bytes()).collect());
		part_list.push(uvs.iter().flat_map(|value| value.to_le_bytes()).collect());
		part_list.push(indices.iter().flat_map(|value| value.to_le_bytes()).collect());
		part_list.push(image_bytes.to_vec());
		let mut binary: Vec<u8> = Vec::new();
		let mut view_list = Vec::new();
		for part in part_list.iter() {
			view_list.push(serde_json::json!({ "buffer": 0, "byteOffset": binary.len(), "byteLength": part.len() }));
			binary.extend_from_slice(part);
			while binary.len() % 4 != 0 {
				binary.push(0);
			}
		}
		let json = serde_json::json!({
			"asset": { "version": "2.0" }, "scene": 0, "scenes": [{ "nodes": [0] }],
			"nodes": [{ "name": "plate", "mesh": 0, "extras": { "mark": { "kind": "logo" } } }],
			"meshes": [{ "primitives": [{ "attributes": { "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 }, "indices": 3, "material": 0 }] }],
			"materials": [{ "name": "logoPlate", "extras": { "slot": "accent" }, "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 1.0], "baseColorTexture": { "index": 0 } } }],
			"textures": [{ "source": 0 }], "images": [{ "name": "logo", "mimeType": "image/png", "bufferView": 4 }],
			"buffers": [{ "byteLength": binary.len() }], "bufferViews": view_list,
			"accessors": [
				{ "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3" },
				{ "bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3" },
				{ "bufferView": 2, "componentType": 5123, "normalized": true, "count": 4, "type": "VEC2" },
				{ "bufferView": 3, "componentType": 5123, "count": 6, "type": "SCALAR" }
			]
		});
		let mut json_bytes = serde_json::to_vec(&json).unwrap();
		while json_bytes.len() % 4 != 0 {
			json_bytes.push(b' ');
		}
		let total_length = 12 + 8 + json_bytes.len() + 8 + binary.len();
		let mut glb = Vec::with_capacity(total_length);
		glb.extend_from_slice(&GLB_MAGIC.to_le_bytes());
		glb.extend_from_slice(&2u32.to_le_bytes());
		glb.extend_from_slice(&(total_length as u32).to_le_bytes());
		glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
		glb.extend_from_slice(&CHUNK_TYPE_JSON.to_le_bytes());
		glb.extend_from_slice(&json_bytes);
		glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
		glb.extend_from_slice(&CHUNK_TYPE_BINARY.to_le_bytes());
		glb.extend_from_slice(&binary);
		glb
	}

	#[test]
	fn glb_reads_uv_material_range_and_image() {
		let mut png_bytes: Vec<u8> = Vec::new();
		let source_image = image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]).unwrap();
		image::DynamicImage::ImageRgba8(source_image).write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();
		let glb = build_test_glb(&png_bytes);
		let arrays = read_glb_arrays(&glb, None).unwrap();
		assert_eq!(arrays.uv_data, vec![0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
		let range = &arrays.material_ranges[0];
		assert_eq!((range.name.as_str(), range.index_start, range.index_count), ("logoPlate", 0, 6));
		assert_eq!(range.extras["slot"].as_str(), Some("accent"));
		assert_eq!(range.base_color_image, Some(0));
		assert!(range.has_uv);
		let parts = read_glb_node_arrays(&glb, None).unwrap();
		assert_eq!(parts["plate"].extras["mark"]["kind"].as_str(), Some("logo"));
		assert_eq!(parts["plate"].arrays.uv_data.len(), 8);
		let images = read_glb_images(&glb).unwrap();
		assert_eq!((images[0].name.as_str(), images[0].mime_type.as_str()), ("logo", "image/png"));
		assert_eq!(images[0].data, png_bytes);
		let (width, height, pixels) = decode_glb_image(&images[0]).unwrap();
		assert_eq!((width, height), (2, 1));
		assert_eq!(pixels, vec![255, 0, 0, 255, 0, 255, 0, 128]);
	}
}
