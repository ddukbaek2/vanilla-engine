//==============================================================================
// 피봇 / 앵커 값.
//==============================================================================
use crate::base::vector2::Vector2;

pub struct Pivot;

impl Pivot {
	pub const topLeft: Vector2 = Vector2::create(0.0, 0.0);
	pub const topCenter: Vector2 = Vector2::create(0.5, 0.0);
	pub const topRight: Vector2 = Vector2::create(1.0, 0.0);
	pub const middleLeft: Vector2 = Vector2::create(0.0, 0.5);
	pub const middleCenter: Vector2 = Vector2::create(0.5, 0.5);
	pub const middleRight: Vector2 = Vector2::create(1.0, 0.5);
	pub const bottomLeft: Vector2 = Vector2::create(0.0, 1.0);
	pub const bottomCenter: Vector2 = Vector2::create(0.5, 1.0);
	pub const bottomRight: Vector2 = Vector2::create(1.0, 1.0);
}
