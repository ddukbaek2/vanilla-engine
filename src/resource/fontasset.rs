//==============================================================================
//! 글꼴 애셋. (원본 resource/fontasset.js)
//! - 원본은 FontFace 를 만들어 document.fonts 에 더한다. 그 뒤로 폰트 문자열("16px 이름")에서 그 이름을 쓴다.
//! - 러스트 판은 바이트를 받은 뒤 엔진이 다음 프레임 시작에 Graphic 의 글자 캐시에 이름으로 등록한다.
//!   WOFF, WOFF2 도 된다. (글자 캐시가 TTF 로 푼다)
//! - 원본 descriptors(굵기, 기울기, 글자 범위)는 FontFaceDescriptors 로 받는다. (load_font_with_descriptors, 굵기만이면 load_font_with_bold)
//!   같은 이름에 굵기나 기울기가 다른 파일을 여러 개 올리면 글자 캐시가 CSS 처럼 골라 그린다. 진짜 기울인 얼굴이 있으면 가짜 기울임 대신 그것을 쓴다.
//! - unload 는 원본처럼 그 얼굴을 글꼴 목록에서 뺀다. (document.fonts.delete)
//! - load: 원본 load(assetPath) 처럼 파일 이름에서 확장자를 뗀 것을 패밀리 이름으로 load_font 한다. ("assets/DOSGothic.woff" 면 "DOSGothic")
//! - load_font_from_bytes: 이미 가진 글꼴 바이트로 읽는다. (원본 loadFontFromBytes, 서술자의 source_key 가 비면 이름을 쓴다)
//! - load_font 들은 원본처럼 await 할 수 있는 FontLoadHandle 을 돌려준다. 바이트가 오고 엔진이 글꼴 목록에 올리면 끝난다.
//!   (원본 await fontFace.load() 와 document.fonts.add 뒤, 그 뒤로 그 이름으로 그린다) 받기나 올리기에 실패하면 까닭으로 끝난다.
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use crate::core::textshaping::FontFaceDescriptors;
use std::cell::RefCell;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

//==============================================================================
/// 글꼴 받기 손잡이. (FontAsset 의 load_font 들, Engine::register_font_face_when_loaded 가 돌려준다. 원본 loadFont 의 Promise 자리)
/// - 엔진이 글꼴 목록에 올리면(또는 실패하면) 끝난다. 복제한 손잡이는 같은 올리기를 가리킨다.
//==============================================================================
#[derive(Clone, Debug)]
pub struct FontLoadHandle {
	load_handle: AssetLoadHandle,
	registration_result: Rc<RefCell<Option<Result<(), String>>>>,
}

impl FontLoadHandle {
	//==============================================================================
	/// 생성. (올리기 전)
	//==============================================================================
	pub(crate) fn new(load_handle: AssetLoadHandle) -> FontLoadHandle {
		FontLoadHandle {
			load_handle,
			registration_result: Rc::new(RefCell::new(None)),
		}
	}

	//==============================================================================
	/// 바이트 읽기 손잡이. (씬에 맡길 때)
	//==============================================================================
	pub fn get_load_handle(&self) -> &AssetLoadHandle {
		&self.load_handle
	}

	//==============================================================================
	/// 끝났는지. (글꼴 목록에 올렸거나 실패했거나)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.registration_result.borrow().is_some()
	}

	//==============================================================================
	/// 올리기 끝냄. (엔진이 부른다)
	//==============================================================================
	pub(crate) fn finish(&self, registration_result: Result<(), String>) {
		*self.registration_result.borrow_mut() = Some(registration_result);
	}
}

//==============================================================================
/// 글꼴 받기를 기다리는 퓨처. (FontLoadHandle 을 await 하면 된다)
//==============================================================================
#[derive(Clone, Debug)]
pub struct FontLoadFuture {
	font_load_handle: FontLoadHandle,
}

impl Future for FontLoadFuture {
	type Output = Result<(), String>;

	//==============================================================================
	/// 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut FontLoadFuture>, context: &mut Context<'_>) -> Poll<Result<(), String>> {
		let registration_result = self.font_load_handle.registration_result.borrow().clone();
		match registration_result {
			Some(registration_result) => {
				Poll::Ready(registration_result)
			}
			None => {
				context.waker().wake_by_ref();
				Poll::Pending
			}
		}
	}
}

impl IntoFuture for FontLoadHandle {
	type Output = Result<(), String>;
	type IntoFuture = FontLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> FontLoadFuture {
		FontLoadFuture {
			font_load_handle: self,
		}
	}
}

//==============================================================================
/// 글꼴 애셋.
//==============================================================================
pub struct FontAsset {
	asset_base: AssetBase,
	family: String,
	descriptors: FontFaceDescriptors,
	font_load_handle: Option<FontLoadHandle>,
}

impl FontAsset {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> FontAsset {
		FontAsset {
			asset_base: AssetBase::new(AssetType::Font),
			family: String::new(),
			descriptors: FontFaceDescriptors::new(),
			font_load_handle: None,
		}
	}

	//==============================================================================
	/// 글꼴 읽기 요청. (원본 load(assetPath), 파일 이름에서 확장자를 뗀 것을 패밀리 이름으로 load_font 한다)
	///
	/// # 인자
	/// - `engine`: 글꼴을 받아 등록할 엔진.
	/// - `asset_path`: 글꼴 파일 경로. ("assets/DOSGothic.woff" 면 패밀리 이름은 "DOSGothic")
	///
	/// # 돌려줌
	/// await 할 수 있는 글꼴 받기 손잡이. (받기나 올리기에 실패하면 까닭으로 끝난다)
	//==============================================================================
	pub fn load(&mut self, engine: &mut Engine, asset_path: &str) -> FontLoadHandle {
		let family = read_font_family_from_path(asset_path);
		self.load_font(engine, &family, asset_path)
	}

	//==============================================================================
	/// 글꼴 읽기 요청. (원본 load_font(family, asset_path). 다 오면 엔진이 family 이름으로 등록한다)
	//==============================================================================
	pub fn load_font(&mut self, engine: &mut Engine, family: &str, asset_path: &str) -> FontLoadHandle {
		self.load_font_with_bold(engine, family, asset_path, false)
	}

	//==============================================================================
	/// 굵은 글꼴 읽기 요청. (원본 descriptors 의 weight 자리)
	//==============================================================================
	pub fn load_font_with_bold(&mut self, engine: &mut Engine, family: &str, asset_path: &str, is_bold: bool) -> FontLoadHandle {
		self.load_font_with_descriptors(engine, family, asset_path, FontFaceDescriptors::create_with_bold(is_bold))
	}

	//==============================================================================
	/// 서술자를 주어 글꼴 읽기 요청. (원본 loadFont(family, assetPath, descriptors), 서술자의 source_key 가 비면 경로를 쓴다)
	/// - 이미 요청했으면 다시 하지 않고 그 손잡이를 돌려준다. (원본도 fontFace 가 있으면 바로 끝난다)
	//==============================================================================
	pub fn load_font_with_descriptors(&mut self, engine: &mut Engine, family: &str, asset_path: &str, mut descriptors: FontFaceDescriptors) -> FontLoadHandle {
		if let Some(font_load_handle) = &self.font_load_handle {
			return font_load_handle.clone();
		}
		if descriptors.source_key.is_empty() {
			descriptors.source_key = asset_path.to_string();
		}
		self.family = family.to_string();
		self.descriptors = descriptors.clone();
		let load_handle = self.asset_base.request_load(engine, asset_path);
		let font_load_handle = engine.register_font_face_when_loaded(family, descriptors, load_handle);
		self.font_load_handle = Some(font_load_handle.clone());
		font_load_handle
	}

	//==============================================================================
	/// 이미 가진 글꼴 바이트로 읽기. (ttf, otf, woff, woff2, 원본 loadFontFromBytes, 엔진이 다음 프레임 시작에 family 이름으로 올린다)
	//==============================================================================
	pub fn load_font_from_bytes(&mut self, engine: &mut Engine, family: &str, bytes: Vec<u8>) -> FontLoadHandle {
		self.load_font_from_bytes_with_descriptors(engine, family, bytes, FontFaceDescriptors::new())
	}

	//==============================================================================
	/// 서술자를 주어 이미 가진 글꼴 바이트로 읽기. (원본 loadFontFromBytes(family, bytes, descriptors))
	//==============================================================================
	pub fn load_font_from_bytes_with_descriptors(&mut self, engine: &mut Engine, family: &str, bytes: Vec<u8>, mut descriptors: FontFaceDescriptors) -> FontLoadHandle {
		if let Some(font_load_handle) = &self.font_load_handle {
			return font_load_handle.clone();
		}
		if descriptors.source_key.is_empty() {
			descriptors.source_key = family.to_string();
		}
		self.family = family.to_string();
		self.descriptors = descriptors.clone();
		let load_handle = self.asset_base.load_from_bytes(&descriptors.source_key, bytes);
		let font_load_handle = engine.register_font_face_when_loaded(family, descriptors, load_handle);
		self.font_load_handle = Some(font_load_handle.clone());
		font_load_handle
	}

	//==============================================================================
	/// 애셋 내리기. (원본 unload: 글꼴 목록에서 이 얼굴을 뺀다)
	//==============================================================================
	pub fn unload(&mut self, engine: &mut Engine) {
		if self.asset_base.get_load_handle().is_none() {
			return;
		}
		engine.unregister_font_face(&self.family, &self.descriptors.source_key);
		self.asset_base.unload();
		self.font_load_handle = None;
	}

	//==============================================================================
	/// 얼굴 서술자.
	//==============================================================================
	pub fn get_descriptors(&self) -> &FontFaceDescriptors {
		&self.descriptors
	}

	//==============================================================================
	/// 글꼴 이름.
	//==============================================================================
	pub fn get_family(&self) -> &str {
		&self.family
	}

	//==============================================================================
	/// 굵은 글꼴인지.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.descriptors.minimum_weight >= crate::core::textshaping::BOLD_WEIGHT_THRESHOLD
	}
}

impl Default for FontAsset {
	fn default() -> FontAsset {
		FontAsset::new()
	}
}

impl Asset for FontAsset {
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}
}

//==============================================================================
/// 글꼴 파일 경로의 패밀리 이름. (쿼리와 조각을 떼고 마지막 경로 조각에서 확장자를 뗀다, 원본 readFontFamilyFromPath)
///
/// # 인자
/// - `asset_path`: 글꼴 파일 경로.
///
/// # 돌려줌
/// 패밀리 이름.
//==============================================================================
fn read_font_family_from_path(asset_path: &str) -> String {
	let path = asset_path.split('?').next().unwrap_or("");
	let path = path.split('#').next().unwrap_or("");
	let file_name = match path.rfind('/') {
		Some(slash_index) => {
			&path[slash_index + 1..]
		}
		None => {
			path
		}
	};
	match file_name.rfind('.') {
		Some(dot_index) if dot_index > 0 => {
			String::from(&file_name[..dot_index])
		}
		_ => {
			String::from(file_name)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;

	//==============================================================================
	/// load 는 파일 이름에서 확장자를 뗀 것을 패밀리 이름으로 글꼴을 읽는다. (원본 load)
	//==============================================================================
	#[test]
	fn load_uses_file_name_as_family() {
		assert_eq!(read_font_family_from_path("examples/assets/DOSGothic.woff"), "DOSGothic");
		assert_eq!(read_font_family_from_path("fonts/Pixel.Bold.ttf?v=2#x"), "Pixel.Bold");
		assert_eq!(read_font_family_from_path("Plain"), "Plain");
		assert_eq!(read_font_family_from_path(".hidden"), ".hidden");
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut font_asset = FontAsset::new();
		font_asset.load(&mut engine, "examples/assets/DOSGothic.woff");
		assert_eq!(font_asset.get_family(), "DOSGothic");
	}

	//==============================================================================
	/// 바이트로 읽으면 바로 읽힌 것이고, 내리면 읽히지 않은 것이 된다. 두 번째 읽기는 무시한다.
	//==============================================================================
	#[test]
	fn load_from_bytes_and_unload() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut font_asset = FontAsset::new();
		font_asset.load_font_from_bytes_with_descriptors(&mut engine, "Pixel", vec![0u8, 1, 0, 0], FontFaceDescriptors::create_with_bold(true));
		assert!(font_asset.is_loaded());
		assert_eq!(font_asset.get_family(), "Pixel");
		assert!(font_asset.is_bold());
		assert_eq!(font_asset.get_descriptors().source_key, "Pixel");
		font_asset.load_font_from_bytes(&mut engine, "Other", vec![0u8]);
		assert_eq!(font_asset.get_family(), "Pixel");
		assert_eq!(engine.get_pending_font_face_count(), 1);
		font_asset.unload(&mut engine);
		assert!(!font_asset.is_loaded());
		assert_eq!(engine.get_pending_font_face_count(), 0);
	}
}
