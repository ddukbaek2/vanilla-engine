//==============================================================================
//! 점토 모형(GLB) 읽기와 뼈대 자세. (vanilla.js graphics3d/toon/glbmeshreader.js)
//! - 재질 색으로 칠하는 모형을 툰 3D 의 MeshBuilder 꼭짓점 배열로 읽는다. 텍스처 좌표와 재질 텍스처가 있으면 그것도 읽는다. (외계행성 광부 키우기에서 왔다)
//!   - 부품 하나가 노드 하나이고, 노드의 메시는 색마다 프리미티브 하나다. 꼭짓점은 위치와 노멀, 뼈대가 있으면 본 번호와 무게다.
//!     노멀이 없는 프리미티브는 glTF 명세대로 평평한 노멀(삼각형마다 하나)을 짓는다. (꼭짓점을 삼각형마다 따로 푼다)
//!   - 재질의 baseColorFactor 는 선형 색이라 sRGB 로 바꿔 담고, emissiveFactor 가 있으면 빛남으로 담는다. (emissiveTexture 가 있으면 빛남은 재질 범위로만 넘긴다)
//!   - TEXCOORD_0 이 있으면 uv_data 로, TEXCOORD_1 이 있으면 uv1_data(2 번 UV, 천사 고리)로 담는다. 프리미티브마다 재질 범위(ToonMaterialRange)를 담는다. (재질 이름, extras, 텍스처 그림 차례)
//!     부품 안에서 칸을 갈라 칠할 때(강조색 칸, 표시판) 재질 이름이나 extras 로 고르고 ToonDrawOptions 의 index_start, index_count 로 그 칸만 그린다.
//!   - 그림은 read_glb_images 로 읽고 decode_glb_image 로 풀어 ToonRenderer::create_texture 에 넘긴다.
//!   - 재질 extras 의 outline 은 바깥선 껍데기(뒷면만 그림), basic 은 빛을 받지 않는 부품이다.
//!     바깥선 설정을 주면 그 껍데기는 담지 않고, ".outline" 짝이 있는 노드의 몸으로 점마다 같은 굵기의 껍데기를 새로 짓는다.
//!   - 뼈대(skin)가 있으면 본 노드와 그 거꾸로 묶는 행렬, 움직임(animations)과 그 extras 를 함께 읽는다. 자세는 write_skin_pose 가 셈한다.
//!     섞은 자세를 스킨 행렬로 짓기 전에 뼈마다 고치려면 pose_adjust 에 고리(ToonPoseAdjust::Hook)나 고침 표(ToonPoseAdjust::Bones, adjust_bones, BoneModifier)를 준다.
//!   - 부품을 따로 숨기거나 칠하는 모형은 read_glb_node_arrays 로 노드마다 따로 읽는다. (노드 extras 와 비치는 정도를 함께)
//!   - 한 덩이로 읽은 뼈대 모형의 노드 extras 와 부품의 자리, 따르는 본은 read_glb_node_infos 로 따로 읽는다.
//!   - 꼭짓점 속성을 정수로 담은 GLB(KHR_mesh_quantization)도 읽는다. 정규화 정수는 glTF 명세의 공식으로 나누고, 텍스처 좌표는 재질 텍스처의 KHR_texture_transform 을 구워 넣는다.
//!     이름 없는 메시 노드는 이름 있는 부모 노드의 부품에 든다. (양자화 도구가 되풀이 변환을 맡기는 잎 노드) extensionsRequired 에 읽지 못하는 확장(meshopt 압축 등)이 있거나 바이너리 덩어리 밖의 버퍼를 가리키면 Err.
//! - 엔진의 SkinnedModel(텍스처 PBR)과는 따로다. 툰 그리기에 쓸 꼭짓점 배열만 뽑는다.
//==============================================================================
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::base::matrix4::Matrix4;
use crate::base::quaternion::Quaternion;
use crate::base::trigonometry::hypot_list;
use crate::graphics3d::toon::meshbuilder::{MeshBuilder, ToonMaterialRange, ToonMeshArrays};

/// GLB 의 머리와 덩어리 종류.
const GLB_MAGIC: u32 = 0x46546c67;
const CHUNK_TYPE_JSON: u32 = 0x4e4f534a;
const CHUNK_TYPE_BINARY: u32 = 0x004e4942;
/// 삼각형 프리미티브. (glTF 의 mode 4, 없으면 4)
const TRIANGLES_MODE: u64 = 4;
/// 바깥선 껍데기 노드의 이름 끝. (read_glb_node_arrays 가 앞 이름의 노드에 함께 담는다)
const OUTLINE_NODE_SUFFIX: &str = ".outline";
/// 한 번만 트는 움직임의 extras.loop. (끝 시각에 묶어 끝 자세를 둔다)
const LOOP_ONCE: &str = "once";
/// 실수 접근자의 수 종류.
const FLOAT_COMPONENT_TYPE: u64 = 5126;
/// 읽을 줄 아는 glTF 확장. (extensionsRequired 에 이 밖의 것이 있으면 읽지 않고 오류를 돌려준다)
/// - KHR_mesh_quantization: 꼭짓점 속성을 정수로 담은 것. (위치는 노드 변환이나 inverseBindMatrices 가, 텍스처 좌표는 KHR_texture_transform 이 되풀이를 맡는다)
/// - KHR_texture_transform: 텍스처 좌표의 offset, rotation, scale. (재질 텍스처의 것을 읽을 때 텍스처 좌표에 구워 넣는다)
const SUPPORTED_EXTENSIONS: [&str; 2] = ["KHR_mesh_quantization", "KHR_texture_transform"];
/// 아직 읽지 못하는 필수 압축 확장의 오류 글. (확장 이름, 글)
const UNSUPPORTED_EXTENSION_MESSAGES: [(&str, &str); 2] = [
	("EXT_meshopt_compression", "GLB 의 필수 확장 EXT_meshopt_compression(meshopt 압축)은 아직 읽지 못합니다. (압축 없이 양자화만 한 KHR_mesh_quantization 은 읽는다)"),
	("KHR_draco_mesh_compression", "GLB 의 필수 확장 KHR_draco_mesh_compression(Draco 압축)은 읽지 못합니다. (압축 없이 양자화만 한 KHR_mesh_quantization 은 읽는다)"),
];

//==============================================================================
/// 바깥선을 새로 지을 때의 설정.
/// - width: 껍데기 굵기. (0 이면 껍데기를 하나도 짓지 않는다, GLB 의 껍데기도 담지 않는다)
/// - part_widths: 부품(노드 이름)마다 따로 준 굵기. added_parts: ".outline" 짝이 없어도 껍데기를 지을 부품.
/// - inner_parts: inner_width 가 0 보다 크면 그 굵기의 껍데기를 하나 더 지어 바깥선 껍데기 끝에 담고 그 자리를 inner_outline_ranges 로 돌려준다.
///   inner_parts 와 그 위에 얹힌 inner_over_parts 의 몸이 든 자리는 inner_body_ranges 로 돌려준다. (유리 같은 부위를 둘레의 가는 선과 함께 한 번 더 그릴 때)
///
/// # 예제
/// 부품 하나만 가늘게, 짝이 없는 부품에도 바깥선 껍데기 짓기
/// ```rust,no_run
/// use std::collections::HashMap;
/// use vanilla::{read_glb_arrays, ToonMeshArrays, ToonOutlineOptions};
///
/// fn read_mannequin(bytes: &[u8]) -> Result<ToonMeshArrays, String> {
/// 	let outline_options = ToonOutlineOptions {
/// 		width: 1.5,
/// 		part_widths: HashMap::from([(String::from("Mannequin_F"), 0.8)]),
/// 		added_parts: vec![String::from("Mannequin_F")],
/// 		..ToonOutlineOptions::default()
/// 	};
/// 	read_glb_arrays(bytes, Some(&outline_options))
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonOutlineOptions {
	/// 껍데기 굵기. (모형 좌표, 0 이면 껍데기를 하나도 짓지 않는다)
	pub width: f64,
	/// 부품(노드 이름)마다 따로 준 굵기.
	pub part_widths: HashMap<String, f64>,
	/// ".outline" 짝이 없어도 껍데기를 지을 부품. (노드 이름)
	pub added_parts: Vec<String>,
	/// 안쪽 껍데기를 하나 더 지을 부품. (노드 이름, inner_width 가 0 보다 클 때)
	pub inner_parts: Vec<String>,
	/// inner_parts 위에 얹힌 부품. (노드 이름, 몸이 든 자리만 inner_body_ranges 로)
	pub inner_over_parts: Vec<String>,
	/// 안쪽 껍데기 굵기. (0 이면 짓지 않는다)
	pub inner_width: f64,
}

//==============================================================================
/// 뼈대의 노드 하나. (쉬는 자세)
///
/// # 예제
/// 쉬는 자세의 노드 둘로 본 행렬 셈하기
/// ```rust
/// use std::collections::HashMap;
/// use vanilla::{write_skin_pose, ToonSkeleton, ToonSkeletonNode};
///
/// let root = ToonSkeletonNode { name: String::from("root"), parent_index: None, translation: [0.0, 1.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let arm = ToonSkeletonNode { name: String::from("arm"), parent_index: Some(0), translation: [0.5, 0.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [2.0; 3], matrix: None };
/// let skeleton = ToonSkeleton { joints: vec![0, 1], inverse_bind_matrices: Vec::new(), nodes: vec![root, arm], animations: HashMap::new() };
/// let mut joint_matrices = vec![0.0f32; skeleton.joints.len() * 16];
/// write_skin_pose(&skeleton, "Idle", 0.0, &mut joint_matrices, None, None, None);
/// assert!((joint_matrices[16] - 2.0).abs() < 0.0001);
/// assert!((joint_matrices[28] - 0.5).abs() < 0.0001 && (joint_matrices[29] - 1.0).abs() < 0.0001);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonSkeletonNode {
	/// 노드 이름. (없으면 빈 글자)
	pub name: String,
	/// 부모 노드의 차례. (뿌리면 None)
	pub parent_index: Option<usize>,
	/// 자리. ([x, y, z])
	pub translation: [f64; 3],
	/// 돌림 사원수. ([x, y, z, w])
	pub rotation: [f64; 4],
	/// 크기. ([x, y, z])
	pub scale: [f64; 3],
	/// glTF 의 matrix. (있으면 자리, 돌림, 크기 대신 쓰고 움직임을 받지 않는다)
	pub matrix: Option<[f64; 16]>,
}

//==============================================================================
/// 움직임의 한 갈래. (노드 하나의 자리, 돌림, 크기 가운데 하나)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToonChannelPath {
	/// 자리. (값 셋)
	Translation,
	/// 돌림. (사원수 넷)
	Rotation,
	/// 크기. (값 셋)
	Scale,
	/// 그 밖의 것. (weights 같은, 읽기만 하고 자세에 쓰지 않는다)
	Other,
}

//==============================================================================
/// 움직임의 한 갈래. (움직이는 노드, 바꾸는 것, 열쇠 시각과 값)
///
/// # 예제
/// 앞 키를 그대로 두는 STEP 갈래로 노드 하나 옮기기
/// ```rust
/// use std::collections::HashMap;
/// use vanilla::{write_skin_pose, ToonAnimation, ToonAnimationChannel, ToonChannelPath, ToonSkeleton, ToonSkeletonNode};
///
/// let node = ToonSkeletonNode { name: String::from("body"), parent_index: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let hop = ToonAnimationChannel { node_index: 0, path: ToonChannelPath::Translation, times: vec![0.0, 0.5], values: vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0], is_step: true };
/// let animation = ToonAnimation { duration: 1.0, channels: vec![hop], extras: serde_json::Value::Null };
/// let skeleton = ToonSkeleton { joints: vec![0], inverse_bind_matrices: Vec::new(), nodes: vec![node], animations: HashMap::from([(String::from("Hop"), animation)]) };
/// let mut joint_matrices = [0.0; 16];
/// write_skin_pose(&skeleton, "Hop", 0.25, &mut joint_matrices, None, None, None);
/// assert_eq!(joint_matrices[13], 0.0);
/// write_skin_pose(&skeleton, "Hop", 0.75, &mut joint_matrices, None, None, None);
/// assert_eq!(joint_matrices[13], 1.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonAnimationChannel {
	/// 움직이는 노드의 차례.
	pub node_index: usize,
	/// 바꾸는 것. (자리, 돌림, 크기 가운데 하나)
	pub path: ToonChannelPath,
	/// 열쇠 시각. (초)
	pub times: Vec<f64>,
	/// 열쇠마다의 값. (돌림은 넷, 자리와 크기는 셋씩)
	pub values: Vec<f64>,
	/// interpolation 이 STEP 인지. (사이를 섞지 않고 앞 키)
	pub is_step: bool,
}

//==============================================================================
/// 움직임 하나. (길이, 갈래들, extras)
///
/// # 예제
/// 한 번만 트는 움직임은 끝 시각에 묶기
/// ```rust
/// use std::collections::HashMap;
/// use vanilla::{write_skin_pose, ToonAnimation, ToonAnimationChannel, ToonChannelPath, ToonSkeleton, ToonSkeletonNode};
///
/// let node = ToonSkeletonNode { name: String::from("body"), parent_index: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let rise = ToonAnimationChannel { node_index: 0, path: ToonChannelPath::Translation, times: vec![0.0, 2.0], values: vec![0.0, 0.0, 0.0, 0.0, 4.0, 0.0], is_step: false };
/// let animation = ToonAnimation { duration: 2.0, channels: vec![rise], extras: serde_json::json!({ "loop": "once" }) };
/// let skeleton = ToonSkeleton { joints: vec![0], inverse_bind_matrices: Vec::new(), nodes: vec![node], animations: HashMap::from([(String::from("Rise"), animation)]) };
/// let mut joint_matrices = [0.0; 16];
/// write_skin_pose(&skeleton, "Rise", 3.0, &mut joint_matrices, None, None, None);
/// assert_eq!(joint_matrices[13], 4.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonAnimation {
	/// 길이. (갈래들의 가장 늦은 열쇠 시각, 초)
	pub duration: f64,
	/// 갈래들.
	pub channels: Vec<ToonAnimationChannel>,
	/// 움직임의 extras. (loop 가 "once" 면 한 번만 튼다, 없으면 빈 것)
	pub extras: Value,
}

//==============================================================================
/// 뼈대와 움직임. (첫 skin 만)
///
/// # 예제
/// 자리 움직임 하나로 두 본의 자세 셈하기
/// ```rust
/// use std::collections::HashMap;
/// use vanilla::{write_skin_pose, ToonAnimation, ToonAnimationChannel, ToonChannelPath, ToonSkeleton, ToonSkeletonNode};
///
/// let root = ToonSkeletonNode { name: String::from("root"), parent_index: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let arm = ToonSkeletonNode { name: String::from("arm"), parent_index: Some(0), translation: [1.0, 0.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let lift = ToonAnimationChannel { node_index: 0, path: ToonChannelPath::Translation, times: vec![0.0, 1.0], values: vec![0.0, 0.0, 0.0, 0.0, 2.0, 0.0], is_step: false };
/// let animation = ToonAnimation { duration: 1.0, channels: vec![lift], extras: serde_json::Value::Null };
/// let skeleton = ToonSkeleton { joints: vec![0, 1], inverse_bind_matrices: Vec::new(), nodes: vec![root, arm], animations: HashMap::from([(String::from("Lift"), animation)]) };
/// let mut joint_matrices = vec![0.0f32; skeleton.joints.len() * 16];
/// write_skin_pose(&skeleton, "Lift", 0.5, &mut joint_matrices, None, None, None);
/// assert!((joint_matrices[16 + 12] - 1.0).abs() < 0.0001);
/// assert!((joint_matrices[16 + 13] - 1.0).abs() < 0.0001);
/// ```
///
/// GLB 의 뼈대로 그 시각의 자세 셈하기
/// ```rust,no_run
/// use vanilla::{read_glb_arrays, write_skin_pose};
///
/// fn read_pose(bytes: &[u8], time: f64) -> Result<Vec<f32>, String> {
/// 	let arrays = read_glb_arrays(bytes, None)?;
/// 	let Some(skeleton) = arrays.skeleton.as_ref() else {
/// 		return Ok(Vec::new());
/// 	};
/// 	let mut joint_matrices = vec![0.0f32; skeleton.joints.len() * 16];
/// 	write_skin_pose(skeleton, "Survey", time, &mut joint_matrices, None, None, None);
/// 	Ok(joint_matrices)
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonSkeleton {
	/// 본 노드의 차례. (skin 의 joints, 꼭짓점의 본 번호가 이 목록의 차례다)
	pub joints: Vec<usize>,
	/// 본마다 거꾸로 묶는 행렬. (16 씩, 없으면 단위 행렬)
	pub inverse_bind_matrices: Vec<f32>,
	/// 모든 노드의 쉬는 자세. (노드 차례)
	pub nodes: Vec<ToonSkeletonNode>,
	/// 이름마다 움직임.
	pub animations: HashMap<String, ToonAnimation>,
}

//==============================================================================
/// 노드마다 따로 읽은 부품. (배열, 노드 extras, 비치는 재질의 가장 작은 불투명도)
///
/// # 예제
/// 부품마다 메시를 따로 올리고 바이저만 빼기
/// ```rust,no_run
/// use vanilla::{read_glb_node_arrays, ToonMesh, ToonOutlineOptions, ToonRenderer};
///
/// fn create_part_meshes(gl: &vanilla::GlContext, renderer: &ToonRenderer, bytes: &[u8]) -> Result<Vec<(String, ToonMesh, f64)>, String> {
/// 	let outline_options = ToonOutlineOptions { width: 1.5, added_parts: vec![String::from("vanguard_Mesh"), String::from("vanguard_visor")], ..ToonOutlineOptions::default() };
/// 	let part_arrays = read_glb_node_arrays(bytes, Some(&outline_options))?;
/// 	let mut meshes = Vec::new();
/// 	for (part_name, part) in part_arrays.iter() {
/// 		if part_name == "vanguard_visor" {
/// 			continue;
/// 		}
/// 		if let Some(mesh) = renderer.create_mesh(gl, &part.arrays) {
/// 			meshes.push((part_name.clone(), mesh, part.alpha));
/// 		}
/// 	}
/// 	Ok(meshes)
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonPartArrays {
	/// 부품의 꼭짓점 배열. (뼈대는 읽지 않아 skeleton 은 None)
	pub arrays: ToonMeshArrays,
	/// 노드 extras. (없으면 빈 것)
	pub extras: Value,
	/// 비치는 재질(alphaMode BLEND)의 가장 작은 불투명도. (없으면 1)
	pub alpha: f64,
}

//==============================================================================
/// 메시 노드 하나의 정보. (extras, 꼭짓점 범위의 가운데, 가장 무겁게 딸린 본의 skin 안 차례)
///
/// # 예제
/// 걷는 모형의 바이저 자리 읽기
/// ```rust,no_run
/// use vanilla::{read_glb_arrays, read_glb_node_infos, write_skin_pose, Matrix4};
///
/// fn read_visor_position(bytes: &[u8]) -> Result<Option<[f64; 3]>, String> {
/// 	let arrays = read_glb_arrays(bytes, None)?;
/// 	let node_infos = read_glb_node_infos(bytes)?;
/// 	let (Some(skeleton), Some(visor_info)) = (arrays.skeleton.as_ref(), node_infos.get("vanguard_visor")) else {
/// 		return Ok(None);
/// 	};
/// 	let Some(joint_index) = visor_info.joint_index else {
/// 		return Ok(Some(visor_info.center));
/// 	};
/// 	let mut joint_matrices = vec![0.0f32; skeleton.joints.len() * 16];
/// 	write_skin_pose(skeleton, "Walk", 0.25, &mut joint_matrices, None, None, None);
/// 	let mut joint_matrix = Matrix4::create_identity();
/// 	joint_matrix.elements.copy_from_slice(&joint_matrices[joint_index * 16..joint_index * 16 + 16]);
/// 	let visor_position = joint_matrix.transform_point(visor_info.center[0], visor_info.center[1], visor_info.center[2]);
/// 	Ok(Some([visor_position.x, visor_position.y, visor_position.z]))
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonNodeInfo {
	/// 노드 extras. (없으면 빈 것)
	pub extras: Value,
	/// 꼭짓점 범위의 가운데. (모형 좌표, 꼭짓점이 없으면 원점)
	pub center: [f64; 3],
	/// 가장 무겁게 딸린 본의 skin 안 차례. (뼈대로 움직이지 않으면 None)
	pub joint_index: Option<usize>,
}

//==============================================================================
/// 자세를 섞을 앞 움직임. (움직임이 바뀔 때 잠깐)
///
/// # 예제
/// 움직임이 바뀔 때 앞 자세를 잠깐 섞기
/// ```rust
/// # use std::collections::HashMap;
/// # use vanilla::{ToonAnimation, ToonAnimationChannel, ToonChannelPath, ToonSkeleton, ToonSkeletonNode};
/// use vanilla::{write_skin_pose, ToonPoseBlend};
///
/// # let node = ToonSkeletonNode { name: String::from("body"), parent_index: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// # let create_hold = |height: f64| ToonAnimation { duration: 1.0, channels: vec![ToonAnimationChannel { node_index: 0, path: ToonChannelPath::Translation, times: vec![0.0], values: vec![0.0, height, 0.0], is_step: false }], extras: serde_json::Value::Null };
/// # let skeleton = ToonSkeleton { joints: vec![0], inverse_bind_matrices: Vec::new(), nodes: vec![node], animations: HashMap::from([(String::from("Survey"), create_hold(0.0)), (String::from("Run"), create_hold(2.0))]) };
/// let blend_duration = 0.2;
/// let time_since_change = 0.05;
/// let blend = ToonPoseBlend { animation_name: "Survey", time: 1.4, weight: 1.0 - time_since_change / blend_duration };
/// let mut joint_matrices = vec![0.0f32; skeleton.joints.len() * 16];
/// write_skin_pose(&skeleton, "Run", time_since_change, &mut joint_matrices, Some(blend), None, None);
/// assert!((joint_matrices[13] - 0.5).abs() < 0.0001);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonPoseBlend<'a> {
	/// 앞 움직임의 이름.
	pub animation_name: &'a str,
	/// 앞 움직임의 시각. (초)
	pub time: f64,
	/// 앞 자세의 무게. (0 이면 섞지 않음, 1 이면 앞 자세 그대로)
	pub weight: f64,
}

//==============================================================================
/// 노드마다의 흔들림 배율. (움직임 이름 → (노드 차례, 배율) 목록, 1 이면 그대로, 0 이면 쉬는 자세)
//==============================================================================
pub type ToonSwingScales = HashMap<String, Vec<(usize, f64)>>;

//==============================================================================
/// 뼈 고치기의 꼴. (adjust_bones 와 write_skin_pose 의 ToonPoseAdjust::Bones 표에서 고침 하나가 움직임 값에 어떻게 걸리는지, 원본 BoneModifyMode)
/// - Add: 움직임 위에 덧붙인다. 자리는 부모 공간에서 더하고, 돌림은 그 뼈의 공간에서 뒤에 곱하고(q x 준 돌림), 크기는 곱한다.
/// - Replace: 준 값으로 바꾼다. (자리, 돌림, 크기 모두 부모 공간의 지역 값)
///
/// # 예제
/// 엉덩이는 내리고 머리는 움직임 값을 버리고 정면으로 두기
/// ```rust
/// use std::collections::HashMap;
/// use vanilla::{write_skin_pose, BoneModifier, BoneModifyMode, ToonPoseAdjust, ToonSkeleton, ToonSkeletonNode};
///
/// let hip = ToonSkeletonNode { name: String::from("hip"), parent_index: None, translation: [0.0, 5.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let head = ToonSkeletonNode { name: String::from("head"), parent_index: Some(0), translation: [0.0, 1.0, 0.0], rotation: [0.0, 0.7071, 0.0, 0.7071], scale: [1.0; 3], matrix: None };
/// let skeleton = ToonSkeleton { joints: vec![0, 1], inverse_bind_matrices: Vec::new(), nodes: vec![hip, head], animations: HashMap::new() };
/// let modifiers = [
/// 	BoneModifier { name: Some("hip"), translation: Some([0.0, -2.0, 0.0]), mode: BoneModifyMode::Add, ..Default::default() },
/// 	BoneModifier { name: Some("head"), rotation: Some([0.0, 0.0, 0.0, 1.0]), mode: BoneModifyMode::Replace, ..Default::default() },
/// ];
/// let mut joint_matrices = vec![0.0f32; skeleton.joints.len() * 16];
/// write_skin_pose(&skeleton, "Idle", 0.0, &mut joint_matrices, None, None, Some(ToonPoseAdjust::Bones(&modifiers)));
/// assert_eq!(joint_matrices[13], 3.0);
/// assert_eq!(joint_matrices[16], 1.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BoneModifyMode {
	/// 움직임 위에 덧붙임. (자리는 더함, 돌림은 그 뼈의 공간에서 뒤에 곱함, 크기는 곱함)
	#[default]
	Add = 0,
	/// 준 값으로 바꿈. (부모 공간의 지역 값)
	Replace = 1,
}

//==============================================================================
/// 뼈 하나의 고침. (adjust_bones 와 ToonPoseAdjust::Bones 표의 한 줄, 원본 { nodeIndex, name, translation, rotation, scale, mode, weight })
/// - 준 칸(Some)만 건다. node_index 가 None 이면 name 으로 찾는다. (같은 이름이 여럿이면 앞 노드, 못 찾거나 행렬로 둔 노드면 건너뛴다)
/// - weight 는 고치기 앞과 뒤 사이의 몫이다. (0 이면 그대로, 1 이면 다 고침, 0 ~ 1 로 자름, 돌림은 가까운 쪽으로 섞어 정규화)
/// - 기본은 칸 모두 None, mode Add, weight 1 이다.
///
/// # 예제
/// 꼬리 마디를 그 뼈의 x 축으로 0.8 라디안 더 돌리고 그 반만 걸기
/// ```rust
/// use vanilla::{BoneModifier, BoneModifyMode};
///
/// let half_angle: f64 = 0.4;
/// let modifier = BoneModifier { name: Some("b_Tail02_013"), rotation: Some([half_angle.sin(), 0.0, 0.0, half_angle.cos()]), weight: 0.5, ..Default::default() };
/// assert_eq!(modifier.mode, BoneModifyMode::Add);
/// assert_eq!(modifier.node_index, None);
/// ```
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BoneModifier<'a> {
	/// 고칠 노드의 skeleton.nodes 차례. (None 이면 name 으로 찾음)
	pub node_index: Option<usize>,
	/// 고칠 노드의 이름. (node_index 가 None 일 때 쓴다)
	pub name: Option<&'a str>,
	/// 자리. ([x, y, z], Add 면 더함, Replace 면 그 값, None 이면 그대로)
	pub translation: Option<[f64; 3]>,
	/// 돌림 사원수. ([x, y, z, w], Add 면 그 뼈의 공간에서 뒤에 곱함, Replace 면 그 값을 정규화, None 이면 그대로)
	pub rotation: Option<[f64; 4]>,
	/// 크기. ([x, y, z], Add 면 곱함, Replace 면 그 값, None 이면 그대로)
	pub scale: Option<[f64; 3]>,
	/// 거는 꼴. (기본 Add)
	pub mode: BoneModifyMode,
	/// 고치기 앞과 뒤 사이의 몫. (0 ~ 1, 기본 1)
	pub weight: f64,
}

impl Default for BoneModifier<'_> {
	//==============================================================================
	/// 아무것도 고치지 않는 고침. (칸 모두 None, mode Add, weight 1)
	//==============================================================================
	fn default() -> Self {
		BoneModifier { node_index: None, name: None, translation: None, rotation: None, scale: None, mode: BoneModifyMode::Add, weight: 1.0 }
	}
}

//==============================================================================
/// 섞은 자세의 뼈마다 지역 변환을 고치는 것. (write_skin_pose 의 pose_adjust, 원본 writeSkinPose 일곱째 인자의 함수나 표)
/// - Hook: (노드마다의 지역 값, 뼈대) 로 불러 그 함수가 지역 값을 제자리에서 고친다. (이 부름만의 사본이라 뼈대의 쉬는 자세는 바뀌지 않는다)
/// - Bones: adjust_bones 의 뼈마다 고침 표로 건다.
///
/// # 예제
/// 고리 안에서 표 고침과 직접 고침을 같이 쓰기
/// ```rust
/// use std::collections::HashMap;
/// use vanilla::{adjust_bones, write_skin_pose, BoneModifier, ToonPoseAdjust, ToonSkeleton, ToonSkeletonNode};
///
/// let spine = ToonSkeletonNode { name: String::from("spine"), parent_index: None, translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let tail = ToonSkeletonNode { name: String::from("tail"), parent_index: Some(0), translation: [0.0, 0.0, -1.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let skeleton = ToonSkeleton { joints: vec![0, 1], inverse_bind_matrices: Vec::new(), nodes: vec![spine, tail], animations: HashMap::new() };
/// let half_angle: f64 = 0.3;
/// let mut crouch = |locals: &mut [ToonSkeletonNode], pose_skeleton: &ToonSkeleton| {
/// 	adjust_bones(locals, pose_skeleton, &[BoneModifier { name: Some("spine"), rotation: Some([half_angle.sin(), 0.0, 0.0, half_angle.cos()]), ..Default::default() }]);
/// 	locals[1].scale[1] *= 1.2;
/// };
/// let mut joint_matrices = vec![0.0f32; skeleton.joints.len() * 16];
/// write_skin_pose(&skeleton, "Idle", 0.0, &mut joint_matrices, None, None, Some(ToonPoseAdjust::Hook(&mut crouch)));
/// assert!((joint_matrices[5] - 0.6f64.cos() as f32).abs() < 0.0001);
/// assert_eq!(skeleton.nodes[1].scale, [1.0; 3]);
/// ```
//==============================================================================
pub enum ToonPoseAdjust<'a> {
	/// 노드마다의 지역 값을 직접 고치는 고리. (locals, skeleton)
	Hook(&'a mut dyn FnMut(&mut [ToonSkeletonNode], &ToonSkeleton)),
	/// adjust_bones 의 뼈마다 고침 표.
	Bones(&'a [BoneModifier<'a>]),
}

//==============================================================================
/// GLB 의 JSON 과 바이너리.
//==============================================================================
struct GlbChunks {
	json: Value,
	binary: Vec<u8>,
}

//==============================================================================
/// 리틀 엔디언 u32 읽기. (모자라면 None)
//==============================================================================
fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
	let slice = bytes.get(offset..offset + 4)?;
	Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

//==============================================================================
/// GLB 의 덩어리 읽기.
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
			check_required_extensions(&json)?;
			Ok(GlbChunks { json, binary })
		}
		_ => {
			Err(String::from("GLB 의 덩어리가 모자란다."))
		}
	}
}

//==============================================================================
/// 필수 확장 확인. (extensionsRequired 에 읽지 못하는 확장이 있으면 Err)
//==============================================================================
fn check_required_extensions(json: &Value) -> Result<(), String> {
	let required_extensions = json["extensionsRequired"].as_array().map(|extensions| extensions.as_slice()).unwrap_or(&[]);
	for extension in required_extensions.iter() {
		let extension_name = extension.as_str().unwrap_or("");
		if SUPPORTED_EXTENSIONS.contains(&extension_name) {
			continue;
		}
		return match UNSUPPORTED_EXTENSION_MESSAGES.iter().find(|(name, _)| *name == extension_name) {
			Some((_, message)) => {
				Err(String::from(*message))
			}
			None => {
				Err(format!("GLB 의 필수 확장 {} 은 읽지 못합니다.", extension_name))
			}
		};
	}
	Ok(())
}

//==============================================================================
/// 접근자 하나를 수 목록으로 읽기. (촘촘한 것과 byteStride 로 띄운 것, 정수는 담긴 값 그대로)
/// - 바이너리 덩어리(buffers[0]) 밖의 버퍼를 가리키면 Err. (EXT_meshopt_compression 의 fallback 버퍼도 여기 든다)
/// - 읽을 바이트가 바이너리 덩어리 밖에 걸치면 Err. (0 으로 채우거나 짧게 읽지 않는다, 원본 SkinnedModel 의 접근자 읽기처럼)
/// - bufferView 가 없으면 glTF 명세대로 0 으로 채운 count 개다. (원본 SkinnedModel 의 접근자 읽기와 같다, sparse 는 읽지 않는다)
//==============================================================================
fn read_accessor(chunks: &GlbChunks, accessor_index: usize) -> Result<Vec<f64>, String> {
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
	let count = accessor["count"].as_u64().unwrap_or(0) as usize;
	let buffer_view_index = match accessor["bufferView"].as_u64() {
		Some(buffer_view_index) => {
			buffer_view_index as usize
		}
		None => {
			return Ok(vec![0.0; count * component_count]);
		}
	};
	let buffer_view = &chunks.json["bufferViews"][buffer_view_index];
	let buffer_index = buffer_view["buffer"].as_u64().unwrap_or(0);
	if buffer_index != 0 {
		return Err(format!("GLB 의 바깥 버퍼(buffers[{}])는 읽지 못합니다. (바이너리 덩어리 buffers[0] 만 읽는다, EXT_meshopt_compression 의 fallback 버퍼도 여기 든다)", buffer_index));
	}
	let element_offset = buffer_view["byteOffset"].as_u64().unwrap_or(0) as usize + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
	let tight_byte_size = component_count * component_size;
	let byte_stride = buffer_view["byteStride"].as_u64().unwrap_or(0) as usize;
	let stride = if byte_stride == 0 { tight_byte_size } else { byte_stride };
	if count > 0 {
		let end_offset = element_offset + (count - 1) * stride + tight_byte_size;
		if end_offset > chunks.binary.len() {
			return Err(format!("GLB 접근자 {} 가 바이너리 덩어리 밖을 읽습니다. (바이트 {} ~ {}, 바이너리 덩어리 {} 바이트)", accessor_index, element_offset, end_offset, chunks.binary.len()));
		}
	}
	let mut values = Vec::with_capacity(count * component_count);
	for element_index in 0..count {
		for component_index in 0..component_count {
			let offset = element_offset + element_index * stride + component_index * component_size;
			let bytes = &chunks.binary[offset..offset + component_size];
			let value = match component_type {
				5120 => {
					bytes[0] as i8 as f64
				}
				5121 => {
					bytes[0] as f64
				}
				5122 => {
					i16::from_le_bytes([bytes[0], bytes[1]]) as f64
				}
				5123 => {
					u16::from_le_bytes([bytes[0], bytes[1]]) as f64
				}
				5125 => {
					u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
				}
				_ => {
					f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
				}
			};
			values.push(value);
		}
	}
	Ok(values)
}

//==============================================================================
/// 정규화 정수(normalized 접근자)를 실수로 바꿀 때 나눌 수. (glTF 명세의 공식, 표에 없는 수 종류는 None)
//==============================================================================
fn read_normalized_divisor(component_type: u64) -> Option<f64> {
	match component_type {
		5120 => {
			Some(127.0)
		}
		5121 => {
			Some(255.0)
		}
		5122 => {
			Some(32767.0)
		}
		5123 => {
			Some(65535.0)
		}
		_ => {
			None
		}
	}
}

//==============================================================================
/// 실수로 쓰는 접근자 읽기. (위치, 노멀, 텍스처 좌표, 움직임 값, KHR_mesh_quantization 의 정수 속성을 실수로 되풀이)
/// - 정규화(normalized) 정수는 glTF 명세의 공식으로 나눈다. (부호 없는 것은 c / 255, c / 65535, 부호 있는 것은 max(c / 127, -1), max(c / 32767, -1))
/// - 정규화가 아닌 정수는 값 그대로 실수로 옮긴다. (위치의 크기와 자리는 노드 변환이, 텍스처 좌표의 것은 KHR_texture_transform 이 맡는다)
/// - 정수 접근자의 값은 32비트 실수로 반올림해 둔다. (원본의 Float32Array 와 같은 값)
/// - is_always_normalized 가 true 면 정수는 normalized 표시가 없어도 정규화로 읽는다. (명세가 늘 정규화로 정한 무게)
//==============================================================================
fn read_float_accessor(chunks: &GlbChunks, accessor_index: usize, is_always_normalized: bool) -> Result<Vec<f64>, String> {
	let accessor = &chunks.json["accessors"][accessor_index];
	let values = read_accessor(chunks, accessor_index)?;
	let component_type = accessor["componentType"].as_u64().unwrap_or(FLOAT_COMPONENT_TYPE);
	if component_type == FLOAT_COMPONENT_TYPE {
		return Ok(values);
	}
	let is_normalized = accessor["normalized"].as_bool() == Some(true) || is_always_normalized;
	match read_normalized_divisor(component_type) {
		Some(divisor) if is_normalized => {
			Ok(values.iter().map(|value| ((value / divisor).max(-1.0) as f32) as f64).collect())
		}
		_ => {
			Ok(values.iter().map(|value| (*value as f32) as f64).collect())
		}
	}
}

//==============================================================================
/// 무게 접근자 읽기. (정수로 담겼으면 normalized 가 없어도 0 ~ 1 로, glTF 명세가 무게의 정수는 늘 정규화로 정했다)
//==============================================================================
fn read_weight_accessor(chunks: &GlbChunks, accessor_index: usize) -> Result<Vec<f64>, String> {
	read_float_accessor(chunks, accessor_index, true)
}

//==============================================================================
/// 재질 텍스처의 KHR_texture_transform 을 텍스처 좌표에 구워 넣기. (변환이 없으면 받은 좌표 그대로)
/// - 명세의 차례대로 크기, 돌림(시계 반대, 라디안), 자리를 건다. uv' = offset + R(rotation) x (scale x uv)
///   R 은 명세의 GLSL mat3(cos, sin, 0, -sin, cos, 0, 0, 0, 1) 이다. (열 차례)
/// - 프리미티브의 텍스처 좌표는 재질의 모든 텍스처가 함께 쓰므로, 바탕 텍스처(없으면 빛남 텍스처)의 변환을 건다. (gltfpack 같은 양자화 도구는 한 재질의 텍스처에 같은 변환을 준다)
/// - texture_info 는 재질의 텍스처 정보({ index, texCoord, extensions }), tex_coord_index 는 uvs 가 몇 번 텍스처 좌표(TEXCOORD_n)인지다.
/// - 변환을 건 값은 32비트 실수로 반올림해 둔다. (원본의 Float32Array 와 같은 값) 변환이 없거나 다른 좌표 묶음의 것이면 받은 uvs 그대로.
//==============================================================================
fn apply_texture_transform(uvs: Option<Vec<f64>>, texture_info: &Value, tex_coord_index: f64) -> Option<Vec<f64>> {
	let uvs = uvs?;
	let transform = &texture_info["extensions"]["KHR_texture_transform"];
	if transform.is_null() {
		return Some(uvs);
	}
	let base_tex_coord = texture_info["texCoord"].as_f64().unwrap_or(0.0);
	let transform_tex_coord = transform["texCoord"].as_f64().unwrap_or(base_tex_coord);
	if transform_tex_coord != tex_coord_index {
		return Some(uvs);
	}
	let offset = read_float_array(&transform["offset"], [0.0, 0.0]);
	let scale = read_float_array(&transform["scale"], [1.0, 1.0]);
	let rotation = transform["rotation"].as_f64().unwrap_or(0.0);
	let cosine = rotation.cos();
	let sine = rotation.sin();
	let mut result = vec![0.0; uvs.len()];
	for value_index in (0..uvs.len().saturating_sub(1)).step_by(2) {
		let scaled_u = uvs[value_index] * scale[0];
		let scaled_v = uvs[value_index + 1] * scale[1];
		result[value_index] = ((offset[0] + cosine * scaled_u - sine * scaled_v) as f32) as f64;
		result[value_index + 1] = ((offset[1] + sine * scaled_u + cosine * scaled_v) as f32) as f64;
	}
	Some(result)
}

//==============================================================================
/// 꼭짓점 속성을 삼각형 꼭짓점마다 따로 풀기. (번호 차례대로 component_count 씩 옮긴다, 범위 밖 번호는 0)
//==============================================================================
fn unweld_attribute(values: &[f64], indices: &[u32], component_count: usize) -> Vec<f64> {
	let mut result = Vec::with_capacity(indices.len() * component_count);
	for index in indices.iter() {
		for component_index in 0..component_count {
			result.push(values.get(*index as usize * component_count + component_index).copied().unwrap_or(0.0));
		}
	}
	result
}

//==============================================================================
/// 노멀이 없는 프리미티브의 평평한 노멀 짓기. (glTF 2.0 명세: 노멀이 없으면 삼각형마다 노멀 하나인 평평한 노멀이다)
/// - 꼭짓점을 삼각형마다 따로 풀고(번호는 0 부터 차례로), 삼각형마다 (p1 - p0) x (p2 - p0) 을 길이 1 로 줄여 세 꼭짓점에 준다. (넓이가 0 인 삼각형은 0)
/// - 돌려주는 것은 (위치, 노멀, 번호, 뼈대 번호, 무게, 텍스처 좌표, 2 번 텍스처 좌표)다. 없던 속성은 None 그대로.
//==============================================================================
#[allow(clippy::type_complexity)]
fn create_flat_normals(positions: &[f64], indices: &[u32], joints: Option<Vec<f64>>, weights: Option<Vec<f64>>, uvs: Option<Vec<f64>>, uvs1: Option<Vec<f64>>) -> (Vec<f64>, Vec<f64>, Vec<u32>, Option<Vec<f64>>, Option<Vec<f64>>, Option<Vec<f64>>, Option<Vec<f64>>) {
	let flat_positions = unweld_attribute(positions, indices, 3);
	let mut flat_normals = vec![0.0; flat_positions.len()];
	for (triangle_index, triangle) in flat_positions.chunks_exact(9).enumerate() {
		let first_edge = [triangle[3] - triangle[0], triangle[4] - triangle[1], triangle[5] - triangle[2]];
		let second_edge = [triangle[6] - triangle[0], triangle[7] - triangle[1], triangle[8] - triangle[2]];
		let cross = [first_edge[1] * second_edge[2] - first_edge[2] * second_edge[1], first_edge[2] * second_edge[0] - first_edge[0] * second_edge[2], first_edge[0] * second_edge[1] - first_edge[1] * second_edge[0]];
		let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
		if length <= 0.0 {
			continue;
		}
		for corner_index in 0..3 {
			for axis in 0..3 {
				flat_normals[triangle_index * 9 + corner_index * 3 + axis] = cross[axis] / length;
			}
		}
	}
	let flat_indices = (0..indices.len() as u32).collect();
	(flat_positions, flat_normals, flat_indices, joints.map(|joints| unweld_attribute(&joints, indices, 4)), weights.map(|weights| unweld_attribute(&weights, indices, 4)), uvs.map(|uvs| unweld_attribute(&uvs, indices, 2)), uvs1.map(|uvs1| unweld_attribute(&uvs1, indices, 2)))
}

//==============================================================================
/// 꼭짓점 번호 읽기. (번호 접근자가 없으면 0 부터 차례로)
//==============================================================================
fn read_indices(chunks: &GlbChunks, primitive: &Value, point_count: usize) -> Result<Vec<u32>, String> {
	match primitive["indices"].as_u64() {
		Some(indices_accessor) => {
			Ok(read_accessor(chunks, indices_accessor as usize)?.iter().map(|value| *value as u32).collect())
		}
		None => {
			Ok((0..point_count as u32).collect())
		}
	}
}

//==============================================================================
/// JSON 수 배열을 COUNT 개 읽기. (배열이 아니거나 모자라거나 수가 아닌 칸은 fallback)
//==============================================================================
fn read_float_array<const COUNT: usize>(value: &Value, fallback: [f64; COUNT]) -> [f64; COUNT] {
	let mut result = fallback;
	if let Some(array) = value.as_array() {
		for (component_index, component) in array.iter().take(COUNT).enumerate() {
			result[component_index] = component.as_f64().unwrap_or(fallback[component_index]);
		}
	}
	result
}

//==============================================================================
/// 자리, 돌림(사원수), 크기로 행렬 짓기.
//==============================================================================
fn create_transform_matrix(translation: [f64; 3], rotation: [f64; 4], scale: [f64; 3]) -> Matrix4 {
	let mut matrix = Matrix4::create_translation(translation[0], translation[1], translation[2]);
	let rotation_matrix = Matrix4::create_rotation_from_quaternion(Quaternion::create(rotation[0], rotation[1], rotation[2], rotation[3]));
	matrix.multiply(&rotation_matrix);
	matrix.scale(scale[0], scale[1], scale[2]);
	matrix
}

//==============================================================================
/// 노드의 자기 행렬. (matrix 가 있으면 그것, 아니면 자리, 돌림, 크기)
//==============================================================================
fn read_node_matrix(node: &Value) -> Matrix4 {
	if node["matrix"].is_array() {
		let mut matrix = Matrix4::default();
		matrix.elements = read_float_array(&node["matrix"], matrix.read_elements()).map(|value| value as f32);
		return matrix;
	}
	create_transform_matrix(read_float_array(&node["translation"], [0.0; 3]), read_float_array(&node["rotation"], [0.0, 0.0, 0.0, 1.0]), read_float_array(&node["scale"], [1.0; 3]))
}

//==============================================================================
/// 노드 이름. (없으면 빈 글자)
//==============================================================================
fn read_node_name(node: &Value) -> String {
	String::from(node["name"].as_str().unwrap_or(""))
}

//==============================================================================
/// 프리미티브의 재질. (없으면 Null)
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
/// 프리미티브 하나 담기. (바깥선 껍데기는 바깥선 배열에, 나머지는 색과 빛남과 함께)
/// - 뼈대로 움직이는 프리미티브는 노드 행렬을 쓰지 않는다. (glTF 의 약속, 자리는 본이 정한다)
/// - part_node 는 재질 범위의 node_name 으로 쓸 부품 노드다. (visit_mesh_nodes 가 넘긴 것)
/// - 바이너리 덩어리 밖의 버퍼를 가리키는 접근자가 있으면 Err.
//==============================================================================
fn add_primitive(builder: &mut MeshBuilder, chunks: &GlbChunks, node: &Value, primitive: &Value, matrix: &Matrix4, is_outline_rebuilt: bool, part_node: &Value) -> Result<(), String> {
	let mode = primitive["mode"].as_u64().unwrap_or(TRIANGLES_MODE);
	let attributes = &primitive["attributes"];
	let position_accessor = match attributes["POSITION"].as_u64() {
		Some(position_accessor) if mode == TRIANGLES_MODE => {
			position_accessor as usize
		}
		_ => {
			return Ok(());
		}
	};
	let positions = read_float_accessor(chunks, position_accessor, false)?;
	let indices = read_indices(chunks, primitive, positions.len() / 3)?;
	let is_skinned = !node["skin"].is_null() && attributes["JOINTS_0"].is_u64() && attributes["WEIGHTS_0"].is_u64();
	let identity_matrix = Matrix4::default();
	let part_matrix = if is_skinned { &identity_matrix } else { matrix };
	let joints = if is_skinned { Some(read_accessor(chunks, attributes["JOINTS_0"].as_u64().unwrap_or(0) as usize)?) } else { None };
	let weights = if is_skinned { Some(read_weight_accessor(chunks, attributes["WEIGHTS_0"].as_u64().unwrap_or(0) as usize)?) } else { None };
	let material = read_material(chunks, primitive);
	if material["extras"]["outline"].as_bool() == Some(true) {
		if !is_outline_rebuilt {
			builder.add_outline_triangles(&positions, &indices, part_matrix, joints.as_deref(), weights.as_deref());
		}
		return Ok(());
	}
	let normals = match attributes["NORMAL"].as_u64() {
		Some(normal_accessor) => {
			Some(read_float_accessor(chunks, normal_accessor as usize, false)?)
		}
		None => {
			None
		}
	};
	let base_color_texture = &material["pbrMetallicRoughness"]["baseColorTexture"];
	let transformed_texture_info = if base_color_texture.is_null() { &material["emissiveTexture"] } else { base_color_texture };
	let source_uvs = match attributes["TEXCOORD_0"].as_u64() {
		Some(uv_accessor) => {
			Some(read_float_accessor(chunks, uv_accessor as usize, false)?)
		}
		None => {
			None
		}
	};
	let source_uvs1 = match attributes["TEXCOORD_1"].as_u64() {
		Some(uv_accessor) => {
			Some(read_float_accessor(chunks, uv_accessor as usize, false)?)
		}
		None => {
			None
		}
	};
	let uvs = apply_texture_transform(source_uvs, transformed_texture_info, 0.0);
	let uvs1 = apply_texture_transform(source_uvs1, transformed_texture_info, 1.0);
	// 노멀이 없으면 명세대로 평평한 노멀을 짓는다. (꼭짓점을 삼각형마다 따로 푼다)
	let (positions, normals, indices, joints, weights, uvs, uvs1) = match normals {
		Some(normals) => {
			(positions, normals, indices, joints, weights, uvs, uvs1)
		}
		None => {
			create_flat_normals(&positions, &indices, joints, weights, uvs, uvs1)
		}
	};
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
		node_name: read_node_name(part_node),
		index_start,
		index_count: builder.get_index_count() - index_start,
		extras: if material["extras"].is_object() { material["extras"].clone() } else { Value::Object(serde_json::Map::new()) },
		alpha_mode: String::from(material["alphaMode"].as_str().unwrap_or("OPAQUE")),
		alpha_cutoff: material["alphaCutoff"].as_f64().unwrap_or(0.5),
		base_color_factor: base_color,
		emissive_factor: read_float_array(&material["emissiveFactor"], [0.0; 3]),
		base_color_image: read_texture_image(chunks, &material["pbrMetallicRoughness"]["baseColorTexture"]),
		emissive_image,
		has_uv: uvs.is_some(),
	});
	Ok(())
}

//==============================================================================
/// 재질의 텍스처 정보가 가리키는 그림 차례. (textures[index].source, 없으면 None)
//==============================================================================
fn read_texture_image(chunks: &GlbChunks, texture_info: &Value) -> Option<usize> {
	let texture_index = texture_info["index"].as_u64()? as usize;
	chunks.json["textures"][texture_index]["source"].as_u64().map(|source| source as usize)
}

//==============================================================================
/// GLB 에 든 그림 하나. (images 의 차례, 재질 범위의 base_color_image, emissive_image 가 가리킨다)
///
/// # 예제
/// GLB 의 그림을 images 차례대로 텍스처로 올리기
/// ```rust,no_run
/// use vanilla::core::graphic::GlTexture;
/// use vanilla::{decode_glb_image, read_glb_images, ToonRenderer, ToonTextureOptions};
///
/// fn create_image_textures(gl: &vanilla::GlContext, renderer: &ToonRenderer, bytes: &[u8]) -> Result<Vec<Option<GlTexture>>, String> {
/// 	let mut textures = Vec::new();
/// 	for image in read_glb_images(bytes)?.iter() {
/// 		if image.data.is_empty() {
/// 			textures.push(None);
/// 			continue;
/// 		}
/// 		let (width, height, pixels) = decode_glb_image(image)?;
/// 		textures.push(Some(renderer.create_texture(gl, width, height, &pixels, ToonTextureOptions::default())?));
/// 	}
/// 	Ok(textures)
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonGlbImage {
	/// 그림 이름. (없으면 빈 글자)
	pub name: String,
	/// 그림 종류. ("image/png" 같은, 없으면 빈 글자)
	pub mime_type: String,
	/// 그림 파일 바이트. (바깥 파일 uri 면 빈 것)
	pub data: Vec<u8>,
}

//==============================================================================
/// GLB 에 든 그림들. (바이너리 덩어리의 bufferView 로 담긴 그림만 읽는다, 바깥 파일 uri 는 data 가 빈 것)
///
/// # 인자
/// - `bytes`: GLB 파일 전체.
///
/// # 돌려줌
/// images 차례의 그림. (바깥 파일 uri 인 그림은 data 가 빈 것) GLB 가 아니거나 덩어리가 잘렸거나 모자라거나 JSON 을 읽지 못하면 Err(까닭 글).
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
/// read_glb_images 의 그림 하나를 풀기. ((너비, 높이, RGBA 바이트), 알파를 미리 곱하지 않는다, image 크레이트의 png, jpeg, webp, gif, bmp, ico)
///
/// # 인자
/// - `image`: read_glb_images 의 그림 하나.
///
/// # 돌려줌
/// (너비, 높이, RGBA 바이트) 풀어 낸 그림. (ToonRenderer::create_texture 에 넘긴다) 풀지 못하면 Err(까닭 글).
//==============================================================================
pub fn decode_glb_image(image: &ToonGlbImage) -> Result<(u32, u32, Vec<u8>), String> {
	let decoded = image::load_from_memory(&image.data).map_err(|error| format!("GLB 그림 {} 을 풀지 못했다: {}", image.name, error))?;
	let rgba = decoded.to_rgba8();
	Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

//==============================================================================
/// 몸으로 새로 지을 바깥선 껍데기의 굵기. (".outline" 짝도 added_parts 도 없거나 굵기가 0 이하면 None)
//==============================================================================
fn read_rebuilt_outline_width(node: &Value, node_names: &HashSet<String>, outline_options: &ToonOutlineOptions) -> Option<f64> {
	let node_name = read_node_name(node);
	let outline_name = format!("{}{}", node_name, OUTLINE_NODE_SUFFIX);
	if !node_names.contains(&outline_name) && !outline_options.added_parts.contains(&node_name) {
		return None;
	}
	let width = outline_options.part_widths.get(&node_name).copied().unwrap_or(outline_options.width);
	if width > 0.0 { Some(width) } else { None }
}

//==============================================================================
/// 뼈대 메시의 쉬는 자세 크기. (본마다 본의 쉬는 월드 행렬 x inverseBindMatrices 의 균일 크기, 그 가운데값, 셀 수 없으면 1)
/// - 정수 위치(KHR_mesh_quantization)의 뼈대 메시는 되풀이 크기가 inverseBindMatrices 에 들어 있어, 메시 좌표 1 이 모형 좌표로 이만큼이다.
/// - 쉬는 자세가 묶은 자세와 같은 보통 모형에서 실수 위치면 1 에 가깝다.
//==============================================================================
fn read_skin_bind_scale(chunks: &GlbChunks, skin: &Value) -> Result<f64, String> {
	let nodes = chunks.json["nodes"].as_array().map(|nodes| nodes.as_slice()).unwrap_or(&[]);
	let mut parent_indices: Vec<Option<usize>> = vec![None; nodes.len()];
	for (node_index, node) in nodes.iter().enumerate() {
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
	let inverse_bind_matrices = match skin["inverseBindMatrices"].as_u64() {
		Some(accessor_index) => {
			Some(read_float_accessor(chunks, accessor_index as usize, false)?)
		}
		None => {
			None
		}
	};
	let mut scales = Vec::new();
	let joints = skin["joints"].as_array().map(|joints| joints.as_slice()).unwrap_or(&[]);
	for (joint_index, joint) in joints.iter().enumerate() {
		let mut world_matrix = Matrix4::default();
		let mut node_index = joint.as_u64().map(|joint| joint as usize);
		while let Some(current_index) = node_index {
			let mut parent_matrix = read_node_matrix(&chunks.json["nodes"][current_index]);
			parent_matrix.multiply(&world_matrix);
			world_matrix = parent_matrix;
			node_index = parent_indices.get(current_index).copied().flatten();
		}
		if let Some(inverse_bind_values) = inverse_bind_matrices.as_ref().and_then(|values| values.get(joint_index * 16..joint_index * 16 + 16)) {
			let mut inverse_bind_matrix = Matrix4::default();
			for (element_index, value) in inverse_bind_values.iter().enumerate() {
				inverse_bind_matrix.elements[element_index] = *value as f32;
			}
			world_matrix.multiply(&inverse_bind_matrix);
		}
		let elements = world_matrix.elements.map(|value| value as f64);
		let determinant = elements[0] * (elements[5] * elements[10] - elements[6] * elements[9]) - elements[4] * (elements[1] * elements[10] - elements[2] * elements[9]) + elements[8] * (elements[1] * elements[6] - elements[2] * elements[5]);
		let scale = determinant.abs().cbrt();
		if scale > 0.0 && scale.is_finite() {
			scales.push(scale);
		}
	}
	if scales.is_empty() {
		return Ok(1.0);
	}
	scales.sort_by(|first, second| first.partial_cmp(second).unwrap_or(std::cmp::Ordering::Equal));
	Ok(scales[scales.len() / 2])
}

//==============================================================================
/// 노드의 몸으로 바깥선 껍데기 짓기. (색마다 나뉜 프리미티브를 이어 붙여 한 번에 넘긴다, 바깥선 재질은 빼고 노멀이 없는 것은 평평한 노멀로)
/// - outline_width 는 모형 좌표의 굵기다. 정수 위치 뼈대 메시는 read_skin_bind_scale 로 나눠 메시 좌표로 바꾼다.
//==============================================================================
fn add_rebuilt_outline(builder: &mut MeshBuilder, chunks: &GlbChunks, node: &Value, matrix: &Matrix4, outline_width: f64) -> Result<(), String> {
	let mut positions = Vec::new();
	let mut normals = Vec::new();
	let mut indices = Vec::new();
	let is_skinned = !node["skin"].is_null();
	let mut joints = Vec::new();
	let mut weights = Vec::new();
	let mut has_integer_positions = false;
	let mesh_index = node["mesh"].as_u64().unwrap_or(0) as usize;
	let primitives = chunks.json["meshes"][mesh_index]["primitives"].as_array().cloned().unwrap_or_default();
	for primitive in primitives.iter() {
		let mode = primitive["mode"].as_u64().unwrap_or(TRIANGLES_MODE);
		let attributes = &primitive["attributes"];
		if mode != TRIANGLES_MODE || !attributes["POSITION"].is_u64() {
			continue;
		}
		let material = read_material(chunks, primitive);
		let has_skin = attributes["JOINTS_0"].is_u64() && attributes["WEIGHTS_0"].is_u64();
		if material["extras"]["outline"].as_bool() == Some(true) || has_skin != is_skinned {
			continue;
		}
		let start_index = (positions.len() / 3) as u32;
		let position_accessor = attributes["POSITION"].as_u64().unwrap_or(0) as usize;
		has_integer_positions = has_integer_positions || chunks.json["accessors"][position_accessor]["componentType"].as_u64().unwrap_or(FLOAT_COMPONENT_TYPE) != FLOAT_COMPONENT_TYPE;
		let primitive_positions = read_float_accessor(chunks, position_accessor, false)?;
		let primitive_normals = match attributes["NORMAL"].as_u64() {
			Some(normal_accessor) => {
				Some(read_float_accessor(chunks, normal_accessor as usize, false)?)
			}
			None => {
				None
			}
		};
		let primitive_indices = read_indices(chunks, primitive, primitive_positions.len() / 3)?;
		let primitive_joints = if is_skinned { Some(read_accessor(chunks, attributes["JOINTS_0"].as_u64().unwrap_or(0) as usize)?) } else { None };
		let primitive_weights = if is_skinned { Some(read_weight_accessor(chunks, attributes["WEIGHTS_0"].as_u64().unwrap_or(0) as usize)?) } else { None };
		// 노멀이 없으면 몸과 같이 평평한 노멀로 짓는다. (껍데기는 같은 자리 점을 함께 밀어 이어진다)
		let (primitive_positions, primitive_normals, primitive_indices, primitive_joints, primitive_weights, _, _) = match primitive_normals {
			Some(primitive_normals) => {
				(primitive_positions, primitive_normals, primitive_indices, primitive_joints, primitive_weights, None, None)
			}
			None => {
				create_flat_normals(&primitive_positions, &primitive_indices, primitive_joints, primitive_weights, None, None)
			}
		};
		positions.extend_from_slice(&primitive_positions);
		normals.extend_from_slice(&primitive_normals);
		indices.extend(primitive_indices.iter().map(|index| start_index + index));
		if let (Some(primitive_joints), Some(primitive_weights)) = (primitive_joints, primitive_weights) {
			joints.extend(primitive_joints);
			weights.extend(primitive_weights);
		}
	}
	if indices.is_empty() {
		return Ok(());
	}
	let identity_matrix = Matrix4::default();
	let part_matrix = if is_skinned { &identity_matrix } else { matrix };
	if is_skinned {
		// 정수 위치 뼈대 메시는 메시 좌표가 양자화 칸이라, 굵기를 쉬는 자세의 모형 좌표로 맞춘다. (실수 위치는 메시 좌표 그대로)
		let shell_width = if has_integer_positions { outline_width / read_skin_bind_scale(chunks, &chunks.json["skins"][node["skin"].as_u64().unwrap_or(0) as usize])? } else { outline_width };
		builder.add_triangle_shell(&positions, &normals, &indices, part_matrix, shell_width, Some(&joints), Some(&weights));
	}
	else {
		builder.add_triangle_shell(&positions, &normals, &indices, part_matrix, outline_width, None, None);
	}
	Ok(())
}

//==============================================================================
/// 노드 이름 모음. (이름이 없는 노드는 빈 글자)
//==============================================================================
fn read_node_names(chunks: &GlbChunks) -> HashSet<String> {
	chunks.json["nodes"].as_array().map(|nodes| nodes.iter().map(read_node_name).collect()).unwrap_or_default()
}

//==============================================================================
/// 뼈대와 움직임 읽기. (없으면 None, 첫 skin 만)
//==============================================================================
fn read_skeleton(chunks: &GlbChunks) -> Result<Option<ToonSkeleton>, String> {
	let skin = match chunks.json["skins"].as_array().and_then(|skins| skins.first()) {
		Some(skin) => {
			skin
		}
		None => {
			return Ok(None);
		}
	};
	let source_nodes = match chunks.json["nodes"].as_array() {
		Some(source_nodes) => {
			source_nodes
		}
		None => {
			return Ok(None);
		}
	};
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
			matrix: if node["matrix"].is_array() { Some(read_float_array(&node["matrix"], Matrix4::default().read_elements())) } else { None },
		}
	}).collect();
	let joints: Vec<usize> = match skin["joints"].as_array() {
		Some(joints) => {
			joints.iter().filter_map(|joint| joint.as_u64().map(|joint| joint as usize)).collect()
		}
		None => {
			return Ok(None);
		}
	};
	let inverse_bind_matrices = match skin["inverseBindMatrices"].as_u64() {
		Some(accessor_index) => {
			read_float_accessor(chunks, accessor_index as usize, false)?.iter().map(|value| *value as f32).collect()
		}
		None => {
			joints.iter().flat_map(|_| Matrix4::default().elements).collect()
		}
	};
	let mut animations = HashMap::new();
	for animation in chunks.json["animations"].as_array().cloned().unwrap_or_default() {
		let mut channels = Vec::new();
		let mut duration = 0.0f64;
		for channel in animation["channels"].as_array().cloned().unwrap_or_default() {
			let sampler = &animation["samplers"][channel["sampler"].as_u64().unwrap_or(0) as usize];
			let times = read_float_accessor(chunks, sampler["input"].as_u64().unwrap_or(0) as usize, false)?;
			let values = read_float_accessor(chunks, sampler["output"].as_u64().unwrap_or(0) as usize, false)?;
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
	Ok(Some(ToonSkeleton { joints, inverse_bind_matrices, nodes, animations }))
}

//==============================================================================
/// 장면의 메시 노드를 차례로 돌기. (노드마다 부모까지 곱한 행렬, 부품 노드와 함께 visit_event(node, matrix, part_node) 를 부른다)
/// - 부품 노드는 자신부터 위로 올라가 처음 만나는 이름 있는 노드다. (이름 있는 조상이 없으면 메시 노드 자신)
///   이름 없는 메시 노드는 이름 있는 부모 노드의 부품에 든다. (KHR_mesh_quantization 의 되풀이 변환을 맡는 잎 노드처럼)
/// - visit_event 가 Err 를 돌려주면 거기서 멈추고 그 Err 를 돌려준다.
//==============================================================================
fn visit_mesh_nodes(chunks: &GlbChunks, visit_event: &mut dyn FnMut(&Value, &Matrix4, &Value) -> Result<(), String>) -> Result<(), String> {
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
	// 노드 하나 돌기. (부모 행렬에 자기 행렬을 곱하고, 메시가 있으면 visit_event, 그다음 자식으로, parent_part_node 는 위의 이름 있는 노드)
	//==============================================================================
	fn visit<'a>(chunks: &'a GlbChunks, node_index: usize, parent_matrix: &Matrix4, parent_part_node: Option<&'a Value>, visit_event: &mut dyn FnMut(&Value, &Matrix4, &Value) -> Result<(), String>) -> Result<(), String> {
		let node = &chunks.json["nodes"][node_index];
		let mut matrix = *parent_matrix;
		let node_matrix = read_node_matrix(node);
		matrix.multiply(&node_matrix);
		let is_named = node["name"].as_str().is_some_and(|name| !name.is_empty());
		let named_node = if is_named { Some(node) } else { parent_part_node };
		if node["mesh"].is_u64() {
			visit_event(node, &matrix, named_node.unwrap_or(node))?;
		}
		if let Some(children) = node["children"].as_array() {
			for child in children {
				if let Some(child_index) = child.as_u64() {
					visit(chunks, child_index as usize, &matrix, named_node, visit_event)?;
				}
			}
		}
		Ok(())
	}
	for node_index in root_nodes {
		visit(chunks, node_index, &Matrix4::default(), None, visit_event)?;
	}
	Ok(())
}

//==============================================================================
/// 노드 메시의 프리미티브 목록.
//==============================================================================
fn read_primitives(chunks: &GlbChunks, node: &Value) -> Vec<Value> {
	let mesh_index = node["mesh"].as_u64().unwrap_or(0) as usize;
	chunks.json["meshes"][mesh_index]["primitives"].as_array().cloned().unwrap_or_default()
}

//==============================================================================
/// GLB 하나를 꼭짓점 배열로. (뼈대가 있으면 skeleton 도, 바깥선 설정을 주면 껍데기를 새로 짓는다)
///
/// # 인자
/// - `bytes`: GLB 파일 전체.
/// - `outline_options`: 주면 GLB 의 바깥선 껍데기 대신 ".outline" 짝이 있는 노드의 몸으로 껍데기를 새로 짓는다. (None 이면 GLB 의 껍데기 그대로)
///
/// # 돌려줌
/// MeshBuilder::read_arrays 의 꼴과 skeleton (없으면 None), inner_outline_ranges, inner_body_ranges. (ToonRenderer::create_mesh 에 넘긴다) GLB 가 아니거나 덩어리가 잘렸거나 모자라거나 JSON 을 읽지 못하면 Err(까닭 글).
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
	visit_mesh_nodes(&chunks, &mut |node: &Value, matrix: &Matrix4, part_node: &Value| {
		let body_start = builder.get_index_count();
		for primitive in read_primitives(&chunks, node).iter() {
			add_primitive(&mut builder, &chunks, node, primitive, matrix, is_outline_rebuilt, part_node)?;
		}
		if !is_outline_rebuilt {
			return Ok(());
		}
		if let Some(outline_width) = read_rebuilt_outline_width(part_node, &node_names, options) {
			add_rebuilt_outline(&mut builder, &chunks, node, matrix, outline_width)?;
		}
		let node_name = read_node_name(part_node);
		if options.inner_parts.contains(&node_name) {
			inner_nodes.push((node.clone(), *matrix));
		}
		let body_count = builder.get_index_count() - body_start;
		let is_inner_body = options.inner_parts.contains(&node_name) || options.inner_over_parts.contains(&node_name);
		if is_inner_body && body_count > 0 {
			inner_body_ranges.push((body_start, body_count));
		}
		Ok(())
	})?;
	let mut inner_outline_ranges = Vec::new();
	if options.inner_width > 0.0 {
		for (inner_node, inner_matrix) in inner_nodes.iter() {
			let inner_start = builder.get_outline_index_count();
			add_rebuilt_outline(&mut builder, &chunks, inner_node, inner_matrix, options.inner_width)?;
			let inner_count = builder.get_outline_index_count() - inner_start;
			if inner_count > 0 {
				inner_outline_ranges.push((inner_start, inner_count));
			}
		}
	}
	let mut arrays = builder.read_arrays();
	arrays.skeleton = read_skeleton(&chunks)?;
	arrays.inner_outline_ranges = inner_outline_ranges;
	arrays.inner_body_ranges = if inner_nodes.is_empty() { Vec::new() } else { inner_body_ranges };
	Ok(arrays)
}

//==============================================================================
/// GLB 하나를 노드 이름마다의 꼭짓점 배열로. (뼈대는 읽지 않는다)
/// - 이름 끝의 ".outline" 노드(바깥선 껍데기)는 그 앞 이름의 노드에 함께 담는다.
/// - 부품마다 노드 extras 와 비치는 재질(alphaMode BLEND)의 가장 작은 불투명도를 함께 담는다.
///
/// # 인자
/// - `bytes`: GLB 파일 전체.
/// - `outline_options`: read_glb_arrays 와 같음.
///
/// # 돌려줌
/// 노드 이름마다 부품의 배열과 extras, alpha. GLB 가 아니거나 덩어리가 잘렸거나 모자라거나 JSON 을 읽지 못하면 Err(까닭 글).
//==============================================================================
pub fn read_glb_node_arrays(bytes: &[u8], outline_options: Option<&ToonOutlineOptions>) -> Result<HashMap<String, ToonPartArrays>, String> {
	let chunks = read_glb_chunks(bytes)?;
	let mut builders: HashMap<String, MeshBuilder> = HashMap::new();
	let mut part_extras: HashMap<String, Value> = HashMap::new();
	let mut part_alphas: HashMap<String, f64> = HashMap::new();
	let is_outline_rebuilt = outline_options.is_some();
	let node_names = read_node_names(&chunks);
	visit_mesh_nodes(&chunks, &mut |node: &Value, matrix: &Matrix4, part_node: &Value| {
		let node_name = read_node_name(part_node);
		let part_name = String::from(node_name.strip_suffix(OUTLINE_NODE_SUFFIX).unwrap_or(&node_name));
		let builder = builders.entry(part_name.clone()).or_default();
		if part_name == node_name && part_node["extras"].is_object() {
			part_extras.insert(part_name.clone(), part_node["extras"].clone());
		}
		for primitive in read_primitives(&chunks, node).iter() {
			add_primitive(builder, &chunks, node, primitive, matrix, is_outline_rebuilt, part_node)?;
			let material = read_material(&chunks, primitive);
			if material["alphaMode"].as_str() == Some("BLEND") {
				let base_color = read_float_array(&material["pbrMetallicRoughness"]["baseColorFactor"], [1.0; 4]);
				let previous_alpha = part_alphas.get(&part_name).copied().unwrap_or(1.0);
				part_alphas.insert(part_name.clone(), previous_alpha.min(base_color[3]));
			}
		}
		if let Some(outline_options) = outline_options {
			if let Some(outline_width) = read_rebuilt_outline_width(part_node, &node_names, outline_options) {
				add_rebuilt_outline(builder, &chunks, node, matrix, outline_width)?;
			}
		}
		Ok(())
	})?;
	let mut part_arrays = HashMap::new();
	for (part_name, builder) in builders.into_iter() {
		let extras = part_extras.remove(&part_name).unwrap_or_else(|| Value::Object(serde_json::Map::new()));
		let alpha = part_alphas.get(&part_name).copied().unwrap_or(1.0);
		part_arrays.insert(part_name, ToonPartArrays { arrays: builder.read_arrays(), extras, alpha });
	}
	Ok(part_arrays)
}

//==============================================================================
/// GLB 의 메시 노드마다의 extras, 가운데, 따르는 본. (이름 끝이 ".outline" 인 노드와 바깥선 재질은 뺀다, 이름 없는 메시 노드는 이름 있는 부모 부품에 모은다)
/// - center 는 꼭짓점이 놓인 범위의 가운데다. (모형 좌표, 뼈대로 움직이는 노드는 쉬는 자세의 자리, 꼭짓점이 없으면 원점)
/// - joint_index 는 그 노드의 꼭짓점이 가장 무겁게 딸린 본의 skin 안 차례다. (write_skin_pose 가 쓴 본 행렬의 차례)
///
/// # 인자
/// - `bytes`: GLB 파일 전체.
///
/// # 돌려줌
/// 노드 이름마다 메시 노드의 extras, center, joint_index. GLB 가 아니거나 덩어리가 잘렸거나 모자라거나 JSON 을 읽지 못하면 Err(까닭 글).
//==============================================================================
pub fn read_glb_node_infos(bytes: &[u8]) -> Result<HashMap<String, ToonNodeInfo>, String> {
	let chunks = read_glb_chunks(bytes)?;
	// 부품 이름마다 (부품 노드, 가장 작은 자리, 가장 큰 자리, 본마다의 무게 합).
	let mut part_bounds: HashMap<String, (Value, [f64; 3], [f64; 3], HashMap<usize, f64>)> = HashMap::new();
	visit_mesh_nodes(&chunks, &mut |node: &Value, matrix: &Matrix4, part_node: &Value| {
		let node_name = read_node_name(part_node);
		if node_name.ends_with(OUTLINE_NODE_SUFFIX) {
			return Ok(());
		}
		let (_, minimum, maximum, joint_weights) = part_bounds.entry(node_name).or_insert_with(|| (part_node.clone(), [f64::MAX; 3], [f64::MIN; 3], HashMap::new()));
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
			let positions = read_float_accessor(&chunks, position_accessor, false)?;
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
				let joints = read_accessor(&chunks, attributes["JOINTS_0"].as_u64().unwrap_or(0) as usize)?;
				let weights = read_weight_accessor(&chunks, attributes["WEIGHTS_0"].as_u64().unwrap_or(0) as usize)?;
				for (joint, weight) in joints.iter().zip(weights.iter()) {
					*joint_weights.entry(*joint as usize).or_insert(0.0) += *weight;
				}
			}
		}
		Ok(())
	})?;
	let mut node_infos = HashMap::new();
	for (node_name, (part_node, minimum, maximum, joint_weights)) in part_bounds.into_iter() {
		let center = if minimum[0] <= maximum[0] { [(minimum[0] + maximum[0]) / 2.0, (minimum[1] + maximum[1]) / 2.0, (minimum[2] + maximum[2]) / 2.0] } else { [0.0; 3] };
		let joint_index = joint_weights.iter().filter(|(_, weight)| **weight > 0.0).max_by(|first, second| first.1.partial_cmp(second.1).unwrap_or(std::cmp::Ordering::Equal)).map(|(joint_index, _)| *joint_index);
		let extras = if part_node["extras"].is_object() { part_node["extras"].clone() } else { Value::Object(serde_json::Map::new()) };
		node_infos.insert(node_name, ToonNodeInfo { extras, center, joint_index });
	}
	Ok(node_infos)
}

//==============================================================================
/// 움직임의 한 갈래를 그 시각에서 읽기. (LINEAR 는 사이를 섞고 돌림은 사원수를 가까운 쪽으로 섞어 원본처럼 Math.hypot 길이로 정규화, STEP 은 앞 키)
//==============================================================================
fn sample_channel(channel: &ToonAnimationChannel, time: f64) -> Vec<f64> {
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
		let dot: f64 = (0..4).map(|component_index| values[start_offset + component_index] * values[end_offset + component_index]).sum();
		sign = if dot < 0.0 { -1.0 } else { 1.0 };
	}
	let mut result: Vec<f64> = (0..component_count).map(|component_index| {
		let start_value = values[start_offset + component_index];
		let end_value = values[end_offset + component_index] * sign;
		start_value + (end_value - start_value) * ratio
	}).collect();
	if component_count == 4 {
		let length = hypot_list(&result);
		if length > 0.0 {
			for value in result.iter_mut() {
				*value /= length;
			}
		}
	}
	result
}

//==============================================================================
/// 두 돌림 섞기. (가까운 쪽으로 섞어 원본처럼 Math.hypot 길이로 정규화, ratio 가 1 이면 other)
//==============================================================================
fn mix_rotation(rotation: [f64; 4], other_rotation: [f64; 4], ratio: f64) -> [f64; 4] {
	let dot: f64 = (0..4).map(|component_index| rotation[component_index] * other_rotation[component_index]).sum();
	let sign = if dot < 0.0 { -1.0 } else { 1.0 };
	let mixed: [f64; 4] = [0, 1, 2, 3].map(|component_index| rotation[component_index] + (other_rotation[component_index] * sign - rotation[component_index]) * ratio);
	let length = hypot_list(&mixed);
	if length <= 0.0 {
		return rotation;
	}
	mixed.map(|value| value / length)
}

//==============================================================================
/// 움직임을 그 시각에서 읽은 노드마다의 자리, 돌림, 크기. (움직임이 없으면 쉬는 자세)
//==============================================================================
fn read_animation_locals(skeleton: &ToonSkeleton, animation_name: &str, time: f64, swing_scales: Option<&ToonSwingScales>) -> Vec<ToonSkeletonNode> {
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
/// 움직임에서 한 노드의 돌림이 쉬는 자세에서 가장 벗어난 각. (라디안, 열쇠 프레임마다 보고, 그 노드의 돌림이 없으면 0)
///
/// # 인자
/// - `skeleton`: read_glb_arrays 의 skeleton.
/// - `animation_name`: 움직임 이름.
/// - `node_index`: skeleton.nodes 의 차례.
///
/// # 돌려줌
/// 쉬는 자세에서 가장 벗어난 각. (라디안, 없으면 0)
//==============================================================================
pub fn read_animation_swing(skeleton: &ToonSkeleton, animation_name: &str, node_index: usize) -> f64 {
	let (animation, node) = match (skeleton.animations.get(animation_name), skeleton.nodes.get(node_index)) {
		(Some(animation), Some(node)) => {
			(animation, node)
		}
		_ => {
			return 0.0;
		}
	};
	let mut largest_angle = 0.0f64;
	for channel in animation.channels.iter() {
		if channel.node_index != node_index || channel.path != ToonChannelPath::Rotation {
			continue;
		}
		for rotation in channel.values.chunks(4) {
			if rotation.len() < 4 {
				continue;
			}
			let dot: f64 = (0..4).map(|component_index| rotation[component_index] * node.rotation[component_index]).sum();
			largest_angle = largest_angle.max(2.0 * dot.abs().min(1.0).acos());
		}
	}
	largest_angle
}

//==============================================================================
/// 두 자세 섞기. (자리와 크기는 곧게, 돌림은 가까운 쪽으로 섞어 정규화, ratio 가 1 이면 other)
//==============================================================================
fn mix_locals(locals: &mut [ToonSkeletonNode], other_locals: &[ToonSkeletonNode], ratio: f64) {
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
/// 두 사원수 곱. (rotation x other_rotation, 뒤의 돌림을 앞 돌림의 공간에서 건다, 원본처럼 Math.hypot 길이로 정규화, 길이가 0 이면 rotation)
//==============================================================================
fn multiply_rotation(rotation: [f64; 4], other_rotation: [f64; 4]) -> [f64; 4] {
	let [x, y, z, w] = rotation;
	let [other_x, other_y, other_z, other_w] = other_rotation;
	let result = [
		w * other_x + x * other_w + y * other_z - z * other_y,
		w * other_y - x * other_z + y * other_w + z * other_x,
		w * other_z + x * other_y - y * other_x + z * other_w,
		w * other_w - x * other_x - y * other_y - z * other_z,
	];
	let length = hypot_list(&result);
	if length <= 0.0 {
		return rotation;
	}
	result.map(|value| value / length)
}

//==============================================================================
/// 섞은 자세의 뼈마다 지역 변환 고치기. (write_skin_pose 가 움직임을 읽어 섞은 뒤, 스킨 행렬을 짓기 전에 거는 꼴, 행렬로 둔 노드는 건너뜀)
/// - 고침은 차례대로 걸린다. (같은 뼈에 둘을 주면 앞 고침 위에 뒤 고침)
/// - weight 가 0 이하인 고침과 못 찾은 뼈는 건너뛴다.
///
/// # 인자
/// - `locals`: 노드마다의 지역 값. (ToonPoseAdjust::Hook 고리가 받는 것, 고친 값을 여기에 쓴다)
/// - `skeleton`: read_glb_arrays 의 skeleton. (이름으로 뼈를 찾는다)
/// - `modifiers`: 뼈마다의 고침. (BoneModifier, 차례대로)
///
/// # 예제
/// 엉덩이를 반만 올리고 같은 뼈에 두 번 고침 걸기
/// ```rust
/// use std::collections::HashMap;
/// use vanilla::{adjust_bones, BoneModifier, ToonSkeleton, ToonSkeletonNode};
///
/// let hip = ToonSkeletonNode { name: String::from("hip"), parent_index: None, translation: [0.0, 1.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
/// let skeleton = ToonSkeleton { joints: vec![0], inverse_bind_matrices: Vec::new(), nodes: vec![hip], animations: HashMap::new() };
/// let mut locals = skeleton.nodes.clone();
/// adjust_bones(&mut locals, &skeleton, &[
/// 	BoneModifier { name: Some("hip"), translation: Some([0.0, 4.0, 0.0]), weight: 0.5, ..Default::default() },
/// 	BoneModifier { node_index: Some(0), scale: Some([2.0, 1.0, 1.0]), ..Default::default() },
/// ]);
/// assert_eq!(locals[0].translation, [0.0, 3.0, 0.0]);
/// assert_eq!(locals[0].scale, [2.0, 1.0, 1.0]);
/// ```
//==============================================================================
pub fn adjust_bones(locals: &mut [ToonSkeletonNode], skeleton: &ToonSkeleton, modifiers: &[BoneModifier]) {
	let mut name_indices: Option<HashMap<&str, usize>> = None;
	for modifier in modifiers.iter() {
		let mut node_index = modifier.node_index;
		if node_index.is_none() {
			if let Some(name) = modifier.name {
				let indices = name_indices.get_or_insert_with(|| {
					let mut indices = HashMap::new();
					for (index, node) in skeleton.nodes.iter().enumerate() {
						indices.entry(node.name.as_str()).or_insert(index);
					}
					indices
				});
				node_index = indices.get(name).copied();
			}
		}
		let local = match node_index.and_then(|node_index| locals.get_mut(node_index)) {
			Some(local) if local.matrix.is_none() => {
				local
			}
			_ => {
				continue;
			}
		};
		let is_replace = modifier.mode == BoneModifyMode::Replace;
		let weight = modifier.weight.clamp(0.0, 1.0);
		if weight <= 0.0 {
			continue;
		}
		let mix_vector = |before: [f64; 3], after: [f64; 3]| -> [f64; 3] {
			if weight >= 1.0 {
				return after;
			}
			[0, 1, 2].map(|component_index| before[component_index] + (after[component_index] - before[component_index]) * weight)
		};
		if let Some(translation) = modifier.translation {
			let before = local.translation;
			let after = if is_replace { translation } else { [0, 1, 2].map(|component_index| before[component_index] + translation[component_index]) };
			local.translation = mix_vector(before, after);
		}
		if let Some(rotation) = modifier.rotation {
			let before = local.rotation;
			let after = if is_replace { multiply_rotation([0.0, 0.0, 0.0, 1.0], rotation) } else { multiply_rotation(before, rotation) };
			local.rotation = if weight >= 1.0 { after } else { mix_rotation(before, after, weight) };
		}
		if let Some(scale) = modifier.scale {
			let before = local.scale;
			let after = if is_replace { scale } else { [0, 1, 2].map(|component_index| before[component_index] * scale[component_index]) };
			local.scale = mix_vector(before, after);
		}
	}
}

//==============================================================================
/// 뼈대 자세 셈하기. (움직임을 그 시각에서 읽어 본마다 행렬을 target 에 16 씩 쓴다, 움직임이 없으면 쉬는 자세)
/// - blend 를 주면 앞 움직임의 자세를 그 무게만큼 섞는다. (움직임이 바뀔 때 잠깐)
/// - swing_scales 를 주면 그 움직임의 그 노드는 돌림을 쉬는 자세 쪽으로 줄인다. (섞는 앞 자세에도 같이)
/// - pose_adjust 를 주면 섞은 뒤, 스킨 행렬을 짓기 전에 뼈마다의 지역 변환을 고친다. (움직임에 없는 자세, 머리 돌리기, 웅크림 같은 뼈 덮어쓰기)
///   Hook 이면 (locals, skeleton) 으로 불러 그 함수가 locals 를 고치고, Bones 면 adjust_bones 의 뼈마다 고침 표로 건다. (None 이면 움직임 그대로)
///
/// # 인자
/// - `skeleton`: read_glb_arrays 의 skeleton.
/// - `animation_name`: 움직임 이름. (없으면 쉬는 자세)
/// - `time`: 초. (움직임 길이로 되풀이, 한 번만 트는 움직임은 끝 시각에 묶음)
/// - `target`: 본마다 16 씩 쓸 곳. (skeleton.joints 수 x 16 이상, 모자라면 들어가는 본까지만 쓴다, ToonDrawOptions 의 joint_matrices)
/// - `blend`: 섞을 앞 자세와 그 무게. (None 이면 섞지 않음)
/// - `swing_scales`: 움직임 이름마다 (노드 차례, 배율) 목록의 흔들림 배율. (1 이면 그대로, 0 이면 쉬는 자세, None 이면 없음)
/// - `pose_adjust`: 섞은 뒤 뼈마다의 지역 변환을 고치는 고리나 고침 표. (None 이면 움직임 그대로, 고리가 받는 locals 는 이 부름만의 사본이라 제자리에서 고쳐도 된다)
//==============================================================================
pub fn write_skin_pose(skeleton: &ToonSkeleton, animation_name: &str, time: f64, target: &mut [f32], blend: Option<ToonPoseBlend>, swing_scales: Option<&ToonSwingScales>, pose_adjust: Option<ToonPoseAdjust>) {
	let mut locals = read_animation_locals(skeleton, animation_name, time, swing_scales);
	if let Some(blend) = blend {
		if blend.weight > 0.0 {
			let blend_locals = read_animation_locals(skeleton, blend.animation_name, blend.time, swing_scales);
			mix_locals(&mut locals, &blend_locals, blend.weight.min(1.0));
		}
	}
	match pose_adjust {
		Some(ToonPoseAdjust::Hook(hook)) => {
			hook(&mut locals, skeleton);
		}
		Some(ToonPoseAdjust::Bones(modifiers)) => {
			adjust_bones(&mut locals, skeleton, modifiers);
		}
		None => {
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
				Matrix4 { elements: elements.map(|value| value as f32) }
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

//==============================================================================
/// write_skin_pose 의 뼈 고침(pose_adjust)과 adjust_bones 시험. (원본 tests/graphics3d/skinposeadjust.test.mjs 와 같은 내용, 원본 예제 fox.glb 가 없으면 그 시험은 건너뛴다)
//==============================================================================
#[cfg(test)]
mod pose_adjust_tests {
	use super::*;

	/// 원본 예제 자산 자리. (fox.glb, 저장소 옆의 vanilla.js)
	const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

	//==============================================================================
	/// fox.glb 의 뼈대. (없으면 None)
	//==============================================================================
	fn read_fox_skeleton() -> Option<ToonSkeleton> {
		let glb_bytes = match std::fs::read(std::path::Path::new(EXAMPLE_ASSET_DIRECTORY).join("fox.glb")) {
			Ok(glb_bytes) => {
				glb_bytes
			}
			Err(_) => {
				eprintln!("fox.glb 없음, 건너뜀");
				return None;
			}
		};
		read_glb_arrays(&glb_bytes, None).unwrap().skeleton
	}

	//==============================================================================
	/// 그 시각의 스킨 행렬. (섞기와 흔들림 배율 없이)
	//==============================================================================
	fn write_pose(skeleton: &ToonSkeleton, animation_name: &str, time: f64, pose_adjust: Option<ToonPoseAdjust>) -> Vec<f32> {
		let mut target = vec![0.0f32; skeleton.joints.len() * 16];
		write_skin_pose(skeleton, animation_name, time, &mut target, None, None, pose_adjust);
		target
	}

	//==============================================================================
	/// 비트까지 같은지. (원본 Object.is 와 같게 비트로 본다)
	//==============================================================================
	fn is_same_bits(left: &[f32], right: &[f32]) -> bool {
		left.len() == right.len() && left.iter().zip(right.iter()).all(|(left_value, right_value)| left_value.to_bits() == right_value.to_bits())
	}

	//==============================================================================
	/// 그 노드와 그 아래 노드 차례들.
	//==============================================================================
	fn read_subtree(skeleton: &ToonSkeleton, root_index: usize) -> HashSet<usize> {
		let mut indices = HashSet::from([root_index]);
		let mut is_growing = true;
		while is_growing {
			is_growing = false;
			for (node_index, node) in skeleton.nodes.iter().enumerate() {
				if !indices.contains(&node_index) && node.parent_index.map(|parent_index| indices.contains(&parent_index)).unwrap_or(false) {
					indices.insert(node_index);
					is_growing = true;
				}
			}
		}
		indices
	}

	//==============================================================================
	/// 작은 뼈대. (뿌리 아래 팔, 팔은 matrix 로 둔 노드 아래 하나 더, 움직임 "Wave")
	//==============================================================================
	fn create_small_skeleton() -> ToonSkeleton {
		let root = ToonSkeletonNode { name: String::from("root"), parent_index: None, translation: [0.0, 1.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: None };
		let arm = ToonSkeletonNode { name: String::from("arm"), parent_index: Some(0), translation: [1.0, 0.0, 0.0], rotation: [0.0, 0.0, 0.2, 0.98], scale: [1.0; 3], matrix: None };
		let fixed = ToonSkeletonNode { name: String::from("fixed"), parent_index: Some(1), translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3], matrix: Some([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 1.0]) };
		let wave = ToonAnimationChannel { node_index: 1, path: ToonChannelPath::Rotation, times: vec![0.0, 1.0], values: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.7071, 0.7071], is_step: false };
		let animation = ToonAnimation { duration: 1.0, channels: vec![wave], extras: Value::Null };
		ToonSkeleton { joints: vec![0, 1, 2], inverse_bind_matrices: Vec::new(), nodes: vec![root, arm, fixed], animations: HashMap::from([(String::from("Wave"), animation)]) }
	}

	//==============================================================================
	/// 고리가 없거나 아무것도 고치지 않으면 비트까지 같고, 바꾸기, 무게, 차례, 행렬 노드 건너뛰기. (fox.glb 없이 도는 작은 뼈대)
	//==============================================================================
	#[test]
	fn small_skeleton_adjust() {
		let skeleton = create_small_skeleton();
		let base_pose = write_pose(&skeleton, "Wave", 0.4, None);
		let mut empty_hook = |_locals: &mut [ToonSkeletonNode], _skeleton: &ToonSkeleton| {
		};
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Wave", 0.4, Some(ToonPoseAdjust::Hook(&mut empty_hook)))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Wave", 0.4, Some(ToonPoseAdjust::Bones(&[])))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Wave", 0.4, Some(ToonPoseAdjust::Bones(&[BoneModifier { name: Some("arm"), rotation: Some([0.3, 0.0, 0.0, 0.95]), weight: 0.0, ..Default::default() }])))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Wave", 0.4, Some(ToonPoseAdjust::Bones(&[BoneModifier { name: Some("missing"), translation: Some([1.0, 2.0, 3.0]), ..Default::default() }])))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Wave", 0.4, Some(ToonPoseAdjust::Bones(&[BoneModifier { name: Some("fixed"), translation: Some([1.0, 2.0, 3.0]), ..Default::default() }])))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Wave", 0.4, Some(ToonPoseAdjust::Bones(&[BoneModifier { node_index: Some(9), translation: Some([1.0, 2.0, 3.0]), ..Default::default() }])))));
		let mut locals = skeleton.nodes.clone();
		adjust_bones(&mut locals, &skeleton, &[BoneModifier { name: Some("root"), translation: Some([1.0, 2.0, 3.0]), rotation: Some([0.0, 0.0, 0.0, 2.0]), scale: Some([2.0, 2.0, 2.0]), mode: BoneModifyMode::Replace, ..Default::default() }]);
		assert_eq!((locals[0].translation, locals[0].rotation, locals[0].scale), ([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [2.0, 2.0, 2.0]));
		let mut locals = skeleton.nodes.clone();
		adjust_bones(&mut locals, &skeleton, &[BoneModifier { name: Some("root"), translation: Some([0.0, 4.0, 0.0]), scale: Some([3.0, 1.0, 1.0]), weight: 0.5, ..Default::default() }, BoneModifier { name: Some("root"), translation: Some([0.0, 1.0, 0.0]), ..Default::default() }]);
		assert_eq!(locals[0].translation, [0.0, 4.0, 0.0]);
		assert_eq!(locals[0].scale, [2.0, 1.0, 1.0]);
		assert_eq!(BoneModifyMode::Add as i32, 0);
		assert_eq!(BoneModifyMode::Replace as i32, 1);
	}

	//==============================================================================
	/// 원본 시험과 같은 내용. (fox.glb 의 Run 0.3 초, 고리 없음과 빈 고리, 한 뼈 고치면 그 아래 본만 바뀜, 번호와 이름, 표와 고리, 바꾸기와 무게)
	//==============================================================================
	#[test]
	fn fox_pose_adjust() {
		let Some(skeleton) = read_fox_skeleton() else {
			return;
		};
		let joint_count = skeleton.joints.len();
		let base_pose = write_pose(&skeleton, "Run", 0.3, None);
		let mut empty_hook = |_locals: &mut [ToonSkeletonNode], _skeleton: &ToonSkeleton| {
		};
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Hook(&mut empty_hook)))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Bones(&[])))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Bones(&[BoneModifier { name: Some("b_Tail02_013"), rotation: Some([0.3, 0.0, 0.0, 0.95]), weight: 0.0, ..Default::default() }])))));
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Bones(&[BoneModifier { name: Some("missing bone"), translation: Some([1.0, 2.0, 3.0]), ..Default::default() }])))));
		// 꼬리 둘째 마디를 고치면 그 마디와 그 아래 본만 바뀌고 나머지 스킨 행렬은 비트까지 같다.
		let tail_index = skeleton.nodes.iter().position(|node| node.name == "b_Tail02_013").unwrap();
		let subtree = read_subtree(&skeleton, tail_index);
		let half_angle: f64 = 0.4;
		let turn = [half_angle.sin(), 0.0, 0.0, half_angle.cos()];
		let table_pose = write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Bones(&[BoneModifier { name: Some("b_Tail02_013"), rotation: Some(turn), mode: BoneModifyMode::Add, ..Default::default() }])));
		let mut changed_count = 0;
		for joint_index in 0..joint_count {
			let range = joint_index * 16..joint_index * 16 + 16;
			let is_same = is_same_bits(&base_pose[range.clone()], &table_pose[range]);
			if subtree.contains(&skeleton.joints[joint_index]) {
				assert!(!is_same, "{}", joint_index);
				changed_count += 1;
			}
			else {
				assert!(is_same, "{}", joint_index);
			}
		}
		assert!(changed_count >= 1);
		assert!(is_same_bits(&table_pose, &write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Bones(&[BoneModifier { node_index: Some(tail_index), rotation: Some(turn), ..Default::default() }])))));
		// 표의 Add 돌림은 그 뼈의 공간에서 뒤에 곱한 것(q x 준 돌림)과 같다.
		let mut turn_hook = |locals: &mut [ToonSkeletonNode], _skeleton: &ToonSkeleton| {
			let [x, y, z, w] = locals[tail_index].rotation;
			let (turn_x, turn_w) = (turn[0], turn[3]);
			locals[tail_index].rotation = [w * turn_x + x * turn_w, y * turn_w + z * turn_x, z * turn_w - y * turn_x, w * turn_w - x * turn_x];
		};
		let hook_pose = write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Hook(&mut turn_hook)));
		for (table_value, hook_value) in table_pose.iter().zip(hook_pose.iter()) {
			assert!((table_value - hook_value).abs() <= 1e-5);
		}
		// 고리가 움직임이 없는 노드를 제자리에서 고쳐도 뼈대의 쉬는 자세는 바뀌지 않는다.
		let animated_nodes: HashSet<usize> = skeleton.animations["Run"].channels.iter().map(|channel| channel.node_index).collect();
		let still_index = (0..skeleton.nodes.len()).find(|node_index| !animated_nodes.contains(node_index) && skeleton.nodes[*node_index].matrix.is_none()).unwrap();
		let rest_translation = skeleton.nodes[still_index].translation;
		let mut move_hook = |locals: &mut [ToonSkeletonNode], _skeleton: &ToonSkeleton| {
			locals[still_index].translation[1] += 5.0;
		};
		let moved_pose = write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Hook(&mut move_hook)));
		assert!(!is_same_bits(&base_pose, &moved_pose));
		assert_eq!(skeleton.nodes[still_index].translation, rest_translation);
		assert!(is_same_bits(&base_pose, &write_pose(&skeleton, "Run", 0.3, None)));
		// Replace 와 weight: 바꾸기는 그 값, 무게 반은 앞과 뒤의 가운데, 고침은 차례대로.
		let hip_index = skeleton.nodes.iter().position(|node| node.name == "b_Hip_01").unwrap();
		let read_local = |modifiers: &[BoneModifier]| -> ToonSkeletonNode {
			let mut captured = None;
			let mut capture_hook = |locals: &mut [ToonSkeletonNode], pose_skeleton: &ToonSkeleton| {
				adjust_bones(locals, pose_skeleton, modifiers);
				captured = Some(locals[hip_index].clone());
			};
			write_pose(&skeleton, "Run", 0.3, Some(ToonPoseAdjust::Hook(&mut capture_hook)));
			captured.unwrap()
		};
		let before = read_local(&[]);
		let replaced = read_local(&[BoneModifier { name: Some("b_Hip_01"), translation: Some([1.0, 2.0, 3.0]), rotation: Some([0.0, 0.0, 0.0, 2.0]), scale: Some([2.0, 2.0, 2.0]), mode: BoneModifyMode::Replace, ..Default::default() }]);
		assert_eq!((replaced.translation, replaced.rotation, replaced.scale), ([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [2.0, 2.0, 2.0]));
		let half_added = read_local(&[BoneModifier { name: Some("b_Hip_01"), translation: Some([0.0, 4.0, 0.0]), scale: Some([3.0, 1.0, 1.0]), weight: 0.5, ..Default::default() }]);
		assert!((half_added.translation[1] - (before.translation[1] + 2.0)).abs() <= 1e-9);
		assert!((half_added.scale[0] - before.scale[0] * 2.0).abs() <= 1e-9);
		assert!((half_added.translation[0] - before.translation[0]).abs() <= 1e-12);
		let twice = read_local(&[BoneModifier { name: Some("b_Hip_01"), translation: Some([0.0, 1.0, 0.0]), ..Default::default() }, BoneModifier { name: Some("b_Hip_01"), translation: Some([0.0, 1.0, 0.0]), ..Default::default() }]);
		assert!((twice.translation[1] - (before.translation[1] + 2.0)).abs() <= 1e-9);
	}

	//==============================================================================
	/// JSON 의 고침 표를 BoneModifier 로. (mode 는 "add", "replace" 글자)
	//==============================================================================
	fn read_modifiers(value: &Value) -> Vec<BoneModifier<'_>> {
		let read_numbers = |value: &Value| -> Option<Vec<f64>> {
			value.as_array().map(|numbers| numbers.iter().map(|number| number.as_f64().unwrap()).collect())
		};
		value.as_array().unwrap().iter().map(|modifier| {
			BoneModifier {
				node_index: modifier["nodeIndex"].as_u64().map(|node_index| node_index as usize),
				name: modifier["name"].as_str(),
				translation: read_numbers(&modifier["translation"]).map(|numbers| [numbers[0], numbers[1], numbers[2]]),
				rotation: read_numbers(&modifier["rotation"]).map(|numbers| [numbers[0], numbers[1], numbers[2], numbers[3]]),
				scale: read_numbers(&modifier["scale"]).map(|numbers| [numbers[0], numbers[1], numbers[2]]),
				mode: if modifier["mode"].as_str() == Some("replace") { BoneModifyMode::Replace } else { BoneModifyMode::Add },
				weight: modifier["weight"].as_f64().unwrap_or(1.0),
			}
		}).collect()
	}

	//==============================================================================
	/// 두 엔진 견주기. (원본 cb59ad0 을 크롬에서 돌려 찍은 tests/assets/skinposeadjust/chrome_skinpose.json 의 본마다 스킨 행렬과 1e-9 안에서 같은지, fox.glb 가 없으면 건너뜀)
	//==============================================================================
	#[test]
	fn fox_pose_matches_vanilla_js() {
		let Some(skeleton) = read_fox_skeleton() else {
			return;
		};
		let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/skinposeadjust/chrome_skinpose.json")).unwrap();
		let document: Value = serde_json::from_str(&text).unwrap();
		assert_eq!(document["jointCount"].as_u64(), Some(skeleton.joints.len() as u64));
		let mut maximum_error = 0.0f64;
		let mut compared_count = 0;
		for pose_case in document["cases"].as_array().unwrap() {
			let blend = pose_case["blend"].as_object().map(|blend| ToonPoseBlend { animation_name: blend["animationName"].as_str().unwrap(), time: blend["time"].as_f64().unwrap(), weight: blend["weight"].as_f64().unwrap() });
			let modifiers = if pose_case["modifiers"].is_array() { read_modifiers(&pose_case["modifiers"]) } else { Vec::new() };
			let mut crouch_hook = |locals: &mut [ToonSkeletonNode], pose_skeleton: &ToonSkeleton| {
				let crouch_half: f64 = 0.6 / 2.0;
				adjust_bones(locals, pose_skeleton, &[BoneModifier { name: Some("b_Spine01_02"), rotation: Some([crouch_half.sin(), 0.0, 0.0, crouch_half.cos()]), ..Default::default() }]);
				let tail_index = pose_skeleton.nodes.iter().position(|node| node.name == "b_Tail01_012").unwrap();
				locals[tail_index].scale[1] *= 1.2;
			};
			let pose_adjust = if pose_case["hook"].as_str() == Some("crouch") {
				Some(ToonPoseAdjust::Hook(&mut crouch_hook))
			}
			else if pose_case["modifiers"].is_array() {
				Some(ToonPoseAdjust::Bones(&modifiers))
			}
			else {
				None
			};
			let mut target = vec![0.0f32; skeleton.joints.len() * 16];
			write_skin_pose(&skeleton, pose_case["animationName"].as_str().unwrap(), pose_case["time"].as_f64().unwrap(), &mut target, blend, None, pose_adjust);
			let expected: Vec<f64> = pose_case["jointMatrices"].as_array().unwrap().iter().map(|number| number.as_f64().unwrap()).collect();
			assert_eq!(expected.len(), target.len());
			for (index, value) in target.iter().enumerate() {
				let error = (*value as f64 - expected[index]).abs();
				assert!(error <= 1e-9, "{} {} {} {}", pose_case["name"], index, value, expected[index]);
				maximum_error = maximum_error.max(error);
				compared_count += 1;
			}
		}
		eprintln!("skinposeadjust 두 엔진 견주기: 값 {} 개, 가장 큰 차 {:e}", compared_count, maximum_error);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 시험용 작은 GLB. (네모 하나, TEXCOORD_0 은 정규화한 u16, 재질 이름과 extras, 바탕색 텍스처의 그림)
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

	//==============================================================================
	/// 작은 GLB 에서 UV, 재질 범위, 노드 extras, 그림을 읽고 그림을 RGBA 로 푸는지.
	//==============================================================================
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
