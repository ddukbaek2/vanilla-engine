//==============================================================================
// 문자열 텍스처 캐시.
// - 원본은 오프스크린 2D 캔버스에 fillText / strokeText 로 문자열을 구워 텍스처로 쓴다. (LRU 상한 512)
// - 여기서는 fontdue 로 글자를 래스터화해 문자열 한 장으로 굽는다. 상한과 키 구성(방식, 폰트 문자열, 선 두께, 배율, 글)은 원본과 같다.
//   원본은 키를 문자열로 이어 붙인다. 여기서는 찾을 때마다 문자열을 만들지 않게 키의 해시로 찾고, 같은 해시 안에서 키를 하나씩 맞춰 본다.
// - 글자는 흰색으로 굽고, 그릴 때 색을 곱한다. (색이 바뀌어도 다시 굽지 않는다)
// - 폰트 문자열은 Canvas2D font 와 같은 꼴을 읽는다. 예: "16px DOSGothic", "bold 24px default", "italic 16px default"
// - 기울임(italic, oblique)은 기울임 글꼴을 찾지 않고 래스터를 가로로 밀어 흉내 낸다. (기준선 위는 오른쪽, 아래는 왼쪽, 높이의 0.2 배. 전진폭은 그대로)
// - 첫 폰트에 없는 글자는 기본 폰트, 대체 폰트 순으로 찾는다. (브라우저의 글꼴 대체와 같은 자리)
// - 웹에서 글꼴 바이트가 하나도 없으면 원본처럼 브라우저 Canvas2D 로 굽고 잰다. 그래서 시스템 글꼴로 나온다. (platform/web/systemfont.rs)
// - 글자 안티앨리어싱(set_text_antialias_enabled, 원본 setTextAntialiasEnabled): 끄면 구운 글자의 알파를 문턱값 128 로 0 과 255 로 가르고 텍스처를 NEAREST 로 뽑는다.
//   도트 글꼴을 설계 크기의 정수 배로 그리는 프로젝트가 끈다. 원본은 모듈 전역 값이고 이미 구운 글자는 그대로 둔다.
//   여기서는 캐시가 들고, 켜짐 여부를 키에 넣어 바꾼 다음 그리는 글자부터 바로 그 방식으로 굽는다. (rasterize_text 는 원본 ShadowText 처럼 받지 않는다)
// - 글 재기(measure_text)는 폰트 문자열과 글이 같으면 다시 재지 않는다. (원본 measureTextMetrics 의 캐시, 2048 개, 넘으면 오래 안 쓴 반을 버린다)
//   글꼴을 새로 등록하면 비운다. (원본은 웹 글꼴이 받아지면 비운다) measure_text 가 &self 라 캐시는 RefCell 에 든다.
// - Graphic 없이 재기(measure_text_without_graphic): 원본은 어디서든 공용 2D 캔버스로 글을 잰다. 러스트 판은 글꼴을 등록할 때마다
//   글꼴 목록(Rc)을 스레드 하나의 전역에 알려 두고, 그 목록으로 잰다. UI 레이아웃이 그리기 전에 글 줄 수를 알아야 할 때 쓴다. (UIConfirmWindow)
//==============================================================================
use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

//==============================================================================
// Graphic 없이 재기에 쓰는 글꼴 목록. (마지막으로 글꼴을 등록한 캐시의 것)
//==============================================================================
struct SharedFontSet {
	font_table: HashMap<String, Rc<fontdue::Font>>,
	default_font: Option<Rc<fontdue::Font>>,
	default_bold_font: Option<Rc<fontdue::Font>>,
	fallback_font_list: Vec<Rc<fontdue::Font>>,
}

thread_local! {
	static SHARED_FONT_SET: RefCell<Option<SharedFontSet>> = const { RefCell::new(None) };
}

//==============================================================================
// Graphic 없이 글 재기. (폰트 문자열과 글, 원본의 공용 캔버스 measureText 자리)
// - 글꼴 바이트가 없으면 웹은 브라우저 시스템 글꼴로 재고, 데스크탑은 None 이다. (그릴 때도 글이 나오지 않는 상태)
//==============================================================================
pub fn measure_text_without_graphic(font_string: &str, text: &str) -> Option<TextMetrics> {
	let font_description = parse_font_string(font_string);
	let font_list: Vec<Rc<fontdue::Font>> = SHARED_FONT_SET.with(|shared_font_set_cell| {
		let shared_font_set = shared_font_set_cell.borrow();
		let shared_font_set = match shared_font_set.as_ref() {
			Some(shared_font_set) => {
				shared_font_set
			}
			None => {
				return Vec::new();
			}
		};
		let mut font_list: Vec<Rc<fontdue::Font>> = Vec::new();
		let font_key = create_font_key(&font_description.family_name, font_description.is_bold);
		let regular_font_key = create_font_key(&font_description.family_name, false);
		if let Some(font) = shared_font_set.font_table.get(&font_key) {
			font_list.push(font.clone());
		}
		else if let Some(font) = shared_font_set.font_table.get(&regular_font_key) {
			font_list.push(font.clone());
		}
		if font_description.is_bold {
			if let Some(default_bold_font) = &shared_font_set.default_bold_font {
				font_list.push(default_bold_font.clone());
			}
		}
		if let Some(default_font) = &shared_font_set.default_font {
			font_list.push(default_font.clone());
		}
		for fallback_font in &shared_font_set.fallback_font_list {
			font_list.push(fallback_font.clone());
		}
		font_list
	});
	if font_list.is_empty() {
		#[cfg(target_arch = "wasm32")]
		{
			return Some(crate::platform::web::systemfont::measure_text(&font_description, text));
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			return None;
		}
	}
	let font_size = font_description.font_size;
	let glyph_placements = layout_glyphs(&font_list, text, font_size);
	let advance_width = calculate_advance_width(&font_list, &glyph_placements, font_size);
	let (font_ascent, font_descent) = read_line_metrics(&font_list[0], font_size);
	Some(TextMetrics {
		width: advance_width,
		font_ascent,
		font_descent,
	})
}

// 캐시 상한. (원본과 같음)
const MAXIMUM_ENTRY_COUNT: usize = 512;
// 글 재기 캐시 상한. (원본과 같음)
const MAXIMUM_MEASURE_COUNT: usize = 2048;

// 안티앨리어싱을 끈 글자의 알파 문턱값. (원본과 같음)
const ALPHA_THRESHOLD: u8 = 128;

// 흉내 낸 기울임의 기울기. (기준선에서 1 픽셀 올라갈 때 오른쪽으로 밀리는 픽셀, 약 11 도)
const ITALIC_SHEAR: f32 = 0.2;

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
//==============================================================================
#[derive(Clone, Debug)]
pub struct TextRaster {
	pub coverage: Vec<u8>,
	pub width: i32,
	pub height: i32,
	pub origin_x: f32,
	pub baseline_y: f32,
	pub advance_width: f32,
	pub font_ascent: f32,
	pub font_descent: f32,
}

//==============================================================================
// 문자열 측정 결과. (Canvas2D TextMetrics 의 쓰는 칸만)
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct TextMetrics {
	pub width: f32,
	pub font_ascent: f32,
	pub font_descent: f32,
}

//==============================================================================
// 폰트 문자열 해석 결과.
//==============================================================================
#[derive(Clone, Debug)]
pub struct FontDescription {
	pub font_size: f32,
	pub is_bold: bool,
	pub is_italic: bool,
	pub family_name: String,
}

struct CachedTextEntry {
	text_draw_kind: TextDrawKind,
	is_antialias: bool,
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
	fn is_same_key(&self, text_draw_kind: TextDrawKind, is_antialias: bool, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> bool {
		self.text_draw_kind == text_draw_kind && self.is_antialias == is_antialias && self.line_width.to_bits() == line_width.to_bits() && self.bake_scale.to_bits() == bake_scale.to_bits() && self.font_string == font_string && self.text == text
	}
}

struct CachedTextMetrics {
	font_string: String,
	text: String,
	text_metrics: TextMetrics,
	last_used_count: u64,
}

struct GlyphPlacement {
	font_index: usize,
	glyph_index: u16,
	pen_x: f32,
	metrics: fontdue::Metrics,
}

pub struct TextStringTextureCache {
	font_table: HashMap<String, Rc<fontdue::Font>>,
	default_font: Option<Rc<fontdue::Font>>,
	default_bold_font: Option<Rc<fontdue::Font>>,
	fallback_font_list: Vec<Rc<fontdue::Font>>,
	// 키의 해시 → 그 해시를 가진 구운 문자열들.
	entry_table: HashMap<u64, Vec<CachedTextEntry>>,
	entry_count: usize,
	frame_number: u64,
	// 글꼴을 등록한 횟수. (늘면 재 둔 글 폭이 바뀌었을 수 있다)
	font_registration_count: u64,
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
			font_table: HashMap::new(),
			default_font: None,
			default_bold_font: None,
			fallback_font_list: Vec::new(),
			entry_table: HashMap::new(),
			entry_count: 0,
			frame_number: 0,
			font_registration_count: 0,
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
	// 폰트 등록. (폰트 문자열의 이름으로 찾는다. 대소문자 구분 없음)
	//==============================================================================
	pub fn register_font(&mut self, family_name: &str, is_bold: bool, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = load_font(font_bytes)?;
		let font_key = create_font_key(family_name, is_bold);
		self.font_table.insert(font_key, Rc::new(font));
		self.font_registration_count += 1;
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	// 기본 폰트 설정. (이름을 찾지 못한 폰트 문자열과 글자 대체에 쓴다)
	//==============================================================================
	pub fn set_default_font(&mut self, is_bold: bool, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(load_font(font_bytes)?);
		if is_bold {
			self.default_bold_font = Some(font);
		}
		else {
			self.default_font = Some(font);
		}
		self.font_registration_count += 1;
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	// 대체 폰트 추가. (앞 폰트들에 없는 글자를 찾는다)
	//==============================================================================
	pub fn add_fallback_font(&mut self, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = load_font(font_bytes)?;
		self.fallback_font_list.push(Rc::new(font));
		self.font_registration_count += 1;
		self.publish_shared_font_set();
		Ok(())
	}

	//==============================================================================
	// 글꼴 목록을 Graphic 없이 재기에 알리기. (measure_text_without_graphic)
	//==============================================================================
	fn publish_shared_font_set(&self) {
		let shared_font_set = SharedFontSet {
			font_table: self.font_table.clone(),
			default_font: self.default_font.clone(),
			default_bold_font: self.default_bold_font.clone(),
			fallback_font_list: self.fallback_font_list.clone(),
		};
		SHARED_FONT_SET.with(|shared_font_set_cell| {
			*shared_font_set_cell.borrow_mut() = Some(shared_font_set);
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
	// 글꼴을 등록한 횟수. (등록, 기본 글꼴, 대체 글꼴. 재 둔 글 폭을 다시 잴지 가린다)
	//==============================================================================
	pub fn get_font_registration_count(&self) -> u64 {
		self.font_registration_count
	}

	//==============================================================================
	// 기본 폰트가 있는지.
	//==============================================================================
	pub fn has_default_font(&self) -> bool {
		self.default_font.is_some()
	}

	//==============================================================================
	// 문자열 측정.
	//==============================================================================
	pub fn measure_text(&self, font_string: &str, text: &str) -> TextMetrics {
		// 글꼴이 새로 들어왔으면 잰 것을 비운다.
		if self.measured_font_registration_count.get() != self.font_registration_count {
			self.measure_table.borrow_mut().clear();
			self.measure_count.set(0);
			self.measured_font_registration_count.set(self.font_registration_count);
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
		let font_list = self.resolve_font_list(&font_description);
		if font_list.is_empty() {
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
		let glyph_placements = layout_glyphs(&font_list, text, font_size);
		let advance_width = calculate_advance_width(&font_list, &glyph_placements, font_size);
		let (font_ascent, font_descent) = read_line_metrics(&font_list[0], font_size);
		TextMetrics {
			width: advance_width,
			font_ascent,
			font_descent,
		}
	}

	//==============================================================================
	// 구운 문자열 반환. (없으면 굽는다)
	//==============================================================================
	pub fn get_entry(&mut self, gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> Option<TextEntry> {
		if text.is_empty() {
			return None;
		}
		if let Some(entry) = self.find_entry(text_draw_kind, text, font_string, line_width, bake_scale) {
			return Some(entry);
		}

		let entry = self.bake_entry(gl, text_draw_kind, text, font_string, line_width, bake_scale)?;
		if self.entry_count >= MAXIMUM_ENTRY_COUNT {
			self.evict_oldest_entry(gl);
		}
		let is_antialias = self.is_text_antialias_enabled;
		let entry_hash = create_entry_hash(text_draw_kind, is_antialias, text, font_string, line_width, bake_scale);
		self.entry_table.entry(entry_hash).or_default().push(CachedTextEntry {
			text_draw_kind,
			is_antialias,
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
	// 이미 구운 문자열 찾기. (GL 을 건드리지 않는다, 없으면 None)
	//==============================================================================
	pub(crate) fn find_entry(&mut self, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> Option<TextEntry> {
		let is_antialias = self.is_text_antialias_enabled;
		let entry_hash = create_entry_hash(text_draw_kind, is_antialias, text, font_string, line_width, bake_scale);
		let frame_number = self.frame_number;
		let cached_text_entry_list = self.entry_table.get_mut(&entry_hash)?;
		let cached_text_entry = cached_text_entry_list.iter_mut().find(|cached_text_entry| cached_text_entry.is_same_key(text_draw_kind, is_antialias, text, font_string, line_width, bake_scale))?;
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
	// - 글꼴 바이트가 없으면 None. (웹에서 시스템 글꼴로 굽는 경우)
	//==============================================================================
	pub fn rasterize_text(&self, text: &str, font_string: &str, bake_scale: f32, padding: i32) -> Option<TextRaster> {
		if text.is_empty() {
			return None;
		}
		let font_description = parse_font_string(font_string);
		let font_list = self.resolve_font_list(&font_description);
		if font_list.is_empty() {
			return None;
		}
		let pixel_size = font_description.font_size * bake_scale;
		let glyph_placements = layout_glyphs(&font_list, text, pixel_size);
		let advance_width = calculate_advance_width(&font_list, &glyph_placements, pixel_size);
		let (font_ascent, font_descent) = read_line_metrics(&font_list[0], pixel_size);
		let mut minimum_x: f32 = 0.0;
		let mut maximum_x: f32 = advance_width;
		let mut top_extent: f32 = font_ascent;
		let mut bottom_extent: f32 = font_descent;
		for glyph_placement in &glyph_placements {
			let metrics = glyph_placement.metrics;
			if metrics.width == 0 || metrics.height == 0 {
				continue;
			}
			let left_x = glyph_placement.pen_x + metrics.xmin as f32;
			minimum_x = minimum_x.min(left_x);
			maximum_x = maximum_x.max(left_x + metrics.width as f32);
			top_extent = top_extent.max((metrics.ymin + metrics.height as i32) as f32);
			bottom_extent = bottom_extent.max(-(metrics.ymin as f32));
		}
		let italic_shear = calculate_italic_shear(&font_description);
		minimum_x -= bottom_extent * italic_shear;
		maximum_x += top_extent * italic_shear;
		let padding = padding.max(1);
		let origin_x = padding as f32 - minimum_x.floor();
		let baseline_y = padding + top_extent.ceil() as i32;
		let width = (origin_x + maximum_x).ceil() as i32 + padding;
		let height = baseline_y + bottom_extent.ceil() as i32 + padding;
		if width <= 0 || height <= 0 {
			return None;
		}
		let mut coverage = vec![0u8; (width * height) as usize];
		for glyph_placement in &glyph_placements {
			let metrics = glyph_placement.metrics;
			if metrics.width == 0 || metrics.height == 0 {
				continue;
			}
			let font = &font_list[glyph_placement.font_index];
			let (glyph_metrics, glyph_bitmap) = font.rasterize_indexed(glyph_placement.glyph_index, pixel_size);
			let glyph_left_x = (origin_x + glyph_placement.pen_x).round() as i32 + glyph_metrics.xmin;
			let glyph_top_y = baseline_y - (glyph_metrics.ymin + glyph_metrics.height as i32);
			for glyph_y in 0..glyph_metrics.height as i32 {
				let bitmap_y = glyph_top_y + glyph_y;
				if bitmap_y < 0 || bitmap_y >= height {
					continue;
				}
				for glyph_x in 0..glyph_metrics.width as i32 {
					let bitmap_x = glyph_left_x + glyph_x;
					if bitmap_x < 0 || bitmap_x >= width {
						continue;
					}
					let glyph_value = glyph_bitmap[(glyph_y * glyph_metrics.width as i32 + glyph_x) as usize];
					let bitmap_index = (bitmap_y * width + bitmap_x) as usize;
					coverage[bitmap_index] = coverage[bitmap_index].max(glyph_value);
				}
			}
		}
		if italic_shear > 0.0 {
			coverage = shear_coverage(&coverage, width, height, baseline_y as f32, italic_shear);
		}
		Some(TextRaster { coverage, width, height, origin_x, baseline_y: baseline_y as f32, advance_width, font_ascent, font_descent })
	}

	//==============================================================================
	// 구운 문자열 수.
	//==============================================================================
	pub fn get_entry_count(&self) -> usize {
		self.entry_count
	}

	//==============================================================================
	// 폰트 문자열에 맞는 폰트 목록. (첫 번째가 주 폰트, 뒤는 글자 대체 순서)
	//==============================================================================
	fn resolve_font_list(&self, font_description: &FontDescription) -> Vec<Rc<fontdue::Font>> {
		let mut font_list: Vec<Rc<fontdue::Font>> = Vec::new();
		let font_key = create_font_key(&font_description.family_name, font_description.is_bold);
		let regular_font_key = create_font_key(&font_description.family_name, false);
		if let Some(font) = self.font_table.get(&font_key) {
			font_list.push(font.clone());
		}
		else if let Some(font) = self.font_table.get(&regular_font_key) {
			font_list.push(font.clone());
		}
		if font_description.is_bold {
			if let Some(default_bold_font) = &self.default_bold_font {
				font_list.push(default_bold_font.clone());
			}
		}
		if let Some(default_font) = &self.default_font {
			font_list.push(default_font.clone());
		}
		for fallback_font in &self.fallback_font_list {
			font_list.push(fallback_font.clone());
		}
		font_list
	}

	//==============================================================================
	// 문자열 굽기.
	//==============================================================================
	fn bake_entry(&self, gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> Option<TextEntry> {
		let font_description = parse_font_string(font_string);
		let font_list = self.resolve_font_list(&font_description);
		if font_list.is_empty() {
			// 웹: 글꼴 바이트가 없으면 브라우저 시스템 글꼴로 굽는다.
			#[cfg(target_arch = "wasm32")]
			{
				return bake_system_font_entry(gl, text_draw_kind, text, &font_description, line_width, bake_scale, self.is_text_antialias_enabled);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}

		// 배율을 곱한 크기로 배치.
		let pixel_size = font_description.font_size * bake_scale;
		let glyph_placements = layout_glyphs(&font_list, text, pixel_size);
		let advance_width = calculate_advance_width(&font_list, &glyph_placements, pixel_size);
		let (font_ascent, font_descent) = read_line_metrics(&font_list[0], pixel_size);

		// 글자 경계. (펜 원점 기준, 위가 양수)
		let mut minimum_x: f32 = 0.0;
		let mut maximum_x: f32 = advance_width;
		let mut top_extent: f32 = font_ascent;
		let mut bottom_extent: f32 = font_descent;
		for glyph_placement in &glyph_placements {
			let metrics = glyph_placement.metrics;
			if metrics.width == 0 || metrics.height == 0 {
				continue;
			}
			let left_x = glyph_placement.pen_x + metrics.xmin as f32;
			let right_x = left_x + metrics.width as f32;
			minimum_x = minimum_x.min(left_x);
			maximum_x = maximum_x.max(right_x);
			top_extent = top_extent.max((metrics.ymin + metrics.height as i32) as f32);
			bottom_extent = bottom_extent.max(-(metrics.ymin as f32));
		}

		// 기울이면 기준선 위는 오른쪽, 아래는 왼쪽으로 밀리므로 그만큼 경계를 넓힌다.
		let italic_shear = calculate_italic_shear(&font_description);
		minimum_x -= bottom_extent * italic_shear;
		maximum_x += top_extent * italic_shear;

		// 비트맵 크기. (외곽선 두께만큼 여백)
		let stroke_radius = match text_draw_kind {
			TextDrawKind::Stroke => {
				(line_width * bake_scale * 0.5).max(0.5)
			}
			TextDrawKind::Fill => {
				0.0
			}
		};
		let padding = stroke_radius.ceil() as i32 + 1;
		let origin_x = padding as f32 - minimum_x.floor();
		let baseline_y = padding + top_extent.ceil() as i32;
		let bitmap_width = (origin_x + maximum_x).ceil() as i32 + padding;
		let bitmap_height = baseline_y + bottom_extent.ceil() as i32 + padding;
		if bitmap_width <= 0 || bitmap_height <= 0 {
			return None;
		}

		// 글자 래스터화. (겹치는 곳은 큰 값)
		let mut coverage = vec![0u8; (bitmap_width * bitmap_height) as usize];
		for glyph_placement in &glyph_placements {
			let metrics = glyph_placement.metrics;
			if metrics.width == 0 || metrics.height == 0 {
				continue;
			}
			let font = &font_list[glyph_placement.font_index];
			let (glyph_metrics, glyph_bitmap) = font.rasterize_indexed(glyph_placement.glyph_index, pixel_size);
			let glyph_left_x = (origin_x + glyph_placement.pen_x).round() as i32 + glyph_metrics.xmin;
			let glyph_top_y = baseline_y - (glyph_metrics.ymin + glyph_metrics.height as i32);
			for glyph_y in 0..glyph_metrics.height as i32 {
				let bitmap_y = glyph_top_y + glyph_y;
				if bitmap_y < 0 || bitmap_y >= bitmap_height {
					continue;
				}
				for glyph_x in 0..glyph_metrics.width as i32 {
					let bitmap_x = glyph_left_x + glyph_x;
					if bitmap_x < 0 || bitmap_x >= bitmap_width {
						continue;
					}
					let glyph_value = glyph_bitmap[(glyph_y * glyph_metrics.width as i32 + glyph_x) as usize];
					let bitmap_index = (bitmap_y * bitmap_width + bitmap_x) as usize;
					coverage[bitmap_index] = coverage[bitmap_index].max(glyph_value);
				}
			}
		}

		// 기울임 흉내. (외곽선은 기울인 모양에 긋는다)
		if italic_shear > 0.0 {
			coverage = shear_coverage(&coverage, bitmap_width, bitmap_height, baseline_y as f32, italic_shear);
		}

		// 외곽선: 윤곽 바깥으로 반두께 넓힌 것에서 안으로 반두께 좁힌 것을 뺀 띠. (Canvas2D strokeText 는 윤곽선 가운데에 선을 긋는다)
		if text_draw_kind == TextDrawKind::Stroke {
			coverage = build_stroke_coverage(&coverage, bitmap_width, bitmap_height, stroke_radius);
		}

		// 안티앨리어싱을 끈 경우: 문턱값으로 갈라 가장자리의 반투명 픽셀을 없앤다.
		let is_antialias = self.is_text_antialias_enabled;
		if !is_antialias {
			apply_alpha_threshold(&mut coverage);
		}

		// 흰색 프리멀티플라이드 RGBA 로 올리기.
		let mut pixels = Vec::with_capacity(coverage.len() * 4);
		for value in &coverage {
			pixels.push(*value);
			pixels.push(*value);
			pixels.push(*value);
			pixels.push(*value);
		}
		let texture = upload_text_texture(gl, bitmap_width, bitmap_height, &pixels, is_antialias)?;

		Some(TextEntry {
			texture,
			advance_width: advance_width / bake_scale,
			font_ascent: font_ascent / bake_scale,
			font_descent: font_descent / bake_scale,
			pen_offset_x: origin_x / bake_scale,
			baseline_offset_y: baseline_y as f32 / bake_scale,
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
// 브라우저 시스템 글꼴로 굽기. (웹. 글꼴 바이트가 하나도 없을 때)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn bake_system_font_entry(gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_description: &FontDescription, line_width: f32, bake_scale: f32, is_antialias: bool) -> Option<TextEntry> {
	let mut system_font_bitmap = crate::platform::web::systemfont::bake_text(text_draw_kind, text, font_description, line_width, bake_scale)?;
	if !is_antialias {
		// 흰색 프리멀티플라이드라 네 칸 모두 덮인 정도다.
		apply_alpha_threshold(&mut system_font_bitmap.pixels);
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
// 구운 문자열 키의 해시. (방식, 안티앨리어싱, 폰트 문자열, 선 두께, 배율, 글)
//==============================================================================
fn create_entry_hash(text_draw_kind: TextDrawKind, is_antialias: bool, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> u64 {
	let mut hasher = DefaultHasher::new();
	text_draw_kind.hash(&mut hasher);
	is_antialias.hash(&mut hasher);
	font_string.hash(&mut hasher);
	line_width.to_bits().hash(&mut hasher);
	bake_scale.to_bits().hash(&mut hasher);
	text.hash(&mut hasher);
	hasher.finish()
}

//==============================================================================
// 폰트 문자열 해석. ("bold 16px DOSGothic" → 크기 16, 굵게, 이름 DOSGothic)
//==============================================================================
pub fn parse_font_string(font_string: &str) -> FontDescription {
	let mut font_size: f32 = 10.0;
	let mut is_bold = false;
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
			is_bold = true;
			continue;
		}
		if lowercase_token == "italic" || lowercase_token == "oblique" {
			is_italic = true;
			continue;
		}
		if let Ok(font_weight) = lowercase_token.parse::<u32>() {
			if font_weight >= 600 {
				is_bold = true;
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
	let family_name = family_name_parts.join(" ").trim_matches(|character| character == '"' || character == '\'').to_string();
	FontDescription {
		font_size,
		is_bold,
		is_italic,
		family_name,
	}
}

//==============================================================================
// 폰트 표의 키.
//==============================================================================
fn create_font_key(family_name: &str, is_bold: bool) -> String {
	let lowercase_family_name = family_name.to_lowercase();
	if is_bold {
		format!("{}|bold", lowercase_family_name)
	}
	else {
		lowercase_family_name
	}
}

//==============================================================================
// 폰트 읽기.
//==============================================================================
fn load_font(font_bytes: Vec<u8>) -> Result<fontdue::Font, String> {
	let font_bytes = decompress_web_font(font_bytes)?;
	fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default()).map_err(|error| format!("폰트 읽기 실패: {}", error))
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
fn read_line_metrics(font: &fontdue::Font, pixel_size: f32) -> (f32, f32) {
	match font.horizontal_line_metrics(pixel_size) {
		Some(line_metrics) => {
			(line_metrics.ascent, -line_metrics.descent)
		}
		None => {
			(pixel_size * 0.8, pixel_size * 0.2)
		}
	}
}

//==============================================================================
// 글자 배치. (글자마다 그 글자를 가진 첫 폰트를 고른다)
//==============================================================================
fn layout_glyphs(font_list: &[Rc<fontdue::Font>], text: &str, pixel_size: f32) -> Vec<GlyphPlacement> {
	let mut glyph_placements: Vec<GlyphPlacement> = Vec::new();
	let mut pen_x: f32 = 0.0;
	let mut previous_glyph: Option<(usize, u16)> = None;
	for character in text.chars() {
		let mut resolved_font_index = 0;
		let mut resolved_glyph_index = font_list[0].lookup_glyph_index(character);
		if resolved_glyph_index == 0 {
			for (font_index, font) in font_list.iter().enumerate().skip(1) {
				let glyph_index = font.lookup_glyph_index(character);
				if glyph_index != 0 {
					resolved_font_index = font_index;
					resolved_glyph_index = glyph_index;
					break;
				}
			}
		}
		let font = &font_list[resolved_font_index];
		if let Some((previous_font_index, previous_glyph_index)) = previous_glyph {
			if previous_font_index == resolved_font_index {
				if let Some(kerning) = font.horizontal_kern_indexed(previous_glyph_index, resolved_glyph_index, pixel_size) {
					pen_x += kerning;
				}
			}
		}
		let metrics = font.metrics_indexed(resolved_glyph_index, pixel_size);
		glyph_placements.push(GlyphPlacement {
			font_index: resolved_font_index,
			glyph_index: resolved_glyph_index,
			pen_x,
			metrics,
		});
		pen_x += metrics.advance_width;
		previous_glyph = Some((resolved_font_index, resolved_glyph_index));
	}
	glyph_placements
}

//==============================================================================
// 배치된 글자들의 전체 전진 폭.
//==============================================================================
fn calculate_advance_width(_font_list: &[Rc<fontdue::Font>], glyph_placements: &[GlyphPlacement], _pixel_size: f32) -> f32 {
	match glyph_placements.last() {
		Some(last_glyph_placement) => {
			last_glyph_placement.pen_x + last_glyph_placement.metrics.advance_width
		}
		None => {
			0.0
		}
	}
}

//==============================================================================
// 흉내 낸 기울임의 기울기. (italic 이 아니면 0)
//==============================================================================
fn calculate_italic_shear(font_description: &FontDescription) -> f32 {
	if font_description.is_italic {
		ITALIC_SHEAR
	}
	else {
		0.0
	}
}

//==============================================================================
// 덮인 정도를 가로로 기울이기. (기준선 위 줄은 오른쪽, 아래 줄은 왼쪽으로 줄 높이 × 기울기만큼 밀고, 두 픽셀 사이는 섞는다)
//==============================================================================
fn shear_coverage(coverage: &[u8], bitmap_width: i32, bitmap_height: i32, baseline_y: f32, shear: f32) -> Vec<u8> {
	let read_coverage = |position_x: i32, position_y: i32| -> f32 {
		if position_x < 0 || position_x >= bitmap_width {
			return 0.0;
		}
		coverage[(position_y * bitmap_width + position_x) as usize] as f32
	};
	let mut sheared_coverage = vec![0u8; coverage.len()];
	for position_y in 0..bitmap_height {
		let shift = (baseline_y - (position_y as f32 + 0.5)) * shear;
		let shift_floor = shift.floor();
		let fraction = shift - shift_floor;
		let shift_integer = shift_floor as i32;
		for position_x in 0..bitmap_width {
			// 밀린 자리 x 의 값은 원래 자리 x - shift 에서 온다.
			let source_x = position_x - shift_integer;
			let value = read_coverage(source_x, position_y) * (1.0 - fraction) + read_coverage(source_x - 1, position_y) * fraction;
			sheared_coverage[(position_y * bitmap_width + position_x) as usize] = value.round().clamp(0.0, 255.0) as u8;
		}
	}
	sheared_coverage
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
		let glyph_count = font.glyph_count();
		let hangul_glyph_index = font.lookup_glyph_index('가');
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
		let first_hash = create_entry_hash(TextDrawKind::Fill, true, "가나다", "16px default", 0.0, 1.5);
		let second_hash = create_entry_hash(TextDrawKind::Fill, true, "가나다", "16px default", 0.0, 1.5);
		assert_eq!(first_hash, second_hash);
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Stroke, true, "가나다", "16px default", 0.0, 1.5));
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Fill, true, "가나다", "16px default", 0.0, 2.0));
		assert_ne!(first_hash, create_entry_hash(TextDrawKind::Fill, false, "가나다", "16px default", 0.0, 1.5));
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
			text: String::from("가나다"),
			font_string: String::from("16px default"),
			line_width: 0.0,
			bake_scale: 1.5,
			entry: texture_entry,
			last_used_frame_number: 0,
		};
		assert!(cached_text_entry.is_same_key(TextDrawKind::Fill, true, "가나다", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, false, "가나다", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, "가나", "16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, "가나다", "bold 16px default", 0.0, 1.5));
		assert!(!cached_text_entry.is_same_key(TextDrawKind::Fill, true, "가나다", "16px default", 2.0, 1.5));
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
	// italic 을 읽고, 기울이면 기준선 위는 오른쪽으로 아래는 왼쪽으로 밀린다.
	//==============================================================================
	#[test]
	fn italic_shears_coverage() {
		let font_description = parse_font_string("italic bold 20px \"My Font\"");
		assert!(font_description.is_italic);
		assert!(font_description.is_bold);
		assert_eq!(font_description.family_name, "My Font");
		assert!(!parse_font_string("20px default").is_italic);
		// 세로 막대 하나. (x = 5, 줄 0 ~ 9, 기준선은 줄 10)
		let width = 12;
		let height = 12;
		let mut coverage = vec![0u8; (width * height) as usize];
		for position_y in 0..10 {
			coverage[(position_y * width + 5) as usize] = 255;
		}
		let sheared_coverage = shear_coverage(&coverage, width, height, 10.0, 0.5);
		let find_center_x = |position_y: i32| -> f32 {
			let mut weighted_sum = 0.0;
			let mut weight_sum = 0.0;
			for position_x in 0..width {
				let value = sheared_coverage[(position_y * width + position_x) as usize] as f32;
				weighted_sum += value * position_x as f32;
				weight_sum += value;
			}
			weighted_sum / weight_sum
		};
		assert!((find_center_x(9) - 5.25).abs() < 0.05);
		assert!((find_center_x(0) - 9.75).abs() < 0.05);
		assert!(find_center_x(0) > find_center_x(5));
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
