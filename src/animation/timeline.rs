//==============================================================================
//! 타임라인. (키프레임 애니메이션 문서 재생기)
//! - 원본 animation/timeline.js 를 옮긴 것이다. 문서(서술)는 무대 노드 목록 + 트랙(대상 이름 / 속성 / 키) + 마커로 이루어진 JSON 이며
//!   타임라인 편집기가 저장한다. 키의 easing 은 그 키에서 다음 키까지 구간에 걸린다.
//! - 이징 곡선은 core/tween.rs 의 easing_function 을 그대로 쓴다. "bezier" 는 CSS cubic-bezier 와 같은 제어점이다.
//! - 사용:
//!   ```text
//!     // 씬 create 에서 문서를 요청하고 맡긴다.
//!     self.timeline_asset.load(engine, "assets/intro.timeline.json");
//!     self.scene_base.track_asset(&self.timeline_asset);
//!     // initialize 에서 무대를 만들고 튼다.
//!     let mut timeline = Timeline::create_from_json_asset(&mut self.timeline_asset).unwrap_or_default();
//!     let stage_id = self.scene_base.get_root_id();
//!     timeline.build_stage(self.scene_base.get_node_tree_mut(), stage_id, Some(&|image_name: &str| image_table.get(image_name).cloned()));
//!     timeline.play();
//!     // 매 프레임 (tick):
//!     timeline.tick(self.scene_base.get_node_tree_mut(), time_delta);
//!   ```
//! - 원본과 다른 점:
//!   - 대상은 객체 참조 대신 TimelineTarget 이다. 노드는 Node(NodeId), 노드가 아닌 것(화면 효과 같은)은 External(`Rc<RefCell<dyn TimelinePropertyTarget>>`) 이다.
//!     노드에 거는 함수(tick, seek, stop, evaluate, compile, build_stage, bind)는 NodeTree 를 같이 받는다.
//!   - External 대상에는 원본처럼 evaluate 가 그 자리에서 set_timeline_property 를 부른다. (씬은 대상을 `Rc<RefCell>` 로 나눠 쥔다)
//!     넘겨받는 쪽은 TimelinePropertyTarget 트레이트를 구현한다. ScreenEffect 는 이 파일에서 구현했다.
//!     마지막 evaluate 가 External 대상에 건 값은 get_external_properties 로 읽을 수 있다. (편집기와 시험용)
//!   - 값은 원본의 숫자 / 문자열 / 참거짓을 TimelineValue 열거형으로 든다. 숫자는 f64 다.
//!   - 핸들러는 원본처럼 마지막 인자로 타임라인 자신(&mut Timeline)을 받고, 노드에 거는 함수에 넘길 NodeTree 를 그 뒤에 같이 받는다.
//!     부르는 동안 핸들러를 칸에서 꺼내 두므로 핸들러 안에서 pause, seek, set_*_handler 를 부를 수 있다. (새로 건 핸들러가 이긴다)
//!   - 글 속성(text, number, fontSize, visibleCharacters, color)은 원본 getComponent(Text) 가 Text 를 상속한 RichText 도 찾듯 RichText 노드에도 건다.
//!   - resolve_timeline_easing 은 함수 대신 TimelineEasing 값을 돌려준다. (evaluate(progress) 로 부른다)
//!   - TIMELINE_PROPERTY_DEFINITIONS 는 이름 → 정의 객체 대신 정의 배열이고 find_timeline_property_definition 으로 찾는다.
//!     TIMELINE_NODE_DEFAULTS 는 기본값을 채운 TimelineNodeDescription 이다. (원본 키 type 은 node_type)
//!   - 원본의 스프라이트 격자 표와 사운드 표(WeakMap)는 노드에 붙는 작은 컴포넌트 TimelineSpriteFrameGrid, TimelineSound 로 바꿨다.
//!   - 원본 Timeline.setAudioManager(정적)와 사운드 노드의 비동기 읽기는 load_sounds(node_tree, engine) 로 바꿨다.
//!     무대를 만든 뒤 한 번 부르면 사운드 노드마다 오디오 애셋을 읽고 플레이어를 만든다. (돌려준 애셋을 씬이 track_audio_asset 으로 맡겨도 된다)
//!   - loadFromUrl(비동기 fetch)은 load_from_url(엔진, 애셋 경로)이다. 엔진의 애셋 받기로 바이트를 부탁하고, 엔진 작업 안에서 await 하면 타임라인이 나온다.
//!     (데스크탑은 애셋 폴더 후보, 웹은 asset_base_url 에서 fetch) await 없이는 JsonAsset 으로 받은 문서를 create_from_json_asset / load_from_text 로 넘긴다.
//!   - "data:" 주소 이미지: 웹은 원본처럼 브라우저가 풀어(fetch 와 HTMLImageElement, platform/web.rs) 다 풀리면 스프라이트에 걸고 서술의 칸을 다시 고른다.
//!     (원본 image.onload 의 applySpriteFrame, 그동안 TimelineSpriteFrameGrid 가 기다리며 갱신한다)
//!     데스크탑은 그 자리에서 푼다. 브라우저처럼 본문의 퍼센트 꼴(%XX)을 먼저 풀고, ;base64 면 base64 를 푼다. 주소의 형이 image/svg+xml 이면 resvg 로 그린 SVG,
//!     그 밖은 내용으로 가린 래스터(png, jpeg, webp, gif, bmp, ico, avif)다. (Image::load_from_memory_with_mime_type)
//! - 이징 곡선 값은 원본과 같다. 같은 이름의 무대 노드가 둘이면 모두 붙이고 이름 표에는 뒤의 것이 남는다(원본과 같다).
//!   부모가 자기 자신이거나 고리가 되면 무대 부모에 붙인다. load_from_text, create_from_json_asset 은 원본에도 있다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::asset::AssetLoadHandle;
use crate::core::audioplayer::AudioPlayer;
use crate::core::component::paint::Paint;
use crate::core::component::richtext::RichText;
use crate::core::component::sprite::Sprite;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::engine::Engine;
use crate::core::graphic::{BlendMode, TextAlign, TextBaseline};
use crate::core::image::Image;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::WorldNode;
use crate::core::tween::easing_function;
use crate::effect::particlesystem::ParticleSystem;
use crate::effect::screeneffect::ScreenEffect;
use crate::effect::shadersprite::{ShaderSprite, ShaderSpriteEffect};
use crate::resource::audioasset::AudioAsset;
use crate::resource::textasset::JsonAsset;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;
use std::sync::LazyLock;

//==============================================================================
/// 이징 이름 목록. ("linear" / "step" / "bezier" / "<종류>.<in|out|inOut>")
//==============================================================================
pub const TIMELINE_EASING_NAMES: [&str; 33] = [
	"linear", "step", "bezier",
	"quadratic.in", "quadratic.out", "quadratic.inOut",
	"cubic.in", "cubic.out", "cubic.inOut",
	"quartic.in", "quartic.out", "quartic.inOut",
	"quintic.in", "quintic.out", "quintic.inOut",
	"sinusoidal.in", "sinusoidal.out", "sinusoidal.inOut",
	"exponential.in", "exponential.out", "exponential.inOut",
	"circular.in", "circular.out", "circular.inOut",
	"elastic.in", "elastic.out", "elastic.inOut",
	"back.in", "back.out", "back.inOut",
	"bounce.in", "bounce.out", "bounce.inOut",
];

/// 이징 함수. (진행률 → 이징 값)
type EasingFunction = fn(f64) -> f64;

/// 이름 → Tween 이징 함수. (원본 `Tween.easing_function[종류][방향]`)
const EASING_FUNCTION_TABLE: [(&str, EasingFunction); 30] = [
	("quadratic.in", easing_function::quadratic::ease_in),
	("quadratic.out", easing_function::quadratic::ease_out),
	("quadratic.inOut", easing_function::quadratic::ease_in_out),
	("cubic.in", easing_function::cubic::ease_in),
	("cubic.out", easing_function::cubic::ease_out),
	("cubic.inOut", easing_function::cubic::ease_in_out),
	("quartic.in", easing_function::quartic::ease_in),
	("quartic.out", easing_function::quartic::ease_out),
	("quartic.inOut", easing_function::quartic::ease_in_out),
	("quintic.in", easing_function::quintic::ease_in),
	("quintic.out", easing_function::quintic::ease_out),
	("quintic.inOut", easing_function::quintic::ease_in_out),
	("sinusoidal.in", easing_function::sinusoidal::ease_in),
	("sinusoidal.out", easing_function::sinusoidal::ease_out),
	("sinusoidal.inOut", easing_function::sinusoidal::ease_in_out),
	("exponential.in", easing_function::exponential::ease_in),
	("exponential.out", easing_function::exponential::ease_out),
	("exponential.inOut", easing_function::exponential::ease_in_out),
	("circular.in", easing_function::circular::ease_in),
	("circular.out", easing_function::circular::ease_out),
	("circular.inOut", easing_function::circular::ease_in_out),
	("elastic.in", easing_function::elastic::ease_in),
	("elastic.out", easing_function::elastic::ease_out),
	("elastic.inOut", easing_function::elastic::ease_in_out),
	("back.in", easing_function::back::ease_in),
	("back.out", easing_function::back::ease_out),
	("back.inOut", easing_function::back::ease_in_out),
	("bounce.in", easing_function::bounce::ease_in),
	("bounce.out", easing_function::bounce::ease_out),
	("bounce.inOut", easing_function::bounce::ease_in_out),
];

/// "bezier" 키에 곡선이 없을 때 쓰는 제어점. (CSS ease-in-out)
const DEFAULT_BEZIER_CURVE: [f64; 4] = [0.42, 0.0, 0.58, 1.0];

/// 파티클 씨앗. (수집 차례로 서로 다른 씨앗을 준다)
const PARTICLE_RANDOM_SEED_BASE: u32 = 0x9E3779B1;
const PARTICLE_RANDOM_SEED_STEP: u32 = 7919;

/// emit 이벤트에 개수가 없을 때 내는 수.
const DEFAULT_EMIT_COUNT: f64 = 30.0;

//==============================================================================
/// 3차 베지어 이징 값. (CSS cubic-bezier 와 같은 제어점, x 는 0 ~ 1 로 자른다. 뉴턴법 8 번)
//==============================================================================
fn evaluate_bezier_easing(x1: f64, y1: f64, x2: f64, y2: f64, progress: f64) -> f64 {
	if progress <= 0.0 {
		return 0.0;
	}
	if progress >= 1.0 {
		return 1.0;
	}
	let clamped_x1 = x1.clamp(0.0, 1.0);
	let clamped_x2 = x2.clamp(0.0, 1.0);
	let sample_curve = |t: f64, point1: f64, point2: f64| {
		let one_minus = 1.0 - t;
		3.0 * one_minus * one_minus * t * point1 + 3.0 * one_minus * t * t * point2 + t * t * t
	};
	let sample_derivative = |t: f64, point1: f64, point2: f64| {
		let one_minus = 1.0 - t;
		3.0 * one_minus * one_minus * point1 + 6.0 * one_minus * t * (point2 - point1) + 3.0 * t * t * (1.0 - point2)
	};
	let mut t = progress;
	for _ in 0..8 {
		let error = sample_curve(t, clamped_x1, clamped_x2) - progress;
		if error.abs() < 0.00001 {
			break;
		}
		let derivative = sample_derivative(t, clamped_x1, clamped_x2);
		if derivative.abs() < 0.000001 {
			break;
		}
		t -= error / derivative;
		t = t.clamp(0.0, 1.0);
	}
	sample_curve(t, y1, y2)
}

//==============================================================================
/// 3차 베지어 이징 생성. (CSS cubic-bezier 와 같은 제어점, x 는 0 ~ 1)
///
/// # 인자
/// - `x1`: 첫 제어점 x. (0 ~ 1 로 자른다)
/// - `y1`: 첫 제어점 y. (자르지 않는다, 0 ~ 1 밖이면 넘쳤다 돌아온다)
/// - `x2`: 둘째 제어점 x. (0 ~ 1 로 자른다)
/// - `y2`: 둘째 제어점 y. (자르지 않는다)
///
/// # 돌려줌
/// 진행률(0 ~ 1)을 받아 이징 값을 내는 함수. (진행률 0 이하는 0, 1 이상은 1)
///
/// # 예제
/// CSS ease-in-out 곡선
/// ```rust
/// use vanilla::create_bezier_easing;
///
/// let ease_in_out = create_bezier_easing(0.42, 0.0, 0.58, 1.0);
/// assert_eq!(ease_in_out(0.0), 0.0);
/// assert_eq!(ease_in_out(1.0), 1.0);
/// assert!((ease_in_out(0.5) - 0.5).abs() < 0.001);
/// assert!(ease_in_out(0.1) < 0.1);
/// ```
//==============================================================================
pub fn create_bezier_easing(x1: f64, y1: f64, x2: f64, y2: f64) -> impl Fn(f64) -> f64 + Clone {
	move |progress: f64| evaluate_bezier_easing(x1, y1, x2, y2, progress)
}

//==============================================================================
/// 계단 이징. (구간 내내 0, 다음 키에서 바뀐다)
//==============================================================================
fn step_easing(_progress: f64) -> f64 {
	0.0
}

//==============================================================================
/// 타임라인 이징. (원본에서 resolve_timeline_easing 이 돌려주는 함수 자리)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub enum TimelineEasing {
	/// Tween 이징 함수, linear, step.
	Function(EasingFunction),
	/// 3차 베지어 (x1, y1, x2, y2).
	Bezier(f64, f64, f64, f64),
}

impl TimelineEasing {
	//==============================================================================
	/// 진행률(0 ~ 1)의 이징 값.
	///
	/// # 인자
	/// - `progress`: 구간 진행률. (0 ~ 1)
	///
	/// # 돌려줌
	/// 이징 값. (보통 0 ~ 1, elastic, back, 베지어 y 처럼 넘치는 곡선은 밖으로 나간다. step 은 늘 0)
	//==============================================================================
	pub fn evaluate(&self, progress: f64) -> f64 {
		match self {
			TimelineEasing::Function(easing) => {
				easing(progress)
			}
			TimelineEasing::Bezier(x1, y1, x2, y2) => {
				evaluate_bezier_easing(*x1, *y1, *x2, *y2, progress)
			}
		}
	}
}

//==============================================================================
/// 이징 반환. (모르는 이름은 linear, "bezier" 는 curve [x1, y1, x2, y2] 를 쓰고 없으면 ease-in-out)
///
/// # 인자
/// - `easing_name`: 이징 이름. (TIMELINE_EASING_NAMES 의 것, 빈 글은 linear)
/// - `curve`: "bezier" 의 제어점 [x1, y1, x2, y2]. 없거나 넷보다 적으면 [0.42, 0, 0.58, 1]. "bezier" 가 아니면 보지 않는다.
///
/// # 돌려줌
/// evaluate(progress) 로 부르는 이징 값.
///
/// # 예제
/// 이름으로 이징 고르기
/// ```rust
/// use vanilla::resolve_timeline_easing;
///
/// let quadratic_in = resolve_timeline_easing("quadratic.in", None);
/// assert_eq!(quadratic_in.evaluate(0.5), 0.25);
/// let step = resolve_timeline_easing("step", None);
/// assert_eq!(step.evaluate(0.9), 0.0);
/// let curve: [f64; 4] = [0.68, -0.55, 0.265, 1.55];
/// let overshoot = resolve_timeline_easing("bezier", Some(&curve));
/// assert!(overshoot.evaluate(0.3) < 0.0);
/// assert_eq!(resolve_timeline_easing("unknown", None).evaluate(0.3), 0.3);
/// ```
//==============================================================================
pub fn resolve_timeline_easing(easing_name: &str, curve: Option<&[f64]>) -> TimelineEasing {
	if easing_name.is_empty() || easing_name == "linear" {
		return TimelineEasing::Function(easing_function::linear);
	}
	if easing_name == "step" {
		return TimelineEasing::Function(step_easing);
	}
	if easing_name == "bezier" {
		let control_points = match curve {
			Some(curve) if curve.len() >= 4 => {
				[curve[0], curve[1], curve[2], curve[3]]
			}
			_ => {
				DEFAULT_BEZIER_CURVE
			}
		};
		return TimelineEasing::Bezier(control_points[0], control_points[1], control_points[2], control_points[3]);
	}
	match EASING_FUNCTION_TABLE.iter().find(|(name, _)| *name == easing_name) {
		Some((_, easing)) => {
			TimelineEasing::Function(*easing)
		}
		None => {
			TimelineEasing::Function(easing_function::linear)
		}
	}
}

//==============================================================================
/// 원본 parseInt(글, 16) 흉내. (앞 공백, 부호, 0x 를 건너뛰고 16 진 숫자가 이어지는 데까지. 숫자가 없으면 None = NaN)
//==============================================================================
fn parse_leading_hexadecimal(text: &str) -> Option<f64> {
	let trimmed_text = text.trim_start();
	let (sign, unsigned_text) = if let Some(rest) = trimmed_text.strip_prefix('-') {
		(-1.0, rest)
	}
	else if let Some(rest) = trimmed_text.strip_prefix('+') {
		(1.0, rest)
	}
	else {
		(1.0, trimmed_text)
	};
	let digit_text = unsigned_text.strip_prefix("0x").or_else(|| unsigned_text.strip_prefix("0X")).unwrap_or(unsigned_text);
	let mut value: f64 = 0.0;
	let mut digit_count = 0;
	for character in digit_text.chars() {
		match character.to_digit(16) {
			Some(digit) => {
				value = value * 16.0 + digit as f64;
				digit_count += 1;
			}
			None => {
				break;
			}
		}
	}
	if digit_count == 0 {
		return None;
	}
	Some(sign * value)
}

//==============================================================================
/// 색 문자열 → 채널. ("#rrggbb" 또는 "#rrggbbaa", 못 읽는 채널은 1)
///
/// # 인자
/// - `hex_text`: 색 글. "#" 로 시작하지 않으면 흰색으로 읽는다. 채널은 두 글자씩 원본 parseInt(글, 16) 처럼 앞의 16 진 숫자만 읽는다.
///
/// # 돌려줌
/// [빨강, 초록, 파랑, 알파] 채널. (0 ~ 1, 알파는 여덟 글자일 때만 읽고 아니면 1)
///
/// # 예제
/// 색 글 읽기
/// ```rust
/// use vanilla::parse_timeline_color;
///
/// assert_eq!(parse_timeline_color("#ff000080"), [1.0, 0.0, 0.0, 128.0 / 255.0]);
/// assert_eq!(parse_timeline_color("#00ff00"), [0.0, 1.0, 0.0, 1.0]);
/// assert_eq!(parse_timeline_color("red"), [1.0, 1.0, 1.0, 1.0]);
/// ```
//==============================================================================
pub fn parse_timeline_color(hex_text: &str) -> [f64; 4] {
	let characters: Vec<char> = match hex_text.strip_prefix('#') {
		Some(rest) => {
			rest.chars().collect()
		}
		None => {
			"ffffff".chars().collect()
		}
	};
	let read_channel = |start_index: usize| -> Option<f64> {
		let end_index = (start_index + 2).min(characters.len());
		if start_index >= end_index {
			return None;
		}
		let channel_text: String = characters[start_index..end_index].iter().collect();
		parse_leading_hexadecimal(&channel_text).map(|value| value / 255.0)
	};
	let red = read_channel(0).unwrap_or(1.0);
	let green = read_channel(2).unwrap_or(1.0);
	let blue = read_channel(4).unwrap_or(1.0);
	let alpha = if characters.len() >= 8 {
		read_channel(6).unwrap_or(1.0)
	}
	else {
		1.0
	};
	[red, green, blue, alpha]
}

//==============================================================================
/// 채널 → 색 문자열. ("#rrggbbaa", 알파가 없으면 1)
///
/// # 인자
/// - `channels`: [빨강, 초록, 파랑, 알파] 채널. (0 ~ 1 로 자르고 반올림한다, 빠진 빨강, 초록, 파랑은 0)
///
/// # 돌려줌
/// 소문자 "#rrggbbaa" 글.
///
/// # 예제
/// 채널을 색 글로
/// ```rust
/// use vanilla::compose_timeline_color;
///
/// assert_eq!(compose_timeline_color(&[1.0, 0.5, 0.0, 1.0]), "#ff8000ff");
/// assert_eq!(compose_timeline_color(&[0.2, 0.4, 0.6]), "#336699ff");
/// ```
//==============================================================================
pub fn compose_timeline_color(channels: &[f64]) -> String {
	let to_byte = |value: f64| -> u32 {
		let clamped_value = value.clamp(0.0, 1.0);
		(clamped_value * 255.0 + 0.5).floor() as u32
	};
	let read_channel = |channel_index: usize, default_value: f64| channels.get(channel_index).copied().unwrap_or(default_value);
	format!("#{:02x}{:02x}{:02x}{:02x}", to_byte(read_channel(0, 0.0)), to_byte(read_channel(1, 0.0)), to_byte(read_channel(2, 0.0)), to_byte(read_channel(3, 1.0)))
}

//==============================================================================
/// 색 문자열 → Color.
//==============================================================================
fn create_color_from_text(hex_text: &str) -> Color {
	let channels = parse_timeline_color(hex_text);
	Color::create(channels[0], channels[1], channels[2], channels[3])
}

//==============================================================================
/// 원본 Math.round. (절반은 위로)
//==============================================================================
fn round_half_up(value: f64) -> f64 {
	(value + 0.5).floor()
}

//==============================================================================
/// 숫자 → 글. (원본 String(숫자) 처럼 정수는 소수점 없이, -0 은 0)
//==============================================================================
fn format_number(value: f64) -> String {
	if value == 0.0 {
		return String::from("0");
	}
	if value.is_infinite() {
		if value > 0.0 {
			return String::from("Infinity");
		}
		return String::from("-Infinity");
	}
	format!("{}", value)
}

//==============================================================================
/// 원본 Number(글) 흉내. (앞뒤 공백을 떼고, 빈 글은 0, 못 읽으면 NaN)
//==============================================================================
fn parse_number_text(text: &str) -> f64 {
	let trimmed_text = text.trim();
	if trimmed_text.is_empty() {
		return 0.0;
	}
	trimmed_text.parse::<f64>().unwrap_or(f64::NAN)
}

//==============================================================================
/// JSON 값 → 숫자. (원본 Number(값). null 은 0, 없는 값과 객체는 NaN)
//==============================================================================
fn read_json_number(value: Option<&Value>) -> f64 {
	match value {
		Some(Value::Number(number)) => {
			number.as_f64().unwrap_or(f64::NAN)
		}
		Some(Value::String(text)) => {
			parse_number_text(text)
		}
		Some(Value::Bool(is_true)) => {
			if *is_true {
				1.0
			}
			else {
				0.0
			}
		}
		Some(Value::Null) => {
			0.0
		}
		_ => {
			f64::NAN
		}
	}
}

//==============================================================================
/// 타임라인 값. (원본 키 값: 숫자, 문자열, 참거짓)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum TimelineValue {
	/// 값 없음. (원본 null)
	None,
	/// 숫자.
	Number(f64),
	/// 글. (색 "#rrggbbaa", 이벤트 이름도 이것)
	String(String),
	/// 참거짓.
	Boolean(bool),
}

impl TimelineValue {
	//==============================================================================
	/// JSON 값에서 만들기. (배열, 객체는 none)
	///
	/// # 인자
	/// - `value`: 서술 키의 value 칸.
	///
	/// # 돌려줌
	/// 숫자는 Number(f64), 글은 String, 참거짓은 Boolean, null 과 배열, 객체는 None.
	///
	/// # 예제
	/// JSON 값을 타임라인 값으로
	/// ```rust
	/// use serde_json::json;
	/// use vanilla::TimelineValue;
	///
	/// assert_eq!(TimelineValue::from_json(&json!(2.5)), TimelineValue::Number(2.5));
	/// assert_eq!(TimelineValue::from_json(&json!("hit")), TimelineValue::String(String::from("hit")));
	/// assert_eq!(TimelineValue::from_json(&json!([1, 2])), TimelineValue::None);
	/// ```
	//==============================================================================
	pub fn from_json(value: &Value) -> TimelineValue {
		match value {
			Value::Number(number) => {
				TimelineValue::Number(number.as_f64().unwrap_or(0.0))
			}
			Value::String(text) => {
				TimelineValue::String(text.clone())
			}
			Value::Bool(is_true) => {
				TimelineValue::Boolean(*is_true)
			}
			_ => {
				TimelineValue::None
			}
		}
	}

	//==============================================================================
	/// JSON 값으로 바꾸기.
	///
	/// # 돌려줌
	/// None 은 null, 숫자는 JSON 숫자(NaN 과 무한은 null), 글과 참거짓은 그대로.
	//==============================================================================
	pub fn to_json(&self) -> Value {
		match self {
			TimelineValue::None => {
				Value::Null
			}
			TimelineValue::Number(number) => {
				json!(*number)
			}
			TimelineValue::String(text) => {
				Value::String(text.clone())
			}
			TimelineValue::Boolean(is_true) => {
				Value::Bool(*is_true)
			}
		}
	}

	//==============================================================================
	/// 숫자로 읽기. (원본 Number(값). 문자열은 읽고, 참거짓은 1 / 0, 없음은 0)
	///
	/// # 돌려줌
	/// 숫자. 글은 앞뒤 공백을 떼고 읽는다. (빈 글은 0, 못 읽으면 NaN)
	///
	/// # 예제
	/// 여러 값을 숫자로
	/// ```rust
	/// use vanilla::TimelineValue;
	///
	/// assert_eq!(TimelineValue::String(String::from(" 12.5 ")).as_number(), 12.5);
	/// assert_eq!(TimelineValue::Boolean(true).as_number(), 1.0);
	/// assert_eq!(TimelineValue::None.as_number(), 0.0);
	/// assert!(TimelineValue::String(String::from("abc")).as_number().is_nan());
	/// ```
	//==============================================================================
	pub fn as_number(&self) -> f64 {
		match self {
			TimelineValue::None => {
				0.0
			}
			TimelineValue::Number(number) => {
				*number
			}
			TimelineValue::String(text) => {
				parse_number_text(text)
			}
			TimelineValue::Boolean(is_true) => {
				if *is_true {
					1.0
				}
				else {
					0.0
				}
			}
		}
	}

	//==============================================================================
	/// 글로 읽기. (원본 String(값))
	///
	/// # 돌려줌
	/// 글. None 은 "null", 숫자는 정수면 소수점 없이(-0 은 "0", 무한은 "Infinity" / "-Infinity"), 참거짓은 "true" / "false".
	///
	/// # 예제
	/// 값을 글로
	/// ```rust
	/// use vanilla::TimelineValue;
	///
	/// assert_eq!(TimelineValue::Number(3.0).to_text(), "3");
	/// assert_eq!(TimelineValue::Number(0.5).to_text(), "0.5");
	/// assert_eq!(TimelineValue::None.to_text(), "null");
	/// assert_eq!(TimelineValue::Boolean(false).to_text(), "false");
	/// ```
	//==============================================================================
	pub fn to_text(&self) -> String {
		match self {
			TimelineValue::None => {
				String::from("null")
			}
			TimelineValue::Number(number) => {
				format_number(*number)
			}
			TimelineValue::String(text) => {
				text.clone()
			}
			TimelineValue::Boolean(is_true) => {
				is_true.to_string()
			}
		}
	}

	//==============================================================================
	/// 참으로 볼지. (원본 visible 속성: true 또는 "true")
	///
	/// # 돌려줌
	/// Boolean(true) 이거나 글 "true" 면 true. 숫자와 None 은 false.
	//==============================================================================
	pub fn is_true(&self) -> bool {
		match self {
			TimelineValue::Boolean(is_true) => {
				*is_true
			}
			TimelineValue::String(text) => {
				text == "true"
			}
			_ => {
				false
			}
		}
	}
}

//==============================================================================
/// 속성 종류. (number 는 이징 보간, color 는 채널 보간, string / boolean 은 계단)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelinePropertyKind {
	/// 숫자. (키 사이를 이징으로 보간)
	Number,
	/// 색 글. (채널마다 이징으로 보간)
	Color,
	/// 글. (계단, 다음 키에서 바뀐다)
	String,
	/// 참거짓. (계단)
	Boolean,
}

//==============================================================================
/// 속성 정의. (이름 → 종류 / 표시 이름 / 노드에 거는 함수)
/// - TIMELINE_PROPERTY_DEFINITIONS 에 든다. 정의가 없는 속성은 노드에 걸지 않고 External 대상에만 넘긴다.
///
/// # 예제
/// opacity 정의를 찾아 노드에 걸기
/// ```rust
/// use vanilla::{find_timeline_property_definition, TimelinePropertyKind, TimelineValue, WorldNode};
///
/// let definition = find_timeline_property_definition("opacity").expect("opacity");
/// assert_eq!(definition.kind, TimelinePropertyKind::Number);
/// assert_eq!(definition.label, "Opacity");
/// let mut node = WorldNode::new("box");
/// if let Some(apply) = definition.apply {
/// 	apply(&mut node, &TimelineValue::Number(1.5));
/// }
/// assert_eq!(node.get_local_opacity(), 1.0);
/// assert!(find_timeline_property_definition("event").is_some_and(|definition| definition.apply.is_none()));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct TimelinePropertyDefinition {
	/// 서술 트랙의 property 이름. (원본 이름 그대로, "scaleX" 처럼)
	pub name: &'static str,
	/// 값 종류. (보간 방식을 정한다)
	pub kind: TimelinePropertyKind,
	/// 편집기에 보일 이름.
	pub label: &'static str,
	/// 노드에 값을 거는 함수. None 이면 노드에 걸지 않는다. ("event")
	pub apply: Option<fn(&mut WorldNode, &TimelineValue)>,
}

//==============================================================================
/// x 적용.
//==============================================================================
fn apply_position_x(node: &mut WorldNode, value: &TimelineValue) {
	let local_position = node.get_local_position();
	node.set_local_position(Vector2::create(value.as_number(), local_position.y));
}

//==============================================================================
/// y 적용.
//==============================================================================
fn apply_position_y(node: &mut WorldNode, value: &TimelineValue) {
	let local_position = node.get_local_position();
	node.set_local_position(Vector2::create(local_position.x, value.as_number()));
}

//==============================================================================
/// width 적용.
//==============================================================================
fn apply_width(node: &mut WorldNode, value: &TimelineValue) {
	let content_size = node.get_content_size();
	node.set_content_size(Vector2::create(value.as_number(), content_size.y));
}

//==============================================================================
/// height 적용.
//==============================================================================
fn apply_height(node: &mut WorldNode, value: &TimelineValue) {
	let content_size = node.get_content_size();
	node.set_content_size(Vector2::create(content_size.x, value.as_number()));
}

//==============================================================================
/// scale_x 적용.
//==============================================================================
fn apply_scale_x(node: &mut WorldNode, value: &TimelineValue) {
	let local_scale = node.get_local_scale();
	node.set_local_scale(Vector2::create(value.as_number(), local_scale.y));
}

//==============================================================================
/// scale_y 적용.
//==============================================================================
fn apply_scale_y(node: &mut WorldNode, value: &TimelineValue) {
	let local_scale = node.get_local_scale();
	node.set_local_scale(Vector2::create(local_scale.x, value.as_number()));
}

//==============================================================================
/// rotation 적용.
//==============================================================================
fn apply_rotation(node: &mut WorldNode, value: &TimelineValue) {
	node.set_local_rotation(value.as_number());
}

//==============================================================================
/// opacity 적용. (0 ~ 1 로 자른다)
//==============================================================================
fn apply_opacity(node: &mut WorldNode, value: &TimelineValue) {
	node.set_local_opacity(value.as_number().clamp(0.0, 1.0));
}

//==============================================================================
/// visible 적용. (true 또는 "true" 면 켠다)
//==============================================================================
fn apply_visible(node: &mut WorldNode, value: &TimelineValue) {
	node.set_active(value.is_true());
}

//==============================================================================
/// color 적용.
//==============================================================================
fn apply_color(node: &mut WorldNode, value: &TimelineValue) {
	apply_node_color(node, &value.to_text());
}

//==============================================================================
/// text 적용.
//==============================================================================
fn apply_text(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_text(&value.to_text());
	}
	else if let Some(rich_text) = node.get_component_mut::<RichText>() {
		rich_text.set_text(&value.to_text());
	}
}

//==============================================================================
/// number 적용. (반올림한 수를 글자로)
//==============================================================================
fn apply_number(node: &mut WorldNode, value: &TimelineValue) {
	let number_text = format_number(round_half_up(value.as_number()));
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_text(&number_text);
	}
	else if let Some(rich_text) = node.get_component_mut::<RichText>() {
		rich_text.set_text(&number_text);
	}
}

//==============================================================================
/// font_size 적용.
/// - 글자 텍스처는 (문자열, 폰트) 조합마다 굽히므로 정수 픽셀로 맞춰 매 프레임 재굽기를 막는다.
//==============================================================================
fn apply_font_size(node: &mut WorldNode, value: &TimelineValue) {
	let font_size = round_half_up(value.as_number());
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_font_size(font_size);
	}
	else if let Some(rich_text) = node.get_component_mut::<RichText>() {
		rich_text.set_font_size(font_size);
	}
}

//==============================================================================
/// visibleCharacters 적용.
//==============================================================================
fn apply_visible_characters(node: &mut WorldNode, value: &TimelineValue) {
	let visible_character_count = round_half_up(value.as_number()) as i32;
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_visible_character_count(visible_character_count);
	}
	else if let Some(rich_text) = node.get_component_mut::<RichText>() {
		rich_text.set_visible_character_count(visible_character_count);
	}
}

//==============================================================================
/// frame 적용. (스프라이트 시트 칸)
//==============================================================================
fn apply_frame(node: &mut WorldNode, value: &TimelineValue) {
	apply_sprite_frame(node, round_half_up(value.as_number()) as i64);
}

//==============================================================================
/// effect 적용. (ShaderSprite 효과 진행률)
//==============================================================================
fn apply_effect_progress(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(shader_sprite) = node.get_component_mut::<ShaderSprite>() {
		shader_sprite.set_effect_progress(value.as_number());
	}
}

//==============================================================================
/// 애니메이션 속성 정의 목록. (원본 TIMELINE_PROPERTY_DEFINITIONS 의 선언 차례)
//==============================================================================
pub const TIMELINE_PROPERTY_DEFINITIONS: [TimelinePropertyDefinition; 17] = [
	TimelinePropertyDefinition { name: "x", kind: TimelinePropertyKind::Number, label: "X", apply: Some(apply_position_x) },
	TimelinePropertyDefinition { name: "y", kind: TimelinePropertyKind::Number, label: "Y", apply: Some(apply_position_y) },
	TimelinePropertyDefinition { name: "width", kind: TimelinePropertyKind::Number, label: "Width", apply: Some(apply_width) },
	TimelinePropertyDefinition { name: "height", kind: TimelinePropertyKind::Number, label: "Height", apply: Some(apply_height) },
	TimelinePropertyDefinition { name: "scaleX", kind: TimelinePropertyKind::Number, label: "Scale X", apply: Some(apply_scale_x) },
	TimelinePropertyDefinition { name: "scaleY", kind: TimelinePropertyKind::Number, label: "Scale Y", apply: Some(apply_scale_y) },
	TimelinePropertyDefinition { name: "rotation", kind: TimelinePropertyKind::Number, label: "Rotation", apply: Some(apply_rotation) },
	TimelinePropertyDefinition { name: "opacity", kind: TimelinePropertyKind::Number, label: "Opacity", apply: Some(apply_opacity) },
	TimelinePropertyDefinition { name: "visible", kind: TimelinePropertyKind::Boolean, label: "Visible", apply: Some(apply_visible) },
	TimelinePropertyDefinition { name: "color", kind: TimelinePropertyKind::Color, label: "Color", apply: Some(apply_color) },
	TimelinePropertyDefinition { name: "text", kind: TimelinePropertyKind::String, label: "Text", apply: Some(apply_text) },
	TimelinePropertyDefinition { name: "number", kind: TimelinePropertyKind::Number, label: "Number", apply: Some(apply_number) },
	TimelinePropertyDefinition { name: "fontSize", kind: TimelinePropertyKind::Number, label: "Font Size", apply: Some(apply_font_size) },
	TimelinePropertyDefinition { name: "visibleCharacters", kind: TimelinePropertyKind::Number, label: "Visible Characters", apply: Some(apply_visible_characters) },
	TimelinePropertyDefinition { name: "frame", kind: TimelinePropertyKind::Number, label: "Frame", apply: Some(apply_frame) },
	TimelinePropertyDefinition { name: "effect", kind: TimelinePropertyKind::Number, label: "Effect Progress", apply: Some(apply_effect_progress) },
	TimelinePropertyDefinition { name: "event", kind: TimelinePropertyKind::String, label: "Event", apply: None },
];

//==============================================================================
/// 속성 정의 찾기. (원본 `TIMELINE_PROPERTY_DEFINITIONS[이름]`)
///
/// # 인자
/// - `property_name`: 속성 이름. (대소문자를 가린다)
///
/// # 돌려줌
/// 그 이름의 정의. 없으면 None.
//==============================================================================
pub fn find_timeline_property_definition(property_name: &str) -> Option<&'static TimelinePropertyDefinition> {
	TIMELINE_PROPERTY_DEFINITIONS.iter().find(|definition| definition.name == property_name)
}

//==============================================================================
/// 노드에서 색을 받는 컴포넌트에 색 걸기. (Text 나 RichText → 글자색, 그 밖에 Paint / Sprite / ShaderSprite → 색)
/// - 원본은 Sprite, ShaderSprite 가 Paint 를, RichText 가 Text 를 상속해서 get_component(Paint), get_component(Text) 하나로 찾는다.
//==============================================================================
fn apply_node_color(node: &mut WorldNode, hex_text: &str) {
	let color = create_color_from_text(hex_text);
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_text_color(color);
		return;
	}
	if let Some(rich_text) = node.get_component_mut::<RichText>() {
		rich_text.set_text_color(color);
		return;
	}
	if let Some(paint_component) = node.get_component_mut::<Paint>() {
		paint_component.set_color(color);
		return;
	}
	if let Some(sprite) = node.get_component_mut::<Sprite>() {
		sprite.set_color(color);
		return;
	}
	if let Some(shader_sprite) = node.get_component_mut::<ShaderSprite>() {
		shader_sprite.get_sprite_mut().set_color(color);
	}
}

//==============================================================================
/// 스프라이트 시트 프레임 적용. (격자로 잘라 image_rect 를 고른다. 격자가 1 x 1 이거나 이미지가 없으면 그대로)
//==============================================================================
fn apply_sprite_frame(node: &mut WorldNode, frame_index: i64) {
	let (grid_columns, grid_rows) = match node.get_component::<TimelineSpriteFrameGrid>() {
		Some(frame_grid) => {
			(frame_grid.get_columns(), frame_grid.get_rows())
		}
		None => {
			(1, 1)
		}
	};
	apply_sprite_frame_with_grid(node, grid_columns, grid_rows, frame_index);
}

//==============================================================================
/// 격자를 주고 스프라이트 시트 프레임 적용. (격자 컴포넌트가 자기 tick 에서 부를 때, 그때는 노드에서 빠져 있다)
//==============================================================================
fn apply_sprite_frame_with_grid(node: &mut WorldNode, grid_columns: usize, grid_rows: usize, frame_index: i64) {
	let shader_sprite = match node.get_component_mut::<ShaderSprite>() {
		Some(shader_sprite) => {
			shader_sprite
		}
		None => {
			return;
		}
	};
	let image = match shader_sprite.get_sprite().get_image() {
		Some(image) => {
			image.clone()
		}
		None => {
			return;
		}
	};
	if grid_columns <= 1 && grid_rows <= 1 {
		return;
	}
	let frame_count = (grid_columns * grid_rows) as i64;
	let wrapped_index = frame_index.rem_euclid(frame_count) as usize;
	let frame_width = image.get_width() as f64 / grid_columns as f64;
	let frame_height = image.get_height() as f64 / grid_rows as f64;
	let column = wrapped_index % grid_columns;
	let row = wrapped_index / grid_columns;
	shader_sprite.get_sprite_mut().set_image_rect(Rect::create(column as f64 * frame_width, row as f64 * frame_height, frame_width, frame_height));
}

//==============================================================================
/// 값 보간. (종류별. number 는 선형, color 는 채널별, 그 밖에는 앞 값)
//==============================================================================
fn interpolate_value(kind: TimelinePropertyKind, from_value: &TimelineValue, to_value: &TimelineValue, eased_progress: f64) -> TimelineValue {
	match kind {
		TimelinePropertyKind::Number => {
			let from_number = from_value.as_number();
			let to_number = to_value.as_number();
			TimelineValue::Number(from_number + (to_number - from_number) * eased_progress)
		}
		TimelinePropertyKind::Color => {
			let from_channels = parse_timeline_color(&color_text_of(from_value));
			let to_channels = parse_timeline_color(&color_text_of(to_value));
			let mixed_channels: Vec<f64> = from_channels.iter().zip(to_channels.iter()).map(|(from_channel, to_channel)| from_channel + (to_channel - from_channel) * eased_progress).collect();
			TimelineValue::String(compose_timeline_color(&mixed_channels))
		}
		_ => {
			from_value.clone()
		}
	}
}

//==============================================================================
/// 색 값의 글. (문자열이 아니면 원본처럼 흰색으로 읽히게 빈 글)
//==============================================================================
fn color_text_of(value: &TimelineValue) -> String {
	match value {
		TimelineValue::String(text) => {
			text.clone()
		}
		_ => {
			String::new()
		}
	}
}

//==============================================================================
/// 키 하나. (시간, 값, 그 키에서 다음 키까지의 이징, bezier 제어점)
///
/// # 예제
/// quadratic.in 으로 0 에서 100 까지 가는 두 키
/// ```rust
/// use serde_json::json;
/// use vanilla::{Timeline, TimelineKey, TimelinePropertyKind, TimelineValue};
///
/// let mut start_key = TimelineKey::new(0.0, TimelineValue::Number(0.0));
/// start_key.easing = String::from("quadratic.in");
/// let end_key = TimelineKey::from_json(&json!({ "time": 2, "value": 100 }));
/// let keys = vec![start_key, end_key];
/// assert_eq!(Timeline::sample_keys(&keys, TimelinePropertyKind::Number, 1.0), TimelineValue::Number(25.0));
/// assert_eq!(Timeline::sample_keys(&keys, TimelinePropertyKind::Number, 3.0), TimelineValue::Number(100.0));
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineKey {
	/// 키 시간. (초)
	pub time: f64,
	/// 키 값.
	pub value: TimelineValue,
	/// 이 키에서 다음 키까지 구간의 이징 이름. (빈 글과 모르는 이름은 linear)
	pub easing: String,
	/// easing 이 "bezier" 일 때의 제어점 [x1, y1, x2, y2]. (None 이면 ease-in-out)
	pub curve: Option<Vec<f64>>,
}

impl TimelineKey {
	//==============================================================================
	/// 생성. (이징 linear)
	///
	/// # 인자
	/// - `time`: 키 시간. (초)
	/// - `value`: 키 값.
	///
	/// # 돌려줌
	/// 이징 "linear", 제어점 없는 키.
	//==============================================================================
	pub fn new(time: f64, value: TimelineValue) -> TimelineKey {
		TimelineKey {
			time,
			value,
			easing: String::from("linear"),
			curve: None,
		}
	}

	//==============================================================================
	/// JSON 키에서 만들기. ({ time, value, easing, curve })
	///
	/// # 인자
	/// - `key_description`: 서술 키 객체. (빠진 time 은 0, value 는 None, easing 은 빈 글, curve 는 None. curve 의 숫자가 아닌 칸은 0)
	///
	/// # 돌려줌
	/// 읽은 키.
	//==============================================================================
	pub fn from_json(key_description: &Value) -> TimelineKey {
		let time = key_description.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0);
		let value = key_description.get("value").map(TimelineValue::from_json).unwrap_or(TimelineValue::None);
		let easing = key_description.get("easing").and_then(|value| value.as_str()).unwrap_or("").to_string();
		let curve = key_description.get("curve").and_then(|value| value.as_array()).map(|curve_array| {
			curve_array.iter().map(|point| point.as_f64().unwrap_or(0.0)).collect()
		});
		TimelineKey {
			time,
			value,
			easing,
			curve,
		}
	}

	//==============================================================================
	/// 이 키에서 시작하는 구간의 이징.
	///
	/// # 돌려줌
	/// easing 과 curve 로 고른 이징. (resolve_timeline_easing 과 같다)
	//==============================================================================
	pub fn resolve_easing(&self) -> TimelineEasing {
		resolve_timeline_easing(&self.easing, self.curve.as_deref())
	}
}

//==============================================================================
/// 마커. (시간과 이름. 지나치면 마커 핸들러가 불린다)
///
/// # 예제
/// 1초 재생하며 지난 마커 받기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use serde_json::json;
/// use vanilla::{NodeTree, Timeline, TimelineMarker};
///
/// let passed_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
/// let mut node_tree = NodeTree::new();
/// let mut timeline = Timeline::new(Some(json!({
/// 	"duration": 2,
/// 	"loop": false,
/// 	"markers": [{ "time": 0.5, "name": "hit" }, { "time": 1.5, "name": "end" }],
/// })));
/// let recorded_list = passed_list.clone();
/// timeline.set_marker_handler(Some(Box::new(move |marker_name: &str, marker: &TimelineMarker, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
/// 	recorded_list.borrow_mut().push(format!("{} {}", marker_name, marker.time));
/// })));
/// timeline.play();
/// timeline.tick(&mut node_tree, 1.0);
/// assert_eq!(*passed_list.borrow(), vec![String::from("hit 0.5")]);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineMarker {
	/// 마커 시간. (초)
	pub time: f64,
	/// 마커 이름.
	pub name: String,
}

//==============================================================================
/// 트랙 대상. (노드, 또는 노드가 아닌 것)
//==============================================================================
#[derive(Clone)]
pub enum TimelineTarget {
	/// 노드. (정의된 속성은 노드에 바로 건다)
	Node(NodeId),
	/// 노드가 아닌 것. (원본 setTimelineProperty 를 가진 객체, evaluate 가 그 자리에서 값을 넘긴다)
	External(Rc<RefCell<dyn TimelinePropertyTarget>>),
}

impl PartialEq for TimelineTarget {
	//==============================================================================
	/// 같은 대상인지. (노드는 핸들, External 은 같은 객체를 가리키는지)
	//==============================================================================
	fn eq(&self, other: &TimelineTarget) -> bool {
		match (self, other) {
			(TimelineTarget::Node(node_id), TimelineTarget::Node(other_node_id)) => {
				node_id == other_node_id
			}
			(TimelineTarget::External(target), TimelineTarget::External(other_target)) => {
				std::ptr::addr_eq(Rc::as_ptr(target), Rc::as_ptr(other_target))
			}
			_ => {
				false
			}
		}
	}
}

impl std::fmt::Debug for TimelineTarget {
	//==============================================================================
	/// 디버그 출력. (External 은 객체 주소만)
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TimelineTarget::Node(node_id) => {
				formatter.debug_tuple("Node").field(node_id).finish()
			}
			TimelineTarget::External(target) => {
				formatter.debug_tuple("External").field(&Rc::as_ptr(target)).finish()
			}
		}
	}
}

//==============================================================================
/// 노드가 아닌 대상이 값을 받는 트레이트. (원본 setTimelineProperty(이름, 값))
/// - `Rc<RefCell<..>>` 로 쥐고 TimelineTarget::External 로 등록하면 evaluate 가 그 자리에서 부른다.
///
/// # 예제
/// 트랙 값을 받는 페이드 대상
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use serde_json::json;
/// use vanilla::{NodeTree, Timeline, TimelinePropertyTarget, TimelineTarget, TimelineValue};
///
/// struct Fader {
/// 	alpha: f64,
/// }
///
/// impl TimelinePropertyTarget for Fader {
/// 	fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue) {
/// 		if property_name == "alpha" {
/// 			self.alpha = value.as_number();
/// 		}
/// 	}
/// }
///
/// let fader: Rc<RefCell<Fader>> = Rc::new(RefCell::new(Fader { alpha: 0.0 }));
/// let fader_target: Rc<RefCell<dyn TimelinePropertyTarget>> = fader.clone();
/// let mut node_tree = NodeTree::new();
/// let mut timeline = Timeline::new(Some(json!({
/// 	"duration": 2,
/// 	"tracks": [{ "target": "fader", "property": "alpha", "keys": [{ "time": 0, "value": 0 }, { "time": 2, "value": 1 }] }],
/// })));
/// timeline.set_target("fader", TimelineTarget::External(fader_target));
/// timeline.evaluate(&mut node_tree, 1.0);
/// assert_eq!(fader.borrow().alpha, 0.5);
/// ```
//==============================================================================
pub trait TimelinePropertyTarget {
	//==============================================================================
	/// 속성 값 받기.
	///
	/// # 인자
	/// - `property_name`: 서술 트랙의 property 이름.
	/// - `value`: 그 시간에 표본한 값.
	//==============================================================================
	fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue);
}

impl TimelinePropertyTarget for ScreenEffect {
	//==============================================================================
	/// 화면 효과에 값 걸기. (값을 그대로 넘긴다)
	//==============================================================================
	fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue) {
		ScreenEffect::set_timeline_property(self, property_name, value);
	}
}

//==============================================================================
/// External 대상에 건 값 하나. (get_external_properties)
///
/// # 예제
/// evaluate 뒤 External 대상에 건 값 읽기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use serde_json::json;
/// use vanilla::{NodeTree, Timeline, TimelineExternalProperty, TimelinePropertyTarget, TimelineTarget, TimelineValue};
///
/// struct Silent;
///
/// impl TimelinePropertyTarget for Silent {
/// 	fn set_timeline_property(&mut self, _property_name: &str, _value: &TimelineValue) {
/// 	}
/// }
///
/// let screen: Rc<RefCell<dyn TimelinePropertyTarget>> = Rc::new(RefCell::new(Silent));
/// let mut node_tree = NodeTree::new();
/// let mut timeline = Timeline::new(Some(json!({
/// 	"duration": 2,
/// 	"tracks": [{ "target": "screen", "property": "vignette", "keys": [{ "time": 0, "value": 0 }, { "time": 2, "value": 1 }] }],
/// })));
/// timeline.set_target("screen", TimelineTarget::External(screen));
/// timeline.evaluate(&mut node_tree, 1.0);
/// let expected_property = TimelineExternalProperty {
/// 	target_name: String::from("screen"),
/// 	property_name: String::from("vignette"),
/// 	value: TimelineValue::Number(0.5),
/// };
/// assert_eq!(timeline.get_external_properties(), &[expected_property]);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineExternalProperty {
	/// 트랙 대상 이름.
	pub target_name: String,
	/// 트랙 속성 이름.
	pub property_name: String,
	/// 건 값.
	pub value: TimelineValue,
}

//==============================================================================
/// 서술 노드. (원본 무대 노드 서술. TIMELINE_NODE_DEFAULTS 가 기본값)
/// - 필드 이름은 snake_case 이고 서술 JSON 의 키는 원본 그대로 camelCase 다. (scale_x ← "scaleX")
///
/// # 예제
/// 서술 JSON 을 기본값 위에 읽기
/// ```rust
/// use serde_json::json;
/// use vanilla::{TimelineNodeDescription, TIMELINE_NODE_DEFAULTS};
///
/// let description = TimelineNodeDescription::from_json(&json!({ "name": "Box", "type": "paint", "x": 40, "scaleX": 2, "visible": false }));
/// assert_eq!(description.node_type, "paint");
/// assert_eq!(description.x, 40.0);
/// assert_eq!(description.scale_x, 2.0);
/// assert!(!description.visible);
/// assert_eq!(description.width, TIMELINE_NODE_DEFAULTS.width);
/// assert_eq!(description.color, "#ffffffff");
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineNodeDescription {
	/// 노드 이름. (트랙의 target 이 이 이름을 가리킨다, 기본 빈 글)
	pub name: String,
	/// 원본 키 type. ("group", "paint", "sprite", "text", "particle", "sound")
	pub node_type: String,
	/// 부모 노드 이름. (None 이면 무대 부모에 붙는다)
	pub parent: Option<String>,
	/// 로컬 x 위치. (부모 왼쪽 위 기준, 기본 0)
	pub x: f64,
	/// 로컬 y 위치. (기본 0)
	pub y: f64,
	/// 컨텐츠 너비. (기본 100)
	pub width: f64,
	/// 컨텐츠 높이. (기본 100)
	pub height: f64,
	/// 가로 배율. (JSON "scaleX", 기본 1)
	pub scale_x: f64,
	/// 세로 배율. (JSON "scaleY", 기본 1)
	pub scale_y: f64,
	/// 회전. (도, 기본 0)
	pub rotation: f64,
	/// 투명도. (0 ~ 1, 기본 1)
	pub opacity: f64,
	/// 피봇 x 비율. (JSON "pivotX", 0 ~ 1, 기본 0.5)
	pub pivot_x: f64,
	/// 피봇 y 비율. (JSON "pivotY", 0 ~ 1, 기본 0.5)
	pub pivot_y: f64,
	/// 보이기. (JSON 이 false 일 때만 끈다, 기본 true)
	pub visible: bool,
	/// 색 글. (paint 의 칠하기 색, text 의 글자색, sprite 의 덧칠 색. 기본 "#ffffffff")
	pub color: String,
	/// paint 귀퉁이 둥근 크기. (JSON "roundSize", 기본 0)
	pub round_size: f64,
	/// sprite 이미지 이름. (이미지 해석기에 넘긴다. "data:" 주소면 그 자리에서 푼다, 기본 빈 글)
	pub image: String,
	/// sprite 시트 가로 칸 수. (JSON "frameColumns", 1 보다 작으면 1, 기본 1)
	pub frame_columns: f64,
	/// sprite 시트 세로 칸 수. (JSON "frameRows", 1 보다 작으면 1, 기본 1)
	pub frame_rows: f64,
	/// sprite 시트 처음 칸 번호. (반올림, 기본 0)
	pub frame: f64,
	/// sprite 의 ShaderSprite 효과 이름. (ShaderSpriteEffect::from_name, 모르는 이름은 효과 없음, 기본 "none")
	pub effect: String,
	/// sprite 효과 진행률. (JSON "effectProgress", 0 ~ 1, 기본 0)
	pub effect_progress: f64,
	/// sprite 효과 색 글. (JSON "effectColor", 빈 글이면 효과 기본 색, 기본 빈 글)
	pub effect_color: String,
	/// sprite 섞기 이름. (JSON "blendMode", 캔버스 합성 이름, 모르는 이름은 Normal, 기본 "source-over")
	pub blend_mode: String,
	/// text 글. (기본 "Text")
	pub text: String,
	/// text 글자 크기. (JSON "fontSize", 픽셀, 기본 32)
	pub font_size: f64,
	/// text 굵게. (JSON 이 true 일 때만, 기본 false)
	pub bold: bool,
	/// text 가로 정렬 이름. (JSON "textAlign", "left", "right", "start", "end", 그 밖은 가운데. 기본 "center")
	pub text_align: String,
	/// particle 서술 객체. (ParticleSystem::apply_description 에 넘긴다, null 이면 None)
	pub particle: Option<Value>,
	/// sound 소리 주소. (Timeline::load_sounds 가 읽는다, 기본 빈 글)
	pub audio: String,
	/// sound 음량. (들고만 있다, 기본 1)
	pub volume: f64,
}

impl Default for TimelineNodeDescription {
	//==============================================================================
	/// 기본값. (원본 TIMELINE_NODE_DEFAULTS)
	//==============================================================================
	fn default() -> TimelineNodeDescription {
		TimelineNodeDescription {
			name: String::new(),
			node_type: String::from("group"),
			parent: None,
			x: 0.0,
			y: 0.0,
			width: 100.0,
			height: 100.0,
			scale_x: 1.0,
			scale_y: 1.0,
			rotation: 0.0,
			opacity: 1.0,
			pivot_x: 0.5,
			pivot_y: 0.5,
			visible: true,
			color: String::from("#ffffffff"),
			round_size: 0.0,
			image: String::new(),
			frame_columns: 1.0,
			frame_rows: 1.0,
			frame: 0.0,
			effect: String::from("none"),
			effect_progress: 0.0,
			effect_color: String::new(),
			blend_mode: String::from("source-over"),
			text: String::from("Text"),
			font_size: 32.0,
			bold: false,
			text_align: String::from("center"),
			particle: None,
			audio: String::new(),
			volume: 1.0,
		}
	}
}

impl TimelineNodeDescription {
	//==============================================================================
	/// JSON 서술에서 만들기. (기본값 위에 준 항목을 덮는다. 원본 Object.assign({}, TIMELINE_NODE_DEFAULTS, 서술))
	/// - 꼴이 맞지 않는 항목은 기본값을 둔다. visible 은 원본처럼 false 일 때만 끈다.
	///
	/// # 인자
	/// - `node_description`: 서술 stage.nodes 의 노드 객체.
	///
	/// # 돌려줌
	/// 기본값을 채운 서술 노드.
	//==============================================================================
	pub fn from_json(node_description: &Value) -> TimelineNodeDescription {
		let defaults = TimelineNodeDescription::default();
		let read_number = |name: &str, default_value: f64| node_description.get(name).and_then(|value| value.as_f64()).map(|value| value).unwrap_or(default_value);
		let read_text = |name: &str, default_value: &str| node_description.get(name).and_then(|value| value.as_str()).unwrap_or(default_value).to_string();
		TimelineNodeDescription {
			name: read_text("name", &defaults.name),
			node_type: read_text("type", &defaults.node_type),
			parent: node_description.get("parent").and_then(|value| value.as_str()).map(|parent_name| parent_name.to_string()),
			x: read_number("x", defaults.x),
			y: read_number("y", defaults.y),
			width: read_number("width", defaults.width),
			height: read_number("height", defaults.height),
			scale_x: read_number("scaleX", defaults.scale_x),
			scale_y: read_number("scaleY", defaults.scale_y),
			rotation: read_number("rotation", defaults.rotation),
			opacity: read_number("opacity", defaults.opacity),
			pivot_x: read_number("pivotX", defaults.pivot_x),
			pivot_y: read_number("pivotY", defaults.pivot_y),
			visible: node_description.get("visible") != Some(&Value::Bool(false)),
			color: read_text("color", &defaults.color),
			round_size: read_number("roundSize", defaults.round_size),
			image: read_text("image", &defaults.image),
			frame_columns: read_number("frameColumns", defaults.frame_columns),
			frame_rows: read_number("frameRows", defaults.frame_rows),
			frame: read_number("frame", defaults.frame),
			effect: read_text("effect", &defaults.effect),
			effect_progress: read_number("effectProgress", defaults.effect_progress),
			effect_color: read_text("effectColor", &defaults.effect_color),
			blend_mode: read_text("blendMode", &defaults.blend_mode),
			text: read_text("text", &defaults.text),
			font_size: read_number("fontSize", defaults.font_size),
			bold: node_description.get("bold") == Some(&Value::Bool(true)),
			text_align: read_text("textAlign", &defaults.text_align),
			particle: node_description.get("particle").filter(|value| !value.is_null()).cloned(),
			audio: read_text("audio", &defaults.audio),
			volume: read_number("volume", defaults.volume),
		}
	}
}

//==============================================================================
/// 서술 노드 기본값. (원본 TIMELINE_NODE_DEFAULTS)
//==============================================================================
pub static TIMELINE_NODE_DEFAULTS: LazyLock<TimelineNodeDescription> = LazyLock::new(TimelineNodeDescription::default);

//==============================================================================
/// 스프라이트 시트 격자. (노드 서술의 frame_columns / frame_rows. 원본 spriteFrameGridTable 자리)
/// - 그리지 않는 표시용 컴포넌트다. frame 속성이 이것을 읽어 칸을 고른다.
/// - 웹에서 "data:" 주소 그림을 브라우저가 푸는 동안만 갱신한다. 다 풀리면 스프라이트에 걸고 서술의 칸을 고른다. (원본 image.onload 의 applySpriteFrame)
///
/// # 예제
/// 4 x 2 시트의 5 번 칸을 고른 스프라이트 노드
/// ```rust
/// use std::rc::Rc;
/// use serde_json::json;
/// use vanilla::{Image, NodeTree, Rect, ShaderSprite, Timeline, TimelineSpriteFrameGrid};
///
/// let sheet_image = Image::create_from_pixels(64, 32, vec![255u8; 64 * 32 * 4]);
/// let image_resolver = |image_name: &str| -> Option<Rc<Image>> {
/// 	if image_name == "coin" {
/// 		Some(sheet_image.clone())
/// 	}
/// 	else {
/// 		None
/// 	}
/// };
/// let mut node_tree = NodeTree::new();
/// let node_description = json!({ "name": "Coin", "type": "sprite", "image": "coin", "frameColumns": 4, "frameRows": 2, "frame": 5 });
/// let coin_id = Timeline::create_stage_node(&mut node_tree, &node_description, Some(&image_resolver));
/// let frame_grid = node_tree[coin_id].get_component::<TimelineSpriteFrameGrid>().expect("grid");
/// assert_eq!((frame_grid.get_columns(), frame_grid.get_rows()), (4, 2));
/// let image_rect = node_tree[coin_id].get_component::<ShaderSprite>().expect("sprite").get_sprite().get_image_rect();
/// assert_eq!(image_rect, Rect::create(16.0, 16.0, 16.0, 16.0));
/// ```
//==============================================================================
pub struct TimelineSpriteFrameGrid {
	component_base: ComponentBase,
	columns: usize,
	rows: usize,
	/// 브라우저가 푸는 중인 "data:" 주소 그림과 다 풀리면 고를 서술의 칸. (데스크탑은 그 자리에서 풀어 늘 None)
	pending_image_load: Option<(AssetLoadHandle, i64)>,
}

#[crate::reflect]
impl TimelineSpriteFrameGrid {
	//==============================================================================
	/// 생성. (칸 수는 1 이상)
	///
	/// # 인자
	/// - `columns`: 가로 칸 수. (0 이면 1)
	/// - `rows`: 세로 칸 수. (0 이면 1)
	///
	/// # 돌려줌
	/// 기다리는 그림이 없는 격자 컴포넌트.
	//==============================================================================
	pub fn new(columns: usize, rows: usize) -> TimelineSpriteFrameGrid {
		TimelineSpriteFrameGrid {
			component_base: ComponentBase::new(),
			columns: columns.max(1),
			rows: rows.max(1),
			pending_image_load: None,
		}
	}

	//==============================================================================
	/// 격자 설정.
	/// - 걸린 칸을 다시 고르지는 않는다. 다음 frame 값부터 새 격자로 고른다.
	///
	/// # 인자
	/// - `columns`: 가로 칸 수. (0 이면 1)
	/// - `rows`: 세로 칸 수. (0 이면 1)
	//==============================================================================
	pub fn set_grid(&mut self, columns: usize, rows: usize) {
		self.columns = columns.max(1);
		self.rows = rows.max(1);
	}

	//==============================================================================
	/// 가로 칸 수.
	///
	/// # 돌려줌
	/// 가로 칸 수. (1 이상)
	//==============================================================================
	pub fn get_columns(&self) -> usize {
		self.columns
	}

	//==============================================================================
	/// 세로 칸 수.
	///
	/// # 돌려줌
	/// 세로 칸 수. (1 이상)
	//==============================================================================
	pub fn get_rows(&self) -> usize {
		self.rows
	}
}

#[crate::reflect]
impl Component for TimelineSpriteFrameGrid {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("TimelineSpriteFrameGrid")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"TimelineSpriteFrameGrid"
	}

	//==============================================================================
	/// 브라우저가 푸는 "data:" 주소 그림 기다리기. (다 풀리면 스프라이트에 걸고 칸을 고른다, 못 풀었으면 그림 없이 둔다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f64) {
		let (load_handle, frame_index) = match self.pending_image_load.take_if(|(load_handle, _)| load_handle.is_finished()) {
			Some(pending_image_load) => {
				pending_image_load
			}
			None => {
				return;
			}
		};
		let image = match load_handle.get_decoded_image() {
			Some(image) => {
				image
			}
			None => {
				return;
			}
		};
		let node = context.get_node_mut();
		if let Some(shader_sprite) = node.get_component_mut::<ShaderSprite>() {
			shader_sprite.get_sprite_mut().set_image(Some(image));
			shader_sprite.get_sprite_mut().set_image_rect(Rect::zero());
		}
		apply_sprite_frame_with_grid(node, self.columns, self.rows, frame_index);
	}

	//==============================================================================
	/// 웹에서 "data:" 주소 그림을 기다릴 때만 갱신한다.
	//==============================================================================
	fn is_tick_needed(&self) -> bool {
		self.pending_image_load.is_some()
	}

	//==============================================================================
	/// 아무것도 그리지 않으니 노드 거르기를 막지 않는다.
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		true
	}
}

//==============================================================================
/// 사운드 노드 상태. (원본 soundPlayerTable 의 { audio_player, audio_asset, audio_source, volume } 자리)
/// - 애셋과 플레이어는 Timeline::load_sounds 가 채운다. 이벤트 키 play / loop / stop 이 플레이어를 부린다.
///
/// # 예제
/// 사운드 노드를 만들면 주소와 음량만 들고 있다
/// ```rust
/// use serde_json::json;
/// use vanilla::{NodeTree, Timeline, TimelineSound};
///
/// let mut node_tree = NodeTree::new();
/// let node_description = json!({ "name": "Hit", "type": "sound", "audio": "tests/assets/audio/wav_s16_stereo.wav", "volume": 0.5 });
/// let sound_id = Timeline::create_stage_node(&mut node_tree, &node_description, None);
/// let sound = node_tree[sound_id].get_component::<TimelineSound>().expect("sound");
/// assert_eq!(sound.get_audio_source(), "tests/assets/audio/wav_s16_stereo.wav");
/// assert_eq!(sound.get_volume(), 0.5);
/// assert!(sound.get_audio_asset().is_none());
/// ```
//==============================================================================
pub struct TimelineSound {
	component_base: ComponentBase,
	audio_player: Option<AudioPlayer>,
	audio_asset: Option<Rc<AudioAsset>>,
	audio_source: String,
	volume: f64,
}

#[crate::reflect]
impl TimelineSound {
	//==============================================================================
	/// 생성. (비어 있음)
	///
	/// # 돌려줌
	/// 주소가 빈 글, 음량 1, 애셋과 플레이어가 없는 사운드 컴포넌트.
	//==============================================================================
	pub fn new() -> TimelineSound {
		TimelineSound {
			component_base: ComponentBase::new(),
			audio_player: None,
			audio_asset: None,
			audio_source: String::new(),
			volume: 1.0,
		}
	}

	//==============================================================================
	/// 소리 주소 반환.
	///
	/// # 돌려줌
	/// 서술의 audio 주소. (없으면 빈 글)
	//==============================================================================
	pub fn get_audio_source(&self) -> &str {
		&self.audio_source
	}

	//==============================================================================
	/// 음량 반환. (원본처럼 서술 값을 들고만 있다)
	///
	/// # 돌려줌
	/// 서술의 volume. (기본 1, 플레이어에 걸지 않는다)
	//==============================================================================
	pub fn get_volume(&self) -> f64 {
		self.volume
	}

	//==============================================================================
	/// 오디오 애셋 반환.
	///
	/// # 돌려줌
	/// 읽은 오디오 애셋. load_sounds 전이거나 읽기에 실패했으면 None.
	//==============================================================================
	pub fn get_audio_asset(&self) -> Option<&Rc<AudioAsset>> {
		self.audio_asset.as_ref()
	}

	//==============================================================================
	/// 플레이어 반환.
	///
	/// # 돌려줌
	/// 플레이어의 변경 참조. load_sounds 전이면 None.
	//==============================================================================
	pub fn get_audio_player_mut(&mut self) -> Option<&mut AudioPlayer> {
		self.audio_player.as_mut()
	}

	//==============================================================================
	/// 멈춤. (플레이어가 있으면)
	//==============================================================================
	pub fn stop(&mut self) {
		if let Some(audio_player) = self.audio_player.as_mut() {
			audio_player.stop();
		}
	}

	//==============================================================================
	/// 이벤트 처리. (play / loop 는 애셋이 읽혔으면 처음부터 튼다, stop 은 멈춘다)
	//==============================================================================
	fn handle_event(&mut self, event_name: &str) {
		let audio_player = match self.audio_player.as_mut() {
			Some(audio_player) => {
				audio_player
			}
			None => {
				return;
			}
		};
		if event_name == "play" || event_name == "loop" {
			if let Some(audio_asset) = self.audio_asset.as_ref() {
				if audio_asset.is_loaded() {
					audio_player.set_audio_asset(audio_asset.clone());
					audio_player.set_time(0.0);
					audio_player.play(event_name == "loop");
				}
			}
		}
		else if event_name == "stop" {
			audio_player.stop();
		}
	}
}

impl Default for TimelineSound {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TimelineSound {
		TimelineSound::new()
	}
}

#[crate::reflect]
impl Component for TimelineSound {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("TimelineSound")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"TimelineSound"
	}

	//==============================================================================
	/// 갱신할 것이 없다.
	//==============================================================================
	fn is_tick_needed(&self) -> bool {
		false
	}
}

//==============================================================================
/// 노드의 T 컴포넌트를 찾고 없으면 붙인다. (원본 getOrAddComponent)
//==============================================================================
fn get_or_add_component<T: Component + 'static>(node: &mut WorldNode, create: impl FnOnce() -> T) -> &mut T {
	if !node.has_component::<T>() {
		node.add_component(create());
	}
	node.get_component_mut::<T>().expect("component just added")
}

//==============================================================================
/// 가로 정렬 이름 읽기. (모르는 이름은 center)
//==============================================================================
fn read_text_align(text_align_name: &str) -> TextAlign {
	match text_align_name {
		"left" => {
			TextAlign::Left
		}
		"right" => {
			TextAlign::Right
		}
		"start" => {
			TextAlign::Start
		}
		"end" => {
			TextAlign::End
		}
		_ => {
			TextAlign::Center
		}
	}
}

//==============================================================================
/// "data:" 주소 이미지 읽기. (base/dataurl.rs 로 풀고, 형이 image/svg+xml 일 때만 SVG 로 푼다. 브라우저와 같다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn load_data_url_image(data_url: &str) -> Option<Rc<Image>> {
	let data_url = crate::base::dataurl::parse_data_url(data_url)?;
	Image::load_from_memory_with_mime_type(&data_url.bytes, &data_url.mime_type).ok()
}

//==============================================================================
/// "data:" 주소 이미지를 브라우저에 풀어 달라고 부탁하기. (웹, fetch 가 주소의 형을 content-type 으로 주고 브라우저가 그 형으로 푼다. 다 풀리면 손잡이의 그림)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn request_data_url_image(data_url: &str) -> AssetLoadHandle {
	let load_handle = AssetLoadHandle::create_pending(data_url);
	crate::platform::request_asset_bytes(data_url.to_string(), load_handle.clone(), false);
	load_handle
}

//==============================================================================
/// 컴파일한 트랙. (키를 시간순으로 정렬하고 대상 / 속성 정의를 붙인 것)
//==============================================================================
struct CompiledTimelineTrack {
	target_name: String,
	property_name: String,
	is_enabled: bool,
	keys: Vec<TimelineKey>,
	definition: Option<&'static TimelinePropertyDefinition>,
	kind: TimelinePropertyKind,
	target: Option<TimelineTarget>,
}

//==============================================================================
/// 키를 시간순으로 정렬. (같은 시간은 원래 차례를 지킨다)
//==============================================================================
fn sort_keys_by_time(keys: &mut [TimelineKey]) {
	keys.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap_or(std::cmp::Ordering::Equal));
}

//==============================================================================
/// 서술 트랙의 키 목록 읽기.
//==============================================================================
fn read_track_keys(track: &Value) -> Vec<TimelineKey> {
	match track.get("keys").and_then(|value| value.as_array()) {
		Some(key_array) => {
			key_array.iter().map(TimelineKey::from_json).collect()
		}
		None => {
			Vec::new()
		}
	}
}

/// 이벤트 핸들러. ((대상 이름, 이벤트 이름, 키, 타임라인, 노드 트리))
pub type TimelineEventHandler = Box<dyn FnMut(&str, &str, &TimelineKey, &mut Timeline, &mut NodeTree)>;
/// 마커 핸들러. ((마커 이름, 마커, 타임라인, 노드 트리))
pub type TimelineMarkerHandler = Box<dyn FnMut(&str, &TimelineMarker, &mut Timeline, &mut NodeTree)>;
/// 완료 핸들러. ((타임라인, 노드 트리))
pub type TimelineCompleteHandler = Box<dyn FnMut(&mut Timeline, &mut NodeTree)>;
/// 대상 해석기. (이름 → 대상)
pub type TimelineTargetResolver = Box<dyn Fn(&str) -> Option<TimelineTarget>>;
/// 이미지 해석기. (이미지 이름 → 이미지)
pub type TimelineImageResolver<'a> = &'a dyn Fn(&str) -> Option<Rc<Image>>;

//==============================================================================
/// 타임라인.
/// - 서술(JSON)을 들고, 무대를 만들거나(build_stage) 이미 있는 노드에 붙인(bind) 뒤 tick 으로 재생한다.
///
/// # 예제
/// 무대를 만들고 상자를 2초 동안 오른쪽으로 옮기기
/// ```rust
/// use serde_json::json;
/// use vanilla::{NodeTree, Timeline, Vector2};
///
/// let description = json!({
/// 	"duration": 2,
/// 	"loop": false,
/// 	"stage": { "nodes": [{ "name": "Box", "type": "paint", "width": 40, "height": 40, "color": "#ff0000ff" }] },
/// 	"tracks": [{ "target": "Box", "property": "x", "keys": [{ "time": 0, "value": 0 }, { "time": 2, "value": 200 }] }],
/// });
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let mut timeline = Timeline::new(Some(description));
/// let node_table = timeline.build_stage(&mut node_tree, root_id, None);
/// let box_id = node_table["Box"];
/// timeline.play();
/// timeline.tick(&mut node_tree, 0.5);
/// assert_eq!(node_tree[box_id].get_local_position(), Vector2::create(50.0, 0.0));
/// timeline.tick(&mut node_tree, 2.0);
/// assert!(!timeline.is_playing());
/// assert_eq!(timeline.get_time(), 2.0);
/// assert_eq!(node_tree[box_id].get_local_position().x, 200.0);
/// ```
///
/// 씬 initialize 에서 무대를 만들고 tick 에서 재생
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, Timeline};
///
/// const INTRO_TIMELINE: &str = r#"{
/// 	"duration": 3,
/// 	"loop": true,
/// 	"stage": { "nodes": [{ "name": "Logo", "type": "text", "text": "Vanilla" }] },
/// 	"tracks": [{ "target": "Logo", "property": "opacity", "keys": [{ "time": 0, "value": 0 }, { "time": 1, "value": 1 }] }]
/// }"#;
///
/// struct IntroScene {
/// 	scene_base: SceneBase,
/// 	timeline: Timeline,
/// }
///
/// impl Scene for IntroScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn initialize(&mut self, _engine: &mut Engine) {
/// 		self.timeline = Timeline::load_from_text(INTRO_TIMELINE).unwrap_or_default();
/// 		let stage_id = self.scene_base.get_root_id();
/// 		self.timeline.build_stage(self.scene_base.get_node_tree_mut(), stage_id, None);
/// 		self.timeline.play();
/// 	}
///
/// 	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
/// 		self.scene_base.tick(engine, time_delta);
/// 		self.timeline.tick(self.scene_base.get_node_tree_mut(), time_delta);
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let intro_scene = IntroScene {
/// 	scene_base: SceneBase::new(),
/// 	timeline: Timeline::default(),
/// };
/// engine.run(Box::new(intro_scene)).unwrap();
/// ```
/// <!-- image: timeline.webp -->
//==============================================================================
pub struct Timeline {
	description: Value,
	compiled_tracks: Vec<CompiledTimelineTrack>,
	compiled_markers: Vec<TimelineMarker>,
	target_table: Vec<(String, TimelineTarget)>,
	target_resolver: Option<TimelineTargetResolver>,
	event_handler: Option<TimelineEventHandler>,
	marker_handler: Option<TimelineMarkerHandler>,
	complete_handler: Option<TimelineCompleteHandler>,
	time: f64,
	speed: f64,
	is_playing: bool,
	is_dirty: bool,
	/// 타임라인 시간으로 진행하는 파티클이 붙은 노드. (대상 노드에서 모은다)
	particle_node_ids: Vec<NodeId>,
	/// 이벤트 트랙이 없는 파티클 노드. (타임라인이 시작하면 바로 도는 것으로 본다)
	particle_free_run_node_ids: Vec<NodeId>,
	/// 마지막 evaluate 가 External 대상에 건 값. (evaluate 마다 새로 채운다)
	external_property_list: Vec<TimelineExternalProperty>,
}

impl Timeline {
	//==============================================================================
	/// 생성. (서술이 없으면 빈 서술)
	///
	/// # 인자
	/// - `description`: 서술 JSON. None 이거나 객체가 아니면 빈 서술(create_empty_description)이고, 빠진 stage, stage.nodes, tracks, markers 는 채운다.
	///
	/// # 돌려줌
	/// 멈춘 타임라인. (시간 0, 속도 1, 컴파일은 처음 쓸 때 한다)
	//==============================================================================
	pub fn new(description: Option<Value>) -> Timeline {
		let mut timeline = Timeline {
			description: Value::Null,
			compiled_tracks: Vec::new(),
			compiled_markers: Vec::new(),
			target_table: Vec::new(),
			target_resolver: None,
			event_handler: None,
			marker_handler: None,
			complete_handler: None,
			time: 0.0,
			speed: 1.0,
			is_playing: false,
			is_dirty: true,
			particle_node_ids: Vec::new(),
			particle_free_run_node_ids: Vec::new(),
			external_property_list: Vec::new(),
		};
		timeline.set_description(description.unwrap_or_else(Timeline::create_empty_description));
		timeline
	}

	//==============================================================================
	/// 빈 서술 생성. (정적)
	///
	/// # 돌려줌
	/// 이름 "Timeline", 길이 3 초, 초당 30 프레임, 반복, 960 x 540 무대(배경 "#101114ff")에 노드, 트랙, 마커가 없는 서술.
	//==============================================================================
	pub fn create_empty_description() -> Value {
		json!({
			"name": "Timeline",
			"duration": 3,
			"frameRate": 30,
			"loop": true,
			"stage": { "width": 960, "height": 540, "backgroundColor": "#101114ff", "nodes": [] },
			"tracks": [],
			"markers": [],
		})
	}

	//==============================================================================
	/// JSON 글에서 만들기. (원본 loadFromUrl 의 response.json() 다음 자리)
	///
	/// # 인자
	/// - `text`: 서술 JSON 글.
	///
	/// # 돌려줌
	/// 그 서술의 타임라인. JSON 으로 읽지 못하면 까닭을 담은 Err.
	///
	/// # 예제
	/// JSON 글에서 만들기와 실패
	/// ```rust
	/// use vanilla::Timeline;
	///
	/// let timeline = Timeline::load_from_text(r#"{ "duration": 1.5, "loop": false }"#).expect("timeline");
	/// assert_eq!(timeline.get_duration(), 1.5);
	/// assert!(!timeline.is_loop());
	/// assert!(Timeline::load_from_text("{ broken").is_err());
	/// ```
	//==============================================================================
	pub fn load_from_text(text: &str) -> Result<Timeline, String> {
		let description: Value = serde_json::from_str(text).map_err(|error| format!("Timeline: JSON 을 읽지 못했습니다. ({})", error))?;
		Ok(Timeline::new(Some(description)))
	}

	//==============================================================================
	/// 주소에서 읽기. (원본 정적 await Timeline.loadFromUrl(url), 애셋 경로를 받기 부탁하고 엔진 작업 안에서 await 한다)
	/// - 받기나 JSON 읽기에 실패하면 까닭이다. (원본은 Promise 가 거부된다)
	///
	/// # 인자
	/// - `engine`: 바이트 받기를 부탁할 엔진. (부탁은 이 자리에서 바로 한다)
	/// - `url`: 애셋 폴더 기준 경로나 "data:" 주소.
	///
	/// # 돌려줌
	/// await 하면 타임라인이 나오는 Future. 받기나 JSON 읽기에 실패하면 까닭을 담은 Err. (엔진 작업 안에서 await 한다)
	///
	/// # 예제
	/// 씬 create 에서 읽기를 엔진 작업으로 맡기기
	/// ```rust,no_run
	/// use vanilla::{Engine, Scene, SceneBase, Timeline};
	///
	/// struct IntroScene {
	/// 	scene_base: SceneBase,
	/// 	timeline: Option<Timeline>,
	/// }
	///
	/// impl Scene for IntroScene {
	/// 	fn get_scene_base(&self) -> &SceneBase {
	/// 		&self.scene_base
	/// 	}
	///
	/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
	/// 		&mut self.scene_base
	/// 	}
	///
	/// 	fn create(&mut self, engine: &mut Engine) {
	/// 		let timeline_future = Timeline::load_from_url(engine, "data:application/json,{\"duration\":2,\"loop\":true}");
	/// 		let load_task_handle = engine.spawn_scene_task(self, |mut scene_context| async move {
	/// 			match timeline_future.await {
	/// 				Ok(timeline) => {
	/// 					scene_context.with(move |scene, _engine| scene.timeline = Some(timeline));
	/// 				}
	/// 				Err(error) => {
	/// 					eprintln!("{}", error);
	/// 				}
	/// 			}
	/// 		});
	/// 		self.scene_base.track_task(&load_task_handle);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn load_from_url(engine: &Engine, url: &str) -> impl Future<Output = Result<Timeline, String>> + 'static {
		let load_handle = engine.request_asset_bytes(url);
		async move {
			let bytes = load_handle.await?;
			Timeline::load_from_text(&String::from_utf8_lossy(&bytes))
		}
	}

	//==============================================================================
	/// 읽은 JsonAsset 에서 만들기. (아직 안 읽혔거나 실패했으면 None)
	///
	/// # 인자
	/// - `json_asset`: 서술 JSON 을 읽은 애셋. (서술은 복제한다)
	///
	/// # 돌려줌
	/// 그 서술의 타임라인. 애셋이 아직 안 읽혔거나 JSON 으로 풀지 못했으면 None.
	///
	/// # 예제
	/// 읽기 전에는 None, 바이트가 들어오면 타임라인
	/// ```rust
	/// use vanilla::{JsonAsset, Timeline};
	///
	/// let mut timeline_asset = JsonAsset::new();
	/// assert!(Timeline::create_from_json_asset(&mut timeline_asset).is_none());
	/// timeline_asset.load_from_bytes("intro.timeline.json", br#"{ "duration": 2, "loop": false }"#.to_vec());
	/// let timeline = Timeline::create_from_json_asset(&mut timeline_asset).expect("timeline");
	/// assert_eq!(timeline.get_duration(), 2.0);
	/// ```
	//==============================================================================
	pub fn create_from_json_asset(json_asset: &mut JsonAsset) -> Option<Timeline> {
		let description = json_asset.get_data()?.clone();
		Some(Timeline::new(Some(description)))
	}

	//==============================================================================
	/// 서술 설정. (빠진 stage / nodes / tracks / markers 를 채우고 다시 컴파일하게 한다)
	///
	/// # 인자
	/// - `description`: 새 서술 JSON. 객체가 아니면 빈 서술(create_empty_description)을 쓴다.
	//==============================================================================
	pub fn set_description(&mut self, description: Value) {
		self.description = if description.is_object() {
			description
		}
		else {
			Timeline::create_empty_description()
		};
		let description_object = self.description.as_object_mut().expect("description object");
		if !description_object.get("stage").is_some_and(|stage| stage.is_object()) {
			description_object.insert(String::from("stage"), json!({ "width": 960, "height": 540, "backgroundColor": "#101114ff", "nodes": [] }));
		}
		if let Some(stage_object) = description_object.get_mut("stage").and_then(|stage| stage.as_object_mut()) {
			if !stage_object.get("nodes").is_some_and(|nodes| nodes.is_array()) {
				stage_object.insert(String::from("nodes"), json!([]));
			}
		}
		if !description_object.get("tracks").is_some_and(|tracks| tracks.is_array()) {
			description_object.insert(String::from("tracks"), json!([]));
		}
		if !description_object.get("markers").is_some_and(|markers| markers.is_array()) {
			description_object.insert(String::from("markers"), json!([]));
		}
		self.invalidate();
	}

	//==============================================================================
	/// 서술 반환.
	///
	/// # 돌려줌
	/// 서술 JSON 의 참조.
	//==============================================================================
	pub fn get_description(&self) -> &Value {
		&self.description
	}

	//==============================================================================
	/// 서술 반환. (변경용. 트랙 / 키를 고쳤으면 invalidate 를 부른다)
	///
	/// # 돌려줌
	/// 서술 JSON 의 변경 참조.
	//==============================================================================
	pub fn get_description_mut(&mut self) -> &mut Value {
		&mut self.description
	}

	//==============================================================================
	/// 다시 컴파일 요청. (트랙 / 키가 바뀌었을 때)
	//==============================================================================
	pub fn invalidate(&mut self) {
		self.is_dirty = true;
	}

	//==============================================================================
	/// 서술 배열 반환. (stage.nodes 처럼 경로로)
	//==============================================================================
	fn get_description_array(&self, pointer: &str) -> Vec<Value> {
		match self.description.pointer(pointer).and_then(|value| value.as_array()) {
			Some(array) => {
				array.clone()
			}
			None => {
				Vec::new()
			}
		}
	}

	//==============================================================================
	/// 컴파일. (키를 시간순으로 정렬하고 대상 / 속성 정의를 붙인 뒤 파티클을 모은다)
	/// - seek, tick, evaluate 가 서술이 바뀌었으면 스스로 부른다.
	///
	/// # 인자
	/// - `node_tree`: 대상 노드가 든 노드 트리. (대상 노드의 파티클 시스템을 찾는다)
	//==============================================================================
	pub fn compile(&mut self, node_tree: &mut NodeTree) {
		let track_descriptions = self.get_description_array("/tracks");
		let mut compiled_tracks: Vec<CompiledTimelineTrack> = Vec::with_capacity(track_descriptions.len());
		for track in track_descriptions.iter() {
			let target_name = track.get("target").and_then(|value| value.as_str()).unwrap_or("").to_string();
			let property_name = track.get("property").and_then(|value| value.as_str()).unwrap_or("").to_string();
			let definition = find_timeline_property_definition(&property_name);
			let mut keys = read_track_keys(track);
			sort_keys_by_time(&mut keys);
			let target = self.resolve_target(&target_name);
			let kind = match definition {
				Some(definition) => {
					definition.kind
				}
				None => {
					match keys.first().map(|key| &key.value) {
						Some(TimelineValue::Number(_)) | None => {
							TimelinePropertyKind::Number
						}
						Some(_) => {
							TimelinePropertyKind::String
						}
					}
				}
			};
			compiled_tracks.push(CompiledTimelineTrack {
				target_name,
				property_name,
				is_enabled: track.get("enabled") != Some(&Value::Bool(false)),
				keys,
				definition,
				kind,
				target,
			});
		}
		self.compiled_tracks = compiled_tracks;

		let mut compiled_markers: Vec<TimelineMarker> = self.get_description_array("/markers").iter().map(|marker| {
			TimelineMarker {
				time: marker.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0),
				name: marker.get("name").and_then(|value| value.as_str()).unwrap_or("").to_string(),
			}
		}).collect();
		compiled_markers.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap_or(std::cmp::Ordering::Equal));
		self.compiled_markers = compiled_markers;
		self.is_dirty = false;
		self.collect_particle_systems(node_tree);
	}

	//==============================================================================
	/// 더러우면 컴파일.
	//==============================================================================
	fn compile_if_dirty(&mut self, node_tree: &mut NodeTree) {
		if self.is_dirty {
			self.compile(node_tree);
		}
	}

	//==============================================================================
	/// 대상 노드의 파티클 시스템 수집.
	/// - 수동 tick 으로 바꿔 타임라인 시간으로만 진행하고, 씨앗을 줘서 되감을 때마다 같은 결과가 나오게 한다.
	///
	/// # 인자
	/// - `node_tree`: 대상 노드가 든 노드 트리. (대상 표의 노드만 본다, 씨앗이 이미 있으면 두고 없으면 수집 차례로 준다)
	//==============================================================================
	pub fn collect_particle_systems(&mut self, node_tree: &mut NodeTree) {
		self.particle_node_ids.clear();
		self.particle_free_run_node_ids.clear();
		let mut event_target_ids: Vec<NodeId> = Vec::new();
		for compiled_track in self.compiled_tracks.iter() {
			if compiled_track.property_name == "event" {
				if let Some(TimelineTarget::Node(node_id)) = compiled_track.target {
					event_target_ids.push(node_id);
				}
			}
		}
		for (_, target) in self.target_table.iter() {
			let node_id = match target {
				TimelineTarget::Node(node_id) => {
					*node_id
				}
				TimelineTarget::External(_) => {
					continue;
				}
			};
			if self.particle_node_ids.contains(&node_id) {
				continue;
			}
			let particle_system = match node_tree.get_node_mut(node_id).and_then(|node| node.get_component_mut::<ParticleSystem>()) {
				Some(particle_system) => {
					particle_system
				}
				None => {
					continue;
				}
			};
			particle_system.set_manual_tick(true);
			if particle_system.get_random_seed().is_none() {
				let random_seed = PARTICLE_RANDOM_SEED_BASE.wrapping_add((self.particle_node_ids.len() as u32).wrapping_mul(PARTICLE_RANDOM_SEED_STEP));
				particle_system.set_random_seed(random_seed);
			}
			if !event_target_ids.contains(&node_id) {
				self.particle_free_run_node_ids.push(node_id);
			}
			self.particle_node_ids.push(node_id);
		}
	}

	//==============================================================================
	/// 파티클 시간 진행. (재생 중 매 프레임)
	///
	/// # 인자
	/// - `node_tree`: 파티클 노드가 든 노드 트리.
	/// - `time_delta`: 진행할 시간. (초, 0 이하이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn simulate_particles(&mut self, node_tree: &mut NodeTree, time_delta: f64) {
		if time_delta <= 0.0 {
			return;
		}
		for node_id in self.particle_node_ids.iter() {
			if let Some(particle_system) = node_tree.get_node_mut(*node_id).and_then(|node| node.get_component_mut::<ParticleSystem>()) {
				particle_system.simulate(time_delta);
			}
		}
	}

	//==============================================================================
	/// 파티클 재시뮬레이션. (스크럽 / 되감기. 비운 뒤 0 초부터 이벤트를 다시 밟으며 진행)
	/// - 한 걸음은 1/60 초와 time / 600 가운데 큰 것이다. 이벤트 트랙이 없는 파티클은 0 초에 튼 것으로 본다.
	///
	/// # 인자
	/// - `node_tree`: 파티클 노드가 든 노드 트리.
	/// - `time`: 맞출 타임라인 시간. (초)
	//==============================================================================
	pub fn resimulate_particles(&mut self, node_tree: &mut NodeTree, time: f64) {
		self.compile_if_dirty(node_tree);
		if self.particle_node_ids.is_empty() {
			return;
		}
		for node_id in self.particle_node_ids.iter() {
			if let Some(particle_system) = node_tree.get_node_mut(*node_id).and_then(|node| node.get_component_mut::<ParticleSystem>()) {
				particle_system.stop(true);
			}
		}
		for node_id in self.particle_free_run_node_ids.iter() {
			if let Some(particle_system) = node_tree.get_node_mut(*node_id).and_then(|node| node.get_component_mut::<ParticleSystem>()) {
				particle_system.play();
			}
		}

		// 파티클 대상 이벤트 키를 시간순으로 모은다.
		let mut event_list: Vec<(f64, usize, usize)> = Vec::new();
		for (track_index, compiled_track) in self.compiled_tracks.iter().enumerate() {
			if compiled_track.property_name != "event" || !compiled_track.is_enabled {
				continue;
			}
			let node_id = match compiled_track.target {
				Some(TimelineTarget::Node(node_id)) => {
					node_id
				}
				_ => {
					continue;
				}
			};
			if !node_tree.get_node(node_id).is_some_and(|node| node.has_component::<ParticleSystem>()) {
				continue;
			}
			for (key_index, key) in compiled_track.keys.iter().enumerate() {
				if key.time <= time {
					event_list.push((key.time, track_index, key_index));
				}
			}
		}
		event_list.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(std::cmp::Ordering::Equal));

		let step_seconds = (1.0f64 / 60.0).max(time / 600.0);
		let mut current_time = 0.0f64;
		for (event_time, track_index, key_index) in event_list {
			self.advance_particles_to(node_tree, &mut current_time, event_time, step_seconds);
			self.dispatch_event(node_tree, track_index, key_index, true);
		}
		self.advance_particles_to(node_tree, &mut current_time, time, step_seconds);
	}

	//==============================================================================
	/// 파티클을 step_seconds 씩 target_time 까지 진행. (재시뮬레이션용)
	//==============================================================================
	fn advance_particles_to(&mut self, node_tree: &mut NodeTree, current_time: &mut f64, target_time: f64, step_seconds: f64) {
		while *current_time < target_time - 0.000001 {
			let step_delta = step_seconds.min(target_time - *current_time);
			self.simulate_particles(node_tree, step_delta);
			*current_time += step_delta;
		}
	}

	//==============================================================================
	/// 대상 해석. (대상 표 → 사용자 해석기)
	///
	/// # 인자
	/// - `target_name`: 트랙 대상 이름.
	///
	/// # 돌려줌
	/// 대상 표에 있으면 그것, 없으면 해석기가 준 대상. 둘 다 없으면 None.
	//==============================================================================
	pub fn resolve_target(&self, target_name: &str) -> Option<TimelineTarget> {
		if let Some(target) = self.get_target(target_name) {
			return Some(target);
		}
		if let Some(target_resolver) = self.target_resolver.as_ref() {
			if let Some(resolved_target) = target_resolver(target_name) {
				return Some(resolved_target);
			}
		}
		None
	}

	//==============================================================================
	/// 대상 해석기 설정. (이름 → 대상. 노드가 아닌 것은 TimelineTarget::External(객체))
	///
	/// # 인자
	/// - `target_resolver`: 대상 표에 없는 이름을 받아 대상을 주는 함수. (모르는 이름은 None) None 이면 해석기를 뗀다. 컴파일 때 부른다.
	///
	/// # 예제
	/// "screen" 이름을 External 대상으로 풀기
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{Timeline, TimelinePropertyTarget, TimelineTarget, TimelineValue};
	///
	/// struct Silent;
	///
	/// impl TimelinePropertyTarget for Silent {
	/// 	fn set_timeline_property(&mut self, _property_name: &str, _value: &TimelineValue) {
	/// 	}
	/// }
	///
	/// let screen: Rc<RefCell<dyn TimelinePropertyTarget>> = Rc::new(RefCell::new(Silent));
	/// let mut timeline = Timeline::default();
	/// let resolved_screen = screen.clone();
	/// timeline.set_target_resolver(Some(Box::new(move |target_name: &str| {
	/// 	if target_name == "screen" {
	/// 		Some(TimelineTarget::External(resolved_screen.clone()))
	/// 	}
	/// 	else {
	/// 		None
	/// 	}
	/// })));
	/// assert_eq!(timeline.resolve_target("screen"), Some(TimelineTarget::External(screen)));
	/// assert_eq!(timeline.resolve_target("missing"), None);
	/// assert_eq!(timeline.get_target("screen"), None);
	/// ```
	//==============================================================================
	pub fn set_target_resolver(&mut self, target_resolver: Option<TimelineTargetResolver>) {
		self.target_resolver = target_resolver;
		self.invalidate();
	}

	//==============================================================================
	/// 대상 직접 등록. (같은 이름이 있으면 바꾼다)
	///
	/// # 인자
	/// - `target_name`: 트랙 대상 이름.
	/// - `target`: 그 이름의 대상. (노드 핸들 또는 External 객체)
	//==============================================================================
	pub fn set_target(&mut self, target_name: &str, target: TimelineTarget) {
		match self.target_table.iter_mut().find(|(name, _)| name == target_name) {
			Some(entry) => {
				entry.1 = target;
			}
			None => {
				self.target_table.push((target_name.to_string(), target));
			}
		}
		self.invalidate();
	}

	//==============================================================================
	/// 대상 반환. (대상 표에 있는 것만)
	///
	/// # 인자
	/// - `target_name`: 트랙 대상 이름.
	///
	/// # 돌려줌
	/// 대상 표에 등록된 대상의 복제. 없으면 None. (해석기는 묻지 않는다)
	//==============================================================================
	pub fn get_target(&self, target_name: &str) -> Option<TimelineTarget> {
		self.target_table.iter().find(|(name, _)| name == target_name).map(|(_, target)| target.clone())
	}

	//==============================================================================
	/// 노드 트리에 이름으로 붙이기. (무대를 직접 만든 경우. 뿌리 자신, 또는 그 자손에서 트랙 대상 이름을 찾는다)
	/// - 대상 표에 이미 있는 이름은 건너뛰고, 못 찾은 이름은 등록하지 않는다.
	///
	/// # 인자
	/// - `node_tree`: 노드를 찾을 노드 트리.
	/// - `root_id`: 찾기를 시작할 뿌리 노드.
	///
	/// # 예제
	/// 이미 있는 Hero 노드에 붙여 배율 트랙 걸기
	/// ```rust
	/// use serde_json::json;
	/// use vanilla::{NodeTree, Timeline, TimelineTarget};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let world_id = node_tree.create_child(root_id, "World");
	/// let hero_id = node_tree.create_child(world_id, "Hero");
	/// let mut timeline = Timeline::new(Some(json!({
	/// 	"duration": 1,
	/// 	"tracks": [{ "target": "Hero", "property": "scaleX", "keys": [{ "time": 0, "value": 1 }, { "time": 1, "value": 3 }] }],
	/// })));
	/// timeline.bind(&node_tree, root_id);
	/// assert_eq!(timeline.get_target("Hero"), Some(TimelineTarget::Node(hero_id)));
	/// timeline.seek(&mut node_tree, 0.5);
	/// assert_eq!(node_tree[hero_id].get_local_scale().x, 2.0);
	/// ```
	//==============================================================================
	pub fn bind(&mut self, node_tree: &NodeTree, root_id: NodeId) {
		let track_descriptions = self.get_description_array("/tracks");
		for track in track_descriptions.iter() {
			let target_name = track.get("target").and_then(|value| value.as_str()).unwrap_or("");
			if self.get_target(target_name).is_some() {
				continue;
			}
			let is_root_matched = node_tree.get_node(root_id).is_some_and(|root_node| root_node.get_name() == target_name);
			let found_id = if is_root_matched {
				Some(root_id)
			}
			else {
				node_tree.find_descendant_by_name(root_id, target_name)
			};
			if let Some(found_id) = found_id {
				self.target_table.push((target_name.to_string(), TimelineTarget::Node(found_id)));
			}
		}
		self.invalidate();
	}

	//==============================================================================
	/// 무대 노드 생성. (서술의 stage.nodes 를 노드 트리로 만들고 대상으로 등록한다. 이름 → 노드 표를 돌려준다)
	///
	/// # 인자
	/// - `node_tree`: 노드를 만들 노드 트리.
	/// - `parent_id`: 무대 부모 노드. (서술에 parent 가 없거나, 못 찾거나, 자기 자신이나 고리가 되는 노드가 여기 붙는다)
	/// - `image_resolver`: sprite 노드의 이미지 이름을 이미지로 바꾸는 함수. None 이면 "data:" 주소 이미지만 푼다.
	///
	/// # 돌려줌
	/// 노드 이름 → 만든 노드 표. (같은 이름이 둘이면 뒤의 것)
	///
	/// # 예제
	/// 부모 관계가 있는 무대 만들기
	/// ```rust
	/// use serde_json::json;
	/// use vanilla::{NodeTree, Timeline, TimelineTarget};
	///
	/// let mut timeline = Timeline::new(Some(json!({
	/// 	"duration": 1,
	/// 	"stage": { "nodes": [
	/// 		{ "name": "Group", "type": "group", "x": 100 },
	/// 		{ "name": "Box", "type": "paint", "parent": "Group" },
	/// 	] },
	/// })));
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_table = timeline.build_stage(&mut node_tree, root_id, None);
	/// assert_eq!(node_tree.get_parent_id(node_table["Box"]), Some(node_table["Group"]));
	/// assert_eq!(node_tree.get_parent_id(node_table["Group"]), Some(root_id));
	/// assert_eq!(timeline.get_target("Box"), Some(TimelineTarget::Node(node_table["Box"])));
	/// ```
	//==============================================================================
	pub fn build_stage(&mut self, node_tree: &mut NodeTree, parent_id: NodeId, image_resolver: Option<TimelineImageResolver>) -> HashMap<String, NodeId> {
		let node_descriptions = self.get_description_array("/stage/nodes");
		let mut node_table: HashMap<String, NodeId> = HashMap::new();
		let mut created_list: Vec<(NodeId, Option<String>)> = Vec::with_capacity(node_descriptions.len());
		for node_description in node_descriptions.iter() {
			let node_id = Timeline::create_stage_node(node_tree, node_description, image_resolver);
			let node_name = node_description.get("name").and_then(|value| value.as_str()).unwrap_or("").to_string();
			let parent_name = node_description.get("parent").and_then(|value| value.as_str()).map(|name| name.to_string());
			node_table.insert(node_name.clone(), node_id);
			self.set_target(&node_name, TimelineTarget::Node(node_id));
			created_list.push((node_id, parent_name));
		}
		for (node_id, parent_name) in created_list {
			let parent_description_node_id = parent_name.and_then(|parent_name| node_table.get(&parent_name).copied());
			// 부모가 자기 자신이거나 고리가 되면 add_child 가 거절하므로 무대 부모에 붙인다.
			let is_added = match parent_description_node_id {
				Some(parent_description_node_id) => {
					node_tree.add_child(parent_description_node_id, node_id)
				}
				None => {
					false
				}
			};
			if !is_added {
				node_tree.add_child(parent_id, node_id);
			}
		}
		self.invalidate();
		node_table
	}

	//==============================================================================
	/// 무대 노드 한 개 생성. (정적. 붙이지 않은 노드를 만든다)
	///
	/// # 인자
	/// - `node_tree`: 노드를 만들 노드 트리.
	/// - `node_description`: 서술 stage.nodes 의 노드 객체. (parent 는 보지 않는다)
	/// - `image_resolver`: sprite 노드의 이미지 이름을 이미지로 바꾸는 함수. None 이면 "data:" 주소 이미지만 푼다.
	///
	/// # 돌려줌
	/// 만든 노드. (어느 부모에도 붙지 않았고 대상으로 등록하지도 않는다)
	//==============================================================================
	pub fn create_stage_node(node_tree: &mut NodeTree, node_description: &Value, image_resolver: Option<TimelineImageResolver>) -> NodeId {
		let node_name = node_description.get("name").and_then(|value| value.as_str()).unwrap_or("");
		let node_id = node_tree.create_node(node_name);
		Timeline::apply_stage_node_description(&mut node_tree[node_id], node_description, image_resolver);
		node_id
	}

	//==============================================================================
	/// 노드에 서술 반영. (정적. 편집기가 값을 고칠 때도 쓴다)
	/// - 변환, 보이기를 걸고 type 에 맞는 컴포넌트(paint 는 Paint, sprite 는 ShaderSprite 와 격자, text 는 Text, particle 은 ParticleSystem, sound 는 TimelineSound)를 없으면 붙여 값을 건다.
	///
	/// # 인자
	/// - `node`: 값을 걸 노드.
	/// - `node_description`: 서술 노드 객체. (빠진 항목은 TIMELINE_NODE_DEFAULTS)
	/// - `image_resolver`: sprite 노드의 이미지 이름을 이미지로 바꾸는 함수. None 이면 "data:" 주소 이미지만 푼다.
	//==============================================================================
	pub fn apply_stage_node_description(node: &mut WorldNode, node_description: &Value, image_resolver: Option<TimelineImageResolver>) {
		let description = TimelineNodeDescription::from_json(node_description);
		node.set_pivot(Vector2::create(description.pivot_x, description.pivot_y));
		node.set_anchor(Pivot::TOP_LEFT);
		node.set_content_size(Vector2::create(description.width, description.height));
		node.set_local_position(Vector2::create(description.x, description.y));
		node.set_local_scale(Vector2::create(description.scale_x, description.scale_y));
		node.set_local_rotation(description.rotation);
		node.set_local_opacity(description.opacity);
		node.set_active(description.visible);

		if description.node_type == "paint" {
			let paint_component = get_or_add_component(node, Paint::new);
			paint_component.set_color(create_color_from_text(&description.color));
			paint_component.set_round_size(description.round_size);
		}
		else if description.node_type == "sprite" {
			let grid_columns = description.frame_columns.max(1.0) as usize;
			let grid_rows = description.frame_rows.max(1.0) as usize;
			get_or_add_component(node, || TimelineSpriteFrameGrid::new(grid_columns, grid_rows)).set_grid(grid_columns, grid_rows);
			let mut image: Option<Rc<Image>> = None;
			if !description.image.is_empty() {
				if let Some(image_resolver) = image_resolver {
					image = image_resolver(&description.image);
				}
			}
			#[cfg(not(target_arch = "wasm32"))]
			let pending_image_load: Option<(AssetLoadHandle, i64)> = None;
			#[cfg(not(target_arch = "wasm32"))]
			{
				if image.is_none() && description.image.starts_with("data:") {
					image = load_data_url_image(&description.image);
				}
			}
			#[cfg(target_arch = "wasm32")]
			let pending_image_load = if image.is_none() && description.image.starts_with("data:") {
				Some((request_data_url_image(&description.image), round_half_up(description.frame) as i64))
			}
			else {
				None
			};
			get_or_add_component(node, || TimelineSpriteFrameGrid::new(grid_columns, grid_rows)).pending_image_load = pending_image_load;
			{
				let shader_sprite = get_or_add_component(node, ShaderSprite::new);
				shader_sprite.get_sprite_mut().set_image(image);
				shader_sprite.get_sprite_mut().set_image_rect(Rect::zero());
			}
			apply_sprite_frame(node, round_half_up(description.frame) as i64);
			let shader_sprite = get_or_add_component(node, ShaderSprite::new);
			let sprite_color_text = if description.color == "#ffffffff" {
				"#ffffff00"
			}
			else {
				description.color.as_str()
			};
			shader_sprite.get_sprite_mut().set_color(create_color_from_text(sprite_color_text));
			shader_sprite.get_sprite_mut().set_sprite_blend_mode(BlendMode::from_name(&description.blend_mode).unwrap_or(BlendMode::Normal));
			shader_sprite.set_effect(ShaderSpriteEffect::from_name(&description.effect).unwrap_or(ShaderSpriteEffect::None));
			shader_sprite.set_effect_progress(description.effect_progress);
			if !description.effect_color.is_empty() {
				shader_sprite.set_effect_color(create_color_from_text(&description.effect_color));
			}
		}
		else if description.node_type == "text" {
			let text_component = get_or_add_component(node, Text::new);
			text_component.set_text(&description.text);
			text_component.set_font_size(description.font_size);
			text_component.set_bold(description.bold);
			text_component.set_text_color(create_color_from_text(&description.color));
			text_component.set_text_align(read_text_align(&description.text_align));
			text_component.set_text_baseline(TextBaseline::Middle);
		}
		else if description.node_type == "particle" {
			let particle_system = get_or_add_component(node, ParticleSystem::new);
			if let Some(particle_description) = description.particle.as_ref() {
				particle_system.apply_description(particle_description);
			}
		}
		else if description.node_type == "sound" {
			let sound = get_or_add_component(node, TimelineSound::new);
			sound.volume = description.volume;
			if !description.audio.is_empty() && description.audio != sound.audio_source {
				sound.audio_source = description.audio.clone();
				sound.audio_asset = None;
			}
		}
	}

	//==============================================================================
	/// 사운드 노드 준비. (원본 setAudioManager + 사운드 노드의 애셋 읽기 자리)
	/// - 대상 노드 가운데 TimelineSound 가 붙은 것마다 아직 없는 오디오 애셋을 읽고 플레이어를 만든다. 새로 읽은 애셋을 돌려준다.
	///
	/// # 인자
	/// - `node_tree`: 사운드 노드가 든 노드 트리.
	/// - `engine`: 오디오 애셋을 읽고 플레이어를 만들 엔진.
	///
	/// # 돌려줌
	/// 이번에 새로 읽은 오디오 애셋 목록. (씬이 track_audio_asset 으로 맡겨도 된다, 읽지 못한 것은 빼고 표준 오류에 까닭을 찍는다)
	///
	/// # 예제
	/// 무대를 만든 뒤 소리를 읽고 이벤트 키로 틀기
	/// ```rust,no_run
	/// use serde_json::json;
	/// use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, Timeline};
	///
	/// struct SoundScene {
	/// 	scene_base: SceneBase,
	/// 	timeline: Timeline,
	/// }
	///
	/// impl Scene for SoundScene {
	/// 	fn get_scene_base(&self) -> &SceneBase {
	/// 		&self.scene_base
	/// 	}
	///
	/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
	/// 		&mut self.scene_base
	/// 	}
	///
	/// 	fn initialize(&mut self, engine: &mut Engine) {
	/// 		self.timeline = Timeline::new(Some(json!({
	/// 			"duration": 2,
	/// 			"stage": { "nodes": [{ "name": "Hit", "type": "sound", "audio": "tests/assets/audio/wav_s16_stereo.wav" }] },
	/// 			"tracks": [{ "target": "Hit", "property": "event", "keys": [{ "time": 0.5, "value": "play" }] }],
	/// 		})));
	/// 		let root_id = self.scene_base.get_root_id();
	/// 		self.timeline.build_stage(self.scene_base.get_node_tree_mut(), root_id, None);
	/// 		let audio_asset_list = self.timeline.load_sounds(self.scene_base.get_node_tree_mut(), engine);
	/// 		for audio_asset in audio_asset_list.iter() {
	/// 			self.scene_base.track_audio_asset(audio_asset);
	/// 		}
	/// 		self.timeline.play();
	/// 	}
	///
	/// 	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
	/// 		self.scene_base.tick(engine, time_delta);
	/// 		self.timeline.tick(self.scene_base.get_node_tree_mut(), time_delta);
	/// 	}
	/// }
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let sound_scene = SoundScene {
	/// 	scene_base: SceneBase::new(),
	/// 	timeline: Timeline::default(),
	/// };
	/// engine.run(Box::new(sound_scene)).unwrap();
	/// ```
	//==============================================================================
	pub fn load_sounds(&mut self, node_tree: &mut NodeTree, engine: &Engine) -> Vec<Rc<AudioAsset>> {
		let mut loaded_audio_assets: Vec<Rc<AudioAsset>> = Vec::new();
		for (_, target) in self.target_table.iter() {
			let node_id = match target {
				TimelineTarget::Node(node_id) => {
					*node_id
				}
				TimelineTarget::External(_) => {
					continue;
				}
			};
			let sound = match node_tree.get_node_mut(node_id).and_then(|node| node.get_component_mut::<TimelineSound>()) {
				Some(sound) => {
					sound
				}
				None => {
					continue;
				}
			};
			if sound.audio_asset.is_none() && !sound.audio_source.is_empty() {
				match engine.load_audio_asset(&sound.audio_source) {
					Ok(audio_asset) => {
						loaded_audio_assets.push(audio_asset.clone());
						sound.audio_asset = Some(audio_asset);
					}
					Err(error) => {
						eprintln!("Timeline: {}", error);
					}
				}
			}
			if sound.audio_player.is_none() {
				sound.audio_player = Some(engine.get_audio_manager().create_audio_player());
			}
			if let (Some(audio_player), Some(audio_asset)) = (sound.audio_player.as_mut(), sound.audio_asset.as_ref()) {
				audio_player.set_audio_asset(audio_asset.clone());
			}
		}
		loaded_audio_assets
	}

	//==============================================================================
	/// 사운드 노드 정지. (정적. 편집기가 정지 / 되감기 때 부른다)
	///
	/// # 인자
	/// - `node`: 멈출 노드. (TimelineSound 가 없거나 플레이어가 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn stop_sound(node: &mut WorldNode) {
		if let Some(sound) = node.get_component_mut::<TimelineSound>() {
			sound.stop();
		}
	}

	//==============================================================================
	/// 재생. (반복이 아닌데 끝에 있으면 처음부터)
	//==============================================================================
	pub fn play(&mut self) {
		if self.time >= self.get_duration() && !self.is_loop() {
			self.time = 0.0;
		}
		self.is_playing = true;
	}

	//==============================================================================
	/// 일시 정지.
	//==============================================================================
	pub fn pause(&mut self) {
		self.is_playing = false;
	}

	//==============================================================================
	/// 정지. (소리와 파티클을 멈추고 처음으로)
	///
	/// # 인자
	/// - `node_tree`: 대상 노드가 든 노드 트리. (0 초의 값을 다시 건다)
	//==============================================================================
	pub fn stop(&mut self, node_tree: &mut NodeTree) {
		self.is_playing = false;
		for (_, target) in self.target_table.iter() {
			if let TimelineTarget::Node(node_id) = target {
				if let Some(node) = node_tree.get_node_mut(*node_id) {
					Timeline::stop_sound(node);
				}
			}
		}
		for node_id in self.particle_node_ids.iter() {
			if let Some(particle_system) = node_tree.get_node_mut(*node_id).and_then(|node| node.get_component_mut::<ParticleSystem>()) {
				particle_system.stop(true);
			}
		}
		self.seek(node_tree, 0.0);
	}

	//==============================================================================
	/// 시간 이동. (이벤트는 발생하지 않는다. 파티클은 그 시간까지 다시 진행한다)
	///
	/// # 인자
	/// - `node_tree`: 대상 노드가 든 노드 트리.
	/// - `time`: 옮길 시간. (초, 0 ~ 길이로 자른다)
	//==============================================================================
	pub fn seek(&mut self, node_tree: &mut NodeTree, time: f64) {
		self.time = time.min(self.get_duration()).max(0.0);
		let current_time = self.time;
		self.evaluate(node_tree, current_time);
		self.resimulate_particles(node_tree, current_time);
	}

	//==============================================================================
	/// 갱신. (시간을 흘리고 값을 적용, 지나친 이벤트 / 마커를 발생)
	/// - 재생 중이 아니면 아무것도 하지 않는다. 반복이면 끝을 넘을 때 감아 돌고, 아니면 끝에서 멈추고 완료 핸들러를 부른다.
	///
	/// # 인자
	/// - `node_tree`: 대상 노드가 든 노드 트리. (핸들러에도 넘긴다)
	/// - `time_delta`: 지난 프레임에서 흐른 시간. (초, 속도를 곱해 흘린다)
	//==============================================================================
	pub fn tick(&mut self, node_tree: &mut NodeTree, time_delta: f64) {
		if !self.is_playing {
			return;
		}
		let duration = self.get_duration();
		let previous_time = self.time;
		let mut next_time = previous_time + time_delta * self.speed;
		let mut is_wrapped = false;
		if next_time >= duration {
			if self.is_loop() && duration > 0.0 {
				self.fire_events_between(node_tree, previous_time, duration);
				self.simulate_particles(node_tree, duration - previous_time);
				next_time %= duration;
				self.fire_events_between(node_tree, -1.0, next_time);
				is_wrapped = true;
			}
			else {
				self.fire_events_between(node_tree, previous_time, duration);
				self.simulate_particles(node_tree, duration - previous_time);
				self.time = duration;
				self.is_playing = false;
				self.evaluate(node_tree, duration);
				if let Some(mut complete_handler) = self.complete_handler.take() {
					complete_handler(self, node_tree);
					if self.complete_handler.is_none() {
						self.complete_handler = Some(complete_handler);
					}
				}
				return;
			}
		}
		else if next_time < 0.0 {
			next_time = 0.0;
		}
		else {
			self.fire_events_between(node_tree, previous_time, next_time);
		}
		self.time = next_time;
		self.evaluate(node_tree, next_time);
		// 파티클은 타임라인 시간으로만 진행한다. (감기면 처음부터 다시, 매 반복이 같게)
		if is_wrapped {
			self.resimulate_particles(node_tree, next_time);
		}
		else {
			self.simulate_particles(node_tree, next_time - previous_time);
		}
	}

	//==============================================================================
	/// 구간 안의 이벤트 키 / 마커 발생. (from_time 초과 ~ to_time 이하. 이벤트 트랙 차례, 그다음 마커)
	///
	/// # 인자
	/// - `node_tree`: 대상 노드가 든 노드 트리. (파티클, 사운드 노드가 이벤트를 받고 핸들러에도 넘긴다)
	/// - `from_time`: 구간 시작. (초, 이 시간의 키는 뺀다. 0 초 키를 넣으려면 음수를 준다)
	/// - `to_time`: 구간 끝. (초, 이 시간의 키는 넣는다)
	//==============================================================================
	pub fn fire_events_between(&mut self, node_tree: &mut NodeTree, from_time: f64, to_time: f64) {
		self.compile_if_dirty(node_tree);
		let is_inside = |time: f64| time > from_time && time <= to_time;
		let mut event_list: Vec<(usize, usize)> = Vec::new();
		for (track_index, compiled_track) in self.compiled_tracks.iter().enumerate() {
			if compiled_track.property_name != "event" || !compiled_track.is_enabled {
				continue;
			}
			for (key_index, key) in compiled_track.keys.iter().enumerate() {
				if is_inside(key.time) {
					event_list.push((track_index, key_index));
				}
			}
		}
		for (track_index, key_index) in event_list {
			self.dispatch_event(node_tree, track_index, key_index, false);
		}
		if self.marker_handler.is_some() {
			let passed_markers: Vec<TimelineMarker> = self.compiled_markers.iter().filter(|marker| is_inside(marker.time)).cloned().collect();
			for marker in passed_markers.iter() {
				let mut marker_handler = match self.marker_handler.take() {
					Some(marker_handler) => {
						marker_handler
					}
					None => {
						break;
					}
				};
				marker_handler(&marker.name, marker, self, node_tree);
				if self.marker_handler.is_none() {
					self.marker_handler = Some(marker_handler);
				}
			}
		}
	}

	//==============================================================================
	/// 이벤트 키 처리. (파티클 대상은 play / stop / clear / emit:N 을 스스로 처리하고, 사운드 대상은 play / loop / stop, 그 뒤 핸들러)
	/// - is_silent 는 재시뮬레이션용이다. (사운드 / 사용자 핸들러는 부르지 않고 파티클만)
	//==============================================================================
	fn dispatch_event(&mut self, node_tree: &mut NodeTree, track_index: usize, key_index: usize, is_silent: bool) {
		// 앞의 핸들러가 서술을 바꿔 다시 컴파일했으면 없는 트랙일 수 있다.
		let (target, target_name, key) = match self.compiled_tracks.get(track_index).and_then(|compiled_track| compiled_track.keys.get(key_index).map(|key| (compiled_track, key))) {
			Some((compiled_track, key)) => {
				(compiled_track.target.clone(), compiled_track.target_name.clone(), key.clone())
			}
			None => {
				return;
			}
		};
		let event_name = key.value.to_text();
		if let Some(TimelineTarget::Node(node_id)) = target {
			if let Some(node) = node_tree.get_node_mut(node_id) {
				let node_position = node.get_local_position();
				if !is_silent {
					if let Some(sound) = node.get_component_mut::<TimelineSound>() {
						sound.handle_event(&event_name);
					}
				}
				if let Some(particle_system) = node.get_component_mut::<ParticleSystem>() {
					if event_name == "play" {
						particle_system.play();
					}
					else if event_name == "stop" {
						particle_system.stop(false);
					}
					else if event_name == "clear" {
						particle_system.stop(true);
					}
					else if event_name.starts_with("emit") {
						let emit_count = match event_name.find(':') {
							Some(separator_index) => {
								parse_number_text(&event_name[separator_index + 1..])
							}
							None => {
								DEFAULT_EMIT_COUNT
							}
						};
						let emit_count = if emit_count.is_finite() {
							emit_count
						}
						else {
							DEFAULT_EMIT_COUNT
						};
						// 원본처럼 낼 때의 노드 위치로 낸다. (월드 공간)
						particle_system.set_node_position(node_position);
						particle_system.emit(emit_count.max(0.0).ceil() as usize, None);
					}
				}
			}
		}
		if !is_silent {
			if let Some(mut event_handler) = self.event_handler.take() {
				event_handler(&target_name, &event_name, &key, self, node_tree);
				if self.event_handler.is_none() {
					self.event_handler = Some(event_handler);
				}
			}
		}
	}

	//==============================================================================
	/// 시간의 값 적용. (연속 속성만, 이벤트는 tick 에서만 발생. External 대상에는 set_timeline_property 로 넘기고 건 값을 적어 둔다)
	/// - 꺼진 트랙, 키가 없는 트랙, 대상을 못 찾은 트랙은 건너뛴다. 시간은 자르지 않는다.
	///
	/// # 인자
	/// - `node_tree`: 대상 노드가 든 노드 트리.
	/// - `time`: 표본할 시간. (초)
	//==============================================================================
	pub fn evaluate(&mut self, node_tree: &mut NodeTree, time: f64) {
		self.compile_if_dirty(node_tree);
		self.external_property_list.clear();
		for compiled_track in self.compiled_tracks.iter() {
			if !compiled_track.is_enabled || compiled_track.keys.is_empty() || compiled_track.property_name == "event" {
				continue;
			}
			let target = match compiled_track.target.as_ref() {
				Some(target) => {
					target
				}
				None => {
					continue;
				}
			};
			let value = Timeline::sample_keys(&compiled_track.keys, compiled_track.kind, time);
			match target {
				TimelineTarget::Node(node_id) => {
					let apply = compiled_track.definition.and_then(|definition| definition.apply);
					if let (Some(apply), Some(node)) = (apply, node_tree.get_node_mut(*node_id)) {
						apply(node, &value);
					}
				}
				TimelineTarget::External(external_target) => {
					external_target.borrow_mut().set_timeline_property(&compiled_track.property_name, &value);
					self.external_property_list.push(TimelineExternalProperty {
						target_name: compiled_track.target_name.clone(),
						property_name: compiled_track.property_name.clone(),
						value,
					});
				}
			}
		}
	}

	//==============================================================================
	/// 마지막 evaluate 가 External 대상에 건 값 반환. (트랙 차례)
	///
	/// # 돌려줌
	/// External 대상에 건 값 조각. (evaluate 전이면 비어 있다)
	//==============================================================================
	pub fn get_external_properties(&self) -> &[TimelineExternalProperty] {
		&self.external_property_list
	}

	//==============================================================================
	/// 키 목록에서 시간의 값 표본. (정적. 키는 시간순이어야 한다. 편집기의 커브 그리기도 쓴다)
	///
	/// # 인자
	/// - `sorted_keys`: 시간순으로 정렬한 키 목록.
	/// - `kind`: 값 종류. (Number, Color 는 앞 키의 이징으로 보간, 그 밖은 앞 키 값)
	/// - `time`: 표본할 시간. (초, 첫 키 앞은 첫 키 값, 마지막 키 뒤는 마지막 키 값)
	///
	/// # 돌려줌
	/// 그 시간의 값. 키가 없으면 Number 는 Number(0), 그 밖은 None.
	//==============================================================================
	pub fn sample_keys(sorted_keys: &[TimelineKey], kind: TimelinePropertyKind, time: f64) -> TimelineValue {
		let key_count = sorted_keys.len();
		if key_count == 0 {
			if kind == TimelinePropertyKind::Number {
				return TimelineValue::Number(0.0);
			}
			return TimelineValue::None;
		}
		if time <= sorted_keys[0].time {
			return sorted_keys[0].value.clone();
		}
		if time >= sorted_keys[key_count - 1].time {
			return sorted_keys[key_count - 1].value.clone();
		}
		let mut segment_index = 0;
		while segment_index + 2 < key_count && sorted_keys[segment_index + 1].time <= time {
			segment_index += 1;
		}
		let from_key = &sorted_keys[segment_index];
		let to_key = &sorted_keys[segment_index + 1];
		let segment_duration = to_key.time - from_key.time;
		if segment_duration <= 0.0 {
			return to_key.value.clone();
		}
		if kind != TimelinePropertyKind::Number && kind != TimelinePropertyKind::Color {
			return from_key.value.clone();
		}
		let easing = from_key.resolve_easing();
		let progress = (time - from_key.time) / segment_duration;
		let eased_progress = easing.evaluate(progress.clamp(0.0, 1.0));
		interpolate_value(kind, &from_key.value, &to_key.value, eased_progress)
	}

	//==============================================================================
	/// 서술 트랙의 시간 값 표본. (정적. 서술 트랙 그대로, 정렬은 여기서. 정의가 없는 속성은 number)
	///
	/// # 인자
	/// - `track`: 서술 트랙 객체. ({ property, keys })
	/// - `time`: 표본할 시간. (초)
	///
	/// # 돌려줌
	/// 그 시간의 값. (sample_keys 와 같다)
	///
	/// # 예제
	/// 색 트랙과 글 트랙 표본
	/// ```rust
	/// use serde_json::json;
	/// use vanilla::{Timeline, TimelineValue};
	///
	/// let color_track = json!({ "property": "color", "keys": [{ "time": 0, "value": "#ff0000ff" }, { "time": 2, "value": "#0000ffff" }] });
	/// assert_eq!(Timeline::sample_track(&color_track, 1.0), TimelineValue::String(String::from("#800080ff")));
	/// let text_track = json!({ "property": "text", "keys": [{ "time": 1, "value": "B" }, { "time": 0, "value": "A" }] });
	/// assert_eq!(Timeline::sample_track(&text_track, 0.5).to_text(), "A");
	/// assert_eq!(Timeline::sample_track(&text_track, 1.0).to_text(), "B");
	/// ```
	//==============================================================================
	pub fn sample_track(track: &Value, time: f64) -> TimelineValue {
		let property_name = track.get("property").and_then(|value| value.as_str()).unwrap_or("");
		let kind = match find_timeline_property_definition(property_name) {
			Some(definition) => {
				definition.kind
			}
			None => {
				TimelinePropertyKind::Number
			}
		};
		let mut sorted_keys = read_track_keys(track);
		sort_keys_by_time(&mut sorted_keys);
		Timeline::sample_keys(&sorted_keys, kind, time)
	}

	//==============================================================================
	/// 시간 반환.
	///
	/// # 돌려줌
	/// 지금 재생 시간. (초, 0 ~ 길이)
	//==============================================================================
	pub fn get_time(&self) -> f64 {
		self.time
	}

	//==============================================================================
	/// 재생 중 여부 반환.
	///
	/// # 돌려줌
	/// play 뒤 pause, stop, 끝 닿기 전이면 true.
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.is_playing
	}

	//==============================================================================
	/// 속도 설정. (1 = 실시간)
	///
	/// # 인자
	/// - `speed`: 시간 배율. (2 는 두 배 빠르기, 음수면 거꾸로 가며 0 에서 더 가지 않고 이벤트는 나지 않는다)
	//==============================================================================
	pub fn set_speed(&mut self, speed: f64) {
		self.speed = speed;
	}

	//==============================================================================
	/// 속도 반환.
	///
	/// # 돌려줌
	/// 시간 배율. (기본 1)
	//==============================================================================
	pub fn get_speed(&self) -> f64 {
		self.speed
	}

	//==============================================================================
	/// 반복 설정. (서술의 loop 를 바꾼다)
	///
	/// # 인자
	/// - `is_loop`: 끝을 넘으면 처음으로 감아 돌지.
	//==============================================================================
	pub fn set_loop(&mut self, is_loop: bool) {
		if let Some(description_object) = self.description.as_object_mut() {
			description_object.insert(String::from("loop"), Value::Bool(is_loop));
		}
	}

	//==============================================================================
	/// 반복 여부 반환. (서술의 loop 가 true 일 때만)
	///
	/// # 돌려줌
	/// 서술의 loop 가 참거짓 true 면 true. (없거나 다른 꼴이면 false)
	//==============================================================================
	pub fn is_loop(&self) -> bool {
		self.description.get("loop") == Some(&Value::Bool(true))
	}

	//==============================================================================
	/// 길이 반환. (초, 못 읽거나 0 이하면 0)
	///
	/// # 돌려줌
	/// 서술의 duration. (초, 글 "4" 처럼 숫자로 읽히는 글도 받는다. 못 읽거나 0 이하면 0)
	//==============================================================================
	pub fn get_duration(&self) -> f64 {
		let duration = read_json_number(self.description.get("duration"));
		if duration.is_finite() && duration > 0.0 {
			duration
		}
		else {
			0.0
		}
	}

	//==============================================================================
	/// 초당 프레임 반환. (못 읽거나 0 이하면 30)
	///
	/// # 돌려줌
	/// 서술의 frameRate. (못 읽거나 0 이하면 30, 재생 시간 계산에는 쓰지 않는다)
	//==============================================================================
	pub fn get_frame_rate(&self) -> f64 {
		let frame_rate = read_json_number(self.description.get("frameRate"));
		if frame_rate.is_finite() && frame_rate > 0.0 {
			frame_rate
		}
		else {
			30.0
		}
	}

	//==============================================================================
	/// 이벤트 핸들러 설정. ((대상 이름, 이벤트 이름, 키, 타임라인, 노드 트리))
	///
	/// # 인자
	/// - `event_handler`: tick 이 지나친 event 트랙 키마다 부를 함수. (이벤트 이름은 키 값의 글, 대상을 못 찾은 트랙도 부른다) None 이면 뗀다.
	///
	/// # 예제
	/// 이벤트를 적고 land 에서 멈추기
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use serde_json::json;
	/// use vanilla::{NodeTree, Timeline, TimelineKey};
	///
	/// let event_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
	/// let mut node_tree = NodeTree::new();
	/// let mut timeline = Timeline::new(Some(json!({
	/// 	"duration": 2,
	/// 	"loop": true,
	/// 	"tracks": [{ "target": "hero", "property": "event", "keys": [{ "time": 0.5, "value": "jump" }, { "time": 1.5, "value": "land" }] }],
	/// })));
	/// let recorded_list = event_list.clone();
	/// timeline.set_event_handler(Some(Box::new(move |target_name: &str, event_name: &str, key: &TimelineKey, timeline: &mut Timeline, _node_tree: &mut NodeTree| {
	/// 	recorded_list.borrow_mut().push(format!("{} {} {}", target_name, event_name, key.time));
	/// 	if event_name == "land" {
	/// 		timeline.pause();
	/// 	}
	/// })));
	/// timeline.play();
	/// timeline.tick(&mut node_tree, 1.0);
	/// timeline.tick(&mut node_tree, 1.0);
	/// assert_eq!(*event_list.borrow(), vec![String::from("hero jump 0.5"), String::from("hero land 1.5")]);
	/// assert!(!timeline.is_playing());
	/// ```
	//==============================================================================
	pub fn set_event_handler(&mut self, event_handler: Option<TimelineEventHandler>) {
		self.event_handler = event_handler;
	}

	//==============================================================================
	/// 마커 핸들러 설정. ((마커 이름, 마커, 타임라인, 노드 트리))
	///
	/// # 인자
	/// - `marker_handler`: tick 이 지나친 마커마다 부를 함수. (그 구간의 이벤트 키 뒤에 시간순으로) None 이면 뗀다.
	//==============================================================================
	pub fn set_marker_handler(&mut self, marker_handler: Option<TimelineMarkerHandler>) {
		self.marker_handler = marker_handler;
	}

	//==============================================================================
	/// 완료 핸들러 설정. (반복이 아닐 때 끝에 닿으면, (타임라인, 노드 트리))
	///
	/// # 인자
	/// - `complete_handler`: 반복이 아닌 재생이 끝에 닿은 tick 에 한 번 부를 함수. (끝 값을 건 뒤) None 이면 뗀다.
	///
	/// # 예제
	/// 1초 타임라인이 끝나면 세기
	/// ```rust
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	/// use serde_json::json;
	/// use vanilla::{NodeTree, Timeline};
	///
	/// let completed_count = Rc::new(Cell::new(0));
	/// let mut node_tree = NodeTree::new();
	/// let mut timeline = Timeline::new(Some(json!({ "duration": 1, "loop": false })));
	/// let counter = completed_count.clone();
	/// timeline.set_complete_handler(Some(Box::new(move |_timeline: &mut Timeline, _node_tree: &mut NodeTree| {
	/// 	counter.set(counter.get() + 1);
	/// })));
	/// timeline.play();
	/// timeline.tick(&mut node_tree, 0.6);
	/// timeline.tick(&mut node_tree, 0.6);
	/// assert_eq!(completed_count.get(), 1);
	/// assert_eq!(timeline.get_time(), 1.0);
	/// ```
	//==============================================================================
	pub fn set_complete_handler(&mut self, complete_handler: Option<TimelineCompleteHandler>) {
		self.complete_handler = complete_handler;
	}
}

impl Default for Timeline {
	//==============================================================================
	/// 기본값. (빈 서술의 타임라인, new(None) 과 같다)
	//==============================================================================
	fn default() -> Timeline {
		Timeline::new(None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;

	//==============================================================================
	/// 두 수가 오차 안에서 같은지.
	//==============================================================================
	fn assert_near(actual: f64, expected: f64, tolerance: f64, label: &str) {
		assert!((actual - expected).abs() <= tolerance, "{}: {} != {}", label, actual, expected);
	}

	//==============================================================================
	/// 이징 곡선이 원본과 같은 값을 낸다. (원본 node 실행 값. exponential, elastic 은 원본 수식을 실제 Math 로 계산한 값)
	//==============================================================================
	#[test]
	fn easing_curves_match_original() {
		let progress_list = [0.0f64, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
		let expected_table: [(&str, [f64; 7]); 33] = [
			("linear", [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0]),
			("step", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
			("bezier", [0.0, 0.019722447263855636, 0.1291619005687877, 0.5, 0.8708380994312122, 0.9802775527361445, 1.0]),
			("quadratic.in", [0.0, 0.010000000000000002, 0.0625, 0.25, 0.5625, 0.81, 1.0]),
			("quadratic.out", [0.0, 0.19, 0.4375, 0.75, 0.9375, 0.9900000000000001, 1.0]),
			("quadratic.inOut", [0.0, 0.020000000000000004, 0.125, 0.5, 0.875, 0.98, 1.0]),
			("cubic.in", [0.0, 0.0010000000000000002, 0.015625, 0.125, 0.421875, 0.7290000000000001, 1.0]),
			("cubic.out", [0.0, 0.2709999999999999, 0.578125, 0.875, 0.984375, 0.999, 1.0]),
			("cubic.inOut", [0.0, 0.004000000000000001, 0.0625, 0.5, 0.9375, 0.996, 1.0]),
			("quartic.in", [0.0, 0.00010000000000000003, 0.00390625, 0.0625, 0.31640625, 0.6561000000000001, 1.0]),
			("quartic.out", [0.0, 0.3438999999999999, 0.68359375, 0.9375, 0.99609375, 0.9999, 1.0]),
			("quartic.inOut", [0.0, 0.0008000000000000003, 0.03125, 0.5, 0.96875, 0.9992, 1.0]),
			("quintic.in", [0.0, 0.000010000000000000004, 0.0009765625, 0.03125, 0.2373046875, 0.5904900000000002, 1.0]),
			("quintic.out", [0.0, 0.4095099999999998, 0.7626953125, 0.96875, 0.9990234375, 0.99999, 1.0]),
			("quintic.inOut", [0.0, 0.00016000000000000007, 0.015625, 0.5, 0.984375, 0.99984, 1.0]),
			("sinusoidal.in", [0.0, 0.01231165940486223, 0.07612046748871326, 0.29289321881345254, 0.6173165676349103, 0.8435655349597695, 1.0000000000000033]),
			("sinusoidal.out", [0.0, 0.15643446504023087, 0.3826834323650897, 0.7071067811865475, 0.923879532511287, 0.9876883405951378, 1.0]),
			("sinusoidal.inOut", [0.0, 0.024471741852423234, 0.14644660940672627, 0.5000000000000017, 0.8535533905989214, 0.9755282583629648, 1.00000000176454]),
			("exponential.in", [0.0, 0.0019531249999999998, 0.005524271728019903, 0.03125, 0.1767766952966369, 0.5000000000000001, 1.0]),
			("exponential.out", [0.0, 0.5, 0.8232233047033631, 0.96875, 0.99447572827198, 0.998046875, 1.0]),
			("exponential.inOut", [0.0, 0.0019531249999999993, 0.015625, 0.5, 0.984375, 0.998046875, 1.0]),
			("circular.in", [0.0, 0.005012562893380035, 0.031754163448145745, 0.1339745962155614, 0.3385621722338523, 0.5641101056459327, 1.0]),
			("circular.out", [0.0, 0.43588989435406733, 0.6614378277661477, 0.8660254037844386, 0.9682458365518543, 0.99498743710662, 1.0]),
			("circular.inOut", [0.0, 0.010102051443364402, 0.0669872981077807, 0.5, 0.9330127018922193, 0.9898979485566356, 1.0]),
			("elastic.in", [0.0, 0.0, 0.0039062499999999952, 0.0, -0.12500000000000003, 0.0, 1.0]),
			("elastic.out", [0.0, 1.0, 1.125, 1.0, 0.99609375, 1.0, 1.0]),
			("elastic.inOut", [0.0, 0.001953125, 0.0, 0.5, 1.0, 0.998046875, 1.0]),
			("back.in", [0.0, -0.014314220000000002, -0.06413656250000001, -0.08769750000000004, 0.1825903124999999, 0.5911720199999999, 0.9999999999999998]),
			("back.out", [0.0, 0.4088279800000001, 0.8174096875000001, 1.0876975, 1.0641365625, 1.01431422, 1.0]),
			("back.inOut", [0.0, -0.037518552000000004, -0.09968184375, 0.5, 1.09968184375, 1.0375185519999999, 1.0]),
			("bounce.in", [0.0, 0.01187500000000008, 0.02734375, 0.234375, 0.52734375, 0.9243750000000001, 1.0]),
			("bounce.out", [0.0, 0.07562500000000001, 0.47265625, 0.765625, 0.97265625, 0.9881249999999999, 1.0]),
			("bounce.inOut", [0.0, 0.030000000000000027, 0.1171875, 0.5, 0.8828125, 0.97, 1.0]),
		];
		for (easing_index, (easing_name, expected_values)) in expected_table.iter().enumerate() {
			assert_eq!(TIMELINE_EASING_NAMES[easing_index], *easing_name);
			let easing = resolve_timeline_easing(easing_name, None);
			for (progress, expected_value) in progress_list.iter().zip(expected_values.iter()) {
				assert_near(easing.evaluate(*progress), *expected_value, 0.0002, &format!("{} at {}", easing_name, progress));
			}
		}
		assert_near(resolve_timeline_easing("unknown.out", None).evaluate(0.3), 0.3, 0.000001, "unknown name is linear");
		assert_near(resolve_timeline_easing("cubic", None).evaluate(0.3), 0.3, 0.000001, "family without direction is linear");
	}

	//==============================================================================
	/// 베지어 이징이 원본과 같은 값을 낸다. (제어점 x 가 0 ~ 1 밖이면 자른다)
	//==============================================================================
	#[test]
	fn bezier_easing_matches_original() {
		let progress_list = [0.0f64, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
		let expected_table: [([f64; 4], [f64; 7]); 4] = [
			([0.25, 0.1, 0.25, 1.0], [0.0, 0.09479499730150565, 0.4085105930156005, 0.8024033876954126, 0.9604590729525888, 0.994316477509521, 1.0]),
			([0.68, -0.55, 0.265, 1.55], [0.0, -0.06629147620026413, -0.0828074555627528, 0.6066814989627444, 1.089166506168796, 1.0623731930901974, 1.0]),
			([0.42, 0.0, 0.58, 1.0], [0.0, 0.019722447263855636, 0.1291619005687877, 0.5, 0.8708380994312122, 0.9802775527361445, 1.0]),
			([1.5, 0.0, -0.5, 1.0], [0.0, 0.0037616480437726607, 0.029723406016020297, 0.5, 0.9702765939839797, 0.9962383519562273, 1.0]),
		];
		for (curve, expected_values) in expected_table.iter() {
			let easing = create_bezier_easing(curve[0], curve[1], curve[2], curve[3]);
			let resolved_easing = resolve_timeline_easing("bezier", Some(curve));
			for (progress, expected_value) in progress_list.iter().zip(expected_values.iter()) {
				assert_near(easing(*progress), *expected_value, 0.0002, &format!("{:?} at {}", curve, progress));
				assert_near(resolved_easing.evaluate(*progress), *expected_value, 0.0002, &format!("resolved {:?} at {}", curve, progress));
			}
		}
	}

	//==============================================================================
	/// 색 글 읽기가 원본 parseInt 와 같게 읽는다. (못 읽는 채널은 1)
	//==============================================================================
	#[test]
	fn parse_timeline_color_matches_original() {
		let expected_table: [(&str, [f64; 4]); 9] = [
			("#ff8000", [1.0, 0.5019607843137255, 0.0, 1.0]),
			("#11223344", [0.06666666666666667, 0.13333333333333333, 0.2, 0.26666666666666666]),
			("abc", [1.0, 1.0, 1.0, 1.0]),
			("#zz", [1.0, 1.0, 1.0, 1.0]),
			("#1", [0.00392156862745098, 1.0, 1.0, 1.0]),
			("", [1.0, 1.0, 1.0, 1.0]),
			("#12345", [0.07058823529411765, 0.20392156862745098, 0.0196078431372549, 1.0]),
			("#ff00ff7f", [1.0, 0.0, 1.0, 0.4980392156862745]),
			("#1g2h3i", [0.00392156862745098, 0.00784313725490196, 0.011764705882352941, 1.0]),
		];
		for (hex_text, expected_channels) in expected_table.iter() {
			let channels = parse_timeline_color(hex_text);
			for (channel_index, (channel, expected_channel)) in channels.iter().zip(expected_channels.iter()).enumerate() {
				assert_near(*channel, *expected_channel, 0.000001, &format!("{} channel {}", hex_text, channel_index));
			}
		}
	}

	//==============================================================================
	/// 채널 → 색 글이 원본과 같다. (0 ~ 1 로 자르고 반올림, 알파가 없으면 ff)
	//==============================================================================
	#[test]
	fn compose_timeline_color_matches_original() {
		assert_eq!(compose_timeline_color(&[1.0, 0.5, 0.0, 1.0]), "#ff8000ff");
		assert_eq!(compose_timeline_color(&[0.2, 0.4, 0.6]), "#336699ff");
		assert_eq!(compose_timeline_color(&[1.2, -0.1, 0.5, 0.5]), "#ff008080");
		assert_eq!(compose_timeline_color(&[0.5, 0.25, 0.75, 0.1]), "#8040bf1a");
	}

	//==============================================================================
	/// 트랙 표본이 원본과 같다. (정렬, 구간 이징, 베지어, 계단, 색 보간, 문자열 / 참거짓 계단, 같은 시간 키)
	//==============================================================================
	#[test]
	fn sample_track_matches_original() {
		let time_list = [-1.0f64, 0.0, 0.3, 0.5, 0.99, 1.0, 1.4, 2.0, 2.5, 5.0];
		let number_tracks: [(Value, [f64; 10]); 3] = [
			(json!({ "property": "x", "keys": [{ "time": 1, "value": 50, "easing": "cubic.out" }, { "time": 0, "value": 0, "easing": "quadratic.in" }, { "time": 2, "value": 100 }] }),
				[0.0, 0.0, 4.5, 12.5, 49.004999999999995, 50.0, 89.19999999999999, 100.0, 100.0, 100.0]),
			(json!({ "property": "y", "keys": [{ "time": 0, "value": 10, "easing": "bezier", "curve": [0.68, -0.55, 0.265, 1.55] }, { "time": 1, "value": 20, "easing": "step" }, { "time": 2, "value": 40 }] }),
				[10.0, 10.0, 9.50682322875037, 16.066814989627446, 20.073713280149843, 20.0, 20.0, 40.0, 40.0, 40.0]),
			// 0.99 의 elastic.out 은 원본 수식을 실제 Math 로 계산한 값. (원본 엔진 pow 는 1.0038581575804497)
			(json!({ "property": "opacity", "keys": [{ "time": 0, "value": 0, "easing": "elastic.out" }, { "time": 1, "value": 1, "easing": "bounce.inOut" }, { "time": 1, "value": 0.5 }, { "time": 3, "value": 0 }] }),
				[0.0, 0.0, 1.0, 1.0, 1.0010337677272523, 0.5, 0.4, 0.25, 0.125, 0.0]),
		];
		for (track, expected_values) in number_tracks.iter() {
			for (time, expected_value) in time_list.iter().zip(expected_values.iter()) {
				let value = Timeline::sample_track(track, *time);
				assert_near(value.as_number(), *expected_value, 0.0005, &format!("{} at {}", track["property"], time));
			}
		}
		let color_track = json!({ "property": "color", "keys": [{ "time": 0, "value": "#ff000080", "easing": "sinusoidal.inOut" }, { "time": 2, "value": "#0000ffff" }] });
		// 1 초는 두 엔진 sin, cos 가 같은 비트(크롬 Math.cos)라 빨강이 80 이다. 감속 비율을 두 엔진 다 f64(0.49999999999999994)로 셈해 파랑이 7f 다. (원본 #80007fc0)
		let expected_colors = ["#ff000080", "#ff000080", "#f1000e87", "#da002593", "#82007dbf", "#80007fc0", "#3500cae5", "#0000ffff", "#0000ffff", "#0000ffff"];
		for (time, expected_color) in time_list.iter().zip(expected_colors.iter()) {
			assert_eq!(Timeline::sample_track(&color_track, *time), TimelineValue::String(expected_color.to_string()), "color at {}", time);
		}
		let text_track = json!({ "property": "text", "keys": [{ "time": 0, "value": "A" }, { "time": 1, "value": "B" }] });
		let expected_texts = ["A", "A", "A", "A", "A", "B", "B", "B", "B", "B"];
		for (time, expected_text) in time_list.iter().zip(expected_texts.iter()) {
			assert_eq!(Timeline::sample_track(&text_track, *time).to_text(), *expected_text, "text at {}", time);
		}
		let visible_track = json!({ "property": "visible", "keys": [{ "time": 0, "value": false }, { "time": 0.5, "value": true }] });
		let expected_visibles = [false, false, false, true, true, true, true, true, true, true];
		for (time, expected_visible) in time_list.iter().zip(expected_visibles.iter()) {
			assert_eq!(Timeline::sample_track(&visible_track, *time), TimelineValue::Boolean(*expected_visible), "visible at {}", time);
		}
		assert_eq!(Timeline::sample_keys(&[], TimelinePropertyKind::Number, 1.0), TimelineValue::Number(0.0));
		assert_eq!(Timeline::sample_keys(&[], TimelinePropertyKind::String, 1.0), TimelineValue::None);
	}

	//==============================================================================
	/// 값을 받아 적는 external 대상. (시험용)
	//==============================================================================
	struct RecordingTarget {
		log: Rc<RefCell<Vec<String>>>,
	}

	impl TimelinePropertyTarget for RecordingTarget {
		//==============================================================================
		/// 받은 값을 적는다.
		//==============================================================================
		fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue) {
			self.log.borrow_mut().push(format!("set {}={:.4}", property_name, value.as_number()));
		}
	}

	//==============================================================================
	/// 재생이 원본과 같은 차례로 값 / 이벤트 / 마커 / 완료를 낸다. (원본 node 실행 기록과 비교)
	//==============================================================================
	#[test]
	fn playback_matches_original_event_order() {
		let description = json!({
			"duration": 2, "frameRate": 30, "loop": true,
			"tracks": [
				{ "target": "screen", "property": "vignette", "keys": [{ "time": 0, "value": 0 }, { "time": 2, "value": 1 }] },
				{ "target": "screen", "property": "event", "keys": [{ "time": 0.5, "value": "hit" }, { "time": 1.5, "value": "end" }] },
				{ "target": "screen", "property": "event", "enabled": false, "keys": [{ "time": 1, "value": "disabled" }] },
			],
			"markers": [{ "time": 1.0, "name": "middle" }, { "time": 0, "name": "start" }],
		});
		let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let mut node_tree = NodeTree::new();
		let mut timeline = Timeline::new(Some(description));
		let screen: Rc<RefCell<dyn TimelinePropertyTarget>> = Rc::new(RefCell::new(RecordingTarget { log: log.clone() }));
		let resolved_screen = screen.clone();
		timeline.set_target_resolver(Some(Box::new(move |target_name: &str| {
			if target_name == "screen" {
				Some(TimelineTarget::External(resolved_screen.clone()))
			}
			else {
				None
			}
		})));
		let event_log = log.clone();
		timeline.set_event_handler(Some(Box::new(move |target_name: &str, event_name: &str, _key: &TimelineKey, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			event_log.borrow_mut().push(format!("event {}:{}", target_name, event_name));
		})));
		let marker_log = log.clone();
		timeline.set_marker_handler(Some(Box::new(move |marker_name: &str, _marker: &TimelineMarker, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			marker_log.borrow_mut().push(format!("marker {}", marker_name));
		})));
		let complete_log = log.clone();
		timeline.set_complete_handler(Some(Box::new(move |_timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			complete_log.borrow_mut().push(String::from("complete"));
		})));

		timeline.play();
		for time_delta in [0.25f64, 0.25, 0.6, 0.6, 0.6, 0.6] {
			timeline.tick(&mut node_tree, time_delta);
			log.borrow_mut().push(format!("time {:.4}", timeline.get_time()));
		}
		timeline.set_loop(false);
		timeline.tick(&mut node_tree, 0.7);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		timeline.tick(&mut node_tree, 0.3);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		timeline.play();
		log.borrow_mut().push(format!("replay time {:.4}", timeline.get_time()));
		timeline.seek(&mut node_tree, 1.25);
		log.borrow_mut().push(format!("seek time {:.4}", timeline.get_time()));
		timeline.tick(&mut node_tree, 1.0);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		timeline.play();
		log.borrow_mut().push(format!("replay time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		timeline.tick(&mut node_tree, 0.1);
		log.borrow_mut().push(format!("time {:.4}", timeline.get_time()));
		assert_eq!(timeline.get_external_properties().len(), 1);
		assert_eq!(timeline.get_target("screen"), None);
		assert_eq!(timeline.resolve_target("screen"), Some(TimelineTarget::External(screen.clone())));

		// 원본 기록. (숫자는 소수 넷째 자리로 맞췄다)
		let expected_log = [
			"set vignette=0.1250", "time 0.2500",
			"event screen:hit", "set vignette=0.2500", "time 0.5000",
			"marker middle", "set vignette=0.5500", "time 1.1000",
			"event screen:end", "set vignette=0.8500", "time 1.7000",
			"marker start", "set vignette=0.1500", "time 0.3000",
			"event screen:hit", "set vignette=0.4500", "time 0.9000",
			"event screen:end", "marker middle", "set vignette=0.8000", "time 1.6000 playing true",
			"set vignette=0.9500", "time 1.9000 playing true",
			"replay time 1.9000",
			"set vignette=0.6250", "seek time 1.2500",
			"event screen:end", "set vignette=1.0000", "complete", "time 2.0000 playing false",
			"replay time 0.0000 playing true",
			"set vignette=0.0500", "time 0.1000",
		];
		assert_eq!(*log.borrow(), expected_log.iter().map(|line| line.to_string()).collect::<Vec<String>>());
	}

	//==============================================================================
	/// 핸들러 안에서 타임라인을 멈추고 옮길 수 있다. (원본 핸들러의 마지막 인자, 타임라인 자신)
	//==============================================================================
	#[test]
	fn handlers_can_control_timeline() {
		let description = json!({
			"duration": 2, "loop": true,
			"tracks": [{ "target": "screen", "property": "event", "keys": [{ "time": 0.5, "value": "stop" }] }],
			"markers": [{ "time": 1.0, "name": "jump" }],
		});
		let mut node_tree = NodeTree::new();
		let mut timeline = Timeline::new(Some(description));
		timeline.set_event_handler(Some(Box::new(|_target_name: &str, event_name: &str, _key: &TimelineKey, timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			if event_name == "stop" {
				timeline.pause();
			}
		})));
		timeline.set_marker_handler(Some(Box::new(|_marker_name: &str, _marker: &TimelineMarker, timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			timeline.set_speed(2.0);
		})));
		timeline.play();
		timeline.tick(&mut node_tree, 0.6);
		assert!(!timeline.is_playing());
		timeline.play();
		timeline.tick(&mut node_tree, 0.6);
		assert_eq!(timeline.get_speed(), 2.0);
	}

	//==============================================================================
	/// 같은 이름 무대 노드는 모두 붙고 이름 표에는 뒤의 것이 남는다. 자기 자신을 부모로 두면 무대 부모에 붙는다.
	//==============================================================================
	#[test]
	fn build_stage_keeps_same_name_nodes_and_rejects_self_parent() {
		let description = json!({
			"duration": 1,
			"stage": { "nodes": [
				{ "name": "a", "type": "node" },
				{ "name": "a", "type": "node" },
				{ "name": "self", "type": "node", "parent": "self" },
				{ "name": "child", "type": "node", "parent": "a" },
			] },
			"tracks": [],
		});
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let mut timeline = Timeline::new(Some(description));
		let node_table = timeline.build_stage(&mut node_tree, root_id, None);
		assert_eq!(node_tree.get_child_count(root_id), 3);
		assert_eq!(node_tree.get_child_count(node_table["a"]), 1);
		assert_eq!(node_tree.get_parent_id(node_table["self"]), Some(root_id));
	}

	//==============================================================================
	/// 무대를 만들고 트랙 값을 노드에 건다. (부모 관계, Paint / Text 색, 글, 보이기, 위치)
	//==============================================================================
	#[test]
	fn build_stage_creates_nodes_and_applies_tracks() {
		let description = json!({
			"duration": 2, "loop": false,
			"stage": { "nodes": [
				{ "name": "Group", "type": "group", "x": 100, "y": 50 },
				{ "name": "Box", "type": "paint", "parent": "Group", "width": 40, "height": 20, "color": "#ff0000ff", "roundSize": 4 },
				{ "name": "Label", "type": "text", "text": "Hello", "fontSize": 24, "bold": true, "color": "#00ff00ff", "textAlign": "left", "visible": false },
				{ "name": "Score", "type": "text" },
			] },
			"tracks": [
				{ "target": "Box", "property": "x", "keys": [{ "time": 0, "value": 0 }, { "time": 2, "value": 200 }] },
				{ "target": "Box", "property": "color", "keys": [{ "time": 0, "value": "#ff0000ff" }, { "time": 2, "value": "#0000ffff" }] },
				{ "target": "Label", "property": "visible", "keys": [{ "time": 0, "value": false }, { "time": 1, "value": true }] },
				{ "target": "Label", "property": "visibleCharacters", "keys": [{ "time": 0, "value": 0 }, { "time": 2, "value": 5 }] },
				{ "target": "Score", "property": "number", "keys": [{ "time": 0, "value": 0 }, { "time": 2, "value": 1000 }] },
				{ "target": "Missing", "property": "x", "keys": [{ "time": 0, "value": 1 }] },
			],
		});
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let mut timeline = Timeline::new(Some(description));
		let node_table = timeline.build_stage(&mut node_tree, root_id, None);
		let group_id = node_table["Group"];
		let box_id = node_table["Box"];
		let label_id = node_table["Label"];
		let score_id = node_table["Score"];
		assert_eq!(node_tree.get_parent_id(box_id), Some(group_id));
		assert_eq!(node_tree.get_parent_id(label_id), Some(root_id));
		assert_eq!(node_tree[group_id].get_local_position(), Vector2::create(100.0, 50.0));
		assert_eq!(node_tree[box_id].get_content_size(), Vector2::create(40.0, 20.0));
		assert_eq!(node_tree[box_id].get_pivot(), Vector2::create(0.5, 0.5));
		assert_eq!(node_tree[box_id].get_component::<Paint>().map(|paint| paint.get_round_size()), Some(4.0));
		assert!(!node_tree[label_id].is_active());
		let label = node_tree[label_id].get_component::<Text>().expect("label text");
		assert_eq!(label.get_text(), "Hello");
		assert!(label.is_bold());
		assert_eq!(label.get_text_align(), TextAlign::Left);
		assert_eq!(node_tree[score_id].get_component::<Text>().map(|text| text.get_text().to_string()), Some(String::from("Text")));

		timeline.seek(&mut node_tree, 1.0);
		assert_near(node_tree[box_id].get_local_position().x, 100.0, 0.0001, "box x at 1");
		let box_color = node_tree[box_id].get_component::<Paint>().expect("box paint").get_color();
		assert_near(box_color.red, 128.0 / 255.0, 0.0001, "box red at 1");
		assert_near(box_color.blue, 128.0 / 255.0, 0.0001, "box blue at 1");
		assert!(node_tree[label_id].is_active());
		assert_eq!(node_tree[label_id].get_component::<Text>().map(|text| text.get_visible_character_count()), Some(3));
		assert_eq!(node_tree[score_id].get_component::<Text>().map(|text| text.get_text().to_string()), Some(String::from("500")));

		timeline.play();
		timeline.tick(&mut node_tree, 5.0);
		assert!(!timeline.is_playing());
		assert_near(node_tree[box_id].get_local_position().x, 200.0, 0.0001, "box x at end");
		assert_eq!(node_tree[score_id].get_component::<Text>().map(|text| text.get_text().to_string()), Some(String::from("1000")));
	}

	//==============================================================================
	/// bind 는 이미 있는 노드 트리에서 트랙 대상 이름을 찾아 붙인다.
	//==============================================================================
	#[test]
	fn bind_finds_targets_by_name() {
		let description = json!({
			"duration": 1,
			"tracks": [
				{ "target": "Hero", "property": "scaleX", "keys": [{ "time": 0, "value": 1 }, { "time": 1, "value": 3 }] },
				{ "target": "Hero", "property": "rotation", "keys": [{ "time": 0, "value": 0 }, { "time": 1, "value": 2 }] },
			],
		});
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let world_id = node_tree.create_child(root_id, "World");
		let hero_id = node_tree.create_child(world_id, "Hero");
		let mut timeline = Timeline::new(Some(description));
		timeline.bind(&node_tree, root_id);
		assert_eq!(timeline.get_target("Hero"), Some(TimelineTarget::Node(hero_id)));
		timeline.seek(&mut node_tree, 0.5);
		assert_near(node_tree[hero_id].get_local_scale().x, 2.0, 0.0001, "hero scale");
		assert_near(node_tree[hero_id].get_local_rotation(), 1.0, 0.0001, "hero rotation");
	}

	//==============================================================================
	/// 파티클은 타임라인 시간으로만 진행하고, 이벤트 play 로 버스트가 나며, 되감아도 같은 결과가 나온다.
	//==============================================================================
	#[test]
	fn particle_events_follow_timeline_time() {
		let description = json!({
			"duration": 2, "loop": false,
			"stage": { "nodes": [
				{ "name": "Burst", "type": "particle", "width": 20, "height": 20, "particle": {
					"looping": false, "duration": 1.2, "emissionRate": 0, "bursts": [{ "time": 0, "count": 25 }],
					"shape": "circle", "shapeRadius": 6, "lifetime": [0.8, 1.0], "speed": [100, 200], "blendMode": "lighter",
				} },
			] },
			"tracks": [
				{ "target": "Burst", "property": "event", "keys": [{ "time": 0.5, "value": "play" }] },
			],
		});
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let mut timeline = Timeline::new(Some(description));
		let node_table = timeline.build_stage(&mut node_tree, root_id, None);
		let burst_id = node_table["Burst"];
		let particle_count = |node_tree: &NodeTree| node_tree[burst_id].get_component::<ParticleSystem>().map(|particle_system| particle_system.get_particle_count()).unwrap_or(0);

		timeline.seek(&mut node_tree, 0.0);
		assert!(node_tree[burst_id].get_component::<ParticleSystem>().is_some_and(|particle_system| particle_system.is_manual_tick()));
		assert_eq!(particle_count(&node_tree), 0);
		timeline.play();
		timeline.tick(&mut node_tree, 0.4);
		assert_eq!(particle_count(&node_tree), 0);
		timeline.tick(&mut node_tree, 0.2);
		assert_eq!(particle_count(&node_tree), 25);

		timeline.seek(&mut node_tree, 0.2);
		assert_eq!(particle_count(&node_tree), 0);
		timeline.seek(&mut node_tree, 0.6);
		assert_eq!(particle_count(&node_tree), 25);
		timeline.seek(&mut node_tree, 1.9);
		assert_eq!(particle_count(&node_tree), 0);
	}

	//==============================================================================
	/// frame 속성은 스프라이트 시트 격자에서 칸을 고른다. (넘치면 감는다)
	//==============================================================================
	#[test]
	fn frame_track_selects_sprite_sheet_cell() {
		let sheet_image = Image::create_from_pixels(64, 32, vec![255u8; 64 * 32 * 4]);
		let description = json!({
			"duration": 1,
			"stage": { "nodes": [
				{ "name": "Coin", "type": "sprite", "image": "coin.png", "frameColumns": 4, "frameRows": 2, "frame": 1, "effect": "glow", "blendMode": "lighter" },
			] },
			"tracks": [
				{ "target": "Coin", "property": "frame", "keys": [{ "time": 0, "value": 0 }, { "time": 1, "value": 10 }] },
			],
		});
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let mut timeline = Timeline::new(Some(description));
		let image_resolver = |image_name: &str| -> Option<Rc<Image>> {
			if image_name == "coin.png" {
				Some(sheet_image.clone())
			}
			else {
				None
			}
		};
		let node_table = timeline.build_stage(&mut node_tree, root_id, Some(&image_resolver));
		let coin_id = node_table["Coin"];
		let image_rect_of = |node_tree: &NodeTree| node_tree[coin_id].get_component::<ShaderSprite>().expect("coin sprite").get_sprite().get_image_rect();
		assert_eq!(image_rect_of(&node_tree), Rect::create(16.0, 0.0, 16.0, 16.0));
		let coin_sprite = node_tree[coin_id].get_component::<ShaderSprite>().expect("coin sprite");
		assert_eq!(coin_sprite.get_effect(), ShaderSpriteEffect::from_name("glow").expect("glow effect"));
		assert_eq!(coin_sprite.get_sprite().get_sprite_blend_mode(), BlendMode::Lighter);
		assert_near(coin_sprite.get_sprite().get_color().alpha, 0.0, 0.000001, "default sprite tint is transparent");

		timeline.seek(&mut node_tree, 0.5);
		assert_eq!(image_rect_of(&node_tree), Rect::create(16.0, 16.0, 16.0, 16.0));
		timeline.seek(&mut node_tree, 1.0);
		assert_eq!(image_rect_of(&node_tree), Rect::create(32.0, 0.0, 16.0, 16.0));
	}

	//==============================================================================
	/// 서술을 채우고 길이 / 프레임 / 반복을 원본처럼 읽는다. (data 주소 base64 풀기 포함)
	//==============================================================================
	#[test]
	fn description_defaults_and_data_url() {
		let timeline = Timeline::new(Some(json!({ "duration": "4", "frameRate": -1 })));
		assert_eq!(timeline.get_duration(), 4.0);
		assert_eq!(timeline.get_frame_rate(), 30.0);
		assert!(!timeline.is_loop());
		assert!(timeline.get_description().pointer("/stage/nodes").is_some_and(|nodes| nodes.is_array()));
		assert!(timeline.get_description()["tracks"].is_array());
		let empty_timeline = Timeline::default();
		assert_eq!(empty_timeline.get_duration(), 3.0);
		assert!(empty_timeline.is_loop());
		assert_eq!(TIMELINE_NODE_DEFAULTS.width, 100.0);
		assert_eq!(TIMELINE_NODE_DEFAULTS.node_type, "group");
	}

	//==============================================================================
	/// "data:" 주소: 퍼센트 꼴을 먼저 푼다. (base64 본문의 %3D, base64 가 아닌 본문의 %XX 바이트)
	//==============================================================================
	#[test]
	fn data_url_image_decodes_percent_encoding() {
		let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
		let image = load_data_url_image(&format!("data:image/png;base64,{}", png_base64)).expect("base64");
		assert_eq!((image.get_width(), image.get_height()), (1, 1));
		assert!(load_data_url_image(&format!("data:image/png;BASE64,{}", png_base64.replace('=', "%3D"))).is_some());
		let png_bytes = crate::base::dataurl::decode_base64(png_base64).expect("png bytes");
		let percent_body: String = png_bytes.iter().map(|byte| format!("%{:02X}", byte)).collect();
		assert!(load_data_url_image(&format!("data:image/png,{}", percent_body)).is_some());
		assert!(load_data_url_image("data:image/png,not-an-image").is_none());
	}

	//==============================================================================
	/// "data:" 주소의 SVG: 브라우저처럼 형이 image/svg+xml 일 때만 SVG 로 그린다. (퍼센트 꼴과 base64, 다른 형이면 못 푼다) 스프라이트 노드에도 걸린다.
	//==============================================================================
	#[test]
	fn data_url_svg_image_follows_mime_type() {
		let svg_text = "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='2'><rect width='2' height='2' fill='#ff0000'/></svg>";
		let percent_svg: String = svg_text.bytes().map(|byte| format!("%{:02X}", byte)).collect();
		let image = load_data_url_image(&format!("data:image/svg+xml;charset=utf-8,{}", percent_svg)).expect("svg");
		assert_eq!((image.get_width(), image.get_height()), (4, 2));
		assert_eq!(&image.get_pixels()[0..4], &[255, 0, 0, 255]);
		let base64_svg = "PHN2ZyB4bWxucz0naHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmcnIHdpZHRoPSc0JyBoZWlnaHQ9JzInPjxyZWN0IHdpZHRoPScyJyBoZWlnaHQ9JzInIGZpbGw9JyNmZjAwMDAnLz48L3N2Zz4=";
		assert_eq!(crate::base::dataurl::decode_base64(base64_svg).expect("svg bytes"), svg_text.as_bytes().to_vec());
		assert!(load_data_url_image(&format!("data:IMAGE/SVG+XML;base64,{}", base64_svg)).is_some());
		assert!(load_data_url_image(&format!("data:text/plain,{}", percent_svg)).is_none());
		assert!(load_data_url_image(&format!("data:,{}", percent_svg)).is_none());
		assert!(load_data_url_image(&format!("data:image/png;base64,{}", base64_svg)).is_none());
		let mut node_tree = NodeTree::new();
		let node_id = Timeline::create_stage_node(&mut node_tree, &json!({ "name": "logo", "type": "sprite", "image": format!("data:image/svg+xml;base64,{}", base64_svg) }), None);
		let sprite_image = node_tree[node_id].get_component::<ShaderSprite>().and_then(|shader_sprite| shader_sprite.get_sprite().get_image().cloned()).expect("sprite image");
		assert_eq!((sprite_image.get_width(), sprite_image.get_height()), (4, 2));
		assert!(!node_tree[node_id].get_component::<TimelineSpriteFrameGrid>().expect("grid").is_tick_needed());
	}

	//==============================================================================
	/// 글 속성은 원본처럼 RichText 노드에도 건다. (원본 getComponent(Text) 가 RichText 도 찾는다)
	//==============================================================================
	#[test]
	fn text_properties_apply_to_rich_text() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let rich_id = node_tree.create_child(root_id, "rich");
		node_tree.add_component(rich_id, RichText::new());
		let description = json!({
			"duration": 1,
			"tracks": [
				{ "target": "rich", "property": "text", "keys": [{ "time": 0, "value": "<b>hi</b>" }] },
				{ "target": "rich", "property": "fontSize", "keys": [{ "time": 0, "value": 30.4 }] },
				{ "target": "rich", "property": "color", "keys": [{ "time": 0, "value": "#ff0000ff" }] },
				{ "target": "rich", "property": "visibleCharacters", "keys": [{ "time": 0, "value": 1 }] },
			],
		});
		let mut timeline = Timeline::new(Some(description));
		timeline.bind(&node_tree, root_id);
		timeline.evaluate(&mut node_tree, 0.0);
		let rich_text = node_tree[rich_id].get_component::<RichText>().expect("rich text");
		assert_eq!(rich_text.get_text(), "<b>hi</b>");
		assert_eq!(rich_text.get_font_size(), 30.0);
		assert_eq!(rich_text.get_text_color(), Color::create(1.0, 0.0, 0.0, 1.0));
		assert_eq!(rich_text.get_visible_character_count(), 1);
	}

	//==============================================================================
	/// 핸들러 꼴 별칭은 공개라 크레이트 밖에서도 이름으로 지어 건다. (lib.rs 에서도 내보낸다)
	//==============================================================================
	#[test]
	fn handler_aliases_are_public() {
		let complete_handler: crate::TimelineCompleteHandler = Box::new(|timeline: &mut Timeline, _node_tree: &mut NodeTree| {
			timeline.set_speed(3.0);
		});
		let event_handler: crate::TimelineEventHandler = Box::new(|_target_name: &str, _event_name: &str, _key: &TimelineKey, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {});
		let marker_handler: crate::TimelineMarkerHandler = Box::new(|_marker_name: &str, _marker: &TimelineMarker, _timeline: &mut Timeline, _node_tree: &mut NodeTree| {});
		let target_resolver: crate::TimelineTargetResolver = Box::new(|_target_name: &str| None);
		let mut node_tree = NodeTree::new();
		let mut timeline = Timeline::new(Some(json!({ "duration": 0.5, "loop": false })));
		timeline.set_complete_handler(Some(complete_handler));
		timeline.set_event_handler(Some(event_handler));
		timeline.set_marker_handler(Some(marker_handler));
		timeline.set_target_resolver(Some(target_resolver));
		timeline.play();
		timeline.tick(&mut node_tree, 1.0);
		assert_eq!(timeline.get_speed(), 3.0);
	}
}
