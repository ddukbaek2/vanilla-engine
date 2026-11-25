//==============================================================================
// 애셋 함께 받기. (원본 resource/assetloader.js)
// - 그림, JSON, 글, 바이너리, 소리, 글꼴을 목록에 넣고 동시에 받는 수(set_concurrency)만큼씩 받는다. 실패한 것도 끝난 것으로 쳐서 멈추지 않는다.
// - 원본은 await load() 로 기다린다. 러스트 판은 엔진의 "요청 → 나중에 다 읽힘" 꼴을 따른다:
//     add_image 같은 함수가 아직 비어 있는 읽기 손잡이를 단 애셋을 바로 돌려주고, load(engine) 가 첫 몫을 요청한다.
//     그 뒤로는 update(engine) 를 프레임마다 불러야 앞의 것이 끝난 만큼 다음 것을 요청한다. (데스크탑은 그 자리에서 읽혀 load 에서 다 끝난다)
//     씬은 get_completion_handle() 을 scene_base.track_load_handle 로 맡겨 다 받을 때까지 initialize 를 미룬다.
//     GameSceneBase 는 track_asset_loader 하나로 맡기고, draw_on_load 가 update 를 부르고 진행률을 로딩 화면에 넘긴다.
// - 손잡이를 나눠 드는 값싼 복제 꼴이다. (Clone 하면 같은 목록을 가리킨다)
// - 진행률은 끝난 수 / 전체 수. 실패는 get_failed_list() 에 AssetLoadFailure 로 남는다. 바뀌면 set_progress_handler, 다 끝나면 set_complete_handler 의 함수를 부른다.
// - 원본과 다른 점:
//   글꼴은 FontAsset 대신 읽기 손잡이를 돌려준다. (러스트 판 글꼴은 엔진이 이름으로 등록하고, FontAsset 은 손잡이를 든 껍데기라서)
//   소리는 Rc<AudioAsset> 을 돌려주고, 바이트를 받은 뒤 디코딩까지 끝나야 끝난 것으로 친다.
//   원본 addTask(직접 만든 받기) 자리는 add_load_handle 이다. 직접 요청한 읽기 손잡이를 넣으면 진행률에 같이 센다. (동시 수에는 넣지 않는다)
//   그림은 원본 img.onload 처럼 다 받았을 때 풀어 본다. 풀지 못하는 것(깨진 PNG)은 실패 목록에 들고, 푼 그림은 손잡이에 담아 ImageAsset 이 다시 풀지 않는다.
//   원본 load() 는 부른 때의 목록 수로 일꾼을 띄워 그 뒤에 더한 것을 일꾼 수만큼 함께 받지 못하는데(원본 버그), 러스트 판은 update 마다 빈 자리를 채운다.
//==============================================================================
use crate::core::asset::{Asset, AssetLoadHandle};
use crate::core::engine::Engine;
use crate::core::image::Image;
use crate::resource::audioasset::AudioAsset;
use crate::resource::blobasset::BlobAsset;
use crate::resource::imageasset::ImageAsset;
use crate::resource::textasset::{JsonAsset, TextAsset};
use std::cell::RefCell;
use std::rc::Rc;

// 함께 받는 수의 기본값. (원본과 같음)
const DEFAULT_CONCURRENCY: usize = 6;

// 다 받음 손잡이의 경로. (애셋이 아닌 대기 표시)
const COMPLETION_HANDLE_PATH: &str = "AssetLoader.completion";

//==============================================================================
// 받지 못한 것 하나. (원본 { path, error })
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetLoadFailure {
	pub path: String,
	pub error_message: String,
}

// 진행 알림 함수. (진행률, 끝난 수, 전체 수)
pub type AssetLoaderProgressHandler = Box<dyn FnMut(f32, usize, usize)>;

// 다 받음 알림 함수. (실패 목록)
pub type AssetLoaderCompleteHandler = Box<dyn FnMut(&[AssetLoadFailure])>;

//==============================================================================
// 목록 한 칸의 종류.
//==============================================================================
enum AssetLoaderEntryKind {
	// 바이트를 받으면 끝나는 것. (글, JSON, 바이너리, 글꼴)
	Bytes,
	// 바이트를 받은 뒤 풀어 보기까지. (is_decode_checked: 풀어 보았는지)
	Image {
		is_decode_checked: bool,
	},
	// 바이트를 받은 뒤 디코딩까지. (is_handed_over: 바이트를 애셋에 넘겼는지)
	Audio {
		audio_asset: Rc<AudioAsset>,
		is_handed_over: bool,
	},
	// 바깥에서 요청한 손잡이. (바로 시작한 것으로 친다)
	External,
}

//==============================================================================
// 목록 한 칸.
//==============================================================================
struct AssetLoaderEntry {
	path: String,
	load_handle: AssetLoadHandle,
	kind: AssetLoaderEntryKind,
	is_started: bool,
	is_counted: bool,
}

impl AssetLoaderEntry {
	//==============================================================================
	// 바이트 요청이 끝났는지. (동시 수를 셀 때 쓴다)
	//==============================================================================
	fn is_request_finished(&self) -> bool {
		self.load_handle.is_finished()
	}

	//==============================================================================
	// 다 끝났는지. (소리는 디코딩까지)
	//==============================================================================
	fn is_finished(&self) -> bool {
		match &self.kind {
			AssetLoaderEntryKind::Audio { audio_asset, is_handed_over } => {
				if !self.load_handle.is_finished() {
					return false;
				}
				if self.load_handle.is_failed() {
					return true;
				}
				*is_handed_over && (audio_asset.is_loaded() || audio_asset.get_load_error().is_some())
			}
			AssetLoaderEntryKind::Image { is_decode_checked } => {
				self.load_handle.is_failed() || (self.load_handle.is_loaded() && *is_decode_checked)
			}
			_ => {
				self.load_handle.is_finished()
			}
		}
	}

	//==============================================================================
	// 실패 까닭. (실패가 아니면 None)
	//==============================================================================
	fn get_error_message(&self) -> Option<String> {
		if let Some(error_message) = self.load_handle.get_error_message() {
			return Some(error_message);
		}
		match &self.kind {
			AssetLoaderEntryKind::Audio { audio_asset, .. } => {
				audio_asset.get_load_error()
			}
			_ => {
				None
			}
		}
	}
}

//==============================================================================
// 함께 받기 상태.
//==============================================================================
struct AssetLoaderState {
	concurrency: usize,
	entry_list: Vec<AssetLoaderEntry>,
	finished_count: usize,
	loaded_count: usize,
	failed_list: Vec<AssetLoadFailure>,
	completion_handle: AssetLoadHandle,
	is_loading: bool,
	progress_handler: Option<AssetLoaderProgressHandler>,
	complete_handler: Option<AssetLoaderCompleteHandler>,
}

//==============================================================================
// 애셋 함께 받기.
//==============================================================================
#[derive(Clone)]
pub struct AssetLoader {
	state: Rc<RefCell<AssetLoaderState>>,
}

impl AssetLoader {
	//==============================================================================
	// 생성. (동시에 받는 수 6)
	//==============================================================================
	pub fn new() -> AssetLoader {
		AssetLoader {
			state: Rc::new(RefCell::new(AssetLoaderState {
				concurrency: DEFAULT_CONCURRENCY,
				entry_list: Vec::new(),
				finished_count: 0,
				loaded_count: 0,
				failed_list: Vec::new(),
				completion_handle: AssetLoadHandle::create_pending(COMPLETION_HANDLE_PATH),
				is_loading: false,
				progress_handler: None,
				complete_handler: None,
			})),
		}
	}

	//==============================================================================
	// 동시에 받는 수 설정. (1 이상)
	//==============================================================================
	pub fn set_concurrency(&self, concurrency: usize) {
		self.state.borrow_mut().concurrency = concurrency.max(1);
	}

	//==============================================================================
	// 동시에 받는 수 반환.
	//==============================================================================
	pub fn get_concurrency(&self) -> usize {
		self.state.borrow().concurrency
	}

	//==============================================================================
	// 목록에 넣기. (비어 있는 읽기 손잡이를 만들어 돌려준다)
	//==============================================================================
	fn add_entry(&self, path: &str, kind: AssetLoaderEntryKind) -> AssetLoadHandle {
		let load_handle = AssetLoadHandle::create_pending(path);
		self.state.borrow_mut().entry_list.push(AssetLoaderEntry {
			path: path.to_string(),
			load_handle: load_handle.clone(),
			kind,
			is_started: false,
			is_counted: false,
		});
		load_handle
	}

	//==============================================================================
	// 그림 넣기. (받으면 바이트가 차는 애셋, 처음 get_image 할 때 푼다)
	//==============================================================================
	pub fn add_image(&self, path: &str) -> ImageAsset {
		let load_handle = self.add_entry(path, AssetLoaderEntryKind::Image { is_decode_checked: false });
		let mut image_asset = ImageAsset::new();
		image_asset.get_asset_base_mut().load_from_handle(load_handle);
		image_asset
	}

	//==============================================================================
	// JSON 넣기.
	//==============================================================================
	pub fn add_json(&self, path: &str) -> JsonAsset {
		let load_handle = self.add_entry(path, AssetLoaderEntryKind::Bytes);
		let mut json_asset = JsonAsset::new();
		json_asset.get_asset_base_mut().load_from_handle(load_handle);
		json_asset
	}

	//==============================================================================
	// 글 넣기.
	//==============================================================================
	pub fn add_text(&self, path: &str) -> TextAsset {
		let load_handle = self.add_entry(path, AssetLoaderEntryKind::Bytes);
		let mut text_asset = TextAsset::new();
		text_asset.get_asset_base_mut().load_from_handle(load_handle);
		text_asset
	}

	//==============================================================================
	// 바이너리 넣기.
	//==============================================================================
	pub fn add_blob(&self, path: &str) -> BlobAsset {
		let load_handle = self.add_entry(path, AssetLoaderEntryKind::Bytes);
		let mut blob_asset = BlobAsset::new();
		blob_asset.get_asset_base_mut().load_from_handle(load_handle);
		blob_asset
	}

	//==============================================================================
	// 글꼴 넣기. (받으면 엔진이 다음 프레임 시작에 family 이름으로 등록한다. 읽기 손잡이를 돌려준다)
	//==============================================================================
	pub fn add_font(&self, engine: &mut Engine, family: &str, path: &str, is_bold: bool) -> AssetLoadHandle {
		let load_handle = self.add_entry(path, AssetLoaderEntryKind::Bytes);
		engine.register_font_when_loaded(family, is_bold, load_handle.clone());
		load_handle
	}

	//==============================================================================
	// 소리 넣기. (바이트를 받으면 디코딩한다. 데스크탑은 그 자리에서, 웹은 뒤에서)
	//==============================================================================
	pub fn add_audio(&self, path: &str) -> Rc<AudioAsset> {
		let audio_asset = Rc::new(AudioAsset::new());
		self.add_entry(path, AssetLoaderEntryKind::Audio {
			audio_asset: audio_asset.clone(),
			is_handed_over: false,
		});
		audio_asset
	}

	//==============================================================================
	// 바깥에서 요청한 읽기 손잡이 넣기. (원본 addTask 자리, 진행률에 같이 센다)
	//==============================================================================
	pub fn add_load_handle(&self, load_handle: &AssetLoadHandle) {
		self.state.borrow_mut().entry_list.push(AssetLoaderEntry {
			path: load_handle.get_asset_path().to_string(),
			load_handle: load_handle.clone(),
			kind: AssetLoaderEntryKind::External,
			is_started: true,
			is_counted: false,
		});
	}

	//==============================================================================
	// 받기 시작. (첫 몫을 요청한다. 데스크탑은 여기서 다 끝난다. 그 뒤로는 update 를 프레임마다 부른다)
	//==============================================================================
	pub fn load(&self, engine: &Engine) {
		self.state.borrow_mut().is_loading = true;
		self.update(engine);
	}

	//==============================================================================
	// 갱신. (끝난 소리 바이트를 디코딩에 넘기고, 빈 자리만큼 다음 것을 요청하고, 진행과 다 받음을 알린다. load 전이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn update(&self, engine: &Engine) {
		if !self.state.borrow().is_loading {
			return;
		}
		loop {
			let is_started = self.start_next_entries(engine);
			self.hand_over_audio_bytes();
			self.decode_loaded_images();
			if !is_started {
				break;
			}
		}
		self.count_finished_entries();
	}

	//==============================================================================
	// 빈 자리만큼 요청하기. (하나라도 요청했으면 true)
	//==============================================================================
	fn start_next_entries(&self, engine: &Engine) -> bool {
		let mut state = self.state.borrow_mut();
		let concurrency = state.concurrency;
		let mut in_flight_count = state.entry_list.iter().filter(|entry| entry.is_started && !entry.is_request_finished() && !matches!(entry.kind, AssetLoaderEntryKind::External)).count();
		let mut is_started = false;
		for entry in state.entry_list.iter_mut() {
			if in_flight_count >= concurrency {
				break;
			}
			if entry.is_started {
				continue;
			}
			entry.is_started = true;
			is_started = true;
			engine.request_asset_bytes_into(&entry.path, &entry.load_handle);
			if !entry.is_request_finished() {
				in_flight_count += 1;
			}
		}
		is_started
	}

	//==============================================================================
	// 받은 소리 바이트를 애셋에 넘기기. (디코딩은 애셋이 한다)
	//==============================================================================
	fn hand_over_audio_bytes(&self) {
		let mut state = self.state.borrow_mut();
		for entry in state.entry_list.iter_mut() {
			let load_handle = entry.load_handle.clone();
			if let AssetLoaderEntryKind::Audio { audio_asset, is_handed_over } = &mut entry.kind {
				if *is_handed_over || !load_handle.is_loaded() {
					continue;
				}
				*is_handed_over = true;
				let bytes = match load_handle.get_bytes() {
					Some(bytes) => {
						bytes
					}
					None => {
						continue;
					}
				};
				let byte_vector = (*bytes).clone();
				if let Err(error_message) = audio_asset.load_from_bytes(byte_vector) {
					audio_asset.set_load_error(error_message);
				}
			}
		}
	}

	//==============================================================================
	// 받은 그림 풀어 보기. (풀면 손잡이에 담고, 풀지 못하면 손잡이를 실패로 바꾼다)
	//==============================================================================
	fn decode_loaded_images(&self) {
		let mut state = self.state.borrow_mut();
		for entry in state.entry_list.iter_mut() {
			let load_handle = entry.load_handle.clone();
			if let AssetLoaderEntryKind::Image { is_decode_checked } = &mut entry.kind {
				if *is_decode_checked || !load_handle.is_loaded() {
					continue;
				}
				*is_decode_checked = true;
				// 웹은 받을 때 브라우저가 이미 풀어 두었다.
				if load_handle.get_decoded_image().is_some() {
					continue;
				}
				let bytes = match load_handle.get_bytes() {
					Some(bytes) => {
						bytes
					}
					None => {
						continue;
					}
				};
				match Image::load_from_memory(&bytes) {
					Ok(image) => {
						load_handle.set_decoded_image(image);
					}
					Err(error_message) => {
						load_handle.finish(Err(format!("그림을 풀지 못했습니다 ({}): {}", entry.path, error_message)));
					}
				}
			}
		}
	}

	//==============================================================================
	// 끝난 것 세기. (새로 끝난 것이 있으면 진행을 알리고, 다 끝났으면 다 받음 손잡이를 끝내고 알린다)
	//==============================================================================
	fn count_finished_entries(&self) {
		let mut is_changed = false;
		{
			let mut state = self.state.borrow_mut();
			let mut new_failure_list: Vec<AssetLoadFailure> = Vec::new();
			let mut newly_finished_count = 0;
			let mut newly_loaded_count = 0;
			for entry in state.entry_list.iter_mut() {
				if entry.is_counted || !entry.is_finished() {
					continue;
				}
				entry.is_counted = true;
				newly_finished_count += 1;
				match entry.get_error_message() {
					Some(error_message) => {
						eprintln!("AssetLoader: 받지 못했습니다 {} ({})", entry.path, error_message);
						new_failure_list.push(AssetLoadFailure {
							path: entry.path.clone(),
							error_message,
						});
					}
					None => {
						newly_loaded_count += 1;
					}
				}
			}
			if newly_finished_count > 0 {
				is_changed = true;
				state.finished_count += newly_finished_count;
				state.loaded_count += newly_loaded_count;
				state.failed_list.extend(new_failure_list);
			}
		}
		if is_changed {
			self.notify_progress();
		}
		let is_newly_completed = {
			let state = self.state.borrow();
			state.finished_count >= state.entry_list.len() && !state.completion_handle.is_finished()
		};
		if is_newly_completed {
			let completion_handle = self.state.borrow().completion_handle.clone();
			completion_handle.finish(Ok(Vec::new()));
			self.notify_complete();
		}
	}

	//==============================================================================
	// 진행 알리기. (알림 함수는 부르는 동안 꺼내 두어 그 안에서 이 객체를 불러도 된다)
	//==============================================================================
	fn notify_progress(&self) {
		let progress_handler = self.state.borrow_mut().progress_handler.take();
		if let Some(mut progress_handler) = progress_handler {
			let progress = self.get_progress();
			let finished_count = self.get_finished_count();
			let total_count = self.get_total_count();
			progress_handler(progress, finished_count, total_count);
			let mut state = self.state.borrow_mut();
			if state.progress_handler.is_none() {
				state.progress_handler = Some(progress_handler);
			}
		}
	}

	//==============================================================================
	// 다 받음 알리기.
	//==============================================================================
	fn notify_complete(&self) {
		let complete_handler = self.state.borrow_mut().complete_handler.take();
		if let Some(mut complete_handler) = complete_handler {
			let failed_list = self.get_failed_list();
			complete_handler(&failed_list);
			let mut state = self.state.borrow_mut();
			if state.complete_handler.is_none() {
				state.complete_handler = Some(complete_handler);
			}
		}
	}

	//==============================================================================
	// 진행 알림 함수 설정. (진행률, 끝난 수, 전체 수. None 이면 끔)
	//==============================================================================
	pub fn set_progress_handler(&self, progress_handler: Option<AssetLoaderProgressHandler>) {
		self.state.borrow_mut().progress_handler = progress_handler;
	}

	//==============================================================================
	// 다 받음 알림 함수 설정. (실패 목록. None 이면 끔)
	//==============================================================================
	pub fn set_complete_handler(&self, complete_handler: Option<AssetLoaderCompleteHandler>) {
		self.state.borrow_mut().complete_handler = complete_handler;
	}

	//==============================================================================
	// 다 받음 손잡이. (다 끝나면 끝나는 손잡이, 씬이 track_load_handle 로 맡긴다. 다 끝난 뒤 더 넣은 것은 기다리지 않는다)
	//==============================================================================
	pub fn get_completion_handle(&self) -> AssetLoadHandle {
		self.state.borrow().completion_handle.clone()
	}

	//==============================================================================
	// 진행률. (끝난 수 / 전체 수, 넣은 것이 없으면 1)
	//==============================================================================
	pub fn get_progress(&self) -> f32 {
		let state = self.state.borrow();
		if state.entry_list.is_empty() {
			return 1.0;
		}
		state.finished_count as f32 / state.entry_list.len() as f32
	}

	//==============================================================================
	// 전체 수.
	//==============================================================================
	pub fn get_total_count(&self) -> usize {
		self.state.borrow().entry_list.len()
	}

	//==============================================================================
	// 끝난 수. (받은 것과 실패한 것)
	//==============================================================================
	pub fn get_finished_count(&self) -> usize {
		self.state.borrow().finished_count
	}

	//==============================================================================
	// 받은 수. (실패한 것은 빼고)
	//==============================================================================
	pub fn get_loaded_count(&self) -> usize {
		self.state.borrow().loaded_count
	}

	//==============================================================================
	// 실패 목록. (복사본)
	//==============================================================================
	pub fn get_failed_list(&self) -> Vec<AssetLoadFailure> {
		self.state.borrow().failed_list.clone()
	}

	//==============================================================================
	// 넣은 것을 모두 받았는지. (실패 포함, 넣은 것이 없으면 true)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		let state = self.state.borrow();
		state.finished_count >= state.entry_list.len()
	}

	//==============================================================================
	// 지금 받는 중인 수. (요청했고 바이트가 아직 오지 않은 것)
	//==============================================================================
	pub fn get_in_flight_count(&self) -> usize {
		self.state.borrow().entry_list.iter().filter(|entry| entry.is_started && !entry.is_request_finished() && !matches!(entry.kind, AssetLoaderEntryKind::External)).count()
	}
}

impl Default for AssetLoader {
	fn default() -> AssetLoader {
		AssetLoader::new()
	}
}

//==============================================================================
// 시험. (데스크탑, 임시 애셋 폴더)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;
	use std::cell::Cell;

	//==============================================================================
	// 시험용 엔진. (애셋 폴더를 임시 폴더 하나로)
	//==============================================================================
	fn create_test_engine(asset_directory_name: &str) -> (Engine, std::path::PathBuf) {
		let asset_directory_path = std::env::temp_dir().join(format!("{}-{}", asset_directory_name, std::process::id()));
		std::fs::create_dir_all(&asset_directory_path).expect("asset directory");
		let mut engine_configuration = EngineConfiguration::new();
		engine_configuration.asset_directory_paths = vec![asset_directory_path.clone()];
		(Engine::new(engine_configuration), asset_directory_path)
	}

	//==============================================================================
	// 데스크탑은 load 에서 다 끝난다. 애셋이 채워지고, 실패는 목록에, 알림은 한 번씩.
	//==============================================================================
	#[test]
	fn load_fills_assets_and_reports_failures() {
		let (mut engine, asset_directory_path) = create_test_engine("vanillaAssetLoaderTest");
		std::fs::write(asset_directory_path.join("hello.txt"), "안녕 vanilla").expect("text file");
		std::fs::write(asset_directory_path.join("data.json"), "{\"count\": 3}").expect("json file");
		std::fs::write(asset_directory_path.join("data.bin"), [1u8, 2, 3]).expect("blob file");
		let asset_loader = AssetLoader::new();
		asset_loader.set_concurrency(2);
		let mut text_asset = asset_loader.add_text("hello.txt");
		let mut json_asset = asset_loader.add_json("data.json");
		let blob_asset = asset_loader.add_blob("data.bin");
		let missing_image_asset = asset_loader.add_image("missing.png");
		let font_handle = asset_loader.add_font(&mut engine, "MissingFont", "missing.ttf", false);
		let external_handle = AssetLoadHandle::create_pending("external");
		asset_loader.add_load_handle(&external_handle);
		let progress_call_count = Rc::new(Cell::new(0));
		let progress_call_count_for_handler = progress_call_count.clone();
		asset_loader.set_progress_handler(Some(Box::new(move |_progress, _finished_count, _total_count| {
			progress_call_count_for_handler.set(progress_call_count_for_handler.get() + 1);
		})));
		let complete_failure_count = Rc::new(Cell::new(usize::MAX));
		let complete_failure_count_for_handler = complete_failure_count.clone();
		asset_loader.set_complete_handler(Some(Box::new(move |failed_list| {
			complete_failure_count_for_handler.set(failed_list.len());
		})));
		assert_eq!(asset_loader.get_progress(), 0.0);
		assert!(!text_asset.is_loaded());

		// update 는 load 전이면 아무것도 하지 않는다.
		asset_loader.update(&engine);
		assert!(!text_asset.is_loaded());

		asset_loader.load(&engine);
		assert_eq!(asset_loader.get_finished_count(), 5);
		assert_eq!(asset_loader.get_loaded_count(), 3);
		assert!(!asset_loader.is_finished());
		assert!(!asset_loader.get_completion_handle().is_finished());
		assert_eq!(text_asset.get_text(), Some("안녕 vanilla"));
		assert_eq!(json_asset.get_data().expect("json")["count"], serde_json::json!(3));
		assert_eq!(blob_asset.get_blob().expect("blob").as_slice(), &[1u8, 2, 3]);
		assert!(!missing_image_asset.is_loaded());
		assert!(font_handle.is_failed());
		let failed_path_list: Vec<String> = asset_loader.get_failed_list().into_iter().map(|failure| failure.path).collect();
		assert_eq!(failed_path_list, vec![String::from("missing.png"), String::from("missing.ttf")]);

		// 바깥 손잡이가 끝나면 다 받음.
		external_handle.finish(Ok(Vec::new()));
		asset_loader.update(&engine);
		assert!(asset_loader.is_finished());
		assert_eq!(asset_loader.get_progress(), 1.0);
		assert!(asset_loader.get_completion_handle().is_finished());
		assert_eq!(complete_failure_count.get(), 2);
		assert_eq!(progress_call_count.get(), 2);
		asset_loader.update(&engine);
		assert_eq!(progress_call_count.get(), 2);

		// 빈 목록은 load 에서 바로 끝난다.
		let empty_loader = AssetLoader::new();
		empty_loader.load(&engine);
		assert!(empty_loader.is_finished());
		assert!(empty_loader.get_completion_handle().is_finished());
		assert_eq!(empty_loader.get_progress(), 1.0);
		let _ = std::fs::remove_dir_all(&asset_directory_path);
	}

	//==============================================================================
	// 동시 수 설정. (데스크탑은 요청한 자리에서 읽혀 동시 수와 상관없이 load 에서 다 끝난다)
	//==============================================================================
	#[test]
	fn concurrency_setting_and_desktop_load() {
		let (engine, asset_directory_path) = create_test_engine("vanillaAssetLoaderConcurrency");
		let asset_loader = AssetLoader::new();
		asset_loader.set_concurrency(3);
		assert_eq!(asset_loader.get_concurrency(), 3);
		asset_loader.set_concurrency(0);
		assert_eq!(asset_loader.get_concurrency(), 1);
		for file_index in 0..4 {
			std::fs::write(asset_directory_path.join(format!("file{}.txt", file_index)), "x").expect("file");
			asset_loader.add_text(&format!("file{}.txt", file_index));
		}
		asset_loader.load(&engine);
		assert_eq!(asset_loader.get_in_flight_count(), 0);
		assert_eq!(asset_loader.get_loaded_count(), 4);
		let _ = std::fs::remove_dir_all(&asset_directory_path);
	}

	//==============================================================================
	// 그림은 다 받았을 때 풀어 본다. 깨진 그림은 실패 목록에, 푼 그림은 ImageAsset 이 그대로 쓴다.
	//==============================================================================
	#[test]
	fn broken_image_is_failure_and_good_image_is_decoded_once() {
		let (engine, asset_directory_path) = create_test_engine("vanillaAssetLoaderImage");
		std::fs::write(asset_directory_path.join("broken.png"), [0x89u8, b'P', b'N', b'G', 0, 1, 2, 3]).expect("broken file");
		let mut png_bytes: Vec<u8> = Vec::new();
		let pixel_image = image::RgbaImage::from_pixel(2, 3, image::Rgba([255, 0, 0, 255]));
		pixel_image.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).expect("png");
		std::fs::write(asset_directory_path.join("good.png"), &png_bytes).expect("good file");
		let asset_loader = AssetLoader::new();
		let mut broken_image_asset = asset_loader.add_image("broken.png");
		let mut good_image_asset = asset_loader.add_image("good.png");
		asset_loader.load(&engine);
		assert!(asset_loader.is_finished());
		assert_eq!(asset_loader.get_loaded_count(), 1);
		let failed_path_list: Vec<String> = asset_loader.get_failed_list().into_iter().map(|failure| failure.path).collect();
		assert_eq!(failed_path_list, vec![String::from("broken.png")]);
		assert!(broken_image_asset.get_image().is_none());
		assert!(good_image_asset.get_asset_base().get_load_handle().and_then(|load_handle| load_handle.get_decoded_image()).is_some());
		assert_eq!(good_image_asset.get_image_size(), crate::base::vector2::Vector2::create(2.0, 3.0));
		let _ = std::fs::remove_dir_all(&asset_directory_path);
	}
}
