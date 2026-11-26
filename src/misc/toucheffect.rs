//==============================================================================
//! 터치 효과 컴포넌트.
//! - 원본 misc/toucheffect.js 를 옮긴 것이다. 누른 자리에 하늘색 빛 입자를 흩뿌리고 0.4 초 동안 흐려지며 퍼지게 한다.
//! - 입자는 흰색 방사형 그라디언트 그림(64x64, 한 번만 만든다)을 하늘색(rgb 100, 200, 255)으로 틴트하고 더하기(lighter) 블렌드로 그린다.
//! - 사용:
//!   ```text
//!     let touch_effect_id = node_tree.create_child(root_id, "TouchEffect");
//!     node_tree[touch_effect_id].set_pivot(Pivot::TOP_LEFT);
//!     node_tree[touch_effect_id].add_component(TouchEffect::new());
//!     // 누를 때: node_tree[touch_effect_id].get_component_mut::<TouchEffect>().unwrap().create_touch_particle(x, y);
//!   ```
//! - 원본과 다른 점:
//!   원본은 WorldNode 를 상속한 노드이고, 러스트 판은 노드에 붙는 컴포넌트다. 블렌드는 원본처럼 push_transform 에서 더하기로 바꾸고
//!   pop_transform 에서 앞의 블렌드로 되돌려 자식까지 더하기로 그린다. (컴포넌트의 노드 출력 상태 앞뒤 훅)
//!   그라디언트는 원본처럼 오프스크린 캔버스(엔진 Canvas2D)의 방사 그라디언트로 구운 Image 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::canvas2d::Canvas2D;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic};
use crate::core::image::Image;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use std::cell::RefCell;
use std::rc::Rc;

/// 입자용 방사형 그라디언트 그림 크기. (픽셀)
const PARTICLE_GRADIENT_IMAGE_SIZE: u32 = 64;

thread_local! {
	/// 입자용 방사형 그라디언트 그림. (처음 그릴 때 한 번 만든다. 원본 particleGradientCanvas)
	static PARTICLE_GRADIENT_IMAGE: RefCell<Option<Rc<Image>>> = const { RefCell::new(None) };
}

//==============================================================================
/// 방사형 그라디언트 그림 반환. (가운데 불투명 흰색 → 가장자리 투명. 원본처럼 엔진 Canvas2D 의 방사 그라디언트로 한 번 굽는다)
//==============================================================================
fn get_particle_gradient_image() -> Rc<Image> {
	PARTICLE_GRADIENT_IMAGE.with(|image_cell| {
		let mut image_option = image_cell.borrow_mut();
		let gradient_image = image_option.get_or_insert_with(|| {
			let canvas_size = PARTICLE_GRADIENT_IMAGE_SIZE as f64;
			let half_size = canvas_size * 0.5;
			let mut canvas = Canvas2D::new(PARTICLE_GRADIENT_IMAGE_SIZE, PARTICLE_GRADIENT_IMAGE_SIZE);
			if let Some(mut radial_gradient) = canvas.create_radial_gradient(half_size as f32, half_size as f32, 0.0, half_size as f32, half_size as f32, half_size as f32) {
				radial_gradient.add_color_stop(0.0, Color::create(1.0, 1.0, 1.0, 1.0));
				radial_gradient.add_color_stop(1.0, Color::create(1.0, 1.0, 1.0, 0.0));
				canvas.set_fill_style(radial_gradient);
			}
			canvas.fill_rect(0.0, 0.0, canvas_size as f32, canvas_size as f32);
			canvas.to_image()
		});
		gradient_image.clone()
	})
}

//==============================================================================
/// 터치 입자.
/// - TouchEffect 가 목록으로 든다. 값만 담는 구조체다.
///
/// # 예제
/// 입자 하나의 값
/// ```rust
/// use vanilla::{TouchParticle, Vector2};
///
/// let mut particle = TouchParticle::new();
/// particle.position = Vector2::create(40.0, 30.0);
/// particle.radius = 20.0;
/// assert_eq!(particle.life, particle.max_life);
/// assert_eq!(particle.velocity, Vector2::zero());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchParticle {
	/// 가운데 위치. (노드 영역 좌표, 픽셀)
	pub position: Vector2,
	/// 속도. (픽셀/초)
	pub velocity: Vector2,
	/// 남은 수명. (TouchEffect 에서 1 로 시작해 초당 2.5 씩 줄고, 0 이하면 빠진다)
	pub life: f64,
	/// 처음 수명. (그릴 때 투명도는 life / max_life)
	pub max_life: f64,
	/// 반지름. (픽셀)
	pub radius: f64,
}

impl TouchParticle {
	//==============================================================================
	/// 생성. (제자리, 수명 1, 반지름 0)
	///
	/// # 돌려줌
	/// 위치와 속도가 0, 수명과 처음 수명이 1, 반지름이 0 인 입자.
	//==============================================================================
	pub fn new() -> TouchParticle {
		TouchParticle {
			position: Vector2::zero(),
			velocity: Vector2::zero(),
			life: 1.0,
			max_life: 1.0,
			radius: 0.0,
		}
	}
}

impl Default for TouchParticle {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TouchParticle {
		TouchParticle::new()
	}
}

//==============================================================================
/// 터치 효과 컴포넌트.
/// - 누를 때 create_touch_particle 을 부르면 tick 에서 입자를 줄이고 draw 에서 더하기 블렌드로 그린다.
///
/// # 예제
/// 노드에 붙이고 누른 자리에 입자 만들기
/// ```rust
/// use vanilla::{NodeTree, Pivot, TouchEffect};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let touch_effect_id = node_tree.create_child(root_id, "TouchEffect");
/// node_tree[touch_effect_id].set_pivot(Pivot::TOP_LEFT);
/// node_tree[touch_effect_id].add_component(TouchEffect::new());
///
/// let touch_effect = node_tree[touch_effect_id].get_component_mut::<TouchEffect>().unwrap();
/// touch_effect.create_touch_particle(120.0, 80.0);
/// assert_eq!(touch_effect.get_particle_count(), 1);
/// touch_effect.update_touch_particles(0.5);
/// assert_eq!(touch_effect.get_particle_count(), 0);
/// ```
//==============================================================================
pub struct TouchEffect {
	component_base: ComponentBase,
	touch_particles: Vec<TouchParticle>,
	/// push_transform 앞의 블렌드. (pop_transform 에서 되돌린다)
	original_blend_mode: BlendMode,
}

#[crate::reflect]
impl TouchEffect {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 입자가 없는 새 터치 효과.
	//==============================================================================
	pub fn new() -> TouchEffect {
		TouchEffect {
			component_base: ComponentBase::new(),
			touch_particles: Vec::new(),
			original_blend_mode: BlendMode::Normal,
		}
	}

	//==============================================================================
	/// 터치 입자 만들기. (노드 영역 좌표, ±5 픽셀 흩뿌림, ±60 픽셀/초 속도, 반지름 10 ~ 35)
	///
	/// # 인자
	/// - `x`: 누른 자리 x. (노드 영역 좌표, 픽셀)
	/// - `y`: 누른 자리 y. (노드 영역 좌표, 픽셀)
	//==============================================================================
	pub fn create_touch_particle(&mut self, x: f64, y: f64) {
		let mut particle = TouchParticle::new();
		particle.position = Vector2::create(x + (math::random() - 0.5) * 10.0, y + (math::random() - 0.5) * 10.0);
		particle.velocity = Vector2::create((math::random() - 0.5) * 120.0, (math::random() - 0.5) * 120.0);
		particle.life = 1.0;
		particle.max_life = 1.0;
		particle.radius = math::random() * 25.0 + 10.0;
		self.touch_particles.push(particle);
	}

	//==============================================================================
	/// 터치 입자 갱신. (수명이 초당 2.5 씩 줄고, 다 되면 뺀다)
	///
	/// # 인자
	/// - `time_delta`: 지난 시간. (초, 위치는 속도 * time_delta 만큼 움직인다)
	//==============================================================================
	pub fn update_touch_particles(&mut self, time_delta: f64) {
		let mut particle_index = self.touch_particles.len();
		while particle_index > 0 {
			particle_index -= 1;
			let particle = &mut self.touch_particles[particle_index];
			particle.life -= time_delta * 2.5;
			particle.position.x += particle.velocity.x * time_delta;
			particle.position.y += particle.velocity.y * time_delta;
			if particle.life <= 0.0 {
				self.touch_particles.remove(particle_index);
			}
		}
	}

	//==============================================================================
	/// 터치 입자 출력. (흰색 그라디언트를 입자 색으로 틴트하고, 남은 수명만큼 흐리게)
	/// - 블렌드는 바꾸지 않는다. (컴포넌트로 그릴 때는 push_transform 이 더하기로 바꾼다)
	///
	/// # 인자
	/// - `graphic`: 그릴 그래픽. (이미지 틴트와 전역 알파는 그린 뒤 되돌린다)
	//==============================================================================
	pub fn draw_touch_particles(&self, graphic: &mut Graphic) {
		if self.touch_particles.is_empty() {
			return;
		}
		let gradient_image = get_particle_gradient_image();
		let particle_color = Color::create_from_rgba("rgb(100, 200, 255)");
		let original_alpha = graphic.get_global_alpha();
		graphic.set_image_tint_color(Some(particle_color));
		for particle in self.touch_particles.iter() {
			let opacity = (particle.life / particle.max_life).max(0.0);
			graphic.set_global_alpha(original_alpha * opacity * 0.8);
			let draw_position = Vector2::create(particle.position.x - particle.radius, particle.position.y - particle.radius);
			let draw_size = Vector2::create(particle.radius * 2.0, particle.radius * 2.0);
			graphic.draw_image(&gradient_image, draw_position, draw_size);
		}
		graphic.set_image_tint_color(None);
		graphic.set_global_alpha(original_alpha);
	}

	//==============================================================================
	/// 살아 있는 입자 수. (러스트 판에서 더한 것)
	///
	/// # 돌려줌
	/// 수명이 남은 입자 수.
	//==============================================================================
	pub fn get_particle_count(&self) -> usize {
		self.touch_particles.len()
	}
}

impl Default for TouchEffect {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TouchEffect {
		TouchEffect::new()
	}
}

#[crate::reflect]
impl Component for TouchEffect {
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
	/// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"TouchEffect"
	}

	//==============================================================================
	/// 갱신.
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f64) {
		self.update_touch_particles(time_delta);
	}

	//==============================================================================
	/// 출력 상태 시작. (앞의 블렌드를 기억하고 더하기로, 이 노드와 자식이 모두 더하기로 그려진다)
	//==============================================================================
	fn push_transform(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		self.original_blend_mode = graphic.get_blend_mode();
		graphic.set_blend_mode(BlendMode::Lighter);
	}

	//==============================================================================
	/// 출력.
	//==============================================================================
	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_touch_particles(graphic);
	}

	//==============================================================================
	/// 출력 상태 종료. (앞의 블렌드로 되돌린다)
	//==============================================================================
	fn pop_transform(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		graphic.set_blend_mode(self.original_blend_mode);
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 입자는 흩뿌린 범위 안에서 생기고, 0.4 초 뒤 사라진다.
	//==============================================================================
	#[test]
	fn particles_spawn_and_expire() {
		let mut touch_effect = TouchEffect::new();
		for _ in 0..10 {
			touch_effect.create_touch_particle(100.0, 50.0);
		}
		assert_eq!(touch_effect.get_particle_count(), 10);
		for particle in touch_effect.touch_particles.iter() {
			assert!((particle.position.x - 100.0).abs() <= 5.0);
			assert!((particle.position.y - 50.0).abs() <= 5.0);
			assert!(particle.radius >= 10.0 && particle.radius < 35.0);
		}
		touch_effect.update_touch_particles(0.2);
		assert_eq!(touch_effect.get_particle_count(), 10);
		assert!((touch_effect.touch_particles[0].life - 0.5).abs() < 0.0001);
		touch_effect.update_touch_particles(0.2);
		assert_eq!(touch_effect.get_particle_count(), 0);
	}

	//==============================================================================
	/// 그라디언트 그림은 가운데가 불투명하고 모서리는 투명하며, 한 번만 만든다.
	/// - 헤드리스 크롬의 같은 캔버스 코드(createRadialGradient, fillRect)와 64x64 알파가 모두 같았다. (2026-11-27, 색은 흰색이라 곱한 값이 알파와 같다)
	//==============================================================================
	#[test]
	fn gradient_image_is_radial() {
		let gradient_image = get_particle_gradient_image();
		assert_eq!(gradient_image.get_width(), PARTICLE_GRADIENT_IMAGE_SIZE);
		let pixels = gradient_image.get_pixels();
		let row_stride = PARTICLE_GRADIENT_IMAGE_SIZE as usize * 4;
		let center_index = 32 * row_stride + 32 * 4;
		assert!(pixels[center_index + 3] >= 245);
		assert_eq!(pixels[3], 0);
		// 크롬이 같은 캔버스 코드로 낸 가운데 줄 알파. (x = 0, 16, 48, 63)
		let middle_row_alphas: Vec<u8> = [0usize, 16, 48, 63].iter().map(|pixel_x| pixels[32 * row_stride + pixel_x * 4 + 3]).collect();
		assert_eq!(middle_row_alphas, vec![4, 131, 123, 4]);
		assert!(Rc::ptr_eq(&gradient_image, &get_particle_gradient_image()));
	}
}
