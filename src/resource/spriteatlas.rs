//==============================================================================
//! 스프라이트 아틀라스. (vanilla.js resource/spriteatlas.js 의 SpriteAtlas)
//! - 쪽 그림 몇 장에 모은 그림을 이름으로 꺼낸다. 같은 쪽 그림을 쓰는 스프라이트는 엔진이 묶어 한 번에 그린다.
//! - 읽는 JSON 꼴은 원본과 같은 둘이다.
//!   1. tools/atlas.cjs 가 만든 것. { images: { "이름.png": { x, y, width, height, page } }, meta: { imageFileName, pages: [{ imageFileName }] } }
//!   ```text
//!      page 가 없으면 0 쪽, meta.pages 가 없으면 meta.imageFileName 한 장이다.
//!   ```
//!   2. 쪽 경로와 [쪽, x, y, 가로, 세로] 표. { pagePaths: `["경로"]`, frames: { "이름": [쪽, x, y, 가로, 세로] } }
//! - 그림 이름은 확장자를 뗀 이름으로도 찾는다. ("icon-close.png" 는 "icon-close" 로도 "icon-close.png" 로도 찾는다)
//! - 쪽 경로에 빗금(/)이 없으면 JSON 이 있는 폴더를 기준으로, 있으면 적힌 그대로 읽는다. ("./" 로 시작하면 그 "./" 를 떼고 애셋 폴더 기준이다)
//! - 원본과 다른 것:
//!   원본은 load 가 JSON 을 기다린 뒤 쪽 그림을 받는 async 다. 러스트 판은 애셋처럼 요청만 하고, update 가 JSON 이 온 것을 보고 쪽 그림을 청한다.
//!   load, load_data 는 원본처럼 await 할 수 있는 SpriteAtlasLoadHandle 을 돌려준다. 손잡이는 불러오기 상태를 나눠 들어 아틀라스를 빌리지 않으므로
//!   씬이 든 아틀라스도 scene_context.with(|씬, 엔진| 씬.아틀라스.load(엔진, 경로)).await 로 기다린다. (기다리는 동안 프레임마다 update 를 불러 준다)
//!   데스크탑은 그 자리에서 읽혀 load 가 끝나면 다 읽혔다. 웹은 JSON 과 쪽 그림이 차례로 오므로 is_loaded 가 true 가 될 때까지 tick 에서 update 를 부르고,
//!   그 뒤에 조각을 건다. (씬이 맡아 기다리게 하려면 JSON 은 track_json_asset, 쪽 그림은 온 뒤 update 가 청한다)
//!   원본의 찾을 아틀라스 목록(add_search_atlas, find_frame, apply_search_frame)은 스레드 하나에 하나인 목록이고, 아틀라스를 나눠 들게 `Rc<RefCell<SpriteAtlas>>` 로 넣는다.
//!   find_frame 은 빌린 조각 대신 조각의 복사(Frame 은 그림 Rc 와 영역이라 값싸다)를 돌려준다. 넘긴 목록에서 찾는 find_frame_in_order 도 있다.
//!   get_page_images 는 원본 HTMLImageElement 목록 대신 읽힌 쪽 그림(Image) 목록이다.
//==============================================================================
use crate::base::rect::Rect;
use crate::core::asset::Asset;
use crate::core::component::sprite::Sprite;
use crate::core::engine::Engine;
use crate::core::frame::Frame;
use crate::core::image::Image;
use crate::core::scene::SceneBase;
use crate::core::task;
use crate::resource::imageasset::ImageAsset;
use crate::resource::textasset::JsonAsset;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

thread_local! {
	/// 찾을 아틀라스 목록. (원본 모듈의 searchAtlasList, 넣은 차례로 찾는다)
	static SEARCH_ATLAS_LIST: RefCell<Vec<Rc<RefCell<SpriteAtlas>>>> = const { RefCell::new(Vec::new()) };
}

//==============================================================================
/// 아틀라스 자료에서 읽은 조각 하나. (이름, 쪽, 영역)
//==============================================================================
struct AtlasFrameValue {
	frame_name: String,
	page_index: usize,
	rect: Rect,
}

//==============================================================================
/// 아틀라스 자료에서 읽은 것. (조각 값과 쪽 그림, 자료를 읽었는지)
//==============================================================================
#[derive(Default)]
struct SpriteAtlasData {
	frame_value_list: Vec<AtlasFrameValue>,
	page_asset_list: Vec<ImageAsset>,
	is_data_read: bool,
}

//==============================================================================
/// 아틀라스 불러오기 하나의 나눠 든 상태. (아틀라스와 받기 손잡이가 나눠 들어, 손잡이의 await 가 아틀라스를 빌리지 않고 이어 간다)
//==============================================================================
struct SpriteAtlasLoad {
	json_asset: Option<JsonAsset>,
	base_directory: String,
	atlas_data: RefCell<SpriteAtlasData>,
	/// 지은 조각. (쪽 그림이 다 오면 한 번 채운다, 채우면 다 읽힌 것이다)
	frame_table: OnceCell<HashMap<String, Frame>>,
	/// 끝났는지. (조각을 지었거나, 원본처럼 자료를 읽지 못해 조각 없이 끝났거나)
	is_finished: Cell<bool>,
}

impl SpriteAtlasLoad {
	//==============================================================================
	/// 생성.
	//==============================================================================
	fn new(json_asset: Option<JsonAsset>, base_directory: &str) -> SpriteAtlasLoad {
		SpriteAtlasLoad {
			json_asset,
			base_directory: base_directory.to_string(),
			atlas_data: RefCell::new(SpriteAtlasData::default()),
			frame_table: OnceCell::new(),
			is_finished: Cell::new(false),
		}
	}

	//==============================================================================
	/// 진행. (JSON 이 왔으면 쪽 그림을 청하고, 쪽 그림이 다 왔으면 조각을 짓는다. 끝났으면 true)
	/// - 원본처럼 JSON 을 읽지 못했거나 꼴을 모르면 조각을 짓지 않고 끝난다. (원본 loadData 가 알리고 그냥 돌아온다)
	//==============================================================================
	fn update(&self, engine: &Engine) -> bool {
		if self.is_finished.get() {
			return true;
		}
		let is_data_read = self.atlas_data.borrow().is_data_read;
		if !is_data_read {
			let atlas_data = match self.json_asset.as_ref() {
				Some(json_asset) => {
					if !json_asset.get_asset_base().get_load_handle().map(|load_handle| load_handle.is_finished()).unwrap_or(false) {
						return false;
					}
					json_asset.get_data().cloned()
				}
				None => {
					return false;
				}
			};
			let is_data_read = match atlas_data {
				Some(atlas_data) => {
					self.read_data(engine, &atlas_data)
				}
				None => {
					eprintln!("SpriteAtlas: 아틀라스 JSON 을 읽지 못했습니다");
					false
				}
			};
			if !is_data_read {
				self.is_finished.set(true);
				return true;
			}
		}
		let is_every_page_finished = self.atlas_data.borrow().page_asset_list.iter().all(|page_asset| page_asset.get_asset_base().get_load_handle().map(|load_handle| load_handle.is_finished()).unwrap_or(true));
		if !is_every_page_finished {
			return false;
		}
		self.build_frames();
		self.is_finished.set(true);
		true
	}

	//==============================================================================
	/// 자료 읽기. (조각 값을 담고 쪽 그림을 청한다, 꼴을 모르면 원본처럼 알리고 false)
	//==============================================================================
	fn read_data(&self, engine: &Engine, atlas_data: &serde_json::Value) -> bool {
		let mut page_path_list: Vec<String> = Vec::new();
		let mut frame_value_list: Vec<AtlasFrameValue> = Vec::new();
		if let Some(image_table) = atlas_data.get("images").and_then(|images| images.as_object()) {
			let meta = atlas_data.get("meta");
			let page_list = meta.and_then(|meta| meta.get("pages")).and_then(|pages| pages.as_array());
			match page_list {
				Some(page_list) => {
					for page in page_list {
						let image_file_name = page.get("imageFileName").and_then(|value| value.as_str()).unwrap_or("");
						page_path_list.push(image_file_name.to_string());
					}
				}
				None => {
					if let Some(image_file_name) = meta.and_then(|meta| meta.get("imageFileName")).and_then(|value| value.as_str()) {
						page_path_list.push(image_file_name.to_string());
					}
				}
			}
			for (file_name, image_data) in image_table {
				let read_number = |key: &str| -> f32 {
					image_data.get(key).and_then(|value| value.as_f64()).unwrap_or(0.0) as f32
				};
				let page_index = image_data.get("page").and_then(|value| value.as_u64()).unwrap_or(0) as usize;
				frame_value_list.push(AtlasFrameValue {
					frame_name: file_name.clone(),
					page_index,
					rect: Rect::create(read_number("x"), read_number("y"), read_number("width"), read_number("height")),
				});
			}
		}
		else if let Some(frame_table) = atlas_data.get("frames").and_then(|frames| frames.as_object()) {
			if let Some(page_paths) = atlas_data.get("pagePaths").and_then(|page_paths| page_paths.as_array()) {
				for page_path in page_paths {
					page_path_list.push(page_path.as_str().unwrap_or("").to_string());
				}
			}
			for (frame_name, frame_values) in frame_table {
				let read_number = |index: usize| -> f64 {
					frame_values.get(index).and_then(|value| value.as_f64()).unwrap_or(0.0)
				};
				frame_value_list.push(AtlasFrameValue {
					frame_name: frame_name.clone(),
					page_index: read_number(0) as usize,
					rect: Rect::create(read_number(1) as f32, read_number(2) as f32, read_number(3) as f32, read_number(4) as f32),
				});
			}
		}
		else {
			eprintln!("SpriteAtlas: 아틀라스 꼴을 읽지 못했습니다");
			return false;
		}

		// 쪽 그림을 한꺼번에 청한다.
		let mut page_asset_list: Vec<ImageAsset> = Vec::new();
		for page_path in &page_path_list {
			let resolved_path = if page_path.contains('/') {
				page_path.trim_start_matches("./").to_string()
			}
			else {
				format!("{}{}", self.base_directory, page_path)
			};
			let mut page_asset = ImageAsset::new();
			page_asset.load(engine, &resolved_path);
			page_asset_list.push(page_asset);
		}
		let mut sprite_atlas_data = self.atlas_data.borrow_mut();
		sprite_atlas_data.frame_value_list = frame_value_list;
		sprite_atlas_data.page_asset_list = page_asset_list;
		sprite_atlas_data.is_data_read = true;
		true
	}

	//==============================================================================
	/// 조각 짓기. (쪽 그림이 없거나 풀리지 않은 조각은 뺀다)
	//==============================================================================
	fn build_frames(&self) {
		let mut frame_table: HashMap<String, Frame> = HashMap::new();
		let mut sprite_atlas_data = self.atlas_data.borrow_mut();
		let mut page_image_list = Vec::with_capacity(sprite_atlas_data.page_asset_list.len());
		for page_asset in sprite_atlas_data.page_asset_list.iter_mut() {
			page_image_list.push(page_asset.get_image());
		}
		for frame_value in &sprite_atlas_data.frame_value_list {
			let page_image = match page_image_list.get(frame_value.page_index).and_then(|page_image| page_image.clone()) {
				Some(page_image) => {
					page_image
				}
				None => {
					continue;
				}
			};
			let frame = Frame::new(page_image, Some(frame_value.rect));
			let base_name = remove_extension(&frame_value.frame_name);
			if base_name != frame_value.frame_name && !frame_table.contains_key(base_name) {
				frame_table.insert(base_name.to_string(), frame.clone());
			}
			frame_table.insert(frame_value.frame_name.clone(), frame);
		}
		let _ = self.frame_table.set(frame_table);
	}
}

//==============================================================================
/// 아틀라스 받기 손잡이. (SpriteAtlas::load, load_data 가 돌려준다. 원본 load 의 Promise 자리, 아틀라스를 빌리지 않는다)
/// - await 하면 JSON 과 쪽 그림을 다 받아 조각을 지을 때까지 기다린다. (기다리는 동안 프레임마다 update 를 불러 준다)
//==============================================================================
#[derive(Clone)]
pub struct SpriteAtlasLoadHandle {
	atlas_load: Rc<SpriteAtlasLoad>,
}

impl SpriteAtlasLoadHandle {
	//==============================================================================
	/// 끝났는지. (조각을 지었거나 자료를 읽지 못해 끝났거나)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.atlas_load.is_finished.get()
	}
}

impl IntoFuture for SpriteAtlasLoadHandle {
	type Output = ();
	type IntoFuture = SpriteAtlasLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> SpriteAtlasLoadFuture {
		SpriteAtlasLoadFuture {
			atlas_load: self.atlas_load,
		}
	}
}

//==============================================================================
/// 아틀라스 받기를 기다리는 퓨처. (손잡이를 await 하거나 wait_for_load 가 돌려준다)
//==============================================================================
#[derive(Clone)]
pub struct SpriteAtlasLoadFuture {
	atlas_load: Rc<SpriteAtlasLoad>,
}

impl Future for SpriteAtlasLoadFuture {
	type Output = ();

	//==============================================================================
	/// update 를 부르고 끝났는지 보기. (아직이면 다음 프레임에 다시 본다, 원본처럼 자료를 읽지 못해도 끝난다)
	//==============================================================================
	fn poll(self: Pin<&mut SpriteAtlasLoadFuture>, context: &mut Context<'_>) -> Poll<()> {
		task::with_polling_engine(|engine| self.atlas_load.update(engine));
		if self.atlas_load.is_finished.get() {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

//==============================================================================
/// 스프라이트 아틀라스.
//==============================================================================
pub struct SpriteAtlas {
	atlas_load: Rc<SpriteAtlasLoad>,
	/// 앞서 다 읽힌 불러오기. (다시 불러오는 동안 원본처럼 앞의 조각을 그대로 쓴다)
	previous_atlas_load: Option<Rc<SpriteAtlasLoad>>,
}

//==============================================================================
/// 확장자를 뗀 이름. ("icon-close.png" → "icon-close", 확장자가 없으면 그대로)
//==============================================================================
fn remove_extension(file_name: &str) -> &str {
	let slash_index = file_name.rfind('/').map(|index| index as i64).unwrap_or(-1);
	match file_name.rfind('.') {
		Some(dot_index) if dot_index > 0 && (dot_index as i64) > slash_index => {
			&file_name[..dot_index]
		}
		_ => {
			file_name
		}
	}
}

impl SpriteAtlas {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> SpriteAtlas {
		SpriteAtlas {
			atlas_load: Rc::new(SpriteAtlasLoad::new(None, "")),
			previous_atlas_load: None,
		}
	}

	//==============================================================================
	/// 아틀라스 JSON 과 쪽 그림 불러오기. (JSON 이 오면 쪽 그림을 청한다, 데스크탑은 이 자리에서 다 읽힌다)
	/// - 원본처럼 await 할 수 있는 손잡이를 돌려준다. (조각을 지을 때까지)
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, json_path: &str) -> SpriteAtlasLoadHandle {
		let base_directory = match json_path.rfind('/') {
			Some(slash_index) => {
				&json_path[..slash_index + 1]
			}
			None => {
				""
			}
		};
		let mut json_asset = JsonAsset::new();
		json_asset.load(engine, json_path);
		self.replace_atlas_load(SpriteAtlasLoad::new(Some(json_asset), base_directory));
		self.atlas_load.update(engine);
		SpriteAtlasLoadHandle {
			atlas_load: self.atlas_load.clone(),
		}
	}

	//==============================================================================
	/// 아틀라스 자료로 불러오기. (JSON 을 읽은 값이나 코드에 둔 같은 꼴의 값, 쪽 경로에 빗금이 없으면 base_directory 를 앞에 붙인다)
	/// - 원본 loadData 처럼 await 할 수 있는 손잡이를 돌려준다. (쪽 그림을 다 받아 조각을 지을 때까지)
	//==============================================================================
	pub fn load_data(&mut self, engine: &Engine, atlas_data: &serde_json::Value, base_directory: &str) -> SpriteAtlasLoadHandle {
		self.replace_atlas_load(SpriteAtlasLoad::new(None, base_directory));
		if !self.atlas_load.read_data(engine, atlas_data) {
			self.atlas_load.is_finished.set(true);
		}
		self.atlas_load.update(engine);
		SpriteAtlasLoadHandle {
			atlas_load: self.atlas_load.clone(),
		}
	}

	//==============================================================================
	/// 새 불러오기로 바꾸기. (조각을 지은 앞의 불러오기는 새 것이 조각을 지을 때까지 남겨 둔다)
	//==============================================================================
	fn replace_atlas_load(&mut self, atlas_load: SpriteAtlasLoad) {
		if self.atlas_load.frame_table.get().is_some() {
			self.previous_atlas_load = Some(self.atlas_load.clone());
		}
		self.atlas_load = Rc::new(atlas_load);
	}

	//==============================================================================
	/// 지금 보이는 불러오기. (새 것이 조각을 지었으면 새 것, 아니면 앞서 다 읽힌 것, 그것도 없으면 새 것)
	//==============================================================================
	fn read_shown_atlas_load(&self) -> &Rc<SpriteAtlasLoad> {
		if self.atlas_load.frame_table.get().is_none() {
			if let Some(previous_atlas_load) = &self.previous_atlas_load {
				return previous_atlas_load;
			}
		}
		&self.atlas_load
	}

	//==============================================================================
	/// 지금 보이는 조각 표. (지은 것이 없으면 None)
	//==============================================================================
	fn read_frame_table(&self) -> Option<&HashMap<String, Frame>> {
		self.read_shown_atlas_load().frame_table.get()
	}

	//==============================================================================
	/// 진행. (JSON 이 왔으면 쪽 그림을 청하고, 쪽 그림이 다 왔으면 조각을 짓는다. 끝났으면 true, 원본처럼 자료를 읽지 못해도 끝난다)
	//==============================================================================
	pub fn update(&mut self, engine: &Engine) -> bool {
		self.atlas_load.update(engine)
	}

	//==============================================================================
	/// 씬이 아틀라스 JSON 을 기다리게 맡기기. (쪽 그림은 JSON 이 온 뒤 update 가 청한다)
	//==============================================================================
	pub fn track_json_asset(&self, scene_base: &mut SceneBase) {
		if let Some(json_asset) = &self.atlas_load.json_asset {
			scene_base.track_asset(json_asset);
		}
	}

	//==============================================================================
	/// 다 읽혔는지. (조각을 지었는지, 다시 불러오는 동안은 원본처럼 앞의 것으로 본다)
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		self.read_frame_table().is_some()
	}

	//==============================================================================
	/// 다 읽히기를 기다리는 퓨처. (load 나 load_data 가 돌려준 손잡이를 await 하는 것과 같다. 아틀라스를 빌리지 않는다)
	//==============================================================================
	pub fn wait_for_load(&self) -> SpriteAtlasLoadFuture {
		SpriteAtlasLoadFuture {
			atlas_load: self.atlas_load.clone(),
		}
	}

	//==============================================================================
	/// 조각 읽기. (없거나 아직 읽히지 않았으면 None)
	//==============================================================================
	pub fn read_frame(&self, frame_name: &str) -> Option<&Frame> {
		self.read_frame_table()?.get(frame_name)
	}

	//==============================================================================
	/// 조각이 있는지.
	//==============================================================================
	pub fn has_frame(&self, frame_name: &str) -> bool {
		self.read_frame(frame_name).is_some()
	}

	//==============================================================================
	/// 스프라이트에 조각 걸기. (없는 이름이면 그림을 비우고 false)
	//==============================================================================
	pub fn apply_frame(&self, sprite: &mut Sprite, frame_name: &str) -> bool {
		let frame = self.read_frame(frame_name);
		sprite.set_frame(frame);
		frame.is_some()
	}

	//==============================================================================
	/// 쪽 그림 목록. (읽힌 쪽만, 쪽 차례, 다시 불러오는 동안은 원본처럼 앞의 것)
	//==============================================================================
	pub fn get_page_images(&mut self) -> Vec<Rc<Image>> {
		self.read_shown_atlas_load().atlas_data.borrow_mut().page_asset_list.iter_mut().filter_map(|page_asset| page_asset.get_image()).collect()
	}

	//==============================================================================
	/// 쪽 수.
	//==============================================================================
	pub fn get_page_count(&self) -> usize {
		self.read_shown_atlas_load().atlas_data.borrow().page_asset_list.len()
	}

	//==============================================================================
	/// 조각 이름 목록.
	//==============================================================================
	pub fn get_frame_names(&self) -> Vec<String> {
		self.read_frame_table().map(|frame_table| frame_table.keys().cloned().collect()).unwrap_or_default()
	}

	//==============================================================================
	/// 여러 아틀라스에서 차례로 조각 찾기. (앞의 것이 이긴다, 원본 SpriteAtlas.findFrame 의 찾을 목록 자리)
	//==============================================================================
	pub fn find_frame_in_order<'a>(atlas_list: &[&'a SpriteAtlas], frame_name: &str) -> Option<&'a Frame> {
		atlas_list.iter().find_map(|sprite_atlas| sprite_atlas.read_frame(frame_name))
	}

	//==============================================================================
	/// 여러 아틀라스에서 차례로 찾은 조각을 스프라이트에 걸기. (없으면 그림을 비우고 false, 원본 applySearchFrame)
	//==============================================================================
	pub fn apply_frame_in_order(atlas_list: &[&SpriteAtlas], sprite: &mut Sprite, frame_name: &str) -> bool {
		let frame = SpriteAtlas::find_frame_in_order(atlas_list, frame_name);
		sprite.set_frame(frame);
		frame.is_some()
	}

	//==============================================================================
	/// 찾을 아틀라스 넣기. (맨 뒤에 넣는다, 이미 들었으면 그대로)
	//==============================================================================
	pub fn add_search_atlas(sprite_atlas: &Rc<RefCell<SpriteAtlas>>) {
		SEARCH_ATLAS_LIST.with(|search_atlas_list| {
			let mut search_atlas_list = search_atlas_list.borrow_mut();
			if search_atlas_list.iter().any(|search_atlas| Rc::ptr_eq(search_atlas, sprite_atlas)) {
				return;
			}
			search_atlas_list.push(sprite_atlas.clone());
		});
	}

	//==============================================================================
	/// 찾을 아틀라스 빼기.
	//==============================================================================
	pub fn remove_search_atlas(sprite_atlas: &Rc<RefCell<SpriteAtlas>>) {
		SEARCH_ATLAS_LIST.with(|search_atlas_list| {
			search_atlas_list.borrow_mut().retain(|search_atlas| !Rc::ptr_eq(search_atlas, sprite_atlas));
		});
	}

	//==============================================================================
	/// 찾을 아틀라스 모두 빼기.
	//==============================================================================
	pub fn clear_search_atlases() {
		SEARCH_ATLAS_LIST.with(|search_atlas_list| {
			search_atlas_list.borrow_mut().clear();
		});
	}

	//==============================================================================
	/// 찾을 아틀라스들에서 조각 찾기. (넣은 차례로, 없으면 None, 다른 곳에서 고치는 중인 아틀라스는 건너뛴다)
	//==============================================================================
	pub fn find_frame(frame_name: &str) -> Option<Frame> {
		SEARCH_ATLAS_LIST.with(|search_atlas_list| {
			for search_atlas in search_atlas_list.borrow().iter() {
				if let Ok(sprite_atlas) = search_atlas.try_borrow() {
					if let Some(frame) = sprite_atlas.read_frame(frame_name) {
						return Some(frame.clone());
					}
				}
			}
			None
		})
	}

	//==============================================================================
	/// 찾을 아틀라스들에서 찾은 조각을 스프라이트에 걸기. (없으면 그림을 비우고 false)
	//==============================================================================
	pub fn apply_search_frame(sprite: &mut Sprite, frame_name: &str) -> bool {
		let frame = SpriteAtlas::find_frame(frame_name);
		sprite.set_frame(frame.as_ref());
		frame.is_some()
	}
}

impl Default for SpriteAtlas {
	fn default() -> SpriteAtlas {
		SpriteAtlas::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;

	//==============================================================================
	/// 시험용 엔진과 쪽 그림 둘. (애셋 폴더를 임시 폴더 하나로)
	//==============================================================================
	fn create_test_engine(asset_directory_name: &str) -> (Engine, std::path::PathBuf) {
		let asset_directory_path = std::env::temp_dir().join(asset_directory_name);
		std::fs::create_dir_all(asset_directory_path.join("atlas")).expect("asset directory");
		for (page_index, page_width) in [(0, 64u32), (1, 32u32)] {
			let page_image = image::RgbaImage::from_pixel(page_width, 32, image::Rgba([255, 255, 255, 255]));
			page_image.save(asset_directory_path.join("atlas").join(format!("icons-{}.png", page_index))).expect("page png");
		}
		let mut engine_configuration = EngineConfiguration::new();
		engine_configuration.asset_directory_paths = vec![asset_directory_path.clone()];
		(Engine::new(engine_configuration), asset_directory_path)
	}

	//==============================================================================
	/// atlas.cjs 꼴: 쪽 여럿, 확장자를 뗀 이름, 없는 이름.
	//==============================================================================
	#[test]
	fn load_atlas_cjs_json() {
		let (engine, asset_directory_path) = create_test_engine("vanillaSpriteAtlasTestJson");
		let json_text = r#"{ "images": { "star.png": { "x": 2, "y": 2, "width": 20, "height": 24, "page": 0 }, "coin.png": { "x": 4, "y": 6, "width": 10, "height": 10, "page": 1 } }, "meta": { "imageFileName": "icons-0.png", "pages": [{ "imageFileName": "icons-0.png" }, { "imageFileName": "icons-1.png" }] } }"#;
		std::fs::write(asset_directory_path.join("atlas").join("icons.json"), json_text).expect("json");
		let mut sprite_atlas = SpriteAtlas::new();
		sprite_atlas.load(&engine, "atlas/icons.json");
		assert!(sprite_atlas.update(&engine));
		assert_eq!(sprite_atlas.get_page_count(), 2);
		let star_frame = sprite_atlas.read_frame("star").expect("star");
		assert_eq!(star_frame.get_image_rect(), Rect::create(2.0, 2.0, 20.0, 24.0));
		assert_eq!(star_frame.get_image().get_width(), 64);
		assert!(sprite_atlas.has_frame("star.png"));
		let coin_frame = sprite_atlas.read_frame("coin").expect("coin");
		assert_eq!(coin_frame.get_image().get_width(), 32);
		assert!(sprite_atlas.read_frame("nothing").is_none());
		let mut sprite = Sprite::new();
		assert!(sprite_atlas.apply_frame(&mut sprite, "coin"));
		assert_eq!(sprite.get_image_rect(), Rect::create(4.0, 6.0, 10.0, 10.0));
		assert!(!sprite_atlas.apply_frame(&mut sprite, "nothing"));
		assert!(sprite.get_image().is_none());
	}

	//==============================================================================
	/// 쪽 경로와 표 꼴, 여러 아틀라스를 찾는 차례. (앞의 것이 이긴다)
	//==============================================================================
	#[test]
	fn load_frames_table_and_search_order() {
		let (engine, _asset_directory_path) = create_test_engine("vanillaSpriteAtlasTestTable");
		let first_data = serde_json::json!({ "pagePaths": ["./atlas/icons-1.png"], "frames": { "star": [0, 0, 0, 8, 8] } });
		let second_data = serde_json::json!({ "pagePaths": ["icons-0.png"], "frames": { "star": [0, 2, 2, 20, 24], "gem": [0, 30, 2, 16, 16] } });
		let mut first_atlas = SpriteAtlas::new();
		first_atlas.load_data(&engine, &first_data, "");
		let mut second_atlas = SpriteAtlas::new();
		second_atlas.load_data(&engine, &second_data, "atlas/");
		assert!(first_atlas.is_loaded() && second_atlas.is_loaded());
		let atlas_list = [&first_atlas, &second_atlas];
		assert_eq!(SpriteAtlas::find_frame_in_order(&atlas_list, "star").expect("star").get_image_rect().size.x, 8.0);
		assert_eq!(SpriteAtlas::find_frame_in_order(&atlas_list, "gem").expect("gem").get_image_rect().size.x, 16.0);
		let mut sprite = Sprite::new();
		assert!(SpriteAtlas::apply_frame_in_order(&atlas_list, &mut sprite, "gem"));
		assert!(!SpriteAtlas::apply_frame_in_order(&atlas_list, &mut sprite, "nothing"));
		assert_eq!(second_atlas.get_page_images().len(), 1);

		// 찾을 아틀라스 목록. (넣은 차례로, 같은 것은 한 번)
		let first_atlas = Rc::new(RefCell::new(first_atlas));
		let second_atlas = Rc::new(RefCell::new(second_atlas));
		SpriteAtlas::add_search_atlas(&second_atlas);
		SpriteAtlas::add_search_atlas(&first_atlas);
		SpriteAtlas::add_search_atlas(&second_atlas);
		assert_eq!(SpriteAtlas::find_frame("star").expect("star").get_image_rect().size.x, 20.0);
		SpriteAtlas::remove_search_atlas(&second_atlas);
		assert_eq!(SpriteAtlas::find_frame("star").expect("star").get_image_rect().size.x, 8.0);
		assert!(SpriteAtlas::apply_search_frame(&mut sprite, "star"));
		assert!(!SpriteAtlas::apply_search_frame(&mut sprite, "gem"));
		SpriteAtlas::clear_search_atlases();
		assert!(SpriteAtlas::find_frame("star").is_none());
	}
}
