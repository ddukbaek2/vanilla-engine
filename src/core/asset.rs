//==============================================================================
//! 애셋.
//! - 원본은 new ImageAsset() 뒤에 await asset.load(경로) 로 읽는다. 러스트 판도 load(engine, 경로) 가 읽기를 요청하고 읽기 손잡이(AssetLoadHandle)를
//!   돌려주며, 손잡이는 애셋을 빌리지 않는 나눠 든 상태라 엔진 작업 안에서 그대로 await 한다. (scene_context.with(|씬, 엔진| 씬.애셋.load(엔진, 경로)).await)
//!   바이트가 다 오면 상태가 loaded 가 되고, await 는 읽은 바이트나 실패 까닭(원본 reject 자리)으로 끝난다. 이미 load 한 애셋은 wait_for_load().await 로도 기다린다.
//!   그림 손잡이(ImageAsset::load)는 원본 img.onload 처럼 풀기까지 끝나야 끝나고, 풀지 못하면 실패로 끝난다.
//!   데스크탑은 요청하는 자리에서 파일을 바로 읽어 끝내고, 웹은 fetch 가 끝난 뒤 다음 프레임들 사이에 끝난다.
//! - await 없이 쓰는 꼴도 그대로 된다: 씬은 create 에서 애셋을 요청하고 scene_base.track_asset 으로 맡겨 두면, 맡긴 애셋이 다 읽힌 뒤 엔진이 initialize 를 부른다.
//!   (원본처럼 다 읽힌 뒤 0.3초 더 기다린다) 그동안은 원본처럼 draw_on_load 로 로딩 화면을 그린다.
//! - 중앙 리소스 매니저는 원본처럼 두지 않는다. 애셋은 씬이 멤버로 든다.
//==============================================================================
use crate::core::engine::Engine;
use crate::core::image::Image;
use std::cell::{Cell, RefCell};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

//==============================================================================
/// 애셋 타입. (원본 AssetType)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetType {
	/// 정하지 않음. (원본 none)
	None,
	/// 그림.
	Image,
	/// 소리.
	Audio,
	/// JSON.
	Json,
	/// 글.
	Text,
	/// 글꼴.
	Font,
	/// 바이트 덩어리.
	Blob,
	/// 여러 애셋 묶음.
	Bunch,
	/// 비주얼. (원본 visual)
	Visual,
}

//==============================================================================
/// 읽기 상태.
//==============================================================================
#[derive(Clone, Debug)]
pub enum AssetLoadState {
	/// 읽는 중.
	Pending,
	/// 다 읽음. (읽은 바이트)
	Loaded(Rc<Vec<u8>>),
	/// 실패. (까닭 글)
	Failed(String),
}

//==============================================================================
/// 읽기 손잡이. (요청한 쪽과 읽는 쪽이 나눠 든다)
/// - 복제하면 같은 읽기 상태를 나눠 든다. 읽는 쪽이 finish 로 끝내면 복제한 손잡이 모두가 끝난 것을 본다.
/// - 엔진 작업 안에서 손잡이를 그대로 await 하면 읽은 바이트나 실패 까닭으로 끝난다. (IntoFuture)
///
/// # 예제
/// 읽는 중인 손잡이를 나눠 들고 한쪽에서 끝내기
/// ```rust
/// use vanilla::AssetLoadHandle;
///
/// let load_handle = AssetLoadHandle::create_pending("level.json");
/// let shared_handle = load_handle.clone();
/// assert!(!load_handle.is_finished());
/// shared_handle.finish(Ok(b"{}".to_vec()));
/// assert!(load_handle.is_loaded());
/// assert!(load_handle.is_same_load(&shared_handle));
/// assert_eq!(load_handle.get_bytes().expect("bytes").as_slice(), b"{}");
/// assert_eq!(load_handle.get_asset_path(), "level.json");
/// ```
//==============================================================================
#[derive(Clone)]
pub struct AssetLoadHandle {
	asset_path: String,
	load_state: Rc<RefCell<AssetLoadState>>,
	/// 받은 바이트를 미리 풀어 둔 그림. (AssetLoader 가 다 받았을 때 풀어 보고 ImageAsset 이 그대로 쓴다)
	decoded_image: Rc<RefCell<Option<Rc<Image>>>>,
	/// 응답의 content-type. (웹 fetch 응답 머리, 데스크탑 파일과 머리가 없는 응답은 None)
	content_type: Rc<RefCell<Option<String>>>,
	/// 그림으로 풀어야 끝나는지. (ImageAsset 이 건다, await 가 다 받은 뒤 풀어 보고 풀지 못하면 실패로 끝낸다)
	is_image_decode_requested: Rc<Cell<bool>>,
	/// JSON 으로 풀려야 끝나는지. (JsonAsset 이 건다, await 가 다 받은 뒤 JSON 인지 보고 아니면 실패로 끝낸다)
	is_json_parse_requested: Rc<Cell<bool>>,
	/// 풀린 그림을 올릴 주인. (ImageAsset::load 가 엔진에 든 Graphic 의 GL 컨텍스트 번호를 건다, None 이면 모른다)
	image_owner: Rc<Cell<Option<usize>>>,
}

impl std::fmt::Debug for AssetLoadHandle {
	//==============================================================================
	/// 디버그 출력. (경로와 읽기 상태, 풀어 둔 그림은 있는지만)
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("AssetLoadHandle").field("asset_path", &self.asset_path).field("load_state", &self.load_state).field("has_decoded_image", &self.decoded_image.borrow().is_some()).finish()
	}
}

impl AssetLoadHandle {
	//==============================================================================
	/// 생성. (읽는 중)
	///
	/// # 인자
	/// - `asset_path`: 요청 경로. (손잡이에 적어 둔다)
	///
	/// # 돌려줌
	/// 읽는 중인 새 손잡이. (읽는 쪽이 finish 로 끝낸다)
	//==============================================================================
	pub fn create_pending(asset_path: &str) -> AssetLoadHandle {
		AssetLoadHandle {
			asset_path: asset_path.to_string(),
			load_state: Rc::new(RefCell::new(AssetLoadState::Pending)),
			decoded_image: Rc::new(RefCell::new(None)),
			content_type: Rc::new(RefCell::new(None)),
			is_image_decode_requested: Rc::new(Cell::new(false)),
			is_json_parse_requested: Rc::new(Cell::new(false)),
			image_owner: Rc::new(Cell::new(None)),
		}
	}

	//==============================================================================
	/// 생성. (이미 가진 바이트로 바로 끝난 것)
	///
	/// # 인자
	/// - `asset_path`: 요청 경로. (손잡이에 적어 둔다)
	/// - `bytes`: 이미 가진 바이트.
	///
	/// # 돌려줌
	/// 이미 다 읽힌 손잡이.
	//==============================================================================
	pub fn create_loaded(asset_path: &str, bytes: Vec<u8>) -> AssetLoadHandle {
		AssetLoadHandle {
			asset_path: asset_path.to_string(),
			load_state: Rc::new(RefCell::new(AssetLoadState::Loaded(Rc::new(bytes)))),
			decoded_image: Rc::new(RefCell::new(None)),
			content_type: Rc::new(RefCell::new(None)),
			is_image_decode_requested: Rc::new(Cell::new(false)),
			is_json_parse_requested: Rc::new(Cell::new(false)),
			image_owner: Rc::new(Cell::new(None)),
		}
	}

	//==============================================================================
	/// 요청 경로.
	///
	/// # 돌려줌
	/// 만들 때 준 경로.
	//==============================================================================
	pub fn get_asset_path(&self) -> &str {
		&self.asset_path
	}

	//==============================================================================
	/// 같은 읽기를 가리키는 손잡이인지. (복제한 손잡이끼리는 같다)
	///
	/// # 인자
	/// - `other`: 견줄 손잡이.
	///
	/// # 돌려줌
	/// 같은 읽기 상태를 나눠 들면 true.
	//==============================================================================
	pub fn is_same_load(&self, other: &AssetLoadHandle) -> bool {
		Rc::ptr_eq(&self.load_state, &other.load_state)
	}

	//==============================================================================
	/// 다 읽혔는지.
	///
	/// # 돌려줌
	/// 바이트가 다 왔으면 true.
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		matches!(*self.load_state.borrow(), AssetLoadState::Loaded(_))
	}

	//==============================================================================
	/// 실패했는지.
	///
	/// # 돌려줌
	/// 실패로 끝났으면 true.
	//==============================================================================
	pub fn is_failed(&self) -> bool {
		matches!(*self.load_state.borrow(), AssetLoadState::Failed(_))
	}

	//==============================================================================
	/// 끝났는지. (읽혔거나 실패했거나)
	///
	/// # 돌려줌
	/// 읽혔거나 실패했으면 true. 읽는 중이면 false.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		!matches!(*self.load_state.borrow(), AssetLoadState::Pending)
	}

	//==============================================================================
	/// 읽은 바이트. (다 읽혔을 때만)
	///
	/// # 돌려줌
	/// 읽은 바이트. (나눠 든 `Rc`) 읽는 중이거나 실패했으면 None.
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
	/// 실패 까닭.
	///
	/// # 돌려줌
	/// 실패 까닭. 실패하지 않았으면 None.
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
	/// 미리 풀어 둔 그림 설정과 반환. (같은 손잡이를 나눠 든 ImageAsset 이 다시 풀지 않게)
	/// - 아직 올리지 않은 그림으로 맡긴다. (씬 로딩 화면 동안 Graphic::prepare_loaded_images 가 올린다, 원본 DecodedImageStore)
	///
	/// # 인자
	/// - `image`: 받은 바이트를 풀어 둔 그림.
	//==============================================================================
	pub fn set_decoded_image(&self, image: Rc<Image>) {
		crate::core::decodedimagestore::store(&image, self.image_owner.get());
		*self.decoded_image.borrow_mut() = Some(image);
	}

	//==============================================================================
	/// 풀린 그림을 올릴 주인 걸기. (ImageAsset::load 가 엔진에 든 Graphic 의 것으로 건다)
	//==============================================================================
	pub(crate) fn set_image_owner(&self, owner: Option<usize>) {
		self.image_owner.set(owner);
	}

	//==============================================================================
	/// 미리 풀어 둔 그림 반환.
	///
	/// # 돌려줌
	/// 풀어 둔 그림. 없으면 None.
	//==============================================================================
	pub fn get_decoded_image(&self) -> Option<Rc<Image>> {
		self.decoded_image.borrow().clone()
	}

	//==============================================================================
	/// 응답의 content-type 설정과 반환. (웹 플랫폼이 fetch 응답 머리를 적는다, 원본 response.headers.get("content-type") 자리)
	///
	/// # 인자
	/// - `content_type`: 응답의 content-type. 없으면 None.
	//==============================================================================
	pub fn set_content_type(&self, content_type: Option<String>) {
		*self.content_type.borrow_mut() = content_type;
	}

	//==============================================================================
	/// 응답의 content-type 반환.
	///
	/// # 돌려줌
	/// 적어 둔 content-type. (데스크탑은 data: 주소면 그 형, 파일이면 None) 적지 않았으면 None.
	//==============================================================================
	pub fn get_content_type(&self) -> Option<String> {
		self.content_type.borrow().clone()
	}

	//==============================================================================
	/// 그림으로 풀어야 끝나는 손잡이로 하기. (ImageAsset 이 부른다, 원본 img.onload 처럼 await 가 풀기까지 기다린다)
	//==============================================================================
	pub fn request_image_decode(&self) {
		self.is_image_decode_requested.set(true);
	}

	//==============================================================================
	/// 그림으로 풀어야 끝나는 손잡이인지. (웹은 받은 응답의 형을 몰라도 이 손잡이면 브라우저가 내용으로 가려 푼다)
	//==============================================================================
	#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
	pub(crate) fn is_image_decode_requested(&self) -> bool {
		self.is_image_decode_requested.get()
	}

	//==============================================================================
	/// JSON 으로 풀려야 끝나는 손잡이로 하기. (JsonAsset 이 부른다, 원본 JsonAsset.load 처럼 받기나 해석이 실패하면 await 가 실패로 끝난다)
	//==============================================================================
	pub fn request_json_parse(&self) {
		self.is_json_parse_requested.set(true);
	}

	//==============================================================================
	/// 읽기 끝냄. (읽는 쪽이 부른다)
	///
	/// # 인자
	/// - `result`: 읽은 바이트, 실패면 Err(까닭). (실패면 표준 에러에 까닭을 찍는다)
	///
	/// # 예제
	/// 실패로 끝내면 까닭이 남는다
	/// ```rust
	/// use vanilla::AssetLoadHandle;
	///
	/// let load_handle = AssetLoadHandle::create_pending("missing.png");
	/// load_handle.finish(Err(String::from("not found")));
	/// assert!(load_handle.is_failed());
	/// assert!(load_handle.is_finished());
	/// assert_eq!(load_handle.get_error_message(), Some(String::from("not found")));
	/// assert!(load_handle.get_bytes().is_none());
	/// ```
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
/// 다 읽히기를 기다리는 퓨처. (읽은 바이트, 실패하면 까닭. 원본 await asset.load() 자리)
/// - 엔진 작업 안에서 await 한다. 아직이면 다음 프레임에 다시 본다.
/// - 보통은 손잡이를 그대로 await 하거나 Asset::wait_for_load 로 얻는다.
///
/// # 예제
/// 엔진 작업 안에서 읽기 손잡이 기다리기
/// ```rust
/// use vanilla::{AssetLoadFuture, AssetLoadHandle, Engine, EngineConfiguration};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let load_handle = AssetLoadHandle::create_loaded("greeting.txt", b"hello".to_vec());
/// let task_handle = engine.spawn_task(|_engine_context| async move {
/// 	let bytes = AssetLoadFuture::new(Some(load_handle)).await.expect("loaded");
/// 	assert_eq!(bytes.as_slice(), b"hello");
/// 	assert!(AssetLoadFuture::new(None).await.is_err());
/// });
/// assert!(task_handle.is_finished());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct AssetLoadFuture {
	load_handle: Option<AssetLoadHandle>,
}

impl AssetLoadFuture {
	//==============================================================================
	/// 생성. (손잡이가 없으면 요청 전이라 바로 실패로 끝난다)
	///
	/// # 인자
	/// - `load_handle`: 기다릴 읽기 손잡이. None 이면 처음 볼 때 Err 로 끝난다.
	///
	/// # 돌려줌
	/// 새 퓨처.
	//==============================================================================
	pub fn new(load_handle: Option<AssetLoadHandle>) -> AssetLoadFuture {
		AssetLoadFuture {
			load_handle,
		}
	}
}

impl Future for AssetLoadFuture {
	type Output = Result<Rc<Vec<u8>>, String>;

	//==============================================================================
	/// 끝났는지 보기. (읽혔으면 바이트, 실패했으면 까닭, 아직이면 스스로 깨워 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut AssetLoadFuture>, context: &mut Context<'_>) -> Poll<Result<Rc<Vec<u8>>, String>> {
		let load_handle = match &self.load_handle {
			Some(load_handle) => {
				load_handle
			}
			None => {
				return Poll::Ready(Err(String::from("Asset: 읽기를 요청하기 전에 기다렸습니다")));
			}
		};
		let load_state = load_handle.load_state.borrow().clone();
		match load_state {
			AssetLoadState::Pending => {
				context.waker().wake_by_ref();
				Poll::Pending
			}
			AssetLoadState::Loaded(bytes) => {
				// 그림 손잡이: 웹은 받을 때 브라우저가 풀어 두었고, 아니면 여기서 풀어 본다. (AssetLoader 의 그림과 같다)
				if load_handle.is_image_decode_requested.get() && load_handle.get_decoded_image().is_none() {
					match Image::load_from_memory(&bytes) {
						Ok(image) => {
							load_handle.set_decoded_image(image);
						}
						Err(error_message) => {
							let error_message = format!("그림을 풀지 못했습니다 ({}): {}", load_handle.get_asset_path(), error_message);
							load_handle.finish(Err(error_message.clone()));
							return Poll::Ready(Err(error_message));
						}
					}
				}
				// JSON 손잡이: 풀리지 않는 JSON 은 받기가 끝나도 실패다. (원본 JsonAsset.load 의 거부, 받은 글은 원본처럼 남긴다)
				if load_handle.is_json_parse_requested.get() {
					if let Err(error) = serde_json::from_slice::<serde_json::Value>(&bytes) {
						return Poll::Ready(Err(format!("Error loading json: {} ({})", load_handle.get_asset_path(), error)));
					}
				}
				Poll::Ready(Ok(bytes))
			}
			AssetLoadState::Failed(error_message) => {
				Poll::Ready(Err(error_message))
			}
		}
	}
}

impl IntoFuture for AssetLoadHandle {
	type Output = Result<Rc<Vec<u8>>, String>;
	type IntoFuture = AssetLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기. (다 읽힐 때까지)
	//==============================================================================
	fn into_future(self) -> AssetLoadFuture {
		AssetLoadFuture::new(Some(self))
	}
}

impl IntoFuture for &AssetLoadHandle {
	type Output = Result<Rc<Vec<u8>>, String>;
	type IntoFuture = AssetLoadFuture;

	//==============================================================================
	/// 빌린 손잡이를 await 하기. (복제해 기다린다)
	//==============================================================================
	fn into_future(self) -> AssetLoadFuture {
		AssetLoadFuture::new(Some(self.clone()))
	}
}

//==============================================================================
/// 애셋 공통 상태. (원본 Asset 의 asset_path, asset_type)
/// - 애셋 타입과 읽기 손잡이를 든다. 애셋 구조체가 필드로 들고 Asset 트레이트로 돌려준다.
///
/// # 예제
/// 가진 바이트로 읽고 내리기
/// ```rust
/// use vanilla::{AssetBase, AssetType};
///
/// let mut asset_base = AssetBase::new(AssetType::Text);
/// assert!(!asset_base.is_loaded());
/// assert_eq!(asset_base.get_asset_path(), "");
/// asset_base.load_from_bytes("note.txt", b"hello".to_vec());
/// assert!(asset_base.is_loaded());
/// assert_eq!(asset_base.get_asset_path(), "note.txt");
/// assert_eq!(asset_base.get_asset_type(), AssetType::Text);
/// asset_base.unload();
/// assert!(asset_base.get_load_handle().is_none());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct AssetBase {
	asset_type: AssetType,
	load_handle: Option<AssetLoadHandle>,
}

impl AssetBase {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `asset_type`: 애셋 타입.
	///
	/// # 돌려줌
	/// 읽기를 요청하기 전의 공통 상태.
	//==============================================================================
	pub fn new(asset_type: AssetType) -> AssetBase {
		AssetBase {
			asset_type,
			load_handle: None,
		}
	}

	//==============================================================================
	/// 애셋 타입.
	///
	/// # 돌려줌
	/// 만들 때 준 애셋 타입.
	//==============================================================================
	pub fn get_asset_type(&self) -> AssetType {
		self.asset_type
	}

	//==============================================================================
	/// 읽기 요청. (엔진에게 바이트를 부탁하고 읽기 손잡이를 돌려준다. 이미 요청했으면 다시 하지 않고 그 손잡이를 돌려준다)
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `asset_path`: 애셋 경로. (데스크탑은 애셋 폴더 기준, 웹은 asset_base_url 기준, data: 주소도 된다)
	///
	/// # 돌려줌
	/// 읽기 손잡이. (await 하면 읽은 바이트나 실패 까닭으로 끝난다)
	///
	/// # 예제
	/// data: 주소는 데스크탑에서 그 자리에서 읽히고, 다시 요청하면 처음 손잡이를 돌려준다
	/// ```rust
	/// use vanilla::{AssetBase, AssetType, Engine, EngineConfiguration};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut asset_base = AssetBase::new(AssetType::Text);
	/// let load_handle = asset_base.request_load(&engine, "data:,hello");
	/// assert!(load_handle.is_loaded());
	/// let again_handle = asset_base.request_load(&engine, "data:,other");
	/// assert!(again_handle.is_same_load(&load_handle));
	/// assert_eq!(asset_base.get_bytes().expect("bytes").as_slice(), b"hello");
	/// ```
	//==============================================================================
	pub fn request_load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		self.request_load_with_cache(engine, asset_path, false)
	}

	//==============================================================================
	/// 읽기 요청. (is_no_cache 면 웹은 브라우저 캐시를 쓰지 않는다, 이미 요청했으면 다시 하지 않고 그 손잡이를 돌려준다)
	///
	/// # 인자
	/// - `engine`: 바이트를 부탁할 엔진.
	/// - `asset_path`: 애셋 경로. (데스크탑은 애셋 폴더 기준, 웹은 asset_base_url 기준, data: 주소도 된다)
	/// - `is_no_cache`: true 면 웹에서 브라우저 캐시를 쓰지 않는다. (데스크탑은 늘 파일을 새로 읽어 켠 것과 같다)
	///
	/// # 돌려줌
	/// 읽기 손잡이.
	//==============================================================================
	pub fn request_load_with_cache(&mut self, engine: &Engine, asset_path: &str, is_no_cache: bool) -> AssetLoadHandle {
		if let Some(load_handle) = &self.load_handle {
			return load_handle.clone();
		}
		let load_handle = engine.request_asset_bytes_with_cache(asset_path, is_no_cache);
		self.load_handle = Some(load_handle.clone());
		load_handle
	}

	//==============================================================================
	/// 이미 가진 바이트로 읽기. (내장 자료나 직접 받은 것, 읽기 손잡이를 돌려준다)
	/// - 이미 요청한 손잡이가 있어도 새 손잡이로 바꾼다.
	///
	/// # 인자
	/// - `asset_path`: 적어 둘 경로.
	/// - `bytes`: 애셋 바이트.
	///
	/// # 돌려줌
	/// 이미 다 읽힌 읽기 손잡이.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) -> AssetLoadHandle {
		let load_handle = AssetLoadHandle::create_loaded(asset_path, bytes);
		self.load_handle = Some(load_handle.clone());
		load_handle
	}

	//==============================================================================
	/// 남이 채울 읽기 손잡이로 읽기. (AssetLoader 가 차례를 기다리게 한 뒤 바이트를 채운다. 이미 요청했으면 바꾸지 않는다)
	///
	/// # 인자
	/// - `load_handle`: 남이 채울 읽기 손잡이.
	//==============================================================================
	pub fn load_from_handle(&mut self, load_handle: AssetLoadHandle) {
		if self.load_handle.is_some() {
			return;
		}
		self.load_handle = Some(load_handle);
	}

	//==============================================================================
	/// 내리기. (원본 unload)
	//==============================================================================
	pub fn unload(&mut self) {
		self.load_handle = None;
	}

	//==============================================================================
	/// 읽기 손잡이.
	///
	/// # 돌려줌
	/// 읽기 손잡이. 요청 전이거나 내렸으면 None.
	//==============================================================================
	pub fn get_load_handle(&self) -> Option<&AssetLoadHandle> {
		self.load_handle.as_ref()
	}

	//==============================================================================
	/// 요청 경로. (요청 전이면 빈 글)
	///
	/// # 돌려줌
	/// 요청 경로. 요청 전이면 빈 글.
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
	/// 바이트가 다 왔는지.
	///
	/// # 돌려줌
	/// 바이트가 다 왔으면 true. 요청 전이면 false.
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
	/// 읽은 바이트.
	///
	/// # 돌려줌
	/// 읽은 바이트. 요청 전이거나 읽는 중이거나 실패했으면 None.
	//==============================================================================
	pub fn get_bytes(&self) -> Option<Rc<Vec<u8>>> {
		self.load_handle.as_ref().and_then(|load_handle| load_handle.get_bytes())
	}
}

//==============================================================================
/// 애셋 트레이트. (씬의 track_asset 이 읽기 손잡이를 꺼낼 때 쓴다)
/// - 구현체는 AssetBase 를 들고 get_asset_base, get_asset_base_mut 만 채운다. 나머지는 AssetBase 를 읽는 기본 구현이다.
///
/// # 예제
/// 직접 만든 애셋 타입
/// ```rust
/// use vanilla::{Asset, AssetBase, AssetType};
///
/// struct NoteAsset {
/// 	asset_base: AssetBase,
/// }
///
/// impl Asset for NoteAsset {
/// 	fn get_asset_base(&self) -> &AssetBase {
/// 		&self.asset_base
/// 	}
///
/// 	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
/// 		&mut self.asset_base
/// 	}
/// }
///
/// let mut note_asset = NoteAsset {
/// 	asset_base: AssetBase::new(AssetType::Text),
/// };
/// note_asset.get_asset_base_mut().load_from_bytes("note.txt", b"hello".to_vec());
/// assert!(note_asset.is_loaded());
/// assert_eq!(note_asset.get_asset_path(), "note.txt");
/// ```
//==============================================================================
pub trait Asset {
	//==============================================================================
	/// 공통 상태.
	///
	/// # 돌려줌
	/// 구현체가 든 AssetBase.
	//==============================================================================
	fn get_asset_base(&self) -> &AssetBase;

	//==============================================================================
	/// 공통 상태. (변경용)
	///
	/// # 돌려줌
	/// 구현체가 든 AssetBase. (변경용)
	//==============================================================================
	fn get_asset_base_mut(&mut self) -> &mut AssetBase;

	//==============================================================================
	/// 다 읽혔는지.
	///
	/// # 돌려줌
	/// 바이트가 다 왔으면 true.
	//==============================================================================
	fn is_loaded(&self) -> bool {
		self.get_asset_base().is_loaded()
	}

	//==============================================================================
	/// 요청 경로.
	///
	/// # 돌려줌
	/// 요청 경로. 요청 전이면 빈 글.
	//==============================================================================
	fn get_asset_path(&self) -> &str {
		self.get_asset_base().get_asset_path()
	}

	//==============================================================================
	/// 다 읽히기를 기다리는 퓨처. (load 가 돌려준 손잡이를 await 하는 것과 같다, 이미 load 한 애셋을 뒤에 엔진 작업 안에서 기다릴 때 쓴다)
	/// - 퓨처는 애셋을 빌리지 않아서 씬이 든 애셋에도 쓸 수 있다. 요청 전이면 바로 실패로 끝난다.
	/// - 끝나면 읽은 바이트, 실패하면 까닭이다. (원본은 거부된다) 그림 풀기와 JSON 풀기는 애셋의 get_image, get_data 가 한다.
	///
	/// # 돌려줌
	/// 다 읽히면 읽은 바이트로, 실패하거나 요청 전이면 까닭으로 끝나는 퓨처.
	///
	/// # 예제
	/// 가진 바이트로 읽은 글 애셋을 엔진 작업에서 기다리기
	/// ```rust
	/// use vanilla::{Asset, Engine, EngineConfiguration, TextAsset};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut text_asset = TextAsset::new();
	/// text_asset.load_from_bytes("note.txt", b"hello".to_vec());
	/// let load_future = text_asset.wait_for_load();
	/// let empty_future = TextAsset::new().wait_for_load();
	/// let task_handle = engine.spawn_task(|_engine_context| async move {
	/// 	let bytes = load_future.await.expect("loaded");
	/// 	assert_eq!(bytes.as_slice(), b"hello");
	/// 	assert!(empty_future.await.is_err());
	/// });
	/// assert!(task_handle.is_finished());
	/// ```
	//==============================================================================
	fn wait_for_load(&self) -> AssetLoadFuture {
		AssetLoadFuture::new(self.get_asset_base().get_load_handle().cloned())
	}
}
