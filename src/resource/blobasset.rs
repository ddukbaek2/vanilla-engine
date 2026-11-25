//==============================================================================
//! 바이너리 애셋. (원본 resource/blobasset.js)
//! - 원본은 fetch 로 받은 Blob 을 든다. 러스트 판은 바이트를 그대로 든다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use std::rc::Rc;

//==============================================================================
/// 바이너리 애셋.
/// - 받은 바이트를 풀지 않고 그대로 든다. (애셋 타입 blob)
///
/// # 예제
/// 가진 바이트로 읽고 꺼내기
/// ```rust
/// use vanilla::{Asset, BlobAsset};
///
/// let mut blob_asset = BlobAsset::new();
/// assert!(blob_asset.get_blob().is_none());
/// blob_asset.load_from_bytes("data.bin", vec![1, 2, 3]);
/// assert!(blob_asset.is_loaded());
/// assert_eq!(blob_asset.get_blob().expect("bytes").as_slice(), &[1, 2, 3]);
/// blob_asset.unload();
/// assert!(!blob_asset.is_loaded());
/// ```
//==============================================================================
pub struct BlobAsset {
	asset_base: AssetBase,
}

impl BlobAsset {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 요청 전의 빈 바이너리 애셋.
	//==============================================================================
	pub fn new() -> BlobAsset {
		BlobAsset {
			asset_base: AssetBase::new(AssetType::Blob),
		}
	}

	//==============================================================================
	/// 읽기 요청. (원본 load, await 할 수 있는 읽기 손잡이를 돌려준다)
	/// - 이미 요청했으면 다시 하지 않고 처음 손잡이를 돌려준다.
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `asset_path`: 애셋 경로. (데스크탑은 애셋 폴더 기준, 웹은 asset_base_url 기준, data: 주소도 된다)
	///
	/// # 돌려줌
	/// 읽기 손잡이. (데스크탑은 그 자리에서 읽혀 끝나 있다. 못 읽으면 실패로 끝난다)
	///
	/// # 예제
	/// data: 주소를 데스크탑에서 그 자리에서 읽기
	/// ```rust
	/// use vanilla::{BlobAsset, Engine, EngineConfiguration};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut blob_asset = BlobAsset::new();
	/// let load_handle = blob_asset.load(&engine, "data:,abc");
	/// assert!(load_handle.is_loaded());
	/// assert_eq!(blob_asset.get_blob().expect("bytes").as_slice(), b"abc");
	/// ```
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		self.asset_base.request_load(engine, asset_path)
	}

	//==============================================================================
	/// 이미 가진 바이트로 읽기.
	/// - 이미 요청한 손잡이가 있어도 새 손잡이로 바꾼다.
	///
	/// # 인자
	/// - `asset_path`: 적어 둘 경로.
	/// - `bytes`: 애셋 바이트.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	/// 내리기.
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
	}

	//==============================================================================
	/// 바이트. (원본 get_blob. 다 읽히기 전이면 None)
	///
	/// # 돌려줌
	/// 읽은 바이트. 요청 전이거나 읽는 중이거나 실패했으면 None.
	//==============================================================================
	pub fn get_blob(&self) -> Option<Rc<Vec<u8>>> {
		self.asset_base.get_bytes()
	}
}

impl Default for BlobAsset {
	//==============================================================================
	/// 요청 전의 빈 바이너리 애셋. (new 와 같다)
	//==============================================================================
	fn default() -> BlobAsset {
		BlobAsset::new()
	}
}

impl Asset for BlobAsset {
	//==============================================================================
	/// 공통 상태.
	//==============================================================================
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	//==============================================================================
	/// 공통 상태. (변경용)
	//==============================================================================
	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}
