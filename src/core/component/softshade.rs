//==============================================================================
//! 부드러운 그늘. (vanilla.js core/component/softshade.js 의 SoftShade, SoftShadeShape)
//! - 글 뒤를 어둡게 눌러 밝은 바탕 위에서도 읽히게 할 때 쓴다. 모양을 작은 그림 한 장에 구워 노드의 내용 크기로 늘려 그린다.
//! - 부드러운 그라데이션이라 낮은 해상도로 구워 늘려도 자국이 보이지 않는다. (구운 한 픽셀이 굽기 단위만큼을 덮고, 가로세로 상한이 있다)
//! - 노드 크기, 모양, 색, 짙기, 양옆 흐림이 바뀌면 다시 굽는다. 같은 모양을 구운 것이 있으면 나눠 쓴다. (sharedbakecache.rs) 노드의 불투명도는 한 장째로 먹는다.
//! - 원본 getBakeCanvas 는 구운 캔버스 대신 구운 Image 를 돌려주는 get_bake_image 다.
//==============================================================================
use std::rc::Rc;

use crate::base::color::Color;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::core::node::worldnode::NodeTransform;
use crate::core::sharedbakecache;

//==============================================================================
/// 부드러운 그늘의 모양.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoftShadeShape {
	/// 가운데 가로줄이 가장 짙고 위아래 끝으로 차츰 엷어지는 띠. (양옆은 fade_width 안쪽까지 차츰)
	Band,
	/// 아래 끝이 가장 짙고 위 끝에서 0 인 세로 그라데이션.
	Bottom,
}

// 그늘 모양의 반사 값. (원본 SoftShadeShape 글)
crate::core::reflection::impl_reflect_names!(SoftShadeShape, [
	(SoftShadeShape::Band, "band"),
	(SoftShadeShape::Bottom, "bottom"),
]);

pub struct SoftShade {
	component_base: ComponentBase,
	shape: SoftShadeShape,
	color: Color,
	opacity: f32,
	fade_width: f32,
	bake_unit: f32,
	bake_maximum_size: u32,
	bake_image: Option<Rc<Image>>,
	bake_key: Option<(SoftShadeShape, [u32; 3], u32, u32, u32, u32)>,
}

//==============================================================================
/// 부드럽게 0 에서 1 로. (edge 이하에서 세제곱 곡선, edge 가 0 이면 계단)
//==============================================================================
fn read_smooth_rise(edge: f32, value: f32) -> f32 {
	if edge <= 0.0 {
		return if value > 0.0 { 1.0 } else { 0.0 };
	}
	let ratio = (value / edge).clamp(0.0, 1.0);
	ratio * ratio * (3.0 - 2.0 * ratio)
}

#[crate::reflect]
impl SoftShade {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> SoftShade {
		SoftShade {
			component_base: ComponentBase::new(),
			shape: SoftShadeShape::Band,
			color: Color::black(),
			opacity: 0.5,
			fade_width: 0.0,
			bake_unit: 4.0,
			bake_maximum_size: 256,
			bake_image: None,
			bake_key: None,
		}
	}

	//==============================================================================
	/// 출력.
	//==============================================================================
	pub fn draw_shade(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let content_width = content_size.x;
		let content_height = content_size.y;
		if content_width <= 0.0 || content_height <= 0.0 || self.opacity <= 0.0 {
			return;
		}
		let bake_width = ((content_width / self.bake_unit).ceil() as u32).clamp(2, self.bake_maximum_size.max(2));
		let bake_height = ((content_height / self.bake_unit).ceil() as u32).clamp(2, self.bake_maximum_size.max(2));
		let fade_ratio = (self.fade_width / content_width).min(0.5);
		let color_key = [(self.color.red * 255.0).round() as u32, (self.color.green * 255.0).round() as u32, (self.color.blue * 255.0).round() as u32];
		let bake_key = (self.shape, color_key, (self.opacity * 1000.0).round() as u32, (fade_ratio * 1000.0).round() as u32, bake_width, bake_height);
		if self.bake_key != Some(bake_key) {
			self.bake_key = Some(bake_key);
			// 같은 모양을 구운 것이 있으면 나눠 쓰고, 없으면 굽는다. (sharedbakecache.rs)
			let shared_key = format!("SoftShade|{:?}|{:?}|{}|{}|{}|{}", bake_key.0, bake_key.1, bake_key.2, bake_key.3, bake_key.4, bake_key.5);
			self.bake_image = sharedbakecache::acquire_shared_bake(&shared_key, || Some(self.bake_shade(bake_width, bake_height, fade_ratio)));
		}
		let bake_image = match &self.bake_image {
			Some(bake_image) => bake_image.clone(),
			None => {
				return;
			}
		};
		let image_width = bake_image.get_width() as f32;
		let image_height = bake_image.get_height() as f32;
		graphic.draw_image_with_source_and_destination(&bake_image, 0.0, 0.0, image_width, image_height, 0.0, 0.0, content_width, content_height);
	}

	//==============================================================================
	/// 굽기. (픽셀마다 알파를 셈한다, 텍스처 캐시가 그림마다 한 번 올리므로 구울 때마다 새 그림)
	//==============================================================================
	pub fn bake_shade(&self, bake_width: u32, bake_height: u32, fade_ratio: f32) -> Rc<Image> {
		let red = (self.color.red * 255.0).round().clamp(0.0, 255.0) as u8;
		let green = (self.color.green * 255.0).round().clamp(0.0, 255.0) as u8;
		let blue = (self.color.blue * 255.0).round().clamp(0.0, 255.0) as u8;
		let is_band = self.shape == SoftShadeShape::Band;
		let mut pixels = vec![0u8; (bake_width * bake_height * 4) as usize];
		for row in 0..bake_height {
			let down = (row as f32 + 0.5) / bake_height as f32;
			let row_shade = if is_band { read_smooth_rise(0.5, down.min(1.0 - down)) } else { down };
			for column in 0..bake_width {
				let across = (column as f32 + 0.5) / bake_width as f32;
				let column_shade = if is_band { read_smooth_rise(fade_ratio, across.min(1.0 - across)) } else { 1.0 };
				let pixel_index = ((row * bake_width + column) * 4) as usize;
				pixels[pixel_index] = red;
				pixels[pixel_index + 1] = green;
				pixels[pixel_index + 2] = blue;
				pixels[pixel_index + 3] = (255.0 * self.opacity * row_shade * column_shade).round().clamp(0.0, 255.0) as u8;
			}
		}
		Image::create_from_pixels(bake_width, bake_height, pixels)
	}

	//==============================================================================
	/// 그늘 설정. (모양, 색, 가장 짙은 곳의 불투명도, 띠의 양옆이 엷어지는 너비)
	//==============================================================================
	pub fn set_shade(&mut self, shape: SoftShadeShape, color: impl Into<Color>, opacity: f32, fade_width: f32) {
		self.shape = shape;
		self.color = color.into();
		self.opacity = opacity;
		self.fade_width = fade_width;
	}

	//==============================================================================
	/// 굽기 해상도 설정. (구운 한 픽셀이 덮는 단위, 가로세로 상한)
	//==============================================================================
	pub fn set_bake_resolution(&mut self, bake_unit: f32, bake_maximum_size: u32) {
		self.bake_unit = bake_unit.max(1.0);
		self.bake_maximum_size = bake_maximum_size.max(2);
	}

	//==============================================================================
	/// 모양 반환.
	//==============================================================================
	pub fn get_shape(&self) -> SoftShadeShape {
		self.shape
	}

	//==============================================================================
	/// 색 반환.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	/// 가장 짙은 곳의 불투명도 반환.
	//==============================================================================
	pub fn get_opacity(&self) -> f32 {
		self.opacity
	}

	//==============================================================================
	/// 띠의 양옆이 엷어지는 너비 반환.
	//==============================================================================
	pub fn get_fade_width(&self) -> f32 {
		self.fade_width
	}

	//==============================================================================
	/// 구운 한 픽셀이 덮는 단위 반환.
	//==============================================================================
	pub fn get_bake_unit(&self) -> f32 {
		self.bake_unit
	}

	//==============================================================================
	/// 구운 그림의 가로세로 상한 반환.
	//==============================================================================
	pub fn get_bake_maximum_size(&self) -> u32 {
		self.bake_maximum_size
	}

	//==============================================================================
	/// 구운 그림 반환. (아직 굽지 않았으면 None, 원본 getBakeCanvas)
	//==============================================================================
	#[reflect(name = "BakeCanvas")]
	pub fn get_bake_image(&self) -> Option<&Rc<Image>> {
		self.bake_image.as_ref()
	}
}

impl Default for SoftShade {
	fn default() -> SoftShade {
		SoftShade::new()
	}
}

#[crate::reflect]
impl Component for SoftShade {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"SoftShade"
	}

	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_shade(transform, graphic);
	}
}
