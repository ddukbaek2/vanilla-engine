//==============================================================================
// 터치 효과 컴포넌트.
// - 원본 misc/toucheffect.js 를 옮긴 것이다. 누른 자리에 하늘색 빛 입자를 흩뿌리고 0.4 초 동안 흐려지며 퍼지게 한다.
// - 입자는 흰색 방사형 그라디언트 그림(64x64, 한 번만 만든다)을 하늘색(rgb 100, 200, 255)으로 틴트하고 더하기(lighter) 블렌드로 그린다.
// - 사용:
//     let touchEffectId = nodeTree.createChild(rootId, "TouchEffect");
//     nodeTree[touchEffectId].setPivot(Pivot::topLeft);
//     nodeTree[touchEffectId].addComponent(TouchEffect::new());
//     // 누를 때: nodeTree[touchEffectId].getComponentMut::<TouchEffect>().unwrap().createTouchParticle(x, y);
// - 원본과 다른 점:
//   원본은 WorldNode 를 상속한 노드이고 pushTransform / popTransform 에서 블렌드를 바꿔 자식까지 더하기로 그린다.
//   러스트 판은 노드에 붙는 컴포넌트라 자기 draw 동안만 블렌드를 바꾸고, 끝나면 앞의 블렌드로 되돌린다. (자식은 원래 블렌드로 그린다)
//   그라디언트는 오프스크린 캔버스 대신 픽셀로 직접 만든 Image 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic};
use crate::core::image::Image;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use std::cell::RefCell;
use std::rc::Rc;

// 입자용 방사형 그라디언트 그림 크기. (픽셀)
const PARTICLE_GRADIENT_IMAGE_SIZE: u32 = 64;

thread_local! {
	// 입자용 방사형 그라디언트 그림. (처음 그릴 때 한 번 만든다. 원본 particleGradientCanvas)
	static PARTICLE_GRADIENT_IMAGE: RefCell<Option<Rc<Image>>> = const { RefCell::new(None) };
}

//==============================================================================
// 방사형 그라디언트 그림 반환. (가운데 불투명 흰색 → 가장자리 투명, 직선 보간. 없으면 한 번 만든다)
//==============================================================================
fn getParticleGradientImage() -> Rc<Image> {
	PARTICLE_GRADIENT_IMAGE.with(|imageCell| {
		let mut imageOption = imageCell.borrow_mut();
		let gradientImage = imageOption.get_or_insert_with(|| {
			let imageSize = PARTICLE_GRADIENT_IMAGE_SIZE as usize;
			let halfSize = imageSize as f32 * 0.5;
			let mut pixels = vec![0u8; imageSize * imageSize * 4];
			for pixelY in 0..imageSize {
				for pixelX in 0..imageSize {
					let deltaX = pixelX as f32 + 0.5 - halfSize;
					let deltaY = pixelY as f32 + 0.5 - halfSize;
					let distanceRatio = (deltaX * deltaX + deltaY * deltaY).sqrt() / halfSize;
					let alpha = math::clamp01(1.0 - distanceRatio);
					let pixelIndex = (pixelY * imageSize + pixelX) * 4;
					pixels[pixelIndex] = 255;
					pixels[pixelIndex + 1] = 255;
					pixels[pixelIndex + 2] = 255;
					pixels[pixelIndex + 3] = (alpha * 255.0).round() as u8;
				}
			}
			Image::createFromPixels(PARTICLE_GRADIENT_IMAGE_SIZE, PARTICLE_GRADIENT_IMAGE_SIZE, pixels)
		});
		gradientImage.clone()
	})
}

//==============================================================================
// 터치 입자.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchParticle {
	pub position: Vector2,
	pub velocity: Vector2,
	pub life: f32,
	pub maxLife: f32,
	pub radius: f32,
}

impl TouchParticle {
	//==============================================================================
	// 생성. (제자리, 수명 1, 반지름 0)
	//==============================================================================
	pub fn new() -> TouchParticle {
		TouchParticle {
			position: Vector2::zero(),
			velocity: Vector2::zero(),
			life: 1.0,
			maxLife: 1.0,
			radius: 0.0,
		}
	}
}

impl Default for TouchParticle {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TouchParticle {
		TouchParticle::new()
	}
}

//==============================================================================
// 터치 효과 컴포넌트.
//==============================================================================
pub struct TouchEffect {
	componentBase: ComponentBase,
	touchParticles: Vec<TouchParticle>,
}

impl TouchEffect {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TouchEffect {
		TouchEffect {
			componentBase: ComponentBase::new(),
			touchParticles: Vec::new(),
		}
	}

	//==============================================================================
	// 터치 입자 만들기. (노드 영역 좌표, ±5 픽셀 흩뿌림, ±60 픽셀/초 속도, 반지름 10 ~ 35)
	//==============================================================================
	pub fn createTouchParticle(&mut self, x: f32, y: f32) {
		let mut particle = TouchParticle::new();
		particle.position = Vector2::create(x + (math::random() - 0.5) * 10.0, y + (math::random() - 0.5) * 10.0);
		particle.velocity = Vector2::create((math::random() - 0.5) * 120.0, (math::random() - 0.5) * 120.0);
		particle.life = 1.0;
		particle.maxLife = 1.0;
		particle.radius = math::random() * 25.0 + 10.0;
		self.touchParticles.push(particle);
	}

	//==============================================================================
	// 터치 입자 갱신. (수명이 초당 2.5 씩 줄고, 다 되면 뺀다)
	//==============================================================================
	pub fn updateTouchParticles(&mut self, timeDelta: f32) {
		let mut particleIndex = self.touchParticles.len();
		while particleIndex > 0 {
			particleIndex -= 1;
			let particle = &mut self.touchParticles[particleIndex];
			particle.life -= timeDelta * 2.5;
			particle.position.x += particle.velocity.x * timeDelta;
			particle.position.y += particle.velocity.y * timeDelta;
			if particle.life <= 0.0 {
				self.touchParticles.remove(particleIndex);
			}
		}
	}

	//==============================================================================
	// 터치 입자 출력. (흰색 그라디언트를 입자 색으로 틴트하고, 남은 수명만큼 흐리게)
	//==============================================================================
	pub fn drawTouchParticles(&self, graphic: &mut Graphic) {
		if self.touchParticles.is_empty() {
			return;
		}
		let gradientImage = getParticleGradientImage();
		let particleColor = Color::createFromRGBA("rgb(100, 200, 255)");
		let originalAlpha = graphic.getGlobalAlpha();
		graphic.setImageTintColor(Some(particleColor));
		for particle in self.touchParticles.iter() {
			let opacity = (particle.life / particle.maxLife).max(0.0);
			graphic.setGlobalAlpha(originalAlpha * opacity * 0.8);
			let drawPosition = Vector2::create(particle.position.x - particle.radius, particle.position.y - particle.radius);
			let drawSize = Vector2::create(particle.radius * 2.0, particle.radius * 2.0);
			graphic.drawImage(&gradientImage, drawPosition, drawSize);
		}
		graphic.setImageTintColor(None);
		graphic.setGlobalAlpha(originalAlpha);
	}

	//==============================================================================
	// 살아 있는 입자 수. (러스트 판에서 더한 것)
	//==============================================================================
	pub fn getParticleCount(&self) -> usize {
		self.touchParticles.len()
	}
}

impl Default for TouchEffect {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TouchEffect {
		TouchEffect::new()
	}
}

impl Component for TouchEffect {
	//==============================================================================
	// 공통 상태 반환.
	//==============================================================================
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	//==============================================================================
	// 공통 상태 반환. (변경용)
	//==============================================================================
	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	//==============================================================================
	// 컴포넌트 이름.
	//==============================================================================
	fn getComponentType(&self) -> &'static str {
		"TouchEffect"
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, timeDelta: f32) {
		self.updateTouchParticles(timeDelta);
	}

	//==============================================================================
	// 출력. (더하기 블렌드로 그리고 앞의 블렌드로 되돌린다)
	//==============================================================================
	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		if self.touchParticles.is_empty() {
			return;
		}
		let originalBlendMode = graphic.getBlendMode();
		graphic.setBlendMode(BlendMode::lighter);
		self.drawTouchParticles(graphic);
		graphic.setBlendMode(originalBlendMode);
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 입자는 흩뿌린 범위 안에서 생기고, 0.4 초 뒤 사라진다.
	//==============================================================================
	#[test]
	fn particlesSpawnAndExpire() {
		let mut touchEffect = TouchEffect::new();
		for _ in 0..10 {
			touchEffect.createTouchParticle(100.0, 50.0);
		}
		assert_eq!(touchEffect.getParticleCount(), 10);
		for particle in touchEffect.touchParticles.iter() {
			assert!((particle.position.x - 100.0).abs() <= 5.0);
			assert!((particle.position.y - 50.0).abs() <= 5.0);
			assert!(particle.radius >= 10.0 && particle.radius < 35.0);
		}
		touchEffect.updateTouchParticles(0.2);
		assert_eq!(touchEffect.getParticleCount(), 10);
		assert!((touchEffect.touchParticles[0].life - 0.5).abs() < 0.0001);
		touchEffect.updateTouchParticles(0.2);
		assert_eq!(touchEffect.getParticleCount(), 0);
	}

	//==============================================================================
	// 그라디언트 그림은 가운데가 불투명하고 모서리는 투명하며, 한 번만 만든다.
	//==============================================================================
	#[test]
	fn gradientImageIsRadial() {
		let gradientImage = getParticleGradientImage();
		assert_eq!(gradientImage.getWidth(), PARTICLE_GRADIENT_IMAGE_SIZE);
		let pixels = gradientImage.getPixels();
		let rowStride = PARTICLE_GRADIENT_IMAGE_SIZE as usize * 4;
		let centerIndex = 32 * rowStride + 32 * 4;
		assert!(pixels[centerIndex + 3] >= 245);
		assert_eq!(pixels[3], 0);
		assert!(Rc::ptr_eq(&gradientImage, &getParticleGradientImage()));
	}
}
