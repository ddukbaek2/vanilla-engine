//==============================================================================
//! 색 공간. (게임마다 3D 빛 셈을 어느 공간에서 할지 고르는 칸, 유니티 Player Settings 의 Color Space 꼴, 원본 core/graphic/colorspace.js)
//! - Gamma(기본): 지금까지의 길이다. 색 텍스처를 RGBA 로 올리고 셰이더가 pow(색, 2.2) 근사로 선형으로 바꿔 빛을 셈하고 pow(1 / 2.2) 로 화면 색을 낸다.
//!   바탕색, 틴트, 데칼은 감마를 건 색(sRGB) 그대로 곱하고 섞는다. 이미 나온 게임의 화면이 바뀌지 않는다.
//! - Linear: 3D 의 정석 길이다. 색 텍스처(바탕색, 빛남, 데칼, 매트캡, 천사 고리, 램프)를 SRGB8_ALPHA8 로 올려 GPU 가 선형으로 풀어 읽고
//!   (거르기와 밉맵이 선형 공간) 넘기는 색(꼭짓점 색, 틴트, 빛남 색, 데칼 색, 바깥선 색)은 sRGB 표준 곡선(IEC 61966-2-1)으로 선형으로 바꿔
//!   곱하고 섞는다. 화면에 낼 때도 같은 곡선으로 sRGB 로 바꾼다. 노멀, 마스크 같은 자료 텍스처는 선형 그대로. (ToonTextureOptions 의 is_color_data: false)
//! - 게임은 EngineConfiguration::color_space 로 한 번 고른다. 툰 렌더러, 후처리, 화면 바깥선, GLB 읽기는 따로 주지 않으면 엔진의 값을 따른다.
//!   (엔진을 만들지 않았으면 Gamma) 고른 값은 짓는 때 정해지고 그 뒤에 바뀌지 않는다. (프로그램과 텍스처 형식이 갈리므로)
//! - 2D(Graphic 의 2D 묶음, 글자, UI)는 어느 값이든 지금처럼 감마 공간(sRGB 값 그대로)에서 섞고 합성한다. 이 칸은 3D(툰) 길만 가른다.
//! - 셰이더: Linear 면 엔진이 짓는 3D 셰이더의 #version 다음 줄에 #define VANILLA_COLOR_SPACE_LINEAR 1 이 들어간다.
//!   머티리얼 조각은 #ifdef VANILLA_COLOR_SPACE_LINEAR 로 가를 수 있다. (유니티 셰이더의 UNITY_COLORSPACE_GAMMA 와 같은 쓰임)
//! - 데스크탑 GL 은 기본 프레임버퍼에 GL_FRAMEBUFFER_SRGB 를 켜지 않는다. (셰이더가 곡선을 직접 걸므로 켜면 두 번 걸린다, 웹 캔버스와 같음)
//!
//! # 예제
//! 선형 색 공간 게임의 엔진 설정과 그 엔진이 짓는 툰 렌더러
//! ```rust,no_run
//! use vanilla::{ColorSpace, Engine, EngineConfiguration, ToonRenderer};
//!
//! let mut engine_configuration = EngineConfiguration::new();
//! engine_configuration.color_space = ColorSpace::Linear;
//! let engine = Engine::new(engine_configuration);
//! if let Some(graphic) = engine.get_graphic() {
//! 	let renderer = ToonRenderer::new(graphic.get_gl()).unwrap();
//! 	assert_eq!(renderer.get_color_space(), ColorSpace::Linear);
//! }
//! ```
//==============================================================================

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// Linear 일 때 셰이더 #version 다음 줄에 넣는 정의.
const LINEAR_DEFINE_LINE: &str = "#define VANILLA_COLOR_SPACE_LINEAR 1\n";

//==============================================================================
/// 색 공간 종류. (원본 ColorSpace)
///
/// # 예제
/// 선형 색 공간으로 엔진 설정 짓기
/// ```rust
/// use vanilla::{ColorSpace, EngineConfiguration};
///
/// let mut engine_configuration = EngineConfiguration::new();
/// assert_eq!(engine_configuration.color_space, ColorSpace::Gamma);
/// engine_configuration.color_space = ColorSpace::Linear;
/// assert_eq!(engine_configuration.color_space, ColorSpace::Linear);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorSpace {
	/// 감마 공간. (기본, 지금까지의 화면, pow 2.2 근사)
	#[default]
	Gamma,
	/// 선형 공간. (sRGB 텍스처, sRGB 표준 곡선, 3D 의 정석)
	Linear,
}

//==============================================================================
/// 엔진의 색 공간 읽기. (원본 readEngineColorSpace, 이 스레드의 전역 엔진의 EngineConfiguration::color_space, 엔진이 없으면 Gamma)
/// - 툰 렌더러, 후처리, 화면 바깥선, GLB 읽기가 따로 받지 않을 때 쓰는 기본값이다.
///
/// # 돌려줌
/// 엔진의 색 공간. (엔진을 만들지 않았거나 다른 스레드면 Gamma)
//==============================================================================
pub(crate) fn read_engine_color_space() -> ColorSpace {
	match crate::core::globalengine::get_global_engine() {
		Ok(engine) => {
			engine.get_color_space()
		}
		Err(_) => {
			ColorSpace::Gamma
		}
	}
}

//==============================================================================
/// 셰이더 소스에 색 공간 정의 넣기. (원본 addColorSpaceDefine, Linear 면 첫 줄 #version 다음에 #define VANILLA_COLOR_SPACE_LINEAR 1, Gamma 면 그대로)
///
/// # 인자
/// - `source`: 셰이더 소스.
/// - `color_space`: 색 공간.
///
/// # 돌려줌
/// 정의를 넣은 소스. (#version 줄이 없으면 맨 앞에 넣는다)
//==============================================================================
pub(crate) fn add_color_space_define(source: &str, color_space: ColorSpace) -> String {
	if color_space != ColorSpace::Linear {
		return String::from(source);
	}
	match source.find('\n') {
		Some(line_end) if source.starts_with("#version") => {
			let mut result = String::with_capacity(source.len() + LINEAR_DEFINE_LINE.len());
			result.push_str(&source[..line_end + 1]);
			result.push_str(LINEAR_DEFINE_LINE);
			result.push_str(&source[line_end + 1..]);
			result
		}
		_ => {
			format!("{}{}", LINEAR_DEFINE_LINE, source)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 정의 줄은 Linear 만, #version 다음 줄에. (원본 tooncolorspacechrome 의 정의 줄 검사)
	//==============================================================================
	#[test]
	fn add_color_space_define_puts_line_after_version() {
		let source = "#version 300 es\nvoid main() {}\n";
		assert_eq!(add_color_space_define(source, ColorSpace::Gamma), source);
		assert_eq!(add_color_space_define(source, ColorSpace::Linear), "#version 300 es\n#define VANILLA_COLOR_SPACE_LINEAR 1\nvoid main() {}\n");
		assert_eq!(add_color_space_define("void main() {}\n", ColorSpace::Linear), "#define VANILLA_COLOR_SPACE_LINEAR 1\nvoid main() {}\n");
	}

	//==============================================================================
	/// 엔진이 없는 스레드는 Gamma. (원본 readEngineColorSpace 의 엔진이 없는 페이지)
	//==============================================================================
	#[test]
	fn read_engine_color_space_without_engine_is_gamma() {
		let color_space = std::thread::spawn(read_engine_color_space).join().unwrap();
		assert_eq!(color_space, ColorSpace::Gamma);
		assert_eq!(ColorSpace::default(), ColorSpace::Gamma);
	}
}
