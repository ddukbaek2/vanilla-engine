//==============================================================================
// 그림자를 깐 그림. (vanilla.js core/component/shadowimage.js 의 ShadowImage)
// - 그림(또는 그림 안의 조각)을 노드 안에 비를 지켜 가장 크게, 가운데로 그린다.
// - ShadowText 와 같이 그림의 모양(불투명도) 밑에 그림자를 깐 모습을 한 장으로 구워 그 한 장을 그린다.
//   노드의 불투명도가 그림과 그림자에 한 장째로 먹는다.
// - 그림 색을 주면 그림을 그 색 한 빛의 모양으로 칠한다. (같은 자리 위에 원래 그림을 얹어 그림자만 남기는 감쌈 등)
// - 굽는 배율, 그림, 색, 그림자, 노드 크기가 바뀌면 다시 굽는다. 줄이기는 겹친 넓이로 고르게 섞는다.
// - 그림 경로(image_path)는 Sprite 의 것과 같이 UI 문서(UIDocument)가 그림 대신 적고 읽는 값이다. 그리기에는 쓰지 않는다.
//==============================================================================
use std::rc::Rc;

use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::core::component::shadowtext::compose_shadow_image;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::core::node::worldnode::NodeTransform;

// 구운 그림의 가장자리 여유. (안티앨리어싱, 픽셀)
const BAKE_PADDING: i32 = 2;

pub struct ShadowImage {
	component_base: ComponentBase,
	image: Option<Rc<Image>>,
	image_rect: Option<Rect>,
	image_color: Option<Color>,
	shadow_color: Color,
	shadow_offset: f32,
	shadow_blur: f32,
	image_version: u32,
	bake_image: Option<Rc<Image>>,
	bake_scale: f32,
	bake_padding: f32,
	bake_key: String,
	image_path: String,
}

//==============================================================================
// 그림의 조각을 그릴 크기로 바꾼 곧은 알파 RGBA. (줄일 때는 겹친 넓이로 고르게, 키울 때는 가까운 픽셀)
//==============================================================================
fn resample_image(image: &Image, source_rect: [f32; 4], target_width: i32, target_height: i32) -> Vec<u8> {
	let source_pixels = image.get_pixels();
	let source_width = image.get_width() as i32;
	let source_height = image.get_height() as i32;
	let mut pixels = vec![0u8; (target_width * target_height * 4) as usize];
	let step_x = source_rect[2] / target_width as f32;
	let step_y = source_rect[3] / target_height as f32;
	for target_y in 0..target_height {
		let start_y = source_rect[1] + target_y as f32 * step_y;
		for target_x in 0..target_width {
			let start_x = source_rect[0] + target_x as f32 * step_x;
			let mut sum = [0.0f32; 4];
			let mut weight_sum = 0.0f32;
			let sample_start_y = start_y.floor() as i32;
			let sample_end_y = (start_y + step_y.max(1.0)).ceil() as i32;
			let sample_start_x = start_x.floor() as i32;
			let sample_end_x = (start_x + step_x.max(1.0)).ceil() as i32;
			for sample_y in sample_start_y..sample_end_y {
				if sample_y < 0 || sample_y >= source_height {
					continue;
				}
				let overlap_y = ((sample_y + 1) as f32).min(start_y + step_y.max(1.0)) - (sample_y as f32).max(start_y);
				for sample_x in sample_start_x..sample_end_x {
					if sample_x < 0 || sample_x >= source_width {
						continue;
					}
					let overlap_x = ((sample_x + 1) as f32).min(start_x + step_x.max(1.0)) - (sample_x as f32).max(start_x);
					let weight = overlap_x.max(0.0) * overlap_y.max(0.0);
					if weight <= 0.0 {
						continue;
					}
					let offset = ((sample_y * source_width + sample_x) * 4) as usize;
					let alpha = source_pixels[offset + 3] as f32;
					// 색은 알파를 곱해 섞어 가장자리가 검게 번지지 않게 한다.
					sum[0] += source_pixels[offset] as f32 * alpha * weight;
					sum[1] += source_pixels[offset + 1] as f32 * alpha * weight;
					sum[2] += source_pixels[offset + 2] as f32 * alpha * weight;
					sum[3] += alpha * weight;
					weight_sum += weight;
				}
			}
			let offset = ((target_y * target_width + target_x) * 4) as usize;
			if weight_sum > 0.0 && sum[3] > 0.0 {
				pixels[offset] = (sum[0] / sum[3]).round().clamp(0.0, 255.0) as u8;
				pixels[offset + 1] = (sum[1] / sum[3]).round().clamp(0.0, 255.0) as u8;
				pixels[offset + 2] = (sum[2] / sum[3]).round().clamp(0.0, 255.0) as u8;
				pixels[offset + 3] = (sum[3] / weight_sum).round().clamp(0.0, 255.0) as u8;
			}
		}
	}
	pixels
}

impl ShadowImage {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ShadowImage {
		ShadowImage {
			component_base: ComponentBase::new(),
			image: None,
			image_rect: None,
			image_color: None,
			shadow_color: Color::black(),
			shadow_offset: 0.0,
			shadow_blur: 0.0,
			image_version: 0,
			bake_image: None,
			bake_scale: 1.0,
			bake_padding: 0.0,
			bake_key: String::new(),
			image_path: String::new(),
		}
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	pub fn draw_shadow_image(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let image = match &self.image {
			Some(image) => {
				image.clone()
			}
			None => {
				return;
			}
		};
		let source_rect = match self.image_rect {
			Some(image_rect) => {
				[image_rect.position.x, image_rect.position.y, image_rect.size.x, image_rect.size.y]
			}
			None => {
				[0.0, 0.0, image.get_width() as f32, image.get_height() as f32]
			}
		};
		let content_size = transform.get_content_size();
		if source_rect[2] <= 0.0 || source_rect[3] <= 0.0 || content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}

		// 노드 안에 그림의 비를 지켜 가장 크게, 가운데로.
		let fit_scale = (content_size.x / source_rect[2]).min(content_size.y / source_rect[3]);
		let image_width = source_rect[2] * fit_scale;
		let image_height = source_rect[3] * fit_scale;
		let bake_scale = graphic.calculate_text_bake_scale();
		let bake_key = format!("{}|{:?}|{:?}|{}|{}|{}|{}|{}", self.image_version, self.image_color, self.shadow_color, self.shadow_offset, self.shadow_blur, image_width, image_height, bake_scale);
		if bake_key != self.bake_key {
			self.bake_key = bake_key;
			self.bake(&image, source_rect, image_width, image_height, bake_scale);
		}
		let bake_image = match &self.bake_image {
			Some(bake_image) => {
				bake_image.clone()
			}
			None => {
				return;
			}
		};
		let quad_x = (content_size.x - image_width) * 0.5 - self.bake_padding;
		let quad_y = (content_size.y - image_height) * 0.5 - self.bake_padding;
		let bake_width = bake_image.get_width() as f32;
		let bake_height = bake_image.get_height() as f32;
		graphic.draw_image_with_source_and_destination(&bake_image, 0.0, 0.0, bake_width, bake_height, quad_x, quad_y, bake_width / self.bake_scale, bake_height / self.bake_scale);
	}

	//==============================================================================
	// 굽기. (그릴 크기로 바꾼 그림을 가장자리를 둘러 놓고, 그 모양으로 그림자를 깐다)
	//==============================================================================
	fn bake(&mut self, image: &Image, source_rect: [f32; 4], image_width: f32, image_height: f32, bake_scale: f32) {
		let scaled_width = ((image_width * bake_scale).round() as i32).max(1);
		let scaled_height = ((image_height * bake_scale).round() as i32).max(1);
		let scaled_blur = self.shadow_blur * bake_scale;
		let scaled_offset = (self.shadow_offset * bake_scale).round() as i32;
		let padding = BAKE_PADDING + scaled_blur.ceil() as i32 * 2;
		let bake_width = scaled_width + padding * 2;
		let bake_height = scaled_height + padding * 2 + scaled_offset.max(0);
		let scaled_pixels = resample_image(image, source_rect, scaled_width, scaled_height);
		let mut shape_pixels = vec![0u8; (bake_width * bake_height * 4) as usize];
		let mut shape_coverage = vec![0u8; (bake_width * bake_height) as usize];
		for y in 0..scaled_height {
			for x in 0..scaled_width {
				let source_offset = ((y * scaled_width + x) * 4) as usize;
				let target_index = ((y + padding) * bake_width + x + padding) as usize;
				let target_offset = target_index * 4;
				let alpha = scaled_pixels[source_offset + 3];
				match self.image_color {
					Some(image_color) => {
						shape_pixels[target_offset] = (image_color.red * 255.0).round() as u8;
						shape_pixels[target_offset + 1] = (image_color.green * 255.0).round() as u8;
						shape_pixels[target_offset + 2] = (image_color.blue * 255.0).round() as u8;
						shape_pixels[target_offset + 3] = (alpha as f32 * image_color.alpha).round() as u8;
					}
					None => {
						shape_pixels[target_offset..target_offset + 4].copy_from_slice(&scaled_pixels[source_offset..source_offset + 4]);
					}
				}
				shape_coverage[target_index] = alpha;
			}
		}
		self.bake_image = Some(compose_shadow_image(&shape_coverage, Some(&shape_pixels), bake_width, bake_height, Color::white(), self.shadow_color, scaled_blur, scaled_offset));
		self.bake_scale = bake_scale;
		self.bake_padding = padding as f32 / bake_scale;
	}

	//==============================================================================
	// 그림 설정. (조각을 주지 않으면 그림 전체)
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>, image_rect: Option<Rect>) {
		self.image = image;
		self.image_rect = image_rect;
		self.image_version += 1;
	}

	//==============================================================================
	// 그림을 한 빛으로 칠할 색 설정. (None 이면 그림 그대로)
	//==============================================================================
	pub fn set_image_color(&mut self, image_color: Option<Color>) {
		self.image_color = image_color;
	}

	//==============================================================================
	// 그림을 한 빛으로 칠할 색 반환.
	//==============================================================================
	pub fn get_image_color(&self) -> Option<Color> {
		self.image_color
	}

	//==============================================================================
	// 그림 경로 설정. (UI 문서가 적고 읽는다)
	//==============================================================================
	pub fn set_image_path(&mut self, image_path: &str) {
		self.image_path = String::from(image_path);
	}

	//==============================================================================
	// 그림 경로 반환. (정하지 않았으면 빈 글)
	//==============================================================================
	pub fn get_image_path(&self) -> &str {
		&self.image_path
	}

	//==============================================================================
	// 그림자 설정. (색과 불투명도는 색의 알파로, 아래로 내려가는 만큼, 번지는 만큼, 둘 다 내용 크기 단위)
	//==============================================================================
	pub fn set_shadow(&mut self, shadow_color: impl Into<Color>, shadow_offset: f32, shadow_blur: f32) {
		self.shadow_color = shadow_color.into();
		self.shadow_offset = shadow_offset;
		self.shadow_blur = shadow_blur.max(0.0);
	}
}

impl Default for ShadowImage {
	fn default() -> ShadowImage {
		ShadowImage::new()
	}
}

impl Component for ShadowImage {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ShadowImage"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_shadow_image(transform, graphic);
	}
}
