//==============================================================================
//! 스킨드 모델. (원본 graphics3d/skinnedmodel.js)
//! - 모델 인스턴스, 메시/본/스킨/머티리얼/애니메이션 데이터 + 포즈 상태. 렌더링은 SkinnedModelRenderer 가 맡는다.
//! - GLB(바이너리 glTF)와 바이너리 FBX 를 공통 중립 스키마(MeshDescription + MaterialDescription)로 해석해 GL 리소스를 구성한다.
//!   (임포터는 파싱만 → 중립 스키마 → 공용 GL 업로드 upload_meshes_and_materials)
//! - 스킨 없는 정적 메시는 합성 스킨(조인트 1개)으로, 모프 타깃은 실수 텍스처로 올려 같은 스키닝 경로로 그린다.
//! - 애니메이션 크로스페이드와 조인트 회전 오프셋(절차 포즈 가공), 모프 가중치(glTF weights / 프로그램)를 원본처럼 지원한다.
//! - 원본과 다른 것:
//!   - 원본은 async 이고 URL 을 fetch 한다(loadFromUrl, 바이트는 loadFromBytes). 러스트 판의 바이트 함수
//!     load_from_bytes / load_from_fbx_bytes / add_animations_from_fbx_bytes 는 그 자리에서 끝난다.
//!     URL 함수 load_from_url / load_from_fbx_url 은 ImageAsset 처럼 엔진의 애셋 받기(데스크탑은 애셋 폴더 후보, 웹은 asset_base_url 에서 fetch)로
//!     바이트를 부탁하고 받기 손잡이(SkinnedModelLoadHandle)를 돌려준다. 다 받으면 손잡이의 create_model(gl, 렌더러)이 모델을 짓는다. (await 없는 꼴)
//!     원본 await SkinnedModel.loadFromUrl(gl, url, renderer) 자리는 엔진 작업 안에서 load_from_url_async(엔진, 경로, 렌더러).await 다. (fbx 는 load_from_fbx_url_async)
//!     다 받으면 작업을 폴링하는 엔진의 GL 로 모델을 짓는다. (렌더러는 부를 때 셰이더 프로그램만 나눠 들어 빌리지 않는다)
//!     add_animations_from_fbx_url 은 받기를 부탁하고 await 할 수 있는 AnimationLoadHandle 을 돌려준다. 원본처럼 await 가 끝날 때는 클립이 이미 더해져 있다.
//!     (클립 목록을 손잡이와 나눠 들어, 다 받으면 await 하는 퓨처나 다음 update, play_animation 가운데 먼저 닿는 쪽이 더한다. 원본처럼 받은 차례로 더한다)
//!   - GL 을 쓰는 함수는 gl 을 받는다. update(delta_time) 는 GL 을 쓰지 않는다. 원본 update 6단계(바뀐 모프 타깃 업로드)는
//!     렌더러의 draw / draw_depth / draw_wireframe 가 upload_pending_morph_targets 로 대신한다. (그래서 그리기 함수가 &mut 모델을 받는다)
//!     덕분에 씬의 tick 에서 update 를 부를 수 있다.
//!   - 원본 노드 / 드로어블 / 스킨 / 클립 객체는 SkinnedModelNode / Drawable / Skin / AnimationClip 구조체다.
//!     드로어블의 머티리얼은 `Rc<Material>` 이다. 클립은 인덱스로 가리키고, sample_animation 도 클립 인덱스를 받는다.
//!     upload_wireframe_indices / upload_morph_targets 는 원본의 드로어블 객체 대신 드로어블 인덱스를 받는다. (모델을 &mut 로 빌린 채 드로어블을 넘길 수 없어서다)
//!   - 드로어블의 mesh_description 은 모프 타깃을 morph_target_list 로 옮긴 뒤라 morph_targets 가 비어 있다. (큰 델타 배열을 두 벌 들지 않는다)
//!   - 원본처럼 노드 matrix 와 CUBICSPLINE 보간, 프리미티브 mode 는 보지 않는다. FBX 는 원본처럼 스킨이 붙은 메시만 그린다.
//! - 올린 버퍼, 머티리얼 원형, 모프 텍스처, 노드와 스킨 원형, 클립은 SkinnedModelAsset 이 한 벌 들고, 모델은 그 인스턴스다. (원본과 같다)
//!   create_instance, create_from_asset 은 GPU 업로드 없이 인스턴스를 짓고, 받기(create_model, load_from_url_async)는 자산 캐시를 쓴다.
//!   destroy 는 이 인스턴스 것(와이어프레임 버퍼, 갈라진 모프 텍스처, get_material 로 갈라진 머티리얼)을 지우고 자산을 놓는다.
//!   set_material 로 바깥에서 넣은 머티리얼은 지우지 않는다. (원본과 같다)
//!
//! # 예제
//! GL 없이 glTF 내용을 해석하고 포즈 계산하기 (스킨 없는 메시는 노드 자신이 조인트인 합성 스킨으로)
//! ```rust
//! use serde_json::json;
//! use vanilla::SkinnedModel;
//!
//! let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
//! let binary_buffer: Vec<u8> = positions.iter().flat_map(|value| value.to_le_bytes()).collect();
//! let json = json!({
//! 	"scenes": [{ "nodes": [0] }],
//! 	"nodes": [{ "name": "Triangle", "mesh": 0, "translation": [0.0, 2.0, 0.0] }],
//! 	"meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 } }] }],
//! 	"accessors": [{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" }],
//! 	"bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }]
//! });
//! let mut skinned_model = SkinnedModel::create_without_shader_program();
//! let (mesh_description_list, material_description_list) = skinned_model.interpret_content(&json, &binary_buffer).unwrap();
//! assert_eq!(mesh_description_list.len(), 1);
//! assert!(material_description_list.is_empty());
//! skinned_model.update(0.0);
//! let joint_matrix_array = &skinned_model.get_skin_list()[0].joint_matrix_array;
//! assert_eq!(joint_matrix_array[13], 2.0);
//! ```
//==============================================================================
use crate::base::matrix4::Matrix4;
use crate::base::quaternion::Quaternion;
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use crate::core::graphic::{GlBuffer, GlTexture, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use crate::core::task;
use crate::graphics3d::fbxloader::{FbxAnimation, FbxLoadHandle, FbxLoader, FbxSceneData};
use crate::graphics3d::material::{ImageDescription, Material, MaterialDescription};
use crate::graphics3d::skinnedmodelasset::{SkinnedModelAsset, SkinnedModelAssetContent};
use crate::graphics3d::skinnedmodelrenderer::SkinnedModelRenderer;
use glow::HasContext;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// GLB 머리와 청크 타입.
const GLB_MAGIC: u32 = 0x46546c67;
const GLB_CHUNKTYPE_JSON: u32 = 0x4e4f534a;
const GLB_CHUNKTYPE_BINARY: u32 = 0x004e4942;

/// 접근자 컴포넌트 타입. (원본 COMPONENT_ARRAY_TABLE 의 키)
pub const COMPONENT_TYPE_UNSIGNED_BYTE: u32 = 5121;
/// 부호 없는 16비트 정수. (glTF componentType 5123)
pub const COMPONENT_TYPE_UNSIGNED_SHORT: u32 = 5123;
/// 부호 없는 32비트 정수. (glTF componentType 5125)
pub const COMPONENT_TYPE_UNSIGNED_INT: u32 = 5125;
/// 32비트 실수. (glTF componentType 5126)
pub const COMPONENT_TYPE_FLOAT: u32 = 5126;

/// 모프 타깃 텍스처 가로 크기. (정점 델타를 실수 텍스처에 행 단위로 적재, 버텍스 셰이더가 gl_VertexID 로 조회)
const MORPH_TEXTURE_WIDTH: usize = 2048;

/// 드로어블당 모프 타깃 최대 개수. (셰이더 유니폼 배열 크기와 일치)
pub const MORPH_TARGET_MAXIMUM: usize = 96;

//==============================================================================
/// 애니메이션 채널 경로. (원본 문자열 "translation" / "rotation" / "scale" / "weights")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimationPath {
	/// 이동. (키마다 x, y, z 3개)
	Translation,
	/// 회전. (키마다 쿼터니언 x, y, z, w 4개)
	Rotation,
	/// 크기. (키마다 x, y, z 3개)
	Scale,
	/// 모프 가중치. (키마다 모프 타깃 수만큼)
	Weights,
}

impl AnimationPath {
	//==============================================================================
	/// glTF 경로 문자열로부터. (모르는 경로면 None)
	///
	/// # 인자
	/// - `path_name`: glTF 채널 경로. ("translation", "rotation", "scale", "weights", 대소문자까지 같아야 한다)
	///
	/// # 돌려줌
	/// 맞는 경로. 모르는 문자열이면 None.
	//==============================================================================
	pub fn from_name(path_name: &str) -> Option<AnimationPath> {
		match path_name {
			"translation" => {
				Some(AnimationPath::Translation)
			}
			"rotation" => {
				Some(AnimationPath::Rotation)
			}
			"scale" => {
				Some(AnimationPath::Scale)
			}
			"weights" => {
				Some(AnimationPath::Weights)
			}
			_ => {
				None
			}
		}
	}
}

//==============================================================================
/// 모델 노드. (본 계층 + 포즈 상태, 원본 nodeList 원소)
///
/// # 예제
/// 이름으로 노드를 찾아 월드 위치 읽기 (update 뒤의 값)
/// ```rust
/// use vanilla::{SkinnedModel, SkinnedModelNode};
///
/// fn find_world_position(skinned_model: &SkinnedModel, node_name: &str) -> Option<[f64; 3]> {
/// 	let node: &SkinnedModelNode = skinned_model.get_node_list().iter().find(|node| node.name == node_name)?;
/// 	let elements = node.world_matrix.read_elements();
/// 	Some([elements[12], elements[13], elements[14]])
/// }
///
/// let skinned_model = SkinnedModel::create_without_shader_program();
/// assert_eq!(find_world_position(&skinned_model, "Head"), None);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct SkinnedModelNode {
	/// 노드 이름. (glTF 노드 name, FBX 는 종류 꼬리를 뗀 Model 이름, 없으면 빈 문자열)
	pub name: String,
	/// 자식 노드 인덱스 목록.
	pub child_indices: Vec<usize>,
	/// glTF 메시 인덱스. (메시가 없거나 FBX 면 None)
	pub mesh_index: Option<usize>,
	/// 스킨 인덱스. (skin_list 안의 자리, 스킨 없는 glTF 메시 노드에는 합성 스킨이 붙는다. 없으면 None)
	pub skin_index: Option<usize>,
	/// 기본 포즈 이동. (update 가 매번 이 값에서 시작한다)
	pub base_translation: [f64; 3],
	/// 기본 포즈 회전. (FBX 는 사전 회전까지 곱한 것)
	pub base_rotation: Quaternion,
	/// 기본 포즈 크기.
	pub base_scale: [f64; 3],
	/// 지금 포즈 이동. (마지막 update 가 기본 포즈에 클립을 샘플링한 값)
	pub current_translation: [f64; 3],
	/// 지금 포즈 회전. (조인트 회전 오프셋까지 곱한 값)
	pub current_rotation: Quaternion,
	/// 지금 포즈 크기.
	pub current_scale: [f64; 3],
	/// 모델 공간 월드 행렬. (update 가 루트부터 계산한다, 루트에 닿지 않는 노드는 그대로)
	pub world_matrix: Matrix4,
}

//==============================================================================
/// 스킨. (조인트 노드 인덱스 + 역바인드 행렬 + 셰이더로 올릴 조인트 행렬 배열)
///
/// # 예제
/// 조인트 행렬 배열에서 조인트 하나의 행렬 꺼내기
/// ```rust
/// use vanilla::{Matrix4, Skin};
///
/// fn read_joint_matrix(skin: &Skin, joint_index: usize) -> Option<Matrix4> {
/// 	let elements = skin.joint_matrix_array.get(joint_index * 16..joint_index * 16 + 16)?;
/// 	let mut matrix = Matrix4::create_identity();
/// 	matrix.elements.copy_from_slice(elements);
/// 	Some(matrix)
/// }
///
/// let skin = Skin {
/// 	joint_node_indices: vec![0],
/// 	inverse_bind_matrices: vec![Matrix4::create_identity()],
/// 	joint_matrix_array: Matrix4::create_translation(1.0, 2.0, 3.0).get_elements().to_vec(),
/// };
/// assert_eq!(read_joint_matrix(&skin, 0).unwrap().elements[13], 2.0);
/// assert!(read_joint_matrix(&skin, 1).is_none());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skin {
	/// 조인트마다 노드 인덱스. (노드가 없는 조인트는 usize::MAX 로 두고 단위 행렬을 쓴다)
	pub joint_node_indices: Vec<usize>,
	/// 조인트마다 역바인드 행렬. (joint_node_indices 와 같은 자리)
	pub inverse_bind_matrices: Vec<Matrix4>,
	/// 셰이더로 올릴 조인트 행렬. (조인트마다 열 우선 16개, 월드 x 역바인드, update 가 채운다)
	pub joint_matrix_array: Vec<f32>,
}

//==============================================================================
/// 모프 타깃. (정점별 위치 델타 + 선택 노멀 델타)
///
/// # 예제
/// 정점 2개짜리 타깃을 모프 텍스처 텍셀로 펼치기
/// ```rust
/// use vanilla::{MorphTarget, SkinnedModel};
///
/// let morph_target = MorphTarget {
/// 	name: String::from("smile"),
/// 	position_deltas: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
/// 	normal_deltas: None,
/// };
/// let (texture_width, rows_per_target, texel_data) = SkinnedModel::build_morph_texel_data(&[morph_target], 2);
/// assert_eq!((texture_width, rows_per_target), (2, 1));
/// assert_eq!(texel_data.len(), 2 * 2 * 4);
/// assert_eq!(&texel_data[0..8], &[1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0]);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct MorphTarget {
	/// 타깃 이름. (glTF mesh.extras.targetNames, 없으면 "target0" 꼴)
	pub name: String,
	/// 정점마다 위치 델타. (x, y, z 3개씩)
	pub position_deltas: Vec<f64>,
	/// 정점마다 노멀 델타. (3개씩, 없으면 None)
	pub normal_deltas: Option<Vec<f64>>,
}

//==============================================================================
/// 인덱스 배열. (원본 Uint16Array / Uint32Array)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum IndexArray {
	/// 16비트 인덱스. (GL UNSIGNED_SHORT)
	UnsignedShort(Vec<u16>),
	/// 32비트 인덱스. (GL UNSIGNED_INT)
	UnsignedInt(Vec<u32>),
}

impl IndexArray {
	//==============================================================================
	/// 인덱스 개수.
	///
	/// # 돌려줌
	/// 인덱스 개수.
	//==============================================================================
	pub fn len(&self) -> usize {
		match self {
			IndexArray::UnsignedShort(indices) => {
				indices.len()
			}
			IndexArray::UnsignedInt(indices) => {
				indices.len()
			}
		}
	}

	//==============================================================================
	/// 비었는지.
	///
	/// # 돌려줌
	/// 비었으면 true.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	//==============================================================================
	/// 인덱스 하나 반환.
	///
	/// # 인자
	/// - `position`: 인덱스 자리. (0 부터, 밖이면 패닉)
	///
	/// # 돌려줌
	/// 그 자리의 인덱스 값.
	//==============================================================================
	pub fn get(&self, position: usize) -> usize {
		match self {
			IndexArray::UnsignedShort(indices) => {
				indices[position] as usize
			}
			IndexArray::UnsignedInt(indices) => {
				indices[position] as usize
			}
		}
	}

	//==============================================================================
	/// GL 원소 타입. (UNSIGNED_SHORT / UNSIGNED_INT)
	///
	/// # 돌려줌
	/// glow::UNSIGNED_SHORT 나 glow::UNSIGNED_INT.
	//==============================================================================
	pub fn get_gl_component_type(&self) -> u32 {
		match self {
			IndexArray::UnsignedShort(_) => {
				glow::UNSIGNED_SHORT
			}
			IndexArray::UnsignedInt(_) => {
				glow::UNSIGNED_INT
			}
		}
	}

	//==============================================================================
	/// 바이트로 보기. (GL 버퍼 업로드용)
	//==============================================================================
	fn as_bytes(&self) -> &[u8] {
		match self {
			IndexArray::UnsignedShort(indices) => {
				slice_to_bytes(indices)
			}
			IndexArray::UnsignedInt(indices) => {
				slice_to_bytes(indices)
			}
		}
	}
}

//==============================================================================
/// 접근자 배열. (원본 readAccessorArray 가 돌려주는 타입 배열, 컴포넌트 타입 그대로)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum AccessorArray {
	/// 부호 없는 8비트. (componentType 5121)
	UnsignedByte(Vec<u8>),
	/// 부호 없는 16비트. (componentType 5123)
	UnsignedShort(Vec<u16>),
	/// 부호 없는 32비트. (componentType 5125)
	UnsignedInt(Vec<u32>),
	/// 32비트 실수. (componentType 5126)
	Float(Vec<f32>),
}

impl AccessorArray {
	//==============================================================================
	/// 원소(컴포넌트) 개수.
	///
	/// # 돌려줌
	/// 컴포넌트 개수. (원소 수 x 원소당 컴포넌트 수)
	//==============================================================================
	pub fn len(&self) -> usize {
		match self {
			AccessorArray::UnsignedByte(values) => {
				values.len()
			}
			AccessorArray::UnsignedShort(values) => {
				values.len()
			}
			AccessorArray::UnsignedInt(values) => {
				values.len()
			}
			AccessorArray::Float(values) => {
				values.len()
			}
		}
	}

	//==============================================================================
	/// 비었는지.
	///
	/// # 돌려줌
	/// 비었으면 true.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	//==============================================================================
	/// 컴포넌트 타입. (5121 / 5123 / 5125 / 5126)
	///
	/// # 돌려줌
	/// glTF 컴포넌트 타입 번호. (COMPONENT_TYPE_UNSIGNED_BYTE 같은 상수)
	//==============================================================================
	pub fn get_component_type(&self) -> u32 {
		match self {
			AccessorArray::UnsignedByte(_) => {
				COMPONENT_TYPE_UNSIGNED_BYTE
			}
			AccessorArray::UnsignedShort(_) => {
				COMPONENT_TYPE_UNSIGNED_SHORT
			}
			AccessorArray::UnsignedInt(_) => {
				COMPONENT_TYPE_UNSIGNED_INT
			}
			AccessorArray::Float(_) => {
				COMPONENT_TYPE_FLOAT
			}
		}
	}

	//==============================================================================
	/// 실수 목록으로. (원본 new Float32Array(typedArray), 정수는 정규화하지 않고 값 그대로)
	///
	/// # 돌려줌
	/// f64 로 바꾼 새 목록.
	//==============================================================================
	pub fn to_float_list(&self) -> Vec<f64> {
		match self {
			AccessorArray::UnsignedByte(values) => {
				values.iter().map(|value| *value as f64).collect()
			}
			AccessorArray::UnsignedShort(values) => {
				values.iter().map(|value| *value as f64).collect()
			}
			AccessorArray::UnsignedInt(values) => {
				values.iter().map(|value| *value as f64).collect()
			}
			AccessorArray::Float(values) => {
				values.iter().map(|value| *value as f64).collect()
			}
		}
	}

	//==============================================================================
	/// 16비트 정수 목록으로. (원본 new Uint16Array(typedArray))
	///
	/// # 돌려줌
	/// u16 으로 바꾼 새 목록. (u32 는 아래 16비트만 남고, 실수는 소수점 아래를 버리고 0 ~ 65535 로 자른다)
	//==============================================================================
	pub fn to_unsigned_short_list(&self) -> Vec<u16> {
		match self {
			AccessorArray::UnsignedByte(values) => {
				values.iter().map(|value| *value as u16).collect()
			}
			AccessorArray::UnsignedShort(values) => {
				values.clone()
			}
			AccessorArray::UnsignedInt(values) => {
				values.iter().map(|value| *value as u16).collect()
			}
			AccessorArray::Float(values) => {
				values.iter().map(|value| *value as u16).collect()
			}
		}
	}

	//==============================================================================
	/// 인덱스 배열로. (원본: Uint8 은 Uint16 으로 넓히고 나머지는 그대로)
	///
	/// # 돌려줌
	/// 인덱스 배열. (UnsignedInt 는 32비트 그대로, 나머지는 16비트)
	//==============================================================================
	pub fn to_index_array(&self) -> IndexArray {
		match self {
			AccessorArray::UnsignedInt(values) => {
				IndexArray::UnsignedInt(values.clone())
			}
			_ => {
				IndexArray::UnsignedShort(self.to_unsigned_short_list())
			}
		}
	}
}

//==============================================================================
/// FBX 코너 배열. (원본 splitFbxCornersByMaterial 의 cornerArrays, 비인덱스 코너마다 값)
///
/// # 예제
/// 삼각형 하나의 코너 배열 (재질 번호가 하나뿐이면 그대로 돌아온다)
/// ```rust
/// use vanilla::SkinnedModel;
/// use vanilla::graphics3d::skinnedmodel::FbxCornerArrays;
///
/// let corner_arrays = FbxCornerArrays {
/// 	positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
/// 	normals: vec![0.0; 9],
/// 	texture_coordinates: vec![0.0; 6],
/// 	joints: vec![0; 12],
/// 	weights: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
/// };
/// let sub_mesh_list = SkinnedModel::split_fbx_corners_by_material(&[3], corner_arrays);
/// assert_eq!(sub_mesh_list.len(), 1);
/// assert_eq!(sub_mesh_list[0].0, 3);
/// assert_eq!(sub_mesh_list[0].1.positions.len(), 9);
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxCornerArrays {
	/// 코너 위치. (코너마다 x, y, z)
	pub positions: Vec<f64>,
	/// 코너 노멀. (코너마다 3개)
	pub normals: Vec<f64>,
	/// 코너 UV. (코너마다 2개)
	pub texture_coordinates: Vec<f64>,
	/// 코너마다 조인트 4개. (스킨 joint_node_indices 안의 자리)
	pub joints: Vec<u16>,
	/// 코너마다 조인트 가중치 4개.
	pub weights: Vec<f64>,
}

//==============================================================================
/// 중립 메시 서술. (원본 meshDescription, 순수 CPU 배열)
/// - joints / weights 는 정점마다 4개, material_index 가 None 이면 기본 머티리얼, node_index 는 모프 가중치 채널의 대상 노드.
///
/// # 예제
/// 스킨 없는 glTF 삼각형의 서술 (합성 스킨 하나에 가중치 1)
/// ```rust
/// use serde_json::json;
/// use vanilla::SkinnedModel;
///
/// let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
/// let binary_buffer: Vec<u8> = positions.iter().flat_map(|value| value.to_le_bytes()).collect();
/// let json = json!({
/// 	"scenes": [{ "nodes": [0] }],
/// 	"nodes": [{ "name": "Triangle", "mesh": 0 }],
/// 	"meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 } }] }],
/// 	"accessors": [{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" }],
/// 	"bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }]
/// });
/// let mut skinned_model = SkinnedModel::create_without_shader_program();
/// let (mesh_description_list, _material_description_list) = skinned_model.interpret_content(&json, &binary_buffer).unwrap();
/// let mesh_description = &mesh_description_list[0];
/// assert_eq!(mesh_description.positions.len(), 9);
/// assert!(mesh_description.normals.is_none());
/// assert!(mesh_description.indices.is_none());
/// assert_eq!(&mesh_description.weights[0..4], &[1.0, 0.0, 0.0, 0.0]);
/// assert_eq!(mesh_description.material_index, None);
/// assert_eq!(skinned_model.get_skin_list()[mesh_description.skin_index].joint_node_indices, vec![0]);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct MeshDescription {
	/// 정점 위치. (정점마다 x, y, z)
	pub positions: Vec<f64>,
	/// 정점 노멀. (3개씩, None 이면 업로드할 때 스무스 노멀을 지어 쓴다)
	pub normals: Option<Vec<f64>>,
	/// 정점 UV. (2개씩, 없으면 None)
	pub texture_coordinates: Option<Vec<f64>>,
	/// 정점마다 조인트 4개. (스킨 joint_node_indices 안의 자리)
	pub joints: Vec<u16>,
	/// 정점마다 조인트 가중치 4개. (0 ~ 1)
	pub weights: Vec<f64>,
	/// 삼각형 인덱스. (None 이면 정점 3개씩 삼각형으로 그린다)
	pub indices: Option<IndexArray>,
	/// 쓰는 스킨 인덱스. (모델 skin_list 안의 자리)
	pub skin_index: usize,
	/// 머티리얼 서술 인덱스. (None 이면 기본 머티리얼)
	pub material_index: Option<usize>,
	/// 머티리얼 이름. (glTF materials 의 name, FBX 는 Material 이름, 없으면 빈 문자열)
	pub material_name: String,
	/// 이 메시를 가진 노드 인덱스. (모프 가중치 채널의 대상, 없으면 None)
	pub node_index: Option<usize>,
	/// 모프 타깃 목록.
	pub morph_targets: Vec<MorphTarget>,
	/// 타깃마다 기본 가중치. (glTF 노드나 메시의 weights, 없으면 0)
	pub morph_weights: Vec<f32>,
}

//==============================================================================
/// 드로어블. (원본 drawableList 원소, 버텍스 어레이, 인덱스, 스킨, 머티리얼 참조, 모프 상태)
///
/// # 예제
/// 머티리얼 이름으로 드로어블 자리 찾기 (set_material 에 넘길 자리)
/// ```rust
/// use vanilla::SkinnedModel;
///
/// fn find_drawable_index(skinned_model: &SkinnedModel, material_name: &str) -> Option<usize> {
/// 	skinned_model.get_drawable_list().iter().position(|drawable| drawable.material_name == material_name)
/// }
///
/// let skinned_model = SkinnedModel::create_without_shader_program();
/// assert_eq!(find_drawable_index(&skinned_model, "Body"), None);
/// ```
//==============================================================================
#[derive(Clone)]
pub struct Drawable {
	/// 버텍스 어레이. (정점 버퍼와 인덱스 버퍼를 묶은 것)
	pub vertex_array: GlVertexArray,
	/// 정점 버퍼 목록. (셰이더가 쓰는 어트리뷰트만)
	pub vertex_buffer_list: Vec<GlBuffer>,
	/// 인덱스로 그리는지. (false 면 drawArrays)
	pub is_indexed: bool,
	/// 인덱스 버퍼. (인덱스가 없으면 None)
	pub index_buffer: Option<GlBuffer>,
	/// 인덱스 개수.
	pub index_count: usize,
	/// 인덱스 GL 타입. (UNSIGNED_SHORT 나 UNSIGNED_INT, 인덱스가 없으면 0)
	pub index_component_type: u32,
	/// 인덱스 버퍼 시작 바이트. (업로드는 0 으로 둔다)
	pub index_byte_offset: i32,
	/// 와이어프레임 선 인덱스 버퍼. (처음 draw_wireframe 할 때 짓는다, 그 전에는 None)
	pub wireframe_index_buffer: Option<GlBuffer>,
	/// 선 인덱스 개수. (모서리 수 x 2)
	pub wireframe_index_count: usize,
	/// 선 인덱스 GL 타입. (정점이 65535 개를 넘으면 UNSIGNED_INT)
	pub wireframe_index_component_type: u32,
	/// 정점 개수.
	pub vertex_count: usize,
	/// 머티리얼. (같은 모델의 다른 드로어블이나 다른 곳과 Rc 로 나눠 들 수 있다)
	pub material: Rc<Material>,
	/// 머티리얼 이름. (MeshDescription 의 material_name)
	pub material_name: String,
	/// 쓰는 스킨 인덱스.
	pub skin_index: usize,
	/// 메시를 가진 노드 인덱스. (모프 가중치 채널의 대상)
	pub node_index: Option<usize>,
	/// 업로드한 메시 서술. (모프 타깃은 morph_target_list 로 옮겨 비어 있다, 같은 자산의 인스턴스들이 나눠 든다)
	pub mesh_description: Rc<MeshDescription>,
	/// 모프 타깃 목록. (MORPH_TARGET_MAXIMUM 개까지, 자산과 나눠 들고 add_morph_target 하면 이 드로어블 것으로 갈라진다)
	pub morph_target_list: Rc<Vec<MorphTarget>>,
	/// 타깃마다 가중치. (morph_target_list 와 같은 자리)
	pub morph_weights: Vec<f32>,
	/// 모프 델타 텍스처. (RGBA32F, 타깃이 없거나 올리기 전이면 None)
	pub morph_texture: Option<GlTexture>,
	/// 타깃 블록 하나의 행 수. (위치 블록과 노멀 블록이 각각 이만큼)
	pub morph_rows_per_target: i32,
	/// 모프 텍스처 가로. (텍셀, 정점 수를 1 ~ 2048 로 자른 값)
	pub morph_texture_width: i32,
	/// 모프 텍스처를 자산과 나눠 쓰는지. (true 면 destroy 가 지우지 않고, 다시 올릴 때 이 드로어블 것을 새로 짓는다)
	pub is_morph_texture_shared: bool,
	/// 모프 타깃을 다시 올려야 하는지. (렌더러가 그리기 전에 upload_pending_morph_targets 로 올린다)
	pub is_morph_dirty: bool,
}

//==============================================================================
/// 인스턴스 드로어블 만들기. (버퍼, 버텍스 어레이, 메시 서술, 모프 타깃, 머티리얼 원형은 나눠 쓰고 모프 가중치와 와이어프레임 버퍼는 따로)
//==============================================================================
fn create_instance_drawable(template_drawable: &Drawable) -> Drawable {
	let mut drawable = template_drawable.clone();
	drawable.wireframe_index_buffer = None;
	drawable.wireframe_index_count = 0;
	drawable.wireframe_index_component_type = 0;
	drawable.is_morph_texture_shared = template_drawable.morph_texture.is_some();
	drawable.is_morph_dirty = false;
	drawable
}

//==============================================================================
/// 인스턴스 스킨 만들기. (조인트와 역바인드 행렬을 베끼고 조인트 행렬 배열은 0 으로 따로)
//==============================================================================
fn create_instance_skin(template_skin: &Skin) -> Skin {
	let mut skin = template_skin.clone();
	skin.joint_matrix_array = vec![0.0; template_skin.joint_matrix_array.len()];
	skin
}

//==============================================================================
/// 클립 더하기. (같은 이름 클립은 그 자리를 바꾼다, 클립 목록은 자산의 인스턴스들이 나눠 써 인스턴스마다 더해도 한 벌만 남는다)
//==============================================================================
fn merge_animation_clips(animation_list: &mut Vec<AnimationClip>, added_clip_list: Vec<AnimationClip>) {
	for added_clip in added_clip_list {
		match animation_list.iter().position(|animation| animation.name == added_clip.name) {
			Some(same_name_index) => {
				animation_list[same_name_index] = added_clip;
			}
			None => {
				animation_list.push(added_clip);
			}
		}
	}
}

//==============================================================================
/// 드로어블 모프 타깃 업로드. (upload_morph_targets 본문, 자산과 나눠 쓰던 텍스처면 건드리지 않고 이 드로어블 것을 새로 짓는다)
//==============================================================================
fn upload_drawable_morph_targets(gl: &glow::Context, drawable: &mut Drawable) -> Result<(), String> {
	if drawable.is_morph_texture_shared {
		drawable.morph_texture = None;
		drawable.is_morph_texture_shared = false;
	}
	let target_count = drawable.morph_target_list.len();
	let vertex_count = drawable.vertex_count;
	if target_count == 0 || vertex_count == 0 {
		if let Some(morph_texture) = drawable.morph_texture.take() {
			unsafe {
				crate::core::glstatecache::delete_texture(gl, morph_texture);
			}
		}
		drawable.is_morph_dirty = false;
		return Ok(());
	}
	let (texture_width, rows_per_target, texel_data) = SkinnedModel::build_morph_texel_data(&drawable.morph_target_list, vertex_count);
	let texture_height = rows_per_target * 2 * target_count;
	unsafe {
		let morph_texture = match drawable.morph_texture {
			Some(morph_texture) => {
				morph_texture
			}
			None => {
				crate::core::glstatecache::create_texture(gl)?
			}
		};
		drawable.morph_texture = Some(morph_texture);
		crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(morph_texture));
		gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, texture_width as i32, texture_height as i32, 0, glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(Some(slice_to_bytes(&texel_data))));
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
		crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, None);
	}
	drawable.morph_rows_per_target = rows_per_target as i32;
	drawable.morph_texture_width = texture_width as i32;
	drawable.is_morph_dirty = false;
	Ok(())
}

//==============================================================================
/// 애니메이션 채널. (노드 인덱스 + 경로 + 시간/값 커브 + 키프레임 커서)
///
/// # 예제
/// 클립에서 회전 채널만 세기
/// ```rust
/// use vanilla::{AnimationClip, AnimationPath};
/// use vanilla::graphics3d::skinnedmodel::AnimationChannel;
///
/// let animation_clip = AnimationClip {
/// 	name: String::from("Rise"),
/// 	duration: 2.0,
/// 	channels: vec![AnimationChannel {
/// 		node_index: 0,
/// 		path: AnimationPath::Translation,
/// 		times: vec![0.0, 2.0],
/// 		values: vec![0.0, 0.0, 0.0, 0.0, 4.0, 0.0],
/// 		cursor: 0,
/// 	}],
/// };
/// let rotation_channel_count = animation_clip.channels.iter().filter(|channel| channel.path == AnimationPath::Rotation).count();
/// assert_eq!(rotation_channel_count, 0);
/// assert_eq!(animation_clip.channels[0].times.last().copied(), Some(animation_clip.duration));
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct AnimationChannel {
	/// 대상 노드 인덱스.
	pub node_index: usize,
	/// 채널 경로.
	pub path: AnimationPath,
	/// 키 시간 목록. (초, 오름차순)
	pub times: Vec<f64>,
	/// 키마다 값. (이동과 크기는 3개, 회전은 쿼터니언 4개, 모프 가중치는 타깃 수만큼)
	pub values: Vec<f64>,
	/// 키프레임 커서. (지난 샘플링의 키 자리, 시간이 뒤로 가면 0 부터 다시 찾는다)
	pub cursor: usize,
}

//==============================================================================
/// 애니메이션 클립.
///
/// # 예제
/// 가장 긴 클립 이름 찾기
/// ```rust
/// use vanilla::SkinnedModel;
///
/// fn find_longest_clip_name(skinned_model: &SkinnedModel) -> Option<String> {
/// 	let animation_list = skinned_model.get_animation_list();
/// 	let longest_clip = animation_list.iter().max_by(|left, right| left.duration.total_cmp(&right.duration))?;
/// 	Some(longest_clip.name.clone())
/// }
///
/// let skinned_model = SkinnedModel::create_without_shader_program();
/// assert_eq!(find_longest_clip_name(&skinned_model), None);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct AnimationClip {
	/// 클립 이름. (glTF 애니메이션 name, FBX 는 스택 이름이나 붙일 때 준 이름)
	pub name: String,
	/// 길이. (초, 채널 마지막 키 시간 가운데 가장 큰 것, 이 길이로 반복한다)
	pub duration: f64,
	/// 채널 목록.
	pub channels: Vec<AnimationChannel>,
}

//==============================================================================
/// 스킨드 모델.
///
/// # 예제
/// 클립 둘이 든 glTF 를 해석하고 1초 크로스페이드로 넘기기 (GL 없이 포즈만)
/// ```rust
/// use serde_json::json;
/// use vanilla::SkinnedModel;
///
/// let float_values: [f32; 23] = [
/// 	0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0,
/// 	0.0, 2.0,
/// 	0.0, 0.0, 0.0, 0.0, 4.0, 0.0,
/// 	0.0, 1.0, 0.0, 0.0, 1.0, 0.0,
/// ];
/// let binary_buffer: Vec<u8> = float_values.iter().flat_map(|value| value.to_le_bytes()).collect();
/// let json = json!({
/// 	"scenes": [{ "nodes": [0] }],
/// 	"nodes": [{ "name": "Triangle", "mesh": 0 }],
/// 	"meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 } }] }],
/// 	"accessors": [
/// 		{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
/// 		{ "bufferView": 1, "componentType": 5126, "count": 2, "type": "SCALAR" },
/// 		{ "bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3" },
/// 		{ "bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3" }
/// 	],
/// 	"bufferViews": [
/// 		{ "buffer": 0, "byteOffset": 0, "byteLength": 36 },
/// 		{ "buffer": 0, "byteOffset": 36, "byteLength": 8 },
/// 		{ "buffer": 0, "byteOffset": 44, "byteLength": 24 },
/// 		{ "buffer": 0, "byteOffset": 68, "byteLength": 24 }
/// 	],
/// 	"animations": [
/// 		{ "name": "Rise", "samplers": [{ "input": 1, "output": 2 }], "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }] },
/// 		{ "name": "Hold", "samplers": [{ "input": 1, "output": 3 }], "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }] }
/// 	]
/// });
/// let mut skinned_model = SkinnedModel::create_without_shader_program();
/// skinned_model.interpret_content(&json, &binary_buffer).unwrap();
/// assert_eq!(skinned_model.get_animation_name_list(), vec![String::from("Rise"), String::from("Hold")]);
/// skinned_model.update(1.0);
/// assert_eq!(skinned_model.get_node_list()[0].current_translation, [0.0, 2.0, 0.0]);
/// skinned_model.play_animation("Hold", 1.0);
/// skinned_model.update(0.5);
/// assert_eq!(skinned_model.get_node_list()[0].current_translation, [0.0, 2.0, 0.0]);
/// skinned_model.update(0.5);
/// assert_eq!(skinned_model.get_node_list()[0].current_translation, [0.0, 1.0, 0.0]);
/// ```
//==============================================================================
pub struct SkinnedModel {
	shader_program: Option<Rc<ShaderProgram>>,
	node_list: Vec<SkinnedModelNode>,
	root_node_indices: Vec<usize>,
	skin_list: Vec<Skin>,
	drawable_list: Vec<Drawable>,
	/// 애니메이션 클립 목록. (애니메이션 FBX 받기 손잡이와 나눠 들어, await 하는 퓨처가 클립을 더할 수 있다)
	animation_list: Rc<RefCell<Vec<AnimationClip>>>,
	current_animation_index: Option<usize>,
	previous_animation_index: Option<usize>,
	current_time: f64,
	previous_time: f64,
	fade_duration: f64,
	fade_elapsed: f64,
	time_scale: f64,
	joint_rotation_offsets: Vec<(usize, Quaternion)>,
	/// 받는 중인 애니메이션 FBX. (add_animations_from_fbx_url)
	pending_animation_load_list: Vec<AnimationLoadHandle>,
	/// 나눠 쓰는 자산. (올리기 전이나 destroy 뒤에는 None)
	asset: Option<Rc<SkinnedModelAsset>>,
	/// get_material 로 갈라진 이 인스턴스의 머티리얼. (destroy 가 지운다)
	instance_material_list: Vec<Rc<Material>>,
}

//==============================================================================
/// 애니메이션 FBX 받기의 나눠 든 상태. (받기 손잡이, 클립 이름, 이동 무시, 부탁할 때의 노드 이름 표, 모델의 클립 목록, 더했는지)
//==============================================================================
struct AnimationLoad {
	fbx_load_handle: FbxLoadHandle,
	clip_name: String,
	ignore_translation: bool,
	node_index_by_name: HashMap<String, usize>,
	animation_list: Rc<RefCell<Vec<AnimationClip>>>,
	apply_result: RefCell<Option<Result<(), String>>>,
}

impl AnimationLoad {
	//==============================================================================
	/// 다 받았으면 클립 더하기. (끝났으면 결과, 받는 중이거나 클립 목록을 빌릴 수 없으면 None)
	//==============================================================================
	fn apply(&self) -> Option<Result<(), String>> {
		if let Some(apply_result) = self.apply_result.borrow().clone() {
			return Some(apply_result);
		}
		let scene_data_result = self.fbx_load_handle.get_scene_data()?;
		let apply_result = match scene_data_result {
			Ok(scene_data) => {
				let mut added_clip_list = convert_fbx_scene_animations(&self.node_index_by_name, &scene_data, &self.clip_name, self.ignore_translation);
				let mut animation_list = self.animation_list.try_borrow_mut().ok()?;
				merge_animation_clips(&mut animation_list, std::mem::take(&mut added_clip_list));
				Ok(())
			}
			Err(error_message) => {
				eprintln!("{}", error_message);
				Err(error_message)
			}
		};
		*self.apply_result.borrow_mut() = Some(apply_result.clone());
		Some(apply_result)
	}
}

//==============================================================================
/// 애니메이션 FBX 받기 손잡이. (SkinnedModel::add_animations_from_fbx_url 이 돌려준다. 원본 addAnimationsFromFbxUrl 의 Promise 자리)
/// - await 하면 다 받아 클립을 모델에 더할 때까지 기다린다. 받기나 파싱에 실패하면 까닭으로 끝난다. (원본 reject)
///
/// # 예제
/// 걷기 클립을 붙이고 다 붙은 뒤 틀기
/// ```rust,no_run
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{Engine, SkinnedModel};
///
/// fn add_walk_clip(engine: &Engine, skinned_model: Rc<RefCell<SkinnedModel>>) {
/// 	let animation_load_handle = skinned_model.borrow_mut().add_animations_from_fbx_url(engine, "anims/walk.fbx", "Walk", true);
/// 	engine.spawn_task(move |_engine_context| async move {
/// 		if animation_load_handle.await.is_ok() {
/// 			skinned_model.borrow_mut().play_animation("Walk", 0.25);
/// 		}
/// 	});
/// }
/// ```
//==============================================================================
#[derive(Clone)]
pub struct AnimationLoadHandle {
	animation_load: Rc<AnimationLoad>,
}

impl AnimationLoadHandle {
	//==============================================================================
	/// 바이트 읽기 손잡이. (씬에 맡길 때)
	///
	/// # 돌려줌
	/// FBX 바이트 읽기 손잡이. (SceneBase::track_load_handle 에 넘긴다)
	//==============================================================================
	pub fn get_load_handle(&self) -> &AssetLoadHandle {
		self.animation_load.fbx_load_handle.get_load_handle()
	}

	//==============================================================================
	/// 끝났는지. (클립을 더했거나 실패했거나)
	///
	/// # 돌려줌
	/// 클립을 더했거나 받기, 파싱에 실패했으면 true. (바이트를 다 받아도 update, play_animation, await 가운데 하나가 더하기 전에는 false)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.animation_load.apply_result.borrow().is_some()
	}
}

impl std::fmt::Debug for AnimationLoadHandle {
	//==============================================================================
	/// 디버그 출력. (클립 이름과 끝났는지)
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("AnimationLoadHandle").field("clip_name", &self.animation_load.clip_name).field("is_finished", &self.is_finished()).finish()
	}
}

//==============================================================================
/// 애니메이션 FBX 받기를 기다리는 퓨처. (다 받으면 클립을 더하고 끝난다)
///
/// # 예제
/// 손잡이를 퓨처로 바꿔 실패 까닭 받기
/// ```rust,no_run
/// use std::future::IntoFuture;
/// use vanilla::{AnimationLoadFuture, Engine, SkinnedModel};
///
/// fn request_idle_clip(engine: &Engine, skinned_model: &mut SkinnedModel) {
/// 	let animation_load_future: AnimationLoadFuture = skinned_model.add_animations_from_fbx_url(engine, "anims/idle.fbx", "Idle", false).into_future();
/// 	engine.spawn_task(move |_engine_context| async move {
/// 		if let Err(error_message) = animation_load_future.await {
/// 			eprintln!("{}", error_message);
/// 		}
/// 	});
/// }
/// ```
//==============================================================================
#[derive(Clone)]
pub struct AnimationLoadFuture {
	animation_load: Rc<AnimationLoad>,
}

impl Future for AnimationLoadFuture {
	type Output = Result<(), String>;

	//==============================================================================
	/// 다 받았으면 클립을 더하고 끝내기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut AnimationLoadFuture>, context: &mut Context<'_>) -> Poll<Result<(), String>> {
		match self.animation_load.apply() {
			Some(apply_result) => {
				Poll::Ready(apply_result)
			}
			None => {
				context.waker().wake_by_ref();
				Poll::Pending
			}
		}
	}
}

impl IntoFuture for AnimationLoadHandle {
	type Output = Result<(), String>;
	type IntoFuture = AnimationLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> AnimationLoadFuture {
		AnimationLoadFuture {
			animation_load: self.animation_load,
		}
	}
}

//==============================================================================
/// 애니메이션 FBX 의 클립 이름. (채널이 있는 스택이 하나면 받은 이름 그대로, 여럿이면 "받은 이름/스택 이름")
/// - 스택 이름은 FbxLoader 가 종류 꼬리를 뗀 이름이다. (비었으면 채널 있는 스택 사이의 차례 번호)
/// - 받은 이름이 비었으면 스택 이름만 쓴다.
//==============================================================================
fn create_fbx_clip_name(clip_name: &str, stack_name: &str, stack_index: usize, stack_count: usize) -> String {
	if stack_count <= 1 {
		return clip_name.to_string();
	}
	let stack_label = if stack_name.is_empty() {
		stack_index.to_string()
	}
	else {
		stack_name.to_string()
	};
	if clip_name.is_empty() {
		stack_label
	}
	else {
		format!("{}/{}", clip_name, stack_label)
	}
}

//==============================================================================
/// FBX 의 애니메이션들을 모델의 클립으로 바꾸기. (노드는 이름으로 맞추고, 이동 무시면 이동 채널을 뺀다. 채널이 없는 것은 뺀다)
/// - 채널이 있는 스택이 여럿이면 스택마다 제 이름의 클립이다. (create_fbx_clip_name)
//==============================================================================
fn convert_fbx_scene_animations(node_index_by_name: &HashMap<String, usize>, scene_data: &FbxSceneData, clip_name: &str, ignore_translation: bool) -> Vec<AnimationClip> {
	let mut added_clip_list = Vec::new();
	let channel_animation_list: Vec<_> = scene_data.animation_list.iter().filter(|fbx_animation| !fbx_animation.channels.is_empty()).collect();
	let stack_count = channel_animation_list.len();
	for (stack_index, fbx_animation) in channel_animation_list.into_iter().enumerate() {
		let mut channels = convert_fbx_animation_channels(scene_data, fbx_animation, |node_index| {
			let source_name = scene_data.node_list.get(node_index)?.name.as_str();
			node_index_by_name.get(source_name).copied()
		});
		if ignore_translation {
			channels.retain(|channel| channel.path != AnimationPath::Translation);
		}
		added_clip_list.push(AnimationClip {
			name: create_fbx_clip_name(clip_name, &fbx_animation.name, stack_index, stack_count),
			duration: fbx_animation.duration,
			channels,
		});
	}
	added_clip_list
}

//==============================================================================
/// 모델 파일 꼴. (받기 손잡이가 어느 해석기로 지을지)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SkinnedModelFileFormat {
	Glb,
	Fbx,
}

thread_local! {
	/// 받는 중인 모델 바이트. (정규화한 자산 키 → 받기 손잡이, 같은 파일을 함께 받으면 한 번만 읽는다. 원본 pendingAssetLoadTable 자리)
	static PENDING_MODEL_LOAD_TABLE: RefCell<HashMap<String, AssetLoadHandle>> = RefCell::new(HashMap::new());
}

//==============================================================================
/// 모델 바이트 부탁. (같은 키를 받는 중이면 그 손잡이를 나눠 들고, 끝난 것은 표에서 뺀다)
//==============================================================================
fn request_model_bytes(engine: &Engine, url: &str) -> AssetLoadHandle {
	let asset_key = SkinnedModelAsset::normalize_key(url);
	PENDING_MODEL_LOAD_TABLE.with(|pending_model_load_table| {
		let mut pending_model_load_table = pending_model_load_table.borrow_mut();
		pending_model_load_table.retain(|_, load_handle| !load_handle.is_finished());
		if let Some(load_handle) = pending_model_load_table.get(&asset_key) {
			return load_handle.clone();
		}
		let load_handle = engine.request_asset_bytes(url);
		if !load_handle.is_finished() {
			pending_model_load_table.insert(asset_key, load_handle.clone());
		}
		load_handle
	})
}

//==============================================================================
/// 지은 모델의 자산을 캐시에 넣기. (받기가 지은 모델, 실패는 그대로)
//==============================================================================
fn register_loaded_model(asset_key: &str, model_result: Result<SkinnedModel, String>) -> Result<SkinnedModel, String> {
	let skinned_model = model_result?;
	if let Some(skinned_model_asset) = skinned_model.get_asset() {
		SkinnedModelAsset::register(asset_key, skinned_model_asset)?;
	}
	Ok(skinned_model)
}

//==============================================================================
/// 모델 받기 손잡이. (SkinnedModel::load_from_url, load_from_fbx_url 이 돌려준다. 원본 loadFromUrl, loadFromFbxUrl 의 Promise 자리)
/// - 씬은 get_load_handle 을 SceneBase::track_load_handle 로 맡겨 다 받을 때까지 initialize 를 미룰 수 있다.
/// - 모델은 GL 이 있는 자리에서 create_model(gl, 렌더러)로 짓는다.
///
/// # 예제
/// 씬이 받기를 맡기고 GL 이 있는 draw 에서 렌더러와 모델을 짓는다
/// ```rust,no_run
/// use vanilla::{Engine, Graphic, Scene, SceneBase, SkinnedModel, SkinnedModelLoadHandle, SkinnedModelRenderer};
///
/// struct CharacterScene {
/// 	scene_base: SceneBase,
/// 	model_load_handle: Option<SkinnedModelLoadHandle>,
/// 	skinned_model_renderer: Option<SkinnedModelRenderer>,
/// 	skinned_model: Option<SkinnedModel>,
/// }
///
/// impl Scene for CharacterScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn load(&mut self, engine: &Engine) {
/// 		let model_load_handle = SkinnedModel::load_from_url(engine, "fox.glb");
/// 		self.scene_base.track_load_handle(model_load_handle.get_load_handle());
/// 		self.model_load_handle = Some(model_load_handle);
/// 	}
///
/// 	fn tick(&mut self, time_delta: f64) {
/// 		self.scene_base.tick(time_delta);
/// 		if let Some(skinned_model) = &mut self.skinned_model {
/// 			skinned_model.update(time_delta);
/// 		}
/// 	}
///
/// 	fn draw(&mut self, graphic: &mut Graphic) {
/// 		let gl = graphic.get_gl();
/// 		if self.skinned_model_renderer.is_none() {
/// 			self.skinned_model_renderer = SkinnedModelRenderer::new(gl, None, None).ok();
/// 		}
/// 		let model_result = match (&self.skinned_model_renderer, &self.model_load_handle) {
/// 			(Some(skinned_model_renderer), Some(model_load_handle)) => {
/// 				model_load_handle.create_model(gl, skinned_model_renderer)
/// 			}
/// 			_ => {
/// 				None
/// 			}
/// 		};
/// 		if let Some(model_result) = model_result {
/// 			self.model_load_handle = None;
/// 			match model_result {
/// 				Ok(skinned_model) => {
/// 					self.skinned_model = Some(skinned_model);
/// 				}
/// 				Err(error_message) => {
/// 					eprintln!("{}", error_message);
/// 				}
/// 			}
/// 		}
/// 	}
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct SkinnedModelLoadHandle {
	load_handle: AssetLoadHandle,
	file_format: SkinnedModelFileFormat,
}

impl SkinnedModelLoadHandle {
	//==============================================================================
	/// 바이트 읽기 손잡이. (씬에 맡길 때)
	///
	/// # 돌려줌
	/// 바이트 읽기 손잡이. (SceneBase::track_load_handle 에 넘긴다)
	//==============================================================================
	pub fn get_load_handle(&self) -> &AssetLoadHandle {
		&self.load_handle
	}

	//==============================================================================
	/// 끝났는지. (받았거나 실패했거나)
	///
	/// # 돌려줌
	/// 받기가 끝났으면(받았거나 실패했으면) true.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.load_handle.is_finished()
	}

	//==============================================================================
	/// 받은 바이트로 모델 짓기. (받는 중이면 None, 받기나 해석에 실패하면 Err. 원본 문구 "SkinnedModel load failed: url")
	/// - 자산 캐시를 쓴다. 같은 셰이더 프로그램으로 같은 주소(SkinnedModelAsset::normalize_key)의 자산이 있으면 풀지도 올리지도 않고
	///   그 자산을 나눠 쓰는 새 인스턴스를 돌려준다. 없으면 받은 바이트로 지어 캐시에 넣는다. (끝난 뒤 다시 불러도 인스턴스가 하나 더 생긴다)
	/// - 손잡이는 부탁할 때 렌더러를 모르므로 바이트는 읽는다. (같은 파일을 함께 받는 중이면 한 번만 읽는다)
	///
	/// # 인자
	/// - `gl`: 모델을 지을 GL 컨텍스트. (graphic.get_gl())
	/// - `skinned_model_renderer`: 셰이더 프로그램을 나눠 줄 렌더러.
	///
	/// # 돌려줌
	/// 받는 중이면 None. 끝났으면 지은 모델, 받기에 실패하면 Err("SkinnedModel load failed: 경로", FBX 는 "FbxLoader load failed: 경로"), 해석이나 업로드에 실패하면 그 Err.
	//==============================================================================
	pub fn create_model(&self, gl: &glow::Context, skinned_model_renderer: &SkinnedModelRenderer) -> Option<Result<SkinnedModel, String>> {
		self.create_model_with_shader_program(gl, skinned_model_renderer.get_shader_program())
	}

	//==============================================================================
	/// 받은 바이트로 모델 짓기 본문. (create_model 과 같고 렌더러 대신 셰이더 프로그램을 받는다)
	//==============================================================================
	fn create_model_with_shader_program(&self, gl: &glow::Context, shader_program: Rc<ShaderProgram>) -> Option<Result<SkinnedModel, String>> {
		if !self.load_handle.is_finished() {
			return None;
		}
		let asset_key = SkinnedModelAsset::normalize_key(self.load_handle.get_asset_path());
		if let Some(cached_asset) = SkinnedModelAsset::find(&asset_key, &shader_program) {
			return Some(SkinnedModel::create_from_asset(&cached_asset));
		}
		let bytes = match self.load_handle.get_bytes() {
			Some(bytes) => {
				bytes
			}
			None => {
				let error_label = match self.file_format {
					SkinnedModelFileFormat::Glb => {
						"SkinnedModel"
					}
					SkinnedModelFileFormat::Fbx => {
						"FbxLoader"
					}
				};
				return Some(Err(format!("{} load failed: {}", error_label, self.load_handle.get_asset_path())));
			}
		};
		let model_result = match self.file_format {
			SkinnedModelFileFormat::Glb => {
				SkinnedModel::load_from_bytes_with_shader_program(gl, &bytes, shader_program)
			}
			SkinnedModelFileFormat::Fbx => {
				SkinnedModel::load_from_fbx_bytes_with_shader_program(gl, &bytes, shader_program)
			}
		};
		Some(register_loaded_model(&asset_key, model_result))
	}

	//==============================================================================
	/// 다 받으면 모델을 짓는 퓨처. (load_from_url_async, load_from_fbx_url_async 가 돌려준다. GL 은 작업을 폴링하는 엔진의 것을 쓴다)
	//==============================================================================
	fn create_model_when_loaded(self, shader_program: Rc<ShaderProgram>) -> impl Future<Output = Result<SkinnedModel, String>> + 'static {
		async move {
			let asset_key = SkinnedModelAsset::normalize_key(self.load_handle.get_asset_path());
			let bytes_result = (&self.load_handle).await;
			// 함께 받던 다른 쪽이 먼저 지었으면 나눠 쓴다.
			if let Some(cached_asset) = SkinnedModelAsset::find(&asset_key, &shader_program) {
				return SkinnedModel::create_from_asset(&cached_asset);
			}
			let bytes = match bytes_result {
				Ok(bytes) => {
					bytes
				}
				Err(_) => {
					let error_label = match self.file_format {
						SkinnedModelFileFormat::Glb => {
							"SkinnedModel"
						}
						SkinnedModelFileFormat::Fbx => {
							"FbxLoader"
						}
					};
					return Err(format!("{} load failed: {}", error_label, self.load_handle.get_asset_path()));
				}
			};
			let file_format = self.file_format;
			let model_result = task::with_polling_graphic(|graphic| -> Result<SkinnedModel, String> {
				let gl = graphic.get_tracked_gl();
				match file_format {
					SkinnedModelFileFormat::Glb => {
						SkinnedModel::load_from_bytes_with_shader_program(gl, &bytes, shader_program)
					}
					SkinnedModelFileFormat::Fbx => {
						SkinnedModel::load_from_fbx_bytes_with_shader_program(gl, &bytes, shader_program)
					}
				}
			});
			match model_result {
				Some(model_result) => {
					register_loaded_model(&asset_key, model_result)
				}
				None => {
					Err(String::from("SkinnedModel: 그래픽을 붙인 엔진의 작업 안에서만 모델을 지을 수 있습니다"))
				}
			}
		}
	}
}

impl SkinnedModel {
	//==============================================================================
	/// 생성. (셰이더 프로그램은 렌더러의 템플릿, renderer.get_shader_program())
	///
	/// # 인자
	/// - `shader_program`: 업로드와 그리기에 쓸 셰이더 프로그램. (renderer.get_shader_program())
	///
	/// # 돌려줌
	/// 노드, 드로어블, 클립이 없는 빈 모델. (재생 속도 1)
	//==============================================================================
	pub fn new(shader_program: Rc<ShaderProgram>) -> SkinnedModel {
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		skinned_model.shader_program = Some(shader_program);
		skinned_model
	}

	//==============================================================================
	/// 셰이더 없이 생성. (해석과 포즈 계산만 하는 모델, upload_meshes_and_materials 는 오류)
	///
	/// # 돌려줌
	/// 셰이더 없는 빈 모델. (interpret_content, interpret_fbx_content, update 는 되고 upload_meshes_and_materials 는 Err)
	//==============================================================================
	pub fn create_without_shader_program() -> SkinnedModel {
		SkinnedModel {
			shader_program: None,
			node_list: Vec::new(),
			root_node_indices: Vec::new(),
			skin_list: Vec::new(),
			drawable_list: Vec::new(),
			animation_list: Rc::new(RefCell::new(Vec::new())),
			current_animation_index: None,
			previous_animation_index: None,
			current_time: 0.0,
			previous_time: 0.0,
			fade_duration: 0.0,
			fade_elapsed: 0.0,
			time_scale: 1.0,
			joint_rotation_offsets: Vec::new(),
			pending_animation_load_list: Vec::new(),
			asset: None,
			instance_material_list: Vec::new(),
		}
	}

	//==============================================================================
	/// 바이트로부터 로드. (GLB 바이너리 glTF, 원본 정적 loadFromUrl 의 fetch 뒤 부분)
	///
	/// # 인자
	/// - `gl`: 모델을 지을 GL 컨텍스트.
	/// - `bytes`: GLB 파일 바이트 전체.
	/// - `skinned_model_renderer`: 셰이더 프로그램을 나눠 줄 렌더러.
	///
	/// # 돌려줌
	/// 업로드까지 마친 모델. (클립이 있으면 첫 클립을 재생 중으로 둔다) GLB 가 아니거나 해석, 업로드에 실패하면 Err.
	///
	/// # 예제
	/// GLB 바이트로 모델을 짓고 기본 포즈 계산하기
	/// ```rust,no_run
	/// use vanilla::{SkinnedModel, SkinnedModelRenderer};
	///
	/// fn create_model(gl: &glow::Context, skinned_model_renderer: &SkinnedModelRenderer, glb_bytes: &[u8]) -> Result<SkinnedModel, String> {
	/// 	let mut skinned_model = SkinnedModel::load_from_bytes(gl, glb_bytes, skinned_model_renderer)?;
	/// 	skinned_model.update(0.0);
	/// 	Ok(skinned_model)
	/// }
	/// ```
	//==============================================================================
	pub fn load_from_bytes(gl: &glow::Context, bytes: &[u8], skinned_model_renderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
		SkinnedModel::load_from_bytes_with_shader_program(gl, bytes, skinned_model_renderer.get_shader_program())
	}

	//==============================================================================
	/// 바이트로부터 로드. (바이너리 FBX, 원본 정적 loadFromFbxUrl 의 fetch 뒤 부분)
	///
	/// # 인자
	/// - `gl`: 모델을 지을 GL 컨텍스트.
	/// - `bytes`: 바이너리 FBX 파일 바이트 전체.
	/// - `skinned_model_renderer`: 셰이더 프로그램을 나눠 줄 렌더러.
	///
	/// # 돌려줌
	/// 업로드까지 마친 모델. (스킨이 붙은 메시만 드로어블이 된다) 파싱, 해석, 업로드에 실패하면 Err.
	///
	/// # 예제
	/// 캐릭터 FBX 에 애니메이션 FBX 를 본 이름으로 붙여 틀기
	/// ```rust,no_run
	/// use vanilla::{SkinnedModel, SkinnedModelRenderer};
	///
	/// fn create_character(gl: &glow::Context, skinned_model_renderer: &SkinnedModelRenderer, character_bytes: &[u8], walk_bytes: &[u8]) -> Result<SkinnedModel, String> {
	/// 	let mut skinned_model = SkinnedModel::load_from_fbx_bytes(gl, character_bytes, skinned_model_renderer)?;
	/// 	skinned_model.add_animations_from_fbx_bytes(walk_bytes, "Walk", true)?;
	/// 	skinned_model.play_animation("Walk", 0.0);
	/// 	Ok(skinned_model)
	/// }
	/// ```
	//==============================================================================
	pub fn load_from_fbx_bytes(gl: &glow::Context, bytes: &[u8], skinned_model_renderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
		SkinnedModel::load_from_fbx_bytes_with_shader_program(gl, bytes, skinned_model_renderer.get_shader_program())
	}

	//==============================================================================
	/// 바이트로부터 로드. (GLB, 렌더러 대신 그 셰이더 프로그램을 받는다)
	//==============================================================================
	fn load_from_bytes_with_shader_program(gl: &glow::Context, bytes: &[u8], shader_program: Rc<ShaderProgram>) -> Result<SkinnedModel, String> {
		let mut skinned_model = SkinnedModel::new(shader_program);
		skinned_model.parse_binary(gl, bytes)?;
		Ok(skinned_model)
	}

	//==============================================================================
	/// 바이트로부터 로드. (바이너리 FBX, 렌더러 대신 그 셰이더 프로그램을 받는다)
	//==============================================================================
	fn load_from_fbx_bytes_with_shader_program(gl: &glow::Context, bytes: &[u8], shader_program: Rc<ShaderProgram>) -> Result<SkinnedModel, String> {
		let scene_data = FbxLoader::new().parse(bytes)?;
		let mut skinned_model = SkinnedModel::new(shader_program);
		skinned_model.parse_fbx_content(gl, &scene_data)?;
		Ok(skinned_model)
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 받기 부탁. (GLB, 원본 정적 loadFromUrl, 다 받으면 손잡이의 create_model 이 짓는다)
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `url`: GLB 애셋 경로. (데스크탑은 애셋 폴더 후보 기준 상대 경로, 웹은 asset_base_url 기준 주소)
	///
	/// # 돌려줌
	/// 받기 손잡이. (다 받으면 create_model 로 짓는다)
	//==============================================================================
	pub fn load_from_url(engine: &Engine, url: &str) -> SkinnedModelLoadHandle {
		SkinnedModelLoadHandle {
			load_handle: request_model_bytes(engine, url),
			file_format: SkinnedModelFileFormat::Glb,
		}
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 받기 부탁. (바이너리 FBX, 원본 정적 loadFromFbxUrl)
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `url`: 바이너리 FBX 애셋 경로. (데스크탑은 애셋 폴더 후보 기준 상대 경로, 웹은 asset_base_url 기준 주소)
	///
	/// # 돌려줌
	/// 받기 손잡이. (다 받으면 create_model 로 짓는다)
	//==============================================================================
	pub fn load_from_fbx_url(engine: &Engine, url: &str) -> SkinnedModelLoadHandle {
		SkinnedModelLoadHandle {
			load_handle: request_model_bytes(engine, url),
			file_format: SkinnedModelFileFormat::Fbx,
		}
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 모델 받기. (GLB, 원본 await SkinnedModel.loadFromUrl(gl, url, renderer), 엔진 작업 안에서 await 한다)
	/// - 다 받으면 작업을 폴링하는 엔진의 GL 로 짓는다. 렌더러는 지금 셰이더 프로그램만 나눠 들어, 퓨처가 렌더러를 빌리지 않는다.
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `url`: GLB 애셋 경로.
	/// - `skinned_model_renderer`: 셰이더 프로그램을 나눠 줄 렌더러.
	///
	/// # 돌려줌
	/// 엔진 작업 안에서 await 하는 퓨처. 받기에 실패하면 Err("SkinnedModel load failed: 경로"), 그래픽이 붙은 엔진의 작업 밖에서 폴링하면 Err, 해석이나 업로드에 실패하면 그 Err.
	///
	/// # 예제
	/// 엔진 작업에서 모델을 받아 나눠 든 자리에 넣기
	/// ```rust,no_run
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{Engine, SkinnedModel, SkinnedModelRenderer};
	///
	/// fn start_model_load(engine: &Engine, skinned_model_renderer: &SkinnedModelRenderer, model_slot: Rc<RefCell<Option<SkinnedModel>>>) {
	/// 	let model_future = SkinnedModel::load_from_url_async(engine, "fox.glb", skinned_model_renderer);
	/// 	engine.spawn_task(move |_engine_context| async move {
	/// 		match model_future.await {
	/// 			Ok(skinned_model) => {
	/// 				*model_slot.borrow_mut() = Some(skinned_model);
	/// 			}
	/// 			Err(error_message) => {
	/// 				eprintln!("{}", error_message);
	/// 			}
	/// 		}
	/// 	});
	/// }
	/// ```
	//==============================================================================
	pub fn load_from_url_async(engine: &Engine, url: &str, skinned_model_renderer: &SkinnedModelRenderer) -> impl Future<Output = Result<SkinnedModel, String>> + 'static {
		SkinnedModel::load_cached_async(engine, url, skinned_model_renderer, SkinnedModelFileFormat::Glb)
	}

	//==============================================================================
	/// 자산 캐시를 거쳐 받기. (캐시에 있으면 받지 않고 인스턴스, 받는 중이면 그 받기를 나눠 들고, 없으면 받아 지어 캐시에 넣는다)
	//==============================================================================
	fn load_cached_async(engine: &Engine, url: &str, skinned_model_renderer: &SkinnedModelRenderer, file_format: SkinnedModelFileFormat) -> impl Future<Output = Result<SkinnedModel, String>> + 'static {
		let shader_program = skinned_model_renderer.get_shader_program();
		let cached_asset = SkinnedModelAsset::find(url, &shader_program);
		let load_handle = match cached_asset {
			Some(_) => {
				None
			}
			None => {
				Some(SkinnedModelLoadHandle {
					load_handle: request_model_bytes(engine, url),
					file_format,
				})
			}
		};
		async move {
			match (cached_asset, load_handle) {
				(Some(cached_asset), _) => {
					SkinnedModel::create_from_asset(&cached_asset)
				}
				(None, Some(load_handle)) => {
					load_handle.create_model_when_loaded(shader_program).await
				}
				(None, None) => {
					Err(String::from("SkinnedModel: no load handle."))
				}
			}
		}
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 모델 받기. (바이너리 FBX, 원본 await SkinnedModel.loadFromFbxUrl(gl, url, renderer))
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `url`: 바이너리 FBX 애셋 경로.
	/// - `skinned_model_renderer`: 셰이더 프로그램을 나눠 줄 렌더러.
	///
	/// # 돌려줌
	/// 엔진 작업 안에서 await 하는 퓨처. 받기에 실패하면 Err("FbxLoader load failed: 경로"), 그래픽이 붙은 엔진의 작업 밖에서 폴링하면 Err, 파싱, 해석, 업로드에 실패하면 그 Err.
	//==============================================================================
	pub fn load_from_fbx_url_async(engine: &Engine, url: &str, skinned_model_renderer: &SkinnedModelRenderer) -> impl Future<Output = Result<SkinnedModel, String>> + 'static {
		SkinnedModel::load_cached_async(engine, url, skinned_model_renderer, SkinnedModelFileFormat::Fbx)
	}

	//==============================================================================
	/// GLB 청크 읽기. (머리 확인 + JSON 청크 + BIN 청크, GL 무관)
	///
	/// # 인자
	/// - `bytes`: GLB 파일 바이트 전체.
	///
	/// # 돌려줌
	/// (glTF JSON, BIN 청크 바이트). 머리가 glTF 가 아니거나, 청크가 파일 밖을 가리키거나, JSON 이 깨졌거나, JSON 과 BIN 청크 가운데 하나라도 없으면 Err.
	///
	/// # 예제
	/// 손으로 감싼 GLB 에서 JSON 과 BIN 꺼내기
	/// ```rust
	/// use vanilla::SkinnedModel;
	///
	/// let json_bytes = br#"{"asset":{"version":"2.0"}}"#;
	/// let binary_bytes = [1u8, 2, 3, 4];
	/// let mut glb_bytes = Vec::new();
	/// glb_bytes.extend_from_slice(b"glTF");
	/// glb_bytes.extend_from_slice(&2u32.to_le_bytes());
	/// glb_bytes.extend_from_slice(&((12 + 8 + json_bytes.len() + 8 + binary_bytes.len()) as u32).to_le_bytes());
	/// glb_bytes.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
	/// glb_bytes.extend_from_slice(b"JSON");
	/// glb_bytes.extend_from_slice(json_bytes);
	/// glb_bytes.extend_from_slice(&(binary_bytes.len() as u32).to_le_bytes());
	/// glb_bytes.extend_from_slice(b"BIN\0");
	/// glb_bytes.extend_from_slice(&binary_bytes);
	/// let (json, binary_buffer) = SkinnedModel::read_glb_chunks(&glb_bytes).unwrap();
	/// assert_eq!(json["asset"]["version"], "2.0");
	/// assert_eq!(binary_buffer, vec![1, 2, 3, 4]);
	/// assert!(SkinnedModel::read_glb_chunks(b"not a glb").is_err());
	/// ```
	//==============================================================================
	pub fn read_glb_chunks(bytes: &[u8]) -> Result<(Value, Vec<u8>), String> {
		if read_uint32(bytes, 0) != Some(GLB_MAGIC) {
			return Err(String::from("SkinnedModel: not a GLB file."));
		}

		// 청크 순회. (JSON + BIN)
		let mut json: Option<Value> = None;
		let mut binary_buffer: Option<Vec<u8>> = None;
		let mut chunk_offset = 12usize;
		while chunk_offset < bytes.len() {
			let chunk_length = read_uint32(bytes, chunk_offset).ok_or_else(|| String::from("SkinnedModel: invalid GLB chunks."))? as usize;
			let chunk_type = read_uint32(bytes, chunk_offset + 4).ok_or_else(|| String::from("SkinnedModel: invalid GLB chunks."))?;
			let chunk_start = chunk_offset + 8;
			let chunk_end = chunk_start.checked_add(chunk_length).filter(|chunk_end| *chunk_end <= bytes.len()).ok_or_else(|| String::from("SkinnedModel: invalid GLB chunks."))?;
			if chunk_type == GLB_CHUNKTYPE_JSON {
				let json_text = String::from_utf8_lossy(&bytes[chunk_start..chunk_end]);
				let parsed_json: Value = serde_json::from_str(&json_text).map_err(|error| format!("SkinnedModel: invalid glTF JSON: {}", error))?;
				json = Some(parsed_json);
			}
			else if chunk_type == GLB_CHUNKTYPE_BINARY {
				binary_buffer = Some(bytes[chunk_start..chunk_end].to_vec());
			}
			chunk_offset = chunk_end;
		}
		match (json, binary_buffer) {
			(Some(json), Some(binary_buffer)) => {
				Ok((json, binary_buffer))
			}
			_ => {
				Err(String::from("SkinnedModel: invalid GLB chunks."))
			}
		}
	}

	//==============================================================================
	/// GLB 바이너리 해석. (청크 읽기 → 내용 해석 → GL 업로드)
	///
	/// # 인자
	/// - `gl`: 업로드할 GL 컨텍스트.
	/// - `bytes`: GLB 파일 바이트 전체.
	///
	/// # 돌려줌
	/// 다 되면 Ok. 청크 읽기, 해석, 업로드의 Err.
	//==============================================================================
	pub fn parse_binary(&mut self, gl: &glow::Context, bytes: &[u8]) -> Result<(), String> {
		let (json, binary_buffer) = SkinnedModel::read_glb_chunks(bytes)?;
		self.parse_content(gl, &json, &binary_buffer)
	}

	//==============================================================================
	/// glTF 내용 해석. (노드 / 스킨 / 메시 / 애니메이션 / 텍스처 → GL 업로드)
	///
	/// # 인자
	/// - `gl`: 업로드할 GL 컨텍스트.
	/// - `json`: glTF JSON 전체.
	/// - `binary_buffer`: BIN 청크 바이트.
	///
	/// # 돌려줌
	/// 다 되면 Ok. 해석이나 업로드의 Err.
	//==============================================================================
	pub fn parse_content(&mut self, gl: &glow::Context, json: &Value, binary_buffer: &[u8]) -> Result<(), String> {
		let (mesh_description_list, material_description_list) = self.interpret_content(json, binary_buffer)?;
		self.upload_meshes_and_materials(gl, mesh_description_list, material_description_list)
	}

	//==============================================================================
	/// glTF 내용 해석. (GL 무관, 노드 / 스킨 / 애니메이션을 채우고 중립 메시 / 머티리얼 서술을 돌려준다)
	/// - 모델의 노드, 루트, 스킨, 클립을 이 내용으로 바꾼다. 클립이 있으면 첫 클립을 재생 중으로 둔다.
	///
	/// # 인자
	/// - `json`: glTF JSON 전체.
	/// - `binary_buffer`: BIN 청크 바이트.
	///
	/// # 돌려줌
	/// (메시 서술 목록, 머티리얼 서술 목록). 노드나 씬이 없거나, 없는 메시를 가리키거나, 프리미티브에 POSITION 이 없거나, 스킨 메시에 JOINTS_0 이나 WEIGHTS_0 이 없거나, 접근자를 읽을 수 없으면 Err.
	//==============================================================================
	pub fn interpret_content(&mut self, json: &Value, binary_buffer: &[u8]) -> Result<(Vec<MeshDescription>, Vec<MaterialDescription>), String> {
		// 노드 구성.
		let node_value_list = json["nodes"].as_array().ok_or_else(|| String::from("SkinnedModel: glTF has no nodes."))?;
		let mut node_list = Vec::with_capacity(node_value_list.len());
		for node_value in node_value_list.iter() {
			let rotation = match read_float_array(&node_value["rotation"]) {
				Some(rotation_values) if rotation_values.len() >= 4 => {
					Quaternion::create(rotation_values[0], rotation_values[1], rotation_values[2], rotation_values[3])
				}
				_ => {
					Quaternion::identity()
				}
			};
			node_list.push(SkinnedModelNode {
				name: node_value["name"].as_str().unwrap_or("").to_string(),
				child_indices: read_index_array(&node_value["children"]),
				mesh_index: read_index(&node_value["mesh"]),
				skin_index: read_index(&node_value["skin"]),
				base_translation: read_vector3(&node_value["translation"], [0.0, 0.0, 0.0]),
				base_rotation: rotation,
				base_scale: read_vector3(&node_value["scale"], [1.0, 1.0, 1.0]),
				current_translation: [0.0, 0.0, 0.0],
				current_rotation: Quaternion::identity(),
				current_scale: [1.0, 1.0, 1.0],
				world_matrix: Matrix4::create_identity(),
			});
		}
		self.node_list = node_list;
		let scene_index = read_index(&json["scene"]).unwrap_or(0);
		let scene = json["scenes"].get(scene_index).ok_or_else(|| String::from("SkinnedModel: glTF scene not found."))?;
		self.root_node_indices = read_index_array(&scene["nodes"]);

		// 스킨 구성.
		let mut skin_list = Vec::new();
		if let Some(skin_value_list) = json["skins"].as_array() {
			for skin_value in skin_value_list.iter() {
				let joint_node_indices = read_index_array(&skin_value["joints"]);
				let inverse_bind_array = match read_index(&skin_value["inverseBindMatrices"]) {
					Some(accessor_index) => {
						read_accessor_array(json, binary_buffer, accessor_index)?.to_float_list()
					}
					None => {
						Vec::new()
					}
				};
				let mut inverse_bind_matrices = Vec::with_capacity(joint_node_indices.len());
				for joint_index in 0..joint_node_indices.len() {
					let mut matrix = Matrix4::create_identity();
					if let Some(matrix_values) = inverse_bind_array.get(joint_index * 16..joint_index * 16 + 16) {
						matrix.elements = std::array::from_fn(|element_index| matrix_values[element_index] as f32);
					}
					inverse_bind_matrices.push(matrix);
				}
				skin_list.push(Skin {
					joint_matrix_array: vec![0.0; joint_node_indices.len() * 16],
					joint_node_indices,
					inverse_bind_matrices,
				});
			}
		}
		self.skin_list = skin_list;

		// 중립 머티리얼 서술 구성. (glTF metallic-roughness → 그대로 매핑)
		let mut material_description_list = Vec::new();
		if let Some(material_value_list) = json["materials"].as_array() {
			for material_value in material_value_list.iter() {
				let mut description = Material::create_default_description();
				let pbr_metallic_roughness = &material_value["pbrMetallicRoughness"];
				if pbr_metallic_roughness.is_object() {
					if let Some(base_color_factor) = read_float_array(&pbr_metallic_roughness["baseColorFactor"]) {
						if base_color_factor.len() >= 3 {
							description.base_color_factor = [base_color_factor[0], base_color_factor[1], base_color_factor[2]];
						}
					}
					description.metallic_factor = pbr_metallic_roughness["metallicFactor"].as_f64().map(|value| value).unwrap_or(1.0);
					description.roughness_factor = pbr_metallic_roughness["roughnessFactor"].as_f64().map(|value| value).unwrap_or(1.0);
					description.base_color_image = create_gltf_image_description(json, binary_buffer, &pbr_metallic_roughness["baseColorTexture"]);
					description.metallic_roughness_image = create_gltf_image_description(json, binary_buffer, &pbr_metallic_roughness["metallicRoughnessTexture"]);
				}
				description.normal_image = create_gltf_image_description(json, binary_buffer, &material_value["normalTexture"]);
				description.occlusion_image = create_gltf_image_description(json, binary_buffer, &material_value["occlusionTexture"]);
				description.emissive_image = create_gltf_image_description(json, binary_buffer, &material_value["emissiveTexture"]);
				if let Some(emissive_factor) = read_float_array(&material_value["emissiveFactor"]) {
					if emissive_factor.len() >= 3 {
						description.emissive_factor = [emissive_factor[0], emissive_factor[1], emissive_factor[2]];
					}
				}
				material_description_list.push(description);
			}
		}

		// 중립 메시 서술 구성. (메시 노드의 프리미티브, 접근자를 CPU 배열로 펼침)
		// - 스킨 없는 정적 메시는 노드 자신을 유일한 조인트로 갖는 합성 스킨을 붙여 같은 스키닝 경로로 그린다.
		let mut mesh_description_list = Vec::new();
		for node_index in 0..self.node_list.len() {
			let mesh_index = match self.node_list[node_index].mesh_index {
				Some(mesh_index) => {
					mesh_index
				}
				None => {
					continue;
				}
			};
			let mesh_value = json["meshes"].get(mesh_index).ok_or_else(|| format!("SkinnedModel: mesh {} not found.", mesh_index))?;
			let is_static_mesh = self.node_list[node_index].skin_index.is_none();
			if is_static_mesh {
				self.skin_list.push(Skin {
					joint_node_indices: vec![node_index],
					inverse_bind_matrices: vec![Matrix4::create_identity()],
					joint_matrix_array: vec![0.0; 16],
				});
				self.node_list[node_index].skin_index = Some(self.skin_list.len() - 1);
			}
			let skin_index = self.node_list[node_index].skin_index.unwrap_or(0);
			let target_name_list: Vec<String> = match mesh_value["extras"]["targetNames"].as_array() {
				Some(target_name_values) => {
					target_name_values.iter().map(|target_name_value| target_name_value.as_str().unwrap_or("").to_string()).collect()
				}
				None => {
					Vec::new()
				}
			};
			// 기본 모프 가중치. (glTF 노드의 weights 가 있으면 그것, 없으면 메시 weights)
			let base_morph_weights = read_float_array(&json["nodes"][node_index]["weights"]).or_else(|| read_float_array(&mesh_value["weights"])).unwrap_or_default();
			let primitive_value_list = mesh_value["primitives"].as_array().map(|primitive_values| primitive_values.as_slice()).unwrap_or(&[]);
			for primitive_value in primitive_value_list.iter() {
				let attributes = &primitive_value["attributes"];
				let position_accessor_index = read_index(&attributes["POSITION"]).ok_or_else(|| String::from("SkinnedModel: primitive has no POSITION."))?;
				let positions = read_accessor_array(json, binary_buffer, position_accessor_index)?.to_float_list();
				let vertex_count = positions.len() / 3;
				let normals = match read_index(&attributes["NORMAL"]) {
					Some(accessor_index) => {
						Some(read_accessor_array(json, binary_buffer, accessor_index)?.to_float_list())
					}
					None => {
						None
					}
				};
				let texture_coordinates = match read_index(&attributes["TEXCOORD_0"]) {
					Some(accessor_index) => {
						Some(read_accessor_array(json, binary_buffer, accessor_index)?.to_float_list())
					}
					None => {
						None
					}
				};
				let joints;
				let mut weights;
				if is_static_mesh {
					joints = vec![0u16; vertex_count * 4];
					weights = vec![0.0f64; vertex_count * 4];
					for vertex_index in 0..vertex_count {
						weights[vertex_index * 4] = 1.0;
					}
				}
				else {
					let joint_accessor_index = read_index(&attributes["JOINTS_0"]).ok_or_else(|| String::from("SkinnedModel: skinned primitive has no JOINTS_0."))?;
					let weight_accessor_index = read_index(&attributes["WEIGHTS_0"]).ok_or_else(|| String::from("SkinnedModel: skinned primitive has no WEIGHTS_0."))?;
					joints = read_accessor_array(json, binary_buffer, joint_accessor_index)?.to_unsigned_short_list();
					let weight_source = read_accessor_array(json, binary_buffer, weight_accessor_index)?;
					weights = weight_source.to_float_list();
					let component_type = weight_source.get_component_type();
					if component_type != COMPONENT_TYPE_FLOAT {
						let normalize_divisor = if component_type == COMPONENT_TYPE_UNSIGNED_BYTE {
							255.0
						}
						else {
							65535.0
						};
						for weight in weights.iter_mut() {
							*weight /= normalize_divisor;
						}
					}
				}
				let indices = match read_index(&primitive_value["indices"]) {
					Some(accessor_index) => {
						Some(read_accessor_array(json, binary_buffer, accessor_index)?.to_index_array())
					}
					None => {
						None
					}
				};

				// 모프 타깃. (위치 델타 필수, 노멀 델타 선택, 이름은 mesh.extras.targetNames)
				let mut morph_targets = Vec::new();
				if let Some(target_value_list) = primitive_value["targets"].as_array() {
					for (target_index, target_value) in target_value_list.iter().enumerate() {
						let position_accessor_index = match read_index(&target_value["POSITION"]) {
							Some(position_accessor_index) => {
								position_accessor_index
							}
							None => {
								continue;
							}
						};
						let position_deltas = read_accessor_array(json, binary_buffer, position_accessor_index)?.to_float_list();
						let normal_deltas = match read_index(&target_value["NORMAL"]) {
							Some(accessor_index) => {
								Some(read_accessor_array(json, binary_buffer, accessor_index)?.to_float_list())
							}
							None => {
								None
							}
						};
						let target_name = match target_name_list.get(target_index) {
							Some(target_name) => {
								target_name.clone()
							}
							None => {
								format!("target{}", target_index)
							}
						};
						morph_targets.push(MorphTarget { name: target_name, position_deltas, normal_deltas });
					}
				}
				let mut morph_weights = vec![0.0f32; morph_targets.len()];
				for (weight_index, morph_weight) in morph_weights.iter_mut().enumerate() {
					*morph_weight = (base_morph_weights.get(weight_index).copied().unwrap_or(0.0)) as f32;
				}

				let material_index = read_index(&primitive_value["material"]);
				let material_name = match material_index {
					Some(material_index) => {
						json["materials"][material_index]["name"].as_str().unwrap_or("").to_string()
					}
					None => {
						String::new()
					}
				};
				mesh_description_list.push(MeshDescription {
					positions,
					normals,
					texture_coordinates,
					joints,
					weights,
					indices,
					skin_index,
					material_index,
					material_name,
					node_index: Some(node_index),
					morph_targets,
					morph_weights,
				});
			}
		}

		// 애니메이션 구성.
		let mut animation_list = Vec::new();
		if let Some(animation_value_list) = json["animations"].as_array() {
			for animation_value in animation_value_list.iter() {
				let mut duration = 0.0f64;
				let mut channels = Vec::new();
				let channel_value_list = animation_value["channels"].as_array().map(|channel_values| channel_values.as_slice()).unwrap_or(&[]);
				for channel_value in channel_value_list.iter() {
					let sampler_index = read_index(&channel_value["sampler"]).ok_or_else(|| String::from("SkinnedModel: animation channel has no sampler."))?;
					let sampler_value = &animation_value["samplers"][sampler_index];
					let input_accessor_index = read_index(&sampler_value["input"]).ok_or_else(|| String::from("SkinnedModel: animation sampler has no input."))?;
					let output_accessor_index = read_index(&sampler_value["output"]).ok_or_else(|| String::from("SkinnedModel: animation sampler has no output."))?;
					let times = read_accessor_array(json, binary_buffer, input_accessor_index)?.to_float_list();
					let values = read_accessor_array(json, binary_buffer, output_accessor_index)?.to_float_list();
					if let Some(last_time) = times.last() {
						duration = duration.max(*last_time);
					}
					let node_index = read_index(&channel_value["target"]["node"]);
					let path = channel_value["target"]["path"].as_str().and_then(AnimationPath::from_name);
					if let (Some(node_index), Some(path)) = (node_index, path) {
						channels.push(AnimationChannel { node_index, path, times, values, cursor: 0 });
					}
				}
				animation_list.push(AnimationClip {
					name: animation_value["name"].as_str().unwrap_or("").to_string(),
					duration,
					channels,
				});
			}
		}
		let has_animation = !animation_list.is_empty();
		self.replace_animation_list(animation_list);
		if has_animation {
			self.current_animation_index = Some(0);
		}
		Ok((mesh_description_list, material_description_list))
	}

	//==============================================================================
	/// FBX 중간 표현 해석. (노드/스킨/메시/텍스처/애니메이션 → 기존 스키닝 파이프라인)
	///
	/// # 인자
	/// - `gl`: 업로드할 GL 컨텍스트.
	/// - `scene_data`: FbxLoader 가 낸 중간 표현.
	///
	/// # 돌려줌
	/// 다 되면 Ok. 업로드의 Err.
	//==============================================================================
	pub fn parse_fbx_content(&mut self, gl: &glow::Context, scene_data: &FbxSceneData) -> Result<(), String> {
		let (mesh_description_list, material_description_list) = self.interpret_fbx_content(scene_data)?;
		self.upload_meshes_and_materials(gl, mesh_description_list, material_description_list)
	}

	//==============================================================================
	/// FBX 중간 표현 해석. (GL 무관, 노드 / 스킨 / 애니메이션을 채우고 중립 메시 / 머티리얼 서술을 돌려준다)
	/// - 모델의 노드, 루트, 스킨, 클립을 이 내용으로 바꾼다. 클립이 있으면 첫 클립을 재생 중으로 둔다.
	///
	/// # 인자
	/// - `scene_data`: FbxLoader 가 낸 중간 표현.
	///
	/// # 돌려줌
	/// (메시 서술 목록, 머티리얼 서술 목록). (지오메트리가 이어진 스킨 디포머마다, 그 지오메트리가 쓰는 머티리얼마다 메시 하나, 지금은 Err 를 내지 않는다)
	//==============================================================================
	pub fn interpret_fbx_content(&mut self, scene_data: &FbxSceneData) -> Result<(Vec<MeshDescription>, Vec<MaterialDescription>), String> {
		// 노드 구성. (오일러 도 → 쿼터니언, 사전회전 우측 누적)
		self.node_list = scene_data.node_list.iter().map(|fbx_node| {
			let rotation_quaternion = create_quaternion_from_fbx_euler(fbx_node.rotation_euler_degrees[0], fbx_node.rotation_euler_degrees[1], fbx_node.rotation_euler_degrees[2]);
			let pre_rotation_quaternion = create_quaternion_from_fbx_euler(fbx_node.pre_rotation_degrees[0], fbx_node.pre_rotation_degrees[1], fbx_node.pre_rotation_degrees[2]);
			let base_rotation = pre_rotation_quaternion.multiply(rotation_quaternion);
			SkinnedModelNode {
				name: fbx_node.name.clone(),
				child_indices: fbx_node.child_indices.clone(),
				mesh_index: None,
				skin_index: None,
				base_translation: [fbx_node.translation[0], fbx_node.translation[1], fbx_node.translation[2]],
				base_rotation,
				base_scale: [fbx_node.scale[0], fbx_node.scale[1], fbx_node.scale[2]],
				current_translation: [0.0, 0.0, 0.0],
				current_rotation: Quaternion::identity(),
				current_scale: [1.0, 1.0, 1.0],
				world_matrix: Matrix4::create_identity(),
			}
		}).collect();
		self.root_node_indices = scene_data.root_node_indices.clone();

		// 스킨 구성. (클러스터 → 조인트 노드 인덱스 + 링크 변환의 역행렬)
		let skin_id_list = scene_data.skin_deformer_by_id.keys().to_vec();
		let mut skin_list = Vec::with_capacity(skin_id_list.len());
		for skin_id in skin_id_list.iter() {
			let skin_deformer = match scene_data.skin_deformer_by_id.get(*skin_id) {
				Some(skin_deformer) => {
					skin_deformer
				}
				None => {
					continue;
				}
			};
			let mut joint_node_indices = Vec::with_capacity(skin_deformer.cluster_ids.len());
			let mut inverse_bind_matrices = Vec::with_capacity(skin_deformer.cluster_ids.len());
			for cluster_id in skin_deformer.cluster_ids.iter() {
				let cluster = scene_data.cluster_by_id.get(*cluster_id);
				let bone_node_index = cluster.and_then(|cluster| cluster.bone_node_id).and_then(|bone_node_id| scene_data.node_id_to_index.get(&bone_node_id).copied());
				joint_node_indices.push(bone_node_index.unwrap_or(usize::MAX));
				// 클러스터 Transform = TransformLink^-1 x 메시 바인드 전역행렬, 그 자체가 역바인드 행렬.
				// 미보유 시 TransformLink 의 역행렬로 대체. (메시 전역 = 단위행렬 가정)
				let mut inverse_bind_matrix = Matrix4::create_identity();
				if let Some(cluster) = cluster {
					if let Some(transform) = cluster.transform.as_ref().filter(|transform| transform.len() >= 16) {
						for (element_index, element) in inverse_bind_matrix.elements.iter_mut().enumerate() {
							*element = transform[element_index] as f32;
						}
					}
					else if let Some(transform_link) = cluster.transform_link.as_ref().filter(|transform_link| transform_link.len() >= 16) {
						let mut link_matrix = Matrix4::create_identity();
						for (element_index, element) in link_matrix.elements.iter_mut().enumerate() {
							*element = transform_link[element_index] as f32;
						}
						inverse_bind_matrix = Matrix4::create_inverse(&link_matrix);
					}
				}
				inverse_bind_matrices.push(inverse_bind_matrix);
			}
			skin_list.push(Skin {
				joint_matrix_array: vec![0.0; joint_node_indices.len() * 16],
				joint_node_indices,
				inverse_bind_matrices,
			});
		}
		self.skin_list = skin_list;

		// 이미지 서술 구성 헬퍼. (텍스처 id → 임베디드 이미지 바이트, FBX 는 상하 반전 필요)
		let create_fbx_image_description = |texture_id: Option<i64>| -> Option<ImageDescription> {
			let texture = scene_data.texture_by_id.get(texture_id?)?;
			let video = scene_data.video_by_id.get(texture.video_id?)?;
			let content_bytes = video.content_bytes.as_ref().filter(|content_bytes| !content_bytes.is_empty())?;
			Some(ImageDescription {
				bytes: content_bytes.clone(),
				mime_type: None,
				source: None,
				flip_y: true,
				is_color_data: false,
				anisotropy: 0.0,
			})
		};

		// 중립 머티리얼 서술 구성. (FBX phong → 비금속 + 스펙큘러/글로스 근사 경로)
		let mut material_description_list = Vec::with_capacity(scene_data.material_by_id.len());
		for fbx_material in scene_data.material_by_id.values().iter() {
			let mut description = Material::create_default_description();
			description.base_color_factor = [fbx_material.diffuse_color[0], fbx_material.diffuse_color[1], fbx_material.diffuse_color[2]];
			description.base_color_image = create_fbx_image_description(fbx_material.diffuse_texture_id);
			description.normal_image = create_fbx_image_description(fbx_material.normal_texture_id);
			description.specular_image = create_fbx_image_description(fbx_material.specular_texture_id);
			description.glossiness_image = create_fbx_image_description(fbx_material.gloss_texture_id);
			material_description_list.push(description);
		}

		// 스킨드 메시 → 중립 메시 서술 구성.
		let mut mesh_description_list = Vec::new();
		for (skin_list_index, skin_id) in skin_id_list.iter().enumerate() {
			let skin_deformer = match scene_data.skin_deformer_by_id.get(*skin_id) {
				Some(skin_deformer) => {
					skin_deformer
				}
				None => {
					continue;
				}
			};
			let geometry = match skin_deformer.geometry_id.and_then(|geometry_id| scene_data.geometry_by_id.get(geometry_id)) {
				Some(geometry) => {
					geometry
				}
				None => {
					continue;
				}
			};

			// 정점별 스킨 영향 누적. (클러스터 로컬 조인트 인덱스 + 가중치)
			let original_vertex_count = geometry.original_vertex_count;
			let mut influences_per_vertex: Vec<Vec<(u16, f64)>> = vec![Vec::new(); original_vertex_count];
			for (cluster_local_index, cluster_id) in skin_deformer.cluster_ids.iter().enumerate() {
				let cluster = match scene_data.cluster_by_id.get(*cluster_id) {
					Some(cluster) => {
						cluster
					}
					None => {
						continue;
					}
				};
				for (influence_index, vertex_index) in cluster.vertex_indices.iter().enumerate() {
					let weight = cluster.weights.get(influence_index).copied().unwrap_or(0.0);
					if *vertex_index >= 0 && (*vertex_index as usize) < original_vertex_count {
						influences_per_vertex[*vertex_index as usize].push((cluster_local_index as u16, weight));
					}
				}
			}

			// 정점별 상위 4개 선택 + 정규화.
			let mut vertex_joint_array = vec![0u16; original_vertex_count * 4];
			let mut vertex_weight_array = vec![0.0f64; original_vertex_count * 4];
			for (vertex_index, influence_list) in influences_per_vertex.iter_mut().enumerate() {
				influence_list.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(std::cmp::Ordering::Equal));
				let used_count = influence_list.len().min(4);
				let mut weight_sum: f64 = influence_list.iter().take(used_count).map(|influence| influence.1).sum();
				if weight_sum <= 0.0 {
					weight_sum = 1.0;
				}
				for slot_index in 0..used_count {
					vertex_joint_array[vertex_index * 4 + slot_index] = influence_list[slot_index].0;
					vertex_weight_array[vertex_index * 4 + slot_index] = influence_list[slot_index].1 / weight_sum;
				}
			}

			// 코너 단위(비인덱스)로 조인트/가중치 펼침.
			let corner_count = geometry.corner_count;
			let mut corner_joint_array = vec![0u16; corner_count * 4];
			let mut corner_weight_array = vec![0.0f64; corner_count * 4];
			for corner_index in 0..corner_count {
				let original_vertex_index = geometry.original_vertex_indices[corner_index];
				if original_vertex_index < 0 || original_vertex_index as usize >= original_vertex_count {
					continue;
				}
				let source_offset = original_vertex_index as usize * 4;
				corner_joint_array[corner_index * 4..corner_index * 4 + 4].copy_from_slice(&vertex_joint_array[source_offset..source_offset + 4]);
				corner_weight_array[corner_index * 4..corner_index * 4 + 4].copy_from_slice(&vertex_weight_array[source_offset..source_offset + 4]);
			}

			// 머티리얼별 부분 메시. (삼각형의 머티리얼 번호는 소유 노드 material_ids 안의 자리, 번호마다 드로어블 하나)
			let owner_node_index = scene_data.node_list.iter().position(|fbx_node| fbx_node.geometry_id.is_some() && fbx_node.geometry_id == skin_deformer.geometry_id);
			let owner_material_ids: &[i64] = match owner_node_index {
				Some(owner_node_index) => {
					&scene_data.node_list[owner_node_index].material_ids
				}
				None => {
					&[]
				}
			};
			let sub_mesh_list = SkinnedModel::split_fbx_corners_by_material(&geometry.material_index_per_triangle, FbxCornerArrays {
				positions: geometry.positions.clone(),
				normals: geometry.normals.clone(),
				texture_coordinates: geometry.texture_coordinates.clone(),
				joints: corner_joint_array,
				weights: corner_weight_array,
			});
			for (local_material_index, corner_arrays) in sub_mesh_list {
				let material_id = owner_material_ids.get(local_material_index).copied();
				let material_index = material_id.and_then(|material_id| scene_data.material_by_id.index_of(material_id));
				let material_name = match material_id.and_then(|material_id| scene_data.material_by_id.get(material_id)) {
					Some(fbx_material) => {
						fbx_material.name.clone()
					}
					None => {
						String::new()
					}
				};
				mesh_description_list.push(MeshDescription {
					positions: corner_arrays.positions,
					normals: Some(corner_arrays.normals),
					texture_coordinates: Some(corner_arrays.texture_coordinates),
					joints: corner_arrays.joints,
					weights: corner_arrays.weights,
					indices: None,
					skin_index: skin_list_index,
					material_index,
					material_name,
					node_index: owner_node_index,
					morph_targets: Vec::new(),
					morph_weights: Vec::new(),
				});
			}
		}

		// 애니메이션 구성. (회전 채널은 오일러 도 → 사전회전 합성 쿼터니언 4성분으로 변환)
		let animation_list: Vec<AnimationClip> = scene_data.animation_list.iter().map(|fbx_animation| {
			let channels = convert_fbx_animation_channels(scene_data, fbx_animation, Some);
			AnimationClip {
				name: fbx_animation.name.clone(),
				duration: fbx_animation.duration,
				channels,
			}
		}).collect();
		let has_animation = !animation_list.is_empty();
		self.replace_animation_list(animation_list);
		if has_animation {
			self.current_animation_index = Some(0);
		}
		Ok((mesh_description_list, material_description_list))
	}

	//==============================================================================
	/// 애니메이션 FBX 추가. (본 이름 매칭 리타게팅, Mixamo 애니메이션 전용 FBX 의 중간 표현)
	/// - 동일 스켈레톤을 전제로, 채널을 현재 모델의 노드 인덱스로 재지정해 클립을 추가한다.
	/// - ignore_translation: 루트 모션이 박힌 클립에서 이동 채널을 버린다. (게임 로직이 위치를 제어할 때)
	/// - clip_name: 채널이 있는 스택이 하나면 이 이름, 여럿이면 스택마다 "이 이름/스택 이름" 이다.
	///
	/// # 인자
	/// - `scene_data`: 애니메이션 FBX 의 중간 표현.
	/// - `clip_name`: 더할 클립 이름. (스택이 여럿이면 모두 이 이름이 된다)
	/// - `ignore_translation`: true 면 이동 채널을 버린다.
	//==============================================================================
	pub fn add_animations_from_fbx_scene_data(&mut self, scene_data: &FbxSceneData, clip_name: &str, ignore_translation: bool) {
		let mut added_clip_list = convert_fbx_scene_animations(&self.create_node_index_by_name(), scene_data, clip_name, ignore_translation);
		merge_animation_clips(&mut self.animation_list.borrow_mut(), std::mem::take(&mut added_clip_list));
	}

	//==============================================================================
	/// 클립 목록 바꾸기. (해석할 때, 자산과 나눠 쓰던 목록이면 새 목록으로 떨어지고 아니면 그 자리에서 바꾼다)
	/// - 자산을 쓰는 다른 인스턴스의 클립은 그대로다. 자산 없이 받는 중인 애니메이션 FBX 는 바뀐 목록에 더한다.
	//==============================================================================
	fn replace_animation_list(&mut self, animation_list: Vec<AnimationClip>) {
		if self.asset.is_some() {
			self.animation_list = Rc::new(RefCell::new(animation_list));
		}
		else {
			*self.animation_list.borrow_mut() = animation_list;
		}
	}

	//==============================================================================
	/// 노드 이름 → 인덱스 표. (애니메이션 FBX 의 노드를 이름으로 맞출 때)
	//==============================================================================
	fn create_node_index_by_name(&self) -> HashMap<String, usize> {
		let mut node_index_by_name: HashMap<String, usize> = HashMap::new();
		for (node_index, node) in self.node_list.iter().enumerate() {
			node_index_by_name.insert(node.name.clone(), node_index);
		}
		node_index_by_name
	}

	//==============================================================================
	/// 애니메이션 FBX 추가. (바이트, 원본 addAnimationsFromFbxUrl 의 fetch 뒤 부분)
	///
	/// # 인자
	/// - `bytes`: 애니메이션 FBX 파일 바이트 전체.
	/// - `clip_name`: 더할 클립 이름.
	/// - `ignore_translation`: true 면 이동 채널을 버린다.
	///
	/// # 돌려줌
	/// 더했으면 Ok. 파싱에 실패하면 클립을 더하지 않고 Err.
	///
	/// # 예제
	/// FBX 가 아닌 바이트는 클립 없이 Err
	/// ```rust
	/// use vanilla::SkinnedModel;
	///
	/// let mut skinned_model = SkinnedModel::create_without_shader_program();
	/// assert!(skinned_model.add_animations_from_fbx_bytes(b"not an fbx", "Walk", true).is_err());
	/// assert!(skinned_model.get_animation_name_list().is_empty());
	/// ```
	//==============================================================================
	pub fn add_animations_from_fbx_bytes(&mut self, bytes: &[u8], clip_name: &str, ignore_translation: bool) -> Result<(), String> {
		let scene_data = FbxLoader::new().parse(bytes)?;
		self.add_animations_from_fbx_scene_data(&scene_data, clip_name, ignore_translation);
		Ok(())
	}

	//==============================================================================
	/// 애니메이션 FBX 추가. (애셋 경로, 원본 addAnimationsFromFbxUrl, await 할 수 있는 손잡이를 돌려준다)
	/// - await 하면 다 받아 클립을 더할 때까지 기다린다. (원본처럼 await 가 끝날 때는 클립이 이미 더해져 있다)
	///   await 하지 않아도 다 받은 뒤 다음 update 나 play_animation 에서 더한다. 받은 차례로 더한다. (원본 async 함수가 끝나는 차례)
	/// - 받기나 파싱에 실패하면 클립을 더하지 않고 오류를 찍고, await 는 까닭으로 끝난다. (원본은 Promise 가 거부된다)
	/// - 노드 이름은 부르는 때의 노드로 맞춘다. 모델을 지은 뒤에 부른다.
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `url`: 애니메이션 FBX 애셋 경로.
	/// - `clip_name`: 더할 클립 이름.
	/// - `ignore_translation`: true 면 이동 채널을 버린다.
	///
	/// # 돌려줌
	/// await 할 수 있는 받기 손잡이. (버려도 모델이 들고 있어 클립은 더해진다)
	//==============================================================================
	pub fn add_animations_from_fbx_url(&mut self, engine: &Engine, url: &str, clip_name: &str, ignore_translation: bool) -> AnimationLoadHandle {
		let animation_load_handle = AnimationLoadHandle {
			animation_load: Rc::new(AnimationLoad {
				fbx_load_handle: FbxLoader::load_from_url(engine, url),
				clip_name: String::from(clip_name),
				ignore_translation,
				node_index_by_name: self.create_node_index_by_name(),
				animation_list: self.animation_list.clone(),
				apply_result: RefCell::new(None),
			}),
		};
		self.pending_animation_load_list.push(animation_load_handle.clone());
		animation_load_handle
	}

	//==============================================================================
	/// 다 받은 애니메이션 FBX 의 클립 더하기. (await 하는 퓨처가 이미 더한 것은 빼기만 한다)
	//==============================================================================
	fn apply_finished_animation_loads(&mut self) {
		self.pending_animation_load_list.retain(|animation_load_handle| animation_load_handle.animation_load.apply().is_none());
	}

	//==============================================================================
	/// 중립 메시/머티리얼 서술 업로드. (공용, Material 생성 + 버텍스 어레이/드로어블 구성)
	/// - 예전 자산은 먼저 놓는다. (destroy, 다시 불러도 새지 않는다)
	/// - 올린 버퍼, 머티리얼, 모프 텍스처와 지금 노드, 스킨, 클립을 새 자산(SkinnedModelAsset, 참조 수 1)으로 묶고 이 모델이 그 첫 인스턴스가 된다.
	///
	/// # 인자
	/// - `gl`: 업로드할 GL 컨텍스트.
	/// - `mesh_description_list`: 메시 서술 목록.
	/// - `material_description_list`: 머티리얼 서술 목록. (material_index 가 가리키는 자리)
	///
	/// # 돌려줌
	/// 다 되면 Ok. 셰이더 없이 만든 모델이거나, 머티리얼이나 GL 객체를 지을 수 없으면 Err.
	//==============================================================================
	pub fn upload_meshes_and_materials(&mut self, gl: &glow::Context, mesh_description_list: Vec<MeshDescription>, material_description_list: Vec<MaterialDescription>) -> Result<(), String> {
		let shader_program = match &self.shader_program {
			Some(shader_program) => {
				shader_program.clone()
			}
			None => {
				return Err(String::from("SkinnedModel: no shader program to upload with."));
			}
		};
		self.destroy(gl);
		let mut material_list = Vec::with_capacity(material_description_list.len());
		for material_description in material_description_list.iter() {
			let material = Material::create_from_description(gl, shader_program.clone(), material_description)?;
			material_list.push(Rc::new(material));
		}
		let default_description = Material::create_default_description();
		let default_material = Rc::new(Material::create_from_description(gl, shader_program.clone(), &default_description)?);

		let mut buffer_byte_count = 0;
		let mut template_drawable_list = Vec::with_capacity(mesh_description_list.len());
		for mut mesh_description in mesh_description_list.into_iter() {
			let generated_normals;
			let normals = match &mesh_description.normals {
				Some(normals) => {
					normals
				}
				None => {
					generated_normals = SkinnedModel::generate_smooth_normals(&mesh_description.positions, mesh_description.indices.as_ref());
					&generated_normals
				}
			};
			let vertex_array = unsafe {
				crate::core::glstatecache::create_vertex_array(gl)?
			};
			let mut vertex_buffer_list = Vec::new();
			unsafe {
				crate::core::glstatecache::bind_vertex_array(gl, Some(vertex_array));
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexPosition", &mesh_description.positions, 3)? {
				vertex_buffer_list.push(vertex_buffer);
				buffer_byte_count += mesh_description.positions.len() * std::mem::size_of::<f32>();
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexNormal", normals, 3)? {
				vertex_buffer_list.push(vertex_buffer);
				buffer_byte_count += normals.len() * std::mem::size_of::<f32>();
			}
			if let Some(texture_coordinates) = &mesh_description.texture_coordinates {
				if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexTextureCoordinate", texture_coordinates, 2)? {
					vertex_buffer_list.push(vertex_buffer);
					buffer_byte_count += texture_coordinates.len() * std::mem::size_of::<f32>();
				}
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_integer_attribute(gl, &shader_program, "vertexJoints", &mesh_description.joints, 4)? {
				vertex_buffer_list.push(vertex_buffer);
				buffer_byte_count += std::mem::size_of_val(mesh_description.joints.as_slice());
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexWeights", &mesh_description.weights, 4)? {
				vertex_buffer_list.push(vertex_buffer);
				buffer_byte_count += mesh_description.weights.len() * std::mem::size_of::<f32>();
			}

			// 인덱스. (없는 메시는 drawArrays 로 출력)
			let mut index_count = 0;
			let mut index_component_type = 0;
			let mut index_buffer = None;
			let is_indexed = mesh_description.indices.is_some();
			if let Some(indices) = &mesh_description.indices {
				unsafe {
					let created_index_buffer = gl.create_buffer()?;
					gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(created_index_buffer));
					gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices.as_bytes(), glow::STATIC_DRAW);
					index_buffer = Some(created_index_buffer);
				}
				index_count = indices.len();
				index_component_type = indices.get_gl_component_type();
				buffer_byte_count += indices.as_bytes().len();
			}
			unsafe {
				crate::core::glstatecache::bind_vertex_array(gl, None);
			}

			let material = match mesh_description.material_index.and_then(|material_index| material_list.get(material_index)) {
				Some(material) => {
					material.clone()
				}
				None => {
					default_material.clone()
				}
			};
			let morph_target_list = std::mem::take(&mut mesh_description.morph_targets);
			let morph_weights = mesh_description.morph_weights.clone();
			let is_morph_dirty = !morph_target_list.is_empty();
			template_drawable_list.push(Drawable {
				vertex_array,
				vertex_buffer_list,
				is_indexed,
				index_buffer,
				index_count,
				index_component_type,
				index_byte_offset: 0,
				wireframe_index_buffer: None,
				wireframe_index_count: 0,
				wireframe_index_component_type: 0,
				vertex_count: mesh_description.positions.len() / 3,
				material,
				material_name: mesh_description.material_name.clone(),
				skin_index: mesh_description.skin_index,
				node_index: mesh_description.node_index,
				mesh_description: Rc::new(mesh_description),
				morph_target_list: Rc::new(morph_target_list),
				morph_weights,
				morph_texture: None,
				morph_rows_per_target: 0,
				morph_texture_width: 0,
				is_morph_texture_shared: false,
				is_morph_dirty,
			});
		}

		// 모프 텍스처는 원형에 한 번 올려 인스턴스들이 나눠 쓴다.
		for template_drawable in template_drawable_list.iter_mut() {
			if template_drawable.is_morph_dirty {
				upload_drawable_morph_targets(gl, template_drawable)?;
				if template_drawable.morph_texture.is_some() {
					buffer_byte_count += (template_drawable.morph_texture_width as usize) * (template_drawable.morph_rows_per_target as usize) * 2 * template_drawable.morph_target_list.len() * 16;
				}
			}
		}

		// 자산으로 묶고 첫 인스턴스로 붙는다.
		let mut asset_material_list = material_list;
		asset_material_list.push(default_material);
		let skinned_model_asset = Rc::new(SkinnedModelAsset::new(shader_program, SkinnedModelAssetContent {
			drawable_list: template_drawable_list,
			material_list: asset_material_list,
			node_list: self.node_list.clone(),
			root_node_indices: self.root_node_indices.clone(),
			skin_list: self.skin_list.iter().map(create_instance_skin).collect(),
			animation_list: self.animation_list.clone(),
			buffer_byte_count,
		}));
		self.attach_asset(skinned_model_asset);
		Ok(())
	}

	//==============================================================================
	/// 자산 붙이기. (자산의 원형에서 노드, 스킨, 드로어블을 베끼고 클립 목록은 나눠 쓴다, 참조 수는 부른 쪽이 늘린다)
	//==============================================================================
	fn attach_asset(&mut self, skinned_model_asset: Rc<SkinnedModelAsset>) {
		self.node_list = skinned_model_asset.get_node_list().to_vec();
		self.root_node_indices = skinned_model_asset.get_root_node_indices().to_vec();
		self.skin_list = skinned_model_asset.get_skin_list().iter().map(create_instance_skin).collect();
		self.drawable_list = skinned_model_asset.get_drawable_list().iter().map(create_instance_drawable).collect();
		self.animation_list = skinned_model_asset.get_animation_list().clone();
		// 해석할 때처럼 첫 클립을 돌린다.
		self.current_animation_index = if self.animation_list.borrow().is_empty() {
			None
		}
		else {
			Some(0)
		};
		self.previous_animation_index = None;
		self.asset = Some(skinned_model_asset);
	}

	//==============================================================================
	/// 자산으로부터 인스턴스 짓기. (자산의 버퍼, 머티리얼 원형, 클립을 나눠 쓰고 포즈와 재생 상태는 따로, GPU 업로드 없음)
	///
	/// # 인자
	/// - `skinned_model_asset`: 나눠 쓸 자산.
	///
	/// # 돌려줌
	/// 자산의 참조 수를 하나 늘린 새 인스턴스. (노드는 자산을 올릴 때의 원형에서 베낀다) 지워진 자산이면 Err.
	//==============================================================================
	pub fn create_from_asset(skinned_model_asset: &Rc<SkinnedModelAsset>) -> Result<SkinnedModel, String> {
		skinned_model_asset.retain()?;
		let mut skinned_model = SkinnedModel::new(skinned_model_asset.get_shader_program().clone());
		skinned_model.attach_asset(skinned_model_asset.clone());
		Ok(skinned_model)
	}

	//==============================================================================
	/// 인스턴스 짓기. (이 모델과 같은 자산을 나눠 쓰는 새 모델, 유니티의 Instantiate 처럼 GPU 자원을 다시 올리지 않는다)
	///
	/// # 돌려줌
	/// 새 인스턴스. (노드는 자산을 올릴 때의 원형에서 베끼고 재생 상태는 처음부터) 올리기 전이면 Err.
	///
	/// # 예제
	/// 여우 하나를 받아 열 마리 띄우기
	/// ```rust,no_run
	/// use vanilla::SkinnedModel;
	///
	/// fn spawn_foxes(source_model: &SkinnedModel) -> Result<Vec<SkinnedModel>, String> {
	/// 	let mut model_list = Vec::new();
	/// 	for model_index in 1..10 {
	/// 		let mut instance_model = source_model.create_instance()?;
	/// 		instance_model.play_animation(if model_index % 2 == 0 { "Walk" } else { "Run" }, 0.0);
	/// 		model_list.push(instance_model);
	/// 	}
	/// 	Ok(model_list)
	/// }
	///
	/// fn destroy_foxes(gl: &glow::Context, source_model: &mut SkinnedModel, model_list: &mut Vec<SkinnedModel>) {
	/// 	let skinned_model_asset = source_model.get_asset().cloned();
	/// 	for model in model_list.iter_mut() {
	/// 		model.destroy(gl);
	/// 	}
	/// 	source_model.destroy(gl);
	/// 	assert!(skinned_model_asset.is_some_and(|skinned_model_asset| skinned_model_asset.is_destroyed()));
	/// }
	/// ```
	//==============================================================================
	pub fn create_instance(&self) -> Result<SkinnedModel, String> {
		match &self.asset {
			Some(skinned_model_asset) => {
				SkinnedModel::create_from_asset(skinned_model_asset)
			}
			None => {
				Err(String::from("SkinnedModel: no asset to instance (upload first)."))
			}
		}
	}

	//==============================================================================
	/// 자산 반환.
	///
	/// # 돌려줌
	/// 나눠 쓰는 자산. (올리기 전이나 destroy 뒤에는 None)
	//==============================================================================
	pub fn get_asset(&self) -> Option<&Rc<SkinnedModelAsset>> {
		self.asset.as_ref()
	}

	//==============================================================================
	/// 와이어프레임 인덱스 업로드. (삼각형 인덱스 → 중복 없는 모서리 선 인덱스, 비인덱스 메시는 삼각형별 3변)
	/// - 드로어블의 버텍스 어레이와 무관한 별도 엘리먼트 버퍼. 렌더러가 선 출력 시 잠시 바꿔 끼운다.
	/// - 이미 올린 선 버퍼가 있으면 지우고 새로 올린다.
	///
	/// # 인자
	/// - `gl`: 업로드할 GL 컨텍스트.
	/// - `drawable_index`: 드로어블 자리.
	///
	/// # 돌려줌
	/// 올렸으면 Ok. 드로어블이 없거나 버퍼를 지을 수 없으면 Err.
	//==============================================================================
	pub fn upload_wireframe_indices(&mut self, gl: &glow::Context, drawable_index: usize) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		if let Some(wireframe_index_buffer) = drawable.wireframe_index_buffer.take() {
			unsafe {
				gl.delete_buffer(wireframe_index_buffer);
			}
		}
		let edge_list = SkinnedModel::build_wireframe_edge_list(drawable.mesh_description.indices.as_ref(), drawable.vertex_count);
		let use_unsigned_int = drawable.vertex_count > 65535;
		let wireframe_indices = if use_unsigned_int {
			IndexArray::UnsignedInt(edge_list.iter().map(|index| *index as u32).collect())
		}
		else {
			IndexArray::UnsignedShort(edge_list.iter().map(|index| *index as u16).collect())
		};
		unsafe {
			let wireframe_index_buffer = gl.create_buffer()?;
			crate::core::glstatecache::bind_vertex_array(gl, None);
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(wireframe_index_buffer));
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, wireframe_indices.as_bytes(), glow::STATIC_DRAW);
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
			drawable.wireframe_index_buffer = Some(wireframe_index_buffer);
		}
		drawable.wireframe_index_count = wireframe_indices.len();
		drawable.wireframe_index_component_type = wireframe_indices.get_gl_component_type();
		Ok(())
	}

	//==============================================================================
	/// FBX 코너를 머티리얼별로 나누기. (GL 무관, 비인덱스 코너 배열)
	/// - 메시 하나에 머티리얼이 여럿이면 머티리얼마다 부분 메시로 나눠 그린다. (three.js FBXLoader 의 그룹, 유니티와 언리얼의 서브메시)
	/// - 부분 메시는 머티리얼 번호가 작은 것부터, 삼각형 차례는 원래대로. 번호가 하나뿐이면 배열을 그대로 쓴다.
	///
	/// # 인자
	/// - `material_index_per_triangle`: 삼각형마다 머티리얼 번호. (소유 노드 material_ids 안의 자리, 음수는 0)
	/// - `corner_arrays`: 코너마다 위치 3, 노멀 3, UV 2, 조인트 4, 가중치 4.
	///
	/// # 돌려줌
	/// (머티리얼 번호, 그 번호의 코너 배열) 목록.
	///
	/// # 예제
	/// 삼각형 셋 가운데 가운데 것만 재질 1 (부분 메시 둘, 삼각형 2개와 1개)
	/// ```rust
	/// use vanilla::SkinnedModel;
	/// use vanilla::graphics3d::skinnedmodel::FbxCornerArrays;
	///
	/// let corner_count = 9;
	/// let corner_arrays = FbxCornerArrays {
	/// 	positions: (0..corner_count * 3).map(|index| index as f64).collect(),
	/// 	normals: vec![0.0; corner_count * 3],
	/// 	texture_coordinates: vec![0.0; corner_count * 2],
	/// 	joints: vec![0; corner_count * 4],
	/// 	weights: vec![0.0; corner_count * 4],
	/// };
	/// let sub_mesh_list = SkinnedModel::split_fbx_corners_by_material(&[0, 1, 0], corner_arrays);
	/// assert_eq!(sub_mesh_list.len(), 2);
	/// assert_eq!(sub_mesh_list[0].0, 0);
	/// assert_eq!(sub_mesh_list[0].1.positions.len() / 9, 2);
	/// assert_eq!(sub_mesh_list[1].1.positions[0], 9.0);
	/// ```
	//==============================================================================
	pub fn split_fbx_corners_by_material(material_index_per_triangle: &[i32], corner_arrays: FbxCornerArrays) -> Vec<(usize, FbxCornerArrays)> {
		let mut triangle_count_by_material: Vec<(usize, usize)> = Vec::new();
		for &material_index in material_index_per_triangle.iter() {
			let local_material_index = material_index.max(0) as usize;
			match triangle_count_by_material.iter_mut().find(|(index, _count)| *index == local_material_index) {
				Some((_index, count)) => {
					*count += 1;
				}
				None => {
					triangle_count_by_material.push((local_material_index, 1));
				}
			}
		}
		triangle_count_by_material.sort_by_key(|(index, _count)| *index);
		if triangle_count_by_material.len() <= 1 {
			let local_material_index = triangle_count_by_material.first().map(|(index, _count)| *index).unwrap_or(0);
			return vec![(local_material_index, corner_arrays)];
		}
		//==============================================================================
		/// 원천 배열에서 주어진 재질의 삼각형 조각만 모아 새 배열로 만든다. (삼각형마다 성분 수 x 3 개를 가져온다)
		//==============================================================================
		fn gather<T: Copy>(source_array: &[T], component_count: usize, material_index_per_triangle: &[i32], local_material_index: usize, triangle_count: usize) -> Vec<T> {
			let triangle_stride = component_count * 3;
			let mut target_array = Vec::with_capacity(triangle_count * triangle_stride);
			for (triangle_index, &material_index) in material_index_per_triangle.iter().enumerate() {
				if material_index.max(0) as usize != local_material_index {
					continue;
				}
				target_array.extend_from_slice(&source_array[triangle_index * triangle_stride..(triangle_index + 1) * triangle_stride]);
			}
			target_array
		}
		triangle_count_by_material.iter().map(|&(local_material_index, sub_triangle_count)| {
			(local_material_index, FbxCornerArrays {
				positions: gather(&corner_arrays.positions, 3, material_index_per_triangle, local_material_index, sub_triangle_count),
				normals: gather(&corner_arrays.normals, 3, material_index_per_triangle, local_material_index, sub_triangle_count),
				texture_coordinates: gather(&corner_arrays.texture_coordinates, 2, material_index_per_triangle, local_material_index, sub_triangle_count),
				joints: gather(&corner_arrays.joints, 4, material_index_per_triangle, local_material_index, sub_triangle_count),
				weights: gather(&corner_arrays.weights, 4, material_index_per_triangle, local_material_index, sub_triangle_count),
			})
		}).collect()
	}

	//==============================================================================
	/// 와이어프레임 모서리 목록 만들기. (GL 무관, 인덱스 쌍 목록)
	///
	/// # 인자
	/// - `triangle_indices`: 삼각형 인덱스. (None 이면 정점 3개씩 삼각형)
	/// - `vertex_count`: 정점 개수.
	///
	/// # 돌려줌
	/// 선 인덱스 쌍 목록. (작은 인덱스, 큰 인덱스 차례, 겹친 모서리는 한 번만)
	///
	/// # 예제
	/// 사각형(삼각형 두 개)의 모서리 5개
	/// ```rust
	/// use vanilla::SkinnedModel;
	/// use vanilla::graphics3d::skinnedmodel::IndexArray;
	///
	/// let quad_indices = IndexArray::UnsignedShort(vec![0, 1, 2, 2, 1, 3]);
	/// let edge_list = SkinnedModel::build_wireframe_edge_list(Some(&quad_indices), 4);
	/// assert_eq!(edge_list, vec![0, 1, 1, 2, 0, 2, 1, 3, 2, 3]);
	/// assert_eq!(SkinnedModel::build_wireframe_edge_list(None, 3), vec![0, 1, 1, 2, 2, 0]);
	/// ```
	//==============================================================================
	pub fn build_wireframe_edge_list(triangle_indices: Option<&IndexArray>, vertex_count: usize) -> Vec<usize> {
		let mut edge_list = Vec::new();
		match triangle_indices {
			Some(triangle_indices) => {
				let mut edge_key_set: HashSet<usize> = HashSet::new();
				let triangle_count = triangle_indices.len() / 3;
				for triangle_index in 0..triangle_count {
					for corner_index in 0..3 {
						let index_a = triangle_indices.get(triangle_index * 3 + corner_index);
						let index_b = triangle_indices.get(triangle_index * 3 + (corner_index + 1) % 3);
						let minimum_index = index_a.min(index_b);
						let maximum_index = index_a.max(index_b);
						let edge_key = minimum_index * vertex_count + maximum_index;
						if edge_key_set.contains(&edge_key) {
							continue;
						}
						edge_key_set.insert(edge_key);
						edge_list.push(minimum_index);
						edge_list.push(maximum_index);
					}
				}
			}
			None => {
				let mut vertex_index = 0;
				while vertex_index + 2 < vertex_count {
					edge_list.extend_from_slice(&[vertex_index, vertex_index + 1, vertex_index + 1, vertex_index + 2, vertex_index + 2, vertex_index]);
					vertex_index += 3;
				}
			}
		}
		edge_list
	}

	//==============================================================================
	/// 모프 타깃 업로드. (드로어블의 델타 목록 → RGBA32F 텍스처, 타깃마다 위치 블록 + 노멀 블록)
	/// - 텍셀 인덱스 = 정점 인덱스. 블록 시작 행 = 타깃 인덱스 x 2 x 타깃당 행 수.
	///
	/// # 인자
	/// - `gl`: 업로드할 GL 컨텍스트.
	/// - `drawable_index`: 드로어블 자리.
	///
	/// # 돌려줌
	/// 올렸으면 Ok. (타깃이 없으면 텍스처를 지우고 Ok) 드로어블이 없거나 텍스처를 지을 수 없으면 Err.
	//==============================================================================
	pub fn upload_morph_targets(&mut self, gl: &glow::Context, drawable_index: usize) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		upload_drawable_morph_targets(gl, drawable)
	}

	//==============================================================================
	/// 모프 텍셀 데이터 만들기. (GL 무관, 텍스처 가로, 타깃당 행 수, RGBA 실수 배열)
	///
	/// # 인자
	/// - `morph_target_list`: 모프 타깃 목록.
	/// - `vertex_count`: 정점 개수.
	///
	/// # 돌려줌
	/// (텍스처 가로, 타깃당 행 수, RGBA 실수 배열). (가로는 정점 수를 1 ~ 2048 로 자른 값, 높이는 행 수 x 2 x 타깃 수, 알파는 0)
	//==============================================================================
	pub fn build_morph_texel_data(morph_target_list: &[MorphTarget], vertex_count: usize) -> (usize, usize, Vec<f32>) {
		let target_count = morph_target_list.len();
		let texture_width = vertex_count.clamp(1, MORPH_TEXTURE_WIDTH);
		let rows_per_target = vertex_count.div_ceil(texture_width);
		let texture_height = rows_per_target * 2 * target_count;
		let mut texel_data = vec![0.0f32; texture_width * texture_height * 4];
		for (target_index, morph_target) in morph_target_list.iter().enumerate() {
			let position_block_offset = target_index * 2 * rows_per_target * texture_width;
			let normal_block_offset = position_block_offset + rows_per_target * texture_width;
			for vertex_index in 0..vertex_count {
				let position_texel_offset = (position_block_offset + vertex_index) * 4;
				for component_index in 0..3 {
					texel_data[position_texel_offset + component_index] = (morph_target.position_deltas.get(vertex_index * 3 + component_index).copied().unwrap_or(0.0)) as f32;
				}
				if let Some(normal_deltas) = &morph_target.normal_deltas {
					let normal_texel_offset = (normal_block_offset + vertex_index) * 4;
					for component_index in 0..3 {
						texel_data[normal_texel_offset + component_index] = (normal_deltas.get(vertex_index * 3 + component_index).copied().unwrap_or(0.0)) as f32;
					}
				}
			}
		}
		(texture_width, rows_per_target, texel_data)
	}

	//==============================================================================
	/// 바뀐 모프 타깃 업로드. (원본 update 6단계, 추가/변경된 드로어블만, 렌더러가 그리기 전에 부른다)
	/// - 올리다 실패한 드로어블은 오류를 찍고 다시 올리지 않는다.
	///
	/// # 인자
	/// - `gl`: 업로드할 GL 컨텍스트.
	//==============================================================================
	pub fn upload_pending_morph_targets(&mut self, gl: &glow::Context) {
		for drawable_index in 0..self.drawable_list.len() {
			if self.drawable_list[drawable_index].is_morph_dirty {
				if let Err(error) = self.upload_morph_targets(gl, drawable_index) {
					eprintln!("SkinnedModel: {}", error);
					self.drawable_list[drawable_index].is_morph_dirty = false;
				}
			}
		}
	}

	//==============================================================================
	/// 모프 타깃 추가. (프로그램 생성, 정점별 위치 델타 [+ 노멀 델타], 다음 그리기 전에 업로드)
	///
	/// # 인자
	/// - `drawable_index`: 드로어블 자리.
	/// - `target_name`: 타깃 이름. (set_morph_weight 가 이 이름으로 찾는다)
	/// - `position_deltas`: 정점마다 위치 델타. (정점 수 x 3 개여야 한다)
	/// - `normal_deltas`: 정점마다 노멀 델타. (None 이면 노멀은 그대로)
	///
	/// # 돌려줌
	/// 더했으면 Ok. (가중치 0 으로 시작) 드로어블이 없거나, 타깃이 이미 MORPH_TARGET_MAXIMUM 개이거나, 델타 개수가 맞지 않으면 Err.
	///
	/// # 예제
	/// 첫 드로어블의 정점을 모두 위로 올리는 타깃을 더하고 반만 걸기 (드로어블이 없으면 Err)
	/// ```rust
	/// use vanilla::SkinnedModel;
	///
	/// fn add_lift_target(skinned_model: &mut SkinnedModel) -> Result<(), String> {
	/// 	let vertex_count = skinned_model.get_drawable_list().first().map(|drawable| drawable.vertex_count).unwrap_or(0);
	/// 	let mut position_deltas = vec![0.0; vertex_count * 3];
	/// 	for vertex_index in 0..vertex_count {
	/// 		position_deltas[vertex_index * 3 + 1] = 0.1;
	/// 	}
	/// 	skinned_model.add_morph_target(0, "Lift", position_deltas, None)?;
	/// 	skinned_model.set_morph_weight("Lift", 0.5);
	/// 	Ok(())
	/// }
	///
	/// let mut skinned_model = SkinnedModel::create_without_shader_program();
	/// assert!(add_lift_target(&mut skinned_model).is_err());
	/// ```
	//==============================================================================
	pub fn add_morph_target(&mut self, drawable_index: usize, target_name: &str, position_deltas: Vec<f64>, normal_deltas: Option<Vec<f64>>) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		if drawable.morph_target_list.len() >= MORPH_TARGET_MAXIMUM {
			return Err(format!("SkinnedModel: morph target limit {} exceeded.", MORPH_TARGET_MAXIMUM));
		}
		if position_deltas.len() != drawable.vertex_count * 3 {
			return Err(format!("SkinnedModel: morph target \"{}\" delta count mismatch.", target_name));
		}
		// 타깃 목록은 자산과 나눠 쓰므로 갈라진 뒤 더한다. (이 인스턴스만 갈라진다)
		Rc::make_mut(&mut drawable.morph_target_list).push(MorphTarget { name: target_name.to_string(), position_deltas, normal_deltas });
		drawable.morph_weights.resize(drawable.morph_target_list.len(), 0.0);
		drawable.is_morph_dirty = true;
		Ok(())
	}

	//==============================================================================
	/// 모프 가중치 설정. (이름 일치 타깃 전부, 드로어블 여러 개가 같은 이름을 가질 수 있음)
	///
	/// # 인자
	/// - `target_name`: 타깃 이름. (이름이 같은 타깃 모두)
	/// - `weight`: 가중치. (0 = 없음, 1 = 델타 그대로, 자르지 않는다)
	//==============================================================================
	pub fn set_morph_weight(&mut self, target_name: &str, weight: f64) {
		for drawable in self.drawable_list.iter_mut() {
			for target_index in 0..drawable.morph_target_list.len() {
				if drawable.morph_target_list[target_index].name == target_name {
					drawable.morph_weights[target_index] = weight as f32;
				}
			}
		}
	}

	//==============================================================================
	/// 모프 가중치 반환. (첫 일치 타깃, 없으면 0)
	///
	/// # 인자
	/// - `target_name`: 타깃 이름.
	///
	/// # 돌려줌
	/// 처음 맞는 타깃의 가중치. 없으면 0.
	//==============================================================================
	pub fn get_morph_weight(&self, target_name: &str) -> f64 {
		for drawable in self.drawable_list.iter() {
			for target_index in 0..drawable.morph_target_list.len() {
				if drawable.morph_target_list[target_index].name == target_name {
					return (drawable.morph_weights[target_index]) as f64;
				}
			}
		}
		0.0
	}

	//==============================================================================
	/// 머티리얼 교체. (드로어블 단위, 외부 텍스처로 만든 머티리얼 인스턴스 적용)
	///
	/// # 인자
	/// - `drawable_index`: 드로어블 자리.
	/// - `material`: 바꿔 낄 머티리얼.
	///
	/// # 돌려줌
	/// 바꿨으면 Ok. 드로어블이 없으면 Err.
	//==============================================================================
	pub fn set_material(&mut self, drawable_index: usize, material: Rc<Material>) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		drawable.material = material;
		Ok(())
	}

	//==============================================================================
	/// 이 인스턴스의 머티리얼 반환. (유니티 Renderer.material, 처음 부르면 지금 머티리얼의 사본으로 갈라져 이 드로어블에 낀다)
	/// - 사본은 텍스처를 빌려 쓰고 값만 따로다. 값을 바꿔도 같은 자산을 쓰는 다른 인스턴스는 그대로다. 사본은 이 모델의 destroy 가 지운다.
	/// - 이미 이 인스턴스의 사본이면 그것을 돌려준다.
	///
	/// # 인자
	/// - `drawable_index`: 드로어블 자리.
	///
	/// # 돌려줌
	/// 이 인스턴스만 쓰는 머티리얼. 드로어블이 없으면 Err.
	///
	/// # 예제
	/// 여러 마리 가운데 한 마리만 붉게
	/// ```rust,no_run
	/// use vanilla::SkinnedModel;
	///
	/// fn tint_red(blue_fox: &SkinnedModel) -> Result<SkinnedModel, String> {
	/// 	let mut red_fox = blue_fox.create_instance()?;
	/// 	red_fox.get_material(0)?.set_base_color_factor([1.0, 0.3, 0.3]);
	/// 	assert_ne!(blue_fox.get_shared_material(0)?.get_base_color_factor(), red_fox.get_shared_material(0)?.get_base_color_factor());
	/// 	Ok(red_fox)
	/// }
	/// ```
	//==============================================================================
	pub fn get_material(&mut self, drawable_index: usize) -> Result<Rc<Material>, String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		let is_instance_material = self.instance_material_list.iter().any(|instance_material| Rc::ptr_eq(instance_material, &drawable.material));
		if !is_instance_material {
			let instance_material = Rc::new(drawable.material.create_instance());
			self.instance_material_list.push(instance_material.clone());
			drawable.material = instance_material;
		}
		Ok(drawable.material.clone())
	}

	//==============================================================================
	/// 낀 머티리얼 반환. (유니티 Renderer.sharedMaterial, 갈라지지 않는다. 자산의 원형이면 값을 바꿀 때 그 자산을 쓰는 모든 인스턴스가 바뀐다)
	///
	/// # 인자
	/// - `drawable_index`: 드로어블 자리.
	///
	/// # 돌려줌
	/// 지금 드로어블에 낀 머티리얼. (get_material 로 갈라졌으면 그 사본, set_material 로 바꿨으면 그것) 드로어블이 없으면 Err.
	//==============================================================================
	pub fn get_shared_material(&self, drawable_index: usize) -> Result<Rc<Material>, String> {
		let drawable = self.drawable_list.get(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		Ok(drawable.material.clone())
	}

	//==============================================================================
	/// 스무스 노멀 생성. (노멀 미보유 메시, 바인드 포즈 기준, 스키닝 회전은 셰이더에서 적용)
	///
	/// # 인자
	/// - `positions`: 정점 위치. (3개씩)
	/// - `indices`: 삼각형 인덱스. (None 이면 정점 3개씩 삼각형)
	///
	/// # 돌려줌
	/// 정점마다 단위 노멀. (positions 와 같은 길이, 면 노멀을 넓이 가중으로 더해 정규화한다. 삼각형에 쓰이지 않은 정점은 0)
	///
	/// # 예제
	/// xy 평면 위 반시계 삼각형은 +Z
	/// ```rust
	/// use vanilla::SkinnedModel;
	///
	/// let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
	/// let normals = SkinnedModel::generate_smooth_normals(&positions, None);
	/// assert_eq!(normals, vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
	/// ```
	//==============================================================================
	pub fn generate_smooth_normals(positions: &[f64], indices: Option<&IndexArray>) -> Vec<f64> {
		let mut normals = vec![0.0f64; positions.len()];
		let triangle_count = match indices {
			Some(indices) => {
				indices.len() / 3
			}
			None => {
				positions.len() / 9
			}
		};
		for triangle_index in 0..triangle_count {
			let read_corner = |corner_index: usize| -> usize {
				match indices {
					Some(indices) => {
						indices.get(triangle_index * 3 + corner_index) * 3
					}
					None => {
						(triangle_index * 3 + corner_index) * 3
					}
				}
			};
			let index_a = read_corner(0);
			let index_b = read_corner(1);
			let index_c = read_corner(2);
			if index_a + 2 >= positions.len() || index_b + 2 >= positions.len() || index_c + 2 >= positions.len() {
				continue;
			}
			let edge_abx = positions[index_b] - positions[index_a];
			let edge_aby = positions[index_b + 1] - positions[index_a + 1];
			let edge_abz = positions[index_b + 2] - positions[index_a + 2];
			let edge_acx = positions[index_c] - positions[index_a];
			let edge_acy = positions[index_c + 1] - positions[index_a + 1];
			let edge_acz = positions[index_c + 2] - positions[index_a + 2];
			let face_normal_x = edge_aby * edge_acz - edge_abz * edge_acy;
			let face_normal_y = edge_abz * edge_acx - edge_abx * edge_acz;
			let face_normal_z = edge_abx * edge_acy - edge_aby * edge_acx;
			for corner_offset in [index_a, index_b, index_c] {
				normals[corner_offset] += face_normal_x;
				normals[corner_offset + 1] += face_normal_y;
				normals[corner_offset + 2] += face_normal_z;
			}
		}
		for normal in normals.as_chunks_mut::<3>().0 {
			let normal_length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
			if normal_length > 0.0 {
				normal[0] /= normal_length;
				normal[1] /= normal_length;
				normal[2] /= normal_length;
			}
		}
		normals
	}

	//==============================================================================
	/// 실수형 어트리뷰트 바인드. (신규 버퍼 생성 + 포인터 설정, 셰이더가 안 쓰는 어트리뷰트면 None)
	/// - 지금 묶인 버텍스 어레이에 붙는다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shader_program`: 어트리뷰트 위치를 찾을 셰이더.
	/// - `attribute_name`: 어트리뷰트 이름. (vertexPosition 같은 것)
	/// - `data_array`: 정점 자료.
	/// - `component_count`: 정점당 성분 수. (1 ~ 4)
	///
	/// # 돌려줌
	/// 새로 지은 정점 버퍼. 셰이더가 그 어트리뷰트를 쓰지 않으면 Ok(None), 버퍼를 지을 수 없으면 Err.
	//==============================================================================
	pub fn bind_float_attribute(gl: &glow::Context, shader_program: &ShaderProgram, attribute_name: &str, data_array: &[f64], component_count: i32) -> Result<Option<GlBuffer>, String> {
		let attribute_location = match shader_program.get_attribute_location(gl, attribute_name) {
			Some(attribute_location) => {
				attribute_location
			}
			None => {
				return Ok(None);
			}
		};
		unsafe {
			let vertex_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
			// 셈은 f64 로 들고 GPU 에는 f32 로 올린다. (원본 Float32Array)
			let float32_array: Vec<f32> = data_array.iter().map(|value| *value as f32).collect();
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, slice_to_bytes(&float32_array), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(attribute_location);
			gl.vertex_attrib_pointer_f32(attribute_location, component_count, glow::FLOAT, false, 0, 0);
			Ok(Some(vertex_buffer))
		}
	}

	//==============================================================================
	/// 정수형 어트리뷰트 바인드. (조인트 인덱스, 셰이더가 안 쓰는 어트리뷰트면 None)
	/// - 지금 묶인 버텍스 어레이에 붙는다. 셰이더에서는 정수 어트리뷰트(uvec4 같은)로 읽는다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `shader_program`: 어트리뷰트 위치를 찾을 셰이더.
	/// - `attribute_name`: 어트리뷰트 이름. (vertexJoints 같은 것)
	/// - `data_array`: u16 정점 자료.
	/// - `component_count`: 정점당 성분 수. (1 ~ 4)
	///
	/// # 돌려줌
	/// 새로 지은 정점 버퍼. 셰이더가 그 어트리뷰트를 쓰지 않으면 Ok(None), 버퍼를 지을 수 없으면 Err.
	//==============================================================================
	pub fn bind_integer_attribute(gl: &glow::Context, shader_program: &ShaderProgram, attribute_name: &str, data_array: &[u16], component_count: i32) -> Result<Option<GlBuffer>, String> {
		let attribute_location = match shader_program.get_attribute_location(gl, attribute_name) {
			Some(attribute_location) => {
				attribute_location
			}
			None => {
				return Ok(None);
			}
		};
		unsafe {
			let vertex_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
			// u16 그대로 올린다. (원본 Uint16Array, 포인터가 UNSIGNED_SHORT 라 f32 로 바꿔 올리면 조인트 번호가 깨져 jointMatrices 밖을 읽는다)
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, slice_to_bytes(data_array), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(attribute_location);
			gl.vertex_attrib_pointer_i32(attribute_location, component_count, glow::UNSIGNED_SHORT, 0, 0);
			Ok(Some(vertex_buffer))
		}
	}

	//==============================================================================
	/// 애니메이션 재생. (크로스페이드, 같은 이름이 여럿이면 원본처럼 마지막 것, 원본 fadeDuration 기본값 0.25. 다 받은 애니메이션 FBX 의 클립을 먼저 더한다)
	///
	/// # 인자
	/// - `animation_name`: 틀 클립 이름. (없는 이름이거나 지금 클립과 같으면 무시)
	/// - `fade_duration`: 크로스페이드 시간. (초, 0 이면 다음 update 에서 바로 바뀐다)
	//==============================================================================
	pub fn play_animation(&mut self, animation_name: &str, fade_duration: f64) {
		self.apply_finished_animation_loads();
		let animation_list = self.animation_list.borrow();
		if let Some(current_animation_index) = self.current_animation_index {
			if animation_list[current_animation_index].name == animation_name {
				return;
			}
		}
		let found_animation_index = animation_list.iter().rposition(|animation| animation.name == animation_name);
		drop(animation_list);
		let found_animation_index = match found_animation_index {
			Some(found_animation_index) => {
				found_animation_index
			}
			None => {
				return;
			}
		};
		self.previous_animation_index = self.current_animation_index;
		self.previous_time = self.current_time;
		self.current_animation_index = Some(found_animation_index);
		self.current_time = 0.0;
		self.fade_duration = fade_duration;
		self.fade_elapsed = 0.0;
	}

	//==============================================================================
	/// 애니메이션 샘플링. (클립 인덱스, 현재 노드 포즈에 가중 블렌딩)
	///
	/// # 인자
	/// - `animation_index`: 클립 인덱스. (밖이면 무시)
	/// - `time`: 클립 시간. (초, 클립 길이로 나눈 나머지로 반복한다)
	/// - `blend_weight`: 섞는 비율. (0 ~ 1, 0 이면 지금 포즈 그대로, 1 이면 클립 값)
	//==============================================================================
	pub fn sample_animation(&mut self, animation_index: usize, time: f64, blend_weight: f64) {
		let mut animation_list = self.animation_list.borrow_mut();
		let animation = match animation_list.get_mut(animation_index) {
			Some(animation) => {
				animation
			}
			None => {
				return;
			}
		};
		sample_animation_clip(animation, &mut self.node_list, &mut self.drawable_list, time, blend_weight);
	}

	//==============================================================================
	/// 갱신. (다 받은 애니메이션 FBX 의 클립 더하기 → 시간 전진 → 포즈 샘플링 → 월드/조인트 행렬 계산, GL 무관)
	///
	/// # 인자
	/// - `delta_time`: 지난 시간. (초, 클립 시간에는 재생 속도를 곱하고 크로스페이드 시간에는 곱하지 않는다)
	//==============================================================================
	pub fn update(&mut self, delta_time: f64) {
		self.apply_finished_animation_loads();
		// 애니메이션이 없어도 기본 포즈로 월드/조인트 행렬은 계산한다. (정적 모델 지원)
		let current_animation_index = self.current_animation_index;
		if current_animation_index.is_some() {
			self.current_time += delta_time * self.time_scale;
			if self.previous_animation_index.is_some() {
				self.previous_time += delta_time * self.time_scale;
				self.fade_elapsed += delta_time;
				if self.fade_elapsed >= self.fade_duration {
					self.previous_animation_index = None;
				}
			}
		}

		// 1. 기본 포즈로 리셋.
		for node in self.node_list.iter_mut() {
			node.current_translation = node.base_translation;
			node.current_scale = node.base_scale;
			node.current_rotation = node.base_rotation;
		}

		// 2. 이전/현재 클립 샘플링. (크로스페이드)
		if let Some(current_animation_index) = current_animation_index {
			match self.previous_animation_index {
				Some(previous_animation_index) => {
					let previous_time = self.previous_time;
					self.sample_animation(previous_animation_index, previous_time, 1.0);
					let blend_weight = (self.fade_elapsed / self.fade_duration.max(0.0001)).min(1.0);
					let current_time = self.current_time;
					self.sample_animation(current_animation_index, current_time, blend_weight);
				}
				None => {
					let current_time = self.current_time;
					self.sample_animation(current_animation_index, current_time, 1.0);
				}
			}
		}

		// 3. 조인트 회전 오프셋. (절차 포즈 가공)
		for (node_index, offset_rotation) in self.joint_rotation_offsets.iter() {
			if let Some(node) = self.node_list.get_mut(*node_index) {
				node.current_rotation = node.current_rotation.multiply(*offset_rotation);
			}
		}

		// 4. 월드 행렬 계산. (루트부터 명시적 스택 순회)
		let mut traversal_stack: Vec<(usize, Option<usize>)> = Vec::new();
		for root_index in self.root_node_indices.iter() {
			traversal_stack.push((*root_index, None));
		}
		while let Some((node_index, parent_index)) = traversal_stack.pop() {
			let parent_world_matrix = parent_index.and_then(|parent_index| self.node_list.get(parent_index)).map(|parent_node| parent_node.world_matrix);
			let node = match self.node_list.get_mut(node_index) {
				Some(node) => {
					node
				}
				None => {
					continue;
				}
			};
			let mut local_matrix = Matrix4::create_translation(node.current_translation[0], node.current_translation[1], node.current_translation[2]);
			let rotation_matrix = Matrix4::create_rotation_from_quaternion(node.current_rotation);
			local_matrix.multiply(&rotation_matrix);
			local_matrix.scale(node.current_scale[0], node.current_scale[1], node.current_scale[2]);
			match parent_world_matrix {
				Some(parent_world_matrix) => {
					let mut world_matrix = parent_world_matrix;
					world_matrix.multiply(&local_matrix);
					node.world_matrix = world_matrix;
				}
				None => {
					node.world_matrix = local_matrix;
				}
			}
			for child_index in node.child_indices.iter() {
				traversal_stack.push((*child_index, Some(node_index)));
			}
		}

		// 5. 조인트 행렬. (월드 x 역바인드, 본이 없는 조인트는 단위 행렬)
		let node_list = &self.node_list;
		for skin in self.skin_list.iter_mut() {
			for joint_index in 0..skin.joint_node_indices.len() {
				let joint_matrix = match node_list.get(skin.joint_node_indices[joint_index]) {
					Some(joint_node) => {
						let mut joint_matrix = joint_node.world_matrix;
						joint_matrix.multiply(&skin.inverse_bind_matrices[joint_index]);
						joint_matrix
					}
					None => {
						Matrix4::create_identity()
					}
				};
				joint_matrix.write_to_slice(&mut skin.joint_matrix_array, joint_index * 16);
			}
		}
	}

	//==============================================================================
	/// 조인트 회전 오프셋 설정. (이름 부분 일치 노드에 로컬 회전 누적)
	///
	/// # 인자
	/// - `joint_name_part`: 노드 이름에 든 글자. (부분 일치, 맞는 노드 모두)
	/// - `offset_rotation`: 샘플링한 로컬 회전 뒤에 곱할 회전. (같은 노드에 다시 주면 바꾼다)
	///
	/// # 예제
	/// 이름에 Head 가 든 노드를 Y 축으로 30도 더 돌리기
	/// ```rust
	/// use vanilla::{Quaternion, SkinnedModel};
	///
	/// let mut skinned_model = SkinnedModel::create_without_shader_program();
	/// skinned_model.set_joint_rotation_offset("Head", Quaternion::create_from_euler(0.0, 30.0f64.to_radians(), 0.0));
	/// skinned_model.update(0.0);
	/// skinned_model.clear_joint_rotation_offsets();
	/// ```
	//==============================================================================
	pub fn set_joint_rotation_offset(&mut self, joint_name_part: &str, offset_rotation: Quaternion) {
		for node_index in 0..self.node_list.len() {
			if !self.node_list[node_index].name.contains(joint_name_part) {
				continue;
			}
			match self.joint_rotation_offsets.iter_mut().find(|entry| entry.0 == node_index) {
				Some(entry) => {
					entry.1 = offset_rotation;
				}
				None => {
					self.joint_rotation_offsets.push((node_index, offset_rotation));
				}
			}
		}
	}

	//==============================================================================
	/// 조인트 회전 오프셋 전체 해제.
	//==============================================================================
	pub fn clear_joint_rotation_offsets(&mut self) {
		self.joint_rotation_offsets.clear();
	}

	//==============================================================================
	/// 애니메이션 이름 목록 반환.
	///
	/// # 돌려줌
	/// 클립 이름 목록. (클립 차례, 같은 이름이 여럿일 수 있다)
	//==============================================================================
	pub fn get_animation_name_list(&self) -> Vec<String> {
		self.animation_list.borrow().iter().map(|animation| animation.name.clone()).collect()
	}

	//==============================================================================
	/// 재생 속도 설정.
	///
	/// # 인자
	/// - `time_scale`: 재생 속도 배수. (기본 1, 0 이면 멈춘다, 크로스페이드 시간에는 걸리지 않는다)
	//==============================================================================
	pub fn set_time_scale(&mut self, time_scale: f64) {
		self.time_scale = time_scale;
	}

	//==============================================================================
	/// 셰이더 프로그램 반환. (셰이더 없이 만든 모델이면 None)
	///
	/// # 돌려줌
	/// 업로드에 쓰는 셰이더 프로그램. 셰이더 없이 만든 모델이면 None.
	//==============================================================================
	pub fn get_shader_program(&self) -> Option<&Rc<ShaderProgram>> {
		self.shader_program.as_ref()
	}

	//==============================================================================
	/// 노드 목록 반환.
	///
	/// # 돌려줌
	/// 노드 목록. (노드 인덱스 차례)
	//==============================================================================
	pub fn get_node_list(&self) -> &[SkinnedModelNode] {
		&self.node_list
	}

	//==============================================================================
	/// 노드 목록 반환. (변경용, 원본은 살아 있는 배열을 내준다)
	///
	/// # 돌려줌
	/// 바꿀 수 있는 노드 목록. (base_ 값을 바꾸면 다음 update 부터 기본 포즈가 바뀐다)
	//==============================================================================
	pub fn get_node_list_mut(&mut self) -> &mut [SkinnedModelNode] {
		&mut self.node_list
	}

	//==============================================================================
	/// 루트 노드 인덱스 목록 반환.
	///
	/// # 돌려줌
	/// 루트 노드 인덱스 목록.
	//==============================================================================
	pub fn get_root_node_indices(&self) -> &[usize] {
		&self.root_node_indices
	}

	//==============================================================================
	/// 스킨 목록 반환.
	///
	/// # 돌려줌
	/// 스킨 목록. (조인트 행렬은 마지막 update 의 것)
	//==============================================================================
	pub fn get_skin_list(&self) -> &[Skin] {
		&self.skin_list
	}

	//==============================================================================
	/// 드로어블 목록 반환.
	///
	/// # 돌려줌
	/// 드로어블 목록. (업로드 전이나 셰이더 없이 만든 모델이면 비어 있다)
	//==============================================================================
	pub fn get_drawable_list(&self) -> &[Drawable] {
		&self.drawable_list
	}

	//==============================================================================
	/// 드로어블 목록 반환. (변경용)
	///
	/// # 돌려줌
	/// 바꿀 수 있는 드로어블 목록.
	//==============================================================================
	pub fn get_drawable_list_mut(&mut self) -> &mut [Drawable] {
		&mut self.drawable_list
	}

	//==============================================================================
	/// 애니메이션 클립 목록 반환. (길이, 채널 보기. 애니메이션 FBX 받기 손잡이와 나눠 드는 목록이라 빌린 값으로 준다)
	///
	/// # 돌려줌
	/// 클립 목록을 빌린 값. (`Ref` 를 든 동안 await 중인 AnimationLoadFuture 는 클립을 더하지 못하고 기다린다)
	//==============================================================================
	pub fn get_animation_list(&self) -> std::cell::Ref<'_, [AnimationClip]> {
		std::cell::Ref::map(self.animation_list.borrow(), |animation_list| animation_list.as_slice())
	}

	//==============================================================================
	/// 파괴. (이 인스턴스 것을 지우고 자산을 놓는다, 마지막 인스턴스면 자산이 버퍼, 버텍스 어레이, 모프 텍스처, 머티리얼 원형을 지운다)
	/// - 이 인스턴스 것: 와이어프레임 버퍼, add_morph_target 으로 갈라진 모프 텍스처, get_material 로 갈라진 머티리얼.
	/// - set_material 로 바깥에서 넣은 머티리얼은 지우지 않는다. 자산의 머티리얼 원형을 다른 모델에 넣어 썼다면 자산이 지워질 때 함께 지워진다.
	///
	/// # 인자
	/// - `gl`: 드로어블을 지은 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		for drawable in self.drawable_list.drain(..) {
			unsafe {
				if let Some(wireframe_index_buffer) = drawable.wireframe_index_buffer {
					gl.delete_buffer(wireframe_index_buffer);
				}
				if !drawable.is_morph_texture_shared {
					if let Some(morph_texture) = drawable.morph_texture {
						crate::core::glstatecache::delete_texture(gl, morph_texture);
					}
				}
			}
		}
		for instance_material in self.instance_material_list.drain(..) {
			instance_material.destroy(gl);
		}
		if let Some(skinned_model_asset) = self.asset.take() {
			skinned_model_asset.release(gl);
		}
	}
}

//==============================================================================
/// 애니메이션 클립 샘플링. (현재 노드 포즈에 가중 블렌딩, 원본 sampleAnimation 본문)
//==============================================================================
fn sample_animation_clip(animation: &mut AnimationClip, node_list: &mut [SkinnedModelNode], drawable_list: &mut [Drawable], time: f64, blend_weight: f64) {
	let looped_time = if animation.duration > 0.0 {
		time % animation.duration
	}
	else {
		0.0
	};
	for channel in animation.channels.iter_mut() {
		let node = match node_list.get_mut(channel.node_index) {
			Some(node) => {
				node
			}
			None => {
				continue;
			}
		};
		let times = &channel.times;
		let key_count = times.len();
		if key_count == 0 {
			continue;
		}

		// 키프레임 커서 탐색. (역방향 점프 시 리셋 후 전진)
		if channel.cursor + 1 >= key_count || times[channel.cursor] > looped_time {
			channel.cursor = 0;
		}
		while channel.cursor + 2 < key_count && times[channel.cursor + 1] < looped_time {
			channel.cursor += 1;
		}
		let frame_index = channel.cursor;
		let next_index = (frame_index + 1).min(key_count - 1);
		let span_duration = times[next_index] - times[frame_index];
		let factor = if span_duration > 0.0 {
			((looped_time - (times[frame_index])) / span_duration).clamp(0.0, 1.0)
		}
		else {
			0.0
		};
		let values = &channel.values;

		match channel.path {
			AnimationPath::Rotation => {
				let base_offset = frame_index * 4;
				let next_offset = next_index * 4;
				if next_offset + 4 > values.len() || base_offset + 4 > values.len() {
					continue;
				}
				let from_rotation = Quaternion::create(values[base_offset], values[base_offset + 1], values[base_offset + 2], values[base_offset + 3]);
				let to_rotation = Quaternion::create(values[next_offset], values[next_offset + 1], values[next_offset + 2], values[next_offset + 3]);
				let sampled_rotation = Quaternion::slerp(from_rotation, to_rotation, factor);
				if blend_weight >= 1.0 {
					node.current_rotation = sampled_rotation;
				}
				else {
					node.current_rotation = Quaternion::slerp(node.current_rotation, sampled_rotation, blend_weight);
				}
			}
			AnimationPath::Translation | AnimationPath::Scale => {
				let base_offset = frame_index * 3;
				let next_offset = next_index * 3;
				if next_offset + 3 > values.len() || base_offset + 3 > values.len() {
					continue;
				}
				let target_array = if channel.path == AnimationPath::Translation {
					&mut node.current_translation
				}
				else {
					&mut node.current_scale
				};
				for component_index in 0..3 {
					let from_value = values[base_offset + component_index];
					let to_value = values[next_offset + component_index];
					let sampled_value = from_value + (to_value - from_value) * factor;
					target_array[component_index] += (sampled_value - target_array[component_index]) * blend_weight;
				}
			}
			AnimationPath::Weights => {
				// 모프 가중치. (키마다 타깃 개수만큼의 값, 해당 노드의 드로어블 전부에 적용)
				for drawable in drawable_list.iter_mut() {
					if drawable.node_index != Some(channel.node_index) {
						continue;
					}
					let target_count = drawable.morph_weights.len();
					let base_offset = frame_index * target_count;
					let next_offset = next_index * target_count;
					if next_offset + target_count > values.len() || base_offset + target_count > values.len() {
						continue;
					}
					for target_index in 0..target_count {
						let from_value = values[base_offset + target_index];
						let to_value = values[next_offset + target_index];
						let sampled_value = from_value + (to_value - from_value) * factor;
						let current_weight = drawable.morph_weights[target_index] as f64;
						drawable.morph_weights[target_index] = (current_weight + (sampled_value - current_weight) * blend_weight) as f32;
					}
				}
			}
		}
	}
}

//==============================================================================
/// FBX 오일러(도) → 쿼터니언. (eEulerXYZ = X 먼저 적용, 열벡터 기준 Rz x Ry x Rx 합성)
///
/// # 인자
/// - `x_degree`: X 축 회전. (도)
/// - `y_degree`: Y 축 회전. (도)
/// - `z_degree`: Z 축 회전. (도)
///
/// # 돌려줌
/// X, Y, Z 차례로 적용한 회전.
///
/// # 예제
/// X 90도 뒤 Z 90도: +X 축이 +Y 로 간다
/// ```rust
/// use vanilla::Matrix4;
/// use vanilla::graphics3d::skinnedmodel::create_quaternion_from_fbx_euler;
///
/// let rotation = create_quaternion_from_fbx_euler(90.0, 0.0, 90.0);
/// let rotated_x = Matrix4::create_rotation_from_quaternion(rotation).transform_direction(1.0, 0.0, 0.0);
/// assert!((rotated_x.y - 1.0).abs() < 1.0e-5);
/// ```
//==============================================================================
pub fn create_quaternion_from_fbx_euler(x_degree: f64, y_degree: f64, z_degree: f64) -> Quaternion {
	let degree_to_radian = std::f64::consts::PI / 180.0;
	let rotation_x = Quaternion::create_from_euler(x_degree * degree_to_radian, 0.0, 0.0);
	let rotation_y = Quaternion::create_from_euler(0.0, y_degree * degree_to_radian, 0.0);
	let rotation_z = Quaternion::create_from_euler(0.0, 0.0, z_degree * degree_to_radian);
	let rotation_zy = rotation_z.multiply(rotation_y);
	rotation_zy.multiply(rotation_x)
}

//==============================================================================
/// FBX 애니메이션 채널 변환. (회전 = 오일러 도 → 사전회전 합성 쿼터니언 4성분)
/// - resolve_node_index 로 대상 노드 인덱스를 재지정한다. (본 이름 매칭 리타게팅용, None 이면 버린다)
//==============================================================================
fn convert_fbx_animation_channels(scene_data: &FbxSceneData, fbx_animation: &FbxAnimation, resolve_node_index: impl Fn(usize) -> Option<usize>) -> Vec<AnimationChannel> {
	let mut channels = Vec::with_capacity(fbx_animation.channels.len());
	for fbx_channel in fbx_animation.channels.iter() {
		let target_node_index = match resolve_node_index(fbx_channel.node_index) {
			Some(target_node_index) => {
				target_node_index
			}
			None => {
				continue;
			}
		};
		if fbx_channel.path == AnimationPath::Rotation {
			let pre_rotation_degrees = scene_data.node_list.get(fbx_channel.node_index).map(|source_fbx_node| source_fbx_node.pre_rotation_degrees).unwrap_or([0.0, 0.0, 0.0]);
			let pre_rotation_quaternion = create_quaternion_from_fbx_euler(pre_rotation_degrees[0], pre_rotation_degrees[1], pre_rotation_degrees[2]);
			let key_count = fbx_channel.times.len();
			let mut quaternion_values = vec![0.0f64; key_count * 4];
			for key_index in 0..key_count {
				let read_value = |component_index: usize| fbx_channel.values.get(key_index * 3 + component_index).copied().unwrap_or(0.0);
				let euler_quaternion = create_quaternion_from_fbx_euler(read_value(0), read_value(1), read_value(2));
				let rotation_quaternion = pre_rotation_quaternion.multiply(euler_quaternion);
				quaternion_values[key_index * 4] = rotation_quaternion.x;
				quaternion_values[key_index * 4 + 1] = rotation_quaternion.y;
				quaternion_values[key_index * 4 + 2] = rotation_quaternion.z;
				quaternion_values[key_index * 4 + 3] = rotation_quaternion.w;
			}
			channels.push(AnimationChannel { node_index: target_node_index, path: fbx_channel.path, times: fbx_channel.times.clone(), values: quaternion_values, cursor: 0 });
		}
		else {
			channels.push(AnimationChannel { node_index: target_node_index, path: fbx_channel.path, times: fbx_channel.times.clone(), values: fbx_channel.values.clone(), cursor: 0 });
		}
	}
	channels
}

//==============================================================================
/// 접근자 CPU 읽기. (촘촘한 배치 + 인터리브(byte_stride) 배치 + 희소(sparse) 접근자 지원)
///
/// # 인자
/// - `json`: glTF JSON 전체.
/// - `binary_buffer`: BIN 청크 바이트.
/// - `accessor_index`: 접근자 인덱스.
///
/// # 돌려줌
/// 컴포넌트 타입 그대로의 배열. (정규화하지 않는다, bufferView 가 없는 접근자는 0 으로 채운다) 접근자가 없거나, 타입을 모르거나, 버퍼 밖을 가리키면 Err.
///
/// # 예제
/// 실수 VEC2 접근자 하나 읽기
/// ```rust
/// use serde_json::json;
/// use vanilla::graphics3d::skinnedmodel::{read_accessor_array, AccessorArray};
///
/// let binary_buffer: Vec<u8> = [0.5f32, 1.0, 1.5, 2.0].iter().flat_map(|value| value.to_le_bytes()).collect();
/// let json = json!({
/// 	"accessors": [{ "bufferView": 0, "componentType": 5126, "count": 2, "type": "VEC2" }],
/// 	"bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 16 }]
/// });
/// assert_eq!(read_accessor_array(&json, &binary_buffer, 0), Ok(AccessorArray::Float(vec![0.5, 1.0, 1.5, 2.0])));
/// assert!(read_accessor_array(&json, &binary_buffer, 1).is_err());
/// ```
//==============================================================================
pub fn read_accessor_array(json: &Value, binary_buffer: &[u8], accessor_index: usize) -> Result<AccessorArray, String> {
	let accessor = json["accessors"].get(accessor_index).ok_or_else(|| format!("SkinnedModel: accessor {} not found.", accessor_index))?;
	let type_name = accessor["type"].as_str().unwrap_or("");
	let component_count = type_component_count(type_name).ok_or_else(|| format!("SkinnedModel: unknown accessor type {}.", type_name))?;
	let component_type = accessor["componentType"].as_u64().unwrap_or(0) as u32;
	let component_byte_size = component_type_byte_size(component_type).ok_or_else(|| format!("SkinnedModel: unsupported accessor component type {}.", component_type))?;
	let count = read_index(&accessor["count"]).unwrap_or(0);
	let tight_byte_size = component_count * component_byte_size;
	let mut element_bytes = vec![0u8; count * tight_byte_size];

	if let Some(buffer_view_index) = read_index(&accessor["bufferView"]) {
		let buffer_view = json["bufferViews"].get(buffer_view_index).ok_or_else(|| format!("SkinnedModel: bufferView {} not found.", buffer_view_index))?;
		let element_offset = read_index(&buffer_view["byteOffset"]).unwrap_or(0) + read_index(&accessor["byteOffset"]).unwrap_or(0);
		let byte_stride = read_index(&buffer_view["byteStride"]).unwrap_or(0);
		if byte_stride == 0 || byte_stride == tight_byte_size {
			let source_bytes = binary_buffer.get(element_offset..element_offset + count * tight_byte_size).ok_or_else(|| String::from("SkinnedModel: accessor out of buffer range."))?;
			element_bytes.copy_from_slice(source_bytes);
		}
		else {
			for element_index in 0..count {
				let source_start = element_offset + element_index * byte_stride;
				let source_bytes = binary_buffer.get(source_start..source_start + tight_byte_size).ok_or_else(|| String::from("SkinnedModel: accessor out of buffer range."))?;
				element_bytes[element_index * tight_byte_size..(element_index + 1) * tight_byte_size].copy_from_slice(source_bytes);
			}
		}
	}

	// 희소 접근자. (인덱스 목록의 요소만 값으로 덮어쓴다, 모프 타깃 델타에 흔히 쓰인다)
	let sparse = &accessor["sparse"];
	if sparse.is_object() {
		let sparse_count = read_index(&sparse["count"]).unwrap_or(0);
		let sparse_indices = &sparse["indices"];
		let index_buffer_view_index = read_index(&sparse_indices["bufferView"]).ok_or_else(|| String::from("SkinnedModel: sparse indices have no bufferView."))?;
		let index_buffer_view = &json["bufferViews"][index_buffer_view_index];
		let index_component_type = sparse_indices["componentType"].as_u64().unwrap_or(0) as u32;
		let index_byte_size = component_type_byte_size(index_component_type).ok_or_else(|| format!("SkinnedModel: unsupported sparse index type {}.", index_component_type))?;
		let index_offset = read_index(&index_buffer_view["byteOffset"]).unwrap_or(0) + read_index(&sparse_indices["byteOffset"]).unwrap_or(0);
		let sparse_values = &sparse["values"];
		let value_buffer_view_index = read_index(&sparse_values["bufferView"]).ok_or_else(|| String::from("SkinnedModel: sparse values have no bufferView."))?;
		let value_buffer_view = &json["bufferViews"][value_buffer_view_index];
		let value_offset = read_index(&value_buffer_view["byteOffset"]).unwrap_or(0) + read_index(&sparse_values["byteOffset"]).unwrap_or(0);
		for sparse_index in 0..sparse_count {
			let index_start = index_offset + sparse_index * index_byte_size;
			let index_bytes = binary_buffer.get(index_start..index_start + index_byte_size).ok_or_else(|| String::from("SkinnedModel: sparse indices out of buffer range."))?;
			let target_element = match index_byte_size {
				1 => {
					index_bytes[0] as usize
				}
				2 => {
					u16::from_le_bytes([index_bytes[0], index_bytes[1]]) as usize
				}
				_ => {
					u32::from_le_bytes([index_bytes[0], index_bytes[1], index_bytes[2], index_bytes[3]]) as usize
				}
			};
			let value_start = value_offset + sparse_index * tight_byte_size;
			let value_bytes = binary_buffer.get(value_start..value_start + tight_byte_size).ok_or_else(|| String::from("SkinnedModel: sparse values out of buffer range."))?;
			if let Some(target_bytes) = element_bytes.get_mut(target_element * tight_byte_size..(target_element + 1) * tight_byte_size) {
				target_bytes.copy_from_slice(value_bytes);
			}
		}
	}

	// 바이트 → 컴포넌트 타입 배열. (리틀 엔디언)
	let accessor_array = match component_type {
		COMPONENT_TYPE_UNSIGNED_BYTE => {
			AccessorArray::UnsignedByte(element_bytes)
		}
		COMPONENT_TYPE_UNSIGNED_SHORT => {
			AccessorArray::UnsignedShort(element_bytes.as_chunks::<2>().0.iter().map(|chunk| u16::from_le_bytes(*chunk)).collect())
		}
		COMPONENT_TYPE_UNSIGNED_INT => {
			AccessorArray::UnsignedInt(element_bytes.as_chunks::<4>().0.iter().map(|chunk| u32::from_le_bytes(*chunk)).collect())
		}
		_ => {
			AccessorArray::Float(element_bytes.as_chunks::<4>().0.iter().map(|chunk| f32::from_le_bytes(*chunk)).collect())
		}
	};
	Ok(accessor_array)
}

//==============================================================================
/// 이미지 서술 구성. (텍스처 참조 → 임베디드 이미지 바이트, 버퍼뷰가 없는 이미지는 None)
//==============================================================================
fn create_gltf_image_description(json: &Value, binary_buffer: &[u8], texture_info: &Value) -> Option<ImageDescription> {
	let texture_index = read_index(&texture_info["index"])?;
	let texture = json["textures"].get(texture_index)?;
	let image_index = read_index(&texture["source"])?;
	let image = json["images"].get(image_index)?;
	let buffer_view_index = read_index(&image["bufferView"])?;
	let buffer_view = json["bufferViews"].get(buffer_view_index)?;
	let byte_offset = read_index(&buffer_view["byteOffset"]).unwrap_or(0);
	let byte_length = read_index(&buffer_view["byteLength"]).unwrap_or(0);
	let image_bytes = binary_buffer.get(byte_offset..byte_offset + byte_length)?;
	Some(ImageDescription {
		bytes: Rc::new(image_bytes.to_vec()),
		mime_type: image["mimeType"].as_str().map(|mime_type| mime_type.to_string()),
		source: None,
		flip_y: false,
		is_color_data: false,
		anisotropy: 0.0,
	})
}

//==============================================================================
/// 어트리뷰트 타입별 컴포넌트 수. (원본 TYPE_COMPONENT_COUNT_TABLE)
//==============================================================================
fn type_component_count(type_name: &str) -> Option<usize> {
	match type_name {
		"SCALAR" => {
			Some(1)
		}
		"VEC2" => {
			Some(2)
		}
		"VEC3" => {
			Some(3)
		}
		"VEC4" => {
			Some(4)
		}
		"MAT4" => {
			Some(16)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 컴포넌트 타입별 바이트 크기. (원본 COMPONENT_ARRAY_TABLE 의 BYTES_PER_ELEMENT, 표에 없으면 None)
//==============================================================================
fn component_type_byte_size(component_type: u32) -> Option<usize> {
	match component_type {
		COMPONENT_TYPE_UNSIGNED_BYTE => {
			Some(1)
		}
		COMPONENT_TYPE_UNSIGNED_SHORT => {
			Some(2)
		}
		COMPONENT_TYPE_UNSIGNED_INT | COMPONENT_TYPE_FLOAT => {
			Some(4)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// JSON 인덱스 읽기. (음이 아닌 정수만)
//==============================================================================
fn read_index(value: &Value) -> Option<usize> {
	value.as_u64().map(|index| index as usize)
}

//==============================================================================
/// JSON 인덱스 배열 읽기. (없으면 빈 목록)
//==============================================================================
fn read_index_array(value: &Value) -> Vec<usize> {
	match value.as_array() {
		Some(index_values) => {
			index_values.iter().filter_map(read_index).collect()
		}
		None => {
			Vec::new()
		}
	}
}

//==============================================================================
/// JSON 실수 배열 읽기. (배열이 아니면 None)
//==============================================================================
fn read_float_array(value: &Value) -> Option<Vec<f64>> {
	value.as_array().map(|number_values| number_values.iter().map(|number_value| number_value.as_f64().unwrap_or(0.0)).collect())
}

//==============================================================================
/// JSON 3성분 벡터 읽기. (없거나 짧으면 기본값)
//==============================================================================
fn read_vector3(value: &Value, default_value: [f64; 3]) -> [f64; 3] {
	match read_float_array(value) {
		Some(values) if values.len() >= 3 => {
			[values[0], values[1], values[2]]
		}
		_ => {
			default_value
		}
	}
}

//==============================================================================
/// 부호 없는 32비트 읽기. (리틀 엔디언, 밖이면 None)
//==============================================================================
fn read_uint32(bytes: &[u8], offset: usize) -> Option<u32> {
	let value_bytes = bytes.get(offset..offset + 4)?;
	Some(u32::from_le_bytes([value_bytes[0], value_bytes[1], value_bytes[2], value_bytes[3]]))
}

/// GPU 버퍼에 그대로 올리는 수 꼴. (f32 와 정수, 로직 셈의 f64 는 올리기 전에 f32 로 바꾼다)
trait GpuScalar: Copy {}
impl GpuScalar for f32 {}
impl GpuScalar for u8 {}
impl GpuScalar for u16 {}
impl GpuScalar for u32 {}
impl GpuScalar for i32 {}

//==============================================================================
/// 숫자 배열을 바이트 배열로 본다. (GL 버퍼 업로드용, 리틀 엔디언 대상)
//==============================================================================
fn slice_to_bytes<T: GpuScalar>(values: &[T]) -> &[u8] {
	// GPU 로 가는 수는 f32 와 정수뿐이다. (원본 Float32Array, Uint16Array, Uint32Array, f64 를 넘기면 컴파일이 막는다)
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	/// 예제 애셋 폴더. (이 저장소 옆에 받아 둔 vanilla.js 저장소, 읽기만 한다. 없으면 그 시험은 건너뛴다)
	const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

	//==============================================================================
	/// 시험용 GLB 바이너리 버퍼. (바이트를 붙이고 buffer_view 를 기록한다)
	//==============================================================================
	struct GlbBufferBuilder {
		binary_bytes: Vec<u8>,
		buffer_view_list: Vec<Value>,
	}

	impl GlbBufferBuilder {
		//==============================================================================
		/// 생성.
		//==============================================================================
		fn new() -> GlbBufferBuilder {
			GlbBufferBuilder {
				binary_bytes: Vec::new(),
				buffer_view_list: Vec::new(),
			}
		}

		//==============================================================================
		/// 바이트 붙이기. (4 바이트 맞춤, byte_stride 가 0 이 아니면 기록, buffer_view 번호를 돌려준다)
		//==============================================================================
		fn add_buffer_view(&mut self, bytes: &[u8], byte_stride: usize) -> usize {
			while !self.binary_bytes.len().is_multiple_of(4) {
				self.binary_bytes.push(0);
			}
			let mut buffer_view = json!({ "buffer": 0, "byteOffset": self.binary_bytes.len(), "byteLength": bytes.len() });
			if byte_stride > 0 {
				buffer_view["byteStride"] = json!(byte_stride);
			}
			self.binary_bytes.extend_from_slice(bytes);
			self.buffer_view_list.push(buffer_view);
			self.buffer_view_list.len() - 1
		}
	}

	//==============================================================================
	/// 실수 목록을 리틀 엔디언 바이트로.
	//==============================================================================
	fn float_bytes(values: &[f32]) -> Vec<u8> {
		values.iter().flat_map(|value| value.to_le_bytes()).collect()
	}

	//==============================================================================
	/// JSON + BIN 을 GLB 파일 바이트로 감싼다.
	//==============================================================================
	fn wrap_glb(json: &Value, binary_bytes: &[u8]) -> Vec<u8> {
		let mut json_bytes = serde_json::to_vec(json).expect("json");
		while !json_bytes.len().is_multiple_of(4) {
			json_bytes.push(b' ');
		}
		let mut padded_binary_bytes = binary_bytes.to_vec();
		while !padded_binary_bytes.len().is_multiple_of(4) {
			padded_binary_bytes.push(0);
		}
		let total_length = 12 + 8 + json_bytes.len() + 8 + padded_binary_bytes.len();
		let mut glb_bytes = Vec::with_capacity(total_length);
		glb_bytes.extend_from_slice(&GLB_MAGIC.to_le_bytes());
		glb_bytes.extend_from_slice(&2u32.to_le_bytes());
		glb_bytes.extend_from_slice(&(total_length as u32).to_le_bytes());
		glb_bytes.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
		glb_bytes.extend_from_slice(&GLB_CHUNKTYPE_JSON.to_le_bytes());
		glb_bytes.extend_from_slice(&json_bytes);
		glb_bytes.extend_from_slice(&(padded_binary_bytes.len() as u32).to_le_bytes());
		glb_bytes.extend_from_slice(&GLB_CHUNKTYPE_BINARY.to_le_bytes());
		glb_bytes.extend_from_slice(&padded_binary_bytes);
		glb_bytes
	}

	//==============================================================================
	/// 시험용 GLB. (뼈 2개 스킨 삼각형 + 같은 메시를 쓰는 정적 노드, 인터리브 위치/UV, 8비트 인덱스와 가중치, 희소 접근자, 클립 2개)
	/// - 노드 0 Root(자식 1, 2, 3), 1 Joint(이동 0,1,0), 2 스킨 메시, 3 정적 메시(이동 5,0,0).
	/// - 클립 A: Joint 이동 (0,1,0) → (0,3,0) 0~2초, 클립 B: Joint 이동 (4,1,0) 고정 + 회전 Z 90도.
	//==============================================================================
	fn build_test_glb() -> Vec<u8> {
		let mut buffer_builder = GlbBufferBuilder::new();

		// 위치 + UV 인터리브. (정점 3개, 정점당 20 바이트)
		let interleaved_values: [f32; 15] = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0];
		let interleaved_view = buffer_builder.add_buffer_view(&float_bytes(&interleaved_values), 20);
		let index_view = buffer_builder.add_buffer_view(&[0u8, 1, 2], 0);
		let joint_view = buffer_builder.add_buffer_view(&[0u8, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], 0);
		let weight_view = buffer_builder.add_buffer_view(&[128u8, 127, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0], 0);
		let mut inverse_bind_values = Vec::new();
		inverse_bind_values.extend_from_slice(Matrix4::create_identity().get_elements());
		inverse_bind_values.extend_from_slice(Matrix4::create_translation(0.0, -1.0, 0.0).get_elements());
		let inverse_bind_view = buffer_builder.add_buffer_view(&float_bytes(&inverse_bind_values), 0);
		let time_view = buffer_builder.add_buffer_view(&float_bytes(&[0.0, 2.0]), 0);
		let translation_aview = buffer_builder.add_buffer_view(&float_bytes(&[0.0, 1.0, 0.0, 0.0, 3.0, 0.0]), 0);
		let translation_bview = buffer_builder.add_buffer_view(&float_bytes(&[4.0, 1.0, 0.0, 4.0, 1.0, 0.0]), 0);
		let half_square_root = std::f64::consts::FRAC_1_SQRT_2;
		let rotation_bview = buffer_builder.add_buffer_view(&float_bytes(&[0.0, 0.0, half_square_root as f32, half_square_root as f32, 0.0, 0.0, half_square_root as f32, half_square_root as f32]), 0);
		let sparse_index_view = buffer_builder.add_buffer_view(&[2u8], 0);
		let sparse_value_view = buffer_builder.add_buffer_view(&float_bytes(&[7.0, 8.0, 9.0]), 0);

		let json = json!({
			"asset": { "version": "2.0" },
			"scene": 0,
			"scenes": [{ "nodes": [0] }],
			"nodes": [
				{ "name": "Root", "children": [1, 2, 3] },
				{ "name": "Joint", "translation": [0.0, 1.0, 0.0] },
				{ "name": "SkinnedMesh", "mesh": 0, "skin": 0 },
				{ "name": "StaticMesh", "mesh": 0, "translation": [5.0, 0.0, 0.0] }
			],
			"skins": [{ "joints": [0, 1], "inverseBindMatrices": 5 }],
			"materials": [{ "name": "testMaterial", "pbrMetallicRoughness": { "baseColorFactor": [0.25, 0.5, 0.75, 1.0], "roughnessFactor": 0.5 } }],
			"meshes": [{
				"weights": [0.25],
				"extras": { "targetNames": ["smile"] },
				"primitives": [{
					"attributes": { "POSITION": 0, "TEXCOORD_0": 1, "JOINTS_0": 3, "WEIGHTS_0": 4 },
					"indices": 2,
					"material": 0,
					"targets": [{ "POSITION": 11 }]
				}]
			}],
			"accessors": [
				{ "bufferView": interleaved_view, "byteOffset": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
				{ "bufferView": interleaved_view, "byteOffset": 12, "componentType": 5126, "count": 3, "type": "VEC2" },
				{ "bufferView": index_view, "componentType": 5121, "count": 3, "type": "SCALAR" },
				{ "bufferView": joint_view, "componentType": 5121, "count": 3, "type": "VEC4" },
				{ "bufferView": weight_view, "componentType": 5121, "normalized": true, "count": 3, "type": "VEC4" },
				{ "bufferView": inverse_bind_view, "componentType": 5126, "count": 2, "type": "MAT4" },
				{ "bufferView": time_view, "componentType": 5126, "count": 2, "type": "SCALAR" },
				{ "bufferView": translation_aview, "componentType": 5126, "count": 2, "type": "VEC3" },
				{ "bufferView": translation_bview, "componentType": 5126, "count": 2, "type": "VEC3" },
				{ "bufferView": rotation_bview, "componentType": 5126, "count": 2, "type": "VEC4" },
				{ "componentType": 5126, "count": 3, "type": "VEC3", "sparse": { "count": 1, "indices": { "bufferView": sparse_index_view, "componentType": 5121 }, "values": { "bufferView": sparse_value_view } } },
				{ "componentType": 5126, "count": 3, "type": "VEC3", "sparse": { "count": 1, "indices": { "bufferView": sparse_index_view, "componentType": 5121 }, "values": { "bufferView": sparse_value_view } } }
			],
			"animations": [
				{ "name": "A", "samplers": [{ "input": 6, "output": 7 }], "channels": [{ "sampler": 0, "target": { "node": 1, "path": "translation" } }] },
				{ "name": "B", "samplers": [{ "input": 6, "output": 8 }, { "input": 6, "output": 9 }], "channels": [{ "sampler": 0, "target": { "node": 1, "path": "translation" } }, { "sampler": 1, "target": { "node": 1, "path": "rotation" } }] }
			],
			"bufferViews": buffer_builder.buffer_view_list,
			"buffers": [{ "byteLength": buffer_builder.binary_bytes.len() }]
		});
		wrap_glb(&json, &buffer_builder.binary_bytes)
	}

	//==============================================================================
	/// 시험용 GLB 를 셰이더 없는 모델로 해석한다.
	//==============================================================================
	fn interpret_test_glb() -> (SkinnedModel, Vec<MeshDescription>, Vec<MaterialDescription>) {
		let glb_bytes = build_test_glb();
		let (json, binary_buffer) = SkinnedModel::read_glb_chunks(&glb_bytes).expect("glb chunks");
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		let (mesh_description_list, material_description_list) = skinned_model.interpret_content(&json, &binary_buffer).expect("interpret");
		(skinned_model, mesh_description_list, material_description_list)
	}

	//==============================================================================
	/// 값 두 개가 허용 오차 안에서 같은지.
	//==============================================================================
	fn assert_near(actual: f64, expected: f64) {
		assert!((actual - expected).abs() < 1.0e-5, "{} expected {}", actual, expected);
	}

	//==============================================================================
	/// GLB 청크와 접근자 해석. (인터리브, 8비트 인덱스 → 16비트, 희소 접근자, 잘못된 머리)
	//==============================================================================
	#[test]
	fn glb_accessor_decoding() {
		let glb_bytes = build_test_glb();
		let (json, binary_buffer) = SkinnedModel::read_glb_chunks(&glb_bytes).expect("glb chunks");
		let positions = read_accessor_array(&json, &binary_buffer, 0).expect("positions");
		assert_eq!(positions, AccessorArray::Float(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
		let texture_coordinates = read_accessor_array(&json, &binary_buffer, 1).expect("uv");
		assert_eq!(texture_coordinates, AccessorArray::Float(vec![0.0, 1.0, 1.0, 1.0, 0.0, 0.0]));
		let indices = read_accessor_array(&json, &binary_buffer, 2).expect("indices");
		assert_eq!(indices.get_component_type(), COMPONENT_TYPE_UNSIGNED_BYTE);
		assert_eq!(indices.to_index_array(), IndexArray::UnsignedShort(vec![0, 1, 2]));
		let sparse_array = read_accessor_array(&json, &binary_buffer, 10).expect("sparse");
		assert_eq!(sparse_array, AccessorArray::Float(vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 7.0, 8.0, 9.0]));
		let inverse_bind = read_accessor_array(&json, &binary_buffer, 5).expect("inverse bind").to_float_list();
		assert_eq!(inverse_bind.len(), 32);
		assert_eq!(inverse_bind[16 + 13], -1.0);

		let mut broken_bytes = glb_bytes.clone();
		broken_bytes[0] = 0;
		assert!(SkinnedModel::read_glb_chunks(&broken_bytes).is_err());
		assert!(SkinnedModel::read_glb_chunks(&glb_bytes[..20]).is_err());
	}

	//==============================================================================
	/// 내용 해석. (노드, 루트, 실제 스킨 + 정적 메시의 합성 스킨, 가중치 정규화, 모프 타깃, 머티리얼 서술, 클립)
	//==============================================================================
	#[test]
	fn interpret_content_builds_neutral_schema() {
		let (skinned_model, mesh_description_list, material_description_list) = interpret_test_glb();
		let node_list = skinned_model.get_node_list();
		assert_eq!(node_list.len(), 4);
		assert_eq!(node_list[0].child_indices, vec![1, 2, 3]);
		assert_eq!(skinned_model.get_root_node_indices(), &[0]);
		assert_eq!(node_list[1].base_translation, [0.0, 1.0, 0.0]);

		let skin_list = skinned_model.get_skin_list();
		assert_eq!(skin_list.len(), 2);
		assert_eq!(skin_list[0].joint_node_indices, vec![0, 1]);
		assert_eq!(skin_list[1].joint_node_indices, vec![3]);
		assert_eq!(node_list[3].skin_index, Some(1));

		assert_eq!(mesh_description_list.len(), 2);
		let skinned_mesh = &mesh_description_list[0];
		assert_eq!(skinned_mesh.skin_index, 0);
		assert_eq!(skinned_mesh.joints, vec![0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
		assert_near(skinned_mesh.weights[0], 128.0 / 255.0);
		assert_near(skinned_mesh.weights[4], 1.0);
		assert_eq!(skinned_mesh.indices, Some(IndexArray::UnsignedShort(vec![0, 1, 2])));
		assert!(skinned_mesh.normals.is_none());
		assert_eq!(skinned_mesh.material_index, Some(0));
		assert_eq!(skinned_mesh.material_name, "testMaterial");
		assert_eq!(skinned_mesh.morph_targets.len(), 1);
		assert_eq!(skinned_mesh.morph_targets[0].name, "smile");
		assert_eq!(skinned_mesh.morph_weights, vec![0.25]);
		let static_mesh = &mesh_description_list[1];
		assert_eq!(static_mesh.skin_index, 1);
		assert_eq!(static_mesh.joints, vec![0u16; 12]);
		assert_eq!(static_mesh.weights, vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);

		assert_eq!(material_description_list.len(), 1);
		assert_eq!(material_description_list[0].base_color_factor, [0.25, 0.5, 0.75]);
		assert_eq!(material_description_list[0].metallic_factor, 1.0);
		assert_eq!(material_description_list[0].roughness_factor, 0.5);

		assert_eq!(skinned_model.get_animation_name_list(), vec![String::from("A"), String::from("B")]);
		assert_eq!(skinned_model.get_animation_list()[0].duration, 2.0);
	}

	//==============================================================================
	/// 애니메이션 샘플링과 반복, 크로스페이드. (선형 이동, 루프, 페이드 중간 가중 0.5, 페이드 끝)
	//==============================================================================
	#[test]
	fn animation_sampling_and_crossfade() {
		let (mut skinned_model, _mesh_description_list, _material_description_list) = interpret_test_glb();

		// 클립 A 0.5초: (0, 1.5, 0). 2.5초는 반복해 0.5초와 같다.
		skinned_model.update(0.5);
		assert_near(skinned_model.get_node_list()[1].current_translation[1], 1.5);
		skinned_model.update(2.0);
		assert_near(skinned_model.get_node_list()[1].current_translation[1], 1.5);

		// 클립 B 로 1초 페이드. 0.5초 뒤: A(3초 → 반복 1초 = y 2) 와 B(4, 1, 0) 를 반씩.
		skinned_model.play_animation("B", 1.0);
		skinned_model.update(0.5);
		let translation = skinned_model.get_node_list()[1].current_translation;
		assert_near(translation[0], 2.0);
		assert_near(translation[1], 1.5);
		// 회전은 단위에서 Z 90도 쪽으로 반. (Z 45도)
		let rotation = skinned_model.get_node_list()[1].current_rotation;
		let expected_rotation = Quaternion::create_from_euler(0.0, 0.0, std::f64::consts::FRAC_PI_4);
		assert_near(rotation.z, expected_rotation.z);
		assert_near(rotation.w, expected_rotation.w);

		// 페이드가 끝나면 B 만.
		skinned_model.update(0.6);
		let final_translation = skinned_model.get_node_list()[1].current_translation;
		assert_near(final_translation[0], 4.0);
		assert_near(final_translation[1], 1.0);

		// 같은 클립을 다시 틀면 무시, 없는 클립도 무시.
		skinned_model.play_animation("B", 0.25);
		skinned_model.play_animation("none", 0.25);
		skinned_model.update(0.0);
		assert_near(skinned_model.get_node_list()[1].current_translation[0], 4.0);

		// 재생 속도 0 이면 시간이 멈춘다. (A 를 페이드 없이 0초에)
		skinned_model.play_animation("A", 0.0);
		skinned_model.set_time_scale(0.0);
		skinned_model.update(1.0);
		assert_near(skinned_model.get_node_list()[1].current_translation[1], 1.0);
	}

	//==============================================================================
	/// 월드 행렬과 조인트 행렬. (바인드 포즈면 조인트 행렬이 단위, 움직이면 월드 x 역바인드, 정적 메시 합성 스킨, 회전 오프셋)
	//==============================================================================
	#[test]
	fn joint_matrix_computation() {
		let (mut skinned_model, _mesh_description_list, _material_description_list) = interpret_test_glb();
		skinned_model.set_time_scale(0.0);
		skinned_model.update(0.0);
		let skin_list = skinned_model.get_skin_list();
		assert_eq!(&skin_list[0].joint_matrix_array[16..32], Matrix4::create_identity().get_elements());
		assert_eq!(&skin_list[1].joint_matrix_array[0..16], Matrix4::create_translation(5.0, 0.0, 0.0).get_elements());

		// 0.5초 (Joint 이동 y 1.5) → 조인트 행렬 = 이동(0, 1.5, 0) x 이동(0, -1, 0) = 이동(0, 0.5, 0).
		skinned_model.set_time_scale(1.0);
		skinned_model.update(0.5);
		let joint_matrix_array = &skinned_model.get_skin_list()[0].joint_matrix_array;
		assert_near((joint_matrix_array[16 + 13]) as f64, 0.5);
		assert_near((joint_matrix_array[16 + 12]) as f64, 0.0);

		// 회전 오프셋 (이름 부분 일치): Joint 를 X 90도 돌리면 월드 행렬의 Y 축이 +Z 로 간다.
		skinned_model.set_time_scale(0.0);
		skinned_model.set_joint_rotation_offset("Joi", Quaternion::create_from_euler(std::f64::consts::FRAC_PI_2, 0.0, 0.0));
		skinned_model.update(0.0);
		let world_elements = skinned_model.get_node_list()[1].world_matrix.elements;
		assert_near((world_elements[4]) as f64, 0.0);
		assert_near((world_elements[5]) as f64, 0.0);
		assert_near((world_elements[6]) as f64, 1.0);
		skinned_model.clear_joint_rotation_offsets();
		skinned_model.update(0.0);
		assert_near((skinned_model.get_node_list()[1].world_matrix.elements[5]) as f64, 1.0);
	}

	//==============================================================================
	/// 스무스 노멀과 와이어프레임 모서리, 모프 텍셀 배치. (GL 무관 부분)
	//==============================================================================
	#[test]
	fn generated_geometry_helpers() {
		let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
		let normals = SkinnedModel::generate_smooth_normals(&positions, None);
		assert_eq!(normals, vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);

		let quad_indices = IndexArray::UnsignedShort(vec![0, 1, 2, 2, 1, 3]);
		let edge_list = SkinnedModel::build_wireframe_edge_list(Some(&quad_indices), 4);
		assert_eq!(edge_list, vec![0, 1, 1, 2, 0, 2, 1, 3, 2, 3]);
		assert_eq!(SkinnedModel::build_wireframe_edge_list(None, 3), vec![0, 1, 1, 2, 2, 0]);

		let morph_target_list = vec![
			MorphTarget { name: String::from("first"), position_deltas: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], normal_deltas: Some(vec![0.5, 0.5, 0.5, 0.25, 0.25, 0.25]) },
			MorphTarget { name: String::from("second"), position_deltas: vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0], normal_deltas: None },
		];
		let (texture_width, rows_per_target, texel_data) = SkinnedModel::build_morph_texel_data(&morph_target_list, 2);
		assert_eq!((texture_width, rows_per_target), (2, 1));
		assert_eq!(texel_data.len(), 2 * 4 * 4);
		assert_eq!(&texel_data[0..8], &[1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0]);
		assert_eq!(&texel_data[8..12], &[0.5, 0.5, 0.5, 0.0]);
		assert_eq!(&texel_data[16..20], &[7.0, 8.0, 9.0, 0.0]);
		assert_eq!(&texel_data[24..32], &[0.0; 8]);
	}

	//==============================================================================
	/// FBX 오일러 변환. (X 먼저 적용, Rz x Ry x Rx)
	//==============================================================================
	#[test]
	fn fbx_euler_order_is_xfirst() {
		let rotation = create_quaternion_from_fbx_euler(90.0, 0.0, 90.0);
		let rotation_matrix = Matrix4::create_rotation_from_quaternion(rotation);
		// +Y 를 X 90도 → +Z, 다시 Z 90도 → +Z 그대로.
		let rotated_y = rotation_matrix.transform_direction(0.0, 1.0, 0.0);
		assert_near(rotated_y.x, 0.0);
		assert_near(rotated_y.y, 0.0);
		assert_near(rotated_y.z, 1.0);
		// +X 를 X 90도 → +X, 다시 Z 90도 → +Y.
		let rotated_x = rotation_matrix.transform_direction(1.0, 0.0, 0.0);
		assert_near(rotated_x.y, 1.0);
	}

	//==============================================================================
	/// 실제 GLB. (원본 예제 fox.glb, 조인트 24개, 클립 Survey/Walk/Run, 노멀 없음, 인덱스 없음, PNG 텍스처, 파일이 없으면 건너뛴다)
	//==============================================================================
	#[test]
	fn fox_glb_from_examples() {
		let fox_path = std::path::Path::new(EXAMPLE_ASSET_DIRECTORY).join("fox.glb");
		let glb_bytes = match std::fs::read(&fox_path) {
			Ok(glb_bytes) => {
				glb_bytes
			}
			Err(_) => {
				eprintln!("fox.glb 없음, 건너뜀");
				return;
			}
		};
		let (json, binary_buffer) = SkinnedModel::read_glb_chunks(&glb_bytes).expect("fox chunks");
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		let (mesh_description_list, material_description_list) = skinned_model.interpret_content(&json, &binary_buffer).expect("fox interpret");
		assert_eq!(skinned_model.get_animation_name_list(), vec![String::from("Survey"), String::from("Walk"), String::from("Run")]);
		assert_eq!(skinned_model.get_skin_list()[0].joint_node_indices.len(), 24);
		assert_eq!(mesh_description_list.len(), 1);
		let fox_mesh = &mesh_description_list[0];
		assert!(fox_mesh.normals.is_none() && fox_mesh.indices.is_none());
		assert_eq!(fox_mesh.positions.len() / 3, fox_mesh.weights.len() / 4);
		let normals = SkinnedModel::generate_smooth_normals(&fox_mesh.positions, None);
		let first_length = (normals[0] * normals[0] + normals[1] * normals[1] + normals[2] * normals[2]).sqrt();
		assert_near(first_length, 1.0);
		let base_color_image = material_description_list[0].base_color_image.as_ref().expect("fox texture");
		assert_eq!(base_color_image.mime_type.as_deref(), Some("image/png"));
		let (image_width, image_height, pixels) = crate::graphics3d::material::decode_image_bytes(&base_color_image.bytes).expect("fox png");
		assert_eq!(pixels.len(), (image_width * image_height * 4) as usize);

		// 걷기 클립 중간에서 조인트 행렬이 바인드 포즈와 달라야 한다.
		skinned_model.play_animation("Walk", 0.0);
		skinned_model.update(0.0);
		let rest_matrix_array = skinned_model.get_skin_list()[0].joint_matrix_array.clone();
		skinned_model.update(0.3);
		let walk_matrix_array = &skinned_model.get_skin_list()[0].joint_matrix_array;
		assert!(walk_matrix_array.iter().all(|value| value.is_finite()));
		assert!(rest_matrix_array.iter().zip(walk_matrix_array.iter()).any(|(rest_value, walk_value)| (rest_value - walk_value).abs() > 1.0e-3));
	}

	//==============================================================================
	/// 애니메이션 FBX 의 스택이 여럿이면 스택마다 "클립 이름/스택 이름" 클립이 된다. (하나면 받은 이름 그대로, 채널 없는 스택은 빠진다)
	//==============================================================================
	#[test]
	fn fbx_animation_stacks_become_named_clips() {
		let create_animation = |name: &str, channel_count: usize| {
			let channels = (0..channel_count).map(|_| crate::graphics3d::fbxloader::FbxAnimationChannel { node_index: 0, path: AnimationPath::Translation, times: vec![0.0, 1.0], values: vec![0.0; 6] }).collect();
			FbxAnimation { name: name.to_string(), duration: 1.0, channels }
		};
		let mut scene_data = FbxSceneData::default();
		scene_data.animation_list = vec![create_animation("Take 001", 1), create_animation("Empty", 0), create_animation("", 1)];
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		skinned_model.add_animations_from_fbx_scene_data(&scene_data, "Walk", false);
		assert_eq!(skinned_model.get_animation_name_list(), vec!["Walk/Take 001", "Walk/1"]);
		scene_data.animation_list.truncate(2);
		skinned_model.add_animations_from_fbx_scene_data(&scene_data, "Run", false);
		assert_eq!(skinned_model.get_animation_name_list(), vec!["Walk/Take 001", "Walk/1", "Run"]);
		assert_eq!(create_fbx_clip_name("", "Idle", 0, 2), "Idle");
		assert_eq!(create_fbx_clip_name("Jump", "Take", 0, 1), "Jump");
	}
	//==============================================================================
	/// 자산 캐시. (원본 tests/graphics3d/skinnedmodelasset.test.mjs 와 같은 값, 가짜 GL 로 GL 객체 수를 센다)
	/// - 같은 자산 두 번 읽기에 올리기 한 번, 인스턴스 포즈 따로, 머티리얼 갈라짐, 모프 타깃 갈라짐, 해제 차례, 캐시 비우기, 이름으로 넣고 찾기.
	//==============================================================================
	#[test]
	fn asset_cache_shares_resources_and_releases_by_reference_count() {
		use crate::core::imagetexturecache::fake_gl::{self, FakeGlObject};
		let gl = fake_gl::create_context();
		let shader_program = Rc::new(ShaderProgram::new(&gl, "", "").expect("shader program"));
		SkinnedModelAsset::clear_cache();

		// 삼각형 하나, 인덱스, 머티리얼 하나, 모프 타깃 하나, 클립 하나. (원본 시험 GLB 와 같다)
		let float_values: [f32; 26] = [
			0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0,
			0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
			0.0, 1.0,
			0.0, 0.0, 0.0, 0.0, 2.0, 0.0,
		];
		let mut binary_bytes: Vec<u8> = float_values.iter().flat_map(|value| value.to_le_bytes()).collect();
		for index in [0u16, 1, 2, 0] {
			binary_bytes.extend_from_slice(&index.to_le_bytes());
		}
		let json = json!({
			"asset": { "version": "2.0" },
			"scenes": [{ "nodes": [0] }],
			"nodes": [{ "name": "Triangle", "mesh": 0 }],
			"meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 4, "material": 0, "targets": [{ "POSITION": 1 }] }], "extras": { "targetNames": ["Lift"] } }],
			"materials": [{ "name": "Body", "pbrMetallicRoughness": { "baseColorFactor": [0.25, 0.5, 0.75, 1.0] } }],
			"accessors": [
				{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
				{ "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3" },
				{ "bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR" },
				{ "bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3" },
				{ "bufferView": 4, "componentType": 5123, "count": 3, "type": "SCALAR" }
			],
			"bufferViews": [
				{ "buffer": 0, "byteOffset": 0, "byteLength": 36 },
				{ "buffer": 0, "byteOffset": 36, "byteLength": 36 },
				{ "buffer": 0, "byteOffset": 72, "byteLength": 8 },
				{ "buffer": 0, "byteOffset": 80, "byteLength": 24 },
				{ "buffer": 0, "byteOffset": 104, "byteLength": 6 }
			],
			"buffers": [{ "byteLength": binary_bytes.len() }],
			"animations": [{ "name": "Rise", "samplers": [{ "input": 2, "output": 3 }], "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }] }]
		});
		let glb_bytes = wrap_glb(&json, &binary_bytes);
		let create_handle = |path: &str| SkinnedModelLoadHandle {
			load_handle: AssetLoadHandle::create_loaded(path, glb_bytes.clone()),
			file_format: SkinnedModelFileFormat::Glb,
		};
		let live_count = |object: FakeGlObject| fake_gl::get_live_count(object);

		// 1. 같은 자산을 다르게 적어 세 번 지으면 올리기 한 번.
		let mut first_model = create_handle("models/../models/triangle.glb").create_model_with_shader_program(&gl, shader_program.clone()).expect("finished").expect("first model");
		let mut second_model = create_handle("./models/triangle.glb").create_model_with_shader_program(&gl, shader_program.clone()).expect("finished").expect("second model");
		let mut third_model = create_handle("models\\triangle.glb").create_model_with_shader_program(&gl, shader_program.clone()).expect("finished").expect("third model");
		let asset = first_model.get_asset().expect("asset").clone();
		assert!(Rc::ptr_eq(&asset, second_model.get_asset().expect("asset")) && Rc::ptr_eq(&asset, third_model.get_asset().expect("asset")));
		assert_eq!(asset.get_key().as_deref(), Some("models/triangle.glb"));
		assert_eq!(asset.get_reference_count(), 3);
		assert_eq!(SkinnedModelAsset::get_cached_count(), 1);
		assert_eq!(live_count(FakeGlObject::VertexArray), 1);
		assert_eq!(live_count(FakeGlObject::Buffer), 5);
		assert_eq!(live_count(FakeGlObject::Texture), 17);
		// 위치 36 + 노멀 36 + 조인트 24 + 가중치 48 + 인덱스 6 + 모프 텍스처 3 x 2 x 16 + 머티리얼 텍스처 16 x 4
		assert_eq!(asset.get_gpu_byte_count(), 36 + 36 + 24 + 48 + 6 + 3 * 2 * 16 + 16 * 4);

		// 인스턴스는 포즈와 재생 상태가 따로다. (첫 클립을 돌리고 한 쪽만 움직인다)
		first_model.update(0.5);
		assert_eq!(first_model.get_node_list()[0].current_translation[1], 1.0);
		assert_eq!(second_model.get_node_list()[0].current_translation[1], 0.0);
		assert!(Rc::ptr_eq(&first_model.animation_list, &second_model.animation_list));

		// 2. 머티리얼 갈라짐. (텍스처는 새로 짓지 않고 값만 따로)
		let shared_material = second_model.get_shared_material(0).expect("shared material");
		assert!(Rc::ptr_eq(&shared_material, &asset.get_material_list()[0]));
		let instance_material = second_model.get_material(0).expect("instance material");
		assert!(!Rc::ptr_eq(&instance_material, &shared_material));
		assert!(Rc::ptr_eq(&second_model.get_material(0).expect("instance material"), &instance_material));
		instance_material.set_base_color_factor([1.0, 0.0, 0.0]);
		assert_eq!(shared_material.get_base_color_factor(), [0.25, 0.5, 0.75]);
		assert!(Rc::ptr_eq(&first_model.get_shared_material(0).expect("shared material"), &shared_material));
		assert_eq!(live_count(FakeGlObject::Texture), 17);
		assert_eq!(instance_material.get_texture_bindings()[0].2, shared_material.get_texture_bindings()[0].2);

		// 3. 모프 타깃 갈라짐. (더한 인스턴스만 제 텍스처를 짓는다)
		let shared_morph_texture = third_model.get_drawable_list()[0].morph_texture;
		third_model.add_morph_target(0, "Push", vec![0.0; 9], None).expect("morph target");
		third_model.upload_pending_morph_targets(&gl);
		assert_ne!(third_model.get_drawable_list()[0].morph_texture, shared_morph_texture);
		assert_eq!(live_count(FakeGlObject::Texture), 18);
		assert_eq!(first_model.get_drawable_list()[0].morph_target_list.len(), 1);
		assert_eq!(third_model.get_drawable_list()[0].morph_target_list.len(), 2);

		// 4. 해제 차례. (마지막 인스턴스가 지울 때까지 자산 자원은 남는다)
		third_model.destroy(&gl);
		assert_eq!(live_count(FakeGlObject::Texture), 17);
		assert_eq!(asset.get_reference_count(), 2);
		first_model.destroy(&gl);
		assert_eq!((live_count(FakeGlObject::VertexArray), live_count(FakeGlObject::Buffer), live_count(FakeGlObject::Texture)), (1, 5, 17));
		assert_eq!(asset.get_reference_count(), 1);
		assert!(!asset.is_destroyed());
		let mut fourth_model = second_model.create_instance().expect("fourth model");
		assert_eq!(asset.get_reference_count(), 2);
		assert_eq!(live_count(FakeGlObject::Buffer), 5);
		fourth_model.destroy(&gl);
		second_model.destroy(&gl);
		assert!(asset.is_destroyed());
		assert_eq!(asset.get_reference_count(), 0);
		assert_eq!((live_count(FakeGlObject::VertexArray), live_count(FakeGlObject::Buffer), live_count(FakeGlObject::Texture)), (0, 0, 0));
		assert_eq!(SkinnedModelAsset::get_cached_count(), 0);
		assert_eq!(asset.get_key(), None);
		assert!(SkinnedModel::create_from_asset(&asset).is_err());

		// 5. 다 지운 뒤 다시 지으면 새 자산. 캐시를 비우면 다음 받기는 새 자산, 쓰던 인스턴스는 그대로.
		let mut reloaded_model = create_handle("models/triangle.glb").create_model_with_shader_program(&gl, shader_program.clone()).expect("finished").expect("reloaded model");
		assert!(!Rc::ptr_eq(reloaded_model.get_asset().expect("asset"), &asset));
		SkinnedModelAsset::clear_cache();
		assert_eq!(SkinnedModelAsset::get_cached_count(), 0);
		assert_eq!(reloaded_model.get_asset().expect("asset").get_key(), None);
		let mut fresh_model = create_handle("models/triangle.glb").create_model_with_shader_program(&gl, shader_program.clone()).expect("finished").expect("fresh model");
		assert!(!Rc::ptr_eq(fresh_model.get_asset().expect("asset"), reloaded_model.get_asset().expect("asset")));
		assert!(!reloaded_model.get_asset().expect("asset").is_destroyed());
		reloaded_model.destroy(&gl);
		fresh_model.destroy(&gl);
		assert_eq!((live_count(FakeGlObject::VertexArray), live_count(FakeGlObject::Buffer), live_count(FakeGlObject::Texture)), (0, 0, 0));

		// 6. 이름으로 넣고 찾기. (바이트로 지은 모델)
		let mut bytes_model = SkinnedModel::load_from_bytes_with_shader_program(&gl, &glb_bytes, shader_program.clone()).expect("bytes model");
		assert_eq!(bytes_model.get_asset().expect("asset").get_key(), None);
		SkinnedModelAsset::register("Triangle", bytes_model.get_asset().expect("asset")).expect("register");
		let found_asset = SkinnedModelAsset::find("./Triangle", &shader_program).expect("found asset");
		assert!(Rc::ptr_eq(&found_asset, bytes_model.get_asset().expect("asset")));
		let other_shader_program = Rc::new(ShaderProgram::new(&gl, "", "").expect("shader program"));
		assert!(SkinnedModelAsset::find("Triangle", &other_shader_program).is_none());
		bytes_model.destroy(&gl);
		assert!(SkinnedModelAsset::find("Triangle", &shader_program).is_none());

		// 7. 키 정규화.
		assert_eq!(SkinnedModelAsset::normalize_key("./a/b/../c.glb"), "a/c.glb");
		assert_eq!(SkinnedModelAsset::normalize_key("a\\\\b//c.glb"), "a/b/c.glb");
		assert_eq!(SkinnedModelAsset::normalize_key("/a/./b.glb"), "/a/b.glb");
		assert_eq!(SkinnedModelAsset::normalize_key("../a.glb"), "../a.glb");
		assert_eq!(SkinnedModelAsset::normalize_key("https://example.com/a/./b/../c.glb?v=1#x"), "https://example.com/a/c.glb?v=1#x");
	}
}
