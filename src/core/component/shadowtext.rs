//==============================================================================
//! 그림자를 깐 한 줄 글. (vanilla.js core/component/shadowtext.js 의 ShadowText)
//! - 글자 노드 두 벌(같은 글을 검게 밑에, 글을 위에)로 그림자를 깔면 둘이 따로 비쳐, 노드를 흐리게 할 때 흐린 글 사이로 밑의 그림자가 보인다.
//!   이 컴포넌트는 그림자 위에 글을 얹은 모습을 그림 한 장에 구워 그 한 장을 그리므로, 노드의 불투명도가 글과 그림자에 한 장째로 먹는다.
//! - 가로 정렬은 노드 안에서 왼쪽, 가운데, 오른쪽이고, 세로는 노드의 가운데다.
//! - 글이 노드 너비를 넘으면 처음 크기에서 최소 비율(set_fit_minimum_ratio)까지 줄여 맞춘다.
//! - 그림자는 상자 흐림 세 번(가우시안에 가까움)으로 번지게 할 수 있다. 번지는 만큼 구운 그림의 가장자리를 넓힌다.
//! - 굽는 배율, 글, 꼴, 글 방향, 노드 너비가 바뀌거나 글꼴이 새로 올라오면 다시 굽는다. 같은 모양을 구운 것이 있으면 나눠 쓴다. (sharedbakecache.rs)
//! - 글 방향(set_text_direction, 원본 setTextDirection, 처음 Ltr)으로 양방향 정렬과 모양 짓기를 하고, 정렬 Start / End 를 푼다.
//! - 화면 밖 거르기는 마지막으로 구운 한 장이 놓이는 사각형(넘친 글, 그림자 여백 포함)으로 판정한다. 세터로 바꾼 뒤 아직 굽지 않았거나 노드 너비나 굽는 배율이 달라졌으면 거르지 않는다.
//! - 기울임, 밑줄, 취소선(set_italic, set_underline, set_strikethrough): 선의 자리, 두께는 Text 의 가운데 기준선과 같고, 선도 글과 함께 구워 그림자가 깔린다.
//!   한 줄을 줄여 맞추는 글이라 줄바꿈 방식은 두지 않는다.
//! - 웹은 원본처럼 캔버스에 그림자(shadowColor, shadowBlur, shadowOffsetY)를 걸고 글자 색으로 fillText 해 굽는다. (platform/web/systemfont.rs 의 bake_shadow_text, 여백과 자리 셈이 원본과 같다)
//!   데스크탑은 글자 캐시의 rasterize_text 로 모양을 받아 상자 흐림 세 번으로 그림자를 셈한다.
//! - 색 글리프(이모지)는 제 색에 글자 색의 알파만 곱해 얹고, 그림자는 이모지 모양까지 드리운다. (원본 fillStyle 과 캔버스 그림자)
//! - 원본과 다른 것: 원본 setFontFace(FontFace) 는 set_font_family(올린 글꼴 이름), 그림자의 색과 불투명도(setShadow 의 색 글과 opacity)는 Color 하나(알파가 불투명도)로 받는다.
//!   원본 getBakeCanvas 는 구운 캔버스 대신 구운 Image 를 돌려주는 get_bake_image 다.
//==============================================================================
use std::rc::Rc;

use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::richtext::build_font_string;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline, TextDirection};
use crate::core::image::Image;
use crate::core::node::worldnode::NodeTransform;
use crate::core::sharedbakecache;
use crate::core::textstringtexturecache::TextRaster;

/// 구운 그림의 가장자리 여유. (안티앨리어싱, 픽셀)
const BAKE_PADDING: i32 = 2;

//==============================================================================
/// 덮인 정도(0 ~ 255)를 상자 흐림 세 번으로 번지게 하기. (반지름은 픽셀, 0 이하면 그대로)
/// - 가로, 세로 상자 흐림을 세 번 거듭해 가우시안에 가깝게 한다. 상자 반지름은 표준편차를 radius / 2 로 보고 셈하며 1 이상이다.
/// - 그림 밖은 0 으로 쳐서 가장자리 쪽은 옅어진다.
/// - coverage 가 width x height 보다 짧으면 패닉한다. (radius 가 0 이하이거나 width, height 가 0 이하면 읽지 않고 그대로 돌려준다)
///
/// # 인자
/// - `coverage`: 덮인 정도. (0 ~ 255, 한 줄 width 개씩 height 줄)
/// - `width`: 너비. (픽셀)
/// - `height`: 높이. (픽셀)
/// - `radius`: 흐림 반지름. (픽셀, 0 이하면 흐리지 않는다)
///
/// # 돌려줌
/// 번진 덮인 정도. (coverage 와 같은 길이, 0 ~ 255)
///
/// # 예제
/// 한 점을 번지게 하기
/// ```rust
/// use vanilla::blur_coverage;
///
/// let width = 9;
/// let height = 9;
/// let mut coverage = vec![0u8; (width * height) as usize];
/// coverage[(4 * width + 4) as usize] = 255;
/// let blurred = blur_coverage(&coverage, width, height, 4.0);
/// assert_eq!(blurred.len(), coverage.len());
/// assert!(blurred[(4 * width + 4) as usize] < 255);
/// assert!(blurred[(4 * width + 5) as usize] > 0);
/// assert_eq!(blur_coverage(&coverage, width, height, 0.0), coverage);
/// ```
//==============================================================================
pub fn blur_coverage(coverage: &[u8], width: i32, height: i32, radius: f64) -> Vec<u8> {
	if radius <= 0.0 || width <= 0 || height <= 0 {
		return coverage.to_vec();
	}
	// 가우시안의 표준편차를 상자 셋으로 나눈 반지름. (sigma ≈ radius / 2, 상자 너비 = sqrt(12 sigma² / 3 + 1))
	let sigma = radius * 0.5;
	let box_radius = (((12.0 * sigma * sigma / 3.0 + 1.0).sqrt() - 1.0) * 0.5).round().max(1.0) as i32;
	let mut values: Vec<f64> = coverage.iter().map(|value| *value as f64).collect();
	let mut scratch = vec![0.0f64; values.len()];
	for _ in 0..3 {
		blur_axis(&values, &mut scratch, width, height, box_radius, true);
		blur_axis(&scratch, &mut values, width, height, box_radius, false);
	}
	values.iter().map(|value| value.round().clamp(0.0, 255.0) as u8).collect()
}

//==============================================================================
/// 한 축으로 상자 흐림. (밖은 0)
//==============================================================================
fn blur_axis(source: &[f64], target: &mut [f64], width: i32, height: i32, box_radius: i32, is_horizontal: bool) {
	let (line_count, line_length) = if is_horizontal { (height, width) } else { (width, height) };
	let window = (box_radius * 2 + 1) as f64;
	for line_index in 0..line_count {
		let read_index = |position: i32| -> usize {
			if is_horizontal { (line_index * width + position) as usize } else { (position * width + line_index) as usize }
		};
		let mut sum = 0.0f64;
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
/// 그림자 위에 모양을 얹은 RGBA 그림 짓기. (모양의 덮인 정도와 색, 그림자의 덮인 정도와 색과 아래로 내림, 곧은 알파)
/// - 그림자는 shape 를 흐려 shadow_offset_y 만큼 내려 깔고, 그 위에 모양을 알파로 얹는다.
/// - shape_pixels 를 주면 모양의 색을 픽셀마다 쓴다. (그림, None 이면 shape_color 한 빛)
/// - 내린 그림자가 그림 밖으로 나간 부분은 버린다. 길이가 모자라면(shape_coverage 가 width x height, shape_pixels 가 width x height x 4 보다 짧으면) 패닉한다.
///
/// # 인자
/// - `shape_coverage`: 모양의 덮인 정도. (0 ~ 255, width x height) 그림자는 이것을 흐려 짓고, shape_pixels 가 None 이면 모양의 알파도 이것이다.
/// - `shape_pixels`: 모양의 곧은 알파 RGBA 픽셀. (width x height x 4, None 이면 shape_color 한 빛)
/// - `width`: 너비. (픽셀)
/// - `height`: 높이. (픽셀)
/// - `shape_color`: shape_pixels 가 None 일 때 모양 색. (알파를 덮인 정도에 곱한다)
/// - `shadow_color`: 그림자 색. (알파가 불투명도)
/// - `shadow_blur`: 그림자 흐림 반지름. (픽셀, blur_coverage 의 radius, 0 이하면 흐리지 않는다)
/// - `shadow_offset_y`: 그림자를 아래로 내리는 만큼. (픽셀, 음수면 위로)
///
/// # 돌려줌
/// 지은 새 그림. (width x height)
///
/// # 예제
/// 흰 점 아래 한 칸에 검은 그림자
/// ```rust
/// use vanilla::{compose_shadow_image, Color};
///
/// let shape_coverage = [255u8, 0, 0];
/// let image = compose_shadow_image(&shape_coverage, None, 1, 3, Color::white(), Color::black(), 0.0, 1);
/// let pixels = image.get_pixels();
/// assert_eq!(&pixels[0..4], &[255, 255, 255, 255]);
/// assert_eq!(&pixels[4..8], &[0, 0, 0, 255]);
/// assert_eq!(pixels[11], 0);
/// ```
//==============================================================================
pub fn compose_shadow_image(shape_coverage: &[u8], shape_pixels: Option<&[u8]>, width: i32, height: i32, shape_color: Color, shadow_color: Color, shadow_blur: f64, shadow_offset_y: i32) -> Rc<Image> {
	let shadow_coverage = blur_coverage(shape_coverage, width, height, shadow_blur);
	let mut pixels = vec![0u8; (width * height * 4) as usize];
	for y in 0..height {
		for x in 0..width {
			let pixel_index = (y * width + x) as usize;
			let shadow_y = y - shadow_offset_y;
			let shadow_alpha = if shadow_y >= 0 && shadow_y < height { shadow_coverage[(shadow_y * width + x) as usize] as f64 / 255.0 * shadow_color.alpha } else { 0.0 };
			let (shape_red, shape_green, shape_blue, shape_alpha) = match shape_pixels {
				Some(shape_pixels) => {
					let offset = pixel_index * 4;
					(shape_pixels[offset] as f64 / 255.0, shape_pixels[offset + 1] as f64 / 255.0, shape_pixels[offset + 2] as f64 / 255.0, shape_pixels[offset + 3] as f64 / 255.0)
				}
				None => {
					(shape_color.red, shape_color.green, shape_color.blue, shape_coverage[pixel_index] as f64 / 255.0 * shape_color.alpha)
				}
			};
			// 그림자 위에 모양을 얹는다. (곧은 알파의 over)
			let alpha = shape_alpha + shadow_alpha * (1.0 - shape_alpha);
			let offset = pixel_index * 4;
			if alpha > 0.0 {
				let mix_channel = |shape_value: f64, shadow_value: f64| ((shape_value * shape_alpha + shadow_value * shadow_alpha * (1.0 - shape_alpha)) / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
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
/// 글자 색 글자와 색 글리프 층을 한 모양으로. (그림자에 쓸 덮인 정도, 곧은 알파 RGBA)
/// - 글자는 글자 색, 색 글리프는 프리멀티플라이드 제 색에 글자 색의 알파만 곱해 그 위에 얹는다. (크롬 fillText 는 색 글리프에 fillStyle 의 알파만 먹인다)
//==============================================================================
fn compose_color_text_shape(coverage: &[u8], color_pixels: &[u8], text_color: Color) -> (Vec<u8>, Vec<u8>) {
	let mut shape_coverage = Vec::with_capacity(coverage.len());
	let mut shape_pixels = vec![0u8; coverage.len() * 4];
	for (pixel_index, value) in coverage.iter().enumerate() {
		let offset = pixel_index * 4;
		let color_alpha = color_pixels[offset + 3] as f64 / 255.0 * text_color.alpha;
		let text_alpha = *value as f64 / 255.0 * text_color.alpha * (1.0 - color_alpha);
		let alpha = color_alpha + text_alpha;
		shape_coverage.push((*value).max(color_pixels[offset + 3]));
		if alpha <= 0.0 {
			continue;
		}
		let text_channel_list = [text_color.red, text_color.green, text_color.blue];
		for channel_index in 0..3 {
			let premultiplied = color_pixels[offset + channel_index] as f64 / 255.0 * text_color.alpha + text_channel_list[channel_index] * text_alpha;
			shape_pixels[offset + channel_index] = (premultiplied / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
		}
		shape_pixels[offset + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
	}
	(shape_coverage, shape_pixels)
}

//==============================================================================
/// 구운 한 장. (그림, 논리 좌표의 펜 자리와 기준선, 전진폭, 위아래, 배율, 구울 때의 노드 너비)
//==============================================================================
struct BakedText {
	image: Rc<Image>,
	scale: f64,
	content_width: f64,
	pen_offset_x: f64,
	baseline_offset_y: f64,
	advance_width: f64,
	font_ascent: f64,
	font_descent: f64,
}

//==============================================================================
/// 그림자를 깐 한 줄 글 컴포넌트. (그림자 위에 글을 얹은 모습을 그림 한 장에 구워 그린다)
/// - 노드의 불투명도가 글과 그림자에 한 장째로 먹는다. 가로는 정렬대로, 세로는 노드 가운데에 놓는다.
/// - 글이 노드 너비를 넘으면 처음 크기에서 최소 비율까지 줄여 맞춘다.
/// - 처음은 빈 글, 글꼴 default, 크기 16, 가운데 정렬, Ltr, 흰 글자, 검은 그림자(내림 0, 번짐 0), 최소 비율 0.6.
///
/// # 예제
/// 굵은 제목에 흐린 그림자
/// ```rust
/// use vanilla::{Color, NodeTree, ShadowText, TextAlign, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let title_id = node_tree.create_child(root_id, "Title");
/// node_tree[title_id].set_content_size(Vector2::create(320.0, 48.0));
/// let shadow_text = node_tree.add_component(title_id, ShadowText::create_with_text("Stage Clear", 32.0)).unwrap();
/// shadow_text.set_text_style(32.0, true, "#ffee88");
/// shadow_text.set_shadow(Color::black(), 0.6, 2.0, 3.0);
/// shadow_text.set_text_align(TextAlign::Left);
///
/// let shadow_text = node_tree[title_id].get_component::<ShadowText>().unwrap();
/// assert_eq!(shadow_text.get_text(), "Stage Clear");
/// assert!(shadow_text.is_bold());
/// assert_eq!(shadow_text.get_text_align(), TextAlign::Left);
/// ```
//==============================================================================
pub struct ShadowText {
	component_base: ComponentBase,
	text: String,
	font_family: String,
	font_size: f64,
	is_bold: bool,
	is_italic: bool,
	is_underline: bool,
	is_strikethrough: bool,
	text_align: TextAlign,
	text_direction: TextDirection,
	text_color: Color,
	shadow_color: Color,
	shadow_offset: f64,
	shadow_blur: f64,
	fit_minimum_ratio: f64,
	baked_text: Option<Rc<BakedText>>,
	bake_key: String,
	/// 세터로 바꾼 뒤 아직 굽지 않았는지. (화면 밖 거르기가 지난 한 장을 믿을지 가린다)
	is_bake_dirty: bool,
}

#[crate::reflect]
impl ShadowText {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 글, 글꼴 default, 크기 16, 가운데 정렬, 흰 글자, 검은 그림자인 새 컴포넌트.
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
			text_direction: TextDirection::Ltr,
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
	/// 글로 생성.
	///
	/// # 인자
	/// - `text`: 그릴 한 줄 글.
	/// - `font_size`: 처음 글자 크기. (픽셀)
	///
	/// # 돌려줌
	/// 그 글과 크기로 만든 새 컴포넌트. (나머지는 new 와 같다)
	///
	/// # 예제
	/// 점수 글 만들기
	/// ```rust
	/// use vanilla::ShadowText;
	///
	/// let shadow_text = ShadowText::create_with_text("Score 100", 24.0);
	/// assert_eq!(shadow_text.get_text(), "Score 100");
	/// assert_eq!(shadow_text.get_font_size(), 24.0);
	/// assert_eq!(shadow_text.get_font_family(), "default");
	/// ```
	//==============================================================================
	pub fn create_with_text(text: &str, font_size: f64) -> ShadowText {
		let mut shadow_text = ShadowText::new();
		shadow_text.set_text(text);
		shadow_text.font_size = font_size;
		shadow_text
	}

	//==============================================================================
	/// Canvas2D font 꼴 문자열. (Text 와 같은 꼴)
	//==============================================================================
	fn build_font_string(&self, font_size: f64) -> String {
		build_font_string(&self.font_family, font_size, self.is_bold, self.is_italic)
	}

	//==============================================================================
	/// 밑줄 / 취소선 두께. (Text 와 같이 max(1, 글자 크기 / 16))
	//==============================================================================
	fn calculate_decoration_line_width(font_size: f64) -> f64 {
		(font_size / 16.0).max(1.0)
	}

	//==============================================================================
	/// 출력.
	/// - 글이 비면 그리지 않는다. 굽는 배율, 글, 꼴, 색, 그림자, 최소 비율, 노드 너비, 글 방향, 올린 글꼴 수가 지난번과 다르면 다시 굽는다.
	/// - 글꼴 모양을 받지 못해 굽지 못하면 그림자 없이 Graphic 의 글로 그린다. (밑줄, 취소선은 선으로 긋는다)
	///
	/// # 인자
	/// - `transform`: 그릴 노드 트랜스폼. (content_size 를 읽는다)
	/// - `graphic`: 그릴 그래픽. (노드 트랜스폼이 걸린 상태, 굽는 배율과 글자 캐시에 쓴다)
	//==============================================================================
	pub fn draw_shadow_text(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.text.is_empty() {
			return;
		}
		let content_size = transform.get_content_size();
		let bake_scale = graphic.calculate_text_bake_scale();
		let font_registration_count = graphic.get_text_string_texture_cache().get_font_registration_count();
		let bake_key = format!("ShadowText|{}|{}|{}|{}|{}|{}|{}|{:?}|{:?}|{}|{}|{}|{}|{}|{}|{}", self.text, self.font_family, self.font_size, self.is_bold, self.is_italic, self.is_underline, self.is_strikethrough, self.text_color, self.shadow_color, self.shadow_offset, self.shadow_blur, self.fit_minimum_ratio, content_size.x, bake_scale, self.text_direction.get_name(), font_registration_count);
		if bake_key != self.bake_key {
			// 같은 모양을 구운 것이 있으면 나눠 쓰고, 없으면 굽는다.
			self.baked_text = sharedbakecache::acquire_shared_bake(&bake_key, || self.bake(graphic, content_size.x, bake_scale).map(Rc::new));
			self.bake_key = bake_key;
		}
		self.is_bake_dirty = false;
		let baked_text = match &self.baked_text {
			Some(baked_text) => {
				baked_text
			}
			None => {
				// 글꼴 모양을 받을 수 없으면 그림자 없이 엔진의 글로.
				let text_direction = self.resolve_shaping_text_direction();
				let text_align = graphic.resolve_text_align(self.text_align, text_direction);
				graphic.set_text_direction(text_direction);
				let anchor_x = match text_align {
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
					let start_x = match text_align {
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

		let image_width = baked_text.image.get_width() as f64;
		let image_height = baked_text.image.get_height() as f64;
		let image = baked_text.image.clone();
		let quad_rect = self.calculate_quad_rect(graphic, baked_text, content_size.x, content_size.y);
		graphic.draw_image_with_source_and_destination(&image, 0.0, 0.0, image_width, image_height, quad_rect.position.x, quad_rect.position.y, quad_rect.size.x, quad_rect.size.y);
	}

	//==============================================================================
	/// 구운 한 장이 놓이는 사각형. (가로는 정렬대로, 세로는 가운데(middle 기준선)에 놓는다)
	//==============================================================================
	fn calculate_quad_rect(&self, graphic: &Graphic, baked_text: &BakedText, content_width: f64, content_height: f64) -> Rect {
		let (anchor_x, align_offset) = match graphic.resolve_text_align(self.text_align, self.resolve_shaping_text_direction()) {
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
		let image_width = baked_text.image.get_width() as f64;
		let image_height = baked_text.image.get_height() as f64;
		Rect::create(quad_x, quad_y, image_width / baked_text.scale, image_height / baked_text.scale)
	}

	//==============================================================================
	/// 굽기. (넘치면 줄이고, 모양을 받아 그림자를 깐 한 장으로)
	//==============================================================================
	fn bake(&self, graphic: &mut Graphic, fit_width: f64, bake_scale: f64) -> Option<BakedText> {
		let full_font_string = self.build_font_string(self.font_size);
		let text_width = graphic.get_text_string_texture_cache().measure_text(&full_font_string, &self.text).width;
		let mut font_size = self.font_size;
		if fit_width > 0.0 && text_width > fit_width {
			let fit_ratio = (fit_width / text_width).max(self.fit_minimum_ratio);
			font_size = (self.font_size * fit_ratio * 10.0).floor() / 10.0;
		}
		let font_string = self.build_font_string(font_size);
		// 웹: 원본처럼 캔버스에 그림자를 걸고 fillText 로 굽는다.
		#[cfg(target_arch = "wasm32")]
		{
			if graphic.get_text_string_texture_cache().is_browser_raster() {
				return self.bake_with_browser(&font_string, fit_width, bake_scale);
			}
		}
		let scaled_blur = self.shadow_blur * bake_scale;
		let scaled_offset = (self.shadow_offset * bake_scale).round() as i32;
		let mut padding = BAKE_PADDING + scaled_blur.ceil() as i32 * 2 + scaled_offset.max(0);
		if self.is_underline {
			// 밑줄은 기준선 아래로 내려가므로 여유를 더 둔다.
			padding += ((font_size * 0.25 + 2.0) * bake_scale).ceil() as i32;
		}
		// 원본은 글 색 그대로 Canvas2D 에 굽는다. (맥 데스크탑은 그 색 밝기로 감마 전보정)
		let mut text_raster = graphic.get_text_string_texture_cache().rasterize_text_with_color(&self.text, &font_string, bake_scale, padding, self.resolve_shaping_text_direction().to_shaping_direction(), self.text_color)?;
		self.draw_decoration_lines(&mut text_raster, font_size, bake_scale);
		let image = match text_raster.color_pixels.as_ref() {
			Some(color_pixels) => {
				// 색 글리프(이모지)는 제 색에 글자 색의 알파만 곱하고, 그림자는 이모지 모양까지 드리운다. (원본 fillStyle 과 캔버스 그림자)
				let (shape_coverage, shape_pixels) = compose_color_text_shape(&text_raster.coverage, color_pixels, self.text_color);
				compose_shadow_image(&shape_coverage, Some(&shape_pixels), text_raster.width, text_raster.height, self.text_color, self.shadow_color, scaled_blur, scaled_offset)
			}
			None => {
				compose_shadow_image(&text_raster.coverage, None, text_raster.width, text_raster.height, self.text_color, self.shadow_color, scaled_blur, scaled_offset)
			}
		};
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
	/// 브라우저 캔버스로 굽기. (웹, 원본 bakeText: 글자 색으로 fillText, 그림자는 캔버스의 shadowColor / shadowBlur / shadowOffsetY, 밑줄과 취소선은 fillRect)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	fn bake_with_browser(&self, font_string: &str, fit_width: f64, bake_scale: f64) -> Option<BakedText> {
		let font_description = crate::core::textstringtexturecache::parse_font_string(font_string);
		let browser_bake = crate::platform::web::systemfont::bake_shadow_text(&self.text, &font_description, bake_scale, self.resolve_shaping_text_direction().to_shaping_direction(), self.text_color, self.shadow_color, self.shadow_offset, self.shadow_blur, self.is_underline, self.is_strikethrough)?;
		Some(BakedText {
			image: Image::create_from_pixels(browser_bake.bitmap.width, browser_bake.bitmap.height, browser_bake.bitmap.pixels),
			scale: bake_scale,
			content_width: fit_width,
			pen_offset_x: browser_bake.pen_x / bake_scale,
			baseline_offset_y: browser_bake.baseline_y / bake_scale,
			advance_width: browser_bake.advance_width / bake_scale,
			font_ascent: browser_bake.font_ascent / bake_scale,
			font_descent: browser_bake.font_descent / bake_scale,
		})
	}

	//==============================================================================
	/// 밑줄 / 취소선을 글 모양 래스터에 긋기. (가운데 기준선 자리에서 Text 와 같은 오프셋, 길이는 전진폭)
	//==============================================================================
	fn draw_decoration_lines(&self, text_raster: &mut TextRaster, font_size: f64, bake_scale: f64) {
		if !self.is_underline && !self.is_strikethrough {
			return;
		}
		let middle_y = text_raster.baseline_y - (text_raster.font_ascent - text_raster.font_descent) * 0.5;
		let line_width = ShadowText::calculate_decoration_line_width(font_size) * bake_scale;
		let mut center_y_list: Vec<f64> = Vec::new();
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
	/// 글 설정.
	///
	/// # 인자
	/// - `text`: 그릴 한 줄 글. (빈 글이면 그리지 않는다)
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		self.text = String::from(text);
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 글 반환.
	///
	/// # 돌려줌
	/// 그리는 글. (정하지 않았으면 빈 글)
	//==============================================================================
	pub fn get_text(&self) -> &str {
		&self.text
	}

	//==============================================================================
	/// 글꼴 이름 설정. (엔진에 올린 글꼴 이름, 처음은 default. 원본 setFontFace)
	///
	/// # 인자
	/// - `font_family`: 엔진에 올린 글꼴 이름.
	//==============================================================================
	#[reflect(name = "FontFace")]
	pub fn set_font_family(&mut self, font_family: &str) {
		self.font_family = String::from(font_family);
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 글꼴 이름 반환. (원본 getFontFace, 원본은 FontFace 객체라 반사 저장이 적지 않아 이것도 저장에서 뺀다)
	///
	/// # 돌려줌
	/// 글꼴 이름.
	//==============================================================================
	#[reflect(unsaved, name = "FontFace")]
	pub fn get_font_family(&self) -> &str {
		&self.font_family
	}

	//==============================================================================
	/// 처음 글자 크기 반환. (원본 getFontSize)
	///
	/// # 돌려줌
	/// 처음 글자 크기. (픽셀, 넘쳐 줄인 크기가 아니다)
	//==============================================================================
	pub fn get_font_size(&self) -> f64 {
		self.font_size
	}

	//==============================================================================
	/// 가로 정렬 설정.
	///
	/// # 인자
	/// - `text_align`: 노드 안 가로 정렬. (Left, Center, Right, Start / End 는 글 방향으로 푼다, 처음 Center)
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: TextAlign) {
		self.text_align = text_align;
	}

	//==============================================================================
	/// 가로 정렬 반환.
	///
	/// # 돌려줌
	/// 가로 정렬.
	//==============================================================================
	pub fn get_text_align(&self) -> TextAlign {
		self.text_align
	}

	//==============================================================================
	/// 글 방향 설정. (Ltr, Rtl, Auto. 원본 setTextDirection("ltr" / "rtl"), Inherit 는 Ltr 로 본다)
	///
	/// # 인자
	/// - `text_direction`: 글 방향. (Auto 는 글의 첫 강한 글자로 정한다, 처음 Ltr)
	//==============================================================================
	pub fn set_text_direction(&mut self, text_direction: TextDirection) {
		self.text_direction = text_direction;
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 글 방향 반환.
	///
	/// # 돌려줌
	/// 설정한 글 방향. (Auto, Inherit 를 푼 값이 아니다)
	//==============================================================================
	pub fn get_text_direction(&self) -> TextDirection {
		self.text_direction
	}

	//==============================================================================
	/// 굽는 실제 방향. (Rtl 이면 오른쪽에서 왼쪽, Auto 는 글의 첫 강한 글자로, 그 밖은 왼쪽에서 오른쪽)
	//==============================================================================
	fn resolve_shaping_text_direction(&self) -> TextDirection {
		match self.text_direction {
			TextDirection::Rtl => {
				TextDirection::Rtl
			}
			TextDirection::Auto => {
				crate::core::graphic::detect_text_direction(&self.text)
			}
			_ => {
				TextDirection::Ltr
			}
		}
	}

	//==============================================================================
	/// 굵게 여부.
	///
	/// # 돌려줌
	/// 굵게 그리면 true.
	//==============================================================================
	pub fn is_bold(&self) -> bool {
		self.is_bold
	}

	//==============================================================================
	/// 글의 꼴 설정. (처음 크기, 굵기, 색)
	///
	/// # 인자
	/// - `font_size`: 처음 글자 크기. (픽셀, 넘치면 이 크기에서 줄인다)
	/// - `is_bold`: 굵게 그릴지.
	/// - `text_color`: 글자 색. (Color 또는 "#rrggbb" 같은 문자열, 알파가 글자 불투명도)
	//==============================================================================
	pub fn set_text_style(&mut self, font_size: f64, is_bold: bool, text_color: impl Into<Color>) {
		self.font_size = font_size;
		self.is_bold = is_bold;
		self.text_color = text_color.into();
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 기울임 설정.
	///
	/// # 인자
	/// - `is_italic`: 기울여 그릴지. (처음 false)
	//==============================================================================
	pub fn set_italic(&mut self, is_italic: bool) {
		self.is_italic = is_italic;
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 기울임 여부.
	///
	/// # 돌려줌
	/// 기울여 그리면 true.
	//==============================================================================
	pub fn is_italic(&self) -> bool {
		self.is_italic
	}

	//==============================================================================
	/// 밑줄 설정.
	///
	/// # 인자
	/// - `is_underline`: 밑줄을 그을지. (처음 false)
	//==============================================================================
	pub fn set_underline(&mut self, is_underline: bool) {
		self.is_underline = is_underline;
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 밑줄 여부.
	///
	/// # 돌려줌
	/// 밑줄을 그으면 true.
	//==============================================================================
	pub fn is_underline(&self) -> bool {
		self.is_underline
	}

	//==============================================================================
	/// 취소선 설정.
	///
	/// # 인자
	/// - `is_strikethrough`: 취소선을 그을지. (처음 false)
	//==============================================================================
	pub fn set_strikethrough(&mut self, is_strikethrough: bool) {
		self.is_strikethrough = is_strikethrough;
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 취소선 여부.
	///
	/// # 돌려줌
	/// 취소선을 그으면 true.
	//==============================================================================
	pub fn is_strikethrough(&self) -> bool {
		self.is_strikethrough
	}

	//==============================================================================
	/// 그림자 설정. (색, 불투명도, 아래로 내려가는 만큼, 번지는 만큼, 원본 setShadow(색, 불투명도, 내려가는 만큼, 번지는 만큼) 과 같다)
	///
	/// # 인자
	/// - `shadow_color`: 그림자 색. (Color 또는 "#rrggbb" 같은 문자열, 색의 알파는 읽지 않는다, 처음 검정)
	/// - `shadow_opacity`: 그림자 불투명도. (0 ~ 1, 처음 1)
	/// - `shadow_offset`: 그림자가 아래로 내려가는 만큼. (노드 좌표, 처음 0)
	/// - `shadow_blur`: 그림자가 번지는 만큼. (노드 좌표, 0 보다 작으면 0, 처음 0)
	//==============================================================================
	pub fn set_shadow(&mut self, shadow_color: impl Into<Color>, shadow_opacity: f64, shadow_offset: f64, shadow_blur: f64) {
		let mut shadow_color: Color = shadow_color.into();
		shadow_color.alpha = shadow_opacity.clamp(0.0, 1.0);
		self.shadow_color = shadow_color;
		self.shadow_offset = shadow_offset;
		self.shadow_blur = shadow_blur.max(0.0);
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 넘칠 때 줄이는 가장 작은 비율 설정. (1 이면 줄이지 않는다)
	///
	/// # 인자
	/// - `fit_minimum_ratio`: 처음 글자 크기에 대한 가장 작은 비율. (0.01 ~ 1 로 맞춘다, 처음 0.6)
	///
	/// # 예제
	/// 줄이지 않게 하기와 아래 한계
	/// ```rust
	/// use vanilla::ShadowText;
	///
	/// let mut shadow_text = ShadowText::create_with_text("A very long caption", 20.0);
	/// assert_eq!(shadow_text.get_fit_minimum_ratio(), 0.6);
	/// shadow_text.set_fit_minimum_ratio(1.0);
	/// assert_eq!(shadow_text.get_fit_minimum_ratio(), 1.0);
	/// shadow_text.set_fit_minimum_ratio(0.0);
	/// assert_eq!(shadow_text.get_fit_minimum_ratio(), 0.01);
	/// ```
	//==============================================================================
	pub fn set_fit_minimum_ratio(&mut self, fit_minimum_ratio: f64) {
		self.fit_minimum_ratio = fit_minimum_ratio.clamp(0.01, 1.0);
		self.is_bake_dirty = true;
	}

	//==============================================================================
	/// 넘칠 때 줄이는 가장 작은 비율 반환.
	///
	/// # 돌려줌
	/// 처음 글자 크기에 대한 가장 작은 비율. (0.01 ~ 1)
	//==============================================================================
	pub fn get_fit_minimum_ratio(&self) -> f64 {
		self.fit_minimum_ratio
	}

	//==============================================================================
	/// 구운 그림 반환. (아직 굽지 않았거나 그림자 없이 엔진의 글로 그렸으면 None, 원본 getBakeCanvas)
	///
	/// # 돌려줌
	/// 마지막으로 구운 그림. 아직 굽지 않았거나 굽지 못해 엔진의 글로 그렸으면 None.
	//==============================================================================
	#[reflect(name = "BakeCanvas")]
	pub fn get_bake_image(&self) -> Option<&Rc<Image>> {
		self.baked_text.as_ref().map(|baked_text| &baked_text.image)
	}
}

impl Default for ShadowText {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ShadowText {
		ShadowText::new()
	}
}

#[crate::reflect]
impl Component for ShadowText {
	//==============================================================================
	/// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 이름. ("ShadowText")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"ShadowText"
	}

	//==============================================================================
	/// tick 이 할 일이 있는지. (그리기만 하므로 false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 노드 content_size 영역 안에만 그리는지. (true, 실제 사각형은 calculate_drawn_rect 가 준다)
	//==============================================================================
	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 화면 밖 거르기에 쓰는, 실제로 그리는 사각형. (글이 비면 빈 사각형, 바꾼 뒤 아직 굽지 않았거나 노드 너비, 굽는 배율이 구울 때와 다르면 None)
	//==============================================================================
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

	//==============================================================================
	/// 출력. (draw_shadow_text)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_shadow_text(transform, graphic);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 흐림은 한 점을 둘레로 번지게 하고 가운데를 낮춘다. 반지름 0 이면 그대로다.
	//==============================================================================
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
