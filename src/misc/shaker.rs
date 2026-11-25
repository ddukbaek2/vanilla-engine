//==============================================================================
//! 흔들림 발생기. (화면 / 노드 공용)
//! - 원본 misc/shaker.js 를 옮긴 것이다. 위상이 다른 두 사인파로 x / y 오프셋을 만들고 지수 감쇠로 잦아들게 한다.
//! - add_shake(strength) 로 세기를 더하고, 매 프레임 tick(time_delta) 뒤 get_offset() 을 카메라나 노드 위치에 더해 쓴다.
//! - 원본과 다른 점:
//!   생성자의 options 객체({ decay_rate, frequency })는 ShakerOptions 구조체다. 비운 값은 ..ShakerOptions::default() 로 채운다.
//!   ```text
//!     let shaker = Shaker::new(ShakerOptions { decay_rate: 10.0, ..ShakerOptions::default() });
//!   ```
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
/// 흔들림 설정. (원본 options = { decay_rate = 9, frequency = 42 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakerOptions {
	/// 감쇠율. (클수록 빨리 잦아든다)
	pub decay_rate: f32,
	/// 진동 주파수. (라디안 / 초)
	pub frequency: f32,
}

impl Default for ShakerOptions {
	//==============================================================================
	/// 기본값. (감쇠율 9, 주파수 42)
	//==============================================================================
	fn default() -> ShakerOptions {
		ShakerOptions {
			decay_rate: 9.0,
			frequency: 42.0,
		}
	}
}

//==============================================================================
/// 흔들림 발생기.
//==============================================================================
#[derive(Clone, Debug)]
pub struct Shaker {
	strength: f32,
	elapsed_seconds: f32,
	decay_rate: f32,
	frequency: f32,
}

impl Shaker {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new(options: ShakerOptions) -> Shaker {
		Shaker {
			strength: 0.0,
			elapsed_seconds: 0.0,
			decay_rate: options.decay_rate,
			frequency: options.frequency,
		}
	}

	//==============================================================================
	/// 흔들림 더하기. (최대 오프셋 픽셀. 이미 흔들리는 중이면 더 센 쪽을 따른다)
	//==============================================================================
	pub fn add_shake(&mut self, strength: f32) {
		self.strength = self.strength.max(strength);
	}

	//==============================================================================
	/// 갱신. (세기가 0.05 아래로 떨어지면 멈춘다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		self.elapsed_seconds += time_delta;
		if self.strength > 0.0 {
			self.strength *= (-self.decay_rate * time_delta).exp();
			if self.strength < 0.05 {
				self.strength = 0.0;
			}
		}
	}

	//==============================================================================
	/// 지금 오프셋 반환. (두 축의 주파수 비를 1 : 1.37 로 달리 두어 궤적이 단조롭지 않게 한다)
	//==============================================================================
	pub fn get_offset(&self) -> Vector2 {
		if self.strength <= 0.0 {
			return Vector2::zero();
		}
		let time = self.elapsed_seconds;
		let offset_x = (time * self.frequency).sin() * self.strength;
		let offset_y = (time * self.frequency * 1.37).cos() * self.strength;
		Vector2::create(offset_x, offset_y)
	}

	//==============================================================================
	/// 흔들리는 중인지.
	//==============================================================================
	pub fn is_shaking(&self) -> bool {
		self.strength > 0.0
	}

	//==============================================================================
	/// 곧바로 멈추기.
	//==============================================================================
	pub fn stop(&mut self) {
		self.strength = 0.0;
	}

	//==============================================================================
	/// 감쇠율 설정. (클수록 빨리 잦아든다)
	//==============================================================================
	pub fn set_decay_rate(&mut self, decay_rate: f32) {
		self.decay_rate = decay_rate;
	}

	//==============================================================================
	/// 진동 주파수 설정.
	//==============================================================================
	pub fn set_frequency(&mut self, frequency: f32) {
		self.frequency = frequency;
	}
}

impl Default for Shaker {
	//==============================================================================
	/// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> Shaker {
		Shaker::new(ShakerOptions::default())
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 세기는 exp(-decay_rate * dt) 로 줄고, 0.05 아래면 멈춘다. 더 약한 흔들림은 무시한다.
	//==============================================================================
	#[test]
	fn shake_decays_and_stops() {
		let mut shaker = Shaker::new(ShakerOptions {
			decay_rate: 10.0,
			..ShakerOptions::default()
		});
		assert!(!shaker.is_shaking());
		assert_eq!(shaker.get_offset(), Vector2::zero());

		shaker.add_shake(8.0);
		shaker.add_shake(3.0);
		shaker.tick(0.1);
		let expected_strength = 8.0 * (-1.0f32).exp();
		let offset = shaker.get_offset();
		let expected_offset_x = (0.1f32 * 42.0).sin() * expected_strength;
		let expected_offset_y = (0.1f32 * 42.0 * 1.37).cos() * expected_strength;
		assert!((offset.x - expected_offset_x).abs() < 0.0001);
		assert!((offset.y - expected_offset_y).abs() < 0.0001);

		let mut tick_count = 0;
		while shaker.is_shaking() {
			shaker.tick(0.1);
			tick_count += 1;
		}
		// 2.94 * e^(-n) < 0.05 → n = 5
		assert_eq!(tick_count, 5);
		assert_eq!(shaker.get_offset(), Vector2::zero());

		shaker.add_shake(5.0);
		shaker.stop();
		assert!(!shaker.is_shaking());
	}
}
