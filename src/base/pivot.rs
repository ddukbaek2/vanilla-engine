//==============================================================================
//! 피봇 / 앵커 값.
//==============================================================================
use crate::base::vector2::Vector2;

pub struct Pivot;

impl Pivot {
	pub const TOP_LEFT: Vector2 = Vector2::create(0.0, 0.0);
	pub const TOP_CENTER: Vector2 = Vector2::create(0.5, 0.0);
	pub const TOP_RIGHT: Vector2 = Vector2::create(1.0, 0.0);
	pub const MIDDLE_LEFT: Vector2 = Vector2::create(0.0, 0.5);
	pub const MIDDLE_CENTER: Vector2 = Vector2::create(0.5, 0.5);
	pub const MIDDLE_RIGHT: Vector2 = Vector2::create(1.0, 0.5);
	pub const BOTTOM_LEFT: Vector2 = Vector2::create(0.0, 1.0);
	pub const BOTTOM_CENTER: Vector2 = Vector2::create(0.5, 1.0);
	pub const BOTTOM_RIGHT: Vector2 = Vector2::create(1.0, 1.0);
}
