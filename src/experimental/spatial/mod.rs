//==============================================================================
//! 공간 나누기. (초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다, 원본 src/experimental/spatial/)
//! - 축 정렬 경계 상자(AABB2D)와 동적 AABB 나무(DynamicTree)다. 2D 물리의 넓은 단계가 쓰고, 노드 거르기와 터치 맞추기에도 쓸 수 있다.
//! - 원본 vanilla.js 의 Experimental.AABB2D, Experimental.DynamicTree 와 같은 API, 같은 셈이다. (값은 둘 다 f64)
//==============================================================================

pub mod aabb2d;
pub mod dynamictree;

pub use aabb2d::AABB2D;
pub use dynamictree::DynamicTree;
