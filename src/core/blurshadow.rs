//==============================================================================
//! 흐린 둥근 사각형 그림자. (엔진이 그리는 질문 창, 문맥 메뉴, 빠른 메뉴가 쓴다, 원본 core/blurshadow.js 와 같은 셈)
//! - 브라우저의 CSS box-shadow 나 크롬 views 의 BubbleBorder 그림자를 쓰지 않고, 둥근 사각형의 덮인 정도를 같은 시그마의 가우스로 흐린 그림을 CPU 로 구워 엔진 Graphic 으로 그린다.
//!   두 엔진이 같은 셈으로 같은 그림을 굽는다. (웹과 데스크탑 같다)
//! - 흐림 반경을 시그마로 바꾸는 셈은 크롬 원문 그대로다.
//!   CSS box-shadow 와 Canvas2D shadowBlur 는 블링크 shadow_data.h 의 BlurRadiusToStdDev (반경 x 0.5, 명세 그대로),
//!   views 그림자(gfx::ShadowValue)는 ui/gfx/skia_paint_util.cc 의 RadiusToSigma(blur / 2) (0.288675 x 반경 + 0.5).
//! - 그림은 지금 변환의 배율(기기 픽셀)로 굽고, 크기, 모서리, 시그마, 배율, 색이 같으면 다시 굽지 않는다.
//==============================================================================
use std::rc::Rc;

use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;
use crate::core::image::Image;

//==============================================================================
/// CSS box-shadow 의 흐림 반경을 가우스 시그마로. (블링크 ShadowData::BlurRadiusToStdDev, 반경의 반)
///
/// # 인자
/// - `blur_radius`: CSS 흐림 반경. (CSS 픽셀, 0 이상)
///
/// # 돌려줌
/// 시그마. (CSS 픽셀)
//==============================================================================
pub(crate) fn css_blur_radius_to_sigma(blur_radius: f64) -> f64 {
	blur_radius.max(0.0) * 0.5
}

//==============================================================================
/// 크롬 views 그림자 값(gfx::ShadowValue 의 blur)을 가우스 시그마로. (ui/gfx/skia_paint_util.cc 의 RadiusToSigma(blur / 2))
///
/// # 인자
/// - `shadow_blur`: ShadowValue 의 blur. (도형 안팎을 같이 센 정수 픽셀)
///
/// # 돌려줌
/// 시그마. (DIP)
//==============================================================================
pub(crate) fn views_shadow_blur_to_sigma(shadow_blur: i32) -> f64 {
	// blur() / 2 는 정수 나눗셈이고, 0.288675f 는 float 를 double 로 넓혀 셈한 뒤 float 로 돌린다.
	let radius = (shadow_blur / 2) as f64;
	if radius > 0.0 {
		(0.288675f32 as f64 * radius + 0.5) as f32 as f64
	}
	else {
		0.0
	}
}

//==============================================================================
/// 구운 그림자 덮임 그림. (기기 픽셀, 둥근 사각형은 (margin, margin) 에서 시작한다)
//==============================================================================
pub(crate) struct ShadowMask {
	/// 폭. (기기 픽셀)
	pub(crate) width: u32,
	/// 높이. (기기 픽셀)
	pub(crate) height: u32,
	/// 둥근 사각형 바깥 여백. (기기 픽셀, 가우스 3 시그마 + 1)
	pub(crate) margin: u32,
	/// 덮인 정도. (0 ~ 1, 줄마다 왼쪽에서 오른쪽으로)
	pub(crate) coverage_list: Vec<f32>,
}

//==============================================================================
/// 둥근 사각형의 덮인 정도를 가우스로 흐린 그림 굽기.
/// - 덮인 정도는 픽셀 가운데의 부호 거리로 셈한 반 픽셀 앤티에일리어싱이다.
/// - 가우스는 가로, 세로 두 번 나눠 걸고(같은 값이 되는 줄과 열은 다시 셈하지 않는다), 낟알은 3 시그마까지 합이 1 이 되게 고른다.
///
/// # 인자
/// - `width`, `height`: 둥근 사각형 크기. (기기 픽셀)
/// - `corner_radius`: 모서리 반지름. (기기 픽셀)
/// - `sigma`: 가우스 시그마. (기기 픽셀, 0 이면 흐리지 않는다)
///
/// # 돌려줌
/// 구운 덮임 그림.
//==============================================================================
pub(crate) fn bake_shadow_mask(width: f64, height: f64, corner_radius: f64, sigma: f64) -> ShadowMask {
	let width = width.max(0.0);
	let height = height.max(0.0);
	let corner_radius = corner_radius.max(0.0).min(width * 0.5).min(height * 0.5);
	let kernel_radius = if sigma > 0.01 {
		(sigma * 3.0).ceil() as usize
	}
	else {
		0
	};
	let margin = kernel_radius as u32 + 1;
	let image_width = width.ceil() as u32 + margin * 2;
	let image_height = height.ceil() as u32 + margin * 2;
	let image_width_size = image_width as usize;
	let image_height_size = image_height as usize;

	// 가우스 낟알.
	let mut kernel = Vec::with_capacity(kernel_radius * 2 + 1);
	if kernel_radius == 0 {
		kernel.push(1.0f32);
	}
	else {
		let denominator = 2.0 * sigma * sigma;
		let mut sum = 0.0f64;
		let mut weight_list = Vec::with_capacity(kernel_radius * 2 + 1);
		for offset in -(kernel_radius as i64)..=(kernel_radius as i64) {
			let weight = (-(offset * offset) as f64 / denominator).exp();
			sum += weight;
			weight_list.push(weight);
		}
		for weight in weight_list {
			kernel.push((weight / sum) as f32);
		}
	}

	// 덮인 정도와 가로 흐림. (열 먼저 담아 세로 흐림이 이어진 조각을 읽게 한다)
	let half_width = width * 0.5;
	let half_height = height * 0.5;
	let center_x = margin as f64 + half_width;
	let center_y = margin as f64 + half_height;
	let mut column_major = vec![0.0f32; image_width_size * image_height_size];
	let mut coverage_row = vec![0.0f32; image_width_size];
	let mut previous_coverage_row: Vec<f32> = Vec::new();
	let mut blurred_row = vec![0.0f32; image_width_size];
	for row_index in 0..image_height_size {
		let local_y = (row_index as f64 + 0.5 - center_y).abs() - (half_height - corner_radius);
		for column_index in 0..image_width_size {
			let local_x = (column_index as f64 + 0.5 - center_x).abs() - (half_width - corner_radius);
			let outside_length = (local_x.max(0.0) * local_x.max(0.0) + local_y.max(0.0) * local_y.max(0.0)).sqrt();
			let distance = outside_length + local_x.max(local_y).min(0.0) - corner_radius;
			coverage_row[column_index] = (0.5 - distance).clamp(0.0, 1.0) as f32;
		}
		if coverage_row != previous_coverage_row {
			blur_line(&coverage_row, &kernel, kernel_radius, &mut blurred_row);
			previous_coverage_row.clear();
			previous_coverage_row.extend_from_slice(&coverage_row);
		}
		for column_index in 0..image_width_size {
			column_major[column_index * image_height_size + row_index] = blurred_row[column_index];
		}
	}

	// 세로 흐림.
	let mut coverage_list = vec![0.0f32; image_width_size * image_height_size];
	let mut blurred_column = vec![0.0f32; image_height_size];
	for column_index in 0..image_width_size {
		let column_start = column_index * image_height_size;
		let column = &column_major[column_start..column_start + image_height_size];
		if column_index == 0 || column != &column_major[column_start - image_height_size..column_start] {
			blur_line(column, &kernel, kernel_radius, &mut blurred_column);
		}
		for row_index in 0..image_height_size {
			coverage_list[row_index * image_width_size + column_index] = blurred_column[row_index];
		}
	}
	ShadowMask {
		width: image_width,
		height: image_height,
		margin,
		coverage_list,
	}
}

//==============================================================================
/// 한 줄 가우스. (줄 밖은 0)
//==============================================================================
fn blur_line(source: &[f32], kernel: &[f32], kernel_radius: usize, destination: &mut [f32]) {
	let length = source.len();
	for index in 0..length {
		let mut sum = 0.0f64;
		let first = index.saturating_sub(kernel_radius);
		let last = (index + kernel_radius).min(length - 1);
		for source_index in first..=last {
			sum += source[source_index] as f64 * kernel[source_index + kernel_radius - index] as f64;
		}
		destination[index] = sum as f32;
	}
}

//==============================================================================
/// 흐린 둥근 사각형 그림자 그리개. (구운 그림 하나를 들고 같은 값이면 다시 쓴다)
//==============================================================================
#[derive(Default)]
pub(crate) struct BlurredRoundRectShadow {
	/// 구운 값. (폭, 높이, 모서리, 시그마, 배율, 빨강, 초록, 파랑, 알파의 비트)
	baked_key: Option<[u64; 9]>,
	/// 구운 그림.
	baked_image: Option<Rc<Image>>,
	/// 구운 그림의 여백. (기기 픽셀)
	baked_margin: u32,
}

impl BlurredRoundRectShadow {
	//==============================================================================
	/// 그림자 그리기. (지금 Graphic 변환 안의 논리 좌표)
	///
	/// # 인자
	/// - `graphic`: 그릴 Graphic.
	/// - `shape_rect`: 그림자를 드리우는 둥근 사각형. (밀기 전)
	/// - `corner_radius`: 모서리 반지름.
	/// - `offset`: 그림자 밀기.
	/// - `sigma`: 가우스 시그마. (논리 픽셀)
	/// - `color`: 그림자 색. (덮인 정도에 알파를 곱한다)
	//==============================================================================
	pub(crate) fn draw(&mut self, graphic: &mut Graphic, shape_rect: Rect, corner_radius: f64, offset: Vector2, sigma: f64, color: Color) {
		if shape_rect.size.x <= 0.0 || shape_rect.size.y <= 0.0 || color.alpha <= 0.0 {
			return;
		}
		let transform_matrix = graphic.get_transform_matrix();
		let scale = (transform_matrix.a * transform_matrix.a + transform_matrix.b * transform_matrix.b).sqrt().max(0.001);
		let key = [
			shape_rect.size.x.to_bits(),
			shape_rect.size.y.to_bits(),
			corner_radius.to_bits(),
			sigma.to_bits(),
			scale.to_bits(),
			color.red.to_bits(),
			color.green.to_bits(),
			color.blue.to_bits(),
			color.alpha.to_bits(),
		];
		if self.baked_key != Some(key) || self.baked_image.is_none() {
			let mask = bake_shadow_mask(shape_rect.size.x * scale, shape_rect.size.y * scale, corner_radius * scale, sigma * scale);
			let red = (color.red.clamp(0.0, 1.0) * 255.0).round() as u8;
			let green = (color.green.clamp(0.0, 1.0) * 255.0).round() as u8;
			let blue = (color.blue.clamp(0.0, 1.0) * 255.0).round() as u8;
			let alpha = color.alpha.clamp(0.0, 1.0);
			let mut straight_pixels = Vec::with_capacity(mask.coverage_list.len() * 4);
			for coverage in &mask.coverage_list {
				straight_pixels.extend_from_slice(&[red, green, blue, (*coverage as f64 * alpha * 255.0).round() as u8]);
			}
			self.baked_image = Some(Image::create_from_pixels(mask.width, mask.height, straight_pixels));
			self.baked_margin = mask.margin;
			self.baked_key = Some(key);
		}
		if let Some(image) = &self.baked_image {
			let margin = self.baked_margin as f64 / scale;
			let position = Vector2::create(shape_rect.position.x + offset.x - margin, shape_rect.position.y + offset.y - margin);
			let content_size = Vector2::create(image.get_width() as f64 / scale, image.get_height() as f64 / scale);
			graphic.draw_image(image, position, content_size);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn sigma_conversion_matches_chromium() {
		// CSS 0 4px 16px 은 시그마 8, views 의 흐림 8 과 12 는 RadiusToSigma(4), RadiusToSigma(6).
		assert_eq!(css_blur_radius_to_sigma(16.0), 8.0);
		assert!((views_shadow_blur_to_sigma(8) - 1.6547).abs() < 1e-4);
		assert!((views_shadow_blur_to_sigma(12) - 2.23205).abs() < 1e-4);
		assert_eq!(views_shadow_blur_to_sigma(1), 0.0);
	}

	#[test]
	fn baked_mask_keeps_mass_and_edges() {
		let mask = bake_shadow_mask(100.0, 60.0, 8.0, 8.0);
		assert_eq!(mask.margin, 25);
		assert_eq!(mask.width, 150);
		assert_eq!(mask.height, 110);
		let area = 100.0 * 60.0 - (4.0 - std::f64::consts::PI) * 64.0;
		let sum: f64 = mask.coverage_list.iter().map(|coverage| *coverage as f64).sum();
		assert!((sum - area).abs() / area < 0.01, "sum {sum} area {area}");
		let read = |x: u32, y: u32| mask.coverage_list[(y * mask.width + x) as usize];
		// 가운데는 거의 다 덮이고, 곧은 변 위는 반, 3 시그마 밖은 거의 0.
		assert!(read(75, 55) > 0.99);
		assert!((read(75, 25) - 0.5).abs() < 0.03, "{}", read(75, 25));
		assert!(read(75, 0) < 0.003);
		// 위아래, 좌우가 같다.
		assert!((read(10, 30) - read(139, 79)).abs() < 1e-5);
	}

	#[test]
	fn unblurred_mask_is_antialiased_shape() {
		let mask = bake_shadow_mask(10.0, 10.0, 0.0, 0.0);
		assert_eq!(mask.margin, 1);
		let read = |x: u32, y: u32| mask.coverage_list[(y * mask.width + x) as usize];
		assert_eq!(read(0, 0), 0.0);
		assert_eq!(read(5, 5), 1.0);
	}
}
