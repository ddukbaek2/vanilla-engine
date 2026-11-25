//==============================================================================
//! 솔버 안의 심볼 종류. (원본 ui/autolayout/layoutsymboltype.js 의 LayoutSymbolType)
//! - Invalid: 정해지지 않은 무효 심볼.
//! - External: 사용자의 LayoutVariable 하나에 하나씩 대응하는 심볼.
//! - Slack: 부등식 제약을 등식으로 바꿀 때 더하는 슬랙 변수.
//! - Error: required 가 아닌 제약의 어긋난 양을 나타내는 오차 변수.
//! - Dummy: required 등식 제약에 붙여 지울 때 찾는 표시 변수.
//! - 원본은 숫자 값(0 ~ 4)을 담은 객체다. 러스트 판은 같은 값을 매긴 열거형이다.
//!
//! # 예제
//! 심볼의 종류 읽기
//! ```rust
//! use vanilla::ui::autolayout::layoutsymbol::LayoutSymbol;
//! use vanilla::ui::autolayout::layoutsymboltype::LayoutSymbolType;
//!
//! let slack_symbol = LayoutSymbol::new(LayoutSymbolType::Slack);
//! assert_eq!(slack_symbol.get_type(), LayoutSymbolType::Slack);
//! assert_eq!(LayoutSymbol::invalid().get_type(), LayoutSymbolType::Invalid);
//! assert_eq!(LayoutSymbolType::Dummy as i32, 4);
//! ```
//==============================================================================

//==============================================================================
/// 솔버 안의 심볼 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutSymbolType {
	/// 무효.
	Invalid = 0,
	/// 사용자 변수.
	External = 1,
	/// 슬랙 변수.
	Slack = 2,
	/// 오차 변수.
	Error = 3,
	/// required 등식의 표시 변수.
	Dummy = 4,
}
