//==============================================================================
// 그림자를 깐 한 줄 글. (vanilla.js core/component/shadowtext.js 의 ShadowText)
// - 글자 노드 두 벌(같은 글을 검게 밑에, 글을 위에)로 그림자를 깔면 둘이 따로 비쳐, 노드를 흐리게 할 때 흐린 글 사이로 밑의 그림자가 보인다.
//   이 컴포넌트는 그림자 위에 글을 얹은 모습을 그림 한 장에 구워 그 한 장을 그리므로, 노드의 불투명도가 글과 그림자에 한 장째로 먹는다.
// - 가로 정렬은 노드 안에서 왼쪽, 가운데, 오른쪽이고, 세로는 노드의 가운데다.
// - 글이 노드 너비를 넘으면 처음 크기에서 최소 비율(set_fit_minimum_ratio)까지 줄여 맞춘다.
// - 그림자는 상자 흐림 세 번(가우시안에 가까움)으로 번지게 할 수 있다. 번지는 만큼 구운 그림의 가장자리를 넓힌다.
// - 굽는 배율, 글, 꼴, 노드 너비가 바뀌면 다시 굽는다.
// - 화면 밖 거르기는 마지막으로 구운 한 장이 놓이는 사각형(넘친 글, 그림자 여백 포함)으로 판정한다. 세터로 바꾼 뒤 아직 굽지 않았거나 노드 너비나 굽는 배율이 달라졌으면 거르지 않는다.
// - 원본과 다른 것: 웹에서 글꼴 바이트 없이 시스템 글꼴로 굽는 경우는 모양을 받을 수 없어 그림자 없이 Graphic 의 글로 그린다.
//   기울임, 밑줄, 취소선(set_italic, set_underline, set_strikethrough)을 더했다. (원본에는 없다) 이름과 선의 자리, 두께는 Text 의 가운데 기준선과 같고,
//   선도 글과 함께 구워 그림자가 깔린다. 한 줄을 줄여 맞추는 글이라 줄바꿈 방식은 두지 않는다.
//==============================================================================
use std::rc::Rc;

use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::richtext::build_font_string;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::image::Image;
use crate::core::node::worldnode::NodeTransform;
use crate::core::textstringtexturecache::TextRaster;

// 구운 그림의 가장자리 여유. (안티앨리어싱, 픽셀)
const BAKE_PADDING: i32 = 2;

//==============================================================================
// 덮인 정도(0 ~ 255)를 상자 흐림 세 번으로 번지게 하기. (반지름은 픽셀, 0 이하면 그대로)
//==============================================================================
pub fn blur_coverage(coverage: &[u8], width: i32, height: i32, radius: f32) -> Vec<u8> {
	if radius <= 0.0 || width <= 0 || height <= 0 {
		return coverage.to_vec();
	}
	// 가우시안의 표준편차를 상자 셋으로 나눈 반지름. (sigma ≈ radius / 2, 상자 너비 = sqrt(12 sigma² / 3 + 1))
	let sigma = radius * 0.5;
	let box_radius = (((12.0 * sigma * sigma / 3.0 + 1.0).sqrt() - 1.0) * 0.5).round().max(1.0) as i32;
	let mut values: Vec<f32> = coverage.iter().map(|value| *value as f32).collect();
	let mut scratch = vec![0.0f32; values.len()];
	for _ in 0..3 {
		blur_axis(&values, &mut scratch, width, height, box_radius, true);
		blur_axis(&scratch, &mut values, width, height, box_radius, false);
	}
	values.iter().map(|value| value.round().clamp(0.0, 255.0) as u8).collect()
}

//==============================================================================
// 한 축으로 상자 흐림. (밖은 0)
//==============================================================================
fn blur_axis(source: &[f32], target: &mut [f32], width: i32, height: i32, box_radius: i32, is_horizontal: bool) {
	let (line_count, line_length) = if is_horizontal { (height, width) } else { (width, height) };
	let window = (box_radius * 2 + 1) as f32;
	for line_index in 0..line_count {
		let read_index = |position: i32| -> usize {
			if is_horizontal { (line_index * width + position) as usize } else { (position * width + line_index) as usize }
		};
		let mut sum = 0.0f32;
		for position in -box_radius..=box_radius {
			if position >= 0 && position < line_length {
				sum += source[read_index(position)];
			}
		}
		for position in 0..line_length {
			target[read_index(position)] = sum / window;
			let leaving = position - box_radius;
			let entering = position + box_radius + 1;
			if leaving >= 0 {
				sum -= source[read_index(leaving)];
			}
			if entering < line_length {
				sum += source[read_index(entering)];
			}
		}
	}
}

//==============================================================================
// 그림자 위에 모양을 얹은 RGBA 그림 짓기. (모양의 덮인 정도와 색, 그림자의 덮인 정도와 색과 아래로 내림, 곧은 알파)
// - 그림자는 shape 를 흐려 shadow_offset_y 만큼 내려 깔고, 그 위에 모양을 알파로 얹는다.
// - shape_pixels 를 주면 모양의 색을 픽셀마다 쓴다. (그림, None 이면 shape_color 한 빛)
//==============================================================================
pub fn compose_shadow_image(shape_coverage: &[u8], shape_pixels: Option<&[u8]>, width: i32, height: i32, shape_color: Color, shadow_color: Color, shadow_blur: f32, shadow_offset_y: i32) -> Rc<Image> {
	let shadow_coverage = blur_coverage(shape_coverage, width, height, shadow_blur);
	let mut pixels = vec![0u8; (width * height * 4) as usize];
	for y in 0..height {
		for x in 0..width {
			let pixel_index = (y * width + x) as usize;
			let shadow_y = y - shadow_offset_y;
			let shadow_alpha = if shadow_y >= 0 && shadow_y < height { shadow_coverage[(shadow_y * width + x) as usize] as f32 / 255.0 * shadow_color.alpha } else { 0.0 };
			let (shape_red, shape_green, shape_blue, shape_alpha) = match shape_pixels {
				Some(shape_pixels) => {
					let offset = pixel_index * 4;
					(shape_pixels[offset] as f32 / 255.0, shape_pixels[offset + 1] as f32 / 255.0, shape_pixels[offset + 2] as f32 / 255.0, shape_pixels[offset + 3] as f32 / 255.0)
				}
				None => {
					(shape_color.red, shape_color.green, shape_color.blue, shape_coverage[pixel_index] as f32 / 255.0 * shape_color.alpha)
				}
			};
			// 그림자 위에 모양을 얹는다. (곧은 알파의 over)
			let alpha = shape_alpha + shadow_alpha * (1.0 - shape_alpha);
			let offset = pixel_index * 4;
			if alpha > 0.0 {
				let mix_channel = |shape_value: f32, shadow_value: f32| ((shape_value * shape_alpha + shadow_value * shadow_alpha * (1.0 - shape_alpha)) / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
				pixels[offset] = mix_channel(shape_red, shadow_color.red);
				pixels[offset + 1] = mix_channel(shape_green, shadow_color.green);
				pixels[offset + 2] = mix_channel(shape_blue, shadow_color.blue);
				pixels[offset + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
			}
		}
	}
	Image::create_from_pixels(width as u32, height as u32, pixels)
}

//==============================================================================
// 구운 한 장. (그림, 논리 좌표의 펜 자리와 기준선, 전진폭, 위아래, 배율, 구울 때의 노드 너비)
//==============================================================================
struct BakedText {
	image: Rc<Image>,
	scale: f32,
	content_width: f32,
	pen_offset_x: f32,
	baseline_offset_y: f32,
	advance_width: f32,
	font_ascent: f32,
	font_descent: f32,
}

pub struct ShadowText {
	component_base: ComponentBase,
	text: String,
	font_family: String,
	font_size: f32,
	is_bold: bool,
	is_italic: bool,
	is_underline: bool,
	is_strikethrough: bool,
	text_align: TextAlign,
	text_color: Color,
	shadow_color: Color,
	shadow_offset: f32,
	shadow_blur: f32,
	fit_minimum_ratio: f32,
	baked_text: Option<BakedText>,
	bake_key: String,
	// 세터로 바꾼 뒤 아직 굽지 않았는지. (화면 밖 거르기가 지난 한 장을 믿을지 가린다)
	is_bake_dirty: bool,
}

impl ShadowText {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ShadowText {
		ShadowText {
			component_base: ComponentBase::new(),
			text: String::new(),
			font_family: String::from("default"),
			font_size: 16.0,
			is_bold: false,
			is_italic: false,
			is_underline: false,
			is_strikethrough: false,
			text_align: TextAlign::Center,
			text_color: Color::white(),
			shadow_color: Color::black(),
			shadow_offset: 0.0,
			shadow_blur: 0.0,
			fit_minimum_ratio: 0.6,
			baked_text: None,
			bake_key: String::new(),
			is_bake_dirty: true,
		}
	}

	//==============================================================================
	// 글로 생성.
	//==============================================================================
	pub fn create_with_text(text: &str, font_size: f32) -> ShadowText {
		let mut shadow_text = ShadowText::new();
		shadow_text.set_text(text);
		shadow_text.font_size = font_size;
		shadow_text
	}

	//==============================================================================
	// Canvas2D font 꼴 문자열. (Text 와 같은 꼴)
	//==============================================================================
	fn build_font_string(&self, font_size: f32) -> String {
		build_font_string(&self.font_family, font_size, self.is_bold, self.is_italic)
	}

	//==============================================================================
	// 밑줄 / 취소선 두께. (Text 와 같이 max(1, 글자 크기 / 16))
	//==============================================================================
	fn calculate_decoration_line_width(font_size: f32) -> f32 {
		(font_size / 16.0).max(1.0)
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	pub fn draw_shadow_text(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.text.is_empty() {
			return;
		}
		let content_size = transform.get_content_size();
		let bake_scale = graphic.calculate_text_bake_scale();
		let bake_key = format!("{}|{}|{}|{}|{}|{}|{}|{:?}|{:?}|{}|{}|{}|{}|{}", self.text, self.font_family, self.font_size, self.is_bold, self.is_italic, self.is_underline, self.is_strikethrough, self.text_color, self.shadow_color, self.shadow_offset, self.shadow_blur, self.fit_minimum_ratio, content_size.x, bake_scale);
		if bake_key != self.bake_key {
			self.bake_key = bake_key;
			self.baked_text = self.bake(graphic, content_size.x, bake_scale);
		}
		self.is_bake_dirty = false;
		let baked_text = match &self.baked_text {
			Some(baked_text) => {
				baked_text
			}
			None => {
				// 글꼴 모양을 받을 수 없으면 그림자 없이 엔진의 글로.
				let text_align = self.text_align;
				let anchor_x = match graphic.resolve_text_align(text_align) {
					TextAlign::Left => {
						0.0
					}
					TextAlign::Right => {
						content_size.x
					}
					_ => {
						content_size.x * 0.5
					}
				};
				let font_string = self.build_font_string(self.font_size);
				graphic.set_font_string(&font_string);
				graphic.set_text_align(text_align);
				graphic.set_text_baseline(TextBaseline::Middle);
				graphic.set_fill_color(self.text_color);
				graphic.draw_fill_text(&self.text, anchor_x, content_size.y * 0.5);
				if self.is_underline || self.is_strikethrough {
					let text_width = graphic.measure_text(&self.text).width;
					let start_x = match graphic.resolve_text_align(text_align) {
						TextAlign::Left => {
							anchor_x
						}
						TextAlign::Right => {
							anchor_x - text_width
						}
						_ => {
							anchor_x - text_width * 0.5
						}
					};
					let line_width = ShadowText::calculate_decoration_line_width(self.font_size);
					let middle_y = content_size.y * 0.5;
					graphic.set_stroke_color(self.text_color);
					if self.is_underline {
						let underline_y = middle_y + self.font_size * 0.5 + 2.0;
						graphic.draw_line(&[Vector2::create(start_x, underline_y), Vector2::create(start_x + text_width, underline_y)], line_width);
					}
					if self.is_strikethrough {
						graphic.draw_line(&[Vector2::create(start_x, middle_y), Vector2::create(start_x + text_width, middle_y)], line_width);
					}
				}
				return;
			}
		};

		let image_width = baked_text.image.get_width() as f32;
		let image_height = baked_text.image.get_height() as f32;
		let image = baked_text.image.clone();
		let quad_rect = self.calculate_quad_rect(graphic, baked_text, content_size.x, content_size.y);
		graphic.draw_image_with_source_and_destination(&image, 0.0, 0.0, image_width, image_height, quad_rect.position.x, quad_rect.position.y, quad_rect.size.x, quad_rect.size.y);
	}

	//==============================================================================
	// 구운 한 장이 놓이는 사각형. (가로는 정렬대로, 세로는 가운데(middle 기준선)에 놓는다)
	//==============================================================================
	fn calculate_quad_rect(&self, graphic: &Graphic, baked_text: &BakedText, content_width: f32, content_height: f32) -> Rect {
		let (anchor_x, align_offset) = match graphic.resolve_text_align(self.text_align) {
			TextAlign::Left => {
				(0.0, 0.0)
			}
			TextAlign::Right => {
				(content_width, -baked_text.advance_width)
			}
			_ => {
				(content_width * 0.5, -baked_text.advance_width * 0.5)
			}
		};
		let quad_x = anchor_x + align_offset - baked_text.pen_offset_x;
		let quad_y = content_height * 0.5 + (baked_text.font_ascent - baked_text.font_descent) * 0.5 - baked_text.baseline_offset_y;
		let image_width = baked_text.image.get_width() as f32;
		let image_height = baked_text.image.get_height() as f32;
		Rect::create(quad_x, quad_y, image_width / baked_text.scale, image_height / baked_text.scale)
	}

	//==============================================================================
	// 굽기. (넘치면 줄이고, 모양을 받아 그림자를 깐 한 장으로)
	//==============================================================================
	fn bake(&self, graphic: &mut Graphic, fit_width: f32, bake_scale: f32) -> Option<BakedText> {
		let full_font_string = self.build_font_string(self.font_size);
		let text_width = graphic.get_text_string_texture_cache().measure_text(&full_font_string, &self.text).width;
		let mut font_size = self.font_size;
		if fit_width > 0.0 && text_width > fit_width {
			let fit_ratio = (fit_width / text_width).max(self.fit_minimum_ratio);
			font_size = (self.font_size * fit_ratio * 10.0).floor() / 10.0;
		}
		let font_string = self.build_font_string(font_size);
		let scaled_blur = self.shadow_blur * bake_scale;
		let scaled_offset = (self.shadow_offset * bake_scale).round() as i32;
		let mut padding = BAKE_PADDING + scaled_blur.ceil() as i32 * 2 + scaled_offset.max(0);
		if self.is_underline {
			// 밑줄은 기준선 아래로 내려가므로 여유를 더 둔다.
			padding += ((font_size * 0.25 + 2.0) * bake_scale).ceil() as i32;
		}
		let mut text_raster = graphic.get_text_string_texture_cache().rasterize_text(&self.text, &font_string, bake_scale, padding)?;
		self.draw_decoration_lines(&mut text_raster, font_size, bake_scale);
		let image = compose_shadow_image(&text_raster.coverage, None, text_raster.width, text_raster.height, self.text_color, self.shadow_color, scaled_blur, scaled_offset);
		Some(BakedText {
			image,
			scale: bake_scale,
			content_width: fit_width,
			pen_offset_x: text_raster.origin_x / bake_scale,
			baseline_offset_y: text_raster.baseline_y / bake_scale,
			advance_width: text_raster.advance_width / bake_scale,
			font_ascent: text_raster.font_ascent / bake_scale,
			font_descent: text_raster.font_descent / bake_scale,
		})
	}

	//==============================================================================
	// 밑줄 / 취소선을 글 모양 래스터에 긋기. (가운데 기준선 자리에서 Text 와 같은 오프셋, 길이는 전진폭)
	//==============================================================================
	fn draw_decoration_lines(&self, text_raster: &mut TextRaster, font_size: f32, bake_scale: f32) {
		if !self.is_underline && !self.is_strikethrough {
			return;
		}
		let middle_y = text_raster.baseline_y - (text_raster.font_ascent - text_raster.font_descent) * 0.5;
		let line_width = ShadowText::calculate_decoration_line_width(font_size) * bake_scale;
		let mut center_y_list: Vec<f32> = Vec::new();
		if self.is_underline {
			center_y_list.push(middle_y + (font_size * 0.5 + 2.0) * bake_scale);
		}
		if self.is_strikethrough {
			center_y_list.push(middle_y);
		}
		let start_x = text_raster.origin_x.floor().max(0.0) as i32;
		let end_x = ((text_raster.origin_x + text_raster.advance_width).ceil() as i32).min(text_raster.width);
		for center_y in center_y_list {
			let start_y = (center_y - line_width * 0.5).round().max(0.0) as i32;
			let end_y = ((center_y + line_width * 0.5).round() as i32).max(start_y + 1).min(text_raster.height);
			for position_y in start_y..end_y {
				for position_x in start_x..end_x {
					text_raster.coverage[(position_y * text_raster.width + position_x) as usize] = 255;
				}
			}
		}
	}

	//==============================================================================
	// 글 설정.
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		self.text = String::from(text);
		self.is_bake_dirty = true;
	}

	pub fn get_text(&self) -> &str {
		&self.text
	}

	//==============================================================================
	// 글꼴 이름 설정. (엔진에 올린 글꼴 이름, 처음은 default)
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		self.font_family = String::from(font_family);
		self.is_bake_dirty = true;
	}

	//==============================================================================
	// 글의 꼴 설정. (처음 크기, 굵기, 색)
	//==============================================================================
	pub fn set_text_style(&mut self, font_size: f32, is_bold: bool, text_color: impl Into<Color>) {
		self.font_size = font_size;
		self.is_bold = is_bold;
		self.text_color = text_color.into();
		self.is_bake_dirty = true;
	}

	//==============================================================================
	// 기울임 설정.
	//==============================================================================
	pub fn set_italic(&mut self, is_italic: bool) {
		self.is_italic = is_italic;
		self.is_bake_dirty = true;
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn is_italic(&self) -> bool {
		self.is_italic
	}

	//==============================================================================
	// 밑줄 설정.
	//==============================================================================
	pub fn set_underline(&mut self, is_underline: bool) {
		self.is_underline = is_underline;
		self.is_bake_dirty = true;
	}

	//==============================================================================
	// 밑줄 여부.
	//==============================================================================
	pub fn is_underline(&self) -> bool {
		self.is_underline
	}

	//==============================================================================
	// 취소선 설정.
	//==============================================================================
	pub fn set_strikethrough(&mut self, is_strikethrough: bool) {
		self.is_strikethrough = is_strikethrough;
		self.is_bake_dirty = true;
	}

	//==============================================================================
	// 취소선 여부.
	//==============================================================================
	pub fn is_strikethrough(&self) -> bool {
		self.is_strikethrough
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		self.text_align = text_align;
	}

	//==============================================================================
	// 그림자 설정. (색과 불투명도는 색의 알파로, 아래로 내려가는 만큼, 번지는 만큼, 둘 다 내용 크기 단위)
	//==============================================================================
	pub fn set_shadow(&mut self, shadow_color: impl Into<Color>, shadow_offset: f32, shadow_blur: f32) {
		self.shadow_color = shadow_color.into();
		self.shadow_offset = shadow_offset;
		self.shadow_blur = shadow_blur.max(0.0);
		self.is_bake_dirty = true;
	}

	//==============================================================================
	// 넘칠 때 줄이는 가장 작은 비율 설정. (1 이면 줄이지 않는다)
	//==============================================================================
	pub fn set_fit_minimum_ratio(&mut self, fit_minimum_ratio: f32) {
		self.fit_minimum_ratio = fit_minimum_ratio.clamp(0.01, 1.0);
		self.is_bake_dirty = true;
	}
}

impl Default for ShadowText {
	fn default() -> ShadowText {
		ShadowText::new()
	}
}

impl Component for ShadowText {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ShadowText"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, graphic: &mut Graphic) -> Option<Rect> {
		if self.text.is_empty() {
			return Some(Rect::create(0.0, 0.0, 0.0, 0.0));
		}
		if self.is_bake_dirty {
			return None;
		}
		let baked_text = self.baked_text.as_ref()?;
		let content_size = transform.get_content_size();
		if baked_text.content_width != content_size.x || baked_text.scale != graphic.calculate_text_bake_scale() {
			return None;
		}
		Some(self.calculate_quad_rect(graphic, baked_text, content_size.x, content_size.y))
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_shadow_text(transform, graphic);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn blur_spreads_and_keeps_total() {
		let width = 21;
		let height = 21;
		let mut coverage = vec![0u8; (width * height) as usize];
		coverage[(10 * width + 10) as usize] = 255;
		let blurred = blur_coverage(&coverage, width, height, 4.0);
		assert!(blurred[(10 * width + 10) as usize] < 255);
		assert!(blurred[(10 * width + 12) as usize] > 0);
		assert_eq!(blur_coverage(&coverage, width, height, 0.0), coverage);
	}
}
