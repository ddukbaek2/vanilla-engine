//==============================================================================
//! 아지랑이 효과 컴포넌트. (원본 effect/heathazeeffect.js 의 HeatHazeEffect)
//! - 이 노드가 그려지기 전까지 화면에 그려진 것을 노드 영역만큼 떠 와서, 한 줄씩 사인 곡선을 따라 좌우로 흔들어 다시 그린다.
//!   그래서 씬의 맨 위(마지막)에 그려지는 노드에 붙여야 보인다. 자식 노드는 효과 위에 그려진다.
//! - 원본처럼 묶음을 내보낸 뒤 노드의 화면 영역(픽셀)을 copyTexSubImage2D 로 텍스처에 떠 오고, 줄마다 쿼드 한 장씩 정점 색 쿼드(draw_colored_quads)로 그린다.
//!   노드 영역이 화면 밖으로 나가면 화면 가장자리 픽셀을 늘인다. (CLAMP_TO_EDGE)
//! - 원본처럼 노드에 붙이는 컴포넌트이고, 붙을 때 노드 content_size 가 비어 있으면 300 x 300 으로 정한다.
//! - 원본과 다른 점:
//!   - 떠 온 텍스처는 GL 자원이라 다 쓰면 destroy() 를 부른다. 원본처럼 텍스처를 만든 컨텍스트(약한 손잡이)를 쥐어 인자가 없고,
//!     컴포넌트가 버려질 때(Drop, 노드를 지우거나 씬을 내릴 때)도 스스로 지운다. (ScreenEffect 와 같다)
//!   - 줄이 한 번에 그릴 수 있는 정점 수(Graphic 한도)를 넘으면 나눠 그린다. 원본은 배열을 늘려 한 번에 그린다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{GlTexture, Graphic, PARTICLE_FLOATS_PER_VERTEX};
use crate::effect::particlesystem::draw_colored_quads_in_chunks;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use glow::HasContext;
use std::rc::Weak;

/// 붙을 때 content_size 가 비어 있으면 쓰는 크기. (원본 생성자)
const DEFAULT_CONTENT_SIZE: f64 = 300.0;

//==============================================================================
/// 아지랑이 효과 컴포넌트.
/// - 맨 마지막에 그려지는 노드에 붙인다. 그 노드 영역에 앞서 그려진 화면을 줄마다 sin(시계 + 줄 y x 빈도) x 폭만큼 좌우로 민다.
///
/// # 예제
/// 노드에 붙여 폭과 빈도 정하기
/// ```rust
/// use vanilla::{HeatHazeEffect, NodeTree, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let haze_id = node_tree.create_child(root_id, "HeatHaze");
/// let heat_haze_effect = node_tree.add_component(haze_id, HeatHazeEffect::new()).expect("heat haze");
/// heat_haze_effect.set_amplitude(6.0);
/// heat_haze_effect.set_frequency(0.08);
/// assert_eq!(heat_haze_effect.calculate_row_offset(0.0), 0.0);
/// assert!(heat_haze_effect.calculate_row_offset(10.0).abs() <= 6.0);
/// assert_eq!(node_tree[haze_id].get_content_size(), Vector2::create(300.0, 300.0));
/// ```
//==============================================================================
pub struct HeatHazeEffect {
	component_base: ComponentBase,
	timer: f64,
	/// 흔들리는 속도.
	speed: f64,
	/// 흔들리는 폭. (강도, 픽셀)
	amplitude: f64,
	/// 흔들리는 빈도. (물결의 촘촘함)
	frequency: f64,
	capture_texture: Option<GlTexture>,
	capture_texture_width: i32,
	capture_texture_height: i32,
	/// 떠 온 텍스처를 만든 컨텍스트의 약한 손잡이. (destroy 와 Drop 이 쓴다)
	gl_context: Weak<glow::Context>,
}

#[crate::reflect]
impl HeatHazeEffect {
	//==============================================================================
	/// 생성. (속도 2, 폭 3, 빈도 0.05)
	///
	/// # 돌려줌
	/// 시계가 0 이고 떠 온 텍스처가 없는 효과.
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
			gl_context: Weak::new(),
		}
	}

	//==============================================================================
	/// 속도 설정.
	///
	/// # 인자
	/// - `value`: 흔들림 시계가 흐르는 빠르기. (초당 라디안, tick 마다 time_delta x 속도만큼 흐른다. 기본 2)
	//==============================================================================
	pub fn set_speed(&mut self, value: f64) {
		self.speed = value;
	}

	//==============================================================================
	/// 속도 반환.
	///
	/// # 돌려줌
	/// 흔들림 시계가 흐르는 빠르기. (초당 라디안)
	//==============================================================================
	pub fn get_speed(&self) -> f64 {
		self.speed
	}

	//==============================================================================
	/// 강도 설정.
	///
	/// # 인자
	/// - `value`: 줄이 좌우로 밀리는 가장 큰 폭. (노드 좌표 픽셀, 기본 3)
	//==============================================================================
	pub fn set_amplitude(&mut self, value: f64) {
		self.amplitude = value;
	}

	//==============================================================================
	/// 강도 반환.
	///
	/// # 돌려줌
	/// 줄이 좌우로 밀리는 가장 큰 폭. (노드 좌표 픽셀)
	//==============================================================================
	pub fn get_amplitude(&self) -> f64 {
		self.amplitude
	}

	//==============================================================================
	/// 빈도 설정.
	///
	/// # 인자
	/// - `value`: 줄 위치에 곱하는 빈도. (노드 세로 1 픽셀마다 라디안, 클수록 물결이 촘촘하다. 기본 0.05)
	//==============================================================================
	pub fn set_frequency(&mut self, value: f64) {
		self.frequency = value;
	}

	//==============================================================================
	/// 빈도 반환.
	///
	/// # 돌려줌
	/// 줄 위치에 곱하는 빈도. (노드 세로 1 픽셀마다 라디안)
	//==============================================================================
	pub fn get_frequency(&self) -> f64 {
		self.frequency
	}

	//==============================================================================
	/// 흔들림 시계 반환. (tick 마다 time_delta * 속도씩 흐른다)
	///
	/// # 돌려줌
	/// 흔들림 시계. (라디안, 처음 0)
	//==============================================================================
	pub fn get_timer(&self) -> f64 {
		self.timer
	}

	//==============================================================================
	/// 줄의 좌우 흔들림. (sin(시계 + 줄 위치 * 빈도) * 폭)
	///
	/// # 인자
	/// - `local_y`: 줄의 노드 로컬 y. (노드 위가 0)
	///
	/// # 돌려줌
	/// 줄을 밀 x 거리. (노드 좌표, -폭 ~ 폭)
	//==============================================================================
	pub fn calculate_row_offset(&self, local_y: f64) -> f64 {
		(self.timer + local_y * self.frequency).sin() * self.amplitude
	}

	//==============================================================================
	/// GL 자원 해제. (떠 온 텍스처, 원본 destroy)
	/// - 버려질 때(Drop)도 스스로 부른다. GL 컨텍스트가 이미 사라졌으면 텍스처도 같이 사라졌으므로 잊기만 한다.
	//==============================================================================
	pub fn destroy(&mut self) {
		if let Some(capture_texture) = self.capture_texture.take() {
			if let Some(gl) = self.gl_context.upgrade() {
				unsafe {
					crate::core::glstatecache::delete_texture(&gl, capture_texture);
				}
			}
		}
		self.capture_texture_width = 0;
		self.capture_texture_height = 0;
	}

	//==============================================================================
	/// 떠 올 텍스처 준비. (크기가 바뀌면 다시 잡는다. 실패하면 None)
	//==============================================================================
	fn ensure_capture_texture(&mut self, gl: &glow::Context, width: i32, height: i32) -> Option<GlTexture> {
		if let Some(capture_texture) = self.capture_texture {
			if self.capture_texture_width == width && self.capture_texture_height == height {
				return Some(capture_texture);
			}
			unsafe {
				crate::core::glstatecache::delete_texture(gl, capture_texture);
			}
			self.capture_texture = None;
		}
		let capture_texture = unsafe {
			let capture_texture = match crate::core::glstatecache::create_texture(gl) {
				Ok(capture_texture) => {
					capture_texture
				}
				Err(error) => {
					eprintln!("HeatHazeEffect: {}", error);
					return None;
				}
			};
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(capture_texture));
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
		self.gl_context = crate::core::imagetexturecache::find_gl_context_handle(gl).unwrap_or_default();
		Some(capture_texture)
	}
}

impl Default for HeatHazeEffect {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> HeatHazeEffect {
		HeatHazeEffect::new()
	}
}

impl Drop for HeatHazeEffect {
	//==============================================================================
	/// 버려질 때 GL 자원 해제. (destroy)
	//==============================================================================
	fn drop(&mut self) {
		self.destroy();
	}
}

#[crate::reflect]
impl Component for HeatHazeEffect {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("HeatHazeEffect")
	//==============================================================================
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
	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f64) {
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

		// 노드 영역의 화면 바운딩 박스. (드로잉 버퍼 픽셀, 왼쪽 위 원점. 원본 getWorldBounds)
		let transform_matrix = graphic.get_transform_matrix();
		let corners = [
			transform_matrix.transform_point(0.0, 0.0),
			transform_matrix.transform_point(content_size.x, 0.0),
			transform_matrix.transform_point(0.0, content_size.y),
			transform_matrix.transform_point(content_size.x, content_size.y),
		];
		let minimum_x = corners.iter().map(|corner| corner.x).fold(f64::INFINITY, f64::min);
		let maximum_x = corners.iter().map(|corner| corner.x).fold(f64::NEG_INFINITY, f64::max);
		let minimum_y = corners.iter().map(|corner| corner.y).fold(f64::INFINITY, f64::min);
		let maximum_y = corners.iter().map(|corner| corner.y).fold(f64::NEG_INFINITY, f64::max);
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
		let capture_texture = match self.ensure_capture_texture(graphic.get_tracked_gl(), region_width, region_height) {
			Some(capture_texture) => {
				capture_texture
			}
			None => {
				return;
			}
		};
		unsafe {
			let gl = graphic.get_tracked_gl();
			crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(capture_texture));
			gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, region_x, buffer_height - region_bottom, region_width, region_height);
		}

		// 줄마다 쿼드. (x, y, u, v, r, g, b, a)
		let row_count = (content_size.y.ceil() as usize).max(1);
		let row_height = content_size.y / row_count as f64;
		let left_u = (minimum_x - region_x as f64) / region_width as f64;
		let right_u = (maximum_x - region_x as f64) / region_width as f64;
		let mut vertex_values: Vec<f32> = Vec::with_capacity(row_count * 6 * PARTICLE_FLOATS_PER_VERTEX);
		for row_index in 0..row_count {
			let top_y = row_index as f64 * row_height;
			let bottom_y = top_y + row_height;
			let offset_x = self.calculate_row_offset(top_y);
			let source_top_y = minimum_y + top_y / content_size.y * bounds_height;
			let source_bottom_y = minimum_y + bottom_y / content_size.y * bounds_height;
			let top_v = 1.0 - (source_top_y - region_y as f64) / region_height as f64;
			let bottom_v = 1.0 - (source_bottom_y - region_y as f64) / region_height as f64;
			let left_x = offset_x;
			let right_x = offset_x + content_size.x;
			let corners: [(f64, f64, f64, f64); 4] = [
				(left_x, top_y, left_u, top_v),
				(right_x, top_y, right_u, top_v),
				(left_x, bottom_y, left_u, bottom_v),
				(right_x, bottom_y, right_u, bottom_v),
			];
			for corner_index in [0usize, 1, 2, 2, 1, 3] {
				let (corner_x, corner_y, corner_u, corner_v) = corners[corner_index];
				vertex_values.extend_from_slice(&[corner_x as f32, corner_y as f32, corner_u as f32, corner_v as f32, 1.0, 1.0, 1.0, 1.0]);
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
		assert!((heat_haze_effect.calculate_row_offset(10.0) - 2.0f64.sin() * 5.0).abs() < 0.00001);
	}

	//==============================================================================
	/// 떠 온 텍스처는 destroy() 와 버려질 때(Drop) 지우고, 크기가 바뀌면 바꿔 잡는다. 컨텍스트가 먼저 사라져도 패닉 없이 잊는다. (가짜 GL)
	//==============================================================================
	#[test]
	fn capture_texture_is_released() {
		use crate::core::imagetexturecache::fake_gl::{self, FakeGlObject};
		let gl = fake_gl::create_context();
		crate::core::imagetexturecache::register_pixel_read_context(&gl);
		let mut heat_haze_effect = HeatHazeEffect::new();
		assert!(heat_haze_effect.ensure_capture_texture(&gl, 32, 16).is_some());
		assert!(heat_haze_effect.ensure_capture_texture(&gl, 64, 16).is_some());
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Texture), 1);
		heat_haze_effect.destroy();
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Texture), 0);
		assert!(heat_haze_effect.ensure_capture_texture(&gl, 32, 16).is_some());
		drop(heat_haze_effect);
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Texture), 0);
		let mut orphan_effect = HeatHazeEffect::new();
		assert!(orphan_effect.ensure_capture_texture(&gl, 8, 8).is_some());
		drop(gl);
		drop(orphan_effect);
	}
}
