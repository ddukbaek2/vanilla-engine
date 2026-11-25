//==============================================================================
// 레이아웃 제약 축. (원본 ui/autolayout/layoutconstraintaxis.js 의 LayoutConstraintAxis, UIKit 의 NSLayoutConstraint.Axis 와 같은 생각)
// - Horizontal: 가로 축. width, left, right, centerX 와 관련된 제약.
// - Vertical: 세로 축. height, top, bottom, centerY 와 관련된 제약.
// - 원본은 문자열 값("horizontal", "vertical")을 담은 얼린 객체다. 러스트 판은 열거형이다.
//==============================================================================

//==============================================================================
// 레이아웃 제약 축.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutConstraintAxis {
	// 가로 축.
	Horizontal,
	// 세로 축.
	Vertical,
}
