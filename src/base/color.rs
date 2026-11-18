//==============================================================================
// 색상. (각 성분 0 ~ 1)
//==============================================================================
use crate::base::math;

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
	pub fn createFromBytes(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
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
	pub fn createFromHEX(hexString: &str) -> Color {
		let trimmedString = hexString.trim().trim_start_matches('#');
		let readByte = |text: &str| -> Option<u8> {
			u8::from_str_radix(text, 16).ok()
		};
		let characterCount = trimmedString.len();
		if characterCount == 3 {
			let mut components = [0u8; 3];
			for (componentIndex, character) in trimmedString.chars().enumerate() {
				let doubledText = format!("{}{}", character, character);
				match readByte(&doubledText) {
					Some(value) => {
						components[componentIndex] = value;
					}
					None => {
						return Color::white();
					}
				}
			}
			return Color::createFromBytes(components[0], components[1], components[2], 255);
		}
		if characterCount == 6 || characterCount == 8 {
			let red = readByte(&trimmedString[0..2]);
			let green = readByte(&trimmedString[2..4]);
			let blue = readByte(&trimmedString[4..6]);
			let alpha = if characterCount == 8 {
				readByte(&trimmedString[6..8])
			}
			else {
				Some(255)
			};
			if let (Some(red), Some(green), Some(blue), Some(alpha)) = (red, green, blue, alpha) {
				return Color::createFromBytes(red, green, blue, alpha);
			}
		}
		Color::white()
	}

	//==============================================================================
	// "rgb(r, g, b)" / "rgba(r, g, b, a)" 문자열로 생성. (r, g, b 는 0 ~ 255, a 는 0 ~ 1)
	//==============================================================================
	pub fn createFromRGBA(rgbaString: &str) -> Color {
		let trimmedString = rgbaString.trim();
		let openIndex = trimmedString.find('(');
		let closeIndex = trimmedString.rfind(')');
		if let (Some(openIndex), Some(closeIndex)) = (openIndex, closeIndex) {
			let inner = &trimmedString[openIndex + 1..closeIndex];
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
	pub fn toHEXString(&self) -> String {
		let toByte = |value: f32| -> u8 {
			(math::clamp01(value) * 255.0).round() as u8
		};
		format!("#{:02x}{:02x}{:02x}{:02x}", toByte(self.red), toByte(self.green), toByte(self.blue), toByte(self.alpha))
	}

	//==============================================================================
	// 알파만 바꾼 색.
	//==============================================================================
	pub fn withAlpha(&self, alpha: f32) -> Color {
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
// 문자열 → 색상. ("#..." 또는 "rgb(...)", 그 외는 흰색. 원본 parseColorValue 와 같다)
//==============================================================================
impl From<&str> for Color {
	fn from(colorString: &str) -> Color {
		let trimmedColorString = colorString.trim().to_lowercase();
		if trimmedColorString.starts_with('#') {
			return Color::createFromHEX(&trimmedColorString);
		}
		if trimmedColorString.starts_with("rgb") {
			return Color::createFromRGBA(&trimmedColorString);
		}
		Color::white()
	}
}

impl From<&String> for Color {
	fn from(colorString: &String) -> Color {
		Color::from(colorString.as_str())
	}
}
