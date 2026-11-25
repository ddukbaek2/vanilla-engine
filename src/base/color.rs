//==============================================================================
// 색상. (각 성분 0 ~ 1, 원본 base/color.js)
// - 원본의 red, green, blue, alpha 프로퍼티는 공개 필드로, clone 은 복사(Copy)로 갈음한다.
// - to_rgba_string, to_hex_string 은 원본처럼 알파가 1 보다 작을 때만 알파를 붙인다. 원본은 0 ~ 1 밖의 값을 자르지 않아
//   두 자리가 넘는 16진수가 나올 수 있는데, 러스트 판은 0 ~ 1 로 잘라 늘 두 자리다.
// - from_hex_cached, from_cache 는 Color 가 Copy 라 나눠 쓰는 인스턴스 대신 캐시에 든 값을 복사해 준다.
//==============================================================================
use crate::base::math;
use std::cell::RefCell;
use std::collections::HashMap;

// 색 문자열 캐시가 드는 상한. (넘으면 가장 오래 안 쓴 것부터 버린다, 원본과 같음)
const MAXIMUM_CACHED_COLOR_COUNT: usize = 1024;

//==============================================================================
// 색 문자열 캐시. (문자열 → 풀어 둔 색과 마지막으로 쓴 번호)
//==============================================================================
struct ColorStringCache {
	color_table: HashMap<String, (Color, u64)>,
	use_count: u64,
}

thread_local! {
	// 색 문자열 → 풀어 둔 색. (Color::from_cache)
	static COLOR_STRING_CACHE: RefCell<ColorStringCache> = RefCell::new(ColorStringCache {
		color_table: HashMap::new(),
		use_count: 0,
	});
	// "색 문자열@알파" → 풀어 둔 색. (Color::from_hex_cached)
	static COLOR_HEX_CACHE: RefCell<HashMap<String, Color>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
	pub red: f32,
	pub green: f32,
	pub blue: f32,
	pub alpha: f32,
}

impl Default for Color {
	fn default() -> Color {
		Color::white()
	}
}

impl Color {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub const fn create(red: f32, green: f32, blue: f32, alpha: f32) -> Color {
		Color { red, green, blue, alpha }
	}

	//==============================================================================
	// 0 ~ 255 정수 성분으로 생성.
	//==============================================================================
	pub fn create_from_bytes(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
		Color::create(red as f32 / 255.0, green as f32 / 255.0, blue as f32 / 255.0, alpha as f32 / 255.0)
	}

	//==============================================================================
	// 흰색.
	//==============================================================================
	pub const fn white() -> Color {
		Color::create(1.0, 1.0, 1.0, 1.0)
	}

	//==============================================================================
	// 검은색.
	//==============================================================================
	pub const fn black() -> Color {
		Color::create(0.0, 0.0, 0.0, 1.0)
	}

	//==============================================================================
	// 투명.
	//==============================================================================
	pub const fn transparent() -> Color {
		Color::create(0.0, 0.0, 0.0, 0.0)
	}

	//==============================================================================
	// 무작위 색. (알파 1)
	//==============================================================================
	pub fn random() -> Color {
		let red = math::random();
		let green = math::random();
		let blue = math::random();
		Color::create(red, green, blue, 1.0)
	}

	//==============================================================================
	// "#rgb", "#rrggbb", "#rrggbbaa" 문자열로 생성. (읽지 못하면 흰색)
	//==============================================================================
	pub fn create_from_hex(hex_string: &str) -> Color {
		let trimmed_string = hex_string.trim().trim_start_matches('#');
		let read_byte = |text: &str| -> Option<u8> {
			u8::from_str_radix(text, 16).ok()
		};
		let character_count = trimmed_string.len();
		if character_count == 3 {
			let mut components = [0u8; 3];
			for (component_index, character) in trimmed_string.chars().enumerate() {
				let doubled_text = format!("{}{}", character, character);
				match read_byte(&doubled_text) {
					Some(value) => {
						components[component_index] = value;
					}
					None => {
						return Color::white();
					}
				}
			}
			return Color::create_from_bytes(components[0], components[1], components[2], 255);
		}
		if character_count == 6 || character_count == 8 {
			let red = read_byte(&trimmed_string[0..2]);
			let green = read_byte(&trimmed_string[2..4]);
			let blue = read_byte(&trimmed_string[4..6]);
			let alpha = if character_count == 8 {
				read_byte(&trimmed_string[6..8])
			}
			else {
				Some(255)
			};
			if let (Some(red), Some(green), Some(blue), Some(alpha)) = (red, green, blue, alpha) {
				return Color::create_from_bytes(red, green, blue, alpha);
			}
		}
		Color::white()
	}

	//==============================================================================
	// "rgb(r, g, b)" / "rgba(r, g, b, a)" 문자열로 생성. (r, g, b 는 0 ~ 255, a 는 0 ~ 1)
	//==============================================================================
	pub fn create_from_rgba(rgba_string: &str) -> Color {
		let trimmed_string = rgba_string.trim();
		let open_index = trimmed_string.find('(');
		let close_index = trimmed_string.rfind(')');
		if let (Some(open_index), Some(close_index)) = (open_index, close_index) {
			let inner = &trimmed_string[open_index + 1..close_index];
			let values: Vec<f32> = inner.split(',').filter_map(|part| part.trim().parse::<f32>().ok()).collect();
			if values.len() >= 3 {
				let alpha = if values.len() >= 4 {
					values[3]
				}
				else {
					1.0
				};
				return Color::create(values[0] / 255.0, values[1] / 255.0, values[2] / 255.0, alpha);
			}
		}
		Color::white()
	}

	//==============================================================================
	// "rgb(r, g, b)" 또는 "rgba(r, g, b, a)" 문자열로 변환. (알파가 1 보다 작을 때만 rgba)
	//==============================================================================
	pub fn to_rgba_string(&self) -> String {
		let red = convert_component_to_byte(self.red);
		let green = convert_component_to_byte(self.green);
		let blue = convert_component_to_byte(self.blue);
		if self.alpha < 1.0 {
			format!("rgba({}, {}, {}, {})", red, green, blue, self.alpha)
		}
		else {
			format!("rgb({}, {}, {})", red, green, blue)
		}
	}

	//==============================================================================
	// "#rrggbb" 또는 "#rrggbbaa" 문자열로 변환. (알파가 1 보다 작을 때만 aa)
	//==============================================================================
	pub fn to_hex_string(&self) -> String {
		let red = convert_component_to_byte(self.red);
		let green = convert_component_to_byte(self.green);
		let blue = convert_component_to_byte(self.blue);
		if self.alpha < 1.0 {
			let alpha = convert_component_to_byte(self.alpha);
			format!("#{:02x}{:02x}{:02x}{:02x}", red, green, blue, alpha)
		}
		else {
			format!("#{:02x}{:02x}{:02x}", red, green, blue)
		}
	}

	//==============================================================================
	// 알파만 바꾼 색.
	//==============================================================================
	pub fn with_alpha(&self, alpha: f32) -> Color {
		Color::create(self.red, self.green, self.blue, alpha)
	}

	//==============================================================================
	// 선형 보간.
	//==============================================================================
	pub fn lerp(from: Color, to: Color, time: f32) -> Color {
		Color::create(
			math::lerp(from.red, to.red, time),
			math::lerp(from.green, to.green, time),
			math::lerp(from.blue, to.blue, time),
			math::lerp(from.alpha, to.alpha, time),
		)
	}
}

//==============================================================================
// 문자열 → 색상. ("#..." 또는 "rgb(...)", 그 외는 흰색. 원본 parse_color_value 와 같다)
//==============================================================================
impl Color {
	//==============================================================================
	// 색 문자열로 캐시한 색 반환. (원본 Color.fromCache, 풀기는 From<&str> 과 같다: "#...", "rgb(...)", 그 밖은 흰색)
	// - 같은 문자열이면 다시 풀지 않는다. (From<&str> 은 부를 때마다 소문자 글을 새로 만든다) 매 프레임 같은 문자열로 색을 정할 때 쓴다.
	// - 원본처럼 1024 개를 넘으면 가장 오래 안 쓴 것부터 버린다. Color 는 Copy 라 나눠 쓰는 인스턴스가 없다.
	//==============================================================================
	pub fn from_cache(color_string: &str) -> Color {
		COLOR_STRING_CACHE.with(|color_cache| {
			let mut color_cache = color_cache.borrow_mut();
			color_cache.use_count += 1;
			let use_count = color_cache.use_count;
			if let Some((color, last_used_count)) = color_cache.color_table.get_mut(color_string) {
				*last_used_count = use_count;
				return *color;
			}
			let color = Color::from(color_string);
			color_cache.color_table.insert(color_string.to_string(), (color, use_count));
			while color_cache.color_table.len() > MAXIMUM_CACHED_COLOR_COUNT {
				let oldest_color_string = color_cache.color_table.iter().min_by_key(|(_, (_, last_used_count))| *last_used_count).map(|(cached_color_string, _)| cached_color_string.clone());
				match oldest_color_string {
					Some(oldest_color_string) => {
						color_cache.color_table.remove(&oldest_color_string);
					}
					None => {
						break;
					}
				}
			}
			color
		})
	}

	//==============================================================================
	// 색 문자열 캐시에 든 수. (시험과 통계용)
	//==============================================================================
	pub fn get_cached_color_count() -> usize {
		COLOR_STRING_CACHE.with(|color_cache| color_cache.borrow().color_table.len())
	}
}

impl Color {
	//==============================================================================
	// HEX 문자열과 알파로 캐시한 색 반환. (원본 Color.fromHEXCached, 알파는 0.01 단위로 다듬어 키에 넣는다)
	//==============================================================================
	pub fn from_hex_cached(color_string: &str, alpha: f32) -> Color {
		let quantized_alpha = (alpha * 100.0).round() / 100.0;
		let cache_key = format!("{}@{}", color_string, quantized_alpha);
		COLOR_HEX_CACHE.with(|color_cache| {
			if let Some(color) = color_cache.borrow().get(&cache_key) {
				return *color;
			}
			let mut color = Color::create_from_hex(color_string);
			color.alpha = quantized_alpha;
			let mut color_cache = color_cache.borrow_mut();
			if color_cache.len() >= MAXIMUM_CACHED_COLOR_COUNT {
				color_cache.clear();
			}
			color_cache.insert(cache_key, color);
			color
		})
	}
}

//==============================================================================
// 성분(0 ~ 1)을 0 ~ 255 정수로. (반올림, 범위 밖은 자른다)
//==============================================================================
fn convert_component_to_byte(value: f32) -> u8 {
	(math::clamp01(value) * 255.0).round() as u8
}

impl From<&str> for Color {
	fn from(color_string: &str) -> Color {
		let trimmed_color_string = color_string.trim().to_lowercase();
		if trimmed_color_string.starts_with('#') {
			return Color::create_from_hex(&trimmed_color_string);
		}
		if trimmed_color_string.starts_with("rgb") {
			return Color::create_from_rgba(&trimmed_color_string);
		}
		Color::white()
	}
}

impl From<&String> for Color {
	fn from(color_string: &String) -> Color {
		Color::from(color_string.as_str())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 문자열 변환이 원본과 같은지. (알파가 1 이면 붙이지 않는다)
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
	// HEX 캐시는 알파를 0.01 단위로 다듬는다.
	//==============================================================================
	#[test]
	fn hex_cache_quantizes_alpha() {
		let color = Color::from_hex_cached("#ff0000", 0.504);
		assert_eq!(color.red, 1.0);
		assert_eq!(color.alpha, 0.5);
		assert_eq!(Color::from_hex_cached("#ff0000", 0.504), color);
		assert_eq!(Color::from_cache("rgb(0, 255, 0)").green, 1.0);
	}

	//==============================================================================
	// 색 문자열 캐시: 상한을 넘으면 가장 오래 안 쓴 것 하나만 버리고, 자주 쓴 것은 남는다. (원본 fromCache 와 같음)
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
