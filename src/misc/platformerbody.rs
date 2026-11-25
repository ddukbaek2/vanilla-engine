//==============================================================================
// 플랫포머 물리 컴포넌트.
// - 원본 misc/platformerbody.js 를 옮긴 것이다. 중력 적분, 바닥 접지, 좌우 이동 경계처럼 횡스크롤 게임이 다시 만들던
//   최소 물리(운동학)를 담는다. 노드의 로컬 위치를 직접 움직인다. (아래쪽이 +Y)
// - 사용:
//     let body = node_tree[hero_id].add_component(PlatformerBody::new());
//     body.set_ground_y(560.0);
//     body.set_horizontal_bounds(Some(40.0), Some(920.0));
//     // 이동: body.set_velocity_x(±speed), 도약: body.jump(-720.0)
// - 원본과 다른 점:
//   경계의 null 은 None 이다. 착지 알림은 FnMut() 콜백이다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::ComponentContext;

// 착지 알림.
type LandedEvent = Box<dyn FnMut()>;

//==============================================================================
// 플랫포머 물리 컴포넌트.
//==============================================================================
pub struct PlatformerBody {
	component_base: ComponentBase,
	velocity: Vector2,
	gravity: f32,
	ground_y: f32,
	left_bound: Option<f32>,
	right_bound: Option<f32>,
	is_grounded: bool,
	landed_event: Option<LandedEvent>,
}

#[crate::reflect]
impl PlatformerBody {
	//==============================================================================
	// 생성. (중력 초당 2000, 바닥 0, 경계 없음, 공중)
	//==============================================================================
	pub fn new() -> PlatformerBody {
		PlatformerBody {
			component_base: ComponentBase::new(),
			velocity: Vector2::zero(),
			gravity: 2000.0,
			ground_y: 0.0,
			left_bound: None,
			right_bound: None,
			is_grounded: false,
			landed_event: None,
		}
	}

	//==============================================================================
	// 한 걸음 적분. (속도에 중력 → 위치 → 접지 → 경계. 다음 위치를 돌려준다)
	// - tick 이 부른다. 노드 없이도 시험할 수 있게 떼어 냈다.
	//==============================================================================
	pub fn integrate(&mut self, current_position: Vector2, time_delta: f32) -> Vector2 {
		self.velocity = Vector2::create(self.velocity.x, self.velocity.y + self.gravity * time_delta);

		let mut next_x = current_position.x + self.velocity.x * time_delta;
		let mut next_y = current_position.y + self.velocity.y * time_delta;

		// 접지.
		let was_grounded = self.is_grounded;
		if next_y >= self.ground_y {
			next_y = self.ground_y;
			self.velocity = Vector2::create(self.velocity.x, 0.0);
			self.is_grounded = true;
			if !was_grounded {
				if let Some(landed_event) = self.landed_event.as_mut() {
					landed_event();
				}
			}
		}
		else {
			self.is_grounded = false;
		}

		// 좌우 경계.
		if let Some(left_bound) = self.left_bound {
			if next_x < left_bound {
				next_x = left_bound;
			}
		}
		if let Some(right_bound) = self.right_bound {
			if next_x > right_bound {
				next_x = right_bound;
			}
		}
		Vector2::create(next_x, next_y)
	}

	//==============================================================================
	// 도약. (접지 중일 때만, 위쪽이 음수 속도. 뛰었으면 true)
	//==============================================================================
	pub fn jump(&mut self, jump_speed: f32) -> bool {
		if !self.is_grounded {
			return false;
		}
		self.velocity = Vector2::create(self.velocity.x, jump_speed);
		self.is_grounded = false;
		true
	}

	//==============================================================================
	// 가로 속도 설정.
	//==============================================================================
	pub fn set_velocity_x(&mut self, velocity_x: f32) {
		self.velocity = Vector2::create(velocity_x, self.velocity.y);
	}

	//==============================================================================
	// 세로 속도 설정.
	//==============================================================================
	pub fn set_velocity_y(&mut self, velocity_y: f32) {
		self.velocity = Vector2::create(self.velocity.x, velocity_y);
	}

	//==============================================================================
	// 속도 반환.
	//==============================================================================
	pub fn get_velocity(&self) -> Vector2 {
		self.velocity
	}

	//==============================================================================
	// 중력 설정. (초당 속도 변화량)
	//==============================================================================
	pub fn set_gravity(&mut self, gravity: f32) {
		self.gravity = gravity;
	}

	//==============================================================================
	// 중력 반환.
	//==============================================================================
	pub fn get_gravity(&self) -> f32 {
		self.gravity
	}

	//==============================================================================
	// 바닥 높이 설정. (부모 영역 기준 로컬 Y)
	//==============================================================================
	pub fn set_ground_y(&mut self, ground_y: f32) {
		self.ground_y = ground_y;
	}

	//==============================================================================
	// 바닥 높이 반환.
	//==============================================================================
	pub fn get_ground_y(&self) -> f32 {
		self.ground_y
	}

	//==============================================================================
	// 좌우 이동 경계 설정. (None 이면 그 쪽은 막지 않는다)
	//==============================================================================
	pub fn set_horizontal_bounds(&mut self, left_bound: Option<f32>, right_bound: Option<f32>) {
		self.left_bound = left_bound;
		self.right_bound = right_bound;
	}

	//==============================================================================
	// 접지 중인지.
	//==============================================================================
	pub fn is_grounded(&self) -> bool {
		self.is_grounded
	}

	//==============================================================================
	// 착지 알림 설정. (공중에서 바닥에 닿는 순간)
	//==============================================================================
	pub fn set_landed_event(&mut self, landed_event: impl FnMut() + 'static) {
		self.landed_event = Some(Box::new(landed_event));
	}
}

impl Default for PlatformerBody {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> PlatformerBody {
		PlatformerBody::new()
	}
}

#[crate::reflect]
impl Component for PlatformerBody {
	//==============================================================================
	// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"PlatformerBody"
	}

	//==============================================================================
	// 갱신. (적분한 위치를 노드 로컬 위치에 넣는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		let current_position = context.get_transform().get_local_position();
		let next_position = self.integrate(current_position, time_delta);
		context.get_transform_mut().set_local_position(next_position);
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::NodeTree;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 떨어져 바닥에 닿으면 한 번 알리고, 뛰면 올라갔다 다시 내려온다. 경계 밖으로 못 간다.
	//==============================================================================
	#[test]
	fn fall_land_jump_and_bounds() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let hero_id = node_tree.create_child(root_id, "hero");
		node_tree[hero_id].set_local_position(Vector2::create(100.0, 0.0));
		let landed_count = Rc::new(Cell::new(0));
		let landed_count_for_event = landed_count.clone();
		let body = node_tree[hero_id].add_component(PlatformerBody::new());
		body.set_ground_y(100.0);
		body.set_horizontal_bounds(Some(40.0), Some(120.0));
		body.set_landed_event(move || {
			landed_count_for_event.set(landed_count_for_event.get() + 1);
		});

		// 첫 걸음: 속도 2000 * 0.1 = 200, 위치 0 + 200 * 0.1 = 20.
		node_tree.tick(&engine, 0.1);
		assert_eq!(node_tree[hero_id].get_local_position(), Vector2::create(100.0, 20.0));
		assert!(!node_tree[hero_id].get_component::<PlatformerBody>().expect("body").is_grounded());
		for _ in 0..5 {
			node_tree.tick(&engine, 0.1);
		}
		assert_eq!(node_tree[hero_id].get_local_position().y, 100.0);
		assert_eq!(landed_count.get(), 1);
		let body = node_tree[hero_id].get_component_mut::<PlatformerBody>().expect("body");
		assert!(body.is_grounded());
		assert_eq!(body.get_velocity().y, 0.0);

		// 뛰기. (공중에서는 다시 못 뛴다)
		assert!(body.jump(-500.0));
		assert!(!body.jump(-500.0));
		body.set_velocity_x(-1000.0);
		node_tree.tick(&engine, 0.1);
		let hero_position = node_tree[hero_id].get_local_position();
		assert_eq!(hero_position.x, 40.0);
		assert!((hero_position.y - (100.0 - 30.0)).abs() < 0.001);
		for _ in 0..10 {
			node_tree.tick(&engine, 0.1);
		}
		assert_eq!(node_tree[hero_id].get_local_position().y, 100.0);
		assert_eq!(landed_count.get(), 2);
	}
}
