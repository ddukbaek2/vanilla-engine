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
	pub fn isZero(&self) -> bool {
		self.position.x == 0.0 && self.position.y == 0.0 && self.size.x == 0.0 && self.size.y == 0.0
	}

	//==============================================================================
	// 너비.
	//==============================================================================
	pub fn getWidth(&self) -> f32 {
		self.size.x
	}

	//==============================================================================
	// 높이.
	//==============================================================================
	pub fn getHeight(&self) -> f32 {
		self.size.y
	}

	//==============================================================================
	// 점이 안에 있는지.
	//==============================================================================
	pub fn contains(&self, point: Vector2) -> bool {
		let rightX = self.position.x + self.size.x;
		let bottomY = self.position.y + self.size.y;
		point.x >= self.position.x && point.x <= rightX && point.y >= self.position.y && point.y <= bottomY
	}

	//==============================================================================
	// 다른 사각형과 겹치는지.
	//==============================================================================
	pub fn intersects(&self, other: &Rect) -> bool {
		let rightX = self.position.x + self.size.x;
		let bottomY = self.position.y + self.size.y;
		let otherRightX = other.position.x + other.size.x;
		let otherBottomY = other.position.y + other.size.y;
		self.position.x < otherRightX && rightX > other.position.x && self.position.y < otherBottomY && bottomY > other.position.y
	}
}
