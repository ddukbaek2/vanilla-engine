//==============================================================================
//! 스킨드 모델. (원본 graphics3d/skinnedmodel.js)
//! - 모델 인스턴스, 메시/본/스킨/머티리얼/애니메이션 데이터 + 포즈 상태. 렌더링은 SkinnedModelRenderer 가 맡는다.
//! - GLB(바이너리 glTF)와 바이너리 FBX 를 공통 중립 스키마(MeshDescription + MaterialDescription)로 해석해 GL 리소스를 구성한다.
//!   (임포터는 파싱만 → 중립 스키마 → 공용 GL 업로드 upload_meshes_and_materials)
//! - 스킨 없는 정적 메시는 합성 스킨(조인트 1개)으로, 모프 타깃은 실수 텍스처로 올려 같은 스키닝 경로로 그린다.
//! - 애니메이션 크로스페이드와 조인트 회전 오프셋(절차 포즈 가공), 모프 가중치(glTF weights / 프로그램)를 원본처럼 지원한다.
//! - 원본과 다른 것:
//!   - 원본은 async 이고 URL 을 fetch 한다(loadFromUrl, 바이트는 loadFromBytes). 러스트 판의 바이트 함수
//!   ```text
//!     load_from_bytes / load_from_fbx_bytes / add_animations_from_fbx_bytes 는 그 자리에서 끝난다.
//!     URL 함수 load_from_url / load_from_fbx_url 은 ImageAsset 처럼 엔진의 애셋 받기(데스크탑은 애셋 폴더 후보, 웹은 asset_base_url 에서 fetch)로
//!     바이트를 부탁하고 받기 손잡이(SkinnedModelLoadHandle)를 돌려준다. 다 받으면 손잡이의 create_model(gl, 렌더러)이 모델을 짓는다. (await 없는 꼴)
//!     원본 await SkinnedModel.loadFromUrl(gl, url, renderer) 자리는 엔진 작업 안에서 load_from_url_async(엔진, 경로, 렌더러).await 다. (fbx 는 load_from_fbx_url_async)
//!     다 받으면 작업을 폴링하는 엔진의 GL 로 모델을 짓는다. (렌더러는 부를 때 셰이더 프로그램만 나눠 들어 빌리지 않는다)
//!     add_animations_from_fbx_url 은 받기를 부탁하고 await 할 수 있는 AnimationLoadHandle 을 돌려준다. 원본처럼 await 가 끝날 때는 클립이 이미 더해져 있다.
//!     (클립 목록을 손잡이와 나눠 들어, 다 받으면 await 하는 퓨처나 다음 update, play_animation 가운데 먼저 닿는 쪽이 더한다. 원본처럼 받은 차례로 더한다)
//!   ```
//!   - GL 을 쓰는 함수는 gl 을 받는다. update(delta_time) 는 GL 을 쓰지 않는다. 원본 update 6단계(바뀐 모프 타깃 업로드)는
//!   ```text
//!     렌더러의 draw / draw_depth / draw_wireframe 가 upload_pending_morph_targets 로 대신한다. (그래서 그리기 함수가 &mut 모델을 받는다)
//!     덕분에 씬의 tick 에서 update 를 부를 수 있다.
//!   ```
//!   - 원본 노드 / 드로어블 / 스킨 / 클립 객체는 SkinnedModelNode / Drawable / Skin / AnimationClip 구조체다.
//!   ```text
//!     드로어블의 머티리얼은 Rc<Material> 이다. 클립은 인덱스로 가리키고, sample_animation 도 클립 인덱스를 받는다.
//!     upload_wireframe_indices / upload_morph_targets 는 원본의 드로어블 객체 대신 드로어블 인덱스를 받는다. (모델을 &mut 로 빌린 채 드로어블을 넘길 수 없어서다)
//!   ```
//!   - 드로어블의 mesh_description 은 모프 타깃을 morph_target_list 로 옮긴 뒤라 morph_targets 가 비어 있다. (큰 델타 배열을 두 벌 들지 않는다)
//!   - 원본처럼 노드 matrix 와 CUBICSPLINE 보간, 프리미티브 mode 는 보지 않는다. FBX 는 원본처럼 스킨이 붙은 메시만 그린다.
//!   - destroy 가 지우는 머티리얼: 원본은 이 모델이 업로드에서 지은 것을 지운다. 러스트 판은 그 가운데 다른 곳에서 Rc 로 쥐지 않은 것만 지운다.
//!   ```text
//!     (참조 수를 셀 수 있어서다, 자바스크립트에는 참조 수가 없다)
//!   ```
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
/// 전역 상수 목록.
//==============================================================================
/// GLB 머리와 청크 타입.
const GLB_MAGIC: u32 = 0x46546c67;
const GLB_CHUNKTYPE_JSON: u32 = 0x4e4f534a;
const GLB_CHUNKTYPE_BINARY: u32 = 0x004e4942;

/// 접근자 컴포넌트 타입. (원본 COMPONENT_ARRAY_TABLE 의 키)
pub const COMPONENT_TYPE_UNSIGNED_BYTE: u32 = 5121;
pub const COMPONENT_TYPE_UNSIGNED_SHORT: u32 = 5123;
pub const COMPONENT_TYPE_UNSIGNED_INT: u32 = 5125;
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
	Translation,
	Rotation,
	Scale,
	Weights,
}

impl AnimationPath {
	//==============================================================================
	/// glTF 경로 문자열로부터. (모르는 경로면 None)
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
/// 모델 노드. (본 계층 + 포즈 상태, 원본 node_list 원소)
//==============================================================================
#[derive(Clone, Debug)]
pub struct SkinnedModelNode {
	pub name: String,
	pub child_indices: Vec<usize>,
	pub mesh_index: Option<usize>,
	pub skin_index: Option<usize>,
	pub base_translation: [f32; 3],
	pub base_rotation: Quaternion,
	pub base_scale: [f32; 3],
	pub current_translation: [f32; 3],
	pub current_rotation: Quaternion,
	pub current_scale: [f32; 3],
	pub world_matrix: Matrix4,
}

//==============================================================================
/// 스킨. (조인트 노드 인덱스 + 역바인드 행렬 + 셰이더로 올릴 조인트 행렬 배열)
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skin {
	pub joint_node_indices: Vec<usize>,
	pub inverse_bind_matrices: Vec<Matrix4>,
	pub joint_matrix_array: Vec<f32>,
}

//==============================================================================
/// 모프 타깃. (정점별 위치 델타 + 선택 노멀 델타)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct MorphTarget {
	pub name: String,
	pub position_deltas: Vec<f32>,
	pub normal_deltas: Option<Vec<f32>>,
}

//==============================================================================
/// 인덱스 배열. (원본 Uint16Array / Uint32Array)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum IndexArray {
	UnsignedShort(Vec<u16>),
	UnsignedInt(Vec<u32>),
}

impl IndexArray {
	//==============================================================================
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
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	//==============================================================================
	/// 인덱스 하나 반환.
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
/// 접근자 배열. (원본 read_accessor_array 가 돌려주는 타입 배열, 컴포넌트 타입 그대로)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum AccessorArray {
	UnsignedByte(Vec<u8>),
	UnsignedShort(Vec<u16>),
	UnsignedInt(Vec<u32>),
	Float(Vec<f32>),
}

impl AccessorArray {
	//==============================================================================
	/// 원소(컴포넌트) 개수.
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
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	//==============================================================================
	/// 컴포넌트 타입. (5121 / 5123 / 5125 / 5126)
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
	//==============================================================================
	pub fn to_float_list(&self) -> Vec<f32> {
		match self {
			AccessorArray::UnsignedByte(values) => {
				values.iter().map(|value| *value as f32).collect()
			}
			AccessorArray::UnsignedShort(values) => {
				values.iter().map(|value| *value as f32).collect()
			}
			AccessorArray::UnsignedInt(values) => {
				values.iter().map(|value| *value as f32).collect()
			}
			AccessorArray::Float(values) => {
				values.clone()
			}
		}
	}

	//==============================================================================
	/// 16비트 정수 목록으로. (원본 new Uint16Array(typedArray))
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
/// 중립 메시 서술. (원본 mesh_description, 순수 CPU 배열)
/// - joints / weights 는 정점마다 4개, material_index 가 None 이면 기본 머티리얼, node_index 는 모프 가중치 채널의 대상 노드.
//==============================================================================
#[derive(Clone, Debug)]
pub struct MeshDescription {
	pub positions: Vec<f32>,
	pub normals: Option<Vec<f32>>,
	pub texture_coordinates: Option<Vec<f32>>,
	pub joints: Vec<u16>,
	pub weights: Vec<f32>,
	pub indices: Option<IndexArray>,
	pub skin_index: usize,
	pub material_index: Option<usize>,
	pub material_name: String,
	pub node_index: Option<usize>,
	pub morph_targets: Vec<MorphTarget>,
	pub morph_weights: Vec<f32>,
}

//==============================================================================
/// 드로어블. (원본 drawable_list 원소, 버텍스 어레이, 인덱스, 스킨, 머티리얼 참조, 모프 상태)
//==============================================================================
pub struct Drawable {
	pub vertex_array: GlVertexArray,
	pub vertex_buffer_list: Vec<GlBuffer>,
	pub is_indexed: bool,
	pub index_buffer: Option<GlBuffer>,
	pub index_count: usize,
	pub index_component_type: u32,
	pub index_byte_offset: i32,
	pub wireframe_index_buffer: Option<GlBuffer>,
	pub wireframe_index_count: usize,
	pub wireframe_index_component_type: u32,
	pub vertex_count: usize,
	pub material: Rc<Material>,
	pub material_name: String,
	pub skin_index: usize,
	pub node_index: Option<usize>,
	pub mesh_description: MeshDescription,
	pub morph_target_list: Vec<MorphTarget>,
	pub morph_weights: Vec<f32>,
	pub morph_texture: Option<GlTexture>,
	pub morph_rows_per_target: i32,
	pub morph_texture_width: i32,
	pub is_morph_dirty: bool,
}

//==============================================================================
/// 애니메이션 채널. (노드 인덱스 + 경로 + 시간/값 커브 + 키프레임 커서)
//==============================================================================
#[derive(Clone, Debug)]
pub struct AnimationChannel {
	pub node_index: usize,
	pub path: AnimationPath,
	pub times: Vec<f32>,
	pub values: Vec<f32>,
	pub cursor: usize,
}

//==============================================================================
/// 애니메이션 클립.
//==============================================================================
#[derive(Clone, Debug)]
pub struct AnimationClip {
	pub name: String,
	pub duration: f32,
	pub channels: Vec<AnimationChannel>,
}

//==============================================================================
/// 스킨드 모델.
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
	current_time: f32,
	previous_time: f32,
	fade_duration: f32,
	fade_elapsed: f32,
	time_scale: f32,
	joint_rotation_offsets: Vec<(usize, Quaternion)>,
	/// 받는 중인 애니메이션 FBX. (add_animations_from_fbx_url)
	pending_animation_load_list: Vec<AnimationLoadHandle>,
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
				animation_list.append(&mut added_clip_list);
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
//==============================================================================
#[derive(Clone)]
pub struct AnimationLoadHandle {
	animation_load: Rc<AnimationLoad>,
}

impl AnimationLoadHandle {
	//==============================================================================
	/// 바이트 읽기 손잡이. (씬에 맡길 때)
	//==============================================================================
	pub fn get_load_handle(&self) -> &AssetLoadHandle {
		self.animation_load.fbx_load_handle.get_load_handle()
	}

	//==============================================================================
	/// 끝났는지. (클립을 더했거나 실패했거나)
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
/// FBX 의 애니메이션들을 모델의 클립으로 바꾸기. (노드는 이름으로 맞추고, 이동 무시면 이동 채널을 뺀다. 채널이 없는 것은 뺀다)
//==============================================================================
fn convert_fbx_scene_animations(node_index_by_name: &HashMap<String, usize>, scene_data: &FbxSceneData, clip_name: &str, ignore_translation: bool) -> Vec<AnimationClip> {
	let mut added_clip_list = Vec::new();
	for fbx_animation in scene_data.animation_list.iter() {
		if fbx_animation.channels.is_empty() {
			continue;
		}
		let mut channels = convert_fbx_animation_channels(scene_data, fbx_animation, |node_index| {
			let source_name = scene_data.node_list.get(node_index)?.name.as_str();
			node_index_by_name.get(source_name).copied()
		});
		if ignore_translation {
			channels.retain(|channel| channel.path != AnimationPath::Translation);
		}
		added_clip_list.push(AnimationClip {
			name: clip_name.to_string(),
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

//==============================================================================
/// 모델 받기 손잡이. (SkinnedModel::load_from_url, load_from_fbx_url 이 돌려준다. 원본 loadFromUrl, loadFromFbxUrl 의 Promise 자리)
/// - 씬은 get_load_handle 을 SceneBase::track_load_handle 로 맡겨 다 받을 때까지 initialize 를 미룰 수 있다.
/// - 모델은 GL 이 있는 자리에서 create_model(gl, 렌더러)로 짓는다.
//==============================================================================
#[derive(Clone, Debug)]
pub struct SkinnedModelLoadHandle {
	load_handle: AssetLoadHandle,
	file_format: SkinnedModelFileFormat,
}

impl SkinnedModelLoadHandle {
	//==============================================================================
	/// 바이트 읽기 손잡이. (씬에 맡길 때)
	//==============================================================================
	pub fn get_load_handle(&self) -> &AssetLoadHandle {
		&self.load_handle
	}

	//==============================================================================
	/// 끝났는지. (받았거나 실패했거나)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.load_handle.is_finished()
	}

	//==============================================================================
	/// 받은 바이트로 모델 짓기. (받는 중이면 None, 받기나 해석에 실패하면 Err. 원본 문구 "SkinnedModel load failed: url")
	//==============================================================================
	pub fn create_model(&self, gl: &glow::Context, skinned_model_renderer: &SkinnedModelRenderer) -> Option<Result<SkinnedModel, String>> {
		if !self.load_handle.is_finished() {
			return None;
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
		match self.file_format {
			SkinnedModelFileFormat::Glb => {
				Some(SkinnedModel::load_from_bytes(gl, &bytes, skinned_model_renderer))
			}
			SkinnedModelFileFormat::Fbx => {
				Some(SkinnedModel::load_from_fbx_bytes(gl, &bytes, skinned_model_renderer))
			}
		}
	}

	//==============================================================================
	/// 다 받으면 모델을 짓는 퓨처. (load_from_url_async, load_from_fbx_url_async 가 돌려준다. GL 은 작업을 폴링하는 엔진의 것을 쓴다)
	//==============================================================================
	fn create_model_when_loaded(self, shader_program: Rc<ShaderProgram>) -> impl Future<Output = Result<SkinnedModel, String>> + 'static {
		async move {
			let bytes = match (&self.load_handle).await {
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
			let model_result = task::with_polling_graphic(|graphic| {
				let gl = graphic.get_gl();
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
					model_result
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
	//==============================================================================
	pub fn new(shader_program: Rc<ShaderProgram>) -> SkinnedModel {
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		skinned_model.shader_program = Some(shader_program);
		skinned_model
	}

	//==============================================================================
	/// 셰이더 없이 생성. (해석과 포즈 계산만 하는 모델, upload_meshes_and_materials 는 오류)
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
		}
	}

	//==============================================================================
	/// 바이트로부터 로드. (GLB 바이너리 glTF, 원본 정적 loadFromUrl 의 fetch 뒤 부분)
	//==============================================================================
	pub fn load_from_bytes(gl: &glow::Context, bytes: &[u8], skinned_model_renderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
		SkinnedModel::load_from_bytes_with_shader_program(gl, bytes, skinned_model_renderer.get_shader_program())
	}

	//==============================================================================
	/// 바이트로부터 로드. (바이너리 FBX, 원본 정적 loadFromFbxUrl 의 fetch 뒤 부분)
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
	//==============================================================================
	pub fn load_from_url(engine: &Engine, url: &str) -> SkinnedModelLoadHandle {
		SkinnedModelLoadHandle {
			load_handle: engine.request_asset_bytes(url),
			file_format: SkinnedModelFileFormat::Glb,
		}
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 받기 부탁. (바이너리 FBX, 원본 정적 loadFromFbxUrl)
	//==============================================================================
	pub fn load_from_fbx_url(engine: &Engine, url: &str) -> SkinnedModelLoadHandle {
		SkinnedModelLoadHandle {
			load_handle: engine.request_asset_bytes(url),
			file_format: SkinnedModelFileFormat::Fbx,
		}
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 모델 받기. (GLB, 원본 await SkinnedModel.loadFromUrl(gl, url, renderer), 엔진 작업 안에서 await 한다)
	/// - 다 받으면 작업을 폴링하는 엔진의 GL 로 짓는다. 렌더러는 지금 셰이더 프로그램만 나눠 들어, 퓨처가 렌더러를 빌리지 않는다.
	//==============================================================================
	pub fn load_from_url_async(engine: &Engine, url: &str, skinned_model_renderer: &SkinnedModelRenderer) -> impl Future<Output = Result<SkinnedModel, String>> + 'static {
		SkinnedModel::load_from_url(engine, url).create_model_when_loaded(skinned_model_renderer.get_shader_program())
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 모델 받기. (바이너리 FBX, 원본 await SkinnedModel.loadFromFbxUrl(gl, url, renderer))
	//==============================================================================
	pub fn load_from_fbx_url_async(engine: &Engine, url: &str, skinned_model_renderer: &SkinnedModelRenderer) -> impl Future<Output = Result<SkinnedModel, String>> + 'static {
		SkinnedModel::load_from_fbx_url(engine, url).create_model_when_loaded(skinned_model_renderer.get_shader_program())
	}

	//==============================================================================
	/// GLB 청크 읽기. (머리 확인 + JSON 청크 + BIN 청크, GL 무관)
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
	//==============================================================================
	pub fn parse_binary(&mut self, gl: &glow::Context, bytes: &[u8]) -> Result<(), String> {
		let (json, binary_buffer) = SkinnedModel::read_glb_chunks(bytes)?;
		self.parse_content(gl, &json, &binary_buffer)
	}

	//==============================================================================
	/// glTF 내용 해석. (노드 / 스킨 / 메시 / 애니메이션 / 텍스처 → GL 업로드)
	//==============================================================================
	pub fn parse_content(&mut self, gl: &glow::Context, json: &Value, binary_buffer: &[u8]) -> Result<(), String> {
		let (mesh_description_list, material_description_list) = self.interpret_content(json, binary_buffer)?;
		self.upload_meshes_and_materials(gl, mesh_description_list, material_description_list)
	}

	//==============================================================================
	/// glTF 내용 해석. (GL 무관, 노드 / 스킨 / 애니메이션을 채우고 중립 메시 / 머티리얼 서술을 돌려준다)
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
						matrix.elements.copy_from_slice(matrix_values);
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
					description.metallic_factor = pbr_metallic_roughness["metallicFactor"].as_f64().map(|value| value as f32).unwrap_or(1.0);
					description.roughness_factor = pbr_metallic_roughness["roughnessFactor"].as_f64().map(|value| value as f32).unwrap_or(1.0);
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
					weights = vec![0.0f32; vertex_count * 4];
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
					*morph_weight = base_morph_weights.get(weight_index).copied().unwrap_or(0.0);
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
				let mut duration = 0.0f32;
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
		*self.animation_list.borrow_mut() = animation_list;
		if has_animation {
			self.current_animation_index = Some(0);
		}
		Ok((mesh_description_list, material_description_list))
	}

	//==============================================================================
	/// FBX 중간 표현 해석. (노드/스킨/메시/텍스처/애니메이션 → 기존 스키닝 파이프라인)
	//==============================================================================
	pub fn parse_fbx_content(&mut self, gl: &glow::Context, scene_data: &FbxSceneData) -> Result<(), String> {
		let (mesh_description_list, material_description_list) = self.interpret_fbx_content(scene_data)?;
		self.upload_meshes_and_materials(gl, mesh_description_list, material_description_list)
	}

	//==============================================================================
	/// FBX 중간 표현 해석. (GL 무관, 노드 / 스킨 / 애니메이션을 채우고 중립 메시 / 머티리얼 서술을 돌려준다)
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
				base_translation: [fbx_node.translation[0] as f32, fbx_node.translation[1] as f32, fbx_node.translation[2] as f32],
				base_rotation,
				base_scale: [fbx_node.scale[0] as f32, fbx_node.scale[1] as f32, fbx_node.scale[2] as f32],
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
			description.base_color_factor = [fbx_material.diffuse_color[0] as f32, fbx_material.diffuse_color[1] as f32, fbx_material.diffuse_color[2] as f32];
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
			let mut vertex_weight_array = vec![0.0f32; original_vertex_count * 4];
			for (vertex_index, influence_list) in influences_per_vertex.iter_mut().enumerate() {
				influence_list.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(std::cmp::Ordering::Equal));
				let used_count = influence_list.len().min(4);
				let mut weight_sum: f64 = influence_list.iter().take(used_count).map(|influence| influence.1).sum();
				if weight_sum <= 0.0 {
					weight_sum = 1.0;
				}
				for slot_index in 0..used_count {
					vertex_joint_array[vertex_index * 4 + slot_index] = influence_list[slot_index].0;
					vertex_weight_array[vertex_index * 4 + slot_index] = (influence_list[slot_index].1 / weight_sum) as f32;
				}
			}

			// 코너 단위(비인덱스)로 조인트/가중치 펼침.
			let corner_count = geometry.corner_count;
			let mut corner_joint_array = vec![0u16; corner_count * 4];
			let mut corner_weight_array = vec![0.0f32; corner_count * 4];
			for corner_index in 0..corner_count {
				let original_vertex_index = geometry.original_vertex_indices[corner_index];
				if original_vertex_index < 0 || original_vertex_index as usize >= original_vertex_count {
					continue;
				}
				let source_offset = original_vertex_index as usize * 4;
				corner_joint_array[corner_index * 4..corner_index * 4 + 4].copy_from_slice(&vertex_joint_array[source_offset..source_offset + 4]);
				corner_weight_array[corner_index * 4..corner_index * 4 + 4].copy_from_slice(&vertex_weight_array[source_offset..source_offset + 4]);
			}

			// 머티리얼 인덱스 조회. (지오메트리 소유 노드의 첫 머티리얼)
			let owner_node_index = scene_data.node_list.iter().position(|fbx_node| fbx_node.geometry_id.is_some() && fbx_node.geometry_id == skin_deformer.geometry_id);
			let material_index = owner_node_index.and_then(|owner_node_index| scene_data.node_list[owner_node_index].material_ids.first().copied()).and_then(|material_id| scene_data.material_by_id.index_of(material_id));

			mesh_description_list.push(MeshDescription {
				positions: geometry.positions.clone(),
				normals: Some(geometry.normals.clone()),
				texture_coordinates: Some(geometry.texture_coordinates.clone()),
				joints: corner_joint_array,
				weights: corner_weight_array,
				indices: None,
				skin_index: skin_list_index,
				material_index,
				material_name: String::new(),
				node_index: owner_node_index,
				morph_targets: Vec::new(),
				morph_weights: Vec::new(),
			});
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
		*self.animation_list.borrow_mut() = animation_list;
		if has_animation {
			self.current_animation_index = Some(0);
		}
		Ok((mesh_description_list, material_description_list))
	}

	//==============================================================================
	/// 애니메이션 FBX 추가. (본 이름 매칭 리타게팅, Mixamo 애니메이션 전용 FBX 의 중간 표현)
	/// - 동일 스켈레톤을 전제로, 채널을 현재 모델의 노드 인덱스로 재지정해 클립을 추가한다.
	/// - ignore_translation: 루트 모션이 박힌 클립에서 이동 채널을 버린다. (게임 로직이 위치를 제어할 때)
	//==============================================================================
	pub fn add_animations_from_fbx_scene_data(&mut self, scene_data: &FbxSceneData, clip_name: &str, ignore_translation: bool) {
		let mut added_clip_list = convert_fbx_scene_animations(&self.create_node_index_by_name(), scene_data, clip_name, ignore_translation);
		self.animation_list.borrow_mut().append(&mut added_clip_list);
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
		let mut material_list = Vec::with_capacity(material_description_list.len());
		for material_description in material_description_list.iter() {
			let material = Material::create_from_description(gl, shader_program.clone(), material_description)?;
			material_list.push(Rc::new(material));
		}
		let default_description = Material::create_default_description();
		let default_material = Rc::new(Material::create_from_description(gl, shader_program.clone(), &default_description)?);

		self.drawable_list = Vec::with_capacity(mesh_description_list.len());
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
				gl.create_vertex_array()?
			};
			let mut vertex_buffer_list = Vec::new();
			unsafe {
				gl.bind_vertex_array(Some(vertex_array));
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexPosition", &mesh_description.positions, 3)? {
				vertex_buffer_list.push(vertex_buffer);
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexNormal", normals, 3)? {
				vertex_buffer_list.push(vertex_buffer);
			}
			if let Some(texture_coordinates) = &mesh_description.texture_coordinates {
				if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexTextureCoordinate", texture_coordinates, 2)? {
					vertex_buffer_list.push(vertex_buffer);
				}
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_integer_attribute(gl, &shader_program, "vertexJoints", &mesh_description.joints, 4)? {
				vertex_buffer_list.push(vertex_buffer);
			}
			if let Some(vertex_buffer) = SkinnedModel::bind_float_attribute(gl, &shader_program, "vertexWeights", &mesh_description.weights, 4)? {
				vertex_buffer_list.push(vertex_buffer);
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
			}
			unsafe {
				gl.bind_vertex_array(None);
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
			self.drawable_list.push(Drawable {
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
				mesh_description,
				morph_target_list,
				morph_weights,
				morph_texture: None,
				morph_rows_per_target: 0,
				morph_texture_width: 0,
				is_morph_dirty,
			});
		}
		Ok(())
	}

	//==============================================================================
	/// 와이어프레임 인덱스 업로드. (삼각형 인덱스 → 중복 없는 모서리 선 인덱스, 비인덱스 메시는 삼각형별 3변)
	/// - 드로어블의 버텍스 어레이와 무관한 별도 엘리먼트 버퍼. 렌더러가 선 출력 시 잠시 바꿔 끼운다.
	//==============================================================================
	pub fn upload_wireframe_indices(&mut self, gl: &glow::Context, drawable_index: usize) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
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
			gl.bind_vertex_array(None);
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
	/// 와이어프레임 모서리 목록 만들기. (GL 무관, 인덱스 쌍 목록)
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
	//==============================================================================
	pub fn upload_morph_targets(&mut self, gl: &glow::Context, drawable_index: usize) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		let target_count = drawable.morph_target_list.len();
		let vertex_count = drawable.vertex_count;
		if target_count == 0 || vertex_count == 0 {
			if let Some(morph_texture) = drawable.morph_texture.take() {
				unsafe {
					gl.delete_texture(morph_texture);
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
					gl.create_texture()?
				}
			};
			drawable.morph_texture = Some(morph_texture);
			gl.bind_texture(glow::TEXTURE_2D, Some(morph_texture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, texture_width as i32, texture_height as i32, 0, glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(Some(slice_to_bytes(&texel_data))));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.bind_texture(glow::TEXTURE_2D, None);
		}
		drawable.morph_rows_per_target = rows_per_target as i32;
		drawable.morph_texture_width = texture_width as i32;
		drawable.is_morph_dirty = false;
		Ok(())
	}

	//==============================================================================
	/// 모프 텍셀 데이터 만들기. (GL 무관, 텍스처 가로, 타깃당 행 수, RGBA 실수 배열)
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
					texel_data[position_texel_offset + component_index] = morph_target.position_deltas.get(vertex_index * 3 + component_index).copied().unwrap_or(0.0);
				}
				if let Some(normal_deltas) = &morph_target.normal_deltas {
					let normal_texel_offset = (normal_block_offset + vertex_index) * 4;
					for component_index in 0..3 {
						texel_data[normal_texel_offset + component_index] = normal_deltas.get(vertex_index * 3 + component_index).copied().unwrap_or(0.0);
					}
				}
			}
		}
		(texture_width, rows_per_target, texel_data)
	}

	//==============================================================================
	/// 바뀐 모프 타깃 업로드. (원본 update 6단계, 추가/변경된 드로어블만, 렌더러가 그리기 전에 부른다)
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
	//==============================================================================
	pub fn add_morph_target(&mut self, drawable_index: usize, target_name: &str, position_deltas: Vec<f32>, normal_deltas: Option<Vec<f32>>) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		if drawable.morph_target_list.len() >= MORPH_TARGET_MAXIMUM {
			return Err(format!("SkinnedModel: morph target limit {} exceeded.", MORPH_TARGET_MAXIMUM));
		}
		if position_deltas.len() != drawable.vertex_count * 3 {
			return Err(format!("SkinnedModel: morph target \"{}\" delta count mismatch.", target_name));
		}
		drawable.morph_target_list.push(MorphTarget { name: target_name.to_string(), position_deltas, normal_deltas });
		drawable.morph_weights.resize(drawable.morph_target_list.len(), 0.0);
		drawable.is_morph_dirty = true;
		Ok(())
	}

	//==============================================================================
	/// 모프 가중치 설정. (이름 일치 타깃 전부, 드로어블 여러 개가 같은 이름을 가질 수 있음)
	//==============================================================================
	pub fn set_morph_weight(&mut self, target_name: &str, weight: f32) {
		for drawable in self.drawable_list.iter_mut() {
			for target_index in 0..drawable.morph_target_list.len() {
				if drawable.morph_target_list[target_index].name == target_name {
					drawable.morph_weights[target_index] = weight;
				}
			}
		}
	}

	//==============================================================================
	/// 모프 가중치 반환. (첫 일치 타깃, 없으면 0)
	//==============================================================================
	pub fn get_morph_weight(&self, target_name: &str) -> f32 {
		for drawable in self.drawable_list.iter() {
			for target_index in 0..drawable.morph_target_list.len() {
				if drawable.morph_target_list[target_index].name == target_name {
					return drawable.morph_weights[target_index];
				}
			}
		}
		0.0
	}

	//==============================================================================
	/// 머티리얼 교체. (드로어블 단위, 외부 텍스처로 만든 머티리얼 인스턴스 적용)
	//==============================================================================
	pub fn set_material(&mut self, drawable_index: usize, material: Rc<Material>) -> Result<(), String> {
		let drawable = self.drawable_list.get_mut(drawable_index).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawable_index))?;
		drawable.material = material;
		Ok(())
	}

	//==============================================================================
	/// 스무스 노멀 생성. (노멀 미보유 메시, 바인드 포즈 기준, 스키닝 회전은 셰이더에서 적용)
	//==============================================================================
	pub fn generate_smooth_normals(positions: &[f32], indices: Option<&IndexArray>) -> Vec<f32> {
		let mut normals = vec![0.0f32; positions.len()];
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
	//==============================================================================
	pub fn bind_float_attribute(gl: &glow::Context, shader_program: &ShaderProgram, attribute_name: &str, data_array: &[f32], component_count: i32) -> Result<Option<GlBuffer>, String> {
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
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, slice_to_bytes(data_array), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(attribute_location);
			gl.vertex_attrib_pointer_f32(attribute_location, component_count, glow::FLOAT, false, 0, 0);
			Ok(Some(vertex_buffer))
		}
	}

	//==============================================================================
	/// 정수형 어트리뷰트 바인드. (조인트 인덱스, 셰이더가 안 쓰는 어트리뷰트면 None)
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
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, slice_to_bytes(data_array), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(attribute_location);
			gl.vertex_attrib_pointer_i32(attribute_location, component_count, glow::UNSIGNED_SHORT, 0, 0);
			Ok(Some(vertex_buffer))
		}
	}

	//==============================================================================
	/// 애니메이션 재생. (크로스페이드, 같은 이름이 여럿이면 원본처럼 마지막 것, 원본 fade_duration 기본값 0.25. 다 받은 애니메이션 FBX 의 클립을 먼저 더한다)
	//==============================================================================
	pub fn play_animation(&mut self, animation_name: &str, fade_duration: f32) {
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
	//==============================================================================
	pub fn sample_animation(&mut self, animation_index: usize, time: f32, blend_weight: f32) {
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
	//==============================================================================
	pub fn update(&mut self, delta_time: f32) {
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
	//==============================================================================
	pub fn get_animation_name_list(&self) -> Vec<String> {
		self.animation_list.borrow().iter().map(|animation| animation.name.clone()).collect()
	}

	//==============================================================================
	/// 재생 속도 설정.
	//==============================================================================
	pub fn set_time_scale(&mut self, time_scale: f32) {
		self.time_scale = time_scale;
	}

	//==============================================================================
	/// 셰이더 프로그램 반환. (셰이더 없이 만든 모델이면 None)
	//==============================================================================
	pub fn get_shader_program(&self) -> Option<&Rc<ShaderProgram>> {
		self.shader_program.as_ref()
	}

	//==============================================================================
	/// 노드 목록 반환.
	//==============================================================================
	pub fn get_node_list(&self) -> &[SkinnedModelNode] {
		&self.node_list
	}

	//==============================================================================
	/// 노드 목록 반환. (변경용, 원본은 살아 있는 배열을 내준다)
	//==============================================================================
	pub fn get_node_list_mut(&mut self) -> &mut [SkinnedModelNode] {
		&mut self.node_list
	}

	//==============================================================================
	/// 루트 노드 인덱스 목록 반환.
	//==============================================================================
	pub fn get_root_node_indices(&self) -> &[usize] {
		&self.root_node_indices
	}

	//==============================================================================
	/// 스킨 목록 반환.
	//==============================================================================
	pub fn get_skin_list(&self) -> &[Skin] {
		&self.skin_list
	}

	//==============================================================================
	/// 드로어블 목록 반환.
	//==============================================================================
	pub fn get_drawable_list(&self) -> &[Drawable] {
		&self.drawable_list
	}

	//==============================================================================
	/// 드로어블 목록 반환. (변경용)
	//==============================================================================
	pub fn get_drawable_list_mut(&mut self) -> &mut [Drawable] {
		&mut self.drawable_list
	}

	//==============================================================================
	/// 애니메이션 클립 목록 반환. (길이, 채널 보기. 애니메이션 FBX 받기 손잡이와 나눠 드는 목록이라 빌린 값으로 준다)
	//==============================================================================
	pub fn get_animation_list(&self) -> std::cell::Ref<'_, [AnimationClip]> {
		std::cell::Ref::map(self.animation_list.borrow(), |animation_list| animation_list.as_slice())
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제, 다른 곳에서도 쓰는 머티리얼은 남긴다)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		let mut destroyed_material_list: Vec<*const Material> = Vec::new();
		for drawable in self.drawable_list.drain(..) {
			unsafe {
				gl.delete_vertex_array(drawable.vertex_array);
				for vertex_buffer in drawable.vertex_buffer_list.iter() {
					gl.delete_buffer(*vertex_buffer);
				}
				if let Some(index_buffer) = drawable.index_buffer {
					gl.delete_buffer(index_buffer);
				}
				if let Some(wireframe_index_buffer) = drawable.wireframe_index_buffer {
					gl.delete_buffer(wireframe_index_buffer);
				}
				if let Some(morph_texture) = drawable.morph_texture {
					gl.delete_texture(morph_texture);
				}
			}
			let material_pointer = Rc::as_ptr(&drawable.material);
			if destroyed_material_list.contains(&material_pointer) {
				continue;
			}
			destroyed_material_list.push(material_pointer);
			if Rc::strong_count(&drawable.material) == 1 {
				drawable.material.destroy(gl);
			}
		}
	}
}

//==============================================================================
/// 애니메이션 클립 샘플링. (현재 노드 포즈에 가중 블렌딩, 원본 sample_animation 본문)
//==============================================================================
fn sample_animation_clip(animation: &mut AnimationClip, node_list: &mut [SkinnedModelNode], drawable_list: &mut [Drawable], time: f32, blend_weight: f32) {
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
			((looped_time - times[frame_index]) / span_duration).clamp(0.0, 1.0)
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
						let current_weight = drawable.morph_weights[target_index];
						drawable.morph_weights[target_index] = current_weight + (sampled_value - current_weight) * blend_weight;
					}
				}
			}
		}
	}
}

//==============================================================================
/// FBX 오일러(도) → 쿼터니언. (eEulerXYZ = X 먼저 적용, 열벡터 기준 Rz x Ry x Rx 합성)
//==============================================================================
pub fn create_quaternion_from_fbx_euler(x_degree: f64, y_degree: f64, z_degree: f64) -> Quaternion {
	let degree_to_radian = std::f64::consts::PI / 180.0;
	let rotation_x = Quaternion::create_from_euler((x_degree * degree_to_radian) as f32, 0.0, 0.0);
	let rotation_y = Quaternion::create_from_euler(0.0, (y_degree * degree_to_radian) as f32, 0.0);
	let rotation_z = Quaternion::create_from_euler(0.0, 0.0, (z_degree * degree_to_radian) as f32);
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
			let mut quaternion_values = vec![0.0f32; key_count * 4];
			for key_index in 0..key_count {
				let read_value = |component_index: usize| fbx_channel.values.get(key_index * 3 + component_index).copied().unwrap_or(0.0) as f64;
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
fn read_float_array(value: &Value) -> Option<Vec<f32>> {
	value.as_array().map(|number_values| number_values.iter().map(|number_value| number_value.as_f64().unwrap_or(0.0) as f32).collect())
}

//==============================================================================
/// JSON 3성분 벡터 읽기. (없거나 짧으면 기본값)
//==============================================================================
fn read_vector3(value: &Value, default_value: [f32; 3]) -> [f32; 3] {
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

//==============================================================================
/// 숫자 배열을 바이트 배열로 본다. (GL 버퍼 업로드용, 리틀 엔디언 대상)
//==============================================================================
fn slice_to_bytes<T: Copy>(values: &[T]) -> &[u8] {
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
		let half_square_root = std::f32::consts::FRAC_1_SQRT_2;
		let rotation_bview = buffer_builder.add_buffer_view(&float_bytes(&[0.0, 0.0, half_square_root, half_square_root, 0.0, 0.0, half_square_root, half_square_root]), 0);
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
	fn assert_near(actual: f32, expected: f32) {
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
		let expected_rotation = Quaternion::create_from_euler(0.0, 0.0, std::f32::consts::FRAC_PI_4);
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
		assert_near(joint_matrix_array[16 + 13], 0.5);
		assert_near(joint_matrix_array[16 + 12], 0.0);

		// 회전 오프셋 (이름 부분 일치): Joint 를 X 90도 돌리면 월드 행렬의 Y 축이 +Z 로 간다.
		skinned_model.set_time_scale(0.0);
		skinned_model.set_joint_rotation_offset("Joi", Quaternion::create_from_euler(std::f32::consts::FRAC_PI_2, 0.0, 0.0));
		skinned_model.update(0.0);
		let world_elements = skinned_model.get_node_list()[1].world_matrix.elements;
		assert_near(world_elements[4], 0.0);
		assert_near(world_elements[5], 0.0);
		assert_near(world_elements[6], 1.0);
		skinned_model.clear_joint_rotation_offsets();
		skinned_model.update(0.0);
		assert_near(skinned_model.get_node_list()[1].world_matrix.elements[5], 1.0);
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
}
