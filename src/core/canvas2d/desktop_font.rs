//==============================================================================
//! 데스크탑 Canvas2D 의 font 읽기. (크롬 CanvasRenderingContext2D 의 font 처럼 CSS font 줄임 꼴로 읽어 틀리면 무시하고, 게터는 크롬이 직렬화한 글을 돌려준다)
//! - 글은 CSS Syntax 3 처럼 토큰으로 나눈다. (주석, 빈칸, 이스케이프, 따옴표 글, 이름, 수, 길이, 퍼센트, 함수. 블링크 CSSTokenizer)
//! - 문법은 블링크 Font::ParseShorthand 다. 시스템 글꼴 낱말 하나이거나, 기울기, small-caps, 굵기, 너비를 넷까지(normal 도 하나로 센다) 두고
//!   크기, 줄높이, 이름 목록이 온다. CSS 전역 낱말(inherit 같은)과 default 는 이름이 되지 않는다.
//! - 직렬화는 블링크 BaseRenderingContext2D::font 다. 기울기가 14 도면 "italic ", 굵기는 700 이면 "bold ", 400 이 아니면 수,
//!   small-caps, 크기(유효 숫자 여섯 자리 px), 이름 목록. 줄높이, 너비, 14 도가 아닌 oblique 는 직렬화에서 빠진다.
//!   (굵기와 기울기는 블링크 FontSelectionValue 처럼 0.25 단위로 버린 값이다)
//! - 크기는 엔진 글자 캐시의 폰트 문자열 셈으로 센다. (문서에 붙이지 않은 캔버스의 부모 10px, 뿌리 16px, 고정폭 13 / 16)
//! - 시스템 글꼴은 크롬의 기본 제공자(layout_theme_font_provider_default.cc)처럼 Arial 16px, 컨트롤 셋은 2pt 작다.
//!   윈도우 크롬의 small-caption, menu, status-bar 는 운영체제가 정한 글꼴이라 다를 수 있다.
//==============================================================================
use crate::core::textstringtexturecache::{compute_canvas_font_size, evaluate_css_math_angle, evaluate_css_math_number};

/// 기본 글꼴. (원본 font 의 처음 값)
const DEFAULT_FONT: &str = "10px sans-serif";
/// 시스템 글꼴 낱말. (블링크 IsValidSystemFont, caption 부터 status-bar 까지)
const SYSTEM_FONT_KEYWORD_LIST: [&str; 9] = ["caption", "icon", "menu", "message-box", "small-caption", "-webkit-mini-control", "-webkit-small-control", "-webkit-control", "status-bar"];
/// 일반 이름. (블링크 ConsumeGenericFamily, serif 부터 math 까지)
const GENERIC_FAMILY_LIST: [&str; 8] = ["serif", "sans-serif", "cursive", "fantasy", "monospace", "system-ui", "-webkit-body", "math"];
/// 따옴표를 쳐야 하는 일반 이름. (블링크 FontFamily::InferredTypeFor)
const INFERRED_GENERIC_FAMILY_LIST: [&str; 7] = ["cursive", "fantasy", "monospace", "sans-serif", "serif", "system-ui", "math"];
/// 크기 낱말. (블링크 ConsumeFontSize, xx-small 부터 -webkit-xxx-large 까지와 math)
const FONT_SIZE_KEYWORD_LIST: [&str; 12] = ["xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large", "xxx-large", "smaller", "larger", "-webkit-xxx-large", "math"];
/// 너비 낱말. (블링크 ConsumeFontStretchKeywordOnly, normal 은 앞에서 먹는다)
const FONT_STRETCH_KEYWORD_LIST: [&str; 8] = ["ultra-condensed", "extra-condensed", "condensed", "semi-condensed", "semi-expanded", "expanded", "extra-expanded", "ultra-expanded"];
/// CSS 전역 낱말. (블링크 IsCSSWideKeyword, 크롬 158 은 revert-rule 도 켜져 있다)
const CSS_WIDE_KEYWORD_LIST: [&str; 6] = ["initial", "inherit", "unset", "revert", "revert-layer", "revert-rule"];
/// 기울기 14 도와 굵기 400, 700 의 FontSelectionValue 속값. (값 x 4)
const ITALIC_SLOPE_BACKING: i32 = 56;
const NORMAL_WEIGHT_BACKING: i32 = 1600;
const BOLD_WEIGHT_BACKING: i32 = 2800;

//==============================================================================
/// 읽은 캔버스 글꼴.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasFont {
	/// 받은 글. (크롬처럼 같은 글을 다시 받으면 다시 읽지 않는다)
	pub(crate) source_font: String,
	/// font 게터가 돌려줄 글. (크롬의 직렬화)
	pub(crate) serialized_font: String,
	/// 엔진 글자 캐시가 읽을 글. (직렬화에서 빠지는 oblique 기울기를 담는다)
	pub(crate) drawing_font: String,
}

impl CanvasFont {
	//==============================================================================
	/// 처음 글꼴. ("10px sans-serif")
	//==============================================================================
	pub(crate) fn create_default() -> CanvasFont {
		CanvasFont {
			source_font: String::from(DEFAULT_FONT),
			serialized_font: String::from(DEFAULT_FONT),
			drawing_font: String::from(DEFAULT_FONT),
		}
	}
}

//==============================================================================
/// 캔버스 font 글 읽기. (크롬 font 세터처럼 CSS font 줄임 꼴이 아니면 None)
//==============================================================================
pub(crate) fn parse_canvas_font(font_string: &str) -> Option<CanvasFont> {
	let mut stream = CssTokenStream {
		token_list: tokenize_css(font_string),
		position: 0,
	};
	stream.skip_whitespace();
	if let Some(keyword) = stream.peek_keyword() {
		if SYSTEM_FONT_KEYWORD_LIST.contains(&keyword.as_str()) {
			stream.consume_including_whitespace();
			if !stream.is_at_end() {
				return None;
			}
			// 크롬 기본 제공자: 컨트롤 셋은 기본 글자 크기(16)보다 2pt(96dpi 에서 2 / 72 x 96 px) 작다.
			let font_size = if keyword.ends_with("-control") {
				16.0 - (2.0 / 72.0) * 96.0
			}
			else {
				16.0
			};
			let serialized_font = format!("{}px Arial", format_css_number(font_size));
			return Some(CanvasFont {
				source_font: font_string.to_string(),
				serialized_font: serialized_font.clone(),
				drawing_font: serialized_font,
			});
		}
	}
	// 기울기, small-caps, 굵기, 너비. (차례 없이 넷까지, normal 도 하나를 쓴다)
	let mut slope_backing: Option<i32> = None;
	let mut is_small_caps = false;
	let mut weight_backing: Option<i32> = None;
	let mut has_stretch = false;
	for _ in 0..4 {
		let keyword = match stream.peek() {
			Some(_) => {
				stream.peek_keyword().unwrap_or_default()
			}
			None => {
				break;
			}
		};
		if keyword == "normal" {
			stream.consume_including_whitespace();
			continue;
		}
		if slope_backing.is_none() && (keyword == "italic" || keyword == "oblique") {
			slope_backing = Some(consume_font_style(&mut stream, keyword == "oblique")?);
			continue;
		}
		if !is_small_caps && keyword == "small-caps" {
			stream.consume_including_whitespace();
			is_small_caps = true;
			continue;
		}
		if weight_backing.is_none() {
			if let Some(consumed_backing) = consume_font_weight(&mut stream) {
				weight_backing = Some(consumed_backing);
				continue;
			}
		}
		if has_stretch || !FONT_STRETCH_KEYWORD_LIST.contains(&keyword.as_str()) {
			break;
		}
		stream.consume_including_whitespace();
		has_stretch = true;
	}
	if stream.is_at_end() {
		return None;
	}
	let size_text = consume_font_size(&mut stream)?;
	if stream.is_at_end() {
		return None;
	}
	if stream.peek() == Some(&CssToken::Delimiter { character: '/' }) {
		stream.consume_including_whitespace();
		if !consume_line_height(&mut stream) {
			return None;
		}
	}
	let family_list = consume_font_family(&mut stream)?;
	if !stream.is_at_end() {
		return None;
	}
	let is_monospace_only = family_list.len() == 1 && family_list[0] == FamilyName::Generic(String::from("monospace"));
	let font_size = compute_canvas_font_size(&size_text, is_monospace_only)?;
	let slope_backing = slope_backing.unwrap_or(0);
	let weight_backing = weight_backing.unwrap_or(NORMAL_WEIGHT_BACKING);
	let mut serialized_font = String::new();
	if slope_backing == ITALIC_SLOPE_BACKING {
		serialized_font.push_str("italic ");
	}
	if weight_backing == BOLD_WEIGHT_BACKING {
		serialized_font.push_str("bold ");
	}
	else if weight_backing != NORMAL_WEIGHT_BACKING {
		serialized_font.push_str(&format!("{} ", (weight_backing as f32 / 4.0) as i32));
	}
	if is_small_caps {
		serialized_font.push_str("small-caps ");
	}
	serialized_font.push_str(&format_css_number(font_size));
	serialized_font.push_str("px ");
	let serialized_family_list: Vec<String> = family_list.iter().map(serialize_family_name).collect();
	serialized_font.push_str(&serialized_family_list.join(", "));
	// 14 도가 아닌 기울기는 직렬화에서 빠지지만 크롬은 기울여 그린다. (엔진 글자 캐시는 oblique 를 italic 처럼 그린다)
	let drawing_font = if slope_backing != 0 && slope_backing != ITALIC_SLOPE_BACKING {
		format!("oblique {}", serialized_font)
	}
	else {
		serialized_font.clone()
	};
	Some(CanvasFont {
		source_font: font_string.to_string(),
		serialized_font,
		drawing_font,
	})
}

//==============================================================================
/// 기울기 읽기. (italic 은 14 도, oblique 는 각도가 없으면 14 도. FontSelectionValue 속값, 틀린 각도면 None)
/// - 적힌 각도는 제 단위의 수가 -90 ~ 90 이어야 하고(블링크 IsAngleWithinLimits), 0 이면 normal 이다. 함수는 셈한 도가 0 일 때만 normal.
/// - 각도가 아닌 것이 오면 oblique 만 먹고 그것은 남긴다.
//==============================================================================
fn consume_font_style(stream: &mut CssTokenStream, is_oblique: bool) -> Option<i32> {
	stream.consume_including_whitespace();
	if !is_oblique {
		return Some(ITALIC_SLOPE_BACKING);
	}
	match stream.peek().cloned() {
		Some(CssToken::Dimension { value, unit, .. }) => {
			let unit_degree = match unit.to_ascii_lowercase().as_str() {
				"deg" => {
					1.0
				}
				"grad" => {
					0.9
				}
				"rad" => {
					180.0 / std::f64::consts::PI
				}
				"turn" => {
					360.0
				}
				_ => {
					return Some(ITALIC_SLOPE_BACKING);
				}
			};
			stream.consume_including_whitespace();
			if !(-90.0..=90.0).contains(&value) {
				return None;
			}
			if value == 0.0 {
				return Some(0);
			}
			Some(convert_font_selection_backing(value * unit_degree))
		}
		Some(CssToken::Function { text }) => {
			match evaluate_css_math_angle(&text) {
				Some(degree) => {
					stream.consume_including_whitespace();
					if degree == 0.0 {
						Some(0)
					}
					else {
						Some(convert_font_selection_backing(degree))
					}
				}
				None => {
					Some(ITALIC_SLOPE_BACKING)
				}
			}
		}
		_ => {
			Some(ITALIC_SLOPE_BACKING)
		}
	}
}

//==============================================================================
/// 굵기 읽기. (bold, bolder 는 700, lighter 는 100(부모 400 에서), 1 ~ 1000 의 수, 수가 되는 함수. FontSelectionValue 속값, 아니면 먹지 않고 None)
/// - 함수 값은 크롬처럼 NaN 은 0 으로 두고 1 ~ 1000 으로 자른다.
//==============================================================================
fn consume_font_weight(stream: &mut CssTokenStream) -> Option<i32> {
	let weight = match stream.peek()? {
		CssToken::Ident { name } => {
			match name.to_ascii_lowercase().as_str() {
				"bold" | "bolder" => {
					700.0
				}
				"lighter" => {
					100.0
				}
				_ => {
					return None;
				}
			}
		}
		CssToken::Number { value, .. } => {
			if !(1.0..=1000.0).contains(value) {
				return None;
			}
			*value
		}
		CssToken::Function { text } => {
			let number = evaluate_css_math_number(text)?;
			let number = if number.is_nan() {
				0.0
			}
			else {
				number
			};
			number.clamp(1.0, 1000.0)
		}
		_ => {
			return None;
		}
	};
	stream.consume_including_whitespace();
	Some(convert_font_selection_backing(weight))
}

//==============================================================================
/// 크기 읽기. (엔진 글자 캐시가 셀 소문자 글: 크기 낱말, 0, 음수가 아닌 길이와 퍼센트, 함수. 아니면 None)
//==============================================================================
fn consume_font_size(stream: &mut CssTokenStream) -> Option<String> {
	let size_text = match stream.peek()? {
		CssToken::Ident { name } => {
			let keyword = name.to_ascii_lowercase();
			if !FONT_SIZE_KEYWORD_LIST.contains(&keyword.as_str()) {
				return None;
			}
			// -webkit-xxx-large 는 xxx-large 와 같고, math 는 부모 크기다. (math-depth 가 바뀌지 않은 캔버스)
			match keyword.as_str() {
				"-webkit-xxx-large" => {
					String::from("xxx-large")
				}
				"math" => {
					String::from("100%")
				}
				_ => {
					keyword
				}
			}
		}
		CssToken::Number { value, .. } => {
			if *value != 0.0 {
				return None;
			}
			String::from("0px")
		}
		CssToken::Dimension { text, unit, .. } => {
			format!("{}{}", text, unit.to_ascii_lowercase())
		}
		CssToken::Percentage { text, .. } => {
			format!("{}%", text)
		}
		CssToken::Function { text } => {
			text.clone()
		}
		_ => {
			return None;
		}
	};
	compute_canvas_font_size(&size_text, false)?;
	stream.consume_including_whitespace();
	Some(size_text)
}

//==============================================================================
/// 줄높이 읽기. (normal, 음수가 아닌 수, 길이, 퍼센트, 수나 길이가 되는 함수. 캔버스는 쓰지 않고 맞는지만 본다)
//==============================================================================
fn consume_line_height(stream: &mut CssTokenStream) -> bool {
	let is_valid = match stream.peek() {
		Some(CssToken::Ident { name }) => {
			name.eq_ignore_ascii_case("normal")
		}
		Some(CssToken::Number { value, .. }) => {
			*value >= 0.0
		}
		Some(CssToken::Dimension { text, unit, .. }) => {
			compute_canvas_font_size(&format!("{}{}", text, unit.to_ascii_lowercase()), false).is_some()
		}
		Some(CssToken::Percentage { text, .. }) => {
			compute_canvas_font_size(&format!("{}%", text), false).is_some()
		}
		Some(CssToken::Function { text }) => {
			evaluate_css_math_number(text).is_some() || compute_canvas_font_size(text, false).is_some()
		}
		_ => {
			false
		}
	};
	if is_valid {
		stream.consume_including_whitespace();
	}
	is_valid
}

//==============================================================================
/// 글꼴 이름 하나. (따옴표 없는 일반 이름은 소문자 낱말)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
enum FamilyName {
	/// 일반 이름. ("sans-serif" 같은 것)
	Generic(String),
	/// 이름. (따옴표 글이나 이어 적은 낱말들)
	Named(String),
}

//==============================================================================
/// 이름 목록 읽기. (쉼표로 이은 일반 이름, 따옴표 글, 빈칸으로 이은 낱말들. 틀리면 None)
//==============================================================================
fn consume_font_family(stream: &mut CssTokenStream) -> Option<Vec<FamilyName>> {
	let mut family_list = Vec::new();
	loop {
		match stream.peek().cloned() {
			Some(CssToken::Ident { name }) => {
				let keyword = name.to_ascii_lowercase();
				if GENERIC_FAMILY_LIST.contains(&keyword.as_str()) {
					stream.consume_including_whitespace();
					family_list.push(FamilyName::Generic(keyword));
				}
				else {
					// 이어 적은 낱말은 빈칸 하나로 잇는다. 낱말 하나가 CSS 전역 낱말이나 default 면 이름이 아니다. (블링크 ConcatenateFamilyName)
					let mut word_list: Vec<String> = Vec::new();
					while let Some(CssToken::Ident { name }) = stream.peek() {
						word_list.push(name.clone());
						stream.consume_including_whitespace();
					}
					if word_list.len() == 1 && (is_css_wide_keyword(&word_list[0]) || word_list[0].eq_ignore_ascii_case("default")) {
						return None;
					}
					family_list.push(FamilyName::Named(word_list.join(" ")));
				}
			}
			Some(CssToken::QuotedString { value }) => {
				stream.consume_including_whitespace();
				family_list.push(FamilyName::Named(value));
			}
			_ => {
				return None;
			}
		}
		if stream.peek() != Some(&CssToken::Comma) {
			return Some(family_list);
		}
		stream.consume_including_whitespace();
	}
}

//==============================================================================
/// 이름 하나 직렬화. (블링크 SerializeFontFamily: CSS 전역 낱말, default, 일반 이름, CSS 식별자가 아닌 이름은 따옴표를 친다)
/// - 맥 크롬은 이름 BlinkMacSystemFont 를 이름 "system-ui" 로 바꾼다. (style_builder_converter.cc 의 ConvertFontFamily)
//==============================================================================
fn serialize_family_name(family_name: &FamilyName) -> String {
	let name = match family_name {
		FamilyName::Generic(keyword) => {
			return keyword.clone();
		}
		FamilyName::Named(name) => {
			if cfg!(target_os = "macos") && name == "BlinkMacSystemFont" {
				"system-ui"
			}
			else {
				name.as_str()
			}
		}
	};
	let is_quote_needed = is_css_wide_keyword(name) || name.eq_ignore_ascii_case("default") || INFERRED_GENERIC_FAMILY_LIST.contains(&name.to_ascii_lowercase().as_str()) || !is_css_identifier(name);
	if !is_quote_needed {
		return name.to_string();
	}
	// 따옴표 글: 제어 문자는 "\16진 ", 따옴표와 역슬래시는 앞에 역슬래시. (블링크 SerializeString)
	let mut quoted_name = String::from("\"");
	for character in name.chars() {
		if (character as u32) <= 0x1f || character as u32 == 0x7f {
			quoted_name.push_str(&format!("\\{:x} ", character as u32));
		}
		else if character == '"' || character == '\\' {
			quoted_name.push('\\');
			quoted_name.push(character);
		}
		else {
			quoted_name.push(character);
		}
	}
	quoted_name.push('"');
	quoted_name
}

//==============================================================================
/// CSS 전역 낱말인지. (대소문자를 가리지 않는다)
//==============================================================================
fn is_css_wide_keyword(word: &str) -> bool {
	CSS_WIDE_KEYWORD_LIST.iter().any(|keyword| word.eq_ignore_ascii_case(keyword))
}

//==============================================================================
/// 따옴표 없이 적을 수 있는 CSS 식별자인지. (블링크 IsCSSTokenizerIdentifier: 하이픈 하나까지, 이름 첫 글자, 이름 글자들)
//==============================================================================
fn is_css_identifier(name: &str) -> bool {
	let mut character_iterator = name.chars().peekable();
	if character_iterator.peek() == Some(&'-') {
		character_iterator.next();
	}
	match character_iterator.next() {
		Some(character) if is_name_start_character(character) => {}
		_ => {
			return false;
		}
	}
	character_iterator.all(is_name_character)
}

//==============================================================================
/// 블링크 FontSelectionValue 속값. (값 x 4 를 0 쪽으로 버리고 i16 으로 자른다, NaN 은 0)
//==============================================================================
fn convert_font_selection_backing(value: f64) -> i32 {
	(value * 4.0) as i16 as i32
}

//==============================================================================
/// 크롬 수 직렬화. (블링크 StringBuilder::AppendNumber(float): 유효 숫자 여섯 자리로 반은 올리고, 소수점 아래 끝 0 을 뗀다.
/// 10 의 -7 승보다 작으면 "1.00000e-7" 처럼 지수 꼴이고 그때는 0 을 떼지 않는다. double-conversion 의 ToPrecision)
//==============================================================================
fn format_css_number(value: f32) -> String {
	let value = value as f64;
	if value == 0.0 || !value.is_finite() {
		return String::from("0");
	}
	let is_negative = value < 0.0;
	// f32 에서 온 수는 소수 전개가 짧아 유효 숫자 일흔한 자리면 정확하다.
	let exact_text = format!("{:.70e}", value.abs());
	let (mantissa_text, exponent_text) = exact_text.split_once('e').unwrap_or((&exact_text, "0"));
	let mut exponent: i32 = exponent_text.parse().unwrap_or(0);
	let digit_list: Vec<u8> = mantissa_text.bytes().filter(|byte| byte.is_ascii_digit()).map(|byte| byte - b'0').collect();
	let mut rounded_digit_list: Vec<u8> = digit_list[..6].to_vec();
	if digit_list[6] >= 5 {
		let mut digit_index = 5;
		loop {
			if rounded_digit_list[digit_index] < 9 {
				rounded_digit_list[digit_index] += 1;
				break;
			}
			rounded_digit_list[digit_index] = 0;
			if digit_index == 0 {
				rounded_digit_list.insert(0, 1);
				rounded_digit_list.pop();
				exponent += 1;
				break;
			}
			digit_index -= 1;
		}
	}
	let digit_text: String = rounded_digit_list.iter().map(|digit| (b'0' + digit) as char).collect();
	let decimal_point = exponent + 1;
	let mut text = String::new();
	if is_negative {
		text.push('-');
	}
	if -decimal_point + 1 > 6 || decimal_point > 6 {
		text.push_str(&digit_text[..1]);
		text.push('.');
		text.push_str(&digit_text[1..]);
		text.push('e');
		if exponent >= 0 {
			text.push('+');
		}
		text.push_str(&exponent.to_string());
		return text;
	}
	if decimal_point <= 0 {
		text.push_str("0.");
		for _ in 0..-decimal_point {
			text.push('0');
		}
		text.push_str(&digit_text);
	}
	else {
		text.push_str(&digit_text[..decimal_point as usize]);
		text.push('.');
		text.push_str(&digit_text[decimal_point as usize..]);
	}
	let trimmed_length = text.trim_end_matches('0').trim_end_matches('.').len();
	text.truncate(trimmed_length);
	text
}

//==============================================================================
/// CSS 토큰. (CSS Syntax 3, 글꼴 문법이 가르는 것만)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
enum CssToken {
	/// 빈칸.
	Whitespace,
	/// 이름. (이스케이프를 푼 글)
	Ident {
		name: String,
	},
	/// 함수 하나. (이름부터 닫는 괄호까지, 이스케이프를 풀고 주석을 뺀 글)
	Function {
		text: String,
	},
	/// 따옴표 글. (이스케이프를 푼 글)
	QuotedString {
		value: String,
	},
	/// 수.
	Number {
		value: f64,
		text: String,
	},
	/// 퍼센트. (적힌 수 글)
	Percentage {
		value: f64,
		text: String,
	},
	/// 단위가 붙은 수. (적힌 수 글과 이스케이프를 푼 단위)
	Dimension {
		value: f64,
		text: String,
		unit: String,
	},
	/// 쉼표.
	Comma,
	/// 한 글자 기호. ('/', '+' 같은 것)
	Delimiter {
		character: char,
	},
	/// 여는 괄호.
	LeftParenthesis,
	/// 닫는 괄호.
	RightParenthesis,
	/// 글꼴 문법에 없는 토큰. (url(), 줄이 바뀐 따옴표 글, 닫히지 않은 함수, 함수 안의 따옴표 글, CDC)
	Other,
}

//==============================================================================
/// 읽는 중인 토큰 목록.
//==============================================================================
struct CssTokenStream {
	token_list: Vec<CssToken>,
	position: usize,
}

impl CssTokenStream {
	//==============================================================================
	/// 다음 토큰. (끝이면 None)
	//==============================================================================
	fn peek(&self) -> Option<&CssToken> {
		self.token_list.get(self.position)
	}

	//==============================================================================
	/// 다음 토큰이 이름이면 그 소문자. (블링크 CSSValueID 처럼 ASCII 대소문자를 가리지 않는다)
	//==============================================================================
	fn peek_keyword(&self) -> Option<String> {
		match self.peek() {
			Some(CssToken::Ident { name }) => {
				Some(name.to_ascii_lowercase())
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 빈칸 넘기기.
	//==============================================================================
	fn skip_whitespace(&mut self) {
		while self.peek() == Some(&CssToken::Whitespace) {
			self.position += 1;
		}
	}

	//==============================================================================
	/// 토큰 하나와 뒤의 빈칸 먹기.
	//==============================================================================
	fn consume_including_whitespace(&mut self) {
		self.position += 1;
		self.skip_whitespace();
	}

	//==============================================================================
	/// 다 읽었는지.
	//==============================================================================
	fn is_at_end(&self) -> bool {
		self.position >= self.token_list.len()
	}
}

//==============================================================================
/// 글을 CSS 토큰으로. (주석은 빠지고, 함수는 닫는 괄호까지 토큰 하나다)
//==============================================================================
fn tokenize_css(text: &str) -> Vec<CssToken> {
	// 미리 다듬기: CR LF, CR, FF 는 LF, NULL 은 U+FFFD. (CSS Syntax 3 의 3.3)
	let mut character_list: Vec<char> = Vec::with_capacity(text.len());
	let mut character_iterator = text.chars().peekable();
	while let Some(character) = character_iterator.next() {
		match character {
			'\r' => {
				if character_iterator.peek() == Some(&'\n') {
					character_iterator.next();
				}
				character_list.push('\n');
			}
			'\u{c}' => {
				character_list.push('\n');
			}
			'\0' => {
				character_list.push('\u{fffd}');
			}
			_ => {
				character_list.push(character);
			}
		}
	}
	let mut tokenizer = CssTokenizer {
		character_list,
		position: 0,
	};
	let mut token_list = Vec::new();
	while let Some(token) = tokenizer.next_token() {
		token_list.push(token);
	}
	token_list
}

//==============================================================================
/// CSS 토큰 나누개. (CSS Syntax 3 의 4 장)
//==============================================================================
struct CssTokenizer {
	character_list: Vec<char>,
	position: usize,
}

impl CssTokenizer {
	//==============================================================================
	/// 지금 자리에서 offset 만큼 뒤의 글자. (끝이면 None)
	//==============================================================================
	fn peek_character(&self, offset: usize) -> Option<char> {
		self.character_list.get(self.position + offset).copied()
	}

	//==============================================================================
	/// 다음 토큰. (끝이면 None)
	//==============================================================================
	fn next_token(&mut self) -> Option<CssToken> {
		self.consume_comments();
		let character = self.peek_character(0)?;
		if is_whitespace_character(character) {
			while self.peek_character(0).map(is_whitespace_character).unwrap_or(false) {
				self.position += 1;
			}
			return Some(CssToken::Whitespace);
		}
		let token = match character {
			'"' | '\'' => {
				self.position += 1;
				self.consume_string(character)
			}
			'(' => {
				self.position += 1;
				CssToken::LeftParenthesis
			}
			')' => {
				self.position += 1;
				CssToken::RightParenthesis
			}
			',' => {
				self.position += 1;
				CssToken::Comma
			}
			'+' | '.' if self.starts_number(0) => {
				self.consume_numeric()
			}
			'-' if self.starts_number(0) => {
				self.consume_numeric()
			}
			'-' if self.peek_character(1) == Some('-') && self.peek_character(2) == Some('>') => {
				self.position += 3;
				CssToken::Other
			}
			'-' | '\\' if self.starts_identifier(0) => {
				self.consume_ident_like()
			}
			_ if character.is_ascii_digit() => {
				self.consume_numeric()
			}
			_ if is_name_start_character(character) => {
				self.consume_ident_like()
			}
			_ => {
				self.position += 1;
				CssToken::Delimiter {
					character,
				}
			}
		};
		Some(token)
	}

	//==============================================================================
	/// 주석 넘기기. (닫히지 않은 주석은 끝까지)
	//==============================================================================
	fn consume_comments(&mut self) {
		while self.peek_character(0) == Some('/') && self.peek_character(1) == Some('*') {
			self.position += 2;
			loop {
				match self.peek_character(0) {
					None => {
						return;
					}
					Some('*') if self.peek_character(1) == Some('/') => {
						self.position += 2;
						break;
					}
					Some(_) => {
						self.position += 1;
					}
				}
			}
		}
	}

	//==============================================================================
	/// 따옴표 글. (여는 따옴표 다음부터, 끝에 닿으면 거기까지. 줄이 바뀌면 틀린 글)
	//==============================================================================
	fn consume_string(&mut self, ending_character: char) -> CssToken {
		let mut value = String::new();
		loop {
			match self.peek_character(0) {
				None => {
					return CssToken::QuotedString {
						value,
					};
				}
				Some(character) if character == ending_character => {
					self.position += 1;
					return CssToken::QuotedString {
						value,
					};
				}
				Some('\n') => {
					return CssToken::Other;
				}
				Some('\\') => {
					match self.peek_character(1) {
						None => {
							self.position += 1;
						}
						Some('\n') => {
							self.position += 2;
						}
						Some(_) => {
							self.position += 1;
							value.push(self.consume_escaped_character());
						}
					}
				}
				Some(character) => {
					value.push(character);
					self.position += 1;
				}
			}
		}
	}

	//==============================================================================
	/// 수, 퍼센트, 단위가 붙은 수.
	//==============================================================================
	fn consume_numeric(&mut self) -> CssToken {
		let start_position = self.position;
		if matches!(self.peek_character(0), Some('+') | Some('-')) {
			self.position += 1;
		}
		while self.peek_character(0).map(|character| character.is_ascii_digit()).unwrap_or(false) {
			self.position += 1;
		}
		if self.peek_character(0) == Some('.') && self.peek_character(1).map(|character| character.is_ascii_digit()).unwrap_or(false) {
			self.position += 1;
			while self.peek_character(0).map(|character| character.is_ascii_digit()).unwrap_or(false) {
				self.position += 1;
			}
		}
		if matches!(self.peek_character(0), Some('e') | Some('E')) {
			let exponent_digit_offset = if matches!(self.peek_character(1), Some('+') | Some('-')) {
				2
			}
			else {
				1
			};
			if self.peek_character(exponent_digit_offset).map(|character| character.is_ascii_digit()).unwrap_or(false) {
				self.position += exponent_digit_offset;
				while self.peek_character(0).map(|character| character.is_ascii_digit()).unwrap_or(false) {
					self.position += 1;
				}
			}
		}
		let text: String = self.character_list[start_position..self.position].iter().collect();
		let value = text.parse::<f64>().unwrap_or(0.0);
		if self.starts_identifier(0) {
			let unit = self.consume_name();
			return CssToken::Dimension {
				value,
				text,
				unit,
			};
		}
		if self.peek_character(0) == Some('%') {
			self.position += 1;
			return CssToken::Percentage {
				value,
				text,
			};
		}
		CssToken::Number {
			value,
			text,
		}
	}

	//==============================================================================
	/// 이름이나 함수. (url( 은 글꼴에 없는 토큰이다)
	//==============================================================================
	fn consume_ident_like(&mut self) -> CssToken {
		let name = self.consume_name();
		if self.peek_character(0) != Some('(') {
			return CssToken::Ident {
				name,
			};
		}
		self.position += 1;
		if name.eq_ignore_ascii_case("url") {
			return CssToken::Other;
		}
		self.consume_function(name)
	}

	//==============================================================================
	/// 함수 안을 닫는 괄호까지 먹어 글로 잇기. (빈칸은 하나로, 닫히지 않았거나 따옴표 글이 있으면 글꼴에 없는 토큰)
	//==============================================================================
	fn consume_function(&mut self, name: String) -> CssToken {
		let mut text = name;
		text.push('(');
		let mut depth = 0;
		loop {
			let token = match self.next_token() {
				Some(token) => {
					token
				}
				None => {
					return CssToken::Other;
				}
			};
			match token {
				CssToken::RightParenthesis => {
					text.push(')');
					if depth == 0 {
						return CssToken::Function {
							text,
						};
					}
					depth -= 1;
				}
				CssToken::LeftParenthesis => {
					text.push('(');
					depth += 1;
				}
				CssToken::Whitespace => {
					text.push(' ');
				}
				CssToken::Ident { name } => {
					text.push_str(&name);
				}
				CssToken::Function { text: inner_text } => {
					text.push_str(&inner_text);
				}
				CssToken::Number { text: number_text, .. } => {
					text.push_str(&number_text);
				}
				CssToken::Percentage { text: number_text, .. } => {
					text.push_str(&number_text);
					text.push('%');
				}
				CssToken::Dimension { text: number_text, unit, .. } => {
					text.push_str(&number_text);
					text.push_str(&unit);
				}
				CssToken::Comma => {
					text.push(',');
				}
				CssToken::Delimiter { character } => {
					text.push(character);
				}
				CssToken::QuotedString { .. } | CssToken::Other => {
					return CssToken::Other;
				}
			}
		}
	}

	//==============================================================================
	/// 이름 글자들. (이스케이프를 푼다)
	//==============================================================================
	fn consume_name(&mut self) -> String {
		let mut name = String::new();
		loop {
			match self.peek_character(0) {
				Some(character) if is_name_character(character) => {
					name.push(character);
					self.position += 1;
				}
				Some('\\') if self.is_valid_escape(0) => {
					self.position += 1;
					name.push(self.consume_escaped_character());
				}
				_ => {
					return name;
				}
			}
		}
	}

	//==============================================================================
	/// 역슬래시 다음의 이스케이프 한 글자. (16진 여섯 자리까지와 뒤의 빈칸 하나, 0, 서로게이트, U+10FFFF 너머, 끝은 U+FFFD)
	//==============================================================================
	fn consume_escaped_character(&mut self) -> char {
		let character = match self.peek_character(0) {
			Some(character) => {
				character
			}
			None => {
				return '\u{fffd}';
			}
		};
		if !character.is_ascii_hexdigit() {
			self.position += 1;
			return character;
		}
		let mut code_point: u32 = 0;
		let mut digit_count = 0;
		while digit_count < 6 {
			match self.peek_character(0).and_then(|character| character.to_digit(16)) {
				Some(digit) => {
					code_point = code_point * 16 + digit;
					self.position += 1;
					digit_count += 1;
				}
				None => {
					break;
				}
			}
		}
		if self.peek_character(0).map(is_whitespace_character).unwrap_or(false) {
			self.position += 1;
		}
		if code_point == 0 {
			return '\u{fffd}';
		}
		char::from_u32(code_point).unwrap_or('\u{fffd}')
	}

	//==============================================================================
	/// offset 자리부터 수가 시작하는지. (부호, 숫자, 점 다음 숫자)
	//==============================================================================
	fn starts_number(&self, offset: usize) -> bool {
		let is_digit = |character: Option<char>| character.map(|character| character.is_ascii_digit()).unwrap_or(false);
		match self.peek_character(offset) {
			Some('+') | Some('-') => {
				is_digit(self.peek_character(offset + 1)) || (self.peek_character(offset + 1) == Some('.') && is_digit(self.peek_character(offset + 2)))
			}
			Some('.') => {
				is_digit(self.peek_character(offset + 1))
			}
			character => {
				is_digit(character)
			}
		}
	}

	//==============================================================================
	/// offset 자리부터 이름이 시작하는지. (하이픈 다음의 이름 첫 글자나 하이픈이나 이스케이프, 이름 첫 글자, 이스케이프)
	//==============================================================================
	fn starts_identifier(&self, offset: usize) -> bool {
		match self.peek_character(offset) {
			Some('-') => {
				match self.peek_character(offset + 1) {
					Some(character) if is_name_start_character(character) || character == '-' => {
						true
					}
					Some('\\') => {
						self.is_valid_escape(offset + 1)
					}
					_ => {
						false
					}
				}
			}
			Some('\\') => {
				self.is_valid_escape(offset)
			}
			Some(character) => {
				is_name_start_character(character)
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// offset 자리가 맞는 이스케이프인지. (역슬래시 다음이 줄바꿈이 아니다, 끝이어도 된다)
	//==============================================================================
	fn is_valid_escape(&self, offset: usize) -> bool {
		self.peek_character(offset) == Some('\\') && self.peek_character(offset + 1) != Some('\n')
	}
}

//==============================================================================
/// CSS 빈칸인지. (미리 다듬은 뒤의 줄바꿈, 탭, 빈칸)
//==============================================================================
fn is_whitespace_character(character: char) -> bool {
	matches!(character, '\n' | '\t' | ' ')
}

//==============================================================================
/// CSS 이름 첫 글자인지. (영문자, 밑줄, ASCII 밖)
//==============================================================================
fn is_name_start_character(character: char) -> bool {
	character.is_ascii_alphabetic() || character == '_' || character as u32 >= 0x80
}

//==============================================================================
/// CSS 이름 글자인지. (이름 첫 글자, 숫자, 하이픈)
//==============================================================================
fn is_name_character(character: char) -> bool {
	is_name_start_character(character) || character.is_ascii_digit() || character == '-'
}

//==============================================================================
/// 시험. (기대값은 헤드리스 크롬 158 맥에서 문서에 붙이지 않은 캔버스의 font 에 넣고 읽은 값)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 넣은 글과 크롬이 돌려준 글. (None 은 크롬이 무시해 앞 값이 남은 것)
	//==============================================================================
	const CHROME_FONT_TABLE: &[(&str, Option<&str>)] = &[
		("10px sans-serif", Some("10px sans-serif")),
		("bold 16px sans-serif", Some("bold 16px sans-serif")),
		("italic bold 12px serif", Some("italic bold 12px serif")),
		("BOLD 10PX SANS-SERIF", Some("bold 10px sans-serif")),
		("10px Arial", Some("10px Arial")),
		("10px 'Arial'", Some("10px Arial")),
		("10px \"Noto Sans\"", Some("10px \"Noto Sans\"")),
		("10px Noto Sans", Some("10px \"Noto Sans\"")),
		("10px  Noto   Sans", Some("10px \"Noto Sans\"")),
		("10px 'serif'", Some("10px \"serif\"")),
		("10px serif, 'Noto Sans', monospace", Some("10px serif, \"Noto Sans\", monospace")),
		("10px SERIF", Some("10px serif")),
		("10px a,b", Some("10px a, b")),
		("10px a , b", Some("10px a, b")),
		("100 10px a", Some("100 10px a")),
		("400 10px a", Some("10px a")),
		("700 10px a", Some("bold 10px a")),
		("550 10px a", Some("550 10px a")),
		("500.5 10px a", Some("500 10px a")),
		("500.9 10px a", Some("500 10px a")),
		("700.1 10px a", Some("bold 10px a")),
		("400.2 10px a", Some("10px a")),
		("1 10px a", Some("1 10px a")),
		("1000 10px a", Some("1000 10px a")),
		("0 10px a", None),
		("1001 10px a", None),
		("0.5 10px a", None),
		("bolder 10px a", Some("bold 10px a")),
		("lighter 10px a", Some("100 10px a")),
		("normal 10px a", Some("10px a")),
		("normal normal normal normal 10px a", Some("10px a")),
		("normal normal normal normal normal 10px a", None),
		("bold bold 10px a", None),
		("calc(300 + 400) 10px a", Some("bold 10px a")),
		("calc(2000) 10px a", Some("1000 10px a")),
		("calc(0.5) 10px a", Some("1 10px a")),
		("calc(nan) 10px a", Some("1 10px a")),
		("calc(-5) 10px a", Some("1 10px a")),
		("calc(infinity) 10px a", Some("1000 10px a")),
		("calc(1px) 10px a", None),
		("min(300, 800) 10px a", Some("300 10px a")),
		("1e3 10px a", Some("1000 10px a")),
		("+500 10px a", Some("500 10px a")),
		("500e-1 10px a", Some("50 10px a")),
		("italic 10px a", Some("italic 10px a")),
		("oblique 10px a", Some("italic 10px a")),
		("oblique 10deg 10px a", Some("10px a")),
		("oblique 14deg 10px a", Some("italic 10px a")),
		("oblique 14.2deg 10px a", Some("italic 10px a")),
		("oblique 13.9deg 10px a", Some("10px a")),
		("oblique 0deg 10px a", Some("10px a")),
		("oblique 90deg 10px a", Some("10px a")),
		("oblique 91deg 10px a", None),
		("oblique -14deg 10px a", Some("10px a")),
		("oblique 0.25turn 10px a", Some("10px a")),
		("oblique 1turn 10px a", Some("10px a")),
		("oblique 95grad 10px a", None),
		("oblique 2rad 10px a", Some("10px a")),
		("oblique 0.0390625turn 10px a", Some("italic 10px a")),
		("oblique 15.5grad 10px a", Some("10px a")),
		("oblique calc(7deg * 2) 10px a", Some("italic 10px a")),
		("oblique calc(14deg + 0.1deg) 10px a", Some("italic 10px a")),
		("oblique calc(100deg) 10px a", Some("10px a")),
		("oblique calc(0deg) 10px a", Some("10px a")),
		("oblique 0 10px a", None),
		("oblique 10 10px a", Some("italic 10 10px a")),
		("oblique 14deg bold 10px a", Some("italic bold 10px a")),
		("oblique 14deg italic 10px a", None),
		("italic italic 10px a", None),
		("Oblique 14DEG 10px a", Some("italic 10px a")),
		("small-caps 10px a", Some("small-caps 10px a")),
		("small-caps small-caps 10px a", None),
		("all-small-caps 10px a", None),
		("condensed 10px a", Some("10px a")),
		("50% 10px a", None),
		("condensed condensed 10px a", None),
		("condensed bold italic small-caps 10px a", Some("italic bold small-caps 10px a")),
		("normal italic small-caps bold condensed 10px a", None),
		("small-caps bold italic 10px a", Some("italic bold small-caps 10px a")),
		("0 a", Some("0px a")),
		("-0px a", Some("0px a")),
		("-1px a", None),
		("12.34567px a", Some("12.3457px a")),
		("1e1px a", Some("10px a")),
		("12pt a", Some("16px a")),
		("1pc a", Some("16px a")),
		("1in a", Some("96px a")),
		("1cm a", Some("37.7953px a")),
		("1mm a", Some("3.77953px a")),
		("1Q a", Some("0.944882px a")),
		("2em a", Some("20px a")),
		("150% a", Some("15px a")),
		("1rem a", Some("16px a")),
		("calc(1rem) a", Some("16px a")),
		("xx-small a", Some("9px a")),
		("small a", Some("13px a")),
		("medium a", Some("16px a")),
		("xxx-large a", Some("48px a")),
		("-webkit-xxx-large a", Some("48px a")),
		("larger a", Some("12px a")),
		("smaller a", Some("8.33333px a")),
		("math a", Some("10px a")),
		("medium monospace", Some("13px monospace")),
		("2em monospace", Some("16.25px monospace")),
		("larger monospace", Some("9.75px monospace")),
		("smaller monospace", Some("6.77083px monospace")),
		("math monospace", Some("8.125px monospace")),
		("150% monospace", Some("12.1875px monospace")),
		("calc(2em) monospace", Some("20px monospace")),
		("calc(50% + 1px) monospace", Some("6px monospace")),
		("1rem monospace", Some("16px monospace")),
		("-webkit-xxx-large monospace", Some("39px monospace")),
		("1em MONOSPACE", Some("8.125px monospace")),
		("medium 'monospace'", Some("16px \"monospace\"")),
		("medium monospace, serif", Some("16px monospace, serif")),
		("calc(10px + 2px) a", Some("12px a")),
		("calc(-10px) a", Some("0px a")),
		("max(1em, 15px) a", Some("15px a")),
		("10 a", None),
		("20000px a", Some("10000px a")),
		("1e10px a", Some("10000px a")),
		("10.123456789px a", Some("10.1235px a")),
		("0.00001px a", Some("0.00001px a")),
		("0.0000001px a", Some("1.00000e-7px a")),
		("0.000001px a", Some("0.000001px a")),
		("1000.125px a", Some("1000.13px a")),
		("1000.0625px a", Some("1000.06px a")),
		("100.0005px a", Some("100.001px a")),
		("99.99995px a", Some("99.9999px a")),
		("10.5PX a", Some("10.5px a")),
		("+.5px a", Some("0.5px a")),
		("5.px a", None),
		("1.e1px a", None),
		("10px/12px a", Some("10px a")),
		("10px / 12px a", Some("10px a")),
		("10px/normal a", Some("10px a")),
		("10px/1.5 a", Some("10px a")),
		("10px/abc a", None),
		("10px/-1 a", None),
		("10px/ a", None),
		("10px/calc(1px + 1px) a", Some("10px a")),
		("10px/calc(1) a", Some("10px a")),
		("10px/calc(1deg) a", None),
		("10px/12px/14px a", None),
		("10px /a", None),
		("10px a b", Some("10px \"a b\"")),
		("10px serif x", None),
		("10px x serif", Some("10px \"x serif\"")),
		("10px inherit", None),
		("10px revert-rule", None),
		("10px Revert", None),
		("10px DEFAULT", None),
		("10px a default", Some("10px \"a default\"")),
		("10px 'default'", Some("10px \"default\"")),
		("10px \"inherit\"", Some("10px \"inherit\"")),
		("10px 'a\"b'", Some("10px \"a\\\"b\"")),
		("10px 'a\\\\b'", Some("10px \"a\\\\b\"")),
		("10px 1a", None),
		("10px '1a'", Some("10px \"1a\"")),
		("10px -a", Some("10px -a")),
		("10px --a", Some("10px \"--a\"")),
		("10px -1a", None),
		("10px a_b", Some("10px a_b")),
		("10px \u{e9}", Some("10px \u{e9}")),
		("10px 맑은 고딕", Some("10px \"맑은 고딕\"")),
		("10px system-ui", Some("10px system-ui")),
		("10px ui-serif", Some("10px ui-serif")),
		("10px -webkit-body", Some("10px -webkit-body")),
		("10px math", Some("10px math")),
		("10px System-UI", Some("10px system-ui")),
		("10px 'system-ui'", Some("10px \"system-ui\"")),
		("10px a,", None),
		("10px ,a", None),
		("10px", None),
		("10px 'unterminated", Some("10px unterminated")),
		("10px a !important", None),
		("10px a;", None),
		("10px ''", Some("10px \"\"")),
		("10px \\61", Some("10px a")),
		("10px a\\ b", Some("10px \"a b\"")),
		("10px 'a\\\nb'", Some("10px ab")),
		("10px 'tab\tx'", Some("10px \"tab\\9 x\"")),
		("10px a\\,b", Some("10px \"a,b\"")),
		("10px \\31 a", Some("10px \"1a\"")),
		("10px -\\31 a", Some("10px \"-1a\"")),
		("10px \\2d a", Some("10px -a")),
		("10px a\\", Some("10px a\u{fffd}")),
		("10px \\0", Some("10px \u{fffd}")),
		("10px 'a\\'", Some("10px \"a'\"")),
		("10px a\u{0}b", Some("10px a\u{fffd}b")),
		("10px a\\9 b", Some("10px \"a\\9 b\"")),
		("10px -", None),
		("10px 'serif' serif", None),
		("10px sans-serif sans-serif", None),
		("10px a 'b'", None),
		("10px #a", None),
		("10px a.b", None),
		("10px\\ a", None),
		("10p\\78 a", None),
		("10\\70x a", Some("10px a")),
		("caption", Some("16px Arial")),
		("menu", Some("16px Arial")),
		("status-bar", Some("16px Arial")),
		("-webkit-control", Some("13.3333px Arial")),
		("-webkit-mini-control", Some("13.3333px Arial")),
		("Caption", Some("16px Arial")),
		("  caption  ", Some("16px Arial")),
		("caption 10px", None),
		("bold caption", None),
		("inherit", None),
		("revert-layer", None),
		("", None),
		("   ", None),
		("normal", None),
		("10px/12px", None),
		("  10px   a  ", Some("10px a")),
		("\t10px\na", Some("10px a")),
		("10px\u{a0}a", None),
		("10px/*x*/a", Some("10px a")),
		("/*c*/10px a", Some("10px a")),
		("10px a/*c*/b", Some("10px \"a b\"")),
		("italic/*x*/10px a", Some("italic 10px a")),
		("italic10px a", None),
		("10px\u{c}a", Some("10px a")),
		("10px\ra", Some("10px a")),
	];

	//==============================================================================
	/// 크롬이 받은 글은 같은 직렬화로, 무시한 글은 None 으로.
	//==============================================================================
	#[test]
	fn parse_and_serialize_like_chrome() {
		for (font_string, expected_font) in CHROME_FONT_TABLE {
			let actual_font = parse_canvas_font(font_string).map(|canvas_font| canvas_font.serialized_font);
			assert_eq!(actual_font.as_deref(), *expected_font, "{:?}", font_string);
		}
		// 맥 크롬만 이름 BlinkMacSystemFont 를 "system-ui" 로 바꾼다. (대소문자를 가린다)
		let expected_system_font = if cfg!(target_os = "macos") {
			"10px \"system-ui\""
		}
		else {
			"10px BlinkMacSystemFont"
		};
		assert_eq!(parse_canvas_font("10px BlinkMacSystemFont").unwrap().serialized_font, expected_system_font);
		assert_eq!(parse_canvas_font("10px blinkmacsystemfont").unwrap().serialized_font, "10px blinkmacsystemfont");
	}

	//==============================================================================
	/// 직렬화에서 빠지는 oblique 기울기는 그릴 글꼴에 남는다.
	//==============================================================================
	#[test]
	fn drawing_font_keeps_oblique_slope() {
		let oblique_font = parse_canvas_font("oblique 10deg bold 12px a").unwrap();
		assert_eq!(oblique_font.serialized_font, "bold 12px a");
		assert_eq!(oblique_font.drawing_font, "oblique bold 12px a");
		assert!(crate::core::textstringtexturecache::parse_font_string(&oblique_font.drawing_font).is_italic);
		let italic_font = parse_canvas_font("oblique 12px a").unwrap();
		assert_eq!(italic_font.drawing_font, "italic 12px a");
		let upright_font = parse_canvas_font("oblique 0deg 12px a").unwrap();
		assert_eq!(upright_font.drawing_font, "12px a");
		assert_eq!(parse_canvas_font("caption").unwrap().drawing_font, "16px Arial");
	}

	//==============================================================================
	/// 수 직렬화: 유효 숫자 여섯 자리, 반은 올림, 끝 0 떼기, 아주 작으면 지수 꼴.
	//==============================================================================
	#[test]
	fn format_number_like_blink() {
		assert_eq!(format_css_number(0.0), "0");
		assert_eq!(format_css_number(16.0), "16");
		assert_eq!(format_css_number(10000.0), "10000");
		assert_eq!(format_css_number(8.333333), "8.33333");
		assert_eq!(format_css_number(999999.5), "1.00000e+6");
		assert_eq!(format_css_number(9.999996), "10");
		assert_eq!(format_css_number(0.00000099999996), "0.000001");
	}
}
