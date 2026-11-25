//==============================================================================
// 타임라인. (키프레임 애니메이션 문서 재생기)
// - 원본 experimental/animation/timeline.js 를 옮긴 것이다. 문서(서술)는 무대 노드 목록 + 트랙(대상 이름 / 속성 / 키) + 마커로 이루어진 JSON 이며
//   타임라인 편집기가 저장한다. 키의 easing 은 그 키에서 다음 키까지 구간에 걸린다.
// - 이징 곡선은 core/tween.rs 의 easing_function 을 그대로 쓴다. "bezier" 는 CSS cubic-bezier 와 같은 제어점이다.
// - 사용:
//     // 씬 create 에서 문서를 요청하고 맡긴다.
//     self.timeline_asset.load(engine, "assets/intro.timeline.json");
//     self.scene_base.track_asset(&self.timeline_asset);
//     // initialize 에서 무대를 만들고 튼다.
//     let mut timeline = Timeline::create_from_json_asset(&mut self.timeline_asset).unwrap_or_default();
//     let stage_id = self.scene_base.get_root_id();
//     timeline.build_stage(self.scene_base.get_node_tree_mut(), stage_id, Some(&|image_name: &str| image_table.get(image_name).cloned()));
//     timeline.play();
//     // 매 프레임 (tick):
//     timeline.tick(self.scene_base.get_node_tree_mut(), time_delta);
//     timeline.apply_external_properties("screen", &mut self.screen_effect);
// - 원본과 다른 점:
//   - 대상은 객체 참조 대신 TimelineTarget 이다. 노드는 NodeId(node), 노드가 아닌 것(화면 효과 같은)은 external 이다.
//     노드에 거는 함수(tick, seek, stop, evaluate, compile, build_stage, bind)는 NodeTree 를 같이 받는다.
//   - external 대상의 값은 원본처럼 그 자리에서 set_timeline_property 를 부르지 못해서(대상을 빌려 들 수 없다) 모아 두었다가,
//     씬이 apply_external_properties(대상 이름, 대상) 로 넘긴다. 모은 값은 evaluate 를 할 때마다 새로 채운다.
//     넘겨받는 쪽은 TimelinePropertyTarget 트레이트를 구현한다. ScreenEffect 는 이 파일에서 구현했다.
//   - 값은 원본의 숫자 / 문자열 / 참거짓을 TimelineValue 열거형으로 든다. 숫자는 f32 다.
//   - 핸들러는 원본 마지막 인자(타임라인 자신) 대신 NodeTree 를 받는다. (타임라인은 도는 중이라 빌려 줄 수 없다)
//     씬 상태를 바꾸려면 Rc<RefCell<...>> 를 잡아 두거나 NodeTree 로 노드를 바꾼다.
//   - resolve_timeline_easing 은 함수 대신 TimelineEasing 값을 돌려준다. (evaluate(progress) 로 부른다)
//   - TIMELINE_PROPERTY_DEFINITIONS 는 이름 → 정의 객체 대신 정의 배열이고 find_timeline_property_definition 으로 찾는다.
//     TIMELINE_NODE_DEFAULTS 는 기본값을 채운 TimelineNodeDescription 이다. (원본 키 type 은 node_type)
//   - 원본의 스프라이트 격자 표와 사운드 표(WeakMap)는 노드에 붙는 작은 컴포넌트 TimelineSpriteFrameGrid, TimelineSound 로 바꿨다.
//   - 원본 Timeline.setAudioManager(정적)와 사운드 노드의 비동기 읽기는 load_sounds(node_tree, engine) 로 바꿨다.
//     무대를 만든 뒤 한 번 부르면 사운드 노드마다 오디오 애셋을 읽고 플레이어를 만든다. (돌려준 애셋을 씬이 track_audio_asset 으로 맡겨도 된다)
//   - loadFromUrl(비동기 fetch) 대신 JsonAsset 으로 받은 문서를 create_from_json_asset / load_from_text 로 넘긴다.
//   - "data:" 주소 이미지는 원본처럼 나중에 읽히지 않고 그 자리에서 base64 를 풀어 만든다.
// - 이징 곡선 값은 원본과 같다. 같은 이름의 무대 노드가 둘이면 모두 붙이고 이름 표에는 뒤의 것이 남는다(원본과 같다).
//   부모가 자기 자신이거나 고리가 되면 무대 부모에 붙인다. load_from_text, create_from_json_asset 은 원본에도 있다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::audioplayer::AudioPlayer;
use crate::core::component::paint::Paint;
use crate::core::component::sprite::Sprite;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::engine::Engine;
use crate::core::graphic::{BlendMode, TextAlign, TextBaseline};
use crate::core::image::Image;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::WorldNode;
use crate::core::tween::easing_function;
use crate::effect::particlesystem::ParticleSystem;
use crate::effect::screeneffect::ScreenEffect;
use crate::effect::shadersprite::{ShaderSprite, ShaderSpriteEffect};
use crate::resource::audioasset::AudioAsset;
use crate::resource::textasset::JsonAsset;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

//==============================================================================
// 이징 이름 목록. ("linear" / "step" / "bezier" / "<종류>.<in|out|in_out>")
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

// 이징 함수. (진행률 → 이징 값)
type EasingFunction = fn(f32) -> f32;

// 이름 → Tween 이징 함수. (원본 Tween.easing_function[종류][방향])
const EASING_FUNCTION_TABLE: [(&str, EasingFunction); 30] = [
	("quadratic.in", easing_function::quadratic::r#in),
	("quadratic.out", easing_function::quadratic::out),
	("quadratic.inOut", easing_function::quadratic::in_out),
	("cubic.in", easing_function::cubic::r#in),
	("cubic.out", easing_function::cubic::out),
	("cubic.inOut", easing_function::cubic::in_out),
	("quartic.in", easing_function::quartic::r#in),
	("quartic.out", easing_function::quartic::out),
	("quartic.inOut", easing_function::quartic::in_out),
	("quintic.in", easing_function::quintic::r#in),
	("quintic.out", easing_function::quintic::out),
	("quintic.inOut", easing_function::quintic::in_out),
	("sinusoidal.in", easing_function::sinusoidal::r#in),
	("sinusoidal.out", easing_function::sinusoidal::out),
	("sinusoidal.inOut", easing_function::sinusoidal::in_out),
	("exponential.in", easing_function::exponential::r#in),
	("exponential.out", easing_function::exponential::out),
	("exponential.inOut", easing_function::exponential::in_out),
	("circular.in", easing_function::circular::r#in),
	("circular.out", easing_function::circular::out),
	("circular.inOut", easing_function::circular::in_out),
	("elastic.in", easing_function::elastic::r#in),
	("elastic.out", easing_function::elastic::out),
	("elastic.inOut", easing_function::elastic::in_out),
	("back.in", easing_function::back::r#in),
	("back.out", easing_function::back::out),
	("back.inOut", easing_function::back::in_out),
	("bounce.in", easing_function::bounce::r#in),
	("bounce.out", easing_function::bounce::out),
	("bounce.inOut", easing_function::bounce::in_out),
];

// "bezier" 키에 곡선이 없을 때 쓰는 제어점. (CSS ease-in-out)
const DEFAULT_BEZIER_CURVE: [f32; 4] = [0.42, 0.0, 0.58, 1.0];

// 파티클 씨앗. (수집 차례로 서로 다른 씨앗을 준다)
const PARTICLE_RANDOM_SEED_BASE: u32 = 0x9E3779B1;
const PARTICLE_RANDOM_SEED_STEP: u32 = 7919;

// emit 이벤트에 개수가 없을 때 내는 수.
const DEFAULT_EMIT_COUNT: f32 = 30.0;

//==============================================================================
// 3차 베지어 이징 값. (CSS cubic-bezier 와 같은 제어점, x 는 0 ~ 1 로 자른다. 뉴턴법 8 번)
//==============================================================================
fn evaluate_bezier_easing(x1: f32, y1: f32, x2: f32, y2: f32, progress: f32) -> f32 {
	if progress <= 0.0 {
		return 0.0;
	}
	if progress >= 1.0 {
		return 1.0;
	}
	let clamped_x1 = x1.clamp(0.0, 1.0);
	let clamped_x2 = x2.clamp(0.0, 1.0);
	let sample_curve = |t: f32, point1: f32, point2: f32| {
		let one_minus = 1.0 - t;
		3.0 * one_minus * one_minus * t * point1 + 3.0 * one_minus * t * t * point2 + t * t * t
	};
	let sample_derivative = |t: f32, point1: f32, point2: f32| {
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
// 3차 베지어 이징 생성. (CSS cubic-bezier 와 같은 제어점, x 는 0 ~ 1)
//==============================================================================
pub fn create_bezier_easing(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 + Clone {
	move |progress: f32| evaluate_bezier_easing(x1, y1, x2, y2, progress)
}

//==============================================================================
// 계단 이징. (구간 내내 0, 다음 키에서 바뀐다)
//==============================================================================
fn step_easing(_progress: f32) -> f32 {
	0.0
}

//==============================================================================
// 타임라인 이징. (원본에서 resolve_timeline_easing 이 돌려주는 함수 자리)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub enum TimelineEasing {
	// Tween 이징 함수, linear, step.
	Function(EasingFunction),
	// 3차 베지어 (x1, y1, x2, y2).
	Bezier(f32, f32, f32, f32),
}

impl TimelineEasing {
	//==============================================================================
	// 진행률(0 ~ 1)의 이징 값.
	//==============================================================================
	pub fn evaluate(&self, progress: f32) -> f32 {
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
// 이징 반환. (모르는 이름은 linear, "bezier" 는 curve [x1, y1, x2, y2] 를 쓰고 없으면 ease-in-out)
//==============================================================================
pub fn resolve_timeline_easing(easing_name: &str, curve: Option<&[f32]>) -> TimelineEasing {
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
// 원본 parseInt(글, 16) 흉내. (앞 공백, 부호, 0x 를 건너뛰고 16 진 숫자가 이어지는 데까지. 숫자가 없으면 None = NaN)
//==============================================================================
fn parse_leading_hexadecimal(text: &str) -> Option<f32> {
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
	let mut value: f32 = 0.0;
	let mut digit_count = 0;
	for character in digit_text.chars() {
		match character.to_digit(16) {
			Some(digit) => {
				value = value * 16.0 + digit as f32;
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
// 색 문자열 → 채널. ("#rrggbb" 또는 "#rrggbbaa", 못 읽는 채널은 1)
//==============================================================================
pub fn parse_timeline_color(hex_text: &str) -> [f32; 4] {
	let characters: Vec<char> = match hex_text.strip_prefix('#') {
		Some(rest) => {
			rest.chars().collect()
		}
		None => {
			"ffffff".chars().collect()
		}
	};
	let read_channel = |start_index: usize| -> Option<f32> {
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
// 채널 → 색 문자열. ("#rrggbbaa", 알파가 없으면 1)
//==============================================================================
pub fn compose_timeline_color(channels: &[f32]) -> String {
	let to_byte = |value: f32| -> u32 {
		let clamped_value = value.clamp(0.0, 1.0);
		(clamped_value * 255.0 + 0.5).floor() as u32
	};
	let read_channel = |channel_index: usize, default_value: f32| channels.get(channel_index).copied().unwrap_or(default_value);
	format!("#{:02x}{:02x}{:02x}{:02x}", to_byte(read_channel(0, 0.0)), to_byte(read_channel(1, 0.0)), to_byte(read_channel(2, 0.0)), to_byte(read_channel(3, 1.0)))
}

//==============================================================================
// 색 문자열 → Color.
//==============================================================================
fn create_color_from_text(hex_text: &str) -> Color {
	let channels = parse_timeline_color(hex_text);
	Color::create(channels[0], channels[1], channels[2], channels[3])
}

//==============================================================================
// 원본 Math.round. (절반은 위로)
//==============================================================================
fn round_half_up(value: f32) -> f32 {
	(value + 0.5).floor()
}

//==============================================================================
// 숫자 → 글. (원본 String(숫자) 처럼 정수는 소수점 없이, -0 은 0)
//==============================================================================
fn format_number(value: f32) -> String {
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
// 원본 Number(글) 흉내. (앞뒤 공백을 떼고, 빈 글은 0, 못 읽으면 NaN)
//==============================================================================
fn parse_number_text(text: &str) -> f32 {
	let trimmed_text = text.trim();
	if trimmed_text.is_empty() {
		return 0.0;
	}
	trimmed_text.parse::<f32>().unwrap_or(f32::NAN)
}

//==============================================================================
// JSON 값 → 숫자. (원본 Number(값). null 은 0, 없는 값과 객체는 NaN)
//==============================================================================
fn read_json_number(value: Option<&Value>) -> f32 {
	match value {
		Some(Value::Number(number)) => {
			number.as_f64().unwrap_or(f64::NAN) as f32
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
			f32::NAN
		}
	}
}

//==============================================================================
// 타임라인 값. (원본 키 값: 숫자, 문자열, 참거짓)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum TimelineValue {
	// 값 없음. (원본 null)
	None,
	Number(f32),
	String(String),
	Boolean(bool),
}

impl TimelineValue {
	//==============================================================================
	// JSON 값에서 만들기. (배열, 객체는 none)
	//==============================================================================
	pub fn from_json(value: &Value) -> TimelineValue {
		match value {
			Value::Number(number) => {
				TimelineValue::Number(number.as_f64().unwrap_or(0.0) as f32)
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
	// JSON 값으로 바꾸기.
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
	// 숫자로 읽기. (원본 Number(값). 문자열은 읽고, 참거짓은 1 / 0, 없음은 0)
	//==============================================================================
	pub fn as_number(&self) -> f32 {
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
	// 글로 읽기. (원본 String(값))
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
	// 참으로 볼지. (원본 visible 속성: true 또는 "true")
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
// 속성 종류. (number 는 이징 보간, color 는 채널 보간, string / boolean 은 계단)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelinePropertyKind {
	Number,
	Color,
	String,
	Boolean,
}

//==============================================================================
// 속성 정의. (이름 → 종류 / 표시 이름 / 노드에 거는 함수)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct TimelinePropertyDefinition {
	pub name: &'static str,
	pub kind: TimelinePropertyKind,
	pub label: &'static str,
	pub apply: Option<fn(&mut WorldNode, &TimelineValue)>,
}

//==============================================================================
// x 적용.
//==============================================================================
fn apply_position_x(node: &mut WorldNode, value: &TimelineValue) {
	let local_position = node.get_local_position();
	node.set_local_position(Vector2::create(value.as_number(), local_position.y));
}

//==============================================================================
// y 적용.
//==============================================================================
fn apply_position_y(node: &mut WorldNode, value: &TimelineValue) {
	let local_position = node.get_local_position();
	node.set_local_position(Vector2::create(local_position.x, value.as_number()));
}

//==============================================================================
// width 적용.
//==============================================================================
fn apply_width(node: &mut WorldNode, value: &TimelineValue) {
	let content_size = node.get_content_size();
	node.set_content_size(Vector2::create(value.as_number(), content_size.y));
}

//==============================================================================
// height 적용.
//==============================================================================
fn apply_height(node: &mut WorldNode, value: &TimelineValue) {
	let content_size = node.get_content_size();
	node.set_content_size(Vector2::create(content_size.x, value.as_number()));
}

//==============================================================================
// scale_x 적용.
//==============================================================================
fn apply_scale_x(node: &mut WorldNode, value: &TimelineValue) {
	let local_scale = node.get_local_scale();
	node.set_local_scale(Vector2::create(value.as_number(), local_scale.y));
}

//==============================================================================
// scale_y 적용.
//==============================================================================
fn apply_scale_y(node: &mut WorldNode, value: &TimelineValue) {
	let local_scale = node.get_local_scale();
	node.set_local_scale(Vector2::create(local_scale.x, value.as_number()));
}

//==============================================================================
// rotation 적용.
//==============================================================================
fn apply_rotation(node: &mut WorldNode, value: &TimelineValue) {
	node.set_local_rotation(value.as_number());
}

//==============================================================================
// opacity 적용. (0 ~ 1 로 자른다)
//==============================================================================
fn apply_opacity(node: &mut WorldNode, value: &TimelineValue) {
	node.set_local_opacity(value.as_number().clamp(0.0, 1.0));
}

//==============================================================================
// visible 적용. (true 또는 "true" 면 켠다)
//==============================================================================
fn apply_visible(node: &mut WorldNode, value: &TimelineValue) {
	node.set_active(value.is_true());
}

//==============================================================================
// color 적용.
//==============================================================================
fn apply_color(node: &mut WorldNode, value: &TimelineValue) {
	apply_node_color(node, &value.to_text());
}

//==============================================================================
// text 적용.
//==============================================================================
fn apply_text(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_text(&value.to_text());
	}
}

//==============================================================================
// number 적용. (반올림한 수를 글자로)
//==============================================================================
fn apply_number(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_text(&format_number(round_half_up(value.as_number())));
	}
}

//==============================================================================
// font_size 적용.
// - 글자 텍스처는 (문자열, 폰트) 조합마다 굽히므로 정수 픽셀로 맞춰 매 프레임 재굽기를 막는다.
//==============================================================================
fn apply_font_size(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_font_size(round_half_up(value.as_number()));
	}
}

//==============================================================================
// visibleCharacters 적용.
//==============================================================================
fn apply_visible_characters(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_visible_character_count(round_half_up(value.as_number()) as i32);
	}
}

//==============================================================================
// frame 적용. (스프라이트 시트 칸)
//==============================================================================
fn apply_frame(node: &mut WorldNode, value: &TimelineValue) {
	apply_sprite_frame(node, round_half_up(value.as_number()) as i64);
}

//==============================================================================
// effect 적용. (ShaderSprite 효과 진행률)
//==============================================================================
fn apply_effect_progress(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(shader_sprite) = node.get_component_mut::<ShaderSprite>() {
		shader_sprite.set_effect_progress(value.as_number());
	}
}

//==============================================================================
// 애니메이션 속성 정의 목록. (원본 TIMELINE_PROPERTY_DEFINITIONS 의 선언 차례)
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
// 속성 정의 찾기. (원본 TIMELINE_PROPERTY_DEFINITIONS[이름])
//==============================================================================
pub fn find_timeline_property_definition(property_name: &str) -> Option<&'static TimelinePropertyDefinition> {
	TIMELINE_PROPERTY_DEFINITIONS.iter().find(|definition| definition.name == property_name)
}

//==============================================================================
// 노드에서 색을 받는 컴포넌트에 색 걸기. (Text → 글자색, 그 밖에 Paint / Sprite / ShaderSprite → 색)
// - 원본은 Sprite, ShaderSprite 가 Paint 를 상속해서 get_component(Paint) 하나로 찾는다.
//==============================================================================
fn apply_node_color(node: &mut WorldNode, hex_text: &str) {
	let color = create_color_from_text(hex_text);
	if let Some(text_component) = node.get_component_mut::<Text>() {
		text_component.set_text_color(color);
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
// 스프라이트 시트 프레임 적용. (격자로 잘라 image_rect 를 고른다. 격자가 1 x 1 이거나 이미지가 없으면 그대로)
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
	let frame_width = image.get_width() as f32 / grid_columns as f32;
	let frame_height = image.get_height() as f32 / grid_rows as f32;
	let column = wrapped_index % grid_columns;
	let row = wrapped_index / grid_columns;
	shader_sprite.get_sprite_mut().set_image_rect(Rect::create(column as f32 * frame_width, row as f32 * frame_height, frame_width, frame_height));
}

//==============================================================================
// 값 보간. (종류별. number 는 선형, color 는 채널별, 그 밖에는 앞 값)
//==============================================================================
fn interpolate_value(kind: TimelinePropertyKind, from_value: &TimelineValue, to_value: &TimelineValue, eased_progress: f32) -> TimelineValue {
	match kind {
		TimelinePropertyKind::Number => {
			let from_number = from_value.as_number();
			let to_number = to_value.as_number();
			TimelineValue::Number(from_number + (to_number - from_number) * eased_progress)
		}
		TimelinePropertyKind::Color => {
			let from_channels = parse_timeline_color(&color_text_of(from_value));
			let to_channels = parse_timeline_color(&color_text_of(to_value));
			let mixed_channels: Vec<f32> = from_channels.iter().zip(to_channels.iter()).map(|(from_channel, to_channel)| from_channel + (to_channel - from_channel) * eased_progress).collect();
			TimelineValue::String(compose_timeline_color(&mixed_channels))
		}
		_ => {
			from_value.clone()
		}
	}
}

//==============================================================================
// 색 값의 글. (문자열이 아니면 원본처럼 흰색으로 읽히게 빈 글)
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
// 키 하나. (시간, 값, 그 키에서 다음 키까지의 이징, bezier 제어점)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineKey {
	pub time: f32,
	pub value: TimelineValue,
	pub easing: String,
	pub curve: Option<Vec<f32>>,
}

impl TimelineKey {
	//==============================================================================
	// 생성. (이징 linear)
	//==============================================================================
	pub fn new(time: f32, value: TimelineValue) -> TimelineKey {
		TimelineKey {
			time,
			value,
			easing: String::from("linear"),
			curve: None,
		}
	}

	//==============================================================================
	// JSON 키에서 만들기. ({ time, value, easing, curve })
	//==============================================================================
	pub fn from_json(key_description: &Value) -> TimelineKey {
		let time = key_description.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
		let value = key_description.get("value").map(TimelineValue::from_json).unwrap_or(TimelineValue::None);
		let easing = key_description.get("easing").and_then(|value| value.as_str()).unwrap_or("").to_string();
		let curve = key_description.get("curve").and_then(|value| value.as_array()).map(|curve_array| {
			curve_array.iter().map(|point| point.as_f64().unwrap_or(0.0) as f32).collect()
		});
		TimelineKey {
			time,
			value,
			easing,
			curve,
		}
	}

	//==============================================================================
	// 이 키에서 시작하는 구간의 이징.
	//==============================================================================
	pub fn resolve_easing(&self) -> TimelineEasing {
		resolve_timeline_easing(&self.easing, self.curve.as_deref())
	}
}

//==============================================================================
// 마커. (시간과 이름. 지나치면 마커 핸들러가 불린다)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineMarker {
	pub time: f32,
	pub name: String,
}

//==============================================================================
// 트랙 대상. (노드, 또는 노드가 아닌 것)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineTarget {
	// 노드. (정의된 속성은 노드에 바로 건다)
	Node(NodeId),
	// 노드가 아닌 것. (값을 모아 두었다가 apply_external_properties 로 넘긴다. 원본 set_timeline_property 를 가진 객체)
	External,
}

//==============================================================================
// 노드가 아닌 대상이 값을 받는 트레이트. (원본 set_timeline_property(이름, 값))
//==============================================================================
pub trait TimelinePropertyTarget {
	//==============================================================================
	// 속성 값 받기.
	//==============================================================================
	fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue);
}

impl TimelinePropertyTarget for ScreenEffect {
	//==============================================================================
	// 화면 효과에 값 걸기. (값을 그대로 넘긴다)
	//==============================================================================
	fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue) {
		ScreenEffect::set_timeline_property(self, property_name, value);
	}
}

//==============================================================================
// 모아 둔 external 값 하나.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineExternalProperty {
	pub target_name: String,
	pub property_name: String,
	pub value: TimelineValue,
}

//==============================================================================
// 서술 노드. (원본 무대 노드 서술. TIMELINE_NODE_DEFAULTS 가 기본값)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineNodeDescription {
	pub name: String,
	// 원본 키 type. ("group", "paint", "sprite", "text", "particle", "sound")
	pub node_type: String,
	pub parent: Option<String>,
	pub x: f32,
	pub y: f32,
	pub width: f32,
	pub height: f32,
	pub scale_x: f32,
	pub scale_y: f32,
	pub rotation: f32,
	pub opacity: f32,
	pub pivot_x: f32,
	pub pivot_y: f32,
	pub visible: bool,
	pub color: String,
	pub round_size: f32,
	pub image: String,
	pub frame_columns: f32,
	pub frame_rows: f32,
	pub frame: f32,
	pub effect: String,
	pub effect_progress: f32,
	pub effect_color: String,
	pub blend_mode: String,
	pub text: String,
	pub font_size: f32,
	pub bold: bool,
	pub text_align: String,
	pub particle: Option<Value>,
	pub audio: String,
	pub volume: f32,
}

impl Default for TimelineNodeDescription {
	//==============================================================================
	// 기본값. (원본 TIMELINE_NODE_DEFAULTS)
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
	// JSON 서술에서 만들기. (기본값 위에 준 항목을 덮는다. 원본 Object.assign({}, TIMELINE_NODE_DEFAULTS, 서술))
	// - 꼴이 맞지 않는 항목은 기본값을 둔다. visible 은 원본처럼 false 일 때만 끈다.
	//==============================================================================
	pub fn from_json(node_description: &Value) -> TimelineNodeDescription {
		let defaults = TimelineNodeDescription::default();
		let read_number = |name: &str, default_value: f32| node_description.get(name).and_then(|value| value.as_f64()).map(|value| value as f32).unwrap_or(default_value);
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
// 서술 노드 기본값. (원본 TIMELINE_NODE_DEFAULTS)
//==============================================================================
pub static TIMELINE_NODE_DEFAULTS: LazyLock<TimelineNodeDescription> = LazyLock::new(TimelineNodeDescription::default);

//==============================================================================
// 스프라이트 시트 격자. (노드 서술의 frame_columns / frame_rows. 원본 spriteFrameGridTable 자리)
// - 그리지도 갱신하지도 않는 표시용 컴포넌트다. frame 속성이 이것을 읽어 칸을 고른다.
//==============================================================================
pub struct TimelineSpriteFrameGrid {
	component_base: ComponentBase,
	columns: usize,
	rows: usize,
}

impl TimelineSpriteFrameGrid {
	//==============================================================================
	// 생성. (칸 수는 1 이상)
	//==============================================================================
	pub fn new(columns: usize, rows: usize) -> TimelineSpriteFrameGrid {
		TimelineSpriteFrameGrid {
			component_base: ComponentBase::new(),
			columns: columns.max(1),
			rows: rows.max(1),
		}
	}

	//==============================================================================
	// 격자 설정.
	//==============================================================================
	pub fn set_grid(&mut self, columns: usize, rows: usize) {
		self.columns = columns.max(1);
		self.rows = rows.max(1);
	}

	//==============================================================================
	// 가로 칸 수.
	//==============================================================================
	pub fn get_columns(&self) -> usize {
		self.columns
	}

	//==============================================================================
	// 세로 칸 수.
	//==============================================================================
	pub fn get_rows(&self) -> usize {
		self.rows
	}
}

impl Component for TimelineSpriteFrameGrid {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"TimelineSpriteFrameGrid"
	}

	//==============================================================================
	// 갱신할 것이 없다.
	//==============================================================================
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	// 아무것도 그리지 않으니 노드 거르기를 막지 않는다.
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		true
	}
}

//==============================================================================
// 사운드 노드 상태. (원본 soundPlayerTable 의 { audio_player, audio_asset, audio_source, volume } 자리)
// - 애셋과 플레이어는 Timeline::load_sounds 가 채운다. 이벤트 키 play / loop / stop 이 플레이어를 부린다.
//==============================================================================
pub struct TimelineSound {
	component_base: ComponentBase,
	audio_player: Option<AudioPlayer>,
	audio_asset: Option<Rc<AudioAsset>>,
	audio_source: String,
	volume: f32,
}

impl TimelineSound {
	//==============================================================================
	// 생성. (비어 있음)
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
	// 소리 주소 반환.
	//==============================================================================
	pub fn get_audio_source(&self) -> &str {
		&self.audio_source
	}

	//==============================================================================
	// 음량 반환. (원본처럼 서술 값을 들고만 있다)
	//==============================================================================
	pub fn get_volume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	// 오디오 애셋 반환.
	//==============================================================================
	pub fn get_audio_asset(&self) -> Option<&Rc<AudioAsset>> {
		self.audio_asset.as_ref()
	}

	//==============================================================================
	// 플레이어 반환.
	//==============================================================================
	pub fn get_audio_player_mut(&mut self) -> Option<&mut AudioPlayer> {
		self.audio_player.as_mut()
	}

	//==============================================================================
	// 멈춤. (플레이어가 있으면)
	//==============================================================================
	pub fn stop(&mut self) {
		if let Some(audio_player) = self.audio_player.as_mut() {
			audio_player.stop();
		}
	}

	//==============================================================================
	// 이벤트 처리. (play / loop 는 애셋이 읽혔으면 처음부터 튼다, stop 은 멈춘다)
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
	fn default() -> TimelineSound {
		TimelineSound::new()
	}
}

impl Component for TimelineSound {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"TimelineSound"
	}

	//==============================================================================
	// 갱신할 것이 없다.
	//==============================================================================
	fn is_tick_needed(&self) -> bool {
		false
	}
}

//==============================================================================
// 노드의 T 컴포넌트를 찾고 없으면 붙인다. (원본 get_or_add_component)
//==============================================================================
fn get_or_add_component<T: Component + 'static>(node: &mut WorldNode, create: impl FnOnce() -> T) -> &mut T {
	if !node.has_component::<T>() {
		node.add_component(create());
	}
	node.get_component_mut::<T>().expect("component just added")
}

//==============================================================================
// 가로 정렬 이름 읽기. (모르는 이름은 center)
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
// base64 풀기. (표준 알파벳, = 채움과 공백은 건너뛴다. 틀린 글자가 있으면 None)
//==============================================================================
fn decode_base64(text: &str) -> Option<Vec<u8>> {
	let mut bytes: Vec<u8> = Vec::with_capacity(text.len() * 3 / 4);
	let mut accumulator: u32 = 0;
	let mut bit_count = 0;
	for character in text.bytes() {
		let sextet = match character {
			b'A'..=b'Z' => {
				character - b'A'
			}
			b'a'..=b'z' => {
				character - b'a' + 26
			}
			b'0'..=b'9' => {
				character - b'0' + 52
			}
			b'+' | b'-' => {
				62
			}
			b'/' | b'_' => {
				63
			}
			b'=' | b' ' | b'\n' | b'\r' | b'\t' => {
				continue;
			}
			_ => {
				return None;
			}
		};
		accumulator = (accumulator << 6) | sextet as u32;
		bit_count += 6;
		if bit_count >= 8 {
			bit_count -= 8;
			bytes.push(((accumulator >> bit_count) & 0xff) as u8);
		}
	}
	Some(bytes)
}

//==============================================================================
// "data:" 주소 이미지 읽기. (";base64," 꼴만)
//==============================================================================
fn load_data_url_image(data_url: &str) -> Option<Rc<Image>> {
	let separator_index = data_url.find(',')?;
	let header = &data_url[..separator_index];
	if !header.ends_with(";base64") {
		return None;
	}
	let bytes = decode_base64(&data_url[separator_index + 1..])?;
	Image::load_from_memory(&bytes).ok()
}

//==============================================================================
// 컴파일한 트랙. (키를 시간순으로 정렬하고 대상 / 속성 정의를 붙인 것)
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
// 키를 시간순으로 정렬. (같은 시간은 원래 차례를 지킨다)
//==============================================================================
fn sort_keys_by_time(keys: &mut [TimelineKey]) {
	keys.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap_or(std::cmp::Ordering::Equal));
}

//==============================================================================
// 서술 트랙의 키 목록 읽기.
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

// 이벤트 핸들러. ((대상 이름, 이벤트 이름, 키, 노드 트리))
type TimelineEventHandler = Box<dyn FnMut(&str, &str, &TimelineKey, &mut NodeTree)>;
// 마커 핸들러. ((마커 이름, 마커, 노드 트리))
type TimelineMarkerHandler = Box<dyn FnMut(&str, &TimelineMarker, &mut NodeTree)>;
// 완료 핸들러. ((노드 트리))
type TimelineCompleteHandler = Box<dyn FnMut(&mut NodeTree)>;
// 대상 해석기. (이름 → 대상)
type TimelineTargetResolver = Box<dyn Fn(&str) -> Option<TimelineTarget>>;
// 이미지 해석기. (이미지 이름 → 이미지)
pub type TimelineImageResolver<'a> = &'a dyn Fn(&str) -> Option<Rc<Image>>;

//==============================================================================
// 타임라인.
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
	time: f32,
	speed: f32,
	is_playing: bool,
	is_dirty: bool,
	// 타임라인 시간으로 진행하는 파티클이 붙은 노드. (대상 노드에서 모은다)
	particle_node_ids: Vec<NodeId>,
	// 이벤트 트랙이 없는 파티클 노드. (타임라인이 시작하면 바로 도는 것으로 본다)
	particle_free_run_node_ids: Vec<NodeId>,
	// external 대상에 넘길 값. (evaluate 마다 새로 채운다)
	external_property_list: Vec<TimelineExternalProperty>,
}

impl Timeline {
	//==============================================================================
	// 생성. (서술이 없으면 빈 서술)
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
	// 빈 서술 생성. (정적)
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
	// JSON 글에서 만들기. (원본 loadFromUrl 의 response.json() 다음 자리)
	//==============================================================================
	pub fn load_from_text(text: &str) -> Result<Timeline, String> {
		let description: Value = serde_json::from_str(text).map_err(|error| format!("Timeline: JSON 을 읽지 못했습니다. ({})", error))?;
		Ok(Timeline::new(Some(description)))
	}

	//==============================================================================
	// 읽은 JsonAsset 에서 만들기. (아직 안 읽혔거나 실패했으면 None)
	//==============================================================================
	pub fn create_from_json_asset(json_asset: &mut JsonAsset) -> Option<Timeline> {
		let description = json_asset.get_data()?.clone();
		Some(Timeline::new(Some(description)))
	}

	//==============================================================================
	// 서술 설정. (빠진 stage / nodes / tracks / markers 를 채우고 다시 컴파일하게 한다)
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
	// 서술 반환.
	//==============================================================================
	pub fn get_description(&self) -> &Value {
		&self.description
	}

	//==============================================================================
	// 서술 반환. (변경용. 트랙 / 키를 고쳤으면 invalidate 를 부른다)
	//==============================================================================
	pub fn get_description_mut(&mut self) -> &mut Value {
		&mut self.description
	}

	//==============================================================================
	// 다시 컴파일 요청. (트랙 / 키가 바뀌었을 때)
	//==============================================================================
	pub fn invalidate(&mut self) {
		self.is_dirty = true;
	}

	//==============================================================================
	// 서술 배열 반환. (stage.nodes 처럼 경로로)
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
	// 컴파일. (키를 시간순으로 정렬하고 대상 / 속성 정의를 붙인 뒤 파티클을 모은다)
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
				time: marker.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32,
				name: marker.get("name").and_then(|value| value.as_str()).unwrap_or("").to_string(),
			}
		}).collect();
		compiled_markers.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap_or(std::cmp::Ordering::Equal));
		self.compiled_markers = compiled_markers;
		self.is_dirty = false;
		self.collect_particle_systems(node_tree);
	}

	//==============================================================================
	// 더러우면 컴파일.
	//==============================================================================
	fn compile_if_dirty(&mut self, node_tree: &mut NodeTree) {
		if self.is_dirty {
			self.compile(node_tree);
		}
	}

	//==============================================================================
	// 대상 노드의 파티클 시스템 수집.
	// - 수동 tick 으로 바꿔 타임라인 시간으로만 진행하고, 씨앗을 줘서 되감을 때마다 같은 결과가 나오게 한다.
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
				TimelineTarget::External => {
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
	// 파티클 시간 진행. (재생 중 매 프레임)
	//==============================================================================
	pub fn simulate_particles(&mut self, node_tree: &mut NodeTree, time_delta: f32) {
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
	// 파티클 재시뮬레이션. (스크럽 / 되감기. 비운 뒤 0 초부터 이벤트를 다시 밟으며 진행)
	//==============================================================================
	pub fn resimulate_particles(&mut self, node_tree: &mut NodeTree, time: f32) {
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
		let mut event_list: Vec<(f32, usize, usize)> = Vec::new();
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

		let step_seconds = (1.0f32 / 60.0).max(time / 600.0);
		let mut current_time = 0.0f32;
		for (event_time, track_index, key_index) in event_list {
			self.advance_particles_to(node_tree, &mut current_time, event_time, step_seconds);
			self.dispatch_event(node_tree, track_index, key_index, true);
		}
		self.advance_particles_to(node_tree, &mut current_time, time, step_seconds);
	}

	//==============================================================================
	// 파티클을 step_seconds 씩 target_time 까지 진행. (재시뮬레이션용)
	//==============================================================================
	fn advance_particles_to(&mut self, node_tree: &mut NodeTree, current_time: &mut f32, target_time: f32, step_seconds: f32) {
		while *current_time < target_time - 0.000001 {
			let step_delta = step_seconds.min(target_time - *current_time);
			self.simulate_particles(node_tree, step_delta);
			*current_time += step_delta;
		}
	}

	//==============================================================================
	// 대상 해석. (대상 표 → 사용자 해석기)
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
	// 대상 해석기 설정. (이름 → 대상. 노드가 아닌 것은 TimelineTarget::External)
	//==============================================================================
	pub fn set_target_resolver(&mut self, target_resolver: Option<TimelineTargetResolver>) {
		self.target_resolver = target_resolver;
		self.invalidate();
	}

	//==============================================================================
	// 대상 직접 등록. (같은 이름이 있으면 바꾼다)
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
	// 대상 반환. (대상 표에 있는 것만)
	//==============================================================================
	pub fn get_target(&self, target_name: &str) -> Option<TimelineTarget> {
		self.target_table.iter().find(|(name, _)| name == target_name).map(|(_, target)| *target)
	}

	//==============================================================================
	// 노드 트리에 이름으로 붙이기. (무대를 직접 만든 경우. 뿌리 자신, 또는 그 자손에서 트랙 대상 이름을 찾는다)
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
	// 무대 노드 생성. (서술의 stage.nodes 를 노드 트리로 만들고 대상으로 등록한다. 이름 → 노드 표를 돌려준다)
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
	// 무대 노드 한 개 생성. (정적. 붙이지 않은 노드를 만든다)
	//==============================================================================
	pub fn create_stage_node(node_tree: &mut NodeTree, node_description: &Value, image_resolver: Option<TimelineImageResolver>) -> NodeId {
		let node_name = node_description.get("name").and_then(|value| value.as_str()).unwrap_or("");
		let node_id = node_tree.create_node(node_name);
		Timeline::apply_stage_node_description(&mut node_tree[node_id], node_description, image_resolver);
		node_id
	}

	//==============================================================================
	// 노드에 서술 반영. (정적. 편집기가 값을 고칠 때도 쓴다)
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
			if image.is_none() && description.image.starts_with("data:") {
				image = load_data_url_image(&description.image);
			}
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
	// 사운드 노드 준비. (원본 setAudioManager + 사운드 노드의 애셋 읽기 자리)
	// - 대상 노드 가운데 TimelineSound 가 붙은 것마다 아직 없는 오디오 애셋을 읽고 플레이어를 만든다. 새로 읽은 애셋을 돌려준다.
	//==============================================================================
	pub fn load_sounds(&mut self, node_tree: &mut NodeTree, engine: &Engine) -> Vec<Rc<AudioAsset>> {
		let mut loaded_audio_assets: Vec<Rc<AudioAsset>> = Vec::new();
		for (_, target) in self.target_table.iter() {
			let node_id = match target {
				TimelineTarget::Node(node_id) => {
					*node_id
				}
				TimelineTarget::External => {
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
	// 사운드 노드 정지. (정적. 편집기가 정지 / 되감기 때 부른다)
	//==============================================================================
	pub fn stop_sound(node: &mut WorldNode) {
		if let Some(sound) = node.get_component_mut::<TimelineSound>() {
			sound.stop();
		}
	}

	//==============================================================================
	// 재생. (반복이 아닌데 끝에 있으면 처음부터)
	//==============================================================================
	pub fn play(&mut self) {
		if self.time >= self.get_duration() && !self.is_loop() {
			self.time = 0.0;
		}
		self.is_playing = true;
	}

	//==============================================================================
	// 일시 정지.
	//==============================================================================
	pub fn pause(&mut self) {
		self.is_playing = false;
	}

	//==============================================================================
	// 정지. (소리와 파티클을 멈추고 처음으로)
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
	// 시간 이동. (이벤트는 발생하지 않는다. 파티클은 그 시간까지 다시 진행한다)
	//==============================================================================
	pub fn seek(&mut self, node_tree: &mut NodeTree, time: f32) {
		self.time = time.min(self.get_duration()).max(0.0);
		let current_time = self.time;
		self.evaluate(node_tree, current_time);
		self.resimulate_particles(node_tree, current_time);
	}

	//==============================================================================
	// 갱신. (시간을 흘리고 값을 적용, 지나친 이벤트 / 마커를 발생)
	//==============================================================================
	pub fn tick(&mut self, node_tree: &mut NodeTree, time_delta: f32) {
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
				if let Some(complete_handler) = self.complete_handler.as_mut() {
					complete_handler(node_tree);
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
	// 구간 안의 이벤트 키 / 마커 발생. (from_time 초과 ~ to_time 이하. 이벤트 트랙 차례, 그다음 마커)
	//==============================================================================
	pub fn fire_events_between(&mut self, node_tree: &mut NodeTree, from_time: f32, to_time: f32) {
		self.compile_if_dirty(node_tree);
		let is_inside = |time: f32| time > from_time && time <= to_time;
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
		if let Some(marker_handler) = self.marker_handler.as_mut() {
			for marker in self.compiled_markers.iter() {
				if is_inside(marker.time) {
					marker_handler(&marker.name, marker, node_tree);
				}
			}
		}
	}

	//==============================================================================
	// 이벤트 키 처리. (파티클 대상은 play / stop / clear / emit:N 을 스스로 처리하고, 사운드 대상은 play / loop / stop, 그 뒤 핸들러)
	// - is_silent 는 재시뮬레이션용이다. (사운드 / 사용자 핸들러는 부르지 않고 파티클만)
	//==============================================================================
	fn dispatch_event(&mut self, node_tree: &mut NodeTree, track_index: usize, key_index: usize, is_silent: bool) {
		let (target, target_name, key) = {
			let compiled_track = &self.compiled_tracks[track_index];
			(compiled_track.target, compiled_track.target_name.clone(), compiled_track.keys[key_index].clone())
		};
		let event_name = key.value.to_text();
		if let Some(TimelineTarget::Node(node_id)) = target {
			if let Some(node) = node_tree.get_node_mut(node_id) {
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
						particle_system.emit(emit_count.max(0.0).ceil() as usize, None);
					}
				}
			}
		}
		if !is_silent {
			if let Some(event_handler) = self.event_handler.as_mut() {
				event_handler(&target_name, &event_name, &key, node_tree);
			}
		}
	}

	//==============================================================================
	// 시간의 값 적용. (연속 속성만, 이벤트는 tick 에서만 발생. external 대상 값은 모아 둔다)
	//==============================================================================
	pub fn evaluate(&mut self, node_tree: &mut NodeTree, time: f32) {
		self.compile_if_dirty(node_tree);
		self.external_property_list.clear();
		for compiled_track in self.compiled_tracks.iter() {
			if !compiled_track.is_enabled || compiled_track.keys.is_empty() || compiled_track.property_name == "event" {
				continue;
			}
			let target = match compiled_track.target {
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
					if let (Some(apply), Some(node)) = (apply, node_tree.get_node_mut(node_id)) {
						apply(node, &value);
					}
				}
				TimelineTarget::External => {
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
	// 모아 둔 external 값 넘기기. (그 이름의 대상 값을 set_timeline_property 로 넘기고 목록에서 뺀다. 원본 evaluate 의 set_timeline_property 자리)
	//==============================================================================
	pub fn apply_external_properties(&mut self, target_name: &str, target: &mut dyn TimelinePropertyTarget) {
		self.external_property_list.retain(|external_property| {
			if external_property.target_name != target_name {
				return true;
			}
			target.set_timeline_property(&external_property.property_name, &external_property.value);
			false
		});
	}

	//==============================================================================
	// 모아 둔 external 값 반환. (넘기지 않은 것)
	//==============================================================================
	pub fn get_external_properties(&self) -> &[TimelineExternalProperty] {
		&self.external_property_list
	}

	//==============================================================================
	// 키 목록에서 시간의 값 표본. (정적. 키는 시간순이어야 한다. 편집기의 커브 그리기도 쓴다)
	//==============================================================================
	pub fn sample_keys(sorted_keys: &[TimelineKey], kind: TimelinePropertyKind, time: f32) -> TimelineValue {
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
	// 서술 트랙의 시간 값 표본. (정적. 서술 트랙 그대로, 정렬은 여기서. 정의가 없는 속성은 number)
	//==============================================================================
	pub fn sample_track(track: &Value, time: f32) -> TimelineValue {
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
	// 시간 반환.
	//==============================================================================
	pub fn get_time(&self) -> f32 {
		self.time
	}

	//==============================================================================
	// 재생 중 여부 반환.
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.is_playing
	}

	//==============================================================================
	// 속도 설정. (1 = 실시간)
	//==============================================================================
	pub fn set_speed(&mut self, speed: f32) {
		self.speed = speed;
	}

	//==============================================================================
	// 속도 반환.
	//==============================================================================
	pub fn get_speed(&self) -> f32 {
		self.speed
	}

	//==============================================================================
	// 반복 설정. (서술의 loop 를 바꾼다)
	//==============================================================================
	pub fn set_loop(&mut self, is_loop: bool) {
		if let Some(description_object) = self.description.as_object_mut() {
			description_object.insert(String::from("loop"), Value::Bool(is_loop));
		}
	}

	//==============================================================================
	// 반복 여부 반환. (서술의 loop 가 true 일 때만)
	//==============================================================================
	pub fn is_loop(&self) -> bool {
		self.description.get("loop") == Some(&Value::Bool(true))
	}

	//==============================================================================
	// 길이 반환. (초, 못 읽거나 0 이하면 0)
	//==============================================================================
	pub fn get_duration(&self) -> f32 {
		let duration = read_json_number(self.description.get("duration"));
		if duration.is_finite() && duration > 0.0 {
			duration
		}
		else {
			0.0
		}
	}

	//==============================================================================
	// 초당 프레임 반환. (못 읽거나 0 이하면 30)
	//==============================================================================
	pub fn get_frame_rate(&self) -> f32 {
		let frame_rate = read_json_number(self.description.get("frameRate"));
		if frame_rate.is_finite() && frame_rate > 0.0 {
			frame_rate
		}
		else {
			30.0
		}
	}

	//==============================================================================
	// 이벤트 핸들러 설정. ((대상 이름, 이벤트 이름, 키, 노드 트리))
	//==============================================================================
	pub fn set_event_handler(&mut self, event_handler: Option<TimelineEventHandler>) {
		self.event_handler = event_handler;
	}

	//==============================================================================
	// 마커 핸들러 설정. ((마커 이름, 마커, 노드 트리))
	//==============================================================================
	pub fn set_marker_handler(&mut self, marker_handler: Option<TimelineMarkerHandler>) {
		self.marker_handler = marker_handler;
	}

	//==============================================================================
	// 완료 핸들러 설정. (반복이 아닐 때 끝에 닿으면)
	//==============================================================================
	pub fn set_complete_handler(&mut self, complete_handler: Option<TimelineCompleteHandler>) {
		self.complete_handler = complete_handler;
	}
}

impl Default for Timeline {
	fn default() -> Timeline {
		Timeline::new(None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;

	//==============================================================================
	// 두 수가 오차 안에서 같은지.
	//==============================================================================
	fn assert_near(actual: f32, expected: f64, tolerance: f64, label: &str) {
		assert!(((actual as f64) - expected).abs() <= tolerance, "{}: {} != {}", label, actual, expected);
	}

	//==============================================================================
	// 이징 곡선이 원본과 같은 값을 낸다. (원본 node 실행 값. exponential, elastic 은 원본 수식을 실제 Math 로 계산한 값)
	//==============================================================================
	#[test]
	fn easing_curves_match_original() {
		let progress_list = [0.0f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
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
	// 베지어 이징이 원본과 같은 값을 낸다. (제어점 x 가 0 ~ 1 밖이면 자른다)
	//==============================================================================
	#[test]
	fn bezier_easing_matches_original() {
		let progress_list = [0.0f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
		let expected_table: [([f32; 4], [f64; 7]); 4] = [
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
	// 색 글 읽기가 원본 parseInt 와 같게 읽는다. (못 읽는 채널은 1)
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
	// 채널 → 색 글이 원본과 같다. (0 ~ 1 로 자르고 반올림, 알파가 없으면 ff)
	//==============================================================================
	#[test]
	fn compose_timeline_color_matches_original() {
		assert_eq!(compose_timeline_color(&[1.0, 0.5, 0.0, 1.0]), "#ff8000ff");
		assert_eq!(compose_timeline_color(&[0.2, 0.4, 0.6]), "#336699ff");
		assert_eq!(compose_timeline_color(&[1.2, -0.1, 0.5, 0.5]), "#ff008080");
		assert_eq!(compose_timeline_color(&[0.5, 0.25, 0.75, 0.1]), "#8040bf1a");
	}

	//==============================================================================
	// 트랙 표본이 원본과 같다. (정렬, 구간 이징, 베지어, 계단, 색 보간, 문자열 / 참거짓 계단, 같은 시간 키)
	//==============================================================================
	#[test]
	fn sample_track_matches_original() {
		let time_list = [-1.0f32, 0.0, 0.3, 0.5, 0.99, 1.0, 1.4, 2.0, 2.5, 5.0];
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
		// 1 초의 빨강은 원본 엔진 sin(급수 근사)이 0.5000000000000017 을 내서 7f 로 내림된다. 실제 sin 은 0.5 라 80 이다.
		let expected_colors = ["#ff000080", "#ff000080", "#f1000e87", "#da002593", "#82007dbf", "#800080c0", "#3500cae5", "#0000ffff", "#0000ffff", "#0000ffff"];
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
	// 값을 받아 적는 external 대상. (시험용)
	//==============================================================================
	struct RecordingTarget {
		log: Rc<RefCell<Vec<String>>>,
	}

	impl TimelinePropertyTarget for RecordingTarget {
		//==============================================================================
		// 받은 값을 적는다.
		//==============================================================================
		fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue) {
			self.log.borrow_mut().push(format!("set {}={:.4}", property_name, value.as_number()));
		}
	}

	//==============================================================================
	// 재생이 원본과 같은 차례로 값 / 이벤트 / 마커 / 완료를 낸다. (원본 node 실행 기록과 비교)
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
		timeline.set_target_resolver(Some(Box::new(|target_name: &str| {
			if target_name == "screen" {
				Some(TimelineTarget::External)
			}
			else {
				None
			}
		})));
		let event_log = log.clone();
		timeline.set_event_handler(Some(Box::new(move |target_name: &str, event_name: &str, _key: &TimelineKey, _node_tree: &mut NodeTree| {
			event_log.borrow_mut().push(format!("event {}:{}", target_name, event_name));
		})));
		let marker_log = log.clone();
		timeline.set_marker_handler(Some(Box::new(move |marker_name: &str, _marker: &TimelineMarker, _node_tree: &mut NodeTree| {
			marker_log.borrow_mut().push(format!("marker {}", marker_name));
		})));
		let complete_log = log.clone();
		timeline.set_complete_handler(Some(Box::new(move |_node_tree: &mut NodeTree| {
			complete_log.borrow_mut().push(String::from("complete"));
		})));
		let mut screen = RecordingTarget { log: log.clone() };
		//==============================================================================
		// 한 번 갱신하고 모인 external 값을 넘긴다.
		//==============================================================================
		fn tick_and_record(timeline: &mut Timeline, node_tree: &mut NodeTree, screen: &mut RecordingTarget, time_delta: f32) {
			timeline.tick(node_tree, time_delta);
			timeline.apply_external_properties("screen", screen);
		}

		timeline.play();
		for time_delta in [0.25f32, 0.25, 0.6, 0.6, 0.6, 0.6] {
			tick_and_record(&mut timeline, &mut node_tree, &mut screen, time_delta);
			log.borrow_mut().push(format!("time {:.4}", timeline.get_time()));
		}
		timeline.set_loop(false);
		tick_and_record(&mut timeline, &mut node_tree, &mut screen, 0.7);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		tick_and_record(&mut timeline, &mut node_tree, &mut screen, 0.3);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		timeline.play();
		log.borrow_mut().push(format!("replay time {:.4}", timeline.get_time()));
		timeline.seek(&mut node_tree, 1.25);
		timeline.apply_external_properties("screen", &mut screen);
		log.borrow_mut().push(format!("seek time {:.4}", timeline.get_time()));
		tick_and_record(&mut timeline, &mut node_tree, &mut screen, 1.0);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		timeline.play();
		log.borrow_mut().push(format!("replay time {:.4} playing {}", timeline.get_time(), timeline.is_playing()));
		tick_and_record(&mut timeline, &mut node_tree, &mut screen, 0.1);
		log.borrow_mut().push(format!("time {:.4}", timeline.get_time()));

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
			// 원본은 끝에 닿은 tick 에서 값을 건 뒤 완료 핸들러를 부른다. 러스트 판은 external 값을 tick 이 끝난 뒤 넘겨서 완료가 먼저 적힌다.
			"event screen:end", "complete", "set vignette=1.0000", "time 2.0000 playing false",
			"replay time 0.0000 playing true",
			"set vignette=0.0500", "time 0.1000",
		];
		assert_eq!(*log.borrow(), expected_log.iter().map(|line| line.to_string()).collect::<Vec<String>>());
	}

	//==============================================================================
	// 같은 이름 무대 노드는 모두 붙고 이름 표에는 뒤의 것이 남는다. 자기 자신을 부모로 두면 무대 부모에 붙는다.
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
	// 무대를 만들고 트랙 값을 노드에 건다. (부모 관계, Paint / Text 색, 글, 보이기, 위치)
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
	// bind 는 이미 있는 노드 트리에서 트랙 대상 이름을 찾아 붙인다.
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
	// 파티클은 타임라인 시간으로만 진행하고, 이벤트 play 로 버스트가 나며, 되감아도 같은 결과가 나온다.
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
	// frame 속성은 스프라이트 시트 격자에서 칸을 고른다. (넘치면 감는다)
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
	// 서술을 채우고 길이 / 프레임 / 반복을 원본처럼 읽는다. (data 주소 base64 풀기 포함)
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
		assert_eq!(decode_base64("SGVsbG8="), Some(b"Hello".to_vec()));
		assert_eq!(decode_base64("SGk"), Some(b"Hi".to_vec()));
		assert_eq!(decode_base64("S*"), None);
	}
}
