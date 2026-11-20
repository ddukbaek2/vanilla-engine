//==============================================================================
// 2D 충돌 유틸.
// - 원본 misc/collision2d.js 를 옮긴 것이다. 원-원 겹침 분리(밀어내기)와 반경 안 최근접 찾기처럼 액션 게임마다 다시 만들던 판정을 모았다.
// - 모두 정적 함수다.
// - 원본과 다른 점:
//   resolve_circle_overlap 은 null 대신 None, { push_a, push_b } 대신 CircleOverlapPush 를 돌려준다.
//   find_nearest_in_radius 는 대상의 참조를 돌려주고, 고칠 때 쓰도록 번호를 돌려주는 find_nearest_index_in_radius 를 더했다.
//   is_static_b 기본값(false)은 러스트에 기본 인자가 없어 늘 넘긴다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
// 원-원 겹침을 풀 이동량. (원본 { push_a, push_b })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleOverlapPush {
	pub push_a: Vector2,
	pub push_b: Vector2,
}

//==============================================================================
// 2D 충돌 유틸. (정적 함수 묶음)
//==============================================================================
pub struct Collision2D;

impl Collision2D {
	//==============================================================================
	// 원-원 겹침 검사. (닿기만 한 것은 겹친 것이 아니다)
	//==============================================================================
	pub fn test_circle_overlap(position_a: Vector2, radius_a: f32, position_b: Vector2, radius_b: f32) -> bool {
		let difference_x = position_b.x - position_a.x;
		let difference_y = position_b.y - position_a.y;
		let radius_sum = radius_a + radius_b;
		difference_x * difference_x + difference_y * difference_y < radius_sum * radius_sum
	}

	//==============================================================================
	// 원-원 겹침 분리. (겹쳐 있으면 서로 절반씩 밀어낼 이동량, 안 겹치면 None)
	// - is_static_b 면 B 는 고정물로 보고 A 만 겹친 깊이 전체를 밀린다.
	// - 두 중심이 거의 같으면 x 축으로 밀어낸다.
	//==============================================================================
	pub fn resolve_circle_overlap(position_a: Vector2, radius_a: f32, position_b: Vector2, radius_b: f32, is_static_b: bool) -> Option<CircleOverlapPush> {
		let difference_x = position_b.x - position_a.x;
		let difference_y = position_b.y - position_a.y;
		let radius_sum = radius_a + radius_b;
		let distance_squared = difference_x * difference_x + difference_y * difference_y;
		if distance_squared >= radius_sum * radius_sum {
			return None;
		}
		let distance = distance_squared.sqrt();
		let mut direction_x = 1.0;
		let mut direction_y = 0.0;
		if distance > 0.000001 {
			direction_x = difference_x / distance;
			direction_y = difference_y / distance;
		}
		let overlap_depth = radius_sum - distance;
		if is_static_b {
			return Some(CircleOverlapPush {
				push_a: Vector2::create(-direction_x * overlap_depth, -direction_y * overlap_depth),
				push_b: Vector2::zero(),
			});
		}
		let half_depth = overlap_depth * 0.5;
		Some(CircleOverlapPush {
			push_a: Vector2::create(-direction_x * half_depth, -direction_y * half_depth),
			push_b: Vector2::create(direction_x * half_depth, direction_y * half_depth),
		})
	}

	//==============================================================================
	// 반경 안 최근접 대상 번호 찾기. (중심에서 radius 안에 있는 것 가운데 가장 가까운 것, 거리가 같으면 뒤의 것)
	//==============================================================================
	pub fn find_nearest_index_in_radius<T>(target_list: &[T], get_position_handler: impl Fn(&T) -> Vector2, center: Vector2, radius: f32) -> Option<usize> {
		let mut nearest_index: Option<usize> = None;
		let mut nearest_distance_squared = radius * radius;
		for (target_index, target) in target_list.iter().enumerate() {
			let target_position = get_position_handler(target);
			let difference_x = target_position.x - center.x;
			let difference_y = target_position.y - center.y;
			let distance_squared = difference_x * difference_x + difference_y * difference_y;
			if distance_squared <= nearest_distance_squared {
				nearest_distance_squared = distance_squared;
				nearest_index = Some(target_index);
			}
		}
		nearest_index
	}

	//==============================================================================
	// 반경 안 최근접 대상 찾기. (투사체 명중 판정처럼)
	//==============================================================================
	pub fn find_nearest_in_radius<T>(target_list: &[T], get_position_handler: impl Fn(&T) -> Vector2, center: Vector2, radius: f32) -> Option<&T> {
		let nearest_index = Collision2D::find_nearest_index_in_radius(target_list, get_position_handler, center, radius)?;
		target_list.get(nearest_index)
	}

	//==============================================================================
	// 점-원 포함 검사. (경계 위도 안으로 친다)
	//==============================================================================
	pub fn test_point_in_circle(point: Vector2, center: Vector2, radius: f32) -> bool {
		let difference_x = point.x - center.x;
		let difference_y = point.y - center.y;
		difference_x * difference_x + difference_y * difference_y <= radius * radius
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 겹침 검사, 분리량, 고정물, 같은 중심.
	//==============================================================================
	#[test]
	fn circle_overlap_cases() {
		let position_a = Vector2::create(0.0, 0.0);
		let position_b = Vector2::create(6.0, 0.0);
		assert!(Collision2D::test_circle_overlap(position_a, 4.0, position_b, 4.0));
		assert!(!Collision2D::test_circle_overlap(position_a, 3.0, position_b, 3.0));
		assert!(Collision2D::resolve_circle_overlap(position_a, 3.0, position_b, 3.0, false).is_none());

		let push = Collision2D::resolve_circle_overlap(position_a, 4.0, position_b, 4.0, false).expect("push");
		assert_eq!(push.push_a, Vector2::create(-1.0, 0.0));
		assert_eq!(push.push_b, Vector2::create(1.0, 0.0));
		let static_push = Collision2D::resolve_circle_overlap(position_a, 4.0, position_b, 4.0, true).expect("push");
		assert_eq!(static_push.push_a, Vector2::create(-2.0, 0.0));
		assert_eq!(static_push.push_b, Vector2::zero());
		let same_push = Collision2D::resolve_circle_overlap(position_a, 1.0, position_a, 1.0, false).expect("push");
		assert_eq!(same_push.push_a, Vector2::create(-1.0, 0.0));
		assert_eq!(same_push.push_b, Vector2::create(1.0, 0.0));
	}

	//==============================================================================
	// 최근접 찾기와 점 포함.
	//==============================================================================
	#[test]
	fn nearest_and_point_in_circle() {
		let target_list = [Vector2::create(10.0, 0.0), Vector2::create(3.0, 4.0), Vector2::create(0.0, 30.0), Vector2::create(-4.0, 3.0)];
		let nearest_index = Collision2D::find_nearest_index_in_radius(&target_list, |target| *target, Vector2::zero(), 12.0);
		// 거리 5 가 둘이면 뒤의 것. (원본 <= 비교)
		assert_eq!(nearest_index, Some(3));
		let nearest_target = Collision2D::find_nearest_in_radius(&target_list, |target| *target, Vector2::zero(), 12.0);
		assert_eq!(nearest_target, Some(&Vector2::create(-4.0, 3.0)));
		assert!(Collision2D::find_nearest_in_radius(&target_list, |target| *target, Vector2::create(100.0, 100.0), 5.0).is_none());
		assert!(Collision2D::test_point_in_circle(Vector2::create(3.0, 4.0), Vector2::zero(), 5.0));
		assert!(!Collision2D::test_point_in_circle(Vector2::create(3.0, 4.1), Vector2::zero(), 5.0));
	}
}
