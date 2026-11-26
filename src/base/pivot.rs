//==============================================================================
//! 피봇 / 앵커 값.
//! - 원본 base/pivot.js 의 Pivot 객체를 연관 상수로 옮겼다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
/// 피봇과 앵커 상수. (내용 크기 안의 비율, (0, 0) 이 왼쪽 위, (1, 1) 이 오른쪽 아래)
///
/// # 예제
/// 노드의 기준점을 아래 가운데로 두기
/// ```rust
/// use vanilla::{NodeTree, Pivot, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let icon_id = node_tree.create_child(root_id, "icon");
/// node_tree[icon_id].set_content_size(Vector2::create(64.0, 64.0));
/// node_tree[icon_id].set_pivot(Pivot::BOTTOM_CENTER);
/// assert_eq!(node_tree[icon_id].get_pivot(), Vector2::create(0.5, 1.0));
/// ```
//==============================================================================
pub struct Pivot;

impl Pivot {
	/// 왼쪽 위. (0, 0)
	pub const TOP_LEFT: Vector2 = Vector2::create(0.0, 0.0);
	/// 위 가운데. (0.5, 0)
	pub const TOP_CENTER: Vector2 = Vector2::create(0.5, 0.0);
	/// 오른쪽 위. (1, 0)
	pub const TOP_RIGHT: Vector2 = Vector2::create(1.0, 0.0);
	/// 왼쪽 가운데. (0, 0.5)
	pub const MIDDLE_LEFT: Vector2 = Vector2::create(0.0, 0.5);
	/// 정가운데. (0.5, 0.5)
	pub const MIDDLE_CENTER: Vector2 = Vector2::create(0.5, 0.5);
	/// 오른쪽 가운데. (1, 0.5)
	pub const MIDDLE_RIGHT: Vector2 = Vector2::create(1.0, 0.5);
	/// 왼쪽 아래. (0, 1)
	pub const BOTTOM_LEFT: Vector2 = Vector2::create(0.0, 1.0);
	/// 아래 가운데. (0.5, 1)
	pub const BOTTOM_CENTER: Vector2 = Vector2::create(0.5, 1.0);
	/// 오른쪽 아래. (1, 1)
	pub const BOTTOM_RIGHT: Vector2 = Vector2::create(1.0, 1.0);
}
