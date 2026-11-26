//==============================================================================
//! FBX 로더. (원본 graphics3d/fbxloader.js)
//! - 바이너리 FBX → GL 무관 중간 표현(FbxSceneData). 저수준 노드/프로퍼티 파싱 후 오브젝트/커넥션 그래프를 해석해
//!   노드 계층 / 스키닝 메시 / 머티리얼 / 임베디드 텍스처 / 골격 애니메이션을 뽑는다. 결과는 SkinnedModel 이 GL 리소스로 소비한다.
//! - 원본처럼 순수 파서다. GL 이나 셰이딩을 모르고, wasm32 에서도 돈다. 압축 배열(deflate, zlib 머리)은 flate2(순수 러스트)로 푼다.
//! - 원본과 다른 것:
//!   - 원본은 async 다. (압축 배열을 브라우저 DecompressionStream 으로 풀어서다) 러스트 판은 flate2 로 그 자리에서 끝난다.
//!     원본 loadFromUrl(url) 의 Promise 는 ImageAsset 처럼 받기 손잡이(FbxLoadHandle)다. load_from_url(엔진, 애셋 경로)이 엔진의 애셋 받기
//!     (데스크탑은 애셋 폴더 후보에서 읽고, 웹은 asset_base_url 에서 fetch)로 바이트를 부탁하고, 다 받으면 get_scene_data 가 파싱한 결과를 돌려준다.
//!     손잡이는 그대로 await 할 수 있다. (엔진 작업 안에서 FbxLoader::load_from_url(engine, 경로).await 가 원본 await FbxLoader.loadFromUrl(url) 처럼 파싱 결과를 낸다)
//!   - 64비트 정수: 원본은 오브젝트 id 처럼 하나로 든 값이 2^53 을 넘으면 BigInt 로 든다. 러스트 판은 i64 다. 배열(키 시간)은 원본이 Number 다.
//!   - 원본은 순서 있는 Map 을 쓴다. 러스트 판은 넣은 순서를 지키는 FbxObjectMap 으로 같은 순회 순서를 낸다.
//!   - 원본에서 만들고 쓰지 않는 objectById 와 getDataView / getFileBytes / getTextDecoder 는 옮기지 않았다.
//!
//! # 예제
//! 노드 레코드로 짠 장면을 중간 표현으로 해석
//! ```rust
//! use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
//!
//! fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
//! 	FbxNodeRecord {
//! 		name: String::from(name),
//! 		properties,
//! 		children,
//! 	}
//! }
//!
//! let hips = record("Model", vec![FbxPropertyValue::Int64(200), FbxPropertyValue::String(String::from("Hips")), FbxPropertyValue::String(String::from("LimbNode"))], Vec::new());
//! let root_connection = record("C", vec![FbxPropertyValue::String(String::from("OO")), FbxPropertyValue::Int64(200), FbxPropertyValue::Int64(0)], Vec::new());
//! let objects = record("Objects", Vec::new(), vec![hips]);
//! let connections = record("Connections", Vec::new(), vec![root_connection]);
//! let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
//! assert_eq!(scene_data.node_list.len(), 1);
//! assert!(scene_data.node_list[0].is_bone);
//! assert_eq!(scene_data.root_node_indices, vec![0]);
//! assert!(FbxLoader::new().parse(b"; FBX 7.4.0 project file").is_err());
//! ```
//==============================================================================
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use crate::graphics3d::skinnedmodel::AnimationPath;
use std::collections::{HashMap, HashSet};
use std::future::{Future, IntoFuture};
use std::io::Read;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// FBX 시간 단위. (1초 = 46186158000 ktime)
pub const FBX_TIME_UNIT: f64 = 46186158000.0;

/// 파일 끝 꼬리 크기. (최상위 노드 파싱을 멈추는 여유, 원본 byteLength - 160)
const FBX_FOOTER_MARGIN: i64 = 160;

//==============================================================================
/// 프로퍼티 값. (원본 { type, value }, 타입 코드 Y C I F D L f d l i b S R)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum FbxPropertyValue {
	/// 16비트 정수. (타입 코드 Y)
	Int16(i16),
	/// 참거짓. (타입 코드 C, 바이트 하나가 0 이 아니면 true)
	Boolean(bool),
	/// 32비트 정수. (타입 코드 I)
	Int32(i32),
	/// 32비트 실수. (타입 코드 F)
	Float32(f32),
	/// 64비트 실수. (타입 코드 D)
	Float64(f64),
	/// 64비트 정수. (타입 코드 L, 오브젝트 id)
	Int64(i64),
	/// 32비트 실수 배열. (타입 코드 f)
	Float32array(Vec<f32>),
	/// 64비트 실수 배열. (타입 코드 d, 정점 위치 같은 것)
	Float64array(Vec<f64>),
	/// 64비트 정수 배열. (타입 코드 l, 키 시간 같은 것)
	Int64array(Vec<i64>),
	/// 32비트 정수 배열. (타입 코드 i, 폴리곤 정점 인덱스 같은 것)
	Int32array(Vec<i32>),
	/// 참거짓 배열. (타입 코드 b, 원소 바이트 그대로)
	BooleanArray(Vec<u8>),
	/// 문자열. (타입 코드 S, UTF-8 로 읽을 수 없는 바이트는 대체 문자로 바꾼다)
	String(String),
	/// 날 바이트. (타입 코드 R, 임베디드 이미지 같은 것)
	Raw(Rc<Vec<u8>>),
}

impl FbxPropertyValue {
	//==============================================================================
	/// 타입 코드 반환. (원본 property.type)
	///
	/// # 돌려줌
	/// 타입 코드 글자. (Y, C, I, F, D, L, f, d, l, i, b, S, R 가운데 하나)
	//==============================================================================
	pub fn get_type_code(&self) -> char {
		match self {
			FbxPropertyValue::Int16(_) => {
				'Y'
			}
			FbxPropertyValue::Boolean(_) => {
				'C'
			}
			FbxPropertyValue::Int32(_) => {
				'I'
			}
			FbxPropertyValue::Float32(_) => {
				'F'
			}
			FbxPropertyValue::Float64(_) => {
				'D'
			}
			FbxPropertyValue::Int64(_) => {
				'L'
			}
			FbxPropertyValue::Float32array(_) => {
				'f'
			}
			FbxPropertyValue::Float64array(_) => {
				'd'
			}
			FbxPropertyValue::Int64array(_) => {
				'l'
			}
			FbxPropertyValue::Int32array(_) => {
				'i'
			}
			FbxPropertyValue::BooleanArray(_) => {
				'b'
			}
			FbxPropertyValue::String(_) => {
				'S'
			}
			FbxPropertyValue::Raw(_) => {
				'R'
			}
		}
	}

	//==============================================================================
	/// 수 하나로 읽기. (스칼라 타입만, 아니면 None)
	///
	/// # 돌려줌
	/// 스칼라 값을 f64 로 바꾼 것. (Boolean 은 1 이나 0) 배열, 문자열, 날 바이트면 None.
	//==============================================================================
	pub fn as_number(&self) -> Option<f64> {
		match self {
			FbxPropertyValue::Int16(value) => {
				Some(*value as f64)
			}
			FbxPropertyValue::Boolean(value) => {
				Some(f64::from(u8::from(*value)))
			}
			FbxPropertyValue::Int32(value) => {
				Some(*value as f64)
			}
			FbxPropertyValue::Float32(value) => {
				Some(*value as f64)
			}
			FbxPropertyValue::Float64(value) => {
				Some(*value)
			}
			FbxPropertyValue::Int64(value) => {
				Some(*value as f64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 정수 하나로 읽기. (오브젝트 id 같은 정수 스칼라, 아니면 None)
	///
	/// # 돌려줌
	/// 정수, 실수 스칼라를 i64 로 바꾼 것. (실수는 0 쪽으로 자른다) Boolean, 배열, 문자열, 날 바이트면 None.
	//==============================================================================
	pub fn as_integer(&self) -> Option<i64> {
		match self {
			FbxPropertyValue::Int16(value) => {
				Some(*value as i64)
			}
			FbxPropertyValue::Int32(value) => {
				Some(*value as i64)
			}
			FbxPropertyValue::Int64(value) => {
				Some(*value)
			}
			FbxPropertyValue::Float32(value) => {
				Some(*value as i64)
			}
			FbxPropertyValue::Float64(value) => {
				Some(*value as i64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 문자열로 읽기. (S 타입만)
	///
	/// # 돌려줌
	/// 문자열 조각. String 이 아니면 None.
	//==============================================================================
	pub fn as_string(&self) -> Option<&str> {
		match self {
			FbxPropertyValue::String(value) => {
				Some(value.as_str())
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 배열 길이 반환. (배열 타입이 아니면 0)
	///
	/// # 돌려줌
	/// 원소 개수. 배열이 아니면 0.
	//==============================================================================
	pub fn get_array_length(&self) -> usize {
		match self {
			FbxPropertyValue::Float32array(values) => {
				values.len()
			}
			FbxPropertyValue::Float64array(values) => {
				values.len()
			}
			FbxPropertyValue::Int64array(values) => {
				values.len()
			}
			FbxPropertyValue::Int32array(values) => {
				values.len()
			}
			FbxPropertyValue::BooleanArray(values) => {
				values.len()
			}
			_ => {
				0
			}
		}
	}

	//==============================================================================
	/// 배열 원소 하나를 수로 읽기. (배열 밖이거나 배열이 아니면 None)
	///
	/// # 인자
	/// - `index`: 원소 자리. (0 부터)
	///
	/// # 돌려줌
	/// 그 원소를 f64 로 바꾼 것. 자리가 배열 밖이거나 배열이 아니면 None.
	//==============================================================================
	pub fn get_number_at(&self, index: usize) -> Option<f64> {
		match self {
			FbxPropertyValue::Float32array(values) => {
				values.get(index).map(|value| *value as f64)
			}
			FbxPropertyValue::Float64array(values) => {
				values.get(index).copied()
			}
			FbxPropertyValue::Int64array(values) => {
				values.get(index).map(|value| *value as f64)
			}
			FbxPropertyValue::Int32array(values) => {
				values.get(index).map(|value| *value as f64)
			}
			FbxPropertyValue::BooleanArray(values) => {
				values.get(index).map(|value| *value as f64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 배열 원소 하나를 정수로 읽기. (배열 밖이거나 배열이 아니면 None)
	///
	/// # 인자
	/// - `index`: 원소 자리. (0 부터)
	///
	/// # 돌려줌
	/// 그 원소를 i64 로 바꾼 것. (실수는 0 쪽으로 자른다) 자리가 배열 밖이거나 배열이 아니면 None.
	//==============================================================================
	pub fn get_integer_at(&self, index: usize) -> Option<i64> {
		match self {
			FbxPropertyValue::Int64array(values) => {
				values.get(index).copied()
			}
			FbxPropertyValue::Int32array(values) => {
				values.get(index).map(|value| *value as i64)
			}
			FbxPropertyValue::BooleanArray(values) => {
				values.get(index).map(|value| *value as i64)
			}
			FbxPropertyValue::Float32array(values) => {
				values.get(index).map(|value| *value as i64)
			}
			FbxPropertyValue::Float64array(values) => {
				values.get(index).map(|value| *value as i64)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 수 목록으로 바꾸기. (숫자 배열 타입 무엇이든 f64 로, 아니면 빈 목록)
	///
	/// # 돌려줌
	/// 모든 원소를 f64 로 담은 새 목록. 배열이 아니면 빈 목록.
	//==============================================================================
	pub fn to_number_list(&self) -> Vec<f64> {
		let array_length = self.get_array_length();
		let mut number_list = Vec::with_capacity(array_length);
		for element_index in 0..array_length {
			number_list.push(self.get_number_at(element_index).unwrap_or(0.0));
		}
		number_list
	}

	//==============================================================================
	/// 정수 목록으로 바꾸기. (숫자 배열 타입 무엇이든 i64 로, 아니면 빈 목록)
	///
	/// # 돌려줌
	/// 모든 원소를 i64 로 담은 새 목록. (실수는 0 쪽으로 자른다) 배열이 아니면 빈 목록.
	//==============================================================================
	pub fn to_integer_list(&self) -> Vec<i64> {
		let array_length = self.get_array_length();
		let mut integer_list = Vec::with_capacity(array_length);
		for element_index in 0..array_length {
			integer_list.push(self.get_integer_at(element_index).unwrap_or(0));
		}
		integer_list
	}
}

//==============================================================================
/// 노드 레코드. (원본 { name, properties, children })
///
/// # 예제
/// 자식과 프로퍼티 읽기
/// ```rust
/// use vanilla::graphics3d::fbxloader::{FbxNodeRecord, FbxPropertyValue};
///
/// let vertices = FbxNodeRecord {
/// 	name: String::from("Vertices"),
/// 	properties: vec![FbxPropertyValue::Float64array(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0])],
/// 	children: Vec::new(),
/// };
/// let geometry = FbxNodeRecord {
/// 	name: String::from("Geometry"),
/// 	properties: vec![FbxPropertyValue::Int64(100), FbxPropertyValue::String(String::from("Plane")), FbxPropertyValue::String(String::from("Mesh"))],
/// 	children: vec![vertices],
/// };
/// assert_eq!(geometry.get_object_id(), Some(100));
/// assert_eq!(geometry.get_string_property(2), "Mesh");
/// assert_eq!(geometry.get_string_property(0), "");
/// let positions = geometry.find_child("Vertices").and_then(|child| child.get_property(0)).unwrap();
/// assert_eq!(positions.get_array_length(), 6);
/// assert_eq!(positions.get_number_at(3), Some(1.0));
/// assert!(geometry.find_child("Normals").is_none());
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct FbxNodeRecord {
	/// 노드 이름. (Objects, Model, Vertices 같은 것)
	pub name: String,
	/// 프로퍼티 값 목록. (파일에 적힌 차례)
	pub properties: Vec<FbxPropertyValue>,
	/// 자식 노드 레코드 목록.
	pub children: Vec<FbxNodeRecord>,
}

impl FbxNodeRecord {
	//==============================================================================
	/// 이름이 같은 첫 자식 반환. (원본 children.find)
	///
	/// # 인자
	/// - `child_name`: 찾을 자식 이름. (대소문자까지 같아야 한다)
	///
	/// # 돌려줌
	/// 이름이 같은 첫 자식. 없으면 None.
	//==============================================================================
	pub fn find_child(&self, child_name: &str) -> Option<&FbxNodeRecord> {
		self.children.iter().find(|child| child.name == child_name)
	}

	//==============================================================================
	/// 프로퍼티 반환. (없으면 None)
	///
	/// # 인자
	/// - `property_index`: 프로퍼티 자리. (0 부터)
	///
	/// # 돌려줌
	/// 그 자리의 프로퍼티. 자리가 밖이면 None.
	//==============================================================================
	pub fn get_property(&self, property_index: usize) -> Option<&FbxPropertyValue> {
		self.properties.get(property_index)
	}

	//==============================================================================
	/// 첫 프로퍼티를 정수로 반환. (오브젝트 id)
	///
	/// # 돌려줌
	/// 첫 프로퍼티를 정수로 읽은 값. (as_integer) 프로퍼티가 없거나 정수로 읽을 수 없으면 None.
	//==============================================================================
	pub fn get_object_id(&self) -> Option<i64> {
		self.properties.first().and_then(|property| property.as_integer())
	}

	//==============================================================================
	/// 프로퍼티를 문자열로 반환. (없거나 문자열이 아니면 빈 문자열)
	///
	/// # 인자
	/// - `property_index`: 프로퍼티 자리. (0 부터)
	///
	/// # 돌려줌
	/// 그 자리의 문자열 복사본. 자리가 밖이거나 문자열이 아니면 빈 문자열.
	//==============================================================================
	pub fn get_string_property(&self, property_index: usize) -> String {
		self.properties.get(property_index).and_then(|property| property.as_string()).unwrap_or("").to_string()
	}
}

//==============================================================================
/// 넣은 순서를 지키는 id 맵. (원본 `Map<number, object>` 자리, 순회 순서가 결과 순서를 정한다)
///
/// # 예제
/// 넣은 차례 그대로 순회 (같은 id 를 다시 넣으면 자리는 그대로, 값만 바뀐다)
/// ```rust
/// use vanilla::graphics3d::fbxloader::FbxObjectMap;
///
/// let mut object_map = FbxObjectMap::new();
/// object_map.insert(30, "skin");
/// object_map.insert(10, "cluster");
/// object_map.insert(30, "replaced");
/// assert_eq!(object_map.keys(), &[30, 10]);
/// assert_eq!(object_map.values(), &["replaced", "cluster"]);
/// assert_eq!(object_map.get(30), Some(&"replaced"));
/// assert_eq!(object_map.index_of(10), Some(1));
/// assert!(!object_map.contains(20));
/// assert_eq!(object_map.len(), 2);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxObjectMap<T> {
	id_list: Vec<i64>,
	value_list: Vec<T>,
	index_by_id: HashMap<i64, usize>,
}

impl<T> Default for FbxObjectMap<T> {
	//==============================================================================
	/// 빈 맵.
	//==============================================================================
	fn default() -> FbxObjectMap<T> {
		FbxObjectMap::new()
	}
}

impl<T> FbxObjectMap<T> {
	//==============================================================================
	/// 생성. (비어 있음)
	///
	/// # 돌려줌
	/// 빈 맵.
	//==============================================================================
	pub fn new() -> FbxObjectMap<T> {
		FbxObjectMap {
			id_list: Vec::new(),
			value_list: Vec::new(),
			index_by_id: HashMap::new(),
		}
	}

	//==============================================================================
	/// 넣기. (원본 Map.set, 이미 있는 id 면 자리를 지키고 값만 바꾼다)
	///
	/// # 인자
	/// - `id`: 오브젝트 id.
	/// - `value`: 넣을 값. (이미 있는 id 면 그 자리의 값을 바꾼다)
	//==============================================================================
	pub fn insert(&mut self, id: i64, value: T) {
		match self.index_by_id.get(&id) {
			Some(existing_index) => {
				self.value_list[*existing_index] = value;
			}
			None => {
				self.index_by_id.insert(id, self.id_list.len());
				self.id_list.push(id);
				self.value_list.push(value);
			}
		}
	}

	//==============================================================================
	/// 값 반환. (원본 Map.get)
	///
	/// # 인자
	/// - `id`: 찾을 오브젝트 id.
	///
	/// # 돌려줌
	/// 그 id 의 값. 없으면 None.
	//==============================================================================
	pub fn get(&self, id: i64) -> Option<&T> {
		self.index_by_id.get(&id).map(|index| &self.value_list[*index])
	}

	//==============================================================================
	/// 값 반환. (변경용)
	///
	/// # 인자
	/// - `id`: 찾을 오브젝트 id.
	///
	/// # 돌려줌
	/// 그 id 의 바꿀 수 있는 값. 없으면 None.
	//==============================================================================
	pub fn get_mut(&mut self, id: i64) -> Option<&mut T> {
		match self.index_by_id.get(&id) {
			Some(index) => {
				Some(&mut self.value_list[*index])
			}
			None => {
				None
			}
		}
	}

	//==============================================================================
	/// 있는지. (원본 Map.has)
	///
	/// # 인자
	/// - `id`: 찾을 오브젝트 id.
	///
	/// # 돌려줌
	/// 있으면 true.
	//==============================================================================
	pub fn contains(&self, id: i64) -> bool {
		self.index_by_id.contains_key(&id)
	}

	//==============================================================================
	/// 넣은 순서의 자리 반환. (원본 Array.from(map.keys()).index_of)
	///
	/// # 인자
	/// - `id`: 찾을 오브젝트 id.
	///
	/// # 돌려줌
	/// 처음 넣은 차례의 자리. (0 부터, keys 와 values 의 자리) 없으면 None.
	//==============================================================================
	pub fn index_of(&self, id: i64) -> Option<usize> {
		self.index_by_id.get(&id).copied()
	}

	//==============================================================================
	/// id 목록 반환. (넣은 순서)
	///
	/// # 돌려줌
	/// id 목록. (넣은 차례)
	//==============================================================================
	pub fn keys(&self) -> &[i64] {
		&self.id_list
	}

	//==============================================================================
	/// 값 목록 반환. (넣은 순서)
	///
	/// # 돌려줌
	/// 값 목록. (넣은 차례, keys 와 같은 자리)
	//==============================================================================
	pub fn values(&self) -> &[T] {
		&self.value_list
	}

	//==============================================================================
	/// 값 목록 반환. (변경용)
	///
	/// # 돌려줌
	/// 바꿀 수 있는 값 목록. (넣은 차례)
	//==============================================================================
	pub fn values_mut(&mut self) -> &mut [T] {
		&mut self.value_list
	}

	//==============================================================================
	/// (id, 값) 순회. (넣은 순서)
	///
	/// # 돌려줌
	/// (id, 값 참조) 반복자. (넣은 차례)
	//==============================================================================
	pub fn iter(&self) -> impl Iterator<Item = (i64, &T)> {
		self.id_list.iter().copied().zip(self.value_list.iter())
	}

	//==============================================================================
	/// 개수 반환.
	///
	/// # 돌려줌
	/// 넣은 id 개수.
	//==============================================================================
	pub fn len(&self) -> usize {
		self.id_list.len()
	}

	//==============================================================================
	/// 비었는지.
	///
	/// # 돌려줌
	/// 하나도 없으면 true.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.id_list.is_empty()
	}
}

//==============================================================================
/// 지오메트리. (삼각화 + 노멀/UV 레이어를 코너 단위로 펼친 것 + 원본 정점 매핑)
///
/// # 예제
/// 사각형 폴리곤 하나가 삼각형 두 개(코너 6개)로 펼쳐진다
/// ```rust
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// let geometry = record("Geometry", vec![FbxPropertyValue::Int64(100)], vec![
/// 	record("Vertices", vec![FbxPropertyValue::Float64array(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0])], Vec::new()),
/// 	record("PolygonVertexIndex", vec![FbxPropertyValue::Int32array(vec![0, 1, 2, -4])], Vec::new()),
/// ]);
/// let objects = record("Objects", Vec::new(), vec![geometry]);
/// let connections = record("Connections", Vec::new(), Vec::new());
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// let geometry = scene_data.geometry_by_id.get(100).unwrap();
/// assert_eq!(geometry.corner_count, 6);
/// assert_eq!(geometry.original_vertex_count, 4);
/// assert_eq!(geometry.original_vertex_indices, vec![0, 1, 2, 0, 2, 3]);
/// assert_eq!(&geometry.positions[3..6], &[1.0, 0.0, 0.0]);
/// assert_eq!(geometry.normals, vec![0.0; 18]);
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxGeometry {
	/// 코너별 위치. (코너마다 x, y, z, 삼각형 코너 차례)
	pub positions: Vec<f32>,
	/// 코너별 노멀. (코너마다 3개, 노멀 레이어가 없으면 0)
	pub normals: Vec<f32>,
	/// 코너별 UV. (코너마다 2개, UV 레이어가 없으면 0)
	pub texture_coordinates: Vec<f32>,
	/// 코너마다 원본 정점 인덱스. (스킨 가중치를 코너로 펼칠 때 쓴다)
	pub original_vertex_indices: Vec<i32>,
	/// 삼각형마다 머티리얼 인덱스. (노드 material_ids 안의 자리, 머티리얼 레이어가 없으면 0)
	pub material_index_per_triangle: Vec<i32>,
	/// 코너 개수. (삼각형 수 x 3)
	pub corner_count: usize,
	/// 원본 정점 개수. (Vertices 배열 길이 / 3)
	pub original_vertex_count: usize,
}

//==============================================================================
/// 스킨 디포머. (클러스터 목록 + 붙은 지오메트리)
///
/// # 예제
/// 스킨이 클러스터와 지오메트리에 이어진다
/// ```rust
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// fn connect(source_id: i64, destination_id: i64) -> FbxNodeRecord {
/// 	record("C", vec![FbxPropertyValue::String(String::from("OO")), FbxPropertyValue::Int64(source_id), FbxPropertyValue::Int64(destination_id)], Vec::new())
/// }
///
/// let geometry = record("Geometry", vec![FbxPropertyValue::Int64(100)], vec![
/// 	record("Vertices", vec![FbxPropertyValue::Float64array(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0])], Vec::new()),
/// 	record("PolygonVertexIndex", vec![FbxPropertyValue::Int32array(vec![0, 1, -3])], Vec::new()),
/// ]);
/// let skin = record("Deformer", vec![FbxPropertyValue::Int64(300), FbxPropertyValue::String(String::from("Skin")), FbxPropertyValue::String(String::from("Skin"))], Vec::new());
/// let cluster = record("Deformer", vec![FbxPropertyValue::Int64(301), FbxPropertyValue::String(String::from("Cluster")), FbxPropertyValue::String(String::from("Cluster"))], Vec::new());
/// let objects = record("Objects", Vec::new(), vec![geometry, skin, cluster]);
/// let connections = record("Connections", Vec::new(), vec![connect(301, 300), connect(300, 100)]);
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// let skin_deformer = scene_data.skin_deformer_by_id.get(300).unwrap();
/// assert_eq!(skin_deformer.cluster_ids, vec![301]);
/// assert_eq!(skin_deformer.geometry_id, Some(100));
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxSkinDeformer {
	/// 이 스킨의 클러스터 id 목록. (커넥션 차례, 클러스터 하나가 본 하나)
	pub cluster_ids: Vec<i64>,
	/// 스킨이 붙은 지오메트리 id. (커넥션이 없으면 None)
	pub geometry_id: Option<i64>,
}

//==============================================================================
/// 클러스터. (본별 정점 가중치 + 바인드 역행렬용 링크 변환, 행렬은 열 우선 16개)
///
/// # 예제
/// 클러스터의 정점 가중치와 움직이는 본
/// ```rust
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// let cluster = record("Deformer", vec![FbxPropertyValue::Int64(301), FbxPropertyValue::String(String::from("Cluster")), FbxPropertyValue::String(String::from("Cluster"))], vec![
/// 	record("Indexes", vec![FbxPropertyValue::Int32array(vec![0, 2])], Vec::new()),
/// 	record("Weights", vec![FbxPropertyValue::Float64array(vec![1.0, 0.5])], Vec::new()),
/// ]);
/// let bone_connection = record("C", vec![FbxPropertyValue::String(String::from("OO")), FbxPropertyValue::Int64(200), FbxPropertyValue::Int64(301)], Vec::new());
/// let objects = record("Objects", Vec::new(), vec![cluster]);
/// let connections = record("Connections", Vec::new(), vec![bone_connection]);
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// let cluster = scene_data.cluster_by_id.get(301).unwrap();
/// assert_eq!(cluster.vertex_indices, vec![0, 2]);
/// assert_eq!(cluster.weights, vec![1.0, 0.5]);
/// assert_eq!(cluster.bone_node_id, Some(200));
/// assert!(cluster.transform_link.is_none());
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxCluster {
	/// 영향을 받는 원본 정점 인덱스 목록. (Indexes)
	pub vertex_indices: Vec<i64>,
	/// 정점마다 가중치. (Weights, vertex_indices 와 같은 자리)
	pub weights: Vec<f64>,
	/// 클러스터 Transform 행렬. (열 우선 16개, 그대로 역바인드 행렬로 쓴다. 없으면 None)
	pub transform: Option<Vec<f64>>,
	/// 본의 바인드 전역 행렬 TransformLink. (열 우선 16개, transform 이 없을 때 이 역행렬을 역바인드로 쓴다. 없으면 None)
	pub transform_link: Option<Vec<f64>>,
	/// 이 클러스터를 움직이는 본 노드 id. (커넥션이 없으면 None)
	pub bone_node_id: Option<i64>,
}

//==============================================================================
/// 비디오. (임베디드 이미지 바이트)
///
/// # 예제
/// 같은 파일을 두 번 참조해 비어 있는 Content 는 먼저 나온 바이트를 나눠 든다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// fn video(id: i64, file_name: &str, content_bytes: Vec<u8>) -> FbxNodeRecord {
/// 	record("Video", vec![FbxPropertyValue::Int64(id)], vec![
/// 		record("RelativeFilename", vec![FbxPropertyValue::String(String::from(file_name))], Vec::new()),
/// 		record("Content", vec![FbxPropertyValue::Raw(Rc::new(content_bytes))], Vec::new()),
/// 	])
/// }
///
/// let objects = record("Objects", Vec::new(), vec![video(500, "textures\\skin.png", vec![1, 2, 3]), video(501, "other/skin.png", Vec::new())]);
/// let connections = record("Connections", Vec::new(), Vec::new());
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// let shared_video = scene_data.video_by_id.get(501).unwrap();
/// assert_eq!(shared_video.filename, "other/skin.png");
/// assert_eq!(shared_video.content_bytes.as_deref(), Some(&vec![1, 2, 3]));
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxVideo {
	/// 임베디드 이미지 바이트. (Content 의 R 값, 비어 있으면 파일 이름이 같은 다른 비디오의 바이트를 나눠 든다. Content 가 없으면 None)
	pub content_bytes: Option<Rc<Vec<u8>>>,
	/// 파일 경로. (RelativeFilename 이나 Filename, 없으면 빈 문자열)
	pub filename: String,
}

//==============================================================================
/// 텍스처. (비디오 연결)
///
/// # 예제
/// 텍스처로 이어진 비디오 찾기
/// ```rust
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// let texture = record("Texture", vec![FbxPropertyValue::Int64(501)], vec![
/// 	record("RelativeFilename", vec![FbxPropertyValue::String(String::from("skin.png"))], Vec::new()),
/// ]);
/// let video_connection = record("C", vec![FbxPropertyValue::String(String::from("OO")), FbxPropertyValue::Int64(500), FbxPropertyValue::Int64(501)], Vec::new());
/// let objects = record("Objects", Vec::new(), vec![texture]);
/// let connections = record("Connections", Vec::new(), vec![video_connection]);
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// let texture = scene_data.texture_by_id.get(501).unwrap();
/// assert_eq!(texture.filename, "skin.png");
/// assert_eq!(texture.video_id, Some(500));
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxTexture {
	/// 이 텍스처로 이어진 비디오 id. (커넥션이 없으면 None)
	pub video_id: Option<i64>,
	/// 파일 경로. (RelativeFilename 이나 FileName, 없으면 빈 문자열)
	pub filename: String,
}

//==============================================================================
/// 머티리얼. (phong 색상/광택 + 슬롯별 텍스처 id)
///
/// # 예제
/// Properties70 의 색과 OP 커넥션의 텍스처 슬롯
/// ```rust
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// fn text(value: &str) -> FbxPropertyValue {
/// 	FbxPropertyValue::String(String::from(value))
/// }
///
/// let diffuse_record = record("P", vec![text("DiffuseColor"), text("Color"), text(""), text("A"), FbxPropertyValue::Float64(0.8), FbxPropertyValue::Float64(0.2), FbxPropertyValue::Float64(0.1)], Vec::new());
/// let material = record("Material", vec![FbxPropertyValue::Int64(600)], vec![record("Properties70", Vec::new(), vec![diffuse_record])]);
/// let texture = record("Texture", vec![FbxPropertyValue::Int64(501)], Vec::new());
/// let slot_connection = record("C", vec![text("OP"), FbxPropertyValue::Int64(501), FbxPropertyValue::Int64(600), text("DiffuseColor")], Vec::new());
/// let objects = record("Objects", Vec::new(), vec![material, texture]);
/// let connections = record("Connections", Vec::new(), vec![slot_connection]);
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// let material = scene_data.material_by_id.get(600).unwrap();
/// assert_eq!(material.diffuse_color, [0.8, 0.2, 0.1]);
/// assert_eq!(material.shininess, 20.0);
/// assert_eq!(material.diffuse_texture_id, Some(501));
/// assert_eq!(material.normal_texture_id, None);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxMaterial {
	/// 난반사 색. (RGB, DiffuseColor 나 Diffuse, 없으면 1, 1, 1)
	pub diffuse_color: [f64; 3],
	/// 정반사 색. (RGB, SpecularColor 나 Specular, 없으면 0.5, 0.5, 0.5)
	pub specular_color: [f64; 3],
	/// 광택 지수. (Shininess 나 ShininessExponent, 없으면 20)
	pub shininess: f64,
	/// 난반사 텍스처 id. (OP 커넥션 DiffuseColor, 없으면 None)
	pub diffuse_texture_id: Option<i64>,
	/// 노멀 맵 텍스처 id. (OP 커넥션 NormalMap 이나 Bump, 없으면 None)
	pub normal_texture_id: Option<i64>,
	/// 정반사 텍스처 id. (OP 커넥션 SpecularColor 나 SpecularFactor, 없으면 None)
	pub specular_texture_id: Option<i64>,
	/// 광택 텍스처 id. (OP 커넥션 Shininess 나 ShininessExponent, 없으면 None)
	pub gloss_texture_id: Option<i64>,
	/// 불투명도 텍스처 id. (OP 커넥션 TransparentColor 나 TransparencyFactor, 없으면 None)
	pub opacity_texture_id: Option<i64>,
}

//==============================================================================
/// 모델 노드. (계층 + 로컬 변환(오일러 도) + 지오메트리/스킨/머티리얼 소속)
/// - name 은 원본처럼 FBX 이름 그대로다. ("Hips\u{0}\u{1}Model" 꼴, 본 이름 매칭도 이 문자열로 한다)
///
/// # 예제
/// 루트 본 아래 자식 본 하나와 로컬 이동
/// ```rust
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// fn text(value: &str) -> FbxPropertyValue {
/// 	FbxPropertyValue::String(String::from(value))
/// }
///
/// fn connect(source_id: i64, destination_id: i64) -> FbxNodeRecord {
/// 	record("C", vec![text("OO"), FbxPropertyValue::Int64(source_id), FbxPropertyValue::Int64(destination_id)], Vec::new())
/// }
///
/// let translation_record = record("P", vec![text("Lcl Translation"), text("Lcl Translation"), text(""), text("A"), FbxPropertyValue::Float64(0.0), FbxPropertyValue::Float64(10.0), FbxPropertyValue::Float64(0.0)], Vec::new());
/// let hips = record("Model", vec![FbxPropertyValue::Int64(200), text("Hips"), text("LimbNode")], Vec::new());
/// let spine = record("Model", vec![FbxPropertyValue::Int64(201), text("Spine"), text("LimbNode")], vec![record("Properties70", Vec::new(), vec![translation_record])]);
/// let objects = record("Objects", Vec::new(), vec![hips, spine]);
/// let connections = record("Connections", Vec::new(), vec![connect(200, 0), connect(201, 200)]);
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// assert_eq!(scene_data.root_node_indices, vec![0]);
/// assert_eq!(scene_data.node_list[0].child_indices, vec![1]);
/// let spine = &scene_data.node_list[1];
/// assert_eq!(spine.parent_id, Some(200));
/// assert_eq!(spine.translation, [0.0, 10.0, 0.0]);
/// assert_eq!(spine.scale, [1.0, 1.0, 1.0]);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxNode {
	/// 오브젝트 id.
	pub id: i64,
	/// FBX 이름. (Model 의 두 번째 프로퍼티 그대로)
	pub name: String,
	/// 본인지. (Model 의 세 번째 프로퍼티가 LimbNode 면 true)
	pub is_bone: bool,
	/// 로컬 이동. (Lcl Translation, 없으면 0, 0, 0)
	pub translation: [f64; 3],
	/// 로컬 회전. (Lcl Rotation, X, Y, Z 오일러 각 도, 없으면 0, 0, 0)
	pub rotation_euler_degrees: [f64; 3],
	/// 사전 회전. (PreRotation, 오일러 각 도, SkinnedModel 이 로컬 회전 앞에 곱한다. 없으면 0, 0, 0)
	pub pre_rotation_degrees: [f64; 3],
	/// 로컬 크기. (Lcl Scaling, 없으면 1, 1, 1)
	pub scale: [f64; 3],
	/// 이 노드가 가진 지오메트리 id. (없으면 None)
	pub geometry_id: Option<i64>,
	/// 이 노드의 지오메트리에 붙은 스킨 id. (없으면 None)
	pub skin_id: Option<i64>,
	/// 이 노드에 이어진 머티리얼 id 목록. (커넥션 차례, 지오메트리의 머티리얼 인덱스가 이 목록의 자리다)
	pub material_ids: Vec<i64>,
	/// 자식 노드 인덱스 목록. (node_list 안의 자리)
	pub child_indices: Vec<usize>,
	/// 부모 노드 id. (루트면 None)
	pub parent_id: Option<i64>,
}

//==============================================================================
/// 애니메이션 채널. (노드 인덱스 + 경로 + 키 시간(초) + 값, 회전은 오일러 도 3성분)
///
/// # 예제
/// 채널의 키를 하나씩 읽기 (키마다 X, Y, Z 3개)
/// ```rust
/// use vanilla::AnimationPath;
/// use vanilla::graphics3d::fbxloader::FbxAnimationChannel;
///
/// let channel = FbxAnimationChannel {
/// 	node_index: 0,
/// 	path: AnimationPath::Translation,
/// 	times: vec![0.0, 1.0],
/// 	values: vec![0.0, 1.0, 0.0, 0.0, 3.0, 0.0],
/// };
/// for (key_index, key_time) in channel.times.iter().enumerate() {
/// 	let key_value = &channel.values[key_index * 3..key_index * 3 + 3];
/// 	assert_eq!(key_value[1], 1.0 + 2.0 * key_time);
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxAnimationChannel {
	/// 대상 노드 인덱스. (node_list 안의 자리)
	pub node_index: usize,
	/// 채널 경로. (Translation, Rotation, Scale 가운데 하나)
	pub path: AnimationPath,
	/// 키 시간 목록. (초, 오름차순, X, Y, Z 커브 키 시간의 합집합)
	pub times: Vec<f32>,
	/// 키마다 X, Y, Z 값 3개. (회전은 오일러 각 도, 커브가 없는 축은 0, 그 축의 키가 없는 시간은 선형 보간한 값)
	pub values: Vec<f32>,
}

//==============================================================================
/// 애니메이션. (스택 하나)
///
/// # 예제
/// 스택, 레이어, 커브노드, 커브를 잇고 Y 이동 채널 하나를 얻는다
/// ```rust
/// use vanilla::AnimationPath;
/// use vanilla::graphics3d::fbxloader::{FbxLoader, FbxNodeRecord, FbxPropertyValue, FBX_TIME_UNIT};
///
/// fn record(name: &str, properties: Vec<FbxPropertyValue>, children: Vec<FbxNodeRecord>) -> FbxNodeRecord {
/// 	FbxNodeRecord {
/// 		name: String::from(name),
/// 		properties,
/// 		children,
/// 	}
/// }
///
/// fn text(value: &str) -> FbxPropertyValue {
/// 	FbxPropertyValue::String(String::from(value))
/// }
///
/// fn connect(connection_type: &str, source_id: i64, destination_id: i64, property_name: &str) -> FbxNodeRecord {
/// 	record("C", vec![text(connection_type), FbxPropertyValue::Int64(source_id), FbxPropertyValue::Int64(destination_id), text(property_name)], Vec::new())
/// }
///
/// let curve = record("AnimationCurve", vec![FbxPropertyValue::Int64(403)], vec![
/// 	record("KeyTime", vec![FbxPropertyValue::Int64array(vec![0, FBX_TIME_UNIT as i64])], Vec::new()),
/// 	record("KeyValueFloat", vec![FbxPropertyValue::Float32array(vec![1.0, 3.0])], Vec::new()),
/// ]);
/// let objects = record("Objects", Vec::new(), vec![
/// 	record("Model", vec![FbxPropertyValue::Int64(200), text("Hips"), text("LimbNode")], Vec::new()),
/// 	record("AnimationStack", vec![FbxPropertyValue::Int64(400), text("Walk")], Vec::new()),
/// 	record("AnimationLayer", vec![FbxPropertyValue::Int64(401)], Vec::new()),
/// 	record("AnimationCurveNode", vec![FbxPropertyValue::Int64(402), text("T")], Vec::new()),
/// 	curve,
/// ]);
/// let connections = record("Connections", Vec::new(), vec![
/// 	connect("OO", 401, 400, ""),
/// 	connect("OO", 402, 401, ""),
/// 	connect("OP", 402, 200, "Lcl Translation"),
/// 	connect("OP", 403, 402, "d|Y"),
/// ]);
/// let scene_data = FbxLoader::new().interpret(&[objects, connections]).unwrap();
/// let animation = &scene_data.animation_list[0];
/// assert_eq!(animation.name, "Walk");
/// assert_eq!(animation.duration, 1.0);
/// let channel = &animation.channels[0];
/// assert_eq!(channel.path, AnimationPath::Translation);
/// assert_eq!(channel.times, vec![0.0, 1.0]);
/// assert_eq!(channel.values, vec![0.0, 1.0, 0.0, 0.0, 3.0, 0.0]);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxAnimation {
	/// 스택 이름. (AnimationStack 의 두 번째 프로퍼티 그대로)
	pub name: String,
	/// 길이. (초, 채널마다 마지막 키 시간 가운데 가장 큰 것, 채널이 없으면 0)
	pub duration: f32,
	/// 노드와 경로마다 채널 목록. (스택에 든 레이어의 커브노드만)
	pub channels: Vec<FbxAnimationChannel>,
}

//==============================================================================
/// 중간 표현. (원본 sceneData)
///
/// # 예제
/// 이름 앞부분으로 노드 찾기 (FBX 이름은 뒤에 클래스 이름이 붙어 있을 수 있다)
/// ```rust
/// use vanilla::graphics3d::fbxloader::FbxSceneData;
///
/// fn find_node_index(scene_data: &FbxSceneData, name_prefix: &str) -> Option<usize> {
/// 	scene_data.node_list.iter().position(|node| node.name.starts_with(name_prefix))
/// }
///
/// let scene_data = FbxSceneData::default();
/// assert_eq!(find_node_index(&scene_data, "mixamorig:Hips"), None);
/// assert!(scene_data.root_node_indices.is_empty());
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct FbxSceneData {
	/// 모델 노드 목록. (Objects 의 Model 차례)
	pub node_list: Vec<FbxNode>,
	/// 노드 id 에서 node_list 자리로 가는 표.
	pub node_id_to_index: HashMap<i64, usize>,
	/// 루트 노드 인덱스 목록. (id 0 에 이어진 노드, 커넥션 차례)
	pub root_node_indices: Vec<usize>,
	/// 지오메트리. (id 로 찾는다, Objects 차례)
	pub geometry_by_id: FbxObjectMap<FbxGeometry>,
	/// 스킨 디포머. (id 로 찾는다)
	pub skin_deformer_by_id: FbxObjectMap<FbxSkinDeformer>,
	/// 클러스터. (id 로 찾는다)
	pub cluster_by_id: FbxObjectMap<FbxCluster>,
	/// 머티리얼. (id 로 찾는다, 넣은 차례의 자리가 SkinnedModel 머티리얼 서술의 자리다)
	pub material_by_id: FbxObjectMap<FbxMaterial>,
	/// 텍스처. (id 로 찾는다)
	pub texture_by_id: FbxObjectMap<FbxTexture>,
	/// 비디오. (id 로 찾는다, 임베디드 이미지)
	pub video_by_id: FbxObjectMap<FbxVideo>,
	/// 애니메이션 목록. (AnimationStack 마다 하나, Objects 차례)
	pub animation_list: Vec<FbxAnimation>,
}

//==============================================================================
/// 오브젝트-오브젝트 커넥션. (OO)
//==============================================================================
struct ObjectConnection {
	source_id: i64,
	destination_id: i64,
}

//==============================================================================
/// 오브젝트-프로퍼티 커넥션. (OP)
//==============================================================================
struct PropertyConnection {
	source_id: i64,
	destination_id: i64,
	property_name: String,
}

//==============================================================================
/// 애니메이션 커브. (키 시간 ktime + 값)
//==============================================================================
struct AnimationCurve {
	key_times: Vec<i64>,
	key_values: Vec<f64>,
}

//==============================================================================
/// 애니메이션 커브노드. (대상 노드 / 프로퍼티 + 채널 X/Y/Z 별 커브 id)
//==============================================================================
struct AnimationCurveNode {
	target_node_id: Option<i64>,
	target_property: Option<String>,
	curve_by_channel: HashMap<String, i64>,
}

//==============================================================================
/// 애니메이션 스택. (이름 + 소속 레이어)
//==============================================================================
struct AnimationStack {
	name: String,
	layer_ids: Vec<i64>,
}

//==============================================================================
/// 레이어 리더. (노멀/UV, 매핑/참조 방식별 조회, 원본 buildLayerReader 가 돌려주는 함수)
//==============================================================================
struct LayerReader<'a> {
	data_array: Option<&'a FbxPropertyValue>,
	mapping_type: String,
	reference_type: String,
	index_array: Option<&'a FbxPropertyValue>,
	component_count: usize,
}

impl LayerReader<'_> {
	//==============================================================================
	/// 코너 하나의 값 읽기. (레이어가 없으면 0)
	//==============================================================================
	fn read(&self, original_vertex_index: usize, polygon_vertex_position: usize, component_values: &mut [f64]) {
		let data_array = match self.data_array {
			Some(data_array) => {
				data_array
			}
			None => {
				component_values.fill(0.0);
				return;
			}
		};
		let mut mapped_index = 0;
		if self.mapping_type == "ByVertice" || self.mapping_type == "ByVertex" {
			mapped_index = original_vertex_index;
		}
		else if self.mapping_type == "ByPolygonVertex" {
			mapped_index = polygon_vertex_position;
		}
		else if self.mapping_type == "AllSame" {
			mapped_index = 0;
		}
		let mut data_index = mapped_index as i64;
		if self.reference_type == "IndexToDirect" {
			if let Some(index_array) = self.index_array {
				data_index = index_array.get_integer_at(mapped_index).unwrap_or(-1);
			}
		}
		for (component_index, component_value) in component_values.iter_mut().enumerate().take(self.component_count) {
			*component_value = if data_index >= 0 {
				data_array.get_number_at(data_index as usize * self.component_count + component_index).unwrap_or(0.0)
			}
			else {
				0.0
			};
		}
	}
}

//==============================================================================
/// FBX 받기 손잡이. (FbxLoader::load_from_url 이 돌려준다. 원본 loadFromUrl 의 Promise 자리)
/// - 씬은 get_load_handle 을 SceneBase::track_load_handle 로 맡겨 다 받을 때까지 initialize 를 미룰 수 있다.
///
/// # 예제
/// 씬이 받기를 맡기고 initialize 에서 결과 읽기
/// ```rust,no_run
/// use vanilla::{Engine, FbxLoadHandle, Scene, SceneBase};
/// use vanilla::graphics3d::fbxloader::FbxLoader;
///
/// struct AnimationScene {
/// 	scene_base: SceneBase,
/// 	fbx_load_handle: Option<FbxLoadHandle>,
/// }
///
/// impl Scene for AnimationScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn create(&mut self, engine: &mut Engine) {
/// 		let fbx_load_handle = FbxLoader::load_from_url(engine, "anims/walk.fbx");
/// 		self.scene_base.track_load_handle(fbx_load_handle.get_load_handle());
/// 		self.fbx_load_handle = Some(fbx_load_handle);
/// 	}
///
/// 	fn initialize(&mut self, _engine: &mut Engine) {
/// 		let scene_data_result = self.fbx_load_handle.as_ref().and_then(|fbx_load_handle| fbx_load_handle.get_scene_data());
/// 		match scene_data_result {
/// 			Some(Ok(scene_data)) => {
/// 				println!("{} animations", scene_data.animation_list.len());
/// 			}
/// 			Some(Err(error_message)) => {
/// 				eprintln!("{}", error_message);
/// 			}
/// 			None => {
/// 			}
/// 		}
/// 	}
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxLoadHandle {
	load_handle: AssetLoadHandle,
}

impl FbxLoadHandle {
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
	/// 파싱한 중간 표현. (받는 중이면 None, 받기나 파싱에 실패하면 Err. 원본 문구 "FbxLoader load failed: url")
	/// - 끝난 뒤에는 부를 때마다 받은 바이트를 다시 파싱한다. 한 번 받아 두고 쓴다.
	///
	/// # 돌려줌
	/// 받는 중이면 None. 끝났으면 파싱한 중간 표현, 받기에 실패하면 Err("FbxLoader load failed: 경로"), 파싱에 실패하면 FbxLoader::parse 의 Err.
	///
	/// # 예제
	/// 없는 파일은 Err 로 끝난다 (데스크탑은 부탁한 자리에서 바로 끝난다)
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration};
	/// use vanilla::graphics3d::fbxloader::FbxLoader;
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let fbx_load_handle = FbxLoader::load_from_url(&engine, "missing_model.fbx");
	/// assert!(fbx_load_handle.is_finished());
	/// let scene_data_result = fbx_load_handle.get_scene_data().unwrap();
	/// assert_eq!(scene_data_result.err(), Some(String::from("FbxLoader load failed: missing_model.fbx")));
	/// ```
	//==============================================================================
	pub fn get_scene_data(&self) -> Option<Result<FbxSceneData, String>> {
		if !self.load_handle.is_finished() {
			return None;
		}
		let file_bytes = match self.load_handle.get_bytes() {
			Some(file_bytes) => {
				file_bytes
			}
			None => {
				return Some(Err(format!("FbxLoader load failed: {}", self.load_handle.get_asset_path())));
			}
		};
		Some(FbxLoader::new().parse(&file_bytes))
	}
}

//==============================================================================
/// FBX 받기를 기다리는 퓨처. (FbxLoadHandle 을 await 하면 된다. 다 받으면 파싱한 결과, 원본 loadFromUrl 의 Promise 가 풀린 값)
///
/// # 예제
/// 엔진 작업 안에서 손잡이를 퓨처로 바꿔 await
/// ```rust,no_run
/// use std::future::IntoFuture;
/// use vanilla::{Engine, FbxLoadFuture};
/// use vanilla::graphics3d::fbxloader::FbxLoader;
///
/// fn start_loading(engine: &mut Engine) {
/// 	let fbx_load_future: FbxLoadFuture = FbxLoader::load_from_url(engine, "anims/walk.fbx").into_future();
/// 	engine.spawn_task(move |_engine_context| async move {
/// 		match fbx_load_future.await {
/// 			Ok(scene_data) => {
/// 				println!("{} nodes", scene_data.node_list.len());
/// 			}
/// 			Err(error_message) => {
/// 				eprintln!("{}", error_message);
/// 			}
/// 		}
/// 	});
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct FbxLoadFuture {
	fbx_load_handle: FbxLoadHandle,
}

impl Future for FbxLoadFuture {
	type Output = Result<FbxSceneData, String>;

	//==============================================================================
	/// 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut FbxLoadFuture>, context: &mut Context<'_>) -> Poll<Result<FbxSceneData, String>> {
		match self.fbx_load_handle.get_scene_data() {
			Some(scene_data_result) => {
				Poll::Ready(scene_data_result)
			}
			None => {
				context.waker().wake_by_ref();
				Poll::Pending
			}
		}
	}
}

impl IntoFuture for FbxLoadHandle {
	type Output = Result<FbxSceneData, String>;
	type IntoFuture = FbxLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> FbxLoadFuture {
		FbxLoadFuture {
			fbx_load_handle: self,
		}
	}
}

//==============================================================================
/// FBX 로더.
/// - 바이너리 FBX 만 읽는다. (ASCII FBX 는 Err)
///
/// # 예제
/// 바이너리 FBX 가 아닌 바이트는 Err
/// ```rust
/// use vanilla::graphics3d::fbxloader::FbxLoader;
///
/// let mut fbx_loader = FbxLoader::new();
/// let parse_result = fbx_loader.parse(b"; FBX 7.4.0 project file");
/// assert_eq!(parse_result.err(), Some(String::from("FbxLoader: not a binary FBX file.")));
/// ```
//==============================================================================
pub struct FbxLoader {
	read_cursor: usize,
	use_wide_offsets: bool,
}

impl Default for FbxLoader {
	//==============================================================================
	/// 생성. (원본 new FbxLoader())
	//==============================================================================
	fn default() -> FbxLoader {
		FbxLoader::new()
	}
}

impl FbxLoader {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 로더. (파싱할 때마다 파일 처음부터 읽는다)
	//==============================================================================
	pub fn new() -> FbxLoader {
		FbxLoader {
			read_cursor: 0,
			use_wide_offsets: false,
		}
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 받기 부탁. (원본 정적 loadFromUrl, 다 받으면 손잡이의 get_scene_data 가 파싱한다)
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `url`: FBX 애셋 경로. (데스크탑은 애셋 폴더 후보 기준 상대 경로, 웹은 asset_base_url 기준 주소)
	///
	/// # 돌려줌
	/// 받기 손잡이. (데스크탑은 그 자리에서 읽어 이미 끝나 있다)
	//==============================================================================
	pub fn load_from_url(engine: &Engine, url: &str) -> FbxLoadHandle {
		FbxLoadHandle {
			load_handle: engine.request_asset_bytes(url),
		}
	}

	//==============================================================================
	/// 파싱. (바이너리 → 중간 표현)
	///
	/// # 인자
	/// - `file_bytes`: 바이너리 FBX 파일 바이트 전체.
	///
	/// # 돌려줌
	/// 중간 표현. 바이너리 FBX 가 아니거나(ASCII FBX 포함), 파일이 잘렸거나, Objects 나 Connections 노드가 없으면 Err.
	//==============================================================================
	pub fn parse(&mut self, file_bytes: &[u8]) -> Result<FbxSceneData, String> {
		let top_node_list = self.parse_node_records(file_bytes)?;
		self.interpret(&top_node_list)
	}

	//==============================================================================
	/// 최상위 노드 목록 파싱. (머리 확인 + 버전별 오프셋 크기 + 노드 레코드들, 원본 parse 의 앞부분)
	/// - 버전 7500 부터는 64비트 오프셋으로 읽는다. 끝의 160 바이트 꼬리 앞에서 멈춘다.
	///
	/// # 인자
	/// - `file_bytes`: 바이너리 FBX 파일 바이트 전체.
	///
	/// # 돌려줌
	/// 최상위 노드 레코드 목록. 머리가 "Kaydara FBX Binary" 가 아니거나, 레코드가 파일 밖을 가리키거나, 모르는 프로퍼티 타입이거나, 압축 배열을 풀 수 없으면 Err.
	///
	/// # 예제
	/// 머리와 빈 Objects 노드 하나뿐인 7.4 파일
	/// ```rust
	/// use vanilla::graphics3d::fbxloader::FbxLoader;
	///
	/// let node_name = b"Objects";
	/// let mut file_bytes = Vec::new();
	/// file_bytes.extend_from_slice(b"Kaydara FBX Binary  \x00\x1a\x00");
	/// file_bytes.extend_from_slice(&7400u32.to_le_bytes());
	/// let node_end_offset = file_bytes.len() + 13 + node_name.len();
	/// file_bytes.extend_from_slice(&(node_end_offset as u32).to_le_bytes());
	/// file_bytes.extend_from_slice(&0u32.to_le_bytes());
	/// file_bytes.extend_from_slice(&0u32.to_le_bytes());
	/// file_bytes.push(node_name.len() as u8);
	/// file_bytes.extend_from_slice(node_name);
	/// file_bytes.extend_from_slice(&[0u8; 13]);
	/// file_bytes.extend_from_slice(&[0u8; 160]);
	/// assert_eq!(FbxLoader::read_version(&file_bytes), Ok(7400));
	/// let top_node_list = FbxLoader::new().parse_node_records(&file_bytes).unwrap();
	/// assert_eq!(top_node_list.len(), 1);
	/// assert_eq!(top_node_list[0].name, "Objects");
	/// assert!(FbxLoader::new().interpret(&top_node_list).is_err());
	/// ```
	//==============================================================================
	pub fn parse_node_records(&mut self, file_bytes: &[u8]) -> Result<Vec<FbxNodeRecord>, String> {
		let magic_length = file_bytes.len().min(20);
		let magic_text = String::from_utf8_lossy(&file_bytes[..magic_length]);
		if !magic_text.starts_with("Kaydara FBX Binary") {
			return Err(String::from("FbxLoader: not a binary FBX file."));
		}
		let fbx_version = read_uint32(file_bytes, 23)?;
		self.use_wide_offsets = fbx_version >= 7500;
		self.read_cursor = 27;

		let mut top_node_list = Vec::new();
		while (self.read_cursor as i64) < file_bytes.len() as i64 - FBX_FOOTER_MARGIN {
			match self.read_node_record(file_bytes)? {
				Some(top_node) => {
					top_node_list.push(top_node);
				}
				None => {
					break;
				}
			}
		}
		Ok(top_node_list)
	}

	//==============================================================================
	/// FBX 버전 반환. (머리의 버전 번호, 7400 = 7.4)
	///
	/// # 인자
	/// - `file_bytes`: FBX 파일 바이트. (머리가 맞는지는 보지 않는다)
	///
	/// # 돌려줌
	/// 버전 번호. (바이트 23 ~ 26 의 리틀 엔디언 u32) 27 바이트보다 짧으면 Err.
	//==============================================================================
	pub fn read_version(file_bytes: &[u8]) -> Result<u32, String> {
		read_uint32(file_bytes, 23)
	}

	//==============================================================================
	/// 오프셋 값 읽기. (버전에 따라 32/64비트)
	//==============================================================================
	fn read_offset_value(&mut self, file_bytes: &[u8]) -> Result<usize, String> {
		if self.use_wide_offsets {
			let value = read_uint64(file_bytes, self.read_cursor)?;
			self.read_cursor += 8;
			return Ok(value as usize);
		}
		let value = read_uint32(file_bytes, self.read_cursor)?;
		self.read_cursor += 4;
		Ok(value as usize)
	}

	//==============================================================================
	/// 노드 레코드 읽기. (재귀, 프로퍼티 + 자식, 끝 표시 레코드면 None)
	//==============================================================================
	fn read_node_record(&mut self, file_bytes: &[u8]) -> Result<Option<FbxNodeRecord>, String> {
		let end_offset = self.read_offset_value(file_bytes)?;
		let property_count = self.read_offset_value(file_bytes)?;
		self.read_offset_value(file_bytes)?;
		let name_length = read_uint8(file_bytes, self.read_cursor)? as usize;
		self.read_cursor += 1;
		if end_offset == 0 {
			return Ok(None);
		}
		let node_name = String::from_utf8_lossy(read_slice(file_bytes, self.read_cursor, name_length)?).into_owned();
		self.read_cursor += name_length;

		let mut property_list = Vec::with_capacity(property_count.min(64));
		for _property_index in 0..property_count {
			let property = self.read_property(file_bytes)?;
			property_list.push(property);
		}

		let mut child_node_list = Vec::new();
		while self.read_cursor < end_offset {
			match self.read_node_record(file_bytes)? {
				Some(child_node) => {
					child_node_list.push(child_node);
				}
				None => {
					break;
				}
			}
		}
		self.read_cursor = end_offset;
		Ok(Some(FbxNodeRecord {
			name: node_name,
			properties: property_list,
			children: child_node_list,
		}))
	}

	//==============================================================================
	/// 배열 프로퍼티 바이트 읽기. (raw / deflate 인코딩, 원소 바이트를 돌려준다)
	//==============================================================================
	fn read_array_property_bytes(&mut self, file_bytes: &[u8], element_byte_size: usize) -> Result<Vec<u8>, String> {
		let array_length = read_uint32(file_bytes, self.read_cursor)? as usize;
		let encoding = read_uint32(file_bytes, self.read_cursor + 4)?;
		let compressed_length = read_uint32(file_bytes, self.read_cursor + 8)? as usize;
		self.read_cursor += 12;

		let raw_byte_length = array_length * element_byte_size;
		let source_bytes = if encoding == 1 {
			let compressed_bytes = read_slice(file_bytes, self.read_cursor, compressed_length)?;
			let inflated_bytes = FbxLoader::inflate(compressed_bytes, raw_byte_length)?;
			self.read_cursor += compressed_length;
			inflated_bytes
		}
		else {
			let raw_bytes = read_slice(file_bytes, self.read_cursor, raw_byte_length)?.to_vec();
			self.read_cursor += raw_byte_length;
			raw_bytes
		};
		if source_bytes.len() < raw_byte_length {
			return Err(String::from("FbxLoader: array property is shorter than its length."));
		}
		Ok(source_bytes)
	}

	//==============================================================================
	/// deflate 압축 해제. (zlib 머리가 붙은 deflate, 원본 DecompressionStream("deflate"))
	///
	/// # 인자
	/// - `compressed_bytes`: zlib 머리가 붙은 deflate 바이트.
	/// - `expected_byte_length`: 풀린 크기 예상. (미리 잡을 용량일 뿐, 풀린 길이와 맞는지 보지 않는다)
	///
	/// # 돌려줌
	/// 풀린 바이트. 압축 자료가 깨졌으면 Err.
	///
	/// # 예제
	/// zlib 으로 누른 바이트 풀기
	/// ```rust
	/// use std::io::Write;
	/// use vanilla::graphics3d::fbxloader::FbxLoader;
	///
	/// let original_bytes = vec![7u8; 256];
	/// let mut zlib_encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
	/// zlib_encoder.write_all(&original_bytes).unwrap();
	/// let compressed_bytes = zlib_encoder.finish().unwrap();
	/// assert_eq!(FbxLoader::inflate(&compressed_bytes, original_bytes.len()), Ok(original_bytes));
	/// assert!(FbxLoader::inflate(&[1, 2, 3], 3).is_err());
	/// ```
	//==============================================================================
	pub fn inflate(compressed_bytes: &[u8], expected_byte_length: usize) -> Result<Vec<u8>, String> {
		let mut decompressed_bytes = Vec::with_capacity(expected_byte_length);
		let mut zlib_decoder = flate2::read::ZlibDecoder::new(compressed_bytes);
		zlib_decoder.read_to_end(&mut decompressed_bytes).map_err(|error| format!("FbxLoader: inflate failed: {}", error))?;
		Ok(decompressed_bytes)
	}

	//==============================================================================
	/// 단일 프로퍼티 읽기.
	//==============================================================================
	fn read_property(&mut self, file_bytes: &[u8]) -> Result<FbxPropertyValue, String> {
		let type_code = read_uint8(file_bytes, self.read_cursor)? as char;
		self.read_cursor += 1;
		match type_code {
			'Y' => {
				let value = read_int16(file_bytes, self.read_cursor)?;
				self.read_cursor += 2;
				Ok(FbxPropertyValue::Int16(value))
			}
			'C' => {
				let value = read_uint8(file_bytes, self.read_cursor)?;
				self.read_cursor += 1;
				Ok(FbxPropertyValue::Boolean(value != 0))
			}
			'I' => {
				let value = read_int32(file_bytes, self.read_cursor)?;
				self.read_cursor += 4;
				Ok(FbxPropertyValue::Int32(value))
			}
			'F' => {
				let value = read_float32(file_bytes, self.read_cursor)?;
				self.read_cursor += 4;
				Ok(FbxPropertyValue::Float32(value))
			}
			'D' => {
				let value = read_float64(file_bytes, self.read_cursor)?;
				self.read_cursor += 8;
				Ok(FbxPropertyValue::Float64(value))
			}
			'L' => {
				let value = read_int64(file_bytes, self.read_cursor)?;
				self.read_cursor += 8;
				Ok(FbxPropertyValue::Int64(value))
			}
			'f' => {
				let element_bytes = self.read_array_property_bytes(file_bytes, 4)?;
				let values = element_bytes.as_chunks::<4>().0.iter().map(|chunk| f32::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::Float32array(values))
			}
			'd' => {
				let element_bytes = self.read_array_property_bytes(file_bytes, 8)?;
				let values = element_bytes.as_chunks::<8>().0.iter().map(|chunk| f64::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::Float64array(values))
			}
			'l' => {
				let element_bytes = self.read_array_property_bytes(file_bytes, 8)?;
				let values = element_bytes.as_chunks::<8>().0.iter().map(|chunk| i64::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::Int64array(values))
			}
			'i' => {
				let element_bytes = self.read_array_property_bytes(file_bytes, 4)?;
				let values = element_bytes.as_chunks::<4>().0.iter().map(|chunk| i32::from_le_bytes(*chunk)).collect();
				Ok(FbxPropertyValue::Int32array(values))
			}
			'b' => {
				let element_bytes = self.read_array_property_bytes(file_bytes, 1)?;
				Ok(FbxPropertyValue::BooleanArray(element_bytes))
			}
			'S' => {
				let string_length = read_uint32(file_bytes, self.read_cursor)? as usize;
				self.read_cursor += 4;
				let value = String::from_utf8_lossy(read_slice(file_bytes, self.read_cursor, string_length)?).into_owned();
				self.read_cursor += string_length;
				Ok(FbxPropertyValue::String(value))
			}
			'R' => {
				let raw_length = read_uint32(file_bytes, self.read_cursor)? as usize;
				self.read_cursor += 4;
				let value = read_slice(file_bytes, self.read_cursor, raw_length)?.to_vec();
				self.read_cursor += raw_length;
				Ok(FbxPropertyValue::Raw(Rc::new(value)))
			}
			_ => {
				Err(format!("FbxLoader: unknown property type: {}", type_code))
			}
		}
	}

	//==============================================================================
	/// 오브젝트/커넥션 그래프 해석. (중간 표현 구성)
	///
	/// # 인자
	/// - `top_node_list`: 최상위 노드 레코드 목록. (parse_node_records 가 돌려준 것)
	///
	/// # 돌려줌
	/// 중간 표현. Objects 나 Connections 노드가 없으면 Err.
	//==============================================================================
	pub fn interpret(&self, top_node_list: &[FbxNodeRecord]) -> Result<FbxSceneData, String> {
		let objects_node = top_node_list.iter().find(|node| node.name == "Objects").ok_or_else(|| String::from("FbxLoader: Objects node not found."))?;
		let connections_node = top_node_list.iter().find(|node| node.name == "Connections").ok_or_else(|| String::from("FbxLoader: Connections node not found."))?;

		// 커넥션 목록 구성. (OO: 오브젝트-오브젝트, OP: 오브젝트-프로퍼티)
		let mut object_connection_list = Vec::new();
		let mut property_connection_list = Vec::new();
		for connection in connections_node.children.iter() {
			let connection_type = connection.get_string_property(0);
			let source_id = connection.get_property(1).and_then(|property| property.as_integer());
			let destination_id = connection.get_property(2).and_then(|property| property.as_integer());
			let (source_id, destination_id) = match (source_id, destination_id) {
				(Some(source_id), Some(destination_id)) => {
					(source_id, destination_id)
				}
				_ => {
					continue;
				}
			};
			if connection_type == "OO" {
				object_connection_list.push(ObjectConnection { source_id, destination_id });
			}
			else if connection_type == "OP" {
				let property_name = connection.get_string_property(3);
				property_connection_list.push(PropertyConnection { source_id, destination_id, property_name });
			}
		}

		let geometry_by_id = self.interpret_geometries(objects_node);
		let (mut skin_deformer_by_id, cluster_by_id) = self.interpret_skins(objects_node, &object_connection_list);
		let video_by_id = self.interpret_videos(objects_node);
		let texture_by_id = self.interpret_textures(objects_node, &object_connection_list);
		let material_by_id = self.interpret_materials(objects_node, &property_connection_list, &texture_by_id);
		let (node_list, node_id_to_index, root_node_indices) = self.interpret_models(objects_node, &object_connection_list, &geometry_by_id, &mut skin_deformer_by_id, &material_by_id);
		let animation_list = self.interpret_animations(objects_node, &object_connection_list, &property_connection_list, &node_id_to_index);

		Ok(FbxSceneData {
			node_list,
			node_id_to_index,
			root_node_indices,
			geometry_by_id,
			skin_deformer_by_id,
			cluster_by_id,
			material_by_id,
			texture_by_id,
			video_by_id,
			animation_list,
		})
	}

	//==============================================================================
	/// 지오메트리 해석. (삼각화 + 노멀/UV 레이어 펼침 + 원본 정점 매핑)
	//==============================================================================
	fn interpret_geometries(&self, objects_node: &FbxNodeRecord) -> FbxObjectMap<FbxGeometry> {
		let mut geometry_by_id = FbxObjectMap::new();
		for object_node in objects_node.children.iter() {
			if object_node.name != "Geometry" {
				continue;
			}
			let geometry_id = match object_node.get_object_id() {
				Some(geometry_id) => {
					geometry_id
				}
				None => {
					continue;
				}
			};
			if let Some(geometry) = self.interpret_single_geometry(object_node) {
				geometry_by_id.insert(geometry_id, geometry);
			}
		}
		geometry_by_id
	}

	//==============================================================================
	/// 단일 지오메트리 해석. (정점 / 폴리곤 배열이 없으면 None)
	//==============================================================================
	fn interpret_single_geometry(&self, geometry_node: &FbxNodeRecord) -> Option<FbxGeometry> {
		let positions = geometry_node.find_child("Vertices")?.get_property(0)?;
		let polygon_vertex_indices = geometry_node.find_child("PolygonVertexIndex")?.get_property(0)?.to_integer_list();

		// 폴리곤 → 팬 삼각화. (코너: 원본 정점 인덱스 + 폴리곤 정점 순번)
		let mut corner_vertex_indices: Vec<i64> = Vec::new();
		let mut corner_polygon_vertex_positions: Vec<usize> = Vec::new();
		let decode_vertex_index = |raw_index: i64| -> i64 {
			if raw_index < 0 {
				-raw_index - 1
			}
			else {
				raw_index
			}
		};
		let mut polygon_start = 0;
		for polygon_vertex_position in 0..polygon_vertex_indices.len() {
			let is_polygon_end = polygon_vertex_indices[polygon_vertex_position] < 0;
			if is_polygon_end {
				let polygon_end = polygon_vertex_position;
				for triangle_corner in (polygon_start + 1)..polygon_end {
					let first_vertex_index = decode_vertex_index(polygon_vertex_indices[polygon_start]);
					let second_vertex_index = polygon_vertex_indices[triangle_corner];
					let third_vertex_index = decode_vertex_index(polygon_vertex_indices[triangle_corner + 1]);
					corner_vertex_indices.push(first_vertex_index);
					corner_polygon_vertex_positions.push(polygon_start);
					corner_vertex_indices.push(second_vertex_index);
					corner_polygon_vertex_positions.push(triangle_corner);
					corner_vertex_indices.push(third_vertex_index);
					corner_polygon_vertex_positions.push(triangle_corner + 1);
				}
				polygon_start = polygon_vertex_position + 1;
			}
		}

		let normal_reader = self.build_layer_reader(geometry_node, "LayerElementNormal", "Normals", 3);
		let uv_reader = self.build_layer_reader(geometry_node, "LayerElementUV", "UV", 2);
		let material_layer = self.build_material_layer(geometry_node);

		let corner_count = corner_vertex_indices.len();
		let triangle_count = corner_count / 3;
		let mut output_positions = vec![0.0f32; corner_count * 3];
		let mut output_normals = vec![0.0f32; corner_count * 3];
		let mut output_texture_coordinates = vec![0.0f32; corner_count * 2];
		let mut output_original_vertex_indices = vec![0i32; corner_count];
		let mut material_index_per_triangle = vec![0i32; triangle_count];

		let mut normal = [0.0f64; 3];
		let mut texture_coordinate = [0.0f64; 2];
		for corner_index in 0..corner_count {
			let original_vertex_index = corner_vertex_indices[corner_index];
			let polygon_vertex_position = corner_polygon_vertex_positions[corner_index];
			output_original_vertex_indices[corner_index] = original_vertex_index as i32;
			let position_offset = (original_vertex_index.max(0) as usize) * 3;
			for component_index in 0..3 {
				output_positions[corner_index * 3 + component_index] = positions.get_number_at(position_offset + component_index).unwrap_or(0.0) as f32;
			}

			normal_reader.read(original_vertex_index.max(0) as usize, polygon_vertex_position, &mut normal);
			output_normals[corner_index * 3] = normal[0] as f32;
			output_normals[corner_index * 3 + 1] = normal[1] as f32;
			output_normals[corner_index * 3 + 2] = normal[2] as f32;

			uv_reader.read(original_vertex_index.max(0) as usize, polygon_vertex_position, &mut texture_coordinate);
			output_texture_coordinates[corner_index * 2] = texture_coordinate[0] as f32;
			output_texture_coordinates[corner_index * 2 + 1] = texture_coordinate[1] as f32;
		}

		for (triangle_index, material_index) in material_index_per_triangle.iter_mut().enumerate() {
			let first_corner_polygon_vertex_position = corner_polygon_vertex_positions[triangle_index * 3];
			*material_index = material_layer(first_corner_polygon_vertex_position);
		}

		Some(FbxGeometry {
			positions: output_positions,
			normals: output_normals,
			texture_coordinates: output_texture_coordinates,
			original_vertex_indices: output_original_vertex_indices,
			material_index_per_triangle,
			corner_count,
			original_vertex_count: positions.get_array_length() / 3,
		})
	}

	//==============================================================================
	/// 레이어 리더 구성. (노멀/UV, 매핑/참조 방식별 조회기)
	//==============================================================================
	fn build_layer_reader<'a>(&self, geometry_node: &'a FbxNodeRecord, layer_element_name: &str, data_array_name: &str, component_count: usize) -> LayerReader<'a> {
		let layer_node = match geometry_node.find_child(layer_element_name) {
			Some(layer_node) => {
				layer_node
			}
			None => {
				return LayerReader {
					data_array: None,
					mapping_type: String::new(),
					reference_type: String::new(),
					index_array: None,
					component_count,
				};
			}
		};
		let index_array_name = if data_array_name == "UV" {
			String::from("UVIndex")
		}
		else {
			format!("{}Index", data_array_name)
		};
		let data_array = layer_node.find_child(data_array_name).and_then(|data_node| data_node.get_property(0));
		let mapping_type = layer_node.find_child("MappingInformationType").map(|mapping_node| mapping_node.get_string_property(0)).unwrap_or_default();
		let reference_type = layer_node.find_child("ReferenceInformationType").map(|reference_node| reference_node.get_string_property(0)).unwrap_or_default();
		let index_array = layer_node.find_child(&index_array_name).and_then(|index_node| index_node.get_property(0));
		LayerReader {
			data_array,
			mapping_type,
			reference_type,
			index_array,
			component_count,
		}
	}

	//==============================================================================
	/// 머티리얼 레이어 구성. (폴리곤 정점 순번 → 머티리얼 인덱스, 원본처럼 순번으로 바로 찾는다)
	//==============================================================================
	fn build_material_layer<'a>(&self, geometry_node: &'a FbxNodeRecord) -> Box<dyn Fn(usize) -> i32 + 'a> {
		let layer_node = match geometry_node.find_child("LayerElementMaterial") {
			Some(layer_node) => {
				layer_node
			}
			None => {
				return Box::new(|_polygon_vertex_position| 0);
			}
		};
		let mapping_type = layer_node.find_child("MappingInformationType").map(|mapping_node| mapping_node.get_string_property(0)).unwrap_or_default();
		let material_array = layer_node.find_child("Materials").and_then(|materials_node| materials_node.get_property(0));
		Box::new(move |polygon_vertex_position| {
			let material_array = match material_array {
				Some(material_array) => {
					material_array
				}
				None => {
					return 0;
				}
			};
			if mapping_type == "AllSame" {
				return material_array.get_integer_at(0).unwrap_or(0) as i32;
			}
			material_array.get_integer_at(polygon_vertex_position).unwrap_or(0) as i32
		})
	}

	//==============================================================================
	/// 스킨/클러스터 해석. (본별 정점 가중치 + 바인드 역행렬용 링크 변환)
	//==============================================================================
	fn interpret_skins(&self, objects_node: &FbxNodeRecord, object_connection_list: &[ObjectConnection]) -> (FbxObjectMap<FbxSkinDeformer>, FbxObjectMap<FbxCluster>) {
		let mut skin_deformer_by_id = FbxObjectMap::new();
		let mut cluster_by_id = FbxObjectMap::new();
		for object_node in objects_node.children.iter() {
			if object_node.name != "Deformer" {
				continue;
			}
			let deformer_id = match object_node.get_object_id() {
				Some(deformer_id) => {
					deformer_id
				}
				None => {
					continue;
				}
			};
			let deformer_sub_type = object_node.get_string_property(2);
			if deformer_sub_type == "Skin" {
				skin_deformer_by_id.insert(deformer_id, FbxSkinDeformer::default());
			}
			else if deformer_sub_type == "Cluster" {
				let read_array = |child_name: &str| object_node.find_child(child_name).and_then(|child| child.get_property(0));
				let cluster = FbxCluster {
					vertex_indices: read_array("Indexes").map(|property| property.to_integer_list()).unwrap_or_default(),
					weights: read_array("Weights").map(|property| property.to_number_list()).unwrap_or_default(),
					transform: read_array("Transform").map(|property| property.to_number_list()),
					transform_link: read_array("TransformLink").map(|property| property.to_number_list()),
					bone_node_id: None,
				};
				cluster_by_id.insert(deformer_id, cluster);
			}
		}

		// 커넥션으로 스킨↔클러스터, 클러스터↔본 연결.
		for connection in object_connection_list.iter() {
			if skin_deformer_by_id.contains(connection.destination_id) && cluster_by_id.contains(connection.source_id) {
				if let Some(skin_deformer) = skin_deformer_by_id.get_mut(connection.destination_id) {
					skin_deformer.cluster_ids.push(connection.source_id);
				}
			}
			else if let Some(cluster) = cluster_by_id.get_mut(connection.destination_id) {
				cluster.bone_node_id = Some(connection.source_id);
			}
		}
		(skin_deformer_by_id, cluster_by_id)
	}

	//==============================================================================
	/// 비디오 해석. (임베디드 이미지 바이트)
	//==============================================================================
	fn interpret_videos(&self, objects_node: &FbxNodeRecord) -> FbxObjectMap<FbxVideo> {
		let mut video_by_id = FbxObjectMap::new();
		for object_node in objects_node.children.iter() {
			if object_node.name != "Video" {
				continue;
			}
			let video_id = match object_node.get_object_id() {
				Some(video_id) => {
					video_id
				}
				None => {
					continue;
				}
			};
			let content_bytes = match object_node.find_child("Content").and_then(|content_node| content_node.get_property(0)) {
				Some(FbxPropertyValue::Raw(bytes)) => {
					Some(bytes.clone())
				}
				_ => {
					None
				}
			};
			let filename = object_node.children.iter().find(|child| child.name == "RelativeFilename" || child.name == "Filename").map(|filename_node| filename_node.get_string_property(0)).unwrap_or_default();
			video_by_id.insert(video_id, FbxVideo { content_bytes, filename });
		}

		// 동일 이미지 중복 참조 보완. (같은 파일을 두 번 참조하면 두 번째 Content 는 비어 있다)
		let mut content_by_file_name: HashMap<String, Rc<Vec<u8>>> = HashMap::new();
		for video in video_by_id.values().iter() {
			let content_bytes = match &video.content_bytes {
				Some(content_bytes) if !content_bytes.is_empty() => {
					content_bytes
				}
				_ => {
					continue;
				}
			};
			let file_name = FbxLoader::extract_file_name(&video.filename);
			content_by_file_name.entry(file_name).or_insert_with(|| content_bytes.clone());
		}
		for video in video_by_id.values_mut().iter_mut() {
			let has_content = video.content_bytes.as_ref().map(|content_bytes| !content_bytes.is_empty()).unwrap_or(false);
			if has_content {
				continue;
			}
			let file_name = FbxLoader::extract_file_name(&video.filename);
			if let Some(shared_content_bytes) = content_by_file_name.get(&file_name) {
				video.content_bytes = Some(shared_content_bytes.clone());
			}
		}
		video_by_id
	}

	//==============================================================================
	/// 파일 경로에서 파일명 추출. (윈도우/유닉스 구분자 모두 대응)
	///
	/// # 인자
	/// - `file_path`: 파일 경로. (/ 와 \ 구분자 모두)
	///
	/// # 돌려줌
	/// 마지막 구분자 뒤의 파일 이름. (구분자가 없으면 경로 그대로)
	//==============================================================================
	pub fn extract_file_name(file_path: &str) -> String {
		let normalized_path = file_path.replace('\\', "/");
		match normalized_path.rfind('/') {
			Some(last_separator_index) => {
				normalized_path[last_separator_index + 1..].to_string()
			}
			None => {
				normalized_path
			}
		}
	}

	//==============================================================================
	/// 텍스처 해석. (비디오 연결)
	//==============================================================================
	fn interpret_textures(&self, objects_node: &FbxNodeRecord, object_connection_list: &[ObjectConnection]) -> FbxObjectMap<FbxTexture> {
		let mut texture_by_id = FbxObjectMap::new();
		for object_node in objects_node.children.iter() {
			if object_node.name != "Texture" {
				continue;
			}
			let texture_id = match object_node.get_object_id() {
				Some(texture_id) => {
					texture_id
				}
				None => {
					continue;
				}
			};
			let filename = object_node.children.iter().find(|child| child.name == "RelativeFilename" || child.name == "FileName").map(|filename_node| filename_node.get_string_property(0)).unwrap_or_default();
			texture_by_id.insert(texture_id, FbxTexture { video_id: None, filename });
		}
		for connection in object_connection_list.iter() {
			if let Some(texture) = texture_by_id.get_mut(connection.destination_id) {
				texture.video_id = Some(connection.source_id);
			}
		}
		texture_by_id
	}

	//==============================================================================
	/// 머티리얼 해석. (phong 색상/광택 + 슬롯별 텍스처 연결)
	//==============================================================================
	fn interpret_materials(&self, objects_node: &FbxNodeRecord, property_connection_list: &[PropertyConnection], texture_by_id: &FbxObjectMap<FbxTexture>) -> FbxObjectMap<FbxMaterial> {
		let mut material_by_id = FbxObjectMap::new();
		for object_node in objects_node.children.iter() {
			if object_node.name != "Material" {
				continue;
			}
			let material_id = match object_node.get_object_id() {
				Some(material_id) => {
					material_id
				}
				None => {
					continue;
				}
			};
			let mut material = FbxMaterial {
				diffuse_color: [1.0, 1.0, 1.0],
				specular_color: [0.5, 0.5, 0.5],
				shininess: 20.0,
				diffuse_texture_id: None,
				normal_texture_id: None,
				specular_texture_id: None,
				gloss_texture_id: None,
				opacity_texture_id: None,
			};
			if let Some(properties70_node) = object_node.find_child("Properties70") {
				for property_record in properties70_node.children.iter() {
					let property_key = property_record.get_string_property(0);
					if property_key == "DiffuseColor" || property_key == "Diffuse" {
						if let Some(color) = read_property_vector(property_record) {
							material.diffuse_color = color;
						}
					}
					else if property_key == "SpecularColor" || property_key == "Specular" {
						if let Some(color) = read_property_vector(property_record) {
							material.specular_color = color;
						}
					}
					else if property_key == "Shininess" || property_key == "ShininessExponent" {
						if let Some(shininess) = property_record.get_property(4).and_then(|property| property.as_number()) {
							material.shininess = shininess;
						}
					}
				}
			}
			material_by_id.insert(material_id, material);
		}

		// 텍스처 → 머티리얼 슬롯 연결. (OP 프로퍼티명으로 판별)
		for connection in property_connection_list.iter() {
			if !texture_by_id.contains(connection.source_id) {
				continue;
			}
			let material = match material_by_id.get_mut(connection.destination_id) {
				Some(material) => {
					material
				}
				None => {
					continue;
				}
			};
			let property_name = connection.property_name.as_str();
			if property_name == "DiffuseColor" {
				material.diffuse_texture_id = Some(connection.source_id);
			}
			else if property_name == "NormalMap" || property_name == "Bump" {
				material.normal_texture_id = Some(connection.source_id);
			}
			else if property_name == "SpecularColor" || property_name == "SpecularFactor" {
				material.specular_texture_id = Some(connection.source_id);
			}
			else if property_name == "Shininess" || property_name == "ShininessExponent" {
				material.gloss_texture_id = Some(connection.source_id);
			}
			else if property_name == "TransparentColor" || property_name == "TransparencyFactor" {
				material.opacity_texture_id = Some(connection.source_id);
			}
		}
		material_by_id
	}

	//==============================================================================
	/// 모델(노드) 해석. (계층 + 로컬 변환 + 지오메트리/스킨/머티리얼 소속)
	//==============================================================================
	fn interpret_models(&self, objects_node: &FbxNodeRecord, object_connection_list: &[ObjectConnection], geometry_by_id: &FbxObjectMap<FbxGeometry>, skin_deformer_by_id: &mut FbxObjectMap<FbxSkinDeformer>, material_by_id: &FbxObjectMap<FbxMaterial>) -> (Vec<FbxNode>, HashMap<i64, usize>, Vec<usize>) {
		let mut node_list: Vec<FbxNode> = Vec::new();
		let mut node_id_to_index: HashMap<i64, usize> = HashMap::new();
		for object_node in objects_node.children.iter() {
			if object_node.name != "Model" {
				continue;
			}
			let model_id = match object_node.get_object_id() {
				Some(model_id) => {
					model_id
				}
				None => {
					continue;
				}
			};
			let model_name = object_node.get_string_property(1);
			let model_sub_type = object_node.get_string_property(2);
			let mut node = FbxNode {
				id: model_id,
				name: model_name,
				is_bone: model_sub_type == "LimbNode",
				translation: [0.0, 0.0, 0.0],
				rotation_euler_degrees: [0.0, 0.0, 0.0],
				pre_rotation_degrees: [0.0, 0.0, 0.0],
				scale: [1.0, 1.0, 1.0],
				geometry_id: None,
				skin_id: None,
				material_ids: Vec::new(),
				child_indices: Vec::new(),
				parent_id: None,
			};
			if let Some(properties70_node) = object_node.find_child("Properties70") {
				for property_record in properties70_node.children.iter() {
					let property_key = property_record.get_string_property(0);
					let property_vector = match read_property_vector(property_record) {
						Some(property_vector) => {
							property_vector
						}
						None => {
							continue;
						}
					};
					if property_key == "Lcl Translation" {
						node.translation = property_vector;
					}
					else if property_key == "Lcl Rotation" {
						node.rotation_euler_degrees = property_vector;
					}
					else if property_key == "Lcl Scaling" {
						node.scale = property_vector;
					}
					else if property_key == "PreRotation" {
						node.pre_rotation_degrees = property_vector;
					}
				}
			}
			node_id_to_index.insert(model_id, node_list.len());
			node_list.push(node);
		}

		// 커넥션으로 계층/지오메트리/스킨/머티리얼 소속 구성.
		let mut root_node_indices = Vec::new();
		for connection in object_connection_list.iter() {
			let source_node_index = node_id_to_index.get(&connection.source_id).copied();
			let destination_node_index = node_id_to_index.get(&connection.destination_id).copied();
			if source_node_index.is_some() && connection.destination_id == 0 {
				root_node_indices.push(source_node_index.unwrap_or_default());
			}
			else if let (Some(child_index), Some(parent_index)) = (source_node_index, destination_node_index) {
				node_list[parent_index].child_indices.push(child_index);
				node_list[child_index].parent_id = Some(connection.destination_id);
			}
			else if geometry_by_id.contains(connection.source_id) && destination_node_index.is_some() {
				let owner_node = &mut node_list[destination_node_index.unwrap_or_default()];
				owner_node.geometry_id = Some(connection.source_id);
			}
			else if skin_deformer_by_id.contains(connection.source_id) && geometry_by_id.contains(connection.destination_id) {
				// 스킨은 지오메트리에 붙는다, 지오메트리를 소유한 노드에서 역참조하도록 지오메트리에 기록.
				if let Some(skin_deformer) = skin_deformer_by_id.get_mut(connection.source_id) {
					skin_deformer.geometry_id = Some(connection.destination_id);
				}
			}
			else if material_by_id.contains(connection.source_id) && destination_node_index.is_some() {
				let owner_node = &mut node_list[destination_node_index.unwrap_or_default()];
				owner_node.material_ids.push(connection.source_id);
			}
		}

		// 지오메트리 → 스킨 역참조를 노드에 반영.
		for (skin_id, skin_deformer) in skin_deformer_by_id.iter() {
			let skin_geometry_id = match skin_deformer.geometry_id {
				Some(skin_geometry_id) => {
					skin_geometry_id
				}
				None => {
					continue;
				}
			};
			for node in node_list.iter_mut() {
				if node.geometry_id == Some(skin_geometry_id) {
					node.skin_id = Some(skin_id);
				}
			}
		}
		(node_list, node_id_to_index, root_node_indices)
	}

	//==============================================================================
	/// 애니메이션 해석. (스택 → 레이어 → 커브노드(T/R/S) → 커브(X/Y/Z))
	//==============================================================================
	fn interpret_animations(&self, objects_node: &FbxNodeRecord, object_connection_list: &[ObjectConnection], property_connection_list: &[PropertyConnection], node_id_to_index: &HashMap<i64, usize>) -> Vec<FbxAnimation> {
		// 커브 / 커브노드 / 레이어 / 스택 수집.
		let mut curve_by_id: FbxObjectMap<AnimationCurve> = FbxObjectMap::new();
		let mut curve_node_by_id: FbxObjectMap<AnimationCurveNode> = FbxObjectMap::new();
		let mut layer_id_set: HashSet<i64> = HashSet::new();
		let mut stack_by_id: FbxObjectMap<AnimationStack> = FbxObjectMap::new();
		for object_node in objects_node.children.iter() {
			let object_id = match object_node.get_object_id() {
				Some(object_id) => {
					object_id
				}
				None => {
					continue;
				}
			};
			if object_node.name == "AnimationCurve" {
				let key_times = object_node.find_child("KeyTime").and_then(|key_time_node| key_time_node.get_property(0)).map(|property| property.to_integer_list()).unwrap_or_default();
				let key_values = object_node.find_child("KeyValueFloat").and_then(|key_value_node| key_value_node.get_property(0)).map(|property| property.to_number_list()).unwrap_or_default();
				curve_by_id.insert(object_id, AnimationCurve { key_times, key_values });
			}
			else if object_node.name == "AnimationCurveNode" {
				curve_node_by_id.insert(object_id, AnimationCurveNode { target_node_id: None, target_property: None, curve_by_channel: HashMap::new() });
			}
			else if object_node.name == "AnimationLayer" {
				layer_id_set.insert(object_id);
			}
			else if object_node.name == "AnimationStack" {
				stack_by_id.insert(object_id, AnimationStack { name: object_node.get_string_property(1), layer_ids: Vec::new() });
			}
		}

		// 커브 → 커브노드 채널(d|X/d|Y/d|Z) 연결.
		for connection in property_connection_list.iter() {
			if curve_by_id.contains(connection.source_id) && curve_node_by_id.contains(connection.destination_id) {
				if let Some(curve_node) = curve_node_by_id.get_mut(connection.destination_id) {
					let channel_name = connection.property_name.replacen("d|", "", 1);
					curve_node.curve_by_channel.insert(channel_name, connection.source_id);
				}
			}
			else if curve_node_by_id.contains(connection.source_id) && node_id_to_index.contains_key(&connection.destination_id) {
				if let Some(curve_node) = curve_node_by_id.get_mut(connection.source_id) {
					curve_node.target_node_id = Some(connection.destination_id);
					curve_node.target_property = Some(connection.property_name.clone());
				}
			}
		}

		// 레이어 → 스택 소속 + 커브노드 → 레이어 소속.
		let mut layer_id_by_curve_node_id: HashMap<i64, i64> = HashMap::new();
		for connection in object_connection_list.iter() {
			if layer_id_set.contains(&connection.source_id) && stack_by_id.contains(connection.destination_id) {
				if let Some(stack) = stack_by_id.get_mut(connection.destination_id) {
					stack.layer_ids.push(connection.source_id);
				}
			}
			else if curve_node_by_id.contains(connection.source_id) && layer_id_set.contains(&connection.destination_id) {
				layer_id_by_curve_node_id.insert(connection.source_id, connection.destination_id);
			}
		}

		// 스택 → 애니메이션 클립. (스택 소속 레이어의 커브노드만 노드/프로퍼티별 채널로 변환)
		let mut animation_list = Vec::new();
		for (_stack_id, stack) in stack_by_id.iter() {
			let mut channel_list = Vec::new();
			let mut duration = 0.0f32;
			for (curve_node_id, curve_node) in curve_node_by_id.iter() {
				let target_node_id = match curve_node.target_node_id {
					Some(target_node_id) if node_id_to_index.contains_key(&target_node_id) => {
						target_node_id
					}
					_ => {
						continue;
					}
				};
				let owning_layer_id = layer_id_by_curve_node_id.get(&curve_node_id);
				let is_in_stack = match owning_layer_id {
					Some(owning_layer_id) => {
						stack.layer_ids.contains(owning_layer_id)
					}
					None => {
						false
					}
				};
				if !is_in_stack {
					continue;
				}
				if let Some((channel, channel_duration)) = self.build_animation_channel(curve_node, target_node_id, &curve_by_id, node_id_to_index) {
					channel_list.push(channel);
					duration = duration.max(channel_duration);
				}
			}
			animation_list.push(FbxAnimation {
				name: stack.name.clone(),
				duration,
				channels: channel_list,
			});
		}
		animation_list
	}

	//==============================================================================
	/// 애니메이션 채널 구성. (커브노드 → 노드 인덱스 + 경로 + 키타임/값, 채널과 길이를 돌려준다)
	//==============================================================================
	fn build_animation_channel(&self, curve_node: &AnimationCurveNode, target_node_id: i64, curve_by_id: &FbxObjectMap<AnimationCurve>, node_id_to_index: &HashMap<i64, usize>) -> Option<(FbxAnimationChannel, f32)> {
		let path = match curve_node.target_property.as_deref() {
			Some("Lcl Translation") => {
				AnimationPath::Translation
			}
			Some("Lcl Rotation") => {
				AnimationPath::Rotation
			}
			Some("Lcl Scaling") => {
				AnimationPath::Scale
			}
			_ => {
				return None;
			}
		};

		// 채널별 키타임 합집합 구성. (X/Y/Z 커브의 시간축이 동일하다고 가정, 아니면 합집합 정렬)
		let channel_names = ["X", "Y", "Z"];
		let mut time_set: HashSet<i64> = HashSet::new();
		for channel_name in channel_names.iter() {
			let curve = curve_node.curve_by_channel.get(*channel_name).and_then(|curve_id| curve_by_id.get(*curve_id));
			if let Some(curve) = curve {
				for key_time in curve.key_times.iter() {
					time_set.insert(*key_time);
				}
			}
		}
		let mut sorted_times: Vec<i64> = time_set.into_iter().collect();
		sorted_times.sort_unstable();
		if sorted_times.is_empty() {
			return None;
		}

		let component_count = 3;
		let mut times = vec![0.0f32; sorted_times.len()];
		let mut values = vec![0.0f32; sorted_times.len() * component_count];
		for (time_index, sorted_time) in sorted_times.iter().enumerate() {
			times[time_index] = (*sorted_time as f64 / FBX_TIME_UNIT) as f32;
			for (channel_index, channel_name) in channel_names.iter().enumerate() {
				let curve = curve_node.curve_by_channel.get(*channel_name).and_then(|curve_id| curve_by_id.get(*curve_id));
				let sampled_value = FbxLoader::sample_curve_at_time(curve, *sorted_time);
				values[time_index * component_count + channel_index] = sampled_value as f32;
			}
		}

		let node_index = *node_id_to_index.get(&target_node_id)?;
		let duration = *times.last().unwrap_or(&0.0);
		let channel = FbxAnimationChannel {
			node_index,
			path,
			times,
			values,
		};
		Some((channel, duration))
	}

	//==============================================================================
	/// 커브 시간 샘플링. (선형 보간, 키 사이, 커브가 없으면 0)
	//==============================================================================
	fn sample_curve_at_time(curve: Option<&AnimationCurve>, key_time: i64) -> f64 {
		let curve = match curve {
			Some(curve) => {
				curve
			}
			None => {
				return 0.0;
			}
		};
		let key_times = &curve.key_times;
		let key_values = &curve.key_values;
		if key_times.is_empty() || key_values.is_empty() {
			return 0.0;
		}
		if key_time <= key_times[0] {
			return key_values[0];
		}
		if key_time >= key_times[key_times.len() - 1] {
			return key_values[key_values.len() - 1];
		}
		let mut key_index = 0;
		while key_index < key_times.len() - 1 && key_times[key_index + 1] < key_time {
			key_index += 1;
		}
		let span_duration = (key_times[key_index + 1] - key_times[key_index]) as f64;
		let factor = if span_duration > 0.0 {
			(key_time - key_times[key_index]) as f64 / span_duration
		}
		else {
			0.0
		};
		let from_value = key_values.get(key_index).copied().unwrap_or(0.0);
		let to_value = key_values.get(key_index + 1).copied().unwrap_or(from_value);
		from_value + (to_value - from_value) * factor
	}
}

//==============================================================================
/// Properties70 레코드의 벡터 값 읽기. (P: 이름, 타입, 레이블, 플래그, x, y, z, 모자라면 None)
//==============================================================================
fn read_property_vector(property_record: &FbxNodeRecord) -> Option<[f64; 3]> {
	let x = property_record.get_property(4)?.as_number()?;
	let y = property_record.get_property(5)?.as_number()?;
	let z = property_record.get_property(6)?.as_number()?;
	Some([x, y, z])
}

//==============================================================================
/// 바이트 구간 읽기. (파일 밖이면 오류, 원본 DataView 의 RangeError 자리)
//==============================================================================
fn read_slice(file_bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
	let end_offset = offset.checked_add(length).ok_or_else(|| String::from("FbxLoader: offset overflow."))?;
	file_bytes.get(offset..end_offset).ok_or_else(|| String::from("FbxLoader: unexpected end of file."))
}

//==============================================================================
/// 부호 없는 8비트 읽기.
//==============================================================================
fn read_uint8(file_bytes: &[u8], offset: usize) -> Result<u8, String> {
	Ok(read_slice(file_bytes, offset, 1)?[0])
}

//==============================================================================
/// 부호 있는 16비트 읽기. (리틀 엔디언)
//==============================================================================
fn read_int16(file_bytes: &[u8], offset: usize) -> Result<i16, String> {
	let bytes = read_slice(file_bytes, offset, 2)?;
	Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
}

//==============================================================================
/// 부호 없는 32비트 읽기. (리틀 엔디언)
//==============================================================================
fn read_uint32(file_bytes: &[u8], offset: usize) -> Result<u32, String> {
	let bytes = read_slice(file_bytes, offset, 4)?;
	Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

//==============================================================================
/// 부호 있는 32비트 읽기. (리틀 엔디언)
//==============================================================================
fn read_int32(file_bytes: &[u8], offset: usize) -> Result<i32, String> {
	let bytes = read_slice(file_bytes, offset, 4)?;
	Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

//==============================================================================
/// 부호 없는 64비트 읽기. (리틀 엔디언)
//==============================================================================
fn read_uint64(file_bytes: &[u8], offset: usize) -> Result<u64, String> {
	let bytes = read_slice(file_bytes, offset, 8)?;
	Ok(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
}

//==============================================================================
/// 부호 있는 64비트 읽기. (리틀 엔디언)
//==============================================================================
fn read_int64(file_bytes: &[u8], offset: usize) -> Result<i64, String> {
	let bytes = read_slice(file_bytes, offset, 8)?;
	Ok(i64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
}

//==============================================================================
/// 32비트 실수 읽기. (리틀 엔디언)
//==============================================================================
fn read_float32(file_bytes: &[u8], offset: usize) -> Result<f32, String> {
	let bytes = read_slice(file_bytes, offset, 4)?;
	Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

//==============================================================================
/// 64비트 실수 읽기. (리틀 엔디언)
//==============================================================================
fn read_float64(file_bytes: &[u8], offset: usize) -> Result<f64, String> {
	let bytes = read_slice(file_bytes, offset, 8)?;
	Ok(f64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graphics3d::skinnedmodel::SkinnedModel;
	use std::io::Write;

	/// 예제 애셋 폴더. (이 저장소 옆에 받아 둔 vanilla.js 저장소, 읽기만 한다. 없으면 그 시험은 건너뛴다)
	const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

	//==============================================================================
	/// 시험용 노드 레코드. (이름 + 인코딩된 프로퍼티 + 자식)
	//==============================================================================
	struct TestNode {
		name: String,
		property_bytes_list: Vec<Vec<u8>>,
		children: Vec<TestNode>,
	}

	//==============================================================================
	/// 시험용 노드 만들기.
	//==============================================================================
	fn test_node(name: &str, property_bytes_list: Vec<Vec<u8>>, children: Vec<TestNode>) -> TestNode {
		TestNode {
			name: name.to_string(),
			property_bytes_list,
			children,
		}
	}

	//==============================================================================
	/// 노드 레코드 인코딩. (7.4 형식 32비트 오프셋, start_offset 은 파일 안 절대 위치)
	//==============================================================================
	fn encode_node(node: &TestNode, start_offset: usize) -> Vec<u8> {
		let property_bytes: Vec<u8> = node.property_bytes_list.concat();
		let header_length = 13 + node.name.len();
		let mut child_bytes = Vec::new();
		let mut child_offset = start_offset + header_length + property_bytes.len();
		for child in node.children.iter() {
			let encoded_child = encode_node(child, child_offset);
			child_offset += encoded_child.len();
			child_bytes.extend_from_slice(&encoded_child);
		}
		if !node.children.is_empty() {
			child_bytes.extend_from_slice(&[0u8; 13]);
		}
		let end_offset = start_offset + header_length + property_bytes.len() + child_bytes.len();
		let mut encoded_bytes = Vec::new();
		encoded_bytes.extend_from_slice(&(end_offset as u32).to_le_bytes());
		encoded_bytes.extend_from_slice(&(node.property_bytes_list.len() as u32).to_le_bytes());
		encoded_bytes.extend_from_slice(&(property_bytes.len() as u32).to_le_bytes());
		encoded_bytes.push(node.name.len() as u8);
		encoded_bytes.extend_from_slice(node.name.as_bytes());
		encoded_bytes.extend_from_slice(&property_bytes);
		encoded_bytes.extend_from_slice(&child_bytes);
		encoded_bytes
	}

	//==============================================================================
	/// 프로퍼티 인코딩: 64비트 정수.
	//==============================================================================
	fn property_long(value: i64) -> Vec<u8> {
		let mut bytes = vec![b'L'];
		bytes.extend_from_slice(&value.to_le_bytes());
		bytes
	}

	//==============================================================================
	/// 프로퍼티 인코딩: 문자열.
	//==============================================================================
	fn property_string(value: &str) -> Vec<u8> {
		let mut bytes = vec![b'S'];
		bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
		bytes.extend_from_slice(value.as_bytes());
		bytes
	}

	//==============================================================================
	/// 프로퍼티 인코딩: 64비트 실수.
	//==============================================================================
	fn property_double(value: f64) -> Vec<u8> {
		let mut bytes = vec![b'D'];
		bytes.extend_from_slice(&value.to_le_bytes());
		bytes
	}

	//==============================================================================
	/// 프로퍼티 인코딩: 배열. (원소 바이트, 압축이면 zlib deflate)
	//==============================================================================
	fn property_array(type_code: u8, element_bytes: Vec<u8>, element_count: usize, is_compressed: bool) -> Vec<u8> {
		let mut bytes = vec![type_code];
		bytes.extend_from_slice(&(element_count as u32).to_le_bytes());
		if is_compressed {
			let mut zlib_encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
			zlib_encoder.write_all(&element_bytes).expect("deflate");
			let compressed_bytes = zlib_encoder.finish().expect("deflate finish");
			bytes.extend_from_slice(&1u32.to_le_bytes());
			bytes.extend_from_slice(&(compressed_bytes.len() as u32).to_le_bytes());
			bytes.extend_from_slice(&compressed_bytes);
		}
		else {
			bytes.extend_from_slice(&0u32.to_le_bytes());
			bytes.extend_from_slice(&(element_bytes.len() as u32).to_le_bytes());
			bytes.extend_from_slice(&element_bytes);
		}
		bytes
	}

	//==============================================================================
	/// 프로퍼티 인코딩: 64비트 실수 배열.
	//==============================================================================
	fn property_double_array(values: &[f64], is_compressed: bool) -> Vec<u8> {
		property_array(b'd', values.iter().flat_map(|value| value.to_le_bytes()).collect(), values.len(), is_compressed)
	}

	//==============================================================================
	/// 프로퍼티 인코딩: 32비트 정수 배열.
	//==============================================================================
	fn property_integer_array(values: &[i32]) -> Vec<u8> {
		property_array(b'i', values.iter().flat_map(|value| value.to_le_bytes()).collect(), values.len(), false)
	}

	//==============================================================================
	/// 문자열 하나를 가진 자식 노드.
	//==============================================================================
	fn string_node(name: &str, value: &str) -> TestNode {
		test_node(name, vec![property_string(value)], Vec::new())
	}

	//==============================================================================
	/// 커넥션 노드. (OP 면 프로퍼티 이름까지)
	//==============================================================================
	fn connection_node(connection_type: &str, source_id: i64, destination_id: i64, property_name: Option<&str>) -> TestNode {
		let mut property_bytes_list = vec![property_string(connection_type), property_long(source_id), property_long(destination_id)];
		if let Some(property_name) = property_name {
			property_bytes_list.push(property_string(property_name));
		}
		test_node("C", property_bytes_list, Vec::new())
	}

	//==============================================================================
	/// 시험용 바이너리 FBX. (사각형 지오메트리 + 본 + 메시 노드 + 스킨/클러스터 + Y 이동 커브 하나)
	//==============================================================================
	fn build_test_fbx() -> Vec<u8> {
		let quad_vertices = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
		let geometry_node = test_node("Geometry", vec![property_long(100), property_string("Plane\u{0}\u{1}Geometry"), property_string("Mesh")], vec![
			test_node("Vertices", vec![property_double_array(&quad_vertices, true)], Vec::new()),
			test_node("PolygonVertexIndex", vec![property_integer_array(&[0, 1, 2, -4])], Vec::new()),
			test_node("LayerElementNormal", vec![], vec![
				string_node("MappingInformationType", "ByPolygonVertex"),
				string_node("ReferenceInformationType", "Direct"),
				test_node("Normals", vec![property_double_array(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], false)], Vec::new()),
			]),
			test_node("LayerElementUV", vec![], vec![
				string_node("MappingInformationType", "ByPolygonVertex"),
				string_node("ReferenceInformationType", "IndexToDirect"),
				test_node("UV", vec![property_double_array(&[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0], false)], Vec::new()),
				test_node("UVIndex", vec![property_integer_array(&[0, 1, 2, 3])], Vec::new()),
			]),
		]);
		let translation_record = test_node("P", vec![property_string("Lcl Translation"), property_string("Lcl Translation"), property_string(""), property_string("A"), property_double(1.0), property_double(2.0), property_double(3.0)], Vec::new());
		let bone_node = test_node("Model", vec![property_long(200), property_string("Bone\u{0}\u{1}Model"), property_string("LimbNode")], vec![test_node("Properties70", vec![], vec![translation_record])]);
		let mesh_node = test_node("Model", vec![property_long(201), property_string("Mesh\u{0}\u{1}Model"), property_string("Mesh")], Vec::new());
		let skin_node = test_node("Deformer", vec![property_long(300), property_string("Skin"), property_string("Skin")], Vec::new());
		let mut transform_link = [0.0f64; 16];
		transform_link[0] = 1.0;
		transform_link[5] = 1.0;
		transform_link[10] = 1.0;
		transform_link[15] = 1.0;
		transform_link[12] = 1.0;
		transform_link[13] = 2.0;
		transform_link[14] = 3.0;
		let cluster_node = test_node("Deformer", vec![property_long(301), property_string("Cluster"), property_string("Cluster")], vec![
			test_node("Indexes", vec![property_integer_array(&[0, 1, 2, 3])], Vec::new()),
			test_node("Weights", vec![property_double_array(&[1.0, 1.0, 1.0, 1.0], false)], Vec::new()),
			test_node("TransformLink", vec![property_double_array(&transform_link, false)], Vec::new()),
		]);
		let stack_node = test_node("AnimationStack", vec![property_long(400), property_string("Take\u{0}\u{1}AnimStack"), property_string("")], Vec::new());
		let layer_node = test_node("AnimationLayer", vec![property_long(401), property_string("Layer"), property_string("")], Vec::new());
		let curve_node_node = test_node("AnimationCurveNode", vec![property_long(402), property_string("T"), property_string("")], Vec::new());
		let key_time_bytes: Vec<u8> = [0i64, FBX_TIME_UNIT as i64].iter().flat_map(|value| value.to_le_bytes()).collect();
		let key_value_bytes: Vec<u8> = [1.0f32, 3.0].iter().flat_map(|value| value.to_le_bytes()).collect();
		let curve_node = test_node("AnimationCurve", vec![property_long(403), property_string(""), property_string("")], vec![
			test_node("KeyTime", vec![property_array(b'l', key_time_bytes, 2, true)], Vec::new()),
			test_node("KeyValueFloat", vec![property_array(b'f', key_value_bytes, 2, false)], Vec::new()),
		]);
		let objects_node = test_node("Objects", vec![], vec![geometry_node, bone_node, mesh_node, skin_node, cluster_node, stack_node, layer_node, curve_node_node, curve_node]);
		let connections_node = test_node("Connections", vec![], vec![
			connection_node("OO", 200, 0, None),
			connection_node("OO", 201, 0, None),
			connection_node("OO", 100, 201, None),
			connection_node("OO", 300, 100, None),
			connection_node("OO", 301, 300, None),
			connection_node("OO", 200, 301, None),
			connection_node("OO", 401, 400, None),
			connection_node("OO", 402, 401, None),
			connection_node("OP", 402, 200, Some("Lcl Translation")),
			connection_node("OP", 403, 402, Some("d|Y")),
		]);

		let mut file_bytes = Vec::new();
		file_bytes.extend_from_slice(b"Kaydara FBX Binary  \x00\x1a\x00");
		file_bytes.extend_from_slice(&7400u32.to_le_bytes());
		for top_node in [objects_node, connections_node].iter() {
			let encoded_top_node = encode_node(top_node, file_bytes.len());
			file_bytes.extend_from_slice(&encoded_top_node);
		}
		file_bytes.extend_from_slice(&[0u8; 13]);
		file_bytes.extend_from_slice(&[0u8; 176]);
		file_bytes
	}

	//==============================================================================
	/// 값 두 개가 허용 오차 안에서 같은지.
	//==============================================================================
	fn assert_near(actual: f32, expected: f32) {
		assert!((actual - expected).abs() < 1.0e-5, "{} expected {}", actual, expected);
	}

	//==============================================================================
	/// 노드 레코드 파싱. (머리, 최상위 노드, 압축 배열, 문자열과 정수 프로퍼티, 잘못된 머리)
	//==============================================================================
	#[test]
	fn node_record_parsing() {
		let file_bytes = build_test_fbx();
		assert_eq!(FbxLoader::read_version(&file_bytes), Ok(7400));
		let top_node_list = FbxLoader::new().parse_node_records(&file_bytes).expect("records");
		let top_node_names: Vec<&str> = top_node_list.iter().map(|node| node.name.as_str()).collect();
		assert_eq!(top_node_names, vec!["Objects", "Connections"]);
		let geometry_node = &top_node_list[0].children[0];
		assert_eq!(geometry_node.get_object_id(), Some(100));
		assert_eq!(geometry_node.get_string_property(1), "Plane\u{0}\u{1}Geometry");
		let vertices = geometry_node.find_child("Vertices").and_then(|node| node.get_property(0)).expect("vertices");
		assert_eq!(vertices.get_type_code(), 'd');
		assert_eq!(vertices.to_number_list(), vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
		assert_eq!(top_node_list[1].children.len(), 10);
		assert!(FbxLoader::new().parse(b"; FBX 7.4.0 project file, ascii").is_err());
		assert!(FbxLoader::new().parse(&file_bytes[..200]).is_err());
	}

	//==============================================================================
	/// 그래프 해석. (팬 삼각화, 레이어 매핑, 계층과 소속, 스킨/클러스터, 애니메이션 채널)
	//==============================================================================
	#[test]
	fn scene_interpretation() {
		let scene_data = FbxLoader::new().parse(&build_test_fbx()).expect("scene");
		let geometry = scene_data.geometry_by_id.get(100).expect("geometry");
		assert_eq!(geometry.corner_count, 6);
		assert_eq!(geometry.original_vertex_count, 4);
		assert_eq!(geometry.original_vertex_indices, vec![0, 1, 2, 0, 2, 3]);
		assert_eq!(&geometry.positions[3..6], &[1.0, 0.0, 0.0]);
		assert_eq!(&geometry.texture_coordinates[10..12], &[0.0, 1.0]);
		assert_eq!(&geometry.normals[15..18], &[0.0, 0.0, 1.0]);

		assert_eq!(scene_data.node_list.len(), 2);
		assert_eq!(scene_data.root_node_indices, vec![0, 1]);
		assert!(scene_data.node_list[0].is_bone);
		assert_eq!(scene_data.node_list[0].translation, [1.0, 2.0, 3.0]);
		assert_eq!(scene_data.node_list[1].geometry_id, Some(100));
		assert_eq!(scene_data.node_list[1].skin_id, Some(300));
		let skin_deformer = scene_data.skin_deformer_by_id.get(300).expect("skin");
		assert_eq!(skin_deformer.cluster_ids, vec![301]);
		assert_eq!(skin_deformer.geometry_id, Some(100));
		assert_eq!(scene_data.cluster_by_id.get(301).and_then(|cluster| cluster.bone_node_id), Some(200));

		assert_eq!(scene_data.animation_list.len(), 1);
		let animation = &scene_data.animation_list[0];
		assert_eq!(animation.name, "Take\u{0}\u{1}AnimStack");
		assert_eq!(animation.duration, 1.0);
		assert_eq!(animation.channels.len(), 1);
		let channel = &animation.channels[0];
		assert_eq!(channel.node_index, 0);
		assert_eq!(channel.path, AnimationPath::Translation);
		assert_eq!(channel.times, vec![0.0, 1.0]);
		assert_eq!(channel.values, vec![0.0, 1.0, 0.0, 0.0, 3.0, 0.0]);
	}

	//==============================================================================
	/// FBX → 모델. (중립 메시 서술, 스킨 역바인드 = TransformLink 의 역, 애니메이션 샘플링, 이름 매칭 리타게팅)
	//==============================================================================
	#[test]
	fn fbx_to_skinned_model() {
		let scene_data = FbxLoader::new().parse(&build_test_fbx()).expect("scene");
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		let (mesh_description_list, material_description_list) = skinned_model.interpret_fbx_content(&scene_data).expect("interpret");
		assert!(material_description_list.is_empty());
		assert_eq!(mesh_description_list.len(), 1);
		let mesh_description = &mesh_description_list[0];
		assert_eq!(mesh_description.positions.len(), 18);
		assert_eq!(mesh_description.weights[0..4], [1.0, 0.0, 0.0, 0.0]);
		assert_eq!(mesh_description.node_index, Some(1));
		assert_eq!(mesh_description.material_index, None);

		// 0.5초: 본 이동 (0, 2, 0) → 조인트 행렬 = 이동(0, 2, 0) x 이동(-1, -2, -3) = 이동(-1, 0, -3).
		skinned_model.update(0.5);
		let joint_matrix_array = &skinned_model.get_skin_list()[0].joint_matrix_array;
		assert_near(joint_matrix_array[12], -1.0);
		assert_near(joint_matrix_array[13], 0.0);
		assert_near(joint_matrix_array[14], -3.0);

		// 같은 장면을 이름으로 붙이면 클립이 하나 더, 이동을 버리면 채널이 없다.
		skinned_model.add_animations_from_fbx_scene_data(&scene_data, "Moved", false);
		skinned_model.add_animations_from_fbx_scene_data(&scene_data, "Still", true);
		let animation_list = skinned_model.get_animation_list();
		assert_eq!(animation_list.len(), 3);
		assert_eq!(animation_list[1].channels.len(), 1);
		assert!(animation_list[2].channels.is_empty());
	}

	//==============================================================================
	/// 압축 풀기. (zlib 머리 deflate 왕복)
	//==============================================================================
	#[test]
	fn inflate_round_trip() {
		let original_bytes: Vec<u8> = (0..4096u32).map(|value| (value % 251) as u8).collect();
		let mut zlib_encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
		zlib_encoder.write_all(&original_bytes).expect("deflate");
		let compressed_bytes = zlib_encoder.finish().expect("deflate finish");
		assert_eq!(FbxLoader::inflate(&compressed_bytes, original_bytes.len()), Ok(original_bytes));
		assert!(FbxLoader::inflate(&[1, 2, 3], 3).is_err());
	}

	//==============================================================================
	/// URL 받기 손잡이. (엔진 애셋 폴더 기준. 없는 파일은 Err, 예제 walk.fbx 가 있으면 파싱 결과)
	//==============================================================================
	#[test]
	fn load_from_url_uses_engine_assets() {
		let mut engine_configuration = crate::core::engine::EngineConfiguration::new();
		engine_configuration.asset_directory_paths = vec![std::path::PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
		let engine = Engine::new(engine_configuration);
		let missing_load_handle = FbxLoader::load_from_url(&engine, "missing_model.fbx");
		assert!(missing_load_handle.is_finished());
		assert_eq!(missing_load_handle.get_scene_data().map(|scene_data_result| scene_data_result.err()), Some(Some(String::from("FbxLoader load failed: missing_model.fbx"))));
		if !std::path::Path::new(EXAMPLE_ASSET_DIRECTORY).join("anims").join("walk.fbx").exists() {
			eprintln!("walk.fbx 없음, 건너뜀");
			return;
		}
		let walk_load_handle = FbxLoader::load_from_url(&engine, "anims/walk.fbx");
		let scene_data = walk_load_handle.get_scene_data().expect("finished").expect("scene");
		assert!(!scene_data.animation_list.is_empty());
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		skinned_model.add_animations_from_fbx_url(&engine, "anims/walk.fbx", "Walk", true);
		assert!(skinned_model.get_animation_list().is_empty());
		skinned_model.update(0.0);
		assert!(skinned_model.get_animation_name_list().contains(&String::from("Walk")));

		// 원본 await addAnimationsFromFbxUrl: 손잡이를 await 하면 끝날 때 클립이 이미 더해져 있고, 다음 update 가 다시 더하지 않는다.
		let walk_clip_count = scene_data.animation_list.iter().filter(|fbx_animation| !fbx_animation.channels.is_empty()).count();
		let mut awaited_model = SkinnedModel::create_without_shader_program();
		let animation_load_handle = awaited_model.add_animations_from_fbx_url(&engine, "anims/walk.fbx", "Walk", true);
		assert!(awaited_model.get_animation_list().is_empty());
		let mut animation_load_future = std::future::IntoFuture::into_future(animation_load_handle.clone());
		let mut context = std::task::Context::from_waker(std::task::Waker::noop());
		assert_eq!(std::pin::Pin::new(&mut animation_load_future).poll(&mut context), std::task::Poll::Ready(Ok(())));
		assert!(animation_load_handle.is_finished());
		assert_eq!(awaited_model.get_animation_name_list().iter().filter(|name| name.as_str() == "Walk").count(), walk_clip_count);
		awaited_model.update(0.0);
		assert_eq!(awaited_model.get_animation_name_list().iter().filter(|name| name.as_str() == "Walk").count(), walk_clip_count);
	}

	//==============================================================================
	/// 실제 FBX. (원본 예제 anims/walk.fbx, Mixamo 7.7 형식, 압축 배열, 본 계층, 걷기 스택, 파일이 없으면 건너뛴다)
	//==============================================================================
	#[test]
	fn walk_fbx_from_examples() {
		let walk_path = std::path::Path::new(EXAMPLE_ASSET_DIRECTORY).join("anims").join("walk.fbx");
		let file_bytes = match std::fs::read(&walk_path) {
			Ok(file_bytes) => {
				file_bytes
			}
			Err(_) => {
				eprintln!("walk.fbx 없음, 건너뜀");
				return;
			}
		};
		assert_eq!(FbxLoader::read_version(&file_bytes), Ok(7700));
		let top_node_list = FbxLoader::new().parse_node_records(&file_bytes).expect("records");
		let top_node_names: Vec<&str> = top_node_list.iter().map(|node| node.name.as_str()).collect();
		assert!(top_node_names.contains(&"FBXHeaderExtension"));
		assert!(top_node_names.contains(&"Objects"));
		assert!(top_node_names.contains(&"Connections"));

		let scene_data = FbxLoader::new().interpret(&top_node_list).expect("scene");
		let hips_index = scene_data.node_list.iter().position(|node| node.name.starts_with("mixamorig:Hips")).expect("hips");
		assert!(scene_data.node_list[hips_index].is_bone);
		assert!(!scene_data.animation_list.is_empty());
		let walk_animation = scene_data.animation_list.iter().max_by_key(|animation| animation.channels.len()).expect("walk");
		assert!(walk_animation.channels.len() > 10);
		assert!(walk_animation.duration > 0.5 && walk_animation.duration < 10.0);
		assert!(walk_animation.channels.iter().any(|channel| channel.node_index == hips_index && channel.path == AnimationPath::Translation));

		// 걷기 장면을 모델로 해석하고, 같은 장면을 이동 없이 이름 매칭으로 붙인다.
		let mut skinned_model = SkinnedModel::create_without_shader_program();
		skinned_model.interpret_fbx_content(&scene_data).expect("interpret");
		let original_animation_count = skinned_model.get_animation_list().len();
		skinned_model.add_animations_from_fbx_scene_data(&scene_data, "WalkInPlace", true);
		{
			let animation_list = skinned_model.get_animation_list();
			assert!(animation_list.len() > original_animation_count);
			let added_clip = &animation_list[animation_list.len() - 1];
			assert_eq!(added_clip.name, "WalkInPlace");
			assert!(added_clip.channels.iter().all(|channel| channel.path != AnimationPath::Translation));
		}
		skinned_model.play_animation("WalkInPlace", 0.0);
		skinned_model.update(0.4);
		let hips_world_matrix = skinned_model.get_node_list()[hips_index].world_matrix;
		assert!(hips_world_matrix.elements.iter().all(|value| value.is_finite()));
	}
}
