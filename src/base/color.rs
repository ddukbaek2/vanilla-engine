//==============================================================================
// 색상. (각 성분 0 ~ 1)
//==============================================================================
use crate::base::math;
use std::cell::RefCell;
use std::collections::HashMap;

// 색 문자열 캐시가 드는 상한. (넘으면 비우고 다시 쌓는다)
const MAXIMUM_CACHED_COLOR_COUNT: usize = 1024;

thread_local! {
	// 색 문자열 → 풀어 둔 색. (Color::from_cache)
	static COLOR_STRING_CACHE: RefCell<HashMap<String, Color>> = RefCell::new(HashMap::new());
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
	// "#rrggbbaa" 문자열로 변환.
	//==============================================================================
	pub fn to_hexstring(&self) -> String {
		let to_byte = |value: f32| -> u8 {
			(math::clamp01(value) * 255.0).round() as u8
		};
		format!("#{:02x}{:02x}{:02x}{:02x}", to_byte(self.red), to_byte(self.green), to_byte(self.blue), to_byte(self.alpha))
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
	// - 원본은 오래 안 쓴 것부터 버린다. 여기서는 1024 개를 넘으면 통째로 비우고 다시 쌓는다. Color 는 Copy 라 나눠 쓰는 인스턴스가 없다.
	//==============================================================================
	pub fn from_cache(color_string: &str) -> Color {
		COLOR_STRING_CACHE.with(|color_cache| {
			if let Some(color) = color_cache.borrow().get(color_string) {
				return *color;
			}
			let color = Color::from(color_string);
			let mut color_cache = color_cache.borrow_mut();
			if color_cache.len() >= MAXIMUM_CACHED_COLOR_COUNT {
				color_cache.clear();
			}
			color_cache.insert(color_string.to_string(), color);
			color
		})
	}
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
