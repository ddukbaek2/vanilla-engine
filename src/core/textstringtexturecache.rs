//==============================================================================
// 문자열 텍스처 캐시.
// - 원본은 오프스크린 2D 캔버스에 fillText / strokeText 로 문자열을 구워 텍스처로 쓴다. (LRU 상한 512)
// - 여기서는 글자 모양 짓기(textshaping.rs: 양방향 정렬, rustybuzz 모양 짓기)로 글리프를 놓고 fontdue 로 래스터화해 문자열 한 장으로 굽는다.
//   상한과 키 구성(방식, 문단 방향, 폰트 문자열, 선 두께, 배율, 글)은 원본과 같다.
//   원본은 키를 문자열로 이어 붙인다. 여기서는 찾을 때마다 문자열을 만들지 않게 키의 해시로 찾고, 같은 해시 안에서 키를 하나씩 맞춰 본다.
// - 글자는 흰색으로 굽고, 그릴 때 색을 곱한다. (색이 바뀌어도 다시 굽지 않는다)
//   색 글리프(이모지)는 제 색으로 흰 글자 위에 얹어 굽는다. 원본도 흰색으로 fillText 한 캔버스를 올리므로 이모지는 제 색으로 구워지고 그릴 때 같은 색이 곱해진다.
//   외곽선(Stroke)으로 구울 때도 색 글리프는 테를 긋지 않고 색 그대로 얹는다. (크롬 strokeText 와 같다)
// - 폰트 문자열은 Canvas2D font 와 같은 꼴을 읽는다. 예: "16px DOSGothic", "bold 24px default", "italic 500 16px default"
// - 글꼴은 원본 document.fonts 처럼 이름 하나 아래에 얼굴(굵기, 기울기, 글자 범위)을 여럿 올린다. (register_font_face)
//   고르기는 CSS 의 차례를 따르고, 기울인 얼굴이 없으면 가짜 기울임(기울기 1/4), 굵은 얼굴이 없으면 가짜 굵게를 글리프마다 입힌다. (textshaping.rs)
// - 이름의 얼굴들에 없는 글자는 기본 폰트, 대체 폰트 순으로 찾는다. (브라우저의 글꼴 대체와 같은 자리)
//   대체 글꼴 후보 파일은 쓸 때 읽는다. 이모지 표시 글자는 색 글리프 표가 있는 후보를, 그 밖의 글자는 색 글리프 표가 없는 후보를 먼저 읽는다. (표 목록만 먼저 엿본다)
// - 글꼴 얼굴을 새로 올리면 그 이름을 쓰는 폰트 문자열로 구웠고 그 얼굴의 글자 범위에 든 글자가 있는 문자열만 버린다. (원본 invalidateFontFaces)
//   기본 폰트와 대체 폰트를 바꾸면 모두 버린다.
// - 웹에서 글꼴 바이트가 하나도 없으면 원본처럼 브라우저 Canvas2D 로 굽고 잰다. 그래서 시스템 글꼴로 나온다. (platform/web/systemfont.rs)
// - 글자 안티앨리어싱(set_text_antialias_enabled, 원본 setTextAntialiasEnabled): 끄면 구운 글자의 알파를 문턱값 128 로 0 과 255 로 가르고 텍스처를 NEAREST 로 뽑는다.
//   도트 글꼴을 설계 크기의 정수 배로 그리는 프로젝트가 끈다. 원본은 모듈 전역 값이고 이미 구운 글자는 그대로 둔다.
//   여기서는 캐시가 들고, 켜짐 여부를 키에 넣어 바꾼 다음 그리는 글자부터 바로 그 방식으로 굽는다. (rasterize_text 는 원본 ShadowText 처럼 받지 않는다)
// - 글 재기(measure_text)는 폰트 문자열과 글이 같으면 다시 재지 않는다. (원본 measureTextMetrics 의 캐시, 2048 개)
//   원본은 넘칠 때마다 가장 오래 안 쓴 하나를 버린다. 여기서는 넘치면 오래 안 쓴 반을 한 번에 버린다. (하나씩 찾으면 넘친 뒤로 잴 때마다 2048 개를 훑는다)
//   글꼴을 새로 등록하면 비운다. (원본은 웹 글꼴이 받아지면 비운다) measure_text 가 &self 라 캐시는 RefCell 에 든다.
// - Graphic 없이 재기(measure_text_without_graphic): 원본은 어디서든 공용 2D 캔버스로 글을 잰다. 러스트 판은 글꼴을 등록할 때마다
//   글꼴 목록(Rc)을 스레드 하나의 전역에 알려 두고, 그 목록으로 잰다. UI 레이아웃이 그리기 전에 글 줄 수를 알아야 할 때 쓴다. (UIConfirmWindow)
//==============================================================================
use crate::core::textshaping::{self, FontFaceDescriptors, LoadedFont, RegisteredFontFace, ResolvedFontFace, ShapedText, ShapingDirection};
use unicode_segmentation::UnicodeSegmentation;
use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

//==============================================================================
// Graphic 없이 재기에 쓰는 캐시. (마지막으로 글꼴을 등록한 캐시의 얼굴 표를 옮겨 담는다, GL 은 쓰지 않는다)
//==============================================================================
thread_local! {
	static SHARED_MEASURE_CACHE: RefCell<Option<TextStringTextureCache>> = const { RefCell::new(None) };
}

//==============================================================================
// Graphic 없이 글 재기. (폰트 문자열과 글, 원본의 공용 캔버스 measureText 자리)
// - 그릴 때와 같은 얼굴 고르기와 모양 잇기로 잰다.
// - 글꼴 바이트가 없으면 웹은 브라우저 시스템 글꼴로 재고, 데스크탑은 None 이다. (그릴 때도 글이 나오지 않는 상태)
//==============================================================================
pub fn measure_text_without_graphic(font_string: &str, text: &str) -> Option<TextMetrics> {
	SHARED_MEASURE_CACHE.with(|shared_measure_cache_cell| {
		let shared_measure_cache = shared_measure_cache_cell.borrow();
		match shared_measure_cache.as_ref() {
			Some(shared_measure_cache) => {
				let font_description = parse_font_string(font_string);
				if shared_measure_cache.resolve_face_list(&font_description).is_empty() {
					#[cfg(target_arch = "wasm32")]
					{
						return Some(crate::platform::web::systemfont::measure_text(&font_description, text));
					}
					#[cfg(not(target_arch = "wasm32"))]
					{
						return None;
					}
				}
				Some(shared_measure_cache.measure_text(font_string, text))
			}
			None => {
				#[cfg(target_arch = "wasm32")]
				{
					Some(crate::platform::web::systemfont::measure_text(&parse_font_string(font_string), text))
				}
				#[cfg(not(target_arch = "wasm32"))]
				{
					None
				}
			}
		}
	})
}

// 캐시 상한. (원본과 같음)
const MAXIMUM_ENTRY_COUNT: usize = 512;
// 글 재기 캐시 상한. (원본과 같음)
const MAXIMUM_MEASURE_COUNT: usize = 2048;

// 안티앨리어싱을 끈 글자의 알파 문턱값. (원본과 같음)
const ALPHA_THRESHOLD: u8 = 128;

//==============================================================================
// 굽는 방식.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextDrawKind {
	Fill,
	Stroke,
}

//==============================================================================
// 구운 문자열 하나. (길이 단위는 모두 논리 픽셀, 베이크 배율로 나눈 값)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct TextEntry {
	pub texture: crate::core::graphic::GlTexture,
	pub advance_width: f32,
	pub font_ascent: f32,
	pub font_descent: f32,
	pub pen_offset_x: f32,
	pub baseline_offset_y: f32,
	pub quad_width: f32,
	pub quad_height: f32,
}

//==============================================================================
// 글 모양 래스터. (덮인 정도 0 ~ 255, 길이 단위는 굽는 배율을 곱한 픽셀)
// - coverage 는 색 글리프를 뺀 글자의 덮인 정도다. 색 글리프(이모지)가 있으면 color_pixels 에 제 색으로 든다. (프리멀티플라이드 RGBA, 같은 크기)
//==============================================================================
#[derive(Clone, Debug)]
pub struct TextRaster {
	pub coverage: Vec<u8>,
	pub color_pixels: Option<Vec<u8>>,
	pub width: i32,
	pub height: i32,
	pub origin_x: f32,
	pub baseline_y: f32,
	pub advance_width: f32,
	pub font_ascent: f32,
	pub font_descent: f32,
}

//==============================================================================
// 문자열 측정 결과. (Canvas2D TextMetrics)
// - actual_bounding_box_* 는 펜 원점(왼쪽 정렬, 알파벳 기준선)에서 글리프가 실제로 닿는 거리다. (왼쪽, 위, 아래는 그쪽으로 양수)
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct TextMetrics {
	pub width: f32,
	pub font_ascent: f32,
	pub font_descent: f32,
	pub actual_bounding_box_left: f32,
	pub actual_bounding_box_right: f32,
	pub actual_bounding_box_ascent: f32,
	pub actual_bounding_box_descent: f32,
}

//==============================================================================
// 폰트 문자열 해석 결과.
// - font_weight 는 CSS 굵기다. ("bold" 700, 숫자, 없으면 400) is_bold 는 굵기가 600 이상인지다.
//==============================================================================
#[derive(Clone, Debug)]
pub struct FontDescription {
	pub font_size: f32,
	pub font_weight: u16,
	pub is_bold: bool,
	pub is_italic: bool,
	pub family_name: String,
}

struct CachedTextEntry {
	text_draw_kind: TextDrawKind,
	is_antialias: bool,
	direction: ShapingDirection,
	text: String,
	font_string: String,
	line_width: f32,
	bake_scale: f32,
	entry: TextEntry,
	last_used_frame_number: u64,
}

impl CachedTextEntry {
	//==============================================================================
	// 키가 같은지. (선 두께와 배율은 비트로 맞춘다)
	//==============================================================================
	fn is_same_key(&self, text_draw_kind: TextDrawKind, is_antialias: bool, direction: ShapingDirection, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> bool {
		self.text_draw_kind == text_draw_kind && self.is_antialias == is_antialias && self.direction == direction && self.line_width.to_bits() == line_width.to_bits() && self.bake_scale.to_bits() == bake_scale.to_bits() && self.font_string == font_string && self.text == text
	}
}

struct CachedTextMetrics {
	font_string: String,
	text: String,
	text_metrics: TextMetrics,
	last_used_count: u64,
}

//==============================================================================
// 래스터로 놓은 글. (덮인 정도와 자리, 길이 단위는 배율을 곱한 픽셀)
// - coverage 는 색 글리프를 뺀 글자, color_pixels 는 색 글리프 층이다. (프리멀티플라이드 RGBA, 색 글리프가 없으면 None)
//==============================================================================
struct LaidOutText {
	coverage: Vec<u8>,
	color_pixels: Option<Vec<u8>>,
	width: i32,
	height: i32,
	origin_x: f32,
	baseline_y: i32,
	advance_width: f32,
	font_ascent: f32,
	font_descent: f32,
}

pub struct TextStringTextureCache {
	// 이름(소문자) → 그 이름의 얼굴들. (원본 document.fonts)
	font_family_table: HashMap<String, Vec<RegisteredFontFace>>,
	// 이름(소문자) → 얼굴 열쇠(source_key)의 차례. (뒤가 먼저 그린다, set_font_face_order)
	font_face_order_table: HashMap<String, Vec<String>>,
	// 기본 폰트의 얼굴들. (보통, 굵게)
	default_face_list: Vec<RegisteredFontFace>,
	// 대체 폰트들. (앞의 것이 먼저, 후보 파일을 쓸 때 읽어 더하므로 RefCell)
	fallback_face_list: RefCell<Vec<RegisteredFontFace>>,
	// 얼굴을 올린 차례 번호.
	face_order_number: u64,
	// 키의 해시 → 그 해시를 가진 구운 문자열들.
	entry_table: HashMap<u64, Vec<CachedTextEntry>>,
	entry_count: usize,
	frame_number: u64,
	// 글꼴을 등록한 횟수. (늘면 재 둔 글 폭이 바뀌었을 수 있다)
	font_registration_count: Cell<u64>,
	// 글자가 없을 때 차례로 읽어 볼 대체 글꼴 파일. (원본 브라우저의 시스템 글꼴 대체 자리, 데스크탑, 쓸 때 읽는다)
	// 앞의 경로와 그 파일에 색 글리프 표가 있는지. (더할 때 한 번 엿본다)
	fallback_font_candidate_list: RefCell<Vec<(std::path::PathBuf, bool)>>,
	// 버릴 구운 문자열의 텍스처. (등록할 때는 GL 이 없어서 다음 굽기 때 지운다)
	pending_deleted_texture_list: Vec<crate::core::graphic::GlTexture>,
	// 글자 안티앨리어싱. (기본 켬)
	is_text_antialias_enabled: bool,
	// 글 재기 캐시. (폰트 문자열과 글의 해시 → 잰 것들, 쓸 때마다 센 번호, 그때의 글꼴 등록 횟수)
	measure_table: RefCell<HashMap<u64, Vec<CachedTextMetrics>>>,
	measure_count: Cell<usize>,
	measure_use_count: Cell<u64>,
	measured_font_registration_count: Cell<u64>,
}

impl TextStringTextureCache {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TextStringTextureCache {
		TextStringTextureCache {
			font_family_table: HashMap::new(),
			font_face_order_table: HashMap::new(),
			default_face_list: Vec::new(),
			fallback_face_list: RefCell::new(Vec::new()),
			face_order_number: 0,
			entry_table: HashMap::new(),
			entry_count: 0,
			frame_number: 0,
			font_registration_count: Cell::new(0),
			fallback_font_candidate_list: RefCell::new(Vec::new()),
			pending_deleted_texture_list: Vec::new(),
			is_text_antialias_enabled: true,
			measure_table: RefCell::new(HashMap::new()),
			measure_count: Cell::new(0),
			measure_use_count: Cell::new(0),
			measured_font_registration_count: Cell::new(0),
		}
	}

	//==============================================================================
	// 프레임 시작. (LRU 순서를 프레임 번호로 센다)
	//==============================================================================
	pub fn begin_frame(&mut self) {
		self.frame_number += 1;
	}

	//==============================================================================
	// 폰트 등록. (폰트 문자열의 이름으로 찾는다. 대소문자 구분 없음, 굵으면 굵기 700 얼굴, 아니면 400)
	//==============================================================================
	pub fn register_font(&mut self, family_name: &str, is_bold: bool, font_bytes: Vec<u8>) -> Result<(), String> {
		self.register_font_face(family_name, FontFaceDescriptors::create_with_bold(is_bold), font_bytes)
	}

	//==============================================================================
	// 글꼴 얼굴 등록. (원본 new FontFace(family, url, descriptors) 를 document.fonts 에 더하기)
	// - 같은 이름에 같은 열쇠(source_key), 굵기, 기울기의 얼굴이 있으면 바꾼다. 없으면 맨 뒤(가장 먼저 그리는 자리)에 둔다.
	//   set_font_face_order 로 차례를 정한 이름이면 그 차례에 맞춰 놓는다.
	// - 그 이름을 쓰는 폰트 문자열로 구웠고 이 얼굴의 글자 범위에 든 글자가 있는 문자열은 버린다. (다음에 그릴 때 이 얼굴로 다시 굽는다)
	//==============================================================================
	pub fn register_font_face(&mut self, family_name: &str, font_face_descriptors: FontFaceDescriptors, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(load_font(font_bytes)?);
		let family_key = family_name.trim().trim_matches(|character| character == '"' || character == '\'').to_lowercase();
		self.face_order_number += 1;
		let order_number = self.face_order_number;
		let face_list = self.font_family_table.entry(family_key.clone()).or_default();
		face_list.retain(|face| !(face.descriptors.source_key == font_face_descriptors.source_key && face.descriptors.minimum_weight == font_face_descriptors.minimum_weight && face.descriptors.maximum_weight == font_face_descriptors.maximum_weight && face.descriptors.is_italic == font_face_descriptors.is_italic));
		face_list.push(RegisteredFontFace {
			descriptors: font_face_descriptors.clone(),
			font,
			order_number,
		});
		self.apply_font_face_order(&family_key);
		self.invalidate_entries_for_face(&family_key, &font_face_descriptors);
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	// 글꼴 얼굴 지우기. (원본 document.fonts.delete, 열쇠가 같은 얼굴 모두, 지운 것이 있으면 true)
	//==============================================================================
	pub fn unregister_font_face(&mut self, family_name: &str, source_key: &str) -> bool {
		let family_key = family_name.trim().to_lowercase();
		let mut removed_descriptor_list: Vec<FontFaceDescriptors> = Vec::new();
		if let Some(face_list) = self.font_family_table.get_mut(&family_key) {
			face_list.retain(|face| {
				if face.descriptors.source_key == source_key {
					removed_descriptor_list.push(face.descriptors.clone());
					return false;
				}
				true
			});
		}
		for removed_descriptors in &removed_descriptor_list {
			self.invalidate_entries_for_face(&family_key, removed_descriptors);
		}
		if removed_descriptor_list.is_empty() {
			return false;
		}
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		true
	}

	//==============================================================================
	// 한 이름의 얼굴 차례 정하기. (얼굴 열쇠 목록, 뒤가 먼저 그린다. 원본 uifont 가 document.fonts 에서 빼고 차례대로 다시 더하는 것)
	// - 목록에 없는 얼굴은 목록의 얼굴보다 뒤로(나중에 그리게) 밀리지 않고 앞에 남는다. 아직 받지 않은 얼굴의 열쇠도 넣어 둘 수 있다. (받으면 그 자리에 놓인다)
	// - 차례가 바뀌면 그 이름으로 구운 문자열을 모두 버린다.
	//==============================================================================
	pub fn set_font_face_order(&mut self, family_name: &str, source_key_list: &[String]) {
		let family_key = family_name.trim().to_lowercase();
		self.font_face_order_table.insert(family_key.clone(), source_key_list.to_vec());
		self.apply_font_face_order(&family_key);
		let all_descriptors = FontFaceDescriptors::new();
		self.invalidate_entries_for_face(&family_key, &all_descriptors);
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	// 정한 차례대로 얼굴의 차례 번호 다시 매기기.
	//==============================================================================
	fn apply_font_face_order(&mut self, family_key: &str) {
		let source_key_list = match self.font_face_order_table.get(family_key) {
			Some(source_key_list) => {
				source_key_list
			}
			None => {
				return;
			}
		};
		let face_list = match self.font_family_table.get_mut(family_key) {
			Some(face_list) => {
				face_list
			}
			None => {
				return;
			}
		};
		// 목록에 있는 얼굴은 목록 차례 뒤로, 없는 얼굴은 지금 차례 그대로 앞에.
		face_list.sort_by_key(|face| {
			match source_key_list.iter().position(|source_key| *source_key == face.descriptors.source_key) {
				Some(position) => {
					(1, position as u64, face.order_number)
				}
				None => {
					(0, 0, face.order_number)
				}
			}
		});
		for (face_index, face) in face_list.iter_mut().enumerate() {
			face.order_number = face_index as u64 + 1;
		}
		self.face_order_number = self.face_order_number.max(face_list.len() as u64);
	}

	//==============================================================================
	// 얼굴이 바뀐 이름으로 구운 문자열 버리기. (그 얼굴의 글자 범위에 든 글자가 있는 것만, 텍스처는 다음 굽기 때 지운다)
	//==============================================================================
	fn invalidate_entries_for_face(&mut self, family_key: &str, font_face_descriptors: &FontFaceDescriptors) {
		let pending_deleted_texture_list = &mut self.pending_deleted_texture_list;
		let mut removed_count = 0;
		self.entry_table.retain(|_, cached_text_entry_list| {
			cached_text_entry_list.retain(|cached_text_entry| {
				let font_description = parse_font_string(&cached_text_entry.font_string);
				let is_affected = font_description.family_name.to_lowercase() == family_key && font_face_descriptors.contains_any_character(&cached_text_entry.text);
				if is_affected {
					pending_deleted_texture_list.push(cached_text_entry.entry.texture);
					removed_count += 1;
				}
				!is_affected
			});
			!cached_text_entry_list.is_empty()
		});
		self.entry_count -= removed_count;
	}

	//==============================================================================
	// 구운 문자열 모두 버리기. (원본 clear, 텍스처는 다음 굽기 때 지운다)
	//==============================================================================
	pub fn clear(&mut self) {
		for (_, cached_text_entry_list) in self.entry_table.drain() {
			for cached_text_entry in cached_text_entry_list {
				self.pending_deleted_texture_list.push(cached_text_entry.entry.texture);
			}
		}
		self.entry_count = 0;
	}

	//==============================================================================
	// 기본 폰트 설정. (이름을 찾지 못한 폰트 문자열과 글자 대체에 쓴다, 굵으면 굵기 700 얼굴)
	//==============================================================================
	pub fn set_default_font(&mut self, is_bold: bool, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(load_font(font_bytes)?);
		let font_face_descriptors = FontFaceDescriptors::create_with_bold(is_bold);
		self.default_face_list.retain(|face| face.descriptors.is_italic || face.descriptors.minimum_weight != font_face_descriptors.minimum_weight);
		self.face_order_number += 1;
		self.default_face_list.push(RegisteredFontFace {
			descriptors: font_face_descriptors,
			font,
			order_number: self.face_order_number,
		});
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	// 대체 폰트 추가. (앞 폰트들에 없는 글자를 찾는다)
	//==============================================================================
	pub fn add_fallback_font(&mut self, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(load_font(font_bytes)?);
		self.face_order_number += 1;
		self.fallback_face_list.borrow_mut().push(RegisteredFontFace {
			descriptors: FontFaceDescriptors::new(),
			font,
			order_number: self.face_order_number,
		});
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	// 글꼴 목록을 Graphic 없이 재기에 알리기. (measure_text_without_graphic)
	//==============================================================================
	fn publish_shared_font_set(&self) {
		let mut shared_measure_cache = TextStringTextureCache::new();
		shared_measure_cache.font_family_table = self.font_family_table.clone();
		shared_measure_cache.font_face_order_table = self.font_face_order_table.clone();
		shared_measure_cache.default_face_list = self.default_face_list.clone();
		shared_measure_cache.fallback_face_list = RefCell::new(self.fallback_face_list.borrow().clone());
		shared_measure_cache.fallback_font_candidate_list = RefCell::new(self.fallback_font_candidate_list.borrow().clone());
		SHARED_MEASURE_CACHE.with(|shared_measure_cache_cell| {
			*shared_measure_cache_cell.borrow_mut() = Some(shared_measure_cache);
		});
	}

	//==============================================================================
	// 글자 안티앨리어싱 켬 / 끔. (원본 setTextAntialiasEnabled, 바꾼 다음 그리는 글자부터 그 방식으로 굽는다)
	//==============================================================================
	pub fn set_text_antialias_enabled(&mut self, value: bool) {
		self.is_text_antialias_enabled = value;
	}

	//==============================================================================
	// 글자 안티앨리어싱이 켜져 있는지. (원본 isTextAntialiasOn)
	//==============================================================================
	pub fn is_text_antialias_on(&self) -> bool {
		self.is_text_antialias_enabled
	}

	//==============================================================================
	// 글꼴을 등록한 횟수. (등록, 지우기, 차례, 기본 글꼴, 대체 글꼴. 재 둔 글 폭을 다시 잴지 가린다)
	//==============================================================================
	pub fn get_font_registration_count(&self) -> u64 {
		self.font_registration_count.get()
	}

	//==============================================================================
	// 대체 글꼴 후보 파일 더하기. (데스크탑, 원본 브라우저가 글꼴에 없는 글자를 시스템 글꼴로 그리는 자리)
	// - 바로 읽지 않는다. 등록된 어느 글꼴에도 없는 글자를 재거나 구울 때 앞의 후보부터 읽어 대체 폰트로 더한다. (큰 한중일 글꼴을 처음에 읽지 않게)
	// - 없는 파일, 읽지 못하는 파일은 넘긴다.
	//==============================================================================
	pub fn add_fallback_font_candidate(&mut self, font_path: std::path::PathBuf) {
		let has_color_tables = peek_color_font_tables(&font_path);
		self.fallback_font_candidate_list.borrow_mut().push((font_path, has_color_tables));
	}

	//==============================================================================
	// 글의 글자가 모두 그려지게 대체 글꼴 후보 읽기. (어느 글꼴에도 없는 글자가 있으면 그 글자를 가진 후보가 나올 때까지)
	// - 이모지 표시 글자 묶음의 첫 글자는 색 글리프로, 그 밖의 글자는 색이 아닌 글리프로 가진 글꼴을 찾는다. (브라우저가 이모지를 색 이모지 글꼴로, 맨 ❤ 는 글 글꼴로 그리는 자리)
	//   바라는 쪽의 후보가 다 떨어졌으면 다른 쪽이라도 가진 글꼴이 있으면 그것으로 그린다.
	//==============================================================================
	fn ensure_fallback_coverage(&self, text: &str) {
		if self.fallback_font_candidate_list.borrow().is_empty() {
			return;
		}
		for grapheme in text.graphemes(true) {
			let is_emoji_grapheme = textshaping::is_emoji_presentation(grapheme);
			for (character_index, character) in grapheme.chars().enumerate() {
				if character.is_whitespace() || character.is_control() || ('\u{200B}'..='\u{200F}').contains(&character) || ('\u{2028}'..='\u{206F}').contains(&character) || ('\u{FE00}'..='\u{FE0F}').contains(&character) {
					continue;
				}
				let is_color_wanted = is_emoji_grapheme && character_index == 0;
				loop {
					let is_wanted_covered = if is_color_wanted {
						self.is_color_character_covered(character)
					}
					else {
						self.is_text_character_covered(character)
					};
					if is_wanted_covered {
						break;
					}
					let has_wanted_candidate = self.fallback_font_candidate_list.borrow().iter().any(|(_, has_color_tables)| *has_color_tables == is_color_wanted);
					if !has_wanted_candidate && self.is_character_covered(character) {
						break;
					}
					if !self.load_next_fallback_candidate(is_color_wanted) {
						return;
					}
				}
			}
		}
	}

	//==============================================================================
	// 대체 글꼴 후보 하나 읽어 대체 폰트로 더하기. (색을 바라면 색 글리프 표가 있는 후보, 아니면 없는 후보부터. 후보가 없으면 false)
	// - 못 읽는 파일은 버리고 다음 후보를 본다.
	//==============================================================================
	fn load_next_fallback_candidate(&self, is_color_wanted: bool) -> bool {
		loop {
			let font_path = {
				let mut fallback_font_candidate_list = self.fallback_font_candidate_list.borrow_mut();
				if fallback_font_candidate_list.is_empty() {
					return false;
				}
				let preferred_index = fallback_font_candidate_list.iter().position(|(_, has_color_tables)| *has_color_tables == is_color_wanted).unwrap_or(0);
				fallback_font_candidate_list.remove(preferred_index).0
			};
			let font = match std::fs::read(&font_path).map_err(|error| error.to_string()).and_then(load_font) {
				Ok(font) => {
					Rc::new(font)
				}
				Err(_) => {
					continue;
				}
			};
			self.fallback_face_list.borrow_mut().push(RegisteredFontFace {
				descriptors: FontFaceDescriptors::new(),
				font,
				order_number: 0,
			});
			self.font_registration_count.set(self.font_registration_count.get() + 1);
			self.publish_shared_font_set();
			return true;
		}
	}

	//==============================================================================
	// 등록된 어느 글꼴이 그 글자를 가졌는지.
	//==============================================================================
	fn is_character_covered(&self, character: char) -> bool {
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && face.font.has_character(character)));
		is_in_family || self.default_face_list.iter().any(|face| face.font.has_character(character)) || self.fallback_face_list.borrow().iter().any(|face| face.font.has_character(character))
	}

	//==============================================================================
	// 등록된 어느 글꼴이 그 글자를 색이 아닌 글리프로 가졌는지.
	//==============================================================================
	fn is_text_character_covered(&self, character: char) -> bool {
		let has_text_character = |face: &RegisteredFontFace| face.font.has_character(character) && !face.font.has_color_character(character);
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && has_text_character(face)));
		is_in_family || self.default_face_list.iter().any(has_text_character) || self.fallback_face_list.borrow().iter().any(has_text_character)
	}

	//==============================================================================
	// 등록된 어느 글꼴이 그 글자를 색 글리프로 가졌는지.
	//==============================================================================
	fn is_color_character_covered(&self, character: char) -> bool {
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && face.font.has_color_character(character)));
		is_in_family || self.default_face_list.iter().any(|face| face.font.has_color_character(character)) || self.fallback_face_list.borrow().iter().any(|face| face.font.has_color_character(character))
	}

	//==============================================================================
	// 기본 폰트가 있는지.
	//==============================================================================
	pub fn has_default_font(&self) -> bool {
		!self.default_face_list.is_empty()
	}

	//==============================================================================
	// 이름에 올린 얼굴 수. (그 이름이 없으면 0)
	//==============================================================================
	pub fn get_font_face_count(&self, family_name: &str) -> usize {
		self.font_family_table.get(&family_name.trim().to_lowercase()).map(|face_list| face_list.len()).unwrap_or(0)
	}

	//==============================================================================
	// 문자열 측정. (문단 방향은 왼쪽에서 오른쪽, 원본 measureTextMetrics 도 방향을 주지 않는다)
	//==============================================================================
	pub fn measure_text(&self, font_string: &str, text: &str) -> TextMetrics {
		// 글꼴이 새로 들어왔으면 잰 것을 비운다.
		if self.measured_font_registration_count.get() != self.font_registration_count.get() {
			self.measure_table.borrow_mut().clear();
			self.measure_count.set(0);
			self.measured_font_registration_count.set(self.font_registration_count.get());
		}
		let use_count = self.measure_use_count.get() + 1;
		self.measure_use_count.set(use_count);
		let mut hasher = DefaultHasher::new();
		font_string.hash(&mut hasher);
		text.hash(&mut hasher);
		let measure_key = hasher.finish();
		if let Some(cached_list) = self.measure_table.borrow_mut().get_mut(&measure_key) {
			for cached in cached_list.iter_mut() {
				if cached.font_string == font_string && cached.text == text {
					cached.last_used_count = use_count;
					return cached.text_metrics;
				}
			}
		}
		let text_metrics = self.measure_text_uncached(font_string, text);
		if self.measure_count.get() >= MAXIMUM_MEASURE_COUNT {
			self.evict_old_measures();
		}
		self.measure_table.borrow_mut().entry(measure_key).or_default().push(CachedTextMetrics {
			font_string: font_string.to_string(),
			text: text.to_string(),
			text_metrics,
			last_used_count: use_count,
		});
		self.measure_count.set(self.measure_count.get() + 1);
		text_metrics
	}

	//==============================================================================
	// 잰 것 가운데 오래 안 쓴 반 버리기. (쓴 번호가 가운데보다 작은 것)
	//==============================================================================
	fn evict_old_measures(&self) {
		let mut measure_table = self.measure_table.borrow_mut();
		let mut last_used_list: Vec<u64> = measure_table.values().flat_map(|cached_list| cached_list.iter().map(|cached| cached.last_used_count)).collect();
		if last_used_list.is_empty() {
			return;
		}
		let middle_index = last_used_list.len() / 2;
		let (_, middle_count, _) = last_used_list.select_nth_unstable(middle_index);
		let threshold_count = *middle_count;
		let mut remain_count = 0;
		measure_table.retain(|_, cached_list| {
			cached_list.retain(|cached| cached.last_used_count >= threshold_count);
			remain_count += cached_list.len();
			!cached_list.is_empty()
		});
		self.measure_count.set(remain_count);
	}

	//==============================================================================
	// 잰 글 수. (캐시에 든 것, 시험과 통계용)
	//==============================================================================
	pub fn get_measure_count(&self) -> usize {
		self.measure_count.get()
	}

	//==============================================================================
	// 문자열 측정. (캐시를 거치지 않는다)
	//==============================================================================
	fn measure_text_uncached(&self, font_string: &str, text: &str) -> TextMetrics {
		self.ensure_fallback_coverage(text);
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_face_list(&font_description);
		if face_list.is_empty() {
			// 웹: 글꼴 바이트가 없으면 브라우저 시스템 글꼴로 잰다.
			#[cfg(target_arch = "wasm32")]
			{
				return crate::platform::web::systemfont::measure_text(&font_description, text);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return TextMetrics::default();
			}
		}
		let font_size = font_description.font_size;
		let system_font_measure = create_system_font_measure(&font_description, font_size);
		let shaped_text = textshaping::shape_text(&face_list, text, font_size, ShapingDirection::LeftToRight, system_font_measure.as_deref());
		let (font_ascent, font_descent) = read_line_metrics(&face_list[0].font, font_size);
		let mut text_metrics = TextMetrics {
			width: shaped_text.advance_width,
			font_ascent,
			font_descent,
			actual_bounding_box_left: 0.0,
			actual_bounding_box_right: 0.0,
			actual_bounding_box_ascent: 0.0,
			actual_bounding_box_descent: 0.0,
		};
		let mut is_first_glyph = true;
		for shaped_glyph in &shaped_text.glyph_list {
			if shaped_glyph.face_index == textshaping::SYSTEM_FACE_INDEX {
				continue;
			}
			let face = &face_list[shaped_glyph.face_index];
			let metrics = face.font.raster_font.metrics_indexed(shaped_glyph.glyph_index, font_size);
			if metrics.width == 0 || metrics.height == 0 {
				continue;
			}
			let mut left_x = shaped_glyph.position_x + metrics.xmin as f32;
			let mut right_x = left_x + metrics.width as f32;
			let mut top_y = shaped_glyph.position_y + (metrics.ymin + metrics.height as i32) as f32;
			let mut bottom_y = shaped_glyph.position_y + metrics.ymin as f32;
			if face.is_synthetic_bold {
				let radius = textshaping::calculate_synthetic_bold_radius(font_size);
				left_x -= radius;
				right_x += radius;
				top_y += radius;
				bottom_y -= radius;
			}
			if face.is_synthetic_italic {
				right_x += top_y.max(0.0) * textshaping::SYNTHETIC_ITALIC_SKEW;
				left_x += bottom_y.min(0.0) * textshaping::SYNTHETIC_ITALIC_SKEW;
			}
			if is_first_glyph {
				text_metrics.actual_bounding_box_left = -left_x;
				text_metrics.actual_bounding_box_right = right_x;
				text_metrics.actual_bounding_box_ascent = top_y;
				text_metrics.actual_bounding_box_descent = -bottom_y;
				is_first_glyph = false;
			}
			else {
				text_metrics.actual_bounding_box_left = text_metrics.actual_bounding_box_left.max(-left_x);
				text_metrics.actual_bounding_box_right = text_metrics.actual_bounding_box_right.max(right_x);
				text_metrics.actual_bounding_box_ascent = text_metrics.actual_bounding_box_ascent.max(top_y);
				text_metrics.actual_bounding_box_descent = text_metrics.actual_bounding_box_descent.max(-bottom_y);
			}
		}
		text_metrics
	}

	//==============================================================================
	// 구운 문자열 반환. (없으면 굽는다, direction 은 문단 방향. 원본 getEntry 의 textDirection)
	//==============================================================================
	pub fn get_entry(&mut self, gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<TextEntry> {
		if text.is_empty() {
			return None;
		}
		self.delete_pending_textures(gl);
		if let Some(entry) = self.find_entry(text_draw_kind, text, font_string, line_width, bake_scale, direction) {
			return Some(entry);
		}

		let entry = self.bake_entry(gl, text_draw_kind, text, font_string, line_width, bake_scale, direction)?;
		if self.entry_count >= MAXIMUM_ENTRY_COUNT {
			self.evict_oldest_entry(gl);
		}
		let is_antialias = self.is_text_antialias_enabled;
		let entry_hash = create_entry_hash(text_draw_kind, is_antialias, direction, text, font_string, line_width, bake_scale);
		self.entry_table.entry(entry_hash).or_default().push(CachedTextEntry {
			text_draw_kind,
			is_antialias,
			direction,
			text: text.to_string(),
			font_string: font_string.to_string(),
			line_width,
			bake_scale,
			entry,
			last_used_frame_number: self.frame_number,
		});
		self.entry_count += 1;
		Some(entry)
	}

	//==============================================================================
	// 버리기로 한 텍스처 지우기.
	//==============================================================================
	fn delete_pending_textures(&mut self, gl: &glow::Context) {
		for texture in self.pending_deleted_texture_list.drain(..) {
			unsafe {
				gl.delete_texture(texture);
			}
		}
	}

	//==============================================================================
	// 버리기로 한 텍스처가 있는지. (있으면 굽는 길로 가서 지운다)
	//==============================================================================
	pub(crate) fn has_pending_deleted_textures(&self) -> bool {
		!self.pending_deleted_texture_list.is_empty()
	}

	//==============================================================================
	// 이미 구운 문자열 찾기. (GL 을 건드리지 않는다, 없으면 None)
	//==============================================================================
	pub(crate) fn find_entry(&mut self, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<TextEntry> {
		let is_antialias = self.is_text_antialias_enabled;
		let entry_hash = create_entry_hash(text_draw_kind, is_antialias, direction, text, font_string, line_width, bake_scale);
		let frame_number = self.frame_number;
		let cached_text_entry_list = self.entry_table.get_mut(&entry_hash)?;
		let cached_text_entry = cached_text_entry_list.iter_mut().find(|cached_text_entry| cached_text_entry.is_same_key(text_draw_kind, is_antialias, direction, text, font_string, line_width, bake_scale))?;
		cached_text_entry.last_used_frame_number = frame_number;
		Some(cached_text_entry.entry)
	}

	//==============================================================================
	// 가장 오래 안 쓴 문자열 하나 버리기.
	//==============================================================================
	fn evict_oldest_entry(&mut self, gl: &glow::Context) {
		let mut oldest_position: Option<(u64, usize, u64)> = None;
		for (entry_hash, cached_text_entry_list) in &self.entry_table {
			for (entry_index, cached_text_entry) in cached_text_entry_list.iter().enumerate() {
				let is_older = match oldest_position {
					Some((_, _, oldest_frame_number)) => {
						cached_text_entry.last_used_frame_number < oldest_frame_number
					}
					None => {
						true
					}
				};
				if is_older {
					oldest_position = Some((*entry_hash, entry_index, cached_text_entry.last_used_frame_number));
				}
			}
		}
		let (entry_hash, entry_index, _) = match oldest_position {
			Some(oldest_position) => {
				oldest_position
			}
			None => {
				return;
			}
		};
		if let Some(cached_text_entry_list) = self.entry_table.get_mut(&entry_hash) {
			let cached_text_entry = cached_text_entry_list.swap_remove(entry_index);
			unsafe {
				gl.delete_texture(cached_text_entry.entry.texture);
			}
			if cached_text_entry_list.is_empty() {
				self.entry_table.remove(&entry_hash);
			}
			self.entry_count -= 1;
		}
	}

	//==============================================================================
	// 글의 모양만 래스터로 받기. (텍스처로 올리지 않는다, 그림자를 깐 글처럼 따로 굽는 쪽이 쓴다)
	// - 웹에서 글꼴 바이트가 없으면 브라우저 시스템 글꼴로 받는다. 데스크탑에서 글꼴이 하나도 없으면 None.
	//==============================================================================
	pub fn rasterize_text(&self, text: &str, font_string: &str, bake_scale: f32, padding: i32, direction: ShapingDirection) -> Option<TextRaster> {
		if text.is_empty() {
			return None;
		}
		self.ensure_fallback_coverage(text);
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_face_list(&font_description);
		if face_list.is_empty() {
			// 웹: 글꼴 바이트가 없으면 브라우저 시스템 글꼴의 덮인 정도를 받는다.
			#[cfg(target_arch = "wasm32")]
			{
				return rasterize_system_font_text(&font_description, text, bake_scale, padding.max(1), direction);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}
		let laid_out_text = lay_out_text(&face_list, &font_description, text, font_description.font_size * bake_scale, direction, padding.max(1))?;
		Some(TextRaster {
			coverage: laid_out_text.coverage,
			color_pixels: laid_out_text.color_pixels,
			width: laid_out_text.width,
			height: laid_out_text.height,
			origin_x: laid_out_text.origin_x,
			baseline_y: laid_out_text.baseline_y as f32,
			advance_width: laid_out_text.advance_width,
			font_ascent: laid_out_text.font_ascent,
			font_descent: laid_out_text.font_descent,
		})
	}

	//==============================================================================
	// 글리프 하나 래스터. (글자 아틀라스가 쓴다, 글리프 번호는 글자로 찾는다. 웹에서 글꼴 바이트가 없으면 브라우저 시스템 글꼴로, 데스크탑에서 글꼴이 하나도 없으면 None)
	// - 돌려주는 것: (래스터, 전진폭, 글꼴 위 길이, 글꼴 아래 길이) 길이 단위는 픽셀.
	//==============================================================================
	pub fn rasterize_character(&self, character: &str, font_string: &str) -> Option<(textshaping::GlyphRaster, f32, f32, f32)> {
		self.ensure_fallback_coverage(character);
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_face_list(&font_description);
		let font_size = font_description.font_size;
		if face_list.is_empty() {
			// 웹: 글꼴 바이트가 없으면 브라우저 시스템 글꼴로 굽는다.
			#[cfg(target_arch = "wasm32")]
			{
				let glyph_raster = crate::platform::web::systemfont::rasterize_piece(&font_description, font_size, character, false)?;
				let text_metrics = crate::platform::web::systemfont::measure_text(&font_description, character);
				return Some((glyph_raster, text_metrics.width, text_metrics.font_ascent, text_metrics.font_descent));
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}
		let system_font_measure = create_system_font_measure(&font_description, font_size);
		let shaped_text = textshaping::shape_text(&face_list, character, font_size, ShapingDirection::LeftToRight, system_font_measure.as_deref());
		let (font_ascent, font_descent) = read_line_metrics(&face_list[0].font, font_size);
		// 여러 글리프로 지어지는 글자 묶음(결합 부호)도 한 장에 모은다.
		let mut minimum_x = i32::MAX;
		let mut maximum_x = i32::MIN;
		let mut maximum_top = i32::MIN;
		let mut minimum_bottom = i32::MAX;
		let mut glyph_raster_list: Vec<(textshaping::GlyphRaster, i32, i32)> = Vec::new();
		for shaped_glyph in &shaped_text.glyph_list {
			let glyph_raster = match rasterize_shaped_glyph(&face_list, &font_description, character, shaped_glyph, font_size) {
				Some(glyph_raster) => {
					glyph_raster
				}
				None => {
					continue;
				}
			};
			let left_x = shaped_glyph.position_x.round() as i32 + glyph_raster.left_x;
			let top_y = shaped_glyph.position_y.round() as i32 + glyph_raster.top_y;
			minimum_x = minimum_x.min(left_x);
			maximum_x = maximum_x.max(left_x + glyph_raster.width);
			maximum_top = maximum_top.max(top_y);
			minimum_bottom = minimum_bottom.min(top_y - glyph_raster.height);
			glyph_raster_list.push((glyph_raster, left_x, top_y));
		}
		if glyph_raster_list.is_empty() {
			let empty_raster = textshaping::GlyphRaster {
				coverage: Vec::new(),
				width: 0,
				height: 0,
				left_x: 0,
				top_y: 0,
				color_pixels: None,
			};
			return Some((empty_raster, shaped_text.advance_width, font_ascent, font_descent));
		}
		let width = maximum_x - minimum_x;
		let height = maximum_top - minimum_bottom;
		let mut coverage = vec![0u8; (width * height) as usize];
		let mut color_pixels: Option<Vec<u8>> = None;
		for (glyph_raster, left_x, top_y) in &glyph_raster_list {
			let offset_x = left_x - minimum_x;
			let offset_y = maximum_top - top_y;
			if let Some(glyph_color_pixels) = glyph_raster.color_pixels.as_ref() {
				let layer_pixels = color_pixels.get_or_insert_with(|| vec![0u8; (width * height * 4) as usize]);
				draw_color_pixels(layer_pixels, width, height, glyph_color_pixels, glyph_raster.width, glyph_raster.height, offset_x, offset_y);
			}
			for glyph_y in 0..glyph_raster.height {
				for glyph_x in 0..glyph_raster.width {
					let index = ((offset_y + glyph_y) * width + offset_x + glyph_x) as usize;
					coverage[index] = coverage[index].max(glyph_raster.coverage[(glyph_y * glyph_raster.width + glyph_x) as usize]);
				}
			}
		}
		let glyph_raster = textshaping::GlyphRaster {
			coverage,
			width,
			height,
			left_x: minimum_x,
			top_y: maximum_top,
			color_pixels,
		};
		Some((glyph_raster, shaped_text.advance_width, font_ascent, font_descent))
	}

	//==============================================================================
	// 구운 문자열 수.
	//==============================================================================
	pub fn get_entry_count(&self) -> usize {
		self.entry_count
	}

	//==============================================================================
	// 폰트 문자열에 맞는 얼굴 목록. (이름의 얼굴들, 기본 폰트, 대체 폰트 차례. 글자마다 앞에서부터 찾는다)
	//==============================================================================
	fn resolve_face_list(&self, font_description: &FontDescription) -> Vec<ResolvedFontFace> {
		let mut face_list: Vec<ResolvedFontFace> = Vec::new();
		let family_key = font_description.family_name.to_lowercase();
		if let Some(family_face_list) = self.font_family_table.get(&family_key) {
			let family_face_reference_list: Vec<&RegisteredFontFace> = family_face_list.iter().collect();
			textshaping::select_font_faces(&family_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		}
		let default_face_reference_list: Vec<&RegisteredFontFace> = self.default_face_list.iter().collect();
		textshaping::select_font_faces(&default_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		for fallback_face in self.fallback_face_list.borrow().iter() {
			let fallback_face_reference_list = [fallback_face];
			textshaping::select_font_faces(&fallback_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		}
		face_list
	}

	//==============================================================================
	// 문자열 굽기.
	//==============================================================================
	fn bake_entry(&self, gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<TextEntry> {
		self.ensure_fallback_coverage(text);
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_face_list(&font_description);
		if face_list.is_empty() {
			// 웹: 글꼴 바이트가 없으면 브라우저 시스템 글꼴로 굽는다.
			#[cfg(target_arch = "wasm32")]
			{
				return bake_system_font_entry(gl, text_draw_kind, text, &font_description, line_width, bake_scale, self.is_text_antialias_enabled, direction);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}

		// 비트맵 여백. (외곽선 두께만큼)
		let stroke_radius = match text_draw_kind {
			TextDrawKind::Stroke => {
				(line_width * bake_scale * 0.5).max(0.5)
			}
			TextDrawKind::Fill => {
				0.0
			}
		};
		let padding = stroke_radius.ceil() as i32 + 1;
		let pixel_size = font_description.font_size * bake_scale;
		let laid_out_text = lay_out_text(&face_list, &font_description, text, pixel_size, direction, padding)?;
		let mut coverage = laid_out_text.coverage;
		let mut color_pixels = laid_out_text.color_pixels;
		let bitmap_width = laid_out_text.width;
		let bitmap_height = laid_out_text.height;

		// 외곽선: 윤곽 바깥으로 반두께 넓힌 것에서 안으로 반두께 좁힌 것을 뺀 띠. (Canvas2D strokeText 는 윤곽선 가운데에 선을 긋는다, 색 글리프는 테 없이 그대로)
		if text_draw_kind == TextDrawKind::Stroke {
			coverage = build_stroke_coverage(&coverage, bitmap_width, bitmap_height, stroke_radius);
		}

		// 안티앨리어싱을 끈 경우: 문턱값으로 갈라 가장자리의 반투명 픽셀을 없앤다.
		let is_antialias = self.is_text_antialias_enabled;
		if !is_antialias {
			apply_alpha_threshold(&mut coverage);
			if let Some(color_pixels) = color_pixels.as_mut() {
				apply_premultiplied_alpha_threshold(color_pixels);
			}
		}

		// 흰색 프리멀티플라이드 RGBA 위에 색 글리프를 얹어 올리기.
		let pixels = compose_text_pixels(&coverage, color_pixels.as_deref());
		let texture = upload_text_texture(gl, bitmap_width, bitmap_height, &pixels, is_antialias)?;

		Some(TextEntry {
			texture,
			advance_width: laid_out_text.advance_width / bake_scale,
			font_ascent: laid_out_text.font_ascent / bake_scale,
			font_descent: laid_out_text.font_descent / bake_scale,
			pen_offset_x: laid_out_text.origin_x / bake_scale,
			baseline_offset_y: laid_out_text.baseline_y as f32 / bake_scale,
			quad_width: bitmap_width as f32 / bake_scale,
			quad_height: bitmap_height as f32 / bake_scale,
		})
	}
}

impl Default for TextStringTextureCache {
	fn default() -> TextStringTextureCache {
		TextStringTextureCache::new()
	}
}

//==============================================================================
// 글을 모양 짓고 글리프를 래스터해 한 장에 놓기. (외곽선, 그림자 몫의 여백 padding)
//==============================================================================
fn lay_out_text(face_list: &[ResolvedFontFace], font_description: &FontDescription, text: &str, pixel_size: f32, direction: ShapingDirection, padding: i32) -> Option<LaidOutText> {
	let system_font_measure = create_system_font_measure(font_description, pixel_size);
	let shaped_text: ShapedText = textshaping::shape_text(face_list, text, pixel_size, direction, system_font_measure.as_deref());
	let advance_width = shaped_text.advance_width;
	let (font_ascent, font_descent) = read_line_metrics(&face_list[0].font, pixel_size);

	// 글리프 래스터와 경계. (펜 원점 기준, 위가 양수)
	let mut glyph_raster_list: Vec<(textshaping::GlyphRaster, i32, i32)> = Vec::with_capacity(shaped_text.glyph_list.len());
	let mut minimum_x: f32 = 0.0;
	let mut maximum_x: f32 = advance_width;
	let mut top_extent: f32 = font_ascent;
	let mut bottom_extent: f32 = font_descent;
	for shaped_glyph in &shaped_text.glyph_list {
		let glyph_raster = match rasterize_shaped_glyph(face_list, font_description, text, shaped_glyph, pixel_size) {
			Some(glyph_raster) => {
				glyph_raster
			}
			None => {
				continue;
			}
		};
		let left_x = shaped_glyph.position_x.round() as i32 + glyph_raster.left_x;
		let top_y = shaped_glyph.position_y.round() as i32 + glyph_raster.top_y;
		minimum_x = minimum_x.min(left_x as f32);
		maximum_x = maximum_x.max((left_x + glyph_raster.width) as f32);
		top_extent = top_extent.max(top_y as f32);
		bottom_extent = bottom_extent.max((glyph_raster.height - top_y) as f32);
		glyph_raster_list.push((glyph_raster, left_x, top_y));
	}

	let origin_x = padding as f32 - minimum_x.floor();
	let baseline_y = padding + top_extent.ceil() as i32;
	let width = (origin_x + maximum_x).ceil() as i32 + padding;
	let height = baseline_y + bottom_extent.ceil() as i32 + padding;
	if width <= 0 || height <= 0 {
		return None;
	}

	// 겹치는 곳은 큰 값. 색 글리프는 따로 색 층에 얹는다.
	let mut coverage = vec![0u8; (width * height) as usize];
	let mut color_pixels: Option<Vec<u8>> = None;
	let origin_x_integer = origin_x as i32;
	for (glyph_raster, left_x, top_y) in &glyph_raster_list {
		let glyph_left_x = origin_x_integer + left_x;
		let glyph_top_y = baseline_y - top_y;
		if let Some(glyph_color_pixels) = glyph_raster.color_pixels.as_ref() {
			let layer_pixels = color_pixels.get_or_insert_with(|| vec![0u8; (width * height * 4) as usize]);
			draw_color_pixels(layer_pixels, width, height, glyph_color_pixels, glyph_raster.width, glyph_raster.height, glyph_left_x, glyph_top_y);
		}
		for glyph_y in 0..glyph_raster.height {
			let bitmap_y = glyph_top_y + glyph_y;
			if bitmap_y < 0 || bitmap_y >= height {
				continue;
			}
			for glyph_x in 0..glyph_raster.width {
				let bitmap_x = glyph_left_x + glyph_x;
				if bitmap_x < 0 || bitmap_x >= width {
					continue;
				}
				let glyph_value = glyph_raster.coverage[(glyph_y * glyph_raster.width + glyph_x) as usize];
				let bitmap_index = (bitmap_y * width + bitmap_x) as usize;
				coverage[bitmap_index] = coverage[bitmap_index].max(glyph_value);
			}
		}
	}
	Some(LaidOutText {
		coverage,
		color_pixels,
		width,
		height,
		origin_x,
		baseline_y,
		advance_width,
		font_ascent,
		font_descent,
	})
}

//==============================================================================
// 브라우저 시스템 글꼴로 글 모양 래스터 받기. (웹, 글꼴 바이트가 없을 때 rasterize_text 자리)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn rasterize_system_font_text(font_description: &FontDescription, text: &str, bake_scale: f32, padding: i32, direction: ShapingDirection) -> Option<TextRaster> {
	let pixel_size = font_description.font_size * bake_scale;
	let glyph_raster = crate::platform::web::systemfont::rasterize_piece(font_description, pixel_size, text, direction == ShapingDirection::RightToLeft)?;
	let mut scaled_description = font_description.clone();
	scaled_description.font_size = pixel_size;
	let text_metrics = crate::platform::web::systemfont::measure_text(&scaled_description, text);
	let width = glyph_raster.width + padding * 2;
	let height = glyph_raster.height + padding * 2;
	let mut coverage = vec![0u8; (width * height) as usize];
	for position_y in 0..glyph_raster.height {
		for position_x in 0..glyph_raster.width {
			coverage[((position_y + padding) * width + position_x + padding) as usize] = glyph_raster.coverage[(position_y * glyph_raster.width + position_x) as usize];
		}
	}
	let color_pixels = glyph_raster.color_pixels.as_ref().map(|glyph_color_pixels| {
		let mut layer_pixels = vec![0u8; (width * height * 4) as usize];
		draw_color_pixels(&mut layer_pixels, width, height, glyph_color_pixels, glyph_raster.width, glyph_raster.height, padding, padding);
		layer_pixels
	});
	Some(TextRaster {
		coverage,
		color_pixels,
		width,
		height,
		origin_x: (padding - glyph_raster.left_x) as f32,
		baseline_y: (padding + glyph_raster.top_y) as f32,
		advance_width: text_metrics.width,
		font_ascent: text_metrics.font_ascent,
		font_descent: text_metrics.font_descent,
	})
}

//==============================================================================
// 모양 지은 글리프 하나 래스터. (시스템 글꼴 조각은 브라우저 캔버스로, 비었으면 None)
//==============================================================================
fn rasterize_shaped_glyph(face_list: &[ResolvedFontFace], font_description: &FontDescription, text: &str, shaped_glyph: &textshaping::ShapedGlyph, pixel_size: f32) -> Option<textshaping::GlyphRaster> {
	if shaped_glyph.face_index == textshaping::SYSTEM_FACE_INDEX {
		#[cfg(target_arch = "wasm32")]
		{
			return crate::platform::web::systemfont::rasterize_piece(font_description, pixel_size, &text[shaped_glyph.text_start..shaped_glyph.text_end], shaped_glyph.is_right_to_left);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			let _ = (font_description, text);
			return None;
		}
	}
	let glyph_raster = textshaping::rasterize_glyph(&face_list[shaped_glyph.face_index], shaped_glyph.glyph_index, pixel_size);
	if glyph_raster.width == 0 || glyph_raster.height == 0 {
		return None;
	}
	Some(glyph_raster)
}

//==============================================================================
// 시스템 글꼴 조각 재는 함수. (웹이면 브라우저 캔버스로 재고, 데스크탑은 없다)
//==============================================================================
#[allow(clippy::type_complexity)]
fn create_system_font_measure(font_description: &FontDescription, pixel_size: f32) -> Option<Box<dyn Fn(&str, bool) -> f32>> {
	#[cfg(target_arch = "wasm32")]
	{
		let font_description = font_description.clone();
		return Some(Box::new(move |piece_text: &str, is_right_to_left: bool| crate::platform::web::systemfont::measure_piece(&font_description, pixel_size, piece_text, is_right_to_left)));
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		let _ = (font_description, pixel_size);
		None
	}
}

//==============================================================================
// 브라우저 시스템 글꼴로 굽기. (웹. 글꼴 바이트가 하나도 없을 때)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn bake_system_font_entry(gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_description: &FontDescription, line_width: f32, bake_scale: f32, is_antialias: bool, direction: ShapingDirection) -> Option<TextEntry> {
	let mut system_font_bitmap = crate::platform::web::systemfont::bake_text(text_draw_kind, text, font_description, line_width, bake_scale, direction)?;
	if !is_antialias {
		// 프리멀티플라이드라 알파로 가르고 색은 곧은 색으로 채운다. (흰 글자는 네 칸 모두 덮인 정도, 색 이모지는 제 색)
		apply_premultiplied_alpha_threshold(&mut system_font_bitmap.pixels);
	}
	let texture = upload_text_texture(gl, system_font_bitmap.bitmap_width, system_font_bitmap.bitmap_height, &system_font_bitmap.pixels, is_antialias)?;
	Some(TextEntry {
		texture,
		advance_width: system_font_bitmap.advance_width / bake_scale,
		font_ascent: system_font_bitmap.font_ascent / bake_scale,
		font_descent: system_font_bitmap.font_descent / bake_scale,
		pen_offset_x: system_font_bitmap.pen_x / bake_scale,
		baseline_offset_y: system_font_bitmap.baseline_y / bake_scale,
		quad_width: system_font_bitmap.bitmap_width as f32 / bake_scale,
		quad_height: system_font_bitmap.bitmap_height as f32 / bake_scale,
	})
}

//==============================================================================
// 문턱값으로 0 과 255 로 가르기. (안티앨리어싱을 끈 글자)
//==============================================================================
fn apply_alpha_threshold(values: &mut [u8]) {
	for value in values.iter_mut() {
		*value = if *value >= ALPHA_THRESHOLD {
			255
		}
		else {
			0
		};
	}
}

//==============================================================================
// 프리멀티플라이드 RGBA 를 알파 문턱값으로 가르기. (원본처럼 곧은 알파의 알파 칸만 0 과 255 로, 남는 픽셀은 곧은 색 그대로 불투명)
//==============================================================================
fn apply_premultiplied_alpha_threshold(pixels: &mut [u8]) {
	for pixel in pixels.chunks_exact_mut(4) {
		let alpha = pixel[3] as u32;
		if alpha >= ALPHA_THRESHOLD as u32 {
			for channel_index in 0..3 {
				pixel[channel_index] = ((pixel[channel_index] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
			}
			pixel[3] = 255;
		}
		else {
			pixel.fill(0);
		}
	}
}

//==============================================================================
// 색 그림을 색 층에 얹기. (프리멀티플라이드 RGBA, 원본 위에 얹기. 왼쪽 위 자리 offset 부터, 층 밖은 버린다)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn draw_color_pixels(layer_pixels: &mut [u8], layer_width: i32, layer_height: i32, source_pixels: &[u8], source_width: i32, source_height: i32, offset_x: i32, offset_y: i32) {
	for source_y in 0..source_height {
		let layer_y = offset_y + source_y;
		if layer_y < 0 || layer_y >= layer_height {
			continue;
		}
		for source_x in 0..source_width {
			let layer_x = offset_x + source_x;
			if layer_x < 0 || layer_x >= layer_width {
				continue;
			}
			let source_index = ((source_y * source_width + source_x) * 4) as usize;
			let source_alpha = source_pixels[source_index + 3] as u32;
			if source_alpha == 0 {
				continue;
			}
			let layer_index = ((layer_y * layer_width + layer_x) * 4) as usize;
			for channel_index in 0..4 {
				let layer_value = layer_pixels[layer_index + channel_index] as u32;
				layer_pixels[layer_index + channel_index] = (source_pixels[source_index + channel_index] as u32 + (layer_value * (255 - source_alpha) + 127) / 255).min(255) as u8;
			}
		}
	}
}

//==============================================================================
// 흰 글자와 색 층을 한 장으로. (흰색 프리멀티플라이드 RGBA 위에 색 층을 얹는다, 색 층이 없으면 흰 글자만)
//==============================================================================
pub(crate) fn compose_text_pixels(coverage: &[u8], color_pixels: Option<&[u8]>) -> Vec<u8> {
	let mut pixels = Vec::with_capacity(coverage.len() * 4);
	match color_pixels {
		Some(color_pixels) => {
			for (pixel_index, value) in coverage.iter().enumerate() {
				let color_alpha = color_pixels[pixel_index * 4 + 3] as u32;
				let white_value = (*value as u32 * (255 - color_alpha) + 127) / 255;
				for channel_index in 0..4 {
					pixels.push((color_pixels[pixel_index * 4 + channel_index] as u32 + white_value).min(255) as u8);
				}
			}
		}
		None => {
			for value in coverage {
				pixels.push(*value);
				pixels.push(*value);
				pixels.push(*value);
				pixels.push(*value);
			}
		}
	}
	pixels
}

//==============================================================================
// 글꼴 파일에 색 글리프 표(sbix, CBDT, COLR)가 있는지 엿보기. (파일 앞머리의 표 목록만 읽는다, 못 읽으면 false)
// - 대체 글꼴 후보를 고를 때 쓴다. 큰 색 이모지 글꼴(맥 Apple Color Emoji 는 190MB 남짓)을 이모지가 아닌 글자 때문에 읽지 않게 한다.
//==============================================================================
fn peek_color_font_tables(font_path: &std::path::Path) -> bool {
	use std::io::Read;
	let mut header_bytes: Vec<u8> = Vec::new();
	let is_read = std::fs::File::open(font_path).and_then(|file| file.take(65536).read_to_end(&mut header_bytes)).is_ok();
	if !is_read {
		return false;
	}
	match rustybuzz::ttf_parser::RawFace::parse(&header_bytes, 0) {
		Ok(raw_face) => {
			raw_face.table_records.into_iter().any(|table_record| matches!(&table_record.tag.to_bytes(), b"sbix" | b"CBDT" | b"COLR"))
		}
		Err(_) => {
			false
		}
	}
}

//==============================================================================
// 구운 픽셀을 텍스처로 올리기. (흰색 프리멀티플라이드 RGBA, 안티앨리어싱을 끄면 NEAREST)
//==============================================================================
fn upload_text_texture(gl: &glow::Context, bitmap_width: i32, bitmap_height: i32, pixels: &[u8], is_antialias: bool) -> Option<crate::core::graphic::GlTexture> {
	let texture_filter = if is_antialias {
		glow::LINEAR
	}
	else {
		glow::NEAREST
	};
	unsafe {
		let texture = gl.create_texture().ok()?;
		gl.bind_texture(glow::TEXTURE_2D, Some(texture));
		gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
		gl.tex_image_2d(
			glow::TEXTURE_2D,
			0,
			glow::RGBA8 as i32,
			bitmap_width,
			bitmap_height,
			0,
			glow::RGBA,
			glow::UNSIGNED_BYTE,
			glow::PixelUnpackData::Slice(Some(pixels)),
		);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, texture_filter as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, texture_filter as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
		Some(texture)
	}
}

//==============================================================================
// 구운 문자열 키의 해시. (방식, 안티앨리어싱, 문단 방향, 폰트 문자열, 선 두께, 배율, 글)
//==============================================================================
fn create_entry_hash(text_draw_kind: TextDrawKind, is_antialias: bool, direction: ShapingDirection, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> u64 {
	let mut hasher = DefaultHasher::new();
	text_draw_kind.hash(&mut hasher);
	is_antialias.hash(&mut hasher);
	direction.hash(&mut hasher);
	font_string.hash(&mut hasher);
	line_width.to_bits().hash(&mut hasher);
	bake_scale.to_bits().hash(&mut hasher);
	text.hash(&mut hasher);
	hasher.finish()
}

//==============================================================================
// 폰트 문자열 해석. ("bold 16px DOSGothic" → 크기 16, 굵기 700, 이름 DOSGothic)
// - 크기 앞의 낱말: italic / oblique 는 기울임, bold / bolder 는 700, lighter 는 100, 숫자는 굵기(1 ~ 1000), normal 과 그 밖은 넘긴다.
// - 크기 뒤는 모두 이름이다. 따옴표는 뗀다. 쉼표로 이은 목록이면 첫 이름을 쓴다.
//==============================================================================
pub fn parse_font_string(font_string: &str) -> FontDescription {
	let mut font_size: f32 = 10.0;
	let mut font_weight: u16 = 400;
	let mut is_italic = false;
	let mut family_name_parts: Vec<&str> = Vec::new();
	let mut has_found_size = false;
	for token in font_string.split_whitespace() {
		if has_found_size {
			family_name_parts.push(token);
			continue;
		}
		let lowercase_token = token.to_lowercase();
		if lowercase_token == "bold" || lowercase_token == "bolder" {
			font_weight = 700;
			continue;
		}
		if lowercase_token == "lighter" {
			font_weight = 100;
			continue;
		}
		if lowercase_token == "italic" || lowercase_token == "oblique" {
			is_italic = true;
			continue;
		}
		if let Ok(parsed_weight) = lowercase_token.parse::<u32>() {
			if (1..=1000).contains(&parsed_weight) {
				font_weight = parsed_weight as u16;
			}
			continue;
		}
		if let Some(size_text) = lowercase_token.strip_suffix("px") {
			if let Ok(parsed_font_size) = size_text.parse::<f32>() {
				font_size = parsed_font_size;
				has_found_size = true;
			}
			continue;
		}
	}
	let family_list_text = family_name_parts.join(" ");
	let first_family_text = family_list_text.split(',').next().unwrap_or("");
	let family_name = first_family_text.trim().trim_matches(|character| character == '"' || character == '\'').to_string();
	FontDescription {
		font_size,
		font_weight,
		is_bold: font_weight >= textshaping::BOLD_WEIGHT_THRESHOLD,
		is_italic,
		family_name,
	}
}

//==============================================================================
// 폰트 읽기. (웹 글꼴이면 풀고, 래스터와 모양 짓기 얼굴을 만든다)
//==============================================================================
fn load_font(font_bytes: Vec<u8>) -> Result<LoadedFont, String> {
	let font_bytes = decompress_web_font(font_bytes)?;
	LoadedFont::create_from_bytes(font_bytes)
}

//==============================================================================
// 웹 글꼴이면 TTF 로 풀기. (머리 4 바이트가 "wOFF" 면 WOFF, "wOF2" 면 WOFF2. 아니면 그대로)
// - fontdue 는 ttf, ttc, otf 만 읽는다. 원본 기본 글꼴 DOSGothic 이 .woff 라서 푼다.
//==============================================================================
fn decompress_web_font(font_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
	let signature = font_bytes.get(0..4).unwrap_or(&[]);
	if signature == b"wOFF" {
		return wuff::decompress_woff1(&font_bytes).map_err(|error| format!("WOFF 풀기 실패: {:?}", error));
	}
	if signature == b"wOF2" {
		return wuff::decompress_woff2(&font_bytes).map_err(|error| format!("WOFF2 풀기 실패: {:?}", error));
	}
	Ok(font_bytes)
}

//==============================================================================
// 줄 높이 값. (위로 올라간 길이, 아래로 내려간 길이. 둘 다 양수)
//==============================================================================
fn read_line_metrics(font: &LoadedFont, pixel_size: f32) -> (f32, f32) {
	match font.raster_font.horizontal_line_metrics(pixel_size) {
		Some(line_metrics) => {
			(line_metrics.ascent, -line_metrics.descent)
		}
		None => {
			(pixel_size * 0.8, pixel_size * 0.2)
		}
	}
}

//==============================================================================
// 외곽선 띠 만들기. (원 모양으로 넓힌 값 - 원 모양으로 좁힌 값)
//==============================================================================
fn build_stroke_coverage(coverage: &[u8], bitmap_width: i32, bitmap_height: i32, stroke_radius: f32) -> Vec<u8> {
	let radius_ceiling = stroke_radius.ceil() as i32;
	let mut offset_list: Vec<(i32, i32)> = Vec::new();
	for offset_y in -radius_ceiling..=radius_ceiling {
		for offset_x in -radius_ceiling..=radius_ceiling {
			let distance = ((offset_x * offset_x + offset_y * offset_y) as f32).sqrt();
			if distance <= stroke_radius + 0.5 {
				offset_list.push((offset_x, offset_y));
			}
		}
	}

	let read_coverage = |position_x: i32, position_y: i32| -> u8 {
		if position_x < 0 || position_y < 0 || position_x >= bitmap_width || position_y >= bitmap_height {
			return 0;
		}
		coverage[(position_y * bitmap_width + position_x) as usize]
	};

	let mut stroke_coverage = vec![0u8; coverage.len()];
	for position_y in 0..bitmap_height {
		for position_x in 0..bitmap_width {
			let mut dilated_value: u8 = 0;
			let mut eroded_value: u8 = 255;
			for (offset_x, offset_y) in &offset_list {
				let value = read_coverage(position_x + offset_x, position_y + offset_y);
				dilated_value = dilated_value.max(value);
				eroded_value = eroded_value.min(value);
			}
			stroke_coverage[(position_y * bitmap_width + position_x) as usize] = dilated_value.saturating_sub(eroded_value);
		}
	}
	stroke_coverage
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// WOFF 글꼴이 풀려서 읽히는지. (VANILLA_TEST_WOFF_PATH 에 .woff 경로를 줄 때만 본다)
	//==============================================================================
	#[test]
	fn load_woff_font() {
		let font_path = match std::env::var("VANILLA_TEST_WOFF_PATH") {
			Ok(font_path) => {
				font_path
			}
			Err(_) => {
				return;
			}
		};
		let font_bytes = std::fs::read(&font_path).expect("woff file");
		let font = load_font(font_bytes).expect("woff font");
		let glyph_count = font.raster_font.glyph_count();
		let hangul_glyph_index = font.raster_font.lookup_glyph_index('가');
		assert!(glyph_count > 0);
		assert!(hangul_glyph_index != 0);
	}

	//==============================================================================
	// 글 재기 캐시: 같은 폰트 문자열과 글은 한 번만 재 두고, 글꼴을 새로 등록하면 비운다.
	// (엔진 기본 글꼴 후보 가운데 이 PC 에 있는 것으로 본다, 하나도 없으면 건너뛴다)
	//==============================================================================
	#[test]
	fn measure_cache_reuses_and_clears_on_font_registration() {
		let engine_configuration = crate::core::engine::EngineConfiguration::new();
		let font_bytes = match engine_configuration.default_font_paths.iter().find_map(|font_path| std::fs::read(font_path).ok()) {
			Some(font_bytes) => {
				font_bytes
			}
			None => {
				return;
			}
		};
		let mut text_string_texture_cache = TextStringTextureCache::new();
		text_string_texture_cache.set_default_font(false, font_bytes.clone()).expect("default font");
		let first_metrics = text_string_texture_cache.measure_text("20px default", "가나다 abc");
		assert!(first_metrics.width > 0.0);
		assert_eq!(text_string_texture_cache.get_measure_count(), 1);
		let second_metrics = text_string_texture_cache.measure_text("20px default", "가나다 abc");
		assert_eq!(first_metrics.width, second_metrics.width);
		assert_eq!(text_string_texture_cache.get_measure_count(), 1);
		let bold_metrics = text_string_texture_cache.measure_text("40px default", "가나다 abc");
		assert!(bold_metrics.width > first_metrics.width);
		assert_eq!(text_string_texture_cache.get_measure_count(), 2);
		text_string_texture_cache.add_fallback_font(font_bytes).expect("fallback font");
		text_string_texture_cache.measure_text("20px default", "가나다 abc");
		assert_eq!(text_string_texture_cache.get_measure_count(), 1);
		for text_index in 0..(MAXIMUM_MEASURE_COUNT + 10) {
			text_string_texture_cache.measure_text("20px default", &text_index.to_string());
		}
		assert!(text_string_texture_cache.get_measure_count() <= MAXIMUM_MEASURE_COUNT);
	}

	//==============================================================================
	// 구운 문자열 키: 같은 키는 같은 해시, 하나라도 다르면 맞지 않는다.
	//==============================================================================
	#[test]
	fn entry_key_matches_only_same_key() {
		let first_hash = create_entry_hash(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5);
		let second_hash = create_entry_hash(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5);
		assert_eq!(first_hash, second_hash);
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Stroke, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 2.0));
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Fill, false, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		let texture_entry = TextEntry {
			texture: glow::NativeTexture(std::num::NonZeroU32::new(1).expect("one")),
			advance_width: 0.0,
			font_ascent: 0.0,
			font_descent: 0.0,
			pen_offset_x: 0.0,
			baseline_offset_y: 0.0,
			quad_width: 0.0,
			quad_height: 0.0,
		};
		let cached_text_entry = CachedTextEntry {
			text_draw_kind: TextDrawKind::Fill,
			is_antialias: true,
			direction: ShapingDirection::LeftToRight,
			text: String::from("가나다"),
			font_string: String::from("16px default"),
			line_width: 0.0,
			bake_scale: 1.5,
			entry: texture_entry,
			last_used_frame_number: 0,
		};
		assert!(cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, false, ShapingDirection::LeftToRight, "가나다", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "bold 16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, ShapingDirection::LeftToRight, "가나다", "16px default", 2.0, 1.5));
	}

	//==============================================================================
	// 안티앨리어싱을 끄면 문턱값 128 로 0 과 255 로 갈린다. (원본과 같은 문턱)
	//==============================================================================
	#[test]
	fn alpha_threshold_splits_at_128() {
		let mut values: Vec<u8> = vec![0, 1, 127, 128, 200, 255];
		apply_alpha_threshold(&mut values);
		assert_eq!(values, vec![0, 0, 0, 255, 255, 255]);
		let mut text_string_texture_cache = TextStringTextureCache::new();
		assert!(text_string_texture_cache.is_text_antialias_on());
		text_string_texture_cache.set_text_antialias_enabled(false);
		assert!(!text_string_texture_cache.is_text_antialias_on());
	}

	//==============================================================================
	// 폰트 문자열: italic, 굵기, 따옴표 이름, 이름 목록의 첫 이름을 읽는다.
	//==============================================================================
	#[test]
	fn parse_font_string_reads_style_weight_and_family() {
		let font_description = parse_font_string("italic bold 20px \"My Font\"");
		assert!(font_description.is_italic);
		assert!(font_description.is_bold);
		assert_eq!(font_description.font_weight, 700);
		assert_eq!(font_description.family_name, "My Font");
		let weighted_description = parse_font_string("500 16px \"GameUI\", sans-serif");
		assert_eq!(weighted_description.font_weight, 500);
		assert!(!weighted_description.is_bold);
		assert_eq!(weighted_description.family_name, "GameUI");
		assert!(!parse_font_string("20px default").is_italic);
	}

	//==============================================================================
	// 웹 글꼴이 아닌 바이트는 그대로 둔다.
	//==============================================================================
	#[test]
	fn keep_plain_font_bytes() {
		let plain_bytes: Vec<u8> = vec![0, 1, 0, 0, 9, 9];
		let result_bytes = decompress_web_font(plain_bytes.clone()).expect("plain bytes");
		assert_eq!(result_bytes, plain_bytes);
	}
}
