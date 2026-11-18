//==============================================================================
// 흔들림 발생기. (화면 / 노드 공용)
// - 원본 misc/shaker.js 를 옮긴 것이다. 위상이 다른 두 사인파로 x / y 오프셋을 만들고 지수 감쇠로 잦아들게 한다.
// - addShake(strength) 로 세기를 더하고, 매 프레임 tick(timeDelta) 뒤 getOffset() 을 카메라나 노드 위치에 더해 쓴다.
// - 원본과 다른 점:
//   생성자의 options 객체({ decayRate, frequency })는 ShakerOptions 구조체다. 비운 값은 ..ShakerOptions::default() 로 채운다.
//     let shaker = Shaker::new(ShakerOptions { decayRate: 10.0, ..ShakerOptions::default() });
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
// 흔들림 설정. (원본 options = { decayRate = 9, frequency = 42 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakerOptions {
	// 감쇠율. (클수록 빨리 잦아든다)
	pub decayRate: f32,
	// 진동 주파수. (라디안 / 초)
	pub frequency: f32,
}

impl Default for ShakerOptions {
	//==============================================================================
	// 기본값. (감쇠율 9, 주파수 42)
	//==============================================================================
	fn default() -> ShakerOptions {
		ShakerOptions {
			decayRate: 9.0,
			frequency: 42.0,
		}
	}
}

//==============================================================================
// 흔들림 발생기.
//==============================================================================
#[derive(Clone, Debug)]
pub struct Shaker {
	strength: f32,
	elapsedSeconds: f32,
	decayRate: f32,
	frequency: f32,
}

impl Shaker {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(options: ShakerOptions) -> Shaker {
		Shaker {
			strength: 0.0,
			elapsedSeconds: 0.0,
			decayRate: options.decayRate,
			frequency: options.frequency,
		}
	}

	//==============================================================================
	// 흔들림 더하기. (최대 오프셋 픽셀. 이미 흔들리는 중이면 더 센 쪽을 따른다)
	//==============================================================================
	pub fn addShake(&mut self, strength: f32) {
		self.strength = self.strength.max(strength);
	}

	//==============================================================================
	// 갱신. (세기가 0.05 아래로 떨어지면 멈춘다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		self.elapsedSeconds += timeDelta;
		if self.strength > 0.0 {
			self.strength *= (-self.decayRate * timeDelta).exp();
			if self.strength < 0.05 {
				self.strength = 0.0;
			}
		}
	}

	//==============================================================================
	// 지금 오프셋 반환. (두 축의 주파수 비를 1 : 1.37 로 달리 두어 궤적이 단조롭지 않게 한다)
	//==============================================================================
	pub fn getOffset(&self) -> Vector2 {
		if self.strength <= 0.0 {
			return Vector2::zero();
		}
		let time = self.elapsedSeconds;
		let offsetX = (time * self.frequency).sin() * self.strength;
		let offsetY = (time * self.frequency * 1.37).cos() * self.strength;
		Vector2::create(offsetX, offsetY)
	}

	//==============================================================================
	// 흔들리는 중인지.
	//==============================================================================
	pub fn isShaking(&self) -> bool {
		self.strength > 0.0
	}

	//==============================================================================
	// 곧바로 멈추기.
	//==============================================================================
	pub fn stop(&mut self) {
		self.strength = 0.0;
	}

	//==============================================================================
	// 감쇠율 설정. (클수록 빨리 잦아든다)
	//==============================================================================
	pub fn setDecayRate(&mut self, decayRate: f32) {
		self.decayRate = decayRate;
	}

	//==============================================================================
	// 진동 주파수 설정.
	//==============================================================================
	pub fn setFrequency(&mut self, frequency: f32) {
		self.frequency = frequency;
	}
}

impl Default for Shaker {
	//==============================================================================
	// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> Shaker {
		Shaker::new(ShakerOptions::default())
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 세기는 exp(-decayRate * dt) 로 줄고, 0.05 아래면 멈춘다. 더 약한 흔들림은 무시한다.
	//==============================================================================
	#[test]
	fn shakeDecaysAndStops() {
		let mut shaker = Shaker::new(ShakerOptions {
			decayRate: 10.0,
			..ShakerOptions::default()
		});
		assert!(!shaker.isShaking());
		assert_eq!(shaker.getOffset(), Vector2::zero());

		shaker.addShake(8.0);
		shaker.addShake(3.0);
		shaker.tick(0.1);
		let expectedStrength = 8.0 * (-1.0f32).exp();
		let offset = shaker.getOffset();
		let expectedOffsetX = (0.1f32 * 42.0).sin() * expectedStrength;
		let expectedOffsetY = (0.1f32 * 42.0 * 1.37).cos() * expectedStrength;
		assert!((offset.x - expectedOffsetX).abs() < 0.0001);
		assert!((offset.y - expectedOffsetY).abs() < 0.0001);

		let mut tickCount = 0;
		while shaker.isShaking() {
			shaker.tick(0.1);
			tickCount += 1;
		}
		// 2.94 * e^(-n) < 0.05 → n = 5
		assert_eq!(tickCount, 5);
		assert_eq!(shaker.getOffset(), Vector2::zero());

		shaker.addShake(5.0);
		shaker.stop();
		assert!(!shaker.isShaking());
	}
}
