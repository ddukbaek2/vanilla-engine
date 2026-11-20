//==============================================================================
// 색상 값. (원본과 같이 문자열이다. Color::from 이나 set_fill_color 에 그대로 넘긴다)
//==============================================================================
pub struct Colors;

impl Colors {
	pub const BLACK: &'static str = "#000000";
	pub const WHITE: &'static str = "#ffffff";
	pub const VANILLA: &'static str = "#f3e5ab";
	pub const DARK_VANILLA: &'static str = "#d1c1b2";
	pub const LIGHT_VANILLA: &'static str = "#f1e9d8";
}
