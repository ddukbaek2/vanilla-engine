//==============================================================================
// 인디케이터. (돌아가는 톱니바퀴 모양 로딩 표시)
// - 원본 experimental/indicator.js 를 옮긴 것이다. 노드가 아닌 그냥 객체라서, 들고 있는 쪽이 tick 과 draw 를 불러 준다.
//   중심에서 바깥으로 뻗는 톱니 8 개와 두꺼운 안쪽 링을 회전시켜 그린다.
// - 원본과 다른 점:
//   - 원본 tick / draw 는 this.isVisible() 를 부르지만 그런 함수가 없어(getVisible 만 있다) 실제로는 예외가 난다.
//     러스트 판은 출력 여부를 그대로 읽는다. 이름은 원본처럼 setVisible / getVisible 이다.
//   - 색은 문자열 대신 Color 다. (원본 "#ffffff", "rgba(...)" 는 Color::createFromHEX / createFromRGBA 로 만든다)
//   - 원본 선 끝(lineCap round)은 둥글다. 러스트 Graphic 의 선은 끝이 각지다.
//   - 원본 import 경로("./graphic.js")는 깨져 있다. 러스트 판은 core::graphic 을 쓴다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

// 톱니 개수.
const TEETH_COUNT: usize = 8;

//==============================================================================
// 인디케이터.
//==============================================================================
#[derive(Clone, Debug)]
pub struct Indicator {
	// 화면 위 출력 위치. (가운데)
	position: Vector2,
	// 톱니바퀴 기본 반지름.
	radius: f32,
	color: Color,
	// 지금 회전 각도. (라디안)
	rotation: f32,
	// 회전 속도. (라디안 / 초)
	speed: f32,
	isVisible: bool,
}

impl Indicator {
	//==============================================================================
	// 생성. (반지름 20, 흰색, 초당 0.75 바퀴, 보임)
	//==============================================================================
	pub fn new() -> Indicator {
		Indicator {
			position: Vector2::zero(),
			radius: 20.0,
			color: Color::white(),
			rotation: 0.0,
			speed: math::PI * 1.5,
			isVisible: true,
		}
	}

	//==============================================================================
	// 갱신. (보일 때만 회전 각도를 쌓는다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		if !self.isVisible {
			return;
		}
		self.rotation += self.speed * timeDelta;
	}

	//==============================================================================
	// 톱니 선분 목록. (회전 전 가운데 기준. 안쪽 링보다 살짝 안쪽에서 바깥 끝까지)
	//==============================================================================
	pub fn buildTeethSegments(&self) -> Vec<(Vector2, Vector2)> {
		let innerRadius = self.radius * 0.5;
		let outerRadius = self.radius;
		(0..TEETH_COUNT).map(|toothIndex| {
			let angle = (toothIndex as f32 / TEETH_COUNT as f32) * math::PI * 2.0;
			let cosine = angle.cos();
			let sine = angle.sin();
			(Vector2::create(cosine * innerRadius * 0.8, sine * innerRadius * 0.8), Vector2::create(cosine * outerRadius, sine * outerRadius))
		}).collect()
	}

	//==============================================================================
	// 출력. (위치로 옮기고 돌린 뒤 톱니와 안쪽 링을 선으로 그린다)
	//==============================================================================
	pub fn draw(&self, graphic: &mut Graphic) {
		if !self.isVisible {
			return;
		}
		graphic.pushState();
		graphic.translate(self.position.x, self.position.y);
		graphic.rotate(self.rotation);
		graphic.setStrokeColor(self.color);
		graphic.setFillColor(self.color);

		// 톱니.
		let teethLineWidth = self.radius * 0.2;
		for (startPosition, endPosition) in self.buildTeethSegments() {
			graphic.drawLine(&[startPosition, endPosition], teethLineWidth);
		}

		// 안쪽 링. (가운데 구멍은 두꺼운 외곽선으로)
		graphic.drawStrokeCircle(Vector2::zero(), self.radius * 0.5, self.radius * 0.3);
		graphic.popState();
	}

	//==============================================================================
	// 위치 설정. (가운데 좌표)
	//==============================================================================
	pub fn setPosition(&mut self, x: f32, y: f32) {
		self.position = Vector2::create(x, y);
	}

	//==============================================================================
	// 위치 반환.
	//==============================================================================
	pub fn getPosition(&self) -> Vector2 {
		self.position
	}

	//==============================================================================
	// 크기 설정. (반지름, 1 이상)
	//==============================================================================
	pub fn setRadius(&mut self, radius: f32) {
		self.radius = radius.max(1.0);
	}

	//==============================================================================
	// 크기 반환.
	//==============================================================================
	pub fn getRadius(&self) -> f32 {
		self.radius
	}

	//==============================================================================
	// 색 설정.
	//==============================================================================
	pub fn setColor(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	// 회전 속도 설정. (라디안 / 초)
	//==============================================================================
	pub fn setSpeed(&mut self, speedRadiansPerSecond: f32) {
		self.speed = speedRadiansPerSecond;
	}

	//==============================================================================
	// 회전 각도 반환. (라디안)
	//==============================================================================
	pub fn getRotation(&self) -> f32 {
		self.rotation
	}

	//==============================================================================
	// 출력 여부 설정.
	//==============================================================================
	pub fn setVisible(&mut self, isVisible: bool) {
		self.isVisible = isVisible;
	}

	//==============================================================================
	// 출력 여부 반환.
	//==============================================================================
	pub fn getVisible(&self) -> bool {
		self.isVisible
	}
}

impl Default for Indicator {
	fn default() -> Indicator {
		Indicator::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 보일 때만 돌고, 반지름은 1 아래로 내려가지 않으며, 톱니 8 개가 안쪽 링 안에서 바깥 끝까지 뻗는다.
	//==============================================================================
	#[test]
	fn rotatesOnlyWhenVisibleAndBuildsTeeth() {
		let mut indicator = Indicator::new();
		indicator.setSpeed(2.0);
		indicator.tick(0.5);
		assert_eq!(indicator.getRotation(), 1.0);
		indicator.setVisible(false);
		indicator.tick(0.5);
		assert_eq!(indicator.getRotation(), 1.0);
		assert!(!indicator.getVisible());

		indicator.setRadius(0.2);
		assert_eq!(indicator.getRadius(), 1.0);
		indicator.setRadius(40.0);
		let teethSegments = indicator.buildTeethSegments();
		assert_eq!(teethSegments.len(), 8);
		assert!((teethSegments[0].0.x - 16.0).abs() < 0.0001);
		assert!((teethSegments[0].1.x - 40.0).abs() < 0.0001);
		assert!((teethSegments[2].1.y - 40.0).abs() < 0.0001);
		indicator.setPosition(100.0, 50.0);
		assert_eq!(indicator.getPosition(), Vector2::create(100.0, 50.0));
	}
}
