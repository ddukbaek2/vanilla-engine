//==============================================================================
// 사각형. (좌상 위치 + 크기)
//==============================================================================
use crate::base::vector2::Vector2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
	pub position: Vector2,
	pub size: Vector2,
}

impl Rect {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub const fn create(x: f32, y: f32, width: f32, height: f32) -> Rect {
		Rect {
			position: Vector2::create(x, y),
			size: Vector2::create(width, height),
		}
	}

	//==============================================================================
	// 빈 사각형.
	//==============================================================================
	pub const fn zero() -> Rect {
		Rect::create(0.0, 0.0, 0.0, 0.0)
	}

	//==============================================================================
	// 모든 값이 0 인지.
	//==============================================================================
	pub fn is_zero(&self) -> bool {
		self.position.x == 0.0 && self.position.y == 0.0 && self.size.x == 0.0 && self.size.y == 0.0
	}

	//==============================================================================
	// 너비.
	//==============================================================================
	pub fn get_width(&self) -> f32 {
		self.size.x
	}

	//==============================================================================
	// 높이.
	//==============================================================================
	pub fn get_height(&self) -> f32 {
		self.size.y
	}

	//==============================================================================
	// 점이 안에 있는지.
	//==============================================================================
	pub fn contains(&self, point: Vector2) -> bool {
		let right_x = self.position.x + self.size.x;
		let bottom_y = self.position.y + self.size.y;
		point.x >= self.position.x && point.x <= right_x && point.y >= self.position.y && point.y <= bottom_y
	}

	//==============================================================================
	// 다른 사각형과 겹치는지.
	//==============================================================================
	pub fn intersects(&self, other: &Rect) -> bool {
		let right_x = self.position.x + self.size.x;
		let bottom_y = self.position.y + self.size.y;
		let other_right_x = other.position.x + other.size.x;
		let other_bottom_y = other.position.y + other.size.y;
		self.position.x < other_right_x && right_x > other.position.x && self.position.y < other_bottom_y && bottom_y > other.position.y
	}
}
