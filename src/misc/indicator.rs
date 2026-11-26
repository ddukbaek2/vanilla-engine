//==============================================================================
//! 인디케이터. (돌아가는 톱니바퀴 모양 로딩 표시)
//! - 원본 misc/indicator.js 를 옮긴 것이다. 노드가 아닌 그냥 객체라서, 들고 있는 쪽이 tick 과 draw 를 불러 준다.
//!   중심에서 바깥으로 뻗는 톱니 8 개와 두꺼운 안쪽 링을 회전시켜 그린다.
//! - 원본과 같이 선 끝은 각지다. set_color 는 Color 와 글("#ffffff" 등, Color 의 `From<&str>`)을 받는다.
//!   build_teeth_segments, get_radius, get_rotation 은 원본에도 있다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

/// 톱니 개수.
const TEETH_COUNT: usize = 8;

//==============================================================================
/// 인디케이터.
///
/// # 예제
/// 보일 때만 돈다
/// ```rust
/// use vanilla::Indicator;
///
/// let mut indicator = Indicator::new();
/// indicator.set_speed(2.0);
/// indicator.tick(0.5);
/// assert_eq!(indicator.get_rotation(), 1.0);
///
/// indicator.set_visible(false);
/// indicator.tick(0.5);
/// assert_eq!(indicator.get_rotation(), 1.0);
/// ```
///
/// 화면 가운데에 로딩 표시 그리기
/// ```rust,no_run
/// use vanilla::{Graphic, Indicator};
///
/// fn create_loading_indicator() -> Indicator {
/// 	let mut indicator = Indicator::new();
/// 	indicator.set_position(640.0, 360.0);
/// 	indicator.set_radius(32.0);
/// 	indicator.set_color("#ffcc00");
/// 	indicator
/// }
///
/// fn draw_loading(indicator: &mut Indicator, graphic: &mut Graphic, time_delta: f32) {
/// 	indicator.tick(time_delta);
/// 	indicator.draw(graphic);
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Indicator {
	/// 화면 위 출력 위치. (가운데)
	position: Vector2,
	/// 톱니바퀴 기본 반지름.
	radius: f32,
	color: Color,
	/// 지금 회전 각도. (라디안)
	rotation: f32,
	/// 회전 속도. (라디안 / 초)
	speed: f32,
	is_visible: bool,
}

impl Indicator {
	//==============================================================================
	/// 생성. (반지름 20, 흰색, 초당 0.75 바퀴, 보임)
	///
	/// # 돌려줌
	/// 위치 (0, 0), 회전 0 인 인디케이터.
	//==============================================================================
	pub fn new() -> Indicator {
		Indicator {
			position: Vector2::zero(),
			radius: 20.0,
			color: Color::white(),
			rotation: 0.0,
			speed: math::PI * 1.5,
			is_visible: true,
		}
	}

	//==============================================================================
	/// 갱신. (보일 때만 회전 각도를 쌓는다)
	///
	/// # 인자
	/// - `time_delta`: 지난 프레임부터 흐른 시간. (초)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		if !self.is_visible {
			return;
		}
		self.rotation += self.speed * time_delta;
	}

	//==============================================================================
	/// 톱니 선분 목록. (회전 전 가운데 기준. 안쪽 링보다 살짝 안쪽에서 바깥 끝까지)
	///
	/// # 돌려줌
	/// 톱니 8 개의 (안쪽 끝, 바깥 끝). 안쪽 끝은 반지름의 0.4 배, 바깥 끝은 반지름 거리이고, 0 라디안(오른쪽)부터 시계 방향(y 아래)으로 45 도씩이다.
	//==============================================================================
	pub fn build_teeth_segments(&self) -> Vec<(Vector2, Vector2)> {
		let inner_radius = self.radius * 0.5;
		let outer_radius = self.radius;
		(0..TEETH_COUNT).map(|tooth_index| {
			let angle = (tooth_index as f32 / TEETH_COUNT as f32) * math::PI * 2.0;
			let cosine = angle.cos();
			let sine = angle.sin();
			(Vector2::create(cosine * inner_radius * 0.8, sine * inner_radius * 0.8), Vector2::create(cosine * outer_radius, sine * outer_radius))
		}).collect()
	}

	//==============================================================================
	/// 출력. (위치로 옮기고 돌린 뒤 톱니와 안쪽 링을 선으로 그린다)
	/// - 숨겨 있으면 그리지 않는다. 톱니 굵기는 반지름의 0.2 배, 안쪽 링은 반지름 0.5 배 자리에 0.3 배 굵기다. 그리기 상태는 들어올 때로 되돌린다.
	///
	/// # 인자
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn draw(&self, graphic: &mut Graphic) {
		if !self.is_visible {
			return;
		}
		graphic.push_state();
		graphic.translate(self.position.x, self.position.y);
		graphic.rotate(self.rotation);
		graphic.set_stroke_color(self.color);
		graphic.set_fill_color(self.color);

		// 톱니.
		let teeth_line_width = self.radius * 0.2;
		for (start_position, end_position) in self.build_teeth_segments() {
			graphic.draw_line(&[start_position, end_position], teeth_line_width);
		}

		// 안쪽 링. (가운데 구멍은 두꺼운 외곽선으로)
		graphic.draw_stroke_circle(Vector2::zero(), self.radius * 0.5, self.radius * 0.3);
		graphic.pop_state();
	}

	//==============================================================================
	/// 위치 설정. (가운데 좌표)
	///
	/// # 인자
	/// - `x`: 가운데 x. (그릴 때의 좌표계)
	/// - `y`: 가운데 y. (그릴 때의 좌표계)
	//==============================================================================
	pub fn set_position(&mut self, x: f32, y: f32) {
		self.position = Vector2::create(x, y);
	}

	//==============================================================================
	/// 위치 반환.
	///
	/// # 돌려줌
	/// 가운데 좌표.
	//==============================================================================
	pub fn get_position(&self) -> Vector2 {
		self.position
	}

	//==============================================================================
	/// 크기 설정. (반지름, 1 이상)
	///
	/// # 인자
	/// - `radius`: 톱니 바깥 끝까지의 반지름. (1 보다 작으면 1)
	//==============================================================================
	pub fn set_radius(&mut self, radius: f32) {
		self.radius = radius.max(1.0);
	}

	//==============================================================================
	/// 크기 반환.
	///
	/// # 돌려줌
	/// 반지름. (처음 20)
	//==============================================================================
	pub fn get_radius(&self) -> f32 {
		self.radius
	}

	//==============================================================================
	/// 색 설정.
	///
	/// # 인자
	/// - `color`: 톱니와 링의 색. (Color 나 "#ffffff" 꼴 글)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	/// 회전 속도 설정. (라디안 / 초)
	///
	/// # 인자
	/// - `speed_radians_per_second`: 회전 속도. (라디안 / 초, 처음 1.5π, 음수면 거꾸로 돈다)
	//==============================================================================
	pub fn set_speed(&mut self, speed_radians_per_second: f32) {
		self.speed = speed_radians_per_second;
	}

	//==============================================================================
	/// 회전 각도 반환. (라디안)
	///
	/// # 돌려줌
	/// 지금까지 쌓인 회전 각도. (라디안, 한 바퀴마다 되감지 않는다)
	//==============================================================================
	pub fn get_rotation(&self) -> f32 {
		self.rotation
	}

	//==============================================================================
	/// 출력 여부 설정.
	///
	/// # 인자
	/// - `is_visible`: 참이면 그리고 돈다. 거짓이면 그리지 않고 회전도 멈춘다.
	//==============================================================================
	pub fn set_visible(&mut self, is_visible: bool) {
		self.is_visible = is_visible;
	}

	//==============================================================================
	/// 출력 여부 반환.
	///
	/// # 돌려줌
	/// 보이면 true.
	//==============================================================================
	pub fn get_visible(&self) -> bool {
		self.is_visible
	}
}

impl Default for Indicator {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Indicator {
		Indicator::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 보일 때만 돌고, 반지름은 1 아래로 내려가지 않으며, 톱니 8 개가 안쪽 링 안에서 바깥 끝까지 뻗는다.
	//==============================================================================
	#[test]
	fn rotates_only_when_visible_and_builds_teeth() {
		let mut indicator = Indicator::new();
		indicator.set_speed(2.0);
		indicator.tick(0.5);
		assert_eq!(indicator.get_rotation(), 1.0);
		indicator.set_visible(false);
		indicator.tick(0.5);
		assert_eq!(indicator.get_rotation(), 1.0);
		assert!(!indicator.get_visible());

		indicator.set_radius(0.2);
		assert_eq!(indicator.get_radius(), 1.0);
		indicator.set_radius(40.0);
		let teeth_segments = indicator.build_teeth_segments();
		assert_eq!(teeth_segments.len(), 8);
		assert!((teeth_segments[0].0.x - 16.0).abs() < 0.0001);
		assert!((teeth_segments[0].1.x - 40.0).abs() < 0.0001);
		assert!((teeth_segments[2].1.y - 40.0).abs() < 0.0001);
		indicator.set_position(100.0, 50.0);
		assert_eq!(indicator.get_position(), Vector2::create(100.0, 50.0));
	}
}
