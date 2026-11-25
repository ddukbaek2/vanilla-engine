//==============================================================================
//! 사각형. (좌상 위치 + 크기, 원본 base/rect.js)
//! - 원본의 width, min, center, left 같은 프로퍼티는 get_..., set_... 함수로 옮겼다.
//! - intersects 는 러스트 판에서 먼저 만든 것으로 변이 닿기만 하면 겹치지 않는 것으로 본다.
//!   원본 overlaps 는 변이 닿아도 겹친 것으로 보고, intersection 이 이것을 쓴다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
/// 사각형. (왼쪽 위 위치와 크기를 공개 필드로 둔 Copy 값, y 는 아래로 커진다)
///
/// # 예제
/// 점과 영역 검사, 겹친 영역 구하기
/// ```rust
/// use vanilla::{Rect, Vector2};
///
/// let panel = Rect::create(0.0, 0.0, 100.0, 50.0);
/// let button = Rect::create(80.0, 40.0, 40.0, 20.0);
/// assert!(panel.contains(Vector2::create(10.0, 10.0)));
/// assert!(panel.overlaps(&button));
///
/// let overlap = panel.intersection(&button);
/// assert_eq!(overlap, Rect::create(80.0, 40.0, 20.0, 10.0));
/// let bounds = panel.union(&button);
/// assert_eq!((bounds.get_right(), bounds.get_bottom()), (120.0, 60.0));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
	/// 왼쪽 위 위치.
	pub position: Vector2,
	/// 크기. (x 가 너비, y 가 높이)
	pub size: Vector2,
}

impl Rect {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `x`: 왼쪽 x.
	/// - `y`: 위 y.
	/// - `width`: 너비.
	/// - `height`: 높이.
	///
	/// # 돌려줌
	/// 만든 사각형.
	//==============================================================================
	pub const fn create(x: f32, y: f32, width: f32, height: f32) -> Rect {
		Rect {
			position: Vector2::create(x, y),
			size: Vector2::create(width, height),
		}
	}

	//==============================================================================
	/// 빈 사각형.
	///
	/// # 돌려줌
	/// 위치와 크기가 모두 0 인 사각형.
	//==============================================================================
	pub const fn zero() -> Rect {
		Rect::create(0.0, 0.0, 0.0, 0.0)
	}

	//==============================================================================
	/// 모든 값이 0 인지.
	///
	/// # 돌려줌
	/// 위치와 크기의 네 값이 모두 0 이면 true.
	//==============================================================================
	pub fn is_zero(&self) -> bool {
		self.position.x == 0.0 && self.position.y == 0.0 && self.size.x == 0.0 && self.size.y == 0.0
	}

	//==============================================================================
	/// 너비.
	///
	/// # 돌려줌
	/// 너비. (size.x)
	//==============================================================================
	pub fn get_width(&self) -> f32 {
		self.size.x
	}

	//==============================================================================
	/// 높이.
	///
	/// # 돌려줌
	/// 높이. (size.y)
	//==============================================================================
	pub fn get_height(&self) -> f32 {
		self.size.y
	}

	//==============================================================================
	/// 너비 설정.
	///
	/// # 인자
	/// - `width`: 새 너비. (왼쪽 위는 그대로)
	//==============================================================================
	pub fn set_width(&mut self, width: f32) {
		self.size.x = width;
	}

	//==============================================================================
	/// 높이 설정.
	///
	/// # 인자
	/// - `height`: 새 높이. (왼쪽 위는 그대로)
	//==============================================================================
	pub fn set_height(&mut self, height: f32) {
		self.size.y = height;
	}

	//==============================================================================
	/// 왼쪽 위. (위치)
	///
	/// # 돌려줌
	/// 왼쪽 위 점.
	//==============================================================================
	pub fn get_min(&self) -> Vector2 {
		self.position
	}

	//==============================================================================
	/// 왼쪽 위 설정. (오른쪽 아래는 그대로 두고 크기를 바꾼다)
	///
	/// # 인자
	/// - `minimum`: 새 왼쪽 위 점.
	///
	/// # 예제
	/// 오른쪽 아래를 둔 채 왼쪽 위만 옮기기
	/// ```rust
	/// use vanilla::{Rect, Vector2};
	///
	/// let mut rect = Rect::create(10.0, 20.0, 100.0, 50.0);
	/// rect.set_min(Vector2::zero());
	/// assert_eq!(rect, Rect::create(0.0, 0.0, 110.0, 70.0));
	/// ```
	//==============================================================================
	pub fn set_min(&mut self, minimum: Vector2) {
		let old_maximum = self.get_max();
		self.position = minimum;
		self.size = Vector2::create(old_maximum.x - minimum.x, old_maximum.y - minimum.y);
	}

	//==============================================================================
	/// 오른쪽 아래. (위치 + 크기)
	///
	/// # 돌려줌
	/// 오른쪽 아래 점.
	//==============================================================================
	pub fn get_max(&self) -> Vector2 {
		Vector2::create(self.position.x + self.size.x, self.position.y + self.size.y)
	}

	//==============================================================================
	/// 오른쪽 아래 설정. (위치는 그대로 두고 크기를 바꾼다)
	///
	/// # 인자
	/// - `maximum`: 새 오른쪽 아래 점.
	//==============================================================================
	pub fn set_max(&mut self, maximum: Vector2) {
		self.size = Vector2::create(maximum.x - self.position.x, maximum.y - self.position.y);
	}

	//==============================================================================
	/// 가운데.
	///
	/// # 돌려줌
	/// 가운데 점.
	//==============================================================================
	pub fn get_center(&self) -> Vector2 {
		Vector2::create(self.position.x + self.size.x / 2.0, self.position.y + self.size.y / 2.0)
	}

	//==============================================================================
	/// 가운데 설정. (크기는 그대로 두고 옮긴다)
	///
	/// # 인자
	/// - `center`: 새 가운데 점.
	//==============================================================================
	pub fn set_center(&mut self, center: Vector2) {
		self.position = Vector2::create(center.x - self.size.x / 2.0, center.y - self.size.y / 2.0);
	}

	//==============================================================================
	/// 왼쪽 x.
	///
	/// # 돌려줌
	/// 왼쪽 x.
	//==============================================================================
	pub fn get_left(&self) -> f32 {
		self.position.x
	}

	//==============================================================================
	/// 왼쪽 x 설정. (크기는 그대로 두고 옮긴다)
	///
	/// # 인자
	/// - `left`: 새 왼쪽 x.
	//==============================================================================
	pub fn set_left(&mut self, left: f32) {
		self.position.x = left;
	}

	//==============================================================================
	/// 위 y.
	///
	/// # 돌려줌
	/// 위 y.
	//==============================================================================
	pub fn get_top(&self) -> f32 {
		self.position.y
	}

	//==============================================================================
	/// 위 y 설정. (크기는 그대로 두고 옮긴다)
	///
	/// # 인자
	/// - `top`: 새 위 y.
	//==============================================================================
	pub fn set_top(&mut self, top: f32) {
		self.position.y = top;
	}

	//==============================================================================
	/// 오른쪽 x.
	///
	/// # 돌려줌
	/// 오른쪽 x. (왼쪽 x + 너비)
	//==============================================================================
	pub fn get_right(&self) -> f32 {
		self.position.x + self.size.x
	}

	//==============================================================================
	/// 오른쪽 x 설정. (크기는 그대로 두고 옮긴다)
	///
	/// # 인자
	/// - `right`: 새 오른쪽 x.
	//==============================================================================
	pub fn set_right(&mut self, right: f32) {
		self.position.x = right - self.size.x;
	}

	//==============================================================================
	/// 아래 y.
	///
	/// # 돌려줌
	/// 아래 y. (위 y + 높이)
	//==============================================================================
	pub fn get_bottom(&self) -> f32 {
		self.position.y + self.size.y
	}

	//==============================================================================
	/// 아래 y 설정. (크기는 그대로 두고 옮긴다)
	///
	/// # 인자
	/// - `bottom`: 새 아래 y.
	//==============================================================================
	pub fn set_bottom(&mut self, bottom: f32) {
		self.position.y = bottom - self.size.y;
	}

	//==============================================================================
	/// 점이 안에 있는지.
	///
	/// # 인자
	/// - `point`: 판정할 점.
	///
	/// # 돌려줌
	/// 점이 안이나 변 위에 있으면 true.
	//==============================================================================
	pub fn contains(&self, point: Vector2) -> bool {
		let right_x = self.position.x + self.size.x;
		let bottom_y = self.position.y + self.size.y;
		point.x >= self.position.x && point.x <= right_x && point.y >= self.position.y && point.y <= bottom_y
	}

	//==============================================================================
	/// 다른 사각형과 겹치는지.
	///
	/// # 인자
	/// - `other`: 겹침을 볼 사각형.
	///
	/// # 돌려줌
	/// 안쪽이 겹치면 true. 변이 닿기만 하면 false.
	///
	/// # 예제
	/// 맞닿은 사각형은 intersects 로는 겹치지 않고 overlaps 로는 겹친다
	/// ```rust
	/// use vanilla::Rect;
	///
	/// let left = Rect::create(0.0, 0.0, 10.0, 10.0);
	/// let touching = Rect::create(10.0, 0.0, 5.0, 5.0);
	/// assert!(!left.intersects(&touching));
	/// assert!(left.overlaps(&touching));
	/// ```
	//==============================================================================
	pub fn intersects(&self, other: &Rect) -> bool {
		let right_x = self.position.x + self.size.x;
		let bottom_y = self.position.y + self.size.y;
		let other_right_x = other.position.x + other.size.x;
		let other_bottom_y = other.position.y + other.size.y;
		self.position.x < other_right_x && right_x > other.position.x && self.position.y < other_bottom_y && bottom_y > other.position.y
	}

	//==============================================================================
	/// 다른 사각형과 겹치는지. (원본 overlaps, 변이 닿기만 해도 겹친 것으로 본다)
	///
	/// # 인자
	/// - `other`: 겹침을 볼 사각형.
	///
	/// # 돌려줌
	/// 겹치거나 변이 닿으면 true.
	//==============================================================================
	pub fn overlaps(&self, other: &Rect) -> bool {
		if self.position.x + self.size.x < other.position.x || self.position.x > other.position.x + other.size.x {
			return false;
		}
		if self.position.y + self.size.y < other.position.y || self.position.y > other.position.y + other.size.y {
			return false;
		}
		true
	}

	//==============================================================================
	/// 겹친 영역. (겹치지 않으면 빈 사각형, 변만 닿으면 너비나 높이가 0)
	///
	/// # 인자
	/// - `other`: 겹친 영역을 구할 사각형.
	///
	/// # 돌려줌
	/// 겹친 사각형. 겹치지 않으면 Rect::zero(), 변만 닿으면 너비나 높이가 0 인 사각형.
	///
	/// # 예제
	/// 겹침, 맞닿음, 떨어짐
	/// ```rust
	/// use vanilla::Rect;
	///
	/// let first = Rect::create(0.0, 0.0, 10.0, 10.0);
	/// assert_eq!(first.intersection(&Rect::create(5.0, 5.0, 10.0, 10.0)), Rect::create(5.0, 5.0, 5.0, 5.0));
	/// assert_eq!(first.intersection(&Rect::create(10.0, 0.0, 5.0, 5.0)), Rect::create(10.0, 0.0, 0.0, 5.0));
	/// assert!(first.intersection(&Rect::create(20.0, 20.0, 5.0, 5.0)).is_zero());
	/// ```
	//==============================================================================
	pub fn intersection(&self, other: &Rect) -> Rect {
		if !self.overlaps(other) {
			return Rect::zero();
		}
		let maximum = self.get_max();
		let other_maximum = other.get_max();
		let left_x = self.position.x.max(other.position.x);
		let top_y = self.position.y.max(other.position.y);
		let right_x = maximum.x.min(other_maximum.x);
		let bottom_y = maximum.y.min(other_maximum.y);
		Rect::create(left_x, top_y, right_x - left_x, bottom_y - top_y)
	}

	//==============================================================================
	/// 둘을 다 덮는 가장 작은 사각형.
	///
	/// # 인자
	/// - `other`: 같이 덮을 사각형.
	///
	/// # 돌려줌
	/// 둘을 다 덮는 가장 작은 사각형.
	//==============================================================================
	pub fn union(&self, other: &Rect) -> Rect {
		let maximum = self.get_max();
		let other_maximum = other.get_max();
		let left_x = self.position.x.min(other.position.x);
		let top_y = self.position.y.min(other.position.y);
		let right_x = maximum.x.max(other_maximum.x);
		let bottom_y = maximum.y.max(other_maximum.y);
		Rect::create(left_x, top_y, right_x - left_x, bottom_y - top_y)
	}

	//==============================================================================
	/// 위치와 크기를 성분별로 범위 제한.
	///
	/// # 인자
	/// - `value`: 제한할 사각형.
	/// - `minimum`: 위치와 크기의 최소값. (position, size 를 따로 본다)
	/// - `maximum`: 위치와 크기의 최대값. (position, size 를 따로 본다)
	///
	/// # 돌려줌
	/// 위치와 크기를 성분별로 자른 새 사각형.
	///
	/// # 예제
	/// 위치는 0 ~ 100, 크기는 10 ~ 200 으로 자르기
	/// ```rust
	/// use vanilla::Rect;
	///
	/// let minimum = Rect::create(0.0, 0.0, 10.0, 10.0);
	/// let maximum = Rect::create(100.0, 100.0, 200.0, 200.0);
	/// let clamped = Rect::clamp(Rect::create(-5.0, 50.0, 500.0, 1.0), minimum, maximum);
	/// assert_eq!(clamped, Rect::create(0.0, 50.0, 200.0, 10.0));
	/// ```
	//==============================================================================
	pub fn clamp(value: Rect, minimum: Rect, maximum: Rect) -> Rect {
		let position = Vector2::clamp(value.position, minimum.position, maximum.position);
		let size = Vector2::clamp(value.size, minimum.size, maximum.size);
		Rect::create(position.x, position.y, size.x, size.y)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 변 읽기와 쓰기. (min 을 바꾸면 max 는 그대로, right 와 bottom 과 center 를 바꾸면 크기는 그대로)
	//==============================================================================
	#[test]
	fn edges_and_center() {
		let mut rect = Rect::create(10.0, 20.0, 100.0, 50.0);
		assert_eq!(rect.get_left(), 10.0);
		assert_eq!(rect.get_top(), 20.0);
		assert_eq!(rect.get_right(), 110.0);
		assert_eq!(rect.get_bottom(), 70.0);
		assert_eq!(rect.get_min(), Vector2::create(10.0, 20.0));
		assert_eq!(rect.get_max(), Vector2::create(110.0, 70.0));
		assert_eq!(rect.get_center(), Vector2::create(60.0, 45.0));

		rect.set_min(Vector2::create(0.0, 0.0));
		assert_eq!(rect, Rect::create(0.0, 0.0, 110.0, 70.0));
		rect.set_max(Vector2::create(40.0, 30.0));
		assert_eq!(rect, Rect::create(0.0, 0.0, 40.0, 30.0));
		rect.set_right(100.0);
		assert_eq!(rect, Rect::create(60.0, 0.0, 40.0, 30.0));
		rect.set_bottom(100.0);
		assert_eq!(rect, Rect::create(60.0, 70.0, 40.0, 30.0));
		rect.set_center(Vector2::zero());
		assert_eq!(rect, Rect::create(-20.0, -15.0, 40.0, 30.0));
		rect.set_left(1.0);
		rect.set_top(2.0);
		rect.set_width(3.0);
		rect.set_height(4.0);
		assert_eq!(rect, Rect::create(1.0, 2.0, 3.0, 4.0));
	}

	//==============================================================================
	/// 겹침, 겹친 영역, 합친 영역, 범위 제한. (원본 overlaps 는 변이 닿아도 겹친다)
	//==============================================================================
	#[test]
	fn overlap_intersection_union_clamp() {
		let first = Rect::create(0.0, 0.0, 10.0, 10.0);
		let second = Rect::create(5.0, 5.0, 10.0, 10.0);
		let touching = Rect::create(10.0, 0.0, 5.0, 5.0);
		let apart = Rect::create(20.0, 20.0, 5.0, 5.0);
		assert!(first.overlaps(&second));
		assert!(first.overlaps(&touching));
		assert!(!first.intersects(&touching));
		assert!(!first.overlaps(&apart));
		assert_eq!(first.intersection(&second), Rect::create(5.0, 5.0, 5.0, 5.0));
		assert_eq!(first.intersection(&touching), Rect::create(10.0, 0.0, 0.0, 5.0));
		assert_eq!(first.intersection(&apart), Rect::zero());
		assert_eq!(first.union(&apart), Rect::create(0.0, 0.0, 25.0, 25.0));
		let clamped = Rect::clamp(Rect::create(-5.0, 50.0, 500.0, 1.0), Rect::create(0.0, 0.0, 10.0, 10.0), Rect::create(100.0, 40.0, 200.0, 200.0));
		assert_eq!(clamped, Rect::create(0.0, 40.0, 200.0, 10.0));
	}
}
