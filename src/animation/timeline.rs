//==============================================================================
// 타임라인. (키프레임 애니메이션 문서 재생기)
// - 원본 experimental/animation/timeline.js 를 옮긴 것이다. 문서(서술)는 무대 노드 목록 + 트랙(대상 이름 / 속성 / 키) + 마커로 이루어진 JSON 이며
//   타임라인 편집기가 저장한다. 키의 easing 은 그 키에서 다음 키까지 구간에 걸린다.
// - 이징 곡선은 core/tween.rs 의 easingFunction 을 그대로 쓴다. "bezier" 는 CSS cubic-bezier 와 같은 제어점이다.
// - 사용:
//     // 씬 create 에서 문서를 요청하고 맡긴다.
//     self.timelineAsset.load(engine, "assets/intro.timeline.json");
//     self.sceneBase.trackAsset(&self.timelineAsset);
//     // initialize 에서 무대를 만들고 튼다.
//     let mut timeline = Timeline::createFromJsonAsset(&mut self.timelineAsset).unwrap_or_default();
//     let stageId = self.sceneBase.getRootId();
//     timeline.buildStage(self.sceneBase.getNodeTreeMut(), stageId, Some(&|imageName: &str| imageTable.get(imageName).cloned()));
//     timeline.play();
//     // 매 프레임 (tick):
//     timeline.tick(self.sceneBase.getNodeTreeMut(), timeDelta);
//     timeline.applyExternalProperties("screen", &mut self.screenEffect);
// - 원본과 다른 점:
//   - 대상은 객체 참조 대신 TimelineTarget 이다. 노드는 NodeId(node), 노드가 아닌 것(화면 효과 같은)은 external 이다.
//     노드에 거는 함수(tick, seek, stop, evaluate, compile, buildStage, bind)는 NodeTree 를 같이 받는다.
//   - external 대상의 값은 원본처럼 그 자리에서 setTimelineProperty 를 부르지 못해서(대상을 빌려 들 수 없다) 모아 두었다가,
//     씬이 applyExternalProperties(대상 이름, 대상) 로 넘긴다. 모은 값은 evaluate 를 할 때마다 새로 채운다.
//     넘겨받는 쪽은 TimelinePropertyTarget 트레이트를 구현한다. ScreenEffect 는 이 파일에서 구현했다.
//   - 값은 원본의 숫자 / 문자열 / 참거짓을 TimelineValue 열거형으로 든다. 숫자는 f32 다.
//   - 핸들러는 원본 마지막 인자(타임라인 자신) 대신 NodeTree 를 받는다. (타임라인은 도는 중이라 빌려 줄 수 없다)
//     씬 상태를 바꾸려면 Rc<RefCell<...>> 를 잡아 두거나 NodeTree 로 노드를 바꾼다.
//   - resolveTimelineEasing 은 함수 대신 TimelineEasing 값을 돌려준다. (evaluate(progress) 로 부른다)
//   - TIMELINE_PROPERTY_DEFINITIONS 는 이름 → 정의 객체 대신 정의 배열이고 findTimelinePropertyDefinition 으로 찾는다.
//     TIMELINE_NODE_DEFAULTS 는 기본값을 채운 TimelineNodeDescription 이다. (원본 키 type 은 nodeType)
//   - 원본의 스프라이트 격자 표와 사운드 표(WeakMap)는 노드에 붙는 작은 컴포넌트 TimelineSpriteFrameGrid, TimelineSound 로 바꿨다.
//   - 원본 Timeline.setAudioManager(정적)와 사운드 노드의 비동기 읽기는 loadSounds(nodeTree, engine) 로 바꿨다.
//     무대를 만든 뒤 한 번 부르면 사운드 노드마다 오디오 애셋을 읽고 플레이어를 만든다. (돌려준 애셋을 씬이 trackAudioAsset 으로 맡겨도 된다)
//   - loadFromUrl(비동기 fetch) 대신 JsonAsset 으로 받은 문서를 createFromJsonAsset / loadFromText 로 넘긴다.
//   - "data:" 주소 이미지는 원본처럼 나중에 읽히지 않고 그 자리에서 base64 를 풀어 만든다.
//   - 원본 엔진의 Math.pow 는 정수 지수만 맞게 계산해서 원본 exponential, elastic 곡선은 소수 지수에서 값이 틀어진다.
//     러스트 판은 Tween 처럼 실제 powf 를 쓴다. (나머지 곡선은 원본과 같은 값이 나온다)
//   - 같은 이름의 무대 노드가 둘이면 원본은 앞의 것을 붙이지 않고 버린다. 러스트 판은 모두 붙이고 이름 표에는 뒤의 것이 남는다.
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
use crate::core::tween::easingFunction;
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
// 이징 이름 목록. ("linear" / "step" / "bezier" / "<종류>.<in|out|inOut>")
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

// 이름 → Tween 이징 함수. (원본 Tween.easingFunction[종류][방향])
const EASING_FUNCTION_TABLE: [(&str, EasingFunction); 30] = [
	("quadratic.in", easingFunction::quadratic::r#in),
	("quadratic.out", easingFunction::quadratic::out),
	("quadratic.inOut", easingFunction::quadratic::inOut),
	("cubic.in", easingFunction::cubic::r#in),
	("cubic.out", easingFunction::cubic::out),
	("cubic.inOut", easingFunction::cubic::inOut),
	("quartic.in", easingFunction::quartic::r#in),
	("quartic.out", easingFunction::quartic::out),
	("quartic.inOut", easingFunction::quartic::inOut),
	("quintic.in", easingFunction::quintic::r#in),
	("quintic.out", easingFunction::quintic::out),
	("quintic.inOut", easingFunction::quintic::inOut),
	("sinusoidal.in", easingFunction::sinusoidal::r#in),
	("sinusoidal.out", easingFunction::sinusoidal::out),
	("sinusoidal.inOut", easingFunction::sinusoidal::inOut),
	("exponential.in", easingFunction::exponential::r#in),
	("exponential.out", easingFunction::exponential::out),
	("exponential.inOut", easingFunction::exponential::inOut),
	("circular.in", easingFunction::circular::r#in),
	("circular.out", easingFunction::circular::out),
	("circular.inOut", easingFunction::circular::inOut),
	("elastic.in", easingFunction::elastic::r#in),
	("elastic.out", easingFunction::elastic::out),
	("elastic.inOut", easingFunction::elastic::inOut),
	("back.in", easingFunction::back::r#in),
	("back.out", easingFunction::back::out),
	("back.inOut", easingFunction::back::inOut),
	("bounce.in", easingFunction::bounce::r#in),
	("bounce.out", easingFunction::bounce::out),
	("bounce.inOut", easingFunction::bounce::inOut),
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
fn evaluateBezierEasing(x1: f32, y1: f32, x2: f32, y2: f32, progress: f32) -> f32 {
	if progress <= 0.0 {
		return 0.0;
	}
	if progress >= 1.0 {
		return 1.0;
	}
	let clampedX1 = x1.clamp(0.0, 1.0);
	let clampedX2 = x2.clamp(0.0, 1.0);
	let sampleCurve = |t: f32, point1: f32, point2: f32| {
		let oneMinus = 1.0 - t;
		3.0 * oneMinus * oneMinus * t * point1 + 3.0 * oneMinus * t * t * point2 + t * t * t
	};
	let sampleDerivative = |t: f32, point1: f32, point2: f32| {
		let oneMinus = 1.0 - t;
		3.0 * oneMinus * oneMinus * point1 + 6.0 * oneMinus * t * (point2 - point1) + 3.0 * t * t * (1.0 - point2)
	};
	let mut t = progress;
	for _ in 0..8 {
		let error = sampleCurve(t, clampedX1, clampedX2) - progress;
		if error.abs() < 0.00001 {
			break;
		}
		let derivative = sampleDerivative(t, clampedX1, clampedX2);
		if derivative.abs() < 0.000001 {
			break;
		}
		t -= error / derivative;
		t = t.clamp(0.0, 1.0);
	}
	sampleCurve(t, y1, y2)
}

//==============================================================================
// 3차 베지어 이징 생성. (CSS cubic-bezier 와 같은 제어점, x 는 0 ~ 1)
//==============================================================================
pub fn createBezierEasing(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 + Clone {
	move |progress: f32| evaluateBezierEasing(x1, y1, x2, y2, progress)
}

//==============================================================================
// 계단 이징. (구간 내내 0, 다음 키에서 바뀐다)
//==============================================================================
fn stepEasing(_progress: f32) -> f32 {
	0.0
}

//==============================================================================
// 타임라인 이징. (원본에서 resolveTimelineEasing 이 돌려주는 함수 자리)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub enum TimelineEasing {
	// Tween 이징 함수, linear, step.
	function(EasingFunction),
	// 3차 베지어 (x1, y1, x2, y2).
	bezier(f32, f32, f32, f32),
}

impl TimelineEasing {
	//==============================================================================
	// 진행률(0 ~ 1)의 이징 값.
	//==============================================================================
	pub fn evaluate(&self, progress: f32) -> f32 {
		match self {
			TimelineEasing::function(easing) => {
				easing(progress)
			}
			TimelineEasing::bezier(x1, y1, x2, y2) => {
				evaluateBezierEasing(*x1, *y1, *x2, *y2, progress)
			}
		}
	}
}

//==============================================================================
// 이징 반환. (모르는 이름은 linear, "bezier" 는 curve [x1, y1, x2, y2] 를 쓰고 없으면 ease-in-out)
//==============================================================================
pub fn resolveTimelineEasing(easingName: &str, curve: Option<&[f32]>) -> TimelineEasing {
	if easingName.is_empty() || easingName == "linear" {
		return TimelineEasing::function(easingFunction::linear);
	}
	if easingName == "step" {
		return TimelineEasing::function(stepEasing);
	}
	if easingName == "bezier" {
		let controlPoints = match curve {
			Some(curve) if curve.len() >= 4 => {
				[curve[0], curve[1], curve[2], curve[3]]
			}
			_ => {
				DEFAULT_BEZIER_CURVE
			}
		};
		return TimelineEasing::bezier(controlPoints[0], controlPoints[1], controlPoints[2], controlPoints[3]);
	}
	match EASING_FUNCTION_TABLE.iter().find(|(name, _)| *name == easingName) {
		Some((_, easing)) => {
			TimelineEasing::function(*easing)
		}
		None => {
			TimelineEasing::function(easingFunction::linear)
		}
	}
}

//==============================================================================
// 원본 parseInt(글, 16) 흉내. (앞 공백, 부호, 0x 를 건너뛰고 16 진 숫자가 이어지는 데까지. 숫자가 없으면 None = NaN)
//==============================================================================
fn parseLeadingHexadecimal(text: &str) -> Option<f32> {
	let trimmedText = text.trim_start();
	let (sign, unsignedText) = if let Some(rest) = trimmedText.strip_prefix('-') {
		(-1.0, rest)
	}
	else if let Some(rest) = trimmedText.strip_prefix('+') {
		(1.0, rest)
	}
	else {
		(1.0, trimmedText)
	};
	let digitText = unsignedText.strip_prefix("0x").or_else(|| unsignedText.strip_prefix("0X")).unwrap_or(unsignedText);
	let mut value: f32 = 0.0;
	let mut digitCount = 0;
	for character in digitText.chars() {
		match character.to_digit(16) {
			Some(digit) => {
				value = value * 16.0 + digit as f32;
				digitCount += 1;
			}
			None => {
				break;
			}
		}
	}
	if digitCount == 0 {
		return None;
	}
	Some(sign * value)
}

//==============================================================================
// 색 문자열 → 채널. ("#rrggbb" 또는 "#rrggbbaa", 못 읽는 채널은 1)
//==============================================================================
pub fn parseTimelineColor(hexText: &str) -> [f32; 4] {
	let characters: Vec<char> = match hexText.strip_prefix('#') {
		Some(rest) => {
			rest.chars().collect()
		}
		None => {
			"ffffff".chars().collect()
		}
	};
	let readChannel = |startIndex: usize| -> Option<f32> {
		let endIndex = (startIndex + 2).min(characters.len());
		if startIndex >= endIndex {
			return None;
		}
		let channelText: String = characters[startIndex..endIndex].iter().collect();
		parseLeadingHexadecimal(&channelText).map(|value| value / 255.0)
	};
	let red = readChannel(0).unwrap_or(1.0);
	let green = readChannel(2).unwrap_or(1.0);
	let blue = readChannel(4).unwrap_or(1.0);
	let alpha = if characters.len() >= 8 {
		readChannel(6).unwrap_or(1.0)
	}
	else {
		1.0
	};
	[red, green, blue, alpha]
}

//==============================================================================
// 채널 → 색 문자열. ("#rrggbbaa", 알파가 없으면 1)
//==============================================================================
pub fn composeTimelineColor(channels: &[f32]) -> String {
	let toByte = |value: f32| -> u32 {
		let clampedValue = value.clamp(0.0, 1.0);
		(clampedValue * 255.0 + 0.5).floor() as u32
	};
	let readChannel = |channelIndex: usize, defaultValue: f32| channels.get(channelIndex).copied().unwrap_or(defaultValue);
	format!("#{:02x}{:02x}{:02x}{:02x}", toByte(readChannel(0, 0.0)), toByte(readChannel(1, 0.0)), toByte(readChannel(2, 0.0)), toByte(readChannel(3, 1.0)))
}

//==============================================================================
// 색 문자열 → Color.
//==============================================================================
fn createColorFromText(hexText: &str) -> Color {
	let channels = parseTimelineColor(hexText);
	Color::create(channels[0], channels[1], channels[2], channels[3])
}

//==============================================================================
// 원본 Math.round. (절반은 위로)
//==============================================================================
fn roundHalfUp(value: f32) -> f32 {
	(value + 0.5).floor()
}

//==============================================================================
// 숫자 → 글. (원본 String(숫자) 처럼 정수는 소수점 없이, -0 은 0)
//==============================================================================
fn formatNumber(value: f32) -> String {
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
fn parseNumberText(text: &str) -> f32 {
	let trimmedText = text.trim();
	if trimmedText.is_empty() {
		return 0.0;
	}
	trimmedText.parse::<f32>().unwrap_or(f32::NAN)
}

//==============================================================================
// JSON 값 → 숫자. (원본 Number(값). null 은 0, 없는 값과 객체는 NaN)
//==============================================================================
fn readJsonNumber(value: Option<&Value>) -> f32 {
	match value {
		Some(Value::Number(number)) => {
			number.as_f64().unwrap_or(f64::NAN) as f32
		}
		Some(Value::String(text)) => {
			parseNumberText(text)
		}
		Some(Value::Bool(isTrue)) => {
			if *isTrue {
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
	none,
	number(f32),
	string(String),
	boolean(bool),
}

impl TimelineValue {
	//==============================================================================
	// JSON 값에서 만들기. (배열, 객체는 none)
	//==============================================================================
	pub fn fromJson(value: &Value) -> TimelineValue {
		match value {
			Value::Number(number) => {
				TimelineValue::number(number.as_f64().unwrap_or(0.0) as f32)
			}
			Value::String(text) => {
				TimelineValue::string(text.clone())
			}
			Value::Bool(isTrue) => {
				TimelineValue::boolean(*isTrue)
			}
			_ => {
				TimelineValue::none
			}
		}
	}

	//==============================================================================
	// JSON 값으로 바꾸기.
	//==============================================================================
	pub fn toJson(&self) -> Value {
		match self {
			TimelineValue::none => {
				Value::Null
			}
			TimelineValue::number(number) => {
				json!(*number)
			}
			TimelineValue::string(text) => {
				Value::String(text.clone())
			}
			TimelineValue::boolean(isTrue) => {
				Value::Bool(*isTrue)
			}
		}
	}

	//==============================================================================
	// 숫자로 읽기. (원본 Number(값). 문자열은 읽고, 참거짓은 1 / 0, 없음은 0)
	//==============================================================================
	pub fn asNumber(&self) -> f32 {
		match self {
			TimelineValue::none => {
				0.0
			}
			TimelineValue::number(number) => {
				*number
			}
			TimelineValue::string(text) => {
				parseNumberText(text)
			}
			TimelineValue::boolean(isTrue) => {
				if *isTrue {
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
	pub fn toText(&self) -> String {
		match self {
			TimelineValue::none => {
				String::from("null")
			}
			TimelineValue::number(number) => {
				formatNumber(*number)
			}
			TimelineValue::string(text) => {
				text.clone()
			}
			TimelineValue::boolean(isTrue) => {
				isTrue.to_string()
			}
		}
	}

	//==============================================================================
	// 참으로 볼지. (원본 visible 속성: true 또는 "true")
	//==============================================================================
	pub fn isTrue(&self) -> bool {
		match self {
			TimelineValue::boolean(isTrue) => {
				*isTrue
			}
			TimelineValue::string(text) => {
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
	number,
	color,
	string,
	boolean,
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
fn applyPositionX(node: &mut WorldNode, value: &TimelineValue) {
	let localPosition = node.getLocalPosition();
	node.setLocalPosition(Vector2::create(value.asNumber(), localPosition.y));
}

//==============================================================================
// y 적용.
//==============================================================================
fn applyPositionY(node: &mut WorldNode, value: &TimelineValue) {
	let localPosition = node.getLocalPosition();
	node.setLocalPosition(Vector2::create(localPosition.x, value.asNumber()));
}

//==============================================================================
// width 적용.
//==============================================================================
fn applyWidth(node: &mut WorldNode, value: &TimelineValue) {
	let contentSize = node.getContentSize();
	node.setContentSize(Vector2::create(value.asNumber(), contentSize.y));
}

//==============================================================================
// height 적용.
//==============================================================================
fn applyHeight(node: &mut WorldNode, value: &TimelineValue) {
	let contentSize = node.getContentSize();
	node.setContentSize(Vector2::create(contentSize.x, value.asNumber()));
}

//==============================================================================
// scaleX 적용.
//==============================================================================
fn applyScaleX(node: &mut WorldNode, value: &TimelineValue) {
	let localScale = node.getLocalScale();
	node.setLocalScale(Vector2::create(value.asNumber(), localScale.y));
}

//==============================================================================
// scaleY 적용.
//==============================================================================
fn applyScaleY(node: &mut WorldNode, value: &TimelineValue) {
	let localScale = node.getLocalScale();
	node.setLocalScale(Vector2::create(localScale.x, value.asNumber()));
}

//==============================================================================
// rotation 적용.
//==============================================================================
fn applyRotation(node: &mut WorldNode, value: &TimelineValue) {
	node.setLocalRotation(value.asNumber());
}

//==============================================================================
// opacity 적용. (0 ~ 1 로 자른다)
//==============================================================================
fn applyOpacity(node: &mut WorldNode, value: &TimelineValue) {
	node.setLocalOpacity(value.asNumber().clamp(0.0, 1.0));
}

//==============================================================================
// visible 적용. (true 또는 "true" 면 켠다)
//==============================================================================
fn applyVisible(node: &mut WorldNode, value: &TimelineValue) {
	node.setActive(value.isTrue());
}

//==============================================================================
// color 적용.
//==============================================================================
fn applyColor(node: &mut WorldNode, value: &TimelineValue) {
	applyNodeColor(node, &value.toText());
}

//==============================================================================
// text 적용.
//==============================================================================
fn applyText(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(textComponent) = node.getComponentMut::<Text>() {
		textComponent.setText(&value.toText());
	}
}

//==============================================================================
// number 적용. (반올림한 수를 글자로)
//==============================================================================
fn applyNumber(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(textComponent) = node.getComponentMut::<Text>() {
		textComponent.setText(&formatNumber(roundHalfUp(value.asNumber())));
	}
}

//==============================================================================
// fontSize 적용.
// - 글자 텍스처는 (문자열, 폰트) 조합마다 굽히므로 정수 픽셀로 맞춰 매 프레임 재굽기를 막는다.
//==============================================================================
fn applyFontSize(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(textComponent) = node.getComponentMut::<Text>() {
		textComponent.setFontSize(roundHalfUp(value.asNumber()));
	}
}

//==============================================================================
// visibleCharacters 적용.
//==============================================================================
fn applyVisibleCharacters(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(textComponent) = node.getComponentMut::<Text>() {
		textComponent.setVisibleCharacterCount(roundHalfUp(value.asNumber()) as i32);
	}
}

//==============================================================================
// frame 적용. (스프라이트 시트 칸)
//==============================================================================
fn applyFrame(node: &mut WorldNode, value: &TimelineValue) {
	applySpriteFrame(node, roundHalfUp(value.asNumber()) as i64);
}

//==============================================================================
// effect 적용. (ShaderSprite 효과 진행률)
//==============================================================================
fn applyEffectProgress(node: &mut WorldNode, value: &TimelineValue) {
	if let Some(shaderSprite) = node.getComponentMut::<ShaderSprite>() {
		shaderSprite.setEffectProgress(value.asNumber());
	}
}

//==============================================================================
// 애니메이션 속성 정의 목록. (원본 TIMELINE_PROPERTY_DEFINITIONS 의 선언 차례)
//==============================================================================
pub const TIMELINE_PROPERTY_DEFINITIONS: [TimelinePropertyDefinition; 17] = [
	TimelinePropertyDefinition { name: "x", kind: TimelinePropertyKind::number, label: "X", apply: Some(applyPositionX) },
	TimelinePropertyDefinition { name: "y", kind: TimelinePropertyKind::number, label: "Y", apply: Some(applyPositionY) },
	TimelinePropertyDefinition { name: "width", kind: TimelinePropertyKind::number, label: "Width", apply: Some(applyWidth) },
	TimelinePropertyDefinition { name: "height", kind: TimelinePropertyKind::number, label: "Height", apply: Some(applyHeight) },
	TimelinePropertyDefinition { name: "scaleX", kind: TimelinePropertyKind::number, label: "Scale X", apply: Some(applyScaleX) },
	TimelinePropertyDefinition { name: "scaleY", kind: TimelinePropertyKind::number, label: "Scale Y", apply: Some(applyScaleY) },
	TimelinePropertyDefinition { name: "rotation", kind: TimelinePropertyKind::number, label: "Rotation", apply: Some(applyRotation) },
	TimelinePropertyDefinition { name: "opacity", kind: TimelinePropertyKind::number, label: "Opacity", apply: Some(applyOpacity) },
	TimelinePropertyDefinition { name: "visible", kind: TimelinePropertyKind::boolean, label: "Visible", apply: Some(applyVisible) },
	TimelinePropertyDefinition { name: "color", kind: TimelinePropertyKind::color, label: "Color", apply: Some(applyColor) },
	TimelinePropertyDefinition { name: "text", kind: TimelinePropertyKind::string, label: "Text", apply: Some(applyText) },
	TimelinePropertyDefinition { name: "number", kind: TimelinePropertyKind::number, label: "Number", apply: Some(applyNumber) },
	TimelinePropertyDefinition { name: "fontSize", kind: TimelinePropertyKind::number, label: "Font Size", apply: Some(applyFontSize) },
	TimelinePropertyDefinition { name: "visibleCharacters", kind: TimelinePropertyKind::number, label: "Visible Characters", apply: Some(applyVisibleCharacters) },
	TimelinePropertyDefinition { name: "frame", kind: TimelinePropertyKind::number, label: "Frame", apply: Some(applyFrame) },
	TimelinePropertyDefinition { name: "effect", kind: TimelinePropertyKind::number, label: "Effect Progress", apply: Some(applyEffectProgress) },
	TimelinePropertyDefinition { name: "event", kind: TimelinePropertyKind::string, label: "Event", apply: None },
];

//==============================================================================
// 속성 정의 찾기. (원본 TIMELINE_PROPERTY_DEFINITIONS[이름])
//==============================================================================
pub fn findTimelinePropertyDefinition(propertyName: &str) -> Option<&'static TimelinePropertyDefinition> {
	TIMELINE_PROPERTY_DEFINITIONS.iter().find(|definition| definition.name == propertyName)
}

//==============================================================================
// 노드에서 색을 받는 컴포넌트에 색 걸기. (Text → 글자색, 그 밖에 Paint / Sprite / ShaderSprite → 색)
// - 원본은 Sprite, ShaderSprite 가 Paint 를 상속해서 getComponent(Paint) 하나로 찾는다.
//==============================================================================
fn applyNodeColor(node: &mut WorldNode, hexText: &str) {
	let color = createColorFromText(hexText);
	if let Some(textComponent) = node.getComponentMut::<Text>() {
		textComponent.setTextColor(color);
		return;
	}
	if let Some(paintComponent) = node.getComponentMut::<Paint>() {
		paintComponent.setColor(color);
		return;
	}
	if let Some(sprite) = node.getComponentMut::<Sprite>() {
		sprite.setColor(color);
		return;
	}
	if let Some(shaderSprite) = node.getComponentMut::<ShaderSprite>() {
		shaderSprite.getSpriteMut().setColor(color);
	}
}

//==============================================================================
// 스프라이트 시트 프레임 적용. (격자로 잘라 imageRect 를 고른다. 격자가 1 x 1 이거나 이미지가 없으면 그대로)
//==============================================================================
fn applySpriteFrame(node: &mut WorldNode, frameIndex: i64) {
	let (gridColumns, gridRows) = match node.getComponent::<TimelineSpriteFrameGrid>() {
		Some(frameGrid) => {
			(frameGrid.getColumns(), frameGrid.getRows())
		}
		None => {
			(1, 1)
		}
	};
	let shaderSprite = match node.getComponentMut::<ShaderSprite>() {
		Some(shaderSprite) => {
			shaderSprite
		}
		None => {
			return;
		}
	};
	let image = match shaderSprite.getSprite().getImage() {
		Some(image) => {
			image.clone()
		}
		None => {
			return;
		}
	};
	if gridColumns <= 1 && gridRows <= 1 {
		return;
	}
	let frameCount = (gridColumns * gridRows) as i64;
	let wrappedIndex = frameIndex.rem_euclid(frameCount) as usize;
	let frameWidth = image.getWidth() as f32 / gridColumns as f32;
	let frameHeight = image.getHeight() as f32 / gridRows as f32;
	let column = wrappedIndex % gridColumns;
	let row = wrappedIndex / gridColumns;
	shaderSprite.getSpriteMut().setImageRect(Rect::create(column as f32 * frameWidth, row as f32 * frameHeight, frameWidth, frameHeight));
}

//==============================================================================
// 값 보간. (종류별. number 는 선형, color 는 채널별, 그 밖에는 앞 값)
//==============================================================================
fn interpolateValue(kind: TimelinePropertyKind, fromValue: &TimelineValue, toValue: &TimelineValue, easedProgress: f32) -> TimelineValue {
	match kind {
		TimelinePropertyKind::number => {
			let fromNumber = fromValue.asNumber();
			let toNumber = toValue.asNumber();
			TimelineValue::number(fromNumber + (toNumber - fromNumber) * easedProgress)
		}
		TimelinePropertyKind::color => {
			let fromChannels = parseTimelineColor(&colorTextOf(fromValue));
			let toChannels = parseTimelineColor(&colorTextOf(toValue));
			let mixedChannels: Vec<f32> = fromChannels.iter().zip(toChannels.iter()).map(|(fromChannel, toChannel)| fromChannel + (toChannel - fromChannel) * easedProgress).collect();
			TimelineValue::string(composeTimelineColor(&mixedChannels))
		}
		_ => {
			fromValue.clone()
		}
	}
}

//==============================================================================
// 색 값의 글. (문자열이 아니면 원본처럼 흰색으로 읽히게 빈 글)
//==============================================================================
fn colorTextOf(value: &TimelineValue) -> String {
	match value {
		TimelineValue::string(text) => {
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
	pub fn fromJson(keyDescription: &Value) -> TimelineKey {
		let time = keyDescription.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
		let value = keyDescription.get("value").map(TimelineValue::fromJson).unwrap_or(TimelineValue::none);
		let easing = keyDescription.get("easing").and_then(|value| value.as_str()).unwrap_or("").to_string();
		let curve = keyDescription.get("curve").and_then(|value| value.as_array()).map(|curveArray| {
			curveArray.iter().map(|point| point.as_f64().unwrap_or(0.0) as f32).collect()
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
	pub fn resolveEasing(&self) -> TimelineEasing {
		resolveTimelineEasing(&self.easing, self.curve.as_deref())
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
	node(NodeId),
	// 노드가 아닌 것. (값을 모아 두었다가 applyExternalProperties 로 넘긴다. 원본 setTimelineProperty 를 가진 객체)
	external,
}

//==============================================================================
// 노드가 아닌 대상이 값을 받는 트레이트. (원본 setTimelineProperty(이름, 값))
//==============================================================================
pub trait TimelinePropertyTarget {
	//==============================================================================
	// 속성 값 받기.
	//==============================================================================
	fn setTimelineProperty(&mut self, propertyName: &str, value: &TimelineValue);
}

impl TimelinePropertyTarget for ScreenEffect {
	//==============================================================================
	// 화면 효과에 값 걸기. (ScreenEffect 의 setTimelineProperty 는 값을 글로 받는다)
	//==============================================================================
	fn setTimelineProperty(&mut self, propertyName: &str, value: &TimelineValue) {
		ScreenEffect::setTimelineProperty(self, propertyName, &value.toText());
	}
}

//==============================================================================
// 모아 둔 external 값 하나.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineExternalProperty {
	pub targetName: String,
	pub propertyName: String,
	pub value: TimelineValue,
}

//==============================================================================
// 서술 노드. (원본 무대 노드 서술. TIMELINE_NODE_DEFAULTS 가 기본값)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineNodeDescription {
	pub name: String,
	// 원본 키 type. ("group", "paint", "sprite", "text", "particle", "sound")
	pub nodeType: String,
	pub parent: Option<String>,
	pub x: f32,
	pub y: f32,
	pub width: f32,
	pub height: f32,
	pub scaleX: f32,
	pub scaleY: f32,
	pub rotation: f32,
	pub opacity: f32,
	pub pivotX: f32,
	pub pivotY: f32,
	pub visible: bool,
	pub color: String,
	pub roundSize: f32,
	pub image: String,
	pub frameColumns: f32,
	pub frameRows: f32,
	pub frame: f32,
	pub effect: String,
	pub effectProgress: f32,
	pub effectColor: String,
	pub blendMode: String,
	pub text: String,
	pub fontSize: f32,
	pub bold: bool,
	pub textAlign: String,
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
			nodeType: String::from("group"),
			parent: None,
			x: 0.0,
			y: 0.0,
			width: 100.0,
			height: 100.0,
			scaleX: 1.0,
			scaleY: 1.0,
			rotation: 0.0,
			opacity: 1.0,
			pivotX: 0.5,
			pivotY: 0.5,
			visible: true,
			color: String::from("#ffffffff"),
			roundSize: 0.0,
			image: String::new(),
			frameColumns: 1.0,
			frameRows: 1.0,
			frame: 0.0,
			effect: String::from("none"),
			effectProgress: 0.0,
			effectColor: String::new(),
			blendMode: String::from("source-over"),
			text: String::from("Text"),
			fontSize: 32.0,
			bold: false,
			textAlign: String::from("center"),
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
	pub fn fromJson(nodeDescription: &Value) -> TimelineNodeDescription {
		let defaults = TimelineNodeDescription::default();
		let readNumber = |name: &str, defaultValue: f32| nodeDescription.get(name).and_then(|value| value.as_f64()).map(|value| value as f32).unwrap_or(defaultValue);
		let readText = |name: &str, defaultValue: &str| nodeDescription.get(name).and_then(|value| value.as_str()).unwrap_or(defaultValue).to_string();
		TimelineNodeDescription {
			name: readText("name", &defaults.name),
			nodeType: readText("type", &defaults.nodeType),
			parent: nodeDescription.get("parent").and_then(|value| value.as_str()).map(|parentName| parentName.to_string()),
			x: readNumber("x", defaults.x),
			y: readNumber("y", defaults.y),
			width: readNumber("width", defaults.width),
			height: readNumber("height", defaults.height),
			scaleX: readNumber("scaleX", defaults.scaleX),
			scaleY: readNumber("scaleY", defaults.scaleY),
			rotation: readNumber("rotation", defaults.rotation),
			opacity: readNumber("opacity", defaults.opacity),
			pivotX: readNumber("pivotX", defaults.pivotX),
			pivotY: readNumber("pivotY", defaults.pivotY),
			visible: nodeDescription.get("visible") != Some(&Value::Bool(false)),
			color: readText("color", &defaults.color),
			roundSize: readNumber("roundSize", defaults.roundSize),
			image: readText("image", &defaults.image),
			frameColumns: readNumber("frameColumns", defaults.frameColumns),
			frameRows: readNumber("frameRows", defaults.frameRows),
			frame: readNumber("frame", defaults.frame),
			effect: readText("effect", &defaults.effect),
			effectProgress: readNumber("effectProgress", defaults.effectProgress),
			effectColor: readText("effectColor", &defaults.effectColor),
			blendMode: readText("blendMode", &defaults.blendMode),
			text: readText("text", &defaults.text),
			fontSize: readNumber("fontSize", defaults.fontSize),
			bold: nodeDescription.get("bold") == Some(&Value::Bool(true)),
			textAlign: readText("textAlign", &defaults.textAlign),
			particle: nodeDescription.get("particle").filter(|value| !value.is_null()).cloned(),
			audio: readText("audio", &defaults.audio),
			volume: readNumber("volume", defaults.volume),
		}
	}
}

//==============================================================================
// 서술 노드 기본값. (원본 TIMELINE_NODE_DEFAULTS)
//==============================================================================
pub static TIMELINE_NODE_DEFAULTS: LazyLock<TimelineNodeDescription> = LazyLock::new(TimelineNodeDescription::default);

//==============================================================================
// 스프라이트 시트 격자. (노드 서술의 frameColumns / frameRows. 원본 spriteFrameGridTable 자리)
// - 그리지도 갱신하지도 않는 표시용 컴포넌트다. frame 속성이 이것을 읽어 칸을 고른다.
//==============================================================================
pub struct TimelineSpriteFrameGrid {
	componentBase: ComponentBase,
	columns: usize,
	rows: usize,
}

impl TimelineSpriteFrameGrid {
	//==============================================================================
	// 생성. (칸 수는 1 이상)
	//==============================================================================
	pub fn new(columns: usize, rows: usize) -> TimelineSpriteFrameGrid {
		TimelineSpriteFrameGrid {
			componentBase: ComponentBase::new(),
			columns: columns.max(1),
			rows: rows.max(1),
		}
	}

	//==============================================================================
	// 격자 설정.
	//==============================================================================
	pub fn setGrid(&mut self, columns: usize, rows: usize) {
		self.columns = columns.max(1);
		self.rows = rows.max(1);
	}

	//==============================================================================
	// 가로 칸 수.
	//==============================================================================
	pub fn getColumns(&self) -> usize {
		self.columns
	}

	//==============================================================================
	// 세로 칸 수.
	//==============================================================================
	pub fn getRows(&self) -> usize {
		self.rows
	}
}

impl Component for TimelineSpriteFrameGrid {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"TimelineSpriteFrameGrid"
	}

	//==============================================================================
	// 갱신할 것이 없다.
	//==============================================================================
	fn isTickNeeded(&self) -> bool {
		false
	}

	//==============================================================================
	// 아무것도 그리지 않으니 노드 거르기를 막지 않는다.
	//==============================================================================
	fn isDrawnInsideContent(&self) -> bool {
		true
	}
}

//==============================================================================
// 사운드 노드 상태. (원본 soundPlayerTable 의 { audioPlayer, audioAsset, audioSource, volume } 자리)
// - 애셋과 플레이어는 Timeline::loadSounds 가 채운다. 이벤트 키 play / loop / stop 이 플레이어를 부린다.
//==============================================================================
pub struct TimelineSound {
	componentBase: ComponentBase,
	audioPlayer: Option<AudioPlayer>,
	audioAsset: Option<Rc<AudioAsset>>,
	audioSource: String,
	volume: f32,
}

impl TimelineSound {
	//==============================================================================
	// 생성. (비어 있음)
	//==============================================================================
	pub fn new() -> TimelineSound {
		TimelineSound {
			componentBase: ComponentBase::new(),
			audioPlayer: None,
			audioAsset: None,
			audioSource: String::new(),
			volume: 1.0,
		}
	}

	//==============================================================================
	// 소리 주소 반환.
	//==============================================================================
	pub fn getAudioSource(&self) -> &str {
		&self.audioSource
	}

	//==============================================================================
	// 음량 반환. (원본처럼 서술 값을 들고만 있다)
	//==============================================================================
	pub fn getVolume(&self) -> f32 {
		self.volume
	}

	//==============================================================================
	// 오디오 애셋 반환.
	//==============================================================================
	pub fn getAudioAsset(&self) -> Option<&Rc<AudioAsset>> {
		self.audioAsset.as_ref()
	}

	//==============================================================================
	// 플레이어 반환.
	//==============================================================================
	pub fn getAudioPlayerMut(&mut self) -> Option<&mut AudioPlayer> {
		self.audioPlayer.as_mut()
	}

	//==============================================================================
	// 멈춤. (플레이어가 있으면)
	//==============================================================================
	pub fn stop(&mut self) {
		if let Some(audioPlayer) = self.audioPlayer.as_mut() {
			audioPlayer.stop();
		}
	}

	//==============================================================================
	// 이벤트 처리. (play / loop 는 애셋이 읽혔으면 처음부터 튼다, stop 은 멈춘다)
	//==============================================================================
	fn handleEvent(&mut self, eventName: &str) {
		let audioPlayer = match self.audioPlayer.as_mut() {
			Some(audioPlayer) => {
				audioPlayer
			}
			None => {
				return;
			}
		};
		if eventName == "play" || eventName == "loop" {
			if let Some(audioAsset) = self.audioAsset.as_ref() {
				if audioAsset.isLoaded() {
					audioPlayer.setAudioAsset(audioAsset.clone());
					audioPlayer.setTime(0.0);
					audioPlayer.play(eventName == "loop");
				}
			}
		}
		else if eventName == "stop" {
			audioPlayer.stop();
		}
	}
}

impl Default for TimelineSound {
	fn default() -> TimelineSound {
		TimelineSound::new()
	}
}

impl Component for TimelineSound {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"TimelineSound"
	}

	//==============================================================================
	// 갱신할 것이 없다.
	//==============================================================================
	fn isTickNeeded(&self) -> bool {
		false
	}
}

//==============================================================================
// 노드의 T 컴포넌트를 찾고 없으면 붙인다. (원본 getOrAddComponent)
//==============================================================================
fn getOrAddComponent<T: Component + 'static>(node: &mut WorldNode, create: impl FnOnce() -> T) -> &mut T {
	if !node.hasComponent::<T>() {
		node.addComponent(create());
	}
	node.getComponentMut::<T>().expect("component just added")
}

//==============================================================================
// 가로 정렬 이름 읽기. (모르는 이름은 center)
//==============================================================================
fn readTextAlign(textAlignName: &str) -> TextAlign {
	match textAlignName {
		"left" => {
			TextAlign::left
		}
		"right" => {
			TextAlign::right
		}
		"start" => {
			TextAlign::start
		}
		"end" => {
			TextAlign::end
		}
		_ => {
			TextAlign::center
		}
	}
}

//==============================================================================
// base64 풀기. (표준 알파벳, = 채움과 공백은 건너뛴다. 틀린 글자가 있으면 None)
//==============================================================================
fn decodeBase64(text: &str) -> Option<Vec<u8>> {
	let mut bytes: Vec<u8> = Vec::with_capacity(text.len() * 3 / 4);
	let mut accumulator: u32 = 0;
	let mut bitCount = 0;
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
		bitCount += 6;
		if bitCount >= 8 {
			bitCount -= 8;
			bytes.push(((accumulator >> bitCount) & 0xff) as u8);
		}
	}
	Some(bytes)
}

//==============================================================================
// "data:" 주소 이미지 읽기. (";base64," 꼴만)
//==============================================================================
fn loadDataUrlImage(dataUrl: &str) -> Option<Rc<Image>> {
	let separatorIndex = dataUrl.find(',')?;
	let header = &dataUrl[..separatorIndex];
	if !header.ends_with(";base64") {
		return None;
	}
	let bytes = decodeBase64(&dataUrl[separatorIndex + 1..])?;
	Image::loadFromMemory(&bytes).ok()
}

//==============================================================================
// 컴파일한 트랙. (키를 시간순으로 정렬하고 대상 / 속성 정의를 붙인 것)
//==============================================================================
struct CompiledTimelineTrack {
	targetName: String,
	propertyName: String,
	isEnabled: bool,
	keys: Vec<TimelineKey>,
	definition: Option<&'static TimelinePropertyDefinition>,
	kind: TimelinePropertyKind,
	target: Option<TimelineTarget>,
}

//==============================================================================
// 키를 시간순으로 정렬. (같은 시간은 원래 차례를 지킨다)
//==============================================================================
fn sortKeysByTime(keys: &mut [TimelineKey]) {
	keys.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap_or(std::cmp::Ordering::Equal));
}

//==============================================================================
// 서술 트랙의 키 목록 읽기.
//==============================================================================
fn readTrackKeys(track: &Value) -> Vec<TimelineKey> {
	match track.get("keys").and_then(|value| value.as_array()) {
		Some(keyArray) => {
			keyArray.iter().map(TimelineKey::fromJson).collect()
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
	compiledTracks: Vec<CompiledTimelineTrack>,
	compiledMarkers: Vec<TimelineMarker>,
	targetTable: Vec<(String, TimelineTarget)>,
	targetResolver: Option<TimelineTargetResolver>,
	eventHandler: Option<TimelineEventHandler>,
	markerHandler: Option<TimelineMarkerHandler>,
	completeHandler: Option<TimelineCompleteHandler>,
	time: f32,
	speed: f32,
	isPlaying: bool,
	isDirty: bool,
	// 타임라인 시간으로 진행하는 파티클이 붙은 노드. (대상 노드에서 모은다)
	particleNodeIds: Vec<NodeId>,
	// 이벤트 트랙이 없는 파티클 노드. (타임라인이 시작하면 바로 도는 것으로 본다)
	particleFreeRunNodeIds: Vec<NodeId>,
	// external 대상에 넘길 값. (evaluate 마다 새로 채운다)
	externalPropertyList: Vec<TimelineExternalProperty>,
}

impl Timeline {
	//==============================================================================
	// 생성. (서술이 없으면 빈 서술)
	//==============================================================================
	pub fn new(description: Option<Value>) -> Timeline {
		let mut timeline = Timeline {
			description: Value::Null,
			compiledTracks: Vec::new(),
			compiledMarkers: Vec::new(),
			targetTable: Vec::new(),
			targetResolver: None,
			eventHandler: None,
			markerHandler: None,
			completeHandler: None,
			time: 0.0,
			speed: 1.0,
			isPlaying: false,
			isDirty: true,
			particleNodeIds: Vec::new(),
			particleFreeRunNodeIds: Vec::new(),
			externalPropertyList: Vec::new(),
		};
		timeline.setDescription(description.unwrap_or_else(Timeline::createEmptyDescription));
		timeline
	}

	//==============================================================================
	// 빈 서술 생성. (정적)
	//==============================================================================
	pub fn createEmptyDescription() -> Value {
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
	pub fn loadFromText(text: &str) -> Result<Timeline, String> {
		let description: Value = serde_json::from_str(text).map_err(|error| format!("Timeline: JSON 을 읽지 못했습니다. ({})", error))?;
		Ok(Timeline::new(Some(description)))
	}

	//==============================================================================
	// 읽은 JsonAsset 에서 만들기. (아직 안 읽혔거나 실패했으면 None)
	//==============================================================================
	pub fn createFromJsonAsset(jsonAsset: &mut JsonAsset) -> Option<Timeline> {
		let description = jsonAsset.getData()?.clone();
		Some(Timeline::new(Some(description)))
	}

	//==============================================================================
	// 서술 설정. (빠진 stage / nodes / tracks / markers 를 채우고 다시 컴파일하게 한다)
	//==============================================================================
	pub fn setDescription(&mut self, description: Value) {
		self.description = if description.is_object() {
			description
		}
		else {
			Timeline::createEmptyDescription()
		};
		let descriptionObject = self.description.as_object_mut().expect("description object");
		if !descriptionObject.get("stage").is_some_and(|stage| stage.is_object()) {
			descriptionObject.insert(String::from("stage"), json!({ "width": 960, "height": 540, "backgroundColor": "#101114ff", "nodes": [] }));
		}
		if let Some(stageObject) = descriptionObject.get_mut("stage").and_then(|stage| stage.as_object_mut()) {
			if !stageObject.get("nodes").is_some_and(|nodes| nodes.is_array()) {
				stageObject.insert(String::from("nodes"), json!([]));
			}
		}
		if !descriptionObject.get("tracks").is_some_and(|tracks| tracks.is_array()) {
			descriptionObject.insert(String::from("tracks"), json!([]));
		}
		if !descriptionObject.get("markers").is_some_and(|markers| markers.is_array()) {
			descriptionObject.insert(String::from("markers"), json!([]));
		}
		self.invalidate();
	}

	//==============================================================================
	// 서술 반환.
	//==============================================================================
	pub fn getDescription(&self) -> &Value {
		&self.description
	}

	//==============================================================================
	// 서술 반환. (변경용. 트랙 / 키를 고쳤으면 invalidate 를 부른다)
	//==============================================================================
	pub fn getDescriptionMut(&mut self) -> &mut Value {
		&mut self.description
	}

	//==============================================================================
	// 다시 컴파일 요청. (트랙 / 키가 바뀌었을 때)
	//==============================================================================
	pub fn invalidate(&mut self) {
		self.isDirty = true;
	}

	//==============================================================================
	// 서술 배열 반환. (stage.nodes 처럼 경로로)
	//==============================================================================
	fn getDescriptionArray(&self, pointer: &str) -> Vec<Value> {
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
	pub fn compile(&mut self, nodeTree: &mut NodeTree) {
		let trackDescriptions = self.getDescriptionArray("/tracks");
		let mut compiledTracks: Vec<CompiledTimelineTrack> = Vec::with_capacity(trackDescriptions.len());
		for track in trackDescriptions.iter() {
			let targetName = track.get("target").and_then(|value| value.as_str()).unwrap_or("").to_string();
			let propertyName = track.get("property").and_then(|value| value.as_str()).unwrap_or("").to_string();
			let definition = findTimelinePropertyDefinition(&propertyName);
			let mut keys = readTrackKeys(track);
			sortKeysByTime(&mut keys);
			let target = self.resolveTarget(&targetName);
			let kind = match definition {
				Some(definition) => {
					definition.kind
				}
				None => {
					match keys.first().map(|key| &key.value) {
						Some(TimelineValue::number(_)) | None => {
							TimelinePropertyKind::number
						}
						Some(_) => {
							TimelinePropertyKind::string
						}
					}
				}
			};
			compiledTracks.push(CompiledTimelineTrack {
				targetName,
				propertyName,
				isEnabled: track.get("enabled") != Some(&Value::Bool(false)),
				keys,
				definition,
				kind,
				target,
			});
		}
		self.compiledTracks = compiledTracks;

		let mut compiledMarkers: Vec<TimelineMarker> = self.getDescriptionArray("/markers").iter().map(|marker| {
			TimelineMarker {
				time: marker.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32,
				name: marker.get("name").and_then(|value| value.as_str()).unwrap_or("").to_string(),
			}
		}).collect();
		compiledMarkers.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap_or(std::cmp::Ordering::Equal));
		self.compiledMarkers = compiledMarkers;
		self.isDirty = false;
		self.collectParticleSystems(nodeTree);
	}

	//==============================================================================
	// 더러우면 컴파일.
	//==============================================================================
	fn compileIfDirty(&mut self, nodeTree: &mut NodeTree) {
		if self.isDirty {
			self.compile(nodeTree);
		}
	}

	//==============================================================================
	// 대상 노드의 파티클 시스템 수집.
	// - 수동 tick 으로 바꿔 타임라인 시간으로만 진행하고, 씨앗을 줘서 되감을 때마다 같은 결과가 나오게 한다.
	//==============================================================================
	pub fn collectParticleSystems(&mut self, nodeTree: &mut NodeTree) {
		self.particleNodeIds.clear();
		self.particleFreeRunNodeIds.clear();
		let mut eventTargetIds: Vec<NodeId> = Vec::new();
		for compiledTrack in self.compiledTracks.iter() {
			if compiledTrack.propertyName == "event" {
				if let Some(TimelineTarget::node(nodeId)) = compiledTrack.target {
					eventTargetIds.push(nodeId);
				}
			}
		}
		for (_, target) in self.targetTable.iter() {
			let nodeId = match target {
				TimelineTarget::node(nodeId) => {
					*nodeId
				}
				TimelineTarget::external => {
					continue;
				}
			};
			if self.particleNodeIds.contains(&nodeId) {
				continue;
			}
			let particleSystem = match nodeTree.getNodeMut(nodeId).and_then(|node| node.getComponentMut::<ParticleSystem>()) {
				Some(particleSystem) => {
					particleSystem
				}
				None => {
					continue;
				}
			};
			particleSystem.setManualTick(true);
			if particleSystem.getRandomSeed().is_none() {
				let randomSeed = PARTICLE_RANDOM_SEED_BASE.wrapping_add((self.particleNodeIds.len() as u32).wrapping_mul(PARTICLE_RANDOM_SEED_STEP));
				particleSystem.setRandomSeed(randomSeed);
			}
			if !eventTargetIds.contains(&nodeId) {
				self.particleFreeRunNodeIds.push(nodeId);
			}
			self.particleNodeIds.push(nodeId);
		}
	}

	//==============================================================================
	// 파티클 시간 진행. (재생 중 매 프레임. 방출점은 노드 contentSize 가운데)
	//==============================================================================
	pub fn simulateParticles(&mut self, nodeTree: &mut NodeTree, timeDelta: f32) {
		if timeDelta <= 0.0 {
			return;
		}
		for nodeId in self.particleNodeIds.iter() {
			if let Some(node) = nodeTree.getNodeMut(*nodeId) {
				let emitterCenter = node.getContentSize().multiplyScalar(0.5);
				if let Some(particleSystem) = node.getComponentMut::<ParticleSystem>() {
					particleSystem.simulate(timeDelta, emitterCenter);
				}
			}
		}
	}

	//==============================================================================
	// 파티클 재시뮬레이션. (스크럽 / 되감기. 비운 뒤 0 초부터 이벤트를 다시 밟으며 진행)
	//==============================================================================
	pub fn resimulateParticles(&mut self, nodeTree: &mut NodeTree, time: f32) {
		self.compileIfDirty(nodeTree);
		if self.particleNodeIds.is_empty() {
			return;
		}
		for nodeId in self.particleNodeIds.iter() {
			if let Some(particleSystem) = nodeTree.getNodeMut(*nodeId).and_then(|node| node.getComponentMut::<ParticleSystem>()) {
				particleSystem.stop(true);
			}
		}
		for nodeId in self.particleFreeRunNodeIds.iter() {
			if let Some(particleSystem) = nodeTree.getNodeMut(*nodeId).and_then(|node| node.getComponentMut::<ParticleSystem>()) {
				particleSystem.play();
			}
		}

		// 파티클 대상 이벤트 키를 시간순으로 모은다.
		let mut eventList: Vec<(f32, usize, usize)> = Vec::new();
		for (trackIndex, compiledTrack) in self.compiledTracks.iter().enumerate() {
			if compiledTrack.propertyName != "event" || !compiledTrack.isEnabled {
				continue;
			}
			let nodeId = match compiledTrack.target {
				Some(TimelineTarget::node(nodeId)) => {
					nodeId
				}
				_ => {
					continue;
				}
			};
			if !nodeTree.getNode(nodeId).is_some_and(|node| node.hasComponent::<ParticleSystem>()) {
				continue;
			}
			for (keyIndex, key) in compiledTrack.keys.iter().enumerate() {
				if key.time <= time {
					eventList.push((key.time, trackIndex, keyIndex));
				}
			}
		}
		eventList.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(std::cmp::Ordering::Equal));

		let stepSeconds = (1.0f32 / 60.0).max(time / 600.0);
		let mut currentTime = 0.0f32;
		for (eventTime, trackIndex, keyIndex) in eventList {
			self.advanceParticlesTo(nodeTree, &mut currentTime, eventTime, stepSeconds);
			self.dispatchEvent(nodeTree, trackIndex, keyIndex, true);
		}
		self.advanceParticlesTo(nodeTree, &mut currentTime, time, stepSeconds);
	}

	//==============================================================================
	// 파티클을 stepSeconds 씩 targetTime 까지 진행. (재시뮬레이션용)
	//==============================================================================
	fn advanceParticlesTo(&mut self, nodeTree: &mut NodeTree, currentTime: &mut f32, targetTime: f32, stepSeconds: f32) {
		while *currentTime < targetTime - 0.000001 {
			let stepDelta = stepSeconds.min(targetTime - *currentTime);
			self.simulateParticles(nodeTree, stepDelta);
			*currentTime += stepDelta;
		}
	}

	//==============================================================================
	// 대상 해석. (대상 표 → 사용자 해석기)
	//==============================================================================
	pub fn resolveTarget(&self, targetName: &str) -> Option<TimelineTarget> {
		if let Some(target) = self.getTarget(targetName) {
			return Some(target);
		}
		if let Some(targetResolver) = self.targetResolver.as_ref() {
			if let Some(resolvedTarget) = targetResolver(targetName) {
				return Some(resolvedTarget);
			}
		}
		None
	}

	//==============================================================================
	// 대상 해석기 설정. (이름 → 대상. 노드가 아닌 것은 TimelineTarget::external)
	//==============================================================================
	pub fn setTargetResolver(&mut self, targetResolver: Option<TimelineTargetResolver>) {
		self.targetResolver = targetResolver;
		self.invalidate();
	}

	//==============================================================================
	// 대상 직접 등록. (같은 이름이 있으면 바꾼다)
	//==============================================================================
	pub fn setTarget(&mut self, targetName: &str, target: TimelineTarget) {
		match self.targetTable.iter_mut().find(|(name, _)| name == targetName) {
			Some(entry) => {
				entry.1 = target;
			}
			None => {
				self.targetTable.push((targetName.to_string(), target));
			}
		}
		self.invalidate();
	}

	//==============================================================================
	// 대상 반환. (대상 표에 있는 것만)
	//==============================================================================
	pub fn getTarget(&self, targetName: &str) -> Option<TimelineTarget> {
		self.targetTable.iter().find(|(name, _)| name == targetName).map(|(_, target)| *target)
	}

	//==============================================================================
	// 노드 트리에 이름으로 붙이기. (무대를 직접 만든 경우. 뿌리 자신, 또는 그 자손에서 트랙 대상 이름을 찾는다)
	//==============================================================================
	pub fn bind(&mut self, nodeTree: &NodeTree, rootId: NodeId) {
		let trackDescriptions = self.getDescriptionArray("/tracks");
		for track in trackDescriptions.iter() {
			let targetName = track.get("target").and_then(|value| value.as_str()).unwrap_or("");
			if self.getTarget(targetName).is_some() {
				continue;
			}
			let isRootMatched = nodeTree.getNode(rootId).is_some_and(|rootNode| rootNode.getName() == targetName);
			let foundId = if isRootMatched {
				Some(rootId)
			}
			else {
				nodeTree.findDescendantByName(rootId, targetName)
			};
			if let Some(foundId) = foundId {
				self.targetTable.push((targetName.to_string(), TimelineTarget::node(foundId)));
			}
		}
		self.invalidate();
	}

	//==============================================================================
	// 무대 노드 생성. (서술의 stage.nodes 를 노드 트리로 만들고 대상으로 등록한다. 이름 → 노드 표를 돌려준다)
	//==============================================================================
	pub fn buildStage(&mut self, nodeTree: &mut NodeTree, parentId: NodeId, imageResolver: Option<TimelineImageResolver>) -> HashMap<String, NodeId> {
		let nodeDescriptions = self.getDescriptionArray("/stage/nodes");
		let mut nodeTable: HashMap<String, NodeId> = HashMap::new();
		let mut createdList: Vec<(NodeId, Option<String>)> = Vec::with_capacity(nodeDescriptions.len());
		for nodeDescription in nodeDescriptions.iter() {
			let nodeId = Timeline::createStageNode(nodeTree, nodeDescription, imageResolver);
			let nodeName = nodeDescription.get("name").and_then(|value| value.as_str()).unwrap_or("").to_string();
			let parentName = nodeDescription.get("parent").and_then(|value| value.as_str()).map(|name| name.to_string());
			nodeTable.insert(nodeName.clone(), nodeId);
			self.setTarget(&nodeName, TimelineTarget::node(nodeId));
			createdList.push((nodeId, parentName));
		}
		for (nodeId, parentName) in createdList {
			let parentDescriptionNodeId = parentName.and_then(|parentName| nodeTable.get(&parentName).copied());
			match parentDescriptionNodeId {
				Some(parentDescriptionNodeId) => {
					nodeTree.addChild(parentDescriptionNodeId, nodeId);
				}
				None => {
					nodeTree.addChild(parentId, nodeId);
				}
			}
		}
		self.invalidate();
		nodeTable
	}

	//==============================================================================
	// 무대 노드 한 개 생성. (정적. 붙이지 않은 노드를 만든다)
	//==============================================================================
	pub fn createStageNode(nodeTree: &mut NodeTree, nodeDescription: &Value, imageResolver: Option<TimelineImageResolver>) -> NodeId {
		let nodeName = nodeDescription.get("name").and_then(|value| value.as_str()).unwrap_or("");
		let nodeId = nodeTree.createNode(nodeName);
		Timeline::applyStageNodeDescription(&mut nodeTree[nodeId], nodeDescription, imageResolver);
		nodeId
	}

	//==============================================================================
	// 노드에 서술 반영. (정적. 편집기가 값을 고칠 때도 쓴다)
	//==============================================================================
	pub fn applyStageNodeDescription(node: &mut WorldNode, nodeDescription: &Value, imageResolver: Option<TimelineImageResolver>) {
		let description = TimelineNodeDescription::fromJson(nodeDescription);
		node.setPivot(Vector2::create(description.pivotX, description.pivotY));
		node.setAnchor(Pivot::topLeft);
		node.setContentSize(Vector2::create(description.width, description.height));
		node.setLocalPosition(Vector2::create(description.x, description.y));
		node.setLocalScale(Vector2::create(description.scaleX, description.scaleY));
		node.setLocalRotation(description.rotation);
		node.setLocalOpacity(description.opacity);
		node.setActive(description.visible);

		if description.nodeType == "paint" {
			let paintComponent = getOrAddComponent(node, Paint::new);
			paintComponent.setColor(createColorFromText(&description.color));
			paintComponent.setRoundSize(description.roundSize);
		}
		else if description.nodeType == "sprite" {
			let gridColumns = description.frameColumns.max(1.0) as usize;
			let gridRows = description.frameRows.max(1.0) as usize;
			getOrAddComponent(node, || TimelineSpriteFrameGrid::new(gridColumns, gridRows)).setGrid(gridColumns, gridRows);
			let mut image: Option<Rc<Image>> = None;
			if !description.image.is_empty() {
				if let Some(imageResolver) = imageResolver {
					image = imageResolver(&description.image);
				}
			}
			if image.is_none() && description.image.starts_with("data:") {
				image = loadDataUrlImage(&description.image);
			}
			{
				let shaderSprite = getOrAddComponent(node, ShaderSprite::new);
				shaderSprite.getSpriteMut().setImage(image);
				shaderSprite.getSpriteMut().setImageRect(Rect::zero());
			}
			applySpriteFrame(node, roundHalfUp(description.frame) as i64);
			let shaderSprite = getOrAddComponent(node, ShaderSprite::new);
			let spriteColorText = if description.color == "#ffffffff" {
				"#ffffff00"
			}
			else {
				description.color.as_str()
			};
			shaderSprite.getSpriteMut().setColor(createColorFromText(spriteColorText));
			shaderSprite.getSpriteMut().setSpriteBlendMode(BlendMode::fromName(&description.blendMode).unwrap_or(BlendMode::normal));
			shaderSprite.setEffect(ShaderSpriteEffect::fromName(&description.effect).unwrap_or(ShaderSpriteEffect::none));
			shaderSprite.setEffectProgress(description.effectProgress);
			if !description.effectColor.is_empty() {
				shaderSprite.setEffectColor(createColorFromText(&description.effectColor));
			}
		}
		else if description.nodeType == "text" {
			let textComponent = getOrAddComponent(node, Text::new);
			textComponent.setText(&description.text);
			textComponent.setFontSize(description.fontSize);
			textComponent.setBold(description.bold);
			textComponent.setTextColor(createColorFromText(&description.color));
			textComponent.setTextAlign(readTextAlign(&description.textAlign));
			textComponent.setTextBaseline(TextBaseline::middle);
		}
		else if description.nodeType == "particle" {
			let particleSystem = getOrAddComponent(node, ParticleSystem::new);
			if let Some(particleDescription) = description.particle.as_ref() {
				particleSystem.applyDescription(particleDescription);
			}
		}
		else if description.nodeType == "sound" {
			let sound = getOrAddComponent(node, TimelineSound::new);
			sound.volume = description.volume;
			if !description.audio.is_empty() && description.audio != sound.audioSource {
				sound.audioSource = description.audio.clone();
				sound.audioAsset = None;
			}
		}
	}

	//==============================================================================
	// 사운드 노드 준비. (원본 setAudioManager + 사운드 노드의 애셋 읽기 자리)
	// - 대상 노드 가운데 TimelineSound 가 붙은 것마다 아직 없는 오디오 애셋을 읽고 플레이어를 만든다. 새로 읽은 애셋을 돌려준다.
	//==============================================================================
	pub fn loadSounds(&mut self, nodeTree: &mut NodeTree, engine: &Engine) -> Vec<Rc<AudioAsset>> {
		let mut loadedAudioAssets: Vec<Rc<AudioAsset>> = Vec::new();
		for (_, target) in self.targetTable.iter() {
			let nodeId = match target {
				TimelineTarget::node(nodeId) => {
					*nodeId
				}
				TimelineTarget::external => {
					continue;
				}
			};
			let sound = match nodeTree.getNodeMut(nodeId).and_then(|node| node.getComponentMut::<TimelineSound>()) {
				Some(sound) => {
					sound
				}
				None => {
					continue;
				}
			};
			if sound.audioAsset.is_none() && !sound.audioSource.is_empty() {
				match engine.loadAudioAsset(&sound.audioSource) {
					Ok(audioAsset) => {
						loadedAudioAssets.push(audioAsset.clone());
						sound.audioAsset = Some(audioAsset);
					}
					Err(error) => {
						eprintln!("Timeline: {}", error);
					}
				}
			}
			if sound.audioPlayer.is_none() {
				sound.audioPlayer = Some(engine.getAudioManager().createAudioPlayer());
			}
			if let (Some(audioPlayer), Some(audioAsset)) = (sound.audioPlayer.as_mut(), sound.audioAsset.as_ref()) {
				audioPlayer.setAudioAsset(audioAsset.clone());
			}
		}
		loadedAudioAssets
	}

	//==============================================================================
	// 사운드 노드 정지. (정적. 편집기가 정지 / 되감기 때 부른다)
	//==============================================================================
	pub fn stopSound(node: &mut WorldNode) {
		if let Some(sound) = node.getComponentMut::<TimelineSound>() {
			sound.stop();
		}
	}

	//==============================================================================
	// 재생. (반복이 아닌데 끝에 있으면 처음부터)
	//==============================================================================
	pub fn play(&mut self) {
		if self.time >= self.getDuration() && !self.isLoop() {
			self.time = 0.0;
		}
		self.isPlaying = true;
	}

	//==============================================================================
	// 일시 정지.
	//==============================================================================
	pub fn pause(&mut self) {
		self.isPlaying = false;
	}

	//==============================================================================
	// 정지. (소리와 파티클을 멈추고 처음으로)
	//==============================================================================
	pub fn stop(&mut self, nodeTree: &mut NodeTree) {
		self.isPlaying = false;
		for (_, target) in self.targetTable.iter() {
			if let TimelineTarget::node(nodeId) = target {
				if let Some(node) = nodeTree.getNodeMut(*nodeId) {
					Timeline::stopSound(node);
				}
			}
		}
		for nodeId in self.particleNodeIds.iter() {
			if let Some(particleSystem) = nodeTree.getNodeMut(*nodeId).and_then(|node| node.getComponentMut::<ParticleSystem>()) {
				particleSystem.stop(true);
			}
		}
		self.seek(nodeTree, 0.0);
	}

	//==============================================================================
	// 시간 이동. (이벤트는 발생하지 않는다. 파티클은 그 시간까지 다시 진행한다)
	//==============================================================================
	pub fn seek(&mut self, nodeTree: &mut NodeTree, time: f32) {
		self.time = time.min(self.getDuration()).max(0.0);
		let currentTime = self.time;
		self.evaluate(nodeTree, currentTime);
		self.resimulateParticles(nodeTree, currentTime);
	}

	//==============================================================================
	// 갱신. (시간을 흘리고 값을 적용, 지나친 이벤트 / 마커를 발생)
	//==============================================================================
	pub fn tick(&mut self, nodeTree: &mut NodeTree, timeDelta: f32) {
		if !self.isPlaying {
			return;
		}
		let duration = self.getDuration();
		let previousTime = self.time;
		let mut nextTime = previousTime + timeDelta * self.speed;
		let mut isWrapped = false;
		if nextTime >= duration {
			if self.isLoop() && duration > 0.0 {
				self.fireEventsBetween(nodeTree, previousTime, duration);
				self.simulateParticles(nodeTree, duration - previousTime);
				nextTime %= duration;
				self.fireEventsBetween(nodeTree, -1.0, nextTime);
				isWrapped = true;
			}
			else {
				self.fireEventsBetween(nodeTree, previousTime, duration);
				self.simulateParticles(nodeTree, duration - previousTime);
				self.time = duration;
				self.isPlaying = false;
				self.evaluate(nodeTree, duration);
				if let Some(completeHandler) = self.completeHandler.as_mut() {
					completeHandler(nodeTree);
				}
				return;
			}
		}
		else if nextTime < 0.0 {
			nextTime = 0.0;
		}
		else {
			self.fireEventsBetween(nodeTree, previousTime, nextTime);
		}
		self.time = nextTime;
		self.evaluate(nodeTree, nextTime);
		// 파티클은 타임라인 시간으로만 진행한다. (감기면 처음부터 다시, 매 반복이 같게)
		if isWrapped {
			self.resimulateParticles(nodeTree, nextTime);
		}
		else {
			self.simulateParticles(nodeTree, nextTime - previousTime);
		}
	}

	//==============================================================================
	// 구간 안의 이벤트 키 / 마커 발생. (fromTime 초과 ~ toTime 이하. 이벤트 트랙 차례, 그다음 마커)
	//==============================================================================
	pub fn fireEventsBetween(&mut self, nodeTree: &mut NodeTree, fromTime: f32, toTime: f32) {
		self.compileIfDirty(nodeTree);
		let isInside = |time: f32| time > fromTime && time <= toTime;
		let mut eventList: Vec<(usize, usize)> = Vec::new();
		for (trackIndex, compiledTrack) in self.compiledTracks.iter().enumerate() {
			if compiledTrack.propertyName != "event" || !compiledTrack.isEnabled {
				continue;
			}
			for (keyIndex, key) in compiledTrack.keys.iter().enumerate() {
				if isInside(key.time) {
					eventList.push((trackIndex, keyIndex));
				}
			}
		}
		for (trackIndex, keyIndex) in eventList {
			self.dispatchEvent(nodeTree, trackIndex, keyIndex, false);
		}
		if let Some(markerHandler) = self.markerHandler.as_mut() {
			for marker in self.compiledMarkers.iter() {
				if isInside(marker.time) {
					markerHandler(&marker.name, marker, nodeTree);
				}
			}
		}
	}

	//==============================================================================
	// 이벤트 키 처리. (파티클 대상은 play / stop / clear / emit:N 을 스스로 처리하고, 사운드 대상은 play / loop / stop, 그 뒤 핸들러)
	// - isSilent 는 재시뮬레이션용이다. (사운드 / 사용자 핸들러는 부르지 않고 파티클만)
	//==============================================================================
	fn dispatchEvent(&mut self, nodeTree: &mut NodeTree, trackIndex: usize, keyIndex: usize, isSilent: bool) {
		let (target, targetName, key) = {
			let compiledTrack = &self.compiledTracks[trackIndex];
			(compiledTrack.target, compiledTrack.targetName.clone(), compiledTrack.keys[keyIndex].clone())
		};
		let eventName = key.value.toText();
		if let Some(TimelineTarget::node(nodeId)) = target {
			if let Some(node) = nodeTree.getNodeMut(nodeId) {
				if !isSilent {
					if let Some(sound) = node.getComponentMut::<TimelineSound>() {
						sound.handleEvent(&eventName);
					}
				}
				let emitterCenter = node.getContentSize().multiplyScalar(0.5);
				if let Some(particleSystem) = node.getComponentMut::<ParticleSystem>() {
					if eventName == "play" {
						particleSystem.play();
					}
					else if eventName == "stop" {
						particleSystem.stop(false);
					}
					else if eventName == "clear" {
						particleSystem.stop(true);
					}
					else if eventName.starts_with("emit") {
						let emitCount = match eventName.find(':') {
							Some(separatorIndex) => {
								parseNumberText(&eventName[separatorIndex + 1..])
							}
							None => {
								DEFAULT_EMIT_COUNT
							}
						};
						let emitCount = if emitCount.is_finite() {
							emitCount
						}
						else {
							DEFAULT_EMIT_COUNT
						};
						particleSystem.emit(emitCount.max(0.0).ceil() as usize, emitterCenter);
					}
				}
			}
		}
		if !isSilent {
			if let Some(eventHandler) = self.eventHandler.as_mut() {
				eventHandler(&targetName, &eventName, &key, nodeTree);
			}
		}
	}

	//==============================================================================
	// 시간의 값 적용. (연속 속성만, 이벤트는 tick 에서만 발생. external 대상 값은 모아 둔다)
	//==============================================================================
	pub fn evaluate(&mut self, nodeTree: &mut NodeTree, time: f32) {
		self.compileIfDirty(nodeTree);
		self.externalPropertyList.clear();
		for compiledTrack in self.compiledTracks.iter() {
			if !compiledTrack.isEnabled || compiledTrack.keys.is_empty() || compiledTrack.propertyName == "event" {
				continue;
			}
			let target = match compiledTrack.target {
				Some(target) => {
					target
				}
				None => {
					continue;
				}
			};
			let value = Timeline::sampleKeys(&compiledTrack.keys, compiledTrack.kind, time);
			match target {
				TimelineTarget::node(nodeId) => {
					let apply = compiledTrack.definition.and_then(|definition| definition.apply);
					if let (Some(apply), Some(node)) = (apply, nodeTree.getNodeMut(nodeId)) {
						apply(node, &value);
					}
				}
				TimelineTarget::external => {
					self.externalPropertyList.push(TimelineExternalProperty {
						targetName: compiledTrack.targetName.clone(),
						propertyName: compiledTrack.propertyName.clone(),
						value,
					});
				}
			}
		}
	}

	//==============================================================================
	// 모아 둔 external 값 넘기기. (그 이름의 대상 값을 setTimelineProperty 로 넘기고 목록에서 뺀다. 원본 evaluate 의 setTimelineProperty 자리)
	//==============================================================================
	pub fn applyExternalProperties(&mut self, targetName: &str, target: &mut dyn TimelinePropertyTarget) {
		self.externalPropertyList.retain(|externalProperty| {
			if externalProperty.targetName != targetName {
				return true;
			}
			target.setTimelineProperty(&externalProperty.propertyName, &externalProperty.value);
			false
		});
	}

	//==============================================================================
	// 모아 둔 external 값 반환. (넘기지 않은 것)
	//==============================================================================
	pub fn getExternalProperties(&self) -> &[TimelineExternalProperty] {
		&self.externalPropertyList
	}

	//==============================================================================
	// 키 목록에서 시간의 값 표본. (정적. 키는 시간순이어야 한다. 편집기의 커브 그리기도 쓴다)
	//==============================================================================
	pub fn sampleKeys(sortedKeys: &[TimelineKey], kind: TimelinePropertyKind, time: f32) -> TimelineValue {
		let keyCount = sortedKeys.len();
		if keyCount == 0 {
			if kind == TimelinePropertyKind::number {
				return TimelineValue::number(0.0);
			}
			return TimelineValue::none;
		}
		if time <= sortedKeys[0].time {
			return sortedKeys[0].value.clone();
		}
		if time >= sortedKeys[keyCount - 1].time {
			return sortedKeys[keyCount - 1].value.clone();
		}
		let mut segmentIndex = 0;
		while segmentIndex + 2 < keyCount && sortedKeys[segmentIndex + 1].time <= time {
			segmentIndex += 1;
		}
		let fromKey = &sortedKeys[segmentIndex];
		let toKey = &sortedKeys[segmentIndex + 1];
		let segmentDuration = toKey.time - fromKey.time;
		if segmentDuration <= 0.0 {
			return toKey.value.clone();
		}
		if kind != TimelinePropertyKind::number && kind != TimelinePropertyKind::color {
			return fromKey.value.clone();
		}
		let easing = fromKey.resolveEasing();
		let progress = (time - fromKey.time) / segmentDuration;
		let easedProgress = easing.evaluate(progress.clamp(0.0, 1.0));
		interpolateValue(kind, &fromKey.value, &toKey.value, easedProgress)
	}

	//==============================================================================
	// 서술 트랙의 시간 값 표본. (정적. 서술 트랙 그대로, 정렬은 여기서. 정의가 없는 속성은 number)
	//==============================================================================
	pub fn sampleTrack(track: &Value, time: f32) -> TimelineValue {
		let propertyName = track.get("property").and_then(|value| value.as_str()).unwrap_or("");
		let kind = match findTimelinePropertyDefinition(propertyName) {
			Some(definition) => {
				definition.kind
			}
			None => {
				TimelinePropertyKind::number
			}
		};
		let mut sortedKeys = readTrackKeys(track);
		sortKeysByTime(&mut sortedKeys);
		Timeline::sampleKeys(&sortedKeys, kind, time)
	}

	//==============================================================================
	// 시간 반환.
	//==============================================================================
	pub fn getTime(&self) -> f32 {
		self.time
	}

	//==============================================================================
	// 재생 중 여부 반환.
	//==============================================================================
	pub fn isPlaying(&self) -> bool {
		self.isPlaying
	}

	//==============================================================================
	// 속도 설정. (1 = 실시간)
	//==============================================================================
	pub fn setSpeed(&mut self, speed: f32) {
		self.speed = speed;
	}

	//==============================================================================
	// 속도 반환.
	//==============================================================================
	pub fn getSpeed(&self) -> f32 {
		self.speed
	}

	//==============================================================================
	// 반복 설정. (서술의 loop 를 바꾼다)
	//==============================================================================
	pub fn setLoop(&mut self, isLoop: bool) {
		if let Some(descriptionObject) = self.description.as_object_mut() {
			descriptionObject.insert(String::from("loop"), Value::Bool(isLoop));
		}
	}

	//==============================================================================
	// 반복 여부 반환. (서술의 loop 가 true 일 때만)
	//==============================================================================
	pub fn isLoop(&self) -> bool {
		self.description.get("loop") == Some(&Value::Bool(true))
	}

	//==============================================================================
	// 길이 반환. (초, 못 읽거나 0 이하면 0)
	//==============================================================================
	pub fn getDuration(&self) -> f32 {
		let duration = readJsonNumber(self.description.get("duration"));
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
	pub fn getFrameRate(&self) -> f32 {
		let frameRate = readJsonNumber(self.description.get("frameRate"));
		if frameRate.is_finite() && frameRate > 0.0 {
			frameRate
		}
		else {
			30.0
		}
	}

	//==============================================================================
	// 이벤트 핸들러 설정. ((대상 이름, 이벤트 이름, 키, 노드 트리))
	//==============================================================================
	pub fn setEventHandler(&mut self, eventHandler: Option<TimelineEventHandler>) {
		self.eventHandler = eventHandler;
	}

	//==============================================================================
	// 마커 핸들러 설정. ((마커 이름, 마커, 노드 트리))
	//==============================================================================
	pub fn setMarkerHandler(&mut self, markerHandler: Option<TimelineMarkerHandler>) {
		self.markerHandler = markerHandler;
	}

	//==============================================================================
	// 완료 핸들러 설정. (반복이 아닐 때 끝에 닿으면)
	//==============================================================================
	pub fn setCompleteHandler(&mut self, completeHandler: Option<TimelineCompleteHandler>) {
		self.completeHandler = completeHandler;
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
	fn assertNear(actual: f32, expected: f64, tolerance: f64, label: &str) {
		assert!(((actual as f64) - expected).abs() <= tolerance, "{}: {} != {}", label, actual, expected);
	}

	//==============================================================================
	// 이징 곡선이 원본과 같은 값을 낸다. (원본 node 실행 값. exponential, elastic 은 원본 수식을 실제 Math 로 계산한 값)
	//==============================================================================
	#[test]
	fn easingCurvesMatchOriginal() {
		let progressList = [0.0f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
		let expectedTable: [(&str, [f64; 7]); 33] = [
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
		for (easingIndex, (easingName, expectedValues)) in expectedTable.iter().enumerate() {
			assert_eq!(TIMELINE_EASING_NAMES[easingIndex], *easingName);
			let easing = resolveTimelineEasing(easingName, None);
			for (progress, expectedValue) in progressList.iter().zip(expectedValues.iter()) {
				assertNear(easing.evaluate(*progress), *expectedValue, 0.0002, &format!("{} at {}", easingName, progress));
			}
		}
		assertNear(resolveTimelineEasing("unknown.out", None).evaluate(0.3), 0.3, 0.000001, "unknown name is linear");
		assertNear(resolveTimelineEasing("cubic", None).evaluate(0.3), 0.3, 0.000001, "family without direction is linear");
	}

	//==============================================================================
	// 베지어 이징이 원본과 같은 값을 낸다. (제어점 x 가 0 ~ 1 밖이면 자른다)
	//==============================================================================
	#[test]
	fn bezierEasingMatchesOriginal() {
		let progressList = [0.0f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
		let expectedTable: [([f32; 4], [f64; 7]); 4] = [
			([0.25, 0.1, 0.25, 1.0], [0.0, 0.09479499730150565, 0.4085105930156005, 0.8024033876954126, 0.9604590729525888, 0.994316477509521, 1.0]),
			([0.68, -0.55, 0.265, 1.55], [0.0, -0.06629147620026413, -0.0828074555627528, 0.6066814989627444, 1.089166506168796, 1.0623731930901974, 1.0]),
			([0.42, 0.0, 0.58, 1.0], [0.0, 0.019722447263855636, 0.1291619005687877, 0.5, 0.8708380994312122, 0.9802775527361445, 1.0]),
			([1.5, 0.0, -0.5, 1.0], [0.0, 0.0037616480437726607, 0.029723406016020297, 0.5, 0.9702765939839797, 0.9962383519562273, 1.0]),
		];
		for (curve, expectedValues) in expectedTable.iter() {
			let easing = createBezierEasing(curve[0], curve[1], curve[2], curve[3]);
			let resolvedEasing = resolveTimelineEasing("bezier", Some(curve));
			for (progress, expectedValue) in progressList.iter().zip(expectedValues.iter()) {
				assertNear(easing(*progress), *expectedValue, 0.0002, &format!("{:?} at {}", curve, progress));
				assertNear(resolvedEasing.evaluate(*progress), *expectedValue, 0.0002, &format!("resolved {:?} at {}", curve, progress));
			}
		}
	}

	//==============================================================================
	// 색 글 읽기가 원본 parseInt 와 같게 읽는다. (못 읽는 채널은 1)
	//==============================================================================
	#[test]
	fn parseTimelineColorMatchesOriginal() {
		let expectedTable: [(&str, [f64; 4]); 9] = [
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
		for (hexText, expectedChannels) in expectedTable.iter() {
			let channels = parseTimelineColor(hexText);
			for (channelIndex, (channel, expectedChannel)) in channels.iter().zip(expectedChannels.iter()).enumerate() {
				assertNear(*channel, *expectedChannel, 0.000001, &format!("{} channel {}", hexText, channelIndex));
			}
		}
	}

	//==============================================================================
	// 채널 → 색 글이 원본과 같다. (0 ~ 1 로 자르고 반올림, 알파가 없으면 ff)
	//==============================================================================
	#[test]
	fn composeTimelineColorMatchesOriginal() {
		assert_eq!(composeTimelineColor(&[1.0, 0.5, 0.0, 1.0]), "#ff8000ff");
		assert_eq!(composeTimelineColor(&[0.2, 0.4, 0.6]), "#336699ff");
		assert_eq!(composeTimelineColor(&[1.2, -0.1, 0.5, 0.5]), "#ff008080");
		assert_eq!(composeTimelineColor(&[0.5, 0.25, 0.75, 0.1]), "#8040bf1a");
	}

	//==============================================================================
	// 트랙 표본이 원본과 같다. (정렬, 구간 이징, 베지어, 계단, 색 보간, 문자열 / 참거짓 계단, 같은 시간 키)
	//==============================================================================
	#[test]
	fn sampleTrackMatchesOriginal() {
		let timeList = [-1.0f32, 0.0, 0.3, 0.5, 0.99, 1.0, 1.4, 2.0, 2.5, 5.0];
		let numberTracks: [(Value, [f64; 10]); 3] = [
			(json!({ "property": "x", "keys": [{ "time": 1, "value": 50, "easing": "cubic.out" }, { "time": 0, "value": 0, "easing": "quadratic.in" }, { "time": 2, "value": 100 }] }),
				[0.0, 0.0, 4.5, 12.5, 49.004999999999995, 50.0, 89.19999999999999, 100.0, 100.0, 100.0]),
			(json!({ "property": "y", "keys": [{ "time": 0, "value": 10, "easing": "bezier", "curve": [0.68, -0.55, 0.265, 1.55] }, { "time": 1, "value": 20, "easing": "step" }, { "time": 2, "value": 40 }] }),
				[10.0, 10.0, 9.50682322875037, 16.066814989627446, 20.073713280149843, 20.0, 20.0, 40.0, 40.0, 40.0]),
			// 0.99 의 elastic.out 은 원본 수식을 실제 Math 로 계산한 값. (원본 엔진 pow 는 1.0038581575804497)
			(json!({ "property": "opacity", "keys": [{ "time": 0, "value": 0, "easing": "elastic.out" }, { "time": 1, "value": 1, "easing": "bounce.inOut" }, { "time": 1, "value": 0.5 }, { "time": 3, "value": 0 }] }),
				[0.0, 0.0, 1.0, 1.0, 1.0010337677272523, 0.5, 0.4, 0.25, 0.125, 0.0]),
		];
		for (track, expectedValues) in numberTracks.iter() {
			for (time, expectedValue) in timeList.iter().zip(expectedValues.iter()) {
				let value = Timeline::sampleTrack(track, *time);
				assertNear(value.asNumber(), *expectedValue, 0.0005, &format!("{} at {}", track["property"], time));
			}
		}
		let colorTrack = json!({ "property": "color", "keys": [{ "time": 0, "value": "#ff000080", "easing": "sinusoidal.inOut" }, { "time": 2, "value": "#0000ffff" }] });
		// 1 초의 빨강은 원본 엔진 sin(급수 근사)이 0.5000000000000017 을 내서 7f 로 내림된다. 실제 sin 은 0.5 라 80 이다.
		let expectedColors = ["#ff000080", "#ff000080", "#f1000e87", "#da002593", "#82007dbf", "#800080c0", "#3500cae5", "#0000ffff", "#0000ffff", "#0000ffff"];
		for (time, expectedColor) in timeList.iter().zip(expectedColors.iter()) {
			assert_eq!(Timeline::sampleTrack(&colorTrack, *time), TimelineValue::string(expectedColor.to_string()), "color at {}", time);
		}
		let textTrack = json!({ "property": "text", "keys": [{ "time": 0, "value": "A" }, { "time": 1, "value": "B" }] });
		let expectedTexts = ["A", "A", "A", "A", "A", "B", "B", "B", "B", "B"];
		for (time, expectedText) in timeList.iter().zip(expectedTexts.iter()) {
			assert_eq!(Timeline::sampleTrack(&textTrack, *time).toText(), *expectedText, "text at {}", time);
		}
		let visibleTrack = json!({ "property": "visible", "keys": [{ "time": 0, "value": false }, { "time": 0.5, "value": true }] });
		let expectedVisibles = [false, false, false, true, true, true, true, true, true, true];
		for (time, expectedVisible) in timeList.iter().zip(expectedVisibles.iter()) {
			assert_eq!(Timeline::sampleTrack(&visibleTrack, *time), TimelineValue::boolean(*expectedVisible), "visible at {}", time);
		}
		assert_eq!(Timeline::sampleKeys(&[], TimelinePropertyKind::number, 1.0), TimelineValue::number(0.0));
		assert_eq!(Timeline::sampleKeys(&[], TimelinePropertyKind::string, 1.0), TimelineValue::none);
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
		fn setTimelineProperty(&mut self, propertyName: &str, value: &TimelineValue) {
			self.log.borrow_mut().push(format!("set {}={:.4}", propertyName, value.asNumber()));
		}
	}

	//==============================================================================
	// 재생이 원본과 같은 차례로 값 / 이벤트 / 마커 / 완료를 낸다. (원본 node 실행 기록과 비교)
	//==============================================================================
	#[test]
	fn playbackMatchesOriginalEventOrder() {
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
		let mut nodeTree = NodeTree::new();
		let mut timeline = Timeline::new(Some(description));
		timeline.setTargetResolver(Some(Box::new(|targetName: &str| {
			if targetName == "screen" {
				Some(TimelineTarget::external)
			}
			else {
				None
			}
		})));
		let eventLog = log.clone();
		timeline.setEventHandler(Some(Box::new(move |targetName: &str, eventName: &str, _key: &TimelineKey, _nodeTree: &mut NodeTree| {
			eventLog.borrow_mut().push(format!("event {}:{}", targetName, eventName));
		})));
		let markerLog = log.clone();
		timeline.setMarkerHandler(Some(Box::new(move |markerName: &str, _marker: &TimelineMarker, _nodeTree: &mut NodeTree| {
			markerLog.borrow_mut().push(format!("marker {}", markerName));
		})));
		let completeLog = log.clone();
		timeline.setCompleteHandler(Some(Box::new(move |_nodeTree: &mut NodeTree| {
			completeLog.borrow_mut().push(String::from("complete"));
		})));
		let mut screen = RecordingTarget { log: log.clone() };
		//==============================================================================
		// 한 번 갱신하고 모인 external 값을 넘긴다.
		//==============================================================================
		fn tickAndRecord(timeline: &mut Timeline, nodeTree: &mut NodeTree, screen: &mut RecordingTarget, timeDelta: f32) {
			timeline.tick(nodeTree, timeDelta);
			timeline.applyExternalProperties("screen", screen);
		}

		timeline.play();
		for timeDelta in [0.25f32, 0.25, 0.6, 0.6, 0.6, 0.6] {
			tickAndRecord(&mut timeline, &mut nodeTree, &mut screen, timeDelta);
			log.borrow_mut().push(format!("time {:.4}", timeline.getTime()));
		}
		timeline.setLoop(false);
		tickAndRecord(&mut timeline, &mut nodeTree, &mut screen, 0.7);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.getTime(), timeline.isPlaying()));
		tickAndRecord(&mut timeline, &mut nodeTree, &mut screen, 0.3);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.getTime(), timeline.isPlaying()));
		timeline.play();
		log.borrow_mut().push(format!("replay time {:.4}", timeline.getTime()));
		timeline.seek(&mut nodeTree, 1.25);
		timeline.applyExternalProperties("screen", &mut screen);
		log.borrow_mut().push(format!("seek time {:.4}", timeline.getTime()));
		tickAndRecord(&mut timeline, &mut nodeTree, &mut screen, 1.0);
		log.borrow_mut().push(format!("time {:.4} playing {}", timeline.getTime(), timeline.isPlaying()));
		timeline.play();
		log.borrow_mut().push(format!("replay time {:.4} playing {}", timeline.getTime(), timeline.isPlaying()));
		tickAndRecord(&mut timeline, &mut nodeTree, &mut screen, 0.1);
		log.borrow_mut().push(format!("time {:.4}", timeline.getTime()));

		// 원본 기록. (숫자는 소수 넷째 자리로 맞췄다)
		let expectedLog = [
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
		assert_eq!(*log.borrow(), expectedLog.iter().map(|line| line.to_string()).collect::<Vec<String>>());
	}

	//==============================================================================
	// 무대를 만들고 트랙 값을 노드에 건다. (부모 관계, Paint / Text 색, 글, 보이기, 위치)
	//==============================================================================
	#[test]
	fn buildStageCreatesNodesAndAppliesTracks() {
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
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let mut timeline = Timeline::new(Some(description));
		let nodeTable = timeline.buildStage(&mut nodeTree, rootId, None);
		let groupId = nodeTable["Group"];
		let boxId = nodeTable["Box"];
		let labelId = nodeTable["Label"];
		let scoreId = nodeTable["Score"];
		assert_eq!(nodeTree.getParentId(boxId), Some(groupId));
		assert_eq!(nodeTree.getParentId(labelId), Some(rootId));
		assert_eq!(nodeTree[groupId].getLocalPosition(), Vector2::create(100.0, 50.0));
		assert_eq!(nodeTree[boxId].getContentSize(), Vector2::create(40.0, 20.0));
		assert_eq!(nodeTree[boxId].getPivot(), Vector2::create(0.5, 0.5));
		assert_eq!(nodeTree[boxId].getComponent::<Paint>().map(|paint| paint.getRoundSize()), Some(4.0));
		assert!(!nodeTree[labelId].isActive());
		let label = nodeTree[labelId].getComponent::<Text>().expect("label text");
		assert_eq!(label.getText(), "Hello");
		assert!(label.isBold());
		assert_eq!(label.getTextAlign(), TextAlign::left);
		assert_eq!(nodeTree[scoreId].getComponent::<Text>().map(|text| text.getText().to_string()), Some(String::from("Text")));

		timeline.seek(&mut nodeTree, 1.0);
		assertNear(nodeTree[boxId].getLocalPosition().x, 100.0, 0.0001, "box x at 1");
		let boxColor = nodeTree[boxId].getComponent::<Paint>().expect("box paint").getColor();
		assertNear(boxColor.red, 128.0 / 255.0, 0.0001, "box red at 1");
		assertNear(boxColor.blue, 128.0 / 255.0, 0.0001, "box blue at 1");
		assert!(nodeTree[labelId].isActive());
		assert_eq!(nodeTree[labelId].getComponent::<Text>().map(|text| text.getVisibleCharacterCount()), Some(3));
		assert_eq!(nodeTree[scoreId].getComponent::<Text>().map(|text| text.getText().to_string()), Some(String::from("500")));

		timeline.play();
		timeline.tick(&mut nodeTree, 5.0);
		assert!(!timeline.isPlaying());
		assertNear(nodeTree[boxId].getLocalPosition().x, 200.0, 0.0001, "box x at end");
		assert_eq!(nodeTree[scoreId].getComponent::<Text>().map(|text| text.getText().to_string()), Some(String::from("1000")));
	}

	//==============================================================================
	// bind 는 이미 있는 노드 트리에서 트랙 대상 이름을 찾아 붙인다.
	//==============================================================================
	#[test]
	fn bindFindsTargetsByName() {
		let description = json!({
			"duration": 1,
			"tracks": [
				{ "target": "Hero", "property": "scaleX", "keys": [{ "time": 0, "value": 1 }, { "time": 1, "value": 3 }] },
				{ "target": "Hero", "property": "rotation", "keys": [{ "time": 0, "value": 0 }, { "time": 1, "value": 2 }] },
			],
		});
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let worldId = nodeTree.createChild(rootId, "World");
		let heroId = nodeTree.createChild(worldId, "Hero");
		let mut timeline = Timeline::new(Some(description));
		timeline.bind(&nodeTree, rootId);
		assert_eq!(timeline.getTarget("Hero"), Some(TimelineTarget::node(heroId)));
		timeline.seek(&mut nodeTree, 0.5);
		assertNear(nodeTree[heroId].getLocalScale().x, 2.0, 0.0001, "hero scale");
		assertNear(nodeTree[heroId].getLocalRotation(), 1.0, 0.0001, "hero rotation");
	}

	//==============================================================================
	// 파티클은 타임라인 시간으로만 진행하고, 이벤트 play 로 버스트가 나며, 되감아도 같은 결과가 나온다.
	//==============================================================================
	#[test]
	fn particleEventsFollowTimelineTime() {
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
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let mut timeline = Timeline::new(Some(description));
		let nodeTable = timeline.buildStage(&mut nodeTree, rootId, None);
		let burstId = nodeTable["Burst"];
		let particleCount = |nodeTree: &NodeTree| nodeTree[burstId].getComponent::<ParticleSystem>().map(|particleSystem| particleSystem.getParticleCount()).unwrap_or(0);

		timeline.seek(&mut nodeTree, 0.0);
		assert!(nodeTree[burstId].getComponent::<ParticleSystem>().is_some_and(|particleSystem| particleSystem.isManualTick()));
		assert_eq!(particleCount(&nodeTree), 0);
		timeline.play();
		timeline.tick(&mut nodeTree, 0.4);
		assert_eq!(particleCount(&nodeTree), 0);
		timeline.tick(&mut nodeTree, 0.2);
		assert_eq!(particleCount(&nodeTree), 25);

		timeline.seek(&mut nodeTree, 0.2);
		assert_eq!(particleCount(&nodeTree), 0);
		timeline.seek(&mut nodeTree, 0.6);
		assert_eq!(particleCount(&nodeTree), 25);
		timeline.seek(&mut nodeTree, 1.9);
		assert_eq!(particleCount(&nodeTree), 0);
	}

	//==============================================================================
	// frame 속성은 스프라이트 시트 격자에서 칸을 고른다. (넘치면 감는다)
	//==============================================================================
	#[test]
	fn frameTrackSelectsSpriteSheetCell() {
		let sheetImage = Image::createFromPixels(64, 32, vec![255u8; 64 * 32 * 4]);
		let description = json!({
			"duration": 1,
			"stage": { "nodes": [
				{ "name": "Coin", "type": "sprite", "image": "coin.png", "frameColumns": 4, "frameRows": 2, "frame": 1, "effect": "glow", "blendMode": "lighter" },
			] },
			"tracks": [
				{ "target": "Coin", "property": "frame", "keys": [{ "time": 0, "value": 0 }, { "time": 1, "value": 10 }] },
			],
		});
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let mut timeline = Timeline::new(Some(description));
		let imageResolver = |imageName: &str| -> Option<Rc<Image>> {
			if imageName == "coin.png" {
				Some(sheetImage.clone())
			}
			else {
				None
			}
		};
		let nodeTable = timeline.buildStage(&mut nodeTree, rootId, Some(&imageResolver));
		let coinId = nodeTable["Coin"];
		let imageRectOf = |nodeTree: &NodeTree| nodeTree[coinId].getComponent::<ShaderSprite>().expect("coin sprite").getSprite().getImageRect();
		assert_eq!(imageRectOf(&nodeTree), Rect::create(16.0, 0.0, 16.0, 16.0));
		let coinSprite = nodeTree[coinId].getComponent::<ShaderSprite>().expect("coin sprite");
		assert_eq!(coinSprite.getEffect(), ShaderSpriteEffect::fromName("glow").expect("glow effect"));
		assert_eq!(coinSprite.getSprite().getSpriteBlendMode(), BlendMode::lighter);
		assertNear(coinSprite.getSprite().getColor().alpha, 0.0, 0.000001, "default sprite tint is transparent");

		timeline.seek(&mut nodeTree, 0.5);
		assert_eq!(imageRectOf(&nodeTree), Rect::create(16.0, 16.0, 16.0, 16.0));
		timeline.seek(&mut nodeTree, 1.0);
		assert_eq!(imageRectOf(&nodeTree), Rect::create(32.0, 0.0, 16.0, 16.0));
	}

	//==============================================================================
	// 서술을 채우고 길이 / 프레임 / 반복을 원본처럼 읽는다. (data 주소 base64 풀기 포함)
	//==============================================================================
	#[test]
	fn descriptionDefaultsAndDataUrl() {
		let timeline = Timeline::new(Some(json!({ "duration": "4", "frameRate": -1 })));
		assert_eq!(timeline.getDuration(), 4.0);
		assert_eq!(timeline.getFrameRate(), 30.0);
		assert!(!timeline.isLoop());
		assert!(timeline.getDescription().pointer("/stage/nodes").is_some_and(|nodes| nodes.is_array()));
		assert!(timeline.getDescription()["tracks"].is_array());
		let emptyTimeline = Timeline::default();
		assert_eq!(emptyTimeline.getDuration(), 3.0);
		assert!(emptyTimeline.isLoop());
		assert_eq!(TIMELINE_NODE_DEFAULTS.width, 100.0);
		assert_eq!(TIMELINE_NODE_DEFAULTS.nodeType, "group");
		assert_eq!(decodeBase64("SGVsbG8="), Some(b"Hello".to_vec()));
		assert_eq!(decodeBase64("SGk"), Some(b"Hi".to_vec()));
		assert_eq!(decodeBase64("S*"), None);
	}
}
