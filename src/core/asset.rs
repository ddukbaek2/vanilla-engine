//==============================================================================
// 애셋.
// - 원본은 new ImageAsset() 뒤에 await asset.load(경로) 로 읽는다. 러스트는 씬 생성에서 기다릴 수 없어서
//   "읽기 요청 → 나중에 다 읽힘" 꼴로 바꿨다. 요청하면 AssetLoadHandle 을 받고, 바이트가 다 오면 상태가 loaded 가 된다.
//   데스크탑은 요청하는 자리에서 파일을 바로 읽어 끝내고, 웹은 fetch 가 끝난 뒤 다음 프레임들 사이에 끝난다.
// - 씬은 create 에서 애셋을 요청하고 scene_base.track_asset 으로 맡겨 두면, 맡긴 애셋이 다 읽힌 뒤 엔진이 start 를 부른다.
//   그동안은 원본처럼 draw_on_load 로 로딩 화면을 그린다.
// - 중앙 리소스 매니저는 원본처럼 두지 않는다. 애셋은 씬이 멤버로 든다.
//==============================================================================
use crate::core::engine::Engine;
use crate::core::image::Image;
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
// 애셋 타입. (원본 AssetType)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetType {
	None,
	Image,
	Audio,
	Json,
	Text,
	Font,
	Blob,
	Bunch,
	Visual,
}

//==============================================================================
// 읽기 상태.
//==============================================================================
#[derive(Clone, Debug)]
pub enum AssetLoadState {
	Pending,
	Loaded(Rc<Vec<u8>>),
	Failed(String),
}

//==============================================================================
// 읽기 손잡이. (요청한 쪽과 읽는 쪽이 나눠 든다)
//==============================================================================
#[derive(Clone)]
pub struct AssetLoadHandle {
	asset_path: String,
	load_state: Rc<RefCell<AssetLoadState>>,
	// 받은 바이트를 미리 풀어 둔 그림. (AssetLoader 가 다 받았을 때 풀어 보고 ImageAsset 이 그대로 쓴다)
	decoded_image: Rc<RefCell<Option<Rc<Image>>>>,
}

impl std::fmt::Debug for AssetLoadHandle {
	//==============================================================================
	// 디버그 출력. (경로와 읽기 상태, 풀어 둔 그림은 있는지만)
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("AssetLoadHandle").field("asset_path", &self.asset_path).field("load_state", &self.load_state).field("has_decoded_image", &self.decoded_image.borrow().is_some()).finish()
	}
}

impl AssetLoadHandle {
	//==============================================================================
	// 생성. (읽는 중)
	//==============================================================================
	pub fn create_pending(asset_path: &str) -> AssetLoadHandle {
		AssetLoadHandle {
			asset_path: asset_path.to_string(),
			load_state: Rc::new(RefCell::new(AssetLoadState::Pending)),
			decoded_image: Rc::new(RefCell::new(None)),
		}
	}

	//==============================================================================
	// 생성. (이미 가진 바이트로 바로 끝난 것)
	//==============================================================================
	pub fn create_loaded(asset_path: &str, bytes: Vec<u8>) -> AssetLoadHandle {
		AssetLoadHandle {
			asset_path: asset_path.to_string(),
			load_state: Rc::new(RefCell::new(AssetLoadState::Loaded(Rc::new(bytes)))),
			decoded_image: Rc::new(RefCell::new(None)),
		}
	}

	//==============================================================================
	// 요청 경로.
	//==============================================================================
	pub fn get_asset_path(&self) -> &str {
		&self.asset_path
	}

	//==============================================================================
	// 다 읽혔는지.
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		matches!(*self.load_state.borrow(), AssetLoadState::Loaded(_))
	}

	//==============================================================================
	// 실패했는지.
	//==============================================================================
	pub fn is_failed(&self) -> bool {
		matches!(*self.load_state.borrow(), AssetLoadState::Failed(_))
	}

	//==============================================================================
	// 끝났는지. (읽혔거나 실패했거나)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		!matches!(*self.load_state.borrow(), AssetLoadState::Pending)
	}

	//==============================================================================
	// 읽은 바이트. (다 읽혔을 때만)
	//==============================================================================
	pub fn get_bytes(&self) -> Option<Rc<Vec<u8>>> {
		match &*self.load_state.borrow() {
			AssetLoadState::Loaded(bytes) => {
				Some(bytes.clone())
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 실패 까닭.
	//==============================================================================
	pub fn get_error_message(&self) -> Option<String> {
		match &*self.load_state.borrow() {
			AssetLoadState::Failed(error_message) => {
				Some(error_message.clone())
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 미리 풀어 둔 그림 설정과 반환. (같은 손잡이를 나눠 든 ImageAsset 이 다시 풀지 않게)
	//==============================================================================
	pub fn set_decoded_image(&self, image: Rc<Image>) {
		*self.decoded_image.borrow_mut() = Some(image);
	}

	pub fn get_decoded_image(&self) -> Option<Rc<Image>> {
		self.decoded_image.borrow().clone()
	}

	//==============================================================================
	// 읽기 끝냄. (읽는 쪽이 부른다)
	//==============================================================================
	pub fn finish(&self, result: Result<Vec<u8>, String>) {
		let load_state = match result {
			Ok(bytes) => {
				AssetLoadState::Loaded(Rc::new(bytes))
			}
			Err(error_message) => {
				eprintln!("Asset: {}", error_message);
				AssetLoadState::Failed(error_message)
			}
		};
		*self.load_state.borrow_mut() = load_state;
	}
}

//==============================================================================
// 애셋 공통 상태. (원본 Asset 의 asset_path, asset_type)
//==============================================================================
#[derive(Clone, Debug)]
pub struct AssetBase {
	asset_type: AssetType,
	load_handle: Option<AssetLoadHandle>,
}

impl AssetBase {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(asset_type: AssetType) -> AssetBase {
		AssetBase {
			asset_type,
			load_handle: None,
		}
	}

	//==============================================================================
	// 애셋 타입.
	//==============================================================================
	pub fn get_asset_type(&self) -> AssetType {
		self.asset_type
	}

	//==============================================================================
	// 읽기 요청. (엔진에게 바이트를 부탁한다. 이미 요청했으면 다시 하지 않는다)
	//==============================================================================
	pub fn request_load(&mut self, engine: &Engine, asset_path: &str) {
		if self.load_handle.is_some() {
			return;
		}
		let load_handle = engine.request_asset_bytes(asset_path);
		self.load_handle = Some(load_handle);
	}

	//==============================================================================
	// 이미 가진 바이트로 읽기. (내장 자료나 직접 받은 것)
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.load_handle = Some(AssetLoadHandle::create_loaded(asset_path, bytes));
	}

	//==============================================================================
	// 남이 채울 읽기 손잡이로 읽기. (AssetLoader 가 차례를 기다리게 한 뒤 바이트를 채운다. 이미 요청했으면 바꾸지 않는다)
	//==============================================================================
	pub fn load_from_handle(&mut self, load_handle: AssetLoadHandle) {
		if self.load_handle.is_some() {
			return;
		}
		self.load_handle = Some(load_handle);
	}

	//==============================================================================
	// 내리기. (원본 unload)
	//==============================================================================
	pub fn unload(&mut self) {
		self.load_handle = None;
	}

	//==============================================================================
	// 읽기 손잡이.
	//==============================================================================
	pub fn get_load_handle(&self) -> Option<&AssetLoadHandle> {
		self.load_handle.as_ref()
	}

	//==============================================================================
	// 요청 경로. (요청 전이면 빈 글)
	//==============================================================================
	pub fn get_asset_path(&self) -> &str {
		match &self.load_handle {
			Some(load_handle) => {
				load_handle.get_asset_path()
			}
			None => {
				""
			}
		}
	}

	//==============================================================================
	// 바이트가 다 왔는지.
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		match &self.load_handle {
			Some(load_handle) => {
				load_handle.is_loaded()
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	// 읽은 바이트.
	//==============================================================================
	pub fn get_bytes(&self) -> Option<Rc<Vec<u8>>> {
		self.load_handle.as_ref().and_then(|load_handle| load_handle.get_bytes())
	}
}

//==============================================================================
// 애셋 트레이트. (씬의 track_asset 이 읽기 손잡이를 꺼낼 때 쓴다)
//==============================================================================
pub trait Asset {
	//==============================================================================
	// 공통 상태.
	//==============================================================================
	fn get_asset_base(&self) -> &AssetBase;

	//==============================================================================
	// 공통 상태. (변경용)
	//==============================================================================
	fn get_asset_base_mut(&mut self) -> &mut AssetBase;

	//==============================================================================
	// 다 읽혔는지.
	//==============================================================================
	fn is_loaded(&self) -> bool {
		self.get_asset_base().is_loaded()
	}

	//==============================================================================
	// 요청 경로.
	//==============================================================================
	fn get_asset_path(&self) -> &str {
		self.get_asset_base().get_asset_path()
	}
}
