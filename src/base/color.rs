//==============================================================================
//! 색상. (각 성분 0 ~ 1, 원본 base/color.js)
//! - 원본의 red, green, blue, alpha 프로퍼티는 공개 필드로, clone 은 복사(Copy)로 갈음한다.
//! - 성분은 0 ~ 1 이 보통이지만 원본처럼 밖의 값도 자르지 않고 담는다. (빛나는 색처럼 1 을 넘는 HDR 색, 유니티 Color 와 같다)
//! - to_rgba_string, to_hex_string 은 원본처럼 알파가 1 보다 작을 때만 알파를 붙이고, 성분은 0 ~ 255 로, rgba 의 알파는 0 ~ 1 로 잘라 적는다.
//!   (유니티 Color32 로 바꿀 때와 같다, NaN 은 원본처럼 "NaN")
//! - create_from_rgba 는 원본 createFromRGBA 의 정규식과 같은 것을 읽는다. (소문자로 바꿔 rgb 로 시작해야 하고, 그 안의 첫 rgb(정수, 정수, 정수[, 알파]))
//! - create_from_hex 는 원본 createFromHEX 처럼 # 으로 시작해야 하고 "#rgb", "#rgba", "#rrggbb", "#rrggbbaa" 를 읽는다.
//!   두 자리씩 원본 parseInt(글, 16) 처럼 읽어, 그 밖의 길이나 16진수가 아닌 글자도 원본과 같은 값(못 읽은 성분은 NaN)이 된다.
//! - from_hex_cached, from_cache 는 Color 가 Copy 라 나눠 쓰는 인스턴스 대신 캐시에 든 값을 복사해 준다.
//!   두 캐시 모두 1024 개를 넘으면 가장 오래 안 쓴 것부터 버린다. (원본 fromCache 의 규칙, 원본 fromHEXCached 에도 같은 규칙을 넣었다)
//! - 문자열에서 색을 짓는 `From<&str>` 은 원본 Graphic 의 parseColorValue 처럼 from_cache 를 거친다.
//==============================================================================
use crate::base::math;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

/// 색 문자열 캐시가 드는 상한. (넘으면 가장 오래 안 쓴 것부터 버린다, 원본과 같음)
const MAXIMUM_CACHED_COLOR_COUNT: usize = 1024;

//==============================================================================
/// 색 문자열 캐시. (키 → 풀어 둔 색과 마지막으로 쓴 번호, 쓴 번호 → 키. 원본 Map 의 넣은 차례처럼 가장 오래 안 쓴 것을 바로 찾는다)
//==============================================================================
struct ColorStringCache {
	color_table: HashMap<Rc<str>, (Color, u64)>,
	use_order_table: BTreeMap<u64, Rc<str>>,
	use_count: u64,
}

impl ColorStringCache {
	//==============================================================================
	/// 키로 캐시한 색 읽기. (없으면 make_color 로 지어 넣고, MAXIMUM_CACHED_COLOR_COUNT 를 넘으면 가장 오래 안 쓴 것부터 버린다)
	//==============================================================================
	fn read_or_insert(&mut self, cache_key: &str, make_color: impl FnOnce() -> Color) -> Color {
		self.use_count += 1;
		let use_count = self.use_count;
		if let Some((color, last_used_count)) = self.color_table.get_mut(cache_key) {
			if let Some(cached_key) = self.use_order_table.remove(last_used_count) {
				self.use_order_table.insert(use_count, cached_key);
			}
			*last_used_count = use_count;
			return *color;
		}
		let color = make_color();
		let cached_key: Rc<str> = Rc::from(cache_key);
		self.color_table.insert(cached_key.clone(), (color, use_count));
		self.use_order_table.insert(use_count, cached_key);
		while self.color_table.len() > MAXIMUM_CACHED_COLOR_COUNT {
			match self.use_order_table.pop_first() {
				Some((_, oldest_cache_key)) => {
					self.color_table.remove(&oldest_cache_key);
				}
				None => {
					break;
				}
			}
		}
		color
	}
}

thread_local! {
	/// 색 문자열 → 풀어 둔 색. (Color::from_cache)
	static COLOR_STRING_CACHE: RefCell<ColorStringCache> = RefCell::new(ColorStringCache {
		color_table: HashMap::new(),
		use_order_table: BTreeMap::new(),
		use_count: 0,
	});
	/// "색 문자열@알파" → 풀어 둔 색. (Color::from_hex_cached)
	static COLOR_HEX_CACHE: RefCell<ColorStringCache> = RefCell::new(ColorStringCache {
		color_table: HashMap::new(),
		use_order_table: BTreeMap::new(),
		use_count: 0,
	});
}

//==============================================================================
/// 색상. (빨강, 초록, 파랑, 알파를 0 ~ 1 공개 필드로 둔 Copy 값, 기본값은 흰색)
///
/// # 예제
/// 문자열로 만들고 섞어 문자열로 내기
/// ```rust
/// use vanilla::Color;
///
/// let red = Color::create_from_hex("#ff0000");
/// let blue = Color::create_from_rgba("rgba(0, 0, 255, 0.5)");
/// let purple = Color::lerp(red, blue, 0.5);
/// assert_eq!(purple.to_hex_string(), "#800080bf");
/// assert_eq!(purple.to_rgba_string(), "rgba(128, 0, 128, 0.75)");
///
/// let faded = red.with_alpha(0.25);
/// assert_eq!(faded.to_rgba_string(), "rgba(255, 0, 0, 0.25)");
/// assert_eq!(Color::from("#ff0000"), red);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
	/// 빨강. (0 ~ 1, 범위 밖도 자르지 않는다)
	pub red: f64,
	/// 초록. (0 ~ 1, 범위 밖도 자르지 않는다)
	pub green: f64,
	/// 파랑. (0 ~ 1, 범위 밖도 자르지 않는다)
	pub blue: f64,
	/// 알파. (0 이 투명, 1 이 불투명, 범위 밖도 자르지 않는다)
	pub alpha: f64,
}

impl Default for Color {
	//==============================================================================
	/// 흰색. (원본 new Color() 와 같다)
	//==============================================================================
	fn default() -> Color {
		Color::white()
	}
}

impl Color {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `red`: 빨강. (0 ~ 1)
	/// - `green`: 초록. (0 ~ 1)
	/// - `blue`: 파랑. (0 ~ 1)
	/// - `alpha`: 알파. (0 ~ 1)
	///
	/// # 돌려줌
	/// 만든 색. (범위를 자르지 않는다)
	//==============================================================================
	pub const fn create(red: f64, green: f64, blue: f64, alpha: f64) -> Color {
		Color { red, green, blue, alpha }
	}

	//==============================================================================
	/// 0 ~ 255 정수 성분으로 생성.
	///
	/// # 인자
	/// - `red`: 빨강. (0 ~ 255)
	/// - `green`: 초록. (0 ~ 255)
	/// - `blue`: 파랑. (0 ~ 255)
	/// - `alpha`: 알파. (0 ~ 255)
	///
	/// # 돌려줌
	/// 성분마다 255 로 나눈 색.
	//==============================================================================
	pub fn create_from_bytes(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
		Color::create(red as f64 / 255.0, green as f64 / 255.0, blue as f64 / 255.0, alpha as f64 / 255.0)
	}

	//==============================================================================
	/// 흰색.
	///
	/// # 돌려줌
	/// (1, 1, 1, 1) 색.
	//==============================================================================
	pub const fn white() -> Color {
		Color::create(1.0, 1.0, 1.0, 1.0)
	}

	//==============================================================================
	/// 검은색.
	///
	/// # 돌려줌
	/// (0, 0, 0, 1) 색.
	//==============================================================================
	pub const fn black() -> Color {
		Color::create(0.0, 0.0, 0.0, 1.0)
	}

	//==============================================================================
	/// 투명.
	///
	/// # 돌려줌
	/// (0, 0, 0, 0) 색.
	//==============================================================================
	pub const fn transparent() -> Color {
		Color::create(0.0, 0.0, 0.0, 0.0)
	}

	//==============================================================================
	/// 무작위 색. (알파 1)
	///
	/// # 돌려줌
	/// 빨강, 초록, 파랑이 각각 0 이상 1 미만 난수(math::random)이고 알파가 1 인 색.
	//==============================================================================
	pub fn random() -> Color {
		let red = math::random();
		let green = math::random();
		let blue = math::random();
		Color::create(red, green, blue, 1.0)
	}

	//==============================================================================
	/// "#rgb", "#rgba", "#rrggbb", "#rrggbbaa" 문자열로 생성. (원본 createFromHEX, # 으로 시작하지 않으면 흰색)
	/// - 3, 4 자리는 자리마다 두 번 써 늘인다. 빨강, 초록, 파랑은 두 자리씩 읽고, 알파는 8 자리일 때만 읽는다.
	///
	/// # 인자
	/// - `hex_string`: # 으로 시작하는 16진수 색 글. (앞뒤 공백과 대소문자는 가리지 않는다)
	///
	/// # 돌려줌
	/// 읽은 색. # 으로 시작하지 않으면 흰색, 못 읽은 성분은 NaN.
	///
	/// # 예제
	/// 짧은 꼴, 알파 붙은 꼴, # 이 빠진 글
	/// ```rust
	/// use vanilla::Color;
	///
	/// assert_eq!(Color::create_from_hex("#f00"), Color::create(1.0, 0.0, 0.0, 1.0));
	/// assert_eq!(Color::create_from_hex("#00ff0080"), Color::create(0.0, 1.0, 0.0, 128.0 / 255.0));
	/// assert_eq!(Color::create_from_hex("ff0000"), Color::white());
	/// ```
	//==============================================================================
	pub fn create_from_hex(hex_string: &str) -> Color {
		let mut color = Color::create(1.0, 1.0, 1.0, 1.0);
		let lowercase_string = hex_string.trim().to_lowercase();
		let hex_text = match lowercase_string.strip_prefix('#') {
			Some(hex_text) => {
				hex_text
			}
			None => {
				return color;
			}
		};
		let mut digit_list: Vec<char> = hex_text.chars().collect();
		if digit_list.len() == 3 || digit_list.len() == 4 {
			digit_list = digit_list.iter().flat_map(|digit| [*digit, *digit]).collect();
		}
		color.red = parse_hex_integer(&digit_list, 0) / 255.0;
		color.green = parse_hex_integer(&digit_list, 2) / 255.0;
		color.blue = parse_hex_integer(&digit_list, 4) / 255.0;
		if digit_list.len() == 8 {
			color.alpha = parse_hex_integer(&digit_list, 6) / 255.0;
		}
		color
	}

	//==============================================================================
	/// "rgb(r, g, b)" / "rgba(r, g, b, a)" 문자열로 생성. (r, g, b 는 0 ~ 255, a 는 0 ~ 1)
	/// - 원본 정규식 `rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)(?:\s*,\s*([\d.]+))?\s*\)` 의 첫 일치를 읽는다. r, g, b 는 정수, 알파는 원본 parseFloat 처럼 앞에서 읽히는 데까지.
	///
	/// # 인자
	/// - `rgba_string`: "rgb(r, g, b)" 나 "rgba(r, g, b, a)" 글. (앞뒤 공백과 대소문자는 가리지 않는다)
	///
	/// # 돌려줌
	/// 읽은 색. rgb 로 시작하지 않거나 맞는 꼴이 없으면 흰색, 알파가 없으면 1.
	///
	/// # 예제
	/// rgba 글과 값이 모자란 글
	/// ```rust
	/// use vanilla::Color;
	///
	/// assert_eq!(Color::create_from_rgba("rgba(255, 0, 0, 0.5)"), Color::create(1.0, 0.0, 0.0, 0.5));
	/// assert_eq!(Color::create_from_rgba("rgb(0, 255)"), Color::white());
	/// ```
	//==============================================================================
	pub fn create_from_rgba(rgba_string: &str) -> Color {
		let lowered_string = rgba_string.trim().to_lowercase();
		if !lowered_string.starts_with("rgb") {
			return Color::white();
		}
		let character_list: Vec<char> = lowered_string.chars().collect();
		for start_index in 0..character_list.len() {
			if let Some((red, green, blue, alpha)) = match_rgba_function(&character_list, start_index) {
				return Color::create(red / 255.0, green / 255.0, blue / 255.0, alpha.unwrap_or(1.0));
			}
		}
		Color::white()
	}

	//==============================================================================
	/// "rgb(r, g, b)" 또는 "rgba(r, g, b, a)" 문자열로 변환. (알파가 1 보다 작을 때만 rgba)
	///
	/// # 돌려줌
	/// "rgb(r, g, b)", 알파가 1 보다 작으면 "rgba(r, g, b, a)". (r, g, b 는 성분 x 255 를 반올림해 0 ~ 255 로 자른 정수, a 는 0 ~ 1 로 자른 알파)
	//==============================================================================
	pub fn to_rgba_string(&self) -> String {
		let red = format_component_number(self.red, 10);
		let green = format_component_number(self.green, 10);
		let blue = format_component_number(self.blue, 10);
		if self.alpha < 1.0 {
			format!("rgba({}, {}, {}, {})", red, green, blue, math::clamp(self.alpha, 0.0, 1.0))
		}
		else {
			format!("rgb({}, {}, {})", red, green, blue)
		}
	}

	//==============================================================================
	/// "#rrggbb" 또는 "#rrggbbaa" 문자열로 변환. (알파가 1 보다 작을 때만 aa)
	///
	/// # 돌려줌
	/// "#rrggbb", 알파가 1 보다 작으면 "#rrggbbaa". (소문자 16진수, 성분마다 0 ~ 255 로 자른 두 자리)
	///
	/// # 예제
	/// 알파가 1 이면 붙이지 않는다
	/// ```rust
	/// use vanilla::Color;
	///
	/// assert_eq!(Color::create(1.0, 0.5, 0.0, 1.0).to_hex_string(), "#ff8000");
	/// assert_eq!(Color::create(1.0, 0.5, 0.0, 0.5).to_hex_string(), "#ff800080");
	/// ```
	//==============================================================================
	pub fn to_hex_string(&self) -> String {
		let red = format_component_number(self.red, 16);
		let green = format_component_number(self.green, 16);
		let blue = format_component_number(self.blue, 16);
		if self.alpha < 1.0 {
			let alpha = format_component_number(self.alpha, 16);
			format!("#{}{}{}{}", red, green, blue, alpha)
		}
		else {
			format!("#{}{}{}", red, green, blue)
		}
	}

	//==============================================================================
	/// 알파만 바꾼 색.
	///
	/// # 인자
	/// - `alpha`: 새 알파. (0 ~ 1)
	///
	/// # 돌려줌
	/// 빨강, 초록, 파랑은 그대로이고 알파만 바꾼 새 색.
	//==============================================================================
	pub fn with_alpha(&self, alpha: f64) -> Color {
		Color::create(self.red, self.green, self.blue, alpha)
	}

	//==============================================================================
	/// 선형 보간.
	///
	/// # 인자
	/// - `from`: 시작 색.
	/// - `to`: 끝 색.
	/// - `time`: 보간 비율. (0 이면 from, 1 이면 to, 0 ~ 1 밖도 자르지 않는다)
	///
	/// # 돌려줌
	/// 네 성분을 각각 math::lerp 로 보간한 색.
	//==============================================================================
	pub fn lerp(from: Color, to: Color, time: f64) -> Color {
		Color::create(
			math::lerp(from.red, to.red, time),
			math::lerp(from.green, to.green, time),
			math::lerp(from.blue, to.blue, time),
			math::lerp(from.alpha, to.alpha, time),
		)
	}
}

//==============================================================================
/// 문자열 → 색상. ("#..." 또는 "rgb(...)", 그 외는 흰색. 원본 parseColorValue 와 같다)
//==============================================================================
impl Color {
	//==============================================================================
	/// 색 문자열로 캐시한 색 반환. (원본 Color.fromCache, "#...", "rgb(...)", 그 밖은 흰색)
	/// - 같은 문자열이면 다시 풀지 않는다. (풀 때마다 소문자 글을 새로 만든다) 매 프레임 같은 문자열로 색을 정할 때 쓴다. `From<&str>` 도 이것을 거친다.
	/// - 원본처럼 1024 개를 넘으면 가장 오래 안 쓴 것부터 버린다. Color 는 Copy 라 나눠 쓰는 인스턴스가 없다.
	///
	/// # 인자
	/// - `color_string`: "#..." 나 "rgb(...)" 색 글. (앞뒤 공백과 대소문자는 가리지 않는다, 캐시 키는 넘긴 글 그대로)
	///
	/// # 돌려줌
	/// 풀어 둔 색의 복사본. 그 밖의 글이면 흰색.
	///
	/// # 예제
	/// 같은 글은 한 번만 푼다
	/// ```rust
	/// use vanilla::Color;
	///
	/// let first = Color::from_cache("#336699");
	/// let second = Color::from_cache("#336699");
	/// assert_eq!(first, second);
	/// assert!(Color::get_cached_color_count() >= 1);
	/// assert_eq!(Color::from_cache("hsl(0, 0%, 0%)"), Color::white());
	/// ```
	//==============================================================================
	pub fn from_cache(color_string: &str) -> Color {
		COLOR_STRING_CACHE.with(|color_cache| color_cache.borrow_mut().read_or_insert(color_string, || parse_color_string(color_string)))
	}

	//==============================================================================
	/// 색 문자열 캐시에 든 수. (시험과 통계용)
	///
	/// # 돌려줌
	/// from_cache 캐시에 든 색 수. (이 스레드의 것, 1024 개까지)
	//==============================================================================
	pub fn get_cached_color_count() -> usize {
		COLOR_STRING_CACHE.with(|color_cache| color_cache.borrow().color_table.len())
	}
}

impl Color {
	//==============================================================================
	/// HEX 문자열과 알파로 캐시한 색 반환. (원본 Color.fromHEXCached, 알파는 0.01 단위로 다듬어 키에 넣는다)
	/// - from_cache 처럼 1024 개를 넘으면 가장 오래 안 쓴 것부터 버린다.
	///
	/// # 인자
	/// - `color_string`: # 으로 시작하는 16진수 색 글. (create_from_hex 로 읽는다)
	/// - `alpha`: 알파. (0 ~ 1, 0.01 단위로 반올림해 쓰고, 글에 든 알파는 버린다)
	///
	/// # 돌려줌
	/// 캐시한 색의 복사본.
	///
	/// # 예제
	/// 알파는 0.01 단위로 다듬는다
	/// ```rust
	/// use vanilla::Color;
	///
	/// let color = Color::from_hex_cached("#ff0000", 0.504);
	/// assert_eq!(color.red, 1.0);
	/// assert_eq!(color.alpha, 0.5);
	/// ```
	//==============================================================================
	pub fn from_hex_cached(color_string: &str, alpha: f64) -> Color {
		let quantized_alpha = (alpha * 100.0).round() / 100.0;
		let cache_key = format!("{}@{}", color_string, quantized_alpha);
		COLOR_HEX_CACHE.with(|color_cache| {
			color_cache.borrow_mut().read_or_insert(&cache_key, || {
				let mut color = Color::create_from_hex(color_string);
				color.alpha = quantized_alpha;
				color
			})
		})
	}
}

//==============================================================================
/// 색 문자열 풀기. ("#..." 또는 "rgb(...)", 그 밖은 흰색. 원본 fromCache 가 캐시에 없을 때 푸는 것과 같다)
//==============================================================================
fn parse_color_string(color_string: &str) -> Color {
	let trimmed_color_string = color_string.trim().to_lowercase();
	if trimmed_color_string.starts_with('#') {
		return Color::create_from_hex(&trimmed_color_string);
	}
	if trimmed_color_string.starts_with("rgb") {
		return Color::create_from_rgba(&trimmed_color_string);
	}
	Color::white()
}

//==============================================================================
/// 성분(0 ~ 1)을 원본처럼 math.clamp(math.round(성분 * 255), 0, 255) 의 글로. (원본 toByteComponent)
/// - 원본 Number.toString(16) 과 padStart(2, "0") 처럼 16진수는 두 자리가 안 되면 앞에 0 을 채운다. NaN 은 원본처럼 "NaN" 이다.
//==============================================================================
fn format_component_number(value: f64, radix: u32) -> String {
	let rounded_value = math::clamp(math::round(value * 255.0), 0.0, 255.0);
	if rounded_value.is_nan() {
		return String::from("NaN");
	}
	if rounded_value.is_infinite() {
		if rounded_value > 0.0 {
			return String::from("Infinity");
		}
		return String::from("-Infinity");
	}
	let integer_value = rounded_value as i64;
	if radix != 16 {
		return integer_value.to_string();
	}
	let digit_text = format!("{:x}", integer_value.unsigned_abs());
	let signed_text = if integer_value < 0 {
		format!("-{}", digit_text)
	}
	else {
		digit_text
	};
	format!("{:0>2}", signed_text)
}

//==============================================================================
/// 원본 createFromRGBA 정규식이 그 자리에서 맞는지 보고 맞으면 (r, g, b, 알파) 를 돌려준다.
/// - `rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)(?:\s*,\s*([\d.]+))?\s*\)`: 알파 묶음이 맞지 않으면 묶음 없이 닫는 괄호를 본다. (정규식의 되짚기와 같다)
//==============================================================================
fn match_rgba_function(character_list: &[char], start_index: usize) -> Option<(f64, f64, f64, Option<f64>)> {
	let mut index = start_index;
	for expected_character in ['r', 'g', 'b'] {
		if character_list.get(index) != Some(&expected_character) {
			return None;
		}
		index += 1;
	}
	if character_list.get(index) == Some(&'a') {
		index += 1;
	}
	if character_list.get(index) != Some(&'(') {
		return None;
	}
	index += 1;
	let mut component_list = [0.0f64; 3];
	for (component_index, component) in component_list.iter_mut().enumerate() {
		if component_index > 0 {
			index = skip_regex_whitespace(character_list, index);
			if character_list.get(index) != Some(&',') {
				return None;
			}
			index += 1;
		}
		index = skip_regex_whitespace(character_list, index);
		let digit_start_index = index;
		while character_list.get(index).is_some_and(|character| character.is_ascii_digit()) {
			index += 1;
		}
		if index == digit_start_index {
			return None;
		}
		let digit_text: String = character_list[digit_start_index..index].iter().collect();
		*component = digit_text.parse::<f64>().unwrap_or(f64::NAN);
	}

	// 알파 묶음. (\s*,\s*[\d.]+ 뒤에 \s*\) 가 와야 맞는다)
	let alpha_start_index = skip_regex_whitespace(character_list, index);
	if character_list.get(alpha_start_index) == Some(&',') {
		let alpha_text_start_index = skip_regex_whitespace(character_list, alpha_start_index + 1);
		let mut alpha_end_index = alpha_text_start_index;
		while character_list.get(alpha_end_index).is_some_and(|character| character.is_ascii_digit() || *character == '.') {
			alpha_end_index += 1;
		}
		if alpha_end_index > alpha_text_start_index {
			let close_index = skip_regex_whitespace(character_list, alpha_end_index);
			if character_list.get(close_index) == Some(&')') {
				let alpha_text: String = character_list[alpha_text_start_index..alpha_end_index].iter().collect();
				return Some((component_list[0], component_list[1], component_list[2], Some(parse_float_prefix(&alpha_text))));
			}
		}
	}
	let close_index = skip_regex_whitespace(character_list, index);
	if character_list.get(close_index) == Some(&')') {
		return Some((component_list[0], component_list[1], component_list[2], None));
	}
	None
}

//==============================================================================
/// 정규식 \s* 처럼 공백 건너뛰기. (건너뛴 다음 자리)
//==============================================================================
fn skip_regex_whitespace(character_list: &[char], start_index: usize) -> usize {
	let mut index = start_index;
	while character_list.get(index).is_some_and(|character| character.is_whitespace()) {
		index += 1;
	}
	index
}

//==============================================================================
/// 숫자와 점으로 된 글을 원본 parseFloat 처럼 앞에서 읽히는 데까지 읽기. ("0.5.7" 은 0.5, "." 은 NaN)
//==============================================================================
fn parse_float_prefix(number_text: &str) -> f64 {
	let mut prefix_text = String::new();
	let mut has_point = false;
	let mut has_digit = false;
	for character in number_text.chars() {
		if character.is_ascii_digit() {
			has_digit = true;
			prefix_text.push(character);
		}
		else if character == '.' && !has_point {
			has_point = true;
			prefix_text.push(character);
		}
		else {
			break;
		}
	}
	if !has_digit {
		return f64::NAN;
	}
	prefix_text.trim_end_matches('.').parse::<f64>().unwrap_or(f64::NAN)
}

//==============================================================================
/// 16진수 글자 목록의 그 자리에서 두 자리를 원본 Number.parseInt(글, 16) 처럼 읽기. (못 읽으면 NaN)
/// - 앞 공백을 건너뛰고 부호(+, -)와 0x 를 떼어 낸 뒤, 앞에서부터 16진수인 자리까지 읽는다. 한 자리도 없으면 NaN 이다.
//==============================================================================
fn parse_hex_integer(digit_list: &[char], start_index: usize) -> f64 {
	let part_text: String = digit_list.iter().skip(start_index).take(2).collect();
	let mut remaining_text = part_text.trim_start();
	let mut sign = 1.0;
	if let Some(stripped_text) = remaining_text.strip_prefix('-') {
		sign = -1.0;
		remaining_text = stripped_text;
	}
	else if let Some(stripped_text) = remaining_text.strip_prefix('+') {
		remaining_text = stripped_text;
	}
	if remaining_text.starts_with("0x") || remaining_text.starts_with("0X") {
		remaining_text = &remaining_text[2..];
	}
	let mut parsed_value: Option<u32> = None;
	for character in remaining_text.chars() {
		match character.to_digit(16) {
			Some(digit_value) => {
				parsed_value = Some(parsed_value.unwrap_or(0) * 16 + digit_value);
			}
			None => {
				break;
			}
		}
	}
	match parsed_value {
		Some(parsed_value) => {
			sign * parsed_value as f64
		}
		None => {
			f64::NAN
		}
	}
}

impl From<&str> for Color {
	//==============================================================================
	/// 문자열로 색 짓기. (from_cache 를 거친다)
	//==============================================================================
	fn from(color_string: &str) -> Color {
		Color::from_cache(color_string)
	}
}

impl From<&String> for Color {
	//==============================================================================
	/// String 참조로 색 짓기. (&str 판과 같다)
	//==============================================================================
	fn from(color_string: &String) -> Color {
		Color::from(color_string.as_str())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 문자열 변환이 원본과 같은지. (알파가 1 이면 붙이지 않는다)
	//==============================================================================
	#[test]
	fn strings_match_original() {
		let color = Color::create(1.0, 0.5, 0.0, 1.0);
		assert_eq!(color.to_rgba_string(), "rgb(255, 128, 0)");
		assert_eq!(color.to_hex_string(), "#ff8000");
		let transparent_color = color.with_alpha(0.5);
		assert_eq!(transparent_color.to_rgba_string(), "rgba(255, 128, 0, 0.5)");
		assert_eq!(transparent_color.to_hex_string(), "#ff800080");
	}

	//==============================================================================
	/// HEX 캐시는 알파를 0.01 단위로 다듬는다.
	//==============================================================================
	#[test]
	fn hex_cache_quantizes_alpha() {
		let color = Color::from_hex_cached("#ff0000", 0.504);
		assert_eq!(color.red, 1.0);
		assert_eq!(color.alpha, 0.5);
		assert_eq!(Color::from_hex_cached("#ff0000", 0.504), color);
		assert_eq!(Color::from_cache("rgb(0, 255, 0)").green, 1.0);
		assert_eq!(Color::from(" #00FF00 "), Color::create(0.0, 1.0, 0.0, 1.0));
		assert_eq!(Color::from("hsl(0, 0%, 0%)"), Color::white());
		for color_index in 0..=MAXIMUM_CACHED_COLOR_COUNT {
			Color::from_hex_cached(&format!("#{:06x}", color_index), 1.0);
		}
		let hex_cache_count = COLOR_HEX_CACHE.with(|color_cache| color_cache.borrow().color_table.len());
		assert_eq!(hex_cache_count, MAXIMUM_CACHED_COLOR_COUNT);
	}

	//==============================================================================
	/// 16진 글은 원본처럼 읽고 쓴다. (# 필요, #rgba, 두 자리씩 parseInt, 범위 밖 성분은 자르지 않는다)
	//==============================================================================
	#[test]
	fn hex_strings_match_original_range() {
		assert_eq!(Color::create_from_hex("#f00a"), Color::create(1.0, 0.0, 0.0, 170.0 / 255.0));
		assert_eq!(Color::create_from_hex(" #00FF0080 "), Color::create(0.0, 1.0, 0.0, 128.0 / 255.0));
		assert_eq!(Color::create_from_hex("ff0000"), Color::white());
		let five_digit_color = Color::create_from_hex("#12345");
		assert_eq!(five_digit_color.blue, 5.0 / 255.0);
		assert_eq!(five_digit_color.alpha, 1.0);
		let partial_color = Color::create_from_hex("#1zz");
		assert_eq!(partial_color.red, 17.0 / 255.0);
		assert!(partial_color.green.is_nan());
		assert_eq!(Color::create(1.5, -0.2, 0.0, 1.0).to_hex_string(), "#ff0000");
		assert_eq!(Color::create(1.5, -0.2, 0.0, 1.0).to_rgba_string(), "rgb(255, 0, 0)");
		assert_eq!(partial_color.to_hex_string(), "#11NaNNaN");
	}

	//==============================================================================
	/// 0 ~ 1 밖 성분은 담고(HDR), 0 ~ 255 글로 낼 때만 자른다. (원본 toByteComponent, 같은 입력을 원본 node 로 돌린 값)
	//==============================================================================
	#[test]
	fn out_of_range_components_are_kept_and_clamped_in_text() {
		let bright_color = Color::create(1.5, -0.2, 0.5, 1.2);
		assert_eq!(bright_color.red, 1.5);
		assert_eq!(bright_color.alpha, 1.2);
		assert_eq!(bright_color.to_rgba_string(), "rgb(255, 0, 128)");
		assert_eq!(bright_color.to_hex_string(), "#ff0080");
		let negative_alpha_color = Color::create(0.5, 0.5, 0.5, -0.5);
		assert_eq!(negative_alpha_color.to_rgba_string(), "rgba(128, 128, 128, 0)");
		assert_eq!(negative_alpha_color.to_hex_string(), "#80808000");
		let not_a_number_color = Color::create(f64::NAN, 1.5, -0.2, f64::NAN);
		assert_eq!(not_a_number_color.to_hex_string(), "#NaNff00");
		assert_eq!(not_a_number_color.to_rgba_string(), "rgb(NaN, 255, 0)");
	}

	//==============================================================================
	/// rgb 글은 원본 createFromRGBA 의 정규식처럼 읽는다. (같은 글을 원본 node 로 돌린 값, ")(" 처럼 괄호가 뒤집혀도 흰색)
	//==============================================================================
	#[test]
	fn rgba_strings_match_original_regex() {
		let byte = |value: f64| value / 255.0;
		let case_list: [(&str, (f64, f64, f64, f64)); 14] = [
			("rgba(255, 0, 0, 0.5)", (1.0, 0.0, 0.0, 0.5)),
			("rgb(0, 255)", (1.0, 1.0, 1.0, 1.0)),
			("RGB(10,20,30)", (byte(10.0), byte(20.0), byte(30.0), 1.0)),
			("rgb(10.5, 0, 0)", (1.0, 1.0, 1.0, 1.0)),
			("rgba(1,2,3,0.5.7)", (byte(1.0), byte(2.0), byte(3.0), 0.5)),
			("x rgb(1,2,3)", (1.0, 1.0, 1.0, 1.0)),
			("rgbx rgb(1,2,3)", (byte(1.0), byte(2.0), byte(3.0), 1.0)),
			("rgb)(", (1.0, 1.0, 1.0, 1.0)),
			("rgb(1,2,3,)", (1.0, 1.0, 1.0, 1.0)),
			("rgb( 1 , 2 , 3 )", (byte(1.0), byte(2.0), byte(3.0), 1.0)),
			("rgba(300, 0, 0, 2)", (byte(300.0), 0.0, 0.0, 2.0)),
			("rgb(1,2,3) rgb(4,5,6)", (byte(1.0), byte(2.0), byte(3.0), 1.0)),
			("rgba(1,2,3,4", (1.0, 1.0, 1.0, 1.0)),
			("rgb(1,2,3 , 0.5 )", (byte(1.0), byte(2.0), byte(3.0), 0.5)),
		];
		for (rgba_string, (red, green, blue, alpha)) in case_list {
			assert_eq!(Color::create_from_rgba(rgba_string), Color::create(red, green, blue, alpha), "{}", rgba_string);
		}
		assert!(Color::create_from_rgba("rgba(1,2,3,.)").alpha.is_nan());
		assert_eq!(Color::from("rgb)("), Color::white());
	}

	//==============================================================================
	/// 색 문자열 캐시: 상한을 넘으면 가장 오래 안 쓴 것 하나만 버리고, 자주 쓴 것은 남는다. (원본 fromCache 와 같음)
	//==============================================================================
	#[test]
	fn color_cache_evicts_least_recently_used() {
		let first_color = Color::from_cache("#ff0000");
		assert_eq!(first_color, Color::create(1.0, 0.0, 0.0, 1.0));
		for color_index in 0..MAXIMUM_CACHED_COLOR_COUNT {
			Color::from_cache(&format!("rgb({}, 0, 0)", color_index % 256 + color_index / 256 * 1000));
			if color_index % 100 == 0 {
				Color::from_cache("#ff0000");
			}
		}
		assert_eq!(Color::get_cached_color_count(), MAXIMUM_CACHED_COLOR_COUNT);
		let is_kept = COLOR_STRING_CACHE.with(|color_cache| color_cache.borrow().color_table.contains_key("#ff0000"));
		assert!(is_kept);
	}
}
