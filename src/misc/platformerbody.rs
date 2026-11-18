//==============================================================================
// 플랫포머 물리 컴포넌트.
// - 원본 misc/platformerbody.js 를 옮긴 것이다. 중력 적분, 바닥 접지, 좌우 이동 경계처럼 횡스크롤 게임이 다시 만들던
//   최소 물리(운동학)를 담는다. 노드의 로컬 위치를 직접 움직인다. (아래쪽이 +Y)
// - 사용:
//     let body = nodeTree[heroId].addComponent(PlatformerBody::new());
//     body.setGroundY(560.0);
//     body.setHorizontalBounds(Some(40.0), Some(920.0));
//     // 이동: body.setVelocityX(±speed), 도약: body.jump(-720.0)
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
	componentBase: ComponentBase,
	velocity: Vector2,
	gravity: f32,
	groundY: f32,
	leftBound: Option<f32>,
	rightBound: Option<f32>,
	isGrounded: bool,
	landedEvent: Option<LandedEvent>,
}

impl PlatformerBody {
	//==============================================================================
	// 생성. (중력 초당 2000, 바닥 0, 경계 없음, 공중)
	//==============================================================================
	pub fn new() -> PlatformerBody {
		PlatformerBody {
			componentBase: ComponentBase::new(),
			velocity: Vector2::zero(),
			gravity: 2000.0,
			groundY: 0.0,
			leftBound: None,
			rightBound: None,
			isGrounded: false,
			landedEvent: None,
		}
	}

	//==============================================================================
	// 한 걸음 적분. (속도에 중력 → 위치 → 접지 → 경계. 다음 위치를 돌려준다)
	// - tick 이 부른다. 노드 없이도 시험할 수 있게 떼어 냈다.
	//==============================================================================
	pub fn integrate(&mut self, currentPosition: Vector2, timeDelta: f32) -> Vector2 {
		self.velocity = Vector2::create(self.velocity.x, self.velocity.y + self.gravity * timeDelta);

		let mut nextX = currentPosition.x + self.velocity.x * timeDelta;
		let mut nextY = currentPosition.y + self.velocity.y * timeDelta;

		// 접지.
		let wasGrounded = self.isGrounded;
		if nextY >= self.groundY {
			nextY = self.groundY;
			self.velocity = Vector2::create(self.velocity.x, 0.0);
			self.isGrounded = true;
			if !wasGrounded {
				if let Some(landedEvent) = self.landedEvent.as_mut() {
					landedEvent();
				}
			}
		}
		else {
			self.isGrounded = false;
		}

		// 좌우 경계.
		if let Some(leftBound) = self.leftBound {
			if nextX < leftBound {
				nextX = leftBound;
			}
		}
		if let Some(rightBound) = self.rightBound {
			if nextX > rightBound {
				nextX = rightBound;
			}
		}
		Vector2::create(nextX, nextY)
	}

	//==============================================================================
	// 도약. (접지 중일 때만, 위쪽이 음수 속도. 뛰었으면 true)
	//==============================================================================
	pub fn jump(&mut self, jumpSpeed: f32) -> bool {
		if !self.isGrounded {
			return false;
		}
		self.velocity = Vector2::create(self.velocity.x, jumpSpeed);
		self.isGrounded = false;
		true
	}

	//==============================================================================
	// 가로 속도 설정.
	//==============================================================================
	pub fn setVelocityX(&mut self, velocityX: f32) {
		self.velocity = Vector2::create(velocityX, self.velocity.y);
	}

	//==============================================================================
	// 세로 속도 설정.
	//==============================================================================
	pub fn setVelocityY(&mut self, velocityY: f32) {
		self.velocity = Vector2::create(self.velocity.x, velocityY);
	}

	//==============================================================================
	// 속도 반환.
	//==============================================================================
	pub fn getVelocity(&self) -> Vector2 {
		self.velocity
	}

	//==============================================================================
	// 중력 설정. (초당 속도 변화량)
	//==============================================================================
	pub fn setGravity(&mut self, gravity: f32) {
		self.gravity = gravity;
	}

	//==============================================================================
	// 중력 반환.
	//==============================================================================
	pub fn getGravity(&self) -> f32 {
		self.gravity
	}

	//==============================================================================
	// 바닥 높이 설정. (부모 영역 기준 로컬 Y)
	//==============================================================================
	pub fn setGroundY(&mut self, groundY: f32) {
		self.groundY = groundY;
	}

	//==============================================================================
	// 바닥 높이 반환.
	//==============================================================================
	pub fn getGroundY(&self) -> f32 {
		self.groundY
	}

	//==============================================================================
	// 좌우 이동 경계 설정. (None 이면 그 쪽은 막지 않는다)
	//==============================================================================
	pub fn setHorizontalBounds(&mut self, leftBound: Option<f32>, rightBound: Option<f32>) {
		self.leftBound = leftBound;
		self.rightBound = rightBound;
	}

	//==============================================================================
	// 접지 중인지.
	//==============================================================================
	pub fn isGrounded(&self) -> bool {
		self.isGrounded
	}

	//==============================================================================
	// 착지 알림 설정. (공중에서 바닥에 닿는 순간)
	//==============================================================================
	pub fn setLandedEvent(&mut self, landedEvent: impl FnMut() + 'static) {
		self.landedEvent = Some(Box::new(landedEvent));
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

impl Component for PlatformerBody {
	//==============================================================================
	// 공통 상태 반환.
	//==============================================================================
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	//==============================================================================
	// 공통 상태 반환. (변경용)
	//==============================================================================
	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	//==============================================================================
	// 컴포넌트 이름.
	//==============================================================================
	fn getComponentType(&self) -> &'static str {
		"PlatformerBody"
	}

	//==============================================================================
	// 갱신. (적분한 위치를 노드 로컬 위치에 넣는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		let currentPosition = context.getTransform().getLocalPosition();
		let nextPosition = self.integrate(currentPosition, timeDelta);
		context.getTransformMut().setLocalPosition(nextPosition);
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
	fn fallLandJumpAndBounds() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let heroId = nodeTree.createChild(rootId, "hero");
		nodeTree[heroId].setLocalPosition(Vector2::create(100.0, 0.0));
		let landedCount = Rc::new(Cell::new(0));
		let landedCountForEvent = landedCount.clone();
		let body = nodeTree[heroId].addComponent(PlatformerBody::new());
		body.setGroundY(100.0);
		body.setHorizontalBounds(Some(40.0), Some(120.0));
		body.setLandedEvent(move || {
			landedCountForEvent.set(landedCountForEvent.get() + 1);
		});

		// 첫 걸음: 속도 2000 * 0.1 = 200, 위치 0 + 200 * 0.1 = 20.
		nodeTree.tick(&engine, 0.1);
		assert_eq!(nodeTree[heroId].getLocalPosition(), Vector2::create(100.0, 20.0));
		assert!(!nodeTree[heroId].getComponent::<PlatformerBody>().expect("body").isGrounded());
		for _ in 0..5 {
			nodeTree.tick(&engine, 0.1);
		}
		assert_eq!(nodeTree[heroId].getLocalPosition().y, 100.0);
		assert_eq!(landedCount.get(), 1);
		let body = nodeTree[heroId].getComponentMut::<PlatformerBody>().expect("body");
		assert!(body.isGrounded());
		assert_eq!(body.getVelocity().y, 0.0);

		// 뛰기. (공중에서는 다시 못 뛴다)
		assert!(body.jump(-500.0));
		assert!(!body.jump(-500.0));
		body.setVelocityX(-1000.0);
		nodeTree.tick(&engine, 0.1);
		let heroPosition = nodeTree[heroId].getLocalPosition();
		assert_eq!(heroPosition.x, 40.0);
		assert!((heroPosition.y - (100.0 - 30.0)).abs() < 0.001);
		for _ in 0..10 {
			nodeTree.tick(&engine, 0.1);
		}
		assert_eq!(nodeTree[heroId].getLocalPosition().y, 100.0);
		assert_eq!(landedCount.get(), 2);
	}
}
