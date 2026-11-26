//==============================================================================
//! 색 이름 상수. (원본 base/colors.js)
//! - 원본 Colors 객체의 값을 연관 상수로 옮겼다. 원본처럼 "#rrggbb" 문자열이다.
//==============================================================================

//==============================================================================
/// 색상 값. (원본과 같이 문자열이다. Color::from 이나 set_fill_color 에 그대로 넘긴다)
///
/// # 예제
/// 바닐라색 문자열로 색 만들기
/// ```rust
/// use vanilla::{Color, Colors};
///
/// let background = Color::create_from_hex(Colors::VANILLA);
/// assert_eq!(background.to_hex_string(), Colors::VANILLA);
/// assert_eq!(Color::from(Colors::BLACK), Color::black());
/// ```
//==============================================================================
pub struct Colors;

impl Colors {
	/// 검은색. (#000000)
	pub const BLACK: &'static str = "#000000";
	/// 흰색. (#ffffff)
	pub const WHITE: &'static str = "#ffffff";
	/// 바닐라색. (#f3e5ab)
	pub const VANILLA: &'static str = "#f3e5ab";
	/// 어두운 바닐라색. (#d1c1b2)
	pub const DARK_VANILLA: &'static str = "#d1c1b2";
	/// 밝은 바닐라색. (#f1e9d8)
	pub const LIGHT_VANILLA: &'static str = "#f1e9d8";
}
