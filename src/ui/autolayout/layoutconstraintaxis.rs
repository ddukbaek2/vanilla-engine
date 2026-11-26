//==============================================================================
//! 레이아웃 제약 축. (원본 ui/autolayout/layoutconstraintaxis.js 의 LayoutConstraintAxis, UIKit 의 NSLayoutConstraint.Axis 와 같은 생각)
//! - Horizontal: 가로 축. width, left, right, centerX 와 관련된 제약.
//! - Vertical: 세로 축. height, top, bottom, centerY 와 관련된 제약.
//! - 원본은 문자열 값("horizontal", "vertical")을 담은 얼린 객체다. 러스트 판은 열거형이다.
//!
//! # 예제
//! 가로 축의 hug 우선순위만 바꾸기
//! ```rust
//! use vanilla::{LayoutConstraintAxis, LayoutPriority, UINode};
//!
//! let mut ui_node = UINode::new();
//! assert_eq!(ui_node.get_content_hugging_priority(LayoutConstraintAxis::Horizontal), LayoutPriority::DEFAULT_LOW);
//! ui_node.set_content_hugging_priority(600.0, LayoutConstraintAxis::Horizontal).unwrap();
//! assert_eq!(ui_node.get_content_hugging_priority(LayoutConstraintAxis::Horizontal), 600.0);
//! assert_eq!(ui_node.get_content_hugging_priority(LayoutConstraintAxis::Vertical), LayoutPriority::DEFAULT_LOW);
//! ```
//==============================================================================

//==============================================================================
/// 레이아웃 제약 축.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutConstraintAxis {
	/// 가로 축.
	Horizontal,
	/// 세로 축.
	Vertical,
}
