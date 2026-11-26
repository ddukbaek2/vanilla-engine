//==============================================================================
//! 2D 뼈대 애니메이션. (원본 src/experimental/skeleton2d/, 원본 Experimental.Skeleton2D... 이름들)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 뼈대 자료(Skeleton2DData, 엔진 자체 JSON 과 DragonBones 5.x JSON 읽기), 인스턴스(Skeleton2D, 뼈, 슬롯, IK),
//!   클립과 곡선(선형, 계단, 베지어), 애니메이션 상태(트랙, 크로스페이드, 층, 사건), 노드에 붙는 컴포넌트(Skeleton2DAnimator).
//! - Spine 형식은 읽지 않는다. Spine 런타임 라이선스가 그 런타임을 넣은 제품의 사용자마다 Spine 편집기 라이선스를 요구해서다.
//! - 원본과 다른 점: 원본의 -1 번호는 Option 이다. (뼈 부모, 시간줄 대상) 자료는 `Rc<Skeleton2DData>` 로 나눠 들고,
//!   뼈, 슬롯, IK 인스턴스는 제 자료를 복사해 든다. 붙임은 두 갈래 열거(Skeleton2DAttachment)다. 슬롯 섞기는 BlendMode 다.
//!
//! # 예제
//! 엔진 JSON 으로 자료를 짓고 걷기를 반 초 돌린 뒤 뼈의 월드 자리 읽기
//! ```rust
//! use std::rc::Rc;
//! use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DAnimationState, Skeleton2DData};
//!
//! let json = serde_json::json!({
//! 	"bones": [{ "name": "root" }, { "name": "leg", "parent": "root", "rotation": 90, "length": 30 }],
//! 	"animations": { "walk": { "bones": { "leg": { "rotate": [{ "time": 0, "value": -20 }, { "time": 1, "value": 20 }] } } } }
//! });
//! let skeleton_data = Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton"));
//! let mut skeleton = Skeleton2D::new(skeleton_data.clone());
//! let mut animation_state = Skeleton2DAnimationState::new(skeleton_data.clone());
//! animation_state.set_animation(0, "walk", true, 0.0);
//! animation_state.update(0.5);
//! animation_state.apply(&mut skeleton);
//! skeleton.update_world_transform();
//! let tip = skeleton.find_bone("leg").expect("leg").local_to_world(30.0, 0.0);
//! assert!((tip.y - 30.0).abs() < 0.001);
//! ```
//==============================================================================
pub(crate) mod dragonbonesreader;
pub mod skeleton2d;
pub mod skeleton2danimation;
pub mod skeleton2danimationstate;
pub mod skeleton2danimator;
pub mod skeleton2ddata;
pub(crate) mod skeleton2djsonreader;

pub use self::skeleton2d::{Skeleton2D, Skeleton2DBone, Skeleton2DIkConstraint, Skeleton2DSlot};
pub use self::skeleton2danimation::{evaluate_skeleton2d_curve, Skeleton2DAnimation, Skeleton2DCurveType, Skeleton2DEvent, Skeleton2DPose, Skeleton2DTimeline, Skeleton2DTimelineType, SKELETON2D_BONE_VALUE_COUNT};
pub use self::skeleton2danimationstate::{Skeleton2DAnimationState, Skeleton2DFiredEvent, Skeleton2DTrackEntry};
pub use self::skeleton2danimator::Skeleton2DAnimator;
pub use self::skeleton2ddata::{Skeleton2DAttachment, Skeleton2DAttachmentType, Skeleton2DBoneData, Skeleton2DData, Skeleton2DIkConstraintData, Skeleton2DMeshAttachment, Skeleton2DRegionAttachment, Skeleton2DSkin, Skeleton2DSlotData};
