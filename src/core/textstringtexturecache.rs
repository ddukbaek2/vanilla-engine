//==============================================================================
// 문자열 텍스처 캐시.
// - 원본은 오프스크린 2D 캔버스에 fillText / strokeText 로 문자열을 구워 텍스처로 쓴다. (LRU 상한 512)
// - 여기서는 글자 모양 짓기(textshaping.rs: 양방향 정렬, rustybuzz 모양 짓기)로 글리프를 놓고 fontdue 로 래스터화해 문자열 한 장으로 굽는다.
//   상한과 키 구성(방식, 문단 방향, 폰트 문자열, 선 두께, 배율, 글)은 원본과 같다.
//   원본은 키를 문자열로 이어 붙인다. 여기서는 찾을 때마다 문자열을 만들지 않게 키의 해시로 찾고, 같은 해시 안에서 키를 하나씩 맞춰 본다.
// - 글자는 흰색으로 굽고, 그릴 때 색을 곱한다. (색이 바뀌어도 다시 굽지 않는다)
//   색 글리프(이모지)는 제 색으로 흰 글자 위에 얹어 굽는다. 원본도 흰색으로 fillText 한 캔버스를 올리므로 이모지는 제 색으로 구워지고 그릴 때 같은 색이 곱해진다.
//   외곽선(Stroke)으로 구울 때도 색 글리프는 테를 긋지 않고 색 그대로 얹는다. (크롬 strokeText 와 같다)
// - 폰트 문자열은 Canvas2D font 와 같은 꼴(CSS font 줄임 꼴)을 읽는다. 예: "16px DOSGothic", "bold 24px default", "italic 500 16px default", "12pt 'A B', C, sans-serif"
//   크기는 px 밖의 CSS 단위와 이름 크기도 읽고 "/줄높이"는 넘긴다. 쉼표로 이은 이름 목록은 글자마다 앞 이름의 얼굴부터 찾는다. (브라우저의 글꼴 목록과 같다, parse_font_string)
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
	// 목록의 첫 이름.
	pub family_name: String,
	// 쉼표로 이은 이름 목록. (앞 이름부터 찾는다)
	pub family_name_list: Vec<String>,
}

//==============================================================================
// 글꼴 파일 하나의 얼굴. (데스크탑 시스템 글꼴, platform 이 찾아 준다)
// - face_index 를 모르면(맥) post_script_name 으로 모음 안의 얼굴을 찾고, 그것도 없으면 굵기와 기울기로 고른다.
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFileSource {
	pub path: std::path::PathBuf,
	pub face_index: Option<u32>,
	pub post_script_name: Option<String>,
}

//==============================================================================
// 데스크탑 시스템 글꼴의 얼굴 후보 하나. (청한 굵기와 기울기로 운영체제가 찾아 준 파일, 쓸 때 읽는다)
//==============================================================================
struct SystemFontCandidate {
	weight: u16,
	is_italic: bool,
	source: FontFileSource,
	is_loaded: bool,
}

//==============================================================================
// 데스크탑 시스템 글꼴 이름 하나. (얼굴 후보들과 읽은 얼굴들)
//==============================================================================
#[derive(Default)]
struct SystemFontFamily {
	candidate_list: Vec<SystemFontCandidate>,
	face_list: Vec<RegisteredFontFace>,
}

//==============================================================================
// 데스크탑 시스템 글꼴 상태. (원본 브라우저가 CSS 이름을 운영체제 글꼴로 찾고, 없는 글자를 운영체제의 대체 글꼴로 그리는 자리)
// - Graphic 없이 재기의 사본과 나눠 들어(Rc) 같은 파일을 두 번 읽지 않는다.
//==============================================================================
#[derive(Default)]
struct SystemFontState {
	// 올리지 않은 이름을 운영체제 글꼴에서 찾는지. (set_system_font_chain 이 켠다)
	is_lookup_enabled: bool,
	// 글꼴에 없는 글자를 운영체제가 고른 대체 글꼴로 그리는지.
	is_fallback_enabled: bool,
	// 기본 글꼴 사슬. (원본 SYSTEM_FONT_STRING 의 이름 차례, 기본 폰트 뒤에 찾는다)
	chain_name_list: Vec<String>,
	// 이름(소문자) → 찾은 글꼴. (None 은 운영체제에 없는 이름)
	family_table: HashMap<String, Option<SystemFontFamily>>,
	// 이미 물어본 대체 글꼴. (글자, 굵게, 기울임, 색)
	fallback_query_set: std::collections::HashSet<(char, bool, bool, bool)>,
	// 운영체제가 고른 대체 글꼴 얼굴. (source_key 가 같은 것끼리 한 이름으로 고른다)
	fallback_face_list: Vec<RegisteredFontFace>,
	// 읽은 파일 바이트. (같은 파일의 여러 얼굴이 나눠 든다)
	shared_bytes_table: HashMap<std::path::PathBuf, std::rc::Weak<[u8]>>,
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
	// 데스크탑 시스템 글꼴. (기본 글꼴 사슬, 이름 찾기, 대체 글꼴)
	system_font_state: Rc<RefCell<SystemFontState>>,
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
			system_font_state: Rc::new(RefCell::new(SystemFontState::default())),
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
				let is_affected = font_description.family_name_list.iter().any(|family_name| family_name.to_lowercase() == family_key) && font_face_descriptors.contains_any_character(&cached_text_entry.text);
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
		shared_measure_cache.system_font_state = self.system_font_state.clone();
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
	// 시스템 글꼴 사슬 정하기. (데스크탑, 원본 SYSTEM_FONT_STRING 처럼 기본 폰트 뒤에 이름 차례로 찾는다)
	// - 켜면 폰트 문자열의 올리지 않은 이름도 운영체제 글꼴에서 찾는다. ("16px Arial" 이 시스템의 Arial 로 그려진다, 브라우저와 같다)
	// - 얼굴은 그 굵기와 기울기의 글자를 처음 그리거나 잴 때 읽는다.
	//==============================================================================
	pub fn set_system_font_chain(&mut self, family_name_list: Vec<String>) {
		{
			let mut system_font_state = self.system_font_state.borrow_mut();
			system_font_state.is_lookup_enabled = true;
			system_font_state.chain_name_list = family_name_list.iter().map(|family_name| family_name.trim().to_lowercase()).collect();
		}
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	// 시스템 글꼴 이름에 얼굴 후보 파일 더하기. (데스크탑, 그 이름을 사슬 맨 뒤에 둔다. 운영체제에서 찾지 않고 파일을 주는 경우)
	// - 얼굴 번호가 없으면 PostScript 이름, 그것도 없으면 청한 굵기와 기울기로 모음 안의 얼굴을 고른다. 가변 글꼴은 굵기를 축 값으로 만든다.
	//==============================================================================
	pub fn add_system_font_candidate(&mut self, family_name: &str, weight: u16, is_italic: bool, source: FontFileSource) {
		{
			let mut system_font_state = self.system_font_state.borrow_mut();
			let family_key = family_name.trim().to_lowercase();
			if !system_font_state.chain_name_list.contains(&family_key) {
				system_font_state.chain_name_list.push(family_key.clone());
			}
			let system_font_family = system_font_state.family_table.entry(family_key).or_insert_with(|| Some(SystemFontFamily::default())).get_or_insert_with(SystemFontFamily::default);
			if !system_font_family.candidate_list.iter().any(|candidate| candidate.source == source && candidate.weight == weight && candidate.is_italic == is_italic) {
				system_font_family.candidate_list.push(SystemFontCandidate {
					weight,
					is_italic,
					source,
					is_loaded: false,
				});
			}
		}
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	// 운영체제 대체 글꼴 켬 / 끔. (데스크탑, 켜면 어느 글꼴에도 없는 글자를 운영체제가 고른 글꼴로 그린다. 대체 글꼴 후보 파일이 있으면 그것이 먼저다)
	//==============================================================================
	pub fn set_system_font_fallback_enabled(&mut self, value: bool) {
		self.system_font_state.borrow_mut().is_fallback_enabled = value;
		self.clear();
		self.font_registration_count.set(self.font_registration_count.get() + 1);
		self.publish_shared_font_set();
	}

	//==============================================================================
	// 시스템 글꼴 사슬이나 후보가 있는지. (데스크탑)
	//==============================================================================
	pub fn has_system_font(&self) -> bool {
		let system_font_state = self.system_font_state.borrow();
		system_font_state.chain_name_list.iter().any(|family_key| matches!(system_font_state.family_table.get(family_key), Some(Some(_)))) || (system_font_state.is_lookup_enabled && !system_font_state.chain_name_list.is_empty())
	}

	//==============================================================================
	// 시스템 글꼴 이름의 얼굴들을 고른 목록에 더하기. (처음 보는 이름이면 운영체제에서 찾고, 청한 굵기와 기울기에 맞는 후보를 읽는다, 없으면 false)
	//==============================================================================
	fn append_system_family_faces(&self, family_key: &str, desired_weight: u16, is_italic_desired: bool, output_list: &mut Vec<ResolvedFontFace>) -> bool {
		let mut system_font_state = self.system_font_state.borrow_mut();
		if !system_font_state.family_table.contains_key(family_key) {
			if !system_font_state.is_lookup_enabled {
				return false;
			}
			let system_font_family = find_system_font_family(family_key);
			system_font_state.family_table.insert(family_key.to_string(), system_font_family);
		}
		let state = &mut *system_font_state;
		let system_font_family = match state.family_table.get_mut(family_key) {
			Some(Some(system_font_family)) => {
				system_font_family
			}
			_ => {
				return false;
			}
		};
		// 청한 기울기를 먼저, 그다음 굵기로 가장 맞는 후보를 아직 읽지 않았으면 읽는다.
		let best_candidate_index = (0..system_font_family.candidate_list.len()).min_by_key(|candidate_index| {
			let candidate = &system_font_family.candidate_list[*candidate_index];
			(candidate.is_italic != is_italic_desired, textshaping::calculate_weight_score(candidate.weight, candidate.weight, desired_weight))
		});
		if let Some(candidate_index) = best_candidate_index {
			if !system_font_family.candidate_list[candidate_index].is_loaded {
				system_font_family.candidate_list[candidate_index].is_loaded = true;
				let candidate = &system_font_family.candidate_list[candidate_index];
				match load_system_font_face(&mut state.shared_bytes_table, &candidate.source, candidate.weight, candidate.is_italic) {
					Ok(registered_font_face) => {
						let is_duplicate = system_font_family.face_list.iter().any(|face| Rc::ptr_eq(face.font.get_font_bytes(), registered_font_face.font.get_font_bytes()) && face.font.get_face_index() == registered_font_face.font.get_face_index() && face.descriptors == registered_font_face.descriptors);
						if !is_duplicate {
							system_font_family.face_list.push(registered_font_face);
						}
					}
					Err(error) => {
						eprintln!("TextStringTextureCache: {} ({})", error, candidate.source.path.display());
					}
				}
			}
		}
		if system_font_family.face_list.is_empty() {
			return false;
		}
		let face_reference_list: Vec<&RegisteredFontFace> = system_font_family.face_list.iter().collect();
		textshaping::select_font_faces(&face_reference_list, desired_weight, is_italic_desired, output_list);
		true
	}

	//==============================================================================
	// 글의 글자가 운영체제 대체 글꼴로 그려지게 하기. (어느 얼굴에도 없는 글자를 운영체제에 묻는다, 같은 물음은 한 번)
	// - 굵게를 청했는데 그 글자를 가진 대체 얼굴이 모두 600 아래면 굵은 대체 얼굴을 따로 묻는다. (크롬도 굵기를 주고 대체 글꼴을 고른다)
	//==============================================================================
	fn ensure_system_fallback_coverage(&self, text: &str, font_description: &FontDescription, resolved_face_list: &[ResolvedFontFace]) {
		if !self.system_font_state.borrow().is_fallback_enabled {
			return;
		}
		let is_bold_desired = font_description.font_weight >= textshaping::BOLD_WEIGHT_THRESHOLD;
		for grapheme in text.graphemes(true) {
			let is_emoji_grapheme = textshaping::is_emoji_presentation(grapheme);
			let character = match grapheme.chars().next() {
				Some(character) => {
					character
				}
				None => {
					continue;
				}
			};
			if character.is_whitespace() || character.is_control() || ('\u{200B}'..='\u{200F}').contains(&character) || ('\u{2028}'..='\u{206F}').contains(&character) || ('\u{FE00}'..='\u{FE0F}').contains(&character) {
				continue;
			}
			let is_covered_by_face = |face: &RegisteredFontFace| -> bool {
				if is_emoji_grapheme {
					face.font.has_color_character(character)
				}
				else {
					face.font.has_character(character) && !face.font.has_color_character(character)
				}
			};
			let is_resolved_covered = resolved_face_list.iter().any(|face| {
				let has_character = if is_emoji_grapheme {
					face.font.has_color_character(character)
				}
				else {
					face.font.has_character(character) && !face.font.has_color_character(character)
				};
				face.descriptors.contains_character(character) && has_character
			});
			if is_resolved_covered {
				continue;
			}
			let query_key = (character, is_bold_desired, font_description.is_italic, is_emoji_grapheme);
			{
				let system_font_state = self.system_font_state.borrow();
				if system_font_state.fallback_query_set.contains(&query_key) {
					continue;
				}
				let covering_face_list: Vec<&RegisteredFontFace> = system_font_state.fallback_face_list.iter().filter(|face| is_covered_by_face(face)).collect();
				if !covering_face_list.is_empty() && (!is_bold_desired || covering_face_list.iter().any(|face| face.descriptors.maximum_weight >= textshaping::BOLD_WEIGHT_THRESHOLD)) {
					continue;
				}
			}
			self.system_font_state.borrow_mut().fallback_query_set.insert(query_key);
			let source = match find_system_fallback_font(grapheme, font_description.font_weight, font_description.is_italic, is_emoji_grapheme) {
				Some(source) => {
					source
				}
				None => {
					continue;
				}
			};
			let mut system_font_state = self.system_font_state.borrow_mut();
			let state = &mut *system_font_state;
			let is_loaded_already = state.fallback_face_list.iter().any(|face| face.descriptors.source_key == read_fallback_source_key(&source) && face.descriptors.minimum_weight == font_description.font_weight.clamp(face.descriptors.minimum_weight, face.descriptors.maximum_weight) && face.descriptors.is_italic == font_description.is_italic);
			if is_loaded_already {
				continue;
			}
			match load_system_font_face(&mut state.shared_bytes_table, &source, font_description.font_weight, font_description.is_italic) {
				Ok(mut registered_font_face) => {
					registered_font_face.descriptors.source_key = read_fallback_source_key(&source);
					let is_duplicate = state.fallback_face_list.iter().any(|face| Rc::ptr_eq(face.font.get_font_bytes(), registered_font_face.font.get_font_bytes()) && face.font.get_face_index() == registered_font_face.font.get_face_index() && face.descriptors == registered_font_face.descriptors);
					if !is_duplicate {
						state.fallback_face_list.push(registered_font_face);
					}
				}
				Err(error) => {
					eprintln!("TextStringTextureCache: {} ({})", error, source.path.display());
				}
			}
		}
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
		is_in_family || self.default_face_list.iter().any(|face| face.font.has_character(character)) || self.fallback_face_list.borrow().iter().any(|face| face.font.has_character(character)) || self.is_system_character_covered(&|face| face.font.has_character(character))
	}

	//==============================================================================
	// 읽은 시스템 글꼴 얼굴(사슬, 운영체제 대체 글꼴) 가운데 조건에 맞는 것이 있는지.
	//==============================================================================
	fn is_system_character_covered(&self, is_covered: &dyn Fn(&RegisteredFontFace) -> bool) -> bool {
		let system_font_state = self.system_font_state.borrow();
		let is_in_chain = system_font_state.chain_name_list.iter().any(|family_key| match system_font_state.family_table.get(family_key) {
			Some(Some(system_font_family)) => {
				system_font_family.face_list.iter().any(is_covered)
			}
			_ => {
				false
			}
		});
		is_in_chain || system_font_state.fallback_face_list.iter().any(is_covered)
	}

	//==============================================================================
	// 등록된 어느 글꼴이 그 글자를 색이 아닌 글리프로 가졌는지.
	//==============================================================================
	fn is_text_character_covered(&self, character: char) -> bool {
		let has_text_character = |face: &RegisteredFontFace| face.font.has_character(character) && !face.font.has_color_character(character);
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && has_text_character(face)));
		is_in_family || self.default_face_list.iter().any(has_text_character) || self.fallback_face_list.borrow().iter().any(has_text_character) || self.is_system_character_covered(&has_text_character)
	}

	//==============================================================================
	// 등록된 어느 글꼴이 그 글자를 색 글리프로 가졌는지.
	//==============================================================================
	fn is_color_character_covered(&self, character: char) -> bool {
		let is_in_family = self.font_family_table.values().any(|face_list| face_list.iter().any(|face| face.descriptors.contains_character(character) && face.font.has_color_character(character)));
		is_in_family || self.default_face_list.iter().any(|face| face.font.has_color_character(character)) || self.fallback_face_list.borrow().iter().any(|face| face.font.has_color_character(character)) || self.is_system_character_covered(&|face| face.font.has_color_character(character))
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
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(text, &font_description);
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
			let (glyph_left_x, glyph_bottom_y, glyph_width, glyph_height) = face.font.read_glyph_metrics(shaped_glyph.glyph_index, font_size);
			if glyph_width == 0 || glyph_height == 0 {
				continue;
			}
			let mut left_x = shaped_glyph.position_x + glyph_left_x as f32;
			let mut right_x = left_x + glyph_width as f32;
			let mut top_y = shaped_glyph.position_y + (glyph_bottom_y + glyph_height) as f32;
			let mut bottom_y = shaped_glyph.position_y + glyph_bottom_y as f32;
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
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(text, &font_description);
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
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(character, &font_description);
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
	// 폰트 문자열에 맞는 얼굴 목록. (이름 목록의 차례로 이름마다 그 얼굴들, 기본 폰트, 대체 폰트 차례. 글자마다 앞에서부터 찾는다, 올리지 않은 이름은 넘긴다)
	//==============================================================================
	fn resolve_face_list(&self, font_description: &FontDescription) -> Vec<ResolvedFontFace> {
		let mut face_list: Vec<ResolvedFontFace> = Vec::new();
		let mut used_system_family_list: Vec<String> = Vec::new();
		for family_name in &font_description.family_name_list {
			let family_key = family_name.to_lowercase();
			if let Some(family_face_list) = self.font_family_table.get(&family_key) {
				let family_face_reference_list: Vec<&RegisteredFontFace> = family_face_list.iter().collect();
				textshaping::select_font_faces(&family_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
				continue;
			}
			// 올리지 않은 이름은 운영체제 글꼴에서 찾는다. (데스크탑, 사슬을 켰을 때)
			if !used_system_family_list.contains(&family_key) && self.append_system_family_faces(&family_key, font_description.font_weight, font_description.is_italic, &mut face_list) {
				used_system_family_list.push(family_key);
			}
		}
		let default_face_reference_list: Vec<&RegisteredFontFace> = self.default_face_list.iter().collect();
		textshaping::select_font_faces(&default_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		// 기본 글꼴 사슬. (원본 SYSTEM_FONT_STRING 의 차례)
		let chain_name_list = self.system_font_state.borrow().chain_name_list.clone();
		for family_key in chain_name_list {
			if used_system_family_list.contains(&family_key) {
				continue;
			}
			if self.append_system_family_faces(&family_key, font_description.font_weight, font_description.is_italic, &mut face_list) {
				used_system_family_list.push(family_key);
			}
		}
		for fallback_face in self.fallback_face_list.borrow().iter() {
			let fallback_face_reference_list = [fallback_face];
			textshaping::select_font_faces(&fallback_face_reference_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		}
		// 운영체제가 고른 대체 글꼴. (같은 글꼴의 굵기들은 한 이름으로 고른다)
		let system_font_state = self.system_font_state.borrow();
		let mut grouped_key_list: Vec<&str> = Vec::new();
		for fallback_face in system_font_state.fallback_face_list.iter() {
			let source_key = fallback_face.descriptors.source_key.as_str();
			if grouped_key_list.contains(&source_key) {
				continue;
			}
			grouped_key_list.push(source_key);
			let group_face_list: Vec<&RegisteredFontFace> = system_font_state.fallback_face_list.iter().filter(|face| face.descriptors.source_key == source_key).collect();
			textshaping::select_font_faces(&group_face_list, font_description.font_weight, font_description.is_italic, &mut face_list);
		}
		face_list
	}

	//==============================================================================
	// 글자를 그릴 얼굴 목록. (대체 글꼴 후보와 운영체제 대체 글꼴을 먼저 채우고 고른다)
	//==============================================================================
	fn resolve_covering_face_list(&self, text: &str, font_description: &FontDescription) -> Vec<ResolvedFontFace> {
		self.ensure_fallback_coverage(text);
		let face_list = self.resolve_face_list(font_description);
		if !self.system_font_state.borrow().is_fallback_enabled {
			return face_list;
		}
		self.ensure_system_fallback_coverage(text, font_description, &face_list);
		self.resolve_face_list(font_description)
	}

	//==============================================================================
	// 문자열 굽기.
	//==============================================================================
	fn bake_entry(&self, gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<TextEntry> {
		let font_description = parse_font_string(font_string);
		let face_list = self.resolve_covering_face_list(text, &font_description);
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
// 운영체제 글꼴에서 이름 하나 찾기. (보통, 굵게, 기울임, 굵은 기울임 네 가지를 묻고 같은 얼굴은 하나로, 하나도 없으면 None. 데스크탑만)
//==============================================================================
fn find_system_font_family(family_key: &str) -> Option<SystemFontFamily> {
	#[cfg(not(target_arch = "wasm32"))]
	{
		let mut system_font_family = SystemFontFamily::default();
		for (weight, is_italic) in [(400, false), (700, false), (400, true), (700, true)] {
			let source = match crate::platform::find_family_font_file(family_key, weight, is_italic) {
				Some(source) => {
					source
				}
				None => {
					continue;
				}
			};
			// 같은 얼굴이면 앞(보통 굵기, 바른 꼴)의 것만 둔다. (기울인 얼굴이 없는 글꼴은 바른 얼굴에 가짜 기울임)
			if system_font_family.candidate_list.iter().any(|candidate| candidate.source == source) {
				continue;
			}
			system_font_family.candidate_list.push(SystemFontCandidate {
				weight,
				is_italic,
				source,
				is_loaded: false,
			});
		}
		if system_font_family.candidate_list.is_empty() {
			return None;
		}
		Some(system_font_family)
	}
	#[cfg(target_arch = "wasm32")]
	{
		let _ = family_key;
		None
	}
}

//==============================================================================
// 운영체제가 고른 대체 글꼴 파일. (데스크탑만, 웹은 None)
//==============================================================================
fn find_system_fallback_font(grapheme: &str, weight: u16, is_italic: bool, is_color_wanted: bool) -> Option<FontFileSource> {
	#[cfg(not(target_arch = "wasm32"))]
	{
		crate::platform::find_fallback_font_file(grapheme, weight, is_italic, is_color_wanted)
	}
	#[cfg(target_arch = "wasm32")]
	{
		let _ = (grapheme, weight, is_italic, is_color_wanted);
		None
	}
}

//==============================================================================
// 운영체제 대체 글꼴을 한 이름으로 묶는 열쇠. (파일 경로, 같은 파일의 굵기 얼굴들이 함께 고른다)
//==============================================================================
fn read_fallback_source_key(source: &FontFileSource) -> String {
	source.path.to_string_lossy().into_owned()
}

//==============================================================================
// 시스템 글꼴 파일의 얼굴 하나 읽기. (같은 파일의 바이트는 나눠 든다)
// - 얼굴 번호: 준 번호, 없으면 PostScript 이름이 같은 얼굴, 그것도 없으면 청한 굵기와 기울기로 고른 얼굴.
// - 서술자: 가변 글꼴(wght 축)이면 청한 굵기를 축 범위로 자른 값의 인스턴스, 아니면 그 얼굴의 OS/2 굵기와 기울임 표시.
//==============================================================================
fn load_system_font_face(shared_bytes_table: &mut HashMap<std::path::PathBuf, std::rc::Weak<[u8]>>, source: &FontFileSource, weight: u16, is_italic: bool) -> Result<RegisteredFontFace, String> {
	let font_bytes: Rc<[u8]> = match shared_bytes_table.get(&source.path).and_then(|weak_bytes| weak_bytes.upgrade()) {
		Some(font_bytes) => {
			font_bytes
		}
		None => {
			let font_bytes: Rc<[u8]> = Rc::from(std::fs::read(&source.path).map_err(|error| format!("글꼴 파일 읽기 실패: {}", error))?);
			shared_bytes_table.insert(source.path.clone(), Rc::downgrade(&font_bytes));
			font_bytes
		}
	};
	let face_index = match source.face_index {
		Some(face_index) => {
			face_index
		}
		None => {
			let face_count = rustybuzz::ttf_parser::fonts_in_collection(&font_bytes).unwrap_or(1);
			let post_script_index = source.post_script_name.as_ref().and_then(|post_script_name| {
				(0..face_count).find(|face_index| {
					rustybuzz::ttf_parser::Face::parse(&font_bytes, *face_index).map(|face| face.names().into_iter().any(|name| name.name_id == rustybuzz::ttf_parser::name_id::POST_SCRIPT_NAME && name.to_string().as_deref() == Some(post_script_name.as_str()))).unwrap_or(false)
				})
			});
			post_script_index.unwrap_or_else(|| textshaping::find_collection_face_index(&font_bytes, weight, is_italic))
		}
	};
	let mut font_face_descriptors = FontFaceDescriptors::new();
	let variation_list = match textshaping::read_weight_axis_range(&font_bytes, face_index) {
		Some((minimum_weight, maximum_weight)) => {
			font_face_descriptors.set_weight(weight.clamp(minimum_weight, maximum_weight));
			textshaping::read_weight_variation(&font_bytes, face_index, weight)
		}
		None => {
			Vec::new()
		}
	};
	let font = LoadedFont::create_from_shared_bytes(font_bytes, face_index, &variation_list)?;
	if variation_list.is_empty() && textshaping::read_weight_axis_range(font.get_font_bytes(), face_index).is_none() {
		let shaping_face = font.get_shaping_face();
		font_face_descriptors.set_weight(shaping_face.weight().to_number());
	}
	font_face_descriptors.is_italic = font.get_shaping_face().is_italic();
	Ok(RegisteredFontFace {
		descriptors: font_face_descriptors,
		font: Rc::new(font),
		order_number: 0,
	})
}

//==============================================================================
// 폰트 문자열 해석. (Canvas2D font 의 CSS font 줄임 꼴, "bold 16px DOSGothic" → 크기 16, 굵기 700, 이름 DOSGothic)
// - 크기 앞의 낱말: normal, italic / oblique(뒤의 각도는 넘긴다)는 기울임, small-caps, 늘이기 낱말(condensed 등)은 넘긴다.
//   bold / bolder 는 700, lighter 는 100, 숫자는 굵기(1 ~ 1000). 그 밖의 낱말이 오면 틀린 문자열이다.
// - 크기: px, pt, pc, in, cm, mm, q 와 em, rem, %, larger, smaller, 이름 크기(xx-small ~ xxx-large). 뒤의 "/줄높이"는 읽고 넘긴다. (캔버스는 줄높이를 쓰지 않는다)
//   em, rem, %, larger, smaller 는 10px 에 대어 셈한다. (원본이 글을 굽고 재는 캔버스는 문서에 붙이지 않아 브라우저가 기본 글꼴 10px 에 댄다, 크롬으로 확인)
// - 크기 뒤는 쉼표로 이은 이름 목록이다. 따옴표 이름은 따옴표를 떼고, 따옴표 없는 이름은 낱말을 빈칸 하나로 잇는다. 그리는 쪽이 목록의 앞 이름부터 찾는다.
// - 틀린 문자열(크기나 이름이 없음, 음수 크기, 모르는 낱말)은 캔버스의 처음 값 "10px sans-serif" 로 본다.
//==============================================================================
pub fn parse_font_string(font_string: &str) -> FontDescription {
	parse_font_string_strict(font_string).unwrap_or_else(|| FontDescription {
		font_size: 10.0,
		font_weight: 400,
		is_bold: false,
		is_italic: false,
		family_name: String::from("sans-serif"),
		family_name_list: vec![String::from("sans-serif")],
	})
}

//==============================================================================
// 폰트 문자열 해석. (틀리면 None)
//==============================================================================
fn parse_font_string_strict(font_string: &str) -> Option<FontDescription> {
	let mut font_weight: u16 = 400;
	let mut is_italic = false;
	let mut remainder = font_string.trim_start();
	let mut font_size: Option<f32> = None;
	let mut is_after_oblique = false;
	while !remainder.is_empty() {
		let token_end = remainder.find(char::is_whitespace).unwrap_or(remainder.len());
		let token = &remainder[..token_end];
		let lowercase_token = token.to_lowercase();
		// 크기. ("16px", "16px/1.5" 처럼 줄높이가 붙어 와도 된다)
		let size_text = lowercase_token.split('/').next().unwrap_or("");
		if let Some(parsed_font_size) = parse_css_font_size(size_text) {
			font_size = Some(parsed_font_size);
			remainder = remainder[size_text.len()..].trim_start();
			// 줄높이. ("/1.5", "/ 20px")
			if let Some(line_height_text) = remainder.strip_prefix('/') {
				let line_height_text = line_height_text.trim_start();
				let line_height_end = line_height_text.find(char::is_whitespace).unwrap_or(line_height_text.len());
				if line_height_end == 0 {
					return None;
				}
				remainder = line_height_text[line_height_end..].trim_start();
			}
			break;
		}
		// oblique 뒤의 각도. ("oblique 10deg")
		let is_angle = is_after_oblique && ["deg", "grad", "rad", "turn"].iter().any(|unit| lowercase_token.strip_suffix(unit).map(|number_text| number_text.parse::<f32>().is_ok()).unwrap_or(false));
		is_after_oblique = false;
		if is_angle {
			remainder = remainder[token_end..].trim_start();
			continue;
		}
		match lowercase_token.as_str() {
			"normal" | "small-caps" | "ultra-condensed" | "extra-condensed" | "condensed" | "semi-condensed" | "semi-expanded" | "expanded" | "extra-expanded" | "ultra-expanded" => {}
			"italic" => {
				is_italic = true;
			}
			"oblique" => {
				is_italic = true;
				is_after_oblique = true;
			}
			"bold" | "bolder" => {
				font_weight = 700;
			}
			"lighter" => {
				font_weight = 100;
			}
			_ => {
				if let Ok(parsed_weight) = lowercase_token.parse::<f32>() {
					if !(1.0..=1000.0).contains(&parsed_weight) {
						return None;
					}
					font_weight = parsed_weight.round() as u16;
				}
				else {
					return None;
				}
			}
		}
		remainder = remainder[token_end..].trim_start();
	}
	let font_size = font_size?;
	let family_name_list = parse_css_family_list(remainder)?;
	Some(FontDescription {
		font_size,
		font_weight,
		is_bold: font_weight >= textshaping::BOLD_WEIGHT_THRESHOLD,
		is_italic,
		family_name: family_name_list[0].clone(),
		family_name_list,
	})
}

//==============================================================================
// CSS 글자 크기 하나 읽기. (픽셀, 크기가 아니거나 음수면 None. 상대 크기는 10px 에 댄다)
//==============================================================================
fn parse_css_font_size(size_text: &str) -> Option<f32> {
	const BASE_FONT_SIZE: f32 = 10.0;
	let named_size = match size_text {
		"xx-small" => Some(9.0),
		"x-small" => Some(10.0),
		"small" => Some(13.0),
		"medium" => Some(16.0),
		"large" => Some(18.0),
		"x-large" => Some(24.0),
		"xx-large" => Some(32.0),
		"xxx-large" => Some(48.0),
		"larger" => Some(BASE_FONT_SIZE * 1.2),
		"smaller" => Some(BASE_FONT_SIZE / 1.2),
		_ => None,
	};
	if named_size.is_some() {
		return named_size;
	}
	// 단위와 1 단위의 픽셀. (CSS 절대 길이: 1in = 96px = 2.54cm = 72pt = 6pc, 1q = 0.25mm)
	let unit_list: [(&str, f32); 10] = [("px", 1.0), ("pt", 96.0 / 72.0), ("pc", 16.0), ("in", 96.0), ("cm", 96.0 / 2.54), ("mm", 96.0 / 25.4), ("q", 96.0 / 101.6), ("rem", BASE_FONT_SIZE), ("em", BASE_FONT_SIZE), ("%", BASE_FONT_SIZE / 100.0)];
	for (unit, pixel_per_unit) in unit_list {
		if let Some(number_text) = size_text.strip_suffix(unit) {
			// "rem" 이 "em" 보다 먼저라 "2rem" 을 "2r" + "em" 으로 읽지 않는다.
			let number = number_text.parse::<f32>().ok()?;
			if !number.is_finite() || number < 0.0 || number_text.starts_with('+') {
				return None;
			}
			return Some(number * pixel_per_unit);
		}
	}
	None
}

//==============================================================================
// CSS 글꼴 이름 목록 읽기. ("'A B', C D, sans-serif" → ["A B", "C D", "sans-serif"], 비었거나 틀리면 None)
//==============================================================================
fn parse_css_family_list(family_list_text: &str) -> Option<Vec<String>> {
	let mut family_name_list: Vec<String> = Vec::new();
	let mut characters = family_list_text.chars().peekable();
	loop {
		while characters.peek().map(|character| character.is_whitespace()).unwrap_or(false) {
			characters.next();
		}
		let family_name = match characters.peek().copied() {
			Some(quote) if quote == '"' || quote == '\'' => {
				characters.next();
				let mut quoted_name = String::new();
				loop {
					match characters.next() {
						Some(character) if character == quote => {
							break;
						}
						Some(character) => {
							quoted_name.push(character);
						}
						None => {
							return None;
						}
					}
				}
				while characters.peek().map(|character| character.is_whitespace()).unwrap_or(false) {
					characters.next();
				}
				if !matches!(characters.peek(), None | Some(',')) {
					return None;
				}
				quoted_name
			}
			_ => {
				let mut unquoted_text = String::new();
				while let Some(character) = characters.peek().copied() {
					if character == ',' {
						break;
					}
					if character == '"' || character == '\'' {
						return None;
					}
					unquoted_text.push(character);
					characters.next();
				}
				let word_list: Vec<&str> = unquoted_text.split_whitespace().collect();
				if word_list.is_empty() {
					return None;
				}
				word_list.join(" ")
			}
		};
		family_name_list.push(family_name);
		match characters.next() {
			Some(',') => {
				continue;
			}
			_ => {
				break;
			}
		}
	}
	Some(family_name_list)
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
		let font_bytes = match crate::platform::find_fallback_font_file("가", 400, false, false).and_then(|source| std::fs::read(source.path).ok()) {
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
	// 폰트 문자열: CSS 단위와 이름 크기, 줄높이, 이름 목록, 틀린 문자열. (크롬 Canvas2D 의 font 가 돌려준 값과 대었다)
	//==============================================================================
	#[test]
	fn parse_font_string_reads_css_units_and_family_list() {
		let read_size = |font_string: &str| -> f32 {
			parse_font_string(font_string).font_size
		};
		assert_eq!(read_size("12pt a"), 16.0);
		assert_eq!(read_size("2em a"), 20.0);
		assert_eq!(read_size("2rem a"), 20.0);
		assert_eq!(read_size("150% a"), 15.0);
		assert_eq!(read_size("medium a"), 16.0);
		assert_eq!(read_size("x-large a"), 24.0);
		assert_eq!(read_size("xx-small a"), 9.0);
		assert_eq!(read_size("larger a"), 12.0);
		assert!((read_size("smaller a") - 8.33333).abs() < 0.001);
		assert_eq!(read_size("1in a"), 96.0);
		assert!((read_size("1cm a") - 37.7953).abs() < 0.001);
		assert!((read_size("10q a") - 9.44882).abs() < 0.001);
		assert_eq!(read_size("1pc a"), 16.0);
		assert_eq!(read_size("16.5px a"), 16.5);
		assert_eq!(read_size("16PX A"), 16.0);
		let line_height_description = parse_font_string("bold 16px/20px a, b");
		assert_eq!(line_height_description.font_size, 16.0);
		assert_eq!(line_height_description.font_weight, 700);
		assert_eq!(line_height_description.family_name_list, vec!["a", "b"]);
		assert_eq!(parse_font_string("16px / 1.5 a").family_name, "a");
		let list_description = parse_font_string("500 16px 'A B', \"C\", sans-serif");
		assert_eq!(list_description.family_name_list, vec!["A B", "C", "sans-serif"]);
		assert_eq!(parse_font_string("italic 16px a  b c").family_name, "a b c");
		let keyword_description = parse_font_string("italic small-caps bold condensed 16px a");
		assert!(keyword_description.is_italic && keyword_description.font_weight == 700);
		assert!(parse_font_string("oblique 10deg 16px a").is_italic);
		assert_eq!(parse_font_string("lighter 16px a").font_weight, 100);
		for invalid_font_string in ["16px", "-5px a", "bold a 16px", "x 16px a", "16px 'a"] {
			let invalid_description = parse_font_string(invalid_font_string);
			assert_eq!(invalid_description.font_size, 10.0, "{}", invalid_font_string);
			assert_eq!(invalid_description.family_name, "sans-serif", "{}", invalid_font_string);
		}
	}

	//==============================================================================
	// 시스템 글꼴 사슬과 운영체제 이름 찾기, 대체 글꼴: 맥 크롬 Canvas2D 의 measureText 값과 같다. (맥만, 같은 글꼴이 있을 때)
	// - 한글은 사슬의 Apple SD Gothic Neo, "16px Helvetica" 와 "16px Menlo" 는 올리지 않은 이름을 운영체제 글꼴에서 찾은 것, 태국어는 운영체제 대체 글꼴.
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn system_font_chain_matches_chrome_widths_on_macos() {
		let mut text_string_texture_cache = TextStringTextureCache::new();
		let system_font_description = parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
		text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
		text_string_texture_cache.set_system_font_fallback_enabled(true);
		if crate::platform::find_family_font_file("Apple SD Gothic Neo", 400, false).is_none() || crate::platform::find_family_font_file("Menlo", 400, false).is_none() {
			return;
		}
		let measure_width = |font_string: &str, text: &str| -> f32 {
			text_string_texture_cache.measure_text(font_string, text).width
		};
		assert!((measure_width("16px default", "가나다라") - 55.360).abs() < 0.01);
		assert!((measure_width("bold 16px default", "가나다라") - 55.360).abs() < 0.01);
		assert!((measure_width("16px Helvetica", "Hello World Wifi") - 113.508).abs() < 0.01);
		assert!((measure_width("16px Menlo", "Hello World Wifi") - 154.125).abs() < 0.01);
		assert!(measure_width("16px default", "ภาษาไทย") > 50.0);
		assert!(measure_width("bold 16px default", "Hello World Wifi") > measure_width("16px default", "Hello World Wifi"));
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
