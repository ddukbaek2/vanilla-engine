//==============================================================================
//! 아지랑이 효과 컴포넌트. (원본 effect/heathazeeffect.js 의 HeatHazeEffect)
//! - 이 노드가 그려지기 전까지 화면에 그려진 것을 노드 영역만큼 떠 와서, 한 줄씩 사인 곡선을 따라 좌우로 흔들어 다시 그린다.
//!   그래서 씬의 맨 위(마지막)에 그려지는 노드에 붙여야 보인다. 자식 노드는 효과 위에 그려진다.
//! - 원본처럼 묶음을 내보낸 뒤 노드의 화면 영역(픽셀)을 copyTexSubImage2D 로 텍스처에 떠 오고, 줄마다 쿼드 한 장씩 정점 색 쿼드(draw_colored_quads)로 그린다.
//!   노드 영역이 화면 밖으로 나가면 화면 가장자리 픽셀을 늘인다. (CLAMP_TO_EDGE)
//! - 원본처럼 노드에 붙이는 컴포넌트이고, 붙을 때 노드 content_size 가 비어 있으면 300 x 300 으로 정한다.
//! - 원본과 다른 점:
//!   - 떠 온 텍스처는 GL 자원이라 다 쓰면 destroy(graphic) 를 부른다. 원본 destroy() 는 텍스처를 만든 컨텍스트를 쥐고 있어 인자가 없다.
//!   ```text
//!     러스트 판은 GL 컨텍스트를 Graphic 만 쥐므로 graphic 을 받는다. (ScreenEffect 와 같다. 부르지 않으면 컨텍스트가 없어질 때까지 남는다)
//!   ```
//!   - 줄이 한 번에 그릴 수 있는 정점 수(Graphic 한도)를 넘으면 나눠 그린다. 원본은 배열을 늘려 한 번에 그린다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{GlTexture, Graphic, PARTICLE_FLOATS_PER_VERTEX};
use crate::effect::particlesystem::draw_colored_quads_in_chunks;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use glow::HasContext;

/// 붙을 때 content_size 가 비어 있으면 쓰는 크기. (원본 생성자)
const DEFAULT_CONTENT_SIZE: f32 = 300.0;

//==============================================================================
/// 아지랑이 효과 컴포넌트.
//==============================================================================
pub struct HeatHazeEffect {
	component_base: ComponentBase,
	timer: f32,
	/// 흔들리는 속도.
	speed: f32,
	/// 흔들리는 폭. (강도, 픽셀)
	amplitude: f32,
	/// 흔들리는 빈도. (물결의 촘촘함)
	frequency: f32,
	capture_texture: Option<GlTexture>,
	capture_texture_width: i32,
	capture_texture_height: i32,
}

#[crate::reflect]
impl HeatHazeEffect {
	//==============================================================================
	/// 생성. (속도 2, 폭 3, 빈도 0.05)
	//==============================================================================
	pub fn new() -> HeatHazeEffect {
		HeatHazeEffect {
			component_base: ComponentBase::new(),
			timer: 0.0,
			speed: 2.0,
			amplitude: 3.0,
			frequency: 0.05,
			capture_texture: None,
			capture_texture_width: 0,
			capture_texture_height: 0,
		}
	}

	//==============================================================================
	/// 속도 설정.
	//==============================================================================
	pub fn set_speed(&mut self, value: f32) {
		self.speed = value;
	}

	//==============================================================================
	/// 속도 반환.
	//==============================================================================
	pub fn get_speed(&self) -> f32 {
		self.speed
	}

	//==============================================================================
	/// 강도 설정.
	//==============================================================================
	pub fn set_amplitude(&mut self, value: f32) {
		self.amplitude = value;
	}

	//==============================================================================
	/// 강도 반환.
	//==============================================================================
	pub fn get_amplitude(&self) -> f32 {
		self.amplitude
	}

	//==============================================================================
	/// 빈도 설정.
	//==============================================================================
	pub fn set_frequency(&mut self, value: f32) {
		self.frequency = value;
	}

	//==============================================================================
	/// 빈도 반환.
	//==============================================================================
	pub fn get_frequency(&self) -> f32 {
		self.frequency
	}

	//==============================================================================
	/// 흔들림 시계 반환. (tick 마다 time_delta * 속도씩 흐른다)
	//==============================================================================
	pub fn get_timer(&self) -> f32 {
		self.timer
	}

	//==============================================================================
	/// 줄의 좌우 흔들림. (sin(시계 + 줄 위치 * 빈도) * 폭)
	//==============================================================================
	pub fn calculate_row_offset(&self, local_y: f32) -> f32 {
		(self.timer + local_y * self.frequency).sin() * self.amplitude
	}

	//==============================================================================
	/// GL 자원 해제. (떠 온 텍스처)
	//==============================================================================
	pub fn destroy(&mut self, graphic: &mut Graphic) {
		if let Some(capture_texture) = self.capture_texture.take() {
			unsafe {
				graphic.get_gl().delete_texture(capture_texture);
			}
		}
		self.capture_texture_width = 0;
		self.capture_texture_height = 0;
	}

	//==============================================================================
	/// 떠 올 텍스처 준비. (크기가 바뀌면 다시 잡는다. 실패하면 None)
	//==============================================================================
	fn ensure_capture_texture(&mut self, graphic: &Graphic, width: i32, height: i32) -> Option<GlTexture> {
		let gl = graphic.get_gl();
		if let Some(capture_texture) = self.capture_texture {
			if self.capture_texture_width == width && self.capture_texture_height == height {
				return Some(capture_texture);
			}
			unsafe {
				gl.delete_texture(capture_texture);
			}
			self.capture_texture = None;
		}
		let capture_texture = unsafe {
			let capture_texture = match gl.create_texture() {
				Ok(capture_texture) => {
					capture_texture
				}
				Err(error) => {
					eprintln!("HeatHazeEffect: {}", error);
					return None;
				}
			};
			gl.bind_texture(glow::TEXTURE_2D, Some(capture_texture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGB8 as i32, width, height, 0, glow::RGB, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			capture_texture
		};
		self.capture_texture = Some(capture_texture);
		self.capture_texture_width = width;
		self.capture_texture_height = height;
		Some(capture_texture)
	}
}

impl Default for HeatHazeEffect {
	fn default() -> HeatHazeEffect {
		HeatHazeEffect::new()
	}
}

#[crate::reflect]
impl Component for HeatHazeEffect {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"HeatHazeEffect"
	}

	//==============================================================================
	/// 붙을 때. (노드 content_size 가 비어 있으면 300 x 300)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 && content_size.y <= 0.0 {
			transform.set_content_size(Vector2::create(DEFAULT_CONTENT_SIZE, DEFAULT_CONTENT_SIZE));
		}
	}

	//==============================================================================
	/// 갱신. (흔들림 시계를 흘린다)
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f32) {
		self.timer += time_delta * self.speed;
	}

	//==============================================================================
	/// 출력. (지금까지 그려진 화면의 노드 영역을 떠 와서 줄마다 흔들어 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}
		// 앞서 그린 것이 화면에 다 나가야 떠 올 수 있다.
		graphic.flush_batch();

		// 노드 영역의 화면 바운딩 박스. (드로잉 버퍼 픽셀, 왼쪽 위 원점. 원본 get_world_bounds)
		let transform_matrix = graphic.get_transform_matrix();
		let corners = [
			transform_matrix.transform_point(0.0, 0.0),
			transform_matrix.transform_point(content_size.x, 0.0),
			transform_matrix.transform_point(0.0, content_size.y),
			transform_matrix.transform_point(content_size.x, content_size.y),
		];
		let minimum_x = corners.iter().map(|corner| corner.x).fold(f32::INFINITY, f32::min);
		let maximum_x = corners.iter().map(|corner| corner.x).fold(f32::NEG_INFINITY, f32::max);
		let minimum_y = corners.iter().map(|corner| corner.y).fold(f32::INFINITY, f32::min);
		let maximum_y = corners.iter().map(|corner| corner.y).fold(f32::NEG_INFINITY, f32::max);
		let bounds_width = maximum_x - minimum_x;
		let bounds_height = maximum_y - minimum_y;
		if bounds_width <= 0.0 || bounds_height <= 0.0 {
			return;
		}

		// 화면 안으로 자른 복사 영역.
		let buffer_width = graphic.get_drawing_buffer_width();
		let buffer_height = graphic.get_drawing_buffer_height();
		let region_x = (minimum_x.floor() as i32).max(0);
		let region_y = (minimum_y.floor() as i32).max(0);
		let region_right = (maximum_x.ceil() as i32).min(buffer_width);
		let region_bottom = (maximum_y.ceil() as i32).min(buffer_height);
		let region_width = region_right - region_x;
		let region_height = region_bottom - region_y;
		if region_width <= 0 || region_height <= 0 {
			return;
		}

		// 떠 오기. (GL 은 아래가 0 이라 텍스처 0 줄이 영역 아래쪽이다)
		let capture_texture = match self.ensure_capture_texture(graphic, region_width, region_height) {
			Some(capture_texture) => {
				capture_texture
			}
			None => {
				return;
			}
		};
		unsafe {
			let gl = graphic.get_gl();
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, Some(capture_texture));
			gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, region_x, buffer_height - region_bottom, region_width, region_height);
		}

		// 줄마다 쿼드. (x, y, u, v, r, g, b, a)
		let row_count = (content_size.y.ceil() as usize).max(1);
		let row_height = content_size.y / row_count as f32;
		let left_u = (minimum_x - region_x as f32) / region_width as f32;
		let right_u = (maximum_x - region_x as f32) / region_width as f32;
		let mut vertex_values: Vec<f32> = Vec::with_capacity(row_count * 6 * PARTICLE_FLOATS_PER_VERTEX);
		for row_index in 0..row_count {
			let top_y = row_index as f32 * row_height;
			let bottom_y = top_y + row_height;
			let offset_x = self.calculate_row_offset(top_y);
			let source_top_y = minimum_y + top_y / content_size.y * bounds_height;
			let source_bottom_y = minimum_y + bottom_y / content_size.y * bounds_height;
			let top_v = 1.0 - (source_top_y - region_y as f32) / region_height as f32;
			let bottom_v = 1.0 - (source_bottom_y - region_y as f32) / region_height as f32;
			let left_x = offset_x;
			let right_x = offset_x + content_size.x;
			let corners: [(f32, f32, f32, f32); 4] = [
				(left_x, top_y, left_u, top_v),
				(right_x, top_y, right_u, top_v),
				(left_x, bottom_y, left_u, bottom_v),
				(right_x, bottom_y, right_u, bottom_v),
			];
			for corner_index in [0usize, 1, 2, 2, 1, 3] {
				let (corner_x, corner_y, corner_u, corner_v) = corners[corner_index];
				vertex_values.extend_from_slice(&[corner_x, corner_y, corner_u, corner_v, 1.0, 1.0, 1.0, 1.0]);
			}
		}
		draw_colored_quads_in_chunks(graphic, &vertex_values, capture_texture);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 붙으면 빈 노드를 300 x 300 으로 정하고, 이미 크기가 있으면 두며, 시계는 속도만큼 흐르고 줄 흔들림은 사인 곡선이다.
	//==============================================================================
	#[test]
	fn attach_size_and_row_offset() {
		let mut empty_transform = NodeTransform::new();
		let mut heat_haze_effect = HeatHazeEffect::new();
		heat_haze_effect.attach(&mut empty_transform);
		assert_eq!(empty_transform.get_content_size(), Vector2::create(300.0, 300.0));
		let mut sized_transform = NodeTransform::new();
		sized_transform.set_content_size(Vector2::create(64.0, 32.0));
		heat_haze_effect.attach(&mut sized_transform);
		assert_eq!(sized_transform.get_content_size(), Vector2::create(64.0, 32.0));

		heat_haze_effect.set_speed(4.0);
		heat_haze_effect.set_amplitude(5.0);
		heat_haze_effect.set_frequency(0.1);
		heat_haze_effect.timer += 0.25 * heat_haze_effect.get_speed();
		assert_eq!(heat_haze_effect.get_timer(), 1.0);
		assert!((heat_haze_effect.calculate_row_offset(10.0) - 2.0f32.sin() * 5.0).abs() < 0.00001);
	}
}
