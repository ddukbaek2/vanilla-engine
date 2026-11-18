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
	fill,
	stroke,
}

//==============================================================================
// 구운 문자열 하나. (길이 단위는 모두 논리 픽셀, 베이크 배율로 나눈 값)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct TextEntry {
	pub texture: crate::core::graphic::GlTexture,
	pub advanceWidth: f32,
	pub fontAscent: f32,
	pub fontDescent: f32,
	pub penOffsetX: f32,
	pub baselineOffsetY: f32,
	pub quadWidth: f32,
	pub quadHeight: f32,
}

//==============================================================================
// 문자열 측정 결과. (Canvas2D TextMetrics 의 쓰는 칸만)
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct TextMetrics {
	pub width: f32,
	pub fontAscent: f32,
	pub fontDescent: f32,
}

//==============================================================================
// 폰트 문자열 해석 결과.
//==============================================================================
#[derive(Clone, Debug)]
pub struct FontDescription {
	pub fontSize: f32,
	pub isBold: bool,
	pub familyName: String,
}

struct CachedTextEntry {
	entry: TextEntry,
	lastUsedFrameNumber: u64,
}

struct GlyphPlacement {
	fontIndex: usize,
	glyphIndex: u16,
	penX: f32,
	metrics: fontdue::Metrics,
}

pub struct TextStringTextureCache {
	fontTable: HashMap<String, Rc<fontdue::Font>>,
	defaultFont: Option<Rc<fontdue::Font>>,
	defaultBoldFont: Option<Rc<fontdue::Font>>,
	fallbackFontList: Vec<Rc<fontdue::Font>>,
	entryTable: HashMap<String, CachedTextEntry>,
	frameNumber: u64,
}

impl TextStringTextureCache {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TextStringTextureCache {
		TextStringTextureCache {
			fontTable: HashMap::new(),
			defaultFont: None,
			defaultBoldFont: None,
			fallbackFontList: Vec::new(),
			entryTable: HashMap::new(),
			frameNumber: 0,
		}
	}

	//==============================================================================
	// 프레임 시작. (LRU 순서를 프레임 번호로 센다)
	//==============================================================================
	pub fn beginFrame(&mut self) {
		self.frameNumber += 1;
	}

	//==============================================================================
	// 폰트 등록. (폰트 문자열의 이름으로 찾는다. 대소문자 구분 없음)
	//==============================================================================
	pub fn registerFont(&mut self, familyName: &str, isBold: bool, fontBytes: Vec<u8>) -> Result<(), String> {
		let font = loadFont(fontBytes)?;
		let fontKey = createFontKey(familyName, isBold);
		self.fontTable.insert(fontKey, Rc::new(font));
		Ok(())
	}

	//==============================================================================
	// 기본 폰트 설정. (이름을 찾지 못한 폰트 문자열과 글자 대체에 쓴다)
	//==============================================================================
	pub fn setDefaultFont(&mut self, isBold: bool, fontBytes: Vec<u8>) -> Result<(), String> {
		let font = Rc::new(loadFont(fontBytes)?);
		if isBold {
			self.defaultBoldFont = Some(font);
		}
		else {
			self.defaultFont = Some(font);
		}
		Ok(())
	}

	//==============================================================================
	// 대체 폰트 추가. (앞 폰트들에 없는 글자를 찾는다)
	//==============================================================================
	pub fn addFallbackFont(&mut self, fontBytes: Vec<u8>) -> Result<(), String> {
		let font = loadFont(fontBytes)?;
		self.fallbackFontList.push(Rc::new(font));
		Ok(())
	}

	//==============================================================================
	// 기본 폰트가 있는지.
	//==============================================================================
	pub fn hasDefaultFont(&self) -> bool {
		self.defaultFont.is_some()
	}

	//==============================================================================
	// 문자열 측정.
	//==============================================================================
	pub fn measureText(&self, fontString: &str, text: &str) -> TextMetrics {
		let fontDescription = parseFontString(fontString);
		let fontList = self.resolveFontList(&fontDescription);
		if fontList.is_empty() {
			// 웹: 글꼴 바이트가 없으면 브라우저 시스템 글꼴로 잰다.
			#[cfg(target_arch = "wasm32")]
			{
				return crate::platform::web::systemfont::measureText(&fontDescription, text);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return TextMetrics::default();
			}
		}
		let fontSize = fontDescription.fontSize;
		let glyphPlacements = layoutGlyphs(&fontList, text, fontSize);
		let advanceWidth = calculateAdvanceWidth(&fontList, &glyphPlacements, fontSize);
		let (fontAscent, fontDescent) = readLineMetrics(&fontList[0], fontSize);
		TextMetrics {
			width: advanceWidth,
			fontAscent,
			fontDescent,
		}
	}

	//==============================================================================
	// 구운 문자열 반환. (없으면 굽는다)
	//==============================================================================
	pub fn getEntry(&mut self, gl: &glow::Context, textDrawKind: TextDrawKind, text: &str, fontString: &str, lineWidth: f32, bakeScale: f32) -> Option<TextEntry> {
		if text.is_empty() {
			return None;
		}
		let entryKey = format!("{:?}|{}|{}|{}|{}", textDrawKind, fontString, lineWidth, bakeScale, text);
		let frameNumber = self.frameNumber;
		if let Some(cachedTextEntry) = self.entryTable.get_mut(&entryKey) {
			cachedTextEntry.lastUsedFrameNumber = frameNumber;
			return Some(cachedTextEntry.entry);
		}

		let entry = self.bakeEntry(gl, textDrawKind, text, fontString, lineWidth, bakeScale)?;
		if self.entryTable.len() >= MAXIMUM_ENTRY_COUNT {
			self.evictOldestEntry(gl);
		}
		self.entryTable.insert(entryKey, CachedTextEntry {
			entry,
			lastUsedFrameNumber: frameNumber,
		});
		Some(entry)
	}

	//==============================================================================
	// 가장 오래 안 쓴 문자열 하나 버리기.
	//==============================================================================
	fn evictOldestEntry(&mut self, gl: &glow::Context) {
		let oldestEntryKey = self.entryTable.iter().min_by_key(|(_, cachedTextEntry)| cachedTextEntry.lastUsedFrameNumber).map(|(entryKey, _)| entryKey.clone());
		if let Some(oldestEntryKey) = oldestEntryKey {
			if let Some(cachedTextEntry) = self.entryTable.remove(&oldestEntryKey) {
				unsafe {
					gl.delete_texture(cachedTextEntry.entry.texture);
				}
			}
		}
	}

	//==============================================================================
	// 구운 문자열 수.
	//==============================================================================
	pub fn getEntryCount(&self) -> usize {
		self.entryTable.len()
	}

	//==============================================================================
	// 폰트 문자열에 맞는 폰트 목록. (첫 번째가 주 폰트, 뒤는 글자 대체 순서)
	//==============================================================================
	fn resolveFontList(&self, fontDescription: &FontDescription) -> Vec<Rc<fontdue::Font>> {
		let mut fontList: Vec<Rc<fontdue::Font>> = Vec::new();
		let fontKey = createFontKey(&fontDescription.familyName, fontDescription.isBold);
		let regularFontKey = createFontKey(&fontDescription.familyName, false);
		if let Some(font) = self.fontTable.get(&fontKey) {
			fontList.push(font.clone());
		}
		else if let Some(font) = self.fontTable.get(&regularFontKey) {
			fontList.push(font.clone());
		}
		if fontDescription.isBold {
			if let Some(defaultBoldFont) = &self.defaultBoldFont {
				fontList.push(defaultBoldFont.clone());
			}
		}
		if let Some(defaultFont) = &self.defaultFont {
			fontList.push(defaultFont.clone());
		}
		for fallbackFont in &self.fallbackFontList {
			fontList.push(fallbackFont.clone());
		}
		fontList
	}

	//==============================================================================
	// 문자열 굽기.
	//==============================================================================
	fn bakeEntry(&self, gl: &glow::Context, textDrawKind: TextDrawKind, text: &str, fontString: &str, lineWidth: f32, bakeScale: f32) -> Option<TextEntry> {
		let fontDescription = parseFontString(fontString);
		let fontList = self.resolveFontList(&fontDescription);
		if fontList.is_empty() {
			// 웹: 글꼴 바이트가 없으면 브라우저 시스템 글꼴로 굽는다.
			#[cfg(target_arch = "wasm32")]
			{
				return bakeSystemFontEntry(gl, textDrawKind, text, &fontDescription, lineWidth, bakeScale);
			}
			#[cfg(not(target_arch = "wasm32"))]
			{
				return None;
			}
		}

		// 배율을 곱한 크기로 배치.
		let pixelSize = fontDescription.fontSize * bakeScale;
		let glyphPlacements = layoutGlyphs(&fontList, text, pixelSize);
		let advanceWidth = calculateAdvanceWidth(&fontList, &glyphPlacements, pixelSize);
		let (fontAscent, fontDescent) = readLineMetrics(&fontList[0], pixelSize);

		// 글자 경계. (펜 원점 기준, 위가 양수)
		let mut minimumX: f32 = 0.0;
		let mut maximumX: f32 = advanceWidth;
		let mut topExtent: f32 = fontAscent;
		let mut bottomExtent: f32 = fontDescent;
		for glyphPlacement in &glyphPlacements {
			let metrics = glyphPlacement.metrics;
			if metrics.width == 0 || metrics.height == 0 {
				continue;
			}
			let leftX = glyphPlacement.penX + metrics.xmin as f32;
			let rightX = leftX + metrics.width as f32;
			minimumX = minimumX.min(leftX);
			maximumX = maximumX.max(rightX);
			topExtent = topExtent.max((metrics.ymin + metrics.height as i32) as f32);
			bottomExtent = bottomExtent.max(-(metrics.ymin as f32));
		}

		// 비트맵 크기. (외곽선 두께만큼 여백)
		let strokeRadius = match textDrawKind {
			TextDrawKind::stroke => {
				(lineWidth * bakeScale * 0.5).max(0.5)
			}
			TextDrawKind::fill => {
				0.0
			}
		};
		let padding = strokeRadius.ceil() as i32 + 1;
		let originX = padding as f32 - minimumX.floor();
		let baselineY = padding + topExtent.ceil() as i32;
		let bitmapWidth = (originX + maximumX).ceil() as i32 + padding;
		let bitmapHeight = baselineY + bottomExtent.ceil() as i32 + padding;
		if bitmapWidth <= 0 || bitmapHeight <= 0 {
			return None;
		}

		// 글자 래스터화. (겹치는 곳은 큰 값)
		let mut coverage = vec![0u8; (bitmapWidth * bitmapHeight) as usize];
		for glyphPlacement in &glyphPlacements {
			let metrics = glyphPlacement.metrics;
			if metrics.width == 0 || metrics.height == 0 {
				continue;
			}
			let font = &fontList[glyphPlacement.fontIndex];
			let (glyphMetrics, glyphBitmap) = font.rasterize_indexed(glyphPlacement.glyphIndex, pixelSize);
			let glyphLeftX = (originX + glyphPlacement.penX).round() as i32 + glyphMetrics.xmin;
			let glyphTopY = baselineY - (glyphMetrics.ymin + glyphMetrics.height as i32);
			for glyphY in 0..glyphMetrics.height as i32 {
				let bitmapY = glyphTopY + glyphY;
				if bitmapY < 0 || bitmapY >= bitmapHeight {
					continue;
				}
				for glyphX in 0..glyphMetrics.width as i32 {
					let bitmapX = glyphLeftX + glyphX;
					if bitmapX < 0 || bitmapX >= bitmapWidth {
						continue;
					}
					let glyphValue = glyphBitmap[(glyphY * glyphMetrics.width as i32 + glyphX) as usize];
					let bitmapIndex = (bitmapY * bitmapWidth + bitmapX) as usize;
					coverage[bitmapIndex] = coverage[bitmapIndex].max(glyphValue);
				}
			}
		}

		// 외곽선: 윤곽 바깥으로 반두께 넓힌 것에서 안으로 반두께 좁힌 것을 뺀 띠. (Canvas2D strokeText 는 윤곽선 가운데에 선을 긋는다)
		if textDrawKind == TextDrawKind::stroke {
			coverage = buildStrokeCoverage(&coverage, bitmapWidth, bitmapHeight, strokeRadius);
		}

		// 흰색 프리멀티플라이드 RGBA 로 올리기.
		let mut pixels = Vec::with_capacity(coverage.len() * 4);
		for value in &coverage {
			pixels.push(*value);
			pixels.push(*value);
			pixels.push(*value);
			pixels.push(*value);
		}
		let texture = uploadTextTexture(gl, bitmapWidth, bitmapHeight, &pixels)?;

		Some(TextEntry {
			texture,
			advanceWidth: advanceWidth / bakeScale,
			fontAscent: fontAscent / bakeScale,
			fontDescent: fontDescent / bakeScale,
			penOffsetX: originX / bakeScale,
			baselineOffsetY: baselineY as f32 / bakeScale,
			quadWidth: bitmapWidth as f32 / bakeScale,
			quadHeight: bitmapHeight as f32 / bakeScale,
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
fn bakeSystemFontEntry(gl: &glow::Context, textDrawKind: TextDrawKind, text: &str, fontDescription: &FontDescription, lineWidth: f32, bakeScale: f32) -> Option<TextEntry> {
	let systemFontBitmap = crate::platform::web::systemfont::bakeText(textDrawKind, text, fontDescription, lineWidth, bakeScale)?;
	let texture = uploadTextTexture(gl, systemFontBitmap.bitmapWidth, systemFontBitmap.bitmapHeight, &systemFontBitmap.pixels)?;
	Some(TextEntry {
		texture,
		advanceWidth: systemFontBitmap.advanceWidth / bakeScale,
		fontAscent: systemFontBitmap.fontAscent / bakeScale,
		fontDescent: systemFontBitmap.fontDescent / bakeScale,
		penOffsetX: systemFontBitmap.penX / bakeScale,
		baselineOffsetY: systemFontBitmap.baselineY / bakeScale,
		quadWidth: systemFontBitmap.bitmapWidth as f32 / bakeScale,
		quadHeight: systemFontBitmap.bitmapHeight as f32 / bakeScale,
	})
}

//==============================================================================
// 구운 픽셀을 텍스처로 올리기. (흰색 프리멀티플라이드 RGBA)
//==============================================================================
fn uploadTextTexture(gl: &glow::Context, bitmapWidth: i32, bitmapHeight: i32, pixels: &[u8]) -> Option<crate::core::graphic::GlTexture> {
	unsafe {
		let texture = gl.create_texture().ok()?;
		gl.bind_texture(glow::TEXTURE_2D, Some(texture));
		gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
		gl.tex_image_2d(
			glow::TEXTURE_2D,
			0,
			glow::RGBA8 as i32,
			bitmapWidth,
			bitmapHeight,
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
pub fn parseFontString(fontString: &str) -> FontDescription {
	let mut fontSize: f32 = 10.0;
	let mut isBold = false;
	let mut familyNameParts: Vec<&str> = Vec::new();
	let mut hasFoundSize = false;
	for token in fontString.split_whitespace() {
		if hasFoundSize {
			familyNameParts.push(token);
			continue;
		}
		let lowercaseToken = token.to_lowercase();
		if lowercaseToken == "bold" || lowercaseToken == "bolder" {
			isBold = true;
			continue;
		}
		if let Ok(fontWeight) = lowercaseToken.parse::<u32>() {
			if fontWeight >= 600 {
				isBold = true;
			}
			continue;
		}
		if let Some(sizeText) = lowercaseToken.strip_suffix("px") {
			if let Ok(parsedFontSize) = sizeText.parse::<f32>() {
				fontSize = parsedFontSize;
				hasFoundSize = true;
			}
			continue;
		}
	}
	let familyName = familyNameParts.join(" ").trim_matches(|character| character == '"' || character == '\'').to_string();
	FontDescription {
		fontSize,
		isBold,
		familyName,
	}
}

//==============================================================================
// 폰트 표의 키.
//==============================================================================
fn createFontKey(familyName: &str, isBold: bool) -> String {
	let lowercaseFamilyName = familyName.to_lowercase();
	if isBold {
		format!("{}|bold", lowercaseFamilyName)
	}
	else {
		lowercaseFamilyName
	}
}

//==============================================================================
// 폰트 읽기.
//==============================================================================
fn loadFont(fontBytes: Vec<u8>) -> Result<fontdue::Font, String> {
	let fontBytes = decompressWebFont(fontBytes)?;
	fontdue::Font::from_bytes(fontBytes, fontdue::FontSettings::default()).map_err(|error| format!("폰트 읽기 실패: {}", error))
}

//==============================================================================
// 웹 글꼴이면 TTF 로 풀기. (머리 4 바이트가 "wOFF" 면 WOFF, "wOF2" 면 WOFF2. 아니면 그대로)
// - fontdue 는 ttf, ttc, otf 만 읽는다. 원본 기본 글꼴 DOSGothic 이 .woff 라서 푼다.
//==============================================================================
fn decompressWebFont(fontBytes: Vec<u8>) -> Result<Vec<u8>, String> {
	let signature = fontBytes.get(0..4).unwrap_or(&[]);
	if signature == b"wOFF" {
		return wuff::decompress_woff1(&fontBytes).map_err(|error| format!("WOFF 풀기 실패: {:?}", error));
	}
	if signature == b"wOF2" {
		return wuff::decompress_woff2(&fontBytes).map_err(|error| format!("WOFF2 풀기 실패: {:?}", error));
	}
	Ok(fontBytes)
}

//==============================================================================
// 줄 높이 값. (위로 올라간 길이, 아래로 내려간 길이. 둘 다 양수)
//==============================================================================
fn readLineMetrics(font: &fontdue::Font, pixelSize: f32) -> (f32, f32) {
	match font.horizontal_line_metrics(pixelSize) {
		Some(lineMetrics) => {
			(lineMetrics.ascent, -lineMetrics.descent)
		}
		None => {
			(pixelSize * 0.8, pixelSize * 0.2)
		}
	}
}

//==============================================================================
// 글자 배치. (글자마다 그 글자를 가진 첫 폰트를 고른다)
//==============================================================================
fn layoutGlyphs(fontList: &[Rc<fontdue::Font>], text: &str, pixelSize: f32) -> Vec<GlyphPlacement> {
	let mut glyphPlacements: Vec<GlyphPlacement> = Vec::new();
	let mut penX: f32 = 0.0;
	let mut previousGlyph: Option<(usize, u16)> = None;
	for character in text.chars() {
		let mut resolvedFontIndex = 0;
		let mut resolvedGlyphIndex = fontList[0].lookup_glyph_index(character);
		if resolvedGlyphIndex == 0 {
			for (fontIndex, font) in fontList.iter().enumerate().skip(1) {
				let glyphIndex = font.lookup_glyph_index(character);
				if glyphIndex != 0 {
					resolvedFontIndex = fontIndex;
					resolvedGlyphIndex = glyphIndex;
					break;
				}
			}
		}
		let font = &fontList[resolvedFontIndex];
		if let Some((previousFontIndex, previousGlyphIndex)) = previousGlyph {
			if previousFontIndex == resolvedFontIndex {
				if let Some(kerning) = font.horizontal_kern_indexed(previousGlyphIndex, resolvedGlyphIndex, pixelSize) {
					penX += kerning;
				}
			}
		}
		let metrics = font.metrics_indexed(resolvedGlyphIndex, pixelSize);
		glyphPlacements.push(GlyphPlacement {
			fontIndex: resolvedFontIndex,
			glyphIndex: resolvedGlyphIndex,
			penX,
			metrics,
		});
		penX += metrics.advance_width;
		previousGlyph = Some((resolvedFontIndex, resolvedGlyphIndex));
	}
	glyphPlacements
}

//==============================================================================
// 배치된 글자들의 전체 전진 폭.
//==============================================================================
fn calculateAdvanceWidth(_fontList: &[Rc<fontdue::Font>], glyphPlacements: &[GlyphPlacement], _pixelSize: f32) -> f32 {
	match glyphPlacements.last() {
		Some(lastGlyphPlacement) => {
			lastGlyphPlacement.penX + lastGlyphPlacement.metrics.advance_width
		}
		None => {
			0.0
		}
	}
}

//==============================================================================
// 외곽선 띠 만들기. (원 모양으로 넓힌 값 - 원 모양으로 좁힌 값)
//==============================================================================
fn buildStrokeCoverage(coverage: &[u8], bitmapWidth: i32, bitmapHeight: i32, strokeRadius: f32) -> Vec<u8> {
	let radiusCeiling = strokeRadius.ceil() as i32;
	let mut offsetList: Vec<(i32, i32)> = Vec::new();
	for offsetY in -radiusCeiling..=radiusCeiling {
		for offsetX in -radiusCeiling..=radiusCeiling {
			let distance = ((offsetX * offsetX + offsetY * offsetY) as f32).sqrt();
			if distance <= strokeRadius + 0.5 {
				offsetList.push((offsetX, offsetY));
			}
		}
	}

	let readCoverage = |positionX: i32, positionY: i32| -> u8 {
		if positionX < 0 || positionY < 0 || positionX >= bitmapWidth || positionY >= bitmapHeight {
			return 0;
		}
		coverage[(positionY * bitmapWidth + positionX) as usize]
	};

	let mut strokeCoverage = vec![0u8; coverage.len()];
	for positionY in 0..bitmapHeight {
		for positionX in 0..bitmapWidth {
			let mut dilatedValue: u8 = 0;
			let mut erodedValue: u8 = 255;
			for (offsetX, offsetY) in &offsetList {
				let value = readCoverage(positionX + offsetX, positionY + offsetY);
				dilatedValue = dilatedValue.max(value);
				erodedValue = erodedValue.min(value);
			}
			strokeCoverage[(positionY * bitmapWidth + positionX) as usize] = dilatedValue.saturating_sub(erodedValue);
		}
	}
	strokeCoverage
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// WOFF 글꼴이 풀려서 읽히는지. (VANILLA_TEST_WOFF_PATH 에 .woff 경로를 줄 때만 본다)
	//==============================================================================
	#[test]
	fn loadWoffFont() {
		let fontPath = match std::env::var("VANILLA_TEST_WOFF_PATH") {
			Ok(fontPath) => {
				fontPath
			}
			Err(_) => {
				return;
			}
		};
		let fontBytes = std::fs::read(&fontPath).expect("woff file");
		let font = loadFont(fontBytes).expect("woff font");
		let glyphCount = font.glyph_count();
		let hangulGlyphIndex = font.lookup_glyph_index('가');
		assert!(glyphCount > 0);
		assert!(hangulGlyphIndex != 0);
	}

	//==============================================================================
	// 웹 글꼴이 아닌 바이트는 그대로 둔다.
	//==============================================================================
	#[test]
	fn keepPlainFontBytes() {
		let plainBytes: Vec<u8> = vec![0, 1, 0, 0, 9, 9];
		let resultBytes = decompressWebFont(plainBytes.clone()).expect("plain bytes");
		assert_eq!(resultBytes, plainBytes);
	}
}
