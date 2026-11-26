//==============================================================================
//! 다이나믹 폰트. (원본 misc/dynamicfont.js 의 DynamicFont, GlyphInfo)
//! - 정한 글자들을 한 글꼴, 한 크기로 글자 아틀라스(그림 한 장)에 모아 굽고, 글을 그릴 때 아틀라스에서 글리프를 잘라 그린다.
//!   글자마다 같은 그림을 쓰므로 Graphic 이 한 번의 묶음으로 그린다. (문자열마다 텍스처를 굽는 Text 와 달리 묶어 그리기가 끊기지 않는다)
//!   점수, 피해 숫자, 시계처럼 글자가 자주 바뀌고 같은 글자를 많이 그리는 곳에 쓴다.
//! - 아틀라스는 가로 1024 픽셀 안에 줄 단위로 놓고, 글리프 사이에 2 픽셀 여백을 둔다. (원본과 같음)
//! - 글자는 글자 묶음(받침, 결합 부호, 이모지 묶음) 단위로 센다. 아틀라스에 없는 글자는 글자 크기의 반만큼 비우고 넘어간다. (원본과 같음)
//! - 글자는 흰색으로, 색 글리프(이모지)는 제 색으로 굽는다. (원본은 흰색으로 fillText 해 브라우저가 이모지를 제 색으로 그린다)
//! - 원본과 다른 것:
//!   원본 setFontAsset(FontAsset) 은 set_font_family(올린 글꼴 이름) 다. (FontAsset.get_family 를 넘긴다)
//!   원본 getAtlasCanvas 는 get_atlas_image (`Rc<Image>`) 다. 데스크탑은 글리프를 글자 캐시의 글꼴 얼굴(모양 짓기, 가짜 굵게와 기울임 포함)로 굽고,
//!   글자를 덮는 얼굴이 하나도 없으면 그 글자는 굽지 못한다. (나머지 글자는 굽는다) 웹은 원본처럼 브라우저 Canvas2D 로 굽는다.
//!   build_atlas 와 draw_text 는 Graphic 을 받는다. (글꼴은 Graphic 의 글자 캐시에 있다)
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::richtext::build_font_string;
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::core::textshaping::GlyphRaster;
use crate::core::textstringtexturecache::compose_text_pixels;
use std::collections::HashMap;
use std::rc::Rc;
use unicode_segmentation::UnicodeSegmentation;

/// 아틀라스 최대 가로 크기. (픽셀, 원본과 같음)
const ATLAS_MAXIMUM_WIDTH: i32 = 1024;
/// 글리프 사이 여백. (픽셀, 원본과 같음)
const ATLAS_PADDING: i32 = 2;

//==============================================================================
/// 글리프 정보. (원본 GlyphInfo)
/// - DynamicFont 가 아틀라스를 구울 때 글자마다 하나씩 만든다. get_glyph 로 읽는다.
///
/// # 예제
/// 글리프 하나의 그릴 자리 셈
/// ```rust
/// use vanilla::{GlyphInfo, Rect, Vector2};
///
/// let glyph_info = GlyphInfo {
/// 	character: String::from("7"),
/// 	uv_rect: Rect::create(2.0, 2.0, 18.0, 24.0),
/// 	advance: 20.0,
/// 	bearing_x: -1.0,
/// 	bearing_y: 23.0,
/// };
/// let pen_position = Vector2::create(100.0, 50.0);
/// let draw_left = pen_position.x - glyph_info.bearing_x;
/// let draw_top = pen_position.y - glyph_info.bearing_y;
/// assert_eq!((draw_left, draw_top), (101.0, 27.0));
/// assert_eq!(pen_position.x + glyph_info.advance, 120.0);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphInfo {
	/// 글자. (글자 묶음 하나)
	pub character: String,
	/// 아틀라스 안의 자리와 크기. (픽셀)
	pub uv_rect: Rect,
	/// 가로로 나아가는 양. (전진폭 + 자간, 픽셀)
	pub advance: f64,
	/// 펜 자리에서 글리프 왼쪽 픽셀까지 거리. (왼쪽이 양수)
	pub bearing_x: f64,
	/// 기준선에서 글리프 위쪽 픽셀까지 거리. (위가 양수)
	pub bearing_y: f64,
}

//==============================================================================
/// 아틀라스에 놓을 글리프 하나. (굽는 동안만)
//==============================================================================
struct AtlasGlyph {
	character: String,
	coverage: Vec<u8>,
	/// 색 글리프 몫. (프리멀티플라이드 RGBA, 없으면 None)
	color_pixels: Option<Vec<u8>>,
	glyph_width: i32,
	glyph_height: i32,
	advance_width: f64,
	bearing_x: f64,
	bearing_y: f64,
	atlas_x: i32,
	atlas_y: i32,
}

//==============================================================================
/// 다이나믹 폰트.
/// - 쓸 글자를 build_atlas 로 한 번 굽고, 매 프레임 draw_text 로 그린다. 둘 다 Graphic 이 있어야 한다.
///
/// # 예제
/// 점수 숫자를 굽고 가운데에 그리기
/// ```rust,no_run
/// use vanilla::{DynamicFont, Graphic, Vector2};
///
/// fn draw_score(dynamic_font: &mut DynamicFont, graphic: &mut Graphic, score: u32) {
/// 	dynamic_font.build_atlas(graphic, "0123456789");
/// 	let score_text = score.to_string();
/// 	let text_width = dynamic_font.measure_text(&score_text);
/// 	dynamic_font.draw_text(&score_text, Vector2::create(640.0 - text_width * 0.5, 80.0), graphic);
/// }
///
/// let mut dynamic_font = DynamicFont::new();
/// dynamic_font.set_font_size(48.0);
/// dynamic_font.set_letter_spacing(2.0);
/// ```
//==============================================================================
pub struct DynamicFont {
	font_family: String,
	font_size: f64,
	letter_spacing: f64,
	glyph_table: HashMap<String, GlyphInfo>,
	/// 아틀라스에 든 글자. (넣은 차례, 다시 구울 때 모두 놓는다)
	character_list: Vec<String>,
	atlas_image: Option<Rc<Image>>,
}

impl DynamicFont {
	//==============================================================================
	/// 생성. (글자 크기 32, 자간 0, 글꼴은 기본 글꼴. 원본과 같음)
	///
	/// # 돌려줌
	/// 아틀라스가 없는 새 다이나믹 폰트. (글꼴 이름 "default")
	//==============================================================================
	pub fn new() -> DynamicFont {
		DynamicFont {
			font_family: String::from("default"),
			font_size: 32.0,
			letter_spacing: 0.0,
			glyph_table: HashMap::new(),
			character_list: Vec::new(),
			atlas_image: None,
		}
	}

	//==============================================================================
	/// 글꼴 이름 설정. (원본 setFontAsset, FontAsset 이면 get_family 를 넘긴다)
	/// - 이미 구운 아틀라스는 바꾸지 않는다. 바꾼 뒤 rebuild_atlas 를 부른다.
	///
	/// # 인자
	/// - `font_family`: Graphic 의 글자 캐시에 올린 글꼴 이름. ("default" 는 기본 글꼴)
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		self.font_family = font_family.to_string();
	}

	//==============================================================================
	/// 글꼴 이름 반환.
	///
	/// # 돌려줌
	/// 지금 글꼴 이름.
	//==============================================================================
	pub fn get_font_family(&self) -> &str {
		&self.font_family
	}

	//==============================================================================
	/// 글자 크기 설정. (픽셀, 다음 build_atlas 부터)
	///
	/// # 인자
	/// - `font_size`: 글자 크기. (픽셀, 아틀라스에 없는 글자는 이것의 반만큼 비운다)
	//==============================================================================
	pub fn set_font_size(&mut self, font_size: f64) {
		self.font_size = font_size;
	}

	//==============================================================================
	/// 글자 크기 반환.
	///
	/// # 돌려줌
	/// 지금 글자 크기. (픽셀)
	//==============================================================================
	pub fn get_font_size(&self) -> f64 {
		self.font_size
	}

	//==============================================================================
	/// 자간 설정. (픽셀, 다음 build_atlas 부터)
	///
	/// # 인자
	/// - `letter_spacing`: 글자마다 전진폭에 더할 양. (픽셀)
	//==============================================================================
	pub fn set_letter_spacing(&mut self, letter_spacing: f64) {
		self.letter_spacing = letter_spacing;
	}

	//==============================================================================
	/// 자간 반환.
	///
	/// # 돌려줌
	/// 지금 자간. (픽셀)
	//==============================================================================
	pub fn get_letter_spacing(&self) -> f64 {
		self.letter_spacing
	}

	//==============================================================================
	/// 아틀라스 그림 반환. (아직 굽지 않았으면 None, 원본 getAtlasCanvas)
	///
	/// # 돌려줌
	/// 가로 1024 픽셀의 아틀라스 그림. 아직 굽지 않았거나 clear_atlas 뒤면 None.
	//==============================================================================
	pub fn get_atlas_image(&self) -> Option<&Rc<Image>> {
		self.atlas_image.as_ref()
	}

	//==============================================================================
	/// 글리프 정보 반환. (아틀라스에 없으면 None)
	///
	/// # 인자
	/// - `character`: 글자 묶음 하나.
	///
	/// # 돌려줌
	/// 그 글자의 글리프 정보. 아틀라스에 없으면 None.
	//==============================================================================
	pub fn get_glyph(&self, character: &str) -> Option<&GlyphInfo> {
		self.glyph_table.get(character)
	}

	//==============================================================================
	/// 아틀라스 굽기. (charset 의 글자 가운데 아직 없는 글자를 더하고, 아틀라스를 모든 글자로 다시 굽는다)
	/// - 든 글자를 모두 다시 놓는다. (원본도 같다, 새 글자만 새 캔버스에 놓아 앞서 구운 글자가 어긋나던 것은 원본에서도 고쳤다)
	/// - 데스크탑에서 글자를 덮는 얼굴이 하나도 없으면 그 글자는 건너뛰고, 하나도 굽지 못하면 그대로 둔다. (웹은 브라우저 Canvas2D 로 굽는다)
	///
	/// # 인자
	/// - `graphic`: 글자 캐시를 쓸 그래픽.
	/// - `charset`: 구울 글자들. (글자 묶음 단위로 나누고, 이미 든 글자는 건너뛴다)
	//==============================================================================
	pub fn build_atlas(&mut self, graphic: &mut Graphic, charset: &str) {
		let mut is_changed = false;
		for character in charset.graphemes(true) {
			if self.character_list.iter().any(|existing_character| existing_character == character) {
				continue;
			}
			self.character_list.push(character.to_string());
			is_changed = true;
		}
		if !is_changed && self.atlas_image.is_some() {
			return;
		}
		self.rebuild_atlas(graphic);
	}

	//==============================================================================
	/// 든 글자를 모두 다시 굽기. (글꼴, 크기, 자간을 바꾼 뒤 부른다)
	/// - 굽지 못하는 글자는 건너뛰고 나머지를 굽는다. (건너뛴 글자는 아틀라스에 없는 글자처럼 그린다)
	/// - 하나도 굽지 못하면 아틀라스를 그대로 둔다.
	///
	/// # 인자
	/// - `graphic`: 글자 캐시를 쓸 그래픽.
	//==============================================================================
	pub fn rebuild_atlas(&mut self, graphic: &mut Graphic) {
		let font_string = build_font_string(&self.font_family, self.font_size, false, false);
		let text_string_texture_cache = graphic.get_text_string_texture_cache();
		self.rebuild_atlas_with(|character| text_string_texture_cache.rasterize_character(character, &font_string).map(|(raster, advance, ascent, descent)| (raster, advance as f64, ascent as f64, descent as f64)));
	}

	//==============================================================================
	/// 글자 래스터 함수로 든 글자를 모두 다시 굽기. (rebuild_atlas 의 몸통)
	///
	/// # 인자
	/// - `rasterize_character`: 글자 → (래스터, 전진폭, 글꼴 위 길이, 글꼴 아래 길이). 굽지 못하면 None.
	//==============================================================================
	fn rebuild_atlas_with(&mut self, rasterize_character: impl Fn(&str) -> Option<(GlyphRaster, f64, f64, f64)>) {
		let mut atlas_glyph_list: Vec<AtlasGlyph> = Vec::new();
		for character in &self.character_list {
			let (glyph_raster, advance_width, _, _) = match rasterize_character(character) {
				Some(rasterized) => {
					rasterized
				}
				None => {
					continue;
				}
			};
			atlas_glyph_list.push(AtlasGlyph {
				character: character.clone(),
				coverage: glyph_raster.coverage,
				color_pixels: glyph_raster.color_pixels,
				glyph_width: glyph_raster.width,
				glyph_height: glyph_raster.height,
				advance_width,
				bearing_x: -glyph_raster.left_x as f64,
				bearing_y: glyph_raster.top_y as f64,
				atlas_x: 0,
				atlas_y: 0,
			});
		}
		if atlas_glyph_list.is_empty() && !self.character_list.is_empty() {
			return;
		}

		// 줄 단위로 놓기. (원본과 같은 차례)
		let mut cursor_x = 0;
		let mut cursor_y = 0;
		let mut row_height = 0;
		for atlas_glyph in atlas_glyph_list.iter_mut() {
			let slot_width = atlas_glyph.glyph_width + ATLAS_PADDING * 2;
			let slot_height = atlas_glyph.glyph_height + ATLAS_PADDING * 2;
			if cursor_x + slot_width > ATLAS_MAXIMUM_WIDTH {
				cursor_x = 0;
				cursor_y += row_height;
				row_height = 0;
			}
			atlas_glyph.atlas_x = cursor_x;
			atlas_glyph.atlas_y = cursor_y;
			cursor_x += slot_width;
			row_height = row_height.max(slot_height);
		}
		let atlas_height = (cursor_y + row_height).max(1);

		// 흰색 글자로 굽기. (알파가 덮인 정도, 색 글리프는 흰 글자 위에 얹은 곧은 색)
		let mut pixels = vec![0u8; (ATLAS_MAXIMUM_WIDTH * atlas_height * 4) as usize];
		for pixel in pixels.chunks_exact_mut(4) {
			pixel[0] = 255;
			pixel[1] = 255;
			pixel[2] = 255;
		}
		self.glyph_table.clear();
		for atlas_glyph in &atlas_glyph_list {
			let left_x = atlas_glyph.atlas_x + ATLAS_PADDING;
			let top_y = atlas_glyph.atlas_y + ATLAS_PADDING;
			let composed_pixels = atlas_glyph.color_pixels.as_ref().map(|color_pixels| compose_text_pixels(&atlas_glyph.coverage, Some(color_pixels)));
			for glyph_y in 0..atlas_glyph.glyph_height {
				for glyph_x in 0..atlas_glyph.glyph_width {
					let glyph_index = (glyph_y * atlas_glyph.glyph_width + glyph_x) as usize;
					let pixel_index = (((top_y + glyph_y) * ATLAS_MAXIMUM_WIDTH + left_x + glyph_x) * 4) as usize;
					match composed_pixels.as_ref() {
						Some(composed_pixels) => {
							let alpha = composed_pixels[glyph_index * 4 + 3] as u32;
							if alpha > 0 {
								for channel_index in 0..3 {
									pixels[pixel_index + channel_index] = ((composed_pixels[glyph_index * 4 + channel_index] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
								}
							}
							pixels[pixel_index + 3] = alpha as u8;
						}
						None => {
							pixels[pixel_index + 3] = atlas_glyph.coverage[glyph_index];
						}
					}
				}
			}
			self.glyph_table.insert(atlas_glyph.character.clone(), GlyphInfo {
				character: atlas_glyph.character.clone(),
				uv_rect: Rect::create(left_x as f64, top_y as f64, atlas_glyph.glyph_width as f64, atlas_glyph.glyph_height as f64),
				advance: atlas_glyph.advance_width + self.letter_spacing,
				bearing_x: atlas_glyph.bearing_x,
				bearing_y: atlas_glyph.bearing_y,
			});
		}
		self.atlas_image = Some(Image::create_from_pixels(ATLAS_MAXIMUM_WIDTH as u32, atlas_height as u32, pixels));
	}

	//==============================================================================
	/// 글 그리기. (position 은 첫 글자의 펜 자리, y 는 기준선. 색은 Graphic 의 그림 틴트를 따른다, 원본 drawText)
	/// - 아틀라스가 없으면 아무것도 그리지 않는다.
	///
	/// # 인자
	/// - `text`: 그릴 글. (아틀라스에 없는 글자는 글자 크기의 반만큼 비운다)
	/// - `position`: 첫 글자의 펜 자리. (x 는 펜 자리, y 는 기준선, 지금 좌표계의 픽셀)
	/// - `graphic`: 그릴 그래픽.
	///
	/// # 예제
	/// 노란 피해 숫자 그리기
	/// ```rust,no_run
	/// use vanilla::{Color, DynamicFont, Graphic, Vector2};
	///
	/// fn draw_damage(dynamic_font: &DynamicFont, graphic: &mut Graphic, damage: i32) {
	/// 	graphic.set_image_tint_color(Some(Color::create(1.0, 0.85, 0.2, 1.0)));
	/// 	dynamic_font.draw_text(&damage.to_string(), Vector2::create(300.0, 200.0), graphic);
	/// 	graphic.set_image_tint_color(None);
	/// }
	/// ```
	//==============================================================================
	pub fn draw_text(&self, text: &str, position: Vector2, graphic: &mut Graphic) {
		let atlas_image = match &self.atlas_image {
			Some(atlas_image) => {
				atlas_image.clone()
			}
			None => {
				return;
			}
		};
		let mut cursor_x = position.x;
		let baseline_y = position.y;
		for character in text.graphemes(true) {
			let glyph_info = match self.glyph_table.get(character) {
				Some(glyph_info) => {
					glyph_info
				}
				None => {
					cursor_x += self.font_size * 0.5;
					continue;
				}
			};
			if glyph_info.uv_rect.size.x > 0.0 && glyph_info.uv_rect.size.y > 0.0 {
				let uv_rect = glyph_info.uv_rect;
				graphic.draw_image_with_source_and_destination(&atlas_image, uv_rect.position.x, uv_rect.position.y, uv_rect.size.x, uv_rect.size.y, cursor_x - glyph_info.bearing_x, baseline_y - glyph_info.bearing_y, uv_rect.size.x, uv_rect.size.y);
			}
			cursor_x += glyph_info.advance;
		}
	}

	//==============================================================================
	/// 글 너비 재기. (아틀라스의 전진폭 합, 없는 글자는 글자 크기의 반)
	///
	/// # 인자
	/// - `text`: 잴 글.
	///
	/// # 돌려줌
	/// 글 너비. (픽셀, 자간 포함)
	///
	/// # 예제
	/// 굽기 전에는 글자마다 크기의 반
	/// ```rust
	/// use vanilla::DynamicFont;
	///
	/// let mut dynamic_font = DynamicFont::new();
	/// dynamic_font.set_font_size(40.0);
	/// assert_eq!(dynamic_font.measure_text("123"), 60.0);
	/// assert!(dynamic_font.get_atlas_image().is_none());
	/// ```
	//==============================================================================
	pub fn measure_text(&self, text: &str) -> f64 {
		text.graphemes(true).map(|character| self.glyph_table.get(character).map(|glyph_info| glyph_info.advance).unwrap_or(self.font_size * 0.5)).sum()
	}

	//==============================================================================
	/// 아틀라스 비우기. (원본 clearAtlas)
	//==============================================================================
	pub fn clear_atlas(&mut self) {
		self.glyph_table.clear();
		self.character_list.clear();
		self.atlas_image = None;
	}
}

impl Default for DynamicFont {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> DynamicFont {
		DynamicFont::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 시험용 글자 래스터. (가로 width, 세로 2 를 꽉 채운 글리프, "?" 는 굽지 못한다)
	//==============================================================================
	fn rasterize_test_character(character: &str) -> Option<(GlyphRaster, f64, f64, f64)> {
		if character == "?" {
			return None;
		}
		let width = character.chars().count() as i32 + 1;
		let glyph_raster = GlyphRaster {
			coverage: vec![255; (width * 2) as usize],
			width,
			height: 2,
			left_x: 0,
			top_y: 2,
			color_pixels: None,
		};
		Some((glyph_raster, width as f64, 2.0, 0.0))
	}

	//==============================================================================
	/// 굽지 못하는 글자가 섞여도 나머지 글자는 굽고, 하나도 굽지 못하면 아틀라스를 그대로 둔다.
	//==============================================================================
	#[test]
	fn rebuild_skips_unbakable_character() {
		let mut dynamic_font = DynamicFont::new();
		dynamic_font.character_list = vec![String::from("?")];
		dynamic_font.rebuild_atlas_with(rasterize_test_character);
		assert!(dynamic_font.get_atlas_image().is_none());
		dynamic_font.character_list = vec![String::from("A"), String::from("?"), String::from("B")];
		dynamic_font.rebuild_atlas_with(rasterize_test_character);
		assert!(dynamic_font.get_atlas_image().is_some());
		assert!(dynamic_font.get_glyph("A").is_some());
		assert!(dynamic_font.get_glyph("B").is_some());
		assert!(dynamic_font.get_glyph("?").is_none());
		assert_eq!(dynamic_font.measure_text("A?B"), 2.0 + dynamic_font.get_font_size() * 0.5 + 2.0);
		let atlas_image = dynamic_font.get_atlas_image().expect("atlas").clone();
		dynamic_font.character_list = vec![String::from("?")];
		dynamic_font.rebuild_atlas_with(rasterize_test_character);
		assert!(Rc::ptr_eq(dynamic_font.get_atlas_image().expect("atlas"), &atlas_image));
	}
}
