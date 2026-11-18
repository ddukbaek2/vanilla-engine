//==============================================================================
// FBX 로더. (원본 experimental/graphics/fbxloader.js)
// - 바이너리 FBX → GL 무관 중간 표현(FbxSceneData). 저수준 노드/프로퍼티 파싱 후 오브젝트/커넥션 그래프를 해석해
//   노드 계층 / 스키닝 메시 / 머티리얼 / 임베디드 텍스처 / 골격 애니메이션을 뽑는다. 결과는 SkinnedModel 이 GL 리소스로 소비한다.
// - 원본처럼 순수 파서다. GL 이나 셰이딩을 모르고, wasm32 에서도 돈다. 압축 배열(deflate, zlib 머리)은 flate2(순수 러스트)로 푼다.
// - 원본과 다른 것:
//   - 원본은 async 지만 러스트 판은 그 자리에서 끝난다. 원본 loadFromUrl(url) 은 데스크탑 경로를 읽는 loadFromPath 로 옮겼다.
//     웹은 BlobAsset 으로 받은 바이트를 parse 에 넘긴다.
//   - 원본은 64비트 정수(오브젝트 id, 키 시간)를 Number 로 바꿔 2^53 을 넘으면 끝자리를 잃는다. 러스트 판은 i64 그대로 든다.
//   - 원본은 순서 있는 Map 을 쓴다. 러스트 판은 넣은 순서를 지키는 FbxObjectMap 으로 같은 순회 순서를 낸다.
//   - 원본은 빠진 노드나 짧은 레코드에서 TypeError 로 멈춘다. 러스트 판은 Objects / Connections 가 없거나 파일이 잘렸으면 오류를 돌려주고,
//     정점 / 폴리곤 배열이 없는 지오메트리(블렌드셰이프 Shape 같은)와 값이 모자란 Properties70 레코드는 건너뛴다.
//     레이어 인덱스가 배열 밖이면 원본은 NaN, 러스트 판은 0 을 넣는다.
//   - 원본에서 만들고 쓰지 않는 objectById 와 getDataView / getFileBytes / getTextDecoder 는 옮기지 않았다.
//==============================================================================
use crate::graphics3d::skinnedmodel::AnimationPath;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// FBX 시간 단위. (1초 = 46186158000 ktime)
pub const FBX_TIME_UNIT: f64 = 46186158000.0;

// 파일 끝 꼬리 크기. (최상위 노드 파싱을 멈추는 여유, 원본 byteLength - 160)
const FBX_FOOTER_MARGIN: i64 = 160;

//==============================================================================
// 프로퍼티 값. (원본 { type, value }, 타입 코드 Y C I F D L f d l i b S R)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum FbxPropertyValue {
	int16(i16),
	boolean(bool),
	int32(i32),
	float32(f32),
	float64(f64),
	int64(i64),
	float32Array(Vec<f32>),
	float64Array(Vec<f64>),
	int64Array(Vec<i64>),
	int32Array(Vec<i32>),
	booleanArray(Vec<u8>),
	string(String),
	raw(Rc<Vec<u8>>),
}

impl FbxPropertyValue {
	//==============================================================================
	// 타입 코드 반환. (원본 property.type)
	//==============================================================================
	pub fn getTypeCode(&self) -> char {
		match self {
			FbxPropertyValue::int16(_) => {
				'Y'
			}
			FbxPropertyValue::boolean(_) => {
				'C'
			}
			FbxPropertyValue::int32(_) => {
				'I'
			}
			FbxPropertyValue::float32(_) => {
				'F'
			}
			FbxPropertyValue::float64(_) => {
				'D'
			}
			FbxPropertyValue::int64(_) => {
				'L'
			}
			FbxPropertyValue::float32Array(_) => {
				'f'
			}
			FbxPropertyValue::float64Array(_) => {
				'd'
			}
			FbxPropertyValue::int64Array(_) => {
				'l'
			}
			FbxPropertyValue::int32Array(_) => {
				'i'
			}
			FbxPropertyValue::booleanArray(_) => {
				'b'
			}
			FbxPropertyValue::string(_) => {
				'S'
			}
			FbxPropertyValue::raw(_) => {
				'R'
			}
		}
	}

	//==============================================================================
	// 수 하나로 읽기. (스칼라 타입만, 아니면 None)
	//==============================================================================
	pub fn asNumber(&self) -> Option<f64> {
		match self {
			FbxPropertyValue::int16(value) => {
				Some(*value as f64)
			}
			FbxPropertyValue::boolean(value) => {
				Some(f64::from(u8::from(*value)))
			}
			FbxPropertyValue::int32(value) => {
				Some(*value as f64)
			}
			FbxPropertyValue::float32(value) => {
				Some(*value as f64)
			}
			FbxPropertyValue::float64(value) => {
				Some(*value)
			}
			FbxPropertyValue::int64(value) => {
				Some(*value as f64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 정수 하나로 읽기. (오브젝트 id 같은 정수 스칼라, 아니면 None)
	//==============================================================================
	pub fn asInteger(&self) -> Option<i64> {
		match self {
			FbxPropertyValue::int16(value) => {
				Some(*value as i64)
			}
			FbxPropertyValue::int32(value) => {
				Some(*value as i64)
			}
			FbxPropertyValue::int64(value) => {
				Some(*value)
			}
			FbxPropertyValue::float32(value) => {
				Some(*value as i64)
			}
			FbxPropertyValue::float64(value) => {
				Some(*value as i64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 문자열로 읽기. (S 타입만)
	//==============================================================================
	pub fn asString(&self) -> Option<&str> {
		match self {
			FbxPropertyValue::string(value) => {
				Some(value.as_str())
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 배열 길이 반환. (배열 타입이 아니면 0)
	//==============================================================================
	pub fn getArrayLength(&self) -> usize {
		match self {
			FbxPropertyValue::float32Array(values) => {
				values.len()
			}
			FbxPropertyValue::float64Array(values) => {
				values.len()
			}
			FbxPropertyValue::int64Array(values) => {
				values.len()
			}
			FbxPropertyValue::int32Array(values) => {
				values.len()
			}
			FbxPropertyValue::booleanArray(values) => {
				values.len()
			}
			_ => {
				0
			}
		}
	}

	//==============================================================================
	// 배열 원소 하나를 수로 읽기. (배열 밖이거나 배열이 아니면 None)
	//==============================================================================
	pub fn getNumberAt(&self, index: usize) -> Option<f64> {
		match self {
			FbxPropertyValue::float32Array(values) => {
				values.get(index).map(|value| *value as f64)
			}
			FbxPropertyValue::float64Array(values) => {
				values.get(index).copied()
			}
			FbxPropertyValue::int64Array(values) => {
				values.get(index).map(|value| *value as f64)
			}
			FbxPropertyValue::int32Array(values) => {
				values.get(index).map(|value| *value as f64)
			}
			FbxPropertyValue::booleanArray(values) => {
				values.get(index).map(|value| *value as f64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 배열 원소 하나를 정수로 읽기. (배열 밖이거나 배열이 아니면 None)
	//==============================================================================
	pub fn getIntegerAt(&self, index: usize) -> Option<i64> {
		match self {
			FbxPropertyValue::int64Array(values) => {
				values.get(index).copied()
			}
			FbxPropertyValue::int32Array(values) => {
				values.get(index).map(|value| *value as i64)
			}
			FbxPropertyValue::booleanArray(values) => {
				values.get(index).map(|value| *value as i64)
			}
			FbxPropertyValue::float32Array(values) => {
				values.get(index).map(|value| *value as i64)
			}
			FbxPropertyValue::float64Array(values) => {
				values.get(index).map(|value| *value as i64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 수 목록으로 바꾸기. (숫자 배열 타입 무엇이든 f64 로, 아니면 빈 목록)
	//==============================================================================
	pub fn toNumberList(&self) -> Vec<f64> {
		let arrayLength = self.getArrayLength();
		let mut numberList = Vec::with_capacity(arrayLength);
		for elementIndex in 0..arrayLength {
			numberList.push(self.getNumberAt(elementIndex).unwrap_or(0.0));
		}
		numberList
	}

	//==============================================================================
	// 정수 목록으로 바꾸기. (숫자 배열 타입 무엇이든 i64 로, 아니면 빈 목록)
	//==============================================================================
	pub fn toIntegerList(&self) -> Vec<i64> {
		let arrayLength = self.getArrayLength();
		let mut integerList = Vec::with_capacity(arrayLength);
		for elementIndex in 0..arrayLength {
			integerList.push(self.getIntegerAt(elementIndex).unwrap_or(0));
		}
		integerList
	}
}

//==============================================================================
// 노드 레코드. (원본 { name, properties, children })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct FbxNodeRecord {
	pub name: String,
	pub properties: Vec<FbxPropertyValue>,
	pub children: Vec<FbxNodeRecord>,
}

impl FbxNodeRecord {
	//==============================================================================
	// 이름이 같은 첫 자식 반환. (원본 children.find)
	//==============================================================================
	pub fn findChild(&self, childName: &str) -> Option<&FbxNodeRecord> {
		self.children.iter().find(|child| child.name == childName)
	}

	//==============================================================================
	// 프로퍼티 반환. (없으면 None)
	//==============================================================================
	pub fn getProperty(&self, propertyIndex: usize) -> Option<&FbxPropertyValue> {
		self.properties.get(propertyIndex)
	}

	//==============================================================================
	// 첫 프로퍼티를 정수로 반환. (오브젝트 id)
	//==============================================================================
	pub fn getObjectId(&self) -> Option<i64> {
		self.properties.first().and_then(|property| property.asInteger())
	}

	//==============================================================================
	// 프로퍼티를 문자열로 반환. (없거나 문자열이 아니면 빈 문자열)
	//==============================================================================
	pub fn getStringProperty(&self, propertyIndex: usize) -> String {
		self.properties.get(propertyIndex).and_then(|property| property.asString()).unwrap_or("").to_string()
	}
}

//==============================================================================
// 넣은 순서를 지키는 id 맵. (원본 Map<number, object> 자리, 순회 순서가 결과 순서를 정한다)
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxObjectMap<T> {
	idList: Vec<i64>,
	valueList: Vec<T>,
	indexById: HashMap<i64, usize>,
}

impl<T> Default for FbxObjectMap<T> {
	//==============================================================================
	// 빈 맵.
	//==============================================================================
	fn default() -> FbxObjectMap<T> {
		FbxObjectMap::new()
	}
}

impl<T> FbxObjectMap<T> {
	//==============================================================================
	// 생성. (비어 있음)
	//==============================================================================
	pub fn new() -> FbxObjectMap<T> {
		FbxObjectMap {
			idList: Vec::new(),
			valueList: Vec::new(),
			indexById: HashMap::new(),
		}
	}

	//==============================================================================
	// 넣기. (원본 Map.set, 이미 있는 id 면 자리를 지키고 값만 바꾼다)
	//==============================================================================
	pub fn insert(&mut self, id: i64, value: T) {
		match self.indexById.get(&id) {
			Some(existingIndex) => {
				self.valueList[*existingIndex] = value;
			}
			None => {
				self.indexById.insert(id, self.idList.len());
				self.idList.push(id);
				self.valueList.push(value);
			}
		}
	}

	//==============================================================================
	// 값 반환. (원본 Map.get)
	//==============================================================================
	pub fn get(&self, id: i64) -> Option<&T> {
		self.indexById.get(&id).map(|index| &self.valueList[*index])
	}

	//==============================================================================
	// 값 반환. (변경용)
	//==============================================================================
	pub fn getMut(&mut self, id: i64) -> Option<&mut T> {
		match self.indexById.get(&id) {
			Some(index) => {
				Some(&mut self.valueList[*index])
			}
			None => {
				None
			}
		}
	}

	//==============================================================================
	// 있는지. (원본 Map.has)
	//==============================================================================
	pub fn contains(&self, id: i64) -> bool {
		self.indexById.contains_key(&id)
	}

	//==============================================================================
	// 넣은 순서의 자리 반환. (원본 Array.from(map.keys()).indexOf)
	//==============================================================================
	pub fn indexOf(&self, id: i64) -> Option<usize> {
		self.indexById.get(&id).copied()
	}

	//==============================================================================
	// id 목록 반환. (넣은 순서)
	//==============================================================================
	pub fn keys(&self) -> &[i64] {
		&self.idList
	}

	//==============================================================================
	// 값 목록 반환. (넣은 순서)
	//==============================================================================
	pub fn values(&self) -> &[T] {
		&self.valueList
	}

	//==============================================================================
	// 값 목록 반환. (변경용)
	//==============================================================================
	pub fn valuesMut(&mut self) -> &mut [T] {
		&mut self.valueList
	}

	//==============================================================================
	// (id, 값) 순회. (넣은 순서)
	//==============================================================================
	pub fn iter(&self) -> impl Iterator<Item = (i64, &T)> {
		self.idList.iter().copied().zip(self.valueList.iter())
	}

	//==============================================================================
	// 개수 반환.
	//==============================================================================
	pub fn len(&self) -> usize {
		self.idList.len()
	}

	//==============================================================================
	// 비었는지.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.idList.is_empty()
	}
}

//==============================================================================
// 지오메트리. (삼각화 + 노멀/UV 레이어를 코너 단위로 펼친 것 + 원본 정점 매핑)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxGeometry {
	pub positions: Vec<f32>,
	pub normals: Vec<f32>,
	pub textureCoordinates: Vec<f32>,
	pub originalVertexIndices: Vec<i32>,
	pub materialIndexPerTriangle: Vec<i32>,
	pub cornerCount: usize,
	pub originalVertexCount: usize,
}

//==============================================================================
// 스킨 디포머. (클러스터 목록 + 붙은 지오메트리)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxSkinDeformer {
	pub clusterIds: Vec<i64>,
	pub geometryId: Option<i64>,
}

//==============================================================================
// 클러스터. (본별 정점 가중치 + 바인드 역행렬용 링크 변환, 행렬은 열 우선 16개)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxCluster {
	pub vertexIndices: Vec<i64>,
	pub weights: Vec<f64>,
	pub transform: Option<Vec<f64>>,
	pub transformLink: Option<Vec<f64>>,
	pub boneNodeId: Option<i64>,
}

//==============================================================================
// 비디오. (임베디드 이미지 바이트)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxVideo {
	pub contentBytes: Option<Rc<Vec<u8>>>,
	pub filename: String,
}

//==============================================================================
// 텍스처. (비디오 연결)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxTexture {
	pub videoId: Option<i64>,
	pub filename: String,
}

//==============================================================================
// 머티리얼. (phong 색상/광택 + 슬롯별 텍스처 id)
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxMaterial {
	pub diffuseColor: [f64; 3],
	pub specularColor: [f64; 3],
	pub shininess: f64,
	pub diffuseTextureId: Option<i64>,
	pub normalTextureId: Option<i64>,
	pub specularTextureId: Option<i64>,
	pub glossTextureId: Option<i64>,
	pub opacityTextureId: Option<i64>,
}

//==============================================================================
// 모델 노드. (계층 + 로컬 변환(오일러 도) + 지오메트리/스킨/머티리얼 소속)
// - name 은 원본처럼 FBX 이름 그대로다. ("Hips\u{0}\u{1}Model" 꼴, 본 이름 매칭도 이 문자열로 한다)
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxNode {
	pub id: i64,
	pub name: String,
	pub isBone: bool,
	pub translation: [f64; 3],
	pub rotationEulerDegrees: [f64; 3],
	pub preRotationDegrees: [f64; 3],
	pub scale: [f64; 3],
	pub geometryId: Option<i64>,
	pub skinId: Option<i64>,
	pub materialIds: Vec<i64>,
	pub childIndices: Vec<usize>,
	pub parentId: Option<i64>,
}

//==============================================================================
// 애니메이션 채널. (노드 인덱스 + 경로 + 키 시간(초) + 값, 회전은 오일러 도 3성분)
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxAnimationChannel {
	pub nodeIndex: usize,
	pub path: AnimationPath,
	pub times: Vec<f32>,
	pub values: Vec<f32>,
}

//==============================================================================
// 애니메이션. (스택 하나)
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxAnimation {
	pub name: String,
	pub duration: f32,
	pub channels: Vec<FbxAnimationChannel>,
}

//==============================================================================
// 중간 표현. (원본 sceneData)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxSceneData {
	pub nodeList: Vec<FbxNode>,
	pub nodeIdToIndex: HashMap<i64, usize>,
	pub rootNodeIndices: Vec<usize>,
	pub geometryById: FbxObjectMap<FbxGeometry>,
	pub skinDeformerById: FbxObjectMap<FbxSkinDeformer>,
	pub clusterById: FbxObjectMap<FbxCluster>,
	pub materialById: FbxObjectMap<FbxMaterial>,
	pub textureById: FbxObjectMap<FbxTexture>,
	pub videoById: FbxObjectMap<FbxVideo>,
	pub animationList: Vec<FbxAnimation>,
}

//==============================================================================
// 오브젝트-오브젝트 커넥션. (OO)
//==============================================================================
struct ObjectConnection {
	sourceId: i64,
	destinationId: i64,
}

//==============================================================================
// 오브젝트-프로퍼티 커넥션. (OP)
//==============================================================================
struct PropertyConnection {
	sourceId: i64,
	destinationId: i64,
	propertyName: String,
}

//==============================================================================
// 애니메이션 커브. (키 시간 ktime + 값)
//==============================================================================
struct AnimationCurve {
	keyTimes: Vec<i64>,
	keyValues: Vec<f64>,
}

//==============================================================================
// 애니메이션 커브노드. (대상 노드 / 프로퍼티 + 채널 X/Y/Z 별 커브 id)
//==============================================================================
struct AnimationCurveNode {
	targetNodeId: Option<i64>,
	targetProperty: Option<String>,
	curveByChannel: HashMap<String, i64>,
}

//==============================================================================
// 애니메이션 스택. (이름 + 소속 레이어)
//==============================================================================
struct AnimationStack {
	name: String,
	layerIds: Vec<i64>,
}

//==============================================================================
// 레이어 리더. (노멀/UV, 매핑/참조 방식별 조회, 원본 buildLayerReader 가 돌려주는 함수)
//==============================================================================
struct LayerReader<'a> {
	dataArray: Option<&'a FbxPropertyValue>,
	mappingType: String,
	referenceType: String,
	indexArray: Option<&'a FbxPropertyValue>,
	componentCount: usize,
}

impl LayerReader<'_> {
	//==============================================================================
	// 코너 하나의 값 읽기. (레이어가 없으면 0)
	//==============================================================================
	fn read(&self, originalVertexIndex: usize, polygonVertexPosition: usize, componentValues: &mut [f64]) {
		let dataArray = match self.dataArray {
			Some(dataArray) => {
				dataArray
			}
			None => {
				componentValues.fill(0.0);
				return;
			}
		};
		let mut mappedIndex = 0;
		if self.mappingType == "ByVertice" || self.mappingType == "ByVertex" {
			mappedIndex = originalVertexIndex;
		}
		else if self.mappingType == "ByPolygonVertex" {
			mappedIndex = polygonVertexPosition;
		}
		else if self.mappingType == "AllSame" {
			mappedIndex = 0;
		}
		let mut dataIndex = mappedIndex as i64;
		if self.referenceType == "IndexToDirect" {
			if let Some(indexArray) = self.indexArray {
				dataIndex = indexArray.getIntegerAt(mappedIndex).unwrap_or(-1);
			}
		}
		for (componentIndex, componentValue) in componentValues.iter_mut().enumerate().take(self.componentCount) {
			*componentValue = if dataIndex >= 0 {
				dataArray.getNumberAt(dataIndex as usize * self.componentCount + componentIndex).unwrap_or(0.0)
			}
			else {
				0.0
			};
		}
	}
}

//==============================================================================
// FBX 로더.
//==============================================================================
pub struct FbxLoader {
	readCursor: usize,
	useWideOffsets: bool,
}

impl Default for FbxLoader {
	//==============================================================================
	// 생성. (원본 new FbxLoader())
	//==============================================================================
	fn default() -> FbxLoader {
		FbxLoader::new()
	}
}

impl FbxLoader {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> FbxLoader {
		FbxLoader {
			readCursor: 0,
			useWideOffsets: false,
		}
	}

	//==============================================================================
	// 파일로부터 파싱. (원본 정적 loadFromUrl 의 데스크탑 판)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub fn loadFromPath(path: &std::path::Path) -> Result<FbxSceneData, String> {
		let fileBytes = std::fs::read(path).map_err(|error| format!("FbxLoader load failed: {} ({})", path.display(), error))?;
		let mut fbxLoader = FbxLoader::new();
		fbxLoader.parse(&fileBytes)
	}

	//==============================================================================
	// 파싱. (바이너리 → 중간 표현)
	//==============================================================================
	pub fn parse(&mut self, fileBytes: &[u8]) -> Result<FbxSceneData, String> {
		let topNodeList = self.parseNodeRecords(fileBytes)?;
		self.interpret(&topNodeList)
	}

	//==============================================================================
	// 최상위 노드 목록 파싱. (머리 확인 + 버전별 오프셋 크기 + 노드 레코드들, 원본 parse 의 앞부분)
	//==============================================================================
	pub fn parseNodeRecords(&mut self, fileBytes: &[u8]) -> Result<Vec<FbxNodeRecord>, String> {
		let magicLength = fileBytes.len().min(20);
		let magicText = String::from_utf8_lossy(&fileBytes[..magicLength]);
		if !magicText.starts_with("Kaydara FBX Binary") {
			return Err(String::from("FbxLoader: not a binary FBX file."));
		}
		let fbxVersion = readUint32(fileBytes, 23)?;
		self.useWideOffsets = fbxVersion >= 7500;
		self.readCursor = 27;

		let mut topNodeList = Vec::new();
		while (self.readCursor as i64) < fileBytes.len() as i64 - FBX_FOOTER_MARGIN {
			match self.readNodeRecord(fileBytes)? {
				Some(topNode) => {
					topNodeList.push(topNode);
				}
				None => {
					break;
				}
			}
		}
		Ok(topNodeList)
	}

	//==============================================================================
	// FBX 버전 반환. (머리의 버전 번호, 7400 = 7.4)
	//==============================================================================
	pub fn readVersion(fileBytes: &[u8]) -> Result<u32, String> {
		readUint32(fileBytes, 23)
	}

	//==============================================================================
	// 오프셋 값 읽기. (버전에 따라 32/64비트)
	//==============================================================================
	fn readOffsetValue(&mut self, fileBytes: &[u8]) -> Result<usize, String> {
		if self.useWideOffsets {
			let value = readUint64(fileBytes, self.readCursor)?;
			self.readCursor += 8;
			return Ok(value as usize);
		}
		let value = readUint32(fileBytes, self.readCursor)?;
		self.readCursor += 4;
		Ok(value as usize)
	}

	//==============================================================================
	// 노드 레코드 읽기. (재귀, 프로퍼티 + 자식, 끝 표시 레코드면 None)
	//==============================================================================
	fn readNodeRecord(&mut self, fileBytes: &[u8]) -> Result<Option<FbxNodeRecord>, String> {
		let endOffset = self.readOffsetValue(fileBytes)?;
		let propertyCount = self.readOffsetValue(fileBytes)?;
		self.readOffsetValue(fileBytes)?;
		let nameLength = readUint8(fileBytes, self.readCursor)? as usize;
		self.readCursor += 1;
		if endOffset == 0 {
			return Ok(None);
		}
		let nodeName = String::from_utf8_lossy(readSlice(fileBytes, self.readCursor, nameLength)?).into_owned();
		self.readCursor += nameLength;

		let mut propertyList = Vec::with_capacity(propertyCount.min(64));
		for _propertyIndex in 0..propertyCount {
			let property = self.readProperty(fileBytes)?;
			propertyList.push(property);
		}

		let mut childNodeList = Vec::new();
		while self.readCursor < endOffset {
			match self.readNodeRecord(fileBytes)? {
				Some(childNode) => {
					childNodeList.push(childNode);
				}
				None => {
					break;
				}
			}
		}
		self.readCursor = endOffset;
		Ok(Some(FbxNodeRecord {
			name: nodeName,
			properties: propertyList,
			children: childNodeList,
		}))
	}

	//==============================================================================
	// 배열 프로퍼티 바이트 읽기. (raw / deflate 인코딩, 원소 바이트를 돌려준다)
	//==============================================================================
	fn readArrayPropertyBytes(&mut self, fileBytes: &[u8], elementByteSize: usize) -> Result<Vec<u8>, String> {
		let arrayLength = readUint32(fileBytes, self.readCursor)? as usize;
		let encoding = readUint32(fileBytes, self.readCursor + 4)?;
		let compressedLength = readUint32(fileBytes, self.readCursor + 8)? as usize;
		self.readCursor += 12;

		let rawByteLength = arrayLength * elementByteSize;
		let sourceBytes = if encoding == 1 {
			let compressedBytes = readSlice(fileBytes, self.readCursor, compressedLength)?;
			let inflatedBytes = FbxLoader::inflate(compressedBytes, rawByteLength)?;
			self.readCursor += compressedLength;
			inflatedBytes
		}
		else {
			let rawBytes = readSlice(fileBytes, self.readCursor, rawByteLength)?.to_vec();
			self.readCursor += rawByteLength;
			rawBytes
		};
		if sourceBytes.len() < rawByteLength {
			return Err(String::from("FbxLoader: array property is shorter than its length."));
		}
		Ok(sourceBytes)
	}

	//==============================================================================
	// deflate 압축 해제. (zlib 머리가 붙은 deflate, 원본 DecompressionStream("deflate"))
	//==============================================================================
	pub fn inflate(compressedBytes: &[u8], expectedByteLength: usize) -> Result<Vec<u8>, String> {
		let mut decompressedBytes = Vec::with_capacity(expectedByteLength);
		let mut zlibDecoder = flate2::read::ZlibDecoder::new(compressedBytes);
		zlibDecoder.read_to_end(&mut decompressedBytes).map_err(|error| format!("FbxLoader: inflate failed: {}", error))?;
		Ok(decompressedBytes)
	}

	//==============================================================================
	// 단일 프로퍼티 읽기.
	//==============================================================================
	fn readProperty(&mut self, fileBytes: &[u8]) -> Result<FbxPropertyValue, String> {
		let typeCode = readUint8(fileBytes, self.readCursor)? as char;
		self.readCursor += 1;
		match typeCode {
			'Y' => {
				let value = readInt16(fileBytes, self.readCursor)?;
				self.readCursor += 2;
				Ok(FbxPropertyValue::int16(value))
			}
			'C' => {
				let value = readUint8(fileBytes, self.readCursor)?;
				self.readCursor += 1;
				Ok(FbxPropertyValue::boolean(value != 0))
			}
			'I' => {
				let value = readInt32(fileBytes, self.readCursor)?;
				self.readCursor += 4;
				Ok(FbxPropertyValue::int32(value))
			}
			'F' => {
				let value = readFloat32(fileBytes, self.readCursor)?;
				self.readCursor += 4;
				Ok(FbxPropertyValue::float32(value))
			}
			'D' => {
				let value = readFloat64(fileBytes, self.readCursor)?;
				self.readCursor += 8;
				Ok(FbxPropertyValue::float64(value))
			}
			'L' => {
				let value = readInt64(fileBytes, self.readCursor)?;
				self.readCursor += 8;
				Ok(FbxPropertyValue::int64(value))
			}
			'f' => {
				let elementBytes = self.readArrayPropertyBytes(fileBytes, 4)?;
				let values = elementBytes.as_chunks::<4>().0.iter().map(|chunk| f32::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::float32Array(values))
			}
			'd' => {
				let elementBytes = self.readArrayPropertyBytes(fileBytes, 8)?;
				let values = elementBytes.as_chunks::<8>().0.iter().map(|chunk| f64::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::float64Array(values))
			}
			'l' => {
				let elementBytes = self.readArrayPropertyBytes(fileBytes, 8)?;
				let values = elementBytes.as_chunks::<8>().0.iter().map(|chunk| i64::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::int64Array(values))
			}
			'i' => {
				let elementBytes = self.readArrayPropertyBytes(fileBytes, 4)?;
				let values = elementBytes.as_chunks::<4>().0.iter().map(|chunk| i32::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::int32Array(values))
			}
			'b' => {
				let elementBytes = self.readArrayPropertyBytes(fileBytes, 1)?;
				Ok(FbxPropertyValue::booleanArray(elementBytes))
			}
			'S' => {
				let stringLength = readUint32(fileBytes, self.readCursor)? as usize;
				self.readCursor += 4;
				let value = String::from_utf8_lossy(readSlice(fileBytes, self.readCursor, stringLength)?).into_owned();
				self.readCursor += stringLength;
				Ok(FbxPropertyValue::string(value))
			}
			'R' => {
				let rawLength = readUint32(fileBytes, self.readCursor)? as usize;
				self.readCursor += 4;
				let value = readSlice(fileBytes, self.readCursor, rawLength)?.to_vec();
				self.readCursor += rawLength;
				Ok(FbxPropertyValue::raw(Rc::new(value)))
			}
			_ => {
				Err(format!("FbxLoader: unknown property type: {}", typeCode))
			}
		}
	}

	//==============================================================================
	// 오브젝트/커넥션 그래프 해석. (중간 표현 구성)
	//==============================================================================
	pub fn interpret(&self, topNodeList: &[FbxNodeRecord]) -> Result<FbxSceneData, String> {
		let objectsNode = topNodeList.iter().find(|node| node.name == "Objects").ok_or_else(|| String::from("FbxLoader: Objects node not found."))?;
		let connectionsNode = topNodeList.iter().find(|node| node.name == "Connections").ok_or_else(|| String::from("FbxLoader: Connections node not found."))?;

		// 커넥션 목록 구성. (OO: 오브젝트-오브젝트, OP: 오브젝트-프로퍼티)
		let mut objectConnectionList = Vec::new();
		let mut propertyConnectionList = Vec::new();
		for connection in connectionsNode.children.iter() {
			let connectionType = connection.getStringProperty(0);
			let sourceId = connection.getProperty(1).and_then(|property| property.asInteger());
			let destinationId = connection.getProperty(2).and_then(|property| property.asInteger());
			let (sourceId, destinationId) = match (sourceId, destinationId) {
				(Some(sourceId), Some(destinationId)) => {
					(sourceId, destinationId)
				}
				_ => {
					continue;
				}
			};
			if connectionType == "OO" {
				objectConnectionList.push(ObjectConnection { sourceId, destinationId });
			}
			else if connectionType == "OP" {
				let propertyName = connection.getStringProperty(3);
				propertyConnectionList.push(PropertyConnection { sourceId, destinationId, propertyName });
			}
		}

		let geometryById = self.interpretGeometries(objectsNode);
		let (mut skinDeformerById, clusterById) = self.interpretSkins(objectsNode, &objectConnectionList);
		let videoById = self.interpretVideos(objectsNode);
		let textureById = self.interpretTextures(objectsNode, &objectConnectionList);
		let materialById = self.interpretMaterials(objectsNode, &propertyConnectionList, &textureById);
		let (nodeList, nodeIdToIndex, rootNodeIndices) = self.interpretModels(objectsNode, &objectConnectionList, &geometryById, &mut skinDeformerById, &materialById);
		let animationList = self.interpretAnimations(objectsNode, &objectConnectionList, &propertyConnectionList, &nodeIdToIndex);

		Ok(FbxSceneData {
			nodeList,
			nodeIdToIndex,
			rootNodeIndices,
			geometryById,
			skinDeformerById,
			clusterById,
			materialById,
			textureById,
			videoById,
			animationList,
		})
	}

	//==============================================================================
	// 지오메트리 해석. (삼각화 + 노멀/UV 레이어 펼침 + 원본 정점 매핑)
	//==============================================================================
	fn interpretGeometries(&self, objectsNode: &FbxNodeRecord) -> FbxObjectMap<FbxGeometry> {
		let mut geometryById = FbxObjectMap::new();
		for objectNode in objectsNode.children.iter() {
			if objectNode.name != "Geometry" {
				continue;
			}
			let geometryId = match objectNode.getObjectId() {
				Some(geometryId) => {
					geometryId
				}
				None => {
					continue;
				}
			};
			if let Some(geometry) = self.interpretSingleGeometry(objectNode) {
				geometryById.insert(geometryId, geometry);
			}
		}
		geometryById
	}

	//==============================================================================
	// 단일 지오메트리 해석. (정점 / 폴리곤 배열이 없으면 None)
	//==============================================================================
	fn interpretSingleGeometry(&self, geometryNode: &FbxNodeRecord) -> Option<FbxGeometry> {
		let positions = geometryNode.findChild("Vertices")?.getProperty(0)?;
		let polygonVertexIndices = geometryNode.findChild("PolygonVertexIndex")?.getProperty(0)?.toIntegerList();

		// 폴리곤 → 팬 삼각화. (코너: 원본 정점 인덱스 + 폴리곤 정점 순번)
		let mut cornerVertexIndices: Vec<i64> = Vec::new();
		let mut cornerPolygonVertexPositions: Vec<usize> = Vec::new();
		let decodeVertexIndex = |rawIndex: i64| -> i64 {
			if rawIndex < 0 {
				-rawIndex - 1
			}
			else {
				rawIndex
			}
		};
		let mut polygonStart = 0;
		for polygonVertexPosition in 0..polygonVertexIndices.len() {
			let isPolygonEnd = polygonVertexIndices[polygonVertexPosition] < 0;
			if isPolygonEnd {
				let polygonEnd = polygonVertexPosition;
				for triangleCorner in (polygonStart + 1)..polygonEnd {
					let firstVertexIndex = decodeVertexIndex(polygonVertexIndices[polygonStart]);
					let secondVertexIndex = polygonVertexIndices[triangleCorner];
					let thirdVertexIndex = decodeVertexIndex(polygonVertexIndices[triangleCorner + 1]);
					cornerVertexIndices.push(firstVertexIndex);
					cornerPolygonVertexPositions.push(polygonStart);
					cornerVertexIndices.push(secondVertexIndex);
					cornerPolygonVertexPositions.push(triangleCorner);
					cornerVertexIndices.push(thirdVertexIndex);
					cornerPolygonVertexPositions.push(triangleCorner + 1);
				}
				polygonStart = polygonVertexPosition + 1;
			}
		}

		let normalReader = self.buildLayerReader(geometryNode, "LayerElementNormal", "Normals", 3);
		let uvReader = self.buildLayerReader(geometryNode, "LayerElementUV", "UV", 2);
		let materialLayer = self.buildMaterialLayer(geometryNode);

		let cornerCount = cornerVertexIndices.len();
		let triangleCount = cornerCount / 3;
		let mut outputPositions = vec![0.0f32; cornerCount * 3];
		let mut outputNormals = vec![0.0f32; cornerCount * 3];
		let mut outputTextureCoordinates = vec![0.0f32; cornerCount * 2];
		let mut outputOriginalVertexIndices = vec![0i32; cornerCount];
		let mut materialIndexPerTriangle = vec![0i32; triangleCount];

		let mut normal = [0.0f64; 3];
		let mut textureCoordinate = [0.0f64; 2];
		for cornerIndex in 0..cornerCount {
			let originalVertexIndex = cornerVertexIndices[cornerIndex];
			let polygonVertexPosition = cornerPolygonVertexPositions[cornerIndex];
			outputOriginalVertexIndices[cornerIndex] = originalVertexIndex as i32;
			let positionOffset = (originalVertexIndex.max(0) as usize) * 3;
			for componentIndex in 0..3 {
				outputPositions[cornerIndex * 3 + componentIndex] = positions.getNumberAt(positionOffset + componentIndex).unwrap_or(0.0) as f32;
			}

			normalReader.read(originalVertexIndex.max(0) as usize, polygonVertexPosition, &mut normal);
			outputNormals[cornerIndex * 3] = normal[0] as f32;
			outputNormals[cornerIndex * 3 + 1] = normal[1] as f32;
			outputNormals[cornerIndex * 3 + 2] = normal[2] as f32;

			uvReader.read(originalVertexIndex.max(0) as usize, polygonVertexPosition, &mut textureCoordinate);
			outputTextureCoordinates[cornerIndex * 2] = textureCoordinate[0] as f32;
			outputTextureCoordinates[cornerIndex * 2 + 1] = textureCoordinate[1] as f32;
		}

		for (triangleIndex, materialIndex) in materialIndexPerTriangle.iter_mut().enumerate() {
			let firstCornerPolygonVertexPosition = cornerPolygonVertexPositions[triangleIndex * 3];
			*materialIndex = materialLayer(firstCornerPolygonVertexPosition);
		}

		Some(FbxGeometry {
			positions: outputPositions,
			normals: outputNormals,
			textureCoordinates: outputTextureCoordinates,
			originalVertexIndices: outputOriginalVertexIndices,
			materialIndexPerTriangle,
			cornerCount,
			originalVertexCount: positions.getArrayLength() / 3,
		})
	}

	//==============================================================================
	// 레이어 리더 구성. (노멀/UV, 매핑/참조 방식별 조회기)
	//==============================================================================
	fn buildLayerReader<'a>(&self, geometryNode: &'a FbxNodeRecord, layerElementName: &str, dataArrayName: &str, componentCount: usize) -> LayerReader<'a> {
		let layerNode = match geometryNode.findChild(layerElementName) {
			Some(layerNode) => {
				layerNode
			}
			None => {
				return LayerReader {
					dataArray: None,
					mappingType: String::new(),
					referenceType: String::new(),
					indexArray: None,
					componentCount,
				};
			}
		};
		let indexArrayName = if dataArrayName == "UV" {
			String::from("UVIndex")
		}
		else {
			format!("{}Index", dataArrayName)
		};
		let dataArray = layerNode.findChild(dataArrayName).and_then(|dataNode| dataNode.getProperty(0));
		let mappingType = layerNode.findChild("MappingInformationType").map(|mappingNode| mappingNode.getStringProperty(0)).unwrap_or_default();
		let referenceType = layerNode.findChild("ReferenceInformationType").map(|referenceNode| referenceNode.getStringProperty(0)).unwrap_or_default();
		let indexArray = layerNode.findChild(&indexArrayName).and_then(|indexNode| indexNode.getProperty(0));
		LayerReader {
			dataArray,
			mappingType,
			referenceType,
			indexArray,
			componentCount,
		}
	}

	//==============================================================================
	// 머티리얼 레이어 구성. (폴리곤 정점 순번 → 머티리얼 인덱스, 원본처럼 순번으로 바로 찾는다)
	//==============================================================================
	fn buildMaterialLayer<'a>(&self, geometryNode: &'a FbxNodeRecord) -> Box<dyn Fn(usize) -> i32 + 'a> {
		let layerNode = match geometryNode.findChild("LayerElementMaterial") {
			Some(layerNode) => {
				layerNode
			}
			None => {
				return Box::new(|_polygonVertexPosition| 0);
			}
		};
		let mappingType = layerNode.findChild("MappingInformationType").map(|mappingNode| mappingNode.getStringProperty(0)).unwrap_or_default();
		let materialArray = layerNode.findChild("Materials").and_then(|materialsNode| materialsNode.getProperty(0));
		Box::new(move |polygonVertexPosition| {
			let materialArray = match materialArray {
				Some(materialArray) => {
					materialArray
				}
				None => {
					return 0;
				}
			};
			if mappingType == "AllSame" {
				return materialArray.getIntegerAt(0).unwrap_or(0) as i32;
			}
			materialArray.getIntegerAt(polygonVertexPosition).unwrap_or(0) as i32
		})
	}

	//==============================================================================
	// 스킨/클러스터 해석. (본별 정점 가중치 + 바인드 역행렬용 링크 변환)
	//==============================================================================
	fn interpretSkins(&self, objectsNode: &FbxNodeRecord, objectConnectionList: &[ObjectConnection]) -> (FbxObjectMap<FbxSkinDeformer>, FbxObjectMap<FbxCluster>) {
		let mut skinDeformerById = FbxObjectMap::new();
		let mut clusterById = FbxObjectMap::new();
		for objectNode in objectsNode.children.iter() {
			if objectNode.name != "Deformer" {
				continue;
			}
			let deformerId = match objectNode.getObjectId() {
				Some(deformerId) => {
					deformerId
				}
				None => {
					continue;
				}
			};
			let deformerSubType = objectNode.getStringProperty(2);
			if deformerSubType == "Skin" {
				skinDeformerById.insert(deformerId, FbxSkinDeformer::default());
			}
			else if deformerSubType == "Cluster" {
				let readArray = |childName: &str| objectNode.findChild(childName).and_then(|child| child.getProperty(0));
				let cluster = FbxCluster {
					vertexIndices: readArray("Indexes").map(|property| property.toIntegerList()).unwrap_or_default(),
					weights: readArray("Weights").map(|property| property.toNumberList()).unwrap_or_default(),
					transform: readArray("Transform").map(|property| property.toNumberList()),
					transformLink: readArray("TransformLink").map(|property| property.toNumberList()),
					boneNodeId: None,
				};
				clusterById.insert(deformerId, cluster);
			}
		}

		// 커넥션으로 스킨↔클러스터, 클러스터↔본 연결.
		for connection in objectConnectionList.iter() {
			if skinDeformerById.contains(connection.destinationId) && clusterById.contains(connection.sourceId) {
				if let Some(skinDeformer) = skinDeformerById.getMut(connection.destinationId) {
					skinDeformer.clusterIds.push(connection.sourceId);
				}
			}
			else if let Some(cluster) = clusterById.getMut(connection.destinationId) {
				cluster.boneNodeId = Some(connection.sourceId);
			}
		}
		(skinDeformerById, clusterById)
	}

	//==============================================================================
	// 비디오 해석. (임베디드 이미지 바이트)
	//==============================================================================
	fn interpretVideos(&self, objectsNode: &FbxNodeRecord) -> FbxObjectMap<FbxVideo> {
		let mut videoById = FbxObjectMap::new();
		for objectNode in objectsNode.children.iter() {
			if objectNode.name != "Video" {
				continue;
			}
			let videoId = match objectNode.getObjectId() {
				Some(videoId) => {
					videoId
				}
				None => {
					continue;
				}
			};
			let contentBytes = match objectNode.findChild("Content").and_then(|contentNode| contentNode.getProperty(0)) {
				Some(FbxPropertyValue::raw(bytes)) => {
					Some(bytes.clone())
				}
				_ => {
					None
				}
			};
			let filename = objectNode.children.iter().find(|child| child.name == "RelativeFilename" || child.name == "Filename").map(|filenameNode| filenameNode.getStringProperty(0)).unwrap_or_default();
			videoById.insert(videoId, FbxVideo { contentBytes, filename });
		}

		// 동일 이미지 중복 참조 보완. (같은 파일을 두 번 참조하면 두 번째 Content 는 비어 있다)
		let mut contentByFileName: HashMap<String, Rc<Vec<u8>>> = HashMap::new();
		for video in videoById.values().iter() {
			let contentBytes = match &video.contentBytes {
				Some(contentBytes) if !contentBytes.is_empty() => {
					contentBytes
				}
				_ => {
					continue;
				}
			};
			let fileName = FbxLoader::extractFileName(&video.filename);
			contentByFileName.entry(fileName).or_insert_with(|| contentBytes.clone());
		}
		for video in videoById.valuesMut().iter_mut() {
			let hasContent = video.contentBytes.as_ref().map(|contentBytes| !contentBytes.is_empty()).unwrap_or(false);
			if hasContent {
				continue;
			}
			let fileName = FbxLoader::extractFileName(&video.filename);
			if let Some(sharedContentBytes) = contentByFileName.get(&fileName) {
				video.contentBytes = Some(sharedContentBytes.clone());
			}
		}
		videoById
	}

	//==============================================================================
	// 파일 경로에서 파일명 추출. (윈도우/유닉스 구분자 모두 대응)
	//==============================================================================
	pub fn extractFileName(filePath: &str) -> String {
		let normalizedPath = filePath.replace('\\', "/");
		match normalizedPath.rfind('/') {
			Some(lastSeparatorIndex) => {
				normalizedPath[lastSeparatorIndex + 1..].to_string()
			}
			None => {
				normalizedPath
			}
		}
	}

	//==============================================================================
	// 텍스처 해석. (비디오 연결)
	//==============================================================================
	fn interpretTextures(&self, objectsNode: &FbxNodeRecord, objectConnectionList: &[ObjectConnection]) -> FbxObjectMap<FbxTexture> {
		let mut textureById = FbxObjectMap::new();
		for objectNode in objectsNode.children.iter() {
			if objectNode.name != "Texture" {
				continue;
			}
			let textureId = match objectNode.getObjectId() {
				Some(textureId) => {
					textureId
				}
				None => {
					continue;
				}
			};
			let filename = objectNode.children.iter().find(|child| child.name == "RelativeFilename" || child.name == "FileName").map(|filenameNode| filenameNode.getStringProperty(0)).unwrap_or_default();
			textureById.insert(textureId, FbxTexture { videoId: None, filename });
		}
		for connection in objectConnectionList.iter() {
			if let Some(texture) = textureById.getMut(connection.destinationId) {
				texture.videoId = Some(connection.sourceId);
			}
		}
		textureById
	}

	//==============================================================================
	// 머티리얼 해석. (phong 색상/광택 + 슬롯별 텍스처 연결)
	//==============================================================================
	fn interpretMaterials(&self, objectsNode: &FbxNodeRecord, propertyConnectionList: &[PropertyConnection], textureById: &FbxObjectMap<FbxTexture>) -> FbxObjectMap<FbxMaterial> {
		let mut materialById = FbxObjectMap::new();
		for objectNode in objectsNode.children.iter() {
			if objectNode.name != "Material" {
				continue;
			}
			let materialId = match objectNode.getObjectId() {
				Some(materialId) => {
					materialId
				}
				None => {
					continue;
				}
			};
			let mut material = FbxMaterial {
				diffuseColor: [1.0, 1.0, 1.0],
				specularColor: [0.5, 0.5, 0.5],
				shininess: 20.0,
				diffuseTextureId: None,
				normalTextureId: None,
				specularTextureId: None,
				glossTextureId: None,
				opacityTextureId: None,
			};
			if let Some(properties70Node) = objectNode.findChild("Properties70") {
				for propertyRecord in properties70Node.children.iter() {
					let propertyKey = propertyRecord.getStringProperty(0);
					if propertyKey == "DiffuseColor" || propertyKey == "Diffuse" {
						if let Some(color) = readPropertyVector(propertyRecord) {
							material.diffuseColor = color;
						}
					}
					else if propertyKey == "SpecularColor" || propertyKey == "Specular" {
						if let Some(color) = readPropertyVector(propertyRecord) {
							material.specularColor = color;
						}
					}
					else if propertyKey == "Shininess" || propertyKey == "ShininessExponent" {
						if let Some(shininess) = propertyRecord.getProperty(4).and_then(|property| property.asNumber()) {
							material.shininess = shininess;
						}
					}
				}
			}
			materialById.insert(materialId, material);
		}

		// 텍스처 → 머티리얼 슬롯 연결. (OP 프로퍼티명으로 판별)
		for connection in propertyConnectionList.iter() {
			if !textureById.contains(connection.sourceId) {
				continue;
			}
			let material = match materialById.getMut(connection.destinationId) {
				Some(material) => {
					material
				}
				None => {
					continue;
				}
			};
			let propertyName = connection.propertyName.as_str();
			if propertyName == "DiffuseColor" {
				material.diffuseTextureId = Some(connection.sourceId);
			}
			else if propertyName == "NormalMap" || propertyName == "Bump" {
				material.normalTextureId = Some(connection.sourceId);
			}
			else if propertyName == "SpecularColor" || propertyName == "SpecularFactor" {
				material.specularTextureId = Some(connection.sourceId);
			}
			else if propertyName == "Shininess" || propertyName == "ShininessExponent" {
				material.glossTextureId = Some(connection.sourceId);
			}
			else if propertyName == "TransparentColor" || propertyName == "TransparencyFactor" {
				material.opacityTextureId = Some(connection.sourceId);
			}
		}
		materialById
	}

	//==============================================================================
	// 모델(노드) 해석. (계층 + 로컬 변환 + 지오메트리/스킨/머티리얼 소속)
	//==============================================================================
	fn interpretModels(&self, objectsNode: &FbxNodeRecord, objectConnectionList: &[ObjectConnection], geometryById: &FbxObjectMap<FbxGeometry>, skinDeformerById: &mut FbxObjectMap<FbxSkinDeformer>, materialById: &FbxObjectMap<FbxMaterial>) -> (Vec<FbxNode>, HashMap<i64, usize>, Vec<usize>) {
		let mut nodeList: Vec<FbxNode> = Vec::new();
		let mut nodeIdToIndex: HashMap<i64, usize> = HashMap::new();
		for objectNode in objectsNode.children.iter() {
			if objectNode.name != "Model" {
				continue;
			}
			let modelId = match objectNode.getObjectId() {
				Some(modelId) => {
					modelId
				}
				None => {
					continue;
				}
			};
			let modelName = objectNode.getStringProperty(1);
			let modelSubType = objectNode.getStringProperty(2);
			let mut node = FbxNode {
				id: modelId,
				name: modelName,
				isBone: modelSubType == "LimbNode",
				translation: [0.0, 0.0, 0.0],
				rotationEulerDegrees: [0.0, 0.0, 0.0],
				preRotationDegrees: [0.0, 0.0, 0.0],
				scale: [1.0, 1.0, 1.0],
				geometryId: None,
				skinId: None,
				materialIds: Vec::new(),
				childIndices: Vec::new(),
				parentId: None,
			};
			if let Some(properties70Node) = objectNode.findChild("Properties70") {
				for propertyRecord in properties70Node.children.iter() {
					let propertyKey = propertyRecord.getStringProperty(0);
					let propertyVector = match readPropertyVector(propertyRecord) {
						Some(propertyVector) => {
							propertyVector
						}
						None => {
							continue;
						}
					};
					if propertyKey == "Lcl Translation" {
						node.translation = propertyVector;
					}
					else if propertyKey == "Lcl Rotation" {
						node.rotationEulerDegrees = propertyVector;
					}
					else if propertyKey == "Lcl Scaling" {
						node.scale = propertyVector;
					}
					else if propertyKey == "PreRotation" {
						node.preRotationDegrees = propertyVector;
					}
				}
			}
			nodeIdToIndex.insert(modelId, nodeList.len());
			nodeList.push(node);
		}

		// 커넥션으로 계층/지오메트리/스킨/머티리얼 소속 구성.
		let mut rootNodeIndices = Vec::new();
		for connection in objectConnectionList.iter() {
			let sourceNodeIndex = nodeIdToIndex.get(&connection.sourceId).copied();
			let destinationNodeIndex = nodeIdToIndex.get(&connection.destinationId).copied();
			if sourceNodeIndex.is_some() && connection.destinationId == 0 {
				rootNodeIndices.push(sourceNodeIndex.unwrap_or_default());
			}
			else if let (Some(childIndex), Some(parentIndex)) = (sourceNodeIndex, destinationNodeIndex) {
				nodeList[parentIndex].childIndices.push(childIndex);
				nodeList[childIndex].parentId = Some(connection.destinationId);
			}
			else if geometryById.contains(connection.sourceId) && destinationNodeIndex.is_some() {
				let ownerNode = &mut nodeList[destinationNodeIndex.unwrap_or_default()];
				ownerNode.geometryId = Some(connection.sourceId);
			}
			else if skinDeformerById.contains(connection.sourceId) && geometryById.contains(connection.destinationId) {
				// 스킨은 지오메트리에 붙는다, 지오메트리를 소유한 노드에서 역참조하도록 지오메트리에 기록.
				if let Some(skinDeformer) = skinDeformerById.getMut(connection.sourceId) {
					skinDeformer.geometryId = Some(connection.destinationId);
				}
			}
			else if materialById.contains(connection.sourceId) && destinationNodeIndex.is_some() {
				let ownerNode = &mut nodeList[destinationNodeIndex.unwrap_or_default()];
				ownerNode.materialIds.push(connection.sourceId);
			}
		}

		// 지오메트리 → 스킨 역참조를 노드에 반영.
		for (skinId, skinDeformer) in skinDeformerById.iter() {
			let skinGeometryId = match skinDeformer.geometryId {
				Some(skinGeometryId) => {
					skinGeometryId
				}
				None => {
					continue;
				}
			};
			for node in nodeList.iter_mut() {
				if node.geometryId == Some(skinGeometryId) {
					node.skinId = Some(skinId);
				}
			}
		}
		(nodeList, nodeIdToIndex, rootNodeIndices)
	}

	//==============================================================================
	// 애니메이션 해석. (스택 → 레이어 → 커브노드(T/R/S) → 커브(X/Y/Z))
	//==============================================================================
	fn interpretAnimations(&self, objectsNode: &FbxNodeRecord, objectConnectionList: &[ObjectConnection], propertyConnectionList: &[PropertyConnection], nodeIdToIndex: &HashMap<i64, usize>) -> Vec<FbxAnimation> {
		// 커브 / 커브노드 / 레이어 / 스택 수집.
		let mut curveById: FbxObjectMap<AnimationCurve> = FbxObjectMap::new();
		let mut curveNodeById: FbxObjectMap<AnimationCurveNode> = FbxObjectMap::new();
		let mut layerIdSet: HashSet<i64> = HashSet::new();
		let mut stackById: FbxObjectMap<AnimationStack> = FbxObjectMap::new();
		for objectNode in objectsNode.children.iter() {
			let objectId = match objectNode.getObjectId() {
				Some(objectId) => {
					objectId
				}
				None => {
					continue;
				}
			};
			if objectNode.name == "AnimationCurve" {
				let keyTimes = objectNode.findChild("KeyTime").and_then(|keyTimeNode| keyTimeNode.getProperty(0)).map(|property| property.toIntegerList()).unwrap_or_default();
				let keyValues = objectNode.findChild("KeyValueFloat").and_then(|keyValueNode| keyValueNode.getProperty(0)).map(|property| property.toNumberList()).unwrap_or_default();
				curveById.insert(objectId, AnimationCurve { keyTimes, keyValues });
			}
			else if objectNode.name == "AnimationCurveNode" {
				curveNodeById.insert(objectId, AnimationCurveNode { targetNodeId: None, targetProperty: None, curveByChannel: HashMap::new() });
			}
			else if objectNode.name == "AnimationLayer" {
				layerIdSet.insert(objectId);
			}
			else if objectNode.name == "AnimationStack" {
				stackById.insert(objectId, AnimationStack { name: objectNode.getStringProperty(1), layerIds: Vec::new() });
			}
		}

		// 커브 → 커브노드 채널(d|X/d|Y/d|Z) 연결.
		for connection in propertyConnectionList.iter() {
			if curveById.contains(connection.sourceId) && curveNodeById.contains(connection.destinationId) {
				if let Some(curveNode) = curveNodeById.getMut(connection.destinationId) {
					let channelName = connection.propertyName.replacen("d|", "", 1);
					curveNode.curveByChannel.insert(channelName, connection.sourceId);
				}
			}
			else if curveNodeById.contains(connection.sourceId) && nodeIdToIndex.contains_key(&connection.destinationId) {
				if let Some(curveNode) = curveNodeById.getMut(connection.sourceId) {
					curveNode.targetNodeId = Some(connection.destinationId);
					curveNode.targetProperty = Some(connection.propertyName.clone());
				}
			}
		}

		// 레이어 → 스택 소속 + 커브노드 → 레이어 소속.
		let mut layerIdByCurveNodeId: HashMap<i64, i64> = HashMap::new();
		for connection in objectConnectionList.iter() {
			if layerIdSet.contains(&connection.sourceId) && stackById.contains(connection.destinationId) {
				if let Some(stack) = stackById.getMut(connection.destinationId) {
					stack.layerIds.push(connection.sourceId);
				}
			}
			else if curveNodeById.contains(connection.sourceId) && layerIdSet.contains(&connection.destinationId) {
				layerIdByCurveNodeId.insert(connection.sourceId, connection.destinationId);
			}
		}

		// 스택 → 애니메이션 클립. (스택 소속 레이어의 커브노드만 노드/프로퍼티별 채널로 변환)
		let mut animationList = Vec::new();
		for (_stackId, stack) in stackById.iter() {
			let mut channelList = Vec::new();
			let mut duration = 0.0f32;
			for (curveNodeId, curveNode) in curveNodeById.iter() {
				let targetNodeId = match curveNode.targetNodeId {
					Some(targetNodeId) if nodeIdToIndex.contains_key(&targetNodeId) => {
						targetNodeId
					}
					_ => {
						continue;
					}
				};
				let owningLayerId = layerIdByCurveNodeId.get(&curveNodeId);
				let isInStack = match owningLayerId {
					Some(owningLayerId) => {
						stack.layerIds.contains(owningLayerId)
					}
					None => {
						false
					}
				};
				if !isInStack {
					continue;
				}
				if let Some((channel, channelDuration)) = self.buildAnimationChannel(curveNode, targetNodeId, &curveById, nodeIdToIndex) {
					channelList.push(channel);
					duration = duration.max(channelDuration);
				}
			}
			animationList.push(FbxAnimation {
				name: stack.name.clone(),
				duration,
				channels: channelList,
			});
		}
		animationList
	}

	//==============================================================================
	// 애니메이션 채널 구성. (커브노드 → 노드 인덱스 + 경로 + 키타임/값, 채널과 길이를 돌려준다)
	//==============================================================================
	fn buildAnimationChannel(&self, curveNode: &AnimationCurveNode, targetNodeId: i64, curveById: &FbxObjectMap<AnimationCurve>, nodeIdToIndex: &HashMap<i64, usize>) -> Option<(FbxAnimationChannel, f32)> {
		let path = match curveNode.targetProperty.as_deref() {
			Some("Lcl Translation") => {
				AnimationPath::translation
			}
			Some("Lcl Rotation") => {
				AnimationPath::rotation
			}
			Some("Lcl Scaling") => {
				AnimationPath::scale
			}
			_ => {
				return None;
			}
		};

		// 채널별 키타임 합집합 구성. (X/Y/Z 커브의 시간축이 동일하다고 가정, 아니면 합집합 정렬)
		let channelNames = ["X", "Y", "Z"];
		let mut timeSet: HashSet<i64> = HashSet::new();
		for channelName in channelNames.iter() {
			let curve = curveNode.curveByChannel.get(*channelName).and_then(|curveId| curveById.get(*curveId));
			if let Some(curve) = curve {
				for keyTime in curve.keyTimes.iter() {
					timeSet.insert(*keyTime);
				}
			}
		}
		let mut sortedTimes: Vec<i64> = timeSet.into_iter().collect();
		sortedTimes.sort_unstable();
		if sortedTimes.is_empty() {
			return None;
		}

		let componentCount = 3;
		let mut times = vec![0.0f32; sortedTimes.len()];
		let mut values = vec![0.0f32; sortedTimes.len() * componentCount];
		for (timeIndex, sortedTime) in sortedTimes.iter().enumerate() {
			times[timeIndex] = (*sortedTime as f64 / FBX_TIME_UNIT) as f32;
			for (channelIndex, channelName) in channelNames.iter().enumerate() {
				let curve = curveNode.curveByChannel.get(*channelName).and_then(|curveId| curveById.get(*curveId));
				let sampledValue = FbxLoader::sampleCurveAtTime(curve, *sortedTime);
				values[timeIndex * componentCount + channelIndex] = sampledValue as f32;
			}
		}

		let nodeIndex = *nodeIdToIndex.get(&targetNodeId)?;
		let duration = *times.last().unwrap_or(&0.0);
		let channel = FbxAnimationChannel {
			nodeIndex,
			path,
			times,
			values,
		};
		Some((channel, duration))
	}

	//==============================================================================
	// 커브 시간 샘플링. (선형 보간, 키 사이, 커브가 없으면 0)
	//==============================================================================
	fn sampleCurveAtTime(curve: Option<&AnimationCurve>, keyTime: i64) -> f64 {
		let curve = match curve {
			Some(curve) => {
				curve
			}
			None => {
				return 0.0;
			}
		};
		let keyTimes = &curve.keyTimes;
		let keyValues = &curve.keyValues;
		if keyTimes.is_empty() || keyValues.is_empty() {
			return 0.0;
		}
		if keyTime <= keyTimes[0] {
			return keyValues[0];
		}
		if keyTime >= keyTimes[keyTimes.len() - 1] {
			return keyValues[keyValues.len() - 1];
		}
		let mut keyIndex = 0;
		while keyIndex < keyTimes.len() - 1 && keyTimes[keyIndex + 1] < keyTime {
			keyIndex += 1;
		}
		let spanDuration = (keyTimes[keyIndex + 1] - keyTimes[keyIndex]) as f64;
		let factor = if spanDuration > 0.0 {
			(keyTime - keyTimes[keyIndex]) as f64 / spanDuration
		}
		else {
			0.0
		};
		let fromValue = keyValues.get(keyIndex).copied().unwrap_or(0.0);
		let toValue = keyValues.get(keyIndex + 1).copied().unwrap_or(fromValue);
		fromValue + (toValue - fromValue) * factor
	}
}

//==============================================================================
// Properties70 레코드의 벡터 값 읽기. (P: 이름, 타입, 레이블, 플래그, x, y, z, 모자라면 None)
//==============================================================================
fn readPropertyVector(propertyRecord: &FbxNodeRecord) -> Option<[f64; 3]> {
	let x = propertyRecord.getProperty(4)?.asNumber()?;
	let y = propertyRecord.getProperty(5)?.asNumber()?;
	let z = propertyRecord.getProperty(6)?.asNumber()?;
	Some([x, y, z])
}

//==============================================================================
// 바이트 구간 읽기. (파일 밖이면 오류, 원본 DataView 의 RangeError 자리)
//==============================================================================
fn readSlice(fileBytes: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
	let endOffset = offset.checked_add(length).ok_or_else(|| String::from("FbxLoader: offset overflow."))?;
	fileBytes.get(offset..endOffset).ok_or_else(|| String::from("FbxLoader: unexpected end of file."))
}

//==============================================================================
// 부호 없는 8비트 읽기.
//==============================================================================
fn readUint8(fileBytes: &[u8], offset: usize) -> Result<u8, String> {
	Ok(readSlice(fileBytes, offset, 1)?[0])
}

//==============================================================================
// 부호 있는 16비트 읽기. (리틀 엔디언)
//==============================================================================
fn readInt16(fileBytes: &[u8], offset: usize) -> Result<i16, String> {
	let bytes = readSlice(fileBytes, offset, 2)?;
	Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
}

//==============================================================================
// 부호 없는 32비트 읽기. (리틀 엔디언)
//==============================================================================
fn readUint32(fileBytes: &[u8], offset: usize) -> Result<u32, String> {
	let bytes = readSlice(fileBytes, offset, 4)?;
	Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

//==============================================================================
// 부호 있는 32비트 읽기. (리틀 엔디언)
//==============================================================================
fn readInt32(fileBytes: &[u8], offset: usize) -> Result<i32, String> {
	let bytes = readSlice(fileBytes, offset, 4)?;
	Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

//==============================================================================
// 부호 없는 64비트 읽기. (리틀 엔디언)
//==============================================================================
fn readUint64(fileBytes: &[u8], offset: usize) -> Result<u64, String> {
	let bytes = readSlice(fileBytes, offset, 8)?;
	Ok(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
}

//==============================================================================
// 부호 있는 64비트 읽기. (리틀 엔디언)
//==============================================================================
fn readInt64(fileBytes: &[u8], offset: usize) -> Result<i64, String> {
	let bytes = readSlice(fileBytes, offset, 8)?;
	Ok(i64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
}

//==============================================================================
// 32비트 실수 읽기. (리틀 엔디언)
//==============================================================================
fn readFloat32(fileBytes: &[u8], offset: usize) -> Result<f32, String> {
	let bytes = readSlice(fileBytes, offset, 4)?;
	Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

//==============================================================================
// 64비트 실수 읽기. (리틀 엔디언)
//==============================================================================
fn readFloat64(fileBytes: &[u8], offset: usize) -> Result<f64, String> {
	let bytes = readSlice(fileBytes, offset, 8)?;
	Ok(f64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graphics3d::skinnedmodel::SkinnedModel;
	use std::io::Write;

	// 예제 애셋 폴더. (이 저장소 옆에 받아 둔 vanilla.js 저장소, 읽기만 한다. 없으면 그 시험은 건너뛴다)
	const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

	//==============================================================================
	// 시험용 노드 레코드. (이름 + 인코딩된 프로퍼티 + 자식)
	//==============================================================================
	struct TestNode {
		name: String,
		propertyBytesList: Vec<Vec<u8>>,
		children: Vec<TestNode>,
	}

	//==============================================================================
	// 시험용 노드 만들기.
	//==============================================================================
	fn testNode(name: &str, propertyBytesList: Vec<Vec<u8>>, children: Vec<TestNode>) -> TestNode {
		TestNode {
			name: name.to_string(),
			propertyBytesList,
			children,
		}
	}

	//==============================================================================
	// 노드 레코드 인코딩. (7.4 형식 32비트 오프셋, startOffset 은 파일 안 절대 위치)
	//==============================================================================
	fn encodeNode(node: &TestNode, startOffset: usize) -> Vec<u8> {
		let propertyBytes: Vec<u8> = node.propertyBytesList.concat();
		let headerLength = 13 + node.name.len();
		let mut childBytes = Vec::new();
		let mut childOffset = startOffset + headerLength + propertyBytes.len();
		for child in node.children.iter() {
			let encodedChild = encodeNode(child, childOffset);
			childOffset += encodedChild.len();
			childBytes.extend_from_slice(&encodedChild);
		}
		if !node.children.is_empty() {
			childBytes.extend_from_slice(&[0u8; 13]);
		}
		let endOffset = startOffset + headerLength + propertyBytes.len() + childBytes.len();
		let mut encodedBytes = Vec::new();
		encodedBytes.extend_from_slice(&(endOffset as u32).to_le_bytes());
		encodedBytes.extend_from_slice(&(node.propertyBytesList.len() as u32).to_le_bytes());
		encodedBytes.extend_from_slice(&(propertyBytes.len() as u32).to_le_bytes());
		encodedBytes.push(node.name.len() as u8);
		encodedBytes.extend_from_slice(node.name.as_bytes());
		encodedBytes.extend_from_slice(&propertyBytes);
		encodedBytes.extend_from_slice(&childBytes);
		encodedBytes
	}

	//==============================================================================
	// 프로퍼티 인코딩: 64비트 정수.
	//==============================================================================
	fn propertyLong(value: i64) -> Vec<u8> {
		let mut bytes = vec![b'L'];
		bytes.extend_from_slice(&value.to_le_bytes());
		bytes
	}

	//==============================================================================
	// 프로퍼티 인코딩: 문자열.
	//==============================================================================
	fn propertyString(value: &str) -> Vec<u8> {
		let mut bytes = vec![b'S'];
		bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
		bytes.extend_from_slice(value.as_bytes());
		bytes
	}

	//==============================================================================
	// 프로퍼티 인코딩: 64비트 실수.
	//==============================================================================
	fn propertyDouble(value: f64) -> Vec<u8> {
		let mut bytes = vec![b'D'];
		bytes.extend_from_slice(&value.to_le_bytes());
		bytes
	}

	//==============================================================================
	// 프로퍼티 인코딩: 배열. (원소 바이트, 압축이면 zlib deflate)
	//==============================================================================
	fn propertyArray(typeCode: u8, elementBytes: Vec<u8>, elementCount: usize, isCompressed: bool) -> Vec<u8> {
		let mut bytes = vec![typeCode];
		bytes.extend_from_slice(&(elementCount as u32).to_le_bytes());
		if isCompressed {
			let mut zlibEncoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
			zlibEncoder.write_all(&elementBytes).expect("deflate");
			let compressedBytes = zlibEncoder.finish().expect("deflate finish");
			bytes.extend_from_slice(&1u32.to_le_bytes());
			bytes.extend_from_slice(&(compressedBytes.len() as u32).to_le_bytes());
			bytes.extend_from_slice(&compressedBytes);
		}
		else {
			bytes.extend_from_slice(&0u32.to_le_bytes());
			bytes.extend_from_slice(&(elementBytes.len() as u32).to_le_bytes());
			bytes.extend_from_slice(&elementBytes);
		}
		bytes
	}

	//==============================================================================
	// 프로퍼티 인코딩: 64비트 실수 배열.
	//==============================================================================
	fn propertyDoubleArray(values: &[f64], isCompressed: bool) -> Vec<u8> {
		propertyArray(b'd', values.iter().flat_map(|value| value.to_le_bytes()).collect(), values.len(), isCompressed)
	}

	//==============================================================================
	// 프로퍼티 인코딩: 32비트 정수 배열.
	//==============================================================================
	fn propertyIntegerArray(values: &[i32]) -> Vec<u8> {
		propertyArray(b'i', values.iter().flat_map(|value| value.to_le_bytes()).collect(), values.len(), false)
	}

	//==============================================================================
	// 문자열 하나를 가진 자식 노드.
	//==============================================================================
	fn stringNode(name: &str, value: &str) -> TestNode {
		testNode(name, vec![propertyString(value)], Vec::new())
	}

	//==============================================================================
	// 커넥션 노드. (OP 면 프로퍼티 이름까지)
	//==============================================================================
	fn connectionNode(connectionType: &str, sourceId: i64, destinationId: i64, propertyName: Option<&str>) -> TestNode {
		let mut propertyBytesList = vec![propertyString(connectionType), propertyLong(sourceId), propertyLong(destinationId)];
		if let Some(propertyName) = propertyName {
			propertyBytesList.push(propertyString(propertyName));
		}
		testNode("C", propertyBytesList, Vec::new())
	}

	//==============================================================================
	// 시험용 바이너리 FBX. (사각형 지오메트리 + 본 + 메시 노드 + 스킨/클러스터 + Y 이동 커브 하나)
	//==============================================================================
	fn buildTestFbx() -> Vec<u8> {
		let quadVertices = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
		let geometryNode = testNode("Geometry", vec![propertyLong(100), propertyString("Plane\u{0}\u{1}Geometry"), propertyString("Mesh")], vec![
			testNode("Vertices", vec![propertyDoubleArray(&quadVertices, true)], Vec::new()),
			testNode("PolygonVertexIndex", vec![propertyIntegerArray(&[0, 1, 2, -4])], Vec::new()),
			testNode("LayerElementNormal", vec![], vec![
				stringNode("MappingInformationType", "ByPolygonVertex"),
				stringNode("ReferenceInformationType", "Direct"),
				testNode("Normals", vec![propertyDoubleArray(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], false)], Vec::new()),
			]),
			testNode("LayerElementUV", vec![], vec![
				stringNode("MappingInformationType", "ByPolygonVertex"),
				stringNode("ReferenceInformationType", "IndexToDirect"),
				testNode("UV", vec![propertyDoubleArray(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0], false)], Vec::new()),
				testNode("UVIndex", vec![propertyIntegerArray(&[0, 1, 2, 3])], Vec::new()),
			]),
		]);
		let translationRecord = testNode("P", vec![propertyString("Lcl Translation"), propertyString("Lcl Translation"), propertyString(""), propertyString("A"), propertyDouble(1.0), propertyDouble(2.0), propertyDouble(3.0)], Vec::new());
		let boneNode = testNode("Model", vec![propertyLong(200), propertyString("Bone\u{0}\u{1}Model"), propertyString("LimbNode")], vec![testNode("Properties70", vec![], vec![translationRecord])]);
		let meshNode = testNode("Model", vec![propertyLong(201), propertyString("Mesh\u{0}\u{1}Model"), propertyString("Mesh")], Vec::new());
		let skinNode = testNode("Deformer", vec![propertyLong(300), propertyString("Skin"), propertyString("Skin")], Vec::new());
		let mut transformLink = [0.0f64; 16];
		transformLink[0] = 1.0;
		transformLink[5] = 1.0;
		transformLink[10] = 1.0;
		transformLink[15] = 1.0;
		transformLink[12] = 1.0;
		transformLink[13] = 2.0;
		transformLink[14] = 3.0;
		let clusterNode = testNode("Deformer", vec![propertyLong(301), propertyString("Cluster"), propertyString("Cluster")], vec![
			testNode("Indexes", vec![propertyIntegerArray(&[0, 1, 2, 3])], Vec::new()),
			testNode("Weights", vec![propertyDoubleArray(&[1.0, 1.0, 1.0, 1.0], false)], Vec::new()),
			testNode("TransformLink", vec![propertyDoubleArray(&transformLink, false)], Vec::new()),
		]);
		let stackNode = testNode("AnimationStack", vec![propertyLong(400), propertyString("Take\u{0}\u{1}AnimStack"), propertyString("")], Vec::new());
		let layerNode = testNode("AnimationLayer", vec![propertyLong(401), propertyString("Layer"), propertyString("")], Vec::new());
		let curveNodeNode = testNode("AnimationCurveNode", vec![propertyLong(402), propertyString("T"), propertyString("")], Vec::new());
		let keyTimeBytes: Vec<u8> = [0i64, FBX_TIME_UNIT as i64].iter().flat_map(|value| value.to_le_bytes()).collect();
		let keyValueBytes: Vec<u8> = [1.0f32, 3.0].iter().flat_map(|value| value.to_le_bytes()).collect();
		let curveNode = testNode("AnimationCurve", vec![propertyLong(403), propertyString(""), propertyString("")], vec![
			testNode("KeyTime", vec![propertyArray(b'l', keyTimeBytes, 2, true)], Vec::new()),
			testNode("KeyValueFloat", vec![propertyArray(b'f', keyValueBytes, 2, false)], Vec::new()),
		]);
		let objectsNode = testNode("Objects", vec![], vec![geometryNode, boneNode, meshNode, skinNode, clusterNode, stackNode, layerNode, curveNodeNode, curveNode]);
		let connectionsNode = testNode("Connections", vec![], vec![
			connectionNode("OO", 200, 0, None),
			connectionNode("OO", 201, 0, None),
			connectionNode("OO", 100, 201, None),
			connectionNode("OO", 300, 100, None),
			connectionNode("OO", 301, 300, None),
			connectionNode("OO", 200, 301, None),
			connectionNode("OO", 401, 400, None),
			connectionNode("OO", 402, 401, None),
			connectionNode("OP", 402, 200, Some("Lcl Translation")),
			connectionNode("OP", 403, 402, Some("d|Y")),
		]);

		let mut fileBytes = Vec::new();
		fileBytes.extend_from_slice(b"Kaydara FBX Binary  \x00\x1a\x00");
		fileBytes.extend_from_slice(&7400u32.to_le_bytes());
		for topNode in [objectsNode, connectionsNode].iter() {
			let encodedTopNode = encodeNode(topNode, fileBytes.len());
			fileBytes.extend_from_slice(&encodedTopNode);
		}
		fileBytes.extend_from_slice(&[0u8; 13]);
		fileBytes.extend_from_slice(&[0u8; 176]);
		fileBytes
	}

	//==============================================================================
	// 값 두 개가 허용 오차 안에서 같은지.
	//==============================================================================
	fn assertNear(actual: f32, expected: f32) {
		assert!((actual - expected).abs() < 1.0e-5, "{} expected {}", actual, expected);
	}

	//==============================================================================
	// 노드 레코드 파싱. (머리, 최상위 노드, 압축 배열, 문자열과 정수 프로퍼티, 잘못된 머리)
	//==============================================================================
	#[test]
	fn nodeRecordParsing() {
		let fileBytes = buildTestFbx();
		assert_eq!(FbxLoader::readVersion(&fileBytes), Ok(7400));
		let topNodeList = FbxLoader::new().parseNodeRecords(&fileBytes).expect("records");
		let topNodeNames: Vec<&str> = topNodeList.iter().map(|node| node.name.as_str()).collect();
		assert_eq!(topNodeNames, vec!["Objects", "Connections"]);
		let geometryNode = &topNodeList[0].children[0];
		assert_eq!(geometryNode.getObjectId(), Some(100));
		assert_eq!(geometryNode.getStringProperty(1), "Plane\u{0}\u{1}Geometry");
		let vertices = geometryNode.findChild("Vertices").and_then(|node| node.getProperty(0)).expect("vertices");
		assert_eq!(vertices.getTypeCode(), 'd');
		assert_eq!(vertices.toNumberList(), vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
		assert_eq!(topNodeList[1].children.len(), 10);
		assert!(FbxLoader::new().parse(b"; FBX 7.4.0 project file, ascii").is_err());
		assert!(FbxLoader::new().parse(&fileBytes[..200]).is_err());
	}

	//==============================================================================
	// 그래프 해석. (팬 삼각화, 레이어 매핑, 계층과 소속, 스킨/클러스터, 애니메이션 채널)
	//==============================================================================
	#[test]
	fn sceneInterpretation() {
		let sceneData = FbxLoader::new().parse(&buildTestFbx()).expect("scene");
		let geometry = sceneData.geometryById.get(100).expect("geometry");
		assert_eq!(geometry.cornerCount, 6);
		assert_eq!(geometry.originalVertexCount, 4);
		assert_eq!(geometry.originalVertexIndices, vec![0, 1, 2, 0, 2, 3]);
		assert_eq!(&geometry.positions[3..6], &[1.0, 0.0, 0.0]);
		assert_eq!(&geometry.textureCoordinates[10..12], &[0.0, 1.0]);
		assert_eq!(&geometry.normals[15..18], &[0.0, 0.0, 1.0]);

		assert_eq!(sceneData.nodeList.len(), 2);
		assert_eq!(sceneData.rootNodeIndices, vec![0, 1]);
		assert!(sceneData.nodeList[0].isBone);
		assert_eq!(sceneData.nodeList[0].translation, [1.0, 2.0, 3.0]);
		assert_eq!(sceneData.nodeList[1].geometryId, Some(100));
		assert_eq!(sceneData.nodeList[1].skinId, Some(300));
		let skinDeformer = sceneData.skinDeformerById.get(300).expect("skin");
		assert_eq!(skinDeformer.clusterIds, vec![301]);
		assert_eq!(skinDeformer.geometryId, Some(100));
		assert_eq!(sceneData.clusterById.get(301).and_then(|cluster| cluster.boneNodeId), Some(200));

		assert_eq!(sceneData.animationList.len(), 1);
		let animation = &sceneData.animationList[0];
		assert_eq!(animation.name, "Take\u{0}\u{1}AnimStack");
		assert_eq!(animation.duration, 1.0);
		assert_eq!(animation.channels.len(), 1);
		let channel = &animation.channels[0];
		assert_eq!(channel.nodeIndex, 0);
		assert_eq!(channel.path, AnimationPath::translation);
		assert_eq!(channel.times, vec![0.0, 1.0]);
		assert_eq!(channel.values, vec![0.0, 1.0, 0.0, 0.0, 3.0, 0.0]);
	}

	//==============================================================================
	// FBX → 모델. (중립 메시 서술, 스킨 역바인드 = TransformLink 의 역, 애니메이션 샘플링, 이름 매칭 리타게팅)
	//==============================================================================
	#[test]
	fn fbxToSkinnedModel() {
		let sceneData = FbxLoader::new().parse(&buildTestFbx()).expect("scene");
		let mut skinnedModel = SkinnedModel::createWithoutShaderProgram();
		let (meshDescriptionList, materialDescriptionList) = skinnedModel.interpretFbxContent(&sceneData).expect("interpret");
		assert!(materialDescriptionList.is_empty());
		assert_eq!(meshDescriptionList.len(), 1);
		let meshDescription = &meshDescriptionList[0];
		assert_eq!(meshDescription.positions.len(), 18);
		assert_eq!(meshDescription.weights[0..4], [1.0, 0.0, 0.0, 0.0]);
		assert_eq!(meshDescription.nodeIndex, Some(1));
		assert_eq!(meshDescription.materialIndex, None);

		// 0.5초: 본 이동 (0, 2, 0) → 조인트 행렬 = 이동(0, 2, 0) x 이동(-1, -2, -3) = 이동(-1, 0, -3).
		skinnedModel.update(0.5);
		let jointMatrixArray = &skinnedModel.getSkinList()[0].jointMatrixArray;
		assertNear(jointMatrixArray[12], -1.0);
		assertNear(jointMatrixArray[13], 0.0);
		assertNear(jointMatrixArray[14], -3.0);

		// 같은 장면을 이름으로 붙이면 클립이 하나 더, 이동을 버리면 채널이 없다.
		skinnedModel.addAnimationsFromFbxSceneData(&sceneData, "Moved", false);
		skinnedModel.addAnimationsFromFbxSceneData(&sceneData, "Still", true);
		let animationList = skinnedModel.getAnimationList();
		assert_eq!(animationList.len(), 3);
		assert_eq!(animationList[1].channels.len(), 1);
		assert!(animationList[2].channels.is_empty());
	}

	//==============================================================================
	// 압축 풀기. (zlib 머리 deflate 왕복)
	//==============================================================================
	#[test]
	fn inflateRoundTrip() {
		let originalBytes: Vec<u8> = (0..4096u32).map(|value| (value % 251) as u8).collect();
		let mut zlibEncoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
		zlibEncoder.write_all(&originalBytes).expect("deflate");
		let compressedBytes = zlibEncoder.finish().expect("deflate finish");
		assert_eq!(FbxLoader::inflate(&compressedBytes, originalBytes.len()), Ok(originalBytes));
		assert!(FbxLoader::inflate(&[1, 2, 3], 3).is_err());
	}

	//==============================================================================
	// 실제 FBX. (원본 예제 anims/walk.fbx, Mixamo 7.7 형식, 압축 배열, 본 계층, 걷기 스택, 파일이 없으면 건너뛴다)
	//==============================================================================
	#[test]
	fn walkFbxFromExamples() {
		let walkPath = std::path::Path::new(EXAMPLE_ASSET_DIRECTORY).join("anims").join("walk.fbx");
		let fileBytes = match std::fs::read(&walkPath) {
			Ok(fileBytes) => {
				fileBytes
			}
			Err(_) => {
				eprintln!("walk.fbx 없음, 건너뜀");
				return;
			}
		};
		assert_eq!(FbxLoader::readVersion(&fileBytes), Ok(7700));
		let topNodeList = FbxLoader::new().parseNodeRecords(&fileBytes).expect("records");
		let topNodeNames: Vec<&str> = topNodeList.iter().map(|node| node.name.as_str()).collect();
		assert!(topNodeNames.contains(&"FBXHeaderExtension"));
		assert!(topNodeNames.contains(&"Objects"));
		assert!(topNodeNames.contains(&"Connections"));

		let sceneData = FbxLoader::new().interpret(&topNodeList).expect("scene");
		let hipsIndex = sceneData.nodeList.iter().position(|node| node.name.starts_with("mixamorig:Hips")).expect("hips");
		assert!(sceneData.nodeList[hipsIndex].isBone);
		assert!(!sceneData.animationList.is_empty());
		let walkAnimation = sceneData.animationList.iter().max_by_key(|animation| animation.channels.len()).expect("walk");
		assert!(walkAnimation.channels.len() > 10);
		assert!(walkAnimation.duration > 0.5 && walkAnimation.duration < 10.0);
		assert!(walkAnimation.channels.iter().any(|channel| channel.nodeIndex == hipsIndex && channel.path == AnimationPath::translation));

		// 걷기 장면을 모델로 해석하고, 같은 장면을 이동 없이 이름 매칭으로 붙인다.
		let mut skinnedModel = SkinnedModel::createWithoutShaderProgram();
		skinnedModel.interpretFbxContent(&sceneData).expect("interpret");
		let originalAnimationCount = skinnedModel.getAnimationList().len();
		skinnedModel.addAnimationsFromFbxSceneData(&sceneData, "WalkInPlace", true);
		let animationList = skinnedModel.getAnimationList();
		assert!(animationList.len() > originalAnimationCount);
		let addedClip = &animationList[animationList.len() - 1];
		assert_eq!(addedClip.name, "WalkInPlace");
		assert!(addedClip.channels.iter().all(|channel| channel.path != AnimationPath::translation));
		skinnedModel.playAnimation("WalkInPlace", 0.0);
		skinnedModel.update(0.4);
		let hipsWorldMatrix = skinnedModel.getNodeList()[hipsIndex].worldMatrix;
		assert!(hipsWorldMatrix.elements.iter().all(|value| value.is_finite()));
	}
}
