//==============================================================================
// 스킨드 모델. (원본 experimental/graphics/skinnedmodel.js)
// - 모델 인스턴스, 메시/본/스킨/머티리얼/애니메이션 데이터 + 포즈 상태. 렌더링은 SkinnedModelRenderer 가 맡는다.
// - GLB(바이너리 glTF)와 바이너리 FBX 를 공통 중립 스키마(MeshDescription + MaterialDescription)로 해석해 GL 리소스를 구성한다.
//   (임포터는 파싱만 → 중립 스키마 → 공용 GL 업로드 uploadMeshesAndMaterials)
// - 스킨 없는 정적 메시는 합성 스킨(조인트 1개)으로, 모프 타깃은 실수 텍스처로 올려 같은 스키닝 경로로 그린다.
// - 애니메이션 크로스페이드와 조인트 회전 오프셋(절차 포즈 가공), 모프 가중치(glTF weights / 프로그램)를 원본처럼 지원한다.
// - 원본과 다른 것:
//   - 원본은 async 이고 URL 을 fetch 한다. 러스트 판은 바이트를 받는 loadFromBytes / loadFromFbxBytes / addAnimationsFromFbxBytes 와
//     데스크탑 경로를 읽는 loadFromPath / loadFromFbxPath / addAnimationsFromFbxPath 를 둔다. (웹은 BlobAsset 의 바이트를 넘긴다)
//   - GL 을 쓰는 함수는 gl 을 받는다. update(deltaTime) 는 GL 을 쓰지 않는다. 원본 update 6단계(바뀐 모프 타깃 업로드)는
//     렌더러의 draw / drawDepth / drawWireframe 가 uploadPendingMorphTargets 로 대신한다. (그래서 그리기 함수가 &mut 모델을 받는다)
//     덕분에 씬의 tick 에서 update 를 부를 수 있다.
//   - 해석과 업로드를 나눠 부를 수 있다: interpretContent / interpretFbxContent 는 노드, 스킨, 애니메이션을 채우고 중립 서술을 돌려준다.
//     셰이더 없이 만든 모델(createWithoutShaderProgram)은 포즈 계산만 한다. (애니메이션 원본 모델, 단위 시험)
//   - 원본 노드 / 드로어블 / 스킨 / 클립 객체는 SkinnedModelNode / Drawable / Skin / AnimationClip 구조체다.
//     드로어블의 머티리얼은 Rc<Material> 이다. 클립은 인덱스로 가리키고, sampleAnimation 도 클립 인덱스를 받는다.
//   - 드로어블의 meshDescription 은 모프 타깃을 morphTargetList 로 옮긴 뒤라 morphTargets 가 비어 있다. (큰 델타 배열을 두 벌 들지 않는다)
//   - 원본이 TypeError 로 멈추는 자리 가운데 몇 곳은 건너뛴다: glTF 바깥(uri) 이미지는 텍스처 없음, inverseBindMatrices 가 없는 스킨은
//     단위 행렬, 본이 없는 클러스터는 단위 조인트, 노드가 없거나 알 수 없는 경로의 채널은 버린다. 원본처럼 노드 matrix 와
//     CUBICSPLINE 보간, 프리미티브 mode, glTF 노드의 weights 는 보지 않는다. FBX 는 원본처럼 스킨이 붙은 메시만 그린다.
//   - 원본에 없는 destroy 를 더했다. (버텍스 어레이, 버퍼, 모프 텍스처, 이 모델만 쓰는 머티리얼 텍스처를 지운다)
//==============================================================================
use crate::base::matrix4::Matrix4;
use crate::base::quaternion::Quaternion;
use crate::core::graphic::{GlBuffer, GlTexture, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::fbxloader::{FbxAnimation, FbxLoader, FbxSceneData};
use crate::graphics3d::material::{ImageDescription, Material, MaterialDescription};
use crate::graphics3d::skinnedmodelrenderer::SkinnedModelRenderer;
use glow::HasContext;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// GLB 머리와 청크 타입.
const GLB_MAGIC: u32 = 0x46546c67;
const GLB_CHUNKTYPE_JSON: u32 = 0x4e4f534a;
const GLB_CHUNKTYPE_BINARY: u32 = 0x004e4942;

// 접근자 컴포넌트 타입. (원본 COMPONENT_ARRAY_TABLE 의 키)
pub const COMPONENT_TYPE_UNSIGNED_BYTE: u32 = 5121;
pub const COMPONENT_TYPE_UNSIGNED_SHORT: u32 = 5123;
pub const COMPONENT_TYPE_UNSIGNED_INT: u32 = 5125;
pub const COMPONENT_TYPE_FLOAT: u32 = 5126;

// 모프 타깃 텍스처 가로 크기. (정점 델타를 실수 텍스처에 행 단위로 적재, 버텍스 셰이더가 gl_VertexID 로 조회)
const MORPH_TEXTURE_WIDTH: usize = 2048;

// 드로어블당 모프 타깃 최대 개수. (셰이더 유니폼 배열 크기와 일치)
pub const MORPH_TARGET_MAXIMUM: usize = 96;

//==============================================================================
// 애니메이션 채널 경로. (원본 문자열 "translation" / "rotation" / "scale" / "weights")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimationPath {
	translation,
	rotation,
	scale,
	weights,
}

impl AnimationPath {
	//==============================================================================
	// glTF 경로 문자열로부터. (모르는 경로면 None)
	//==============================================================================
	pub fn fromName(pathName: &str) -> Option<AnimationPath> {
		match pathName {
			"translation" => {
				Some(AnimationPath::translation)
			}
			"rotation" => {
				Some(AnimationPath::rotation)
			}
			"scale" => {
				Some(AnimationPath::scale)
			}
			"weights" => {
				Some(AnimationPath::weights)
			}
			_ => {
				None
			}
		}
	}
}

//==============================================================================
// 모델 노드. (본 계층 + 포즈 상태, 원본 nodeList 원소)
//==============================================================================
#[derive(Clone, Debug)]
pub struct SkinnedModelNode {
	pub name: String,
	pub childIndices: Vec<usize>,
	pub meshIndex: Option<usize>,
	pub skinIndex: Option<usize>,
	pub baseTranslation: [f32; 3],
	pub baseRotation: Quaternion,
	pub baseScale: [f32; 3],
	pub currentTranslation: [f32; 3],
	pub currentRotation: Quaternion,
	pub currentScale: [f32; 3],
	pub worldMatrix: Matrix4,
}

//==============================================================================
// 스킨. (조인트 노드 인덱스 + 역바인드 행렬 + 셰이더로 올릴 조인트 행렬 배열)
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skin {
	pub jointNodeIndices: Vec<usize>,
	pub inverseBindMatrices: Vec<Matrix4>,
	pub jointMatrixArray: Vec<f32>,
}

//==============================================================================
// 모프 타깃. (정점별 위치 델타 + 선택 노멀 델타)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct MorphTarget {
	pub name: String,
	pub positionDeltas: Vec<f32>,
	pub normalDeltas: Option<Vec<f32>>,
}

//==============================================================================
// 인덱스 배열. (원본 Uint16Array / Uint32Array)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum IndexArray {
	unsignedShort(Vec<u16>),
	unsignedInt(Vec<u32>),
}

impl IndexArray {
	//==============================================================================
	// 인덱스 개수.
	//==============================================================================
	pub fn len(&self) -> usize {
		match self {
			IndexArray::unsignedShort(indices) => {
				indices.len()
			}
			IndexArray::unsignedInt(indices) => {
				indices.len()
			}
		}
	}

	//==============================================================================
	// 비었는지.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	//==============================================================================
	// 인덱스 하나 반환.
	//==============================================================================
	pub fn get(&self, position: usize) -> usize {
		match self {
			IndexArray::unsignedShort(indices) => {
				indices[position] as usize
			}
			IndexArray::unsignedInt(indices) => {
				indices[position] as usize
			}
		}
	}

	//==============================================================================
	// GL 원소 타입. (UNSIGNED_SHORT / UNSIGNED_INT)
	//==============================================================================
	pub fn getGlComponentType(&self) -> u32 {
		match self {
			IndexArray::unsignedShort(_) => {
				glow::UNSIGNED_SHORT
			}
			IndexArray::unsignedInt(_) => {
				glow::UNSIGNED_INT
			}
		}
	}

	//==============================================================================
	// 바이트로 보기. (GL 버퍼 업로드용)
	//==============================================================================
	fn asBytes(&self) -> &[u8] {
		match self {
			IndexArray::unsignedShort(indices) => {
				sliceToBytes(indices)
			}
			IndexArray::unsignedInt(indices) => {
				sliceToBytes(indices)
			}
		}
	}
}

//==============================================================================
// 접근자 배열. (원본 readAccessorArray 가 돌려주는 타입 배열, 컴포넌트 타입 그대로)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum AccessorArray {
	unsignedByte(Vec<u8>),
	unsignedShort(Vec<u16>),
	unsignedInt(Vec<u32>),
	float(Vec<f32>),
}

impl AccessorArray {
	//==============================================================================
	// 원소(컴포넌트) 개수.
	//==============================================================================
	pub fn len(&self) -> usize {
		match self {
			AccessorArray::unsignedByte(values) => {
				values.len()
			}
			AccessorArray::unsignedShort(values) => {
				values.len()
			}
			AccessorArray::unsignedInt(values) => {
				values.len()
			}
			AccessorArray::float(values) => {
				values.len()
			}
		}
	}

	//==============================================================================
	// 비었는지.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	//==============================================================================
	// 컴포넌트 타입. (5121 / 5123 / 5125 / 5126)
	//==============================================================================
	pub fn getComponentType(&self) -> u32 {
		match self {
			AccessorArray::unsignedByte(_) => {
				COMPONENT_TYPE_UNSIGNED_BYTE
			}
			AccessorArray::unsignedShort(_) => {
				COMPONENT_TYPE_UNSIGNED_SHORT
			}
			AccessorArray::unsignedInt(_) => {
				COMPONENT_TYPE_UNSIGNED_INT
			}
			AccessorArray::float(_) => {
				COMPONENT_TYPE_FLOAT
			}
		}
	}

	//==============================================================================
	// 실수 목록으로. (원본 new Float32Array(typedArray), 정수는 정규화하지 않고 값 그대로)
	//==============================================================================
	pub fn toFloatList(&self) -> Vec<f32> {
		match self {
			AccessorArray::unsignedByte(values) => {
				values.iter().map(|value| *value as f32).collect()
			}
			AccessorArray::unsignedShort(values) => {
				values.iter().map(|value| *value as f32).collect()
			}
			AccessorArray::unsignedInt(values) => {
				values.iter().map(|value| *value as f32).collect()
			}
			AccessorArray::float(values) => {
				values.clone()
			}
		}
	}

	//==============================================================================
	// 16비트 정수 목록으로. (원본 new Uint16Array(typedArray))
	//==============================================================================
	pub fn toUnsignedShortList(&self) -> Vec<u16> {
		match self {
			AccessorArray::unsignedByte(values) => {
				values.iter().map(|value| *value as u16).collect()
			}
			AccessorArray::unsignedShort(values) => {
				values.clone()
			}
			AccessorArray::unsignedInt(values) => {
				values.iter().map(|value| *value as u16).collect()
			}
			AccessorArray::float(values) => {
				values.iter().map(|value| *value as u16).collect()
			}
		}
	}

	//==============================================================================
	// 인덱스 배열로. (원본: Uint8 은 Uint16 으로 넓히고 나머지는 그대로)
	//==============================================================================
	pub fn toIndexArray(&self) -> IndexArray {
		match self {
			AccessorArray::unsignedInt(values) => {
				IndexArray::unsignedInt(values.clone())
			}
			_ => {
				IndexArray::unsignedShort(self.toUnsignedShortList())
			}
		}
	}
}

//==============================================================================
// 중립 메시 서술. (원본 meshDescription, 순수 CPU 배열)
// - joints / weights 는 정점마다 4개, materialIndex 가 None 이면 기본 머티리얼, nodeIndex 는 모프 가중치 채널의 대상 노드.
//==============================================================================
#[derive(Clone, Debug)]
pub struct MeshDescription {
	pub positions: Vec<f32>,
	pub normals: Option<Vec<f32>>,
	pub textureCoordinates: Option<Vec<f32>>,
	pub joints: Vec<u16>,
	pub weights: Vec<f32>,
	pub indices: Option<IndexArray>,
	pub skinIndex: usize,
	pub materialIndex: Option<usize>,
	pub materialName: String,
	pub nodeIndex: Option<usize>,
	pub morphTargets: Vec<MorphTarget>,
	pub morphWeights: Vec<f32>,
}

//==============================================================================
// 드로어블. (원본 drawableList 원소, 버텍스 어레이, 인덱스, 스킨, 머티리얼 참조, 모프 상태)
//==============================================================================
pub struct Drawable {
	pub vertexArray: GlVertexArray,
	pub vertexBufferList: Vec<GlBuffer>,
	pub isIndexed: bool,
	pub indexBuffer: Option<GlBuffer>,
	pub indexCount: usize,
	pub indexComponentType: u32,
	pub indexByteOffset: i32,
	pub wireframeIndexBuffer: Option<GlBuffer>,
	pub wireframeIndexCount: usize,
	pub wireframeIndexComponentType: u32,
	pub vertexCount: usize,
	pub material: Rc<Material>,
	pub materialName: String,
	pub skinIndex: usize,
	pub nodeIndex: Option<usize>,
	pub meshDescription: MeshDescription,
	pub morphTargetList: Vec<MorphTarget>,
	pub morphWeights: Vec<f32>,
	pub morphTexture: Option<GlTexture>,
	pub morphRowsPerTarget: i32,
	pub morphTextureWidth: i32,
	pub isMorphDirty: bool,
}

//==============================================================================
// 애니메이션 채널. (노드 인덱스 + 경로 + 시간/값 커브 + 키프레임 커서)
//==============================================================================
#[derive(Clone, Debug)]
pub struct AnimationChannel {
	pub nodeIndex: usize,
	pub path: AnimationPath,
	pub times: Vec<f32>,
	pub values: Vec<f32>,
	pub cursor: usize,
}

//==============================================================================
// 애니메이션 클립.
//==============================================================================
#[derive(Clone, Debug)]
pub struct AnimationClip {
	pub name: String,
	pub duration: f32,
	pub channels: Vec<AnimationChannel>,
}

//==============================================================================
// 스킨드 모델.
//==============================================================================
pub struct SkinnedModel {
	shaderProgram: Option<Rc<ShaderProgram>>,
	nodeList: Vec<SkinnedModelNode>,
	rootNodeIndices: Vec<usize>,
	skinList: Vec<Skin>,
	drawableList: Vec<Drawable>,
	animationList: Vec<AnimationClip>,
	currentAnimationIndex: Option<usize>,
	previousAnimationIndex: Option<usize>,
	currentTime: f32,
	previousTime: f32,
	fadeDuration: f32,
	fadeElapsed: f32,
	timeScale: f32,
	jointRotationOffsets: Vec<(usize, Quaternion)>,
}

impl SkinnedModel {
	//==============================================================================
	// 생성. (셰이더 프로그램은 렌더러의 템플릿, renderer.getShaderProgram())
	//==============================================================================
	pub fn new(shaderProgram: Rc<ShaderProgram>) -> SkinnedModel {
		let mut skinnedModel = SkinnedModel::createWithoutShaderProgram();
		skinnedModel.shaderProgram = Some(shaderProgram);
		skinnedModel
	}

	//==============================================================================
	// 셰이더 없이 생성. (원본에 없는 것, 해석과 포즈 계산만 하는 모델, uploadMeshesAndMaterials 는 오류)
	//==============================================================================
	pub fn createWithoutShaderProgram() -> SkinnedModel {
		SkinnedModel {
			shaderProgram: None,
			nodeList: Vec::new(),
			rootNodeIndices: Vec::new(),
			skinList: Vec::new(),
			drawableList: Vec::new(),
			animationList: Vec::new(),
			currentAnimationIndex: None,
			previousAnimationIndex: None,
			currentTime: 0.0,
			previousTime: 0.0,
			fadeDuration: 0.0,
			fadeElapsed: 0.0,
			timeScale: 1.0,
			jointRotationOffsets: Vec::new(),
		}
	}

	//==============================================================================
	// 바이트로부터 로드. (GLB 바이너리 glTF, 원본 정적 loadFromUrl 의 fetch 뒤 부분)
	//==============================================================================
	pub fn loadFromBytes(gl: &glow::Context, bytes: &[u8], skinnedModelRenderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
		let mut skinnedModel = SkinnedModel::new(skinnedModelRenderer.getShaderProgram());
		skinnedModel.parseBinary(gl, bytes)?;
		Ok(skinnedModel)
	}

	//==============================================================================
	// 바이트로부터 로드. (바이너리 FBX, 원본 정적 loadFromFbxUrl 의 fetch 뒤 부분)
	//==============================================================================
	pub fn loadFromFbxBytes(gl: &glow::Context, bytes: &[u8], skinnedModelRenderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
		let sceneData = FbxLoader::new().parse(bytes)?;
		let mut skinnedModel = SkinnedModel::new(skinnedModelRenderer.getShaderProgram());
		skinnedModel.parseFbxContent(gl, &sceneData)?;
		Ok(skinnedModel)
	}

	//==============================================================================
	// 파일로부터 로드. (GLB, 원본 정적 loadFromUrl 의 데스크탑 판)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub fn loadFromPath(gl: &glow::Context, path: &std::path::Path, skinnedModelRenderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
		let bytes = std::fs::read(path).map_err(|error| format!("SkinnedModel load failed: {} ({})", path.display(), error))?;
		SkinnedModel::loadFromBytes(gl, &bytes, skinnedModelRenderer)
	}

	//==============================================================================
	// 파일로부터 로드. (바이너리 FBX, 원본 정적 loadFromFbxUrl 의 데스크탑 판)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub fn loadFromFbxPath(gl: &glow::Context, path: &std::path::Path, skinnedModelRenderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
		let sceneData = FbxLoader::loadFromPath(path)?;
		let mut skinnedModel = SkinnedModel::new(skinnedModelRenderer.getShaderProgram());
		skinnedModel.parseFbxContent(gl, &sceneData)?;
		Ok(skinnedModel)
	}

	//==============================================================================
	// GLB 청크 읽기. (머리 확인 + JSON 청크 + BIN 청크, GL 무관)
	//==============================================================================
	pub fn readGlbChunks(bytes: &[u8]) -> Result<(Value, Vec<u8>), String> {
		if readUint32(bytes, 0) != Some(GLB_MAGIC) {
			return Err(String::from("SkinnedModel: not a GLB file."));
		}

		// 청크 순회. (JSON + BIN)
		let mut json: Option<Value> = None;
		let mut binaryBuffer: Option<Vec<u8>> = None;
		let mut chunkOffset = 12usize;
		while chunkOffset < bytes.len() {
			let chunkLength = readUint32(bytes, chunkOffset).ok_or_else(|| String::from("SkinnedModel: invalid GLB chunks."))? as usize;
			let chunkType = readUint32(bytes, chunkOffset + 4).ok_or_else(|| String::from("SkinnedModel: invalid GLB chunks."))?;
			let chunkStart = chunkOffset + 8;
			let chunkEnd = chunkStart.checked_add(chunkLength).filter(|chunkEnd| *chunkEnd <= bytes.len()).ok_or_else(|| String::from("SkinnedModel: invalid GLB chunks."))?;
			if chunkType == GLB_CHUNKTYPE_JSON {
				let jsonText = String::from_utf8_lossy(&bytes[chunkStart..chunkEnd]);
				let parsedJson: Value = serde_json::from_str(&jsonText).map_err(|error| format!("SkinnedModel: invalid glTF JSON: {}", error))?;
				json = Some(parsedJson);
			}
			else if chunkType == GLB_CHUNKTYPE_BINARY {
				binaryBuffer = Some(bytes[chunkStart..chunkEnd].to_vec());
			}
			chunkOffset = chunkEnd;
		}
		match (json, binaryBuffer) {
			(Some(json), Some(binaryBuffer)) => {
				Ok((json, binaryBuffer))
			}
			_ => {
				Err(String::from("SkinnedModel: invalid GLB chunks."))
			}
		}
	}

	//==============================================================================
	// GLB 바이너리 해석. (청크 읽기 → 내용 해석 → GL 업로드)
	//==============================================================================
	pub fn parseBinary(&mut self, gl: &glow::Context, bytes: &[u8]) -> Result<(), String> {
		let (json, binaryBuffer) = SkinnedModel::readGlbChunks(bytes)?;
		self.parseContent(gl, &json, &binaryBuffer)
	}

	//==============================================================================
	// glTF 내용 해석. (노드 / 스킨 / 메시 / 애니메이션 / 텍스처 → GL 업로드)
	//==============================================================================
	pub fn parseContent(&mut self, gl: &glow::Context, json: &Value, binaryBuffer: &[u8]) -> Result<(), String> {
		let (meshDescriptionList, materialDescriptionList) = self.interpretContent(json, binaryBuffer)?;
		self.uploadMeshesAndMaterials(gl, meshDescriptionList, materialDescriptionList)
	}

	//==============================================================================
	// glTF 내용 해석. (GL 무관, 노드 / 스킨 / 애니메이션을 채우고 중립 메시 / 머티리얼 서술을 돌려준다)
	//==============================================================================
	pub fn interpretContent(&mut self, json: &Value, binaryBuffer: &[u8]) -> Result<(Vec<MeshDescription>, Vec<MaterialDescription>), String> {
		// 노드 구성.
		let nodeValueList = json["nodes"].as_array().ok_or_else(|| String::from("SkinnedModel: glTF has no nodes."))?;
		let mut nodeList = Vec::with_capacity(nodeValueList.len());
		for nodeValue in nodeValueList.iter() {
			let rotation = match readFloatArray(&nodeValue["rotation"]) {
				Some(rotationValues) if rotationValues.len() >= 4 => {
					Quaternion::create(rotationValues[0], rotationValues[1], rotationValues[2], rotationValues[3])
				}
				_ => {
					Quaternion::identity()
				}
			};
			nodeList.push(SkinnedModelNode {
				name: nodeValue["name"].as_str().unwrap_or("").to_string(),
				childIndices: readIndexArray(&nodeValue["children"]),
				meshIndex: readIndex(&nodeValue["mesh"]),
				skinIndex: readIndex(&nodeValue["skin"]),
				baseTranslation: readVector3(&nodeValue["translation"], [0.0, 0.0, 0.0]),
				baseRotation: rotation,
				baseScale: readVector3(&nodeValue["scale"], [1.0, 1.0, 1.0]),
				currentTranslation: [0.0, 0.0, 0.0],
				currentRotation: Quaternion::identity(),
				currentScale: [1.0, 1.0, 1.0],
				worldMatrix: Matrix4::createIdentity(),
			});
		}
		self.nodeList = nodeList;
		let sceneIndex = readIndex(&json["scene"]).unwrap_or(0);
		let scene = json["scenes"].get(sceneIndex).ok_or_else(|| String::from("SkinnedModel: glTF scene not found."))?;
		self.rootNodeIndices = readIndexArray(&scene["nodes"]);

		// 스킨 구성.
		let mut skinList = Vec::new();
		if let Some(skinValueList) = json["skins"].as_array() {
			for skinValue in skinValueList.iter() {
				let jointNodeIndices = readIndexArray(&skinValue["joints"]);
				let inverseBindArray = match readIndex(&skinValue["inverseBindMatrices"]) {
					Some(accessorIndex) => {
						readAccessorArray(json, binaryBuffer, accessorIndex)?.toFloatList()
					}
					None => {
						Vec::new()
					}
				};
				let mut inverseBindMatrices = Vec::with_capacity(jointNodeIndices.len());
				for jointIndex in 0..jointNodeIndices.len() {
					let mut matrix = Matrix4::createIdentity();
					if let Some(matrixValues) = inverseBindArray.get(jointIndex * 16..jointIndex * 16 + 16) {
						matrix.elements.copy_from_slice(matrixValues);
					}
					inverseBindMatrices.push(matrix);
				}
				skinList.push(Skin {
					jointMatrixArray: vec![0.0; jointNodeIndices.len() * 16],
					jointNodeIndices,
					inverseBindMatrices,
				});
			}
		}
		self.skinList = skinList;

		// 중립 머티리얼 서술 구성. (glTF metallic-roughness → 그대로 매핑)
		let mut materialDescriptionList = Vec::new();
		if let Some(materialValueList) = json["materials"].as_array() {
			for materialValue in materialValueList.iter() {
				let mut description = Material::createDefaultDescription();
				let pbrMetallicRoughness = &materialValue["pbrMetallicRoughness"];
				if pbrMetallicRoughness.is_object() {
					if let Some(baseColorFactor) = readFloatArray(&pbrMetallicRoughness["baseColorFactor"]) {
						if baseColorFactor.len() >= 3 {
							description.baseColorFactor = [baseColorFactor[0], baseColorFactor[1], baseColorFactor[2]];
						}
					}
					description.metallicFactor = pbrMetallicRoughness["metallicFactor"].as_f64().map(|value| value as f32).unwrap_or(1.0);
					description.roughnessFactor = pbrMetallicRoughness["roughnessFactor"].as_f64().map(|value| value as f32).unwrap_or(1.0);
					description.baseColorImage = createGltfImageDescription(json, binaryBuffer, &pbrMetallicRoughness["baseColorTexture"]);
					description.metallicRoughnessImage = createGltfImageDescription(json, binaryBuffer, &pbrMetallicRoughness["metallicRoughnessTexture"]);
				}
				description.normalImage = createGltfImageDescription(json, binaryBuffer, &materialValue["normalTexture"]);
				description.occlusionImage = createGltfImageDescription(json, binaryBuffer, &materialValue["occlusionTexture"]);
				description.emissiveImage = createGltfImageDescription(json, binaryBuffer, &materialValue["emissiveTexture"]);
				if let Some(emissiveFactor) = readFloatArray(&materialValue["emissiveFactor"]) {
					if emissiveFactor.len() >= 3 {
						description.emissiveFactor = [emissiveFactor[0], emissiveFactor[1], emissiveFactor[2]];
					}
				}
				materialDescriptionList.push(description);
			}
		}

		// 중립 메시 서술 구성. (메시 노드의 프리미티브, 접근자를 CPU 배열로 펼침)
		// - 스킨 없는 정적 메시는 노드 자신을 유일한 조인트로 갖는 합성 스킨을 붙여 같은 스키닝 경로로 그린다.
		let mut meshDescriptionList = Vec::new();
		for nodeIndex in 0..self.nodeList.len() {
			let meshIndex = match self.nodeList[nodeIndex].meshIndex {
				Some(meshIndex) => {
					meshIndex
				}
				None => {
					continue;
				}
			};
			let meshValue = json["meshes"].get(meshIndex).ok_or_else(|| format!("SkinnedModel: mesh {} not found.", meshIndex))?;
			let isStaticMesh = self.nodeList[nodeIndex].skinIndex.is_none();
			if isStaticMesh {
				self.skinList.push(Skin {
					jointNodeIndices: vec![nodeIndex],
					inverseBindMatrices: vec![Matrix4::createIdentity()],
					jointMatrixArray: vec![0.0; 16],
				});
				self.nodeList[nodeIndex].skinIndex = Some(self.skinList.len() - 1);
			}
			let skinIndex = self.nodeList[nodeIndex].skinIndex.unwrap_or(0);
			let targetNameList: Vec<String> = match meshValue["extras"]["targetNames"].as_array() {
				Some(targetNameValues) => {
					targetNameValues.iter().map(|targetNameValue| targetNameValue.as_str().unwrap_or("").to_string()).collect()
				}
				None => {
					Vec::new()
				}
			};
			// 기본 모프 가중치. (원본은 내부 노드 객체에서 weights 를 찾아 늘 비어 있으므로 메시 weights 만 쓴다)
			let baseMorphWeights = readFloatArray(&meshValue["weights"]).unwrap_or_default();
			let primitiveValueList = meshValue["primitives"].as_array().map(|primitiveValues| primitiveValues.as_slice()).unwrap_or(&[]);
			for primitiveValue in primitiveValueList.iter() {
				let attributes = &primitiveValue["attributes"];
				let positionAccessorIndex = readIndex(&attributes["POSITION"]).ok_or_else(|| String::from("SkinnedModel: primitive has no POSITION."))?;
				let positions = readAccessorArray(json, binaryBuffer, positionAccessorIndex)?.toFloatList();
				let vertexCount = positions.len() / 3;
				let normals = match readIndex(&attributes["NORMAL"]) {
					Some(accessorIndex) => {
						Some(readAccessorArray(json, binaryBuffer, accessorIndex)?.toFloatList())
					}
					None => {
						None
					}
				};
				let textureCoordinates = match readIndex(&attributes["TEXCOORD_0"]) {
					Some(accessorIndex) => {
						Some(readAccessorArray(json, binaryBuffer, accessorIndex)?.toFloatList())
					}
					None => {
						None
					}
				};
				let joints;
				let mut weights;
				if isStaticMesh {
					joints = vec![0u16; vertexCount * 4];
					weights = vec![0.0f32; vertexCount * 4];
					for vertexIndex in 0..vertexCount {
						weights[vertexIndex * 4] = 1.0;
					}
				}
				else {
					let jointAccessorIndex = readIndex(&attributes["JOINTS_0"]).ok_or_else(|| String::from("SkinnedModel: skinned primitive has no JOINTS_0."))?;
					let weightAccessorIndex = readIndex(&attributes["WEIGHTS_0"]).ok_or_else(|| String::from("SkinnedModel: skinned primitive has no WEIGHTS_0."))?;
					joints = readAccessorArray(json, binaryBuffer, jointAccessorIndex)?.toUnsignedShortList();
					let weightSource = readAccessorArray(json, binaryBuffer, weightAccessorIndex)?;
					weights = weightSource.toFloatList();
					let componentType = weightSource.getComponentType();
					if componentType != COMPONENT_TYPE_FLOAT {
						let normalizeDivisor = if componentType == COMPONENT_TYPE_UNSIGNED_BYTE {
							255.0
						}
						else {
							65535.0
						};
						for weight in weights.iter_mut() {
							*weight /= normalizeDivisor;
						}
					}
				}
				let indices = match readIndex(&primitiveValue["indices"]) {
					Some(accessorIndex) => {
						Some(readAccessorArray(json, binaryBuffer, accessorIndex)?.toIndexArray())
					}
					None => {
						None
					}
				};

				// 모프 타깃. (위치 델타 필수, 노멀 델타 선택, 이름은 mesh.extras.targetNames)
				let mut morphTargets = Vec::new();
				if let Some(targetValueList) = primitiveValue["targets"].as_array() {
					for (targetIndex, targetValue) in targetValueList.iter().enumerate() {
						let positionAccessorIndex = match readIndex(&targetValue["POSITION"]) {
							Some(positionAccessorIndex) => {
								positionAccessorIndex
							}
							None => {
								continue;
							}
						};
						let positionDeltas = readAccessorArray(json, binaryBuffer, positionAccessorIndex)?.toFloatList();
						let normalDeltas = match readIndex(&targetValue["NORMAL"]) {
							Some(accessorIndex) => {
								Some(readAccessorArray(json, binaryBuffer, accessorIndex)?.toFloatList())
							}
							None => {
								None
							}
						};
						let targetName = match targetNameList.get(targetIndex) {
							Some(targetName) => {
								targetName.clone()
							}
							None => {
								format!("target{}", targetIndex)
							}
						};
						morphTargets.push(MorphTarget { name: targetName, positionDeltas, normalDeltas });
					}
				}
				let mut morphWeights = vec![0.0f32; morphTargets.len()];
				for (weightIndex, morphWeight) in morphWeights.iter_mut().enumerate() {
					*morphWeight = baseMorphWeights.get(weightIndex).copied().unwrap_or(0.0);
				}

				let materialIndex = readIndex(&primitiveValue["material"]);
				let materialName = match materialIndex {
					Some(materialIndex) => {
						json["materials"][materialIndex]["name"].as_str().unwrap_or("").to_string()
					}
					None => {
						String::new()
					}
				};
				meshDescriptionList.push(MeshDescription {
					positions,
					normals,
					textureCoordinates,
					joints,
					weights,
					indices,
					skinIndex,
					materialIndex,
					materialName,
					nodeIndex: Some(nodeIndex),
					morphTargets,
					morphWeights,
				});
			}
		}

		// 애니메이션 구성.
		let mut animationList = Vec::new();
		if let Some(animationValueList) = json["animations"].as_array() {
			for animationValue in animationValueList.iter() {
				let mut duration = 0.0f32;
				let mut channels = Vec::new();
				let channelValueList = animationValue["channels"].as_array().map(|channelValues| channelValues.as_slice()).unwrap_or(&[]);
				for channelValue in channelValueList.iter() {
					let samplerIndex = readIndex(&channelValue["sampler"]).ok_or_else(|| String::from("SkinnedModel: animation channel has no sampler."))?;
					let samplerValue = &animationValue["samplers"][samplerIndex];
					let inputAccessorIndex = readIndex(&samplerValue["input"]).ok_or_else(|| String::from("SkinnedModel: animation sampler has no input."))?;
					let outputAccessorIndex = readIndex(&samplerValue["output"]).ok_or_else(|| String::from("SkinnedModel: animation sampler has no output."))?;
					let times = readAccessorArray(json, binaryBuffer, inputAccessorIndex)?.toFloatList();
					let values = readAccessorArray(json, binaryBuffer, outputAccessorIndex)?.toFloatList();
					if let Some(lastTime) = times.last() {
						duration = duration.max(*lastTime);
					}
					let nodeIndex = readIndex(&channelValue["target"]["node"]);
					let path = channelValue["target"]["path"].as_str().and_then(AnimationPath::fromName);
					if let (Some(nodeIndex), Some(path)) = (nodeIndex, path) {
						channels.push(AnimationChannel { nodeIndex, path, times, values, cursor: 0 });
					}
				}
				animationList.push(AnimationClip {
					name: animationValue["name"].as_str().unwrap_or("").to_string(),
					duration,
					channels,
				});
			}
		}
		self.animationList = animationList;
		if !self.animationList.is_empty() {
			self.currentAnimationIndex = Some(0);
		}
		Ok((meshDescriptionList, materialDescriptionList))
	}

	//==============================================================================
	// FBX 중간 표현 해석. (노드/스킨/메시/텍스처/애니메이션 → 기존 스키닝 파이프라인)
	//==============================================================================
	pub fn parseFbxContent(&mut self, gl: &glow::Context, sceneData: &FbxSceneData) -> Result<(), String> {
		let (meshDescriptionList, materialDescriptionList) = self.interpretFbxContent(sceneData)?;
		self.uploadMeshesAndMaterials(gl, meshDescriptionList, materialDescriptionList)
	}

	//==============================================================================
	// FBX 중간 표현 해석. (GL 무관, 노드 / 스킨 / 애니메이션을 채우고 중립 메시 / 머티리얼 서술을 돌려준다)
	//==============================================================================
	pub fn interpretFbxContent(&mut self, sceneData: &FbxSceneData) -> Result<(Vec<MeshDescription>, Vec<MaterialDescription>), String> {
		// 노드 구성. (오일러 도 → 쿼터니언, 사전회전 우측 누적)
		self.nodeList = sceneData.nodeList.iter().map(|fbxNode| {
			let rotationQuaternion = createQuaternionFromFbxEuler(fbxNode.rotationEulerDegrees[0], fbxNode.rotationEulerDegrees[1], fbxNode.rotationEulerDegrees[2]);
			let preRotationQuaternion = createQuaternionFromFbxEuler(fbxNode.preRotationDegrees[0], fbxNode.preRotationDegrees[1], fbxNode.preRotationDegrees[2]);
			let baseRotation = preRotationQuaternion.multiply(rotationQuaternion);
			SkinnedModelNode {
				name: fbxNode.name.clone(),
				childIndices: fbxNode.childIndices.clone(),
				meshIndex: None,
				skinIndex: None,
				baseTranslation: [fbxNode.translation[0] as f32, fbxNode.translation[1] as f32, fbxNode.translation[2] as f32],
				baseRotation,
				baseScale: [fbxNode.scale[0] as f32, fbxNode.scale[1] as f32, fbxNode.scale[2] as f32],
				currentTranslation: [0.0, 0.0, 0.0],
				currentRotation: Quaternion::identity(),
				currentScale: [1.0, 1.0, 1.0],
				worldMatrix: Matrix4::createIdentity(),
			}
		}).collect();
		self.rootNodeIndices = sceneData.rootNodeIndices.clone();

		// 스킨 구성. (클러스터 → 조인트 노드 인덱스 + 링크 변환의 역행렬)
		let skinIdList = sceneData.skinDeformerById.keys().to_vec();
		let mut skinList = Vec::with_capacity(skinIdList.len());
		for skinId in skinIdList.iter() {
			let skinDeformer = match sceneData.skinDeformerById.get(*skinId) {
				Some(skinDeformer) => {
					skinDeformer
				}
				None => {
					continue;
				}
			};
			let mut jointNodeIndices = Vec::with_capacity(skinDeformer.clusterIds.len());
			let mut inverseBindMatrices = Vec::with_capacity(skinDeformer.clusterIds.len());
			for clusterId in skinDeformer.clusterIds.iter() {
				let cluster = sceneData.clusterById.get(*clusterId);
				let boneNodeIndex = cluster.and_then(|cluster| cluster.boneNodeId).and_then(|boneNodeId| sceneData.nodeIdToIndex.get(&boneNodeId).copied());
				jointNodeIndices.push(boneNodeIndex.unwrap_or(usize::MAX));
				// 클러스터 Transform = TransformLink^-1 x 메시 바인드 전역행렬, 그 자체가 역바인드 행렬.
				// 미보유 시 TransformLink 의 역행렬로 대체. (메시 전역 = 단위행렬 가정)
				let mut inverseBindMatrix = Matrix4::createIdentity();
				if let Some(cluster) = cluster {
					if let Some(transform) = cluster.transform.as_ref().filter(|transform| transform.len() >= 16) {
						for (elementIndex, element) in inverseBindMatrix.elements.iter_mut().enumerate() {
							*element = transform[elementIndex] as f32;
						}
					}
					else if let Some(transformLink) = cluster.transformLink.as_ref().filter(|transformLink| transformLink.len() >= 16) {
						let mut linkMatrix = Matrix4::createIdentity();
						for (elementIndex, element) in linkMatrix.elements.iter_mut().enumerate() {
							*element = transformLink[elementIndex] as f32;
						}
						inverseBindMatrix = Matrix4::createInverse(&linkMatrix);
					}
				}
				inverseBindMatrices.push(inverseBindMatrix);
			}
			skinList.push(Skin {
				jointMatrixArray: vec![0.0; jointNodeIndices.len() * 16],
				jointNodeIndices,
				inverseBindMatrices,
			});
		}
		self.skinList = skinList;

		// 이미지 서술 구성 헬퍼. (텍스처 id → 임베디드 이미지 바이트, FBX 는 상하 반전 필요)
		let createFbxImageDescription = |textureId: Option<i64>| -> Option<ImageDescription> {
			let texture = sceneData.textureById.get(textureId?)?;
			let video = sceneData.videoById.get(texture.videoId?)?;
			let contentBytes = video.contentBytes.as_ref().filter(|contentBytes| !contentBytes.is_empty())?;
			Some(ImageDescription {
				bytes: contentBytes.clone(),
				mimeType: None,
				source: None,
				flipY: true,
				isColorData: false,
				anisotropy: 0.0,
			})
		};

		// 중립 머티리얼 서술 구성. (FBX phong → 비금속 + 스펙큘러/글로스 근사 경로)
		let mut materialDescriptionList = Vec::with_capacity(sceneData.materialById.len());
		for fbxMaterial in sceneData.materialById.values().iter() {
			let mut description = Material::createDefaultDescription();
			description.baseColorFactor = [fbxMaterial.diffuseColor[0] as f32, fbxMaterial.diffuseColor[1] as f32, fbxMaterial.diffuseColor[2] as f32];
			description.baseColorImage = createFbxImageDescription(fbxMaterial.diffuseTextureId);
			description.normalImage = createFbxImageDescription(fbxMaterial.normalTextureId);
			description.specularImage = createFbxImageDescription(fbxMaterial.specularTextureId);
			description.glossinessImage = createFbxImageDescription(fbxMaterial.glossTextureId);
			materialDescriptionList.push(description);
		}

		// 스킨드 메시 → 중립 메시 서술 구성.
		let mut meshDescriptionList = Vec::new();
		for (skinListIndex, skinId) in skinIdList.iter().enumerate() {
			let skinDeformer = match sceneData.skinDeformerById.get(*skinId) {
				Some(skinDeformer) => {
					skinDeformer
				}
				None => {
					continue;
				}
			};
			let geometry = match skinDeformer.geometryId.and_then(|geometryId| sceneData.geometryById.get(geometryId)) {
				Some(geometry) => {
					geometry
				}
				None => {
					continue;
				}
			};

			// 정점별 스킨 영향 누적. (클러스터 로컬 조인트 인덱스 + 가중치)
			let originalVertexCount = geometry.originalVertexCount;
			let mut influencesPerVertex: Vec<Vec<(u16, f64)>> = vec![Vec::new(); originalVertexCount];
			for (clusterLocalIndex, clusterId) in skinDeformer.clusterIds.iter().enumerate() {
				let cluster = match sceneData.clusterById.get(*clusterId) {
					Some(cluster) => {
						cluster
					}
					None => {
						continue;
					}
				};
				for (influenceIndex, vertexIndex) in cluster.vertexIndices.iter().enumerate() {
					let weight = cluster.weights.get(influenceIndex).copied().unwrap_or(0.0);
					if *vertexIndex >= 0 && (*vertexIndex as usize) < originalVertexCount {
						influencesPerVertex[*vertexIndex as usize].push((clusterLocalIndex as u16, weight));
					}
				}
			}

			// 정점별 상위 4개 선택 + 정규화.
			let mut vertexJointArray = vec![0u16; originalVertexCount * 4];
			let mut vertexWeightArray = vec![0.0f32; originalVertexCount * 4];
			for (vertexIndex, influenceList) in influencesPerVertex.iter_mut().enumerate() {
				influenceList.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(std::cmp::Ordering::Equal));
				let usedCount = influenceList.len().min(4);
				let mut weightSum: f64 = influenceList.iter().take(usedCount).map(|influence| influence.1).sum();
				if weightSum <= 0.0 {
					weightSum = 1.0;
				}
				for slotIndex in 0..usedCount {
					vertexJointArray[vertexIndex * 4 + slotIndex] = influenceList[slotIndex].0;
					vertexWeightArray[vertexIndex * 4 + slotIndex] = (influenceList[slotIndex].1 / weightSum) as f32;
				}
			}

			// 코너 단위(비인덱스)로 조인트/가중치 펼침.
			let cornerCount = geometry.cornerCount;
			let mut cornerJointArray = vec![0u16; cornerCount * 4];
			let mut cornerWeightArray = vec![0.0f32; cornerCount * 4];
			for cornerIndex in 0..cornerCount {
				let originalVertexIndex = geometry.originalVertexIndices[cornerIndex];
				if originalVertexIndex < 0 || originalVertexIndex as usize >= originalVertexCount {
					continue;
				}
				let sourceOffset = originalVertexIndex as usize * 4;
				cornerJointArray[cornerIndex * 4..cornerIndex * 4 + 4].copy_from_slice(&vertexJointArray[sourceOffset..sourceOffset + 4]);
				cornerWeightArray[cornerIndex * 4..cornerIndex * 4 + 4].copy_from_slice(&vertexWeightArray[sourceOffset..sourceOffset + 4]);
			}

			// 머티리얼 인덱스 조회. (지오메트리 소유 노드의 첫 머티리얼)
			let ownerNodeIndex = sceneData.nodeList.iter().position(|fbxNode| fbxNode.geometryId.is_some() && fbxNode.geometryId == skinDeformer.geometryId);
			let materialIndex = ownerNodeIndex.and_then(|ownerNodeIndex| sceneData.nodeList[ownerNodeIndex].materialIds.first().copied()).and_then(|materialId| sceneData.materialById.indexOf(materialId));

			meshDescriptionList.push(MeshDescription {
				positions: geometry.positions.clone(),
				normals: Some(geometry.normals.clone()),
				textureCoordinates: Some(geometry.textureCoordinates.clone()),
				joints: cornerJointArray,
				weights: cornerWeightArray,
				indices: None,
				skinIndex: skinListIndex,
				materialIndex,
				materialName: String::new(),
				nodeIndex: ownerNodeIndex,
				morphTargets: Vec::new(),
				morphWeights: Vec::new(),
			});
		}

		// 애니메이션 구성. (회전 채널은 오일러 도 → 사전회전 합성 쿼터니언 4성분으로 변환)
		self.animationList = sceneData.animationList.iter().map(|fbxAnimation| {
			let channels = convertFbxAnimationChannels(sceneData, fbxAnimation, Some);
			AnimationClip {
				name: fbxAnimation.name.clone(),
				duration: fbxAnimation.duration,
				channels,
			}
		}).collect();
		if !self.animationList.is_empty() {
			self.currentAnimationIndex = Some(0);
		}
		Ok((meshDescriptionList, materialDescriptionList))
	}

	//==============================================================================
	// 애니메이션 FBX 추가. (본 이름 매칭 리타게팅, Mixamo 애니메이션 전용 FBX 의 중간 표현)
	// - 동일 스켈레톤을 전제로, 채널을 현재 모델의 노드 인덱스로 재지정해 클립을 추가한다.
	// - ignoreTranslation: 루트 모션이 박힌 클립에서 이동 채널을 버린다. (게임 로직이 위치를 제어할 때)
	//==============================================================================
	pub fn addAnimationsFromFbxSceneData(&mut self, sceneData: &FbxSceneData, clipName: &str, ignoreTranslation: bool) {
		let mut nodeIndexByName: HashMap<&str, usize> = HashMap::new();
		for (nodeIndex, node) in self.nodeList.iter().enumerate() {
			nodeIndexByName.insert(node.name.as_str(), nodeIndex);
		}
		let mut addedClipList = Vec::new();
		for fbxAnimation in sceneData.animationList.iter() {
			if fbxAnimation.channels.is_empty() {
				continue;
			}
			let mut channels = convertFbxAnimationChannels(sceneData, fbxAnimation, |nodeIndex| {
				let sourceName = sceneData.nodeList.get(nodeIndex)?.name.as_str();
				nodeIndexByName.get(sourceName).copied()
			});
			if ignoreTranslation {
				channels.retain(|channel| channel.path != AnimationPath::translation);
			}
			addedClipList.push(AnimationClip {
				name: clipName.to_string(),
				duration: fbxAnimation.duration,
				channels,
			});
		}
		self.animationList.append(&mut addedClipList);
	}

	//==============================================================================
	// 애니메이션 FBX 추가. (바이트, 원본 addAnimationsFromFbxUrl 의 fetch 뒤 부분)
	//==============================================================================
	pub fn addAnimationsFromFbxBytes(&mut self, bytes: &[u8], clipName: &str, ignoreTranslation: bool) -> Result<(), String> {
		let sceneData = FbxLoader::new().parse(bytes)?;
		self.addAnimationsFromFbxSceneData(&sceneData, clipName, ignoreTranslation);
		Ok(())
	}

	//==============================================================================
	// 애니메이션 FBX 추가. (파일, 원본 addAnimationsFromFbxUrl 의 데스크탑 판)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub fn addAnimationsFromFbxPath(&mut self, path: &std::path::Path, clipName: &str, ignoreTranslation: bool) -> Result<(), String> {
		let sceneData = FbxLoader::loadFromPath(path)?;
		self.addAnimationsFromFbxSceneData(&sceneData, clipName, ignoreTranslation);
		Ok(())
	}

	//==============================================================================
	// 중립 메시/머티리얼 서술 업로드. (공용, Material 생성 + 버텍스 어레이/드로어블 구성)
	//==============================================================================
	pub fn uploadMeshesAndMaterials(&mut self, gl: &glow::Context, meshDescriptionList: Vec<MeshDescription>, materialDescriptionList: Vec<MaterialDescription>) -> Result<(), String> {
		let shaderProgram = match &self.shaderProgram {
			Some(shaderProgram) => {
				shaderProgram.clone()
			}
			None => {
				return Err(String::from("SkinnedModel: no shader program to upload with."));
			}
		};
		let mut materialList = Vec::with_capacity(materialDescriptionList.len());
		for materialDescription in materialDescriptionList.iter() {
			let material = Material::createFromDescription(gl, shaderProgram.clone(), materialDescription)?;
			materialList.push(Rc::new(material));
		}
		let defaultDescription = Material::createDefaultDescription();
		let defaultMaterial = Rc::new(Material::createFromDescription(gl, shaderProgram.clone(), &defaultDescription)?);

		self.drawableList = Vec::with_capacity(meshDescriptionList.len());
		for mut meshDescription in meshDescriptionList.into_iter() {
			let generatedNormals;
			let normals = match &meshDescription.normals {
				Some(normals) => {
					normals
				}
				None => {
					generatedNormals = SkinnedModel::generateSmoothNormals(&meshDescription.positions, meshDescription.indices.as_ref());
					&generatedNormals
				}
			};
			let vertexArray = unsafe {
				gl.create_vertex_array()?
			};
			let mut vertexBufferList = Vec::new();
			unsafe {
				gl.bind_vertex_array(Some(vertexArray));
			}
			if let Some(vertexBuffer) = SkinnedModel::bindFloatAttribute(gl, &shaderProgram, "vertexPosition", &meshDescription.positions, 3)? {
				vertexBufferList.push(vertexBuffer);
			}
			if let Some(vertexBuffer) = SkinnedModel::bindFloatAttribute(gl, &shaderProgram, "vertexNormal", normals, 3)? {
				vertexBufferList.push(vertexBuffer);
			}
			if let Some(textureCoordinates) = &meshDescription.textureCoordinates {
				if let Some(vertexBuffer) = SkinnedModel::bindFloatAttribute(gl, &shaderProgram, "vertexTextureCoordinate", textureCoordinates, 2)? {
					vertexBufferList.push(vertexBuffer);
				}
			}
			if let Some(vertexBuffer) = SkinnedModel::bindIntegerAttribute(gl, &shaderProgram, "vertexJoints", &meshDescription.joints, 4)? {
				vertexBufferList.push(vertexBuffer);
			}
			if let Some(vertexBuffer) = SkinnedModel::bindFloatAttribute(gl, &shaderProgram, "vertexWeights", &meshDescription.weights, 4)? {
				vertexBufferList.push(vertexBuffer);
			}

			// 인덱스. (없는 메시는 drawArrays 로 출력)
			let mut indexCount = 0;
			let mut indexComponentType = 0;
			let mut indexBuffer = None;
			let isIndexed = meshDescription.indices.is_some();
			if let Some(indices) = &meshDescription.indices {
				unsafe {
					let createdIndexBuffer = gl.create_buffer()?;
					gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(createdIndexBuffer));
					gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, indices.asBytes(), glow::STATIC_DRAW);
					indexBuffer = Some(createdIndexBuffer);
				}
				indexCount = indices.len();
				indexComponentType = indices.getGlComponentType();
			}
			unsafe {
				gl.bind_vertex_array(None);
			}

			let material = match meshDescription.materialIndex.and_then(|materialIndex| materialList.get(materialIndex)) {
				Some(material) => {
					material.clone()
				}
				None => {
					defaultMaterial.clone()
				}
			};
			let morphTargetList = std::mem::take(&mut meshDescription.morphTargets);
			let morphWeights = meshDescription.morphWeights.clone();
			let isMorphDirty = !morphTargetList.is_empty();
			self.drawableList.push(Drawable {
				vertexArray,
				vertexBufferList,
				isIndexed,
				indexBuffer,
				indexCount,
				indexComponentType,
				indexByteOffset: 0,
				wireframeIndexBuffer: None,
				wireframeIndexCount: 0,
				wireframeIndexComponentType: 0,
				vertexCount: meshDescription.positions.len() / 3,
				material,
				materialName: meshDescription.materialName.clone(),
				skinIndex: meshDescription.skinIndex,
				nodeIndex: meshDescription.nodeIndex,
				meshDescription,
				morphTargetList,
				morphWeights,
				morphTexture: None,
				morphRowsPerTarget: 0,
				morphTextureWidth: 0,
				isMorphDirty,
			});
		}
		Ok(())
	}

	//==============================================================================
	// 와이어프레임 인덱스 업로드. (삼각형 인덱스 → 중복 없는 모서리 선 인덱스, 비인덱스 메시는 삼각형별 3변)
	// - 드로어블의 버텍스 어레이와 무관한 별도 엘리먼트 버퍼. 렌더러가 선 출력 시 잠시 바꿔 끼운다.
	//==============================================================================
	pub fn uploadWireframeIndices(&mut self, gl: &glow::Context, drawableIndex: usize) -> Result<(), String> {
		let drawable = self.drawableList.get_mut(drawableIndex).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawableIndex))?;
		let edgeList = SkinnedModel::buildWireframeEdgeList(drawable.meshDescription.indices.as_ref(), drawable.vertexCount);
		let useUnsignedInt = drawable.vertexCount > 65535;
		let wireframeIndices = if useUnsignedInt {
			IndexArray::unsignedInt(edgeList.iter().map(|index| *index as u32).collect())
		}
		else {
			IndexArray::unsignedShort(edgeList.iter().map(|index| *index as u16).collect())
		};
		unsafe {
			let wireframeIndexBuffer = gl.create_buffer()?;
			gl.bind_vertex_array(None);
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(wireframeIndexBuffer));
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, wireframeIndices.asBytes(), glow::STATIC_DRAW);
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
			drawable.wireframeIndexBuffer = Some(wireframeIndexBuffer);
		}
		drawable.wireframeIndexCount = wireframeIndices.len();
		drawable.wireframeIndexComponentType = wireframeIndices.getGlComponentType();
		Ok(())
	}

	//==============================================================================
	// 와이어프레임 모서리 목록 만들기. (GL 무관, 인덱스 쌍 목록)
	//==============================================================================
	pub fn buildWireframeEdgeList(triangleIndices: Option<&IndexArray>, vertexCount: usize) -> Vec<usize> {
		let mut edgeList = Vec::new();
		match triangleIndices {
			Some(triangleIndices) => {
				let mut edgeKeySet: HashSet<usize> = HashSet::new();
				let triangleCount = triangleIndices.len() / 3;
				for triangleIndex in 0..triangleCount {
					for cornerIndex in 0..3 {
						let indexA = triangleIndices.get(triangleIndex * 3 + cornerIndex);
						let indexB = triangleIndices.get(triangleIndex * 3 + (cornerIndex + 1) % 3);
						let minimumIndex = indexA.min(indexB);
						let maximumIndex = indexA.max(indexB);
						let edgeKey = minimumIndex * vertexCount + maximumIndex;
						if edgeKeySet.contains(&edgeKey) {
							continue;
						}
						edgeKeySet.insert(edgeKey);
						edgeList.push(minimumIndex);
						edgeList.push(maximumIndex);
					}
				}
			}
			None => {
				let mut vertexIndex = 0;
				while vertexIndex + 2 < vertexCount {
					edgeList.extend_from_slice(&[vertexIndex, vertexIndex + 1, vertexIndex + 1, vertexIndex + 2, vertexIndex + 2, vertexIndex]);
					vertexIndex += 3;
				}
			}
		}
		edgeList
	}

	//==============================================================================
	// 모프 타깃 업로드. (드로어블의 델타 목록 → RGBA32F 텍스처, 타깃마다 위치 블록 + 노멀 블록)
	// - 텍셀 인덱스 = 정점 인덱스. 블록 시작 행 = 타깃 인덱스 x 2 x 타깃당 행 수.
	//==============================================================================
	pub fn uploadMorphTargets(&mut self, gl: &glow::Context, drawableIndex: usize) -> Result<(), String> {
		let drawable = self.drawableList.get_mut(drawableIndex).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawableIndex))?;
		let targetCount = drawable.morphTargetList.len();
		let vertexCount = drawable.vertexCount;
		if targetCount == 0 || vertexCount == 0 {
			if let Some(morphTexture) = drawable.morphTexture.take() {
				unsafe {
					gl.delete_texture(morphTexture);
				}
			}
			drawable.isMorphDirty = false;
			return Ok(());
		}
		let (textureWidth, rowsPerTarget, texelData) = SkinnedModel::buildMorphTexelData(&drawable.morphTargetList, vertexCount);
		let textureHeight = rowsPerTarget * 2 * targetCount;
		unsafe {
			let morphTexture = match drawable.morphTexture {
				Some(morphTexture) => {
					morphTexture
				}
				None => {
					gl.create_texture()?
				}
			};
			drawable.morphTexture = Some(morphTexture);
			gl.bind_texture(glow::TEXTURE_2D, Some(morphTexture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA32F as i32, textureWidth as i32, textureHeight as i32, 0, glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(Some(sliceToBytes(&texelData))));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.bind_texture(glow::TEXTURE_2D, None);
		}
		drawable.morphRowsPerTarget = rowsPerTarget as i32;
		drawable.morphTextureWidth = textureWidth as i32;
		drawable.isMorphDirty = false;
		Ok(())
	}

	//==============================================================================
	// 모프 텍셀 데이터 만들기. (GL 무관, 텍스처 가로, 타깃당 행 수, RGBA 실수 배열)
	//==============================================================================
	pub fn buildMorphTexelData(morphTargetList: &[MorphTarget], vertexCount: usize) -> (usize, usize, Vec<f32>) {
		let targetCount = morphTargetList.len();
		let textureWidth = vertexCount.clamp(1, MORPH_TEXTURE_WIDTH);
		let rowsPerTarget = vertexCount.div_ceil(textureWidth);
		let textureHeight = rowsPerTarget * 2 * targetCount;
		let mut texelData = vec![0.0f32; textureWidth * textureHeight * 4];
		for (targetIndex, morphTarget) in morphTargetList.iter().enumerate() {
			let positionBlockOffset = targetIndex * 2 * rowsPerTarget * textureWidth;
			let normalBlockOffset = positionBlockOffset + rowsPerTarget * textureWidth;
			for vertexIndex in 0..vertexCount {
				let positionTexelOffset = (positionBlockOffset + vertexIndex) * 4;
				for componentIndex in 0..3 {
					texelData[positionTexelOffset + componentIndex] = morphTarget.positionDeltas.get(vertexIndex * 3 + componentIndex).copied().unwrap_or(0.0);
				}
				if let Some(normalDeltas) = &morphTarget.normalDeltas {
					let normalTexelOffset = (normalBlockOffset + vertexIndex) * 4;
					for componentIndex in 0..3 {
						texelData[normalTexelOffset + componentIndex] = normalDeltas.get(vertexIndex * 3 + componentIndex).copied().unwrap_or(0.0);
					}
				}
			}
		}
		(textureWidth, rowsPerTarget, texelData)
	}

	//==============================================================================
	// 바뀐 모프 타깃 업로드. (원본 update 6단계, 추가/변경된 드로어블만, 렌더러가 그리기 전에 부른다)
	//==============================================================================
	pub fn uploadPendingMorphTargets(&mut self, gl: &glow::Context) {
		for drawableIndex in 0..self.drawableList.len() {
			if self.drawableList[drawableIndex].isMorphDirty {
				if let Err(error) = self.uploadMorphTargets(gl, drawableIndex) {
					eprintln!("SkinnedModel: {}", error);
					self.drawableList[drawableIndex].isMorphDirty = false;
				}
			}
		}
	}

	//==============================================================================
	// 모프 타깃 추가. (프로그램 생성, 정점별 위치 델타 [+ 노멀 델타], 다음 그리기 전에 업로드)
	//==============================================================================
	pub fn addMorphTarget(&mut self, drawableIndex: usize, targetName: &str, positionDeltas: Vec<f32>, normalDeltas: Option<Vec<f32>>) -> Result<(), String> {
		let drawable = self.drawableList.get_mut(drawableIndex).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawableIndex))?;
		if drawable.morphTargetList.len() >= MORPH_TARGET_MAXIMUM {
			return Err(format!("SkinnedModel: morph target limit {} exceeded.", MORPH_TARGET_MAXIMUM));
		}
		if positionDeltas.len() != drawable.vertexCount * 3 {
			return Err(format!("SkinnedModel: morph target \"{}\" delta count mismatch.", targetName));
		}
		drawable.morphTargetList.push(MorphTarget { name: targetName.to_string(), positionDeltas, normalDeltas });
		drawable.morphWeights.resize(drawable.morphTargetList.len(), 0.0);
		drawable.isMorphDirty = true;
		Ok(())
	}

	//==============================================================================
	// 모프 가중치 설정. (이름 일치 타깃 전부, 드로어블 여러 개가 같은 이름을 가질 수 있음)
	//==============================================================================
	pub fn setMorphWeight(&mut self, targetName: &str, weight: f32) {
		for drawable in self.drawableList.iter_mut() {
			for targetIndex in 0..drawable.morphTargetList.len() {
				if drawable.morphTargetList[targetIndex].name == targetName {
					drawable.morphWeights[targetIndex] = weight;
				}
			}
		}
	}

	//==============================================================================
	// 모프 가중치 반환. (첫 일치 타깃, 없으면 0)
	//==============================================================================
	pub fn getMorphWeight(&self, targetName: &str) -> f32 {
		for drawable in self.drawableList.iter() {
			for targetIndex in 0..drawable.morphTargetList.len() {
				if drawable.morphTargetList[targetIndex].name == targetName {
					return drawable.morphWeights[targetIndex];
				}
			}
		}
		0.0
	}

	//==============================================================================
	// 머티리얼 교체. (드로어블 단위, 외부 텍스처로 만든 머티리얼 인스턴스 적용)
	//==============================================================================
	pub fn setMaterial(&mut self, drawableIndex: usize, material: Rc<Material>) -> Result<(), String> {
		let drawable = self.drawableList.get_mut(drawableIndex).ok_or_else(|| format!("SkinnedModel: drawable {} not found.", drawableIndex))?;
		drawable.material = material;
		Ok(())
	}

	//==============================================================================
	// 스무스 노멀 생성. (노멀 미보유 메시, 바인드 포즈 기준, 스키닝 회전은 셰이더에서 적용)
	//==============================================================================
	pub fn generateSmoothNormals(positions: &[f32], indices: Option<&IndexArray>) -> Vec<f32> {
		let mut normals = vec![0.0f32; positions.len()];
		let triangleCount = match indices {
			Some(indices) => {
				indices.len() / 3
			}
			None => {
				positions.len() / 9
			}
		};
		for triangleIndex in 0..triangleCount {
			let readCorner = |cornerIndex: usize| -> usize {
				match indices {
					Some(indices) => {
						indices.get(triangleIndex * 3 + cornerIndex) * 3
					}
					None => {
						(triangleIndex * 3 + cornerIndex) * 3
					}
				}
			};
			let indexA = readCorner(0);
			let indexB = readCorner(1);
			let indexC = readCorner(2);
			if indexA + 2 >= positions.len() || indexB + 2 >= positions.len() || indexC + 2 >= positions.len() {
				continue;
			}
			let edgeABX = positions[indexB] - positions[indexA];
			let edgeABY = positions[indexB + 1] - positions[indexA + 1];
			let edgeABZ = positions[indexB + 2] - positions[indexA + 2];
			let edgeACX = positions[indexC] - positions[indexA];
			let edgeACY = positions[indexC + 1] - positions[indexA + 1];
			let edgeACZ = positions[indexC + 2] - positions[indexA + 2];
			let faceNormalX = edgeABY * edgeACZ - edgeABZ * edgeACY;
			let faceNormalY = edgeABZ * edgeACX - edgeABX * edgeACZ;
			let faceNormalZ = edgeABX * edgeACY - edgeABY * edgeACX;
			for cornerOffset in [indexA, indexB, indexC] {
				normals[cornerOffset] += faceNormalX;
				normals[cornerOffset + 1] += faceNormalY;
				normals[cornerOffset + 2] += faceNormalZ;
			}
		}
		for normal in normals.as_chunks_mut::<3>().0 {
			let normalLength = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
			if normalLength > 0.0 {
				normal[0] /= normalLength;
				normal[1] /= normalLength;
				normal[2] /= normalLength;
			}
		}
		normals
	}

	//==============================================================================
	// 실수형 어트리뷰트 바인드. (신규 버퍼 생성 + 포인터 설정, 셰이더가 안 쓰는 어트리뷰트면 None)
	//==============================================================================
	pub fn bindFloatAttribute(gl: &glow::Context, shaderProgram: &ShaderProgram, attributeName: &str, dataArray: &[f32], componentCount: i32) -> Result<Option<GlBuffer>, String> {
		let attributeLocation = match shaderProgram.getAttributeLocation(gl, attributeName) {
			Some(attributeLocation) => {
				attributeLocation
			}
			None => {
				return Ok(None);
			}
		};
		unsafe {
			let vertexBuffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertexBuffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, sliceToBytes(dataArray), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(attributeLocation);
			gl.vertex_attrib_pointer_f32(attributeLocation, componentCount, glow::FLOAT, false, 0, 0);
			Ok(Some(vertexBuffer))
		}
	}

	//==============================================================================
	// 정수형 어트리뷰트 바인드. (조인트 인덱스, 셰이더가 안 쓰는 어트리뷰트면 None)
	//==============================================================================
	pub fn bindIntegerAttribute(gl: &glow::Context, shaderProgram: &ShaderProgram, attributeName: &str, dataArray: &[u16], componentCount: i32) -> Result<Option<GlBuffer>, String> {
		let attributeLocation = match shaderProgram.getAttributeLocation(gl, attributeName) {
			Some(attributeLocation) => {
				attributeLocation
			}
			None => {
				return Ok(None);
			}
		};
		unsafe {
			let vertexBuffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertexBuffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, sliceToBytes(dataArray), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(attributeLocation);
			gl.vertex_attrib_pointer_i32(attributeLocation, componentCount, glow::UNSIGNED_SHORT, 0, 0);
			Ok(Some(vertexBuffer))
		}
	}

	//==============================================================================
	// 애니메이션 재생. (크로스페이드, 같은 이름이 여럿이면 원본처럼 마지막 것, 원본 fadeDuration 기본값 0.25)
	//==============================================================================
	pub fn playAnimation(&mut self, animationName: &str, fadeDuration: f32) {
		if let Some(currentAnimationIndex) = self.currentAnimationIndex {
			if self.animationList[currentAnimationIndex].name == animationName {
				return;
			}
		}
		let foundAnimationIndex = self.animationList.iter().rposition(|animation| animation.name == animationName);
		let foundAnimationIndex = match foundAnimationIndex {
			Some(foundAnimationIndex) => {
				foundAnimationIndex
			}
			None => {
				return;
			}
		};
		self.previousAnimationIndex = self.currentAnimationIndex;
		self.previousTime = self.currentTime;
		self.currentAnimationIndex = Some(foundAnimationIndex);
		self.currentTime = 0.0;
		self.fadeDuration = fadeDuration;
		self.fadeElapsed = 0.0;
	}

	//==============================================================================
	// 애니메이션 샘플링. (클립 인덱스, 현재 노드 포즈에 가중 블렌딩)
	//==============================================================================
	pub fn sampleAnimation(&mut self, animationIndex: usize, time: f32, blendWeight: f32) {
		let animation = match self.animationList.get_mut(animationIndex) {
			Some(animation) => {
				animation
			}
			None => {
				return;
			}
		};
		sampleAnimationClip(animation, &mut self.nodeList, &mut self.drawableList, time, blendWeight);
	}

	//==============================================================================
	// 갱신. (시간 전진 → 포즈 샘플링 → 월드/조인트 행렬 계산, GL 무관)
	//==============================================================================
	pub fn update(&mut self, deltaTime: f32) {
		// 애니메이션이 없어도 기본 포즈로 월드/조인트 행렬은 계산한다. (정적 모델 지원)
		let currentAnimationIndex = self.currentAnimationIndex;
		if currentAnimationIndex.is_some() {
			self.currentTime += deltaTime * self.timeScale;
			if self.previousAnimationIndex.is_some() {
				self.previousTime += deltaTime * self.timeScale;
				self.fadeElapsed += deltaTime;
				if self.fadeElapsed >= self.fadeDuration {
					self.previousAnimationIndex = None;
				}
			}
		}

		// 1. 기본 포즈로 리셋.
		for node in self.nodeList.iter_mut() {
			node.currentTranslation = node.baseTranslation;
			node.currentScale = node.baseScale;
			node.currentRotation = node.baseRotation;
		}

		// 2. 이전/현재 클립 샘플링. (크로스페이드)
		if let Some(currentAnimationIndex) = currentAnimationIndex {
			match self.previousAnimationIndex {
				Some(previousAnimationIndex) => {
					let previousTime = self.previousTime;
					self.sampleAnimation(previousAnimationIndex, previousTime, 1.0);
					let blendWeight = (self.fadeElapsed / self.fadeDuration.max(0.0001)).min(1.0);
					let currentTime = self.currentTime;
					self.sampleAnimation(currentAnimationIndex, currentTime, blendWeight);
				}
				None => {
					let currentTime = self.currentTime;
					self.sampleAnimation(currentAnimationIndex, currentTime, 1.0);
				}
			}
		}

		// 3. 조인트 회전 오프셋. (절차 포즈 가공)
		for (nodeIndex, offsetRotation) in self.jointRotationOffsets.iter() {
			if let Some(node) = self.nodeList.get_mut(*nodeIndex) {
				node.currentRotation = node.currentRotation.multiply(*offsetRotation);
			}
		}

		// 4. 월드 행렬 계산. (루트부터 명시적 스택 순회)
		let mut traversalStack: Vec<(usize, Option<usize>)> = Vec::new();
		for rootIndex in self.rootNodeIndices.iter() {
			traversalStack.push((*rootIndex, None));
		}
		while let Some((nodeIndex, parentIndex)) = traversalStack.pop() {
			let parentWorldMatrix = parentIndex.and_then(|parentIndex| self.nodeList.get(parentIndex)).map(|parentNode| parentNode.worldMatrix);
			let node = match self.nodeList.get_mut(nodeIndex) {
				Some(node) => {
					node
				}
				None => {
					continue;
				}
			};
			let mut localMatrix = Matrix4::createTranslation(node.currentTranslation[0], node.currentTranslation[1], node.currentTranslation[2]);
			let rotationMatrix = Matrix4::createRotationFromQuaternion(node.currentRotation);
			localMatrix.multiply(&rotationMatrix);
			localMatrix.scale(node.currentScale[0], node.currentScale[1], node.currentScale[2]);
			match parentWorldMatrix {
				Some(parentWorldMatrix) => {
					let mut worldMatrix = parentWorldMatrix;
					worldMatrix.multiply(&localMatrix);
					node.worldMatrix = worldMatrix;
				}
				None => {
					node.worldMatrix = localMatrix;
				}
			}
			for childIndex in node.childIndices.iter() {
				traversalStack.push((*childIndex, Some(nodeIndex)));
			}
		}

		// 5. 조인트 행렬. (월드 x 역바인드, 본이 없는 조인트는 단위 행렬)
		let nodeList = &self.nodeList;
		for skin in self.skinList.iter_mut() {
			for jointIndex in 0..skin.jointNodeIndices.len() {
				let jointMatrix = match nodeList.get(skin.jointNodeIndices[jointIndex]) {
					Some(jointNode) => {
						let mut jointMatrix = jointNode.worldMatrix;
						jointMatrix.multiply(&skin.inverseBindMatrices[jointIndex]);
						jointMatrix
					}
					None => {
						Matrix4::createIdentity()
					}
				};
				jointMatrix.writeToSlice(&mut skin.jointMatrixArray, jointIndex * 16);
			}
		}
	}

	//==============================================================================
	// 조인트 회전 오프셋 설정. (이름 부분 일치 노드에 로컬 회전 누적)
	//==============================================================================
	pub fn setJointRotationOffset(&mut self, jointNamePart: &str, offsetRotation: Quaternion) {
		for nodeIndex in 0..self.nodeList.len() {
			if !self.nodeList[nodeIndex].name.contains(jointNamePart) {
				continue;
			}
			match self.jointRotationOffsets.iter_mut().find(|entry| entry.0 == nodeIndex) {
				Some(entry) => {
					entry.1 = offsetRotation;
				}
				None => {
					self.jointRotationOffsets.push((nodeIndex, offsetRotation));
				}
			}
		}
	}

	//==============================================================================
	// 조인트 회전 오프셋 전체 해제.
	//==============================================================================
	pub fn clearJointRotationOffsets(&mut self) {
		self.jointRotationOffsets.clear();
	}

	//==============================================================================
	// 애니메이션 이름 목록 반환.
	//==============================================================================
	pub fn getAnimationNameList(&self) -> Vec<String> {
		self.animationList.iter().map(|animation| animation.name.clone()).collect()
	}

	//==============================================================================
	// 재생 속도 설정.
	//==============================================================================
	pub fn setTimeScale(&mut self, timeScale: f32) {
		self.timeScale = timeScale;
	}

	//==============================================================================
	// 셰이더 프로그램 반환. (셰이더 없이 만든 모델이면 None)
	//==============================================================================
	pub fn getShaderProgram(&self) -> Option<&Rc<ShaderProgram>> {
		self.shaderProgram.as_ref()
	}

	//==============================================================================
	// 노드 목록 반환.
	//==============================================================================
	pub fn getNodeList(&self) -> &[SkinnedModelNode] {
		&self.nodeList
	}

	//==============================================================================
	// 노드 목록 반환. (변경용, 원본은 살아 있는 배열을 내준다)
	//==============================================================================
	pub fn getNodeListMut(&mut self) -> &mut [SkinnedModelNode] {
		&mut self.nodeList
	}

	//==============================================================================
	// 루트 노드 인덱스 목록 반환.
	//==============================================================================
	pub fn getRootNodeIndices(&self) -> &[usize] {
		&self.rootNodeIndices
	}

	//==============================================================================
	// 스킨 목록 반환.
	//==============================================================================
	pub fn getSkinList(&self) -> &[Skin] {
		&self.skinList
	}

	//==============================================================================
	// 드로어블 목록 반환.
	//==============================================================================
	pub fn getDrawableList(&self) -> &[Drawable] {
		&self.drawableList
	}

	//==============================================================================
	// 드로어블 목록 반환. (변경용)
	//==============================================================================
	pub fn getDrawableListMut(&mut self) -> &mut [Drawable] {
		&mut self.drawableList
	}

	//==============================================================================
	// 애니메이션 클립 목록 반환. (원본에 없는 것, 길이, 채널 보기)
	//==============================================================================
	pub fn getAnimationList(&self) -> &[AnimationClip] {
		&self.animationList
	}

	//==============================================================================
	// 파괴. (GL 리소스 해제, 원본에 없는 것, 다른 곳에서도 쓰는 머티리얼은 남긴다)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		let mut destroyedMaterialList: Vec<*const Material> = Vec::new();
		for drawable in self.drawableList.drain(..) {
			unsafe {
				gl.delete_vertex_array(drawable.vertexArray);
				for vertexBuffer in drawable.vertexBufferList.iter() {
					gl.delete_buffer(*vertexBuffer);
				}
				if let Some(indexBuffer) = drawable.indexBuffer {
					gl.delete_buffer(indexBuffer);
				}
				if let Some(wireframeIndexBuffer) = drawable.wireframeIndexBuffer {
					gl.delete_buffer(wireframeIndexBuffer);
				}
				if let Some(morphTexture) = drawable.morphTexture {
					gl.delete_texture(morphTexture);
				}
			}
			let materialPointer = Rc::as_ptr(&drawable.material);
			if destroyedMaterialList.contains(&materialPointer) {
				continue;
			}
			destroyedMaterialList.push(materialPointer);
			if Rc::strong_count(&drawable.material) == 1 {
				drawable.material.destroy(gl);
			}
		}
	}
}

//==============================================================================
// 애니메이션 클립 샘플링. (현재 노드 포즈에 가중 블렌딩, 원본 sampleAnimation 본문)
//==============================================================================
fn sampleAnimationClip(animation: &mut AnimationClip, nodeList: &mut [SkinnedModelNode], drawableList: &mut [Drawable], time: f32, blendWeight: f32) {
	let loopedTime = if animation.duration > 0.0 {
		time % animation.duration
	}
	else {
		0.0
	};
	for channel in animation.channels.iter_mut() {
		let node = match nodeList.get_mut(channel.nodeIndex) {
			Some(node) => {
				node
			}
			None => {
				continue;
			}
		};
		let times = &channel.times;
		let keyCount = times.len();
		if keyCount == 0 {
			continue;
		}

		// 키프레임 커서 탐색. (역방향 점프 시 리셋 후 전진)
		if channel.cursor + 1 >= keyCount || times[channel.cursor] > loopedTime {
			channel.cursor = 0;
		}
		while channel.cursor + 2 < keyCount && times[channel.cursor + 1] < loopedTime {
			channel.cursor += 1;
		}
		let frameIndex = channel.cursor;
		let nextIndex = (frameIndex + 1).min(keyCount - 1);
		let spanDuration = times[nextIndex] - times[frameIndex];
		let factor = if spanDuration > 0.0 {
			((loopedTime - times[frameIndex]) / spanDuration).clamp(0.0, 1.0)
		}
		else {
			0.0
		};
		let values = &channel.values;

		match channel.path {
			AnimationPath::rotation => {
				let baseOffset = frameIndex * 4;
				let nextOffset = nextIndex * 4;
				if nextOffset + 4 > values.len() || baseOffset + 4 > values.len() {
					continue;
				}
				let fromRotation = Quaternion::create(values[baseOffset], values[baseOffset + 1], values[baseOffset + 2], values[baseOffset + 3]);
				let toRotation = Quaternion::create(values[nextOffset], values[nextOffset + 1], values[nextOffset + 2], values[nextOffset + 3]);
				let sampledRotation = Quaternion::slerp(fromRotation, toRotation, factor);
				if blendWeight >= 1.0 {
					node.currentRotation = sampledRotation;
				}
				else {
					node.currentRotation = Quaternion::slerp(node.currentRotation, sampledRotation, blendWeight);
				}
			}
			AnimationPath::translation | AnimationPath::scale => {
				let baseOffset = frameIndex * 3;
				let nextOffset = nextIndex * 3;
				if nextOffset + 3 > values.len() || baseOffset + 3 > values.len() {
					continue;
				}
				let targetArray = if channel.path == AnimationPath::translation {
					&mut node.currentTranslation
				}
				else {
					&mut node.currentScale
				};
				for componentIndex in 0..3 {
					let fromValue = values[baseOffset + componentIndex];
					let toValue = values[nextOffset + componentIndex];
					let sampledValue = fromValue + (toValue - fromValue) * factor;
					targetArray[componentIndex] += (sampledValue - targetArray[componentIndex]) * blendWeight;
				}
			}
			AnimationPath::weights => {
				// 모프 가중치. (키마다 타깃 개수만큼의 값, 해당 노드의 드로어블 전부에 적용)
				for drawable in drawableList.iter_mut() {
					if drawable.nodeIndex != Some(channel.nodeIndex) {
						continue;
					}
					let targetCount = drawable.morphWeights.len();
					let baseOffset = frameIndex * targetCount;
					let nextOffset = nextIndex * targetCount;
					if nextOffset + targetCount > values.len() || baseOffset + targetCount > values.len() {
						continue;
					}
					for targetIndex in 0..targetCount {
						let fromValue = values[baseOffset + targetIndex];
						let toValue = values[nextOffset + targetIndex];
						let sampledValue = fromValue + (toValue - fromValue) * factor;
						let currentWeight = drawable.morphWeights[targetIndex];
						drawable.morphWeights[targetIndex] = currentWeight + (sampledValue - currentWeight) * blendWeight;
					}
				}
			}
		}
	}
}

//==============================================================================
// FBX 오일러(도) → 쿼터니언. (eEulerXYZ = X 먼저 적용, 열벡터 기준 Rz x Ry x Rx 합성)
//==============================================================================
pub fn createQuaternionFromFbxEuler(xDegree: f64, yDegree: f64, zDegree: f64) -> Quaternion {
	let degreeToRadian = std::f64::consts::PI / 180.0;
	let rotationX = Quaternion::createFromEuler((xDegree * degreeToRadian) as f32, 0.0, 0.0);
	let rotationY = Quaternion::createFromEuler(0.0, (yDegree * degreeToRadian) as f32, 0.0);
	let rotationZ = Quaternion::createFromEuler(0.0, 0.0, (zDegree * degreeToRadian) as f32);
	let rotationZY = rotationZ.multiply(rotationY);
	rotationZY.multiply(rotationX)
}

//==============================================================================
// FBX 애니메이션 채널 변환. (회전 = 오일러 도 → 사전회전 합성 쿼터니언 4성분)
// - resolveNodeIndex 로 대상 노드 인덱스를 재지정한다. (본 이름 매칭 리타게팅용, None 이면 버린다)
//==============================================================================
fn convertFbxAnimationChannels(sceneData: &FbxSceneData, fbxAnimation: &FbxAnimation, resolveNodeIndex: impl Fn(usize) -> Option<usize>) -> Vec<AnimationChannel> {
	let mut channels = Vec::with_capacity(fbxAnimation.channels.len());
	for fbxChannel in fbxAnimation.channels.iter() {
		let targetNodeIndex = match resolveNodeIndex(fbxChannel.nodeIndex) {
			Some(targetNodeIndex) => {
				targetNodeIndex
			}
			None => {
				continue;
			}
		};
		if fbxChannel.path == AnimationPath::rotation {
			let preRotationDegrees = sceneData.nodeList.get(fbxChannel.nodeIndex).map(|sourceFbxNode| sourceFbxNode.preRotationDegrees).unwrap_or([0.0, 0.0, 0.0]);
			let preRotationQuaternion = createQuaternionFromFbxEuler(preRotationDegrees[0], preRotationDegrees[1], preRotationDegrees[2]);
			let keyCount = fbxChannel.times.len();
			let mut quaternionValues = vec![0.0f32; keyCount * 4];
			for keyIndex in 0..keyCount {
				let readValue = |componentIndex: usize| fbxChannel.values.get(keyIndex * 3 + componentIndex).copied().unwrap_or(0.0) as f64;
				let eulerQuaternion = createQuaternionFromFbxEuler(readValue(0), readValue(1), readValue(2));
				let rotationQuaternion = preRotationQuaternion.multiply(eulerQuaternion);
				quaternionValues[keyIndex * 4] = rotationQuaternion.x;
				quaternionValues[keyIndex * 4 + 1] = rotationQuaternion.y;
				quaternionValues[keyIndex * 4 + 2] = rotationQuaternion.z;
				quaternionValues[keyIndex * 4 + 3] = rotationQuaternion.w;
			}
			channels.push(AnimationChannel { nodeIndex: targetNodeIndex, path: fbxChannel.path, times: fbxChannel.times.clone(), values: quaternionValues, cursor: 0 });
		}
		else {
			channels.push(AnimationChannel { nodeIndex: targetNodeIndex, path: fbxChannel.path, times: fbxChannel.times.clone(), values: fbxChannel.values.clone(), cursor: 0 });
		}
	}
	channels
}

//==============================================================================
// 접근자 CPU 읽기. (촘촘한 배치 + 인터리브(byteStride) 배치 + 희소(sparse) 접근자 지원)
//==============================================================================
pub fn readAccessorArray(json: &Value, binaryBuffer: &[u8], accessorIndex: usize) -> Result<AccessorArray, String> {
	let accessor = json["accessors"].get(accessorIndex).ok_or_else(|| format!("SkinnedModel: accessor {} not found.", accessorIndex))?;
	let typeName = accessor["type"].as_str().unwrap_or("");
	let componentCount = typeComponentCount(typeName).ok_or_else(|| format!("SkinnedModel: unknown accessor type {}.", typeName))?;
	let componentType = accessor["componentType"].as_u64().unwrap_or(0) as u32;
	let componentByteSize = componentTypeByteSize(componentType).ok_or_else(|| format!("SkinnedModel: unsupported accessor component type {}.", componentType))?;
	let count = readIndex(&accessor["count"]).unwrap_or(0);
	let tightByteSize = componentCount * componentByteSize;
	let mut elementBytes = vec![0u8; count * tightByteSize];

	if let Some(bufferViewIndex) = readIndex(&accessor["bufferView"]) {
		let bufferView = json["bufferViews"].get(bufferViewIndex).ok_or_else(|| format!("SkinnedModel: bufferView {} not found.", bufferViewIndex))?;
		let elementOffset = readIndex(&bufferView["byteOffset"]).unwrap_or(0) + readIndex(&accessor["byteOffset"]).unwrap_or(0);
		let byteStride = readIndex(&bufferView["byteStride"]).unwrap_or(0);
		if byteStride == 0 || byteStride == tightByteSize {
			let sourceBytes = binaryBuffer.get(elementOffset..elementOffset + count * tightByteSize).ok_or_else(|| String::from("SkinnedModel: accessor out of buffer range."))?;
			elementBytes.copy_from_slice(sourceBytes);
		}
		else {
			for elementIndex in 0..count {
				let sourceStart = elementOffset + elementIndex * byteStride;
				let sourceBytes = binaryBuffer.get(sourceStart..sourceStart + tightByteSize).ok_or_else(|| String::from("SkinnedModel: accessor out of buffer range."))?;
				elementBytes[elementIndex * tightByteSize..(elementIndex + 1) * tightByteSize].copy_from_slice(sourceBytes);
			}
		}
	}

	// 희소 접근자. (인덱스 목록의 요소만 값으로 덮어쓴다, 모프 타깃 델타에 흔히 쓰인다)
	let sparse = &accessor["sparse"];
	if sparse.is_object() {
		let sparseCount = readIndex(&sparse["count"]).unwrap_or(0);
		let sparseIndices = &sparse["indices"];
		let indexBufferViewIndex = readIndex(&sparseIndices["bufferView"]).ok_or_else(|| String::from("SkinnedModel: sparse indices have no bufferView."))?;
		let indexBufferView = &json["bufferViews"][indexBufferViewIndex];
		let indexComponentType = sparseIndices["componentType"].as_u64().unwrap_or(0) as u32;
		let indexByteSize = componentTypeByteSize(indexComponentType).ok_or_else(|| format!("SkinnedModel: unsupported sparse index type {}.", indexComponentType))?;
		let indexOffset = readIndex(&indexBufferView["byteOffset"]).unwrap_or(0) + readIndex(&sparseIndices["byteOffset"]).unwrap_or(0);
		let sparseValues = &sparse["values"];
		let valueBufferViewIndex = readIndex(&sparseValues["bufferView"]).ok_or_else(|| String::from("SkinnedModel: sparse values have no bufferView."))?;
		let valueBufferView = &json["bufferViews"][valueBufferViewIndex];
		let valueOffset = readIndex(&valueBufferView["byteOffset"]).unwrap_or(0) + readIndex(&sparseValues["byteOffset"]).unwrap_or(0);
		for sparseIndex in 0..sparseCount {
			let indexStart = indexOffset + sparseIndex * indexByteSize;
			let indexBytes = binaryBuffer.get(indexStart..indexStart + indexByteSize).ok_or_else(|| String::from("SkinnedModel: sparse indices out of buffer range."))?;
			let targetElement = match indexByteSize {
				1 => {
					indexBytes[0] as usize
				}
				2 => {
					u16::from_le_bytes([indexBytes[0], indexBytes[1]]) as usize
				}
				_ => {
					u32::from_le_bytes([indexBytes[0], indexBytes[1], indexBytes[2], indexBytes[3]]) as usize
				}
			};
			let valueStart = valueOffset + sparseIndex * tightByteSize;
			let valueBytes = binaryBuffer.get(valueStart..valueStart + tightByteSize).ok_or_else(|| String::from("SkinnedModel: sparse values out of buffer range."))?;
			if let Some(targetBytes) = elementBytes.get_mut(targetElement * tightByteSize..(targetElement + 1) * tightByteSize) {
				targetBytes.copy_from_slice(valueBytes);
			}
		}
	}

	// 바이트 → 컴포넌트 타입 배열. (리틀 엔디언)
	let accessorArray = match componentType {
		COMPONENT_TYPE_UNSIGNED_BYTE => {
			AccessorArray::unsignedByte(elementBytes)
		}
		COMPONENT_TYPE_UNSIGNED_SHORT => {
			AccessorArray::unsignedShort(elementBytes.as_chunks::<2>().0.iter().map(|chunk| u16::from_le_bytes(*chunk)).collect())
		}
		COMPONENT_TYPE_UNSIGNED_INT => {
			AccessorArray::unsignedInt(elementBytes.as_chunks::<4>().0.iter().map(|chunk| u32::from_le_bytes(*chunk)).collect())
		}
		_ => {
			AccessorArray::float(elementBytes.as_chunks::<4>().0.iter().map(|chunk| f32::from_le_bytes(*chunk)).collect())
		}
	};
	Ok(accessorArray)
}

//==============================================================================
// 이미지 서술 구성. (텍스처 참조 → 임베디드 이미지 바이트, 버퍼뷰가 없는 이미지는 None)
//==============================================================================
fn createGltfImageDescription(json: &Value, binaryBuffer: &[u8], textureInfo: &Value) -> Option<ImageDescription> {
	let textureIndex = readIndex(&textureInfo["index"])?;
	let texture = json["textures"].get(textureIndex)?;
	let imageIndex = readIndex(&texture["source"])?;
	let image = json["images"].get(imageIndex)?;
	let bufferViewIndex = readIndex(&image["bufferView"])?;
	let bufferView = json["bufferViews"].get(bufferViewIndex)?;
	let byteOffset = readIndex(&bufferView["byteOffset"]).unwrap_or(0);
	let byteLength = readIndex(&bufferView["byteLength"]).unwrap_or(0);
	let imageBytes = binaryBuffer.get(byteOffset..byteOffset + byteLength)?;
	Some(ImageDescription {
		bytes: Rc::new(imageBytes.to_vec()),
		mimeType: image["mimeType"].as_str().map(|mimeType| mimeType.to_string()),
		source: None,
		flipY: false,
		isColorData: false,
		anisotropy: 0.0,
	})
}

//==============================================================================
// 어트리뷰트 타입별 컴포넌트 수. (원본 TYPE_COMPONENT_COUNT_TABLE)
//==============================================================================
fn typeComponentCount(typeName: &str) -> Option<usize> {
	match typeName {
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
// 컴포넌트 타입별 바이트 크기. (원본 COMPONENT_ARRAY_TABLE 의 BYTES_PER_ELEMENT, 표에 없으면 None)
//==============================================================================
fn componentTypeByteSize(componentType: u32) -> Option<usize> {
	match componentType {
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
// JSON 인덱스 읽기. (음이 아닌 정수만)
//==============================================================================
fn readIndex(value: &Value) -> Option<usize> {
	value.as_u64().map(|index| index as usize)
}

//==============================================================================
// JSON 인덱스 배열 읽기. (없으면 빈 목록)
//==============================================================================
fn readIndexArray(value: &Value) -> Vec<usize> {
	match value.as_array() {
		Some(indexValues) => {
			indexValues.iter().filter_map(readIndex).collect()
		}
		None => {
			Vec::new()
		}
	}
}

//==============================================================================
// JSON 실수 배열 읽기. (배열이 아니면 None)
//==============================================================================
fn readFloatArray(value: &Value) -> Option<Vec<f32>> {
	value.as_array().map(|numberValues| numberValues.iter().map(|numberValue| numberValue.as_f64().unwrap_or(0.0) as f32).collect())
}

//==============================================================================
// JSON 3성분 벡터 읽기. (없거나 짧으면 기본값)
//==============================================================================
fn readVector3(value: &Value, defaultValue: [f32; 3]) -> [f32; 3] {
	match readFloatArray(value) {
		Some(values) if values.len() >= 3 => {
			[values[0], values[1], values[2]]
		}
		_ => {
			defaultValue
		}
	}
}

//==============================================================================
// 부호 없는 32비트 읽기. (리틀 엔디언, 밖이면 None)
//==============================================================================
fn readUint32(bytes: &[u8], offset: usize) -> Option<u32> {
	let valueBytes = bytes.get(offset..offset + 4)?;
	Some(u32::from_le_bytes([valueBytes[0], valueBytes[1], valueBytes[2], valueBytes[3]]))
}

//==============================================================================
// 숫자 배열을 바이트 배열로 본다. (GL 버퍼 업로드용, 리틀 엔디언 대상)
//==============================================================================
fn sliceToBytes<T: Copy>(values: &[T]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	// 예제 애셋 폴더. (이 저장소 옆에 받아 둔 vanilla.js 저장소, 읽기만 한다. 없으면 그 시험은 건너뛴다)
	const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

	//==============================================================================
	// 시험용 GLB 바이너리 버퍼. (바이트를 붙이고 bufferView 를 기록한다)
	//==============================================================================
	struct GlbBufferBuilder {
		binaryBytes: Vec<u8>,
		bufferViewList: Vec<Value>,
	}

	impl GlbBufferBuilder {
		//==============================================================================
		// 생성.
		//==============================================================================
		fn new() -> GlbBufferBuilder {
			GlbBufferBuilder {
				binaryBytes: Vec::new(),
				bufferViewList: Vec::new(),
			}
		}

		//==============================================================================
		// 바이트 붙이기. (4 바이트 맞춤, byteStride 가 0 이 아니면 기록, bufferView 번호를 돌려준다)
		//==============================================================================
		fn addBufferView(&mut self, bytes: &[u8], byteStride: usize) -> usize {
			while !self.binaryBytes.len().is_multiple_of(4) {
				self.binaryBytes.push(0);
			}
			let mut bufferView = json!({ "buffer": 0, "byteOffset": self.binaryBytes.len(), "byteLength": bytes.len() });
			if byteStride > 0 {
				bufferView["byteStride"] = json!(byteStride);
			}
			self.binaryBytes.extend_from_slice(bytes);
			self.bufferViewList.push(bufferView);
			self.bufferViewList.len() - 1
		}
	}

	//==============================================================================
	// 실수 목록을 리틀 엔디언 바이트로.
	//==============================================================================
	fn floatBytes(values: &[f32]) -> Vec<u8> {
		values.iter().flat_map(|value| value.to_le_bytes()).collect()
	}

	//==============================================================================
	// JSON + BIN 을 GLB 파일 바이트로 감싼다.
	//==============================================================================
	fn wrapGlb(json: &Value, binaryBytes: &[u8]) -> Vec<u8> {
		let mut jsonBytes = serde_json::to_vec(json).expect("json");
		while !jsonBytes.len().is_multiple_of(4) {
			jsonBytes.push(b' ');
		}
		let mut paddedBinaryBytes = binaryBytes.to_vec();
		while !paddedBinaryBytes.len().is_multiple_of(4) {
			paddedBinaryBytes.push(0);
		}
		let totalLength = 12 + 8 + jsonBytes.len() + 8 + paddedBinaryBytes.len();
		let mut glbBytes = Vec::with_capacity(totalLength);
		glbBytes.extend_from_slice(&GLB_MAGIC.to_le_bytes());
		glbBytes.extend_from_slice(&2u32.to_le_bytes());
		glbBytes.extend_from_slice(&(totalLength as u32).to_le_bytes());
		glbBytes.extend_from_slice(&(jsonBytes.len() as u32).to_le_bytes());
		glbBytes.extend_from_slice(&GLB_CHUNKTYPE_JSON.to_le_bytes());
		glbBytes.extend_from_slice(&jsonBytes);
		glbBytes.extend_from_slice(&(paddedBinaryBytes.len() as u32).to_le_bytes());
		glbBytes.extend_from_slice(&GLB_CHUNKTYPE_BINARY.to_le_bytes());
		glbBytes.extend_from_slice(&paddedBinaryBytes);
		glbBytes
	}

	//==============================================================================
	// 시험용 GLB. (뼈 2개 스킨 삼각형 + 같은 메시를 쓰는 정적 노드, 인터리브 위치/UV, 8비트 인덱스와 가중치, 희소 접근자, 클립 2개)
	// - 노드 0 Root(자식 1, 2, 3), 1 Joint(이동 0,1,0), 2 스킨 메시, 3 정적 메시(이동 5,0,0).
	// - 클립 A: Joint 이동 (0,1,0) → (0,3,0) 0~2초, 클립 B: Joint 이동 (4,1,0) 고정 + 회전 Z 90도.
	//==============================================================================
	fn buildTestGlb() -> Vec<u8> {
		let mut bufferBuilder = GlbBufferBuilder::new();

		// 위치 + UV 인터리브. (정점 3개, 정점당 20 바이트)
		let interleavedValues: [f32; 15] = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0];
		let interleavedView = bufferBuilder.addBufferView(&floatBytes(&interleavedValues), 20);
		let indexView = bufferBuilder.addBufferView(&[0u8, 1, 2], 0);
		let jointView = bufferBuilder.addBufferView(&[0u8, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], 0);
		let weightView = bufferBuilder.addBufferView(&[128u8, 127, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0], 0);
		let mut inverseBindValues = Vec::new();
		inverseBindValues.extend_from_slice(Matrix4::createIdentity().getElements());
		inverseBindValues.extend_from_slice(Matrix4::createTranslation(0.0, -1.0, 0.0).getElements());
		let inverseBindView = bufferBuilder.addBufferView(&floatBytes(&inverseBindValues), 0);
		let timeView = bufferBuilder.addBufferView(&floatBytes(&[0.0, 2.0]), 0);
		let translationAView = bufferBuilder.addBufferView(&floatBytes(&[0.0, 1.0, 0.0, 0.0, 3.0, 0.0]), 0);
		let translationBView = bufferBuilder.addBufferView(&floatBytes(&[4.0, 1.0, 0.0, 4.0, 1.0, 0.0]), 0);
		let halfSquareRoot = std::f32::consts::FRAC_1_SQRT_2;
		let rotationBView = bufferBuilder.addBufferView(&floatBytes(&[0.0, 0.0, halfSquareRoot, halfSquareRoot, 0.0, 0.0, halfSquareRoot, halfSquareRoot]), 0);
		let sparseIndexView = bufferBuilder.addBufferView(&[2u8], 0);
		let sparseValueView = bufferBuilder.addBufferView(&floatBytes(&[7.0, 8.0, 9.0]), 0);

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
				{ "bufferView": interleavedView, "byteOffset": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
				{ "bufferView": interleavedView, "byteOffset": 12, "componentType": 5126, "count": 3, "type": "VEC2" },
				{ "bufferView": indexView, "componentType": 5121, "count": 3, "type": "SCALAR" },
				{ "bufferView": jointView, "componentType": 5121, "count": 3, "type": "VEC4" },
				{ "bufferView": weightView, "componentType": 5121, "normalized": true, "count": 3, "type": "VEC4" },
				{ "bufferView": inverseBindView, "componentType": 5126, "count": 2, "type": "MAT4" },
				{ "bufferView": timeView, "componentType": 5126, "count": 2, "type": "SCALAR" },
				{ "bufferView": translationAView, "componentType": 5126, "count": 2, "type": "VEC3" },
				{ "bufferView": translationBView, "componentType": 5126, "count": 2, "type": "VEC3" },
				{ "bufferView": rotationBView, "componentType": 5126, "count": 2, "type": "VEC4" },
				{ "componentType": 5126, "count": 3, "type": "VEC3", "sparse": { "count": 1, "indices": { "bufferView": sparseIndexView, "componentType": 5121 }, "values": { "bufferView": sparseValueView } } },
				{ "componentType": 5126, "count": 3, "type": "VEC3", "sparse": { "count": 1, "indices": { "bufferView": sparseIndexView, "componentType": 5121 }, "values": { "bufferView": sparseValueView } } }
			],
			"animations": [
				{ "name": "A", "samplers": [{ "input": 6, "output": 7 }], "channels": [{ "sampler": 0, "target": { "node": 1, "path": "translation" } }] },
				{ "name": "B", "samplers": [{ "input": 6, "output": 8 }, { "input": 6, "output": 9 }], "channels": [{ "sampler": 0, "target": { "node": 1, "path": "translation" } }, { "sampler": 1, "target": { "node": 1, "path": "rotation" } }] }
			],
			"bufferViews": bufferBuilder.bufferViewList,
			"buffers": [{ "byteLength": bufferBuilder.binaryBytes.len() }]
		});
		wrapGlb(&json, &bufferBuilder.binaryBytes)
	}

	//==============================================================================
	// 시험용 GLB 를 셰이더 없는 모델로 해석한다.
	//==============================================================================
	fn interpretTestGlb() -> (SkinnedModel, Vec<MeshDescription>, Vec<MaterialDescription>) {
		let glbBytes = buildTestGlb();
		let (json, binaryBuffer) = SkinnedModel::readGlbChunks(&glbBytes).expect("glb chunks");
		let mut skinnedModel = SkinnedModel::createWithoutShaderProgram();
		let (meshDescriptionList, materialDescriptionList) = skinnedModel.interpretContent(&json, &binaryBuffer).expect("interpret");
		(skinnedModel, meshDescriptionList, materialDescriptionList)
	}

	//==============================================================================
	// 값 두 개가 허용 오차 안에서 같은지.
	//==============================================================================
	fn assertNear(actual: f32, expected: f32) {
		assert!((actual - expected).abs() < 1.0e-5, "{} expected {}", actual, expected);
	}

	//==============================================================================
	// GLB 청크와 접근자 해석. (인터리브, 8비트 인덱스 → 16비트, 희소 접근자, 잘못된 머리)
	//==============================================================================
	#[test]
	fn glbAccessorDecoding() {
		let glbBytes = buildTestGlb();
		let (json, binaryBuffer) = SkinnedModel::readGlbChunks(&glbBytes).expect("glb chunks");
		let positions = readAccessorArray(&json, &binaryBuffer, 0).expect("positions");
		assert_eq!(positions, AccessorArray::float(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
		let textureCoordinates = readAccessorArray(&json, &binaryBuffer, 1).expect("uv");
		assert_eq!(textureCoordinates, AccessorArray::float(vec![0.0, 1.0, 1.0, 1.0, 0.0, 0.0]));
		let indices = readAccessorArray(&json, &binaryBuffer, 2).expect("indices");
		assert_eq!(indices.getComponentType(), COMPONENT_TYPE_UNSIGNED_BYTE);
		assert_eq!(indices.toIndexArray(), IndexArray::unsignedShort(vec![0, 1, 2]));
		let sparseArray = readAccessorArray(&json, &binaryBuffer, 10).expect("sparse");
		assert_eq!(sparseArray, AccessorArray::float(vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 7.0, 8.0, 9.0]));
		let inverseBind = readAccessorArray(&json, &binaryBuffer, 5).expect("inverse bind").toFloatList();
		assert_eq!(inverseBind.len(), 32);
		assert_eq!(inverseBind[16 + 13], -1.0);

		let mut brokenBytes = glbBytes.clone();
		brokenBytes[0] = 0;
		assert!(SkinnedModel::readGlbChunks(&brokenBytes).is_err());
		assert!(SkinnedModel::readGlbChunks(&glbBytes[..20]).is_err());
	}

	//==============================================================================
	// 내용 해석. (노드, 루트, 실제 스킨 + 정적 메시의 합성 스킨, 가중치 정규화, 모프 타깃, 머티리얼 서술, 클립)
	//==============================================================================
	#[test]
	fn interpretContentBuildsNeutralSchema() {
		let (skinnedModel, meshDescriptionList, materialDescriptionList) = interpretTestGlb();
		let nodeList = skinnedModel.getNodeList();
		assert_eq!(nodeList.len(), 4);
		assert_eq!(nodeList[0].childIndices, vec![1, 2, 3]);
		assert_eq!(skinnedModel.getRootNodeIndices(), &[0]);
		assert_eq!(nodeList[1].baseTranslation, [0.0, 1.0, 0.0]);

		let skinList = skinnedModel.getSkinList();
		assert_eq!(skinList.len(), 2);
		assert_eq!(skinList[0].jointNodeIndices, vec![0, 1]);
		assert_eq!(skinList[1].jointNodeIndices, vec![3]);
		assert_eq!(nodeList[3].skinIndex, Some(1));

		assert_eq!(meshDescriptionList.len(), 2);
		let skinnedMesh = &meshDescriptionList[0];
		assert_eq!(skinnedMesh.skinIndex, 0);
		assert_eq!(skinnedMesh.joints, vec![0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
		assertNear(skinnedMesh.weights[0], 128.0 / 255.0);
		assertNear(skinnedMesh.weights[4], 1.0);
		assert_eq!(skinnedMesh.indices, Some(IndexArray::unsignedShort(vec![0, 1, 2])));
		assert!(skinnedMesh.normals.is_none());
		assert_eq!(skinnedMesh.materialIndex, Some(0));
		assert_eq!(skinnedMesh.materialName, "testMaterial");
		assert_eq!(skinnedMesh.morphTargets.len(), 1);
		assert_eq!(skinnedMesh.morphTargets[0].name, "smile");
		assert_eq!(skinnedMesh.morphWeights, vec![0.25]);
		let staticMesh = &meshDescriptionList[1];
		assert_eq!(staticMesh.skinIndex, 1);
		assert_eq!(staticMesh.joints, vec![0u16; 12]);
		assert_eq!(staticMesh.weights, vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);

		assert_eq!(materialDescriptionList.len(), 1);
		assert_eq!(materialDescriptionList[0].baseColorFactor, [0.25, 0.5, 0.75]);
		assert_eq!(materialDescriptionList[0].metallicFactor, 1.0);
		assert_eq!(materialDescriptionList[0].roughnessFactor, 0.5);

		assert_eq!(skinnedModel.getAnimationNameList(), vec![String::from("A"), String::from("B")]);
		assert_eq!(skinnedModel.getAnimationList()[0].duration, 2.0);
	}

	//==============================================================================
	// 애니메이션 샘플링과 반복, 크로스페이드. (선형 이동, 루프, 페이드 중간 가중 0.5, 페이드 끝)
	//==============================================================================
	#[test]
	fn animationSamplingAndCrossfade() {
		let (mut skinnedModel, _meshDescriptionList, _materialDescriptionList) = interpretTestGlb();

		// 클립 A 0.5초: (0, 1.5, 0). 2.5초는 반복해 0.5초와 같다.
		skinnedModel.update(0.5);
		assertNear(skinnedModel.getNodeList()[1].currentTranslation[1], 1.5);
		skinnedModel.update(2.0);
		assertNear(skinnedModel.getNodeList()[1].currentTranslation[1], 1.5);

		// 클립 B 로 1초 페이드. 0.5초 뒤: A(3초 → 반복 1초 = y 2) 와 B(4, 1, 0) 를 반씩.
		skinnedModel.playAnimation("B", 1.0);
		skinnedModel.update(0.5);
		let translation = skinnedModel.getNodeList()[1].currentTranslation;
		assertNear(translation[0], 2.0);
		assertNear(translation[1], 1.5);
		// 회전은 단위에서 Z 90도 쪽으로 반. (Z 45도)
		let rotation = skinnedModel.getNodeList()[1].currentRotation;
		let expectedRotation = Quaternion::createFromEuler(0.0, 0.0, std::f32::consts::FRAC_PI_4);
		assertNear(rotation.z, expectedRotation.z);
		assertNear(rotation.w, expectedRotation.w);

		// 페이드가 끝나면 B 만.
		skinnedModel.update(0.6);
		let finalTranslation = skinnedModel.getNodeList()[1].currentTranslation;
		assertNear(finalTranslation[0], 4.0);
		assertNear(finalTranslation[1], 1.0);

		// 같은 클립을 다시 틀면 무시, 없는 클립도 무시.
		skinnedModel.playAnimation("B", 0.25);
		skinnedModel.playAnimation("none", 0.25);
		skinnedModel.update(0.0);
		assertNear(skinnedModel.getNodeList()[1].currentTranslation[0], 4.0);

		// 재생 속도 0 이면 시간이 멈춘다. (A 를 페이드 없이 0초에)
		skinnedModel.playAnimation("A", 0.0);
		skinnedModel.setTimeScale(0.0);
		skinnedModel.update(1.0);
		assertNear(skinnedModel.getNodeList()[1].currentTranslation[1], 1.0);
	}

	//==============================================================================
	// 월드 행렬과 조인트 행렬. (바인드 포즈면 조인트 행렬이 단위, 움직이면 월드 x 역바인드, 정적 메시 합성 스킨, 회전 오프셋)
	//==============================================================================
	#[test]
	fn jointMatrixComputation() {
		let (mut skinnedModel, _meshDescriptionList, _materialDescriptionList) = interpretTestGlb();
		skinnedModel.setTimeScale(0.0);
		skinnedModel.update(0.0);
		let skinList = skinnedModel.getSkinList();
		assert_eq!(&skinList[0].jointMatrixArray[16..32], Matrix4::createIdentity().getElements());
		assert_eq!(&skinList[1].jointMatrixArray[0..16], Matrix4::createTranslation(5.0, 0.0, 0.0).getElements());

		// 0.5초 (Joint 이동 y 1.5) → 조인트 행렬 = 이동(0, 1.5, 0) x 이동(0, -1, 0) = 이동(0, 0.5, 0).
		skinnedModel.setTimeScale(1.0);
		skinnedModel.update(0.5);
		let jointMatrixArray = &skinnedModel.getSkinList()[0].jointMatrixArray;
		assertNear(jointMatrixArray[16 + 13], 0.5);
		assertNear(jointMatrixArray[16 + 12], 0.0);

		// 회전 오프셋 (이름 부분 일치): Joint 를 X 90도 돌리면 월드 행렬의 Y 축이 +Z 로 간다.
		skinnedModel.setTimeScale(0.0);
		skinnedModel.setJointRotationOffset("Joi", Quaternion::createFromEuler(std::f32::consts::FRAC_PI_2, 0.0, 0.0));
		skinnedModel.update(0.0);
		let worldElements = skinnedModel.getNodeList()[1].worldMatrix.elements;
		assertNear(worldElements[4], 0.0);
		assertNear(worldElements[5], 0.0);
		assertNear(worldElements[6], 1.0);
		skinnedModel.clearJointRotationOffsets();
		skinnedModel.update(0.0);
		assertNear(skinnedModel.getNodeList()[1].worldMatrix.elements[5], 1.0);
	}

	//==============================================================================
	// 스무스 노멀과 와이어프레임 모서리, 모프 텍셀 배치. (GL 무관 부분)
	//==============================================================================
	#[test]
	fn generatedGeometryHelpers() {
		let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
		let normals = SkinnedModel::generateSmoothNormals(&positions, None);
		assert_eq!(normals, vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);

		let quadIndices = IndexArray::unsignedShort(vec![0, 1, 2, 2, 1, 3]);
		let edgeList = SkinnedModel::buildWireframeEdgeList(Some(&quadIndices), 4);
		assert_eq!(edgeList, vec![0, 1, 1, 2, 0, 2, 1, 3, 2, 3]);
		assert_eq!(SkinnedModel::buildWireframeEdgeList(None, 3), vec![0, 1, 1, 2, 2, 0]);

		let morphTargetList = vec![
			MorphTarget { name: String::from("first"), positionDeltas: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], normalDeltas: Some(vec![0.5, 0.5, 0.5, 0.25, 0.25, 0.25]) },
			MorphTarget { name: String::from("second"), positionDeltas: vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0], normalDeltas: None },
		];
		let (textureWidth, rowsPerTarget, texelData) = SkinnedModel::buildMorphTexelData(&morphTargetList, 2);
		assert_eq!((textureWidth, rowsPerTarget), (2, 1));
		assert_eq!(texelData.len(), 2 * 4 * 4);
		assert_eq!(&texelData[0..8], &[1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0]);
		assert_eq!(&texelData[8..12], &[0.5, 0.5, 0.5, 0.0]);
		assert_eq!(&texelData[16..20], &[7.0, 8.0, 9.0, 0.0]);
		assert_eq!(&texelData[24..32], &[0.0; 8]);
	}

	//==============================================================================
	// FBX 오일러 변환. (X 먼저 적용, Rz x Ry x Rx)
	//==============================================================================
	#[test]
	fn fbxEulerOrderIsXFirst() {
		let rotation = createQuaternionFromFbxEuler(90.0, 0.0, 90.0);
		let rotationMatrix = Matrix4::createRotationFromQuaternion(rotation);
		// +Y 를 X 90도 → +Z, 다시 Z 90도 → +Z 그대로.
		let rotatedY = rotationMatrix.transformDirection(0.0, 1.0, 0.0);
		assertNear(rotatedY.x, 0.0);
		assertNear(rotatedY.y, 0.0);
		assertNear(rotatedY.z, 1.0);
		// +X 를 X 90도 → +X, 다시 Z 90도 → +Y.
		let rotatedX = rotationMatrix.transformDirection(1.0, 0.0, 0.0);
		assertNear(rotatedX.y, 1.0);
	}

	//==============================================================================
	// 실제 GLB. (원본 예제 fox.glb, 조인트 24개, 클립 Survey/Walk/Run, 노멀 없음, 인덱스 없음, PNG 텍스처, 파일이 없으면 건너뛴다)
	//==============================================================================
	#[test]
	fn foxGlbFromExamples() {
		let foxPath = std::path::Path::new(EXAMPLE_ASSET_DIRECTORY).join("fox.glb");
		let glbBytes = match std::fs::read(&foxPath) {
			Ok(glbBytes) => {
				glbBytes
			}
			Err(_) => {
				eprintln!("fox.glb 없음, 건너뜀");
				return;
			}
		};
		let (json, binaryBuffer) = SkinnedModel::readGlbChunks(&glbBytes).expect("fox chunks");
		let mut skinnedModel = SkinnedModel::createWithoutShaderProgram();
		let (meshDescriptionList, materialDescriptionList) = skinnedModel.interpretContent(&json, &binaryBuffer).expect("fox interpret");
		assert_eq!(skinnedModel.getAnimationNameList(), vec![String::from("Survey"), String::from("Walk"), String::from("Run")]);
		assert_eq!(skinnedModel.getSkinList()[0].jointNodeIndices.len(), 24);
		assert_eq!(meshDescriptionList.len(), 1);
		let foxMesh = &meshDescriptionList[0];
		assert!(foxMesh.normals.is_none() && foxMesh.indices.is_none());
		assert_eq!(foxMesh.positions.len() / 3, foxMesh.weights.len() / 4);
		let normals = SkinnedModel::generateSmoothNormals(&foxMesh.positions, None);
		let firstLength = (normals[0] * normals[0] + normals[1] * normals[1] + normals[2] * normals[2]).sqrt();
		assertNear(firstLength, 1.0);
		let baseColorImage = materialDescriptionList[0].baseColorImage.as_ref().expect("fox texture");
		assert_eq!(baseColorImage.mimeType.as_deref(), Some("image/png"));
		let (imageWidth, imageHeight, pixels) = crate::graphics3d::material::decodeImageBytes(&baseColorImage.bytes).expect("fox png");
		assert_eq!(pixels.len(), (imageWidth * imageHeight * 4) as usize);

		// 걷기 클립 중간에서 조인트 행렬이 바인드 포즈와 달라야 한다.
		skinnedModel.playAnimation("Walk", 0.0);
		skinnedModel.update(0.0);
		let restMatrixArray = skinnedModel.getSkinList()[0].jointMatrixArray.clone();
		skinnedModel.update(0.3);
		let walkMatrixArray = &skinnedModel.getSkinList()[0].jointMatrixArray;
		assert!(walkMatrixArray.iter().all(|value| value.is_finite()));
		assert!(restMatrixArray.iter().zip(walkMatrixArray.iter()).any(|(restValue, walkValue)| (restValue - walkValue).abs() > 1.0e-3));
	}
}
