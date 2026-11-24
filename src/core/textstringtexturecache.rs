//==============================================================================
// 문자열 텍스처 캐시.
// - 원본은 오프스크린 2D 캔버스에 fillText / strokeText 로 문자열을 구워 텍스처로 쓴다. (LRU 상한 512)
// - 여기서는 fontdue 로 글자를 래스터화해 문자열 한 장으로 굽는다. 상한과 키 구성은 원본과 같다.
// - 글자는 흰색으로 굽고, 그릴 때 색을 곱한다. (색이 바뀌어도 다시 굽지 않는다)
// - 폰트 문자열은 Canvas2D font 와 같은 꼴을 읽는다. 예: "16px DOSGothic", "bold 24px default"
// - 첫 폰트에 없는 글자는 기본 폰트, 대체 폰트 순으로 찾는다. (브라우저의 글꼴 대체와 같은 자리)
// - 웹에서 글꼴 바이트가 하나도 없으면 원본처럼 브라우저 Canvas2D 로 굽고 잰다. 그래서 시스템 글꼴로 나온다. (platform/web/systemfont.rs)
//==============================================================================
use glow::HasContext;
use std::collections::HashMap;
use std::rc::Rc;

// 캐시 상한. (원본과 같음)
const MAXIMUM_ENTRY_COUNT: usize = 512;

//==============================================================================
// 굽는 방식.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
	pub family_name: String,
}

struct CachedTextEntry {
	entry: TextEntry,
	last_used_frame_number: u64,
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
	entry_table: HashMap<String, CachedTextEntry>,
	frame_number: u64,
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
			frame_number: 0,
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
		Ok(())
	}

	//==============================================================================
	// 대체 폰트 추가. (앞 폰트들에 없는 글자를 찾는다)
	//==============================================================================
	pub fn add_fallback_font(&mut self, font_bytes: Vec<u8>) -> Result<(), String> {
		let font = load_font(font_bytes)?;
		self.fallback_font_list.push(Rc::new(font));
		Ok(())
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

		let entry_key = format!("{:?}|{}|{}|{}|{}", text_draw_kind, font_string, line_width, bake_scale, text);
		let frame_number = self.frame_number;
		let entry = self.bake_entry(gl, text_draw_kind, text, font_string, line_width, bake_scale)?;
		if self.entry_table.len() >= MAXIMUM_ENTRY_COUNT {
			self.evict_oldest_entry(gl);
		}
		self.entry_table.insert(entry_key, CachedTextEntry {
			entry,
			last_used_frame_number: frame_number,
		});
		Some(entry)
	}

	//==============================================================================
	// 이미 구운 문자열 찾기. (GL 을 건드리지 않는다, 없으면 None)
	//==============================================================================
	pub(crate) fn find_entry(&mut self, text_draw_kind: TextDrawKind, text: &str, font_string: &str, line_width: f32, bake_scale: f32) -> Option<TextEntry> {
		let entry_key = format!("{:?}|{}|{}|{}|{}", text_draw_kind, font_string, line_width, bake_scale, text);
		let frame_number = self.frame_number;
		let cached_text_entry = self.entry_table.get_mut(&entry_key)?;
		cached_text_entry.last_used_frame_number = frame_number;
		Some(cached_text_entry.entry)
	}

	//==============================================================================
	// 가장 오래 안 쓴 문자열 하나 버리기.
	//==============================================================================
	fn evict_oldest_entry(&mut self, gl: &glow::Context) {
		let oldest_entry_key = self.entry_table.iter().min_by_key(|(_, cached_text_entry)| cached_text_entry.last_used_frame_number).map(|(entry_key, _)| entry_key.clone());
		if let Some(oldest_entry_key) = oldest_entry_key {
			if let Some(cached_text_entry) = self.entry_table.remove(&oldest_entry_key) {
				unsafe {
					gl.delete_texture(cached_text_entry.entry.texture);
				}
			}
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
		Some(TextRaster { coverage, width, height, origin_x, baseline_y: baseline_y as f32, advance_width, font_ascent, font_descent })
	}

	//==============================================================================
	// 구운 문자열 수.
	//==============================================================================
	pub fn get_entry_count(&self) -> usize {
		self.entry_table.len()
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
				return bake_system_font_entry(gl, text_draw_kind, text, &font_description, line_width, bake_scale);
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

		// 외곽선: 윤곽 바깥으로 반두께 넓힌 것에서 안으로 반두께 좁힌 것을 뺀 띠. (Canvas2D strokeText 는 윤곽선 가운데에 선을 긋는다)
		if text_draw_kind == TextDrawKind::Stroke {
			coverage = build_stroke_coverage(&coverage, bitmap_width, bitmap_height, stroke_radius);
		}

		// 흰색 프리멀티플라이드 RGBA 로 올리기.
		let mut pixels = Vec::with_capacity(coverage.len() * 4);
		for value in &coverage {
			pixels.push(*value);
			pixels.push(*value);
			pixels.push(*value);
			pixels.push(*value);
		}
		let texture = upload_text_texture(gl, bitmap_width, bitmap_height, &pixels)?;

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
fn bake_system_font_entry(gl: &glow::Context, text_draw_kind: TextDrawKind, text: &str, font_description: &FontDescription, line_width: f32, bake_scale: f32) -> Option<TextEntry> {
	let system_font_bitmap = crate::platform::web::systemfont::bake_text(text_draw_kind, text, font_description, line_width, bake_scale)?;
	let texture = upload_text_texture(gl, system_font_bitmap.bitmap_width, system_font_bitmap.bitmap_height, &system_font_bitmap.pixels)?;
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
// 구운 픽셀을 텍스처로 올리기. (흰색 프리멀티플라이드 RGBA)
//==============================================================================
fn upload_text_texture(gl: &glow::Context, bitmap_width: i32, bitmap_height: i32, pixels: &[u8]) -> Option<crate::core::graphic::GlTexture> {
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
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
		Some(texture)
	}
}

//==============================================================================
// 폰트 문자열 해석. ("bold 16px DOSGothic" → 크기 16, 굵게, 이름 DOSGothic)
//==============================================================================
pub fn parse_font_string(font_string: &str) -> FontDescription {
	let mut font_size: f32 = 10.0;
	let mut is_bold = false;
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
	// 웹 글꼴이 아닌 바이트는 그대로 둔다.
	//==============================================================================
	#[test]
	fn keep_plain_font_bytes() {
		let plain_bytes: Vec<u8> = vec![0, 1, 0, 0, 9, 9];
		let result_bytes = decompress_web_font(plain_bytes.clone()).expect("plain bytes");
		assert_eq!(result_bytes, plain_bytes);
	}
}
