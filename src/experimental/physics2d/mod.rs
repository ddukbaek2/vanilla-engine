//==============================================================================
//! 2D 강체 물리. (초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다, 원본 src/experimental/physics2d/)
//! - 강체(정적, 운동학, 동적), 모양(원, 캡슐, 선분, 볼록 다각형, 사슬), 관절(거리, 회전, 용접, 바퀴), 충돌 거르기, 센서,
//!   닿음과 센서 사건, 광선과 모양 질의, 고정 걸음과 하위 걸음, 섬째 잠자기, 노드를 몸에 잇는 RigidBody2D 컴포넌트다.
//! - 기준은 Box2D 3(Erin Catto 의 강연과 공개 글)과 유니티 2D 물리(Rigidbody2D, Collider2D)의 기능이다. 남의 코드를 옮기지 않고 공개된 셈법으로 짰다.
//! - 원본 vanilla.js 의 Experimental 이름 공간 물리와 같은 API, 같은 셈이다. 셈은 두 엔진 모두 f64 이고 삼각 함수는 직접 짜서,
//!   같은 차례로 만들고 같은 step 을 부르면 결과가 비트까지 같다. (시험 comparison_matches_vanilla_js 가 원본이 쓴 덤프와 견준다)
//! - 원본과 다른 점: 벡터는 f64 의 PhysicsVector2 다. (러스트 엔진 Vector2 는 f32) 사용자 값은 Rc<dyn Any> 를 감싼 PhysicsUserData2D 다. (견줌은 Rc::ptr_eq 로 원본 === 와 같은 뜻)
//==============================================================================
// 옮긴 원문: Box2D 3 의 셈에서 옮김, MIT, THIRD_PARTY_LICENSES.md

pub mod body2d;
pub mod geometry2d;
pub mod joint2d;
pub(crate) mod narrowphase2d;
pub mod physicsdebugdraw2d;
pub mod physicsid2d;
pub mod physicsmath2d;
pub mod physicsuserdata2d;
pub mod physicsvector2;
pub mod physicsworld2d;
pub mod rigidbody2d;
pub mod shape2d;
pub(crate) mod solver2d;

#[cfg(test)]
mod tests;

pub use body2d::{Body2D, BodyDefinition2D, BodyType2D};
pub use geometry2d::{Capsule2D, Circle2D, Polygon2D, Segment2D, ShapeGeometry2D, ShapeType2D, PHYSICS2D_MAXIMUM_POLYGON_VERTICES};
pub use joint2d::{DistanceJointDefinition2D, Joint2D, JointType2D, RevoluteJointDefinition2D, WeldJointDefinition2D, WheelJointDefinition2D};
pub use physicsdebugdraw2d::PhysicsDebugDraw2D;
pub use physicsid2d::{BodyId2D, JointId2D, ShapeId2D};
pub use physicsmath2d::{PhysicsMath2D, Rotation2D};
pub use physicsuserdata2d::PhysicsUserData2D;
pub use physicsvector2::PhysicsVector2;
pub use physicsworld2d::{ContactEvent2D, PhysicsWorld2D, RayResult2D, SensorEvent2D, WorldDefinition2D};
pub use rigidbody2d::RigidBody2D;
pub use shape2d::{ChainDefinition2D, CollisionFilter2D, QueryFilter2D, Shape2D, ShapeDefinition2D};
